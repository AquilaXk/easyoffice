// What the static design guard (design-guard.mjs, scripts/test-design-guard.mjs) found in the tree on the day it was introduced.
// These lists only shrink: a fix removes its line (the test fails on a line that no longer breaks the rule) and the matching
// count in test-design-guard.mjs goes down with it. A new entry is not a fix. Use the tokens of tokens.css, the classes of the
// kit list in docs/design-system.md, and an icon of ui/icons.js instead.

/** Dialog and control rules with a literal px in height, min-height, font-size, line-height, gap or padding, per stylesheet (selector, cut at 110 characters). */
export const KNOWN_SIZE_VIOLATIONS = {};

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
