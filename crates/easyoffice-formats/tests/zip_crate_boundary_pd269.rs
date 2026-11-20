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

// The directory the crate reads must be the directory this crate checked.

fn eocd_at(archive: &[u8]) -> usize {
    archive.windows(4).rposition(|w| w == EOCD_SIG).unwrap()
}

/// `archive` with `extra` inserted after the name of its first central directory entry (extra
/// field length and directory size updated, so the archive stays well formed).
fn with_central_extra(archive: &[u8], extra: &[u8]) -> Vec<u8> {
    let cd = central_entry(archive);
    let name_len = usize::from(u16::from_le_bytes([archive[cd + 28], archive[cd + 29]]));
    let eocd = eocd_at(archive);
    let at = cd + 46 + name_len;
    let mut out = archive[..at].to_vec();
    out.extend_from_slice(extra);
    out.extend_from_slice(&archive[at..]);
    let extra_len = u16::from_le_bytes([out[cd + 30], out[cd + 31]]) + extra.len() as u16;
    put_u16(&mut out, cd + 30, extra_len);
    let eocd = eocd + extra.len();
    let size = u32_at(&out, eocd + 12) + extra.len();
    put_u32(&mut out, eocd + 12, size as u32);
    out
}

fn stored(name: &str, data: &[u8]) -> Vec<u8> {
    ZipArchiveCodec::pack(&[(name.to_string(), data.to_vec())]).unwrap()
}

fn is_refused(archive: &[u8]) -> bool {
    ZipArchiveCodec::unpack(archive, &ZipSecurityPolicy::default()).is_err()
}

/// `real` with a copy of its directory entry (different time stamp) and an end record for that
/// copy placed in the comment of the real end record, followed by `tail` more comment bytes.
fn with_end_record_in_comment(real: &[u8], tail: &[u8]) -> Vec<u8> {
    let cd = central_entry(real);
    let eocd = eocd_at(real);
    let mut copy = real[cd..eocd].to_vec();
    copy[12] = 7;
    let mut fake = real[eocd..eocd + 22].to_vec();
    put_u32(&mut fake, 12, copy.len() as u32);
    put_u32(&mut fake, 16, real.len() as u32);
    let mut out = real.to_vec();
    put_u16(
        &mut out,
        eocd + 20,
        (copy.len() + fake.len() + tail.len()) as u16,
    );
    out.extend_from_slice(&copy);
    out.extend_from_slice(&fake);
    out.extend_from_slice(tail);
    out
}

#[test]
fn an_end_record_hidden_in_the_comment_does_not_change_which_directory_is_read() {
    let real = stored("a.xml", b"hello");
    // Data after the hidden record: the crate takes it, a strict search takes the real record.
    // Nothing after it: both records fit the end of the file and the last one would win.
    for tail in [&b"tail"[..], b""] {
        assert!(matches!(
            ZipArchiveCodec::unpack(
                &with_end_record_in_comment(&real, tail),
                &ZipSecurityPolicy::default()
            ),
            Err(EasyOfficeError::SecurityViolation(_))
        ));
    }
}

#[test]
fn a_unicode_path_field_cannot_rename_the_entry() {
    // The local header and the crate's view say b.xml, the raw directory name says a.xml.
    let mut archive = stored("a.xml", b"hello");
    archive[30] = b'b';
    let name_crc = easyoffice_formats::IeeeCrc32::checksum(b"a.xml").to_le_bytes();
    let mut extra = vec![0x75, 0x70, 6 + 5, 0, 1];
    extra.extend_from_slice(&name_crc);
    extra.extend_from_slice(b"b.xml");
    let renamed = with_central_extra(&archive, &extra);
    assert!(is_refused(&renamed));
}

#[test]
fn a_zip64_field_cannot_override_the_sizes_and_offset_of_the_directory() {
    // The directory claims 9999 bytes, a ZIP64 field gives the crate the true 5.
    let mut archive = stored("a.xml", b"hello");
    let cd = central_entry(&archive);
    put_u32(&mut archive, cd + 20, 9999);
    put_u32(&mut archive, cd + 24, 9999);
    let mut extra = vec![0x01, 0x00, 24, 0];
    extra.extend_from_slice(&5u64.to_le_bytes());
    extra.extend_from_slice(&5u64.to_le_bytes());
    extra.extend_from_slice(&0u64.to_le_bytes());
    assert!(is_refused(&with_central_extra(&archive, &extra)));
}

#[test]
fn an_aes_field_cannot_change_the_method() {
    let archive = stored("a.xml", b"hello");
    let extra = [0x01, 0x99, 7, 0, 2, 0, b'A', b'E', 3, 0, 0];
    assert!(is_refused(&with_central_extra(&archive, &extra)));
}

#[test]
fn bytes_between_the_last_entry_and_the_end_record_are_refused() {
    let archive = stored("a.xml", b"hello");
    let eocd = eocd_at(&archive);
    let mut padded = archive[..eocd].to_vec();
    padded.extend_from_slice(b"JUNK");
    padded.extend_from_slice(&archive[eocd..]);
    let size = u32_at(&padded, eocd + 4 + 12) + 4;
    put_u32(&mut padded, eocd + 4 + 12, size as u32);
    assert!(matches!(
        ZipArchiveCodec::unpack(&padded, &ZipSecurityPolicy::default()),
        Err(EasyOfficeError::CorruptedContainer(_) | EasyOfficeError::SecurityViolation(_))
    ));
}

#[test]
fn a_zip64_locator_with_a_huge_count_is_refused_before_the_crate_reads_anything() {
    let archive = stored("a.xml", b"hello");
    let eocd = eocd_at(&archive);
    let mut out = archive[..eocd].to_vec();
    out.extend_from_slice(&[0x50, 0x4B, 0x06, 0x07]);
    out.extend_from_slice(&[0u8; 16]);
    out.extend_from_slice(&archive[eocd..]);
    let eocd = eocd + 20;
    put_u16(&mut out, eocd + 8, 0xFFFF);
    put_u16(&mut out, eocd + 10, 0xFFFF);
    assert!(matches!(
        ZipArchiveCodec::unpack(&out, &ZipSecurityPolicy::default()),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
}

#[test]
fn an_entry_count_over_the_policy_is_refused_by_the_policy() {
    let mut archive = stored("a.xml", b"hello");
    let eocd = eocd_at(&archive);
    put_u16(&mut archive, eocd + 8, 5_000);
    put_u16(&mut archive, eocd + 10, 5_000);
    assert!(matches!(
        ZipArchiveCodec::unpack(&archive, &ZipSecurityPolicy::default()),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
}

#[test]
fn a_data_descriptor_archive_is_still_read_and_a_wrong_directory_offset_is_not() {
    // Flag 3: the local header carries no CRC or sizes, the directory does.
    let mut archive = ZipArchiveCodec::pack(&[
        ("a.xml".to_string(), b"hello".to_vec()),
        ("b.xml".to_string(), b"world".to_vec()),
    ])
    .unwrap();
    for local in [0usize, 30 + 5 + 5] {
        put_u16(&mut archive, local + 6, 8);
        put_u32(&mut archive, local + 14, 0);
        put_u32(&mut archive, local + 18, 0);
        put_u32(&mut archive, local + 22, 0);
    }
    let entries = ZipArchiveCodec::unpack(&archive, &ZipSecurityPolicy::default()).unwrap();
    assert_eq!(entries[1].data, b"world");
    // The second directory entry now points at the first local header.
    let second = central_entry(&archive) + 46 + 5;
    put_u32(&mut archive, second + 42, 0);
    assert!(matches!(
        ZipArchiveCodec::unpack(&archive, &ZipSecurityPolicy::default()),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
}

#[test]
fn compressed_output_is_the_same_whatever_else_is_linked() {
    // The entry compression is the engine's own DEFLATE adapter, so these bytes do not depend on
    // the backend another crate in the build (the desktop shell links one) picks for the `zip`
    // crate. A new `miniz_oxide` version may change them; update the two numbers on purpose.
    let text = b"<w:p><w:r><w:t>EasyOffice golden bytes 12345 67890</w:t></w:r></w:p>".repeat(60);
    let archive = ZipArchiveCodec::pack_compressed(&[
        ("mimetype".to_string(), b"application/x-golden".to_vec()),
        ("word/document.xml".to_string(), text),
    ])
    .unwrap();
    assert_eq!(
        (
            archive.len(),
            easyoffice_formats::IeeeCrc32::checksum(&archive)
        ),
        (338, 0x7e4f_04f8)
    );
}

#[test]
fn an_end_record_signature_inside_the_directory_is_refused_before_the_crate_reads_it() {
    // A name or extra field could carry a ZIP64 record for the crate to fall back to.
    for signature in [
        [0x50, 0x4B, 0x05, 0x06],
        [0x50, 0x4B, 0x06, 0x06],
        [0x50, 0x4B, 0x06, 0x07],
    ] {
        let mut archive = stored("abcd.x", b"hello");
        let cd = central_entry(&archive);
        archive[cd + 46..cd + 50].copy_from_slice(&signature);
        assert!(matches!(
            ZipArchiveCodec::unpack(&archive, &ZipSecurityPolicy::default()),
            Err(EasyOfficeError::SecurityViolation(_))
        ));
    }
}

#[test]
fn a_directory_that_does_not_start_where_the_end_record_says_is_refused() {
    let mut archive = stored("a.xml", b"hello");
    let cd = central_entry(&archive);
    archive[cd] = 0;
    assert!(matches!(
        ZipArchiveCodec::unpack(&archive, &ZipSecurityPolicy::default()),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
}
