//! Ordered, formatted document model used by structured saves.
//!
//! A [`RichDocument`] is a sequence of blocks: paragraphs (formatted runs plus alignment, heading
//! level, and list kind) and tables at their position in the document. Run text is always literal;
//! no part of it is interpreted as markup. Formats with native run/paragraph formatting (DOCX,
//! HWPX, ODT, RTF, Markdown) are written by [`crate::rich_writers`]; every other format receives
//! the flattened [`UniversalDocumentModel`].

use crate::rich_image::RichImage;
use crate::rich_table::RichTable;
use crate::{
    DocumentFormat, EasyOfficeError, HwpxCodec, UniversalDocumentModel, UniversalFormatConverter,
    XlsxFormulaCache,
};

/// Horizontal paragraph alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RichAlignment {
    Left,
    Center,
    Right,
    Justify,
}

impl RichAlignment {
    /// Parses CSS/OOXML-style alignment tokens (`left`, `start`, `center`, `right`, `end`,
    /// `justify`, `both`).
    pub fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "left" | "start" => Some(Self::Left),
            "center" | "centre" => Some(Self::Center),
            "right" | "end" => Some(Self::Right),
            "justify" | "both" | "distribute" => Some(Self::Justify),
            _ => None,
        }
    }
}

/// List paragraph kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RichListKind {
    Bullet,
    Ordered,
}

/// Numbering style of an ordered list level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RichNumberFormat {
    /// 1, 2, 3
    #[default]
    Decimal,
    /// a, b, c
    LowerLatin,
    /// A, B, C
    UpperLatin,
    /// i, ii, iii
    LowerRoman,
    /// I, II, III
    UpperRoman,
    /// 가, 나, 다
    HangulSyllable,
}

/// Punctuation around the number of an ordered list level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RichNumberEnclosure {
    /// `1.`
    #[default]
    Dot,
    /// `(1)`
    Parentheses,
    /// `1)`
    RightParenthesis,
}

/// How an ordered list paragraph is numbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RichListNumber {
    pub format: RichNumberFormat,
    pub enclosure: RichNumberEnclosure,
}

/// Vertical position of a run relative to the baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RichScript {
    #[default]
    Normal,
    Superscript,
    Subscript,
}

/// A span of literal text sharing one character format.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Font size in half-points (`w:sz` unit); `None` inherits the document default.
    pub size_half_pt: Option<u16>,
    /// Font family name; `None` inherits the document default.
    pub font: Option<String>,
    /// Text color as six upper-case hex digits (`RRGGBB`, no `#`).
    pub color: Option<String>,
    /// Highlight/shading color as six upper-case hex digits.
    pub highlight: Option<String>,
    /// Superscript / subscript position.
    pub script: RichScript,
    /// Hyperlink target the run belongs to (read from Pages; the writers do not emit links yet).
    pub link: Option<String>,
}

/// Normalizes `#rrggbb`/`RRGGBB` to upper-case `RRGGBB`; anything else (including `auto`) is
/// `None`.
pub fn normalize_hex_color(raw: &str) -> Option<String> {
    let hex = raw.trim().trim_start_matches('#');
    (hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then(|| hex.to_ascii_uppercase())
}

/// Named highlight colors of `w:highlight` and their RGB values.
const WORD_HIGHLIGHTS: [(&str, &str); 16] = [
    ("yellow", "FFFF00"),
    ("green", "00FF00"),
    ("cyan", "00FFFF"),
    ("magenta", "FF00FF"),
    ("blue", "0000FF"),
    ("red", "FF0000"),
    ("darkBlue", "000080"),
    ("darkCyan", "008080"),
    ("darkGreen", "008000"),
    ("darkMagenta", "800080"),
    ("darkRed", "800000"),
    ("darkYellow", "808000"),
    ("darkGray", "808080"),
    ("lightGray", "C0C0C0"),
    ("black", "000000"),
    ("white", "FFFFFF"),
];

/// RGB of a `w:highlight` color name (`none` and unknown names are `None`).
pub(crate) fn highlight_name_to_hex(name: &str) -> Option<String> {
    WORD_HIGHLIGHTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, hex)| (*hex).to_string())
}

/// `w:highlight` name for an RGB value that is exactly one of the named colors.
pub(crate) fn hex_to_highlight_name(hex: &str) -> Option<&'static str> {
    WORD_HIGHLIGHTS
        .iter()
        .find(|(_, h)| *h == hex)
        .map(|(n, _)| *n)
}

impl RichRun {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }
}

/// A paragraph: runs plus paragraph properties. `heading_level` 0 is body text, 1–6 headings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichParagraph {
    pub runs: Vec<RichRun>,
    pub alignment: Option<RichAlignment>,
    pub heading_level: u8,
    pub list: Option<RichListKind>,
    /// Nesting level of a list paragraph, 0 for the outermost (read from Pages; the writers do
    /// not emit levels yet).
    pub list_level: u8,
    /// Number format of an ordered list paragraph; `None` is the plain `1.` (read from Pages; the
    /// writers do not emit formats yet).
    pub list_number: Option<RichListNumber>,
    /// Left indent in twips (1/20 point); `None` inherits.
    pub left_indent_twips: Option<u32>,
    /// First-line indent in twips; `None` inherits.
    pub first_line_indent_twips: Option<i32>,
    /// Line height as a percentage of single spacing (200 = double); `None` inherits.
    pub line_spacing_percent: Option<u16>,
}

impl RichParagraph {
    pub fn plain_text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

/// One block of a [`RichDocument`], in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RichBlock {
    Paragraph(RichParagraph),
    /// Table rows of final cell text (formula cells already replaced by their values) and its
    /// merged regions.
    Table(RichTable),
    /// A PNG/JPEG picture on its own line; formats without picture support get its alt text.
    Image(RichImage),
}

/// Ordered, formatted document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichDocument {
    pub title: String,
    pub blocks: Vec<RichBlock>,
    pub official_agency: Option<String>,
}

impl RichDocument {
    pub(crate) fn display_title(&self) -> String {
        if self.title.trim().is_empty() {
            "문서".to_string()
        } else {
            self.title.clone()
        }
    }

    /// Flattens to the plain model for formats without native formatting support.
    pub fn to_universal_model(&self) -> UniversalDocumentModel {
        let mut paragraphs = Vec::new();
        let mut tables = Vec::new();
        for block in &self.blocks {
            match block {
                RichBlock::Paragraph(p) => paragraphs.push(p.plain_text()),
                RichBlock::Table(table) => tables.push(table.rows.clone()),
                RichBlock::Image(img) => paragraphs.push(img.fallback_text().to_string()),
            }
        }
        UniversalDocumentModel {
            title: self.title.clone(),
            paragraphs,
            tables,
            sheet_cells: Vec::new(),
            slides: Vec::new(),
            official_agency: self.official_agency.clone(),
        }
    }
}

const TABLE_FORMULA_FUNCS: [&str; 6] = ["SUM", "AVERAGE", "COUNT", "MAX", "MIN", "PRODUCT"];

fn parse_table_formula(text: &str) -> Option<(&'static str, &'static str)> {
    let body = text.trim().strip_prefix('=')?.trim();
    let open = body.find('(')?;
    let func_raw = body[..open].trim().to_ascii_uppercase();
    let arg_raw = body[open + 1..]
        .trim()
        .strip_suffix(')')?
        .trim()
        .to_ascii_uppercase();
    let func = TABLE_FORMULA_FUNCS.into_iter().find(|f| *f == func_raw)?;
    let dir = ["ABOVE", "BELOW", "LEFT", "RIGHT"]
        .into_iter()
        .find(|d| *d == arg_raw)?;
    Some((func, dir))
}

fn parse_table_number(text: &str) -> Option<f64> {
    let cleaned: String = text
        .trim()
        .chars()
        .filter(|c| *c != ',' && !c.is_whitespace())
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    cleaned.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// Formats a computed table value: integers without a fraction, otherwise up to 10 decimals.
pub fn format_table_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    let s = format!("{n:.10}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Replaces word-processor table formulas (`=SUM(ABOVE)`, `=AVERAGE(LEFT)`, … with SUM, AVERAGE,
/// COUNT, MAX, MIN, PRODUCT over ABOVE, BELOW, LEFT, RIGHT) by their computed values, row by row.
/// Non-numeric cells are skipped; unrecognized formulas are left as text.
pub fn evaluate_table_formulas(rows: &mut [Vec<String>]) {
    for r in 0..rows.len() {
        for c in 0..rows[r].len() {
            let Some((func, dir)) = parse_table_formula(&rows[r][c]) else {
                continue;
            };
            let cell_at = |rr: usize, cc: usize| rows.get(rr).and_then(|row| row.get(cc));
            let operands: Vec<f64> = match dir {
                "ABOVE" => (0..r).filter_map(|rr| cell_at(rr, c)).collect::<Vec<_>>(),
                "BELOW" => (r + 1..rows.len())
                    .filter_map(|rr| cell_at(rr, c))
                    .collect(),
                "LEFT" => rows[r][..c].iter().collect(),
                _ => rows[r][c + 1..].iter().collect(),
            }
            .into_iter()
            .filter_map(|s| parse_table_number(s))
            .collect();
            let value = match func {
                "SUM" => operands.iter().sum(),
                "COUNT" => operands.len() as f64,
                "AVERAGE" if !operands.is_empty() => {
                    operands.iter().sum::<f64>() / operands.len() as f64
                }
                "MAX" if !operands.is_empty() => operands.iter().copied().fold(f64::MIN, f64::max),
                "MIN" if !operands.is_empty() => operands.iter().copied().fold(f64::MAX, f64::min),
                "PRODUCT" if !operands.is_empty() => operands.iter().product(),
                _ => 0.0,
            };
            rows[r][c] = format_table_number(value);
        }
    }
}

impl UniversalFormatConverter {
    /// Encodes a [`RichDocument`]. DOCX, HWPX/HWTX, ODT, RTF, Markdown, TXT, and PDF keep block
    /// order (and formatting where the format has it); other formats get the flattened model.
    pub fn encode_rich(
        doc: &RichDocument,
        format: DocumentFormat,
        formula_cache: Option<&XlsxFormulaCache>,
    ) -> Result<Vec<u8>, EasyOfficeError> {
        use crate::rich_writers as w;
        match format {
            DocumentFormat::Docx => w::pack_docx_rich(doc),
            DocumentFormat::Hwpx | DocumentFormat::Hwtx => {
                HwpxCodec::pack_rich(doc, format == DocumentFormat::Hwtx)
            }
            DocumentFormat::Odt => w::pack_odt_rich(doc),
            DocumentFormat::Rtf => Ok(crate::rich_rtf_writer::encode_rtf_rich(doc).into_bytes()),
            DocumentFormat::Markdown => Ok(w::render_markdown_rich(doc).into_bytes()),
            DocumentFormat::Txt => Ok(w::render_plain_text(doc, "\t", true).into_bytes()),
            DocumentFormat::Pdf => Ok(w::serialize_pdf_rich(doc)),
            _ => Self::encode_with_formula_cache(&doc.to_universal_model(), format, formula_cache),
        }
    }

    /// [`Self::encode_rich`] with a preview picture in the file (PD-132). The engine draws no
    /// pages, so `thumbnail_png` comes from the caller; `None`, or a format without a preview part
    /// (see [`crate::supports_thumbnail`]), gives exactly what [`Self::encode_rich`] gives. A
    /// picture that is not a valid PNG within the limits is an error.
    pub fn encode_rich_with_thumbnail(
        doc: &RichDocument,
        format: DocumentFormat,
        formula_cache: Option<&XlsxFormulaCache>,
        thumbnail_png: Option<&[u8]>,
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let bytes = Self::encode_rich(doc, format, formula_cache)?;
        match thumbnail_png {
            Some(png) if crate::supports_thumbnail(format) => {
                crate::embed_thumbnail(format, &bytes, png)
            }
            _ => Ok(bytes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&[&str]]) -> Vec<Vec<String>> {
        rows.iter()
            .map(|r| r.iter().map(|s| s.to_string()).collect())
            .collect()
    }

    #[test]
    fn table_formulas_evaluate_in_every_direction() {
        let mut rows = grid(&[
            &["항목", "1,000", "=SUM(RIGHT)", "2", "3"],
            &["a", "20", "x", "", ""],
            &["합계", "=sum(above)", "=COUNT(LEFT)", "", ""],
            &["평균", "=AVERAGE(ABOVE)", "=MAX(BELOW)", "", ""],
            &["", "=PRODUCT(LEFT)", "7", "", ""],
        ]);
        evaluate_table_formulas(&mut rows);
        assert_eq!(rows[0][2], "5");
        assert_eq!(rows[2][1], "1020");
        assert_eq!(rows[2][2], "1");
        assert_eq!(rows[3][1], "680");
        assert_eq!(rows[3][2], "7");
        assert_eq!(rows[4][1], "0");
    }

    #[test]
    fn unrecognized_formulas_and_fractions() {
        let mut rows = grid(&[&["1"], &["2"], &["=AVERAGE(ABOVE)"], &["=FOO(ABOVE)"]]);
        evaluate_table_formulas(&mut rows);
        assert_eq!(rows[2][0], "1.5");
        assert_eq!(rows[3][0], "=FOO(ABOVE)");
        assert_eq!(format_table_number(1.0 / 3.0), "0.3333333333");
    }

    #[test]
    fn flattening_keeps_block_text_and_tables() {
        let doc = RichDocument {
            title: "t".into(),
            blocks: vec![
                RichBlock::Paragraph(RichParagraph {
                    runs: vec![RichRun::plain("가"), RichRun::plain("나")],
                    ..RichParagraph::default()
                }),
                RichBlock::Table(grid(&[&["1"]]).into()),
            ],
            official_agency: None,
        };
        let model = doc.to_universal_model();
        assert_eq!(model.paragraphs, vec!["가나".to_string()]);
        assert_eq!(model.tables.len(), 1);
        assert_eq!(model.official_agency, None);
    }
}
