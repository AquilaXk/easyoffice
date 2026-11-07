//! ODT reader producing a [`RichDocument`] from `content.xml` automatic styles: span formats,
//! paragraph alignment, `text:h` outline levels, list paragraphs, and tables in position.

use std::collections::HashMap;

use crate::office_codecs::validate_zip_xml_entries;
use crate::rich_document::normalize_hex_color;
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

/// `14pt`/`9.5pt` to half-points; other units are ignored.
fn parse_points(value: &str) -> Option<u16> {
    let pt: f32 = value.trim().strip_suffix("pt")?.parse().ok()?;
    let half = (pt * 2.0).round();
    (half >= 1.0 && half <= f32::from(u16::MAX)).then_some(half as u16)
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
                            size_half_pt: attr(attrs, "fo:font-size").and_then(parse_points),
                            font: attr(attrs, "style:font-name")
                                .or_else(|| attr(attrs, "fo:font-family"))
                                .map(|f| f.trim().trim_matches('\'').to_string())
                                .filter(|f| !f.is_empty()),
                            color: attr(attrs, "fo:color").and_then(normalize_hex_color),
                            highlight: attr(attrs, "fo:background-color")
                                .and_then(normalize_hex_color),
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

const MAX_REPEATED_SPACES: usize = 1024;
const MAX_OUTLINE_LEVEL: u8 = 6;
const UNSUPPORTED_OBJECT_NOTE: &str = "그림·도형·OLE 개체는 가져오지 않았습니다.";

/// Walker state for ODF `content.xml` events.
struct OdtWalker<'a> {
    styles: &'a HashMap<String, StyleProps>,
    list_styles: &'a HashMap<String, RichListKind>,
    title: String,
    warnings: Vec<String>,
    blocks: Vec<RichBlock>,
    paras: Vec<Para>,
    span_fmts: Vec<CharFmt>,
    list_stack: Vec<RichListKind>,
    tbl_depth: usize,
    rows: Vec<Vec<String>>,
    cur_row: Vec<String>,
    cell_paras: Vec<String>,
    in_automatic_styles: bool,
}

/// Merges a span's character format into the enclosing format.
fn merge_span_fmt(cur: CharFmt, add: CharFmt) -> CharFmt {
    CharFmt {
        bold: cur.bold || add.bold,
        italic: cur.italic || add.italic,
        underline: cur.underline || add.underline,
        strike: cur.strike || add.strike,
        size_half_pt: add.size_half_pt.or(cur.size_half_pt),
        font: add.font.or(cur.font),
        color: add.color.or(cur.color),
        highlight: add.highlight.or(cur.highlight),
    }
}

impl<'a> OdtWalker<'a> {
    fn new(
        styles: &'a HashMap<String, StyleProps>,
        list_styles: &'a HashMap<String, RichListKind>,
    ) -> Self {
        Self {
            styles,
            list_styles,
            title: String::new(),
            warnings: Vec::new(),
            blocks: Vec::new(),
            paras: Vec::new(),
            span_fmts: Vec::new(),
            list_stack: Vec::new(),
            tbl_depth: 0,
            rows: Vec::new(),
            cur_row: Vec::new(),
            cell_paras: Vec::new(),
            in_automatic_styles: false,
        }
    }

    /// Handles an element start; returns true when the subtree must be skipped.
    fn on_start(&mut self, name: &str, attrs: &[(String, String)]) -> bool {
        match name {
            "office:document-content" => {
                self.title = attr(attrs, "title").unwrap_or("").to_string()
            }
            "office:automatic-styles" | "office:font-face-decls" => self.in_automatic_styles = true,
            "text:note" | "office:annotation" | "text:tracked-changes" => return true,
            "draw:frame" | "draw:custom-shape" => {
                let note = UNSUPPORTED_OBJECT_NOTE.to_string();
                if !self.warnings.contains(&note) {
                    self.warnings.push(note);
                }
                return true;
            }
            "text:p" | "text:h" if !self.in_automatic_styles => self.start_paragraph(name, attrs),
            "text:span" if !self.paras.is_empty() => self.start_span(attrs),
            "text:tab" => self.push_inline("\t"),
            "text:line-break" => self.push_inline("\n"),
            "text:s" => {
                let n = attr(attrs, "text:c")
                    .and_then(|c| c.parse::<usize>().ok())
                    .unwrap_or(1);
                self.push_inline(&" ".repeat(n.min(MAX_REPEATED_SPACES)));
            }
            "text:list" => {
                let kind = attr(attrs, "text:style-name")
                    .and_then(|n| self.list_styles.get(n))
                    .copied()
                    .or_else(|| self.list_stack.last().copied())
                    .unwrap_or(RichListKind::Bullet);
                self.list_stack.push(kind);
            }
            "table:table" => {
                self.tbl_depth += 1;
                if self.tbl_depth == 1 {
                    self.rows.clear();
                }
            }
            "table:table-row" if self.tbl_depth == 1 => self.cur_row = Vec::new(),
            "table:table-cell" if self.tbl_depth == 1 => self.cell_paras.clear(),
            "table:covered-table-cell" if self.tbl_depth == 1 => self.cur_row.push(String::new()),
            _ => {}
        }
        false
    }

    fn start_paragraph(&mut self, name: &str, attrs: &[(String, String)]) {
        let style = attr(attrs, "text:style-name")
            .and_then(|n| self.styles.get(n))
            .cloned()
            .unwrap_or_default();
        let outline = if name == "text:h" {
            attr(attrs, "text:outline-level")
                .and_then(|l| l.parse::<u8>().ok())
                .unwrap_or(1)
                .clamp(1, MAX_OUTLINE_LEVEL)
        } else {
            0
        };
        let base = if outline > 0 {
            CharFmt::default()
        } else {
            style.fmt.clone()
        };
        self.span_fmts.clear();
        self.span_fmts.push(base.clone());
        self.paras.push(Para {
            runs: Vec::new(),
            style,
            outline,
            base,
        });
    }

    fn start_span(&mut self, attrs: &[(String, String)]) {
        let cur = self.span_fmts.last().cloned().unwrap_or_default();
        let add = attr(attrs, "text:style-name")
            .and_then(|n| self.styles.get(n))
            .map(|s| s.fmt.clone())
            .unwrap_or_default();
        self.span_fmts.push(merge_span_fmt(cur, add));
    }

    fn push_inline(&mut self, text: &str) {
        push_inline(&mut self.paras, &self.span_fmts, text);
    }

    fn on_end(&mut self, name: &str) {
        match name {
            "office:automatic-styles" | "office:font-face-decls" => {
                self.in_automatic_styles = false
            }
            "text:span" if !self.paras.is_empty() => {
                if self.span_fmts.len() > 1 {
                    self.span_fmts.pop();
                }
            }
            "text:list" => {
                self.list_stack.pop();
            }
            "text:p" | "text:h" if !self.in_automatic_styles => self.finish_paragraph(),
            "table:table-cell" if self.tbl_depth == 1 => {
                while self.cell_paras.last().is_some_and(String::is_empty) {
                    self.cell_paras.pop();
                }
                self.cur_row.push(self.cell_paras.join("\n"));
                self.cell_paras.clear();
            }
            "table:table-row" if self.tbl_depth == 1 => {
                self.rows.push(std::mem::take(&mut self.cur_row))
            }
            "table:table" => {
                self.tbl_depth = self.tbl_depth.saturating_sub(1);
                if self.tbl_depth == 0 && !self.rows.is_empty() {
                    self.blocks
                        .push(RichBlock::Table(std::mem::take(&mut self.rows)));
                }
            }
            _ => {}
        }
    }

    fn finish_paragraph(&mut self) {
        let Some(p) = self.paras.pop() else {
            return;
        };
        let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
        if self.tbl_depth > 0 {
            self.cell_paras.push(text);
        } else if !text.trim().is_empty() {
            let list = if p.outline > 0 {
                None
            } else if let Some(kind) = self.list_stack.last() {
                Some(*kind)
            } else if p.style.list_margin {
                Some(RichListKind::Bullet)
            } else {
                None
            };
            self.blocks.push(RichBlock::Paragraph(RichParagraph {
                runs: p.runs,
                alignment: p.style.align,
                heading_level: p.outline,
                list,
            }));
        }
        self.span_fmts.clear();
        let base = self
            .paras
            .last()
            .map_or_else(CharFmt::default, |q| q.base.clone());
        self.span_fmts.push(base);
    }
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

    let mut walker = OdtWalker::new(&styles, &list_styles);
    let mut i = 0usize;
    while i < events.len() {
        match &events[i] {
            XmlEvent::Start { name, attrs } => {
                if walker.on_start(name, attrs) {
                    i = skip_subtree(&events, i);
                    continue;
                }
            }
            XmlEvent::Text(text) => walker.push_inline(text),
            XmlEvent::End(name) => walker.on_end(name),
        }
        i += 1;
    }
    let OdtWalker {
        mut title,
        warnings,
        blocks,
        ..
    } = walker;
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
            &span_fmts.last().cloned().unwrap_or_default(),
        );
    }
}
