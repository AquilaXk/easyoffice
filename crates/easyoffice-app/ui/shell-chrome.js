(function (host) {
  "use strict";

  // Shell chrome around the embedded HWP editor. While the editor owns a window it brings its
  // own menu row, theme and status line, so the shell keeps out of the way: the editor's 파일 menu
  // forwards seven whitelisted shell commands (see hwp-host.js), the shell theme (light, dark or system)
  // is pushed into the editor, and shell notices become toasts because the shell status bar is hidden
  // (the CSS keys off `data-hwp-embed`). The About dialog holds the diagnostics that used to crowd the
  // status bar.

  const TOAST_MS = 4500;
  const TOAST_ERROR_MS = 8000;
  const ERROR_HINT = /실패|오류|거부|불가|없습니다|취소|못했습니다/;

  // The native menu bar (macOS only; `crates/easyoffice-desktop/src/native_menu.rs`, PD-102, PD-121) runs an item by calling
  // `__EASYOFFICE_NATIVE_MENU__(id)` in the focused window. Each id names a page action: the name of an
  // entry of the shortcut action table (so a menu item and its keyboard shortcut run the same handler)
  // or of `pageMenuActions` (the page's own menu item), optionally `name:argument`. A test keeps this
  // list equal to the Rust table and every item of the page's menu bar inside it.
  // The commands of the Hangul editor (`hwp-editor`) and their editor command ids are in `native-menu-hwp.js` (PD-396).
  const hwpMenu = host.__EASYOFFICE_NATIVE_MENU_HWP__;
  const NATIVE_MENU_ACTIONS = Object.freeze({
    ...hwpMenu.ACTIONS,
    "app:about": "about",
    "file:new": "file-new",
    "file:new-doc": "window-new:doc",
    "file:new-hwp": "window-new:hwp",
    "file:new-sheet": "window-new:sheet",
    "file:new-slide": "window-new:slide",
    "file:new-base": "window-new:base",
    "file:new-board": "window-new:board",
    "file:new-pdf": "window-new:pdf",
    "file:new-conv": "window-new:conv",
    "file:open": "file-open",
    "file:open-recent": "recent-dialog",
    "file:save": "studio-save",
    "file:save-as": "file-save-as",
    "file:save-as-hwpx": "hwp-save-as:hwpx",
    "file:doc-paper": "studio-cmd:layout-paper",
    "file:page-setup": "studio-cmd:layout-paper",
    "page:pages-paper": "studio-cmd:layout-paper",
    "file:export-pdf": "hwp-export-pdf",
    "file:exp-docx": "export:docx",
    "file:exp-odt": "export:odt",
    "file:exp-rtf": "export:rtf",
    "file:exp-pages": "export:pages",
    "file:exp-md": "export:md",
    "file:exp-txt": "export:txt",
    "file:exp-xlsx": "export:xlsx",
    "file:exp-ods": "export:ods",
    "file:exp-csv": "export:csv",
    "file:exp-tsv": "export:tsv",
    "file:exp-pptx": "export:pptx",
    "file:exp-odp": "export:odp",
    "file:exp-base": "export:eobase",
    "file:exp-board": "export:eoboard",
    "file:exp-pdf": "export:pdf",
    "file:exp-dlg": "export-dialog:open",
    "file:base-import": "base-act:import",
    "file:print-preview": "verify-act:print-preview",
    "file:print": "print",
    "file:close": "window-close",
    "edit:undo": "studio-cmd:studio-undo",
    "edit:redo": "studio-cmd:studio-redo",
    "edit:cut": "studio-cmd:clip-cut",
    "edit:copy": "studio-cmd:clip-copy",
    "edit:paste": "studio-cmd:clip-paste",
    "edit:paste-special": "sheet-paste:special",
    "edit:find": "find",
    "edit:replace": "replace",
    "edit:goto": "sheet-act:goto",
    "view:zoom-in": "studio-cmd:zoom-in",
    "view:zoom-out": "studio-cmd:zoom-out",
    "view:zoom-fit-page": "studio-cmd:zoom-fit-page",
    "view:zoom-fit-width": "studio-cmd:zoom-fit-width",
    "view:zoom-100": "studio-cmd:view-zoom",
    "view:split": "studio-cmd:view-split",
    "view:outline": "studio-cmd:view-outline",
    "view:pages-inspector": "pages-inspector",
    "view:theme-system": "theme:system",
    "view:theme-light": "theme:light",
    "view:theme-dark": "theme:dark",
    "view:theme-toggle": "studio-cmd:view-theme",
    "view:toolbar-labels": "view-toolbar-labels",
    "view:ctrl-marks": "studio-cmd:ctrl-marks",
    "view:para-marks": "studio-cmd:para-marks",
    "view:doc-grid": "studio-cmd:doc-grid",
    "insert:shape": "studio-cmd:insert-shape",
    "insert:image": "studio-cmd:insert-image",
    "insert:chart": "studio-cmd:insert-chart",
    "insert:footnote": "studio-cmd:mod-footnote",
    "insert:endnote": "studio-cmd:mod-endnote",
    "insert:symbol": "studio-cmd:mod-symbol",
    "insert:link": "studio-cmd:insert-link",
    "sheet:link-remove": "sheet-act:link-remove",
    "page:break": "studio-cmd:doc-mod-page-break",
    "insert:toc": "studio-cmd:doc-mod-toc",
    "insert:toc-update": "studio-cmd:doc-mod-toc-update",
    "insert:mailmerge": "studio-cmd:doc-mod-mailmerge",
    "insert:field": "studio-cmd:hwp-mod-field",
    "format:bold": "studio-cmd:home-bold",
    "format:italic": "studio-cmd:home-italic",
    "format:underline": "studio-cmd:home-underline",
    "format:strike": "studio-cmd:home-strike", "format:superscript": "studio-cmd:home-superscript", "format:subscript": "studio-cmd:home-subscript",
    "format:font-dialog": "studio-cmd:font-dialog",
    "format:style": "studio-cmd:home-style",
    "format:doc-justify": "studio-cmd:para-shape",
    "format:pages-justify": "studio-cmd:para-shape",
    "format:justify": "studio-cmd:para-shape",
    "format:para-numbering": "studio-cmd:para-numbering",
    "format:para-bullet": "studio-cmd:para-bullet",
    "format:para-checklist": "studio-cmd:para-checklist",
    "format:indent-in": "studio-cmd:indent-in",
    "format:indent-out": "studio-cmd:indent-out",
    "format:numfmt-general": "sheet-numfmt:general",
    "format:numfmt-number": "sheet-numfmt:number",
    "format:numfmt-currency": "sheet-numfmt:currency",
    "format:numfmt-accounting": "sheet-numfmt:accounting",
    "format:numfmt-percent": "sheet-numfmt:percent",
    "format:numfmt-date": "sheet-numfmt:date",
    "format:numfmt-time": "sheet-numfmt:time",
    "format:numfmt-text": "sheet-numfmt:text",
    "format:numfmt-decimal-up": "sheet-numfmt:decimal-up",
    "format:numfmt-decimal-down": "sheet-numfmt:decimal-down",
    "format:cell-format": "sheet-numfmt:dialog",
    "format:row-height": "sheet-act:row-height",
    "format:row-autofit": "sheet-act:row-autofit",
    "format:col-width": "sheet-act:col-width",
    "format:col-autofit": "sheet-act:col-autofit",
    "format:align-left": "sheet-look:align-left",
    "format:align-center": "sheet-look:align-center",
    "format:align-right": "sheet-look:align-right",
    "format:valign-top": "sheet-look:valign-top",
    "format:valign-middle": "sheet-look:valign-middle",
    "format:valign-bottom": "sheet-look:valign-bottom",
    "format:wrap": "sheet-look:wrap",
    "format:border-all": "sheet-look:border-all",
    "format:border-outer": "sheet-look:border-outer",
    "format:border-inner": "sheet-look:border-inner",
    "format:border-top": "sheet-look:border-top",
    "format:border-bottom": "sheet-look:border-bottom",
    "format:border-left": "sheet-look:border-left",
    "format:border-right": "sheet-look:border-right",
    "format:border-none": "sheet-look:border-none",
    "format:border-line-thin": "sheet-look:line-thin",
    "format:border-line-thick": "sheet-look:line-thick",
    "format:border-line-dashed": "sheet-look:line-dashed",
    "format:border-line-dotted": "sheet-look:line-dotted",
    "format:border-line-double": "sheet-look:line-double",
    "format:border-color": "sheet-look:dialog-border",
    "format:cf-quick-greater": "sheet-cf:quick-greater",
    "format:cf-quick-less": "sheet-cf:quick-less",
    "format:cf-quick-between": "sheet-cf:quick-between",
    "format:cf-quick-equal": "sheet-cf:quick-equal",
    "format:cf-quick-text": "sheet-cf:quick-text",
    "format:cf-quick-date": "sheet-cf:quick-date",
    "format:cf-quick-dup": "sheet-cf:quick-dup",
    "format:cf-more-highlight": "sheet-cf:more-highlight",
    "format:cf-quick-top": "sheet-cf:quick-top",
    "format:cf-quick-top-percent": "sheet-cf:quick-top-percent",
    "format:cf-quick-bottom": "sheet-cf:quick-bottom",
    "format:cf-quick-bottom-percent": "sheet-cf:quick-bottom-percent",
    "format:cf-quick-above": "sheet-cf:quick-above",
    "format:cf-quick-below": "sheet-cf:quick-below",
    "format:cf-more-top": "sheet-cf:more-top",
    "format:cf-bar-grad-blue": "sheet-cf:bar-grad-blue",
    "format:cf-bar-grad-green": "sheet-cf:bar-grad-green",
    "format:cf-bar-grad-red": "sheet-cf:bar-grad-red",
    "format:cf-bar-grad-orange": "sheet-cf:bar-grad-orange",
    "format:cf-bar-grad-lightblue": "sheet-cf:bar-grad-lightblue",
    "format:cf-bar-grad-purple": "sheet-cf:bar-grad-purple",
    "format:cf-bar-solid-blue": "sheet-cf:bar-solid-blue",
    "format:cf-bar-solid-green": "sheet-cf:bar-solid-green",
    "format:cf-bar-solid-red": "sheet-cf:bar-solid-red",
    "format:cf-bar-solid-orange": "sheet-cf:bar-solid-orange",
    "format:cf-bar-solid-lightblue": "sheet-cf:bar-solid-lightblue",
    "format:cf-bar-solid-purple": "sheet-cf:bar-solid-purple",
    "format:cf-more-bar": "sheet-cf:more-bar",
    "format:cf-scale-gyr": "sheet-cf:scale-gyr",
    "format:cf-scale-ryg": "sheet-cf:scale-ryg",
    "format:cf-scale-gwr": "sheet-cf:scale-gwr",
    "format:cf-scale-rwg": "sheet-cf:scale-rwg",
    "format:cf-scale-bwr": "sheet-cf:scale-bwr",
    "format:cf-scale-rwb": "sheet-cf:scale-rwb",
    "format:cf-scale-wr": "sheet-cf:scale-wr",
    "format:cf-scale-rw": "sheet-cf:scale-rw",
    "format:cf-scale-gw": "sheet-cf:scale-gw",
    "format:cf-scale-wg": "sheet-cf:scale-wg",
    "format:cf-scale-gy": "sheet-cf:scale-gy",
    "format:cf-scale-yg": "sheet-cf:scale-yg",
    "format:cf-more-scale": "sheet-cf:more-scale",
    "format:cf-icons-3arrows": "sheet-cf:icons-3arrows",
    "format:cf-icons-3arrowsgray": "sheet-cf:icons-3arrowsgray",
    "format:cf-icons-3triangles": "sheet-cf:icons-3triangles",
    "format:cf-icons-4arrowsgray": "sheet-cf:icons-4arrowsgray",
    "format:cf-icons-4arrows": "sheet-cf:icons-4arrows",
    "format:cf-icons-5arrowsgray": "sheet-cf:icons-5arrowsgray",
    "format:cf-icons-5arrows": "sheet-cf:icons-5arrows",
    "format:cf-icons-3trafficlights1": "sheet-cf:icons-3trafficlights1",
    "format:cf-icons-3trafficlights2": "sheet-cf:icons-3trafficlights2",
    "format:cf-icons-3signs": "sheet-cf:icons-3signs",
    "format:cf-icons-4trafficlights": "sheet-cf:icons-4trafficlights",
    "format:cf-icons-4redtoblack": "sheet-cf:icons-4redtoblack",
    "format:cf-icons-3symbols": "sheet-cf:icons-3symbols",
    "format:cf-icons-3symbols2": "sheet-cf:icons-3symbols2",
    "format:cf-icons-3flags": "sheet-cf:icons-3flags",
    "format:cf-icons-3stars": "sheet-cf:icons-3stars",
    "format:cf-icons-4rating": "sheet-cf:icons-4rating",
    "format:cf-icons-5rating": "sheet-cf:icons-5rating",
    "format:cf-icons-5quarters": "sheet-cf:icons-5quarters",
    "format:cf-icons-5boxes": "sheet-cf:icons-5boxes",
    "format:cf-more-icons": "sheet-cf:more-icons",
    "format:cf-new": "sheet-cf:new",
    "format:cf-clear-selection": "sheet-cf:clear-selection",
    "format:cf-clear-sheet": "sheet-cf:clear-sheet",
    "format:cf-clear-table": "sheet-cf:clear-table",
    "format:cf-clear-pivot": "sheet-cf:clear-pivot",
    "format:cf-manage": "sheet-cf:manage",
    "page:doc-paper": "studio-cmd:layout-paper",
    "page:paper": "studio-cmd:layout-paper",
    "page:orientation": "studio-cmd:layout-orientation",
    "page:margins": "studio-cmd:layout-margins",
    "page:doc-header": "studio-cmd:mod-header",
    "page:header": "studio-cmd:mod-header",
    "page:doc-footer": "studio-cmd:mod-footer",
    "page:footer": "studio-cmd:mod-footer",
    "page:columns": "studio-cmd:layout-columns",
    "page:hwp-header": "studio-cmd:hwp-mod-header",
    "page:manuscript": "studio-cmd:hwp-mod-manuscript",
    "page:slide-master": "studio-cmd:slide-mod-master",
    "page:slide-snap": "studio-cmd:slide-mod-snap",
    "page:slide-transition": "studio-cmd:slide-mod-transition",
    "page:slide-presenter": "studio-cmd:slide-mod-presenter",
    "slide:insert-textbox": "slide-act:insert-textbox",
    "slide:insert-picture": "slide-act:insert-picture",
    "slide:bring-front": "slide-act:bring-front",
    "slide:bring-forward": "slide-act:bring-forward", "slide:send-backward": "slide-act:send-backward",
    "slide:send-back": "slide-act:send-back",
    "slide:align-left": "slide-act:align-left", "slide:align-center": "slide-act:align-center", "slide:align-right": "slide-act:align-right",
    "slide:align-top": "slide-act:align-top", "slide:align-middle": "slide-act:align-middle", "slide:align-bottom": "slide-act:align-bottom",
    "slide:distribute-h": "slide-act:distribute-h", "slide:distribute-v": "slide-act:distribute-v",
    "slide:delete-element": "slide-act:delete-element",
    "sheet:insert-picture": "sheet-act:insert-picture",
    "sheet:note-insert": "sheet-act:note-insert",
    "sheet:note-toggle-all": "sheet-act:note-toggle-all",
    "table:insert": "studio-cmd:insert-table",
    "table:row-above": "studio-cmd:table-row-above",
    "table:add-row": "studio-cmd:table-add-row",
    "table:col-left": "studio-cmd:table-col-left",
    "table:col-right": "studio-cmd:table-col-right",
    "table:row-delete": "studio-cmd:table-row-delete",
    "table:col-delete": "studio-cmd:table-col-delete",
    "table:split": "studio-cmd:table-split",
    "table:merge": "studio-cmd:table-merge",
    "table:col-equal": "studio-cmd:table-col-equal",
    "table:border-bg": "studio-cmd:table-border-bg",
    "table:cell-props": "studio-cmd:table-cell-props",
    "table:sum": "studio-cmd:table-sum",
    "table:doc-formula": "studio-cmd:doc-mod-tableformula",
    "table:hwp-border": "studio-cmd:hwp-mod-border",
    "table:autosum": "studio-cmd:data-autosum",
    "table:fn-browser": "studio-cmd:data-fn-browser",
    "table:sort": "studio-cmd:data-sort",
    "table:validation": "studio-cmd:data-validation",
    "table:sheet-pivot": "studio-cmd:sheet-mod-pivot",
    "table:sheet-condfmt": "studio-cmd:sheet-mod-condfmt",
    "table:sheet-freeze": "studio-cmd:sheet-mod-freeze",
    "sheet:row-insert-above": "sheet-act:row-insert-above",
    "sheet:col-insert-left": "sheet-act:col-insert-left",
    "sheet:row-delete": "sheet-act:row-delete",
    "sheet:col-delete": "sheet-act:col-delete",
    "sheet:row-hide": "sheet-act:row-hide",
    "sheet:row-unhide": "sheet-act:row-unhide",
    "sheet:col-hide": "sheet-act:col-hide",
    "sheet:col-unhide": "sheet-act:col-unhide",
    "sheet:merge": "sheet-act:merge",
    "sheet:unmerge": "sheet-act:unmerge",
    "sheet:view-formula-bar": "sheet-act:view-formula-bar",
    "sheet:view-headings": "sheet-act:view-headings",
    "sheet:freeze-first-row": "sheet-act:freeze-first-row",
    "sheet:freeze-first-col": "sheet-act:freeze-first-col",
    "sheet:freeze-clear": "sheet-act:freeze-clear",
    "sheet:name-define": "sheet-act:name-define",
    "sheet:name-manage": "sheet-act:name-manage",
    "sheet:calc-auto": "sheet-act:calc-auto",
    "sheet:calc-auto-no-table": "sheet-act:calc-auto-no-table",
    "sheet:calc-manual": "sheet-act:calc-manual",
    "sheet:calc-now": "sheet-act:calc-now",
    "sheet:calc-sheet": "sheet-act:calc-sheet",
    "sheet:protect": "sheet-act:sheet-protect",
    "sheet:protect-workbook": "sheet-act:workbook-protect",
    "sheet:protect-ranges": "sheet-act:range-allow",
    "sheet:name-create": "sheet-act:name-create",
    "sheet:name-paste": "sheet-act:name-paste",
    "sheet:page-setup": "sheet-act:page-setup",
    "sheet:break-insert": "sheet-act:break-insert",
    "sheet:break-remove": "sheet-act:break-remove",
    "sheet:break-reset": "sheet-act:break-reset",
    "sheet:print-area-set": "sheet-act:print-area-set",
    "sheet:print-area-clear": "sheet-act:print-area-clear",
    "sheet:tab-insert": "sheet-act:tab-insert",
    "sheet:tab-delete": "sheet-act:tab-delete",
    "sheet:tab-rename": "sheet-act:tab-rename",
    "sheet:tab-move-copy": "sheet-act:tab-move-copy",
    "sheet:tab-hide": "sheet-act:tab-hide",
    "sheet:tab-unhide": "sheet-act:tab-unhide",
    "sheet:tab-color": "sheet-act:tab-color",
    "sheet:group": "sheet-act:group",
    "sheet:ungroup": "sheet-act:ungroup",
    "sheet:hide-detail": "sheet-act:hide-detail",
    "sheet:show-detail": "sheet-act:show-detail",
    "sheet:outline-clear": "sheet-act:outline-clear",
    "sheet:subtotal": "sheet-act:subtotal",
    "sheet:sort": "sheet-act:sort-dialog",
    "sheet:filter": "sheet-act:filter-toggle",
    "sheet:filter-reapply": "sheet-act:filter-reapply",
    "sheet:filter-clear": "sheet-act:filter-clear",
    "sheet:remove-duplicates": "sheet-act:dedupe-dialog",
    "sheet:text-to-columns": "sheet-act:text-to-columns",
    "tool:copilot": "studio-cmd:copilot-toggle",
    "tool:diagnose": "studio-cmd:smart-diagnose",
    "tool:scrub-pii": "studio-cmd:scrub-pii",
    "tool:review-verify": "studio-cmd:review-verify",
    "tool:doc-inspect": "verify-act:doc-inspect",
    "tool:bench-run": "studio-cmd:bench-run",
    "tool:review-track": "studio-cmd:review-track",
    "tool:review-accept-all": "studio-cmd:review-accept-all",
    "tool:review-reject-all": "studio-cmd:review-reject-all",
    "tool:review-comment": "studio-cmd:review-comment",
    "tool:review-stats": "studio-cmd:review-stats",
    "tool:pdf-highlight": "studio-cmd:pdf-mod-highlight",
    "tool:pdf-rotate": "studio-cmd:pdf-mod-rotate",
    "tool:pdf-splitmerge": "studio-cmd:pdf-mod-splitmerge",
    "tool:pdf-tooffice": "studio-cmd:pdf-mod-tooffice",
    "tool:palette": "studio-cmd:palette",
    "help:update-check": "update-check",
  });

  // Embed-only Hangul window items (disabled outside the Hangul embed).
  const EMBED_ONLY_ACTIONS = Object.freeze(["hwp-export-pdf", "hwp-save-as"]);
  const EMBED_ONLY_SET = new Set(EMBED_ONLY_ACTIONS);
  const START_DISABLED_ACTIONS = Object.freeze(["studio-save", "file-save-as", "export", "export-dialog", "find", "replace"]);
  const START_DISABLED_SET = new Set(START_DISABLED_ACTIONS);
  const PARSED_NATIVE_MENU_ACTIONS = Object.freeze(
    Object.fromEntries(
      Object.entries(NATIVE_MENU_ACTIONS).map(([id, entry]) => {
        const [name, argument] = entry.split(":");
        return [id, Object.freeze({ id, name, argument })];
      })
    )
  );
  const PARSED_NATIVE_MENU_LIST = Object.freeze(Object.values(PARSED_NATIVE_MENU_ACTIONS));

  // Menu id -> Hangul editor command id (`automation.execute`).
  const EMBED_EDITOR_COMMANDS = hwpMenu.EDITOR_COMMANDS;

  // Same items when a plain text field has focus.
  const TEXT_FIELD_EDIT_COMMANDS = Object.freeze({ "edit:undo": "undo", "edit:redo": "redo", "edit:cut": "cut", "edit:copy": "copy" });

  // Actions that click the page's menu item from `NATIVE_MENU_ACTIONS`.
  function pageMenuActions(document) {
    const click = (selector) => document.querySelector(selector)?.click();
    const clickVisible = (selector) => {
      for (const el of document.querySelectorAll(selector)) {
        if (!el.hidden && !el.closest("[hidden]")) { el.click(); return; }
      }
      click(selector);
    };
    return {
      "studio-cmd": (cmd) => clickVisible(`.md-item[data-studio-menu-cmd="${cmd}"]`),
      "verify-act": (act) => click(`.md-item[data-verify-act="${act}"]`),
      "base-act": (act) => click(`.md-item[data-base-act="${act}"]`),
      "slide-act": (act) => click(`.md-item[data-slide-act="${act}"]`),
      "sheet-act": (act) => click(`.md-item[data-sheet-act="${act}"]`),
      "sheet-numfmt": (action) => click(`.md-item[data-sheet-numfmt="${action}"]`),
      "sheet-paste": (action) => click(`.md-item[data-sheet-paste="${action}"]`),
      "sheet-look": (action) => click(`.md-item[data-sheet-look="${action}"]`),
      "sheet-cf": (action) => click(`.md-item[data-sheet-cf="${action}"]`),
      "hwp-save-as": (fmt) => click(`.md-item[data-hwp-save-as="${fmt}"]`),
      export: (fmt) => clickVisible(`.md-item[data-export-format="${fmt}"]`),
      "export-dialog": () => clickVisible(".md-item[data-export-dialog]"),
      "pages-inspector": () => click("[data-pages-inspector-toggle]"),
      "view-toolbar-labels": () => click("[data-view-toolbar-labels]"),
      "window-new": (kind) => click(`[data-window-new="${kind.toUpperCase()}"]`),
      theme: (mode) => click(`[data-theme-mode-choice="${mode}"]`),
      "update-check": () => click("[data-update-check-toggle]"),
    };
  }

  // Command palette entries built from the sheet page menu bar items.
  function sheetMenuPaletteItems(document) {
    const seen = new Set();
    const out = [];
    for (const el of document.querySelectorAll("#menu-bar .md-item[data-sheet-act], #menu-bar .md-item[data-sheet-numfmt], #menu-bar .md-item[data-sheet-paste], #menu-bar .md-item[data-sheet-look], #menu-bar .md-item[data-sheet-cf]")) {
      const id = `sheet-menu:${el.dataset.sheetAct || el.dataset.sheetNumfmt || el.dataset.sheetPaste || el.dataset.sheetLook || "cf-" + el.dataset.sheetCf}`;
      if (seen.has(id)) continue;
      seen.add(id);
      const title = el.dataset.paletteTitle || el.querySelector(".md-label").textContent;
      const shortcut = el.querySelector(".md-shortcut")?.textContent || "";
      const kind = el.dataset.sheetCf === undefined ? "서식 시트" : `서식 시트 조건부 서식 ${el.parentElement?.closest?.(".md-sub-panel")?.getAttribute("aria-label") ?? ""}`;
      out.push({ id, module: "SHEET", title, shortcut, searchKey: `${kind} ${title}`, run: () => el.click() });
    }
    return out;
  }

  const MODAL_GATED_NATIVE_IDS = new Set([
    "file:exp-dlg", "file:print-preview", "file:base-import", "file:doc-paper",
    "edit:undo", "edit:redo", "edit:cut", "edit:copy", "edit:paste-special", "edit:find", "edit:replace",
    "insert:shape", "insert:image", "insert:chart", "insert:symbol", "insert:link", "insert:toc", "insert:mailmerge",
    "format:bold", "format:italic", "format:underline", "format:strike", "format:superscript", "format:subscript",
    "format:cell-format", "format:row-height", "format:row-autofit", "format:col-width", "format:col-autofit", "format:border-color",
    "sheet:sort", "sheet:remove-duplicates", "sheet:text-to-columns", "sheet:page-setup", "sheet:protect", "sheet:protect-workbook", "sheet:protect-ranges",
    "sheet:name-define", "sheet:name-manage", "sheet:name-create", "sheet:name-paste", "sheet:tab-delete", "sheet:tab-rename", "sheet:tab-move-copy", "sheet:tab-unhide",
    "slide:bring-forward", "slide:send-backward", "slide:align-left", "slide:align-center", "slide:align-right", "slide:align-top", "slide:align-middle", "slide:align-bottom", "slide:distribute-h", "slide:distribute-v",
    "page:doc-paper", "page:paper", "page:margins", "page:columns", "page:slide-transition",
    "table:insert", "table:sort", "table:validation", "table:fn-browser", "table:doc-formula", "table:border-bg", "table:cell-props", "table:sheet-pivot", "table:sheet-condfmt", "table:sheet-freeze", "table:hwp-border",
    "tool:diagnose", "tool:scrub-pii", "tool:doc-inspect", "tool:review-comment", "tool:review-stats", "tool:pdf-highlight", "tool:pdf-rotate", "tool:pdf-splitmerge", "tool:pdf-tooffice", "tool:palette",
  ]);
  const FIELD_GATED_NATIVE_IDS = new Set([
    "insert:link", "edit:paste-special", "format:bold", "format:italic", "format:underline", "format:strike", "format:superscript", "format:subscript",
    "format:cell-format", "format:row-height", "format:row-autofit", "format:col-width", "format:col-autofit",
    "sheet:remove-duplicates", "sheet:text-to-columns", "slide:bring-forward", "slide:send-backward",
    "slide:align-left", "slide:align-center", "slide:align-right", "slide:align-top", "slide:align-middle", "slide:align-bottom",
    "slide:distribute-h", "slide:distribute-v",
  ]);
  const PRINT_PREVIEW_MODULES = new Set(["DOC", "SHEET", "SLIDE", "BASE", "BOARD", "PDF"]);

  function nativeMenuModuleApplies(id, commandModule) {
    if (!commandModule) return true;
    const p = Object.hasOwn(PARSED_NATIVE_MENU_ACTIONS, id) ? PARSED_NATIVE_MENU_ACTIONS[id] : null;
    if (!p) return true;
    if ((commandModule === "START" || commandModule === "CONV") && (p.name === "export" || p.name === "export-dialog")) return false;
    if (commandModule === "START" && START_DISABLED_SET.has(p.name)) return false;
    if (commandModule === "HWP_EMBED" && (Object.hasOwn(EMBED_EDITOR_COMMANDS, id) || EMBED_ONLY_SET.has(p.name) || p.name === "hwp-editor")) return true;
    if (p.name === "hwp-save-as" || p.name === "hwp-editor") return false;
    if (p.name === "verify-act") return p.argument === "doc-inspect" ? (commandModule === "DOC" || commandModule === "SLIDE") : PRINT_PREVIEW_MODULES.has(commandModule);
    if (p.name.startsWith("sheet-")) return commandModule === "SHEET";
    if (p.name === "slide-act") return commandModule === "SLIDE";
    if (p.name === "base-act") return commandModule === "BASE";
    const applies = host.__EASYOFFICE_WORKSPACE_SESSION_UTILS__?.studioCommandApplies;
    if (p.name === "print") return commandModule === "HWP_EMBED" || typeof applies !== "function" || applies("print", commandModule);
    return p.name !== "studio-cmd" || typeof applies !== "function" || applies(p.argument, commandModule);
  }

  function allowsNativeMenuContext(id, document, modal, activeModule) {
    if (MODAL_GATED_NATIVE_IDS.has(id) || id.startsWith("format:cf-")) {
      const hasModal = Boolean(modal?.open) || Array.from(document.querySelectorAll?.("dialog") || []).some((dlg) => dlg.open && !dlg.classList?.contains?.("find-dialog"));
      if (hasModal) return false;
    }
    const tag = document.activeElement?.tagName;
    if ((tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") && FIELD_GATED_NATIVE_IDS.has(id)) return false;
    const appDs = document.getElementById?.("app")?.dataset;
    const cmdMod = appDs?.windowKind === "start" ? "START" : (activeModule === "HWP" && appDs?.hwpEmbed === "true") ? "HWP_EMBED" : activeModule;
    return nativeMenuModuleApplies(id, cmdMod);
  }

  // Runs the page action of a menu id; false for an unknown id or an action the page does not have.
  function runNativeMenuAction(id, actions) {
    const parsed = Object.hasOwn(PARSED_NATIVE_MENU_ACTIONS, id) ? PARSED_NATIVE_MENU_ACTIONS[id] : null;
    if (!parsed) return false;
    const run = actions[parsed.name];
    if (typeof run !== "function") return false;
    run(parsed.argument, id);
    return true;
  }

  function createShellChrome(deps) {
    const { document, hwpHost } = deps;
    const env = deps.env || host;
    const actions = deps.actions || {};
    let toastTimer = null;

    const report = (promise) => {
      Promise.resolve(promise).catch((err) => toast((err?.message || String(err))));
    };

    // ---- theme -----------------------------------------------------------------------------

    const themeApi = () => host.__EASYOFFICE_THEME__;

    function currentTheme() {
      return themeApi().getEffective();
    }

    // Applies the stored theme mode (the first paint already follows it) and keeps the embedded
    // editor on the same mode: it follows every later change at once.
    function applyStoredTheme() {
      const api = themeApi();
      api.subscribe((effective, mode) => {
        if (deps.ribbonState) deps.ribbonState.darkThemeEnabled = effective === "dark";
        deps.icons?.syncThemeToggle(document, effective);
        report(hwpHost.setShellContext({ theme: mode }));
        pushMenuState();
      });
      api.init();
    }

    // ---- native menu state ---------------------------------------------------------------------

    // The native menu bar (macOS only, PD-121) is shared by all windows, so each window tells the shell
    // what the bar should show while it has the focus (`menu_state.rs`): its menu kind (which menus the bar
    // has, `menu-kinds.js`), its theme mode and the items its module cannot use (the page's own rule,
    // `studioCommandApplies`, applied to the table above).
    let menuContext = null;
    let lastMenuState = "";

    function disabledMenuIds(commandModule) {
      const applies = host.__EASYOFFICE_WORKSPACE_SESSION_UTILS__.studioCommandApplies;
      const out = [];
      for (let i = 0; i < PARSED_NATIVE_MENU_LIST.length; i += 1) {
        const { id, name, argument } = PARSED_NATIVE_MENU_LIST[i];
        let isDisabled;
        if ((commandModule === "START" || commandModule === "CONV") && (name === "export" || name === "export-dialog")) {
          isDisabled = true;
        } else if (commandModule === "START" && START_DISABLED_SET.has(name)) {
          isDisabled = true;
        } else if (Object.hasOwn(EMBED_EDITOR_COMMANDS, id) && commandModule === "HWP_EMBED") {
          isDisabled = false;
        } else if (EMBED_ONLY_SET.has(name)) {
          isDisabled = commandModule !== "HWP_EMBED";
        } else if (commandModule === "SHEET" && (id === "format:cf-clear-table" || id === "format:cf-clear-pivot")) {
          isDisabled = document.querySelector(`.md-item[data-sheet-cf="${argument}"]`)?.getAttribute("aria-disabled") === "true";
        } else {
          isDisabled = name === "hwp-editor" || (name === "studio-cmd" && !applies(argument, commandModule)) || (name === "print" && !applies("print", commandModule));
        }
        if (isDisabled) out.push(id);
      }
      return out;
    }

    const TOOLBAR_LABELS_KEY = "toolbar-labels";
    function readToolbarLabels() { try { return env.localStorage?.getItem(TOOLBAR_LABELS_KEY) !== "hidden"; } catch { return true; } }
    function applyToolbarLabels(visible) {
      const shown = Boolean(visible);
      if (document.documentElement?.dataset) document.documentElement.dataset.toolbarLabels = shown ? "shown" : "hidden";
      for (const el of document.querySelectorAll("[data-view-toolbar-labels]")) { el.classList?.toggle?.("active", shown); el.setAttribute("aria-checked", String(shown)); }
    }
    function saveToolbarLabels(visible) {
      const shown = Boolean(visible);
      applyToolbarLabels(shown);
      try { env.localStorage?.setItem(TOOLBAR_LABELS_KEY, shown ? "shown" : "hidden"); } catch {}
      pushMenuState();
      return shown;
    }
    function toggleToolbarLabels() {
      const next = saveToolbarLabels(document.documentElement?.dataset?.toolbarLabels === "hidden");
      report(hwpHost?.setShellContext?.({ toolbarLabels: next }));
      return next;
    }

    // Without a new module context the last one is reused (a theme change); nothing is sent before the
    // first one, and an unchanged state is not sent twice.
    function pushMenuState(commandModule) {
      const bridge = deps.desktopBridge || host.__EASYOFFICE_DESKTOP_BRIDGE__;
      if (commandModule !== undefined) menuContext = { commandModule };
      if (!menuContext || !bridge?.isAvailable() || !deps.hasNativeMenu()) return;
      const active = menuContext.commandModule;
      const toolbarLabels = document.documentElement?.dataset?.toolbarLabels !== "hidden";
      const state = { theme: themeApi().getMode(), disabled: disabledMenuIds(active), kind: host.__EASYOFFICE_MENU_KINDS__.menuKindOf(active, document), toolbarLabels };
      const key = JSON.stringify(state);
      if (key === lastMenuState) return;
      lastMenuState = key;
      // A report that did not arrive is sent again the next time.
      report(bridge.windowCommand("reportMenuState", state).catch((err) => {
        lastMenuState = "";
        throw err;
      }));
    }

    // ---- toasts (shell notices while the shell status bar is hidden) -------------------------

    const embedActive = () => document.getElementById("app")?.dataset?.hwpEmbed === "true";

    function toast(message) {
      const text = String(message || "").trim();
      const el = document.getElementById("shell-toast");
      if (!el || !text) return;
      el.textContent = text;
      el.hidden = false;
      if (toastTimer !== null) env.clearTimeout(toastTimer);
      toastTimer = env.setTimeout(() => {
        el.hidden = true;
        toastTimer = null;
      }, ERROR_HINT.test(text) ? TOAST_ERROR_MS : TOAST_MS);
    }

    // Every module writes its notices into the status line; while the editor owns the window the same
    // text is also shown as a toast. The element keeps working as before for every other window.
    function mirrorFeedbackToToast() {
      const fb = document.getElementById("status-command-feedback");
      if (!fb) return false;
      let holder = fb;
      let descriptor = null;
      while (holder && !descriptor) {
        descriptor = Object.getOwnPropertyDescriptor(holder, "textContent");
        holder = descriptor ? holder : Object.getPrototypeOf(holder);
      }
      if (!descriptor) return false;
      let stored = descriptor.value;
      const read = descriptor.get ? () => descriptor.get.call(fb) : () => stored;
      const write = descriptor.set
        ? (value) => descriptor.set.call(fb, value)
        : (value) => {
            stored = value;
          };
      Object.defineProperty(fb, "textContent", {
        configurable: true,
        get: read,
        set(value) {
          write(value);
          if (embedActive()) toast(value);
        },
      });
      return true;
    }

    // ---- editor 파일 menu --------------------------------------------------------------------

    // Runs a command the editor forwarded. `hwp-host.js` has already checked it against the whitelist.
    function runShellCommand(request) {
      switch (request?.command) {
        case "new": return report(actions.newDocument?.());
        case "open": return report(actions.openFile?.());
        case "recent": return report(actions.openRecent?.(request.id));
        case "recent-dialog": return report(actions.recentDialog?.());
        case "save": return report(actions.save?.());
        case "save-as": return report(actions.saveAs?.(request.format));
        case "export-pdf": return report(actions.exportPdf?.());
        case "print": return report(actions.print?.());
        case "close": return report(actions.close?.());
        case "about": return openAbout();
        case "theme-system": return themeApi().setMode("system");
        case "theme-light": return themeApi().setMode("light");
        case "theme-dark": return themeApi().setMode("dark");
        case "theme-toggle": return themeApi().toggle();
        default: return undefined;
      }
    }

    function pushRecents(entries) {
      return hwpHost.setShellContext({ recents: Array.isArray(entries) ? entries : [] });
    }

    // ---- About dialog -------------------------------------------------------------------------

    function openAbout() {
      const dialog = document.getElementById("about-dialog");
      if (dialog && typeof dialog.showModal === "function" && !dialog.open) dialog.showModal();
    }

    function bindAbout() {
      const dialog = document.getElementById("about-dialog");
      for (const el of document.querySelectorAll("[data-about-open]")) {
        el.addEventListener("click", (ev) => {
          if (ev.stopPropagation) ev.stopPropagation();
          host.__EASYOFFICE_CLOSE_MENUS__(document);
          if (dialog && typeof dialog.showModal === "function" && !dialog.open) dialog.showModal();
        });
      }
      const closeBtn = document.getElementById("btn-about-close");
      if (closeBtn && dialog) closeBtn.addEventListener("click", () => dialog.close?.());
    }

    // "Version x.y.z" under the product name (the reference app's wording).
    function setAppVersion(version) {
      const el = document.getElementById("about-version");
      const text = String(version || "").trim();
      if (!el || !/^[0-9A-Za-z.+-]{1,32}$/.test(text)) return;
      el.textContent = "Version " + text;
      el.hidden = false;
    }

    function bindIconToolbarScroller() {
      host.__EASYOFFICE_TOOLBAR_SCROLL__?.attachAll?.(document);
    }

    // Dragging by the title bar (reference `dialog-drag.ts`); the position stays after the dialog is closed.
    function enableDialogDrag(dialog) {
      const wrap = dialog?.querySelector?.(".dialog-wrap");
      const title = dialog?.querySelector?.(".dialog-title");
      if (!wrap || !title) return;
      let dx = 0, dy = 0, dragging = false, startX = 0, startY = 0;
      const onMove = (ev) => {
        if (!dragging) return;
        dx = (Number(ev.clientX) || 0) - startX;
        dy = (Number(ev.clientY) || 0) - startY;
        wrap.style.transform = `translate(${dx}px, ${dy}px)`;
      };
      const stopDrag = () => {
        dragging = false;
        document.removeEventListener?.("mousemove", onMove);
        document.removeEventListener?.("mouseup", stopDrag);
      };
      title.addEventListener("mousedown", (ev) => {
        if (ev.button !== undefined && ev.button !== 0) return;
        if (ev.target?.closest?.(".dialog-close, button, [data-dialog-close]")) return;
        startX = (Number(ev.clientX) || 0) - dx;
        startY = (Number(ev.clientY) || 0) - dy;
        dragging = true;
        document.addEventListener?.("mousemove", onMove);
        document.addEventListener?.("mouseup", stopDrag);
        ev.preventDefault?.();
      });
      dialog.addEventListener?.("close", stopDrag);
    }

    // Keys while a tool dialog is open, and its first focus (reference `dialog.ts` 95-123, MIT, see
    // THIRD_PARTY_NOTICES.md): Esc closes it, Enter outside a text field presses the primary button,
    // and every other key stays inside (typing in a field is untouched) so editor shortcuts such as
    // Ctrl+Z never reach the document behind it. The focus starts on the primary button, so the
    // browser does not put it on the title bar's close button and draw a focus ring there. The find
    // dialog is the reference's own non-modal dialog and keeps its own keys and focus (`find-dialog.ts`).
    const NON_TEXT_INPUT_TYPES = new Set(["checkbox", "radio", "button", "submit", "reset", "color", "range", "file", "image"]);
    const BUTTON_INPUT_TYPES = new Set(["button", "submit", "reset"]);
    const KEY_BOUND_DIALOGS = new WeakSet();
    const openDialogStack = [];
    const FOCUSABLE_SELECTOR = 'button, a[href], input, select, textarea, [tabindex]';

    const pruneOpenDialogStack = () => {
      for (let i = openDialogStack.length - 1; i >= 0; i -= 1) {
        const dlg = openDialogStack[i];
        if (!dlg || !dlg.open || dlg.isConnected === false) openDialogStack.splice(i, 1);
      }
    };
    const removeOpenDialogStack = (dialog) => {
      const idx = openDialogStack.indexOf(dialog);
      if (idx >= 0) openDialogStack.splice(idx, 1);
      pruneOpenDialogStack();
    };
    const pushOpenDialogStack = (dialog) => { removeOpenDialogStack(dialog); openDialogStack.push(dialog); };
    const topOpenDialog = () => { pruneOpenDialogStack(); return openDialogStack.at(-1) || null; };

    function inputTypeOf(el) {
      return String(el.type || el.getAttribute?.("type") || "text").toLowerCase();
    }

    function isTextInputTarget(el) {
      if (!el || el.nodeType !== 1) return false;
      if (el.isContentEditable === true || el.getAttribute?.("contenteditable") === "plaintext-only") return true;
      const tag = String(el.tagName || "").toUpperCase();
      if (tag === "TEXTAREA") return true;
      if (tag === "INPUT") return !NON_TEXT_INPUT_TYPES.has(inputTypeOf(el));
      return false;
    }

    function isButtonLikeTarget(el) {
      if (!el || el.nodeType !== 1) return false;
      const tag = String(el.tagName || "").toUpperCase();
      if (tag === "BUTTON" || tag === "SUMMARY") return true;
      if (tag === "A" && el.hasAttribute?.("href")) return true;
      if (tag === "INPUT") return BUTTON_INPUT_TYPES.has(inputTypeOf(el));
      const role = el.getAttribute?.("role");
      return role === "button" || role === "menuitem" || role === "option" || role === "tab";
    }

    function dialogFocusables(dialog) {
      const getCs = typeof host.getComputedStyle === "function"
        ? (node) => host.getComputedStyle(node)
        : null;
      return Array.from(dialog.querySelectorAll(FOCUSABLE_SELECTOR)).filter((el) => {
        if (!el || el.disabled || el.hidden) return false;
        if (el.getAttribute?.("aria-disabled") === "true" || el.getAttribute?.("aria-hidden") === "true") return false;
        if (String(el.tagName || "").toUpperCase() === "INPUT" && inputTypeOf(el) === "hidden") return false;
        if (el.classList?.contains?.("native-select-hidden")) return false;
        const selectMenu = el.closest?.(".custom-select-menu");
        if (selectMenu && !selectMenu.closest?.(".custom-select.open")) return false;
        const hiddenAncestor = el.closest?.("[hidden]");
        if (hiddenAncestor && hiddenAncestor !== dialog && dialog.contains(hiddenAncestor)) return false;
        if (getCs) {
          const selfCs = getCs(el);
          if (selfCs && selfCs.visibility === "hidden") return false;
          for (let cur = el; cur && cur !== dialog; cur = cur.parentElement) {
            const curCs = getCs(cur);
            if (curCs && curCs.display === "none") return false;
          }
        }
        const tabAttr = el.getAttribute?.("tabindex");
        if (tabAttr !== null && tabAttr !== undefined && Number(tabAttr) < 0) return false;
        return typeof el.focus === "function";
      });
    }

    function enableDialogKeys(dialog) {
      if (!dialog || dialog.classList.contains("find-dialog") || KEY_BOUND_DIALOGS.has(dialog)) return;
      KEY_BOUND_DIALOGS.add(dialog);
      const activeKeyEvents = typeof WeakSet === "function" ? new WeakSet() : null;
      const popupEscapeEvents = typeof WeakSet === "function" ? new WeakSet() : null;
      const insideDialog = (target) => (
        target === dialog
        || (typeof dialog.contains === "function" ? dialog.contains(target) : target?.closest?.("dialog") === dialog)
      );
      const hasOpenNestedPopup = () => Boolean(
        dialog.querySelector?.(".custom-select.open, .sb-hl-palette:not([hidden])")
      );
      const isTopActive = () => {
        if (!dialog.open) return false;
        const top = topOpenDialog();
        return !top || top === dialog;
      };
      const cycleTabFocus = (ev) => {
        const list = dialogFocusables(dialog);
        if (list.length === 0) {
          ev.preventDefault();
          dialog.tabIndex = -1;
          dialog.focus();
          return;
        }
        const active = document.activeElement;
        if (!insideDialog(active) || !list.includes(active)) {
          ev.preventDefault();
          (ev.shiftKey ? list[list.length - 1] : list[0]).focus();
          return;
        }
        if (ev.shiftKey && active === list[0]) {
          ev.preventDefault();
          list[list.length - 1].focus();
        } else if (!ev.shiftKey && active === list[list.length - 1]) {
          ev.preventDefault();
          list[0].focus();
        }
      };
      const capture = (ev) => {
        if (!isTopActive() || ev.isComposing) return;
        if (ev.key === "Escape") {
          if (ev.defaultPrevented || hasOpenNestedPopup()) {
            popupEscapeEvents?.add(ev);
            return;
          }
          ev.stopPropagation();
          ev.preventDefault();
          dialog.close();
          return;
        }
        if (ev.key === "Tab" && !ev.ctrlKey && !ev.metaKey && !ev.altKey) {
          activeKeyEvents?.add(ev);
          cycleTabFocus(ev);
          return;
        }
        if (insideDialog(ev.target)) {
          activeKeyEvents?.add(ev);
          return;
        }
        if (ev.key === "Enter" && !ev.ctrlKey && !ev.metaKey && !ev.altKey && !hasOpenNestedPopup()) {
          ev.stopPropagation();
          ev.preventDefault();
          dialog.querySelector(".dialog-btn-primary")?.click();
          return;
        }
        ev.stopPropagation();
        if (!isTextInputTarget(ev.target)) ev.preventDefault();
      };
      const onDialogBubble = (ev) => {
        const fromActive = Boolean(activeKeyEvents?.has(ev));
        if (fromActive) activeKeyEvents.delete(ev);
        const escapeOnPopup = Boolean(popupEscapeEvents?.has(ev));
        if (escapeOnPopup) popupEscapeEvents.delete(ev);
        if (ev.isComposing || (!isTopActive() && !fromActive)) return;
        ev.stopPropagation();
        if (!isTopActive() || ev.defaultPrevented) return;
        if (ev.key === "Escape") {
          if (escapeOnPopup || hasOpenNestedPopup()) return;
          ev.preventDefault();
          dialog.close();
          return;
        }
        if (ev.key === "Enter" && !ev.ctrlKey && !ev.metaKey && !ev.altKey && !hasOpenNestedPopup()) {
          if (
            isTextInputTarget(ev.target)
            || isButtonLikeTarget(ev.target)
            || ev.target?.closest?.(".custom-select, select, [role='listbox'], [role='option'], [role='button'], button, a[href]")
          ) {
            return;
          }
          ev.preventDefault();
          dialog.querySelector(".dialog-btn-primary")?.click();
          return;
        }
        if ((ev.ctrlKey || ev.metaKey || ev.altKey) && !isTextInputTarget(ev.target) && !ev.target?.closest?.("input, textarea")) {
          const lower = String(ev.key || "").toLowerCase();
          if (lower !== "c" && lower !== "a") ev.preventDefault();
        }
      };
      const onCancel = (ev) => {
        if (!isTopActive() || hasOpenNestedPopup()) {
          ev.preventDefault();
        }
      };
      const closePopups = (scope = dialog) => {
        host.__EASYOFFICE_COLOR_PICKER__?.closePopupIn?.(scope);
        host.__EASYOFFICE_CUSTOM_SELECT__?.closeOpenIn?.(scope);
        if (scope === document) host.__EASYOFFICE_CLOSE_MENUS__?.(document);
      };
      const release = () => {
        removeOpenDialogStack(dialog);
        document.removeEventListener("keydown", capture, true);
        closePopups(dialog);
      };
      dialog.addEventListener("cancel", onCancel);
      dialog.addEventListener("close", release);
      dialog.addEventListener("keydown", onDialogBubble);
      const origClose = typeof dialog.close === "function" ? dialog.close : null;
      if (origClose) {
        dialog.close = function (...args) {
          removeOpenDialogStack(this);
          document.removeEventListener("keydown", capture, true);
          closePopups(this);
          return origClose.apply(this, args);
        };
      }
      for (const method of ["showModal", "show"]) {
        const open = dialog[method];
        if (typeof open !== "function") continue;
        dialog[method] = function (...args) {
          if (method === "showModal") closePopups(document);
          const out = open.apply(this, args);
          pushOpenDialogStack(this);
          document.removeEventListener("keydown", capture, true);
          document.addEventListener("keydown", capture, true);
          const primary = this.querySelector(".dialog-btn-primary");
          if (primary && !primary.disabled) primary.focus();
          else {
            this.tabIndex = -1;
            this.focus();
          }
          return out;
        };
      }
    }

    function bind() {
      bindAbout();
      bindIconToolbarScroller();
      const initialLabels = readToolbarLabels();
      applyToolbarLabels(initialLabels);
      report(hwpHost?.setShellContext?.({ toolbarLabels: initialLabels }));
      hwpHost?.onToolbarLabels?.((visible) => saveToolbarLabels(visible));
      for (const el of document.querySelectorAll("[data-view-toolbar-labels]")) {
        el.addEventListener("click", (ev) => {
          ev.stopPropagation?.();
          host.__EASYOFFICE_CLOSE_MENUS__?.(document);
          toggleToolbarLabels();
        });
      }
      env.addEventListener?.("storage", (ev) => {
        if (ev.key === TOOLBAR_LABELS_KEY || ev.key === null) {
          const nextLabels = readToolbarLabels();
          applyToolbarLabels(nextLabels);
          report(hwpHost?.setShellContext?.({ toolbarLabels: nextLabels }));
          pushMenuState();
        }
      });
      for (const dlg of document.querySelectorAll("dialog.tool-dialog")) {
        enableDialogDrag(dlg);
        enableDialogKeys(dlg);
      }
      mirrorFeedbackToToast();
      hwpHost.onCommand(runShellCommand);
    }

    return { bind, openAbout, setAppVersion, applyStoredTheme, pushMenuState, pushRecents, toast, runShellCommand, currentTheme, enableDialogDrag, enableDialogKeys, toggleToolbarLabels, getThemeMode: () => themeApi().getMode() };
  }

  host.__EASYOFFICE_CREATE_SHELL_CHROME__ = createShellChrome;
  host.__EASYOFFICE_NATIVE_MENU_TABLE__ = { NATIVE_MENU_ACTIONS, PARSED_NATIVE_MENU_ACTIONS, EMBED_EDITOR_COMMANDS, EMBED_ONLY_ACTIONS, EMBED_ONLY_SET, START_DISABLED_SET, TEXT_FIELD_EDIT_COMMANDS, allowsNativeMenuContext, runNativeMenuAction, pageMenuActions, sheetMenuPaletteItems };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
