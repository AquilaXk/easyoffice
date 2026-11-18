//! 참조 구현: iwork-rs (MIT, Copyright (c) 2026 Zebra Pig; see THIRD_PARTY_NOTICES.md), src/chart.rs
//!
//! Pages chart import (`TSCH.ChartDrawableArchive` 5021 with extension 10000 = `TSCH.ChartArchive`,
//! and legacy `5000` / `5002`): 28 chart types, private `TSCH.ChartGridArchive` with preserved
//! zero-byte blank cells, drawable titles/captions, and fail-closed row/column/cell budgets.

use super::{reference, Reader};
use crate::pages_proto::{read_reference, ArchiveObject, Message};
use crate::pages_schema::{chart, drawable, shape, storage, ty};
use crate::pages_table::{format_iso_date, MAX_TABLE_CELLS, MAX_TABLE_COLS, MAX_TABLE_ROWS};
use crate::{EasyOfficeError, RichBlock, RichParagraph, RichRun, RichTable};

const MAX_SUPER_DEPTH: usize = 6;
const CELL_OVERHEAD: usize = 16;

const CHART_TYPE_LABELS: [&str; 28] = [
    "미지정",
    "세로 막대형",
    "가로 막대형",
    "꺾은선형",
    "영역형",
    "원형",
    "누적 세로 막대형",
    "누적 가로 막대형",
    "누적 영역형",
    "분산형",
    "혼합형",
    "이중 축형",
    "3D 세로 막대형",
    "3D 가로 막대형",
    "3D 꺾은선형",
    "3D 영역형",
    "3D 원형",
    "3D 누적 세로 막대형",
    "3D 누적 가로 막대형",
    "3D 누적 영역형",
    "대화형 세로 막대형",
    "대화형 가로 막대형",
    "거품형",
    "대화형 분산형",
    "대화형 거품형",
    "도넛형",
    "3D 도넛형",
    "방사형",
];

fn chart_type_korean(kind: u64) -> String {
    usize::try_from(kind)
        .ok()
        .and_then(|idx| CHART_TYPE_LABELS.get(idx).copied())
        .map_or_else(|| format!("유형 {kind}"), str::to_string)
}

fn format_chart_number(v: f64) -> String {
    if !v.is_finite() {
        return String::new();
    }
    let v = if v == 0.0 { 0.0 } else { v };
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        v.to_string()
    }
}

fn decode_grid_value(parent: &Message<'_>, raw: &[u8]) -> Result<String, EasyOfficeError> {
    // A zero-length GridValue submessage is an explicit blank cell: never coerce to 0 or drop!
    if raw.is_empty() {
        return Ok(String::new());
    }
    let val = parent.parse_sub(raw)?;
    if let Some(v) = val.double(chart::VAL_NUMBER) {
        return Ok(format_chart_number(v));
    }
    if let Some(v) = val
        .double(chart::VAL_DATE)
        .or_else(|| val.double(chart::VAL_LEGACY_DATE))
    {
        return Ok(format_iso_date(v).unwrap_or_default());
    }
    if let Some(v) = val.double(chart::VAL_DURATION) {
        let n = format_chart_number(v);
        return Ok(if n.is_empty() {
            String::new()
        } else {
            format!("{n}s")
        });
    }
    Ok(String::new())
}

fn clean_label(s: &str) -> String {
    s.chars()
        .map(|c| {
            if matches!(c, '\n' | '\r' | '\u{2028}') {
                ' '
            } else {
                c
            }
        })
        .filter(|c| !c.is_control() && *c != '\u{FFFC}' && *c != storage::FOOTNOTE_MARK_CHAR)
        .collect::<String>()
        .trim()
        .to_string()
}

impl Reader<'_> {
    fn resolve_label_object(&self, obj_id: u64) -> Option<String> {
        let obj = self.objects.get(obj_id)?;
        let st_msg = match obj.type_id {
            ty::STORAGE => Message::parse(&obj.payload).ok()?,
            ty::CAPTION_INFO | ty::SHAPE_WITH_TEXT => {
                let wrapper = Message::parse(&obj.payload).ok()?;
                let sid = reference(&wrapper, shape::TEXT_STORAGE)?;
                self.storage_message(sid)?
            }
            _ => return None,
        };
        let text = st_msg.text(storage::TEXT).ok()?;
        let cleaned = clean_label(&text);
        (!cleaned.is_empty()).then_some(cleaned)
    }

    fn chart_title_and_caption(
        &self,
        drawable_msg: &Message<'_>,
    ) -> (Option<String>, Option<String>) {
        let mut title = None;
        let mut caption = None;
        let mut desc = None;
        let mut cur = drawable_msg.clone();
        for _ in 0..MAX_SUPER_DEPTH {
            if title.is_none() && cur.varint(drawable::TITLE_HIDDEN) != Some(1) {
                title =
                    reference(&cur, drawable::TITLE).and_then(|id| self.resolve_label_object(id));
            }
            if caption.is_none() && cur.varint(drawable::CAPTION_HIDDEN) != Some(1) {
                caption =
                    reference(&cur, drawable::CAPTION).and_then(|id| self.resolve_label_object(id));
            }
            if desc.is_none() {
                desc = cur
                    .string(drawable::DESCRIPTION)
                    .ok()
                    .flatten()
                    .map(|s| clean_label(&s))
                    .filter(|s| !s.is_empty());
            }
            let Ok(Some(next)) = cur.sub(drawable::GEOMETRY) else {
                break;
            };
            cur = next;
        }
        (title, caption.or(desc))
    }

    fn skip_empty_chart(&mut self, type_id: u32, floating: bool) {
        if floating {
            self.skip(format!("쪽 위의 개체: 차트(형식 {type_id})"));
        } else {
            self.skip(format!("차트(형식 {type_id})"));
        }
    }

    /// Imports a chart (`5021` or `5000`) into a title/type paragraph + `RichBlock::Table`,
    /// or reports empty chart shells as skipped.
    pub(super) fn read_chart(
        &mut self,
        obj: &ArchiveObject,
        floating: bool,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let drawable_msg = Message::parse(&obj.payload)?;
        let chart_msg = match drawable_msg.sub(chart::EXTENSION)? {
            Some(ext) => ext,
            None if obj.type_id == ty::CHART_LEGACY && !drawable_msg.fields.is_empty() => {
                drawable_msg.clone()
            }
            None => {
                self.skip_empty_chart(obj.type_id, floating);
                return Ok(Vec::new());
            }
        };

        let Some(grid_raw) = chart_msg.bytes(chart::GRID) else {
            self.skip_empty_chart(obj.type_id, floating);
            return Ok(Vec::new());
        };
        let grid_msg = if obj.type_id == ty::CHART_LEGACY {
            if let Some(grid_obj) =
                read_reference(grid_raw).and_then(|id| self.object(id, ty::CHART_GRID_LEGACY))
            {
                Message::parse(&grid_obj.payload)?
            } else {
                chart_msg.parse_sub(grid_raw)?
            }
        } else if grid_raw.is_empty() {
            self.skip_empty_chart(obj.type_id, floating);
            return Ok(Vec::new());
        } else {
            chart_msg.parse_sub(grid_raw)?
        };

        let raw_row_names = grid_msg.all_bytes(chart::GRID_ROW_NAMES);
        let raw_col_names = grid_msg.all_bytes(chart::GRID_COL_NAMES);
        let raw_rows = grid_msg.all_bytes(chart::GRID_ROWS);
        if raw_row_names.is_empty() && raw_col_names.is_empty() && raw_rows.is_empty() {
            self.skip_empty_chart(obj.type_id, floating);
            return Ok(Vec::new());
        }

        let data_rows = raw_rows.len().max(raw_row_names.len());
        if data_rows > MAX_TABLE_ROWS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 차트 격자 행 수 한도 초과".to_string(),
            ));
        }
        if raw_col_names.len() > MAX_TABLE_COLS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "Pages 차트 격자 열 수 한도 초과".to_string(),
            ));
        }

        let mut parsed_rows: Vec<Vec<&[u8]>> = Vec::with_capacity(raw_rows.len());
        let mut max_row_cols = 0usize;
        for row_bytes in raw_rows {
            if row_bytes.is_empty() {
                parsed_rows.push(Vec::new());
                continue;
            }
            let row_msg = grid_msg.parse_sub(row_bytes)?;
            let cells = row_msg.all_bytes(chart::ROW_VALUES);
            if cells.len() > MAX_TABLE_COLS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 차트 격자 열 수 한도 초과".to_string(),
                ));
            }
            max_row_cols = max_row_cols.max(cells.len());
            parsed_rows.push(cells);
        }

        let data_cols = raw_col_names.len().max(max_row_cols);
        let has_col_header = !raw_col_names.is_empty();
        let has_row_header = !raw_row_names.is_empty();
        let out_rows = data_rows + usize::from(has_col_header);
        let out_cols = data_cols + usize::from(has_row_header);
        if out_rows == 0 || out_cols == 0 {
            self.skip_empty_chart(obj.type_id, floating);
            return Ok(Vec::new());
        }
        let total_cells = out_rows
            .checked_mul(out_cols)
            .filter(|n| *n <= MAX_TABLE_CELLS)
            .ok_or_else(|| {
                EasyOfficeError::ResourceBudgetExceeded(
                    "Pages 차트 격자 셀 수 한도 초과".to_string(),
                )
            })?;

        self.tables.charge_table()?;
        self.tables.charge_cells(total_cells)?;
        self.text_budget.charge(
            total_cells.saturating_mul(std::mem::size_of::<String>()),
            "Pages 차트 격자 슬롯",
        )?;

        let decode_name = |raw: &[u8]| -> String { clean_label(&String::from_utf8_lossy(raw)) };

        let mut grid_out: Vec<Vec<String>> = Vec::with_capacity(out_rows);
        if has_col_header {
            let mut hdr = Vec::with_capacity(out_cols);
            if has_row_header {
                hdr.push(String::new());
            }
            for col_idx in 0..data_cols {
                let label = raw_col_names
                    .get(col_idx)
                    .map(|b| decode_name(b))
                    .unwrap_or_default();
                if !label.is_empty() {
                    self.text_budget.charge(
                        label.len().saturating_add(CELL_OVERHEAD),
                        "Pages 차트 열 이름",
                    )?;
                }
                hdr.push(label);
            }
            grid_out.push(hdr);
        }

        for r in 0..data_rows {
            let mut row_vec = Vec::with_capacity(out_cols);
            if has_row_header {
                let r_name = raw_row_names
                    .get(r)
                    .map(|b| decode_name(b))
                    .unwrap_or_default();
                if !r_name.is_empty() {
                    self.text_budget.charge(
                        r_name.len().saturating_add(CELL_OVERHEAD),
                        "Pages 차트 행 이름",
                    )?;
                }
                row_vec.push(r_name);
            }
            let row_cells = parsed_rows.get(r).map_or(&[][..], Vec::as_slice);
            for c in 0..data_cols {
                let text = match row_cells.get(c) {
                    Some(raw) => decode_grid_value(&grid_msg, raw)?,
                    None => String::new(),
                };
                if !text.is_empty() {
                    self.text_budget
                        .charge(text.len().saturating_add(CELL_OVERHEAD), "Pages 차트 값")?;
                }
                row_vec.push(text);
            }
            grid_out.push(row_vec);
        }

        let chart_kind = chart_msg.varint(chart::CHART_TYPE).unwrap_or(0);
        let kind_label = chart_type_korean(chart_kind);
        let (title, caption) = self.chart_title_and_caption(&drawable_msg);
        let heading_line = match &title {
            Some(t) => format!("차트: {t} ({kind_label})"),
            None => format!("차트 ({kind_label})"),
        };
        self.text_budget
            .charge_block(heading_line.len(), "Pages 차트 제목")?;
        self.count_run()?;

        let mut blocks = Vec::with_capacity(3);
        blocks.push(RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain(heading_line)],
            ..RichParagraph::default()
        }));
        blocks.push(RichBlock::Table(RichTable::new(grid_out)));
        if let Some(cap) = caption.filter(|c| title.as_deref() != Some(c.as_str())) {
            self.text_budget
                .charge_block(cap.len(), "Pages 차트 설명")?;
            self.count_run()?;
            blocks.push(RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain(cap)],
                ..RichParagraph::default()
            }));
        }
        Ok(blocks)
    }
}
