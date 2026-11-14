//! The `BOUNDSHEET` record of a BIFF8 workbook: the name of a sheet, where its substream starts and
//! whether it is shown (PD-209).
//!
//! 참조 구현: Apache POI `hssf/record/BoundSheetRecord.java` lines 38-39 and 108-150 (the position of the
//! `BOF`, the option word whose bit 0 hides the sheet and bit 1 hides it for good, the name length and its
//! encoding flag, then the name), Apache-2.0, Copyright The Apache Software Foundation, commit 47dcff7.
//!
//! Modifications: the name is always UTF-16LE (the writer never needs the compressed form), the record
//! carries the sheet kind byte of the format (`dt`, a worksheet), and the state is the [`SheetState`] of
//! this crate.

use crate::sheet_styling::SheetState;

pub(crate) const BOUNDSHEET: u16 = 0x0085;

/// Appends the record of a worksheet named `name` (at most 31 units, see `xls_sheet_names`) and returns the
/// offset of its stream position, which the caller patches when the substream's place is known.
pub(crate) fn push(stream: &mut Vec<u8>, name: &str, state: SheetState) -> usize {
    let units: Vec<u16> = name.encode_utf16().collect();
    stream.extend_from_slice(&BOUNDSHEET.to_le_bytes());
    stream.extend_from_slice(&((8 + units.len() * 2) as u16).to_le_bytes());
    let position_at = stream.len();
    stream.extend_from_slice(&0u32.to_le_bytes());
    stream.push(state.biff());
    stream.push(0); // dt = worksheet
    stream.push(units.len() as u8);
    stream.push(0x01); // grbit = 1 (UTF-16LE)
    for unit in units {
        stream.extend_from_slice(&unit.to_le_bytes());
    }
    position_at
}

/// The state a record's `hsState` byte (the byte after the stream position) says; `payload` is the record's
/// data and must hold at least the 8 bytes of the fixed part.
pub(crate) fn state_of(payload: &[u8]) -> SheetState {
    SheetState::from_biff(payload[4])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_carries_its_name_state_and_a_patchable_position() {
        for state in [
            SheetState::Visible,
            SheetState::Hidden,
            SheetState::VeryHidden,
        ] {
            let mut stream = vec![0xAA];
            let at = push(&mut stream, "시트1", state);
            assert_eq!(at, 5);
            assert_eq!(u16::from_le_bytes([stream[1], stream[2]]), BOUNDSHEET);
            let len = u16::from_le_bytes([stream[3], stream[4]]) as usize;
            assert_eq!(stream.len(), 5 + len);
            assert_eq!(state_of(&stream[5..]), state);
            assert_eq!(stream[5 + 6], 3);
        }
    }
}
