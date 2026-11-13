//! The page setup of a document in the open and save JSON (PD-213): `"pageSetup":{"paper":{"width","height"},
//! "margins":{"top","bottom","header","footer","left","right"}}` in twips. `top` and `bottom` are the
//! distances from the page edge to the body, `header` and `footer` those to the header and footer areas
//! (`easyoffice_formats::RichPageMargins`). Either key may be absent; a setup that leaves less than
//! `MIN_PAGE_BODY_TWIPS` of text room, a side over `MAX_PAGE_MARGIN_TWIPS` or a header or footer distance
//! beyond its body distance rejects the save.

use std::fmt::Write;

use easyoffice_formats::{
    EasyOfficeError, RichPageFrame, RichPageMargins, RichPaper, MAX_PAGE_MARGIN_TWIPS,
};

use crate::convert_structured::{field, invalid, opt_bounded, JsonObject};

/// `"pageSetup":{…},` for the open JSON, empty when the file gives no setup.
pub(crate) fn write_page_setup(out: &mut String, frame: &RichPageFrame) {
    if frame.margins.is_none() && frame.paper.is_none() {
        return;
    }
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

/// The `"pageSetup"` of a document payload as the margins and paper of its page frame.
pub(crate) fn parse_page_setup(
    entries: &JsonObject,
) -> Result<(Option<RichPageMargins>, Option<RichPaper>), EasyOfficeError> {
    let Some(value) = field(entries, "pageSetup") else {
        return Ok((None, None));
    };
    let setup = object(value, "pageSetup")?;
    let paper = field(setup, "paper")
        .map(|v| object(v, "pageSetup.paper").and_then(parse_paper))
        .transpose()?;
    let margins = field(setup, "margins")
        .map(|v| object(v, "pageSetup.margins").and_then(parse_margins))
        .transpose()?;
    if let (Some(p), Some(m)) = (paper, margins) {
        if !p.holds(&m) {
            return Err(invalid("구조화 저장: 여백이 커서 본문이 남지 않습니다."));
        }
    }
    Ok((margins, paper))
}
