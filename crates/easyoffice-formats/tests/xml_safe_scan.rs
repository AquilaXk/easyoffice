//! `XmlSanitizer::validate_safe_xml` rejects real `<!DOCTYPE`/`<!ENTITY` declarations and lets the
//! same words through inside comments, CDATA sections and processing instructions.

use easyoffice_formats::{EasyOfficeError, XmlSanitizer};

fn rejected(xml: &str) {
    assert_eq!(
        XmlSanitizer::validate_safe_xml(xml),
        Err(EasyOfficeError::XxeOrExternalEntityDetected),
        "{:.80}",
        xml
    );
}

fn accepted(xml: &str) {
    assert_eq!(XmlSanitizer::validate_safe_xml(xml), Ok(()), "{:.80}", xml);
}

fn corrupted(xml: &str) {
    assert!(
        matches!(
            XmlSanitizer::validate_safe_xml(xml),
            Err(EasyOfficeError::CorruptedContainer(_))
        ),
        "{:.80}",
        xml
    );
}

#[test]
fn comment_and_cdata_examples_pass() {
    accepted("<r><!-- <!DOCTYPE html> --><a/></r>");
    accepted("<r><!-- <!ENTITY x \"y\"> --></r>");
    accepted("<r><![CDATA[<!DOCTYPE html><!ENTITY x SYSTEM \"f\">]]></r>");
    accepted("<?xml version=\"1.0\"?><r><!--<!doctype html>--><b>t</b></r>");
    accepted("<r><![CDATA[a <!-- b]]><c/></r>");
    accepted("<r><!-- a <![CDATA[ b --><c/></r>");
    accepted("<?pi <!DOCTYPE x> ?><r/>");
}

#[test]
fn declarations_outside_comments_are_rejected_in_every_spelling() {
    for decl in [
        "<!DOCTYPE r>",
        "<!doctype r>",
        "<!DocType r>",
        "<!  DOCTYPE r>",
        "<!\tDOCTYPE r>",
        "<!\nDOCTYPE r>",
        "<!ENTITY e \"v\">",
        "<!entity e \"v\">",
        "<! EnTiTy e \"v\">",
    ] {
        rejected(&format!("{decl}<r/>"));
        rejected(&format!("<?xml version=\"1.0\"?>{decl}<r/>"));
        rejected(&format!("<r/>{decl}"));
        rejected(&format!("<r>{decl}</r>"));
        rejected(&format!("<!-- ok -->{decl}<r/>"));
        rejected(&format!("<![CDATA[x]]>{decl}"));
        rejected(&format!("<?pi ?>{decl}"));
    }
}

#[test]
fn declaration_right_after_a_closed_comment_or_cdata_is_found() {
    rejected("<!-- <!DOCTYPE a> --><!DOCTYPE b><r/>");
    rejected("<![CDATA[ <!DOCTYPE a> ]]><!DOCTYPE b><r/>");
    rejected("<!----><!DOCTYPE b><r/>");
    rejected("<!--x--><!--y--><!ENTITY e \"v\"><r/>");
}

#[test]
fn comment_end_is_the_earliest_terminator() {
    // `<!-->` closes at once, so the declaration after it is real.
    rejected("<!--><!DOCTYPE x>-->");
    // A `--!>` is not a terminator.
    corrupted("<!-- a --!> <!DOCTYPE x>");
    // Only the first `-->` ends the comment; the second `<!DOCTYPE` is outside.
    rejected("<!-- a --> <!DOCTYPE x> <!-- b -->");
}

#[test]
fn unclosed_comment_or_cdata_is_refused() {
    corrupted("<r><!-- <!DOCTYPE x> </r>");
    corrupted("<r><!-- never closed");
    corrupted("<r><![CDATA[ <!DOCTYPE x> </r>");
    corrupted("<r><![CDATA[ ]] > ");
    corrupted("<r>text<!--");
}

#[test]
fn a_comment_opener_inside_cdata_does_not_hide_what_follows() {
    rejected("<![CDATA[ <!-- ]]><!DOCTYPE x><r/> -->");
    rejected("<![CDATA[ <!-- ]]><!ENTITY e \"v\"><r>-->");
}

#[test]
fn a_cdata_opener_inside_a_comment_does_not_hide_what_follows() {
    rejected("<!-- <![CDATA[ --><!DOCTYPE x><r>]]></r>");
}

#[test]
fn a_processing_instruction_does_not_hide_a_declaration_after_it() {
    rejected("<?pi <!-- ?><!DOCTYPE x [<!ENTITY e \"v\">]><r>&e; --></r>");
    rejected("<?pi <![CDATA[ ?><!DOCTYPE x><r>]]></r>");
    // An unclosed processing instruction hides nothing.
    rejected("<?pi <!DOCTYPE x>");
}

#[test]
fn nested_looking_markup_is_scanned_by_outermost_construct() {
    accepted("<!-- <!-- <!DOCTYPE x> -->");
    accepted("<r><!--a--><!--<!DOCTYPE x>--><![CDATA[<!ENTITY>]]></r>");
    rejected("<r><!--a--><!DOCTYPE x><!--b--></r>");
}

#[test]
fn short_and_boundary_inputs() {
    accepted("");
    accepted("<a/>");
    accepted("<!--x-->");
    accepted("<!DOCTYP");
    rejected("<!DOCTYPE");
    rejected("<a><!ENTITY");
    rejected("<!ENTITY>");
    // A declaration cut off at the end of the input is still a declaration.
    rejected(&format!("{}<!ENTITY", "x".repeat(300)));
    rejected(&format!("{}<!DOCTYPE", "x".repeat(300)));
    corrupted(&format!("{}<!--", "x".repeat(300)));
}

#[test]
fn large_documents_with_many_comments_stay_exact() {
    let body = "<p>text</p><!-- <!DOCTYPE x> -->".repeat(5_000);
    accepted(&format!("<r>{body}</r>"));
    rejected(&format!("<r>{body}<!DOCTYPE x></r>"));
    rejected(&format!("<!DOCTYPE x><r>{body}</r>"));
}
