//! Merged table cells across formats: Pages merge regions are read, DOCX/ODT/HWPX keep merges
//! through write and read, and hostile merge declarations (overlap, huge spans, far addresses)
//! are dropped without allocating for them.

mod pages_fixture;

use easyoffice_formats::{
    CellMerge, DecodedContent, DocumentFormat, RichBlock, RichDocument, RichTable,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

fn merge(row: u32, col: u32, row_span: u32, col_span: u32) -> CellMerge {
    CellMerge {
        row,
        col,
        row_span,
        col_span,
    }
}

fn text_rows(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|r| r.iter().map(ToString::to_string).collect())
        .collect()
}

/// 3x3 grid: a two-column header, a block spanning rows 1-2 of column 2, and a 2x2 block.
fn sample_table() -> RichTable {
    let rows = text_rows(&[&["머리", "", "끝"], &["a", "b", "세로"], &["c", "d", ""]]);
    let (table, dropped) = RichTable::with_merges(rows, vec![merge(0, 0, 1, 2), merge(1, 2, 2, 1)]);
    assert_eq!(dropped, 0);
    table
}

fn doc_of(table: RichTable) -> RichDocument {
    RichDocument {
        page_frame: Default::default(),
        title: "병합".into(),
        blocks: vec![RichBlock::Table(table)],
        official_agency: None,
    }
}

fn decoded_table(bytes: &[u8], name: &str) -> (RichTable, Vec<String>) {
    let decoded = UniversalFormatConverter::decode_rich(bytes, Some(name)).expect("decode");
    let warnings = decoded.warnings.clone();
    let DecodedContent::Document(doc) = decoded.content else {
        panic!("document expected");
    };
    let table = doc
        .blocks
        .into_iter()
        .find_map(|b| match b {
            RichBlock::Table(t) => Some(t),
            _ => None,
        })
        .expect("a table");
    (table, warnings)
}

fn entry_text(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).expect("zip");
    let entry = entries.iter().find(|e| e.path == path).expect("entry");
    String::from_utf8(entry.data.clone()).expect("utf8")
}

fn repack(bytes: &[u8], path: &str, edit: impl Fn(&str) -> String) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).expect("zip");
    let out: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            let data = if e.path == path {
                edit(std::str::from_utf8(&e.data).expect("utf8")).into_bytes()
            } else {
                e.data
            };
            (e.path, data)
        })
        .collect();
    ZipArchiveCodec::pack(&out).expect("pack")
}

#[test]
fn docx_writes_grid_span_and_vertical_merge_and_reads_them_back() {
    let table = sample_table();
    let bytes =
        UniversalFormatConverter::encode_rich(&doc_of(table.clone()), DocumentFormat::Docx, None)
            .unwrap();
    let xml = entry_text(&bytes, "word/document.xml");
    assert!(xml.contains("<w:gridSpan w:val=\"2\"/>"));
    assert!(xml.contains("<w:vMerge w:val=\"restart\"/>"));
    assert!(xml.contains("<w:vMerge/>"));
    let (back, warnings) = decoded_table(&bytes, "a.docx");
    assert_eq!(back, table);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn odt_writes_spans_and_covered_cells_and_reads_them_back() {
    let table = sample_table();
    let bytes =
        UniversalFormatConverter::encode_rich(&doc_of(table.clone()), DocumentFormat::Odt, None)
            .unwrap();
    let xml = entry_text(&bytes, "content.xml");
    assert!(xml.contains("table:number-columns-spanned=\"2\" table:number-rows-spanned=\"1\""));
    assert!(xml.contains("table:number-columns-spanned=\"1\" table:number-rows-spanned=\"2\""));
    assert_eq!(xml.matches("<table:covered-table-cell/>").count(), 2);
    let (back, warnings) = decoded_table(&bytes, "a.odt");
    assert_eq!(back, table);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn hwpx_writes_cell_span_without_covered_cells_and_reads_them_back() {
    let table = sample_table();
    let bytes =
        UniversalFormatConverter::encode_rich(&doc_of(table.clone()), DocumentFormat::Hwpx, None)
            .unwrap();
    let xml = entry_text(&bytes, "Contents/section0.xml");
    assert!(xml.contains("<hp:cellSpan colSpan=\"2\" rowSpan=\"1\"/>"));
    assert!(xml.contains("<hp:cellSpan colSpan=\"1\" rowSpan=\"2\"/>"));
    assert_eq!(xml.matches("<hp:tc ").count(), 7);
    let (back, warnings) = decoded_table(&bytes, "a.hwpx");
    assert_eq!(back, table);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn flat_text_formats_keep_the_cell_text_grid() {
    let doc = doc_of(sample_table());
    let md = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Markdown, None).unwrap();
    let md = String::from_utf8(md).unwrap();
    assert!(md.contains("머리") && md.contains("세로"));
    let txt = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Txt, None).unwrap();
    assert!(String::from_utf8(txt).unwrap().contains("머리"));
}

#[test]
fn stale_merges_never_reach_a_writer() {
    let mut table = sample_table();
    table.rows.truncate(1);
    let bytes =
        UniversalFormatConverter::encode_rich(&doc_of(table), DocumentFormat::Docx, None).unwrap();
    let xml = entry_text(&bytes, "word/document.xml");
    assert!(!xml.contains("w:vMerge"));
    assert!(xml.contains("<w:gridSpan w:val=\"2\"/>"));
}

#[test]
fn docx_hostile_spans_are_dropped_with_a_warning() {
    let bytes =
        UniversalFormatConverter::encode_rich(&doc_of(sample_table()), DocumentFormat::Docx, None)
            .unwrap();
    let hostile = repack(&bytes, "word/document.xml", |xml| {
        xml.replace(
            "<w:gridSpan w:val=\"2\"/>",
            "<w:gridSpan w:val=\"4294967295\"/>",
        )
        .replace("<w:vMerge/>", "<w:vMerge/><w:gridSpan w:val=\"7\"/>")
    });
    let (table, warnings) = decoded_table(&hostile, "a.docx");
    // The 4294967295-column span is refused (that cell stays one column wide) and the 7-column
    // continuation of an orphan `vMerge` is the only span that fits the row limits.
    assert_eq!(table.rows[0], vec!["머리", "끝"]);
    assert_eq!(table.merges, vec![merge(2, 2, 1, 7)]);
    assert!(warnings.iter().any(|w| w.contains("병합")), "{warnings:?}");
}

#[test]
fn docx_orphan_continuation_is_a_plain_cell() {
    let xml = "<w:document xmlns:w=\"w\"><w:body><w:tbl><w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc></w:tr></w:tbl></w:body></w:document>";
    let base = UniversalFormatConverter::encode_rich(
        &doc_of(RichTable::new(text_rows(&[&["x"]]))),
        DocumentFormat::Docx,
        None,
    )
    .unwrap();
    let bytes = repack(&base, "word/document.xml", |_| xml.to_string());
    let (table, _) = decoded_table(&bytes, "a.docx");
    assert_eq!(table.rows.len(), 1);
    assert!(table.merges.is_empty());
}

#[test]
fn odt_overlapping_and_huge_spans_are_dropped() {
    let bytes =
        UniversalFormatConverter::encode_rich(&doc_of(sample_table()), DocumentFormat::Odt, None)
            .unwrap();
    let hostile = repack(&bytes, "content.xml", |xml| {
        xml.replace(
            "table:number-columns-spanned=\"2\" table:number-rows-spanned=\"1\"",
            "table:number-columns-spanned=\"4000000000\" table:number-rows-spanned=\"4000000000\"",
        )
        .replace(
            "table:number-columns-spanned=\"1\" table:number-rows-spanned=\"2\"",
            "table:number-columns-spanned=\"3\" table:number-rows-spanned=\"3\" table:number-columns-repeated=\"4000000000\"",
        )
    });
    let (table, warnings) = decoded_table(&hostile, "a.odt");
    assert!(table.merges.is_empty());
    assert!(table.rows.iter().all(|r| r.len() <= 1_024 + 3));
    assert!(warnings.iter().any(|w| w.contains("병합")), "{warnings:?}");
}

#[test]
fn hwpx_overlapping_duplicate_and_far_cells_are_dropped() {
    let bytes =
        UniversalFormatConverter::encode_rich(&doc_of(sample_table()), DocumentFormat::Hwpx, None)
            .unwrap();
    let hostile = repack(&bytes, "Contents/section0.xml", |xml| {
        let overlapping = xml.replacen(
            "<hp:cellSpan colSpan=\"1\" rowSpan=\"1\"/>",
            "<hp:cellSpan colSpan=\"3\" rowSpan=\"3\"/>",
            1,
        );
        let far = overlapping.replace(
            "colAddr=\"1\" rowAddr=\"2\"",
            "colAddr=\"4000000000\" rowAddr=\"4000000000\"",
        );
        far.replace("rowSpan=\"2\"", "rowSpan=\"65535\"")
    });
    let (table, warnings) = decoded_table(&hostile, "a.hwpx");
    assert!(table.rows.len() <= 3 && table.rows.iter().all(|r| r.len() <= 5));
    assert!(table
        .merges
        .iter()
        .all(|m| m.row_span <= 3 && m.col_span <= 3));
    assert!(warnings.iter().any(|w| w.contains("병합")), "{warnings:?}");
}

#[test]
fn pages_merge_regions_are_read_and_invalid_ones_reported() {
    let grid = vec![
        vec!["가", "나", "다"],
        vec!["라", "마", "바"],
        vec!["사", "아", "자"],
    ];
    let bytes = pages_fixture::merged_table_document(&grid, &[(0, 0, 1, 2), (1, 2, 2, 1)]);
    let (table, warnings) = decoded_table(&bytes, "m.pages");
    assert_eq!(table.merges, vec![merge(0, 0, 1, 2), merge(1, 2, 2, 1)]);
    assert_eq!(table.rows[0], vec!["가", "나", "다"]);
    assert!(!warnings.iter().any(|w| w.contains("병합")), "{warnings:?}");

    // Overlapping, past the grid, and a region of 65535 x 65535 cells.
    let hostile = pages_fixture::merged_table_document(
        &grid,
        &[
            (0, 0, 2, 2),
            (1, 1, 2, 2),
            (2, 2, 2, 2),
            (0, 2, 65_535, 65_535),
        ],
    );
    let (table, warnings) = decoded_table(&hostile, "h.pages");
    assert_eq!(table.merges, vec![merge(0, 0, 2, 2)]);
    assert!(warnings.iter().any(|w| w.contains("병합")), "{warnings:?}");
}

#[test]
fn pages_without_a_merge_map_has_no_regions() {
    let bytes = pages_fixture::merged_table_document(&[vec!["x", "y"]], &[]);
    let (table, _) = decoded_table(&bytes, "p.pages");
    assert!(table.merges.is_empty());
}

#[test]
fn rtf_writes_cell_edges_and_merge_marks_and_reads_them_back() {
    let table = sample_table();
    let bytes =
        UniversalFormatConverter::encode_rich(&doc_of(table.clone()), DocumentFormat::Rtf, None)
            .unwrap();
    let rtf = String::from_utf8(bytes.clone()).unwrap();
    assert!(rtf.contains("\\clmgf\\cellx3120\\clmrg\\cellx6240\\cellx9360 "));
    assert!(rtf.contains("\\cellx6240\\clvmgf\\cellx9360 "));
    assert!(rtf.contains("\\clvmrg\\cellx9360"));
    assert_eq!(rtf.matches("\\row}").count(), 3);
    let (back, warnings) = decoded_table(&bytes, "a.rtf");
    assert_eq!(back, table);
    assert!(!warnings.iter().any(|w| w.contains("병합")), "{warnings:?}");
}

#[test]
fn rtf_table_without_merges_keeps_cells_and_writes_no_marks() {
    let table = RichTable::new(text_rows(&[&["a", "b"], &["c", "d"]]));
    let bytes =
        UniversalFormatConverter::encode_rich(&doc_of(table.clone()), DocumentFormat::Rtf, None)
            .unwrap();
    let rtf = String::from_utf8(bytes.clone()).unwrap();
    assert!(!rtf.contains("\\clm") && !rtf.contains("\\clvm"));
    assert_eq!(decoded_table(&bytes, "a.rtf").0, table);
}

#[test]
fn rtf_hostile_merge_marks_are_dropped_with_a_warning() {
    let rtf = "{\\rtf1\\ansi\n\
        {\\trowd{\\*\\tblidx 0} \\clmgf\\cellx1000\\clmrg\\cellx2000\\clmrg\\cellx3000 a\\cell b\\cell \\row}\n\
        {\\trowd{\\*\\tblidx 0} \\clvmrg\\cellx1000\\cellx2000 c\\cell d\\cell \\row}\n}";
    let (table, warnings) = decoded_table(rtf.as_bytes(), "a.rtf");
    assert_eq!(table.rows.len(), 2);
    assert!(table.merges.is_empty());
    assert!(warnings.iter().any(|w| w.contains("병합")), "{warnings:?}");
}

#[test]
fn flat_rtf_decoder_reads_cell_text_of_rich_output() {
    let bytes =
        UniversalFormatConverter::encode_rich(&doc_of(sample_table()), DocumentFormat::Rtf, None)
            .unwrap();
    let payload = easyoffice_formats::RtfCodec::decode(&bytes).expect("flat decode");
    assert_eq!(payload.tables.len(), 1);
    assert_eq!(
        payload.tables[0],
        text_rows(&[&["머리", "", "끝"], &["a", "b", "세로"], &["c", "d", ""]])
    );
}
