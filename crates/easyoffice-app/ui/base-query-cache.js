// 참조 구현 (reference implementation): @tanstack/table-core (`TanStack/table`, MIT, Copyright (c) 2016 Tanner Linsley,
// commit 1cfb2e57cbfecf25e3d4ee1bda32fbd65e4e0327; see `THIRD_PARTY_NOTICES.md`) 단계별 행 파이프라인
// (`Core -> Filtered -> Sorted -> Grouped`) 구조 참조(이식 코드 0줄).
// 이 파일은 EasyOffice 자체 코드다: BASE 데이터시트의 `queryTableView` 증분 갱신 캐시와 관계 의존 추적을 제공한다 (PD-317).

(function (host) {
  "use strict";

  const MAX_REL_STEPS = 32, MAX_DIRTY_BATCH = 64;
  const cmpCode = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
  const isEmpty = (v) => v === null || v === undefined || v === "" || (Array.isArray(v) && v.length === 0);
  const fieldsSig = (fields) => fields.map((f) => `${f.id}\0${f.name}\0${f.type}\0${f.formula || ""}\0${f.relationTableId || ""}\0${(f.options || []).join("\x01")}`).join("\x02");
  const viewSig = (t) => JSON.stringify([t.filters || [], t.filterConjunction === "or" ? "or" : "and", t.sorts || [], t.groupByFieldId || ""]);

  function analyzeRelations(table, stateIndex) {
    const visited = new Set(), queue = [table.id], extDeps = new Set();
    let selfCyclic = false;
    for (let q = 0; q < queue.length && q <= MAX_REL_STEPS; q += 1) {
      const t = stateIndex.tablesById.get(queue[q]);
      if (!t) continue;
      for (const fld of t.fields || []) {
        if (fld.type !== "relation" || !fld.relationTableId) continue;
        if (fld.relationTableId === table.id) selfCyclic = true; else extDeps.add(fld.relationTableId);
        if (!visited.has(fld.relationTableId)) { visited.add(fld.relationTableId); queue.push(fld.relationTableId); }
      }
    }
    return { selfCyclic, externalDeps: Array.from(extDeps) };
  }

  function binarySearchExact(arr, target, cmp) {
    let lo = 0, hi = arr.length - 1;
    while (lo <= hi) { const mid = (lo + hi) >>> 1, d = cmp(arr[mid], target); if (d === 0) return mid; if (d < 0) lo = mid + 1; else hi = mid - 1; }
    return -1;
  }

  function lowerBound(arr, target, cmp) {
    let lo = 0, hi = arr.length;
    while (lo < hi) { const mid = (lo + hi) >>> 1; if (cmp(arr[mid], target) < 0) lo = mid + 1; else hi = mid; }
    return lo;
  }

  function replaceSorted(arr, prevRow, nextRow, cmp) {
    const pos = binarySearchExact(arr, prevRow, cmp);
    if (pos < 0) return -1;
    if ((pos === 0 || cmp(arr[pos - 1], nextRow) < 0) && (pos === arr.length - 1 || cmp(nextRow, arr[pos + 1]) < 0)) { arr[pos] = nextRow; return pos; }
    arr.splice(pos, 1); const ins = lowerBound(arr, nextRow, cmp); arr.splice(ins, 0, nextRow); return ins;
  }

  function compilePlan(table, stateIndex, M) {
    const meta = stateIndex.fieldsByTableId.get(table.id), plainFields = [], formulaFields = [], sumIds = [];
    for (const fld of table.fields) {
      if (fld.type === "formula") { formulaFields.push(fld); sumIds.push(fld.id); }
      else { plainFields.push(fld); if (fld.type === "number") sumIds.push(fld.id); }
    }
    const activeFilters = (table.filters || []).map((rule) => ({ rule, field: meta.byId.get(rule?.fieldId) })).filter((x) => x.field);
    const activeSorts = (table.sorts || []).map((s) => ({ dir: s?.dir === "desc" ? -1 : 1, field: meta.byId.get(s?.fieldId) })).filter((x) => x.field);
    const groupField = table.groupByFieldId ? meta.byId.get(table.groupByFieldId) : null;
    const cmpRows = !activeSorts.length ? (a, b) => a.origIdx - b.origIdx : (a, b) => {
      for (const { field, dir } of activeSorts) {
        const ae = isEmpty(a.values[field.id]), be = isEmpty(b.values[field.id]);
        if (ae && be) continue;
        if (ae || be) return ae ? 1 : -1;
        const c = M.compareFieldValues(a.values[field.id], b.values[field.id], a.displays[field.id], b.displays[field.id], field);
        if (c !== 0) return c * dir;
      }
      return a.origIdx - b.origIdx;
    };
    return { meta, plainFields, formulaFields, sumIds, activeFilters, useOr: table.filterConjunction === "or", activeSorts, groupField, cmpRows };
  }

  function groupKeyLabel(row, gf) {
    const rawVal = row.values[gf.id], disp = row.displays[gf.id];
    if (gf.type === "checkbox") return { key: rawVal ? "true" : "false", label: rawVal ? "선택됨" : "선택 안 됨" };
    return isEmpty(rawVal) ? { key: "__empty__", label: "미분류" } : { key: disp, label: disp };
  }

  function createBaseQueryCache(M) {
    let boundState = null, stateIndex = null;
    const entries = new Map(), tableRev = new Map();
    const revOf = (id) => tableRev.get(id) || 0;
    const bumpRev = (id) => { tableRev.set(id, revOf(id) + 1); };
    const ensureIndex = (state) => {
      if (state !== boundState || !stateIndex) { boundState = state; entries.clear(); stateIndex = M.buildStateIndex(state); }
      return stateIndex;
    };

    function evalRow(state, table, rec, origIdx, plan) {
      const values = Object.create(null), displays = Object.create(null);
      for (const fld of plan.plainFields) {
        const val = rec.values?.[fld.id];
        values[fld.id] = val === undefined ? null : val;
        displays[fld.id] = M.formatCellDisplay(fld, val, state, stateIndex);
      }
      if (plan.formulaFields.length) {
        const evalCache = new Map(), visiting = new Set();
        for (const ff of plan.formulaFields) {
          let val = evalCache.has(ff.id) ? evalCache.get(ff.id) : undefined;
          if (val === undefined) {
            visiting.add(ff.id);
            val = M.evaluateFormula(ff.formula, rec, table, state, { index: stateIndex, fieldsMeta: plan.meta, visiting, evalCache, fieldDepth: 0 });
            visiting.delete(ff.id); evalCache.set(ff.id, val);
          }
          values[ff.id] = val; displays[ff.id] = M.formatCellDisplay(ff, val, state, stateIndex);
        }
      }
      return { record: rec, values, displays, origIdx };
    }

    function rowPasses(row, plan) {
      if (!plan.activeFilters.length) return true;
      for (const { rule, field } of plan.activeFilters) { const hit = M.matchesFilterRule(rule, row, field); if (plan.useOr && hit) return true; if (!plan.useOr && !hit) return false; }
      return !plan.useOr;
    }

    function buildGroups(entry, filteredRows, collapsedSet) {
      const { plan } = entry, gf = plan.groupField;
      const sums = {}, counts = {}, floats = {};
      for (const id of plan.sumIds) { sums[id] = 0; counts[id] = 0; floats[id] = false; }
      const groupMap = gf ? new Map() : null;
      for (const row of filteredRows) {
        let bucket = null;
        if (gf) {
          const { key, label } = groupKeyLabel(row, gf);
          bucket = groupMap.get(key);
          if (!bucket) { bucket = { key, label, records: [], count: 0, numericSums: Object.fromEntries(plan.sumIds.map((id) => [id, 0])), collapsed: collapsedSet.has(key) }; groupMap.set(key, bucket); }
          bucket.records.push(row); bucket.count += 1;
        }
        for (const id of plan.sumIds) {
          const v = row.values[id];
          if (typeof v !== "number") continue;
          counts[id] += 1;
          if (Number.isFinite(v)) { sums[id] += v; if (bucket) bucket.numericSums[id] += v; if (!Number.isSafeInteger(v)) floats[id] = true; }
        }
      }
      entry.numericCounts = counts; entry.numericSums = sums; entry.hasFloat = floats; entry.groupByKey = groupMap;
      if (!gf) { entry.groups = [{ key: "__all__", label: "", records: filteredRows, count: filteredRows.length, numericSums: sums, collapsed: false }]; return; }
      const groups = Array.from(groupMap.values());
      if (gf.type === "select" && Array.isArray(gf.options)) {
        const orderMap = new Map(gf.options.map((opt, idx) => [opt, idx]));
        groups.sort((a, b) => (a.key === "__empty__" ? 1e9 : (orderMap.get(a.key) ?? 1e8)) - (b.key === "__empty__" ? 1e9 : (orderMap.get(b.key) ?? 1e8)) || cmpCode(a.label, b.label));
      }
      entry.groups = groups;
    }

    function projectView(entry, table) {
      const { plan, allRows, passFlags } = entry, filteredRows = [];
      for (let r = 0; r < allRows.length; r += 1) {
        const pass = rowPasses(allRows[r], plan);
        passFlags[r] = pass ? 1 : 0;
        if (pass) filteredRows.push(allRows[r]);
      }
      if (plan.activeSorts.length) filteredRows.sort(plan.cmpRows);
      entry.filteredRows = filteredRows;
      buildGroups(entry, filteredRows, new Set(table.collapsedGroups || []));
      return wrapResult(entry, table);
    }

    function wrapResult(entry, table) {
      if (!entry.plan.groupField && entry.groups[0]) entry.groups[0].count = entry.filteredRows.length;
      const q = { table, fields: table.fields, totalCount: entry.allRows.length, filteredCount: entry.filteredRows.length, rows: entry.filteredRows, groups: entry.groups, numericSums: entry.numericSums, index: stateIndex };
      Object.defineProperty(q, "numericCounts", { value: entry.numericCounts, enumerable: false, configurable: true });
      entry.query = q; return q;
    }
    function buildFullEntry(state, table) {
      stateIndex.relationWork = undefined;
      const rel = analyzeRelations(table, stateIndex), plan = compilePlan(table, stateIndex, M), recs = table.records;
      const allRows = new Array(recs.length), passFlags = new Array(recs.length), idxById = new Map();
      for (let r = 0; r < recs.length; r += 1) { allRows[r] = evalRow(state, table, recs[r], r, plan); idxById.set(recs[r].id, r); }
      const entry = {
        fSig: fieldsSig(table.fields), vSig: viewSig(table), cSig: (table.collapsedGroups || []).join("\0"),
        plan, selfCyclic: rel.selfCyclic, extDeps: rel.externalDeps, extRevs: new Map(rel.externalDeps.map((id) => [id, revOf(id)])),
        budgetHit: Boolean(stateIndex.relationWork && stateIndex.relationWork.left <= 0), allRows, passFlags, idxById, filteredRows: [], dirty: new Set(),
      };
      projectView(entry, table); return entry;
    }

    function applySumDelta(entry, prevRow, nextRow, reSumIds) {
      for (const id of entry.plan.sumIds) {
        const ov = prevRow ? prevRow.values[id] : undefined, nv = nextRow ? nextRow.values[id] : undefined;
        const oIsNum = typeof ov === "number", nIsNum = typeof nv === "number";
        if (oIsNum !== nIsNum) entry.numericCounts[id] += nIsNum ? 1 : -1;
        const oFin = oIsNum && Number.isFinite(ov) ? ov : 0, nFin = nIsNum && Number.isFinite(nv) ? nv : 0;
        if (oFin === nFin) continue;
        const delta = nFin - oFin, cur = entry.numericSums[id];
        if (!entry.hasFloat[id] && Number.isSafeInteger(oFin) && Number.isSafeInteger(nFin) && Number.isSafeInteger(cur) && Number.isSafeInteger(cur + delta)) entry.numericSums[id] = cur + delta;
        else { if (!Number.isSafeInteger(nFin)) entry.hasFloat[id] = true; reSumIds.add(id); }
      }
    }

    function reSumFiltered(entry, reSumIds) {
      for (const id of reSumIds) {
        let s = 0, flt = false;
        for (const r of entry.filteredRows) { const v = r.values[id]; if (typeof v === "number" && Number.isFinite(v)) { s += v; if (!Number.isSafeInteger(v)) flt = true; } }
        entry.numericSums[id] = s; entry.hasFloat[id] = flt;
        if (entry.plan.groupField) for (const g of entry.groups) { let gs = 0; for (const r of g.records) { const v = r.values[id]; if (typeof v === "number" && Number.isFinite(v)) gs += v; } g.numericSums[id] = gs; }
      }
    }

    function patchGroupedRow(entry, prevRow, nextRow, wasPass, nowPass, reSumIds) {
      if (!wasPass && !nowPass) return true;
      const gf = entry.plan.groupField;
      if (!wasPass || !nowPass) return false;
      const prevKey = groupKeyLabel(prevRow, gf).key, nextKey = groupKeyLabel(nextRow, gf).key;
      const bucket = prevKey === nextKey ? entry.groupByKey?.get(prevKey) : null;
      const cmp = entry.plan.cmpRows, oldHead = bucket?.records[0];
      if (!bucket) return false;
      const pos = binarySearchExact(bucket.records, prevRow, cmp);
      if (pos < 0) return false;
      if (gf.type !== "select" && (pos === 0 || cmp(nextRow, oldHead) < 0)) return false;
      if (replaceSorted(bucket.records, prevRow, nextRow, cmp) < 0) return false;
      for (const id of entry.plan.sumIds) {
        const ov = prevRow.values[id], nv = nextRow.values[id];
        const oF = typeof ov === "number" && Number.isFinite(ov) ? ov : 0, nF = typeof nv === "number" && Number.isFinite(nv) ? nv : 0;
        if (!reSumIds.has(id) && Number.isSafeInteger(bucket.numericSums[id] + nF - oF)) bucket.numericSums[id] += nF - oF; else if (oF !== nF) reSumIds.add(id);
      }
      return true;
    }

    function flushDirty(state, table, entry) {
      if (entry.dirty.size > MAX_DIRTY_BATCH) return false;
      stateIndex.relationWork = { left: 100000 };
      const { plan, filteredRows, idxById } = entry, reSumIds = new Set();
      let groupRebuild = false;
      for (const recId of entry.dirty) {
        const idx = idxById.get(recId), rec = stateIndex.recordMapsByTableId.get(table.id)?.get(recId);
        if (idx === undefined || !rec) { entry.dirty.clear(); return false; }
        const prevRow = entry.allRows[idx], wasPass = entry.passFlags[idx] === 1;
        const nextRow = evalRow(state, table, rec, idx, plan), nowPass = rowPasses(nextRow, plan);
        entry.allRows[idx] = nextRow; entry.passFlags[idx] = nowPass ? 1 : 0;
        if (!plan.activeFilters.length && !plan.activeSorts.length && !plan.groupField) {
          if (prevRow) filteredRows[idx] = nextRow; else filteredRows.push(nextRow);
          applySumDelta(entry, prevRow, nextRow, reSumIds);
          continue;
        }
        if (wasPass && nowPass) { if (replaceSorted(filteredRows, prevRow, nextRow, plan.cmpRows) < 0) return false; }
        else if (wasPass) { const p = binarySearchExact(filteredRows, prevRow, plan.cmpRows); if (p < 0) return false; filteredRows.splice(p, 1); }
        else if (nowPass) filteredRows.splice(lowerBound(filteredRows, nextRow, plan.cmpRows), 0, nextRow);
        applySumDelta(entry, wasPass ? prevRow : null, nowPass ? nextRow : null, reSumIds);
        if (plan.groupField && !groupRebuild && !patchGroupedRow(entry, prevRow, nextRow, wasPass, nowPass, reSumIds)) groupRebuild = true;
      }
      entry.dirty.clear();
      if (stateIndex.relationWork.left <= 0) { entry.budgetHit = true; return false; }
      if (groupRebuild) buildGroups(entry, filteredRows, new Set(table.collapsedGroups || [])); else reSumFiltered(entry, reSumIds);
      return true;
    }

    function query(state, tableId) {
      ensureIndex(state);
      const table = M.getTable(state, tableId);
      if (!table) return { table: null, fields: [], totalCount: 0, filteredCount: 0, rows: [], groups: [], numericSums: {}, index: stateIndex };
      const fSig = fieldsSig(table.fields), vSig = viewSig(table), cSig = (table.collapsedGroups || []).join("\0");
      let entry = entries.get(table.id);
      if (!entry || entry.fSig !== fSig || entry.allRows.length !== table.records.length || (entry.selfCyclic && entry.dirty.size > 0)
        || entry.budgetHit || entry.extDeps.some((id) => entry.extRevs.get(id) !== revOf(id))) {
        entry = buildFullEntry(state, table); entries.set(table.id, entry); return entry.query;
      }
      if (entry.dirty.size > 0 && !flushDirty(state, table, entry)) {
        entry = buildFullEntry(state, table); entries.set(table.id, entry); return entry.query;
      }
      if (entry.vSig !== vSig) { entry.plan = compilePlan(table, stateIndex, M); entry.vSig = vSig; entry.cSig = cSig; return projectView(entry, table); }
      if (entry.cSig !== cSig) {
        entry.cSig = cSig;
        if (entry.plan.groupField) { const col = new Set(table.collapsedGroups || []); for (const g of entry.groups) g.collapsed = col.has(g.key); }
      }
      return wrapResult(entry, table);
    }

    function notePaste(state, tableId, dirtyIds, createdRecords) {
      const recMap = state === boundState ? stateIndex?.recordMapsByTableId.get(tableId) : null;
      if (recMap) for (const rec of createdRecords) recMap.set(rec.id, rec);
      bumpRev(tableId);
      const entry = state === boundState ? entries.get(tableId) : null;
      if (!entry) return;
      if (entry.selfCyclic || entry.budgetHit || dirtyIds.length + createdRecords.length > MAX_DIRTY_BATCH) { entries.delete(tableId); return; }
      for (const id of dirtyIds) entry.dirty.add(id);
      for (const rec of createdRecords) { entry.idxById.set(rec.id, entry.allRows.length); entry.allRows.push(null); entry.passFlags.push(0); entry.dirty.add(rec.id); }
      if (entry.dirty.size > MAX_DIRTY_BATCH) entries.delete(tableId);
    }

    function noteDelete(state, tableId, recId) {
      if (state === boundState) stateIndex?.recordMapsByTableId.get(tableId)?.delete(recId);
      bumpRev(tableId);
      const entry = state === boundState ? entries.get(tableId) : null;
      if (!entry) return;
      if (entry.selfCyclic || entry.budgetHit || entry.dirty.size > 0) { entries.delete(tableId); return; }
      const delIdx = entry.idxById.get(recId), prevRow = delIdx === undefined ? null : entry.allRows[delIdx];
      if (!prevRow) { entries.delete(tableId); return; }
      if (entry.passFlags[delIdx] === 1) {
        const pos = !entry.plan.activeFilters.length && !entry.plan.activeSorts.length ? delIdx : binarySearchExact(entry.filteredRows, prevRow, entry.plan.cmpRows);
        if (pos < 0) { entries.delete(tableId); return; }
        entry.filteredRows.splice(pos, 1);
        if (entry.plan.groupField) buildGroups(entry, entry.filteredRows, new Set(M.getTable(state, tableId)?.collapsedGroups || []));
        else { const reSumIds = new Set(); applySumDelta(entry, prevRow, null, reSumIds); reSumFiltered(entry, reSumIds); }
      }
      entry.allRows.splice(delIdx, 1); entry.passFlags.splice(delIdx, 1); entry.idxById.delete(recId);
      for (let i = delIdx; i < entry.allRows.length; i += 1) { entry.allRows[i].origIdx = i; entry.idxById.set(entry.allRows[i].record.id, i); }
    }

    return {
      index: ensureIndex,
      reset: (s) => { boundState = s || null; entries.clear(); stateIndex = s ? M.buildStateIndex(s) : null; },
      invalidateSchema: (s) => { entries.clear(); boundState = s || null; stateIndex = s ? M.buildStateIndex(s) : null; if (s?.tables) for (const t of s.tables) bumpRev(t.id); },
      noteUpdate: (s, id, ids) => notePaste(s, id, ids, []),
      noteAdd: (s, id, rec) => notePaste(s, id, [], [rec]),
      notePaste, noteDelete, query,
    };
  }

  host.__EASYOFFICE_CREATE_BASE_QUERY_CACHE__ = createBaseQueryCache;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
