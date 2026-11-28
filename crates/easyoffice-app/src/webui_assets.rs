//! Embedded static WebUI assets served by `--serve` (`http_server.rs`).
//! A unit test verifies every `./*.js`, `./*.css`, and `./*.svg` referenced in `ui/index.html`
//! resolves through [`static_asset`] so `--serve` never drifts from `ui/index.html`.

const HTML_CT: &str = "text/html; charset=utf-8";
const CSS_CT: &str = "text/css; charset=utf-8";
const JS_CT: &str = "application/javascript; charset=utf-8";
const SVG_CT: &str = "image/svg+xml; charset=utf-8";

macro_rules! ui_js_table {
    ($($file:literal),* $(,)?) => {
        &[
            $( (concat!("/", $file), JS_CT, include_str!(concat!("../ui/", $file))), )*
        ]
    };
}

macro_rules! ui_css_table {
    ($($file:literal),* $(,)?) => {
        &[
            $( (concat!("/", $file), CSS_CT, include_str!(concat!("../ui/", $file))), )*
        ]
    };
}

const INDEX_HTML: &str = include_str!("../ui/index.html");
const FRAME_PROBE_HTML: &str = include_str!("../ui/frame-probe.html");
const FAVICON_SVG: &str = include_str!("../ui/favicon.svg");

const CSS_ASSETS: &[(&str, &str, &str)] = ui_css_table![
    "base.css",
    "board.css",
    "content.css",
    "copilot.css",
    "home.css",
    "menu-bar.css",
    "shell.css",
    "styles.css",
    "tokens.css",
    "toolbar.css",
];

const JS_ASSETS: &[(&str, &str, &str)] = ui_js_table![
    "app.js",
    "base-board-host.js",
    "base-cell-nav.js",
    "base-cell-select.js",
    "base-clipboard.js",
    "base-grid-size.js",
    "base-grid-window.js",
    "base-import.js",
    "base-model.js",
    "base-query-cache.js",
    "base-studio.js",
    "base-virtual.js",
    "board-router.js",
    "board-studio.js",
    "board-style-bar.js",
    "color-picker.js",
    "command-history.js",
    "copilot-actions.js",
    "copilot-agent-loop.js",
    "copilot-catalog.js",
    "copilot-composer-input.js",
    "copilot-context-compact.js",
    "copilot-cursor.js",
    "copilot-diff.js",
    "copilot-doc-html.js",
    "copilot-edit-mode.js",
    "copilot-ext-ops.js",
    "copilot-feedback.js",
    "copilot-ghost.js",
    "copilot-markdown.js",
    "copilot-model-ui.js",
    "copilot-perform-cite.js",
    "copilot-perform-ext.js",
    "copilot-perform-hwp.js",
    "copilot-perform-sheet.js",
    "copilot-perform-slide.js",
    "copilot-perform.js",
    "copilot-popup-menu.js",
    "copilot-preview.js",
    "copilot-session-anchor.js",
    "copilot-sheet-ops.js",
    "copilot-slide-read.js",
    "copilot-step-labels.js",
    "copilot-stream.js",
    "copilot-text.js",
    "copilot-thread-view.js",
    "copilot-timeline.js",
    "copilot-typing.js",
    "copilot-ui.js",
    "custom-select.js",
    "desktop-bridge.js",
    "doc-clipboard-images.js",
    "doc-dialogs-ext.js",
    "doc-font-defaults.js",
    "doc-font-dialog.js",
    "doc-font-styles.js",
    "doc-image.js",
    "doc-ime.js",
    "doc-line-spacing.js",
    "doc-link.js",
    "doc-notes-ui.js",
    "doc-notes.js",
    "doc-open-type.js",
    "doc-page-frame.js",
    "doc-page-print.js",
    "doc-page-setup.js",
    "doc-pages-core.js",
    "doc-pages.js",
    "doc-paste-model.js",
    "doc-paste-sanitize.js",
    "doc-paste.js",
    "doc-runs-ui.js",
    "doc-runs.js",
    "doc-symbol-dialog.js",
    "doc-table-grid.js",
    "doc-table-runs.js",
    "doc-table-style.js",
    "doc-table-tab.js",
    "doc-table.js",
    "doc-text-fx.js",
    "doc-toc.js",
    "export-dialog.js",
    "file-open.js",
    "find-base.js",
    "find-core.js",
    "find-doc.js",
    "find-replace.js",
    "find-sheet.js",
    "frame-probe.js",
    "home-screen.js",
    "hwp-host.js",
    "hwp-print.js",
    "hwp-session.js",
    "icons.js",
    "ipc-transport.js",
    "layout-lint-ui.js",
    "menu-keyboard.js",
    "menu-kinds.js",
    "module-search.js",
    "number-stepper.js",
    "office-print.js",
    "pages-ui.js",
    "pdf-edit.js",
    "pii-dom-scrubber.js",
    "print-preview.js",
    "recent-documents-dialog.js",
    "ribbon-catalog.js",
    "ribbon-history.js",
    "save-payload.js",
    "sheet-calc-options.js",
    "sheet-cell-edit.js",
    "sheet-cell-write.js",
    "sheet-cf-bar-ui.js",
    "sheet-cf-presets.js",
    "sheet-cf-preview.js",
    "sheet-cf-quick-ui.js",
    "sheet-chart-svg.js",
    "sheet-chart-ui.js",
    "sheet-clip-html.js",
    "sheet-clipboard.js",
    "sheet-col-fit.js",
    "sheet-color-ui.js",
    "sheet-context-menu.js",
    "sheet-controller.js",
    "sheet-default-font.js",
    "sheet-delta.js",
    "sheet-dialog-kit.js",
    "sheet-dv-widgets.js",
    "sheet-edit-box.js",
    "sheet-fill-core.js",
    "sheet-fill-ui.js",
    "sheet-filter-ui.js",
    "sheet-filter.js",
    "sheet-format.js",
    "sheet-formula-bar.js",
    "sheet-formula-point.js",
    "sheet-freeze.js",
    "sheet-grid-layout-paint.js",
    "sheet-grid-selection.js",
    "sheet-grid-text.js",
    "sheet-grid.js",
    "sheet-hidden-rows.js",
    "sheet-history.js",
    "sheet-input.js",
    "sheet-layout.js",
    "sheet-link-ui.js",
    "sheet-link.js",
    "sheet-look-ops.js",
    "sheet-look-ui.js",
    "sheet-look.js",
    "sheet-media-ui.js",
    "sheet-mouse.js",
    "sheet-multi-range.js",
    "sheet-names-ui.js",
    "sheet-names.js",
    "sheet-nav.js",
    "sheet-note-ui.js",
    "sheet-numfmt-display.js",
    "sheet-numfmt-ui.js",
    "sheet-numfmt.js",
    "sheet-ops.js",
    "sheet-outline-ui.js",
    "sheet-outline.js",
    "sheet-page-print.js",
    "sheet-page.js",
    "sheet-paste-core.js",
    "sheet-paste-ui.js",
    "sheet-pivot-model.js",
    "sheet-pivot-ui.js",
    "sheet-point-ui.js",
    "sheet-protect-ranges.js",
    "sheet-protect-ui.js",
    "sheet-protect.js",
    "sheet-ref-tokens.js",
    "sheet-refs.js",
    "sheet-row-fit.js",
    "sheet-row-height.js",
    "sheet-row-resize.js",
    "sheet-rules-ui.js",
    "sheet-rules.js",
    "sheet-selection-chrome.js",
    "sheet-shortcuts.js",
    "sheet-size-dialogs.js",
    "sheet-slicer.js",
    "sheet-sort-ui.js",
    "sheet-sort.js",
    "sheet-spill.js",
    "sheet-store.js",
    "sheet-structure.js",
    "sheet-tab-menu.js",
    "sheet-tab-names.js",
    "sheet-tab-strip.js",
    "sheet-tab-ui.js",
    "sheet-table-bake.js",
    "sheet-table-dedupe.js",
    "sheet-table-gallery.js",
    "sheet-table-ops.js",
    "sheet-table-presets.js",
    "sheet-table-style-ui.js",
    "sheet-table-style.js",
    "sheet-table-totals.js",
    "sheet-table-ui.js",
    "sheet-tabs.js",
    "sheet-text-columns.js",
    "sheet-utils.js",
    "sheet-view-live.js",
    "sheet-view-state.js",
    "sheet-view-ui.js",
    "shell-chrome.js",
    "slide-canvas.js",
    "slide-clipboard.js",
    "slide-context-menu.js",
    "slide-dialogs.js",
    "slide-studio.js",
    "theme.js",
    "toolbar-scroll.js",
    "update-notice.js",
    "window-bridge.js",
    "workspace-history.js",
    "workspace-panels.js",
    "workspace-scrub.js",
    "workspace-session.js",
    "workspace-tools.js",
    "zoom-range.js",
];

/// Resolves a loopback `GET` route (`/` or `/filename.ext`) to `(content_type, body)`.
pub(crate) fn static_asset(route: &str) -> Option<(&'static str, &'static str)> {
    match route {
        "/" | "/index.html" => return Some((HTML_CT, INDEX_HTML)),
        "/frame-probe.html" => return Some((HTML_CT, FRAME_PROBE_HTML)),
        "/favicon.ico" | "/favicon.svg" => return Some((SVG_CT, FAVICON_SVG)),
        _ => {}
    }
    if let Ok(idx) = CSS_ASSETS.binary_search_by_key(&route, |&(p, _, _)| p) {
        let (_, ct, body) = CSS_ASSETS[idx];
        return Some((ct, body));
    }
    if let Ok(idx) = JS_ASSETS.binary_search_by_key(&route, |&(p, _, _)| p) {
        let (_, ct, body) = JS_ASSETS[idx];
        return Some((ct, body));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_tables_are_strictly_sorted_for_binary_search() {
        for pair in CSS_ASSETS.windows(2) {
            assert!(
                pair[0].0 < pair[1].0,
                "CSS_ASSETS out of order: {} >= {}",
                pair[0].0,
                pair[1].0
            );
        }
        for pair in JS_ASSETS.windows(2) {
            assert!(
                pair[0].0 < pair[1].0,
                "JS_ASSETS out of order: {} >= {}",
                pair[0].0,
                pair[1].0
            );
        }
    }

    #[test]
    fn every_asset_referenced_by_index_html_is_served() {
        for needle in ["src=\"./", "href=\"./"] {
            let mut rest = INDEX_HTML;
            while let Some(pos) = rest.find(needle) {
                rest = &rest[pos + needle.len()..];
                let Some(end) = rest.find('"') else {
                    break;
                };
                let rel = &rest[..end];
                if rel.starts_with("hwp-studio/") {
                    continue;
                }
                let route = format!("/{rel}");
                assert!(
                    static_asset(&route).is_some(),
                    "ui/index.html references '{route}' which webui_assets::static_asset does not serve"
                );
            }
        }
    }
}
