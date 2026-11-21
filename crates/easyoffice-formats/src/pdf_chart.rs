//! Pure vector PDF rendering for spreadsheet charts (`PD-254`).
//!
//! Draws chart cards, titles, gridlines, Heckbert nice-number axis ticks, legends,
//! and all five chart types (`column`, `bar`, `line`, `pie`, `scatter`) using PDF path
//! operators (`m`, `l`, `c`, `h`, `f`, `S`, `B`, `re`) with zero rasterization.

use std::fmt::Write;

use crate::pdf_laid::{
    fit_text, rgb_ops, row_height_pt, width_pt, Canvas, DEFAULT_COL_PX, PDF_PT_PER_PX,
};
use crate::sheet_styling::sheet_chart::{
    parse_chart_number, ChartKind, ChartLegendPos, ResolvedSeriesData, SheetCellIndex, SheetChart,
    EMU_PER_PX, MAX_SERIES_PER_CHART,
};
use crate::{OdsUnpacked, SheetStyling};

const PALETTE: [u32; 8] = [
    0x4472C4, 0xED7D31, 0xA5A5A5, 0xFFC000, 0x5B9BD5, 0x70AD47, 0x264478, 0x9E480E,
];
const CARD_BG_RGB: u32 = 0xFFFFFF;
const CARD_BORDER_RGB: u32 = 0xD0D7DE;
const TITLE_RGB: u32 = 0x1F2328;
const AXIS_RGB: u32 = 0x6B7280;
const GRID_RGB: u32 = 0xE5E7EB;
const LABEL_RGB: u32 = 0x4B5563;

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
pub(crate) fn draw_sheet_charts(
    canvas: &mut Canvas,
    sheet_name: &str,
    sheets: &[OdsUnpacked],
    cell_index: &SheetCellIndex<'_>,
    styling: &SheetStyling,
    page_cols: &[u32],
    page_rows: &[u32],
    col_xs: &[f64],
    row_ys: &[f64],
    scale: f64,
) {
    if page_cols.is_empty()
        || page_rows.is_empty()
        || col_xs.len() != page_cols.len() + 1
        || row_ys.len() != page_rows.len() + 1
    {
        return;
    }
    let min_col = page_cols.iter().copied().min().unwrap_or(0);
    let max_col = page_cols.iter().copied().max().unwrap_or(0);
    let min_row = page_rows.iter().copied().min().unwrap_or(0);
    let max_row = page_rows.iter().copied().max().unwrap_or(0);

    for chart in &styling.charts {
        let (fc, fr, fce, fre, tc, tr, tce, tre) =
            chart.anchor.effective_two_cell(&styling.columns);
        if fc > max_col || tc < min_col || fr > max_row || tr < min_row {
            continue;
        }
        let x_left = x_at_col(fc, page_cols, col_xs, styling, scale) + emu_to_pt(fce, scale);
        let y_top = y_at_row(fr, page_rows, row_ys, styling, scale) - emu_to_pt(fre, scale);
        let x_right = x_at_col(tc, page_cols, col_xs, styling, scale) + emu_to_pt(tce, scale);
        let y_bot_grid = y_at_row(tr, page_rows, row_ys, styling, scale) - emu_to_pt(tre, scale);

        let fallback_w = f64::from(chart.anchor.width_px) * PDF_PT_PER_PX * scale;
        let fallback_h = f64::from(chart.anchor.height_px) * PDF_PT_PER_PX * scale;
        let w = if chart.anchor.has_explicit_grid() && x_right > x_left + 40.0 * scale {
            x_right - x_left
        } else {
            fallback_w.max(80.0 * scale)
        };
        let h = if chart.anchor.has_explicit_grid() && y_top > y_bot_grid + 40.0 * scale {
            y_top - y_bot_grid
        } else {
            fallback_h.max(60.0 * scale)
        };
        let y_bottom = y_top - h;
        draw_single_chart(
            canvas,
            chart,
            sheet_name,
            sheets,
            cell_index,
            (x_left, y_bottom, w, h),
            scale,
        );
    }
}

fn draw_single_chart(
    canvas: &mut Canvas,
    chart: &SheetChart,
    sheet_name: &str,
    sheets: &[OdsUnpacked],
    cell_index: &SheetCellIndex<'_>,
    (x, y, w, h): (f64, f64, f64, f64),
    scale: f64,
) {
    let raw_series = chart.resolved_series(sheet_name, sheets);
    let series: Vec<ResolvedSeriesData> = raw_series
        .iter()
        .take(MAX_SERIES_PER_CHART)
        .map(|s| cell_index.resolve_series(sheet_name, &[], s))
        .collect();

    canvas.push_clip_rect(x - 1.0, y - 1.0, w + 2.0, h + 2.0);
    canvas.fill_rect(x, y, w, h, CARD_BG_RGB);
    canvas.stroke_rect(x, y, w, h, CARD_BORDER_RGB, 0.75 * scale);

    let pad = (10.0 * scale).clamp(4.0, 14.0);
    let mut top_y = y + h - pad;
    let bottom_y = y + pad;
    let mut left_x = x + pad;
    let mut right_x = x + w - pad;

    if !chart.title.trim().is_empty() {
        let title_pt = (10.5 * scale).clamp(6.0, 15.0);
        let fitted = fit_text(chart.title.trim(), title_pt, (w - 2.0 * pad).max(10.0));
        let tw = width_pt(&fitted, title_pt);
        let tx = x + (w - tw) / 2.0;
        let ty = top_y - title_pt;
        canvas.colored_text(tx, ty, title_pt, &fitted, TITLE_RGB);
        top_y -= title_pt + 6.0 * scale;
    }

    let legend_items = collect_legend_items(chart.kind, &series);
    let mut plot_bottom = bottom_y;
    if chart.legend != ChartLegendPos::None && !legend_items.is_empty() {
        let leg_pt = (7.0 * scale).clamp(5.0, 10.0);
        match chart.legend {
            ChartLegendPos::Top => {
                let used = draw_horizontal_legend(
                    canvas,
                    &legend_items,
                    left_x,
                    top_y - leg_pt,
                    (right_x - left_x).max(20.0),
                    leg_pt,
                    scale,
                );
                top_y -= used + 4.0 * scale;
            }
            ChartLegendPos::Bottom => {
                let used = draw_horizontal_legend(
                    canvas,
                    &legend_items,
                    left_x,
                    plot_bottom + 2.0 * scale,
                    (right_x - left_x).max(20.0),
                    leg_pt,
                    scale,
                );
                plot_bottom += used + 4.0 * scale;
            }
            ChartLegendPos::Left => {
                let leg_w = (w * 0.26).clamp(36.0 * scale, 110.0 * scale);
                draw_vertical_legend(canvas, &legend_items, left_x, top_y, leg_w, leg_pt, scale);
                left_x += leg_w + 4.0 * scale;
            }
            ChartLegendPos::Right => {
                let leg_w = (w * 0.26).clamp(36.0 * scale, 110.0 * scale);
                right_x -= leg_w;
                draw_vertical_legend(canvas, &legend_items, right_x, top_y, leg_w, leg_pt, scale);
                right_x -= 4.0 * scale;
            }
            ChartLegendPos::None => {}
        }
    }

    let plot_w = (right_x - left_x).max(12.0);
    let plot_h = (top_y - plot_bottom).max(12.0);
    let label_pt = (6.8 * scale).clamp(4.5, 9.5);

    match chart.kind {
        ChartKind::Column => draw_column_chart(
            canvas,
            &series,
            (left_x, plot_bottom, plot_w, plot_h),
            label_pt,
            scale,
        ),
        ChartKind::Bar => draw_bar_chart(
            canvas,
            &series,
            (left_x, plot_bottom, plot_w, plot_h),
            label_pt,
            scale,
        ),
        ChartKind::Line => draw_line_chart(
            canvas,
            &series,
            (left_x, plot_bottom, plot_w, plot_h),
            label_pt,
            scale,
        ),
        ChartKind::Pie => draw_pie_chart(
            canvas,
            &series,
            (left_x, plot_bottom, plot_w, plot_h),
            scale,
        ),
        ChartKind::Scatter => draw_scatter_chart(
            canvas,
            &series,
            (left_x, plot_bottom, plot_w, plot_h),
            label_pt,
            scale,
        ),
    }

    canvas.pop_clip();
}

fn collect_legend_items(kind: ChartKind, series: &[ResolvedSeriesData]) -> Vec<(String, u32)> {
    if kind == ChartKind::Pie {
        let Some(first) = series.first() else {
            return Vec::new();
        };
        let count = first.values.len().max(first.categories.len()).min(8);
        return (0..count)
            .map(|i| {
                let label = first
                    .categories
                    .get(i)
                    .filter(|s| !s.trim().is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("{}", i + 1));
                (label, PALETTE[i % PALETTE.len()])
            })
            .collect();
    }
    series
        .iter()
        .take(8)
        .enumerate()
        .map(|(i, s)| {
            let label = if s.name.trim().is_empty() {
                format!("Series {}", i + 1)
            } else {
                s.name.clone()
            };
            (label, PALETTE[i % PALETTE.len()])
        })
        .collect()
}

fn draw_horizontal_legend(
    canvas: &mut Canvas,
    items: &[(String, u32)],
    start_x: f64,
    y: f64,
    max_w: f64,
    font_pt: f64,
    scale: f64,
) -> f64 {
    let sw = font_pt * 0.9;
    let gap = 8.0 * scale;
    let total_w: f64 = items
        .iter()
        .map(|(lbl, _)| sw + 3.0 * scale + width_pt(&fit_text(lbl, font_pt, 60.0 * scale), font_pt))
        .sum::<f64>()
        + gap * items.len().saturating_sub(1) as f64;
    let mut cx = start_x + ((max_w - total_w.min(max_w)) / 2.0).max(0.0);
    let end_x = start_x + max_w;
    for (lbl, color) in items {
        let text = fit_text(lbl, font_pt, 60.0 * scale);
        let item_w = sw + 3.0 * scale + width_pt(&text, font_pt);
        if cx + item_w > end_x + 1.0 {
            break;
        }
        canvas.fill_rect(cx, y + font_pt * 0.1, sw, sw, *color);
        canvas.colored_text(cx + sw + 3.0 * scale, y, font_pt, &text, LABEL_RGB);
        cx += item_w + gap;
    }
    font_pt + 3.0 * scale
}

fn draw_vertical_legend(
    canvas: &mut Canvas,
    items: &[(String, u32)],
    x: f64,
    top_y: f64,
    max_w: f64,
    font_pt: f64,
    scale: f64,
) {
    let sw = font_pt * 0.9;
    let line_h = font_pt + 4.0 * scale;
    let mut cy = top_y - font_pt;
    for (lbl, color) in items {
        let text = fit_text(lbl, font_pt, (max_w - sw - 5.0 * scale).max(8.0));
        canvas.fill_rect(x, cy + font_pt * 0.1, sw, sw, *color);
        canvas.colored_text(x + sw + 3.5 * scale, cy, font_pt, &text, LABEL_RGB);
        cy -= line_h;
    }
}

struct NiceScale {
    min: f64,
    max: f64,
    ticks: Vec<f64>,
}

fn nice_num(range: f64, round: bool) -> f64 {
    if !range.is_finite() || range <= 0.0 {
        return 1.0;
    }
    let exp = range.log10().floor();
    let pow10 = 10.0_f64.powf(exp);
    let frac = range / pow10;
    let nice = if round {
        if frac < 1.5 {
            1.0
        } else if frac < 3.0 {
            2.0
        } else if frac < 7.0 {
            5.0
        } else {
            10.0
        }
    } else if frac <= 1.0 {
        1.0
    } else if frac <= 2.0 {
        2.0
    } else if frac <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * pow10
}

fn build_nice_scale(raw_min: f64, raw_max: f64, max_ticks: usize) -> NiceScale {
    let (mut lo, mut hi) = if raw_min <= raw_max {
        (raw_min, raw_max)
    } else {
        (raw_max, raw_min)
    };
    if (hi - lo).abs() < 1e-12 {
        if lo.abs() < 1e-12 {
            lo = 0.0;
            hi = 1.0;
        } else if lo > 0.0 {
            lo = 0.0;
        } else {
            hi = 0.0;
        }
    }
    let range = nice_num(hi - lo, false);
    let step = nice_num(range / (max_ticks.saturating_sub(1).max(1) as f64), true);
    let nice_min = (lo / step).floor() * step;
    let nice_max = (hi / step).ceil() * step;
    let mut ticks = Vec::with_capacity(max_ticks + 2);
    let mut cur = nice_min;
    let limit = nice_max + step * 0.25;
    while cur <= limit && ticks.len() < 12 {
        let cleaned = if cur.abs() < step * 1e-9 { 0.0 } else { cur };
        ticks.push(cleaned);
        cur += step;
    }
    NiceScale {
        min: nice_min,
        max: if (nice_max - nice_min).abs() < 1e-12 {
            nice_min + 1.0
        } else {
            nice_max
        },
        ticks,
    }
}

fn format_tick(v: f64) -> String {
    if v.abs() < 1e-9 {
        return "0".to_string();
    }
    if (v - v.round()).abs() < 1e-6 && v.abs() < 1e12 {
        return format!("{}", v.round() as i64);
    }
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn series_bounds(series: &[ResolvedSeriesData], include_zero: bool) -> (f64, f64, usize) {
    let mut lo = if include_zero { 0.0 } else { f64::INFINITY };
    let mut hi = if include_zero { 0.0 } else { f64::NEG_INFINITY };
    let mut n_cats = 0usize;
    for s in series {
        n_cats = n_cats.max(s.values.len()).max(s.categories.len());
        for v in s.values.iter().flatten() {
            if v.is_finite() {
                lo = lo.min(*v);
                hi = hi.max(*v);
            }
        }
    }
    if !lo.is_finite() || !hi.is_finite() {
        (0.0, 10.0, n_cats)
    } else {
        (lo, hi, n_cats)
    }
}

fn category_label(series: &[ResolvedSeriesData], idx: usize) -> String {
    series
        .iter()
        .find_map(|s| s.categories.get(idx).filter(|t| !t.trim().is_empty()))
        .cloned()
        .unwrap_or_else(|| format!("{}", idx + 1))
}

/// Draws Y-axis nice gridlines and tick labels, returning `(px, py, pw, ph, y_span)`.
fn draw_y_grid_and_labels(
    canvas: &mut Canvas,
    y_scale: &NiceScale,
    (x, y, w, h): (f64, f64, f64, f64),
    label_pt: f64,
    scale: f64,
) -> (f64, f64, f64, f64, f64) {
    let y_axis_w = y_scale
        .ticks
        .iter()
        .map(|t| width_pt(&format_tick(*t), label_pt))
        .fold(16.0 * scale, f64::max)
        .min(w * 0.25)
        + 4.0 * scale;
    let x_axis_h = label_pt + 6.0 * scale;
    let px = x + y_axis_w;
    let py = y + x_axis_h;
    let pw = (w - y_axis_w).max(10.0);
    let ph = (h - x_axis_h).max(10.0);
    let span = y_scale.max - y_scale.min;

    for &tick in &y_scale.ticks {
        let ty = py + ((tick - y_scale.min) / span) * ph;
        canvas.stroke_line(px, ty, px + pw, ty, GRID_RGB, 0.45 * scale);
        let lbl = fit_text(&format_tick(tick), label_pt, y_axis_w - 3.0 * scale);
        let lw = width_pt(&lbl, label_pt);
        canvas.colored_text(
            px - lw - 3.0 * scale,
            ty - label_pt * 0.35,
            label_pt,
            &lbl,
            LABEL_RGB,
        );
    }
    (px, py, pw, ph, span)
}

/// Draws evenly spaced X-axis category labels below a plot box.
fn draw_x_cat_labels(
    canvas: &mut Canvas,
    series: &[ResolvedSeriesData],
    n_cats: usize,
    (px, y, pw): (f64, f64, f64),
    label_pt: f64,
    scale: f64,
) {
    let slot_w = pw / n_cats as f64;
    let label_step = ((n_cats as f64 * 28.0 * scale) / pw).ceil().max(1.0) as usize;
    for cat in (0..n_cats).step_by(label_step) {
        let lbl = fit_text(
            &category_label(series, cat),
            label_pt,
            slot_w * label_step as f64,
        );
        let lw = width_pt(&lbl, label_pt);
        let lx = px + (cat as f64 + 0.5) * slot_w - lw / 2.0;
        canvas.colored_text(lx, y + 1.0, label_pt, &lbl, LABEL_RGB);
    }
}

fn draw_column_chart(
    canvas: &mut Canvas,
    series: &[ResolvedSeriesData],
    (x, y, w, h): (f64, f64, f64, f64),
    label_pt: f64,
    scale: f64,
) {
    let (raw_min, raw_max, n_cats) = series_bounds(series, true);
    let y_scale = build_nice_scale(raw_min, raw_max, 5);
    let (px, py, pw, ph, span) =
        draw_y_grid_and_labels(canvas, &y_scale, (x, y, w, h), label_pt, scale);

    let zero_y = py + ((0.0 - y_scale.min) / span).clamp(0.0, 1.0) * ph;
    canvas.stroke_line(px, py, px, py + ph, AXIS_RGB, 0.7 * scale);
    canvas.stroke_line(px, zero_y, px + pw, zero_y, AXIS_RGB, 0.7 * scale);

    if n_cats == 0 || series.is_empty() {
        return;
    }
    let group_w = pw / n_cats as f64;
    let inner_w = group_w * 0.74;
    let bar_w = (inner_w / series.len() as f64).max(1.5);

    for cat in 0..n_cats {
        let gx = px + cat as f64 * group_w + (group_w - inner_w) / 2.0;
        for (si, ser) in series.iter().enumerate() {
            if let Some(Some(val)) = ser.values.get(cat) {
                if val.is_finite() {
                    let vy = py + ((*val - y_scale.min) / span) * ph;
                    let (by, bh) = if vy >= zero_y {
                        (zero_y, (vy - zero_y).max(0.8))
                    } else {
                        (vy, (zero_y - vy).max(0.8))
                    };
                    let bx = gx + si as f64 * bar_w;
                    canvas.fill_rect(
                        bx,
                        by,
                        (bar_w - 0.8).max(1.0),
                        bh,
                        PALETTE[si % PALETTE.len()],
                    );
                }
            }
        }
    }
    draw_x_cat_labels(canvas, series, n_cats, (px, y, pw), label_pt, scale);
}

fn draw_bar_chart(
    canvas: &mut Canvas,
    series: &[ResolvedSeriesData],
    (x, y, w, h): (f64, f64, f64, f64),
    label_pt: f64,
    scale: f64,
) {
    let (raw_min, raw_max, n_cats) = series_bounds(series, true);
    let x_scale = build_nice_scale(raw_min, raw_max, 5);
    let cat_w = (0..n_cats)
        .map(|i| width_pt(&category_label(series, i), label_pt))
        .fold(20.0 * scale, f64::max)
        .min(w * 0.30)
        + 6.0 * scale;
    let x_axis_h = label_pt + 6.0 * scale;
    let px = x + cat_w;
    let py = y + x_axis_h;
    let pw = (w - cat_w).max(10.0);
    let ph = (h - x_axis_h).max(10.0);
    let span = x_scale.max - x_scale.min;

    for &tick in &x_scale.ticks {
        let tx = px + ((tick - x_scale.min) / span) * pw;
        canvas.stroke_line(tx, py, tx, py + ph, GRID_RGB, 0.45 * scale);
        let lbl = format_tick(tick);
        let lw = width_pt(&lbl, label_pt);
        canvas.colored_text(tx - lw / 2.0, y + 1.0, label_pt, &lbl, LABEL_RGB);
    }

    let zero_x = px + ((0.0 - x_scale.min) / span).clamp(0.0, 1.0) * pw;
    canvas.stroke_line(zero_x, py, zero_x, py + ph, AXIS_RGB, 0.7 * scale);
    canvas.stroke_line(px, py, px + pw, py, AXIS_RGB, 0.7 * scale);

    if n_cats == 0 || series.is_empty() {
        return;
    }
    let group_h = ph / n_cats as f64;
    let inner_h = group_h * 0.74;
    let bar_h = (inner_h / series.len() as f64).max(1.5);

    for cat in 0..n_cats {
        let row_top = py + ph - cat as f64 * group_h;
        let gy = row_top - (group_h - inner_h) / 2.0 - bar_h;
        for (si, ser) in series.iter().enumerate() {
            if let Some(Some(val)) = ser.values.get(cat) {
                if val.is_finite() {
                    let vx = px + ((*val - x_scale.min) / span) * pw;
                    let (bx, bw) = if vx >= zero_x {
                        (zero_x, (vx - zero_x).max(0.8))
                    } else {
                        (vx, (zero_x - vx).max(0.8))
                    };
                    let by = gy - si as f64 * bar_h;
                    canvas.fill_rect(
                        bx,
                        by,
                        bw,
                        (bar_h - 0.8).max(1.0),
                        PALETTE[si % PALETTE.len()],
                    );
                }
            }
        }
        let lbl = fit_text(&category_label(series, cat), label_pt, cat_w - 4.0 * scale);
        let lw = width_pt(&lbl, label_pt);
        canvas.colored_text(
            px - lw - 3.0 * scale,
            row_top - group_h * 0.5 - label_pt * 0.35,
            label_pt,
            &lbl,
            LABEL_RGB,
        );
    }
}

fn draw_line_chart(
    canvas: &mut Canvas,
    series: &[ResolvedSeriesData],
    (x, y, w, h): (f64, f64, f64, f64),
    label_pt: f64,
    scale: f64,
) {
    let (raw_min, raw_max, n_cats) = series_bounds(series, true);
    let y_scale = build_nice_scale(raw_min, raw_max, 5);
    let (px, py, pw, ph, span) =
        draw_y_grid_and_labels(canvas, &y_scale, (x, y, w, h), label_pt, scale);
    canvas.stroke_line(px, py, px, py + ph, AXIS_RGB, 0.7 * scale);
    canvas.stroke_line(px, py, px + pw, py, AXIS_RGB, 0.7 * scale);

    if n_cats == 0 {
        return;
    }
    draw_x_cat_labels(canvas, series, n_cats, (px, y, pw), label_pt, scale);
    let slot_w = pw / n_cats as f64;

    let r = (2.4 * scale).clamp(1.2, 4.0);
    for (si, ser) in series.iter().enumerate() {
        let color = PALETTE[si % PALETTE.len()];
        let mut segment = Vec::new();
        for (cat, opt) in ser.values.iter().enumerate() {
            if let Some(val) = opt.filter(|v| v.is_finite()) {
                let pt_x = px + (cat as f64 + 0.5) * slot_w;
                let pt_y = py + ((val - y_scale.min) / span) * ph;
                segment.push((pt_x, pt_y));
            } else if !segment.is_empty() {
                canvas.stroke_polyline(&segment, color, 1.4 * scale);
                for &(mx, my) in &segment {
                    fill_circle(canvas, mx, my, r, color);
                }
                segment.clear();
            }
        }
        if !segment.is_empty() {
            canvas.stroke_polyline(&segment, color, 1.4 * scale);
            for &(mx, my) in &segment {
                fill_circle(canvas, mx, my, r, color);
            }
        }
    }
}

fn draw_pie_chart(
    canvas: &mut Canvas,
    series: &[ResolvedSeriesData],
    (x, y, w, h): (f64, f64, f64, f64),
    scale: f64,
) {
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let r = (w.min(h) * 0.42).max(8.0);
    let Some(first) = series.first() else {
        return;
    };
    let positive: Vec<(usize, f64)> = first
        .values
        .iter()
        .enumerate()
        .filter_map(|(i, v)| v.filter(|x| x.is_finite() && *x > 0.0).map(|x| (i, x)))
        .collect();
    let total: f64 = positive.iter().map(|(_, v)| *v).sum();
    if total <= 0.0 {
        let _ = scale;
        fill_circle(canvas, cx, cy, r, GRID_RGB);
        return;
    }

    let mut angle = std::f64::consts::FRAC_PI_2;
    for (i, val) in positive {
        let sweep = (val / total) * std::f64::consts::TAU;
        let next = angle - sweep;
        fill_pie_wedge(
            canvas,
            cx,
            cy,
            r,
            next,
            angle,
            PALETTE[i % PALETTE.len()],
            CARD_BG_RGB,
            (0.8 * scale).max(0.4),
        );
        angle = next;
    }
}

fn draw_scatter_chart(
    canvas: &mut Canvas,
    series: &[ResolvedSeriesData],
    (x, y, w, h): (f64, f64, f64, f64),
    label_pt: f64,
    scale: f64,
) {
    let mut pairs: Vec<Vec<(f64, f64)>> = Vec::with_capacity(series.len());
    let (mut x_min, mut x_max) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut y_min, mut y_max) = (f64::INFINITY, f64::NEG_INFINITY);

    for ser in series {
        let mut pts = Vec::new();
        for (i, opt_y) in ser.values.iter().enumerate() {
            let Some(yv) = opt_y.filter(|v| v.is_finite()) else {
                continue;
            };
            let xv = ser
                .categories
                .get(i)
                .and_then(|s| parse_chart_number(s))
                .unwrap_or((i + 1) as f64);
            x_min = x_min.min(xv);
            x_max = x_max.max(xv);
            y_min = y_min.min(yv);
            y_max = y_max.max(yv);
            pts.push((xv, yv));
        }
        pairs.push(pts);
    }
    if !x_min.is_finite() || !y_min.is_finite() {
        (x_min, x_max) = (0.0, 10.0);
        (y_min, y_max) = (0.0, 10.0);
    }

    let x_scale = build_nice_scale(x_min, x_max, 5);
    let y_scale = build_nice_scale(y_min, y_max, 5);
    let x_span = x_scale.max - x_scale.min;
    let (px, py, pw, ph, y_span) =
        draw_y_grid_and_labels(canvas, &y_scale, (x, y, w, h), label_pt, scale);
    for &tick in &x_scale.ticks {
        let tx = px + ((tick - x_scale.min) / x_span) * pw;
        canvas.stroke_line(tx, py, tx, py + ph, GRID_RGB, 0.45 * scale);
        let lbl = format_tick(tick);
        let lw = width_pt(&lbl, label_pt);
        canvas.colored_text(tx - lw / 2.0, y + 1.0, label_pt, &lbl, LABEL_RGB);
    }
    canvas.stroke_line(px, py, px, py + ph, AXIS_RGB, 0.7 * scale);
    canvas.stroke_line(px, py, px + pw, py, AXIS_RGB, 0.7 * scale);

    let r = (2.6 * scale).clamp(1.2, 4.0);
    for (si, pts) in pairs.iter().enumerate() {
        if pts.is_empty() {
            continue;
        }
        let color = PALETTE[si % PALETTE.len()];
        let mapped: Vec<(f64, f64)> = pts
            .iter()
            .map(|&(xv, yv)| {
                (
                    px + ((xv - x_scale.min) / x_span) * pw,
                    py + ((yv - y_scale.min) / y_span) * ph,
                )
            })
            .collect();
        if mapped.len() >= 2 {
            canvas.stroke_polyline(&mapped, color, 1.2 * scale);
        }
        for &(mx, my) in &mapped {
            if si % 2 == 0 {
                fill_circle(canvas, mx, my, r, color);
            } else {
                canvas.fill_polygon(
                    &[(mx, my + r), (mx + r, my), (mx, my - r), (mx - r, my)],
                    color,
                );
            }
        }
    }
}

fn fill_circle(canvas: &mut Canvas, cx: f64, cy: f64, r: f64, rgb: u32) {
    let k = 0.552_284_749_8 * r;
    canvas.push_raw_op(format!(
        "q {} rg {:.2} {:.2} m {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c \
         {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c \
         {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c \
         {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c h f Q",
        rgb_ops(rgb),
        cx + r,
        cy,
        cx + r,
        cy + k,
        cx + k,
        cy + r,
        cx,
        cy + r,
        cx - k,
        cy + r,
        cx - r,
        cy + k,
        cx - r,
        cy,
        cx - r,
        cy - k,
        cx - k,
        cy - r,
        cx,
        cy - r,
        cx + k,
        cy - r,
        cx + r,
        cy - k,
        cx + r,
        cy,
    ));
}

#[allow(clippy::too_many_arguments)]
fn fill_pie_wedge(
    canvas: &mut Canvas,
    cx: f64,
    cy: f64,
    r: f64,
    start_rad: f64,
    end_rad: f64,
    fill_rgb: u32,
    stroke_rgb: u32,
    stroke_w: f64,
) {
    let sweep = (end_rad - start_rad).abs();
    if sweep >= std::f64::consts::TAU - 1e-4 {
        fill_circle(canvas, cx, cy, r, fill_rgb);
        return;
    }
    let segments = ((sweep / std::f64::consts::FRAC_PI_2).ceil() as usize).clamp(1, 8);
    let seg = (end_rad - start_rad) / segments as f64;
    let alpha = (4.0 / 3.0) * (seg / 4.0).tan();
    let p0x = cx + r * start_rad.cos();
    let p0y = cy + r * start_rad.sin();

    let mut op = format!(
        "q {stroke_w:.2} w {} rg {} RG {cx:.2} {cy:.2} m {p0x:.2} {p0y:.2} l",
        rgb_ops(fill_rgb),
        rgb_ops(stroke_rgb)
    );
    for i in 0..segments {
        let t0 = start_rad + i as f64 * seg;
        let t1 = t0 + seg;
        let x0 = cx + r * t0.cos();
        let y0 = cy + r * t0.sin();
        let x3 = cx + r * t1.cos();
        let y3 = cy + r * t1.sin();
        let x1 = x0 - alpha * r * t0.sin();
        let y1 = y0 + alpha * r * t0.cos();
        let x2 = x3 + alpha * r * t1.sin();
        let y2 = y3 - alpha * r * t1.cos();
        let _ = write!(op, " {x1:.2} {y1:.2} {x2:.2} {y2:.2} {x3:.2} {y3:.2} c");
    }
    op.push_str(" h B Q");
    canvas.push_raw_op(op);
}
