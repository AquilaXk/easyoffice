//! HWPX reader producing a [`RichDocument`]: character shapes (`hh:charPr`) become run formats,
//! paragraph shapes (`hh:paraPr`) alignment and list kind, styles (`hh:style`, "개요 N") heading
//! levels, and `hp:tbl` tables keep their position in the section.

use std::borrow::Cow;

mod frame;
mod header;
mod objects;
mod table_style;

use self::header::{read_header, Header};
use self::objects::{is_drawing_shape, HwpxMedia, NoteFormatSpec, OLE_SKIP_WARNING};

use crate::decode_budget::DecodeBudget;
use crate::hwpx_codec::section_xml;
use crate::hwpx_parts::{
    DEFAULT_CHAR_HEIGHT, DEFAULT_LINE_SPACING, HWPUNIT_PER_TWIP, LIST_LEFT_MARGIN,
};
use crate::office_codecs::checked_zip_xml_entries_for;
use crate::rich_document::RichColumns;
use crate::rich_image::ImageBudget;
use crate::rich_table::{join_cell_paragraphs, table_from_placed_with_layout, PlacedCell};
use crate::rich_table_style::{CellFill, TableCellBorders};
use crate::rich_xml::{check_paragraph_depth, heading_level_from_name, push_linked_run, CharFmt};
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{
    DocumentFormat, EasyOfficeError, HwpxCodec, RichAlignment, RichBinding, RichBlock,
    RichDocument, RichImage, RichListKind, RichPageFrame, RichPageMargins, RichPaper,
    RichParagraph, RichRun, ZipArchiveCodec, ZipSecurityPolicy,
};

/// EMU per HWPUNIT (1/7200 inch).
pub(super) const EMU_PER_HWPUNIT: u64 = 127;

#[derive(Default)]
struct ParaState {
    runs: Vec<RichRun>,
    para_pr: String,
    style: String,
    images: Vec<RichImage>,
    after_blocks: Vec<RichBlock>,
    columns: Option<RichColumns>,
}

struct RunState {
    char_pr: String,
    text: String,
}

fn finish_paragraph(header: &Header, p: ParaState) -> RichParagraph {
    let style = header.styles.get(&p.style);
    let style_heading = style.map_or(0, |s| {
        heading_level_from_name(&s.name).max(heading_level_from_name(&s.eng_name))
    });
    // The paragraph's own shape wins; fall back to the style's shape.
    let shape = header
        .paras
        .get(&p.para_pr)
        .or_else(|| style.and_then(|s| header.paras.get(&s.para_pr)))
        .cloned()
        .unwrap_or_default();
    let mut heading = style_heading;
    if heading == 0 && shape.heading_type == "OUTLINE" {
        heading = (shape.heading_level + 1).min(6);
    }
    let marker_margin = shape.left == LIST_LEFT_MARGIN;
    let (list, task_checked) = if heading > 0 {
        (None, false)
    } else {
        match shape.heading_type.as_str() {
            "BULLET" => match header
                .numberings
                .bullet_task(shape.heading_id_ref, shape.checked)
            {
                Some(checked) => (Some(RichListKind::Task), checked),
                None => (Some(RichListKind::Bullet), false),
            },
            "NUMBER" => (Some(RichListKind::Ordered), false),
            _ if marker_margin => (Some(RichListKind::Bullet), false),
            _ => (None, false),
        }
    };
    let list_level = if list.is_some() {
        shape.heading_level.min(8)
    } else {
        0
    };
    let list_number = (list == Some(RichListKind::Ordered))
        .then(|| header.numberings.get(shape.heading_id_ref, list_level))
        .flatten();
    let default_list_margin =
        list.is_some() && shape.left == LIST_LEFT_MARGIN * (u32::from(list_level) + 1);
    let to_twips = |hwpunit: i64| i32::try_from(hwpunit / i64::from(HWPUNIT_PER_TWIP)).ok();
    let left_indent_twips = (!marker_margin && !default_list_margin && shape.left > 0)
        .then(|| to_twips(i64::from(shape.left)))
        .flatten()
        .and_then(|t| u32::try_from(t).ok());
    let right_indent_twips = (shape.right > 0)
        .then(|| to_twips(i64::from(shape.right)))
        .flatten()
        .and_then(|t| u32::try_from(t).ok());
    let first_line_indent_twips = (shape.intent != 0)
        .then(|| to_twips(i64::from(shape.intent)))
        .flatten();
    let line_spacing = shape.line_spacing.filter(|s| *s != DEFAULT_LINE_SPACING);
    let body_default_justify = heading == 0
        && list.is_none()
        && style.is_some_and(|s| {
            (s.name == "바탕글" || s.eng_name == "Normal") && s.para_pr == p.para_pr
        });
    let alignment = match shape.align.as_deref() {
        Some("LEFT") => Some(RichAlignment::Left),
        Some("CENTER") => Some(RichAlignment::Center),
        Some("RIGHT") => Some(RichAlignment::Right),
        Some("JUSTIFY") if body_default_justify => None,
        Some("JUSTIFY" | "DISTRIBUTE" | "DISTRIBUTE_SPACE") => Some(RichAlignment::Justify),
        _ => None,
    };
    // Heading styles are bold and sized by definition, so those are not user formatting there;
    // body text at the writer's default height inherits the default size.
    let style_fmt = style.and_then(|s| header.chars.get(&s.char_pr));
    let style_bold = style_fmt.is_some_and(|f| f.bold);
    let inherited_size = if heading > 0 {
        style_fmt.and_then(|f| f.size_half_pt)
    } else {
        u16::try_from(DEFAULT_CHAR_HEIGHT / 50).ok()
    };
    let mut runs: Vec<RichRun> = Vec::new();
    for r in p.runs {
        let fmt = CharFmt {
            bold: r.bold && !(heading > 0 && style_bold),
            italic: r.italic,
            underline: r.underline,
            strike: r.strike,
            size_half_pt: r.size_half_pt.filter(|s| Some(*s) != inherited_size),
            font: r.font,
            font_east_asia: r.font_east_asia,
            color: r.color,
            highlight: r.highlight,
            script: r.script,
            letter_spacing_twips: r.letter_spacing_twips,
            advanced: r.advanced,
        };
        if r.field.is_some() {
            runs.push(RichRun {
                bold: fmt.bold,
                italic: fmt.italic,
                underline: fmt.underline,
                strike: fmt.strike,
                size_half_pt: fmt.size_half_pt,
                font: fmt.font,
                font_east_asia: fmt.font_east_asia,
                color: fmt.color,
                highlight: fmt.highlight,
                script: fmt.script,
                letter_spacing_twips: fmt.letter_spacing_twips,
                advanced: fmt.advanced,
                field: r.field,
                ..RichRun::default()
            });
        } else {
            push_linked_run(&mut runs, &r.text, &fmt, r.link.as_deref());
        }
    }
    RichParagraph {
        list_level,
        list_number,
        task_checked,
        runs,
        alignment,
        heading_level: heading,
        list,
        left_indent_twips,
        right_indent_twips,
        first_line_indent_twips,
        line_spacing,
        columns: p.columns,
        ..RichParagraph::default()
    }
}

/// A non-negative integer attribute (`default` when absent or unreadable, saturating when huge).
fn count_attr(attrs: &Attrs<'_>, name: &str, default: u32) -> u32 {
    attr(attrs, name)
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(default, |v| u32::try_from(v).unwrap_or(u32::MAX))
}

/// Walker state for one `Contents/section*.xml`.
#[derive(Default)]
struct SectionWalker {
    paras: Vec<ParaState>,
    runs: Vec<RunState>,
    t_depth: usize,
    tbl_depth: usize,
    in_cell: bool,
    tbl_no_adjust: bool,
    placed: Vec<PlacedCell>,
    table_images: Vec<RichImage>,
    table_captions: Vec<RichBlock>,
    /// Index of the open `hp:tr` and the column after the previous cell (positions of cells
    /// that carry no `hp:cellAddr`).
    tr_index: u32,
    next_col: u32,
    cell_addr: Option<(u32, u32)>,
    cell_span: (u32, u32),
    cell_border_fill_id: Option<String>,
    cell_sz_width: Option<u32>,
    cell_images: Vec<RichImage>,
    /// Merged regions and cells dropped as invalid, reported once per document.
    dropped_merges: usize,
    cell_paras: Vec<Vec<RichRun>>,
    budget: ImageBudget,
    text_budget: DecodeBudget,
    /// Address of the open hyperlink field (`hp:fieldBegin type="HYPERLINK"` to `hp:fieldEnd`).
    link: Option<String>,
    current_columns: Option<RichColumns>,
    /// The header and footer of the page (`BOTH`/`ODD`/`EVEN` controls) and what was left out.
    frame: RichPageFrame,
    frame_found: [bool; 6],
    /// Page setup of the first section that gives one.
    margins: Option<RichPageMargins>,
    paper: Option<RichPaper>,
    binding: RichBinding,
    left_out: frame::FrameLeftOut,
    /// Footnote and endnote formatting + collected appendix blocks across sections.
    footnote_spec: NoteFormatSpec,
    endnote_spec: NoteFormatSpec,
    next_footnote: usize,
    next_endnote: usize,
    total_notes: usize,
    footnotes: Vec<RichBlock>,
    endnotes: Vec<RichBlock>,
    nest_depth: u8,
    in_note: bool,
}

impl SectionWalker {
    fn on_start(
        &mut self,
        name: &str,
        attrs: &Attrs<'_>,
        header: &Header,
        blocks: &mut Vec<RichBlock>,
    ) -> Result<(), EasyOfficeError> {
        match name {
            "hp:p" => {
                check_paragraph_depth(self.paras.len())?;
                self.paras.push(ParaState {
                    runs: Vec::new(),
                    para_pr: attr(attrs, "paraPrIDRef").unwrap_or_default().into_owned(),
                    style: attr(attrs, "styleIDRef").unwrap_or_default().into_owned(),
                    images: Vec::new(),
                    after_blocks: Vec::new(),
                    columns: (self.tbl_depth == 0 && !self.in_note)
                        .then_some(self.current_columns)
                        .flatten(),
                });
            }
            "hp:run" => self.runs.push(RunState {
                char_pr: attr(attrs, "charPrIDRef").unwrap_or_default().into_owned(),
                text: String::new(),
            }),
            "hp:t" => self.t_depth += 1,
            "hp:tab" if self.t_depth > 0 => {
                push_text(&mut self.runs, "\t", &mut self.text_budget)?;
            }
            "hp:lineBreak" if self.t_depth > 0 => {
                push_text(&mut self.runs, "\n", &mut self.text_budget)?;
            }
            "hp:nbSpace" | "hp:fwSpace" if self.t_depth > 0 => {
                push_text(&mut self.runs, " ", &mut self.text_budget)?;
            }
            "hp:hyphen" if self.t_depth > 0 => {
                push_text(&mut self.runs, "-", &mut self.text_budget)?;
            }
            "hp:tbl" => {
                if self.tbl_depth == 0 {
                    self.flush_text_before_table(header, blocks)?;
                }
                self.tbl_depth += 1;
                if self.tbl_depth == 1 {
                    self.placed.clear();
                    self.table_images.clear();
                    self.table_captions.clear();
                    self.in_cell = false;
                    self.tbl_no_adjust = attr(attrs, "noAdjust").as_deref() == Some("1");
                    self.tr_index = 0;
                }
            }
            "hp:tr" if self.tbl_depth == 1 => self.next_col = 0,
            "hp:tc" if self.tbl_depth == 1 => {
                self.in_cell = true;
                self.cell_paras.clear();
                self.cell_addr = None;
                self.cell_span = (1, 1);
                self.cell_border_fill_id = attr(attrs, "borderFillIDRef").map(Cow::into_owned);
                self.cell_sz_width = None;
                self.cell_images.clear();
            }
            "hp:cellAddr" if self.tbl_depth == 1 => {
                self.cell_addr = Some((
                    count_attr(attrs, "rowAddr", 0),
                    count_attr(attrs, "colAddr", 0),
                ));
            }
            "hp:cellSpan" if self.tbl_depth == 1 => {
                self.cell_span = (
                    count_attr(attrs, "rowSpan", 1).max(1),
                    count_attr(attrs, "colSpan", 1).max(1),
                );
            }
            "hp:cellSz" if self.tbl_depth == 1 && self.in_cell && self.cell_sz_width.is_none() => {
                self.cell_sz_width = attr(attrs, "width")
                    .and_then(|v| v.trim().parse::<u64>().ok())
                    .map(|hu| hu / u64::from(HWPUNIT_PER_TWIP))
                    .and_then(|tw| u32::try_from(tw).ok());
            }
            _ => {}
        }
        Ok(())
    }

    fn on_text(&mut self, text: &str) -> Result<(), EasyOfficeError> {
        if self.t_depth > 0 {
            push_text(&mut self.runs, text, &mut self.text_budget)?;
        }
        Ok(())
    }

    fn on_end(
        &mut self,
        name: &str,
        header: &Header,
        blocks: &mut Vec<RichBlock>,
    ) -> Result<(), EasyOfficeError> {
        match name {
            "hp:t" => self.t_depth = self.t_depth.saturating_sub(1),
            "hp:run" => {
                self.flush_run(header)?;
                self.runs.pop();
            }
            "hp:p" => {
                self.link = None;
                self.finish_paragraph(header, blocks)?;
            }
            "hp:tc" if self.tbl_depth == 1 => {
                self.in_cell = false;
                let (row, col) = self.cell_addr.unwrap_or((self.tr_index, self.next_col));
                let (row_span, col_span) = self.cell_span;
                self.next_col = col.saturating_add(col_span);
                let (text, links) = join_cell_paragraphs(&self.cell_paras);
                self.text_budget.charge_cell(text.len(), "HWPX 표 셀")?;
                let style = self
                    .cell_border_fill_id
                    .take()
                    .and_then(|id| header.border_fills.get(&id).copied())
                    .unwrap_or((CellFill::none(), TableCellBorders::default()));
                self.placed.push(PlacedCell {
                    row,
                    col,
                    row_span,
                    col_span,
                    text,
                    links,
                    width_twips: self.cell_sz_width.take(),
                    style,
                    images: std::mem::take(&mut self.cell_images),
                });
                self.cell_paras.clear();
            }
            "hp:tr" if self.tbl_depth == 1 => self.tr_index = self.tr_index.saturating_add(1),
            "hp:tbl" => {
                self.tbl_depth = self.tbl_depth.saturating_sub(1);
                if self.tbl_depth == 0 {
                    if !self.placed.is_empty() {
                        self.text_budget.charge_block(0, "HWPX 표")?;
                        let (table, dropped) = table_from_placed_with_layout(
                            std::mem::take(&mut self.placed),
                            self.tbl_no_adjust,
                            &mut self.text_budget,
                        )?;
                        self.dropped_merges += dropped;
                        blocks.push(RichBlock::Table(table));
                    }
                    blocks.extend(self.table_images.drain(..).map(RichBlock::Image));
                    blocks.append(&mut self.table_captions);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Moves the text of the open run into the paragraph, as a run of the open hyperlink if there is one.
    pub(super) fn flush_run(&mut self, header: &Header) -> Result<(), EasyOfficeError> {
        let (Some(run), Some(top)) = (self.runs.last_mut(), self.paras.last_mut()) else {
            return Ok(());
        };
        let fmt = header.chars.get(&run.char_pr).cloned().unwrap_or_default();
        let text = std::mem::take(&mut run.text);
        if !text.is_empty() {
            let link_len = self.link.as_deref().map_or(0, str::len);
            self.text_budget.charge_run(link_len, "HWPX 런")?;
        }
        push_linked_run(&mut top.runs, &text, &fmt, self.link.as_deref());
        Ok(())
    }

    fn finish_paragraph(
        &mut self,
        header: &Header,
        blocks: &mut Vec<RichBlock>,
    ) -> Result<(), EasyOfficeError> {
        let Some(p) = self.paras.pop() else {
            return Ok(());
        };
        if self.tbl_depth > 0 {
            self.cell_paras.push(p.runs);
            return Ok(());
        }
        push_paragraph_blocks(header, p, blocks, &mut self.text_budget)
    }

    /// A table is written inside the paragraph that holds it, but the paragraph is only complete
    /// at its end tag, after the table. The text (and pictures) before the table become their
    /// own paragraph now, so the table block follows them; what comes after the table is read
    /// as a paragraph of its own.
    fn flush_text_before_table(
        &mut self,
        header: &Header,
        blocks: &mut Vec<RichBlock>,
    ) -> Result<(), EasyOfficeError> {
        self.flush_run(header)?;
        let Some(top) = self.paras.last_mut() else {
            return Ok(());
        };
        let before = ParaState {
            runs: std::mem::take(&mut top.runs),
            para_pr: top.para_pr.clone(),
            style: top.style.clone(),
            images: std::mem::take(&mut top.images),
            after_blocks: std::mem::take(&mut top.after_blocks),
            columns: top.columns,
        };
        push_paragraph_blocks(header, before, blocks, &mut self.text_budget)
    }
}

/// The paragraph block (when it has text), its pictures and any trailing caption blocks.
fn push_paragraph_blocks(
    header: &Header,
    mut p: ParaState,
    blocks: &mut Vec<RichBlock>,
    text_budget: &mut DecodeBudget,
) -> Result<(), EasyOfficeError> {
    let images = std::mem::take(&mut p.images);
    let after_blocks = std::mem::take(&mut p.after_blocks);
    if p.runs
        .iter()
        .any(|r| r.field.is_some() || !r.text.trim().is_empty())
    {
        text_budget.charge_block(0, "HWPX 문단")?;
        blocks.push(RichBlock::Paragraph(finish_paragraph(header, p)));
    }
    blocks.extend(images.into_iter().map(RichBlock::Image));
    blocks.extend(after_blocks);
    Ok(())
}

fn read_section(
    xml: &str,
    header: &Header,
    media: &HwpxMedia<'_>,
    walker: &mut SectionWalker,
    blocks: &mut Vec<RichBlock>,
    warnings: &mut Vec<String>,
) -> Result<(), EasyOfficeError> {
    let mut reader = XmlReader::new(xml);
    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Start { name: "hp:pic", .. } => {
                let picture = reader.collect_subtree(event)?;
                walker.on_picture(&picture, header, media, blocks, warnings)?;
            }
            XmlEvent::Start {
                name: "hp:equation",
                ..
            } => {
                let eq = reader.collect_subtree(event)?;
                walker.on_equation(&eq, header, media, blocks, warnings)?;
            }
            XmlEvent::Start {
                name: "hp:ctrl", ..
            } => {
                let control = reader.collect_subtree(event)?;
                if !walker.on_frame_control(&control, header)?
                    && !walker.on_note_control(&control, header, media, warnings)?
                {
                    walker.on_ctrl(&control, header)?;
                }
            }
            XmlEvent::Start {
                name: "hp:secPr", ..
            } => {
                let properties = reader.collect_subtree(event)?;
                walker.on_section_properties(&properties);
            }
            XmlEvent::Start {
                name: "hp:caption", ..
            } if walker.tbl_depth == 1 && !walker.in_cell => {
                let cap = reader.collect_subtree(event)?;
                let cap_blocks = walker.extract_caption_blocks(&cap, header, media, warnings)?;
                walker.table_captions.extend(cap_blocks);
            }
            XmlEvent::Start { name, .. } if is_drawing_shape(name) => {
                let shape = reader.collect_subtree(event)?;
                walker.on_shape_or_container(&shape, header, media, blocks, warnings)?;
            }
            XmlEvent::Start {
                name: "hp:ole" | "hp:switch",
                ..
            } => {
                if !warnings.iter().any(|w| w == OLE_SKIP_WARNING) {
                    warnings.push(OLE_SKIP_WARNING.to_string());
                }
                reader.skip_subtree()?;
            }
            XmlEvent::Start { name, ref attrs } => {
                walker.on_start(name, attrs, header, blocks)?;
            }
            XmlEvent::Text(text) => walker.on_text(&text)?,
            XmlEvent::End(name) => walker.on_end(name, header, blocks)?,
        }
    }
    Ok(())
}

/// The page frame read, with the page setup of the first section.
fn finish_frame(walker: &mut SectionWalker) -> RichPageFrame {
    let mut frame = std::mem::take(&mut walker.frame);
    frame.margins = walker.margins;
    frame.paper = walker.paper;
    frame.binding = walker.binding;
    frame
}

fn push_text(
    runs: &mut [RunState],
    text: &str,
    budget: &mut DecodeBudget,
) -> Result<(), EasyOfficeError> {
    if let Some(run) = runs.last_mut() {
        budget.charge(text.len(), "HWPX 텍스트")?;
        run.text.push_str(text);
    }
    Ok(())
}

/// Decodes every `Contents/section*.xml` of a HWPX/HWTX package in order.
pub(crate) fn read_hwpx(bytes: &[u8]) -> Result<(RichDocument, Vec<String>), EasyOfficeError> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
    let parts = checked_zip_xml_entries_for(&entries, Some(DocumentFormat::Hwpx))?;
    let (header_xml, title, _is_template, official_agency, _approval_lines) =
        HwpxCodec::extract_header_and_metadata(&parts)?;
    let header = read_header(header_xml)?;
    let mut sections: Vec<(usize, &crate::ZipArchiveEntry)> = entries
        .iter()
        .filter_map(|e| {
            let idx = e
                .path
                .strip_prefix("Contents/section")
                .and_then(|s| s.strip_suffix(".xml"))
                .and_then(|s| s.parse::<usize>().ok())?;
            Some((idx, e))
        })
        .collect();
    sections.sort_by_key(|(idx, _)| *idx);
    if sections.is_empty() {
        return Err(EasyOfficeError::CorruptedContainer(
            "HWPX 필수 엔트리 'Contents/section0.xml'이 누락되었습니다.".to_string(),
        ));
    }
    let mut blocks = Vec::new();
    let mut warnings = Vec::new();
    let media = HwpxMedia::load(&entries)?;
    let mut walker = SectionWalker::default();
    for (_, entry) in sections {
        let xml = section_xml(&parts, &entry.path)?;
        read_section(
            xml,
            &header,
            &media,
            &mut walker,
            &mut blocks,
            &mut warnings,
        )?;
    }
    for (title_text, note_blocks) in [
        ("각주", std::mem::take(&mut walker.footnotes)),
        ("미주", std::mem::take(&mut walker.endnotes)),
    ] {
        if !note_blocks.is_empty() {
            blocks.push(RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain(title_text)],
                heading_level: 2,
                ..RichParagraph::default()
            }));
            blocks.extend(note_blocks);
        }
    }
    if walker.dropped_merges > 0 {
        warnings.push(format!(
            "표의 병합·셀 {}곳은 범위를 벗어나거나 겹쳐 가져오지 않았습니다.",
            walker.dropped_merges
        ));
    }
    for (left_out, note) in [
        (
            walker.left_out.pages,
            "홀수·짝수 쪽 머리말·꼬리말은 가져오지 않았습니다.",
        ),
        (
            walker.left_out.sections,
            "구역마다 다른 머리말·꼬리말은 첫 구역의 것만 가져왔습니다.",
        ),
        (
            walker.left_out.objects,
            "머리말·꼬리말의 표·그림·도형은 가져오지 않았습니다.",
        ),
        (
            walker.left_out.long,
            "머리말·꼬리말은 문단 64개까지만 가져왔습니다.",
        ),
    ] {
        if left_out {
            warnings.push(note.to_string());
        }
    }
    if official_agency.is_some() {
        warnings.push("기관명 등 공문서 메타데이터는 저장 시 다시 기록되지 않습니다.".to_string());
    }
    Ok((
        RichDocument {
            page_frame: finish_frame(&mut walker),
            title,
            blocks,
            official_agency,
            ..RichDocument::default()
        },
        warnings,
    ))
}

#[cfg(test)]
use frame::link_from_command;

#[cfg(test)]
#[path = "rich_read_hwpx/tests.rs"]
mod tests;
