//! PD-402: a slide size that is none of the presets (`p:sldSz` of any size, ODP page width and height)
//! survives a PPTX and an ODP round trip exactly, and a size outside the allowed range is rejected.

use easyoffice_formats::{
    OdfCodec, OoxmlCodec, RichSlide, RichSlideDeck, SlideAspect, SlideBox, SlideElement,
    SlideMasterLayout, SlideOrientation, SlideTextAlign, ZipArchiveCodec, ZipSecurityPolicy,
    MAX_SLIDE_SIZE_EMU, MIN_SLIDE_SIZE_EMU,
};

fn deck(aspect: SlideAspect, orientation: SlideOrientation) -> RichSlideDeck {
    RichSlideDeck {
        aspect,
        orientation,
        slides: vec![RichSlide {
            layout: SlideMasterLayout::Blank,
            elements: vec![SlideElement::TextBox {
                id: "t".into(),
                rect: SlideBox {
                    x: 100.0,
                    y: 50.0,
                    w: 400.0,
                    h: 60.0,
                },
                text: "크기".into(),
                font_size_pt: 20.0,
                bold: false,
                italic: false,
                underline: false,
                align: SlideTextAlign::Left,
                color: None,
                fill: None,
                role: None,
            }],
            ..RichSlide::default()
        }],
        ..RichSlideDeck::default()
    }
}

fn text_rect(deck: &RichSlideDeck) -> SlideBox {
    match &deck.slides[0].elements[0] {
        SlideElement::TextBox { rect, .. } => *rect,
        other => panic!("not a text box: {other:?}"),
    }
}

fn with_size(pptx: &[u8], cx: u64, cy: u64) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(pptx, &ZipSecurityPolicy::default()).unwrap();
    let edited: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == "ppt/presentation.xml" {
                let xml = String::from_utf8(e.data).unwrap();
                let start = xml.find("<p:sldSz").unwrap();
                let end = start + xml[start..].find("/>").unwrap() + 2;
                let patched = format!(
                    "{}<p:sldSz cx=\"{cx}\" cy=\"{cy}\"/>{}",
                    &xml[..start],
                    &xml[end..]
                );
                (e.path, patched.into_bytes())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    ZipArchiveCodec::pack(&edited).unwrap()
}

#[test]
fn a_custom_size_round_trips_through_pptx_exactly() {
    for (cx, cy) in [(9_144_000, 5_143_500), (9_144_000, 5_715_000), (7_000_001, 3_333_333)] {
        let aspect = SlideAspect::custom(cx, cy).unwrap();
        let src = deck(aspect, SlideOrientation::Landscape);
        let bytes = OoxmlCodec::pack_pptx_rich(&src, false).unwrap();
        let back = OoxmlCodec::unpack_pptx_rich(&bytes).unwrap();
        assert_eq!(back.aspect, aspect, "{cx}x{cy}");
        assert_eq!(back.stage_dimensions_emu(), (cx, cy));
        assert_eq!(text_rect(&back), text_rect(&src), "content is not scaled");
    }
}

#[test]
fn a_portrait_custom_size_keeps_orientation_and_both_sides() {
    let aspect = SlideAspect::custom(8_000_000, 5_000_000).unwrap();
    let src = deck(aspect, SlideOrientation::Portrait);
    assert_eq!(src.stage_dimensions_emu(), (5_000_000, 8_000_000));
    let bytes = OoxmlCodec::pack_pptx_rich(&src, false).unwrap();
    let back = OoxmlCodec::unpack_pptx_rich(&bytes).unwrap();
    assert_eq!(back.orientation, SlideOrientation::Portrait);
    assert_eq!(back.stage_dimensions_emu(), (5_000_000, 8_000_000));
    assert_eq!(back.aspect, aspect);
}

#[test]
fn a_preset_size_stays_a_preset_and_a_pptxgenjs_size_becomes_custom() {
    let bytes = OoxmlCodec::pack_pptx_rich(
        &deck(SlideAspect::Widescreen16x9, SlideOrientation::Landscape),
        false,
    )
    .unwrap();
    assert_eq!(
        OoxmlCodec::unpack_pptx_rich(&bytes).unwrap().aspect,
        SlideAspect::Widescreen16x9
    );
    // PptxGenJS `LAYOUT_16x9` is 10 in x 5.625 in, not the 13.333 in x 7.5 in wide preset.
    let gen = with_size(&bytes, 9_144_000, 5_143_500);
    let back = OoxmlCodec::unpack_pptx_rich(&gen).unwrap();
    assert_eq!(back.aspect, SlideAspect::Custom { cx: 9_144_000, cy: 5_143_500 });
}

#[test]
fn a_custom_size_round_trips_through_odp_within_the_unit_rounding() {
    let aspect = SlideAspect::custom(9_144_000, 5_143_500).unwrap();
    let src = deck(aspect, SlideOrientation::Landscape);
    let back = OdfCodec::unpack_odp_rich(&OdfCodec::pack_odp_rich(&src).unwrap()).unwrap();
    let SlideAspect::Custom { cx, cy } = back.aspect else {
        panic!("not custom: {:?}", back.aspect);
    };
    assert!((i64::from(cx) - 9_144_000).abs() < 400, "cx {cx}");
    assert!((i64::from(cy) - 5_143_500).abs() < 400, "cy {cy}");
    assert_eq!(back.orientation, SlideOrientation::Landscape);
}

#[test]
fn sizes_outside_the_range_are_rejected_not_clamped() {
    assert!(SlideAspect::custom(MIN_SLIDE_SIZE_EMU - 1, 914_400).is_none());
    assert!(SlideAspect::custom(914_400, MAX_SLIDE_SIZE_EMU + 1).is_none());
    assert!(SlideAspect::custom(u64::MAX, u64::MAX).is_none());
    for wire in [
        "custom:1x1",
        "custom:99999999999x914400",
        "custom:914400x",
        "custom:x914400",
        "custom:-5x914400",
        "custom:1e6x1e6",
        "custom:914400",
        "custom",
    ] {
        assert!(SlideAspect::parse(wire).is_none(), "{wire}");
    }
    assert_eq!(
        SlideAspect::parse("custom:9144000x5143500"),
        SlideAspect::custom(9_144_000, 5_143_500)
    );
    let aspect = SlideAspect::custom(9_144_000, 5_143_500).unwrap();
    assert_eq!(SlideAspect::parse(&aspect.to_wire()), Some(aspect));

    let base = OoxmlCodec::pack_pptx_rich(
        &deck(SlideAspect::Widescreen16x9, SlideOrientation::Landscape),
        false,
    )
    .unwrap();
    for (cx, cy) in [
        (1, 1),
        (MIN_SLIDE_SIZE_EMU - 1, 6_858_000),
        (12_192_000, MAX_SLIDE_SIZE_EMU + 1),
        (u64::from(u32::MAX) * 4, 6_858_000),
    ] {
        let hostile = with_size(&base, cx, cy);
        assert!(
            OoxmlCodec::unpack_pptx_rich(&hostile).is_err(),
            "{cx}x{cy} must be rejected"
        );
    }
    // The limits themselves are accepted.
    for (cx, cy) in [
        (MIN_SLIDE_SIZE_EMU, MIN_SLIDE_SIZE_EMU),
        (MAX_SLIDE_SIZE_EMU, MIN_SLIDE_SIZE_EMU),
    ] {
        assert!(OoxmlCodec::unpack_pptx_rich(&with_size(&base, cx, cy)).is_ok());
    }
}
