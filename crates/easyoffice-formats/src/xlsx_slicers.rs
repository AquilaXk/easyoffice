//! Workbook theme XML (`xl/theme/theme1.xml`, ECMA-376 Part 1 §20.1.6) and table slicers (PD-242):
//! lossless `<extLst>` round-trip inside `xl/tables/tableN.xml` (ECMA-376 §18.5.1.2) plus reading
//! native `[MS-XLSX]` `xl/slicerCaches/slicerCache*.xml` (`CT_TableSlicerCache`) and
//! `xl/slicers/slicer*.xml` (`CT_Slicer`).

use std::collections::HashMap;
use std::fmt::Write;

use crate::sheet_styling::sheet_table::MAX_TABLE_TEXT_CHARS;
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
const MAX_SLICER_XML_EVENTS: usize = 16_384;

pub(crate) const SLICER_EXT_URI: &str = "{E0511CE5-2420-4E05-9A11-511CE5000242}";
const SLICER_EO_NS: &str = "urn:easyoffice:spreadsheet:slicers:1.0";

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
struct ExcelSlicerCache {
    table_id: u32,
    /// 1-based `<tableColumn id="...">` in `[MS-XLSX]` `CT_TableSlicerCache`.
    column_id: u32,
    source_name: String,
}

#[derive(Clone)]
struct ExcelSlicerView {
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
    let px = ((emu + EMU_PER_PX / 2) / EMU_PER_PX) as u32;
    px.clamp(MIN_SLICER_DIM_PX, MAX_SLICER_DIM_PX)
}

fn parse_slicer_cache_part(xml: &str) -> Option<(String, ExcelSlicerCache)> {
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
        ExcelSlicerCache {
            table_id,
            column_id,
            source_name,
        },
    ))
}

fn parse_slicer_views_part(xml: &str, out: &mut Vec<ExcelSlicerView>) {
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
        out.push(ExcelSlicerView {
            cache,
            caption,
            width_px,
            height_px,
        });
    }
}

/// Imports native `[MS-XLSX]` `xl/slicerCaches/slicerCache*.xml` + `xl/slicers/slicer*.xml` onto
/// the matching `(sheet_idx, table_idx)` entries in `stylings` when a table has no `<extLst>` slicers.
pub(crate) fn read_excel_table_slicers(
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

    let mut caches: HashMap<String, ExcelSlicerCache> = HashMap::new();
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
