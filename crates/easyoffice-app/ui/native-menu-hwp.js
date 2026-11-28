(function (host) {
  "use strict";

  // The Hangul editor window's part of the native menu table (PD-396). The items here are commands of the editor
  // (`rhwp-studio`, MIT): they have no page action of their own (`hwp-editor`), so the native bar runs them over
  // the embed port as `automation.execute` with the editor's command id, and grays them out in every other window.
  // `shell-chrome.js` joins this list with the page actions of the shared table; a test keeps the ids equal to the
  // Rust table (`native_menu/items_hwp.rs` and the `hwp-editor` lines of `items.rs`).

  // Menu id -> the editor's command id. A shared id (`page:break`, `table:sum`) keeps its page action in the other
  // windows and means this editor command in the Hangul window.
  const EDITOR_COMMANDS = Object.freeze({
    "file:page-setup": "page:setup",
    "edit:undo": "edit:undo", "edit:redo": "edit:redo", "edit:cut": "edit:cut", "edit:copy": "edit:copy",
    "edit:format-copy": "edit:format-copy", "edit:delete": "edit:delete",
    "edit:find": "edit:find", "edit:replace": "edit:find-replace", "edit:find-again": "edit:find-again", "edit:goto": "edit:goto",
    "view:zoom-in": "view:zoom-in", "view:zoom-out": "view:zoom-out", "view:zoom-100": "view:zoom-100",
    "view:zoom-fit-page": "view:zoom-fit-page", "view:zoom-fit-width": "view:zoom-fit-width", "view:zoom-dialog": "view:zoom-dialog",
    "view:toolbar-labels": "view:toolbar-labels", "view:form-mode": "view:form-mode",
    "view:ctrl-marks": "view:ctrl-mark", "view:para-marks": "view:para-mark", "view:doc-grid": "view:toggle-grid",
    "view:border-transparent": "view:border-transparent", "view:clip": "view:toggle-clip", "view:grid-settings": "view:grid-settings",
    "view:toolbox-basic": "view:toolbox-basic", "view:toolbox-format": "view:toolbox-format",
    "insert:shape": "insert:shape", "insert:image": "insert:image", "insert:textbox": "insert:textbox", "insert:equation": "insert:equation",
    "insert:footnote": "insert:footnote", "insert:endnote": "insert:endnote", "insert:endnote-shape": "insert:endnote-shape",
    "insert:symbol": "insert:symbols", "insert:bookmark": "insert:bookmark", "insert:link": "insert:hyperlink", "insert:field": "insert:field",
    "insert:caption-top": "insert:caption-top", "insert:caption-lt": "insert:caption-lt", "insert:caption-lm": "insert:caption-lm",
    "insert:caption-lb": "insert:caption-lb", "insert:caption-rt": "insert:caption-rt", "insert:caption-rm": "insert:caption-rm",
    "insert:caption-rb": "insert:caption-rb", "insert:caption-bottom": "insert:caption-bottom", "insert:caption-none": "insert:caption-none",
    "insert:rotate-cw": "insert:rotate-cw", "insert:rotate-ccw": "insert:rotate-ccw", "insert:flip-horz": "insert:flip-horz",
    "insert:flip-vert": "insert:flip-vert", "insert:picture-delete": "insert:picture-delete",
    "format:bold": "format:bold", "format:italic": "format:italic", "format:underline": "format:underline",
    "format:strike": "format:strikethrough", "format:superscript": "format:superscript", "format:subscript": "format:subscript",
    "format:hwp-char-shape": "format:char-shape", "format:hwp-para-shape": "format:para-shape", "format:style": "format:style-dialog",
    "format:para-numbering": "format:toggle-numbering", "format:para-bullet": "format:toggle-bullet",
    "format:indent-in": "format:level-increase", "format:indent-out": "format:level-decrease",
    "format:hwp-font-up": "format:font-size-increase", "format:hwp-font-down": "format:font-size-decrease",
    "format:hwp-align-left": "format:align-left", "format:hwp-align-center": "format:align-center", "format:hwp-align-right": "format:align-right",
    "format:justify": "format:align-justify", "format:hwp-align-distribute": "format:align-distribute",
    "format:hwp-line-up": "format:line-spacing-increase", "format:hwp-line-down": "format:line-spacing-decrease",
    "format:hwp-object-props": "format:object-properties",
    "page:paper": "page:setup", "page:page-border": "page:page-border", "page:header": "page:header-create", "page:footer": "page:footer-create",
    "page:new-number": "page:new-page-num", "page:hide-current": "page:hide-current", "page:break": "page:break",
    "page:column-break": "page:column-break", "page:columns": "page:col-settings", "page:section-settings": "page:section-settings",
    "table:insert": "table:create", "table:cell-props": "table:cell-props",
    "table:border-bg": "table:border-each", "table:border-one": "table:border-one", "table:hwp-border": "table:border-each",
    "table:insert-row-col": "table:insert-row-col", "table:delete-row-col": "table:delete-row-col",
    "table:cell-select": "table:cell-selection-enter", "table:merge": "table:cell-merge", "table:cell-split": "table:cell-split",
    "table:hwp-split": "table:split", "table:row-equal": "table:cell-height-equal", "table:col-equal": "table:cell-width-equal",
    "table:transpose-copy": "table:transpose-copy", "table:transpose-paste": "table:transpose-paste",
    "table:block-formula": "table:block-formula", "table:sum": "table:block-sum", "table:block-avg": "table:block-avg",
    "table:block-product": "table:block-product", "table:thousand-sep": "table:thousand-sep",
    "table:decimal-add": "table:decimal-add", "table:decimal-remove": "table:decimal-remove",
    "tool:options": "tool:options", "tool:review-history": "edit:document-history",
    "file:set-password": "file:set-password",
  });

  // The ids that exist only for the editor (no other window has them): their page action is `hwp-editor`.
  const EDITOR_ONLY_IDS = Object.freeze([
    "view:zoom-dialog", "view:form-mode", "format:hwp-char-shape", "format:hwp-para-shape",
    "table:cell-select", "table:cell-split", "table:hwp-split",
    "edit:format-copy", "edit:delete", "edit:find-again",
    "view:border-transparent", "view:clip", "view:grid-settings", "view:toolbox-basic", "view:toolbox-format",
    "insert:textbox", "insert:equation", "insert:bookmark", "insert:endnote-shape",
    "insert:caption-top", "insert:caption-lt", "insert:caption-lm", "insert:caption-lb", "insert:caption-rt",
    "insert:caption-rm", "insert:caption-rb", "insert:caption-bottom", "insert:caption-none",
    "insert:rotate-cw", "insert:rotate-ccw", "insert:flip-horz", "insert:flip-vert", "insert:picture-delete",
    "format:hwp-object-props", "format:hwp-font-up", "format:hwp-font-down", "format:hwp-align-left", "format:hwp-align-center",
    "format:hwp-align-right", "format:hwp-align-distribute", "format:hwp-line-up", "format:hwp-line-down",
    "page:page-border", "page:new-number", "page:hide-current", "page:column-break", "page:section-settings",
    "table:insert-row-col", "table:delete-row-col", "table:border-one", "table:row-equal", "table:transpose-copy",
    "table:transpose-paste", "table:block-formula", "table:block-avg", "table:block-product", "table:thousand-sep",
    "table:decimal-add", "table:decimal-remove", "tool:options", "tool:review-history", "file:set-password",
  ]);

  host.__EASYOFFICE_NATIVE_MENU_HWP__ = Object.freeze({
    EDITOR_COMMANDS,
    ACTIONS: Object.freeze(Object.fromEntries(EDITOR_ONLY_IDS.map((id) => [id, "hwp-editor"]))),
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
