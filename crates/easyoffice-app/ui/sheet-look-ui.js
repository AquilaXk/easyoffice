(function (host) {
  "use strict";

  // The 맞춤 and 테두리 panes of the 셀 서식 dialog (PD-207). The dialog is the number format dialog of
  // sheet-numfmt-ui.js with tabs; this module fills the other two panes from the selection and writes them
  // back in one undoable step. The border pane follows the reference editor's cell border dialog
  // (`cell-border-bg-dialog.ts`): a grid of line kinds, a color, preset buttons and the positions around a
  // preview of the cell.

  const H_OPTIONS = [["", "기본"], ["left", "왼쪽"], ["center", "가운데"], ["right", "오른쪽"]];
  const V_OPTIONS = [["top", "위쪽"], ["middle", "가운데"], ["bottom", "아래쪽"]];
  const MIXED = [["mixed", "혼합"]];
  // Position -> [plan kind, preview line [x1, y1, x2, y2] in a 120 x 72 box].
  const POSITIONS = {
    t: ["top", [8, 8, 112, 8]],
    b: ["bottom", [8, 64, 112, 64]],
    l: ["left", [8, 8, 8, 64]],
    r: ["right", [112, 8, 112, 64]],
    h: ["innerH", [8, 36, 112, 36]],
    v: ["innerV", [60, 8, 60, 64]],
  };
  const PRESETS = { none: [], outer: ["t", "b", "l", "r"], inner: ["h", "v"], all: ["t", "b", "l", "r", "h", "v"] };
  const DASH = { thin: "", thick: "", dashed: "6 3", dotted: "1.5 2.5", double: "" };
  const STROKE = { thin: 1, thick: 2.5, dashed: 1.5, dotted: 1.5, double: 3 };
  const SVG_NS = "http://www.w3.org/2000/svg";

  function createSheetLookUi(deps) {
    const { document, kit, ops, getRange } = deps;
    const look = host.__EASYOFFICE_SHEET_LOOK__;
    const { $, val, checked, setVal, setChecked, setSelect, fillOptions, fireChange } = kit;
    let ready = false;
    let loaded = { h: "", v: "bottom", wrap: "" };
    let initial = {};
    let want = {};
    let line = "thin";
    let color = look.DEFAULT_COLOR;
    let borderTouched = false;

    const tabs = () => [...document.querySelectorAll("#sheet-numfmt-dialog [data-fmt-tab]")];
    const panes = () => [...document.querySelectorAll("#sheet-numfmt-dialog [data-fmt-pane]")];

    function showPane(name) {
      for (const tab of tabs()) {
        const on = tab.dataset.fmtTab === name;
        tab.setAttribute("aria-selected", String(on));
        tab.classList.toggle("active", on);
        tab.tabIndex = on ? 0 : -1;
      }
      for (const pane of panes()) pane.hidden = pane.dataset.fmtPane !== name;
    }

    function setOptions(id, pairs, value) {
      const el = $(id);
      if (!el) return;
      el.replaceChildren();
      for (const [v, label] of pairs) {
        const opt = document.createElement("option");
        opt.value = v;
        opt.textContent = label;
        el.appendChild(opt);
      }
      el.value = value;
      fireChange(el);
    }

    // ---- preview -------------------------------------------------------------------------------------
    function drawPreview() {
      const svg = $("sheet-border-preview");
      if (!svg) return;
      svg.replaceChildren();
      const grid = document.createElementNS(SVG_NS, "rect");
      grid.setAttribute("x", "8");
      grid.setAttribute("y", "8");
      grid.setAttribute("width", "104");
      grid.setAttribute("height", "56");
      grid.setAttribute("class", "sheet-border-guide");
      svg.appendChild(grid);
      for (const [pos, [, [x1, y1, x2, y2]]] of Object.entries(POSITIONS)) {
        if (want[pos] !== true) continue;
        const parts = line === "double" ? [-1.5, 1.5] : [0];
        for (const shift of parts) {
          const el = document.createElementNS(SVG_NS, "line");
          const horizontal = y1 === y2;
          el.setAttribute("x1", String(x1 + (horizontal ? 0 : shift)));
          el.setAttribute("x2", String(x2 + (horizontal ? 0 : shift)));
          el.setAttribute("y1", String(y1 + (horizontal ? shift : 0)));
          el.setAttribute("y2", String(y2 + (horizontal ? shift : 0)));
          el.setAttribute("stroke", "#" + color);
          el.setAttribute("stroke-width", String(line === "double" ? 1 : STROKE[line]));
          if (DASH[line]) el.setAttribute("stroke-dasharray", DASH[line]);
          svg.appendChild(el);
        }
      }
      for (const btn of document.querySelectorAll("#sheet-numfmt-dialog [data-border-pos]")) {
        btn.setAttribute("aria-pressed", String(want[btn.dataset.borderPos] === true));
      }
      for (const item of document.querySelectorAll("#sheet-numfmt-dialog [data-border-line]")) {
        const on = item.dataset.borderLine === line;
        item.classList.toggle("active", on);
        item.setAttribute("aria-checked", String(on));
      }
    }

    function touchBorder() {
      borderTouched = true;
      drawPreview();
    }

    function togglePosition(pos) {
      want[pos] = want[pos] !== true;
      touchBorder();
    }

    function applyPreset(name) {
      for (const pos of Object.keys(POSITIONS)) want[pos] = PRESETS[name].includes(pos);
      touchBorder();
    }

    function chooseLine(next) {
      if (!look.LINES.includes(next)) return;
      line = next;
      touchBorder();
    }

    // ---- load and apply ------------------------------------------------------------------------------
    function loadBorder(rg) {
      initial = ops.borderState(rg);
      want = {};
      let edge = null;
      for (const pos of Object.keys(POSITIONS)) {
        const e = initial[pos];
        want[pos] = e === "" ? false : e === "mixed" ? undefined : true;
        if (!edge && e && e !== "mixed") edge = look.parseEdge(e);
      }
      line = edge?.line || ops.pen.line;
      color = edge?.color || ops.pen.color;
      setVal("sheet-border-color", "#" + color);
      borderTouched = false;
      drawPreview();
    }

    function loadAlign(rg) {
      const h = ops.commonValue(ops.readers.textAlign, rg);
      const v = ops.commonValue(ops.readers.verticalAlign, rg);
      const wrap = ops.commonValue(ops.readers.wrap, rg);
      loaded = { h, v: v === "" ? "bottom" : v, wrap };
      setOptions("sheet-align-h-select", h === "mixed" ? [...MIXED, ...H_OPTIONS] : H_OPTIONS, h);
      setOptions("sheet-align-v-select", v === "mixed" ? [...MIXED, ...V_OPTIONS] : V_OPTIONS, loaded.v);
      const box = $("sheet-align-wrap");
      if (box) box.indeterminate = wrap === "mixed";
      setChecked("sheet-align-wrap", wrap === "1");
    }

    // `open(pane)`: loads both panes from the selection and shows `pane`.
    function open(pane) {
      ensureControls();
      const rg = getRange();
      loadAlign(rg);
      loadBorder(rg);
      showPane(pane);
    }

    function alignValues() {
      const values = {};
      const h = val("sheet-align-h-select");
      const v = val("sheet-align-v-select");
      if (h !== loaded.h && h !== "mixed") values.textAlign = h;
      if (v !== loaded.v && v !== "mixed") values.verticalAlign = v === "bottom" ? "" : v;
      const box = $("sheet-align-wrap");
      const wrap = checked("sheet-align-wrap");
      if (box && (wrap !== (loaded.wrap === "1") || (loaded.wrap === "mixed" && !box.indeterminate))) values.wrap = wrap ? true : "";
      return values;
    }

    function borderSteps() {
      const steps = [];
      for (const [pos, [kind]] of Object.entries(POSITIONS)) {
        if (want[pos] === true) steps.push({ kind, edge: look.edgeText(line, color) });
        else if (want[pos] === false && initial[pos] !== "") steps.push({ kind, edge: null });
      }
      return steps;
    }

    // True when the panes have something to write; `apply()` writes it in one step.
    function dirty() {
      return Object.keys(alignValues()).length > 0 || borderTouched;
    }

    function apply() {
      ops.pen.line = line;
      ops.pen.color = color;
      return ops.setLook(alignValues(), borderTouched ? borderSteps() : null);
    }

    function lineSvg(name) {
      const svg = document.createElementNS(SVG_NS, "svg");
      svg.setAttribute("viewBox", "0 0 48 10");
      svg.setAttribute("aria-hidden", "true");
      const offsets = name === "double" ? [3, 7] : [5];
      for (const y of offsets) {
        const el = document.createElementNS(SVG_NS, "line");
        el.setAttribute("x1", "0");
        el.setAttribute("x2", "48");
        el.setAttribute("y1", String(y));
        el.setAttribute("y2", String(y));
        el.setAttribute("stroke", "currentColor");
        el.setAttribute("stroke-width", String(name === "double" ? 1 : name === "thick" ? 2.5 : 1.5));
        if (DASH[name]) el.setAttribute("stroke-dasharray", DASH[name]);
        svg.appendChild(el);
      }
      return svg;
    }

    function ensureControls() {
      if (ready) return;
      ready = true;
      for (const tab of tabs()) tab.addEventListener("click", () => showPane(tab.dataset.fmtTab));
      for (const item of document.querySelectorAll("#sheet-numfmt-dialog [data-border-line]")) {
        item.replaceChildren(lineSvg(item.dataset.borderLine));
        item.addEventListener("click", () => chooseLine(item.dataset.borderLine));
      }
      for (const btn of document.querySelectorAll("#sheet-numfmt-dialog [data-border-pos]")) {
        btn.addEventListener("click", () => togglePosition(btn.dataset.borderPos));
      }
      for (const btn of document.querySelectorAll("#sheet-numfmt-dialog [data-border-preset]")) {
        btn.addEventListener("click", () => applyPreset(btn.dataset.borderPreset));
      }
      fillOptions("sheet-align-h-select", H_OPTIONS);
      fillOptions("sheet-align-v-select", V_OPTIONS);
      host.__EASYOFFICE_COLOR_PICKER__?.setupColorPicker($("sheet-border-color-host"), (picked) => {
        const rgb = look.normalizeColor(picked);
        if (!rgb) return;
        color = rgb;
        setVal("sheet-border-color", "#" + rgb);
        touchBorder();
      });
    }

    return { open, apply, dirty, showPane };
  }

  // The cell format commands of the sheet window: the number format dialog with the two panes, the
  // alignment and border commands, and the state sync for their buttons.
  function createSheetLook(deps) {
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(deps.document);
    let numfmtUi = null;
    const ops = host.__EASYOFFICE_CREATE_SHEET_LOOK_OPS__({ ...deps, openDialog: (pane) => numfmtUi?.openNumfmtDialog(pane) });
    const ui = createSheetLookUi({ document: deps.document, kit, ops, getRange: deps.getRange });
    numfmtUi = host.__EASYOFFICE_CREATE_SHEET_NUMFMT_UI__?.({ document: deps.document, format: deps.format, kit, look: ui });
    return ops;
  }

  host.__EASYOFFICE_CREATE_SHEET_LOOK_UI__ = createSheetLookUi;
  host.__EASYOFFICE_CREATE_SHEET_LOOK__ = createSheetLook;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
