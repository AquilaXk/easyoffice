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
    HwpxParaShape, HwpxStyle, CELL_HEIGHT, CONTAINER_XML, DEFAULT_CHAR_HEIGHT, LIST_LEFT_MARGIN,
    NS_PARAGRAPH, NS_SECTION, SETTINGS_XML, TEXT_WIDTH, VERSION_XML,
};
use crate::office_codecs::validate_zip_xml_entries;
use crate::rich_document::{RichAlignment, RichBlock, RichDocument};
use crate::rich_image::{check_document_images, RichImage};
use crate::rich_image_writers::{hwpx_picture_run, MediaParts};
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

pub struct HwpxCodec;

/// Incrementing paragraph-ID writer for `section0.xml`.
struct SectionWriter {
    xml: String,
    next_para_id: u32,
    next_table_id: u32,
    sec_pr_pending: bool,
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
            self.xml.push_str(&section_properties_xml());
            self.xml.push_str("</hp:run>");
        }
    }

    fn picture_paragraph(&mut self, img: &RichImage, number: usize) {
        self.open_paragraph();
        self.xml
            .push_str(&hwpx_picture_run(img, number, &format!("image{number}")));
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

    fn table_paragraph(&mut self, rows: &[Vec<String>]) {
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
                self.xml.push_str(&format!(
                    "</hp:subList><hp:cellAddr colAddr=\"{c_idx}\" rowAddr=\"{r_idx}\"/><hp:cellSpan colSpan=\"1\" rowSpan=\"1\"/><hp:cellSz width=\"{cell_w}\" height=\"{CELL_HEIGHT}\"/><hp:cellMargin left=\"510\" right=\"510\" top=\"141\" bottom=\"141\"/></hp:tc>"
                ));
            }
            self.xml.push_str("</hp:tr>");
        }
        self.xml.push_str("</hp:tbl></hp:run></hp:p>");
    }
}

fn opf_meta_values(hpf_xml: &str) -> Vec<(String, String)> {
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
            sec.table_paragraph(tbl);
        }
        Self::pack_package(payload, header_xml(), sec.finish(), MediaParts::default())
    }

    /// Writes a [`RichDocument`] in block order: each distinct run format becomes a `charPr`,
    /// each distinct alignment/indent a `paraPr`, and heading levels use "개요 N" styles.
    pub fn pack_rich(doc: &RichDocument, is_template: bool) -> Result<Vec<u8>, EasyOfficeError> {
        let mut char_shapes = vec![HwpxCharShape::default()];
        let mut para_shapes = vec![HwpxParaShape::default()];
        let mut styles: Vec<HwpxStyle> = Vec::new();
        let mut fonts: Vec<String> = Vec::new();
        fn hex_rgb(hex: &str) -> Option<u32> {
            u32::from_str_radix(hex, 16).ok()
        }
        fn intern<T: PartialEq + Copy>(table: &mut Vec<T>, item: T) -> usize {
            table.iter().position(|x| *x == item).unwrap_or_else(|| {
                table.push(item);
                table.len() - 1
            })
        }
        check_document_images(doc)?;
        let mut media = MediaParts::default();
        let mut sec = SectionWriter::new();
        for block in &doc.blocks {
            let p = match block {
                RichBlock::Paragraph(p) => p,
                RichBlock::Table(rows) => {
                    sec.table_paragraph(rows);
                    continue;
                }
                RichBlock::Image(img) => {
                    let (number, _) = media.add(HWPX_MEDIA_DIR, img);
                    sec.picture_paragraph(img, number);
                    continue;
                }
            };
            let level = if (1..=6).contains(&p.heading_level) {
                p.heading_level
            } else {
                0
            };
            let para = HwpxParaShape {
                align: match p.alignment {
                    None => HwpxParaShape::default().align,
                    Some(RichAlignment::Left) => "LEFT",
                    Some(RichAlignment::Center) => "CENTER",
                    Some(RichAlignment::Right) => "RIGHT",
                    Some(RichAlignment::Justify) => "JUSTIFY",
                },
                left_margin: if p.list.is_some() {
                    LIST_LEFT_MARGIN
                } else {
                    0
                },
            };
            let para_pr = intern(&mut para_shapes, para);
            let base = HwpxCharShape {
                bold: level > 0,
                height: [1600, 1400, 1300, 1200, 1100, 1000]
                    .get(usize::from(level).wrapping_sub(1))
                    .copied()
                    .unwrap_or(DEFAULT_CHAR_HEIGHT),
                ..HwpxCharShape::default()
            };
            let style = if level > 0 {
                let char_pr = intern(&mut char_shapes, base);
                let name = format!("개요 {level}");
                styles
                    .iter()
                    .position(|s| s.name == name)
                    .unwrap_or_else(|| {
                        styles.push(HwpxStyle {
                            name,
                            eng_name: format!("Outline {level}"),
                            para_pr,
                            char_pr,
                        });
                        styles.len() - 1
                    })
                    + 1
            } else {
                0
            };
            sec.open_paragraph_with(para_pr, style);
            let runs: Vec<_> = p.runs.iter().filter(|r| !r.text.is_empty()).collect();
            if runs.is_empty() {
                let char_pr = intern(&mut char_shapes, base);
                sec.run(char_pr, "");
            }
            for run in runs {
                let font = run.font.as_ref().map_or(0, |f| {
                    1 + fonts.iter().position(|x| x == f).unwrap_or_else(|| {
                        fonts.push(f.clone());
                        fonts.len() - 1
                    })
                });
                let shape = HwpxCharShape {
                    bold: base.bold || run.bold,
                    italic: run.italic,
                    underline: run.underline,
                    strike: run.strike,
                    height: run.size_half_pt.map_or(base.height, |s| u32::from(s) * 50),
                    font,
                    text_color: run.color.as_deref().and_then(hex_rgb),
                    shade_color: run.highlight.as_deref().and_then(hex_rgb),
                };
                let char_pr = intern(&mut char_shapes, shape);
                sec.run(char_pr, &run.text);
            }
            sec.xml.push_str("</hp:p>");
        }
        let header = header_xml_with(&char_shapes, &para_shapes, &styles, &fonts);
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

    pub fn unpack(bytes: &[u8]) -> Result<HwpxPayload, EasyOfficeError> {
        let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
        validate_zip_xml_entries(&entries)?;
        let header_entry = entries
            .iter()
            .find(|e| e.path == "Contents/header.xml")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "HWPX 필수 엔트리 'Contents/header.xml'이 누락되었습니다.".to_string(),
                )
            })?;
        let header_xml = std::str::from_utf8(&header_entry.data).map_err(|_| {
            EasyOfficeError::InvalidUtf8("Contents/header.xml UTF-8 오류".to_string())
        })?;
        XmlSanitizer::validate_safe_xml(header_xml)?;

        let hpf_xml = entries
            .iter()
            .find(|e| e.path == "Contents/content.hpf")
            .and_then(|e| std::str::from_utf8(&e.data).ok())
            .unwrap_or_default();
        let hpf_title = extract_tag_bodies(hpf_xml, "opf:title")
            .into_iter()
            .next()
            .map(XmlSanitizer::unescape)
            .filter(|s| !s.is_empty());
        let metas = opf_meta_values(hpf_xml);
        let meta = |key: &str| {
            metas
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
                .filter(|s| !s.is_empty())
        };
        // Legacy EasyOffice output stored metadata as attributes on `hh:head`; still read it.
        let legacy = |attr: &str| {
            extract_attr(header_xml, "<hh:head", attr)
                .map(|s| XmlSanitizer::unescape(&s))
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
            let sec_xml = std::str::from_utf8(&sec_entry.data).map_err(|_| {
                EasyOfficeError::InvalidUtf8("Contents/section.xml UTF-8 오류".to_string())
            })?;
            XmlSanitizer::validate_safe_xml(sec_xml)?;

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
