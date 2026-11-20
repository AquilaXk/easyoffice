(function (host) {
  "use strict";

  // Dynamic pivot table dialog, contextual `#sheet-pivot-bar` toolbar (`피벗 테이블 분석`), in-place
  // refresh (`새로 고침` / `Alt+F5` / `모두 새로 고침`), source/field reconfiguration (`데이터 원본 변경`),
  // deletion, and shared-`cacheId` pivot slicer synchronization (PD-171, PD-255).

  const rules = host.__EASYOFFICE_SHEET_RULES__;
  const model = host.__EASYOFFICE_SHEET_PIVOT_MODEL__;
  const PIVOT_MAX_FIELDS = 256;
  const PLACEHOLDER = "범위 없음";
  const boundDocs = new WeakSet();
  const boundDialogDocs = new WeakSet();
  let windowKeyBound = false;

  function refsInBox1(box) {
    const out = [];
    if (!box || box.r2 < box.r1 || box.c2 < box.c1) return out;
    for (let r = box.r1; r <= box.r2; r += 1) {
      for (let c = box.c1; c <= box.c2; c += 1) {
        out.push(rules.refOf(r - 1, c - 1));
      }
    }
    return out;
  }

  function unionRefs(boxA, boxB) {
    const set = new Set(refsInBox1(boxA));
    for (const ref of refsInBox1(boxB)) set.add(ref);
    return [...set];
  }

  function boxesOverlap1(a, b) {
    return a.r2 >= a.r1 && b.r2 >= b.r1 && a.r1 <= b.r2 && a.r2 >= b.r1 && a.c1 <= b.c2 && a.c2 >= b.c1;
  }

  function formatSheetPrefix(sheetName) {
    const s = String(sheetName || "");
    if (/^[A-Za-z0-9_가-힣]+$/.test(s) && !/^[A-Za-z]{1,3}\d+$/.test(s)) return s + "!";
    return "'" + s.replaceAll("'", "''") + "'!";
  }

  function createSheetPivotUi(deps, kit) {
    const {
      document,
      sheetDataStore,
      sheetRowCounts,
      getActiveSheetName,
      getRange,
      sheetStoreHelpers,
      enqueueWorkspaceHistoryOp,
      sheetHistory,
      flushFocusedSheetCellBeforeSwitch,
      performSwitchSheetTabAsync,
      selectionText,
    } = deps;
    const { $, val, checked, setVal, setChecked, setSelect, openDialog, closeDialog, optionsOf, fireChange, note } = kit;
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;

    const pivot = { names: [], editTarget: null };

    const say = (text) => {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = text;
    };

    function allSheetNames() {
      const seen = new Set();
      const out = [];
      for (const sh of [...Object.keys(sheetDataStore), ...Object.keys(layoutApi?.getAll() || {})]) {
        if (sh && !seen.has(sh)) {
          seen.add(sh);
          out.push(sh);
        }
      }
      return out;
    }

    function everyPivotEntry() {
      const out = [];
      for (const sh of allSheetNames()) {
        for (const p of layoutApi?.pivotsOf(sh) || []) out.push({ sheet: sh, pivot: p });
      }
      return out;
    }

    function parseRef1(refText) {
      const gridParse = host.__EASYOFFICE_SHEET_GRID_UTILS__?.parseRef;
      if (gridParse) return gridParse(refText);
      const p = rules.parseCell(refText);
      return p ? { row: p.row + 1, col: p.col + 1 } : null;
    }

    function activeCellPos1() {
      const runtimeRef = sheetHistory?.runtime?.getActiveSheetCellRef?.();
      if (runtimeRef) {
        const p = parseRef1(runtimeRef);
        if (p) return p;
      }
      const selectedTd = document.querySelector?.("#pane-sheet td.selected-cell");
      if (selectedTd?.dataset?.cell) {
        const p = parseRef1(selectedTd.dataset.cell);
        if (p) return p;
      }
      const r = getRange();
      return { row: r.r1, col: r.c1 };
    }

    function pivotAtActive() {
      if (!layoutApi?.pivotAt) return null;
      const pos = activeCellPos1();
      return layoutApi.pivotAt(getActiveSheetName(), pos.row, pos.col);
    }

    function usedRangeText() {
      const map = sheetDataStore[getActiveSheetName()] || {};
      let r1 = Infinity;
      let c1 = Infinity;
      let r0 = 0;
      let c0 = 0;
      for (const [ref, entry] of Object.entries(map)) {
        if (!entry || !(entry.value || entry.formula)) continue;
        const p = rules.parseCell(ref);
        if (!p) continue;
        r1 = Math.min(r1, p.row);
        c1 = Math.min(c1, p.col);
        r0 = Math.max(r0, p.row);
        c0 = Math.max(c0, p.col);
      }
      return Number.isFinite(r1) ? rules.formatRanges([{ row0: r1, col0: c1, row1: r0, col1: c0 }]) : "";
    }

    function findTableAcrossSheets(rawName, preferredSheet) {
      const folded = String(rawName || "").trim().toLowerCase();
      if (!folded || !layoutApi) return null;
      const order = [preferredSheet, getActiveSheetName(), ...allSheetNames()];
      const visited = new Set();
      for (const sh of order) {
        if (!sh || visited.has(sh)) continue;
        visited.add(sh);
        const table = layoutApi.tablesOf(sh).find((t) => String(t.name).toLowerCase() === folded);
        if (table) return { sheet: sh, table };
      }
      return null;
    }

    function resolveSourceInput(raw, fallbackSheet) {
      const trimmed = String(raw || "").trim();
      if (!trimmed) return null;
      const tblHit = findTableAcrossSheets(trimmed, fallbackSheet);
      if (tblHit) {
        const b = layoutApi.tableBox(tblHit.table);
        const r2 = b.r2 - (tblHit.table.totals === true ? 1 : 0);
        if (r2 < b.r1) return null;
        const box = { row0: b.r1 - 1, col0: b.c1 - 1, row1: r2 - 1, col1: b.c2 - 1 };
        return {
          sourceSheet: tblHit.sheet,
          sourceRange: rules.formatRanges([box]),
          sourceTable: tblHit.table.name,
          box,
          table: tblHit.table,
        };
      }
      if (trimmed.includes("!")) {
        const bang = trimmed.lastIndexOf("!");
        let rawSheet = trimmed.slice(0, bang).trim();
        const rawRange = trimmed.slice(bang + 1).trim();
        if (rawSheet.startsWith("'") && rawSheet.endsWith("'") && rawSheet.length >= 2) {
          rawSheet = rawSheet.slice(1, -1).replaceAll("''", "'");
        }
        const matchedSheet = allSheetNames().find((sh) => sh.toLowerCase() === rawSheet.toLowerCase());
        if (!matchedSheet) return null;
        const boxes = rules.parseRanges(rawRange);
        if (!boxes || boxes.length !== 1) return null;
        return {
          sourceSheet: matchedSheet,
          sourceRange: rules.formatRanges([boxes[0]]),
          sourceTable: null,
          box: boxes[0],
          table: null,
        };
      }
      const boxes = rules.parseRanges(trimmed);
      if (!boxes || boxes.length !== 1) return null;
      const sourceSheet = fallbackSheet || getActiveSheetName();
      return {
        sourceSheet,
        sourceRange: rules.formatRanges([boxes[0]]),
        sourceTable: null,
        box: boxes[0],
        table: null,
      };
    }

    function cellText(sheet, row0, col0) {
      const entry = sheetDataStore[sheet]?.[rules.refOf(row0, col0)];
      return String(entry?.value || entry?.formula || "");
    }

    function extractFieldNames(sourceSheet, box, hasHeader, table) {
      const width = box ? Math.min(box.col1 - box.col0 + 1, PIVOT_MAX_FIELDS) : 0;
      const names = [];
      for (let i = 0; i < width; i += 1) {
        const tableCol = table && hasHeader ? table.columns?.[i] || "" : "";
        const text = tableCol || (hasHeader ? cellText(sourceSheet, box.row0, box.col0 + i).trim() : "");
        names.push(text || "열 " + rules.refOf(0, box.col0 + i).replace(/\d+$/, ""));
      }
      return names;
    }

    function fallbackDialogSheet() {
      if (!pivot.editTarget) return getActiveSheetName();
      const held = layoutApi?.pivotsOf(pivot.editTarget.sheet).find((p) => p.name === pivot.editTarget.name);
      return held?.sourceSheet || getActiveSheetName();
    }

    function refreshPivotFields(opts = {}) {
      const src = resolveSourceInput(val("sheet-pivot-range"), fallbackDialogSheet());
      const header = checked("sheet-pivot-header");
      const names = src ? extractFieldNames(src.sourceSheet, src.box, header, src.table) : [];
      const fill = (id, withNone) => {
        const el = $(id);
        if (!el) return;
        const prev = opts.resetDefaults ? "" : el.value;
        el.replaceChildren();
        if (withNone) {
          const none = document.createElement("option");
          none.value = "";
          none.textContent = "(없음)";
          el.appendChild(none);
        }
        names.forEach((name, i) => {
          const opt = document.createElement("option");
          opt.value = String(i);
          opt.textContent = name;
          el.appendChild(opt);
        });
        // Without a range a combo would collapse to its arrow; a placeholder gives it a width.
        if (names.length === 0 && !withNone) {
          const hint = document.createElement("option");
          hint.value = "";
          hint.textContent = PLACEHOLDER;
          el.appendChild(hint);
        }
        const all = optionsOf(el);
        el.value = prev !== "" && all.some((o) => o.value === prev) ? prev : (all[0]?.value ?? "");
        fireChange(el);
      };
      fill("sheet-pivot-filter-select", true);
      fill("sheet-pivot-row-select", false);
      fill("sheet-pivot-row2-select", true);
      fill("sheet-pivot-col-select", true);
      fill("sheet-pivot-col2-select", true);
      fill("sheet-pivot-value-select", false);
      fill("sheet-pivot-value2-select", true);
      if (names.length > 1 && (opts.resetDefaults || val("sheet-pivot-value-select") === "0")) {
        setSelect("sheet-pivot-value-select", String(names.length - 1));
      }
      if (opts.resetDefaults) {
        setSelect("sheet-pivot-agg-select", "sum");
        setSelect("sheet-pivot-agg2-select", "sum");
      }
      pivot.names = names;
    }

    function ensurePivotControls() {
      if (boundDialogDocs.has(document)) return;
      boundDialogDocs.add(document);
      $("sheet-pivot-range")?.addEventListener("change", () => host.__EASYOFFICE_SHEET_PIVOT__?._onFieldInputChanged?.());
      $("sheet-pivot-header")?.addEventListener("change", () => host.__EASYOFFICE_SHEET_PIVOT__?._onFieldInputChanged?.());
      $("btn-sheet-pivot-apply")?.addEventListener("click", () => void host.__EASYOFFICE_SHEET_PIVOT__?._submitDialog?.());
    }

    function setApplyButtonLabel(text) {
      const btn = $("btn-sheet-pivot-apply");
      if (btn) btn.textContent = text;
    }

    function openPivotDialog() {
      if (host.__EASYOFFICE_SHEET_PROTECT__?.guardAction(getActiveSheetName(), "pivotTables") === false) return;
      sheetHistory?.runtime?.getController?.()?.snapshotDomSheetCells();
      ensurePivotControls();
      pivot.editTarget = null;
      setApplyButtonLabel("만들기");
      const r = getRange();
      const multi = r.r1 !== r.r2 || r.c1 !== r.c2;
      setVal("sheet-pivot-range", multi ? selectionText() : usedRangeText());
      setChecked("sheet-pivot-header", true);
      refreshPivotFields({ resetDefaults: true });
      note("sheet-pivot-status", "");
      return openDialog("sheet-pivot-dialog");
    }

    function openPivotDialogForTable(table) {
      if (!table) return false;
      if (host.__EASYOFFICE_SHEET_PROTECT__?.guardAction(getActiveSheetName(), "pivotTables") === false) return false;
      sheetHistory?.runtime?.getController?.()?.snapshotDomSheetCells();
      ensurePivotControls();
      const b = layoutApi.tableBox(table);
      const dataRows = b.r2 - b.r1 + 1 - (table.header === false ? 0 : 1) - (table.totals === true ? 1 : 0);
      if (dataRows < 1) {
        say("요약할 데이터 행이 없음");
        return false;
      }
      pivot.editTarget = null;
      setApplyButtonLabel("만들기");
      setVal("sheet-pivot-range", table.name);
      setChecked("sheet-pivot-header", table.header !== false);
      refreshPivotFields({ resetDefaults: true });
      note("sheet-pivot-status", "");
      return openDialog("sheet-pivot-dialog");
    }

    function openPivotSourceDialog(targetPivot) {
      const activeSheet = getActiveSheetName();
      if (!targetPivot) return false;
      if (host.__EASYOFFICE_SHEET_PROTECT__?.guardAction(activeSheet, "pivotTables") === false) return false;
      sheetHistory?.runtime?.getController?.()?.snapshotDomSheetCells();
      ensurePivotControls();
      pivot.editTarget = { sheet: activeSheet, name: targetPivot.name };
      setApplyButtonLabel("확인");
      const rangeDisplay = targetPivot.sourceTable
        ? targetPivot.sourceTable
        : targetPivot.sourceSheet && targetPivot.sourceSheet !== activeSheet
          ? formatSheetPrefix(targetPivot.sourceSheet) + targetPivot.sourceRange
          : targetPivot.sourceRange;
      setVal("sheet-pivot-range", rangeDisplay);
      setChecked("sheet-pivot-header", targetPivot.hasHeader !== false);
      refreshPivotFields({ resetDefaults: true });
      setSelect("sheet-pivot-filter-select", targetPivot.filterFields?.[0] !== undefined ? String(targetPivot.filterFields[0]) : "");
      setSelect("sheet-pivot-row-select", targetPivot.rowFields?.[0] !== undefined ? String(targetPivot.rowFields[0]) : "0");
      setSelect("sheet-pivot-row2-select", targetPivot.rowFields?.[1] !== undefined ? String(targetPivot.rowFields[1]) : "");
      setSelect("sheet-pivot-col-select", targetPivot.colFields?.[0] !== undefined ? String(targetPivot.colFields[0]) : "");
      setSelect("sheet-pivot-col2-select", targetPivot.colFields?.[1] !== undefined ? String(targetPivot.colFields[1]) : "");
      if (targetPivot.valueFields?.[0]) {
        setSelect("sheet-pivot-value-select", String(targetPivot.valueFields[0].col));
        setSelect("sheet-pivot-agg-select", targetPivot.valueFields[0].agg || "sum");
      }
      if (targetPivot.valueFields?.[1]) {
        setSelect("sheet-pivot-value2-select", String(targetPivot.valueFields[1].col));
        setSelect("sheet-pivot-agg2-select", targetPivot.valueFields[1].agg || "sum");
      } else {
        setSelect("sheet-pivot-value2-select", "");
        setSelect("sheet-pivot-agg2-select", "sum");
      }
      note("sheet-pivot-status", "");
      return openDialog("sheet-pivot-dialog");
    }

    function workbookPayload() {
      const cells = [];
      const sheets = Object.keys(sheetDataStore);
      for (const sheet of sheets) {
        for (const [cell, entry] of Object.entries(sheetDataStore[sheet] || {})) {
          const value = entry?.formula || entry?.value;
          if (value) cells.push({ sheet, cell, value });
        }
      }
      const namesApi = host.__EASYOFFICE_SHEET_NAMES__;
      return { sheets, cells, ...(namesApi?.hasAny() ? { names: namesApi.getAll() } : {}) };
    }

    function uniqueSheetKey() {
      const names = host.__EASYOFFICE_SHEET_TAB_NAMES__;
      const taken = Object.keys(sheetDataStore);
      let n = 1;
      while (names.isTaken("피벗" + n, taken)) n += 1;
      return "피벗" + n;
    }

    function readDialogSpec() {
      const src = resolveSourceInput(val("sheet-pivot-range"), fallbackDialogSheet());
      if (!src) return { error: "데이터 범위를 A1:D90 형식으로 입력하세요." };
      if (val("sheet-pivot-row-select") === "" || val("sheet-pivot-value-select") === "") {
        return { error: "데이터 범위를 입력하고 행과 값 열을 고르세요." };
      }
      const hasHeader = checked("sheet-pivot-header");
      const fieldNames = extractFieldNames(src.sourceSheet, src.box, hasHeader, src.table);
      const r1 = Number(val("sheet-pivot-row-select"));
      const r2Raw = val("sheet-pivot-row2-select");
      const rowFields = [r1];
      if (r2Raw !== "" && Number(r2Raw) !== r1) rowFields.push(Number(r2Raw));

      const c1Raw = val("sheet-pivot-col-select");
      const c2Raw = val("sheet-pivot-col2-select");
      const colFields = [];
      if (c1Raw !== "") colFields.push(Number(c1Raw));
      if (c2Raw !== "" && Number(c2Raw) !== colFields[0]) colFields.push(Number(c2Raw));

      const fRaw = val("sheet-pivot-filter-select");
      const filterFields = fRaw !== "" ? [Number(fRaw)] : [];

      const v1 = Number(val("sheet-pivot-value-select"));
      const agg1 = val("sheet-pivot-agg-select") || "sum";
      const aggText1 = optionsOf($("sheet-pivot-agg-select")).find((o) => o.value === agg1)?.textContent || model.aggLabelOf(agg1);
      const valueFields = [{ col: v1, agg: agg1, name: aggText1 + ": " + (fieldNames[v1] || "값") }];

      const v2Raw = val("sheet-pivot-value2-select");
      if (v2Raw !== "") {
        const v2 = Number(v2Raw);
        const agg2 = val("sheet-pivot-agg2-select") || "sum";
        if (!(v2 === v1 && agg2 === agg1)) {
          const aggText2 = optionsOf($("sheet-pivot-agg2-select"))?.find((o) => o.value === agg2)?.textContent || model.aggLabelOf(agg2);
          valueFields.push({ col: v2, agg: agg2, name: aggText2 + ": " + (fieldNames[v2] || "값") });
        }
      }
      return { src, hasHeader, fieldNames, rowFields, colFields, filterFields, valueFields };
    }

    function clearCellsInBox(sheet, box1) {
      const map = sheetDataStore[sheet];
      if (!map || !box1 || box1.r2 < box1.r1) return;
      for (let r = box1.r1; r <= box1.r2; r += 1) {
        for (let c = box1.c1; c <= box1.c2; c += 1) {
          const ref = rules.refOf(r - 1, c - 1);
          const cur = map[ref];
          if (!cur) continue;
          if (!cur.style && !cur.link) delete map[ref];
          else map[ref] = { ...cur, value: "", formula: "" };
        }
      }
    }

    function writePivotCellsIntoSheet(sheet, cellsMap) {
      const map = sheetDataStore[sheet] || (sheetDataStore[sheet] = {});
      for (const [ref, entry] of Object.entries(cellsMap)) {
        const prev = map[ref];
        map[ref] = prev ? { ...prev, value: entry.value, formula: "" } : { value: entry.value, formula: "" };
      }
    }

    async function repaintActiveSheet() {
      const controller = sheetHistory?.runtime?.getController?.();
      if (controller) {
        const active = getActiveSheetName();
        controller.restoreDomSheetCells(active);
        await controller.recalculateActiveSheetFormulas({ updateFocusedDisplay: true });
        const curRef = sheetHistory?.runtime?.getActiveSheetCellRef?.() || "A1";
        controller.updateSheetSelectionAndAggregates(curRef);
      }
      syncBar();
      host.__EASYOFFICE_SHEET_SLICER__?.syncLayer?.();
    }

    async function submitDialog() {
      if (pivot.editTarget) {
        await submitEditSource();
        return;
      }
      await createPivot();
    }

    async function createPivot() {
      sheetHistory?.runtime?.getController?.()?.snapshotDomSheetCells();
      const spec = readDialogSpec();
      if (spec.error) {
        note("sheet-pivot-status", spec.error);
        return;
      }
      const allPivots = everyPivotEntry().map((e) => e.pivot);
      if (allPivots.length >= model.MAX_PIVOTS_TOTAL) {
        note("sheet-pivot-status", "통합 문서 피벗 테이블 수 한도를 초과했습니다.");
        return;
      }
      const request = model.buildPivotRequest(workbookPayload(), spec.src.sourceSheet, spec.src.sourceRange, spec);
      let result;
      try {
        result = await sheetStoreHelpers.callSheetAction(request);
      } catch (err) {
        note("sheet-pivot-status", "피벗을 만들지 못했습니다: " + (err?.message || err));
        return;
      }
      if (!result || !Array.isArray(result.rowKeys)) {
        note("sheet-pivot-status", "피벗을 만들지 못했습니다.");
        return;
      }
      const created = await enqueueWorkspaceHistoryOp(async () => {
        await flushFocusedSheetCellBeforeSwitch();
        const guard = host.__EASYOFFICE_SHEET_PROTECT__;
        if (guard && !(guard.guardAction(getActiveSheetName(), "pivotTables") && guard.guardStructure())) return false;
        const change = sheetHistory.begin({ addsSheets: true, parts: ["sheetRowCounts", "sheetOrder", "sheetLayout"] });
        const key = uniqueSheetKey();
        const existingPivots = everyPivotEntry().map((e) => e.pivot);
        const built = model.buildPivotSheetCells(result, spec, 0, 0);
        const pivotEntry = {
          name: model.uniquePivotName(existingPivots),
          cacheId: model.resolveCacheId(existingPivots, spec.src.sourceSheet, spec.src.sourceRange, spec.src.sourceTable),
          sourceSheet: spec.src.sourceSheet,
          sourceRange: spec.src.sourceRange,
          sourceTable: spec.src.sourceTable,
          hasHeader: spec.hasHeader,
          location: built.location,
          fieldNames: spec.fieldNames,
          rowFields: spec.rowFields,
          colFields: spec.colFields,
          filterFields: spec.filterFields,
          valueFields: spec.valueFields,
          filters: [],
          slicers: [],
        };
        sheetDataStore[key] = built.cells;
        sheetRowCounts[key] = Math.max(12, built.rows);
        layoutApi?.setPivots(key, [pivotEntry]);
        host.__EASYOFFICE_SHEET_TAB_STRIP__.render({ document, keys: Object.keys(sheetDataStore), active: getActiveSheetName() });
        sheetHistory.commit(change);
        await performSwitchSheetTabAsync(key);
        syncBar();
        return true;
      });
      if (created !== false) closeDialog("sheet-pivot-dialog");
    }

    async function submitEditSource() {
      const target = pivot.editTarget;
      if (!target) return;
      sheetHistory?.runtime?.getController?.()?.snapshotDomSheetCells();
      const held = layoutApi?.pivotsOf(target.sheet).find((p) => p.name === target.name);
      if (!held) {
        closeDialog("sheet-pivot-dialog");
        return;
      }
      const spec = readDialogSpec();
      if (spec.error) {
        note("sheet-pivot-status", spec.error);
        return;
      }
      const maxField = spec.fieldNames.length;
      const keptFilters = (held.filters || []).filter((f) => f.col < maxField);
      const keptSlicers = (held.slicers || [])
        .filter((s) => s.col < maxField)
        .map((s) => ({ ...s, caption: spec.fieldNames[s.col] || s.caption }));
      const nextSpec = { ...spec, filters: keptFilters };
      const request = model.buildPivotRequest(workbookPayload(), spec.src.sourceSheet, spec.src.sourceRange, nextSpec);
      let result;
      try {
        result = await sheetStoreHelpers.callSheetAction(request);
      } catch (err) {
        note("sheet-pivot-status", "피벗을 갱신하지 못했습니다: " + (err?.message || err));
        return;
      }
      if (!result || !Array.isArray(result.rowKeys)) {
        note("sheet-pivot-status", "피벗을 갱신하지 못했습니다.");
        return;
      }
      const oldBox = layoutApi.pivotBox(held);
      const anchorRow0 = Math.max(0, oldBox.r1 - 1);
      const anchorCol0 = Math.max(0, oldBox.c1 - 1);
      const built = model.buildPivotSheetCells(result, nextSpec, anchorRow0, anchorCol0);
      const newBox = layoutApi.pivotBox({ location: built.location });
      if (spec.src.sourceSheet === target.sheet) {
        const srcBox1 = { r1: spec.src.box.row0 + 1, c1: spec.src.box.col0 + 1, r2: spec.src.box.row1 + 1, c2: spec.src.box.col1 + 1 };
        if (boxesOverlap1(newBox, srcBox1)) {
          note("sheet-pivot-status", "피벗 테이블 배치 영역이 데이터 원본 범위와 겹칩니다.");
          return;
        }
      }
      const conflictPivot = layoutApi.pivotsOf(target.sheet).some((p) => p.name !== held.name && boxesOverlap1(layoutApi.pivotBox(p), newBox));
      const conflictTable = layoutApi.tablesIn(target.sheet, newBox.r1, newBox.c1, newBox.r2, newBox.c2).length > 0;
      if (conflictPivot || conflictTable) {
        note("sheet-pivot-status", "피벗 테이블 영역이 다른 표 또는 피벗 테이블과 겹칩니다.");
        return;
      }
      const done = await enqueueWorkspaceHistoryOp(async () => {
        await flushFocusedSheetCellBeforeSwitch();
        if (host.__EASYOFFICE_SHEET_PROTECT__?.guardAction(target.sheet, "pivotTables") === false) return false;
        sheetHistory?.runtime?.getController?.()?.snapshotDomSheetCells();
        const curHeld = layoutApi.pivotsOf(target.sheet).find((p) => p.name === target.name);
        if (!curHeld) return false;
        const allPivots = everyPivotEntry().map((e) => e.pivot);
        const updated = {
          ...curHeld,
          cacheId: model.resolveCacheId(allPivots, spec.src.sourceSheet, spec.src.sourceRange, spec.src.sourceTable, curHeld),
          sourceSheet: spec.src.sourceSheet,
          sourceRange: spec.src.sourceRange,
          sourceTable: spec.src.sourceTable,
          hasHeader: spec.hasHeader,
          location: built.location,
          fieldNames: spec.fieldNames,
          rowFields: spec.rowFields,
          colFields: spec.colFields,
          filterFields: spec.filterFields,
          valueFields: spec.valueFields,
          filters: keptFilters,
          slicers: keptSlicers,
        };
        const change = sheetHistory.begin({
          bySheet: { [target.sheet]: unionRefs(oldBox, newBox) },
          parts: ["sheetLayout", "sheetRowCounts"],
        });
        clearCellsInBox(target.sheet, oldBox);
        writePivotCellsIntoSheet(target.sheet, built.cells);
        sheetRowCounts[target.sheet] = Math.max(sheetRowCounts[target.sheet] || 12, built.rows);
        layoutApi.setPivots(
          target.sheet,
          layoutApi.pivotsOf(target.sheet).map((p) => (p.name === curHeld.name ? updated : p))
        );
        sheetHistory.commit(change);
        await repaintActiveSheet();
        say("피벗 테이블 데이터 원본 변경");
        return true;
      });
      if (done) {
        pivot.editTarget = null;
        closeDialog("sheet-pivot-dialog");
      }
    }

    function syncPivotLiveSource(working) {
      if (working.sourceTable) {
        const tblHit = findTableAcrossSheets(working.sourceTable, working.sourceSheet);
        if (tblHit) {
          const b = layoutApi.tableBox(tblHit.table);
          const r2 = b.r2 - (tblHit.table.totals === true ? 1 : 0);
          if (r2 >= b.r1) {
            const box = { row0: b.r1 - 1, col0: b.c1 - 1, row1: r2 - 1, col1: b.c2 - 1 };
            working.sourceSheet = tblHit.sheet;
            working.sourceRange = rules.formatRanges([box]);
            working.hasHeader = tblHit.table.header !== false;
            working.fieldNames = extractFieldNames(tblHit.sheet, box, working.hasHeader, tblHit.table);
          }
        }
      } else {
        const box = model.parseSingleBox0(working.sourceRange);
        if (box) {
          working.fieldNames = extractFieldNames(working.sourceSheet, box, working.hasHeader !== false, null);
        }
      }
      const maxCol = working.fieldNames.length;
      if (maxCol > 0) {
        working.valueFields = working.valueFields
          .filter((vf) => vf.col < maxCol)
          .map((vf) => ({ ...vf, name: model.defaultValueFieldName(vf.agg, working.fieldNames[vf.col]) }));
        working.filters = (working.filters || []).filter((f) => f.col < maxCol);
        working.slicers = (working.slicers || [])
          .filter((s) => s.col < maxCol)
          .map((s) => ({ ...s, caption: working.fieldNames[s.col] || s.caption }));
      }
      return working;
    }

    async function refreshPivotTargets(targets, statusText) {
      if (!targets || targets.length === 0) return false;
      return enqueueWorkspaceHistoryOp(async () => {
        await flushFocusedSheetCellBeforeSwitch();
        const controller = sheetHistory?.runtime?.getController?.();
        if (controller) sheetDataStore[getActiveSheetName()] = controller.snapshotDomSheetCells();
        const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;
        const touchedSheets = new Set(targets.map((t) => t.sheet));
        for (const sh of touchedSheets) {
          if (protectApi && !protectApi.guardAction(sh, "pivotTables")) return false;
        }
        const wbPayload = workbookPayload();
        const plans = [];
        for (const t of targets) {
          const held = layoutApi.pivotsOf(t.sheet).find((p) => p.name === t.name);
          if (!held) continue;
          let working = structuredClone(held);
          if (t.mutate) working = t.mutate(working);
          syncPivotLiveSource(working);
          const req = model.buildPivotRequest(wbPayload, working.sourceSheet, working.sourceRange, working);
          let result;
          try {
            result = await sheetStoreHelpers.callSheetAction(req);
          } catch (err) {
            say("피벗 새로 고침 실패: " + (err?.message || err));
            return false;
          }
          if (!result || !Array.isArray(result.rowKeys)) return false;
          const oldBox = layoutApi.pivotBox(held);
          const built = model.buildPivotSheetCells(
            result,
            working,
            Math.max(0, oldBox.r1 - 1),
            Math.max(0, oldBox.c1 - 1)
          );
          const newBox = layoutApi.pivotBox({ location: built.location });
          working.location = built.location;
          plans.push({ sheet: t.sheet, oldName: held.name, oldBox, newBox, built, working });
        }
        if (plans.length === 0) return false;
        const blocked = model.refreshBlockedBy(plans, layoutApi);
        if (blocked) {
          say(blocked);
          return false;
        }
        if (plans.some((plan) => model.coversOtherData(plan, sheetDataStore[plan.sheet], rules.refOf))) {
          const replace = await host.__EASYOFFICE_DESKTOP_BRIDGE__.confirm("replace-pivot-cells");
          if (!replace) {
            say("피벗 새로 고침 취소");
            return false;
          }
        }

        const bySheet = {};
        for (const plan of plans) {
          const prev = bySheet[plan.sheet] || [];
          bySheet[plan.sheet] = [...new Set([...prev, ...unionRefs(plan.oldBox, plan.newBox)])];
        }
        const change = sheetHistory.begin({ bySheet, parts: ["sheetLayout", "sheetRowCounts"] });
        const nextPivotsBySheet = new Map();
        for (const sh of touchedSheets) {
          nextPivotsBySheet.set(sh, [...layoutApi.pivotsOf(sh)]);
        }
        for (const plan of plans) {
          clearCellsInBox(plan.sheet, plan.oldBox);
        }
        for (const plan of plans) {
          writePivotCellsIntoSheet(plan.sheet, plan.built.cells);
          sheetRowCounts[plan.sheet] = Math.max(sheetRowCounts[plan.sheet] || 12, plan.built.rows);
          const list = nextPivotsBySheet.get(plan.sheet) || [];
          const idx = list.findIndex((p) => p.name === plan.oldName);
          if (idx >= 0) list[idx] = plan.working;
        }
        for (const [sh, list] of nextPivotsBySheet.entries()) {
          layoutApi.setPivots(sh, list);
        }
        sheetHistory.commit(change);
        await repaintActiveSheet();
        if (statusText) say(statusText);
        return true;
      });
    }

    function refreshPivot(targetPivot) {
      const p = targetPivot || pivotAtActive();
      if (!p) return Promise.resolve(false);
      return refreshPivotTargets([{ sheet: getActiveSheetName(), name: p.name }], "피벗 테이블 새로 고침");
    }

    function refreshAllPivots() {
      const targets = everyPivotEntry().map((e) => ({ sheet: e.sheet, name: e.pivot.name }));
      if (targets.length === 0) return Promise.resolve(false);
      return refreshPivotTargets(targets, "모든 피벗 테이블 새로 고침");
    }

    function deletePivot(targetPivot) {
      const p = targetPivot || pivotAtActive();
      if (!p) return Promise.resolve(false);
      const sheet = getActiveSheetName();
      return enqueueWorkspaceHistoryOp(async () => {
        await flushFocusedSheetCellBeforeSwitch();
        if (host.__EASYOFFICE_SHEET_PROTECT__?.guardAction(sheet, "pivotTables") === false) return false;
        const controller = sheetHistory?.runtime?.getController?.();
        if (controller) sheetDataStore[sheet] = controller.snapshotDomSheetCells();
        const held = layoutApi.pivotsOf(sheet).find((x) => x.name === p.name);
        if (!held) return false;
        const oldBox = layoutApi.pivotBox(held);
        const change = sheetHistory.begin({
          bySheet: { [sheet]: refsInBox1(oldBox) },
          parts: ["sheetLayout"],
        });
        clearCellsInBox(sheet, oldBox);
        layoutApi.setPivots(
          sheet,
          layoutApi.pivotsOf(sheet).filter((x) => x.name !== held.name)
        );
        sheetHistory.commit(change);
        await repaintActiveSheet();
        say("피벗 테이블 삭제");
        return true;
      });
    }

    function renamePivot(targetPivot, toRaw) {
      const to = String(toRaw || "").trim();
      const sheet = getActiveSheetName();
      if (!targetPivot || to === targetPivot.name) return Promise.resolve(false);
      if (!to) {
        say("피벗 테이블 이름을 입력");
        return Promise.resolve(false);
      }
      if (Array.from(to).length > 255) {
        say("이름은 255자까지");
        return Promise.resolve(false);
      }
      const dup = everyPivotEntry().some((e) => e.pivot !== targetPivot && e.pivot.name.toLowerCase() === to.toLowerCase());
      if (dup) {
        say("이미 있는 이름");
        return Promise.resolve(false);
      }
      return enqueueWorkspaceHistoryOp(async () => {
        await flushFocusedSheetCellBeforeSwitch();
        if (host.__EASYOFFICE_SHEET_PROTECT__?.guardAction(sheet, "pivotTables") === false) return false;
        const held = layoutApi.pivotsOf(sheet).find((x) => x.name === targetPivot.name);
        if (!held) return false;
        const change = sheetHistory.begin({ parts: ["sheetLayout"] });
        layoutApi.setPivots(
          sheet,
          layoutApi.pivotsOf(sheet).map((x) => (x === held ? { ...held, name: to } : x))
        );
        sheetHistory.commit(change);
        syncBar();
        host.__EASYOFFICE_SHEET_SLICER__?.syncLayer?.();
        say("피벗 테이블 이름: " + to);
        return true;
      });
    }

    function mutatePivotLayoutOnly(sheet, pivotName, mutator, statusText) {
      return enqueueWorkspaceHistoryOp(async () => {
        await flushFocusedSheetCellBeforeSwitch();
        if (host.__EASYOFFICE_SHEET_PROTECT__?.guardAction(sheet, "pivotTables") === false) return false;
        const held = layoutApi.pivotsOf(sheet).find((x) => x.name === pivotName);
        if (!held) return false;
        const nextPivot = mutator(structuredClone(held));
        if (!nextPivot) return false;
        const change = sheetHistory.begin({ parts: ["sheetLayout"] });
        layoutApi.setPivots(
          sheet,
          layoutApi.pivotsOf(sheet).map((x) => (x.name === pivotName ? nextPivot : x))
        );
        sheetHistory.commit(change);
        syncBar();
        host.__EASYOFFICE_SHEET_SLICER__?.syncLayer?.();
        if (statusText) say(statusText);
        return true;
      });
    }

    function insertPivotSlicers(sheet, pivotName, addedSlicers) {
      return mutatePivotLayoutOnly(
        sheet,
        pivotName,
        (held) => ({ ...held, slicers: [...(held.slicers || []), ...addedSlicers] }),
        "슬라이서 삽입"
      );
    }

    function updatePivotSlicerMeta(sheet, pivotName, slicerIdx, patch, statusText) {
      return mutatePivotLayoutOnly(
        sheet,
        pivotName,
        (held) => {
          if (!Array.isArray(held.slicers) || !held.slicers[slicerIdx]) return null;
          const slicers = held.slicers.map((s, idx) => (idx === slicerIdx ? { ...s, ...patch } : s));
          return { ...held, slicers };
        },
        statusText
      );
    }

    function removePivotSlicer(sheet, pivotName, slicerIdx) {
      return mutatePivotLayoutOnly(
        sheet,
        pivotName,
        (held) => {
          if (!Array.isArray(held.slicers) || !held.slicers[slicerIdx]) return null;
          return { ...held, slicers: held.slicers.filter((_, idx) => idx !== slicerIdx) };
        },
        "슬라이서 삭제"
      );
    }

    function applySlicerFilter(sheet, pivotName, fieldCol, criteria) {
      const origin = layoutApi.pivotsOf(sheet).find((p) => p.name === pivotName);
      if (!origin) return Promise.resolve(false);
      const cacheId = origin.cacheId;
      const linked = everyPivotEntry().filter(
        (e) => e.pivot === origin || (Number.isInteger(cacheId) && cacheId > 0 && e.pivot.cacheId === cacheId)
      );
      const applyFilterPatch = (working) => {
        const rest = (working.filters || []).filter((f) => f.col !== fieldCol);
        if (criteria !== null && criteria !== undefined) {
          rest.push({
            col: fieldCol,
            values: [...(criteria.values || [])],
            blank: Boolean(criteria.blank),
          });
        }
        working.filters = rest;
        return working;
      };
      const targets = linked.map((e) => ({
        sheet: e.sheet,
        name: e.pivot.name,
        mutate: applyFilterPatch,
      }));
      return refreshPivotTargets(targets, criteria ? "피벗 슬라이서 필터 적용" : "피벗 슬라이서 필터 해제");
    }

    function syncBar() {
      const bar = $("sheet-pivot-bar");
      if (!bar) return;
      if ($("pane-sheet")?.hidden) {
        bar.hidden = true;
        return;
      }
      const cur = pivotAtActive();
      bar.hidden = !cur;
      if (!cur) return;
      const input = $("sheet-pivot-name");
      if (input && document.activeElement !== input) input.value = cur.name;
      const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;
      const locked = Boolean(protectApi && protectApi.isOn(getActiveSheetName()) && !protectApi.allows(getActiveSheetName(), "pivotTables"));
      for (const el of bar.querySelectorAll("input, button")) el.disabled = locked;
    }

    function handleBarClick(ev) {
      const btn = ev.target?.closest?.("[data-pivot-act]");
      const cur = pivotAtActive();
      if (!btn || !cur) return;
      const act = btn.dataset.pivotAct;
      if (act === "refresh") void refreshPivot(cur);
      else if (act === "refresh-all") void refreshAllPivots();
      else if (act === "source") openPivotSourceDialog(cur);
      else if (act === "slicer") {
        sheetHistory?.runtime?.getController?.()?.snapshotDomSheetCells();
        host.__EASYOFFICE_SHEET_SLICER__?.openDialogForPivot?.(cur);
      } else if (act === "delete") void deletePivot(cur);
    }

    function handleAltF5(ev) {
      if (ev.defaultPrevented || ev.__eoPivotHandled) return;
      if (!(ev.altKey && !ev.ctrlKey && !ev.metaKey && !ev.shiftKey && ev.key === "F5")) return;
      if ($("pane-sheet")?.hidden || document.querySelector?.("dialog[open]")) return;
      const cur = pivotAtActive();
      if (!cur) return;
      ev.__eoPivotHandled = true;
      ev.preventDefault?.();
      void refreshPivot(cur);
    }

    function ensureBarBound() {
      if (!boundDocs.has(document)) {
        boundDocs.add(document);
        const bar = $("sheet-pivot-bar");
        bar?.addEventListener("click", (ev) => host.__EASYOFFICE_SHEET_PIVOT__?._handleBarClick?.(ev));
        const nameInput = $("sheet-pivot-name");
        nameInput?.addEventListener("keydown", (ev) => {
          const apiRef = host.__EASYOFFICE_SHEET_PIVOT__;
          const cur = apiRef?.pivotAtActive?.();
          if (ev.key === "Enter" && cur) {
            ev.preventDefault?.();
            void apiRef.renamePivot(cur, nameInput.value).then((done) => {
              if (!done) nameInput.value = apiRef.pivotAtActive()?.name ?? "";
              document.querySelector?.("#pane-sheet td.selected-cell")?.focus?.();
            });
          } else if (ev.key === "Escape") {
            ev.preventDefault?.();
            nameInput.value = cur?.name ?? "";
            document.querySelector?.("#pane-sheet td.selected-cell")?.focus?.();
          }
        });
        nameInput?.addEventListener("blur", () => {
          const apiRef = host.__EASYOFFICE_SHEET_PIVOT__;
          const cur = apiRef?.pivotAtActive?.();
          if (!cur || nameInput.value.trim() === cur.name) return;
          void apiRef.renamePivot(cur, nameInput.value).then((done) => {
            if (!done) nameInput.value = apiRef.pivotAtActive()?.name ?? "";
          });
        });
        const pane = $("pane-sheet");
        pane?.addEventListener("keydown", (ev) => host.__EASYOFFICE_SHEET_PIVOT__?._handleAltF5?.(ev));
        pane?.addEventListener("focusin", () => host.__EASYOFFICE_SHEET_PIVOT__?.syncBar?.());
        pane?.addEventListener("click", () => host.__EASYOFFICE_SHEET_PIVOT__?.syncBar?.());
        layoutApi?.onChange(() => host.__EASYOFFICE_SHEET_PIVOT__?.syncBar?.());
      }
      if (!windowKeyBound && typeof host.addEventListener === "function") {
        windowKeyBound = true;
        host.addEventListener("keydown", (ev) => host.__EASYOFFICE_SHEET_PIVOT__?._handleAltF5?.(ev));
      }
    }

    ensureBarBound();
    refreshPivotFields({ resetDefaults: true });

    const api = {
      openPivotDialog,
      openPivotDialogForTable,
      openPivotSourceDialog,
      pivotCells: model.pivotCells,
      refreshPivot,
      refreshAllPivots,
      deletePivot,
      renamePivot,
      pivotAtActive,
      syncBar,
      insertPivotSlicers,
      updatePivotSlicerMeta,
      removePivotSlicer,
      applySlicerFilter,
      _onFieldInputChanged: () => refreshPivotFields(),
      _submitDialog: submitDialog,
      _handleBarClick: handleBarClick,
      _handleAltF5: handleAltF5,
    };
    host.__EASYOFFICE_SHEET_PIVOT__ = api;
    return api;
  }

  host.__EASYOFFICE_CREATE_SHEET_PIVOT_UI__ = createSheetPivotUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
