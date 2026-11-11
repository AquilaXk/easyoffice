//! FORMULA records of the `.xls` writer follow MS-XLS 2.4.127 (PD-178): a 22-byte fixed part
//! (cell, format index, cached result, flags, chain, token count) and the postfix tokens (`rgce`)
//! of the expression, not its text. The expected bytes are written out from the specification, not
//! read back with the engine's own reader.

use easyoffice_formats::{
    LegacyOle2Codec, LegacyXlsPayload, Ole2ContainerCodec, XlsxCachedValue, XlsxFormulaCache,
};

const FORMULA: u16 = 0x0006;
const STRING: u16 = 0x0207;

/// `(opcode, body)` of every record of the workbook stream, in order.
fn records(xls: &[u8]) -> Vec<(u16, Vec<u8>)> {
    let data = Ole2ContainerCodec::unpack(xls)
        .unwrap()
        .into_iter()
        .find(|s| s.name == "Workbook")
        .unwrap()
        .data;
    let mut out = Vec::new();
    let mut at = 0;
    while at + 4 <= data.len() {
        let opcode = u16::from_le_bytes([data[at], data[at + 1]]);
        let len = usize::from(u16::from_le_bytes([data[at + 2], data[at + 3]]));
        out.push((opcode, data[at + 4..at + 4 + len].to_vec()));
        at += 4 + len;
    }
    out
}

fn payload(cells: &[(u32, u32, &str)]) -> LegacyXlsPayload {
    LegacyXlsPayload {
        sheet_name: "S1".to_string(),
        cells: cells
            .iter()
            .map(|(r, c, t)| (*r, *c, t.to_string()))
            .collect(),
    }
}

#[test]
fn formula_record_has_the_fixed_part_and_tokens_not_text() {
    let xls =
        LegacyOle2Codec::pack_xls(&payload(&[(0, 0, "1"), (0, 1, "2"), (0, 2, "=A1+B1")])).unwrap();
    let formulas: Vec<_> = records(&xls)
        .into_iter()
        .filter(|(op, _)| *op == FORMULA)
        .collect();
    assert_eq!(formulas.len(), 1);
    let body = &formulas[0].1;
    let rgce = [0x44, 0, 0, 0, 0xC0, 0x44, 0, 0, 1, 0xC0, 0x03];
    assert_eq!(
        body.len(),
        22 + rgce.len(),
        "22 fixed bytes then the tokens"
    );
    assert_eq!(u16::from_le_bytes([body[0], body[1]]), 0, "row");
    assert_eq!(u16::from_le_bytes([body[2], body[3]]), 2, "column");
    assert_eq!(
        u16::from_le_bytes([body[20], body[21]]) as usize,
        rgce.len(),
        "cce counts token bytes"
    );
    assert_eq!(&body[22..], rgce);
    assert_eq!(&body[6..14], 0.0f64.to_le_bytes(), "no cached result given");
    assert_eq!(
        u16::from_le_bytes([body[14], body[15]]) & 1,
        1,
        "fAlwaysCalc"
    );
}

#[test]
fn cached_results_are_numbers_booleans_or_a_string_record() {
    let mut cache = XlsxFormulaCache::new();
    cache.insert((0, 0), XlsxCachedValue::Number(42.5));
    cache.insert((1, 0), XlsxCachedValue::Boolean(true));
    cache.insert((2, 0), XlsxCachedValue::Text("합격".to_string()));
    let xls = LegacyOle2Codec::pack_xls_multi_with_formula_cache(
        &[payload(&[
            (0, 0, "=1+1"),
            (1, 0, "=1=1"),
            (2, 0, "=\"합격\""),
        ])],
        &[cache],
    )
    .unwrap();
    let all = records(&xls);
    let formulas: Vec<_> = all.iter().filter(|(op, _)| *op == FORMULA).collect();
    assert_eq!(formulas.len(), 3);
    assert_eq!(&formulas[0].1[6..14], 42.5f64.to_le_bytes());
    assert_eq!(&formulas[1].1[6..14], [1, 0, 1, 0, 0, 0, 0xFF, 0xFF]);
    assert_eq!(&formulas[2].1[6..14], [0, 0, 0, 0, 0, 0, 0xFF, 0xFF]);
    // The text result is in the STRING record right after its FORMULA (MS-XLS 2.4.269).
    let at = all.iter().position(|(op, _)| *op == STRING).unwrap();
    assert_eq!(all[at - 1].0, FORMULA);
    let body = &all[at].1;
    assert_eq!(u16::from_le_bytes([body[0], body[1]]), 2);
    assert_eq!(body[2], 1, "UTF-16");
    assert_eq!(&body[3..], [0x69, 0xD5, 0xA9, 0xAC]);
}

#[test]
fn formulas_read_back_as_text_and_values_stay_values() {
    let cells = [
        (0, 0, "3"),
        (0, 1, "=SUM(A1:A3)*2"),
        (1, 0, "=IF(A1>=2,\"큰\",\"작은\")"),
    ];
    let xls = LegacyOle2Codec::pack_xls(&payload(&cells)).unwrap();
    let back = LegacyOle2Codec::unpack_xls(&xls).unwrap();
    for (row, col, text) in cells {
        assert!(
            back.cells.contains(&(row, col, text.to_string())),
            "{text}: {:?}",
            back.cells
        );
    }
}
