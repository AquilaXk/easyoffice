//! PD-257 IPC & readback tests for coordinate-based slide decks (`PPTX`/`PPSX`/`ODP`/`PDF`/`DOCX`/`MD`).

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    deflate_zlib, DocumentFormat, IeeeCrc32, PdfSerializer, RichBlock, RichListKind,
    StrictJsonValue, UniversalFormatConverter,
};

fn field<'a>(obj: &'a [(String, StrictJsonValue)], key: &str) -> &'a StrictJsonValue {
    StrictJsonValue::get_field(obj, key).unwrap_or_else(|| panic!("missing JSON field '{key}'"))
}

fn png_1x1() -> String {
    let raw = [0u8, 37, 99, 235, 255];
    let comp = deflate_zlib(&raw);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let chunk = |kind: &[u8; 4], data: &[u8], out: &mut Vec<u8>| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = IeeeCrc32::update(IeeeCrc32::checksum(kind), data);
        out.extend_from_slice(&crc.to_be_bytes());
    };
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    chunk(b"IHDR", &ihdr, &mut out);
    chunk(b"IDAT", &comp, &mut out);
    chunk(b"IEND", &[], &mut out);
    EasyOfficeIpcRouter::encode_base64(&out)
}

fn rich_slides_json(target: &str) -> String {
    let img = png_1x1();
    format!(
        r#"{{
            "sourceFormat": "pptx",
            "targetFormat": "{target}",
            "title": "분기 리포트",
            "aspect": "4:3",
            "slides": [
                {{
                    "title": "개요 슬라이드",
                    "bullets": ["첫째 요약", "둘째 요약"],
                    "notes": "오프닝 발표 메모",
                    "layout": "titleSlide",
                    "transition": "fade",
                    "elements": [
                        {{"kind":"textBox","id":"tb1","x":40,"y":90,"w":260,"h":48,"text":"자유 상자","fontSizePt":18,"bold":true,"italic":false,"underline":true,"align":"center","color":"1E3A8A","fill":"EFF6FF","role":"subtitle"}},
                        {{"kind":"shape","id":"sh1","x":320,"y":90,"w":180,"h":60,"shapeType":"roundRect","text":"도형 알림","fill":"DBEAFE","stroke":"2563EB","strokeWidthPt":1.5,"color":"0F172A","fontSizePt":14,"bold":true,"italic":true,"underline":true,"align":"center"}},
                        {{"kind":"image","id":"im1","x":40,"y":170,"w":120,"h":80,"dataBase64":"{img}","alt":"브랜드 마크"}},
                        {{"kind":"table","id":"tbl1","x":180,"y":170,"w":240,"h":100,"headerRow":true,"fill":"1E40AF","rows":[["부서","매출"],["개발","500"]]}},
                        {{"kind":"chart","id":"ch1","x":440,"y":170,"w":240,"h":160,"chartType":"column","title":"성장 그래프","categories":["Q1","Q2"],"series":[{{"name":"실적","values":[80,115]}}],"legend":"bottom"}}
                    ]
                }},
                {{
                    "title": "결론",
                    "bullets": ["목표 달성"],
                    "notes": "질의응답 진행",
                    "layout": "twoColumn",
                    "transition": "zoom",
                    "elements": []
                }}
            ]
        }}"#
    )
}

#[test]
fn ipc_structured_slides_save_and_decode_roundtrip_pptx_ppsx_odp() {
    for ext in ["pptx", "ppsx", "odp"] {
        let save_resp =
            EasyOfficeIpcRouter::dispatch("convert_document_format", &rich_slides_json(ext))
                .unwrap();
        let parsed = StrictJsonValue::parse(&save_resp).unwrap();
        let obj = parsed.as_object().unwrap();
        let b64 = field(obj, "convertedBase64").as_str().unwrap();

        let dec_req =
            format!(r#"{{"action":"decode","fileName":"deck.{ext}","sourceBase64":"{b64}"}}"#);
        let dec_resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &dec_req).unwrap();
        let dec_val = StrictJsonValue::parse(&dec_resp).unwrap();
        let dec_obj = dec_val.as_object().unwrap();

        assert_eq!(field(dec_obj, "aspect").as_str(), Some("4:3"), "{ext}");
        let slides = field(dec_obj, "slides").as_array().unwrap();
        assert_eq!(slides.len(), 2, "{ext}");
        let s0 = slides[0].as_object().unwrap();
        assert_eq!(field(s0, "layout").as_str(), Some("titleSlide"), "{ext}");
        assert_eq!(field(s0, "transition").as_str(), Some("fade"), "{ext}");
        let els = field(s0, "elements").as_array().unwrap();
        assert_eq!(els.len(), 5, "{ext}");
        let kinds: Vec<_> = els
            .iter()
            .map(|e| field(e.as_object().unwrap(), "kind").as_str().unwrap())
            .collect();
        assert_eq!(
            kinds,
            ["textBox", "shape", "image", "table", "chart"],
            "{ext}"
        );
        let sh = els[1].as_object().unwrap();
        assert_eq!(field(sh, "bold").as_bool(), Some(true), "{ext}");
        assert_eq!(field(sh, "italic").as_bool(), Some(true), "{ext}");
        assert_eq!(field(sh, "underline").as_bool(), Some(true), "{ext}");

        let s1 = slides[1].as_object().unwrap();
        assert_eq!(field(s1, "layout").as_str(), Some("twoColumn"), "{ext}");
        assert_eq!(field(s1, "transition").as_str(), Some("zoom"), "{ext}");
    }
}

#[test]
fn ipc_structured_slides_to_pdf_produces_multipage_vector_pdf() {
    let resp =
        EasyOfficeIpcRouter::dispatch("convert_document_format", &rich_slides_json("pdf")).unwrap();
    let val = StrictJsonValue::parse(&resp).unwrap();
    let pdf_b64 = field(val.as_object().unwrap(), "convertedBase64")
        .as_str()
        .unwrap();
    let pdf_bytes = EasyOfficeIpcRouter::decode_base64(pdf_b64).unwrap();

    let report = PdfSerializer::inspect_readback(&pdf_bytes).unwrap();
    assert_eq!(report.pages.len(), 2);
    assert!((report.pages[0].width_pt - 720.0).abs() < 0.5);
    assert!((report.pages[0].height_pt - 540.0).abs() < 0.5);

    let parsed = PdfSerializer::parse(&pdf_bytes).unwrap();
    let text = format!(
        "{}\n{}",
        parsed.pages[0].body_text, parsed.pages[1].body_text
    );
    assert!(text.contains("개요 슬라이드"), "{text}");
    assert!(text.contains("성장 그래프"), "{text}");
    assert!(text.contains("결론"), "{text}");
}

#[test]
fn ipc_structured_slides_to_docx_and_markdown_preserves_rich_elements_and_notes() {
    let docx_resp =
        EasyOfficeIpcRouter::dispatch("convert_document_format", &rich_slides_json("docx"))
            .unwrap();
    let docx_b64 = field(
        StrictJsonValue::parse(&docx_resp)
            .unwrap()
            .as_object()
            .unwrap(),
        "convertedBase64",
    )
    .as_str()
    .unwrap()
    .to_string();
    let docx = EasyOfficeIpcRouter::decode_base64(&docx_b64).unwrap();
    let dec = UniversalFormatConverter::decode_rich(&docx, Some("a.docx")).unwrap();
    assert_eq!(dec.format, DocumentFormat::Docx);
    let easyoffice_formats::DecodedContent::Document(doc) = dec.content else {
        panic!("expected document");
    };
    assert!(doc.blocks.iter().any(|b| matches!(b, RichBlock::Table(_))));
    assert!(doc
        .blocks
        .iter()
        .any(|b| matches!(b, RichBlock::Paragraph(p) if p.list == Some(RichListKind::Bullet))));
    assert!(doc.blocks.iter().any(|b| matches!(b, RichBlock::Paragraph(p) if p.plain_text() == "오프닝 발표 메모" && p.runs.iter().all(|r| r.italic))));

    let md_resp =
        EasyOfficeIpcRouter::dispatch("convert_document_format", &rich_slides_json("md")).unwrap();
    let md_b64 = field(
        StrictJsonValue::parse(&md_resp)
            .unwrap()
            .as_object()
            .unwrap(),
        "convertedBase64",
    )
    .as_str()
    .unwrap()
    .to_string();
    let md = String::from_utf8(EasyOfficeIpcRouter::decode_base64(&md_b64).unwrap()).unwrap();
    assert!(
        md.contains("## 개요 슬라이드")
            && md.contains("자유 상자")
            && md.contains("오프닝 발표 메모"),
        "{md}"
    );
}

#[test]
fn ipc_binary_pptx_to_odp_conversion_preserves_coordinate_elements() {
    let pptx_resp =
        EasyOfficeIpcRouter::dispatch("convert_document_format", &rich_slides_json("pptx"))
            .unwrap();
    let pptx_b64 = field(
        StrictJsonValue::parse(&pptx_resp)
            .unwrap()
            .as_object()
            .unwrap(),
        "convertedBase64",
    )
    .as_str()
    .unwrap()
    .to_string();
    let conv_req = format!(
        r#"{{"action":"convert","fileName":"in.pptx","sourceBase64":"{pptx_b64}","targetFormat":"odp"}}"#
    );
    let odp_resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &conv_req).unwrap();
    let odp_b64 = field(
        StrictJsonValue::parse(&odp_resp)
            .unwrap()
            .as_object()
            .unwrap(),
        "convertedBase64",
    )
    .as_str()
    .unwrap()
    .to_string();
    let odp_bytes = EasyOfficeIpcRouter::decode_base64(&odp_b64).unwrap();
    let deck = easyoffice_formats::OdfCodec::unpack_odp_rich(&odp_bytes).unwrap();
    assert_eq!(deck.aspect, easyoffice_formats::SlideAspect::Standard4x3);
    assert_eq!(deck.slides[0].elements.len(), 5);
}

#[test]
fn ipc_inspect_readback_reports_aspect_elements_and_missing_alt() {
    let img = png_1x1();
    let req = format!(
        r#"{{"action":"inspect","sourceFormat":"pptx","aspect":"A4","slides":[{{"title":"","bullets":[],"notes":"","layout":"blank","transition":"none","elements":[{{"kind":"image","id":"im-noalt","x":20,"y":20,"w":80,"h":80,"dataBase64":"{img}","alt":""}}]}}]}}"#
    );
    let resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &req).unwrap();
    let val = StrictJsonValue::parse(&resp).unwrap();
    let obj = val.as_object().unwrap();
    assert_eq!(field(obj, "aspect").as_str(), Some("A4"));
    assert_eq!(field(obj, "slideCount").as_u64(), Some(1));
    let checks = field(obj, "checks").as_object().unwrap();
    assert_eq!(field(checks, "missingAltImageCount").as_u64(), Some(1));
    assert_eq!(field(checks, "emptySlides").as_array().unwrap().len(), 0);
}

#[test]
fn ipc_rejects_invalid_and_oversized_slide_payloads_fail_closed() {
    let bad_coord = r#"{"sourceFormat":"pptx","targetFormat":"pptx","slides":[{"title":"x","elements":[{"kind":"shape","x":4001,"y":10,"w":50,"h":50}]}]}"#;
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", bad_coord).is_err());

    let bad_hex = r##"{"sourceFormat":"pptx","targetFormat":"pptx","slides":[{"title":"x","elements":[{"kind":"textBox","x":10,"y":10,"w":100,"h":40,"color":"ZZZZZZ"}]}]}"##;
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", bad_hex).is_err());
}

#[test]
fn ipc_webui_wire_schema_data_and_lowercase_textbox_roundtrips_cleanly() {
    let img = png_1x1();
    let webui_save = format!(
        r#"{{"sourceFormat":"pptx","targetFormat":"pptx","title":"웹UI_저장","aspect":"16:9","slides":[{{"title":"표지","bullets":["요약"],"notes":"노트","layout":"titleAndContent","transition":"fade","elements":[{{"id":"el-1","kind":"textbox","x":80,"y":140,"w":320,"h":80,"text":"웹UI 글상자","fontSizePt":18,"bold":true,"italic":false,"underline":false,"align":"justify","color":"1F2937","fill":"","role":null}},{{"id":"el-2","kind":"image","x":440,"y":140,"w":240,"h":160,"mime":"image/png","data":"{img}","alt":"웹UI 그림"}}]}}]}}"#
    );
    let resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &webui_save).unwrap();
    let b64 = field(
        StrictJsonValue::parse(&resp).unwrap().as_object().unwrap(),
        "convertedBase64",
    )
    .as_str()
    .unwrap()
    .to_string();
    let dec_req =
        format!(r#"{{"action":"decode","fileName":"webui.pptx","sourceBase64":"{b64}"}}"#);
    let dec_resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &dec_req).unwrap();
    let dec_val = StrictJsonValue::parse(&dec_resp).unwrap();
    let slides = field(dec_val.as_object().unwrap(), "slides")
        .as_array()
        .unwrap();
    let els = field(slides[0].as_object().unwrap(), "elements")
        .as_array()
        .unwrap();
    assert_eq!(els.len(), 2);
}
