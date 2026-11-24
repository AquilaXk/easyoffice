(function (host) {
  "use strict";

  const CELL_OR_INFO_RE = /\b(?:CELL|INFO)\s*\(/i;
  const PLACEHOLDER_TITLES = new Set(["제목 없음", "변환_문서", "Untitled"]);
  const ALIGN_TAGS = { left: "l", center: "c", right: "r" };
  const MAX_LOCK_CELLS = 1000000;

  // The session is told the rows a user and a filter hid (PD-232): `SUBTOTAL` and `AGGREGATE` leave them out, and the engine calculates
  // only the formulas that use them again. `session.hiddenRows` holds what each sheet was told; `sync` sends the sheets whose rows
  // differ (before the cells, with the merges and tables) and `stale` says whether one differs now.
  //
  // The same module also lazily builds and syncs per-sheet cell formats, widths, alignments, locks, scroll origins, and safe document
  // base filename whenever the workbook holds a `CELL(...)` or `INFO(...)` formula (PD-331).
  function createSheetHiddenRowsSync({ session, sessionCall, isFormulaStr, takeSpills, getColumnWidths, getFileName }) {
    const layoutApi = () => host.__EASYOFFICE_SHEET_LAYOUT__;

    function hasCellOrInfoFormula(sheetsMap) {
      for (const name of Object.keys(sheetsMap || {})) {
        const sheet = sheetsMap[name];
        if (!sheet) continue;
        for (const ref of Object.keys(sheet)) {
          const formula = sheet[ref]?.formula;
          if (typeof formula === "string" && CELL_OR_INFO_RE.test(formula)) return true;
        }
      }
      const defs = host.__EASYOFFICE_SHEET_NAMES__?.getAll?.();
      if (Array.isArray(defs)) {
        for (const def of defs) {
          if (typeof def?.refersTo === "string" && CELL_OR_INFO_RE.test(def.refersTo)) return true;
        }
      }
      return false;
    }

    function safeFileName() {
      const raw = typeof getFileName === "function" ? getFileName() : "";
      const clean = String(raw || "").replace(/×$/, "").trim();
      const base = clean.split(/[\\/]/).pop()?.trim() || "";
      return !base || PLACEHOLDER_TITLES.has(base) ? "" : base;
    }

    function collectMetaLock(name, sheetMap, layout) {
      const spillApi = host.__EASYOFFICE_SHEET_SPILL__;
      const parseRef = spillApi?.parseRef;
      if (!parseRef) return null;
      const unlocked = [];
      const locked = [];
      for (const ref of Object.keys(sheetMap || {})) {
        const style = sheetMap[ref]?.style;
        if (!style || (!style.unlocked && !style.locked)) continue;
        const at = parseRef(ref);
        if (at) (style.unlocked ? unlocked : locked).push([at.row, at.col, at.col]);
      }
      const p = layout?.protectOf?.(name);
      if (p?.on === true) {
        for (const range of p.ranges || []) {
          if (range.pwd) continue;
          for (const box of range.sqref || []) {
            const [a, b = a] = String(box).split(":");
            const from = parseRef(a);
            const to = parseRef(b);
            for (let r = from?.row; from && to && r <= to.row && unlocked.length < MAX_LOCK_CELLS; r += 1) {
              unlocked.push([r, Math.min(from.col, to.col), Math.max(from.col, to.col)]);
            }
          }
        }
      }
      const rows = (layout?.unlockedOf?.(name, "row") || []).map((r) => [r[0], r[1]]);
      const cols = (layout?.unlockedOf?.(name, "col") || []).map((r) => [r[0], r[1]]);
      if (!unlocked.length && !locked.length && !rows.length && !cols.length) return null;
      return { unlocked, locked, rows, cols };
    }

    function buildSheetMeta(name, sheetMap, layout) {
      const numFmts = [];
      const nfIds = new Map();
      const cells = {};
      for (const ref of Object.keys(sheetMap || {})) {
        const st = sheetMap[ref]?.style;
        if (!st) continue;
        const code = typeof st.numFmt === "string" ? st.numFmt.trim() : "";
        const ha = ALIGN_TAGS[st.textAlign] || "";
        if (!code && !ha) continue;
        const item = {};
        if (code) {
          let id = nfIds.get(code);
          if (!id) {
            id = numFmts.length + 1;
            numFmts.push(code);
            nfIds.set(code, id);
          }
          item.nf = id;
        }
        if (ha) item.ha = ha;
        cells[ref] = item;
      }
      const colWidths = (typeof getColumnWidths === "function" ? getColumnWidths()?.[name] : null) || {};
      const cols = Object.keys(colWidths)
        .map(Number)
        .filter((c) => Number.isInteger(c) && c >= 1 && Number(colWidths[c]) > 0)
        .sort((a, b) => a - b)
        .map((c) => [c, Math.round(Number(colWidths[c]))]);
      const lock = collectMetaLock(name, sheetMap, layout);
      const sheetLayout = layout?.get?.(name);
      const hiddenCols = (sheetLayout?.hiddenCols || []).map((r) => [r[0], r[1]]);
      const df = host.__EASYOFFICE_SHEET_DEFAULT_FONT__?.get?.();
      const defaultColPx = layout?.defaultColPxOf?.(name);
      const top = host.__EASYOFFICE_SHEET_VIEW_STATE__?.get?.(name)?.top;
      const out = {};
      if (numFmts.length) out.numFmts = numFmts;
      if (Object.keys(cells).length) out.cells = cells;
      if (lock) out.lock = lock;
      if (cols.length) out.cols = cols;
      if (Number.isInteger(defaultColPx) && defaultColPx !== 72) out.defaultColPx = defaultColPx;
      if (df?.name && df?.size) out.defaultFont = { name: df.name, size: df.size };
      if (hiddenCols.length) out.hiddenCols = hiddenCols;
      if (Array.isArray(top) && top.length === 2 && (top[0] !== 1 || top[1] !== 1)) out.origin = [top[0], top[1]];
      return Object.keys(out).length ? out : null;
    }

    function buildMetaPayload(names, sheetsMap) {
      if (!hasCellOrInfoFormula(sheetsMap)) return null;
      const layout = layoutApi();
      const cellMeta = {};
      for (const name of names) {
        const meta = buildSheetMeta(name, sheetsMap[name], layout);
        if (meta) cellMeta[name] = meta;
      }
      const fileName = safeFileName();
      return {
        ...(fileName ? { fileName } : { fileName: "" }),
        cellMeta,
      };
    }

    // Returns whether the answers changed a cell of `sheetsMap`.
    async function sync(names, sheetsMap) {
      const layout = layoutApi();
      let changed = false;
      for (const name of names) {
        const runs = layout.hiddenRunsOf(name);
        if (layout.sameHiddenRuns(session.hiddenRows.get(name) ?? layout.NO_RUNS, runs)) continue;
        const res = await sessionCall({ sessionAction: "hiddenRows", session: session.id, sheet: name, hiddenRows: runs });
        if (!res || res.expired === true) throw new Error("시트 세션이 만료되었습니다.");
        if (res.refused) continue;
        session.hiddenRows.set(name, runs);
        for (const c of Array.isArray(res.changed) ? res.changed : []) {
          session.values.set(c.sheet + "!" + c.cell, c.value);
          const held = sheetsMap[c.sheet]?.[c.cell];
          if (held && isFormulaStr(held.formula || "")) held.value = String(c.value);
          changed = true;
        }
      }
      return changed;
    }

    async function syncMeta(names, sheetsMap) {
      const payloadObj = buildMetaPayload(names, sheetsMap);
      if (!payloadObj) return false;
      const serialized = JSON.stringify(payloadObj);
      if (session.cellMeta === serialized) return false;
      const res = await sessionCall({ sessionAction: "cellMeta", session: session.id, ...payloadObj });
      if (!res || res.expired === true) throw new Error("시트 세션이 만료되었습니다.");
      if (res.refused) return false;
      session.cellMeta = serialized;
      let changed = false;
      for (const c of Array.isArray(res.changed) ? res.changed : []) {
        session.values.set(c.sheet + "!" + c.cell, c.value);
        const held = sheetsMap[c.sheet]?.[c.cell];
        if (held && isFormulaStr(held.formula || "")) held.value = String(c.value);
        changed = true;
      }
      if (typeof takeSpills === "function" && takeSpills(sheetsMap, res)) changed = true;
      return changed;
    }

    function metaField(names, sheetsMap) {
      return buildMetaPayload(names, sheetsMap) || {};
    }

    function metaStale(sheetsMap) {
      if (session.id === null || !session.sheetKey) return false;
      const names = session.sheetKey.split("\u0000");
      const payloadObj = buildMetaPayload(names, sheetsMap || {});
      return Boolean(payloadObj) && session.cellMeta !== JSON.stringify(payloadObj);
    }

    // Whether the session holds other hidden rows of the sheet than the layout has now (a hide, a filter or an undo changed them).
    // False without a session or for a sheet it does not hold.
    function stale(name) {
      const layout = layoutApi();
      if (session.id === null || !layout?.hiddenRunsOf || !session.sheetKey.split("\u0000").includes(name)) return false;
      return !layout.sameHiddenRuns(session.hiddenRows.get(name) ?? layout.NO_RUNS, layout.hiddenRunsOf(name));
    }

    host.__EASYOFFICE_SHEET_CELL_META__ = { metaField };

    return { sync, stale, syncMeta, metaField, metaStale };
  }

  host.__EASYOFFICE_CREATE_SHEET_HIDDEN_ROWS_SYNC__ = createSheetHiddenRowsSync;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
