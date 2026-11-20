//! `.eobase` / `.eoboard` edit packages (PD-265): roundtrip, and Fail-Closed refusal of every way
//! a hostile or damaged package can step outside the format.

mod image_fixture;

use easyoffice_formats::native_package::{
    read_package, write_package, NativePackage, PackageKind, PackageMedia, FORMAT_VERSION,
    MAX_MEDIA_PARTS, MAX_MODEL_BYTES, MAX_PACKAGE_BYTES,
};
use easyoffice_formats::{ZipArchiveCodec, ZipArchiveEntry};
use image_fixture::{make_jpeg, make_png};

const BASE_MIME: &str = "application/vnd.easyoffice.base+zip";
const BOARD_MIME: &str = "application/vnd.easyoffice.board+zip";

fn base(model: &str) -> NativePackage {
    NativePackage {
        kind: PackageKind::Base,
        model: model.to_string(),
        media: Vec::new(),
    }
}

fn board_with_pictures() -> NativePackage {
    NativePackage {
        kind: PackageKind::Board,
        model: r#"{"nodes":[{"id":"n1","media":"media/img-1.png"},{"id":"n2","media":"media/img-2.jpg"}]}"#
            .to_string(),
        media: vec![
            PackageMedia { path: "media/img-1.png".into(), bytes: make_png(16, 12, 2, 3) },
            PackageMedia { path: "media/img-2.jpg".into(), bytes: make_jpeg(32, 24) },
        ],
    }
}

fn manifest(format: &str, version: u64, media: &str) -> String {
    format!(r#"{{"format":"{format}","formatVersion":{version},"media":[{media}]}}"#)
}

/// A package built by hand, so each test breaks exactly one rule.
fn pack(parts: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let entries: Vec<ZipArchiveEntry> = parts
        .iter()
        .map(|(path, data)| ZipArchiveEntry::new(*path, data.clone()))
        .collect();
    ZipArchiveCodec::pack_compressed(&entries).unwrap()
}

fn good_parts() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("mimetype", BASE_MIME.as_bytes().to_vec()),
        (
            "manifest.json",
            manifest("easyoffice-base", 1, "").into_bytes(),
        ),
        ("model.json", br#"{"tables":[]}"#.to_vec()),
    ]
}

fn refused(parts: &[(&str, Vec<u8>)]) -> String {
    read_package(PackageKind::Base, &pack(parts))
        .expect_err("the package must be refused")
        .to_string()
}

#[test]
fn a_base_package_reads_back_as_written() {
    let package = base(r#"{"version":1,"tables":[{"id":"tbl-1","name":"표"}]}"#);
    let bytes = write_package(&package).unwrap();
    assert_eq!(read_package(PackageKind::Base, &bytes).unwrap(), package);
}

#[test]
fn a_board_package_keeps_its_pictures_byte_for_byte() {
    let package = board_with_pictures();
    let bytes = write_package(&package).unwrap();
    assert_eq!(read_package(PackageKind::Board, &bytes).unwrap(), package);
}

#[test]
fn the_first_part_is_the_stored_media_type_and_the_package_is_a_plain_zip() {
    let bytes = write_package(&base("{}")).unwrap();
    assert_eq!(&bytes[..4], b"PK\x03\x04");
    assert_eq!(&bytes[30..38], b"mimetype");
    assert_eq!(&bytes[8..10], &[0, 0], "mimetype is stored, not deflated");
    assert!(bytes
        .windows(BASE_MIME.len())
        .any(|w| w == BASE_MIME.as_bytes()));
}

#[test]
fn the_same_input_writes_the_same_bytes() {
    let package = board_with_pictures();
    assert_eq!(
        write_package(&package).unwrap(),
        write_package(&package).unwrap()
    );
}

#[test]
fn a_package_of_the_other_kind_is_refused() {
    let bytes = write_package(&base("{}")).unwrap();
    assert!(read_package(PackageKind::Board, &bytes).is_err());
    let board = write_package(&board_with_pictures()).unwrap();
    assert!(read_package(PackageKind::Base, &board).is_err());
}

#[test]
fn a_newer_format_version_is_refused_with_its_number() {
    let mut parts = good_parts();
    parts[1].1 = manifest("easyoffice-base", FORMAT_VERSION + 1, "").into_bytes();
    assert!(refused(&parts).contains("더 새로운 버전"));
    parts[1].1 = manifest("easyoffice-base", 0, "").into_bytes();
    refused(&parts);
}

#[test]
fn the_good_hand_built_package_is_accepted() {
    read_package(PackageKind::Base, &pack(&good_parts())).unwrap();
}

#[test]
fn a_part_outside_the_format_is_refused() {
    let mut parts = good_parts();
    parts.push(("extra.txt", b"x".to_vec()));
    assert!(refused(&parts).contains("허용되지 않는 파트"));
    let mut parts = good_parts();
    parts.push(("media/UPPER.png", make_png(4, 4, 2, 3)));
    refused(&parts);
}

/// Renames a part in both headers of a finished archive. The name keeps its length, and the checksum
/// covers the data only, so the archive stays structurally valid and only the name is hostile.
fn rename_part(mut bytes: Vec<u8>, from: &str, to: &str) -> Vec<u8> {
    assert_eq!(from.len(), to.len());
    let mut at = 0;
    while let Some(found) = bytes[at..]
        .windows(from.len())
        .position(|w| w == from.as_bytes())
    {
        let start = at + found;
        bytes[start..start + to.len()].copy_from_slice(to.as_bytes());
        at = start + to.len();
    }
    bytes
}

#[test]
fn a_traversal_path_is_refused() {
    let from = "media/zzzzzzzz.png";
    let mut parts = good_parts();
    parts.push((from, make_png(4, 4, 2, 3)));
    let archive = pack(&parts);
    read_package(PackageKind::Base, &archive).expect_err("only a base without pictures");
    for hostile in [
        "../../zzzzzzzz.png",
        "/media/zzzzzzzz.png",
        "media\\zzzzzzzz.png",
        "C:media/zzzzzzzz.p",
        "media/../zzzzzz.png",
    ] {
        let hostile: String = hostile
            .chars()
            .chain(std::iter::repeat('z'))
            .take(from.len())
            .collect();
        let renamed = rename_part(archive.clone(), from, &hostile);
        assert!(
            read_package(PackageKind::Base, &renamed).is_err(),
            "{hostile}"
        );
    }
}

#[test]
fn a_mimetype_that_is_not_first_is_refused() {
    let mut parts = good_parts();
    parts.swap(0, 1);
    assert!(refused(&parts).contains("mimetype"));
}

#[test]
fn a_missing_or_duplicated_fixed_part_is_refused() {
    let mut parts = good_parts();
    parts.remove(2);
    refused(&parts);
    let mut parts = good_parts();
    parts.remove(1);
    refused(&parts);
    let mut parts = good_parts();
    parts.push(("model.jsox", br#"{"other":1}"#.to_vec()));
    let doubled = rename_part(pack(&parts), "model.jsox", "model.json");
    assert!(read_package(PackageKind::Base, &doubled).is_err());
}

#[test]
fn a_model_that_is_not_strict_json_is_refused() {
    for model in [
        &b"{"[..],
        b"[]",
        b"\"x\"",
        b"{\"a\":1,\"a\":2}",
        b"{\"a\":1} trailing",
        b"\xff\xfe",
    ] {
        let mut parts = good_parts();
        parts[2].1 = model.to_vec();
        refused(&parts);
    }
}

#[test]
fn a_model_nested_beyond_the_depth_limit_is_refused() {
    let mut parts = good_parts();
    parts[2].1 = format!("{}{}", "[".repeat(5000), "]".repeat(5000)).into_bytes();
    refused(&parts);
    parts[2].1 = format!("{{\"a\":{}1{}}}", "[".repeat(200), "]".repeat(200)).into_bytes();
    refused(&parts);
}

#[test]
fn a_model_over_its_size_limit_is_refused_by_the_writer_and_the_reader() {
    let mut huge = String::from("{\"pad\":\"");
    huge.push_str(&"a".repeat(MAX_MODEL_BYTES));
    huge.push_str("\"}");
    assert!(write_package(&base(&huge)).is_err());
    let mut parts = good_parts();
    parts[2].1 = huge.into_bytes();
    refused(&parts);
}

#[test]
fn a_deflate_bomb_in_the_model_is_refused_by_its_declared_size() {
    let mut parts = good_parts();
    parts[2].1 = vec![b' '; MAX_MODEL_BYTES + 1];
    let started = std::time::Instant::now();
    refused(&parts);
    assert!(started.elapsed().as_secs() < 5);
}

#[test]
fn a_package_file_over_the_size_limit_is_refused_before_it_is_opened() {
    let oversized = vec![0u8; MAX_PACKAGE_BYTES + 1];
    assert!(read_package(PackageKind::Base, &oversized).is_err());
}

#[test]
fn garbage_and_truncated_bytes_are_refused() {
    assert!(read_package(PackageKind::Base, b"").is_err());
    assert!(read_package(PackageKind::Base, b"not a zip at all, just text").is_err());
    let bytes = write_package(&base("{}")).unwrap();
    for cut in [bytes.len() - 1, bytes.len() / 2, 40] {
        assert!(
            read_package(PackageKind::Base, &bytes[..cut]).is_err(),
            "{cut}"
        );
    }
}

#[test]
fn a_flipped_byte_in_a_part_is_refused_by_the_checksum() {
    let mut bytes = write_package(&base(r#"{"name":"checksum target"}"#)).unwrap();
    let at = bytes
        .windows(8)
        .position(|w| w == b"checksum")
        .unwrap_or(bytes.len() / 2);
    bytes[at] ^= 0x01;
    assert!(read_package(PackageKind::Base, &bytes).is_err());
}

#[test]
fn a_picture_the_manifest_does_not_list_is_refused() {
    let mut parts = good_parts();
    parts[0].1 = BOARD_MIME.as_bytes().to_vec();
    parts[1].1 = manifest("easyoffice-board", 1, "").into_bytes();
    parts.push(("media/a.png", make_png(8, 8, 2, 3)));
    assert!(read_package(PackageKind::Board, &pack(&parts)).is_err());
}

#[test]
fn a_manifest_item_without_a_part_or_with_a_wrong_size_is_refused() {
    let png = make_png(8, 8, 2, 3);
    let listed = |bytes: usize, media_type: &str| {
        format!(r#"{{"path":"media/a.png","mediaType":"{media_type}","bytes":{bytes}}}"#)
    };
    let parts_with = |item: String, with_part: bool| {
        let mut parts = good_parts();
        parts[0].1 = BOARD_MIME.as_bytes().to_vec();
        parts[1].1 = manifest("easyoffice-board", 1, &item).into_bytes();
        if with_part {
            parts.push(("media/a.png", png.clone()));
        }
        pack(&parts)
    };
    read_package(
        PackageKind::Board,
        &parts_with(listed(png.len(), "image/png"), true),
    )
    .unwrap();
    assert!(read_package(
        PackageKind::Board,
        &parts_with(listed(png.len(), "image/png"), false)
    )
    .is_err());
    assert!(read_package(
        PackageKind::Board,
        &parts_with(listed(png.len() + 1, "image/png"), true)
    )
    .is_err());
    assert!(read_package(
        PackageKind::Board,
        &parts_with(listed(png.len(), "image/jpeg"), true)
    )
    .is_err());
}

#[test]
fn a_part_that_is_not_a_sound_picture_is_refused() {
    for bytes in [
        b"GIF89a....".to_vec(),
        b"\x89PNG\r\n\x1a\n".to_vec(),
        vec![0xFF, 0xD8, 0xFF],
        Vec::new(),
    ] {
        let mut media = board_with_pictures();
        media.media[0].bytes = bytes;
        assert!(write_package(&media).is_err());
    }
    let mut mismatch = board_with_pictures();
    mismatch.media[0].path = "media/img-1.jpg".to_string();
    assert!(write_package(&mismatch).is_err(), "a PNG under a .jpg name");
}

#[test]
fn a_picture_with_too_many_pixels_is_refused() {
    let mut package = board_with_pictures();
    package.media[0].bytes = make_png(9000, 9000, 0, 1);
    assert!(write_package(&package).is_err());
}

#[test]
fn a_base_package_holds_no_pictures_and_a_board_holds_at_most_the_limit() {
    let mut package = base("{}");
    package.media.push(PackageMedia {
        path: "media/a.png".into(),
        bytes: make_png(4, 4, 2, 3),
    });
    assert!(write_package(&package).is_err());
    let png = make_png(4, 4, 2, 3);
    let media = (0..=MAX_MEDIA_PARTS)
        .map(|n| PackageMedia {
            path: format!("media/p{n}.png"),
            bytes: png.clone(),
        })
        .collect();
    let board = NativePackage {
        kind: PackageKind::Board,
        model: "{}".into(),
        media,
    };
    assert!(write_package(&board).is_err());
}

#[test]
fn a_hostile_media_name_is_refused_by_the_writer() {
    for path in [
        "media/../x.png",
        "media/a b.png",
        "media/.png",
        "media/a.gif",
        "x.png",
        "media/",
        "media/a/b.png",
    ] {
        let mut package = board_with_pictures();
        package.media[0].path = path.to_string();
        assert!(write_package(&package).is_err(), "{path}");
    }
}

#[test]
fn a_file_name_resolves_to_a_kind_by_its_extension() {
    assert_eq!(
        PackageKind::from_file_name("a.eobase"),
        Some(PackageKind::Base)
    );
    assert_eq!(
        PackageKind::from_file_name("a.EOBOARD"),
        Some(PackageKind::Board)
    );
    assert_eq!(PackageKind::from_file_name("a.xlsx"), None);
    assert_eq!(PackageKind::from_file_name("eobase"), None);
}

#[test]
fn the_policy_of_the_reader_is_not_the_default() {
    // The default ZIP policy allows 4096 parts; this format allows 515. A package of 516 parts is refused.
    let mut parts = good_parts();
    for n in 0..MAX_MEDIA_PARTS + 1 {
        parts.push((
            Box::leak(format!("media/p{n}.png").into_boxed_str()),
            make_png(4, 4, 2, 3),
        ));
    }
    assert!(read_package(PackageKind::Base, &pack(&parts)).is_err());
}
