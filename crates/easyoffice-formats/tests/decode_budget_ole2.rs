//! Legacy OLE2 documents (BIFF8 workbook, Word, Hangul 5.0) whose records point at the same bytes
//! over and over must hit the decoded-byte budget, not memory (PD-175).

use easyoffice_formats::{EasyOfficeError, LegacyOle2Codec, LegacyXlsPayload, Ole2ContainerCodec};

fn assert_budget_error<T>(result: Result<T, EasyOfficeError>, what: &str) {
    match result {
        Err(EasyOfficeError::ResourceBudgetExceeded(_)) => {}
        Err(other) => panic!("{what}: expected ResourceBudgetExceeded, got {other:?}"),
        Ok(_) => panic!("{what}: expected ResourceBudgetExceeded, the input was accepted"),
    }
}

#[test]
fn many_label_cells_naming_one_big_shared_string_hit_the_decoded_budget() {
    // One 8,000-character string and 40,000 14-byte LABELSST records: 320 MB of copies out of a
    // workbook stream of about 570 KB.
    let text = "A".repeat(8_000);
    let cells = (0..40_000u32).map(|row| (row, 0, text.clone())).collect();
    let packed = LegacyOle2Codec::pack_xls_multi(&[LegacyXlsPayload {
        sheet_name: "S".to_string(),
        cells,
    }])
    .expect("pack xls fixture");
    assert_budget_error(
        LegacyOle2Codec::unpack_xls_multi(&packed),
        "LABELSST fan-out",
    );
}

#[test]
fn an_ordinary_sheet_of_text_cells_still_opens() {
    let cells = (0..20_000u32)
        .map(|row| (row, row % 8, format!("cell {row}")))
        .collect();
    let packed = LegacyOle2Codec::pack_xls_multi(&[LegacyXlsPayload {
        sheet_name: "S".to_string(),
        cells,
    }])
    .expect("pack xls fixture");
    let sheets = LegacyOle2Codec::unpack_xls_multi(&packed).expect("ordinary xls opens");
    assert_eq!(sheets[0].cells.len(), 20_000);
}

/// A Word stream whose piece table lists `pieces` 8-bit pieces, each naming the same
/// `piece_chars` characters at offset 512 of the stream.
fn doc_with_repeated_pieces(pieces: usize, piece_chars: usize) -> Vec<u8> {
    let text_at = 512usize;
    let mut word_doc = vec![0u8; text_at + piece_chars];
    word_doc[0..2].copy_from_slice(&0xA5ECu16.to_le_bytes());
    word_doc[2..4].copy_from_slice(&0x00C1u16.to_le_bytes());
    word_doc[0x000A..0x000C].copy_from_slice(&0x0200u16.to_le_bytes()); // 1Table
    word_doc[text_at..].fill(b'A');

    let mut clx = vec![0x02u8];
    let plc_len = 4 * (pieces + 1) + 8 * pieces;
    clx.extend_from_slice(&(plc_len as u32).to_le_bytes());
    for i in 0..=pieces {
        clx.extend_from_slice(&((i * piece_chars) as u32).to_le_bytes());
    }
    let compressed_fc = ((text_at as u32) * 2) | 0x4000_0000;
    for _ in 0..pieces {
        clx.extend_from_slice(&0u16.to_le_bytes());
        clx.extend_from_slice(&compressed_fc.to_le_bytes());
        clx.extend_from_slice(&0u16.to_le_bytes());
    }
    word_doc[0x01A6..0x01AA].copy_from_slice(&(clx.len() as u32).to_le_bytes());
    Ole2ContainerCodec::pack(&[
        (["Wor", "dDocument"].concat(), word_doc),
        ("1Table".to_string(), clx),
    ])
    .expect("pack doc fixture")
}

#[test]
fn a_piece_table_naming_one_text_run_many_times_hits_the_decoded_budget() {
    // 300 pieces x 1 Mi characters of the same 1 MiB: 300 MB of text out of a 1 MiB stream.
    let bytes = doc_with_repeated_pieces(300, 1024 * 1024);
    assert_budget_error(LegacyOle2Codec::unpack_doc(&bytes), "piece table fan-out");
}

#[test]
fn an_ordinary_piece_table_still_opens() {
    let bytes = doc_with_repeated_pieces(50, 100);
    let doc = LegacyOle2Codec::unpack_doc(&bytes).expect("small piece table opens");
    assert_eq!(doc.paragraphs.concat().len(), 5_000);
}
