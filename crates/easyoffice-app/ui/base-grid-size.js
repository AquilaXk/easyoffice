// 참조 구현 (reference implementation): Univer (Apache-2.0, Copyright 2023-present DreamNum Co., Ltd.,
// commit 4767b3b309467c283bbd52b725536ca73861e0fc,
// `packages/sheets-ui/src/controllers/render-controllers/header-resize.render-controller.ts`;
// see `THIRD_PARTY_NOTICES.md`) 열·행 경계 끌기 및 더블클릭 자동 맞춤 상태 전이 구조 참조(이식 코드 0줄).

(function (host) {
  "use strict";

  const HEADER_EXTRA_PX = 82, CELL_EXTRA_PX = 24, GLYPH_MAX_PX = 16, MAX_FIT_MEASURES = 50000;
  const CORNER_PX = 56, DEFAULT_COL_PX = 144, SAMPLE_ROWS = 40;
  const AUTO_GROW_TYPES = new Set(["relation", "multiSelect", "formula", "select"]);

  function createBaseGridSize(doc, deps) {
    const M = host.__EASYOFFICE_BASE_MODEL__;
    const MIN_COL_PX = M?.MIN_COL_PX || 64, MAX_COL_PX = M?.MAX_COL_PX || 1200;
    const MIN_ROW_PX = M?.MIN_ROW_PX || 20, MAX_ROW_PX = M?.MAX_ROW_PX || 400, DEFAULT_ROW_PX = M?.DEFAULT_ROW_PX || 24;
    const metaByTable = new Map();
    let measureCtx = null, drag = null, ctx = null;

    const clamp = (v, min, max) => Math.max(min, Math.min(max, Math.round(v)));

    function measureTextPx(text, font) {
      const s = String(text || "");
      if (!s) return 0;
      if (measureCtx === null) {
        try { measureCtx = doc.createElement("canvas")?.getContext?.("2d") || false; } catch { measureCtx = false; }
      }
      if (measureCtx) {
        measureCtx.font = font;
        return measureCtx.measureText(s).width;
      }
      let w = 0;
      for (const ch of s) w += ch.codePointAt(0) < 0x80 ? 7.5 : 13;
      return w;
    }

    function maxNonInputLens(fields, rows) {
      const lens = new Array(fields.length).fill(0), limit = Math.min(rows.length, SAMPLE_ROWS);
      for (let i = 0; i < limit; i += 1) {
        const disp = rows[i].displays;
        for (let c = 0; c < fields.length; c += 1) {
          if (!AUTO_GROW_TYPES.has(fields[c].type)) continue;
          const n = String(disp[fields[c].id] || "").length;
          if (n > lens[c]) lens[c] = n;
        }
      }
      return lens;
    }

    function prepareAutoWidths(table, query, footTexts) {
      const widthKey = table.id + "|" + query.fields.map((f) => `${f.id}:${f.name}:${f.type}`).join("|");
      const empty = query.rows.length === 0, footLens = footTexts.map((t) => String(t || "").length);
      const chipMax = maxNonInputLens(query.fields, query.rows), prev = metaByTable.get(table.id);
      const growOnly = Boolean(prev && prev.widthKey === widthKey && ((prev.empty && !empty) || footLens.some((n, i) => n > (prev.footLens[i] || 0)) || chipMax.some((n, i) => query.fields[i]?.width === undefined && n > (prev.chipMax[i] || 0))));
      metaByTable.set(table.id, { widthKey, empty, footLens, chipMax });
      return { widthKey, growOnly };
    }

    function invalidateTable(tableId, gridWidths) {
      metaByTable.delete(tableId);
      const prefix = tableId + "|";
      for (const k of Array.from(gridWidths.keys())) if (k.startsWith(prefix)) gridWidths.delete(k);
    }

    function clearAll(gridWidths) {
      metaByTable.clear();
      gridWidths.clear();
    }

    function computeBestFitWidth(field, query, footText) {
      const typeLabel = M?.FIELD_TYPE_LABELS?.[field.type] || field.type || "";
      let best = measureTextPx(field.name, "700 13px sans-serif") + measureTextPx(typeLabel, "400 11px sans-serif") + HEADER_EXTRA_PX;
      if (footText) {
        const fw = measureTextPx(footText, "700 12px sans-serif") + CELL_EXTRA_PX;
        if (fw > best) best = fw;
      }
      const extraCtrl = field.type === "select" || field.type === "relation" || field.type === "multiSelect" ? 28 : field.type === "url" ? 26 : 0;
      const rows = query.rows || [];
      let measures = 0;
      for (let i = 0; i < rows.length && measures < MAX_FIT_MEASURES; i += 1) {
        const disp = String(rows[i].displays[field.id] ?? "");
        if (!disp || disp.length * GLYPH_MAX_PX + CELL_EXTRA_PX + extraCtrl <= best) continue;
        measures += 1;
        let lineMax = 0;
        for (const line of disp.split("\n")) {
          const w = measureTextPx(line, "400 13px sans-serif");
          if (w > lineMax) lineMax = w;
        }
        const total = lineMax + CELL_EXTRA_PX + extraCtrl;
        if (total > best) best = total;
      }
      return clamp(Math.ceil(best), MIN_COL_PX, MAX_COL_PX);
    }

    function ensureFixedColumns(tbl) {
      if (tbl.firstChild?.tagName === "COLGROUP") return tbl.firstChild;
      const ths = Array.from(tbl.querySelectorAll("thead th"));
      const colgroup = doc.createElement("colgroup");
      let total = 0;
      ths.forEach((th, idx) => {
        const boxW = Math.round(th.getBoundingClientRect().width), styledW = Number.parseFloat(th.style.width);
        const w = boxW > 0 ? boxW : styledW > 0 ? styledW : idx === 0 ? CORNER_PX : DEFAULT_COL_PX;
        const col = doc.createElement("col");
        col.style.width = w + "px";
        colgroup.appendChild(col);
        total += w;
      });
      tbl.insertBefore(colgroup, tbl.firstChild);
      tbl.style.width = total + "px";
      tbl.classList.add("is-fixed");
      return colgroup;
    }

    function onDocMouseMove(ev) {
      if (!drag) return;
      if (drag.kind === "col") {
        const next = clamp(drag.startW + (ev.clientX - drag.startX), MIN_COL_PX, MAX_COL_PX);
        if (next === drag.curW) return;
        drag.moved = true; drag.curW = next;
        drag.col.style.width = next + "px";
        drag.th.style.width = next + "px";
        let sum = 0;
        for (const c of drag.colgroup.childNodes) sum += Number.parseFloat(c.style.width) || DEFAULT_COL_PX;
        drag.tbl.style.width = sum + "px";
      } else {
        const next = clamp(drag.startH + (ev.clientY - drag.startY), MIN_ROW_PX, MAX_ROW_PX);
        if (next === drag.curH) return;
        drag.moved = true; drag.curH = next;
        ctx?.win?.previewRowHeight?.(next);
      }
    }

    function endDrag() {
      const done = drag;
      drag = null;
      doc.removeEventListener("mousemove", onDocMouseMove);
      doc.removeEventListener("mouseup", endDrag);
      if (!done || !done.moved) return;
      if (done.kind === "col" && done.curW !== done.startW) deps.setFieldWidth(done.fieldId, done.curW);
      else if (done.kind === "row" && done.curH !== done.startH) deps.setRowHeight(done.curH);
    }

    function onMouseDown(ev) {
      if (ev.button !== 0 || !ctx) return;
      const colHandle = ev.target?.closest?.(".col-resizer");
      if (colHandle) {
        const th = colHandle.closest("th.base-grid-th"), fieldId = th?.dataset?.fieldId;
        if (!fieldId) return;
        ev.preventDefault();
        ev.stopPropagation?.();
        const colgroup = ensureFixedColumns(ctx.tbl);
        const colIdx = ctx.colIdxOf.get(fieldId);
        const col = colgroup.childNodes[colIdx + 1];
        if (!col) return;
        const startW = Math.round(Number.parseFloat(col.style.width) || th.getBoundingClientRect().width || DEFAULT_COL_PX);
        drag = { kind: "col", fieldId, tbl: ctx.tbl, th, colgroup, col, startX: ev.clientX, startW, curW: startW, moved: false };
        doc.addEventListener("mousemove", onDocMouseMove);
        doc.addEventListener("mouseup", endDrag);
        return;
      }
      const rowHandle = ev.target?.closest?.(".row-resizer");
      if (!rowHandle) return;
      ev.preventDefault();
      ev.stopPropagation?.();
      const boxH = Math.round(rowHandle.closest("tr")?.getBoundingClientRect().height || 0);
      const startH = ctx.table.rowHeight || (boxH >= MIN_ROW_PX && boxH <= MAX_ROW_PX ? boxH : DEFAULT_ROW_PX);
      drag = { kind: "row", startY: ev.clientY, startH, curH: startH, moved: false };
      doc.addEventListener("mousemove", onDocMouseMove);
      doc.addEventListener("mouseup", endDrag);
    }

    function onDblClick(ev) {
      if (!ctx) return;
      const colHandle = ev.target?.closest?.(".col-resizer");
      if (colHandle) {
        const th = colHandle.closest("th.base-grid-th"), fieldId = th?.dataset?.fieldId;
        const idx = fieldId ? ctx.colIdxOf.get(fieldId) : undefined;
        if (idx === undefined) return;
        ev.preventDefault();
        ev.stopPropagation?.();
        const fitted = computeBestFitWidth(ctx.query.fields[idx], ctx.query, ctx.footSums[idx] || "");
        deps.setFieldWidth(fieldId, fitted);
        deps.setFeedback?.(`열 너비 자동 맞춤 (${fitted}px)`);
        return;
      }
      const rowHandle = ev.target?.closest?.(".row-resizer");
      if (!rowHandle) return;
      ev.preventDefault();
      ev.stopPropagation?.();
      if (ctx.table.rowHeight !== undefined) {
        deps.setRowHeight(null);
        deps.setFeedback?.("표준 높이(24px)로 복원");
      }
    }

    function bind(nextCtx) {
      drag = null;
      doc.removeEventListener("mousemove", onDocMouseMove);
      doc.removeEventListener("mouseup", endDrag);
      ctx = { ...nextCtx, colIdxOf: new Map(nextCtx.query.fields.map((f, i) => [f.id, i])) };
      ctx.tbl.addEventListener("mousedown", onMouseDown);
      ctx.tbl.addEventListener("dblclick", onDblClick);
    }

    function release() {
      drag = null;
      doc.removeEventListener("mousemove", onDocMouseMove);
      doc.removeEventListener("mouseup", endDrag);
      ctx = null;
    }

    const fitField = (query, fieldId) => {
      const idx = query.fields.findIndex((f) => f.id === fieldId);
      return idx < 0 ? null : computeBestFitWidth(query.fields[idx], query, "");
    };

    return { prepareAutoWidths, invalidateTable, clearAll, computeBestFitWidth, fitField, bind, release };
  }

  host.__EASYOFFICE_CREATE_BASE_GRID_SIZE__ = createBaseGridSize;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
