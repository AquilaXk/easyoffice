//! Writes the result of an edit: the chosen pages of the open PDFs, in order, as one new PDF
//! (PD-170).
//!
//! Library choice (the three steps of the implementation rule): (1) `hayro-syntax` reads the files
//! and `hayro-write` copies a page together with every object it uses (fonts, pictures, shadings,
//! colour spaces, forms) into a new file; both are MIT OR Apache-2.0 and maintained. (2) Nothing
//! already in the shell reads a PDF back into objects; the PDF writer crate writes the pages around the
//! copies. (3) Nothing had to be ported: the one thing the library does not offer, changing a copied
//! page's `/Rotate`, is solved by writing the page ourselves.
//!
//! Every page is copied as a form XObject (`ExtractionQuery::new_xobject`) and placed on a page of
//! our own, because the library's "copy as page" seals the page dictionary. Our page keeps what the
//! original had on screen: the media box is the original's visible size, `/Rotate` is the original's
//! turn plus the requested one, and the form is drawn through the inverse of the rotation the
//! library bakes into it, so a page that was rotated stays rotated in the same way and a rotation
//! request only changes `/Rotate`. Overlays (stamp, watermark) are one more form drawn on top. The
//! page contents, the fonts and the pictures are copied byte for byte (the content stream is
//! decoded and compressed again by the library, pictures and fonts are not touched).
//!
//! Not carried over: annotations and links, form fields, outlines and the structure tree (the
//! library copies pages, not the catalog). Only the title, author, subject and keywords of the
//! first PDF are kept.

use crate::pdf_edit_budget::{
    Deadline, MAX_OUTPUT_BYTES, MAX_OVERLAY_SIZES, MAX_PAGES, MAX_TOTAL_CONTENT_BYTES,
};
use crate::pdf_edit_inspect::{open_pdf, page_content_bytes, resource_entries, Limits};
use crate::pdf_edit_overlay::{upright_matrix, Overlay};
pub use crate::pdf_edit_wire::{OverlayMaker, PagePlan};
use hayro_syntax::page::{Page, Rotation};
use hayro_syntax::Pdf as ReadPdf;
use hayro_write::pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use hayro_write::{extract, ChunkSettings, ExtractionQuery};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

const PRODUCER: &str = "EasyOffice";
/// Pages between two looks at the deadline, as a mask (a power of two minus one).
const PAGES_PER_CHECK_MASK: usize = 63;
const ORIGINAL_FORM: Name<'static> = Name(b"P");
const OVERLAY_FORM: Name<'static> = Name(b"W");

pub const MSG_TOO_BIG: &str = "편집 결과가 허용 크기(256MiB)를 초과합니다.";

/// A 2x3 matrix `[a b c d e f]` as the PDF writes it: x' = a x + c y + e, y' = b x + d y + f.
type Affine = [f64; 6];

/// `outer` after `inner`.
fn compose(outer: Affine, inner: Affine) -> Affine {
    let [a0, a1, a2, a3, a4, a5] = outer;
    let [b0, b1, b2, b3, b4, b5] = inner;
    [
        a0 * b0 + a2 * b1,
        a1 * b0 + a3 * b1,
        a0 * b2 + a2 * b3,
        a1 * b2 + a3 * b3,
        a0 * b4 + a2 * b5 + a4,
        a1 * b4 + a3 * b5 + a5,
    ]
}

fn invert(m: Affine) -> Option<Affine> {
    let [a, b, c, d, e, f] = m;
    let det = a * d - b * c;
    (det.abs() > 1e-12).then(|| {
        [
            d / det,
            -b / det,
            -c / det,
            a / det,
            (c * f - d * e) / det,
            (b * e - a * f) / det,
        ]
    })
}

fn degrees(rotation: Rotation) -> u16 {
    match rotation {
        Rotation::None => 0,
        Rotation::Horizontal => 90,
        Rotation::Flipped => 180,
        Rotation::FlippedHorizontal => 270,
    }
}

/// How a source page is laid on the page that replaces it.
#[derive(Debug, PartialEq)]
struct Placement {
    /// Size of the page's own space (the visible area before `/Rotate`), points.
    width: f32,
    height: f32,
    /// `/Rotate` of the new page.
    rotate: u16,
    /// Matrix the copied form is drawn through.
    form: [f32; 6],
}

fn placement(page: &Page<'_>, turn: u16) -> Result<Placement, String> {
    let (width, height) = page.base_dimensions();
    let crop = page.intersected_crop_box();
    // The copy carries `initial_transform` as its matrix: rotation of the original, then the shift
    // that puts the visible area at the origin. Drawing it through the inverse of the rotation
    // alone leaves only the shift, which is the page's own space.
    let baked = page.initial_transform(false).as_coeffs();
    let shift = [1.0, 0.0, 0.0, 1.0, -crop.x0, -crop.y0];
    let undo = invert(baked)
        .map(|inverse| compose(shift, inverse))
        .ok_or_else(|| "PDF 쪽의 변환 행렬을 계산할 수 없습니다.".to_string())?;
    #[allow(clippy::cast_possible_truncation)]
    let form = undo.map(|v| v as f32);
    Ok(Placement {
        width,
        height,
        rotate: (degrees(page.rotation()) + turn) % 360,
        form,
    })
}

/// The overlay's size as seen: the page's own size, swapped when it is shown turned sideways.
fn shown_size(placement: &Placement) -> (f32, f32) {
    if placement.rotate % 180 == 90 {
        (placement.height, placement.width)
    } else {
        (placement.width, placement.height)
    }
}

/// Builds the result PDF. `sources` are the PDFs as they were opened; `pages` the pages to write,
/// in order. Every file is parsed again here and every budget is checked again, so the build does
/// not depend on what was checked when the file was opened.
pub fn build(
    sources: &[Arc<Vec<u8>>],
    pages: &[PagePlan],
    overlay: &Overlay,
    make_overlay: &mut OverlayMaker<'_>,
    deadline: &Deadline,
) -> Result<Vec<u8>, String> {
    if pages.is_empty() {
        return Err("저장할 쪽이 없습니다.".to_string());
    }
    if pages.len() > MAX_PAGES {
        return Err(format!("결과의 쪽 수가 한도({MAX_PAGES})를 초과합니다."));
    }
    let parsed = parse_used(sources, pages, deadline)?;
    let placements = place_all(&parsed, pages, deadline)?;

    let mut next = Ref::new(1);
    let (catalog_ref, tree_ref, info_ref) = (next.bump(), next.bump(), next.bump());
    let mut out = Pdf::new();
    let forms = copy_pages(&mut out, &mut next, &parsed, pages, deadline)?;
    let overlays = copy_overlays(
        &mut out,
        &mut next,
        overlay,
        &placements,
        make_overlay,
        deadline,
    )?;

    let mut page_refs = Vec::with_capacity(pages.len());
    for (n, (plan, place)) in pages.iter().zip(&placements).enumerate() {
        if n & PAGES_PER_CHECK_MASK == 0 {
            deadline.check()?;
        }
        let form = *forms
            .get(&(plan.source, plan.index))
            .ok_or("쪽 복사 결과가 없습니다.")?;
        let shown = shown_size(place);
        let overlay_form = overlays
            .get(&overlay_key(shown, n == 0 && overlay.stamp.is_some()))
            .copied();
        let (page_ref, content_ref) = (next.bump(), next.bump());
        page_refs.push(page_ref);
        let mut content = Content::new();
        content
            .save_state()
            .transform(place.form)
            .x_object(ORIGINAL_FORM)
            .restore_state();
        if overlay_form.is_some() {
            content
                .save_state()
                .transform(upright_matrix(place.rotate, place.width, place.height))
                .x_object(OVERLAY_FORM)
                .restore_state();
        }
        out.stream(content_ref, &content.finish());
        let mut page = out.page(page_ref);
        page.media_box(Rect::new(0.0, 0.0, place.width, place.height))
            .parent(tree_ref)
            .contents(content_ref)
            .rotate(i32::from(place.rotate));
        let mut resources = page.resources();
        let mut x_objects = resources.x_objects();
        x_objects.pair(ORIGINAL_FORM, form);
        if let Some(overlay_form) = overlay_form {
            x_objects.pair(OVERLAY_FORM, overlay_form);
        }
        x_objects.finish();
        resources.finish();
        page.finish();
    }

    out.catalog(catalog_ref).pages(tree_ref);
    let count = i32::try_from(page_refs.len()).map_err(|_| "쪽 수가 너무 많습니다.")?;
    out.pages(tree_ref).count(count).kids(page_refs);
    write_info(&mut out, info_ref, pages, &parsed);
    let bytes = out.finish();
    if bytes.len() > MAX_OUTPUT_BYTES {
        return Err(MSG_TOO_BIG.to_string());
    }
    Ok(bytes)
}

/// The sources the pages come from, parsed (a source nobody uses is not read).
fn parse_used(
    sources: &[Arc<Vec<u8>>],
    pages: &[PagePlan],
    deadline: &Deadline,
) -> Result<BTreeMap<usize, ReadPdf>, String> {
    let used: BTreeSet<usize> = pages.iter().map(|p| p.source).collect();
    let mut parsed = BTreeMap::new();
    for source in used {
        deadline.check()?;
        let bytes = sources.get(source).ok_or("쪽의 원본 PDF가 없습니다.")?;
        parsed.insert(source, open_pdf(bytes)?);
    }
    Ok(parsed)
}

/// Where each page goes, with the decoded content of the chosen pages counted against the budget
/// before the library decodes any of it.
fn place_all(
    parsed: &BTreeMap<usize, ReadPdf>,
    pages: &[PagePlan],
    deadline: &Deadline,
) -> Result<Vec<Placement>, String> {
    let (mut total, mut entries) = (0usize, 0usize);
    let mut placements = Vec::with_capacity(pages.len());
    for (n, plan) in pages.iter().enumerate() {
        if n & PAGES_PER_CHECK_MASK == 0 {
            deadline.check()?;
        }
        let pdf = parsed
            .get(&plan.source)
            .ok_or("쪽의 원본 PDF가 없습니다.")?;
        let page = pdf.pages().get(plan.index).ok_or("없는 쪽을 가리킵니다.")?;
        entries = entries.saturating_add(resource_entries(
            page,
            Limits::DEFAULT.resource_entries - entries.min(Limits::DEFAULT.resource_entries),
        )?);
        total = total.saturating_add(page_content_bytes(page, &Limits::DEFAULT, deadline)?);
        if total > MAX_TOTAL_CONTENT_BYTES {
            return Err(format!(
                "쪽 내용의 압축 해제 크기가 한도({}MiB)를 초과합니다.",
                MAX_TOTAL_CONTENT_BYTES / 1_048_576
            ));
        }
        placements.push(placement(page, plan.turn)?);
    }
    Ok(placements)
}

/// Copies the chosen pages of each source into `out`, one call per source so that objects the
/// pages share (a font used on every page) are copied once. Returns the form of each
/// (source, page).
fn copy_pages(
    out: &mut Pdf,
    next: &mut Ref,
    parsed: &BTreeMap<usize, ReadPdf>,
    pages: &[PagePlan],
    deadline: &Deadline,
) -> Result<HashMap<(usize, usize), Ref>, String> {
    let mut forms = HashMap::new();
    for (&source, pdf) in parsed {
        deadline.check()?;
        let indexes: BTreeSet<usize> = pages
            .iter()
            .filter(|p| p.source == source)
            .map(|p| p.index)
            .collect();
        let queries: Vec<ExtractionQuery> = indexes
            .iter()
            .map(|&i| ExtractionQuery::new_xobject(i))
            .collect();
        let copied = extract(
            pdf,
            Box::new(|| next.bump()),
            ChunkSettings::default(),
            &queries,
        )
        .map_err(|_| "쪽을 복사하지 못했습니다.".to_string())?;
        out.extend(&copied.chunk);
        for (&index, root) in indexes.iter().zip(copied.root_refs) {
            let form = root.map_err(|_| "쪽을 복사하지 못했습니다.".to_string())?;
            forms.insert((source, index), form);
        }
    }
    Ok(forms)
}

/// Key of an overlay: the shown size (to a hundredth of a point) and whether it has the stamp (only
/// the first page does).
type OverlayKey = (u32, u32, bool);

fn overlay_key(shown: (f32, f32), first: bool) -> OverlayKey {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let hundredths = |v: f32| (v * 100.0).round() as u32;
    (hundredths(shown.0), hundredths(shown.1), first)
}

/// One overlay form per distinct (shown size, first page) of the pages, made from SVG by the export
/// pipeline and copied into `out`.
fn copy_overlays(
    out: &mut Pdf,
    next: &mut Ref,
    overlay: &Overlay,
    placements: &[Placement],
    make_overlay: &mut OverlayMaker<'_>,
    deadline: &Deadline,
) -> Result<HashMap<OverlayKey, Ref>, String> {
    let mut forms = HashMap::new();
    if overlay.is_empty() {
        return Ok(forms);
    }
    for (n, place) in placements.iter().enumerate() {
        let shown = shown_size(place);
        let key = overlay_key(shown, n == 0 && overlay.stamp.is_some());
        if forms.contains_key(&key) {
            continue;
        }
        deadline.check()?;
        if forms.len() >= MAX_OVERLAY_SIZES {
            return Err(format!(
                "쪽 크기가 {MAX_OVERLAY_SIZES}가지를 넘어 스탬프와 워터마크를 넣을 수 없습니다."
            ));
        }
        if overlay.svg(shown.0, shown.1, n == 0).is_none() {
            continue;
        }
        let bytes = Arc::new(make_overlay(shown.0, shown.1, n == 0)?);
        let pdf = open_pdf(&bytes)?;
        let copied = extract(
            &pdf,
            Box::new(|| next.bump()),
            ChunkSettings::default(),
            &[ExtractionQuery::new_xobject(0)],
        )
        .map_err(|_| "스탬프를 복사하지 못했습니다.".to_string())?;
        out.extend(&copied.chunk);
        let form = copied
            .root_refs
            .into_iter()
            .next()
            .and_then(Result::ok)
            .ok_or("스탬프를 복사하지 못했습니다.")?;
        forms.insert(key, form);
    }
    Ok(forms)
}

/// A key of the document information and the bytes the source has for it.
type InfoField<'a> = (&'a [u8], &'a Option<Box<[u8]>>);

/// The document information: the producer, and the title, author, subject and keywords of the
/// source of the first page (their bytes are copied as they are, whatever their encoding).
fn write_info(out: &mut Pdf, info_ref: Ref, pages: &[PagePlan], parsed: &BTreeMap<usize, ReadPdf>) {
    let meta = pages
        .first()
        .and_then(|p| parsed.get(&p.source))
        .map(ReadPdf::metadata);
    let mut info = out.document_info(info_ref);
    info.producer(TextStr(PRODUCER));
    if let Some(meta) = meta {
        let fields: [InfoField<'_>; 4] = [
            (b"Title", &meta.title),
            (b"Author", &meta.author),
            (b"Subject", &meta.subject),
            (b"Keywords", &meta.keywords),
        ];
        for (key, value) in fields {
            if let Some(value) = value {
                info.pair(Name(key), Str(value));
            }
        }
    }
    info.finish();
}
