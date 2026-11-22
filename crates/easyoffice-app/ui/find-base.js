// 참조 구현 (reference implementation): Univer (`dream-num/univer`, Apache-2.0, Copyright 2023-present DreamNum Co., Ltd.,
// commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, `packages/sheets-find-replace/src/controllers/sheet-find-replace.controller.ts`
// 및 `packages/find-replace/src/services/find-replace.service.ts`; see `THIRD_PARTY_NOTICES.md`) 셀 블록 검색·수식 건너뛰기·1단계
// 실행 취소 구조 참조(이식 코드 0줄).

(function (host) {
  "use strict";

  const { applyEdits } = host.__EASYOFFICE_FIND_CORE__;

  // Collision-free block identifier for an arbitrary (recordId, fieldId) pair.
  const encodeId = (recId, fldId) => recId.length + ":" + recId + ":" + fldId;

  function decodeId(id) {
    const colon = id.indexOf(":");
    const len = Number(id.slice(0, colon));
    const start = colon + 1;
    return { recordId: id.slice(start, start + len), fieldId: id.slice(start + len + 1) };
  }

  function createBaseFindSource(deps = {}) {
    let unwatch = null;
    let watchCb = null;
    let replacing = false;

    function ensureWatch(studio) {
      if (unwatch || !watchCb || typeof studio?.onChange !== "function") return;
      unwatch = studio.onChange(() => { if (!replacing) watchCb?.(); });
    }

    // Indexes visible datasheet rows (walking `query.groups` in visual order, skipping collapsed groups in grid view
    // and skipping checkbox columns whose booleans have no visible TRUE/FALSE text on screen).
    function sync(index) {
      const studio = deps.getBaseStudio?.();
      if (!studio) { index.setBlocks([]); return; }
      ensureWatch(studio);
      studio.activeCellTarget?.();
      const query = studio.liveQuery?.();
      if (!query?.table) { index.setBlocks([]); return; }
      const gridOnly = query.table.view === "grid";
      const fields = query.fields || [];
      const blocks = [];
      for (const group of query.groups || []) {
        if (gridOnly && group.collapsed) continue;
        for (const row of group.records || []) {
          for (const field of fields) {
            if (field.type === "checkbox") continue;
            const text = String(row.displays[field.id] ?? "");
            if (text !== "") blocks.push({ id: encodeId(row.record.id, field.id), text });
          }
        }
      }
      index.setBlocks(blocks);
    }

    function anchor() {
      const at = deps.getBaseStudio?.()?.activeCellTarget?.();
      return at?.recordId && at?.fieldId ? { id: encodeId(at.recordId, at.fieldId), offset: 0 } : null;
    }

    function show(_near, current) {
      if (!current) return;
      const studio = deps.getBaseStudio?.();
      if (!studio) return;
      if (studio.liveQuery()?.table?.view !== "grid") studio.setView("grid");
      const { recordId, fieldId } = decodeId(current.id);
      studio.selectCellById?.(recordId, fieldId);
    }

    async function replace(plan) {
      const studio = deps.getBaseStudio?.(), M = host.__EASYOFFICE_BASE_MODEL__, query = studio?.liveQuery?.();
      if (!studio || !M || !query?.table) return { replaced: 0, skipped: 0, rejected: 0 };
      const fieldsById = new Map((query.fields || []).map((f) => [f.id, f]));
      const rowsById = new Map((query.rows || []).map((r) => [r.record.id, r]));
      const batch = [];
      let replaced = 0, skipped = 0, rejected = 0, firstError = "";

      for (const { id, edits } of plan) {
        const { recordId, fieldId } = decodeId(id);
        const field = fieldsById.get(fieldId), row = rowsById.get(recordId);
        if (!field || !row || field.type === "formula" || field.type === "checkbox") { skipped += edits.length; continue; }
        const nextText = applyEdits([String(row.displays[field.id] ?? "")], edits)[0];
        const raw = field.type === "relation" ? studio.relationIds(field, nextText) : nextText;
        const check = M.validateCellValue(field, raw, null, query.index);
        if (!check.ok) { rejected += edits.length; firstError ||= check.error; continue; }
        batch.push({ recordId, fieldId, value: check.value });
        replaced += edits.length;
      }

      if (batch.length === 0) {
        if (rejected > 0) {
          deps.setFeedback?.(firstError);
          throw new Error(firstError);
        }
        return { replaced: 0, skipped, rejected: 0 };
      }

      let res;
      replacing = true;
      try { res = studio.updateCells(batch); } finally { replacing = false; }
      if (!res.ok) throw new Error(res.error);
      if (rejected > 0) deps.setFeedback?.(`${rejected}개 일치는 필드 규칙에 맞지 않아 건너뛰었습니다: ${firstError}`);
      return { replaced, skipped, rejected };
    }

    function watch(cb) {
      watchCb = cb;
      ensureWatch(deps.getBaseStudio?.());
    }

    function dispose() {
      unwatch?.();
      unwatch = null;
      watchCb = null;
    }

    return { kind: "base", sync, watch, anchor, show, clearHighlight: () => {}, replace, dispose };
  }

  host.__EASYOFFICE_CREATE_BASE_FIND_SOURCE__ = createBaseFindSource;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
