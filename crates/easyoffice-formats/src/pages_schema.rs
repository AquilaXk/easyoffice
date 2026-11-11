//! The ONLY place that knows Pages object type IDs and protobuf field numbers.
//!
//! These numbers are our own constants for the wire format observed in documents and described by
//! public format documentation; no Apple `.proto` file or third-party schema text is copied.
//! Every constant is either confirmed against a document ("확인") or carries the marker
//! `// 검증 필요` (unconfirmed). To correct the mapping, edit only this file.
//!
//! Where the independent public implementation (MIT) and our fixtures disagree, its values are the
//! default and ours stay as a second place that is read as well (QA 2026-11-11, "독립 구현 기준,
//! 실파일 대조 전"): the fixtures are Pages templates with objects added by other tools, so a
//! "확인" that rests on them alone only says that this reader reads those files. Layouts that the
//! independent notes do not know (compact styles, tile type 6003, version-1 cells, footnote type
//! 2012 in table 11, comment type 2033 in table 19) are marked 검증 필요 for that reason.
//! Confirmation basis (see `docs/policy-decisions.md` PD-087):
//! 1. one document created with the web edition (file format version 184): bold/underline
//!    character styles, heading/body paragraph styles, list styles, 14 tables of rich-text cells
//!    stored in the "version 5" packed cell layout (version byte 5, flags word at byte 8);
//! 2. three documents `tests/fixtures/pages/eo-pages-{text,lists-tables,objects}.pages` made for
//!    this project: every character/paragraph property, bullet and numbered lists, text, number,
//!    date, currency, percent, checkbox, formula and rich-text cells (packed cell layout with
//!    version byte 1 and the flags word at byte 4), an image, shapes, a footnote, a header/footer
//!    and a comment.

/// Object type IDs (`MessageInfo.type`).
pub(crate) mod ty {
    /// Document root. 확인
    pub const DOCUMENT_ROOT: u32 = 10000;
    /// Text storage (body and every table-cell text). 확인
    pub const STORAGE: u32 = 2001;
    /// Drawable attachment wrapper placed at U+FFFC characters. 확인
    pub const ATTACHMENT: u32 = 2003;
    /// Paragraph style. 확인
    pub const PARA_STYLE: u32 = 2022;
    /// List style. 확인
    pub const LIST_STYLE: u32 = 2023;
    /// Table placed in the document (table info). 확인
    pub const TABLE_INFO: u32 = 6000;
    /// Table model. 확인
    pub const TABLE_MODEL: u32 = 6001;
    /// Table tile (rows of packed cells), web edition. 확인
    pub const TABLE_TILE: u32 = 6002;
    /// Table tile, fixture documents (same message shape). 검증 필요: fixtures only
    pub const TABLE_TILE_V2: u32 = 6003;
    /// Table data list (strings, rich text payloads, styles, ...). 확인
    pub const TABLE_DATA_LIST: u32 = 6005;
    /// Rich text payload of a table cell (points at a text storage). 확인
    pub const RICH_TEXT_PAYLOAD: u32 = 6218;
    /// Picture (image drawable). 확인 (fixture objects file)
    pub const IMAGE: u32 = 3005;
    /// Shape or text box that carries its own text storage. 확인 (fixture objects file)
    pub const SHAPE_WITH_TEXT: u32 = 2011;
    /// Header/footer text lists (`1` = header storages, `2` = footer storages). 확인
    pub const HEADER_FOOTER_LIST: u32 = 10143;
    /// Footnote (points at the footnote text storage). 검증 필요: fixtures only (newer files use
    /// [`FOOTNOTE_ATTACHMENT`])
    pub const FOOTNOTE: u32 = 2012;
    /// Package metadata (list of data files with identifiers and names). 확인 (fixture)
    pub const PACKAGE_METADATA: u32 = 11006;
    /// Document settings (body switch, note settings). 확인 as fields (fixtures)
    pub const SETTINGS: u32 = 10012;
    /// Drawables floating on pages (page-layout documents). 검증 필요: field layout is from a
    /// public independent implementation's notes; the fixtures only carry an empty list.
    pub const FLOATING_DRAWABLES: u32 = 10010;
    /// Footnote reference attachment of newer files (the same two fields as [`FOOTNOTE`]: the
    /// text storage reference is field 2). 검증 필요: public notes only; the fixtures use 2012.
    pub const FOOTNOTE_ATTACHMENT: u32 = 2008;
    /// Column style (columns live in its properties, see [`super::column`]). 확인 (fixtures)
    pub const COLUMN_STYLE: u32 = 2024;
    /// Hyperlink field: field 2 is the address. 검증 필요: public notes only; no fixture link.
    pub const HYPERLINK: u32 = 2032;
    /// Comment text objects: the fixtures' (3059) and newer files' (3056). Field 1 is the text.
    pub const COMMENT_TEXT: u32 = 3056;
    pub const COMMENT_TEXT_V2: u32 = 3059;
    /// Comment anchor field of the fixtures (points at the comment text, type 3059). 확인
    pub const COMMENT_FIELD: u32 = 2033;
    /// Date/time field (the text it last rendered stays in the storage). 검증 필요
    pub const DATE_FIELD: u32 = 2034;
    /// Bookmark field. 검증 필요
    pub const BOOKMARK: u32 = 2035;
    /// Placeholder, merge and ruby fields. 검증 필요
    pub const PLACEHOLDER_FIELD: u32 = 2031;
    pub const MERGE_FIELD: u32 = 2036;
    pub const RUBY_FIELD: u32 = 2042;
    /// Highlight (comment anchor of newer files). 검증 필요
    pub const HIGHLIGHT: u32 = 2013;
    pub const COMMENT_INFO: u32 = 2014;
    /// Tracked change (insertion or deletion). 검증 필요
    pub const CHANGE: u32 = 2060;
    /// Page-number / page-count attachment. 검증 필요
    pub const NUMBER_ATTACHMENT: u32 = 2043;
    /// Table of contents drawable. 검증 필요
    pub const TOC: u32 = 2240;
    /// Bare shape, movie, group and connection line drawables. 검증 필요
    pub const SHAPE: u32 = 3004;
    pub const MOVIE: u32 = 3007;
    pub const GROUP: u32 = 3008;
    pub const CONNECTION_LINE: u32 = 3009;
    /// Chart drawable. 검증 필요
    pub const CHART: u32 = 5021;
}

/// Hyperlink field. 검증 필요: public notes only; no fixture link.
pub(crate) mod hyperlink {
    /// The address string.
    pub const URL: u32 = 2;
    /// Address prefixes that are imported (anything else, `javascript:` and local paths
    /// included, is dropped and counted).
    pub const ALLOWED_SCHEMES: [&str; 5] = ["http://", "https://", "mailto:", "tel:", "ftp://"];
    /// Longest accepted address.
    pub const MAX_URL_BYTES: usize = 4096;
}

/// Document settings (`TP.SettingsArchive`, type 10012). `BODY` and the note settings are 확인 as
/// fields against the fixtures (`1 = 1`, `30..33 = 0, 0, 0, 10`); every non-default value is
/// 검증 필요 (public notes only).
pub(crate) mod settings {
    /// Word-processing (true) or page-layout (false) document.
    pub const BODY: u32 = 1;
    /// 0 footnotes, 1 document endnotes, 2 section endnotes.
    pub const NOTE_KIND: u32 = 30;
    /// 0 numeric, 1 roman, 2 symbolic, 3 Japanese numeric, 4 Japanese ideographic, 5 Arabic.
    pub const NOTE_FORMAT: u32 = 31;
    pub const FORMAT_ROMAN: u64 = 1;
    pub const FORMAT_SYMBOLIC: u64 = 2;
    pub const FORMAT_JAPANESE_NUMERIC: u64 = 3;
    pub const FORMAT_JAPANESE_IDEOGRAPHIC: u64 = 4;
    /// 0 continuous, 1 restart each page, 2 restart each section.
    pub const NOTE_NUMBERING: u32 = 32;
    pub const NUMBERING_PER_PAGE: u64 = 1;
    pub const NUMBERING_PER_SECTION: u64 = 2;
}

/// Column style properties. 확인 for a single column against the fixtures (`11.7.1.1 = 1`);
/// the unequal layout is 검증 필요 (public notes only).
pub(crate) mod column {
    /// Properties message of a column style.
    pub const PROPS: u32 = 11;
    /// Columns message inside the properties.
    pub const COLUMNS: u32 = 7;
    /// Equal columns `{ count = 1, gap = 2 }`.
    pub const EQUAL: u32 = 1;
    pub const EQUAL_COUNT: u32 = 1;
    /// Unequal columns `{ first = 1, repeated { gap, width } = 2 }`.
    pub const UNEQUAL: u32 = 2;
    pub const UNEQUAL_FOLLOWING: u32 = 2;
}

/// Shape / text box field numbers. 확인 (fixture objects file)
pub(crate) mod shape {
    /// Reference `{ identifier = 1 }` to the shape's text storage.
    pub const TEXT_STORAGE: u32 = 2;
}

/// Footnote field numbers. 확인 (fixture objects file)
pub(crate) mod footnote {
    /// Reference `{ identifier = 1 }` to the footnote text storage.
    pub const TEXT_STORAGE: u32 = 2;
}

/// Picture drawable field numbers. 확인 (fixture objects file)
pub(crate) mod image {
    /// Drawable super message (geometry etc.).
    pub const DRAWABLE: u32 = 1;
    /// Mask reference (crop window). 검증 필요: public notes only; absent in the fixtures.
    pub const MASK: u32 = 5;
    /// Data reference `{ identifier = 1 }` of the picture file.
    pub const DATA: u32 = 11;
}

/// Drawable field numbers.
pub(crate) mod drawable {
    /// Geometry message (position, size, flags, angle).
    pub const GEOMETRY: u32 = 1;
    /// Accessibility description (alternative text). 검증 필요: public notes only; the
    /// fixtures carry no description.
    pub const DESCRIPTION: u32 = 8;
}

/// Floating-drawables list field numbers. 검증 필요 (see [`ty::FLOATING_DRAWABLES`]).
pub(crate) mod floating {
    /// Repeated page groups.
    pub const PAGE_GROUPS: u32 = 1;
    /// Page index of a group (zero based).
    pub const GROUP_PAGE: u32 = 1;
    /// Repeated `{ drawable reference = 1 }` entries inside a page group.
    pub const GROUP_DRAWABLES: u32 = 4;
}

/// Geometry field numbers.
pub(crate) mod geometry {
    /// Position message `{ x = 1, y = 2 }` as 32-bit floats in points.
    pub const POSITION: u32 = 1;
    /// Size message `{ width = 1, height = 2 }` as 32-bit floats in points.
    pub const SIZE: u32 = 2;
}

/// Package metadata field numbers.
pub(crate) mod package_metadata {
    /// Repeated data file descriptions.
    pub const DATA_INFOS: u32 = 4;
}

/// Data file description field numbers.
pub(crate) mod data_info {
    pub const IDENTIFIER: u32 = 1;
    pub const PREFERRED_NAME: u32 = 3;
    pub const FILE_NAME: u32 = 4;
}

/// Field numbers of the document root.
pub(crate) mod root {
    /// Reference to the body text storage. 확인
    pub const BODY_STORAGE: u32 = 4;
}

/// Field numbers of a text storage.
pub(crate) mod storage {
    /// Text (UTF-8 string; paragraphs separated by `\n`). 확인
    pub const TEXT: u32 = 3;
    /// Paragraph style attribute table (`entries = 1 { index = 1, object = 2 }`). 확인
    pub const PARA_STYLE_TABLE: u32 = 5;
    /// List level attribute table (`entries = 1 { index = 1, level = 2 }`). 확인 (fixture lists)
    pub const LIST_LEVEL_TABLE: u32 = 6;
    /// Level entry: nesting level. 확인
    pub const ENTRY_LEVEL: u32 = 2;
    /// List style attribute table (same shape as the paragraph table). 확인 (applied to bullets
    /// and numbered lists in the fixture lists file)
    pub const LIST_STYLE_TABLE: u32 = 7;
    /// Character style attribute table (same shape; an entry without object resets). 확인
    pub const CHAR_STYLE_TABLE: u32 = 8;
    /// Attachment attribute table (index of each U+FFFC to an attachment). 확인
    pub const ATTACHMENT_TABLE: u32 = 9;
    /// Footnote attribute table (index of the reference mark to a footnote, type 2012). 확인
    pub const FOOTNOTE_TABLE: u32 = 11;
    /// Smart field table of newer files (hyperlinks, date fields), and the table the fixtures use
    /// for comment anchors (type 2033) where newer files keep language strings. 검증 필요
    pub const SMARTFIELD_TABLE: u32 = 11;
    pub const LANGUAGE_OR_FIELD_TABLE: u32 = 19;
    /// Footnote table of newer files (the mark sits on U+000E); the fixtures use
    /// [`FOOTNOTE_TABLE`]. 검증 필요: public notes only.
    pub const FOOTNOTE_TABLE_V2: u32 = 16;
    /// Column layout style table (paragraph anchored). 확인
    pub const LAYOUT_STYLE_TABLE: u32 = 12;
    /// Run tables whose entries point at smart fields, bookmarks, tracked changes and comment
    /// anchors. The same object kinds sit in different fields in different file versions (the
    /// fixtures put footnotes in 11 and comments (type 2033) in 19; public notes put hyperlinks in 11 and
    /// comments in 23), so entries are classified by the type of the object they point at.
    pub const MARK_TABLES: [u32; 7] = [11, 15, 16, 19, 21, 22, 23];
    /// Section table of the body (entries point at section objects; a section starts at the
    /// character after its U+0004 break). 확인 (one entry in every fixture)
    pub const SECTION_TABLE: u32 = 17;
    /// Tracked insertion and deletion tables (entries point at a change object).
    pub const INSERTION_TABLE: u32 = 21;
    pub const DELETION_TABLE: u32 = 22;
    /// Footnote mark character of newer files.
    pub const FOOTNOTE_MARK_CHAR: char = '\u{E}';
    /// Attribute table entry: character index (UTF-16 code units). 확인 (BMP and non-BMP: the
    /// fixture text file places a bold run after two emoji)
    pub const ENTRY_INDEX: u32 = 1;
    /// Attribute table entries (repeated). 확인
    pub const TABLE_ENTRIES: u32 = 1;
    /// Attribute table entry: object reference message. 확인
    pub const ENTRY_OBJECT: u32 = 2;
}

/// Field numbers shared by paragraph, character and list styles.
pub(crate) mod style {
    /// Style header (name, identifier, parent). 확인
    pub const HEADER: u32 = 1;
    /// Header: identifier string such as `...-paragraphstyle-Heading 1`. 확인
    pub const HEADER_IDENTIFIER: u32 = 2;
    /// Header: parent style reference. 확인
    pub const HEADER_PARENT: u32 = 3;
    /// Character properties (in paragraph and character styles). 확인
    pub const CHAR_PROPS: u32 = 11;
    /// Paragraph properties (paragraph styles only). 확인
    pub const PARA_PROPS: u32 = 12;
    /// Character properties of a compact style object that has no field 11. 확인 (fixtures)
    pub const COMPACT_CHAR_PROPS: u32 = 2;
    /// Paragraph properties of a compact style object that has no field 12. 확인 (fixtures)
    pub const COMPACT_PARA_PROPS: u32 = 3;
    /// List style: repeated per-level list type. 확인
    pub const LIST_LEVEL_TYPES: u32 = 11;
    /// List style: repeated per-level number format (0 `1.`, 1 `(1)`, 2 `1)`, 3-5 upper-case
    /// roman, 6-8 lower-case roman, 9-14 letters, ...). 검증 필요: public notes only.
    pub const LIST_LEVEL_NUMBER_FORMATS: u32 = 15;
}

/// Field numbers of one character property layout.
pub(crate) struct CharPropFields {
    pub bold: u32,
    pub italic: u32,
    pub underline: u32,
    pub strike: u32,
    pub size_pt: u32,
    pub color: u32,
    pub highlight: Option<u32>,
    /// Font name (string).
    pub font: Option<u32>,
    /// Baseline enum: 1 superscript, 2 subscript.
    pub script: Option<u32>,
    /// Second place for the font name (string) when `font` holds something else.
    pub font_alt: Option<u32>,
    /// Second place for the text background color message.
    pub highlight_alt: Option<u32>,
    /// Text fill message whose field 1 is the color that is drawn (it wins over `color`).
    pub fill: Option<u32>,
}

/// Full layout (`style::CHAR_PROPS`): bold (varint bool) 1, italic 2 (검증 필요: only ever 0 in
/// the fixtures), size in points (fixed32 float) 3, color message 7, script 10, underline 11
/// (확인: the hyperlink character style), strikethrough 12 (검증 필요: only ever 0 in the
/// fixtures), font name 16, highlight 19.
pub(crate) const FULL_CHAR_FIELDS: CharPropFields = CharPropFields {
    bold: 1,
    italic: 2,
    underline: 11, // 확인
    strike: 12,    // 검증 필요
    size_pt: 3,
    color: 7,
    highlight: Some(19),
    font: Some(16),
    script: Some(10),
    // 검증 필요: independent notes put the PostScript font name in 5, the text background color
    // in 26 and the drawn text color in the fill 46.1 of the same property message (our 16 and
    // 19 come from one web-edition document). Both places are read.
    font_alt: Some(5),
    highlight_alt: Some(26),
    fill: Some(46),
};

/// Compact layout (`style::COMPACT_CHAR_PROPS`), every field matches the fixture text file (검증 필요:
/// the independent notes do not know this layout):
/// bold 1, italic 2, underline 3, strikethrough 4, size 6 (float), color 7, highlight 19.
pub(crate) const COMPACT_CHAR_FIELDS: CharPropFields = CharPropFields {
    bold: 1,
    italic: 2,
    underline: 3,
    strike: 4,
    size_pt: 6,
    color: 7,
    highlight: Some(19),
    font: Some(5),    // 확인: font name string (fixture text file)
    script: Some(10), // 확인: 1 superscript, 2 subscript
    font_alt: None,
    highlight_alt: None,
    fill: None,
};

/// Character property message of a style object and the field layout that applies to it: the
/// full layout under field 11, else the compact layout under field 2.
pub(crate) fn char_props_layout<'a>(
    style_msg: &crate::pages_proto::Message<'a>,
) -> Option<(crate::pages_proto::Message<'a>, &'static CharPropFields)> {
    if let Ok(Some(p)) = style_msg.sub(style::CHAR_PROPS) {
        return Some((p, &FULL_CHAR_FIELDS));
    }
    let p = style_msg.sub(style::COMPACT_CHAR_PROPS).ok().flatten()?;
    Some((p, &COMPACT_CHAR_FIELDS))
}

/// Color message fields: red, green, blue, alpha (fixed32 floats 0..1). 확인
pub(crate) const COLOR_RGBA: [u32; 4] = [3, 4, 5, 6];

/// Paragraph property field numbers and enum values (both layouts).
pub(crate) mod para_props {
    /// Alignment enum. 0 left, 1 right, 2 center, 3 justify all 확인 (fixture text file); 4
    /// (natural) = no alignment specified
    pub const ALIGNMENT: u32 = 1;
    pub const ALIGN_LEFT: u64 = 0;
    pub const ALIGN_RIGHT: u64 = 1;
    pub const ALIGN_CENTER: u64 = 2;
    pub const ALIGN_JUSTIFY: u64 = 3;
    /// First-line indent in points (`style::PARA_PROPS`).
    pub const FIRST_LINE_INDENT: u32 = 13;
    /// Left indent in points (`style::PARA_PROPS`).
    pub const LEFT_INDENT: u32 = 16;
    /// Line spacing message (`style::PARA_PROPS`).
    pub const LINE_SPACING: u32 = 19;
}

/// Compact paragraph property layout (`style::COMPACT_PARA_PROPS`), matches the fixture text (검증 필요:
/// not in the independent notes)
/// file: first-line indent 6 (float, points), left indent 7 (float, points), line spacing 8
/// (`{ mode = 1, amount = 2 }`, mode 0 = multiple of single spacing).
pub(crate) mod compact_para {
    pub const FIRST_LINE_INDENT: u32 = 6;
    pub const LEFT_INDENT: u32 = 7;
    pub const LINE_SPACING: u32 = 8;
    pub const SPACING_MODE: u32 = 1;
    pub const SPACING_AMOUNT: u32 = 2;
    pub const SPACING_MULTIPLE: u64 = 0;
}

/// Field numbers of one paragraph property layout.
pub(crate) struct ParaPropFields {
    pub first_line_indent: u32,
    pub left_indent: u32,
    pub line_spacing: u32,
}

pub(crate) const FULL_PARA_FIELDS: ParaPropFields = ParaPropFields {
    first_line_indent: para_props::FIRST_LINE_INDENT,
    left_indent: para_props::LEFT_INDENT,
    line_spacing: para_props::LINE_SPACING,
};

/// Paragraph properties of the full layout according to independent notes (검증 필요): first-line
/// indent 7, left indent 11, line spacing 13 (`{ mode = 1, amount = 2 }`). Read after
/// [`FULL_PARA_FIELDS`] so that either layout fills what the other leaves unset.
pub(crate) const FULL_PARA_FIELDS_ALT: ParaPropFields = ParaPropFields {
    first_line_indent: 7,
    left_indent: 11,
    line_spacing: 13,
};

pub(crate) const COMPACT_PARA_FIELDS: ParaPropFields = ParaPropFields {
    first_line_indent: compact_para::FIRST_LINE_INDENT,
    left_indent: compact_para::LEFT_INDENT,
    line_spacing: compact_para::LINE_SPACING,
};

/// Baseline values of the character `script` field.
pub(crate) mod baseline {
    pub const SUPERSCRIPT: u64 = 1;
    pub const SUBSCRIPT: u64 = 2;
}

/// List level types (`style::LIST_LEVEL_TYPES` values). 0 none, bullet and number 확인
pub(crate) mod list_type {
    pub const IMAGE: u64 = 1; // 검증 필요: no picture bullets in the fixtures
    pub const BULLET: u64 = 2;
    pub const NUMBER: u64 = 3;
}

/// Substrings of a paragraph style identifier that mark headings. 확인 (heading 1-4, title,
/// caption and footnote identifiers observed)
pub(crate) mod heading {
    pub const HEADING_PREFIX: &str = "paragraphstyle-Heading ";
    pub const TITLE_MARKER: &str = "paragraphstyle-Title";
}

/// Attachment field numbers.
pub(crate) mod attachment {
    /// Reference to the drawable (table info, image, shape...). 확인
    pub const DRAWABLE: u32 = 1;
}

/// Header/footer list field numbers.
pub(crate) mod header_footer {
    /// Header text storages. 확인
    pub const HEADERS: u32 = 1;
    /// Footer text storages. 확인
    pub const FOOTERS: u32 = 2;
}

/// Table field numbers.
pub(crate) mod table {
    /// Table info: reference to the model. 확인
    pub const INFO_MODEL: u32 = 2;
    /// Model: number of rows. 확인
    pub const MODEL_ROWS: u32 = 6;
    /// Model: number of columns. 확인
    pub const MODEL_COLS: u32 = 7;
    /// Model: data store message. 확인
    pub const MODEL_DATA_STORE: u32 = 4;
    /// Data store: tile storage. 확인
    pub const STORE_TILES: u32 = 3;
    /// Data store: rich text payload list reference. 확인 (the list's own type number differs
    /// between documents, so it is not checked)
    pub const STORE_RICH_TEXT_LIST: u32 = 17;
    /// Data store: cell format list reference. 확인 (fixture lists file)
    pub const STORE_FORMAT_LIST: u32 = 22;
    /// Tile storage: tile entries (`{ tile id = 1, tile reference = 2 }`). 확인
    pub const TILES_ENTRIES: u32 = 1;
    pub const TILE_ID: u32 = 1;
    pub const TILE_REF: u32 = 2;
    /// Tile storage: rows per tile. 확인
    pub const TILES_SIZE: u32 = 2;
    pub const DEFAULT_TILE_ROWS: u64 = 256;
    /// Tile: repeated row info. 확인
    pub const TILE_ROWS: u32 = 5;
    /// Row info: row index in tile. 확인
    pub const ROW_INDEX: u32 = 1;
    /// Row info: packed cell buffer. 확인
    pub const ROW_CELL_BUFFER: u32 = 6;
    /// Row info: u16 LE cell offsets per column; 0xFFFF means no cell. 확인
    pub const ROW_CELL_OFFSETS: u32 = 7;
    pub const NO_CELL_OFFSET: u16 = 0xFFFF;
    /// Data list: entries. 확인
    pub const LIST_ENTRIES: u32 = 3;
    /// Data list entry: key. 확인
    pub const ENTRY_KEY: u32 = 1;
    /// Rich text list entry: payload reference. 확인
    pub const ENTRY_RICH_TEXT: u32 = 9;
    /// Format list entry: format message. 확인 (fixture lists file)
    pub const ENTRY_FORMAT: u32 = 6;
    /// Data store: string list reference, entry text = field 3. 확인 (Pages 14.4 / file format
    /// 26.4.0 documents with string and error cells)
    pub const STORE_STRING_LIST: u32 = 4;
    pub const ENTRY_STRING: u32 = 3;
    /// Rich text payload: storage reference. 확인
    pub const PAYLOAD_STORAGE: u32 = 1;
    /// Data store: merge region map reference (an object of type 6007 with repeated entries).
    /// 확인: the lists-tables and cellkinds fixtures carry merged cells in this form.
    pub const STORE_MERGE_REGION_MAP: u32 = 13;
    /// Merge region map: repeated entry. 확인
    pub const MERGE_RANGES: u32 = 1;
    /// Entry: origin and size, each a varint packing `row << 16 | column` (the size packs
    /// `rows << 16 | columns`). 확인 (row in the high half, checked against the cell grids of
    /// both fixtures; the public notes' column-high fixed32 layout does not occur).
    pub const RANGE_ORIGIN: u32 = 1;
    pub const RANGE_SIZE: u32 = 2;
    /// Model: merge owner message whose formula store keeps one formula per merged range
    /// (newer file versions). 검증 필요: public `iwork` notes only, absent from the fixtures.
    pub const MODEL_MERGE_OWNER: u32 = 47;
    /// Owner: formula store `{ pairs = 3 { index = 1, formula = 2 } }`; formula: nodes `1 { 1 }`.
    pub const OWNER_FORMULA_STORE: u32 = 2;
    pub const OWNER_STORE_PAIRS: u32 = 3;
    pub const OWNER_PAIR_FORMULA: u32 = 2;
    pub const FORMULA_NODES: u32 = 1;
    pub const NODE_LIST: u32 = 1;
    /// Node kind (varint), colon tract (67) with its tract in field 40 (`columns = 3`,
    /// `rows = 4`, each `{ begin = 1, end = 2 }`) and single cell reference (36) with zigzag
    /// column field 26 and row field 27 (each `{ 1 }`).
    pub const NODE_KIND: u32 = 1;
    pub const NODE_COLON_TRACT: u32 = 40;
    pub const TRACT_COLUMNS: u32 = 3;
    pub const TRACT_ROWS: u32 = 4;
    pub const TRACT_BEGIN: u32 = 1;
    pub const TRACT_END: u32 = 2;
    pub const NODE_COLUMN: u32 = 26;
    pub const NODE_ROW: u32 = 27;
}

/// Cell format message fields and format kinds (fixture lists file). Date formats (kind 261)
/// are recognised by their pattern string alone.
pub(crate) mod cell_format {
    /// Format kind enum.
    pub const KIND: u32 = 1;
    /// Date pattern string such as `yyyy-MM-dd`. 확인
    pub const PATTERN: u32 = 2;
    /// Number of decimal places. 확인 (matches the stored values)
    pub const DECIMALS: u32 = 3;
    /// Currency code such as `KRW`. 확인
    pub const CURRENCY: u32 = 4;
    /// Thousands separator switch. 확인 (field 5 in both layouts; on and off cells compared in
    /// the Pages 14.4 fixtures)
    pub const THOUSANDS: u32 = 5;
    /// Decimal places of the newer layout (Pages 14.4): a varint in field 2 where the older
    /// layout uses field 3. Field 3 of a newer currency entry is the currency code string.
    pub const DECIMALS_V2: u32 = 2;
    /// Decimal-place values above this mean "automatic" (not a count). 검증 필요
    pub const MAX_DECIMALS: u64 = 20;
    /// Currency code string of the newer layout. 확인
    pub const CURRENCY_V2: u32 = 3;
    /// Duration format: style (1 in the fixtures), largest and smallest unit. 확인 (units are
    /// matched against the cell values of the fixtures)
    pub const DURATION_STYLE: u32 = 7;
    pub const DURATION_LARGEST: u32 = 15;
    pub const DURATION_SMALLEST: u32 = 16;
    pub const KIND_DURATION: u64 = 268;
    /// Style number whose display form is verified ("1h 30m").
    pub const DURATION_STYLE_COMPACT: u64 = 1;
    /// Duration unit codes. 확인 for day, hour and minute; week and second follow the same
    /// power-of-two order (검증 필요).
    pub const UNIT_WEEK: u64 = 1;
    pub const UNIT_DAY: u64 = 2;
    pub const UNIT_HOUR: u64 = 4;
    pub const UNIT_MINUTE: u64 = 8;
    pub const UNIT_SECOND: u64 = 16;
    /// Currency format. 확인
    pub const KIND_CURRENCY: u64 = 256;
    /// Number format. 확인
    pub const KIND_NUMBER: u64 = 257;
    /// Percent format. 확인
    pub const KIND_PERCENT: u64 = 258;
    /// Checkbox control format. 확인
    pub const KIND_CHECKBOX: u64 = 264;
}

/// Packed cell layouts.
pub(crate) mod cell {
    /// Web-edition layout: version byte 5, type byte, flags word at byte 8, 4-byte fields from
    /// byte 12 in flag-bit order. 확인 (web document); only rich text cells are read.
    pub const LEGACY_VERSION: u8 = 5;
    pub const LEGACY_FLAGS_OFFSET: usize = 8;
    pub const LEGACY_FIELDS_OFFSET: usize = 12;
    /// Legacy flag bit whose field is the rich text list key. 확인 (key order matches list keys)
    pub const LEGACY_FLAG_RICH_TEXT_KEY: u32 = 0x10;
    /// Version-5 layout of Pages 14.4 (file format 26.4.0) number, string, duration and error
    /// cells: one 4-byte (or larger, see the sizes below) field per set flag bit, in bit order,
    /// from byte 12. 확인 (the field sizes add up to the cell lengths of the fixtures).
    pub const LEGACY_BIT_DECIMAL: u32 = 0;
    pub const LEGACY_BIT_DOUBLE: u32 = 1;
    pub const LEGACY_BIT_STRING_KEY: u32 = 3;
    pub const LEGACY_BIT_RICH_TEXT_KEY: u32 = 4;
    /// Date value (a double of seconds since 2001-01-01) of a version-5 cell. 검증 필요: public
    /// notes only (8 bytes); the fixtures hold no version-5 date cell.
    pub const LEGACY_BIT_DATE: u32 = 2;
    /// Flag bits whose 4-byte fields are present in the fixtures: cell style, text style,
    /// formula, formula error, suggestion, and the number, currency, duration and text formats.
    /// Bits 7, 8, 10, 15 and 18-20 (conditional style and rule, control spec, date and boolean
    /// format, comment, import warning) are 4 bytes according to public notes (검증 필요).
    pub const LEGACY_FOUR_BYTE_BITS: u32 = (1 << 3)
        | (1 << 4)
        | (1 << 5)
        | (1 << 6)
        | (1 << 7)
        | (1 << 8)
        | (1 << 9)
        | (1 << 10)
        | (1 << 11)
        | (1 << 12)
        | (1 << 13)
        | (1 << 14)
        | (1 << 15)
        | (1 << 16)
        | (1 << 17)
        | (1 << 18)
        | (1 << 19)
        | (1 << 20);
    /// Highest flag bit that is decoded; higher bits are never set by known writers.
    pub const LEGACY_LAST_BIT: u32 = 20;
    /// Bitmask of format list key bits: number (13), currency (14), date (15, 검증 필요),
    /// duration (16) and control/checkbox (17) format.
    pub const LEGACY_FORMAT_MASK: u32 = (1 << 13) | (1 << 14) | (1 << 15) | (1 << 16) | (1 << 17);
    /// Current layout: version byte 1, type byte, flags word at byte 4, fields from byte 8 in
    /// flag-bit order. 검증 필요: fixture documents only
    pub const CURRENT_VERSION: u8 = 1;
    pub const FLAGS_OFFSET: usize = 4;
    pub const FIELDS_OFFSET: usize = 8;
    /// Cell type byte values. 확인 unless noted.
    pub const TYPE_EMPTY: u8 = 0;
    pub const TYPE_NUMBER: u8 = 2;
    pub const TYPE_STRING: u8 = 3;
    pub const TYPE_DURATION: u8 = 7;
    pub const TYPE_ERROR: u8 = 8;
    pub const TYPE_DATE: u8 = 5;
    pub const TYPE_BOOL: u8 = 6;
    pub const TYPE_RICH_TEXT: u8 = 9;
    /// Currency cell (decimal value like a number). 검증 필요: public notes only.
    pub const TYPE_CURRENCY: u8 = 10;
    /// Number of flag bits that are decoded (bits 0..=10); higher bits come after every field
    /// that is read.
    pub const KNOWN_FLAG_BITS: u32 = 11;
    /// Bit whose field is the cell value (decimal for numbers, f64 otherwise). 확인
    pub const VALUE_BIT: u32 = 6;
    /// Bit whose field is the rich text list key. 확인
    pub const RICH_TEXT_KEY_BIT: u32 = 9;
    /// Bit whose field is the format list key. 확인
    pub const FORMAT_KEY_BIT: u32 = 10;
    /// Bits 7 and 8 (4-byte ids assumed; never set in the fixtures). 검증 필요
    pub const ASSUMED_FIELD_BYTES: usize = 4;
    /// Size of the decimal128 value of number cells. 확인
    pub const DECIMAL_BYTES: usize = 16;
    /// Size of the f64 value of date and checkbox cells. 확인
    pub const DOUBLE_BYTES: usize = 8;
}

/// Days from 1970-01-01 to 2001-01-01, the epoch of date cell values (seconds). 확인
pub(crate) const DATE_EPOCH_DAYS_FROM_UNIX: i64 = 11_323;
