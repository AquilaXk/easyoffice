//! Mandatory BIFF8 workbook global style records (`FONT`, `XF`, `STYLE`) per `[MS-XLS]`.
//!
//! Legacy spreadsheet readers expect 4 built-in `FONT` records (`0..=3`, with font index `4`
//! skipped before custom fonts starting at index `5`), 16 built-in `XF` records (`0..=14` style
//! XFs with `fStyle=1, ixfeParent=0xFFF`, plus default cell XF `15` with `fStyle=0, ixfeParent=0`),
//! and 6 built-in `STYLE` (`0x0293`) records before any workbook-specific custom styles.

use crate::xls_styles::{default_font_record, XF};
use crate::DefaultFont;

/// Default cell XF index in BIFF8 (`[MS-XLS]` §2.4.353): style XFs occupy `0..=14`, cell defaults at `15`.
pub(crate) const DEFAULT_CELL_XF: u16 = 15;

const STYLE_REC: u16 = 0x0293;
const STYLE_XF_COUNT: usize = 15;
const STYLE_XF_FLAGS: u16 = 0xFFF5;
const CELL_XF_FLAGS: u16 = 0x0001;
const DEFAULT_COLORS_WORD: u16 = 0x0040 | (0x0041 << 7);

/// Built-in style records `[MS-XLS]` §2.4.269 (`ixfe`, `iStyBuiltIn`): Normal, Comma, Currency,
/// Percent, Comma [0], Currency [0].
const BUILTIN_STYLES: [(u16, u8); 6] = [
    (0, 0x00),
    (1, 0x03),
    (2, 0x04),
    (3, 0x05),
    (4, 0x06),
    (5, 0x07),
];

/// Writes the 4 built-in BIFF8 `FONT` records (`0..=3`, `[MS-XLS]` §2.4.122).
pub(crate) fn write_builtin_fonts(out: &mut Vec<u8>, wb_default_font: Option<&DefaultFont>) {
    let record = default_font_record(wb_default_font);
    for _ in 0..4 {
        out.extend_from_slice(&record);
    }
}

/// Writes the 15 built-in style `XF` records (`0..=14`) plus the default cell `XF` at index `15`.
pub(crate) fn write_builtin_xfs(out: &mut Vec<u8>) {
    let mut style_body = [0u8; 20];
    style_body[4..6].copy_from_slice(&STYLE_XF_FLAGS.to_le_bytes());
    style_body[18..20].copy_from_slice(&DEFAULT_COLORS_WORD.to_le_bytes());
    for _ in 0..STYLE_XF_COUNT {
        out.extend_from_slice(&XF.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&style_body);
    }

    let mut cell_body = [0u8; 20];
    cell_body[4..6].copy_from_slice(&CELL_XF_FLAGS.to_le_bytes());
    cell_body[18..20].copy_from_slice(&DEFAULT_COLORS_WORD.to_le_bytes());
    out.extend_from_slice(&XF.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes());
    out.extend_from_slice(&cell_body);
}

/// Writes the 6 built-in `STYLE` (`0x0293`) records (`[MS-XLS]` §2.4.269).
pub(crate) fn write_builtin_styles(out: &mut Vec<u8>) {
    for (ixfe, built_in) in BUILTIN_STYLES {
        out.extend_from_slice(&STYLE_REC.to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&(ixfe | 0x8000).to_le_bytes());
        out.push(built_in);
        out.push(0xFF);
    }
}
