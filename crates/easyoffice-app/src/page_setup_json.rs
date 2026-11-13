//! The page setup of a document in the open and save JSON (PD-213): `"pageSetup":{"paper":{"width","height"},
//! "margins":{"top","bottom","header","footer","left","right"}}` in twips. `top` and `bottom` are the
//! distances from the page edge to the body, `header` and `footer` those to the header and footer areas
//! (`easyoffice_formats::RichPageMargins`). Either key may be absent; a setup that leaves less than
//! `MIN_PAGE_BODY_TWIPS` of text room, a side over `MAX_PAGE_MARGIN_TWIPS` or a header or footer distance
//! beyond its body distance rejects the save.

use std::fmt::Write;

use easyoffice_formats::{
    EasyOfficeError, RichBinding, RichBindingKind, RichPageFrame, RichPageMargins, RichPaper,
    MAX_PAGE_MARGIN_TWIPS,
};

use crate::convert_structured::{field, invalid, opt_bounded, JsonObject};

/// `"pageSetup":{…},` for the open JSON, empty when the file gives no setup.
pub(crate) fn write_page_setup(out: &mut String, frame: &RichPageFrame) {
    if frame.margins.is_none() && frame.paper.is_none() {
        return;
    }
    let binding = frame.binding;
    out.push_str("\"pageSetup\":{");
    let mut sep = "";
    if let Some(p) = frame.paper {
        let _ = write!(
            out,
            "\"paper\":{{\"width\":{},\"height\":{}}}",
            p.width, p.height
        );
        sep = ",";
    }
    if let Some(m) = frame.margins {
        let _ = write!(
            out,
            "{sep}\"margins\":{{\"top\":{},\"bottom\":{},\"header\":{},\"footer\":{},\"left\":{},\"right\":{}}}",
            m.top, m.bottom, m.header, m.footer, m.left, m.right
        );
        sep = ",";
    }
    if binding != RichBinding::default() {
        let kind = match binding.kind {
            RichBindingKind::Single => "single",
            RichBindingKind::Duplex => "duplex",
            RichBindingKind::Top => "top",
        };
        let _ = write!(
            out,
            "{sep}\"binding\":{{\"kind\":\"{kind}\",\"gutter\":{}}}",
            binding.gutter
        );
    }
    out.push_str("},");
}

fn object<'a>(
    value: &'a easyoffice_formats::StrictJsonValue,
    key: &str,
) -> Result<&'a JsonObject, EasyOfficeError> {
    value
        .as_object()
        .ok_or_else(|| invalid(format!("구조화 저장: '{key}'는 객체여야 합니다.")))
}

fn twips(obj: &JsonObject, key: &str, min: i64, scope: &str) -> Result<u32, EasyOfficeError> {
    opt_bounded(obj, key, min, i64::from(MAX_PAGE_MARGIN_TWIPS))?
        .map(|v| v as u32)
        .ok_or_else(|| invalid(format!("구조화 저장: '{scope}.{key}'가 필요합니다.")))
}

fn parse_paper(obj: &JsonObject) -> Result<RichPaper, EasyOfficeError> {
    Ok(RichPaper {
        width: twips(obj, "width", 1, "pageSetup.paper")?,
        height: twips(obj, "height", 1, "pageSetup.paper")?,
    })
}

fn parse_margins(obj: &JsonObject) -> Result<RichPageMargins, EasyOfficeError> {
    let value = |key: &str| twips(obj, key, 0, "pageSetup.margins");
    let margins = RichPageMargins {
        top: value("top")?,
        bottom: value("bottom")?,
        header: value("header")?,
        footer: value("footer")?,
        left: value("left")?,
        right: value("right")?,
    };
    if margins.header > margins.top || margins.footer > margins.bottom {
        return Err(invalid(
            "구조화 저장: 머리말·꼬리말 간격은 위·아래 여백보다 클 수 없습니다.",
        ));
    }
    Ok(margins)
}

pub(crate) type ParsedSetup = (Option<RichPageMargins>, Option<RichPaper>, RichBinding);

/// The `"pageSetup"` of a document payload as the margins and paper of its page frame.
pub(crate) fn parse_page_setup(entries: &JsonObject) -> Result<ParsedSetup, EasyOfficeError> {
    let Some(value) = field(entries, "pageSetup") else {
        return Ok((None, None, RichBinding::default()));
    };
    let setup = object(value, "pageSetup")?;
    let paper = field(setup, "paper")
        .map(|v| object(v, "pageSetup.paper").and_then(parse_paper))
        .transpose()?;
    let margins = field(setup, "margins")
        .map(|v| object(v, "pageSetup.margins").and_then(parse_margins))
        .transpose()?;
    let binding = field(setup, "binding")
        .map(|v| object(v, "pageSetup.binding").and_then(parse_binding))
        .transpose()?
        .unwrap_or_default();
    if let (Some(p), Some(m)) = (paper, margins) {
        if !p.holds_bound(&m, binding) {
            return Err(invalid("구조화 저장: 여백이 커서 본문이 남지 않습니다."));
        }
    }
    Ok((margins, paper, binding))
}

fn parse_binding(obj: &JsonObject) -> Result<RichBinding, EasyOfficeError> {
    let kind = match crate::convert_structured::opt_str(obj, "kind")? {
        Some("single") | None => RichBindingKind::Single,
        Some("duplex") => RichBindingKind::Duplex,
        Some("top") => RichBindingKind::Top,
        Some(_) => {
            return Err(invalid(
                "구조화 저장: 'pageSetup.binding.kind'가 올바르지 않습니다.",
            ))
        }
    };
    let gutter =
        opt_bounded(obj, "gutter", 0, i64::from(MAX_PAGE_MARGIN_TWIPS))?.unwrap_or(0) as u32;
    Ok(RichBinding { kind, gutter })
}
