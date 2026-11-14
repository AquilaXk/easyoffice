use super::*;

#[test]
fn rtf_reads_super_sub_para_layout_and_special_characters() {
    let rtf = concat!(
        "{\\rtf1\\ansi\\pard\\li720\\fi-360\\sl360\\slmult1 ",
        "H{\\sub 2}O X\\super 3\\nosupersub !\\par",
        "\\pard \\bullet \\lquote A\\rquote \\ldblquote B\\rdblquote \\enspace\\emspace\\~\\_\\par}"
    );
    let (doc, _) = read_rtf(rtf.as_bytes()).unwrap();
    assert_eq!(doc.blocks.len(), 2);
    let RichBlock::Paragraph(p0) = &doc.blocks[0] else {
        panic!("paragraph 0");
    };
    assert_eq!(p0.left_indent_twips, Some(720));
    assert_eq!(p0.first_line_indent_twips, Some(-360));
    assert_eq!(p0.line_spacing_percent, Some(150));
    assert!(p0
        .runs
        .iter()
        .any(|r| r.text == "2" && r.script == RichScript::Subscript));
    assert!(p0
        .runs
        .iter()
        .any(|r| r.text == "3" && r.script == RichScript::Superscript));
    assert!(p0
        .runs
        .iter()
        .any(|r| r.text == "!" && r.script == RichScript::Normal));

    let RichBlock::Paragraph(p1) = &doc.blocks[1] else {
        panic!("paragraph 1");
    };
    assert_eq!(
        p1.plain_text(),
        "\u{2022}\u{2018}A\u{2019}\u{201C}B\u{201D}\u{2002}\u{2003}\u{00A0}\u{2011}"
    );
}

#[test]
fn rtf_reads_chcbpat_cb_outlinelevel_and_pn_list_levels() {
    let rtf = concat!(
        "{\\rtf1\\ansi{\\colortbl ;\\red255\\green255\\blue0;\\red0\\green255\\blue255;}",
        "\\pard\\outlinelevel1 HeadingTwo\\par",
        "\\pard{\\*\\pn\\pnlvlblt}\\chcbpat1 BulletItem\\par",
        "\\pard\\pnlvlbody\\cb2 OrderedItem\\par}"
    );
    let (doc, _) = read_rtf(rtf.as_bytes()).unwrap();
    assert_eq!(doc.blocks.len(), 3);
    let RichBlock::Paragraph(h2) = &doc.blocks[0] else {
        panic!("paragraph 0");
    };
    assert_eq!(h2.heading_level, 2);
    let RichBlock::Paragraph(b_item) = &doc.blocks[1] else {
        panic!("paragraph 1");
    };
    assert_eq!(b_item.list, Some(crate::RichListKind::Bullet));
    assert_eq!(b_item.runs[0].highlight.as_deref(), Some("FFFF00"));
    let RichBlock::Paragraph(o_item) = &doc.blocks[2] else {
        panic!("paragraph 2");
    };
    assert_eq!(o_item.list, Some(crate::RichListKind::Ordered));
    assert_eq!(o_item.runs[0].highlight.as_deref(), Some("00FFFF"));
}

#[test]
fn hostile_rtf_runs_and_blocks_exceeding_decode_budget_fail_closed() {
    let mut reader = Reader::with_budget(DecodeBudget::with_limit(256));
    let rtf = format!("{{\\rtf1\\ansi {}}}", "hello\\par ".repeat(8));
    assert!(matches!(
        reader.run(&rtf),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}
