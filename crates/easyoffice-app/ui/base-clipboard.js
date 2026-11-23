// 참조 구현 (reference implementation): glide-data-grid (`glideapps/glide-data-grid`, MIT, Copyright (c) 2021 typeguard, Inc.,
// commit 0875d78cc41535a39f8b417ff88ce1ebe129c363) 및 Univer (`dream-num/univer`, Apache-2.0, Copyright 2023-present
// DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9; see `THIRD_PARTY_NOTICES.md`) 다중 범위 복사·
// 잘라내기·채우기·레코드 추가 붙여넣기 구조 참조(이식 코드 0줄).

(function (host) {
  "use strict";

  const MAX_MULTI_RANGES = 64, MAX_FILL_EXTEND_DIGITS = 15, MS_PER_DAY = 86400000;
  const ISO_DATE = /^(\d{4})-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])$/;
  const TRAIL_DIGITS = /^([\s\S]*?)(\d+)$/;
  const WEEK_KO = ["일", "월", "화", "수", "목", "금", "토"];
  const LOOP_LISTS = [
    WEEK_KO, WEEK_KO.map((d) => d + "요일"), Array.from({ length: 12 }, (_, i) => i + 1 + "월"),
    ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
    ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"],
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
  ];

  const cloneVal = (v) => (Array.isArray(v) ? [...v] : v);
  const byNumAsc = (a, b) => a - b;
  const cellVal = (field, raw, relationIds) => (field.type === "relation" && typeof raw === "string" ? relationIds(field, raw) : raw);

  function parseUtcDateMs(raw) {
    const m = ISO_DATE.exec(String(raw || "").trim());
    if (!m) return null;
    const y = Number(m[1]), mo = Number(m[2]), d = Number(m[3]);
    const ms = Date.UTC(y, mo - 1, d);
    const dt = new Date(ms);
    return dt.getUTCFullYear() === y && dt.getUTCMonth() === mo - 1 && dt.getUTCDate() === d ? ms : null;
  }

  function formatUtcDate(ms) {
    const dt = new Date(ms), p = (n) => String(n).padStart(2, "0");
    return `${dt.getUTCFullYear()}-${p(dt.getUTCMonth() + 1)}-${p(dt.getUTCDate())}`;
  }

  function serializeRanges(ranges, win, fields, toTsv) {
    if (!ranges || ranges.length === 0) return { ok: true, text: "" };
    if (ranges.length === 1) {
      const rg = ranges[0], rows = [];
      for (let r = rg.r1; r <= rg.r2; r += 1) {
        const rec = win.recordAt(r - 1);
        if (rec) rows.push(fields.slice(rg.c1 - 1, rg.c2).map((f) => rec.displays[f.id]));
      }
      return { ok: true, text: toTsv(rows) };
    }
    const base = ranges[0];
    if (ranges.every((rg) => rg.c1 === base.c1 && rg.c2 === base.c2)) {
      const rowSet = new Set();
      for (const rg of ranges) for (let r = rg.r1; r <= rg.r2; r += 1) if (win.recordAt(r - 1)) rowSet.add(r);
      const rows = Array.from(rowSet).sort(byNumAsc).map((r) => {
        const rec = win.recordAt(r - 1);
        return fields.slice(base.c1 - 1, base.c2).map((f) => rec.displays[f.id]);
      });
      return { ok: true, text: toTsv(rows) };
    }
    if (ranges.every((rg) => rg.r1 === base.r1 && rg.r2 === base.r2)) {
      const colSet = new Set();
      for (const rg of ranges) for (let c = rg.c1; c <= rg.c2; c += 1) colSet.add(c);
      const cols = Array.from(colSet).sort(byNumAsc), rows = [];
      for (let r = base.r1; r <= base.r2; r += 1) {
        const rec = win.recordAt(r - 1);
        if (rec) rows.push(cols.map((c) => rec.displays[fields[c - 1].id]));
      }
      return { ok: true, text: toTsv(rows) };
    }
    return { ok: false, error: "다중 선택 영역의 행 또는 열 범위가 서로 달라 복사할 수 없습니다." };
  }

  function clearEditsOf(ranges, win, fields, emptyValueOf) {
    const edits = [], seen = new Set();
    for (const rg of ranges) {
      for (let r = rg.r1; r <= rg.r2; r += 1) {
        const rec = win.recordAt(r - 1);
        if (!rec) continue;
        for (let c = rg.c1; c <= rg.c2; c += 1) {
          const fld = fields[c - 1];
          if (!fld || fld.type === "formula") continue;
          const key = rec.record.id + "\0" + fld.id;
          if (!seen.has(key)) { seen.add(key); edits.push({ recordId: rec.record.id, fieldId: fld.id, value: emptyValueOf(fld) }); }
        }
      }
    }
    return edits;
  }

  // Plans existing-cell edits plus new rows when pasted lines pass the end of the table or land on `+`.
  function planPaste(rows, ranges, win, fields, relationIds, appendOnly = false) {
    const last = ranges?.[ranges.length - 1] || { r1: 1, c1: 1, r2: 1, c2: 1 };
    const multiTarget = (ranges?.length || 0) > 1 || last.r1 !== last.r2 || last.c1 !== last.c2;
    const edits = [], newRows = [], seen = new Set();
    if (!appendOnly && rows.length === 1 && rows[0].length === 1 && multiTarget) {
      const txt = rows[0][0];
      for (const rg of ranges) {
        for (let r = rg.r1; r <= rg.r2; r += 1) {
          const rec = win.recordAt(r - 1);
          if (!rec) continue;
          for (let c = rg.c1; c <= rg.c2; c += 1) {
            const fld = fields[c - 1], key = rec.record.id + "\0" + fld.id;
            if (!fld || fld.type === "formula" || seen.has(key)) continue;
            seen.add(key);
            edits.push({ recordId: rec.record.id, fieldId: fld.id, value: cellVal(fld, txt, relationIds) });
          }
        }
      }
      return { edits, newRows, startC: last.c1, endExistingR: last.r2, endC: last.c2, singleFill: true };
    }
    const startC = appendOnly ? 1 : last.c1, count = win.count, cols = fields.length;
    let r = appendOnly ? count + 1 : last.r1, endExistingR = 0, endC = startC;
    for (const line of rows) {
      while (r <= count && win.isGroup(r - 1)) r += 1;
      const limit = Math.min(line.length, cols - startC + 1);
      if (limit > 0) endC = Math.max(endC, startC + limit - 1);
      if (r <= count) {
        const rec = win.recordAt(r - 1);
        for (let j = 0; j < limit; j += 1) {
          const fld = fields[startC + j - 1];
          if (rec && fld && fld.type !== "formula") edits.push({ recordId: rec.record.id, fieldId: fld.id, value: cellVal(fld, line[j], relationIds) });
        }
        endExistingR = r; r += 1;
      } else {
        const rawValues = {};
        for (let j = 0; j < limit; j += 1) {
          const fld = fields[startC + j - 1];
          if (fld && fld.type !== "formula") rawValues[fld.id] = cellVal(fld, line[j], relationIds);
        }
        newRows.push(rawValues);
      }
    }
    return { edits, newRows, startC, endExistingR, endC, singleFill: false };
  }

  // Writes existing-cell updates and appends pasted new records atomically under MAX_RECORDS_PER_TABLE, refusing whole when exceeded.
  function applyPasteMutation(M, state, tbl, edits, newRows, indexOpt) {
    if (newRows.length > 0 && tbl.records.length + newRows.length > M.MAX_RECORDS_PER_TABLE) {
      return { ok: false, error: `테이블당 최대 레코드 수(${M.MAX_RECORDS_PER_TABLE}건)를 초과했습니다.` };
    }
    const index = indexOpt || M.buildStateIndex(state);
    const fieldsById = index.fieldsByTableId.get(tbl.id)?.byId || new Map(tbl.fields.map((f) => [f.id, f]));
    const recordsById = index.recordMapsByTableId.get(tbl.id) || new Map(tbl.records.map((r) => [r.id, r]));
    const createdIds = [], createdRecords = [], dirtySet = new Set();
    let applied = 0, failed = 0, error = "";
    for (const { recordId, fieldId, value } of edits) {
      const fld = fieldsById.get(fieldId), rec = recordsById.get(recordId);
      const res = fld && rec ? M.validateCellValue(fld, value, state, index) : { ok: false, error: "대상 셀을 찾을 수 없습니다." };
      if (res.ok) { rec.values[fieldId] = res.value; dirtySet.add(recordId); applied += 1; } else { failed += 1; error ||= res.error; }
    }
    for (const rawValues of newRows) {
      const clean = {}; let rowOk = true;
      for (const fld of tbl.fields) {
        if (fld.type === "formula") continue;
        const def = fld.type === "checkbox" ? false : (fld.type === "multiSelect" || fld.type === "relation") ? [] : "";
        if (Object.hasOwn(rawValues, fld.id)) {
          const res = M.validateCellValue(fld, rawValues[fld.id], state, index);
          if (res.ok) { clean[fld.id] = res.value; applied += 1; continue; }
          failed += 1; error ||= res.error;
        }
        const defRes = M.validateCellValue(fld, def, state, index);
        if (!defRes.ok) { rowOk = false; failed += 1; error ||= defRes.error; break; }
        clean[fld.id] = defRes.value;
      }
      if (rowOk) {
        const rSeq = state.nextRecordSeq || tbl.records.length + 1;
        state.nextRecordSeq = rSeq + 1;
        const record = { id: `rec-${rSeq}`, values: clean };
        tbl.records.push(record); recordsById.set(record.id, record);
        createdIds.push(record.id); createdRecords.push(record);
      }
    }
    return applied > 0 || createdIds.length > 0 ? { ok: true, applied, createdIds, createdRecords, dirtyIds: Array.from(dirtySet), failed, error } : { ok: false, error: error || "붙여 넣을 칸이 없습니다." };
  }

  function dataItemsBetween(win, r1, r2) {
    const out = [];
    for (let r = r1; r <= r2; r += 1) if (win.recordAt(r - 1)) out.push(r);
    return out;
  }

  function seriesStep(vals, len, field, forward) {
    const seq = forward ? vals : vals.slice().reverse(), last = seq[seq.length - 1];
    if (field.type === "number" && seq.every((x) => typeof x.raw === "number" && Number.isFinite(x.raw))) {
      if (seq.length === 1) return Array.from({ length: len }, () => last.raw);
      const diff = seq[1].raw - seq[0].raw;
      if (seq.every((x, i) => i === 0 || Math.abs(x.raw - seq[i - 1].raw - diff) < 1e-9)) {
        return Array.from({ length: len }, (_, i) => seq[seq.length - 1].raw + diff * (i + 1));
      }
    }
    if (field.type === "date") {
      const ms = seq.map((x) => parseUtcDateMs(x.raw));
      if (ms.every((t) => t !== null)) {
        const diff = ms.length === 1 ? (forward ? MS_PER_DAY : -MS_PER_DAY) : ms[1] - ms[0];
        if (ms.every((t, i) => i < 2 || t - ms[i - 1] === diff)) return Array.from({ length: len }, (_, i) => formatUtcDate(ms[ms.length - 1] + diff * (i + 1)));
      }
    }
    if (field.type === "text") {
      const disps = seq.map((x) => x.disp);
      const loop = LOOP_LISTS.find((l) => disps.every((d) => l.includes(d)));
      if (loop) {
        const idx = disps.map((d) => loop.indexOf(d)), diff = idx.length === 1 ? (forward ? 1 : -1) : idx[1] - idx[0];
        return Array.from({ length: len }, (_, i) => loop[(((idx[idx.length - 1] + diff * (i + 1)) % loop.length) + loop.length) % loop.length]);
      }
      const parts = disps.map((d) => TRAIL_DIGITS.exec(d));
      if (parts.every((m) => m && m[2].length <= MAX_FILL_EXTEND_DIGITS && m[1] === parts[0][1])) {
        const nums = parts.map((m) => Number(m[2])), diff = nums.length === 1 ? (forward ? 1 : -1) : nums[1] - nums[0];
        if (nums.every((n, i) => i < 2 || n - nums[i - 1] === diff)) {
          return Array.from({ length: len }, (_, i) => parts[parts.length - 1][1] + String(Math.max(0, nums[nums.length - 1] + diff * (i + 1))).padStart(parts[parts.length - 1][2].length, "0"));
        }
      }
    }
    return Array.from({ length: len }, (_, i) => cloneVal(seq[i % seq.length].raw));
  }

  function buildFillEdits(mode, src, target, win, fields, relationIds) {
    const edits = [];
    if (mode === "down") {
      const items = dataItemsBetween(win, src.r1, src.r2);
      if (!items.length) return edits;
      let fromR = items[0], dsts = items.slice(1);
      if (items.length === 1) {
        for (let r = items[0] - 1; r >= 1; r -= 1) if (win.recordAt(r - 1)) { fromR = r; break; }
        if (fromR === items[0]) return edits;
        dsts = [items[0]];
      }
      const srcRec = win.recordAt(fromR - 1);
      for (const r of dsts) {
        const dstRec = win.recordAt(r - 1);
        for (let c = src.c1; c <= src.c2; c += 1) {
          const fld = fields[c - 1];
          if (fld && dstRec && fld.type !== "formula") edits.push({ recordId: dstRec.record.id, fieldId: fld.id, value: cloneVal(srcRec.values[fld.id]) });
        }
      }
      return edits;
    }
    if (mode === "right") {
      const fromC = src.c2 > src.c1 ? src.c1 : src.c1 - 1, cols = src.c2 > src.c1 ? Array.from({ length: src.c2 - src.c1 }, (_, i) => src.c1 + 1 + i) : [src.c1];
      if (fromC < 1) return edits;
      const srcFld = fields[fromC - 1];
      for (const r of dataItemsBetween(win, src.r1, src.r2)) {
        const rec = win.recordAt(r - 1);
        for (const c of cols) {
          const fld = fields[c - 1];
          if (!fld || fld.type === "formula") continue;
          const sameType = fld.type === srcFld.type && (srcFld.type !== "relation" || fld.relationTableId === srcFld.relationTableId);
          edits.push({ recordId: rec.record.id, fieldId: fld.id, value: sameType ? cloneVal(rec.values[srcFld.id]) : cellVal(fld, rec.displays[srcFld.id] ?? "", relationIds) });
        }
      }
      return edits;
    }
    const vert = target.r2 > src.r2 || target.r1 < src.r1;
    if (vert) {
      const forward = target.r2 > src.r2, srcRows = dataItemsBetween(win, src.r1, src.r2);
      const dstRows = forward ? dataItemsBetween(win, src.r2 + 1, target.r2) : dataItemsBetween(win, target.r1, src.r1 - 1).reverse();
      if (!srcRows.length || !dstRows.length) return edits;
      for (let c = src.c1; c <= src.c2; c += 1) {
        const fld = fields[c - 1];
        if (!fld || fld.type === "formula") continue;
        const vals = srcRows.map((r) => { const rec = win.recordAt(r - 1); return { raw: rec.values[fld.id], disp: rec.displays[fld.id] }; });
        seriesStep(vals, dstRows.length, fld, forward).forEach((v, i) => edits.push({ recordId: win.recordAt(dstRows[i] - 1).record.id, fieldId: fld.id, value: v }));
      }
      return edits;
    }
    const forward = target.c2 > src.c2, srcLen = src.c2 - src.c1 + 1;
    const dstCols = forward ? Array.from({ length: target.c2 - src.c2 }, (_, i) => src.c2 + 1 + i) : Array.from({ length: src.c1 - target.c1 }, (_, i) => src.c1 - 1 - i);
    for (const r of dataItemsBetween(win, src.r1, src.r2)) {
      const rec = win.recordAt(r - 1);
      dstCols.forEach((c, idx) => {
        const fld = fields[c - 1], fromCol = forward ? src.c1 + (idx % srcLen) : src.c2 - (idx % srcLen), srcFld = fields[fromCol - 1];
        if (!fld || !srcFld || fld.type === "formula") return;
        const sameType = fld.type === srcFld.type && (srcFld.type !== "relation" || fld.relationTableId === srcFld.relationTableId);
        edits.push({ recordId: rec.record.id, fieldId: fld.id, value: sameType ? cloneVal(rec.values[srcFld.id]) : cellVal(fld, rec.displays[srcFld.id] ?? "", relationIds) });
      });
    }
    return edits;
  }

  host.__EASYOFFICE_BASE_CLIPBOARD__ = Object.freeze({ MAX_MULTI_RANGES, serializeRanges, clearEditsOf, planPaste, applyPasteMutation, buildFillEdits });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
