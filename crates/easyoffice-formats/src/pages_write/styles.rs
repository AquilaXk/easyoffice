//! The style objects of the document: the stylesheet and the styles it lists, the list and column
//! styles, the theme with its drawing presets, and the variations a document's own formatting
//! needs (a paragraph with an alignment, a run with a look).
//!
//! The graph follows the least document that the independent implementation named below reports
//! the app opening. What is chosen rather than required (fonts, sizes, the colour palette, the
//! margins) is this project's own: the values are not copied from any app's templates.
//!
//! 참조 구현: iwork-rs (MIT, Copyright (c) 2026 Zebra Pig), `src/create.rs` 의 `pages`(592-790행),
//! `stylesheet_archive`(841-883행), `paragraph_style`·`list_style`·`column_style`(919-1056행),
//! `theme_presets`·`numbers_theme_with`(2263-2400행)와 `src/style.rs` 의 `register_variation`
//! (1205-1225행), `src/document.rs` 의 `format_text`(8950-9090행)에서 객체 구성·필드 번호·
//! 변형 스타일 규칙(이름·식별자 없음, 부모와 변형 표시, `override_count`)을 따름. 한 메시지 안의
//! 반복 필드는 필드 번호 오름차순으로 모아 쓰도록 순서를 바꿈.

use super::package::{Blueprint, ComponentId, ROOT};
use super::wire::{bytes, float, msg, reference, string, varint, Fld};
use crate::{
    EasyOfficeError, RichColumns, RichLineSpacing, RichListNumber, RichNumberEnclosure,
    RichNumberFormat, RichScript,
};
use std::collections::BTreeMap;

use crate::pages_schema::{compact_para, para_props, table, ty};

const TYPE_STYLESHEET: u32 = 401;
const TYPE_CHARACTER_STYLE: u32 = 2021;
const TYPE_PARAGRAPH_STYLE: u32 = 2022;
const TYPE_LIST_STYLE: u32 = 2023;
const TYPE_COLUMN_STYLE: u32 = 2024;
const TYPE_SHAPE_STYLE: u32 = 2025;
const TYPE_MEDIA_STYLE: u32 = 3016;
const TYPE_CELL_STYLE: u32 = ty::CELL_STYLE;

/// Body text: 11 pt, black (`AppleSDGothicNeo-Regular`). Headings 1-6 use `AppleSDGothicNeo-Bold`.
pub(super) const BODY_POINTS: f32 = 11.0;
const HEADING_POINTS: [f32; 6] = [18.0, 16.0, 14.0, 13.0, 12.0, 11.0];
const BODY_FONT: &str = "AppleSDGothicNeo-Regular";
const HEADING_FONT: &str = "AppleSDGothicNeo-Bold";
const DEFAULT_TAB_STOPS_PT: f32 = 36.0;

/// Paragraph alignment values (`ParagraphStyle` bag field 1): 4 is "natural".
const ALIGN_NATURAL: u64 = 4;

/// `(kind, count, theme field, is a media style)`: six line styles, six shape styles, one text-box
/// style, six picture styles, six movie styles and one drawing line style.
const DRAWING_PRESETS: [(&str, usize, u32, bool); 6] = [
    ("line-style", 6, 4, false),
    ("shape-style", 6, 5, false),
    ("textbox-style", 1, 6, false),
    ("image-style", 6, 7, true),
    ("movie-style", 6, 8, true),
    ("drawing-line-style", 1, 9, false),
];

const BODY_IDENTIFIER: &str = "text-0-paragraphstyle-Body";
const DEFAULT_PARAGRAPH_IDENTIFIER: &str = "paragraph-style-default";
const LIST_IDENTIFIER: &str = "text-0-liststyle-None";
const NULL_CHARACTER_IDENTIFIER: &str = "character-style-null";
const TEXT_BOX_IDENTIFIER: &str = "textbox-style-preset-0";
const IMAGE_IDENTIFIER: &str = "image-style-preset-0";
const TABLE_CELL_IDENTIFIER: &str = "tableCell-0-bodyStyle";

/// `TSP.Color` in sRGB: model 1, three channels, alpha, the colour space (field 12) and the
/// field that follows it. A colour is complete or it is nothing.
fn color(red: f32, green: f32, blue: f32) -> Vec<Fld> {
    vec![
        varint(1, 1),
        float(3, red),
        float(4, green),
        float(5, blue),
        float(6, 1.0),
        varint(12, 1),
        float(13, 1.0),
    ]
}

pub(super) fn rgb(bytes: [u8; 3]) -> Vec<Fld> {
    color(
        f32::from(bytes[0]) / 255.0,
        f32::from(bytes[1]) / 255.0,
        f32::from(bytes[2]) / 255.0,
    )
}

/// The look a run asks for; only set properties are written.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct CharLook {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub size_half_pt: Option<u16>,
    pub font: Option<String>,
    pub color: Option<[u8; 3]>,
    pub highlight: Option<[u8; 3]>,
    pub script: u8,
}

impl CharLook {
    pub(super) fn script_code(script: RichScript) -> u8 {
        match script {
            RichScript::Normal => 0,
            RichScript::Superscript => 1,
            RichScript::Subscript => 2,
        }
    }

    pub(super) fn is_plain(&self) -> bool {
        !self.bold
            && !self.italic
            && !self.underline
            && !self.strike
            && self.size_half_pt.is_none()
            && self.font.is_none()
            && self.color.is_none()
            && self.highlight.is_none()
            && self.script == 0
    }
}

/// Paragraph-level overrides relative to a base paragraph style (`Body` or `Heading 1..6`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub(super) struct ParaLook {
    pub alignment: Option<u64>,
    pub first_line_indent_twips: Option<i32>,
    pub left_indent_twips: Option<u32>,
    pub right_indent_twips: Option<u32>,
    pub line_spacing: Option<RichLineSpacing>,
}

impl ParaLook {
    pub(super) fn is_plain(&self) -> bool {
        self.alignment.is_none()
            && self.first_line_indent_twips.is_none()
            && self.left_indent_twips.is_none()
            && self.right_indent_twips.is_none()
            && self.line_spacing.is_none()
    }
}

/// List style variation key (`Bullet` or ordered `LIST_LEVEL_NUMBER_FORMATS` code).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ListLook {
    Bullet,
    Ordered(u64),
}

impl ListLook {
    pub(super) fn ordered_code(num: RichListNumber) -> u64 {
        let group = match num.enclosure {
            RichNumberEnclosure::Dot => 0,
            RichNumberEnclosure::Parentheses => 1,
            RichNumberEnclosure::RightParenthesis => 2,
        };
        match num.format {
            RichNumberFormat::Decimal => group,
            RichNumberFormat::UpperRoman => 3 + group,
            RichNumberFormat::LowerRoman => 6 + group,
            RichNumberFormat::LowerLatin => 9 + group,
            RichNumberFormat::UpperLatin => 12 + group,
            RichNumberFormat::HangulSyllable => 45,
        }
    }
}

pub(super) struct Styles {
    pub stylesheet: u64,
    pub body: u64,
    pub headings: [u64; 6],
    pub list: u64,
    pub body_column: u64,
    pub image: u64,
    pub table_cell: u64,
    component: ComponentId,
    null_character: u64,
    column: u64,
    /// `(internal identifier, style)` of the styles the stylesheet keys by name.
    named: Vec<(String, u64)>,
    /// Variations by parent, in creation order: `(parent, children)`.
    families: Vec<(u64, Vec<u64>)>,
    paragraph_variations: BTreeMap<(u64, ParaLook), u64>,
    character_variations: BTreeMap<CharLook, u64>,
    list_variations: BTreeMap<ListLook, u64>,
    column_variations: BTreeMap<(u8, bool, u32), u64>,
    cell_variations: BTreeMap<[u8; 3], u64>,
    /// `(theme field, style)` of every drawing preset.
    presets: Vec<(u32, u64)>,
}

/// The line spacing message of a paragraph property bag (`mode` 1, `amount` 2): the multiple as a
/// fraction of the single line, the lengths in points.
fn line_spacing_message(spacing: RichLineSpacing) -> Fld {
    let (mode, amount) = match spacing {
        RichLineSpacing::Multiple(pct) => (compact_para::SPACING_MULTIPLE, f32::from(pct) / 100.0),
        RichLineSpacing::AtLeast(twips) => (compact_para::SPACING_MINIMUM, f32::from(twips) / 20.0),
        RichLineSpacing::Exact(twips) => (compact_para::SPACING_EXACT, f32::from(twips) / 20.0),
        RichLineSpacing::Between(twips) => (compact_para::SPACING_BETWEEN, f32::from(twips) / 20.0),
    };
    msg(
        13,
        vec![
            varint(compact_para::SPACING_MODE, mode),
            float(compact_para::SPACING_AMOUNT, amount),
        ],
    )
}

fn header(name: Option<(&str, &str)>, stylesheet: u64) -> Fld {
    let mut fields = Vec::new();
    if let Some((display, identifier)) = name {
        fields.push(string(1, display));
        fields.push(string(2, identifier));
    }
    fields.push(reference(5, stylesheet));
    msg(1, fields)
}

fn variation_header(parent: u64, stylesheet: u64) -> Fld {
    msg(
        1,
        vec![reference(3, parent), varint(4, 1), reference(5, stylesheet)],
    )
}

fn list_style(stylesheet: u64) -> Vec<Fld> {
    let mut fields = vec![
        header(Some(("None", LIST_IDENTIFIER)), stylesheet),
        varint(10, 4),
    ];
    fields.extend((0..9).map(|_| varint(11, 0)));
    fields.extend((0..9).map(|_| float(12, 0.0)));
    fields.extend((0..9).map(|level| float(13, 36.0 * level as f32)));
    fields
}

fn column_style(stylesheet: u64) -> Vec<Fld> {
    vec![
        header(Some(("None", "column-style-default")), stylesheet),
        varint(10, 9),
        msg(
            11,
            vec![
                varint(1, 1),
                varint(2, 0),
                varint(3, 0),
                float(4, 0.0),
                varint(5, 0),
                msg(7, vec![reference(1, ROOT)]),
            ],
        ),
    ]
}

fn paragraph_style(
    name: &str,
    identifier: &str,
    size: f32,
    bold: bool,
    stylesheet: u64,
    list: u64,
) -> Vec<Fld> {
    let ink = || color(0.0, 0.0, 0.0);
    vec![
        header(Some((name, identifier)), stylesheet),
        // `override_count`: six character properties and three paragraph properties below. A style
        // whose bags hold properties while this says nothing is treated as setting none.
        varint(10, 9),
        msg(
            11,
            vec![
                varint(1, u64::from(bold)),
                varint(2, 0),
                float(3, size),
                string(5, if bold { HEADING_FONT } else { BODY_FONT }),
                msg(7, ink()),
                // The fill drawn inside the glyphs is the colour the app paints with.
                msg(46, vec![msg(1, ink())]),
            ],
        ),
        msg(
            12,
            vec![
                varint(1, ALIGN_NATURAL),
                float(4, DEFAULT_TAB_STOPS_PT),
                reference(40, list),
            ],
        ),
    ]
}

/// A drawing preset style (`kind-preset-n`): a shape-like style holds its property bag one level
/// down, a media style has no fill so its bag starts one field lower.
fn shape_preset(identifier: &str, stylesheet: u64, text_box: bool) -> Vec<Fld> {
    let mut bag = vec![bytes(1, Vec::new())];
    if text_box {
        bag.push(no_stroke());
    }
    vec![msg(
        1,
        vec![
            msg(1, vec![string(2, identifier), reference(5, stylesheet)]),
            varint(10, if text_box { 2 } else { 1 }),
            msg(11, bag),
        ],
    )]
}

fn media_preset(identifier: &str, stylesheet: u64) -> Vec<Fld> {
    vec![
        msg(1, vec![string(2, identifier), reference(5, stylesheet)]),
        msg(11, vec![bytes(1, Vec::new())]),
    ]
}

/// `TSD.StrokeArchive` saying "no outline": black, 1 pt, the empty pattern (type 2).
fn no_stroke() -> Fld {
    msg(
        2,
        vec![
            msg(
                1,
                vec![
                    varint(1, 1),
                    float(3, 0.0),
                    float(4, 0.0),
                    float(5, 0.0),
                    float(6, 1.0),
                ],
            ),
            float(2, 1.0),
            varint(3, 0),
            varint(4, 0),
            float(5, 4.0),
            msg(6, vec![varint(1, 2), float(2, 0.0), varint(3, 0)]),
        ],
    )
}

/// Twenty-seven theme colours: neutrals, then two rows of hues.
const PALETTE: [u32; 27] = [
    0xFFFFFF, 0xE6E6E6, 0xBFBFBF, 0x8C8C8C, 0x595959, 0x000000, 0xB3E0FF, 0x66B8FF, 0x1F7FD1,
    0x0F4C8A, 0xE86A5C, 0xF08C3A, 0xF2C94C, 0x6FB05C, 0x4A74C9, 0x8A58A8, 0xC2527A, 0x666666,
    0xA6A6A6, 0xF2F2F2, 0xFAD9D9, 0xD9FAD9, 0xD9D9FA, 0xFAFAD9, 0xD9FAFA, 0xFAD9FA, 0x7F7F7F,
];

impl Styles {
    /// Creates the named styles and the drawing presets in a new `DocumentStylesheet` component.
    pub(super) fn define(bp: &mut Blueprint) -> Result<Self, EasyOfficeError> {
        let (component, stylesheet) = bp.component("DocumentStylesheet");
        let list = bp.add(component, TYPE_LIST_STYLE, &list_style(stylesheet))?;
        let mut named = vec![(LIST_IDENTIFIER.to_string(), list)];

        let body_fields = paragraph_style(
            "Body",
            BODY_IDENTIFIER,
            BODY_POINTS,
            false,
            stylesheet,
            list,
        );
        let body = bp.add(component, TYPE_PARAGRAPH_STYLE, &body_fields)?;
        named.push((BODY_IDENTIFIER.to_string(), body));
        let mut headings = [0u64; 6];
        for (index, slot) in headings.iter_mut().enumerate() {
            let name = format!("Heading {}", index + 1);
            let identifier = format!("text-0-paragraphstyle-{name}");
            let fields = paragraph_style(
                &name,
                &identifier,
                HEADING_POINTS[index],
                true,
                stylesheet,
                list,
            );
            *slot = bp.add(component, TYPE_PARAGRAPH_STYLE, &fields)?;
            named.push((identifier, *slot));
        }

        let column = bp.add(component, TYPE_COLUMN_STYLE, &column_style(stylesheet))?;
        let body_column = bp.add(
            component,
            TYPE_COLUMN_STYLE,
            &[variation_header(column, stylesheet), varint(10, 1)],
        )?;
        let null_character = bp.add(
            component,
            TYPE_CHARACTER_STYLE,
            &[header(
                Some(("None", NULL_CHARACTER_IDENTIFIER)),
                stylesheet,
            )],
        )?;
        named.push((NULL_CHARACTER_IDENTIFIER.to_string(), null_character));

        // The theme's drawing presets: without them the app looks for a preset at load and
        // refuses the document. Nothing in the body uses them.
        let mut presets = Vec::new();
        for (kind, count, field, media) in DRAWING_PRESETS {
            for index in 0..count {
                let identifier = format!("{kind}-preset-{index}");
                let fields = if media {
                    media_preset(&identifier, stylesheet)
                } else {
                    shape_preset(&identifier, stylesheet, kind == "textbox-style")
                };
                let type_id = if media {
                    TYPE_MEDIA_STYLE
                } else {
                    TYPE_SHAPE_STYLE
                };
                let id = bp.add(component, type_id, &fields)?;
                named.push((identifier, id));
                presets.push((field, id));
            }
        }

        // The styles a text box and a picture are drawn with, listed under their own names.
        let text_box = bp.add(
            component,
            TYPE_SHAPE_STYLE,
            &shape_preset(TEXT_BOX_IDENTIFIER, stylesheet, true),
        )?;
        let image = bp.add(
            component,
            TYPE_MEDIA_STYLE,
            &media_preset(IMAGE_IDENTIFIER, stylesheet),
        )?;
        named.push((TEXT_BOX_IDENTIFIER.to_string(), text_box));
        named.push((IMAGE_IDENTIFIER.to_string(), image));
        named.push((DEFAULT_PARAGRAPH_IDENTIFIER.to_string(), body));
        let table_cell = bp.add(
            component,
            TYPE_CELL_STYLE,
            &[
                header(Some(("Table Cell", TABLE_CELL_IDENTIFIER)), stylesheet),
                varint(10, 0),
            ],
        )?;
        named.push((TABLE_CELL_IDENTIFIER.to_string(), table_cell));

        Ok(Self {
            stylesheet,
            body,
            headings,
            list,
            body_column,
            image,
            table_cell,
            component,
            null_character,
            column,
            named,
            families: vec![(column, vec![body_column])],
            paragraph_variations: BTreeMap::new(),
            character_variations: BTreeMap::new(),
            list_variations: BTreeMap::new(),
            column_variations: BTreeMap::new(),
            cell_variations: BTreeMap::new(),
            presets,
        })
    }

    fn register(&mut self, parent: u64, child: u64) {
        match self.families.iter_mut().find(|(p, _)| *p == parent) {
            Some((_, children)) => children.push(child),
            None => self.families.push((parent, vec![child])),
        }
    }

    /// The anonymous variation of `parent` setting alignment, indents and/or line spacing.
    pub(super) fn paragraph_variation(
        &mut self,
        bp: &mut Blueprint,
        parent: u64,
        look: ParaLook,
    ) -> Result<u64, EasyOfficeError> {
        if look.is_plain() {
            return Ok(parent);
        }
        if let Some(id) = self.paragraph_variations.get(&(parent, look)) {
            return Ok(*id);
        }
        let mut bag = Vec::new();
        let mut count = 0u64;
        if let Some(align) = look.alignment {
            bag.push(varint(1, align));
            count += 1;
        }
        if let Some(first_tw) = look.first_line_indent_twips {
            bag.push(float(7, first_tw as f32 / 20.0));
            count += 1;
        }
        if let Some(left_tw) = look.left_indent_twips {
            bag.push(float(11, left_tw as f32 / 20.0));
            count += 1;
        }
        if let Some(spacing) = look.line_spacing {
            bag.push(line_spacing_message(spacing));
            count += 1;
        }
        if let Some(right_tw) = look.right_indent_twips {
            bag.push(float(para_props::RIGHT_INDENT, right_tw as f32 / 20.0));
            count += 1;
        }
        let fields = [
            variation_header(parent, self.stylesheet),
            varint(10, count),
            msg(12, bag),
        ];
        let id = bp.add(self.component, TYPE_PARAGRAPH_STYLE, &fields)?;
        self.paragraph_variations.insert((parent, look), id);
        self.register(parent, id);
        Ok(id)
    }

    /// The anonymous variation of the null character style that sets `look`; one per distinct look.
    pub(super) fn character(
        &mut self,
        bp: &mut Blueprint,
        look: CharLook,
    ) -> Result<u64, EasyOfficeError> {
        if let Some(id) = self.character_variations.get(&look) {
            return Ok(*id);
        }
        let mut bag = Vec::new();
        let mut count = 0u64;
        for (number, on) in [(1, look.bold), (2, look.italic)] {
            if on {
                bag.push(varint(number, 1));
                count += 1;
            }
        }
        if let Some(half_points) = look.size_half_pt {
            bag.push(float(3, f32::from(half_points) / 2.0));
            count += 1;
        }
        if let Some(ref font_name) = look.font {
            bag.push(string(5, font_name));
            count += 1;
        }
        if let Some(rgb_bytes) = look.color {
            bag.push(msg(7, rgb(rgb_bytes)));
            count += 1;
        }
        if look.script != 0 {
            bag.push(varint(10, u64::from(look.script)));
            count += 1;
        }
        if look.underline {
            bag.push(varint(11, 1));
            count += 1;
        }
        if look.strike {
            bag.push(varint(12, 1));
            count += 1;
        }
        if let Some(hl_bytes) = look.highlight {
            bag.push(msg(26, rgb(hl_bytes)));
            count += 1;
        }
        if let Some(rgb_bytes) = look.color {
            bag.push(msg(46, vec![msg(1, rgb(rgb_bytes))]));
        }
        let fields = [
            variation_header(self.null_character, self.stylesheet),
            varint(10, count),
            msg(11, bag),
        ];
        let id = bp.add(self.component, TYPE_CHARACTER_STYLE, &fields)?;
        self.character_variations.insert(look, id);
        self.register(self.null_character, id);
        Ok(id)
    }

    /// The anonymous variation of `self.list` for bullet or numbered lists.
    pub(super) fn list_variation(
        &mut self,
        bp: &mut Blueprint,
        look: ListLook,
    ) -> Result<u64, EasyOfficeError> {
        if let Some(id) = self.list_variations.get(&look) {
            return Ok(*id);
        }
        let (kind_val, override_count, number_code) = match look {
            ListLook::Bullet => (2u64, 4u64, None),
            ListLook::Ordered(code) => (3u64, 5u64, Some(code)),
        };
        let mut fields = vec![
            variation_header(self.list, self.stylesheet),
            varint(10, override_count),
        ];
        fields.extend((0..9).map(|_| varint(11, kind_val)));
        fields.extend((0..9).map(|_| float(12, 0.0)));
        fields.extend((0..9).map(|level| float(13, 36.0 * level as f32)));
        if let Some(code) = number_code {
            fields.extend((0..9).map(|_| varint(15, code)));
        }
        let id = bp.add(self.component, TYPE_LIST_STYLE, &fields)?;
        self.list_variations.insert(look, id);
        self.register(self.list, id);
        Ok(id)
    }

    /// The anonymous variation of `self.column` for multi-column paragraph layouts (`count >= 2`).
    pub(super) fn column_variation(
        &mut self,
        bp: &mut Blueprint,
        cols: RichColumns,
        body_width_pt: f32,
    ) -> Result<u64, EasyOfficeError> {
        let count = cols.count.clamp(2, 16);
        let body_w_twips = (body_width_pt * 20.0).max(1.0);
        let gap_frac = (f32::from(cols.gap_twips) / body_w_twips).clamp(0.0, 0.9);
        let key = (count, cols.equal, gap_frac.to_bits());
        if let Some(id) = self.column_variations.get(&key) {
            return Ok(*id);
        }
        let columns_inner = if cols.equal {
            let mut eq = vec![varint(1, u64::from(count))];
            if gap_frac > 0.0 {
                eq.push(float(2, gap_frac));
            }
            msg(1, eq)
        } else {
            let avail = (1.0 - gap_frac * f32::from(count - 1)).max(0.1);
            let width = avail / f32::from(count);
            let mut uneq = vec![float(1, width)];
            for _ in 1..count {
                uneq.push(msg(2, vec![float(1, gap_frac), float(2, width)]));
            }
            msg(2, uneq)
        };
        let fields = [
            variation_header(self.column, self.stylesheet),
            varint(10, 1),
            msg(11, vec![msg(7, vec![columns_inner])]),
        ];
        let id = bp.add(self.component, TYPE_COLUMN_STYLE, &fields)?;
        self.column_variations.insert(key, id);
        self.register(self.column, id);
        Ok(id)
    }

    /// The theme's presets (drawing presets, and the text styles the paragraph-style menu offers).
    pub(super) fn theme(&self) -> Vec<Fld> {
        let mut theme = vec![reference(4, self.stylesheet)];
        theme.extend(PALETTE.iter().map(|rgb_value| {
            let [_, r, g, b] = rgb_value.to_be_bytes();
            msg(10, rgb([r, g, b]))
        }));
        theme.push(msg(
            100,
            self.presets
                .iter()
                .map(|(field, id)| reference(*field, *id))
                .collect(),
        ));
        let mut text = vec![reference(1, self.list), reference(6, self.null_character)];
        text.push(reference(7, self.body));
        text.extend(self.headings.iter().map(|id| reference(7, *id)));
        theme.push(msg(110, text));
        vec![msg(1, theme)]
    }

    /// Writes the stylesheet itself, last, once every variation is known.
    pub(super) fn finish(self, bp: &mut Blueprint) -> Result<(), EasyOfficeError> {
        let mut fields: Vec<Fld> = Vec::new();
        for (_, children) in self.families.iter().filter(|(p, _)| *p != self.column) {
            fields.extend(children.iter().map(|id| reference(1, *id)));
        }
        fields.push(varint(4, 0));
        for (parent, children) in &self.families {
            let mut family = vec![reference(1, *parent)];
            family.extend(children.iter().map(|id| reference(2, *id)));
            fields.push(msg(5, family));
        }
        // `can_cull_styles`: a style nothing refers to is removed at the next save.
        fields.push(varint(6, 1));
        let mut ids: Vec<u64> = Vec::new();
        for (_, id) in &self.named {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
        let mut listed: Vec<Fld> = ids.iter().map(|id| reference(1, *id)).collect();
        listed.extend(
            self.named
                .iter()
                .map(|(identifier, id)| msg(2, vec![string(1, identifier), reference(2, *id)])),
        );
        fields.push(msg(8, listed));
        bp.put(self.component, self.stylesheet, TYPE_STYLESHEET, &fields)
    }

    pub(super) fn table_cell_fill(
        &mut self,
        bp: &mut Blueprint,
        fill_rgb: [u8; 3],
    ) -> Result<u64, EasyOfficeError> {
        if let Some(id) = self.cell_variations.get(&fill_rgb) {
            return Ok(*id);
        }
        let fields = [
            variation_header(self.table_cell, self.stylesheet),
            varint(table::CELL_STYLE_OVERRIDES, 1),
            msg(
                table::CELL_STYLE_PROPS,
                vec![msg(
                    table::CELL_PROPS_FILL,
                    vec![msg(table::FILL_COLOR, rgb(fill_rgb))],
                )],
            ),
        ];
        let id = bp.add(self.component, TYPE_CELL_STYLE, &fields)?;
        self.cell_variations.insert(fill_rgb, id);
        self.register(self.table_cell, id);
        Ok(id)
    }
}
