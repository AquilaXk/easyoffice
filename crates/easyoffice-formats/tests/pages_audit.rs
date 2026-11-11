//! Pages reader audit: paragraph terminators, silently dropped elements reported as warnings,
//! merge formula store, floating objects and note settings. Synthetic packages only.

#[allow(dead_code)]
mod image_fixture;
mod pages_fixture;
mod pages_picture_fixture;

use easyoffice_formats::RichBlock;
use pages_fixture::*;
use pages_picture_fixture::*;

fn body_only(b: DocBuilder, st: u64) -> Vec<u8> {
    pages_zip(b.finish(st))
}

#[test]
fn every_paragraph_terminator_starts_a_new_paragraph_and_keeps_styles_aligned() {
    let mut b = DocBuilder::default();
    let bold = b.char_style(Some(true), None, None, None, None);
    // utf16 indexes: 가=0 CR=1 나=2 SEC=3 다=4 ENQ=5 라=6 FF=7 마=8
    let text = "가\r나\u{4}다\u{5}라\u{c}마";
    let st = b.add(
        2001,
        &DocBuilder::storage_payload(text, &[], &[(2, Some(bold)), (3, None)], &[], &[]),
    );
    let (doc, warnings) = open(&body_only(b, st)).expect("opens");
    assert_eq!(paragraph_texts(&doc), vec!["가", "나", "다", "라", "마"]);
    let RichBlock::Paragraph(second) = &doc.blocks[1] else {
        panic!("paragraph")
    };
    assert!(
        second.runs[0].bold,
        "style must land on the second paragraph"
    );
    assert!(warned(&warnings, "쪽·단 나누기 1개"), "{warnings:?}");
    assert!(warned(&warnings, "구역 나누기 1개"), "{warnings:?}");
}

fn zigzag(v: u64) -> u64 {
    v << 1
}

/// Formula node naming a rectangle: colon tract with absolute column and row ranges.
fn tract_node(cols: (u64, Option<u64>), rows: (u64, Option<u64>)) -> Vec<u8> {
    let axis = |(begin, end): (u64, Option<u64>)| {
        let mut o = f_varint(1, begin);
        if let Some(e) = end {
            o.extend(f_varint(2, e));
        }
        o
    };
    cat(&[
        f_varint(1, 67),
        f_bytes(
            40,
            &cat(&[f_bytes(3, &axis(cols)), f_bytes(4, &axis(rows))]),
        ),
    ])
}

fn cell_node(row: u64, col: u64) -> Vec<u8> {
    cat(&[
        f_varint(1, 36),
        f_bytes(26, &f_varint(1, zigzag(col))),
        f_bytes(27, &f_varint(1, zigzag(row))),
    ])
}

fn owner_table_document(nodes: &[Vec<u8>]) -> Vec<u8> {
    let grid = vec![
        vec!["가", "나", "다"],
        vec!["라", "마", "바"],
        vec!["사", "아", "자"],
    ];
    let mut b = DocBuilder::default();
    let table = b.table_with_owner_formulas(&grid, nodes);
    let att = b.attachment(table);
    let st = b.add(
        2001,
        &DocBuilder::storage_payload("\u{FFFC}", &[], &[], &[], &[(0, Some(att))]),
    );
    pages_zip(b.finish(st))
}

fn merges_of(doc: &easyoffice_formats::RichDocument) -> Vec<(u32, u32, u32, u32)> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Table(t) => Some(t),
            _ => None,
        })
        .flat_map(|t| {
            t.merges
                .iter()
                .map(|m| (m.row, m.col, m.row_span, m.col_span))
        })
        .collect()
}

#[test]
fn merge_formula_store_gives_regions_when_there_is_no_region_map() {
    let nodes = vec![
        tract_node((0, Some(1)), (1, None)),
        tract_node((2, None), (1, Some(2))),
    ];
    let (doc, warnings) = open(&owner_table_document(&nodes)).expect("opens");
    assert_eq!(merges_of(&doc), vec![(1, 0, 1, 2), (1, 2, 2, 1)]);
    assert!(!warned(&warnings, "병합"), "{warnings:?}");
}

#[test]
fn merge_formula_store_one_cell_references_merge_nothing() {
    let (doc, warnings) = open(&owner_table_document(&[cell_node(2, 0)])).expect("opens");
    assert!(merges_of(&doc).is_empty());
    assert!(!warned(&warnings, "병합"), "{warnings:?}");
}

#[test]
fn hostile_merge_formulas_are_ignored_without_panicking() {
    let nodes = vec![
        tract_node((5, Some(1)), (0, None)),        // reversed axis
        tract_node((0, Some(u64::MAX)), (0, None)), // span overflows 32 bits
        tract_node((0, None), (0, Some(u64::MAX))),
        tract_node((u64::MAX, None), (0, None)), // origin past 32 bits
        cat(&[f_varint(1, 36), f_bytes(26, &f_varint(1, u64::MAX))]), // half a reference
        f_varint(1, 999),                        // unknown node
        tract_node((0, None), (2, Some(2))),     // the only usable one: a single cell
    ];
    let (doc, _) = open(&owner_table_document(&nodes)).expect("opens");
    assert!(merges_of(&doc).is_empty());
}

#[test]
fn many_merge_formulas_stay_within_budget() {
    let nodes: Vec<Vec<u8>> = (0..5_000)
        .map(|i| tract_node((i % 3, Some(i % 3 + 1)), (i % 3, None)))
        .collect();
    let start = std::time::Instant::now();
    let (doc, _) = open(&owner_table_document(&nodes)).expect("opens");
    assert!(start.elapsed().as_secs() < 30);
    assert!(merges_of(&doc).len() <= 3);
}

const FFFC: &str = "\u{FFFC}";

fn table_field(field: u32, entries: &[(u64, Option<u64>)]) -> Vec<u8> {
    f_bytes(field, &DocBuilder::attr_table(entries))
}

fn with_tables(text: &str, tables: &[Vec<u8>]) -> Vec<u8> {
    cat(&[
        DocBuilder::storage_payload(text, &[], &[], &[], &[]),
        cat(tables),
    ])
}

#[test]
fn marks_without_a_rich_model_equivalent_are_reported_by_kind_and_count() {
    let mut b = DocBuilder::default();
    let link = b.add(2032, &f_str(2, "https://example.com"));
    let link2 = b.add(2032, &f_str(2, "mailto:a@example.com"));
    let bookmark = b.add(2035, &f_str(1, "uuid"));
    let insertion = b.add(2060, &f_varint(1, 1));
    let deletion = b.add(2060, &f_varint(1, 2));
    let comment = b.add(2033, &f_str(2, "c"));
    let date = b.add(2034, &f_str(1, "d"));
    let unknown = b.add(2099, &[]);
    let text = "링크메일책갈피삽입삭제댓글날짜알수없음";
    let st = b.add(
        2001,
        &with_tables(
            text,
            &[
                // Both layouts: hyperlinks in table 11 (newer files) and 19 (comments, fixtures).
                table_field(
                    11,
                    &[
                        (0, Some(link)),
                        (2, Some(link2)),
                        (4, None),
                        (8, Some(date)),
                    ],
                ),
                table_field(15, &[(4, Some(bookmark)), (6, None)]),
                table_field(21, &[(6, Some(insertion))]),
                table_field(22, &[(8, Some(deletion))]),
                table_field(19, &[(10, Some(comment)), (12, Some(unknown))]),
            ],
        ),
    );
    let (doc, warnings) = open(&body_only(b, st)).expect("opens");
    assert_eq!(paragraph_texts(&doc), vec![text]);
    for expected in [
        "날짜·시각 필드(저장된 글자만 가져옴) 1개",
        "책갈피 1개",
        "변경 추적 삽입 표시(글자는 그대로 가져옴) 1개",
        "변경 추적 삭제 표시(삭제된 글자도 그대로 가져옴) 1개",
        "읽을 수 없는 댓글 1개",
        "글 속 표시(형식 2099) 1개",
    ] {
        assert!(warned(&warnings, expected), "{expected}: {warnings:?}");
    }
}

#[test]
fn newer_footnote_table_and_mark_character_are_numbered_like_the_fixture_layout() {
    let mut b = DocBuilder::default();
    let link = b.add(2032, &f_str(2, "https://example.com"));
    let note_text = b.add(
        2001,
        &DocBuilder::storage_payload("주석", &[], &[], &[], &[]),
    );
    let note = b.add(2008, &f_ref(2, note_text));
    // Table 11 holds a hyperlink here and must not shift the footnote number.
    let text = format!("가\u{E}나{FFFC}");
    let st = b.add(
        2001,
        &with_tables(
            &text,
            &[
                table_field(11, &[(2, Some(link))]),
                table_field(16, &[(1, Some(note))]),
            ],
        ),
    );
    let (doc, warnings) = open(&body_only(b, st)).expect("opens");
    assert_eq!(paragraph_texts(&doc), vec!["가1나", "각주", "1 주석"]);
    let RichBlock::Paragraph(p) = &doc.blocks[0] else {
        panic!("paragraph")
    };
    let linked: Vec<(&str, Option<&str>)> = p
        .runs
        .iter()
        .map(|r| (r.text.as_str(), r.link.as_deref()))
        .collect();
    assert_eq!(
        linked,
        vec![
            ("가", None),
            ("1", None),
            ("나", Some("https://example.com"))
        ]
    );
    assert!(!warned(&warnings, "하이퍼링크"), "{warnings:?}");
}

#[test]
fn multi_column_paragraph_ranges_are_reported_through_the_style_chain() {
    let mut b = DocBuilder::default();
    let equal = |count: u64| f_bytes(11, &f_bytes(7, &f_bytes(1, &f_varint(1, count))));
    let base = b.add(2024, &equal(1));
    let two = b.add(2024, &equal(2));
    // A derived style without its own columns inherits the base's single column.
    let derived = b.add(2024, &f_bytes(1, &f_ref(3, base)));
    let derived_two = b.add(2024, &cat(&[f_bytes(1, &f_ref(3, two))]));
    let st = b.add(
        2001,
        &with_tables(
            "가\n나\n다\n라",
            &[table_field(
                12,
                &[
                    (0, Some(derived)),
                    (2, Some(two)),
                    (4, Some(derived_two)),
                    (6, Some(base)),
                ],
            )],
        ),
    );
    let (_, warnings) = open(&body_only(b, st)).expect("opens");
    assert!(
        warned(
            &warnings,
            "다단 문단 구간(단 나누기 없이 한 단으로 가져옴) 2개"
        ),
        "{warnings:?}"
    );
}

#[test]
fn named_drawables_page_number_fields_empty_shapes_and_cell_objects_are_reported() {
    let mut b = DocBuilder::default();
    let chart = b.add(5021, &[]);
    let group = b.add(3008, &[]);
    let toc = b.add(2240, &[]);
    let movie = b.add(3007, &[]);
    let empty_text = b.add(2001, &DocBuilder::storage_payload("", &[], &[], &[], &[]));
    let shape = b.add(2011, &f_ref(2, empty_text));
    let number = b.add(2043, &[]);
    let mut attach = vec![];
    for d in [chart, group, toc, movie, shape] {
        attach.push(b.attachment(d));
    }
    let text = format!("{FFFC}\n{FFFC}\n{FFFC}\n{FFFC}\n{FFFC}\n{FFFC}");
    let entries: Vec<(u64, Option<u64>)> = attach
        .iter()
        .copied()
        .chain([number])
        .enumerate()
        .map(|(i, a)| (i as u64 * 2, Some(a)))
        .collect();
    let st = b.add(
        2001,
        &DocBuilder::storage_payload(&text, &[], &[], &[], &entries),
    );
    let (_, warnings) = open(&body_only(b, st)).expect("opens");
    for expected in [
        "차트(형식 5021) 1개",
        "개체 그룹(형식 3008) 1개",
        "목차(형식 2240) 1개",
        "동영상·오디오(형식 3007) 1개",
        "글이 없는 도형 1개",
        "쪽 번호·쪽 수 필드 1개",
    ] {
        assert!(warned(&warnings, expected), "{expected}: {warnings:?}");
    }
}

#[test]
fn attachment_marks_inside_table_cells_are_reported() {
    let mut b = DocBuilder::default();
    let table = b.table(&[vec!["가\u{FFFC}", "나"], vec!["\u{FFFC}\u{FFFC}", "다"]]);
    let att = b.attachment(table);
    let st = b.add(
        2001,
        &DocBuilder::storage_payload(FFFC, &[], &[], &[], &[(0, Some(att))]),
    );
    let (doc, warnings) = open(&body_only(b, st)).expect("opens");
    assert!(
        warned(&warnings, "표 셀 안의 개체(그림·필드 등) 3개"),
        "{warnings:?}"
    );
    assert!(matches!(&doc.blocks[0], RichBlock::Table(t) if t.rows[0][0] == "가"));
}

/// Shape or text box (type 2011) with the given text paragraph and a geometry position.
fn positioned_text_box(b: &mut DocBuilder, text: &str, at: Option<(f32, f32)>) -> u64 {
    let st = b.add(2001, &DocBuilder::storage_payload(text, &[], &[], &[], &[]));
    let mut payload = f_ref(2, st);
    if let Some((x, y)) = at {
        let position = cat(&[f_f32(1, x), f_f32(2, y)]);
        let geometry = f_bytes(1, &position);
        // ShapeInfo { 1: ShapeArchive { 1: DrawableArchive { 1: Geometry } } }
        let shape = f_bytes(1, &f_bytes(1, &geometry));
        payload = cat(&[f_bytes(1, &shape), payload]);
    }
    b.add(2011, &payload)
}

fn floating_list(b: &mut DocBuilder, pages: &[(u64, &[u64])]) {
    let groups: Vec<Vec<u8>> = pages
        .iter()
        .map(|(page, ids)| {
            let mut parts = vec![f_varint(1, *page)];
            parts.extend(ids.iter().map(|id| f_bytes(4, &f_ref(1, *id))));
            f_bytes(1, &cat(&parts))
        })
        .collect();
    b.add(10010, &cat(&groups));
}

#[test]
fn page_layout_documents_import_floating_text_in_page_then_reading_order() {
    let mut b = DocBuilder::default();
    // Settings: `body = false` marks a page-layout document.
    b.add(10012, &f_varint(1, 0));
    let late = positioned_text_box(&mut b, "아래 글상자", Some((10.0, 400.0)));
    let right = positioned_text_box(&mut b, "오른쪽 글상자", Some((300.0, 20.0)));
    let left = positioned_text_box(&mut b, "왼쪽 글상자", Some((10.0, 20.0)));
    let second_page = positioned_text_box(&mut b, "둘째 쪽", Some((0.0, 0.0)));
    let unplaced = positioned_text_box(&mut b, "위치 없음", None);
    let chart = b.add(5021, &[]);
    // Page 1 is listed first on purpose; a drawable named twice is read once.
    floating_list(
        &mut b,
        &[
            (1, &[second_page]),
            (0, &[late, right, unplaced, left, chart, left]),
        ],
    );
    let st = b.add(
        2001,
        &DocBuilder::storage_payload("\u{4}", &[], &[], &[], &[]),
    );
    let (doc, warnings) = open(&body_only(b, st)).expect("opens");
    assert_eq!(
        paragraph_texts(&doc),
        vec![
            "왼쪽 글상자",
            "오른쪽 글상자",
            "아래 글상자",
            "위치 없음",
            "둘째 쪽"
        ]
    );
    assert!(warned(&warnings, "쪽 배치 문서입니다"), "{warnings:?}");
    assert!(
        warned(&warnings, "쪽 위의 개체: 차트(형식 5021) 1개"),
        "{warnings:?}"
    );
}

#[test]
fn floating_pictures_and_tables_are_imported_and_hostile_lists_are_bounded() {
    let mut b = DocBuilder::default();
    let table = b.table(&[vec!["가", "나"]]);
    // A self-referencing and a huge-page-index group must not hang or panic.
    let mut ids = vec![table, 1, u64::MAX];
    ids.extend((0..2_000).map(|i| 1_000_000 + i));
    floating_list(&mut b, &[(u64::MAX, &ids)]);
    let body = b.add(
        2001,
        &DocBuilder::storage_payload("본문", &[], &[], &[], &[]),
    );
    let start = std::time::Instant::now();
    let (doc, warnings) = open(&body_only(b, body)).expect("opens");
    assert!(start.elapsed().as_secs() < 30);
    assert!(doc.blocks.iter().any(|b| matches!(b, RichBlock::Table(_))));
    assert!(
        warned(&warnings, "쪽 위의 끊어진 개체 참조 2001개"),
        "{warnings:?}"
    );
}

fn link_runs(doc: &easyoffice_formats::RichDocument) -> Vec<(String, Option<String>)> {
    let RichBlock::Paragraph(p) = &doc.blocks[0] else {
        panic!("paragraph")
    };
    p.runs
        .iter()
        .map(|r| (r.text.clone(), r.link.clone()))
        .collect()
}

#[test]
fn hyperlink_runs_carry_their_address_and_end_at_the_next_entry() {
    let mut b = DocBuilder::default();
    let web = b.add(2032, &f_str(2, "https://example.com/a"));
    let mail = b.add(2032, &f_str(2, "mailto:a@example.com"));
    let script = b.add(2032, &f_str(2, "javascript:alert(1)"));
    let file = b.add(2032, &f_str(2, "file:///etc/passwd"));
    let long = b.add(2032, &f_str(2, &format!("http://{}", "a".repeat(5_000))));
    let st = b.add(
        2001,
        &with_tables(
            "일이삼사오육칠팔구십",
            &[table_field(
                11,
                &[
                    (1, Some(web)),
                    (3, None),
                    (4, Some(mail)),
                    (5, Some(script)),
                    (6, Some(file)),
                    (7, Some(long)),
                    (8, None),
                ],
            )],
        ),
    );
    let (doc, warnings) = open(&body_only(b, st)).expect("opens");
    assert_eq!(
        link_runs(&doc),
        vec![
            ("일".to_string(), None),
            (
                "이삼".to_string(),
                Some("https://example.com/a".to_string())
            ),
            ("사".to_string(), None),
            ("오".to_string(), Some("mailto:a@example.com".to_string())),
            ("육칠팔구십".to_string(), None),
        ]
    );
    assert!(
        warned(
            &warnings,
            "허용되지 않는 형식의 하이퍼링크 주소(글자만 가져옴) 3개"
        ),
        "{warnings:?}"
    );
}

#[test]
fn many_hyperlinks_in_one_paragraph_stay_linear() {
    let mut b = DocBuilder::default();
    let link = b.add(2032, &f_str(2, "https://example.com"));
    let text = "가".repeat(20_000);
    let entries: Vec<(u64, Option<u64>)> = (0..10_000u64)
        .flat_map(|i| [(i * 2, Some(link)), (i * 2 + 1, None)])
        .collect();
    let st = b.add(2001, &with_tables(&text, &[table_field(11, &entries)]));
    let start = std::time::Instant::now();
    let (doc, _) = open(&body_only(b, st)).expect("opens");
    assert!(start.elapsed().as_secs() < 30);
    assert_eq!(link_runs(&doc).len(), 20_000);
}

#[test]
fn comments_are_listed_after_the_body_and_unreadable_ones_counted() {
    let mut b = DocBuilder::default();
    let text_a = b.add(3056, &f_str(1, "첫째 댓글"));
    let text_b = b.add(3059, &f_str(1, "둘째 댓글"));
    let anchor_a = b.add(2013, &f_ref(1, text_a));
    let anchor_b = b.add(2033, &f_ref(1, text_b));
    let anchor_empty = b.add(2013, &f_ref(1, 999_999));
    let st = b.add(
        2001,
        &with_tables(
            "본문 글",
            &[
                table_field(23, &[(3, Some(anchor_b)), (4, None)]),
                table_field(19, &[(0, Some(anchor_a)), (1, Some(anchor_empty))]),
            ],
        ),
    );
    let (doc, warnings) = open(&body_only(b, st)).expect("opens");
    assert_eq!(
        paragraph_texts(&doc),
        vec!["본문 글", "댓글", "첫째 댓글", "둘째 댓글"]
    );
    assert!(warned(&warnings, "읽을 수 없는 댓글 1개"), "{warnings:?}");
}

fn settings_object(b: &mut DocBuilder, kind: u64, format: u64, numbering: u64) {
    b.add(
        10012,
        &cat(&[
            f_varint(1, 1),
            f_varint(30, kind),
            f_varint(31, format),
            f_varint(32, numbering),
        ]),
    );
}

#[test]
fn endnotes_use_their_own_heading_and_note_settings_set_the_numbers() {
    let mut b = DocBuilder::default();
    settings_object(&mut b, 1, 1, 2); // endnotes, roman, restart each section
    let notes: Vec<u64> = ["가", "나", "다"]
        .iter()
        .map(|t| {
            let st = b.add(2001, &DocBuilder::storage_payload(t, &[], &[], &[], &[]));
            b.add(2012, &f_ref(2, st))
        })
        .collect();
    let section = b.add(10011, &[]);
    // Marks at 1 and 3, a section starting at 4, a mark at 6.
    let text = format!("일{FFFC}이{FFFC}\u{4}삼{FFFC}");
    let st = b.add(
        2001,
        &with_tables(
            &text,
            &[
                table_field(
                    11,
                    &[
                        (1, Some(notes[0])),
                        (3, Some(notes[1])),
                        (6, Some(notes[2])),
                    ],
                ),
                table_field(17, &[(0, Some(section)), (5, Some(section))]),
            ],
        ),
    );
    let (doc, _) = open(&body_only(b, st)).expect("opens");
    let texts = paragraph_texts(&doc);
    assert_eq!(texts[0], "일i이ii");
    assert_eq!(texts[1], "삼i");
    // Heading and numbers in the appendix agree with the marks.
    assert_eq!(&texts[2..], ["미주", "i 가", "ii 나", "i 다"]);
}

#[test]
fn per_page_footnote_restart_is_reported_because_pages_are_not_laid_out() {
    let mut b = DocBuilder::default();
    settings_object(&mut b, 0, 2, 1); // footnotes, symbolic, restart each page
    let st_note = b.add(2001, &DocBuilder::storage_payload("주", &[], &[], &[], &[]));
    let note = b.add(2012, &f_ref(2, st_note));
    let st = b.add(
        2001,
        &with_tables(&format!("가{FFFC}"), &[table_field(11, &[(1, Some(note))])]),
    );
    let (doc, warnings) = open(&body_only(b, st)).expect("opens");
    assert_eq!(paragraph_texts(&doc), vec!["가*", "각주", "* 주"]);
    assert!(
        warned(
            &warnings,
            "쪽마다 다시 시작하는 각주 번호(연속 번호로 표시) 1개"
        ),
        "{warnings:?}"
    );
}

#[test]
fn list_levels_and_number_formats_come_from_the_list_style() {
    use easyoffice_formats::{RichNumberEnclosure, RichNumberFormat};
    let mut b = DocBuilder::default();
    // Ordered list style whose levels are `1.`, `(i)` (7: lower roman in parentheses), `A)` (14),
    // with one unknown value at level 3.
    let levels = [3u64, 3, 3, 3];
    let formats = [0u64, 7, 14, 200];
    let style = b.add(
        2023,
        &cat(&levels
            .iter()
            .zip(formats)
            .flat_map(|(t, f)| [f_varint(11, *t), f_varint(15, f)])
            .collect::<Vec<_>>()),
    );
    let text = "가\n나\n다\n라";
    let st = b.add(
        2001,
        &cat(&[
            DocBuilder::storage_payload(text, &[], &[], &[(0, Some(style))], &[]),
            f_bytes(
                6,
                &cat(&[0u64, 2, 4, 6]
                    .iter()
                    .zip([0u64, 1, 2, 3])
                    .map(|(i, l)| f_bytes(1, &cat(&[f_varint(1, *i), f_varint(2, l)])))
                    .collect::<Vec<_>>()),
            ),
        ]),
    );
    let (doc, warnings) = open(&body_only(b, st)).expect("opens");
    let paragraphs: Vec<_> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some((p.list_level, p.list_number)),
            _ => None,
        })
        .collect();
    assert_eq!(
        paragraphs.iter().map(|p| p.0).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    assert_eq!(paragraphs[0].1, None);
    let roman = paragraphs[1].1.expect("roman");
    assert_eq!(
        (roman.format, roman.enclosure),
        (
            RichNumberFormat::LowerRoman,
            RichNumberEnclosure::Parentheses
        )
    );
    assert_eq!(
        paragraphs[2].1.expect("latin").format,
        RichNumberFormat::UpperLatin
    );
    assert_eq!(paragraphs[3].1, None);
    assert!(
        warned(
            &warnings,
            "알 수 없는 목록 번호 형식(1. 형식으로 가져옴) 1개"
        ),
        "{warnings:?}"
    );
}

#[test]
fn independent_character_and_paragraph_layouts_are_read_next_to_the_fixture_layout() {
    let mut b = DocBuilder::default();
    let color =
        |r: f32, g: f32, bl: f32| cat(&[f_f32(3, r), f_f32(4, g), f_f32(5, bl), f_f32(6, 1.0)]);
    // Full layout: font name 5, text background 26, drawn color in the fill 46.1 (it wins over 7).
    let style = b.add(
        2021,
        &f_bytes(
            11,
            &cat(&[
                f_str(5, "Helvetica"),
                f_bytes(7, &color(0.0, 0.0, 1.0)),
                f_bytes(26, &color(1.0, 1.0, 0.0)),
                f_bytes(46, &f_bytes(1, &color(1.0, 0.0, 0.0))),
            ]),
        ),
    );
    // Paragraph properties: first-line 7, left indent 11, line spacing 13 `{ 2: 1.5 }`.
    let para = b.add(
        2022,
        &f_bytes(
            12,
            &cat(&[f_f32(7, 18.0), f_f32(11, 36.0), f_bytes(13, &f_f32(2, 1.5))]),
        ),
    );
    let st = b.add(
        2001,
        &DocBuilder::storage_payload("가나", &[(0, Some(para))], &[(0, Some(style))], &[], &[]),
    );
    let (doc, _) = open(&body_only(b, st)).expect("opens");
    let RichBlock::Paragraph(p) = &doc.blocks[0] else {
        panic!("paragraph")
    };
    let run = &p.runs[0];
    assert_eq!(run.font.as_deref(), Some("Helvetica"));
    assert_eq!(run.color.as_deref(), Some("FF0000"));
    assert_eq!(run.highlight.as_deref(), Some("FFFF00"));
    assert_eq!(
        (
            p.first_line_indent_twips,
            p.left_indent_twips,
            p.line_spacing_percent
        ),
        (Some(360), Some(720), Some(150))
    );
}
