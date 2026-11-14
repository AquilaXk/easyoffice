//! Encoding half of the structured save (PD-073): turns the parsed `StructuredContent` into the
//! bytes of the target format. Sheets get their formulas evaluated for the cached values, document
//! formats get the displayed grids, and PDF goes through the shell's writer when it offers one.

use easyoffice_formats::{
    pages_write_loss, CsvCodec, DocumentFormat, EasyOfficeError, LegacyOle2Codec, LegacyXlsPayload,
    OdfCodec, OdsUnpacked, OoxmlCodec, PdfWriteOptions, RichBlock, RichDocument, RichParagraph,
    RichRun, SheetStyling, UniversalDocumentModel, UniversalFormatConverter, XlsxCachedValue,
    XlsxFormulaCache,
};
use easyoffice_sheet::{
    EvaluatedCellValue, PackedCellCoordinate, SpillArea, WorkbookEngine, WorkbookEvalTarget,
};

use crate::convert_hwp::{write_hwp, ShellWriters};
use crate::convert_ipc::compute_xlsx_formula_cache;
use crate::convert_structured::StructuredContent;
use crate::convert_structured_doc::{is_document_format, slides_to_document};
use crate::eval_limit::evaluate_for_file;
use crate::sheet_links_xls::xls_links;

/// Upper bound on cells of the value grids rendered when a workbook is saved to a document format.
const MAX_GRID_CELLS: usize = CsvCodec::MAX_GRID_CELLS;
/// Displayed values of one sheet as a rectangle over its used range.
type ValueGrid = Vec<Vec<String>>;
/// Displayed cells of one sheet as `(row, col, text)`.
type DisplayCells = Vec<(u32, u32, String)>;

fn cached_value(v: &EvaluatedCellValue) -> Option<XlsxCachedValue> {
    match v {
        EvaluatedCellValue::Number(n) if n.is_finite() => Some(XlsxCachedValue::Number(*n)),
        EvaluatedCellValue::Text(s) => Some(XlsxCachedValue::Text(s.clone())),
        EvaluatedCellValue::Boolean(b) => Some(XlsxCachedValue::Boolean(*b)),
        _ => None,
    }
}

/// A formula engine holding every sheet, so cross-sheet references resolve. The cells an array
/// filled (`stylings[i].spills`) are results of its formula: the engine calculates them, so they
/// are not cells there (PD-217).
fn workbook_engine(sheets: &[OdsUnpacked], stylings: &[SheetStyling]) -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    for (index, (name, cells)) in sheets.iter().enumerate() {
        let spills = stylings.get(index).map_or(&[][..], |st| &st.spills[..]);
        wb.add_sheet(name.clone());
        if let Some(styling) = stylings.get(index) {
            wb.add_sheet(name.clone())
                .set_spill_merges(styling.merges.clone());
            // The tables come before the cells, so a structured reference compiles against them (PD-225). The
            // reader and the page already hold them within the bounds; one that is not leaves the references
            // to read as errors.
            let _ = wb.set_tables(name, styling.tables.clone());
        }
        let sheet = wb.add_sheet(name.clone());
        for (row, col, raw) in cells {
            if spills
                .iter()
                .any(|area| area.covers_after_anchor(*row, *col))
            {
                continue;
            }
            if let Ok(coord) = PackedCellCoordinate::new(*row, *col) {
                sheet.set_cell_coord(coord, raw.clone());
            }
        }
    }
    wb
}

/// Every sheet with each formula replaced by the value it displays, for the formats that hold
/// values only (a BIFF8 formula needs a compiled token stream, and CSV has no formulas). Also
/// returns how many formulas were replaced.
fn computed_sheets(
    sheets: &[OdsUnpacked],
    stylings: &[SheetStyling],
) -> Result<(Vec<OdsUnpacked>, usize), EasyOfficeError> {
    let wb = workbook_engine(sheets, stylings);
    let mut a1_pairs: Vec<(&str, String)> = Vec::new();
    for (name, cells) in sheets {
        for (row, col, raw) in cells {
            if raw.starts_with('=') {
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
    for (name, cells) in sheets {
        let mut values = Vec::with_capacity(cells.len());
        for (row, col, raw) in cells {
            if !raw.starts_with('=') {
                values.push((*row, *col, raw.clone()));
                continue;
            }
            if PackedCellCoordinate::new(*row, *col).is_err() {
                continue;
            }
            let value = eval_iter
                .next()
                .flatten()
                .unwrap_or(EvaluatedCellValue::Empty)
                .to_display_string();
            values.push((*row, *col, value));
        }
        add_spilled_cells(name, &mut values, &spills);
        computed.push((name.clone(), values));
    }
    Ok((computed, formulas))
}

/// Adds the cells the arrays of `sheet` filled (as the values they show) to its cells, where it has none (PD-217).
fn add_spilled_cells(sheet: &str, cells: &mut Vec<(u32, u32, String)>, spills: &[SpillArea]) {
    let held: std::collections::HashSet<(u32, u32)> =
        cells.iter().map(|&(r, c, _)| (r, c)).collect();
    for area in spills.iter().filter(|a| a.sheet == sheet && !a.blocked) {
        for (i, value) in area.values.iter().enumerate() {
            let (dr, dc) = ((i / area.cols) as u32, (i % area.cols) as u32);
            let at = (area.anchor.row() + dr, area.anchor.col() + dc);
            if (dr + dc > 0) && !held.contains(&at) {
                cells.push((at.0, at.1, value.to_display_string()));
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
            let kept = cells
                .iter()
                .filter(|(row, col, _)| !spills.iter().any(|a| a.covers_after_anchor(*row, *col)))
                .cloned()
                .collect();
            (name.clone(), kept)
        })
        .collect()
}

/// `sheets` with the cells the arrays filled added as values.
fn with_spilled_cells(sheets: &[OdsUnpacked], spills: &[SpillArea]) -> Vec<OdsUnpacked> {
    sheets
        .iter()
        .map(|(name, cells)| {
            let mut cells = cells.clone();
            add_spilled_cells(name, &mut cells, spills);
            (name.clone(), cells)
        })
        .collect()
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
    let (computed, formulas) = computed_sheets(sheets, stylings)?;
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
    LegacyOle2Codec::pack_xls_multi_values_styled(&payloads, &links, stylings)
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
    let (computed, _) = computed_sheets(sheets, stylings)?;
    let mut computed = computed.into_iter();
    let (first_name, mut cells) = computed.next().unwrap_or_default();
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
    for (name, cells) in sheets {
        let (cache, mut displays) = sheet_values(cells, &mut values, with_grids);
        caches.push(cache);
        if with_grids {
            add_spilled_cells(name, &mut displays, &spills);
            grids.push(rectangular_grid(&displays, &mut grid_cells)?);
        }
    }
    Ok((caches, grids, spills))
}

/// The displayed text of every cell of every sheet at its own position (the PDF of a sheet draws the cells where the
/// sheet has them, so a print area and repeated rows address real rows).
fn evaluate_displays(
    sheets: &[OdsUnpacked],
    stylings: &[SheetStyling],
) -> Result<Vec<DisplayCells>, EasyOfficeError> {
    let (mut values, spills) = evaluate_formulas(sheets, stylings, true)?;
    Ok(sheets
        .iter()
        .map(|(name, cells)| {
            let mut displays = sheet_values(cells, &mut values, true).1;
            add_spilled_cells(name, &mut displays, &spills);
            displays
        })
        .collect())
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
    let wb = workbook_engine(sheets, stylings);
    let a1_addrs: Vec<(&str, String)> = sheets
        .iter()
        .flat_map(|(name, cells)| {
            cells
                .iter()
                .filter(|(_, _, raw)| raw.starts_with('='))
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
    values: &mut impl Iterator<Item = Option<EvaluatedCellValue>>,
    with_grids: bool,
) -> (XlsxFormulaCache, DisplayCells) {
    let mut cache = XlsxFormulaCache::new();
    let mut displays = Vec::new();
    for (row, col, raw) in cells {
        let display = if raw.starts_with('=') {
            if PackedCellCoordinate::new(*row, *col).is_err() {
                continue;
            };
            let value = values.next().flatten();
            if let Some(c) = value.as_ref().and_then(cached_value) {
                cache.insert((*row, *col), c);
            }
            value.map_or_else(String::new, |v| v.to_display_string())
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
        return write_hwp(&hwpx, writers.hwp);
    }
    if let (Some(render), StructuredContent::Document(doc, options)) = (writers.pdf, content) {
        if host_draws_pdf(format, source, options) {
            match render_document(doc, title, options, render) {
                Ok(pdf) => return Ok(pdf),
                Err(reason) => warnings.push(render_fallback_warning(&reason)),
            }
        }
    }
    encode_structured(content, format, title, warnings)
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
    if let (Some(render), StructuredContent::Document(_, options)) = (writers.pdf, content) {
        let native = matches!(source, DocumentFormat::Hwp | DocumentFormat::Hwpx);
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
    encode_target(content, format, source, title, writers, warnings)
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
            UniversalFormatConverter::encode_rich(&doc, format, cache.as_ref())
        }
        StructuredContent::Workbook(page_sheets, stylings) => {
            // The cells an array filled are its formula's results: the engine gives them (and they come
            // back from `with_spilled_cells` and `add_spilled_cells` below), whatever text the page sent.
            let engine_sheets = without_spilled_cells(page_sheets, stylings);
            let sheets: &[OdsUnpacked] = &engine_sheets;
            match format {
                DocumentFormat::Xlsx => {
                    let (caches, _, spills) = evaluate_workbook(sheets, stylings, false)?;
                    OoxmlCodec::pack_xlsx_multi_styled(
                        &with_spilled_cells(sheets, &spills),
                        &caches,
                        stylings,
                    )
                }
                DocumentFormat::Ods => {
                    let (caches, _, spills) = evaluate_workbook(sheets, stylings, false)?;
                    OdfCodec::pack_ods_multi_styled_with_cache(
                        &with_spilled_cells(sheets, &spills),
                        &caches,
                        stylings,
                    )
                }
                DocumentFormat::Pdf => {
                    let displays = evaluate_displays(sheets, stylings)?;
                    let named: Vec<OdsUnpacked> = sheets
                        .iter()
                        .map(|(name, _)| name.clone())
                        .zip(displays)
                        .collect();
                    easyoffice_formats::encode_sheet_pdf(title, &named, stylings)
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
                    };
                    UniversalFormatConverter::encode_rich(&doc, format, None)
                }
            }
        }
        StructuredContent::Slides(slides) if is_document_format(format) => {
            let doc = slides_to_document(slides, title);
            UniversalFormatConverter::encode_rich(&doc, format, None)
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
