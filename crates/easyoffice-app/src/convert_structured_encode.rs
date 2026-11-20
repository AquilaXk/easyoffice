//! Encoding half of the structured save (PD-073): turns the parsed `StructuredContent` into the
//! bytes of the target format. Sheets get their formulas evaluated for the cached values, document
//! formats get the displayed grids, and PDF goes through the shell's writer when it offers one.

use easyoffice_formats::{
    doc_save_loss, pages_write_loss, CellValueKind, CsvCodec, DocumentFormat, EasyOfficeError,
    LegacyOle2Codec, LegacyXlsPayload, OdfCodec, OdsUnpacked, OoxmlCodec, PdfWriteOptions,
    RichBlock, RichDocument, RichParagraph, RichRun, SheetShown, SheetStyling, SpillRef,
    UniversalDocumentModel, UniversalFormatConverter, XlsxCachedValue, XlsxFormulaCache,
};
use easyoffice_sheet::{
    EvaluatedCellValue, PackedCellCoordinate, SpillArea, WorkbookEngine, WorkbookEvalTarget,
};

use crate::convert_hwp::{write_hwp, RichThumbnailRenderer, ShellWriters};
use crate::convert_ipc::compute_xlsx_formula_cache;
use crate::convert_structured::StructuredContent;
use crate::convert_structured_doc::{
    is_document_format, slide_deck_to_document, slides_to_document,
};
use crate::convert_structured_loss::{warn_unsaved_pivot_fields, warn_unsaved_sheet_objects};
use crate::eval_limit::evaluate_for_file;
use crate::sheet_display::{show_cells, Show};
use crate::sheet_links_xls::xls_links;

/// Upper bound on cells of the value grids rendered when a workbook is saved to a document format.
const MAX_GRID_CELLS: usize = CsvCodec::MAX_GRID_CELLS;
/// Displayed values of one sheet as a rectangle over its used range.
type ValueGrid = Vec<Vec<String>>;
/// Displayed cells of one sheet as `(row, col, text)`.
type DisplayCells = Vec<(u32, u32, String)>;

fn evaluated_kind(v: &EvaluatedCellValue) -> CellValueKind {
    match v {
        EvaluatedCellValue::Number(_) => CellValueKind::Number,
        EvaluatedCellValue::Boolean(_) => CellValueKind::Boolean,
        EvaluatedCellValue::Error(_) => CellValueKind::Error,
        EvaluatedCellValue::Text(_) | EvaluatedCellValue::Empty => CellValueKind::Text,
    }
}

#[inline]
fn cell_is_formula(styling: Option<&SheetStyling>, row: u32, col: u32, raw: &str) -> bool {
    styling.map_or_else(
        || CellValueKind::infer(raw),
        |st| st.cell_kind(row, col, raw),
    ) == CellValueKind::Formula
}

fn cached_value(v: &EvaluatedCellValue) -> Option<XlsxCachedValue> {
    match v {
        EvaluatedCellValue::Number(n) if n.is_finite() => Some(XlsxCachedValue::Number(*n)),
        EvaluatedCellValue::Text(s) => Some(XlsxCachedValue::Text(s.clone())),
        EvaluatedCellValue::Boolean(b) => Some(XlsxCachedValue::Boolean(*b)),
        _ => None,
    }
}

struct SpillRefCoverFilter<'a> {
    spills: &'a [SpillRef],
    min_row: u32,
    max_row: u32,
}

impl<'a> SpillRefCoverFilter<'a> {
    fn new(spills: &'a [SpillRef]) -> Option<Self> {
        if spills.is_empty() {
            return None;
        }
        let mut min_row = u32::MAX;
        let mut max_row = 0;
        for s in spills {
            min_row = min_row.min(s.row);
            max_row = max_row.max(s.row.saturating_add(s.rows));
        }
        Some(Self {
            spills,
            min_row,
            max_row,
        })
    }

    #[inline]
    fn covers_after_anchor(&self, row: u32, col: u32) -> bool {
        if row < self.min_row || row >= self.max_row {
            return false;
        }
        self.spills.iter().any(|a| a.covers_after_anchor(row, col))
    }
}

/// A formula engine holding every sheet, so cross-sheet references resolve. The cells an array
/// filled (`stylings[i].spills`) are results of its formula: the engine calculates them, so they
/// are not cells there (PD-217).
fn workbook_engine(
    sheets: &[OdsUnpacked],
    stylings: &[SheetStyling],
) -> Result<WorkbookEngine, EasyOfficeError> {
    let mut wb = WorkbookEngine::new();
    for (index, (name, cells)) in sheets.iter().enumerate() {
        let spills = stylings.get(index).map_or(&[][..], |st| &st.spills[..]);
        wb.try_add_sheet(name.as_str())?;
        if let Some(styling) = stylings.get(index) {
            // The tables come before the cells, so a structured reference compiles against them (PD-225). The
            // reader and the page already hold them within the bounds; one that is not leaves the references
            // to read as errors.
            let _ = wb.set_tables(name, styling.tables.clone());
            // The rows the sheet hides: `SUBTOTAL` and `AGGREGATE` leave them out (PD-232).
            let (hidden, filtered) = styling.hidden_row_runs();
            wb.try_add_sheet(name.clone())?
                .set_hidden_rows(hidden, filtered);
        }
        let sheet = wb
            .sheet_mut(name.as_str())
            .expect("sheet was just added by try_add_sheet");
        let sheet_styling = stylings.get(index);
        if let Some(styling) = sheet_styling {
            sheet.set_spill_merges(styling.merges.clone());
        }
        if let Some(filter) = SpillRefCoverFilter::new(spills) {
            for (row, col, raw) in cells {
                if filter.covers_after_anchor(*row, *col) {
                    continue;
                }
                if let Ok(coord) = PackedCellCoordinate::new(*row, *col) {
                    let kind = sheet_styling.and_then(|st| st.explicit_cell_kind(*row, *col, raw));
                    sheet.set_cell_typed_coord(coord, raw.clone(), kind);
                }
            }
        } else {
            for (row, col, raw) in cells {
                if let Ok(coord) = PackedCellCoordinate::new(*row, *col) {
                    let kind = sheet_styling.and_then(|st| st.explicit_cell_kind(*row, *col, raw));
                    sheet.set_cell_typed_coord(coord, raw.clone(), kind);
                }
            }
        }
    }
    Ok(wb)
}

/// Every sheet with each formula replaced by the value it displays, for the formats that hold
/// values only (a BIFF8 formula needs a compiled token stream, and CSV has no formulas). Also
/// returns how many formulas were replaced.
fn computed_sheets(
    sheets: &[OdsUnpacked],
    stylings: &[SheetStyling],
) -> Result<(Vec<OdsUnpacked>, Vec<SheetStyling>, usize), EasyOfficeError> {
    let wb = workbook_engine(sheets, stylings)?;
    let mut a1_pairs: Vec<(&str, String)> = Vec::new();
    for (index, (name, cells)) in sheets.iter().enumerate() {
        let st = stylings.get(index);
        for (row, col, raw) in cells {
            if cell_is_formula(st, *row, *col, raw) {
                if let Ok(coord) = PackedCellCoordinate::new(*row, *col) {
                    a1_pairs.push((name.as_str(), coord.to_a1()));
                }
            }
        }
    }
    let formulas = a1_pairs.len();
    let targets: Vec<WorkbookEvalTarget<'_>> = a1_pairs
        .iter()
        .map(|(sheet, a1)| WorkbookEvalTarget {
            sheet,
            expr: a1.as_str(),
            anchor: None,
        })
        .collect();
    let (values, spills) = evaluate_for_file(&wb, &targets, true)?;
    let mut eval_iter = values.into_iter();

    let mut computed = Vec::with_capacity(sheets.len());
    let mut computed_stylings = Vec::with_capacity(sheets.len());
    for (index, (name, cells)) in sheets.iter().enumerate() {
        let orig_st = stylings.get(index);
        let mut st = orig_st.cloned().unwrap_or_default();
        let mut values = Vec::with_capacity(cells.len());
        for (row, col, raw) in cells {
            if !cell_is_formula(orig_st, *row, *col, raw) {
                values.push((*row, *col, raw.clone()));
                continue;
            }
            if PackedCellCoordinate::new(*row, *col).is_err() {
                continue;
            }
            let eval = eval_iter
                .next()
                .flatten()
                .unwrap_or(EvaluatedCellValue::Empty);
            let display = eval.to_display_string();
            st.set_cell_kind(*row, *col, &display, evaluated_kind(&eval));
            values.push((*row, *col, display));
        }
        add_spilled_cells(name, &mut values, Some(&mut st), &spills);
        computed.push((name.clone(), values));
        computed_stylings.push(st);
    }
    Ok((computed, computed_stylings, formulas))
}

/// Adds the cells the arrays of `sheet` filled (as the values they show) to its cells, where it has none (PD-217).
fn add_spilled_cells(
    sheet: &str,
    cells: &mut Vec<(u32, u32, String)>,
    mut styling: Option<&mut SheetStyling>,
    spills: &[SpillArea],
) {
    if spills.is_empty() {
        return;
    }
    let applicable: Vec<&SpillArea> = spills
        .iter()
        .filter(|a| a.sheet == sheet && !a.blocked)
        .collect();
    if applicable.is_empty() {
        return;
    }
    let held: std::collections::HashSet<(u32, u32)> =
        cells.iter().map(|&(r, c, _)| (r, c)).collect();
    for area in applicable {
        for (i, value) in area.values.iter().enumerate() {
            let (dr, dc) = ((i / area.cols) as u32, (i % area.cols) as u32);
            let at = (area.anchor.row() + dr, area.anchor.col() + dc);
            if (dr + dc > 0) && !held.contains(&at) {
                let text = value.to_display_string();
                if let Some(st) = styling.as_deref_mut() {
                    st.set_cell_kind(at.0, at.1, &text, evaluated_kind(value));
                }
                cells.push((at.0, at.1, text));
            }
        }
    }
}

/// `sheets` without the cells the arrays filled: they are results of the arrays' formulas.
fn without_spilled_cells(sheets: &[OdsUnpacked], stylings: &[SheetStyling]) -> Vec<OdsUnpacked> {
    sheets
        .iter()
        .enumerate()
        .map(|(index, (name, cells))| {
            let spills = stylings.get(index).map_or(&[][..], |st| &st.spills[..]);
            let kept = if let Some(filter) = SpillRefCoverFilter::new(spills) {
                cells
                    .iter()
                    .filter(|(row, col, _)| !filter.covers_after_anchor(*row, *col))
                    .cloned()
                    .collect()
            } else {
                cells.clone()
            };
            (name.clone(), kept)
        })
        .collect()
}

/// `sheets` and `stylings` with the cells the arrays filled added as typed values.
fn with_spilled_cells(
    sheets: &[OdsUnpacked],
    stylings: &[SheetStyling],
    spills: &[SpillArea],
) -> (Vec<OdsUnpacked>, Vec<SheetStyling>) {
    if spills.is_empty() {
        return (sheets.to_vec(), stylings.to_vec());
    }
    sheets
        .iter()
        .enumerate()
        .map(|(index, (name, cells))| {
            let mut cells = cells.clone();
            let mut st = stylings.get(index).cloned().unwrap_or_default();
            add_spilled_cells(name, &mut cells, Some(&mut st), spills);
            ((name.clone(), cells), st)
        })
        .unzip()
}

/// Longest list of sheet names a warning spells out.
const WARNED_SHEET_NAMES: usize = 5;

/// XLS: every sheet, formulas as values.
fn encode_xls(
    sheets: &[OdsUnpacked],
    stylings: &[SheetStyling],
    title: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<u8>, EasyOfficeError> {
    let (computed, computed_stylings, formulas) = computed_sheets(sheets, stylings)?;
    warn_unsaved_sheet_objects(stylings, "XLS", warnings);
    if formulas > 0 {
        warnings.push(format!(
            "수식 {formulas}개는 계산된 값으로 저장했습니다. XLS 파일에는 수식이 아니라 값이 들어갑니다."
        ));
    }
    let names: Vec<&str> = computed.iter().map(|(name, _)| name.as_str()).collect();
    let clean = LegacyOle2Codec::sanitize_sheet_names(&names);
    let renamed: Vec<String> = names
        .iter()
        .zip(&clean)
        .filter(|(old, new)| old != new)
        .map(|(old, new)| format!("'{old}' → '{new}'"))
        .collect();
    if !renamed.is_empty() {
        let shown = renamed.len().min(WARNED_SHEET_NAMES);
        let rest = match renamed.len() - shown {
            0 => String::new(),
            more => format!(" 외 {more}개"),
        };
        warnings.push(format!(
            "XLS 시트 이름 규칙(31자 이하, : \\ / ? * [ ] 금지, 중복 불가)에 맞춰 시트 {}개의 이름을 바꿨습니다: {}{rest}.",
            renamed.len(),
            renamed[..shown].join(", ")
        ));
    }
    let mut skipped_links = 0;
    let links = xls_links(stylings, &names, &clean, &mut skipped_links);
    if skipped_links > 0 {
        warnings.push(format!(
            "하이퍼링크 {skipped_links}개는 XLS의 범위(65,536행, 256열) 밖이거나 주소가 너무 길어 저장하지 않았습니다."
        ));
    }
    let formatted: usize = stylings
        .iter()
        .map(|styling| styling.cells.values().filter(|c| c.num_fmt != 0).count())
        .sum();
    if formatted > 0 {
        warnings.push(format!(
            "숫자 서식이 있는 칸 {formatted}개는 XLS에 서식을 저장하지 않아 값만 저장했습니다."
        ));
    }
    let (approximated, dropped) = LegacyOle2Codec::xls_color_losses(stylings);
    if approximated > 0 {
        warnings.push(format!(
            "칸 색 {approximated}가지는 XLS의 56색 팔레트에서 가장 가까운 색으로 저장했습니다."
        ));
    }
    if dropped > 0 {
        warnings.push(format!(
            "색이 있는 칸 {dropped}개는 XLS의 범위(65,536행, 256열) 밖이라 색을 저장하지 않았습니다."
        ));
    }
    let mut payloads: Vec<LegacyXlsPayload> = computed
        .into_iter()
        .map(|(sheet_name, cells)| LegacyXlsPayload { sheet_name, cells })
        .collect();
    if payloads.is_empty() {
        payloads.push(LegacyXlsPayload {
            sheet_name: title.to_string(),
            cells: Vec::new(),
        });
    }
    LegacyOle2Codec::pack_xls_multi_values_styled(&payloads, &links, &computed_stylings)
}

/// CSV/TSV hold one sheet: the first, as values. Sheets left out and cells outside what the CSV
/// reader accepts are reported, never dropped silently.
fn encode_delimited(
    sheets: &[OdsUnpacked],
    stylings: &[SheetStyling],
    format: DocumentFormat,
    title: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<u8>, EasyOfficeError> {
    let label = format.extension().to_uppercase();
    let (computed, computed_stylings, _) = computed_sheets(sheets, stylings)?;
    warn_unsaved_sheet_objects(stylings, &label, warnings);
    let mut computed = computed.into_iter();
    let (first_name, mut cells) = computed.next().unwrap_or_default();
    show_cells(&mut cells, computed_stylings.first(), Show::Values);
    let left_out: Vec<String> = computed.map(|(name, _)| name).collect();
    if !left_out.is_empty() {
        let shown = left_out.len().min(WARNED_SHEET_NAMES);
        let more = left_out.len() - shown;
        let names = left_out[..shown].join(", ");
        let rest = if more > 0 {
            format!(" 외 {more}개")
        } else {
            String::new()
        };
        warnings.push(format!(
            "{label}는 시트 하나만 담을 수 있어 첫 시트 '{first_name}'만 저장했습니다. 나머지 시트 {}개({names}{rest})는 포함되지 않았습니다.",
            left_out.len()
        ));
    }
    let total = cells.len();
    cells.retain(|(row, col, _)| {
        (*row as usize) < CsvCodec::MAX_ROWS && (*col as usize) < CsvCodec::MAX_COLS
    });
    if cells.len() < total {
        warnings.push(format!(
            "{label}가 읽을 수 있는 범위({}행 x {}열)를 벗어난 셀 {}개는 제외했습니다.",
            CsvCodec::MAX_ROWS,
            CsvCodec::MAX_COLS,
            total - cells.len()
        ));
    }
    let model = UniversalDocumentModel {
        title: title.to_string(),
        sheet_cells: cells,
        ..UniversalDocumentModel::default()
    };
    UniversalFormatConverter::encode(&model, format)
}

/// The formula caches, the displayed grids (when asked for) and the arrays that spill of a workbook.
type Evaluated = (Vec<XlsxFormulaCache>, Vec<ValueGrid>, Vec<SpillArea>);

/// Evaluates every formula cell across all sheets (cross-sheet references resolve) and returns
/// one formula cache per sheet plus, when `with_grids`, the displayed-value grid of each sheet.
/// One evaluation covers all sheets, so a formula that sheets after it read is computed once.
fn evaluate_workbook(
    sheets: &[OdsUnpacked],
    stylings: &[SheetStyling],
    with_grids: bool,
) -> Result<Evaluated, EasyOfficeError> {
    // The caches of XLSX and ODS may lack the formulas a slow workbook did not get to; displayed
    // values may not.
    let (mut values, spills) = evaluate_formulas(sheets, stylings, with_grids)?;
    let mut caches = Vec::with_capacity(sheets.len());
    let mut grids = Vec::new();
    let mut grid_cells = 0usize;
    for (index, (name, cells)) in sheets.iter().enumerate() {
        let orig_st = stylings.get(index);
        let mut display_st = with_grids.then(|| orig_st.cloned().unwrap_or_default());
        let (cache, mut displays) =
            sheet_values(cells, orig_st, display_st.as_mut(), &mut values, with_grids);
        caches.push(cache);
        if with_grids {
            add_spilled_cells(name, &mut displays, display_st.as_mut(), &spills);
            show_cells(&mut displays, display_st.as_ref(), Show::Table);
            grids.push(rectangular_grid(&displays, &mut grid_cells)?);
        }
    }
    Ok((caches, grids, spills))
}

type EvaluatedDisplays = (Vec<DisplayCells>, Vec<SheetStyling>, Vec<SheetShown>);

/// The displayed text of every cell of every sheet at its own position (the PDF of a sheet draws the cells where the
/// sheet has them, so a print area and repeated rows address real rows).
fn evaluate_displays(
    sheets: &[OdsUnpacked],
    stylings: &[SheetStyling],
) -> Result<EvaluatedDisplays, EasyOfficeError> {
    let (mut values, spills) = evaluate_formulas(sheets, stylings, true)?;
    let mut all_displays = Vec::with_capacity(sheets.len());
    let mut all_stylings = Vec::with_capacity(sheets.len());
    let mut all_shown = Vec::with_capacity(sheets.len());
    for (index, (name, cells)) in sheets.iter().enumerate() {
        let orig_st = stylings.get(index);
        let mut display_st = orig_st.cloned().unwrap_or_default();
        let mut displays = sheet_values(cells, orig_st, Some(&mut display_st), &mut values, true).1;
        add_spilled_cells(name, &mut displays, Some(&mut display_st), &spills);
        let shown = show_cells(&mut displays, Some(&display_st), Show::Page);
        all_displays.push(displays);
        all_stylings.push(display_st);
        all_shown.push(shown);
    }
    Ok((all_displays, all_stylings, all_shown))
}

/// The value of each formula cell across all sheets in sheet and cell order (`None` for one the
/// time limit cut off, which only a `complete`-less caller gets: the others are refused).
fn evaluate_formulas(
    sheets: &[OdsUnpacked],
    stylings: &[SheetStyling],
    complete: bool,
) -> Result<
    (
        std::vec::IntoIter<Option<EvaluatedCellValue>>,
        Vec<SpillArea>,
    ),
    EasyOfficeError,
> {
    let wb = workbook_engine(sheets, stylings)?;
    let a1_addrs: Vec<(&str, String)> = sheets
        .iter()
        .enumerate()
        .flat_map(|(index, (name, cells))| {
            let st = stylings.get(index);
            cells
                .iter()
                .filter(move |(r, c, raw)| cell_is_formula(st, *r, *c, raw))
                .filter_map(move |(r, c, _)| {
                    PackedCellCoordinate::new(*r, *c)
                        .ok()
                        .map(|coord| (name.as_str(), coord.to_a1()))
                })
        })
        .collect();
    let targets: Vec<WorkbookEvalTarget<'_>> = a1_addrs
        .iter()
        .map(|(sheet, a1)| WorkbookEvalTarget {
            sheet,
            expr: a1.as_str(),
            anchor: None,
        })
        .collect();
    let (values, spills) = evaluate_for_file(&wb, &targets, complete)?;
    Ok((values.into_iter(), spills))
}

/// One sheet's formula cache and (when `with_grids`) displayed cells, taking the value of each
/// formula cell from `values` in cell order.
fn sheet_values(
    cells: &[(u32, u32, String)],
    orig_styling: Option<&SheetStyling>,
    mut display_styling: Option<&mut SheetStyling>,
    values: &mut impl Iterator<Item = Option<EvaluatedCellValue>>,
    with_grids: bool,
) -> (XlsxFormulaCache, DisplayCells) {
    let mut cache = XlsxFormulaCache::new();
    let mut displays = Vec::new();
    for (row, col, raw) in cells {
        let display = if cell_is_formula(orig_styling, *row, *col, raw) {
            if PackedCellCoordinate::new(*row, *col).is_err() {
                continue;
            };
            let value = values.next().flatten();
            if let Some(c) = value.as_ref().and_then(cached_value) {
                cache.insert((*row, *col), c);
            }
            let text = value
                .as_ref()
                .map_or_else(String::new, |v| v.to_display_string());
            if let (Some(st), Some(v)) = (display_styling.as_deref_mut(), value.as_ref()) {
                st.set_cell_kind(*row, *col, &text, evaluated_kind(v));
            }
            text
        } else {
            raw.clone()
        };
        if with_grids {
            displays.push((*row, *col, display));
        }
    }
    (cache, displays)
}

/// The displayed cells as a rectangle over their used range, so every row has the same width and
/// empty rows inside the range keep their place. The grid counts against the total budget before
/// it is allocated, so a sheet that is sparse over a huge range is refused, never cut.
fn rectangular_grid(
    displays: &[(u32, u32, String)],
    grid_cells: &mut usize,
) -> Result<ValueGrid, EasyOfficeError> {
    let Some(first) = displays.first() else {
        return Ok(Vec::new());
    };
    let (mut r0, mut r1, mut c0, mut c1) = (first.0, first.0, first.1, first.1);
    for &(row, col, _) in displays {
        r0 = r0.min(row);
        r1 = r1.max(row);
        c0 = c0.min(col);
        c1 = c1.max(col);
    }
    let (rows, cols) = ((r1 - r0) as usize + 1, (c1 - c0) as usize + 1);
    *grid_cells = grid_cells.saturating_add(rows.saturating_mul(cols));
    if *grid_cells > MAX_GRID_CELLS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "시트 값 표가 너무 커서 문서 형식으로 저장할 수 없습니다.".to_string(),
        ));
    }
    let mut grid = vec![vec![String::new(); cols]; rows];
    for (row, col, display) in displays {
        grid[(row - r0) as usize][(col - c0) as usize].clone_from(display);
    }
    Ok(grid)
}

/// Draws a document's pages for a host that can: the document as HWPX and the page settings in,
/// the PDF out (PD-167).
pub type RichPdfRenderer<'a> = dyn Fn(&[u8], &PdfWriteOptions) -> Result<Vec<u8>, String> + 'a;

/// Warning for a host render that failed and left the engine PDF to be written.
fn render_fallback_warning(reason: &str) -> String {
    format!("서식을 살린 PDF를 만들지 못해 기본 PDF(글자 중심)로 저장했습니다: {reason}")
}

/// Whether a host that draws pages takes this conversion. Two kinds of request keep the engine's
/// PDF: a PDF studio save (`source` is PDF), whose page markers the studio reads back, and a
/// highlight phrase, which is an annotation object only the engine writes.
fn host_draws_pdf(
    format: DocumentFormat,
    source: DocumentFormat,
    options: &PdfWriteOptions,
) -> bool {
    format == DocumentFormat::Pdf && source != DocumentFormat::Pdf && options.highlight.is_none()
}

/// [`encode_structured`], except that a document converted to PDF is drawn by `renderer` when
/// there is one (see [`host_draws_pdf`]), and a document converted to HWP 5.0 is written by
/// `writers.hwp` (PD-178). When the renderer fails the engine PDF is written and
/// `warnings` says why.
pub(crate) fn encode_target(
    content: &StructuredContent,
    format: DocumentFormat,
    source: DocumentFormat,
    title: &str,
    writers: ShellWriters<'_>,
    warnings: &mut Vec<String>,
) -> Result<Vec<u8>, EasyOfficeError> {
    if format == DocumentFormat::Hwp {
        let hwpx = encode_structured(content, DocumentFormat::Hwpx, title, warnings)?;
        return write_hwp(&hwpx, writers.hwp, writers.thumbnail);
    }
    if let (Some(render), StructuredContent::Document(doc, options)) = (writers.pdf, content) {
        if host_draws_pdf(format, source, options) {
            match render_document(doc, title, options, render) {
                Ok(pdf) => return Ok(pdf),
                Err(reason) => warnings.push(render_fallback_warning(&reason)),
            }
        }
    }
    encode_structured_with_thumbnail(content, format, title, writers.thumbnail, None, warnings)
}

/// [`encode_target`] for a converted source file. A `.hwp`/`.hwpx` source goes to the host as the
/// file it is: writing it back through the engine's model would drop its page size, margins,
/// headers and fonts. Other sources go through the model as HWPX. Either way the host also gets
/// the request's page settings.
pub(crate) fn encode_source_target(
    content: &StructuredContent,
    source_bytes: &[u8],
    format: DocumentFormat,
    source: DocumentFormat,
    title: &str,
    writers: ShellWriters<'_>,
    warnings: &mut Vec<String>,
) -> Result<Vec<u8>, EasyOfficeError> {
    let native = matches!(source, DocumentFormat::Hwp | DocumentFormat::Hwpx);
    if let (Some(render), StructuredContent::Document(_, options)) = (writers.pdf, content) {
        if native && host_draws_pdf(format, source, options) {
            match render(source_bytes, options) {
                Ok(pdf) => return Ok(pdf),
                Err(reason) => {
                    warnings.push(render_fallback_warning(&reason));
                    return encode_structured(content, format, title, warnings);
                }
            }
        }
    }
    if native && format != DocumentFormat::Hwp && format != DocumentFormat::Pdf {
        return encode_structured_with_thumbnail(
            content,
            format,
            title,
            writers.thumbnail,
            Some(source_bytes),
            warnings,
        );
    }
    encode_target(content, format, source, title, writers, warnings)
}

fn encode_rich_for_host(
    doc: &RichDocument,
    format: DocumentFormat,
    cache: Option<&XlsxFormulaCache>,
    thumbnail: Option<&RichThumbnailRenderer<'_>>,
    native_layout_bytes: Option<&[u8]>,
) -> Result<Vec<u8>, EasyOfficeError> {
    let Some(render) = thumbnail.filter(|_| easyoffice_formats::supports_thumbnail(format)) else {
        return UniversalFormatConverter::encode_rich(doc, format, cache);
    };
    if matches!(format, DocumentFormat::Hwpx | DocumentFormat::Hwtx)
        && native_layout_bytes.is_none()
    {
        let hwpx = UniversalFormatConverter::encode_rich(doc, format, cache)?;
        if let Ok(png) = render(&hwpx) {
            if let Ok(with_thumb) = easyoffice_formats::embed_thumbnail(format, &hwpx, &png) {
                return Ok(with_thumb);
            }
        }
        return Ok(hwpx);
    }
    let png = match native_layout_bytes {
        Some(bytes) => render(bytes).ok(),
        None => UniversalFormatConverter::encode_rich(doc, DocumentFormat::Hwpx, None)
            .ok()
            .and_then(|hwpx| render(&hwpx).ok()),
    };
    if let Some(png) = png {
        if let Ok(with_thumb) =
            UniversalFormatConverter::encode_rich_with_thumbnail(doc, format, cache, Some(&png))
        {
            return Ok(with_thumb);
        }
    }
    UniversalFormatConverter::encode_rich(doc, format, cache)
}

/// The document as HWPX, drawn by `render`.
fn render_document(
    doc: &RichDocument,
    title: &str,
    options: &PdfWriteOptions,
    render: &RichPdfRenderer<'_>,
) -> Result<Vec<u8>, String> {
    let doc = RichDocument {
        title: title.to_string(),
        ..doc.clone()
    };
    let hwpx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None)
        .map_err(|err| err.to_string())?;
    render(&hwpx, options)
}

/// Encodes structured content to `format`, titling the output `title`. What the format cannot
/// hold of the content is added to `warnings`.
pub(crate) fn encode_structured(
    content: &StructuredContent,
    format: DocumentFormat,
    title: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<u8>, EasyOfficeError> {
    encode_structured_with_thumbnail(content, format, title, None, None, warnings)
}

fn encode_structured_with_thumbnail(
    content: &StructuredContent,
    format: DocumentFormat,
    title: &str,
    thumbnail: Option<&RichThumbnailRenderer<'_>>,
    native_layout_bytes: Option<&[u8]>,
    warnings: &mut Vec<String>,
) -> Result<Vec<u8>, EasyOfficeError> {
    if format == DocumentFormat::Pages && !matches!(content, StructuredContent::Document(..)) {
        return Err(EasyOfficeError::InvalidArgument(
            "Pages(.pages)로는 문서 내용만 저장할 수 있습니다. 시트·슬라이드는 다른 형식으로 저장하세요."
                .to_string(),
        ));
    }
    match content {
        StructuredContent::Document(doc, pdf) => {
            let doc = RichDocument {
                title: title.to_string(),
                ..doc.clone()
            };
            let label = format.extension().to_uppercase();
            warnings.extend(doc_save_loss(&doc, format).warnings(&label));
            if format == DocumentFormat::Pdf {
                let (bytes, notes) = UniversalFormatConverter::encode_rich_pdf(&doc, pdf);
                warnings.extend(notes);
                return Ok(bytes);
            }
            if format == DocumentFormat::Pages {
                warnings.extend(pages_write_loss(&doc).warnings());
            }
            let cache = matches!(format, DocumentFormat::Xlsx | DocumentFormat::Ods)
                .then(|| compute_xlsx_formula_cache(&doc.to_universal_model()));
            encode_rich_for_host(&doc, format, cache.as_ref(), thumbnail, native_layout_bytes)
        }
        StructuredContent::Workbook(page_sheets, stylings) => {
            // The values, displays and date filters of the workbook count from its own date system (PD-263).
            let _date_system =
                easyoffice_sheet::enter_date_system(stylings.first().is_some_and(|st| st.date1904));
            // The cells an array filled are its formula's results: the engine gives them (and they come
            // back from `with_spilled_cells` and `add_spilled_cells` below), whatever text the page sent.
            let engine_sheets = without_spilled_cells(page_sheets, stylings);
            let sheets: &[OdsUnpacked] = &engine_sheets;
            match format {
                DocumentFormat::Xlsx => {
                    warn_unsaved_pivot_fields(stylings, warnings);
                    let (caches, _, spills) = evaluate_workbook(sheets, stylings, false)?;
                    let (out_sheets, out_stylings) = with_spilled_cells(sheets, stylings, &spills);
                    OoxmlCodec::pack_xlsx_multi_styled(&out_sheets, &caches, &out_stylings)
                }
                DocumentFormat::Ods => {
                    warn_unsaved_sheet_objects(stylings, "ODS", warnings);
                    let (caches, _, spills) = evaluate_workbook(sheets, stylings, false)?;
                    let (out_sheets, out_stylings) = with_spilled_cells(sheets, stylings, &spills);
                    OdfCodec::pack_ods_multi_styled_with_cache(&out_sheets, &caches, &out_stylings)
                }
                DocumentFormat::Pdf => {
                    let (displays, display_stylings, shown) = evaluate_displays(sheets, stylings)?;
                    let named: Vec<OdsUnpacked> = sheets
                        .iter()
                        .map(|(name, _)| name.clone())
                        .zip(displays)
                        .collect();
                    easyoffice_formats::encode_sheet_pdf(title, &named, &display_stylings, &shown)
                }
                DocumentFormat::Xls => encode_xls(sheets, stylings, title, warnings),
                DocumentFormat::Csv | DocumentFormat::Tsv => {
                    encode_delimited(sheets, stylings, format, title, warnings)
                }
                _ => {
                    let (_, grids, _) = evaluate_workbook(sheets, stylings, true)?;
                    let mut blocks = Vec::with_capacity(sheets.len() * 2);
                    for ((name, _), grid) in sheets.iter().zip(grids) {
                        blocks.push(RichBlock::Paragraph(RichParagraph {
                            runs: vec![RichRun::plain(name.clone())],
                            heading_level: 2,
                            ..RichParagraph::default()
                        }));
                        if !grid.is_empty() {
                            blocks.push(RichBlock::Table(grid.into()));
                        }
                    }
                    let doc = RichDocument {
                        page_frame: Default::default(),
                        title: title.to_string(),
                        blocks,
                        official_agency: None,
                        ..RichDocument::default()
                    };
                    encode_rich_for_host(&doc, format, None, thumbnail, None)
                }
            }
        }
        StructuredContent::SlideDeck(deck) if format == DocumentFormat::Pdf => {
            easyoffice_formats::encode_slide_pdf(deck, title)
        }
        StructuredContent::SlideDeck(deck) if is_document_format(format) => {
            let doc = slide_deck_to_document(deck, title);
            encode_rich_for_host(&doc, format, None, thumbnail, None)
        }
        StructuredContent::SlideDeck(deck) => {
            let mut titled = deck.clone();
            if !title.trim().is_empty() {
                titled.title = title.to_string();
            }
            UniversalFormatConverter::encode_rich_slides(&titled, format)
        }
        StructuredContent::Slides(slides) if is_document_format(format) => {
            let doc = slides_to_document(slides, title);
            encode_rich_for_host(&doc, format, None, thumbnail, None)
        }
        StructuredContent::Slides(slides) => {
            let model = UniversalDocumentModel {
                title: title.to_string(),
                slides: slides.clone(),
                ..UniversalDocumentModel::default()
            };
            UniversalFormatConverter::encode(&model, format)
        }
    }
}
