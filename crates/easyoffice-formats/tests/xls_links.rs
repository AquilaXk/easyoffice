//! Hyperlinks in `.xls` (PD-223): the `HLink` and `HLinkTooltip` records follow MS-XLS 2.4.140 and 2.4.141.
//! The expected bytes are written out from the specification, not read back with the engine's own reader, and
//! the hostile records go through the whole reader.

use easyoffice_formats::{
    read_sheet_styling, CellLink, DocumentFormat, LegacyOle2Codec, LegacyXlsPayload,
    Ole2ContainerCodec, SheetLinks,
};

const HLINK: u16 = 0x01B8;
const TOOLTIP: u16 = 0x0800;
const EOF: u16 = 0x000A;

const STD_LINK: [u8; 16] = [
    0xD0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B,
];
const URL_MONIKER: [u8; 16] = [
    0xE0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B,
];
const FILE_MONIKER: [u8; 16] = [
    0x03, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];

fn payload(name: &str) -> LegacyXlsPayload {
    LegacyXlsPayload {
        sheet_name: name.to_string(),
        cells: vec![(0, 0, "홈페이지".to_string()), (1, 0, "x".to_string())],
    }
}

fn link(target: &str, tip: &str) -> CellLink {
    CellLink::new(target, tip).unwrap()
}

fn utf16z(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn hyperlink_string(text: &str) -> Vec<u8> {
    let mut out = (text.encode_utf16().count() as u32 + 1)
        .to_le_bytes()
        .to_vec();
    out.extend(utf16z(text));
    out
}

fn area(rows: (u16, u16), cols: (u16, u16)) -> Vec<u8> {
    [rows.0, rows.1, cols.0, cols.1]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn record(opcode: u16, data: &[u8]) -> Vec<u8> {
    let mut out = opcode.to_le_bytes().to_vec();
    out.extend((data.len() as u16).to_le_bytes());
    out.extend(data);
    out
}

/// An `HLink` payload the spreadsheet program writes for a URL: display name, URL moniker with its tail.
fn excel_url_link(at: (u16, u16), display: &str, url: &str) -> Vec<u8> {
    let mut out = area((at.0, at.0), (at.1, at.1));
    out.extend(STD_LINK);
    out.extend(2u32.to_le_bytes());
    out.extend(0x17u32.to_le_bytes()); // moniker, absolute, site gave display name, display name
    out.extend(hyperlink_string(display));
    out.extend(URL_MONIKER);
    let url = utf16z(url);
    out.extend((url.len() as u32 + 24).to_le_bytes());
    out.extend(url);
    out.extend([
        0x79, 0x58, 0x81, 0xF4, 0x3B, 0x1D, 0x7F, 0x48, 0xAF, 0x2C, 0x82, 0x5D, 0xC4, 0x85, 0x27,
        0x63, 0, 0, 0, 0, 0xA5, 0xAB, 0, 0,
    ]);
    out
}

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

/// A workbook of one sheet whose worksheet substream carries `extra` records before its last EOF.
fn with_records(extra: &[Vec<u8>]) -> Vec<u8> {
    let xls = LegacyOle2Codec::pack_xls_multi_values(&[payload("S1")]).unwrap();
    let data = Ole2ContainerCodec::unpack(&xls)
        .unwrap()
        .into_iter()
        .find(|s| s.name == "Workbook")
        .unwrap()
        .data;
    let mut stream = data[..data.len() - 4].to_vec();
    for rec in extra {
        stream.extend(rec);
    }
    stream.extend(record(EOF, &[]));
    Ole2ContainerCodec::pack(&[("Workbook".to_string(), stream)]).unwrap()
}

fn read(xls: &[u8]) -> (Vec<(u32, u32, String)>, u32) {
    let (_, st) = read_sheet_styling(xls, DocumentFormat::Xls)
        .unwrap()
        .remove(0);
    let links = st
        .links
        .iter()
        .map(|(&(r, c), l)| (r, c, format!("{}|{}", l.target, l.tip)))
        .collect();
    (links, st.links_rejected)
}

#[test]
fn links_survive_a_save_and_reopen() {
    let mut first = SheetLinks::new();
    first.insert((0, 0), link("https://example.com/a?b=1#c", "화면 설명"));
    first.insert((1, 0), link("mailto:me@example.com", ""));
    first.insert((5, 3), link("#Sheet2!B2", ""));
    first.insert((6, 3), link("#'시트 2'!A1:B3", "안쪽 팁"));
    let mut second = SheetLinks::new();
    second.insert((0, 0), link("https://example.com/", ""));
    let xls = LegacyOle2Codec::pack_xls_multi_values_linked(
        &[payload("첫째"), payload("둘째")],
        &[first.clone(), second.clone()],
    )
    .unwrap();
    let back = read_sheet_styling(&xls, DocumentFormat::Xls).unwrap();
    assert_eq!(back.len(), 2);
    assert_eq!((back[0].0.as_str(), back[1].0.as_str()), ("첫째", "둘째"));
    assert_eq!(back[0].1.links, first);
    assert_eq!(back[1].1.links, second);
    assert_eq!(back[0].1.links_rejected, 0);
    // The cells are untouched.
    let sheets = LegacyOle2Codec::unpack_xls_multi(&xls).unwrap();
    assert_eq!(sheets[0].cells, payload("첫째").cells);
}

#[test]
fn the_records_follow_the_specification() {
    let mut links = SheetLinks::new();
    links.insert((2, 1), link("https://a.b/", "팁"));
    links.insert((3, 1), link("#Sheet1!B2", ""));
    let xls = LegacyOle2Codec::pack_xls_multi_values_linked(&[payload("S1")], &[links]).unwrap();
    let found: Vec<_> = records(&xls)
        .into_iter()
        .filter(|(op, _)| *op == HLINK || *op == TOOLTIP)
        .collect();
    assert_eq!(found.len(), 3);

    let mut expected_url = area((2, 2), (1, 1));
    expected_url.extend(STD_LINK);
    expected_url.extend(2u32.to_le_bytes());
    expected_url.extend(0x03u32.to_le_bytes()); // has moniker, absolute
    expected_url.extend(URL_MONIKER);
    let url = utf16z("https://a.b/");
    expected_url.extend((url.len() as u32 + 24).to_le_bytes());
    expected_url.extend(url);
    expected_url.extend([
        0x79, 0x58, 0x81, 0xF4, 0x3B, 0x1D, 0x7F, 0x48, 0xAF, 0x2C, 0x82, 0x5D, 0xC4, 0x85, 0x27,
        0x63, 0, 0, 0, 0, 0xA5, 0xAB, 0, 0,
    ]);
    assert_eq!(found[0], (HLINK, expected_url));

    let mut expected_tip = TOOLTIP.to_le_bytes().to_vec();
    expected_tip.extend(0u16.to_le_bytes());
    expected_tip.extend(area((2, 2), (1, 1)));
    expected_tip.extend(utf16z("팁"));
    assert_eq!(found[1], (TOOLTIP, expected_tip));

    let mut expected_place = area((3, 3), (1, 1));
    expected_place.extend(STD_LINK);
    expected_place.extend(2u32.to_le_bytes());
    expected_place.extend(0x08u32.to_le_bytes()); // has location
    expected_place.extend(hyperlink_string("Sheet1!B2"));
    assert_eq!(found[2], (HLINK, expected_place));
}

#[test]
fn a_record_the_spreadsheet_program_wrote_is_read() {
    let mut place = area((4, 4), (2, 2));
    place.extend(STD_LINK);
    place.extend(2u32.to_le_bytes());
    place.extend(0x1Cu32.to_le_bytes()); // site gave display name, location, display name
    place.extend(hyperlink_string("표시"));
    place.extend(hyperlink_string("Sheet1!A1"));
    let mut with_fragment = excel_url_link((7, 0), "x", "https://a.b/page");
    // The same record with a location after the moniker.
    with_fragment[28..32].copy_from_slice(&0x1Fu32.to_le_bytes());
    with_fragment.extend(hyperlink_string("section"));
    let mut tip = TOOLTIP.to_le_bytes().to_vec();
    tip.extend(0u16.to_le_bytes());
    tip.extend(area((0, 0), (0, 0)));
    tip.extend(utf16z("설명"));
    let xls = with_records(&[
        record(
            HLINK,
            &excel_url_link((0, 0), "홈페이지", "https://example.com/"),
        ),
        record(TOOLTIP, &tip),
        record(HLINK, &place),
        record(HLINK, &with_fragment),
    ]);
    let (links, rejected) = read(&xls);
    assert_eq!(
        links,
        [
            (0, 0, "https://example.com/|설명".to_string()),
            (4, 2, "#Sheet1!A1|".to_string()),
            (7, 0, "https://a.b/page#section|".to_string()),
        ]
    );
    assert_eq!(rejected, 0);
}

#[test]
fn a_range_gives_its_link_to_every_cell_and_a_tooltip_of_another_range_is_ignored() {
    let mut range = excel_url_link((1, 0), "x", "https://a.b/");
    range[..8].copy_from_slice(&area((1, 2), (0, 1)));
    let mut stray = TOOLTIP.to_le_bytes().to_vec();
    stray.extend(0u16.to_le_bytes());
    stray.extend(area((9, 9), (9, 9)));
    stray.extend(utf16z("다른 칸"));
    let xls = with_records(&[record(HLINK, &range), record(TOOLTIP, &stray)]);
    let (links, rejected) = read(&xls);
    let cells: Vec<_> = links.iter().map(|(r, c, _)| (*r, *c)).collect();
    assert_eq!(cells, [(1, 0), (1, 1), (2, 0), (2, 1)]);
    assert!(links.iter().all(|(_, _, l)| l == "https://a.b/|"));
    assert_eq!(rejected, 0);
}

#[test]
fn addresses_the_policy_refuses_are_counted_and_dropped() {
    let mut records_in = Vec::new();
    for (row, url) in [
        (0u16, "javascript:alert(1)"),
        (1, "file:///etc/passwd"),
        (2, "data:text/html,x"),
        (3, "ftp://a.b/"),
        (4, "relative/path.xlsx"),
        (5, "https://a b/"),
    ] {
        records_in.push(record(HLINK, &excel_url_link((row, 0), "x", url)));
    }
    // A moniker saved as a string takes the same road.
    let mut as_string = area((6, 6), (0, 0));
    as_string.extend(STD_LINK);
    as_string.extend(2u32.to_le_bytes());
    as_string.extend(0x101u32.to_le_bytes());
    as_string.extend(hyperlink_string("\\\\server\\share\\a.xlsx"));
    records_in.push(record(HLINK, &as_string));
    let mut as_url = area((7, 7), (0, 0));
    as_url.extend(STD_LINK);
    as_url.extend(2u32.to_le_bytes());
    as_url.extend(0x101u32.to_le_bytes());
    as_url.extend(hyperlink_string("https://ok.example/"));
    records_in.push(record(HLINK, &as_url));
    // A file moniker.
    let mut file = area((8, 8), (0, 0));
    file.extend(STD_LINK);
    file.extend(2u32.to_le_bytes());
    file.extend(0x01u32.to_le_bytes());
    file.extend(FILE_MONIKER);
    file.extend([0u8; 40]);
    records_in.push(record(HLINK, &file));
    let (links, rejected) = read(&with_records(&records_in));
    assert_eq!(links, [(7, 0, "https://ok.example/|".to_string())]);
    assert_eq!(rejected, 8);
}

#[test]
fn malformed_records_are_dropped_without_failing_the_open() {
    let good = excel_url_link((0, 0), "x", "https://a.b/");
    let mut bad_clsid = good.clone();
    bad_clsid[8] ^= 1;
    let mut bad_version = good.clone();
    bad_version[24] = 3;
    let mut reversed = good.clone();
    reversed[..8].copy_from_slice(&area((5, 1), (0, 0)));
    // A length that claims more than the record holds.
    let mut long_string = good.clone();
    long_string[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut long_moniker = good.clone();
    let moniker_length_at = 24 + 8 + hyperlink_string("x").len() + 16;
    long_moniker[moniker_length_at..moniker_length_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    // An address with no terminating zero, and an odd number of bytes.
    let mut no_zero = good.clone();
    let url_start = moniker_length_at + 4;
    no_zero.truncate(url_start + 4);
    no_zero[moniker_length_at..moniker_length_at + 4].copy_from_slice(&4u32.to_le_bytes());
    no_zero[url_start..url_start + 4].copy_from_slice(&[b'h', 0, b't', 0]);
    let mut records_in: Vec<Vec<u8>> = [
        bad_clsid,
        bad_version,
        reversed,
        long_string,
        long_moniker,
        no_zero,
    ]
    .iter()
    .map(|data| record(HLINK, data))
    .collect();
    for cut in [0, 1, 7, 8, 24, 31, 35, 60] {
        records_in.push(record(HLINK, &good[..cut]));
    }
    records_in.push(record(HLINK, &[]));
    let (links, rejected) = read(&with_records(&records_in));
    assert_eq!(links, Vec::<(u32, u32, String)>::new());
    assert_eq!(rejected as usize, records_in.len());
    // The same file still opens with its cells.
    let sheets = LegacyOle2Codec::unpack_xls_multi(&with_records(&records_in)).unwrap();
    assert_eq!(sheets[0].cells, payload("S1").cells);
}

#[test]
fn a_huge_range_and_a_flood_of_records_stay_inside_the_bounds() {
    // The whole grid in one record links its first cell only.
    let mut whole = excel_url_link((0, 0), "x", "https://a.b/");
    whole[..8].copy_from_slice(&area((0, 65_535), (0, 255)));
    let (links, rejected) = read(&with_records(&[record(HLINK, &whole)]));
    assert_eq!(links, [(0, 0, "https://a.b/|".to_string())]);
    assert_eq!(rejected, 1);

    // Many records over the same big range walk it once: the first cell has its link, the walk stops there.
    let mut band = excel_url_link((0, 0), "x", "https://a.b/");
    band[..8].copy_from_slice(&area((0, 99), (0, 99)));
    let flood: Vec<Vec<u8>> = (0..20_000).map(|_| record(HLINK, &band)).collect();
    let start = std::time::Instant::now();
    let (links, _) = read(&with_records(&flood));
    assert_eq!(links.len(), 10_000);
    assert!(start.elapsed().as_secs() < 10, "{:?}", start.elapsed());

    // Single cells past the per-sheet limit are counted, not kept.
    let cells: Vec<Vec<u8>> = (0..70_000u32)
        .map(|n| {
            let mut one = excel_url_link((0, 0), "", "https://a.b/");
            one[..8].copy_from_slice(&area(
                ((n / 256) as u16, (n / 256) as u16),
                ((n % 256) as u16, (n % 256) as u16),
            ));
            record(HLINK, &one)
        })
        .collect();
    let (links, rejected) = read(&with_records(&cells));
    assert_eq!(links.len(), easyoffice_formats::MAX_LINKS_PER_SHEET);
    assert!(rejected >= 1);
}

#[test]
fn a_link_outside_the_grid_is_refused_on_save() {
    for at in [(65_536, 0), (0, 256)] {
        let mut links = SheetLinks::new();
        links.insert(at, link("https://a.b/", ""));
        assert!(
            LegacyOle2Codec::pack_xls_multi_values_linked(&[payload("S1")], &[links]).is_err(),
            "{at:?}"
        );
    }
    let mut last = SheetLinks::new();
    last.insert((65_535, 255), link("https://a.b/", ""));
    let xls =
        LegacyOle2Codec::pack_xls_multi_values_linked(&[payload("S1")], &[last.clone()]).unwrap();
    assert_eq!(
        read_sheet_styling(&xls, DocumentFormat::Xls).unwrap()[0]
            .1
            .links,
        last
    );
}

#[test]
fn a_workbook_without_links_reads_no_links() {
    let xls = LegacyOle2Codec::pack_xls_multi_values(&[payload("S1")]).unwrap();
    assert_eq!(read(&xls), (Vec::new(), 0));
}

#[test]
fn a_range_past_the_biff8_columns_is_turned_away() {
    let mut wide = excel_url_link((0, 0), "x", "https://a.b/");
    wide[..8].copy_from_slice(&area((0, 0), (40_000, 40_000)));
    let mut edge = excel_url_link((0, 0), "x", "https://a.b/");
    edge[..8].copy_from_slice(&area((1, 1), (255, 255)));
    let (links, rejected) = read(&with_records(&[record(HLINK, &wide), record(HLINK, &edge)]));
    assert_eq!(links, [(1, 255, "https://a.b/|".to_string())]);
    assert_eq!(rejected, 1);
}

#[test]
fn an_address_whose_record_would_pass_8224_bytes_is_refused_on_save() {
    let emoji = format!("https://a.b/{}", "😀".repeat(2_036));
    let long = CellLink::new(&emoji, "").unwrap();
    assert!(!long.fits_xls());
    assert!(CellLink::new("https://a.b/x", &"😀".repeat(255))
        .unwrap()
        .fits_xls());
    let mut links = SheetLinks::new();
    links.insert((0, 0), long);
    assert!(LegacyOle2Codec::pack_xls_multi_values_linked(&[payload("S1")], &[links]).is_err());
    // The longest plain address (2,048 characters) fits.
    let plain = CellLink::new(&format!("https://a.b/{}", "x".repeat(2_030)), "팁").unwrap();
    assert!(plain.fits_xls());
}
