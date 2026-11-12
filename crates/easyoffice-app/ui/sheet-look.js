// 참조 구현 (reference implementation): Univer (https://github.com/dream-num/univer, Apache-2.0,
// Copyright 2023-present DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9):
//   packages/sheets/src/commands/commands/set-border.command.ts -> prepareEdgeRange (:109-176), the
//     inner edges (:257-319), outlineBorder (:353-389) and clearBorder (:389-452): an edge is written on
//     both cells that share it, and the ring around the range takes the facing side;
//   packages/engine-render/src/components/sheets/sheet.render-skeleton.ts -> _setBorderProps (:1764-1800):
//     a cell draws its own sides and the later cell wins a shared slot.
// Modifications: edges are `"<line>:<rrggbb>"` strings on a frozen per-cell object shared through the
// style intern table, the plan is a list of changed cells, there are no diagonals, and the DOM table
// draws each shared edge once in the grid line of the first cell.
//

(function (host) {
  "use strict";

  // Alignment, wrapping and borders of a sheet cell (PD-207): the style keys, the border edit
  // plans and the CSS a cell takes. A cell style carries `textAlign` (`left|center|right`),
  // `verticalAlign` (`top|middle`; the bottom is the default and has no key), `wrap` (`true`) and
  // `border`, a frozen `{ t, r, b, l }` whose edges are `"<line>:<rrggbb>"` strings (the save payload
  // uses the same text). Edges are kept on both cells that share them, so an edit of a range also
  // writes the facing side of the ring around it; where two cells disagree the cell after wins.

  const H_ALIGNS = ["left", "center", "right"];
  const V_ALIGNS = ["top", "middle"];
  const LINES = ["thin", "thick", "dashed", "dotted", "double"];
  const SIDES = ["t", "r", "b", "l"];
  const DEFAULT_COLOR = "000000";
  // CSS of a line kind: [style, width in px].
  const LINE_CSS = { thin: ["solid", 1], thick: ["solid", 2], dashed: ["dashed", 1], dotted: ["dotted", 1], double: ["double", 3] };
  const SIDE_CSS = { t: "Top", r: "Right", b: "Bottom", l: "Left" };
  const LOOK_KEYS = ["textAlign", "verticalAlign", "wrap", "border"];

  // `rrggbb` from `#rrggbb`, `rrggbb` or `#rgb` (any case); null for anything else.
  function normalizeColor(text) {
    const t = String(text ?? "").trim().replace(/^#/, "").toLowerCase();
    if (/^[0-9a-f]{6}$/.test(t)) return t;
    if (/^[0-9a-f]{3}$/.test(t)) return t[0] + t[0] + t[1] + t[1] + t[2] + t[2];
    return null;
  }

  function parseEdge(text) {
    if (typeof text !== "string") return null;
    const at = text.indexOf(":");
    if (at < 0) return null;
    const line = text.slice(0, at);
    const color = text.slice(at + 1);
    return LINES.includes(line) && /^[0-9a-f]{6}$/.test(color) ? { line, color } : null;
  }

  // The edge text of a line kind and a color; null when either is not allowed.
  function edgeText(line, color) {
    const rgb = normalizeColor(color ?? DEFAULT_COLOR);
    return LINES.includes(line) && rgb ? line + ":" + rgb : null;
  }

  // A frozen border of the valid edges of `border`, or "" when it has none.
  function freezeBorder(border) {
    if (!border || typeof border !== "object") return "";
    const out = {};
    let any = false;
    for (const side of SIDES) {
      if (parseEdge(border[side])) {
        out[side] = border[side];
        any = true;
      }
    }
    return any ? Object.freeze(out) : "";
  }

  const borderKey = (b) => (b ? (b.t || "") + "|" + (b.r || "") + "|" + (b.b || "") + "|" + (b.l || "") : "");

  // The same text for every style with this look: the part of `internStyle`'s key that is new.
  const lookKey = (st) => (st ? (st.textAlign || "") + "|" + (st.verticalAlign || "") + "|" + (st.wrap ? 1 : 0) + "|" + borderKey(st.border) : "|||0|");

  // Whether a wrapped cell was ever made in this page: a sheet is scanned for wrapped rows only then (every
  // wrapped style comes from `internStyle`, a clone of the model, or a file open, and each of them notes it).
  let wrapSeen = false;
  const markWrap = () => {
    wrapSeen = true;
  };

  const hasLook = (st) => Boolean(st) && Boolean(st.textAlign || st.verticalAlign || st.wrap || st.border);

  // `border` with the edge of `side` replaced (`null` removes it); "" when no edge is left.
  function withSide(border, side, edge) {
    const next = { ...(border || {}) };
    if (edge) next[side] = edge;
    else delete next[side];
    return freezeBorder(next);
  }

  // ---- border edit plans -------------------------------------------------------------------------
  // `kind`: `all`, `outer`, `inner`, `innerH`, `innerV`, `top`, `bottom`, `left`, `right` or `none`; `edge` is an edge text.
  // `rg` is `{ r1, c1, r2, c2 }`; `borderAt(r, c)` answers a cell's current border ("" or undefined for
  // none) and `maxRow`/`maxCol` bound the ring. The result lists `{ r, c, border }` for the cells whose
  // border changes (`border` is "" when no edge is left).
  function planBorders(rg, kind, edge, borderAt, maxRow, maxCol) {
    const changed = new Map();
    const key = (r, c) => r * 32768 + c;
    const set = (r, c, side, value) => {
      if (r < 1 || c < 1 || r > maxRow || c > maxCol) return;
      const at = key(r, c);
      const hit = changed.get(at);
      const current = hit ? hit.border : borderAt(r, c);
      if ((current?.[side] || null) === (value || null)) return;
      changed.set(at, { r, c, border: withSide(current, side, value) });
    };
    const top = (value) => {
      for (let c = rg.c1; c <= rg.c2; c += 1) {
        set(rg.r1, c, "t", value);
        set(rg.r1 - 1, c, "b", value);
      }
    };
    const bottom = (value) => {
      for (let c = rg.c1; c <= rg.c2; c += 1) {
        set(rg.r2, c, "b", value);
        set(rg.r2 + 1, c, "t", value);
      }
    };
    const left = (value) => {
      for (let r = rg.r1; r <= rg.r2; r += 1) {
        set(r, rg.c1, "l", value);
        set(r, rg.c1 - 1, "r", value);
      }
    };
    const right = (value) => {
      for (let r = rg.r1; r <= rg.r2; r += 1) {
        set(r, rg.c2, "r", value);
        set(r, rg.c2 + 1, "l", value);
      }
    };
    const innerV = (value) => {
      for (let r = rg.r1; r <= rg.r2; r += 1) {
        for (let c = rg.c1; c < rg.c2; c += 1) {
          set(r, c, "r", value);
          set(r, c + 1, "l", value);
        }
      }
    };
    const innerH = (value) => {
      for (let r = rg.r1; r < rg.r2; r += 1) {
        for (let c = rg.c1; c <= rg.c2; c += 1) {
          set(r, c, "b", value);
          set(r + 1, c, "t", value);
        }
      }
    };
    const inner = (value) => {
      innerV(value);
      innerH(value);
    };
    const outer = (value) => {
      top(value);
      bottom(value);
      left(value);
      right(value);
    };
    if (kind === "none") {
      for (let r = rg.r1; r <= rg.r2; r += 1) {
        for (let c = rg.c1; c <= rg.c2; c += 1) for (const side of SIDES) set(r, c, side, null);
      }
      outer(null);
    } else if (kind === "all") {
      outer(edge);
      inner(edge);
    } else if (kind === "outer") outer(edge);
    else if (kind === "inner") inner(edge);
    else if (kind === "top") top(edge);
    else if (kind === "bottom") bottom(edge);
    else if (kind === "left") left(edge);
    else if (kind === "right") right(edge);
    else if (kind === "innerH") innerH(edge);
    else if (kind === "innerV") innerV(edge);
    return [...changed.values()];
  }

  // ---- CSS of a cell -------------------------------------------------------------------------------
  const LINE_HEIGHT_RATIO = 1.35;
  const WRAP_PAD_V = 3;

  function edgeCss(edge) {
    const e = parseEdge(edge);
    if (!e) return null;
    const [style, width] = LINE_CSS[e.line];
    return { style, width: width + "px", color: "#" + e.color };
  }

  // Sets or clears the three border properties of one side of `td` from an edge text.
  function paintSide(td, side, edge) {
    const css = edgeCss(edge);
    const name = "border" + SIDE_CSS[side];
    td.style[name + "Style"] = css ? css.style : "";
    td.style[name + "Width"] = css ? css.width : "";
    td.style[name + "Color"] = css ? css.color : "";
  }

  // The edge each side of the cell at (row, col) takes: the right and bottom edges are drawn in the
  // grid line of this cell and come from the cell after when it has one, the top and left edges of the
  // first row and column are drawn by the cell itself. `borderAt(r, c)` is as in `planBorders`;
  // `span` is `{ r2, c2 }` of a merged range anchored here.
  function resolveEdges(row, col, borderAt, span) {
    const own = borderAt(row, col);
    const lastRow = span ? span.r2 : row;
    const lastCol = span ? span.c2 : col;
    const rightOwn = lastCol === col ? own : borderAt(row, lastCol);
    const bottomOwn = lastRow === row ? own : borderAt(lastRow, col);
    return {
      t: row === 1 ? own?.t || null : null,
      l: col === 1 ? own?.l || null : null,
      r: borderAt(row, lastCol + 1)?.l || rightOwn?.r || null,
      b: borderAt(lastRow + 1, col)?.t || bottomOwn?.b || null,
    };
  }

  // Draws the look of `st` on `td` (alignment, wrapping and the resolved edges); a cell painted before
  // with another look is reset first. `fontPx` is the line height basis of wrapped text.
  function paintTd(td, st, row, col, borderAt, span) {
    const edges = resolveEdges(row, col, borderAt, span);
    const sig = lookKey(st) + "/" + (edges.t || "") + (edges.r || "") + (edges.b || "") + (edges.l || "") + (st?.wrap ? "/" + (st.fontSize || "") : "");
    if (td._lk === sig) return;
    td._lk = sig;
    const s = td.style;
    s.textAlign = st?.textAlign || "";
    s.verticalAlign = st?.verticalAlign || "";
    for (const side of SIDES) paintSide(td, side, edges[side]);
    if (st?.wrap) {
      s.whiteSpace = "pre-wrap";
      s.overflowWrap = "anywhere";
      s.textOverflow = "clip";
      const px = Number.parseFloat(st.fontSize) || 13;
      s.lineHeight = Math.round(px * LINE_HEIGHT_RATIO) + "px";
      s.paddingTop = WRAP_PAD_V + "px";
      s.paddingBottom = WRAP_PAD_V + "px";
    } else if (s.whiteSpace) {
      s.whiteSpace = "";
      s.overflowWrap = "";
      s.textOverflow = "";
      s.lineHeight = "";
      s.paddingTop = "";
      s.paddingBottom = "";
    }
  }

  // The look of a printed cell (PD-207): alignment, wrapping and the cell's own edges through the CSS object
  // model; where two neighbours disagree the collapsed table borders decide.
  function printStyle(td, st) {
    if (!st) return;
    const s = td.style;
    if (st.textAlign) s.textAlign = st.textAlign;
    s.verticalAlign = st.verticalAlign || "bottom";
    if (st.wrap) s.whiteSpace = "pre-wrap";
    for (const side of SIDES) {
      const css = edgeCss(st.border?.[side]);
      if (!css) continue;
      const name = "border" + SIDE_CSS[side];
      s[name + "Style"] = css.style;
      s[name + "Width"] = css.width;
      s[name + "Color"] = css.color;
    }
  }

  host.__EASYOFFICE_SHEET_LOOK__ = {
    printStyle,
    H_ALIGNS,
    V_ALIGNS,
    LINES,
    SIDES,
    DEFAULT_COLOR,
    LOOK_KEYS,
    LINE_HEIGHT_RATIO,
    WRAP_PAD_V,
    normalizeColor,
    parseEdge,
    edgeText,
    freezeBorder,
    borderKey,
    lookKey,
    hasLook,
    markWrap,
    wrapSeen: () => wrapSeen,
    withSide,
    planBorders,
    resolveEdges,
    paintTd,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
