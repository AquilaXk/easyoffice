//! Minimal event tokenizer for the already-validated XML parts of DOCX, HWPX, and ODT.
//!
//! Callers must run [`crate::XmlSanitizer::validate_safe_xml`] first (DOCTYPE, entity
//! declarations, and external references are rejected there); this tokenizer never resolves
//! entities beyond the five predefined ones and numeric character references.

use crate::{EasyOfficeError, RichRun, RichScript, XmlSanitizer};

/// Upper bound on events produced from one XML part.
const MAX_XML_EVENTS: usize = 6_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum XmlEvent {
    Start {
        name: String,
        attrs: Vec<(String, String)>,
    },
    End(String),
    Text(String),
}

/// Looks up an attribute by its qualified name.
pub(crate) fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

/// Boolean OOXML-style toggle value: absent or anything but `0`/`false`/`off`/`none` is true.
pub(crate) fn toggle_on(value: Option<&str>) -> bool {
    !matches!(
        value.map(str::trim),
        Some("0" | "false" | "off" | "none" | "NONE")
    )
}

/// Character format shared by every reader: the four toggles plus size, font, and colors.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CharFmt {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub size_half_pt: Option<u16>,
    pub font: Option<String>,
    pub color: Option<String>,
    pub highlight: Option<String>,
    pub script: RichScript,
}

impl CharFmt {
    #[inline]
    pub(crate) fn style_bits(&self) -> u8 {
        let script_bits = match self.script {
            RichScript::Normal => 0,
            RichScript::Superscript => 1 << 4,
            RichScript::Subscript => 2 << 4,
        };
        u8::from(self.bold)
            | (u8::from(self.italic) << 1)
            | (u8::from(self.underline) << 2)
            | (u8::from(self.strike) << 3)
            | script_bits
    }

    #[inline]
    fn matches_run(&self, run: &RichRun) -> bool {
        let run_script_bits = match run.script {
            RichScript::Normal => 0,
            RichScript::Superscript => 1 << 4,
            RichScript::Subscript => 2 << 4,
        };
        let run_bits = u8::from(run.bold)
            | (u8::from(run.italic) << 1)
            | (u8::from(run.underline) << 2)
            | (u8::from(run.strike) << 3)
            | run_script_bits;
        self.style_bits() == run_bits
            && self.size_half_pt == run.size_half_pt
            && self.font == run.font
            && self.color == run.color
            && self.highlight == run.highlight
    }
}

/// Appends `text` as a run, merging it into the previous run when the format is identical.
pub(crate) fn push_run(runs: &mut Vec<RichRun>, text: &str, fmt: &CharFmt) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = runs.last_mut() {
        if fmt.matches_run(last) {
            last.text.push_str(text);
            return;
        }
    }
    runs.push(RichRun {
        text: text.to_string(),
        bold: fmt.bold,
        italic: fmt.italic,
        underline: fmt.underline,
        strike: fmt.strike,
        size_half_pt: fmt.size_half_pt,
        font: fmt.font.clone(),
        color: fmt.color.clone(),
        highlight: fmt.highlight.clone(),
        script: fmt.script,
    });
}

/// Heading level from a style name such as `heading 2`, `Heading2`, `제목 3`, `개요 1`, `Title`.
pub(crate) fn heading_level_from_name(name: &str) -> u8 {
    let s = name.trim_matches(|c: char| c.is_whitespace() || c == '_' || c == '-');
    if s.eq_ignore_ascii_case("title") {
        return 1;
    }
    let rest = if s
        .get(..7)
        .is_some_and(|p| p.eq_ignore_ascii_case("heading") || p.eq_ignore_ascii_case("outline"))
    {
        Some(&s[7..])
    } else {
        s.strip_prefix("제목").or_else(|| s.strip_prefix("개요"))
    };
    if let Some(tail) = rest {
        let digits = tail.trim_matches(|c: char| c.is_whitespace() || c == '_' || c == '-');
        if let Ok(n) = digits.parse::<u8>() {
            return n.clamp(1, 6);
        }
    }
    0
}

fn corrupt(msg: &str) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(msg.to_string())
}

fn parse_attrs(raw: &str) -> Result<Vec<(String, String)>, EasyOfficeError> {
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
        let Some(val_end) = crate::XmlSanitizer::find_byte_swar(bytes, value_start, quote) else {
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
pub(crate) fn tokenize(xml: &str) -> Result<Vec<XmlEvent>, EasyOfficeError> {
    let bytes = xml.as_bytes();
    let mut events = Vec::with_capacity((bytes.len() / 24).min(MAX_XML_EVENTS));
    let mut i = 0usize;
    while i < bytes.len() {
        if events.len() > MAX_XML_EVENTS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "XML 이벤트 수가 허용 예산을 초과했습니다.".to_string(),
            ));
        }
        if bytes[i] != b'<' {
            let end = crate::XmlSanitizer::find_byte_swar(bytes, i, b'<').unwrap_or(bytes.len());
            events.push(XmlEvent::Text(XmlSanitizer::unescape(&xml[i..end])));
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
            events.push(XmlEvent::Text(rest[9..end].to_string()));
            i += end + 3;
        } else if rest.starts_with("<?") {
            let end = rest
                .find("?>")
                .ok_or_else(|| corrupt("XML 처리 명령이 닫히지 않았습니다."))?;
            i += end + 2;
        } else if rest.starts_with("</") {
            let end = rest
                .find('>')
                .ok_or_else(|| corrupt("XML 끝 태그가 닫히지 않았습니다."))?;
            events.push(XmlEvent::End(rest[2..end].trim().to_string()));
            i += end + 1;
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
            let attrs = parse_attrs(&inner[name_end..])?;
            events.push(XmlEvent::Start {
                name: name.clone(),
                attrs,
            });
            if empty {
                events.push(XmlEvent::End(name));
            }
            i += end + 1;
        }
    }
    Ok(events)
}

/// Returns the index just past the end event matching the start event at `start`.
pub(crate) fn skip_subtree(events: &[XmlEvent], start: usize) -> usize {
    let mut depth = 0usize;
    for (offset, ev) in events[start..].iter().enumerate() {
        match ev {
            XmlEvent::Start { .. } => depth += 1,
            XmlEvent::End(_) => {
                depth -= 1;
                if depth == 0 {
                    return start + offset + 1;
                }
            }
            XmlEvent::Text(_) => {}
        }
    }
    events.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_nested_elements_attributes_and_entities() {
        let ev =
            tokenize("<?xml version=\"1.0\"?><a x=\"1&amp;2\"><b/>t&lt;<!-- c --></a>").unwrap();
        assert_eq!(
            ev,
            vec![
                XmlEvent::Start {
                    name: "a".into(),
                    attrs: vec![("x".into(), "1&2".into())]
                },
                XmlEvent::Start {
                    name: "b".into(),
                    attrs: vec![]
                },
                XmlEvent::End("b".into()),
                XmlEvent::Text("t<".into()),
                XmlEvent::End("a".into()),
            ]
        );
    }

    #[test]
    fn rejects_unterminated_markup() {
        assert!(tokenize("<a").is_err());
        assert!(tokenize("<a x=1>").is_err());
        assert!(tokenize("<!-- x").is_err());
    }

    #[test]
    fn skip_subtree_returns_index_after_matching_end() {
        let ev = tokenize("<a><b><c/></b><d/></a>").unwrap();
        assert_eq!(skip_subtree(&ev, 1), 5);
    }
}
