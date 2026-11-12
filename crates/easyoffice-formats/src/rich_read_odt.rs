//! ODT reader producing a [`RichDocument`] from `content.xml` automatic styles: span formats,
//! paragraph alignment, `text:h` outline levels, list paragraphs, and tables in position.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::office_codecs::{checked_part, checked_zip_xml_entries};
use crate::rich_document::{normalize_hex_color, safe_link_target};
use crate::rich_image::{safe_package_path, ImageBudget};
use crate::rich_table::{TableAssembler, MAX_PADDED_COLUMNS};
use crate::rich_xml::{push_linked_run, CharFmt};
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{
    EasyOfficeError, OdfCodec, RichAlignment, RichBlock, RichDocument, RichImage, RichListKind,
    RichParagraph, RichRun, RichScript, XmlSanitizer, ZipArchiveCodec, ZipArchiveEntry,
    ZipSecurityPolicy,
};

const EMU_PER_INCH: f64 = 914_400.0;
const EMU_PER_CM: f64 = 360_000.0;
const EMU_PER_MM: f64 = 36_000.0;
const EMU_PER_POINT: f64 = 12_700.0;
const EMU_PER_PIXEL: f64 = 9_525.0;
/// EMU per twip (1/20 point).
const EMU_PER_TWIP: f64 = 635.0;
const MAX_LENGTH_EMU: f64 = 182_880_000.0;
const MAX_LINE_PERCENT: f64 = 1_000.0;
const SINGLE_LINE_PERCENT: u16 = 100;

/// Left margin the ODT writer gives list paragraphs; the reader treats it as the list marker.
const LIST_MARGIN: &str = "0.5in";

const MAX_STYLE_INHERIT_DEPTH: usize = 16;
const SPACES_64: &str = "                                                                ";

#[derive(Default, Clone)]
struct StyleProps {
    parent: Option<String>,
    align: Option<RichAlignment>,
    fmt: CharFmt,
    list_margin: bool,
    list_kind: Option<RichListKind>,
    left_twips: Option<u32>,
    first_line_twips: Option<i32>,
    line_percent: Option<u16>,
}

/// A CSS/ODF length (`36pt`, `0.5in`, `1.27cm`, `12.7mm`, `48px`) in EMU.
fn length_emu(value: &str) -> Option<f64> {
    let v = value.trim();
    let (number, unit) = v.split_at(v.find(|c: char| c.is_ascii_alphabetic())?);
    let n: f64 = number.trim().parse().ok()?;
    let per_unit = match unit {
        "in" => EMU_PER_INCH,
        "cm" => EMU_PER_CM,
        "mm" => EMU_PER_MM,
        "pt" => EMU_PER_POINT,
        "px" => EMU_PER_PIXEL,
        _ => return None,
    };
    let emu = n * per_unit;
    (emu.is_finite() && emu.abs() <= MAX_LENGTH_EMU).then_some(emu)
}

fn length_twips(value: &str) -> Option<i32> {
    length_emu(value).map(|e| (e / EMU_PER_TWIP).round() as i32)
}

/// `fo:line-height` as a percentage of single spacing (`200%`); other units and 100% are unset.
fn line_height_percent(value: &str) -> Option<u16> {
    let n: f64 = value.trim().strip_suffix('%')?.trim().parse().ok()?;
    let percent = n.round();
    (n.is_finite()
        && (1.0..=MAX_LINE_PERCENT).contains(&percent)
        && percent as u16 != SINGLE_LINE_PERCENT)
        .then_some(percent as u16)
}

fn off(value: Option<&str>) -> bool {
    value.is_none_or(|v| v == "none")
}

/// `14pt`/`9.5pt` to half-points; other units are ignored.
/// `style:text-position` such as `super 58%`, `sub 58%` or `33% 58%` (positive raise = super).
fn script_of(position: Option<&str>) -> RichScript {
    let first = position
        .and_then(|p| p.split_whitespace().next())
        .unwrap_or("");
    match first {
        "super" => RichScript::Superscript,
        "sub" => RichScript::Subscript,
        other => match other.strip_suffix('%').and_then(|n| n.parse::<i32>().ok()) {
            Some(n) if n > 0 => RichScript::Superscript,
            Some(n) if n < 0 => RichScript::Subscript,
            _ => RichScript::Normal,
        },
    }
}

fn parse_points(value: &str) -> Option<u16> {
    let pt: f32 = value.trim().strip_suffix("pt")?.parse().ok()?;
    let half = (pt * 2.0).round();
    (1.0..=f32::from(u16::MAX))
        .contains(&half)
        .then_some(half as u16)
}

/// Character format described by a `style:text-properties` element.
fn char_fmt_from_text_properties(attrs: &Attrs<'_>) -> CharFmt {
    CharFmt {
        bold: attr(attrs, "fo:font-weight")
            .is_some_and(|w| w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600)),
        italic: attr(attrs, "fo:font-style").is_some_and(|s| s == "italic" || s == "oblique"),
        underline: !off(attr(attrs, "style:text-underline-style").as_deref()),
        strike: !off(attr(attrs, "style:text-line-through-style").as_deref()),
        size_half_pt: attr(attrs, "fo:font-size")
            .as_deref()
            .and_then(parse_points),
        font: attr(attrs, "style:font-name")
            .or_else(|| attr(attrs, "fo:font-family"))
            .map(|f| f.trim().trim_matches('\'').to_string())
            .filter(|f| !f.is_empty()),
        color: attr(attrs, "fo:color")
            .as_deref()
            .and_then(normalize_hex_color),
        highlight: attr(attrs, "fo:background-color")
            .as_deref()
            .and_then(normalize_hex_color),
        script: script_of(attr(attrs, "style:text-position").as_deref()),
    }
}

/// Alignment, list marker margin, indents and line spacing of `style:paragraph-properties`.
fn apply_paragraph_properties(p: &mut StyleProps, attrs: &Attrs<'_>) {
    p.align = attr(attrs, "fo:text-align")
        .as_deref()
        .and_then(RichAlignment::parse);
    p.list_margin = attr(attrs, "fo:margin-left").as_deref() == Some(LIST_MARGIN);
    if !p.list_margin {
        p.left_twips = attr(attrs, "fo:margin-left")
            .as_deref()
            .and_then(length_twips)
            .and_then(|t| u32::try_from(t).ok())
            .filter(|t| *t > 0);
    }
    p.first_line_twips = attr(attrs, "fo:text-indent")
        .as_deref()
        .and_then(length_twips)
        .filter(|t| *t != 0);
    p.line_percent = attr(attrs, "fo:line-height")
        .as_deref()
        .and_then(line_height_percent);
}

fn read_styles_into(
    xml: &str,
    styles: &mut HashMap<String, StyleProps>,
    lists: &mut HashMap<String, RichListKind>,
) -> Result<(), EasyOfficeError> {
    let mut cur: Option<(String, StyleProps)> = None;
    let mut cur_list: Option<String> = None;
    for ev in XmlReader::new(xml) {
        match ev? {
            XmlEvent::Start { name, ref attrs } => match name {
                "style:style" => {
                    let list_kind = match attr(attrs, "style:list-style-name").as_deref() {
                        Some("L_Ordered" | "Numbering" | "Ordered") => Some(RichListKind::Ordered),
                        Some("L_Bullet" | "Bullet") => Some(RichListKind::Bullet),
                        _ => None,
                    };
                    let parent = attr(attrs, "style:parent-style-name").map(Cow::into_owned);
                    cur = attr(attrs, "style:name").map(|n| {
                        (
                            n.to_string(),
                            StyleProps {
                                parent,
                                list_kind,
                                ..StyleProps::default()
                            },
                        )
                    });
                }
                "style:paragraph-properties" => {
                    if let Some((_, p)) = cur.as_mut() {
                        apply_paragraph_properties(p, attrs);
                    }
                }
                "style:text-properties" => {
                    if let Some((_, p)) = cur.as_mut() {
                        p.fmt = char_fmt_from_text_properties(attrs);
                    }
                }
                "text:list-style" => cur_list = attr(attrs, "style:name").map(Cow::into_owned),
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
            XmlEvent::End(name) => match name {
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
    Ok(())
}

fn resolve_style_parents(styles: &mut HashMap<String, StyleProps>) {
    let snapshot = styles.clone();
    for (name, props) in styles.iter_mut() {
        let mut cur_parent = props.parent.as_deref();
        for _ in 0..MAX_STYLE_INHERIT_DEPTH {
            let Some(p_name) = cur_parent else {
                break;
            };
            if p_name == name {
                break;
            }
            let Some(p) = snapshot.get(p_name) else {
                break;
            };
            if props.align.is_none() {
                props.align = p.align;
            }
            if !props.list_margin {
                props.list_margin = p.list_margin;
            }
            if props.list_kind.is_none() {
                props.list_kind = p.list_kind;
            }
            if props.left_twips.is_none() {
                props.left_twips = p.left_twips;
            }
            if props.first_line_twips.is_none() {
                props.first_line_twips = p.first_line_twips;
            }
            if props.line_percent.is_none() {
                props.line_percent = p.line_percent;
            }
            props.fmt = merge_span_fmt(p.fmt.clone(), props.fmt.clone());
            cur_parent = p.parent.as_deref();
        }
    }
}

struct Para {
    runs: Vec<RichRun>,
    style: StyleProps,
    outline: u8,
    base: CharFmt,
    images: Vec<RichImage>,
}

const MAX_REPEATED_SPACES: usize = 1024;
const MAX_OUTLINE_LEVEL: u8 = 6;
const UNSUPPORTED_OBJECT_NOTE: &str = "그림·도형·OLE 개체는 가져오지 않았습니다.";

/// A positive count attribute; unreadable values are 1 and huge ones saturate.
fn count_attr(attrs: &Attrs<'_>, name: &str) -> u32 {
    attr(attrs, name)
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(1, |v| u32::try_from(v).unwrap_or(u32::MAX).max(1))
}

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
    table: TableAssembler,
    cell_paras: Vec<String>,
    /// Row span, column span and repeat count of the open top-level cell.
    cell_shape: (u32, u32, u32),
    /// Merged regions dropped as invalid in this document.
    dropped_merges: usize,
    in_automatic_styles: bool,
    budget: ImageBudget,
    /// Address of the open `text:a`, when it is a safe link.
    link: Option<String>,
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
        script: if add.script == RichScript::Normal {
            cur.script
        } else {
            add.script
        },
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
            table: TableAssembler::default(),
            cell_paras: Vec::new(),
            cell_shape: (1, 1, 1),
            dropped_merges: 0,
            in_automatic_styles: false,
            budget: ImageBudget::default(),
            link: None,
        }
    }

    /// Handles an element start; returns true when the subtree must be skipped.
    fn on_start(&mut self, name: &str, attrs: &Attrs<'_>) -> bool {
        match name {
            "office:document-content" => {
                self.title = attr(attrs, "title").unwrap_or_default().into_owned()
            }
            "office:automatic-styles" | "office:font-face-decls" => self.in_automatic_styles = true,
            "text:note" | "office:annotation" | "text:tracked-changes" => return true,
            "draw:custom-shape" => {
                if !self.warnings.iter().any(|w| w == UNSUPPORTED_OBJECT_NOTE) {
                    self.warnings.push(UNSUPPORTED_OBJECT_NOTE.to_string());
                }
                return true;
            }
            "text:p" | "text:h" if !self.in_automatic_styles => self.start_paragraph(name, attrs),
            "text:span" if !self.paras.is_empty() => self.start_span(attrs),
            "text:a" => {
                self.link = attr(attrs, "xlink:href")
                    .and_then(|href| safe_link_target(&href).map(str::to_string));
            }
            "text:tab" => self.push_inline("\t"),
            "text:line-break" => self.push_inline("\n"),
            "text:s" => {
                let mut rem = attr(attrs, "text:c")
                    .as_deref()
                    .and_then(|c| c.parse::<usize>().ok())
                    .unwrap_or(1)
                    .min(MAX_REPEATED_SPACES);
                while rem > 0 {
                    let chunk = rem.min(SPACES_64.len());
                    self.push_inline(&SPACES_64[..chunk]);
                    rem -= chunk;
                }
            }
            "text:list" => {
                let kind = attr(attrs, "text:style-name")
                    .as_deref()
                    .and_then(|n| self.list_styles.get(n))
                    .copied()
                    .or_else(|| self.list_stack.last().copied())
                    .unwrap_or(RichListKind::Bullet);
                self.list_stack.push(kind);
            }
            "table:table" => {
                self.tbl_depth += 1;
                if self.tbl_depth == 1 {
                    self.table.reset();
                }
            }
            "table:table-row" if self.tbl_depth == 1 => self.table.start_row(),
            "table:table-cell" if self.tbl_depth == 1 => {
                self.cell_paras.clear();
                self.cell_shape = (
                    count_attr(attrs, "table:number-rows-spanned"),
                    count_attr(attrs, "table:number-columns-spanned"),
                    count_attr(attrs, "table:number-columns-repeated"),
                );
            }
            "table:covered-table-cell" if self.tbl_depth == 1 => {
                let repeated = count_attr(attrs, "table:number-columns-repeated");
                for _ in 0..repeated.min(self.padding_room()) {
                    self.table.covered();
                }
            }
            _ => {}
        }
        false
    }

    fn start_paragraph(&mut self, name: &str, attrs: &Attrs<'_>) {
        let style = attr(attrs, "text:style-name")
            .as_deref()
            .and_then(|n| self.styles.get(n))
            .cloned()
            .unwrap_or_default();
        let outline = if name == "text:h" {
            attr(attrs, "text:outline-level")
                .as_deref()
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
            images: Vec::new(),
        });
    }

    fn start_span(&mut self, attrs: &Attrs<'_>) {
        let cur = self.span_fmts.last().cloned().unwrap_or_default();
        let add = attr(attrs, "text:style-name")
            .as_deref()
            .and_then(|n| self.styles.get(n))
            .map(|s| s.fmt.clone())
            .unwrap_or_default();
        self.span_fmts.push(merge_span_fmt(cur, add));
    }

    fn push_inline(&mut self, text: &str) {
        push_inline(&mut self.paras, &self.span_fmts, self.link.as_deref(), text);
    }

    fn warn_once(&mut self, note: &str) {
        if !self.warnings.iter().any(|w| w == note) {
            self.warnings.push(note.to_string());
        }
    }

    /// Reads a `draw:frame` (events span its start to its end): an embedded PNG/JPEG becomes a
    /// picture of the open paragraph; text boxes, charts and anything else are skipped.
    fn on_frame(
        &mut self,
        events: &[XmlEvent<'_>],
        entry_by_path: &HashMap<&str, &ZipArchiveEntry>,
    ) -> Result<(), EasyOfficeError> {
        let Some(frame) = read_frame(events) else {
            self.warn_once(UNSUPPORTED_OBJECT_NOTE);
            return Ok(());
        };
        if self.tbl_depth > 0 || self.paras.is_empty() {
            self.warn_once("표 안의 그림은 가져오지 않았습니다.");
            return Ok(());
        }
        let entry = safe_package_path(&frame.href)
            .and_then(|path| entry_by_path.get(path.as_str()).copied());
        let Some(entry) = entry else {
            self.warn_once("외부 링크이거나 찾을 수 없는 그림은 가져오지 않았습니다.");
            return Ok(());
        };
        let mut image = match RichImage::new(entry.data.clone(), frame.alt.as_str()) {
            Ok(image) => image,
            Err(err) => {
                self.warnings.push(format!(
                    "그림 '{}'을 가져오지 않았습니다: {err}",
                    entry.path
                ));
                return Ok(());
            }
        };
        self.budget.charge(entry.data.len())?;
        if let Some((w, h)) = frame.size_emu {
            // An out-of-range size keeps the default derived from the pixels.
            let _ = image.set_display_emu(w, h);
        }
        if let Some(top) = self.paras.last_mut() {
            top.images.push(image);
        }
        Ok(())
    }

    fn on_end(&mut self, name: &str) {
        match name {
            "office:automatic-styles" | "office:font-face-decls" => {
                self.in_automatic_styles = false
            }
            "text:a" => self.link = None,
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
                let text = self.cell_paras.join("\n");
                self.cell_paras.clear();
                self.push_cell(text);
            }
            "table:table-row" if self.tbl_depth == 1 => self.table.end_row(),
            "table:table" => {
                self.tbl_depth = self.tbl_depth.saturating_sub(1);
                if self.tbl_depth == 0 && !self.table.is_empty() {
                    let (table, dropped) = std::mem::take(&mut self.table).finish();
                    self.dropped_merges += dropped;
                    self.blocks.push(RichBlock::Table(table));
                }
            }
            _ => {}
        }
    }

    /// Cells that a repeat count may still add to the open row.
    fn padding_room(&self) -> u32 {
        u32::try_from(MAX_PADDED_COLUMNS.saturating_sub(self.table.row_len())).unwrap_or(0)
    }

    /// Adds the closed cell, repeated as declared (bounded by the row width limit).
    fn push_cell(&mut self, text: String) {
        let (row_span, col_span, repeated) = self.cell_shape;
        let repeated = repeated.min(self.padding_room()).max(1);
        self.table.anchor(text.clone(), row_span, col_span);
        for _ in 1..repeated {
            self.table.cell(text.clone());
        }
    }

    fn finish_paragraph(&mut self) {
        let Some(p) = self.paras.pop() else {
            return;
        };
        let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
        let images = p.images;
        if self.tbl_depth > 0 {
            self.cell_paras.push(text);
        } else if !text.trim().is_empty() {
            let list = if p.outline > 0 {
                None
            } else if let Some(kind) = self.list_stack.last() {
                Some(*kind)
            } else if let Some(kind) = p.style.list_kind {
                Some(kind)
            } else if p.style.list_margin {
                Some(RichListKind::Bullet)
            } else {
                None
            };
            self.blocks.push(RichBlock::Paragraph(RichParagraph {
                list_level: 0,
                list_number: None,
                runs: p.runs,
                alignment: p.style.align,
                heading_level: p.outline,
                list,
                left_indent_twips: p.style.left_twips,
                first_line_indent_twips: p.style.first_line_twips,
                line_spacing_percent: p.style.line_percent,
            }));
        }
        if self.tbl_depth == 0 {
            self.blocks.extend(images.into_iter().map(RichBlock::Image));
        }
        self.span_fmts.clear();
        let base = self
            .paras
            .last()
            .map_or_else(CharFmt::default, |q| q.base.clone());
        self.span_fmts.push(base);
    }
}

/// Decodes `content.xml` (and `styles.xml`) of an ODT package.
pub(crate) fn read_odt(bytes: &[u8]) -> Result<(RichDocument, Vec<String>), EasyOfficeError> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())?;
    // Every XML entry (styles.xml and content.xml included) is checked once, here.
    let parts = checked_zip_xml_entries(&entries)?;
    let entry_by_path: HashMap<&str, &ZipArchiveEntry> =
        entries.iter().map(|e| (e.path.as_str(), e)).collect();
    if let Some(&m) = entry_by_path.get("mimetype") {
        if std::str::from_utf8(&m.data).unwrap_or_default().trim() != OdfCodec::MIMETYPE_ODT {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: crate::ContainerFamily::ZipOdf,
                detail: "ODF mimetype이 텍스트 문서(ODT)가 아닙니다.".to_string(),
            });
        }
    }
    let xml = checked_part(&parts, "content.xml").ok_or_else(|| {
        EasyOfficeError::CorruptedContainer(
            "ODF 필수 엔트리 'content.xml'이 누락되었습니다.".to_string(),
        )
    })?;
    let mut styles: HashMap<String, StyleProps> = HashMap::new();
    let mut list_styles: HashMap<String, RichListKind> = HashMap::new();
    if let Some(s_xml) = checked_part(&parts, "styles.xml") {
        read_styles_into(s_xml, &mut styles, &mut list_styles)?;
    }
    read_styles_into(xml, &mut styles, &mut list_styles)?;
    resolve_style_parents(&mut styles);

    let mut walker = OdtWalker::new(&styles, &list_styles);
    let mut reader = XmlReader::new(xml);
    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Start {
                name: "draw:frame", ..
            } => {
                let frame = reader.collect_subtree(event)?;
                walker.on_frame(&frame, &entry_by_path)?;
            }
            XmlEvent::Start { name, ref attrs } => {
                if walker.on_start(name, attrs) {
                    reader.skip_subtree()?;
                }
            }
            XmlEvent::Text(text) => walker.push_inline(&text),
            XmlEvent::End(name) => walker.on_end(name),
        }
    }
    let OdtWalker {
        mut title,
        mut warnings,
        blocks,
        dropped_merges,
        ..
    } = walker;
    if dropped_merges > 0 {
        warnings.push(format!(
            "표의 병합 {dropped_merges}곳은 범위를 벗어나거나 겹쳐 가져오지 않았습니다."
        ));
    }
    if title.is_empty() {
        if let Some(&meta) = entry_by_path.get("meta.xml") {
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

/// Fields of one `draw:frame` holding a `draw:image`.
struct Frame {
    href: String,
    alt: String,
    size_emu: Option<(u64, u64)>,
}

fn read_frame(events: &[XmlEvent<'_>]) -> Option<Frame> {
    let mut href = None;
    let mut size_emu = None;
    let (mut desc, mut title) = (String::new(), String::new());
    let mut capture: Option<&str> = None;
    for (idx, event) in events.iter().enumerate() {
        match event {
            XmlEvent::Start { name, attrs } => match *name {
                "draw:frame" if idx == 0 => {
                    let side = |key| {
                        attr(attrs, key)
                            .as_deref()
                            .and_then(length_emu)
                            .filter(|e| *e >= 1.0)
                    };
                    size_emu = side("svg:width")
                        .zip(side("svg:height"))
                        .map(|(w, h)| (w.round() as u64, h.round() as u64));
                }
                "draw:image" if href.is_none() => {
                    href = attr(attrs, "xlink:href").map(Cow::into_owned);
                }
                "svg:desc" => capture = Some("desc"),
                "svg:title" => capture = Some("title"),
                _ => {}
            },
            XmlEvent::Text(text) => match capture {
                Some("desc") => desc.push_str(text),
                Some(_) => title.push_str(text),
                None => {}
            },
            XmlEvent::End(name) if *name == "svg:desc" || *name == "svg:title" => capture = None,
            XmlEvent::End(_) => {}
        }
    }
    let alt = if desc.trim().is_empty() { title } else { desc };
    Some(Frame {
        href: href?,
        alt,
        size_emu,
    })
}

fn push_inline(paras: &mut [Para], span_fmts: &[CharFmt], link: Option<&str>, text: &str) {
    if let Some(p) = paras.last_mut() {
        push_linked_run(
            &mut p.runs,
            text,
            &span_fmts.last().cloned().unwrap_or_default(),
            link,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odt_resolves_parent_style_inheritance_and_cycle_guard() {
        let styles_xml = r##"<office:document-styles>
            <office:styles>
                <style:style style:name="ParentPara" style:family="paragraph">
                    <style:paragraph-properties fo:text-align="center" fo:line-height="150%"/>
                    <style:text-properties fo:font-weight="bold" fo:color="#112233"/>
                </style:style>
                <style:style style:name="Cycle1" style:parent-style-name="Cycle2"/>
                <style:style style:name="Cycle2" style:parent-style-name="Cycle1"/>
            </office:styles>
        </office:document-styles>"##;
        let auto_xml = r#"<office:document-content>
            <office:automatic-styles>
                <style:style style:name="P1" style:parent-style-name="ParentPara">
                    <style:text-properties fo:font-style="italic"/>
                </style:style>
            </office:automatic-styles>
        </office:document-content>"#;
        let mut styles = HashMap::new();
        let mut lists = HashMap::new();
        read_styles_into(styles_xml, &mut styles, &mut lists).unwrap();
        read_styles_into(auto_xml, &mut styles, &mut lists).unwrap();
        resolve_style_parents(&mut styles);
        let p1 = styles.get("P1").expect("P1 resolved");
        assert_eq!(p1.align, Some(RichAlignment::Center));
        assert_eq!(p1.line_percent, Some(150));
        assert!(p1.fmt.bold);
        assert!(p1.fmt.italic);
        assert_eq!(p1.fmt.color.as_deref(), Some("112233"));
    }
}
