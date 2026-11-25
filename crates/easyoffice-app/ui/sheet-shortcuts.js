(function (host) {
  "use strict";

  // Standard spreadsheet shortcuts (physical `ev.code` for Korean IME safety) and batch Ctrl+Enter fill (PD-356).
  const MAX_BATCH_CELLS = 200000;
  const pad2 = (n) => (n < 10 ? "0" + n : String(n));

  function formatLocalDate(now = new Date()) {
    return now.getFullYear() + "-" + pad2(now.getMonth() + 1) + "-" + pad2(now.getDate());
  }

  function formatLocalTime(now = new Date()) {
    return pad2(now.getHours()) + ":" + pad2(now.getMinutes());
  }

  function insertTextAtEditableCaret(document, el, text) {
    if (!el) return false;
    if (el.tagName === "INPUT" || el.tagName === "TEXTAREA") {
      const start = el.selectionStart ?? el.value.length;
      const end = el.selectionEnd ?? start;
      el.value = el.value.slice(0, start) + text + el.value.slice(end);
      el.selectionStart = el.selectionEnd = start + text.length;
      el.dispatchEvent(new (document.defaultView?.Event || host.Event)("input", { bubbles: true }));
      return true;
    }
    const selObj = document.defaultView?.getSelection?.() || host.getSelection?.();
    if (selObj && selObj.rangeCount > 0 && el.contains(selObj.getRangeAt(0).commonAncestorContainer)) {
      const range = selObj.getRangeAt(0);
      range.deleteContents();
      const node = document.createTextNode(text);
      range.insertNode(node);
      range.setStartAfter(node);
      range.collapse(true);
      selObj.removeAllRanges();
      selObj.addRange(range);
    } else {
      el.textContent = String(el.textContent || "") + text;
    }
    el.dispatchEvent(new (document.defaultView?.Event || host.Event)("input", { bubbles: true }));
    return true;
  }

  function createSheetShortcuts(deps) {
    const {
      document,
      api,
      grid,
      sel,
      getRange,
      getRanges,
      selectAxisSpan,
      focusAnchor,
      discardInlineEdit,
      clip,
      visibleEntryOf,
      openFormatDialog,
      runStructureAct,
      triggerAutoSum,
      feedback,
    } = deps;
    const multiApi = host.__EASYOFFICE_SHEET_MULTI_RANGE__;
    const fillCore = host.__EASYOFFICE_SHEET_FILL_CORE__;
    const cellTypeApi = host.__EASYOFFICE_SHEET_CELL_TYPE__;
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;

    function isImeEvent(ev) {
      return Boolean(ev.isComposing) || ev.key === "Process" || ev.keyCode === 229;
    }

    function insertCurrentStamp(isTime) {
      const value = isTime ? formatLocalTime() : formatLocalDate();
      const ref = grid.refOf(sel.ar, sel.ac);
      const sheet = api.getSheetName();
      if (protectApi?.isOn(sheet) && !protectApi.cellEditable(sheet, sel.ar, sel.ac, api.getEntry(ref)?.style)) {
        protectApi.alertBlocked();
        return Promise.resolve(false);
      }
      return api.applyCellEdits([{ ref, value, formula: "" }], isTime ? "시간 삽입" : "날짜 삽입");
    }

    function fillDirection(direction) {
      const ranges = getRanges();
      const fillApi = host.__EASYOFFICE_SHEET_FILL_CORE__;
      if (!fillApi) return Promise.resolve(false);
      const sheet = api.getSheetName();
      const isDown = direction === "down";
      const allEdits = [];
      const seenRefs = new Set();
      const tables = layoutApi?.tablesOf(sheet) || [];
      const tableFor = (_name, r, c) => tables.find((t) => {
        const b = layoutApi.tableBox(t);
        return r >= b.r1 && r <= b.r2 && c >= b.c1 && c <= b.c2;
      }) || null;

      for (let i = 0; i < ranges.length; i += 1) {
        const rg = ranges[i];
        let source, count;
        if (isDown) {
          if (rg.r2 > rg.r1) {
            source = { r1: rg.r1, c1: rg.c1, r2: rg.r1, c2: rg.c2 };
            count = rg.r2 - rg.r1;
          } else if (rg.r1 > 1) {
            source = { r1: rg.r1 - 1, c1: rg.c1, r2: rg.r1 - 1, c2: rg.c2 };
            count = 1;
          } else {
            continue;
          }
        } else {
          if (rg.c2 > rg.c1) {
            source = { r1: rg.r1, c1: rg.c1, r2: rg.r2, c2: rg.c1 };
            count = rg.c2 - rg.c1;
          } else if (rg.c1 > 1) {
            source = { r1: rg.r1, c1: rg.c1 - 1, r2: rg.r2, c2: rg.c1 - 1 };
            count = 1;
          } else {
            continue;
          }
        }
        const plan = fillCore.planFill({
          source,
          direction,
          count,
          mode: "copy",
          cellAt: (r, c) => visibleEntryOf(grid.refOf(r, c)),
          shiftFormula: clip.shiftFormula,
          refOf: (r, c) => grid.refOf(r, c),
          tableFor,
        });
        if (plan.refused) {
          feedback("채우기 범위가 너무 큼");
          return Promise.resolve(false);
        }
        for (let k = 0; k < plan.edits.length; k += 1) {
          const ed = plan.edits[k];
          if (seenRefs.has(ed.ref)) continue;
          seenRefs.add(ed.ref);
          allEdits.push(ed);
          if (allEdits.length > MAX_BATCH_CELLS) {
            feedback("채우기 범위가 너무 큼");
            return Promise.resolve(false);
          }
        }
      }
      if (allEdits.length === 0) return Promise.resolve(false);
      return api.applyCellEdits(allEdits, isDown ? "아래쪽 채우기" : "오른쪽 채우기");
    }

    function applyBatchEntry(rawText, nf, td) {
      const ranges = getRanges();
      const single = ranges.length === 1 && ranges[0].r1 === ranges[0].r2 && ranges[0].c1 === ranges[0].c2;
      if (single) return false;
      const parsed = cellTypeApi.parseTypedInput(rawText, nf);
      const isFormula = Boolean(parsed.formula);
      const edits = [];
      const res = multiApi.forEachUniqueCoord(ranges, grid.packCoord, (r, c) => {
        const ref = grid.refOf(r, c);
        if (isFormula) {
          const dr = r - sel.ar;
          const dc = c - sel.ac;
          const formula = dr === 0 && dc === 0 ? parsed.formula : clip.shiftFormula(parsed.formula, dr, dc);
          edits.push({ ref, value: "", formula });
        } else {
          const ed = { ref, value: parsed.value, formula: "" };
          if (parsed.t) ed.t = parsed.t;
          edits.push(ed);
        }
      }, MAX_BATCH_CELLS);
      if (res.refused) {
        if (td) discardInlineEdit(td);
        feedback("선택 범위가 너무 큼");
        return true;
      }
      if (td) discardInlineEdit(td);
      void api.applyCellEdits(edits, "일괄 입력").then(() => focusAnchor()).catch(() => {});
      return true;
    }

    function handleKey(ev, editing, targetTd) {
      if (isImeEvent(ev)) return false;
      const mod = (ev.ctrlKey || ev.metaKey) && !ev.altKey;
      const code = ev.code;

      if (editing) {
        if (mod && !ev.shiftKey && code === "Enter") {
          const ranges = getRanges();
          const rg = ranges[0];
          if (ranges.length === 1 && rg.r1 === rg.r2 && rg.c1 === rg.c2) return false;
          ev.preventDefault();
          ev.stopPropagation?.();
          const td = targetTd || document.activeElement;
          const raw = td ? grid.readTdValue(td) : "";
          return applyBatchEntry(raw, td?._nf || "", td);
        }
        if (mod && code === "Semicolon") {
          ev.preventDefault();
          ev.stopPropagation?.();
          const td = targetTd || document.activeElement;
          insertTextAtEditableCaret(document, td, ev.shiftKey ? formatLocalTime() : formatLocalDate());
          return true;
        }
        return false;
      }

      if (mod && !ev.shiftKey && code === "Digit1") {
        ev.preventDefault();
        ev.stopPropagation?.();
        openFormatDialog?.();
        return true;
      }
      if (mod && !ev.shiftKey && code === "KeyD") {
        ev.preventDefault();
        ev.stopPropagation?.();
        void fillDirection("down").catch(() => {});
        return true;
      }
      if (mod && !ev.shiftKey && code === "KeyR") {
        ev.preventDefault();
        ev.stopPropagation?.();
        void fillDirection("right").catch(() => {});
        return true;
      }
      if (mod && code === "Semicolon") {
        ev.preventDefault();
        ev.stopPropagation?.();
        void insertCurrentStamp(Boolean(ev.shiftKey)).catch(() => {});
        return true;
      }
      if (ev.altKey && !ev.ctrlKey && !ev.metaKey && !ev.shiftKey && code === "Equal") {
        ev.preventDefault();
        ev.stopPropagation?.();
        triggerAutoSum?.();
        return true;
      }
      if (ev.ctrlKey && !ev.metaKey && !ev.altKey && !ev.shiftKey && code === "Space") {
        ev.preventDefault();
        ev.stopPropagation?.();
        const rg = getRange();
        selectAxisSpan("col", rg.c1, rg.c2);
        return true;
      }
      if (ev.shiftKey && !ev.ctrlKey && !ev.metaKey && !ev.altKey && code === "Space") {
        ev.preventDefault();
        ev.stopPropagation?.();
        const rg = getRange();
        selectAxisSpan("row", rg.r1, rg.r2);
        return true;
      }
      if (mod && ev.shiftKey && code === "KeyL") {
        ev.preventDefault();
        ev.stopPropagation?.();
        runStructureAct?.("filter-toggle");
        return true;
      }
      return false;
    }

    function handleBarKey(ev) {
      if (isImeEvent(ev)) return false;
      const mod = (ev.ctrlKey || ev.metaKey) && !ev.altKey;
      const input = ev.target;
      if (mod && ev.code === "Semicolon") {
        ev.preventDefault();
        insertTextAtEditableCaret(document, input, ev.shiftKey ? formatLocalTime() : formatLocalDate());
        return true;
      }
      if (mod && !ev.shiftKey && ev.code === "Enter") {
        const ranges = getRanges();
        const rg = ranges[0];
        if (ranges.length === 1 && rg.r1 === rg.r2 && rg.c1 === rg.c2) return false;
        ev.preventDefault();
        ev.stopPropagation?.();
        const raw = String(input?.value ?? "");
        const anchorEntry = api.getEntry(grid.refOf(sel.ar, sel.ac));
        const ok = applyBatchEntry(raw, anchorEntry?.style?.numFmt || "", null);
        if (ok) input?.blur?.();
        return ok;
      }
      return false;
    }

    return { handleKey, handleBarKey, fillDirection, insertCurrentStamp };
  }

  host.__EASYOFFICE_CREATE_SHEET_SHORTCUTS__ = createSheetShortcuts;
  host.__EASYOFFICE_SHEET_SHORTCUT_STAMPS__ = { formatLocalDate, formatLocalTime };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
