//! PD-263: opening a workbook whose chart or pivot table the reader turned away (an unsafe target, a
//! missing part) says so in the open response, like the pictures and notes it already counts.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::ZipArchiveCodec;

const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const OD_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn workbook_with_rejected_chart_and_pivot() -> Vec<u8> {
    let parts: Vec<(&str, String)> = vec![
        ("xl/workbook.xml", format!(r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="{OD_REL}"><sheets><sheet name="Report" sheetId="1" r:id="rId1"/></sheets></workbook>"#)),
        ("xl/_rels/workbook.xml.rels", format!(r#"<?xml version="1.0"?><Relationships xmlns="{REL_NS}"><Relationship Id="rId1" Type="{OD_REL}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#)),
        ("xl/worksheets/sheet1.xml", format!(r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="{OD_REL}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Item</t></is></c></row></sheetData><drawing r:id="rIdDraw1"/></worksheet>"#)),
        ("xl/worksheets/_rels/sheet1.xml.rels", format!(r#"<?xml version="1.0"?><Relationships xmlns="{REL_NS}"><Relationship Id="rIdDraw1" Type="{OD_REL}/drawing" Target="/xl/drawings/drawing1.xml"/><Relationship Id="rIdPT" Type="{OD_REL}/pivotTable" Target="../pivotTables/missing.xml"/></Relationships>"#)),
        ("xl/drawings/drawing1.xml", r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><xdr:twoCellAnchor><xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>8</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:graphicFrame><xdr:nvGraphicFramePr><xdr:cNvPr id="2" name="Bad"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rIdBad"/></a:graphicData></xdr:graphic></xdr:graphicFrame></xdr:twoCellAnchor></xdr:wsDr>"#.to_string()),
        ("xl/drawings/_rels/drawing1.xml.rels", format!(r#"<?xml version="1.0"?><Relationships xmlns="{REL_NS}"><Relationship Id="rIdBad" Type="{OD_REL}/chart" Target="../../../etc/passwd"/></Relationships>"#)),
    ];
    let entries: Vec<(String, Vec<u8>)> = parts
        .into_iter()
        .map(|(path, xml)| (path.to_string(), xml.into_bytes()))
        .collect();
    ZipArchiveCodec::pack_compressed(&entries).expect("pack")
}

#[test]
fn open_warns_about_rejected_charts_and_pivot_tables() {
    let b64 = EasyOfficeIpcRouter::encode_base64(&workbook_with_rejected_chart_and_pivot());
    let req = format!(r#"{{"action":"decode","fileName":"r.xlsx","sourceBase64":"{b64}"}}"#);
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &req).expect("decode");
    let start = json.find("\"warnings\":[").expect("warnings") + 12;
    let warnings = &json[start..json[start..].find(']').unwrap() + start];
    assert!(
        warnings
            .contains("Report 시트의 차트 1개는 한도를 넘거나 올바르지 않아 가져오지 않았습니다."),
        "{warnings}"
    );
    assert!(
        warnings.contains(
            "Report 시트의 피벗 테이블 1개는 한도를 넘거나 올바르지 않아 가져오지 않았습니다."
        ),
        "{warnings}"
    );
}
