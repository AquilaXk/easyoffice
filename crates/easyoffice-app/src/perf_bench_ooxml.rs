//! Inputs of the newer paths in the measurement harness (`perf_bench.rs`, `docs/performance.md`): a
//! Word-style document (1,000 bookmarks, a cross reference to each, section breaks), a sheet with a
//! page setup (50,000 rows, manual page breaks, a print area, repeated rows) and a 500 slide deck
//! with groups, effects, animations, footers and shared pictures. `bench_generate_inputs` writes them
//! beside the other inputs, so `open` and `save` measure the reading and writing of each.

use crate::convert_structured::StructuredContent;
use easyoffice_formats::{
    deflate_zlib, AnimClass, AnimDir, AnimEffect, AnimStart, CellBox, ElementFx, IeeeCrc32,
    PdfWriteOptions, RichBlock, RichDocument, RichField, RichImage, RichParagraph, RichRun,
    RichSectionBreak, RichSlide, RichSlideDeck, SheetPage, SheetStyling, SlideAnim, SlideAspect,
    SlideBackground, SlideBox, SlideDash, SlideElement, SlideExt, SlideFooter, SlideGroup,
    SlideOrientation, SlideOutline, SlideShadow, SlideShapeType, SlideTextAlign,
};

const WORD_PARAGRAPHS: usize = 10_000;
const BOOKMARK_EVERY: usize = 10;
const SECTION_EVERY: usize = 500;
const PAGE_ROWS: u32 = 50_000;
const PAGE_BREAK_EVERY: u32 = 50;
const DECK_SLIDES: usize = 500;
const DECK_PICTURES: usize = 4;

/// 10,000 paragraphs: a bookmark on every tenth, a `REF` field to the one before in the next
/// paragraph, a page field in every hundredth and a section break every 500th paragraph.
pub(crate) fn word_document() -> RichDocument {
    let mut blocks = Vec::with_capacity(WORD_PARAGRAPHS);
    for index in 0..WORD_PARAGRAPHS {
        let mut runs = Vec::with_capacity(3);
        if index.is_multiple_of(BOOKMARK_EVERY) {
            runs.push(RichRun {
                text: format!("제{index}항 기준"),
                bookmark_start: Some(format!("bm_{index}")),
                bookmark_end: Some(format!("bm_{index}")),
                ..RichRun::default()
            });
        } else if index % BOOKMARK_EVERY == 1 {
            let target = index - 1;
            runs.push(RichRun::plain("위 내용은 ".to_string()));
            runs.push(RichRun {
                text: format!("제{target}항 기준"),
                field_code: Some(format!("REF bm_{target} \\h")),
                ..RichRun::default()
            });
        } else {
            runs.push(RichRun::plain(format!(
                "{index}. 예산 집행과 성과 점검을 분기별로 나누어 보고한다."
            )));
        }
        if index % 100 == 99 {
            runs.push(RichRun {
                field: Some(RichField::PageNumber),
                ..RichRun::default()
            });
        }
        blocks.push(RichBlock::Paragraph(RichParagraph {
            runs,
            heading_level: if index.is_multiple_of(50) { 2 } else { 0 },
            section_break: (index % SECTION_EVERY == SECTION_EVERY - 1)
                .then_some(RichSectionBreak::NextPage),
            ..RichParagraph::default()
        }));
    }
    RichDocument {
        title: "Word 기능 보고서".to_string(),
        blocks,
        ..RichDocument::default()
    }
}

pub(crate) fn word_content() -> StructuredContent {
    StructuredContent::Document(word_document(), PdfWriteOptions::default())
}

/// 50,000 rows by 3 columns with manual page breaks every 50 rows, a print area, repeated rows.
pub(crate) fn paged_workbook() -> StructuredContent {
    let cells = (0..PAGE_ROWS)
        .flat_map(|row| {
            [
                (row, 0, format!("항목 {row}")),
                (row, 1, (row * 17 % 9_973).to_string()),
                (row, 2, format!("{}.5", row % 311)),
            ]
        })
        .collect();
    let styling = SheetStyling {
        page: SheetPage {
            landscape: true,
            grid_lines: true,
            print_area: vec![CellBox {
                row0: 0,
                col0: 0,
                row1: PAGE_ROWS - 1,
                col1: 2,
            }],
            title_rows: Some((0, 0)),
            row_breaks: (1..PAGE_ROWS / PAGE_BREAK_EVERY)
                .map(|n| n * PAGE_BREAK_EVERY)
                .collect(),
            ..SheetPage::default()
        },
        ..SheetStyling::default()
    };
    StructuredContent::Workbook(vec![("Sheet1".to_string(), cells)], vec![styling])
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = IeeeCrc32::update(IeeeCrc32::checksum(kind), data);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

/// A valid 2x2 PNG whose colour is `seed`, with a padding chunk of `extra` bytes.
fn picture(seed: u8, extra: usize) -> RichImage {
    let raw: Vec<u8> = (0..2)
        .flat_map(|_| [0u8, seed, 99, 235, 255, seed, 99, 235, 255])
        .collect();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    bytes.extend_from_slice(&png_chunk(b"tEXt", &vec![seed; extra]));
    bytes.extend_from_slice(&png_chunk(b"IDAT", &deflate_zlib(&raw)));
    bytes.extend_from_slice(&png_chunk(b"IEND", &[]));
    RichImage::new(bytes, "").expect("picture")
}

fn boxed(x: f64, y: f64) -> SlideBox {
    SlideBox {
        x,
        y,
        w: 120.0,
        h: 60.0,
    }
}

/// One slide of the deck: twelve elements, three groups, effects on half of them, four animations,
/// a footer, and a background that is one of [`DECK_PICTURES`] shared pictures on every other slide.
fn slide(index: usize, pictures: &[RichImage]) -> RichSlide {
    let mut elements = Vec::with_capacity(12);
    let mut fx = Vec::new();
    for n in 0..12usize {
        let id = format!("e{n}");
        let rect = boxed(20.0 + 40.0 * (n % 6) as f64, 20.0 + 70.0 * (n / 6) as f64);
        elements.push(match n % 4 {
            0 => SlideElement::TextBox {
                id: id.clone(),
                rect,
                text: format!("슬라이드 {index} 문구 {n}"),
                font_size_pt: 18.0,
                bold: n == 0,
                italic: false,
                underline: false,
                align: SlideTextAlign::Left,
                color: None,
                fill: None,
                role: None,
            },
            1 => SlideElement::Shape {
                id: id.clone(),
                rect,
                shape_type: SlideShapeType::RoundRect,
                text: String::new(),
                fill: Some("#2563EB".to_string()),
                stroke: None,
                stroke_width_pt: 1.0,
                color: None,
                font_size_pt: 14.0,
                bold: false,
                italic: false,
                underline: false,
                align: SlideTextAlign::Center,
            },
            2 => SlideElement::Image {
                id: id.clone(),
                rect,
                image: pictures[(index + n) % DECK_PICTURES].clone(),
                alt: String::new(),
            },
            _ => SlideElement::Table {
                id: id.clone(),
                rect,
                rows: vec![vec!["a".to_string(), "b".to_string()]; 3],
                header_row: true,
                fill: None,
            },
        });
        if n.is_multiple_of(2) && n % 4 != 3 {
            let mut effect = ElementFx::new(&id);
            effect.rotation_deg = 15.0;
            effect.dash = SlideDash::Dash;
            effect.outline = Some(SlideOutline {
                color: "#111827".to_string(),
                width_pt: 1.5,
            });
            effect.shadow = Some(SlideShadow {
                color: "#000000".to_string(),
                alpha_pct: 40,
                blur_pt: 4.0,
                dist_pt: 3.0,
                angle_deg: 45.0,
            });
            fx.push(effect);
        }
    }
    let groups = vec![
        SlideGroup {
            id: "g0".to_string(),
            members: vec!["e0".into(), "e1".into(), "e2".into()],
        },
        SlideGroup {
            id: "g1".to_string(),
            members: vec!["g0".into(), "e3".into()],
        },
        SlideGroup {
            id: "g2".to_string(),
            members: vec!["e5".into(), "e6".into(), "e7".into()],
        },
    ];
    let anims = (0..4)
        .map(|n| SlideAnim {
            target: format!("e{n}"),
            class: AnimClass::Entrance,
            effect: AnimEffect::Fade,
            start: if n == 0 {
                AnimStart::OnClick
            } else {
                AnimStart::AfterPrevious
            },
            duration_ms: 500,
            delay_ms: 0,
            direction: AnimDir::Bottom,
        })
        .collect();
    RichSlide {
        title: format!("슬라이드 {index}"),
        notes: format!("발표자 메모 {index}"),
        elements,
        ext: SlideExt {
            background: index
                .is_multiple_of(2)
                .then(|| SlideBackground::Picture(pictures[index % DECK_PICTURES].clone())),
            footer: SlideFooter {
                number: true,
                text: Some("보고서".to_string()),
                ..SlideFooter::default()
            },
            fx,
            groups,
            anims,
            ..SlideExt::default()
        },
        ..RichSlide::default()
    }
}

/// 500 slides of [`slide`], four pictures shared by the backgrounds and the picture elements.
pub(crate) fn slide_deck() -> StructuredContent {
    let pictures: Vec<RichImage> = (0..DECK_PICTURES)
        .map(|n| picture(n as u8 + 1, 60_000))
        .collect();
    StructuredContent::SlideDeck(RichSlideDeck {
        title: "확장 덱".to_string(),
        is_slideshow: false,
        aspect: SlideAspect::Widescreen16x9,
        orientation: SlideOrientation::Landscape,
        slides: (0..DECK_SLIDES).map(|i| slide(i, &pictures)).collect(),
        elements_rejected: 0,
        settings: Default::default(),
    })
}
