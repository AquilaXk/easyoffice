// What the static design guard (design-guard.mjs, scripts/test-design-guard.mjs) found in the tree on the day it was introduced.
// These lists only shrink: a fix removes its line (the test fails on a line that no longer breaks the rule) and the matching
// count in test-design-guard.mjs goes down with it. A new entry is not a fix. Use the tokens of tokens.css, the classes of the
// kit list in docs/design-system.md, and an icon of ui/icons.js instead.

/** Dialog and control rules with a literal px in height, min-height, font-size, line-height, gap or padding, per stylesheet (selector, cut at 110 characters). */
export const KNOWN_SIZE_VIOLATIONS = {
  "base.css": [
    "#pane-base .custom-select",
    "#pane-base .custom-select-value",
    "#pane-base .custom-select-sizer",
    ".base-field-edit-btn",
    ".base-row-delete-btn",
    ".base-cell-input, .base-cell-select",
    ".base-kanban-move-select",
    ".base-cal-add-btn",
    ".base-import-dialog .base-dlg-select.custom-select",
  ],
  "board.css": [
    "#pane-board .custom-select",
    "#pane-board .custom-select-value",
    "#pane-board .custom-select-sizer",
    ".board-chart-series-input",
    ".board-style-group .dialog-swatch",
    ".board-style-group .dialog-swatch-chip",
    ".board-style-select",
    ".board-style-input",
  ],
  "content.css": [
    "#doc-comment-dialog .doc-comment-textarea",
    ".sheet-chart-btn",
  ],
  "copilot.css": [
    ".eo-copilot-textarea",
    ".eo-copilot-model-menu [focusgroup~=\"menu\"]",
    ".eo-copilot-menu-scroll",
    ".eo-copilot-menu-footer",
    ".eo-copilot-menu-item-body",
    ".eo-copilot-menu-hint",
  ],
  "home.css": [
    ".home-recent-page-btn",
    ".dialog-body",
    ".dialog-footer",
    ".update-notice__button",
  ],
  "menu-bar.css": [
    ".menu-title",
    ".menu-dropdown",
    ".md-item",
    ".md-icon",
    ".md-shortcut, .md-hint",
    ".md-hint",
    ".md-sub",
    ".md-arrow",
    ".md-arrow .eo-icon",
    ".md-sub-panel",
    ".context-menu",
  ],
  "styles.css": [
    ".action-btn, .secondary-btn",
    ".tb-group",
    ".tb-sep",
    ".tb-btn",
    ".tb-char-shape, .tb-para-shape",
    ".tb-note-btn",
    ".sb-field-grid, .sb-command-track, .sb-character-group, .sb-color-group, .sb-align-group, .sb-viewmark-group",
    ".sb-size-input",
    ".eo-glyph-btn",
    ".slide-context-menu",
    ".slide-context-menu button",
    ".cp-input-wrap",
    ".cp-slash",
    ".cmd-input",
    ".cmd-input.cp-input",
    ".cp-list",
    ".cp-item",
    ".cp-item-label",
    ".cp-item-shortcut",
    ".cp-empty",
    ".statusbar-mode-btn",
    ".sb-formula-colors",
  ],
  "toolbar.css": [
    "#doc-style-bar.tb-scrollable .tb-scroll-track, #sheet-pivot-bar.tb-scrollable .tb-scroll-track",
    "#slide-toolbar.tb-scrollable .tb-scroll-track, #pdf-toolbar.tb-scrollable .tb-scroll-track, #base-toolbar.tb-s…",
    ".tb-scrollable:has(> .tb-scroll-nav-prev:not([hidden])), #icon-toolbar.tb-scrollable:has(> .tb-scroll-nav-prev…",
    ".tb-scrollable:has(> .tb-scroll-nav-next:not([hidden])), #icon-toolbar.tb-scrollable:has(> .tb-scroll-nav-next…",
    "#icon-toolbar .tb-btn, #slide-toolbar .tb-btn, #pdf-toolbar .tb-btn, #base-toolbar .tb-btn, #board-toolbar .tb…",
    ".tb-sprite",
    ".tb-icon-text",
    ".tb-icon-svg",
    ".tb-icon-svg .eo-icon",
    ":root[data-toolbar-labels=\"hidden\"] #icon-toolbar .tb-btn, :root[data-toolbar-labels=\"hidden\"] #slide-toolbar …",
    ".sb-size-group .sb-size-unit:not([hidden])",
    ".custom-select-menu",
    ".custom-select-option",
    ".custom-select-group",
    ".sb-ls-icon",
    ".sb-color-host > input[type=\"color\"]",
    ".tb-color-btn .tb-sprite",
    ".sb-hl-palette",
    ".sb-hl-palette-row",
    ".sb-hl-palette-actions",
    ".sb-hl-palette-actions button",
    ".sb-hl-swatch",
    ".eo-recent-colors .sb-hl-palette-row",
    ".tb-hf-label",
    ".sb-spacing-group",
    ".slide-presenter-notes-input",
    ".slide-context-menu.context-menu[data-mode=\"canvas\"]",
  ],
};

/** CSS written inside scripts with a literal px in those properties, per script. None yet. */
export const KNOWN_SCRIPT_SIZE_VIOLATIONS = {};

/** Arrows drawn from text glyphs (not the `▾` of the standard), as `file -> glyph -> count`. Each is an icon of ui/icons.js or a plain word in waiting. */
export const KNOWN_GLYPH_ARROWS = {
  "base-studio.js": { "▸": 1, "▾": 1 },
  "copilot-ext-ops.js": { "→": 1 },
  "file-open.js": { "→": 1 },
  "index.html": { "→": 1, "‹": 1, "›": 1 },
  "workspace-tools.js": { "→": 1 },
};

/** Control-like classes inside dialogs that predate the kit list (docs/design-system.md). A new one belongs in the kit list, with a PD, or is a kit class. */
export const KNOWN_WIDGET_CLASSES = [
  "board-analysis-grid", "cmd-input", "cmd-list", "cp-input", "cp-input-wrap", "cp-list", "doc-inspect-list",
  "icon-radio-card", "icon-radio-icon", "icon-radio-input", "icon-radio-text", "margin-grid", "outline-list",
  "sheet-check-list", "sheet-line-grid", "sheet-names-list", "sheet-tab-dialog", "sheet-tab-hidden-list",
  "sheet-table-swatch", "sheet-tc-other-input", "slide-chart-series-input", "slide-dialog-delete-btn", "symbol-cat-tab", "symbol-picker-grid",
];
