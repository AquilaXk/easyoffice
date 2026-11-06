//! ZIP-based writers must emit DEFLATE entries (except the ones that must stay stored) and the
//! compressed containers must round-trip through the repo decoders (#124).

use easyoffice_formats::{
    DocumentFormat, UniversalDocumentModel, UniversalFormatConverter, ZipArchiveCodec,
    ZipSecurityPolicy,
};

struct CdEntry {
    name: String,
    method: u16,
    comp: u32,
    uncomp: u32,
    local_offset: u32,
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Central directory entries sorted by physical (local header) order.
fn directory(bytes: &[u8]) -> Vec<CdEntry> {
    let eocd = bytes.len() - 22;
    assert_eq!(u32_at(bytes, eocd), 0x0605_4B50);
    let count = u16_at(bytes, eocd + 10) as usize;
    let mut pos = u32_at(bytes, eocd + 16) as usize;
    let mut out = Vec::new();
    for _ in 0..count {
        assert_eq!(u32_at(bytes, pos), 0x0201_4B50);
        let name_len = u16_at(bytes, pos + 28) as usize;
        let extra = u16_at(bytes, pos + 30) as usize;
        let comment = u16_at(bytes, pos + 32) as usize;
        out.push(CdEntry {
            name: String::from_utf8(bytes[pos + 46..pos + 46 + name_len].to_vec()).unwrap(),
            method: u16_at(bytes, pos + 10),
            comp: u32_at(bytes, pos + 20),
            uncomp: u32_at(bytes, pos + 24),
            local_offset: u32_at(bytes, pos + 42),
        });
        pos += 46 + name_len + extra + comment;
    }
    out.sort_by_key(|e| e.local_offset);
    out
}

fn big_text_model(paragraphs: usize) -> UniversalDocumentModel {
    let words = [
        "기획조정실",
        "예산집행",
        "디지털",
        "행정혁신",
        "report",
        "budget",
        "분기별",
        "성과",
        "표준",
        "문서",
        "Quarterly",
        "alignment",
        "검토",
        "승인",
        "2026",
        "plan",
    ];
    let mut seed = 0x2545_F491u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as usize
    };
    let paragraphs: Vec<String> = (0..paragraphs)
        .map(|i| {
            let mut s = format!("{}. ", i + 1);
            for _ in 0..8 + next() % 6 {
                s.push_str(words[next() % words.len()]);
                s.push(' ');
            }
            s
        })
        .collect();
    UniversalDocumentModel {
        title: "대용량 문서".to_string(),
        paragraphs,
        ..Default::default()
    }
}

fn small_model() -> UniversalDocumentModel {
    let paragraphs: Vec<String> = (0..60)
        .map(|i| format!("문단 {i}: 압축 왕복 검증용 반복 문장입니다. EasyOffice round trip."))
        .collect();
    UniversalDocumentModel {
        title: "왕복".to_string(),
        paragraphs,
        tables: vec![vec![
            vec!["구분".to_string(), "금액".to_string()],
            vec!["인건비".to_string(), "500".to_string()],
        ]],
        sheet_cells: (0..200u32)
            .map(|r| (r, 0, format!("항목 {r}")))
            .chain((0..200u32).map(|r| (r, 1, format!("{}", r * 7))))
            .collect(),
        slides: (0..20)
            .map(|i| {
                (
                    format!("슬라이드 {i}"),
                    vec!["핵심 요약 문장 반복".to_string(); 5],
                    "노트".to_string(),
                )
            })
            .collect(),
        official_agency: Some("행정안전부".to_string()),
    }
}

const ZIP_FORMATS: [DocumentFormat; 9] = [
    DocumentFormat::Docx,
    DocumentFormat::Xlsx,
    DocumentFormat::Pptx,
    DocumentFormat::Ppsx,
    DocumentFormat::Odt,
    DocumentFormat::Ods,
    DocumentFormat::Odp,
    DocumentFormat::Hwpx,
    DocumentFormat::Hwtx,
];

#[test]
fn one_megabyte_docx_and_hwpx_are_compressed_well_below_half_a_megabyte() {
    let model = big_text_model(9_700);
    let text_bytes: usize = model.paragraphs.iter().map(String::len).sum();
    assert!(text_bytes > 700_000, "workload too small: {text_bytes}");

    let docx = UniversalFormatConverter::encode(&model, DocumentFormat::Docx).unwrap();
    let hwpx = UniversalFormatConverter::encode(&model, DocumentFormat::Hwpx).unwrap();
    assert!(docx.len() < 500_000, "docx {} bytes", docx.len());
    assert!(hwpx.len() < 500_000, "hwpx {} bytes", hwpx.len());

    for (bytes, format) in [(&docx, DocumentFormat::Docx), (&hwpx, DocumentFormat::Hwpx)] {
        let (detected, decoded) =
            UniversalFormatConverter::decode(bytes, Some(format.extension())).unwrap();
        assert_eq!(detected, format);
        assert_eq!(decoded.paragraphs, model.paragraphs);
    }
}

#[test]
fn every_zip_format_deflates_large_parts_and_round_trips() {
    let model = small_model();
    for format in ZIP_FORMATS {
        let bytes = UniversalFormatConverter::encode(&model, format).unwrap();
        let entries = directory(&bytes);
        let mut deflated = 0;
        for e in &entries {
            if e.name == "mimetype" {
                continue;
            }
            if e.uncomp >= 2_000 {
                assert_eq!(e.method, 8, "{format:?} {} must be deflated", e.name);
                assert!(e.comp < e.uncomp, "{format:?} {}", e.name);
            }
            if e.method == 8 {
                deflated += 1;
            }
        }
        assert!(deflated > 0, "{format:?} has no deflated entry");

        // Repo decoder verifies CRC-32 and sizes for every entry.
        let unpacked = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
        assert_eq!(unpacked.len(), entries.len(), "{format:?}");
        let (detected, _) =
            UniversalFormatConverter::decode(&bytes, Some(format.extension())).unwrap();
        assert_eq!(detected, format);
    }
}

#[test]
fn odf_and_hwpx_mimetype_is_first_and_stored() {
    let model = small_model();
    for format in [
        DocumentFormat::Odt,
        DocumentFormat::Ods,
        DocumentFormat::Odp,
        DocumentFormat::Hwpx,
        DocumentFormat::Hwtx,
    ] {
        let bytes = UniversalFormatConverter::encode(&model, format).unwrap();
        let entries = directory(&bytes);
        let first = &entries[0];
        assert_eq!(first.name, "mimetype", "{format:?}");
        assert_eq!(first.method, 0, "{format:?}");
        assert_eq!(first.comp, first.uncomp, "{format:?}");
        assert_eq!(first.local_offset, 0, "{format:?}");
        // Local header name sits at fixed offset 30 for the first entry (ODF sniffing rule).
        assert_eq!(&bytes[30..38], b"mimetype");
    }
}

#[test]
fn store_policy_is_explicit_per_entry() {
    assert!(ZipArchiveCodec::must_store("mimetype"));
    for p in [
        "Pictures/a.PNG",
        "word/media/image1.jpg",
        "media/x.jpeg",
        "a/b.gif",
        "a/b.zip",
        "BinData/image1.webp",
    ] {
        assert!(ZipArchiveCodec::must_store(p), "{p}");
    }
    for p in [
        "word/document.xml",
        "Contents/section0.xml",
        "xl/worksheets/sheet1.xml",
        "[Content_Types].xml",
        "content.xml",
        "docs/mimetype.xml",
    ] {
        assert!(!ZipArchiveCodec::must_store(p), "{p}");
    }

    let payload: Vec<u8> = b"<a>repeat repeat repeat repeat</a>".repeat(200);
    let bytes = ZipArchiveCodec::pack_compressed(&[
        ("mimetype".to_string(), b"application/x".repeat(20)),
        ("word/document.xml".to_string(), payload.clone()),
        ("media/pic.png".to_string(), payload.clone()),
    ])
    .unwrap();
    let methods: Vec<(String, u16)> = directory(&bytes)
        .into_iter()
        .map(|e| (e.name, e.method))
        .collect();
    assert_eq!(
        methods,
        vec![
            ("mimetype".to_string(), 0),
            ("word/document.xml".to_string(), 8),
            ("media/pic.png".to_string(), 0),
        ]
    );
    let unpacked = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    assert_eq!(unpacked[1].data, payload);
}

/// Manual interop check: `EO_DUMP_DIR=<dir> cargo test -- --ignored dump_zip_samples`, then open
/// the files with `System.IO.Compression.ZipFile` / `Expand-Archive`.
#[test]
#[ignore]
fn dump_zip_samples() {
    let Ok(dir) = std::env::var("EO_DUMP_DIR") else {
        return;
    };
    let model = big_text_model(9_700);
    let small = small_model();
    for format in ZIP_FORMATS {
        for (tag, m) in [("big", &model), ("small", &small)] {
            let bytes = UniversalFormatConverter::encode(m, format).unwrap();
            std::fs::write(format!("{dir}/{tag}.{}", format.extension()), bytes).unwrap();
        }
    }
}
