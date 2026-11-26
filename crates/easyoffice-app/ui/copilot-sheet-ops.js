// 시트(SHEET) 코파일럿 확장 연산(표·피벗 테이블·조건부 서식·정렬/필터·행열 구조/틀 고정·메모),
// 전송 시점 선택 고정(FrozenSelection), 온디맨드 읽기 도구, 지문(미커밋 DOM 편집 감지 포함) 및 수식 오류 점검 (PD-359).

(function (host) {
  "use strict";

  const MAX_RANGE_CELLS = 4096;
  const MAX_READ_TOOL_CELLS = 512;
  const MAX_FIND_MATCHES = 64;
  const FORMULA_ERROR_CODES = new Set([
    "#REF!", "#NAME?", "#VALUE!", "#CYCLE!", "#DIV/0!", "#N/A", "#NUM!", "#NULL!", "#SPILL!",
  ]);

  function compareCoordEntries(a, b) {
    return a.row - b.row || a.col - b.col;
  }

  function formatSelectionA1(rg, colName) {
    if (!rg || !Number.isInteger(rg.r1) || !Number.isInteger(rg.c1)) return "A1";
    const start = colName(rg.c1) + rg.r1;
    if (rg.r1 === rg.r2 && rg.c1 === rg.c2) return start;
    return start + ":" + colName(rg.c2) + rg.r2;
  }

  function parseRangeBox1(rawRange) {
    const names = host.__EASYOFFICE_SHEET_NAMES__;
    const { parseRef } = host.__EASYOFFICE_SHEET_REFS__;
    const cleaned = String(rawRange || "").trim().toUpperCase().replace(/^[^!]+!/, "");
    if (!cleaned) return null;
    const rg = names?.parseAddress?.(cleaned);
    if (rg && Number.isInteger(rg.r1) && Number.isInteger(rg.c1)) {
      return {
        r1: Math.min(rg.r1, rg.r2),
        c1: Math.min(rg.c1, rg.c2),
        r2: Math.max(rg.r1, rg.r2),
        c2: Math.max(rg.c1, rg.c2),
      };
    }
    const single = parseRef(cleaned);
    if (!single) return null;
    return { r1: single.row, c1: single.col, r2: single.row, c2: single.col };
  }

  function freezeSelection(sheetName, sheetController) {
    const { colName } = host.__EASYOFFICE_SHEET_REFS__;
    const rg = sheetController?.getSelectionRange?.();
    if (!rg || !Number.isInteger(rg.r1) || !Number.isInteger(rg.c1)) {
      return Object.freeze({ sheet: sheetName, range: "A1", r1: 1, c1: 1, r2: 1, c2: 1 });
    }
    const box = {
      r1: Math.min(rg.r1, rg.r2),
      c1: Math.min(rg.c1, rg.c2),
      r2: Math.max(rg.r1, rg.r2),
      c2: Math.max(rg.c1, rg.c2),
    };
    return Object.freeze({
      sheet: sheetName,
      range: formatSelectionA1(box, colName),
      ...box,
    });
  }

  function cellValueKindForPivot(entry) {
    if (!entry) return { kind: "empty", display: "" };
    const rawVal = String(entry.value ?? "");
    const rawFormula = String(entry.formula ?? "");
    if (rawVal === "" && rawFormula === "") return { kind: "empty", display: "" };
    if (FORMULA_ERROR_CODES.has(rawVal.trim().toUpperCase())) {
      return { kind: "error", display: rawVal.trim() };
    }
    if (entry.t === "b" || rawVal === "TRUE" || rawVal === "FALSE") {
      return { kind: "bool", text: rawVal.toUpperCase(), display: rawVal.toUpperCase() };
    }
    const num = Number(rawVal);
    if (entry.t !== "s" && rawVal.trim() !== "" && Number.isFinite(num)) {
      return { kind: "number", num, display: rawVal };
    }
    return { kind: "text", text: rawVal, display: rawVal };
  }

  function executeSheetTable(op, sheetName, sheetDataStore, sheetRowCounts, locations) {
    const guard = host.__EASYOFFICE_SHEET_PROTECT__;
    if (guard?.isOn(sheetName)) throw new Error("보호된 시트에서는 표를 편집할 수 없습니다.");
    const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
    const tableOps = host.__EASYOFFICE_SHEET_TABLE_OPS__;
    const tableBake = host.__EASYOFFICE_SHEET_TABLE_BAKE__;
    const { colName, refOf } = host.__EASYOFFICE_SHEET_REFS__;
    if (!layout || !tableOps) throw new Error("표 모듈을 사용할 수 없습니다.");

    const map = sheetDataStore[sheetName] || (sheetDataStore[sheetName] = Object.create(null));
    const existing = layout.tablesOf(sheetName).map((t) => structuredClone(t));
    const action = String(op.action || "create").trim();

    if (action === "create") {
      const box = parseRangeBox1(op.range);
      if (!box) throw new Error("유효하지 않은 표 범위입니다: " + String(op.range || ""));
      if ((box.r2 - box.r1 + 1) * (box.c2 - box.c1 + 1) > MAX_RANGE_CELLS) {
        throw new Error("표 범위가 허용 한도를 초과했습니다.");
      }
      const takenNames = Object.keys(sheetDataStore).flatMap((s) => layout.tablesOf(s).map((t) => t.name));
      const ctx = {
        tables: existing,
        merges: layout.mergesIn(sheetName, 1, 1, 1048576, 16384),
        filter: layout.filterOf(sheetName),
        boxOf: (t) => layout.tableBox(t),
        cellAt: (r, c) => map[refOf(r, c)] || null,
      };
      const plan = tableOps.planCreate({
        box,
        header: true,
        name: op.tableName || tableOps.defaultName(takenNames),
        ctx,
        totalTables: takenNames.length,
        takenNames,
      });
      if (plan.error) throw new Error(plan.error);
      if (op.style) plan.table.style = String(op.style);
      if (typeof op.totalRow === "boolean") plan.table.totals = op.totalRow;
      for (const hw of plan.headerWrites || []) {
        const ref = refOf(hw.row, hw.col);
        map[ref] = { ...(map[ref] || {}), value: hw.text, formula: "" };
      }
      const boxAfter = layout.tableBox(plan.table);
      if (boxAfter.r2 > (sheetRowCounts[sheetName] || 12)) sheetRowCounts[sheetName] = boxAfter.r2;
      layout.setTables(sheetName, [...existing, plan.table]);
      locations.push({ module: "SHEET", label: plan.table.name, sheet: sheetName, range: plan.table.range });
      return;
    }

    const targetIdx = existing.findIndex((t) =>
      (op.tableName && t.name.toLowerCase() === String(op.tableName).toLowerCase())
      || (op.range && t.range.toUpperCase() === String(op.range).toUpperCase()),
    );
    const idx = targetIdx >= 0 ? targetIdx : (existing.length > 0 ? 0 : -1);
    if (idx < 0) throw new Error("대상 시트 표를 찾을 수 없습니다.");
    const target = existing[idx];

    if (action === "style" && op.style) {
      target.style = String(op.style);
      layout.setTables(sheetName, existing);
    } else if (action === "total_row") {
      const nextTotals = typeof op.totalRow === "boolean" ? op.totalRow : !target.totals;
      const b = layout.tableBox(target);
      if (nextTotals && !target.totals) {
        const totRow = b.r2 + 1;
        target.range = formatSelectionA1({ r1: b.r1, c1: b.c1, r2: totRow, c2: b.c2 }, colName);
        target.totals = true;
        const firstRef = refOf(totRow, b.c1);
        if (!map[firstRef]?.value && !map[firstRef]?.formula) {
          map[firstRef] = { value: "합계", formula: "" };
        }
        if (b.c2 > b.c1) {
          const lastCol = colName(b.c2);
          const sumRef = refOf(totRow, b.c2);
          map[sumRef] = { value: "", formula: `=SUBTOTAL(109,${lastCol}${b.r1 + 1}:${lastCol}${b.r2})` };
        }
        if (totRow > (sheetRowCounts[sheetName] || 12)) sheetRowCounts[sheetName] = totRow;
      } else if (!nextTotals && target.totals && b.r2 > b.r1 + 1) {
        for (let c = b.c1; c <= b.c2; c += 1) delete map[refOf(b.r2, c)];
        target.range = formatSelectionA1({ r1: b.r1, c1: b.c1, r2: b.r2 - 1, c2: b.c2 }, colName);
        target.totals = false;
      }
      layout.setTables(sheetName, existing);
    } else if (action === "to_range") {
      if (tableBake?.plan && tableBake?.apply) {
        const baked = tableBake.plan(target, layout.tableBox(target), (r, c) => map[refOf(r, c)]);
        if (!baked.error) tableBake.apply(baked, map, refOf);
      }
      existing.splice(idx, 1);
      layout.setTables(sheetName, existing);
    }
    locations.push({ module: "SHEET", label: target.name, sheet: sheetName, range: target.range });
  }

  function executeSheetPivot(op, sheetName, sheetDataStore, sheetRowCounts, locations) {
    const pivotModel = host.__EASYOFFICE_SHEET_PIVOT_MODEL__;
    const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
    const guard = host.__EASYOFFICE_SHEET_PROTECT__;
    const { colName, refOf } = host.__EASYOFFICE_SHEET_REFS__;
    if (!pivotModel || !layout) throw new Error("피벗 테이블 모듈을 사용할 수 없습니다.");

    const srcBox = parseRangeBox1(op.sourceRange);
    if (!srcBox || srcBox.r2 <= srcBox.r1) {
      throw new Error("유효한 피벗 데이터 원본 범위(머리글 포함 2행 이상)가 필요합니다.");
    }
    const srcMap = sheetDataStore[sheetName] || {};
    const headers = [];
    for (let c = srcBox.c1; c <= srcBox.c2; c += 1) {
      headers.push(String(srcMap[refOf(srcBox.r1, c)]?.value ?? "").trim());
    }
    const resolveColOffset = (fieldSpec, fallbackIdx) => {
      if (fieldSpec === undefined || fieldSpec === null || fieldSpec === "") return fallbackIdx;
      const cleaned = String(fieldSpec).trim();
      const idxByName = headers.findIndex((h) => h.toLowerCase() === cleaned.toLowerCase());
      if (idxByName >= 0) return idxByName;
      const asNum = Number(cleaned);
      if (Number.isInteger(asNum) && asNum >= 0 && asNum < headers.length) return asNum;
      return fallbackIdx;
    };

    const rawRowSpecs = Array.isArray(op.rowFields) ? op.rowFields : (op.rowField ? [op.rowField] : []);
    const rowCols = rawRowSpecs
      .map((f) => resolveColOffset(f, -1))
      .filter((i) => i >= 0);
    if (rowCols.length === 0) rowCols.push(0);
    const rawColSpecs = Array.isArray(op.colFields) ? op.colFields : (op.colField ? [op.colField] : []);
    const colCols = rawColSpecs
      .map((f) => resolveColOffset(f, -1))
      .filter((i) => i >= 0 && !rowCols.includes(i));
    const valCol = resolveColOffset(op.valueField, Math.min(headers.length - 1, Math.max(0, headers.length - 1)));
    const agg = String(op.agg || "sum").toLowerCase();

    const destSheet = String(op.destSheet || sheetName).trim() || sheetName;
    if (!Object.hasOwn(sheetDataStore, destSheet)) {
      if (destSheet !== sheetName) throw new Error("대상 시트가 존재하지 않습니다: " + destSheet);
      sheetDataStore[destSheet] = Object.create(null);
    }
    if (guard?.isOn(destSheet)) throw new Error("보호된 시트에는 피벗 테이블을 배치할 수 없습니다.");

    let destCoord = op.destCell ? parseRangeBox1(op.destCell) : null;
    if (!destCoord) {
      const autoCol = destSheet === sheetName ? srcBox.c2 + 2 : 1;
      destCoord = { r1: 1, c1: autoCol, r2: 1, c2: autoCol };
    }

    const srcA1 = formatSelectionA1(srcBox, colName);
    const docSpec = {
      range: srcA1,
      header: true,
      rowFields: rowCols,
      colFields: colCols,
      valueFields: [{
        col: valCol,
        agg,
        name: pivotModel.defaultValueFieldName(agg, headers[valCol] || colName(srcBox.c1 + valCol)),
      }],
    };
    const computed = pivotModel.computePivotWithReader(docSpec, (r0, c0) =>
      cellValueKindForPivot(srcMap[refOf(r0 + 1, c0 + 1)]),
    );
    const destRow0 = destCoord.r1 - 1;
    const destCol0 = destCoord.c1 - 1;
    const built = pivotModel.buildPivotSheetCells(computed, destRow0, destCol0, (r0, c0) => refOf(r0 + 1, c0 + 1));

    const dstMap = sheetDataStore[destSheet];
    for (const [ref, cell] of Object.entries(built.cells || {})) {
      dstMap[ref] = { value: String(cell.value ?? ""), formula: "", ...(cell.style ? { style: cell.style } : {}) };
    }
    const endRow = destCoord.r1 + Math.max(1, (computed.grid?.length || 1)) - 1;
    const endCol = destCoord.c1 + Math.max(1, (computed.grid?.[0]?.length || 1)) - 1;
    if (endRow > (sheetRowCounts[destSheet] || 12)) sheetRowCounts[destSheet] = endRow;

    const allPivots = Object.keys(sheetDataStore).flatMap((s) => layout.pivotsOf(s));
    const pivotName = pivotModel.uniquePivotName(allPivots);
    const destRange = formatSelectionA1({ r1: destCoord.r1, c1: destCoord.c1, r2: endRow, c2: endCol }, colName);
    const pivotRecord = {
      name: pivotName,
      cacheId: pivotModel.resolveCacheId(allPivots, sheetName, srcA1),
      sourceSheet: sheetName,
      sourceRange: srcA1,
      location: destRange,
      rowFields: rowCols,
      colFields: colCols,
      filterFields: [],
      dataFields: docSpec.valueFields,
    };
    layout.setPivots(destSheet, [...layout.pivotsOf(destSheet), pivotRecord]);
    locations.push({ module: "SHEET", label: pivotName, sheet: destSheet, range: destRange });
  }

  function executeSheetCondFormat(op, sheetName, locations) {
    const guard = host.__EASYOFFICE_SHEET_PROTECT__;
    if (guard?.isOn(sheetName)) throw new Error("보호된 시트에서는 조건부 서식을 변경할 수 없습니다.");
    const sheetRules = host.__EASYOFFICE_SHEET_RULES__;
    const presets = host.__EASYOFFICE_SHEET_CF_PRESETS__;
    const look = host.__EASYOFFICE_SHEET_LOOK__;
    const { colName } = host.__EASYOFFICE_SHEET_REFS__;
    if (!sheetRules) throw new Error("조건부 서식 모듈을 사용할 수 없습니다.");

    const box = parseRangeBox1(op.range);
    if (!box) throw new Error("유효하지 않은 조건부 서식 범위입니다: " + String(op.range || ""));
    const rangeStr = formatSelectionA1(box, colName);
    const bg = op.fill ? (look?.normalizeColor?.(op.fill) ? "#" + look.normalizeColor(op.fill).replace(/^#/, "") : String(op.fill)) : "#ffc7ce";
    const fg = op.color ? (look?.normalizeColor?.(op.color) ? "#" + look.normalizeColor(op.color).replace(/^#/, "") : String(op.color)) : "#9c0006";

    let rule;
    const preset = String(op.preset || op.ruleType || "cellIs").trim();
    const val1 = op.value1 ?? op.formula1 ?? op.value;
    const val2 = op.value2 ?? op.formula2;
    if (preset === "dataBar" && presets?.barRule) {
      rule = presets.barRule(rangeStr, { pos: op.fill || "#638ec6", gradient: true });
    } else if (preset === "colorScale" && presets?.scaleRule) {
      rule = presets.scaleRule(rangeStr, presets.SCALES?.[0] || { colors: ["#63be7b", "#ffeb84", "#f8696b"] });
    } else if (preset === "iconSet" && presets?.iconRule) {
      rule = presets.iconRule(rangeStr, presets.ICONS?.[0] || "3TrafficLights1");
    } else if (preset === "textContains" || preset === "containsText") {
      rule = { r: rangeStr, t: "text", op: "contains", text: String(val1 ?? ""), bg, fg, stop: false, fl: 0 };
    } else if (preset === "top10") {
      const n = Math.max(1, Math.min(1000, Number.parseInt(String(val1 || "10"), 10) || 10));
      rule = { r: rangeStr, t: "top", n, percent: false, bottom: false, bg, fg, stop: false, fl: 0 };
    } else if (preset === "aboveAverage" || preset === "belowAverage") {
      rule = { r: rangeStr, t: "average", above: preset !== "belowAverage" && op.operator !== "below", equal: false, std: 0, bg, fg, stop: false, fl: 0 };
    } else if (preset === "duplicateValues") {
      rule = { r: rangeStr, t: "dup", unique: false, bg, fg, stop: false, fl: 0 };
    } else if (preset === "expression") {
      rule = { r: rangeStr, t: "expr", f: String(val1 || "=TRUE"), bg, fg, stop: false, fl: 0 };
    } else {
      const cellOp = ["greaterThan", "lessThan", "between", "equal"].includes(preset)
        ? preset
        : String(op.operator || "greaterThan");
      rule = {
        r: rangeStr,
        t: "cellIs",
        op: cellOp,
        a: String(val1 ?? "0"),
        b: String(val2 ?? ""),
        bg,
        fg,
        stop: false,
        fl: 0,
      };
    }

    const cur = sheetRules.get(sheetName);
    sheetRules.set(sheetName, { cf: [...(cur.cf || []), rule], dv: cur.dv || [] });
    locations.push({ module: "SHEET", label: "조건부 서식 " + rangeStr, sheet: sheetName, range: rangeStr });
  }

  function executeSheetSortFilter(op, sheetName, sheetDataStore, locations) {
    const guard = host.__EASYOFFICE_SHEET_PROTECT__;
    const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
    const { colName, refOf } = host.__EASYOFFICE_SHEET_REFS__;
    const action = String(op.action || "").trim();
    const box = parseRangeBox1(op.range);
    if (!box) throw new Error("유효하지 않은 정렬/필터 범위입니다: " + String(op.range || ""));
    const rangeStr = formatSelectionA1(box, colName);

    if (action === "clear_filter") {
      layout?.setFilter?.(sheetName, null);
      locations.push({ module: "SHEET", label: "필터 해제", sheet: sheetName, range: rangeStr });
      return;
    }

    const map = sheetDataStore[sheetName] || (sheetDataStore[sheetName] = Object.create(null));
    const colOffset = Math.max(0, Math.min(box.c2 - box.c1, Number(op.colOffset) || 0));
    const targetCol = box.c1 + colOffset;

    if (action === "filter") {
      if (guard?.isOn(sheetName) && !guard.guardAction?.(sheetName, "autoFilter")) {
        throw new Error("보호된 시트에서는 자동 필터를 사용할 수 없습니다.");
      }
      const hiddenRuns = [];
      const query = String(op.query ?? "").trim().toLowerCase();
      if (query && box.r2 > box.r1) {
        let runStart = null;
        for (let r = box.r1 + 1; r <= box.r2; r += 1) {
          const val = String(map[refOf(r, targetCol)]?.value ?? "").toLowerCase();
          const hide = !val.includes(query);
          if (hide && runStart === null) runStart = r;
          else if (!hide && runStart !== null) {
            hiddenRuns.push([runStart, r - 1]);
            runStart = null;
          }
        }
        if (runStart !== null) hiddenRuns.push([runStart, box.r2]);
      }
      layout?.setFilter?.(sheetName, {
        r1: box.r1,
        c1: box.c1,
        r2: box.r2,
        c2: box.c2,
        hidden: hiddenRuns,
        cols: query ? { [colOffset]: { op: "contains", text: query } } : {},
      });
      locations.push({ module: "SHEET", label: "필터 " + rangeStr, sheet: sheetName, range: rangeStr });
      return;
    }

    // sort
    if (guard?.isOn(sheetName) && !guard.guardAction?.(sheetName, "sort")) {
      throw new Error("보호된 시트에서는 정렬할 수 없습니다.");
    }
    if ((box.r2 - box.r1 + 1) * (box.c2 - box.c1 + 1) > MAX_RANGE_CELLS) {
      throw new Error("정렬 범위가 허용 한도를 초과했습니다.");
    }
    const sortCore = host.__EASYOFFICE_SHEET_SORT_CORE__;
    const hasHeader = box.r2 > box.r1 && sortCore?.detectHeader
      ? sortCore.detectHeader(box, (r, c) => map[refOf(r, c)])
      : box.r2 > box.r1;
    const dataStart = box.r1 + (hasHeader ? 1 : 0);
    if (dataStart > box.r2) return;

    const rows = [];
    for (let r = dataStart; r <= box.r2; r += 1) {
      const cells = [];
      for (let c = box.c1; c <= box.c2; c += 1) {
        const cur = map[refOf(r, c)];
        cells.push(cur ? structuredClone(cur) : null);
      }
      const keyRaw = String(map[refOf(r, targetCol)]?.value ?? "").trim();
      const keyNum = keyRaw !== "" ? Number(keyRaw) : NaN;
      rows.push({ origRow: r, cells, isBlank: keyRaw === "", isNum: Number.isFinite(keyNum), num: keyNum, text: keyRaw });
    }
    const dir = op.ascending === false ? -1 : 1;
    rows.sort((a, b) => {
      if (a.isBlank !== b.isBlank) return a.isBlank ? 1 : -1;
      if (a.isBlank) return a.origRow - b.origRow;
      if (a.isNum && b.isNum) return (a.num - b.num) * dir || (a.origRow - b.origRow);
      if (a.isNum !== b.isNum) return (a.isNum ? -1 : 1) * dir;
      return a.text.localeCompare(b.text, "ko") * dir || (a.origRow - b.origRow);
    });
    for (let idx = 0; idx < rows.length; idx += 1) {
      const dstR = dataStart + idx;
      const rowCells = rows[idx].cells;
      for (let c = box.c1; c <= box.c2; c += 1) {
        const ref = refOf(dstR, c);
        const cell = rowCells[c - box.c1];
        if (cell) map[ref] = cell;
        else delete map[ref];
      }
    }
    locations.push({ module: "SHEET", label: "정렬 " + rangeStr, sheet: sheetName, range: rangeStr });
  }

  function executeSheetStructure(op, sheetName, sheetDataStore, sheetRowCounts, locations) {
    const guard = host.__EASYOFFICE_SHEET_PROTECT__;
    const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
    const { colName, parseRef, refOf } = host.__EASYOFFICE_SHEET_REFS__;
    const action = String(op.action || "").trim();

    if (action === "freeze" || action === "unfreeze") {
      const rows = action === "unfreeze" ? 0 : Math.max(0, Math.min(64, Number(op.freezeRows ?? op.index ?? op.count ?? 1) || 0));
      const cols = action === "unfreeze" ? 0 : Math.max(0, Math.min(64, Number(op.freezeCols) || 0));
      layout?.setFreeze?.(sheetName, { rows, cols });
      const label = rows === 0 && cols === 0 ? "틀 고정 해제" : `틀 고정 (${rows}행 ${cols}열)`;
      locations.push({ module: "SHEET", label, sheet: sheetName, range: "A1" });
      return;
    }

    if (guard?.isOn(sheetName)) throw new Error("보호된 시트에서는 행·열 구조를 변경할 수 없습니다.");
    const at = Math.max(1, Number(op.at ?? op.index) || 1);
    const count = Math.max(1, Math.min(64, Number(op.count) || 1));
    const isRow = action === "insert_rows" || action === "delete_rows";
    const isInsert = action === "insert_rows" || action === "insert_cols";
    const edit = { axis: isRow ? "row" : "col", op: isInsert ? "insert" : "delete", at, count };

    const map = sheetDataStore[sheetName] || Object.create(null);
    const nextMap = Object.create(null);
    for (const [coord, entry] of Object.entries(map)) {
      const p = parseRef(coord);
      if (!p) continue;
      const nextRow = isRow ? layout.mapIndex(edit, p.row) : p.row;
      const nextCol = !isRow ? layout.mapIndex(edit, p.col) : p.col;
      if (nextRow === null || nextCol === null) continue;
      nextMap[refOf(nextRow, nextCol)] = entry;
    }
    sheetDataStore[sheetName] = nextMap;
    if (isRow) {
      const curRows = sheetRowCounts[sheetName] || 12;
      sheetRowCounts[sheetName] = isInsert ? curRows + count : Math.max(12, curRows - count);
    }
    const labelRef = isRow ? `A${at}` : `${colName(at)}1`;
    locations.push({ module: "SHEET", label: labelRef, sheet: sheetName, range: labelRef });
  }

  function executeSheetNote(op, sheetName, locations) {
    const guard = host.__EASYOFFICE_SHEET_PROTECT__;
    const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
    if (!layout) throw new Error("시트 메모 모듈을 사용할 수 없습니다.");
    if (guard?.isOn(sheetName)) throw new Error("보호된 시트에서는 메모를 편집할 수 없습니다.");

    const cell = String(op.coord || op.cell || "").trim().toUpperCase();
    const notes = layout.notesOf(sheetName).map((n) => ({ ...n }));
    const idx = notes.findIndex((n) => String(n.ref || "").toUpperCase() === cell);
    const text = String(op.text ?? "").trim();

    if (!text) {
      if (idx >= 0) notes.splice(idx, 1);
    } else if (idx >= 0) {
      notes[idx] = { ...notes[idx], text };
    } else {
      notes.push({ ref: cell, author: "코파일럿", text });
    }
    layout.setNotes(sheetName, notes);
    locations.push({ module: "SHEET", label: "메모 " + cell, sheet: sheetName, range: cell });
  }

  function previewEntryForExtendedSheetOp(op, change, getActiveSheetName) {
    const kind = String(op.op || op.type || op.kind || "");
    const sheet = String(op.sheet || getActiveSheetName());
    if (kind === "sheet.table") {
      const target = String(op.range || op.tableName || op.name || "");
      return change("format", `표 (${op.action || "create"}) ${target}`.trim(), "", "", { module: "SHEET", sheet, range: String(op.range || "A1") });
    }
    if (kind === "sheet.pivot") {
      const dst = String(op.destSheet || sheet);
      const rFields = Array.isArray(op.rowFields) ? op.rowFields : (op.rowField ? [op.rowField] : []);
      return change("insert", `피벗 테이블 (${op.sourceRange})`, "", `${rFields.join(", ")} · ${op.agg || "sum"}`, { module: "SHEET", sheet: dst, range: String(op.destCell || "A1") });
    }
    if (kind === "sheet.cond_format") {
      return change("format", `조건부 서식 ${op.range || ""}`, "", String(op.preset || op.ruleType || "cellIs"), { module: "SHEET", sheet, range: String(op.range || "A1") });
    }
    if (kind === "sheet.sort_filter") {
      const labelMap = { sort: "정렬", filter: "자동 필터", clear_filter: "필터 지우기" };
      return change("format", `${labelMap[op.action] || op.action} ${op.range || ""}`, "", String(op.query || ""), { module: "SHEET", sheet, range: String(op.range || "A1") });
    }
    if (kind === "sheet.structure") {
      return change("format", `구조 변경 (${op.action})`, "", "", { module: "SHEET", sheet, range: "A1" });
    }
    if (kind === "sheet.note") {
      const cell = String(op.coord || op.cell || "A1").toUpperCase();
      return change(op.text ? "insert" : "replace", `메모 ${cell}`, "", String(op.text || "(삭제)"), { module: "SHEET", sheet, range: cell });
    }
    return null;
  }

  function detectUncommittedDomEdit(document, sheetDataStore, activeSheet) {
    if (!document) return "";
    const inlineEditor = document.getElementById("sheet-inline-editor");
    if (inlineEditor && !inlineEditor.hidden) {
      return "inline:" + String(inlineEditor.value ?? inlineEditor.textContent ?? "");
    }
    const activeEl = document.activeElement;
    const formulaBar = document.getElementById("formula-input");
    const activeRefText = String(document.getElementById("active-cell-ref")?.textContent || "A1").trim().toUpperCase().split(":")[0];
    if (formulaBar && !formulaBar.disabled && !formulaBar.readOnly) {
      const cell = sheetDataStore[activeSheet]?.[activeRefText];
      const cellTypeApi = host.__EASYOFFICE_SHEET_CELL_TYPE__;
      const stored = cellTypeApi?.editableRaw
        ? cellTypeApi.editableRaw(cell?.value || "", cell?.formula || "", cell?.t, cell?.style?.numFmt)
        : (cell ? String(cell.formula || cell.value || "") : "");
      const draft = String(formulaBar.value ?? "");
      if (draft !== stored) return "fbar:" + activeRefText + ":" + draft;
    }
    const focusedTd = activeEl?.tagName?.toUpperCase() === "TD" ? activeEl : activeEl?.closest?.(".sheet-grid td");
    if (focusedTd?.dataset?.cell) {
      const ref = String(focusedTd.dataset.cell).toUpperCase();
      const cell = sheetDataStore[activeSheet]?.[ref];
      const storedVal = String(cell?.value ?? "");
      const domText = String(focusedTd.textContent ?? "");
      if (focusedTd.isContentEditable && domText !== storedVal) {
        return "td:" + ref + ":" + domText;
      }
    }
    return "";
  }

  function snapshotRangeState(store, rawRange) {
    const { refOf } = host.__EASYOFFICE_SHEET_REFS__;
    const box = parseRangeBox1(rawRange);
    if (!box) return null;
    const out = [];
    const capR = Math.min(box.r2, box.r1 + 63);
    const capC = Math.min(box.c2, box.c1 + 31);
    for (let r = box.r1; r <= capR; r += 1) {
      for (let c = box.c1; c <= capC; c += 1) {
        const e = store[refOf(r, c)];
        if (e) out.push([r, c, e.formula || "", e.value ?? "", e.style || null]);
      }
    }
    return out;
  }

  function computeSheetFingerprint(operations, deps) {
    const { document, sheetDataStore, getActiveSheetName, getSheetController } = deps;
    const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
    const sheetRules = host.__EASYOFFICE_SHEET_RULES__;
    const activeSheet = getActiveSheetName();
    const liveActiveStore = getSheetController?.()?.snapshotDomSheetCells?.() || sheetDataStore[activeSheet] || {};
    const domPending = detectUncommittedDomEdit(document, { ...sheetDataStore, [activeSheet]: liveActiveStore }, activeSheet);

    const perOp = (Array.isArray(operations) ? operations : []).map((op) => {
      const kind = String(op?.op || op?.type || op?.kind || "");
      const sheet = String(op?.sheet || activeSheet);
      const store = sheet === activeSheet ? liveActiveStore : (sheetDataStore[sheet] || {});
      if (kind === "sheet.write") {
        return (Array.isArray(op?.cells) ? op.cells : []).map((c) => {
          const e = store[String(c?.coord || "").toUpperCase()];
          return e ? [e.formula || "", e.value ?? "", e.style || null] : null;
        });
      }
      if (kind === "sheet.format") {
        return snapshotRangeState(store, op.range);
      }
      if (kind === "sheet.chart") {
        return [snapshotRangeState(store, op.dataRange), layout?.chartsOf?.(sheet)?.length || 0];
      }
      if (kind === "sheet.table") {
        return [snapshotRangeState(store, op.range), layout?.tablesOf?.(sheet) || []];
      }
      if (kind === "sheet.pivot") {
        const dst = String(op?.destSheet || sheet);
        return [snapshotRangeState(store, op.sourceRange), layout?.pivotsOf?.(dst) || []];
      }
      if (kind === "sheet.cond_format") {
        return [op.range, sheetRules?.get?.(sheet)?.cf || []];
      }
      if (kind === "sheet.sort_filter") {
        return [snapshotRangeState(store, op.range), layout?.filterOf?.(sheet) || null];
      }
      if (kind === "sheet.structure") {
        return [Object.keys(store).length, layout?.freezeOf?.(sheet) || null];
      }
      if (kind === "sheet.note") {
        const cell = String(op?.cell || "").toUpperCase();
        return layout?.noteAtRef?.(sheet, cell) || null;
      }
      return null;
    });

    return JSON.stringify({ domPending, ops: perOp });
  }

  function collectSheetFormulaErrors(sheetDataStore, targetSheet) {
    const sheets = targetSheet ? [targetSheet] : Object.keys(sheetDataStore || {});
    const found = [];
    for (const sName of sheets) {
      const store = sheetDataStore[sName] || {};
      for (const coord of Object.keys(store)) {
        const val = String(store[coord]?.value ?? "").trim().toUpperCase();
        if (FORMULA_ERROR_CODES.has(val)) {
          found.push({
            key: `${sName}!${coord}=${val}`,
            sheet: sName,
            coord,
            error: val,
            formula: String(store[coord]?.formula || ""),
          });
        }
      }
    }
    return found;
  }

  function readSheetRangeTool(args, deps) {
    const { sheetDataStore, getActiveSheetName, getSheetController } = deps;
    const { refOf } = host.__EASYOFFICE_SHEET_REFS__;
    const sc = getSheetController?.();
    const active = getActiveSheetName();
    if (sc && (!args.sheet || args.sheet === active)) {
      sheetDataStore[active] = sc.snapshotDomSheetCells();
    }
    const sheet = String(args.sheet || active);
    const store = sheetDataStore[sheet] || {};
    const box = parseRangeBox1(args.range);
    if (!box) return { sheet, range: String(args.range || ""), cells: [], error: "유효하지 않은 범위입니다." };

    const cells = [];
    let truncated = false;
    for (let r = box.r1; r <= box.r2; r += 1) {
      for (let c = box.c1; c <= box.c2; c += 1) {
        if (cells.length >= MAX_READ_TOOL_CELLS) {
          truncated = true;
          break;
        }
        const coord = refOf(r, c);
        const rec = store[coord];
        if (!rec || (!rec.value && !rec.formula)) continue;
        cells.push({
          coord,
          raw: rec.formula ? String(rec.formula) : String(rec.value ?? ""),
          value: String(rec.value ?? ""),
        });
      }
      if (truncated) break;
    }
    return { sheet, range: String(args.range || ""), truncated, cells };
  }

  function findSheetCellsTool(args, deps) {
    const { sheetDataStore, getActiveSheetName, getSheetController } = deps;
    const { parseRef, refOf } = host.__EASYOFFICE_SHEET_REFS__;
    const sc = getSheetController?.();
    const active = getActiveSheetName();
    if (sc) sheetDataStore[active] = sc.snapshotDomSheetCells();

    const q = String(args.query || "").trim().toLowerCase();
    if (!q) return { matches: [] };
    const sheetNames = args.sheet && Object.hasOwn(sheetDataStore, args.sheet)
      ? [args.sheet]
      : Object.keys(sheetDataStore);
    const matches = [];
    for (const sName of sheetNames) {
      const store = sheetDataStore[sName] || {};
      const coords = [];
      for (const k of Object.keys(store)) {
        const p = parseRef(k);
        if (p) coords.push({ row: p.row, col: p.col, key: refOf(p.row, p.col) });
      }
      coords.sort(compareCoordEntries);
      for (const item of coords) {
        const rec = store[item.key];
        const val = String(rec?.value ?? "");
        const raw = String(rec?.formula || val);
        if (val.toLowerCase().includes(q) || raw.toLowerCase().includes(q)) {
          matches.push({ sheet: sName, coord: item.key, raw, value: val });
          if (matches.length >= MAX_FIND_MATCHES) return { query: args.query, matches, truncated: true };
        }
      }
    }
    return { query: args.query, matches, truncated: false };
  }

  const EXTENDED_SHEET_KINDS = new Set([
    "sheet.table",
    "sheet.pivot",
    "sheet.cond_format",
    "sheet.sort_filter",
    "sheet.structure",
    "sheet.note",
  ]);

  function executeExtendedSheetOp(kind, op, deps) {
    const { sheetName, sheetDataStore, sheetRowCounts, locations } = deps;
    if (kind === "sheet.table") return executeSheetTable(op, sheetName, sheetDataStore, sheetRowCounts, locations);
    if (kind === "sheet.pivot") return executeSheetPivot(op, sheetName, sheetDataStore, sheetRowCounts, locations);
    if (kind === "sheet.cond_format") return executeSheetCondFormat(op, sheetName, locations);
    if (kind === "sheet.sort_filter") return executeSheetSortFilter(op, sheetName, sheetDataStore, locations);
    if (kind === "sheet.structure") return executeSheetStructure(op, sheetName, sheetDataStore, sheetRowCounts, locations);
    if (kind === "sheet.note") return executeSheetNote(op, sheetName, locations);
    throw new Error("지원하지 않는 시트 작업입니다: " + kind);
  }

  host.__EASYOFFICE_COPILOT_SHEET_OPS__ = Object.freeze({
    EXTENDED_SHEET_KINDS,
    FORMULA_ERROR_CODES,
    freezeSelection,
    parseRangeBox1,
    executeExtendedSheetOp,
    previewEntryForExtendedSheetOp,
    computeSheetFingerprint,
    collectSheetFormulaErrors,
    readSheetRangeTool,
    findSheetCellsTool,
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
