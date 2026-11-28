//! Ordered, formatted document model used by structured saves.
//!
//! A [`RichDocument`] is a sequence of blocks: paragraphs (formatted runs plus alignment, heading
//! level, and list kind) and tables at their position in the document. Run text is always literal;
//! no part of it is interpreted as markup. Formats with native run/paragraph formatting (DOCX,
//! HWPX, ODT, RTF, Markdown) are written by [`crate::rich_writers`]; every other format receives
//! the flattened [`UniversalDocumentModel`].

use crate::pdf_writer::PdfWriteOptions;
use crate::rich_image::RichImage;
pub use crate::rich_line_spacing::{RichLineSpacing, MAX_LINE_PERCENT, MAX_LINE_SPACING_TWIPS};
use crate::rich_notes::{
    is_deleted_run, validate_comments, validate_notes, RichComment, RichNote, RichNoteKind,
    RichRevision,
};
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
    Task,
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

impl RichListNumber {
    /// Canonical compact wire token (`"<format>:<enclosure>"`), zero heap allocation.
    pub const fn to_wire_code(self) -> &'static str {
        use RichNumberEnclosure::{Dot, Parentheses, RightParenthesis};
        use RichNumberFormat::{
            Decimal, HangulSyllable, LowerLatin, LowerRoman, UpperLatin, UpperRoman,
        };
        match (self.format, self.enclosure) {
            (Decimal, Dot) => "decimal:dot",
            (Decimal, Parentheses) => "decimal:paren",
            (Decimal, RightParenthesis) => "decimal:rparen",
            (LowerLatin, Dot) => "lowerLatin:dot",
            (LowerLatin, Parentheses) => "lowerLatin:paren",
            (LowerLatin, RightParenthesis) => "lowerLatin:rparen",
            (UpperLatin, Dot) => "upperLatin:dot",
            (UpperLatin, Parentheses) => "upperLatin:paren",
            (UpperLatin, RightParenthesis) => "upperLatin:rparen",
            (LowerRoman, Dot) => "lowerRoman:dot",
            (LowerRoman, Parentheses) => "lowerRoman:paren",
            (LowerRoman, RightParenthesis) => "lowerRoman:rparen",
            (UpperRoman, Dot) => "upperRoman:dot",
            (UpperRoman, Parentheses) => "upperRoman:paren",
            (UpperRoman, RightParenthesis) => "upperRoman:rparen",
            (HangulSyllable, Dot) => "hangul:dot",
            (HangulSyllable, Parentheses) => "hangul:paren",
            (HangulSyllable, RightParenthesis) => "hangul:rparen",
        }
    }

    /// Parses a wire token produced by [`Self::to_wire_code`].
    pub fn from_wire_code(code: &str) -> Option<Self> {
        let (fmt_s, enc_s) = code.split_once(':')?;
        let format = match fmt_s {
            "decimal" => RichNumberFormat::Decimal,
            "lowerLatin" => RichNumberFormat::LowerLatin,
            "upperLatin" => RichNumberFormat::UpperLatin,
            "lowerRoman" => RichNumberFormat::LowerRoman,
            "upperRoman" => RichNumberFormat::UpperRoman,
            "hangul" => RichNumberFormat::HangulSyllable,
            _ => return None,
        };
        let enclosure = match enc_s {
            "dot" => RichNumberEnclosure::Dot,
            "paren" => RichNumberEnclosure::Parentheses,
            "rparen" => RichNumberEnclosure::RightParenthesis,
            _ => return None,
        };
        Some(Self { format, enclosure })
    }
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
    /// Latin font family, also the East Asian one unless `font_east_asia` (DOCX `eastAsia`, RTF `\af`) names another.
    pub font: Option<String>,
    pub font_east_asia: Option<String>,
    /// PostScript name of the font variant (a [`crate::font_face`] name, PD-368); `.pages` writes it in place of `font`.
    pub font_face: Option<String>,
    /// Text color as six upper-case hex digits (`RRGGBB`, no `#`).
    pub color: Option<String>,
    /// Highlight/shading color as six upper-case hex digits.
    pub highlight: Option<String>,
    /// Superscript / subscript position.
    pub script: RichScript,
    /// Space added after each character, in twips (negative tightens, see [`crate::letter_spacing`]); `None` inherits.
    pub letter_spacing_twips: Option<i16>,
    pub advanced: crate::CharAdvanced,
    /// Hyperlink target the run belongs to (a [`safe_link_target`] address). The DOCX, ODT, HWPX and Pages
    /// writers emit external targets (`#_Toc<N>` internal anchors are counted by `pages_write_loss` in Pages).
    pub link: Option<String>,
    /// A field that the page fills in; `text` is empty. Only header and footer paragraphs carry one.
    pub field: Option<RichField>,
    /// Inline footnote or endnote reference (`(RichNoteKind, id)`).
    pub note_ref: Option<(RichNoteKind, u32)>,
    /// Comment anchor ID referencing [`RichComment::id`] in [`RichDocument::comments`].
    pub comment_id: Option<u32>,
    /// Tracked-change revision metadata (`Insert` or `Delete`) when tracked.
    pub revision: Option<RichRevision>,
    /// The instruction of the field whose result is `text` (`DATE`, `TIME`, `REF`, `SEQ`; [`safe_field_code`]). Runs that
    /// follow one another with the same instruction are one field; the writers that have no counterpart write the text.
    pub field_code: Option<String>,
    /// A bookmark ([`safe_bookmark_name`]) starts before this run's text.
    pub bookmark_start: Option<String>,
    /// The bookmark of that name ends after this run's text.
    pub bookmark_end: Option<String>,
    /// An equation: the checked OMML (`<m:oMath>` or `<m:oMathPara>`) that DOCX keeps as it was written; `text` is its
    /// linear text for the formats that have no equations (PD-401).
    pub math: Option<String>,
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

pub use crate::rich_page_setup::{
    RichBinding, RichBindingKind, RichPageMargins, RichPaper, MAX_PAGE_MARGIN_TWIPS,
    MIN_PAGE_BODY_TWIPS,
};
pub use crate::rich_word_features::{
    safe_bookmark_name, safe_field_code, RichLineNumberRestart, RichLineNumbers, RichProperties,
    RichSectionBreak,
};

/// Most paragraphs of one header or footer; a file with more keeps the first ones.
pub const MAX_PAGE_FRAME_PARAGRAPHS: usize = 64;

/// The header and footer drawn on pages: formatted paragraphs (runs, alignment, indents and line
/// spacing; headings, lists, links and tables are not kept). Optional first-page and even-page
/// variants are used when `first_page_different` or `even_odd_different` is set.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichPageFrame {
    pub header: Vec<RichParagraph>,
    pub footer: Vec<RichParagraph>,
    pub first_header: Vec<RichParagraph>,
    pub first_footer: Vec<RichParagraph>,
    pub even_header: Vec<RichParagraph>,
    pub even_footer: Vec<RichParagraph>,
    pub first_page_different: bool,
    pub even_odd_different: bool,
    /// The page setup of the first section: the distances of the body, header and footer from the page
    /// edges. `None` when the file gives none or gives one that does not fit its paper (the editor then uses
    /// its own default, a writer the format's).
    pub margins: Option<RichPageMargins>,
    /// The paper of the first section; `None` as above (A4 is written).
    pub paper: Option<RichPaper>,
    /// The binding of the first section (one-sided with no gutter when the file gives none).
    pub binding: RichBinding,
    /// Line numbers in the margin of every section (DOCX `w:lnNumType`).
    pub line_numbers: Option<RichLineNumbers>,
    /// The text watermark behind the text of every page (DOCX: a shape in the header, ODT: none).
    pub watermark: Option<String>,
}

impl RichPageFrame {
    pub fn is_empty(&self) -> bool {
        !self.has_any_header()
            && !self.has_any_footer()
            && !self.first_page_different
            && !self.even_odd_different
    }

    pub fn has_any_header(&self) -> bool {
        !self.header.is_empty() || !self.first_header.is_empty() || !self.even_header.is_empty()
    }

    pub fn has_any_footer(&self) -> bool {
        !self.footer.is_empty() || !self.first_footer.is_empty() || !self.even_footer.is_empty()
    }
}

/// Largest paragraph before/after spacing in twips (400 pt = 8,000 twips).
pub const MAX_PARA_SPACING_TWIPS: u32 = 8_000;

/// The paragraph as a header or footer keeps it: runs without links, alignment, indents and line spacing.
pub fn frame_paragraph(p: &RichParagraph) -> RichParagraph {
    RichParagraph {
        runs: p
            .runs
            .iter()
            .map(|r| RichRun {
                link: None,
                note_ref: None,
                comment_id: None,
                revision: None,
                ..r.clone()
            })
            .collect(),
        alignment: p.alignment,
        left_indent_twips: p.left_indent_twips,
        right_indent_twips: p.right_indent_twips,
        first_line_indent_twips: p.first_line_indent_twips,
        line_spacing: p.line_spacing,
        space_before_twips: p.space_before_twips,
        space_after_twips: p.space_after_twips,
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

/// Multi-column layout of a paragraph or section (`count >= 2`, 4-byte `Copy` value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RichColumns {
    pub count: u8,
    pub equal: bool,
    pub gap_twips: u16,
}

/// A paragraph: runs plus paragraph properties. `heading_level` 0 is body text, 1–6 headings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichParagraph {
    pub runs: Vec<RichRun>,
    pub alignment: Option<RichAlignment>,
    pub heading_level: u8,
    pub list: Option<RichListKind>,
    /// Completion state when `list == Some(RichListKind::Task)`.
    pub task_checked: bool,
    /// Nesting level of a list paragraph, 0 for the outermost (read from Pages and DOCX; the DOCX
    /// and ODT writers emit it as `w:ilvl` and nested `text:list`).
    pub list_level: u8,
    /// Number format of an ordered list paragraph; `None` is the plain `1.` (read from Pages and
    /// DOCX; the DOCX and ODT writers emit it in `word/numbering.xml` and `text:list-style`).
    pub list_number: Option<RichListNumber>,
    /// Left indent in twips (1/20 point); `None` inherits.
    pub left_indent_twips: Option<u32>,
    /// Right indent in twips (1/20 point); `None` inherits.
    pub right_indent_twips: Option<u32>,
    /// First-line indent in twips; `None` inherits.
    pub first_line_indent_twips: Option<i32>,
    /// How the lines are spaced (multiple, at least, exact, between lines); `None` inherits.
    pub line_spacing: Option<RichLineSpacing>,
    /// Spacing before paragraph in twips (`0..=8000`); `None` inherits.
    pub space_before_twips: Option<u32>,
    /// Spacing after paragraph in twips (`0..=8000`); `None` inherits.
    pub space_after_twips: Option<u32>,
    /// Multi-column layout in effect on this paragraph (`count > 1`); `None` for a single column.
    pub columns: Option<RichColumns>,
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
    /// The paragraph shows, after the body, text that the document also holds elsewhere (the header and footer of a Pages file).
    /// `Some("")` matches `page_frame`/appendix headings (legacy boolean flag), while non-empty `Some(orig)` matches the exact copy text read by the reader.
    pub frame_copy: Option<String>,
    /// The paragraph is a caption (DOCX style `Caption`) of a picture, table or figure.
    pub caption: bool,
    /// A section break follows this paragraph and the next section begins as [`RichSectionBreak`] says.
    pub section_break: Option<RichSectionBreak>,
    /// Tab stops, borders, shading, drop cap, column look, text boxes and the style of the user (PD-401).
    pub word: crate::rich_word_blocks::RichParagraphWord,
}

impl RichParagraph {
    pub fn plain_text(&self) -> String {
        self.runs
            .iter()
            .filter(|r| r.note_ref.is_none() && !is_deleted_run(r))
            .map(|r| r.text.as_str())
            .collect()
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
    /// Footnotes and endnotes attached to inline `note_ref` runs.
    pub notes: Vec<RichNote>,
    /// Review comments attached to runs (or point anchor runs) via `comment_id`.
    pub comments: Vec<RichComment>,
    /// Whether tracked changes (`Insert` / `Delete` revisions) are enabled on this document.
    pub track_changes: bool,
    /// Document-wide defaults of an opened file (PD-363); `None` for a new document, which gets the
    /// Korean defaults of PD-307 when it is written.
    pub defaults: Option<crate::RichDefaults>,
    /// Author, subject, keywords and comments of the document (the title is `title`).
    pub properties: RichProperties,
    /// The styles the user made (PD-401); a paragraph names one in `word.style_name`.
    pub styles: Vec<crate::rich_word_blocks::RichStyleDef>,
}

/// The headings of the header and footer appendix, and the prefixes it puts before first-page and even-page text.
const COPY_HEADINGS: [&str; 4] = ["머리글", "바닥글", "머리말", "꼬리말"];
const COPY_PREFIXES: [&str; 2] = ["[첫 쪽] ", "[짝수 쪽] "];

fn is_frame_text(text: &str, frame_texts: &std::collections::HashSet<String>) -> bool {
    let bare = COPY_PREFIXES
        .iter()
        .find_map(|prefix| text.strip_prefix(prefix))
        .unwrap_or(text);
    COPY_HEADINGS.contains(&text) || frame_texts.contains(bare)
}

impl RichDocument {
    /// Returns notes deduplicated by `(kind, id)` within document and text byte bounds.
    pub fn validated_notes(&self) -> Vec<RichNote> {
        validate_notes(&self.notes)
    }

    /// Returns comments deduplicated by `id` within document and field byte bounds.
    pub fn validated_comments(&self) -> Vec<RichComment> {
        validate_comments(&self.comments)
    }

    /// The blocks a writer that writes the page frame writes: the blocks without the header and footer copies a Pages reader
    /// appends. A [`RichParagraph::frame_copy`] paragraph is left out only while its text is still the frame's own (a header or
    /// footer paragraph, or the appendix heading); a paragraph the user changed or wrote is body text and is kept. A copy section
    /// runs from a skipped paragraph to the next paragraph that is not skipped.
    pub fn body_blocks(&self) -> impl Iterator<Item = &RichBlock> {
        let frame = &self.page_frame;
        let frame_texts: std::collections::HashSet<String> = [
            &frame.header,
            &frame.footer,
            &frame.first_header,
            &frame.first_footer,
            &frame.even_header,
            &frame.even_footer,
        ]
        .into_iter()
        .flatten()
        .map(RichParagraph::plain_text)
        .collect();
        let mut in_copy = false;
        self.blocks.iter().filter(move |block| {
            if let RichBlock::Paragraph(p) = block {
                in_copy = match p.frame_copy.as_deref() {
                    None => false,
                    Some("") => is_frame_text(&p.plain_text(), &frame_texts),
                    Some(orig) => p.plain_text() == orig,
                };
            }
            !in_copy
        })
    }

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

#[derive(Clone, Copy)]
enum TableFormulaFunc {
    Sum,
    Average,
    Count,
    Max,
    Min,
    Product,
}

#[derive(Clone, Copy)]
enum TableFormulaDir {
    Above,
    Below,
    Left,
    Right,
}

fn parse_table_formula(text: &str) -> Option<(TableFormulaFunc, TableFormulaDir)> {
    let body = text.trim().strip_prefix('=')?.trim();
    let open = body.find('(')?;
    let func_raw = body[..open].trim();
    let arg_raw = body[open + 1..].trim().strip_suffix(')')?.trim();
    let func = if func_raw.eq_ignore_ascii_case("SUM") {
        TableFormulaFunc::Sum
    } else if func_raw.eq_ignore_ascii_case("AVERAGE") {
        TableFormulaFunc::Average
    } else if func_raw.eq_ignore_ascii_case("COUNT") {
        TableFormulaFunc::Count
    } else if func_raw.eq_ignore_ascii_case("MAX") {
        TableFormulaFunc::Max
    } else if func_raw.eq_ignore_ascii_case("MIN") {
        TableFormulaFunc::Min
    } else if func_raw.eq_ignore_ascii_case("PRODUCT") {
        TableFormulaFunc::Product
    } else {
        return None;
    };
    let dir = if arg_raw.eq_ignore_ascii_case("ABOVE") {
        TableFormulaDir::Above
    } else if arg_raw.eq_ignore_ascii_case("BELOW") {
        TableFormulaDir::Below
    } else if arg_raw.eq_ignore_ascii_case("LEFT") {
        TableFormulaDir::Left
    } else if arg_raw.eq_ignore_ascii_case("RIGHT") {
        TableFormulaDir::Right
    } else {
        return None;
    };
    Some((func, dir))
}

fn parse_table_number(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let bytes = trimmed.as_bytes();
    if !bytes
        .iter()
        .any(|&b| b == b',' || b <= b' ' || !b.is_ascii())
    {
        return trimmed.parse::<f64>().ok().filter(|n| n.is_finite());
    }
    let mut stack = [0u8; 64];
    let mut len = 0usize;
    let mut overflow: Option<String> = None;
    for ch in trimmed.chars() {
        if ch == ',' || ch.is_whitespace() {
            continue;
        }
        if !ch.is_ascii() {
            return None;
        }
        if let Some(ref mut s) = overflow {
            s.push(ch);
        } else if len < stack.len() {
            stack[len] = ch as u8;
            len += 1;
        } else {
            let mut s = String::with_capacity(trimmed.len());
            s.push_str(std::str::from_utf8(&stack).unwrap_or(""));
            s.push(ch);
            overflow = Some(s);
        }
    }
    let slice = match overflow.as_deref() {
        Some(s) => s,
        None => {
            if len == 0 {
                return None;
            }
            std::str::from_utf8(&stack[..len]).ok()?
        }
    };
    slice.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// Formats a computed table value: integers without a fraction, otherwise up to 10 decimals.
pub fn format_table_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    let s = format!("{n:.10}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn fold_table_operands<'a, I: Iterator<Item = &'a str>>(func: TableFormulaFunc, cells: I) -> f64 {
    let mut count = 0usize;
    let mut sum = 0.0f64;
    let mut prod = 1.0f64;
    let mut extremum = match func {
        TableFormulaFunc::Max => f64::MIN,
        TableFormulaFunc::Min => f64::MAX,
        _ => 0.0,
    };
    for cell in cells {
        let Some(n) = parse_table_number(cell) else {
            continue;
        };
        count += 1;
        match func {
            TableFormulaFunc::Sum | TableFormulaFunc::Average => sum += n,
            TableFormulaFunc::Count => {}
            TableFormulaFunc::Max => extremum = extremum.max(n),
            TableFormulaFunc::Min => extremum = extremum.min(n),
            TableFormulaFunc::Product => prod *= n,
        }
    }
    if count == 0 {
        return 0.0;
    }
    match func {
        TableFormulaFunc::Sum => sum,
        TableFormulaFunc::Average => sum / count as f64,
        TableFormulaFunc::Count => count as f64,
        TableFormulaFunc::Max | TableFormulaFunc::Min => extremum,
        TableFormulaFunc::Product => prod,
    }
}

/// Cells one table's formulas may read in total (PD-262). A formula over `ABOVE` or `BELOW` reads a
/// whole column, so a long table of such formulas reads quadratically many cells; real tables stay far
/// below this (a 1,000-row running total reads half a million).
pub const MAX_TABLE_FORMULA_OPERANDS: usize = 4_000_000;

/// Replaces word-processor table formulas (`=SUM(ABOVE)`, `=AVERAGE(LEFT)`, … with SUM, AVERAGE,
/// COUNT, MAX, MIN, PRODUCT over ABOVE, BELOW, LEFT, RIGHT) by their computed values, row by row.
/// Non-numeric cells are skipped; unrecognized formulas are left as text, and so is every formula that
/// would read past the [`MAX_TABLE_FORMULA_OPERANDS`] budget of the table.
pub fn evaluate_table_formulas(rows: &mut [Vec<String>]) {
    let mut budget = MAX_TABLE_FORMULA_OPERANDS;
    for r in 0..rows.len() {
        for c in 0..rows[r].len() {
            let Some((func, dir)) = parse_table_formula(&rows[r][c]) else {
                continue;
            };
            let operands = match dir {
                TableFormulaDir::Above => r,
                TableFormulaDir::Below => rows.len() - r - 1,
                TableFormulaDir::Left => c,
                TableFormulaDir::Right => rows[r].len() - c - 1,
            };
            if operands > budget {
                continue;
            }
            budget -= operands;
            let value = match dir {
                TableFormulaDir::Above => fold_table_operands(
                    func,
                    (0..r).filter_map(|rr| rows[rr].get(c).map(String::as_str)),
                ),
                TableFormulaDir::Below => fold_table_operands(
                    func,
                    (r + 1..rows.len()).filter_map(|rr| rows[rr].get(c).map(String::as_str)),
                ),
                TableFormulaDir::Left => {
                    fold_table_operands(func, rows[r][..c].iter().map(String::as_str))
                }
                TableFormulaDir::Right => {
                    fold_table_operands(func, rows[r][c + 1..].iter().map(String::as_str))
                }
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

    /// [`Self::encode_rich`] with a preview picture in the file (PD-132, PD-239). The engine draws
    /// no pages, so `thumbnail_png` comes from the caller; `None`, or a format without a preview
    /// part (see [`crate::supports_thumbnail`]), gives exactly what [`Self::encode_rich`] gives.
    /// `DOCX`, `ODT`, and `HWPX`/`HWTX` insert the preview before packing the ZIP so the archive
    /// is compressed once without an unpack/repack pass. A picture that is not a valid PNG within
    /// the limits is an error.
    pub fn encode_rich_with_thumbnail(
        doc: &RichDocument,
        format: DocumentFormat,
        formula_cache: Option<&XlsxFormulaCache>,
        thumbnail_png: Option<&[u8]>,
    ) -> Result<Vec<u8>, EasyOfficeError> {
        let Some(png) = thumbnail_png.filter(|_| crate::supports_thumbnail(format)) else {
            return Self::encode_rich(doc, format, formula_cache);
        };
        crate::check_thumbnail_png(png)?;
        let entries = match format {
            DocumentFormat::Docx => Some(crate::rich_writers::docx_rich_entries(doc)?),
            DocumentFormat::Odt => Some(crate::odt_rich_writer::odt_rich_entries(doc)?),
            DocumentFormat::Hwpx | DocumentFormat::Hwtx => Some(HwpxCodec::pack_rich_entries(
                doc,
                format == DocumentFormat::Hwtx,
            )?),
            _ => None,
        };
        if let Some(mut entries) = entries {
            crate::embedded_thumbnail::embed_thumbnail_entries(format, &mut entries, png)?;
            return crate::ZipArchiveCodec::pack_compressed(&entries);
        }
        let bytes = Self::encode_rich(doc, format, formula_cache)?;
        crate::embed_thumbnail(format, &bytes, png)
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
            ..RichDocument::default()
        };
        let model = doc.to_universal_model();
        assert_eq!(model.paragraphs, vec!["가나".to_string()]);
        assert_eq!(model.tables.len(), 1);
        assert_eq!(model.official_agency, None);
    }
}
