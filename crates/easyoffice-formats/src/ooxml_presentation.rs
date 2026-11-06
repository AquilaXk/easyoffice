//! ECMA-376 PresentationML (`.pptx`/`.ppsx`) writer and reader.
//!
//! The writer emits every part a conforming consumer requires: a DrawingML theme related from the
//! slide master (and notes master), `p:clrMap` on masters, `p:nvGrpSpPr`/`p:grpSpPr` on each shape
//! tree, `p:nvSpPr`/`p:spPr`/`p:txBody` on each shape, slide titles in a title placeholder shape,
//! speaker notes in real notes slides, and the presentation title in `docProps/core.xml`.

use crate::office_codecs::validate_zip_xml_entries;
use crate::ooxml_shared_parts::{
    core_properties_xml, drawingml_theme_xml, CORE_PROPERTIES_CONTENT_TYPE,
    CORE_PROPERTIES_REL_TYPE, PML_CLR_MAP, PML_SP_TREE_HEADER,
};
use crate::rtf_xml_helpers::{
    extract_attr, extract_attr_from_fragment, extract_balanced_elements, extract_tag_bodies,
};
use crate::{
    EasyOfficeError, OoxmlCodec, OoxmlPptxPayload, XmlSanitizer, ZipArchiveCodec, ZipArchiveEntry,
    ZipSecurityPolicy,
};

const PML_NS: &str = concat!(
    "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
    "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
    "xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\""
);
const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const REL_BASE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CT_PREFIX: &str = "application/vnd.openxmlformats-officedocument";
const SLIDESHOW_MAIN_CT: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml";
const PRESENTATION_MAIN_CT: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml";

fn rels_xml(rels: &[(&str, &str, String)]) -> String {
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{REL_NS}\">"
    );
    for (id, rel_type, target) in rels {
        let full_type = if rel_type.starts_with("http") {
            (*rel_type).to_string()
        } else {
            format!("{REL_BASE}/{rel_type}")
        };
        xml.push_str(&format!(
            "<Relationship Id=\"{id}\" Type=\"{full_type}\" Target=\"{target}\"/>"
        ));
    }
    xml.push_str("</Relationships>");
    xml
}

fn push_paragraphs(out: &mut String, lines: &[&str]) {
    let mut wrote = false;
    for line in lines {
        out.push_str("<a:p><a:r><a:rPr lang=\"ko-KR\" dirty=\"0\"/><a:t>");
        out.push_str(&XmlSanitizer::escape(line));
        out.push_str("</a:t></a:r></a:p>");
        wrote = true;
    }
    if !wrote {
        out.push_str("<a:p><a:endParaRPr lang=\"ko-KR\" dirty=\"0\"/></a:p>");
    }
}

/// One placeholder text shape with explicit geometry.
fn push_text_shape(
    out: &mut String,
    id: u32,
    name: &str,
    placeholder: &str,
    geometry: Option<(u64, u64, u64, u64)>,
    lines: &[&str],
) {
    out.push_str(&format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr>{placeholder}</p:nvPr></p:nvSpPr>"
    ));
    match geometry {
        Some((x, y, cx, cy)) => out.push_str(&format!(
            "<p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr>"
        )),
        None => out.push_str("<p:spPr/>"),
    }
    out.push_str("<p:txBody><a:bodyPr/><a:lstStyle/>");
    push_paragraphs(out, lines);
    out.push_str("</p:txBody></p:sp>");
}

fn slide_xml(title: &str, bullets: &[String]) -> String {
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sld {PML_NS}><p:cSld><p:spTree>{PML_SP_TREE_HEADER}"
    );
    let title_lines: Vec<&str> = if title.is_empty() {
        Vec::new()
    } else {
        vec![title]
    };
    push_text_shape(
        &mut xml,
        2,
        "Title 1",
        "<p:ph type=\"title\"/>",
        Some((457_200, 274_638, 8_229_600, 1_143_000)),
        &title_lines,
    );
    let bullet_lines: Vec<&str> = bullets.iter().map(String::as_str).collect();
    push_text_shape(
        &mut xml,
        3,
        "Content Placeholder 2",
        "<p:ph idx=\"1\"/>",
        Some((457_200, 1_600_200, 8_229_600, 4_525_963)),
        &bullet_lines,
    );
    xml.push_str("</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>");
    xml
}

fn notes_slide_xml(notes: &str) -> String {
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:notes {PML_NS}><p:cSld><p:spTree>{PML_SP_TREE_HEADER}"
    );
    let lines: Vec<&str> = notes.split('\n').collect();
    push_text_shape(
        &mut xml,
        2,
        "Notes Placeholder 1",
        "<p:ph type=\"body\" idx=\"1\"/>",
        None,
        &lines,
    );
    xml.push_str("</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>");
    xml
}

fn shape_paragraph_texts(sp_body: &str) -> Vec<String> {
    extract_tag_bodies(sp_body, "a:p")
        .into_iter()
        .map(|p_inner| {
            extract_tag_bodies(p_inner, "a:t")
                .into_iter()
                .map(XmlSanitizer::unescape)
                .collect::<Vec<_>>()
                .join("")
        })
        .collect()
}

fn is_title_placeholder(sp_body: &str) -> bool {
    extract_balanced_elements(sp_body, "p:ph")
        .iter()
        .any(|(attrs, _)| {
            matches!(
                extract_attr_from_fragment(attrs, "type").as_deref(),
                Some("title") | Some("ctrTitle")
            )
        })
}

fn resolve_rel_target(base_dir: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_string();
    }
    let mut parts: Vec<&str> = base_dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn utf8_entry<'a>(entry: &'a ZipArchiveEntry, label: &str) -> Result<&'a str, EasyOfficeError> {
    let xml = std::str::from_utf8(&entry.data)
        .map_err(|_| EasyOfficeError::InvalidUtf8(format!("{label} UTF-8 오류")))?;
    XmlSanitizer::validate_safe_xml(xml)?;
    Ok(xml)
}

/// Reads speaker notes from the notes slide related to `slide_path`, if any.
fn read_notes_slide(
    entries: &[ZipArchiveEntry],
    slide_path: &str,
) -> Result<Option<String>, EasyOfficeError> {
    let (dir, file) = slide_path.rsplit_once('/').unwrap_or(("", slide_path));
    let rels_path = format!("{dir}/_rels/{file}.rels");
    let Some(rels_entry) = entries.iter().find(|e| e.path == rels_path) else {
        return Ok(None);
    };
    let rels = utf8_entry(rels_entry, &rels_path)?;
    for (attrs, _) in extract_balanced_elements(rels, "Relationship") {
        let is_notes =
            extract_attr_from_fragment(attrs, "Type").is_some_and(|t| t.ends_with("/notesSlide"));
        let Some(target) = extract_attr_from_fragment(attrs, "Target") else {
            continue;
        };
        if !is_notes {
            continue;
        }
        let notes_path = resolve_rel_target(dir, &target);
        let Some(notes_entry) = entries.iter().find(|e| e.path == notes_path) else {
            continue;
        };
        let notes_xml = utf8_entry(notes_entry, &notes_path)?;
        for sp in extract_tag_bodies(notes_xml, "p:sp") {
            let body_ph = extract_balanced_elements(sp, "p:ph")
                .iter()
                .any(|(a, _)| extract_attr_from_fragment(a, "type").as_deref() == Some("body"));
            if body_ph {
                return Ok(Some(shape_paragraph_texts(sp).join("\n")));
            }
        }
    }
    Ok(None)
}

impl OoxmlCodec {
    pub fn pack_pptx(payload: &OoxmlPptxPayload) -> Result<Vec<u8>, EasyOfficeError> {
        let has_notes = payload.slides.iter().any(|(_, _, n)| !n.is_empty());
        let main_ct = if payload.is_slideshow {
            SLIDESHOW_MAIN_CT
        } else {
            PRESENTATION_MAIN_CT
        };
        let pml = format!("{CT_PREFIX}.presentationml");
        let mut content_types = format!(
            concat!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
                "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
                "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
                "<Default Extension=\"xml\" ContentType=\"application/xml\"/>",
                "<Override PartName=\"/ppt/presentation.xml\" ContentType=\"{main_ct}\"/>",
                "<Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"{pml}.slideMaster+xml\"/>",
                "<Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"{pml}.slideLayout+xml\"/>",
                "<Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"{ct}.theme+xml\"/>",
                "<Override PartName=\"/docProps/core.xml\" ContentType=\"{core}\"/>"
            ),
            main_ct = main_ct,
            pml = pml,
            ct = CT_PREFIX,
            core = CORE_PROPERTIES_CONTENT_TYPE
        );
        if has_notes {
            content_types.push_str(&format!(
                "<Override PartName=\"/ppt/theme/theme2.xml\" ContentType=\"{CT_PREFIX}.theme+xml\"/><Override PartName=\"/ppt/notesMasters/notesMaster1.xml\" ContentType=\"{pml}.notesMaster+xml\"/>"
            ));
        }
        for (idx, (_, _, notes)) in payload.slides.iter().enumerate() {
            let n = idx + 1;
            content_types.push_str(&format!(
                "<Override PartName=\"/ppt/slides/slide{n}.xml\" ContentType=\"{pml}.slide+xml\"/>"
            ));
            if !notes.is_empty() {
                content_types.push_str(&format!(
                    "<Override PartName=\"/ppt/notesSlides/notesSlide{n}.xml\" ContentType=\"{pml}.notesSlide+xml\"/>"
                ));
            }
        }
        content_types.push_str("</Types>");

        let root_rels = rels_xml(&[
            ("rId1", "officeDocument", "ppt/presentation.xml".to_string()),
            (
                "rId2",
                CORE_PROPERTIES_REL_TYPE,
                "docProps/core.xml".to_string(),
            ),
        ]);

        let mut pres_xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:presentation {PML_NS}><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rIdMaster1\"/></p:sldMasterIdLst>"
        );
        if has_notes {
            pres_xml.push_str("<p:notesMasterIdLst><p:notesMasterId r:id=\"rIdNotesMaster1\"/></p:notesMasterIdLst>");
        }
        pres_xml.push_str("<p:sldIdLst>");
        let mut pres_rels: Vec<(&str, &str, String)> = Vec::new();
        let slide_ids: Vec<String> = (1..=payload.slides.len())
            .map(|i| format!("rId{i}"))
            .collect();
        for (i, rid) in slide_ids.iter().enumerate() {
            let n = i + 1;
            pres_xml.push_str(&format!("<p:sldId id=\"{}\" r:id=\"{rid}\"/>", 255 + n));
            pres_rels.push((rid.as_str(), "slide", format!("slides/slide{n}.xml")));
        }
        pres_xml.push_str("</p:sldIdLst><p:sldSz cx=\"9144000\" cy=\"6858000\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>");
        pres_rels.push((
            "rIdMaster1",
            "slideMaster",
            "slideMasters/slideMaster1.xml".to_string(),
        ));
        pres_rels.push(("rIdTheme1", "theme", "theme/theme1.xml".to_string()));
        if has_notes {
            pres_rels.push((
                "rIdNotesMaster1",
                "notesMaster",
                "notesMasters/notesMaster1.xml".to_string(),
            ));
        }
        let pres_rels_xml = rels_xml(&pres_rels);

        let slide_master_xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sldMaster {PML_NS}><p:cSld><p:spTree>{PML_SP_TREE_HEADER}</p:spTree></p:cSld>{PML_CLR_MAP}<p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst></p:sldMaster>"
        );
        let slide_master_rels = rels_xml(&[
            (
                "rId1",
                "slideLayout",
                "../slideLayouts/slideLayout1.xml".to_string(),
            ),
            ("rId2", "theme", "../theme/theme1.xml".to_string()),
        ]);
        let slide_layout_xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sldLayout {PML_NS} type=\"obj\" preserve=\"1\"><p:cSld name=\"Title and Content\"><p:spTree>{PML_SP_TREE_HEADER}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"
        );
        let slide_layout_rels = rels_xml(&[(
            "rId1",
            "slideMaster",
            "../slideMasters/slideMaster1.xml".to_string(),
        )]);
        let core_xml = core_properties_xml(&payload.title);

        let mut entries: Vec<(String, Vec<u8>)> = vec![
            (
                "[Content_Types].xml".to_string(),
                content_types.into_bytes(),
            ),
            ("_rels/.rels".to_string(), root_rels.into_bytes()),
            ("docProps/core.xml".to_string(), core_xml.into_bytes()),
            ("ppt/presentation.xml".to_string(), pres_xml.into_bytes()),
            (
                "ppt/_rels/presentation.xml.rels".to_string(),
                pres_rels_xml.into_bytes(),
            ),
            (
                "ppt/slideMasters/slideMaster1.xml".to_string(),
                slide_master_xml.into_bytes(),
            ),
            (
                "ppt/slideMasters/_rels/slideMaster1.xml.rels".to_string(),
                slide_master_rels.into_bytes(),
            ),
            (
                "ppt/slideLayouts/slideLayout1.xml".to_string(),
                slide_layout_xml.into_bytes(),
            ),
            (
                "ppt/slideLayouts/_rels/slideLayout1.xml.rels".to_string(),
                slide_layout_rels.into_bytes(),
            ),
            (
                "ppt/theme/theme1.xml".to_string(),
                drawingml_theme_xml("EasyOffice").into_bytes(),
            ),
        ];
        if has_notes {
            let notes_master_xml = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:notesMaster {PML_NS}><p:cSld><p:spTree>{PML_SP_TREE_HEADER}</p:spTree></p:cSld>{PML_CLR_MAP}</p:notesMaster>"
            );
            entries.push((
                "ppt/notesMasters/notesMaster1.xml".to_string(),
                notes_master_xml.into_bytes(),
            ));
            entries.push((
                "ppt/notesMasters/_rels/notesMaster1.xml.rels".to_string(),
                rels_xml(&[("rId1", "theme", "../theme/theme2.xml".to_string())]).into_bytes(),
            ));
            entries.push((
                "ppt/theme/theme2.xml".to_string(),
                drawingml_theme_xml("EasyOffice Notes").into_bytes(),
            ));
        }

        for (idx, (s_title, bullets, notes)) in payload.slides.iter().enumerate() {
            let n = idx + 1;
            entries.push((
                format!("ppt/slides/slide{n}.xml"),
                slide_xml(s_title, bullets).into_bytes(),
            ));
            let mut slide_rels = vec![(
                "rId1",
                "slideLayout",
                "../slideLayouts/slideLayout1.xml".to_string(),
            )];
            if !notes.is_empty() {
                slide_rels.push((
                    "rId2",
                    "notesSlide",
                    format!("../notesSlides/notesSlide{n}.xml"),
                ));
                entries.push((
                    format!("ppt/notesSlides/notesSlide{n}.xml"),
                    notes_slide_xml(notes).into_bytes(),
                ));
                entries.push((
                    format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"),
                    rels_xml(&[
                        (
                            "rId1",
                            "notesMaster",
                            "../notesMasters/notesMaster1.xml".to_string(),
                        ),
                        ("rId2", "slide", format!("../slides/slide{n}.xml")),
                    ])
                    .into_bytes(),
                ));
            }
            entries.push((
                format!("ppt/slides/_rels/slide{n}.xml.rels"),
                rels_xml(&slide_rels).into_bytes(),
            ));
        }

        for (path, data) in &entries {
            if path.ends_with(".xml") || path.ends_with(".rels") {
                let xml = std::str::from_utf8(data)
                    .map_err(|_| EasyOfficeError::InvalidUtf8(format!("{path} UTF-8 오류")))?;
                XmlSanitizer::validate_safe_xml(xml)?;
            }
        }

        ZipArchiveCodec::pack(&entries)
    }

    pub fn unpack_pptx(bytes: &[u8]) -> Result<OoxmlPptxPayload, EasyOfficeError> {
        let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
        validate_zip_xml_entries(&entries)?;
        let pres_entry = entries
            .iter()
            .find(|e| e.path == "ppt/presentation.xml")
            .ok_or_else(|| {
                EasyOfficeError::CorruptedContainer(
                    "PPTX 필수 엔트리 'ppt/presentation.xml'이 누락되었습니다.".to_string(),
                )
            })?;
        let pres_xml = utf8_entry(pres_entry, "ppt/presentation.xml")?;

        let core_title = entries
            .iter()
            .find(|e| e.path == "docProps/core.xml")
            .and_then(|e| std::str::from_utf8(&e.data).ok())
            .and_then(|xml| extract_tag_bodies(xml, "dc:title").into_iter().next())
            .map(XmlSanitizer::unescape)
            .filter(|s| !s.is_empty());
        let title = core_title
            .or_else(|| {
                extract_attr(pres_xml, "<p:presentation", "title")
                    .or_else(|| extract_attr(pres_xml, "<p:presentation", "name"))
                    .map(|s| XmlSanitizer::unescape(&s))
            })
            .unwrap_or_else(|| "발표자료".to_string());
        let slideshow_ct = entries.iter().any(|e| {
            e.path == "[Content_Types].xml"
                && std::str::from_utf8(&e.data).is_ok_and(|ct| ct.contains(SLIDESHOW_MAIN_CT))
        });
        let is_slideshow = slideshow_ct
            || extract_attr(pres_xml, "<p:presentation", "slideshow").is_some_and(|s| s == "1");

        let mut rel_targets: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        if let Some(rels_entry) = entries
            .iter()
            .find(|e| e.path == "ppt/_rels/presentation.xml.rels")
        {
            let rels_xml = utf8_entry(rels_entry, "ppt/_rels/presentation.xml.rels")?;
            for (r_attrs, _) in extract_balanced_elements(rels_xml, "Relationship") {
                if let (Some(id), Some(target)) = (
                    extract_attr_from_fragment(r_attrs, "Id"),
                    extract_attr_from_fragment(r_attrs, "Target"),
                ) {
                    let normalized = if let Some(stripped) = target.strip_prefix('/') {
                        stripped.to_string()
                    } else if target.starts_with("ppt/") {
                        target
                    } else {
                        format!("ppt/{target}")
                    };
                    rel_targets.insert(id, normalized);
                }
            }
        }

        let mut ordered_slide_entries: Vec<&ZipArchiveEntry> = Vec::new();
        for (sld_id_attrs, _) in extract_balanced_elements(pres_xml, "p:sldId") {
            if let Some(rid) = extract_attr_from_fragment(sld_id_attrs, "r:id")
                .or_else(|| extract_attr_from_fragment(sld_id_attrs, "id"))
            {
                if let Some(target_path) = rel_targets.get(&rid) {
                    if let Some(entry) = entries.iter().find(|e| &e.path == target_path) {
                        ordered_slide_entries.push(entry);
                    }
                }
            }
        }

        if ordered_slide_entries.is_empty() {
            let slide_num = |path: &str| {
                path.strip_prefix("ppt/slides/slide")
                    .and_then(|s| s.strip_suffix(".xml"))
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(usize::MAX)
            };
            let mut slide_entries: Vec<_> = entries
                .iter()
                .filter(|e| e.path.starts_with("ppt/slides/slide") && e.path.ends_with(".xml"))
                .collect();
            slide_entries.sort_by(|a, b| {
                slide_num(&a.path)
                    .cmp(&slide_num(&b.path))
                    .then_with(|| a.path.cmp(&b.path))
            });
            ordered_slide_entries = slide_entries;
        }

        let mut slides = Vec::new();
        for s_entry in ordered_slide_entries {
            let s_xml = utf8_entry(s_entry, "ppt/slides/slide.xml")?;
            let shapes = extract_tag_bodies(s_xml, "p:sp");
            let title_shape = shapes.iter().position(|sp| is_title_placeholder(sp));

            let (s_title, bullets) = if let Some(t_idx) = title_shape {
                let title_text = shape_paragraph_texts(shapes[t_idx])
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                let bullets: Vec<String> = shapes
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != t_idx)
                    .flat_map(|(_, sp)| shape_paragraph_texts(sp))
                    .filter(|s| !s.is_empty())
                    .collect();
                (title_text, bullets)
            } else {
                let a_p_blocks = extract_tag_bodies(s_xml, "a:p");
                let mut bullets: Vec<String> = if !a_p_blocks.is_empty() {
                    shape_paragraph_texts(s_xml)
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect()
                } else {
                    extract_tag_bodies(s_xml, "a:t")
                        .into_iter()
                        .map(XmlSanitizer::unescape)
                        .collect()
                };
                let legacy_title = extract_attr(s_xml, "<p:sld", "title")
                    .map(|s| XmlSanitizer::unescape(&s))
                    .unwrap_or_else(|| {
                        if !bullets.is_empty() {
                            bullets.remove(0)
                        } else {
                            String::new()
                        }
                    });
                (legacy_title, bullets)
            };

            let notes = match read_notes_slide(&entries, &s_entry.path)? {
                Some(n) => n,
                None => extract_attr(s_xml, "<p:sld", "notes")
                    .map(|s| XmlSanitizer::unescape(&s))
                    .unwrap_or_default(),
            };
            slides.push((s_title, bullets, notes));
        }

        Ok(OoxmlPptxPayload {
            title,
            is_slideshow,
            slides,
        })
    }
}
