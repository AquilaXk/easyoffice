use super::cache::{FileStamp, PreviewCache, MAX_CACHED};
use super::*;
use std::io::Cursor;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

const GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x00\x00\x00;";
const FREE: u32 = 0xFFFF_FFFF;
const END: u32 = 0xFFFF_FFFE;
const FAT_SECTOR: u32 = 0xFFFF_FFFD;
const SECTOR: usize = 512;

fn png() -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&[7; 40]);
    bytes
}

fn put32(buf: &mut [u8], at: usize, value: u32) {
    buf[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn dir_entry(name: &str, kind: u8, start: u32, size: u32) -> [u8; 128] {
    let mut raw = [0_u8; 128];
    for (i, unit) in name.encode_utf16().enumerate() {
        raw[i * 2..i * 2 + 2].copy_from_slice(&unit.to_le_bytes());
    }
    let name_bytes = u16::try_from((name.len() + 1) * 2).unwrap();
    raw[64..66].copy_from_slice(&name_bytes.to_le_bytes());
    raw[66] = kind;
    put32(&mut raw, 116, start);
    put32(&mut raw, 120, size);
    raw
}

/// A version 3 compound file whose root holds one stream `name` with the given bytes: a regular
/// stream (4096 bytes and more) or a mini stream.
fn ole2(name: &str, stream: &[u8]) -> Vec<u8> {
    let mini = stream.len() < 4096;
    let mut header = vec![0_u8; SECTOR];
    header[..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    header[30..32].copy_from_slice(&9_u16.to_le_bytes());
    header[32..34].copy_from_slice(&6_u16.to_le_bytes());
    put32(&mut header, 44, 1);
    put32(&mut header, 48, 1);
    put32(&mut header, 56, 4096);
    put32(&mut header, 68, END);
    put32(&mut header, 72, 0);
    for i in 0..109 {
        put32(&mut header, 76 + i * 4, if i == 0 { 0 } else { FREE });
    }
    let mut fat = vec![FREE; SECTOR / 4];
    fat[0] = FAT_SECTOR;
    fat[1] = END;
    let mut sectors: Vec<Vec<u8>> = vec![vec![], vec![]];
    let (root_start, root_size, start) = if mini {
        let mini_count = stream.len().div_ceil(64);
        let data_sectors = (mini_count * 64).div_ceil(SECTOR);
        put32(&mut header, 60, 2);
        put32(&mut header, 64, 1);
        fat[2] = END;
        let mut mini_fat = vec![FREE; SECTOR / 4];
        for (i, slot) in mini_fat.iter_mut().enumerate().take(mini_count) {
            *slot = if i + 1 == mini_count {
                END
            } else {
                u32::try_from(i + 1).unwrap()
            };
        }
        sectors.push(mini_fat.iter().flat_map(|v| v.to_le_bytes()).collect());
        let mut padded = stream.to_vec();
        padded.resize(data_sectors * SECTOR, 0);
        for (i, chunk) in padded.chunks(SECTOR).enumerate() {
            fat[3 + i] = if i + 1 == data_sectors {
                END
            } else {
                u32::try_from(4 + i).unwrap()
            };
            sectors.push(chunk.to_vec());
        }
        (3_u32, u32::try_from(mini_count * 64).unwrap(), 0_u32)
    } else {
        put32(&mut header, 60, END);
        let count = stream.len().div_ceil(SECTOR);
        let mut padded = stream.to_vec();
        padded.resize(count * SECTOR, 0);
        for (i, chunk) in padded.chunks(SECTOR).enumerate() {
            fat[2 + i] = if i + 1 == count {
                END
            } else {
                u32::try_from(3 + i).unwrap()
            };
            sectors.push(chunk.to_vec());
        }
        (END, 0_u32, 2_u32)
    };
    let mut dir = Vec::new();
    dir.extend_from_slice(&dir_entry("Root Entry", 5, root_start, root_size));
    dir.extend_from_slice(&dir_entry("Other", 2, FREE, 0));
    dir.extend_from_slice(&dir_entry(
        name,
        2,
        start,
        u32::try_from(stream.len()).unwrap(),
    ));
    dir.resize(SECTOR, 0);
    sectors[1] = dir;
    sectors[0] = fat.iter().flat_map(|v| v.to_le_bytes()).collect();
    let mut file = header;
    for sector in sectors {
        file.extend_from_slice(&sector);
    }
    file
}

fn zip_entry(name: &str, method: u16, comp: &[u8], uncompressed: usize) -> (Vec<u8>, Vec<u8>) {
    let mut local = Vec::new();
    local.extend_from_slice(&0x0403_4B50_u32.to_le_bytes());
    local.extend_from_slice(&[20, 0, 0, 0]);
    local.extend_from_slice(&method.to_le_bytes());
    local.extend_from_slice(&[0; 4]);
    local.extend_from_slice(&0_u32.to_le_bytes());
    local.extend_from_slice(&u32::try_from(comp.len()).unwrap().to_le_bytes());
    local.extend_from_slice(&u32::try_from(uncompressed).unwrap().to_le_bytes());
    local.extend_from_slice(&u16::try_from(name.len()).unwrap().to_le_bytes());
    local.extend_from_slice(&0_u16.to_le_bytes());
    local.extend_from_slice(name.as_bytes());
    local.extend_from_slice(comp);
    (local, name.as_bytes().to_vec())
}

pub(super) fn zip(entries: &[(&str, u16, Vec<u8>, usize)]) -> Vec<u8> {
    let mut body = Vec::new();
    let mut central = Vec::new();
    for (name, method, comp, uncompressed) in entries {
        let (local, name_bytes) = zip_entry(name, *method, comp, *uncompressed);
        let offset = u32::try_from(body.len()).unwrap();
        body.extend_from_slice(&local);
        central.extend_from_slice(&0x0201_4B50_u32.to_le_bytes());
        central.extend_from_slice(&[20, 0, 20, 0, 0, 0]);
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&[0; 4]);
        central.extend_from_slice(&0_u32.to_le_bytes());
        central.extend_from_slice(&u32::try_from(comp.len()).unwrap().to_le_bytes());
        central.extend_from_slice(&u32::try_from(*uncompressed).unwrap().to_le_bytes());
        central.extend_from_slice(&u16::try_from(name_bytes.len()).unwrap().to_le_bytes());
        central.extend_from_slice(&[0; 12]);
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(&name_bytes);
    }
    let mut file = body;
    let central_at = u32::try_from(file.len()).unwrap();
    file.extend_from_slice(&central);
    file.extend_from_slice(&0x0605_4B50_u32.to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&u16::try_from(entries.len()).unwrap().to_le_bytes());
    file.extend_from_slice(&u16::try_from(entries.len()).unwrap().to_le_bytes());
    file.extend_from_slice(&u32::try_from(central.len()).unwrap().to_le_bytes());
    file.extend_from_slice(&central_at.to_le_bytes());
    file.extend_from_slice(&0_u16.to_le_bytes());
    file
}

fn extract(bytes: Vec<u8>) -> Option<Thumbnail> {
    extract_thumbnail(&mut Cursor::new(bytes))
}

#[test]
fn magic_bytes_pick_png_jpeg_gif_and_bmp_only() {
    assert_eq!(sniff_image(&png()), Some("image/png"));
    assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
    assert_eq!(sniff_image(GIF), Some("image/gif"));
    assert_eq!(sniff_image(b"GIF87a.."), Some("image/gif"));
    assert_eq!(sniff_image(b"BM\x00\x00"), Some("image/bmp"));
    assert_eq!(sniff_image(&[0xD7, 0xCD, 0xC6, 0x9A]), None, "WMF");
    assert_eq!(sniff_image(&[1, 0, 0, 0, 0x6C]), None, "EMF");
    assert_eq!(sniff_image(b""), None);
}

#[test]
fn recent_ids_are_sixteen_lower_case_hex_digits() {
    assert!(is_recent_id("0123456789abcdef"));
    for bad in [
        "",
        "0123456789ABCDEF",
        "0123456789abcde",
        "0123456789abcdef0",
        "../../etc/passwd",
        "0123456789abcdeg",
    ] {
        assert!(!is_recent_id(bad), "{bad}");
    }
}

#[test]
fn a_small_prv_image_is_read_from_the_mini_stream() {
    let found = extract(ole2("PrvImage", GIF)).unwrap();
    assert_eq!((found.mime, found.bytes.as_slice()), ("image/gif", GIF));
    let mut long = png();
    long.extend_from_slice(&[9; 600]);
    assert_eq!(
        extract(ole2("PrvImage", &long)).unwrap().bytes,
        long,
        "a stream over several mini and file sectors"
    );
}

#[test]
fn a_large_prv_image_is_read_from_regular_sectors() {
    let mut big = png();
    big.extend((0..9000_u32).map(|i| u8::try_from(i % 251).unwrap()));
    let found = extract(ole2("PrvImage", &big)).unwrap();
    assert_eq!((found.mime, found.bytes), ("image/png", big));
}

#[test]
fn ole2_without_the_stream_or_with_a_bad_picture_has_no_thumbnail() {
    assert!(
        extract(ole2("PrvText", GIF)).is_none(),
        "another stream name"
    );
    assert!(
        extract(ole2("PrvImage", &[0xD7, 0xCD, 0xC6, 0x9A, 0, 0, 0, 0])).is_none(),
        "WMF magic"
    );
    assert!(
        extract(ole2("PrvImage", &[0; 40])).is_none(),
        "no known magic"
    );
    assert!(extract(ole2("PrvImage", b"")).is_none(), "empty stream");
}

#[test]
fn an_oversized_stream_is_refused_without_reading_it() {
    // The directory claims a stream far larger than the limit; the file itself stays tiny.
    let mut file = ole2("PrvImage", GIF);
    let entry = 2 * SECTOR + 2 * 128;
    let claimed = u32::try_from(MAX_THUMBNAIL_BYTES + 1).unwrap();
    put32(&mut file, entry + 120, claimed);
    assert!(extract(file).is_none());
}

#[test]
fn damaged_ole2_containers_give_none_and_never_loop() {
    let good = ole2("PrvImage", GIF);
    for cut in [8, 100, 512, 700, 1200, 1700, 2050] {
        assert!(extract(good[..cut].to_vec()).is_none(), "cut at {cut}");
    }
    let mut bad_shift = good.clone();
    bad_shift[30] = 40;
    assert!(extract(bad_shift).is_none());
    let mut cyclic = good.clone();
    cyclic[SECTOR + 4..SECTOR + 8].copy_from_slice(&1_u32.to_le_bytes());
    assert!(
        extract(cyclic).is_none(),
        "the directory chain points back to itself"
    );
    let mut huge_fat = good.clone();
    put32(&mut huge_fat, 44, 1_000_000);
    assert!(extract(huge_fat).is_none(), "an absurd FAT sector count");
    let mut stray = good;
    put32(&mut stray, 76, 0x7FFF_FFF0);
    assert!(extract(stray).is_none(), "a FAT sector id beyond the file");
}

#[test]
fn zip_documents_give_their_stored_or_deflated_thumbnail() {
    let png = png();
    let stored = zip(&[
        ("Contents/section0.xml", 0, vec![1, 2, 3], 3),
        ("Preview/PrvImage.png", 0, png.clone(), png.len()),
    ]);
    assert_eq!(extract(stored).unwrap().bytes, png);
    let packed = easyoffice_formats::deflate_raw(&png);
    let deflated = zip(&[("docProps/thumbnail.jpeg", 8, packed, png.len())]);
    let found = extract(deflated).unwrap();
    assert_eq!(
        (found.mime, found.bytes),
        ("image/png", png),
        "the type comes from the bytes, not the name"
    );
    let odf = zip(&[("Thumbnails/thumbnail.png", 0, GIF.to_vec(), GIF.len())]);
    assert_eq!(extract(odf).unwrap().mime, "image/gif");
    let jpg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0];
    let pages = zip(&[("QuickLook/Thumbnail.jpg", 0, jpg.clone(), jpg.len())]);
    let found = extract(pages).unwrap();
    assert_eq!((found.mime, found.bytes), ("image/jpeg", jpg));
}

#[test]
fn zip_thumbnails_with_other_names_types_or_limits_are_ignored() {
    assert!(extract(zip(&[("docProps/thumbnail.emf", 0, vec![1, 0, 0, 0], 4)])).is_none());
    assert!(extract(zip(&[(
        "docProps/thumbnail.wmf",
        0,
        vec![0xD7, 0xCD, 0xC6, 0x9A],
        4
    )]))
    .is_none());
    assert!(extract(zip(&[(
        "word/media/thumbnail.png",
        0,
        GIF.to_vec(),
        GIF.len()
    )]))
    .is_none());
    assert!(
        extract(zip(&[("docProps/thumbnail.png", 0, vec![0; 64], 64)])).is_none(),
        "not an image"
    );
    assert!(
        extract(zip(&[(
            "docProps/thumbnail.png",
            99,
            GIF.to_vec(),
            GIF.len()
        )]))
        .is_none(),
        "unknown method"
    );
    let mut big = png();
    big.resize(MAX_THUMBNAIL_BYTES + 1, 0);
    assert!(
        extract(zip(&[(
            "docProps/thumbnail.png",
            0,
            big.clone(),
            big.len()
        )]))
        .is_none(),
        "over the limit"
    );
    let bomb = easyoffice_formats::deflate_raw(&vec![0x42_u8; MAX_THUMBNAIL_BYTES * 8]);
    assert!(
        extract(zip(&[(
            "docProps/thumbnail.png",
            8,
            bomb,
            MAX_THUMBNAIL_BYTES
        )]))
        .is_none(),
        "inflates past the limit"
    );
}

#[test]
fn damaged_zip_containers_give_none() {
    let good = zip(&[("docProps/thumbnail.png", 0, png(), png().len())]);
    for cut in [4, 30, good.len() / 2, good.len() - 5, good.len() - 1] {
        assert!(extract(good[..cut].to_vec()).is_none(), "cut at {cut}");
    }
    let mut wrong_offset = good.clone();
    let at = good.len() - 6;
    wrong_offset[at..at + 4].copy_from_slice(&0x00FF_FFFF_u32.to_le_bytes());
    assert!(extract(wrong_offset).is_none());
    let mut zip64 = good;
    let at = zip64.len() - 6;
    zip64[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(extract(zip64).is_none(), "ZIP64 is not read");
}

#[test]
fn other_files_have_no_thumbnail() {
    assert!(extract(b"%PDF-1.7 ....".to_vec()).is_none());
    assert!(extract(Vec::new()).is_none());
    assert!(extract(vec![0xD0, 0xCF]).is_none());
}

#[test]
fn the_cache_keys_on_id_time_and_size_and_keeps_ten_entries() {
    let stamp = |n: u64| FileStamp::new(Some(SystemTime::UNIX_EPOCH + Duration::from_secs(n)), n);
    let found = Arc::new(Thumbnail {
        mime: "image/gif",
        bytes: GIF.to_vec(),
    });
    let mut cache = PreviewCache::default();
    assert!(cache.get("a", &stamp(1)).is_none());
    cache.put("a", stamp(1), Some(found.clone()));
    assert_eq!(cache.get("a", &stamp(1)), Some(Some(found.clone())));
    assert!(cache.get("a", &stamp(2)).is_none(), "the file changed");
    cache.put("b", stamp(1), None);
    assert_eq!(
        cache.get("b", &stamp(1)),
        Some(None),
        "a miss is remembered"
    );
    for n in 0..30 {
        cache.put(&format!("id{n}"), stamp(1), None);
    }
    assert_eq!(cache.len(), MAX_CACHED);
    assert!(
        cache.get("a", &stamp(1)).is_none(),
        "the oldest entries went first"
    );
    assert!(cache.get("id29", &stamp(1)).is_some());
}

#[test]
fn the_page_gets_base64_and_the_type() {
    let view = to_view(&Thumbnail {
        mime: "image/gif",
        bytes: GIF.to_vec(),
    });
    assert_eq!(view.mime, "image/gif");
    assert_eq!(
        easyoffice_formats::base64_codec::decode(&view.base64).unwrap(),
        GIF
    );
}
