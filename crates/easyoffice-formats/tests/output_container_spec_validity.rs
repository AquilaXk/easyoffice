//! Structural validity checks for every container written by `UniversalFormatConverter::encode`.
//!
//! Each test unpacks the produced bytes and asserts that the parts and elements required by the
//! governing specification are present and that non-schema attributes are absent.

use easyoffice_formats::{
    CsvCodec, DocumentFormat, OoxmlCodec, OutputCompatibility, PdfDocumentModel, PdfPageModel,
    PdfSerializer, UniversalDocumentModel, UniversalFormatConverter, XlsxCachedValue,
    XlsxFormulaCache, ZipArchiveCodec, ZipArchiveEntry, ZipSecurityPolicy,
};

fn sample_model() -> UniversalDocumentModel {
    UniversalDocumentModel {
        title: "규격검증 문서".to_string(),
        paragraphs: vec!["첫 문단입니다.".to_string(), "둘째 문단".to_string()],
        tables: vec![vec![
            vec!["구분".to_string(), "금액".to_string()],
            vec!["인건비".to_string(), "500".to_string()],
        ]],
        sheet_cells: vec![
            (0, 0, "10".to_string()),
            (1, 0, "32".to_string()),
            (2, 0, "=A1+A2".to_string()),
            (0, 1, "품목".to_string()),
        ],
        slides: vec![
            (
                "첫 슬라이드".to_string(),
                vec!["핵심 1".to_string(), "핵심 2".to_string()],
                "발표자 노트 1".to_string(),
            ),
            (
                "둘째 슬라이드".to_string(),
                vec!["요약".to_string()],
                String::new(),
            ),
        ],
        official_agency: Some("행정안전부".to_string()),
    }
}

fn unpack(bytes: &[u8]) -> Vec<ZipArchiveEntry> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap()
}

fn part<'a>(entries: &'a [ZipArchiveEntry], path: &str) -> &'a str {
    let entry = entries
        .iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("필수 파트 누락: {path}"));
    std::str::from_utf8(&entry.data).unwrap()
}

fn has_part(entries: &[ZipArchiveEntry], path: &str) -> bool {
    entries.iter().any(|e| e.path == path)
}

/// Returns the opening tags (`<tag ...>`) for `tag` found in `xml`.
fn open_tags<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let needle = format!("<{tag}");
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(idx) = xml[pos..].find(&needle) {
        let start = pos + idx;
        let after = start + needle.len();
        let boundary = xml.as_bytes().get(after).copied().unwrap_or(b'>');
        if matches!(boundary, b' ' | b'>' | b'/') {
            let end = xml[start..].find('>').map(|e| start + e + 1).unwrap();
            out.push(&xml[start..end]);
            pos = end;
        } else {
            pos = after;
        }
    }
    out
}

fn assert_content_type_override(content_types: &str, part_name: &str) {
    assert!(
        content_types.contains(&format!("PartName=\"{part_name}\"")),
        "[Content_Types].xml에 {part_name} Override가 없습니다: {content_types}"
    );
}

// ---------------------------------------------------------------- XLSX

#[test]
fn xlsx_cells_carry_only_schema_attributes() {
    let bytes = UniversalFormatConverter::encode(&sample_model(), DocumentFormat::Xlsx).unwrap();
    let entries = unpack(&bytes);
    let sheet = part(&entries, "xl/worksheets/sheet1.xml");
    let cells = open_tags(sheet, "c");
    assert!(!cells.is_empty());
    for c in cells {
        assert!(!c.contains(" row="), "비표준 row 속성: {c}");
        assert!(!c.contains(" col="), "비표준 col 속성: {c}");
        assert!(c.contains(" r=\""), "r 속성 누락: {c}");
    }
}

#[test]
fn xlsx_workbook_requests_full_recalculation_and_never_caches_fake_zero() {
    let bytes = UniversalFormatConverter::encode(&sample_model(), DocumentFormat::Xlsx).unwrap();
    let entries = unpack(&bytes);
    let workbook = part(&entries, "xl/workbook.xml");
    let calc = open_tags(workbook, "calcPr");
    assert_eq!(calc.len(), 1, "calcPr 누락: {workbook}");
    assert!(calc[0].contains("fullCalcOnLoad=\"1\""));
    assert!(workbook.find("<calcPr").unwrap() > workbook.find("</sheets>").unwrap());

    let sheet = part(&entries, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<f>A1+A2</f>"));
    assert!(
        !sheet.contains("<f>A1+A2</f><v>0</v>"),
        "계산하지 않은 0을 캐시값으로 저장했습니다."
    );
}

#[test]
fn xlsx_formula_cache_values_match_computed_results() {
    let mut cache = XlsxFormulaCache::new();
    cache.insert((2, 0), XlsxCachedValue::Number(42.0));
    cache.insert((3, 0), XlsxCachedValue::Text("합격".to_string()));
    cache.insert((4, 0), XlsxCachedValue::Boolean(true));
    let cells = vec![
        (0, 0, "10".to_string()),
        (1, 0, "32".to_string()),
        (2, 0, "=A1+A2".to_string()),
        (3, 0, "=IF(A3>40,\"합격\",\"불합격\")".to_string()),
        (4, 0, "=A3=42".to_string()),
    ];
    let bytes = OoxmlCodec::pack_xlsx_multi_with_formula_cache(
        &[("시트".to_string(), cells.clone())],
        std::slice::from_ref(&cache),
    )
    .unwrap();
    let entries = unpack(&bytes);
    let sheet = part(&entries, "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains("<c r=\"A3\"><f>A1+A2</f><v>42</v></c>"),
        "{sheet}"
    );
    assert!(
        sheet.contains("<c r=\"A4\" t=\"str\"><f>IF(A3&gt;40,&quot;합격&quot;,&quot;불합격&quot;)</f><v>합격</v></c>"),
        "{sheet}"
    );
    assert!(
        sheet.contains("<c r=\"A5\" t=\"b\"><f>A3=42</f><v>1</v></c>"),
        "{sheet}"
    );

    let round_trip = OoxmlCodec::unpack_xlsx_multi(&bytes).unwrap();
    assert_eq!(round_trip[0].1, cells);
}

// ---------------------------------------------------------------- PPTX

#[test]
fn pptx_package_has_theme_master_color_map_and_matching_content_types() {
    let bytes = UniversalFormatConverter::encode(&sample_model(), DocumentFormat::Pptx).unwrap();
    let entries = unpack(&bytes);
    let content_types = part(&entries, "[Content_Types].xml");

    let theme = part(&entries, "ppt/theme/theme1.xml");
    assert!(theme.contains("<a:theme"));
    assert!(theme.contains("<a:clrScheme"));
    assert!(theme.contains("<a:fontScheme"));
    assert!(theme.contains("<a:fmtScheme"));
    assert_content_type_override(content_types, "/ppt/theme/theme1.xml");

    let master = part(&entries, "ppt/slideMasters/slideMaster1.xml");
    assert_eq!(open_tags(master, "p:clrMap").len(), 1, "p:clrMap 누락");
    let master_rels = part(&entries, "ppt/slideMasters/_rels/slideMaster1.xml.rels");
    assert!(master_rels.contains("relationships/theme\" Target=\"../theme/theme1.xml\""));

    let pres = part(&entries, "ppt/presentation.xml");
    let pres_open = open_tags(pres, "p:presentation")[0];
    assert!(
        !pres_open.contains(" title="),
        "비표준 title 속성: {pres_open}"
    );
    assert!(!pres_open.contains(" slideshow="), "비표준 slideshow 속성");
    assert!(!pres.contains("<p:sldIdLst count="), "비표준 count 속성");
    assert!(pres.contains("<p:notesSz "), "필수 p:notesSz 누락");

    // Every XML part in the package has a content type (Default xml covers generic parts,
    // but every PresentationML/DrawingML part must have its specific Override).
    for e in &entries {
        let specific = e.path.starts_with("ppt/slides/slide")
            || e.path.starts_with("ppt/slideMasters/slideMaster")
            || e.path.starts_with("ppt/slideLayouts/slideLayout")
            || e.path.starts_with("ppt/theme/theme")
            || e.path.starts_with("ppt/notesSlides/notesSlide")
            || e.path.starts_with("ppt/notesMasters/notesMaster")
            || e.path == "docProps/core.xml";
        if specific && e.path.ends_with(".xml") {
            assert_content_type_override(content_types, &format!("/{}", e.path));
        }
    }

    let core = part(&entries, "docProps/core.xml");
    assert!(core.contains("<dc:title>규격검증 문서</dc:title>"));
}

#[test]
fn pptx_slides_use_real_shapes_and_notes_slides() {
    let bytes = UniversalFormatConverter::encode(&sample_model(), DocumentFormat::Pptx).unwrap();
    let entries = unpack(&bytes);
    let slide = part(&entries, "ppt/slides/slide1.xml");
    let sld_open = open_tags(slide, "p:sld")[0];
    assert!(
        !sld_open.contains(" title="),
        "비표준 title 속성: {sld_open}"
    );
    assert!(
        !sld_open.contains(" notes="),
        "비표준 notes 속성: {sld_open}"
    );

    let sp_tree_start = slide.find("<p:spTree>").unwrap() + "<p:spTree>".len();
    assert!(slide[sp_tree_start..].starts_with("<p:nvGrpSpPr>"));
    assert!(slide.contains("<p:grpSpPr"));
    let sp_count = open_tags(slide, "p:sp").len();
    assert_eq!(sp_count, 2, "제목·본문 도형 2개가 필요합니다.");
    assert_eq!(open_tags(slide, "p:nvSpPr").len(), sp_count);
    assert_eq!(open_tags(slide, "p:spPr").len(), sp_count);
    assert_eq!(open_tags(slide, "p:txBody").len(), sp_count);
    assert_eq!(open_tags(slide, "a:bodyPr").len(), sp_count);
    assert!(slide.contains("<p:ph type=\"title\"/>"));
    assert!(slide.contains("첫 슬라이드"));

    let slide_rels = part(&entries, "ppt/slides/_rels/slide1.xml.rels");
    assert!(slide_rels.contains("../notesSlides/notesSlide1.xml"));
    let notes = part(&entries, "ppt/notesSlides/notesSlide1.xml");
    assert!(notes.contains("<p:notes"));
    assert!(notes.contains("발표자 노트 1"));
    assert!(has_part(&entries, "ppt/notesMasters/notesMaster1.xml"));
    let pres = part(&entries, "ppt/presentation.xml");
    assert!(pres.contains("<p:notesMasterIdLst>"));

    // Slide 2 has no notes, so no notes slide is written for it.
    assert!(!has_part(&entries, "ppt/notesSlides/notesSlide2.xml"));

    let decoded = OoxmlCodec::unpack_pptx(&bytes).unwrap();
    assert_eq!(decoded.title, "규격검증 문서");
    assert!(!decoded.is_slideshow);
    assert_eq!(decoded.slides, sample_model().slides);

    let ppsx = UniversalFormatConverter::encode(&sample_model(), DocumentFormat::Ppsx).unwrap();
    assert!(OoxmlCodec::unpack_pptx(&ppsx).unwrap().is_slideshow);
    assert_eq!(
        easyoffice_formats::FormatDetector::detect(&ppsx, None).unwrap(),
        DocumentFormat::Ppsx
    );
}

// ---------------------------------------------------------------- HWPX

#[test]
fn hwpx_package_lists_all_parts_and_stores_mimetype_first() {
    let bytes = UniversalFormatConverter::encode(&sample_model(), DocumentFormat::Hwpx).unwrap();
    // First local file header: name must be "mimetype", stored (method 0).
    assert_eq!(&bytes[0..4], b"PK\x03\x04");
    let method = u16::from_le_bytes([bytes[8], bytes[9]]);
    let name_len = u16::from_le_bytes([bytes[26], bytes[27]]) as usize;
    assert_eq!(method, 0);
    assert_eq!(&bytes[30..30 + name_len], b"mimetype");

    let entries = unpack(&bytes);
    assert_eq!(part(&entries, "mimetype"), "application/hwp+zip");
    let version = part(&entries, "version.xml");
    assert!(version.contains("HCFVersion"));
    let settings = part(&entries, "settings.xml");
    assert!(settings.contains("<ha:HWPApplicationSetting"));
    let manifest = part(&entries, "META-INF/manifest.xml");
    let hpf = part(&entries, "Contents/content.hpf");
    for listed in [
        "Contents/header.xml",
        "Contents/section0.xml",
        "settings.xml",
    ] {
        assert!(
            hpf.contains(&format!("href=\"{listed}\"")),
            "content.hpf 누락: {listed}"
        );
    }
    for listed in [
        "version.xml",
        "settings.xml",
        "Contents/content.hpf",
        "Contents/header.xml",
        "Contents/section0.xml",
    ] {
        assert!(
            manifest.contains(&format!("full-path=\"{listed}\"")),
            "manifest 누락: {listed}"
        );
    }
}

#[test]
fn hwpx_header_defines_fonts_shapes_and_styles_with_ids() {
    let bytes = UniversalFormatConverter::encode(&sample_model(), DocumentFormat::Hwpx).unwrap();
    let entries = unpack(&bytes);
    let header = part(&entries, "Contents/header.xml");
    let head_open = open_tags(header, "hh:head")[0];
    assert!(!head_open.ends_with("/>"), "header.xml이 비어 있습니다.");
    for attr in [
        " title=",
        " isTemplate=",
        " officialAgency=",
        " approvalLines=",
    ] {
        assert!(!head_open.contains(attr), "비표준 속성 {attr}: {head_open}");
    }
    assert!(header.contains("<hh:refList>"));
    assert!(header.contains("<hh:fontfaces"));
    assert!(header.contains("<hh:fontface lang=\"HANGUL\""));
    assert!(header.contains("<hh:charProperties"));
    assert!(header.contains("<hh:charPr id=\"0\""));
    assert!(header.contains("<hh:paraProperties"));
    assert!(header.contains("<hh:paraPr id=\"0\""));
    assert!(header.contains("<hh:styles"));
    assert!(header.contains("<hh:style id=\"0\""));
    assert!(header.contains("paraPrIDRef=\"0\" charPrIDRef=\"0\""));
}

#[test]
fn hwpx_section_has_sec_pr_id_refs_and_wrapped_tables() {
    let bytes = UniversalFormatConverter::encode(&sample_model(), DocumentFormat::Hwpx).unwrap();
    let entries = unpack(&bytes);
    let section = part(&entries, "Contents/section0.xml");

    let first_p_start = section.find("<hp:p ").expect("첫 문단 누락");
    let first_p_end = section[first_p_start..].find("</hp:p>").unwrap() + first_p_start;
    assert!(
        section[first_p_start..first_p_end].contains("<hp:secPr"),
        "secPr가 첫 문단에 없습니다."
    );

    for p in open_tags(section, "hp:p") {
        assert!(p.contains("paraPrIDRef=\""), "paraPrIDRef 누락: {p}");
    }
    for run in open_tags(section, "hp:run") {
        assert!(run.contains("charPrIDRef=\""), "charPrIDRef 누락: {run}");
    }

    // Tables live inside hp:p > hp:run, never directly under hs:sec.
    let tbl_pos = section.find("<hp:tbl").expect("표 누락");
    let before = &section[..tbl_pos];
    assert!(before.ends_with('>'));
    let last_open_run = before.rfind("<hp:run").unwrap();
    let last_close_run = before.rfind("</hp:run>").unwrap_or(0);
    assert!(
        last_open_run > last_close_run,
        "hp:tbl이 hp:run 안에 있지 않습니다."
    );
    assert!(
        !section.contains("<hp:tc><hp:t>"),
        "hp:t가 hp:tc 바로 아래에 있습니다."
    );
    assert!(section.contains("<hp:subList"));
    for tc in section.split("<hp:tc").skip(1) {
        let body = &tc[..tc.find("</hp:tc>").unwrap()];
        assert!(body.contains("<hp:subList"));
        assert!(body.contains("<hp:p "));
        assert!(body.contains("<hp:run charPrIDRef=\"0\"><hp:t>"));
    }

    let decoded = easyoffice_formats::HwpxCodec::unpack(&bytes).unwrap();
    assert_eq!(decoded.title, "규격검증 문서");
    assert_eq!(decoded.paragraphs, sample_model().paragraphs);
    assert_eq!(decoded.tables, sample_model().tables);
    assert_eq!(decoded.official_agency.as_deref(), Some("행정안전부"));

    let hwtx = UniversalFormatConverter::encode(&sample_model(), DocumentFormat::Hwtx).unwrap();
    assert!(
        easyoffice_formats::HwpxCodec::unpack(&hwtx)
            .unwrap()
            .is_template
    );
    assert_eq!(
        easyoffice_formats::FormatDetector::detect(&hwtx, None).unwrap(),
        DocumentFormat::Hwtx
    );
    assert_eq!(
        easyoffice_formats::FormatDetector::detect(&bytes, None).unwrap(),
        DocumentFormat::Hwpx
    );
}

#[test]
fn hwpx_empty_document_still_carries_section_properties() {
    let model = UniversalDocumentModel {
        title: "빈 문서".to_string(),
        ..Default::default()
    };
    let payload = easyoffice_formats::HwpxPayload {
        title: model.title.clone(),
        is_template: false,
        official_agency: None,
        approval_lines: Vec::new(),
        paragraphs: Vec::new(),
        tables: Vec::new(),
    };
    let bytes = easyoffice_formats::HwpxCodec::pack(&payload).unwrap();
    let entries = unpack(&bytes);
    assert!(part(&entries, "Contents/section0.xml").contains("<hp:secPr"));
    let decoded = easyoffice_formats::HwpxCodec::unpack(&bytes).unwrap();
    assert!(decoded.paragraphs.is_empty());
    assert!(decoded.tables.is_empty());
}

// ---------------------------------------------------------------- DOCX

#[test]
fn docx_document_has_no_title_attribute_and_tables_have_properties_and_grid() {
    let bytes = UniversalFormatConverter::encode(&sample_model(), DocumentFormat::Docx).unwrap();
    let entries = unpack(&bytes);
    let doc = part(&entries, "word/document.xml");
    let doc_open = open_tags(doc, "w:document")[0];
    assert!(
        !doc_open.contains(" title="),
        "비표준 title 속성: {doc_open}"
    );

    let tbl_start = doc.find("<w:tbl>").expect("표 누락");
    let after_tbl = &doc[tbl_start + "<w:tbl>".len()..];
    assert!(
        after_tbl.starts_with("<w:tblPr>"),
        "w:tblPr가 첫 자식이 아닙니다."
    );
    let grid_pos = after_tbl.find("<w:tblGrid>").expect("w:tblGrid 누락");
    let first_tr = after_tbl.find("<w:tr>").unwrap();
    assert!(grid_pos < first_tr);
    assert_eq!(open_tags(after_tbl, "w:gridCol").len(), 2);

    let core = part(&entries, "docProps/core.xml");
    assert!(core.contains("<dc:title>규격검증 문서</dc:title>"));
    let content_types = part(&entries, "[Content_Types].xml");
    assert_content_type_override(content_types, "/docProps/core.xml");
    let rels = part(&entries, "_rels/.rels");
    assert!(rels.contains("Target=\"docProps/core.xml\""));

    let decoded = OoxmlCodec::unpack_docx(&bytes).unwrap();
    assert_eq!(decoded.title, "규격검증 문서");
    assert_eq!(decoded.tables, sample_model().tables);
    assert_eq!(decoded.paragraphs, sample_model().paragraphs);
}

// ---------------------------------------------------------------- PDF

fn pdf_with_title(title: &str) -> Vec<u8> {
    PdfSerializer::serialize(&PdfDocumentModel {
        title: title.to_string(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: "한글 본문 Korean".to_string(),
            approval_stamp: None,
        }],
        watermark_text: None,
    })
}

#[test]
fn pdf_title_is_utf16be_with_bom_when_non_ascii() {
    let bytes = pdf_with_title("예산 보고서");
    let raw = String::from_utf8_lossy(&bytes);
    let expected_hex: String = std::iter::once(0xFEFFu16)
        .chain("예산 보고서".encode_utf16())
        .map(|u| format!("{u:04X}"))
        .collect();
    assert!(
        raw.contains(&format!("/Title <{expected_hex}>")),
        "UTF-16BE BOM 제목이 없습니다."
    );
    assert!(
        !raw.contains("/Title (예산"),
        "UTF-8 바이트 제목이 남아 있습니다."
    );
    assert_eq!(PdfSerializer::parse(&bytes).unwrap().title, "예산 보고서");

    let ascii = pdf_with_title("Budget (2026)");
    let ascii_raw = String::from_utf8_lossy(&ascii);
    assert!(ascii_raw.contains("/Title (Budget \\(2026\\))"));
    assert_eq!(PdfSerializer::parse(&ascii).unwrap().title, "Budget (2026)");
}

#[test]
fn pdf_korean_font_uses_resolvable_standard_cjk_reference() {
    let bytes = pdf_with_title("문서");
    let raw = String::from_utf8_lossy(&bytes);
    assert!(
        !raw.contains("EasyOfficeGothic"),
        "뷰어가 해석할 수 없는 자체 글꼴명"
    );
    assert!(raw.contains("/Encoding /UniKS-UCS2-H"));
    assert!(raw.contains("/Subtype /CIDFontType0"));
    assert!(raw.contains("/BaseFont /HYGoThic-Medium"));
    assert!(raw.contains("/Registry (Adobe) /Ordering (Korea1) /Supplement 1"));
    assert!(raw.contains("/Type /FontDescriptor"));
    assert!(raw.contains("/FontName /HYGoThic-Medium"));
    assert!(raw.contains("/ToUnicode"));
    let catalog_line = raw.lines().find(|l| l.contains("/Type /Catalog")).unwrap();
    assert!(
        !catalog_line.contains("/Title"),
        "카탈로그에는 /Title 키가 없습니다."
    );
    let parsed = PdfSerializer::parse(&bytes).unwrap();
    assert_eq!(parsed.pages[0].body_text, "한글 본문 Korean");
}

// ---------------------------------------------------------------- CSV

#[test]
fn csv_output_starts_with_utf8_bom_and_round_trips() {
    let bytes = UniversalFormatConverter::encode(&sample_model(), DocumentFormat::Csv).unwrap();
    assert!(
        bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "UTF-8 BOM이 없습니다."
    );
    let (_, decoded) = UniversalFormatConverter::decode(&bytes, Some("csv")).unwrap();
    assert!(decoded.sheet_cells.contains(&(0, 0, "10".to_string())));
    assert!(decoded.sheet_cells.contains(&(0, 1, "품목".to_string())));
    assert_eq!(decoded.title, "10");

    let rows = CsvCodec::parse("\u{FEFF}이름,값\r\n가,1", ',').unwrap();
    assert_eq!(rows[0][0], "이름");
    assert_eq!(
        CsvCodec::parse_scalar_baseline("\u{FEFF}이름,값", ',').unwrap()[0][0],
        "이름"
    );
}

// ---------------------------------------------------------------- DOC / PPT

#[test]
fn engine_hwp_writer_reports_limited_and_the_rest_report_standard() {
    for fmt in DocumentFormat::ALL {
        if matches!(fmt, DocumentFormat::Hwp | DocumentFormat::Hwt) {
            // The engine's HWP container has no DocInfo mappings, shapes or 22-byte PARA_HEADER, so
            // it is not an HWP 5.0 file other readers must accept (PD-178).
            let compatibility = fmt.output_compatibility();
            assert_eq!(compatibility.as_str(), "limited", "{fmt:?}");
            assert!(compatibility.warning().is_some_and(|w| !w.is_empty()));
            assert_ne!(fmt.output_specification(), fmt.standard_specification());
        } else {
            assert_eq!(fmt.output_compatibility(), OutputCompatibility::Standard);
            assert_eq!(fmt.output_specification(), fmt.standard_specification());
        }
    }
    assert!(OutputCompatibility::Standard.warning().is_none());
    assert_eq!(OutputCompatibility::Limited("사유").as_str(), "limited");
}
