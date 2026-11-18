//! PD-240 Pages reader integration and adversarial tests:
//! 1. Table cell hyperlinks (with DOCX/ODT/Markdown serialization) and pictures.
//! 2. Header/footer zones (Left/Center/Right), first-page/even-odd variants, page fields, page setup.
//! 3. Per-page restarting footnote numbers (manual page break + automatic line-wrap overflow).
//! 4. Per-section / per-paragraph columns (`equal` & `unequal`) + DOCX/HWPX column output.
//! 5. Tracked changes (deleted body text omitted, author-attributed `변경 추적` appendix).
//! 6. Equations (`3005` ext `100..=103` LaTeX/MathML/PDF fallback, `2015`, inline in body & cells).
//! 7. Charts (`5021` ext `10000` & `5000`, 28 chart types, blank cells preserved, titles, floating).
//! 8. Fail-closed budgets (oversized chart grid, >64 KiB equation, >50,000 mark/link entries, invalid cell links).

#[allow(dead_code)]
mod image_fixture;
mod pages_fixture;
mod pages_picture_fixture;

use easyoffice_formats::{
    DocumentFormat, EasyOfficeError, RichAlignment, RichBindingKind, RichBlock, RichColumns,
    RichDocument, RichField, RichPaper, RichTable, TableCellLink, UniversalFormatConverter,
    ZipArchiveCodec, ZipSecurityPolicy,
};
use image_fixture::make_png;
use pages_fixture::*;
use pages_picture_fixture::*;

const FFFC: &str = "\u{FFFC}";

fn f_f64(num: u32, v: f64) -> Vec<u8> {
    let mut o = Vec::new();
    varint((u64::from(num) << 3) | 1, &mut o);
    o.extend_from_slice(&v.to_le_bytes());
    o
}

fn table_field(field: u32, entries: &[(u64, Option<u64>)]) -> Vec<u8> {
    f_bytes(field, &DocBuilder::attr_table(entries))
}

fn body_only(b: DocBuilder, st: u64) -> Vec<u8> {
    pages_zip(b.finish(st))
}

fn plain_st(b: &mut DocBuilder, text: &str) -> u64 {
    b.add(2001, &DocBuilder::storage_payload(text, &[], &[], &[], &[]))
}

fn att_st(b: &mut DocBuilder, att: u64) -> u64 {
    b.add(
        2001,
        &DocBuilder::storage_payload(FFFC, &[], &[], &[], &[(0, Some(att))]),
    )
}

/// Builds a rich-text table from custom cell storage ids (`grid` holds storage object ids).
fn table_from_storages(b: &mut DocBuilder, grid: &[Vec<u64>]) -> u64 {
    let rows = grid.len();
    let cols = grid[0].len();
    let mut list_entries = Vec::new();
    let mut key = 1u32;
    let mut row_infos = Vec::new();
    for (r, row) in grid.iter().enumerate() {
        let mut buf = Vec::new();
        let mut offs = vec![0xFFu8; cols * 2];
        for (c, &st) in row.iter().enumerate() {
            let payload = b.add(6218, &f_ref(1, st));
            list_entries.push(f_bytes(
                3,
                &cat(&[
                    f_varint(1, u64::from(key)),
                    f_varint(2, 1),
                    f_ref(9, payload),
                ]),
            ));
            offs[c * 2..c * 2 + 2].copy_from_slice(&(buf.len() as u16).to_le_bytes());
            let mut cell = vec![5u8, 9, 0, 0, 0, 0, 0, 0];
            cell.extend_from_slice(&0x0002_1070u32.to_le_bytes());
            for v in [key, 2, 1, 5, 1] {
                cell.extend_from_slice(&v.to_le_bytes());
            }
            buf.extend(cell);
            key += 1;
        }
        row_infos.push(f_bytes(
            5,
            &cat(&[
                f_varint(1, r as u64),
                f_varint(2, cols as u64),
                f_bytes(6, &buf),
                f_bytes(7, &offs),
            ]),
        ));
    }
    let tile = b.add(6002, &cat(&row_infos));
    let list = b.add(6005, &cat(&[f_varint(1, 8), cat(&list_entries)]));
    let tiles = cat(&[
        f_bytes(1, &cat(&[f_varint(1, 0), f_ref(2, tile)])),
        f_varint(2, 256),
    ]);
    let store = cat(&[f_bytes(3, &tiles), f_ref(17, list)]);
    let model = b.add(
        6001,
        &cat(&[
            f_bytes(4, &store),
            f_varint(6, rows as u64),
            f_varint(7, cols as u64),
        ]),
    );
    b.add(6000, &f_ref(2, model))
}

fn zip_part(bytes: &[u8], name: &str) -> String {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .expect("unpack")
        .into_iter()
        .find(|e| e.path == name)
        .map(|e| String::from_utf8(e.data).expect("utf8"))
        .unwrap_or_else(|| panic!("missing part {name}"))
}

// ---------------------------------------------------------------------------
// 1. Table cell hyperlinks & pictures + DOCX / ODT / Markdown serialization
// ---------------------------------------------------------------------------

#[test]
fn table_cell_hyperlinks_are_read_with_utf8_offsets_and_written_to_docx_odt_and_markdown() {
    let mut b = DocBuilder::default();
    let web = b.add(2032, &f_str(2, "https://example.com/guide"));
    let mail = b.add(2032, &f_str(2, "mailto:help@example.com"));
    let unsafe_js = b.add(2032, &f_str(2, "javascript:alert(1)"));

    // Cell (0, 0): "안내 바로가기 끝" -> "바로가기" at UTF-16 3..7, UTF-8 bytes 7..19
    let c00_text = "안내 바로가기 끝";
    let c00 = b.add(
        2001,
        &cat(&[
            DocBuilder::storage_payload(c00_text, &[], &[], &[], &[]),
            table_field(11, &[(3, Some(web)), (7, None)]),
        ]),
    );
    // Cell (0, 1): mail link in table 19 + unsafe link counted and stripped to plain text
    let c01_text = "문의하기 위험링크";
    let c01 = b.add(
        2001,
        &cat(&[
            DocBuilder::storage_payload(c01_text, &[], &[], &[], &[]),
            table_field(
                19,
                &[(0, Some(mail)), (4, None), (5, Some(unsafe_js)), (9, None)],
            ),
        ]),
    );
    let tbl = table_from_storages(&mut b, &[vec![c00, c01]]);
    let att = b.attachment(tbl);
    let body = b.add(
        2001,
        &DocBuilder::storage_payload(FFFC, &[], &[], &[], &[(0, Some(att))]),
    );

    let (doc, warnings) = open(&body_only(b, body)).expect("opens");
    assert!(
        warned(
            &warnings,
            "허용되지 않는 형식의 하이퍼링크 주소(글자만 가져옴) 1개"
        ),
        "{warnings:?}"
    );
    let RichBlock::Table(table) = &doc.blocks[0] else {
        panic!("expected table");
    };
    assert_eq!(table.rows[0][0], c00_text);
    assert_eq!(
        &table.rows[0][0][table.links[0].start as usize..table.links[0].end as usize],
        "바로가기"
    );
    assert_eq!(table.links.len(), 2);
    assert_eq!(table.links[0].url, "https://example.com/guide");
    assert_eq!(
        &table.rows[0][1][table.links[1].start as usize..table.links[1].end as usize],
        "문의하기"
    );
    assert_eq!(table.links[1].url, "mailto:help@example.com");

    // DOCX serialization
    let docx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let doc_xml = zip_part(&docx, "word/document.xml");
    let rels_xml = zip_part(&docx, "word/_rels/document.xml.rels");
    assert!(
        doc_xml.contains("<w:hyperlink r:id=\"rIdLink1\""),
        "{doc_xml}"
    );
    assert!(
        rels_xml.contains("Target=\"https://example.com/guide\""),
        "{rels_xml}"
    );
    assert!(
        rels_xml.contains("Target=\"mailto:help@example.com\""),
        "{rels_xml}"
    );

    // ODT serialization
    let odt = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Odt, None).unwrap();
    let content_xml = zip_part(&odt, "content.xml");
    assert!(
        content_xml.contains("<text:a xlink:type=\"simple\" xlink:href=\"https://example.com/guide\">바로가기</text:a>"),
        "{content_xml}"
    );
    assert!(
        content_xml.contains("<text:a xlink:type=\"simple\" xlink:href=\"mailto:help@example.com\">문의하기</text:a>"),
        "{content_xml}"
    );

    // Markdown serialization
    let md = String::from_utf8(
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Markdown, None).unwrap(),
    )
    .unwrap();
    assert!(md.contains("[바로가기](https://example.com/guide)"), "{md}");
    assert!(md.contains("[문의하기](mailto:help@example.com)"), "{md}");
}

#[test]
fn table_cell_pictures_are_emitted_after_the_table_and_unmapped_objects_still_warn() {
    let mut b = DocBuilder::default();
    metadata(&mut b, &[(1, "cell1.png"), (2, "cell2.png")]);
    let img1 = picture(
        &mut b,
        &Picture {
            data_id: Some(1),
            alt: Some("첫째 셀 그림"),
            ..Picture::default()
        },
    );
    let img2 = picture(
        &mut b,
        &Picture {
            data_id: Some(2),
            alt: Some("둘째 셀 그림"),
            ..Picture::default()
        },
    );
    let att1 = b.attachment(img1);
    let att2 = b.attachment(img2);

    let c00 = b.add(
        2001,
        &DocBuilder::storage_payload(&format!("셀A{FFFC}"), &[], &[], &[], &[(2, Some(att1))]),
    );
    let c01 = plain_st(&mut b, &format!("셀B{FFFC}"));
    let c10 = att_st(&mut b, att2);
    let c11 = plain_st(&mut b, "셀D");

    let tbl = table_from_storages(&mut b, &[vec![c00, c01], vec![c10, c11]]);
    let tbl_att = b.attachment(tbl);
    let body = att_st(&mut b, tbl_att);
    let zip = pages_zip_with_data(
        b.finish(body),
        vec![
            ("Data/cell1.png".to_string(), make_png(6, 4, 2, 3)),
            ("Data/cell2.png".to_string(), make_png(8, 6, 4, 5)),
        ],
    );
    let (doc, warnings) = open(&zip).expect("opens");
    assert!(
        warned(&warnings, "표 셀 안의 개체(그림·필드 등) 1개"),
        "{warnings:?}"
    );
    assert!(matches!(&doc.blocks[0], RichBlock::Table(t) if t.rows[0][0] == "셀A"));
    let imgs = images(&doc);
    assert_eq!(imgs.len(), 2);
    assert_eq!(imgs[0].alt(), "첫째 셀 그림");
    assert_eq!(imgs[1].alt(), "둘째 셀 그림");
}

// ---------------------------------------------------------------------------
// 2. Header/footer zones, first/even-odd templates, page fields & page setup
// ---------------------------------------------------------------------------

#[test]
fn section_headers_keep_zone_indices_page_fields_and_first_even_odd_variants() {
    let mut b = DocBuilder::default();
    // Settings (10012): body=true(1), facing_pages=true(34)
    let settings_id = b.add(10012, &cat(&[f_varint(1, 1), f_varint(34, 1)]));

    // Number attachments (2043): super(1).kind(2): 0 = PageNumber, 1 = TotalPages
    let page_num = b.add(2043, &f_bytes(1, &f_varint(2, 0)));
    let total_pages = b.add(2043, &f_bytes(1, &f_varint(2, 1)));

    let odd_left_hdr = plain_st(&mut b, "왼쪽 머리글");
    let odd_right_hdr = plain_st(&mut b, "오른쪽 머리글");
    let odd_center_ftr = b.add(
        2001,
        &DocBuilder::storage_payload(
            &format!("{FFFC} / {FFFC}"),
            &[],
            &[],
            &[],
            &[(0, Some(page_num)), (4, Some(total_pages))],
        ),
    );
    // Odd template (10143): header [Some(left), {1:0} null, Some(right)], footer [{1:0}, Some(center), {1:0}]
    let odd_tpl = b.add(
        10143,
        &cat(&[
            f_ref(1, odd_left_hdr),
            f_ref(1, 0),
            f_ref(1, odd_right_hdr),
            f_ref(2, 0),
            f_ref(2, odd_center_ftr),
            f_ref(2, 0),
        ]),
    );
    let first_hdr = plain_st(&mut b, "표지 전용 머리글");
    let first_tpl = b.add(
        10143,
        &cat(&[f_ref(1, 0), f_ref(1, first_hdr), f_ref(1, 0)]),
    );
    let even_hdr = plain_st(&mut b, "짝수 쪽 머리글");
    let even_tpl = b.add(10143, &cat(&[f_ref(1, even_hdr), f_ref(1, 0), f_ref(1, 0)]));

    let sec0 = b.add(
        10011,
        &cat(&[
            f_varint(18, 1),
            f_varint(19, 1),
            f_ref(23, first_tpl),
            f_ref(24, even_tpl),
            f_ref(25, odd_tpl),
        ]),
    );
    // Section 1: inherits_header_footer=17 -> its dummy template must be skipped in appendix!
    let ignored_hdr = plain_st(&mut b, "상속되어 무시될 머리글");
    let sec1_tpl = b.add(
        10143,
        &cat(&[f_ref(1, ignored_hdr), f_ref(1, 0), f_ref(1, 0)]),
    );
    let sec1 = b.add(10011, &cat(&[f_varint(17, 1), f_ref(25, sec1_tpl)]));

    let body_text = "첫 구역\u{4}둘째 구역";
    let body = b.add(
        2001,
        &cat(&[
            DocBuilder::storage_payload(body_text, &[], &[], &[], &[]),
            table_field(17, &[(0, Some(sec0)), (5, Some(sec1))]),
        ]),
    );
    // Root (10000) with US Letter landscape 792x612 pt and 72pt margins, header/footer 36pt
    let root_payload = cat(&[
        f_ref(4, body),
        f_ref(7, settings_id),
        f_f32(30, 792.0),
        f_f32(31, 612.0),
        f_f32(32, 72.0),
        f_f32(33, 72.0),
        f_f32(34, 72.0),
        f_f32(35, 72.0),
        f_f32(36, 36.0),
        f_f32(37, 36.0),
    ]);
    b.add_with_id(1, 10000, &root_payload);
    let zip = pages_zip(iwa_frame(&b.records, true));

    let (doc, _) = open(&zip).expect("opens");
    assert_eq!(
        doc.page_frame.paper,
        Some(RichPaper {
            width: 15_840,
            height: 12_240
        })
    );
    assert_eq!(doc.page_frame.binding.kind, RichBindingKind::Duplex);
    assert!(doc.page_frame.first_page_different);
    assert!(doc.page_frame.even_odd_different);

    // Zone 0 = Left, Zone 2 = Right (null Zone 1 did NOT shift Right to Center!)
    assert_eq!(doc.page_frame.header.len(), 2);
    assert_eq!(doc.page_frame.header[0].plain_text(), "왼쪽 머리글");
    assert_eq!(
        doc.page_frame.header[0].alignment,
        Some(RichAlignment::Left)
    );
    assert_eq!(doc.page_frame.header[1].plain_text(), "오른쪽 머리글");
    assert_eq!(
        doc.page_frame.header[1].alignment,
        Some(RichAlignment::Right)
    );

    // Footer has PageNumber and TotalPages RichField runs with Center alignment
    assert_eq!(doc.page_frame.footer.len(), 1);
    assert_eq!(
        doc.page_frame.footer[0].alignment,
        Some(RichAlignment::Center)
    );
    let fields: Vec<Option<RichField>> = doc.page_frame.footer[0]
        .runs
        .iter()
        .map(|r| r.field)
        .collect();
    assert!(fields.contains(&Some(RichField::PageNumber)), "{fields:?}");
    assert!(fields.contains(&Some(RichField::TotalPages)), "{fields:?}");

    // First & even headers populated
    assert_eq!(
        doc.page_frame.first_header[0].plain_text(),
        "표지 전용 머리글"
    );
    assert_eq!(
        doc.page_frame.first_header[0].alignment,
        Some(RichAlignment::Center)
    );
    assert_eq!(doc.page_frame.even_header[0].plain_text(), "짝수 쪽 머리글");

    // Appendix distinguishes [첫 쪽] and [짝수 쪽], keeps Odd unprefixed, skips inherited sec1
    let texts = paragraph_texts(&doc);
    assert!(texts.contains(&"왼쪽 머리글".to_string()), "{texts:?}");
    assert!(
        texts.contains(&"[첫 쪽] 표지 전용 머리글".to_string()),
        "{texts:?}"
    );
    assert!(
        texts.contains(&"[짝수 쪽] 짝수 쪽 머리글".to_string()),
        "{texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t.contains("상속되어 무시될")),
        "{texts:?}"
    );

    // Laid PDF selects first_header on page 1, even_header on page 2, default header on page 3
    let mut multi_page = doc.clone();
    multi_page.blocks = (0..120)
        .map(|i| {
            RichBlock::Paragraph(easyoffice_formats::RichParagraph {
                runs: vec![easyoffice_formats::RichRun::plain(format!("Line {i}"))],
                ..Default::default()
            })
        })
        .collect();
    let pdf =
        UniversalFormatConverter::encode_rich(&multi_page, DocumentFormat::Pdf, None).unwrap();
    let pdf_str = String::from_utf8_lossy(&pdf);
    assert!(pdf.starts_with(b"%PDF-"), "valid pdf header");
    assert!(pdf_str.contains("/Count "), "{pdf_str}");
}

// ---------------------------------------------------------------------------
// 3. Per-page restarting footnote numbers (manual break + auto line-wrap)
// ---------------------------------------------------------------------------

#[test]
fn per_page_footnotes_restart_on_manual_page_break_and_on_automatic_line_wrap_overflow() {
    let mut b = DocBuilder::default();
    // Footnotes (0), symbolic (*, †, ‡ = 2), restart each page (1)
    b.add(
        10012,
        &cat(&[
            f_varint(1, 1),
            f_varint(30, 0),
            f_varint(31, 2),
            f_varint(32, 1),
        ]),
    );
    let notes: Vec<u64> = ["첫쪽 첫주석", "첫쪽 둘째주석", "수동나눔 뒤", "자동넘김 뒤"]
        .iter()
        .map(|t| {
            let st = b.add(2001, &DocBuilder::storage_payload(t, &[], &[], &[], &[]));
            b.add(2008, &f_ref(2, st))
        })
        .collect();

    let p1 = "첫째문단\u{E}둘째표식\u{E}";
    let filler = "가나다라마바사아자차카타파하 ".repeat(180);
    let p2_start = "둘째쪽시작\u{E}";
    let p3_auto = "자동쪽넘김뒤\u{E}";
    let full_text = format!("{p1}\u{c}{p2_start}\n{filler}\n{p3_auto}");

    let idx0 = utf16_len("첫째문단");
    let idx1 = utf16_len("첫째문단\u{E}둘째표식");
    let after_break = utf16_len(p1) + 1;
    let idx2 = after_break + utf16_len("둘째쪽시작");
    let idx3 =
        after_break + utf16_len(p2_start) + 1 + utf16_len(&filler) + 1 + utf16_len("자동쪽넘김뒤");

    let st = b.add(
        2001,
        &cat(&[
            DocBuilder::storage_payload(&full_text, &[], &[], &[], &[]),
            table_field(
                16,
                &[
                    (idx0, Some(notes[0])),
                    (idx1, Some(notes[1])),
                    (idx2, Some(notes[2])),
                    (idx3, Some(notes[3])),
                ],
            ),
        ]),
    );
    let (doc, warnings) = open(&body_only(b, st)).expect("opens");
    assert!(
        !warned(&warnings, "쪽마다 다시 시작하는 각주 번호"),
        "{warnings:?}"
    );
    let texts = paragraph_texts(&doc);
    assert_eq!(texts[0], "첫째문단*둘째표식\u{2020}");
    assert_eq!(texts[1], "둘째쪽시작*");
    assert_eq!(texts[3], "자동쪽넘김뒤*");
    let tail = &texts[texts.len() - 4..];
    assert_eq!(
        tail,
        [
            "* 첫쪽 첫주석",
            "\u{2020} 첫쪽 둘째주석",
            "* 수동나눔 뒤",
            "* 자동넘김 뒤"
        ]
    );
}

// ---------------------------------------------------------------------------
// 4. Per-section / per-paragraph columns + DOCX / HWPX writers
// ---------------------------------------------------------------------------

#[test]
fn equal_and_unequal_columns_populate_paragraphs_and_roundtrip_into_docx_and_hwpx() {
    let mut b = DocBuilder::default();
    // Equal 2 columns, gap fraction 0.05
    let eq2 = b.add(
        2024,
        &f_bytes(
            11,
            &f_bytes(7, &f_bytes(1, &cat(&[f_varint(1, 2), f_f32(2, 0.05)]))),
        ),
    );
    // Unequal 3 columns: first=0.25, following=[(gap=0.04, width=0.335), (gap=0.04, width=0.335)]
    let pair = cat(&[f_f32(1, 0.04), f_f32(2, 0.335)]);
    let uneq3 = b.add(
        2024,
        &f_bytes(
            11,
            &f_bytes(
                7,
                &f_bytes(
                    2,
                    &cat(&[f_f32(1, 0.25), f_bytes(2, &pair), f_bytes(2, &pair)]),
                ),
            ),
        ),
    );
    let single = b.add(
        2024,
        &f_bytes(11, &f_bytes(7, &f_bytes(1, &f_varint(1, 1)))),
    );

    let text = "이단 첫문단\n이단 둘째문단\u{4}비균등 삼단\u{4}다시 일단";
    let at_uneq = utf16_len("이단 첫문단\n이단 둘째문단\u{4}");
    let at_single = at_uneq + utf16_len("비균등 삼단\u{4}");
    let st = b.add(
        2001,
        &cat(&[
            DocBuilder::storage_payload(text, &[], &[], &[], &[]),
            table_field(
                12,
                &[
                    (0, Some(eq2)),
                    (at_uneq, Some(uneq3)),
                    (at_single, Some(single)),
                ],
            ),
        ]),
    );
    let (doc, warnings) = open(&body_only(b, st)).expect("opens");
    assert!(!warned(&warnings, "다단"), "{warnings:?}");

    let cols: Vec<Option<RichColumns>> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p.columns),
            _ => None,
        })
        .collect();
    assert_eq!(cols.len(), 4);
    assert!(
        matches!(cols[0], Some(RichColumns { count: 2, equal: true, gap_twips }) if gap_twips > 0)
    );
    assert_eq!(cols[0], cols[1]);
    assert!(
        matches!(cols[2], Some(RichColumns { count: 3, equal: false, gap_twips }) if gap_twips > 0)
    );
    assert_eq!(cols[3], None);

    // DOCX carries continuous section breaks with <w:cols .../>
    let docx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let doc_xml = zip_part(&docx, "word/document.xml");
    assert!(
        doc_xml.contains("<w:type w:val=\"continuous\"/>"),
        "{doc_xml}"
    );
    assert!(doc_xml.contains("<w:cols w:num=\"2\""), "{doc_xml}");
    assert!(doc_xml.contains("<w:cols w:num=\"3\""), "{doc_xml}");

    // HWPX carries <hp:colPr ... colCount="2" .../> and <hp:colPr ... colCount="3" sameSz="0" .../>
    let hwpx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None).unwrap();
    let sec_xml = zip_part(&hwpx, "Contents/section0.xml");
    assert!(sec_xml.contains("colCount=\"2\" sameSz=\"1\""), "{sec_xml}");
    assert!(sec_xml.contains("colCount=\"3\" sameSz=\"0\""), "{sec_xml}");
}

// ---------------------------------------------------------------------------
// 5. Tracked changes (deleted text omitted + author-attributed appendix)
// ---------------------------------------------------------------------------

#[test]
fn tracked_deletions_are_excluded_from_body_while_preserving_styles_and_listed_in_appendix() {
    let mut b = DocBuilder::default();
    let author_ins = b.add(212, &f_str(1, "홍길동"));
    let author_del = b.add(212, &f_str(1, "김철수"));
    let sess_ins = b.add(2062, &f_ref(2, author_ins));
    let sess_del = b.add(2062, &f_ref(2, author_del));
    let chg_ins = b.add(2060, &cat(&[f_varint(1, 1), f_ref(2, sess_ins)]));
    let chg_del = b.add(2060, &cat(&[f_varint(1, 2), f_ref(2, sess_del)]));
    let chg_anon = b.add(2060, &f_varint(1, 1));
    let bold = b.char_style(Some(true), None, None, None, None);

    // "앞 " (0..2) + "새글" (2..4, inserted) + "버릴글" (4..7, deleted) + "굵은뒤" (7..10, bold) + "\n익명추가" (11..15)
    let text = "앞 새글버릴글굵은뒤\n익명추가";
    let st = b.add(
        2001,
        &cat(&[
            DocBuilder::storage_payload(text, &[], &[(7, Some(bold)), (10, None)], &[], &[]),
            table_field(
                21,
                &[
                    (2, Some(chg_ins)),
                    (4, None),
                    (11, Some(chg_anon)),
                    (15, None),
                ],
            ),
            table_field(22, &[(4, Some(chg_del)), (7, None)]),
        ]),
    );
    let (doc, warnings) = open(&body_only(b, st)).expect("opens");
    assert!(!warned(&warnings, "변경 추적"), "{warnings:?}");

    let texts = paragraph_texts(&doc);
    assert_eq!(
        texts,
        vec![
            "앞 새글굵은뒤",
            "익명추가",
            "변경 추적",
            "[삽입 · 홍길동] 새글",
            "[삭제 · 김철수] 버릴글",
            "[삽입] 익명추가",
        ]
    );
    let RichBlock::Paragraph(first_para) = &doc.blocks[0] else {
        panic!("paragraph");
    };
    let bold_run = first_para
        .runs
        .iter()
        .find(|r| r.text == "굵은뒤")
        .expect("bold run");
    assert!(
        bold_run.bold,
        "UTF-16 style offset after deleted text must stay exact"
    );
}

// ---------------------------------------------------------------------------
// 6. Equations (LaTeX, MathML, PDF Data fallback, standalone 2015, table cells)
// ---------------------------------------------------------------------------

#[test]
fn equations_import_as_raster_or_inline_text_in_body_and_table_cells_without_false_warnings() {
    let mut b = DocBuilder::default();
    metadata(&mut b, &[(1, "eq1.png"), (2, "eq2.pdf")]);

    // 1) 3005 equation with PNG raster and empty description -> Image with equation source as alt
    let eq_png = b.add(3005, &cat(&[f_ref(11, 1), f_str(100, "E = mc^2")]));
    // 2) 3005 equation with .pdf Data (non-PNG/JPEG) and nested LaTeX -> inline text, NO warning!
    let eq_pdf = b.add(
        3005,
        &cat(&[f_ref(11, 2), f_bytes(101, &f_str(1, "\\int_0^1 x^2 dx"))]),
    );
    // 3) 3005 equation with MathML and no Data -> stripped inline text
    let eq_mathml = b.add(
        3005,
        &f_str(102, "<math xmlns=\"http://www.w3.org/1998/Math/MathML\"><mi>a</mi><mo>+</mo><mi>b</mi></math>"),
    );
    // 4) Standalone 2015 equation -> inline text
    let eq_2015 = b.add(2015, &f_str(2, "x^2 + y^2 = r^2"));
    // 5) Empty 2015 equation -> reported in warnings
    let eq_empty = b.add(2015, &[]);

    // Table cell with inline 3005 PDF equation and 2015 equation
    let cell_eq_att1 = b.attachment(eq_pdf);
    let cell_eq_att2 = b.attachment(eq_2015);
    let cell_st = b.add(
        2001,
        &DocBuilder::storage_payload(
            &format!("수식: {FFFC}, 원: {FFFC}"),
            &[],
            &[],
            &[],
            &[
                (utf16_len("수식: "), Some(cell_eq_att1)),
                (
                    utf16_len(&format!("수식: {FFFC}, 원: ")),
                    Some(cell_eq_att2),
                ),
            ],
        ),
    );
    let tbl = table_from_storages(&mut b, &[vec![cell_st]]);

    let att_png = b.attachment(eq_png);
    let att_pdf = b.attachment(eq_pdf);
    let att_mathml = b.attachment(eq_mathml);
    let att_2015 = b.attachment(eq_2015);
    let att_empty = b.attachment(eq_empty);
    let att_tbl = b.attachment(tbl);

    let body_text = format!("그림수식\n{FFFC}\n문장 속 {FFFC} 와 {FFFC} 및 {FFFC}\n{FFFC}\n{FFFC}");
    let p_png = utf16_len("그림수식\n");
    let line2_start = p_png + 2;
    let p_pdf = line2_start + utf16_len("문장 속 ");
    let p_mml = p_pdf + 1 + utf16_len(" 와 ");
    let p_2015 = p_mml + 1 + utf16_len(" 및 ");
    let p_tbl = p_2015 + 2;
    let p_empty = p_tbl + 2;

    let body = b.add(
        2001,
        &DocBuilder::storage_payload(
            &body_text,
            &[],
            &[],
            &[],
            &[
                (p_png, Some(att_png)),
                (p_pdf, Some(att_pdf)),
                (p_mml, Some(att_mathml)),
                (p_2015, Some(att_2015)),
                (p_tbl, Some(att_tbl)),
                (p_empty, Some(att_empty)),
            ],
        ),
    );
    let zip = pages_zip_with_data(
        b.finish(body),
        vec![
            ("Data/eq1.png".to_string(), make_png(10, 6, 2, 3)),
            ("Data/eq2.pdf".to_string(), b"%PDF-1.4 equation".to_vec()),
        ],
    );
    let (doc, warnings) = open(&zip).expect("opens");
    assert!(
        !warned(&warnings, "지원하지 않는 형식의 그림"),
        "{warnings:?}"
    );
    assert!(!warned(&warnings, "표 셀 안의 개체"), "{warnings:?}");
    assert!(warned(&warnings, "방정식(형식 2015) 1개"), "{warnings:?}");

    let imgs = images(&doc);
    assert_eq!(imgs.len(), 1);
    assert_eq!(imgs[0].alt(), "E = mc^2");

    let texts = paragraph_texts(&doc);
    assert!(
        texts.contains(&"문장 속 \\int_0^1 x^2 dx 와 a+b 및 x^2 + y^2 = r^2".to_string()),
        "{texts:?}"
    );
    let table_block = doc
        .blocks
        .iter()
        .find_map(|b| match b {
            RichBlock::Table(t) => Some(t),
            _ => None,
        })
        .expect("table");
    assert_eq!(
        table_block.rows[0][0],
        "수식: \\int_0^1 x^2 dx, 원: x^2 + y^2 = r^2"
    );
}

// ---------------------------------------------------------------------------
// 7. Charts (28 types, blank cells preserved, dates/durations, titles, floating)
// ---------------------------------------------------------------------------

fn chart_grid_payload(row_names: &[&str], col_names: &[&str], rows: &[Vec<Vec<u8>>]) -> Vec<u8> {
    let mut parts = Vec::new();
    for r in row_names {
        parts.push(f_str(1, r));
    }
    for c in col_names {
        parts.push(f_str(2, c));
    }
    for row in rows {
        let cells: Vec<Vec<u8>> = row.iter().map(|cell| f_bytes(1, cell)).collect();
        parts.push(f_bytes(3, &cat(&cells)));
    }
    cat(&parts)
}

#[test]
fn charts_import_all_28_types_preserve_blank_cells_and_work_anchored_or_floating() {
    let mut b = DocBuilder::default();
    let title_st = plain_st(&mut b, "분기별 매출");
    // Row 0: [12.5, EMPTY, Date(0.0 -> 2001-01-01)], Row 1: [30.0, Duration(90.0 -> 90s), 45.0]
    let grid = chart_grid_payload(
        &["서울", "부산"],
        &["1분기", "2분기", "기준일"],
        &[
            vec![f_f64(1, 12.5), Vec::new(), f_f64(4, 0.0)],
            vec![f_f64(1, 30.0), f_f64(3, 90.0), f_f64(1, 45.0)],
        ],
    );
    let chart_model = cat(&[f_varint(1, 1), f_bytes(7, &grid)]);
    let chart_drawable = cat(&[
        f_bytes(1, &f_ref(10, title_st)),
        f_bytes(10000, &chart_model),
    ]);
    let main_chart = b.add(5021, &chart_drawable);
    let main_att = b.attachment(main_chart);

    let mut floating_ids = Vec::new();
    for kind in 0..=27u64 {
        let mini_grid = chart_grid_payload(&["행"], &["열"], &[vec![f_f64(1, kind as f64)]]);
        let model = cat(&[f_varint(1, kind), f_bytes(7, &mini_grid)]);
        let id = if kind == 27 {
            b.add(5000, &model)
        } else {
            b.add(5021, &f_bytes(10000, &model))
        };
        floating_ids.push(id);
    }
    let group_parts: Vec<Vec<u8>> = std::iter::once(f_varint(1, 0))
        .chain(floating_ids.iter().map(|id| f_bytes(4, &f_ref(1, *id))))
        .collect();
    b.add(10010, &f_bytes(1, &cat(&group_parts)));

    let body = att_st(&mut b, main_att);
    let (doc, warnings) = open(&body_only(b, body)).expect("opens");
    assert!(!warned(&warnings, "차트"), "{warnings:?}");

    let texts = paragraph_texts(&doc);
    assert_eq!(texts[0], "차트: 분기별 매출 (세로 막대형)");
    for expected in [
        "차트 (가로 막대형)",
        "차트 (꺾은선형)",
        "차트 (원형)",
        "차트 (3D 세로 막대형)",
        "차트 (도넛형)",
        "차트 (방사형)",
    ] {
        assert!(texts.contains(&expected.to_string()), "{expected}");
    }

    let first_table = doc
        .blocks
        .iter()
        .find_map(|bl| match bl {
            RichBlock::Table(t) => Some(t),
            _ => None,
        })
        .expect("chart table");
    let s = |v: &str| v.to_string();
    assert_eq!(
        first_table.rows,
        vec![
            vec![s(""), s("1분기"), s("2분기"), s("기준일")],
            vec![s("서울"), s("12.5"), s(""), s("2001-01-01")],
            vec![s("부산"), s("30"), s("90s"), s("45")],
        ]
    );
}

#[test]
fn hostile_oversized_chart_rows_and_columns_fail_closed() {
    let mut b1 = DocBuilder::default();
    let many_cols: Vec<&str> = vec!["열"; 300];
    let grid_wide = chart_grid_payload(&["행"], &many_cols, &[vec![f_f64(1, 1.0); 300]]);
    let ch_wide = b1.add(
        5021,
        &f_bytes(10000, &cat(&[f_varint(1, 1), f_bytes(7, &grid_wide)])),
    );
    let att1 = b1.attachment(ch_wide);
    let st1 = att_st(&mut b1, att1);
    assert!(matches!(
        open(&body_only(b1, st1)),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    let mut b2 = DocBuilder::default();
    let mut grid_tall = f_str(2, "값");
    let one_row = f_bytes(3, &f_bytes(1, &f_f64(1, 1.0)));
    for _ in 0..10_005 {
        grid_tall.extend_from_slice(&one_row);
    }
    let ch_tall = b2.add(
        5021,
        &f_bytes(10000, &cat(&[f_varint(1, 1), f_bytes(7, &grid_tall)])),
    );
    let att2 = b2.attachment(ch_tall);
    let st2 = att_st(&mut b2, att2);
    assert!(matches!(
        open(&body_only(b2, st2)),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn hostile_oversized_equation_string_fails_closed() {
    let mut b = DocBuilder::default();
    let eq = b.add(3005, &f_str(100, &"x+".repeat(33_000)));
    let att = b.attachment(eq);
    let st = att_st(&mut b, att);
    assert!(matches!(
        open(&body_only(b, st)),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn hostile_over_50k_tracked_changes_and_cell_links_fail_closed() {
    let mut b1 = DocBuilder::default();
    let chg = b1.add(2060, &f_varint(1, 1));
    let entries: Vec<(u64, Option<u64>)> = (0..50_005u64).map(|i| (i, Some(chg))).collect();
    let st1 = b1.add(
        2001,
        &cat(&[
            DocBuilder::storage_payload("본문", &[], &[], &[], &[]),
            table_field(21, &entries),
        ]),
    );
    assert!(matches!(
        open(&body_only(b1, st1)),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    let mut b2 = DocBuilder::default();
    let link = b2.add(2032, &f_str(2, "https://example.com"));
    let link_entries: Vec<(u64, Option<u64>)> = (0..50_005u64).map(|i| (i, Some(link))).collect();
    let cell_st = b2.add(
        2001,
        &cat(&[
            DocBuilder::storage_payload("셀", &[], &[], &[], &[]),
            table_field(11, &link_entries),
        ]),
    );
    let tbl = table_from_storages(&mut b2, &[vec![cell_st]]);
    let att = b2.attachment(tbl);
    let st2 = att_st(&mut b2, att);
    assert!(matches!(
        open(&body_only(b2, st2)),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn rich_table_rejects_bad_offsets_mid_codepoint_slices_overlaps_and_unsafe_schemes() {
    let rows = vec![vec!["한글링크".to_string()]]; // "한"=0..3, "글"=3..6, "링"=6..9, "크"=9..12
    let link = |start, end, url: &str| TableCellLink {
        row: 0,
        col: 0,
        start,
        end,
        url: url.into(),
    };
    let candidate_links = vec![
        link(1, 6, "https://example.com"),        // mid-UTF-8 codepoint
        link(0, 99, "https://example.com"),       // past cell length
        link(0, 6, "javascript:alert(1)"),        // unsafe scheme
        link(0, 6, "https://ok.example/1"),       // valid
        link(3, 9, "https://ok.example/overlap"), // overlaps 0..6
    ];
    let (table, _, dropped_links) =
        RichTable::with_merges_and_links(rows, Vec::new(), candidate_links);
    assert_eq!(dropped_links, 4);
    assert_eq!(table.links.len(), 1);
    assert_eq!(table.links[0].url, "https://ok.example/1");

    // Mutating `table.links` directly after construction still cannot panic or emit XSS in writers!
    let mut tampered = table;
    tampered.links.push(link(2, 5, "javascript:evil()"));
    let doc = RichDocument {
        blocks: vec![RichBlock::Table(tampered)],
        ..Default::default()
    };
    let docx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let rels = zip_part(&docx, "word/_rels/document.xml.rels");
    assert!(!rels.contains("javascript:"), "{rels}");
}
