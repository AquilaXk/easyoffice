//! Single-pass JSON serializer for [`LayoutLintReport`] in readback payloads (`PD-258`).

use std::fmt::Write;

use easyoffice_formats::LayoutLintReport;

use crate::EasyOfficeIpcRouter;

/// Appends `"layoutLint":{...}` into an open JSON object buffer without intermediate allocations.
pub(crate) fn write_layout_lint_property(out: &mut String, report: &LayoutLintReport) {
    let _ = write!(
        out,
        "\"layoutLint\":{{\"issueCount\":{},\"overflowCount\":{},\"overlapCount\":{},\"emptyPlaceholderCount\":{},\"truncated\":{},\"issues\":[",
        report.issues.len(),
        report.overflow_count,
        report.overlap_count,
        report.empty_placeholder_count,
        report.truncated,
    );
    for (idx, issue) in report.issues.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"kind\":\"{}\",\"pageOrSlide\":{},\"blockIndex\":",
            issue.kind.as_str(),
            issue.page_or_slide
        );
        match issue.block_index {
            Some(bi) => {
                let _ = write!(out, "{bi}");
            }
            None => out.push_str("null"),
        }
        out.push_str(",\"targetId\":\"");
        EasyOfficeIpcRouter::escape_json_into(out, &issue.target_id);
        out.push_str("\",\"message\":\"");
        EasyOfficeIpcRouter::escape_json_into(out, &issue.message);
        out.push_str("\"}");
    }
    out.push_str("]}");
}
