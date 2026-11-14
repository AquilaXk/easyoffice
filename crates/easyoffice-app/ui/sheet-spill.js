(function (host) {
  "use strict";

  // Arrays that spill (PD-217). The engine decides where the array a formula calculated goes; the page keeps
  // the result in the sheet model next to the cells the user wrote: the anchor's entry carries the size
  // (`spillRows`, `spillCols`) and every cell the array filled is an entry with its value and the anchor's
  // address (`spillOf`). Those entries are results, not input: the page never sends them to the engine, and a
  // cell the user types into stops being one (the engine then answers `#SPILL!` for the array).

  const MAX_ROW = 1048576;
  const MAX_COL = 16384;

  function colNumber(letters) {
    let n = 0;
    for (let i = 0; i < letters.length; i += 1) n = n * 26 + (letters.charCodeAt(i) - 64);
    return n;
  }

  function colLetters(n) {
    let out = "";
    for (let v = n; v > 0; v = Math.floor((v - 1) / 26)) out = String.fromCharCode(65 + ((v - 1) % 26)) + out;
    return out;
  }

  function parseRef(ref) {
    const m = /^([A-Z]{1,3})([1-9][0-9]*)$/.exec(String(ref || "").toUpperCase());
    if (!m) return null;
    const row = Number(m[2]);
    const col = colNumber(m[1]);
    return row <= MAX_ROW && col <= MAX_COL ? { row, col } : null;
  }

  const refOf = (row, col) => colLetters(col) + row;

  const isChild = (entry) => Boolean(entry && entry.spillOf);
  const isAnchor = (entry) => Boolean(entry && entry.spillRows > 0);

  // The area a cell belongs to: `{ anchor, r0, c0, r1, c1 }` (1-based, inclusive), `null` for a cell that is no part of one.
  // `source` is a sheet model or a function that gives the entry of a ref.
  function areaOf(source, ref) {
    const get = typeof source === "function" ? source : (r) => source?.[r];
    const entry = get(ref);
    const anchorRef = isChild(entry) ? entry.spillOf : isAnchor(entry) ? ref : null;
    const anchor = anchorRef ? get(anchorRef) : null;
    const at = anchorRef ? parseRef(anchorRef) : null;
    if (!at || !isAnchor(anchor)) return null;
    return { anchor: anchorRef, r0: at.row, c0: at.col, r1: at.row + anchor.spillRows - 1, c1: at.col + anchor.spillCols - 1 };
  }

  function forEachChild(anchorRef, rows, cols, visit) {
    const at = parseRef(anchorRef);
    if (!at) return;
    for (let r = 0; r < rows; r += 1) {
      for (let c = 0; c < cols; c += 1) {
        if (r + c > 0 && at.row + r <= MAX_ROW && at.col + c <= MAX_COL) visit(refOf(at.row + r, at.col + c));
      }
    }
  }

  // A cell the array stops filling goes back to what it was before: empty, with the format it has (the lock of a cell
  // on a protected sheet is part of its format) and its hyperlink (PD-222).
  function release(map, ref) {
    const held = map[ref];
    if (held.style || held.link) {
      const { style, link } = held;
      map[ref] = { value: "", formula: "", ...(style ? { style } : {}), ...(link ? { link } : {}) };
    } else {
      delete map[ref];
    }
  }

  // Forgets the cells the array of `anchorRef` filled.
  function clearArea(map, anchorRef) {
    const anchor = map[anchorRef];
    if (!isAnchor(anchor)) {
      // The formula was edited or removed, so its entry no longer says how big the area was: look for its cells.
      for (const ref of Object.keys(map)) if (map[ref]?.spillOf === anchorRef) release(map, ref);
      return;
    }
    forEachChild(anchorRef, anchor.spillRows, anchor.spillCols, (ref) => {
      if (map[ref]?.spillOf === anchorRef) release(map, ref);
    });
    delete anchor.spillRows;
    delete anchor.spillCols;
  }

  // Fills the area of the array at `anchorRef`; a cell the user wrote stays as it is. A cell that holds only a format (or a
  // hyperlink) is an empty cell: the array fills it and the cell keeps its format, so a cell unlocked on a protected sheet
  // is still unlocked with a value in it (PD-222).
  function fillArea(map, anchorRef, rows, cols, valueOf) {
    const anchor = map[anchorRef];
    if (!anchor) return;
    anchor.spillRows = rows;
    anchor.spillCols = cols;
    forEachChild(anchorRef, rows, cols, (ref) => {
      const held = map[ref];
      if (held && !held.spillOf && (held.value || held.formula)) return;
      const value = valueOf(ref);
      const child = { value: value === undefined || value === null ? "" : String(value), formula: "", spillOf: anchorRef };
      if (held?.style) child.style = held.style;
      if (held?.link) child.link = held.link;
      map[ref] = child;
    });
  }

  // One entry of the engine's `spills` list: `{ cell, rows, cols, blocked }` (`rows` 0: the formula no longer
  // spills). `valueOf(ref)` gives the value of a cell the array filled.
  function applyInfo(map, info, valueOf) {
    clearArea(map, info.cell);
    if (info.rows > 0 && info.blocked !== true) fillArea(map, info.cell, info.rows, info.cols, valueOf);
  }

  // The new value of a cell an array filled; false when the cell is not one.
  function setValue(map, ref, value) {
    const entry = map?.[ref];
    if (!isChild(entry)) return false;
    entry.value = String(value ?? "");
    return true;
  }

  // The whole picture from a stateless answer (`{ sheet, cell, rows, cols, blocked, values }` per array, row by
  // row): every area is forgotten and the listed ones are filled again.
  function replaceAll(sheets, spills) {
    for (const map of Object.values(sheets || {})) {
      for (const ref of Object.keys(map || {})) {
        const entry = map[ref];
        if (isChild(entry)) release(map, ref);
        else if (isAnchor(entry)) {
          delete entry.spillRows;
          delete entry.spillCols;
        }
      }
    }
    for (const spill of Array.isArray(spills) ? spills : []) {
      const map = sheets?.[spill.sheet];
      if (!map || spill.blocked === true || !(spill.rows > 0)) continue;
      const at = parseRef(spill.cell);
      if (!at) continue;
      fillArea(map, spill.cell, spill.rows, spill.cols, (ref) => {
        const p = parseRef(ref);
        return spill.values?.[(p.row - at.row) * spill.cols + (p.col - at.col)];
      });
    }
  }

  // The refs of the areas of one sheet, for the save payload: `["A1:A3", ...]`.
  function areasOf(map) {
    const out = [];
    for (const ref of Object.keys(map || {})) {
      const entry = map[ref];
      const at = isAnchor(entry) ? parseRef(ref) : null;
      if (at && entry.spillRows * entry.spillCols > 1) {
        out.push(ref + ":" + refOf(at.row + entry.spillRows - 1, at.col + entry.spillCols - 1));
      }
    }
    return out;
  }

  // The cells of a protected sheet that may be changed, for the engine (it will not place an array over the others):
  // `{ unlocked, locked, rows, cols }`, 1-based, or `null` for a sheet that is not protected.
  const MAX_LOCK_CELLS = 1000000;
  function lockPayload(name, map, layoutApi) {
    const p = layoutApi?.protectOf?.(name);
    if (!p || p.on !== true) return null;
    const unlocked = [];
    const locked = [];
    for (const ref of Object.keys(map || {})) {
      const style = map[ref]?.style;
      if (!style || (!style.unlocked && !style.locked)) continue;
      const at = parseRef(ref);
      if (at) (style.unlocked ? unlocked : locked).push([at.row, at.col, at.col]);
    }
    // Ranges allowed without a password are open to everyone.
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
    const runs = (axis) => (layoutApi.unlockedOf(name, axis) || []).map((r) => [r[0], r[1]]);
    return { unlocked, locked, rows: runs("row"), cols: runs("col") };
  }

  // The merged ranges of a sheet, for the engine (an array is not placed across one, PD-222): `["A1:B2", ...]`.
  function mergePayload(name, layoutApi) {
    return (layoutApi?.get?.(name)?.merges || []).map((m) => refOf(m.r1, m.c1) + ":" + refOf(m.r2, m.c2));
  }

  // The tables of a sheet, for the engine: an array is not placed in or across one (PD-224), and a structured reference is
  // resolved against the table's name, rows and columns (PD-225).
  function tablePayload(name, layoutApi) {
    return (layoutApi?.get?.(name)?.tables || []).map((t) => ({
      name: t.name,
      range: t.range,
      header: t.header !== false,
      totals: t.totals === true,
      columns: t.columns,
    }));
  }

  // What the engine is told of `kind` (`merges` or `tables`) for a sheet.
  const blockerPayload = (kind, name, layoutApi) => (kind === "tables" ? tablePayload : mergePayload)(name, layoutApi);

  // The `merges` and `tables` fields of a request without a session: `{ "Sheet1": ["A1:B2"] }` (tables: their objects),
  // nothing when no sheet has one.
  function mergesField(names, layoutApi) {
    const out = {};
    for (const kind of ["merges", "tables"]) {
      const byName = {};
      for (const name of names) {
        const list = blockerPayload(kind, name, layoutApi);
        if (list.length) byName[name] = list;
      }
      if (Object.keys(byName).length) out[kind] = byName;
    }
    return out;
  }

  // The engine is not told about cells an array filled.
  const isInput = (entry) => !isChild(entry);

  // Whether the text of a cell the user left unchanged still stands for the entry the array filled.
  const stillResult = (entry, rawText) => isChild(entry) && String(entry.value) === String(rawText);

  host.__EASYOFFICE_SHEET_SPILL__ = {
    parseRef,
    refOf,
    isChild,
    isAnchor,
    isInput,
    stillResult,
    areaOf,
    applyInfo,
    setValue,
    replaceAll,
    areasOf,
    lockPayload,
    mergePayload,
    blockerPayload,
    mergesField,
    clearArea,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
