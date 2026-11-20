//! `FILTERXML` (PD-271): the XPath 1.0 query over an XML text, and its limits.
//!
//! The function documentation names two results, `#VALUE!` for an XML text that is not valid and
//! the values found (its examples read attributes and elements: `//rc/@title`,
//! `//QuoteApiModel/Data/LastPrice`). The hostile cases are the ones that make the query crate run
//! for minutes or deep without a limit (measured on 4,000 nodes: nested scans in predicates).
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;
use std::time::{Duration, Instant};

fn eval(formula: &str) -> String {
    let mut model = new_empty_model();
    model._set("A1", formula);
    model.evaluate();
    model._get_text("A1")
}

/// A formula text for a literal: quotes doubled.
fn lit(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

#[test]
fn filterxml_reads_the_documented_examples() {
    let xml = "<rc-list><rc title=\"Alpha\" timestamp=\"2026-11-20T10:00:00Z\"/>\
               <rc title=\"Beta\" timestamp=\"2026-11-20T11:30:00Z\"/></rc-list>";
    let mut model = new_empty_model();
    model._set("B3", xml);
    model._set("A1", "=FILTERXML(B3,\"//rc/@title\")");
    model._set("C1", "=FILTERXML(B3,\"//rc/@timestamp\")");
    model.evaluate();
    // the values spill down, in document order
    assert_eq!(model._get_text("A1"), "Alpha");
    assert_eq!(model._get_text("A2"), "Beta");
    assert_eq!(model._get_text("C1"), "2026-11-20T10:00:00Z");
    assert_eq!(model._get_text("C2"), "2026-11-20T11:30:00Z");
    assert_eq!(
        eval(&format!(
            "=FILTERXML({},\"//QuoteApiModel/Data/LastPrice\")",
            lit("<QuoteApiModel><Data><LastPrice>182.5</LastPrice></Data></QuoteApiModel>")
        )),
        "182.5"
    );
}

#[test]
fn filterxml_returns_nodes_numbers_and_xpath_values() {
    let xml = lit("<a><b id=\"1\">x</b><b id=\"2\">7</b><b id=\"3\">z</b><c>t<d>u</d></c></a>");
    let mut model = new_empty_model();
    model._set("A1", &format!("=FILTERXML({xml},\"//b\")"));
    model._set("B1", &format!("=FILTERXML({xml},\"count(//b)\")"));
    model._set("C1", &format!("=FILTERXML({xml},\"//b[@id>1]\")"));
    model._set("D1", &format!("=FILTERXML({xml},\"//c\")"));
    model._set("E1", &format!("=FILTERXML({xml},\"boolean(//d)\")"));
    model._set("F1", &format!("=FILTERXML({xml},\"name(/*)\")"));
    model._set("G1", &format!("=FILTERXML({xml},\"//b[2]/text()\")"));
    model._set(
        "H1",
        "=SUM(FILTERXML(\"<r><n>1</n><n>2</n><n>4</n></r>\",\"//n\"))",
    );
    model.evaluate();
    assert_eq!(model._get_text("A1"), "x");
    assert_eq!(model._get_text("A2"), "7");
    assert_eq!(model._get_text("A3"), "z");
    assert_eq!(model._get_text("B1"), "3");
    assert_eq!(model._get_text("C1"), "7");
    assert_eq!(model._get_text("C2"), "z");
    // an element's value is the text of its descendants
    assert_eq!(model._get_text("D1"), "tu");
    assert_eq!(model._get_text("E1"), "TRUE");
    assert_eq!(model._get_text("F1"), "a");
    assert_eq!(model._get_text("G1"), "7");
    assert_eq!(model._get_text("H1"), "7");
}

#[test]
fn filterxml_is_value_error_for_bad_input() {
    for formula in [
        // not well formed
        "=FILTERXML(\"<a><b></a>\",\"//b\")",
        "=FILTERXML(\"\",\"//b\")",
        "=FILTERXML(\"just text\",\"//b\")",
        // an unbound namespace prefix
        "=FILTERXML(\"<p:a/>\",\"//a\")",
        // not a query
        "=FILTERXML(\"<a/>\",\"//[\")",
        "=FILTERXML(\"<a/>\",\"\")",
        // nothing found
        "=FILTERXML(\"<a><b/></a>\",\"//c\")",
        // arguments
        "=FILTERXML(\"<a/>\")",
        "=FILTERXML(\"<a/>\",\"/a\",\"x\")",
    ] {
        assert_eq!(eval(formula), "#VALUE!", "{formula}");
    }
    assert_eq!(eval("=FILTERXML(#N/A,\"//a\")"), "#N/A");
    assert_eq!(eval("=FILTERXML(\"<a/>\",#NAME?)"), "#NAME?");
}

#[test]
fn filterxml_refuses_declarations_like_the_xml_readers_of_the_engine() {
    let entity = lit("<!DOCTYPE a [<!ENTITY x \"boom\">]><a>&x;</a>");
    assert_eq!(eval(&format!("=FILTERXML({entity},\"/a\")")), "#VALUE!");
    let doctype = lit("<!DOCTYPE a SYSTEM \"file:///etc/passwd\"><a>1</a>");
    assert_eq!(eval(&format!("=FILTERXML({doctype},\"/a\")")), "#VALUE!");
    // the same words in a comment and a CDATA section are text
    let comment = lit("<a><!-- <!DOCTYPE --><![CDATA[<!ENTITY]]>ok</a>");
    assert_eq!(eval(&format!("=FILTERXML({comment},\"/a\")")), "<!ENTITYok");
}

#[test]
fn filterxml_fails_closed_on_deep_and_wide_documents() {
    // 129 levels
    let deep = format!("{}x{}", "<a>".repeat(129), "</a>".repeat(129));
    assert_eq!(
        eval(&format!("=FILTERXML({},\"/a\")", lit(&deep))),
        "#VALUE!"
    );
    let ok = format!("{}x{}", "<a>".repeat(128), "</a>".repeat(128));
    assert_eq!(eval(&format!("=FILTERXML({},\"//a\")", lit(&ok))).len(), 1);
    // more than 2,000 nodes
    let wide = format!("<r>{}</r>", "<i>x</i>".repeat(1_001));
    assert_eq!(
        eval(&format!("=FILTERXML({},\"//i\")", lit(&wide))),
        "#VALUE!"
    );
    let fits = format!("<r>{}</r>", "<i>x</i>".repeat(990));
    let mut model = new_empty_model();
    model._set("A1", &format!("=FILTERXML({},\"//i\")", lit(&fits)));
    model.evaluate();
    assert_eq!(model._get_text("A1"), "x");
    assert_eq!(model._get_text("A990"), "x");
    assert_eq!(model._get_text("A991"), "");
}

#[test]
fn filterxml_fails_closed_on_hostile_queries_without_running_them() {
    // 1,500 nodes: the nested scans below take minutes in the query crate
    let wide = lit(&format!("<r>{}</r>", "<i>x</i>".repeat(700)));
    let started = Instant::now();
    for xpath in [
        "//i[count(//i[count(//i)>1])>1]",
        "//*[//*[//*]]",
        "//i[count(//i)>1][count(//i//i)>1]",
        "//*//*//*//*",
        "//i/descendant::i/following::i/preceding::i/descendant-or-self::i",
        &format!("/r{}", "/i".repeat(200)),
        &format!("{}1{}", "(".repeat(300), ")".repeat(300)),
        "//i[",
        "//i['",
    ] {
        assert_eq!(
            eval(&format!("=FILTERXML({wide},{})", lit(xpath))),
            "#VALUE!",
            "{xpath}"
        );
    }
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    // quoted text does not count as a scan
    assert_eq!(
        eval("=FILTERXML(\"<a><b>//</b></a>\",\"//b[.='//' and .='//' and .='//' and .='//']\")"),
        "//"
    );
    // a query with one scan in a predicate runs
    assert_eq!(
        eval("=FILTERXML(\"<a><b>1</b><b>2</b></a>\",\"//b[count(//b)=2][1]\")"),
        "1"
    );
}
