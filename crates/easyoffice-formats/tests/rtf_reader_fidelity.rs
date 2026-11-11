//! RTF reading follows the specification where the reader used to guess (PD-172): paragraph
//! properties last until `\pard`, a list's kind comes from the list table, and `\'hh` bytes are
//! decoded with the code page of `\ansicpgN`.

use easyoffice_formats::{
    DecodedContent, RichAlignment, RichBlock, RichListKind, RichParagraph, UniversalFormatConverter,
};

fn read(rtf: &str) -> (Vec<RichParagraph>, Vec<String>) {
    let read = UniversalFormatConverter::decode_rich(rtf.as_bytes(), Some("rtf")).unwrap();
    let DecodedContent::Document(doc) = read.content else {
        panic!("document");
    };
    let paragraphs = doc
        .blocks
        .into_iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p),
            _ => None,
        })
        .collect();
    (paragraphs, read.warnings)
}

fn text(p: &RichParagraph) -> String {
    p.runs.iter().map(|r| r.text.as_str()).collect()
}

#[test]
fn paragraph_properties_last_until_pard() {
    let (paras, _) = read("{\\rtf1\\ansi\\pard\\qc A\\par B\\par\\pard C\\par}");
    let centered: Vec<_> = paras.iter().map(|p| p.alignment).collect();
    assert_eq!(
        centered,
        [
            Some(RichAlignment::Center),
            Some(RichAlignment::Center),
            None
        ]
    );
}

#[test]
fn list_kind_comes_from_the_list_table() {
    let table = concat!(
        "{\\*\\listtable{\\list\\listid7{\\listlevel\\levelnfc0}}",
        "{\\list{\\listlevel\\levelnfc23}\\listid9}}",
        "{\\*\\listoverridetable{\\listoverride\\listid7\\ls3}{\\listoverride\\listid9\\ls2}}"
    );
    let (paras, _) = read(&format!(
        "{{\\rtf1\\ansi{table}\\pard\\ls3 one\\par\\pard\\ls2 dot\\par}}"
    ));
    // ls3 is the decimal list and ls2 the bullet list, whatever their numbers.
    assert_eq!(paras[0].list, Some(RichListKind::Ordered));
    assert_eq!(paras[1].list, Some(RichListKind::Bullet));
}

#[test]
fn a_list_without_a_table_keeps_the_old_convention() {
    let (paras, _) = read("{\\rtf1\\ansi\\pard\\ls1 a\\par\\pard\\ls2 b\\par}");
    assert_eq!(paras[0].list, Some(RichListKind::Bullet));
    assert_eq!(paras[1].list, Some(RichListKind::Ordered));
}

#[test]
fn code_page_bytes_are_decoded_with_ansicpg() {
    let (paras, warnings) = read("{\\rtf1\\ansi\\ansicpg1252\\deff0 caf\\'e9 \\'80 \\'e9\\'e9}");
    assert_eq!(text(&paras[0]), "café € éé");
    assert!(
        warnings.iter().all(|w| !w.contains("코드 페이지")),
        "{warnings:?}"
    );

    let (paras, _) = read("{\\rtf1\\ansi\\ansicpg949 \\'b0\\'a1\\'b3\\'aa}");
    assert_eq!(text(&paras[0]), "가나");
}

#[test]
fn an_unsupported_code_page_is_reported_not_guessed() {
    let (paras, warnings) = read("{\\rtf1\\ansi\\ansicpg932 a\\'82\\'a0b}");
    let shown = text(&paras[0]);
    assert!(shown.starts_with('a') && shown.ends_with('b'), "{shown}");
    assert!(
        !shown.contains('\u{3042}'),
        "not decoded as a guess: {shown}"
    );
    assert!(warnings.iter().any(|w| w.contains("932")), "{warnings:?}");
}
