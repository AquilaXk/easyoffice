//! Spreadsheet floating image XObjects and cell note indicators for PDF output (`PD-256`).

use std::collections::HashMap;
use std::fmt::Write;

use crate::pdf_image::PdfImageObject;
use crate::pdf_laid::{fit_text, width_pt, Canvas, DEFAULT_COL_PX, PDF_PT_PER_PX, ROW_PX};
use crate::pdf_writer::{pdf_text_string, PdfLaidPage, PdfPageEntry};
use crate::sheet_styling::sheet_chart::{SheetCellIndex, EMU_PER_PX};
use crate::sheet_styling::sheet_media_notes::effective_image_two_cell;
use crate::{pack_cell_pos, OdsUnpacked, PdfSerializer, SheetStyling};

const KOREAN_CID_FONT: &str = "HYGoThic-Medium";
const KOREAN_CMAP: &str = "UniKS-UCS2-H";
const TO_UNICODE_CMAP: &str = "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n1 beginbfrange\n<0000> <FFFF> <0000>\nendbfrange\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend";

const NOTE_TRIANGLE_RGB: &str = "0.82 0.14 0.14";
const FALLBACK_BG_RGB: u32 = 0xF8FAFC;
const FALLBACK_BORDER_RGB: u32 = 0x94A3B8;
const FALLBACK_TEXT_RGB: u32 = 0x334155;

pub(crate) struct SheetPdfPage {
    pub laid: PdfLaidPage,
    pub xobjects: Vec<(String, PdfImageObject)>,
}

fn emu_to_pt(emu: u32, scale: f64) -> f64 {
    (f64::from(emu) / EMU_PER_PX as f64) * PDF_PT_PER_PX * scale
}

fn col_width_pt(styling: &SheetStyling, col: u32, scale: f64) -> f64 {
    if styling
        .hidden_cols
        .iter()
        .any(|&(a, b)| a <= col && col <= b)
    {
        return 0.0;
    }
    styling
        .columns
        .get(&col)
        .map_or(DEFAULT_COL_PX, |px| f64::from(*px))
        * PDF_PT_PER_PX
        * scale
}

fn row_height_pt(styling: &SheetStyling, row: u32, scale: f64) -> f64 {
    if styling.row_hidden(row) {
        0.0
    } else {
        ROW_PX * PDF_PT_PER_PX * scale
    }
}

fn x_at_col(
    col: u32,
    page_cols: &[u32],
    col_xs: &[f64],
    styling: &SheetStyling,
    scale: f64,
) -> f64 {
    if let Some(pos) = page_cols.iter().position(|&c| c == col) {
        return col_xs[pos];
    }
    let first = page_cols[0];
    if col < first {
        let mut x = col_xs[0];
        let mut c = col;
        while c < first {
            x -= col_width_pt(styling, c, scale);
            c += 1;
        }
        return x;
    }
    let last = *page_cols.last().unwrap_or(&first);
    let mut x = *col_xs.last().unwrap_or(&col_xs[0]);
    let mut c = last + 1;
    while c < col {
        x += col_width_pt(styling, c, scale);
        c += 1;
    }
    x
}

fn y_at_row(
    row: u32,
    page_rows: &[u32],
    row_ys: &[f64],
    styling: &SheetStyling,
    scale: f64,
) -> f64 {
    if let Some(pos) = page_rows.iter().position(|&r| r == row) {
        return row_ys[pos];
    }
    let first = page_rows[0];
    if row < first {
        let mut y = row_ys[0];
        let mut r = row;
        while r < first {
            y += row_height_pt(styling, r, scale);
            r += 1;
        }
        return y;
    }
    let last = *page_rows.last().unwrap_or(&first);
    let mut y = *row_ys.last().unwrap_or(&row_ys[0]);
    let mut r = last + 1;
    while r < row {
        y -= row_height_pt(styling, r, scale);
        r += 1;
    }
    y
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_sheet_notes_indicators(
    canvas: &mut Canvas,
    styling: &SheetStyling,
    page_cols: &[u32],
    page_rows: &[u32],
    col_xs: &[f64],
    row_ys: &[f64],
    scale: f64,
) {
    if styling.notes.is_empty()
        || page_cols.is_empty()
        || page_rows.is_empty()
        || col_xs.len() != page_cols.len() + 1
        || row_ys.len() != page_rows.len() + 1
    {
        return;
    }
    let min_col = page_cols.iter().copied().min().unwrap_or(0);
    let max_col = page_cols.iter().copied().max().unwrap_or(0);
    let mut col_lookup: HashMap<u32, usize> = HashMap::with_capacity(page_cols.len());
    for (idx, &c) in page_cols.iter().enumerate() {
        col_lookup.entry(c).or_insert(idx);
    }

    for (r_idx, &r) in page_rows.iter().enumerate() {
        let start = pack_cell_pos(r, min_col);
        let end = pack_cell_pos(r, max_col);
        for (&packed, _) in styling.notes.range(start..=end) {
            let c = (packed & 0xFFFF_FFFF) as u32;
            let Some(&c_idx) = col_lookup.get(&c) else {
                continue;
            };
            let right = col_xs[c_idx + 1];
            let left = col_xs[c_idx];
            let top = row_ys[r_idx];
            let bot = row_ys[r_idx + 1];
            let tri = (4.5 * scale)
                .min((right - left) * 0.45)
                .min((top - bot) * 0.45);
            if tri >= 1.0 {
                canvas.push_raw_op(format!(
                    "q {NOTE_TRIANGLE_RGB} rg {right:.2} {top:.2} m {:.2} {top:.2} l {right:.2} {:.2} l h f Q",
                    right - tri,
                    top - tri
                ));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_sheet_floating_images(
    canvas: &mut Canvas,
    styling: &SheetStyling,
    page_cols: &[u32],
    page_rows: &[u32],
    col_xs: &[f64],
    row_ys: &[f64],
    scale: f64,
) -> Vec<(String, PdfImageObject)> {
    let mut xobjects = Vec::new();
    if styling.images.is_empty()
        || page_cols.is_empty()
        || page_rows.is_empty()
        || col_xs.len() != page_cols.len() + 1
        || row_ys.len() != page_rows.len() + 1
    {
        return xobjects;
    }
    let min_col = page_cols.iter().copied().min().unwrap_or(0);
    let max_col = page_cols.iter().copied().max().unwrap_or(0);
    let min_row = page_rows.iter().copied().min().unwrap_or(0);
    let max_row = page_rows.iter().copied().max().unwrap_or(0);

    for img in &styling.images {
        let (fc, fr, fce, fre, tc, tr, tce, tre) =
            effective_image_two_cell(&img.anchor, &styling.columns);
        if fc > max_col || tc < min_col || fr > max_row || tr < min_row {
            continue;
        }
        let x_left = x_at_col(fc, page_cols, col_xs, styling, scale) + emu_to_pt(fce, scale);
        let y_top = y_at_row(fr, page_rows, row_ys, styling, scale) - emu_to_pt(fre, scale);
        let x_right = x_at_col(tc, page_cols, col_xs, styling, scale) + emu_to_pt(tce, scale);
        let y_bot_grid = y_at_row(tr, page_rows, row_ys, styling, scale) - emu_to_pt(tre, scale);

        let fallback_w = f64::from(img.anchor.width_px) * PDF_PT_PER_PX * scale;
        let fallback_h = f64::from(img.anchor.height_px) * PDF_PT_PER_PX * scale;
        let w = if img.anchor.has_explicit_grid() && x_right > x_left + scale {
            x_right - x_left
        } else {
            fallback_w.max(4.0 * scale)
        };
        let h = if img.anchor.has_explicit_grid() && y_top > y_bot_grid + scale {
            y_top - y_bot_grid
        } else {
            fallback_h.max(4.0 * scale)
        };
        let y_bottom = y_top - h;

        if let Some(pdf_obj) = crate::pdf_image::pdf_image_object(&img.image) {
            let res_name = format!("Im{}", xobjects.len() + 1);
            canvas.push_raw_op(format!(
                "q {w:.2} 0 0 {h:.2} {x_left:.2} {y_bottom:.2} cm /{res_name} Do Q"
            ));
            xobjects.push((res_name, pdf_obj));
        } else {
            canvas.fill_rect(x_left, y_bottom, w, h, FALLBACK_BG_RGB);
            canvas.stroke_rect(x_left, y_bottom, w, h, FALLBACK_BORDER_RGB, 0.75 * scale);
            let label = if img.alt.is_empty() {
                &img.name
            } else {
                &img.alt
            };
            let font_sz = (9.0 * scale).clamp(5.0, h * 0.45);
            let fitted = fit_text(label, (w - 6.0 * scale).max(4.0), font_sz);
            if !fitted.is_empty() {
                let lw = width_pt(&fitted, font_sz);
                let tx = x_left + ((w - lw) / 2.0).max(3.0 * scale);
                let ty = y_bottom + (h - font_sz) / 2.0;
                canvas.colored_text(tx, ty, font_sz, &fitted, FALLBACK_TEXT_RGB);
            }
        }
    }
    xobjects
}

fn expand_bounds(
    used: Option<(u32, u32, u32, u32)>,
    (r1, c1, r2, c2): (u32, u32, u32, u32),
    max_cells: u64,
) -> Option<(u32, u32, u32, u32)> {
    let next = used.map_or((r1, c1, r2, c2), |(ar1, ac1, ar2, ac2)| {
        (ar1.min(r1), ac1.min(c1), ar2.max(r2), ac2.max(c2))
    });
    let rows = u64::from(next.2.saturating_sub(next.0)).saturating_add(1);
    let cols = u64::from(next.3.saturating_sub(next.1)).saturating_add(1);
    if rows.saturating_mul(cols) <= max_cells {
        Some(next)
    } else {
        used
    }
}

pub(crate) fn extend_used_bounds(
    mut used: Option<(u32, u32, u32, u32)>,
    styling: &SheetStyling,
    max_cells: u64,
) -> Option<(u32, u32, u32, u32)> {
    for ch in &styling.charts {
        let (fc, fr, _, _, tc, tr, _, _) = ch.anchor.effective_two_cell(&styling.columns);
        used = expand_bounds(used, (fr, fc, tr, tc), max_cells);
    }
    for img in &styling.images {
        let (fc, fr, _, _, tc, tr, _, _) = effective_image_two_cell(&img.anchor, &styling.columns);
        used = expand_bounds(used, (fr, fc, tr, tc), max_cells);
    }
    for &packed in styling.notes.keys() {
        let r = (packed >> 32) as u32;
        let c = (packed & 0xFFFF_FFFF) as u32;
        used = expand_bounds(used, (r, c, r, c), max_cells);
    }
    used
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_sheet_overlays(
    canvas: &mut Canvas,
    sheet_name: &str,
    sheets: &[OdsUnpacked],
    cell_index: &SheetCellIndex<'_>,
    styling: &SheetStyling,
    page_cols: &[u32],
    page_rows: &[u32],
    xs: &[f64],
    ys: &[f64],
    head_w: f64,
    head_h: f64,
    scale: f64,
) -> Vec<(String, PdfImageObject)> {
    let col_xs = if head_w > 0.0 && xs.len() > 1 {
        &xs[1..]
    } else {
        xs
    };
    let row_ys = if head_h > 0.0 && ys.len() > 1 {
        &ys[1..]
    } else {
        ys
    };
    draw_sheet_notes_indicators(canvas, styling, page_cols, page_rows, col_xs, row_ys, scale);
    if !styling.charts.is_empty() {
        crate::pdf_chart::draw_sheet_charts(
            canvas, sheet_name, sheets, cell_index, styling, page_cols, page_rows, col_xs, row_ys,
            scale,
        );
    }
    draw_sheet_floating_images(canvas, styling, page_cols, page_rows, col_xs, row_ys, scale)
}

struct SheetPdfPlan {
    page_obj: usize,
    content_obj: usize,
    image_objs: Vec<(String, usize, Option<usize>)>,
}

struct SheetPdfWriter {
    out: Vec<u8>,
    offsets: Vec<usize>,
}

impl SheetPdfWriter {
    fn line(&mut self, line: &str) {
        self.out.extend_from_slice(line.as_bytes());
        self.out.push(b'\n');
    }

    fn object(&mut self, id: usize, body: &str) {
        self.offsets[id] = self.out.len();
        self.line(&format!("{id} 0 obj"));
        self.line(body);
        self.line("endobj");
    }

    fn stream_bytes(&mut self, id: usize, dict_extra: &str, data: &[u8]) {
        self.offsets[id] = self.out.len();
        self.line(&format!("{id} 0 obj"));
        self.line(&format!("<<{dict_extra} /Length {} >>", data.len()));
        self.line("stream");
        self.out.extend_from_slice(data);
        self.out.push(b'\n');
        self.line("endstream");
        self.line("endobj");
    }
}

pub(crate) fn serialize_sheet_pdf(title: &str, pages: Vec<SheetPdfPage>) -> Vec<u8> {
    if pages.iter().all(|p| p.xobjects.is_empty()) {
        let entries: Vec<PdfPageEntry> = pages
            .into_iter()
            .map(|p| PdfPageEntry::Laid(p.laid))
            .collect();
        return PdfSerializer::serialize_entries(title, None, &entries);
    }

    let mut next_id = 3usize;
    let mut plans = Vec::with_capacity(pages.len());
    let mut kids_refs = String::new();
    for (idx, page) in pages.iter().enumerate() {
        let page_obj = next_id;
        let content_obj = next_id + 1;
        next_id += 2;
        if idx > 0 {
            kids_refs.push(' ');
        }
        let _ = write!(kids_refs, "{page_obj} 0 R");
        let mut image_objs = Vec::with_capacity(page.xobjects.len());
        for (name, obj) in &page.xobjects {
            let img_id = next_id;
            next_id += 1;
            let mask_id = if obj.soft_mask.is_some() {
                let id = next_id;
                next_id += 1;
                Some(id)
            } else {
                None
            };
            image_objs.push((name.clone(), img_id, mask_id));
        }
        plans.push(SheetPdfPlan {
            page_obj,
            content_obj,
            image_objs,
        });
    }

    let latin_font_obj = next_id;
    let korean_font_obj = next_id + 1;
    let cid_font_obj = next_id + 2;
    let descriptor_obj = next_id + 3;
    let cmap_obj = next_id + 4;
    let info_obj = next_id + 5;
    let total_objects = next_id + 6;

    let mut w = SheetPdfWriter {
        out: Vec::with_capacity(8192),
        offsets: vec![0; total_objects],
    };
    w.line("%PDF-1.4");
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(
        2,
        &format!(
            "<< /Type /Pages /Count {} /Kids [{kids_refs}] >>",
            pages.len()
        ),
    );

    let fonts = format!("/Font << /F1 {latin_font_obj} 0 R /F2 {korean_font_obj} 0 R >>");
    for (page, plan) in pages.iter().zip(&plans) {
        let xobj_dict = if plan.image_objs.is_empty() {
            String::new()
        } else {
            let mut s = String::from(" /XObject <<");
            for (name, img_id, _) in &plan.image_objs {
                let _ = write!(s, " /{name} {img_id} 0 R");
            }
            s.push_str(" >>");
            s
        };
        let laid = &page.laid;
        w.object(
            plan.page_obj,
            &format!(
                "<< /Type /Page /Parent 2 0 R /PageNumber {} /Rotate {} /MediaBox [0 0 {:.2} {:.2}] /Resources << {fonts}{xobj_dict} >> /Contents {} 0 R >>",
                laid.page_number,
                laid.rotation_degrees,
                laid.width_pt,
                laid.height_pt,
                plan.content_obj
            ),
        );
        w.stream_bytes(plan.content_obj, "", laid.content.as_bytes());
        for ((_, obj), &(_, img_id, mask_id)) in page.xobjects.iter().zip(&plan.image_objs) {
            let mut dict = obj.dict.clone();
            if let Some(mask) = mask_id {
                let _ = write!(dict, " /SMask {mask} 0 R");
            }
            w.stream_bytes(img_id, &dict, &obj.data);
            if let (Some(mask), Some(soft)) = (mask_id, obj.soft_mask.as_deref()) {
                w.stream_bytes(mask, &soft.dict, &soft.data);
            }
        }
    }

    w.object(
        latin_font_obj,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    );
    w.object(
        korean_font_obj,
        &format!(
            "<< /Type /Font /Subtype /Type0 /BaseFont /{KOREAN_CID_FONT}-{KOREAN_CMAP} /Encoding /{KOREAN_CMAP} /DescendantFonts [{cid_font_obj} 0 R] /ToUnicode {cmap_obj} 0 R >>"
        ),
    );
    w.object(
        cid_font_obj,
        &format!(
            "<< /Type /Font /Subtype /CIDFontType0 /BaseFont /{KOREAN_CID_FONT} /CIDSystemInfo << /Registry (Adobe) /Ordering (Korea1) /Supplement 1 >> /FontDescriptor {descriptor_obj} 0 R /DW 1000 /W {} >>",
            crate::pdf_text_wrap::cid_ascii_widths()
        ),
    );
    w.object(
        descriptor_obj,
        &format!(
            "<< /Type /FontDescriptor /FontName /{KOREAN_CID_FONT} /Flags 4 /FontBBox [-6 -145 1003 880] /ItalicAngle 0 /Ascent 880 /Descent -120 /CapHeight 880 /StemV 93 >>"
        ),
    );
    w.stream_bytes(
        cmap_obj,
        " /Type /CMap /CMapName /Adobe-Identity-UCS",
        TO_UNICODE_CMAP.as_bytes(),
    );
    w.object(
        info_obj,
        &format!(
            "<< /Title {} /Producer (EasyOffice) >>",
            pdf_text_string(title)
        ),
    );

    let xref_offset = w.out.len();
    w.line("xref");
    w.line(&format!("0 {total_objects}"));
    w.line("0000000000 65535 f ");
    let offsets = std::mem::take(&mut w.offsets);
    for offset in offsets.iter().skip(1) {
        w.line(&format!("{offset:010} 00000 n "));
    }
    w.line("trailer");
    w.line(&format!(
        "<< /Size {total_objects} /Root 1 0 R /Info {info_obj} 0 R >>"
    ));
    w.line("startxref");
    w.line(&xref_offset.to_string());
    w.line("%%EOF");
    w.out
}
