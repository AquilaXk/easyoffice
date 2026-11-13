//! JSON form of a sheet's page setup, shared by the open answer and the save request (PD-213).
//!
//! `page`: `{"landscape": bool, "paper": code, "margins": {"left","right","top","bottom","header","footer"},
//! "scale": {"percent": n} | {"fitWidth": n, "fitHeight": n}, "centerH": bool, "centerV": bool, "grid": bool,
//! "headings": bool, "printArea": ["A1:D10", ...], "titleRows": [first, last], "titleCols": [first, last]}`.
//! Margins are thousandths of an inch (`SheetPage::margins`), rows and columns are 1-based and inclusive,
//! `paper` is the code of `pageSetup@paperSize`, and a fit of `0` leaves that direction as long as it needs.
//! Input past a bound (`SheetPage::within_bounds`: scale 10 to 400, pages across and down, margins, ranges, the
//! repeated rows and columns) is refused, not trimmed.

use std::fmt::Write;

use easyoffice_formats::{
    CellBox, EasyOfficeError, PageMargins, PageScale, SheetPage, SheetStyling, StrictJsonValue,
};

const ROWS: u32 = 1_048_576;
const COLS: u32 = 16_384;

fn bad(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("쪽 설정 JSON이 올바르지 않습니다: {what}"))
}

type Obj<'a> = &'a [(String, StrictJsonValue)];

fn field<'a>(obj: Obj<'a>, key: &str) -> Option<&'a StrictJsonValue> {
    StrictJsonValue::get_field(obj, key)
}

fn flag(obj: Obj<'_>, key: &str) -> Result<bool, EasyOfficeError> {
    field(obj, key).map_or(Ok(false), |v| v.as_bool().ok_or_else(|| bad(key)))
}

fn number(v: Option<&StrictJsonValue>, what: &str) -> Result<u32, EasyOfficeError> {
    v.and_then(StrictJsonValue::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| bad(what))
}

fn parse_margins(v: &StrictJsonValue) -> Result<PageMargins, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("margins"))?;
    let side = |key: &str| number(field(obj, key), key);
    Ok(PageMargins {
        left: side("left")?,
        right: side("right")?,
        top: side("top")?,
        bottom: side("bottom")?,
        header: side("header")?,
        footer: side("footer")?,
    })
}

fn parse_scale(v: &StrictJsonValue) -> Result<PageScale, EasyOfficeError> {
    let obj = v.as_object().ok_or_else(|| bad("scale"))?;
    if let Some(p) = field(obj, "percent") {
        let percent =
            u16::try_from(number(Some(p), "scale.percent")?).map_err(|_| bad("scale.percent"))?;
        return Ok(PageScale::Percent(percent));
    }
    let pages = |key: &str| {
        let n = number(field(obj, key), key)?;
        u16::try_from(n).map_err(|_| bad(key))
    };
    Ok(PageScale::Fit {
        width: pages("fitWidth")?,
        height: pages("fitHeight")?,
    })
}

/// `[first, last]` (1-based, inclusive) as a zero-based run of at most `limit` lines.
fn parse_run(v: &StrictJsonValue, limit: u32, what: &str) -> Result<(u32, u32), EasyOfficeError> {
    let pair = v
        .as_array()
        .filter(|p| p.len() == 2)
        .ok_or_else(|| bad(what))?;
    let at = |i: usize| {
        number(Some(&pair[i]), what)
            .ok()
            .filter(|n| (1..=limit).contains(n))
    };
    match (at(0), at(1)) {
        (Some(a), Some(b)) if a <= b => Ok((a - 1, b - 1)),
        _ => Err(bad(what)),
    }
}

fn parse_print_area(v: &StrictJsonValue) -> Result<Vec<CellBox>, EasyOfficeError> {
    let items = v.as_array().ok_or_else(|| bad("printArea"))?;
    if items.len() > easyoffice_formats::MAX_PRINT_RANGES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "인쇄 영역 수가 한도를 초과했습니다.".into(),
        ));
    }
    items
        .iter()
        .map(|item| {
            item.as_str()
                .and_then(CellBox::parse)
                .ok_or_else(|| bad("printArea"))
        })
        .collect()
}

/// Reads the `page` object of a sheet into `styling`.
pub(crate) fn parse_page(obj: Obj<'_>, styling: &mut SheetStyling) -> Result<(), EasyOfficeError> {
    let Some(value) = field(obj, "page") else {
        return Ok(());
    };
    let page_obj = value.as_object().ok_or_else(|| bad("page"))?;
    let mut page = SheetPage {
        landscape: flag(page_obj, "landscape")?,
        center_horizontal: flag(page_obj, "centerH")?,
        center_vertical: flag(page_obj, "centerV")?,
        grid_lines: flag(page_obj, "grid")?,
        headings: flag(page_obj, "headings")?,
        ..SheetPage::default()
    };
    if let Some(v) = field(page_obj, "paper") {
        page.paper = Some(u16::try_from(number(Some(v), "paper")?).map_err(|_| bad("paper"))?);
    }
    if let Some(v) = field(page_obj, "margins") {
        page.margins = Some(parse_margins(v)?);
    }
    if let Some(v) = field(page_obj, "scale") {
        page.scale = parse_scale(v)?;
    }
    if let Some(v) = field(page_obj, "printArea") {
        page.print_area = parse_print_area(v)?;
    }
    if let Some(v) = field(page_obj, "titleRows") {
        page.title_rows = Some(parse_run(v, ROWS, "titleRows")?);
    }
    if let Some(v) = field(page_obj, "titleCols") {
        page.title_cols = Some(parse_run(v, COLS, "titleCols")?);
    }
    if !page.within_bounds(ROWS, COLS) {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "쪽 설정 값(배율·여백·인쇄 영역·반복 행열)이 한도를 초과했습니다.".into(),
        ));
    }
    styling.page = page;
    Ok(())
}

fn page_json(page: &SheetPage) -> String {
    let mut out = String::from("{");
    let mut sep = "";
    let mut put = |out: &mut String, text: String| {
        let _ = write!(out, "{sep}{text}");
        sep = ",";
    };
    for (key, on) in [
        ("landscape", page.landscape),
        ("centerH", page.center_horizontal),
        ("centerV", page.center_vertical),
        ("grid", page.grid_lines),
        ("headings", page.headings),
    ] {
        if on {
            put(&mut out, format!("\"{key}\":true"));
        }
    }
    if let Some(code) = page.paper {
        put(&mut out, format!("\"paper\":{code}"));
    }
    if let Some(m) = page.margins {
        put(
            &mut out,
            format!(
                "\"margins\":{{\"left\":{},\"right\":{},\"top\":{},\"bottom\":{},\"header\":{},\"footer\":{}}}",
                m.left, m.right, m.top, m.bottom, m.header, m.footer
            ),
        );
    }
    match page.scale {
        PageScale::Percent(100) => {}
        PageScale::Percent(p) => put(&mut out, format!("\"scale\":{{\"percent\":{p}}}")),
        PageScale::Fit { width, height } => {
            put(
                &mut out,
                format!("\"scale\":{{\"fitWidth\":{width},\"fitHeight\":{height}}}"),
            );
        }
    }
    if !page.print_area.is_empty() {
        let items: Vec<String> = page
            .print_area
            .iter()
            .map(|b| format!("\"{}\"", b.to_a1()))
            .collect();
        put(&mut out, format!("\"printArea\":[{}]", items.join(",")));
    }
    if let Some((a, b)) = page.title_rows {
        put(&mut out, format!("\"titleRows\":[{},{}]", a + 1, b + 1));
    }
    if let Some((a, b)) = page.title_cols {
        put(&mut out, format!("\"titleCols\":[{},{}]", a + 1, b + 1));
    }
    out.push('}');
    out
}

/// `,"page":{..}` when the setup is not the default.
pub(crate) fn write_page(out: &mut String, styling: &SheetStyling) {
    if !styling.page.is_default() {
        let _ = write!(out, ",\"page\":{}", page_json(&styling.page));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Result<SheetStyling, EasyOfficeError> {
        let json = StrictJsonValue::parse(text).unwrap();
        let mut styling = SheetStyling::default();
        parse_page(json.as_object().unwrap(), &mut styling).map(|()| styling)
    }

    const FULL: &str = r#"{"page":{"landscape":true,"centerH":true,"centerV":true,"grid":true,"headings":true,"paper":8,"margins":{"left":500,"right":600,"top":800,"bottom":900,"header":250,"footer":350},"scale":{"fitWidth":1,"fitHeight":0},"printArea":["A1:D10","F15"],"titleRows":[1,2],"titleCols":[1,1]}}"#;

    #[test]
    fn a_setup_round_trips_through_json() {
        let styling = read(FULL).unwrap();
        assert!(styling.page.landscape);
        assert_eq!(
            styling.page.scale,
            PageScale::Fit {
                width: 1,
                height: 0
            }
        );
        assert_eq!(styling.page.print_area.len(), 2);
        let mut out = String::new();
        write_page(&mut out, &styling);
        assert_eq!(out, format!(",{}", &FULL[1..FULL.len() - 1]));
        assert_eq!(read(&format!("{{{}}}", &out[1..])).unwrap(), styling);
        let mut none = String::new();
        write_page(&mut none, &SheetStyling::default());
        assert_eq!(none, "");
    }

    #[test]
    fn hostile_setups_are_refused() {
        for text in [
            r#"{"page":[]}"#,
            r#"{"page":{"landscape":1}}"#,
            r#"{"page":{"scale":{"percent":9}}}"#,
            r#"{"page":{"scale":{"percent":401}}}"#,
            r#"{"page":{"scale":{"percent":-5}}}"#,
            r#"{"page":{"scale":{"percent":1.5}}}"#,
            r#"{"page":{"scale":{"fitWidth":1}}}"#,
            r#"{"page":{"scale":{"fitWidth":1001,"fitHeight":1}}}"#,
            r#"{"page":{"scale":{"fitWidth":70000,"fitHeight":1}}}"#,
            r#"{"page":{"paper":0}}"#,
            r#"{"page":{"paper":119}}"#,
            r#"{"page":{"paper":70000}}"#,
            r#"{"page":{"margins":{"left":-1,"right":1,"top":1,"bottom":1,"header":1,"footer":1}}}"#,
            r#"{"page":{"margins":{"left":22001,"right":1,"top":1,"bottom":1,"header":1,"footer":1}}}"#,
            r#"{"page":{"margins":{"left":1}}}"#,
            r#"{"page":{"margins":[1,2,3,4,5,6]}}"#,
            r#"{"page":{"printArea":"A1"}}"#,
            r#"{"page":{"printArea":["A1:"]}}"#,
            r#"{"page":{"printArea":["B2:A1:C3"]}}"#,
            r#"{"page":{"printArea":[7]}}"#,
            r#"{"page":{"titleRows":[3,2]}}"#,
            r#"{"page":{"titleRows":[0,2]}}"#,
            r#"{"page":{"titleRows":[1,1048577]}}"#,
            r#"{"page":{"titleRows":[1,257]}}"#,
            r#"{"page":{"titleRows":[1,5000]}}"#,
            r#"{"page":{"titleCols":[1,65]}}"#,
            r#"{"page":{"titleCols":[1]}}"#,
            r#"{"page":{"titleCols":["A","B"]}}"#,
        ] {
            assert!(read(text).is_err(), "{text}");
        }
        let many = (0..65)
            .map(|i| format!("\"A{}\"", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        assert!(read(&format!("{{\"page\":{{\"printArea\":[{many}]}}}}")).is_err());
        let edge = (0..64)
            .map(|i| format!("\"A{}\"", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        assert!(read(&format!("{{\"page\":{{\"printArea\":[{edge}]}}}}")).is_ok());
        assert!(read(
            r#"{"page":{"titleRows":[1,256],"titleCols":[1,64],"scale":{"percent":400}}}"#
        )
        .is_ok());
    }
}
