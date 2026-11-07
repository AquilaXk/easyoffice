//! HWPX reader producing a [`RichDocument`]: character shapes (`hh:charPr`) become run formats,
//! paragraph shapes (`hh:paraPr`) alignment and list kind, styles (`hh:style`, "개요 N") heading
//! levels, and `hp:tbl` tables keep their position in the section.

use std::collections::HashMap;

use crate::hwpx_parts::{
    DEFAULT_CHAR_HEIGHT, DEFAULT_FONT_FACE, DEFAULT_LINE_PERCENT, HWPUNIT_PER_TWIP,
    LIST_LEFT_MARGIN,
};
use crate::office_codecs::validate_zip_xml_entries;
use crate::rich_document::normalize_hex_color;
use crate::rich_image::{safe_package_path, ImageBudget};
use crate::rich_xml::{
    attr, heading_level_from_name, push_run, skip_subtree, tokenize, CharFmt, XmlEvent,
};
use crate::{
    EasyOfficeError, HwpxCodec, RichAlignment, RichBlock, RichDocument, RichImage, RichListKind,
    RichParagraph, RichRun, RichScript, XmlSanitizer, ZipArchiveCodec, ZipArchiveEntry,
    ZipSecurityPolicy,
};

/// EMU per HWPUNIT (1/7200 inch).
const EMU_PER_HWPUNIT: u64 = 127;

#[derive(Default, Clone)]
struct ParaShape {
    align: Option<String>,
    heading_type: String,
    heading_level: u8,
    left: u32,
    /// First-line indent (`hc:intent`), HWPUNIT.
    intent: i32,
    /// `hh:lineSpacing` percentage when its type is PERCENT.
    line_percent: Option<u32>,
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
    /// Hangul-slot font faces by id (the writer repeats the same face in every language slot).
    fonts: HashMap<String, String>,
    chars: HashMap<String, CharFmt>,
    paras: HashMap<String, ParaShape>,
    styles: HashMap<String, StyleInfo>,
}

/// Parsing state while reading `Contents/header.xml`.
#[derive(Default)]
struct HeaderBuilder {
    header: Header,
    cur_char: Option<(String, CharFmt)>,
    cur_para: Option<(String, ParaShape, bool)>,
    intent_seen: bool,
    hangul_slot: bool,
}

impl HeaderBuilder {
    fn char_fmt_mut(&mut self) -> Option<&mut CharFmt> {
        self.cur_char.as_mut().map(|(_, f)| f)
    }

    fn para_mut(&mut self) -> Option<&mut ParaShape> {
        self.cur_para.as_mut().map(|(_, p, _)| p)
    }

    fn on_start(&mut self, name: &str, attrs: &[(String, String)]) {
        if !self.on_char_start(name, attrs) {
            self.on_para_start(name, attrs);
        }
    }

    /// Fonts and character shapes; `false` when `name` is not one of them.
    fn on_char_start(&mut self, name: &str, attrs: &[(String, String)]) -> bool {
        match name {
            "hh:fontface" => self.hangul_slot = attr(attrs, "lang") == Some("HANGUL"),
            "hh:font" if self.hangul_slot => {
                if let (Some(id), Some(face)) = (attr(attrs, "id"), attr(attrs, "face")) {
                    self.header.fonts.insert(id.to_string(), face.to_string());
                }
            }
            "hh:charPr" => {
                self.cur_char =
                    attr(attrs, "id").map(|id| (id.to_string(), char_fmt_from_attrs(attrs)))
            }
            "hh:fontRef" => {
                let font = attr(attrs, "hangul")
                    .and_then(|id| self.header.fonts.get(id))
                    .filter(|face| face.as_str() != DEFAULT_FONT_FACE)
                    .cloned();
                if let Some(f) = self.char_fmt_mut() {
                    f.font = font;
                }
            }
            "hh:bold" => {
                if let Some(f) = self.char_fmt_mut() {
                    f.bold = true;
                }
            }
            "hh:italic" => {
                if let Some(f) = self.char_fmt_mut() {
                    f.italic = true;
                }
            }
            "hh:underline" => {
                if let Some(f) = self.char_fmt_mut() {
                    f.underline = attr(attrs, "type").is_some_and(|t| t != "NONE");
                }
            }
            "hh:strikeout" => {
                if let Some(f) = self.char_fmt_mut() {
                    f.strike = attr(attrs, "shape").is_some_and(|t| t != "NONE");
                }
            }
            "hh:supscript" | "hh:subscript" => {
                let script = if name == "hh:supscript" {
                    RichScript::Superscript
                } else {
                    RichScript::Subscript
                };
                if let Some(f) = self.char_fmt_mut() {
                    f.script = script;
                }
            }
            _ => return false,
        }
        true
    }

    /// Paragraph shapes and styles.
    fn on_para_start(&mut self, name: &str, attrs: &[(String, String)]) {
        match name {
            "hh:paraPr" => {
                self.intent_seen = false;
                self.cur_para =
                    attr(attrs, "id").map(|id| (id.to_string(), ParaShape::default(), false));
            }
            "hh:align" => {
                if let Some(p) = self.para_mut() {
                    p.align = attr(attrs, "horizontal").map(str::to_string);
                }
            }
            "hh:heading" => {
                if let Some(p) = self.para_mut() {
                    p.heading_type = attr(attrs, "type").unwrap_or("NONE").to_string();
                    p.heading_level = attr(attrs, "level")
                        .and_then(|l| l.parse().ok())
                        .unwrap_or(0);
                }
            }
            "hc:left" => {
                if let Some((_, p, seen)) = self.cur_para.as_mut() {
                    if !*seen {
                        *seen = true;
                        p.left = attr(attrs, "value")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0);
                    }
                }
            }
            "hc:intent" if !self.intent_seen => {
                self.intent_seen = true;
                if let Some(p) = self.para_mut() {
                    p.intent = attr(attrs, "value")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                }
            }
            "hh:lineSpacing" => {
                if let Some(p) = self.para_mut() {
                    p.line_percent = (attr(attrs, "type") == Some("PERCENT"))
                        .then(|| attr(attrs, "value").and_then(|v| v.parse().ok()))
                        .flatten();
                }
            }
            "hh:style" => self.register_style(attrs),
            _ => {}
        }
    }

    fn register_style(&mut self, attrs: &[(String, String)]) {
        let Some(id) = attr(attrs, "id") else {
            return;
        };
        self.header.styles.insert(
            id.to_string(),
            StyleInfo {
                name: attr(attrs, "name").unwrap_or("").to_string(),
                eng_name: attr(attrs, "engName").unwrap_or("").to_string(),
                para_pr: attr(attrs, "paraPrIDRef").unwrap_or("").to_string(),
                char_pr: attr(attrs, "charPrIDRef").unwrap_or("").to_string(),
            },
        );
    }

    fn on_end(&mut self, name: &str) {
        match name {
            "hh:charPr" => {
                if let Some((id, f)) = self.cur_char.take() {
                    self.header.chars.insert(id, f);
                }
            }
            "hh:paraPr" => {
                if let Some((id, p, _)) = self.cur_para.take() {
                    self.header.paras.insert(id, p);
                }
            }
            _ => {}
        }
    }
}

/// Base character format of a `hh:charPr` element (size, text and shade colours).
fn char_fmt_from_attrs(attrs: &[(String, String)]) -> CharFmt {
    CharFmt {
        size_half_pt: attr(attrs, "height")
            .and_then(|v| v.parse::<u32>().ok())
            .map(|v| v / 50)
            .and_then(|v| u16::try_from(v).ok())
            .filter(|v| *v > 0),
        color: attr(attrs, "textColor")
            .and_then(normalize_hex_color)
            .filter(|c| c != "000000"),
        highlight: attr(attrs, "shadeColor").and_then(normalize_hex_color),
        ..CharFmt::default()
    }
}

fn read_header(xml: &str) -> Result<Header, EasyOfficeError> {
    let mut builder = HeaderBuilder::default();
    for ev in tokenize(xml)? {
        match ev {
            XmlEvent::Start { name, attrs } => builder.on_start(&name, &attrs),
            XmlEvent::End(name) => builder.on_end(&name),
            XmlEvent::Text(_) => {}
        }
    }
    Ok(builder.header)
}

#[derive(Default)]
struct ParaState {
    runs: Vec<RichRun>,
    para_pr: String,
    style: String,
    images: Vec<RichImage>,
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
    let marker_margin = shape.left == LIST_LEFT_MARGIN;
    let list = if heading > 0 {
        None
    } else {
        match shape.heading_type.as_str() {
            "BULLET" => Some(RichListKind::Bullet),
            "NUMBER" => Some(RichListKind::Ordered),
            _ if marker_margin => Some(RichListKind::Bullet),
            _ => None,
        }
    };
    let to_twips = |hwpunit: i64| i32::try_from(hwpunit / i64::from(HWPUNIT_PER_TWIP)).ok();
    let left_indent_twips = (!marker_margin && shape.left > 0)
        .then(|| to_twips(i64::from(shape.left)))
        .flatten()
        .and_then(|t| u32::try_from(t).ok());
    let first_line_indent_twips = (shape.intent != 0)
        .then(|| to_twips(i64::from(shape.intent)))
        .flatten();
    let line_spacing_percent = shape
        .line_percent
        .filter(|p| *p != DEFAULT_LINE_PERCENT && (1..=1_000).contains(p))
        .and_then(|p| u16::try_from(p).ok());
    let alignment = match shape.align.as_deref() {
        Some("LEFT") => Some(RichAlignment::Left),
        Some("CENTER") => Some(RichAlignment::Center),
        Some("RIGHT") => Some(RichAlignment::Right),
        Some("DISTRIBUTE" | "DISTRIBUTE_SPACE") => Some(RichAlignment::Justify),
        _ => None,
    };
    // Heading styles are bold and sized by definition, so those are not user formatting there;
    // body text at the writer's default height inherits the default size.
    let style_fmt = style.and_then(|s| header.chars.get(&s.char_pr));
    let style_bold = style_fmt.is_some_and(|f| f.bold);
    let inherited_size = if heading > 0 {
        style_fmt.and_then(|f| f.size_half_pt)
    } else {
        u16::try_from(DEFAULT_CHAR_HEIGHT / 50).ok()
    };
    let mut runs: Vec<RichRun> = Vec::new();
    for r in p.runs {
        let fmt = CharFmt {
            bold: r.bold && !(heading > 0 && style_bold),
            italic: r.italic,
            underline: r.underline,
            strike: r.strike,
            size_half_pt: r.size_half_pt.filter(|s| Some(*s) != inherited_size),
            font: r.font,
            color: r.color,
            highlight: r.highlight,
            script: r.script,
        };
        push_run(&mut runs, &r.text, &fmt);
    }
    RichParagraph {
        runs,
        alignment,
        heading_level: heading,
        list,
        left_indent_twips,
        first_line_indent_twips,
        line_spacing_percent,
    }
}

const SKIPPED_SUBTREES: [&str; 13] = [
    "hp:secPr",
    "hp:ctrl",
    "hp:equation",
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

const SHAPE_SKIP_NOTE: &str = "그림·도형·수식·머리말은 가져오지 않았습니다.";

/// Walker state for one `Contents/section*.xml`.
#[derive(Default)]
struct SectionWalker {
    paras: Vec<ParaState>,
    runs: Vec<RunState>,
    t_depth: usize,
    tbl_depth: usize,
    table_rows: Vec<Vec<String>>,
    cur_row: Vec<String>,
    cell_paras: Vec<String>,
    budget: ImageBudget,
}

/// Binary item id -> package entry, from the `opf:item` manifest of `Contents/content.hpf`.
struct HwpxMedia<'a> {
    by_id: HashMap<String, &'a ZipArchiveEntry>,
}

impl<'a> HwpxMedia<'a> {
    fn load(entries: &'a [ZipArchiveEntry]) -> Result<Self, EasyOfficeError> {
        let mut by_id = HashMap::new();
        let Some(hpf) = entries.iter().find(|e| e.path == "Contents/content.hpf") else {
            return Ok(Self { by_id });
        };
        let xml = std::str::from_utf8(&hpf.data).map_err(|_| {
            EasyOfficeError::InvalidUtf8("Contents/content.hpf UTF-8 오류".to_string())
        })?;
        for event in tokenize(xml)? {
            let XmlEvent::Start { name, attrs } = event else {
                continue;
            };
            let (Some(id), Some(href)) = (attr(&attrs, "id"), attr(&attrs, "href")) else {
                continue;
            };
            let entry =
                safe_package_path(href).and_then(|path| entries.iter().find(|e| e.path == path));
            if let (true, Some(entry)) = (name == "opf:item", entry) {
                by_id.insert(id.to_string(), entry);
            }
        }
        Ok(Self { by_id })
    }
}

/// Fields of one `hp:pic`.
#[derive(Default)]
struct PictureFields<'e> {
    item: Option<&'e str>,
    size: Option<(u64, u64)>,
    alt: String,
}

fn scan_picture(events: &[XmlEvent]) -> PictureFields<'_> {
    let mut fields = PictureFields::default();
    let mut in_comment = false;
    for event in events {
        match event {
            XmlEvent::Start { name, attrs } => match name.as_str() {
                "hc:img" if fields.item.is_none() => fields.item = attr(attrs, "binaryItemIDRef"),
                "hp:curSz" | "hp:sz" if fields.size.is_none() => {
                    let side = |key| {
                        attr(attrs, key)
                            .and_then(|v| v.trim().parse::<u64>().ok())
                            .filter(|v| *v > 0)
                    };
                    fields.size = side("width").zip(side("height")).and_then(|(w, h)| {
                        Some((
                            w.checked_mul(EMU_PER_HWPUNIT)?,
                            h.checked_mul(EMU_PER_HWPUNIT)?,
                        ))
                    });
                }
                "hp:shapeComment" => in_comment = true,
                _ => {}
            },
            XmlEvent::Text(text) if in_comment => fields.alt.push_str(text),
            XmlEvent::End(name) if name == "hp:shapeComment" => in_comment = false,
            _ => {}
        }
    }
    fields
}

impl SectionWalker {
    /// Reads an `hp:pic` (events span its start to its end) into the open paragraph.
    fn on_picture(
        &mut self,
        events: &[XmlEvent],
        media: &HwpxMedia<'_>,
        warnings: &mut Vec<String>,
    ) -> Result<(), EasyOfficeError> {
        let fields = scan_picture(events);
        let entry = fields.item.and_then(|id| media.by_id.get(id));
        let mut warn = |note: String| {
            if !warnings.contains(&note) {
                warnings.push(note);
            }
        };
        let Some(entry) = entry else {
            warn("찾을 수 없거나 외부에 있는 그림은 가져오지 않았습니다.".to_string());
            return Ok(());
        };
        if self.tbl_depth > 0 || self.paras.is_empty() {
            warn("표 안의 그림은 가져오지 않았습니다.".to_string());
            return Ok(());
        }
        let mut image = match RichImage::new(entry.data.clone(), fields.alt.as_str()) {
            Ok(image) => image,
            Err(err) => {
                warn(format!(
                    "그림 '{}'을 가져오지 않았습니다: {err}",
                    entry.path
                ));
                return Ok(());
            }
        };
        self.budget.charge(entry.data.len())?;
        if let Some((w, h)) = fields.size {
            // An out-of-range size keeps the default derived from the pixels.
            let _ = image.set_display_emu(w, h);
        }
        if let Some(top) = self.paras.last_mut() {
            top.images.push(image);
        }
        Ok(())
    }
}

impl SectionWalker {
    fn on_start(&mut self, name: &str, attrs: &[(String, String)]) {
        match name {
            "hp:p" => self.paras.push(ParaState {
                runs: Vec::new(),
                para_pr: attr(attrs, "paraPrIDRef").unwrap_or("").to_string(),
                style: attr(attrs, "styleIDRef").unwrap_or("").to_string(),
                images: Vec::new(),
            }),
            "hp:run" => self.runs.push(RunState {
                char_pr: attr(attrs, "charPrIDRef").unwrap_or("").to_string(),
                text: String::new(),
            }),
            "hp:t" => self.t_depth += 1,
            "hp:tab" if self.t_depth > 0 => push_text(&mut self.runs, "\t"),
            "hp:lineBreak" if self.t_depth > 0 => push_text(&mut self.runs, "\n"),
            "hp:nbSpace" | "hp:fwSpace" if self.t_depth > 0 => push_text(&mut self.runs, " "),
            "hp:hyphen" if self.t_depth > 0 => push_text(&mut self.runs, "-"),
            "hp:tbl" => {
                self.tbl_depth += 1;
                if self.tbl_depth == 1 {
                    self.table_rows.clear();
                }
            }
            "hp:tr" if self.tbl_depth == 1 => self.cur_row = Vec::new(),
            "hp:tc" if self.tbl_depth == 1 => self.cell_paras.clear(),
            _ => {}
        }
    }

    fn on_text(&mut self, text: &str) {
        if self.t_depth > 0 {
            push_text(&mut self.runs, text);
        }
    }

    fn on_end(&mut self, name: &str, header: &Header, blocks: &mut Vec<RichBlock>) {
        match name {
            "hp:t" => self.t_depth = self.t_depth.saturating_sub(1),
            "hp:run" => {
                let Some(run) = self.runs.pop() else {
                    return;
                };
                let fmt = header.chars.get(&run.char_pr).cloned().unwrap_or_default();
                if let Some(top) = self.paras.last_mut() {
                    push_run(&mut top.runs, &run.text, &fmt);
                }
            }
            "hp:p" => self.finish_paragraph(header, blocks),
            "hp:tc" if self.tbl_depth == 1 => {
                while self.cell_paras.last().is_some_and(String::is_empty) {
                    self.cell_paras.pop();
                }
                self.cur_row.push(self.cell_paras.join("\n"));
                self.cell_paras.clear();
            }
            "hp:tr" if self.tbl_depth == 1 => {
                self.table_rows.push(std::mem::take(&mut self.cur_row));
            }
            "hp:tbl" => {
                self.tbl_depth = self.tbl_depth.saturating_sub(1);
                if self.tbl_depth == 0 && !self.table_rows.is_empty() {
                    blocks.push(RichBlock::Table(std::mem::take(&mut self.table_rows)));
                }
            }
            _ => {}
        }
    }

    fn finish_paragraph(&mut self, header: &Header, blocks: &mut Vec<RichBlock>) {
        let Some(mut p) = self.paras.pop() else {
            return;
        };
        if self.tbl_depth > 0 {
            self.cell_paras
                .push(p.runs.iter().map(|r| r.text.as_str()).collect());
            return;
        }
        let images = std::mem::take(&mut p.images);
        if p.runs.iter().any(|r| !r.text.trim().is_empty()) {
            blocks.push(RichBlock::Paragraph(finish_paragraph(header, p)));
        }
        blocks.extend(images.into_iter().map(RichBlock::Image));
    }
}

fn read_section(
    xml: &str,
    header: &Header,
    media: &HwpxMedia<'_>,
    walker: &mut SectionWalker,
    blocks: &mut Vec<RichBlock>,
    warnings: &mut Vec<String>,
) -> Result<(), EasyOfficeError> {
    let events = tokenize(xml)?;
    let mut i = 0usize;
    while i < events.len() {
        match &events[i] {
            XmlEvent::Start { name, .. } if name == "hp:pic" => {
                let end = skip_subtree(&events, i);
                walker.on_picture(&events[i..end], media, warnings)?;
                i = end;
                continue;
            }
            XmlEvent::Start { name, attrs } => {
                if SKIPPED_SUBTREES.contains(&name.as_str()) {
                    if !matches!(name.as_str(), "hp:secPr" | "hp:ctrl") {
                        let note = SHAPE_SKIP_NOTE.to_string();
                        if !warnings.contains(&note) {
                            warnings.push(note);
                        }
                    }
                    i = skip_subtree(&events, i);
                    continue;
                }
                walker.on_start(name, attrs);
            }
            XmlEvent::Text(text) => walker.on_text(text),
            XmlEvent::End(name) => walker.on_end(name, header, blocks),
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
    let media = HwpxMedia::load(&entries)?;
    let mut walker = SectionWalker::default();
    for (_, entry) in sections {
        let xml = std::str::from_utf8(&entry.data).map_err(|_| {
            EasyOfficeError::InvalidUtf8("Contents/section.xml UTF-8 오류".to_string())
        })?;
        XmlSanitizer::validate_safe_xml(xml)?;
        read_section(
            xml,
            &header,
            &media,
            &mut walker,
            &mut blocks,
            &mut warnings,
        )?;
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
