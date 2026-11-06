//! DOCX reader producing a [`RichDocument`]: runs (bold/italic/underline/strike), alignment,
//! heading levels (style names and outline levels), list kinds (list styles and `numPr` through
//! `numbering.xml`), and tables at their position in the body.

use std::collections::HashMap;

use crate::office_codecs::validate_zip_xml_entries;
use crate::rich_xml::{
    attr, heading_level_from_name, push_run, skip_subtree, toggle_on, tokenize, CharFmt, XmlEvent,
};
use crate::{
    EasyOfficeError, RichAlignment, RichBlock, RichDocument, RichListKind, RichParagraph,
    XmlSanitizer, ZipArchiveCodec, ZipArchiveEntry, ZipSecurityPolicy,
};

#[derive(Default)]
struct ParaState {
    runs: Vec<crate::RichRun>,
    style_id: Option<String>,
    jc: Option<String>,
    num_id: Option<String>,
    outline: Option<u8>,
}

fn entry_xml<'a>(
    entries: &'a [ZipArchiveEntry],
    path: &str,
) -> Result<Option<&'a str>, EasyOfficeError> {
    let Some(entry) = entries.iter().find(|e| e.path == path) else {
        return Ok(None);
    };
    let xml = std::str::from_utf8(&entry.data)
        .map_err(|_| EasyOfficeError::InvalidUtf8(format!("{path} UTF-8 오류")))?;
    XmlSanitizer::validate_safe_xml(xml)?;
    Ok(Some(xml))
}

/// styleId -> lower-cased style name.
fn read_styles(xml: &str) -> Result<HashMap<String, String>, EasyOfficeError> {
    let mut out = HashMap::new();
    let mut current: Option<String> = None;
    for ev in tokenize(xml)? {
        match ev {
            XmlEvent::Start { name, attrs } if name == "w:style" => {
                current = attr(&attrs, "w:styleId").map(str::to_string);
            }
            XmlEvent::Start { name, attrs } if name == "w:name" => {
                if let (Some(id), Some(val)) = (current.as_ref(), attr(&attrs, "w:val")) {
                    out.insert(id.clone(), val.to_string());
                }
            }
            XmlEvent::End(name) if name == "w:style" => current = None,
            _ => {}
        }
    }
    Ok(out)
}

/// numId -> list kind, resolved through the first level of the abstract numbering definition.
fn read_numbering(xml: &str) -> Result<HashMap<String, RichListKind>, EasyOfficeError> {
    let mut abstract_fmt: HashMap<String, String> = HashMap::new();
    let mut num_to_abstract: HashMap<String, String> = HashMap::new();
    let mut cur_abstract: Option<String> = None;
    let mut cur_num: Option<String> = None;
    let mut first_level = false;
    for ev in tokenize(xml)? {
        match ev {
            XmlEvent::Start { name, attrs } => match name.as_str() {
                "w:abstractNum" => {
                    cur_abstract = attr(&attrs, "w:abstractNumId").map(str::to_string);
                    first_level = true;
                }
                "w:lvl" if first_level => {}
                "w:numFmt" if first_level => {
                    if let (Some(id), Some(val)) = (cur_abstract.as_ref(), attr(&attrs, "w:val")) {
                        abstract_fmt
                            .entry(id.clone())
                            .or_insert_with(|| val.to_string());
                        first_level = false;
                    }
                }
                "w:num" => cur_num = attr(&attrs, "w:numId").map(str::to_string),
                "w:abstractNumId" if cur_num.is_some() => {
                    if let (Some(n), Some(a)) = (cur_num.as_ref(), attr(&attrs, "w:val")) {
                        num_to_abstract.insert(n.clone(), a.to_string());
                    }
                }
                _ => {}
            },
            XmlEvent::End(name) if name == "w:num" => cur_num = None,
            XmlEvent::End(name) if name == "w:abstractNum" => cur_abstract = None,
            _ => {}
        }
    }
    Ok(num_to_abstract
        .into_iter()
        .map(|(num, abs)| {
            let kind = match abstract_fmt.get(&abs).map(String::as_str) {
                Some("bullet" | "none") | None => RichListKind::Bullet,
                Some(_) => RichListKind::Ordered,
            };
            (num, kind)
        })
        .collect())
}

fn alignment(jc: Option<&str>) -> Option<RichAlignment> {
    jc.and_then(RichAlignment::parse)
}

struct Resolver<'a> {
    styles: &'a HashMap<String, String>,
    numbering: &'a HashMap<String, RichListKind>,
}

impl Resolver<'_> {
    fn finish(&self, p: ParaState) -> RichParagraph {
        let style_name = p
            .style_id
            .as_ref()
            .map(|id| self.styles.get(id).cloned().unwrap_or_else(|| id.clone()));
        let mut heading = style_name
            .as_deref()
            .map_or(0, heading_level_from_name)
            .max(p.style_id.as_deref().map_or(0, heading_level_from_name));
        if heading == 0 {
            if let Some(level) = p.outline.filter(|l| *l < 9) {
                heading = (level + 1).min(6);
            }
        }
        let lower_name = style_name.unwrap_or_default().to_lowercase();
        let list = if heading > 0 {
            None
        } else if lower_name.starts_with("list bullet") {
            Some(RichListKind::Bullet)
        } else if lower_name.starts_with("list number") {
            Some(RichListKind::Ordered)
        } else {
            p.num_id.as_deref().filter(|id| *id != "0").map(|id| {
                self.numbering
                    .get(id)
                    .copied()
                    .unwrap_or(RichListKind::Bullet)
            })
        };
        RichParagraph {
            alignment: alignment(p.jc.as_deref()),
            runs: p.runs,
            heading_level: heading,
            list,
        }
    }
}

fn plain(p: &ParaState) -> String {
    p.runs.iter().map(|r| r.text.as_str()).collect()
}

/// Decodes `word/document.xml` (plus styles and numbering) of a DOCX package.
pub(crate) fn read_docx(bytes: &[u8]) -> Result<(RichDocument, Vec<String>), EasyOfficeError> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
    validate_zip_xml_entries(&entries)?;
    let doc_xml = entry_xml(&entries, "word/document.xml")?.ok_or_else(|| {
        EasyOfficeError::CorruptedContainer(
            "DOCX 필수 엔트리 'word/document.xml'이 누락되었습니다.".to_string(),
        )
    })?;
    let styles = match entry_xml(&entries, "word/styles.xml")? {
        Some(xml) => read_styles(xml)?,
        None => HashMap::new(),
    };
    let numbering = match entry_xml(&entries, "word/numbering.xml")? {
        Some(xml) => read_numbering(xml)?,
        None => HashMap::new(),
    };
    let title = entry_xml(&entries, "docProps/core.xml")?
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
    let events = tokenize(doc_xml)?;
    let mut warnings: Vec<String> = Vec::new();
    let mut blocks: Vec<RichBlock> = Vec::new();

    let mut paras: Vec<ParaState> = Vec::new();
    let (mut in_ppr, mut in_run, mut in_rpr, mut in_t) = (false, false, false, false);
    let mut fmt = CharFmt::default();
    let mut cur_text = String::new();
    let mut tbl_depth = 0usize;
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut cur_row: Vec<String> = Vec::new();
    let mut cell_paras: Vec<String> = Vec::new();

    let mut i = 0usize;
    while i < events.len() {
        match &events[i] {
            XmlEvent::Start { name, attrs } => {
                match name.as_str() {
                    "mc:Fallback" | "w:rPrChange" | "w:pPrChange" | "w:sectPrChange"
                    | "w:tblPrChange" | "w:trPrChange" | "w:tcPrChange" | "w:del"
                    | "w:moveFrom" | "w:instrText" | "w:delText" => {
                        i = skip_subtree(&events, i);
                        continue;
                    }
                    "w:drawing" | "w:pict" | "w:object" => {
                        let note = "그림·도형·OLE 개체는 가져오지 않았습니다.".to_string();
                        if !warnings.contains(&note) {
                            warnings.push(note);
                        }
                        i = skip_subtree(&events, i);
                        continue;
                    }
                    "w:tbl" => {
                        tbl_depth += 1;
                        if tbl_depth == 1 {
                            rows.clear();
                        }
                    }
                    "w:tr" if tbl_depth == 1 => cur_row = Vec::new(),
                    "w:tc" if tbl_depth == 1 => cell_paras.clear(),
                    "w:p" => paras.push(ParaState::default()),
                    "w:pPr" => in_ppr = true,
                    "w:r" => {
                        in_run = true;
                        fmt = CharFmt::default();
                        cur_text.clear();
                    }
                    "w:rPr" if in_run => in_rpr = true,
                    "w:t" if in_run => in_t = true,
                    _ => {}
                }
                if let Some(top) = paras.last_mut() {
                    let val = attr(attrs, "w:val");
                    match name.as_str() {
                        "w:pStyle" if in_ppr => top.style_id = val.map(str::to_string),
                        "w:jc" if in_ppr => top.jc = val.map(str::to_string),
                        "w:numId" if in_ppr => top.num_id = val.map(str::to_string),
                        "w:outlineLvl" if in_ppr => top.outline = val.and_then(|v| v.parse().ok()),
                        _ => {}
                    }
                }
                if in_run && in_rpr {
                    match name.as_str() {
                        "w:b" => fmt.bold = toggle_on(attr(attrs, "w:val")),
                        "w:i" => fmt.italic = toggle_on(attr(attrs, "w:val")),
                        "w:strike" | "w:dstrike" => fmt.strike = toggle_on(attr(attrs, "w:val")),
                        "w:u" => fmt.underline = toggle_on(attr(attrs, "w:val")),
                        _ => {}
                    }
                } else if in_run && !in_ppr {
                    match name.as_str() {
                        "w:tab" | "w:ptab" => cur_text.push('\t'),
                        "w:br" if !matches!(attr(attrs, "w:type"), Some("page" | "column")) => {
                            cur_text.push('\n');
                        }
                        "w:cr" => cur_text.push('\n'),
                        "w:noBreakHyphen" => cur_text.push('-'),
                        _ => {}
                    }
                }
            }
            XmlEvent::Text(text) => {
                if in_t {
                    cur_text.push_str(text);
                }
            }
            XmlEvent::End(name) => match name.as_str() {
                "w:t" => in_t = false,
                "w:rPr" => in_rpr = false,
                "w:pPr" => in_ppr = false,
                "w:r" => {
                    if let Some(top) = paras.last_mut() {
                        push_run(&mut top.runs, &cur_text, fmt);
                    }
                    cur_text.clear();
                    in_run = false;
                    in_rpr = false;
                    in_t = false;
                }
                "w:p" => {
                    if let Some(p) = paras.pop() {
                        if tbl_depth > 0 {
                            cell_paras.push(plain(&p));
                        } else if p.runs.iter().any(|r| !r.text.trim().is_empty()) {
                            blocks.push(RichBlock::Paragraph(resolver.finish(p)));
                        }
                    }
                }
                "w:tc" if tbl_depth == 1 => {
                    while cell_paras.last().is_some_and(String::is_empty) {
                        cell_paras.pop();
                    }
                    cur_row.push(cell_paras.join("\n"));
                    cell_paras.clear();
                }
                "w:tr" if tbl_depth == 1 => rows.push(std::mem::take(&mut cur_row)),
                "w:tbl" => {
                    tbl_depth = tbl_depth.saturating_sub(1);
                    if tbl_depth == 0 && !rows.is_empty() {
                        blocks.push(RichBlock::Table(std::mem::take(&mut rows)));
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    if entries.iter().any(|e| {
        let p = e.path.as_str();
        (p.starts_with("word/header") || p.starts_with("word/footer")) && p.ends_with(".xml")
    }) {
        warnings.push("머리말·꼬리말은 가져오지 않았습니다.".to_string());
    }
    Ok((
        RichDocument {
            title,
            blocks,
            official_agency: None,
        },
        warnings,
    ))
}
