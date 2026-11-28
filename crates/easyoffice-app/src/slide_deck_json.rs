//! JSON parser, decoder serializer, and readback serializer for [`RichSlideDeck`] (`PD-257`).

use std::fmt::Write;

use easyoffice_formats::{
    base64_codec, normalize_hex_color, normalize_table_rows, ChartKind, ChartLegendPos,
    EasyOfficeError, ElementFx, RichImage, RichSlide, RichSlideDeck, SlideAspect, SlideBox,
    SlideChartSeries, SlideElement, SlideMasterLayout, SlideOrientation, SlidePlaceholderRole,
    SlideShapeType, SlideTextAlign, SlideTransition, StrictJsonValue, DEFAULT_SLIDE_FONT_PT,
    DEFAULT_SLIDE_STROKE_PT, MAX_ELEMENTS_PER_DECK, MAX_ELEMENTS_PER_SLIDE, MAX_SLIDES_PER_DECK,
    MAX_SLIDE_CHART_POINTS, MAX_SLIDE_CHART_SERIES, MAX_SLIDE_TOTAL_IMAGE_BYTES,
};

use crate::convert_structured::{as_array, as_object, field, invalid, opt_str, Budget, JsonObject};
use crate::slide_ext_json::{parse_element_fx, parse_slide_ext, write_element_fx, write_slide_ext};
use crate::slide_image_table::{image_by_ref, parse_image_table, ImageRefs, ImageTable};

fn push_f64(out: &mut String, v: f64) {
    if v.is_finite() {
        let r = (v * 1000.0).round() / 1000.0;
        let _ = write!(out, "{}", if r == 0.0 { 0.0 } else { r });
    } else {
        out.push('0');
    }
}

fn opt_bool_def(obj: &JsonObject, key: &str, default: bool) -> Result<bool, EasyOfficeError> {
    match field(obj, key) {
        None => Ok(default),
        Some(v) => v
            .as_bool()
            .ok_or_else(|| invalid(format!("슬라이드 '{key}'는 true/false여야 합니다."))),
    }
}

fn as_finite_f64(v: &StrictJsonValue, key: &str) -> Result<f64, EasyOfficeError> {
    let StrictJsonValue::Number(raw) = v else {
        return Err(invalid(format!(
            "슬라이드 '{key}'는 유한한 숫자여야 합니다."
        )));
    };
    raw.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .ok_or_else(|| invalid(format!("슬라이드 '{key}' 숫자가 올바르지 않습니다.")))
}

fn opt_f64(obj: &JsonObject, key: &str) -> Result<Option<f64>, EasyOfficeError> {
    field(obj, key).map(|v| as_finite_f64(v, key)).transpose()
}

fn req_f64(obj: &JsonObject, key: &str) -> Result<f64, EasyOfficeError> {
    let v = field(obj, key)
        .ok_or_else(|| invalid(format!("슬라이드 개체 필수 필드 '{key}'가 누락되었습니다.")))?;
    as_finite_f64(v, key)
}

fn opt_hex(obj: &JsonObject, key: &str) -> Result<Option<String>, EasyOfficeError> {
    let Some(raw) = opt_str(obj, key)? else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    normalize_hex_color(raw).map(Some).ok_or_else(|| {
        invalid(format!(
            "슬라이드 '{key}'는 RRGGBB/#RRGGBB 색이어야 합니다: {raw}"
        ))
    })
}

fn parse_box(obj: &JsonObject) -> Result<SlideBox, EasyOfficeError> {
    let src = if let Some(rv) = field(obj, "rect") {
        as_object(rv, "rect")?
    } else {
        obj
    };
    let rect = SlideBox {
        x: req_f64(src, "x")?,
        y: req_f64(src, "y")?,
        w: req_f64(src, "w")?,
        h: req_f64(src, "h")?,
    };
    rect.validate()?;
    Ok(rect)
}

fn parse_element(
    item: &StrictJsonValue,
    idx: usize,
    budget: &mut Budget,
    total_img_bytes: &mut usize,
    images: &ImageRefs,
) -> Result<SlideElement, EasyOfficeError> {
    budget.take(1)?;
    let obj = as_object(item, "elements")?;
    let kind = opt_str(obj, "kind")?
        .ok_or_else(|| invalid("슬라이드 개체에 'kind' 필드가 필요합니다."))?;
    let id = opt_str(obj, "id")?
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("el-{}", idx + 1));
    let rect = parse_box(obj)?;

    match kind {
        "textBox" | "textbox" => {
            let align = match opt_str(obj, "align")? {
                None => SlideTextAlign::Left,
                Some(s) => SlideTextAlign::parse(s)
                    .ok_or_else(|| invalid(format!("알 수 없는 정렬 값: {s}")))?,
            };
            let role = match opt_str(obj, "role")? {
                None => None,
                Some(s) => Some(
                    SlidePlaceholderRole::parse(s)
                        .ok_or_else(|| invalid(format!("알 수 없는 자리표시자 역할: {s}")))?,
                ),
            };
            Ok(SlideElement::TextBox {
                id,
                rect,
                text: opt_str(obj, "text")?.unwrap_or("").to_string(),
                font_size_pt: opt_f64(obj, "fontSizePt")?.unwrap_or(DEFAULT_SLIDE_FONT_PT),
                bold: opt_bool_def(obj, "bold", false)?,
                italic: opt_bool_def(obj, "italic", false)?,
                underline: opt_bool_def(obj, "underline", false)?,
                align,
                color: opt_hex(obj, "color")?,
                fill: opt_hex(obj, "fill")?,
                role,
            })
        }
        "shape" => {
            let shape_type = match opt_str(obj, "shapeType")? {
                None => SlideShapeType::Rect,
                Some(s) => SlideShapeType::parse(s)
                    .ok_or_else(|| invalid(format!("지원하지 않는 도형 유형: {s}")))?,
            };
            let align = match opt_str(obj, "align")? {
                None => SlideTextAlign::Center,
                Some(s) => SlideTextAlign::parse(s)
                    .ok_or_else(|| invalid(format!("알 수 없는 정렬 값: {s}")))?,
            };
            Ok(SlideElement::Shape {
                id,
                rect,
                shape_type,
                text: opt_str(obj, "text")?.unwrap_or("").to_string(),
                fill: opt_hex(obj, "fill")?,
                stroke: opt_hex(obj, "stroke")?,
                stroke_width_pt: opt_f64(obj, "strokeWidthPt")?.unwrap_or(DEFAULT_SLIDE_STROKE_PT),
                color: opt_hex(obj, "color")?,
                font_size_pt: opt_f64(obj, "fontSizePt")?.unwrap_or(16.0),
                bold: opt_bool_def(obj, "bold", false)?,
                italic: opt_bool_def(obj, "italic", false)?,
                underline: opt_bool_def(obj, "underline", false)?,
                align,
            })
        }
        "image" => {
            let alt = opt_str(obj, "alt")?.unwrap_or("").to_string();
            let image = match image_by_ref(obj, images)? {
                Some(shared) => shared,
                None => {
                    let b64 = match opt_str(obj, "dataBase64")? {
                        Some(s) if !s.is_empty() => s,
                        _ => opt_str(obj, "data")?
                            .filter(|s| !s.is_empty())
                            .ok_or_else(|| {
                                invalid("슬라이드 이미지 개체에 'imageRef', 'dataBase64' 또는 'data'가 필요합니다.")
                            })?,
                    };
                    let bytes = base64_codec::decode(b64)
                        .ok_or_else(|| invalid("슬라이드 이미지 Base64 디코딩에 실패했습니다."))?;
                    *total_img_bytes = total_img_bytes.saturating_add(bytes.len());
                    if *total_img_bytes > MAX_SLIDE_TOTAL_IMAGE_BYTES {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "슬라이드 전체 이미지 크기가 상한(128 MiB)을 초과했습니다.".to_string(),
                        ));
                    }
                    RichImage::new(bytes, &alt)?
                }
            };
            Ok(SlideElement::Image {
                id,
                rect,
                image,
                alt,
            })
        }
        "table" => {
            let raw_rows = field(obj, "rows")
                .ok_or_else(|| invalid("슬라이드 표 개체에 'rows'가 필요합니다."))?;
            let row_vals = as_array(raw_rows, "rows")?;
            let mut rows = Vec::with_capacity(row_vals.len());
            for rv in row_vals {
                let cells = as_array(rv, "rows[]")?;
                let mut row = Vec::with_capacity(cells.len());
                for cv in cells {
                    budget.take(1)?;
                    let cell = cv
                        .as_str()
                        .ok_or_else(|| invalid("슬라이드 표 셀은 문자열이어야 합니다."))?;
                    row.push(cell.to_string());
                }
                rows.push(row);
            }
            Ok(SlideElement::Table {
                id,
                rect,
                rows: normalize_table_rows(rows)?,
                header_row: opt_bool_def(obj, "headerRow", false)?,
                fill: opt_hex(obj, "fill")?,
            })
        }
        "chart" => {
            let ct_raw = opt_str(obj, "chartType")?
                .ok_or_else(|| invalid("슬라이드 차트 개체에 'chartType'이 필요합니다."))?;
            let chart_type = ChartKind::parse(ct_raw)
                .ok_or_else(|| invalid(format!("지원하지 않는 차트 유형: {ct_raw}")))?;
            let legend = match opt_str(obj, "legend")? {
                None => ChartLegendPos::Right,
                Some(s) => ChartLegendPos::parse(s)
                    .ok_or_else(|| invalid(format!("알 수 없는 범례 위치: {s}")))?,
            };
            let mut categories = Vec::new();
            if let Some(cats_v) = field(obj, "categories") {
                let arr = as_array(cats_v, "categories")?;
                if arr.len() > MAX_SLIDE_CHART_POINTS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "슬라이드 차트 항목 수가 상한({MAX_SLIDE_CHART_POINTS})을 초과했습니다."
                    )));
                }
                for cv in arr {
                    budget.take(1)?;
                    let s = cv
                        .as_str()
                        .ok_or_else(|| invalid("차트 categories 항목은 문자열이어야 합니다."))?;
                    categories.push(s.to_string());
                }
            }
            let ser_v = field(obj, "series")
                .ok_or_else(|| invalid("슬라이드 차트 개체에 'series'가 필요합니다."))?;
            let ser_arr = as_array(ser_v, "series")?;
            if ser_arr.is_empty() || ser_arr.len() > MAX_SLIDE_CHART_SERIES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "슬라이드 차트 계열 수({})가 1~{MAX_SLIDE_CHART_SERIES} 범위를 벗어났습니다.",
                    ser_arr.len()
                )));
            }
            let mut series = Vec::with_capacity(ser_arr.len());
            for sv in ser_arr {
                let so = as_object(sv, "series[]")?;
                let name = opt_str(so, "name")?.unwrap_or("").to_string();
                let vals_v = field(so, "values")
                    .ok_or_else(|| invalid("차트 계열에 'values' 배열이 필요합니다."))?;
                let vals_arr = as_array(vals_v, "values")?;
                if vals_arr.len() > MAX_SLIDE_CHART_POINTS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "슬라이드 차트 포인트 수 상한({MAX_SLIDE_CHART_POINTS}) 초과"
                    )));
                }
                let mut values = Vec::with_capacity(vals_arr.len());
                for nv in vals_arr {
                    budget.take(1)?;
                    values.push(as_finite_f64(nv, "series.values[]")?);
                }
                series.push(SlideChartSeries { name, values });
            }
            Ok(SlideElement::Chart {
                id,
                rect,
                chart_type,
                title: opt_str(obj, "title")?.unwrap_or("").to_string(),
                categories,
                series,
                legend,
            })
        }
        other => Err(invalid(format!("알 수 없는 슬라이드 개체 kind: {other}"))),
    }
}

pub(crate) fn parse_slide_deck(
    entries: &JsonObject,
    items: &[StrictJsonValue],
    budget: &mut Budget,
) -> Result<RichSlideDeck, EasyOfficeError> {
    if items.len() > MAX_SLIDES_PER_DECK {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "슬라이드 수가 상한({MAX_SLIDES_PER_DECK})을 초과했습니다."
        )));
    }
    let aspect = match opt_str(entries, "aspect")? {
        None => SlideAspect::Widescreen16x9,
        Some(s) => SlideAspect::parse(s)
            .ok_or_else(|| invalid(format!("지원하지 않는 슬라이드 화면비입니다: {s}")))?,
    };
    let orientation = match opt_str(entries, "orientation")? {
        None => SlideOrientation::Landscape,
        Some(s) => SlideOrientation::parse(s)
            .ok_or_else(|| invalid(format!("지원하지 않는 슬라이드 방향입니다: {s}")))?,
    };
    let title = opt_str(entries, "title")?.unwrap_or("").to_string();
    let mut slides = Vec::with_capacity(items.len());
    let (mut total_elements, mut total_img_bytes) = (0usize, 0usize);
    let mut bg_cache = crate::slide_ext_json::BackgroundCache::new();
    let images = parse_image_table(entries, budget, &mut total_img_bytes)?;

    for item in items {
        budget.take(1)?;
        let obj = as_object(item, "slides")?;
        let mut bullets = Vec::new();
        if let Some(v) = field(obj, "bullets") {
            for b in as_array(v, "bullets")? {
                budget.take(1)?;
                let text = b
                    .as_str()
                    .ok_or_else(|| invalid("구조화 저장: 'bullets' 항목은 문자열이어야 합니다."))?;
                if !text.trim().is_empty() {
                    bullets.push(text.to_string());
                }
            }
        }
        let layout = match opt_str(obj, "layout")? {
            None => SlideMasterLayout::TitleAndContent,
            Some(s) => SlideMasterLayout::parse(s)
                .ok_or_else(|| invalid(format!("지원하지 않는 슬라이드 레이아웃: {s}")))?,
        };
        let transition = match opt_str(obj, "transition")? {
            None => SlideTransition::None,
            Some(s) => SlideTransition::parse(s)
                .ok_or_else(|| invalid(format!("지원하지 않는 슬라이드 전환 효과: {s}")))?,
        };
        let mut elements = Vec::new();
        let mut fxs = Vec::new();
        if let Some(ev) = field(obj, "elements") {
            let arr = as_array(ev, "elements")?;
            if arr.len() > MAX_ELEMENTS_PER_SLIDE {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "슬라이드당 개체 수가 상한({MAX_ELEMENTS_PER_SLIDE})을 초과했습니다."
                )));
            }
            for (ei, e_item) in arr.iter().enumerate() {
                total_elements += 1;
                if total_elements > MAX_ELEMENTS_PER_DECK {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "덱 전체 개체 수가 상한({MAX_ELEMENTS_PER_DECK})을 초과했습니다."
                    )));
                }
                let el = parse_element(e_item, ei, budget, &mut total_img_bytes, &images)?;
                if !matches!(el, SlideElement::Table { .. } | SlideElement::Chart { .. }) {
                    fxs.extend(parse_element_fx(as_object(e_item, "elements")?, el.id())?);
                }
                elements.push(el);
            }
        }
        let ext = parse_slide_ext(
            obj,
            fxs,
            budget,
            &mut total_img_bytes,
            &mut bg_cache,
            &images,
        )?;
        slides.push(RichSlide {
            title: opt_str(obj, "title")?.unwrap_or("").to_string(),
            bullets,
            notes: opt_str(obj, "notes")?.unwrap_or("").to_string(),
            layout,
            transition,
            elements,
            ext,
        });
    }

    let deck = RichSlideDeck {
        title,
        is_slideshow: false,
        aspect,
        orientation,
        slides,
        elements_rejected: 0,
    };
    deck.validate()?;
    Ok(deck)
}

fn push_opt_hex(
    out: &mut String,
    key: &str,
    hex: &Option<String>,
    push_str: fn(&mut String, &str),
) {
    if let Some(h) = hex {
        let _ = write!(out, ",\"{key}\":");
        push_str(out, h);
    }
}

fn write_element_json(
    out: &mut String,
    el: &SlideElement,
    fx: Option<&ElementFx>,
    table: &ImageTable,
    push_str: fn(&mut String, &str),
) {
    let r = el.rect();
    out.push('{');
    match el {
        SlideElement::TextBox {
            id,
            text,
            font_size_pt,
            bold,
            italic,
            underline,
            align,
            color,
            fill,
            role,
            ..
        } => {
            out.push_str("\"kind\":\"textBox\",\"id\":");
            push_str(out, id);
            out.push_str(",\"x\":");
            push_f64(out, r.x);
            out.push_str(",\"y\":");
            push_f64(out, r.y);
            out.push_str(",\"w\":");
            push_f64(out, r.w);
            out.push_str(",\"h\":");
            push_f64(out, r.h);
            out.push_str(",\"text\":");
            push_str(out, text);
            out.push_str(",\"fontSizePt\":");
            push_f64(out, *font_size_pt);
            let _ = write!(
                out,
                ",\"bold\":{bold},\"italic\":{italic},\"underline\":{underline},\"align\":\"{}\"",
                align.as_str()
            );
            push_opt_hex(out, "color", color, push_str);
            push_opt_hex(out, "fill", fill, push_str);
            if let Some(rl) = role {
                let _ = write!(out, ",\"role\":\"{}\"", rl.as_str());
            }
        }
        SlideElement::Shape {
            id,
            shape_type,
            text,
            fill,
            stroke,
            stroke_width_pt,
            color,
            font_size_pt,
            bold,
            italic,
            underline,
            align,
            ..
        } => {
            out.push_str("\"kind\":\"shape\",\"id\":");
            push_str(out, id);
            out.push_str(",\"x\":");
            push_f64(out, r.x);
            out.push_str(",\"y\":");
            push_f64(out, r.y);
            out.push_str(",\"w\":");
            push_f64(out, r.w);
            out.push_str(",\"h\":");
            push_f64(out, r.h);
            let _ = write!(out, ",\"shapeType\":\"{}\",\"text\":", shape_type.as_str());
            push_str(out, text);
            // A shape with no fill reads back as an empty color, not as the editor's default blue.
            if fill.is_none() {
                out.push_str(",\"fill\":\"\"");
            }
            push_opt_hex(out, "fill", fill, push_str);
            if stroke.is_none() {
                out.push_str(",\"stroke\":\"\"");
            }
            push_opt_hex(out, "stroke", stroke, push_str);
            out.push_str(",\"strokeWidthPt\":");
            push_f64(out, *stroke_width_pt);
            push_opt_hex(out, "color", color, push_str);
            out.push_str(",\"fontSizePt\":");
            push_f64(out, *font_size_pt);
            let _ = write!(
                out,
                ",\"bold\":{bold},\"italic\":{italic},\"underline\":{underline},\"align\":\"{}\"",
                align.as_str()
            );
        }
        SlideElement::Image { id, image, alt, .. } => {
            out.push_str("\"kind\":\"image\",\"id\":");
            push_str(out, id);
            out.push_str(",\"x\":");
            push_f64(out, r.x);
            out.push_str(",\"y\":");
            push_f64(out, r.y);
            out.push_str(",\"w\":");
            push_f64(out, r.w);
            out.push_str(",\"h\":");
            push_f64(out, r.h);
            out.push_str(",\"imageRef\":");
            push_str(out, &table.id_of(image));
            out.push_str(",\"alt\":");
            push_str(out, alt);
        }
        SlideElement::Table {
            id,
            rows,
            header_row,
            fill,
            ..
        } => {
            out.push_str("\"kind\":\"table\",\"id\":");
            push_str(out, id);
            out.push_str(",\"x\":");
            push_f64(out, r.x);
            out.push_str(",\"y\":");
            push_f64(out, r.y);
            out.push_str(",\"w\":");
            push_f64(out, r.w);
            out.push_str(",\"h\":");
            push_f64(out, r.h);
            let _ = write!(out, ",\"headerRow\":{header_row}");
            push_opt_hex(out, "fill", fill, push_str);
            out.push_str(",\"rows\":[");
            for (ri, row) in rows.iter().enumerate() {
                if ri > 0 {
                    out.push(',');
                }
                out.push('[');
                for (ci, cell) in row.iter().enumerate() {
                    if ci > 0 {
                        out.push(',');
                    }
                    push_str(out, cell);
                }
                out.push(']');
            }
            out.push(']');
        }
        SlideElement::Chart {
            id,
            chart_type,
            title,
            categories,
            series,
            legend,
            ..
        } => {
            out.push_str("\"kind\":\"chart\",\"id\":");
            push_str(out, id);
            out.push_str(",\"x\":");
            push_f64(out, r.x);
            out.push_str(",\"y\":");
            push_f64(out, r.y);
            out.push_str(",\"w\":");
            push_f64(out, r.w);
            out.push_str(",\"h\":");
            push_f64(out, r.h);
            let _ = write!(out, ",\"chartType\":\"{}\",\"title\":", chart_type.as_str());
            push_str(out, title);
            out.push_str(",\"categories\":[");
            for (ci, cat) in categories.iter().enumerate() {
                if ci > 0 {
                    out.push(',');
                }
                push_str(out, cat);
            }
            out.push_str("],\"series\":[");
            for (si, ser) in series.iter().enumerate() {
                if si > 0 {
                    out.push(',');
                }
                out.push_str("{\"name\":");
                push_str(out, &ser.name);
                out.push_str(",\"values\":[");
                for (vi, &val) in ser.values.iter().enumerate() {
                    if vi > 0 {
                        out.push(',');
                    }
                    push_f64(out, val);
                }
                out.push_str("]}");
            }
            let _ = write!(out, "],\"legend\":\"{}\"", legend.as_str());
        }
    }
    write_element_fx(out, fx, push_str);
    out.push('}');
}

pub(crate) fn write_slide_deck_decode_fields(
    out: &mut String,
    deck: &RichSlideDeck,
    push_str: fn(&mut String, &str),
) {
    let table = ImageTable::from_deck(deck);
    table.write(out);
    let _ = write!(
        out,
        "\"aspect\":\"{}\",\"orientation\":\"{}\",\"slides\":[",
        deck.aspect.as_str(),
        deck.orientation.as_str()
    );
    for (idx, s) in deck.slides.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str("{\"title\":");
        push_str(out, &s.title);
        out.push_str(",\"bullets\":[");
        for (bi, b) in s.bullets.iter().enumerate() {
            if bi > 0 {
                out.push(',');
            }
            push_str(out, b);
        }
        out.push_str("],\"notes\":");
        push_str(out, &s.notes);
        let _ = write!(
            out,
            ",\"layout\":\"{}\",\"transition\":\"{}\"",
            s.layout.as_str(),
            s.transition.as_str()
        );
        if !s.elements.is_empty() {
            out.push_str(",\"elements\":[");
            for (ei, el) in s.elements.iter().enumerate() {
                if ei > 0 {
                    out.push(',');
                }
                write_element_json(out, el, s.ext.fx_of(el.id()), &table, push_str);
            }
            out.push(']');
        }
        write_slide_ext(out, &s.ext, &table, push_str);
        out.push('}');
    }
}

pub(crate) fn write_slide_deck_readback(
    out: &mut String,
    deck: &RichSlideDeck,
    max_items: usize,
    push_str: fn(&mut String, &str),
    write_list: fn(&mut String, &[usize]),
) {
    let (mut untitled, mut empty, mut dense) = (Vec::new(), Vec::new(), Vec::new());
    let mut missing_alt = 0usize;

    for (idx, s) in deck.slides.iter().enumerate() {
        let no = idx + 1;
        let has_role_title = s.elements.iter().any(|e| {
            matches!(e, SlideElement::TextBox { role: Some(SlidePlaceholderRole::Title), text, .. } if !text.trim().is_empty())
        });
        let t_empty = s.title.trim().is_empty() && !has_role_title;
        let non_empty_bullets = s.bullets.iter().filter(|b| !b.trim().is_empty()).count();
        let body_chars: usize = s.bullets.iter().map(|b| b.chars().count()).sum();
        if t_empty {
            untitled.push(no);
        }
        if t_empty && non_empty_bullets == 0 && s.elements.is_empty() {
            empty.push(no);
        }
        if non_empty_bullets > 8 || body_chars > 400 {
            dense.push(no);
        }
        for el in &s.elements {
            if let SlideElement::Image { alt, image, .. } = el {
                if alt.trim().is_empty() && image.alt().trim().is_empty() {
                    missing_alt += 1;
                }
            }
        }
    }

    let truncated = deck.slides.len() > max_items;
    let kept = deck.slides.len().min(max_items);
    let _ = write!(
        out,
        "\"aspect\":\"{}\",\"orientation\":\"{}\",\"slideCount\":{},\"slidesTruncated\":{truncated},\"slides\":[",
        deck.aspect.as_str(),
        deck.orientation.as_str(),
        deck.slides.len()
    );
    for (idx, s) in deck.slides.iter().take(kept).enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let body_chars: usize =
            s.title.chars().count() + s.bullets.iter().map(|b| b.chars().count()).sum::<usize>();
        let has_notes = !s.notes.trim().is_empty();
        let _ = write!(out, "{{\"index\":{idx},\"number\":{},\"title\":", idx + 1);
        push_str(out, &s.title);
        let _ = write!(out, ",\"bulletCount\":{},\"bullets\":[", s.bullets.len());
        for (bi, b) in s.bullets.iter().enumerate() {
            if bi > 0 {
                out.push(',');
            }
            push_str(out, b);
        }
        let _ = write!(
            out,
            "],\"hasNotes\":{has_notes},\"charCount\":{body_chars},\"notes\":"
        );
        push_str(out, &s.notes);
        let _ = write!(
            out,
            ",\"layout\":\"{}\",\"transition\":\"{}\",\"elementCount\":{},\"elements\":[",
            s.layout.as_str(),
            s.transition.as_str(),
            s.elements.len()
        );
        for (ei, el) in s.elements.iter().enumerate() {
            if ei > 0 {
                out.push(',');
            }
            let kind = match el {
                SlideElement::TextBox { .. } => "textBox",
                SlideElement::Shape { .. } => "shape",
                SlideElement::Image { .. } => "image",
                SlideElement::Table { .. } => "table",
                SlideElement::Chart { .. } => "chart",
            };
            let r = el.rect();
            let _ = write!(out, "{{\"kind\":\"{kind}\",\"id\":");
            push_str(out, el.id());
            out.push_str(",\"x\":");
            push_f64(out, r.x);
            out.push_str(",\"y\":");
            push_f64(out, r.y);
            out.push_str(",\"w\":");
            push_f64(out, r.w);
            out.push_str(",\"h\":");
            push_f64(out, r.h);
            out.push('}');
        }
        out.push_str("]}");
    }
    out.push_str("],\"checks\":{\"untitledSlides\":[");
    write_list(out, &untitled);
    out.push_str("],\"emptySlides\":[");
    write_list(out, &empty);
    out.push_str("],\"denseSlides\":[");
    write_list(out, &dense);
    let _ = write!(out, "],\"missingAltImageCount\":{missing_alt}}},");
    crate::layout_lint_json::write_layout_lint_property(
        out,
        &easyoffice_formats::lint_slide_deck(deck),
    );
}
