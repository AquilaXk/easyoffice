//! Boundary of the engine's HWP 5.0 and HWPX readers (PD-269, step 5).
//!
//! The Hangul editor's parser (rhwp) opens these files in the same process as the engine and has no
//! time, record-count or sector-sharing limit (PD-269 item 13 lists the measurements), so the engine
//! keeps its own readers. These tests are the pass line any later reader has to meet: hostile OLE2
//! structures, a record flood, and shapes that pass the ZIP and inflate limits but make a reader slow.
//! The files are built here, nothing is read from disk.

use easyoffice_formats::deflate::deflate_raw;
use easyoffice_formats::{
    EasyOfficeError, Hwp5Codec, Hwp5DocumentPayload, Hwp5ParagraphRecord, HwpxCodec, HwpxPayload,
    Ole2ContainerCodec, UniversalFormatConverter, ZipArchiveCodec,
};
use std::time::{Duration, Instant};

const SECTOR: usize = 512;
const NO_SECTOR: u32 = 0xFFFF_FFFE;
/// A linear pass over these inputs takes milliseconds; the rhwp layout takes minutes (item 13).
const TIME_LIMIT: Duration = Duration::from_secs(15);

fn payload(text: &str) -> Hwp5DocumentPayload {
    Hwp5DocumentPayload {
        version: (5, 1, 0, 0),
        is_template: false,
        agency_name: String::new(),
        document_number: String::new(),
        paragraphs: vec![Hwp5ParagraphRecord {
            text: text.to_string(),
            style_name: "본문".to_string(),
            is_official_header: false,
        }],
        table_cells: Vec::new(),
    }
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn sector_offset(sector: u32) -> usize {
    (sector as usize + 1) * SECTOR
}

fn fat_slot(bytes: &[u8], sector: u32) -> usize {
    let fat_sector = u32_at(bytes, 76 + 4 * (sector as usize / 128));
    sector_offset(fat_sector) + 4 * (sector as usize % 128)
}

fn next_sector(bytes: &[u8], sector: u32) -> u32 {
    u32_at(bytes, fat_slot(bytes, sector))
}

fn set_next_sector(bytes: &mut [u8], sector: u32, next: u32) {
    let slot = fat_slot(bytes, sector);
    put_u32(bytes, slot, next);
}

/// Offset and directory index of the entry called `name`.
fn dir_entry(bytes: &[u8], name: &str) -> (usize, usize) {
    let wanted: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let (mut sector, mut index) = (u32_at(bytes, 48), 0);
    while sector != NO_SECTOR {
        for slot in 0..SECTOR / 128 {
            let at = sector_offset(sector) + slot * 128;
            if bytes[at..].starts_with(&wanted) && bytes[at + wanted.len()..][..2] == [0, 0] {
                return (at, index);
            }
            index += 1;
        }
        sector = next_sector(bytes, sector);
    }
    panic!("no directory entry {name}");
}

/// A file whose `BodyText/Section0` is long enough to live in the regular FAT (not the mini stream).
fn big_hwp() -> Vec<u8> {
    let text = "가나다라".repeat(2_000);
    let bytes = Hwp5Codec::pack(&payload(&text)).expect("pack");
    let (entry, _) = dir_entry(&bytes, "Section0");
    assert!(
        u32_at(&bytes, entry + 120) as usize > 4096,
        "stream is in the FAT"
    );
    bytes
}

fn assert_refused<T: std::fmt::Debug>(result: Result<T, EasyOfficeError>, what: &str) {
    assert!(
        matches!(
            result,
            Err(EasyOfficeError::SecurityViolation(_)
                | EasyOfficeError::CorruptedContainer(_)
                | EasyOfficeError::ResourceBudgetExceeded(_))
        ),
        "{what}: {result:?}"
    );
}

fn refused_quickly(bytes: &[u8], what: &str) {
    let start = Instant::now();
    assert_refused(Hwp5Codec::unpack(bytes), what);
    assert!(
        start.elapsed() < TIME_LIMIT,
        "{what} took {:?}",
        start.elapsed()
    );
}

#[test]
fn a_fat_chain_that_loops_is_refused() {
    let mut bytes = big_hwp();
    let (entry, _) = dir_entry(&bytes, "Section0");
    let start = u32_at(&bytes, entry + 116);
    set_next_sector(&mut bytes, start, start);
    refused_quickly(&bytes, "chain onto itself");

    let mut bytes = big_hwp();
    let (entry, _) = dir_entry(&bytes, "Section0");
    let start = u32_at(&bytes, entry + 116);
    let second = next_sector(&bytes, start);
    set_next_sector(&mut bytes, second, start);
    refused_quickly(&bytes, "chain back to its first sector");
}

#[test]
fn a_directory_that_loops_is_refused() {
    let mut chain = big_hwp();
    let first_dir = u32_at(&chain, 48);
    set_next_sector(&mut chain, first_dir, first_dir);
    refused_quickly(&chain, "directory sector chain onto itself");

    let mut sibling = big_hwp();
    let (entry, index) = dir_entry(&sibling, "Section0");
    put_u32(&mut sibling, entry + 68, index as u32);
    put_u32(&mut sibling, entry + 72, index as u32);
    refused_quickly(&sibling, "entry that is its own sibling");

    let mut child = big_hwp();
    let (entry, index) = dir_entry(&child, "BodyText");
    put_u32(&mut child, entry + 76, index as u32);
    refused_quickly(&child, "storage that is its own child");
}

#[test]
fn streams_that_share_sectors_are_refused() {
    // Two streams naming one chain would read the same megabytes twice; with 4,096 entries the
    // copies add up to gigabytes from a small file.
    let mut bytes = big_hwp();
    let (section, _) = dir_entry(&bytes, "Section0");
    let (doc_info, _) = dir_entry(&bytes, "DocInfo");
    let (start, size) = (u32_at(&bytes, section + 116), u32_at(&bytes, section + 120));
    put_u32(&mut bytes, doc_info + 116, start);
    put_u32(&mut bytes, doc_info + 120, size);
    refused_quickly(&bytes, "two streams on one chain");
}

#[test]
fn header_counts_and_stream_sizes_that_lie_are_refused() {
    let mut fat_count = big_hwp();
    put_u32(&mut fat_count, 44, 0x00FF_FFFF);
    refused_quickly(&fat_count, "16 million FAT sectors");

    let mut difat = big_hwp();
    let new_sector = (difat.len() / SECTOR - 1) as u32;
    let mut sector = vec![0xFF_u8; SECTOR];
    put_u32(&mut sector, SECTOR - 4, new_sector);
    difat.extend_from_slice(&sector);
    put_u32(&mut difat, 68, new_sector);
    put_u32(&mut difat, 72, 0xFFFF);
    refused_quickly(&difat, "DIFAT sector that links to itself");

    let mut size = big_hwp();
    let (entry, _) = dir_entry(&size, "Section0");
    put_u32(&mut size, entry + 120, 0x7FFF_FFFF);
    refused_quickly(&size, "stream that claims 2 GiB");

    let mut far = big_hwp();
    let (entry, _) = dir_entry(&far, "Section0");
    let start = u32_at(&far, entry + 116);
    set_next_sector(&mut far, start, 0x00FF_FFFF);
    refused_quickly(&far, "chain that leaves the file");
}

#[test]
fn a_mini_stream_chain_that_loops_is_refused() {
    let mut bytes = big_hwp();
    let (entry, _) = dir_entry(&bytes, "FileHeader");
    let start = u32_at(&bytes, entry + 116);
    let mini_fat = sector_offset(u32_at(&bytes, 60));
    // The 256-byte header is four 64-byte mini sectors.
    put_u32(&mut bytes, mini_fat + 4 * (start as usize + 3), start);
    refused_quickly(&bytes, "mini chain back to its start");
}

/// A file whose only section is `section` (already in the stored form of the header's flag).
fn hwp_with_section(section: &[u8]) -> Vec<u8> {
    let base = Hwp5Codec::pack(&payload("본문")).expect("pack");
    let mut streams = Ole2ContainerCodec::unpack(&base).expect("unpack");
    streams.retain(|stream| !stream.name.starts_with("BodyText/"));
    let mut entries: Vec<(String, Vec<u8>)> = streams
        .into_iter()
        .map(|stream| (stream.name, stream.data))
        .collect();
    entries.push(("BodyText/Section0".to_string(), section.to_vec()));
    Ole2ContainerCodec::pack(&entries).expect("pack sections")
}

#[test]
fn a_flood_of_empty_records_hits_the_record_budget() {
    // 600,000 records of four bytes: 2.3 MiB inflated. rhwp keeps a record object for each and
    // needs 64 bytes of memory for every four bytes of the file (item 13); the budget ends it first.
    let mut section = Vec::new();
    for _ in 0..600_000 {
        section.extend_from_slice(&Hwp5Codec::encode_record_header(67, 0, 0));
    }
    let start = Instant::now();
    match Hwp5Codec::unpack(&hwp_with_section(&section)) {
        Err(EasyOfficeError::ResourceBudgetExceeded(_)) => {}
        other => panic!("record flood: {other:?}"),
    }
    assert!(start.elapsed() < TIME_LIMIT);
}

#[test]
fn two_hundred_thousand_paragraphs_open_in_linear_time() {
    // 400,000 records stay under the budget. rhwp's page layout needs over a minute for this
    // file (item 13); the engine reads the records once.
    let mut section = Vec::new();
    for _ in 0..200_000 {
        section.extend_from_slice(&Hwp5Codec::encode_record_header(66, 0, 22));
        section.extend_from_slice(&[0_u8; 22]);
        section.extend_from_slice(&Hwp5Codec::encode_record_header(67, 1, 4));
        section.extend_from_slice(&[0x61, 0, 0x0D, 0]);
    }
    let bytes = hwp_with_section(&section);
    let start = Instant::now();
    let opened = Hwp5Codec::unpack(&bytes).expect("opens");
    assert_eq!(opened.paragraphs.len(), 200_000);
    assert!(start.elapsed() < TIME_LIMIT, "took {:?}", start.elapsed());
}

fn hwpx_with_section(body: &str) -> Vec<u8> {
    let base = HwpxCodec::pack(&HwpxPayload {
        title: "t".to_string(),
        is_template: false,
        official_agency: None,
        approval_lines: Vec::new(),
        paragraphs: vec!["본문".to_string()],
        tables: Vec::new(),
    })
    .expect("pack");
    let mut entries: Vec<(String, Vec<u8>)> = ZipArchiveCodec::unpack(&base, &Default::default())
        .expect("unpack")
        .into_iter()
        .map(|entry| (entry.path, entry.data))
        .collect();
    let section = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><hs:sec xmlns:hs=\"http://www.owpml.org/owpml/2011/section\" xmlns:hp=\"http://www.owpml.org/owpml/2011/paragraph\">{body}</hs:sec>"
    );
    for entry in &mut entries {
        if entry.0 == "Contents/section0.xml" {
            entry.1 = section.clone().into_bytes();
        }
    }
    ZipArchiveCodec::pack_compressed(&entries).expect("pack package")
}

/// Text that does not repeat, so the 100:1 ratio limit of the ZIP guard lets the section through.
fn unique(index: usize) -> String {
    format!("{:x}", index.wrapping_mul(2_654_435_761) ^ (index << 7))
}

#[test]
fn a_hundred_thousand_paragraphs_in_a_section_open_in_linear_time() {
    // 1 MB of ZIP that the engine opens in under a second; rhwp's page layout needs 34 s (item 13).
    let body: String = (0..100_000)
        .map(|i| format!("<hp:p><hp:run><hp:t>{}</hp:t></hp:run></hp:p>", unique(i)))
        .collect();
    let bytes = hwpx_with_section(&body);
    let start = Instant::now();
    let (_, model) = UniversalFormatConverter::decode(&bytes, Some("hwpx")).expect("opens");
    assert!(!model.paragraphs.is_empty());
    assert!(start.elapsed() < TIME_LIMIT, "took {:?}", start.elapsed());
}

#[test]
fn the_engine_writers_embed_no_clock() {
    // An OLE2 writer that stamps the directory with the current time (the crate rhwp uses does)
    // makes every save differ; the engine's bytes depend on the document alone.
    let streams = [("Stream".to_string(), vec![1_u8, 2, 3])];
    let doc = payload("본문");
    let hwpx = HwpxPayload {
        title: "t".to_string(),
        is_template: false,
        official_agency: None,
        approval_lines: Vec::new(),
        paragraphs: vec!["본문".to_string()],
        tables: Vec::new(),
    };
    let first = (
        Ole2ContainerCodec::pack(&streams).unwrap(),
        Hwp5Codec::pack_compressed(&doc).unwrap(),
        HwpxCodec::pack(&hwpx).unwrap(),
        deflate_raw(b"same bytes"),
    );
    std::thread::sleep(Duration::from_millis(30));
    let second = (
        Ole2ContainerCodec::pack(&streams).unwrap(),
        Hwp5Codec::pack_compressed(&doc).unwrap(),
        HwpxCodec::pack(&hwpx).unwrap(),
        deflate_raw(b"same bytes"),
    );
    assert_eq!(first, second);
}
