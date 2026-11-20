use crate::biff8_formula::{checked_rgce, formula_records};
use crate::cell_numbers::numeric_cell_value;
use crate::cell_type::{biff8_error_code, CellValueKind};
use crate::ole2_codec::{write_u32_le, Ole2ContainerCodec};
use crate::sheet_links::xls_links;
use crate::sheet_links::SheetLinks;
use crate::sheet_styling::{sheet_states, SheetStyling};
use crate::xls_boundsheet;
use crate::xls_styles::{blank_record, default_font_record, set_formula_xf, XlsColorPlan};
use crate::{EasyOfficeError, LegacyOle2Codec, XlsxFormulaCache};

/// Legacy `.xls` BIFF8 spreadsheet payload model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyXlsPayload {
    pub sheet_name: String,
    pub cells: Vec<(u32, u32, String)>,
}

/// How a single cell is encoded in a BIFF8 worksheet substream.
enum XlsCellRecord {
    Formula(Vec<u8>),
    Number(f64),
    BoolErr(u8, u8),
    LabelSst,
}

impl LegacyOle2Codec {
    pub const BIFF8_BOF: u16 = 0x0809;
    pub const BIFF8_EOF: u16 = 0x000A;
    pub const BIFF8_FILEPASS: u16 = 0x002F;
    pub const BIFF8_FONT: u16 = 0x0031;
    pub const BIFF8_CONTINUE: u16 = 0x003C;
    pub const BIFF8_BOUNDSHEET: u16 = 0x0085;
    pub const BIFF8_MULRK: u16 = 0x00BD;
    pub const BIFF8_XF: u16 = 0x00E0;
    pub const BIFF8_SST: u16 = 0x00FC;
    pub const BIFF8_LABELSST: u16 = 0x00FD;
    pub const BIFF8_DIMENSIONS: u16 = 0x0200;
    pub const BIFF8_NUMBER: u16 = 0x0203;
    pub const BIFF8_LABEL: u16 = 0x0204;
    pub const BIFF8_BOOLERR: u16 = 0x0205;
    pub const BIFF8_STRING: u16 = 0x0207;
    pub const BIFF8_RK: u16 = 0x027E;
    pub const BIFF8_FORMULA: u16 = 0x0006;

    #[inline]
    pub(crate) fn decode_latin1_bytes(slice: &[u8]) -> String {
        slice.iter().map(|&b| b as char).collect()
    }

    #[inline]
    pub(crate) fn format_biff_number(num: f64) -> String {
        if num == 0.0 {
            "0".to_string()
        } else if num.fract() == 0.0 && num.abs() < 1e15 {
            (num as i64).to_string()
        } else {
            format!("{num}")
        }
    }

    #[inline]
    pub(crate) fn decode_rk_number(rk: u32) -> f64 {
        let base = if (rk & 0x02) != 0 {
            ((rk as i32) >> 2) as f64
        } else {
            f64::from_bits(((rk & !3) as u64) << 32)
        };
        if (rk & 0x01) != 0 {
            base / 100.0
        } else {
            base
        }
    }

    fn emit_sst_with_continue(stream: &mut Vec<u8>, sst_strings: &[String]) {
        let max_rec = crate::MAX_BIFF8_RECORD_DATA_LEN;
        let mut current_opcode = Self::BIFF8_SST;
        let mut current_buf = Vec::with_capacity(max_rec.min(1024));
        let total_count = sst_strings.len() as u32;
        current_buf.extend_from_slice(&total_count.to_le_bytes());
        current_buf.extend_from_slice(&total_count.to_le_bytes());

        let flush_record = |out: &mut Vec<u8>, opcode: u16, buf: &mut Vec<u8>| {
            out.extend_from_slice(&opcode.to_le_bytes());
            out.extend_from_slice(&(buf.len() as u16).to_le_bytes());
            out.extend_from_slice(buf);
            buf.clear();
        };

        for s in sst_strings {
            let units: Vec<u16> = s.encode_utf16().collect();
            let char_count = units.len() as u16;
            if current_buf.len() + 5 > max_rec {
                flush_record(stream, current_opcode, &mut current_buf);
                current_opcode = Self::BIFF8_CONTINUE;
            }
            current_buf.extend_from_slice(&char_count.to_le_bytes());
            current_buf.push(0x01); // UTF-16LE
            let mut unit_idx = 0usize;
            while unit_idx < units.len() {
                let avail_units = max_rec.saturating_sub(current_buf.len()) / 2;
                let mut take = avail_units.min(units.len() - unit_idx);
                // A record ends between characters: a surrogate pair does not straddle two records.
                if unit_idx + take < units.len()
                    && take > 0
                    && (0xD800..0xDC00).contains(&units[unit_idx + take - 1])
                {
                    take -= 1;
                }
                if take == 0 {
                    flush_record(stream, current_opcode, &mut current_buf);
                    current_opcode = Self::BIFF8_CONTINUE;
                    current_buf.push(0x01); // continuation grbit = UTF-16LE
                    continue;
                }
                for &u in &units[unit_idx..unit_idx + take] {
                    current_buf.extend_from_slice(&u.to_le_bytes());
                }
                unit_idx += take;
            }
        }

        if !current_buf.is_empty() {
            flush_record(stream, current_opcode, &mut current_buf);
        }
    }

    /// Sheet names a BIFF8 workbook accepts; see [`crate::xls_sheet_names::sanitize_sheet_names`].
    pub fn sanitize_sheet_names<S: AsRef<str>>(names: &[S]) -> Vec<String> {
        crate::xls_sheet_names::sanitize_sheet_names(names)
    }

    pub fn pack_xls(payload: &LegacyXlsPayload) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_xls_multi(std::slice::from_ref(payload))
    }

    /// Writes `=` cells as FORMULA records (token strings, PD-178). A formula outside the token set
    /// of `biff8_formula` is refused with its cell, never written as text in a formula record.
    pub fn pack_xls_multi(sheets: &[LegacyXlsPayload]) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_xls_multi_with_formula_cache(sheets, &[])
    }

    /// [`Self::pack_xls_multi`] with the computed result of each formula cell (one cache per
    /// sheet, keyed by zero-based `(row, col)`) as its cached value.
    pub fn pack_xls_multi_with_formula_cache(
        sheets: &[LegacyXlsPayload],
        caches: &[XlsxFormulaCache],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_xls_multi_inner(sheets, true, caches, &[], &[])
    }

    pub fn pack_xls_multi_values(sheets: &[LegacyXlsPayload]) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_xls_multi_inner(sheets, false, &[], &[], &[])
    }

    /// [`Self::pack_xls_multi_values`] with the hyperlinks of each sheet (PD-223): one `HLink` record per
    /// linked cell, and an `HLinkTooltip` for a link with a screen tip. A link outside the BIFF8 grid is
    /// refused.
    pub fn pack_xls_multi_values_linked(
        sheets: &[LegacyXlsPayload],
        links: &[SheetLinks],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_xls_multi_inner(sheets, false, &[], links, &[])
    }

    /// [`Self::pack_xls_multi_values_linked`] with the fill and font colours of the cells of each sheet
    /// (`stylings` has one entry per sheet, PD-227) and explicit cell value kinds (PD-241).
    pub fn pack_xls_multi_values_styled(
        sheets: &[LegacyXlsPayload],
        links: &[SheetLinks],
        stylings: &[SheetStyling],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        Self::pack_xls_multi_inner(sheets, false, &[], links, stylings)
    }

    /// What a save as XLS cannot keep of the colours of `stylings`: `(approximated, dropped)` is the number
    /// of colours that took the closest of the 56 palette colours and of coloured cells outside the 65,536
    /// rows by 256 columns of the format.
    pub fn xls_color_losses(stylings: &[SheetStyling]) -> (u32, u32) {
        XlsColorPlan::plan(stylings).map_or((0, 0), |plan| (plan.approximated, plan.dropped))
    }

    fn classify_xls_cell(
        row: u32,
        col: u32,
        raw: &str,
        allow_formulas: bool,
        styling: Option<&SheetStyling>,
    ) -> Result<XlsCellRecord, EasyOfficeError> {
        let kind = styling.map_or_else(
            || CellValueKind::infer(raw),
            |st| st.cell_kind(row, col, raw),
        );
        match kind {
            CellValueKind::Formula if allow_formulas => {
                if let Some(expr) = raw.strip_prefix('=') {
                    return Ok(XlsCellRecord::Formula(checked_rgce(row, col, expr)?));
                }
                Ok(XlsCellRecord::LabelSst)
            }
            CellValueKind::Number => {
                if let Some(num) = numeric_cell_value(raw)
                    .or_else(|| raw.trim().parse::<f64>().ok().filter(|n| n.is_finite()))
                {
                    return Ok(XlsCellRecord::Number(num));
                }
                Ok(XlsCellRecord::LabelSst)
            }
            CellValueKind::Boolean => {
                let truthy = !raw.trim().eq_ignore_ascii_case("FALSE") && raw.trim() != "0";
                Ok(XlsCellRecord::BoolErr(u8::from(truthy), 0))
            }
            CellValueKind::Error => match biff8_error_code(raw.trim()) {
                Some(code) => Ok(XlsCellRecord::BoolErr(code, 1)),
                None => Ok(XlsCellRecord::LabelSst),
            },
            CellValueKind::Text | CellValueKind::Formula => Ok(XlsCellRecord::LabelSst),
        }
    }

    fn pack_xls_multi_inner(
        sheets: &[LegacyXlsPayload],
        allow_formulas: bool,
        caches: &[XlsxFormulaCache],
        links: &[SheetLinks],
        stylings: &[SheetStyling],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        if sheets.is_empty() {
            return Err(EasyOfficeError::EmptyPayload);
        }
        let mut classified: Vec<Vec<XlsCellRecord>> = Vec::with_capacity(sheets.len());
        let mut sst_strings: Vec<String> = Vec::new();
        let mut sst_index_map: std::collections::HashMap<&str, u32> =
            std::collections::HashMap::new();
        for (sheet_idx, sheet) in sheets.iter().enumerate() {
            let styling = stylings.get(sheet_idx);
            let mut sheet_records = Vec::with_capacity(sheet.cells.len());
            for &(row, col, ref val) in &sheet.cells {
                if row > crate::MAX_BIFF8_ROW || col > crate::MAX_BIFF8_COL {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "BIFF8 셀 좌표(row={row}, col={col})가 명세 상한(row<={}, col<={})을 초과했습니다.",
                        crate::MAX_BIFF8_ROW,
                        crate::MAX_BIFF8_COL
                    )));
                }
                if val.encode_utf16().count() > 32_765 {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "BIFF8 셀 문자열 길이가 32,765 UTF-16 코드 유닛 한도를 초과했습니다."
                            .to_string(),
                    ));
                }
                let rec = Self::classify_xls_cell(row, col, val, allow_formulas, styling)?;
                if matches!(rec, XlsCellRecord::LabelSst)
                    && !sst_index_map.contains_key(val.as_str())
                {
                    let idx = sst_strings.len() as u32;
                    sst_index_map.insert(val.as_str(), idx);
                    sst_strings.push(val.clone());
                }
                sheet_records.push(rec);
            }
            classified.push(sheet_records);
        }

        let colors = XlsColorPlan::plan(stylings);
        let mut stream = Vec::with_capacity(512);

        // 1. Workbook Globals Substream BOF (0x0809, len 16, substream 0x0005)
        stream.extend_from_slice(&Self::BIFF8_BOF.to_le_bytes());
        stream.extend_from_slice(&16u16.to_le_bytes());
        stream.extend_from_slice(&0x0600u16.to_le_bytes()); // BIFF8
        stream.extend_from_slice(&0x0005u16.to_le_bytes()); // Workbook Globals
        stream.extend_from_slice(&0x0DBBu16.to_le_bytes());
        stream.extend_from_slice(&0x07CCu16.to_le_bytes());
        stream.extend_from_slice(&0u32.to_le_bytes());
        stream.extend_from_slice(&0x0006u32.to_le_bytes());

        // DATEMODE (MS-XLS 2.4.77): the dates count from 1904 when the first sheet's styling says so.
        if stylings.first().is_some_and(|st| st.date1904) {
            stream.extend_from_slice(&0x0022u16.to_le_bytes());
            stream.extend_from_slice(&2u16.to_le_bytes());
            stream.extend_from_slice(&1u16.to_le_bytes());
        }
        stream.extend(default_font_record());
        if let Some(plan) = &colors {
            plan.font_records(&mut stream);
        }
        stream.extend_from_slice(&Self::BIFF8_XF.to_le_bytes());
        stream.extend_from_slice(&20u16.to_le_bytes());
        stream.extend_from_slice(&[0u8; 20]);
        if let Some(plan) = &colors {
            plan.xf_and_palette_records(&mut stream);
        }

        // 2. BOUNDSHEET records (0x0085)
        let mut lb_ply_pos_offsets = Vec::with_capacity(sheets.len());
        let sheet_names = Self::sanitize_sheet_names(
            &sheets
                .iter()
                .map(|s| s.sheet_name.as_str())
                .collect::<Vec<_>>(),
        );
        let states = sheet_states(sheets.len(), stylings);
        for (name, &state) in sheet_names.iter().zip(&states) {
            lb_ply_pos_offsets.push(xls_boundsheet::push(&mut stream, name, state));
        }

        if !sst_strings.is_empty() {
            Self::emit_sst_with_continue(&mut stream, &sst_strings);
        }

        // End of Workbook Globals substream
        stream.extend_from_slice(&Self::BIFF8_EOF.to_le_bytes());
        stream.extend_from_slice(&0u16.to_le_bytes());

        // 3. Worksheet Substreams
        for (sheet_idx, sheet) in sheets.iter().enumerate() {
            let worksheet_bof_pos = stream.len() as u32;
            write_u32_le(
                &mut stream,
                lb_ply_pos_offsets[sheet_idx],
                worksheet_bof_pos,
            );

            stream.extend_from_slice(&Self::BIFF8_BOF.to_le_bytes());
            stream.extend_from_slice(&16u16.to_le_bytes());
            stream.extend_from_slice(&0x0600u16.to_le_bytes()); // BIFF8
            stream.extend_from_slice(&0x0010u16.to_le_bytes()); // Worksheet substream
            stream.extend_from_slice(&0x0DBBu16.to_le_bytes());
            stream.extend_from_slice(&0x07CCu16.to_le_bytes());
            stream.extend_from_slice(&0u32.to_le_bytes());
            stream.extend_from_slice(&0x0006u32.to_le_bytes());

            let (max_r, max_c) = sheet
                .cells
                .iter()
                .fold((0u32, 0u32), |(mr, mc), &(r, c, _)| {
                    (mr.max(r + 1), mc.max(c + 1))
                });
            stream.extend_from_slice(&Self::BIFF8_DIMENSIONS.to_le_bytes());
            stream.extend_from_slice(&14u16.to_le_bytes());
            stream.extend_from_slice(&0u32.to_le_bytes()); // first_row
            stream.extend_from_slice(&max_r.to_le_bytes()); // last_row + 1
            stream.extend_from_slice(&0u16.to_le_bytes()); // first_col
            stream.extend_from_slice(&(max_c.min(u16::MAX as u32) as u16).to_le_bytes()); // last_col + 1
            stream.extend_from_slice(&0u16.to_le_bytes()); // reserved

            let styling = stylings.get(sheet_idx);
            let xf_of =
                |row: u32, col: u32| colors.as_ref().map_or(0, |plan| plan.xf(styling, row, col));
            let valued: std::collections::HashSet<(u32, u32)> = if colors.is_some() {
                sheet.cells.iter().map(|&(r, c, _)| (r, c)).collect()
            } else {
                std::collections::HashSet::new()
            };
            let mut blanks = styling
                .filter(|_| colors.is_some())
                .map(XlsColorPlan::colored_cells)
                .unwrap_or_default()
                .into_iter()
                .filter(|at| !valued.contains(at))
                .peekable();
            for ((row, col, raw), rec) in sheet.cells.iter().zip(&classified[sheet_idx]) {
                while let Some((br, bc)) = blanks.next_if(|&at| at < (*row, *col)) {
                    blank_record(&mut stream, br as u16, bc as u16, xf_of(br, bc));
                }
                let r16 = *row as u16;
                let c16 = *col as u16;
                let xf = xf_of(*row, *col);
                match rec {
                    XlsCellRecord::Formula(rgce) => {
                        let cached = caches.get(sheet_idx).and_then(|c| c.get(&(*row, *col)));
                        let mut record = formula_records(r16, c16, rgce, cached);
                        set_formula_xf(&mut record, xf);
                        stream.extend_from_slice(&record);
                    }
                    XlsCellRecord::Number(num) => {
                        stream.extend_from_slice(&Self::BIFF8_NUMBER.to_le_bytes());
                        stream.extend_from_slice(&14u16.to_le_bytes());
                        stream.extend_from_slice(&r16.to_le_bytes());
                        stream.extend_from_slice(&c16.to_le_bytes());
                        stream.extend_from_slice(&xf.to_le_bytes());
                        stream.extend_from_slice(&num.to_bits().to_le_bytes());
                    }
                    XlsCellRecord::BoolErr(val_byte, err_flag) => {
                        stream.extend_from_slice(&Self::BIFF8_BOOLERR.to_le_bytes());
                        stream.extend_from_slice(&8u16.to_le_bytes());
                        stream.extend_from_slice(&r16.to_le_bytes());
                        stream.extend_from_slice(&c16.to_le_bytes());
                        stream.extend_from_slice(&xf.to_le_bytes());
                        stream.push(*val_byte);
                        stream.push(*err_flag);
                    }
                    XlsCellRecord::LabelSst => {
                        let sst_idx = sst_index_map.get(raw.as_str()).copied().unwrap_or(0);
                        stream.extend_from_slice(&Self::BIFF8_LABELSST.to_le_bytes());
                        stream.extend_from_slice(&10u16.to_le_bytes());
                        stream.extend_from_slice(&r16.to_le_bytes());
                        stream.extend_from_slice(&c16.to_le_bytes());
                        stream.extend_from_slice(&xf.to_le_bytes());
                        stream.extend_from_slice(&sst_idx.to_le_bytes());
                    }
                }
            }
            for (br, bc) in blanks {
                blank_record(&mut stream, br as u16, bc as u16, xf_of(br, bc));
            }

            if let Some(sheet_links) = links.get(sheet_idx) {
                xls_links::write_links(&mut stream, sheet_links)?;
            }
            stream.extend_from_slice(&Self::BIFF8_EOF.to_le_bytes());
            stream.extend_from_slice(&0u16.to_le_bytes());
        }

        Ole2ContainerCodec::pack(&[("Workbook".to_string(), stream)])
    }
}
