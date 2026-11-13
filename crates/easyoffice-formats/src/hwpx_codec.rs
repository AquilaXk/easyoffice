//! KS X 6101 (OWPML) HWPX container codec for `.hwpx` and `.hwtx`.
//!
//! The writer stores `mimetype` first (uncompressed), lists every part in both
//! `META-INF/manifest.xml` and `Contents/content.hpf`, writes a populated `header.xml`
//! (fonts, character/paragraph shapes, styles), `version.xml`, and `settings.xml`, places
//! `hp:secPr` in the first paragraph of `section0.xml`, references shape IDs from every paragraph
//! and run, and wraps tables as `hp:p > hp:run > hp:tbl` with `hp:tc > hp:subList > hp:p` cells.
//! Document metadata (template flag, issuing agency, approval lines) lives in OPF `opf:meta`.

use crate::hwpx_parts::{
    header_xml, header_xml_with, manifest_xml, section_properties_xml, HwpxCharShape,
    HwpxParaShape, HwpxStyle, CELL_HEIGHT, CONTAINER_XML, DEFAULT_CHAR_HEIGHT,
    DEFAULT_LINE_PERCENT, HEADING_BULLET, HEADING_NONE, HEADING_NUMBER, HWPUNIT_PER_TWIP,
    LIST_LEFT_MARGIN, NS_PARAGRAPH, NS_SECTION, SETTINGS_XML, TEXT_WIDTH, VERSION_XML,
};
use crate::office_codecs::{checked_part, checked_zip_xml_entries};
use crate::rich_document::{
    frame_paragraph, RichAlignment, RichBlock, RichDocument, RichField, RichPageMargins,
    RichParagraph,
};
use crate::rich_image::{check_document_images, RichImage};
use crate::rich_image_writers::{hwpx_picture_run, MediaParts};
use crate::rich_link_writers::{external_link_groups, hwpx_link_begin, hwpx_link_end};
use crate::rich_page_frame::{hwpx_frame_control, hwpx_number_control};
use crate::rich_table::{CellSpan, SpanMap};
use crate::rtf_xml_helpers::{
    extract_attr, extract_attr_from_fragment, extract_tag_bodies,
    extract_tags_or_self_closing_with_attrs, extract_tags_with_attrs, strip_tag_blocks,
};
use crate::{EasyOfficeError, XmlSanitizer, ZipArchiveCodec, ZipSecurityPolicy};

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
    next_field_id: u32,
    /// Text a file-name field is written as.
    file_text: String,
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
            next_field_id: 0,
            file_text: String::new(),
        }
    }

    /// Index (1-based, 0 is the default face) of a font name.
    fn font_index(&mut self, name: &str) -> usize {
        if let Some(&idx) = self.font_map.get(name) {
            return idx;
        }
        self.fonts.push(name.to_string());
        let idx = self.fonts.len();
        self.font_map.insert(name.to_string(), idx);
        idx
    }

    fn write_paragraph(&mut self, sec: &mut SectionWriter, p: &RichParagraph) {
        let level = if (1..=6).contains(&p.heading_level) {
            p.heading_level
        } else {
            0
        };
        let para = HwpxParaShape {
            align: match p.alignment {
                None => None,
                Some(RichAlignment::Left) => Some("LEFT"),
                Some(RichAlignment::Center) => Some("CENTER"),
                Some(RichAlignment::Right) => Some("RIGHT"),
                Some(RichAlignment::Justify) => Some("JUSTIFY"),
            },
            heading_type: match p.list {
                Some(crate::RichListKind::Bullet) => HEADING_BULLET,
                Some(crate::RichListKind::Ordered) => HEADING_NUMBER,
                None => HEADING_NONE,
            },
            left_margin: p
                .left_indent_twips
                .map(|t| t.saturating_mul(HWPUNIT_PER_TWIP))
                .unwrap_or(if p.list.is_some() {
                    LIST_LEFT_MARGIN
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
        sec.open_paragraph_with(para_pr, style);
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
                if let Some(field) = run.field {
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
    /// `hp:ctrl` header and footer of the page, written into the run that holds the section properties.
    frame_controls: String,
    /// Page margins of the section (`hp:pagePr`); the defaults when `None`.
    margins: Option<RichPageMargins>,
}

impl SectionWriter {
    fn new() -> Self {
        Self {
            xml: format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><hs:sec xmlns:hs=\"{NS_SECTION}\" xmlns:hp=\"{NS_PARAGRAPH}\">"
            ),
            next_para_id: 0,
            next_table_id: 1,
            sec_pr_pending: true,
            frame_controls: String::new(),
            margins: None,
        }
    }

    /// Paragraphs written apart from the section (a header or footer), numbered from `first_para_id`.
    fn fragment(first_para_id: u32) -> Self {
        Self {
            xml: String::new(),
            next_para_id: first_para_id,
            next_table_id: 1,
            sec_pr_pending: false,
            frame_controls: String::new(),
            margins: None,
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
        self.open_paragraph_with(0, 0);
    }

    fn open_paragraph_with(&mut self, para_pr: usize, style: usize) {
        self.xml.push_str(&format!(
            "<hp:p id=\"{}\" paraPrIDRef=\"{para_pr}\" styleIDRef=\"{style}\" pageBreak=\"0\" columnBreak=\"0\" merged=\"0\">",
            self.next_para_id
        ));
        self.next_para_id += 1;
        if self.sec_pr_pending {
            self.sec_pr_pending = false;
            self.xml.push_str("<hp:run charPrIDRef=\"0\">");
            self.xml
                .push_str(&section_properties_xml(self.margins.as_ref()));
            self.xml.push_str(&self.frame_controls);
            self.xml.push_str("</hp:run>");
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
        self.xml.push_str(&XmlSanitizer::escape(text));
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

    fn table_paragraph(&mut self, rows: &[Vec<String>], spans: Option<&SpanMap>) {
        let row_cnt = rows.len();
        let col_cnt = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
        let cell_w = TEXT_WIDTH / col_cnt as u32;
        let tbl_id = self.next_table_id;
        self.next_table_id += 1;
        self.open_paragraph();
        self.xml.push_str("<hp:run charPrIDRef=\"0\">");
        self.xml.push_str(&format!(
            concat!(
                "<hp:tbl id=\"{id}\" zOrder=\"0\" numberingType=\"TABLE\" textWrap=\"TOP_AND_BOTTOM\" textFlow=\"BOTH_SIDES\" lock=\"0\" dropcapstyle=\"None\" pageBreak=\"CELL\" repeatHeader=\"1\" rowCnt=\"{rows}\" colCnt=\"{cols}\" cellSpacing=\"0\" borderFillIDRef=\"2\" noAdjust=\"0\">",
                "<hp:sz width=\"{w}\" widthRelTo=\"ABSOLUTE\" height=\"{h}\" heightRelTo=\"ABSOLUTE\" protect=\"0\"/>",
                "<hp:pos treatAsChar=\"1\" affectLSpacing=\"0\" flowWithText=\"1\" allowOverlap=\"0\" holdAnchorAndSO=\"0\" vertRelTo=\"PARA\" horzRelTo=\"COLUMN\" vertAlign=\"TOP\" horzAlign=\"LEFT\" vertOffset=\"0\" horzOffset=\"0\"/>",
                "<hp:outMargin left=\"283\" right=\"283\" top=\"283\" bottom=\"283\"/>",
                "<hp:inMargin left=\"510\" right=\"510\" top=\"141\" bottom=\"141\"/>"
            ),
            id = tbl_id,
            rows = row_cnt,
            cols = col_cnt,
            w = cell_w * col_cnt as u32,
            h = CELL_HEIGHT * row_cnt as u32
        ));
        for (r_idx, row) in rows.iter().enumerate() {
            self.xml.push_str("<hp:tr>");
            for (c_idx, cell) in row.iter().enumerate() {
                let (row_span, col_span) =
                    match spans.map_or(CellSpan::Plain, |m| m.at(r_idx, c_idx)) {
                        CellSpan::Plain => (1, 1),
                        CellSpan::Anchor { row_span, col_span } => (row_span, col_span),
                        // The anchor's cell covers this position.
                        CellSpan::Covered { .. } => continue,
                    };
                self.xml.push_str("<hp:tc name=\"\" header=\"0\" hasMargin=\"0\" protect=\"0\" editable=\"0\" dirty=\"0\" borderFillIDRef=\"2\">");
                self.xml.push_str("<hp:subList id=\"\" textDirection=\"HORIZONTAL\" lineWrap=\"BREAK\" vertAlign=\"CENTER\" linkListIDRef=\"0\" linkListNextIDRef=\"0\" textWidth=\"0\" textHeight=\"0\" hasTextRef=\"0\" hasNumRef=\"0\">");
                for line in cell.split('\n') {
                    self.xml.push_str(&format!(
                        "<hp:p id=\"{}\" paraPrIDRef=\"0\" styleIDRef=\"0\" pageBreak=\"0\" columnBreak=\"0\" merged=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>{}</hp:t></hp:run></hp:p>",
                        self.next_para_id,
                        XmlSanitizer::escape(line)
                    ));
                    self.next_para_id += 1;
                }
                let cell_w_span = cell_w * col_span;
                let cell_h_span = CELL_HEIGHT * row_span;
                self.xml.push_str(&format!(
                    "</hp:subList><hp:cellAddr colAddr=\"{c_idx}\" rowAddr=\"{r_idx}\"/><hp:cellSpan colSpan=\"{col_span}\" rowSpan=\"{row_span}\"/><hp:cellSz width=\"{cell_w_span}\" height=\"{cell_h_span}\"/><hp:cellMargin left=\"510\" right=\"510\" top=\"141\" bottom=\"141\"/></hp:tc>"
                ));
            }
            self.xml.push_str("</hp:tr>");
        }
        self.xml.push_str("</hp:tbl></hp:run></hp:p>");
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
        for p in &payload.paragraphs {
            sec.text_paragraph(p);
        }
        for tbl in &payload.tables {
            sec.table_paragraph(tbl, None);
        }
        Self::pack_package(payload, header_xml(), sec.finish(), MediaParts::new())
    }

    /// Writes a [`RichDocument`] in block order: each distinct run format becomes a `charPr`,
    /// each distinct alignment/indent a `paraPr`, and heading levels use "개요 N" styles. The header and
    /// footer are controls of the first paragraph.
    pub fn pack_rich(doc: &RichDocument, is_template: bool) -> Result<Vec<u8>, EasyOfficeError> {
        check_document_images(doc)?;
        let mut media = MediaParts::new();
        let mut sec = SectionWriter::new();
        let mut shapes = ShapeTables::new();
        shapes.file_text = doc.display_title();
        if !doc.page_frame.is_empty() {
            sec.margins = doc.page_frame.margins;
        }
        for (header, paragraphs) in [
            (true, &doc.page_frame.header),
            (false, &doc.page_frame.footer),
        ] {
            if paragraphs.is_empty() {
                continue;
            }
            let mut fragment = SectionWriter::fragment(sec.next_para_id);
            for p in paragraphs {
                shapes.write_paragraph(&mut fragment, &frame_paragraph(p));
            }
            sec.next_para_id = fragment.next_para_id;
            sec.frame_controls.push_str(&hwpx_frame_control(
                header,
                u32::from(!header) + 1,
                &fragment.xml,
            ));
        }
        for block in &doc.blocks {
            match block {
                RichBlock::Paragraph(p) => shapes.write_paragraph(&mut sec, p),
                RichBlock::Table(table) => {
                    sec.table_paragraph(&table.rows, table.span_map().as_ref());
                }
                RichBlock::Image(img) => {
                    let picture = media.add(HWPX_MEDIA_DIR, img);
                    sec.picture_paragraph(img, picture.ordinal, picture.part);
                }
            }
        }
        let header = header_xml_with(
            &shapes.char_shapes,
            &shapes.para_shapes,
            &shapes.styles,
            &shapes.fonts,
        );
        let meta = HwpxPayload {
            title: doc.display_title(),
            is_template,
            official_agency: doc.official_agency.clone(),
            approval_lines: vec!["기안".to_string(), "검토".to_string(), "결재".to_string()],
            paragraphs: Vec::new(),
            tables: Vec::new(),
        };
        Self::pack_package(&meta, header, sec.finish(), media)
    }

    fn pack_package(
        payload: &HwpxPayload,
        header: String,
        section: String,
        media: MediaParts,
    ) -> Result<Vec<u8>, EasyOfficeError> {
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
        ZipArchiveCodec::pack_compressed(&entries)
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
        let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
        let parts = checked_zip_xml_entries(&entries)?;
        let (_header_xml, title, is_template, official_agency, approval_lines) =
            Self::extract_header_and_metadata(&parts)?;

        let mut sec_entries: Vec<(usize, &crate::ZipArchiveEntry)> = entries
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
        sec_entries.sort_by_key(|(idx, _)| *idx);

        if sec_entries.is_empty() {
            return Err(EasyOfficeError::CorruptedContainer(
                "HWPX 필수 엔트리 'Contents/section0.xml'이 누락되었습니다.".to_string(),
            ));
        }

        let mut tables = Vec::new();
        let mut paragraphs = Vec::new();

        for (_, sec_entry) in sec_entries {
            let sec_xml = section_xml(&parts, &sec_entry.path)?;

            for tbl_body in extract_tag_bodies(sec_xml, "hp:tbl") {
                tables.push(extract_hwpx_table_rows(tbl_body)?);
                let tbl_without_tc = strip_tag_blocks(tbl_body, "hp:tc");
                append_hwpx_equations(&tbl_without_tc, &mut paragraphs)?;
            }

            for p_body in extract_tag_bodies(sec_xml, "hp:p") {
                let has_table = p_body.contains("<hp:tbl");
                let p_text_only = if has_table {
                    strip_tag_blocks(p_body, "hp:tbl")
                } else {
                    p_body.to_string()
                };
                if is_structural_carrier(&p_text_only, has_table) {
                    continue;
                }
                extract_hwpx_paragraphs_and_eqs(&p_text_only, &mut paragraphs)?;
            }
            let sec_without_tables = strip_tag_blocks(sec_xml, "hp:tbl");
            let outside_p = strip_tag_blocks(&sec_without_tables, "hp:p");
            append_hwpx_equations(&outside_p, &mut paragraphs)?;
        }

        Ok(HwpxPayload {
            title,
            is_template,
            official_agency,
            approval_lines,
            paragraphs,
            tables,
        })
    }
}

/// A `Contents/section*.xml` part of a package whose XML entries were checked as a whole.
pub(crate) fn section_xml<'a>(
    parts: &[(&str, &'a str)],
    path: &str,
) -> Result<&'a str, EasyOfficeError> {
    checked_part(parts, path).ok_or_else(|| {
        EasyOfficeError::CorruptedContainer(format!(
            "HWPX 구역 엔트리 '{path}'를 읽을 수 없습니다."
        ))
    })
}

/// A paragraph that only anchors a table or carries section/column properties has no text.
fn is_structural_carrier(p_body: &str, had_table: bool) -> bool {
    let carries_props = p_body.contains("<hp:secPr") || p_body.contains("<hp:ctrl");
    (had_table || carries_props)
        && extract_tags_or_self_closing_with_attrs(p_body, "hp:t").is_empty()
        && !p_body.contains("<hp:equation")
}

fn extract_hwpx_paragraphs_and_eqs(
    p_body: &str,
    out: &mut Vec<String>,
) -> Result<(), EasyOfficeError> {
    let p_without_eq = strip_tag_blocks(p_body, "hp:equation");
    let text = extract_tag_bodies(&p_without_eq, "hp:t")
        .into_iter()
        .map(XmlSanitizer::unescape)
        .collect::<Vec<_>>()
        .join("");
    if !text.is_empty() || !p_body.contains("<hp:equation") {
        out.push(text);
    }
    append_hwpx_equations(p_body, out)
}

fn extract_hwpx_cell_text(tc_body: &str) -> Result<String, EasyOfficeError> {
    let cell_paras = extract_tag_bodies(tc_body, "hp:p");
    let mut parts = Vec::new();
    if !cell_paras.is_empty() {
        for p in cell_paras {
            extract_hwpx_paragraphs_and_eqs(p, &mut parts)?;
        }
        let tc_no_p = strip_tag_blocks(tc_body, "hp:p");
        append_hwpx_equations(&tc_no_p, &mut parts)?;
    } else {
        let tc_no_eq = strip_tag_blocks(tc_body, "hp:equation");
        let direct_t = extract_tag_bodies(&tc_no_eq, "hp:t");
        let text_nodes = if !direct_t.is_empty() {
            direct_t
        } else {
            extract_tag_bodies(&tc_no_eq, "hp:cellText")
        };
        let t = text_nodes
            .into_iter()
            .map(XmlSanitizer::unescape)
            .collect::<Vec<_>>()
            .join("");
        if !t.is_empty() || !tc_body.contains("<hp:equation") {
            parts.push(t);
        }
        append_hwpx_equations(tc_body, &mut parts)?;
    }
    Ok(parts.join("\n"))
}

fn extract_hwpx_table_rows(tbl_body: &str) -> Result<Vec<Vec<String>>, EasyOfficeError> {
    let mut rows = Vec::new();
    let tr_bodies = extract_tag_bodies(tbl_body, "hp:tr");
    let row_slices = if !tr_bodies.is_empty() {
        tr_bodies
    } else {
        vec![tbl_body]
    };
    for tr_body in row_slices {
        let mut cells = Vec::new();
        for tc_body in extract_tag_bodies(tr_body, "hp:tc") {
            cells.push(extract_hwpx_cell_text(tc_body)?);
        }
        rows.push(cells);
    }
    Ok(rows)
}

fn append_hwpx_equations(xml: &str, out: &mut Vec<String>) -> Result<(), EasyOfficeError> {
    for eq_body in extract_tag_bodies(xml, "hp:equation") {
        for raw_script in extract_tag_bodies(eq_body, "hp:script") {
            let script = XmlSanitizer::unescape(raw_script);
            let trimmed = script.trim();
            if !trimmed.is_empty() {
                let latex = crate::hwp_eqedit::convert_eqedit_to_latex(trimmed)?;
                out.push(format!("$${latex}$$"));
            }
        }
    }
    Ok(())
}
