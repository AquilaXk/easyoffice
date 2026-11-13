//! A sheet with ten thousand links writes and reads back whole, and the links cost little next to the cells (PD-218).
//! The timings print with `--nocapture`; the test itself asserts the round trip and a generous ratio.

use easyoffice_formats::{CellLink, OdfCodec, OdsUnpacked, OoxmlCodec, SheetLinks, SheetStyling};
use std::time::Instant;

const ROWS: u32 = 10_000;

fn sheet() -> Vec<OdsUnpacked> {
    let cells = (0..ROWS)
        .flat_map(|row| {
            [
                (row, 0, format!("항목 {row}")),
                (row, 1, (row * 3).to_string()),
            ]
        })
        .collect();
    vec![("Data".to_string(), cells)]
}

fn linked() -> Vec<SheetStyling> {
    let mut links = SheetLinks::new();
    for row in 0..ROWS {
        let target = if row % 4 == 0 {
            format!("#Data!B{}", row + 1)
        } else {
            format!("https://example.com/items/{row}?ref=sheet&lang=ko")
        };
        links.insert((row, 0), CellLink::new(&target, "").unwrap());
    }
    vec![SheetStyling {
        links,
        ..SheetStyling::default()
    }]
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

#[test]
fn ten_thousand_links_round_trip_in_both_formats_without_slowing_the_file_down() {
    let sheets = sheet();
    let with = linked();
    let plain = vec![SheetStyling::default()];

    let t = Instant::now();
    let xlsx_plain = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &plain).unwrap();
    let write_plain = ms(t);
    let t = Instant::now();
    let xlsx = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &[], &with).unwrap();
    let write_links = ms(t);
    let t = Instant::now();
    OoxmlCodec::unpack_xlsx_with_styling(&xlsx_plain).unwrap();
    let read_plain = ms(t);
    let t = Instant::now();
    let (_, back) = OoxmlCodec::unpack_xlsx_with_styling(&xlsx).unwrap();
    let read_links = ms(t);
    assert_eq!(back[0].1.links, with[0].links);
    assert_eq!(back[0].1.links_rejected, 0);

    let t = Instant::now();
    let ods = OdfCodec::pack_ods_multi_styled(&sheets, &with).unwrap();
    let ods_write = ms(t);
    let t = Instant::now();
    let (_, back) = OdfCodec::unpack_ods_with_styling(&ods).unwrap();
    let ods_read = ms(t);
    assert_eq!(back[0].1.links, with[0].links);

    println!(
        "xlsx write {write_plain:.1} -> {write_links:.1} ms, read {read_plain:.1} -> {read_links:.1} ms; ods write {ods_write:.1} ms, read {ods_read:.1} ms; xlsx {} -> {} bytes",
        xlsx_plain.len(),
        xlsx.len()
    );
    // A debug build and a busy machine are slow; the links must not be the bulk of the work.
    assert!(write_links < write_plain * 4.0 + 500.0);
    assert!(read_links < read_plain * 4.0 + 500.0);
}
