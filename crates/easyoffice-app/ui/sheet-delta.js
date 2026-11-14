(function (host) {
  "use strict";

  // The cells an undo step brings back (PD-221). A delta holds the entries of a set of cells of one sheet as they
  // were before a change: the addresses as packed coordinates in one `Float64Array`, one flag byte per cell that
  // tells which parts the entry has, and the parts in parallel arrays (the text strings are the ones the sheet held,
  // shared, not copied). Applying a delta writes those entries back and returns the delta of what they replaced, so
  // the same shape serves undo and redo. A cell that did not exist is a flag with nothing else.
  //
  // Reference implementation: the inverse-mutation pattern of the reference spreadsheet engine A (Apache-2.0,
  // Copyright 2023-present DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9),
  // `packages/sheets/src/commands/mutations/set-range-values.mutation.ts` lines 114-154
  // (`SetRangeValuesUndoMutationFactory`: the old value of every cell the change names becomes the change of the undo)
  // and `packages/core/src/services/undoredo/undoredo.service.ts` lines 219-263 (`pushUndoRedo`: a new item empties the
  // redo stack, the oldest leaves past the limit). The sparse matrix of old values is a struct of arrays here, the
  // entries are copied only where the page changes them in place, and the limit counts bytes as well as steps.

  const { packRef, packCoord, refOf, PACK_STRIDE } = host.__EASYOFFICE_SHEET_REFS__;

  const PRESENT = 1;
  const HAS_VALUE = 2;
  const HAS_FORMULA = 4;
  const HAS_STYLE = 8;
  const HAS_LINK = 16;
  const HAS_EXTRA = 32;
  const KNOWN_KEYS = new Set(["value", "formula", "style", "link"]);
  // What a cell costs beyond the strings: the packed address (8), the flag (1) and the two text pointers (16).
  const CELL_BYTES = 25;
  const POINTER_BYTES = 8;
  const STRING_HEADER_BYTES = 16;
  // What a full entry of the sheet model costs on average (object, strings, shared style): the figure the
  // history uses for a record that holds whole sheets.
  const ENTRY_BYTES = 130;

  // A key the packed address gives back unchanged; any other key (lower case, `$`) is kept as text.
  const CANONICAL = /^[A-Z]{1,3}[1-9][0-9]*$/;

  const frozenOrCopy = (obj) => (obj && typeof obj === "object" && !Object.isFrozen(obj) ? { ...obj } : obj);

  function sameShallow(a, b) {
    if (a === b) return true;
    if (!a || !b || typeof a !== "object" || typeof b !== "object") return false;
    const ka = Object.keys(a);
    if (ka.length !== Object.keys(b).length) return false;
    for (const k of ka) if (a[k] !== b[k]) return false;
    return true;
  }

  function extrasOf(entry) {
    let out = null;
    for (const k of Object.keys(entry)) {
      if (!KNOWN_KEYS.has(k)) (out ??= {})[k] = entry[k];
    }
    return out;
  }

  // Whether two entries (either may be absent) hold the same cell.
  function sameEntry(a, b) {
    if (a === b) return true;
    if (!a || !b) return false;
    if (a.value !== b.value || a.formula !== b.formula) return false;
    if (!sameShallow(a.style, b.style) || !sameShallow(a.link, b.link)) return false;
    const ea = extrasOf(a);
    const eb = extrasOf(b);
    return ea === eb || sameShallow(ea, eb);
  }

  // ---- building ------------------------------------------------------------------------------------------------
  function builder() {
    return { packed: [], flags: [], values: [], formulas: [], styles: null, links: null, extras: null, odd: null, bytes: 0 };
  }

  function stringBytes(s) {
    return typeof s === "string" ? STRING_HEADER_BYTES + s.length * 2 : 0;
  }

  // Adds the cell `ref` as `entry` holds it now (`undefined`: no such cell).
  function add(b, ref, entry) {
    const at = b.packed.length;
    const packed = packRef(ref);
    if (packed > 0 && CANONICAL.test(ref)) {
      b.packed.push(packed);
    } else {
      (b.odd ??= []).push(ref);
      b.packed.push(-b.odd.length);
      b.bytes += stringBytes(ref);
    }
    let flag = 0;
    let value;
    let formula;
    if (entry) {
      flag = PRESENT;
      if ("value" in entry) {
        flag |= HAS_VALUE;
        value = entry.value;
        b.bytes += stringBytes(value);
      }
      if ("formula" in entry) {
        flag |= HAS_FORMULA;
        formula = entry.formula;
        b.bytes += stringBytes(formula);
      }
      if (entry.style !== undefined) {
        flag |= HAS_STYLE;
        (b.styles ??= new Array(at))[at] = frozenOrCopy(entry.style);
        b.bytes += POINTER_BYTES + (Object.isFrozen(entry.style) ? 0 : 64);
      }
      if (entry.link !== undefined) {
        flag |= HAS_LINK;
        (b.links ??= new Array(at))[at] = frozenOrCopy(entry.link);
        b.bytes += POINTER_BYTES + (Object.isFrozen(entry.link) ? 0 : 64);
      }
      const extra = extrasOf(entry);
      if (extra) {
        flag |= HAS_EXTRA;
        (b.extras ??= new Array(at))[at] = extra;
        b.bytes += 64;
      }
    }
    b.flags.push(flag);
    b.values.push(value);
    b.formulas.push(formula);
    b.bytes += CELL_BYTES;
  }

  function finish(b) {
    const n = b.packed.length;
    const lazy = (arr) => {
      if (arr) arr.length = n;
      return arr;
    };
    return {
      n,
      packed: Float64Array.from(b.packed),
      flags: Uint8Array.from(b.flags),
      values: b.values,
      formulas: b.formulas,
      styles: lazy(b.styles),
      links: lazy(b.links),
      extras: lazy(b.extras),
      odd: b.odd,
      bytes: b.bytes,
    };
  }

  function refAt(delta, i) {
    const p = delta.packed[i];
    if (p < 0) return delta.odd[-p - 1];
    const col = p % PACK_STRIDE;
    return refOf((p - col) / PACK_STRIDE, col);
  }

  function entryAt(delta, i) {
    const flag = delta.flags[i];
    if ((flag & PRESENT) === 0) return undefined;
    const out = {};
    if (flag & HAS_VALUE) out.value = delta.values[i];
    if (flag & HAS_FORMULA) out.formula = delta.formulas[i];
    if (flag & HAS_STYLE) out.style = frozenOrCopy(delta.styles[i]);
    if (flag & HAS_LINK) out.link = frozenOrCopy(delta.links[i]);
    if (flag & HAS_EXTRA) Object.assign(out, delta.extras[i]);
    return out;
  }

  const emptyDelta = () => finish(builder());

  // ---- capturing -----------------------------------------------------------------------------------------------
  // The cells named by `refs` (strings), absent ones included.
  function captureRefs(map, refs) {
    const b = builder();
    const seen = new Set();
    for (const ref of refs) {
      if (seen.has(ref)) continue;
      seen.add(ref);
      add(b, ref, map?.[ref]);
    }
    return finish(b);
  }

  // The cells of `map` that `test(row, col)` accepts, by one pass over the keys.
  function captureWhere(map, test) {
    const b = builder();
    if (map) {
      for (const ref of Object.keys(map)) {
        const packed = packRef(ref);
        if (packed <= 0) continue;
        const col = packed % PACK_STRIDE;
        if (test((packed - col) / PACK_STRIDE, col)) add(b, ref, map[ref]);
      }
    }
    return finish(b);
  }

  const captureAll = (map) => captureWhere(map, () => true);

  // The cells of the range `rg` ({ r1, c1, r2, c2 }): by address when the range is small against the sheet, else by
  // the keys. A cell the range has no entry for is captured only by address; `reconcile` adds those once the change is done.
  function captureRange(map, rg) {
    const area = (rg.r2 - rg.r1 + 1) * (rg.c2 - rg.c1 + 1);
    if (area <= 4096) {
      const refs = [];
      for (let r = rg.r1; r <= rg.r2; r += 1) for (let c = rg.c1; c <= rg.c2; c += 1) refs.push(refOf(r, c));
      return captureRefs(map, refs);
    }
    return captureWhere(map, (row, col) => row >= rg.r1 && row <= rg.r2 && col >= rg.c1 && col <= rg.c2);
  }

  // ---- reading -------------------------------------------------------------------------------------------------
  function refsOf(delta) {
    const out = new Array(delta.n);
    for (let i = 0; i < delta.n; i += 1) out[i] = refAt(delta, i);
    return out;
  }

  function* cells(delta) {
    for (let i = 0; i < delta.n; i += 1) yield [refAt(delta, i), entryAt(delta, i)];
  }

  // The same delta without the cells for which `keep(ref, entry, index)` is false.
  function filter(delta, keep) {
    const b = builder();
    for (let i = 0; i < delta.n; i += 1) {
      const ref = refAt(delta, i);
      const entry = entryAt(delta, i);
      if (keep(ref, entry, i)) add(b, ref, entry);
    }
    return finish(b);
  }

  // The cells of `a` then those of `b`.
  function concat(a, b) {
    if (b.n === 0) return a;
    const out = builder();
    for (const d of [a, b]) for (let i = 0; i < d.n; i += 1) add(out, refAt(d, i), entryAt(d, i));
    return finish(out);
  }

  // Where the cell at the packed address `packed` goes when `edit` ({ axis, op, at, count }) inserts or deletes rows or
  // columns: its new packed address, or -1 when the edit deletes it.
  function shiftPacked(edit, packed) {
    const col = packed % PACK_STRIDE;
    const row = (packed - col) / PACK_STRIDE;
    const rowAxis = edit.axis === "row";
    const index = rowAxis ? row : col;
    let moved = index;
    if (edit.op === "insert") {
      if (index >= edit.at) moved = index + edit.count;
    } else if (index >= edit.at + edit.count) {
      moved = index - edit.count;
    } else if (index >= edit.at) {
      return -1;
    }
    return moved === index ? packed : rowAxis ? packCoord(moved, col) : packCoord(row, moved);
  }

  const refOfPacked = (packed) => {
    const col = packed % PACK_STRIDE;
    return refOf((packed - col) / PACK_STRIDE, col);
  };

  // Where the cell `ref` goes under `edit` (`null` when the edit deletes it).
  function shiftedRef(edit, ref) {
    const packed = packRef(ref);
    if (packed <= 0) return ref;
    const moved = shiftPacked(edit, packed);
    return moved < 0 ? null : moved === packed ? ref : refOfPacked(moved);
  }

  const inverseEdit = (edit) => ({ ...edit, op: edit.op === "insert" ? "delete" : "insert" });

  // What a change did to the cells `delta` names, once the change is done and `map` holds the result: drops the cells
  // that are the same now, and, when `scan` is given, adds as absent the cells `map` holds that `delta` does not name
  // (cells the change made). `edit` is the row or column edit the change made (the cells moved with it): a cell is
  // compared where the edit put it, and one the edit deleted is kept. `scan(row, col)` limits the cells looked for.
  function reconcile(delta, map, { edit = null, scan = null } = {}) {
    const keep = new Uint8Array(delta.n);
    let dropped = 0;
    for (let i = 0; i < delta.n; i += 1) {
      let ref = refAt(delta, i);
      if (edit) ref = shiftedRef(edit, ref);
      if (ref !== null && sameEntry(entryAt(delta, i), map?.[ref])) dropped += 1;
      else keep[i] = 1;
    }
    let out = dropped === 0 ? delta : filter(delta, (_ref, _entry, i) => keep[i] === 1);
    if (scan && map) {
      const named = new Set(delta.packed);
      const back = edit ? inverseEdit(edit) : null;
      const added = builder();
      let grew = false;
      for (const ref of Object.keys(map)) {
        const packed = packRef(ref);
        if (packed <= 0 || !CANONICAL.test(ref)) continue;
        const col = packed % PACK_STRIDE;
        if (!scan((packed - col) / PACK_STRIDE, col)) continue;
        const was = back ? shiftPacked(back, packed) : packed;
        if (was >= 0 && !named.has(was)) {
          add(added, refOfPacked(was), undefined);
          grew = true;
        }
      }
      if (grew) out = concat(out, finish(added));
    }
    return out;
  }

  // ---- applying ------------------------------------------------------------------------------------------------
  // Writes the entries of `delta` into `map` and returns the delta of the cells as they were there. `onWrite(ref)` is
  // told each cell written (the page's extent of the sheet grows with them).
  function apply(map, delta, onWrite) {
    const undone = captureRefs(map, refsOf(delta));
    for (let i = 0; i < delta.n; i += 1) {
      const ref = refAt(delta, i);
      const entry = entryAt(delta, i);
      if (entry) {
        map[ref] = entry;
        onWrite?.(ref);
      } else {
        delete map[ref];
      }
    }
    return undone;
  }

  // Moves every cell of `map` for `edit` (an insert or a delete of rows or columns); the cells of a deleted band are
  // returned as a delta (`gone`) and are no longer in the map. Returns the new map.
  function shiftMap(map, edit) {
    const out = {};
    const gone = builder();
    for (const ref of Object.keys(map)) {
      const to = shiftedRef(edit, ref);
      if (to === null) add(gone, ref, map[ref]);
      else out[to] = map[ref];
    }
    return { map: out, gone: finish(gone) };
  }

  // The packed addresses a written delta leaves with content (`written`) and without any (`removed`), for the page's used
  // extent of the sheet.
  function extentChange(delta) {
    const written = [];
    const removed = [];
    for (let i = 0; i < delta.n; i += 1) {
      const packed = delta.packed[i];
      if (packed < 0) continue;
      const has = (delta.flags[i] & PRESENT) !== 0 && (delta.values[i] || delta.formulas[i]);
      (has ? written : removed).push(packed);
    }
    return { written, removed };
  }

  // The estimated bytes of a sheet model of `count` entries, for a record that holds whole sheets.
  const bytesOfEntries = (count) => count * ENTRY_BYTES;

  host.__EASYOFFICE_SHEET_DELTA__ = {
    emptyDelta,
    captureRefs,
    captureWhere,
    captureAll,
    captureRange,
    reconcile,
    concat,
    filter,
    apply,
    refsOf,
    cells,
    refAt,
    entryAt,
    sameEntry,
    shiftMap,
    shiftedRef,
    shiftPacked,
    inverseEdit,
    bytesOfEntries,
    extentChange,
    packCoord,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
