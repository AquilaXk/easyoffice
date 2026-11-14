//! The hidden state of an XLS sheet follows the worksheet it belongs to, not the position of its `BOUNDSHEET`
//! record: a hidden chart or macro sheet before the worksheets does not take their names or states (PD-209).

use easyoffice_formats::{
    LegacyOle2Codec, LegacyXlsPayload, Ole2ContainerCodec, SheetState, SheetStyling,
};

const BOUNDSHEET: u16 = 0x0085;

fn payload(name: &str) -> LegacyXlsPayload {
    LegacyXlsPayload {
        sheet_name: name.to_string(),
        cells: vec![(0, 0, name.to_string())],
    }
}

fn state(state: SheetState) -> SheetStyling {
    let mut styling = SheetStyling::default();
    state.apply(&mut styling);
    styling
}

/// A `BOUNDSHEET` record of kind `dt` named `name` with `hs_state`.
fn record(name: &str, dt: u8, hs_state: u8) -> Vec<u8> {
    let units: Vec<u16> = name.encode_utf16().collect();
    let mut out = BOUNDSHEET.to_le_bytes().to_vec();
    out.extend(((8 + units.len() * 2) as u16).to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend([hs_state, dt, units.len() as u8, 1]);
    for unit in units {
        out.extend(unit.to_le_bytes());
    }
    out
}

/// `xls` with `extra` written before its first `BOUNDSHEET`.
fn with_record_before_first_sheet(xls: &[u8], extra: &[u8]) -> Vec<u8> {
    let mut streams = Ole2ContainerCodec::unpack(xls).unwrap();
    let wb = streams
        .iter_mut()
        .find(|s| s.name == "Workbook" || s.name == "Book")
        .unwrap();
    let mut at = 0;
    while at + 4 <= wb.data.len() {
        let opcode = u16::from_le_bytes([wb.data[at], wb.data[at + 1]]);
        let len = u16::from_le_bytes([wb.data[at + 2], wb.data[at + 3]]) as usize;
        if opcode == BOUNDSHEET {
            wb.data.splice(at..at, extra.iter().copied());
            break;
        }
        at += 4 + len;
    }
    let entries: Vec<(String, Vec<u8>)> = streams.into_iter().map(|s| (s.name, s.data)).collect();
    Ole2ContainerCodec::pack(&entries).unwrap()
}

#[test]
fn a_hidden_non_worksheet_before_the_sheets_leaves_their_names_and_states_alone() {
    let sheets = [payload("첫째"), payload("둘째"), payload("셋째")];
    let stylings = [
        state(SheetState::Visible),
        state(SheetState::Hidden),
        state(SheetState::VeryHidden),
    ];
    let xls = LegacyOle2Codec::pack_xls_multi_values_styled(&sheets, &[], &stylings).unwrap();
    let read = |bytes: &[u8]| -> Vec<(String, SheetState)> {
        LegacyOle2Codec::unpack_xls_with_styling(bytes)
            .unwrap()
            .1
            .into_iter()
            .map(|(name, styling)| (name, SheetState::of(&styling)))
            .collect()
    };
    let want = vec![
        ("첫째".to_string(), SheetState::Visible),
        ("둘째".to_string(), SheetState::Hidden),
        ("셋째".to_string(), SheetState::VeryHidden),
    ];
    assert_eq!(read(&xls), want);
    // A hidden chart sheet (dt 2), a macro sheet (dt 1) and a module (dt 6) first.
    for dt in [1u8, 2, 6] {
        let patched = with_record_before_first_sheet(&xls, &record("매크로", dt, 2));
        assert_eq!(read(&patched), want, "dt {dt}");
    }
}
