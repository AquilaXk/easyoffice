//! Korean presentation defaults (`PD-310`) across PPTX and ODP:
//! - Widescreen 16:9 (`12,192,000 x 6,858,000` EMU = `33.867cm x 19.05cm`), `맑은 고딕` theme,
//!   master/ODP default styles (`18pt`, `ko-KR`), default `SlideTransition::None`,
//!   Korean master layout names, layout-specific placeholders (`ctrTitle`/`subTitle`, `secHead`,
//!   `twoObj` 2-column master layout), empty-placeholder omission on `Blank`, and ODP spec fixes.

use easyoffice_formats::{
    EasyOfficeError, OdfCodec, OoxmlCodec, OoxmlPptxPayload, RichSlide, RichSlideDeck, SlideAspect,
    SlideBox, SlideElement, SlideMasterLayout, SlideTextAlign, SlideTransition, ZipArchiveCodec,
    ZipSecurityPolicy,
};

fn unpack(bytes: &[u8]) -> Vec<(String, String)> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .expect("unpack zip")
        .into_iter()
        .map(|entry| {
            (
                entry.path,
                String::from_utf8_lossy(&entry.data).into_owned(),
            )
        })
        .collect()
}

fn part<'a>(parts: &'a [(String, String)], path: &str) -> &'a str {
    parts
        .iter()
        .find(|(p, _)| p == path)
        .map(|(_, s)| s.as_str())
        .unwrap_or_else(|| panic!("missing part {path}"))
}

#[test]
fn pptx_legacy_and_rich_emit_16_9_malgun_gothic_theme_master_and_layouts() {
    let default_deck = RichSlideDeck::default();
    assert_eq!(default_deck.aspect, SlideAspect::Widescreen16x9);
    let default_slide = RichSlide::default();
    assert_eq!(default_slide.layout, SlideMasterLayout::TitleAndContent);
    assert_eq!(default_slide.transition, SlideTransition::None);

    let legacy = OoxmlCodec::pack_pptx(&OoxmlPptxPayload {
        title: "한국어 기본 발표자료".into(),
        is_slideshow: false,
        slides: vec![("표지".into(), vec!["부제목".into()], "노트 내용".into())],
    })
    .expect("pack legacy pptx");
    let l_parts = unpack(&legacy);

    let pres = part(&l_parts, "ppt/presentation.xml");
    assert!(
        pres.contains("<p:sldSz cx=\"12192000\" cy=\"6858000\"/>"),
        "{pres}"
    );
    assert!(pres.contains("<p:defaultTextStyle>"), "{pres}");
    assert!(pres.contains("lang=\"ko-KR\""), "{pres}");
    assert!(pres.contains("sz=\"1800\""), "{pres}");

    for theme_path in ["ppt/theme/theme1.xml", "ppt/theme/theme2.xml"] {
        let theme = part(&l_parts, theme_path);
        assert!(
            theme.contains(
                "<a:majorFont><a:latin typeface=\"맑은 고딕\"/><a:ea typeface=\"맑은 고딕\""
            ),
            "{theme_path}: {theme}"
        );
        assert!(
            theme.contains(
                "<a:minorFont><a:latin typeface=\"맑은 고딕\"/><a:ea typeface=\"맑은 고딕\""
            ),
            "{theme_path}: {theme}"
        );
        assert!(theme.contains("<a:font script=\"Hang\" typeface=\"맑은 고딕\"/>"));
        assert!(!theme.contains("Calibri"), "{theme_path} leaked Calibri");
    }

    let master = part(&l_parts, "ppt/slideMasters/slideMaster1.xml");
    assert!(master.contains("<p:bg><p:bgRef idx=\"1001\">"), "{master}");
    assert!(master.contains("<p:txStyles>"), "{master}");
    assert!(master.contains("sz=\"4400\""), "{master}");
    assert!(master.contains("sz=\"2800\""), "{master}");
    assert!(master.contains("sz=\"1800\""), "{master}");
    assert!(
        master.contains("<a:buFont typeface=\"맑은 고딕\""),
        "{master}"
    );

    let lay1 = part(&l_parts, "ppt/slideLayouts/slideLayout1.xml");
    assert!(lay1.contains("name=\"제목 슬라이드\""), "{lay1}");
    assert!(lay1.contains("<p:ph type=\"ctrTitle\"/>"), "{lay1}");
    assert!(
        lay1.contains("<p:ph type=\"subTitle\" idx=\"1\"/>"),
        "{lay1}"
    );
    assert!(lay1.contains("sz=\"6000\""), "{lay1}");
    assert!(lay1.contains("sz=\"2400\""), "{lay1}");

    let lay2 = part(&l_parts, "ppt/slideLayouts/slideLayout2.xml");
    assert!(lay2.contains("name=\"제목 및 내용\""), "{lay2}");
    assert!(lay2.contains("<p:ph type=\"title\"/>"), "{lay2}");

    let lay3 = part(&l_parts, "ppt/slideLayouts/slideLayout3.xml");
    assert!(lay3.contains("name=\"구역 머리글\""), "{lay3}");

    let lay4 = part(&l_parts, "ppt/slideLayouts/slideLayout4.xml");
    assert!(lay4.contains("name=\"두 개의 내용\""), "{lay4}");
    assert!(
        lay4.contains("<p:ph idx=\"1\"/>") && lay4.contains("<p:ph idx=\"2\"/>"),
        "{lay4}"
    );

    let lay5 = part(&l_parts, "ppt/slideLayouts/slideLayout5.xml");
    assert!(lay5.contains("name=\"빈 화면\""), "{lay5}");
    assert!(
        !lay5.contains("<p:sp>"),
        "blank layout has no placeholders: {lay5}"
    );

    let deck = RichSlideDeck {
        title: "레이아웃 검증".into(),
        is_slideshow: false,
        aspect: SlideAspect::Widescreen16x9,
        elements_rejected: 0,
        slides: vec![
            RichSlide {
                title: "대제목".into(),
                bullets: vec!["부제목".into()],
                notes: String::new(),
                layout: SlideMasterLayout::TitleSlide,
                transition: SlideTransition::None,
                elements: vec![],
            },
            RichSlide {
                title: "구역".into(),
                bullets: vec!["설명".into()],
                notes: String::new(),
                layout: SlideMasterLayout::SectionHeader,
                transition: SlideTransition::None,
                elements: vec![],
            },
            RichSlide {
                title: String::new(),
                bullets: vec![],
                notes: String::new(),
                layout: SlideMasterLayout::Blank,
                transition: SlideTransition::None,
                elements: vec![SlideElement::TextBox {
                    id: "free-1".into(),
                    rect: SlideBox {
                        x: 80.0,
                        y: 120.0,
                        w: 300.0,
                        h: 60.0,
                    },
                    text: "자유 글상자".into(),
                    font_size_pt: 18.0,
                    bold: false,
                    italic: false,
                    underline: false,
                    align: SlideTextAlign::Left,
                    color: None,
                    fill: None,
                    role: None,
                }],
            },
        ],
    };
    let bytes = OoxmlCodec::pack_pptx_rich(&deck, false).expect("pack rich");
    let parts = unpack(&bytes);
    let s1 = part(&parts, "ppt/slides/slide1.xml");
    assert!(s1.contains("<p:ph type=\"ctrTitle\"/>"), "{s1}");
    assert!(s1.contains("<p:ph type=\"subTitle\" idx=\"1\"/>"), "{s1}");
    assert!(!s1.contains("<p:transition"), "{s1}");

    let s2 = part(&parts, "ppt/slides/slide2.xml");
    assert!(s2.contains("<p:ph type=\"title\"/>"), "{s2}");
    assert!(s2.contains("<p:ph type=\"body\" idx=\"1\"/>"), "{s2}");

    let s3 = part(&parts, "ppt/slides/slide3.xml");
    assert!(
        !s3.contains("<p:ph "),
        "blank slide omits empty placeholders: {s3}"
    );
    assert!(
        s3.contains("<a:latin typeface=\"+mn-lt\"/><a:ea typeface=\"+mn-ea\"/>"),
        "{s3}"
    );

    let back = OoxmlCodec::unpack_pptx_rich(&bytes).expect("unpack rich");
    assert_eq!(back.aspect, SlideAspect::Widescreen16x9);
    assert_eq!(back.slides[0].title, "대제목");
    assert_eq!(back.slides[0].bullets, vec!["부제목"]);
    assert_eq!(back.slides[0].layout, SlideMasterLayout::TitleSlide);
    assert_eq!(back.slides[0].transition, SlideTransition::None);
    assert_eq!(back.slides[1].layout, SlideMasterLayout::SectionHeader);
    assert_eq!(back.slides[2].layout, SlideMasterLayout::Blank);
    assert!(back.slides[2].title.is_empty() && back.slides[2].bullets.is_empty());
    assert_eq!(back.slides[2].elements.len(), 1);
}

#[test]
fn odp_writes_malgun_gothic_default_styles_standard_xlink_and_layout_frames() {
    let deck = RichSlideDeck {
        title: "ODP 한국어 기본값".into(),
        is_slideshow: false,
        aspect: SlideAspect::Widescreen16x9,
        elements_rejected: 0,
        slides: vec![
            RichSlide {
                title: "첫 화면".into(),
                bullets: vec!["부제".into()],
                notes: String::new(),
                layout: SlideMasterLayout::TitleSlide,
                transition: SlideTransition::None,
                elements: vec![],
            },
            RichSlide {
                title: String::new(),
                bullets: vec![],
                notes: String::new(),
                layout: SlideMasterLayout::Blank,
                transition: SlideTransition::None,
                elements: vec![SlideElement::TextBox {
                    id: "tb".into(),
                    rect: SlideBox {
                        x: 60.0,
                        y: 90.0,
                        w: 240.0,
                        h: 50.0,
                    },
                    text: "본문 상자".into(),
                    font_size_pt: 18.0,
                    bold: false,
                    italic: false,
                    underline: false,
                    align: SlideTextAlign::Left,
                    color: None,
                    fill: None,
                    role: None,
                }],
            },
        ],
    };
    let odp = OdfCodec::pack_odp_rich(&deck).expect("pack odp");
    let parts = unpack(&odp);

    let styles = part(&parts, "styles.xml");
    assert!(styles.contains("style:name=\"맑은 고딕\""), "{styles}");
    assert!(
        styles.contains("<style:default-style style:family=\"graphic\">"),
        "{styles}"
    );
    assert!(
        styles.contains("<style:default-style style:family=\"paragraph\">"),
        "{styles}"
    );
    assert!(
        styles.contains("fo:font-size=\"18pt\"")
            && styles.contains("style:font-size-asian=\"18pt\""),
        "{styles}"
    );
    assert!(
        styles.contains("fo:language=\"ko\" fo:country=\"KR\""),
        "{styles}"
    );
    assert!(styles.contains("style:name=\"Standard-title\""), "{styles}");
    assert!(
        styles.contains("<presentation:placeholder presentation:object=\"subtitle\""),
        "{styles}"
    );
    assert!(
        styles.contains("<style:presentation-page-layout style:name=\"eo_blank\"/>"),
        "{styles}"
    );
    assert!(
        styles.contains("fo:margin-top=\"0cm\" fo:margin-bottom=\"0cm\""),
        "{styles}"
    );
    assert!(
        styles.contains("fo:page-width=\"33.867cm\" fo:page-height=\"19.050cm\""),
        "{styles}"
    );

    let content = part(&parts, "content.xml");
    assert!(
        content.contains("xmlns:xlink=\"http://www.w3.org/1999/xlink\""),
        "{content}"
    );
    let root_open = &content[..content.find('>').unwrap()];
    assert!(!content.contains("<office:document-content title="));
    assert!(!root_open.contains(" title="), "{content}");
    assert!(content.contains("<office:font-face-decls>"), "{content}");
    assert!(
        content.contains("presentation:placeholder=\"true\" presentation:class=\"subtitle\""),
        "{content}"
    );
    assert!(
        content.contains("style:font-name-asian=\"맑은 고딕\""),
        "{content}"
    );

    let blank_page_start = content.rfind("<draw:page ").unwrap();
    let blank_page = &content[blank_page_start..];
    assert!(
        !blank_page.contains("presentation:placeholder=\"true\""),
        "blank ODP slide omits empty layout frames: {blank_page}"
    );

    let back = OdfCodec::unpack_odp_rich(&odp).expect("unpack odp");
    assert_eq!(back.title, "ODP 한국어 기본값");
    assert_eq!(back.aspect, SlideAspect::Widescreen16x9);
    assert_eq!(back.slides[0].layout, SlideMasterLayout::TitleSlide);
    assert_eq!(back.slides[0].transition, SlideTransition::None);
    assert_eq!(back.slides[1].layout, SlideMasterLayout::Blank);
    assert!(back.slides[1].title.is_empty() && back.slides[1].bullets.is_empty());
    assert_eq!(back.slides[1].elements.len(), 1);
}

#[test]
fn hostile_pptx_slide_size_and_oom_odp_coords_fail_closed() {
    let bad_pptx = ZipArchiveCodec::pack(&[
        (
            "[Content_Types].xml".into(),
            b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>"
                .to_vec(),
        ),
        (
            "ppt/presentation.xml".into(),
            b"<p:presentation xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:sldSz cx=\"abc\" cy=\"6858000\"/><p:sldIdLst/></p:presentation>"
                .to_vec(),
        ),
    ])
    .unwrap();
    assert!(matches!(
        OoxmlCodec::unpack_pptx_rich(&bad_pptx),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let bad_odp = ZipArchiveCodec::pack(&[
        ("mimetype".into(), OdfCodec::MIMETYPE_ODP.as_bytes().to_vec()),
        (
            "content.xml".into(),
            b"<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\"><office:body><office:presentation><draw:page><draw:frame draw:name=\"oob\" svg:x=\"10pt\" svg:y=\"10pt\" svg:width=\"5000pt\" svg:height=\"40pt\"><draw:text-box><text:p>x</text:p></draw:text-box></draw:frame></draw:page></office:presentation></office:body></office:document-content>"
                .to_vec(),
        ),
    ])
    .unwrap();
    assert!(matches!(
        OdfCodec::unpack_odp_rich(&bad_odp),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}
