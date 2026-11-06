//! HWPX reader producing a [`RichDocument`]: character shapes (`hh:charPr`) become run formats,
//! paragraph shapes (`hh:paraPr`) alignment and list kind, styles (`hh:style`, "개요 N") heading
//! levels, and `hp:tbl` tables keep their position in the section.

use std::collections::HashMap;

use crate::hwpx_parts::LIST_LEFT_MARGIN;
use crate::office_codecs::validate_zip_xml_entries;
use crate::rich_xml::{
    attr, heading_level_from_name, push_run, skip_subtree, tokenize, CharFmt, XmlEvent,
};
use crate::{
    EasyOfficeError, HwpxCodec, RichAlignment, RichBlock, RichDocument, RichListKind,
    RichParagraph, RichRun, XmlSanitizer, ZipArchiveCodec, ZipSecurityPolicy,
};

#[derive(Default, Clone)]
struct ParaShape {
    align: Option<String>,
    heading_type: String,
    heading_level: u8,
    left: u32,
}

#[derive(Default, Clone)]
struct StyleInfo {
    name: String,
    eng_name: String,
    para_pr: String,
    char_pr: String,
}

#[derive(Default)]
struct Header {
    chars: HashMap<String, CharFmt>,
    paras: HashMap<String, ParaShape>,
    styles: HashMap<String, StyleInfo>,
}

fn read_header(xml: &str) -> Result<Header, EasyOfficeError> {
    let mut h = Header::default();
    let mut cur_char: Option<(String, CharFmt)> = None;
    let mut cur_para: Option<(String, ParaShape, bool)> = None;
    for ev in tokenize(xml)? {
        match ev {
            XmlEvent::Start { name, attrs } => match name.as_str() {
                "hh:charPr" => {
                    cur_char = attr(&attrs, "id").map(|id| (id.to_string(), CharFmt::default()));
                }
                "hh:bold" => {
                    if let Some((_, f)) = cur_char.as_mut() {
                        f.bold = true;
                    }
                }
                "hh:italic" => {
                    if let Some((_, f)) = cur_char.as_mut() {
                        f.italic = true;
                    }
                }
                "hh:underline" => {
                    if let Some((_, f)) = cur_char.as_mut() {
                        f.underline = attr(&attrs, "type").is_some_and(|t| t != "NONE");
                    }
                }
                "hh:strikeout" => {
                    if let Some((_, f)) = cur_char.as_mut() {
                        f.strike = attr(&attrs, "shape").is_some_and(|t| t != "NONE");
                    }
                }
                "hh:paraPr" => {
                    cur_para =
                        attr(&attrs, "id").map(|id| (id.to_string(), ParaShape::default(), false));
                }
                "hh:align" => {
                    if let Some((_, p, _)) = cur_para.as_mut() {
                        p.align = attr(&attrs, "horizontal").map(str::to_string);
                    }
                }
                "hh:heading" => {
                    if let Some((_, p, _)) = cur_para.as_mut() {
                        p.heading_type = attr(&attrs, "type").unwrap_or("NONE").to_string();
                        p.heading_level = attr(&attrs, "level")
                            .and_then(|l| l.parse().ok())
                            .unwrap_or(0);
                    }
                }
                "hc:left" => {
                    if let Some((_, p, seen)) = cur_para.as_mut() {
                        if !*seen {
                            *seen = true;
                            p.left = attr(&attrs, "value")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0);
                        }
                    }
                }
                "hh:style" => {
                    if let Some(id) = attr(&attrs, "id") {
                        h.styles.insert(
                            id.to_string(),
                            StyleInfo {
                                name: attr(&attrs, "name").unwrap_or("").to_string(),
                                eng_name: attr(&attrs, "engName").unwrap_or("").to_string(),
                                para_pr: attr(&attrs, "paraPrIDRef").unwrap_or("").to_string(),
                                char_pr: attr(&attrs, "charPrIDRef").unwrap_or("").to_string(),
                            },
                        );
                    }
                }
                _ => {}
            },
            XmlEvent::End(name) => match name.as_str() {
                "hh:charPr" => {
                    if let Some((id, f)) = cur_char.take() {
                        h.chars.insert(id, f);
                    }
                }
                "hh:paraPr" => {
                    if let Some((id, p, _)) = cur_para.take() {
                        h.paras.insert(id, p);
                    }
                }
                _ => {}
            },
            XmlEvent::Text(_) => {}
        }
    }
    Ok(h)
}

#[derive(Default)]
struct ParaState {
    runs: Vec<RichRun>,
    para_pr: String,
    style: String,
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
    let list = if heading > 0 {
        None
    } else {
        match shape.heading_type.as_str() {
            "BULLET" => Some(RichListKind::Bullet),
            "NUMBER" => Some(RichListKind::Ordered),
            _ if shape.left == LIST_LEFT_MARGIN => Some(RichListKind::Bullet),
            _ => None,
        }
    };
    let alignment = match shape.align.as_deref() {
        Some("LEFT") => Some(RichAlignment::Left),
        Some("CENTER") => Some(RichAlignment::Center),
        Some("RIGHT") => Some(RichAlignment::Right),
        Some("DISTRIBUTE" | "DISTRIBUTE_SPACE") => Some(RichAlignment::Justify),
        _ => None,
    };
    let mut runs = p.runs;
    // Heading styles are bold by definition, so the flag is not user formatting there.
    let style_bold = style.is_some_and(|s| header.chars.get(&s.char_pr).is_some_and(|f| f.bold));
    if heading > 0 && style_bold {
        let mut merged: Vec<RichRun> = Vec::new();
        for r in runs {
            push_run(
                &mut merged,
                &r.text,
                CharFmt {
                    bold: false,
                    italic: r.italic,
                    underline: r.underline,
                    strike: r.strike,
                },
            );
        }
        runs = merged;
    }
    RichParagraph {
        runs,
        alignment,
        heading_level: heading,
        list,
    }
}

const SKIPPED_SUBTREES: [&str; 14] = [
    "hp:secPr",
    "hp:ctrl",
    "hp:equation",
    "hp:pic",
    "hp:ole",
    "hp:container",
    "hp:rect",
    "hp:ellipse",
    "hp:line",
    "hp:arc",
    "hp:polygon",
    "hp:curve",
    "hp:connectLine",
    "hp:textart",
];

fn read_section(
    xml: &str,
    header: &Header,
    blocks: &mut Vec<RichBlock>,
    warnings: &mut Vec<String>,
) -> Result<(), EasyOfficeError> {
    let events = tokenize(xml)?;
    let mut paras: Vec<ParaState> = Vec::new();
    let mut runs: Vec<RunState> = Vec::new();
    let mut t_depth = 0usize;
    let mut tbl_depth = 0usize;
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut cur_row: Vec<String> = Vec::new();
    let mut cell_paras: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < events.len() {
        match &events[i] {
            XmlEvent::Start { name, attrs } => {
                if SKIPPED_SUBTREES.contains(&name.as_str()) {
                    if !matches!(name.as_str(), "hp:secPr" | "hp:ctrl") {
                        let note = "그림·도형·수식·머리말은 가져오지 않았습니다.".to_string();
                        if !warnings.contains(&note) {
                            warnings.push(note);
                        }
                    }
                    i = skip_subtree(&events, i);
                    continue;
                }
                match name.as_str() {
                    "hp:p" => paras.push(ParaState {
                        runs: Vec::new(),
                        para_pr: attr(attrs, "paraPrIDRef").unwrap_or("").to_string(),
                        style: attr(attrs, "styleIDRef").unwrap_or("").to_string(),
                    }),
                    "hp:run" => runs.push(RunState {
                        char_pr: attr(attrs, "charPrIDRef").unwrap_or("").to_string(),
                        text: String::new(),
                    }),
                    "hp:t" => t_depth += 1,
                    "hp:tab" if t_depth > 0 => push_text(&mut runs, "\t"),
                    "hp:lineBreak" if t_depth > 0 => push_text(&mut runs, "\n"),
                    "hp:nbSpace" | "hp:fwSpace" if t_depth > 0 => push_text(&mut runs, " "),
                    "hp:hyphen" if t_depth > 0 => push_text(&mut runs, "-"),
                    "hp:tbl" => {
                        tbl_depth += 1;
                        if tbl_depth == 1 {
                            table_rows.clear();
                        }
                    }
                    "hp:tr" if tbl_depth == 1 => cur_row = Vec::new(),
                    "hp:tc" if tbl_depth == 1 => cell_paras.clear(),
                    _ => {}
                }
            }
            XmlEvent::Text(text) => {
                if t_depth > 0 {
                    push_text(&mut runs, text);
                }
            }
            XmlEvent::End(name) => match name.as_str() {
                "hp:t" => t_depth = t_depth.saturating_sub(1),
                "hp:run" => {
                    if let Some(run) = runs.pop() {
                        let fmt = header.chars.get(&run.char_pr).copied().unwrap_or_default();
                        if let Some(top) = paras.last_mut() {
                            push_run(&mut top.runs, &run.text, fmt);
                        }
                    }
                }
                "hp:p" => {
                    if let Some(p) = paras.pop() {
                        if tbl_depth > 0 {
                            cell_paras.push(p.runs.iter().map(|r| r.text.as_str()).collect());
                        } else if p.runs.iter().any(|r| !r.text.trim().is_empty()) {
                            blocks.push(RichBlock::Paragraph(finish_paragraph(header, p)));
                        }
                    }
                }
                "hp:tc" if tbl_depth == 1 => {
                    while cell_paras.last().is_some_and(String::is_empty) {
                        cell_paras.pop();
                    }
                    cur_row.push(cell_paras.join("\n"));
                    cell_paras.clear();
                }
                "hp:tr" if tbl_depth == 1 => table_rows.push(std::mem::take(&mut cur_row)),
                "hp:tbl" => {
                    tbl_depth = tbl_depth.saturating_sub(1);
                    if tbl_depth == 0 && !table_rows.is_empty() {
                        blocks.push(RichBlock::Table(std::mem::take(&mut table_rows)));
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    Ok(())
}

fn push_text(runs: &mut [RunState], text: &str) {
    if let Some(run) = runs.last_mut() {
        run.text.push_str(text);
    }
}

/// Decodes every `Contents/section*.xml` of a HWPX/HWTX package in order.
pub(crate) fn read_hwpx(bytes: &[u8]) -> Result<(RichDocument, Vec<String>), EasyOfficeError> {
    // Reuses the legacy reader for package validation, title, and issuing agency.
    let meta = HwpxCodec::unpack(bytes)?;
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
    validate_zip_xml_entries(&entries)?;
    let header = match entries.iter().find(|e| e.path == "Contents/header.xml") {
        Some(e) => {
            let xml = std::str::from_utf8(&e.data).map_err(|_| {
                EasyOfficeError::InvalidUtf8("Contents/header.xml UTF-8 오류".to_string())
            })?;
            XmlSanitizer::validate_safe_xml(xml)?;
            read_header(xml)?
        }
        None => Header::default(),
    };
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
    let mut blocks = Vec::new();
    let mut warnings = Vec::new();
    for (_, entry) in sections {
        let xml = std::str::from_utf8(&entry.data).map_err(|_| {
            EasyOfficeError::InvalidUtf8("Contents/section.xml UTF-8 오류".to_string())
        })?;
        XmlSanitizer::validate_safe_xml(xml)?;
        read_section(xml, &header, &mut blocks, &mut warnings)?;
    }
    if meta.official_agency.is_some() {
        warnings.push("기관명 등 공문서 메타데이터는 저장 시 다시 기록되지 않습니다.".to_string());
    }
    Ok((
        RichDocument {
            title: meta.title,
            blocks,
            official_agency: meta.official_agency,
        },
        warnings,
    ))
}
