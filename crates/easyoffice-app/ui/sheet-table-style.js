(function (host) {
  "use strict";

  // How a table looks (PD-230): the style a table names (one of the 60 built-in ones of `sheet-table-presets.js`, or the
  // definition the file gave, `styleDef`) turned into the fill, text colour, bold and border edges of each cell, and the
  // lookup of the table that holds a cell. The grid asks `paintTd` for every cell it draws; the gallery asks `previewCells`.
  //
  // 참조 구현: Apache POI `poi/src/main/java/org/apache/poi/ss/usermodel/TableStyleType.java` lines 41-250 (the elements of a
  // style in the order they are applied, later ones over earlier ones; the region each one covers; a stripe starts at the
  // first row below the header and its size comes from the element; the column stripes start at the first column), Apache-2.0,
  // Copyright The Apache Software Foundation, commit 47dcff769bb395034dddfd59c4609659b6dbc6a4. Source and license text:
  // `THIRD_PARTY_NOTICES.md`.
  //
  // Modifications: the region test is answered for one cell at a time from the table's rectangle (no range objects); a cell
  // draws its own top edge only on the table's first row and its left edge only on the first column, and its bottom and right
  // edges are the edge it shares with the next cell, taken from the highest element that names it; the colours are resolved to
  // `rrggbb` once per style with the Office theme; an automatic (black) colour follows the page theme; a cell's own fill, text
  // colour and borders win over the table style, and conditional formatting wins over both (the CSS orders them).

  // The elements in the order they are applied (`StyleElement::key` in `table_style.rs`).
  const ORDER = ["w", "c1", "c2", "r1", "r2", "lc", "fc", "h", "t", "fh", "lh", "ft", "lt"];
  const [WHOLE, COL1, COL2, ROW1, ROW2, LAST_COL, FIRST_COL, HEADER, TOTAL, FIRST_HEAD, LAST_HEAD, FIRST_TOTAL, LAST_TOTAL] = ORDER.map((_, i) => i);
  const EDGES = ["t", "b", "l", "r", "h", "v"];
  const [EDGE_T, EDGE_B, EDGE_L, EDGE_R, EDGE_H, EDGE_V] = EDGES.map((_, i) => i);
  const WEIGHT_PX = { thin: 1, medium: 2, thick: 3, double: 2 };
  // Office theme slots as SpreadsheetML numbers them (light 1, dark 1, light 2, dark 2, accents 1-6).
  const THEME = ["ffffff", "000000", "e7e6e6", "44546a", "4472c4", "ed7d31", "a5a5a5", "ffc000", "5b9bd5", "70ad47", "0563c1", "954f72"];
  const BLOCK = 64; // rows per lookup bucket
  const TALL_BLOCKS = 4096; // a table taller than this many buckets is kept in a plain list
  const DARK_FILL_LUMINANCE = 62; // `sheet-look.js`: a fill at or under this is dark, its text is white
  const PRESETS = host.__EASYOFFICE_SHEET_TABLE_PRESETS__ || {};
  const layoutApi = () => host.__EASYOFFICE_SHEET_LAYOUT__;

  // ---- colours ---------------------------------------------------------------------------------------------------------
  // `rgb` lightened or darkened by `tint` in HLS (ECMA-376 18.3.1.15), as `apply_tint` of `xlsx_view.rs` does.
  function applyTint(rgb, tint) {
    if (!tint) return rgb;
    const n = Number.parseInt(rgb, 16);
    const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => v / 255);
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    let l = (max + min) / 2;
    const d = max - min;
    let h = 0;
    let s = 0;
    if (d !== 0) {
      s = d / (1 - Math.abs(2 * l - 1));
      if (max === r) h = (((g - b) / d) % 6 + 6) % 6;
      else if (max === g) h = (b - r) / d + 2;
      else h = (r - g) / d + 4;
    }
    l = tint < 0 ? l * (1 + tint) : l * (1 - tint) + tint;
    const c = (1 - Math.abs(2 * l - 1)) * s;
    const x = c * (1 - Math.abs((h % 2) - 1));
    const [r1, g1, b1] = [[c, x, 0], [x, c, 0], [0, c, x], [0, x, c], [x, 0, c], [c, 0, x]][Math.min(5, Math.floor(h))];
    const m = l - c / 2;
    const byte = (v) => Math.max(0, Math.min(255, Math.round((v + m) * 255)));
    return ((byte(r1) << 16) | (byte(g1) << 8) | byte(b1)).toString(16).padStart(6, "0");
  }

  // `rrggbb`, or `t<slot>[:tint]` (a built-in style's theme colour), as `rrggbb`; null for none.
  function colorOf(ref) {
    if (!ref) return null;
    if (ref[0] !== "t") return ref;
    const [slot, tint] = ref.slice(1).split(":");
    const base = THEME[Number(slot)];
    return base ? applyTint(base, tint ? Number(tint) : 0) : null;
  }

  // Black is the automatic colour: it follows the page theme's text (dark windows show it light), as the cell colours do.
  const AUTO = "000000";

  // ---- compiling a style -----------------------------------------------------------------------------------------------
  // An element is `{ f, c, b, i, sz, bd: [t, b, l, r, h, v] }` with the colours resolved and each edge `{ px, color }` or null.
  function compileFormat(raw) {
    const edge = (text) => {
      if (!text) return null;
      const at = text.indexOf(":");
      const weight = text.slice(0, at);
      return { px: WEIGHT_PX[weight] || 1, color: colorOf(text.slice(at + 1)) || AUTO };
    };
    const hasEdge = raw.bd && Object.keys(raw.bd).length > 0;
    return {
      f: colorOf(raw.f),
      c: colorOf(raw.c),
      b: Boolean(raw.b),
      i: Boolean(raw.i),
      sz: Math.max(1, Math.min(9, Number(raw.sz) || 1)),
      bd: hasEdge ? EDGES.map((key) => edge(raw.bd[key])) : null,
    };
  }

  // A style as an array in `ORDER` (null for an element it does not format).
  const compileStyle = (def) => ORDER.map((key) => (def?.[key] ? compileFormat(def[key]) : null));

  const compiledPresets = new Map();
  const compiledDefs = new WeakMap();

  // The compiled style a table names, or null when it names none or one nobody defined.
  function styleOf(table) {
    const name = table?.style;
    if (!name) return null;
    if (Object.hasOwn(PRESETS, name)) {
      let compiled = compiledPresets.get(name);
      if (!compiled) compiledPresets.set(name, (compiled = compileStyle(PRESETS[name])));
      return compiled;
    }
    const def = table.styleDef;
    if (!def || typeof def !== "object") return null;
    let compiled = compiledDefs.get(def);
    if (!compiled) compiledDefs.set(def, (compiled = compileStyle(def)));
    return compiled;
  }

  // ---- where a table is ------------------------------------------------------------------------------------------------
  // A table's rectangle (the layout parses it once per range text).
  const geometryOf = (table) => layoutApi().tableBox(table);

  // The tables of a sheet by bucket of 64 rows, rebuilt when the layout changes.
  const indexes = new Map();

  function indexFor(sheet) {
    const layout = layoutApi();
    const version = layout.version();
    const held = indexes.get(sheet);
    const tables = layout.tablesOf(sheet);
    if (held && held.version === version && held.tables === tables) return held;
    const index = { version, tables, blocks: new Map(), tall: [] };
    for (const table of tables) {
      const g = geometryOf(table);
      if (g.r2 < g.r1) continue;
      const first = Math.floor(g.r1 / BLOCK);
      const last = Math.floor(g.r2 / BLOCK);
      if (last - first >= TALL_BLOCKS) {
        index.tall.push(table);
        continue;
      }
      for (let b = first; b <= last; b += 1) {
        const list = index.blocks.get(b);
        if (list) list.push(table);
        else index.blocks.set(b, [table]);
      }
    }
    indexes.set(sheet, index);
    return index;
  }

  const holds = (g, row, col) => row >= g.r1 && row <= g.r2 && col >= g.c1 && col <= g.c2;

  // The table of `sheet` that has the cell (1-based), or null.
  function tableAt(sheet, row, col) {
    const index = indexFor(sheet);
    if (index.blocks.size === 0 && index.tall.length === 0) return null;
    for (const table of index.blocks.get(Math.floor(row / BLOCK)) || []) if (holds(geometryOf(table), row, col)) return table;
    for (const table of index.tall) if (holds(geometryOf(table), row, col)) return table;
    return null;
  }

  // ---- which elements format a cell ------------------------------------------------------------------------------------
  // The rows (or columns) of the stripe that has index `at` of a run that starts at `start`: [first, last] of the stripe and
  // whether it is the first kind. The sizes are those of the two stripe elements (1 for one that is missing).
  function stripeAt(at, start, end, size1, size2) {
    const k = (at - start) % (size1 + size2);
    const first = k < size1;
    const from = at - (first ? k : k - size1);
    return [from, Math.min(end, from + (first ? size1 : size2) - 1), first];
  }

  // Pushes `index, format, firstRow, lastRow, firstCol, lastCol` of every element that covers the cell, lowest first.
  function covering(out, table, g, cs, row, col) {
    out.length = 0;
    const header = table.header !== false;
    const totals = table.totals === true;
    const add = (i, r1, r2, c1, c2) => {
      if (cs[i]) out.push(i, cs[i], r1, r2, c1, c2);
    };
    add(WHOLE, g.r1, g.r2, g.c1, g.c2);
    if (table.columnStripes) {
      const [from, to, first] = stripeAt(col, g.c1, g.c2, cs[COL1]?.sz || 1, cs[COL2]?.sz || 1);
      add(first ? COL1 : COL2, g.r1, g.r2, from, to);
    }
    const firstData = g.r1 + (header ? 1 : 0);
    if (table.rowStripes && row >= firstData) {
      const [from, to, first] = stripeAt(row, firstData, g.r2, cs[ROW1]?.sz || 1, cs[ROW2]?.sz || 1);
      add(first ? ROW1 : ROW2, from, to, g.c1, g.c2);
    }
    if (table.lastColumn && col === g.c2) add(LAST_COL, g.r1, g.r2, g.c2, g.c2);
    if (table.firstColumn && col === g.c1) add(FIRST_COL, g.r1, g.r2, g.c1, g.c1);
    if (header && row === g.r1) {
      add(HEADER, g.r1, g.r1, g.c1, g.c2);
      if (col === g.c1) add(FIRST_HEAD, g.r1, g.r1, g.c1, g.c1);
      if (col === g.c2) add(LAST_HEAD, g.r1, g.r1, g.c2, g.c2);
    }
    if (totals && row === g.r2) {
      add(TOTAL, g.r2, g.r2, g.c1, g.c2);
      if (col === g.c1) add(FIRST_TOTAL, g.r2, g.r2, g.c1, g.c1);
      if (col === g.c2) add(LAST_TOTAL, g.r2, g.r2, g.c2, g.c2);
    }
  }

  const STEP = 6;

  // The edge `pick(format, box)` names, the highest element's: `[priority, edge]` or null. `list` is a `covering` result.
  function strongest(list, best, pick) {
    let found = best;
    for (let k = 0; k < list.length; k += STEP) {
      const edge = list[k + 1].bd ? pick(list[k + 1].bd, list, k) : null;
      if (edge && (!found || list[k] >= found[0])) found = [list[k], edge];
    }
    return found;
  }

  const bufA = [];
  const bufB = [];

  // The look of one cell: `{ fill, color, bold, italic, edges: [top, bottom, left, right] }` (each `{ px, color }` or null).
  function lookOf(table, g, cs, row, col) {
    covering(bufA, table, g, cs, row, col);
    const look = { fill: null, color: null, bold: false, italic: false, edges: [null, null, null, null] };
    for (let k = 0; k < bufA.length; k += STEP) {
      const f = bufA[k + 1];
      if (f.f) look.fill = f.f;
      if (f.c) look.color = f.c;
      look.bold ||= f.b;
      look.italic ||= f.i;
    }
    // The edge `own` names for this cell and, with `next` (the cell on the other side), `nextPick` names for that one.
    const edge = (nextPick, own, next) => {
      let found = strongest(bufA, null, own);
      if (next) {
        covering(bufB, table, g, cs, next[0], next[1]);
        found = strongest(bufB, found, nextPick);
      }
      return found ? found[1] : null;
    };
    // A shared edge is drawn by the cell above or to the left; the table's first row and column draw their outer edge.
    if (row === g.r1) look.edges[0] = edge(null, (bd, list, k) => bd[row === list[k + 2] ? EDGE_T : EDGE_H], null);
    if (col === g.c1) look.edges[2] = edge(null, (bd, list, k) => bd[col === list[k + 4] ? EDGE_L : EDGE_V], null);
    look.edges[1] = edge(
      (bd, list, k) => bd[row + 1 === list[k + 2] ? EDGE_T : EDGE_H],
      (bd, list, k) => bd[row === list[k + 3] ? EDGE_B : EDGE_H],
      row < g.r2 ? [row + 1, col] : null,
    );
    look.edges[3] = edge(
      (bd, list, k) => bd[col + 1 === list[k + 4] ? EDGE_L : EDGE_V],
      (bd, list, k) => bd[col === list[k + 5] ? EDGE_R : EDGE_V],
      col < g.c2 ? [row, col + 1] : null,
    );
    return look;
  }

  // The text colour of a dark or light fill (`sheet-look.js` decides the same way).
  function autoTextColor(fill) {
    const n = Number.parseInt(fill, 16);
    return ((((n >> 16) & 255) * 76 + ((n >> 8) & 255) * 151 + (n & 255) * 29) >> 8) <= DARK_FILL_LUMINANCE ? "ffffff" : "000000";
  }

  // ---- the cells of the grid -------------------------------------------------------------------------------------------
  const SHADOW_SIDE = [(px, c) => `inset 0 ${px}px 0 0 ${c}`, (px, c) => `inset 0 -${px}px 0 0 ${c}`, (px, c) => `inset ${px}px 0 0 0 ${c}`, (px, c) => `inset -${px}px 0 0 0 ${c}`];
  const cssColor = (rgb) => (rgb === AUTO ? "var(--color-text)" : "#" + rgb);

  // The inset shadows that draw the edges of a cell look (top, bottom, left, right).
  function shadowsOf(look) {
    const shadows = [];
    look.edges.forEach((e, side) => {
      if (e) shadows.push(SHADOW_SIDE[side](e.px, cssColor(e.color)));
    });
    return shadows;
  }

  function clearTd(td) {
    const s = td.style;
    s.removeProperty("--ts-fill");
    s.removeProperty("--ts-fg");
    s.removeProperty("--ts-shadow");
    delete td.dataset.tsF;
    delete td.dataset.tsC;
    delete td.dataset.tsB;
    delete td.dataset.tsI;
    td.classList.remove("ts-shadow");
    td._ts = "";
  }

  // Draws the table style on `td` (the cell at `td._row`, `td._col`): the table under the cell's own look. A cell that is
  // in no table, or whose table has no style, gets nothing (and loses what an earlier table left on the element).
  function paintTd(td, sheet) {
    const table = tableAt(sheet, td._row, td._col);
    const cs = table ? styleOf(table) : null;
    if (!cs) {
      if (td._ts) clearTd(td);
      return;
    }
    const g = geometryOf(table);
    const look = lookOf(table, g, cs, td._row, td._col);
    const own = td._st;
    const directFill = own?.fill || "";
    const directColor = own?.fontColor || "";
    // Text with a colour of its own on the table keeps it; else the table's colour; black without any fill is the theme's.
    let fg = "";
    if (!directColor) {
      if (look.color && !(look.color === AUTO && !look.fill && !directFill)) fg = look.color;
      else if (look.fill && !directFill) fg = autoTextColor(look.fill);
    }
    const shadows = shadowsOf(look);
    const sig = [look.fill || "", fg, look.bold ? 1 : 0, look.italic ? 1 : 0, shadows.join(",")].join("|");
    if (td._ts === sig) return;
    td._ts = sig;
    const s = td.style;
    if (look.fill) {
      s.setProperty("--ts-fill", "#" + look.fill);
      td.dataset.tsF = "1";
    } else {
      s.removeProperty("--ts-fill");
      delete td.dataset.tsF;
    }
    if (fg) {
      s.setProperty("--ts-fg", "#" + fg);
      td.dataset.tsC = "1";
    } else {
      s.removeProperty("--ts-fg");
      delete td.dataset.tsC;
    }
    if (look.bold) td.dataset.tsB = "1";
    else delete td.dataset.tsB;
    if (look.italic) td.dataset.tsI = "1";
    else delete td.dataset.tsI;
    if (shadows.length > 0) s.setProperty("--ts-shadow", shadows.join(", "));
    else s.removeProperty("--ts-shadow");
    td.classList.toggle("ts-shadow", shadows.length > 0);
  }

  // ---- the gallery -----------------------------------------------------------------------------------------------------
  const PREVIEW_ROWS = 5;
  const PREVIEW_COLS = 5;

  // The look of a small sample table in the style `name`: `rows` rows of `cols` `{ fill, color, edges, bold }` (the flags of
  // `options`: header, totals, firstColumn, lastColumn, rowStripes, columnStripes). `def` is the file's definition of a style
  // that is not built in. Null when the name is none.
  function previewCells(name, options, def) {
    const table = { style: name, styleDef: def, ...options };
    const cs = styleOf(table);
    if (!cs) return null;
    const g = { range: "", r1: 1, c1: 1, r2: PREVIEW_ROWS, c2: PREVIEW_COLS };
    const rows = [];
    for (let r = 1; r <= PREVIEW_ROWS; r += 1) {
      const row = [];
      for (let c = 1; c <= PREVIEW_COLS; c += 1) row.push(lookOf(table, g, cs, r, c));
      rows.push(row);
    }
    return rows;
  }

  // The names of the built-in styles by kind, in the order the gallery lists them.
  const count = (prefix) => Object.keys(PRESETS).filter((n) => n.startsWith(prefix)).length;
  const kindNames = (kind) => Array.from({ length: count("TableStyle" + kind) }, (_, i) => "TableStyle" + kind + (i + 1));
  const BUILT_IN = { light: kindNames("Light"), medium: kindNames("Medium"), dark: kindNames("Dark") };
  const DEFAULT_STYLE = "TableStyleMedium2";

  host.__EASYOFFICE_SHEET_TABLE_STYLE__ = {
    applyTint,
    colorOf,
    styleOf,
    geometryOf,
    tableAt,
    lookOf,
    paintTd,
    previewCells,
    shadowsOf,
    cssColor,
    autoTextColor,
    BUILT_IN,
    DEFAULT_STYLE,
    PREVIEW_ROWS,
    PREVIEW_COLS,
    isBuiltIn: (name) => Object.hasOwn(PRESETS, name),
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
