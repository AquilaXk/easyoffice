//! The ZIP reader and writer are adapters over the `zip` crate (PD-269). These tests hold the
//! limits that stay in this crate against hostile archives: what the crate would tolerate or read
//! without a bound (a lying size field, data around the archive, a second spelling of one path,
//! ZIP64 and other methods) must still be refused, fail closed, and without a large allocation.

use easyoffice_formats::{EasyOfficeError, ZipArchiveCodec, ZipSecurityPolicy};

const EOCD_SIG: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];

fn u32_at(bytes: &[u8], at: usize) -> usize {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}

/// Offset of the central directory entry of the first (and only) entry.
fn central_entry(archive: &[u8]) -> usize {
    let eocd = archive.windows(4).rposition(|w| w == EOCD_SIG).unwrap();
    u32_at(archive, eocd + 16)
}

fn one_entry(name: &str, data: Vec<u8>) -> Vec<u8> {
    ZipArchiveCodec::pack_deflate(&[(name.to_string(), data)]).unwrap()
}

fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn a_stream_larger_than_its_declared_size_is_cut_at_the_budget_not_read_to_the_end() {
    // 8 MiB of zeros packs to a few KiB; both headers then claim 100 bytes.
    let mut archive = one_entry("a.xml", vec![0u8; 8 * 1024 * 1024]);
    let cd = central_entry(&archive);
    put_u32(&mut archive, 22, 100);
    put_u32(&mut archive, cd + 24, 100);
    let policy = ZipSecurityPolicy {
        max_single_entry_uncompressed_bytes: 64 * 1024,
        max_compression_ratio: f64::INFINITY,
        ..ZipSecurityPolicy::default()
    };
    assert!(matches!(
        ZipArchiveCodec::unpack(&archive, &policy),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
    // Within the budget the lie is caught by the size comparison instead.
    let roomy = ZipSecurityPolicy {
        max_compression_ratio: f64::INFINITY,
        ..ZipSecurityPolicy::default()
    };
    assert!(matches!(
        ZipArchiveCodec::unpack(&archive, &roomy),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
}

#[test]
fn bytes_before_or_after_the_archive_are_refused() {
    let archive = one_entry("a.xml", b"<a/>".repeat(50));
    let mut prefixed = b"junk".to_vec();
    prefixed.extend_from_slice(&archive);
    assert!(ZipArchiveCodec::unpack(&prefixed, &ZipSecurityPolicy::default()).is_err());

    let mut suffixed = archive.clone();
    suffixed.extend_from_slice(b"trailing");
    assert!(matches!(
        ZipArchiveCodec::unpack(&suffixed, &ZipSecurityPolicy::default()),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    // A comment length that does not reach the end of the file.
    let eocd = archive.windows(4).rposition(|w| w == EOCD_SIG).unwrap();
    let mut short_comment = archive.clone();
    put_u16(&mut short_comment, eocd + 20, 3);
    assert!(ZipArchiveCodec::unpack(&short_comment, &ZipSecurityPolicy::default()).is_err());
}

#[test]
fn zip64_markers_and_foreign_methods_are_refused() {
    let archive = one_entry("a.xml", b"<a/>".repeat(50));
    let eocd = archive.windows(4).rposition(|w| w == EOCD_SIG).unwrap();
    let cd = central_entry(&archive);

    let mut zip64 = archive.clone();
    put_u16(&mut zip64, eocd + 8, 0xFFFF);
    put_u16(&mut zip64, eocd + 10, 0xFFFF);
    put_u32(&mut zip64, eocd + 16, u32::MAX);
    assert!(matches!(
        ZipArchiveCodec::unpack(&zip64, &ZipSecurityPolicy::default()),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    for method in [1u16, 9, 12, 14, 93, 98] {
        let mut foreign = archive.clone();
        put_u16(&mut foreign, 8, method);
        put_u16(&mut foreign, cd + 10, method);
        assert!(
            ZipArchiveCodec::unpack(&foreign, &ZipSecurityPolicy::default()).is_err(),
            "method {method}"
        );
    }
}

#[test]
fn an_encrypted_entry_is_refused_in_either_header() {
    let archive = one_entry("a.xml", b"<a/>".repeat(50));
    let cd = central_entry(&archive);
    let mut central = archive.clone();
    put_u16(&mut central, cd + 8, 1);
    assert!(matches!(
        ZipArchiveCodec::unpack(&central, &ZipSecurityPolicy::default()),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(_))
    ));
    let mut local = archive.clone();
    put_u16(&mut local, 6, 1);
    assert!(ZipArchiveCodec::unpack(&local, &ZipSecurityPolicy::default()).is_err());
}

#[test]
fn a_local_header_that_disagrees_with_the_directory_is_a_security_violation() {
    let archive = one_entry("a.xml", b"<a/>".repeat(50));
    // Local CRC differs from the directory's.
    let mut crc = archive.clone();
    crc[14] ^= 0xFF;
    assert!(matches!(
        ZipArchiveCodec::unpack(&crc, &ZipSecurityPolicy::default()),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
    // Local name differs from the directory's.
    let mut name = archive.clone();
    name[30] = b'b';
    assert!(matches!(
        ZipArchiveCodec::unpack(&name, &ZipSecurityPolicy::default()),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
}

#[test]
fn a_path_listed_twice_is_a_security_violation_not_a_silent_overwrite() {
    let archive = ZipArchiveCodec::pack(&[
        ("a.xml".to_string(), b"one".to_vec()),
        ("b.xml".to_string(), b"two".to_vec()),
    ])
    .unwrap();
    let cd = central_entry(&archive);
    let second = cd + 46 + "a.xml".len();
    let mut twice = archive.clone();
    twice[second + 46..second + 46 + 5].copy_from_slice(b"a.xml");
    assert!(matches!(
        ZipArchiveCodec::unpack(&twice, &ZipSecurityPolicy::default()),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
}

#[test]
fn packing_stores_what_deflate_did_not_shrink_and_what_would_pass_the_bomb_ratio() {
    // Random bytes do not shrink; 32 MiB of zeros compresses past the reader's ratio of 100.
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let random: Vec<u8> = (0..20_000)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect();
    let zeros = vec![0u8; 32 * 1024 * 1024];
    let archive = ZipArchiveCodec::pack_compressed(&[
        ("random.bin".to_string(), random.clone()),
        ("zeros.xml".to_string(), zeros.clone()),
    ])
    .unwrap();
    let cd = central_entry(&archive);
    // Method 0 (stored) in the first directory entry; the second one is stored as well.
    assert_eq!(u16::from_le_bytes([archive[cd + 10], archive[cd + 11]]), 0);
    let second = cd + 46 + "random.bin".len();
    assert_eq!(
        u16::from_le_bytes([archive[second + 10], archive[second + 11]]),
        0
    );
    let entries = ZipArchiveCodec::unpack(&archive, &ZipSecurityPolicy::default()).unwrap();
    assert_eq!(entries[0].data, random);
    assert_eq!(entries[1].data, zeros);
}
