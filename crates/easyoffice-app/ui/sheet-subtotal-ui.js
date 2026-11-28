(function (host) {
  "use strict";

  // 데이터 > 개요 > 부분합 and 모두 제거 of the sheet window (PD-391): the 부분합 dialog and the step that puts the rows of
  // `sheet-subtotal.js` into the sheet. The list is the selection (several cells) or the region around the active cell; its first
  // row holds the headings. The subtotal rows are made by one structure edit (rows inserted or deleted after the list) and the
  // list is written over its rows in that same undo step, together with the outline groups that fold the details.
  // Cells of a list the command reads.
  const MAX_LIST_CELLS = 1000000;
  const MESSAGES = {
    none: "제거할 부분합이 없습니다",
    rows: "머리글 아래에 데이터가 있어야 합니다",
    group: "그룹화할 항목을 고르세요",
    columns: "부분합 계산 항목을 고르세요",
    formulas: "수식이 있는 데이터에는 쓸 수 없습니다",
    kept: "이미 부분합이 있습니다. 새로운 값으로 대치하세요",
    limit: "데이터가 너무 많습니다",
    beside: "목록 옆에 다른 데이터가 있어 쓸 수 없습니다",
    referenced: "다른 수식·이름·차트가 이 목록을 참조해 쓸 수 없습니다",
    tables: "표나 병합 셀이 있는 목록에는 쓸 수 없습니다",
    function: "사용할 함수를 고르세요",
  };

  function createSheetSubtotalUi(deps) {
    const { document, kit, sheetDataStore, getActiveSheetName, getRanges, getSelection, getExtent, snapshotDomSheetCells, commitIfEditing, editAxis, applyCellEdits, grid, say } = deps;
    const subtotal = host.__EASYOFFICE_SHEET_SUBTOTAL__;
    const special = host.__EASYOFFICE_SHEET_SPECIAL_CELLS__;
    const outlineApi = host.__EASYOFFICE_SHEET_OUTLINE__;
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const refsApi = host.__EASYOFFICE_SHEET_REFS__;
    const { $, note } = kit;

    // The list the command works on: the selected range, or the region around the active cell.
    function listRegion(map) {
      const ranges = getRanges();
      const only = ranges.length === 1 ? ranges[0] : null;
      // A selection is cut to the cells that hold data: Ctrl+A or whole columns must not make the command read a million lines.
      if (only && (only.r1 !== only.r2 || only.c1 !== only.c2)) {
        const extent = getExtent();
        return { r1: only.r1, c1: only.c1, r2: Math.min(only.r2, extent.dataRows), c2: Math.min(only.c2, extent.dataCols) };
      }
      const at = getSelection();
      const filled = (row, col) => {
        const entry = map[refsApi.refOf(row, col)];
        return Boolean(entry) && (Boolean(entry.formula) || String(entry.value ?? "") !== "");
      };
      return special.regionAround({ maxRows: grid.MAX_ROWS, maxCols: grid.MAX_COLS, isFilled: filled }, at.ar, at.ac);
    }

    const cellsOfRow = (map, row, region) => {
      const cells = [];
      for (let col = region.c1; col <= region.c2; col += 1) cells.push(map[refsApi.refOf(row, col)] ?? null);
      return cells;
    };

    // Why a list may not be reorganised, or `null`: too big, a table or merged cells in it, or a formula, name or chart that
    // points into it (the rows move by this command, and what points at them would not follow).
    function refusalFor(map, region, sheet) {
      const rows = region.r2 - region.r1 + 1;
      const cols = region.c2 - region.c1 + 1;
      if (rows < 1 || cols < 1) return "rows";
      if (rows > subtotal.MAX_ROWS || rows * cols > MAX_LIST_CELLS) return "limit";
      if (layoutApi.tablesIn(sheet, region.r1, region.c1, region.r2, region.c2).length > 0 || layoutApi.mergesIn(sheet, region.r1, region.c1, region.r2, region.c2).length > 0) return "tables";
      const look = (text, home) => subtotal.refersInto(text, { sheet, home, region, colIndex: refsApi.colIndex });
      const inside = (p) => p.row >= region.r1 && p.row <= region.r2 && p.col >= region.c1 && p.col <= region.c2;
      for (const name of Object.keys(sheetDataStore)) {
        const cells = name === sheet ? map : sheetDataStore[name] || {};
        for (const ref of Object.keys(cells)) {
          const formula = cells[ref]?.formula;
          if (!formula) continue;
          const p = name === sheet ? refsApi.parseRef(ref) : null;
          if (p && inside(p)) continue;
          if (look(formula, name)) return "referenced";
        }
      }
      const names = host.__EASYOFFICE_SHEET_NAMES__?.getAll?.() ?? [];
      if (names.some((def) => look(def.refersTo, null))) return "referenced";
      for (const owner of Object.keys(sheetDataStore)) {
        for (const chart of layoutApi.chartsOf(owner)) {
          const texts = [chart.dataRange, ...(chart.series || []).flatMap((s) => [s.nameRef, s.categoriesRef, s.valuesRef])];
          if (texts.some((text) => text && look(text, owner))) return "referenced";
        }
      }
      return null;
    }

    // The rows of the list, and whether something sits beside it in its rows (those cells would not move with the list).
    function readList(map, region) {
      const rows = [];
      for (let row = region.r1; row <= region.r2; row += 1) rows.push(cellsOfRow(map, row, region));
      for (const ref of Object.keys(map)) {
        const p = refsApi.parseRef(ref);
        const entry = map[ref];
        const filled = Boolean(entry) && (Boolean(entry.formula) || String(entry.value ?? "") !== "");
        if (p && filled && p.row >= region.r1 && p.row <= region.r2 && (p.col < region.c1 || p.col > region.c2)) return { error: "beside" };
      }
      return { rows };
    }

    const headingOf = (cells, index, region) => {
      const text = String(cells[index]?.value ?? "").trim();
      return text !== "" ? text : refsApi.colName(region.c1 + index) + "열";
    };

    // ---- applying a plan -----------------------------------------------------------------------------------

    // Puts `rows` over the list's rows, and the groups `groups` (`[first, last]` sheet rows, outer first) over its outline.
    function writeList(sheet, region, rows, groups, summaryAbove) {
      const map = sheetDataStore[sheet] || (sheetDataStore[sheet] = {});
      // The rows of the new list only: after a deletion the old rows below its end hold what moved up under it.
      const lastRow = region.r1 + rows.length - 1;
      for (let row = region.r1; row <= lastRow; row += 1) {
        for (let col = region.c1; col <= region.c2; col += 1) delete map[refsApi.refOf(row, col)];
      }
      rows.forEach((cells, i) => {
        cells.forEach((entry, j) => {
          if (entry) map[refsApi.refOf(region.r1 + i, region.c1 + j)] = { ...entry };
        });
      });
      let outline = layoutApi.outlineOf(sheet) || {};
      for (let level = 0; level < 8; level += 1) {
        const result = outlineApi.ungroup(outline, "row", region.r1, lastRow);
        if (result.error) break;
        outline = result.outline;
      }
      if (groups.length > 0) {
        outline = { ...outline };
        if (summaryAbove) outline.summaryAbove = true;
        else delete outline.summaryAbove;
        for (const [first, last] of groups) {
          const result = outlineApi.group(outline, "row", first, last);
          if (result.error) break;
          outline = result.outline;
        }
      }
      layoutApi.setOutline(sheet, outline);
    }

    // Rows are inserted or deleted after the list so the rows below it move as in the program; the list is rewritten in the same step.
    async function apply(region, rows, groups, summaryAbove) {
      const sheet = getActiveSheetName();
      const delta = rows.length - (region.r2 - region.r1 + 1);
      const write = (name) => writeList(name, region, rows, groups, summaryAbove);
      if (delta > 0) return editAxis("row", "insert", region.r2 + 1, delta, write);
      if (delta < 0) return editAxis("row", "delete", region.r1 + rows.length, -delta, write);
      // The same number of rows: a plain edit of the cells; the outline follows it as a step of its own.
      const edits = [];
      for (let i = 0; i < rows.length; i += 1) {
        for (let j = 0; j < rows[i].length; j += 1) {
          const entry = rows[i][j];
          edits.push({ ref: refsApi.refOf(region.r1 + i, region.c1 + j), value: entry?.value ?? "", formula: entry?.formula ?? "", style: entry?.style });
        }
      }
      const done = await applyCellEdits(edits, "부분합");
      if (done) {
        write(sheet);
        grid.layoutChanged();
      }
      return done;
    }

    // ---- the dialog ----------------------------------------------------------------------------------------

    function fillDialog(region, header) {
      const group = $("sheet-subtotal-group");
      const fn = $("sheet-subtotal-fn");
      kit.fillOptions("sheet-subtotal-fn", subtotal.FUNCTIONS.map((f) => [f.id, f.label]));
      fn.value = "sum";
      group.replaceChildren();
      header.forEach((_, i) => {
        const option = document.createElement("option");
        option.value = String(i);
        option.textContent = headingOf(header, i, region);
        group.appendChild(option);
      });
      group.value = "0";
      kit.fireChange(group);
      kit.fireChange(fn);
      const list = $("sheet-subtotal-columns");
      list.replaceChildren();
      header.forEach((_, i) => {
        const item = document.createElement("li");
        const label = document.createElement("label");
        label.className = "dialog-checkbox";
        const input = document.createElement("input");
        input.setAttribute("type", "checkbox");
        input.type = "checkbox";
        input.dataset.subtotalCol = String(i);
        // The program checks the last column.
        input.checked = i === header.length - 1;
        label.appendChild(input);
        label.appendChild(document.createTextNode(" " + headingOf(header, i, region)));
        item.appendChild(label);
        list.appendChild(item);
      });
      kit.setChecked("sheet-subtotal-replace", true);
      kit.setChecked("sheet-subtotal-below", true);
      note("sheet-subtotal-status", "");
    }

    function readDialog(region) {
      const calcCols = [...document.querySelectorAll("#sheet-subtotal-columns input[data-subtotal-col]")].filter((box) => box.checked).map((box) => Number(box.dataset.subtotalCol));
      return {
        firstRow: region.r1,
        firstCol: region.c1,
        groupCol: Number($("sheet-subtotal-group").value),
        calcCols,
        fn: subtotal.functionOf($("sheet-subtotal-fn").value),
        below: kit.checked("sheet-subtotal-below"),
        replace: kit.checked("sheet-subtotal-replace"),
        colName: refsApi.colName,
      };
    }

    async function open() {
      commitIfEditing();
      const map = snapshotDomSheetCells();
      const region = listRegion(map);
      if (region.r2 - region.r1 < 1) {
        say(MESSAGES.rows);
        return false;
      }
      const refused = refusalFor(map, region, getActiveSheetName());
      if (refused) {
        say(MESSAGES[refused]);
        return false;
      }
      const list = readList(map, region);
      if (list.error) {
        say(MESSAGES[list.error]);
        return false;
      }
      fillDialog(region, list.rows[0]);
      const planned = await new Promise((resolve) => {
        let settled = false;
        const dialog = $("sheet-subtotal-dialog");
        const ok = $("btn-sheet-subtotal-ok");
        const removeAll = $("btn-sheet-subtotal-remove");
        const closers = Array.from(dialog.querySelectorAll("[data-dialog-close]"));
        const finish = (value) => {
          if (settled) return;
          settled = true;
          ok.removeEventListener("click", onOk);
          removeAll.removeEventListener("click", onRemove);
          closers.forEach((closer) => closer.removeEventListener("click", onCancel));
          dialog.removeEventListener("close", onCancel);
          kit.closeDialog("sheet-subtotal-dialog");
          resolve(value);
        };
        // A plan that fails stays in the dialog with its reason.
        const answer = (result) => {
          if (result.error) note("sheet-subtotal-status", MESSAGES[result.error] || MESSAGES.limit);
          else finish(result);
        };
        const onOk = () => answer(subtotal.plan(list.rows, readDialog(region)));
        const onRemove = () => {
          const removed = subtotal.removeAll(list.rows);
          answer(removed.error ? removed : { rows: removed.rows, groups: [], below: true });
        };
        const onCancel = () => finish(null);
        ok.addEventListener("click", onOk);
        removeAll.addEventListener("click", onRemove);
        closers.forEach((closer) => closer.addEventListener("click", onCancel));
        dialog.addEventListener("close", onCancel);
        kit.openDialog("sheet-subtotal-dialog");
      });
      if (!planned) return false;
      return apply(region, planned.rows, planned.groups, !planned.below);
    }

    return { actions: { subtotal: open } };
  }

  host.__EASYOFFICE_CREATE_SHEET_SUBTOTAL_UI__ = createSheetSubtotalUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
