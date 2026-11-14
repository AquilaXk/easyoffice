use super::*;

#[test]
fn docx_style_inheritance_resolves_based_on_and_outline_lvl_and_breaks_cycles() {
    let styles_xml = r#"<w:styles>
        <w:style w:type="paragraph" w:styleId="BaseH2">
            <w:name w:val="Heading 2"/>
        </w:style>
        <w:style w:type="paragraph" w:styleId="CustomSub">
            <w:name w:val="Custom Subheading"/>
            <w:basedOn w:val="BaseH2"/>
        </w:style>
        <w:style w:type="paragraph" w:styleId="Outline3">
            <w:name w:val="Section Title"/>
            <w:pPr><w:outlineLvl w:val="2"/></w:pPr>
        </w:style>
        <w:style w:type="paragraph" w:styleId="CycleA">
            <w:basedOn w:val="CycleB"/>
        </w:style>
        <w:style w:type="paragraph" w:styleId="CycleB">
            <w:basedOn w:val="CycleA"/>
        </w:style>
    </w:styles>"#;
    let styles = read_styles(styles_xml).expect("styles parse");
    let numbering = Numbering::default();
    let resolver = Resolver {
        styles: &styles,
        numbering: &numbering,
    };
    assert_eq!(resolver.resolve_style_chain("CustomSub"), (2, None));
    assert_eq!(resolver.resolve_style_chain("Outline3"), (3, None));
    assert_eq!(resolver.resolve_style_chain("CycleA"), (0, None));
}

#[test]
fn hostile_docx_runs_and_blocks_exceeding_decode_budget_fail_closed() {
    let mut walker = DocxWalker {
        text_budget: DecodeBudget::with_limit(256),
        ..Default::default()
    };
    let styles = HashMap::new();
    let numbering = Numbering::default();
    let resolver = Resolver {
        styles: &styles,
        numbering: &numbering,
    };
    let media = DocxMedia::load(&[]).unwrap();
    let xml = format!(
        "<w:document><w:body>{}</w:body></w:document>",
        "<w:p><w:r><w:t>hello</w:t></w:r></w:p>".repeat(8)
    );
    assert!(matches!(
        walk_part(&xml, &mut walker, &resolver, &media),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}
