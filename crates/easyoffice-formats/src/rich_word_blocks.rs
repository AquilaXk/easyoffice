//! The word processor's paragraph and block features of PD-401 that DOCX stores and the other formats carry as far as they
//! can: tab stops (`w:tabs`), paragraph borders and shading (`w:pBdr`, `w:shd`), the drop cap (`w:framePr w:dropCap`), the
//! column separator and the narrow side of two columns (`w:cols`), a column break (`w:br w:type="column"`), text boxes
//! (`w:pict` with `v:textbox`, `wps:txbx`), equations (the `m:` math of OMML, kept as the file wrote it) and the styles the
//! user made (`w:style w:customStyle`).
//!
//! Reference implementations (구현 원칙 3단계: (1) no crate: both libraries below hold their own object models and write or draw
//! HTML; (2) no dependency of the workspace reads these elements; (3) the element shapes are followed, no code is copied): the
//! MIT DOCX writer library `docx` (`src/file/paragraph/formatting/tab-stop.ts` lines 103-146, `border.ts` 69-96,
//! `src/file/textbox/textbox.ts` 100-128, `shape/shape.ts` 101-107, `src/file/styles/style/paragraph-style.ts`, commit
//! d15c565e) and the Apache-2.0 DOCX renderer `docx-preview` (`src/document-parser.ts` `parseFrame` lines 582-587,
//! `parseBorderProperties` 1536 and on, commit 28682e30). Every value that comes from a file is checked here: a count,
//! a size or a name past its bound reads as absent, and a save request that carries one is refused.

use crate::rich_image_layout::RichImageWrap;
use crate::{RichAlignment, RichLineSpacing, RichParagraph};

/// Most tab stops of one paragraph (Word keeps 64).
pub const MAX_TAB_STOPS: usize = 64;
/// Furthest tab stop, in twips (22 in, the widest page the model keeps).
pub const MAX_TAB_POSITION_TWIPS: u32 = 31_680;
/// Most text boxes a document keeps; one past this reads as absent.
pub const MAX_TEXT_BOXES: usize = 256;
/// Most paragraphs of one text box.
pub const MAX_TEXT_BOX_PARAGRAPHS: usize = 64;
/// Largest side of a text box, in EMU (22 in).
pub const MAX_TEXT_BOX_EMU: u32 = 20_116_800;
/// Smallest side of a text box, in EMU (0.1 cm).
pub const MIN_TEXT_BOX_EMU: u32 = 36_000;
/// Most equations a document keeps.
pub const MAX_MATHS: usize = 2_000;
/// Largest equation, in bytes of its OMML.
pub const MAX_MATH_BYTES: usize = 32_768;
/// Most elements of one equation.
pub const MAX_MATH_NODES: usize = 2_000;
/// Deepest nesting of one equation.
pub const MAX_MATH_DEPTH: usize = 24;
/// Most styles of the user a document keeps.
pub const MAX_CUSTOM_STYLES: usize = 64;
/// Longest style name, in characters.
pub const MAX_STYLE_NAME_CHARS: usize = 64;
/// Most lines a drop cap spans (Word's own range is 1 to 10; one line is no drop).
pub const MAX_DROP_CAP_LINES: u8 = 10;
/// Most columns of a section.
pub const MAX_COLUMNS: u8 = 16;
/// Largest border width, in eighths of a point (`w:sz`, ECMA-376 17.18.2: 2 to 96).
pub const MAX_BORDER_EIGHTHS: u8 = 96;
/// Largest distance of a border from the text, in points (`w:space`).
pub const MAX_BORDER_SPACE_PT: u8 = 31;

/// How the text lines up at a tab stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RichTabAlign {
    #[default]
    Left,
    Center,
    Right,
    Decimal,
}

impl RichTabAlign {
    pub const ALL: [Self; 4] = [Self::Left, Self::Center, Self::Right, Self::Decimal];

    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
            Self::Decimal => "decimal",
        }
    }

    pub fn from_wire_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.wire_name() == name)
    }

    /// The tab type of a DOCX `w:tab/@w:val`; the bar, the clear mark and the list tab are not stops of ours.
    pub fn from_docx(value: &str) -> Option<Self> {
        match value {
            "left" | "start" => Some(Self::Left),
            "center" => Some(Self::Center),
            "right" | "end" => Some(Self::Right),
            "decimal" => Some(Self::Decimal),
            _ => None,
        }
    }
}

/// The character that fills the space before a tab stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RichTabLeader {
    #[default]
    None,
    Dot,
    Hyphen,
    Underscore,
}

impl RichTabLeader {
    pub const ALL: [Self; 4] = [Self::None, Self::Dot, Self::Hyphen, Self::Underscore];

    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Dot => "dot",
            Self::Hyphen => "hyphen",
            Self::Underscore => "underscore",
        }
    }

    pub fn from_wire_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.wire_name() == name)
    }

    /// The leader of a DOCX `w:tab/@w:leader` (`middleDot` reads as the dot, `heavy` as the underscore).
    pub fn from_docx(value: &str) -> Self {
        match value {
            "dot" | "middleDot" => Self::Dot,
            "hyphen" => Self::Hyphen,
            "underscore" | "heavy" => Self::Underscore,
            _ => Self::None,
        }
    }
}

/// One tab stop of a paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RichTabStop {
    pub pos_twips: u32,
    pub align: RichTabAlign,
    pub leader: RichTabLeader,
}

/// The tab stops a paragraph keeps: at most [`MAX_TAB_STOPS`] within [`MAX_TAB_POSITION_TWIPS`], in order of position, one
/// to a position (the later one wins, as a stop set again in the dialog replaces the old one).
pub fn normalize_tab_stops(stops: impl IntoIterator<Item = RichTabStop>) -> Vec<RichTabStop> {
    let mut kept: Vec<RichTabStop> = Vec::new();
    for stop in stops
        .into_iter()
        .filter(|s| s.pos_twips <= MAX_TAB_POSITION_TWIPS)
    {
        if let Some(same) = kept.iter_mut().find(|k| k.pos_twips == stop.pos_twips) {
            *same = stop;
        } else if kept.len() < MAX_TAB_STOPS {
            kept.push(stop);
        }
    }
    kept.sort_by_key(|s| s.pos_twips);
    kept
}

/// The line of a paragraph border.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RichBorderStyle {
    #[default]
    Single,
    Double,
    Dotted,
    Dashed,
}

impl RichBorderStyle {
    pub const ALL: [Self; 4] = [Self::Single, Self::Double, Self::Dotted, Self::Dashed];

    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Double => "double",
            Self::Dotted => "dotted",
            Self::Dashed => "dashed",
        }
    }

    pub fn from_wire_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.wire_name() == name)
    }

    /// The style of a DOCX `w:val`; `nil` and `none` are no border, any other line reads as a single one.
    pub fn from_docx(value: &str) -> Option<Self> {
        match value {
            "nil" | "none" | "" => None,
            "double" => Some(Self::Double),
            "dotted" => Some(Self::Dotted),
            "dashed" | "dashSmallGap" | "dotDash" | "dotDotDash" => Some(Self::Dashed),
            _ => Some(Self::Single),
        }
    }
}

/// One side of a paragraph border.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RichBorderLine {
    pub style: RichBorderStyle,
    /// Width in eighths of a point (`w:sz`), 2 to [`MAX_BORDER_EIGHTHS`].
    pub eighths: u8,
    /// Six upper-case hex digits; `None` is the automatic (black) color.
    pub color: Option<String>,
    /// Distance from the text in points.
    pub space_pt: u8,
}

impl RichBorderLine {
    /// The line within the bounds, or `None` for a width or a color a file should not give.
    pub fn checked(self) -> Option<Self> {
        let color_ok = self
            .color
            .as_deref()
            .is_none_or(|c| crate::normalize_hex_color(c).as_deref() == Some(c));
        ((2..=MAX_BORDER_EIGHTHS).contains(&self.eighths)
            && self.space_pt <= MAX_BORDER_SPACE_PT
            && color_ok)
            .then_some(self)
    }
}

/// The four sides of a paragraph border; `None` is no line.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct RichParaBorders {
    pub top: Option<RichBorderLine>,
    pub right: Option<RichBorderLine>,
    pub bottom: Option<RichBorderLine>,
    pub left: Option<RichBorderLine>,
}

impl RichParaBorders {
    pub fn is_empty(&self) -> bool {
        self.top.is_none() && self.right.is_none() && self.bottom.is_none() && self.left.is_none()
    }
}

/// A drop cap: the paragraph holds the large first letters and the next paragraph wraps around them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RichDropCap {
    /// Lines the letter is as tall as, 2 to [`MAX_DROP_CAP_LINES`].
    pub lines: u8,
    /// `margin` (the letter is out in the margin) instead of `drop` (inside the text).
    pub in_margin: bool,
}

impl RichDropCap {
    pub fn checked(self) -> Option<Self> {
        (2..=MAX_DROP_CAP_LINES)
            .contains(&self.lines)
            .then_some(self)
    }
}

/// The narrow side of a two-column section (the word processor's 왼쪽 and 오른쪽 column presets).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RichNarrowColumn {
    #[default]
    None,
    First,
    Last,
}

impl RichNarrowColumn {
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::First => "first",
            Self::Last => "last",
        }
    }

    pub fn from_wire_name(name: &str) -> Option<Self> {
        [Self::None, Self::First, Self::Last]
            .into_iter()
            .find(|n| n.wire_name() == name)
    }
}

/// What a column layout adds to [`crate::RichColumns`]: the line between the columns and the narrow side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RichColumnLook {
    pub separator: bool,
    pub narrow: RichNarrowColumn,
}

impl RichColumnLook {
    pub fn is_default(self) -> bool {
        self == Self::default()
    }
}

/// A text box anchored to a paragraph: its own paragraphs, its size and how the text goes around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichTextBox {
    pub paragraphs: Vec<RichParagraph>,
    pub width_emu: u32,
    pub height_emu: u32,
    pub wrap: RichImageWrap,
    /// Fill color (six upper-case hex digits); `None` is no fill.
    pub fill: Option<String>,
    /// Whether the box has its outline.
    pub border: bool,
}

impl RichTextBox {
    /// The box when its size and paragraph count are within bounds; the paragraphs hold no text box (no nesting).
    pub fn checked(mut self) -> Option<Self> {
        let side = MIN_TEXT_BOX_EMU..=MAX_TEXT_BOX_EMU;
        if !side.contains(&self.width_emu)
            || !side.contains(&self.height_emu)
            || self.paragraphs.len() > MAX_TEXT_BOX_PARAGRAPHS
            || self.wrap == RichImageWrap::Inline
        {
            return None;
        }
        for p in &mut self.paragraphs {
            p.word.text_boxes.clear();
        }
        Some(self)
    }
}

/// A style of the user (DOCX `w:style` with `w:customStyle`): a name and the look it gives a paragraph.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichStyleDef {
    pub name: String,
    pub font: Option<String>,
    pub size_half_pt: Option<u16>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Option<String>,
    pub alignment: Option<RichAlignment>,
    pub left_indent_twips: Option<u32>,
    pub first_line_indent_twips: Option<i32>,
    pub space_before_twips: Option<u32>,
    pub space_after_twips: Option<u32>,
    pub line_spacing: Option<RichLineSpacing>,
}

impl RichStyleDef {
    /// The style within bounds: a name of 1 to [`MAX_STYLE_NAME_CHARS`] characters without control characters.
    pub fn checked(self) -> Option<Self> {
        let name = self.name.trim();
        let ok = !name.is_empty()
            && name.chars().count() <= MAX_STYLE_NAME_CHARS
            && !name.chars().any(char::is_control)
            && name == self.name
            && self.size_half_pt.is_none_or(|s| (1..=3276).contains(&s))
            && self
                .color
                .as_deref()
                .is_none_or(|c| crate::normalize_hex_color(c).as_deref() == Some(c));
        ok.then_some(self)
    }
}

/// The names the word processor gives its own styles; a style of the user cannot take one (a file with the same name would
/// be read as that style).
pub fn is_reserved_style_name(name: &str) -> bool {
    const RESERVED: [&str; 12] = [
        "표준",
        "본문",
        "Normal",
        "Caption",
        "caption",
        "Hyperlink",
        "List Bullet",
        "List Number",
        "Table Normal",
        "Header",
        "Footer",
        "Default Paragraph Font",
    ];
    RESERVED.iter().any(|r| r.eq_ignore_ascii_case(name))
        || crate::rich_xml::heading_level_from_name(name) > 0
        || name.to_ascii_lowercase().starts_with("toc")
}

/// What a paragraph carries of the features of PD-401 (one field of [`RichParagraph`], so a paragraph without them costs one
/// empty value).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichParagraphWord {
    pub tab_stops: Vec<RichTabStop>,
    pub borders: Option<RichParaBorders>,
    /// Paragraph shading: fill color, six upper-case hex digits.
    pub shading: Option<String>,
    pub drop_cap: Option<RichDropCap>,
    /// The paragraph starts in the next column (a `w:br w:type="column"` before its text).
    pub column_break_before: bool,
    /// The look of the column layout the paragraph's section has.
    pub column_look: Option<RichColumnLook>,
    pub text_boxes: Vec<RichTextBox>,
    /// The name of the style of the user the paragraph has ([`crate::RichDocument::styles`]).
    pub style_name: Option<String>,
}

impl RichParagraphWord {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// The OMML of an equation of a save request, checked against the list of elements and the bounds the reader holds, and its
/// linear text; `None` when it is not one equation or past a bound.
pub fn checked_omml(omml: &str) -> Option<(String, String)> {
    crate::rich_read_ooxml::checked_omml(omml)
}

// ---------------------------------------------------------------- DOCX writers

/// The id of a style of the user in `word/styles.xml` (`EoStyle` and its place in the list), or `None` for a name the
/// document does not list.
pub(crate) fn docx_custom_style_id(styles: &[RichStyleDef], name: &str) -> Option<String> {
    styles
        .iter()
        .position(|s| s.name == name)
        .map(|i| format!("EoStyle{}", i + 1))
}

/// The styles of the user as `w:style` elements (paragraph styles based on the normal one, as the DOCX writer library's
/// `ParagraphStyle` builds them: `w:name`, `w:basedOn`, `w:qFormat`, then `w:pPr` and `w:rPr`).
pub(crate) fn docx_custom_styles_xml(styles: &[RichStyleDef]) -> String {
    let mut xml = String::new();
    for (i, s) in styles.iter().enumerate() {
        xml.push_str(&format!(
            "<w:style w:type=\"paragraph\" w:customStyle=\"1\" w:styleId=\"EoStyle{}\"><w:name w:val=\"{}\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/>",
            i + 1,
            crate::XmlSanitizer::escape(&s.name)
        ));
        let mut ppr = String::new();
        if s.space_before_twips.is_some()
            || s.space_after_twips.is_some()
            || s.line_spacing.is_some()
        {
            ppr.push_str("<w:spacing");
            if let Some(v) = s.space_before_twips {
                ppr.push_str(&format!(" w:before=\"{v}\""));
            }
            if let Some(v) = s.space_after_twips {
                ppr.push_str(&format!(" w:after=\"{v}\""));
            }
            if let Some(spacing) = s.line_spacing {
                ppr.push_str(&crate::rich_writers::docx_line_attributes(spacing));
            }
            ppr.push_str("/>");
        }
        if s.left_indent_twips.is_some() || s.first_line_indent_twips.is_some() {
            ppr.push_str("<w:ind");
            if let Some(v) = s.left_indent_twips {
                ppr.push_str(&format!(" w:left=\"{v}\""));
            }
            match s.first_line_indent_twips {
                Some(v) if v >= 0 => ppr.push_str(&format!(" w:firstLine=\"{v}\"")),
                Some(v) => ppr.push_str(&format!(" w:hanging=\"{}\"", v.unsigned_abs())),
                None => {}
            }
            ppr.push_str("/>");
        }
        if let Some(a) = s.alignment {
            let jc = match a {
                RichAlignment::Left => "left",
                RichAlignment::Center => "center",
                RichAlignment::Right => "right",
                RichAlignment::Justify => "both",
            };
            ppr.push_str(&format!("<w:jc w:val=\"{jc}\"/>"));
        }
        if !ppr.is_empty() {
            xml.push_str(&format!("<w:pPr>{ppr}</w:pPr>"));
        }
        let mut rpr = String::new();
        if let Some(font) = &s.font {
            let f = crate::XmlSanitizer::escape(font);
            rpr.push_str(&format!(
                "<w:rFonts w:ascii=\"{f}\" w:hAnsi=\"{f}\" w:eastAsia=\"{f}\"/>"
            ));
        }
        if s.bold {
            rpr.push_str("<w:b/>");
        }
        if s.italic {
            rpr.push_str("<w:i/>");
        }
        if let Some(color) = &s.color {
            rpr.push_str(&format!("<w:color w:val=\"{color}\"/>"));
        }
        if let Some(size) = s.size_half_pt {
            rpr.push_str(&format!(
                "<w:sz w:val=\"{size}\"/><w:szCs w:val=\"{size}\"/>"
            ));
        }
        if s.underline {
            rpr.push_str("<w:u w:val=\"single\"/>");
        }
        if !rpr.is_empty() {
            xml.push_str(&format!("<w:rPr>{rpr}</w:rPr>"));
        }
        xml.push_str("</w:style>");
    }
    xml
}

/// `w:framePr` of a drop cap paragraph, as the word processor writes it (the paragraph is a frame that the text wraps around).
pub(crate) fn docx_frame_pr_xml(cap: Option<RichDropCap>) -> String {
    cap.map_or(String::new(), |c| {
        format!(
            "<w:framePr w:dropCap=\"{}\" w:lines=\"{}\" w:wrap=\"around\" w:vAnchor=\"text\" w:hAnchor=\"text\"/>",
            if c.in_margin { "margin" } else { "drop" },
            c.lines
        )
    })
}

/// `w:pBdr` and `w:shd` of a paragraph (schema order: before `w:tabs`).
pub(crate) fn docx_borders_xml(w: &RichParagraphWord) -> String {
    let mut xml = String::new();
    if let Some(borders) = w.borders.as_ref().filter(|b| !b.is_empty()) {
        xml.push_str("<w:pBdr>");
        for (tag, side) in [
            ("w:top", &borders.top),
            ("w:left", &borders.left),
            ("w:bottom", &borders.bottom),
            ("w:right", &borders.right),
        ] {
            if let Some(line) = side {
                xml.push_str(&format!(
                    "<{tag} w:val=\"{}\" w:sz=\"{}\" w:space=\"{}\" w:color=\"{}\"/>",
                    line.style.wire_name(),
                    line.eighths,
                    line.space_pt,
                    line.color.as_deref().unwrap_or("auto")
                ));
            }
        }
        xml.push_str("</w:pBdr>");
    }
    if let Some(fill) = &w.shading {
        xml.push_str(&format!(
            "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{fill}\"/>"
        ));
    }
    xml
}

/// `w:tabs` of the stops of a paragraph.
pub(crate) fn docx_tab_stops_xml(stops: &[RichTabStop]) -> String {
    if stops.is_empty() {
        return String::new();
    }
    let mut xml = String::from("<w:tabs>");
    for s in stops {
        xml.push_str(&format!("<w:tab w:val=\"{}\"", s.align.wire_name()));
        if s.leader != RichTabLeader::None {
            xml.push_str(&format!(" w:leader=\"{}\"", s.leader.wire_name()));
        }
        xml.push_str(&format!(" w:pos=\"{}\"/>", s.pos_twips));
    }
    xml.push_str("</w:tabs>");
    xml
}

const VML_NAMESPACE: &str = concat!("urn:schemas-mi", "crosoft-com:vml");
const OFFICE_NAMESPACE: &str = concat!("urn:schemas-mi", "crosoft-com:office:office");
const WORD_VML_NAMESPACE: &str = concat!("urn:schemas-mi", "crosoft-com:office:word");

/// A text box as the DOCX writer library writes it (`w:r` with a `w:pict` holding a `v:shape` of the text box type and its
/// `v:textbox`), anchored to the paragraph. `paragraphs` is the body of the box, already written.
pub(crate) fn docx_text_box_run(index: usize, b: &RichTextBox, paragraphs: &str) -> String {
    // Points with four decimals, rounded (a 96 dpi pixel is 0.75 pt, so pixel sizes are written exactly).
    let pt = |emu: u32| {
        let tenth_thousandths = (u64::from(emu) * 10_000 + 6_350) / 12_700;
        format!(
            "{}.{:04}pt",
            tenth_thousandths / 10_000,
            tenth_thousandths % 10_000
        )
    };
    let (z, wrap) = match b.wrap {
        RichImageWrap::Behind => ("-1", "none"),
        RichImageWrap::InFront => ("1", "none"),
        RichImageWrap::TopBottom => ("1", "topAndBottom"),
        _ => ("1", "square"),
    };
    let fill = b.fill.as_deref().map_or(" filled=\"f\"".to_string(), |c| {
        format!(" fillcolor=\"#{c}\"")
    });
    let stroke = if b.border { "t" } else { "f" };
    format!(
        concat!(
            "<w:r><w:pict xmlns:v=\"{vml}\" xmlns:o=\"{office}\" xmlns:w10=\"{w10}\">",
            "<v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\" path=\"m,l,21600r21600,l21600,xe\">",
            "<v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/></v:shapetype>",
            "<v:shape id=\"eo_textbox_{index}\" type=\"#_x0000_t202\" ",
            "style=\"position:absolute;margin-left:0;margin-top:0;width:{w};height:{h};z-index:{z};mso-wrap-style:square\"",
            "{fill} stroked=\"{stroke}\"><v:textbox><w:txbxContent>{paras}</w:txbxContent></v:textbox>",
            "<w10:wrap type=\"{wrap}\"/></v:shape></w:pict></w:r>"
        ),
        vml = VML_NAMESPACE,
        office = OFFICE_NAMESPACE,
        w10 = WORD_VML_NAMESPACE,
        index = index,
        w = pt(b.width_emu),
        h = pt(b.height_emu),
        z = z,
        fill = fill,
        stroke = stroke,
        paras = paragraphs,
        wrap = wrap,
    )
}
