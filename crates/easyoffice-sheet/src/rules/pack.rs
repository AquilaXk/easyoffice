//! Bit-packed form of the cell appearances sent to the page.
//!
//! Colours are deduplicated into a palette (a colour scale uses a few hundred distinct colours at
//! most), and each cell is two to four 32-bit words:
//!
//! ```text
//! word 0   row (zero-based)
//! word 1   bits  0..14  column            bit 18 has background   bit 21 has icon
//!          bits 14..18  style flags       bit 19 has font colour  bit 22 hide value
//!                                         bit 20 has data bar     bit 23 gradient bar
//!          bits 24..32  icon: set << 3 | index
//! word 2   (background or font colour)   bits 0..16 background palette index, 16..32 font colour
//! word 3   (data bar)  bits 0..16 colour palette index, 16..24 length + 128, 24..32 axis percent
//! ```
//!
//! A cell with no flag set means "no formatting": the page clears whatever it showed there.

use super::{Bar, CellVisual, Icon};
use crate::{CellMap, PackedCellCoordinate};

/// Palette indexes are 16 bits; a colour past the limit reuses the last palette entry.
pub const MAX_PALETTE: usize = 0xFFFF;

const BG: u32 = 1 << 18;
const FG: u32 = 1 << 19;
const BAR: u32 = 1 << 20;
const ICON: u32 = 1 << 21;
const HIDE: u32 = 1 << 22;
const GRADIENT: u32 = 1 << 23;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackedVisuals {
    /// `0xRRGGBB` colours.
    pub palette: Vec<u32>,
    pub words: Vec<u32>,
}

impl PackedVisuals {
    /// The words as little-endian bytes (what the page decodes from base64).
    pub fn word_bytes(&self) -> Vec<u8> {
        self.words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }
}

struct Palette {
    colors: Vec<u32>,
    index: CellMap<u16>,
}

impl Palette {
    fn id(&mut self, rgb: u32) -> u32 {
        let key = PackedCellCoordinate(u64::from(rgb));
        if let Some(&i) = self.index.get(&key) {
            return u32::from(i);
        }
        if self.colors.len() >= MAX_PALETTE {
            return (MAX_PALETTE - 1) as u32;
        }
        let i = self.colors.len() as u16;
        self.colors.push(rgb);
        self.index.insert(key, i);
        u32::from(i)
    }
}

pub fn encode_visuals(visuals: &[CellVisual]) -> PackedVisuals {
    let mut palette = Palette {
        colors: Vec::new(),
        index: CellMap::default(),
    };
    let mut words = Vec::with_capacity(visuals.len() * 3);
    for v in visuals {
        let mut head = v.coord.col() & 0x3FFF | u32::from(v.flags & 0xF) << 14;
        if v.bg.is_some() {
            head |= BG;
        }
        if v.fg.is_some() {
            head |= FG;
        }
        if v.bar.is_some() {
            head |= BAR;
        }
        if let Some(icon) = v.icon {
            head |= ICON | (u32::from(icon.set) << 3 | u32::from(icon.index)) << 24;
        }
        if v.hide_value {
            head |= HIDE;
        }
        if v.bar.is_some_and(|b| b.gradient) {
            head |= GRADIENT;
        }
        words.push(v.coord.row());
        words.push(head);
        if v.bg.is_some() || v.fg.is_some() {
            let bg = v.bg.map_or(0, |c| palette.id(c));
            let fg = v.fg.map_or(0, |c| palette.id(c));
            words.push(bg | fg << 16);
        }
        if let Some(bar) = v.bar {
            let color = palette.id(bar.color);
            words.push(
                color
                    | (u32::from((i16::from(bar.pct) + 128) as u8)) << 16
                    | u32::from(bar.start) << 24,
            );
        }
    }
    PackedVisuals {
        palette: palette.colors,
        words,
    }
}

/// Inverse of [`encode_visuals`] (the page has the same decoder; this one backs the tests).
pub fn decode_visuals(packed: &PackedVisuals) -> Vec<CellVisual> {
    let color = |i: u32| packed.palette.get(i as usize).copied();
    let mut out = Vec::new();
    let mut at = 0;
    let words = &packed.words;
    while at + 1 < words.len() {
        let (row, head) = (words[at], words[at + 1]);
        at += 2;
        let coord = PackedCellCoordinate((u64::from(row) << 32) | u64::from(head & 0x3FFF));
        let mut v = CellVisual {
            coord,
            bg: None,
            fg: None,
            flags: ((head >> 14) & 0xF) as u8,
            bar: None,
            icon: None,
            hide_value: head & HIDE != 0,
        };
        if head & (BG | FG) != 0 {
            let w = words[at];
            at += 1;
            if head & BG != 0 {
                v.bg = color(w & 0xFFFF);
            }
            if head & FG != 0 {
                v.fg = color(w >> 16);
            }
        }
        if head & BAR != 0 {
            let w = words[at];
            at += 1;
            v.bar = Some(Bar {
                pct: (((w >> 16) & 0xFF) as i16 - 128) as i8,
                start: (w >> 24) as u8,
                color: color(w & 0xFFFF).unwrap_or(0),
                gradient: head & GRADIENT != 0,
            });
        }
        if head & ICON != 0 {
            let id = head >> 24;
            v.icon = Some(Icon {
                set: (id >> 3) as u8,
                index: (id & 7) as u8,
            });
        }
        out.push(v);
    }
    out
}
