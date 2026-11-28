//! PD-398: the slide transitions with their side, the automatic advance and the repeat flag of the show, the text
//! typefaces and the theme color scheme survive a PPTX round trip (the transitions and the advance also an ODP one), and
//! hostile values in those parts stay inside their limits.

use easyoffice_formats::{
    AnimDir, ColorScheme, DeckSettings, ElementFx, OdfCodec, OoxmlCodec, RichSlide, RichSlideDeck,
    SlideBox, SlideElement, SlideExt, SlideMasterLayout, SlideTextAlign, SlideTransition,
    ZipArchiveCodec, ZipSecurityPolicy,
};

fn text_box(id: &str) -> SlideElement {
    SlideElement::TextBox {
        id: id.into(),
        rect: SlideBox {
            x: 40.0,
            y: 60.0,
            w: 300.0,
            h: 80.0,
        },
        text: "글꼴".into(),
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

fn slide(transition: SlideTransition, dir: Option<AnimDir>, advance: Option<u32>) -> RichSlide {
    RichSlide {
        layout: SlideMasterLayout::Blank,
        transition,
        ext: SlideExt {
            transition_dir: dir,
            advance_after_ms: advance,
            ..SlideExt::default()
        },
        ..RichSlide::default()
    }
}

fn parts(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .unwrap()
        .into_iter()
        .map(|e| (e.path, e.data))
        .collect()
}

fn edit(
    all: Vec<(String, Vec<u8>)>,
    path: &str,
    f: impl Fn(String) -> String,
) -> Vec<(String, Vec<u8>)> {
    all.into_iter()
        .map(|(p, d)| {
            if p == path {
                (p, f(String::from_utf8(d).unwrap()).into_bytes())
            } else {
                (p, d)
            }
        })
        .collect()
}

#[test]
fn every_transition_keeps_its_side_and_the_advance_time_in_pptx_and_odp() {
    let cases = [
        (SlideTransition::Fade, None, None),
        (SlideTransition::Push, Some(AnimDir::Bottom), Some(3000)),
        (SlideTransition::Wipe, Some(AnimDir::Right), Some(1500)),
        (SlideTransition::Split, Some(AnimDir::Top), None),
        (SlideTransition::Cover, Some(AnimDir::Left), Some(500)),
        (SlideTransition::Pull, Some(AnimDir::Top), None),
        (SlideTransition::Zoom, None, Some(2000)),
        (SlideTransition::RandomBar, Some(AnimDir::Left), None),
        (SlideTransition::None, None, Some(4000)),
    ];
    let deck = RichSlideDeck {
        slides: cases.iter().map(|&(t, d, a)| slide(t, d, a)).collect(),
        ..RichSlideDeck::default()
    };
    let pptx =
        OoxmlCodec::unpack_pptx_rich(&OoxmlCodec::pack_pptx_rich(&deck, false).unwrap()).unwrap();
    let odp = OdfCodec::unpack_odp_rich(&OdfCodec::pack_odp_rich(&deck).unwrap()).unwrap();
    for (name, read) in [("pptx", pptx), ("odp", odp)] {
        for (i, &(t, d, a)) in cases.iter().enumerate() {
            let s = &read.slides[i];
            assert_eq!(s.transition, t, "{name} slide {i}");
            assert_eq!(s.ext.advance_after_ms, a, "{name} slide {i}");
            // A split and the bars keep their axis, and the sides are exact where the effect has one.
            // The ODF wipe has an axis only; the sides of a push, a cover and a pull are exact there.
            let exact = name == "pptx"
                || matches!(
                    t,
                    SlideTransition::Push | SlideTransition::Cover | SlideTransition::Pull
                );
            if t.has_side() && exact {
                assert_eq!(s.ext.transition_dir, d, "{name} slide {i}");
            }
        }
    }
}

#[test]
fn a_click_that_does_not_advance_and_the_repeat_flag_are_kept() {
    let mut s = slide(SlideTransition::Fade, None, Some(1000));
    s.ext.no_click_advance = true;
    let deck = RichSlideDeck {
        slides: vec![s],
        settings: DeckSettings {
            show_loop: true,
            manual_advance: true,
            color_scheme: None,
        },
        ..RichSlideDeck::default()
    };
    let bytes = OoxmlCodec::pack_pptx_rich(&deck, false).unwrap();
    let all = parts(&bytes);
    let slide_xml = String::from_utf8(
        all.iter()
            .find(|(p, _)| p == "ppt/slides/slide1.xml")
            .unwrap()
            .1
            .clone(),
    )
    .unwrap();
    assert!(
        slide_xml.contains("<p:transition advClick=\"0\" advTm=\"1000\"><p:fade/></p:transition>")
    );
    let props = String::from_utf8(
        all.iter()
            .find(|(p, _)| p == "ppt/presProps.xml")
            .unwrap()
            .1
            .clone(),
    )
    .unwrap();
    assert!(props.contains("<p:showPr loop=\"1\" useTimings=\"0\">"));
    let read = OoxmlCodec::unpack_pptx_rich(&bytes).unwrap();
    assert!(read.settings.show_loop && read.settings.manual_advance);
    assert!(read.slides[0].ext.no_click_advance);
}

#[test]
fn text_typefaces_and_the_color_scheme_round_trip() {
    let mut fx = ElementFx::new("t1");
    fx.font = Some("Georgia".into());
    fx.east_font = Some("맑은 고딕".into());
    let mut s = slide(SlideTransition::None, None, None);
    s.elements = vec![text_box("t1")];
    s.ext.fx = vec![fx];
    let colors: Vec<String> = [
        "000000", "FFFFFF", "44546A", "E7E6E6", "4472C4", "ED7D31", "A5A5A5", "FFC000", "5B9BD5",
        "70AD47", "0563C1", "954F72",
    ]
    .iter()
    .map(|c| c.to_string())
    .collect();
    let scheme = ColorScheme::checked("Office", &colors).unwrap();
    let deck = RichSlideDeck {
        slides: vec![s],
        settings: DeckSettings {
            show_loop: false,
            manual_advance: false,
            color_scheme: Some(scheme.clone()),
        },
        ..RichSlideDeck::default()
    };
    let bytes = OoxmlCodec::pack_pptx_rich(&deck, false).unwrap();
    let read = OoxmlCodec::unpack_pptx_rich(&bytes).unwrap();
    assert_eq!(read.settings.color_scheme, Some(scheme));
    let fx = read.slides[0]
        .ext
        .fx_of(read.slides[0].elements[0].id())
        .expect("fx");
    assert_eq!(fx.font.as_deref(), Some("Georgia"));
    assert_eq!(fx.east_font.as_deref(), Some("맑은 고딕"));
}

#[test]
fn hostile_transition_values_are_dropped_not_followed() {
    let deck = RichSlideDeck {
        slides: vec![slide(SlideTransition::Push, None, None)],
        ..RichSlideDeck::default()
    };
    let all = parts(&OoxmlCodec::pack_pptx_rich(&deck, false).unwrap());
    for bad in [
        "99999999999999999999",
        "-5",
        "4294967295",
        "NaN",
        "86400001",
    ] {
        let edited = edit(all.clone(), "ppt/slides/slide1.xml", |xml| {
            xml.replace(
                "<p:transition>",
                &format!("<p:transition advTm=\"{bad}\" spd=\"zzz\">"),
            )
        });
        let read =
            OoxmlCodec::unpack_pptx_rich(&ZipArchiveCodec::pack_compressed(&edited).unwrap())
                .unwrap();
        assert_eq!(read.slides[0].ext.advance_after_ms, None, "advTm {bad}");
        assert_eq!(read.slides[0].transition, SlideTransition::Push);
    }
    let edited = edit(all, "ppt/slides/slide1.xml", |xml| {
        xml.replace(
            "<p:push dir=\"l\"/>",
            "<p:push dir=\"sideways\"/><p:unknownEffect/>",
        )
    });
    let read =
        OoxmlCodec::unpack_pptx_rich(&ZipArchiveCodec::pack_compressed(&edited).unwrap()).unwrap();
    assert_eq!(read.slides[0].transition, SlideTransition::Push);
    assert_eq!(read.slides[0].ext.transition_dir, None);
}

#[test]
fn a_theme_with_a_flood_of_colors_is_refused_or_read_with_twelve() {
    let deck = RichSlideDeck {
        slides: vec![slide(SlideTransition::None, None, None)],
        ..RichSlideDeck::default()
    };
    let all = parts(&OoxmlCodec::pack_pptx_rich(&deck, false).unwrap());
    let flood: String = (0..20_000)
        .map(|_| "<a:accent1><a:srgbClr val=\"123456\"/></a:accent1>")
        .collect();
    let edited = edit(all, "ppt/theme/theme1.xml", |xml| {
        xml.replace("</a:clrScheme>", &format!("{flood}</a:clrScheme>"))
    });
    // Refused (event budget) or read with the one value of each slot: a flood never grows the scheme.
    if let Ok(read) =
        OoxmlCodec::unpack_pptx_rich(&ZipArchiveCodec::pack_compressed(&edited).unwrap())
    {
        if let Some(s) = read.settings.color_scheme {
            assert_eq!(s.colors.len(), 12);
        }
    }
}
