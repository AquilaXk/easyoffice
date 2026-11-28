//! PD-394: slide extensions (hidden slides, backgrounds, header/footer fields, rotation and flip,
//! outline dash, outer shadow, paragraph format, groups, animations) survive a PPTX and an ODP
//! round trip, and hostile input stays inside the budgets.

use easyoffice_formats::{
    deflate_zlib, AnimClass, AnimDir, AnimEffect, AnimStart, EasyOfficeError, ElementFx, IeeeCrc32,
    OdfCodec, OoxmlCodec, RichImage, RichSlide, RichSlideDeck, SlideAnim, SlideBackground,
    SlideBox, SlideBullet, SlideDash, SlideDate, SlideElement, SlideExt, SlideFooter, SlideGroup,
    SlideMasterLayout, SlideNumberStyle, SlideOutline, SlideParaFmt, SlideShadow, SlideShapeType,
    SlideTextAlign, ZipArchiveCodec, ZipSecurityPolicy,
};

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = IeeeCrc32::update(IeeeCrc32::checksum(kind), data);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

/// A 2x2 RGBA PNG.
fn png() -> Vec<u8> {
    let raw: Vec<u8> = (0..2)
        .flat_map(|_| [0u8, 37, 99, 235, 255, 37, 99, 235, 255])
        .collect();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    bytes.extend_from_slice(&png_chunk(b"IDAT", &deflate_zlib(&raw)));
    bytes.extend_from_slice(&png_chunk(b"IEND", &[]));
    bytes
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> SlideBox {
    SlideBox { x, y, w, h }
}

fn text_box(id: &str, r: SlideBox, text: &str) -> SlideElement {
    SlideElement::TextBox {
        id: id.into(),
        rect: r,
        text: text.into(),
        font_size_pt: 20.0,
        bold: false,
        italic: false,
        underline: false,
        align: SlideTextAlign::Left,
        color: Some("1F2937".into()),
        fill: None,
        role: None,
    }
}

fn shape(id: &str, r: SlideBox) -> SlideElement {
    SlideElement::Shape {
        id: id.into(),
        rect: r,
        shape_type: SlideShapeType::Ellipse,
        text: "도형".into(),
        fill: Some("2563EB".into()),
        stroke: Some("1D4ED8".into()),
        stroke_width_pt: 2.0,
        color: Some("FFFFFF".into()),
        font_size_pt: 16.0,
        bold: true,
        italic: false,
        underline: false,
        align: SlideTextAlign::Center,
    }
}

fn image(id: &str, r: SlideBox) -> SlideElement {
    SlideElement::Image {
        id: id.into(),
        rect: r,
        image: RichImage::new(png(), "그림").unwrap(),
        alt: "그림".into(),
    }
}

fn deck_with(slides: Vec<RichSlide>) -> RichSlideDeck {
    RichSlideDeck {
        title: "확장".into(),
        slides,
        ..RichSlideDeck::default()
    }
}

fn anim(target: &str, class: AnimClass, effect: AnimEffect, start: AnimStart) -> SlideAnim {
    SlideAnim {
        target: target.into(),
        class,
        effect,
        start,
        duration_ms: 700,
        delay_ms: 200,
        // Only fly and wipe have a side; the others read back as the default one.
        direction: if matches!(effect, AnimEffect::Fly | AnimEffect::Wipe) {
            AnimDir::Left
        } else {
            AnimDir::default()
        },
    }
}

/// A slide that uses every extension once.
fn full_slide() -> RichSlide {
    let elements = vec![
        text_box("t1", rect(40.0, 60.0, 300.0, 80.0), "첫째\n둘째"),
        shape("s1", rect(400.0, 60.0, 160.0, 90.0)),
        image("i1", rect(600.0, 80.0, 100.0, 100.0)),
        text_box("t2", rect(40.0, 300.0, 300.0, 60.0), "안 묶음"),
    ];
    let mut fx_t1 = ElementFx::new("t1");
    fx_t1.rotation_deg = 30.0;
    fx_t1.outline = Some(SlideOutline {
        color: "FF0000".into(),
        width_pt: 3.0,
    });
    fx_t1.dash = SlideDash::Dash;
    fx_t1.para = Some(SlideParaFmt {
        justify: true,
        bullet: SlideBullet::Number(SlideNumberStyle::ArabicPeriod),
        line_spacing_pct: Some(150),
        space_before_pt: Some(6.0),
        space_after_pt: Some(3.0),
    });
    let mut fx_s1 = ElementFx::new("s1");
    fx_s1.flip_h = true;
    fx_s1.flip_v = true;
    fx_s1.dash = SlideDash::Dot;
    fx_s1.shadow = Some(SlideShadow {
        color: "000000".into(),
        alpha_pct: 40,
        blur_pt: 4.0,
        dist_pt: 3.0,
        angle_deg: 45.0,
    });
    let mut fx_i1 = ElementFx::new("i1");
    fx_i1.rotation_deg = 90.0;
    fx_i1.outline = Some(SlideOutline {
        color: "00AA00".into(),
        width_pt: 1.5,
    });
    let mut fx_t2 = ElementFx::new("t2");
    fx_t2.para = Some(SlideParaFmt {
        bullet: SlideBullet::Char('•'),
        ..SlideParaFmt::default()
    });
    RichSlide {
        title: "제목".into(),
        layout: SlideMasterLayout::Blank,
        elements,
        ext: SlideExt {
            hidden: true,
            background: Some(SlideBackground::Gradient {
                from: "112233".into(),
                to: "AABBCC".into(),
                angle_deg: 90,
            }),
            footer: SlideFooter {
                number: true,
                date: SlideDate::Fixed("2026-11-28".into()),
                text: Some("분기 보고".into()),
            },
            fx: vec![fx_t1, fx_s1, fx_i1, fx_t2],
            groups: vec![
                SlideGroup {
                    id: "g1".into(),
                    members: vec!["t1".into(), "s1".into()],
                },
                SlideGroup {
                    id: "g2".into(),
                    members: vec!["g1".into(), "i1".into()],
                },
            ],
            anims: vec![
                anim(
                    "g2",
                    AnimClass::Entrance,
                    AnimEffect::Fly,
                    AnimStart::OnClick,
                ),
                anim(
                    "t2",
                    AnimClass::Entrance,
                    AnimEffect::Fade,
                    AnimStart::WithPrevious,
                ),
                anim(
                    "t2",
                    AnimClass::Emphasis,
                    AnimEffect::Spin,
                    AnimStart::AfterPrevious,
                ),
                anim("s1", AnimClass::Exit, AnimEffect::Wipe, AnimStart::OnClick),
            ],
            ..SlideExt::default()
        },
        ..RichSlide::default()
    }
}

pub fn dump_deck() -> RichSlideDeck {
    let mut s = full_slide();
    s.ext.hidden = false;
    deck_with(vec![s])
}

fn assert_ext_roundtrip(read: &RichSlideDeck, want: &RichSlide) {
    let got = &read.slides[0];
    assert_eq!(got.ext.hidden, want.ext.hidden, "hidden");
    assert_eq!(got.ext.background, want.ext.background, "background");
    assert_eq!(got.ext.footer, want.ext.footer, "footer");
    assert_eq!(got.ext.groups, want.ext.groups, "groups");
    assert_eq!(got.ext.anims, want.ext.anims, "anims");
    assert_eq!(got.ext.fx, want.ext.fx, "fx");
    let ids: Vec<&str> = got.elements.iter().map(SlideElement::id).collect();
    assert_eq!(ids, ["t1", "s1", "i1", "t2"]);
}

#[test]
fn pptx_roundtrip_keeps_every_extension() {
    let deck = deck_with(vec![full_slide()]);
    let bytes = OoxmlCodec::pack_pptx_rich(&deck, false).unwrap();
    let read = OoxmlCodec::unpack_pptx_rich(&bytes).unwrap();
    assert_ext_roundtrip(&read, &deck.slides[0]);
}

#[test]
fn pptx_xml_uses_the_standard_elements() {
    let deck = deck_with(vec![full_slide()]);
    let bytes = OoxmlCodec::pack_pptx_rich(&deck, false).unwrap();
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let xml = entries
        .iter()
        .find(|e| e.path == "ppt/slides/slide1.xml")
        .map(|e| String::from_utf8(e.data.clone()).unwrap())
        .unwrap();
    for needle in [
        " show=\"0\"",
        "<p:bg><p:bgPr><a:gradFill",
        "<p:grpSp>",
        "rot=\"1800000\"",
        "flipH=\"1\" flipV=\"1\"",
        "<a:prstDash val=\"dash\"/>",
        "<a:outerShdw blurRad=\"50800\" dist=\"38100\" dir=\"2700000\"",
        "<a:alpha val=\"40000\"/>",
        "<a:buAutoNum type=\"arabicPeriod\"/>",
        "<a:buChar char=\"•\"/>",
        "<a:lnSpc><a:spcPct val=\"150000\"/></a:lnSpc>",
        "type=\"sldNum\"",
        "type=\"dt\"",
        "type=\"ftr\"",
        "<p:timing>",
        "presetClass=\"entr\"",
        "presetClass=\"emph\"",
        "presetClass=\"exit\"",
        "nodeType=\"withEffect\"",
        "nodeType=\"afterEffect\"",
    ] {
        assert!(xml.contains(needle), "missing {needle}");
    }
}

#[test]
fn pptx_picture_and_solid_backgrounds_roundtrip() {
    let pic = RichSlide {
        ext: SlideExt {
            background: Some(SlideBackground::Picture(RichImage::new(png(), "").unwrap())),
            ..SlideExt::default()
        },
        ..RichSlide::default()
    };
    let solid = RichSlide {
        ext: SlideExt {
            background: Some(SlideBackground::Solid("FFEEDD".into())),
            ..SlideExt::default()
        },
        ..RichSlide::default()
    };
    let deck = deck_with(vec![pic.clone(), solid.clone()]);
    let read =
        OoxmlCodec::unpack_pptx_rich(&OoxmlCodec::pack_pptx_rich(&deck, false).unwrap()).unwrap();
    assert_eq!(read.slides[0].ext.background, pic.ext.background);
    assert_eq!(read.slides[1].ext.background, solid.ext.background);
}

#[test]
fn pptx_date_field_stays_automatic() {
    let slide = RichSlide {
        ext: SlideExt {
            footer: SlideFooter {
                number: false,
                date: SlideDate::Auto,
                text: None,
            },
            ..SlideExt::default()
        },
        ..RichSlide::default()
    };
    let deck = deck_with(vec![slide]);
    let read =
        OoxmlCodec::unpack_pptx_rich(&OoxmlCodec::pack_pptx_rich(&deck, false).unwrap()).unwrap();
    assert_eq!(read.slides[0].ext.footer.date, SlideDate::Auto);
}

#[test]
fn model_rejects_dangling_and_cyclic_references() {
    let mut slide = full_slide();
    slide.ext.anims.push(anim(
        "nope",
        AnimClass::Entrance,
        AnimEffect::Appear,
        AnimStart::OnClick,
    ));
    assert!(matches!(
        deck_with(vec![slide]).validate(),
        Err(EasyOfficeError::InvalidArgument(_))
    ));

    let mut slide = full_slide();
    slide.ext.groups[0].members.push("g2".into());
    assert!(
        deck_with(vec![slide]).validate().is_err(),
        "a group cannot contain its own parent"
    );

    let mut slide = full_slide();
    slide.ext.anims[0].effect = AnimEffect::Spin;
    assert!(
        deck_with(vec![slide]).validate().is_err(),
        "spin is not an entrance effect"
    );

    let mut slide = full_slide();
    slide.ext.fx[0].shadow = Some(SlideShadow {
        color: "000000".into(),
        alpha_pct: 40,
        blur_pt: 9e9,
        dist_pt: 3.0,
        angle_deg: 0.0,
    });
    assert!(
        deck_with(vec![slide]).validate().is_err(),
        "shadow blur is capped"
    );
}

#[test]
fn pptx_reader_caps_animations() {
    let deck = deck_with(vec![full_slide()]);
    let bytes = OoxmlCodec::pack_pptx_rich(&deck, false).unwrap();
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let mut out = Vec::new();
    for e in entries {
        let data = if e.path == "ppt/slides/slide1.xml" {
            let xml = String::from_utf8(e.data).unwrap();
            let start = xml.find("<p:par><p:cTn id=\"3\"").unwrap();
            let end = xml.find("</p:childTnLst></p:cTn><p:prevCondLst>").unwrap();
            let one_click = &xml[start..end];
            let flood = one_click.repeat(300);
            format!("{}{flood}{}", &xml[..start], &xml[end..]).into_bytes()
        } else {
            e.data
        };
        out.push((e.path, data));
    }
    let hostile = ZipArchiveCodec::pack_compressed(&out).unwrap();
    assert!(matches!(
        OoxmlCodec::unpack_pptx_rich(&hostile),
        Err(EasyOfficeError::ResourceBudgetExceeded(_) | EasyOfficeError::CorruptedContainer(_))
    ));
}

#[test]
fn pptx_duplicate_shape_names_get_unique_ids() {
    let deck = deck_with(vec![RichSlide {
        layout: SlideMasterLayout::Blank,
        elements: vec![
            text_box("same", rect(10.0, 10.0, 100.0, 40.0), "가"),
            text_box("same", rect(10.0, 80.0, 100.0, 40.0), "나"),
        ],
        ..RichSlide::default()
    }]);
    let read =
        OoxmlCodec::unpack_pptx_rich(&OoxmlCodec::pack_pptx_rich(&deck, false).unwrap()).unwrap();
    let ids: Vec<&str> = read.slides[0]
        .elements
        .iter()
        .map(SlideElement::id)
        .collect();
    assert_eq!(ids, ["same", "same-2"]);
}

/// A valid PNG of about `extra` bytes: a 2x2 picture with a padding chunk before its data.
fn padded_png(extra: usize, fill: u8) -> RichImage {
    let mut bytes = png();
    let at = 33; // signature (8) + IHDR chunk (25)
    let pad = png_chunk(b"tEXt", &vec![fill; extra]);
    bytes.splice(at..at, pad);
    RichImage::new(bytes, "").unwrap()
}

fn bg_slide(img: &RichImage) -> RichSlide {
    RichSlide {
        ext: SlideExt {
            background: Some(SlideBackground::Picture(img.clone())),
            ..SlideExt::default()
        },
        ..RichSlide::default()
    }
}

fn bg_ptr(s: &RichSlide) -> *const u8 {
    match &s.ext.background {
        Some(SlideBackground::Picture(i)) => i.bytes().as_ptr(),
        _ => std::ptr::null(),
    }
}

const BIG: usize = 30 * 1024 * 1024;

#[test]
fn many_slides_sharing_one_large_background_count_once_and_share_one_buffer() {
    let big = padded_png(BIG, b'a');
    let deck = deck_with((0..200).map(|_| bg_slide(&big)).collect());
    deck.validate().expect("a shared picture counts once");
    for (name, bytes) in [
        ("pptx", OoxmlCodec::pack_pptx_rich(&deck, false).unwrap()),
        ("odp", OdfCodec::pack_odp_rich(&deck).unwrap()),
    ] {
        let read = if name == "pptx" {
            OoxmlCodec::unpack_pptx_rich(&bytes).unwrap()
        } else {
            OdfCodec::unpack_odp_rich(&bytes).unwrap()
        };
        assert_eq!(read.slides.len(), 200, "{name}");
        let first = bg_ptr(&read.slides[0]);
        assert!(!first.is_null(), "{name}");
        assert!(
            read.slides.iter().all(|s| bg_ptr(s) == first),
            "{name}: one buffer for all pages"
        );
    }
}

#[test]
fn distinct_large_backgrounds_over_the_deck_limit_are_refused() {
    let imgs: Vec<RichImage> = (0..5u8).map(|i| padded_png(BIG, b'a' + i)).collect();
    let deck = deck_with(imgs.iter().map(bg_slide).collect());
    assert!(matches!(
        deck.validate(),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

fn slide_xml_of(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .unwrap()
        .into_iter()
        .map(|e| (e.path, e.data))
        .collect()
}

fn edit_part(
    parts: Vec<(String, Vec<u8>)>,
    path: &str,
    edit: impl Fn(String) -> String,
) -> Vec<(String, Vec<u8>)> {
    parts
        .into_iter()
        .map(|(p, d)| {
            if p == path {
                (p, edit(String::from_utf8(d).unwrap()).into_bytes())
            } else {
                (p, d)
            }
        })
        .collect()
}

#[test]
fn non_finite_and_extreme_numbers_are_ignored_instead_of_refusing_the_file() {
    let deck = deck_with(vec![full_slide()]);
    let pptx = slide_xml_of(&OoxmlCodec::pack_pptx_rich(&deck, false).unwrap());
    let hostile = edit_part(pptx, "ppt/slides/slide1.xml", |xml| {
        xml.replace("rot=\"1800000\"", "rot=\"inf\"")
            .replace("blurRad=\"50800\"", "blurRad=\"NaN\"")
            .replace("dir=\"2700000\"", "dir=\"1e999\"")
            .replace("<a:ln w=\"38100\">", "<a:ln w=\"inf\">")
    });
    let read =
        OoxmlCodec::unpack_pptx_rich(&ZipArchiveCodec::pack_compressed(&hostile).unwrap()).unwrap();
    for fx in &read.slides[0].ext.fx {
        assert!(fx.rotation_deg.is_finite() && (0.0..360.0).contains(&fx.rotation_deg));
        if let Some(s) = &fx.shadow {
            assert!(s.blur_pt.is_finite() && s.angle_deg.is_finite() && s.dist_pt.is_finite());
        }
    }

    let odp = slide_xml_of(&OdfCodec::pack_odp_rich(&deck).unwrap());
    let hostile = edit_part(odp, "content.xml", |xml| {
        let at = xml.find("rotate (").expect("a rotated frame");
        let end = xml[at..].find(')').unwrap() + at;
        let xml = format!("{}rotate (1e308{}", &xml[..at], &xml[end..]);
        // The shadow offset overflows once it is turned into points.
        let key = "draw:shadow-offset-x=\"";
        let from = xml.find(key).expect("a shadow") + key.len();
        let to = xml[from..].find('"').unwrap() + from;
        format!("{}1e308cm{}", &xml[..from], &xml[to..])
    });
    let read =
        OdfCodec::unpack_odp_rich(&ZipArchiveCodec::pack_compressed(&hostile).unwrap()).unwrap();
    for fx in &read.slides[0].ext.fx {
        assert!(fx.rotation_deg.is_finite() && (0.0..360.0).contains(&fx.rotation_deg));
        if let Some(s) = &fx.shadow {
            assert!(s.dist_pt.is_finite() && s.angle_deg.is_finite());
        }
    }
}

const WRAP_SP: &str = "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"100000\" cy=\"100000\"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>t</a:t></a:r></a:p></p:txBody></p:sp>";

fn group_xml(id: usize, name: &str, inner: &str) -> String {
    format!(
        "<p:grpSp><p:nvGrpSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{inner}</p:grpSp>"
    )
}

fn sp_xml(id: usize, name: &str) -> String {
    WRAP_SP
        .replace("{id}", &id.to_string())
        .replace("{name}", name)
}

fn with_tree(tree: String) -> RichSlideDeck {
    let deck = deck_with(vec![RichSlide {
        layout: SlideMasterLayout::Blank,
        ..RichSlide::default()
    }]);
    let parts = slide_xml_of(&OoxmlCodec::pack_pptx_rich(&deck, false).unwrap());
    let edited = edit_part(parts, "ppt/slides/slide1.xml", |xml| {
        let at = xml.find("</p:spTree>").unwrap();
        format!("{}{tree}{}", &xml[..at], &xml[at..])
    });
    OoxmlCodec::unpack_pptx_rich(&ZipArchiveCodec::pack_compressed(&edited).unwrap()).unwrap()
}

#[test]
fn groups_nested_or_numerous_beyond_the_limits_are_opened_not_refused() {
    let mut tree = group_xml(10, "inner", &(sp_xml(11, "a") + &sp_xml(12, "b")));
    for n in 0..12 {
        tree = group_xml(
            100 + n,
            &format!("wrap{n}"),
            &(tree + &sp_xml(200 + n, &format!("e{n}"))),
        );
    }
    let read = with_tree(tree);
    let s = &read.slides[0];
    assert_eq!(s.elements.len(), 14, "no element is lost");
    assert!(s.ext.groups.len() <= 13);
    let known: std::collections::HashSet<&str> = s
        .ext
        .groups
        .iter()
        .map(|g| g.id.as_str())
        .chain(s.elements.iter().map(SlideElement::id))
        .collect();
    assert!(s
        .ext
        .groups
        .iter()
        .all(|g| g.members.len() >= 2 && g.members.iter().all(|m| known.contains(m.as_str()))));

    let many: String = (0..70)
        .map(|n| {
            group_xml(
                1000 + n,
                &format!("g{n}"),
                &(sp_xml(2000 + 2 * n, &format!("x{n}")) + &sp_xml(2001 + 2 * n, &format!("y{n}"))),
            )
        })
        .collect();
    let read = with_tree(many);
    let s = &read.slides[0];
    assert_eq!(s.elements.len(), 140);
    assert!(s.ext.groups.len() <= 64, "{} groups", s.ext.groups.len());
}

#[test]
fn a_group_that_is_not_contiguous_keeps_the_draw_order() {
    let elements = vec![
        text_box("a", rect(0.0, 0.0, 50.0, 20.0), "가"),
        text_box("x", rect(60.0, 0.0, 50.0, 20.0), "엑스"),
        text_box("b", rect(120.0, 0.0, 50.0, 20.0), "나"),
    ];
    let slide = RichSlide {
        layout: SlideMasterLayout::Blank,
        elements,
        ext: SlideExt {
            groups: vec![SlideGroup {
                id: "g".into(),
                members: vec!["a".into(), "b".into()],
            }],
            ..SlideExt::default()
        },
        ..RichSlide::default()
    };
    let read = OoxmlCodec::unpack_pptx_rich(
        &OoxmlCodec::pack_pptx_rich(&deck_with(vec![slide.clone()]), false).unwrap(),
    )
    .unwrap();
    let ids: Vec<&str> = read.slides[0]
        .elements
        .iter()
        .map(SlideElement::id)
        .collect();
    assert_eq!(ids, ["a", "x", "b"], "x stays between a and b");
    let read = OdfCodec::unpack_odp_rich(
        &OdfCodec::pack_odp_rich(&deck_with(vec![slide]).clone()).unwrap(),
    )
    .unwrap();
    let ids: Vec<&str> = read.slides[0]
        .elements
        .iter()
        .map(SlideElement::id)
        .collect();
    assert_eq!(ids, ["a", "x", "b"]);
}

#[test]
fn odp_animation_ids_are_unique_in_the_whole_document() {
    let page = || {
        let mut s = full_slide();
        s.ext.hidden = false;
        s
    };
    let bytes = OdfCodec::pack_odp_rich(&deck_with(vec![page(), page()])).unwrap();
    let content = String::from_utf8(
        slide_xml_of(&bytes)
            .into_iter()
            .find(|(p, _)| p == "content.xml")
            .unwrap()
            .1,
    )
    .unwrap();
    let ids: Vec<&str> = content
        .split("xml:id=\"")
        .skip(1)
        .map(|r| r.split('"').next().unwrap())
        .collect();
    let unique: std::collections::HashSet<&&str> = ids.iter().collect();
    assert!(ids.len() >= 6);
    assert_eq!(unique.len(), ids.len(), "{ids:?}");
}

#[test]
fn colors_are_written_as_six_digit_upper_case_hex() {
    let mut slide = full_slide();
    slide.ext.background = Some(SlideBackground::Solid("#aabbcc".into()));
    slide.ext.fx[1].shadow.as_mut().unwrap().color = "abcdef".into();
    let deck = deck_with(vec![slide]);
    let pptx = slide_xml_of(&OoxmlCodec::pack_pptx_rich(&deck, false).unwrap());
    let xml = String::from_utf8(
        pptx.into_iter()
            .find(|(p, _)| p == "ppt/slides/slide1.xml")
            .unwrap()
            .1,
    )
    .unwrap();
    assert!(xml.contains("<a:srgbClr val=\"AABBCC\"/>"));
    assert!(xml.contains("<a:srgbClr val=\"ABCDEF\">"));
    assert!(!xml.contains("srgbClr val=\"#"));
}

fn page_count(pdf: &[u8]) -> usize {
    let text = String::from_utf8_lossy(pdf);
    text.matches("/Type /Page").count() - text.matches("/Type /Pages").count()
}

#[test]
fn pdf_export_leaves_hidden_slides_out_unless_all_are_hidden() {
    let slide = |hidden: bool| RichSlide {
        title: "쪽".into(),
        ext: SlideExt {
            hidden,
            ..SlideExt::default()
        },
        ..RichSlide::default()
    };
    let pdf = |slides: Vec<RichSlide>| {
        easyoffice_formats::encode_slide_pdf(&deck_with(slides), "").unwrap()
    };
    assert_eq!(
        page_count(&pdf(vec![slide(false), slide(true), slide(false)])),
        2
    );
    assert_eq!(
        page_count(&pdf(vec![slide(true), slide(true)])),
        2,
        "a deck of hidden slides is exported whole"
    );
}

#[test]
fn pptx_group_rotation_turns_its_members_about_the_group_center() {
    let elements = vec![
        text_box("t1", rect(0.0, 100.0, 100.0, 40.0), "가"),
        text_box("t2", rect(200.0, 100.0, 100.0, 40.0), "나"),
    ];
    let slide = RichSlide {
        layout: SlideMasterLayout::Blank,
        elements,
        ext: SlideExt {
            groups: vec![SlideGroup {
                id: "g".into(),
                members: vec!["t1".into(), "t2".into()],
            }],
            ..SlideExt::default()
        },
        ..RichSlide::default()
    };
    let bytes = OoxmlCodec::pack_pptx_rich(&deck_with(vec![slide]), false).unwrap();
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let out: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path != "ppt/slides/slide1.xml" {
                return (e.path, e.data);
            }
            let xml = String::from_utf8(e.data).unwrap();
            // The first `p:grpSpPr` is the shape tree's own; the group's comes after `<p:grpSp>`.
            let (head, group) = xml.split_once("<p:grpSp>").unwrap();
            let group = group.replacen(
                "<p:grpSpPr><a:xfrm>",
                "<p:grpSpPr><a:xfrm rot=\"5400000\">",
                1,
            );
            (e.path, format!("{head}<p:grpSp>{group}").into_bytes())
        })
        .collect();
    let read =
        OoxmlCodec::unpack_pptx_rich(&ZipArchiveCodec::pack_compressed(&out).unwrap()).unwrap();
    let s = &read.slides[0];
    // The group's center is (150, 120); a quarter turn clockwise puts the left box above it.
    let center = |i: usize| {
        let r = s.elements[i].rect();
        (r.x + r.w / 2.0, r.y + r.h / 2.0)
    };
    let (x, y) = center(0);
    assert!(
        (x - 150.0).abs() < 0.1 && (y - 20.0).abs() < 0.1,
        "left box moved to {x},{y}"
    );
    let (x, y) = center(1);
    assert!(
        (x - 150.0).abs() < 0.1 && (y - 220.0).abs() < 0.1,
        "right box moved to {x},{y}"
    );
    assert_eq!(s.ext.fx_of("t1").map(|f| f.rotation_deg), Some(90.0));
    assert_eq!(s.ext.groups.len(), 1);
}

#[test]
fn odp_roundtrip_keeps_every_extension() {
    let deck = deck_with(vec![full_slide()]);
    let bytes = OdfCodec::pack_odp_rich(&deck).unwrap();
    let read = OdfCodec::unpack_odp_rich(&bytes).unwrap();
    assert_ext_roundtrip(&read, &deck.slides[0]);
}
