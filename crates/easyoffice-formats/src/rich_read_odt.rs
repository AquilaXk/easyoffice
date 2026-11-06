//! ODT reader producing a [`RichDocument`] from `content.xml` automatic styles: span formats,
//! paragraph alignment, `text:h` outline levels, list paragraphs, and tables in position.

use std::collections::HashMap;

use crate::office_codecs::validate_zip_xml_entries;
use crate::rich_xml::{attr, push_run, skip_subtree, tokenize, CharFmt, XmlEvent};
use crate::{
    EasyOfficeError, OdfCodec, RichAlignment, RichBlock, RichDocument, RichListKind, RichParagraph,
    RichRun, XmlSanitizer, ZipArchiveCodec, ZipSecurityPolicy,
};

/// Left margin the ODT writer gives list paragraphs; the reader treats it as the list marker.
const LIST_MARGIN: &str = "0.5in";

#[derive(Default, Clone)]
struct StyleProps {
    align: Option<RichAlignment>,
    fmt: CharFmt,
    list_margin: bool,
}

fn off(value: Option<&str>) -> bool {
    value.is_none_or(|v| v == "none")
}

fn read_styles(
    events: &[XmlEvent],
) -> (HashMap<String, StyleProps>, HashMap<String, RichListKind>) {
    let mut styles: HashMap<String, StyleProps> = HashMap::new();
    let mut lists: HashMap<String, RichListKind> = HashMap::new();
    let mut cur: Option<(String, StyleProps)> = None;
    let mut cur_list: Option<String> = None;
    for ev in events {
        match ev {
            XmlEvent::Start { name, attrs } => match name.as_str() {
                "style:style" => {
                    cur = attr(attrs, "style:name").map(|n| (n.to_string(), StyleProps::default()));
                }
                "style:paragraph-properties" => {
                    if let Some((_, p)) = cur.as_mut() {
                        p.align = attr(attrs, "fo:text-align").and_then(RichAlignment::parse);
                        p.list_margin = attr(attrs, "fo:margin-left") == Some(LIST_MARGIN);
                    }
                }
                "style:text-properties" => {
                    if let Some((_, p)) = cur.as_mut() {
                        p.fmt = CharFmt {
                            bold: attr(attrs, "fo:font-weight").is_some_and(|w| {
                                w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600)
                            }),
                            italic: attr(attrs, "fo:font-style")
                                .is_some_and(|s| s == "italic" || s == "oblique"),
                            underline: !off(attr(attrs, "style:text-underline-style")),
                            strike: !off(attr(attrs, "style:text-line-through-style")),
                        };
                    }
                }
                "text:list-style" => cur_list = attr(attrs, "style:name").map(str::to_string),
                "text:list-level-style-number" => {
                    if let Some(n) = cur_list.as_ref() {
                        lists.entry(n.clone()).or_insert(RichListKind::Ordered);
                    }
                }
                "text:list-level-style-bullet" => {
                    if let Some(n) = cur_list.as_ref() {
                        lists.entry(n.clone()).or_insert(RichListKind::Bullet);
                    }
                }
                _ => {}
            },
            XmlEvent::End(name) => match name.as_str() {
                "style:style" => {
                    if let Some((n, p)) = cur.take() {
                        styles.insert(n, p);
                    }
                }
                "text:list-style" => cur_list = None,
                _ => {}
            },
            XmlEvent::Text(_) => {}
        }
    }
    (styles, lists)
}

struct Para {
    runs: Vec<RichRun>,
    style: StyleProps,
    outline: u8,
    base: CharFmt,
}

/// Decodes `content.xml` of an ODT package.
pub(crate) fn read_odt(bytes: &[u8]) -> Result<(RichDocument, Vec<String>), EasyOfficeError> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
    validate_zip_xml_entries(&entries)?;
    if let Some(m) = entries.iter().find(|e| e.path == "mimetype") {
        if std::str::from_utf8(&m.data).unwrap_or_default().trim() != OdfCodec::MIMETYPE_ODT {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: crate::ContainerFamily::ZipOdf,
                detail: "ODF mimetype이 텍스트 문서(ODT)가 아닙니다.".to_string(),
            });
        }
    }
    let content = entries
        .iter()
        .find(|e| e.path == "content.xml")
        .ok_or_else(|| {
            EasyOfficeError::CorruptedContainer(
                "ODF 필수 엔트리 'content.xml'이 누락되었습니다.".to_string(),
            )
        })?;
    let xml = std::str::from_utf8(&content.data)
        .map_err(|_| EasyOfficeError::InvalidUtf8("ODF content.xml UTF-8 오류".to_string()))?;
    XmlSanitizer::validate_safe_xml(xml)?;
    let events = tokenize(xml)?;
    let (styles, list_styles) = read_styles(&events);

    let mut title = String::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut blocks: Vec<RichBlock> = Vec::new();
    let mut paras: Vec<Para> = Vec::new();
    let mut span_fmts: Vec<CharFmt> = Vec::new();
    let mut list_stack: Vec<RichListKind> = Vec::new();
    let mut tbl_depth = 0usize;
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut cur_row: Vec<String> = Vec::new();
    let mut cell_paras: Vec<String> = Vec::new();
    let mut in_automatic_styles = false;
    let mut i = 0usize;
    while i < events.len() {
        match &events[i] {
            XmlEvent::Start { name, attrs } => match name.as_str() {
                "office:document-content" => {
                    title = attr(attrs, "title").unwrap_or("").to_string();
                }
                "office:automatic-styles" | "office:font-face-decls" => {
                    in_automatic_styles = true;
                }
                "text:note" | "office:annotation" | "text:tracked-changes" => {
                    i = skip_subtree(&events, i);
                    continue;
                }
                "draw:frame" | "draw:custom-shape" => {
                    let note = "그림·도형·OLE 개체는 가져오지 않았습니다.".to_string();
                    if !warnings.contains(&note) {
                        warnings.push(note);
                    }
                    i = skip_subtree(&events, i);
                    continue;
                }
                "text:p" | "text:h" if !in_automatic_styles => {
                    let style = attr(attrs, "text:style-name")
                        .and_then(|n| styles.get(n))
                        .cloned()
                        .unwrap_or_default();
                    let outline = if name == "text:h" {
                        attr(attrs, "text:outline-level")
                            .and_then(|l| l.parse::<u8>().ok())
                            .unwrap_or(1)
                            .clamp(1, 6)
                    } else {
                        0
                    };
                    let base = if outline > 0 {
                        CharFmt::default()
                    } else {
                        style.fmt
                    };
                    span_fmts.clear();
                    span_fmts.push(base);
                    paras.push(Para {
                        runs: Vec::new(),
                        style,
                        outline,
                        base,
                    });
                }
                "text:span" if !paras.is_empty() => {
                    let cur = span_fmts.last().copied().unwrap_or_default();
                    let add = attr(attrs, "text:style-name")
                        .and_then(|n| styles.get(n))
                        .map(|s| s.fmt)
                        .unwrap_or_default();
                    span_fmts.push(CharFmt {
                        bold: cur.bold || add.bold,
                        italic: cur.italic || add.italic,
                        underline: cur.underline || add.underline,
                        strike: cur.strike || add.strike,
                    });
                }
                "text:tab" => push_inline(&mut paras, &span_fmts, "\t"),
                "text:line-break" => push_inline(&mut paras, &span_fmts, "\n"),
                "text:s" => {
                    let n = attr(attrs, "text:c")
                        .and_then(|c| c.parse::<usize>().ok())
                        .unwrap_or(1);
                    push_inline(&mut paras, &span_fmts, &" ".repeat(n.min(1024)));
                }
                "text:list" => {
                    let kind = attr(attrs, "text:style-name")
                        .and_then(|n| list_styles.get(n))
                        .copied()
                        .or_else(|| list_stack.last().copied())
                        .unwrap_or(RichListKind::Bullet);
                    list_stack.push(kind);
                }
                "table:table" => {
                    tbl_depth += 1;
                    if tbl_depth == 1 {
                        rows.clear();
                    }
                }
                "table:table-row" if tbl_depth == 1 => cur_row = Vec::new(),
                "table:table-cell" if tbl_depth == 1 => cell_paras.clear(),
                "table:covered-table-cell" if tbl_depth == 1 => cur_row.push(String::new()),
                _ => {}
            },
            XmlEvent::Text(text) => push_inline(&mut paras, &span_fmts, text),
            XmlEvent::End(name) => match name.as_str() {
                "office:automatic-styles" | "office:font-face-decls" => {
                    in_automatic_styles = false;
                }
                "text:span" if !paras.is_empty() => {
                    if span_fmts.len() > 1 {
                        span_fmts.pop();
                    }
                }
                "text:list" => {
                    list_stack.pop();
                }
                "text:p" | "text:h" if !in_automatic_styles => {
                    if let Some(p) = paras.pop() {
                        let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
                        if tbl_depth > 0 {
                            cell_paras.push(text);
                        } else if !text.trim().is_empty() {
                            let list = if p.outline > 0 {
                                None
                            } else if let Some(kind) = list_stack.last() {
                                Some(*kind)
                            } else if p.style.list_margin {
                                Some(RichListKind::Bullet)
                            } else {
                                None
                            };
                            blocks.push(RichBlock::Paragraph(RichParagraph {
                                runs: p.runs,
                                alignment: p.style.align,
                                heading_level: p.outline,
                                list,
                            }));
                        }
                        span_fmts.clear();
                        span_fmts.push(paras.last().map_or(CharFmt::default(), |q| q.base));
                    }
                }
                "table:table-cell" if tbl_depth == 1 => {
                    while cell_paras.last().is_some_and(String::is_empty) {
                        cell_paras.pop();
                    }
                    cur_row.push(cell_paras.join("\n"));
                    cell_paras.clear();
                }
                "table:table-row" if tbl_depth == 1 => rows.push(std::mem::take(&mut cur_row)),
                "table:table" => {
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
    if title.is_empty() {
        if let Some(meta) = entries.iter().find(|e| e.path == "meta.xml") {
            if let Ok(m) = std::str::from_utf8(&meta.data) {
                title = crate::rtf_xml_helpers::extract_tag_bodies(m, "dc:title")
                    .into_iter()
                    .next()
                    .map(XmlSanitizer::unescape)
                    .unwrap_or_default();
            }
        }
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

fn push_inline(paras: &mut [Para], span_fmts: &[CharFmt], text: &str) {
    if let Some(p) = paras.last_mut() {
        push_run(
            &mut p.runs,
            text,
            span_fmts.last().copied().unwrap_or_default(),
        );
    }
}
