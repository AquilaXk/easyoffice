//! Remembered window geometry (PD-099): the size, position and maximized state of the last
//! document window, restored when the next window opens. Same behaviour as the window-state plugin
//! the reference shell uses, but stored as one fixed 18-byte little-endian record in the app data
//! directory instead of a JSON map keyed by window label (our labels are per launch, so a map would
//! only grow). Pure encode / decode / placement rules live here; `window_commands.rs` wires them.

use std::fs;
use std::path::Path;

const FILE_NAME: &str = "window-state.bin";
const MAGIC: [u8; 4] = *b"EOWS";
const VERSION: u8 = 1;
/// magic (4) + version (1) + flags (1) + width (u16) + height (u16) + x (i32) + y (i32).
pub const RECORD_LEN: usize = 18;
const FLAG_MAXIMIZED: u8 = 0b0000_0001;
const KNOWN_FLAGS: u8 = FLAG_MAXIMIZED;
/// Smallest edge a stored size may have; anything below is a damaged record.
const MIN_EDGE: u16 = 200;
/// Part of the title bar that must lie on a monitor for a stored position to be reused.
const VISIBLE_GRIP: (i32, i32) = (120, 40);

/// Logical-pixel geometry of a window in its normal (not maximized) state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowGeometry {
    pub width: u16,
    pub height: u16,
    pub x: i32,
    pub y: i32,
    pub maximized: bool,
}

/// A monitor's work area in logical pixels: origin and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl MonitorRect {
    /// True when the point lies inside this rectangle; overflow fails closed (outside).
    fn contains(self, px: i32, py: i32) -> bool {
        let (Some(right), Some(bottom)) = (
            self.x.checked_add(self.width),
            self.y.checked_add(self.height),
        ) else {
            return false;
        };
        px >= self.x && px < right && py >= self.y && py < bottom
    }
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let slice = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_i32(bytes: &[u8], at: usize) -> Option<i32> {
    let slice = bytes.get(at..at.checked_add(4)?)?;
    Some(i32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

impl WindowGeometry {
    /// Builds a record from logical sizes; `None` when a value does not fit the record.
    pub fn from_logical(size: (f64, f64), origin: (f64, f64), maximized: bool) -> Option<Self> {
        let edge = |v: f64| {
            let rounded = v.round();
            (rounded.is_finite()
                && rounded >= f64::from(MIN_EDGE)
                && rounded <= f64::from(u16::MAX))
            .then_some(rounded as u16)
        };
        let coord = |v: f64| {
            let rounded = v.round();
            (rounded.is_finite()
                && rounded >= f64::from(i32::MIN)
                && rounded <= f64::from(i32::MAX))
            .then_some(rounded as i32)
        };
        Some(Self {
            width: edge(size.0)?,
            height: edge(size.1)?,
            x: coord(origin.0)?,
            y: coord(origin.1)?,
            maximized,
        })
    }

    pub fn encode(self) -> [u8; RECORD_LEN] {
        let mut out = [0u8; RECORD_LEN];
        out[..4].copy_from_slice(&MAGIC);
        out[4] = VERSION;
        out[5] = if self.maximized { FLAG_MAXIMIZED } else { 0 };
        out[6..8].copy_from_slice(&self.width.to_le_bytes());
        out[8..10].copy_from_slice(&self.height.to_le_bytes());
        out[10..14].copy_from_slice(&self.x.to_le_bytes());
        out[14..18].copy_from_slice(&self.y.to_le_bytes());
        out
    }

    /// Fails closed: wrong length, magic, version, unknown flag bits or a tiny edge give `None`.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != RECORD_LEN || bytes[..4] != MAGIC || bytes[4] != VERSION {
            return None;
        }
        let flags = bytes[5];
        if flags & !KNOWN_FLAGS != 0 {
            return None;
        }
        let width = read_u16(bytes, 6)?;
        let height = read_u16(bytes, 8)?;
        if width < MIN_EDGE || height < MIN_EDGE {
            return None;
        }
        Some(Self {
            width,
            height,
            x: read_i32(bytes, 10)?,
            y: read_i32(bytes, 14)?,
            maximized: flags & FLAG_MAXIMIZED != 0,
        })
    }

    pub fn size(self) -> (f64, f64) {
        (f64::from(self.width), f64::from(self.height))
    }

    /// The stored origin, if a grip of the title bar (top-left corner and the point
    /// `VISIBLE_GRIP` further in) lands on one of the monitors; otherwise the caller cascades.
    pub fn visible_origin(self, monitors: &[MonitorRect]) -> Option<(f64, f64)> {
        let grip_x = self.x.checked_add(VISIBLE_GRIP.0)?;
        let grip_y = self.y.checked_add(VISIBLE_GRIP.1)?;
        monitors
            .iter()
            .any(|m| m.contains(self.x, self.y) && m.contains(grip_x, grip_y))
            .then(|| (f64::from(self.x), f64::from(self.y)))
    }

    pub fn load(dir: &Path) -> Option<Self> {
        Self::decode(&fs::read(dir.join(FILE_NAME)).ok()?)
    }

    pub fn save(self, dir: &Path) -> std::io::Result<()> {
        fs::create_dir_all(dir)?;
        fs::write(dir.join(FILE_NAME), self.encode())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: WindowGeometry = WindowGeometry {
        width: 1440,
        height: 900,
        x: -1600,
        y: 24,
        maximized: true,
    };

    #[test]
    fn record_is_18_little_endian_bytes_and_round_trips() {
        let bytes = SAMPLE.encode();
        assert_eq!(&bytes[..6], b"EOWS\x01\x01");
        assert_eq!(&bytes[6..10], &[0xA0, 0x05, 0x84, 0x03]);
        assert_eq!(&bytes[10..14], &(-1600i32).to_le_bytes());
        assert_eq!(WindowGeometry::decode(&bytes), Some(SAMPLE));
    }

    #[test]
    fn damaged_records_fail_closed() {
        let good = SAMPLE.encode();
        assert_eq!(WindowGeometry::decode(&good[..17]), None, "short");
        let mut long = good.to_vec();
        long.push(0);
        assert_eq!(WindowGeometry::decode(&long), None, "long");
        for (at, value) in [(0, b'X'), (4, 2), (5, 0b10)] {
            let mut bad = good;
            bad[at] = value;
            assert_eq!(WindowGeometry::decode(&bad), None, "byte {at}");
        }
        let mut tiny = good;
        tiny[6..8].copy_from_slice(&199u16.to_le_bytes());
        assert_eq!(WindowGeometry::decode(&tiny), None, "edge below minimum");
    }

    #[test]
    fn logical_values_round_and_reject_out_of_range() {
        let g = WindowGeometry::from_logical((1279.6, 860.2), (10.4, -3.5), false).unwrap();
        assert_eq!((g.width, g.height, g.x, g.y), (1280, 860, 10, -4));
        assert_eq!(
            WindowGeometry::from_logical((100.0, 860.0), (0.0, 0.0), false),
            None
        );
        assert_eq!(
            WindowGeometry::from_logical((70_000.0, 860.0), (0.0, 0.0), false),
            None
        );
        assert_eq!(
            WindowGeometry::from_logical((800.0, f64::NAN), (0.0, 0.0), false),
            None
        );
        assert_eq!(
            WindowGeometry::from_logical((800.0, 600.0), (1e12, 0.0), false),
            None
        );
    }

    #[test]
    fn origin_is_reused_only_when_the_title_bar_is_on_a_monitor() {
        let primary = MonitorRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let left = MonitorRect {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
        };
        assert_eq!(
            SAMPLE.visible_origin(&[primary, left]),
            Some((-1600.0, 24.0))
        );
        assert_eq!(SAMPLE.visible_origin(&[primary]), None, "monitor unplugged");
        let edge = WindowGeometry { x: 1850, ..SAMPLE };
        assert_eq!(edge.visible_origin(&[primary]), None, "grip off screen");
        let far = WindowGeometry {
            x: i32::MAX,
            ..SAMPLE
        };
        assert_eq!(
            far.visible_origin(&[primary]),
            None,
            "overflow fails closed"
        );
        let huge = MonitorRect {
            x: i32::MAX - 5,
            y: 0,
            width: 100,
            height: 100,
        };
        assert!(
            !huge.contains(i32::MAX - 1, 1),
            "monitor overflow fails closed"
        );
    }

    #[test]
    fn state_round_trips_through_the_data_directory() {
        let dir = std::env::temp_dir().join(format!("easyoffice-wstate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(WindowGeometry::load(&dir), None, "no file yet");
        SAMPLE.save(&dir).unwrap();
        assert_eq!(WindowGeometry::load(&dir), Some(SAMPLE));
        fs::write(dir.join(FILE_NAME), b"garbage").unwrap();
        assert_eq!(WindowGeometry::load(&dir), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
