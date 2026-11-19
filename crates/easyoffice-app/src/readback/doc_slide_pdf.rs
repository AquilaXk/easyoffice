//! Structured readback and integrity checks for Documents (`DOCX`/`ODT`/`RTF`/`MD`/`TXT`/`Pages`/`HWP`/`HWPX`),
//! Slides (`PPTX`/`PPSX`/`PPT`/`ODP`), and PDF (`ISO 32000`) (PD-248).

use std::collections::HashSet;
use std::fmt::Write;

use easyoffice_formats::rich_document::toc_bookmark_number;
use easyoffice_formats::{
    evaluate_table_formulas, DecodedSlide, PdfReadbackReport, RichAlignment, RichBindingKind,
    RichBlock, RichDocument, RichField, RichImage, RichListKind, RichNoteKind, RichParagraph,
    RichRevision,
};

use super::{push_json_str, push_opt_json_str};

const MAX_TABLE_PREVIEW_CELLS: usize = 256;
const MAX_PDF_TEXT_PREVIEW_CHARS: usize = 512;

fn align_str(a: RichAlignment) -> &'static str {
    match a {
        RichAlignment::Left => "left",
        RichAlignment::Center => "center",
        RichAlignment::Right => "right",
        RichAlignment::Justify => "justify",
    }
}

fn field_str(f: RichField) -> &'static str {
    match f {
        RichField::PageNumber => "page",
        RichField::TotalPages => "pages",
        RichField::FileName => "filename",
    }
}

fn truncate_chars(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}

fn count_equation_markers(s: &str) -> usize {
    let mut rest = s;
    let mut count = 0usize;
    while let Some(open) = rest.find("$$") {
        let after_open = &rest[open + 2..];
        if let Some(close) = after_open.find("$$") {
            if !after_open[..close].trim().is_empty() {
                count += 1;
            }
            rest = &after_open[close + 2..];
        } else {
            break;
        }
    }
    count
}

fn is_table_formula(cell: &str) -> bool {
    let trimmed = cell.trim();
    let Some(expr) = trimmed.strip_prefix('=') else {
        return false;
    };
    let upper = expr.trim().to_ascii_uppercase();
    ["SUM(", "AVERAGE(", "AVG(", "COUNT(", "MAX(", "MIN("]
        .iter()
        .any(|prefix| upper.starts_with(prefix))
}

fn frame_zone_text(paras: &[RichParagraph]) -> String {
    paras
        .iter()
        .map(RichParagraph::plain_text)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn write_document_readback(out: &mut String, doc: &RichDocument, max_items: usize) {
    let frame = &doc.page_frame;
    let paper = frame.paper.unwrap_or(easyoffice_formats::RichPaper::A4);
    let orientation = if paper.width > paper.height {
        "landscape"
    } else {
        "portrait"
    };
    let binding_kind = match frame.binding.kind {
        RichBindingKind::Single => "single",
        RichBindingKind::Duplex => "duplex",
        RichBindingKind::Top => "top",
    };
    let _ = write!(
        out,
        "\"pageSetup\":{{\"paper\":{{\"widthTwips\":{},\"heightTwips\":{},\"orientation\":\"{orientation}\"}},\"margins\":",
        paper.width, paper.height
    );
    match frame.margins {
        Some(m) => {
            let _ = write!(
                out,
                "{{\"top\":{},\"bottom\":{},\"header\":{},\"footer\":{},\"left\":{},\"right\":{}}}",
                m.top, m.bottom, m.header, m.footer, m.left, m.right
            );
        }
        None => out.push_str("null"),
    }
    let _ = write!(
        out,
        ",\"binding\":{{\"kind\":\"{binding_kind}\",\"gutterTwips\":{}}},\"firstPageDifferent\":{},\"evenOddDifferent\":{},\"headersFooters\":{{",
        frame.binding.gutter, frame.first_page_different, frame.even_odd_different
    );
    for (i, (key, paras)) in [
        ("header", &frame.header),
        ("footer", &frame.footer),
        ("firstHeader", &frame.first_header),
        ("firstFooter", &frame.first_footer),
        ("evenHeader", &frame.even_header),
        ("evenFooter", &frame.even_footer),
    ]
    .into_iter()
    .enumerate()
    {
        if i > 0 {
            out.push(',');
        }
        let text = frame_zone_text(paras);
        let _ = write!(out, "\"{key}\":{{\"paragraphs\":{},\"text\":", paras.len());
        push_json_str(out, &text);
        out.push('}');
    }
    out.push_str("}},");

    let mut bookmark_ids: HashSet<u32> = HashSet::new();
    for block in &doc.blocks {
        if let RichBlock::Paragraph(p) = block {
            if let Some(bm) = p.bookmark {
                bookmark_ids.insert(bm);
            }
        }
    }

    let mut para_count = 0usize;
    let mut heading_count = 0usize;
    let mut list_count = 0usize;
    let mut task_count = 0usize;
    let mut checked_task_count = 0usize;
    let mut table_count = 0usize;
    let mut image_count = 0usize;
    let mut missing_alt_image_count = 0usize;
    let mut equation_count = 0usize;
    let mut link_count = 0usize;
    let mut word_count = 0usize;
    let mut chars_with_spaces = 0usize;
    let mut chars_no_spaces = 0usize;
    let mut footnote_section = false;
    let mut table_formula_count = 0usize;
    let mut revision_count = 0usize;

    let mut outline: Vec<(usize, u8, String, Option<u32>)> = Vec::new();
    let mut empty_headings: Vec<usize> = Vec::new();
    let mut missing_alt_images: Vec<usize> = Vec::new();
    type ReadbackImageEntry<'a> = (usize, Option<(u32, u32)>, &'a RichImage);
    let mut image_entries: Vec<ReadbackImageEntry<'_>> = Vec::new();
    let mut broken_bookmarks: Vec<(usize, String)> = Vec::new();
    let mut irregular_tables: Vec<usize> = Vec::new();
    let mut revisions: Vec<(usize, &RichRevision, &str)> = Vec::new();

    for (idx, block) in doc.blocks.iter().enumerate() {
        match block {
            RichBlock::Paragraph(p) => {
                para_count += 1;
                let text = p.plain_text();
                let trimmed = text.trim();
                word_count += text.split_whitespace().count();
                chars_with_spaces += text.chars().count();
                chars_no_spaces += text.chars().filter(|c| !c.is_whitespace()).count();
                equation_count += count_equation_markers(&text);
                if p.heading_level > 0 {
                    heading_count += 1;
                    if matches!(trimmed, "각주" | "미주") {
                        footnote_section = true;
                    }
                    if trimmed.is_empty() {
                        empty_headings.push(idx);
                    }
                    if outline.len() < max_items {
                        outline.push((idx, p.heading_level, text.clone(), p.bookmark));
                    }
                }
                if let Some(kind) = p.list {
                    list_count += 1;
                    if kind == RichListKind::Task {
                        task_count += 1;
                        if p.task_checked {
                            checked_task_count += 1;
                        }
                    }
                }
                for r in &p.runs {
                    if let Some(rev) = &r.revision {
                        revision_count += 1;
                        if revisions.len() < max_items {
                            revisions.push((idx, rev, r.text.as_str()));
                        }
                    }
                    if let Some(url) = &r.link {
                        link_count += 1;
                        if let Some(name) = url.strip_prefix('#') {
                            if let Some(num) = toc_bookmark_number(name) {
                                if !bookmark_ids.contains(&num)
                                    && broken_bookmarks.len() < max_items
                                {
                                    broken_bookmarks.push((idx, url.clone()));
                                }
                            }
                        }
                    }
                }
            }
            RichBlock::Table(t) => {
                table_count += 1;
                link_count += t.links.len();
                let expected_cols = t.rows.first().map_or(0, Vec::len);
                let mut rect_ok = true;
                for row in &t.rows {
                    if row.len() != expected_cols {
                        rect_ok = false;
                    }
                    for cell in row {
                        word_count += cell.split_whitespace().count();
                        chars_with_spaces += cell.chars().count();
                        chars_no_spaces += cell.chars().filter(|c| !c.is_whitespace()).count();
                        equation_count += count_equation_markers(cell);
                        if is_table_formula(cell) {
                            table_formula_count += 1;
                        }
                    }
                }
                let valid_merges = t.validated_merges();
                let dropped_merges = t.merges.len().saturating_sub(valid_merges.len());
                if !rect_ok || dropped_merges > 0 {
                    irregular_tables.push(idx);
                }
                for l in &t.links {
                    if let Some(name) = l.url.strip_prefix('#') {
                        if let Some(num) = toc_bookmark_number(name) {
                            if !bookmark_ids.contains(&num) && broken_bookmarks.len() < max_items {
                                broken_bookmarks.push((idx, l.url.clone()));
                            }
                        }
                    }
                }
                let cell_imgs = t.validated_cell_images();
                image_count += cell_imgs.len();
                let mut table_missing_alt = false;
                for ci in cell_imgs {
                    if image_entries.len() < max_items {
                        image_entries.push((idx, Some((ci.row, ci.col)), &ci.image));
                    }
                    if ci.image.alt().trim().is_empty() {
                        missing_alt_image_count += 1;
                        table_missing_alt = true;
                    }
                }
                if table_missing_alt {
                    missing_alt_images.push(idx);
                }
            }
            RichBlock::Image(img) => {
                image_count += 1;
                if image_entries.len() < max_items {
                    image_entries.push((idx, None, img));
                }
                if img.alt().trim().is_empty() {
                    missing_alt_image_count += 1;
                    missing_alt_images.push(idx);
                }
            }
        }
    }

    let validated_notes = doc.validated_notes();
    let validated_comments = doc.validated_comments();
    let footnote_count = validated_notes
        .iter()
        .filter(|n| n.kind == RichNoteKind::Footnote)
        .count();
    let endnote_count = validated_notes.len() - footnote_count;
    footnote_section |= !validated_notes.is_empty();

    let _ = write!(
        out,
        "\"statistics\":{{\"blockCount\":{},\"paragraphCount\":{para_count},\"headingCount\":{heading_count},\"listItemCount\":{list_count},\"taskItemCount\":{task_count},\"checkedTaskCount\":{checked_task_count},\"tableCount\":{table_count},\"imageCount\":{image_count},\"missingAltImageCount\":{missing_alt_image_count},\"equationCount\":{equation_count},\"footnoteSectionPresent\":{footnote_section},\"footnoteCount\":{footnote_count},\"endnoteCount\":{endnote_count},\"commentCount\":{},\"revisionCount\":{revision_count},\"trackChanges\":{},\"linkCount\":{link_count},\"wordCount\":{word_count},\"charCountWithSpaces\":{chars_with_spaces},\"charCountWithoutSpaces\":{chars_no_spaces}}},\"outline\":[",
        doc.blocks.len(),
        validated_comments.len(),
        doc.track_changes
    );
    for (i, (b_idx, level, text, bm)) in outline.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{{\"blockIndex\":{b_idx},\"level\":{level},\"text\":");
        push_json_str(out, text);
        out.push_str(",\"bookmark\":");
        match bm {
            Some(n) => {
                let _ = write!(out, "{n}");
            }
            None => out.push_str("null"),
        }
        out.push('}');
    }

    let truncated = doc.blocks.len() > max_items;
    let kept = doc.blocks.len().min(max_items);
    let _ = write!(out, "],\"blocksTruncated\":{truncated},\"blocks\":[");
    for (idx, block) in doc.blocks.iter().take(kept).enumerate() {
        if idx > 0 {
            out.push(',');
        }
        write_doc_block_summary(out, idx, block);
    }

    out.push_str("],\"images\":[");
    for (i, (b_idx, cell_pos, img)) in image_entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        match cell_pos {
            Some((r, c)) => {
                let _ = write!(
                    out,
                    "{{\"tableBlockIndex\":{b_idx},\"row\":{r},\"col\":{c},"
                );
            }
            None => {
                let _ = write!(out, "{{\"blockIndex\":{b_idx},");
            }
        }
        write_readback_image_meta(out, img);
        out.push('}');
    }

    out.push_str("],\"checks\":{\"emptyHeadings\":[");
    write_usize_list(out, &empty_headings);
    out.push_str("],\"missingAltImages\":[");
    write_usize_list(out, &missing_alt_images);
    out.push_str("],\"irregularTables\":[");
    write_usize_list(out, &irregular_tables);
    out.push_str("],\"brokenInternalBookmarks\":[");
    for (i, (b_idx, target)) in broken_bookmarks.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{{\"blockIndex\":{b_idx},\"target\":");
        push_json_str(out, target);
        out.push('}');
    }
    let _ = write!(out, "],\"tableFormulaCount\":{table_formula_count}}}");
    if !validated_notes.is_empty() {
        out.push_str(",\"notes\":[");
        for (i, n) in validated_notes.iter().take(max_items).enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"id\":{},\"kind\":\"{}\",\"text\":",
                n.id,
                n.kind.wire_name()
            );
            push_json_str(out, &n.text);
            out.push('}');
        }
        out.push(']');
    }
    if !validated_comments.is_empty() {
        out.push_str(",\"comments\":[");
        for (i, c) in validated_comments.iter().take(max_items).enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(out, "{{\"id\":{},\"author\":", c.id);
            push_json_str(out, &c.author);
            out.push_str(",\"date\":");
            push_json_str(out, &c.date);
            out.push_str(",\"text\":");
            push_json_str(out, &c.text);
            out.push('}');
        }
        out.push(']');
    }
    if !revisions.is_empty() {
        out.push_str(",\"revisions\":[");
        for (i, (b_idx, rev, text)) in revisions.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"blockIndex\":{b_idx},\"id\":{},\"kind\":\"{}\",\"author\":",
                rev.id,
                rev.kind.wire_name()
            );
            push_json_str(out, &rev.author);
            out.push_str(",\"date\":");
            push_json_str(out, &rev.date);
            out.push_str(",\"text\":");
            push_json_str(out, text);
            out.push('}');
        }
        out.push(']');
    }
}

fn write_usize_list(out: &mut String, items: &[usize]) {
    for (i, v) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{v}");
    }
}

fn write_doc_block_summary(out: &mut String, index: usize, block: &RichBlock) {
    match block {
        RichBlock::Paragraph(p) => {
            let text = p.plain_text();
            let has_eq = count_equation_markers(&text) > 0;
            let _ = write!(out, "{{\"index\":{index},\"type\":\"paragraph\",\"text\":");
            push_json_str(out, &text);
            let _ = write!(
                out,
                ",\"runCount\":{},\"heading\":{},\"align\":",
                p.runs.len(),
                p.heading_level
            );
            push_opt_json_str(out, p.alignment.map(align_str));
            out.push_str(",\"list\":");
            match p.list {
                Some(RichListKind::Bullet) => {
                    let _ = write!(out, "\"bullet\",\"listLevel\":{}", p.list_level.min(8));
                }
                Some(RichListKind::Ordered) => {
                    let code = p.list_number.unwrap_or_default().to_wire_code();
                    let _ = write!(
                        out,
                        "\"ordered\",\"listLevel\":{},\"listNumber\":\"{code}\"",
                        p.list_level.min(8)
                    );
                }
                Some(RichListKind::Task) => {
                    let _ = write!(
                        out,
                        "\"task\",\"checked\":{},\"listLevel\":{}",
                        p.task_checked,
                        p.list_level.min(8)
                    );
                }
                None => out.push_str("null"),
            }
            if let Some(cols) = p.columns.filter(|c| c.count > 1) {
                let _ = write!(
                    out,
                    ",\"columns\":{{\"count\":{},\"equal\":{},\"gapTwips\":{}}}",
                    cols.count, cols.equal, cols.gap_twips
                );
            }
            let _ = write!(
                out,
                ",\"pageBreakBefore\":{},\"hasEquation\":{has_eq},\"links\":[",
                p.page_break_before
            );
            let mut l_i = 0usize;
            for r in &p.runs {
                if let Some(url) = &r.link {
                    if l_i > 0 {
                        out.push(',');
                    }
                    l_i += 1;
                    push_json_str(out, url);
                }
            }
            out.push_str("],\"fields\":[");
            let mut f_i = 0usize;
            for r in &p.runs {
                if let Some(f) = r.field {
                    if f_i > 0 {
                        out.push(',');
                    }
                    f_i += 1;
                    push_json_str(out, field_str(f));
                }
            }
            out.push_str("]}");
        }
        RichBlock::Table(t) => {
            let mut rows_eval = t.rows.clone();
            let formula_cells: usize = t
                .rows
                .iter()
                .flat_map(|r| r.iter())
                .filter(|c| is_table_formula(c))
                .count();
            if formula_cells > 0 {
                evaluate_table_formulas(&mut rows_eval);
            }
            let row_count = rows_eval.len();
            let col_count = rows_eval.iter().map(Vec::len).max().unwrap_or(0);
            let has_custom_widths = t.validated_col_widths().is_some();
            let styled_cell_count = t.validated_cell_styles().len();
            let cell_image_count = t.validated_cell_images().len();
            let _ = write!(
                out,
                "{{\"index\":{index},\"type\":\"table\",\"rows\":{row_count},\"cols\":{col_count},\"mergeCount\":{},\"linkCount\":{},\"formulaCellCount\":{formula_cells},\"hasCustomColWidths\":{has_custom_widths},\"styledCellCount\":{styled_cell_count},\"cellImageCount\":{cell_image_count},\"cells\":[",
                t.merges.len(),
                t.links.len()
            );
            let mut budget = MAX_TABLE_PREVIEW_CELLS;
            for (r_idx, row) in rows_eval.iter().enumerate() {
                if budget == 0 {
                    break;
                }
                if r_idx > 0 {
                    out.push(',');
                }
                out.push('[');
                for (c_idx, cell) in row.iter().enumerate() {
                    if budget == 0 {
                        break;
                    }
                    if c_idx > 0 {
                        out.push(',');
                    }
                    budget -= 1;
                    push_json_str(out, cell);
                }
                out.push(']');
            }
            out.push_str("]}");
        }
        RichBlock::Image(img) => {
            let _ = write!(out, "{{\"index\":{index},\"type\":\"image\",");
            write_readback_image_meta(out, img);
            out.push('}');
        }
    }
}

fn write_readback_image_meta(out: &mut String, img: &RichImage) {
    let (width_emu, height_emu) = img.display_emu();
    let _ = write!(
        out,
        "\"format\":\"{}\",\"bytes\":{},\"widthPx\":{},\"heightPx\":{},\"widthEmu\":{width_emu},\"heightEmu\":{height_emu},\"alt\":",
        img.format().extension(),
        img.bytes().len(),
        img.width_px(),
        img.height_px(),
    );
    push_json_str(out, img.alt());
}

pub(super) fn write_slide_deck_readback(
    out: &mut String,
    deck: &easyoffice_formats::RichSlideDeck,
    max_items: usize,
) {
    crate::slide_deck_json::write_slide_deck_readback(
        out,
        deck,
        max_items,
        push_json_str,
        write_usize_list,
    );
}

pub(super) fn write_slides_readback(out: &mut String, slides: &[DecodedSlide], max_items: usize) {
    let mut untitled: Vec<usize> = Vec::new();
    let mut empty: Vec<usize> = Vec::new();
    let mut dense: Vec<usize> = Vec::new();

    for (idx, (title, bullets, _)) in slides.iter().enumerate() {
        let slide_no = idx + 1;
        let t_empty = title.trim().is_empty();
        let non_empty_bullets = bullets.iter().filter(|b| !b.trim().is_empty()).count();
        let body_chars: usize = bullets.iter().map(|b| b.chars().count()).sum();
        if t_empty {
            untitled.push(slide_no);
        }
        if t_empty && non_empty_bullets == 0 {
            empty.push(slide_no);
        }
        if non_empty_bullets > 8 || body_chars > 400 {
            dense.push(slide_no);
        }
    }

    let truncated = slides.len() > max_items;
    let kept = slides.len().min(max_items);
    let _ = write!(
        out,
        "\"slideCount\":{},\"slidesTruncated\":{truncated},\"slides\":[",
        slides.len()
    );
    for (idx, (title, bullets, notes)) in slides.iter().take(kept).enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let body_chars: usize =
            title.chars().count() + bullets.iter().map(|b| b.chars().count()).sum::<usize>();
        let has_notes = !notes.trim().is_empty();
        let _ = write!(out, "{{\"index\":{idx},\"number\":{},\"title\":", idx + 1);
        push_json_str(out, title);
        let _ = write!(out, ",\"bulletCount\":{},\"bullets\":[", bullets.len());
        for (b_i, b) in bullets.iter().enumerate() {
            if b_i > 0 {
                out.push(',');
            }
            push_json_str(out, b);
        }
        let _ = write!(
            out,
            "],\"hasNotes\":{has_notes},\"charCount\":{body_chars},\"notes\":"
        );
        push_json_str(out, notes);
        out.push('}');
    }
    out.push_str("],\"checks\":{\"untitledSlides\":[");
    write_usize_list(out, &untitled);
    out.push_str("],\"emptySlides\":[");
    write_usize_list(out, &empty);
    out.push_str("],\"denseSlides\":[");
    write_usize_list(out, &dense);
    out.push_str("]}");
}

pub(super) fn write_pdf_readback(out: &mut String, report: &PdfReadbackReport, max_items: usize) {
    let mut blank_pages: Vec<usize> = Vec::new();
    let mut rotated_pages: Vec<usize> = Vec::new();
    for p in &report.pages {
        if p.body_text.trim().is_empty() {
            blank_pages.push(p.page_number);
        }
        if p.rotation_degrees.rem_euclid(360) != 0 {
            rotated_pages.push(p.page_number);
        }
    }

    let truncated = report.pages.len() > max_items;
    let kept = report.pages.len().min(max_items);
    let _ = write!(
        out,
        "\"pageCount\":{},\"watermarkText\":",
        report.pages.len()
    );
    push_opt_json_str(out, report.watermark_text.as_deref());
    let _ = write!(
        out,
        ",\"outlineCount\":{},\"outlines\":[",
        report.outline_count
    );
    for (i, t) in report.outline_titles.iter().take(max_items).enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_str(out, t);
    }
    let _ = write!(
        out,
        "],\"linkAnnotationCount\":{},\"markupAnnotationCount\":{},\"formFieldCount\":{},\"pagesTruncated\":{truncated},\"pages\":[",
        report.link_annotation_count, report.markup_annotation_count, report.form_field_count
    );
    for (idx, p) in report.pages.iter().take(kept).enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let char_count = p.body_text.chars().count();
        let preview = truncate_chars(&p.body_text, MAX_PDF_TEXT_PREVIEW_CHARS);
        let _ = write!(
            out,
            "{{\"pageNumber\":{},\"rotationDegrees\":{},\"widthPt\":{:.2},\"heightPt\":{:.2},\"charCount\":{char_count},\"linkCount\":{},\"markupCount\":{},\"widgetCount\":{},\"approvalStamp\":",
            p.page_number,
            p.rotation_degrees,
            p.width_pt,
            p.height_pt,
            p.link_count,
            p.markup_count,
            p.widget_count
        );
        push_opt_json_str(out, p.approval_stamp.as_deref());
        out.push_str(",\"textPreview\":");
        push_json_str(out, preview);
        out.push('}');
    }
    let _ = write!(out, "],\"checks\":{{\"blankPages\":[");
    write_usize_list(out, &blank_pages);
    out.push_str("],\"rotatedPages\":[");
    write_usize_list(out, &rotated_pages);
    let _ = write!(
        out,
        "],\"hasAcroForm\":{},\"hasOutlines\":{}}}",
        report.has_acro_form, report.has_outlines
    );
}
