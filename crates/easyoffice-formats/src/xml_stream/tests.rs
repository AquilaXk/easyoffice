use super::*;

/// All events of `xml`; for callers that look ahead or reread a range.
fn tokenize(xml: &str) -> Result<Vec<XmlEvent<'_>>, EasyOfficeError> {
    let mut events = Vec::with_capacity((xml.len() / 24).min(65_536));
    let mut reader = XmlReader::new(xml);
    while let Some(event) = reader.next_event()? {
        events.push(event);
    }
    Ok(events)
}

/// The earlier owned tokenizer, kept verbatim as the reference the pull reader is compared to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Legacy {
    Start {
        name: String,
        attrs: Vec<(String, String)>,
    },
    End(String),
    Text(String),
}

fn legacy_parse_attrs(raw: &str) -> Result<Vec<(String, String)>, EasyOfficeError> {
    let bytes = raw.as_bytes();
    let mut attrs = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let name_start = i;
        while i < bytes.len() && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let name = &raw[name_start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            return Err(corrupt("XML 속성에 '='가 없습니다."));
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || (bytes[i] != b'"' && bytes[i] != b'\'') {
            return Err(corrupt("XML 속성 값에 따옴표가 없습니다."));
        }
        let quote = bytes[i];
        i += 1;
        let value_start = i;
        let Some(val_end) = XmlSanitizer::find_byte_swar(bytes, value_start, quote) else {
            return Err(corrupt("XML 속성 값이 닫히지 않았습니다."));
        };
        if XmlSanitizer::find_byte_swar(&bytes[value_start..val_end], 0, b'<').is_some() {
            return Err(corrupt(
                "XML 속성 값에 금지된 문자 '<'가 포함되어 있습니다.",
            ));
        }
        i = val_end;
        attrs.push((
            name.to_string(),
            XmlSanitizer::unescape(&raw[value_start..i]),
        ));
        i += 1;
    }
    Ok(attrs)
}

/// Tokenizes `xml` into start/end/text events; empty elements yield a start and an end.
fn legacy_tokenize(xml: &str) -> Result<Vec<Legacy>, EasyOfficeError> {
    let bytes = xml.as_bytes();
    let mut events = Vec::with_capacity((bytes.len() / 24).min(65_536));
    let mut i = 0usize;
    while i < bytes.len() {
        if events.len() > MAX_XML_EVENTS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "XML 이벤트 수가 허용 예산을 초과했습니다.".to_string(),
            ));
        }
        if bytes[i] != b'<' {
            let end = XmlSanitizer::find_byte_swar(bytes, i, b'<').unwrap_or(bytes.len());
            events.push(Legacy::Text(XmlSanitizer::unescape(&xml[i..end])));
            i = end;
            continue;
        }
        let rest = &xml[i..];
        if rest.starts_with("<!--") {
            let end = rest
                .find("-->")
                .ok_or_else(|| corrupt("XML 주석이 닫히지 않았습니다."))?;
            i += end + 3;
        } else if rest.starts_with("<![CDATA[") {
            let end = rest
                .find("]]>")
                .ok_or_else(|| corrupt("CDATA가 닫히지 않았습니다."))?;
            events.push(Legacy::Text(rest[9..end].to_string()));
            i += end + 3;
        } else if rest.starts_with("<?") {
            let end = rest
                .find("?>")
                .ok_or_else(|| corrupt("XML 처리 명령이 닫히지 않았습니다."))?;
            i += end + 2;
        } else if rest.starts_with("</") {
            let close_idx = XmlSanitizer::find_byte_swar(bytes, i + 2, b'>')
                .ok_or_else(|| corrupt("XML 끝 태그가 닫히지 않았습니다."))?;
            events.push(Legacy::End(xml[i + 2..close_idx].trim().to_string()));
            i = close_idx + 1;
        } else {
            let mut end = None;
            let mut quote: Option<u8> = None;
            for (off, &b) in rest.as_bytes().iter().enumerate().skip(1) {
                match quote {
                    Some(q) if b == q => quote = None,
                    Some(_) => {}
                    None if b == b'"' || b == b'\'' => quote = Some(b),
                    None if b == b'>' => {
                        end = Some(off);
                        break;
                    }
                    None => {}
                }
            }
            let end = end.ok_or_else(|| corrupt("XML 시작 태그가 닫히지 않았습니다."))?;
            let mut inner = &rest[1..end];
            let empty = inner.ends_with('/');
            if empty {
                inner = &inner[..inner.len() - 1];
            }
            let name_end = inner
                .find(|c: char| c.is_ascii_whitespace())
                .unwrap_or(inner.len());
            let name = inner[..name_end].to_string();
            if name.is_empty() {
                return Err(corrupt("XML 태그 이름이 비어 있습니다."));
            }
            let attrs = legacy_parse_attrs(&inner[name_end..])?;
            events.push(Legacy::Start {
                name: name.clone(),
                attrs,
            });
            if empty {
                events.push(Legacy::End(name));
            }
            i += end + 1;
        }
    }
    Ok(events)
}

fn owned(events: Vec<XmlEvent<'_>>) -> Vec<Legacy> {
    events
        .into_iter()
        .map(|event| match event {
            XmlEvent::Start { name, attrs } => Legacy::Start {
                name: name.to_string(),
                attrs: attrs
                    .iter()
                    .map(|(n, v)| (n.to_string(), v.into_owned()))
                    .collect(),
            },
            XmlEvent::End(name) => Legacy::End(name.to_string()),
            XmlEvent::Text(text) => Legacy::Text(text.into_owned()),
        })
        .collect()
}

/// Both readers must give the same events, or both must refuse the input.
fn assert_same(xml: &str) {
    match (legacy_tokenize(xml), tokenize(xml)) {
        (Ok(old), Ok(new)) => assert_eq!(old, owned(new), "events differ for {xml:?}"),
        (Err(_), Err(_)) => {}
        (old, new) => panic!("{xml:?}: old {old:?}, new {:?}", new.map(owned)),
    }
}

/// Inputs on which the crate reads a malformed construct another way than the owned tokenizer did.
/// None of them is well-formed XML:
/// - `<!-->` and `<?>` closed at their own opener; the crate needs the closing `-->` and `?>`;
/// - `<!` that opens no comment, CDATA section or declaration was a start tag named `!...`;
/// - an end tag holding a quote reads like a start tag (its `>` can be quoted); the owned one ended
///   it at the first `>`;
/// - a `<` right before a character that cannot start a name or right after a quote, and `=` where an
///   attribute name starts: the crate refuses the first two and reads the third as a part of the name.
fn is_malformed_construct(xml: &str) -> bool {
    let bang_garbage = xml.match_indices("<!").any(|(at, _)| {
        let rest = &xml[at + 2..];
        !(rest.starts_with("--") || rest.starts_with("[CDATA[") || rest.starts_with("DOCTYPE"))
    });
    let end_tag_quote = xml.split("</").skip(1).any(|tail| {
        tail.split('>')
            .next()
            .is_some_and(|t| t.contains(['"', '\'']))
    });
    let odd_name_start = xml.as_bytes().windows(2).any(|w| match (w[0], w[1]) {
        (b'<', next) => b"<=&'\"/ >".contains(&next),
        (prev, b'=') => b" '\"=".contains(&prev),
        (b'\'' | b'"', b'<') => true,
        _ => false,
    });
    xml.contains("<!-->") || xml.contains("<?>") || bang_garbage || end_tag_quote || odd_name_start
}

/// Like [`assert_same`] for inputs the owned tokenizer was lenient with: a difference must come from
/// one of the constructs of [`is_malformed_construct`].
fn assert_same_unless_malformed(xml: &str) {
    let old = legacy_tokenize(xml);
    let new = tokenize(xml).map(owned);
    let same = match (&old, &new) {
        (Ok(old), Ok(new)) => old == new,
        (Err(_), Err(_)) => true,
        _ => false,
    };
    assert!(
        same || is_malformed_construct(xml),
        "{xml:?}: old {old:?}, new {new:?}"
    );
}

const SAMPLES: &[&str] = &[
    "<?xml version=\"1.0\"?><a x=\"1&amp;2\"><b/>t&lt;<!-- c --></a>",
    "<a><![CDATA[<x>&amp;]]>t</a>",
    "<a x='1' y=\"2\"  z = '3'/>",
    "<a x=\"a>b\" y='c>d'>t</a>",
    "<a>&#x41;&#65;&#0;&#xD800;&bogus;&amp</a>",
    "<a   ></a  ><b/ >",
    "<a><!-->x</a>",
    "<a>\n  <b>\n</b>\n</a>",
    "<a b=\"1\"c=\"2\"/>",
    "<a b=\"1\" b=\"2\"/>",
    "text only &amp; more",
    "<a",
    "<a x=1>",
    "<a x=\"1>",
    "<a x>",
    "<a =\"1\"/>",
    "< a/>",
    "<>",
    "</a",
    "<!-- x",
    "<![CDATA[ x",
    "<? x",
    "",
];

#[test]
fn tokenizes_nested_elements_attributes_and_entities() {
    let ev = tokenize("<?xml version=\"1.0\"?><a x=\"1&amp;2\"><b/>t&lt;<!-- c --></a>").unwrap();
    assert_eq!(
        owned(ev),
        vec![
            Legacy::Start {
                name: "a".into(),
                attrs: vec![("x".into(), "1&2".into())]
            },
            Legacy::Start {
                name: "b".into(),
                attrs: vec![]
            },
            Legacy::End("b".into()),
            Legacy::Text("t<".into()),
            Legacy::End("a".into()),
        ]
    );
}

#[test]
fn a_doctype_declaration_is_refused() {
    // The earlier reader skipped the declaration (PD-178); a declaration can define entities, so the
    // reader refuses it with the error of the safe-XML check, whatever it holds or where it is.
    for xml in [
        "<!DOCTYPE a><a/>",
        "<!doctype a><a/>",
        "<!DOCTYPE a SYSTEM \"a.dtd\"><a/>",
        "<!DOCTYPE a [<!ELEMENT a ANY>]><a/>",
        "<!DOCTYPE a [<!ENTITY x \"]>\">]><a/>",
        "<?xml version=\"1.0\"?>\n<!DOCTYPE a [<!ENTITY x SYSTEM \"file:///etc/passwd\">]><a>&x;</a>",
        "<a><!DOCTYPE a></a>",
    ] {
        assert!(
            matches!(tokenize(xml), Err(EasyOfficeError::XxeOrExternalEntityDetected)),
            "{xml}"
        );
    }
    assert!(tokenize("<!DOCTYPE a [<a/>").is_err());
    assert!(tokenize("<!DOCTYPE a \"x><a/>").is_err());
}

#[test]
fn rejects_unterminated_markup() {
    assert!(tokenize("<a").is_err());
    assert!(tokenize("<a x=1>").is_err());
    assert!(tokenize("<!-- x").is_err());
}

#[test]
fn text_and_attributes_borrow_unless_an_entity_needs_decoding() {
    let ev = tokenize("<a x=\"plain\" y=\"1&amp;2\">abc&lt;</a>").unwrap();
    let XmlEvent::Start { attrs, .. } = &ev[0] else {
        panic!("start expected");
    };
    assert!(matches!(attrs.get("x"), Some(Cow::Borrowed("plain"))));
    assert!(matches!(attrs.get("y"), Some(Cow::Owned(v)) if v == "1&2"));
    assert!(matches!(&ev[1], XmlEvent::Text(Cow::Owned(t)) if t == "abc<"));
}

#[test]
fn reader_matches_the_owned_tokenizer_on_samples() {
    for xml in SAMPLES {
        assert_same_unless_malformed(xml);
    }
}

#[test]
fn reader_matches_the_owned_tokenizer_on_mutated_samples() {
    // Every truncation and every single-byte replacement of each sample.
    for xml in SAMPLES.iter().filter(|x| x.is_ascii()) {
        for cut in 0..=xml.len() {
            assert_same_unless_malformed(&xml[..cut]);
        }
        for at in 0..xml.len() {
            for b in *b"<>\"'/=& a" {
                let mut bytes = xml.as_bytes().to_vec();
                bytes[at] = b;
                assert_same_unless_malformed(std::str::from_utf8(&bytes).unwrap());
            }
        }
    }
}

#[test]
fn skip_and_collect_subtree_cover_the_matching_element() {
    let xml = "<a><b><c/></b><d/></a>";
    let mut reader = XmlReader::new(xml);
    reader.next_event().unwrap();
    reader.next_event().unwrap();
    reader.skip_subtree().unwrap();
    assert_eq!(
        reader.next_event().unwrap().unwrap(),
        tokenize(xml).unwrap()[5]
    );

    let mut reader = XmlReader::new(xml);
    reader.next_event().unwrap();
    let b = reader.next_event().unwrap().unwrap();
    let sub = reader.collect_subtree(b).unwrap();
    assert_eq!(sub, tokenize(xml).unwrap()[1..5].to_vec());
}

#[test]
fn event_budget_fails_closed() {
    let xml = "<a/>".repeat(10);
    let mut reader = XmlReader::with_budget(&xml, 8);
    let result: Result<Vec<_>, _> = (&mut reader).collect();
    assert!(matches!(
        result,
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
    assert!(reader.next().is_none(), "no events after an error");
    assert!(legacy_tokenize(&xml).is_ok());
}

#[test]
fn hostile_inputs_stay_bounded() {
    // Deep nesting needs no recursion; the crate's stack of open elements is bounded by MAX_XML_DEPTH.
    let deep = "<a>".repeat(200_000) + &"</a>".repeat(200_000);
    assert_eq!(tokenize(&deep).unwrap().len(), 400_000);
    let mut reader = XmlReader::new(&deep);
    reader.next_event().unwrap();
    reader.skip_subtree().unwrap();
    assert!(reader.next_event().unwrap().is_none());

    // A huge attribute value is one borrowed slice.
    let big = format!("<a x=\"{}\"/>", "v".repeat(8 << 20));
    let ev = tokenize(&big).unwrap();
    let XmlEvent::Start { attrs, .. } = &ev[0] else {
        panic!("start expected");
    };
    assert_eq!(attrs.get("x").unwrap().len(), 8 << 20);

    // Truncated and endless entities and markup are refused or kept literally, never read past the end.
    for xml in [
        "<a>&#x".to_string(),
        "<a>&#".to_string(),
        format!("<a>&{}</a>", "x".repeat(1 << 20)),
        format!("<a x=\"{}", "&".repeat(1 << 20)),
        format!("<!--{}", "-".repeat(1 << 20)),
    ] {
        assert_same(&xml);
    }
}

#[test]
fn swar_tag_and_attr_scans_match_legacy_tokenizer_across_long_tags() {
    let long_tag = format!("ns:{}", "element_name".repeat(4));
    let long_attr = "attribute_identifier_".repeat(3);
    let long_spaces = " ".repeat(29);
    let xml = format!(
        "<{long_tag}{long_spaces}{long_attr}   =   'a > \"b\"' \t second_attr=\"한글>값\"   empty=\"\"  />"
    );
    assert_same(&xml);
    for pad in 0..=16usize {
        let padded = format!(
            "<el{}a=\"v>1\"   longname_0123456789='x'/>",
            " ".repeat(pad)
        );
        assert_same(&padded);
    }
}

/// The text events of `xml` as strings.
fn texts(xml: &str) -> Vec<String> {
    tokenize(xml)
        .unwrap()
        .into_iter()
        .filter_map(|event| match event {
            XmlEvent::Text(text) => Some(text.into_owned()),
            _ => None,
        })
        .collect()
}

#[test]
fn only_predefined_and_numeric_entities_resolve() {
    assert_eq!(
        texts("<a>&lt;&gt;&amp;&quot;&apos;&#65;&#x42;&#X43;</a>"),
        ["<>&\"'ABC"]
    );
    // Anything else stays as written, in text and in attribute values, and ends no read.
    for literal in [
        "&bogus;",
        "&amp",
        "&",
        "& ;",
        "&#;",
        "&#x;",
        "&#xZZ;",
        "&#-1;",
        "&#99999999999;",
        "&#x110000;",
        "&#xD800;",
        "&#x0;",
        "&#0;",
        "&#x1;",
        "&#xFFFE;",
        "&lol9;",
    ] {
        assert_eq!(texts(&format!("<a>{literal}</a>")), [literal], "{literal}");
        let tag = format!("<a v=\"{literal}\"/>");
        let ev = tokenize(&tag).unwrap();
        let XmlEvent::Start { attrs, .. } = &ev[0] else {
            panic!("start expected");
        };
        assert_eq!(attrs.get("v").unwrap(), literal, "{literal}");
    }
}

#[test]
fn text_stays_one_event_across_references_and_splits_at_markup() {
    let ev = tokenize("<a>x&amp;y&lt;&lt;z&bogus;<!-- c -->w&#65;</a>").unwrap();
    let kinds: Vec<_> = ev
        .iter()
        .map(|event| match event {
            XmlEvent::Start { .. } => "start".to_string(),
            XmlEvent::End(_) => "end".to_string(),
            XmlEvent::Text(text) => format!("text:{text}"),
        })
        .collect();
    assert_eq!(kinds, ["start", "text:x&y<<z&bogus;", "text:wA", "end"]);
    // A lone `&` right before markup or the end is text too.
    assert_eq!(texts("<a>a &</a>"), ["a &"]);
    assert_eq!(texts("a & b"), ["a & b"]);
    assert_eq!(texts(&"&".repeat(1 << 16)), ["&".repeat(1 << 16)]);
}

#[test]
fn cdata_is_text_as_written_and_apart_from_the_text_around_it() {
    assert_eq!(
        texts("<a>1<![CDATA[<x>&amp;]]>2<![CDATA[]]></a>"),
        ["1", "<x>&amp;", "2", ""]
    );
    assert_eq!(texts("<a><![CDATA[<!DOCTYPE a>]]></a>"), ["<!DOCTYPE a>"]);
    assert!(tokenize("<a><![CDATA[x</a>").is_err());
}

#[test]
fn comments_and_processing_instructions_make_no_event() {
    let xml = "<?xml version=\"1.0\"?><?xml-stylesheet href=\"a.css\"?><a><!-- <b/> <!DOCTYPE x> --><?pi <c/> ?></a>";
    assert_eq!(
        owned(tokenize(xml).unwrap()),
        vec![
            Legacy::Start {
                name: "a".into(),
                attrs: vec![]
            },
            Legacy::End("a".into())
        ]
    );
    assert!(tokenize("<a><?pi </a>").is_err());
    assert!(tokenize("<a><!-- </a>").is_err());
}

#[test]
fn names_and_attributes_keep_their_namespace_prefix() {
    let xml = "<w:p xmlns:w=\"urn:w\" w:val=\"1\" val=\"2\"><w:r/></w:p >";
    let ev = tokenize(xml).unwrap();
    let XmlEvent::Start { name, attrs } = &ev[0] else {
        panic!("start expected");
    };
    assert_eq!(*name, "w:p");
    assert_eq!(attrs.get("w:val").unwrap(), "1");
    assert_eq!(attrs.get("val").unwrap(), "2");
    assert_eq!(attrs.get("xmlns:w").unwrap(), "urn:w");
    assert!(attrs.get("w").is_none());
    let XmlEvent::Start { name, attrs } = &ev[1] else {
        panic!("start expected");
    };
    assert_eq!((*name, attrs.iter_raw().count()), ("w:r", 0));
    assert_eq!(ev[2], XmlEvent::End("w:r"));
    assert_eq!(ev[3], XmlEvent::End("w:p"));
}

#[test]
fn end_tags_need_no_partner_and_may_pad_their_name() {
    let ev = owned(tokenize("</x><a></b ></ a>").unwrap());
    assert_eq!(
        ev,
        vec![
            Legacy::End("x".into()),
            Legacy::Start {
                name: "a".into(),
                attrs: vec![]
            },
            Legacy::End("b".into()),
            Legacy::End("a".into()),
        ]
    );
}

#[test]
fn duplicate_attributes_keep_document_order_and_the_first_one_wins() {
    let ev = tokenize("<a b=\"1\" c=\"x\" b=\"2\"/>").unwrap();
    let XmlEvent::Start { attrs, .. } = &ev[0] else {
        panic!("start expected");
    };
    assert_eq!(attrs.get("b").unwrap(), "1");
    let all: Vec<_> = attrs.iter_raw().collect();
    assert_eq!(all, [("b", "1"), ("c", "x"), ("b", "2")]);
}

#[test]
fn text_and_attribute_whitespace_is_kept_as_written() {
    // No end-of-line or attribute-value normalization: the readers see the bytes of the file.
    let ev = tokenize("<a v=\"p\tq\r\nr\ns\"> x\r\ny\rz \t</a>").unwrap();
    let XmlEvent::Start { attrs, .. } = &ev[0] else {
        panic!("start expected");
    };
    assert_eq!(attrs.get("v").unwrap(), "p\tq\r\nr\ns");
    assert_eq!(ev[1], XmlEvent::Text(Cow::Borrowed(" x\r\ny\rz \t")));
}

#[test]
fn a_leading_byte_order_mark_makes_no_text() {
    let ev = owned(tokenize("\u{FEFF}<?xml version=\"1.0\"?><a/>").unwrap());
    assert_eq!(ev.len(), 2);
    assert_eq!(texts("\u{FEFF}<a>\u{FEFF}</a>"), ["\u{FEFF}"]);
}

#[test]
fn declarations_and_entity_attacks_are_refused_before_any_expansion() {
    let billion_laughs = "<?xml version=\"1.0\"?><!DOCTYPE lolz [<!ENTITY lol \"lol\">\
        <!ENTITY lol2 \"&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;\">\
        <!ENTITY lol3 \"&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;\">]><lolz>&lol3;</lolz>";
    let xxe = "<!DOCTYPE foo [<!ENTITY xxe SYSTEM \"file:///etc/passwd\">]><foo>&xxe;</foo>";
    let external_dtd = "<!DOCTYPE foo SYSTEM \"http://example.invalid/foo.dtd\"><foo/>";
    for xml in [billion_laughs, xxe, external_dtd] {
        let mut reader = XmlReader::new(xml);
        let result: Result<Vec<_>, _> = (&mut reader).collect();
        assert!(
            matches!(result, Err(EasyOfficeError::XxeOrExternalEntityDetected)),
            "{xml}"
        );
        assert!(reader.next().is_none(), "no event after the refusal");
    }
    // Without a declaration the same references are only text.
    assert_eq!(texts("<lolz>&lol3;&xxe;</lolz>"), ["&lol3;&xxe;"]);
}

#[test]
fn a_malformed_attribute_list_ends_the_reader() {
    for xml in [
        "<a x>",
        "<a x=1>",
        "<a x=\"1>",
        "<a x='1\">",
        "<a x=\"1\" y/>",
        "<a x=/>",
        "< a/>",
        "<>",
    ] {
        let mut reader = XmlReader::new(xml);
        assert!(reader.next_event().is_err(), "{xml}");
        assert!(reader.next_event().unwrap().is_none(), "{xml}");
    }
}

#[test]
fn hostile_sizes_are_read_in_linear_time_and_bounded_memory() {
    // 100,000 attributes in one tag, long names and values, and deep nesting with long names.
    let many = format!(
        "<a {}/>",
        (0..100_000)
            .map(|i| format!("k{i}=\"v\""))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let ev = tokenize(&many).unwrap();
    let XmlEvent::Start { attrs, .. } = &ev[0] else {
        panic!("start expected");
    };
    assert_eq!(attrs.iter_raw().count(), 100_000);
    assert_eq!(attrs.get("k99999").unwrap(), "v");

    let long_name = "n".repeat(1 << 20);
    let long_tag = format!("<{long_name} {long_name}=\"{long_name}\"/>");
    let ev = tokenize(&long_tag).unwrap();
    assert_eq!(ev.len(), 2);

    let nested = format!("<{0}>", "x".repeat(64)).repeat(20_000);
    assert_eq!(tokenize(&nested).unwrap().len(), 20_000);
}

#[test]
fn nesting_depth_fails_closed_past_the_limit() {
    let at_limit = "<a>".repeat(MAX_XML_DEPTH);
    assert_eq!(tokenize(&at_limit).unwrap().len(), MAX_XML_DEPTH);
    let past_limit = "<a>".repeat(MAX_XML_DEPTH + 1);
    let mut reader = XmlReader::unbounded(&past_limit);
    let result: Result<Vec<_>, _> = (&mut reader).collect();
    assert!(matches!(
        result,
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
    assert!(reader.next().is_none());
    // Closed elements free their depth, empty elements take none.
    let wide = "<a><b/></a>".repeat(MAX_XML_DEPTH);
    assert!(XmlReader::unbounded(&wide).all(|event| event.is_ok()));
}

#[test]
fn attributes_past_the_index_and_in_long_tags_read_in_order() {
    // The first four attributes are indexed when the tag is read, the rest are parsed on demand.
    let tag = "<a a=\"1\" b=\"2\" c=\"3\" d=\"4\" e=\"5&amp;\" f=\"6\" a=\"7\"/>";
    let ev = tokenize(tag).unwrap();
    let XmlEvent::Start { attrs, .. } = &ev[0] else {
        panic!("start expected");
    };
    let all: Vec<_> = attrs.iter_raw().collect();
    assert_eq!(
        all,
        [
            ("a", "1"),
            ("b", "2"),
            ("c", "3"),
            ("d", "4"),
            ("e", "5&amp;"),
            ("f", "6"),
            ("a", "7")
        ]
    );
    assert_eq!(attrs.get("e").unwrap(), "5&");
    assert_eq!(attrs.get("a").unwrap(), "1");
    assert!(attrs.get("g").is_none());

    // A tag longer than 64 KiB has no index; every lookup parses.
    let long = format!("<a x=\"{}\" y=\"2\"/>", "v".repeat(70_000));
    let ev = tokenize(&long).unwrap();
    let XmlEvent::Start { attrs, .. } = &ev[0] else {
        panic!("start expected");
    };
    assert_eq!(attrs.get("x").unwrap().len(), 70_000);
    assert_eq!(attrs.get("y").unwrap(), "2");
    assert_eq!(attrs.iter_raw().count(), 2);
}

#[test]
fn a_less_than_sign_inside_a_quoted_value_is_read_as_text() {
    // The tokenizer ends a tag at the first `>` outside quotes, so the value cannot open markup.
    let ev = tokenize("<a v=\"x<y\"/>").unwrap();
    let XmlEvent::Start { attrs, .. } = &ev[0] else {
        panic!("start expected");
    };
    assert_eq!(attrs.get("v").unwrap(), "x<y");
}
