//! Ordered, formatted document model used by structured saves.
//!
//! A [`RichDocument`] is a sequence of blocks: paragraphs (formatted runs plus alignment, heading
//! level, and list kind) and tables at their position in the document. Run text is always literal;
//! no part of it is interpreted as markup. Formats with native run/paragraph formatting (DOCX,
//! HWPX, ODT, RTF, Markdown) are written by [`crate::rich_writers`]; every other format receives
//! the flattened [`UniversalDocumentModel`].

use crate::pdf_writer::PdfWriteOptions;
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
    /// Hyperlink target the run belongs to (a [`safe_link_target`] address). The DOCX, ODT and HWPX writers
    /// emit it; the Pages writer drops it (counted by `pages_write_loss`).
    pub link: Option<String>,
    /// A field that the page fills in; `text` is empty. Only header and footer paragraphs carry one.
    pub field: Option<RichField>,
}

/// A value the page fills in at each page it is drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RichField {
    /// The number of the page (DOCX `PAGE`, ODT `text:page-number`, HWPX `autoNum` of type `PAGE`).
    PageNumber,
    /// The number of pages (DOCX `NUMPAGES`, ODT `text:page-count`, HWPX `autoNum` of type `TOTAL_PAGE`).
    TotalPages,
    /// The name of the file (DOCX `FILENAME`, ODT `text:file-name`). HWPX has no counterpart that is known
    /// to read back, so it is written as the title's text.
    FileName,
}

/// Page margins in twips, as a word processor's page setup gives them: `top` and `bottom` are the distances
/// from the page edge to the body (a taller header pushes the body down), `header` and `footer` the distances
/// from the page edge to the header and footer, `left` and `right` the distances to the body at the sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RichPageMargins {
    pub top: u32,
    pub bottom: u32,
    pub header: u32,
    pub footer: u32,
    pub left: u32,
    pub right: u32,
}

/// The paper as it is shown, in twips: a landscape page is wider than it is high.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RichPaper {
    pub width: u32,
    pub height: u32,
}

/// Where the pages are bound: one-sided (`Single`), facing pages (`Duplex`, the gutter alternates sides) or at the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RichBindingKind {
    #[default]
    Single,
    Duplex,
    Top,
}

/// The binding of the pages: its kind and the gutter in twips, added to the margin on the bound side (the reference
/// editor's page setup `제본`, `rhwp-studio/src/ui/page-setup-dialog.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RichBinding {
    pub kind: RichBindingKind,
    pub gutter: u32,
}

/// Largest page margin or paper side a document keeps, in twips (22 inches).
pub const MAX_PAGE_MARGIN_TWIPS: u32 = 31_680;
/// Least room the margins leave for the text on a page, in twips (10 mm, the limit of the reference editor's
/// page setup `rhwp-studio/src/core/page-body-limits.ts`).
pub const MIN_PAGE_BODY_TWIPS: u32 = 567;

impl RichPageMargins {
    /// The new-document page setup of the reference editor (A4: top 20 mm and bottom 15 mm margins, header
    /// and footer areas of 15 mm, side margins of 30 mm; `rhwp` `src/model/page.rs:36-51`, areas at
    /// `:238-269`), in this form: the header starts 20 mm from the top and the body 35 mm; the footer ends
    /// 15 mm from the bottom and the body 30 mm.
    pub const HWP_DEFAULT: Self = Self {
        top: 1984,
        bottom: 1700,
        header: 1134,
        footer: 850,
        left: 1701,
        right: 1701,
    };

    /// Whether every value is within [`MAX_PAGE_MARGIN_TWIPS`].
    pub fn within_bounds(&self) -> bool {
        [
            self.top,
            self.bottom,
            self.header,
            self.footer,
            self.left,
            self.right,
        ]
        .iter()
        .all(|v| *v <= MAX_PAGE_MARGIN_TWIPS)
    }
}

impl RichPaper {
    pub const A4: Self = Self {
        width: 11_906,
        height: 16_838,
    };

    /// Whether the sides are within [`MAX_PAGE_MARGIN_TWIPS`] and the `margins` leave at least
    /// [`MIN_PAGE_BODY_TWIPS`] of text width and height. The reference editor refuses a setup whose body
    /// would vanish.
    pub fn holds(&self, margins: &RichPageMargins) -> bool {
        self.holds_bound(margins, RichBinding::default())
    }

    /// [`Self::holds`] with the gutter, which takes room from the width (as the reference editor's body-size check
    /// counts it) and from the height of a page bound at the top.
    pub fn holds_bound(&self, margins: &RichPageMargins, binding: RichBinding) -> bool {
        let room = |side: u32, a: u32, b: u32| {
            side <= MAX_PAGE_MARGIN_TWIPS
                && side.saturating_sub(a).saturating_sub(b) >= MIN_PAGE_BODY_TWIPS
        };
        let (side_gutter, top_gutter) = match binding.kind {
            RichBindingKind::Top => (0, binding.gutter),
            _ => (binding.gutter, 0),
        };
        binding.gutter <= MAX_PAGE_MARGIN_TWIPS
            && margins.within_bounds()
            && room(
                self.width,
                margins.left.saturating_add(side_gutter),
                margins.right,
            )
            && room(
                self.height,
                margins.top.saturating_add(top_gutter),
                margins.bottom,
            )
    }
}

/// Most paragraphs of one header or footer; a file with more keeps the first ones.
pub const MAX_PAGE_FRAME_PARAGRAPHS: usize = 64;

/// The header and footer drawn on every page: formatted paragraphs (runs, alignment, indents and line
/// spacing; headings, lists, links and tables are not kept). Files with different first-page or
/// odd/even frames keep the default one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichPageFrame {
    pub header: Vec<RichParagraph>,
    pub footer: Vec<RichParagraph>,
    /// The page setup of the first section: the distances of the body, header and footer from the page
    /// edges. `None` when the file gives none or gives one that does not fit its paper (the editor then uses
    /// its own default, a writer the format's).
    pub margins: Option<RichPageMargins>,
    /// The paper of the first section; `None` as above (A4 is written).
    pub paper: Option<RichPaper>,
    /// The binding of the first section (one-sided with no gutter when the file gives none).
    pub binding: RichBinding,
}

impl RichPageFrame {
    pub fn is_empty(&self) -> bool {
        self.header.is_empty() && self.footer.is_empty()
    }
}

/// The paragraph as a header or footer keeps it: runs without links, alignment, indents and line spacing.
pub fn frame_paragraph(p: &RichParagraph) -> RichParagraph {
    RichParagraph {
        runs: p
            .runs
            .iter()
            .map(|r| RichRun {
                link: None,
                ..r.clone()
            })
            .collect(),
        alignment: p.alignment,
        left_indent_twips: p.left_indent_twips,
        first_line_indent_twips: p.first_line_indent_twips,
        line_spacing_percent: p.line_spacing_percent,
        ..RichParagraph::default()
    }
}

/// Longest hyperlink address a document keeps, in characters.
pub const MAX_LINK_CHARS: usize = 2048;

/// Prefix of a table-of-contents bookmark name (`_Toc` and a number, as word processors write them).
pub const TOC_BOOKMARK_PREFIX: &str = "_Toc";

/// The number of a table-of-contents bookmark name (`_Toc12` is 12): digits only, no leading zero, within
/// `u32`, so the number gives the same name back.
pub fn toc_bookmark_number(name: &str) -> Option<u32> {
    let digits = name.strip_prefix(TOC_BOOKMARK_PREFIX)?;
    let number = digits.parse::<u32>().ok().filter(|n| *n > 0)?;
    (number.to_string() == digits).then_some(number)
}

/// The hyperlink address when it is a plain web or mail target (`http`, `https`, `mailto`), else `None`
/// (fail-closed: `javascript:`, `file:`, `data:` and every other scheme are refused, as are whitespace,
/// control characters and addresses over [`MAX_LINK_CHARS`]) apart from the in-document `#_Toc<number>` bookmark target. The editor's `doc-link.js` applies the same rule.
pub fn safe_link_target(raw: &str) -> Option<&str> {
    let url = raw.trim();
    let unsafe_char = |c: char| c <= ' ' || ('\u{7f}'..='\u{9f}').contains(&c);
    if url.is_empty() || url.chars().count() > MAX_LINK_CHARS || url.chars().any(unsafe_char) {
        return None;
    }
    if let Some(name) = url.strip_prefix('#') {
        // The one in-document target: a table-of-contents bookmark (`#_Toc12`).
        return toc_bookmark_number(name).map(|_| url);
    }
    let (scheme, rest) = url.split_once(':')?;
    let has_target = match scheme.to_ascii_lowercase().as_str() {
        "http" | "https" => rest
            .strip_prefix("//")
            .is_some_and(|r| !r.is_empty() && !r.starts_with(['/', '?', '#'])),
        "mailto" => !rest.is_empty() && !rest.starts_with('?'),
        _ => false,
    };
    has_target.then_some(url)
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

/// Role of a paragraph inside a table-of-contents block (DOCX `w:sdt` "Table of Contents", ODT
/// `text:table-of-content`): the block's title line or an entry at a heading level (1–[`MAX_TOC_LEVEL`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RichToc {
    Title,
    Entry(u8),
}

/// Largest page number of a table-of-contents entry (seven digits).
pub const MAX_TOC_PAGE: u32 = 9_999_999;

/// Deepest entry level of a table-of-contents block (the word-processor styles `TOC 1`–`TOC 9`).
pub const MAX_TOC_LEVEL: u8 = 9;

/// A paragraph: runs plus paragraph properties. `heading_level` 0 is body text, 1–6 headings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichParagraph {
    pub runs: Vec<RichRun>,
    pub alignment: Option<RichAlignment>,
    pub heading_level: u8,
    pub list: Option<RichListKind>,
    /// Nesting level of a list paragraph, 0 for the outermost (read from Pages and DOCX; the DOCX
    /// and ODT writers emit it as `w:ilvl` and nested `text:list`).
    pub list_level: u8,
    /// Number format of an ordered list paragraph; `None` is the plain `1.` (read from Pages and
    /// DOCX; the DOCX and ODT writers emit it in `word/numbering.xml` and `text:list-style`).
    pub list_number: Option<RichListNumber>,
    /// Left indent in twips (1/20 point); `None` inherits.
    pub left_indent_twips: Option<u32>,
    /// First-line indent in twips; `None` inherits.
    pub first_line_indent_twips: Option<i32>,
    /// Line height as a percentage of single spacing (200 = double); `None` inherits.
    pub line_spacing_percent: Option<u16>,
    /// Set on the paragraphs of a generated table of contents; the writers wrap consecutive ones in the
    /// format's table-of-contents container and the readers recover the role from the style name.
    pub toc: Option<RichToc>,
    /// Number of the table-of-contents bookmark (`_Toc<number>`) that marks this heading; the entries of a
    /// table of contents link to it as `#_Toc<number>`.
    pub bookmark: Option<u32>,
    /// The paragraph starts a new page (DOCX `w:pageBreakBefore`, ODT `fo:break-before="page"`). A page break
    /// between two paragraphs of a file reads as this flag on the one after it.
    pub page_break_before: bool,
    /// Page number of a table-of-contents entry (at most [`MAX_TOC_PAGE`]); the writers put it after a right tab
    /// (DOCX `PAGEREF` field, ODT `text:index-entry-page-number`) and the readers split it off the entry text.
    pub toc_page: Option<u32>,
    /// Whether the tab before the page number is filled with dots (else it is blank).
    pub toc_dots: bool,
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
    /// Header and footer of every page.
    pub page_frame: RichPageFrame,
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
    /// Encodes a [`RichDocument`]. DOCX, HWPX/HWTX, ODT, RTF, Markdown, TXT, PDF, and Pages (body text
    /// and basic formats only) keep block
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
            DocumentFormat::Pdf => Ok(w::serialize_pdf_rich(doc, &PdfWriteOptions::default()).0),
            DocumentFormat::Pages => crate::pages_write::pack_pages(doc),
            _ => Self::encode_with_formula_cache(&doc.to_universal_model(), format, formula_cache),
        }
    }

    /// PDF encoding of a [`RichDocument`] with the page settings of the PDF Studio (rotation,
    /// watermark, approval stamp, highlight phrase); the default options give what
    /// [`Self::encode_rich`] gives. Also returns notes for the person saving: a highlight phrase
    /// that is not in the text, or occurrences left out over [`crate::MAX_HIGHLIGHT_ANNOTATIONS`].
    pub fn encode_rich_pdf(
        doc: &RichDocument,
        options: &PdfWriteOptions,
    ) -> (Vec<u8>, Vec<String>) {
        crate::rich_writers::serialize_pdf_rich(doc, options)
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
            page_frame: Default::default(),
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
