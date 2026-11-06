//! The ONLY place that knows Pages object type IDs and protobuf field numbers.
//!
//! These numbers are our own constants for the wire format observed in documents and described by
//! public format documentation; no Apple `.proto` file or third-party schema text is copied.
//! Every constant is either confirmed against a real document ("확인") or carries the marker
//! `// 검증 필요` (unconfirmed). To correct the mapping, edit only this file.
//!
//! Confirmation basis: one real document created with the web edition of the word processor
//! (file format version 184 in `Metadata/Properties.plist`), which contains bold and underline
//! character styles, heading/body paragraph styles, list styles and 14 tables whose cells are all
//! rich-text cells.

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
    /// Table tile (rows of packed cells). 확인
    pub const TABLE_TILE: u32 = 6002;
    /// Table data list (strings, rich text payloads, styles, ...). 확인
    pub const TABLE_DATA_LIST: u32 = 6005;
    /// Rich text payload of a table cell (points at a text storage). 확인
    pub const RICH_TEXT_PAYLOAD: u32 = 6218;
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
    /// List style attribute table (same shape). 확인 (shape); 목록 문단 적용 검증 필요
    pub const LIST_STYLE_TABLE: u32 = 7;
    /// Character style attribute table (same shape; an entry without object resets). 확인
    pub const CHAR_STYLE_TABLE: u32 = 8;
    /// Attachment attribute table (index of each U+FFFC to an attachment). 확인
    pub const ATTACHMENT_TABLE: u32 = 9;
    /// Attribute table entry: character index (UTF-16 code units). 확인 (BMP); 비BMP 검증 필요
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
    /// List style: repeated per-level list type. 확인
    pub const LIST_LEVEL_TYPES: u32 = 11;
}

/// Character property field numbers.
pub(crate) mod char_props {
    /// Bold (varint bool). 확인
    pub const BOLD: u32 = 1;
    /// Italic (varint bool). 검증 필요
    pub const ITALIC: u32 = 2;
    /// Font size in points (fixed32 float). 확인
    pub const SIZE_PT: u32 = 3;
    /// Underline (varint bool). 확인
    pub const UNDERLINE: u32 = 11;
    /// Strikethrough (varint bool). 검증 필요
    pub const STRIKE: u32 = 12;
    /// Text color message. 확인
    pub const COLOR: u32 = 7;
    /// Color message: red, green, blue, alpha (fixed32 floats 0..1). 확인
    pub const COLOR_RGBA: [u32; 4] = [3, 4, 5, 6];
}

/// Paragraph property field numbers and enum values.
pub(crate) mod para_props {
    /// Alignment enum. 값 0/2 확인, 4(natural)=정렬 지정 없음, 1/3 검증 필요
    pub const ALIGNMENT: u32 = 1;
    pub const ALIGN_LEFT: u64 = 0;
    pub const ALIGN_RIGHT: u64 = 1; // 검증 필요
    pub const ALIGN_CENTER: u64 = 2;
    pub const ALIGN_JUSTIFY: u64 = 3; // 검증 필요
}

/// List level types (`style::LIST_LEVEL_TYPES` values). 확인
pub(crate) mod list_type {
    pub const IMAGE: u64 = 1;
    pub const BULLET: u64 = 2;
    pub const NUMBER: u64 = 3;
}

/// Substrings of a paragraph style identifier that mark headings. 확인 (heading 1-4, caption,
/// footnote identifiers observed); `Title` 검증 필요
pub(crate) mod heading {
    pub const HEADING_PREFIX: &str = "paragraphstyle-Heading ";
    pub const TITLE_MARKER: &str = "paragraphstyle-Title";
}

/// Attachment field numbers.
pub(crate) mod attachment {
    /// Reference to the drawable (table info, image, shape...). 확인
    pub const DRAWABLE: u32 = 1;
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
    /// Data store: rich text payload list reference. 확인
    pub const STORE_RICH_TEXT_LIST: u32 = 17;
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
    /// Row info: packed cell buffer (version-5 layout). 확인
    pub const ROW_CELL_BUFFER: u32 = 6;
    /// Row info: u16 LE cell offsets per column; 0xFFFF means no cell. 확인
    pub const ROW_CELL_OFFSETS: u32 = 7;
    pub const NO_CELL_OFFSET: u16 = 0xFFFF;
    /// Data list: list type. 확인
    pub const LIST_TYPE: u32 = 1;
    /// Data list: entries. 확인
    pub const LIST_ENTRIES: u32 = 3;
    /// Data list entry: key. 확인
    pub const ENTRY_KEY: u32 = 1;
    /// Rich text list entry: payload reference. 확인
    pub const ENTRY_RICH_TEXT: u32 = 9;
    /// List type of the rich text payload list. 확인
    pub const LIST_TYPE_RICH_TEXT: u64 = 8;
    /// Rich text payload: storage reference. 확인
    pub const PAYLOAD_STORAGE: u32 = 1;
}

/// Packed cell layout (version 5).
pub(crate) mod cell {
    /// Cell format version byte. 확인
    pub const VERSION: u8 = 5;
    /// Cell type byte: rich text. 확인
    pub const TYPE_RICH_TEXT: u8 = 9;
    /// Offset of the little-endian flags word. 확인
    pub const FLAGS_OFFSET: usize = 8;
    /// Offset of the first optional 4-byte field. 확인
    pub const FIELDS_OFFSET: usize = 12;
    /// Flag bit whose field is the rich text list key. 확인 (키 순서가 목록 키와 일치); 비트 의미 검증 필요
    pub const FLAG_RICH_TEXT_KEY: u32 = 0x10;
}
