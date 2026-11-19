// 참조 구현: jmcnamara/rust_xlsxwriter (MIT OR Apache-2.0, Copyright 2022-2026 John McNamara, src/comment.rs, src/note.rs, src/vml.rs; see THIRD_PARTY_NOTICES.md)

//! Bidirectional OOXML SpreadsheetML cell notes (`xl/comments*.xml` +
//! `xl/drawings/vmlDrawing*.vml`) with packed `u64` indexing and fail-closed budgets (`PD-256`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;

use crate::cell_type::{pack_cell_pos, unpack_cell_pos};
use crate::rtf_xml_helpers::format_col_letters;
use crate::sheet_styling::sheet_media_notes::{
    validate_workbook_notes, SheetCellNote, MAX_NOTES_PER_SHEET, MAX_NOTE_AUTHOR_BYTES,
    MAX_SHEET_NOTE_TEXT_BYTES, MAX_WORKBOOK_NOTES,
};
use crate::sheet_styling::xlsx_charts::local_name;
use crate::sheet_styling::xlsx_parts::resolve_zip_target;
use crate::sheet_styling::SheetStyling;
use crate::xml_stream::{unescape_cow, XmlEvent, XmlReader};
use crate::{CellBox, EasyOfficeError, OoxmlCodec, XmlSanitizer};

const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const SPREADSHEET_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const COMMENTS_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const VML_DRAWING_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing";
const COMMENTS_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml";
const VML_CONTENT_TYPE: &str = "application/vnd.openxmlformats-officedocument.vmlDrawing";
const VML_NS: &str = concat!("urn:schemas-micro", "soft-com:vml");
const OFFICE_NS: &str = concat!("urn:schemas-micro", "soft-com:office:office");
const SHEET_VML_NS: &str = concat!("urn:schemas-micro", "soft-com:office:ex", "cel");

const MAX_COMMENTS_XML_EVENTS: usize = 65_536;
const MAX_VML_XML_EVENTS: usize = 131_072;

/// Workbook-wide comment reading state: the note count and the parts already read. A comments or
/// VML part belongs to one sheet, so a second sheet naming it is turned away unread (PD-262).
#[derive(Default)]
pub(crate) struct WorkbookComments {
    notes: usize,
    parts: HashSet<String>,
}

#[derive(Default)]
pub(crate) struct SheetCommentPart {
    pub(crate) legacy_drawing_xml: String,
    pub(crate) rels: String,
}

/// Generates `xl/comments*.xml` and `xl/drawings/vmlDrawing*.vml` parts for sheets with notes.
pub(crate) struct CommentWriter {
    next_comment_id: usize,
    next_vml_id: usize,
    has_notes: bool,
    pub(crate) parts: Vec<(String, Vec<u8>)>,
    pub(crate) content_types: String,
}

impl CommentWriter {
    pub(crate) fn new(stylings: &[SheetStyling]) -> Result<Self, EasyOfficeError> {
        validate_workbook_notes(stylings)?;
        Ok(Self {
            next_comment_id: 1,
            next_vml_id: 1,
            has_notes: false,
            parts: Vec::new(),
            content_types: String::new(),
        })
    }

    pub(crate) fn default_content_types(&self) -> String {
        if self.has_notes {
            format!("<Default Extension=\"vml\" ContentType=\"{VML_CONTENT_TYPE}\"/>")
        } else {
            String::new()
        }
    }

    pub(crate) fn add_sheet(
        &mut self,
        sheet_idx_1: usize,
        styling: Option<&SheetStyling>,
    ) -> Result<SheetCommentPart, EasyOfficeError> {
        let Some(st) = styling.filter(|s| !s.notes.is_empty()) else {
            return Ok(SheetCommentPart::default());
        };
        self.has_notes = true;
        let c = self.next_comment_id;
        self.next_comment_id += 1;
        let v = self.next_vml_id;
        self.next_vml_id += 1;

        let comments_xml = build_comments_xml(st);
        XmlSanitizer::validate_safe_xml(&comments_xml)?;
        let vml_xml = build_vml_xml(sheet_idx_1, st);
        XmlSanitizer::validate_safe_xml(&vml_xml)?;

        self.parts
            .push((format!("xl/comments{c}.xml"), comments_xml.into_bytes()));
        self.parts.push((
            format!("xl/drawings/vmlDrawing{v}.vml"),
            vml_xml.into_bytes(),
        ));
        let _ = write!(
            self.content_types,
            "<Override PartName=\"/xl/comments{c}.xml\" ContentType=\"{COMMENTS_CONTENT_TYPE}\"/>"
        );
        Ok(SheetCommentPart {
            legacy_drawing_xml: format!("<legacyDrawing xmlns:r=\"{REL_NS}\" r:id=\"rIdV1\"/>"),
            rels: format!(
                "<Relationship Id=\"rIdC1\" Type=\"{COMMENTS_REL_TYPE}\" Target=\"../comments{c}.xml\"/>\
                 <Relationship Id=\"rIdV1\" Type=\"{VML_DRAWING_REL_TYPE}\" Target=\"../drawings/vmlDrawing{v}.vml\"/>"
            ),
        })
    }
}

#[inline]
fn space_attr(text: &str) -> &'static str {
    if text.starts_with(char::is_whitespace)
        || text.ends_with(char::is_whitespace)
        || text.contains(['\n', '\r'])
    {
        " xml:space=\"preserve\""
    } else {
        ""
    }
}

fn build_comments_xml(st: &SheetStyling) -> String {
    let mut authors: Vec<&str> = Vec::new();
    let mut author_ids: HashMap<&str, usize> = HashMap::new();
    for note in st.notes.values() {
        let a = note.author.as_str();
        if !author_ids.contains_key(a) {
            author_ids.insert(a, authors.len());
            authors.push(a);
        }
    }

    let mut xml = String::with_capacity(256 + st.notes.len().saturating_mul(192));
    let _ = write!(
        xml,
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <comments xmlns=\"{SPREADSHEET_NS}\"><authors>"
    );
    for author in &authors {
        let _ = write!(xml, "<author>{}</author>", XmlSanitizer::escape(author));
    }
    xml.push_str("</authors><commentList>");

    for (&packed, note) in &st.notes {
        let (row, col) = unpack_cell_pos(packed);
        let author_id = author_ids[note.author.as_str()];
        let _ = write!(
            xml,
            "<comment ref=\"{}{}\" authorId=\"{author_id}\"><text>",
            format_col_letters(col),
            row + 1
        );
        if !note.author.is_empty() {
            let prefix = format!("{}:", note.author);
            let _ = write!(
                xml,
                "<r><rPr><b/><sz val=\"9\"/><rFont val=\"맑은 고딕\"/></rPr>\
                 <t{}>{}</t></r>",
                space_attr(&prefix),
                XmlSanitizer::escape(&prefix)
            );
            let _ = write!(
                xml,
                "<r><rPr><sz val=\"9\"/><rFont val=\"맑은 고딕\"/></rPr>\
                 <t xml:space=\"preserve\">\n{}</t></r>",
                XmlSanitizer::escape(&note.text)
            );
        } else {
            let _ = write!(
                xml,
                "<r><rPr><sz val=\"9\"/><rFont val=\"맑은 고딕\"/></rPr>\
                 <t{}>{}</t></r>",
                space_attr(&note.text),
                XmlSanitizer::escape(&note.text)
            );
        }
        xml.push_str("</text></comment>");
    }
    xml.push_str("</commentList></comments>");
    xml
}

fn build_vml_xml(sheet_idx_1: usize, st: &SheetStyling) -> String {
    let mut xml = String::with_capacity(512 + st.notes.len().saturating_mul(512));
    let _ = write!(
        xml,
        "<xml xmlns:v=\"{VML_NS}\" xmlns:o=\"{OFFICE_NS}\" xmlns:x=\"{SHEET_VML_NS}\">\
         <o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"{sheet_idx_1}\"/></o:shapelayout>\
         <v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\" path=\"m,l,21600r21600,l21600,xe\">\
         <v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/></v:shapetype>"
    );
    let base_shape_id = (sheet_idx_1 as u32).saturating_mul(1024);
    for (idx, (&packed, note)) in st.notes.iter().enumerate() {
        let (row, col) = unpack_cell_pos(packed);
        let shape_id = base_shape_id.saturating_add(idx as u32 + 1);
        let z = idx + 1;
        let left_pt = f64::from(col + 1) * 60.0 + 11.25;
        let top_pt = f64::from(row) * 18.0 + 1.5;
        let vis = if note.visible { "visible" } else { "hidden" };
        let c1 = (col + 1).min(OoxmlCodec::MAX_XLSX_COLS - 1);
        let r1 = row.saturating_sub(1);
        let c2 = (c1 + 2).min(OoxmlCodec::MAX_XLSX_COLS - 1);
        let r2 = (r1 + 4).min(OoxmlCodec::MAX_XLSX_ROWS - 1);
        let vis_el = if note.visible { "<x:Visible/>" } else { "" };
        let _ = write!(
            xml,
            "<v:shape id=\"_x0000_s{shape_id}\" type=\"#_x0000_t202\" \
             style=\"position:absolute;margin-left:{left_pt:.2}pt;margin-top:{top_pt:.2}pt;\
             width:108pt;height:59.25pt;z-index:{z};visibility:{vis}\" \
             fillcolor=\"#ffffe1\" o:insetmode=\"auto\">\
             <v:fill color2=\"#ffffe1\"/><v:shadow on=\"t\" color=\"black\" obscured=\"t\"/>\
             <v:path o:connecttype=\"none\"/>\
             <v:textbox style=\"mso-direction-alt:auto\"><div style=\"text-align:left\"/></v:textbox>\
             <x:ClientData ObjectType=\"Note\"><x:MoveWithCells/><x:SizeWithCells/>\
             <x:Anchor>{c1}, 15, {r1}, 2, {c2}, 15, {r2}, 4</x:Anchor>\
             <x:AutoFill>False</x:AutoFill><x:Row>{row}</x:Row><x:Column>{col}</x:Column>{vis_el}\
             </x:ClientData></v:shape>"
        );
    }
    xml.push_str("</xml>");
    xml
}

// ---- Reading ------------------------------------------------------------------------------------------

struct SheetCommentRels {
    comments: Vec<Option<String>>,
    vmls: HashMap<String, Option<String>>,
}

fn sheet_comment_rels(
    entries: &HashMap<&str, &[u8]>,
    part_path: &str,
) -> Result<SheetCommentRels, EasyOfficeError> {
    let (dir, file) = part_path.rsplit_once('/').unwrap_or(("", part_path));
    let rels_path = if dir.is_empty() {
        format!("_rels/{file}.rels")
    } else {
        format!("{dir}/_rels/{file}.rels")
    };
    let mut comments = Vec::new();
    let mut vmls = HashMap::new();
    let Some(xml) = entries
        .get(rels_path.as_str())
        .and_then(|d| std::str::from_utf8(d).ok())
    else {
        return Ok(SheetCommentRels { comments, vmls });
    };
    let sheet_dir = part_path.rsplit_once('/').map_or("", |(d, _)| d);
    let mut reader = XmlReader::with_budget(xml, MAX_COMMENTS_XML_EVENTS);
    while let Some(ev) = reader.next_event()? {
        let XmlEvent::Start { name, attrs } = ev else {
            continue;
        };
        if local_name(name) != "Relationship" {
            continue;
        }
        let (mut ty, mut id, mut target, mut external) = (None, None, None, false);
        for (k, v) in attrs.iter_raw() {
            match k {
                "Type" => ty = Some(unescape_cow(v)),
                "Id" => id = Some(unescape_cow(v)),
                "Target" => target = Some(unescape_cow(v)),
                "TargetMode" => external = v.eq_ignore_ascii_case("External"),
                _ => {}
            }
        }
        let (Some(ty), Some(id), Some(target)) = (ty, id, target) else {
            continue;
        };
        if ty == COMMENTS_REL_TYPE {
            if external {
                comments.push(None);
            } else {
                comments
                    .push(resolve_zip_target(sheet_dir, &target).filter(|p| p.starts_with("xl/")));
            }
        } else if ty == VML_DRAWING_REL_TYPE {
            let resolved = if external {
                None
            } else {
                resolve_zip_target(sheet_dir, &target).filter(|p| p.starts_with("xl/"))
            };
            vmls.insert(id.into_owned(), resolved);
        }
    }
    Ok(SheetCommentRels { comments, vmls })
}

fn sheet_legacy_rids(sheet_xml: &str) -> Result<Vec<String>, EasyOfficeError> {
    let tail_start = sheet_xml
        .rfind("</sheetData>")
        .or_else(|| sheet_xml.rfind("<sheetData/>"))
        .unwrap_or(0);
    let tail = &sheet_xml[tail_start..];
    if !tail.contains("legacyDrawing") {
        return Ok(Vec::new());
    }
    let mut reader = XmlReader::with_budget(tail, MAX_COMMENTS_XML_EVENTS);
    let mut ids = Vec::new();
    while let Some(ev) = reader.next_event()? {
        let XmlEvent::Start { name, attrs } = ev else {
            continue;
        };
        if local_name(name) == "legacyDrawing" {
            if let Some(id) = attrs.get("r:id").or_else(|| attrs.get("id")) {
                ids.push(id.into_owned());
            }
        }
    }
    Ok(ids)
}

fn style_has_visible(style: &str) -> bool {
    style
        .split(';')
        .filter_map(|decl| decl.split_once(':'))
        .any(|(k, v)| {
            k.trim().eq_ignore_ascii_case("visibility") && v.trim().eq_ignore_ascii_case("visible")
        })
}

fn parse_vml_visibility(xml: &str, visible_out: &mut HashSet<u64>) -> Result<(), EasyOfficeError> {
    XmlSanitizer::validate_safe_xml(xml)?;
    let mut reader = XmlReader::with_budget(xml, MAX_VML_XML_EVENTS);
    let (mut in_shape, mut in_note_data) = (false, false);
    let mut style_visible = false;
    let mut visible_tag: Option<bool> = None;
    let (mut row, mut col) = (None::<u32>, None::<u32>);
    let mut capture: Option<&str> = None;
    let mut buf = String::new();
    let mut shape_count = 0usize;

    while let Some(ev) = reader.next_event()? {
        match ev {
            XmlEvent::Start { name, attrs } => match local_name(name) {
                "shape" => {
                    shape_count += 1;
                    if shape_count > MAX_NOTES_PER_SHEET * 2 {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "VML 도형 수가 허용 상한({MAX_NOTES_PER_SHEET})을 초과했습니다."
                        )));
                    }
                    in_shape = true;
                    in_note_data = false;
                    style_visible = attrs.get("style").is_some_and(|s| style_has_visible(&s));
                    visible_tag = None;
                    row = None;
                    col = None;
                    capture = None;
                }
                "ClientData" if in_shape => {
                    if attrs
                        .get("ObjectType")
                        .is_some_and(|v| v.eq_ignore_ascii_case("Note"))
                    {
                        in_note_data = true;
                        visible_tag = None;
                        row = None;
                        col = None;
                    }
                }
                "Visible" if in_note_data => {
                    capture = Some("Visible");
                    buf.clear();
                }
                "Row" if in_note_data => {
                    capture = Some("Row");
                    buf.clear();
                }
                "Column" if in_note_data => {
                    capture = Some("Column");
                    buf.clear();
                }
                _ => {}
            },
            XmlEvent::Text(t) if capture.is_some() => {
                if buf.len() + t.len() <= 64 {
                    buf.push_str(&t);
                }
            }
            XmlEvent::End(name) => match local_name(name) {
                "Visible" if capture == Some("Visible") => {
                    capture = None;
                    let t = buf.trim();
                    visible_tag = Some(t.is_empty() || t.eq_ignore_ascii_case("true") || t == "1");
                    buf.clear();
                }
                "Row" if capture == Some("Row") => {
                    capture = None;
                    row = buf.trim().parse::<u32>().ok();
                    buf.clear();
                }
                "Column" if capture == Some("Column") => {
                    capture = None;
                    col = buf.trim().parse::<u32>().ok();
                    buf.clear();
                }
                "ClientData" if in_note_data => {
                    if let (Some(r), Some(c)) = (row, col) {
                        if r < OoxmlCodec::MAX_XLSX_ROWS
                            && c < OoxmlCodec::MAX_XLSX_COLS
                            && visible_tag.unwrap_or(style_visible)
                        {
                            visible_out.insert(pack_cell_pos(r, c));
                        }
                    }
                    in_note_data = false;
                }
                "shape" => {
                    in_shape = false;
                    in_note_data = false;
                    capture = None;
                }
                _ => {}
            },
            _ => {}
        }
    }
    Ok(())
}

fn parse_single_cell_ref(raw: &str) -> Option<(u32, u32)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.contains(':') || !trimmed.bytes().any(|b| b.is_ascii_digit()) {
        return None;
    }
    let b = CellBox::parse(trimmed)?;
    (b.row0 == b.row1
        && b.col0 == b.col1
        && b.row0 < OoxmlCodec::MAX_XLSX_ROWS
        && b.col0 < OoxmlCodec::MAX_XLSX_COLS)
        .then_some((b.row0, b.col0))
}

fn strip_author_prefix(author: &str, raw: &str) -> String {
    if author.is_empty() {
        return raw.to_string();
    }
    if let Some(after) = raw.strip_prefix(author) {
        if let Some(rest) = after.strip_prefix(":\r\n") {
            return rest.to_string();
        }
        if let Some(rest) = after.strip_prefix(":\n") {
            return rest.to_string();
        }
        if after == ":" {
            return String::new();
        }
    }
    raw.to_string()
}

fn parse_comments_xml(
    xml: &str,
    visible_set: &HashSet<u64>,
    seen_in_sheet: &mut usize,
    workbook_notes: &mut usize,
    notes_out: &mut BTreeMap<u64, SheetCellNote>,
) -> Result<u32, EasyOfficeError> {
    let mut reader = XmlReader::with_budget(xml, MAX_COMMENTS_XML_EVENTS);
    let mut authors: Vec<String> = Vec::new();
    let mut in_authors = false;
    let mut in_author = false;
    let mut author_buf = String::new();

    let mut cur_cell: Option<(u32, u32)> = None;
    let mut cur_author_id: Option<usize> = None;
    let mut cur_bad_comment = false;
    let mut in_text = false;
    let mut rph_depth = 0usize;
    let mut in_t = false;
    let mut text_buf = String::new();
    let mut rejected = 0u32;

    let max_raw_buf = MAX_SHEET_NOTE_TEXT_BYTES + MAX_NOTE_AUTHOR_BYTES + 64;

    while let Some(ev) = reader.next_event()? {
        match ev {
            XmlEvent::Start { name, attrs } => match local_name(name) {
                "authors" => in_authors = true,
                "author" if in_authors => {
                    if authors.len() >= MAX_NOTES_PER_SHEET {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "시트 메모 작성자 수가 허용 상한({MAX_NOTES_PER_SHEET})을 초과했습니다."
                        )));
                    }
                    in_author = true;
                    author_buf.clear();
                }
                "comment" => {
                    *seen_in_sheet += 1;
                    if *seen_in_sheet > MAX_NOTES_PER_SHEET {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "시트 메모 수가 허용 상한({MAX_NOTES_PER_SHEET})을 초과했습니다."
                        )));
                    }
                    if *workbook_notes >= MAX_WORKBOOK_NOTES {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "통합 문서 전체 메모 수가 허용 상한({MAX_WORKBOOK_NOTES})을 초과했습니다."
                        )));
                    }
                    *workbook_notes += 1;
                    cur_cell = attrs.get("ref").and_then(|r| parse_single_cell_ref(&r));
                    let aid = match attrs.get("authorId") {
                        None => Some(0usize),
                        Some(v) => v.trim().parse::<usize>().ok(),
                    };
                    cur_author_id = aid;
                    cur_bad_comment = cur_cell.is_none() || cur_author_id.is_none();
                    if cur_bad_comment {
                        rejected = rejected.saturating_add(1);
                    }
                    text_buf.clear();
                    in_text = false;
                    rph_depth = 0;
                    in_t = false;
                }
                "text" if !cur_bad_comment && cur_cell.is_some() => in_text = true,
                "rPh" if in_text => rph_depth += 1,
                "t" if in_text && rph_depth == 0 => in_t = true,
                _ => {}
            },
            XmlEvent::Text(t) => {
                if in_author {
                    if author_buf.len() + t.len() > MAX_NOTE_AUTHOR_BYTES {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "시트 메모 작성자 길이가 허용 상한({MAX_NOTE_AUTHOR_BYTES}B)을 초과했습니다."
                        )));
                    }
                    author_buf.push_str(&t);
                } else if in_t {
                    if text_buf.len() + t.len() > max_raw_buf {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                            "시트 메모 본문 길이가 허용 상한({MAX_SHEET_NOTE_TEXT_BYTES}B)을 초과했습니다."
                        )));
                    }
                    text_buf.push_str(&t);
                }
            }
            XmlEvent::End(name) => match local_name(name) {
                "authors" => in_authors = false,
                "author" if in_author => {
                    in_author = false;
                    authors.push(std::mem::take(&mut author_buf));
                }
                "t" if in_t => in_t = false,
                "rPh" if rph_depth > 0 => rph_depth -= 1,
                "text" => in_text = false,
                "comment" => {
                    if !cur_bad_comment {
                        if let Some((row, col)) = cur_cell {
                            let author = cur_author_id
                                .and_then(|id| authors.get(id))
                                .cloned()
                                .unwrap_or_default();
                            let text = strip_author_prefix(&author, &text_buf);
                            if text.len() > MAX_SHEET_NOTE_TEXT_BYTES {
                                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                                    "시트 메모 본문 길이가 허용 상한({MAX_SHEET_NOTE_TEXT_BYTES}B)을 초과했습니다."
                                )));
                            }
                            let packed = pack_cell_pos(row, col);
                            let visible = visible_set.contains(&packed);
                            notes_out.insert(
                                packed,
                                SheetCellNote {
                                    author,
                                    text,
                                    visible,
                                },
                            );
                        }
                    }
                    cur_cell = None;
                    cur_author_id = None;
                    cur_bad_comment = false;
                    text_buf.clear();
                    in_text = false;
                    in_t = false;
                    rph_depth = 0;
                }
                _ => {}
            },
        }
    }
    Ok(rejected)
}

/// Reads cell notes and VML visibility for worksheet `part_path` into `notes_out`.
pub(crate) fn read_sheet_comments(
    sheet_xml: &str,
    part_path: &str,
    entries: &HashMap<&str, &[u8]>,
    workbook: &mut WorkbookComments,
    notes_out: &mut std::collections::BTreeMap<u64, SheetCellNote>,
) -> Result<u32, EasyOfficeError> {
    let SheetCommentRels {
        comments: comment_targets,
        vmls: vml_rels,
    } = sheet_comment_rels(entries, part_path)?;
    if comment_targets.is_empty() {
        return Ok(0);
    }

    let mut visible_set = HashSet::new();
    let legacy_rids = sheet_legacy_rids(sheet_xml)?;
    let vml_paths: Vec<Option<String>> = if legacy_rids.is_empty() {
        vml_rels.into_values().collect()
    } else {
        legacy_rids
            .iter()
            .filter_map(|rid| vml_rels.get(rid).cloned())
            .collect()
    };
    for path_opt in vml_paths {
        let Some(vml_path) = path_opt else {
            continue;
        };
        if !workbook.parts.insert(vml_path.clone()) {
            continue;
        }
        if let Some(vml_xml) = entries
            .get(vml_path.as_str())
            .and_then(|d| std::str::from_utf8(d).ok())
        {
            parse_vml_visibility(vml_xml, &mut visible_set)?;
        }
    }

    let mut rejected = 0u32;
    let mut seen_in_sheet = 0usize;
    for target_opt in comment_targets {
        let Some(comment_path) = target_opt else {
            rejected = rejected.saturating_add(1);
            continue;
        };
        if !workbook.parts.insert(comment_path.clone()) {
            rejected = rejected.saturating_add(1);
            continue;
        }
        let Some(comments_xml) = entries
            .get(comment_path.as_str())
            .and_then(|d| std::str::from_utf8(d).ok())
        else {
            rejected = rejected.saturating_add(1);
            continue;
        };
        rejected = rejected.saturating_add(parse_comments_xml(
            comments_xml,
            &visible_set,
            &mut seen_in_sheet,
            &mut workbook.notes,
            notes_out,
        )?);
    }
    Ok(rejected)
}
