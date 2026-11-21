//! The engine's OLE2 writer (PD-269, step 5). The same input writes the same bytes on every run and
//! every operating system, the directory follows the file format for the entries it leaves unused,
//! and the engine's own reader accepts every shape the writer can produce. Names, case-insensitive
//! duplicates and the directory budget are refused.

use easyoffice_formats::{EasyOfficeError, Ole2ContainerCodec, MAX_OLE2_DIR_ENTRIES};
use sha2::{Digest, Sha256};

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn pattern(len: usize, salt: u32) -> Vec<u8> {
    (0..len as u32)
        .map(|i| (i.wrapping_mul(2_654_435_761).wrapping_add(salt) >> 24) as u8)
        .collect()
}

/// Streams on both sides of the mini-stream cutoff (4,096 bytes) and an empty one.
fn mixed_streams() -> Vec<(String, Vec<u8>)> {
    vec![
        ("FileHeader".to_string(), pattern(256, 1)),
        ("DocInfo".to_string(), pattern(4_095, 2)),
        ("BodyText/Section0".to_string(), pattern(4_096, 3)),
        ("BodyText/Section1".to_string(), pattern(70_000, 4)),
        ("BinData/BIN0001.png".to_string(), pattern(300_000, 5)),
        ("Empty".to_string(), Vec::new()),
        ("한글/문서".to_string(), pattern(63, 6)),
    ]
}

fn assert_reads_back(streams: &[(String, Vec<u8>)], bytes: &[u8]) {
    let read = Ole2ContainerCodec::unpack(bytes).expect("the engine reads what it wrote");
    assert_eq!(read.len(), streams.len());
    for (name, data) in streams {
        let found = read.iter().find(|stream| &stream.name == name);
        assert_eq!(
            found.map(|stream| &stream.data),
            Some(data),
            "stream {name}"
        );
    }
}

#[test]
fn unused_directory_entries_hold_no_sibling_or_child_ids() {
    // Root and one stream use two of the four entries of the directory sector; the format wants
    // the unused ones to carry "no stream" (all ones) in their three id fields.
    let bytes = Ole2ContainerCodec::pack(&[("A".to_string(), vec![1, 2, 3])]).unwrap();
    let directory = 512;
    for entry in 2..4 {
        let at = directory + entry * 128;
        assert_eq!(bytes[at + 66], 0, "entry {entry} is free");
        assert_eq!(
            bytes[at + 68..at + 80],
            [0xFF; 12],
            "entry {entry} left, right and child ids"
        );
    }
}

#[test]
fn every_shape_the_writer_produces_reads_back() {
    let streams = mixed_streams();
    assert_reads_back(&streams, &Ole2ContainerCodec::pack(&streams).unwrap());
    // Over 7.14 MiB the FAT no longer fits in the header and continues in DIFAT sectors.
    let large = vec![("BodyText/Section0".to_string(), pattern(9 * 1024 * 1024, 7))];
    assert_reads_back(&large, &Ole2ContainerCodec::pack(&large).unwrap());
    let many: Vec<(String, Vec<u8>)> = (0..MAX_OLE2_DIR_ENTRIES - 2)
        .map(|i| (format!("S/n{i:05}"), pattern(1 + i % 5_000, i as u32)))
        .collect();
    assert_reads_back(&many, &Ole2ContainerCodec::pack(&many).unwrap());
}

#[test]
fn the_same_streams_write_the_same_bytes() {
    let streams = mixed_streams();
    let first = Ole2ContainerCodec::pack(&streams).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(30));
    assert_eq!(Ole2ContainerCodec::pack(&streams).unwrap(), first);
    // Fixed on purpose: the digest is the same on macOS, Windows and Linux.
    assert_eq!(
        hex_digest(&first),
        "ee29c6469799fb67794e57f9f2e074c21087c254928f24046e54fdcd32cc9816"
    );
}

#[test]
fn names_duplicates_and_entry_counts_are_refused() {
    let one = |name: &str| Ole2ContainerCodec::pack(&[(name.to_string(), vec![1])]);
    assert!(matches!(
        Ole2ContainerCodec::pack(&[]),
        Err(EasyOfficeError::EmptyPayload)
    ));
    assert!(matches!(
        one(" / "),
        Err(EasyOfficeError::InvalidArgument(_))
    ));
    assert!(matches!(
        one("a:b"),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
    assert!(matches!(
        one("S/.."),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
    assert!(matches!(
        one(&"x".repeat(32)),
        Err(EasyOfficeError::InvalidArgument(_))
    ));
    assert!(one(&"x".repeat(31)).is_ok());

    let pair = |a: &str, b: &str| {
        Ole2ContainerCodec::pack(&[(a.to_string(), vec![1]), (b.to_string(), vec![2])])
    };
    assert!(matches!(
        pair("Workbook", "WORKBOOK"),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
    assert!(matches!(
        pair("Dir/a", "dir"),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
    assert!(matches!(
        pair("café", "CAFÉ"),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
    assert!(pair("Dir/a", "DIR/b").is_ok());

    let entries = |count: usize| -> Vec<(String, Vec<u8>)> {
        (0..count).map(|i| (format!("n{i:05}"), vec![1])).collect()
    };
    assert!(Ole2ContainerCodec::pack(&entries(MAX_OLE2_DIR_ENTRIES - 1)).is_ok());
    assert!(matches!(
        Ole2ContainerCodec::pack(&entries(MAX_OLE2_DIR_ENTRIES)),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}
