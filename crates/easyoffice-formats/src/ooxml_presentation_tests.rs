//! Reading tests of `ooxml_presentation.rs` (table cells of a slide, text of a notes slide), kept in their own file so the reader
//! stays under the line ceiling.

use crate::ooxml_presentation::{extract_notes_xml_text, PML_NS};
use crate::{OoxmlCodec, ZipArchiveCodec};

#[test]
fn pptx_slide_extracts_graphic_frame_table_cells() {
    let slide_xml = concat!(
        "<p:sld><p:cSld><p:spTree>",
        "<p:sp><p:nvSpPr><p:nvPr><p:ph type=\"title\"/></p:nvPr></p:nvSpPr>",
        "<p:txBody><a:p><a:r><a:t>분기 실적</a:t></a:r></a:p></p:txBody></p:sp>",
        "<p:sp><p:nvSpPr><p:nvPr><p:ph idx=\"1\"/></p:nvPr></p:nvSpPr>",
        "<p:txBody><a:p><a:r><a:t>요약 항목</a:t></a:r></a:p></p:txBody></p:sp>",
        "<p:graphicFrame><a:graphic><a:graphicData><a:tbl>",
        "<a:tr><a:tc><a:txBody><a:p><a:r><a:t>항목</a:t></a:r></a:p></a:txBody></a:tc>",
        "<a:tc><a:txBody><a:p><a:r><a:t>매출</a:t></a:r></a:p></a:txBody></a:tc></a:tr>",
        "<a:tr><a:tc><a:txBody><a:p><a:r><a:t>1분기</a:t></a:r></a:p></a:txBody></a:tc>",
        "<a:tc><a:txBody><a:p><a:r><a:t>1200</a:t></a:r></a:p></a:txBody></a:tc></a:tr>",
        "</a:tbl></a:graphicData></a:graphic></p:graphicFrame>",
        "</p:spTree></p:cSld></p:sld>"
    );
    let bytes = ZipArchiveCodec::pack(&[
        (
            "[Content_Types].xml".into(),
            b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>"
                .to_vec(),
        ),
        (
            "ppt/presentation.xml".into(),
            format!("<p:presentation {PML_NS}><p:sldIdLst/></p:presentation>").into_bytes(),
        ),
        (
            "ppt/slides/slide1.xml".into(),
            slide_xml.as_bytes().to_vec(),
        ),
    ])
    .unwrap();
    let (title, bullets, _) = &OoxmlCodec::unpack_pptx(&bytes).unwrap().slides[0];
    assert_eq!(title, "분기 실적");
    assert_eq!(
        bullets,
        &vec![
            "요약 항목".to_string(),
            "항목\t매출".to_string(),
            "1분기\t1200".to_string()
        ]
    );
}

#[test]
fn pptx_notes_slide_includes_body_and_custom_shapes_excluding_chrome() {
    let notes_xml = concat!(
        "<p:notes><p:cSld><p:spTree>",
        "<p:sp><p:nvSpPr><p:nvPr><p:ph type=\"sldImg\"/></p:nvPr></p:nvSpPr>",
        "<p:txBody><a:p><a:r><a:t>슬라이드 썸네일</a:t></a:r></a:p></p:txBody></p:sp>",
        "<p:sp><p:nvSpPr><p:nvPr><p:ph type=\"body\" idx=\"1\"/></p:nvPr></p:nvSpPr>",
        "<p:txBody><a:p><a:r><a:t>발표자 핵심 메모</a:t></a:r></a:p></p:txBody></p:sp>",
        "<p:sp><p:nvSpPr><p:nvPr/></p:nvSpPr>",
        "<p:txBody><a:p><a:r><a:t>추가 자유 텍스트 상자</a:t></a:r></a:p></p:txBody></p:sp>",
        "<p:sp><p:nvSpPr><p:nvPr><p:ph type=\"sldNum\" idx=\"10\"/></p:nvPr></p:nvSpPr>",
        "<p:txBody><a:p><a:r><a:t>3</a:t></a:r></a:p></p:txBody></p:sp>",
        "</p:spTree></p:cSld></p:notes>"
    );
    assert_eq!(
        extract_notes_xml_text(notes_xml).as_deref(),
        Some("발표자 핵심 메모\n추가 자유 텍스트 상자")
    );
}
