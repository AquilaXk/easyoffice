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
  function fillArea(map, anchorRef, rows, cols, valueOf, linkOf) {
    const anchor = map[anchorRef];
    if (!anchor) return;
    anchor.spillRows = rows;
    anchor.spillCols = cols;
    forEachChild(anchorRef, rows, cols, (ref) => {
      const held = map[ref];
      if (held && !held.spillOf && (held.value || held.formula)) return;
      const value = valueOf(ref);
      const child = { value: value === undefined || value === null ? "" : String(value), formula: "", spillOf: anchorRef };
      if (linkOf && linkOf(ref) === true) child.formulaLink = true;
      if (held?.style) child.style = held.style;
      if (held?.link) child.link = held.link;
      map[ref] = child;
    });
  }

  // One entry of the engine's `spills` list: `{ cell, rows, cols, blocked }` (`rows` 0: the formula no longer
  // spills). `valueOf(ref)` gives the value of a cell the array filled.
  function applyInfo(map, info, valueOf, linkOf) {
    clearArea(map, info.cell);
    if (info.rows > 0 && info.blocked !== true) fillArea(map, info.cell, info.rows, info.cols, valueOf, linkOf);
  }

  // The new value (and `HYPERLINK` flag, PD-236) of a cell an array filled; false when the cell is not one.
  function setValue(map, ref, value, isLink) {
    const entry = map?.[ref];
    if (!isChild(entry)) return false;
    entry.value = String(value ?? "");
    if (isLink) entry.formulaLink = true;
    else delete entry.formulaLink;
    return true;
  }

  // Preserves computed anchor metadata across DOM snapshots when `formula` is unchanged.
  function copyComputedMeta(prevEntry, cellEntry, formula) {
    if (!prevEntry || prevEntry.formula !== formula) return;
    if (isAnchor(prevEntry)) {
      cellEntry.spillRows = prevEntry.spillRows;
      cellEntry.spillCols = prevEntry.spillCols;
    }
    if (typeof prevEntry.formulaLink === "boolean") {
      cellEntry.formulaLink = prevEntry.formulaLink;
    }
  }

  function writeAnchorFormulaLink(entry, isLink) {
    if (!entry || !entry.formula) return false;
    const mayLink = host.__EASYOFFICE_SHEET_LINK__?.mayYieldHyperlink?.(entry.formula) === true;
    if (isLink) {
      if (entry.formulaLink === true) return false;
      entry.formulaLink = true;
      return true;
    }
    if (mayLink) {
      if (entry.formulaLink === false) return false;
      entry.formulaLink = false;
      return true;
    }
    if (entry.formulaLink !== undefined) {
      delete entry.formulaLink;
      return true;
    }
    return false;
  }

  // Applies `ChangedCell` `link` flags (`c.link === true`) to formula anchor entries without adding keys to plain formulas (PD-236).
  function applyChangedLinks(sheetsMap, changedList, linkMap) {
    let toggled = false;
    for (const c of Array.isArray(changedList) ? changedList : []) {
      const isLink = c.link === true;
      const key = c.sheet + "!" + c.cell;
      if (linkMap) {
        if (isLink) linkMap.set(key, true);
        else linkMap.delete(key);
      }
      if (writeAnchorFormulaLink(sheetsMap?.[c.sheet]?.[c.cell], isLink)) toggled = true;
    }
    return toggled;
  }

  // The whole picture from a stateless answer (`{ sheet, cell, rows, cols, blocked, values, links? }` per array, row by
  // row, plus optional `formulaLinks`): every area is forgotten and the listed ones are filled again.
  function replaceAll(sheets, spills, formulaLinks) {
    const linkSet = Array.isArray(formulaLinks) ? new Set(formulaLinks) : null;
    for (const [sheetName, map] of Object.entries(sheets || {})) {
      for (const ref of Object.keys(map || {})) {
        const entry = map[ref];
        if (isChild(entry)) release(map, ref);
        else {
          if (isAnchor(entry)) {
            delete entry.spillRows;
            delete entry.spillCols;
          }
          if (linkSet && entry?.formula) {
            writeAnchorFormulaLink(entry, linkSet.has(sheetName + "!" + ref));
          }
        }
      }
    }
    for (const spill of Array.isArray(spills) ? spills : []) {
      const map = sheets?.[spill.sheet];
      if (!map || spill.blocked === true || !(spill.rows > 0)) continue;
      const at = parseRef(spill.cell);
      if (!at) continue;
      const hasLinks = Array.isArray(spill.links);
      if (hasLinks && map[spill.cell]) writeAnchorFormulaLink(map[spill.cell], Boolean(spill.links[0]));
      fillArea(
        map,
        spill.cell,
        spill.rows,
        spill.cols,
        (ref) => {
          const p = parseRef(ref);
          return spill.values?.[(p.row - at.row) * spill.cols + (p.col - at.col)];
        },
        hasLinks || linkSet
          ? (ref) => {
              if (hasLinks) {
                const p = parseRef(ref);
                return Boolean(spill.links[(p.row - at.row) * spill.cols + (p.col - at.col)]);
              }
              return linkSet.has(spill.sheet + "!" + ref);
            }
          : null
      );
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

  // The pivot tables of a sheet, for `GETPIVOTDATA` (PD-292): where each one is, its source, its fields and its filters, and
  // nothing else (a name or a field name can be long and the engine does not read them). A table with no source range or no
  // value field has nothing to answer with and is not sent.
  function pivotPayload(name, layoutApi) {
    return (layoutApi?.get?.(name)?.pivots || [])
      .filter((p) => p.sourceRange && p.valueFields?.length)
      .map((p) => ({
        sourceSheet: p.sourceSheet,
        sourceRange: p.sourceRange,
        hasHeader: p.hasHeader,
        location: p.location,
        rowFields: p.rowFields,
        colFields: p.colFields,
        valueFields: p.valueFields,
        filters: p.filters,
      }));
  }

  const PAYLOADS = { merges: mergePayload, tables: tablePayload, pivots: pivotPayload };

  // What the engine is told of `kind` (`merges`, `tables` or `pivots`) for a sheet.
  const blockerPayload = (kind, name, layoutApi) => PAYLOADS[kind](name, layoutApi);

  // The `merges`, `tables` and `pivots` fields of a request without a session: `{ "Sheet1": ["A1:B2"] }` (tables and pivots: their
  // objects), nothing when no sheet has one.
  function mergesField(names, layoutApi) {
    const out = {};
    for (const kind of Object.keys(PAYLOADS)) {
      const byName = {};
      for (const name of names) {
        const list = blockerPayload(kind, name, layoutApi);
        if (list.length) byName[name] = list;
      }
      if (Object.keys(byName).length) out[kind] = byName;
    }
    return out;
  }

  // The `hiddenRows` field of a request without a session: `{ "Sheet1": { hidden, filtered } }` for the sheets that hide rows (PD-232).
  function hiddenRowsField(names, layoutApi) {
    const byName = {};
    for (const name of names) {
      const runs = layoutApi?.hiddenRunsOf?.(name);
      if (runs && (runs.hidden.length || runs.filtered.length)) byName[name] = runs;
    }
    return Object.keys(byName).length ? { hiddenRows: byName } : {};
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
    copyComputedMeta,
    writeAnchorFormulaLink,
    applyChangedLinks,
    replaceAll,
    areasOf,
    lockPayload,
    mergePayload,
    pivotPayload,
    blockerPayload,
    mergesField,
    hiddenRowsField,
    clearArea,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
