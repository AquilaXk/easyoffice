use super::*;

#[test]
fn odt_resolves_parent_style_inheritance_and_cycle_guard() {
    let styles_xml = r##"<office:document-styles>
            <office:styles>
                <style:style style:name="ParentPara" style:family="paragraph">
                    <style:paragraph-properties fo:text-align="center" fo:line-height="150%"/>
                    <style:text-properties fo:font-weight="bold" fo:color="#112233"/>
                </style:style>
                <style:style style:name="Cycle1" style:parent-style-name="Cycle2"/>
                <style:style style:name="Cycle2" style:parent-style-name="Cycle1"/>
            </office:styles>
        </office:document-styles>"##;
    let auto_xml = r#"<office:document-content>
            <office:automatic-styles>
                <style:style style:name="P1" style:parent-style-name="ParentPara">
                    <style:text-properties fo:font-style="italic"/>
                </style:style>
            </office:automatic-styles>
        </office:document-content>"#;
    let mut styles = HashMap::new();
    let mut lists = HashMap::new();
    let mut table_styles = OdtTableStylesMap::default();
    read_styles_into(styles_xml, &mut styles, &mut lists, &mut table_styles).unwrap();
    read_styles_into(auto_xml, &mut styles, &mut lists, &mut table_styles).unwrap();
    resolve_style_parents(&mut styles);
    let p1 = styles.get("P1").expect("P1 resolved");
    assert_eq!(p1.align, Some(RichAlignment::Center));
    assert_eq!(p1.line_percent, Some(150));
    assert!(p1.fmt.bold);
    assert!(p1.fmt.italic);
    assert_eq!(p1.fmt.color.as_deref(), Some("112233"));
}

#[test]
fn hostile_on_frame_charges_image_budget_before_clone_or_decode() {
    let entry = ZipArchiveEntry {
        path: "Pictures/bad.bin".to_string(),
        data: vec![0u8; 1024],
        crc32: 0,
    };
    let mut by_path = HashMap::new();
    by_path.insert("Pictures/bad.bin", &entry);
    let styles = HashMap::new();
    let lists = HashMap::new();
    let table_styles = OdtTableStylesMap::default();
    let mut walker = OdtWalker::new(&styles, &lists, &table_styles);
    walker.paras.push(Para {
        runs: Vec::new(),
        style: StyleProps::default(),
        outline: 0,
        toc: None,
        bookmark: None,
        toc_dots: false,
        base: CharFmt::default(),
        images: Vec::new(),
    });
    let frame_xml = r#"<draw:frame><draw:image xlink:href="Pictures/bad.bin"/></draw:frame>"#;
    let events: Vec<XmlEvent<'_>> = XmlReader::new(frame_xml)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for _ in 0..crate::MAX_IMAGES_PER_DOCUMENT {
        walker.on_frame(&events, &by_path).unwrap();
    }
    assert!(matches!(
        walker.on_frame(&events, &by_path),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn hostile_odt_runs_exceeding_decode_budget_fail_closed() {
    let styles = HashMap::new();
    let lists = HashMap::new();
    let table_styles = OdtTableStylesMap::default();
    let mut walker = OdtWalker::new(&styles, &lists, &table_styles);
    walker.text_budget = DecodeBudget::with_limit(256);
    let xml = format!(
        "<office:document-content>{}</office:document-content>",
        "<text:p>hello</text:p>".repeat(8)
    );
    let mut reader = XmlReader::new(&xml);
    let mut hit = None;
    while let Some(ev) = reader.next_event().unwrap() {
        let res = match ev {
            XmlEvent::Start { name, ref attrs } => walker.on_start(name, attrs).map(|_| ()),
            XmlEvent::Text(text) => walker.push_inline(&text),
            XmlEvent::End(name) => walker.on_end(name),
        };
        if let Err(err) = res {
            hit = Some(err);
            break;
        }
    }
    assert!(matches!(
        hit,
        Some(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn hostile_odt_table_paragraphs_exceeding_decode_budget_fail_closed() {
    let styles = HashMap::new();
    let lists = HashMap::new();
    let table_styles = OdtTableStylesMap::default();
    let mut walker = OdtWalker::new(&styles, &lists, &table_styles);
    walker.text_budget = DecodeBudget::with_limit(512);
    let xml = format!(
        "<office:document-content><table:table><table:table-row><table:table-cell>{}</table:table-cell></table:table-row></table:table></office:document-content>",
        "<text:p/>".repeat(16)
    );
    let mut reader = XmlReader::new(&xml);
    let mut hit = None;
    while let Some(ev) = reader.next_event().unwrap() {
        let res = match ev {
            XmlEvent::Start { name, ref attrs } => walker.on_start(name, attrs).map(|_| ()),
            XmlEvent::Text(text) => walker.push_inline(&text),
            XmlEvent::End(name) => walker.on_end(name),
        };
        if let Err(err) = res {
            hit = Some(err);
            break;
        }
    }
    assert!(matches!(
        hit,
        Some(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}
