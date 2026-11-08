//! Pages pictures: `Data/` lookup through package metadata, alternative text, warnings for
//! unsupported data, and fail-closed behavior on hostile packages. Synthetic packages only.

mod image_fixture;
mod pages_fixture;
mod pages_picture_fixture;

use easyoffice_formats::{EasyOfficeError, RichBlock, MAX_IMAGES_PER_DOCUMENT, MAX_IMAGE_BYTES};
use image_fixture::{make_jpeg, make_png, png_chunk, PNG_SIGNATURE};
use pages_fixture::*;
use pages_picture_fixture::*;

/// Single-picture package: metadata id 1 -> `Data/pic.bin` holding `bytes`.
fn one_picture_zip(bytes: Vec<u8>, p: &Picture) -> Vec<u8> {
    let mut b = DocBuilder::default();
    metadata(&mut b, &[(1, "pic.bin")]);
    let img = picture(&mut b, p);
    let att = b.attachment(img);
    let body = body_with(&mut b, "앞", &[att], "뒤");
    pages_zip_with_data(b.finish(body), vec![("Data/pic.bin".to_string(), bytes)])
}

/// Package whose `n` anchors all point at the same picture file.
fn repeated_picture_zip(bytes: Vec<u8>, n: usize) -> Vec<u8> {
    let mut b = DocBuilder::default();
    metadata(&mut b, &[(1, "pic.bin")]);
    let img = picture(&mut b, &Picture::default());
    let atts: Vec<u64> = (0..n).map(|_| b.attachment(img)).collect();
    let body = body_with(&mut b, "앞", &atts, "뒤");
    pages_zip_with_data(b.finish(body), vec![("Data/pic.bin".to_string(), bytes)])
}

#[test]
fn picture_keeps_description_as_alternative_text_and_geometry_size() {
    let png = make_png(8, 4, 2, 3);
    let zip = one_picture_zip(
        png.clone(),
        &Picture {
            alt: Some("  회사 로고  "),
            size_pt: Some((100.0, 50.0)),
            ..Picture::default()
        },
    );
    let (doc, warnings) = open(&zip).expect("opens");
    let imgs = images(&doc);
    assert_eq!(imgs.len(), 1);
    assert_eq!(imgs[0].bytes(), png.as_slice());
    assert_eq!(imgs[0].alt(), "회사 로고");
    assert_eq!(imgs[0].display_emu(), (100 * 12_700, 50 * 12_700));
    assert!(!warned(&warnings, "가져오지 않은 요소"), "{warnings:?}");
    // It sits between its neighbouring paragraphs.
    assert!(matches!(doc.blocks[1], RichBlock::Image(_)));
}

#[test]
fn jpeg_data_is_imported_too() {
    let (doc, _) = open(&one_picture_zip(make_jpeg(6, 3), &Picture::default())).expect("opens");
    assert_eq!(images(&doc).len(), 1);
}

#[test]
fn nonsense_geometry_falls_back_to_the_pixel_size() {
    let default_size = {
        let (doc, _) = open(&one_picture_zip(make_png(8, 4, 2, 3), &Picture::default())).unwrap();
        images(&doc)[0].display_emu()
    };
    for size in [
        (f32::NAN, 10.0),
        (f32::INFINITY, 10.0),
        (-5.0, 10.0),
        (0.0, 0.0),
        (1.0e30, 1.0e30),
    ] {
        let zip = one_picture_zip(
            make_png(8, 4, 2, 3),
            &Picture {
                size_pt: Some(size),
                ..Picture::default()
            },
        );
        let (doc, _) = open(&zip).expect("opens");
        let shown = images(&doc)[0].display_emu();
        assert!(shown.0 > 0 && shown.1 > 0, "{size:?}");
        if !(size.0 > 1.0 && size.1 > 1.0 && size.0.is_finite()) {
            assert_eq!(shown, default_size, "{size:?}");
        }
    }
}

#[test]
fn unsupported_format_is_reported_without_text_in_the_body() {
    let samples: [Vec<u8>; 2] = [
        b"GIF89a\x01\x00\x01\x00\x00\x00\x00;".to_vec(),
        b"II*\x00\x08\x00\x00\x00".to_vec(),
    ];
    for bytes in samples {
        // A description must not leak into the body either.
        let zip = one_picture_zip(
            bytes,
            &Picture {
                alt: Some("분기 매출 표"),
                ..Picture::default()
            },
        );
        let (doc, warnings) = open(&zip).expect("opens");
        assert!(images(&doc).is_empty());
        assert_eq!(paragraph_texts(&doc), vec!["앞", "뒤"]);
        assert!(
            warned(&warnings, "지원하지 않는 형식의 그림 1개"),
            "{warnings:?}"
        );
    }
}

#[test]
fn damaged_pictures_are_skipped_with_a_warning() {
    let png = make_png(8, 4, 2, 3);
    let mut flipped = png.clone();
    let mid = flipped.len() / 2;
    flipped[mid] ^= 0xFF;
    // 100000 x 100000 pixels declared by a tiny file: over the pixel budget.
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&100_000u32.to_be_bytes());
    ihdr.extend_from_slice(&100_000u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut huge_dims = PNG_SIGNATURE.to_vec();
    huge_dims.extend(png_chunk(b"IHDR", &ihdr));
    huge_dims.extend(png_chunk(b"IDAT", &[0; 8]));
    huge_dims.extend(png_chunk(b"IEND", &[]));

    let mut cases: Vec<Vec<u8>> = vec![flipped, huge_dims, PNG_SIGNATURE.to_vec()];
    // Every truncation point of a valid PNG and JPEG that still carries the magic bytes (a
    // shorter prefix reads as unsupported data and gets the other warning).
    for (full, magic) in [(png, PNG_SIGNATURE.len()), (make_jpeg(6, 3), 3)] {
        for cut in magic..full.len() {
            cases.push(full[..cut].to_vec());
        }
    }
    for bytes in cases {
        let (doc, warnings) = open(&one_picture_zip(bytes, &Picture::default())).expect("opens");
        assert!(images(&doc).is_empty());
        assert_eq!(paragraph_texts(&doc), vec!["앞", "뒤"]);
        assert!(
            warned(&warnings, "손상되었거나 한도를 넘은 PNG·JPEG 그림 1개"),
            "{warnings:?}"
        );
    }
}

#[test]
fn broken_references_skip_the_picture_without_failing() {
    // No data reference, unknown identifier, file missing from the package, empty file name,
    // and a name that tries to leave `Data/` (looked up as a plain key, never as a path).
    type Case<'a> = (Option<u64>, &'a [(u64, &'a str)], &'a str);
    let cases: [Case; 5] = [
        (None, &[(1, "pic.bin")], "그림 데이터 참조가 없는 그림"),
        (Some(9), &[(1, "pic.bin")], "파일 이름을 찾을 수 없는 그림"),
        (Some(1), &[(1, "other.png")], "파일이 없는 그림"),
        (Some(1), &[(1, "")], "파일 이름을 찾을 수 없는 그림"),
        (Some(1), &[(1, "../../etc/passwd")], "파일이 없는 그림"),
    ];
    for (data_id, files, expected) in cases {
        let mut b = DocBuilder::default();
        metadata(&mut b, files);
        let img = picture(
            &mut b,
            &Picture {
                data_id,
                ..Picture::default()
            },
        );
        let att = b.attachment(img);
        let body = body_with(&mut b, "앞", &[att], "뒤");
        let zip = pages_zip_with_data(
            b.finish(body),
            vec![("Data/pic.bin".to_string(), make_png(2, 2, 2, 3))],
        );
        let (doc, warnings) = open(&zip).expect("opens");
        assert!(images(&doc).is_empty(), "{expected}");
        assert!(warned(&warnings, expected), "{expected}: {warnings:?}");
    }
}

#[test]
fn too_many_pictures_fail_closed() {
    let zip = repeated_picture_zip(make_png(2, 2, 2, 3), MAX_IMAGES_PER_DOCUMENT + 1);
    assert!(matches!(
        open(&zip),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
    // Exactly the limit still opens.
    let ok = repeated_picture_zip(make_png(2, 2, 2, 3), MAX_IMAGES_PER_DOCUMENT);
    let (doc, _) = open(&ok).expect("opens at the limit");
    assert_eq!(images(&doc).len(), MAX_IMAGES_PER_DOCUMENT);
}

#[test]
fn total_picture_bytes_over_the_budget_fail_closed() {
    // One 31 MiB file referenced five times: 155 MiB against the 128 MiB document budget. The
    // budget is checked before each copy, so the fifth reference is refused without cloning.
    let zip = repeated_picture_zip(bulky_png(31 * 1024 * 1024), 5);
    assert!(matches!(
        open(&zip),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn a_single_oversized_picture_is_skipped() {
    let bytes = vec![0u8; MAX_IMAGE_BYTES + 1];
    let (doc, warnings) = open(&one_picture_zip(bytes, &Picture::default())).expect("opens");
    assert!(images(&doc).is_empty());
    assert!(
        warned(&warnings, "크기 한도를 넘은 그림 1개"),
        "{warnings:?}"
    );
}

#[test]
fn self_referencing_attachments_and_shapes_terminate() {
    let mut b = DocBuilder::default();
    // Attachment whose drawable is itself.
    let looped = b.reserve_id();
    b.add_with_id(looped, 2003, &f_ref(1, looped));
    // Shape whose text anchors the shape itself again.
    let shape = b.reserve_id();
    let shape_att = b.attachment(shape);
    let inner = b.add(
        2001,
        &DocBuilder::storage_payload(
            "도형 글\n\u{FFFC}",
            &[],
            &[],
            &[],
            &[(utf16_len("도형 글\n"), Some(shape_att))],
        ),
    );
    b.add_with_id(shape, 2011, &f_ref(2, inner));
    let body = body_with(&mut b, "앞", &[looped, shape_att], "뒤");
    let (doc, warnings) = open(&pages_zip(b.finish(body))).expect("opens");
    assert_eq!(paragraph_texts(&doc), vec!["앞", "도형 글", "뒤"]);
    assert!(!warnings.is_empty());
}

#[test]
fn cropped_pictures_are_imported_whole_and_reported() {
    let zip = one_picture_zip(
        make_png(4, 4, 2, 3),
        &Picture {
            masked: true,
            ..Picture::default()
        },
    );
    let (doc, warnings) = open(&zip).expect("opens");
    assert_eq!(images(&doc).len(), 1);
    assert!(
        warned(&warnings, "자른 그림(전체 그림으로 가져옴) 1개"),
        "{warnings:?}"
    );
}

#[test]
fn page_layout_floating_drawables_are_reported_not_dropped_silently() {
    let mut b = DocBuilder::default();
    let img = picture(&mut b, &Picture::default());
    let shape = b.add(2011, &[]);
    let missing = 777_777u64;
    // `{ 1: page group { 1: page index, 4: [ { 1: drawable ref } ] } }`
    let group = |page: u64, refs: &[u64]| {
        let mut parts = vec![f_varint(1, page)];
        parts.extend(refs.iter().map(|r| f_bytes(4, &f_ref(1, *r))));
        f_bytes(1, &cat(&parts))
    };
    b.add(
        10010,
        &cat(&[group(0, &[img, shape, shape]), group(1, &[missing])]),
    );
    let body = body_with(&mut b, "앞", &[], "뒤");
    let (_, warnings) = open(&pages_zip(b.finish(body))).expect("opens");
    assert!(
        warned(&warnings, "쪽 위의 개체: 그림(형식 3005) 1개"),
        "{warnings:?}"
    );
    assert!(
        warned(&warnings, "쪽 위의 개체: 그림·도형 등 개체(형식 2011) 1개"),
        "{warnings:?}"
    );
    assert!(
        warned(&warnings, "쪽 위의 끊어진 개체 참조 1개"),
        "{warnings:?}"
    );
}
