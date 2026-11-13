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
  // Ranges up to this many cells are read by address (see computeRangeMasks).
  const DIRECT_AREA_CELLS = 4096;
  // Alignment, wrapping and borders (PD-207) are keys of the same style object.
  const look = host.__EASYOFFICE_SHEET_LOOK__;
  const STYLE_KEYS = ["fontWeight", "fontStyle", "textDecoration", "fontSize", "numFmt", ...look.LOOK_KEYS];
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

  const FLAG_BITS = { bold: 1, italic: 2, underline: 4, strike: 8 };

  function entryMask(entry) {
    const st = entry?.style;
    if (!st) return 0;
    let m = 0;
    const fw = st.fontWeight;
    if (fw === "600" || fw === "bold" || Number(fw) >= 600) m |= 1;
    if (st.fontStyle === "italic") m |= 2;
    const td = st.textDecoration;
    if (typeof td === "string" && td.length > 0) {
      if (td.includes("underline")) m |= 4;
      if (td.includes("line-through")) m |= 8;
    }
    return m;
  }

  function hasFlag(entry, kind) {
    return (entryMask(entry) & (FLAG_BITS[kind] || 0)) !== 0;
  }

  function writeFlag(style, kind, on) {
    if (kind === "bold") style.fontWeight = on ? "600" : "";
    else if (kind === "italic") style.fontStyle = on ? "italic" : "";
    else {
      const td = typeof style.textDecoration === "string" ? style.textDecoration : "";
      const hasU = kind === "underline" ? on : td.includes("underline");
      const hasS = kind === "strike" ? on : td.includes("line-through");
      if (hasU && hasS) style.textDecoration = "underline line-through";
      else if (hasU) style.textDecoration = "underline";
      else if (hasS) style.textDecoration = "line-through";
      else style.textDecoration = "";
    }
  }

  // Editor style <-> save payload `style` (PD-089). The editor keeps CSS strings in the model
  // (`fontWeight`, `fontStyle`, `textDecoration`, `fontSize` in px) and the number format code
  // (`numFmt`, PD-193); the payload carries flags, `sizePt` (1 px = 0.75 pt) and `nf`, the 1-based
  // entry of the sheet's `numFmts` list (`internNf` adds a code to it, `codes` is that list).
  const PT_PER_PX = 0.75;
  const FLAG_KINDS = ["bold", "italic", "underline", "strike"];
  const MIN_SIZE_PT = 1;
  const MAX_SIZE_PT = 409;

  function styleToPayload(style, internNf) {
    if (!style) return null;
    const out = {};
    if (style.numFmt && internNf) {
      const nf = internNf(style.numFmt);
      if (nf > 0) out.nf = nf;
    }
    if (look.H_ALIGNS.includes(style.textAlign)) out.ha = style.textAlign;
    if (look.V_ALIGNS.includes(style.verticalAlign)) out.va = style.verticalAlign;
    if (style.wrap) out.wrap = true;
    const border = look.freezeBorder(style.border);
    if (border) out.bd = { ...border };
    if (style.unlocked) out.unlocked = true;
    if (style.locked) out.locked = true;
    if (style.hideFormula) out.hideFormula = true;
    const mask = entryMask({ style });
    if (mask & 1) out.bold = true;
    if (mask & 2) out.italic = true;
    if (mask & 4) out.underline = true;
    if (mask & 8) out.strike = true;
    const px = /^(\d+(?:\.\d+)?)px$/.exec(String(style.fontSize || "").trim());
    if (px) {
      const pt = Math.round(Number(px[1]) * PT_PER_PX * 100) / 100;
      if (pt >= MIN_SIZE_PT && pt <= MAX_SIZE_PT) out.sizePt = pt;
    }
    return Object.keys(out).length > 0 ? out : null;
  }

  function styleFromPayload(payload, codes) {
    if (!payload || typeof payload !== "object") return null;
    const code = Number.isInteger(payload.nf) && Array.isArray(codes) ? codes[payload.nf - 1] : "";
    const style = { fontWeight: "", fontStyle: "", textDecoration: "", fontSize: "" };
    if (typeof code === "string" && code !== "") style.numFmt = code;
    FLAG_KINDS.forEach((kind) => {
      if (payload[kind] === true) writeFlag(style, kind, true);
    });
    if (Number.isFinite(payload.sizePt) && payload.sizePt > 0) {
      style.fontSize = Math.round((payload.sizePt / PT_PER_PX) * 100) / 100 + "px";
    }
    if (look.H_ALIGNS.includes(payload.ha)) style.textAlign = payload.ha;
    if (payload.va === "top" || payload.va === "middle") style.verticalAlign = payload.va;
    if (payload.wrap === true) style.wrap = true;
    const border = look.freezeBorder(payload.bd);
    if (border) style.border = border;
    if (payload.unlocked === true) style.unlocked = true;
    else if (payload.locked === true) style.locked = true;
    if (payload.hideFormula === true) style.hideFormula = true;
    return Object.values(style).some(Boolean) ? internStyle(style) : null;
  }

  function isEmptyEntry(entry) {
    const st = entry.style;
    const styled = st && STYLE_KEYS.some((k) => st[k]);
    return !entry.value && !entry.formula && !styled;
  }

  // A range written with one format holds one frozen style object per distinct look instead of one
  // per cell; code that changes a cell's style writes a new object (never edits one in place).
  const internedStyles = new Map();

  function internStyle(style) {
    if (style.wrap) look.markWrap();
    const key = `${style.fontWeight}|${style.fontStyle}|${style.textDecoration}|${style.fontSize}|${style.numFmt || ""}|${look.lookKey(style)}`;
    let shared = internedStyles.get(key);
    if (!shared) {
      shared = Object.freeze(style);
      internedStyles.set(key, shared);
    }
    return shared;
  }

  function patchCell(map, ref, mutate) {
    const entry = map[ref] || { value: "", formula: "" };
    const style = { fontWeight: "", fontStyle: "", textDecoration: "", fontSize: "", ...entry.style };
    mutate(style);
    if (STYLE_KEYS.some((k) => style[k])) {
      entry.style = internStyle(style);
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
      refreshFocusedSheetCellPendingSnapshot,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      pushRawUndoSnapshot,
      captureWorkspaceSnapshot,
    } = api;
    const numfmt = host.__EASYOFFICE_SHEET_NUMFMT__;
    let stateTimer = null;

    // A protected sheet refuses a change of format unless it allows 셀 서식; the lock state of a cell changes only on
    // a sheet that is not protected (PD-216). True when the change is refused (the status line says so).
    function formatBlocked(lock) {
      const guard = host.__EASYOFFICE_SHEET_PROTECT__;
      if (!guard) return false;
      return !(lock ? guard.guardUnprotected(getActiveSheetName()) : guard.guardAction(getActiveSheetName(), "formatCells"));
    }

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

    function computeRangeMasks(map, rg) {
      const area = (rg.r2 - rg.r1 + 1) * (rg.c2 - rg.c1 + 1);
      let orMask = 0;
      let andMask = 15;
      // A small range is read by address: listing the sheet's keys only to count them costs more than the range.
      const keys = area <= DIRECT_AREA_CELLS ? null : Object.keys(map);
      if (keys !== null && keys.length < area) {
        let visited = 0;
        for (let i = 0; i < keys.length; i += 1) {
          const ref = keys[i];
          const packed = gridUtils.packRef(ref);
          if (packed <= 0) continue;
          const col = packed & 32767;
          const row = (packed - col) / 32768;
          if (row >= rg.r1 && row <= rg.r2 && col >= rg.c1 && col <= rg.c2) {
            const mask = entryMask(map[ref]);
            orMask |= mask;
            andMask &= mask;
            visited += 1;
          }
        }
        if (visited < area) andMask = 0;
      } else {
        for (let r = rg.r1; r <= rg.r2; r += 1) {
          for (let c = rg.c1; c <= rg.c2; c += 1) {
            const mask = entryMask(map[gridUtils.refOf(r, c)]);
            orMask |= mask;
            andMask &= mask;
          }
        }
      }
      return { orMask, andMask };
    }

    function flagStateFromMasks(orMask, andMask, kind) {
      const bit = FLAG_BITS[kind] || 0;
      if ((orMask & bit) === 0) return "false";
      return (andMask & bit) !== 0 ? "true" : "mixed";
    }

    // "true" / "false" / "mixed" for a boolean format over the range (absent cells count as off).
    function rangeFlagState(map, rg, kind) {
      const { orMask, andMask } = computeRangeMasks(map, rg);
      return flagStateFromMasks(orMask, andMask, kind);
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

    // Number format of every cell of the selection (PD-193): `resolve(cells)` gets
    // `[{ ref, code, raw }]` and answers one new code per cell (undefined keeps it). Returns
    // `{ count }` or `{ refused }` when the sheet would hold more codes than it may.
    function applyNumFmtToModel(resolve) {
      const curSheet = getActiveSheetName();
      const map = snapshotDomSheetCells();
      sheetDataStore[curSheet] = map;
      const rg = effectiveRange();
      const cells = [];
      forEachRef(rg, (ref) => {
        const e = map[ref];
        cells.push({ ref, code: e?.style?.numFmt || "", raw: e ? String(e.value ?? "") : "" });
      });
      const next = resolve(cells);
      const codes = new Set();
      Object.keys(map).forEach((ref) => map[ref]?.style?.numFmt && codes.add(map[ref].style.numFmt));
      let known = codes.size;
      for (const code of next) {
        if (code && !codes.has(code)) {
          if (!numfmt.codeWithinBounds(code) || known >= numfmt.MAX_CODES) return { refused: true };
          codes.add(code);
          known += 1;
        }
      }
      let count = 0;
      cells.forEach((cell, at) => {
        const code = next[at];
        if (code === undefined || code === cell.code) return;
        patchCell(map, cell.ref, (st) => {
          st.numFmt = code;
        });
        count += 1;
      });
      refreshWindowFromModel();
      saveActiveSheetCellsToWorkbook();
      return { count };
    }

    // One undoable operation: `resolve` as in applyNumFmtToModel.
    function formatNumbers(resolve, label) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        if (formatBlocked(false)) return { count: 0 };
        pushRawUndoSnapshot(captureWorkspaceSnapshot(false));
        const result = applyNumFmtToModel(resolve);
        refreshFocusedSheetCellPendingSnapshot?.();
        const fb = document.getElementById("status-command-feedback");
        if (fb) fb.textContent = result.refused ? "서식 코드를 더 늘릴 수 없음" : (label || "표시 형식") + ": " + result.count + "칸";
        return result;
      });
    }

    // One undoable operation over the styles of the selection (PD-207): `edit({ map, rg, patch })` writes
    // through `patch(ref, mutate)` and answers the number of cells it changed (or a refusal note).
    function editStyles(label, edit, lock = false) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        if (formatBlocked(lock)) return 0;
        pushRawUndoSnapshot(captureWorkspaceSnapshot(false));
        const curSheet = getActiveSheetName();
        const map = snapshotDomSheetCells();
        sheetDataStore[curSheet] = map;
        const result = edit({ map, rg: effectiveRange(), patch: (ref, mutate) => patchCell(map, ref, mutate) });
        refreshWindowFromModel();
        saveActiveSheetCellsToWorkbook();
        refreshFocusedSheetCellPendingSnapshot?.();
        const fb = document.getElementById("status-command-feedback");
        if (fb) fb.textContent = typeof result === "string" ? result : label + ": " + result + "칸";
        return result;
      });
    }

    // The anchor cell of the selection with its number format code and stored value.
    function anchorCell() {
      const sel = getSelection();
      const ref = gridUtils.refOf(sel.ar, sel.ac);
      const e = sheetDataStore[getActiveSheetName()]?.[ref];
      return { ref, code: e?.style?.numFmt || "", raw: e ? String(e.value ?? "") : "" };
    }

    // Ribbon / shortcut entry: runs inside the tracked ribbon op (one undo step). True when handled.
    function applyCommand(cmdId) {
      const kind = COMMANDS[cmdId];
      if (!kind) return false;
      // The ribbon sets its own status line after the effect, so the refusal is said again once that is done.
      if (formatBlocked(false)) host.setTimeout?.(() => host.__EASYOFFICE_SHEET_PROTECT__.say(host.__EASYOFFICE_SHEET_PROTECT__.BLOCKED), 0);
      else applyToModel(kind);
      return true;
    }

    // Standalone entry (Ctrl+5 strikethrough, programmatic): its own single undoable operation.
    function formatRange(kind, value) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        if (formatBlocked(false)) return 0;
        pushRawUndoSnapshot(captureWorkspaceSnapshot(false));
        const count = applyToModel(kind, value);
        refreshFocusedSheetCellPendingSnapshot?.();
        const fb = document.getElementById("status-command-feedback");
        if (fb) fb.textContent = "셀 서식: " + count + "칸";
        return count;
      });
    }

    function syncToolbar() {
      if (getActiveModule() !== "SHEET") return;
      const map = sheetDataStore[getActiveSheetName()] || {};
      const rg = getRange();
      const { orMask, andMask } = computeRangeMasks(map, rg);
      for (let i = 0; i < FLAG_KINDS.length; i += 1) {
        const kind = FLAG_KINDS[i];
        setPressed(document.getElementById(BUTTON_IDS[kind]), flagStateFromMasks(orMask, andMask, kind));
      }
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

    return { applyCommand, formatRange, formatNumbers, editStyles, effectiveRange, anchorCell, syncToolbar, scheduleToolbarSync, rangeFlagState: (kind) => rangeFlagState(sheetDataStore[getActiveSheetName()] || {}, getRange(), kind) };
  }

  host.__EASYOFFICE_CREATE_SHEET_FORMAT__ = createSheetFormat;
  host.__EASYOFFICE_SHEET_STYLE_CODEC__ = { styleToPayload, styleFromPayload };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
