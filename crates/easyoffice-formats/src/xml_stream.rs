//! Pull reader over a borrowed XML string for the XML parts of OOXML, ODF and HWPX. Events borrow
//! the input (`&str` names, `Cow` text), attributes are decoded lazily, and nothing is allocated
//! unless an entity has to be resolved.
//!
//! The tokenizer is the `quick-xml` crate (MIT, Copyright (c) 2016 Johann Tuffe,
//! https://github.com/tafia/quick-xml, 0.42.0). This module is its adapter (PD-269): it turns the
//! crate's events into the three the readers consume, and keeps the engine's rules around it, all
//! of them Fail-Closed:
//!
//! - a `<!DOCTYPE` declaration is refused, so no entity can be declared or expanded (the crate
//!   reports it as an event instead of rejecting it);
//! - entities resolve to the five predefined ones and numeric references to XML 1.0 characters
//!   ([`unescape_cow`]); anything else, such as `&bogus;` or `&#0;`, stays as written;
//! - the number of events ([`MAX_XML_EVENTS`]) and the nesting depth ([`MAX_XML_DEPTH`]) are
//!   budgeted, and a malformed tag or attribute ends the reader with an error;
//! - names and attribute values are kept as written (no end-tag matching, no whitespace
//!   normalization, no namespace resolution), the way the readers were written against.
//!
//! Callers still run [`crate::XmlSanitizer::validate_safe_xml`] first for the packages they open.

use crate::{EasyOfficeError, XmlSanitizer};
use quick_xml::errors::{Error as XmlError, SyntaxError};
use quick_xml::events::attributes::{AttrError, Attributes};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use std::borrow::Cow;
use std::ops::Range;

/// Upper bound on events read from one XML part by default.
pub(crate) const MAX_XML_EVENTS: usize = 6_000_000;

/// Deepest element nesting a reader follows. The crate keeps a stack entry for every open element
/// (the earlier reader kept none), so this bounds that stack to a few MiB however large the part
/// is. Real documents stay in the tens; the readers cap their own structures far below this.
pub(crate) const MAX_XML_DEPTH: usize = 1_000_000;

fn corrupt(msg: &str) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(msg.to_string())
}

fn syntax_error(error: &XmlError) -> EasyOfficeError {
    corrupt(match error {
        XmlError::Syntax(SyntaxError::UnclosedPI | SyntaxError::UnclosedXmlDecl) => {
            "XML 처리 명령이 닫히지 않았습니다."
        }
        XmlError::Syntax(SyntaxError::UnclosedComment) => "XML 주석이 닫히지 않았습니다.",
        XmlError::Syntax(SyntaxError::UnclosedCData) => "CDATA가 닫히지 않았습니다.",
        XmlError::Syntax(SyntaxError::UnclosedDoctype) => "DOCTYPE 선언이 닫히지 않았습니다.",
        XmlError::Syntax(
            SyntaxError::UnclosedTag
            | SyntaxError::UnclosedSingleQuotedAttributeValue
            | SyntaxError::UnclosedDoubleQuotedAttributeValue,
        ) => "XML 태그가 닫히지 않았습니다.",
        _ => "XML 구문이 올바르지 않습니다.",
    })
}

fn attribute_error(error: &AttrError) -> EasyOfficeError {
    corrupt(match error {
        AttrError::ExpectedEq(_) => "XML 속성에 '='가 없습니다.",
        AttrError::UnquotedValue(_) | AttrError::ExpectedValue(_) => {
            "XML 속성 값에 따옴표가 없습니다."
        }
        AttrError::ExpectedQuote(..) => "XML 속성 값이 닫히지 않았습니다.",
        AttrError::Duplicated(..) => "XML 속성 이름이 겹칩니다.",
    })
}

/// Attributes of one start tag, kept as the raw text between the name and `>`. They were checked
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
    /// [`unescape_cow`]. A repeated name is listed again, the first one wins in [`Self::get`].
    pub(crate) fn iter_raw(&self) -> impl Iterator<Item = (&'a str, &'a str)> {
        let mut attributes = Attributes::new(self.raw, 0);
        attributes.with_checks(false);
        attributes.filter_map(|attribute| {
            let attribute = attribute.ok()?;
            match attribute.value {
                Cow::Borrowed(value) => Some((attribute.key.0, value)),
                Cow::Owned(_) => None,
            }
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

/// One event of the document. `Start` of an empty element is followed by its `End`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum XmlEvent<'a> {
    Start { name: &'a str, attrs: Attrs<'a> },
    End(&'a str),
    Text(Cow<'a, str>),
}

/// Where `part` lies in `xml`. Every string of an event the crate reads from a `&str` is a slice of
/// that input, so the position follows from the addresses; anything else yields an empty range
/// that the caller's `get` refuses.
fn span_in(xml: &str, part: &str) -> Range<usize> {
    let start = (part.as_ptr() as usize).wrapping_sub(xml.as_ptr() as usize);
    start..start.saturating_add(part.len())
}

/// Pull reader: an iterator of events that stops after the first error.
pub(crate) struct XmlReader<'a> {
    xml: &'a str,
    reader: Reader<&'a [u8]>,
    pending_end: Option<&'a str>,
    finished: bool,
    depth: usize,
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
        let mut reader = Reader::from_str(xml);
        let config = reader.config_mut();
        // The readers match their own tags, and real files carry stray end tags and bare `&`.
        config.check_end_names = false;
        config.allow_unmatched_ends = true;
        config.allow_dangling_amp = true;
        // End tag names are trimmed on both sides when they are read.
        config.trim_markup_names_in_closing_tags = false;
        Self {
            xml,
            reader,
            pending_end: None,
            finished: false,
            depth: 0,
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
        if self.finished {
            return Ok(None);
        }
        let result = self.read_event();
        if result.is_err() {
            self.finished = true;
            self.pending_end = None;
        }
        result
    }

    fn pull(&mut self) -> Result<Event<'a>, EasyOfficeError> {
        self.reader
            .read_event()
            .map_err(|error| syntax_error(&error))
    }

    fn read_event(&mut self) -> Result<Option<XmlEvent<'a>>, EasyOfficeError> {
        if let Some(name) = self.pending_end.take() {
            return Ok(Some(XmlEvent::End(name)));
        }
        loop {
            let event = self.pull()?;
            if matches!(event, Event::Eof) {
                return Ok(None);
            }
            if self.emitted > self.budget {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "XML 이벤트 수가 허용 예산을 초과했습니다.".to_string(),
                ));
            }
            match event {
                Event::Start(tag) => return self.read_start_tag(&tag, false).map(Some),
                Event::Empty(tag) => return self.read_start_tag(&tag, true).map(Some),
                Event::End(tag) => {
                    self.emitted += 1;
                    self.depth = self.depth.saturating_sub(1);
                    let name = self.slice(span_in(self.xml, tag.name().as_ref().trim_ascii()))?;
                    return Ok(Some(XmlEvent::End(name)));
                }
                Event::CData(data) => {
                    self.emitted += 1;
                    return Ok(Some(XmlEvent::Text(data.into_inner())));
                }
                Event::Text(_) | Event::GeneralRef(_) => {
                    return self.read_text_run(&event).map(Some)
                }
                Event::DocType(_) => return Err(EasyOfficeError::XxeOrExternalEntityDetected),
                Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::Eof => {}
            }
        }
    }

    fn slice(&self, span: Range<usize>) -> Result<&'a str, EasyOfficeError> {
        self.xml
            .get(span)
            .ok_or_else(|| corrupt("XML 이벤트 위치를 찾지 못했습니다."))
    }

    /// The input bytes of a text or reference event, where a reference is `&name;`.
    fn text_span(&self, event: &Event<'a>) -> Result<Range<usize>, EasyOfficeError> {
        match event {
            Event::Text(text) => Ok(span_in(self.xml, text)),
            Event::GeneralRef(name) => {
                let span = span_in(self.xml, name);
                Ok(span.start.saturating_sub(1)..span.end + 1)
            }
            _ => Err(corrupt("XML 텍스트 이벤트가 아닙니다.")),
        }
    }

    /// One text event for the characters up to the next tag, comment or CDATA section. The crate
    /// hands out each `&...;` apart from the text around it; they are joined again here and
    /// resolved by [`unescape_cow`], so text and attribute values read the same.
    fn read_text_run(&mut self, first: &Event<'a>) -> Result<XmlEvent<'a>, EasyOfficeError> {
        let mut span = self.text_span(first)?;
        // Text and references follow each other until the next `<` (or the end of the input).
        while self
            .xml
            .as_bytes()
            .get(span.end)
            .is_some_and(|&b| b != b'<')
        {
            let next = self.pull()?;
            span.end = self.text_span(&next)?.end;
        }
        self.emitted += 1;
        Ok(XmlEvent::Text(unescape_cow(self.slice(span)?)))
    }

    fn read_start_tag(
        &mut self,
        tag: &BytesStart<'a>,
        empty: bool,
    ) -> Result<XmlEvent<'a>, EasyOfficeError> {
        let name_len = tag.name().as_ref().len();
        let content = self.slice(span_in(self.xml, tag))?;
        let (name, raw) = content.split_at(name_len);
        if name.is_empty() {
            return Err(corrupt("XML 태그 이름이 비어 있습니다."));
        }
        check_attributes(raw)?;
        if !empty {
            self.depth += 1;
            if self.depth > MAX_XML_DEPTH {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "XML 중첩 깊이가 허용 한도를 초과했습니다.".to_string(),
                ));
            }
        }
        self.emitted += 1 + usize::from(empty);
        self.last_start_empty = empty;
        self.pending_end = empty.then_some(name);
        Ok(XmlEvent::Start {
            name,
            attrs: Attrs { raw },
        })
    }
}

/// Refuses a start tag whose attributes are not `name="value"` pairs, or whose value holds `<`.
fn check_attributes(raw: &str) -> Result<(), EasyOfficeError> {
    if raw.is_empty() {
        return Ok(());
    }
    if XmlSanitizer::find_byte_swar(raw.as_bytes(), 0, b'<').is_some() {
        return Err(corrupt(
            "XML 속성 값에 금지된 문자 '<'가 포함되어 있습니다.",
        ));
    }
    let mut attributes = Attributes::new(raw, 0);
    attributes.with_checks(false);
    attributes
        .try_for_each(|attribute| attribute.map(drop).map_err(|error| attribute_error(&error)))
}

impl<'a> Iterator for XmlReader<'a> {
    type Item = Result<XmlEvent<'a>, EasyOfficeError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_event().transpose()
    }
}

#[cfg(test)]
mod tests;
