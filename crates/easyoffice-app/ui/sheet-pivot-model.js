(function (host) {
  "use strict";

  // Dynamic pivot table model, schema validation, single-pass O(R) grouping fallback, and tabular sheet
  // cell layout (PD-255). Shares the `"pivots"` JSON contract with the workbook format decoder/writer and
  // the `"pivot"` session action of `evaluate_sheet`.

  const rules = host.__EASYOFFICE_SHEET_RULES__;

  const MAX_PIVOTS_PER_SHEET = 32;
  const MAX_PIVOTS_TOTAL = 128;
  const MAX_PIVOT_FIELDS = 256;
  const MAX_PIVOT_AXIS_FIELDS = 16;
  const MAX_PIVOT_VALUE_FIELDS = 16;
  const MAX_PIVOT_FILTERS = 64;
  const MAX_PIVOT_CELLS = 250000;
  const MAX_SLICERS_PER_PIVOT = 32;
  const MAX_FILTER_VALUES = 2000;
  const MAX_NAME_LEN = 255;
  const MAX_POS = 65535;
  const DEFAULT_SLICER_W = 184;
  const DEFAULT_SLICER_H = 210;
  const TUPLE_SEP = "\u001f";

  const AGG_KINDS = Object.freeze(["sum", "average", "count", "min", "max"]);
  const AGG_SET = new Set(AGG_KINDS);
  const AGG_LABELS = Object.freeze({
    sum: "합계",
    average: "평균",
    count: "개수",
    min: "최소",
    max: "최대",
  });

  const numberText = (n) => (typeof n === "number" && Number.isFinite(n) ? String(Math.round(n * 1e10) / 1e10) : "");

  function aggLabelOf(agg) {
    return AGG_LABELS[agg] || AGG_LABELS.sum;
  }

  function defaultValueFieldName(agg, fieldName) {
    return aggLabelOf(agg) + ": " + (fieldName || "값");
  }

  function trimChars(raw, maxChars) {
    const s = String(raw ?? "").trim();
    const chars = Array.from(s);
    return chars.length > maxChars ? chars.slice(0, maxChars).join("") : s;
  }

  function parseSingleBox0(rawRange) {
    const text = String(rawRange || "").trim();
    if (!text) return null;
    const parts = text.includes(":") ? text : text + ":" + text;
    const boxes = rules.parseRanges(parts);
    return boxes && boxes.length === 1 ? boxes[0] : null;
  }

  function normalizeRangeA1(rawRange) {
    const box = parseSingleBox0(rawRange);
    return box ? rules.formatRanges([box]) : "";
  }

  function checkNonNegInt(val, label) {
    const n = Number(val);
    if (!Number.isInteger(n) || n < 0 || n >= 16384) {
      throw new Error("유효하지 않은 피벗 " + label + " 열 번호입니다.");
    }
    return n;
  }

  function normalizeFieldIndexList(raw, label) {
    if (raw === undefined || raw === null) return [];
    if (!Array.isArray(raw)) throw new Error("피벗 " + label + " 필드 목록은 배열이어야 합니다.");
    if (raw.length > MAX_PIVOT_AXIS_FIELDS) throw new Error("피벗 필드 개수가 허용 한도를 초과했습니다.");
    const seen = new Set();
    const out = [];
    for (const item of raw) {
      const idx = checkNonNegInt(item, label);
      if (!seen.has(idx)) {
        seen.add(idx);
        out.push(idx);
      }
    }
    return out;
  }

  function normalizeValueFields(raw, fieldNames, allowEmpty = false) {
    if (allowEmpty && (raw === undefined || raw === null)) return [];
    if (!Array.isArray(raw) || (!allowEmpty && raw.length === 0)) {
      throw new Error("피벗 값 필드는 최소 1개 이상이어야 합니다.");
    }
    if (raw.length === 0) return [];
    if (raw.length > MAX_PIVOT_VALUE_FIELDS) {
      throw new Error("피벗 값 필드 개수가 허용 한도를 초과했습니다.");
    }
    return raw.map((vf) => {
      if (!vf || typeof vf !== "object") throw new Error("피벗 값 필드 항목이 올바르지 않습니다.");
      const col = checkNonNegInt(vf.col, "값");
      const agg = String(vf.agg || "sum");
      if (!AGG_SET.has(agg)) {
        throw new Error("피벗 집계는 sum, average, count, min, max 중 하나입니다.");
      }
      const fallback = defaultValueFieldName(agg, fieldNames[col]);
      const name = trimChars(vf.name || fallback, MAX_NAME_LEN) || fallback;
      return { col, agg, name };
    });
  }

  function normalizeFilters(raw) {
    if (raw === undefined || raw === null) return [];
    if (!Array.isArray(raw)) throw new Error("피벗 필터 목록은 배열이어야 합니다.");
    if (raw.length > MAX_PIVOT_FILTERS) throw new Error("피벗 필터 개수가 허용 한도를 초과했습니다.");
    const byCol = new Map();
    for (const f of raw) {
      if (!f || typeof f !== "object") throw new Error("피벗 필터 항목이 올바르지 않습니다.");
      const col = checkNonNegInt(f.col, "필터");
      if (f.values !== undefined && !Array.isArray(f.values)) {
        throw new Error("피벗 필터 값 목록은 배열이어야 합니다.");
      }
      const vals = Array.isArray(f.values) ? f.values : [];
      if (vals.length > MAX_FILTER_VALUES) {
        throw new Error("피벗 필터 선택 항목 수가 허용 한도를 초과했습니다.");
      }
      byCol.set(col, {
        col,
        values: vals.map((v) => String(v ?? "")),
        blank: f.blank === true,
      });
    }
    return [...byCol.values()];
  }

  function normalizeSlicers(raw, fieldNames) {
    if (raw === undefined || raw === null) return [];
    if (!Array.isArray(raw)) throw new Error("피벗 슬라이서 목록은 배열이어야 합니다.");
    if (raw.length > MAX_SLICERS_PER_PIVOT) {
      throw new Error("피벗 슬라이서 개수가 허용 한도를 초과했습니다.");
    }
    return raw.map((s, idx) => {
      if (!s || typeof s !== "object") throw new Error("피벗 슬라이서 항목이 올바르지 않습니다.");
      const col = checkNonNegInt(s.col, "슬라이서");
      const caption = trimChars(s.caption || fieldNames[col] || "열 " + (col + 1), MAX_NAME_LEN);
      const leftNum = Number(s.left);
      const topNum = Number(s.top);
      const left = Math.max(0, Math.min(MAX_POS, Number.isFinite(leftNum) ? Math.round(leftNum) : 420 + (idx % 3) * 196));
      const top = Math.max(0, Math.min(MAX_POS, Number.isFinite(topNum) ? Math.round(topNum) : 28 + Math.floor(idx / 3) * 24));
      const width = Math.max(120, Math.min(1200, Math.round(Number(s.width) || DEFAULT_SLICER_W)));
      const height = Math.max(100, Math.min(1200, Math.round(Number(s.height) || DEFAULT_SLICER_H)));
      return { col, caption, left, top, width, height, multiSelect: s.multiSelect === true };
    });
  }

  function pivotsFromJson(rawList) {
    if (rawList === undefined || rawList === null) return [];
    if (!Array.isArray(rawList)) throw new Error("피벗 테이블 목록 형식이 올바르지 않습니다.");
    if (rawList.length > MAX_PIVOTS_PER_SHEET) {
      throw new Error("시트당 피벗 테이블 수 한도(" + MAX_PIVOTS_PER_SHEET + "개)를 초과했습니다.");
    }
    return rawList.map((item, idx) => {
      if (!item || typeof item !== "object") throw new Error("피벗 테이블 항목이 올바르지 않습니다.");
      const name = trimChars(item.name || "피벗 테이블" + (idx + 1), MAX_NAME_LEN) || "피벗 테이블" + (idx + 1);
      const cacheIdNum = Number(item.cacheId);
      const cacheId = Number.isInteger(cacheIdNum) && cacheIdNum >= 1 ? cacheIdNum : idx + 1;
      const sourceSheet = trimChars(item.sourceSheet, MAX_NAME_LEN);
      if (!sourceSheet) throw new Error("피벗 테이블 원본 시트 이름이 비어 있습니다.");
      const sourceRange = normalizeRangeA1(item.sourceRange);
      if (!sourceRange) throw new Error("피벗 테이블 원본 범위가 올바르지 않습니다.");
      const location = normalizeRangeA1(item.location);
      if (!location) throw new Error("피벗 테이블 배치 범위가 올바르지 않습니다.");
      const sourceTable = typeof item.sourceTable === "string" && item.sourceTable.trim()
        ? trimChars(item.sourceTable, MAX_NAME_LEN)
        : null;
      const hasHeader = item.hasHeader !== false;
      if (item.fieldNames !== undefined && !Array.isArray(item.fieldNames)) {
        throw new Error("피벗 필드 이름 목록은 배열이어야 합니다.");
      }
      const rawFields = Array.isArray(item.fieldNames) ? item.fieldNames : [];
      if (rawFields.length > MAX_PIVOT_FIELDS) {
        throw new Error("피벗 필드 개수가 허용 한도를 초과했습니다.");
      }
      const fieldNames = rawFields.map((fn, i) => trimChars(fn, MAX_NAME_LEN) || "열 " + (i + 1));
      const rowFields = normalizeFieldIndexList(item.rowFields, "행");
      const colFields = normalizeFieldIndexList(item.colFields, "열");
      const filterFields = normalizeFieldIndexList(item.filterFields, "보고서 필터");
      const valueFields = normalizeValueFields(item.valueFields, fieldNames, true);
      if (rowFields.length === 0 && valueFields.length === 0) {
        throw new Error("피벗 테이블은 행 필드 또는 값 필드를 하나 이상 가져야 합니다.");
      }
      const filters = normalizeFilters(item.filters);
      const slicers = normalizeSlicers(item.slicers, fieldNames);
      return {
        name,
        cacheId,
        sourceSheet,
        sourceRange,
        sourceTable,
        hasHeader,
        location,
        fieldNames,
        rowFields,
        colFields,
        filterFields,
        valueFields,
        filters,
        slicers,
      };
    });
  }

  function cacheSourceKey(sourceSheet, sourceRange, sourceTable) {
    const sheetKey = String(sourceSheet || "").trim().toUpperCase();
    const refKey = sourceTable
      ? "T:" + String(sourceTable).trim().toLowerCase()
      : "R:" + normalizeRangeA1(sourceRange);
    return sheetKey + "|" + refKey;
  }

  function resolveCacheId(allPivots, sourceSheet, sourceRange, sourceTable, excludePivot = null) {
    const targetKey = cacheSourceKey(sourceSheet, sourceRange, sourceTable);
    let maxId = 0;
    for (const p of allPivots || []) {
      if (!p || p === excludePivot) continue;
      const cid = Number(p.cacheId);
      if (Number.isInteger(cid) && cid > maxId) maxId = cid;
      if (
        Number.isInteger(cid) &&
        cid >= 1 &&
        cacheSourceKey(p.sourceSheet, p.sourceRange, p.sourceTable) === targetKey
      ) {
        return cid;
      }
    }
    return maxId + 1;
  }

  function uniquePivotName(allPivots) {
    const taken = new Set((allPivots || []).map((p) => String(p?.name || "").toLowerCase()));
    let n = 1;
    while (taken.has(("피벗 테이블" + n).toLowerCase())) n += 1;
    return "피벗 테이블" + n;
  }

  // ---- Request builder (`sessionAction: "pivot"`) ------------------------------------------------
  function buildPivotRequest(wbPayload, sourceSheet, sourceRange, spec) {
    const rowFields = Array.isArray(spec.rowFields) ? spec.rowFields : [0];
    const colFields = spec.colFields || [];
    const filterFields = spec.filterFields || [];
    const valueFields = Array.isArray(spec.valueFields) ? spec.valueFields : [{ col: 0, agg: "sum", name: "합계: 값" }];
    const filters = spec.filters || [];
    const base = {
      sessionAction: "pivot",
      sheet: sourceSheet,
      ...wbPayload,
      range: sourceRange,
      header: spec.hasHeader !== false,
      row: rowFields[0] ?? 0,
      col: colFields.length > 0 ? colFields[0] : null,
      value: valueFields[0]?.col ?? 0,
      agg: valueFields[0]?.agg || "sum",
    };
    const isSimple =
      rowFields.length === 1 &&
      colFields.length <= 1 &&
      filterFields.length === 0 &&
      valueFields.length === 1 &&
      filters.length === 0;
    if (isSimple) return base;
    return {
      ...base,
      rowFields: [...rowFields],
      colFields: [...colFields],
      filterFields: [...filterFields],
      valueFields: valueFields.map((vf) => ({ col: vf.col, agg: vf.agg, name: vf.name })),
      filters: filters.map((f) => ({ col: f.col, values: [...(f.values || [])], blank: Boolean(f.blank) })),
    };
  }

  // ---- Single-pass O(R) pivot engine (used by offline-rpc and test verification) ------------------
  function createAcc() {
    return { count: 0, sum: 0, min: Infinity, max: -Infinity };
  }

  function addAcc(acc, val) {
    acc.count += 1;
    acc.sum += val;
    if (val < acc.min) acc.min = val;
    if (val > acc.max) acc.max = val;
  }

  function evalAcc(acc, agg) {
    if (!acc || acc.count === 0) return 0;
    if (agg === "sum") return acc.sum;
    if (agg === "average") return acc.sum / acc.count;
    if (agg === "count") return acc.count;
    if (agg === "min") return acc.min;
    if (agg === "max") return acc.max;
    return 0;
  }

  function normalizePivotKeyPair(cell) {
    let disp = "";
    if (cell.kind === "number") {
      disp = numberText(cell.num);
    } else if (cell.kind === "text") {
      const raw = String(cell.text ?? "");
      const trimmed = raw.trim();
      const strictNum = trimmed !== "" && /^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/.test(trimmed) ? Number(trimmed) : NaN;
      disp = Number.isFinite(strictNum) ? numberText(strictNum) : raw;
    } else {
      disp = String(cell.display ?? "");
    }
    return { disp, canon: disp.toLowerCase() };
  }

  function numericContribution(cell, agg) {
    if (cell.kind === "number" && Number.isFinite(cell.num)) return cell.num;
    if (agg === "count" && cell.kind !== "empty" && String(cell.display ?? "") !== "") return 1;
    return null;
  }

  function parseRequestSpec(doc, width) {
    const rowFields = Array.isArray(doc.rowFields)
      ? normalizeFieldIndexList(doc.rowFields, "행")
      : doc.row !== undefined && doc.row !== null
        ? [checkNonNegInt(doc.row, "행")]
        : [];

    const colFields = Array.isArray(doc.colFields)
      ? normalizeFieldIndexList(doc.colFields, "열")
      : doc.col !== undefined && doc.col !== null
        ? [checkNonNegInt(doc.col, "열")]
        : [];

    const filterFields = Array.isArray(doc.filterFields) ? normalizeFieldIndexList(doc.filterFields, "필터") : [];

    let valueFields;
    if (Array.isArray(doc.valueFields)) {
      valueFields = normalizeValueFields(doc.valueFields, [], true);
    } else {
      if (doc.value === undefined || doc.value === null) {
        throw new Error("InvalidArgument: 피벗 값 열이 필요합니다.");
      }
      const vCol = checkNonNegInt(doc.value, "값");
      const agg = String(doc.agg || "sum");
      if (!AGG_SET.has(agg)) {
        throw new Error("InvalidArgument: 피벗 집계는 sum, average, count, min, max 중 하나입니다.");
      }
      valueFields = [{ col: vCol, agg, name: defaultValueFieldName(agg, "") }];
    }

    if (rowFields.length === 0 && valueFields.length === 0) {
      throw new Error("InvalidArgument: 피벗 테이블은 행 필드 또는 값 필드를 하나 이상 가져야 합니다.");
    }

    const filters = normalizeFilters(doc.filters);
    const allCols = [...rowFields, ...colFields, ...filterFields, ...valueFields.map((v) => v.col), ...filters.map((f) => f.col)];
    if (allCols.some((c) => c >= width)) {
      throw new Error("InvalidArgument: 피벗 열이 선택한 범위를 벗어났습니다.");
    }
    return { header: doc.header !== false, rowFields, colFields, filterFields, valueFields, filters };
  }

  // `readCell(row0, col0)` returns `{ kind: "empty"|"number"|"text"|"bool"|"error", num?, text?, display }`.
  function computePivotWithReader(doc, readCell) {
    const box = parseSingleBox0(doc.range);
    if (!box) throw new Error("InvalidArgument: 유효하지 않은 피벗 범위입니다.");
    const height = box.row1 - box.row0 + 1;
    const width = box.col1 - box.col0 + 1;
    const spec = parseRequestSpec(doc, width);
    const startOff = spec.header ? 1 : 0;
    const vCount = spec.valueFields.length;

    const compiledFilters = spec.filters.map((f) => ({
      col: f.col,
      set: new Set((f.values || []).filter((v) => v !== "").map((v) => String(v).toLowerCase())),
      blank: Boolean(f.blank),
    }));

    const rowIndexMap = new Map();
    const rowKeys = [];
    const rowTuples = [];
    const rowAccs = [];

    const colIndexMap = new Map();
    const columnKeys = [];
    const columnTuples = [];
    const colAccs = [];

    if (spec.colFields.length === 0) {
      colIndexMap.set("total", 0);
      columnKeys.push("Total");
      colAccs.push(Array.from({ length: vCount }, createAcc));
    }

    const buckets = new Map();
    const grandAccs = Array.from({ length: vCount }, createAcc);

    for (let rOff = startOff; rOff < height; rOff += 1) {
      const r0 = box.row0 + rOff;
      let passFilter = true;
      for (let fi = 0; fi < compiledFilters.length; fi += 1) {
        const cf = compiledFilters[fi];
        const { disp, canon } = normalizePivotKeyPair(readCell(r0, box.col0 + cf.col));
        if (!(disp === "" ? cf.blank : cf.set.has(canon))) {
          passFilter = false;
          break;
        }
      }
      if (!passFilter) continue;

      const rTuple = [];
      const rCanons = [];
      let anyNonEmptyRow = false;
      for (let i = 0; i < spec.rowFields.length; i += 1) {
        const pair = normalizePivotKeyPair(readCell(r0, box.col0 + spec.rowFields[i]));
        if (pair.disp !== "") anyNonEmptyRow = true;
        rTuple.push(pair.disp);
        rCanons.push(pair.canon);
      }
      if (spec.rowFields.length === 0) {
        for (let j = 0; j < spec.colFields.length && !anyNonEmptyRow; j += 1) {
          if (normalizePivotKeyPair(readCell(r0, box.col0 + spec.colFields[j])).disp !== "") anyNonEmptyRow = true;
        }
        for (let v = 0; v < vCount && !anyNonEmptyRow; v += 1) {
          const cell = readCell(r0, box.col0 + spec.valueFields[v].col);
          if (cell.kind !== "empty" && String(cell.display ?? "") !== "") anyNonEmptyRow = true;
        }
      }
      if (!anyNonEmptyRow) continue;
      if (spec.rowFields.length > 1) {
        for (let i = 0; i < rTuple.length; i += 1) {
          if (rTuple[i] === "") {
            rTuple[i] = "(빈 셀)";
            rCanons[i] = "(빈 셀)";
          }
        }
      }

      const rCanonKey = rCanons.join(TUPLE_SEP);
      let rIdx = rowIndexMap.get(rCanonKey);
      if (rIdx === undefined) {
        rIdx = rowKeys.length;
        rowIndexMap.set(rCanonKey, rIdx);
        rowKeys.push(rTuple.length === 0 ? "Total" : rTuple.length === 1 ? rTuple[0] : rTuple.join(" / "));
        rowTuples.push(rTuple);
        rowAccs.push(Array.from({ length: vCount }, createAcc));
        if (rowKeys.length * Math.max(1, columnKeys.length) * Math.max(1, vCount) > MAX_PIVOT_CELLS) {
          throw new Error("ResourceBudgetExceeded: 피벗 결과가 " + MAX_PIVOT_CELLS + "칸을 넘습니다. 행·열 항목을 줄여 주세요.");
        }
      }

      let cIdx = 0;
      if (spec.colFields.length > 0) {
        const cTuple = [];
        const cCanons = [];
        for (let j = 0; j < spec.colFields.length; j += 1) {
          const pair = normalizePivotKeyPair(readCell(r0, box.col0 + spec.colFields[j]));
          cTuple.push(pair.disp === "" ? "(Blank)" : pair.disp);
          cCanons.push(pair.disp === "" ? "(blank)" : pair.canon);
        }
        const cCanonKey = cCanons.join(TUPLE_SEP);
        const foundC = colIndexMap.get(cCanonKey);
        if (foundC !== undefined) {
          cIdx = foundC;
        } else {
          cIdx = columnKeys.length;
          colIndexMap.set(cCanonKey, cIdx);
          columnKeys.push(cTuple.length === 1 ? cTuple[0] : cTuple.join(" / "));
          columnTuples.push(cTuple);
          colAccs.push(Array.from({ length: vCount }, createAcc));
          if (rowKeys.length * columnKeys.length * Math.max(1, vCount) > MAX_PIVOT_CELLS) {
            throw new Error("ResourceBudgetExceeded: 피벗 결과가 " + MAX_PIVOT_CELLS + "칸을 넘습니다. 행·열 항목을 줄여 주세요.");
          }
        }
      }

      if (vCount > 0) {
        const bKey = rIdx * 16777216 + cIdx;
        let cellAccList = buckets.get(bKey);
        for (let v = 0; v < vCount; v += 1) {
          const vf = spec.valueFields[v];
          const num = numericContribution(readCell(r0, box.col0 + vf.col), vf.agg);
          if (num !== null) {
            if (!cellAccList) {
              cellAccList = Array.from({ length: vCount }, createAcc);
              buckets.set(bKey, cellAccList);
            }
            addAcc(cellAccList[v], num);
            addAcc(rowAccs[rIdx][v], num);
            addAcc(colAccs[cIdx][v], num);
            addAcc(grandAccs[v], num);
          }
        }
      }
    }

    if (spec.rowFields.length === 0 && rowKeys.length === 0) {
      rowKeys.push("Total");
      rowTuples.push([]);
      rowAccs.push(Array.from({ length: vCount }, createAcc));
    }
    if (columnKeys.length === 0) {
      columnKeys.push("Total");
      colAccs.push(Array.from({ length: vCount }, createAcc));
    }

    const valuesMatrix = Array.from({ length: rowKeys.length }, (_, r) =>
      Array.from({ length: columnKeys.length }, (__, c) => {
        const list = buckets.get(r * 16777216 + c);
        return spec.valueFields.map((vf, v) => (list ? evalAcc(list[v], vf.agg) : 0));
      })
    );
    const valueRowTotals = rowAccs.map((accs) => spec.valueFields.map((vf, v) => evalAcc(accs[v], vf.agg)));
    const valueColumnTotals = colAccs.map((accs) => spec.valueFields.map((vf, v) => evalAcc(accs[v], vf.agg)));
    const valueGrandTotals = spec.valueFields.map((vf, v) => evalAcc(grandAccs[v], vf.agg));

    const values = spec.valueFields.map((vf, v) => ({
      col: vf.col,
      agg: vf.agg,
      matrix: valuesMatrix.map((row) => row.map((cell) => cell[v])),
      rowTotals: valueRowTotals.map((rt) => rt[v]),
      columnTotals: valueColumnTotals.map((ct) => ct[v]),
      grandTotal: valueGrandTotals[v],
    }));

    const first = values[0] || {
      matrix: valuesMatrix.map(() => columnKeys.map(() => 0)),
      rowTotals: rowKeys.map(() => 0),
      columnTotals: columnKeys.map(() => 0),
      grandTotal: 0,
    };
    return {
      rowKeys,
      rowTuples,
      columnKeys,
      columnTuples,
      colTuples: columnTuples,
      matrix: first.matrix,
      rowTotals: first.rowTotals,
      columnTotals: first.columnTotals,
      grandTotal: first.grandTotal,
      valuesMatrix,
      valueRowTotals,
      valueColumnTotals,
      valueGrandTotals,
      values,
      grandTotals: valueGrandTotals,
    };
  }

  // ---- Result normalization & Sheet cell rendering ------------------------------------------------
  function splitFallbackTuple(keyText, arity) {
    if (arity <= 0) return [];
    const s = String(keyText ?? "");
    if (arity === 1) return [s];
    const parts = s.includes(TUPLE_SEP) ? s.split(TUPLE_SEP) : s.split(" / ");
    const out = parts.slice(0, arity);
    while (out.length < arity) out.push("");
    return out;
  }

  function normalizePivotResult(result, spec) {
    const rowArity = Array.isArray(spec.rowFields) ? spec.rowFields.length : 1;
    const colArity = spec.colFields?.length || 0;
    const vCount = Array.isArray(spec.valueFields) ? spec.valueFields.length : 1;
    const rowKeys = Array.isArray(result.rowKeys) ? result.rowKeys.map(String) : [];
    const columnKeys = Array.isArray(result.columnKeys) ? result.columnKeys.map(String) : [];
    const numCols = Math.max(1, columnKeys.length);

    const rowTuples = Array.isArray(result.rowTuples) && result.rowTuples.length === rowKeys.length
      ? result.rowTuples.map((t) => (Array.isArray(t) ? t.map(String) : splitFallbackTuple(t, rowArity)))
      : rowKeys.map((k) => splitFallbackTuple(k, rowArity));

    const rawColTuples = result.columnTuples || result.colTuples;
    const columnTuples = colArity === 0
      ? []
      : Array.isArray(rawColTuples) && rawColTuples.length === columnKeys.length
        ? rawColTuples.map((t) => (Array.isArray(t) ? t.map(String) : splitFallbackTuple(t, colArity)))
        : columnKeys.map((k) => splitFallbackTuple(k, colArity));

    const rawValues = result.values || result.valueResults;
    let values;
    if (vCount === 0) {
      values = [];
    } else if (Array.isArray(result.valuesMatrix)) {
      values = Array.from({ length: vCount }, (_, v) => ({
        matrix: result.valuesMatrix.map((row) => (Array.isArray(row) ? row.map((cell) => cell?.[v] ?? 0) : [])),
        rowTotals: (result.valueRowTotals || []).map((rt) => (Array.isArray(rt) ? rt[v] ?? 0 : 0)),
        columnTotals: (result.valueColumnTotals || []).map((ct) => (Array.isArray(ct) ? ct[v] ?? 0 : 0)),
        grandTotal: Array.isArray(result.valueGrandTotals) ? result.valueGrandTotals[v] ?? 0 : result.grandTotal ?? 0,
      }));
    } else if (Array.isArray(rawValues) && rawValues.length >= vCount) {
      values = Array.from({ length: vCount }, (_, v) => ({
        matrix: rawValues[v]?.matrix || [],
        rowTotals: rawValues[v]?.rowTotals || [],
        columnTotals: rawValues[v]?.columnTotals || [],
        grandTotal: rawValues[v]?.grandTotal ?? 0,
      }));
    } else if (Array.isArray(result.matrix?.[0]?.[0])) {
      values = Array.from({ length: vCount }, (_, v) => ({
        matrix: result.matrix.map((row) => row.map((cell) => cell?.[v] ?? 0)),
        rowTotals: (result.rowTotals || []).map((rt) => (Array.isArray(rt) ? rt[v] ?? 0 : rt ?? 0)),
        columnTotals: (result.columnTotals || []).map((ct) => (Array.isArray(ct) ? ct[v] ?? 0 : ct ?? 0)),
        grandTotal: Array.isArray(result.grandTotals) ? result.grandTotals[v] ?? 0 : result.grandTotal ?? 0,
      }));
    } else if (vCount > 1 && Array.isArray(result.matrix?.[0]) && result.matrix[0].length === numCols * vCount) {
      values = Array.from({ length: vCount }, (_, v) => ({
        matrix: result.matrix.map((row) => Array.from({ length: numCols }, (__, c) => row[c * vCount + v] ?? 0)),
        rowTotals: (result.rowTotals || []).map((rt, r) =>
          Array.isArray(rt) ? rt[v] ?? 0 : result.rowTotals[r * vCount + v] ?? 0
        ),
        columnTotals: Array.from({ length: numCols }, (_, c) => result.columnTotals?.[c * vCount + v] ?? 0),
        grandTotal: Array.isArray(result.grandTotals) ? result.grandTotals[v] ?? 0 : result.grandTotal ?? 0,
      }));
    } else {
      const single = {
        matrix: Array.isArray(result.matrix) ? result.matrix : [],
        rowTotals: Array.isArray(result.rowTotals) ? result.rowTotals : [],
        columnTotals: Array.isArray(result.columnTotals) ? result.columnTotals : [],
        grandTotal: result.grandTotal ?? 0,
      };
      values = Array.from({ length: vCount }, (_, v) => (v === 0 ? single : {
        matrix: single.matrix.map((r) => r.map(() => 0)),
        rowTotals: single.rowTotals.map(() => 0),
        columnTotals: single.columnTotals.map(() => 0),
        grandTotal: 0,
      }));
    }

    return { rowKeys, rowTuples, columnKeys, columnTuples, values };
  }

  function formatFilterSummary(cond) {
    if (!cond) return "(모두)";
    const parts = [...(cond.values || []), ...(cond.blank ? ["(빈 셀)"] : [])];
    return parts.length === 0 ? "(없음)" : parts.join(", ");
  }

  function buildPivotSheetCells(result, spec, anchorRow0 = 0, anchorCol0 = 0) {
    const cells = {};
    const put = (row0, col0, text) => {
      const str = String(text ?? "");
      if (str !== "") cells[rules.refOf(row0, col0)] = { value: str, formula: "" };
    };

    const fieldNames = spec.fieldNames || [];
    const rowFields = Array.isArray(spec.rowFields) ? spec.rowFields : [0];
    const colFields = spec.colFields || [];
    const filterFields = spec.filterFields || [];
    const valueFields = Array.isArray(spec.valueFields)
      ? spec.valueFields
      : [{ col: 0, agg: "sum", name: defaultValueFieldName("sum", fieldNames[0]) }];
    const filters = spec.filters || [];

    const norm = normalizePivotResult(result, { rowFields, colFields, valueFields });
    const R = rowFields.length;
    const L = Math.max(1, R);
    const V = valueFields.length;
    const F = filterFields.length;
    const twoWay = colFields.length > 0 || (V === 1 && norm.columnKeys.length > 1);
    const colLabels = twoWay
      ? (norm.columnKeys.length > 0 ? norm.columnKeys : ["(Blank)"])
      : ["Total"];
    const C = colLabels.length;

    for (let fi = 0; fi < F; fi += 1) {
      const fCol = filterFields[fi];
      const cond = filters.find((c) => c.col === fCol);
      put(anchorRow0 + fi, anchorCol0, fieldNames[fCol] || "필터 " + (fi + 1));
      put(anchorRow0 + fi, anchorCol0 + 1, formatFilterSummary(cond));
    }

    const bodyStartRow = anchorRow0 + (F > 0 ? F + 1 : 0);
    const multiHeader = twoWay && V > 1;
    const rowHeaderRow = multiHeader ? bodyStartRow + 1 : bodyStartRow;
    const dataStartRow = rowHeaderRow + 1;

    if (R === 0) {
      put(rowHeaderRow, anchorCol0, "값");
    } else {
      for (let rf = 0; rf < R; rf += 1) {
        const fCol = rowFields[rf];
        put(rowHeaderRow, anchorCol0 + rf, fieldNames[fCol] || "행");
      }
    }

    let totalCols;
    if (V === 0) {
      if (twoWay) {
        for (let c = 0; c < C; c += 1) {
          const label = norm.columnTuples[c]?.length > 1 ? norm.columnTuples[c].join(" / ") : colLabels[c];
          put(rowHeaderRow, anchorCol0 + L + c, label);
        }
        totalCols = L + C;
      } else {
        totalCols = L;
      }
    } else if (!twoWay && V === 1) {
      put(rowHeaderRow, anchorCol0 + L, valueFields[0].name);
      totalCols = L + 1;
    } else if (!twoWay) {
      for (let v = 0; v < V; v += 1) {
        put(rowHeaderRow, anchorCol0 + L + v, valueFields[v].name);
      }
      totalCols = L + V;
    } else if (V === 1) {
      for (let c = 0; c < C; c += 1) {
        const label = norm.columnTuples[c]?.length > 1 ? norm.columnTuples[c].join(" / ") : colLabels[c];
        put(rowHeaderRow, anchorCol0 + L + c, label);
      }
      put(rowHeaderRow, anchorCol0 + L + C, "합계");
      totalCols = L + C + 1;
    } else {
      for (let c = 0; c < C; c += 1) {
        const label = norm.columnTuples[c]?.length > 1 ? norm.columnTuples[c].join(" / ") : colLabels[c];
        put(bodyStartRow, anchorCol0 + L + c * V, label);
        for (let v = 0; v < V; v += 1) {
          put(rowHeaderRow, anchorCol0 + L + c * V + v, valueFields[v].name);
        }
      }
      put(bodyStartRow, anchorCol0 + L + C * V, "합계");
      for (let v = 0; v < V; v += 1) {
        put(rowHeaderRow, anchorCol0 + L + C * V + v, valueFields[v].name);
      }
      totalCols = L + (C + 1) * V;
    }

    const numRows = R === 0 ? 0 : norm.rowTuples.length;
    for (let r = 0; r < numRows; r += 1) {
      const row0 = dataStartRow + r;
      const tuple = norm.rowTuples[r];
      for (let rf = 0; rf < R; rf += 1) {
        put(row0, anchorCol0 + rf, tuple[rf] ?? "");
      }
      if (V === 0) continue;
      if (!twoWay) {
        for (let v = 0; v < V; v += 1) {
          put(row0, anchorCol0 + L + v, numberText(norm.values[v].matrix[r]?.[0]));
        }
      } else if (V === 1) {
        const line = norm.values[0].matrix[r] || [];
        for (let c = 0; c < C; c += 1) {
          put(row0, anchorCol0 + L + c, numberText(line[c]));
        }
        put(row0, anchorCol0 + L + C, numberText(norm.values[0].rowTotals[r]));
      } else {
        for (let c = 0; c < C; c += 1) {
          for (let v = 0; v < V; v += 1) {
            put(row0, anchorCol0 + L + c * V + v, numberText(norm.values[v].matrix[r]?.[c]));
          }
        }
        for (let v = 0; v < V; v += 1) {
          put(row0, anchorCol0 + L + C * V + v, numberText(norm.values[v].rowTotals[r]));
        }
      }
    }

    const totalRow = V === 0 ? Math.max(rowHeaderRow, dataStartRow + numRows - 1) : dataStartRow + numRows;
    if (V > 0) {
      put(totalRow, anchorCol0, "합계");
      if (!twoWay) {
        for (let v = 0; v < V; v += 1) {
          put(totalRow, anchorCol0 + L + v, numberText(norm.values[v].grandTotal));
        }
      } else if (V === 1) {
        for (let c = 0; c < C; c += 1) {
          put(totalRow, anchorCol0 + L + c, numberText(norm.values[0].columnTotals[c] ?? 0));
        }
        put(totalRow, anchorCol0 + L + C, numberText(norm.values[0].grandTotal));
      } else {
        for (let c = 0; c < C; c += 1) {
          for (let v = 0; v < V; v += 1) {
            put(totalRow, anchorCol0 + L + c * V + v, numberText(norm.values[v].columnTotals[c] ?? 0));
          }
        }
        for (let v = 0; v < V; v += 1) {
          put(totalRow, anchorCol0 + L + C * V + v, numberText(norm.values[v].grandTotal));
        }
      }
    }

    const lastCol0 = anchorCol0 + Math.max(1, totalCols) - 1;
    const location = rules.refOf(anchorRow0, anchorCol0) + ":" + rules.refOf(totalRow, lastCol0);
    return { cells, rows: totalRow + 1, location };
  }

  // Legacy 1-row, 1-value helper preserved for existing callers and unit tests.
  function pivotCells(result, labels) {
    const hasCol = Boolean(labels?.col);
    const built = buildPivotSheetCells(
      result,
      {
        fieldNames: [labels?.row || "행", labels?.col || "열"],
        rowFields: [0],
        colFields: hasCol ? [1] : [],
        filterFields: [],
        valueFields: [{ col: 0, agg: "sum", name: labels?.value || "값" }],
        filters: [],
      },
      0,
      0
    );
    return { cells: built.cells, rows: built.rows };
  }

  host.__EASYOFFICE_SHEET_PIVOT_MODEL__ = {
    MAX_PIVOTS_PER_SHEET,
    MAX_PIVOTS_TOTAL,
    MAX_PIVOT_FIELDS,
    MAX_PIVOT_CELLS,
    MAX_SLICERS_PER_PIVOT,
    AGG_KINDS,
    AGG_LABELS,
    aggLabelOf,
    defaultValueFieldName,
    numberText,
    parseSingleBox0,
    normalizeRangeA1,
    pivotsFromJson,
    resolveCacheId,
    uniquePivotName,
    buildPivotRequest,
    computePivotWithReader,
    normalizePivotResult,
    buildPivotSheetCells,
    pivotCells,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
