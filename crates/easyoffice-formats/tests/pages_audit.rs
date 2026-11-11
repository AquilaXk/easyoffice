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
        "하이퍼링크의 주소(글자만 가져옴) 2개",
        "날짜·시각 필드(저장된 글자만 가져옴) 1개",
        "책갈피 1개",
        "변경 추적 삽입 표시(글자는 그대로 가져옴) 1개",
        "변경 추적 삭제 표시(삭제된 글자도 그대로 가져옴) 1개",
        "댓글 1개",
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
    assert!(warned(&warnings, "하이퍼링크"), "{warnings:?}");
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
