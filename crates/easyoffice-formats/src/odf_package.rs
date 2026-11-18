//! ODF package extraction, manifest generation, and `.odp` presentation codec helpers.

use crate::odf_hwpx_codecs::{OdfCodec, OdpUnpacked};
use crate::office_codecs::validate_zip_xml_entries;
use crate::rtf_xml_helpers::{
    extract_attr, extract_attr_from_fragment, extract_balanced_elements,
    extract_odf_blocks_in_order, extract_odf_inline_text, extract_tag_bodies, strip_tag_blocks,
};
use crate::sheet_styling::ods_view::read_views;
use crate::sheet_styling::SheetStyling;
use crate::{ContainerFamily, EasyOfficeError, XmlSanitizer, ZipArchiveCodec, ZipSecurityPolicy};

impl OdfCodec {
    pub fn pack_odp(
        title: &str,
        slides: &[(String, Vec<String>, String)],
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let manifest = Self::build_manifest(Self::MIMETYPE_ODP);
        let mut content = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:presentation=\"urn:oasis:names:tc:opendocument:xmlns:presentation:1.0\" office:version=\"1.2\" title=\"{}\"><office:body><office:presentation>",
            XmlSanitizer::escape(title)
        );
        for (s_title, bullets, notes) in slides {
            content.push_str(&format!(
                "<draw:page draw:name=\"{}\">",
                XmlSanitizer::escape(s_title)
            ));
            if !s_title.is_empty() {
                content.push_str(&format!(
                    "<draw:frame draw:layer=\"layout\" presentation:class=\"title\"><draw:text-box><text:p>{}</text:p></draw:text-box></draw:frame>",
                    XmlSanitizer::escape(s_title)
                ));
            }
            content.push_str("<draw:frame draw:layer=\"layout\"><draw:text-box>");
            for b in bullets {
                content.push_str("<text:p>");
                content.push_str(&XmlSanitizer::escape(b));
                content.push_str("</text:p>");
            }
            content.push_str("</draw:text-box></draw:frame>");
            if !notes.is_empty() {
                content.push_str("<presentation:notes><draw:frame><draw:text-box>");
                for line in notes.split('\n') {
                    content.push_str("<text:p>");
                    content.push_str(&XmlSanitizer::escape(line));
                    content.push_str("</text:p>");
                }
                content.push_str("</draw:text-box></draw:frame></presentation:notes>");
            }
            content.push_str("</draw:page>");
        }
        content.push_str("</office:presentation></office:body></office:document-content>");
        XmlSanitizer::validate_safe_xml(&content)?;

        ZipArchiveCodec::pack_compressed(&[
            (
                "mimetype".to_string(),
                Self::MIMETYPE_ODP.as_bytes().to_vec(),
            ),
            ("META-INF/manifest.xml".to_string(), manifest.into_bytes()),
            ("content.xml".to_string(), content.into_bytes()),
        ])
    }

    pub fn unpack_odp(bytes: &[u8]) -> Result<OdpUnpacked, EasyOfficeError> {
        let (content_xml, meta_xml_opt) =
            Self::extract_content_and_meta_xml(bytes, Self::MIMETYPE_ODP)?;
        let title = extract_attr(&content_xml, "<office:document-content", "title")
            .map(XmlSanitizer::unescape)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                meta_xml_opt
                    .as_deref()
                    .and_then(|m| extract_tag_bodies(m, "dc:title").into_iter().next())
                    .map(XmlSanitizer::unescape)
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| "발표자료".to_string());

        let raw_slides = extract_balanced_elements(&content_xml, "draw:page");
        if raw_slides.len() > 4096 {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "ODP 슬라이드 개수가 상한(4,096)을 초과했습니다.".to_string(),
            ));
        }
        let mut budget = crate::decode_budget::DecodeBudget::new();
        budget.charge(title.len(), "ODP 제목")?;

        let mut slides = Vec::with_capacity(raw_slides.len());
        for (attrs, inner) in raw_slides {
            let slide = Self::parse_odp_slide(attrs, inner);
            budget.charge(slide.0.len(), "ODP 슬라이드 제목")?;
            for bullet in &slide.1 {
                budget.charge(bullet.len(), "ODP 슬라이드 본문")?;
            }
            budget.charge(slide.2.len(), "ODP 슬라이드 발표자 노트")?;
            slides.push(slide);
        }
        Ok((title, slides))
    }

    /// Speaker notes of a slide: the `notes` attribute, else the notes pages' paragraph text.
    fn odp_slide_notes(attrs: &str, inner: &str) -> String {
        let notes = extract_attr_from_fragment(attrs, "notes")
            .map(XmlSanitizer::unescape)
            .unwrap_or_default();
        let notes_blocks = extract_balanced_elements(inner, "presentation:notes");
        if !notes.is_empty() || notes_blocks.is_empty() {
            return notes;
        }
        let mut note_lines = Vec::new();
        for (_, n_inner) in &notes_blocks {
            let cleaned = strip_tag_blocks(n_inner, "text:page-number");
            for (_, _, p_body) in extract_odf_blocks_in_order(&cleaned, &["text:p", "text:h"]) {
                let txt = extract_odf_inline_text(p_body);
                if !txt.trim().is_empty() {
                    note_lines.push(txt);
                }
            }
        }
        note_lines.join("\n")
    }

    /// Text of the first non-empty title-class frame.
    fn odp_frame_title(without_page_num: &str) -> Option<String> {
        extract_balanced_elements(without_page_num, "draw:frame")
            .into_iter()
            .filter(|(f_attrs, _)| {
                extract_attr_from_fragment(f_attrs, "presentation:class") == Some("title")
            })
            .find_map(|(_, f_inner)| {
                let t_parts: Vec<String> =
                    extract_odf_blocks_in_order(f_inner, &["text:p", "text:h"])
                        .into_iter()
                        .map(|(_, _, b)| extract_odf_inline_text(b))
                        .filter(|s| !s.is_empty())
                        .collect();
                (!t_parts.is_empty()).then(|| t_parts.join(" "))
            })
    }

    fn parse_odp_slide(attrs: &str, inner: &str) -> (String, Vec<String>, String) {
        let notes = Self::odp_slide_notes(attrs, inner);
        let without_notes = strip_tag_blocks(inner, "presentation:notes");
        let without_page_num = strip_tag_blocks(&without_notes, "text:page-number");
        let frame_title = Self::odp_frame_title(&without_page_num);
        let attr_title = extract_attr_from_fragment(attrs, "draw:name")
            .or_else(|| extract_attr_from_fragment(attrs, "title"))
            .map(XmlSanitizer::unescape)
            .unwrap_or_default();

        let mut bullets: Vec<String> =
            extract_odf_blocks_in_order(&without_notes, &["text:p", "text:h"])
                .into_iter()
                .filter_map(|(_, _, b)| {
                    let had_page_num = b.contains("<text:page-number");
                    let cleaned = strip_tag_blocks(b, "text:page-number");
                    let txt = extract_odf_inline_text(&cleaned);
                    (!(had_page_num && txt.trim().is_empty())).then_some(txt)
                })
                .collect();

        let s_title = match frame_title {
            Some(ft) => {
                if bullets.first() == Some(&ft) {
                    bullets.remove(0);
                }
                ft
            }
            None => attr_title,
        };
        (s_title, bullets, notes)
    }

    pub(crate) fn build_manifest(mime: &str) -> String {
        Self::build_manifest_with_media(mime, &[])
    }

    /// Manifest listing `content.xml` plus embedded media parts `(path, media type)`.
    pub(crate) fn build_manifest_with_media(mime: &str, media: &[(String, &str)]) -> String {
        let mut xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\"><manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"{mime}\"/><manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/>"
        );
        for (path, media_type) in media {
            xml.push_str(&format!(
                "<manifest:file-entry manifest:full-path=\"{}\" manifest:media-type=\"{media_type}\"/>",
                XmlSanitizer::escape(path)
            ));
        }
        xml.push_str("</manifest:manifest>");
        xml
    }

    pub(crate) fn extract_content_and_meta_xml(
        bytes: &[u8],
        expected_mime: &str,
    ) -> Result<(String, Option<String>), EasyOfficeError> {
        Self::extract_parts(bytes, expected_mime).map(|(content, meta, _, _)| (content, meta))
    }

    /// `content.xml`, `meta.xml`, `settings.xml` and `styles.xml` (when the file has them) of an ODF package.
    #[allow(clippy::type_complexity)]
    pub(crate) fn extract_parts(
        bytes: &[u8],
        expected_mime: &str,
    ) -> Result<(String, Option<String>, Option<String>, Option<String>), EasyOfficeError> {
        let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
        validate_zip_xml_entries(&entries)?;
        if let Some(mime_entry) = entries.iter().find(|e| e.path == "mimetype") {
            let actual_mime = std::str::from_utf8(&mime_entry.data).unwrap_or_default();
            if actual_mime.trim() != expected_mime {
                return Err(EasyOfficeError::SignatureMismatch {
                    expected: ContainerFamily::ZipOdf,
                    detail: format!(
                        "ODF mimetype 불일치 (expected: {expected_mime}, actual: {actual_mime})"
                    ),
                });
            }
        }
        let content_entry = entries
            .iter()
            .find(|e| e.path == "content.xml")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "ODF 필수 엔트리 'content.xml'이 누락되었습니다.".to_string(),
                )
            })?;
        let xml = std::str::from_utf8(&content_entry.data)
            .map_err(|_| EasyOfficeError::InvalidUtf8("ODF content.xml UTF-8 오류".to_string()))?;
        XmlSanitizer::validate_safe_xml(xml)?;

        let meta_xml = if let Some(m_entry) = entries.iter().find(|e| e.path == "meta.xml") {
            let m_str = std::str::from_utf8(&m_entry.data)
                .map_err(|_| EasyOfficeError::InvalidUtf8("ODF meta.xml UTF-8 오류".to_string()))?;
            XmlSanitizer::validate_safe_xml(m_str)?;
            Some(m_str.to_string())
        } else {
            None
        };

        let settings_xml = match entries.iter().find(|e| e.path == "settings.xml") {
            Some(entry) => {
                let text = std::str::from_utf8(&entry.data).map_err(|_| {
                    EasyOfficeError::InvalidUtf8("ODF settings.xml UTF-8 오류".to_string())
                })?;
                XmlSanitizer::validate_safe_xml(text)?;
                Some(text.to_string())
            }
            None => None,
        };

        let styles_xml = match entries.iter().find(|e| e.path == "styles.xml") {
            Some(entry) => {
                let text = std::str::from_utf8(&entry.data).map_err(|_| {
                    EasyOfficeError::InvalidUtf8("ODF styles.xml UTF-8 오류".to_string())
                })?;
                XmlSanitizer::validate_safe_xml(text)?;
                Some(text.to_string())
            }
            None => None,
        };

        Ok((xml.to_string(), meta_xml, settings_xml, styles_xml))
    }

    /// A workbook shows at least one sheet: a file that hides them all opens on its first.
    pub(crate) fn show_one_sheet(stylings: &mut [(String, SheetStyling)]) {
        if !stylings.is_empty() && stylings.iter().all(|(_, s)| s.hidden) {
            crate::SheetState::Visible.apply(&mut stylings[0].1);
        }
    }

    /// Gives each table the frozen pane and the view `settings.xml` names for its sheet.
    pub(crate) fn apply_views(stylings: &mut [(String, SheetStyling)], settings: Option<&str>) {
        let Some(settings) = settings else { return };
        let mut views = read_views(settings);
        for (name, styling) in stylings {
            if let Some(table) = views.remove(name.as_str()) {
                (styling.freeze, styling.view) = (table.freeze, table.view);
            }
        }
    }
}
