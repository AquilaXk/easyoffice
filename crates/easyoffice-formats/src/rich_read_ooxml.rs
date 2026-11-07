//! DOCX reader producing a [`RichDocument`]: runs (bold/italic/underline/strike), alignment,
//! heading levels (style names and outline levels), list kinds (list styles and `numPr` through
//! `numbering.xml`), and tables at their position in the body.

use std::collections::HashMap;

use crate::office_codecs::validate_zip_xml_entries;
use crate::rich_document::{highlight_name_to_hex, normalize_hex_color};
use crate::rich_read_images::{drawing_picture, DocxMedia, DrawingOutcome};
use crate::rich_xml::{
    attr, heading_level_from_name, push_run, skip_subtree, toggle_on, tokenize, CharFmt, XmlEvent,
};
use crate::{
    EasyOfficeError, RichAlignment, RichBlock, RichDocument, RichImage, RichListKind,
    RichParagraph, RichScript, XmlSanitizer, ZipArchiveCodec, ZipArchiveEntry, ZipSecurityPolicy,
    MAX_IMAGES_PER_DOCUMENT, MAX_TOTAL_IMAGE_BYTES,
};

#[derive(Default)]
struct ParaState {
    runs: Vec<crate::RichRun>,
    style_id: Option<String>,
    jc: Option<String>,
    num_id: Option<String>,
    outline: Option<u8>,
    images: Vec<RichImage>,
    left_indent: Option<u32>,
    first_line: Option<i32>,
    line_percent: Option<u16>,
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
            left_indent_twips: p.left_indent,
            first_line_indent_twips: p.first_line,
            line_spacing_percent: p.line_percent,
        }
    }
}

fn plain(p: &ParaState) -> String {
    p.runs.iter().map(|r| r.text.as_str()).collect()
}

/// Elements whose whole subtree is ignored (revisions, fallbacks, deleted text).
const SKIPPED_ELEMENTS: [&str; 11] = [
    "mc:Fallback",
    "w:rPrChange",
    "w:pPrChange",
    "w:sectPrChange",
    "w:tblPrChange",
    "w:trPrChange",
    "w:tcPrChange",
    "w:del",
    "w:moveFrom",
    "w:instrText",
    "w:delText",
];
const UNSUPPORTED_OBJECT_ELEMENTS: [&str; 2] = ["w:pict", "w:object"];
const DRAWING_ELEMENT: &str = "w:drawing";
/// `w:spacing/@w:line` of single line spacing (240ths of a line).
const DOCX_SINGLE_LINE_240THS: i64 = 240;
const SINGLE_LINE_PERCENT: u16 = 100;
const UNSUPPORTED_OBJECT_NOTE: &str = "그림·도형·OLE 개체는 가져오지 않았습니다.";

/// Applies a run-property element (`w:b`, `w:sz`, ...) to `fmt`.
fn apply_run_property(fmt: &mut CharFmt, name: &str, attrs: &[(String, String)]) {
    let val = attr(attrs, "w:val");
    match name {
        "w:b" => fmt.bold = toggle_on(val),
        "w:i" => fmt.italic = toggle_on(val),
        "w:strike" | "w:dstrike" => fmt.strike = toggle_on(val),
        "w:u" => fmt.underline = toggle_on(val),
        "w:rFonts" => {
            fmt.font = ["w:ascii", "w:hAnsi", "w:eastAsia"]
                .iter()
                .find_map(|k| attr(attrs, k))
                .filter(|f| !f.trim().is_empty())
                .map(str::to_string);
        }
        "w:sz" => fmt.size_half_pt = val.and_then(|v| v.parse::<u16>().ok()).filter(|v| *v > 0),
        "w:color" => fmt.color = val.and_then(normalize_hex_color),
        "w:highlight" => fmt.highlight = val.and_then(highlight_name_to_hex),
        "w:vertAlign" => {
            fmt.script = match val {
                Some("superscript") => RichScript::Superscript,
                Some("subscript") => RichScript::Subscript,
                _ => RichScript::Normal,
            };
        }
        "w:shd" => {
            if let Some(fill) = attr(attrs, "w:fill").and_then(normalize_hex_color) {
                fmt.highlight = Some(fill);
            }
        }
        _ => {}
    }
}

/// `w:spacing` (auto line rule, 240ths of a line) and `w:ind` of a paragraph.
fn apply_para_layout(top: &mut ParaState, name: &str, attrs: &[(String, String)]) {
    let number = |key: &str| attr(attrs, key).and_then(|v| v.trim().parse::<i64>().ok());
    match name {
        "w:spacing" if attr(attrs, "w:lineRule").is_none_or(|r| r == "auto") => {
            top.line_percent = number("w:line")
                .filter(|l| *l > 0)
                .and_then(|l| u16::try_from(l * 100 / DOCX_SINGLE_LINE_240THS).ok())
                .filter(|p| *p != SINGLE_LINE_PERCENT);
        }
        "w:ind" => {
            top.left_indent = number("w:left").and_then(|v| u32::try_from(v).ok());
            top.first_line = number("w:firstLine")
                .or_else(|| number("w:hanging").map(|h| -h))
                .and_then(|v| i32::try_from(v).ok());
        }
        _ => {}
    }
}

/// Mutable walker state for `word/document.xml` events.
#[derive(Default)]
struct DocxWalker {
    warnings: Vec<String>,
    blocks: Vec<RichBlock>,
    paras: Vec<ParaState>,
    in_ppr: bool,
    in_run: bool,
    in_rpr: bool,
    in_t: bool,
    fmt: CharFmt,
    cur_text: String,
    tbl_depth: usize,
    rows: Vec<Vec<String>>,
    cur_row: Vec<String>,
    cell_paras: Vec<String>,
    image_count: usize,
    image_bytes: usize,
}

impl DocxWalker {
    /// Handles an element start; returns true when the whole subtree must be skipped.
    fn on_start(&mut self, name: &str, attrs: &[(String, String)]) -> bool {
        if SKIPPED_ELEMENTS.contains(&name) {
            return true;
        }
        if UNSUPPORTED_OBJECT_ELEMENTS.contains(&name) {
            let note = UNSUPPORTED_OBJECT_NOTE.to_string();
            if !self.warnings.contains(&note) {
                self.warnings.push(note);
            }
            return true;
        }
        self.track_structure(name);
        if let Some(top) = self.paras.last_mut() {
            if self.in_ppr {
                Self::apply_para_property(top, name, attr(attrs, "w:val"));
                apply_para_layout(top, name, attrs);
            }
        }
        if self.in_run && self.in_rpr {
            apply_run_property(&mut self.fmt, name, attrs);
        } else if self.in_run && !self.in_ppr {
            self.apply_inline_break(name, attrs);
        }
        false
    }

    fn track_structure(&mut self, name: &str) {
        match name {
            "w:tbl" => {
                self.tbl_depth += 1;
                if self.tbl_depth == 1 {
                    self.rows.clear();
                }
            }
            "w:tr" if self.tbl_depth == 1 => self.cur_row = Vec::new(),
            "w:tc" if self.tbl_depth == 1 => self.cell_paras.clear(),
            "w:p" => self.paras.push(ParaState::default()),
            "w:pPr" => self.in_ppr = true,
            "w:r" => {
                self.in_run = true;
                self.fmt = CharFmt::default();
                self.cur_text.clear();
            }
            "w:rPr" if self.in_run => self.in_rpr = true,
            "w:t" if self.in_run => self.in_t = true,
            _ => {}
        }
    }

    fn apply_para_property(top: &mut ParaState, name: &str, val: Option<&str>) {
        match name {
            "w:pStyle" => top.style_id = val.map(str::to_string),
            "w:jc" => top.jc = val.map(str::to_string),
            "w:numId" => top.num_id = val.map(str::to_string),
            "w:outlineLvl" => top.outline = val.and_then(|v| v.parse().ok()),
            _ => {}
        }
    }

    fn apply_inline_break(&mut self, name: &str, attrs: &[(String, String)]) {
        match name {
            "w:tab" | "w:ptab" => self.cur_text.push('\t'),
            "w:br" if !matches!(attr(attrs, "w:type"), Some("page" | "column")) => {
                self.cur_text.push('\n');
            }
            "w:cr" => self.cur_text.push('\n'),
            "w:noBreakHyphen" => self.cur_text.push('-'),
            _ => {}
        }
    }

    fn on_text(&mut self, text: &str) {
        if self.in_t {
            self.cur_text.push_str(text);
        }
    }

    fn on_end(&mut self, name: &str, resolver: &Resolver<'_>) {
        match name {
            "w:t" => self.in_t = false,
            "w:rPr" => self.in_rpr = false,
            "w:pPr" => self.in_ppr = false,
            "w:r" => {
                if let Some(top) = self.paras.last_mut() {
                    push_run(&mut top.runs, &self.cur_text, &self.fmt);
                }
                self.cur_text.clear();
                self.in_run = false;
                self.in_rpr = false;
                self.in_t = false;
            }
            "w:p" => self.finish_paragraph(resolver),
            "w:tc" if self.tbl_depth == 1 => {
                while self.cell_paras.last().is_some_and(String::is_empty) {
                    self.cell_paras.pop();
                }
                self.cur_row.push(self.cell_paras.join("\n"));
                self.cell_paras.clear();
            }
            "w:tr" if self.tbl_depth == 1 => self.rows.push(std::mem::take(&mut self.cur_row)),
            "w:tbl" => {
                self.tbl_depth = self.tbl_depth.saturating_sub(1);
                if self.tbl_depth == 0 && !self.rows.is_empty() {
                    self.blocks
                        .push(RichBlock::Table(std::mem::take(&mut self.rows)));
                }
            }
            _ => {}
        }
    }

    /// Records the picture of a `w:drawing` in the open paragraph, or a warning when skipped.
    fn on_drawing(
        &mut self,
        events: &[XmlEvent],
        media: &DocxMedia<'_>,
    ) -> Result<(), EasyOfficeError> {
        let img = match drawing_picture(events, media) {
            DrawingOutcome::Picture(img) => img,
            DrawingOutcome::Skipped(note) => {
                self.warn_once(note);
                return Ok(());
            }
        };
        if self.tbl_depth > 0 || self.paras.is_empty() {
            self.warn_once("표 안의 그림은 가져오지 않았습니다.".to_string());
            return Ok(());
        }
        self.image_count += 1;
        self.image_bytes = self.image_bytes.saturating_add(img.bytes().len());
        if self.image_count > MAX_IMAGES_PER_DOCUMENT || self.image_bytes > MAX_TOTAL_IMAGE_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "DOCX 그림이 문서당 한도(개수·합계 크기)를 넘었습니다.".to_string(),
            ));
        }
        if let Some(top) = self.paras.last_mut() {
            top.images.push(img);
        }
        Ok(())
    }

    fn warn_once(&mut self, note: String) {
        if !self.warnings.contains(&note) {
            self.warnings.push(note);
        }
    }

    fn finish_paragraph(&mut self, resolver: &Resolver<'_>) {
        let Some(mut p) = self.paras.pop() else {
            return;
        };
        let images = std::mem::take(&mut p.images);
        if self.tbl_depth > 0 {
            self.cell_paras.push(plain(&p));
        } else {
            if p.runs.iter().any(|r| !r.text.trim().is_empty()) {
                self.blocks.push(RichBlock::Paragraph(resolver.finish(p)));
            }
            self.blocks.extend(images.into_iter().map(RichBlock::Image));
        }
    }
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
    let media = DocxMedia::load(&entries)?;
    let mut walker = DocxWalker::default();
    let mut i = 0usize;
    while i < events.len() {
        match &events[i] {
            XmlEvent::Start { name, .. } if name == DRAWING_ELEMENT => {
                let end = skip_subtree(&events, i);
                walker.on_drawing(&events[i..end], &media)?;
                i = end;
                continue;
            }
            XmlEvent::Start { name, attrs } => {
                if walker.on_start(name, attrs) {
                    i = skip_subtree(&events, i);
                    continue;
                }
            }
            XmlEvent::Text(text) => walker.on_text(text),
            XmlEvent::End(name) => walker.on_end(name, &resolver),
        }
        i += 1;
    }
    let mut warnings = walker.warnings;
    if entries.iter().any(|e| {
        let p = e.path.as_str();
        (p.starts_with("word/header") || p.starts_with("word/footer")) && p.ends_with(".xml")
    }) {
        warnings.push("머리말·꼬리말은 가져오지 않았습니다.".to_string());
    }
    Ok((
        RichDocument {
            title,
            blocks: walker.blocks,
            official_agency: None,
        },
        warnings,
    ))
}
