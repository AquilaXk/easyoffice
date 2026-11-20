// 참조 구현: jmcnamara/rust_xlsxwriter (MIT OR Apache-2.0, Copyright 2022-2026 John McNamara, src/pivot_table.rs, src/pivot_cache.rs; see THIRD_PARTY_NOTICES.md)

//! Bidirectional OOXML SpreadsheetML pivot table & pivot cache codec (`PD-255`).

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::sheet_names::fold_name;
use crate::sheet_styling::sheet_chart::{parse_sheet_and_box, SheetCellIndex};
use crate::sheet_styling::sheet_pivot::{
    resolve_pivot_source, validate_workbook_pivots, CacheCell, PivotAggKind, PivotFilterCriteria,
    PivotSlicer, PivotValueField, SharedItemsCol, SheetPivotTable, MAX_PIVOTS_PER_SHEET,
    MAX_PIVOT_AXIS_FIELDS, MAX_PIVOT_FIELD_NAMES_BYTES, MAX_PIVOT_FILTERS, MAX_PIVOT_FILTER_VALUES,
    MAX_PIVOT_SLICERS, MAX_PIVOT_TEXT_BYTES, MAX_PIVOT_VALUE_FIELDS, MAX_WORKBOOK_PIVOTS,
    MAX_WORKBOOK_PIVOT_FIELD_NAMES_BYTES,
};
use crate::sheet_styling::xlsx_parts::resolve_zip_target;
use crate::sheet_styling::xlsx_slicers::{MAX_SLICER_DIM_PX, MAX_SLICER_POS_PX, MIN_SLICER_DIM_PX};
use crate::sheet_styling::SheetStyling;
use crate::xml_stream::{unescape_cow, Attrs, XmlEvent, XmlReader};
use crate::{CellBox, EasyOfficeError, OdsUnpacked, OoxmlCodec, XlsxFormulaCache, XmlSanitizer};

const PIVOT_CACHE_DEF_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition";
const PIVOT_CACHE_REC_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheRecords";
const PIVOT_TABLE_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable";
const PIVOT_CACHE_DEF_CT: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml";
const PIVOT_CACHE_REC_CT: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml";
const PIVOT_TABLE_CT: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml";
const EO_PIVOT_EXT_URI: &str = "{E0511CE5-2420-4E05-9A11-511CE5000255}";
const EO_PIVOT_NS: &str = "urn:easyoffice:spreadsheet:pivots:1.0";
const SPREADSHEET_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_RELS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const MAX_CACHE_RECORDS: usize = 100_000;
const MAX_PIVOT_XML_EVENTS: usize = 262_144;

#[inline]
fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    sheet: String,
    range: String,
    table: Option<String>,
}

/// Generates `xl/pivotCache/*`, `xl/pivotTables/*`, workbook `<pivotCaches>`, relationships, and content types.
pub(crate) struct PivotWriter {
    pub(crate) parts: Vec<(String, Vec<u8>)>,
    pub(crate) content_types: String,
    pub(crate) wb_rels: String,
    pub(crate) wb_pivot_caches: String,
    pub(crate) sheet_rels: Vec<String>,
}

impl PivotWriter {
    pub(crate) fn build(
        sheets: &[OdsUnpacked],
        caches: &[XlsxFormulaCache],
        stylings: &[SheetStyling],
    ) -> Result<Self, EasyOfficeError> {
        validate_workbook_pivots(stylings)?;
        let mut sheet_rels = vec![String::new(); sheets.len()];
        if stylings.iter().all(|st| st.pivots.is_empty()) {
            return Ok(Self {
                parts: Vec::new(),
                content_types: String::new(),
                wb_rels: String::new(),
                wb_pivot_caches: String::new(),
                sheet_rels,
            });
        }
        let cell_index = SheetCellIndex::build(sheets);
        let max_rows_by_sheet: Vec<u32> = sheets
            .iter()
            .map(|(_, cells)| cells.iter().map(|&(r, _, _)| r).max().unwrap_or(0))
            .collect();
        let (mut parts, mut content_types, mut wb_rels, mut wb_caches_inner) =
            (Vec::new(), String::new(), String::new(), String::new());
        let mut cache_ids: HashMap<CacheKey, (u32, Vec<String>)> = HashMap::new();
        let (mut next_cid, mut next_pid) = (1u32, 1usize);

        for (s_idx, (sheet_name, _)) in sheets.iter().enumerate() {
            let Some(st) = stylings.get(s_idx) else {
                continue;
            };
            for pivot in &st.pivots {
                let (src_sheet, src_box) =
                    resolve_pivot_source(pivot, sheet_name, sheets, stylings);
                let key = CacheKey {
                    sheet: fold_name(&src_sheet),
                    range: src_box.to_a1(),
                    table: pivot.source_table.as_deref().map(fold_name),
                };
                let (cid, field_names) = if let Some((c, n)) = cache_ids.get(&key) {
                    (*c, n.clone())
                } else {
                    let cid = next_cid;
                    next_cid += 1;
                    let names = write_cache_parts(
                        cid,
                        &src_sheet,
                        src_box,
                        pivot,
                        &cell_index,
                        caches,
                        sheets,
                        &max_rows_by_sheet,
                        &mut parts,
                        &mut content_types,
                    )?;
                    let _ = write!(wb_rels, "<Relationship Id=\"rIdPC{cid}\" Type=\"{PIVOT_CACHE_DEF_REL}\" Target=\"pivotCache/pivotCacheDefinition{cid}.xml\"/>");
                    let _ = write!(
                        wb_caches_inner,
                        "<pivotCache cacheId=\"{cid}\" r:id=\"rIdPC{cid}\"/>"
                    );
                    cache_ids.insert(key, (cid, names.clone()));
                    (cid, names)
                };

                let p_num = next_pid;
                next_pid += 1;
                let pt_xml = build_pivot_table_xml(pivot, cid, &field_names);
                XmlSanitizer::validate_safe_xml(&pt_xml)?;
                let pt_rels = format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{PKG_RELS_NS}\"><Relationship Id=\"rId1\" Type=\"{PIVOT_CACHE_DEF_REL}\" Target=\"../pivotCache/pivotCacheDefinition{cid}.xml\"/></Relationships>"
                );
                XmlSanitizer::validate_safe_xml(&pt_rels)?;
                parts.push((
                    format!("xl/pivotTables/pivotTable{p_num}.xml"),
                    pt_xml.into_bytes(),
                ));
                parts.push((
                    format!("xl/pivotTables/_rels/pivotTable{p_num}.xml.rels"),
                    pt_rels.into_bytes(),
                ));
                let _ = write!(content_types, "<Override PartName=\"/xl/pivotTables/pivotTable{p_num}.xml\" ContentType=\"{PIVOT_TABLE_CT}\"/>");
                let _ = write!(sheet_rels[s_idx], "<Relationship Id=\"rIdPT{p_num}\" Type=\"{PIVOT_TABLE_REL}\" Target=\"../pivotTables/pivotTable{p_num}.xml\"/>");
            }
        }
        let wb_pivot_caches = if wb_caches_inner.is_empty() {
            String::new()
        } else {
            format!("<pivotCaches>{wb_caches_inner}</pivotCaches>")
        };
        Ok(Self {
            parts,
            content_types,
            wb_rels,
            wb_pivot_caches,
            sheet_rels,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn write_cache_parts(
    cid: u32,
    src_sheet: &str,
    src_box: CellBox,
    pivot: &SheetPivotTable,
    cell_index: &SheetCellIndex<'_>,
    caches: &[XlsxFormulaCache],
    sheets: &[OdsUnpacked],
    max_rows_by_sheet: &[u32],
    parts: &mut Vec<(String, Vec<u8>)>,
    content_types: &mut String,
) -> Result<Vec<String>, EasyOfficeError> {
    let col_count = (src_box.col1 - src_box.col0 + 1) as usize;
    let field_names: Vec<String> = (0..col_count)
        .map(|c_off| {
            let header = pivot
                .has_header
                .then(|| {
                    cell_index
                        .cell_display_text(
                            src_sheet,
                            src_box.row0,
                            src_box.col0 + c_off as u32,
                            caches,
                        )
                        .filter(|s| !s.trim().is_empty())
                })
                .flatten();
            pivot
                .field_names
                .get(c_off)
                .filter(|s| !s.trim().is_empty())
                .cloned()
                .or(header)
                .unwrap_or_else(|| format!("Column{}", c_off + 1))
        })
        .collect();

    let data_row0 = src_box.row0 + u32::from(pivot.has_header);
    let sheet_max_r = sheets
        .iter()
        .position(|(n, _)| n.eq_ignore_ascii_case(src_sheet))
        .map_or(src_box.row1, |i| max_rows_by_sheet[i]);
    let data_row1 = src_box.row1.min(sheet_max_r);

    let mut cols: Vec<SharedItemsCol> = (0..col_count).map(|_| SharedItemsCol::new()).collect();
    let mut records_xml = String::with_capacity(1024);
    let mut record_count = 0usize;
    if data_row0 <= data_row1 {
        for r in data_row0..=data_row1 {
            if record_count >= MAX_CACHE_RECORDS {
                break;
            }
            record_count += 1;
            records_xml.push_str("<r>");
            for (c_off, shared) in cols.iter_mut().enumerate() {
                let txt = cell_index
                    .cell_display_text(src_sheet, r, src_box.col0 + c_off as u32, caches)
                    .unwrap_or_default();
                let cell = CacheCell::from_raw(&txt);
                match shared.push_cell(&cell) {
                    Some(idx) => {
                        let _ = write!(records_xml, "<x v=\"{idx}\"/>");
                    }
                    None => cell.write_xml(&mut records_xml),
                }
            }
            records_xml.push_str("</r>");
        }
    }

    let full_records_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><pivotCacheRecords xmlns=\"{SPREADSHEET_NS}\" xmlns:r=\"{REL_NS}\" count=\"{record_count}\">{records_xml}</pivotCacheRecords>"
    );
    XmlSanitizer::validate_safe_xml(&full_records_xml)?;

    let mut def_xml = String::with_capacity(1024 + col_count * 128);
    let _ = write!(
        def_xml,
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><pivotCacheDefinition xmlns=\"{SPREADSHEET_NS}\" xmlns:r=\"{REL_NS}\" r:id=\"rIdPCR1\" refreshOnLoad=\"1\" saveData=\"1\" createdVersion=\"3\" refreshedVersion=\"3\" minRefreshableVersion=\"3\" recordCount=\"{record_count}\"><cacheSource type=\"worksheet\"><worksheetSource ref=\"{}\" sheet=\"{}\"/></cacheSource><cacheFields count=\"{col_count}\">",
        XmlSanitizer::escape(&src_box.to_a1()),
        XmlSanitizer::escape(src_sheet),
    );
    for (fname, shared) in field_names.iter().zip(&cols) {
        shared.write_cache_field_xml(fname, &mut def_xml);
    }
    def_xml.push_str("</cacheFields></pivotCacheDefinition>");
    XmlSanitizer::validate_safe_xml(&def_xml)?;

    let cache_rels = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{PKG_RELS_NS}\"><Relationship Id=\"rIdPCR1\" Type=\"{PIVOT_CACHE_REC_REL}\" Target=\"pivotCacheRecords{cid}.xml\"/></Relationships>"
    );
    XmlSanitizer::validate_safe_xml(&cache_rels)?;
    parts.push((
        format!("xl/pivotCache/pivotCacheDefinition{cid}.xml"),
        def_xml.into_bytes(),
    ));
    parts.push((
        format!("xl/pivotCache/_rels/pivotCacheDefinition{cid}.xml.rels"),
        cache_rels.into_bytes(),
    ));
    parts.push((
        format!("xl/pivotCache/pivotCacheRecords{cid}.xml"),
        full_records_xml.into_bytes(),
    ));
    let _ = write!(
        content_types,
        "<Override PartName=\"/xl/pivotCache/pivotCacheDefinition{cid}.xml\" ContentType=\"{PIVOT_CACHE_DEF_CT}\"/><Override PartName=\"/xl/pivotCache/pivotCacheRecords{cid}.xml\" ContentType=\"{PIVOT_CACHE_REC_CT}\"/>"
    );
    Ok(field_names)
}

fn push_field_list(xml: &mut String, tag: &str, fields: &[u32]) {
    if fields.is_empty() {
        return;
    }
    let _ = write!(xml, "<{tag} count=\"{}\">", fields.len());
    for &col in fields {
        let _ = write!(xml, "<field x=\"{col}\"/>");
    }
    let _ = write!(xml, "</{tag}>");
}

fn build_pivot_table_xml(
    pivot: &SheetPivotTable,
    cache_id: u32,
    resolved_names: &[String],
) -> String {
    let f_count = resolved_names.len().max(pivot.field_names.len()).max(1);
    let mut xml = String::with_capacity(2048);
    let _ = write!(
        xml,
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><pivotTableDefinition xmlns=\"{SPREADSHEET_NS}\" name=\"{}\" cacheId=\"{cache_id}\" dataCaption=\"값\" updatedVersion=\"3\" minRefreshableVersion=\"3\" createdVersion=\"3\" useAutoFormatting=\"1\" applyNumberFormats=\"0\" applyWidthHeightFormats=\"1\" outline=\"0\" compact=\"0\" compactData=\"0\"><location ref=\"{}\" firstHeaderRow=\"1\" firstDataRow=\"{}\" firstDataCol=\"{}\"/><pivotFields count=\"{f_count}\">",
        XmlSanitizer::escape(&pivot.name),
        pivot.location.to_a1(),
        1 + pivot.col_fields.len(),
        pivot.row_fields.len().max(1),
    );
    for idx in 0..f_count {
        let col = idx as u32;
        let axis = if pivot.row_fields.contains(&col) {
            Some("axisRow")
        } else if pivot.col_fields.contains(&col) {
            Some("axisCol")
        } else if pivot.filter_fields.contains(&col) {
            Some("axisPage")
        } else {
            None
        };
        xml.push_str("<pivotField");
        if let Some(ax) = axis {
            let _ = write!(xml, " axis=\"{ax}\"");
        }
        if pivot.value_fields.iter().any(|v| v.col == col) {
            xml.push_str(" dataField=\"1\"");
        }
        if axis.is_some() {
            xml.push_str(
                " showAll=\"0\"><items count=\"1\"><item t=\"default\"/></items></pivotField>",
            );
        } else {
            xml.push_str(" showAll=\"0\"/>");
        }
    }
    xml.push_str("</pivotFields>");
    push_field_list(&mut xml, "rowFields", &pivot.row_fields);
    push_field_list(&mut xml, "colFields", &pivot.col_fields);
    if !pivot.filter_fields.is_empty() {
        let _ = write!(xml, "<pageFields count=\"{}\">", pivot.filter_fields.len());
        for &col in &pivot.filter_fields {
            let _ = write!(xml, "<pageField fld=\"{col}\" hier=\"-1\"/>");
        }
        xml.push_str("</pageFields>");
    }
    if !pivot.value_fields.is_empty() {
        let _ = write!(xml, "<dataFields count=\"{}\">", pivot.value_fields.len());
        for vf in &pivot.value_fields {
            let name = if vf.name.trim().is_empty() {
                resolved_names
                    .get(vf.col as usize)
                    .cloned()
                    .unwrap_or_else(|| format!("Value{}", vf.col + 1))
            } else {
                vf.name.clone()
            };
            let _ = write!(
                xml,
                "<dataField name=\"{}\" fld=\"{}\" subtotal=\"{}\" baseField=\"0\" baseItem=\"0\"/>",
                XmlSanitizer::escape(&name), vf.col, vf.agg.ooxml_subtotal(),
            );
        }
        xml.push_str("</dataFields>");
    }
    xml.push_str("<pivotTableStyleInfo name=\"PivotStyleLight16\" showRowHeaders=\"1\" showColHeaders=\"1\" showRowStripes=\"0\" showColStripes=\"0\" showLastColumn=\"1\"/>");

    if pivot.source_table.is_some()
        || !pivot.has_header
        || !pivot.filters.is_empty()
        || !pivot.slicers.is_empty()
    {
        let _ = write!(
            xml,
            "<extLst><ext uri=\"{EO_PIVOT_EXT_URI}\" xmlns:eo=\"{EO_PIVOT_NS}\"><eo:pivot hasHeader=\"{}\"",
            u8::from(pivot.has_header)
        );
        if let Some(tbl) = pivot.source_table.as_deref() {
            let _ = write!(xml, " sourceTable=\"{}\"", XmlSanitizer::escape(tbl));
        }
        xml.push('>');
        for f in &pivot.filters {
            let _ = write!(
                xml,
                "<eo:filter col=\"{}\" blank=\"{}\">",
                f.col,
                u8::from(f.blank)
            );
            for v in &f.values {
                let _ = write!(xml, "<eo:val>{}</eo:val>", XmlSanitizer::escape(v));
            }
            xml.push_str("</eo:filter>");
        }
        for s in &pivot.slicers {
            let _ = write!(
                xml,
                "<eo:slicer col=\"{}\" caption=\"{}\" left=\"{}\" top=\"{}\" width=\"{}\" height=\"{}\" multi=\"{}\"/>",
                s.col, XmlSanitizer::escape(&s.caption), s.left, s.top, s.width, s.height, u8::from(s.multi_select),
            );
        }
        xml.push_str("</eo:pivot></ext></extLst>");
    }
    xml.push_str("</pivotTableDefinition>");
    xml
}

// ---- Reading ------------------------------------------------------------------------------------------

#[derive(Clone, Default)]
struct ParsedCacheDef {
    source_sheet: String,
    source_range: String,
    source_table: Option<String>,
    field_names: Vec<String>,
}

enum ParseOutcome<T> {
    Ok(T),
    Corrupt,
    Budget(EasyOfficeError),
}

fn budget_err(msg: impl Into<String>) -> EasyOfficeError {
    EasyOfficeError::ResourceBudgetExceeded(msg.into())
}

fn rels_of_part(
    entries: &HashMap<&str, &[u8]>,
    part_path: &str,
    wanted_type: &str,
) -> Vec<(String, String)> {
    let (dir, file) = part_path.rsplit_once('/').unwrap_or(("", part_path));
    let rels_path = if dir.is_empty() {
        format!("_rels/{file}.rels")
    } else {
        format!("{dir}/_rels/{file}.rels")
    };
    let Some(xml) = entries
        .get(rels_path.as_str())
        .and_then(|d| std::str::from_utf8(d).ok())
    else {
        return Vec::new();
    };
    let mut reader = XmlReader::with_budget(xml, MAX_PIVOT_XML_EVENTS);
    let mut out = Vec::new();
    while let Ok(Some(ev)) = reader.next_event() {
        let XmlEvent::Start { name, attrs } = ev else {
            continue;
        };
        if local_name(name) != "Relationship" {
            continue;
        }
        let (mut ty_ok, mut id, mut target) = (false, None, None);
        for (k, v) in attrs.iter_raw() {
            match k {
                "Type" => ty_ok = v == wanted_type,
                "Id" => id = Some(unescape_cow(v)),
                "Target" => target = Some(unescape_cow(v)),
                _ => {}
            }
        }
        if ty_ok {
            if let (Some(id), Some(target)) = (id, target) {
                out.push((id.into_owned(), target.into_owned()));
            }
        }
    }
    out
}

fn parse_workbook_pivot_caches(
    wb_xml: &str,
    entries: &HashMap<&str, &[u8]>,
) -> HashMap<u32, String> {
    let wb_rels: HashMap<String, String> =
        rels_of_part(entries, "xl/workbook.xml", PIVOT_CACHE_DEF_REL)
            .into_iter()
            .collect();
    let mut out = HashMap::new();
    let mut reader = XmlReader::with_budget(wb_xml, MAX_PIVOT_XML_EVENTS);
    while let Ok(Some(ev)) = reader.next_event() {
        let XmlEvent::Start { name, attrs } = ev else {
            continue;
        };
        if local_name(name) != "pivotCache" {
            continue;
        }
        let cid = attrs
            .get("cacheId")
            .and_then(|s| s.trim().parse::<u32>().ok())
            .filter(|&c| c > 0);
        let rid = attrs.get("r:id").or_else(|| attrs.get("id"));
        if let (Some(cid), Some(rid)) = (cid, rid) {
            if let Some(path) = wb_rels
                .get(rid.as_ref())
                .and_then(|t| resolve_zip_target("xl", t))
            {
                out.insert(cid, path);
            }
        }
    }
    out
}

fn parse_cache_definition(xml: &str) -> ParseOutcome<ParsedCacheDef> {
    let mut reader = XmlReader::with_budget(xml, MAX_PIVOT_XML_EVENTS);
    let mut def = ParsedCacheDef::default();
    let mut saw_root = false;
    let mut field_name_bytes = 0usize;
    loop {
        let ev = match reader.next_event() {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(e @ EasyOfficeError::ResourceBudgetExceeded(_)) => return ParseOutcome::Budget(e),
            Err(_) => return ParseOutcome::Corrupt,
        };
        let XmlEvent::Start { name, attrs } = ev else {
            continue;
        };
        match local_name(name) {
            "pivotCacheDefinition" => saw_root = true,
            "worksheetSource" => {
                if let Some(sh) = attrs.get("sheet") {
                    if sh.len() > MAX_PIVOT_TEXT_BYTES {
                        return ParseOutcome::Budget(budget_err(
                            "피벗 캐시 시트 이름이 길이 상한을 초과했습니다.",
                        ));
                    }
                    def.source_sheet = sh.into_owned();
                }
                if let Some(rf) = attrs.get("ref") {
                    if rf.len() > MAX_PIVOT_TEXT_BYTES {
                        return ParseOutcome::Budget(budget_err(
                            "피벗 캐시 범위 문자열이 길이 상한을 초과했습니다.",
                        ));
                    }
                    if let Some((_, b)) = parse_sheet_and_box(&def.source_sheet, &rf) {
                        def.source_range = b.to_a1();
                    }
                }
                if let Some(tbl) = attrs.get("name").filter(|s| !s.trim().is_empty()) {
                    if tbl.len() > MAX_PIVOT_TEXT_BYTES {
                        return ParseOutcome::Budget(budget_err(
                            "피벗 캐시 표 이름이 길이 상한을 초과했습니다.",
                        ));
                    }
                    def.source_table = Some(tbl.into_owned());
                }
            }
            "cacheField" => {
                if def.field_names.len() >= OoxmlCodec::MAX_XLSX_COLS as usize {
                    return ParseOutcome::Budget(budget_err(
                        "피벗 캐시 필드 수가 최대 열 수를 초과했습니다.",
                    ));
                }
                if let Some(fname) = attrs.get("name") {
                    if fname.len() > MAX_PIVOT_TEXT_BYTES {
                        return ParseOutcome::Budget(budget_err(
                            "피벗 캐시 필드 이름이 길이 상한을 초과했습니다.",
                        ));
                    }
                    field_name_bytes += fname.len();
                    if field_name_bytes > MAX_PIVOT_FIELD_NAMES_BYTES {
                        return ParseOutcome::Budget(budget_err(
                            "피벗 캐시 필드 이름 총 길이가 상한을 초과했습니다.",
                        ));
                    }
                    def.field_names.push(fname.into_owned());
                }
            }
            _ => {}
        }
    }
    if saw_root {
        ParseOutcome::Ok(def)
    } else {
        ParseOutcome::Corrupt
    }
}

fn parse_eo_slicer_attrs(attrs: &Attrs<'_>) -> Option<PivotSlicer> {
    let u32_of = |k: &str| attrs.get(k)?.trim().parse::<u32>().ok();
    let s = PivotSlicer {
        col: u32_of("col")?,
        caption: attrs.get("caption")?.into_owned(),
        left: u32_of("left")?,
        top: u32_of("top")?,
        width: u32_of("width")?,
        height: u32_of("height")?,
        multi_select: matches!(attrs.get("multi").as_deref(), Some("1" | "true")),
    };
    let ok = s.left <= MAX_SLICER_POS_PX
        && s.top <= MAX_SLICER_POS_PX
        && (MIN_SLICER_DIM_PX..=MAX_SLICER_DIM_PX).contains(&s.width)
        && (MIN_SLICER_DIM_PX..=MAX_SLICER_DIM_PX).contains(&s.height)
        && !s.caption.trim().is_empty();
    ok.then_some(s)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AxisSection {
    None,
    Row,
    Col,
}

fn parse_pivot_table_xml(xml: &str) -> ParseOutcome<SheetPivotTable> {
    let mut reader = XmlReader::with_budget(xml, MAX_PIVOT_XML_EVENTS);
    let mut pt = SheetPivotTable::default();
    let (mut saw_root, mut saw_location, mut in_eo_pivot, mut in_eo_val) =
        (false, false, false, false);
    let mut axis_sec = AxisSection::None;
    let mut cur_filter: Option<PivotFilterCriteria> = None;
    let mut val_buf = String::new();

    loop {
        let ev = match reader.next_event() {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(e @ EasyOfficeError::ResourceBudgetExceeded(_)) => return ParseOutcome::Budget(e),
            Err(_) => return ParseOutcome::Corrupt,
        };
        match ev {
            XmlEvent::Start { name, attrs } => match local_name(name) {
                "pivotTableDefinition" if !saw_root => {
                    saw_root = true;
                    let Some(pt_name) = attrs.get("name").filter(|s| !s.trim().is_empty()) else {
                        return ParseOutcome::Corrupt;
                    };
                    if pt_name.len() > MAX_PIVOT_TEXT_BYTES {
                        return ParseOutcome::Budget(budget_err(format!(
                            "피벗 이름이 상한({MAX_PIVOT_TEXT_BYTES}B)을 초과했습니다."
                        )));
                    }
                    let Some(cid) = attrs
                        .get("cacheId")
                        .and_then(|s| s.trim().parse::<u32>().ok())
                        .filter(|&c| c > 0)
                    else {
                        return ParseOutcome::Corrupt;
                    };
                    pt.name = pt_name.into_owned();
                    pt.cache_id = cid;
                }
                "location" if saw_root => {
                    let Some(loc_box) = attrs.get("ref").and_then(|r| CellBox::parse(&r)) else {
                        return ParseOutcome::Corrupt;
                    };
                    pt.location = loc_box;
                    saw_location = true;
                }
                "rowFields" if saw_root => axis_sec = AxisSection::Row,
                "colFields" if saw_root => axis_sec = AxisSection::Col,
                "field" if axis_sec != AxisSection::None => {
                    if let Some(Ok(col)) = attrs
                        .get("x")
                        .and_then(|s| s.trim().parse::<i32>().ok())
                        .map(u32::try_from)
                    {
                        let target = if axis_sec == AxisSection::Row {
                            &mut pt.row_fields
                        } else {
                            &mut pt.col_fields
                        };
                        if target.len() >= MAX_PIVOT_AXIS_FIELDS {
                            return ParseOutcome::Budget(budget_err(format!(
                                "피벗 축 필드 수가 상한({MAX_PIVOT_AXIS_FIELDS})을 초과했습니다."
                            )));
                        }
                        target.push(col);
                    }
                }
                "pageField" if saw_root => {
                    if let Some(col) = attrs.get("fld").and_then(|s| s.trim().parse::<u32>().ok()) {
                        if pt.filter_fields.len() >= MAX_PIVOT_AXIS_FIELDS {
                            return ParseOutcome::Budget(budget_err(format!(
                                "피벗 필터 필드 수가 상한({MAX_PIVOT_AXIS_FIELDS})을 초과했습니다."
                            )));
                        }
                        pt.filter_fields.push(col);
                    }
                }
                "dataField" if saw_root => {
                    if pt.value_fields.len() >= MAX_PIVOT_VALUE_FIELDS {
                        return ParseOutcome::Budget(budget_err(format!(
                            "피벗 값 필드 수가 상한({MAX_PIVOT_VALUE_FIELDS})을 초과했습니다."
                        )));
                    }
                    let Some(col) = attrs.get("fld").and_then(|s| s.trim().parse::<u32>().ok())
                    else {
                        return ParseOutcome::Corrupt;
                    };
                    let agg = match attrs.get("subtotal") {
                        None => PivotAggKind::Sum,
                        Some(s) => match PivotAggKind::from_ooxml(&s) {
                            Some(a) => a,
                            None => return ParseOutcome::Corrupt,
                        },
                    };
                    let fname = attrs
                        .get("name")
                        .map_or_else(String::new, |s| s.into_owned());
                    if fname.len() > MAX_PIVOT_TEXT_BYTES {
                        return ParseOutcome::Budget(budget_err(format!(
                            "피벗 값 필드 이름이 상한({MAX_PIVOT_TEXT_BYTES}B)을 초과했습니다."
                        )));
                    }
                    pt.value_fields.push(PivotValueField {
                        col,
                        agg,
                        name: fname,
                    });
                }
                "pivot" if saw_root && name.starts_with("eo:") => {
                    in_eo_pivot = true;
                    if let Some(hh) = attrs.get("hasHeader") {
                        pt.has_header = matches!(hh.as_ref(), "1" | "true");
                    }
                    if let Some(tbl) = attrs.get("sourceTable").filter(|s| !s.trim().is_empty()) {
                        if tbl.len() > MAX_PIVOT_TEXT_BYTES {
                            return ParseOutcome::Budget(budget_err(format!(
                                "피벗 표 이름이 상한({MAX_PIVOT_TEXT_BYTES}B)을 초과했습니다."
                            )));
                        }
                        pt.source_table = Some(tbl.into_owned());
                    }
                }
                "filter" if in_eo_pivot && name.starts_with("eo:") => {
                    if pt.filters.len() >= MAX_PIVOT_FILTERS {
                        return ParseOutcome::Budget(budget_err(format!(
                            "피벗 필터 수가 상한({MAX_PIVOT_FILTERS})을 초과했습니다."
                        )));
                    }
                    let Some(col) = attrs.get("col").and_then(|s| s.trim().parse::<u32>().ok())
                    else {
                        return ParseOutcome::Corrupt;
                    };
                    cur_filter = Some(PivotFilterCriteria {
                        col,
                        values: Vec::new(),
                        blank: matches!(attrs.get("blank").as_deref(), Some("1" | "true")),
                    });
                }
                "val" if cur_filter.is_some() && name.starts_with("eo:") => {
                    in_eo_val = true;
                    val_buf.clear();
                }
                "slicer" if in_eo_pivot && name.starts_with("eo:") => {
                    if pt.slicers.len() >= MAX_PIVOT_SLICERS {
                        return ParseOutcome::Budget(budget_err(format!(
                            "피벗 슬라이서 수가 상한({MAX_PIVOT_SLICERS})을 초과했습니다."
                        )));
                    }
                    if let Some(s) = parse_eo_slicer_attrs(&attrs) {
                        if s.caption.len() > MAX_PIVOT_TEXT_BYTES {
                            return ParseOutcome::Budget(budget_err(format!("피벗 슬라이서 캡션이 상한({MAX_PIVOT_TEXT_BYTES}B)을 초과했습니다.")));
                        }
                        pt.slicers.push(s);
                    }
                }
                _ => {}
            },
            XmlEvent::Text(t) if in_eo_val => {
                if val_buf.len() + t.len() > MAX_PIVOT_TEXT_BYTES {
                    return ParseOutcome::Budget(budget_err(format!(
                        "피벗 필터 값이 상한({MAX_PIVOT_TEXT_BYTES}B)을 초과했습니다."
                    )));
                }
                val_buf.push_str(&t);
            }
            XmlEvent::End(name) => match local_name(name) {
                "rowFields" | "colFields" => axis_sec = AxisSection::None,
                "pivot" if in_eo_pivot && name.starts_with("eo:") => in_eo_pivot = false,
                "val" if in_eo_val => {
                    in_eo_val = false;
                    if let Some(f) = cur_filter.as_mut() {
                        if f.values.len() >= MAX_PIVOT_FILTER_VALUES {
                            return ParseOutcome::Budget(budget_err(format!("피벗 필터 항목 수가 상한({MAX_PIVOT_FILTER_VALUES})을 초과했습니다.")));
                        }
                        f.values.push(std::mem::take(&mut val_buf));
                    }
                }
                "filter" if cur_filter.is_some() && name.starts_with("eo:") => {
                    if let Some(f) = cur_filter.take() {
                        pt.filters.push(f);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    if saw_root && saw_location {
        ParseOutcome::Ok(pt)
    } else {
        ParseOutcome::Corrupt
    }
}

/// Reads pivot definitions across all sheets in the workbook after sheet tables are loaded.
pub(crate) fn read_workbook_pivots(
    entries: &HashMap<&str, &[u8]>,
    wb_xml: &str,
    sheet_part_paths: &[&str],
    stylings: &mut [(String, SheetStyling)],
) -> Result<(), EasyOfficeError> {
    let wb_cache_paths = parse_workbook_pivot_caches(wb_xml, entries);
    let mut parsed_caches: HashMap<u32, ParsedCacheDef> = HashMap::new();
    for (&cid, path) in &wb_cache_paths {
        if parsed_caches.len() >= MAX_WORKBOOK_PIVOTS {
            break;
        }
        if let Some(xml) = entries
            .get(path.as_str())
            .and_then(|d| std::str::from_utf8(d).ok())
        {
            match parse_cache_definition(xml) {
                ParseOutcome::Ok(def) => {
                    parsed_caches.insert(cid, def);
                }
                ParseOutcome::Budget(e) => return Err(e),
                ParseOutcome::Corrupt => {}
            }
        }
    }

    let table_lookup: HashMap<String, (String, CellBox)> = stylings
        .iter()
        .flat_map(|(sh, st)| {
            st.tables
                .iter()
                .map(move |t| (fold_name(&t.name), (sh.clone(), t.range)))
        })
        .collect();

    let mut workbook_total = 0usize;
    let mut workbook_field_name_bytes = 0usize;
    for (s_idx, &part_path) in sheet_part_paths.iter().enumerate() {
        let rels = rels_of_part(entries, part_path, PIVOT_TABLE_REL);
        if rels.is_empty() {
            continue;
        }
        if rels.len() > MAX_PIVOTS_PER_SHEET {
            return Err(budget_err(format!(
                "시트 피벗 테이블 수({})가 허용 상한({MAX_PIVOTS_PER_SHEET})을 초과했습니다.",
                rels.len()
            )));
        }
        if workbook_total + rels.len() > MAX_WORKBOOK_PIVOTS {
            return Err(budget_err(format!("통합 문서 전체 피벗 테이블 수({})가 허용 상한({MAX_WORKBOOK_PIVOTS})을 초과했습니다.", workbook_total + rels.len())));
        }
        workbook_total += rels.len();

        let sheet_dir = part_path.rsplit_once('/').map_or("", |(d, _)| d);
        let mut seen_paths = HashSet::new();
        let mut rejected = 0u32;
        for (_, target) in rels {
            let Some(pt_path) = resolve_zip_target(sheet_dir, &target) else {
                rejected = rejected.saturating_add(1);
                continue;
            };
            if !seen_paths.insert(pt_path.clone()) {
                continue;
            }
            let Some(pt_xml) = entries
                .get(pt_path.as_str())
                .and_then(|d| std::str::from_utf8(d).ok())
            else {
                rejected = rejected.saturating_add(1);
                continue;
            };
            let mut pt = match parse_pivot_table_xml(pt_xml) {
                ParseOutcome::Ok(p) => p,
                ParseOutcome::Budget(e) => return Err(e),
                ParseOutcome::Corrupt => {
                    rejected = rejected.saturating_add(1);
                    continue;
                }
            };
            let cache_def = parsed_caches.get(&pt.cache_id).cloned().or_else(|| {
                let pt_dir = pt_path.rsplit_once('/').map_or("", |(d, _)| d);
                rels_of_part(entries, &pt_path, PIVOT_CACHE_DEF_REL)
                    .into_iter()
                    .next()
                    .and_then(|(_, t)| resolve_zip_target(pt_dir, &t))
                    .and_then(|p| entries.get(p.as_str()).copied())
                    .and_then(|d| std::str::from_utf8(d).ok())
                    .and_then(|xml| match parse_cache_definition(xml) {
                        ParseOutcome::Ok(def) => Some(def),
                        _ => None,
                    })
            });
            let Some(mut cache_def) = cache_def else {
                rejected = rejected.saturating_add(1);
                continue;
            };
            if cache_def.source_range.is_empty() {
                if let Some(tbl_name) = pt
                    .source_table
                    .as_deref()
                    .or(cache_def.source_table.as_deref())
                {
                    if let Some((sh, range)) = table_lookup.get(&fold_name(tbl_name)) {
                        cache_def.source_sheet = sh.clone();
                        cache_def.source_range = range.to_a1();
                    }
                }
            }
            pt.source_sheet = cache_def.source_sheet;
            pt.source_range = cache_def.source_range;
            if pt.source_table.is_none() {
                pt.source_table = cache_def.source_table;
            }
            pt.field_names = cache_def.field_names;
            workbook_field_name_bytes += pt.field_name_bytes();
            if workbook_field_name_bytes > MAX_WORKBOOK_PIVOT_FIELD_NAMES_BYTES {
                return Err(budget_err(format!(
                    "통합 문서 전체 피벗 필드 이름 총 길이가 상한({MAX_WORKBOOK_PIVOT_FIELD_NAMES_BYTES}B)을 초과했습니다."
                )));
            }
            for vf in &mut pt.value_fields {
                if vf.name.trim().is_empty() {
                    vf.name = pt
                        .field_names
                        .get(vf.col as usize)
                        .cloned()
                        .unwrap_or_else(|| format!("Value{}", vf.col + 1));
                }
            }
            match pt.check_budgets() {
                Ok(()) => stylings[s_idx].1.pivots.push(pt),
                Err(EasyOfficeError::ResourceBudgetExceeded(m)) => {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(m))
                }
                Err(_) => rejected = rejected.saturating_add(1),
            }
        }
        stylings[s_idx].1.pivots_rejected = rejected;
    }

    crate::sheet_styling::xlsx_slicers::read_pivot_slicer_parts(entries, stylings);
    Ok(())
}
