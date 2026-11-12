(function (host) {
  "use strict";

  // Commands of the sheet window for alignment, wrapping and borders (PD-207): the 서식 menu items,
  // the toolbar buttons and the 셀 서식 dialog panes run `run(action)` or the plans below. Every
  // command is one undoable operation over the selection (`format.editStyles`).
  //
  // Actions: `align-left|center|right`, `valign-top|middle|bottom`, `wrap`, `border-all|outer|inner|top|
  // bottom|left|right|none`, `line-thin|thick|dashed|dotted|double` (the line the next border takes),
  // `dialog-align`, `dialog-border`.

  const AREA_LIMIT = 500000; // styled cells one sheet may hold (the file writers' limit)
  const STATE_SCAN_CELLS = 20000;
  const H_ACTIONS = { "align-left": "left", "align-center": "center", "align-right": "right" };
  const V_ACTIONS = { "valign-top": "top", "valign-middle": "middle", "valign-bottom": "" };
  const BORDER_KINDS = { "border-all": "all", "border-outer": "outer", "border-inner": "inner", "border-top": "top", "border-bottom": "bottom", "border-left": "left", "border-right": "right", "border-none": "none" };
  const LABELS = { align: "맞춤", wrap: "줄 바꿈", border: "테두리" };

  function createSheetLookOps(deps) {
    const { document, format, gridUtils, sheetDataStore, getActiveSheetName, getActiveModule, getRange, openDialog } = deps;
    const look = host.__EASYOFFICE_SHEET_LOOK__;
    const pen = { line: "thin", color: look.DEFAULT_COLOR };

    // The value `read(entry)` has in every cell of the selection, "" when none has one, "mixed" otherwise.
    function commonValue(read, rg) {
      const map = sheetDataStore[getActiveSheetName()] || {};
      const area = (rg.r2 - rg.r1 + 1) * (rg.c2 - rg.c1 + 1);
      let seen;
      let count = 0;
      const take = (value) => {
        count += 1;
        seen = seen === undefined ? value : seen === value ? seen : "mixed";
      };
      if (area <= STATE_SCAN_CELLS) {
        for (let r = rg.r1; r <= rg.r2 && seen !== "mixed"; r += 1) {
          for (let c = rg.c1; c <= rg.c2; c += 1) take(read(map[gridUtils.refOf(r, c)]));
        }
        return seen ?? "";
      }
      for (const ref of Object.keys(map)) {
        const packed = gridUtils.packRef(ref);
        const col = packed & 32767;
        const row = (packed - col) / 32768;
        if (packed > 0 && row >= rg.r1 && row <= rg.r2 && col >= rg.c1 && col <= rg.c2) take(read(map[ref]));
      }
      if (count < area) take("");
      return seen ?? "";
    }

    const readers = {
      textAlign: (e) => e?.style?.textAlign || "",
      verticalAlign: (e) => e?.style?.verticalAlign || "",
      wrap: (e) => (e?.style?.wrap ? "1" : ""),
    };

    // Sets `key` of every cell of the selection to `value`; the same value again clears it.
    function setKey(label, key, value) {
      const rg = format.effectiveRange();
      const toggled = commonValue(readers[key], rg) === (value === true ? "1" : value) ? "" : value;
      return format.editStyles(label, ({ rg: range, patch }) => {
        let n = 0;
        for (let r = range.r1; r <= range.r2; r += 1) {
          for (let c = range.c1; c <= range.c2; c += 1) {
            patch(gridUtils.refOf(r, c), (st) => {
              st[key] = toggled;
            });
            n += 1;
          }
        }
        return n;
      });
    }

    // The selected cells with `values` written by `{ textAlign, verticalAlign, wrap }` in one step (the dialog).
    function setLook(values, border) {
      return format.editStyles(LABELS.align, (ctx) => {
        let n = 0;
        for (let r = ctx.rg.r1; r <= ctx.rg.r2; r += 1) {
          for (let c = ctx.rg.c1; c <= ctx.rg.c2; c += 1) {
            ctx.patch(gridUtils.refOf(r, c), (st) => Object.assign(st, values));
            n += 1;
          }
        }
        const refused = border ? applyBorders(ctx, border) : null;
        return refused || n;
      });
    }

    // Applies border steps `[{ kind, edge }]` in order inside an edit; a note when the range is too large.
    function applyBorders({ map, rg, patch }, steps) {
      const area = (rg.r2 - rg.r1 + 1) * (rg.c2 - rg.c1 + 1);
      if (area > AREA_LIMIT && steps.some((s) => s.kind !== "top" && s.kind !== "bottom" && s.kind !== "left" && s.kind !== "right" && s.kind !== "outer")) {
        return "테두리 범위 초과";
      }
      const borderAt = (r, c) => map[gridUtils.refOf(r, c)]?.style?.border;
      let n = 0;
      for (const { kind, edge } of steps) {
        const plan = look.planBorders(rg, kind, edge, borderAt, gridUtils.MAX_ROWS, gridUtils.MAX_COLS);
        for (const { r, c, border } of plan) {
          patch(gridUtils.refOf(r, c), (st) => {
            st.border = border;
          });
        }
        n += plan.length;
      }
      return n;
    }

    function applyBorder(kind, line = pen.line, color = pen.color) {
      const edge = kind === "none" ? null : look.edgeText(line, color);
      if (kind !== "none" && !edge) return Promise.resolve(null);
      return format.editStyles(LABELS.border, (ctx) => applyBorders(ctx, [{ kind, edge }]));
    }

    // The border positions a selection has an edge on, for the dialog: `{ t, b, l, r, h, v }` with the
    // edge text each has in every cell concerned ("" for none, "mixed" for cells that differ).
    function borderState(rg) {
      const map = sheetDataStore[getActiveSheetName()] || {};
      const at = (r, c) => map[gridUtils.refOf(r, c)]?.style?.border;
      const out = {};
      const gather = (key, cells) => {
        let seen;
        for (const [r, c, side] of cells) {
          const edge = at(r, c)?.[side] || "";
          seen = seen === undefined ? edge : seen === edge ? seen : "mixed";
        }
        out[key] = seen ?? "";
      };
      const cols = [];
      const rows = [];
      for (let c = rg.c1; c <= rg.c2; c += 1) cols.push(c);
      for (let r = rg.r1; r <= rg.r2; r += 1) rows.push(r);
      const cap = (list) => list.slice(0, 200);
      gather("t", cap(cols).map((c) => [rg.r1, c, "t"]));
      gather("b", cap(cols).map((c) => [rg.r2, c, "b"]));
      gather("l", cap(rows).map((r) => [r, rg.c1, "l"]));
      gather("r", cap(rows).map((r) => [r, rg.c2, "r"]));
      gather("h", rg.r1 === rg.r2 ? [] : cap(cols).map((c) => [rg.r1, c, "b"]));
      gather("v", rg.c1 === rg.c2 ? [] : cap(rows).map((r) => [r, rg.c1, "r"]));
      return out;
    }

    function setLine(line) {
      pen.line = line;
      sync();
    }

    function setColor(color) {
      const rgb = look.normalizeColor(color);
      if (rgb) pen.color = rgb;
    }

    function run(action) {
      if (action in H_ACTIONS) return setKey(LABELS.align, "textAlign", H_ACTIONS[action]);
      if (action in V_ACTIONS) return setKey(LABELS.align, "verticalAlign", V_ACTIONS[action]);
      if (action === "wrap") return setKey(LABELS.wrap, "wrap", true);
      if (action in BORDER_KINDS) return applyBorder(BORDER_KINDS[action]);
      if (action.startsWith("line-") && look.LINES.includes(action.slice(5))) return setLine(action.slice(5));
      if (action.startsWith("dialog-")) return openDialog?.(action.slice(7));
      return null;
    }

    // Pressed and checked states of the buttons and menu items that carry `data-sheet-look`.
    function sync() {
      if (getActiveModule() !== "SHEET") return;
      const rg = getRange();
      const state = {
        textAlign: commonValue(readers.textAlign, rg),
        verticalAlign: commonValue(readers.verticalAlign, rg),
        wrap: commonValue(readers.wrap, rg),
      };
      for (const el of document.querySelectorAll("[data-sheet-look]")) {
        const action = el.dataset.sheetLook;
        let on = null;
        if (action in H_ACTIONS) on = state.textAlign === H_ACTIONS[action];
        else if (action === "valign-bottom") on = state.verticalAlign === "";
        else if (action in V_ACTIONS) on = state.verticalAlign === V_ACTIONS[action];
        else if (action === "wrap") on = state.wrap === "1";
        else if (action.startsWith("line-")) on = pen.line === action.slice(5);
        if (on === null) continue;
        el.setAttribute(el.getAttribute("role") === "menuitemcheckbox" || el.getAttribute("role") === "menuitemradio" ? "aria-checked" : "aria-pressed", String(on));
      }
    }

    function bind() {
      for (const el of document.querySelectorAll("[data-sheet-look]")) {
        el.addEventListener("click", (ev) => {
          ev.stopPropagation?.();
          host.__EASYOFFICE_CLOSE_MENUS__?.(document);
          if (el.getAttribute("aria-disabled") === "true") return;
          void Promise.resolve(run(el.dataset.sheetLook)).then(sync).catch(() => {});
        });
      }
    }

    bind();
    return { run, setLook, applyBorders, borderState, setColor, setLine, sync, pen, commonValue, readers };
  }

  host.__EASYOFFICE_CREATE_SHEET_LOOK_OPS__ = createSheetLookOps;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
