//! The page setup of a document in the open and save JSON (PD-213): `"pageSetup":{"paper":{"width","height"},
//! "margins":{"top","bottom","header","footer","left","right"}}` in twips. `top` and `bottom` are the
//! distances from the page edge to the body, `header` and `footer` those to the header and footer areas
//! (`easyoffice_formats::RichPageMargins`). Either key may be absent; a setup that leaves less than
//! `MIN_PAGE_BODY_TWIPS` of text room, a side over `MAX_PAGE_MARGIN_TWIPS` or a header or footer distance
//! beyond its body distance rejects the save.

use easyoffice_formats::json_out::push_json;
use easyoffice_formats::{
    EasyOfficeError, RichBinding, RichBindingKind, RichPageFrame, RichPageMargins, RichPaper,
    MAX_PAGE_MARGIN_TWIPS,
};

use crate::convert_structured::{field, invalid, opt_bounded, JsonObject};
use serde::Serialize;

#[derive(Serialize)]
struct PaperOut {
    width: u32,
    height: u32,
}

#[derive(Serialize)]
struct MarginsOut {
    top: u32,
    bottom: u32,
    header: u32,
    footer: u32,
    left: u32,
    right: u32,
}

#[derive(Serialize)]
struct BindingOut {
    kind: &'static str,
    gutter: u32,
}

#[derive(Serialize)]
struct PageSetupOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    paper: Option<PaperOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    margins: Option<MarginsOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    binding: Option<BindingOut>,
}

/// `"pageSetup":{…},` for the open JSON, empty when the file gives no setup.
pub(crate) fn write_page_setup(out: &mut String, frame: &RichPageFrame) {
    if frame.margins.is_none() && frame.paper.is_none() {
        return;
    }
    let binding = frame.binding;
    let setup = PageSetupOut {
        paper: frame.paper.map(|p| PaperOut {
            width: p.width,
            height: p.height,
        }),
        margins: frame.margins.map(|m| MarginsOut {
            top: m.top,
            bottom: m.bottom,
            header: m.header,
            footer: m.footer,
            left: m.left,
            right: m.right,
        }),
        binding: (binding != RichBinding::default()).then_some(BindingOut {
            kind: match binding.kind {
                RichBindingKind::Single => "single",
                RichBindingKind::Duplex => "duplex",
                RichBindingKind::Top => "top",
            },
            gutter: binding.gutter,
        }),
    };
    out.push_str("\"pageSetup\":");
    push_json(out, &setup);
    out.push(',');
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
