(function (host) {
  "use strict";

  // Range-wide cell formatting (bold, italic, underline, strikethrough, font size). The format is
  // written to the sheet MODEL (`sheetDataStore`) for every cell of the selected range, so rows and
  // columns outside the virtualized DOM window receive it too and re-rendered cells show it.
  const SIZE_PRESETS = ["13px", "15px", "17px", "12px"];
  // Whole-sheet selections (Ctrl+A is 1,048,576 x 16,384) are clamped to the used extent above this.
  const MAX_FORMAT_CELLS = 1000000;
  // Selections larger than this refresh the toolbar state after a short delay while navigating.
  const EAGER_STATE_CELLS = 20000;
  const STATE_DELAY_MS = 120;
  const STYLE_KEYS = ["fontWeight", "fontStyle", "textDecoration", "fontSize", "condfmtHigh", "frozenPanes"];
  const COMMANDS = {
    "sheet-home-bold": "bold",
    "sheet-home-italic": "italic",
    "sheet-home-underline": "underline",
    "sheet-home-strike": "strike",
    "sheet-home-style": "fontSize",
  };
  const BUTTON_IDS = {
    bold: "btn-stylebar-bold",
    italic: "btn-stylebar-italic",
    underline: "btn-stylebar-underline",
    strike: "btn-stylebar-strike",
  };

  function decorationTokens(style) {
    return String(style?.textDecoration || "").split(/\s+/).filter(Boolean);
  }

  function hasFlag(entry, kind) {
    const st = entry?.style;
    if (!st) return false;
    if (kind === "bold") return st.fontWeight === "600" || st.fontWeight === "bold" || Number(st.fontWeight) >= 600;
    if (kind === "italic") return st.fontStyle === "italic";
    return decorationTokens(st).includes(kind === "underline" ? "underline" : "line-through");
  }

  function writeFlag(style, kind, on) {
    if (kind === "bold") style.fontWeight = on ? "600" : "";
    else if (kind === "italic") style.fontStyle = on ? "italic" : "";
    else {
      const token = kind === "underline" ? "underline" : "line-through";
      const rest = decorationTokens(style).filter((t) => t !== token);
      style.textDecoration = (on ? [...rest, token] : rest).join(" ");
    }
  }

  // Editor style <-> save payload `style` (PD-089). The editor keeps CSS strings in the model
  // (`fontWeight`, `fontStyle`, `textDecoration`, `fontSize` in px); the payload carries flags and
  // `sizePt` (1 px = 0.75 pt).
  const PT_PER_PX = 0.75;
  const FLAG_KINDS = ["bold", "italic", "underline", "strike"];
  const MIN_SIZE_PT = 1;
  const MAX_SIZE_PT = 409;

  function styleToPayload(style) {
    if (!style) return null;
    const out = {};
    FLAG_KINDS.forEach((kind) => {
      if (hasFlag({ style }, kind)) out[kind] = true;
    });
    const px = /^(\d+(?:\.\d+)?)px$/.exec(String(style.fontSize || "").trim());
    if (px) {
      const pt = Math.round(Number(px[1]) * PT_PER_PX * 100) / 100;
      if (pt >= MIN_SIZE_PT && pt <= MAX_SIZE_PT) out.sizePt = pt;
    }
    return Object.keys(out).length > 0 ? out : null;
  }

  function styleFromPayload(payload) {
    if (!payload || typeof payload !== "object") return null;
    const style = { fontWeight: "", fontStyle: "", textDecoration: "", fontSize: "" };
    FLAG_KINDS.forEach((kind) => {
      if (payload[kind] === true) writeFlag(style, kind, true);
    });
    if (Number.isFinite(payload.sizePt) && payload.sizePt > 0) {
      style.fontSize = Math.round((payload.sizePt / PT_PER_PX) * 100) / 100 + "px";
    }
    return Object.values(style).some(Boolean) ? style : null;
  }

  function isEmptyEntry(entry) {
    const st = entry.style;
    const styled = st && STYLE_KEYS.some((k) => st[k]);
    return !entry.value && !entry.formula && !styled;
  }

  function patchCell(map, ref, mutate) {
    const entry = map[ref] || { value: "", formula: "" };
    const style = { fontWeight: "", fontStyle: "", textDecoration: "", fontSize: "", ...entry.style };
    mutate(style);
    entry.style = style;
    if (STYLE_KEYS.some((k) => style[k])) {
      map[ref] = entry;
      return;
    }
    delete entry.style;
    if (isEmptyEntry(entry)) delete map[ref];
    else map[ref] = entry;
  }

  function setPressed(el, state) {
    if (el) el.setAttribute("aria-pressed", state);
  }

  function createSheetFormat(api) {
    const {
      document,
      gridUtils,
      sheetDataStore,
      getActiveSheetName,
      getActiveModule,
      getRange,
      getSelection,
      getExtent,
      snapshotDomSheetCells,
      saveActiveSheetCellsToWorkbook,
      refreshWindowFromModel,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      pushRawUndoSnapshot,
      captureWorkspaceSnapshot,
    } = api;
    let stateTimer = null;

    // The selected range, clamped to the used extent when it is too large to materialize.
    function effectiveRange() {
      const rg = getRange();
      const area = (rg.r2 - rg.r1 + 1) * (rg.c2 - rg.c1 + 1);
      if (area <= MAX_FORMAT_CELLS) return { ...rg, clamped: false };
      const ext = getExtent();
      const r2 = Math.max(rg.r1, Math.min(rg.r2, ext.rows || 1));
      const c2 = Math.max(rg.c1, Math.min(rg.c2, ext.cols || 1));
      return { r1: rg.r1, r2, c1: rg.c1, c2, clamped: true };
    }

    function forEachRef(rg, cb) {
      for (let r = rg.r1; r <= rg.r2; r += 1) {
        for (let c = rg.c1; c <= rg.c2; c += 1) cb(gridUtils.refOf(r, c));
      }
    }

    // "true" / "false" / "mixed" for a boolean format over the range (absent cells count as off).
    function rangeFlagState(map, rg, kind) {
      const area = (rg.r2 - rg.r1 + 1) * (rg.c2 - rg.c1 + 1);
      let on = 0;
      const keys = Object.keys(map);
      if (keys.length < area) {
        for (const ref of keys) {
          const p = gridUtils.parseRef(ref);
          if (p && p.row >= rg.r1 && p.row <= rg.r2 && p.col >= rg.c1 && p.col <= rg.c2 && hasFlag(map[ref], kind)) on += 1;
        }
      } else {
        forEachRef(rg, (ref) => {
          if (hasFlag(map[ref], kind)) on += 1;
        });
      }
      if (on === 0) return "false";
      return on === area ? "true" : "mixed";
    }

    function nextSize(map) {
      const sel = getSelection();
      const cur = map[gridUtils.refOf(sel.ar, sel.ac)]?.style?.fontSize || "";
      const idx = SIZE_PRESETS.indexOf(cur);
      return SIZE_PRESETS[idx >= 0 ? (idx + 1) % SIZE_PRESETS.length : 1];
    }

    // Writes the format into the model for the whole range; returns the number of cells touched.
    function applyToModel(kind, value) {
      const curSheet = getActiveSheetName();
      const map = snapshotDomSheetCells();
      sheetDataStore[curSheet] = map;
      const rg = effectiveRange();
      let mutate;
      if (kind === "fontSize") {
        const size = value || nextSize(map);
        mutate = (st) => {
          st.fontSize = size;
        };
      } else {
        const on = rangeFlagState(map, rg, kind) !== "true";
        mutate = (st) => writeFlag(st, kind, on);
      }
      forEachRef(rg, (ref) => patchCell(map, ref, mutate));
      refreshWindowFromModel();
      saveActiveSheetCellsToWorkbook();
      syncToolbar();
      return (rg.r2 - rg.r1 + 1) * (rg.c2 - rg.c1 + 1);
    }

    // Ribbon / shortcut entry: runs inside the tracked ribbon op (one undo step). True when handled.
    function applyCommand(cmdId) {
      const kind = COMMANDS[cmdId];
      if (!kind) return false;
      applyToModel(kind);
      return true;
    }

    // Standalone entry (Ctrl+5 strikethrough, programmatic): its own single undoable operation.
    function formatRange(kind, value) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        pushRawUndoSnapshot(captureWorkspaceSnapshot(false));
        const count = applyToModel(kind, value);
        const fb = document.getElementById("status-command-feedback");
        if (fb) fb.textContent = "셀 서식: " + count + "칸";
        return count;
      });
    }

    function syncToolbar() {
      if (getActiveModule() !== "SHEET") return;
      const map = sheetDataStore[getActiveSheetName()] || {};
      const rg = getRange();
      const barVisible = document.getElementById("doc-style-bar")?.hidden === false;
      Object.keys(BUTTON_IDS).forEach((kind) => {
        const state = rangeFlagState(map, rg, kind);
        setPressed(document.querySelector('[data-command-id="sheet-home-' + kind + '"]'), state);
        if (barVisible) setPressed(document.getElementById(BUTTON_IDS[kind]), state);
      });
    }

    // Selection moves while navigating: small ranges update at once, huge ones after a short delay.
    function scheduleToolbarSync() {
      const rg = getRange();
      if ((rg.r2 - rg.r1 + 1) * (rg.c2 - rg.c1 + 1) <= EAGER_STATE_CELLS) {
        syncToolbar();
        return;
      }
      if (stateTimer !== null) host.clearTimeout?.(stateTimer);
      stateTimer =
        host.setTimeout?.(() => {
          stateTimer = null;
          syncToolbar();
        }, STATE_DELAY_MS) ?? null;
    }

    return { applyCommand, formatRange, syncToolbar, scheduleToolbarSync, rangeFlagState: (kind) => rangeFlagState(sheetDataStore[getActiveSheetName()] || {}, getRange(), kind) };
  }

  host.__EASYOFFICE_CREATE_SHEET_FORMAT__ = createSheetFormat;
  host.__EASYOFFICE_SHEET_STYLE_CODEC__ = { styleToPayload, styleFromPayload };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
