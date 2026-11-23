//! `FILTERXML` (PD-320): cleanroom W3C XPath 1.0 queries over XML text, and fail-closed limits.
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

/// Formula string literal with doubled quotes.
fn lit(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

#[test]
fn filterxml_reads_documented_examples_and_spills_vertically() {
    let xml = "<rc-list><rc title=\"Alpha\" timestamp=\"2026-11-20T10:00:00Z\"/>\
               <rc title=\"Beta\" timestamp=\"2026-11-20T11:30:00Z\"/></rc-list>";
    let mut model = new_empty_model();
    model._set("B3", xml);
    model._set("A1", "=FILTERXML(B3,\"//rc/@title\")");
    model._set("C1", "=FILTERXML(B3,\"//rc/@timestamp\")");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "Alpha");
    assert_eq!(model._get_text("A2"), "Beta");
    assert_eq!(model._get_text("A3"), "");
    assert_eq!(model._get_text("C1"), "2026-11-20T10:00:00Z");
    assert_eq!(model._get_text("C2"), "2026-11-20T11:30:00Z");

    assert_eq!(
        eval(&format!(
            "=FILTERXML({},\"//QuoteApiModel/Data/LastPrice\")",
            lit("<QuoteApiModel><Data><LastPrice>182.5</LastPrice></Data></QuoteApiModel>")
        )),
        "182.5"
    );
    assert_eq!(
        eval("=SUM(FILTERXML(\"<r><n>1</n><n>2</n><n>4</n></r>\",\"//n\"))"),
        "7"
    );
    assert_eq!(
        eval("=INDEX(FILTERXML(\"<r><a>10</a><a>20</a></r>\",\"//a\"), 2)"),
        "20"
    );
}

#[test]
fn filterxml_supports_all_12_axes_reverse_proximity_filter_union_and_keywords() {
    let xml = lit("<r><div id=\"1\"><mod>a</mod><and>b</and></div>\
         <div id=\"2\"><mod>c</mod><b>first</b><b>mid</b><b>last</b></div>\
         <div id=\"3\">x<d>y</d></div></r>");
    let mut model = new_empty_model();
    model._set("A1", &format!("=FILTERXML({xml},\"//div[2]/b[last()]\")"));
    model._set(
        "A2",
        &format!("=FILTERXML({xml},\"//div[2]/b[last()]/preceding-sibling::b[1]\")"),
    );
    model._set(
        "A3",
        &format!("=FILTERXML({xml},\"//div[2]/b[1]/following-sibling::b[1]\")"),
    );
    model._set("A4", &format!("=FILTERXML({xml},\"(//div/mod)[2]\")"));
    model._set(
        "A5",
        &format!("=FILTERXML({xml},\"//div[1]/and/parent::div/@id\")"),
    );
    model._set(
        "A6",
        &format!("=FILTERXML({xml},\"name(//d/ancestor::*[1])\")"),
    );
    model._set(
        "A7",
        &format!("=FILTERXML({xml},\"name(//d/ancestor-or-self::*[2])\")"),
    );
    model._set("A8", &format!("=FILTERXML({xml},\"//d/preceding::b[1]\")"));
    model._set(
        "A9",
        &format!("=FILTERXML({xml},\"//div[1]/and/following::mod[1]\")"),
    );
    model._set(
        "A10",
        &format!("=FILTERXML({xml},\"//div[div or mod]/and | //div[3]/d\")"),
    );
    model._set(
        "A12",
        &format!("=FILTERXML({xml},\"//div[1]/child::and/self::and\")"),
    );
    model.evaluate();
    assert_eq!(model._get_text("A1"), "last");
    assert_eq!(model._get_text("A2"), "mid");
    assert_eq!(model._get_text("A3"), "mid");
    assert_eq!(model._get_text("A4"), "c");
    assert_eq!(model._get_text("A5"), "1");
    assert_eq!(model._get_text("A6"), "div");
    assert_eq!(model._get_text("A7"), "div");
    assert_eq!(model._get_text("A8"), "last");
    assert_eq!(model._get_text("A9"), "c");
    assert_eq!(model._get_text("A10"), "b");
    assert_eq!(model._get_text("A11"), "y");
    assert_eq!(model._get_text("A12"), "b");

    // Descendant text, arithmetic keywords on keyword-named nodes, comments/PIs.
    assert_eq!(
        eval(&format!(
            "=FILTERXML({},\"//node[. * 2 = 6]\")",
            lit("<r><node>2</node><node>3</node></r>")
        )),
        "3"
    );
    assert_eq!(
        eval(&format!(
            "=FILTERXML({},\"//item[node * 2 = 6]/node\")",
            lit("<r><item><node>2</node></item><item><node>3</node></item></r>")
        )),
        "3"
    );
    assert_eq!(
        eval(&format!(
            "=FILTERXML({},\"//text[div div 2 = 3]\")",
            lit("<r><text><div>6</div></text><text><div>4</div></text></r>")
        )),
        "6"
    );
    assert_eq!(
        eval("=FILTERXML(\"<a><?pi hello?><!-- note --><b/></a>\",\"//comment()\")"),
        " note "
    );
    assert_eq!(
        eval("=FILTERXML(\"<a><?pi hello?><!-- note --></a>\",\"//processing-instruction('pi')\")"),
        "hello"
    );
}

#[test]
fn filterxml_supports_all_27_core_xpath_functions_and_comparisons() {
    let xml = lit(
        "<r xml:lang=\"en-US\"><n xml:id=\"x1\">1</n><n>2</n><n>4</n>\
         <s>  alpha   beta </s><!-- ignored --><a><b k=\"2\">y</b><c k=\"3\">n</c></a></r>",
    );
    assert_eq!(eval(&format!("=FILTERXML({xml},\"count(//n)\")")), "3");
    assert_eq!(eval(&format!("=FILTERXML({xml},\"sum(//n)\")")), "7");
    assert_eq!(
        eval(&format!("=FILTERXML({xml},\"//n[position()=2]\")")),
        "2"
    );
    assert_eq!(eval(&format!("=FILTERXML({xml},\"//n[last()]\")")), "4");
    assert_eq!(eval(&format!("=FILTERXML({xml},\"id('x1')\")")), "1");
    assert_eq!(eval(&format!("=FILTERXML({xml},\"name(/*)\")")), "r");
    assert_eq!(eval(&format!("=FILTERXML({xml},\"local-name(/*)\")")), "r");
    assert_eq!(
        eval(&format!("=FILTERXML({xml},\"normalize-space(//s)\")")),
        "alpha beta"
    );
    assert_eq!(eval(&format!("=FILTERXML({xml},\"string(//n[2])\")")), "2");
    assert_eq!(
        eval(&format!("=FILTERXML({xml},\"concat('x', '-', //n[2])\")")),
        "x-2"
    );
    assert_eq!(
        eval(&format!(
            "=FILTERXML({xml},\"starts-with(normalize-space(//s), 'alpha')\")"
        )),
        "TRUE"
    );
    assert_eq!(
        eval(&format!("=FILTERXML({xml},\"contains(//s, 'beta')\")")),
        "TRUE"
    );
    assert_eq!(
        eval(&format!("=FILTERXML({xml},\"substring('12345', 2, 3)\")")),
        "234"
    );
    assert_eq!(
        eval(&format!(
            "=FILTERXML({xml},\"substring-before('a:b', ':')\")"
        )),
        "a"
    );
    assert_eq!(
        eval(&format!(
            "=FILTERXML({xml},\"substring-after('a:b', ':')\")"
        )),
        "b"
    );
    assert_eq!(
        eval(&format!("=FILTERXML({xml},\"string-length('한글abc')\")")),
        "5"
    );
    assert_eq!(
        eval(&format!(
            "=FILTERXML({xml},\"translate('bar', 'abc', 'ABC')\")"
        )),
        "BAr"
    );
    assert_eq!(eval(&format!("=FILTERXML({xml},\"boolean(//n)\")")), "TRUE");
    assert_eq!(
        eval(&format!("=FILTERXML({xml},\"not(//missing)\")")),
        "TRUE"
    );
    assert_eq!(
        eval(&format!("=FILTERXML({xml},\"true() and not(false())\")")),
        "TRUE"
    );
    assert_eq!(
        eval(&format!("=FILTERXML({xml},\"//n[lang('en')][1]\")")),
        "1"
    );
    assert_eq!(
        eval(&format!(
            "=FILTERXML({xml},\"number(//n[2]) + floor(1.9) + ceiling(1.1) + round(-1.5)\")"
        )),
        "4"
    );
    // NodeSet-to-NodeSet existential comparison and subtraction with whitespace around minus.
    assert_eq!(
        eval(&format!("=FILTERXML({xml},\"//n[. = //a/*/@k]\")")),
        "2"
    );
    assert_eq!(
        eval(&format!("=FILTERXML({xml},\"//n[. > 1 and . != 4]\")")),
        "2"
    );
    assert_eq!(eval(&format!("=FILTERXML({xml},\"//n[3] - //n[1]\")")), "3");
}

#[test]
fn filterxml_handles_namespaces_and_rejects_unbound_prefixes() {
    let ns_xml = lit("<root xmlns:p=\"urn:x\" xmlns=\"urn:d\">\
         <p:item p:code=\"42\">val</p:item><plain>ok</plain></root>");
    assert_eq!(eval(&format!("=FILTERXML({ns_xml},\"//p:item\")")), "val");
    assert_eq!(
        eval(&format!("=FILTERXML({ns_xml},\"//p:item/@p:code\")")),
        "42"
    );
    assert_eq!(
        eval(&format!("=FILTERXML({ns_xml},\"namespace-uri(//p:item)\")")),
        "urn:x"
    );
    assert_eq!(
        eval(&format!("=FILTERXML({ns_xml},\"name(//p:item)\")")),
        "p:item"
    );
    assert_eq!(eval(&format!("=FILTERXML({ns_xml},\"//p:*\")")), "val");
    assert_eq!(
        eval(&format!(
            "=FILTERXML({ns_xml},\"//*[local-name()='plain' and namespace-uri()='urn:d']\")"
        )),
        "ok"
    );
    // Unprefixed `plain` must not match default-namespaced elements in XPath 1.0.
    assert_eq!(
        eval(&format!("=FILTERXML({ns_xml},\"//plain\")")),
        "#VALUE!"
    );
    // Unbound prefix in XML or XPath fails closed without panicking.
    assert_eq!(eval("=FILTERXML(\"<p:a/>\",\"//a\")"), "#VALUE!");
    assert_eq!(eval("=FILTERXML(\"<a/>\",\"//p:a\")"), "#VALUE!");
}

#[test]
fn filterxml_is_value_error_for_bad_input_and_propagates_cell_errors() {
    for formula in [
        "=FILTERXML(\"<a><b></a>\",\"//b\")",
        "=FILTERXML(\"\",\"//b\")",
        "=FILTERXML(\"just text\",\"//b\")",
        "=FILTERXML(\"<a/>\",\"//[\")",
        "=FILTERXML(\"<a/>\",\"\")",
        "=FILTERXML(\"<a/>\",\"1 div 0\")",
        "=FILTERXML(\"<a/>\",\"0 div 0\")",
        "=FILTERXML(\"<a/>\",\"//\")",
        "=FILTERXML(\"<a/>\",\"/a/\")",
        "=FILTERXML(\"<a><b/></a>\",\"//c\")",
        "=FILTERXML(\"<a/>\")",
        "=FILTERXML(\"<a/>\",\"/a\",\"x\")",
        "=FILTERXML({\"<a>1</a>\",\"<a>2</a>\"},\"//a\")",
    ] {
        assert_eq!(eval(formula), "#VALUE!", "{formula}");
    }
    assert_eq!(eval("=FILTERXML(#N/A,\"//a\")"), "#N/A");
    assert_eq!(eval("=FILTERXML(\"<a/>\",#NAME?)"), "#NAME?");
    assert_eq!(eval("=FILTERXML(#REF!,\"//a\")"), "#REF!");
}

#[test]
fn filterxml_refuses_declarations_and_honors_cdata_and_comments() {
    let entity = lit("<!DOCTYPE a [<!ENTITY x \"boom\">]><a>&x;</a>");
    assert_eq!(eval(&format!("=FILTERXML({entity},\"/a\")")), "#VALUE!");
    let doctype = lit("<!DOCTYPE a SYSTEM \"file:///etc/passwd\"><a>1</a>");
    assert_eq!(eval(&format!("=FILTERXML({doctype},\"/a\")")), "#VALUE!");
    let comment = lit("<a><!-- <!DOCTYPE --><![CDATA[<!ENTITY]]>ok</a>");
    assert_eq!(eval(&format!("=FILTERXML({comment},\"/a\")")), "<!ENTITYok");
}

#[test]
fn filterxml_fails_closed_on_deep_wide_and_hostile_queries() {
    let deep = format!("{}x{}", "<a>".repeat(129), "</a>".repeat(129));
    assert_eq!(
        eval(&format!("=FILTERXML({},\"/a\")", lit(&deep))),
        "#VALUE!"
    );
    let ok = format!("{}x{}", "<a>".repeat(128), "</a>".repeat(128));
    assert_eq!(
        eval(&format!("=FILTERXML({},\"//a[last()]\")", lit(&ok))),
        "x"
    );

    let over_nodes = format!("<r>{}</r>", "<i/>".repeat(16_384));
    assert_eq!(
        eval(&format!("=FILTERXML({},\"//i\")", lit(&over_nodes))),
        "#VALUE!"
    );

    let fits = format!("<r>{}</r>", "<i>x</i>".repeat(990));
    let mut model = new_empty_model();
    model._set("A1", &format!("=FILTERXML({},\"//i\")", lit(&fits)));
    model.evaluate();
    assert_eq!(model._get_text("A1"), "x");
    assert_eq!(model._get_text("A990"), "x");
    assert_eq!(model._get_text("A991"), "");

    // Hostile O(N^3) queries abort via MAX_XPATH_OPS within milliseconds.
    let wide = lit(&format!("<r>{}</r>", "<i>x</i>".repeat(700)));
    let started = Instant::now();
    for xpath in [
        "/r/i[count(../i/following-sibling::i)>1]",
        "//i[count(//i[count(//i)>1])>1]",
        "//*[//*[//*]]",
        "//i[count(//i)>1][count(//i//i)>1]",
        "//i/descendant::i/following::i/preceding::i/descendant-or-self::i",
        &format!("/r{}", "/i".repeat(200)),
        &format!("{}1{}", "(".repeat(300), ")".repeat(300)),
        &"a".repeat(5_000),
    ] {
        assert_eq!(
            eval(&format!("=FILTERXML({wide},{})", lit(xpath))),
            "#VALUE!",
            "{xpath}"
        );
    }
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );

    assert_eq!(
        eval("=FILTERXML(\"<a><b>//</b></a>\",\"//b[.='//' and .='//' and .='//' and .='//']\")"),
        "//"
    );
    assert_eq!(
        eval("=FILTERXML(\"<a><b>1</b><b>2</b></a>\",\"//b[count(//b)=2][1]\")"),
        "1"
    );
}
