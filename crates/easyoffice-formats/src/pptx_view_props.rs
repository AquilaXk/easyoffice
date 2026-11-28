//! Parser-based reading, checking and patching of `ppt/viewProps.xml` (`PD-402`), the part that holds the drawing guides of
//! a deck next to the views of the file (normal, outline, notes, sorter, grid).
//!
//! One check, [`scan`], stands at the three doors of the part: the reader (a part that fails it is not kept, the deck gets a
//! fresh one), the JSON boundary (a part that fails it refuses the save) and the writer (a part, or a patch of it, that fails it
//! is replaced by a fresh one). It is the `quick-xml` tokenizer the other readers already use, with end names checked, and it
//! accepts only: well-formed XML of at most [`MAX_VIEW_PROPS_BYTES`], one root element that is the PresentationML `viewPr`
//! (the namespace is declared on the root, under any prefix), nothing but white space after the root, no DOCTYPE or entity
//! declaration, no processing instruction (the XML declaration apart, at the very start, with no encoding or UTF-8), and no
//! element below the root that redeclares the namespace.
//!
//! The patch ([`patch`]) takes byte ranges from the same events, so a comment, a single-quoted attribute or a `>` inside an
//! attribute value cannot be taken for markup: it replaces the `showGuides` of the first slide view's `cSldViewPr` and its
//! `guideLst` and leaves every other byte of the part as it was.

use std::fmt::Write;
use std::ops::Range;

use quick_xml::escape::escape;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use quick_xml::XmlVersion;

use crate::rich_slide_guides::{SlideGuide, MAX_VIEW_PROPS_BYTES};
use crate::EasyOfficeError;

const PML_NS: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const DML_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
/// Children of the root that follow `slideViewPr` in the schema order.
const AFTER_SLIDE_VIEW: [&str; 6] = [
    "outlineViewPr",
    "notesTextViewPr",
    "sorterViewPr",
    "notesViewPr",
    "gridSpacing",
    "extLst",
];

fn bad(why: &str) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(format!(
        "PPTX 보기 속성(viewProps)이 올바르지 않습니다: {why}"
    ))
}

/// The first `p:cSldViewPr` of the first slide view.
#[derive(Debug, Default)]
pub(crate) struct CsldView {
    pub tag: Range<usize>,
    pub span: Range<usize>,
    pub empty: bool,
    /// Start of the end tag (meaningless when `empty`).
    pub inner_end: usize,
    pub guide_lst: Option<Range<usize>>,
    pub ext_lst: Option<usize>,
    pub show: bool,
}

/// The first `p:slideViewPr`.
#[derive(Debug, Default)]
pub(crate) struct SlideView {
    pub span: Range<usize>,
    pub empty: bool,
    pub inner_start: usize,
    pub csld: Option<CsldView>,
}

/// What a checked view part holds: where its pieces lie and the guides of the slide view.
#[derive(Debug, Default)]
pub(crate) struct ViewScan {
    /// `"p:"` or `""`: the prefix the root (and so the new elements) uses.
    pub prefix: String,
    pub root_tag: Range<usize>,
    pub root_empty: bool,
    pub root_close: usize,
    pub slide: Option<SlideView>,
    /// Where a slide view goes when the part has none: before the first view that follows it, else at the root's end.
    pub before_later: Option<usize>,
    pub guides: Vec<SlideGuide>,
}

#[derive(Default)]
struct Scanner {
    out: ViewScan,
    stack: Vec<String>,
    root_seen: bool,
    done: bool,
    in_slide: bool,
    in_csld: bool,
    in_list: bool,
    list_start: usize,
}

fn local(qname: &str) -> &str {
    qname.rsplit(':').next().unwrap_or(qname)
}

fn attrs_of(e: &BytesStart<'_>) -> Result<Vec<(String, String)>, EasyOfficeError> {
    e.attributes()
        .map(|a| {
            let a = a.map_err(|_| bad("속성"))?;
            let value = a
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(|_| bad("속성 값"))?;
            Ok((a.key.as_ref().to_string(), value.into_owned()))
        })
        .collect()
}

impl Scanner {
    /// A start (or empty) tag at `[before, after)`.
    fn open(
        &mut self,
        e: &BytesStart<'_>,
        before: usize,
        after: usize,
        empty: bool,
    ) -> Result<(), EasyOfficeError> {
        if self.done {
            return Err(bad("루트 요소 뒤에 내용이 있습니다"));
        }
        let name = e.name().as_ref().to_string();
        let attrs = attrs_of(e)?;
        if self.stack.is_empty() {
            if self.root_seen || local(&name) != "viewPr" {
                return Err(bad("루트 요소가 viewPr가 아닙니다"));
            }
            let prefix = name.split_once(':').map(|(p, _)| p);
            let decl = prefix.map_or_else(|| "xmlns".to_string(), |p| format!("xmlns:{p}"));
            if !attrs.iter().any(|(k, v)| *k == decl && v == PML_NS) {
                return Err(bad("루트 요소가 PresentationML 이름 공간이 아닙니다"));
            }
            self.root_seen = true;
            self.out.prefix = prefix.map_or_else(String::new, |p| format!("{p}:"));
            self.out.root_tag = before..after;
            self.out.root_empty = empty;
        } else {
            let root_decl = if self.out.prefix.is_empty() {
                "xmlns".to_string()
            } else {
                format!("xmlns:{}", self.out.prefix.trim_end_matches(':'))
            };
            if attrs.iter().any(|(k, _)| *k == root_decl) {
                return Err(bad("하위 요소가 이름 공간을 다시 선언합니다"));
            }
        }
        self.stack.push(name);
        let name = local(&self.stack[self.stack.len() - 1]).to_string();
        self.record(&name, &attrs, before, after, empty);
        if empty {
            self.close(before, after, true);
        }
        Ok(())
    }

    fn record(
        &mut self,
        name: &str,
        attrs: &[(String, String)],
        before: usize,
        after: usize,
        empty: bool,
    ) {
        let depth = self.stack.len();
        let attr = |key: &str| {
            attrs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
        };
        if depth == 2 {
            if name == "slideViewPr" && self.out.slide.is_none() {
                self.in_slide = true;
                self.out.slide = Some(SlideView {
                    span: before..after,
                    empty,
                    inner_start: after,
                    csld: None,
                });
            } else if AFTER_SLIDE_VIEW.contains(&name) && self.out.before_later.is_none() {
                self.out.before_later = Some(before);
            }
        } else if depth == 3 && self.in_slide && name == "cSldViewPr" {
            if let Some(slide) = self.out.slide.as_mut().filter(|s| s.csld.is_none()) {
                self.in_csld = true;
                slide.csld = Some(CsldView {
                    tag: before..after,
                    span: before..after,
                    empty,
                    show: attr("showGuides").is_some_and(|v| v == "1" || v == "true"),
                    ..CsldView::default()
                });
            }
        } else if depth == 4 && self.in_csld {
            if let Some(csld) = self.out.slide.as_mut().and_then(|s| s.csld.as_mut()) {
                if name == "guideLst" && csld.guide_lst.is_none() {
                    self.in_list = true;
                    self.list_start = before;
                    csld.guide_lst = Some(before..after);
                } else if name == "extLst" && csld.ext_lst.is_none() {
                    csld.ext_lst = Some(before);
                }
            }
        } else if depth == 5 && self.in_list && name == "guide" {
            let pos = attr("pos").and_then(|v| v.trim().parse::<i32>().ok());
            if let Some(pos) = pos {
                let vertical = attr("orient") != Some("horz");
                self.out.guides.push(SlideGuide { vertical, pos });
            }
        }
    }

    /// An end tag (or the end of an empty tag) that starts at `before` and ends at `after`.
    fn close(&mut self, before: usize, after: usize, empty: bool) {
        let depth = self.stack.len();
        if depth == 1 {
            self.out.root_close = before;
            self.done = true;
        } else if depth == 2 && self.in_slide {
            self.in_slide = false;
            if let Some(slide) = self.out.slide.as_mut() {
                slide.span.end = after;
            }
        } else if depth == 3 && self.in_csld {
            self.in_csld = false;
            if let Some(csld) = self.out.slide.as_mut().and_then(|s| s.csld.as_mut()) {
                csld.span.end = after;
                csld.inner_end = if empty { after } else { before };
            }
        } else if depth == 4 && self.in_list {
            self.in_list = false;
            if let Some(csld) = self.out.slide.as_mut().and_then(|s| s.csld.as_mut()) {
                csld.guide_lst = Some(self.list_start..after);
            }
        }
        self.stack.pop();
    }
}

/// Checks `xml` as a view part (see the module) and finds its pieces.
pub(crate) fn scan(xml: &str) -> Result<ViewScan, EasyOfficeError> {
    if xml.len() > MAX_VIEW_PROPS_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "PPTX 보기 속성(viewProps)이 {MAX_VIEW_PROPS_BYTES}바이트를 넘습니다."
        )));
    }
    let mut reader = Reader::from_str(xml);
    let config = reader.config_mut();
    config.check_end_names = true;
    config.allow_unmatched_ends = false;
    config.allow_dangling_amp = false;
    let mut sc = Scanner::default();
    loop {
        let before = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        let event = reader.read_event().map_err(|_| bad("XML 구문"))?;
        let after = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        let outside = sc.stack.is_empty();
        match event {
            Event::Start(e) => sc.open(&e, before, after, false)?,
            Event::Empty(e) => sc.open(&e, before, after, true)?,
            Event::End(_) => sc.close(before, after, false),
            Event::Text(t) => {
                if outside && !t.trim_ascii().is_empty() {
                    return Err(bad("루트 요소 밖에 글이 있습니다"));
                }
            }
            Event::CData(_) | Event::Comment(_) if !outside || !sc.done => {}
            Event::CData(_) | Event::Comment(_) => {
                return Err(bad("루트 요소 뒤에 내용이 있습니다"))
            }
            Event::GeneralRef(r) => {
                let known = ["amp", "lt", "gt", "quot", "apos"].contains(&&*r) || r.is_char_ref();
                if outside || !known {
                    return Err(bad("알 수 없는 개체 참조"));
                }
            }
            Event::Decl(d) => {
                let utf8 = d
                    .encoding()
                    .is_none_or(|e| e.is_ok_and(|v| v.eq_ignore_ascii_case("utf-8")));
                if before != 0 || !utf8 {
                    return Err(bad("XML 선언"));
                }
            }
            Event::DocType(_) | Event::PI(_) => return Err(bad("DOCTYPE 또는 처리 지시")),
            Event::Eof => break,
        }
    }
    if !sc.stack.is_empty() || !sc.done {
        return Err(bad("닫히지 않은 요소"));
    }
    Ok(sc.out)
}

/// Checks a view part: well-formed, one PresentationML `viewPr` root, no DOCTYPE or processing instruction, at most
/// [`MAX_VIEW_PROPS_BYTES`] (the check every door of the part uses).
pub fn validate_view_props(xml: &str) -> Result<(), EasyOfficeError> {
    scan(xml).map(|_| ())
}

fn csld_fragment(pp: &str, show: bool, list: &str) -> String {
    let show = if show { " showGuides=\"1\"" } else { "" };
    format!(
        "<{pp}cSldViewPr{show}><{pp}cViewPr varScale=\"1\"><{pp}scale><a:sx xmlns:a=\"{DML_NS}\" n=\"100\" d=\"100\"/><a:sy xmlns:a=\"{DML_NS}\" n=\"100\" d=\"100\"/></{pp}scale><{pp}origin x=\"0\" y=\"0\"/></{pp}cViewPr>{list}</{pp}cSldViewPr>"
    )
}

/// A whole slide view with the guides.
pub(crate) fn slide_view_fragment(pp: &str, show: bool, list: &str) -> String {
    format!(
        "<{pp}slideViewPr>{}</{pp}slideViewPr>",
        csld_fragment(pp, show, list)
    )
}

/// The start tag `src` of `cSldViewPr` with `showGuides` set as asked and every other attribute kept (values decoded and
/// written again), as an open tag.
fn csld_open_tag(src: &str, show: bool) -> Option<String> {
    let mut reader = Reader::from_str(src);
    let (Event::Start(e) | Event::Empty(e)) = reader.read_event().ok()? else {
        return None;
    };
    let mut out = format!("<{}", e.name().as_ref());
    for a in e.attributes() {
        let a = a.ok()?;
        if a.key.as_ref() == "showGuides" {
            continue;
        }
        let value = a.normalized_value(XmlVersion::Implicit1_0).ok()?;
        let _ = write!(out, " {}=\"{}\"", a.key.as_ref(), escape(value.as_ref()));
    }
    if show {
        out.push_str(" showGuides=\"1\"");
    }
    out.push('>');
    Some(out)
}

/// `raw` with the first slide view's `showGuides` and `guideLst` set and nothing else changed; `None` when a piece cannot be
/// rewritten (the caller then writes a fresh part).
pub(crate) fn patch(raw: &str, scan: &ViewScan, show: bool, list: &str) -> Option<String> {
    let pp = scan.prefix.as_str();
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    match &scan.slide {
        None if scan.root_empty => {
            let open = raw[scan.root_tag.clone()]
                .trim_end_matches('>')
                .trim_end()
                .trim_end_matches('/')
                .trim_end();
            let close = format!("</{pp}viewPr>");
            edits.push((
                scan.root_tag.clone(),
                format!("{open}>{}{close}", slide_view_fragment(pp, show, list)),
            ));
        }
        None => {
            let at = scan.before_later.unwrap_or(scan.root_close);
            edits.push((at..at, slide_view_fragment(pp, show, list)));
        }
        Some(slide) if slide.empty => {
            edits.push((slide.span.clone(), slide_view_fragment(pp, show, list)))
        }
        Some(SlideView {
            inner_start,
            csld: None,
            ..
        }) => {
            edits.push((*inner_start..*inner_start, csld_fragment(pp, show, list)));
        }
        Some(SlideView { csld: Some(c), .. }) if c.empty => {
            let open = csld_open_tag(&raw[c.tag.clone()], show)?;
            edits.push((c.span.clone(), format!("{open}{list}</{pp}cSldViewPr>")));
        }
        Some(SlideView { csld: Some(c), .. }) => {
            edits.push((c.tag.clone(), csld_open_tag(&raw[c.tag.clone()], show)?));
            match &c.guide_lst {
                Some(range) => edits.push((range.clone(), list.to_string())),
                None if !list.is_empty() => {
                    let at = c.ext_lst.unwrap_or(c.inner_end);
                    edits.push((at..at, list.to_string()));
                }
                None => {}
            }
        }
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut out = raw.to_string();
    for (range, text) in edits {
        if !raw.is_char_boundary(range.start)
            || !raw.is_char_boundary(range.end)
            || range.end > raw.len()
        {
            return None;
        }
        out.replace_range(range, &text);
    }
    Some(out)
}
