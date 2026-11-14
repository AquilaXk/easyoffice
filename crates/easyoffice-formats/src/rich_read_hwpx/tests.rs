use super::*;

#[test]
fn a_hyperlink_command_is_read_in_the_three_known_forms() {
    for (command, expected) in [
        ("https://a.b/c?d=1;e=2", Some("https://a.b/c?d=1;e=2")),
        ("HYPERLINK \"https://a.b/x\"", Some("https://a.b/x")),
        ("http\\://a.b/x;1;0;0;", Some("http://a.b/x")),
        ("mailto\\:me@a.b;1;0;0;", Some("mailto:me@a.b")),
        ("javascript:alert(1)", None),
        ("HYPERLINK \"file:///etc/passwd\"", None),
        ("javascript\\:alert(1);1;0;0;", None),
        ("", None),
    ] {
        assert_eq!(link_from_command(command).as_deref(), expected, "{command}");
    }
}

#[test]
fn hostile_on_picture_charges_image_budget_before_clone_or_decode() {
    let bad_blob = vec![0u8; 1024];
    let entry = ZipArchiveEntry {
        path: "BinData/bad.bin".to_string(),
        data: bad_blob,
        crc32: 0,
    };
    let mut media = HwpxMedia {
        by_id: HashMap::new(),
    };
    media.by_id.insert("img1".to_string(), &entry);
    let mut walker = SectionWalker::default();
    walker.paras.push(ParaState::default());
    let pic_xml = r#"<hp:pic><hc:img binaryItemIDRef="img1"/></hp:pic>"#;
    let events: Vec<XmlEvent<'_>> = XmlReader::new(pic_xml)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut warnings = Vec::new();
    for _ in 0..crate::MAX_IMAGES_PER_DOCUMENT {
        walker.on_picture(&events, &media, &mut warnings).unwrap();
    }
    assert!(matches!(
        walker.on_picture(&events, &media, &mut warnings),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn hostile_section_runs_and_blocks_exceeding_decode_budget_fail_closed() {
    let mut walker = SectionWalker {
        text_budget: DecodeBudget::with_limit(256),
        ..Default::default()
    };
    let header = Header::default();
    let media = HwpxMedia {
        by_id: HashMap::new(),
    };
    let mut blocks = Vec::new();
    let mut warnings = Vec::new();
    let xml = format!(
        "<hs:sec>{}</hs:sec>",
        "<hp:p><hp:run><hp:t>hello</hp:t></hp:run></hp:p>".repeat(8)
    );
    assert!(matches!(
        read_section(
            &xml,
            &header,
            &media,
            &mut walker,
            &mut blocks,
            &mut warnings
        ),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}
