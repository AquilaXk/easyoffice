//! PD-375: the OpenType features and the line-grid switch of a run (the 고급 tab of the Font dialog) survive DOCX, the formats
//! that cannot keep them say so, and hostile `w14:*` values stay inside the model's bounds.

use easyoffice_formats::{
    pages_write_loss, CharAdvanced, DecodedContent, DocumentFormat, OpenType, RichBlock,
    RichDocument, RichParagraph, RichRun, UniversalFormatConverter, ZipArchiveCodec,
    ZipSecurityPolicy,
};

fn run(text: &str, advanced: CharAdvanced) -> RichRun {
    RichRun {
        text: text.to_string(),
        advanced,
        ..RichRun::default()
    }
}

fn document(runs: Vec<RichRun>) -> RichDocument {
    RichDocument {
        title: "OpenType".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs,
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    }
}

fn decode(bytes: &[u8], name: &str) -> RichDocument {
    match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .expect("decode")
        .content
    {
        DecodedContent::Document(d) => d,
        other => panic!("expected document: {other:?}"),
    }
}

fn advanced_of(doc: &RichDocument) -> Vec<(String, CharAdvanced)> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.runs.iter())
        .map(|r| (r.text.clone(), r.advanced))
        .collect()
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).expect("unpack");
    let entry = entries.iter().find(|e| e.path == path).expect("part");
    String::from_utf8(entry.data.clone()).expect("utf8")
}

fn features() -> CharAdvanced {
    CharAdvanced {
        open_type: OpenType {
            ligatures: 0b0011,
            num_spacing: 2,
            num_form: 1,
            style_sets: 1 | 1 << 4,
            contextual_alts: true,
        },
        ..CharAdvanced::default()
    }
}

fn sample() -> RichDocument {
    document(vec![
        run("특성", features()),
        run(
            "눈금 끔",
            CharAdvanced {
                grid_snap_off: true,
                ..CharAdvanced::default()
            },
        ),
        run("보통", CharAdvanced::default()),
    ])
}

/// `docx` with the first run's `w:rPr` content replaced by `props`.
fn docx_with(props: &str) -> Vec<u8> {
    let bytes = UniversalFormatConverter::encode_rich(
        &document(vec![run("x", features())]),
        DocumentFormat::Docx,
        None,
    )
    .unwrap();
    let xml = part(&bytes, "word/document.xml");
    let start = xml.find("<w:rPr>").unwrap();
    let end = xml.find("</w:rPr>").unwrap();
    let hostile = format!("{}<w:rPr>{props}{}", &xml[..start], &xml[end..]);
    let entries: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default())
            .unwrap()
            .into_iter()
            .map(|e| {
                if e.path == "word/document.xml" {
                    (e.path, hostile.clone().into_bytes())
                } else {
                    (e.path, e.data)
                }
            })
            .collect();
    ZipArchiveCodec::pack_compressed(&entries).unwrap()
}

fn only(doc: &RichDocument) -> CharAdvanced {
    let back = advanced_of(doc);
    assert_eq!(back.len(), 1, "{back:?}");
    back[0].1
}

#[test]
fn docx_keeps_the_features_and_the_grid_switch() {
    let bytes =
        UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Docx, None).unwrap();
    let xml = part(&bytes, "word/document.xml");
    for word in [
        "<w14:ligatures w14:val=\"standardContextual\"/>",
        "<w14:numForm w14:val=\"lining\"/>",
        "<w14:numSpacing w14:val=\"tabular\"/>",
        "<w14:styleSet w14:id=\"1\"/><w14:styleSet w14:id=\"5\"/>",
        "<w14:cntxtAlts/>",
        "<w:snapToGrid w:val=\"0\"/>",
    ] {
        assert!(xml.contains(word), "{word} in {xml}");
    }
    let back = advanced_of(&decode(&bytes, "a.docx"));
    assert!(back.contains(&("특성".to_string(), features())), "{back:?}");
    assert!(back.iter().any(|(t, a)| t == "눈금 끔" && a.grid_snap_off));
    assert!(back.iter().any(|(t, a)| t == "보통" && a.is_none()));
}

#[test]
fn the_formats_without_a_counterpart_drop_them_and_pages_says_so() {
    let loss = pages_write_loss(&sample());
    assert_eq!(loss.formats, 2, "two runs carry a setting");
    for (fmt, name) in [
        (DocumentFormat::Odt, "a.odt"),
        (DocumentFormat::Rtf, "a.rtf"),
        (DocumentFormat::Hwpx, "a.hwpx"),
        (DocumentFormat::Pages, "a.pages"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&sample(), fmt, None).unwrap();
        let back = advanced_of(&decode(&bytes, name));
        assert!(
            back.iter()
                .all(|(_, a)| a.open_type.is_none() && !a.grid_snap_off),
            "{name}: {back:?}"
        );
    }
}

#[test]
fn hostile_w14_values_read_as_not_set() {
    for (props, expected) in [
        (
            r#"<w14:ligatures w14:val="Standard"/><w14:numSpacing w14:val="9"/><w14:numForm w14:val=""/>"#,
            OpenType::default(),
        ),
        (
            r#"<w14:ligatures w14:val="standardstandardstandardstandard"/>"#,
            OpenType::default(),
        ),
        (r#"<w14:ligatures/>"#, OpenType::default()),
        (
            r#"<w14:stylisticSets><w14:styleSet w14:id="0"/><w14:styleSet w14:id="21"/><w14:styleSet w14:id="4294967296"/><w14:styleSet w14:id="-1"/><w14:styleSet/></w14:stylisticSets>"#,
            OpenType::default(),
        ),
        (
            r#"<w14:stylisticSets><w14:styleSet w14:id="20"/><w14:styleSet w14:id="1"/><w14:styleSet w14:id="1" w14:val="0"/></w14:stylisticSets>"#,
            OpenType {
                style_sets: 1 << 19,
                ..OpenType::default()
            },
        ),
        (
            r#"<w14:ligatures w14:val="all"/><w14:cntxtAlts w14:val="0"/>"#,
            OpenType {
                ligatures: 0b1111,
                ..OpenType::default()
            },
        ),
    ] {
        let doc = decode(&docx_with(props), "h.docx");
        assert_eq!(only(&doc).open_type, expected, "{props}");
    }
}

#[test]
fn the_grid_switch_reads_on_unless_the_value_turns_it_off() {
    for (props, off) in [
        (r#"<w:snapToGrid/>"#, false),
        (r#"<w:snapToGrid w:val="1"/>"#, false),
        (r#"<w:snapToGrid w:val="true"/>"#, false),
        (r#"<w:snapToGrid w:val="0"/>"#, true),
        (r#"<w:snapToGrid w:val="false"/>"#, true),
        (r#"<w:snapToGrid w:val="off"/>"#, true),
    ] {
        let doc = decode(&docx_with(props), "h.docx");
        assert_eq!(only(&doc).grid_snap_off, off, "{props}");
    }
}
