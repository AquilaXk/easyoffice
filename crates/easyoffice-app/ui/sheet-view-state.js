(function (host) {
  "use strict";

  // What each sheet remembers about how it was left (PD-208): the selected cells, the zoom and how far it
  // is scrolled. It is the `view` of a sheet in the open answer and the save request, `{ active: [row, col],
  // range: [row1, col1, row2, col2], zoom, top: [row, col] }`, 1-based, a key left out at its default
  // (A1, A1, 100, A1). The store is kept apart from the layout on purpose: a view is not an undo step and
  // does not make the document unsaved, as in the spreadsheet program, so no history snapshot holds it.
  // A sheet that is renamed or copied hands its view to the new name; the old name keeps its entry, so an undo that gives
  // the name back (or the sheet back, after a deletion or a copy that goes) finds the view where it was, and an insert
  // clears the name it takes.

  const MIN_ZOOM = 10;
  const MAX_ZOOM = 400;

  const bySheet = Object.create(null);

  const isCell = (list, rows, cols) =>
    Array.isArray(list) && list.length === 2 && Number.isInteger(list[0]) && Number.isInteger(list[1]) && list[0] >= 1 && list[0] <= rows && list[1] >= 1 && list[1] <= cols;

  // `json` reduced to what a sheet may carry; anything out of its bound is dropped, never clamped.
  function clean(json, rows, cols) {
    const out = {};
    if (isCell(json?.active, rows, cols)) out.active = [...json.active];
    const r = json?.range;
    if (Array.isArray(r) && r.length === 4 && isCell([r[0], r[1]], rows, cols) && isCell([r[2], r[3]], rows, cols)) {
      const range = [Math.min(r[0], r[2]), Math.min(r[1], r[3]), Math.max(r[0], r[2]), Math.max(r[1], r[3])];
      const [row, col] = out.active || [range[0], range[1]];
      if (row >= range[0] && row <= range[2] && col >= range[1] && col <= range[3]) {
        out.range = range;
        if (!out.active && (range[0] !== 1 || range[1] !== 1)) out.active = [range[0], range[1]];
      } else delete out.active;
    }
    // A range of the active cell alone and an active cell at A1 are the defaults.
    if (out.range && out.range[0] === out.range[2] && out.range[1] === out.range[3] && out.range[0] === (out.active || [1, 1])[0] && out.range[1] === (out.active || [1, 1])[1]) delete out.range;
    if (out.active && !out.range && out.active[0] === 1 && out.active[1] === 1) delete out.active;
    if (Number.isInteger(json?.zoom) && json.zoom >= MIN_ZOOM && json.zoom <= MAX_ZOOM && json.zoom !== 100) out.zoom = json.zoom;
    if (isCell(json?.top, rows, cols) && (json.top[0] !== 1 || json.top[1] !== 1)) out.top = [...json.top];
    return out;
  }

  function limits() {
    const utils = host.__EASYOFFICE_SHEET_GRID_UTILS__;
    return [utils?.MAX_ROWS || 1048576, utils?.MAX_COLS || 16384];
  }

  const get = (name) => bySheet[name] || null;

  function set(name, json) {
    const view = clean(json, ...limits());
    if (Object.keys(view).length > 0) bySheet[name] = view;
    else delete bySheet[name];
  }

  const remove = (name) => void delete bySheet[name];

  function copy(from, to) {
    if (bySheet[from]) bySheet[to] = structuredClone(bySheet[from]);
    else delete bySheet[to];
  }

  function replaceAll(all) {
    for (const name of Object.keys(bySheet)) delete bySheet[name];
    for (const name of Object.keys(all || {})) set(name, all[name]);
  }

  const getAll = () => structuredClone({ ...bySheet });

  // The view of a sheet as the grid has it now: `selection` is the input's `{ ar, ac, cr, cc }`, `top` the first
  // scrolled `{ row, col }` and `zoom` percent.
  function fromLive({ selection, top, zoom }) {
    const range = [Math.min(selection.ar, selection.cr), Math.min(selection.ac, selection.cc), Math.max(selection.ar, selection.cr), Math.max(selection.ac, selection.cc)];
    return { active: [selection.ar, selection.ac], range, zoom, top: [top.row, top.col] };
  }

  // What the grid is given back: the anchor and the opposite corner of the range, the zoom and the scroll cell. A sheet
  // with no view is at A1, 100 percent, unscrolled.
  function toLive(json) {
    const view = json || {};
    const [ar, ac] = view.active || [1, 1];
    const [r1, c1, r2, c2] = view.range || [ar, ac, ar, ac];
    return {
      selection: { ar, ac, cr: ar === r1 ? r2 : r1, cc: ac === c1 ? c2 : c1 },
      zoom: view.zoom || 100,
      top: { row: view.top?.[0] || 1, col: view.top?.[1] || 1 },
    };
  }

  host.__EASYOFFICE_SHEET_VIEW_STATE__ = { MIN_ZOOM, MAX_ZOOM, get, set, remove, copy, replaceAll, getAll, fromLive, toLive };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
