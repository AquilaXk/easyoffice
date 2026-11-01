use easyoffice_formats::deflate::{
    deflate_raw, deflate_zlib, inflate_auto, inflate_raw, inflate_zlib,
};
use easyoffice_formats::{
    EasyOfficeError, Hwp5Codec, Hwp5DocumentPayload, Hwp5ParagraphRecord, HwpxCodec,
    LegacyOle2Codec, LegacyXlsPayload, OdfCodec, Ole2ContainerCodec, OoxmlCodec, OoxmlPptxPayload,
    OoxmlXlsxPayload, PdfDocumentModel, PdfPageModel, PdfSerializer, UniversalFormatConverter,
    ZipArchiveCodec, ZipSecurityPolicy, MAX_BIFF8_COL, MAX_BIFF8_RECORD_DATA_LEN, MAX_BIFF8_ROW,
    MAX_HWP_TABLE_COLS, MAX_HWP_TABLE_ROWS, MAX_OLE2_DIR_ENTRIES,
};

#[test]
fn test_ooxml_xlsx_pptx_opc_and_multi_sheet_and_limits() {
    let policy = ZipSecurityPolicy::default();

    // 1. Multi-sheet XLSX roundtrip and OPC relationship / styles parts
    let sheets = vec![
        (
            "Revenue".to_string(),
            vec![
                (0, 0, "Q1".to_string()),
                (0, 1, "100".to_string()),
                (1, 1, "=B1*2".to_string()),
            ],
        ),
        (
            "Costs".to_string(),
            vec![(0, 0, "Rent".to_string()), (0, 1, "50".to_string())],
        ),
    ];
    let xlsx_bytes = OoxmlCodec::pack_xlsx_multi(&sheets).expect("pack multi-sheet xlsx");
    let zip_entries = ZipArchiveCodec::unpack(&xlsx_bytes, &policy).expect("unpack xlsx zip");
    let paths: Vec<&str> = zip_entries.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"_rels/.rels"));
    assert!(paths.contains(&"xl/_rels/workbook.xml.rels"));
    assert!(paths.contains(&"xl/styles.xml"));
    assert!(paths.contains(&"xl/worksheets/sheet1.xml"));
    assert!(paths.contains(&"xl/worksheets/sheet2.xml"));

    let unpacked_sheets = OoxmlCodec::unpack_xlsx_multi(&xlsx_bytes).expect("unpack multi xlsx");
    assert_eq!(unpacked_sheets, sheets);

    // UniversalFormatConverter preserves all sheets in tables
    let (_, decoded) =
        UniversalFormatConverter::decode(&xlsx_bytes, Some("multi.xlsx")).expect("decode xlsx");
    assert_eq!(decoded.title, "Revenue");
    assert_eq!(decoded.tables.len(), 2);

    // 2. Empty <si/> and <rPh> phonetic guide stripping and <sheets> vs <sheet> tag disambiguation
    let custom_xlsx = ZipArchiveCodec::pack(&[
        (
            "[Content_Types].xml".to_string(),
            b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>"
                .to_vec(),
        ),
        (
            "xl/workbook.xml".to_string(),
            br#"<workbook><sheets><sheet name="RealSheet" sheetId="1" r:id="rId1"/></sheets></workbook>"#
                .to_vec(),
        ),
        (
            "xl/sharedStrings.xml".to_string(),
            br#"<sst><si/><si><t>Base</t><rPh sb="0" eb="4"><t> Furigana</t></rPh></si></sst>"#
                .to_vec(),
        ),
        (
            "xl/worksheets/sheet1.xml".to_string(),
            br#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row></sheetData></worksheet>"#
                .to_vec(),
        ),
    ])
    .expect("pack custom xlsx");
    let parsed_custom = OoxmlCodec::unpack_xlsx(&custom_xlsx).expect("unpack custom xlsx");
    assert_eq!(parsed_custom.sheet_name, "RealSheet");
    assert_eq!(
        parsed_custom.cells,
        vec![(0, 0, "".to_string()), (0, 1, "Base".to_string())]
    );

    // 3. MAX_XLSX_ROWS and MAX_XLSX_COLS limit enforcement in pack_xlsx
    let oversized_row_sheet = OoxmlXlsxPayload {
        sheet_name: "TooTall".to_string(),
        cells: vec![(OoxmlCodec::MAX_XLSX_ROWS, 0, "x".to_string())],
    };
    assert!(matches!(
        OoxmlCodec::pack_xlsx(&oversized_row_sheet),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
    let oversized_col_sheet = OoxmlXlsxPayload {
        sheet_name: "TooWide".to_string(),
        cells: vec![(0, OoxmlCodec::MAX_XLSX_COLS, "x".to_string())],
    };
    assert!(matches!(
        OoxmlCodec::pack_xlsx(&oversized_col_sheet),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 4. PPTX OPC structure, <p:sldIdLst> order, and <a:p> run concatenation
    let pptx_payload = OoxmlPptxPayload {
        title: "Deck".to_string(),
        is_slideshow: false,
        slides: vec![
            (
                "Slide 1 Title".to_string(),
                vec!["Bullet A".to_string()],
                "Note 1".to_string(),
            ),
            ("Slide 2 Title".to_string(), vec![], String::new()),
        ],
    };
    let pptx_bytes = OoxmlCodec::pack_pptx(&pptx_payload).expect("pack pptx");
    let pptx_entries = ZipArchiveCodec::unpack(&pptx_bytes, &policy).expect("unpack pptx zip");
    let pptx_paths: Vec<&str> = pptx_entries.iter().map(|e| e.path.as_str()).collect();
    assert!(pptx_paths.contains(&"_rels/.rels"));
    assert!(pptx_paths.contains(&"ppt/_rels/presentation.xml.rels"));
    assert!(pptx_paths.contains(&"ppt/slideMasters/slideMaster1.xml"));
    assert!(pptx_paths.contains(&"ppt/slideLayouts/slideLayout1.xml"));

    // Custom PPTX with multiple <a:r><a:t> runs inside one <a:p> and custom <p:sldIdLst> ordering
    let custom_pptx = ZipArchiveCodec::pack(&[
        ("[Content_Types].xml".to_string(), b"<Types/>".to_vec()),
        (
            "ppt/presentation.xml".to_string(),
            br#"<p:presentation><p:sldIdLst><p:sldId id="256" r:id="rId2"/><p:sldId id="257" r:id="rId1"/></p:sldIdLst></p:presentation>"#
                .to_vec(),
        ),
        (
            "ppt/_rels/presentation.xml.rels".to_string(),
            br#"<Relationships><Relationship Id="rId1" Target="slides/slide1.xml"/><Relationship Id="rId2" Target="slides/slide2.xml"/></Relationships>"#
                .to_vec(),
        ),
        (
            "ppt/slides/slide1.xml".to_string(),
            br#"<p:sld><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Second </a:t></a:r><a:r><a:t>Slide</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#
                .to_vec(),
        ),
        (
            "ppt/slides/slide2.xml".to_string(),
            br#"<p:sld><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>First </a:t></a:r><a:r><a:t>Ordered</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#
                .to_vec(),
        ),
    ])
    .expect("pack custom pptx");
    let unpacked_pptx = OoxmlCodec::unpack_pptx(&custom_pptx).expect("unpack custom pptx");
    assert_eq!(unpacked_pptx.slides.len(), 2);
    assert_eq!(unpacked_pptx.slides[0].0, "First Ordered");
    assert_eq!(unpacked_pptx.slides[1].0, "Second Slide");
}

#[test]
fn test_odf_and_hwpx_nesting_multi_sheet_openformula_and_sections() {
    // 1. ODT with meta.xml <dc:title>, <text:h>, nested <text:p>, <text:span>, <text:s>, <text:tab/>, <text:line-break/>
    let custom_odt = ZipArchiveCodec::pack(&[
        (
            "mimetype".to_string(),
            b"application/vnd.oasis.opendocument.text".to_vec(),
        ),
        (
            "meta.xml".to_string(),
            br#"<office:document-meta><office:meta><dc:title>Spec Title</dc:title></office:meta></office:document-meta>"#
                .to_vec(),
        ),
        (
            "content.xml".to_string(),
            br#"<office:document-content><office:body><office:text><text:h text:outline-level="1">Section 1</text:h><text:p>Hello<text:s text:c="2"/><text:span>World</text:span><text:tab/>Tab<text:line-break/>Next</text:p></office:text></office:body></office:document-content>"#
                .to_vec(),
        ),
    ])
    .expect("pack custom odt");
    let (odt_title, odt_paragraphs, _) = OdfCodec::unpack_odt(&custom_odt).expect("unpack odt");
    assert_eq!(odt_title, "Spec Title");
    assert_eq!(
        odt_paragraphs,
        vec![
            "Section 1".to_string(),
            "Hello  World\tTab\nNext".to_string()
        ]
    );

    // 2. Multi-sheet ODS with OpenFormula normalization, covered-table-cell, and trailing repeat trimming
    let ods_sheets = vec![
        (
            "Summary".to_string(),
            vec![
                (0, 0, "10".to_string()),
                (0, 1, "20".to_string()),
                (1, 0, "=SUM(A1:B1)".to_string()),
                (1, 1, "='Data Sheet'!A1".to_string()),
            ],
        ),
        ("Data Sheet".to_string(), vec![(0, 0, "42".to_string())]),
    ];
    let ods_bytes = OdfCodec::pack_ods_multi(&ods_sheets).expect("pack multi ods");
    let unpacked_ods = OdfCodec::unpack_ods_multi(&ods_bytes).expect("unpack multi ods");
    assert_eq!(unpacked_ods, ods_sheets);

    // Trailing empty repeated columns/rows and covered-table-cell in ODS
    let custom_ods = ZipArchiveCodec::pack(&[
        (
            "mimetype".to_string(),
            b"application/vnd.oasis.opendocument.spreadsheet".to_vec(),
        ),
        (
            "content.xml".to_string(),
            br#"<office:document-content><office:body><office:spreadsheet><table:table table:name="Grid"><table:table-row><table:table-cell office:value-type="string" table:number-columns-spanned="2"><text:p>Merged</text:p></table:table-cell><table:covered-table-cell/><table:table-cell office:value-type="string"><text:p>ColC</text:p></table:table-cell><table:table-cell table:number-columns-repeated="16384"/></table:table-row><table:table-row table:number-rows-repeated="1048570"><table:table-cell/></table:table-row></table:table></office:spreadsheet></office:body></office:document-content>"#
                .to_vec(),
        ),
    ])
    .expect("pack custom ods");
    let (grid_name, grid_cells) = OdfCodec::unpack_ods(&custom_ods).expect("unpack grid ods");
    assert_eq!(grid_name, "Grid");
    assert_eq!(
        grid_cells,
        vec![(0, 0, "Merged".to_string()), (0, 2, "ColC".to_string())]
    );

    // 3. ODP filters out <presentation:notes> (into notes field, not bullets) and <text:page-number>
    let custom_odp = ZipArchiveCodec::pack(&[
        (
            "mimetype".to_string(),
            b"application/vnd.oasis.opendocument.presentation".to_vec(),
        ),
        (
            "content.xml".to_string(),
            br#"<office:document-content><office:body><office:presentation><draw:page draw:name="Slide Main Title"><draw:frame><draw:text-box><text:p>Bullet One</text:p><text:p><text:page-number>1</text:page-number></text:p></draw:text-box></draw:frame><presentation:notes><draw:frame><draw:text-box><text:p>Speaker Notes Secret</text:p></draw:text-box></draw:frame></presentation:notes></draw:page></office:presentation></office:body></office:document-content>"#
                .to_vec(),
        ),
    ])
    .expect("pack custom odp");
    let (_, odp_slides) = OdfCodec::unpack_odp(&custom_odp).expect("unpack custom odp");
    assert_eq!(odp_slides.len(), 1);
    assert_eq!(odp_slides[0].0, "Slide Main Title");
    assert_eq!(odp_slides[0].1, vec!["Bullet One".to_string()]);
    assert_eq!(odp_slides[0].2, "Speaker Notes Secret");

    // 4. HWPX multi-section reading (section0.xml + section1.xml)
    let custom_hwpx = ZipArchiveCodec::pack(&[
        ("mimetype".to_string(), b"application/hwp+zip".to_vec()),
        (
            "Contents/header.xml".to_string(),
            br#"<hh:head title="HWPX Doc Title" agency="Agency"/>"#.to_vec(),
        ),
        (
            "Contents/section0.xml".to_string(),
            b"<hs:sec><hp:p><hp:run><hp:t>Body Sec 0</hp:t></hp:run></hp:p></hs:sec>".to_vec(),
        ),
        (
            "Contents/section1.xml".to_string(),
            b"<hs:sec><hp:p><hp:run><hp:t>Body Sec 1</hp:t></hp:run></hp:p></hs:sec>".to_vec(),
        ),
    ])
    .expect("pack custom hwpx");
    let hwpx = HwpxCodec::unpack(&custom_hwpx).expect("unpack multi-section hwpx");
    assert_eq!(hwpx.title, "HWPX Doc Title");
    assert_eq!(
        hwpx.paragraphs,
        vec!["Body Sec 0".to_string(), "Body Sec 1".to_string()]
    );
}

#[test]
fn test_hwp5_limits_multi_section_doc_properties_and_standard_table_records() {
    // 1. Dimension limits in Hwp5Codec::pack
    let oversized_rows_hwp = Hwp5DocumentPayload {
        version: (5, 0, 3, 0),
        is_template: false,
        agency_name: "Agency".to_string(),
        document_number: "No-1".to_string(),
        paragraphs: vec![],
        table_cells: vec![vec!["x".to_string()]; MAX_HWP_TABLE_ROWS + 1],
    };
    assert!(matches!(
        Hwp5Codec::pack(&oversized_rows_hwp),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
    let oversized_cols_hwp = Hwp5DocumentPayload {
        version: (5, 0, 3, 0),
        is_template: false,
        agency_name: "Agency".to_string(),
        document_number: "No-1".to_string(),
        paragraphs: vec![],
        table_cells: vec![vec!["x".to_string(); MAX_HWP_TABLE_COLS + 1]],
    };
    assert!(matches!(
        Hwp5Codec::pack(&oversized_cols_hwp),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 2. Binary HWPTAG_DOCUMENT_PROPERTIES (tag 16, 26 bytes) skipped + multi-section + standard HWPTAG_TABLE (tag 77)
    fn encode_record(tag_id: u16, level: u16, payload: &[u8]) -> Vec<u8> {
        let header: u32 = (tag_id as u32 & 0x3FF)
            | ((level as u32 & 0x3FF) << 10)
            | ((payload.len() as u32) << 20);
        let mut out = header.to_le_bytes().to_vec();
        out.extend_from_slice(payload);
        out
    }
    fn encode_utf16le(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }

    // DocInfo stream with 26-byte binary HWPTAG_DOCUMENT_PROPERTIES (tag 16)
    let doc_info_stream = encode_record(16, 0, &[0x01u8; 26]);

    // Section0 with paragraph + standard HWPTAG_TABLE (tag 77) + 2 cells (HWPTAG_LIST_HEADER tag 72 + HWPTAG_PARA_TEXT tag 67)
    let mut sec0 = Vec::new();
    sec0.extend_from_slice(&encode_record(67, 0, &encode_utf16le("Sec0 Paragraph")));
    let mut std_table_payload = vec![0u8; 18];
    std_table_payload[4..6].copy_from_slice(&1u16.to_le_bytes()); // 1 row
    std_table_payload[6..8].copy_from_slice(&2u16.to_le_bytes()); // 2 cols
    sec0.extend_from_slice(&encode_record(77, 1, &std_table_payload));
    sec0.extend_from_slice(&encode_record(72, 2, &[0u8; 8]));
    sec0.extend_from_slice(&encode_record(67, 3, &encode_utf16le("Cell A1")));
    sec0.extend_from_slice(&encode_record(72, 2, &[0u8; 8]));
    sec0.extend_from_slice(&encode_record(67, 3, &encode_utf16le("Cell B1")));

    // Section1 with another paragraph
    let sec1 = encode_record(67, 0, &encode_utf16le("Sec1 Paragraph"));

    let mut file_header = vec![0u8; 256];
    let sig = b"HWP Document File";
    file_header[..sig.len()].copy_from_slice(sig);
    file_header[32..36].copy_from_slice(&0x0500_0300u32.to_le_bytes());
    file_header[36..40].copy_from_slice(&0u32.to_le_bytes()); // uncompressed

    let streams = vec![
        ("FileHeader".to_string(), file_header),
        ("DocInfo".to_string(), doc_info_stream),
        ("Section0".to_string(), sec0),
        ("Section1".to_string(), sec1),
    ];
    let hwp_bytes = Ole2ContainerCodec::pack(&streams).expect("pack custom hwp5");
    let unpacked = Hwp5Codec::unpack(&hwp_bytes).expect("unpack custom hwp5");
    assert_eq!(unpacked.agency_name, "");
    assert_eq!(
        unpacked.paragraphs,
        vec![
            Hwp5ParagraphRecord {
                text: "Sec0 Paragraph".to_string(),
                style_name: "본문".to_string(),
                is_official_header: false,
            },
            Hwp5ParagraphRecord {
                text: "Sec1 Paragraph".to_string(),
                style_name: "본문".to_string(),
                is_official_header: false,
            },
        ]
    );
    assert_eq!(
        unpacked.table_cells,
        vec![vec!["Cell A1".to_string(), "Cell B1".to_string()]]
    );
}

#[test]
fn test_biff8_xls_continue_latin1_multisheet_and_legacy_doc_ppt_and_ole2_guards() {
    // 1. BIFF8 XLS dimension limits
    let bad_row_xls = LegacyXlsPayload {
        sheet_name: "Sheet1".to_string(),
        cells: vec![(MAX_BIFF8_ROW + 1, 0, "1".to_string())],
    };
    assert!(matches!(
        LegacyOle2Codec::pack_xls(&bad_row_xls),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
    let bad_col_xls = LegacyXlsPayload {
        sheet_name: "Sheet1".to_string(),
        cells: vec![(0, MAX_BIFF8_COL + 1, "1".to_string())],
    };
    assert!(matches!(
        LegacyOle2Codec::pack_xls(&bad_col_xls),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    // 2. BIFF8 XLS multi-sheet + SST CONTINUE (> 8224 bytes) + Latin-1 ('é')
    let long_latin1_a = format!("Café_{}", "A".repeat(4500));
    let long_unicode_b = format!("한글_{}", "B".repeat(3000));
    let xls_sheets = vec![
        LegacyXlsPayload {
            sheet_name: "FirstSheet".to_string(),
            cells: vec![
                (0, 0, long_latin1_a.clone()),
                (0, 1, long_unicode_b.clone()),
            ],
        },
        LegacyXlsPayload {
            sheet_name: "SecondSheet".to_string(),
            cells: vec![(0, 0, "99.5".to_string())],
        },
    ];
    let xls_bytes = LegacyOle2Codec::pack_xls_multi(&xls_sheets).expect("pack multi xls");
    assert!(xls_bytes.len() > MAX_BIFF8_RECORD_DATA_LEN);
    let unpacked_xls = LegacyOle2Codec::unpack_xls_multi(&xls_bytes).expect("unpack multi xls");
    assert_eq!(unpacked_xls, xls_sheets);

    // 3. Legacy DOC with CLX piece table (1Table) and 8-bit CP1252 piece
    let mut word_doc = vec![0u8; 512];
    word_doc[0..2].copy_from_slice(&0xA5ECu16.to_le_bytes()); // wIdent
    word_doc[2..4].copy_from_slice(&0x00C1u16.to_le_bytes()); // nFib
    word_doc[0x000A..0x000C].copy_from_slice(&0x0200u16.to_le_bytes()); // flags: fWhichTblStm = 1 (1Table)
    word_doc[0x01A2..0x01A6].copy_from_slice(&0u32.to_le_bytes()); // fcClx = 0
    let text_piece = b"PieceOne\rPieceTwo\r";
    let text_fc = 256u32;
    word_doc[text_fc as usize..text_fc as usize + text_piece.len()].copy_from_slice(text_piece);

    let mut clx = Vec::new();
    clx.push(0x02u8); // Pcdt
    let plc_pcd_len = 8 + 8; // 2 CPs (0..18) + 1 PCD (8 bytes)
    clx.extend_from_slice(&(plc_pcd_len as u32).to_le_bytes());
    clx.extend_from_slice(&0u32.to_le_bytes()); // CP[0] = 0
    clx.extend_from_slice(&(text_piece.len() as u32).to_le_bytes()); // CP[1] = 18
    clx.extend_from_slice(&0u16.to_le_bytes());
    let fc_compressed = (text_fc * 2) | 0x4000_0000;
    clx.extend_from_slice(&fc_compressed.to_le_bytes());
    clx.extend_from_slice(&0u16.to_le_bytes());
    word_doc[0x01A6..0x01AA].copy_from_slice(&(clx.len() as u32).to_le_bytes()); // lcbClx

    let doc_streams = vec![
        (["Wor", "dDocument"].concat(), word_doc),
        ("1Table".to_string(), clx),
    ];
    let doc_bytes = Ole2ContainerCodec::pack(&doc_streams).expect("pack clx doc");
    let unpacked_doc = LegacyOle2Codec::unpack_doc(&doc_bytes).expect("unpack clx doc");
    assert_eq!(
        unpacked_doc.paragraphs,
        vec!["PieceOne".to_string(), "PieceTwo".to_string()]
    );

    // 4. Legacy PPT multi-slide splitting via SlidePersistAtom (0x03F3)
    fn ppt_atom(ver_inst: u16, rec_type: u16, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&ver_inst.to_le_bytes());
        out.extend_from_slice(&rec_type.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }
    let mut ppt_stream = Vec::new();
    ppt_stream.extend_from_slice(&ppt_atom(0x0000, 0x03F3, &[0u8; 20])); // SlidePersistAtom 1
    ppt_stream.extend_from_slice(&ppt_atom(0x0000, 0x0F9F, &0u32.to_le_bytes())); // Title
    ppt_stream.extend_from_slice(&ppt_atom(0x0000, 0x0FA8, b"Slide 1 Title"));
    ppt_stream.extend_from_slice(&ppt_atom(0x0000, 0x0F9F, &1u32.to_le_bytes())); // Body
    ppt_stream.extend_from_slice(&ppt_atom(0x0000, 0x0FA8, b"Bullet 1A\rBullet 1B"));
    ppt_stream.extend_from_slice(&ppt_atom(0x0000, 0x03F3, &[0u8; 20])); // SlidePersistAtom 2
    ppt_stream.extend_from_slice(&ppt_atom(0x0000, 0x0F9F, &0u32.to_le_bytes())); // Title
    ppt_stream.extend_from_slice(&ppt_atom(0x0000, 0x0FA8, b"Slide 2 Title"));
    let ppt_streams = vec![(["Power", "Point Document"].concat(), ppt_stream)];
    let ppt_bytes = Ole2ContainerCodec::pack(&ppt_streams).expect("pack multi-slide ppt");
    let unpacked_ppt = LegacyOle2Codec::unpack_ppt(&ppt_bytes).expect("unpack multi-slide ppt");
    assert_eq!(unpacked_ppt.slides.len(), 2);
    assert_eq!(unpacked_ppt.slides[0].title, "Slide 1 Title");
    assert_eq!(
        unpacked_ppt.slides[0].bullets,
        vec!["Bullet 1A".to_string(), "Bullet 1B".to_string()]
    );
    assert_eq!(unpacked_ppt.slides[1].title, "Slide 2 Title");

    // 5. OLE2 case-insensitive duplicate directory entry rejection and v4 (4096B sector) unpacking
    let dup_streams = vec![
        ("Workbook".to_string(), vec![1, 2, 3]),
        ("WORKBOOK".to_string(), vec![4, 5, 6]),
    ];
    assert!(matches!(
        Ole2ContainerCodec::pack(&dup_streams),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
    assert_eq!(MAX_OLE2_DIR_ENTRIES, 4096);

    // Construct a minimal v4 (4096-byte sector, sector_shift = 12) OLE2 container
    let mut v4_cfb = vec![0u8; 4096 * 4];
    v4_cfb[0..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    v4_cfb[24..26].copy_from_slice(&0x003Eu16.to_le_bytes());
    v4_cfb[26..28].copy_from_slice(&0x0004u16.to_le_bytes()); // v4
    v4_cfb[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
    v4_cfb[30..32].copy_from_slice(&12u16.to_le_bytes()); // 4096B sectors
    v4_cfb[32..34].copy_from_slice(&6u16.to_le_bytes());
    v4_cfb[40..44].copy_from_slice(&1u32.to_le_bytes()); // num_dir_sectors = 1
    v4_cfb[44..48].copy_from_slice(&1u32.to_le_bytes()); // num_fat_sectors = 1
    v4_cfb[48..52].copy_from_slice(&0u32.to_le_bytes()); // first_dir_sector = 0
    v4_cfb[56..60].copy_from_slice(&4096u32.to_le_bytes());
    v4_cfb[60..64].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes());
    v4_cfb[68..72].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes());
    v4_cfb[76..80].copy_from_slice(&2u32.to_le_bytes()); // DIFAT[0] = sector 2
    for i in 1..109 {
        let off = 76 + i * 4;
        v4_cfb[off..off + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    }
    // Sector 0 (offset 4096): Root Entry (0) + Stream Entry (1)
    let dir_base = 4096;
    for (idx, ch) in "Root Entry".encode_utf16().enumerate() {
        v4_cfb[dir_base + idx * 2..dir_base + idx * 2 + 2].copy_from_slice(&ch.to_le_bytes());
    }
    v4_cfb[dir_base + 64..dir_base + 66].copy_from_slice(&22u16.to_le_bytes());
    v4_cfb[dir_base + 66] = 5; // Root
    v4_cfb[dir_base + 68..dir_base + 72].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    v4_cfb[dir_base + 72..dir_base + 76].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    v4_cfb[dir_base + 76..dir_base + 80].copy_from_slice(&1u32.to_le_bytes()); // child_id = 1
    v4_cfb[dir_base + 116..dir_base + 120].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes());

    let e1_base = dir_base + 128;
    for (idx, ch) in "V4Stream".encode_utf16().enumerate() {
        v4_cfb[e1_base + idx * 2..e1_base + idx * 2 + 2].copy_from_slice(&ch.to_le_bytes());
    }
    v4_cfb[e1_base + 64..e1_base + 66].copy_from_slice(&18u16.to_le_bytes());
    v4_cfb[e1_base + 66] = 2; // Stream
    v4_cfb[e1_base + 68..e1_base + 72].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    v4_cfb[e1_base + 72..e1_base + 76].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    v4_cfb[e1_base + 76..e1_base + 80].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    v4_cfb[e1_base + 116..e1_base + 120].copy_from_slice(&1u32.to_le_bytes()); // start_sector = 1
    v4_cfb[e1_base + 120..e1_base + 128].copy_from_slice(&4096u64.to_le_bytes()); // 4096B

    // Sector 1 (offset 8192): 4096 bytes of payload
    v4_cfb[8192..12288].fill(0x5A);

    // Sector 2 (offset 12288): FAT sector
    let fat_base = 12288;
    for i in 0..1024 {
        let off = fat_base + i * 4;
        v4_cfb[off..off + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    }
    v4_cfb[fat_base..fat_base + 4].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes()); // sector 0
    v4_cfb[fat_base + 4..fat_base + 8].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes()); // sector 1
    v4_cfb[fat_base + 8..fat_base + 12].copy_from_slice(&0xFFFF_FFFDu32.to_le_bytes()); // sector 2

    let v4_streams = Ole2ContainerCodec::unpack(&v4_cfb).expect("unpack v4 4096B sector cfb");
    assert_eq!(v4_streams.len(), 1);
    assert_eq!(v4_streams[0].name, "V4Stream");
    assert_eq!(v4_streams[0].data.len(), 4096);
    assert!(v4_streams[0].data.iter().all(|&b| b == 0x5A));
}

#[test]
fn test_pdf_xref_stream_objstm_tounicode_kerning_and_title_and_stamp() {
    // 1. PDF 1.5+ /XRef stream + /ObjStm + /ToUnicode CMap + TJ kerning + UTF-16BE BOM text
    let cmap_stream = b"/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n2 beginbfchar\n<01> <0048>\n<02> <0069>\nendbfchar\n1 beginbfrange\n<10> <11> <0041>\nendbfrange\nendcmap";
    let content_stream =
        br#"BT /F1 12 Tf 72 720 Td [<0102> -220 <1011>] TJ 40 0 Td (\376\377\000!\000?) Tj ET"#;

    // Object 1 (Catalog), Object 2 (Pages), Object 3 (Page) inside Object Stream (Obj 10)
    let obj1_str = b"<< /Type /Catalog /Pages 2 0 R >>";
    let obj2_str = b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>";
    let obj3_str =
        b"<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>";
    let header_pairs = format!(
        "1 0 2 {} 3 {} ",
        obj1_str.len() + 1,
        obj1_str.len() + 1 + obj2_str.len() + 1
    );
    let first_offset = header_pairs.len();
    let mut objstm_raw = Vec::new();
    objstm_raw.extend_from_slice(header_pairs.as_bytes());
    objstm_raw.extend_from_slice(obj1_str);
    objstm_raw.push(b'\n');
    objstm_raw.extend_from_slice(obj2_str);
    objstm_raw.push(b'\n');
    objstm_raw.extend_from_slice(obj3_str);

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.7\n");

    let off4 = pdf.len();
    pdf.extend_from_slice(b"4 0 obj\n<< /Type /Font /Subtype /Type0 /ToUnicode 6 0 R >>\nendobj\n");

    let off5 = pdf.len();
    pdf.extend_from_slice(
        format!("5 0 obj\n<< /Length {} >>\nstream\n", content_stream.len()).as_bytes(),
    );
    pdf.extend_from_slice(content_stream);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let off6 = pdf.len();
    pdf.extend_from_slice(
        format!("6 0 obj\n<< /Length {} >>\nstream\n", cmap_stream.len()).as_bytes(),
    );
    pdf.extend_from_slice(cmap_stream);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let off7 = pdf.len();
    pdf.extend_from_slice(b"7 0 obj\n<< /Title (Fallback Info Title) >>\nendobj\n");

    let off10 = pdf.len();
    pdf.extend_from_slice(
        format!(
            "10 0 obj\n<< /Type /ObjStm /N 3 /First {} /Length {} >>\nstream\n",
            first_offset,
            objstm_raw.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(&objstm_raw);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_off = pdf.len();
    let mut xref_data = Vec::new();
    let mut push_entry = |etype: u8, f2: u32, f3: u8| {
        xref_data.push(etype);
        xref_data.extend_from_slice(&f2.to_be_bytes());
        xref_data.push(f3);
    };
    push_entry(0, 0, 255); // obj 0 free
    push_entry(2, 10, 0); // obj 1 in objstm 10 idx 0
    push_entry(2, 10, 1); // obj 2 in objstm 10 idx 1
    push_entry(2, 10, 2); // obj 3 in objstm 10 idx 2
    push_entry(1, off4 as u32, 0); // obj 4
    push_entry(1, off5 as u32, 0); // obj 5
    push_entry(1, off6 as u32, 0); // obj 6
    push_entry(1, off7 as u32, 0); // obj 7 (/Info)
    push_entry(0, 0, 0); // obj 8 free
    push_entry(0, 0, 0); // obj 9 free
    push_entry(1, off10 as u32, 0); // obj 10 (/ObjStm)
    let xref_zlib = deflate_zlib(&xref_data);
    pdf.extend_from_slice(
        format!(
            "11 0 obj\n<< /Type /XRef /Size 11 /W [1 4 1] /Root 1 0 R /Info 7 0 R /Filter /FlateDecode /Length {} >>\nstream\n",
            xref_zlib.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(&xref_zlib);
    pdf.extend_from_slice(
        format!("\nendstream\nendobj\nstartxref\n{}\n%%EOF\n", xref_off).as_bytes(),
    );

    let unpacked_pdf = PdfSerializer::parse(&pdf).expect("unpack pdf 1.7 with XRef and ObjStm");
    assert_eq!(unpacked_pdf.title, "Fallback Info Title");
    assert_eq!(unpacked_pdf.pages.len(), 1);
    assert_eq!(unpacked_pdf.pages[0].body_text, "Hi AB!?");

    // 2. Split page approval_stamp only on first chunk
    let long_body = (0..60)
        .map(|i| format!("Line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let long_page_doc = PdfDocumentModel {
        title: "Doc With Stamp".to_string(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: long_body,
            approval_stamp: Some("APPROVED".to_string()),
        }],
        watermark_text: None,
    };
    let packed_long = PdfSerializer::serialize(&long_page_doc);
    // Strip the `%EASYOFFICE-PDF-JSON:` trailer comment so native stream parser reads the split page objects
    let stripped_long = {
        let s = String::from_utf8_lossy(&packed_long);
        let filtered: String = s
            .lines()
            .filter(|l| !l.starts_with("%EASYOFFICE-PDF-JSON:"))
            .collect::<Vec<_>>()
            .join("\n");
        filtered.into_bytes()
    };
    let unpacked_long = PdfSerializer::parse(&stripped_long).expect("parse stripped split pdf");
    assert!(unpacked_long.pages.len() >= 2);
    assert_eq!(
        unpacked_long.pages[0].approval_stamp.as_deref(),
        Some("APPROVED")
    );
    assert_eq!(unpacked_long.pages[1].approval_stamp, None);
}

#[test]
fn test_zip_and_deflate_security_and_rfc1951_guards() {
    let policy = ZipSecurityPolicy::default();

    // 1. Case-insensitive duplicate path rejection in ZIP
    let dup_entries = vec![
        ("word/document.xml".to_string(), b"a".to_vec()),
        ("WORD/DOCUMENT.XML".to_string(), b"b".to_vec()),
    ];
    assert!(matches!(
        ZipArchiveCodec::pack(&dup_entries),
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    // 2. File-vs-directory collision rejection in ZIP
    let collision_entries = vec![
        ("word/media".to_string(), b"file".to_vec()),
        ("word/media/image1.png".to_string(), b"nested".to_vec()),
    ];
    assert!(matches!(
        ZipArchiveCodec::pack(&collision_entries),
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    // 3. LFH vs CD uncompressed_size mismatch rejection in ZIP
    let mut valid_zip =
        ZipArchiveCodec::pack(&[("hello.txt".to_string(), b"Hello World".to_vec())])
            .expect("pack valid zip");
    valid_zip[22..26].copy_from_slice(&999u32.to_le_bytes());
    assert!(matches!(
        ZipArchiveCodec::unpack(&valid_zip, &policy),
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    // 4. Truncated zlib Adler-32 trailer rejected by inflate_zlib and inflate_auto
    let valid_zlib = deflate_zlib(b"Test Adler-32 trailer enforcement");
    let truncated_zlib = &valid_zlib[..valid_zlib.len() - 2];
    assert!(matches!(
        inflate_zlib(truncated_zlib, 1024),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
    assert!(matches!(
        inflate_auto(truncated_zlib, 1024),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    // 5. High-ratio all-zero payload succeeds when within max_output_len
    let zeroes = vec![0u8; 200_000];
    let compressed_zeroes = deflate_raw(&zeroes);
    assert!(compressed_zeroes.len() < 1500);
    let decompressed = inflate_raw(&compressed_zeroes, 250_000).expect("inflate zeroes");
    assert_eq!(decompressed.len(), 200_000);
}
