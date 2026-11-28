(function (host) {
  "use strict";

  // The find / replace dialog (#find-dialog) for the document, sheet and base modules (Ctrl+F finds,
  // Ctrl+H replaces; the Hangul window keeps the editor's Ctrl+F2). It is not modal: the page stays editable, the
  // matches are painted with the CSS Custom Highlight API (no DOM change) when the web view has it,
  // otherwise the current match is selected. The text, the matches and the offset table live in
  // `find-core.js` and a source (`find-doc.js`, `find-sheet.js`, `find-base.js`); this file is the UI around them.

  const { buildMatcher, createFindIndex } = host.__EASYOFFICE_FIND_CORE__;
  const DIALOG_ID = "find-dialog";
  const KIND_OF = { DOC: "doc", HWP: "doc", SHEET: "sheet", SLIDE: "slide", BASE: "base", BOARD: "board", PDF: "pdf" };
  const FIND_ONLY_KINDS = new Set(["board", "pdf"]);
  // Matches on each side of the current one that get an on-screen highlight range.
  const NEAR_WINDOW = 1000;
  const MAX_SELECTION_QUERY = 100;
  const MSG_NO_MATCH = "일치하는 항목이 없습니다";
  const MSG_TOO_MANY = "일치 항목이 너무 많아 모두 바꾸기를 할 수 없습니다. 찾을 내용을 좁혀 주세요.";

  function createFindReplace(deps) {
    const { document, getActiveModule, setFeedback } = deps;
    const env = deps.env || host;
    const byId = (id) => document.getElementById(id);
    const index = createFindIndex();
    let source = null;
    let matcher = { ok: true, empty: true, key: "" };
    let current = null; // the match shown (an index.at() result)
    let origin = null; // where the next search starts: { id, offset }
    let composing = false;
    let countTimer = null;

    // The dialog is static markup: its elements are looked up once.
    let elements = null;
    const ui = () => (elements ??= {
      dialog: byId(DIALOG_ID), title: byId("find-dialog-title"), query: byId("find-query"),
      replacement: byId("find-replacement"), replaceRow: byId("find-replace-row"), count: byId("find-count"), status: byId("find-status"),
      replaceLabel: byId("find-replace-row")?.querySelector("label"), replaceBtn: byId("btn-find-replace"), replaceAllBtn: byId("btn-find-replace-all"),
      caseBox: byId("find-opt-case"), sheetOptions: byId("find-sheet-options"),
      wholeBox: byId("find-opt-whole"), formulaBox: byId("find-opt-formula"), workbookBox: byId("find-opt-workbook"),
      replaceButtons: byId("find-replace-buttons"),
    });
    const isPagesDoc = () => getActiveModule() === "DOC" && byId("doc-editor")?.dataset?.eoDocFlavor === "pages";

    function createSource(moduleCode) {
      if (moduleCode === "DOC" || moduleCode === "HWP") return host.__EASYOFFICE_CREATE_DOC_FIND_SOURCE__({ root: byId("doc-editor"), document, env, withUndo });
      if (moduleCode === "BASE") return host.__EASYOFFICE_CREATE_BASE_FIND_SOURCE__({ getBaseStudio: deps.getBaseStudio, setFeedback });
      if (moduleCode === "SLIDE" || moduleCode === "PDF" || moduleCode === "BOARD") {
        const hooks = host.__EASYOFFICE_MODULE_FIND_HOOKS__ || {};
        return host.__EASYOFFICE_CREATE_MODULE_FIND_SOURCE__?.(moduleCode, {
          document,
          getSlideDeck: hooks.getSlideDeck,
          getSlideIndex: hooks.getSlideIndex,
          switchSlide: hooks.switchSlide,
          getSlideCanvas: () => host.__EASYOFFICE_SLIDE_CANVAS__,
          withUndo,
          renderSlideDeck: hooks.renderSlideDeck,
          getPdfPages: hooks.getPdfPages,
          goToPdfPage: hooks.goToPdfPage,
          getBoardStudio: () => host.__EASYOFFICE_BASE_BOARD_HOST__?.getBoardStudio?.(),
        });
      }
      return host.__EASYOFFICE_CREATE_SHEET_FIND_SOURCE__({
        sheetDataStore: deps.sheetDataStore, getActiveSheetName: deps.getActiveSheetName,
        getActiveCellRef: deps.getActiveSheetCellRef, sheetController: deps.sheetController,
        gridUtils: host.__EASYOFFICE_SHEET_GRID_UTILS__, sheetHistory: deps.sheetHistory,
        computeEvaluatedSheetStore: deps.sheetStoreHelpers.computeEvaluatedSheetStore,
        isFormulaStr: deps.sheetStoreHelpers.isFormulaStr,
      });
    }

    // One undo step around a synchronous document edit.
    function withUndo(mutate) {
      const pre = deps.captureWorkspaceSnapshot(false);
      const result = mutate();
      deps.saveDocState();
      deps.commitDeferredUndoSnapshot(pre);
      deps.noteActivity();
      return result;
    }

    // ---- state -> screen --------------------------------------------------------------------

    // The reference dialog's two text places: the "n / total" label in the option row and the status line under it.
    function showStatus(text, position = "") {
      const { count, status } = ui();
      count.textContent = position;
      count.hidden = position === "";
      status.textContent = text;
    }

    function totalLabel(result) {
      return result.complete ? String(result.total) : result.total + "+";
    }

    // The sheet-only options (whole cell, formula, all sheets); the document source has none.
    function sheetOptions() {
      const { wholeBox, formulaBox, workbookBox } = ui();
      return source?.kind === "sheet" ? { wholeCell: wholeBox.checked, formula: formulaBox.checked, workbook: workbookBox.checked } : {};
    }

    function readMatcher() {
      const { query, caseBox } = ui();
      return buildMatcher({ query: query.value, matchCase: caseBox.checked, wholeCell: sheetOptions().wholeCell });
    }

    // Matches around `n` for the on-screen highlight.
    function nearMatches(n) {
      const out = [];
      for (let i = Math.max(0, n - NEAR_WINDOW); i < Math.min(index.total(), n + NEAR_WINDOW + 1); i += 1) out.push(index.at(i));
      return out;
    }

    function goTo(n, result) {
      current = n < 0 ? null : index.at(n);
      if (!current) {
        source.clearHighlight();
        showStatus(matcher.empty ? "" : MSG_NO_MATCH);
        return;
      }
      origin = { id: current.id, offset: current.start };
      source.show(nearMatches(n), current, sheetOptions());
      showStatus("", n + 1 + " / " + totalLabel(result));
    }

    // ---- search -----------------------------------------------------------------------------

    function refresh() {
      matcher = readMatcher();
      if (!matcher.ok) {
        current = null;
        source.clearHighlight();
        showStatus(matcher.error);
        return null;
      }
      source.sync(index, sheetOptions());
      return index.search(matcher);
    }

    // A new query or option: the first match at or after the starting position.
    function search() {
      const result = refresh();
      if (!result) return;
      if (matcher.empty) {
        current = null;
        source.clearHighlight();
        return showStatus("");
      }
      goTo(index.neighbour(origin?.id, origin?.offset ?? 0, true), result);
    }

    function step(forward) {
      const result = refresh();
      if (!result || matcher.empty) return;
      const from = current ? { id: current.id, offset: forward ? current.end : current.start } : origin;
      goTo(index.neighbour(from?.id, from?.offset ?? 0, forward), result);
    }

    // ---- replacing --------------------------------------------------------------------------

    function reportReplaced({ replaced, skipped, rejected = 0, skippedLabel = "수식 셀" }) {
      const skipNote = skipped > 0 ? " (" + skippedLabel + " " + skipped + "개는 건너뜀)" : "";
      const invalidNote = rejected > 0 ? " (유효성 불일치 " + rejected + "개는 건너뜀)" : "";
      const verb = isPagesDoc() ? "대치했습니다" : "바꿨습니다";
      const text = replaced + "개를 " + verb + skipNote + invalidNote;
      setFeedback?.(text);
      return text;
    }

    async function replaceCurrent() {
      const result = refresh();
      if (!result || matcher.empty) return;
      if (!current) return step(true);
      const value = ui().replacement.value;
      const { id, start, end } = current;
      const outcome = await source.replace([{ id, edits: [{ start, end, value }] }], sheetOptions());
      origin = { id, offset: start + value.length };
      current = null;
      const next = refresh();
      goTo(next ? index.neighbour(id, origin.offset, true) : -1, next);
      if (outcome.replaced === 0) showStatus(outcome.notice || "수식 셀의 결과 값은 바꿀 수 없습니다");
    }

    async function replaceAll() {
      const result = refresh();
      if (!result || matcher.empty) return;
      if (!result.complete) return showStatus(MSG_TOO_MANY);
      const replacement = ui().replacement.value;
      const plan = [];
      for (const id of index.blockIds()) {
        const found = index.matchesInBlock(id);
        if (found.length > 0) plan.push({ id, edits: found.map((m) => ({ start: m.start, end: m.end, value: replacement })) });
      }
      if (plan.length === 0) return showStatus(MSG_NO_MATCH);
      const outcome = await source.replace(plan, sheetOptions());
      current = null;
      origin = null;
      const after = refresh();
      source.clearHighlight();
      showStatus(reportReplaced(outcome) + (after && after.total > 0 ? " · 남은 일치 " + totalLabel(after) : ""));
    }

    // ---- dialog -----------------------------------------------------------------------------

    function selectedQuery() {
      const text = String(env.getSelection?.()?.toString() ?? "");
      return text !== "" && text.length <= MAX_SELECTION_QUERY && !/[\r\n]/.test(text) ? text : "";
    }

    function applyMode(mode) {
      const { title, replaceLabel, replaceRow, replaceButtons, replaceBtn, replaceAllBtn, sheetOptions: optionsRow } = ui();
      const replacing = mode === "replace" && !FIND_ONLY_KINDS.has(source?.kind);
      const pages = isPagesDoc();
      const replaceTitle = { HWP: "찾아 바꾸기", SLIDE: "바꾸기" }[getActiveModule()] || (pages ? "찾기 및 대치" : "찾기 및 바꾸기");
      title.textContent = replacing ? replaceTitle : "찾기";
      if (replaceLabel) replaceLabel.textContent = pages ? "대치할 내용" : "바꿀 내용";
      if (replaceBtn) replaceBtn.textContent = pages ? "대치" : "바꾸기";
      if (replaceAllBtn) replaceAllBtn.textContent = pages ? "모두 대치" : "모두 바꾸기";
      replaceRow.hidden = !replacing;
      replaceButtons.hidden = !replacing;
      optionsRow.hidden = source?.kind !== "sheet";
    }

    function ensureSource(moduleCode) {
      const kind = KIND_OF[moduleCode];
      if (source?.kind === kind) return false;
      source?.dispose();
      source = createSource(moduleCode);
      source.watch(() => {
        if (countTimer !== null || matcher.empty) return;
        countTimer = env.setTimeout(refreshCount, 0);
      });
      return true;
    }

    // The document changed while the dialog is open: the count follows (only the changed blocks are searched again).
    function refreshCount() {
      countTimer = null;
      if (!source) return;
      const result = refresh();
      if (result && current) showStatus("", index.neighbour(current.id, current.start, true) + 1 + " / " + totalLabel(result));
      else if (result) showStatus(result.total === 0 ? MSG_NO_MATCH : totalLabel(result) + "개 찾음");
    }

    // Opens the dialog for the active module; false when the module has no find (the caller falls back).
    function open(mode) {
      const moduleCode = getActiveModule();
      if (!KIND_OF[moduleCode]) return false;
      const els = ui();
      if (!els.dialog) return false;
      const switched = ensureSource(moduleCode);
      applyMode(mode);
      const picked = moduleCode === "DOC" ? selectedQuery() : "";
      if (picked) els.query.value = picked;
      if (!els.dialog.open) {
        origin = source.anchor(sheetOptions());
        els.dialog.show();
        document.addEventListener("keydown", onKeyCapture, true);
      } else if (switched) {
        current = null;
        origin = source.anchor(sheetOptions());
      }
      els.query.focus();
      els.query.select();
      search();
      return true;
    }

    function close() {
      if (ui().dialog.open) return;
      document.removeEventListener("keydown", onKeyCapture, true);
      source?.dispose();
      source = null;
      current = null;
    }

    function bind() {
      const els = ui();
      if (!els.dialog) return;
      const rerun = () => { if (!composing) search(); };
      els.query.addEventListener("input", rerun);
      els.query.addEventListener("compositionstart", () => { composing = true; });
      els.query.addEventListener("compositionend", () => { composing = false; search(); });
      for (const box of [els.caseBox, els.wholeBox, els.formulaBox, els.workbookBox]) box.addEventListener("change", search);
      byId("btn-find-next").addEventListener("click", () => step(true));
      byId("btn-find-prev").addEventListener("click", () => step(false));
      byId("btn-find-replace").addEventListener("click", () => void replaceCurrent().catch((err) => showStatus(err.message)));
      byId("btn-find-replace-all").addEventListener("click", () => void replaceAll().catch((err) => showStatus(err.message)));
      els.dialog.addEventListener("close", close);
    }

    // Key handling while the dialog is open, as in the reference (`find-dialog.ts` 318-357): a document
    // capture handler. Esc closes; Enter on a button stays with the button; Enter elsewhere (not
    // composing, no modifier) goes to the next match, Shift+Enter to the previous one, and in the
    // replace field replaces; every other key inside the dialog stays there (page shortcuts such as undo do not run).
    function onKeyCapture(event) {
      const els = ui();
      const inDialog = els.dialog.contains(event.target);
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        els.dialog.close();
      } else if (inDialog && event.target.tagName === "BUTTON" && event.key === "Enter") {
        event.stopPropagation();
      } else if (event.key === "Enter" && !event.altKey && !event.ctrlKey && !event.metaKey && !event.isComposing) {
        event.preventDefault();
        event.stopPropagation();
        if (event.target === els.replacement && !event.shiftKey) void replaceCurrent().catch((err) => showStatus(err.message));
        else step(!event.shiftKey);
        els.query.focus();
        els.query.select();
      } else if (inDialog) {
        event.stopPropagation();
      }
    }

    return { bind, open, close };
  }

  host.__EASYOFFICE_CREATE_FIND_REPLACE__ = createFindReplace;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
