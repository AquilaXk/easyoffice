//! Pull reader over a borrowed XML string for the already-validated XML parts of OOXML, ODF and
//! HWPX. Events borrow the input (`&str` names, `Cow` text), attributes are decoded lazily, and
//! nothing is allocated unless an entity has to be resolved.
//!
//! 참조 구현 (reference implementation): quick-xml 0.42.0 (MIT, Copyright (c) 2016 Johann Tuffe),
//! https://github.com/tafia/quick-xml. The shape follows `src/reader/slice_reader.rs`
//! (`Reader::read_event` over a `&'a str`, events borrow the input), `src/reader/state.rs:314-367`
//! (`emit_start` / `close_expanded_empty`: an empty element yields a start and a deferred end),
//! `src/events/attributes.rs` (`Attributes` iterator parses attributes on demand) and
//! `src/escape.rs:236` (`unescape` returns `Cow`, borrowed when there is no `&`). The event
//! grammar itself is the one of the earlier owned tokenizer, so results stay identical, and the
//! tag-end search keeps its quote awareness. Safe Rust only, no dependency (PD-053, PD-095).
//!
//! Callers must run [`crate::XmlSanitizer::validate_safe_xml`] first (DOCTYPE, entity
//! declarations, and external references are rejected there); this reader never resolves
//! entities beyond the five predefined ones and numeric character references.

use crate::{EasyOfficeError, XmlSanitizer};
use std::borrow::Cow;

/// Upper bound on events read from one XML part by default.
pub(crate) const MAX_XML_EVENTS: usize = 6_000_000;

fn corrupt(msg: &str) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(msg.to_string())
}

/// Attribute list of one start tag, kept as the raw text between the name and `>`. It was checked
/// when the tag was read, so lookups and iteration cannot fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Attrs<'a> {
    raw: &'a str,
}

impl<'a> Attrs<'a> {
    /// The first attribute with this qualified name; the entities of its value are resolved.
    pub(crate) fn get(&self, name: &str) -> Option<Cow<'a, str>> {
        self.iter_raw()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| unescape_cow(v))
    }

    /// Attributes in document order as (qualified name, decoded value).
    #[cfg(test)]
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&'a str, Cow<'a, str>)> {
        self.iter_raw()
            .map(|(name, value)| (name, unescape_cow(value)))
    }

    /// Attributes in document order with their values as written; decode the ones you keep with
    /// [`unescape_cow`].
    pub(crate) fn iter_raw(&self) -> impl Iterator<Item = (&'a str, &'a str)> {
        let raw = self.raw;
        let mut pos = 0usize;
        std::iter::from_fn(move || match next_attr(raw, &mut pos) {
            Ok(Some(pair)) => Some(pair),
            _ => None,
        })
    }
}

/// Looks up an attribute by its qualified name.
pub(crate) fn attr<'a>(attrs: &Attrs<'a>, name: &str) -> Option<Cow<'a, str>> {
    attrs.get(name)
}

/// Resolves the predefined and numeric entities of `text`; borrowed when it has no `&`.
pub(crate) fn unescape_cow(text: &str) -> Cow<'_, str> {
    if XmlSanitizer::find_byte_swar(text.as_bytes(), 0, b'&').is_none() {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(XmlSanitizer::unescape(text))
    }
}

const SWAR_LO: u64 = 0x0101_0101_0101_0101;
const SWAR_HI: u64 = 0x8080_8080_8080_8080;
const SWAR_SPACES: u64 = 0x2020_2020_2020_2020;

#[inline]
const fn swar_has_zero(word: u64) -> u64 {
    word.wrapping_sub(SWAR_LO) & !word & SWAR_HI
}

#[inline]
const fn swar_has_le_20(word: u64) -> u64 {
    word.wrapping_sub(SWAR_LO * 0x21) & !word & SWAR_HI
}

#[inline]
fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i + 8 <= bytes.len() {
        let word = u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
        if word != SWAR_SPACES {
            break;
        }
        i += 8;
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

#[inline]
fn find_attr_name_end(bytes: &[u8], mut i: usize) -> usize {
    const EQ_MASK: u64 = SWAR_LO * (b'=' as u64);
    while i + 8 <= bytes.len() {
        let word = u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
        if (swar_has_le_20(word) | swar_has_zero(word ^ EQ_MASK)) != 0 {
            break;
        }
        i += 8;
    }
    while i < bytes.len() && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

#[inline]
fn find_ascii_ws(bytes: &[u8], mut i: usize) -> usize {
    while i + 8 <= bytes.len() {
        let word = u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
        if swar_has_le_20(word) != 0 {
            break;
        }
        i += 8;
    }
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// Reads the attribute at `*pos`: `Ok(None)` at the end, an error for a missing `=` or quote.
fn next_attr<'a>(
    raw: &'a str,
    pos: &mut usize,
) -> Result<Option<(&'a str, &'a str)>, EasyOfficeError> {
    let bytes = raw.as_bytes();
    let mut i = skip_ws(bytes, *pos);
    if i >= bytes.len() {
        *pos = i;
        return Ok(None);
    }
    let name_start = i;
    i = find_attr_name_end(bytes, i);
    let name = &raw[name_start..i];
    i = skip_ws(bytes, i);
    if i >= bytes.len() || bytes[i] != b'=' {
        return Err(corrupt("XML 속성에 '='가 없습니다."));
    }
    i = skip_ws(bytes, i + 1);
    if i >= bytes.len() || (bytes[i] != b'"' && bytes[i] != b'\'') {
        return Err(corrupt("XML 속성 값에 따옴표가 없습니다."));
    }
    let value_start = i + 1;
    let Some(value_end) = XmlSanitizer::find_byte_swar(bytes, value_start, bytes[i]) else {
        return Err(corrupt("XML 속성 값이 닫히지 않았습니다."));
    };
    *pos = value_end + 1;
    Ok(Some((name, &raw[value_start..value_end])))
}

/// One event of the document. `Start` of an empty element is followed by its `End`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum XmlEvent<'a> {
    Start { name: &'a str, attrs: Attrs<'a> },
    End(&'a str),
    Text(Cow<'a, str>),
}

/// Index of the `>` that closes the start tag at `from`, skipping quoted attribute values.
fn find_tag_end(bytes: &[u8], from: usize) -> Option<usize> {
    const GT_MASK: u64 = SWAR_LO * (b'>' as u64);
    const DQ_MASK: u64 = SWAR_LO * (b'"' as u64);
    const SQ_MASK: u64 = SWAR_LO * (b'\'' as u64);
    let mut i = from;
    while i < bytes.len() {
        while i + 8 <= bytes.len() {
            let word = u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
            let hit = swar_has_zero(word ^ GT_MASK)
                | swar_has_zero(word ^ DQ_MASK)
                | swar_has_zero(word ^ SQ_MASK);
            if hit != 0 {
                i += (hit.trailing_zeros() as usize) >> 3;
                break;
            }
            i += 8;
        }
        if i >= bytes.len() {
            break;
        }
        match bytes[i] {
            b'>' => return Some(i),
            q @ (b'"' | b'\'') => i = XmlSanitizer::find_byte_swar(bytes, i + 1, q)? + 1,
            _ => i += 1,
        }
    }
    None
}

/// Length of the `<!DOCTYPE ...>` declaration at the start of `rest`, including an internal subset
/// (`[...]`, where `>` is not the end) and quoted literals. A declaration is not an element: it
/// produces no event (PD-178). Entities it declares are not expanded (only the five predefined
/// ones are), and the safe-XML check (`XmlSanitizer::validate_safe_xml`) still refuses the
/// declaration for the packages this reader is given.
fn doctype_len(rest: &str) -> Result<usize, EasyOfficeError> {
    let bytes = rest.as_bytes();
    let mut i = "<!DOCTYPE".len();
    let mut subset_depth = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'>' if subset_depth == 0 => return Ok(i + 1),
            b'[' => subset_depth += 1,
            b']' => subset_depth = subset_depth.saturating_sub(1),
            q @ (b'"' | b'\'') => {
                i = XmlSanitizer::find_byte_swar(bytes, i + 1, q)
                    .ok_or_else(|| corrupt("DOCTYPE의 따옴표가 닫히지 않았습니다."))?;
            }
            _ => {}
        }
        i += 1;
    }
    Err(corrupt("DOCTYPE 선언이 닫히지 않았습니다."))
}

/// Pull reader: an iterator of events that stops after the first error.
pub(crate) struct XmlReader<'a> {
    xml: &'a str,
    pos: usize,
    pending_end: Option<&'a str>,
    emitted: usize,
    budget: usize,
    last_start_empty: bool,
}

impl<'a> XmlReader<'a> {
    pub(crate) fn new(xml: &'a str) -> Self {
        Self::with_budget(xml, MAX_XML_EVENTS)
    }

    /// A reader that fails closed once it has produced more than `budget` events.
    pub(crate) fn with_budget(xml: &'a str, budget: usize) -> Self {
        Self {
            xml,
            pos: 0,
            pending_end: None,
            emitted: 0,
            budget,
            last_start_empty: false,
        }
    }

    /// A reader without an event limit, for consumers that keep no event: memory stays constant
    /// and time is linear in the input, which the archive size policy already bounds.
    pub(crate) fn unbounded(xml: &'a str) -> Self {
        Self::with_budget(xml, usize::MAX)
    }

    /// Whether the start event returned last was written as `<name/>`.
    pub(crate) fn last_start_was_empty(&self) -> bool {
        self.last_start_empty
    }

    /// Reads on past the end of the element whose start event was just returned.
    pub(crate) fn skip_subtree(&mut self) -> Result<(), EasyOfficeError> {
        let mut depth = 1usize;
        while depth > 0 {
            match self.next_event()? {
                Some(XmlEvent::Start { .. }) => depth += 1,
                Some(XmlEvent::End(_)) => depth -= 1,
                Some(XmlEvent::Text(_)) => {}
                None => break,
            }
        }
        Ok(())
    }

    /// The events of the element that `start` opened, `start` first and its end last.
    pub(crate) fn collect_subtree(
        &mut self,
        start: XmlEvent<'a>,
    ) -> Result<Vec<XmlEvent<'a>>, EasyOfficeError> {
        let mut events = vec![start];
        let mut depth = 1usize;
        while depth > 0 {
            let Some(event) = self.next_event()? else {
                break;
            };
            match event {
                XmlEvent::Start { .. } => depth += 1,
                XmlEvent::End(_) => depth -= 1,
                XmlEvent::Text(_) => {}
            }
            events.push(event);
        }
        Ok(events)
    }

    /// The next event, `None` at the end of the input.
    pub(crate) fn next_event(&mut self) -> Result<Option<XmlEvent<'a>>, EasyOfficeError> {
        let result = self.read_event();
        if result.is_err() {
            self.pos = self.xml.len();
            self.pending_end = None;
        }
        result
    }

    fn read_event(&mut self) -> Result<Option<XmlEvent<'a>>, EasyOfficeError> {
        if let Some(name) = self.pending_end.take() {
            return Ok(Some(XmlEvent::End(name)));
        }
        let xml = self.xml;
        let bytes = xml.as_bytes();
        while self.pos < bytes.len() {
            if self.emitted > self.budget {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "XML 이벤트 수가 허용 예산을 초과했습니다.".to_string(),
                ));
            }
            let i = self.pos;
            if bytes[i] != b'<' {
                let end = XmlSanitizer::find_byte_swar(bytes, i, b'<').unwrap_or(bytes.len());
                self.pos = end;
                self.emitted += 1;
                return Ok(Some(XmlEvent::Text(unescape_cow(&xml[i..end]))));
            }
            let rest = &xml[i..];
            match bytes.get(i + 1) {
                Some(b'!') if rest.starts_with("<!--") => {
                    let end = rest
                        .find("-->")
                        .ok_or_else(|| corrupt("XML 주석이 닫히지 않았습니다."))?;
                    self.pos = i + end + 3;
                }
                Some(b'!') if rest.starts_with("<![CDATA[") => {
                    let end = rest
                        .find("]]>")
                        .ok_or_else(|| corrupt("CDATA가 닫히지 않았습니다."))?;
                    self.pos = i + end + 3;
                    self.emitted += 1;
                    return Ok(Some(XmlEvent::Text(Cow::Borrowed(&rest[9..end]))));
                }
                Some(b'!') if rest.starts_with("<!DOCTYPE") => {
                    self.pos = i + doctype_len(rest)?;
                }
                Some(b'?') => {
                    let end = rest
                        .find("?>")
                        .ok_or_else(|| corrupt("XML 처리 명령이 닫히지 않았습니다."))?;
                    self.pos = i + end + 2;
                }
                Some(b'/') => {
                    let close = XmlSanitizer::find_byte_swar(bytes, i + 2, b'>')
                        .ok_or_else(|| corrupt("XML 끝 태그가 닫히지 않았습니다."))?;
                    self.pos = close + 1;
                    self.emitted += 1;
                    return Ok(Some(XmlEvent::End(xml[i + 2..close].trim())));
                }
                _ => return self.read_start_tag(i).map(Some),
            }
        }
        Ok(None)
    }

    fn read_start_tag(&mut self, i: usize) -> Result<XmlEvent<'a>, EasyOfficeError> {
        let xml = self.xml;
        let end = find_tag_end(xml.as_bytes(), i + 1)
            .ok_or_else(|| corrupt("XML 시작 태그가 닫히지 않았습니다."))?;
        let mut inner = &xml[i + 1..end];
        let empty = inner.ends_with('/');
        if empty {
            inner = &inner[..inner.len() - 1];
        }
        let name_end = find_ascii_ws(inner.as_bytes(), 0);
        let name = &inner[..name_end];
        if name.is_empty() {
            return Err(corrupt("XML 태그 이름이 비어 있습니다."));
        }
        let raw = &inner[name_end..];
        let mut attr_pos = 0usize;
        while next_attr(raw, &mut attr_pos)?.is_some() {}
        self.pos = end + 1;
        self.emitted += 1 + usize::from(empty);
        self.last_start_empty = empty;
        self.pending_end = empty.then_some(name);
        Ok(XmlEvent::Start {
            name,
            attrs: Attrs { raw },
        })
    }
}

impl<'a> Iterator for XmlReader<'a> {
    type Item = Result<XmlEvent<'a>, EasyOfficeError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_event().transpose()
    }
}

#[cfg(test)]
mod tests;
