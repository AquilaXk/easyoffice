//! Workbook theme XML (`xl/theme/theme1.xml`, ECMA-376 Part 1 §20.1.6) and table slicers (PD-242):
//! lossless `<extLst>` round-trip inside `xl/tables/tableN.xml` (ECMA-376 §18.5.1.2) plus reading
//! native `[MS-XLSX]` `xl/slicerCaches/slicerCache*.xml` (`CT_TableSlicerCache`) and
//! `xl/slicers/slicer*.xml` (`CT_Slicer`).

use std::collections::HashMap;
use std::fmt::Write;

use crate::sheet_styling::sheet_table::{SheetTable, MAX_TABLE_TEXT_CHARS};
use crate::sheet_styling::SheetStyling;
use crate::xml_stream::{Attrs, XmlEvent, XmlReader};
use crate::XmlSanitizer;

/// Slicers one table may hold.
pub const MAX_SLICERS_PER_TABLE: usize = 64;
/// Slicers all tables of a workbook may hold together.
pub const MAX_SLICERS_TOTAL: usize = 256;
pub const MIN_SLICER_DIM_PX: u32 = 80;
pub const MAX_SLICER_DIM_PX: u32 = 2_000;
pub const MAX_SLICER_POS_PX: u32 = 65_535;
const DEFAULT_SLICER_W_PX: u32 = 184;
const DEFAULT_SLICER_H_PX: u32 = 210;
const EMU_PER_PX: u64 = 9_525;
const SLICER_ROW_HEIGHT_EMU: u64 = 228_600;
const MAX_SLICER_XML_EVENTS: usize = 16_384;

pub(crate) const SLICER_EXT_URI: &str = "{E0511CE5-2420-4E05-9A11-511CE5000242}";
const SLICER_EO_NS: &str = "urn:easyoffice:spreadsheet:slicers:1.0";
const SLICER_MAIN_2010_NS: &str = concat!(
    "http://schemas.micro",
    "soft.com/office/spreadsheetml/2009/9/main"
);
const SLICER_MAIN_2013_NS: &str = concat!(
    "http://schemas.micro",
    "soft.com/office/spreadsheetml/2010/11/main"
);
const SLICER_CACHE_REL_TYPE: &str = concat!(
    "http://schemas.micro",
    "soft.com/office/2007/relationships/slicerCache"
);
const SLICER_VIEW_REL_TYPE: &str = concat!(
    "http://schemas.micro",
    "soft.com/office/2007/relationships/slicer"
);
const SLICER_CACHE_CONTENT_TYPE: &str = concat!("application/vnd.ms-ex", "cel.slicerCache+xml");
const SLICER_VIEW_CONTENT_TYPE: &str = concat!("application/vnd.ms-ex", "cel.slicer+xml");
const WB_SLICER_CACHES_EXT_URI: &str = "{B58B0392-4F1F-4190-BB64-5DF3571DCE5F}";
const WS_SLICERS_EXT_URI: &str = "{A8765BA9-456A-4DAB-B4F3-ACF838C121DE}";
const TABLE_SLICER_CACHE_EXT_URI: &str = "{2F2917AC-EB37-4324-AD4E-5DD8C200BD13}";
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub(crate) const THEME_CONTENT_OVERRIDE: &str =
    "<Override PartName=\"/xl/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>";
pub(crate) const THEME_WORKBOOK_REL: &str =
    "<Relationship Id=\"rIdTheme\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"theme/theme1.xml\"/>";

/// One visual filter slicer bound to a zero-based column offset of its parent [`crate::SheetTable`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSlicer {
    pub col: u32,
    pub caption: String,
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    pub multi_select: bool,
}

impl TableSlicer {
    pub fn within_bounds(&self, table_width: u32) -> bool {
        let len = self.caption.chars().count();
        self.col < table_width
            && (1..=MAX_TABLE_TEXT_CHARS).contains(&len)
            && self.left <= MAX_SLICER_POS_PX
            && self.top <= MAX_SLICER_POS_PX
            && (MIN_SLICER_DIM_PX..=MAX_SLICER_DIM_PX).contains(&self.width)
            && (MIN_SLICER_DIM_PX..=MAX_SLICER_DIM_PX).contains(&self.height)
    }
}

/// Appends `<extLst>` for `slicers` at the end of `<table>` (ECMA-376 §18.5.1.2 `CT_Table`).
pub(crate) fn write_table_slicers_ext(out: &mut String, slicers: &[TableSlicer]) {
    if slicers.is_empty() {
        return;
    }
    let _ = write!(
        out,
        "<extLst><ext uri=\"{SLICER_EXT_URI}\"><eo:slicers xmlns:eo=\"{SLICER_EO_NS}\">"
    );
    for s in slicers {
        let _ = write!(
            out,
            "<eo:slicer col=\"{}\" caption=\"{}\" left=\"{}\" top=\"{}\" width=\"{}\" height=\"{}\" multi=\"{}\"/>",
            s.col,
            XmlSanitizer::escape(&s.caption),
            s.left,
            s.top,
            s.width,
            s.height,
            u8::from(s.multi_select),
        );
    }
    out.push_str("</eo:slicers></ext></extLst>");
}

/// Parses one `<eo:slicer .../>` element inside a table's `<extLst>`.
pub(crate) fn parse_eo_slicer(attrs: &Attrs<'_>, max_cols: u32) -> Option<TableSlicer> {
    let u32_attr = |k: &str| attrs.get(k)?.trim().parse::<u32>().ok();
    let slicer = TableSlicer {
        col: u32_attr("col")?,
        caption: attrs.get("caption")?.into_owned(),
        left: u32_attr("left")?,
        top: u32_attr("top")?,
        width: u32_attr("width")?,
        height: u32_attr("height")?,
        multi_select: matches!(attrs.get("multi").as_deref(), Some("1" | "true")),
    };
    slicer.within_bounds(max_cols).then_some(slicer)
}

#[derive(Clone)]
struct SlicerCacheDef {
    table_id: u32,
    /// 1-based `<tableColumn id="...">` in `[MS-XLSX]` `CT_TableSlicerCache`.
    column_id: u32,
    source_name: String,
}

#[derive(Clone)]
struct SlicerViewDef {
    cache: String,
    caption: Option<String>,
    width_px: u32,
    height_px: u32,
}

fn emu_to_px(raw: Option<&str>, fallback: u32) -> u32 {
    let Some(emu) = raw.and_then(|s| s.trim().parse::<u64>().ok()) else {
        return fallback;
    };
    if emu == 0 {
        return fallback;
    }
    let px = emu.saturating_add(EMU_PER_PX / 2) / EMU_PER_PX;
    u32::try_from(px)
        .unwrap_or(u32::MAX)
        .clamp(MIN_SLICER_DIM_PX, MAX_SLICER_DIM_PX)
}

fn parse_slicer_cache_part(xml: &str) -> Option<(String, SlicerCacheDef)> {
    let mut reader = XmlReader::with_budget(xml, MAX_SLICER_XML_EVENTS);
    let mut cache_name: Option<String> = None;
    let mut source_name = String::new();
    let mut table_link: Option<(u32, u32)> = None;
    while let Ok(Some(event)) = reader.next_event() {
        let XmlEvent::Start { name, attrs } = event else {
            continue;
        };
        let local = name.rsplit(':').next().unwrap_or(name);
        match local {
            "slicerCacheDefinition" if cache_name.is_none() => {
                cache_name = attrs.get("name").map(|s| s.into_owned());
                source_name = attrs
                    .get("sourceName")
                    .map(|s| s.into_owned())
                    .unwrap_or_default();
            }
            "tableSlicerCache" => {
                let tid = attrs
                    .get("tableId")
                    .and_then(|s| s.trim().parse::<u32>().ok())?;
                let col = attrs
                    .get("column")
                    .and_then(|s| s.trim().parse::<u32>().ok())?;
                table_link = Some((tid, col));
            }
            _ => {}
        }
    }
    let (table_id, column_id) = table_link?;
    Some((
        cache_name?,
        SlicerCacheDef {
            table_id,
            column_id,
            source_name,
        },
    ))
}

fn parse_slicer_views_part(xml: &str, out: &mut Vec<SlicerViewDef>) {
    let mut reader = XmlReader::with_budget(xml, MAX_SLICER_XML_EVENTS);
    while let Ok(Some(event)) = reader.next_event() {
        if out.len() >= MAX_SLICERS_TOTAL {
            break;
        }
        let XmlEvent::Start { name, attrs } = event else {
            continue;
        };
        let local = name.rsplit(':').next().unwrap_or(name);
        if local != "slicer" {
            continue;
        }
        let Some(cache) = attrs.get("cache").map(|s| s.into_owned()) else {
            continue;
        };
        if cache.is_empty() {
            continue;
        }
        let caption = attrs
            .get("caption")
            .or_else(|| attrs.get("name"))
            .map(|s| s.into_owned())
            .filter(|s| !s.is_empty());
        let width_px = emu_to_px(attrs.get("width").as_deref(), DEFAULT_SLICER_W_PX);
        let height_px = emu_to_px(attrs.get("height").as_deref(), DEFAULT_SLICER_H_PX);
        out.push(SlicerViewDef {
            cache,
            caption,
            width_px,
            height_px,
        });
    }
}

/// Imports native `[MS-XLSX]` `xl/slicerCaches/slicerCache*.xml` + `xl/slicers/slicer*.xml` onto
/// the matching `(sheet_idx, table_idx)` entries in `stylings` when a table has no `<extLst>` slicers.
pub(crate) fn read_table_slicer_parts(
    entries: &HashMap<&str, &[u8]>,
    table_ids: &[(u32, usize, usize)],
    stylings: &mut [(String, SheetStyling)],
) {
    if table_ids.is_empty() {
        return;
    }
    let mut cache_paths: Vec<&str> = entries
        .keys()
        .copied()
        .filter(|p| p.starts_with("xl/slicerCaches/") && p.ends_with(".xml"))
        .collect();
    if cache_paths.is_empty() {
        return;
    }
    cache_paths.sort_unstable();
    cache_paths.truncate(MAX_SLICERS_TOTAL);

    let mut caches: HashMap<String, SlicerCacheDef> = HashMap::new();
    for path in cache_paths {
        let Some(xml) = entries.get(path).and_then(|d| std::str::from_utf8(d).ok()) else {
            continue;
        };
        if let Some((name, cache)) = parse_slicer_cache_part(xml) {
            caches.insert(name, cache);
        }
    }
    if caches.is_empty() {
        return;
    }

    let mut slicer_paths: Vec<&str> = entries
        .keys()
        .copied()
        .filter(|p| p.starts_with("xl/slicers/") && p.ends_with(".xml"))
        .collect();
    slicer_paths.sort_unstable();
    slicer_paths.truncate(MAX_SLICERS_TOTAL);

    let mut views = Vec::new();
    for path in slicer_paths {
        if let Some(xml) = entries.get(path).and_then(|d| std::str::from_utf8(d).ok()) {
            parse_slicer_views_part(xml, &mut views);
        }
    }

    let by_table_id: HashMap<u32, (usize, usize)> = table_ids
        .iter()
        .map(|&(tid, s_idx, t_idx)| (tid, (s_idx, t_idx)))
        .collect();
    let mut total_added = 0usize;
    for view in views {
        if total_added >= MAX_SLICERS_TOTAL {
            break;
        }
        let Some(cache) = caches.get(&view.cache) else {
            continue;
        };
        let Some(&(s_idx, t_idx)) = by_table_id.get(&cache.table_id) else {
            continue;
        };
        let Some((_, st)) = stylings.get_mut(s_idx) else {
            continue;
        };
        let Some(table) = st.tables.get_mut(t_idx) else {
            continue;
        };
        if !table.header || table.slicers.len() >= MAX_SLICERS_PER_TABLE {
            continue;
        }
        let width = table.width();
        let col = if (1..=width).contains(&cache.column_id) {
            cache.column_id - 1
        } else if cache.column_id == 0 && width > 0 {
            0
        } else {
            continue;
        };
        if table.slicers.iter().any(|s| s.col == col) {
            continue;
        }
        let raw_caption = view
            .caption
            .or_else(|| (!cache.source_name.is_empty()).then(|| cache.source_name.clone()))
            .or_else(|| table.columns.get(col as usize).cloned())
            .unwrap_or_else(|| format!("Column{}", col + 1));
        let caption: String = raw_caption.chars().take(MAX_TABLE_TEXT_CHARS).collect();
        let slot = table.slicers.len() as u32;
        let slicer = TableSlicer {
            col,
            caption,
            left: (24 + slot * 200).min(MAX_SLICER_POS_PX),
            top: 24,
            width: view.width_px,
            height: view.height_px,
            multi_select: false,
        };
        if slicer.within_bounds(width) {
            table.slicers.push(slicer);
            total_added += 1;
        }
    }
}

#[derive(Clone)]
struct PivotSlicerCacheDef {
    source_name: String,
    pivot_cache_id: Option<u32>,
    targets: Vec<(Option<u32>, String)>,
}

fn parse_pivot_slicer_cache_part(xml: &str) -> Option<(String, PivotSlicerCacheDef)> {
    let mut reader = XmlReader::with_budget(xml, MAX_SLICER_XML_EVENTS);
    let mut cache_name: Option<String> = None;
    let mut source_name = String::new();
    let mut pivot_cache_id: Option<u32> = None;
    let mut targets: Vec<(Option<u32>, String)> = Vec::new();
    let mut is_table_slicer = false;
    while let Ok(Some(event)) = reader.next_event() {
        let XmlEvent::Start { name, attrs } = event else {
            continue;
        };
        let local = name.rsplit(':').next().unwrap_or(name);
        match local {
            "slicerCacheDefinition" if cache_name.is_none() => {
                cache_name = attrs.get("name").map(|s| s.into_owned());
                source_name = attrs
                    .get("sourceName")
                    .map(|s| s.into_owned())
                    .unwrap_or_default();
            }
            "tableSlicerCache" => is_table_slicer = true,
            "tabular" | "olap" => {
                if let Some(cid) = attrs
                    .get("pivotCacheId")
                    .and_then(|s| s.trim().parse::<u32>().ok())
                {
                    pivot_cache_id = Some(cid);
                }
            }
            "pivotTable" => {
                if let Some(pt_name) = attrs.get("name").filter(|s| !s.trim().is_empty()) {
                    let tab_id = attrs
                        .get("tabId")
                        .and_then(|s| s.trim().parse::<u32>().ok());
                    targets.push((tab_id, pt_name.into_owned()));
                }
            }
            _ => {}
        }
    }
    if is_table_slicer || (targets.is_empty() && pivot_cache_id.is_none()) {
        return None;
    }
    Some((
        cache_name?,
        PivotSlicerCacheDef {
            source_name,
            pivot_cache_id,
            targets,
        },
    ))
}

/// Imports `[MS-XLSX]` pivot slicers (`<tabular>` / `<olap>` + `<pivotTables><pivotTable .../>`)
/// onto matching [`crate::SheetPivotTable`]s when `<extLst>` did not already define them.
pub(crate) fn read_pivot_slicer_parts(
    entries: &HashMap<&str, &[u8]>,
    stylings: &mut [(String, SheetStyling)],
) {
    if stylings.iter().all(|(_, st)| st.pivots.is_empty()) {
        return;
    }
    let mut cache_paths: Vec<&str> = entries
        .keys()
        .copied()
        .filter(|p| p.starts_with("xl/slicerCaches/") && p.ends_with(".xml"))
        .collect();
    if cache_paths.is_empty() {
        return;
    }
    cache_paths.sort_unstable();
    cache_paths.truncate(MAX_SLICERS_TOTAL);

    let mut caches: HashMap<String, PivotSlicerCacheDef> = HashMap::new();
    for path in cache_paths {
        let Some(xml) = entries.get(path).and_then(|d| std::str::from_utf8(d).ok()) else {
            continue;
        };
        if let Some((name, cache)) = parse_pivot_slicer_cache_part(xml) {
            caches.insert(name, cache);
        }
    }
    if caches.is_empty() {
        return;
    }

    let mut slicer_paths: Vec<&str> = entries
        .keys()
        .copied()
        .filter(|p| p.starts_with("xl/slicers/") && p.ends_with(".xml"))
        .collect();
    slicer_paths.sort_unstable();
    slicer_paths.truncate(MAX_SLICERS_TOTAL);

    let mut views = Vec::new();
    for path in slicer_paths {
        if let Some(xml) = entries.get(path).and_then(|d| std::str::from_utf8(d).ok()) {
            parse_slicer_views_part(xml, &mut views);
        }
    }

    let mut total_added = 0usize;
    for view in views {
        if total_added >= MAX_SLICERS_TOTAL {
            break;
        }
        let Some(cache) = caches.get(&view.cache) else {
            continue;
        };
        let target_pos = stylings.iter().enumerate().find_map(|(s_idx, (_, st))| {
            let tab_1 = (s_idx + 1) as u32;
            st.pivots.iter().enumerate().find_map(|(p_idx, pv)| {
                let matched_target = cache.targets.iter().any(|(tid, pt_name)| {
                    tid.is_none_or(|t| t == tab_1) && pv.name.eq_ignore_ascii_case(pt_name)
                });
                let matched_cache =
                    cache.targets.is_empty() && cache.pivot_cache_id == Some(pv.cache_id);
                (matched_target || matched_cache).then_some((s_idx, p_idx))
            })
        });
        let Some((s_idx, p_idx)) = target_pos else {
            continue;
        };
        let pivot = &mut stylings[s_idx].1.pivots[p_idx];
        if pivot.slicers.len() >= crate::MAX_PIVOT_SLICERS {
            continue;
        }
        let src_folded = crate::sheet_names::fold_name(&cache.source_name);
        let Some(col) = pivot
            .field_names
            .iter()
            .position(|f| crate::sheet_names::fold_name(f) == src_folded)
            .map(|c| c as u32)
        else {
            continue;
        };
        if pivot.slicers.iter().any(|s| s.col == col) {
            continue;
        }
        let raw_cap = view
            .caption
            .or_else(|| (!cache.source_name.is_empty()).then(|| cache.source_name.clone()))
            .unwrap_or_else(|| format!("Column{}", col + 1));
        let mut caption = String::new();
        for ch in raw_cap.chars() {
            if caption.len() + ch.len_utf8() > crate::MAX_PIVOT_TEXT_BYTES {
                break;
            }
            caption.push(ch);
        }
        let slot = pivot.slicers.len() as u32;
        let slicer = crate::PivotSlicer {
            col,
            caption,
            left: (24 + slot * 200).min(MAX_SLICER_POS_PX),
            top: 24,
            width: view.width_px,
            height: view.height_px,
            multi_select: false,
        };
        if slicer.check_bounds(pivot.known_source_width()).is_ok() {
            pivot.slicers.push(slicer);
            total_added += 1;
        }
    }
}

/// Builds a compliant DrawingML `xl/theme/theme1.xml` (ECMA-376 Part 1 §20.1.6) from SpreadsheetML's
/// 12 theme slots (`lt1`, `dk1`, `lt2`, `dk2`, `accent1..6`, `hlink`, `folHlink`).
pub(crate) fn build_theme_xml(theme: [u32; 12]) -> String {
    const DRAWING_ORDER: [(&str, usize); 12] = [
        ("dk1", 1),
        ("lt1", 0),
        ("dk2", 3),
        ("lt2", 2),
        ("accent1", 4),
        ("accent2", 5),
        ("accent3", 6),
        ("accent4", 7),
        ("accent5", 8),
        ("accent6", 9),
        ("hlink", 10),
        ("folHlink", 11),
    ];
    let mut xml = String::with_capacity(1_536);
    xml.push_str(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"Office Theme\">",
        "<a:themeElements><a:clrScheme name=\"Office\">",
    ));
    for (tag, idx) in DRAWING_ORDER {
        let rgb = theme[idx] & 0xFF_FFFF;
        let _ = write!(xml, "<a:{tag}><a:srgbClr val=\"{rgb:06X}\"/></a:{tag}>");
    }
    xml.push_str(concat!(
        "</a:clrScheme>",
        "<a:fontScheme name=\"Office\">",
        "<a:majorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont>",
        "<a:minorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont>",
        "</a:fontScheme>",
        "<a:fmtScheme name=\"Office\">",
        "<a:fillStyleLst><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>",
        "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>",
        "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:fillStyleLst>",
        "<a:lnStyleLst><a:ln w=\"6350\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln>",
        "<a:ln w=\"12700\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln>",
        "<a:ln w=\"19050\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln></a:lnStyleLst>",
        "<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle>",
        "<a:effectStyle><a:effectLst/></a:effectStyle>",
        "<a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>",
        "<a:bgFillStyleLst><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>",
        "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>",
        "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:bgFillStyleLst>",
        "</a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>",
    ));
    xml
}

/// Emits standard `[MS-XLSX]` `xl/slicerCaches/slicerCache{k}.xml` (`CT_SlicerCacheDefinition` +
/// `CT_TableSlicerCache`) and `xl/slicers/slicer{m}.xml` (`CT_Slicers`) parts alongside workbook and
/// worksheet `<extLst>` / `.rels` / `[Content_Types].xml` overrides (PD-244).
#[derive(Debug, Default)]
pub(crate) struct SlicerPackageWriter {
    cache_count: usize,
    sheet_view_count: usize,
    cur_sheet_views_xml: String,
    wb_cache_refs: String,
    pub(crate) parts: Vec<(String, Vec<u8>)>,
    pub(crate) content_types: String,
    pub(crate) wb_rels: String,
}

fn escape_slicer_source_name(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\n' | '\r' | '\t' => {
                let _ = write!(out, "_x{:04x}_", c as u32);
            }
            _ => out.push(c),
        }
    }
    out
}

impl SlicerPackageWriter {
    pub(crate) fn add_table(
        &mut self,
        table_id: usize,
        table: &SheetTable,
        resolved_col_names: &[String],
    ) {
        if !table.header || table.slicers.is_empty() {
            return;
        }
        let width = table.width();
        for s in &table.slicers {
            if self.cache_count >= MAX_SLICERS_TOTAL {
                break;
            }
            if !s.within_bounds(width) {
                continue;
            }
            let Some(col_name) = resolved_col_names.get(s.col as usize) else {
                continue;
            };
            self.cache_count += 1;
            let k = self.cache_count;
            let cache_name = format!("Slicer_{k}");
            let source_escaped = XmlSanitizer::escape(&escape_slicer_source_name(col_name));
            let cache_xml = format!(
                concat!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
                    "<slicerCacheDefinition xmlns=\"{main_ns}\" name=\"{cache_name}\" sourceName=\"{source_escaped}\">",
                    "<extLst><ext uri=\"{ext_uri}\" xmlns:x15=\"{x15_ns}\">",
                    "<x15:tableSlicerCache tableId=\"{table_id}\" column=\"{col1}\"/>",
                    "</ext></extLst></slicerCacheDefinition>"
                ),
                main_ns = SLICER_MAIN_2010_NS,
                cache_name = cache_name,
                source_escaped = source_escaped,
                ext_uri = TABLE_SLICER_CACHE_EXT_URI,
                x15_ns = SLICER_MAIN_2013_NS,
                table_id = table_id,
                col1 = s.col + 1,
            );
            self.parts.push((
                format!("xl/slicerCaches/slicerCache{k}.xml"),
                cache_xml.into_bytes(),
            ));
            let _ = write!(
                self.content_types,
                "<Override PartName=\"/xl/slicerCaches/slicerCache{k}.xml\" ContentType=\"{SLICER_CACHE_CONTENT_TYPE}\"/>"
            );
            let _ = write!(
                self.wb_rels,
                "<Relationship Id=\"rIdSc{k}\" Type=\"{SLICER_CACHE_REL_TYPE}\" Target=\"slicerCaches/slicerCache{k}.xml\"/>"
            );
            let _ = write!(self.wb_cache_refs, "<x14:slicerCache r:id=\"rIdSc{k}\"/>");
            let _ = write!(
                self.cur_sheet_views_xml,
                "<slicer name=\"{cache_name}\" cache=\"{cache_name}\" caption=\"{}\" columnCount=\"1\" showCaption=\"1\" style=\"SlicerStyleLight1\" lockedPosition=\"0\" rowHeight=\"{SLICER_ROW_HEIGHT_EMU}\" width=\"{}\" height=\"{}\"/>",
                XmlSanitizer::escape(&s.caption),
                u64::from(s.width) * EMU_PER_PX,
                u64::from(s.height) * EMU_PER_PX,
            );
        }
    }

    pub(crate) fn finish_sheet(&mut self, sheet_parts_xml: &mut String, sheet_rels: &mut String) {
        if self.cur_sheet_views_xml.is_empty() {
            return;
        }
        self.sheet_view_count += 1;
        let m = self.sheet_view_count;
        let views = std::mem::take(&mut self.cur_sheet_views_xml);
        let slicer_xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><slicers xmlns=\"{SLICER_MAIN_2010_NS}\">{views}</slicers>"
        );
        self.parts
            .push((format!("xl/slicers/slicer{m}.xml"), slicer_xml.into_bytes()));
        let _ = write!(
            self.content_types,
            "<Override PartName=\"/xl/slicers/slicer{m}.xml\" ContentType=\"{SLICER_VIEW_CONTENT_TYPE}\"/>"
        );
        let _ = write!(
            sheet_rels,
            "<Relationship Id=\"rIdSl{m}\" Type=\"{SLICER_VIEW_REL_TYPE}\" Target=\"../slicers/slicer{m}.xml\"/>"
        );
        let _ = write!(
            sheet_parts_xml,
            "<extLst><ext uri=\"{WS_SLICERS_EXT_URI}\" xmlns:x14=\"{SLICER_MAIN_2010_NS}\" xmlns:r=\"{REL_NS}\"><x14:slicers><x14:slicer r:id=\"rIdSl{m}\"/></x14:slicers></ext></extLst>"
        );
    }

    pub(crate) fn workbook_ext_xml(&self) -> String {
        if self.wb_cache_refs.is_empty() {
            return String::new();
        }
        format!(
            "<extLst><ext uri=\"{WB_SLICER_CACHES_EXT_URI}\" xmlns:x14=\"{SLICER_MAIN_2010_NS}\"><x14:slicerCaches>{}</x14:slicerCaches></ext></extLst>",
            self.wb_cache_refs
        )
    }
}
