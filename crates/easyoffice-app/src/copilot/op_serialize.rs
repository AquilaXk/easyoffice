//! 코파일럿 응답 연산·위치·읽기 도구의 단일 패스 JSON 직렬화기.

use std::fmt::Write as _;

use super::op_types::{
    write_doc_blocks, write_slide_element, write_str_array, write_tool_call_json, CopilotLocation,
    CopilotOperation, CopilotRunResponse,
};
use super::types::push_json_str;

pub fn serialize_run_response(resp: &CopilotRunResponse) -> String {
    let mut out = String::with_capacity(512 + resp.reply.len());
    out.push_str("{\"status\":\"ok\",\"mode\":");
    push_json_str(&mut out, resp.mode.as_str());
    out.push_str(",\"reply\":");
    push_json_str(&mut out, &resp.reply);
    out.push_str(",\"reasoning\":");
    push_json_str(&mut out, &resp.reasoning);
    let _ = write!(
        out,
        ",\"updatedInPlace\":{},\"droppedOperations\":{},\"operations\":[",
        resp.updated_in_place, resp.dropped_operations
    );
    for (idx, op) in resp.operations.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        write_operation_json(&mut out, op);
    }
    out.push_str("],\"locations\":[");
    for (idx, loc) in resp.locations.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        write_location_json(&mut out, loc);
    }
    if !resp.tool_calls.is_empty() {
        out.push_str("],\"toolCalls\":[");
        for (idx, tc) in resp.tool_calls.iter().enumerate() {
            if idx > 0 {
                out.push(',');
            }
            write_tool_call_json(&mut out, tc);
        }
    }
    out.push(']');
    if let Some(usage) = resp.usage {
        super::usage::write_usage_json_field(&mut out, usage);
    }
    out.push('}');
    out
}

fn write_kv_pairs(out: &mut String, pairs: &[(String, String)]) {
    out.push('{');
    for (idx, (k, v)) in pairs.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        push_json_str(out, k);
        out.push(':');
        push_json_str(out, v);
    }
    out.push('}');
}

pub fn write_operation_json(out: &mut String, op: &CopilotOperation) {
    match op {
        CopilotOperation::SheetWrite {
            sheet,
            range,
            cells,
        } => {
            out.push_str("{\"kind\":\"sheet.write\",\"sheet\":");
            push_json_str(out, sheet);
            out.push_str(",\"range\":");
            push_json_str(out, range);
            out.push_str(",\"cells\":[");
            for (idx, c) in cells.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                out.push_str("{\"coord\":");
                push_json_str(out, &c.coord);
                out.push_str(",\"raw\":");
                push_json_str(out, &c.raw);
                if let Some(fmt) = &c.num_fmt {
                    out.push_str(",\"numFmt\":");
                    push_json_str(out, fmt);
                }
                if let Some(bold) = c.bold {
                    let _ = write!(out, ",\"bold\":{bold}");
                }
                if let Some(fill) = &c.fill {
                    out.push_str(",\"fill\":");
                    push_json_str(out, fill);
                }
                if let Some(color) = &c.color {
                    out.push_str(",\"color\":");
                    push_json_str(out, color);
                }
                out.push('}');
            }
            out.push_str("]}");
        }
        CopilotOperation::SheetFormat {
            sheet,
            range,
            fill,
            color,
            bold,
            num_fmt,
        } => {
            out.push_str("{\"kind\":\"sheet.format\",\"sheet\":");
            push_json_str(out, sheet);
            out.push_str(",\"range\":");
            push_json_str(out, range);
            if let Some(f) = fill {
                out.push_str(",\"fill\":");
                push_json_str(out, f);
            }
            if let Some(c) = color {
                out.push_str(",\"color\":");
                push_json_str(out, c);
            }
            if let Some(b) = bold {
                let _ = write!(out, ",\"bold\":{b}");
            }
            if let Some(fmt) = num_fmt {
                out.push_str(",\"numFmt\":");
                push_json_str(out, fmt);
            }
            out.push('}');
        }
        CopilotOperation::SheetChart {
            sheet,
            chart_type,
            data_range,
            title,
            legend,
            switch_row_col,
            width,
            height,
        } => {
            out.push_str("{\"kind\":\"sheet.chart\",\"sheet\":");
            push_json_str(out, sheet);
            let _ = write!(
                out,
                ",\"chartType\":\"{}\",\"dataRange\":",
                chart_type.as_str()
            );
            push_json_str(out, data_range);
            out.push_str(",\"title\":");
            push_json_str(out, title);
            let _ = write!(
                out,
                ",\"legend\":\"{}\",\"switchRowCol\":{switch_row_col}",
                legend.as_str()
            );
            if let Some(w) = width {
                let _ = write!(out, ",\"width\":{w}");
            }
            if let Some(h) = height {
                let _ = write!(out, ",\"height\":{h}");
            }
            out.push('}');
        }
        CopilotOperation::SheetTable {
            sheet,
            action,
            range,
            table_name,
            style,
            total_row,
        } => {
            out.push_str("{\"kind\":\"sheet.table\",\"sheet\":");
            push_json_str(out, sheet);
            out.push_str(",\"action\":");
            push_json_str(out, action);
            if let Some(r) = range {
                out.push_str(",\"range\":");
                push_json_str(out, r);
            }
            if let Some(tn) = table_name {
                out.push_str(",\"tableName\":");
                push_json_str(out, tn);
            }
            if let Some(st) = style {
                out.push_str(",\"style\":");
                push_json_str(out, st);
            }
            if let Some(tr) = total_row {
                let _ = write!(out, ",\"totalRow\":{tr}");
            }
            out.push('}');
        }
        CopilotOperation::SheetPivot {
            sheet,
            source_range,
            dest_sheet,
            dest_cell,
            row_field,
            col_field,
            value_field,
            agg,
        } => {
            out.push_str("{\"kind\":\"sheet.pivot\",\"sheet\":");
            push_json_str(out, sheet);
            out.push_str(",\"sourceRange\":");
            push_json_str(out, source_range);
            if let Some(ds) = dest_sheet {
                out.push_str(",\"destSheet\":");
                push_json_str(out, ds);
            }
            out.push_str(",\"destCell\":");
            push_json_str(out, dest_cell);
            out.push_str(",\"rowField\":");
            push_json_str(out, row_field);
            if let Some(cf) = col_field {
                out.push_str(",\"colField\":");
                push_json_str(out, cf);
            }
            out.push_str(",\"valueField\":");
            push_json_str(out, value_field);
            out.push_str(",\"agg\":");
            push_json_str(out, agg);
            out.push('}');
        }
        CopilotOperation::SheetCondFormat {
            sheet,
            range,
            preset,
            value1,
            value2,
            fill,
            color,
        } => {
            out.push_str("{\"kind\":\"sheet.cond_format\",\"sheet\":");
            push_json_str(out, sheet);
            out.push_str(",\"range\":");
            push_json_str(out, range);
            out.push_str(",\"preset\":");
            push_json_str(out, preset);
            if let Some(v1) = value1 {
                out.push_str(",\"value1\":");
                push_json_str(out, v1);
            }
            if let Some(v2) = value2 {
                out.push_str(",\"value2\":");
                push_json_str(out, v2);
            }
            if let Some(f) = fill {
                out.push_str(",\"fill\":");
                push_json_str(out, f);
            }
            if let Some(c) = color {
                out.push_str(",\"color\":");
                push_json_str(out, c);
            }
            out.push('}');
        }
        CopilotOperation::SheetSortFilter {
            sheet,
            action,
            range,
            col_offset,
            ascending,
            query,
        } => {
            out.push_str("{\"kind\":\"sheet.sort_filter\",\"sheet\":");
            push_json_str(out, sheet);
            out.push_str(",\"action\":");
            push_json_str(out, action);
            out.push_str(",\"range\":");
            push_json_str(out, range);
            if let Some(co) = col_offset {
                let _ = write!(out, ",\"colOffset\":{co}");
            }
            if let Some(asc) = ascending {
                let _ = write!(out, ",\"ascending\":{asc}");
            }
            if let Some(q) = query {
                out.push_str(",\"query\":");
                push_json_str(out, q);
            }
            out.push('}');
        }
        CopilotOperation::SheetStructure {
            sheet,
            action,
            index,
            count,
        } => {
            out.push_str("{\"kind\":\"sheet.structure\",\"sheet\":");
            push_json_str(out, sheet);
            out.push_str(",\"action\":");
            push_json_str(out, action);
            let _ = write!(out, ",\"index\":{index},\"count\":{count}}}");
        }
        CopilotOperation::SheetNote { sheet, coord, text } => {
            out.push_str("{\"kind\":\"sheet.note\",\"sheet\":");
            push_json_str(out, sheet);
            out.push_str(",\"coord\":");
            push_json_str(out, coord);
            out.push_str(",\"text\":");
            push_json_str(out, text);
            out.push('}');
        }
        CopilotOperation::DocAppend { blocks } => {
            out.push_str("{\"kind\":\"doc.append\",\"blocks\":");
            write_doc_blocks(out, blocks);
            out.push('}');
        }
        CopilotOperation::DocReplaceParagraphs { replacements } => {
            out.push_str("{\"kind\":\"doc.replace_paragraphs\",\"replacements\":[");
            for (idx, r) in replacements.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                let _ = write!(out, "{{\"index\":{},\"text\":", r.index);
                push_json_str(out, &r.text);
                out.push('}');
            }
            out.push_str("]}");
        }
        CopilotOperation::DocReplaceSpan {
            index,
            search,
            replace,
        } => {
            let _ = write!(
                out,
                "{{\"kind\":\"doc.replace_span\",\"index\":{index},\"search\":"
            );
            push_json_str(out, search);
            out.push_str(",\"replace\":");
            push_json_str(out, replace);
            out.push('}');
        }
        CopilotOperation::DocReplaceRange {
            start_index,
            end_index,
            blocks,
        } => {
            out.push_str("{\"kind\":\"doc.replace_paragraphs\"");
            if let Some(s) = start_index {
                let _ = write!(out, ",\"startIndex\":{s}");
            }
            if let Some(e) = end_index {
                let _ = write!(out, ",\"endIndex\":{e}");
            }
            out.push_str(",\"blocks\":");
            write_doc_blocks(out, blocks);
            out.push('}');
        }
        CopilotOperation::DocScrubPii => {
            out.push_str("{\"kind\":\"doc.scrub_pii\"}");
        }
        CopilotOperation::SlideAdd {
            title,
            bullets,
            notes,
            layout,
            transition,
            elements,
        } => {
            out.push_str("{\"kind\":\"slide.add\",\"title\":");
            push_json_str(out, title);
            out.push_str(",\"bullets\":");
            write_str_array(out, bullets);
            out.push_str(",\"notes\":");
            push_json_str(out, notes);
            if let Some(l) = layout {
                out.push_str(",\"layout\":");
                push_json_str(out, l);
            }
            if let Some(tr) = transition {
                out.push_str(",\"transition\":");
                push_json_str(out, tr);
            }
            if let Some(els) = elements {
                out.push_str(",\"elements\":[");
                for (i, el) in els.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_slide_element(out, el);
                }
                out.push(']');
            }
            out.push('}');
        }
        CopilotOperation::SlideUpdate {
            index,
            title,
            bullets,
            notes,
            layout,
            transition,
            elements,
        } => {
            out.push_str("{\"kind\":\"slide.update\"");
            if let Some(idx) = index {
                let _ = write!(out, ",\"index\":{idx}");
            }
            if let Some(t) = title {
                out.push_str(",\"title\":");
                push_json_str(out, t);
            }
            if let Some(bs) = bullets {
                out.push_str(",\"bullets\":");
                write_str_array(out, bs);
            }
            if let Some(n) = notes {
                out.push_str(",\"notes\":");
                push_json_str(out, n);
            }
            if let Some(l) = layout {
                out.push_str(",\"layout\":");
                push_json_str(out, l);
            }
            if let Some(tr) = transition {
                out.push_str(",\"transition\":");
                push_json_str(out, tr);
            }
            if let Some(els) = elements {
                out.push_str(",\"elements\":[");
                for (i, el) in els.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_slide_element(out, el);
                }
                out.push(']');
            }
            out.push('}');
        }
        CopilotOperation::SlideAddElement {
            slide_index,
            element,
        } => {
            out.push_str("{\"kind\":\"slide.add_element\"");
            if let Some(idx) = slide_index {
                let _ = write!(out, ",\"slideIndex\":{idx}");
            }
            out.push_str(",\"element\":");
            write_slide_element(out, element);
            out.push('}');
        }
        CopilotOperation::BaseAddField {
            name,
            field_type,
            options,
        } => {
            out.push_str("{\"kind\":\"base.add_field\",\"name\":");
            push_json_str(out, name);
            out.push_str(",\"fieldType\":");
            push_json_str(out, field_type);
            if !options.is_empty() {
                out.push_str(",\"options\":");
                write_str_array(out, options);
            }
            out.push('}');
        }
        CopilotOperation::BaseAddRecords { records } => {
            out.push_str("{\"kind\":\"base.add_records\",\"records\":[");
            for (idx, row) in records.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                write_kv_pairs(out, row);
            }
            out.push_str("]}");
        }
        CopilotOperation::BaseUpdateRecords { updates } => {
            out.push_str("{\"kind\":\"base.update_records\",\"updates\":[");
            for (idx, (row_idx, pairs)) in updates.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                let _ = write!(out, "{{\"rowIndex\":{row_idx},\"values\":");
                write_kv_pairs(out, pairs);
                out.push('}');
            }
            out.push_str("]}");
        }
        CopilotOperation::BaseSetView {
            view,
            filter_field,
            filter_op,
            filter_value,
            sort_field,
            sort_dir,
            group_field,
        } => {
            out.push_str("{\"kind\":\"base.set_view\"");
            if let Some(vw) = view {
                out.push_str(",\"view\":");
                push_json_str(out, vw);
            }
            if let Some(ff) = filter_field {
                out.push_str(",\"filterField\":");
                push_json_str(out, ff);
            }
            if let Some(fo) = filter_op {
                out.push_str(",\"filterOp\":");
                push_json_str(out, fo);
            }
            if let Some(fv) = filter_value {
                out.push_str(",\"filterValue\":");
                push_json_str(out, fv);
            }
            if let Some(sf) = sort_field {
                out.push_str(",\"sortField\":");
                push_json_str(out, sf);
            }
            if let Some(sd) = sort_dir {
                out.push_str(",\"sortDir\":");
                push_json_str(out, sd);
            }
            if let Some(gf) = group_field {
                out.push_str(",\"groupField\":");
                push_json_str(out, gf);
            }
            out.push('}');
        }
        CopilotOperation::BoardAddGraph {
            direction,
            nodes,
            edges,
        } => {
            out.push_str("{\"kind\":\"board.add_graph\",\"direction\":");
            push_json_str(out, direction);
            out.push_str(",\"nodes\":[");
            for (idx, n) in nodes.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                out.push_str("{\"id\":");
                push_json_str(out, &n.id);
                out.push_str(",\"kind\":");
                push_json_str(out, &n.kind);
                if let Some(sh) = &n.shape {
                    out.push_str(",\"shape\":");
                    push_json_str(out, sh);
                }
                out.push_str(",\"text\":");
                push_json_str(out, &n.text);
                if let Some(f) = &n.fill {
                    out.push_str(",\"fill\":");
                    push_json_str(out, f);
                }
                out.push('}');
            }
            out.push_str("],\"edges\":[");
            for (idx, e) in edges.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                out.push_str("{\"from\":");
                push_json_str(out, &e.from_id);
                out.push_str(",\"to\":");
                push_json_str(out, &e.to_id);
                if let Some(lbl) = &e.label {
                    out.push_str(",\"label\":");
                    push_json_str(out, lbl);
                }
                out.push_str(",\"style\":");
                push_json_str(out, &e.style);
                out.push('}');
            }
            out.push_str("]}");
        }
    }
}

fn write_location_json(out: &mut String, loc: &CopilotLocation) {
    out.push_str("{\"module\":");
    push_json_str(out, &loc.module);
    out.push_str(",\"label\":");
    push_json_str(out, &loc.label);
    if let Some(sheet) = &loc.sheet {
        out.push_str(",\"sheet\":");
        push_json_str(out, sheet);
    }
    if let Some(range) = &loc.range {
        out.push_str(",\"range\":");
        push_json_str(out, range);
    }
    if let Some(p_idx) = loc.paragraph_index {
        let _ = write!(out, ",\"paragraphIndex\":{p_idx}");
    }
    if let Some(s_idx) = loc.slide_index {
        let _ = write!(out, ",\"slideIndex\":{s_idx}");
    }
    if let Some(page) = loc.page {
        let _ = write!(out, ",\"page\":{page}");
    }
    out.push('}');
}
