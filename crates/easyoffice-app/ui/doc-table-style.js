(function (host) {
  "use strict";

  // Table cell borders, shading, column widths (`표/셀 속성`, `셀 너비를 같게`) and column-border drag
  // for `table.doc-inline-table` in `#doc-editor`. Dialog structure and Korean labels follow the
  // reference editor (`셀 테두리/배경`, `표/셀 속성`, `셀 너비를 같게`; MIT, Copyright (c) 2025-2026
  // Edward Kim, see THIRD_PARTY_NOTICES.md) and mirror `#sheet-numfmt-dialog` (`sheet-look-ui.js`).

  const TWIPS_PER_PX = 15;
  const TWIPS_PER_MM = 1440 / 25.4;
  const MIN_DRAG_PX = 24;
  const DEFAULT_COL_TWIPS = 2160;
  const MIN_PROPS_MM = 2.5;
  const MAX_PROPS_MM = 500;
  const BORDER_HIT_PX = 5;
  const HEX6_RE = /^#?([0-9a-fA-F]{6})$/;
  const LINE_SET = new Set(["thin", "thick", "dashed", "dotted", "double"]);
  const CSS_BORDER_PREFIX = { thin: "1px solid", thick: "2.5px solid", dashed: "1.5px dashed", dotted: "1.5px dotted", double: "3px double" };
  const DASH = { thin: "", thick: "", dashed: "6 3", dotted: "1.5 2.5", double: "" };
  const STROKE = { thin: 1, thick: 2.5, dashed: 1.5, dotted: 1.5, double: 3 };
  const POSITIONS = { t: [8, 8, 112, 8], b: [8, 64, 112, 64], l: [8, 8, 8, 64], r: [112, 8, 112, 64], h: [8, 36, 112, 36], v: [60, 8, 60, 64] };
  const PRESETS = { none: [], outer: ["t", "b", "l", "r"], inner: ["h", "v"], all: ["t", "b", "l", "r", "h", "v"] };
  const SIDE_INDEX = { top: 0, right: 1, bottom: 2, left: 3 };
  const SIDE_PROPS = ["borderTop", "borderRight", "borderBottom", "borderLeft"];
  const SVG_NS = "http://www.w3.org/2000/svg";
  const COMMAND_LABELS = { "table-border-bg": "셀 테두리/배경", "table-cell-props": "표/셀 속성", "table-col-equal": "셀 너비를 같게" };
  const round1 = (v) => Math.round(v * 10) / 10;
  const normalizeHex6 = (raw) => HEX6_RE.exec(String(raw || "").trim())?.[1]?.toUpperCase() ?? "";

  function normalizeEdgeSlot(raw) {
    if (raw && typeof raw === "object") {
      const k = String(raw.kind || raw.line || "").trim().toLowerCase();
      if (!k || k === "-" || k === "unspecified") return "-";
      if (k === "none") return "none";
      const kind = k === "solid" ? "thin" : k;
      const rgb = normalizeHex6(raw.color || "000000");
      return LINE_SET.has(kind) && rgb ? kind + ":" + rgb : "-";
    }
    const s = String(raw || "").trim();
    if (!s || s === "-" || s === "none") return s === "none" ? "none" : "-";
    const [kindRaw, colorRaw] = s.split(":");
    const kind = kindRaw === "solid" ? "thin" : String(kindRaw || "").toLowerCase();
    const rgb = normalizeHex6(colorRaw);
    return LINE_SET.has(kind) && rgb ? kind + ":" + rgb : "-";
  }

  function edgeToCss(slot) {
    const norm = normalizeEdgeSlot(slot);
    if (norm === "-" || norm === "none") return norm === "none" ? "none" : "";
    const [kind, rgb] = norm.split(":");
    return CSS_BORDER_PREFIX[kind] + " #" + rgb;
  }

  const parseBordersAttr = (raw) => {
    const parts = String(raw || "").split(",");
    return [0, 1, 2, 3].map((i) => normalizeEdgeSlot(parts[i]));
  };

  function writeCellBorders(cell, slots) {
    const clean = [0, 1, 2, 3].map((i) => normalizeEdgeSlot(slots[i]));
    if (clean.every((s) => s === "-")) {
      delete cell.dataset.eoBorders;
      SIDE_PROPS.forEach((p) => { cell.style[p] = ""; });
      return;
    }
    cell.dataset.eoBorders = clean.join(",");
    clean.forEach((s, i) => { cell.style[SIDE_PROPS[i]] = edgeToCss(s); });
  }

  function setCellEdge(cell, side, value) {
    const slots = parseBordersAttr(cell.dataset?.eoBorders);
    slots[SIDE_INDEX[side]] = normalizeEdgeSlot(value);
    writeCellBorders(cell, slots);
  }

  function writeCellFill(cell, hexOrEmpty) {
    const rgb = normalizeHex6(hexOrEmpty);
    if (!rgb) { delete cell.dataset.eoFill; cell.style.backgroundColor = ""; return; }
    cell.dataset.eoFill = rgb;
    cell.style.backgroundColor = "#" + rgb;
  }

  function svgEl(doc, tag, attrs) {
    const el = doc.createElementNS(SVG_NS, tag);
    for (const [k, v] of Object.entries(attrs)) if (v !== "") el.setAttribute(k, String(v));
    return el;
  }

  function createDocTableStyle(deps) {
    const { document, grid, selectionAt, rectOf, placeCaret, withUndo, setFeedback, captureWorkspaceSnapshot, commitDeferredUndoSnapshot, saveDocState } = deps;
    let controlsReady = false;
    let activeSel = null;
    let initialEdges = {};
    let wantEdges = {};
    let borderLine = "thin";
    let borderColor = "000000";
    let borderTouched = false;
    let wantFillMode = "unchanged";
    let bgColor = "FFFFFF";

    const $ = (id) => document.getElementById(id);
    const editor = () => $("doc-editor");

    function notifyMutated() {
      saveDocState?.();
      const root = editor();
      if (root && typeof host.Event === "function") root.dispatchEvent(new host.Event("eo-doc-mutated", { bubbles: true }));
    }

    function paintSwatch(hostId, rgb, sampleId) {
      $(hostId)?.querySelector?.(".dialog-swatch-chip")?.style?.setProperty("--dialog-swatch", rgb ? "#" + rgb : "");
      if (sampleId) $(sampleId)?.style?.setProperty("--sample-fill", rgb ? "#" + rgb : "");
    }

    function lineSvg(name) {
      const svg = svgEl(document, "svg", { viewBox: "0 0 48 10", "aria-hidden": "true" });
      const sw = name === "double" ? 1 : name === "thick" ? 2.5 : 1.5;
      for (const y of name === "double" ? [3, 7] : [5]) {
        svg.appendChild(svgEl(document, "line", { x1: 0, x2: 48, y1: y, y2: y, stroke: "currentColor", "stroke-width": sw, "stroke-dasharray": DASH[name] }));
      }
      return svg;
    }

    function drawBorderPreview() {
      const svg = $("doc-cell-border-preview");
      if (!svg) return;
      svg.replaceChildren(svgEl(document, "rect", { x: 8, y: 8, width: 104, height: 56, class: "sheet-border-guide" }));
      const sw = borderLine === "double" ? 1 : STROKE[borderLine];
      for (const [pos, [x1, y1, x2, y2]] of Object.entries(POSITIONS)) {
        if (wantEdges[pos] !== true) continue;
        const horiz = y1 === y2;
        for (const s of borderLine === "double" ? [-1.5, 1.5] : [0]) {
          svg.appendChild(svgEl(document, "line", {
            x1: x1 + (horiz ? 0 : s), x2: x2 + (horiz ? 0 : s), y1: y1 + (horiz ? s : 0), y2: y2 + (horiz ? s : 0),
            stroke: "#" + borderColor, "stroke-width": sw, "stroke-dasharray": DASH[borderLine],
          }));
        }
      }
      for (const btn of document.querySelectorAll("#doc-cell-border-dialog [data-border-pos]")) {
        btn.setAttribute("aria-pressed", String(wantEdges[btn.dataset.borderPos] === true));
      }
      for (const item of document.querySelectorAll("#doc-cell-border-dialog [data-border-line]")) {
        const on = item.dataset.borderLine === borderLine;
        item.classList.toggle("active", on);
        item.setAttribute("aria-checked", String(on));
      }
    }

    function showBorderPane(name) {
      for (const tab of document.querySelectorAll("#doc-cell-border-dialog [data-doc-cb-tab]")) {
        const on = tab.dataset.docCbTab === name;
        tab.classList.toggle("active", on);
        tab.setAttribute("aria-selected", String(on));
        tab.tabIndex = on ? 0 : -1;
      }
      for (const pane of document.querySelectorAll("#doc-cell-border-dialog [data-doc-cb-pane]")) {
        pane.hidden = pane.dataset.docCbPane !== name;
      }
    }

    function readSelectionCells(layout, rect) {
      const cells = new Set();
      for (let r = rect.top; r <= rect.bottom; r += 1) {
        for (let c = rect.left; c <= rect.right; c += 1) if (layout.slots[r]?.[c]) cells.add(layout.slots[r][c]);
      }
      return Array.from(cells);
    }

    function inspectEdge(cells, layout, predicate, sideIdx) {
      let seen = null;
      for (const cell of cells) {
        const pos = layout.at.get(cell);
        if (!pos || !predicate(pos)) continue;
        const slot = parseBordersAttr(cell.dataset?.eoBorders)[sideIdx];
        if (seen === null) seen = slot;
        else if (seen !== slot) return "mixed";
      }
      return seen ?? "-";
    }

    function loadBorderBgDialog(sel) {
      const layout = grid.layoutOf(sel.table);
      const rect = rectOf(sel);
      const cells = readSelectionCells(layout, rect);
      initialEdges = {
        t: inspectEdge(cells, layout, (p) => p.row === rect.top, 0),
        r: inspectEdge(cells, layout, (p) => p.col + p.colSpan - 1 === rect.right, 1),
        b: inspectEdge(cells, layout, (p) => p.row + p.rowSpan - 1 === rect.bottom, 2),
        l: inspectEdge(cells, layout, (p) => p.col === rect.left, 3),
        h: rect.bottom > rect.top ? inspectEdge(cells, layout, (p) => p.row + p.rowSpan - 1 < rect.bottom, 2) : "-",
        v: rect.right > rect.left ? inspectEdge(cells, layout, (p) => p.col + p.colSpan - 1 < rect.right, 1) : "-",
      };
      wantEdges = {};
      let firstCustom = null;
      for (const pos of Object.keys(POSITIONS)) {
        const st = initialEdges[pos];
        if (st === "none") wantEdges[pos] = false;
        else if (st !== "-" && st !== "mixed") { wantEdges[pos] = true; firstCustom ??= st; }
      }
      [borderLine, borderColor] = firstCustom ? firstCustom.split(":") : ["thin", "000000"];
      borderTouched = false;
      const bInput = $("doc-cell-border-color");
      if (bInput) bInput.value = "#" + borderColor.toLowerCase();
      paintSwatch("doc-cell-border-color-host", borderColor);
      drawBorderPreview();

      const fills = new Set(cells.map((c) => normalizeHex6(c.dataset?.eoFill)));
      const loadedFill = fills.size === 1 ? [...fills][0] : "mixed";
      const hasSolid = Boolean(loadedFill && loadedFill !== "mixed");
      wantFillMode = "unchanged";
      bgColor = hasSolid ? loadedFill : "FFFFFF";
      if ($("doc-cell-bg-none")) $("doc-cell-bg-none").checked = loadedFill === "";
      if ($("doc-cell-bg-solid")) $("doc-cell-bg-solid").checked = hasSolid;
      if ($("doc-cell-bg-color")) $("doc-cell-bg-color").value = "#" + bgColor.toLowerCase();
      paintSwatch("doc-cell-bg-color-host", hasSolid ? bgColor : "", "doc-cell-bg-sample");
      showBorderPane("border");
    }

    function applyBordersToTable(layout, rect, edgeSpec) {
      const val = (flag) => (flag === true ? edgeSpec : "none");
      for (const [cell, pos] of layout.at) {
        const cBot = pos.row + pos.rowSpan - 1;
        const cRight = pos.col + pos.colSpan - 1;
        if (pos.row >= rect.top && cBot <= rect.bottom && pos.col >= rect.left && cRight <= rect.right) {
          for (const [side, flag] of [
            ["top", pos.row === rect.top ? wantEdges.t : wantEdges.h],
            ["bottom", cBot === rect.bottom ? wantEdges.b : wantEdges.h],
            ["left", pos.col === rect.left ? wantEdges.l : wantEdges.v],
            ["right", cRight === rect.right ? wantEdges.r : wantEdges.v],
          ]) if (flag !== undefined) setCellEdge(cell, side, val(flag));
          continue;
        }
        if (wantEdges.t !== undefined && cBot === rect.top - 1 && pos.col >= rect.left && cRight <= rect.right) setCellEdge(cell, "bottom", val(wantEdges.t));
        if (wantEdges.b !== undefined && pos.row === rect.bottom + 1 && pos.col >= rect.left && cRight <= rect.right) setCellEdge(cell, "top", val(wantEdges.b));
        if (wantEdges.l !== undefined && cRight === rect.left - 1 && pos.row >= rect.top && cBot <= rect.bottom) setCellEdge(cell, "right", val(wantEdges.l));
        if (wantEdges.r !== undefined && pos.col === rect.right + 1 && pos.row >= rect.top && cBot <= rect.bottom) setCellEdge(cell, "left", val(wantEdges.r));
      }
    }

    function confirmBorderBgDialog() {
      const dlg = $("doc-cell-border-dialog");
      const sel = activeSel;
      const shouldWriteFill = wantFillMode !== "unchanged";
      if (!sel || !editor()?.contains(sel.table) || (!borderTouched && !shouldWriteFill)) {
        dlg?.close?.();
        return;
      }
      withUndo(() => {
        const layout = grid.layoutOf(sel.table);
        const rect = rectOf(sel);
        if (borderTouched) applyBordersToTable(layout, rect, borderLine + ":" + borderColor);
        if (shouldWriteFill) {
          const nextFill = wantFillMode === "solid" ? bgColor : "";
          for (const cell of readSelectionCells(layout, rect)) writeCellFill(cell, nextFill);
        }
        return { changed: true, cell: sel.from };
      });
      dlg?.close?.();
      placeCaret(sel.from);
      setFeedback?.(COMMAND_LABELS["table-border-bg"] + " 완료");
    }

    function openTablePropsDialog(sel) {
      const dlg = $("doc-table-props-dialog");
      const input = $("doc-table-col-width");
      if (!dlg || !input) return;
      activeSel = sel;
      const layout = grid.layoutOf(sel.table);
      const rect = rectOf(sel);
      const existingCols = grid.colsOf(grid.findColgroup(sel.table));
      const twips = Number(existingCols[rect.left]?.dataset?.eoColTwips) || grid.measureDefaultColTwips(sel.table, layout)[rect.left] || DEFAULT_COL_TWIPS;
      input.value = String(round1(twips / TWIPS_PER_MM));
      if (!dlg.open) dlg.showModal?.();
    }

    function confirmTablePropsDialog() {
      const dlg = $("doc-table-props-dialog");
      const input = $("doc-table-col-width");
      const sel = activeSel;
      if (!sel || !editor()?.contains(sel.table) || !input) return;
      const mm = Number(input.value);
      if (!Number.isFinite(mm) || mm < MIN_PROPS_MM || mm > MAX_PROPS_MM) {
        setFeedback?.("셀 너비는 2.5mm 이상 500mm 이하여야 합니다");
        return;
      }
      const twips = Math.round(mm * TWIPS_PER_MM);
      withUndo(() => {
        const layout = grid.layoutOf(sel.table);
        const rect = rectOf(sel);
        const cols = grid.ensureColgroup(sel.table, layout);
        for (let c = rect.left; c <= Math.min(rect.right, cols.length - 1); c += 1) cols[c].dataset.eoColTwips = String(twips);
        grid.syncTableWidthFromCols(sel.table);
        return { changed: true, cell: sel.from };
      });
      dlg?.close?.();
      placeCaret(sel.from);
      setFeedback?.(COMMAND_LABELS["table-cell-props"] + " 완료");
    }

    function hitColumnBorder(ev, root) {
      const cell = ev.target?.closest?.("td, th");
      const table = cell?.closest?.("table.doc-inline-table");
      if (!table || !root.contains(table)) return null;
      const layout = grid.layoutOf(table);
      const pos = layout.at.get(cell);
      const rect = cell.getBoundingClientRect?.();
      const x = Number(ev.clientX);
      if (!pos || !rect || !(rect.width > 0) || !Number.isFinite(x)) return null;
      const rightEdge = rect.right !== undefined ? rect.right : rect.left + rect.width;
      if (Math.abs(x - rightEdge) <= BORDER_HIT_PX) return { table, layout, col: pos.col + pos.colSpan - 1 };
      return pos.col > 0 && Math.abs(x - rect.left) <= BORDER_HIT_PX ? { table, layout, col: pos.col - 1 } : null;
    }

    function onEditorPointerDown(ev) {
      if (ev.button !== undefined && ev.button !== 0) return;
      const root = editor();
      const hit = root ? hitColumnBorder(ev, root) : null;
      if (!hit) return;
      ev.preventDefault?.();
      ev.stopPropagation?.();
      const preSnap = captureWorkspaceSnapshot?.(false) ?? null;
      const existedBefore = Boolean(grid.findColgroup(hit.table));
      const targetCol = grid.ensureColgroup(hit.table, hit.layout)[hit.col];
      if (!targetCol) return;
      const startTwips = Number(targetCol.dataset.eoColTwips) || DEFAULT_COL_TWIPS;
      const startPx = Math.max(MIN_DRAG_PX, Math.round(startTwips / TWIPS_PER_PX));
      const startX = Number(ev.clientX) || 0;
      let nextTwips = startTwips;

      const onMove = (moveEv) => {
        nextTwips = Math.max(MIN_DRAG_PX, Math.round(startPx + ((Number(moveEv.clientX) || 0) - startX))) * TWIPS_PER_PX;
        targetCol.dataset.eoColTwips = String(nextTwips);
        grid.syncTableWidthFromCols(hit.table);
      };
      const onUp = () => {
        document.removeEventListener("pointermove", onMove);
        document.removeEventListener("pointerup", onUp);
        if (nextTwips !== startTwips) {
          notifyMutated();
          if (preSnap) commitDeferredUndoSnapshot?.(preSnap);
          setFeedback?.(COMMAND_LABELS["table-cell-props"] + " 완료");
        } else if (!existedBefore) {
          grid.findColgroup(hit.table)?.remove();
          hit.table.style.tableLayout = "";
          hit.table.style.width = "";
        }
      };
      document.addEventListener("pointermove", onMove);
      document.addEventListener("pointerup", onUp);
    }

    function setBgChoice(mode, rgb) {
      wantFillMode = mode;
      if ($("doc-cell-bg-none")) $("doc-cell-bg-none").checked = mode === "none";
      if ($("doc-cell-bg-solid")) $("doc-cell-bg-solid").checked = mode === "solid";
      paintSwatch("doc-cell-bg-color-host", mode === "solid" ? rgb : "", "doc-cell-bg-sample");
    }

    function ensureControls() {
      if (controlsReady) return;
      controlsReady = true;
      document.querySelectorAll("#doc-cell-border-dialog [data-doc-cb-tab]").forEach((tab) => {
        tab.addEventListener("click", () => showBorderPane(tab.dataset.docCbTab));
      });
      document.querySelectorAll("#doc-cell-border-dialog [data-border-line]").forEach((item) => {
        item.replaceChildren(lineSvg(item.dataset.borderLine));
        item.addEventListener("click", () => {
          if (!LINE_SET.has(item.dataset.borderLine)) return;
          borderLine = item.dataset.borderLine;
          borderTouched = true;
          drawBorderPreview();
        });
      });
      document.querySelectorAll("#doc-cell-border-dialog [data-border-preset]").forEach((btn) => {
        btn.addEventListener("click", () => {
          const name = btn.dataset.borderPreset;
          const active = PRESETS[name] || [];
          for (const pos of Object.keys(POSITIONS)) {
            wantEdges[pos] = name === "none" ? false : active.includes(pos) ? true : (initialEdges[pos] === "none" ? false : undefined);
          }
          borderTouched = true;
          drawBorderPreview();
        });
      });
      document.querySelectorAll("#doc-cell-border-dialog [data-border-pos]").forEach((btn) => {
        btn.addEventListener("click", () => {
          wantEdges[btn.dataset.borderPos] = wantEdges[btn.dataset.borderPos] !== true;
          borderTouched = true;
          drawBorderPreview();
        });
      });
      const picker = host.__EASYOFFICE_COLOR_PICKER__;
      const applyBorderColor = (picked) => {
        const rgb = normalizeHex6(picked);
        if (rgb) { borderColor = rgb; borderTouched = true; paintSwatch("doc-cell-border-color-host", rgb); drawBorderPreview(); }
      };
      const applyBgColor = (picked) => {
        if (picked === "") { setBgChoice("none", ""); return; }
        const rgb = normalizeHex6(picked);
        if (rgb) { bgColor = rgb; setBgChoice("solid", rgb); }
      };
      if (picker && $("doc-cell-border-color-host")) picker.setupColorPicker($("doc-cell-border-color-host"), applyBorderColor);
      else $("doc-cell-border-color")?.addEventListener("change", (ev) => applyBorderColor(ev.target.value));
      if (picker && $("doc-cell-bg-color-host")) picker.setupColorPicker($("doc-cell-bg-color-host"), applyBgColor, { clear: true, clearLabel: "채우기 없음" });
      else $("doc-cell-bg-color")?.addEventListener("change", (ev) => applyBgColor(ev.target.value));
      $("doc-cell-bg-none")?.addEventListener("change", () => { if ($("doc-cell-bg-none").checked) setBgChoice("none", ""); });
      $("doc-cell-bg-solid")?.addEventListener("change", () => { if ($("doc-cell-bg-solid").checked) setBgChoice("solid", bgColor); });
      $("btn-doc-cell-border-ok")?.addEventListener("click", confirmBorderBgDialog);
      $("btn-doc-table-props-ok")?.addEventListener("click", confirmTablePropsDialog);
      for (const id of ["doc-cell-border-dialog", "doc-table-props-dialog"]) {
        $(id)?.querySelectorAll?.("[data-dialog-close]").forEach((b) => b.addEventListener("click", () => $(id).close?.()));
      }
    }

    function bind() {
      ensureControls();
      const root = editor();
      root?.addEventListener("pointerdown", onEditorPointerDown);
      root?.addEventListener("pointermove", (ev) => { if (root.style) root.style.cursor = hitColumnBorder(ev, root) ? "col-resize" : ""; });
    }

    const handles = (cmd) => Object.hasOwn(COMMAND_LABELS, cmd);

    function run(cmd) {
      const sel = selectionAt();
      if (!sel) { setFeedback?.("표 안에서 사용"); return { changed: false }; }
      ensureControls();
      if (cmd === "table-border-bg") {
        activeSel = sel;
        loadBorderBgDialog(sel);
        const dlg = $("doc-cell-border-dialog");
        if (dlg && !dlg.open) dlg.showModal?.();
        return { changed: false, openedDialog: true };
      }
      if (cmd === "table-cell-props") { openTablePropsDialog(sel); return { changed: false, openedDialog: true }; }
      if (cmd === "table-col-equal") {
        const res = withUndo(() => grid.equalizeColumns(sel.table, sel.from, sel.to));
        if (res.changed) { placeCaret(res.cell ?? sel.from); setFeedback?.(COMMAND_LABELS[cmd] + " 완료"); }
        return res;
      }
      return { changed: false };
    }

    return { bind, handles, run };
  }

  host.__EASYOFFICE_DOC_TABLE_STYLE__ = { normalizeHex6, normalizeEdgeSlot, edgeToCss, parseBordersAttr, writeCellBorders, writeCellFill };
  host.__EASYOFFICE_CREATE_DOC_TABLE_STYLE__ = createDocTableStyle;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
