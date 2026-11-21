//! Bit-packed form of the cell appearances sent to the page.
//!
//! Colours are deduplicated into a palette (a colour scale uses a few hundred distinct colours at
//! most) and border sets into a table (a rule draws the same few sets on thousands of cells), and
//! each cell is two to five 32-bit words:
//!
//! ```text
//! word 0   bits  0..20  row (zero-based)     bit 31 has border
//! word 1   bits  0..14  column            bit 18 has background   bit 21 has icon
//!          bits 14..18  style flags       bit 19 has font colour  bit 22 hide value
//!                                         bit 20 has data bar     bit 23 gradient bar
//!          bits 24..32  icon: set << 3 | index
//! word 2   (background or font colour)   bits 0..16 background palette index, 16..32 font colour
//! word 3   (border)    bits 0..16 index into the border table
//! word 4   (data bar)  bits 0..16 colour palette index, 16..24 length + 128, 24..32 axis percent
//! ```
//!
//! A cell with no flag set means "no formatting": the page clears whatever it showed there. A
//! border set is the four edge words of `BorderEdge::bits` (top, right, bottom, left).

use super::{Bar, CellVisual, Icon};
use crate::{CellMap, PackedCellCoordinate};
use easyoffice_formats::{BorderEdge, CellBorders};
use std::collections::HashMap;

/// Palette and border table indexes are 16 bits; an entry past the limit reuses the last one.
pub const MAX_PALETTE: usize = 0xFFFF;

const BG: u32 = 1 << 18;
const FG: u32 = 1 << 19;
const BAR: u32 = 1 << 20;
const ICON: u32 = 1 << 21;
const HIDE: u32 = 1 << 22;
const GRADIENT: u32 = 1 << 23;
/// In word 0: the rows of a sheet need 20 bits (`PackedCellCoordinate::MAX_ROWS` is 2^20), the top bit says a border word follows.
const BORDER: u32 = 1 << 31;
const ROW_MASK: u32 = (1 << 20) - 1;

type EdgeWords = [u32; 4];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackedVisuals {
    /// `0xRRGGBB` colours.
    pub palette: Vec<u32>,
    /// The border sets the cells name, each as the packed words of its top, right, bottom and left edge.
    pub borders: Vec<EdgeWords>,
    pub words: Vec<u32>,
}

impl PackedVisuals {
    /// The words as little-endian bytes (what the page decodes from base64).
    pub fn word_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.words.len().saturating_mul(4));
        for &w in &self.words {
            bytes.extend_from_slice(&w.to_le_bytes());
        }
        bytes
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

fn edge_words(b: CellBorders) -> EdgeWords {
    [b.top.bits(), b.right.bits(), b.bottom.bits(), b.left.bits()]
}

/// The distinct border sets of an answer. Cells next to each other mostly share a set, so the last one found is
/// checked before the map.
#[derive(Default)]
struct BorderTable {
    sets: Vec<EdgeWords>,
    index: HashMap<EdgeWords, u16>,
    last: Option<(EdgeWords, u32)>,
}

impl BorderTable {
    fn id(&mut self, borders: CellBorders) -> u32 {
        let words = edge_words(borders);
        if let Some((last, id)) = self.last {
            if last == words {
                return id;
            }
        }
        let id = match self.index.get(&words) {
            Some(&i) => u32::from(i),
            None if self.sets.len() >= MAX_PALETTE => (MAX_PALETTE - 1) as u32,
            None => {
                let i = self.sets.len() as u16;
                self.sets.push(words);
                self.index.insert(words, i);
                u32::from(i)
            }
        };
        self.last = Some((words, id));
        id
    }
}

pub fn encode_visuals(visuals: &[CellVisual]) -> PackedVisuals {
    let mut palette = Palette {
        colors: Vec::new(),
        index: CellMap::default(),
    };
    let mut borders = BorderTable::default();
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
        let bordered = !v.border.is_none();
        words.push(v.coord.row() & ROW_MASK | if bordered { BORDER } else { 0 });
        words.push(head);
        if v.bg.is_some() || v.fg.is_some() {
            let bg = v.bg.map_or(0, |c| palette.id(c));
            let fg = v.fg.map_or(0, |c| palette.id(c));
            words.push(bg | fg << 16);
        }
        if bordered {
            words.push(borders.id(v.border));
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
        borders: borders.sets,
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
        let (first, head) = (words[at], words[at + 1]);
        at += 2;
        let row = first & ROW_MASK;
        let coord = PackedCellCoordinate((u64::from(row) << 32) | u64::from(head & 0x3FFF));
        let mut v = CellVisual {
            coord,
            bg: None,
            fg: None,
            flags: ((head >> 14) & 0xF) as u8,
            bar: None,
            icon: None,
            hide_value: head & HIDE != 0,
            border: CellBorders::NONE,
        };
        if head & (BG | FG) != 0 {
            let Some(w) = words.get(at).copied() else {
                return Vec::new();
            };
            at += 1;
            if head & BG != 0 {
                v.bg = color(w & 0xFFFF);
            }
            if head & FG != 0 {
                v.fg = color(w >> 16);
            }
        }
        if first & BORDER != 0 {
            let Some(w) = words.get(at).copied() else {
                return Vec::new();
            };
            at += 1;
            let Some(set) = packed.borders.get((w & 0xFFFF) as usize) else {
                return Vec::new();
            };
            v.border = CellBorders {
                top: BorderEdge::from_bits(set[0]),
                right: BorderEdge::from_bits(set[1]),
                bottom: BorderEdge::from_bits(set[2]),
                left: BorderEdge::from_bits(set[3]),
            };
        }
        if head & BAR != 0 {
            let Some(w) = words.get(at).copied() else {
                return Vec::new();
            };
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
    if at < words.len() {
        return Vec::new();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyoffice_formats::BorderLine;

    fn coord(row: u32, col: u32) -> PackedCellCoordinate {
        PackedCellCoordinate::new(row, col).unwrap()
    }

    #[test]
    fn word_bytes_preallocates_exact_byte_capacity() {
        let visuals = vec![CellVisual {
            coord: coord(0, 0),
            bg: Some(0x112233),
            fg: Some(0x445566),
            flags: 1,
            bar: Some(Bar {
                pct: 50,
                start: 10,
                color: 0x778899,
                gradient: true,
            }),
            icon: None,
            hide_value: false,
            border: CellBorders::NONE,
        }];
        let packed = encode_visuals(&visuals);
        let bytes = packed.word_bytes();
        assert_eq!(bytes.len(), packed.words.len() * 4);
        assert!(bytes.capacity() >= bytes.len());
    }

    #[test]
    fn decode_visuals_rejects_a_border_word_that_is_missing_or_names_no_set() {
        let one = encode_visuals(&[CellVisual {
            coord: coord(2, 3),
            border: CellBorders {
                top: BorderEdge::new(BorderLine::Thin, 0xFF0000),
                ..CellBorders::NONE
            },
            ..CellVisual::blank(coord(2, 3))
        }]);
        assert_eq!(decode_visuals(&one).len(), 1);

        let mut truncated = one.clone();
        truncated.words.pop();
        assert!(decode_visuals(&truncated).is_empty());

        let mut no_table = one.clone();
        no_table.borders.clear();
        assert!(decode_visuals(&no_table).is_empty());

        let mut far = one.clone();
        *far.words.last_mut().unwrap() = 0xFFFF;
        assert!(decode_visuals(&far).is_empty());

        // A set with an unknown line kind or stray bits draws nothing.
        let mut junk = one;
        junk.borders[0] = [0xFF00_0000, 7 << 24, 0x0800_0000, 0];
        assert!(decode_visuals(&junk)[0].border.is_none());
    }

    #[test]
    fn decode_visuals_rejects_truncated_buffers_fail_closed() {
        let mut bg_truncated = encode_visuals(&[CellVisual {
            coord: coord(0, 0),
            bg: Some(0xFFC7CE),
            fg: Some(0x9C0006),
            flags: 0,
            bar: None,
            icon: None,
            hide_value: false,
            border: CellBorders::NONE,
        }]);
        bg_truncated.words.pop();
        assert!(decode_visuals(&bg_truncated).is_empty());

        let mut bar_truncated = encode_visuals(&[CellVisual {
            coord: coord(1, 2),
            bg: Some(0x112233),
            fg: None,
            flags: 0,
            bar: Some(Bar {
                pct: 25,
                start: 0,
                color: 0x638EC6,
                gradient: false,
            }),
            icon: None,
            hide_value: false,
            border: CellBorders::NONE,
        }]);
        bar_truncated.words.pop();
        assert!(decode_visuals(&bar_truncated).is_empty());

        let mut orphan_tail = encode_visuals(&[CellVisual {
            coord: coord(0, 0),
            bg: None,
            fg: None,
            flags: 1,
            bar: None,
            icon: None,
            hide_value: false,
            border: CellBorders::NONE,
        }]);
        orphan_tail.words.push(0xDEAD_BEEF);
        assert!(decode_visuals(&orphan_tail).is_empty());
    }
}
