//! DOCX reader producing a [`RichDocument`]: runs (bold/italic/underline/strike), alignment,
//! heading levels (style names and outline levels), lists (list styles and `numPr` through
//! `numbering.xml`: kind, nesting level and number format), and tables at their position in the
//! body.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::office_codecs::{checked_part, checked_zip_xml_entries};
use crate::rich_document::{highlight_name_to_hex, normalize_hex_color, RichToc};
use crate::rich_read_images::{drawing_picture, load_docx_hyperlinks, DocxMedia, DrawingOutcome};
use crate::rich_read_numbering::{read_numbering, Numbering, LIST_LEVELS};
use crate::rich_table::TableAssembler;
use crate::rich_xml::{
    heading_level_from_name, push_linked_run, toc_role_from_name, toggle_on, CharFmt,
};
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{
    EasyOfficeError, RichAlignment, RichBlock, RichDocument, RichImage, RichListKind,
    RichParagraph, RichScript, XmlSanitizer, ZipArchiveCodec, ZipSecurityPolicy,
    MAX_IMAGES_PER_DOCUMENT, MAX_TOTAL_IMAGE_BYTES,
};

/// `w:vMerge` state of a table cell.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum VMerge {
    #[default]
    None,
    Restart,
    Continue,
}

/// `w:gridSpan/@w:val`, at least 1; an unreadable value is 1 and a huge one is left for the
/// table assembler to reject.
fn grid_span(attrs: &Attrs<'_>) -> u32 {
    attr(attrs, "w:val")
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(1, |v| u32::try_from(v).unwrap_or(u32::MAX).max(1))
}

#[derive(Default)]
struct ParaState {
    runs: Vec<crate::RichRun>,
    style_id: Option<String>,
    jc: Option<String>,
    num_id: Option<String>,
    /// `w:numPr/w:ilvl`, 0 when absent or unreadable.
    ilvl: Option<usize>,
    outline: Option<u8>,
    images: Vec<RichImage>,
    left_indent: Option<u32>,
    first_line: Option<i32>,
    line_percent: Option<u16>,
}

const MAX_STYLE_INHERIT_DEPTH: usize = 16;

#[derive(Default, Clone, Debug)]
struct DocxStyle {
    name: Option<String>,
    based_on: Option<String>,
    outline_lvl: Option<u8>,
}

/// styleId -> style metadata (`w:name`, `w:basedOn`, `w:outlineLvl`).
/// Adapted from BSD-2-Clause and MIT reference implementations (see `THIRD_PARTY_NOTICES.md`).
fn read_styles(xml: &str) -> Result<HashMap<String, DocxStyle>, EasyOfficeError> {
    let mut out: HashMap<String, DocxStyle> = HashMap::new();
    let mut current: Option<String> = None;
    for ev in XmlReader::new(xml) {
        match ev? {
            XmlEvent::Start {
                name: "w:style",
                attrs,
            } => {
                if let Some(id) = attr(&attrs, "w:styleId") {
                    out.entry(id.to_string()).or_default();
                    current = Some(id.into_owned());
                } else {
                    current = None;
                }
            }
            XmlEvent::Start {
                name: "w:name",
                attrs,
            } => {
                if let (Some(id), Some(val)) = (current.as_ref(), attr(&attrs, "w:val")) {
                    out.entry(id.clone()).or_default().name = Some(val.to_string());
                }
            }
            XmlEvent::Start {
                name: "w:basedOn",
                attrs,
            } => {
                if let (Some(id), Some(val)) = (current.as_ref(), attr(&attrs, "w:val")) {
                    out.entry(id.clone()).or_default().based_on = Some(val.to_string());
                }
            }
            XmlEvent::Start {
                name: "w:outlineLvl",
                attrs,
            } => {
                if let (Some(id), Some(val)) = (current.as_ref(), attr(&attrs, "w:val")) {
                    if let Ok(lvl) = val.parse::<u8>() {
                        out.entry(id.clone()).or_default().outline_lvl = Some(lvl);
                    }
                }
            }
            XmlEvent::End("w:style") => current = None,
            _ => {}
        }
    }
    Ok(out)
}

fn alignment(jc: Option<&str>) -> Option<RichAlignment> {
    jc.and_then(RichAlignment::parse)
}

#[inline]
fn starts_with_ignore_ascii_case(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

struct Resolver<'a> {
    styles: &'a HashMap<String, DocxStyle>,
    numbering: &'a Numbering,
}

impl Resolver<'_> {
    fn resolve_style_chain(&self, start_id: &str) -> (u8, Option<RichListKind>) {
        let mut heading = 0u8;
        let mut list_from_style = None;
        let mut cur_id = Some(start_id);
        for _ in 0..MAX_STYLE_INHERIT_DEPTH {
            let Some(id) = cur_id else {
                break;
            };
            if heading == 0 {
                heading = heading_level_from_name(id);
            }
            let Some(st) = self.styles.get(id) else {
                break;
            };
            if let Some(name) = st.name.as_deref() {
                if heading == 0 {
                    heading = heading_level_from_name(name);
                }
                if list_from_style.is_none() {
                    if starts_with_ignore_ascii_case(name, "list bullet") {
                        list_from_style = Some(RichListKind::Bullet);
                    } else if starts_with_ignore_ascii_case(name, "list number") {
                        list_from_style = Some(RichListKind::Ordered);
                    }
                }
            }
            if heading == 0 {
                if let Some(lvl) = st.outline_lvl.filter(|&l| l < 9) {
                    heading = (lvl + 1).min(6);
                }
            }
            cur_id = st.based_on.as_deref();
        }
        (heading, list_from_style)
    }

    /// The table-of-contents role of the paragraph style (its id or its name).
    fn toc_role(&self, style_id: &str) -> Option<RichToc> {
        toc_role_from_name(style_id).or_else(|| {
            let name = self.styles.get(style_id)?.name.as_deref()?;
            toc_role_from_name(name)
        })
    }

    fn finish(&self, p: ParaState) -> RichParagraph {
        let toc = p.style_id.as_deref().and_then(|id| self.toc_role(id));
        let (mut heading, style_list) = p
            .style_id
            .as_deref()
            .map_or((0, None), |id| self.resolve_style_chain(id));
        if toc.is_some() {
            heading = 0;
        } else if heading == 0 {
            if let Some(level) = p.outline.filter(|l| *l < 9) {
                heading = (level + 1).min(6);
            }
        }
        let level = p.ilvl.unwrap_or(0).min(LIST_LEVELS - 1);
        let numbered = p
            .num_id
            .as_deref()
            .filter(|id| *id != "0")
            .map(|id| self.numbering.level(id, level));
        // A list style names the kind (as before); the number format is the level's own when
        // the numbering agrees on the kind.
        let (list, list_number) = match (heading, style_list, numbered) {
            (1.., ..) => (None, None),
            (_, Some(kind), def) => (
                Some(kind),
                def.filter(|d| d.kind == kind).and_then(|d| d.number),
            ),
            (_, None, Some(def)) => (Some(def.kind), def.number),
            (_, None, None) => (None, None),
        };
        RichParagraph {
            list_level: if list.is_some() { level as u8 } else { 0 },
            list_number,
            alignment: alignment(p.jc.as_deref()),
            runs: p.runs,
            heading_level: heading,
            list,
            left_indent_twips: p.left_indent,
            first_line_indent_twips: p.first_line,
            line_spacing_percent: p.line_percent,
            toc,
        }
    }
}

fn plain(p: &ParaState) -> String {
    p.runs.iter().map(|r| r.text.as_str()).collect()
}

/// Elements whose whole subtree is ignored (revisions, fallbacks, deleted text).
const SKIPPED_ELEMENTS: [&str; 11] = [
    "mc:Fallback",
    "w:rPrChange",
    "w:pPrChange",
    "w:sectPrChange",
    "w:tblPrChange",
    "w:trPrChange",
    "w:tcPrChange",
    "w:del",
    "w:moveFrom",
    "w:instrText",
    "w:delText",
];
const UNSUPPORTED_OBJECT_ELEMENTS: [&str; 2] = ["w:pict", "w:object"];
const DRAWING_ELEMENT: &str = "w:drawing";
/// `w:spacing/@w:line` of single line spacing (240ths of a line).
const DOCX_SINGLE_LINE_240THS: i64 = 240;
const SINGLE_LINE_PERCENT: u16 = 100;
const UNSUPPORTED_OBJECT_NOTE: &str = "그림·도형·OLE 개체는 가져오지 않았습니다.";

/// Applies a run-property element (`w:b`, `w:sz`, ...) to `fmt`.
fn apply_run_property(fmt: &mut CharFmt, name: &str, attrs: &Attrs<'_>) {
    let val = attr(attrs, "w:val");
    let val = val.as_deref();
    match name {
        "w:b" => fmt.bold = toggle_on(val),
        "w:i" => fmt.italic = toggle_on(val),
        "w:strike" | "w:dstrike" => fmt.strike = toggle_on(val),
        "w:u" => fmt.underline = toggle_on(val),
        "w:rFonts" => {
            fmt.font = ["w:ascii", "w:hAnsi", "w:eastAsia"]
                .iter()
                .find_map(|k| attr(attrs, k))
                .filter(|f| !f.trim().is_empty())
                .map(Cow::into_owned);
        }
        "w:sz" => fmt.size_half_pt = val.and_then(|v| v.parse::<u16>().ok()).filter(|v| *v > 0),
        "w:color" => fmt.color = val.and_then(normalize_hex_color),
        "w:highlight" => fmt.highlight = val.and_then(highlight_name_to_hex),
        "w:vertAlign" => {
            fmt.script = match val {
                Some("superscript") => RichScript::Superscript,
                Some("subscript") => RichScript::Subscript,
                _ => RichScript::Normal,
            };
        }
        "w:shd" => {
            if let Some(fill) = attr(attrs, "w:fill")
                .as_deref()
                .and_then(normalize_hex_color)
            {
                fmt.highlight = Some(fill);
            }
        }
        _ => {}
    }
}

/// `w:spacing` (auto line rule, 240ths of a line) and `w:ind` of a paragraph.
fn apply_para_layout(top: &mut ParaState, name: &str, attrs: &Attrs<'_>) {
    let number = |key: &str| attr(attrs, key).and_then(|v| v.trim().parse::<i64>().ok());
    match name {
        "w:spacing" if attr(attrs, "w:lineRule").is_none_or(|r| r == "auto") => {
            top.line_percent = number("w:line")
                .filter(|l| *l > 0)
                .and_then(|l| u16::try_from(l * 100 / DOCX_SINGLE_LINE_240THS).ok())
                .filter(|p| *p != SINGLE_LINE_PERCENT);
        }
        "w:ind" => {
            top.left_indent = number("w:left").and_then(|v| u32::try_from(v).ok());
            top.first_line = number("w:firstLine")
                .or_else(|| number("w:hanging").map(|h| -h))
                .and_then(|v| i32::try_from(v).ok());
        }
        _ => {}
    }
}

/// Mutable walker state for `word/document.xml` events.
#[derive(Default)]
struct DocxWalker {
    warnings: Vec<String>,
    blocks: Vec<RichBlock>,
    paras: Vec<ParaState>,
    in_ppr: bool,
    in_run: bool,
    in_rpr: bool,
    in_t: bool,
    fmt: CharFmt,
    cur_text: String,
    tbl_depth: usize,
    table: TableAssembler,
    cell_paras: Vec<String>,
    /// `w:gridSpan` and `w:vMerge` of the open top-level cell.
    cell_span: u32,
    cell_v_merge: VMerge,
    /// Merged regions that were dropped as invalid (reported once per document).
    dropped_merges: usize,
    image_count: usize,
    image_bytes: usize,
    /// Relationship id -> address of the document's hyperlinks, and the one of the open `w:hyperlink`.
    links: HashMap<String, String>,
    link: Option<String>,
}

impl DocxWalker {
    /// Handles an element start; returns true when the whole subtree must be skipped.
    fn on_start(&mut self, name: &str, attrs: &Attrs<'_>) -> bool {
        if SKIPPED_ELEMENTS.contains(&name) {
            return true;
        }
        if UNSUPPORTED_OBJECT_ELEMENTS.contains(&name) {
            if !self.warnings.iter().any(|w| w == UNSUPPORTED_OBJECT_NOTE) {
                self.warnings.push(UNSUPPORTED_OBJECT_NOTE.to_string());
            }
            return true;
        }
        self.track_structure(name, attrs);
        if let Some(top) = self.paras.last_mut() {
            if self.in_ppr {
                Self::apply_para_property(top, name, attr(attrs, "w:val").as_deref());
                apply_para_layout(top, name, attrs);
            }
        }
        if self.in_run && self.in_rpr {
            apply_run_property(&mut self.fmt, name, attrs);
        } else if self.in_run && !self.in_ppr {
            self.apply_inline_break(name, attrs);
        }
        false
    }

    fn track_structure(&mut self, name: &str, attrs: &Attrs<'_>) {
        match name {
            "w:tbl" => {
                self.tbl_depth += 1;
                if self.tbl_depth == 1 {
                    self.table.reset();
                }
            }
            "w:tr" if self.tbl_depth == 1 => self.table.start_row(),
            "w:tc" if self.tbl_depth == 1 => {
                self.cell_paras.clear();
                self.cell_span = 1;
                self.cell_v_merge = VMerge::None;
            }
            "w:gridSpan" if self.tbl_depth == 1 => self.cell_span = grid_span(attrs),
            "w:vMerge" if self.tbl_depth == 1 => {
                self.cell_v_merge = if attr(attrs, "w:val").as_deref() == Some("restart") {
                    VMerge::Restart
                } else {
                    VMerge::Continue
                };
            }
            "w:p" => self.paras.push(ParaState::default()),
            "w:hyperlink" => {
                self.link = attr(attrs, "r:id").and_then(|id| self.links.get(id.as_ref()).cloned());
            }
            "w:pPr" => self.in_ppr = true,
            "w:r" => {
                self.in_run = true;
                self.fmt = CharFmt::default();
                self.cur_text.clear();
            }
            "w:rPr" if self.in_run => self.in_rpr = true,
            "w:t" if self.in_run => self.in_t = true,
            _ => {}
        }
    }

    fn apply_para_property(top: &mut ParaState, name: &str, val: Option<&str>) {
        match name {
            "w:pStyle" => top.style_id = val.map(str::to_string),
            "w:jc" => top.jc = val.map(str::to_string),
            "w:numId" => top.num_id = val.map(str::to_string),
            "w:ilvl" => top.ilvl = val.and_then(|v| v.trim().parse().ok()),
            "w:outlineLvl" => top.outline = val.and_then(|v| v.parse().ok()),
            _ => {}
        }
    }

    fn apply_inline_break(&mut self, name: &str, attrs: &Attrs<'_>) {
        match name {
            "w:tab" | "w:ptab" => self.cur_text.push('\t'),
            "w:br" if !matches!(attr(attrs, "w:type").as_deref(), Some("page" | "column")) => {
                self.cur_text.push('\n');
            }
            "w:cr" => self.cur_text.push('\n'),
            "w:noBreakHyphen" => self.cur_text.push('-'),
            _ => {}
        }
    }

    fn on_text(&mut self, text: &str) {
        if self.in_t {
            self.cur_text.push_str(text);
        }
    }

    fn on_end(&mut self, name: &str, resolver: &Resolver<'_>) {
        match name {
            "w:t" => self.in_t = false,
            "w:rPr" => self.in_rpr = false,
            "w:pPr" => self.in_ppr = false,
            "w:r" => {
                if let Some(top) = self.paras.last_mut() {
                    push_linked_run(
                        &mut top.runs,
                        &self.cur_text,
                        &self.fmt,
                        self.link.as_deref(),
                    );
                }
                self.cur_text.clear();
                self.in_run = false;
                self.in_rpr = false;
                self.in_t = false;
            }
            "w:hyperlink" => self.link = None,
            "w:p" => self.finish_paragraph(resolver),
            "w:tc" if self.tbl_depth == 1 => {
                while self.cell_paras.last().is_some_and(String::is_empty) {
                    self.cell_paras.pop();
                }
                let text = self.cell_paras.join("\n");
                self.cell_paras.clear();
                self.push_cell(text);
            }
            "w:tr" if self.tbl_depth == 1 => self.table.end_row(),
            "w:tbl" => {
                self.tbl_depth = self.tbl_depth.saturating_sub(1);
                if self.tbl_depth == 0 && !self.table.is_empty() {
                    let (table, dropped) = std::mem::take(&mut self.table).finish();
                    self.dropped_merges += dropped;
                    self.blocks.push(RichBlock::Table(table));
                }
            }
            _ => {}
        }
    }

    fn push_cell(&mut self, text: String) {
        let span = self.cell_span;
        match self.cell_v_merge {
            VMerge::None => self.table.spanning(text, span),
            VMerge::Restart => self.table.restart(text, span),
            VMerge::Continue => self.table.continue_above(text, span),
        }
    }

    /// Records the picture of a `w:drawing` in the open paragraph, or a warning when skipped.
    fn on_drawing(
        &mut self,
        events: &[XmlEvent<'_>],
        media: &DocxMedia<'_>,
    ) -> Result<(), EasyOfficeError> {
        let img = match drawing_picture(events, media) {
            DrawingOutcome::Picture(img) => img,
            DrawingOutcome::Skipped(note) => {
                self.warn_once(note);
                return Ok(());
            }
        };
        if self.tbl_depth > 0 || self.paras.is_empty() {
            self.warn_once("표 안의 그림은 가져오지 않았습니다.".to_string());
            return Ok(());
        }
        self.image_count += 1;
        self.image_bytes = self.image_bytes.saturating_add(img.bytes().len());
        if self.image_count > MAX_IMAGES_PER_DOCUMENT || self.image_bytes > MAX_TOTAL_IMAGE_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "DOCX 그림이 문서당 한도(개수·합계 크기)를 넘었습니다.".to_string(),
            ));
        }
        if let Some(top) = self.paras.last_mut() {
            top.images.push(img);
        }
        Ok(())
    }

    fn warn_once(&mut self, note: String) {
        if !self.warnings.contains(&note) {
            self.warnings.push(note);
        }
    }

    fn finish_paragraph(&mut self, resolver: &Resolver<'_>) {
        let Some(mut p) = self.paras.pop() else {
            return;
        };
        let images = std::mem::take(&mut p.images);
        if self.tbl_depth > 0 {
            self.cell_paras.push(plain(&p));
        } else {
            if p.runs.iter().any(|r| !r.text.trim().is_empty()) {
                self.blocks.push(RichBlock::Paragraph(resolver.finish(p)));
            }
            self.blocks.extend(images.into_iter().map(RichBlock::Image));
        }
    }
}

/// Decodes `word/document.xml` (plus styles and numbering) of a DOCX package.
pub(crate) fn read_docx(bytes: &[u8]) -> Result<(RichDocument, Vec<String>), EasyOfficeError> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
    let parts = checked_zip_xml_entries(&entries)?;
    let doc_xml = checked_part(&parts, "word/document.xml").ok_or_else(|| {
        EasyOfficeError::CorruptedContainer(
            "DOCX 필수 엔트리 'word/document.xml'이 누락되었습니다.".to_string(),
        )
    })?;
    let styles = match checked_part(&parts, "word/styles.xml") {
        Some(xml) => read_styles(xml)?,
        None => HashMap::new(),
    };
    let numbering = match checked_part(&parts, "word/numbering.xml") {
        Some(xml) => read_numbering(xml)?,
        None => Numbering::default(),
    };
    let title = checked_part(&parts, "docProps/core.xml")
        .and_then(|xml| {
            crate::rtf_xml_helpers::extract_tag_bodies(xml, "dc:title")
                .into_iter()
                .next()
                .map(XmlSanitizer::unescape)
        })
        .unwrap_or_default();

    let resolver = Resolver {
        styles: &styles,
        numbering: &numbering,
    };
    let media = DocxMedia::load(&entries)?;
    let mut walker = DocxWalker {
        links: load_docx_hyperlinks(&entries)?,
        ..DocxWalker::default()
    };
    let mut reader = XmlReader::new(doc_xml);
    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Start { name, .. } if name == DRAWING_ELEMENT => {
                let drawing = reader.collect_subtree(event)?;
                walker.on_drawing(&drawing, &media)?;
            }
            XmlEvent::Start { name, ref attrs } => {
                if walker.on_start(name, attrs) {
                    reader.skip_subtree()?;
                }
            }
            XmlEvent::Text(text) => walker.on_text(&text),
            XmlEvent::End(name) => walker.on_end(name, &resolver),
        }
    }
    let mut warnings = walker.warnings;
    if walker.dropped_merges > 0 {
        warnings.push(format!(
            "표의 병합 {}곳은 범위를 벗어나거나 겹쳐 가져오지 않았습니다.",
            walker.dropped_merges
        ));
    }
    if entries.iter().any(|e| {
        let p = e.path.as_str();
        (p.starts_with("word/header") || p.starts_with("word/footer")) && p.ends_with(".xml")
    }) {
        warnings.push("머리말·꼬리말은 가져오지 않았습니다.".to_string());
    }
    Ok((
        RichDocument {
            title,
            blocks: walker.blocks,
            official_agency: None,
        },
        warnings,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_style_inheritance_resolves_based_on_and_outline_lvl_and_breaks_cycles() {
        let styles_xml = r#"<w:styles>
            <w:style w:type="paragraph" w:styleId="BaseH2">
                <w:name w:val="Heading 2"/>
            </w:style>
            <w:style w:type="paragraph" w:styleId="CustomSub">
                <w:name w:val="Custom Subheading"/>
                <w:basedOn w:val="BaseH2"/>
            </w:style>
            <w:style w:type="paragraph" w:styleId="Outline3">
                <w:name w:val="Section Title"/>
                <w:pPr><w:outlineLvl w:val="2"/></w:pPr>
            </w:style>
            <w:style w:type="paragraph" w:styleId="CycleA">
                <w:basedOn w:val="CycleB"/>
            </w:style>
            <w:style w:type="paragraph" w:styleId="CycleB">
                <w:basedOn w:val="CycleA"/>
            </w:style>
        </w:styles>"#;
        let styles = read_styles(styles_xml).expect("styles parse");
        let numbering = Numbering::default();
        let resolver = Resolver {
            styles: &styles,
            numbering: &numbering,
        };
        assert_eq!(resolver.resolve_style_chain("CustomSub"), (2, None));
        assert_eq!(resolver.resolve_style_chain("Outline3"), (3, None));
        assert_eq!(resolver.resolve_style_chain("CycleA"), (0, None));
    }
}
