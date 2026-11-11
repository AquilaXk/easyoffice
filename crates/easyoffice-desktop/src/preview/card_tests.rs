//! Cards drawn from content (PD-179): the layout, the engine's content for each format, the limits
//! and the hostile files. The real engine writes the test documents.

use super::bounded::RenderError;
use super::card::{self, Align, Block, Content};
use super::card_source::{self, card_png, content_of};
use super::source::{whole_lines, Source, TEXT_PREFIX_BYTES};
use super::tests::zip;
use super::*;
use crate::pdf_fonts::shared_font_db;
use easyoffice_formats::{
    DocumentFormat, OdfCodec, OdsUnpacked, OoxmlCodec, OoxmlPptxPayload, RichBlock, RichDocument,
    RichListKind, RichParagraph, RichRun, UniversalFormatConverter,
};
use std::fs;
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

const PNG_1X1: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 0,
    1, 0, 0, 0, 1, 8, 6, 0, 0, 0,
];

const KOREAN: &str = "분기 실적 보고서 첫 문단입니다. 매출은 전 분기보다 늘었고 비용은 줄었습니다.";

fn fonts() -> Arc<usvg::fontdb::Database> {
    shared_font_db(&[])
}

fn png_size(png: &[u8]) -> (u32, u32) {
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']), "not a PNG");
    let at = |i: usize| u32::from_be_bytes(png[i..i + 4].try_into().unwrap());
    (at(16), at(20))
}

fn non_white(png: &[u8]) -> usize {
    tiny_skia::Pixmap::decode_png(png)
        .expect("a PNG")
        .pixels()
        .iter()
        .filter(|p| (p.red(), p.green(), p.blue()) != (255, 255, 255))
        .count()
}

fn paragraph(text: &str) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        ..RichParagraph::default()
    })
}

fn document() -> RichDocument {
    RichDocument {
        title: "보고서".to_string(),
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("제목 Heading")],
                heading_level: 1,
                ..RichParagraph::default()
            }),
            paragraph(KOREAN),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("글머리 항목")],
                list: Some(RichListKind::Bullet),
                ..RichParagraph::default()
            }),
            RichBlock::Table(
                vec![
                    vec!["구분".to_string(), "값".to_string()],
                    vec!["a".to_string(), "1".to_string()],
                ]
                .into(),
            ),
        ],
        official_agency: None,
    }
}

fn sheets() -> Vec<OdsUnpacked> {
    let cell = |row: u32, col: u32, raw: &str| (row, col, raw.to_string());
    vec![
        (
            "Sheet1".to_string(),
            vec![
                cell(0, 0, "품목"),
                cell(0, 1, "수량"),
                cell(0, 2, "합계"),
                cell(1, 0, "사과"),
                cell(1, 1, "2"),
                cell(1, 2, "=B2*3"),
                cell(2, 0, "배"),
                cell(2, 1, "5"),
                cell(2, 2, "=SUM(B2:B3)+Sheet2!A1"),
            ],
        ),
        ("Sheet2".to_string(), vec![cell(0, 0, "100")]),
    ]
}

fn slides() -> Vec<(String, Vec<String>, String)> {
    vec![
        (
            "분기 보고".to_string(),
            vec!["매출 증가".to_string(), "비용 절감".to_string()],
            "노트".to_string(),
        ),
        ("둘째 장".to_string(), vec!["x".to_string()], String::new()),
    ]
}

fn encode(format: DocumentFormat) -> Vec<u8> {
    UniversalFormatConverter::encode_rich(&document(), format, None).expect("encoded")
}

/// A file of each format the engine writes, with the extension a recent entry has.
fn samples() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("docx", encode(DocumentFormat::Docx)),
        ("odt", encode(DocumentFormat::Odt)),
        ("rtf", encode(DocumentFormat::Rtf)),
        ("md", encode(DocumentFormat::Markdown)),
        ("txt", encode(DocumentFormat::Txt)),
        ("pages", encode(DocumentFormat::Pages)),
        ("xlsx", OoxmlCodec::pack_xlsx_multi(&sheets()).unwrap()),
        ("ods", OdfCodec::pack_ods_multi(&sheets()).unwrap()),
        (
            "csv",
            "품목,수량,합계\n사과,2,6\n배,5,7\n".as_bytes().to_vec(),
        ),
        ("tsv", "품목\t수량\n사과\t2\n".as_bytes().to_vec()),
        (
            "pptx",
            OoxmlCodec::pack_pptx(&OoxmlPptxPayload {
                title: "발표".to_string(),
                is_slideshow: false,
                slides: slides(),
            })
            .unwrap(),
        ),
        ("odp", OdfCodec::pack_odp("발표", &slides()).unwrap()),
    ]
}

// --- layout -------------------------------------------------------------------------------

#[test]
fn text_is_wrapped_at_spaces_and_inside_words_that_do_not_fit() {
    let lines = card::wrap("alpha beta gamma delta", 10.0, 60.0, 10);
    assert!(lines.len() > 1);
    assert!(lines
        .iter()
        .all(|line| !line.starts_with(' ') && !line.ends_with(' ')));
    assert_eq!(
        lines.join(" "),
        "alpha beta gamma delta",
        "no word is lost or split"
    );
    let long = card::wrap(&"x".repeat(200), 10.0, 100.0, 50);
    assert!(long.len() > 5, "a word wider than a line is broken");
    assert_eq!(long.concat(), "x".repeat(200));
    let korean = card::wrap(KOREAN, 13.0, 200.0, 50);
    assert_eq!(korean.concat().replace(' ', ""), KOREAN.replace(' ', ""));
    assert_eq!(
        card::wrap(KOREAN, 13.0, 40.0, 3).len(),
        3,
        "at most the lines asked"
    );
    assert!(card::wrap("", 13.0, 100.0, 5).is_empty());
    assert!(card::wrap("     ", 13.0, 100.0, 5).is_empty());
}

#[test]
fn every_text_is_escaped_and_the_svg_is_valid_xml() {
    let hostile = "<script>&amp; \"quote\" 'apos' \u{0}\u{1}\u{8}\u{b}\u{1f}\u{7f}</text><rect/>";
    let content = Content::Page(vec![
        Block::Paragraph {
            text: hostile.to_string(),
            heading: 1,
            bullet: true,
            align: Align::Center,
        },
        Block::Table(vec![vec![hostile.to_string(), hostile.to_string()]]),
    ]);
    let svg = card::svg(&content);
    assert!(!svg.contains("<script"), "{svg}");
    assert!(!svg.contains("<rect/>"));
    assert!(roxmltree::Document::parse(&svg).is_ok(), "well-formed XML");
    let png = render_png_sized(&svg, &fonts(), 480).expect("drawn");
    assert_eq!(png_size(&png), (480, 360));
    let grid = card::svg(&Content::Grid(vec![vec![hostile.to_string(); 9]; 40]));
    assert!(roxmltree::Document::parse(&grid).is_ok());
    let slide = card::svg(&Content::Slide {
        title: hostile.to_string(),
        lines: vec![hostile.to_string(); 40],
    });
    assert!(roxmltree::Document::parse(&slide).is_ok());
}

use crate::save_thumbnail::render_png_sized;

#[test]
fn the_svg_does_not_grow_with_the_document() {
    let long = "가나다라 ".repeat(5_000);
    let blocks: Vec<Block> = (0..2_000)
        .map(|_| Block::Paragraph {
            text: long.clone(),
            heading: 0,
            bullet: false,
            align: Align::Left,
        })
        .collect();
    let page = card::svg(&Content::Page(blocks));
    assert!(page.len() < 64 * 1024, "{} bytes", page.len());
    let grid = card::svg(&Content::Grid(vec![vec![long.clone(); 100]; 100]));
    assert!(grid.len() < 64 * 1024, "{} bytes", grid.len());
    let slide = card::svg(&Content::Slide {
        title: long.clone(),
        lines: vec![long; 2_000],
    });
    assert!(slide.len() < 64 * 1024, "{} bytes", slide.len());
}

// --- content of each format -----------------------------------------------------------------

#[test]
fn a_document_gives_its_first_paragraphs() {
    for (extension, bytes) in samples() {
        let Content::Page(blocks) = content_of(&bytes, extension).unwrap_or_else(|e| {
            panic!("{extension}: {e:?}");
        }) else {
            continue;
        };
        let text: String = blocks
            .iter()
            .filter_map(|block| match block {
                Block::Paragraph { text, .. } => Some(text.as_str()),
                Block::Table(_) => None,
            })
            .collect();
        assert!(text.contains("분기 실적 보고서"), "{extension}: {text}");
    }
}

#[test]
fn a_sheet_gives_its_first_cells_with_formula_values() {
    for extension in ["xlsx", "ods"] {
        let bytes = samples()
            .into_iter()
            .find(|(e, _)| *e == extension)
            .unwrap()
            .1;
        let Content::Grid(rows) = content_of(&bytes, extension).unwrap() else {
            panic!("{extension}: not a grid");
        };
        assert_eq!(rows[0][..3], ["품목", "수량", "합계"], "{extension}");
        assert_eq!(rows[1][2], "6", "{extension}: =B2*3");
        assert_eq!(
            rows[2][2], "107",
            "{extension}: a formula that reads another sheet"
        );
    }
    let csv = content_of("a,b\n1,2\n".as_bytes(), "csv").unwrap();
    let Content::Grid(rows) = csv else { panic!() };
    assert_eq!(rows[1][..2], ["1", "2"]);
}

#[test]
fn a_slide_deck_gives_its_first_slide() {
    for extension in ["pptx", "odp"] {
        let bytes = samples()
            .into_iter()
            .find(|(e, _)| *e == extension)
            .unwrap()
            .1;
        let Content::Slide { title, lines } = content_of(&bytes, extension).unwrap() else {
            panic!("{extension}: not a slide");
        };
        assert_eq!(title, "분기 보고", "{extension}");
        assert_eq!(lines, ["매출 증가", "비용 절감"], "{extension}");
    }
}

#[test]
fn the_grid_keeps_to_its_window_and_a_far_cell_costs_nothing() {
    let cell = |row: u32, col: u32, raw: &str| (row, col, raw.to_string());
    let far = vec![(
        "S".to_string(),
        vec![
            cell(0, 0, "near"),
            cell(0, 4_000_000, "far column"),
            cell(3_000_000, 0, "far row"),
        ],
    )];
    let rows = card_source::grid(&far);
    assert_eq!(rows.len(), card::GRID_ROWS);
    assert_eq!(rows[0][0], "near");
    assert!(rows.iter().flatten().filter(|c| !c.is_empty()).count() == 1);
    assert!(card_source::grid(&[]).is_empty());
    // A workbook too large for the formula engine shows the formula text, not a wrong value.
    let many: Vec<(u32, u32, String)> = std::iter::once((0, 0, "=1+1".to_string()))
        .chain((0..250_000_u32).map(|n| (n / 16_000, n % 16_000, "1".to_string())))
        .collect();
    let mut sorted = many;
    sorted.sort_by_key(|(r, c, _)| (*r, *c));
    sorted.dedup_by_key(|(r, c, _)| (*r, *c));
    let big = vec![("S".to_string(), sorted)];
    assert_eq!(card_source::grid(&big)[0][0], "=1+1");
}

// --- pictures ------------------------------------------------------------------------------

#[test]
fn every_format_gets_a_card_picture_at_the_card_size() {
    for (extension, bytes) in samples() {
        let png =
            card_png(&bytes, extension, &fonts()).unwrap_or_else(|e| panic!("{extension}: {e:?}"));
        assert_eq!(png_size(&png), (480, 360), "{extension}");
        assert!(
            non_white(&png) > 500,
            "{extension}: the card has its content"
        );
        assert!(png.len() < MAX_RENDERED_PNG_BYTES, "{extension}");
        if let Ok(folder) = std::env::var("EASYOFFICE_PREVIEW_DUMP") {
            fs::write(
                Path::new(&folder).join(format!("card-{extension}.png")),
                &png,
            )
            .unwrap();
        }
    }
}

#[test]
fn an_empty_document_is_a_white_card_not_an_error() {
    let empty =
        UniversalFormatConverter::encode_rich(&RichDocument::default(), DocumentFormat::Docx, None)
            .unwrap();
    let png = card_png(&empty, "docx", &fonts()).unwrap();
    assert_eq!((png_size(&png), non_white(&png)), ((480, 360), 0));
}

// --- hostile files --------------------------------------------------------------------------

#[test]
fn damaged_files_give_no_card_and_no_panic() {
    let fonts = fonts();
    for (extension, bytes) in samples() {
        let mut flipped = bytes.clone();
        for at in (0..flipped.len()).step_by(53) {
            flipped[at] ^= 0xA5;
        }
        for (what, damaged) in [
            ("cut", bytes[..bytes.len() / 2].to_vec()),
            ("flipped", flipped),
            ("empty", Vec::new()),
            ("garbage", vec![0x5A; 4096]),
        ] {
            let started = Instant::now();
            // Plain text decodes whatever it is; the others refuse or draw what is left.
            let _ = card_png(&damaged, extension, &fonts);
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "{extension} {what}"
            );
        }
    }
    for extension in ["docx", "xlsx", "pptx", "odt", "ods", "odp", "pages"] {
        assert!(
            card_png(b"not a package", extension, &fonts).is_err(),
            "{extension}"
        );
        assert!(card_png(&[], extension, &fonts).is_err(), "{extension}");
    }
}

#[test]
fn a_package_that_declares_a_huge_inflation_is_refused_before_it_is_read() {
    let declared = (card_source::MAX_DECLARED_PACKAGE_BYTES + 1) as usize;
    let bomb = zip(&[("word/document.xml", 8, vec![0x03, 0x00], declared)]);
    assert_eq!(content_of(&bomb, "docx"), Err(RenderError::TooLarge));
    // At the limit the engine decides (this one is no document).
    let at_limit = zip(&[(
        "word/document.xml",
        0,
        vec![0],
        card_source::MAX_DECLARED_PACKAGE_BYTES as usize,
    )]);
    assert_eq!(content_of(&at_limit, "docx"), Err(RenderError::Unreadable));
    // A directory that cannot be read at all is refused too.
    let mut broken = zip(&[("a", 0, vec![1], 1)]);
    let end = broken.len();
    broken[end - 6] = 0xFF;
    assert!(content_of(&broken, "xlsx").is_err());
}

#[test]
fn a_compression_bomb_is_refused_by_the_engine_without_inflating_it() {
    let zeros = easyoffice_formats::deflate_raw(&vec![0_u8; 120 << 20]);
    let bomb = zip(&[("xl/worksheets/sheet1.xml", 8, zeros.clone(), 120 << 20)]);
    let started = Instant::now();
    assert!(zeros.len() < 400 * 1024, "{} bytes", zeros.len());
    assert!(content_of(&bomb, "xlsx").is_err());
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
}

// --- reading -------------------------------------------------------------------------------

#[test]
fn a_text_file_is_read_to_its_prefix_cut_at_a_line() {
    let line = "가나다,라마바,12345\n";
    let text = line.repeat(80_000);
    assert!(text.len() > 5 * TEXT_PREFIX_BYTES);
    let prefix = read_document(
        &mut Cursor::new(text.clone().into_bytes()),
        text.len() as u64,
        Source::Text,
    )
    .unwrap();
    assert!(prefix.len() <= TEXT_PREFIX_BYTES && prefix.len() > TEXT_PREFIX_BYTES - line.len() * 2);
    assert!(prefix.ends_with(b"\n"));
    assert!(std::str::from_utf8(&prefix).is_ok(), "no character is cut");
    // A small file is read whole; a binary kind refuses what is over its limit.
    let small = read_document(&mut Cursor::new(b"a,b\n1,2".to_vec()), 7, Source::Text).unwrap();
    assert_eq!(small, b"a,b\n1,2");
    let over = vec![0_u8; Source::Document.read_bytes() + 1];
    assert_eq!(
        read_document(&mut Cursor::new(over), 0, Source::Document),
        Err(RenderError::TooLarge)
    );
    assert_eq!(
        whole_lines(b"abc".to_vec()),
        b"abc",
        "no line break: kept whole"
    );
    assert_eq!(whole_lines(b"ab\ncd".to_vec()), b"ab\n");
}

#[test]
fn each_kind_has_the_limit_of_the_window_that_opens_it() {
    let dir = tempfile::tempdir().unwrap();
    let state = PreviewState::with_limits(
        Vec::new(),
        2,
        RenderLimits {
            deadline: Duration::from_secs(5),
            permit_wait: Duration::from_millis(100),
        },
    );
    let ten_mib = 10 * 1_048_576_u64;
    for (n, (name, limit)) in [
        ("a.pdf", ten_mib),
        ("b.docx", ten_mib),
        ("c.xlsx", ten_mib),
        ("d.pptx", ten_mib),
        ("e.hwp", 50 * 1_048_576),
    ]
    .into_iter()
    .enumerate()
    {
        let path = dir.path().join(name);
        let reached = Arc::new(AtomicUsize::new(0));
        for (step, size) in [limit + 1, limit].into_iter().enumerate() {
            fs::File::create(&path).unwrap().set_len(size).unwrap();
            let counter = Arc::clone(&reached);
            let id = format!("{:016x}", n * 10 + step);
            let found = load_preview_with(&state, &id, &path, move |bytes| {
                counter.store(bytes.len(), Ordering::SeqCst);
                Ok(PNG_1X1.to_vec())
            });
            if step == 0 {
                assert!(found.is_none(), "{name} over the limit");
                assert_eq!(reached.load(Ordering::SeqCst), 0, "{name}");
            } else {
                assert!(found.is_some(), "{name} at the limit");
                assert_eq!(reached.load(Ordering::SeqCst) as u64, limit, "{name}");
            }
        }
    }
}

#[test]
fn a_text_file_over_the_window_limit_is_not_drawn_but_one_within_is_read_in_part() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.csv");
    let state = PreviewState::new();
    fs::write(&path, "a,b\n".repeat(1_000_000)).unwrap();
    let seen = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&seen);
    let found = load_preview_with(&state, "00000000000000a1", &path, move |bytes| {
        counter.store(bytes.len(), Ordering::SeqCst);
        Ok(PNG_1X1.to_vec())
    });
    assert!(found.is_some());
    assert!(seen.load(Ordering::SeqCst) <= TEXT_PREFIX_BYTES);
    fs::File::create(&path)
        .unwrap()
        .set_len(10 * 1_048_576 + 1)
        .unwrap();
    assert!(
        load_preview_with(&state, "00000000000000a2", &path, |_| Ok(PNG_1X1.to_vec())).is_none()
    );
}

// --- the whole way ---------------------------------------------------------------------------

#[test]
fn recent_files_of_every_card_kind_get_a_picture_from_the_command_path() {
    let dir = tempfile::tempdir().unwrap();
    let state = PreviewState::new();
    for (n, (extension, bytes)) in samples().into_iter().enumerate() {
        let path = dir.path().join(format!("recent.{extension}"));
        fs::write(&path, bytes).unwrap();
        let found = load_preview(&state, &format!("{n:016x}"), &path)
            .unwrap_or_else(|| panic!("{extension} has a card"));
        assert_eq!(
            (found.mime, png_size(&found.bytes)),
            ("image/png", (480, 360)),
            "{extension}"
        );
    }
}

#[test]
fn a_picture_inside_a_pages_file_is_used_without_drawing() {
    let dir = tempfile::tempdir().unwrap();
    let jpg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0];
    let path = dir.path().join("apple.pages");
    fs::write(&path, zip(&[("preview.jpg", 0, jpg.clone(), jpg.len())])).unwrap();
    let found = load_preview_with(&PreviewState::new(), "00000000000000b1", &path, |_| {
        panic!("the picture in the file is shown, nothing is drawn")
    })
    .unwrap();
    assert_eq!((found.mime, &found.bytes), ("image/jpeg", &jpg));
}

#[test]
fn a_pdf_goes_to_the_worker_step_and_its_png_is_checked_like_any_other() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("paper.pdf");
    fs::write(&path, b"%PDF-1.7 whatever").unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&seen);
    let found = load_preview_with(
        &PreviewState::new(),
        "00000000000000c1",
        &path,
        move |bytes| {
            *kept.lock().unwrap() = bytes;
            Ok(PNG_1X1.to_vec())
        },
    );
    assert!(found.is_some());
    assert_eq!(&*seen.lock().unwrap(), b"%PDF-1.7 whatever");
    let huge = |_: Vec<u8>| {
        let mut png = PNG_1X1.to_vec();
        png.resize(MAX_RENDERED_PNG_BYTES + 1, 0);
        Ok(png)
    };
    assert!(load_preview_with(&PreviewState::new(), "00000000000000c2", &path, huge).is_none());
}

// --- timing --------------------------------------------------------------------------------

/// The cards of the files in `EASYOFFICE_PREVIEW_SAMPLES` (a folder), as the start window asks
/// for them: one after the other, the first one with the font database cold. PDFs are timed by
/// `tests/pdf_preview.rs` (they need the worker binary). Run with
/// `cargo test -p easyoffice-desktop --release measure_cards -- --ignored --nocapture`; set
/// `EASYOFFICE_PREVIEW_DUMP` to a folder to keep the pictures.
#[test]
#[ignore = "timing measurement, not a check"]
fn measure_cards() {
    let folder = std::env::var("EASYOFFICE_PREVIEW_SAMPLES").expect("EASYOFFICE_PREVIEW_SAMPLES");
    let mut paths: Vec<PathBuf> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| Source::of(path).is_some_and(|kind| kind != Source::Pdf))
        .collect();
    paths.sort();
    let state = PreviewState::new();
    for (n, path) in paths.iter().enumerate() {
        let size = fs::metadata(path).unwrap().len();
        let before = Instant::now();
        let embedded = extract_thumbnail(&mut fs::File::open(path).unwrap()).is_some();
        let before = before.elapsed();
        let started = Instant::now();
        let found = load_preview(&state, &format!("{n:016x}"), path);
        let cold = started.elapsed();
        let started = Instant::now();
        let again = load_preview(&state, &format!("{n:016x}"), path);
        println!(
            "{:<32} {:>9} bytes  embedded={embedded}  before(read only)={:?}  first={:?}  cached={:?}  png={} bytes",
            path.file_name().unwrap().to_string_lossy(),
            size,
            before,
            cold,
            started.elapsed(),
            found.as_ref().map_or(0, |f| f.bytes.len()),
        );
        assert_eq!(found.is_some(), again.is_some());
        if let (Some(found), Ok(out)) = (found, std::env::var("EASYOFFICE_PREVIEW_DUMP")) {
            let name = path.file_name().unwrap().to_string_lossy();
            fs::write(
                Path::new(&out).join(format!("sample-{name}.png")),
                &found.bytes,
            )
            .unwrap();
        }
    }
}
