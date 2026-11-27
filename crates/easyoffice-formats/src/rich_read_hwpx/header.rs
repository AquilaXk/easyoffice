//! HWPX `Contents/header.xml` parser (`hh:font`, `hh:charPr`, `hh:paraPr`, `hh:style`,
//! `hh:numberings`, `hh:borderFills`).

use std::borrow::Cow;
use std::collections::HashMap;

use super::table_style::BorderFillReader;
use crate::hwpx_parts::{DEFAULT_FONT_FACE, HWPUNIT_PER_TWIP};
use crate::rich_char_spacing::{from_permille, MAX_SPACING_PERCENT};
use crate::rich_document::normalize_hex_color;
use crate::rich_list_writers::HwpxNumberingReader;
use crate::rich_table_style::{CellFill, TableCellBorders};
use crate::rich_xml::CharFmt;
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{EasyOfficeError, RichLineSpacing, RichScript};

#[derive(Default, Clone)]
pub(super) struct ParaShape {
    pub(super) align: Option<String>,
    pub(super) heading_type: String,
    pub(super) heading_id_ref: u32,
    pub(super) heading_level: u8,
    pub(super) checked: bool,
    pub(super) left: u32,
    pub(super) right: u32,
    /// First-line indent (`hc:intent`), HWPUNIT.
    pub(super) intent: i32,
    /// `hh:lineSpacing` with its type.
    pub(super) line_spacing: Option<RichLineSpacing>,
}

#[derive(Default, Clone)]
pub(super) struct StyleInfo {
    pub(super) name: String,
    pub(super) eng_name: String,
    pub(super) para_pr: String,
    pub(super) char_pr: String,
}

#[derive(Default)]
pub(super) struct Header {
    /// Hangul-slot font faces by id (the writer repeats the same face in every language slot).
    pub(super) fonts: HashMap<String, String>,
    pub(super) chars: HashMap<String, CharFmt>,
    pub(super) paras: HashMap<String, ParaShape>,
    pub(super) styles: HashMap<String, StyleInfo>,
    pub(super) border_fills: HashMap<String, (CellFill, TableCellBorders)>,
    pub(super) numberings: HwpxNumberingReader,
}

/// `hh:lineSpacing`: `PERCENT` is a multiple, the other types count HWPUNIT (`FIXED` exact,
/// `AT_LEAST` minimum, `BETWEEN_LINES` the margin between lines).
fn parse_line_spacing(attrs: &Attrs<'_>) -> Option<RichLineSpacing> {
    let raw: i64 = attr(attrs, "value")?.trim().parse().ok()?;
    let twips = raw / i64::from(HWPUNIT_PER_TWIP);
    match attr(attrs, "type").as_deref() {
        Some("PERCENT") => RichLineSpacing::multiple(raw),
        Some("FIXED") => RichLineSpacing::from_twips(RichLineSpacing::Exact, twips),
        Some("AT_LEAST") => RichLineSpacing::from_twips(RichLineSpacing::AtLeast, twips),
        Some("BETWEEN_LINES") if raw >= 0 => RichLineSpacing::between(twips),
        _ => None,
    }
}

/// Base character format of a `hh:charPr` element (size, text and shade colours).
fn char_fmt_from_attrs(attrs: &Attrs<'_>) -> CharFmt {
    CharFmt {
        size_half_pt: attr(attrs, "height")
            .as_deref()
            .and_then(|v| v.parse::<u32>().ok())
            .map(|v| v / 50)
            .and_then(|v| u16::try_from(v).ok())
            .filter(|v| *v > 0),
        color: attr(attrs, "textColor")
            .as_deref()
            .and_then(normalize_hex_color)
            .filter(|c| c != "000000"),
        highlight: attr(attrs, "shadeColor")
            .as_deref()
            .and_then(normalize_hex_color),
        ..CharFmt::default()
    }
}

/// Parsing state while reading `Contents/header.xml`.
#[derive(Default)]
struct HeaderBuilder {
    header: Header,
    border_fills: BorderFillReader,
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

    fn on_start(&mut self, name: &str, attrs: &Attrs<'_>) -> Result<(), EasyOfficeError> {
        self.header.numberings.on_start(name, attrs);
        self.border_fills.on_start(name, attrs)?;
        if !self.on_char_start(name, attrs) {
            self.on_para_start(name, attrs);
        }
        Ok(())
    }

    /// Fonts and character shapes; `false` when `name` is not one of them.
    fn on_char_start(&mut self, name: &str, attrs: &Attrs<'_>) -> bool {
        match name {
            "hh:fontface" => self.hangul_slot = attr(attrs, "lang").as_deref() == Some("HANGUL"),
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
                    .as_deref()
                    .and_then(|id| self.header.fonts.get(id))
                    .filter(|face| face.as_str() != DEFAULT_FONT_FACE)
                    .cloned();
                if let Some(f) = self.char_fmt_mut() {
                    f.font = font;
                }
            }
            "hh:spacing" => {
                // Percent of the font size in the Hangul slot (the writer repeats it in every slot).
                let percent = attr(attrs, "hangul").and_then(|v| v.trim().parse::<i64>().ok());
                if let Some(f) = self.char_fmt_mut() {
                    f.letter_spacing_twips = percent.and_then(|p| {
                        from_permille(
                            p.clamp(-MAX_SPACING_PERCENT, MAX_SPACING_PERCENT) * 10,
                            f.size_half_pt,
                        )
                    });
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
    fn on_para_start(&mut self, name: &str, attrs: &Attrs<'_>) {
        match name {
            "hh:paraPr" => {
                self.intent_seen = false;
                let checked = attr(attrs, "checked").as_deref() == Some("1");
                self.cur_para = attr(attrs, "id").map(|id| {
                    (
                        id.to_string(),
                        ParaShape {
                            checked,
                            ..ParaShape::default()
                        },
                        false,
                    )
                });
            }
            "hh:align" => {
                if let Some(p) = self.para_mut() {
                    p.align = attr(attrs, "horizontal").map(Cow::into_owned);
                }
            }
            "hh:heading" => {
                if let Some(p) = self.para_mut() {
                    p.heading_type =
                        attr(attrs, "type").map_or_else(|| "NONE".to_string(), Cow::into_owned);
                    p.heading_id_ref = attr(attrs, "idRef")
                        .as_deref()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    p.heading_level = attr(attrs, "level")
                        .as_deref()
                        .and_then(|l| l.parse().ok())
                        .unwrap_or(0);
                }
            }
            "hc:left" => {
                if let Some((_, p, seen)) = self.cur_para.as_mut() {
                    if !*seen {
                        *seen = true;
                        p.left = attr(attrs, "value")
                            .as_deref()
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0);
                    }
                }
            }
            "hc:right" => {
                if let Some(p) = self.para_mut() {
                    if p.right == 0 {
                        p.right = attr(attrs, "value")
                            .as_deref()
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0);
                    }
                }
            }
            "hc:intent" if !self.intent_seen => {
                self.intent_seen = true;
                if let Some(p) = self.para_mut() {
                    p.intent = attr(attrs, "value")
                        .as_deref()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                }
            }
            "hh:lineSpacing" => {
                if let Some(p) = self.para_mut() {
                    p.line_spacing = parse_line_spacing(attrs);
                }
            }
            "hh:style" => self.register_style(attrs),
            _ => {}
        }
    }

    fn register_style(&mut self, attrs: &Attrs<'_>) {
        let Some(id) = attr(attrs, "id") else {
            return;
        };
        self.header.styles.insert(
            id.to_string(),
            StyleInfo {
                name: attr(attrs, "name").unwrap_or_default().into_owned(),
                eng_name: attr(attrs, "engName").unwrap_or_default().into_owned(),
                para_pr: attr(attrs, "paraPrIDRef").unwrap_or_default().into_owned(),
                char_pr: attr(attrs, "charPrIDRef").unwrap_or_default().into_owned(),
            },
        );
    }

    fn on_end(&mut self, name: &str) {
        self.header.numberings.on_end(name);
        self.border_fills.on_end(name);
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

pub(super) fn read_header(xml: &str) -> Result<Header, EasyOfficeError> {
    let mut builder = HeaderBuilder::default();
    for ev in XmlReader::new(xml) {
        match ev? {
            XmlEvent::Start { name, attrs } => builder.on_start(name, &attrs)?,
            XmlEvent::End(name) => builder.on_end(name),
            XmlEvent::Text(text) => builder.header.numberings.on_text(&text),
        }
    }
    builder.header.numberings.check_budget()?;
    builder.header.border_fills = builder.border_fills.entries;
    Ok(builder.header)
}
