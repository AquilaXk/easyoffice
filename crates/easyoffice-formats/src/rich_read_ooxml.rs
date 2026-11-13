//! DOCX reader producing a [`RichDocument`]: runs (bold/italic/underline/strike), alignment,
//! heading levels (style names and outline levels), lists (list styles and `numPr` through
//! `numbering.xml`: kind, nesting level and number format), and tables at their position in the
//! body.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::office_codecs::{checked_part, checked_zip_xml_entries};
use crate::rich_document::{frame_paragraph, MAX_PAGE_FRAME_PARAGRAPHS};
use crate::rich_document::{
    highlight_name_to_hex, normalize_hex_color, toc_bookmark_number, RichToc,
};
use crate::rich_page_frame::field_of_instruction;
use crate::rich_read_images::{
    drawing_picture, load_docx_frame_targets, load_docx_hyperlinks, DocxMedia, DrawingOutcome,
};
use crate::rich_read_numbering::{read_numbering, Numbering, LIST_LEVELS};
use crate::rich_read_page_setup::{
    binding_of, fitting_setup, page_gutter, page_margins, page_paper,
};
use crate::rich_table::TableAssembler;
use crate::rich_xml::{
    heading_level_from_name, push_linked_run, take_toc_page, toc_role_from_name, toggle_on, CharFmt,
};
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{
    EasyOfficeError, RichAlignment, RichBlock, RichDocument, RichField, RichImage, RichListKind,
    RichPageFrame, RichPageMargins, RichPaper, RichParagraph, RichRun, RichScript, XmlSanitizer,
    ZipArchiveCodec, ZipSecurityPolicy, MAX_IMAGES_PER_DOCUMENT, MAX_TOTAL_IMAGE_BYTES,
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
    /// Number of a `_Toc` bookmark that starts in the paragraph.
    bookmark: Option<u32>,
    /// `w:leader` of the paragraph's right tab stop (a TOC entry's page number).
    tab_leader: Option<String>,
    /// `w:pageBreakBefore`, or a page break before any text of the paragraph.
    page_break_before: bool,
    /// A page break after text of the paragraph: the next paragraph starts the page.
    page_break_after: bool,
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

    fn finish(&self, mut p: ParaState) -> RichParagraph {
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
        let toc_page = match toc {
            Some(RichToc::Entry(_)) => take_toc_page(&mut p.runs),
            _ => None,
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
            bookmark: p.bookmark,
            toc_page,
            toc_dots: toc_page.is_some() && p.tab_leader.as_deref().is_none_or(|l| l == "dot"),
            page_break_before: p.page_break_before,
        }
    }
}

fn plain(p: &ParaState) -> String {
    p.runs.iter().map(|r| r.text.as_str()).collect()
}

/// A `w:headerReference` or `w:footerReference` of a section.
struct FrameRef {
    header: bool,
    /// `default`, `first` or `even`.
    kind: String,
    rel_id: String,
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

/// The in-document link (`#_Toc12`) to a table-of-contents bookmark name.
fn toc_bookmark_link(name: &str) -> Option<String> {
    toc_bookmark_number(name).map(|_| format!("#{name}"))
}

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
    /// A page break was read that the next paragraph has to carry (a paragraph that held only the break is not kept).
    pending_page_break: bool,
    /// Header and footer parts keep page fields as field runs.
    frame_mode: bool,
    frame_refs: Vec<FrameRef>,
    /// A section shows its own first-page header and footer (`w:titlePg`).
    title_pg: bool,
    /// Instruction text of the open complex field (`w:fldChar` begin to separate) and whether its text is open.
    field_instruction: Option<String>,
    in_instruction: bool,
    /// The field of an open `w:fldSimple` and whether its field run is written.
    simple_field: Option<(RichField, bool)>,
    /// The field of a complex field whose result is open.
    result_field: Option<RichField>,
    /// `w:pgMar` and `w:pgSz` of the open section, and the page setup of the first section that gives margins.
    section_margins: Option<RichPageMargins>,
    section_paper: Option<RichPaper>,
    first_margins: Option<RichPageMargins>,
    section_gutter: u32,
    first_gutter: u32,
    first_paper: Option<RichPaper>,
}

impl DocxWalker {
    /// Handles an element start; returns true when the whole subtree must be skipped.
    fn on_start(&mut self, name: &str, attrs: &Attrs<'_>) -> bool {
        if self.frame_mode && name == "w:instrText" {
            self.in_instruction = true;
            return false;
        }
        if SKIPPED_ELEMENTS.contains(&name) {
            return true;
        }
        if self.frame_mode && name == DRAWING_ELEMENT {
            self.warn_once("머리말·꼬리말의 그림·도형은 가져오지 않았습니다.".to_string());
            return true;
        }
        if UNSUPPORTED_OBJECT_ELEMENTS.contains(&name) {
            if !self.warnings.iter().any(|w| w == UNSUPPORTED_OBJECT_NOTE) {
                self.warnings.push(UNSUPPORTED_OBJECT_NOTE.to_string());
            }
            return true;
        }
        self.track_structure(name, attrs);
        self.track_frame(name, attrs);
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
                self.link = attr(attrs, "r:id")
                    .and_then(|id| self.links.get(id.as_ref()).cloned())
                    .or_else(|| attr(attrs, "w:anchor").and_then(|a| toc_bookmark_link(&a)));
            }
            "w:bookmarkStart" => {
                let number = attr(attrs, "w:name").and_then(|n| toc_bookmark_number(&n));
                if let Some(top) = self.paras.last_mut() {
                    top.bookmark = top.bookmark.or(number);
                }
            }
            "w:pPr" => self.in_ppr = true,
            "w:tab" if self.in_ppr => {
                if let Some(top) = self.paras.last_mut() {
                    top.tab_leader = top
                        .tab_leader
                        .take()
                        .or_else(|| attr(attrs, "w:leader").map(Cow::into_owned));
                }
            }
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

    /// Section header/footer references (body parts) and page fields (header/footer parts).
    fn track_frame(&mut self, name: &str, attrs: &Attrs<'_>) {
        match name {
            "w:sectPr" => {
                self.section_margins = None;
                self.section_paper = None;
            }
            "w:pgMar" => {
                self.section_margins = page_margins(attrs);
                self.section_gutter = page_gutter(attrs);
            }
            "w:pgSz" => self.section_paper = page_paper(attrs),
            "w:headerReference" | "w:footerReference" => {
                if let Some(rel_id) = attr(attrs, "r:id") {
                    self.frame_refs.push(FrameRef {
                        header: name == "w:headerReference",
                        kind: attr(attrs, "w:type").map_or("default".to_string(), Cow::into_owned),
                        rel_id: rel_id.into_owned(),
                    });
                }
            }
            "w:titlePg" => self.title_pg |= toggle_on(attr(attrs, "w:val").as_deref()),
            "w:fldSimple" if self.frame_mode => {
                self.simple_field = attr(attrs, "w:instr")
                    .and_then(|i| field_of_instruction(&i))
                    .map(|field| (field, false));
            }
            "w:fldChar" if self.frame_mode => match attr(attrs, "w:fldCharType").as_deref() {
                Some("begin") => self.field_instruction = Some(String::new()),
                Some("separate") => {
                    let field = self
                        .field_instruction
                        .take()
                        .and_then(|i| field_of_instruction(&i));
                    if let Some(field) = field {
                        self.result_field = Some(field);
                        self.push_field_run(field);
                    }
                }
                Some("end") => {
                    self.field_instruction = None;
                    self.result_field = None;
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn push_field_run(&mut self, field: RichField) {
        if let Some(top) = self.paras.last_mut() {
            top.runs.push(RichRun {
                bold: self.fmt.bold,
                italic: self.fmt.italic,
                underline: self.fmt.underline,
                strike: self.fmt.strike,
                size_half_pt: self.fmt.size_half_pt,
                font: self.fmt.font.clone(),
                color: self.fmt.color.clone(),
                highlight: self.fmt.highlight.clone(),
                script: self.fmt.script,
                field: Some(field),
                ..RichRun::default()
            });
        }
    }

    fn apply_para_property(top: &mut ParaState, name: &str, val: Option<&str>) {
        match name {
            "w:pStyle" => top.style_id = val.map(str::to_string),
            "w:jc" => top.jc = val.map(str::to_string),
            "w:numId" => top.num_id = val.map(str::to_string),
            "w:ilvl" => top.ilvl = val.and_then(|v| v.trim().parse().ok()),
            "w:outlineLvl" => top.outline = val.and_then(|v| v.parse().ok()),
            "w:pageBreakBefore" => top.page_break_before = toggle_on(val),
            _ => {}
        }
    }

    fn apply_inline_break(&mut self, name: &str, attrs: &Attrs<'_>) {
        match name {
            "w:tab" | "w:ptab" => self.cur_text.push('\t'),
            "w:br" if attr(attrs, "w:type").as_deref() == Some("page") => self.note_page_break(),
            "w:br" if attr(attrs, "w:type").as_deref() != Some("column") => {
                self.cur_text.push('\n')
            }
            "w:cr" => self.cur_text.push('\n'),
            "w:noBreakHyphen" => self.cur_text.push('-'),
            _ => {}
        }
    }

    /// A `w:br w:type="page"`: text after it starts the page, so a break before any text belongs to this paragraph
    /// and one after text to the next.
    fn note_page_break(&mut self) {
        let before_text = self.cur_text.trim().is_empty();
        if let Some(top) = self.paras.last_mut() {
            if before_text && top.runs.iter().all(|r| r.text.trim().is_empty()) {
                top.page_break_before = true;
            } else {
                top.page_break_after = true;
            }
        }
    }

    fn on_text(&mut self, text: &str) {
        if self.in_t {
            self.cur_text.push_str(text);
        } else if self.in_instruction {
            if let Some(instruction) = self.field_instruction.as_mut() {
                instruction.push_str(text);
            }
        }
    }

    fn on_end(&mut self, name: &str, resolver: &Resolver<'_>) {
        match name {
            "w:t" => self.in_t = false,
            "w:instrText" => self.in_instruction = false,
            "w:fldSimple" => self.simple_field = None,
            "w:sectPr" => {
                if self.first_margins.is_none() && self.section_margins.is_some() {
                    self.first_gutter = self.section_gutter;
                }
                self.first_margins = self.first_margins.or(self.section_margins);
                self.first_paper = self.first_paper.or(self.section_paper);
            }
            "w:rPr" => self.in_rpr = false,
            "w:pPr" => self.in_ppr = false,
            "w:r" => {
                if let Some((field, written)) = self.simple_field {
                    if !written {
                        self.push_field_run(field);
                        self.simple_field = Some((field, true));
                    }
                } else if self.result_field.is_none() {
                    if let Some(top) = self.paras.last_mut() {
                        push_linked_run(
                            &mut top.runs,
                            &self.cur_text,
                            &self.fmt,
                            self.link.as_deref(),
                        );
                    }
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
            let carried = p.page_break_before;
            let after = p.page_break_after;
            let mut kept = false;
            if p.runs
                .iter()
                .any(|r| r.field.is_some() || !r.text.trim().is_empty())
            {
                let mut paragraph = resolver.finish(p);
                paragraph.page_break_before |= std::mem::take(&mut self.pending_page_break);
                self.blocks.push(RichBlock::Paragraph(paragraph));
                kept = true;
            }
            self.pending_page_break |= (carried && !kept) || after;
            self.blocks.extend(images.into_iter().map(RichBlock::Image));
        }
    }
}

/// Runs the events of one WordprocessingML part (the body or a header/footer) through `walker`.
fn walk_part(
    xml: &str,
    walker: &mut DocxWalker,
    resolver: &Resolver<'_>,
    media: &DocxMedia<'_>,
) -> Result<(), EasyOfficeError> {
    let mut reader = XmlReader::new(xml);
    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Start { name, .. } if name == DRAWING_ELEMENT && !walker.frame_mode => {
                let drawing = reader.collect_subtree(event)?;
                walker.on_drawing(&drawing, media)?;
            }
            XmlEvent::Start { name, ref attrs } => {
                if walker.on_start(name, attrs) {
                    reader.skip_subtree()?;
                }
            }
            XmlEvent::Text(text) => walker.on_text(&text),
            XmlEvent::End(name) => walker.on_end(name, resolver),
        }
    }
    Ok(())
}

/// Whether `word/settings.xml` turns on different headers and footers for odd and even pages.
fn even_and_odd_headers(settings: Option<&str>) -> Result<bool, EasyOfficeError> {
    let Some(xml) = settings else {
        return Ok(false);
    };
    for event in XmlReader::new(xml) {
        if let XmlEvent::Start {
            name: "w:evenAndOddHeaders",
            attrs,
        } = event?
        {
            return Ok(toggle_on(attr(&attrs, "w:val").as_deref()));
        }
    }
    Ok(false)
}

/// Whether `word/settings.xml` mirrors the margins of facing pages and puts the gutter at the top.
fn binding_flags(settings: Option<&str>) -> Result<(bool, bool), EasyOfficeError> {
    let (mut mirror, mut top) = (false, false);
    for event in XmlReader::new(settings.unwrap_or_default()) {
        if let XmlEvent::Start { name, attrs } = event? {
            let on = toggle_on(attr(&attrs, "w:val").as_deref());
            match name {
                "w:mirrorMargins" => mirror = on,
                "w:gutterAtTop" => top = on,
                _ => {}
            }
        }
    }
    Ok((mirror, top))
}

/// The paragraphs of one header or footer part.
fn read_frame_part(
    xml: &str,
    resolver: &Resolver<'_>,
    media: &DocxMedia<'_>,
    warnings: &mut Vec<String>,
) -> Result<Vec<RichParagraph>, EasyOfficeError> {
    let mut walker = DocxWalker {
        frame_mode: true,
        ..DocxWalker::default()
    };
    walk_part(xml, &mut walker, resolver, media)?;
    warnings.append(&mut walker.warnings);
    let mut paragraphs = Vec::new();
    for block in walker.blocks {
        match block {
            RichBlock::Paragraph(p) => paragraphs.push(frame_paragraph(&p)),
            RichBlock::Table(_) => {
                let note = "머리말·꼬리말의 표는 가져오지 않았습니다.".to_string();
                if !warnings.contains(&note) {
                    warnings.push(note);
                }
            }
            RichBlock::Image(_) => {}
        }
    }
    if paragraphs.len() > MAX_PAGE_FRAME_PARAGRAPHS {
        paragraphs.truncate(MAX_PAGE_FRAME_PARAGRAPHS);
        warnings.push(format!(
            "머리말·꼬리말은 문단 {MAX_PAGE_FRAME_PARAGRAPHS}개까지만 가져왔습니다."
        ));
    }
    Ok(paragraphs)
}

/// The header and footer of the page: the default one of the first section that has one. First-page and
/// odd/even ones, and ones that differ between sections, are reported and not kept.
fn read_page_frame(
    entries: &[crate::ZipArchiveEntry],
    parts: &[(&str, &str)],
    resolver: &Resolver<'_>,
    media: &DocxMedia<'_>,
    refs: &[FrameRef],
    title_pg: bool,
) -> Result<(RichPageFrame, Vec<String>), EasyOfficeError> {
    let mut frame = RichPageFrame::default();
    let mut warnings = Vec::new();
    if refs.is_empty() {
        return Ok((frame, warnings));
    }
    let targets = load_docx_frame_targets(entries)?;
    let even = even_and_odd_headers(checked_part(parts, "word/settings.xml"))?;
    let (mut other_pages, mut other_sections) = (false, false);
    for header in [true, false] {
        let mut chosen: Option<&str> = None;
        for r in refs.iter().filter(|r| r.header == header) {
            let Some(part) = targets.get(&r.rel_id) else {
                continue;
            };
            match r.kind.as_str() {
                "default" => match chosen {
                    None => chosen = Some(part),
                    Some(first) => other_sections |= first != part,
                },
                "first" => other_pages |= title_pg,
                "even" => other_pages |= even,
                _ => {}
            }
        }
        let paragraphs = match chosen.and_then(|part| checked_part(parts, part)) {
            Some(xml) => read_frame_part(xml, resolver, media, &mut warnings)?,
            None => Vec::new(),
        };
        if header {
            frame.header = paragraphs;
        } else {
            frame.footer = paragraphs;
        }
    }
    if other_pages {
        warnings.push("첫 쪽·짝수 쪽 머리말·꼬리말은 가져오지 않았습니다.".to_string());
    }
    if other_sections {
        warnings.push("구역마다 다른 머리말·꼬리말은 첫 구역의 것만 가져왔습니다.".to_string());
    }
    Ok((frame, warnings))
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
    walk_part(doc_xml, &mut walker, &resolver, &media)?;
    let (mut page_frame, frame_warnings) = read_page_frame(
        &entries,
        &parts,
        &resolver,
        &media,
        &walker.frame_refs,
        walker.title_pg,
    )?;
    let (mirror, at_top) = binding_flags(checked_part(&parts, "word/settings.xml"))?;
    let binding = binding_of(walker.first_gutter, mirror, at_top);
    (page_frame.margins, page_frame.paper) =
        fitting_setup(walker.first_margins, walker.first_paper, binding);
    if page_frame.margins.is_some() {
        page_frame.binding = binding;
    }
    let mut warnings = walker.warnings;
    for note in frame_warnings {
        if !warnings.contains(&note) {
            warnings.push(note);
        }
    }
    if walker.dropped_merges > 0 {
        warnings.push(format!(
            "표의 병합 {}곳은 범위를 벗어나거나 겹쳐 가져오지 않았습니다.",
            walker.dropped_merges
        ));
    }
    Ok((
        RichDocument {
            title,
            blocks: walker.blocks,
            official_agency: None,
            page_frame,
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
