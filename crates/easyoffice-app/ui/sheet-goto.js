(function (host) {
  "use strict";

  // 편집 > 이동 (Ctrl+G, macOS ⌃G) of the sheet window (PD-391): the 이동 dialog with the names of the sheet, the references used
  // before and a 참조 field, and its 옵션 button, the 이동 옵션 dialog (`sheet-special-cells.js`). The reference field takes a
  // name, an address, `Sheet!A1:B2`, whole rows or columns, and several of them apart with commas.
  const RECENT_LIMIT = 4;
  const MESSAGES = {
    badRef: "참조가 올바르지 않습니다",
    noCells: "해당 셀을 찾을 수 없습니다",
    ranges: "선택할 수 있는 범위는 64개까지입니다",
    scope: "범위가 너무 커서 찾을 수 없습니다",
    sheets: "한 번에 한 시트의 범위만 선택할 수 있습니다",
  };

  function createSheetGoto(deps) {
    const { document, grid, getActiveSheetName, getRanges, getSelection, getExtent, sheetDataStore, switchSheet, selectRange, selectRanges, commitIfEditing } = deps;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const names = host.__EASYOFFICE_SHEET_NAMES__;
    const refsApi = host.__EASYOFFICE_SHEET_REFS__;
    const special = host.__EASYOFFICE_SHEET_SPECIAL_CELLS__;
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const $ = (id) => document.getElementById(id);
    // The references of this window that 이동 took, newest first; the field shows the last one again.
    let recent = [];

    // Opens `id`, runs `onOk` for the OK button and keeps the dialog open while it answers a message; closes on success.
    function openChecked(id, okId, onOk, ready) {
      const dialog = $(id);
      const ok = $(okId);
      return new Promise((resolve) => {
        let settled = false;
        const closers = Array.from(dialog.querySelectorAll("[data-dialog-close]"));
        function finish(value) {
          if (settled) return;
          settled = true;
          ok.removeEventListener("click", press);
          closers.forEach((closer) => closer.removeEventListener("click", cancel));
          dialog.removeEventListener("close", cancel);
          kit.closeDialog(id);
          resolve(value);
        }
        const press = async () => {
          const answer = await onOk();
          if (answer === true) finish(true);
        };
        const cancel = () => finish(false);
        ok.addEventListener("click", press);
        closers.forEach((closer) => closer.addEventListener("click", cancel));
        dialog.addEventListener("close", cancel);
        kit.openDialog(id);
        ready?.(press);
      });
    }

    // ---- the reference ------------------------------------------------------------------------------------

    // The ranges a reference text names, on one sheet: `{ sheet, ranges }` or `{ error }`.
    function resolve(text) {
      const sheet = getActiveSheetName();
      const parts = String(text ?? "").split(",").map((part) => part.trim()).filter(Boolean);
      if (parts.length === 0) return { error: "badRef" };
      let target = null;
      const ranges = [];
      for (const part of parts) {
        const def = names.find(part, sheet);
        const at = def ? names.parseRange(def.refersTo) : part.includes("!") ? names.parseRange(part) : names.parseAddress(part);
        if (!at) return { error: "badRef" };
        const onSheet = at.sheet ?? sheet;
        if (target !== null && names.fold(target) !== names.fold(onSheet)) return { error: "sheets" };
        target = target ?? onSheet;
        ranges.push({ r1: at.r1, c1: at.c1, r2: at.r2, c2: at.c2 });
      }
      if (ranges.length > special.MAX_RANGES) return { error: "ranges" };
      return { sheet: target, ranges };
    }

    async function select(sheet, ranges) {
      const key = Object.keys(sheetDataStore).find((name) => names.fold(name) === names.fold(sheet));
      if (!key) return false;
      if (key !== getActiveSheetName()) await switchSheet(key);
      if (ranges.length === 1) selectRange(ranges[0]);
      else selectRanges(ranges);
      return true;
    }

    // ---- 이동 -----------------------------------------------------------------------------------------------

    function fillList() {
      const list = $("sheet-goto-list");
      const items = [
        ...recent.map((text) => ({ label: text, text })),
        ...names.visibleFor(getActiveSheetName()).map((def) => ({ label: def.name, text: def.name })),
      ];
      list.replaceChildren(
        ...items.map((item) => {
          const li = document.createElement("li");
          li.setAttribute("role", "option");
          li.setAttribute("aria-selected", "false");
          li.textContent = item.label;
          li.dataset.text = item.text;
          return li;
        })
      );
    }

    function openGoto() {
      commitIfEditing();
      fillList();
      const field = $("sheet-goto-ref");
      field.value = recent[0] ?? "";
      kit.note("sheet-goto-status", "");
      const list = $("sheet-goto-list");
      let press = null;
      const onPick = (ev) => {
        const li = ev.target?.closest?.("li[data-text]");
        if (!li) return;
        for (const other of list.children) other.setAttribute("aria-selected", other === li ? "true" : "false");
        field.value = li.dataset.text;
        if (ev.type === "dblclick") void press?.();
      };
      const onKey = (ev) => {
        if (ev.key === "Enter") {
          ev.preventDefault();
          void press?.();
        }
      };
      list.addEventListener("click", onPick);
      list.addEventListener("dblclick", onPick);
      field.addEventListener("keydown", onKey);
      const done = openChecked(
        "sheet-goto-dialog",
        "btn-sheet-goto-ok",
        async () => {
          const text = field.value.trim();
          const found = resolve(text);
          if (found.error) {
            kit.note("sheet-goto-status", MESSAGES[found.error]);
            return false;
          }
          if (!(await select(found.sheet, found.ranges))) {
            kit.note("sheet-goto-status", MESSAGES.badRef);
            return false;
          }
          recent = [text, ...recent.filter((old) => old !== text)].slice(0, RECENT_LIMIT);
          return true;
        },
        (pressOk) => {
          press = pressOk;
          field.focus();
          field.select?.();
        }
      );
      return done.finally(() => {
        list.removeEventListener("click", onPick);
        list.removeEventListener("dblclick", onPick);
        field.removeEventListener("keydown", onKey);
        press = null;
      });
    }

    // ---- 이동 옵션 ------------------------------------------------------------------------------------------

    // What the 이동 옵션 search reads from the sheet.
    function specialView() {
      const sheet = getActiveSheetName();
      const map = sheetDataStore[sheet] || {};
      const extent = getExtent();
      const at = (row, col) => map[refsApi.refOf(row, col)];
      const spill = host.__EASYOFFICE_SHEET_SPILL__;
      const parse = (ref) => refsApi.parseRef(ref);
      return {
        maxRows: grid.MAX_ROWS,
        maxCols: grid.MAX_COLS,
        usedRows: extent.dataRows,
        usedCols: extent.dataCols,
        forEachEntry(visit) {
          for (const ref of Object.keys(map)) {
            const p = parse(ref);
            if (p) visit(p.row, p.col, map[ref]);
          }
        },
        forEachNote(visit) {
          for (const note of layoutApi.notesOf(sheet)) {
            const p = parse(note.ref);
            if (p) visit(p.row, p.col);
          }
        },
        isFilled: (row, col) => {
          const entry = at(row, col);
          return Boolean(entry) && (Boolean(entry.formula) || (entry.value !== undefined && entry.value !== null && entry.value !== ""));
        },
        hasNote: (row, col) => Boolean(layoutApi.noteAtRef(sheet, refsApi.refOf(row, col))),
        rowHidden: (row) => grid.rowHidden(row),
        colHidden: (col) => grid.colHidden(col),
        arrayAt: (row, col) => spill?.areaOf((ref) => map[ref], refsApi.refOf(row, col)) ?? null,
        inferTag: host.__EASYOFFICE_SHEET_CELL_TYPE__.inferTag,
      };
    }

    function readOptions() {
      const kind = Array.from(document.querySelectorAll('input[name="sheet-special-kind"]')).find((radio) => radio.checked)?.value || "constants";
      const group = kind === "formulas" ? "formula" : "constant";
      const types = special.VALUE_TYPES.filter((type) => kit.checked("sheet-special-" + group + "-" + type));
      return { kind, types };
    }

    async function runSpecial() {
      const { kind, types } = readOptions();
      const ranges = getRanges();
      const single = ranges.length === 1 && ranges[0].r1 === ranges[0].r2 && ranges[0].c1 === ranges[0].c2;
      const selection = getSelection();
      const found = special.findSpecial(specialView(), {
        kind,
        types,
        scope: single ? null : ranges,
        active: { row: selection.ar, col: selection.ac },
      });
      if (found.error) {
        kit.note("sheet-special-status", MESSAGES[found.error] || MESSAGES.scope);
        return false;
      }
      if (found.ranges.length === 0) {
        kit.note("sheet-special-status", MESSAGES.noCells);
        return false;
      }
      return select(getActiveSheetName(), found.ranges);
    }

    function openSpecial() {
      commitIfEditing();
      kit.note("sheet-special-status", "");
      return openChecked("sheet-special-dialog", "btn-sheet-special-ok", runSpecial);
    }

    // 이동 > 옵션: the dialog of 이동 옵션 opens over 이동, which closes with it when a search selects cells.
    async function openGotoWithOptions() {
      const opened = openGoto();
      const optionsButton = $("btn-sheet-goto-special");
      const onOptions = async () => {
        if (await openSpecial()) kit.closeDialog("sheet-goto-dialog");
      };
      optionsButton.addEventListener("click", onOptions);
      try {
        return await opened;
      } finally {
        optionsButton.removeEventListener("click", onOptions);
      }
    }

    return { actions: { goto: openGotoWithOptions, "goto-special": openSpecial }, resolve };
  }

  host.__EASYOFFICE_CREATE_SHEET_GOTO__ = createSheetGoto;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
