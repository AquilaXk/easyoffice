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
fn a_doctype_declaration_is_no_element() {
    // The owned tokenizer read it as a start tag named `!DOCTYPE` (and ended it at the first `>`
    // inside an internal subset).
    for xml in [
        "<!DOCTYPE a><a/>",
        "<!DOCTYPE a SYSTEM \"a.dtd\"><a/>",
        "<!DOCTYPE a [<!ELEMENT a ANY>]><a/>",
        "<!DOCTYPE a [<!ENTITY x \"]>\">]><a/>",
    ] {
        let ev = owned(tokenize(xml).unwrap());
        assert_eq!(
            ev,
            vec![
                Legacy::Start {
                    name: "a".into(),
                    attrs: vec![]
                },
                Legacy::End("a".into()),
            ],
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
        assert_same(xml);
    }
}

#[test]
fn reader_matches_the_owned_tokenizer_on_mutated_samples() {
    // Every truncation and every single-byte replacement of each sample.
    for xml in SAMPLES.iter().filter(|x| x.is_ascii()) {
        for cut in 0..=xml.len() {
            assert_same(&xml[..cut]);
        }
        for at in 0..xml.len() {
            for b in *b"<>\"'/=& a" {
                let mut bytes = xml.as_bytes().to_vec();
                bytes[at] = b;
                assert_same(std::str::from_utf8(&bytes).unwrap());
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
    // Deep nesting needs no stack: the reader keeps no element stack.
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
