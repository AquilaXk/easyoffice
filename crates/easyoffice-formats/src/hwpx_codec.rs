//! KS X 6101 (OWPML) HWPX container codec for `.hwpx` and `.hwtx`.
//!
//! The writer stores `mimetype` first (uncompressed), lists every part in both
//! `META-INF/manifest.xml` and `Contents/content.hpf`, writes a populated `header.xml`
//! (fonts, character/paragraph shapes, styles), `version.xml`, and `settings.xml`, places
//! `hp:secPr` in the first paragraph of `section0.xml`, references shape IDs from every paragraph
//! and run, and wraps tables as `hp:p > hp:run > hp:tbl` with `hp:tc > hp:subList > hp:p` cells.
//! Document metadata (template flag, issuing agency, approval lines) lives in OPF `opf:meta`.

use crate::hwpx_parts::{
    header_xml, header_xml_with_all, hwpx_col_ctrl_xml, manifest_xml, section_properties_xml,
    text_width, HwpxCharShape, HwpxParaShape, HwpxStyle, CELL_HEIGHT, CONTAINER_XML,
    DEFAULT_CHAR_HEIGHT, DEFAULT_FONT_FACE, DEFAULT_LINE_PERCENT, HEADING_BULLET, HEADING_NONE,
    HEADING_NUMBER, HEADING_OUTLINE, HWPUNIT_PER_TWIP, LIST_LEFT_MARGIN, MAX_HWPX_BORDER_FILLS,
    NS_CORE, NS_PARAGRAPH, NS_SECTION, SETTINGS_XML, VERSION_XML,
};
pub(crate) use crate::hwpx_unpack::section_xml;
use crate::office_codecs::checked_part;
use crate::rich_document::{
    frame_paragraph, safe_link_target, RichAlignment, RichBinding, RichBlock, RichColumns,
    RichDocument, RichField, RichPageMargins, RichPaper, RichParagraph,
};
use crate::rich_image::{check_document_images, RichImage};
use crate::rich_image_writers::{hwpx_picture_run, MediaParts};
use crate::rich_link_writers::{external_link_groups, hwpx_link_begin, hwpx_link_end};
use crate::rich_list_writers::{
    hwpx_numberings_xml, hwpx_ordered_run_ids, list_kind_of, plan_lists, HWPX_MAX_LIST_LEVEL,
};
use crate::rich_notes::{is_deleted_run, RichNoteKind};
use crate::rich_page_frame::{
    hwpx_frame_control_typed, hwpx_number_control, hwpx_page_hiding_control,
};
use crate::rich_table::{CellSpan, RichTable, TableCellLink};
use crate::rich_table_style::{next_cell_style, slice_cell_images, CellFill, TableCellBorders};
use crate::rich_table_writers::slice_cell_links;
use crate::rtf_xml_helpers::{
    extract_attr, extract_attr_from_fragment, extract_tag_bodies, extract_tags_with_attrs,
};
use crate::{EasyOfficeError, XmlSanitizer, ZipArchiveCodec};

/// OPF metadata entry marking a document as a template (`.hwtx`).
pub const HWPX_TEMPLATE_META_MARKER: &str =
    "<opf:meta name=\"isTemplate\" content=\"text\">1</opf:meta>";

/// Directory of embedded pictures inside the package.
const HWPX_MEDIA_DIR: &str = "BinData";

/// `opf:item` manifest entries for embedded pictures (`image1`, `image2`, ...).
fn hpf_media_items(media: &MediaParts) -> String {
    media
        .listing()
        .iter()
        .enumerate()
        .map(|(idx, (path, mime))| {
            format!(
                "<opf:item id=\"image{}\" href=\"{path}\" media-type=\"{mime}\" isEmbeded=\"1\"/>",
                idx + 1
            )
        })
        .collect()
}

const META_AGENCY: &str = "officialAgency";
const META_APPROVAL: &str = "approvalLines";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HwpxPayload {
    pub title: String,
    pub is_template: bool,
    pub official_agency: Option<String>,
    pub approval_lines: Vec<String>,
    pub paragraphs: Vec<String>,
    pub tables: Vec<Vec<Vec<String>>>,
}

/// The shape tables of `header.xml` (fonts, character and paragraph shapes, styles) and the paragraph
/// writer that fills them.
struct ShapeTables {
    char_shapes: Vec<HwpxCharShape>,
    char_map: std::collections::HashMap<HwpxCharShape, usize>,
    para_shapes: Vec<HwpxParaShape>,
    para_map: std::collections::HashMap<HwpxParaShape, usize>,
    styles: Vec<HwpxStyle>,
    style_map: std::collections::HashMap<u8, usize>,
    fonts: Vec<String>,
    font_map: std::collections::HashMap<String, usize>,
    border_fills: Vec<(CellFill, TableCellBorders)>,
    border_fill_map: std::collections::HashMap<(CellFill, TableCellBorders), u32>,
    next_field_id: u32,
    /// Text a file-name field is written as.
    file_text: String,
    notes_by_key: std::collections::HashMap<(RichNoteKind, u32), String>,
}

fn hex_rgb(hex: &str) -> Option<u32> {
    u32::from_str_radix(hex, 16).ok()
}

/// Index of `item` in `table`, added when it is new.
fn intern<T: Eq + std::hash::Hash + Copy>(
    table: &mut Vec<T>,
    map: &mut std::collections::HashMap<T, usize>,
    item: T,
) -> usize {
    if let Some(&idx) = map.get(&item) {
        return idx;
    }
    let idx = table.len();
    table.push(item);
    map.insert(item, idx);
    idx
}

impl ShapeTables {
    fn new() -> Self {
        Self {
            char_shapes: vec![HwpxCharShape::default()],
            char_map: std::collections::HashMap::from([(HwpxCharShape::default(), 0)]),
            para_shapes: vec![HwpxParaShape::default()],
            para_map: std::collections::HashMap::from([(HwpxParaShape::default(), 0)]),
            styles: Vec::new(),
            style_map: std::collections::HashMap::new(),
            fonts: Vec::new(),
            font_map: std::collections::HashMap::new(),
            border_fills: Vec::new(),
            border_fill_map: std::collections::HashMap::new(),
            next_field_id: 0,
            file_text: String::new(),
            notes_by_key: std::collections::HashMap::new(),
        }
    }

    fn border_fill_id(
        &mut self,
        fill: CellFill,
        borders: TableCellBorders,
    ) -> Result<u32, EasyOfficeError> {
        if fill.is_none() && borders.is_unspecified() {
            return Ok(2);
        }
        if let Some(&id) = self.border_fill_map.get(&(fill, borders)) {
            return Ok(id);
        }
        if self.border_fills.len() + 2 >= MAX_HWPX_BORDER_FILLS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "HWPX 테두리·배경 정의 수가 한도({MAX_HWPX_BORDER_FILLS})를 초과했습니다."
            )));
        }
        self.border_fills.push((fill, borders));
        let id = (self.border_fills.len() + 2) as u32;
        self.border_fill_map.insert((fill, borders), id);
        Ok(id)
    }

    /// Index (1-based, 0 is the default face) of a font name.
    fn font_index(&mut self, name: &str) -> usize {
        if name == DEFAULT_FONT_FACE {
            return 0;
        }
        if let Some(&idx) = self.font_map.get(name) {
            return idx;
        }
        self.fonts.push(name.to_string());
        let idx = self.fonts.len();
        self.font_map.insert(name.to_string(), idx);
        idx
    }

    fn write_paragraph(
        &mut self,
        sec: &mut SectionWriter,
        p: &RichParagraph,
        ordered_run_id: Option<u32>,
    ) {
        let level = if (1..=6).contains(&p.heading_level) {
            p.heading_level
        } else {
            0
        };
        let list_kind = list_kind_of(p);
        let list_lvl = if list_kind.is_some() {
            p.list_level.min(HWPX_MAX_LIST_LEVEL)
        } else {
            0
        };
        let (heading_type, heading_id_ref, checked) = match list_kind {
            Some(crate::RichListKind::Bullet) => {
                (HEADING_BULLET, u32::from(list_lvl % 3) + 1, false)
            }
            Some(crate::RichListKind::Task) => (
                HEADING_BULLET,
                crate::rich_list_writers::HWPX_TASK_BULLET_ID,
                p.task_checked,
            ),
            Some(crate::RichListKind::Ordered) => {
                (HEADING_NUMBER, ordered_run_id.unwrap_or(1).max(1), false)
            }
            None if level > 0 => (HEADING_OUTLINE, 0, false),
            None => (HEADING_NONE, 0, false),
        };
        let heading_level = if list_kind.is_none() && level > 0 {
            level - 1
        } else {
            list_lvl
        };
        let para = HwpxParaShape {
            align: match p.alignment {
                None => None,
                Some(RichAlignment::Left) => Some("LEFT"),
                Some(RichAlignment::Center) => Some("CENTER"),
                Some(RichAlignment::Right) => Some("RIGHT"),
                Some(RichAlignment::Justify) => Some("JUSTIFY"),
            },
            heading_type,
            heading_id_ref,
            heading_level,
            checked,
            left_margin: p
                .left_indent_twips
                .map(|t| t.saturating_mul(HWPUNIT_PER_TWIP))
                .unwrap_or(if list_kind.is_some() {
                    LIST_LEFT_MARGIN * (u32::from(list_lvl) + 1)
                } else {
                    0
                }),
            intent: p
                .first_line_indent_twips
                .map_or(0, |t| t.saturating_mul(HWPUNIT_PER_TWIP as i32)),
            line_percent: p
                .line_spacing_percent
                .map_or(DEFAULT_LINE_PERCENT, u32::from),
        };
        let para_pr = intern(&mut self.para_shapes, &mut self.para_map, para);
        let base = HwpxCharShape {
            bold: level > 0,
            height: [1600, 1400, 1300, 1200, 1100, 1000]
                .get(usize::from(level).wrapping_sub(1))
                .copied()
                .unwrap_or(DEFAULT_CHAR_HEIGHT),
            ..HwpxCharShape::default()
        };
        let style = if level > 0 {
            let char_pr = intern(&mut self.char_shapes, &mut self.char_map, base);
            let idx = *self.style_map.entry(level).or_insert_with(|| {
                self.styles.push(HwpxStyle {
                    name: format!("개요 {level}"),
                    eng_name: format!("Outline {level}"),
                    para_pr,
                    char_pr,
                });
                self.styles.len() - 1
            });
            idx + 1
        } else {
            0
        };
        sec.open_paragraph_with_cols(para_pr, style, p.columns);
        let groups = external_link_groups(&p.runs);
        if groups.is_empty() {
            let char_pr = intern(&mut self.char_shapes, &mut self.char_map, base);
            sec.run(char_pr, "");
        }
        for (link, group) in groups {
            let field_id = link.map(|url| {
                self.next_field_id += 1;
                sec.xml.push_str(&hwpx_link_begin(self.next_field_id, url));
                self.next_field_id
            });
            for run in group {
                if is_deleted_run(run) {
                    continue;
                }
                let font = run.font.as_ref().map_or(0, |f| self.font_index(f));
                let shape = HwpxCharShape {
                    bold: base.bold || run.bold,
                    italic: run.italic,
                    underline: run.underline,
                    strike: run.strike,
                    height: run.size_half_pt.map_or(base.height, |s| u32::from(s) * 50),
                    font,
                    text_color: run.color.as_deref().and_then(hex_rgb),
                    shade_color: run.highlight.as_deref().and_then(hex_rgb),
                    script: run.script,
                };
                let char_pr = intern(&mut self.char_shapes, &mut self.char_map, shape);
                if let Some(key) = run.note_ref {
                    // The first reference takes the body; a repeat is written empty (PD-262).
                    let note_text = self.notes_by_key.remove(&key).unwrap_or_default();
                    sec.note_run(char_pr, key.0, &note_text);
                } else if let Some(field) = run.field {
                    sec.field_run(char_pr, field, &self.file_text);
                } else {
                    sec.run(char_pr, &run.text);
                }
            }
            if let Some(id) = field_id {
                sec.xml.push_str(&hwpx_link_end(id));
            }
        }
        sec.xml.push_str("</hp:p>");
    }
}

pub struct HwpxCodec;

/// Incrementing paragraph-ID writer for `section0.xml`.
struct SectionWriter {
    xml: String,
    next_para_id: u32,
    next_table_id: u32,
    sec_pr_pending: bool,
    track_columns: bool,
    current_columns: Option<RichColumns>,
    /// `hp:ctrl` header and footer of the page, written into the run that holds the section properties.
    frame_controls: String,
    /// Page margins and paper of the section (`hp:pagePr`); the defaults when `None`.
    margins: Option<RichPageMargins>,
    paper: Option<RichPaper>,
    binding: RichBinding,
}

impl SectionWriter {
    fn new() -> Self {
        Self {
            xml: format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><hs:sec xmlns:hs=\"{NS_SECTION}\" xmlns:hp=\"{NS_PARAGRAPH}\" xmlns:hc=\"{NS_CORE}\">"
            ),
            next_para_id: 0,
            next_table_id: 1,
            sec_pr_pending: true,
            track_columns: true,
            current_columns: None,
            frame_controls: String::new(),
            margins: None,
            paper: None,
            binding: RichBinding::default(),
        }
    }

    /// Paragraphs written apart from the section (a header or footer), numbered from `first_para_id`.
    fn fragment(first_para_id: u32) -> Self {
        Self {
            xml: String::new(),
            next_para_id: first_para_id,
            next_table_id: 1,
            sec_pr_pending: false,
            track_columns: false,
            current_columns: None,
            frame_controls: String::new(),
            margins: None,
            paper: None,
            binding: RichBinding::default(),
        }
    }

    fn finish(mut self) -> String {
        if self.sec_pr_pending {
            // Empty document: a carrier paragraph still holds the section properties.
            self.open_paragraph();
            self.xml.push_str("</hp:p>");
        }
        self.xml.push_str("</hs:sec>");
        self.xml
    }

    fn open_paragraph(&mut self) {
        let cols = self.current_columns;
        self.open_paragraph_with_cols(0, 0, cols);
    }

    fn open_paragraph_with_cols(
        &mut self,
        para_pr: usize,
        style: usize,
        cols: Option<RichColumns>,
    ) {
        self.xml.push_str(&format!(
            "<hp:p id=\"{}\" paraPrIDRef=\"{para_pr}\" styleIDRef=\"{style}\" pageBreak=\"0\" columnBreak=\"0\" merged=\"0\">",
            self.next_para_id
        ));
        self.next_para_id += 1;
        if self.sec_pr_pending {
            self.sec_pr_pending = false;
            self.current_columns = cols;
            self.xml.push_str("<hp:run charPrIDRef=\"0\">");
            self.xml.push_str(&section_properties_xml(
                self.margins.as_ref(),
                self.paper.as_ref(),
                self.binding,
                cols,
            ));
            self.xml.push_str(&self.frame_controls);
            self.xml.push_str("</hp:run>");
        } else if self.track_columns && cols != self.current_columns {
            self.current_columns = cols;
            self.xml.push_str(&format!(
                "<hp:run charPrIDRef=\"0\">{}</hp:run>",
                hwpx_col_ctrl_xml(cols)
            ));
        }
    }

    /// `ordinal` numbers this reference (the picture id); `part` is the manifest item of the bytes.
    fn picture_paragraph(&mut self, img: &RichImage, ordinal: usize, part: usize) {
        self.open_paragraph();
        self.xml
            .push_str(&hwpx_picture_run(img, ordinal, &format!("image{part}")));
        self.xml.push_str("</hp:p>");
    }

    fn text_paragraph(&mut self, text: &str) {
        self.open_paragraph();
        self.run(0, text);
        self.xml.push_str("</hp:p>");
    }

    fn run(&mut self, char_pr: usize, text: &str) {
        self.xml
            .push_str(&format!("<hp:run charPrIDRef=\"{char_pr}\"><hp:t>"));
        if !text.contains(['\t', '\n']) {
            self.xml.push_str(&XmlSanitizer::escape(text));
        } else {
            let mut start = 0;
            for (idx, ch) in text.char_indices() {
                let tag = match ch {
                    '\t' => "<hp:tab width=\"0\" leader=\"0\" type=\"1\"/>",
                    '\n' => "<hp:lineBreak/>",
                    _ => continue,
                };
                if idx > start {
                    self.xml.push_str(&XmlSanitizer::escape(&text[start..idx]));
                }
                self.xml.push_str(tag);
                start = idx + 1;
            }
            if start < text.len() {
                self.xml.push_str(&XmlSanitizer::escape(&text[start..]));
            }
        }
        self.xml.push_str("</hp:t></hp:run>");
    }

    /// A run holding a page-number or page-count field; the file name has no field that reads back, so it
    /// is the text `file_name`.
    fn field_run(&mut self, char_pr: usize, field: RichField, file_name: &str) {
        let inner = match field {
            RichField::PageNumber => format!("{}<hp:t/>", hwpx_number_control("PAGE")),
            RichField::TotalPages => format!("{}<hp:t/>", hwpx_number_control("TOTAL_PAGE")),
            RichField::FileName => format!("<hp:t>{}</hp:t>", XmlSanitizer::escape(file_name)),
        };
        self.xml.push_str(&format!(
            "<hp:run charPrIDRef=\"{char_pr}\">{inner}</hp:run>"
        ));
    }

    fn note_run(&mut self, char_pr: usize, kind: RichNoteKind, text: &str) {
        let tag = match kind {
            RichNoteKind::Footnote => "hp:footNote",
            RichNoteKind::Endnote => "hp:endNote",
        };
        let mut paras = String::new();
        for line in text.split('\n') {
            paras.push_str("<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>");
            paras.push_str(&XmlSanitizer::escape(line));
            paras.push_str("</hp:t></hp:run></hp:p>");
        }
        self.xml.push_str(&format!(
            "<hp:run charPrIDRef=\"{char_pr}\"><hp:ctrl><{tag}><hp:subList>{paras}</hp:subList></{tag}></hp:ctrl></hp:run>"
        ));
    }

    fn write_table_line_runs(
        &mut self,
        line: &str,
        line_start: usize,
        cell_links: &[TableCellLink],
        next_field_id: &mut u32,
    ) {
        let line_end = line_start + line.len();
        let mut pos = 0usize;
        for link in cell_links {
            let (s, e) = (link.start as usize, link.end as usize);
            if e <= line_start || s >= line_end {
                continue;
            }
            let Some(url) = safe_link_target(&link.url).filter(|u| !u.starts_with('#')) else {
                continue;
            };
            let ls = s.max(line_start) - line_start;
            let le = e.min(line_end) - line_start;
            if pos < ls {
                self.run(0, &line[pos..ls]);
            }
            *next_field_id += 1;
            let id = *next_field_id;
            self.xml.push_str(&hwpx_link_begin(id, url));
            self.run(0, &line[ls..le]);
            self.xml.push_str(&hwpx_link_end(id));
            pos = le;
        }
        if pos < line.len() || line.is_empty() {
            self.run(0, &line[pos..]);
        }
    }

    fn table_paragraph(
        &mut self,
        table: &RichTable,
        shapes: &mut ShapeTables,
        media: &mut MediaParts,
    ) -> Result<(), EasyOfficeError> {
        let spans = table.span_map();
        let links = table.validated_links();
        let styles = table.validated_cell_styles();
        let images = table.validated_cell_images();
        let col_widths_hu: Option<Vec<u32>> = table.validated_col_widths().map(|widths| {
            widths
                .into_iter()
                .map(|t| t.saturating_mul(HWPUNIT_PER_TWIP))
                .collect()
        });
        let row_cnt = table.rows.len();
        let col_cnt = table.column_count().max(1);
        let default_cell_w =
            text_width(self.margins.as_ref(), self.paper.as_ref(), self.binding) / col_cnt as u32;
        let total_w = col_widths_hu.as_ref().map_or_else(
            || default_cell_w * col_cnt as u32,
            |hu| hu.iter().copied().fold(0u32, u32::saturating_add),
        );
        let no_adjust = if col_widths_hu.is_some() { "1" } else { "0" };
        let tbl_id = self.next_table_id;
        self.next_table_id += 1;
        let mut link_cursor = 0usize;
        let mut style_cursor = 0usize;
        let mut img_cursor = 0usize;
        self.open_paragraph();
        self.xml.push_str("<hp:run charPrIDRef=\"0\">");
        self.xml.push_str(&format!(
            concat!(
                "<hp:tbl id=\"{id}\" zOrder=\"0\" numberingType=\"TABLE\" textWrap=\"TOP_AND_BOTTOM\" textFlow=\"BOTH_SIDES\" lock=\"0\" dropcapstyle=\"None\" pageBreak=\"CELL\" repeatHeader=\"1\" rowCnt=\"{rows}\" colCnt=\"{cols}\" cellSpacing=\"0\" borderFillIDRef=\"2\" noAdjust=\"{no_adjust}\">",
                "<hp:sz width=\"{w}\" widthRelTo=\"ABSOLUTE\" height=\"{h}\" heightRelTo=\"ABSOLUTE\" protect=\"0\"/>",
                "<hp:pos treatAsChar=\"1\" affectLSpacing=\"0\" flowWithText=\"1\" allowOverlap=\"0\" holdAnchorAndSO=\"0\" vertRelTo=\"PARA\" horzRelTo=\"COLUMN\" vertAlign=\"TOP\" horzAlign=\"LEFT\" vertOffset=\"0\" horzOffset=\"0\"/>",
                "<hp:outMargin left=\"283\" right=\"283\" top=\"283\" bottom=\"283\"/>",
                "<hp:inMargin left=\"510\" right=\"510\" top=\"141\" bottom=\"141\"/>"
            ),
            id = tbl_id,
            rows = row_cnt,
            cols = col_cnt,
            no_adjust = no_adjust,
            w = total_w,
            h = CELL_HEIGHT * row_cnt as u32
        ));
        for (r_idx, row) in table.rows.iter().enumerate() {
            self.xml.push_str("<hp:tr>");
            for (c_idx, cell) in row.iter().enumerate() {
                let (row_span, col_span) = match spans
                    .as_ref()
                    .map_or(CellSpan::Plain, |m| m.at(r_idx, c_idx))
                {
                    CellSpan::Plain => (1, 1),
                    CellSpan::Anchor { row_span, col_span } => (row_span, col_span),
                    // The anchor's cell covers this position.
                    CellSpan::Covered { .. } => continue,
                };
                let cl = slice_cell_links(&links, &mut link_cursor, r_idx, c_idx);
                let bf_id = match next_cell_style(&styles, &mut style_cursor, r_idx, c_idx) {
                    Some((fill, borders)) => shapes.border_fill_id(fill, borders)?,
                    None => 2,
                };
                let cell_imgs = slice_cell_images(&images, &mut img_cursor, r_idx, c_idx);
                self.xml.push_str(&format!(
                    "<hp:tc name=\"\" header=\"0\" hasMargin=\"0\" protect=\"0\" editable=\"0\" dirty=\"0\" borderFillIDRef=\"{bf_id}\">"
                ));
                self.xml.push_str("<hp:subList id=\"\" textDirection=\"HORIZONTAL\" lineWrap=\"BREAK\" vertAlign=\"CENTER\" linkListIDRef=\"0\" linkListNextIDRef=\"0\" textWidth=\"0\" textHeight=\"0\" hasTextRef=\"0\" hasNumRef=\"0\">");
                if !cell.is_empty() || !cl.is_empty() || cell_imgs.is_empty() {
                    let mut line_start = 0usize;
                    for line in cell.split('\n') {
                        let line_end = line_start + line.len();
                        self.xml.push_str(&format!(
                            "<hp:p id=\"{}\" paraPrIDRef=\"0\" styleIDRef=\"0\" pageBreak=\"0\" columnBreak=\"0\" merged=\"0\">",
                            self.next_para_id,
                        ));
                        self.next_para_id += 1;
                        if cl.is_empty() {
                            self.run(0, line);
                        } else {
                            self.write_table_line_runs(
                                line,
                                line_start,
                                cl,
                                &mut shapes.next_field_id,
                            );
                        }
                        self.xml.push_str("</hp:p>");
                        line_start = line_end + 1;
                    }
                }
                for ci in cell_imgs {
                    let picture = media.add(HWPX_MEDIA_DIR, &ci.image);
                    self.xml.push_str(&format!(
                        "<hp:p id=\"{}\" paraPrIDRef=\"0\" styleIDRef=\"0\" pageBreak=\"0\" columnBreak=\"0\" merged=\"0\">{}</hp:p>",
                        self.next_para_id,
                        hwpx_picture_run(
                            &ci.image,
                            picture.ordinal,
                            &format!("image{}", picture.part)
                        ),
                    ));
                    self.next_para_id += 1;
                }
                let cell_w_span = col_widths_hu
                    .as_ref()
                    .and_then(|hu| hu.get(c_idx..c_idx + col_span as usize))
                    .map_or(default_cell_w * col_span, |s| {
                        s.iter().copied().fold(0u32, u32::saturating_add)
                    });
                let cell_h_span = CELL_HEIGHT * row_span;
                self.xml.push_str(&format!(
                    "</hp:subList><hp:cellAddr colAddr=\"{c_idx}\" rowAddr=\"{r_idx}\"/><hp:cellSpan colSpan=\"{col_span}\" rowSpan=\"{row_span}\"/><hp:cellSz width=\"{cell_w_span}\" height=\"{cell_h_span}\"/><hp:cellMargin left=\"510\" right=\"510\" top=\"141\" bottom=\"141\"/></hp:tc>"
                ));
            }
            self.xml.push_str("</hp:tr>");
        }
        self.xml.push_str("</hp:tbl></hp:run></hp:p>");
        Ok(())
    }
}

fn opf_meta_values(hpf_xml: &str) -> Vec<(&str, String)> {
    extract_tags_with_attrs(hpf_xml, "opf:meta")
        .into_iter()
        .filter_map(|(attrs, body)| {
            extract_attr_from_fragment(attrs, "name")
                .map(|name| (name, XmlSanitizer::unescape(body)))
        })
        .collect()
}

impl HwpxCodec {
    pub const MIMETYPE_HWPX: &str = "application/hwp+zip";

    pub fn pack(payload: &HwpxPayload) -> Result<Vec<u8>, EasyOfficeError> {
        let mut sec = SectionWriter::new();
        let mut shapes = ShapeTables::new();
        let mut media = MediaParts::new();
        for p in &payload.paragraphs {
            sec.text_paragraph(p);
        }
        for tbl in &payload.tables {
            let rich = RichTable::new(tbl.clone());
            sec.table_paragraph(&rich, &mut shapes, &mut media)?;
        }
        Self::pack_package(payload, header_xml(), sec.finish(), MediaParts::new())
    }

    /// Writes a [`RichDocument`] in block order: each distinct run format becomes a `charPr`,
    /// each distinct alignment/indent a `paraPr`, and heading levels use "개요 N" styles. The header and
    /// footer are controls of the first paragraph.
    pub fn pack_rich(doc: &RichDocument, is_template: bool) -> Result<Vec<u8>, EasyOfficeError> {
        let entries = Self::pack_rich_entries(doc, is_template)?;
        ZipArchiveCodec::pack_compressed(&entries)
    }

    pub(crate) fn pack_rich_entries(
        doc: &RichDocument,
        is_template: bool,
    ) -> Result<Vec<(String, Vec<u8>)>, EasyOfficeError> {
        check_document_images(doc)?;
        let mut media = MediaParts::new();
        let mut sec = SectionWriter::new();
        let mut shapes = ShapeTables::new();
        shapes.file_text = doc.display_title();
        for note in doc.validated_notes() {
            shapes.notes_by_key.insert((note.kind, note.id), note.text);
        }
        sec.margins = doc.page_frame.margins;
        sec.paper = doc.page_frame.paper;
        sec.binding = doc.page_frame.binding;
        let list_plan = plan_lists(&doc.blocks);
        let ordered_ids = hwpx_ordered_run_ids(&list_plan);
        let numberings_xml = hwpx_numberings_xml(&list_plan);
        let pf = &doc.page_frame;
        let eo = pf.even_odd_different;
        let mut ctrl_id = 0u32;
        for (header, apply, paragraphs) in [
            (true, if eo { "ODD" } else { "BOTH" }, &pf.header),
            (true, "EVEN", &pf.even_header),
            (false, if eo { "ODD" } else { "BOTH" }, &pf.footer),
            (false, "EVEN", &pf.even_footer),
        ] {
            if (apply == "EVEN" && !eo) || paragraphs.is_empty() {
                continue;
            }
            let mut fragment = SectionWriter::fragment(sec.next_para_id);
            for p in paragraphs {
                shapes.write_paragraph(&mut fragment, &frame_paragraph(p), None);
            }
            sec.next_para_id = fragment.next_para_id;
            ctrl_id += 1;
            sec.frame_controls.push_str(&hwpx_frame_control_typed(
                header,
                ctrl_id,
                apply,
                &fragment.xml,
            ));
        }
        if pf.first_page_different {
            let hide_hdr = pf.first_header.is_empty() && pf.has_any_header();
            let hide_ftr = pf.first_footer.is_empty() && pf.has_any_footer();
            if hide_hdr || hide_ftr {
                sec.frame_controls
                    .push_str(&hwpx_page_hiding_control(hide_hdr, hide_ftr));
            }
        }
        for (idx, block) in doc.blocks.iter().enumerate() {
            match block {
                RichBlock::Paragraph(p) => {
                    let ordered_id = list_plan
                        .block_run
                        .get(idx)
                        .copied()
                        .flatten()
                        .and_then(|run| ordered_ids.get(run).copied());
                    shapes.write_paragraph(&mut sec, p, ordered_id);
                }
                RichBlock::Table(table) => {
                    sec.table_paragraph(table, &mut shapes, &mut media)?;
                }
                RichBlock::Image(img) => {
                    let picture = media.add(HWPX_MEDIA_DIR, img);
                    sec.picture_paragraph(img, picture.ordinal, picture.part);
                }
            }
        }
        let header = header_xml_with_all(
            &shapes.char_shapes,
            &shapes.para_shapes,
            &shapes.styles,
            &shapes.fonts,
            Some(&numberings_xml),
            &shapes.border_fills,
        );
        let meta = HwpxPayload {
            title: doc.display_title(),
            is_template,
            official_agency: doc.official_agency.clone(),
            approval_lines: vec!["기안".to_string(), "검토".to_string(), "결재".to_string()],
            paragraphs: Vec::new(),
            tables: Vec::new(),
        };
        Self::package_entries(&meta, header, sec.finish(), media)
    }

    fn pack_package(
        payload: &HwpxPayload,
        header: String,
        section: String,
        media: MediaParts,
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let entries = Self::package_entries(payload, header, section, media)?;
        ZipArchiveCodec::pack_compressed(&entries)
    }

    fn package_entries(
        payload: &HwpxPayload,
        header: String,
        section: String,
        media: MediaParts,
    ) -> Result<Vec<(String, Vec<u8>)>, EasyOfficeError> {
        let mut metadata = format!(
            "<opf:title>{}</opf:title><opf:language>ko</opf:language><opf:meta name=\"creator\" content=\"text\">EasyOffice</opf:meta>",
            XmlSanitizer::escape(&payload.title)
        );
        if payload.is_template {
            metadata.push_str(HWPX_TEMPLATE_META_MARKER);
        }
        if let Some(agency) = payload.official_agency.as_deref().filter(|a| !a.is_empty()) {
            metadata.push_str(&format!(
                "<opf:meta name=\"{META_AGENCY}\" content=\"text\">{}</opf:meta>",
                XmlSanitizer::escape(agency)
            ));
        }
        if !payload.approval_lines.is_empty() {
            metadata.push_str(&format!(
                "<opf:meta name=\"{META_APPROVAL}\" content=\"text\">{}</opf:meta>",
                XmlSanitizer::escape(&payload.approval_lines.join("|"))
            ));
        }
        let content_hpf = format!(
            concat!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
                "<opf:package xmlns:opf=\"http://www.idpf.org/2007/opf/\" version=\"\" unique-identifier=\"\" id=\"\">",
                "<opf:metadata>{}</opf:metadata>",
                "<opf:manifest>",
                "<opf:item id=\"header\" href=\"Contents/header.xml\" media-type=\"application/xml\"/>",
                "<opf:item id=\"section0\" href=\"Contents/section0.xml\" media-type=\"application/xml\"/>",
                "<opf:item id=\"settings\" href=\"settings.xml\" media-type=\"application/xml\"/>{}",
                "</opf:manifest>",
                "<opf:spine><opf:itemref idref=\"header\" linear=\"yes\"/><opf:itemref idref=\"section0\" linear=\"yes\"/></opf:spine>",
                "</opf:package>"
            ),
            metadata,
            hpf_media_items(&media)
        );

        let part_names = [
            "version.xml",
            "Contents/content.hpf",
            "Contents/header.xml",
            "Contents/section0.xml",
            "settings.xml",
        ];
        let manifest = manifest_xml(Self::MIMETYPE_HWPX, &part_names, &media.listing());

        for xml in [&content_hpf, &header, &section, &manifest] {
            XmlSanitizer::validate_safe_xml(xml)?;
        }

        let mut entries = vec![
            (
                "mimetype".to_string(),
                Self::MIMETYPE_HWPX.as_bytes().to_vec(),
            ),
            ("version.xml".to_string(), VERSION_XML.as_bytes().to_vec()),
            (
                "META-INF/container.xml".to_string(),
                CONTAINER_XML.as_bytes().to_vec(),
            ),
            ("META-INF/manifest.xml".to_string(), manifest.into_bytes()),
            ("Contents/content.hpf".to_string(), content_hpf.into_bytes()),
            ("Contents/header.xml".to_string(), header.into_bytes()),
            ("Contents/section0.xml".to_string(), section.into_bytes()),
            ("settings.xml".to_string(), SETTINGS_XML.as_bytes().to_vec()),
        ];
        entries.extend(media.into_entries());
        Ok(entries)
    }

    /// Validates XML entries, extracts `Contents/header.xml`, and parses package metadata.
    #[allow(clippy::type_complexity)]
    pub(crate) fn extract_header_and_metadata<'a>(
        parts: &[(&str, &'a str)],
    ) -> Result<(&'a str, String, bool, Option<String>, Vec<String>), EasyOfficeError> {
        let header_xml = checked_part(parts, "Contents/header.xml").ok_or_else(|| {
            EasyOfficeError::CorruptedContainer(
                "HWPX 필수 엔트리 'Contents/header.xml'이 누락되었습니다.".to_string(),
            )
        })?;
        let hpf_xml = checked_part(parts, "Contents/content.hpf").unwrap_or_default();
        let hpf_title = extract_tag_bodies(hpf_xml, "opf:title")
            .into_iter()
            .next()
            .map(XmlSanitizer::unescape)
            .filter(|s| !s.is_empty());
        let metas = opf_meta_values(hpf_xml);
        let meta = |key: &str| {
            metas
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| value.clone())
                .filter(|s| !s.is_empty())
        };
        // Legacy EasyOffice output stored metadata as attributes on `hh:head`; still read it.
        let legacy = |attr: &str| {
            extract_attr(header_xml, "<hh:head", attr)
                .map(XmlSanitizer::unescape)
                .filter(|s| !s.is_empty())
        };

        let title = legacy("title")
            .or(hpf_title)
            .unwrap_or_else(|| "HWPX 문서".to_string());
        let is_template = legacy("isTemplate").is_some_and(|s| s == "1")
            || meta("isTemplate").is_some_and(|s| s == "1");
        let official_agency = legacy("officialAgency").or_else(|| meta(META_AGENCY));
        let approval_lines: Vec<String> = legacy("approvalLines")
            .or_else(|| meta(META_APPROVAL))
            .map(|s| s.split('|').map(ToString::to_string).collect())
            .unwrap_or_default();

        Ok((
            header_xml,
            title,
            is_template,
            official_agency,
            approval_lines,
        ))
    }

    pub fn unpack(bytes: &[u8]) -> Result<HwpxPayload, EasyOfficeError> {
        crate::hwpx_unpack::unpack_hwpx_bytes(bytes)
    }
}
