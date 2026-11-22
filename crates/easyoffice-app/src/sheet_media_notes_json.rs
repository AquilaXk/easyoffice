//! JSON wire serialization and `structure` row/column shifting for worksheet
//! floating images (`images`) and cell notes (`notes`) (`PD-256`).

use std::collections::BTreeMap;
use std::fmt::Write;

use easyoffice_formats::{
    base64_codec, pack_cell_pos, sync_image_grid_from_px, sync_image_px_from_grid, AnchorEdit,
    ChartAnchor, EasyOfficeError, RichImage, RichImageFormat, SheetCellNote, SheetFloatingImage,
    SheetGrid, StrictJsonValue, MAX_IMAGE_BYTES, MAX_NOTES_PER_SHEET, MAX_NOTE_AUTHOR_BYTES,
    MAX_SHEET_IMAGES_PER_SHEET, MAX_SHEET_IMAGE_ID_BYTES, MAX_SHEET_IMAGE_NAME_BYTES,
    MAX_SHEET_NOTE_TEXT_BYTES,
};
use easyoffice_sheet::{Axis, AxisEdit, EditKind, PackedCellCoordinate};

use crate::EasyOfficeIpcRouter;

const MAX_XLSX_ROWS: u32 = PackedCellCoordinate::MAX_ROWS;
const MAX_XLSX_COLS: u32 = PackedCellCoordinate::MAX_COLS;
const LAST_ROW: u32 = MAX_XLSX_ROWS - 1;
const LAST_COL: u32 = MAX_XLSX_COLS - 1;

fn invalid(msg: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(msg.to_string())
}

fn opt_str<'a>(
    entries: &'a [(String, StrictJsonValue)],
    key: &str,
    field_label: &str,
) -> Result<Option<&'a str>, EasyOfficeError> {
    match StrictJsonValue::get_field(entries, key) {
        None | Some(StrictJsonValue::Null) => Ok(None),
        Some(StrictJsonValue::Str(s)) => Ok(Some(s.as_str())),
        Some(_) => Err(invalid(&format!("{field_label} 값이 문자열이어야 합니다."))),
    }
}

fn opt_u32_any(
    entries: &[(String, StrictJsonValue)],
    keys: &[&str],
) -> Result<Option<u32>, EasyOfficeError> {
    for &key in keys {
        match StrictJsonValue::get_field(entries, key) {
            None | Some(StrictJsonValue::Null) => continue,
            Some(v) => {
                let Some(n) = v.as_u64() else {
                    return Err(invalid(&format!("anchor.{key} 값이 정수여야 합니다.")));
                };
                let parsed = u32::try_from(n)
                    .map_err(|_| invalid(&format!("anchor.{key} 값이 범위를 벗어났습니다.")))?;
                return Ok(Some(parsed));
            }
        }
    }
    Ok(None)
}

/// `editAs` of an anchor (`twoCell`, `oneCell`, `absolute`); missing, not a string or any other text is the default.
pub(crate) fn parse_edit_as(obj: &[(String, StrictJsonValue)]) -> AnchorEdit {
    match StrictJsonValue::get_field(obj, "editAs") {
        Some(StrictJsonValue::Str(s)) => AnchorEdit::parse(s),
        _ => AnchorEdit::TwoCell,
    }
}

/// `,"editAs":".."` for an anchor that is not the default two-cell kind.
pub(crate) fn write_edit_as(out: &mut String, edit_as: AnchorEdit) {
    if edit_as != AnchorEdit::TwoCell {
        let _ = write!(out, ",\"editAs\":\"{}\"", edit_as.as_str());
    }
}

fn parse_image_anchor(
    val: Option<&StrictJsonValue>,
    grid: &SheetGrid<'_>,
) -> Result<ChartAnchor, EasyOfficeError> {
    let mut a = ChartAnchor::default();
    let Some(v) = val.filter(|v| !matches!(v, StrictJsonValue::Null)) else {
        a.width_px = 240;
        a.height_px = 160;
        sync_image_grid_from_px(&mut a, grid);
        return Ok(a);
    };
    let Some(obj) = v.as_object() else {
        return Err(invalid("그림 anchor가 객체여야 합니다."));
    };
    if let Some(v) = opt_u32_any(obj, &["fromCol"])? {
        if v >= MAX_XLSX_COLS {
            return Err(invalid("anchor.fromCol 범위를 초과했습니다."));
        }
        a.from_col = v;
    }
    if let Some(v) = opt_u32_any(obj, &["fromRow"])? {
        if v >= MAX_XLSX_ROWS {
            return Err(invalid("anchor.fromRow 범위를 초과했습니다."));
        }
        a.from_row = v;
    }
    if let Some(v) = opt_u32_any(obj, &["colOffEmu"])? {
        a.col_off_emu = v;
    }
    if let Some(v) = opt_u32_any(obj, &["rowOffEmu"])? {
        a.row_off_emu = v;
    }
    if let Some(v) = opt_u32_any(obj, &["toCol"])? {
        if v >= MAX_XLSX_COLS {
            return Err(invalid("anchor.toCol 범위를 초과했습니다."));
        }
        a.to_col = v;
    }
    if let Some(v) = opt_u32_any(obj, &["toRow"])? {
        if v >= MAX_XLSX_ROWS {
            return Err(invalid("anchor.toRow 범위를 초과했습니다."));
        }
        a.to_row = v;
    }
    if let Some(v) = opt_u32_any(obj, &["toColOffEmu"])? {
        a.to_col_off_emu = v;
    }
    if let Some(v) = opt_u32_any(obj, &["toRowOffEmu"])? {
        a.to_row_off_emu = v;
    }
    a.edit_as = parse_edit_as(obj);
    let left_opt = opt_u32_any(obj, &["leftPx", "left"])?;
    let top_opt = opt_u32_any(obj, &["topPx", "top"])?;
    let width_opt = opt_u32_any(obj, &["widthPx", "width"])?;
    let height_opt = opt_u32_any(obj, &["heightPx", "height"])?;

    if let Some(w) = width_opt {
        if !(1..=8_192).contains(&w) {
            return Err(invalid("그림 너비가 허용 범위(1..=8192)를 벗어났습니다."));
        }
        a.width_px = w;
    }
    if let Some(h) = height_opt {
        if !(1..=8_192).contains(&h) {
            return Err(invalid("그림 높이가 허용 범위(1..=8192)를 벗어났습니다."));
        }
        a.height_px = h;
    }
    if let Some(l) = left_opt {
        a.left_px = l;
    }
    if let Some(t) = top_opt {
        a.top_px = t;
    }

    if !a.has_explicit_grid() {
        if width_opt.is_none() {
            a.width_px = 240;
        }
        if height_opt.is_none() {
            a.height_px = 160;
        }
        sync_image_grid_from_px(&mut a, grid);
    } else if width_opt.is_none() || height_opt.is_none() {
        let mut derived = a;
        sync_image_px_from_grid(&mut derived, grid);
        if left_opt.is_none() {
            a.left_px = derived.left_px;
        }
        if top_opt.is_none() {
            a.top_px = derived.top_px;
        }
        if width_opt.is_none() {
            a.width_px = derived.width_px;
        }
        if height_opt.is_none() {
            a.height_px = derived.height_px;
        }
    }
    Ok(a)
}

fn validate_mime(mime_opt: Option<&str>) -> Result<Option<RichImageFormat>, EasyOfficeError> {
    match mime_opt {
        None => Ok(None),
        Some("image/png") => Ok(Some(RichImageFormat::Png)),
        Some("image/jpeg") => Ok(Some(RichImageFormat::Jpeg)),
        Some(other) => Err(invalid(&format!("지원하지 않는 그림 MIME 형식: {other}"))),
    }
}

fn parse_id_and_name(
    obj: &[(String, StrictJsonValue)],
    idx: usize,
) -> Result<(String, String, String), EasyOfficeError> {
    let id = match StrictJsonValue::get_field(obj, "id") {
        None | Some(StrictJsonValue::Null) => format!("pic-{}", idx + 1),
        Some(StrictJsonValue::Str(s)) => {
            if s.is_empty() {
                return Err(invalid("그림 id가 비어 있습니다."));
            }
            if s.len() > MAX_SHEET_IMAGE_ID_BYTES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "그림 id 길이가 허용 상한을 초과했습니다.".to_string(),
                ));
            }
            s.clone()
        }
        Some(_) => return Err(invalid("그림 id가 문자열이어야 합니다.")),
    };
    let name = match opt_str(obj, "name", "그림 이름")? {
        None | Some("") => id.clone(),
        Some(s) => {
            if s.len() > MAX_SHEET_IMAGE_NAME_BYTES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "그림 이름이 허용 상한을 초과했습니다.".to_string(),
                ));
            }
            s.to_string()
        }
    };
    let alt = opt_str(obj, "alt", "대체 텍스트")?
        .unwrap_or("")
        .to_string();
    Ok((id, name, alt))
}

/// Parses `sheets[i].images` for full save/convert.
pub(crate) fn parse_images(
    v: &StrictJsonValue,
    grid: &SheetGrid<'_>,
) -> Result<Vec<SheetFloatingImage>, EasyOfficeError> {
    let Some(items) = v.as_array() else {
        return Err(invalid("시트의 그림 목록이 배열이어야 합니다."));
    };
    if items.len() > MAX_SHEET_IMAGES_PER_SHEET {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "시트 그림 수가 허용 상한({MAX_SHEET_IMAGES_PER_SHEET})을 초과했습니다."
        )));
    }
    let max_b64_len = (MAX_IMAGE_BYTES / 3 + 2) * 4;
    let mut out = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        let Some(obj) = item.as_object() else {
            return Err(invalid("그림 항목이 객체여야 합니다."));
        };
        let (id, name, alt) = parse_id_and_name(obj, idx)?;
        let expected_fmt = validate_mime(opt_str(obj, "mime", "그림 MIME")?)?;
        let data_b64 = opt_str(obj, "data", "그림 데이터")?
            .ok_or_else(|| invalid("그림 data 필드가 비어 있습니다."))?;
        if data_b64.len() > max_b64_len {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "그림 바이트 크기가 허용 상한을 초과했습니다.".to_string(),
            ));
        }
        let bytes = base64_codec::decode(data_b64)
            .ok_or_else(|| invalid("그림 Base64 데이터가 올바르지 않습니다."))?;
        let image = RichImage::new(bytes, alt.clone())?;
        if let Some(fmt) = expected_fmt {
            if image.format() != fmt {
                return Err(invalid("그림 MIME과 실제 이미지 형식이 일치하지 않습니다."));
            }
        }
        let anchor = parse_image_anchor(StrictJsonValue::get_field(obj, "anchor"), grid)?;
        out.push(SheetFloatingImage {
            id,
            name,
            alt,
            image,
            anchor,
        });
    }
    Ok(out)
}

fn write_anchor_json(out: &mut String, a: &ChartAnchor) {
    let _ = write!(
        out,
        "{{\"fromCol\":{},\"fromRow\":{},\"colOffEmu\":{},\"rowOffEmu\":{},\"toCol\":{},\"toRow\":{},\"toColOffEmu\":{},\"toRowOffEmu\":{},\"leftPx\":{},\"topPx\":{},\"widthPx\":{},\"heightPx\":{}",
        a.from_col,
        a.from_row,
        a.col_off_emu,
        a.row_off_emu,
        a.to_col,
        a.to_row,
        a.to_col_off_emu,
        a.to_row_off_emu,
        a.left_px,
        a.top_px,
        a.width_px,
        a.height_px
    );
    write_edit_as(out, a.edit_as);
    out.push('}');
}

pub(crate) fn write_images_json(out: &mut String, images: &[SheetFloatingImage]) {
    out.push('[');
    for (i, img) in images.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":\"");
        EasyOfficeIpcRouter::escape_json_into(out, &img.id);
        out.push_str("\",\"name\":\"");
        EasyOfficeIpcRouter::escape_json_into(out, &img.name);
        out.push_str("\",\"alt\":\"");
        EasyOfficeIpcRouter::escape_json_into(out, &img.alt);
        out.push_str("\",\"mime\":\"");
        out.push_str(img.image.format().mime());
        out.push_str("\",\"data\":\"");
        out.push_str(&base64_codec::encode(img.image.bytes()));
        out.push_str("\",\"anchor\":");
        write_anchor_json(out, &img.anchor);
        out.push('}');
    }
    out.push(']');
}

/// Lightweight image entry for `evaluate_sheet` `structure` edits (`data` optional, `imageId` preserved).
pub(crate) struct StructureImage {
    pub id: String,
    pub name: String,
    pub alt: String,
    pub mime: Option<String>,
    pub image_id: Option<String>,
    pub data: Option<String>,
    pub anchor: ChartAnchor,
}

pub(crate) fn parse_images_for_structure(
    v: &StrictJsonValue,
    grid: &SheetGrid<'_>,
) -> Result<Vec<StructureImage>, EasyOfficeError> {
    let Some(items) = v.as_array() else {
        return Err(invalid("시트의 그림 목록이 배열이어야 합니다."));
    };
    if items.len() > MAX_SHEET_IMAGES_PER_SHEET {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "시트 그림 수가 허용 상한({MAX_SHEET_IMAGES_PER_SHEET})을 초과했습니다."
        )));
    }
    let mut out = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        let Some(obj) = item.as_object() else {
            return Err(invalid("그림 항목이 객체여야 합니다."));
        };
        let (id, name, alt) = parse_id_and_name(obj, idx)?;
        let mime_opt = opt_str(obj, "mime", "그림 MIME")?;
        validate_mime(mime_opt)?;
        let image_id = opt_str(obj, "imageId", "그림 캐시 ID")?.map(str::to_string);
        let data = opt_str(obj, "data", "그림 데이터")?.map(str::to_string);
        let anchor = parse_image_anchor(StrictJsonValue::get_field(obj, "anchor"), grid)?;
        out.push(StructureImage {
            id,
            name,
            alt,
            mime: mime_opt.map(str::to_string),
            image_id,
            data,
            anchor,
        });
    }
    Ok(out)
}

fn shift_anchor_span(lo: u32, hi: u32, edit: &AxisEdit) -> (u32, u32) {
    let limit = match edit.axis {
        Axis::Row => LAST_ROW,
        Axis::Col => LAST_COL,
    };
    match edit.kind {
        EditKind::Insert => {
            if lo >= edit.at {
                (
                    lo.saturating_add(edit.count).min(limit),
                    hi.saturating_add(edit.count).min(limit),
                )
            } else if hi >= edit.at {
                (lo, hi.saturating_add(edit.count).min(limit))
            } else {
                (lo, hi)
            }
        }
        EditKind::Delete => {
            let del_end = edit.at.saturating_add(edit.count.saturating_sub(1));
            if hi < edit.at {
                (lo, hi)
            } else if lo > del_end {
                (lo - edit.count, hi - edit.count)
            } else if lo >= edit.at && hi <= del_end {
                (edit.at.min(limit), edit.at.min(limit))
            } else if lo >= edit.at {
                (edit.at.min(limit), hi - edit.count)
            } else if hi <= del_end {
                (lo, edit.at.saturating_sub(1).max(lo))
            } else {
                (lo, hi - edit.count)
            }
        }
    }
}

pub(crate) fn shift_structure_images(
    images: &[StructureImage],
    edit: &AxisEdit,
    grid: &SheetGrid<'_>,
) -> Vec<StructureImage> {
    images
        .iter()
        .map(|im| {
            let mut anchor = im.anchor;
            // An absolute anchor stays where it is; the others move with their cells (a one-cell one keeps its size).
            if anchor.edit_as != AnchorEdit::Absolute {
                match edit.axis {
                    Axis::Row => {
                        let (fr, tr) = shift_anchor_span(anchor.from_row, anchor.to_row, edit);
                        anchor.from_row = fr;
                        anchor.to_row = tr;
                    }
                    Axis::Col => {
                        let (fc, tc) = shift_anchor_span(anchor.from_col, anchor.to_col, edit);
                        anchor.from_col = fc;
                        anchor.to_col = tc;
                    }
                }
            }
            match anchor.edit_as {
                AnchorEdit::TwoCell => sync_image_px_from_grid(&mut anchor, grid),
                AnchorEdit::OneCell => anchor.start_on_cell(grid, sync_image_grid_from_px),
                AnchorEdit::Absolute => sync_image_grid_from_px(&mut anchor, grid),
            }
            StructureImage {
                id: im.id.clone(),
                name: im.name.clone(),
                alt: im.alt.clone(),
                mime: im.mime.clone(),
                image_id: im.image_id.clone(),
                data: im.data.clone(),
                anchor,
            }
        })
        .collect()
}

pub(crate) fn structure_images_json(images: &[StructureImage]) -> String {
    let mut out = String::with_capacity(64 + images.len() * 192);
    out.push('[');
    for (i, im) in images.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":\"");
        EasyOfficeIpcRouter::escape_json_into(&mut out, &im.id);
        out.push_str("\",\"name\":\"");
        EasyOfficeIpcRouter::escape_json_into(&mut out, &im.name);
        out.push_str("\",\"alt\":\"");
        EasyOfficeIpcRouter::escape_json_into(&mut out, &im.alt);
        out.push('"');
        if let Some(mime) = im.mime.as_deref() {
            out.push_str(",\"mime\":\"");
            EasyOfficeIpcRouter::escape_json_into(&mut out, mime);
            out.push('"');
        }
        if let Some(image_id) = im.image_id.as_deref() {
            out.push_str(",\"imageId\":\"");
            EasyOfficeIpcRouter::escape_json_into(&mut out, image_id);
            out.push('"');
        }
        if let Some(data) = im.data.as_deref() {
            out.push_str(",\"data\":\"");
            EasyOfficeIpcRouter::escape_json_into(&mut out, data);
            out.push('"');
        }
        out.push_str(",\"anchor\":");
        write_anchor_json(&mut out, &im.anchor);
        out.push('}');
    }
    out.push(']');
    out
}

pub(crate) fn parse_notes(
    v: &StrictJsonValue,
) -> Result<BTreeMap<u64, SheetCellNote>, EasyOfficeError> {
    let Some(items) = v.as_array() else {
        return Err(invalid("시트 메모 목록이 배열이어야 합니다."));
    };
    if items.len() > MAX_NOTES_PER_SHEET {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "시트 메모 수가 허용 상한({MAX_NOTES_PER_SHEET})을 초과했습니다."
        )));
    }
    let mut out = BTreeMap::new();
    for item in items {
        let Some(obj) = item.as_object() else {
            return Err(invalid("메모 항목이 객체여야 합니다."));
        };
        let cell_ref = opt_str(obj, "ref", "메모 셀 주소")?
            .ok_or_else(|| invalid("메모 ref 필드가 비어 있습니다."))?;
        let coord = PackedCellCoordinate::parse_a1(cell_ref)?;
        let author = opt_str(obj, "author", "메모 작성자")?
            .unwrap_or("")
            .to_string();
        if author.len() > MAX_NOTE_AUTHOR_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "메모 작성자 길이가 허용 상한을 초과했습니다.".to_string(),
            ));
        }
        let text = opt_str(obj, "text", "메모 내용")?.unwrap_or("").to_string();
        if text.len() > MAX_SHEET_NOTE_TEXT_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "메모 텍스트 길이가 허용 상한을 초과했습니다.".to_string(),
            ));
        }
        let visible = match StrictJsonValue::get_field(obj, "visible") {
            None | Some(StrictJsonValue::Null) => false,
            Some(StrictJsonValue::Bool(b)) => *b,
            Some(_) => return Err(invalid("메모 visible 값이 불리언이어야 합니다.")),
        };
        out.insert(
            pack_cell_pos(coord.row(), coord.col()),
            SheetCellNote {
                author,
                text,
                visible,
            },
        );
    }
    Ok(out)
}

pub(crate) fn write_notes_json(out: &mut String, notes: &BTreeMap<u64, SheetCellNote>) {
    out.push('[');
    for (idx, (&packed, note)) in notes.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str("{\"ref\":\"");
        PackedCellCoordinate(packed).write_a1(out);
        out.push_str("\",\"author\":\"");
        EasyOfficeIpcRouter::escape_json_into(out, &note.author);
        out.push_str("\",\"text\":\"");
        EasyOfficeIpcRouter::escape_json_into(out, &note.text);
        let _ = write!(out, "\",\"visible\":{}}}", note.visible);
    }
    out.push(']');
}

pub(crate) fn notes_json(notes: &BTreeMap<u64, SheetCellNote>) -> String {
    let mut out = String::with_capacity(64 + notes.len() * 64);
    write_notes_json(&mut out, notes);
    out
}

pub(crate) fn shift_notes(
    notes: &BTreeMap<u64, SheetCellNote>,
    edit: &AxisEdit,
) -> BTreeMap<u64, SheetCellNote> {
    let mut moved = BTreeMap::new();
    for (&packed, note) in notes {
        if let Some(next_coord) = edit.map_coord(PackedCellCoordinate(packed)) {
            moved.insert(next_coord.0, note.clone());
        }
    }
    moved
}
