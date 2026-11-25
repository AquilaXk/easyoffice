(function (host) {
  "use strict";

  // Commands of the sheet window for alignment, wrapping and borders (PD-207): the 서식 menu items,
  // the toolbar buttons and the 셀 서식 dialog panes run `run(action)` or the plans below. Every
  // command is one undoable operation over the selection (`format.editStyles`).
  //
  // Actions: `align-left|center|right`, `valign-top|middle|bottom`, `wrap`, `border-all|outer|inner|top|
  // bottom|left|right|none`, `line-thin|thick|dashed|dotted|double` (the line the next border takes),
  // `dialog-align`, `dialog-border`, and the colors (PD-227): `fill:<rrggbb>`, `fill-none`, `font-color:<rrggbb>`,
  // `font-color-auto`.

  const AREA_LIMIT = 500000; // styled cells one sheet may hold (the file writers' limit)
  const STATE_SCAN_CELLS = 20000;
  const H_ACTIONS = { "align-left": "left", "align-center": "center", "align-right": "right" };
  const V_ACTIONS = { "valign-top": "top", "valign-middle": "middle", "valign-bottom": "" };
  const BORDER_KINDS = { "border-all": "all", "border-outer": "outer", "border-inner": "inner", "border-top": "top", "border-bottom": "bottom", "border-left": "left", "border-right": "right", "border-none": "none" };
  const LABELS = { align: "맞춤", wrap: "텍스트 줄 바꿈", border: "테두리", protect: "셀 보호", fill: "채우기 색", fontColor: "글꼴 색" };
  // The style key and label of each color action.
  const COLOR_ACTIONS = { fill: ["fill", LABELS.fill], "font-color": ["fontColor", LABELS.fontColor] };

  function createSheetLookOps(deps) {
    const { document, format, gridUtils, grid, sheetDataStore, getActiveSheetName, getActiveModule, getRange, getRanges, getSelection, openDialog } = deps;
    const look = host.__EASYOFFICE_SHEET_LOOK__;
    const multiApi = host.__EASYOFFICE_SHEET_MULTI_RANGE__;
    const pen = { line: "thin", color: look.DEFAULT_COLOR };

    // For each key of `readers`: the value `read(entry)` has in every cell of the selection, "" when none has
    // one, "mixed" otherwise. One pass over the cells.
    const readers = {
      textAlign: (e) => e?.style?.textAlign || "",
      verticalAlign: (e) => e?.style?.verticalAlign || "",
      wrap: (e) => (e?.style?.wrap ? "1" : ""),
      textRotation: (e) => (e?.style?.textRotation ? String(e.style.textRotation) : ""),
      fontFamily: (e) => e?.style?.fontFamily || "",
      fontSize: (e) => e?.style?.fontSize || "",
      fill: (e) => e?.style?.fill || "",
      fontColor: (e) => e?.style?.fontColor || "",
    };
    const KEYS = Object.keys(readers);
    const activeRanges = (rgOrList) => {
      if (Array.isArray(rgOrList)) return rgOrList;
      if (rgOrList) return [rgOrList];
      return typeof getRanges === "function" ? getRanges() : [getRange()];
    };

    function states(rgOrList) {
      const ranges = activeRanges(rgOrList);
      const map = sheetDataStore[getActiveSheetName()] || {};
      const area = multiApi ? multiApi.totalArea(ranges) : (ranges[0].r2 - ranges[0].r1 + 1) * (ranges[0].c2 - ranges[0].c1 + 1);
      const seen = {};
      const take = (entry) => {
        for (const key of KEYS) {
          const value = readers[key](entry);
          seen[key] = !(key in seen) ? value : seen[key] === value ? value : "mixed";
        }
      };
      // A range past the limit is not scanned (the whole sheet is 17 billion cells): it counts as mixed.
      if (area > STATE_SCAN_CELLS) return Object.fromEntries(KEYS.map((key) => [key, "mixed"]));
      if (ranges.length === 1 || !multiApi) {
        const rg = ranges[0];
        for (let r = rg.r1; r <= rg.r2; r += 1) {
          for (let c = rg.c1; c <= rg.c2; c += 1) take(map[gridUtils.refOf(r, c)]);
        }
      } else {
        multiApi.forEachUniqueCoord(ranges, gridUtils.packCoord, (r, c) => take(map[gridUtils.refOf(r, c)]), STATE_SCAN_CELLS);
      }
      return Object.fromEntries(KEYS.map((key) => [key, seen[key] ?? ""]));
    }

    function walkRanges(ctx, fn) {
      const ranges = ctx.ranges || [ctx.rg];
      let n = 0;
      if (ranges.length <= 1 || !multiApi) {
        const rg = ranges[0];
        for (let r = rg.r1; r <= rg.r2; r += 1) {
          for (let c = rg.c1; c <= rg.c2; c += 1) {
            fn(r, c);
            n += 1;
          }
        }
        return n;
      }
      multiApi.forEachUniqueCoord(ranges, gridUtils.packCoord, (r, c) => {
        fn(r, c);
        n += 1;
      });
      return n;
    }

    // Sets `key` of every cell of the selection to `value`; the same value again clears it.
    function setKey(label, key, value) {
      const ranges = format.effectiveRanges();
      const toggled = states(ranges)[key] === (value === true ? "1" : value) ? "" : value;
      return format.editStyles(label, (ctx) => walkRanges(ctx, (r, c) => {
        ctx.patch(gridUtils.refOf(r, c), (st) => {
          st[key] = toggled;
        });
      }));
    }

    // Sets the color `key` (`fill` or `fontColor`) of every cell of the selection; "" removes it. The same color
    // again is not a toggle: a color button applies what it shows.
    function paint(key, label, rgb) {
      const value = rgb ? look.normalizeColor(rgb) : "";
      if (rgb && !value) return Promise.resolve(null);
      return format.editStyles(label, (ctx) => walkRanges(ctx, (r, c) => {
        ctx.patch(gridUtils.refOf(r, c), (st) => {
          st[key] = value;
        });
      }));
    }

    function applyMergeChange(ctx, mergeOp) {
      const sheet = getActiveSheetName();
      const ranges = ctx.ranges || [ctx.rg];
      if (mergeOp === "unmerge") {
        let count = 0;
        for (let i = 0; i < ranges.length; i += 1) {
          const rg = ranges[i];
          if (layoutApi.mergesIn(sheet, rg.r1, rg.c1, rg.r2, rg.c2).length > 0) {
            layoutApi.removeMerges(sheet, rg.r1, rg.c1, rg.r2, rg.c2);
            count += 1;
          }
        }
        if (count > 0) grid?.layoutChanged?.();
        return null;
      }
      if (mergeOp === "merge") {
        for (let i = 0; i < ranges.length; i += 1) {
          const rg = ranges[i];
          if (rg.r1 === rg.r2 && rg.c1 === rg.c2) continue;
          if (layoutApi.tablesIn(sheet, rg.r1, rg.c1, rg.r2, rg.c2).length > 0) {
            return "표 안의 칸은 병합할 수 없음";
          }
        }
        let changed = false;
        for (let i = 0; i < ranges.length; i += 1) {
          const rg = ranges[i];
          if (rg.r1 === rg.r2 && rg.c1 === rg.c2) continue;
          for (const ref of Object.keys(ctx.map)) {
            const p = gridUtils.parseRef(ref);
            if (!p || (p.row === rg.r1 && p.col === rg.c1)) continue;
            if (p.row >= rg.r1 && p.row <= rg.r2 && p.col >= rg.c1 && p.col <= rg.c2) {
              ctx.map[ref] = { value: "", formula: "", style: ctx.map[ref].style };
            }
          }
          layoutApi.addMerge(sheet, rg.r1, rg.c1, rg.r2, rg.c2);
          changed = true;
        }
        if (changed) grid?.layoutChanged?.();
      }
      return null;
    }

    // The selected cells with `values` written by `{ textAlign, verticalAlign, wrap, textRotation, fontFamily, fontSize, fill, fontColor }`
    // in one step (the dialog), and `locks` `{ locked, hideFormula }` (PD-216), plus optional `mergeOp` ("merge"|"unmerge").
    function setLook(values, border, locks, mergeOp = null) {
      const lock = Object.keys(locks || {}).length > 0;
      const hasValues = Object.keys(values || {}).length > 0;
      return format.editStyles(LABELS.align, (ctx) => {
        if (mergeOp) {
          const mergeErr = applyMergeChange(ctx, mergeOp);
          if (mergeErr) return mergeErr;
        }
        let n = 0;
        if (hasValues) {
          n = walkRanges(ctx, (r, c) => {
            ctx.patch(gridUtils.refOf(r, c), (st) => Object.assign(st, values));
          });
        }
        let refused = null;
        if (border) {
          const ranges = ctx.ranges || [ctx.rg];
          for (let i = 0; i < ranges.length; i += 1) {
            const stepRes = applyBorders({ map: ctx.map, rg: ranges[i], patch: ctx.patch }, border);
            if (typeof stepRes === "string") {
              refused = stepRes;
              break;
            }
            n += stepRes;
          }
        }
        if (lock) {
          const ranges = ctx.ranges || [ctx.rg];
          for (let i = 0; i < ranges.length; i += 1) {
            n = Math.max(n, applyLocks({ map: ctx.map, rg: ranges[i], patch: ctx.patch }, locks));
          }
        }
        if (mergeOp) n = Math.max(n, 1);
        return refused || n;
      }, lock);
    }

    // ---- the lock and the hidden formula of the cells (PD-216) ----------------------------------------------
    const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;

    // `"1"` when every cell of the selection is locked, `""` when none is, `"mixed"`; the same for hidden formulas.
    function protectStates(rgOrList) {
      const rg = Array.isArray(rgOrList) ? rgOrList[0] : rgOrList;
      const sheet = getActiveSheetName();
      const map = sheetDataStore[sheet] || {};
      const axis = getSelection?.().axis;
      const area = (rg.r2 - rg.r1 + 1) * (rg.c2 - rg.c1 + 1);
      let locked;
      let hidden;
      const seen = (prev, value) => (prev === undefined ? value : prev === value ? value : "mixed");
      if (area > STATE_SCAN_CELLS) {
        // A whole row or column is judged by its runs and the cells that carry a flag of their own.
        const runs = layoutApi.unlockedOf(sheet, axis === "row" ? "row" : "col");
        const [a, b] = axis === "row" ? [rg.r1, rg.r2] : [rg.c1, rg.c2];
        const covered = axis ? runs.some((r) => r[0] <= a && r[1] >= b) : false;
        const explicit = Object.values(map).some((e) => e?.style?.unlocked || e?.style?.locked);
        return { locked: covered && !explicit ? "" : explicit || !axis ? "mixed" : "1", hidden: "mixed" };
      }
      for (let r = rg.r1; r <= rg.r2; r += 1) {
        for (let c = rg.c1; c <= rg.c2; c += 1) {
          const st = map[gridUtils.refOf(r, c)]?.style;
          locked = seen(locked, protectApi.cellUnlocked(sheet, r, c, st) ? "" : "1");
          hidden = seen(hidden, st?.hideFormula ? "1" : "");
        }
      }
      return { locked: locked ?? "1", hidden: hidden ?? "" };
    }

    // Writes `locks` (`{ locked, hideFormula }`, each a boolean or absent) into the cells of an `editStyles` step. A
    // whole row or column also takes the lock as its own, so the cells beyond the used part follow it.
    function applyLocks({ map, rg, patch }, locks) {
      const sheet = getActiveSheetName();
      const axis = getSelection?.().axis;
      const whole = getRange();
      let n = 0;
      if ("locked" in locks && axis) {
        const kind = axis === "row" ? "row" : "col";
        const [a, b] = kind === "row" ? [whole.r1, whole.r2] : [whole.c1, whole.c2];
        const runs = layoutApi.unlockedOf(sheet, kind);
        layoutApi.setUnlocked(sheet, kind, locks.locked ? layoutApi.removeRun(runs, a, b) : layoutApi.addRun(runs, a, b));
        // The cells in the range carry no flag of their own now: they follow the row or column.
        for (const ref of Object.keys(map)) {
          const at = gridUtils.parseRef(ref);
          const inside = at && at.row >= rg.r1 && at.row <= rg.r2 && at.col >= rg.c1 && at.col <= rg.c2;
          if (inside && (map[ref].style?.unlocked || map[ref].style?.locked)) {
            patch(ref, (st) => {
              st.unlocked = "";
              st.locked = "";
            });
            n += 1;
          }
        }
      }
      for (let r = rg.r1; r <= rg.r2; r += 1) {
        for (let c = rg.c1; c <= rg.c2; c += 1) {
          patch(gridUtils.refOf(r, c), (st) => {
            if ("locked" in locks && !axis) {
              const inRun = protectApi.cellUnlocked(sheet, r, c, { unlocked: false, locked: false });
              st.unlocked = locks.locked ? "" : true;
              st.locked = locks.locked && inRun ? true : "";
            }
            if ("hideFormula" in locks) st.hideFormula = locks.hideFormula ? true : "";
          });
          n += 1;
        }
      }
      return n;
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
    function borderState(rgOrList) {
      const rg = Array.isArray(rgOrList) ? rgOrList[0] : rgOrList;
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
      const [colorKind, colorValue = ""] = action.split(":");
      const color = COLOR_ACTIONS[colorKind];
      if (color && action.includes(":")) return paint(color[0], color[1], colorValue);
      if (action === "fill-none") return paint("fill", LABELS.fill, "");
      if (action === "font-color-auto") return paint("fontColor", LABELS.fontColor, "");
      if (action.startsWith("dialog-")) return openDialog?.(action.slice(7));
      return null;
    }

    // Pressed and checked states of the buttons and menu items that carry `data-sheet-look`.
    function sync() {
      if (getActiveModule() !== "SHEET") return;
      const state = states(typeof getRanges === "function" ? getRanges() : getRange());
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
    return { run, paint, setLook, applyBorders, borderState, protectStates, setColor, setLine, sync, pen, states };
  }

  host.__EASYOFFICE_CREATE_SHEET_LOOK_OPS__ = createSheetLookOps;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
