//! PD-324: PPTX 0-width/negative-bleed SlideBox, DrawingML theme colors & transforms,
//! p:style refs, nested p:grpSp + p:cxnSp connectors, non-body placeholder filtering,
//! TwoColumn round-trip, and endParaRPr bold/color pollution fixes.

use easyoffice_formats::{
    deflate_zlib, encode_slide_pdf, pt_to_signed_emu, EasyOfficeError, IeeeCrc32, OoxmlCodec,
    PdfSerializer, RichImage, RichSlide, RichSlideDeck, SlideAspect, SlideBox, SlideChartSeries,
    SlideElement, SlideMasterLayout, SlideOrientation, SlideShapeType, SlideTransition,
    ZipArchiveCodec, ZipSecurityPolicy,
};

const PML_NS: &str = concat!(
    "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
    "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
    "xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\""
);

const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const REL_BASE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn rels(items: &[(&str, &str, &str)]) -> Vec<u8> {
    let mut s = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{REL_NS}\">");
    for (id, kind, target) in items {
        s.push_str(&format!(
            "<Relationship Id=\"{id}\" Type=\"{REL_BASE}/{kind}\" Target=\"{target}\"/>"
        ));
    }
    s.push_str("</Relationships>");
    s.into_bytes()
}

fn sp_tree_hdr() -> &'static str {
    concat!(
        "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>",
        "<p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/>",
        "<a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>"
    )
}

fn make_png_1x1() -> Vec<u8> {
    let chunk = |kind: &[u8; 4], data: &[u8]| -> Vec<u8> {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = IeeeCrc32::update(IeeeCrc32::checksum(kind), data);
        out.extend_from_slice(&crc.to_be_bytes());
        out
    };
    let ihdr = [0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0];
    let raw = [0u8, 0x10, 0x40, 0x90];
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    out.extend_from_slice(&chunk(b"IHDR", &ihdr));
    out.extend_from_slice(&chunk(b"IDAT", &deflate_zlib(&raw)));
    out.extend_from_slice(&chunk(b"IEND", &[]));
    out
}

fn minimal_pptx(slides: &[(&str, Option<&str>)], extra_entries: Vec<(String, Vec<u8>)>) -> Vec<u8> {
    let mut ct = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/>",
    );
    let mut sld_ids = String::new();
    let mut pres_rels: Vec<(String, String, String)> = vec![(
        "rIdM".into(),
        format!("{REL_BASE}/slideMaster"),
        "slideMasters/slideMaster1.xml".into(),
    )];
    let mut entries = vec![];
    for (idx, (s_xml, s_rels_xml)) in slides.iter().enumerate() {
        let n = idx + 1;
        ct.push_str(&format!(
            "<Override PartName=\"/ppt/slides/slide{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"
        ));
        sld_ids.push_str(&format!("<p:sldId id=\"{}\" r:id=\"rId{n}\"/>", 255 + n));
        pres_rels.push((
            format!("rId{n}"),
            format!("{REL_BASE}/slide"),
            format!("slides/slide{n}.xml"),
        ));
        entries.push((
            format!("ppt/slides/slide{n}.xml"),
            s_xml.as_bytes().to_vec(),
        ));
        if let Some(rx) = s_rels_xml {
            entries.push((
                format!("ppt/slides/_rels/slide{n}.xml.rels"),
                rx.as_bytes().to_vec(),
            ));
        }
    }
    ct.push_str("</Types>");
    entries.push(("[Content_Types].xml".into(), ct.into_bytes()));
    entries.push((
        "_rels/.rels".into(),
        rels(&[("rId1", "officeDocument", "ppt/presentation.xml")]),
    ));
    entries.push((
        "ppt/presentation.xml".into(),
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:presentation {PML_NS}><p:sldIdLst>{sld_ids}</p:sldIdLst><p:sldSz cx=\"12192000\" cy=\"6858000\"/></p:presentation>"
        )
        .into_bytes(),
    ));
    let mut pr =
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"{REL_NS}\">");
    for (id, ty, target) in pres_rels {
        pr.push_str(&format!(
            "<Relationship Id=\"{id}\" Type=\"{ty}\" Target=\"{target}\"/>"
        ));
    }
    pr.push_str("</Relationships>");
    entries.push(("ppt/_rels/presentation.xml.rels".into(), pr.into_bytes()));
    entries.extend(extra_entries);
    ZipArchiveCodec::pack(&entries).expect("pack test zip")
}

#[test]
fn connectors_zero_dimensions_and_negative_bleed_roundtrip_and_pdf() {
    let neg_x = -18 * 12_700;
    let neg_y = -12 * 12_700;
    let slide1 = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sld {PML_NS}><p:cSld><p:spTree>{}\
        <p:cxnSp><p:nvCxnSpPr><p:cNvPr id=\"2\" name=\"vline\"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr>\
        <p:spPr><a:xfrm><a:off x=\"{neg_x}\" y=\"254000\"/><a:ext cx=\"0\" cy=\"2540000\"/></a:xfrm>\
        <a:prstGeom prst=\"line\"><a:avLst/></a:prstGeom>\
        <a:ln w=\"19050\"><a:solidFill><a:srgbClr val=\"1D4ED8\"/></a:solidFill></a:ln></p:spPr></p:cxnSp>\
        <p:cxnSp><p:nvCxnSpPr><p:cNvPr id=\"3\" name=\"harrow\"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr>\
        <p:spPr><a:xfrm><a:off x=\"508000\" y=\"{neg_y}\"/><a:ext cx=\"3810000\" cy=\"0\"/></a:xfrm>\
        <a:prstGeom prst=\"line\"><a:avLst/></a:prstGeom><a:ln w=\"25400\"><a:tailEnd type=\"triangle\"/></a:ln></p:spPr>\
        <p:style><a:lnRef idx=\"1\"><a:schemeClr val=\"accent1\"/></a:lnRef><a:fillRef idx=\"0\"><a:schemeClr val=\"accent1\"/></a:fillRef></p:style></p:cxnSp>\
        </p:spTree></p:cSld></p:sld>",
        sp_tree_hdr()
    );
    let bytes = minimal_pptx(&[(&slide1, None)], vec![]);
    let deck = OoxmlCodec::unpack_pptx_rich(&bytes).expect("unpack 0-dim cxnSp + negative bleed");
    assert_eq!(deck.slides[0].elements.len(), 2);

    match &deck.slides[0].elements[0] {
        SlideElement::Shape {
            id,
            rect,
            shape_type,
            fill,
            stroke,
            stroke_width_pt,
            ..
        } => {
            assert_eq!(id, "vline");
            assert_eq!(*shape_type, SlideShapeType::Rect);
            assert!((rect.x - (-18.0)).abs() < 0.01);
            assert!((rect.y - 20.0).abs() < 0.01);
            assert_eq!(rect.w, 0.0);
            assert!((rect.h - 200.0).abs() < 0.01);
            assert_eq!(fill.as_deref(), None);
            assert_eq!(stroke.as_deref(), Some("1D4ED8"));
            assert!((*stroke_width_pt - 1.5).abs() < 0.01);
        }
        other => panic!("expected vertical line shape, got {other:?}"),
    }
    match &deck.slides[0].elements[1] {
        SlideElement::Shape {
            id,
            rect,
            shape_type,
            fill,
            stroke,
            stroke_width_pt,
            ..
        } => {
            assert_eq!(id, "harrow");
            assert_eq!(*shape_type, SlideShapeType::RightArrow);
            assert!((rect.x - 40.0).abs() < 0.01);
            assert!((rect.y - (-12.0)).abs() < 0.01);
            assert!((rect.w - 300.0).abs() < 0.01);
            assert_eq!(rect.h, 0.0);
            assert_eq!(fill.as_deref(), None);
            assert_eq!(stroke.as_deref(), Some("4472C4"));
            assert!((*stroke_width_pt - 2.0).abs() < 0.01);
        }
        other => panic!("expected horizontal arrow shape, got {other:?}"),
    }

    let repacked = OoxmlCodec::pack_pptx_rich(&deck, false).expect("repack negative bleed & 0-dim");
    let parts = ZipArchiveCodec::unpack(&repacked, &ZipSecurityPolicy::default()).unwrap();
    let repacked_s1 = parts
        .iter()
        .find(|e| e.path == "ppt/slides/slide1.xml")
        .map(|e| String::from_utf8_lossy(&e.data).into_owned())
        .unwrap();
    assert!(repacked_s1.contains("x=\"-228600\""), "{repacked_s1}");
    assert!(repacked_s1.contains("y=\"-152400\""), "{repacked_s1}");
    assert!(repacked_s1.contains("cx=\"0\""), "{repacked_s1}");
    assert!(repacked_s1.contains("cy=\"0\""), "{repacked_s1}");

    let round = OoxmlCodec::unpack_pptx_rich(&repacked).expect("re-unpack");
    assert_eq!(round.slides[0].elements.len(), 2);

    // PDF rendering handles degenerate 0-dim shapes, images, tables, and charts cleanly
    let mut pdf_deck = deck.clone();
    pdf_deck.slides[0].elements.push(SlideElement::Image {
        id: "img0".into(),
        rect: SlideBox {
            x: 10.0,
            y: 10.0,
            w: 0.0,
            h: 20.0,
        },
        image: RichImage::new(make_png_1x1(), "1x1").unwrap(),
        alt: "zero".into(),
    });
    pdf_deck.slides[0].elements.push(SlideElement::Table {
        id: "tbl0".into(),
        rect: SlideBox {
            x: 10.0,
            y: 10.0,
            w: 50.0,
            h: 0.0,
        },
        rows: vec![vec!["A".into()]],
        header_row: false,
        fill: None,
    });
    pdf_deck.slides[0].elements.push(SlideElement::Chart {
        id: "ch0".into(),
        rect: SlideBox {
            x: 10.0,
            y: 10.0,
            w: 0.0,
            h: 0.0,
        },
        chart_type: easyoffice_formats::ChartKind::Column,
        title: "빈".into(),
        categories: vec!["1".into()],
        series: vec![SlideChartSeries {
            name: "s".into(),
            values: vec![1.0],
        }],
        legend: easyoffice_formats::ChartLegendPos::None,
    });
    let pdf = encode_slide_pdf(&pdf_deck, "PD-323").expect("pdf encode with 0-dim elements");
    let raw_pdf = String::from_utf8_lossy(&pdf);
    assert!(
        raw_pdf.contains(" m ") && raw_pdf.contains(" l "),
        "{raw_pdf}"
    );
    assert!(!raw_pdf.contains("0.00 0 0 "), "no degenerate cm matrix");
    assert!(!raw_pdf.contains("NaN"), "no NaN in PDF stream");
    let _parsed = PdfSerializer::parse(&pdf).expect("valid PDF");

    // Bounds check: -4001pt still fails closed, signed EMU conversion preserves negative bleed
    assert_eq!(pt_to_signed_emu(-18.0), -228_600);
    assert_eq!(pt_to_signed_emu(12.0), 152_400);
    assert_eq!(pt_to_signed_emu(-5_000.0), -50_800_000);
    let bad = SlideBox {
        x: -4001.0,
        y: 0.0,
        w: 10.0,
        h: 10.0,
    };
    assert!(matches!(
        bad.validate(),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn theme_colors_clr_map_ovr_transforms_style_refs_and_ea_font() {
    let theme_xml = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"Custom\">",
        "<a:themeElements><a:clrScheme name=\"Custom\">",
        "<a:dk1><a:sysClr val=\"windowText\" lastClr=\"102030\"/></a:dk1>",
        "<a:lt1><a:sysClr val=\"window\" lastClr=\"F0F1F2\"/></a:lt1>",
        "<a:dk2><a:srgbClr val=\"334455\"/></a:dk2>",
        "<a:lt2><a:srgbClr val=\"E0E1E2\"/></a:lt2>",
        "<a:accent1><a:srgbClr val=\"4472C4\"/></a:accent1>",
        "<a:accent2><a:srgbClr val=\"ED7D31\"/></a:accent2>",
        "<a:accent3><a:srgbClr val=\"A5A5A5\"/></a:accent3>",
        "<a:accent4><a:srgbClr val=\"FFC000\"/></a:accent4>",
        "<a:accent5><a:srgbClr val=\"5B9BD5\"/></a:accent5>",
        "<a:accent6><a:srgbClr val=\"70AD47\"/></a:accent6>",
        "<a:hlink><a:srgbClr val=\"0563C1\"/></a:hlink>",
        "<a:folHlink><a:srgbClr val=\"954F72\"/></a:folHlink>",
        "</a:clrScheme></a:themeElements></a:theme>"
    );
    let master_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sldMaster {PML_NS}><p:cSld><p:spTree>{}</p:spTree></p:cSld>\
        <p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/></p:sldMaster>",
        sp_tree_hdr()
    );
    // Layout overrides tx1 -> dk2 (334455)
    let layout_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sldLayout {PML_NS} type=\"obj\"><p:cSld><p:spTree>{}</p:spTree></p:cSld>\
        <p:clrMapOvr><a:overrideClrMapping bg1=\"lt1\" tx1=\"dk2\" bg2=\"lt2\" tx2=\"dk1\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/></p:clrMapOvr></p:sldLayout>",
        sp_tree_hdr()
    );

    let slide1_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sld {PML_NS}><p:cSld><p:spTree>{}\
        <p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"scheme-shape\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>\
        <p:spPr><a:xfrm><a:off x=\"127000\" y=\"127000\"/><a:ext cx=\"1270000\" cy=\"635000\"/></a:xfrm>\
        <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>\
        <a:solidFill><a:schemeClr val=\"accent1\"><a:lumMod val=\"60000\"/><a:lumOff val=\"40000\"/></a:schemeClr></a:solidFill>\
        <a:ln w=\"12700\"><a:solidFill><a:srgbClr val=\"FF0000\"><a:tint val=\"50000\"/></a:srgbClr></a:solidFill></a:ln></p:spPr>\
        <p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"ko-KR\" sz=\"1400\">\
        <a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill><a:ea typeface=\"맑은 고딕\"/></a:rPr><a:t>테마</a:t></a:r></a:p></p:txBody></p:sp>\
        <p:sp><p:nvSpPr><p:cNvPr id=\"3\" name=\"style-shape\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>\
        <p:spPr><a:xfrm><a:off x=\"1524000\" y=\"127000\"/><a:ext cx=\"1270000\" cy=\"635000\"/></a:xfrm>\
        <a:prstGeom prst=\"roundRect\"><a:avLst/></a:prstGeom></p:spPr>\
        <p:style><a:lnRef idx=\"2\"><a:srgbClr val=\"FFFFFF\"><a:shade val=\"50000\"/></a:srgbClr></a:lnRef>\
        <a:fillRef idx=\"1\"><a:schemeClr val=\"accent2\"/></a:fillRef>\
        <a:fontRef idx=\"minor\"><a:schemeClr val=\"lt1\"/></a:fontRef></p:style>\
        <p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>스타일</a:t></a:r></a:p></p:txBody></p:sp>\
        <p:sp><p:nvSpPr><p:cNvPr id=\"4\" name=\"nofill-shape\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>\
        <p:spPr><a:xfrm><a:off x=\"3000000\" y=\"127000\"/><a:ext cx=\"1270000\" cy=\"635000\"/></a:xfrm>\
        <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln></p:spPr>\
        <p:style><a:lnRef idx=\"2\"><a:schemeClr val=\"accent1\"/></a:lnRef><a:fillRef idx=\"1\"><a:schemeClr val=\"accent1\"/></a:fillRef></p:style>\
        <p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>무채움</a:t></a:r></a:p></p:txBody></p:sp>\
        <p:sp><p:nvSpPr><p:cNvPr id=\"6\" name=\"idx0-shape\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>\
        <p:spPr><a:xfrm><a:off x=\"4500000\" y=\"127000\"/><a:ext cx=\"1270000\" cy=\"635000\"/></a:xfrm>\
        <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr>\
        <p:style><a:lnRef idx=\"0\"><a:schemeClr val=\"accent1\"/></a:lnRef><a:fillRef idx=\"0\"><a:schemeClr val=\"accent1\"/></a:fillRef></p:style>\
        <p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>영번</a:t></a:r></a:p></p:txBody></p:sp>\
        <p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"5\" name=\"tbl-theme\"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr>\
        <p:xfrm><a:off x=\"127000\" y=\"1000000\"/><a:ext cx=\"2540000\" cy=\"1000000\"/></p:xfrm>\
        <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\"><a:tbl><a:tblPr firstRow=\"1\"/>\
        <a:tr h=\"500000\"><a:tc><a:txBody><a:p><a:r><a:t>제목</a:t></a:r></a:p></a:txBody>\
        <a:tcPr><a:solidFill><a:schemeClr val=\"accent1\"/></a:solidFill></a:tcPr></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>\
        </p:spTree></p:cSld></p:sld>",
        sp_tree_hdr()
    );
    // Slide 2 overrides tx1 -> accent2 via slide-level clrMapOvr
    let slide2_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sld {PML_NS}><p:cSld><p:spTree>{}\
        <p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"s2-ovr\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr>\
        <p:spPr><a:xfrm><a:off x=\"127000\" y=\"127000\"/><a:ext cx=\"1270000\" cy=\"635000\"/></a:xfrm><a:prstGeom prst=\"rect\"/></p:spPr>\
        <p:txBody><a:p><a:r><a:rPr><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill></a:rPr><a:t>슬라이드오버라이드</a:t></a:r></a:p></p:txBody></p:sp>\
        </p:spTree></p:cSld>\
        <p:clrMapOvr><a:overrideClrMapping bg1=\"lt1\" tx1=\"accent2\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/></p:clrMapOvr></p:sld>",
        sp_tree_hdr()
    );

    let s_rel = String::from_utf8(rels(&[(
        "rId1",
        "slideLayout",
        "../slideLayouts/slideLayout1.xml",
    )]))
    .unwrap();
    let bytes = minimal_pptx(
        &[(&slide1_xml, Some(&s_rel)), (&slide2_xml, Some(&s_rel))],
        vec![
            ("ppt/theme/theme9.xml".into(), theme_xml.as_bytes().to_vec()),
            (
                "ppt/slideMasters/slideMaster1.xml".into(),
                master_xml.into_bytes(),
            ),
            (
                "ppt/slideMasters/_rels/slideMaster1.xml.rels".into(),
                rels(&[("rId1", "theme", "../theme/theme9.xml")]),
            ),
            (
                "ppt/slideLayouts/slideLayout1.xml".into(),
                layout_xml.into_bytes(),
            ),
            (
                "ppt/slideLayouts/_rels/slideLayout1.xml.rels".into(),
                rels(&[("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")]),
            ),
        ],
    );

    let deck = OoxmlCodec::unpack_pptx_rich(&bytes).expect("unpack themed deck");
    match &deck.slides[0].elements[0] {
        SlideElement::Shape {
            fill,
            stroke,
            color,
            font_size_pt,
            ..
        } => {
            // accent1 (#4472C4) with lumMod 60% + lumOff 40% -> #8FAADC
            let f_hex = fill.as_deref().expect("lumMod/lumOff fill");
            assert_eq!(f_hex, "8FAADC");
            // FF0000 with linear 50% tint -> FFBCBC (0.5 linear -> ~188 = 0xBC)
            assert_eq!(stroke.as_deref(), Some("FFBCBC"));
            // tx1 remapped in slideLayout1 to dk2 = 334455
            assert_eq!(color.as_deref(), Some("334455"));
            assert!((*font_size_pt - 14.0).abs() < 0.01);
        }
        other => panic!("unexpected element 0: {other:?}"),
    }
    match &deck.slides[0].elements[1] {
        SlideElement::Shape {
            fill,
            stroke,
            color,
            ..
        } => {
            assert_eq!(fill.as_deref(), Some("ED7D31"));
            assert_eq!(stroke.as_deref(), Some("BCBCBC"));
            assert_eq!(color.as_deref(), Some("F0F1F2"));
        }
        other => panic!("unexpected element 1: {other:?}"),
    }
    match &deck.slides[0].elements[2] {
        SlideElement::Shape { fill, stroke, .. } => {
            assert_eq!(*fill, None, "explicit noFill overrides p:style fillRef");
            assert_eq!(*stroke, None, "explicit ln/noFill overrides p:style lnRef");
        }
        other => panic!("unexpected element 2: {other:?}"),
    }
    match &deck.slides[0].elements[3] {
        SlideElement::Shape { fill, stroke, .. } => {
            assert_eq!(*fill, None, "fillRef idx=0 means no fill");
            assert_eq!(*stroke, None, "lnRef idx=0 means no stroke");
        }
        other => panic!("unexpected element 3: {other:?}"),
    }
    match &deck.slides[0].elements[4] {
        SlideElement::Table { fill, .. } => {
            assert_eq!(fill.as_deref(), Some("4472C4"));
        }
        other => panic!("unexpected element 4: {other:?}"),
    }
    match &deck.slides[1].elements[0] {
        SlideElement::TextBox { color, .. } => {
            assert_eq!(
                color.as_deref(),
                Some("ED7D31"),
                "slide clrMapOvr overrides layout"
            );
        }
        other => panic!("unexpected slide 2 element: {other:?}"),
    }
}

#[test]
fn nested_groups_map_coordinates_and_fail_closed_above_16_levels() {
    let pt = |v: i64| v * 12_700;
    let slide = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sld {PML_NS}><p:cSld><p:spTree>{}\
        <p:grpSp><p:nvGrpSpPr><p:cNvPr id=\"10\" name=\"outer\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>\
        <p:grpSpPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/>\
        <a:chOff x=\"{}\" y=\"{}\"/><a:chExt cx=\"{}\" cy=\"{}\"/></a:xfrm></p:grpSpPr>\
          <p:grpSp><p:nvGrpSpPr><p:cNvPr id=\"11\" name=\"inner\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>\
          <p:grpSpPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/>\
          <a:chOff x=\"{}\" y=\"{}\"/><a:chExt cx=\"{}\" cy=\"{}\"/></a:xfrm></p:grpSpPr>\
            <p:sp><p:nvSpPr><p:cNvPr id=\"12\" name=\"nested-box\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr>\
            <p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"/></p:spPr>\
            <p:txBody><a:p><a:r><a:t>그룹안</a:t></a:r></a:p></p:txBody></p:sp>\
          </p:grpSp>\
        </p:grpSp></p:spTree></p:cSld></p:sld>",
        sp_tree_hdr(),
        pt(100), pt(50), pt(400), pt(200), pt(1000), pt(1000), pt(200), pt(100),
        pt(1050), pt(1020), pt(100), pt(50), pt(5000), pt(5000), pt(100), pt(50),
        pt(5010), pt(5005), pt(40), pt(20),
    );
    let bytes = minimal_pptx(&[(&slide, None)], vec![]);
    let deck = OoxmlCodec::unpack_pptx_rich(&bytes).expect("unpack nested groups");
    assert_eq!(deck.slides[0].elements.len(), 1);
    let r = deck.slides[0].elements[0].rect();
    assert!((r.x - 220.0).abs() < 0.01, "{r:?}");
    assert!((r.y - 100.0).abs() < 0.01, "{r:?}");
    assert!((r.w - 80.0).abs() < 0.01, "{r:?}");
    assert!((r.h - 40.0).abs() < 0.01, "{r:?}");

    let make_deep_group = |depth: usize| -> Vec<u8> {
        let mut body = sp_tree_hdr().to_string();
        for i in 0..depth {
            body.push_str(&format!(
                "<p:grpSp><p:nvGrpSpPr><p:cNvPr id=\"{}\" name=\"g{i}\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"1270000\" cy=\"1270000\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"1270000\" cy=\"1270000\"/></a:xfrm></p:grpSpPr>",
                10 + i
            ));
        }
        body.push_str("<p:sp><p:nvSpPr><p:cNvPr id=\"99\" name=\"leaf\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"127000\" y=\"127000\"/><a:ext cx=\"127000\" cy=\"127000\"/></a:xfrm><a:prstGeom prst=\"rect\"/></p:spPr><p:txBody><a:p><a:r><a:t>깊이</a:t></a:r></a:p></p:txBody></p:sp>");
        for _ in 0..depth {
            body.push_str("</p:grpSp>");
        }
        let s = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sld {PML_NS}><p:cSld><p:spTree>{body}</p:spTree></p:cSld></p:sld>"
        );
        minimal_pptx(&[(&s, None)], vec![])
    };

    let ok16 = OoxmlCodec::unpack_pptx_rich(&make_deep_group(16)).expect("16 levels allowed");
    assert_eq!(ok16.slides[0].elements.len(), 1);
    assert!(matches!(
        OoxmlCodec::unpack_pptx_rich(&make_deep_group(17)),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn excludes_non_body_placeholders_roundtrips_two_column_and_ignores_end_para_rpr() {
    let slide = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sld {PML_NS}><p:cSld><p:spTree>{}\
        <p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"t\"/><p:cNvSpPr/><p:nvPr><p:ph type=\"title\"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:p><a:r><a:t>제목</a:t></a:r></a:p></p:txBody></p:sp>\
        <p:sp><p:nvSpPr><p:cNvPr id=\"3\" name=\"b\"/><p:cNvSpPr/><p:nvPr><p:ph idx=\"1\"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:p><a:r><a:t>본문</a:t></a:r></a:p></p:txBody></p:sp>\
        <p:sp><p:nvSpPr><p:cNvPr id=\"4\" name=\"dt\"/><p:cNvSpPr/><p:nvPr><p:ph type=\"dt\"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:p><a:r><a:t>2026-11-23</a:t></a:r></a:p></p:txBody></p:sp>\
        <p:sp><p:nvSpPr><p:cNvPr id=\"5\" name=\"ftr\"/><p:cNvSpPr/><p:nvPr><p:ph type=\"ftr\"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:p><a:r><a:t>기밀</a:t></a:r></a:p></p:txBody></p:sp>\
        <p:sp><p:nvSpPr><p:cNvPr id=\"6\" name=\"num\"/><p:cNvSpPr/><p:nvPr><p:ph type=\"sldNum\"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:p><a:r><a:t>7</a:t></a:r></a:p></p:txBody></p:sp>\
        <p:sp><p:nvSpPr><p:cNvPr id=\"7\" name=\"hdr\"/><p:cNvSpPr/><p:nvPr><p:ph type=\"hdr\"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:p><a:r><a:t>머리글</a:t></a:r></a:p></p:txBody></p:sp>\
        <p:sp><p:nvSpPr><p:cNvPr id=\"8\" name=\"tb\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr>\
        <p:spPr><a:xfrm><a:off x=\"127000\" y=\"127000\"/><a:ext cx=\"1270000\" cy=\"635000\"/></a:xfrm><a:prstGeom prst=\"rect\"/></p:spPr>\
        <p:txBody><a:p><a:r><a:rPr sz=\"1600\" b=\"0\"/><a:t>보통</a:t></a:r><a:endParaRPr b=\"1\" i=\"1\" u=\"sng\" sz=\"2800\"><a:solidFill><a:srgbClr val=\"FF0000\"/></a:solidFill></a:endParaRPr></a:p>\
        <a:p><a:endParaRPr b=\"1\"><a:solidFill><a:srgbClr val=\"00FF00\"/></a:solidFill></a:endParaRPr></a:p></p:txBody></p:sp>\
        </p:spTree></p:cSld></p:sld>",
        sp_tree_hdr()
    );
    let bytes = minimal_pptx(&[(&slide, None)], vec![]);
    let deck = OoxmlCodec::unpack_pptx_rich(&bytes).expect("unpack");
    assert_eq!(deck.slides[0].title, "제목");
    assert_eq!(deck.slides[0].bullets, vec!["본문"]);
    match &deck.slides[0].elements[0] {
        SlideElement::TextBox {
            bold,
            italic,
            underline,
            font_size_pt,
            color,
            ..
        } => {
            assert!(!*bold, "endParaRPr b=1 must not force bold");
            assert!(!*italic);
            assert!(!*underline);
            assert!((*font_size_pt - 16.0).abs() < 0.01);
            assert_eq!(*color, None, "endParaRPr fill must not pollute text color");
        }
        other => panic!("expected TextBox, got {other:?}"),
    }

    // TwoColumn roundtrip: left idx="1" + right idx="2"
    let two_col_deck = RichSlideDeck {
        title: "2단 테스트".into(),
        is_slideshow: false,
        aspect: SlideAspect::Widescreen16x9,
        orientation: SlideOrientation::Landscape,
        elements_rejected: 0,
        slides: vec![
            RichSlide {
                title: "비교".into(),
                bullets: vec!["왼쪽 1".into(), "왼쪽 2".into(), "오른쪽 1".into()],
                notes: String::new(),
                layout: SlideMasterLayout::TwoColumn,
                transition: SlideTransition::None,
                ext: Default::default(),
                elements: vec![],
            },
            RichSlide {
                title: "빈 2단".into(),
                bullets: vec![],
                notes: String::new(),
                layout: SlideMasterLayout::TwoColumn,
                transition: SlideTransition::None,
                ext: Default::default(),
                elements: vec![],
            },
        ],
    };
    let packed = OoxmlCodec::pack_pptx_rich(&two_col_deck, false).expect("pack two-col");
    let entries = ZipArchiveCodec::unpack(&packed, &ZipSecurityPolicy::default()).unwrap();
    let s1 = entries
        .iter()
        .find(|e| e.path == "ppt/slides/slide1.xml")
        .map(|e| String::from_utf8_lossy(&e.data).into_owned())
        .unwrap();
    assert!(s1.contains("<p:ph idx=\"1\"/>"), "{s1}");
    assert!(s1.contains("<p:ph idx=\"2\"/>"), "{s1}");
    let p2 = s1.find("<p:ph idx=\"2\"/>").unwrap();
    assert!(s1.find("왼쪽 2").unwrap() < p2 && p2 < s1.find("오른쪽 1").unwrap());
    let back = OoxmlCodec::unpack_pptx_rich(&packed).expect("unpack two-col");
    assert_eq!(back.slides[0].layout, SlideMasterLayout::TwoColumn);
    assert_eq!(back.slides[0].bullets, vec!["왼쪽 1", "왼쪽 2", "오른쪽 1"]);
    assert_eq!(back.slides[1].layout, SlideMasterLayout::TwoColumn);
    assert!(back.slides[1].bullets.is_empty());
}
