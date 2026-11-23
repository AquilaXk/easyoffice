// 참조 구현 (reference implementation): 키 규칙과 글 처리는 `base-cell-nav.js`(glide-data-grid, MIT, Copyright (c) 2021 typeguard, Inc.),
// 다중 범위·잘라내기·채우기·붙여넣기는 `base-clipboard.js`(glide-data-grid, MIT; Univer, Apache-2.0),
// 스크롤 계산은 `base-virtual.js`(TanStack Virtual `virtual-core`, MIT, Copyright (c) 2021-present Tanner Linsley; see `THIRD_PARTY_NOTICES.md`).
// 이 파일은 EasyOffice 자체 코드다: 칸·다중범위·행·열 선택, 채우기 핸들, 키보드 이동, 복사·잘라내기·붙여넣기를 표 DOM에 잇는다 (PD-294, PD-316).

(function (host) {
  "use strict";

  const CELL = "td.base-grid-cell", COL_TH = "th.base-grid-th", ROW_IDX = "td.base-row-index";
  const ROW_CONTROLS = "td.base-grid-cell input, td.base-grid-cell select, td.base-grid-cell button, td.base-grid-cell a, button.base-row-delete-btn";
  const ACTIVE_ID = "base-active-cell";
  const NATIVE_CONTROL = "a, button, select, input.base-cell-checkbox";
  const IGNORE_HDR_TARGET = ".col-resizer, .row-resizer, button.base-field-edit-btn, button.base-row-delete-btn, button.base-inline-add-btn, button.base-group-toggle";
  const IN_RANGE = 1, EDGE_T = 2, EDGE_B = 4, EDGE_L = 8, EDGE_R = 16, ACTIVE = 32;
  const SEL_TOKENS = Array.from({ length: 64 }, (_, f) => ["s", f & EDGE_T && "t", f & EDGE_B && "b", f & EDGE_L && "l", f & EDGE_R && "r", f & ACTIVE && "a"].filter(Boolean).join(" "));
  const isTextInput = (el) => el.classList?.contains("base-cell-input");

  function createCellSelect(doc, deps) {
    const N = host.__EASYOFFICE_BASE_CELL_NAV__, nav = host.__EASYOFFICE_SHEET_NAV__, C = host.__EASYOFFICE_BASE_CLIPBOARD__;
    let sel = null, extraRanges = [], appendTarget = false, tableKey = "", viewKey = "", savedAnchor = null;
    let ctx = null, editing = null, refocus = false, drag = null, fillHandle = null, internalClip = "";

    const cellAt = (r, c) => ctx.win.rowAt(r - 1)?.childNodes[c];
    const coordsOf = (td) => {
      const item = td.parentElement ? ctx.win.itemOf(td.parentElement) : undefined, c = ctx.colOf.get(td.dataset.fieldId);
      return item === undefined || c === undefined ? null : { r: item + 1, c };
    };
    const allRanges = () => (sel ? (extraRanges.length ? [...extraRanges, N.rangeOf(sel)] : [N.rangeOf(sel)]) : []);

    function viewOf({ win, fields }) {
      let last = win.count, first = 1;
      while (last > 0 && win.isGroup(last - 1)) last -= 1;
      while (first <= win.count && win.isGroup(first - 1)) first += 1;
      return {
        maxRows: win.count, maxCols: fields.length, dataRows: last, dataCols: fields.length, firstData: first,
        hasValue: (r, c) => (win.recordAt(r - 1)?.displays[fields[c - 1].id] ?? "") !== "",
        mergeAt: () => null, rowHidden: (r) => win.isGroup(r - 1), colHidden: () => false,
      };
    }

    const inTable = (r, c) => r >= 1 && c >= 1 && r <= ctx.win.count && c <= ctx.fields.length;
    const selectionFits = () => [[sel.ar, sel.ac], [sel.cr, sel.cc], ...(sel.span ? [[sel.span.r2, sel.span.c2]] : [])].every(([r, c]) => inTable(r, c)) && !ctx.win.isGroup(sel.ar - 1);

    function rowIndexMap() {
      if (!ctx) return new Map();
      if (!ctx.rowOf) {
        const map = new Map();
        for (let i = 0; i < ctx.win.count; i += 1) { const rec = ctx.win.recordAt(i); if (rec) map.set(rec.record.id, i + 1); }
        ctx.rowOf = map;
      }
      return ctx.rowOf;
    }

    function rememberAnchor() {
      if (!ctx || !sel) { savedAnchor = null; return; }
      const recId = (r) => ctx.win.recordAt(r - 1)?.record.id || null, fldId = (c) => ctx.fields[c - 1]?.id || null;
      const isCol = (rg) => ctx.view.dataRows >= ctx.view.firstData && rg.r1 <= ctx.view.firstData && rg.r2 >= ctx.view.dataRows;
      savedAnchor = {
        arId: recId(sel.ar), acId: fldId(sel.ac), crId: recId(sel.cr), ccId: fldId(sel.cc), wholeCol: isCol(N.rangeOf(sel)),
        extra: extraRanges.map((rg) => ({ r1Id: recId(rg.r1), r2Id: recId(rg.r2), c1Id: fldId(rg.c1), c2Id: fldId(rg.c2), wholeCol: isCol(rg) })),
      };
    }

    // ---- painting ----

    function flagsFor(ranges, r, c, ar, ac) {
      let f = 0;
      for (const rg of ranges) {
        if (r < rg.r1 || r > rg.r2 || c < rg.c1 || c > rg.c2) continue;
        f |= IN_RANGE | (r === rg.r1 ? EDGE_T : 0) | (r === rg.r2 ? EDGE_B : 0) | (c === rg.c1 ? EDGE_L : 0) | (c === rg.c2 ? EDGE_R : 0);
      }
      return f && r === ar && c === ac ? f | ACTIVE : f;
    }

    function reachable(td, on) {
      td.querySelector("a.base-url-link")?.setAttribute("tabindex", on ? "0" : "-1");
    }

    function paintRowDelete(tr, on) {
      tr.querySelector("button.base-row-delete-btn")?.setAttribute("tabindex", on ? "0" : "-1");
    }

    function paintCell(td, flags, preview = false) {
      if (!td) return;
      td.setAttribute("aria-selected", flags ? "true" : "false");
      if (flags) td.setAttribute("data-sel", SEL_TOKENS[flags]); else td.removeAttribute("data-sel");
      if (preview) td.setAttribute("data-fill-preview", "true"); else td.removeAttribute("data-fill-preview");
      if (flags & ACTIVE) { td.setAttribute("id", ACTIVE_ID); ctx.tbl.setAttribute("aria-activedescendant", ACTIVE_ID); reachable(td, true); }
      else if (td.getAttribute("id") === ACTIVE_ID) { td.removeAttribute("id"); reachable(td, false); }
    }

    const inPreview = (r, c) => {
      if (drag?.kind !== "fill") return false;
      const s = drag.src, t = drag.target;
      return r >= t.r1 && r <= t.r2 && c >= t.c1 && c <= t.c2 && (r < s.r1 || r > s.r2 || c < s.c1 || c > s.c2);
    };

    function syncFillHandle() {
      if (fillHandle?.parentElement) fillHandle.remove();
      if (!ctx || !sel || editing || appendTarget || extraRanges.length > 0 || drag?.kind === "fill") return;
      const td = cellAt(N.rangeOf(sel).r2, N.rangeOf(sel).c2);
      if (!td) return;
      if (!fillHandle) { fillHandle = doc.createElement("span"); fillHandle.className = "base-fill-handle"; fillHandle.setAttribute("aria-hidden", "true"); }
      td.appendChild(fillHandle);
    }

    function paintBuilt() {
      if (!ctx) return;
      const ranges = allRanges(), { firstData, dataRows } = ctx.view, cols = ctx.fields.length;
      const built = ctx.win.builtItems();
      const wholeRowSet = new Set(), wholeColSet = new Set();
      for (const rg of ranges) {
        if (rg.c1 === 1 && rg.c2 >= cols) for (let r = rg.r1; r <= rg.r2; r += 1) wholeRowSet.add(r);
        if (dataRows >= firstData && rg.r1 <= firstData && rg.r2 >= dataRows) for (let c = rg.c1; c <= rg.c2; c += 1) wholeColSet.add(c);
      }
      ctx.tbl.querySelectorAll("thead " + COL_TH).forEach((th) => {
        const c = ctx.colOf.get(th.dataset.fieldId);
        if (wholeColSet.has(c)) th.setAttribute("data-sel-hdr", "true"); else th.removeAttribute("data-sel-hdr");
      });
      const newRow = ctx.tbl.querySelector("tr.base-new-record-row");
      if (newRow) { if (appendTarget) newRow.setAttribute("data-sel", "append"); else newRow.removeAttribute("data-sel"); }
      if (built) {
        for (let i = built.from; i <= built.to; i += 1) {
          const tr = ctx.win.rowAt(i);
          if (!tr || ctx.win.isGroup(i)) continue;
          const idxTd = tr.childNodes[0];
          if (idxTd) { if (wholeRowSet.has(i + 1)) idxTd.setAttribute("data-sel-hdr", "true"); else idxTd.removeAttribute("data-sel-hdr"); }
          paintRowDelete(tr, sel?.ar === i + 1);
          for (let c = 1; c <= cols; c += 1) paintCell(tr.childNodes[c], flagsFor(ranges, i + 1, c, sel?.ar, sel?.ac), inPreview(i + 1, c));
        }
      }
      if (!sel || !cellAt(sel.ar, sel.ac)) ctx.tbl.removeAttribute("aria-activedescendant");
      syncFillHandle();
    }

    function rowBuilt(tr, item) {
      for (const el of tr.querySelectorAll(ROW_CONTROLS)) {
        el.setAttribute("tabindex", "-1");
        if (isTextInput(el)) el.readOnly = true;
      }
      if (!ctx || ctx.win.isGroup(item)) return;
      const ranges = allRanges(), cols = ctx.fields.length;
      const idxTd = tr.childNodes[0];
      if (idxTd) {
        const wholeRow = ranges.some((rg) => rg.c1 === 1 && rg.c2 >= cols && item + 1 >= rg.r1 && item + 1 <= rg.r2);
        if (wholeRow) idxTd.setAttribute("data-sel-hdr", "true"); else idxTd.removeAttribute("data-sel-hdr");
      }
      paintRowDelete(tr, sel?.ar === item + 1);
      for (let c = 1; c <= cols; c += 1) paintCell(tr.childNodes[c], flagsFor(ranges, item + 1, c, sel?.ar, sel?.ac), inPreview(item + 1, c));
    }

    // ---- moving the selection ----

    function revealColumn(td) {
      const wrap = ctx.wrap;
      if (!td || !(wrap.clientWidth > 0)) return;
      const view = wrap.getBoundingClientRect(), box = td.getBoundingClientRect(), head = td.parentElement.firstChild.getBoundingClientRect().width;
      const startEdge = view.left + head, endEdge = view.left + wrap.clientWidth;
      if (box.width > Math.max(endEdge - startEdge, 0)) {
        if (box.left <= startEdge + 0.5 && box.right >= endEdge - 0.5) return;
        wrap.scrollLeft += box.left - startEdge;
      } else if (box.left < startEdge) wrap.scrollLeft -= startEdge - box.left;
      else if (box.right > endEdge) wrap.scrollLeft += box.right - endEdge;
    }

    function revealCorner() {
      ctx.win.scrollToItem(sel.cr - 1);
      revealColumn(cellAt(sel.cr, sel.cc));
    }

    function setSel(next, reveal, extra = extraRanges) {
      sel = next; extraRanges = extra; appendTarget = false;
      rememberAnchor();
      if (editing) leaveEdit(false);
      if (reveal && sel) revealCorner();
      rememberAnchor();
      paintBuilt();
    }

    function selectFirst() {
      if (ctx.view.firstData <= ctx.view.dataRows) setSel(N.single(ctx.view.firstData, 1), false, []);
    }

    // ---- editing ----

    function onEditBlur() {
      if (!editing) return;
      if (editing.text) editing.ctl.readOnly = true;
      editing = null; syncFillHandle();
    }

    function beginEdit(td, ctl) {
      editing = { td, ctl, text: isTextInput(ctl), orig: ctl.value };
      ctl.addEventListener("blur", onEditBlur, { once: true });
      syncFillHandle();
    }

    function leaveEdit(restore, keepFocus = true) {
      const edit = editing;
      if (!edit) return;
      if (restore && edit.text) edit.ctl.value = edit.orig;
      editing = null;
      if (edit.text) edit.ctl.readOnly = true;
      refocus = keepFocus;
      edit.ctl.blur();
      if (refocus) { refocus = false; ctx?.tbl.focus({ preventScroll: true }); }
      refocus = false;
      syncFillHandle();
    }

    function startEdit(seed) {
      const field = ctx.fields[sel.ac - 1];
      let td = cellAt(sel.ar, sel.ac);
      if (!td) { ctx.win.scrollToItem(sel.ar - 1); td = cellAt(sel.ar, sel.ac); }
      if (!td || field.type === "formula") return false;
      if (field.type === "checkbox") { if (seed === undefined) td.querySelector("input.base-cell-checkbox")?.click(); return false; }
      if (field.type === "multiSelect") { const pill = td.querySelector("button.base-multi-opt"); if (pill && seed === undefined) { beginEdit(td, pill); pill.focus({ preventScroll: true }); } return false; }
      if (field.type === "select" || field.type === "relation") {
        const select = td.querySelector("select");
        if (!select) return false;
        beginEdit(td, select); select.focus({ preventScroll: true });
        if (seed === undefined) { try { select.showPicker?.(); } catch { /* a picker needs a user gesture */ } }
        return false;
      }
      const input = td.querySelector("input.base-cell-input");
      if (!input) return false;
      beginEdit(td, input); input.readOnly = false;
      if (seed !== undefined) input.value = field.type === "date" ? "" : seed;
      input.focus({ preventScroll: true });
      if (seed === undefined) { try { input.setSelectionRange(input.value.length, input.value.length); } catch { /* a date input has no caret */ } }
      return seed !== undefined && field.type !== "date";
    }

    // ---- clipboard, cut, fill, append paste ----

    function copyText() {
      return C.serializeRanges(allRanges(), ctx.win, ctx.fields, N.toTsv);
    }

    function onCopy(ev) {
      if (editing || !sel || !ev.clipboardData) return;
      const res = copyText();
      ev.preventDefault();
      if (!res.ok) { deps.setFeedback(res.error); return; }
      internalClip = res.text;
      ev.clipboardData.setData("text/plain", res.text);
    }

    function onCut(ev) {
      if (editing || !sel || !ev.clipboardData) return;
      const res = copyText();
      ev.preventDefault();
      if (!res.ok) { deps.setFeedback(res.error); return; }
      internalClip = res.text;
      ev.clipboardData.setData("text/plain", res.text);
      clearRange();
    }

    function clearRange() {
      const edits = C.clearEditsOf(allRanges(), ctx.win, ctx.fields, N.emptyValueOf);
      if (edits.length > 0) deps.updateCells(edits);
    }

    function runPasteText(text) {
      if (!text) return false;
      const rows = N.parseTsv(text);
      if (!rows) { deps.setFeedback("붙여 넣을 수 없습니다: 글이 너무 길거나 칸이 너무 많습니다."); return false; }
      const ranges = allRanges(), wasAppend = appendTarget, r1 = wasAppend ? ctx.win.count + 1 : ranges[ranges.length - 1].r1;
      const plan = C.planPaste(rows, ranges, ctx.win, ctx.fields, deps.relationIds, wasAppend);
      if (plan.edits.length === 0 && plan.newRows.length === 0) return false;
      const res = deps.applyPaste ? deps.applyPaste(plan.edits, plan.newRows) : deps.updateCells(plan.edits);
      if (!res.ok) return true;
      appendTarget = false; extraRanges = [];
      if (res.createdIds?.length) {
        const rm = rowIndexMap(), firstNew = rm.get(res.createdIds[0]), lastNew = rm.get(res.createdIds[res.createdIds.length - 1]);
        if (firstNew && lastNew) {
          const top = wasAppend || plan.endExistingR === 0 ? firstNew : r1;
          setSel({ ar: top, ac: plan.startC, cr: lastNew, cc: plan.endC, span: null, tabFrom: null }, true, []);
        }
      } else if (!plan.singleFill && plan.endExistingR > 0 && sel) {
        const next = { ...sel, ar: r1, ac: plan.startC, cr: plan.endExistingR, cc: plan.endC, span: null };
        if (plan.endExistingR !== r1 || plan.endC !== plan.startC) setSel(next, false, []);
      }
      return true;
    }

    function onPaste(ev) {
      if (editing || (!sel && !appendTarget) || !ev.clipboardData) return;
      const text = ev.clipboardData.getData("text/plain");
      if (!text) return;
      ev.preventDefault();
      runPasteText(text);
    }

    async function runClipboardCommand(cmd, readSys, writeSys) {
      if (!ctx || editing) { deps.setFeedback("현재 보기나 편집 상태에서는 클립보드 명령을 실행할 수 없습니다."); return false; }
      if (cmd === "clip-copy" || cmd === "clip-cut") {
        if (!sel) { deps.setFeedback((cmd === "clip-cut" ? "오려 둘" : "복사할") + " 칸을 먼저 선택하세요"); return false; }
        const res = copyText();
        if (!res.ok) { deps.setFeedback(res.error); return false; }
        internalClip = res.text;
        const toSys = writeSys ? await writeSys(res.text) : false;
        if (cmd === "clip-cut") clearRange();
        deps.setFeedback("선택 범위 " + (cmd === "clip-cut" ? "오려 두기" : "복사") + " 완료" + (toSys ? "" : " (앱 내부 클립보드)"));
        return true;
      }
      if (cmd === "clip-paste") {
        if (!sel && !appendTarget) { deps.setFeedback("붙여넣을 위치의 칸을 먼저 선택하세요"); return false; }
        const text = (readSys ? await readSys() : "") || internalClip;
        if (!text) { deps.setFeedback("붙일 클립보드 내용이 없습니다"); return false; }
        return runPasteText(text);
      }
      return false;
    }

    function runFill(mode, targetRange, nextSelRange) {
      const src = N.rangeOf(sel), edits = C.buildFillEdits(mode, src, targetRange, ctx.win, ctx.fields, deps.relationIds);
      if (edits.length === 0) return;
      if (nextSelRange) {
        const up = nextSelRange.r1 < src.r1, left = nextSelRange.c1 < src.c1;
        setSel({ ar: up ? nextSelRange.r2 : nextSelRange.r1, ac: left ? nextSelRange.c2 : nextSelRange.c1, cr: up ? nextSelRange.r1 : nextSelRange.r2, cc: left ? nextSelRange.c1 : nextSelRange.c2, span: null, tabFrom: null }, false, []);
      }
      deps.updateCells(edits);
    }

    // ---- keys ----

    function run(intent, ev) {
      if (intent.t === "walk") {
        const next = N.walkAfter(ctx.view, sel, intent.dr, intent.dc);
        if (!editing && intent.dr === 0 && next.ar === sel.ar && next.ac === sel.ac) return;
        ev.preventDefault();
        setSel(next, true, []);
      } else if (intent.t === "cancel") { ev.preventDefault(); leaveEdit(true); }
      else if (intent.t === "edit" || intent.t === "type") { if (startEdit(intent.t === "type" ? intent.text : undefined) || intent.t === "edit") ev.preventDefault(); }
      else if (intent.t === "clear") { ev.preventDefault(); clearRange(); }
      else if (intent.t === "fill") { ev.preventDefault(); runFill(intent.dir, N.rangeOf(sel), null); }
      else if (intent.t === "selectRow") { const rg = N.rangeOf(sel); ev.preventDefault(); setSel({ ...N.single(rg.r1, 1), cr: rg.r2, cc: ctx.fields.length }, false, []); }
      else if (intent.t === "selectCol") { if (ctx.view.dataRows >= ctx.view.firstData) { const rg = N.rangeOf(sel); ev.preventDefault(); setSel({ ...N.single(ctx.view.firstData, rg.c1), cr: ctx.view.dataRows, cc: rg.c2 }, false, []); } }
      else if (intent.t === "escape") {
        if (appendTarget) { appendTarget = false; paintBuilt(); }
        else if (extraRanges.length > 0 || !N.isSingle(N.rangeOf(sel))) setSel(N.single(sel.ar, sel.ac), false, []);
      } else if (intent.t === "all") {
        ev.preventDefault();
        if (ctx.view.dataRows >= ctx.view.firstData) setSel({ ...N.single(ctx.view.firstData, 1), cr: ctx.view.dataRows, cc: ctx.fields.length }, false, []);
      } else {
        ev.preventDefault();
        setSel(N.selectionAfter(ctx.view, sel, intent, ctx.win.visibleSpan()), true, intent.extend ? extraRanges : []);
      }
    }

    function onKeyDown(ev) {
      if (!ctx || !sel) return;
      if (ev.isComposing || ev.key === "Process" || ev.keyCode === 229) {
        if (!editing && ev.key === "Process") startEdit("");
        return;
      }
      const intent = N.intentOf(ev, Boolean(editing));
      if (!intent || (editing && !editing.text && ev.key === "Enter")) return;
      if (!editing && doc.activeElement !== ctx.tbl) ctx.tbl.focus({ preventScroll: true });
      run(intent, ev);
    }

    // ---- pointer, headers, fill handle ----

    const nativeTarget = (ev, at) => {
      const hit = ev.target?.closest?.(NATIVE_CONTROL);
      if (!hit) return false;
      return hit.tagName !== "SELECT" || (sel?.ar === at.r && sel?.ac === at.c && N.isSingle(N.rangeOf(sel)) && extraRanges.length === 0);
    };

    function pickMulti(ev, nextSel, kind) {
      const nextExtra = ev.shiftKey && sel
        ? extraRanges
        : (ev.ctrlKey || ev.metaKey) && sel
          ? (extraRanges.length < C.MAX_MULTI_RANGES ? [...extraRanges, N.rangeOf(sel)] : extraRanges)
          : [];
      setSel(nextSel, false, nextExtra);
      ev.preventDefault();
      ctx.tbl.focus({ preventScroll: true });
      drag = { kind };
      doc.addEventListener("mousemove", onDragMove);
      doc.addEventListener("mouseup", endDrag);
    }

    function onFillDblClick(ev) {
      ev.preventDefault(); ev.stopPropagation?.();
      if (!ctx || !sel) return;
      const src = N.rangeOf(sel), first = src.r2 + 1;
      if (first > ctx.view.dataRows) return;
      const side = [src.c1 - 1, src.c2 + 1].find((c) => c >= 1 && c <= ctx.fields.length && ctx.view.hasValue(first, c));
      if (side === undefined) return;
      let end = first;
      while (end < ctx.view.dataRows && !ctx.win.isGroup(end) && ctx.view.hasValue(end + 1, side)) end += 1;
      runFill("handle", { r1: src.r1, r2: end, c1: src.c1, c2: src.c2 }, { r1: src.r1, r2: end, c1: src.c1, c2: src.c2 });
    }

    function onMouseDown(ev) {
      if (!ctx || ev.button !== 0) return;
      if (ev.target === fillHandle || ev.target?.closest?.(".base-fill-handle")) {
        if (!sel || editing) return;
        ev.preventDefault(); ev.stopPropagation?.();
        const src = N.rangeOf(sel);
        drag = { kind: "fill", src, target: { ...src } };
        doc.addEventListener("mousemove", onDragMove); doc.addEventListener("mouseup", endDrag);
        return;
      }
      if (ev.target?.closest?.(IGNORE_HDR_TARGET)) return;
      if (ev.target?.closest?.("th.base-grid-corner")) {
        if (ctx.view.dataRows >= ctx.view.firstData) { setSel({ ...N.single(ctx.view.firstData, 1), cr: ctx.view.dataRows, cc: ctx.fields.length }, false, []); ev.preventDefault(); ctx.tbl.focus({ preventScroll: true }); }
        return;
      }
      if (ev.target?.closest?.("tr.base-new-record-row")) {
        appendTarget = true; sel = null; extraRanges = []; savedAnchor = null; paintBuilt();
        ev.preventDefault(); ctx.tbl.focus({ preventScroll: true }); return;
      }
      const th = ev.target?.closest?.(COL_TH);
      if (th && ctx.view.dataRows >= ctx.view.firstData) {
        const c = ctx.colOf.get(th.dataset.fieldId);
        if (!c) return;
        const next = ev.shiftKey && sel ? { ...N.single(ctx.view.firstData, sel.ac), cr: ctx.view.dataRows, cc: c } : { ...N.single(ctx.view.firstData, c), cr: ctx.view.dataRows, cc: c };
        return pickMulti(ev, next, "col");
      }
      const rowTd = ev.target?.closest?.(ROW_IDX), rowTr = rowTd?.parentElement;
      if (rowTr?.classList?.contains("base-grid-row")) {
        const item = ctx.win.itemOf(rowTr);
        if (item === undefined || ctx.win.isGroup(item)) return;
        const r = item + 1, next = ev.shiftKey && sel ? { ...N.single(sel.ar, 1), cr: r, cc: ctx.fields.length } : { ...N.single(r, 1), cr: r, cc: ctx.fields.length };
        return pickMulti(ev, next, "row");
      }
      const td = ev.target?.closest?.(CELL) || null, at = td && coordsOf(td);
      if (!at || (editing && editing.td === td)) return;
      const extend = ev.shiftKey && sel, native = !((ev.ctrlKey || ev.metaKey) && !extend) && nativeTarget(ev, at);
      const next = extend ? { ...sel, span: null, cr: at.r, cc: at.c } : N.single(at.r, at.c);
      if (native) { setSel(next, false, []); return; }
      pickMulti(ev, next, "cell");
    }

    function onDragMove(ev) {
      if (!ctx || !drag) return;
      if (drag.kind === "fill") {
        const td = ev.target?.closest?.(CELL), at = td && coordsOf(td);
        if (!at) return;
        const s = drag.src, dv = at.r > s.r2 ? at.r - s.r2 : at.r < s.r1 ? s.r1 - at.r : 0, dh = at.c > s.c2 ? at.c - s.c2 : at.c < s.c1 ? s.c1 - at.c : 0;
        let t = { ...s };
        if (dv > 0 && dv >= dh) t = at.r > s.r2 ? { ...s, r2: at.r } : { ...s, r1: at.r };
        else if (dh > 0) t = at.c > s.c2 ? { ...s, c2: at.c } : { ...s, c1: at.c };
        drag.target = t; paintBuilt(); return;
      }
      if (!sel) return;
      if (drag.kind === "col") {
        const c = ctx.colOf.get(ev.target?.closest?.(COL_TH + "," + CELL)?.dataset?.fieldId);
        if (c && c !== sel.cc) setSel({ ...sel, span: null, cc: c }, false);
        return;
      }
      if (drag.kind === "row") {
        const tr = ev.target?.closest?.("tr.base-grid-row"), item = tr ? ctx.win.itemOf(tr) : undefined;
        if (item !== undefined && !ctx.win.isGroup(item) && item + 1 !== sel.cr) { setSel({ ...sel, span: null, cr: item + 1 }, false); return; }
        const box = ctx.wrap.getBoundingClientRect(), dr = ev.clientY < box.top ? -1 : ev.clientY > box.bottom ? 1 : 0;
        if (dr !== 0) setSel({ ...N.selectionAfter(ctx.view, sel, { t: "move", dr, dc: 0, extend: true }, 1), cc: ctx.fields.length }, true);
        return;
      }
      const td = ev.target?.closest?.(CELL), at = td && coordsOf(td);
      if (at) { if (at.r !== sel.cr || at.c !== sel.cc) setSel({ ...sel, span: null, cr: at.r, cc: at.c }, false); return; }
      const box = ctx.wrap.getBoundingClientRect(), dr = ev.clientY < box.top ? -1 : ev.clientY > box.bottom ? 1 : 0;
      if (dr !== 0) setSel(N.selectionAfter(ctx.view, sel, { t: "move", dr, dc: 0, extend: true }, 1), true);
    }

    function endDrag() {
      const done = drag;
      drag = null;
      doc.removeEventListener("mousemove", onDragMove);
      doc.removeEventListener("mouseup", endDrag);
      if (done?.kind === "fill") {
        const s = done.src, t = done.target;
        paintBuilt();
        if (t.r1 !== s.r1 || t.r2 !== s.r2 || t.c1 !== s.c1 || t.c2 !== s.c2) runFill("handle", t, t);
      }
    }

    function onDblClick(ev) {
      if (ev.target === fillHandle || ev.target?.closest?.(".base-fill-handle")) return onFillDblClick(ev);
      if (ev.target?.closest?.(IGNORE_HDR_TARGET)) return;
      const td = ev.target?.closest?.(CELL);
      if (!td || !sel || editing || ev.target?.closest?.(NATIVE_CONTROL)) return;
      startEdit();
    }

    function onFocusIn(ev) {
      if (!ctx || editing) return;
      if (ev.target === ctx.tbl) { if (!sel && !appendTarget) selectFirst(); return; }
      const td = ev.target?.closest?.(CELL), at = td && coordsOf(td);
      if (!at) return;
      if (!sel || sel.ar !== at.r || sel.ac !== at.c) setSel(N.single(at.r, at.c), false, []);
      if (isTextInput(ev.target)) { ev.target.readOnly = false; beginEdit(td, ev.target); } else if (ev.target.tagName === "SELECT") beginEdit(td, ev.target);
    }

    // ---- lifecycle ----

    function bind(grid, key) {
      endDrag();
      const sameTable = (grid.tableId ?? "") === tableKey, sameView = key === viewKey;
      tableKey = grid.tableId ?? ""; viewKey = key;
      ctx = { ...grid, colOf: new Map(grid.fields.map((f, i) => [f.id, i + 1])), view: viewOf(grid) };
      editing = null;
      if (!sameTable || !sameView) { sel = null; extraRanges = []; savedAnchor = null; appendTarget = false; }
      else if (savedAnchor?.arId) {
        const rm = rowIndexMap(), ar = savedAnchor.wholeCol ? ctx.view.firstData : rm.get(savedAnchor.arId), ac = ctx.colOf.get(savedAnchor.acId);
        if (!ar || !ac || ar > ctx.view.dataRows || ctx.win.isGroup(ar - 1)) { sel = null; extraRanges = []; savedAnchor = null; }
        else {
          const cr = rm.get(savedAnchor.crId) || ar, cc = ctx.colOf.get(savedAnchor.ccId) || ac;
          sel = savedAnchor.wholeCol ? { ...N.single(ctx.view.firstData, ac), cr: ctx.view.dataRows, cc } : { ar, ac, cr, cc, span: sel?.span ? N.rangeOf({ ar, ac, cr, cc }) : null, tabFrom: sel?.tabFrom ?? null };
          extraRanges = savedAnchor.extra.map((ex) => {
            const c1 = ctx.colOf.get(ex.c1Id), c2 = ctx.colOf.get(ex.c2Id);
            if (!c1 || !c2) return null;
            if (ex.wholeCol) return { r1: ctx.view.firstData, r2: ctx.view.dataRows, c1: Math.min(c1, c2), c2: Math.max(c1, c2) };
            const r1 = rm.get(ex.r1Id), r2 = rm.get(ex.r2Id);
            return r1 && r2 ? { r1: Math.min(r1, r2), r2: Math.max(r1, r2), c1: Math.min(c1, c2), c2: Math.max(c1, c2) } : null;
          }).filter(Boolean);
        }
      }
      if (sel && !selectionFits()) { sel = null; extraRanges = []; }
      rememberAnchor();
      const { tbl } = ctx;
      tbl.setAttribute("role", "grid"); tbl.setAttribute("aria-multiselectable", "true"); tbl.setAttribute("tabindex", "0");
      tbl.addEventListener("keydown", onKeyDown); tbl.addEventListener("mousedown", onMouseDown); tbl.addEventListener("dblclick", onDblClick);
      tbl.addEventListener("focusin", onFocusIn); tbl.addEventListener("copy", onCopy); tbl.addEventListener("cut", onCut); tbl.addEventListener("paste", onPaste);
    }

    function beforeRender() { refocus = refocus || Boolean(ctx && ctx.tbl.contains(doc.activeElement)); }
    function afterAttach() { syncFillHandle(); if (!refocus || !ctx) return; refocus = false; ctx.tbl.focus({ preventScroll: true }); }
    function release() { endDrag(); ctx = null; editing = null; }

    function activeTarget() {
      if (!ctx) return null;
      if (editing) leaveEdit(false, false);
      if (!sel) return null;
      const rec = ctx.win.recordAt(sel.ar - 1), fld = ctx.fields[sel.ac - 1];
      return rec && fld ? { recordId: rec.record.id, fieldId: fld.id } : null;
    }

    function selectRecordField(recordId, fieldId) {
      if (!ctx) return false;
      if (editing) leaveEdit(false, false);
      const r = rowIndexMap().get(recordId), c = ctx.colOf.get(fieldId);
      if (!r || !c || ctx.win.isGroup(r - 1)) return false;
      setSel(N.single(r, c), true, []);
      return true;
    }

    return { bind, rowBuilt, beforeRender, afterAttach, release, selection: () => sel, selectedRanges: allRanges, activeTarget, selectRecordField, runClipboardCommand };
  }

  host.__EASYOFFICE_CREATE_BASE_CELL_SELECT__ = createCellSelect;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
