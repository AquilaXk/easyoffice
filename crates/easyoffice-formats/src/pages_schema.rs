//! The ONLY place that knows Pages object type IDs and protobuf field numbers.
//!
//! These numbers are our own constants for the wire format observed in documents and described by
//! public format documentation; no Apple `.proto` file or third-party schema text is copied.
//! Every constant is either confirmed against a real document ("확인") or carries the marker
//! `// 검증 필요` (unconfirmed). To correct the mapping, edit only this file.
//!
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
    /// Table tile, fixture documents (same message shape). 확인
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
    /// Footnote (points at the footnote text storage). 확인 (fixture)
    pub const FOOTNOTE: u32 = 2012;
    /// Package metadata (list of data files with identifiers and names). 확인 (fixture)
    pub const PACKAGE_METADATA: u32 = 11006;
    /// Drawables floating on pages (page-layout documents). 검증 필요: field layout is from a
    /// public independent implementation's notes; the fixtures only carry an empty list.
    pub const FLOATING_DRAWABLES: u32 = 10010;
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
    /// Repeated `{ drawable reference = 1 }` entries inside a page group.
    pub const GROUP_DRAWABLES: u32 = 4;
}

/// Geometry field numbers.
pub(crate) mod geometry {
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
    /// Comment attribute table (index of the commented range start to a comment, type 2033).
    /// 확인
    pub const COMMENT_TABLE: u32 = 19;
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
};

/// Compact layout (`style::COMPACT_CHAR_PROPS`), every field 확인 against the fixture text file:
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

/// Compact paragraph property layout (`style::COMPACT_PARA_PROPS`), 확인 against the fixture text
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
    /// Data store: merge region map reference (an object with repeated cell ranges). 검증 필요:
    /// field numbers are from a public independent implementation's schema notes (the data store
    /// fields 17 and 22 of the same notes match the fixtures); the fixtures carry no merged cell.
    pub const STORE_MERGE_REGION_MAP: u32 = 13;
    /// Merge region map: repeated cell range. 검증 필요 (see [`STORE_MERGE_REGION_MAP`]).
    pub const MERGE_RANGES: u32 = 1;
    /// Cell range: origin `{ packed = 1 }` and size `{ packed = 1 }`, each a fixed32 with the
    /// column in the high 16 bits and the row in the low 16 bits. 검증 필요 (see above).
    pub const RANGE_ORIGIN: u32 = 1;
    pub const RANGE_SIZE: u32 = 2;
    pub const RANGE_PACKED: u32 = 1;
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
    /// Flag bits whose 4-byte fields are present in the fixtures: cell style, text style,
    /// formula, formula error, suggestion, and the number, currency, duration and text formats.
    pub const LEGACY_FOUR_BYTE_BITS: u32 = (1 << 3)
        | (1 << 4)
        | (1 << 5)
        | (1 << 6)
        | (1 << 9)
        | (1 << 11)
        | (1 << 12)
        | (1 << 13)
        | (1 << 14)
        | (1 << 16)
        | (1 << 17);
    /// Flag bits whose field size was never seen (bits 2, 7, 8, 10, 15): such cells are not
    /// read, because every later field offset would be a guess. 검증 필요
    pub const LEGACY_UNVERIFIED_BITS: u32 = (1 << 2) | (1 << 7) | (1 << 8) | (1 << 10) | (1 << 15);
    /// Highest flag bit that is decoded.
    pub const LEGACY_LAST_BIT: u32 = 17;
    /// Bitmask of format list key bits: number (13), currency (14), duration (16) and control/checkbox (17) format.
    pub const LEGACY_FORMAT_MASK: u32 = (1 << 13) | (1 << 14) | (1 << 16) | (1 << 17);
    /// Current layout: version byte 1, type byte, flags word at byte 4, fields from byte 8 in
    /// flag-bit order. 확인 (fixture documents)
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
