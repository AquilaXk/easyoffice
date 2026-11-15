use std::collections::HashMap;

use crate::xml_stream::{attr, XmlEvent, XmlReader};
use crate::EasyOfficeError;

pub(crate) const MAX_STYLE_INHERIT_DEPTH: usize = 16;

#[derive(Default, Clone, Debug)]
pub(crate) struct DocxStyle {
    pub(crate) name: Option<String>,
    pub(crate) based_on: Option<String>,
    pub(crate) outline_lvl: Option<u8>,
}

/// styleId -> style metadata (`w:name`, `w:basedOn`, `w:outlineLvl`).
/// Adapted from BSD-2-Clause and MIT reference implementations (see `THIRD_PARTY_NOTICES.md`).
pub(crate) fn read_styles(xml: &str) -> Result<HashMap<String, DocxStyle>, EasyOfficeError> {
    let mut out: HashMap<String, DocxStyle> = HashMap::new();
    let mut current: Option<String> = None;
    for ev in XmlReader::new(xml) {
        match ev? {
            XmlEvent::Start {
                name: "w:style",
                attrs,
            } => {
                if let Some(id) = attr(&attrs, "w:styleId") {
                    out.entry(id.to_string()).or_default();
                    current = Some(id.into_owned());
                } else {
                    current = None;
                }
            }
            XmlEvent::Start {
                name: "w:name",
                attrs,
            } => {
                if let (Some(id), Some(val)) = (current.as_ref(), attr(&attrs, "w:val")) {
                    out.entry(id.clone()).or_default().name = Some(val.to_string());
                }
            }
            XmlEvent::Start {
                name: "w:basedOn",
                attrs,
            } => {
                if let (Some(id), Some(val)) = (current.as_ref(), attr(&attrs, "w:val")) {
                    out.entry(id.clone()).or_default().based_on = Some(val.to_string());
                }
            }
            XmlEvent::Start {
                name: "w:outlineLvl",
                attrs,
            } => {
                if let (Some(id), Some(val)) = (current.as_ref(), attr(&attrs, "w:val")) {
                    if let Ok(lvl) = val.parse::<u8>() {
                        out.entry(id.clone()).or_default().outline_lvl = Some(lvl);
                    }
                }
            }
            XmlEvent::End("w:style") => current = None,
            _ => {}
        }
    }
    Ok(out)
}
