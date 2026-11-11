(function (host) {
  "use strict";

  // The find / replace dialog (#find-dialog) for the document and sheet modules (Ctrl+F finds,
  // Ctrl+F2 replaces, as in the reference editor). It is not modal: the page stays editable, the
  // matches are painted with the CSS Custom Highlight API (no DOM change) when the web view has it,
  // otherwise the current match is selected. The text, the matches and the offset table live in
  // `find-core.js` and a source (`find-doc.js`, `find-sheet.js`); this file is the UI around them.

  const { buildMatcher, createFindIndex } = host.__EASYOFFICE_FIND_CORE__;
  const DIALOG_ID = "find-dialog";
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
      caseBox: byId("find-opt-case"),
      replaceButtons: byId("find-replace-buttons"),
    });

    function createSource(moduleCode) {
      if (moduleCode === "DOC") return host.__EASYOFFICE_CREATE_DOC_FIND_SOURCE__({ root: byId("doc-editor"), document, env, withUndo });
      return host.__EASYOFFICE_CREATE_SHEET_FIND_SOURCE__({
        sheetDataStore: deps.sheetDataStore, getActiveSheetName: deps.getActiveSheetName,
        getActiveCellRef: deps.getActiveSheetCellRef, sheetController: deps.sheetController,
        gridUtils: host.__EASYOFFICE_SHEET_GRID_UTILS__, captureWorkspaceSnapshot: deps.captureWorkspaceSnapshot,
        commitDeferredUndoSnapshot: deps.commitDeferredUndoSnapshot,
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

    function readMatcher() {
      const { query, caseBox } = ui();
      return buildMatcher({ query: query.value, matchCase: caseBox.checked });
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
      source.show(nearMatches(n), current);
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
      source.sync(index);
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

    function reportReplaced({ replaced, skipped }) {
      const note = skipped > 0 ? " (수식 셀 " + skipped + "개는 건너뜀)" : "";
      const text = replaced + "개를 바꿨습니다" + note;
      setFeedback?.(text);
      return text;
    }

    async function replaceCurrent() {
      const result = refresh();
      if (!result || matcher.empty) return;
      if (!current) return step(true);
      const value = ui().replacement.value;
      const { id, start, end } = current;
      const outcome = await source.replace([{ id, edits: [{ start, end, value }] }]);
      origin = { id, offset: start + value.length };
      current = null;
      const next = refresh();
      goTo(next ? index.neighbour(id, origin.offset, true) : -1, next);
      if (outcome.replaced === 0) showStatus("수식 셀의 결과 값은 바꿀 수 없습니다");
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
      const outcome = await source.replace(plan);
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
      const { title, replaceRow, replaceButtons } = ui();
      const replacing = mode === "replace";
      title.textContent = replacing ? "찾아 바꾸기" : "찾기";
      replaceRow.hidden = !replacing;
      replaceButtons.hidden = !replacing;
    }

    function ensureSource(moduleCode) {
      if (source?.kind === (moduleCode === "DOC" ? "doc" : "sheet")) return;
      source?.dispose();
      source = createSource(moduleCode);
      source.watch(() => {
        if (countTimer !== null || matcher.empty) return;
        countTimer = env.setTimeout(refreshCount, 0);
      });
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
      if (moduleCode !== "DOC" && moduleCode !== "SHEET") return false;
      const els = ui();
      if (!els.dialog) return false;
      ensureSource(moduleCode);
      applyMode(mode);
      const picked = moduleCode === "DOC" ? selectedQuery() : "";
      if (picked) els.query.value = picked;
      if (!els.dialog.open) {
        origin = source.anchor();
        els.dialog.show();
      }
      els.query.focus();
      els.query.select();
      search();
      return true;
    }

    function close() {
      if (ui().dialog.open) return;
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
      els.caseBox.addEventListener("change", search);
      byId("btn-find-next").addEventListener("click", () => step(true));
      byId("btn-find-prev").addEventListener("click", () => step(false));
      byId("btn-find-replace").addEventListener("click", () => void replaceCurrent().catch((err) => showStatus(err.message)));
      byId("btn-find-replace-all").addEventListener("click", () => void replaceAll().catch((err) => showStatus(err.message)));
      els.dialog.addEventListener("close", close);
      els.dialog.addEventListener("keydown", onDialogKeydown);
    }

    // The dialog keeps its keys to itself: the page-wide shortcuts (undo, bold, ...) must not act on the document while a field has the focus.
    function onDialogKeydown(event) {
      event.stopPropagation();
      if (event.isComposing || composing) return;
      const els = ui();
      const key = String(event.key);
      const withCtrl = event.ctrlKey || event.metaKey;
      if (key === "Escape") return void els.dialog.close();
      if (withCtrl && key.toLowerCase() === "f") {
        event.preventDefault();
        return void open("find");
      }
      if (withCtrl && key === "F2") {
        event.preventDefault();
        return void open("replace");
      }
      if (key !== "Enter") return;
      event.preventDefault();
      if (event.target === els.replacement) void replaceCurrent().catch((err) => showStatus(err.message));
      else step(!event.shiftKey);
    }

    return { bind, open, close };
  }

  host.__EASYOFFICE_CREATE_FIND_REPLACE__ = createFindReplace;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
