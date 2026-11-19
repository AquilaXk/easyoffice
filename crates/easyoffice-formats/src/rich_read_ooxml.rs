//! DOCX reader producing a [`RichDocument`]: runs (bold/italic/underline/strike), alignment,
//! heading levels (style names and outline levels), lists (list styles and `numPr` through
//! `numbering.xml`: kind, nesting level and number format), and tables at their position in the
//! body.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::decode_budget::DecodeBudget;
use crate::office_codecs::{checked_part, checked_zip_xml_entries};
use crate::rich_document::{toc_bookmark_number, RichColumns};
use crate::rich_page_frame::field_of_instruction;
use crate::rich_read_images::{drawing_picture, DocxMedia, DocxRelationships, DrawingOutcome};
use crate::rich_read_numbering::{read_numbering, Numbering};
use crate::rich_read_page_setup::{
    binding_of, fitting_setup, page_gutter, page_margins, page_paper,
};
use crate::rich_table::{join_cell_paragraphs, TableAssembler};
use crate::rich_xml::{push_linked_run, toggle_on, CharFmt};
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{
    EasyOfficeError, RichBlock, RichDocument, RichField, RichPageMargins, RichPaper, RichRun,
    XmlSanitizer, ZipArchiveCodec, ZipSecurityPolicy, MAX_IMAGES_PER_DOCUMENT,
    MAX_TOTAL_IMAGE_BYTES,
};

#[path = "rich_read_ooxml/frame.rs"]
mod frame;
#[path = "rich_read_ooxml/styles.rs"]
mod styles;
#[path = "rich_read_ooxml/table.rs"]
mod table;
use frame::*;
use styles::*;
use table::{grid_span, DocxTableState, VMerge};

/// A `w:headerReference` or `w:footerReference` of a section.
pub(super) struct FrameRef {
    pub(super) header: bool,
    /// `default`, `first` or `even`.
    pub(super) kind: String,
    pub(super) rel_id: String,
    pub(super) section: usize,
    pub(super) section_title_pg: bool,
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
pub(super) struct DocxWalker {
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
    table_state: DocxTableState,
    cell_paras: Vec<Vec<RichRun>>,
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
    /// Whether the first section shows its own first-page header and footer (`w:titlePg`).
    title_pg: bool,
    section_title_pg: bool,
    cur_section: usize,
    section_cols: Option<RichColumns>,
    sect_block_start: usize,
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
    text_budget: DecodeBudget,
    sdt_stack: Vec<Option<bool>>,
    checkbox_sdt_content_depth: usize,
}

const MAX_SDT_DEPTH: usize = 16;

impl DocxWalker {
    /// Handles an element start; returns true when the whole subtree must be skipped.
    fn on_start(&mut self, name: &str, attrs: &Attrs<'_>) -> Result<bool, EasyOfficeError> {
        if self.frame_mode && name == "w:instrText" {
            self.in_instruction = true;
            return Ok(false);
        }
        if SKIPPED_ELEMENTS.contains(&name) {
            return Ok(true);
        }
        if self.frame_mode && name == DRAWING_ELEMENT {
            self.warn_once("머리말·꼬리말의 그림·도형은 가져오지 않았습니다.".to_string());
            return Ok(true);
        }
        if UNSUPPORTED_OBJECT_ELEMENTS.contains(&name) {
            if !self.warnings.iter().any(|w| w == UNSUPPORTED_OBJECT_NOTE) {
                self.warnings.push(UNSUPPORTED_OBJECT_NOTE.to_string());
            }
            return Ok(true);
        }
        if name == "w:sdt" {
            if self.sdt_stack.len() >= MAX_SDT_DEPTH {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "DOCX 콘텐츠 컨트롤(w:sdt) 중첩 깊이가 상한(16)을 초과했습니다.".to_string(),
                ));
            }
            self.sdt_stack.push(None);
        } else if name == "w14:checkbox" || name.ends_with(":checkbox") {
            if let Some(slot) = self.sdt_stack.last_mut() {
                *slot = Some(slot.unwrap_or(false));
            }
        } else if name == "w14:checked" || name.ends_with(":checked") {
            if let Some(slot @ Some(_)) = self.sdt_stack.last_mut() {
                let val = attr(attrs, "w14:val").or_else(|| attr(attrs, "w:val"));
                *slot = Some(toggle_on(val.as_deref()));
            }
        } else if name == "w:sdtContent" && self.sdt_stack.last().copied().flatten().is_some() {
            self.checkbox_sdt_content_depth += 1;
        }
        self.track_structure(name, attrs);
        self.track_frame(name, attrs)?;
        if self.tbl_depth == 1 && !self.in_ppr && !self.in_rpr {
            self.table_state.on_start(name, attrs);
        }
        if let Some(top) = self.paras.last_mut() {
            if self.in_ppr {
                Self::apply_para_property(top, name, attr(attrs, "w:val").as_deref());
                apply_para_layout(top, name, attrs);
            }
        }
        if self.in_run && self.in_rpr {
            apply_run_property(&mut self.fmt, name, attrs);
        } else if self.in_run && !self.in_ppr {
            self.apply_inline_break(name, attrs)?;
        }
        Ok(false)
    }

    fn track_structure(&mut self, name: &str, attrs: &Attrs<'_>) {
        match name {
            "w:tbl" => {
                self.tbl_depth += 1;
                if self.tbl_depth == 1 {
                    self.table.reset();
                    self.table_state.reset_table();
                }
            }
            "w:tr" if self.tbl_depth == 1 => self.table.start_row(),
            "w:tc" if self.tbl_depth == 1 => {
                self.cell_paras.clear();
                self.cell_span = 1;
                self.cell_v_merge = VMerge::None;
                self.table_state.start_cell();
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
    fn track_frame(&mut self, name: &str, attrs: &Attrs<'_>) -> Result<(), EasyOfficeError> {
        match name {
            "w:sectPr" => {
                self.section_margins = None;
                self.section_paper = None;
                self.section_cols = None;
                self.section_title_pg = false;
            }
            "w:pgMar" => {
                self.section_margins = page_margins(attrs);
                self.section_gutter = page_gutter(attrs);
            }
            "w:pgSz" => self.section_paper = page_paper(attrs),
            "w:cols" => self.section_cols = parse_docx_cols(attrs),
            "w:headerReference" | "w:footerReference" => {
                if let Some(rel_id) = attr(attrs, "r:id") {
                    self.frame_refs.push(FrameRef {
                        header: name == "w:headerReference",
                        kind: attr(attrs, "w:type").map_or("default".to_string(), Cow::into_owned),
                        rel_id: rel_id.into_owned(),
                        section: self.cur_section,
                        section_title_pg: false,
                    });
                }
            }
            "w:titlePg" => self.section_title_pg |= toggle_on(attr(attrs, "w:val").as_deref()),
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
                        self.push_field_run(field)?;
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
        Ok(())
    }

    fn push_field_run(&mut self, field: RichField) -> Result<(), EasyOfficeError> {
        if let Some(top) = self.paras.last_mut() {
            self.text_budget.charge_run(0, "DOCX 페이지 필드")?;
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
        Ok(())
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

    fn apply_inline_break(&mut self, name: &str, attrs: &Attrs<'_>) -> Result<(), EasyOfficeError> {
        if self.frame_mode && name == "w:ptab" {
            if let Some(next_jc) = match attr(attrs, "w:alignment").as_deref() {
                Some("center") => Some("center"),
                Some("right") => Some("right"),
                _ => None,
            } {
                if let Some(top) = self.paras.last_mut() {
                    if !self.cur_text.is_empty() {
                        let link_len = self.link.as_deref().map_or(0, str::len);
                        self.text_budget.charge_run(link_len, "DOCX 런")?;
                        push_linked_run(
                            &mut top.runs,
                            &self.cur_text,
                            &self.fmt,
                            self.link.as_deref(),
                        );
                        self.cur_text.clear();
                    }
                    let prev_jc = top.jc.take().unwrap_or_else(|| "left".to_string());
                    let prev_runs = std::mem::take(&mut top.runs);
                    top.zones.push((prev_jc, prev_runs));
                    top.jc = Some(next_jc.to_string());
                    return Ok(());
                }
            }
        }
        let ch = match name {
            "w:tab" | "w:ptab" => Some('\t'),
            "w:br" if attr(attrs, "w:type").as_deref() == Some("page") => {
                self.note_page_break();
                None
            }
            "w:br" if attr(attrs, "w:type").as_deref() != Some("column") => Some('\n'),
            "w:cr" => Some('\n'),
            "w:noBreakHyphen" => Some('-'),
            _ => None,
        };
        if let Some(c) = ch {
            self.text_budget.charge(1, "DOCX 제어 문자")?;
            self.cur_text.push(c);
        }
        Ok(())
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

    fn on_text(&mut self, text: &str) -> Result<(), EasyOfficeError> {
        if self.checkbox_sdt_content_depth > 0 {
            self.text_budget.charge(text.len(), "DOCX 확인란 기호")?;
            return Ok(());
        }
        if self.in_t {
            self.text_budget.charge(text.len(), "DOCX 텍스트")?;
            self.cur_text.push_str(text);
        } else if self.in_instruction {
            if let Some(instruction) = self.field_instruction.as_mut() {
                self.text_budget.charge(text.len(), "DOCX 필드")?;
                instruction.push_str(text);
            }
        }
        Ok(())
    }

    fn on_end(&mut self, name: &str, resolver: &Resolver<'_>) -> Result<(), EasyOfficeError> {
        match name {
            "w:sdtContent" if self.sdt_stack.last().copied().flatten().is_some() => {
                self.checkbox_sdt_content_depth = self.checkbox_sdt_content_depth.saturating_sub(1);
            }
            "w:sdt" => {
                if let Some(Some(checked)) = self.sdt_stack.pop() {
                    if let Some(top) = self.paras.last_mut() {
                        if top.task_checkbox.is_none() && top.runs.is_empty() {
                            top.task_checkbox = Some(checked);
                            top.strip_task_space = true;
                        }
                    }
                }
            }
            "w:t" => self.in_t = false,
            "w:instrText" => self.in_instruction = false,
            "w:fldSimple" => self.simple_field = None,
            "w:sectPr" => {
                if self.first_margins.is_none() && self.section_margins.is_some() {
                    self.first_gutter = self.section_gutter;
                }
                self.first_margins = self.first_margins.or(self.section_margins);
                self.first_paper = self.first_paper.or(self.section_paper);
                let sec = self.cur_section;
                let tpg = self.section_title_pg;
                if sec == 0 {
                    self.title_pg = tpg;
                }
                for r in self.frame_refs.iter_mut().rev() {
                    if r.section != sec {
                        break;
                    }
                    r.section_title_pg = tpg;
                }
                self.cur_section += 1;
                let cols = self.section_cols.take();
                if let Some(top) = self.paras.last_mut() {
                    top.sect_end = Some(cols);
                } else {
                    self.apply_section_cols(cols);
                }
            }
            "w:rPr" => self.in_rpr = false,
            "w:pPr" => self.in_ppr = false,
            "w:r" => {
                if let Some((field, written)) = self.simple_field {
                    if !written {
                        self.push_field_run(field)?;
                        self.simple_field = Some((field, true));
                    }
                } else if self.result_field.is_none() && self.checkbox_sdt_content_depth == 0 {
                    if let Some(top) = self.paras.last_mut() {
                        if top.strip_task_space && !self.cur_text.is_empty() {
                            top.strip_task_space = false;
                            if self.cur_text == " " {
                                self.cur_text.clear();
                            } else if let Some(rest) = self.cur_text.strip_prefix(' ') {
                                self.cur_text = rest.to_string();
                            }
                        }
                        if !self.cur_text.is_empty() {
                            let link_len = self.link.as_deref().map_or(0, str::len);
                            self.text_budget.charge_run(link_len, "DOCX 런")?;
                        }
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
            "w:p" => self.finish_paragraph(resolver)?,
            "w:tblPr" | "w:tblGrid" | "w:tcBorders" | "w:tcPr"
                if self.tbl_depth == 1 && !self.in_ppr && !self.in_rpr =>
            {
                self.table_state.on_end(name);
            }
            "w:tc" if self.tbl_depth == 1 => {
                let (text, spans) = join_cell_paragraphs(&self.cell_paras);
                self.cell_paras.clear();
                self.text_budget.charge_cell(text.len(), "DOCX 표 셀")?;
                let (row, col) = self.table.next_coord();
                let (fill, borders, images) = self.table_state.finish_cell(self.cell_span);
                if self.cell_v_merge != VMerge::Continue {
                    if !spans.is_empty() {
                        self.table.push_cell_links(row, col, &spans);
                    }
                    self.table.push_cell_style(row, col, fill, borders);
                    if !images.is_empty() {
                        self.table.push_cell_images(row, col, images);
                    }
                }
                self.push_cell(text)?;
            }
            "w:tr" if self.tbl_depth == 1 => {
                self.table.end_row();
                self.table_state.finish_row();
            }
            "w:tbl" => {
                self.tbl_depth = self.tbl_depth.saturating_sub(1);
                if self.tbl_depth == 0 && !self.table.is_empty() {
                    self.text_budget.charge_block(0, "DOCX 표")?;
                    let (mut table, dropped) =
                        std::mem::take(&mut self.table).finish(&mut self.text_budget)?;
                    if let Some(widths) = self.table_state.resolve_col_widths(table.column_count())
                    {
                        table.col_widths_twips = widths;
                    }
                    self.dropped_merges += dropped;
                    self.blocks.push(RichBlock::Table(table));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn push_cell(&mut self, text: String) -> Result<(), EasyOfficeError> {
        let span = self.cell_span;
        match self.cell_v_merge {
            VMerge::None => self.table.spanning(text, span, &mut self.text_budget)?,
            VMerge::Restart => self.table.restart(text, span, &mut self.text_budget)?,
            VMerge::Continue => self
                .table
                .continue_above(text, span, &mut self.text_budget)?,
        }
        Ok(())
    }

    /// Records the picture of a `w:drawing` in the open paragraph or table cell, or a warning when skipped.
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
        if self.tbl_depth > 1
            || (self.tbl_depth == 1 && !self.table_state.in_cell)
            || self.paras.is_empty()
        {
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
        if self.tbl_depth == 1 {
            self.table_state.cell_images.push(img);
        } else if let Some(top) = self.paras.last_mut() {
            top.images.push(img);
        }
        Ok(())
    }

    fn warn_once(&mut self, note: String) {
        if !self.warnings.contains(&note) {
            self.warnings.push(note);
        }
    }

    fn apply_section_cols(&mut self, cols: Option<RichColumns>) {
        if cols.is_some() {
            for block in &mut self.blocks[self.sect_block_start..] {
                if let RichBlock::Paragraph(p) = block {
                    p.columns = cols;
                }
            }
        }
        self.sect_block_start = self.blocks.len();
    }

    fn finish_paragraph(&mut self, resolver: &Resolver<'_>) -> Result<(), EasyOfficeError> {
        let Some(mut p) = self.paras.pop() else {
            return Ok(());
        };
        let sect_end = p.sect_end.take();
        let images = std::mem::take(&mut p.images);
        if self.tbl_depth > 0 {
            self.cell_paras.push(p.runs);
        } else if !p.zones.is_empty() {
            let mut zones = std::mem::take(&mut p.zones);
            let last_jc = p.jc.take().unwrap_or_else(|| "left".to_string());
            let last_runs = std::mem::take(&mut p.runs);
            zones.push((last_jc, last_runs));
            for (jc, runs) in zones {
                if runs
                    .iter()
                    .any(|r| r.field.is_some() || !r.text.trim().is_empty())
                {
                    self.text_budget.charge_block(0, "DOCX 문단")?;
                    let zone_state = ParaState {
                        runs,
                        style_id: p.style_id.clone(),
                        jc: Some(jc),
                        ..ParaState::default()
                    };
                    self.blocks
                        .push(RichBlock::Paragraph(resolver.finish(zone_state)));
                }
            }
            self.blocks.extend(images.into_iter().map(RichBlock::Image));
        } else {
            let carried = p.page_break_before;
            let after = p.page_break_after;
            let mut kept = false;
            if p.runs
                .iter()
                .any(|r| r.field.is_some() || !r.text.trim().is_empty())
            {
                self.text_budget.charge_block(0, "DOCX 문단")?;
                let mut paragraph = resolver.finish(p);
                paragraph.page_break_before |= std::mem::take(&mut self.pending_page_break);
                self.blocks.push(RichBlock::Paragraph(paragraph));
                kept = true;
            }
            self.pending_page_break |= (carried && !kept) || after;
            self.blocks.extend(images.into_iter().map(RichBlock::Image));
            if let Some(cols) = sect_end {
                self.apply_section_cols(cols);
            }
        }
        Ok(())
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
                if walker.on_start(name, attrs)? {
                    reader.skip_subtree()?;
                }
            }
            XmlEvent::Text(text) => walker.on_text(&text)?,
            XmlEvent::End(name) => walker.on_end(name, resolver)?,
        }
    }
    Ok(())
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
    let docx_rels = DocxRelationships::load(&entries)?;
    let media = docx_rels.media;
    let mut walker = DocxWalker {
        links: docx_rels.links,
        ..DocxWalker::default()
    };
    walk_part(doc_xml, &mut walker, &resolver, &media)?;
    let settings = read_word_settings(checked_part(&parts, "word/settings.xml"))?;
    let (mut page_frame, frame_warnings) = read_page_frame_with_targets(
        &docx_rels.frame_targets,
        &parts,
        &resolver,
        &media,
        &walker.frame_refs,
        walker.title_pg,
        settings,
    )?;
    let binding = binding_of(
        walker.first_gutter,
        settings.mirror_margins,
        settings.gutter_at_top,
    );
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
#[path = "rich_read_ooxml/tests.rs"]
mod tests;
