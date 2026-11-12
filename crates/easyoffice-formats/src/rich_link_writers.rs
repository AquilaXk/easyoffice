//! Hyperlink markup of the DOCX, ODT and HWPX writers: runs of one link are grouped, and each format
//! wraps a group in its own element. Only addresses that pass [`safe_link_target`] are written; any
//! other run is written as plain text.

use crate::rich_document::safe_link_target;
use crate::{RichRun, XmlSanitizer};

/// Consecutive non-empty runs split into groups that share one safe link (`None` for plain text).
pub(crate) fn link_groups(runs: &[RichRun]) -> Vec<(Option<&str>, Vec<&RichRun>)> {
    groups_by(runs, safe_link_target)
}

/// [`link_groups`] for a format without bookmarks (HWPX): an in-document `#_Toc` target is plain text.
pub(crate) fn external_link_groups(runs: &[RichRun]) -> Vec<(Option<&str>, Vec<&RichRun>)> {
    groups_by(runs, |raw| {
        safe_link_target(raw).filter(|url| !url.starts_with('#'))
    })
}

fn groups_by<'a>(
    runs: &'a [RichRun],
    accept: impl Fn(&'a str) -> Option<&'a str>,
) -> Vec<(Option<&'a str>, Vec<&'a RichRun>)> {
    let mut groups: Vec<(Option<&str>, Vec<&RichRun>)> = Vec::new();
    for run in runs.iter().filter(|r| !r.text.is_empty()) {
        let link = run.link.as_deref().and_then(&accept);
        match groups.last_mut() {
            Some((last, members)) if *last == link => members.push(run),
            _ => groups.push((link, vec![run])),
        }
    }
    groups
}

/// Character style of a DOCX run inside a link (`word/styles.xml` defines it).
pub(crate) const DOCX_LINK_STYLE_XML: &str = "<w:style w:type=\"character\" w:styleId=\"Hyperlink\"><w:name w:val=\"Hyperlink\"/><w:rPr><w:color w:val=\"0563C1\"/><w:u w:val=\"single\"/></w:rPr></w:style>";

const DOCX_HYPERLINK_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";

/// External relationships of a DOCX body: one per distinct address, `rIdLink1`, `rIdLink2`, ...
#[derive(Default)]
pub(crate) struct DocxLinks {
    urls: Vec<String>,
}

impl DocxLinks {
    pub(crate) fn rel_id(&mut self, url: &str) -> String {
        let at = self.urls.iter().position(|u| u == url).unwrap_or_else(|| {
            self.urls.push(url.to_string());
            self.urls.len() - 1
        });
        format!("rIdLink{}", at + 1)
    }

    pub(crate) fn relationships(&self) -> String {
        self.urls
            .iter()
            .enumerate()
            .map(|(i, url)| {
                format!(
                    "<Relationship Id=\"rIdLink{}\" Type=\"{DOCX_HYPERLINK_REL_TYPE}\" Target=\"{}\" TargetMode=\"External\"/>",
                    i + 1,
                    XmlSanitizer::escape(url)
                )
            })
            .collect()
    }
}

/// `<w:rPr>` of a run inside a link: the run's own properties after the link's character style.
pub(crate) fn docx_linked_run_properties(properties: &str) -> String {
    const STYLE: &str = "<w:rStyle w:val=\"Hyperlink\"/>";
    match properties.strip_prefix("<w:rPr>") {
        Some(rest) => format!("<w:rPr>{STYLE}{rest}"),
        None => format!("<w:rPr>{STYLE}</w:rPr>"),
    }
}

/// `<hp:fieldBegin>` run that opens a hyperlink field (the address is the `Command` parameter, as the
/// reference editor's HWPX serializer writes it), and the `<hp:fieldEnd>` run that closes it.
pub(crate) fn hwpx_link_begin(field_id: u32, url: &str) -> String {
    let url = XmlSanitizer::escape(url);
    format!(
        "<hp:run charPrIDRef=\"0\"><hp:ctrl><hp:fieldBegin id=\"{field_id}\" type=\"HYPERLINK\" name=\"\" editable=\"0\"><hp:parameters cnt=\"1\" name=\"\"><hp:stringParam name=\"Command\">{url}</hp:stringParam></hp:parameters></hp:fieldBegin></hp:ctrl></hp:run>"
    )
}

pub(crate) fn hwpx_link_end(field_id: u32) -> String {
    format!(
        "<hp:run charPrIDRef=\"0\"><hp:ctrl><hp:fieldEnd beginIDRef=\"{field_id}\"/></hp:ctrl></hp:run>"
    )
}
