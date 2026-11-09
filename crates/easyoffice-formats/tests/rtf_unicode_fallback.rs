//! RTF `\uN` escapes skip exactly `\ucN` fallback units (a byte, a `\'hh`, a control word or a group),
//! with `\ucN` scoped to its group, in both RTF readers.

use easyoffice_formats::{DecodedContent, RichBlock, RichRun, RtfCodec, UniversalFormatConverter};

fn paragraph_texts(rtf: &str) -> Vec<String> {
    let decoded = UniversalFormatConverter::decode_rich(rtf.as_bytes(), Some("a.rtf")).unwrap();
    let DecodedContent::Document(doc) = decoded.content else {
        panic!("document expected");
    };
    doc.blocks
        .iter()
        .filter_map(|block| match block {
            RichBlock::Paragraph(p) => {
                Some(p.runs.iter().map(|r: &RichRun| r.text.as_str()).collect())
            }
            _ => None,
        })
        .collect()
}

fn read(body: &str) -> String {
    paragraph_texts(&format!("{{\\rtf1\\ansi {body}}}")).join("|")
}

#[test]
fn hex_fallback_byte_is_skipped_not_printed() {
    assert_eq!(read("\\uc1\\u-21504\\'3f"), "가");
    assert_eq!(read("\\uc1\\u-21504\\'3fX"), "가X");
    assert_eq!(
        read("\\uc1\\u44032\\'3f"),
        "가",
        "positive code points work too"
    );
}

#[test]
fn uc_count_sets_how_many_fallback_units_follow() {
    assert_eq!(read("\\uc2\\u-21504\\'b0\\'a1"), "가");
    assert_eq!(read("\\uc2\\u-21504\\'b0\\'a1X"), "가X");
    assert_eq!(
        read("\\uc3\\u-21504abcX"),
        "가X",
        "plain characters count one each"
    );
    assert_eq!(
        read("\\uc0\\u-21504A"),
        "가A",
        "uc0 means no fallback follows"
    );
    assert_eq!(
        read("\\uc2\\u-21504?\\'3fX"),
        "가X",
        "plain and hex units mix"
    );
}

#[test]
fn uc_is_scoped_to_its_group() {
    assert_eq!(read("\\uc1 {\\uc0 \\u-21504}\\u-21503?A"), "가각A");
    assert_eq!(
        read("{\\uc0 \\u-21504}\\u-21503\\'3fA"),
        "가각A",
        "uc1 is back after the group"
    );
    assert_eq!(
        read("{\\uc3\\u-21504?}Z"),
        "가Z",
        "a fallback never runs past the end of its group"
    );
}

#[test]
fn control_words_symbols_and_groups_count_as_one_unit() {
    assert_eq!(read("\\uc1\\u-21504\\emdash X"), "가X");
    assert_eq!(read("\\uc1\\u-21504\\}Y"), "가Y");
    assert_eq!(read("\\uc1\\u-21504{\\b ?}X"), "가X");
    assert_eq!(
        read("\\uc1\\u-21504{\\b {\\i ?}}X"),
        "가X",
        "nested groups are skipped whole"
    );
}

#[test]
fn surrogate_pairs_decode_once_with_their_fallbacks_skipped() {
    assert_eq!(read("\\uc1\\u-10179?\\u-8704?"), "😀");
    assert_eq!(read("\\uc1\\u-10179\\'3f\\u-8704\\'3fA"), "😀A");
    assert_eq!(read("\\uc2\\u-10179\\'3f\\'3f\\u-8704\\'3f\\'3fA"), "😀A");
}

#[test]
fn fallback_skipping_is_bounded_by_the_input() {
    assert_eq!(read("\\uc9\\u-21504ab"), "가");
    assert_eq!(
        read("\\uc1\\u-21504{\\b ?"),
        "가",
        "an unclosed group fallback ends at the end of the text"
    );
}

#[test]
fn plain_decoder_skips_the_same_fallback_units() {
    let decode = |line: &str| {
        let rtf = format!("{{\\rtf1\\ansi\\deff0\n\\pard {line}\\par\n}}");
        RtfCodec::decode(rtf.as_bytes()).unwrap().paragraphs
    };
    assert_eq!(decode("\\uc1\\u-21504\\'3f"), ["가"]);
    assert_eq!(decode("\\uc2\\u-21504\\'b0\\'a1X"), ["가X"]);
    assert_eq!(decode("\\uc0\\u-21504A"), ["가A"]);
    assert_eq!(
        decode("\\u-21504?\\u-21503?"),
        ["가각"],
        "the writer's own form is unchanged"
    );
    assert_eq!(decode("\\uc1\\u-10179?\\u-8704?"), ["😀"]);
}
