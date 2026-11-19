use super::*;
use crate::preview::{extract_thumbnail, MAX_THUMBNAIL_BYTES};
use crate::save_file::{write_document_bytes, SaveRequest};
use easyoffice_formats::{
    base64_codec, check_thumbnail_png, RichBlock, RichDocument, RichParagraph, RichRun,
    UniversalDocumentModel, UniversalFormatConverter,
};
use std::fs::File;
use std::io::Cursor;
use std::time::Instant;

/// A4 at 96 dpi, the size of a Hangul page SVG, with a red box near the top left corner.
const A4_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="793.7" height="1122.5" viewBox="0 0 793.7 1122.5"><rect x="100" y="100" width="300" height="200" fill="#ff0000"/></svg>"##;

fn fonts() -> Arc<Database> {
    crate::pdf_fonts::shared_font_db(&[])
}

fn model() -> UniversalDocumentModel {
    UniversalDocumentModel {
        title: "저장 시험".to_string(),
        paragraphs: vec!["본문".to_string()],
        ..Default::default()
    }
}

fn rich() -> RichDocument {
    RichDocument {
        page_frame: Default::default(),
        title: "저장 시험".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("본문")],
            ..RichParagraph::default()
        })],
        official_agency: None,
        ..RichDocument::default()
    }
}

fn hwp() -> Vec<u8> {
    UniversalFormatConverter::encode(&model(), DocumentFormat::Hwp).unwrap()
}

fn hwpx() -> Vec<u8> {
    UniversalFormatConverter::encode_rich(&rich(), DocumentFormat::Hwpx, None).unwrap()
}

fn docx() -> Vec<u8> {
    UniversalFormatConverter::encode_rich(&rich(), DocumentFormat::Docx, None).unwrap()
}

fn pixel(png: &[u8], x: u32, y: u32) -> [u8; 4] {
    let pixmap = tiny_skia::Pixmap::decode_png(png).unwrap();
    let p = pixmap.pixel(x, y).unwrap();
    [p.red(), p.green(), p.blue(), p.alpha()]
}

#[test]
fn a_page_is_drawn_with_its_longer_side_at_the_limit() {
    let png = render_png(A4_SVG, &fonts()).unwrap();
    check_thumbnail_png(&png).unwrap();
    let pixmap = tiny_skia::Pixmap::decode_png(&png).unwrap();
    assert_eq!(pixmap.height(), MAX_THUMBNAIL_SIDE_PX);
    assert_eq!(pixmap.width(), 181);
    // The box sits at 100..400 x 100..300 of 793.7 wide: scaled by 256/1122.5.
    assert_eq!(pixel(&png, 50, 40), [255, 0, 0, 255]);
    // Paper is opaque white wherever the page draws nothing.
    assert_eq!(pixel(&png, 170, 240), [255, 255, 255, 255]);
    assert!(png.len() < MAX_THUMBNAIL_BYTES / 8, "{} bytes", png.len());
}

#[test]
fn a_landscape_page_keeps_its_proportions() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1122.5" height="793.7" viewBox="0 0 1122.5 793.7"/>"#;
    let pixmap = tiny_skia::Pixmap::decode_png(&render_png(svg, &fonts()).unwrap()).unwrap();
    assert_eq!((pixmap.width(), pixmap.height()), (256, 181));
}

#[test]
fn a_very_thin_page_still_has_a_pixel_on_each_side() {
    let svg =
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="5000" height="2" viewBox="0 0 5000 2"/>"#;
    let pixmap = tiny_skia::Pixmap::decode_png(&render_png(svg, &fonts()).unwrap()).unwrap();
    assert_eq!((pixmap.width(), pixmap.height()), (256, 1));
}

#[test]
fn unreadable_oversized_or_pathological_svg_is_an_error() {
    let fonts = fonts();
    assert!(render_png("not svg at all", &fonts).is_err());
    assert!(render_png("", &fonts).is_err());
    let zero = r#"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="0"/>"#;
    assert!(render_png(zero, &fonts).is_err());
    let big = format!("{A4_SVG}{}", " ".repeat(MAX_THUMBNAIL_SVG_BYTES));
    assert!(render_png(&big, &fonts).unwrap_err().contains("한도"));
    // An external picture is never read from the file system.
    let leak = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><image href="/etc/hosts" width="100" height="100"/></svg>"#;
    assert!(render_png(leak, &fonts).is_ok());
}

#[test]
fn a_hostile_page_with_a_huge_declared_size_costs_a_small_picture_only() {
    // 2^31 user units on a side: the scale shrinks it to the limit, memory stays at 256 pixels.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="2147483647" height="2147483647" viewBox="0 0 10 10"><rect width="10" height="10" fill="#00f"/></svg>"##;
    let png = render_png(svg, &fonts()).unwrap();
    assert!(tiny_skia::Pixmap::decode_png(&png).unwrap().width() <= MAX_THUMBNAIL_SIDE_PX);
}

#[test]
fn each_format_shows_the_drawn_page_through_the_start_window_reader() {
    let fonts = fonts();
    let expected = render_png(A4_SVG, &fonts).unwrap();
    let mut full = model();
    full.sheet_cells = vec![(0, 0, "항목".to_string()), (1, 0, "100".to_string())];
    full.slides = vec![("제목".to_string(), vec!["내용".to_string()], String::new())];
    for (extension, document) in [
        ("hwp", hwp()),
        ("hwpx", hwpx()),
        ("docx", docx()),
        (
            "xlsx",
            UniversalFormatConverter::encode(&full, DocumentFormat::Xlsx).unwrap(),
        ),
        (
            "pptx",
            UniversalFormatConverter::encode(&full, DocumentFormat::Pptx).unwrap(),
        ),
        (
            "odt",
            UniversalFormatConverter::encode_rich(&rich(), DocumentFormat::Odt, None).unwrap(),
        ),
        (
            "ods",
            UniversalFormatConverter::encode(&full, DocumentFormat::Ods).unwrap(),
        ),
        (
            "odp",
            UniversalFormatConverter::encode(&full, DocumentFormat::Odp).unwrap(),
        ),
    ] {
        let saved = with_first_page(extension, &document, A4_SVG, &fonts).unwrap();
        let found =
            extract_thumbnail(&mut Cursor::new(saved)).unwrap_or_else(|| panic!("{extension}"));
        assert_eq!(found.mime, "image/png", "{extension}");
        assert_eq!(found.bytes, expected, "{extension}");
    }
}

#[test]
fn a_format_without_a_preview_part_and_a_document_over_the_budget_are_refused() {
    let fonts = fonts();
    assert!(with_first_page("pdf", b"%PDF-1.4", A4_SVG, &fonts).is_err());
    assert!(with_first_page("rtf", b"{\\rtf1}", A4_SVG, &fonts).is_err());
    assert!(with_first_page("csv", b"a,b\n", A4_SVG, &fonts).is_err());
    let huge = vec![0_u8; MAX_EMBED_DOCUMENT_BYTES + 1];
    assert!(with_first_page("hwp", &huge, A4_SVG, &fonts).is_err());
}

#[test]
fn a_failed_preview_never_fails_the_save() {
    let docx = docx();
    let base64 = base64_codec::encode(&docx);
    for svg in [
        "broken",
        "",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"0\" height=\"0\"/>",
    ] {
        let request = SaveRequest::parse("문서.docx", &base64)
            .unwrap()
            .with_thumbnail(svg, &[]);
        assert_eq!(request.bytes(), docx.as_slice(), "{svg:?}");
    }
    // A format that has nowhere to keep the picture is saved as it is.
    let pdf = UniversalFormatConverter::encode(&model(), DocumentFormat::Pdf).unwrap();
    let request = SaveRequest::from_bytes("a.pdf", pdf.clone())
        .unwrap()
        .with_thumbnail(A4_SVG, &[]);
    assert_eq!(request.bytes(), pdf.as_slice());
}

#[test]
fn a_panic_while_making_the_picture_still_saves_the_original_bytes() {
    let docx = docx();
    let request = SaveRequest::parse("문서.docx", &base64_codec::encode(&docx)).unwrap();
    let kept = request.with_thumbnail_using(|_, _| -> Result<Vec<u8>, String> {
        panic!("injected panic in the thumbnail step")
    });
    assert_eq!(kept.bytes(), docx.as_slice());
    assert_eq!(kept.file_name(), "문서.docx");
    // The same request still works afterwards: a panic poisons nothing.
    let drawn = SaveRequest::parse("문서.docx", &base64_codec::encode(&docx))
        .unwrap()
        .with_thumbnail(A4_SVG, &[]);
    assert_ne!(drawn.bytes(), docx.as_slice());
}

#[test]
fn the_saved_file_shows_the_picture_and_still_opens_as_a_document() {
    let dir = tempfile::tempdir().unwrap();
    for (name, document) in [("a.hwp", hwp()), ("b.hwpx", hwpx()), ("c.docx", docx())] {
        let request = SaveRequest::parse(name, &base64_codec::encode(&document))
            .unwrap()
            .with_thumbnail(A4_SVG, &[]);
        assert_ne!(request.bytes(), document.as_slice(), "{name}");
        let path = dir.path().join(name);
        write_document_bytes(&path, request.bytes()).unwrap();
        let found = extract_thumbnail(&mut File::open(&path).unwrap()).unwrap();
        assert_eq!(found.mime, "image/png", "{name}");
        // The same bytes pass the check a save is made to pass.
        crate::save_verify::verify_document_bytes(
            request.extension(),
            &std::fs::read(&path).unwrap(),
        )
        .unwrap();
    }
}

/// First-page SVGs of real documents (set `EASYOFFICE_THUMBNAIL_SVG_DIR`), drawn and embedded:
/// `EASYOFFICE_THUMBNAIL_SVG_DIR=<dir> cargo test --release -p easyoffice-desktop thumbnails_are_measured -- --ignored --nocapture`.
#[test]
#[ignore = "measurement"]
fn thumbnails_are_measured() {
    let dir = std::env::var("EASYOFFICE_THUMBNAIL_SVG_DIR").expect("EASYOFFICE_THUMBNAIL_SVG_DIR");
    let started = Instant::now();
    let fonts = crate::pdf_fonts::shared_font_db(&[]);
    println!("font database {:?}", started.elapsed());
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    names.sort();
    let document = hwp();
    for path in names
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "svg"))
    {
        let svg = std::fs::read_to_string(path).unwrap();
        let started = Instant::now();
        let png = render_png(&svg, &fonts);
        let drawn = started.elapsed();
        let started = Instant::now();
        let embedded = with_first_page("hwp", &document, &svg, &fonts).map(|d| d.len());
        println!(
            "{}: svg {} B, png {:?} B in {drawn:?}; whole step {:?} in {:?}",
            path.file_name().unwrap().to_string_lossy(),
            svg.len(),
            png.as_ref().map(Vec::len),
            embedded.map(|len| len - document.len()),
            started.elapsed(),
        );
        if let Ok(png) = png {
            let out = path.with_extension("png");
            std::fs::write(out, png).unwrap();
        }
    }
}

#[test]
fn a_page_that_declares_entities_is_refused_and_a_picture_that_does_is_left_out() {
    use crate::hostile_svg::{entity_bomb, gzip, page_with_picture, square};
    let started = Instant::now();
    assert!(render_png(&entity_bomb(6), &fonts()).is_err());
    assert!(render_png(&format!("<!DOCTYPE svg>{}", square()), &fonts()).is_err());
    for picture in [entity_bomb(6).into_bytes(), gzip(entity_bomb(6).as_bytes())] {
        let png = render_png(&page_with_picture(&picture), &fonts()).expect("the page is drawn");
        assert_eq!(
            pixel(&png, 1, 1),
            [255, 255, 255, 255],
            "the picture is not drawn"
        );
    }
    // Controls: the same pictures without a declaration are drawn (the square is black).
    for picture in [square().into_bytes(), gzip(square().as_bytes())] {
        let png = render_png(&page_with_picture(&picture), &fonts()).expect("png");
        assert_ne!(pixel(&png, 1, 1), [255, 255, 255, 255]);
    }
    assert!(started.elapsed().as_secs() < 10, "{:?}", started.elapsed());
}

#[test]
fn an_untyped_data_picture_is_left_out_of_the_card_and_the_page_is_drawn() {
    use crate::hostile_svg::{
        page_with_href, picture_that_looks_like_a_raster_header, untyped_data_urls,
    };
    for href in untyped_data_urls(&picture_that_looks_like_a_raster_header()) {
        let png = render_png(&page_with_href(&href), &fonts()).expect("the page is drawn");
        assert_eq!(pixel(&png, 1, 1), [255, 255, 255, 255], "{href:.60}");
    }
}

#[test]
fn svg_pages_and_pictures_over_element_filter_or_use_budgets_are_refused() {
    use crate::hostile_svg::{excessive_filters, page_with_picture, tag_flood, use_explosion_dag};
    let fonts = fonts();
    for hostile in [
        use_explosion_dag(16),
        excessive_filters(80, 4),
        tag_flood(105_000),
    ] {
        assert!(
            render_png(&hostile, &fonts).is_err(),
            "page must be refused"
        );
        // As a nested data:image/svg+xml picture, the hostile SVG is dropped and the page still draws.
        let png = render_png(&page_with_picture(hostile.as_bytes()), &fonts)
            .expect("host page renders without hostile nested svg");
        assert_eq!(pixel(&png, 1, 1), [255, 255, 255, 255]);
    }
}

#[test]
fn non_hangul_documents_draw_and_embed_first_page_thumbnails_automatically() {
    use easyoffice_app::{EasyOfficeIpcRouter, ShellWriters};
    let bundled_fonts = Arc::new(
        crate::pdf_fonts::tests::bundled_only_db()
            .expect("bundled NotoSansKR-Regular.ttf; run scripts/checkout-rhwp.mjs"),
    );
    let bundled_dirs: Vec<std::path::PathBuf> = crate::pdf_fonts::tests::bundled_font_dir()
        .into_iter()
        .collect();
    let mut full = model();
    full.sheet_cells = vec![(0, 0, "매출".to_string()), (1, 0, "=10+20".to_string())];
    full.slides = vec![(
        "분기 발표".to_string(),
        vec!["핵심 지표".to_string()],
        String::new(),
    )];
    // Pure-Korean content drawn against ONLY bundled Noto Sans KR (zero host OS fonts).
    for (ext, bytes) in [
        ("docx", docx()),
        (
            "odt",
            UniversalFormatConverter::encode_rich(&rich(), DocumentFormat::Odt, None).unwrap(),
        ),
        (
            "xlsx",
            UniversalFormatConverter::encode(&full, DocumentFormat::Xlsx).unwrap(),
        ),
        (
            "ods",
            UniversalFormatConverter::encode(&full, DocumentFormat::Ods).unwrap(),
        ),
        (
            "pptx",
            UniversalFormatConverter::encode(&full, DocumentFormat::Pptx).unwrap(),
        ),
        (
            "odp",
            UniversalFormatConverter::encode(&full, DocumentFormat::Odp).unwrap(),
        ),
        ("hwpx", hwpx()),
    ] {
        let embedded = with_drawn_first_page(ext, &bytes, &bundled_fonts).unwrap();
        let found = extract_thumbnail(&mut Cursor::new(&embedded))
            .unwrap_or_else(|| panic!("missing thumbnail in {ext}"));
        check_thumbnail_png(&found.bytes).unwrap();
        let pixmap = tiny_skia::Pixmap::decode_png(&found.bytes).unwrap();
        assert_eq!(pixmap.width().max(pixmap.height()), MAX_THUMBNAIL_SIDE_PX);
        assert!(
            pixmap
                .pixels()
                .iter()
                .any(|p| [p.red(), p.green(), p.blue()] != [255, 255, 255]),
            "non-white ink drawn for pure-Korean .{ext} with bundled font"
        );
    }

    // SaveRequest::with_drawn_thumbnail draws page 1 and skips idempotently on a second pass.
    // Includes Latin + Korean text because another unit test in this binary may have initialized
    // the process-wide `shared_font_db(&[])` before `bundled_dirs` was passed.
    let mixed_rich = RichDocument {
        page_frame: Default::default(),
        title: "Report 저장 시험".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("Body 본문")],
            ..RichParagraph::default()
        })],
        official_agency: None,
        ..RichDocument::default()
    };
    for (name, bytes) in [
        (
            "report.docx",
            UniversalFormatConverter::encode_rich(&mixed_rich, DocumentFormat::Docx, None).unwrap(),
        ),
        (
            "report.odt",
            UniversalFormatConverter::encode_rich(&mixed_rich, DocumentFormat::Odt, None).unwrap(),
        ),
        (
            "book.xlsx",
            UniversalFormatConverter::encode(&full, DocumentFormat::Xlsx).unwrap(),
        ),
        (
            "book.ods",
            UniversalFormatConverter::encode(&full, DocumentFormat::Ods).unwrap(),
        ),
        (
            "deck.pptx",
            UniversalFormatConverter::encode(&full, DocumentFormat::Pptx).unwrap(),
        ),
        (
            "deck.odp",
            UniversalFormatConverter::encode(&full, DocumentFormat::Odp).unwrap(),
        ),
        (
            "doc.hwpx",
            UniversalFormatConverter::encode_rich(&mixed_rich, DocumentFormat::Hwpx, None).unwrap(),
        ),
    ] {
        assert!(!has_non_placeholder_thumbnail(&bytes), "{name}");
        let req = SaveRequest::parse(name, &base64_codec::encode(&bytes))
            .unwrap()
            .with_drawn_thumbnail(&bundled_dirs);
        assert!(has_non_placeholder_thumbnail(req.bytes()), "{name}");
        let found = extract_thumbnail(&mut Cursor::new(req.bytes()))
            .unwrap_or_else(|| panic!("missing thumbnail in {name}"));
        check_thumbnail_png(&found.bytes).unwrap();
        let pixmap = tiny_skia::Pixmap::decode_png(&found.bytes).unwrap();
        assert_eq!(pixmap.width().max(pixmap.height()), MAX_THUMBNAIL_SIDE_PX);
        assert!(
            pixmap
                .pixels()
                .iter()
                .any(|p| [p.red(), p.green(), p.blue()] != [255, 255, 255]),
            "non-white ink drawn for {name}"
        );
        // Second pass skips without rewriting when a real thumbnail is already inside.
        let again = SaveRequest::parse(name, &base64_codec::encode(req.bytes()))
            .unwrap()
            .with_drawn_thumbnail(&bundled_dirs);
        assert_eq!(again.bytes(), req.bytes(), "{name} was rewritten twice");
    }

    // ShellWriters.thumbnail embeds the rhwp-laid first page directly during convert_document_format.
    let draw_thumb = |h: &[u8]| render_hwpx_first_page_png(h, &bundled_fonts);
    let save_hwp = |h: &[u8]| crate::doc_hwp::hwpx_to_hwp(h);
    let writers = ShellWriters {
        pdf: None,
        hwp: Some(&save_hwp),
        thumbnail: Some(&draw_thumb),
        ..ShellWriters::default()
    };
    for target in ["docx", "odt", "hwpx", "hwp"] {
        let payload = format!(
            r#"{{"sourceFormat":"docx","targetFormat":"{target}","forSave":true,"title":"보고서","blocks":[{{"type":"paragraph","heading":1,"runs":[{{"text":"첫 쪽 제목"}}]}},{{"type":"paragraph","runs":[{{"text":"본문 단락입니다."}}]}}]}}"#
        );
        let response = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
            "convert_document_format",
            &payload,
            "",
            writers,
        )
        .unwrap();
        let marker = "\"convertedBase64\":\"";
        let start = response.find(marker).unwrap() + marker.len();
        let b64 = &response[start..start + response[start..].find('"').unwrap()];
        let out = EasyOfficeIpcRouter::decode_base64(b64).unwrap();
        let found = extract_thumbnail(&mut Cursor::new(&out))
            .unwrap_or_else(|| panic!("missing thumbnail in converted {target}"));
        check_thumbnail_png(&found.bytes).unwrap();
        let pixmap = tiny_skia::Pixmap::decode_png(&found.bytes).unwrap();
        assert_eq!((pixmap.width(), pixmap.height()), (181, 256), "{target}");
    }
}

#[test]
fn generic_fallback_draws_ink_even_when_bundled_noto_is_absent() {
    // Simulates a Linux runner without Noto Sans KR or Korean fonts in the DB.
    let mut sys = Database::new();
    sys.load_system_fonts();
    let latin_id =
        sys.faces()
            .find(|f| {
                sys.with_face_data(f.id, |d, idx| {
                    ttf_parser::Face::parse(d, idx)
                        .ok()
                        .and_then(|face| face.glyph_index('R'))
                        .is_some()
                })
                .unwrap_or(false)
                    && !f.families.iter().any(|(n, _)| {
                        n.contains("Noto") || n.contains("Apple") || n.contains("Gothic")
                    })
            })
            .expect("at least one Latin non-Korean system face")
            .id;
    let bytes = sys
        .with_face_data(latin_id, |data, _| data.to_vec())
        .expect("face bytes");
    let mut db = Database::new();
    db.load_font_data(bytes);
    crate::pdf_fonts::set_generic_families(&mut db);
    let db = Arc::new(db);

    let latin_doc = RichDocument {
        page_frame: Default::default(),
        title: "Report 1".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("Summary line")],
            ..RichParagraph::default()
        })],
        official_agency: None,
        ..RichDocument::default()
    };
    let docx_bytes =
        UniversalFormatConverter::encode_rich(&latin_doc, DocumentFormat::Docx, None).unwrap();
    let out = with_drawn_first_page("report.docx", &docx_bytes, &db).unwrap();
    let found = extract_thumbnail(&mut Cursor::new(&out)).unwrap();
    let pixmap = tiny_skia::Pixmap::decode_png(&found.bytes).unwrap();
    assert!(
        pixmap
            .pixels()
            .iter()
            .any(|p| [p.red(), p.green(), p.blue()] != [255, 255, 255]),
        "generic serif/sans-serif must still resolve when Noto Sans KR is absent"
    );
}
