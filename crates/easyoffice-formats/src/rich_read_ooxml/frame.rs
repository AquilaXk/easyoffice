//! Header/footer part loading and `word/settings.xml` flags for the DOCX reader (`PD-211`, `PD-243`).

use std::collections::HashMap;

use super::{walk_part, DocxWalker, FrameRef, Resolver};
use crate::office_codecs::checked_part;
use crate::rich_document::{frame_paragraph, RichColumns, MAX_PAGE_FRAME_PARAGRAPHS};
use crate::rich_read_images::{load_docx_frame_targets, DocxMedia};
use crate::rich_xml::toggle_on;
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{EasyOfficeError, RichBlock, RichPageFrame, RichParagraph};

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DocxSettings {
    pub(crate) even_and_odd_headers: bool,
    pub(crate) mirror_margins: bool,
    pub(crate) gutter_at_top: bool,
    pub(crate) track_revisions: bool,
    /// `w:defaultTabStop`, in twips.
    pub(crate) default_tab_stop: Option<u32>,
}

/// Parses `w:cols` attributes into [`RichColumns`] (`None` when single-column).
pub(super) fn parse_docx_cols(attrs: &Attrs<'_>) -> Option<RichColumns> {
    let count = attr(attrs, "w:num")
        .and_then(|v| v.trim().parse::<u16>().ok())
        .map_or(1, |n| n.clamp(1, 16) as u8);
    if count <= 1 {
        return None;
    }
    let equal = attr(attrs, "w:equalWidth").as_deref() != Some("0")
        && attr(attrs, "w:equalWidth").as_deref() != Some("false");
    let gap_twips = attr(attrs, "w:space")
        .and_then(|v| v.trim().parse::<u32>().ok())
        .map_or(720, |g| g.min(u32::from(u16::MAX)) as u16);
    Some(RichColumns {
        count,
        equal,
        gap_twips,
    })
}

/// Extracted flags from `word/settings.xml` in a single pass.
pub(super) fn read_word_settings(settings: Option<&str>) -> Result<DocxSettings, EasyOfficeError> {
    let mut s = DocxSettings::default();
    let Some(xml) = settings else {
        return Ok(s);
    };
    for event in XmlReader::new(xml) {
        if let XmlEvent::Start { name, attrs } = event? {
            let on = toggle_on(attr(&attrs, "w:val").as_deref());
            match name {
                "w:evenAndOddHeaders" => s.even_and_odd_headers = on,
                "w:mirrorMargins" => s.mirror_margins = on,
                "w:gutterAtTop" => s.gutter_at_top = on,
                "w:trackRevisions" => s.track_revisions = on,
                "w:defaultTabStop" => {
                    s.default_tab_stop = attr(&attrs, "w:val").and_then(|v| v.trim().parse().ok());
                }
                _ => {}
            }
        }
    }
    Ok(s)
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

/// Reads default, first-page (`w:titlePg`), and even-page (`w:evenAndOddHeaders`) headers and
/// footers from the first section; differences in subsequent sections are reported.
pub(super) fn read_page_frame_with_targets(
    targets: &HashMap<String, String>,
    parts: &[(&str, &str)],
    resolver: &Resolver<'_>,
    media: &DocxMedia<'_>,
    refs: &[FrameRef],
    first_section_title_pg: bool,
    settings: DocxSettings,
) -> Result<(RichPageFrame, Vec<String>), EasyOfficeError> {
    let mut frame = RichPageFrame {
        first_page_different: first_section_title_pg,
        even_odd_different: settings.even_and_odd_headers,
        ..RichPageFrame::default()
    };
    let mut warnings = Vec::new();
    if refs.is_empty() {
        return Ok((frame, warnings));
    }
    let mut other_sections = false;
    let load = |chosen: Option<&str>, warnings: &mut Vec<String>| match chosen
        .and_then(|part| checked_part(parts, part))
    {
        Some(xml) => read_frame_part(xml, resolver, media, warnings),
        None => Ok(Vec::new()),
    };
    for header in [true, false] {
        let mut default_part: Option<&str> = None;
        let mut first_part: Option<&str> = None;
        let mut even_part: Option<&str> = None;
        for r in refs.iter().filter(|r| r.header == header) {
            let Some(part) = targets.get(&r.rel_id).map(String::as_str) else {
                continue;
            };
            match (r.section, r.kind.as_str()) {
                (0, "default") => default_part = default_part.or(Some(part)),
                (_, "default") => match default_part {
                    None => default_part = Some(part),
                    Some(prev) => other_sections |= prev != part,
                },
                (0, "first") if first_section_title_pg => first_part = first_part.or(Some(part)),
                (s, "first") if s > 0 && r.section_title_pg => {
                    other_sections |= first_part != Some(part);
                }
                (0, "even") if settings.even_and_odd_headers => {
                    even_part = even_part.or(Some(part));
                }
                (s, "even") if s > 0 && settings.even_and_odd_headers => {
                    other_sections |= even_part != Some(part);
                }
                _ => {}
            }
        }
        let d = load(default_part, &mut warnings)?;
        let f = load(first_part, &mut warnings)?;
        let e = load(even_part, &mut warnings)?;
        if header {
            frame.header = d;
            frame.first_header = f;
            frame.even_header = e;
        } else {
            frame.footer = d;
            frame.first_footer = f;
            frame.even_footer = e;
        }
    }
    if other_sections {
        warnings.push("구역마다 다른 머리말·꼬리말은 첫 구역의 것만 가져왔습니다.".to_string());
    }
    Ok((frame, warnings))
}

#[allow(dead_code)]
pub(super) fn read_page_frame(
    entries: &[crate::ZipArchiveEntry],
    parts: &[(&str, &str)],
    resolver: &Resolver<'_>,
    media: &DocxMedia<'_>,
    refs: &[FrameRef],
    first_section_title_pg: bool,
) -> Result<(RichPageFrame, Vec<String>), EasyOfficeError> {
    let targets = load_docx_frame_targets(entries)?;
    let settings = read_word_settings(checked_part(parts, "word/settings.xml"))?;
    read_page_frame_with_targets(
        &targets,
        parts,
        resolver,
        media,
        refs,
        first_section_title_pg,
        settings,
    )
}
