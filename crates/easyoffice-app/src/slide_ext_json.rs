//! JSON form of the slide extensions (`PD-394`): per-element effects (`rot`, `flipH`, `flipV`, `dash`,
//! `outline`, `shadow`, `para`) and per-slide state (`hidden`, `background`, `footer`, `groups`,
//! `anims`). Parsing is Fail-Closed: unknown names, out-of-range numbers and oversized lists are errors.

use std::fmt::Write;

use easyoffice_formats::{
    base64_codec, check_guides, check_spans, clean_face, normalize_hex_color, AnimClass, AnimDir,
    AnimEffect, AnimStart, ColorScheme, DeckSettings, EasyOfficeError, ElementFx, RichImage,
    RichImageCrop, SlideAnim, SlideBackground, SlideBullet, SlideDash, SlideDate, SlideExt,
    SlideFooter, SlideGroup, SlideGuide, SlideNumberStyle, SlideOutline, SlideParaFmt, SlideShadow,
    StrictJsonValue, TableSpan, COLOR_SCHEME_SLOTS, DEFAULT_ANIM_MS, GUIDE_UNITS_PER_PT,
    MAX_ADVANCE_MS, MAX_CROP_SIDE, MAX_GUIDE_POS, MAX_SLIDE_ANIMS, MAX_SLIDE_GROUPS,
    MAX_SLIDE_TOTAL_IMAGE_BYTES,
};

use crate::convert_structured::{as_array, as_object, field, invalid, opt_str, Budget, JsonObject};
use crate::slide_image_table::{image_by_ref, ImageRefs, ImageTable};

fn num(obj: &JsonObject, key: &str) -> Result<Option<f64>, EasyOfficeError> {
    let Some(v) = field(obj, key) else {
        return Ok(None);
    };
    let StrictJsonValue::Number(raw) = v else {
        return Err(invalid(format!("슬라이드 '{key}'는 숫자여야 합니다.")));
    };
    raw.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .map(Some)
        .ok_or_else(|| invalid(format!("슬라이드 '{key}' 숫자가 올바르지 않습니다.")))
}

fn flag(obj: &JsonObject, key: &str) -> Result<bool, EasyOfficeError> {
    match field(obj, key) {
        None => Ok(false),
        Some(v) => v
            .as_bool()
            .ok_or_else(|| invalid(format!("슬라이드 '{key}'는 true/false여야 합니다."))),
    }
}

fn hex(obj: &JsonObject, key: &str) -> Result<String, EasyOfficeError> {
    let raw =
        opt_str(obj, key)?.ok_or_else(|| invalid(format!("슬라이드 '{key}' 색이 필요합니다.")))?;
    normalize_hex_color(raw)
        .ok_or_else(|| invalid(format!("슬라이드 '{key}'는 RRGGBB 색이어야 합니다: {raw}")))
}

/// A typeface name: an empty one is none, one that [`clean_face`] would change is refused.
fn face(obj: &JsonObject, key: &str) -> Result<Option<String>, EasyOfficeError> {
    match opt_str(obj, key)? {
        None | Some("") => Ok(None),
        Some(raw) if clean_face(raw).as_deref() == Some(raw) => Ok(Some(raw.to_string())),
        Some(_) => Err(invalid(format!(
            "슬라이드 '{key}' 글꼴 이름이 올바르지 않습니다."
        ))),
    }
}

fn named<T>(
    obj: &JsonObject,
    key: &str,
    parse: fn(&str) -> Option<T>,
) -> Result<Option<T>, EasyOfficeError> {
    opt_str(obj, key)?
        .map(|s| parse(s).ok_or_else(|| invalid(format!("지원하지 않는 슬라이드 '{key}' 값: {s}"))))
        .transpose()
}

fn parse_bullet(v: &StrictJsonValue) -> Result<SlideBullet, EasyOfficeError> {
    let o = as_object(v, "bullet")?;
    match opt_str(o, "kind")? {
        None | Some("none") => Ok(SlideBullet::None),
        Some("char") => {
            let mut chars = opt_str(o, "char")?.unwrap_or("").chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Ok(SlideBullet::Char(c)),
                _ => Err(invalid("글머리 기호 'char'는 한 글자여야 합니다.")),
            }
        }
        Some("number") => Ok(SlideBullet::Number(
            named(o, "style", SlideNumberStyle::parse)?.unwrap_or(SlideNumberStyle::ArabicPeriod),
        )),
        Some(other) => Err(invalid(format!("지원하지 않는 글머리 기호 종류: {other}"))),
    }
}

fn parse_para(v: &StrictJsonValue) -> Result<SlideParaFmt, EasyOfficeError> {
    let o = as_object(v, "para")?;
    let pct = num(o, "lineSpacingPct")?;
    if pct.is_some_and(|p| p.fract() != 0.0 || !(0.0..=f64::from(u16::MAX)).contains(&p)) {
        return Err(invalid("줄 간격은 정수 %여야 합니다."));
    }
    Ok(SlideParaFmt {
        justify: flag(o, "justify")?,
        bullet: field(o, "bullet").map_or(Ok(SlideBullet::None), parse_bullet)?,
        line_spacing_pct: pct.map(|p| p as u16),
        space_before_pt: num(o, "spaceBeforePt")?,
        space_after_pt: num(o, "spaceAfterPt")?,
    })
}

/// Effects found on one element object; `None` when it carries none.
pub(crate) fn parse_element_fx(
    obj: &JsonObject,
    id: &str,
) -> Result<Option<ElementFx>, EasyOfficeError> {
    let mut fx = ElementFx::new(id);
    fx.rotation_deg = num(obj, "rot")?.unwrap_or(0.0).rem_euclid(360.0);
    fx.flip_h = flag(obj, "flipH")?;
    fx.flip_v = flag(obj, "flipV")?;
    fx.dash = named(obj, "dash", SlideDash::parse)?.unwrap_or_default();
    if let Some(v) = field(obj, "outline") {
        let o = as_object(v, "outline")?;
        fx.outline = Some(SlideOutline {
            color: hex(o, "color")?,
            width_pt: num(o, "widthPt")?.unwrap_or(0.75),
        });
    }
    if let Some(v) = field(obj, "shadow") {
        let o = as_object(v, "shadow")?;
        let alpha = num(o, "alphaPct")?.unwrap_or(40.0);
        if !(0.0..=100.0).contains(&alpha) {
            return Err(invalid("그림자 투명도는 0~100%여야 합니다."));
        }
        fx.shadow = Some(SlideShadow {
            color: hex(o, "color")?,
            alpha_pct: alpha.round() as u8,
            blur_pt: num(o, "blurPt")?.unwrap_or(4.0),
            dist_pt: num(o, "distPt")?.unwrap_or(3.0),
            angle_deg: num(o, "angleDeg")?.unwrap_or(45.0).rem_euclid(360.0),
        });
    }
    fx.para = field(obj, "para")
        .map(parse_para)
        .transpose()?
        .filter(|p| !p.is_default());
    fx.font = face(obj, "font")?;
    fx.east_font = face(obj, "eastFont")?;
    fx.crop = field(obj, "crop")
        .map(parse_crop)
        .transpose()?
        .filter(|c| !c.is_none());
    Ok((!fx.is_default()).then_some(fx))
}

/// `crop` of a picture: the part of each side cut away, whole thousandths of a percent; a side past 99%, or sides that leave
/// less than 1% of the picture, refuse the save (Fail-Closed).
fn parse_crop(v: &StrictJsonValue) -> Result<RichImageCrop, EasyOfficeError> {
    let o = as_object(v, "crop")?;
    let side = |key: &str| -> Result<u32, EasyOfficeError> {
        let n = num(o, key)?.unwrap_or(0.0);
        if n.fract() != 0.0 || !(0.0..=f64::from(MAX_CROP_SIDE)).contains(&n) {
            return Err(invalid(format!(
                "그림 자르기 '{key}'는 0~{MAX_CROP_SIDE}의 정수여야 합니다."
            )));
        }
        Ok(n as u32)
    };
    let crop = RichImageCrop {
        left: side("left")?,
        top: side("top")?,
        right: side("right")?,
        bottom: side("bottom")?,
    };
    if crop.is_valid() {
        Ok(crop)
    } else {
        Err(invalid("그림 자르기가 그림을 1% 미만으로 남깁니다."))
    }
}

/// Merged cells (`merges`) of a table object, PD-402: `None` when it has none. A span outside the `rows` x `cols`
/// grid, over another span or over the table limits refuses the save (Fail-Closed).
pub(crate) fn parse_table_fx(
    obj: &JsonObject,
    id: &str,
    rows: usize,
    cols: usize,
) -> Result<Option<ElementFx>, EasyOfficeError> {
    let Some(list) = field(obj, "merges") else {
        return Ok(None);
    };
    let int = |o: &JsonObject, key: &str| -> Result<u16, EasyOfficeError> {
        let v = num(o, key)?.ok_or_else(|| invalid(format!("표 병합에 '{key}'가 필요합니다.")))?;
        if v.fract() != 0.0 || !(0.0..=f64::from(u16::MAX)).contains(&v) {
            return Err(invalid(format!(
                "표 병합 '{key}'는 0 이상의 정수여야 합니다."
            )));
        }
        Ok(v as u16)
    };
    let mut spans = Vec::new();
    for item in as_array(list, "merges")? {
        let o = as_object(item, "merges")?;
        spans.push(TableSpan {
            row: int(o, "row")?,
            col: int(o, "col")?,
            row_span: int(o, "rowSpan")?,
            col_span: int(o, "colSpan")?,
        });
    }
    check_spans(&spans, rows, cols)?;
    let mut fx = ElementFx::new(id);
    fx.spans = spans;
    Ok((!fx.is_default()).then_some(fx))
}

/// Background pictures decoded so far by their base64 text.
pub(crate) type BackgroundCache = std::collections::HashMap<String, RichImage>;

fn parse_background(
    v: &StrictJsonValue,
    total_img_bytes: &mut usize,
    cache: &mut BackgroundCache,
    images: &ImageRefs,
) -> Result<SlideBackground, EasyOfficeError> {
    let o = as_object(v, "background")?;
    match opt_str(o, "kind")? {
        Some("solid") => Ok(SlideBackground::Solid(hex(o, "color")?)),
        Some("gradient") => {
            let angle = num(o, "angleDeg")?
                .unwrap_or(90.0)
                .rem_euclid(360.0)
                .round() as u16
                % 360;
            Ok(SlideBackground::Gradient {
                from: hex(o, "from")?,
                to: hex(o, "to")?,
                angle_deg: angle,
            })
        }
        Some("picture") => {
            if let Some(shared) = image_by_ref(o, images)? {
                return Ok(SlideBackground::Picture(shared));
            }
            let b64 = opt_str(o, "dataBase64")?
                .filter(|s| !s.is_empty())
                .or(opt_str(o, "data")?.filter(|s| !s.is_empty()))
                .ok_or_else(|| invalid("슬라이드 배경 그림에 'data'가 필요합니다."))?;
            // Slides that carry the same picture share one decoded buffer and count once.
            if let Some(img) = cache.get(b64) {
                return Ok(SlideBackground::Picture(img.clone()));
            }
            let bytes = base64_codec::decode(b64)
                .ok_or_else(|| invalid("슬라이드 배경 그림 Base64 디코딩에 실패했습니다."))?;
            *total_img_bytes = total_img_bytes.saturating_add(bytes.len());
            if *total_img_bytes > MAX_SLIDE_TOTAL_IMAGE_BYTES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "슬라이드 전체 이미지 크기가 상한(128 MiB)을 초과했습니다.".to_string(),
                ));
            }
            let img = RichImage::new(bytes, "")?;
            cache.insert(b64.to_string(), img.clone());
            Ok(SlideBackground::Picture(img))
        }
        other => Err(invalid(format!(
            "지원하지 않는 슬라이드 배경 종류: {}",
            other.unwrap_or("(없음)")
        ))),
    }
}

fn parse_footer(v: &StrictJsonValue) -> Result<SlideFooter, EasyOfficeError> {
    let o = as_object(v, "footer")?;
    let date = match field(o, "date") {
        None => SlideDate::None,
        Some(d) => {
            let d = as_object(d, "footer.date")?;
            match opt_str(d, "kind")? {
                Some("auto") => SlideDate::Auto,
                Some("fixed") => SlideDate::Fixed(opt_str(d, "text")?.unwrap_or("").to_string()),
                other => {
                    return Err(invalid(format!(
                        "지원하지 않는 날짜 종류: {}",
                        other.unwrap_or("(없음)")
                    )))
                }
            }
        }
    };
    Ok(SlideFooter {
        number: flag(o, "number")?,
        date,
        text: opt_str(o, "text")?.map(str::to_string),
    })
}

fn parse_groups(
    v: &StrictJsonValue,
    budget: &mut Budget,
) -> Result<Vec<SlideGroup>, EasyOfficeError> {
    let arr = as_array(v, "groups")?;
    if arr.len() > MAX_SLIDE_GROUPS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "슬라이드 그룹 수가 상한({MAX_SLIDE_GROUPS})을 초과했습니다."
        )));
    }
    arr.iter()
        .map(|g| {
            let o = as_object(g, "groups[]")?;
            let members = as_array(
                field(o, "members").ok_or_else(|| invalid("그룹에 'members'가 필요합니다."))?,
                "members",
            )?;
            budget.take(members.len())?;
            Ok(SlideGroup {
                id: opt_str(o, "id")?
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| invalid("그룹에 'id'가 필요합니다."))?
                    .to_string(),
                members: members
                    .iter()
                    .map(|m| {
                        m.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| invalid("그룹 구성원은 문자열이어야 합니다."))
                    })
                    .collect::<Result<_, _>>()?,
            })
        })
        .collect()
}

fn parse_anims(
    v: &StrictJsonValue,
    budget: &mut Budget,
) -> Result<Vec<SlideAnim>, EasyOfficeError> {
    let arr = as_array(v, "anims")?;
    if arr.len() > MAX_SLIDE_ANIMS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "슬라이드 애니메이션 수가 상한({MAX_SLIDE_ANIMS})을 초과했습니다."
        )));
    }
    budget.take(arr.len())?;
    let ms = |o: &JsonObject, key: &str, default: u32| -> Result<u32, EasyOfficeError> {
        match num(o, key)? {
            None => Ok(default),
            Some(v) if v.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&v) => Ok(v as u32),
            Some(_) => Err(invalid(format!(
                "애니메이션 '{key}'는 0 이상의 정수 ms여야 합니다."
            ))),
        }
    };
    arr.iter()
        .map(|a| {
            let o = as_object(a, "anims[]")?;
            Ok(SlideAnim {
                target: opt_str(o, "target")?
                    .ok_or_else(|| invalid("애니메이션에 'target'이 필요합니다."))?
                    .to_string(),
                class: named(o, "class", AnimClass::parse)?
                    .ok_or_else(|| invalid("애니메이션에 'class'가 필요합니다."))?,
                effect: named(o, "effect", AnimEffect::parse)?
                    .ok_or_else(|| invalid("애니메이션에 'effect'가 필요합니다."))?,
                start: named(o, "start", AnimStart::parse)?.unwrap_or(AnimStart::OnClick),
                duration_ms: ms(o, "durationMs", DEFAULT_ANIM_MS)?,
                delay_ms: ms(o, "delayMs", 0)?,
                direction: named(o, "direction", AnimDir::parse)?.unwrap_or_default(),
            })
        })
        .collect()
}

/// Slide-level extensions of one slide object plus the element effects gathered while its elements
/// were parsed.
pub(crate) fn parse_slide_ext(
    obj: &JsonObject,
    fx: Vec<ElementFx>,
    budget: &mut Budget,
    total_img_bytes: &mut usize,
    cache: &mut BackgroundCache,
    images: &ImageRefs,
) -> Result<SlideExt, EasyOfficeError> {
    Ok(SlideExt {
        hidden: flag(obj, "hidden")?,
        background: field(obj, "background")
            .map(|b| parse_background(b, total_img_bytes, cache, images))
            .transpose()?,
        footer: field(obj, "footer")
            .map(parse_footer)
            .transpose()?
            .unwrap_or_default(),
        fx,
        groups: field(obj, "groups")
            .map(|g| parse_groups(g, budget))
            .transpose()?
            .unwrap_or_default(),
        anims: field(obj, "anims")
            .map(|a| parse_anims(a, budget))
            .transpose()?
            .unwrap_or_default(),
        transition_dir: named(obj, "transitionDir", AnimDir::parse)?,
        advance_after_ms: match num(obj, "advanceAfterMs")? {
            None => None,
            Some(v) if v.fract() == 0.0 && (0.0..=f64::from(MAX_ADVANCE_MS)).contains(&v) => {
                Some(v as u32)
            }
            Some(_) => {
                return Err(invalid(
                    "슬라이드 자동 넘김 시간은 0~24시간(ms)이어야 합니다.",
                ))
            }
        },
        no_click_advance: flag(obj, "noClickAdvance")?,
    })
}

/// Deck-wide settings (`showLoop`, `colorScheme`) of the top-level object.
pub(crate) fn parse_deck_settings(obj: &JsonObject) -> Result<DeckSettings, EasyOfficeError> {
    let color_scheme = field(obj, "colorScheme")
        .map(|v| {
            let o = as_object(v, "colorScheme")?;
            let colors: Vec<String> = as_array(
                field(o, "colors").ok_or_else(|| invalid("색 구성표에 'colors'가 필요합니다."))?,
                "colors",
            )?
            .iter()
            .map(|c| {
                c.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| invalid("색 구성표의 색은 문자열이어야 합니다."))
            })
            .collect::<Result<_, _>>()?;
            if colors.len() != COLOR_SCHEME_SLOTS {
                return Err(invalid(format!(
                    "색 구성표는 색 {COLOR_SCHEME_SLOTS}개여야 합니다."
                )));
            }
            ColorScheme::checked(opt_str(o, "name")?.unwrap_or(""), &colors)
                .ok_or_else(|| invalid("색 구성표의 색은 RRGGBB여야 합니다."))
        })
        .transpose()?;
    let guides = field(obj, "guides")
        .map(|v| {
            as_array(v, "guides")?
                .iter()
                .map(|item| {
                    let o = as_object(item, "guides")?;
                    let vertical = match opt_str(o, "orient")? {
                        Some("vert") => true,
                        Some("horz") => false,
                        _ => return Err(invalid("안내선 'orient'는 vert 또는 horz여야 합니다.")),
                    };
                    let pt = num(o, "posPt")?
                        .ok_or_else(|| invalid("안내선에 'posPt'가 필요합니다."))?;
                    let pos = (pt * f64::from(GUIDE_UNITS_PER_PT)).round();
                    if pos.abs() > f64::from(MAX_GUIDE_POS) {
                        return Err(invalid("안내선 위치가 슬라이드 좌표 범위를 벗어났습니다."));
                    }
                    Ok(SlideGuide {
                        vertical,
                        pos: pos as i32,
                    })
                })
                .collect::<Result<Vec<_>, EasyOfficeError>>()
        })
        .transpose()?
        .unwrap_or_default();
    check_guides(&guides)?;
    Ok(DeckSettings {
        show_loop: flag(obj, "showLoop")?,
        manual_advance: flag(obj, "manualAdvance")?,
        color_scheme,
        guides,
        show_guides: flag(obj, "showGuides")?,
    })
}

/// `"showLoop":…,"colorScheme":…` fields, each only when the deck sets it; a leading comma comes with each.
pub(crate) fn write_deck_settings(
    out: &mut String,
    settings: &DeckSettings,
    push_str: fn(&mut String, &str),
) {
    if settings.show_loop {
        out.push_str(",\"showLoop\":true");
    }
    if settings.manual_advance {
        out.push_str(",\"manualAdvance\":true");
    }
    if settings.show_guides {
        out.push_str(",\"showGuides\":true");
    }
    if !settings.guides.is_empty() {
        out.push_str(",\"guides\":[");
        for (i, g) in settings.guides.iter().enumerate() {
            let _ = write!(
                out,
                "{}{{\"orient\":\"{}\",\"posPt\":",
                if i > 0 { "," } else { "" },
                if g.vertical { "vert" } else { "horz" }
            );
            push_f64(out, f64::from(g.pos) / f64::from(GUIDE_UNITS_PER_PT));
            out.push('}');
        }
        out.push(']');
    }
    if let Some(s) = &settings.color_scheme {
        out.push_str(",\"colorScheme\":{\"name\":");
        push_str(out, &s.name);
        out.push_str(",\"colors\":[");
        for (i, c) in s.colors.iter().enumerate() {
            let _ = write!(out, "{}\"{c}\"", if i > 0 { "," } else { "" });
        }
        out.push_str("]}");
    }
}

fn push_f64(out: &mut String, v: f64) {
    let r = (v * 1000.0).round() / 1000.0;
    let _ = write!(out, "{}", if r == 0.0 { 0.0 } else { r });
}

/// Fields appended to an element object: nothing for an element without effects.
pub(crate) fn write_element_fx(
    out: &mut String,
    fx: Option<&ElementFx>,
    push_str: fn(&mut String, &str),
) {
    let Some(fx) = fx else { return };
    if fx.rotation_deg != 0.0 {
        out.push_str(",\"rot\":");
        push_f64(out, fx.rotation_deg);
    }
    if fx.flip_h {
        out.push_str(",\"flipH\":true");
    }
    if fx.flip_v {
        out.push_str(",\"flipV\":true");
    }
    if fx.dash != SlideDash::Solid {
        let _ = write!(out, ",\"dash\":\"{}\"", fx.dash.as_str());
    }
    if let Some(o) = &fx.outline {
        let _ = write!(out, ",\"outline\":{{\"color\":\"{}\",\"widthPt\":", o.color);
        push_f64(out, o.width_pt);
        out.push('}');
    }
    if let Some(s) = &fx.shadow {
        let _ = write!(
            out,
            ",\"shadow\":{{\"color\":\"{}\",\"alphaPct\":{},\"blurPt\":",
            s.color, s.alpha_pct
        );
        push_f64(out, s.blur_pt);
        out.push_str(",\"distPt\":");
        push_f64(out, s.dist_pt);
        out.push_str(",\"angleDeg\":");
        push_f64(out, s.angle_deg);
        out.push('}');
    }
    if let Some(p) = &fx.para {
        write_para(out, p, push_str);
    }
    for (key, face) in [("font", &fx.font), ("eastFont", &fx.east_font)] {
        if let Some(face) = face {
            let _ = write!(out, ",\"{key}\":");
            push_str(out, face);
        }
    }
    if let Some(c) = fx.crop {
        let _ = write!(
            out,
            ",\"crop\":{{\"left\":{},\"top\":{},\"right\":{},\"bottom\":{}}}",
            c.left, c.top, c.right, c.bottom
        );
    }
    if !fx.spans.is_empty() {
        out.push_str(",\"merges\":[");
        for (i, s) in fx.spans.iter().enumerate() {
            let sep = if i > 0 { "," } else { "" };
            let _ = write!(
                out,
                "{sep}{{\"row\":{},\"col\":{},\"rowSpan\":{},\"colSpan\":{}}}",
                s.row, s.col, s.row_span, s.col_span
            );
        }
        out.push(']');
    }
}

fn write_para(out: &mut String, p: &SlideParaFmt, push_str: fn(&mut String, &str)) {
    let _ = write!(out, ",\"para\":{{\"justify\":{}", p.justify);
    match p.bullet {
        SlideBullet::None => {}
        SlideBullet::Char(c) => {
            out.push_str(",\"bullet\":{\"kind\":\"char\",\"char\":");
            push_str(out, &c.to_string());
            out.push('}');
        }
        SlideBullet::Number(n) => {
            let _ = write!(
                out,
                ",\"bullet\":{{\"kind\":\"number\",\"style\":\"{}\"}}",
                n.as_str()
            );
        }
    }
    if let Some(pct) = p.line_spacing_pct {
        let _ = write!(out, ",\"lineSpacingPct\":{pct}");
    }
    for (key, v) in [
        ("spaceBeforePt", p.space_before_pt),
        ("spaceAfterPt", p.space_after_pt),
    ] {
        if let Some(v) = v {
            let _ = write!(out, ",\"{key}\":");
            push_f64(out, v);
        }
    }
    out.push('}');
}

/// Fields appended to a slide object: hidden flag, background, footer, groups and animations.
pub(crate) fn write_slide_ext(
    out: &mut String,
    ext: &SlideExt,
    table: &ImageTable,
    push_str: fn(&mut String, &str),
) {
    if ext.hidden {
        out.push_str(",\"hidden\":true");
    }
    if let Some(dir) = ext.transition_dir {
        let _ = write!(out, ",\"transitionDir\":\"{}\"", dir.as_str());
    }
    if let Some(ms) = ext.advance_after_ms {
        let _ = write!(out, ",\"advanceAfterMs\":{ms}");
    }
    if ext.no_click_advance {
        out.push_str(",\"noClickAdvance\":true");
    }
    match &ext.background {
        None => {}
        Some(SlideBackground::Solid(c)) => {
            let _ = write!(
                out,
                ",\"background\":{{\"kind\":\"solid\",\"color\":\"{c}\"}}"
            );
        }
        Some(SlideBackground::Gradient {
            from,
            to,
            angle_deg,
        }) => {
            let _ = write!(out, ",\"background\":{{\"kind\":\"gradient\",\"from\":\"{from}\",\"to\":\"{to}\",\"angleDeg\":{angle_deg}}}");
        }
        Some(SlideBackground::Picture(img)) => {
            out.push_str(",\"background\":{\"kind\":\"picture\",\"imageRef\":");
            push_str(out, &table.id_of(img));
            out.push('}');
        }
    }
    let f = &ext.footer;
    if !f.is_default() {
        let _ = write!(out, ",\"footer\":{{\"number\":{}", f.number);
        match &f.date {
            SlideDate::None => {}
            SlideDate::Auto => out.push_str(",\"date\":{\"kind\":\"auto\"}"),
            SlideDate::Fixed(t) => {
                out.push_str(",\"date\":{\"kind\":\"fixed\",\"text\":");
                push_str(out, t);
                out.push('}');
            }
        }
        if let Some(t) = &f.text {
            out.push_str(",\"text\":");
            push_str(out, t);
        }
        out.push('}');
    }
    if !ext.groups.is_empty() {
        out.push_str(",\"groups\":[");
        for (i, g) in ext.groups.iter().enumerate() {
            out.push_str(if i > 0 { ",{\"id\":" } else { "{\"id\":" });
            push_str(out, &g.id);
            out.push_str(",\"members\":[");
            for (j, m) in g.members.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                push_str(out, m);
            }
            out.push_str("]}");
        }
        out.push(']');
    }
    if !ext.anims.is_empty() {
        out.push_str(",\"anims\":[");
        for (i, a) in ext.anims.iter().enumerate() {
            out.push_str(if i > 0 {
                ",{\"target\":"
            } else {
                "{\"target\":"
            });
            push_str(out, &a.target);
            let _ = write!(
                out,
                ",\"class\":\"{}\",\"effect\":\"{}\",\"start\":\"{}\",\"durationMs\":{},\"delayMs\":{},\"direction\":\"{}\"}}",
                a.class.as_str(),
                a.effect.as_str(),
                a.start.as_str(),
                a.duration_ms,
                a.delay_ms,
                a.direction.as_str()
            );
        }
        out.push(']');
    }
}
