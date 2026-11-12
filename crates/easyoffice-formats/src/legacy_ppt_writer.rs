//! `.ppt` (MS-PPT, PowerPoint 97-2003) stream writer.
//!
//! The document container, environment and main master come from a blank-presentation template
//! (see `assets/legacy/` and `THIRD_PARTY_NOTICES.md`). The writer adds one slide container per
//! slide (title and body placeholders with their text in the shape), lists them in a
//! `SlideListWithText`, rebuilds the drawing group for the new drawings, and ends the stream with a
//! persist directory and a user edit atom that the `Current User` stream points at. The nested
//! EasyOffice payload (title, notes, theme, transition) is kept in one private top-level record that
//! no persist directory entry refers to, so a save/open round trip stays lossless.

use crate::ole2_codec::{read_u16_le, read_u32_le};
use crate::{EasyOfficeError, LegacyOle2Codec, LegacyPptPayload, LegacyPptSlide};

const PPT_TEMPLATE: &[u8] = include_bytes!("../assets/legacy/ppt_document.bin");

/// Private top-level record type; not defined by MS-PPT, never referenced by a persist entry.
pub(crate) const RT_EASYOFFICE_PRIVATE: u16 = 0x7EA0;

const RT_END_DOCUMENT: u16 = 0x03EA;
const RT_MAIN_MASTER: u16 = 0x03F8;
const RT_DRAWING_GROUP: u16 = 0x040B;
const RT_PP_DRAWING: u16 = 0x040C;
const RT_COLOR_SCHEME: u16 = 0x07F0;
const RT_PERSIST_DIRECTORY: u16 = 0x1772;
const RT_USER_EDIT: u16 = 0x0FF5;
const RT_CURRENT_USER: u16 = 0x0FF6;
const ESCHER_DGG_CONTAINER: u16 = 0xF000;
const ESCHER_FDGG: u16 = 0xF006;
const ESCHER_DG_CONTAINER: u16 = 0xF002;
const ESCHER_SPGR_CONTAINER: u16 = 0xF003;
const ESCHER_SP_CONTAINER: u16 = 0xF004;
const ESCHER_DG: u16 = 0xF008;
const ESCHER_SPGR: u16 = 0xF009;
const ESCHER_SP: u16 = 0xF00A;
const ESCHER_OPT: u16 = 0xF00B;
const ESCHER_CLIENT_TEXTBOX: u16 = 0xF00D;
const ESCHER_CLIENT_ANCHOR: u16 = 0xF010;
const ESCHER_CLIENT_DATA: u16 = 0xF011;
const RT_OE_PLACEHOLDER: u16 = 0x0BC3;

const SLIDE_ID_BASE: u32 = 256;
const FIRST_SLIDE_PERSIST_ID: u32 = 3;
const MASTER_SLIDE_ID: u32 = 0x8000_0000;
const MAX_PERSIST_PER_ENTRY: usize = 4000;
/// Escher shape type "rectangle" and the shape flags HaveAnchor | HasShapeType.
const MSOSPT_RECTANGLE: u16 = 1;
const SP_FLAGS_PLACEHOLDER: u32 = 0x0A00;
const SP_FLAGS_GROUP: u32 = 0x0005;
const TITLE_ANCHOR: [u8; 8] = [0xAD, 0x00, 0x20, 0x01, 0x60, 0x15, 0x7D, 0x03];
const BODY_ANCHOR: [u8; 8] = [0xF0, 0x03, 0x20, 0x01, 0x60, 0x15, 0x13, 0x0F];

struct Rec<'a> {
    ver_inst: u16,
    rec_type: u16,
    body: &'a [u8],
    raw: &'a [u8],
}

fn corrupt(msg: &str) -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(format!("PPT 템플릿 {msg}"))
}

fn to_u32(n: usize) -> Result<u32, EasyOfficeError> {
    u32::try_from(n).map_err(|_| {
        EasyOfficeError::ResourceBudgetExceeded(
            "PPT 레코드가 형식 상한을 초과했습니다.".to_string(),
        )
    })
}

fn split_records(data: &[u8]) -> Result<Vec<Rec<'_>>, EasyOfficeError> {
    let mut out = Vec::new();
    let mut off = 0usize;
    while off < data.len() {
        let body_start = off
            .checked_add(8)
            .filter(|e| *e <= data.len())
            .ok_or_else(|| corrupt("헤더 절단"))?;
        let len = read_u32_le(data, off + 4)? as usize;
        let end = body_start
            .checked_add(len)
            .filter(|e| *e <= data.len())
            .ok_or_else(|| corrupt("레코드 경계 초과"))?;
        out.push(Rec {
            ver_inst: read_u16_le(data, off)?,
            rec_type: read_u16_le(data, off + 2)?,
            body: &data[body_start..end],
            raw: &data[off..end],
        });
        off = end;
    }
    Ok(out)
}

fn find<'a, 'b>(recs: &'b [Rec<'a>], rec_type: u16) -> Result<&'b Rec<'a>, EasyOfficeError> {
    recs.iter()
        .find(|r| r.rec_type == rec_type)
        .ok_or_else(|| corrupt("필수 레코드 누락"))
}

fn put(
    buf: &mut Vec<u8>,
    ver: u8,
    inst: u16,
    rec_type: u16,
    body: &[u8],
) -> Result<(), EasyOfficeError> {
    buf.extend_from_slice(&((inst << 4) | u16::from(ver)).to_le_bytes());
    buf.extend_from_slice(&rec_type.to_le_bytes());
    buf.extend_from_slice(&to_u32(body.len())?.to_le_bytes());
    buf.extend_from_slice(body);
    Ok(())
}

fn rec_bytes(ver: u8, inst: u16, rec_type: u16, body: &[u8]) -> Result<Vec<u8>, EasyOfficeError> {
    let mut v = Vec::with_capacity(body.len() + 8);
    put(&mut v, ver, inst, rec_type, body)?;
    Ok(v)
}

/// Appends a record whose first header word is written verbatim (historic nested layout).
fn put_raw(
    buf: &mut Vec<u8>,
    word: u16,
    rec_type: u16,
    body: &[u8],
) -> Result<(), EasyOfficeError> {
    buf.extend_from_slice(&word.to_le_bytes());
    buf.extend_from_slice(&rec_type.to_le_bytes());
    buf.extend_from_slice(&to_u32(body.len())?.to_le_bytes());
    buf.extend_from_slice(body);
    Ok(())
}

/// Nested EasyOffice payload (title, slides with notes/theme/transition) in the original layout.
fn nested_payload(payload: &LegacyPptPayload) -> Result<Vec<u8>, EasyOfficeError> {
    let chars = LegacyOle2Codec::RT_TEXT_CHARS_ATOM;
    let mut doc_children = Vec::new();
    put_raw(&mut doc_children, 0x0000, chars, &utf16le(&payload.title))?;
    for slide in &payload.slides {
        let mut kids = Vec::new();
        let meta = format!("{}|{}", slide.theme, slide.transition);
        put_raw(
            &mut kids,
            0x0002,
            LegacyOle2Codec::RT_SLIDE_ATOM,
            &utf16le(&meta),
        )?;
        put_raw(&mut kids, 0x0000, chars, &utf16le(&slide.title))?;
        put_raw(&mut kids, 0x0020, chars, &utf16le(&slide.notes))?;
        for b in &slide.bullets {
            put_raw(&mut kids, 0x0030, chars, &utf16le(b))?;
        }
        put_raw(&mut doc_children, 0x000F, LegacyOle2Codec::RT_SLIDE, &kids)?;
    }
    let mut out = Vec::new();
    put_raw(
        &mut out,
        0x000F,
        LegacyOle2Codec::RT_DOCUMENT,
        &doc_children,
    )?;
    Ok(out)
}

/// Text shown by a placeholder: no CR inside a title, `\r` between body paragraphs.
fn clean_text(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\r' | '\n' | '\u{000B}' => ' ',
            c if (c as u32) < 0x20 || c == '\u{007F}' => ' ',
            c => c,
        })
        .collect()
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn opt_props(lt_xid: u32) -> Vec<u8> {
    // lTxid, no fill, no line, no shadow (sorted by property id).
    let props: [(u16, u32); 4] = [
        (0x0080, lt_xid),
        (0x01BF, 0x0010_0000),
        (0x01FF, 0x0008_0000),
        (0x023F, 0x0002_0000),
    ];
    let mut out = Vec::new();
    for (id, val) in props {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&val.to_le_bytes());
    }
    out
}

fn sp_container(
    spid: u32,
    anchor: &[u8; 8],
    placement_id: u32,
    placeholder: u8,
    text_type: u32,
    text: &str,
) -> Result<Vec<u8>, EasyOfficeError> {
    let mut sp = Vec::new();
    sp.extend_from_slice(&spid.to_le_bytes());
    sp.extend_from_slice(&SP_FLAGS_PLACEHOLDER.to_le_bytes());
    let mut kids = rec_bytes(2, MSOSPT_RECTANGLE, ESCHER_SP, &sp)?;
    kids.extend(rec_bytes(3, 4, ESCHER_OPT, &opt_props(spid))?);
    kids.extend(rec_bytes(0, 0, ESCHER_CLIENT_ANCHOR, anchor)?);
    let mut oe = Vec::new();
    oe.extend_from_slice(&placement_id.to_le_bytes());
    oe.extend_from_slice(&[placeholder, 0, 0, 0]);
    let data = rec_bytes(0, 0, RT_OE_PLACEHOLDER, &oe)?;
    kids.extend(rec_bytes(0xF, 0, ESCHER_CLIENT_DATA, &data)?);
    let mut tb = rec_bytes(
        0,
        0,
        LegacyOle2Codec::RT_TEXT_HEADER_ATOM,
        &text_type.to_le_bytes(),
    )?;
    tb.extend(rec_bytes(
        0,
        0,
        LegacyOle2Codec::RT_TEXT_CHARS_ATOM,
        &utf16le(text),
    )?);
    kids.extend(rec_bytes(0xF, 0, ESCHER_CLIENT_TEXTBOX, &tb)?);
    rec_bytes(0xF, 0, ESCHER_SP_CONTAINER, &kids)
}

struct MasterParts {
    color_scheme: Vec<u8>,
    background: Vec<u8>,
}

fn master_parts(master_body: &[u8]) -> Result<MasterParts, EasyOfficeError> {
    let kids = split_records(master_body)?;
    let color_scheme = kids
        .iter()
        .find(|r| r.rec_type == RT_COLOR_SCHEME && (r.ver_inst >> 4) == 1)
        .ok_or_else(|| corrupt("색 구성표 누락"))?
        .raw
        .to_vec();
    let drawing = split_records(find(&kids, RT_PP_DRAWING)?.body)?;
    let dg = split_records(find(&drawing, ESCHER_DG_CONTAINER)?.body)?;
    let background = dg
        .iter()
        .rev()
        .find(|r| r.rec_type == ESCHER_SP_CONTAINER)
        .ok_or_else(|| corrupt("배경 도형 누락"))?
        .raw
        .to_vec();
    if background.len() < 20 {
        return Err(corrupt("배경 도형 절단"));
    }
    Ok(MasterParts {
        color_scheme,
        background,
    })
}

struct SlideShapes {
    spids_used: u32,
}

fn slide_container(
    index: usize,
    slide: &LegacyPptSlide,
    parts: &MasterParts,
) -> Result<(Vec<u8>, SlideShapes), EasyOfficeError> {
    let drawing_id = u32::try_from(index)
        .ok()
        .and_then(|i| i.checked_add(2))
        .filter(|d| *d < 4096)
        .ok_or_else(|| {
            EasyOfficeError::ResourceBudgetExceeded(
                "PPT 슬라이드 개수가 형식 상한을 초과했습니다.".to_string(),
            )
        })?;
    let base = drawing_id * 1024;
    let body_text = slide
        .bullets
        .iter()
        .map(|b| clean_text(b))
        .collect::<Vec<_>>()
        .join("\r");
    let has_body = !slide.bullets.is_empty();

    let mut slide_atom = Vec::new();
    slide_atom.extend_from_slice(&(if has_body { 1u32 } else { 7u32 }).to_le_bytes());
    let mut placeholders = [0u8; 8];
    placeholders[0] = 0x0D;
    if has_body {
        placeholders[1] = 0x0E;
    }
    slide_atom.extend_from_slice(&placeholders);
    slide_atom.extend_from_slice(&MASTER_SLIDE_ID.to_le_bytes());
    slide_atom.extend_from_slice(&0u32.to_le_bytes()); // notesIdRef
    slide_atom.extend_from_slice(&0x0007u16.to_le_bytes()); // follow master objects/scheme/background
    slide_atom.extend_from_slice(&0u16.to_le_bytes());

    let mut group = Vec::new();
    group.extend(rec_bytes(1, 0, ESCHER_SPGR, &[0u8; 16])?);
    let mut group_sp = Vec::new();
    group_sp.extend_from_slice(&base.to_le_bytes());
    group_sp.extend_from_slice(&SP_FLAGS_GROUP.to_le_bytes());
    group.extend(rec_bytes(2, 0, ESCHER_SP, &group_sp)?);
    let mut spgr_kids = rec_bytes(0xF, 0, ESCHER_SP_CONTAINER, &group)?;
    spgr_kids.extend(sp_container(
        base + 2,
        &TITLE_ANCHOR,
        0,
        0x0D,
        0,
        &clean_text(&slide.title),
    )?);
    let mut spids_used = 3u32; // group, background, title
    if has_body {
        spgr_kids.extend(sp_container(
            base + 3,
            &BODY_ANCHOR,
            1,
            0x0E,
            1,
            &body_text,
        )?);
        spids_used += 1;
    }

    let mut dg_body = Vec::new();
    let mut dg_atom = Vec::new();
    dg_atom.extend_from_slice(&(spids_used - 1).to_le_bytes());
    dg_atom.extend_from_slice(&(base + spids_used - 1).to_le_bytes());
    dg_body.extend(rec_bytes(
        0,
        u16::try_from(drawing_id).unwrap_or(0),
        ESCHER_DG,
        &dg_atom,
    )?);
    dg_body.extend(rec_bytes(0xF, 0, ESCHER_SPGR_CONTAINER, &spgr_kids)?);
    let mut bg = parts.background.clone();
    bg[16..20].copy_from_slice(&(base + 1).to_le_bytes());
    dg_body.extend(bg);

    let dg_container = rec_bytes(0xF, 0, ESCHER_DG_CONTAINER, &dg_body)?;
    let mut body = rec_bytes(2, 0, LegacyOle2Codec::RT_SLIDE_ATOM, &slide_atom)?;
    body.extend(rec_bytes(0xF, 0, RT_PP_DRAWING, &dg_container)?);
    body.extend_from_slice(&parts.color_scheme);
    Ok((
        rec_bytes(0xF, 0, LegacyOle2Codec::RT_SLIDE, &body)?,
        SlideShapes { spids_used },
    ))
}

/// Drawing group with one cluster per drawing (master = drawing 1, slide `i` = drawing `i + 2`).
fn drawing_group(template: &Rec<'_>, shapes: &[SlideShapes]) -> Result<Vec<u8>, EasyOfficeError> {
    if shapes.is_empty() {
        return Ok(template.raw.to_vec());
    }
    let outer = split_records(template.body)?;
    let dgg = find(&outer, ESCHER_DGG_CONTAINER)?;
    let kids = split_records(dgg.body)?;
    let old = find(&kids, ESCHER_FDGG)?;
    if old.body.len() < 16 {
        return Err(corrupt("드로잉 그룹 절단"));
    }
    let master_csp = read_u32_le(old.body, 8)?;
    let master_used = read_u32_le(old.body, 20)?;
    let mut clusters: Vec<(u32, u32)> = vec![(1, master_used)];
    let mut csp = master_csp;
    for (i, s) in shapes.iter().enumerate() {
        clusters.push((to_u32(i + 2)?, s.spids_used));
        csp = csp.saturating_add(s.spids_used - 1);
    }
    let last = clusters.len() as u32;
    let last_used = clusters[clusters.len() - 1].1;
    let mut fdgg = Vec::new();
    fdgg.extend_from_slice(&(last * 1024 + last_used).to_le_bytes());
    fdgg.extend_from_slice(&(last + 1).to_le_bytes());
    fdgg.extend_from_slice(&csp.to_le_bytes());
    fdgg.extend_from_slice(&last.to_le_bytes());
    for (dg, used) in &clusters {
        fdgg.extend_from_slice(&dg.to_le_bytes());
        fdgg.extend_from_slice(&used.to_le_bytes());
    }
    let mut dgg_body = rec_bytes(0, 0, ESCHER_FDGG, &fdgg)?;
    for k in kids.iter().filter(|r| r.rec_type != ESCHER_FDGG) {
        dgg_body.extend_from_slice(k.raw);
    }
    let dgg_rec = rec_bytes(0xF, 0, ESCHER_DGG_CONTAINER, &dgg_body)?;
    rec_bytes(0xF, 0, RT_DRAWING_GROUP, &dgg_rec)
}

fn slide_list(count: usize) -> Result<Vec<u8>, EasyOfficeError> {
    let mut body = Vec::new();
    for i in 0..count {
        let i = to_u32(i)?;
        let mut atom = Vec::new();
        atom.extend_from_slice(&(FIRST_SLIDE_PERSIST_ID + i).to_le_bytes());
        atom.extend_from_slice(&0u32.to_le_bytes());
        atom.extend_from_slice(&0u32.to_le_bytes());
        atom.extend_from_slice(&(SLIDE_ID_BASE + i).to_le_bytes());
        atom.extend_from_slice(&0u32.to_le_bytes());
        put(
            &mut body,
            0,
            0,
            LegacyOle2Codec::RT_SLIDE_PERSIST_ATOM,
            &atom,
        )?;
    }
    rec_bytes(0xF, 0, LegacyOle2Codec::RT_SLIDE_LIST_WITH_TEXT, &body)
}

fn document_container(
    doc: &Rec<'_>,
    slides: usize,
    shapes: &[SlideShapes],
) -> Result<Vec<u8>, EasyOfficeError> {
    let mut body = Vec::new();
    for child in split_records(doc.body)? {
        if child.rec_type == RT_END_DOCUMENT && slides > 0 {
            body.extend(slide_list(slides)?);
        }
        if child.rec_type == RT_DRAWING_GROUP {
            body.extend(drawing_group(&child, shapes)?);
        } else {
            body.extend_from_slice(child.raw);
        }
    }
    rec_bytes(0xF, 0, LegacyOle2Codec::RT_DOCUMENT, &body)
}

/// Builds the `PowerPoint Document` and `Current User` streams.
pub(crate) fn build_ppt_streams(
    payload: &LegacyPptPayload,
) -> Result<(Vec<u8>, Vec<u8>), EasyOfficeError> {
    let top = split_records(PPT_TEMPLATE)?;
    let doc = find(&top, LegacyOle2Codec::RT_DOCUMENT)?;
    let master = find(&top, RT_MAIN_MASTER)?;
    let parts = master_parts(master.body)?;

    let mut slide_bytes = Vec::new();
    let mut shapes = Vec::new();
    for (i, slide) in payload.slides.iter().enumerate() {
        let (bytes, s) = slide_container(i, slide, &parts)?;
        slide_bytes.push(bytes);
        shapes.push(s);
    }

    let mut stream = document_container(doc, payload.slides.len(), &shapes)?;
    let mut offsets: Vec<u32> = vec![0, to_u32(stream.len())?];
    stream.extend_from_slice(master.raw);
    for bytes in &slide_bytes {
        offsets.push(to_u32(stream.len())?);
        stream.extend_from_slice(bytes);
    }

    put(
        &mut stream,
        0,
        0,
        RT_EASYOFFICE_PRIVATE,
        &nested_payload(payload)?,
    )?;

    let persist_dir_offset = to_u32(stream.len())?;
    let mut dir = Vec::new();
    for (chunk_idx, chunk) in offsets.chunks(MAX_PERSIST_PER_ENTRY).enumerate() {
        let first_id = to_u32(chunk_idx * MAX_PERSIST_PER_ENTRY + 1)?;
        dir.extend_from_slice(&(first_id | (to_u32(chunk.len())? << 20)).to_le_bytes());
        for off in chunk {
            dir.extend_from_slice(&off.to_le_bytes());
        }
    }
    put(&mut stream, 0, 0, RT_PERSIST_DIRECTORY, &dir)?;

    let user_edit_offset = to_u32(stream.len())?;
    let slide_count = to_u32(payload.slides.len())?;
    let last_slide_id = if slide_count == 0 {
        SLIDE_ID_BASE
    } else {
        SLIDE_ID_BASE + slide_count - 1
    };
    let mut ue = Vec::new();
    ue.extend_from_slice(&last_slide_id.to_le_bytes());
    ue.extend_from_slice(&0x106Du16.to_le_bytes());
    ue.extend_from_slice(&[0, 3]); // minor 0, major 3
    ue.extend_from_slice(&0u32.to_le_bytes()); // offsetLastEdit: none, the stream is self-contained
    ue.extend_from_slice(&persist_dir_offset.to_le_bytes());
    ue.extend_from_slice(&1u32.to_le_bytes()); // docPersistIdRef
    ue.extend_from_slice(&(FIRST_SLIDE_PERSIST_ID - 1 + slide_count).to_le_bytes()); // persist id seed
    ue.extend_from_slice(&1u16.to_le_bytes()); // lastViewType: normal
    ue.extend_from_slice(&0u16.to_le_bytes());
    put(&mut stream, 0, 0, RT_USER_EDIT, &ue)?;

    Ok((stream, current_user(user_edit_offset)?))
}

fn current_user(offset_to_current_edit: u32) -> Result<Vec<u8>, EasyOfficeError> {
    const USER: &str = "EasyOffice";
    let mut body = Vec::new();
    body.extend_from_slice(&0x14u32.to_le_bytes());
    body.extend_from_slice(&0xE391_C05Fu32.to_le_bytes()); // header token: not encrypted
    body.extend_from_slice(&offset_to_current_edit.to_le_bytes());
    body.extend_from_slice(&(USER.len() as u16).to_le_bytes());
    body.extend_from_slice(&0x03F4u16.to_le_bytes()); // docFileVersion
    body.extend_from_slice(&[3, 0, 0, 0]); // majorVersion, minorVersion, unused
    body.extend_from_slice(USER.as_bytes());
    body.extend_from_slice(&8u32.to_le_bytes()); // relFlags
    let mut out = rec_bytes(0, 0, RT_CURRENT_USER, &body)?;
    out.extend_from_slice(&utf16le(USER));
    Ok(out)
}
