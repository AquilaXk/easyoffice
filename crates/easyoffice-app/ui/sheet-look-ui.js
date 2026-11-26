(function (host) {
  "use strict";

  // The 맞춤, 테두리 and 보호 panes of the 셀 서식 dialog (PD-207, PD-216; the 보호 pane has 잠금 and 숨기기, as in the Korean spreadsheet help). The dialog is the number format dialog of
  // sheet-numfmt-ui.js with tabs; this module fills the other two panes from the selection and writes them
  // back in one undoable step. The border pane follows the reference editor's cell border dialog
  // (`cell-border-bg-dialog.ts`): a grid of line kinds, a color, preset buttons and the positions around a
  // preview of the cell.

  const H_OPTIONS = [["", "기본"], ["left", "왼쪽"], ["center", "가운데"], ["right", "오른쪽"]];
  const V_OPTIONS = [["top", "위쪽"], ["middle", "가운데"], ["bottom", "아래쪽"]];
  const FONT_FAMILIES = [
    ["", "기본 (맑은 고딕)"], ["맑은 고딕", "맑은 고딕"], ["함초롬돋움", "함초롬돋움"],
    ["함초롬바탕", "함초롬바탕"], ["나눔고딕", "나눔고딕"], ["돋움", "돋움"], ["바탕", "바탕"],
    ["Arial", "Arial"], ["Calibri", "Calibri"], ["Times New Roman", "Times New Roman"], ["Courier New", "Courier New"],
  ];
  const FONT_SIZES = ["", "8", "9", "10", "11", "12", "14", "16", "18", "20", "24", "28", "36", "48", "72"];
  const MIXED = [["mixed", "혼합"]];
  function cssPxToPtStr(pxStr) {
    const m = /^(\d+(?:\.\d+)?)px$/.exec(String(pxStr || "").trim());
    if (!m) return "";
    const pt = Number(m[1]) * 0.75;
    const half = Math.round(pt * 2) / 2;
    return String(Math.abs(pt - half) < 0.08 ? half : Math.round(pt * 10) / 10);
  }
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
    const { document, kit, ops, getRange, getRanges } = deps;
    const look = host.__EASYOFFICE_SHEET_LOOK__;
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const { $, val, checked, setVal, setChecked, fillOptions, fireChange, note } = kit;
    let ready = false;
    let loaded = { h: "", v: "bottom", wrap: "", rot: "" };
    let rotTouched = false;
    let loadedMerge = "";
    let loadedFont = { family: "", size: "" };
    let initial = {};
    let want = {};
    let line = "thin";
    let color = look.DEFAULT_COLOR;
    let borderTouched = false;
    let loadedLocks = { locked: "1", hidden: "" };
    // The 글꼴 and 채우기 panes: the colors the selection has ("" none, "mixed", `rrggbb`) and the ones chosen.
    let loadedColors = { fill: "", fontColor: "" };
    let wantColors = {};

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
      if (name === "font") updateFontSample();
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

    function updateRotationPreview(degText) {
      const n = Number(degText);
      const valid = degText === "" || (/^-?\d+$/.test(degText) && n >= -90 && n <= 90);
      const deg = valid && degText !== "" ? n : 0;
      $("sheet-align-rotation-preview")?.style?.setProperty("--rot-preview", -deg + "deg");
      for (const btn of document.querySelectorAll("#sheet-numfmt-dialog [data-rot-preset]")) {
        const on = valid && degText !== "" && Number(btn.dataset.rotPreset) === deg;
        btn.setAttribute("aria-pressed", String(on));
      }
    }

    const MAX_SAMPLE_PT = 28;
    const MIN_SAMPLE_PT = 8;

    function measureSampleOverflow(sample) {
      const cw = sample.clientWidth;
      const ch = sample.clientHeight;
      if (!(cw > 0 && ch > 0)) return null;
      const cs = host.getComputedStyle?.(sample);
      const padX = cs ? (Number.parseFloat(cs.paddingLeft) || 0) + (Number.parseFloat(cs.paddingRight) || 0) : 0;
      const padY = cs ? (Number.parseFloat(cs.paddingTop) || 0) + (Number.parseFloat(cs.paddingBottom) || 0) : 0;
      const availW = Math.max(1, cw - padX - 2);
      const availH = Math.max(1, ch - padY - 2);
      const textEl = sample.querySelector?.(".sheet-font-sample-text");
      const tr = textEl?.getBoundingClientRect?.();
      const hasBox = Boolean(tr && tr.width > 0 && tr.height > 0);
      const needW = hasBox ? tr.width : (sample.scrollWidth > cw ? sample.scrollWidth : 0);
      const needH = hasBox ? tr.height : (sample.scrollHeight > ch ? sample.scrollHeight : 0);
      const overflowX = (hasBox && tr.width > availW) || sample.scrollWidth > cw;
      const overflowY = (hasBox && tr.height > availH) || sample.scrollHeight > ch;
      if (!overflowX && !overflowY) return null;
      const ratioW = overflowX && needW > 0 ? availW / needW : 1;
      const ratioH = overflowY && needH > 0 ? availH / needH : 1;
      return { ratio: Math.min(ratioW, ratioH), needW, needH };
    }

    function updateFontSample() {
      const sample = $("sheet-font-sample");
      if (!sample) return;
      const ff = val("sheet-font-family-select");
      const fs = val("sheet-font-size-select");
      const dfApi = host.__EASYOFFICE_SHEET_DEFAULT_FONT__;
      const cssFam = ff && ff !== "mixed" ? (dfApi?.quoteCssFont?.(ff) || ("'" + ff + "'")) : "";
      const rawPt = fs && fs !== "mixed" ? Number.parseFloat(fs) : NaN;
      const previewPt = Number.isFinite(rawPt) && rawPt > 0 ? Math.min(rawPt, MAX_SAMPLE_PT) : NaN;
      if (cssFam) sample.style.setProperty("--sample-font", cssFam);
      else sample.style.removeProperty("--sample-font");
      if (Number.isFinite(previewPt)) sample.style.setProperty("--sample-size", previewPt + "pt");
      else sample.style.removeProperty("--sample-size");
      let fitPt = Number.isFinite(previewPt) ? previewPt : 11;
      const minFloor = Math.min(MIN_SAMPLE_PT, fitPt);
      let over = measureSampleOverflow(sample);
      for (let step = 0; over && fitPt > minFloor && step < 6; step += 1) {
        const scaled = Math.floor(fitPt * over.ratio * 2) / 2;
        const nextPt = Math.max(minFloor, step === 0 ? scaled : Math.min(fitPt - 0.5, scaled));
        if (!(nextPt < fitPt)) break;
        const prevW = over.needW;
        const prevH = over.needH;
        fitPt = nextPt;
        sample.style.setProperty("--sample-size", fitPt + "pt");
        over = measureSampleOverflow(sample);
        if (over && !(over.needW < prevW || over.needH < prevH)) break;
      }
    }

    function mergeStateOf(ranges) {
      const sheet = deps.getActiveSheetName?.() ?? "";
      if (!layoutApi) return { state: "", canMerge: false };
      let anyMulti = false;
      let mergedCount = 0;
      let partial = false;
      for (const rg of ranges) {
        if (rg.r2 > rg.r1 || rg.c2 > rg.c1) anyMulti = true;
        const hits = layoutApi.mergesIn(sheet, rg.r1, rg.c1, rg.r2, rg.c2);
        if (hits.length === 1 && hits[0].r1 === rg.r1 && hits[0].c1 === rg.c1 && hits[0].r2 === rg.r2 && hits[0].c2 === rg.c2) {
          mergedCount += 1;
        } else if (hits.length > 0) {
          partial = true;
        }
      }
      const canMerge = anyMulti || mergedCount > 0 || partial;
      const state = partial ? "mixed" : mergedCount === ranges.length ? "1" : mergedCount > 0 ? "mixed" : "";
      return { state, canMerge };
    }

    function loadAlign(ranges) {
      const { textAlign: h, verticalAlign: v, wrap, textRotation } = ops.states(ranges);
      loaded = { h, v: v === "" ? "bottom" : v, wrap, rot: textRotation || "" };
      rotTouched = false;
      setOptions("sheet-align-h-select", h === "mixed" ? [...MIXED, ...H_OPTIONS] : H_OPTIONS, h);
      setOptions("sheet-align-v-select", v === "mixed" ? [...MIXED, ...V_OPTIONS] : V_OPTIONS, loaded.v);
      const box = $("sheet-align-wrap");
      if (box) box.indeterminate = wrap === "mixed";
      setChecked("sheet-align-wrap", wrap === "1");
      const mInfo = mergeStateOf(ranges);
      loadedMerge = mInfo.state;
      const mBox = $("sheet-align-merge");
      if (mBox) {
        mBox.disabled = !mInfo.canMerge;
        mBox.indeterminate = mInfo.state === "mixed";
        mBox.checked = mInfo.state === "1";
      }
      const rotInput = $("sheet-align-rotation");
      if (rotInput) {
        rotInput.value = loaded.rot === "mixed" ? "" : loaded.rot;
        rotInput.placeholder = loaded.rot === "mixed" ? "혼합" : "0";
        updateRotationPreview(rotInput.value.trim());
      }
    }

    function loadFont(ranges) {
      const { fontFamily, fontSize } = ops.states(ranges);
      const sizePt = fontSize === "mixed" ? "mixed" : cssPxToPtStr(fontSize);
      loadedFont = { family: fontFamily || "", size: sizePt };
      const famPairs = [...FONT_FAMILIES];
      if (loadedFont.family && loadedFont.family !== "mixed" && !famPairs.some(([v]) => v === loadedFont.family)) {
        famPairs.push([loadedFont.family, loadedFont.family]);
      }
      setOptions("sheet-font-family-select", loadedFont.family === "mixed" ? [...MIXED, ...famPairs] : famPairs, loadedFont.family);
      const sizePairs = FONT_SIZES.map((s) => [s, s === "" ? "기본" : s]);
      if (sizePt && sizePt !== "mixed" && !FONT_SIZES.includes(sizePt)) sizePairs.push([sizePt, sizePt]);
      setOptions("sheet-font-size-select", sizePt === "mixed" ? [...MIXED, ...sizePairs] : sizePairs, sizePt);
      updateFontSample();
    }

    // The 보호 pane: 잠금 and 숨기기 of the selection. A protected sheet shows them but does not change them.
    function loadProtect(rg) {
      loadedLocks = ops.protectStates(rg);
      const off = host.__EASYOFFICE_SHEET_PROTECT__?.isOn(deps.getActiveSheetName?.() ?? "") === true;
      for (const [id, state] of [["sheet-protect-cell-locked", loadedLocks.locked], ["sheet-protect-cell-hidden", loadedLocks.hidden]]) {
        const box = $(id);
        if (!box) continue;
        box.indeterminate = state === "mixed";
        box.checked = state === "1";
        box.disabled = off;
      }
    }

    // ---- colors ----------------------------------------------------------------------------------------
    // Shows `rgb` ("" for none or mixed) in the chip of the host `hostId` and in the sample.
    function showColor(key) {
      const rgb = key in wantColors ? wantColors[key] : loadedColors[key] === "mixed" ? "" : loadedColors[key];
      const [hostId, sampleId, prop] = key === "fill" ? ["sheet-bgcolor-host", "sheet-fill-sample", "--sample-fill"] : ["sheet-fcolor-host", "sheet-font-sample", "--sample-text"];
      $(hostId)?.querySelector?.(".dialog-swatch-chip")?.style?.setProperty("--dialog-swatch", rgb ? "#" + rgb : "");
      $(sampleId)?.style?.setProperty(prop, rgb ? "#" + rgb : "");
    }

    function loadColors(rg) {
      const { fill, fontColor } = ops.states(rg);
      loadedColors = { fill, fontColor };
      wantColors = {};
      showColor("fill");
      showColor("fontColor");
    }

    // The colors the panes changed: `{ fill, fontColor }`, each only when it was chosen ("" removes it).
    function colorValues() {
      const out = {};
      for (const key of ["fill", "fontColor"]) if (key in wantColors && wantColors[key] !== loadedColors[key]) out[key] = wantColors[key];
      return out;
    }

    function chooseColor(key, picked) {
      const rgb = picked === "" ? "" : look.normalizeColor(picked);
      if (rgb === null) return;
      wantColors[key] = rgb;
      showColor(key);
    }

    // `open(pane)`: loads the panes from the selection and shows `pane`.
    function open(pane) {
      ensureControls();
      const ranges = typeof getRanges === "function" ? getRanges() : [getRange()];
      loadColors(ranges);
      loadAlign(ranges);
      loadFont(ranges);
      loadBorder(ranges);
      loadProtect(ranges);
      showPane(pane);
    }

    // The lock values the 보호 pane changed: `{ locked, hideFormula }`, each only when its box moved.
    function lockValues() {
      const out = {};
      const moved = (id, state) => {
        const box = $(id);
        return box && !box.disabled && (box.checked !== (state === "1") || (state === "mixed" && !box.indeterminate));
      };
      if (moved("sheet-protect-cell-locked", loadedLocks.locked)) out.locked = checked("sheet-protect-cell-locked");
      if (moved("sheet-protect-cell-hidden", loadedLocks.hidden)) out.hideFormula = checked("sheet-protect-cell-hidden");
      return out;
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
      const rotInput = $("sheet-align-rotation");
      if (rotInput) {
        const raw = rotInput.value.trim();
        if (loaded.rot === "mixed" && raw === "" && !rotTouched) {
          // Mixed rotation kept unchanged when left blank.
        } else if (raw === "") {
          if (loaded.rot !== "" && loaded.rot !== "0") values.textRotation = 0;
        } else if (/^-?\d+$/.test(raw)) {
          const deg = Number(raw);
          if (deg >= -90 && deg <= 90 && String(deg === 0 ? "" : deg) !== loaded.rot) {
            values.textRotation = deg;
          }
        }
      }
      return values;
    }

    function fontValues() {
      const values = {};
      const ff = val("sheet-font-family-select");
      const fs = val("sheet-font-size-select");
      if (ff !== loadedFont.family && ff !== "mixed") values.fontFamily = ff;
      if (fs !== loadedFont.size && fs !== "mixed") {
        values.fontSize = fs === "" ? "" : Math.round((Number(fs) / 0.75) * 100) / 100 + "px";
      }
      return values;
    }

    function mergeValue() {
      const box = $("sheet-align-merge");
      if (!box || box.disabled) return null;
      if (box.checked && loadedMerge !== "1") return "merge";
      if (!box.checked && !box.indeterminate && loadedMerge !== "") return "unmerge";
      return null;
    }

    function validate() {
      const rotInput = $("sheet-align-rotation");
      if (rotInput) {
        const raw = rotInput.value.trim();
        if (raw !== "") {
          if (!/^-?\d+$/.test(raw) || Number(raw) < -90 || Number(raw) > 90) {
            note("sheet-numfmt-status", "텍스트 방향은 -90~90 사이의 정수");
            showPane("align");
            rotInput.focus?.();
            return false;
          }
        }
      }
      return true;
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
      return Object.keys(alignValues()).length > 0 || Object.keys(fontValues()).length > 0 || Object.keys(colorValues()).length > 0 || borderTouched || Object.keys(lockValues()).length > 0 || mergeValue() !== null;
    }

    function apply() {
      ops.pen.line = line;
      ops.pen.color = color;
      return ops.setLook({ ...alignValues(), ...fontValues(), ...colorValues() }, borderTouched ? borderSteps() : null, lockValues(), mergeValue());
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
      fillOptions("sheet-font-family-select", FONT_FAMILIES);
      fillOptions("sheet-font-size-select", FONT_SIZES.map((s) => [s, s === "" ? "기본" : s]));
      $("sheet-font-family-select")?.addEventListener("change", updateFontSample);
      $("sheet-font-size-select")?.addEventListener("change", updateFontSample);
      $("sheet-align-rotation")?.addEventListener("input", () => {
        rotTouched = true;
        updateRotationPreview($("sheet-align-rotation")?.value?.trim() || "");
        note("sheet-numfmt-status", "");
      });
      for (const btn of document.querySelectorAll("#sheet-numfmt-dialog [data-rot-preset]")) {
        btn.addEventListener("click", () => {
          rotTouched = true;
          const deg = btn.dataset.rotPreset || "0";
          setVal("sheet-align-rotation", deg);
          updateRotationPreview(deg);
          note("sheet-numfmt-status", "");
        });
      }
      const picker = host.__EASYOFFICE_COLOR_PICKER__;
      picker?.setupColorPicker($("sheet-fcolor-host"), (picked) => chooseColor("fontColor", picked), { clear: true, clearLabel: "자동" });
      picker?.setupColorPicker($("sheet-bgcolor-host"), (picked) => chooseColor("fill", picked), { clear: true, clearLabel: "채우기 없음" });
      picker?.setupColorPicker($("sheet-border-color-host"), (picked) => {
        const rgb = look.normalizeColor(picked);
        if (!rgb) return;
        color = rgb;
        setVal("sheet-border-color", "#" + rgb);
        touchBorder();
      });
      const sampleEl = $("sheet-font-sample");
      if (sampleEl && typeof host.ResizeObserver === "function") {
        let lastW = 0;
        let lastH = 0;
        new host.ResizeObserver(() => {
          const w = sampleEl.clientWidth;
          const h = sampleEl.clientHeight;
          if (!(w > 0 && h > 0) || (w === lastW && h === lastH)) return;
          lastW = w;
          lastH = h;
          updateFontSample();
        }).observe(sampleEl);
      }
    }

    return { open, apply, dirty, validate, showPane, refreshFontSample: updateFontSample };
  }

  // The cell format commands of the sheet window: the number format dialog with the two panes, the
  // alignment and border commands, and the state sync for their buttons.
  function createSheetLook(deps) {
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(deps.document);
    let numfmtUi = null;
    const ops = host.__EASYOFFICE_CREATE_SHEET_LOOK_OPS__({ ...deps, openDialog: (pane) => numfmtUi?.openNumfmtDialog(pane) });
    const ui = createSheetLookUi({ document: deps.document, kit, ops, getRange: deps.getRange, getRanges: deps.getRanges, getActiveSheetName: deps.getActiveSheetName });
    numfmtUi = host.__EASYOFFICE_CREATE_SHEET_NUMFMT_UI__?.({ document: deps.document, format: deps.format, kit, look: ui });
    host.__EASYOFFICE_CREATE_SHEET_COLOR_UI__?.({ document: deps.document, ops });
    return ops;
  }

  host.__EASYOFFICE_CREATE_SHEET_LOOK_UI__ = createSheetLookUi;
  host.__EASYOFFICE_CREATE_SHEET_LOOK__ = createSheetLook;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
