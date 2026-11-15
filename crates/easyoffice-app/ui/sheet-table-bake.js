(function (host) {
  "use strict";

  // 범위로 변환 keeps the look (PD-232): the table style of every cell of a table that turns into a plain range is written
  // into the cell's own format (fill, text colour, bold, italic, the edges of its borders), as the spreadsheet program does.
  // What a cell already has of its own stays: the table style lies under the cell's format. The plan is `plan`, the writing
  // `apply`; both read the look from `sheet-table-style.js` (the same look the grid draws).
  //
  // No reference implementation does this: the reference's delete command removes the table and its theme and leaves the cells as they
  // are (`THIRD_PARTY_NOTICES.md`), so the rule is the spreadsheet program's own.
  //
  // A table of more cells than `MAX_BAKE_CELLS` is not baked (the undo step keeps every cell it touches).

  const MAX_BAKE_CELLS = 100000;
  const AUTO = "000000";
  const BOLD = "600";

  // The edge text of a table edge `{ px, color }`: the 2px and the 3px kinds are the thick line of the cell's border.
  const edgeOf = (edge) => host.__EASYOFFICE_SHEET_LOOK__.edgeText(edge.px >= 2 ? "thick" : "thin", edge.color);

  // The format each cell of `table` takes: `{ ref, row, col, fill?, fontColor?, bold?, italic?, border? }` for the cells that
  // take any. `styleOf(row, col)` is the cell's own format (`{ fill, fontColor, fontWeight, fontStyle, border }` or nothing).
  // Null when the table has no style, or is over the bound (`over` says which).
  function plan(table, box, styleOf) {
    const style = host.__EASYOFFICE_SHEET_TABLE_STYLE__;
    const cs = style.styleOf(table);
    if (!cs) return { cells: [] };
    if ((box.r2 - box.r1 + 1) * (box.c2 - box.c1 + 1) > MAX_BAKE_CELLS) return { over: true, cells: [] };
    const refOf = host.__EASYOFFICE_SHEET_REFS__.refOf;
    const g = style.geometryOf(table);
    const cells = [];
    for (let row = box.r1; row <= box.r2; row += 1) {
      for (let col = box.c1; col <= box.c2; col += 1) {
        const own = styleOf(row, col) || {};
        const look = style.lookOf(table, g, cs, row, col);
        const take = { ref: refOf(row, col), row, col };
        if (look.fill && !own.fill) take.fill = look.fill;
        if (look.color && look.color !== AUTO && !own.fontColor) take.fontColor = look.color;
        if (look.bold && !(own.fontWeight === BOLD || own.fontWeight === "bold")) take.bold = true;
        if (look.italic && own.fontStyle !== "italic") take.italic = true;
        const border = {};
        ["t", "b", "l", "r"].forEach((side, k) => {
          const edge = look.edges[k];
          if (edge && !own.border?.[side]) border[side] = edgeOf(edge);
        });
        if (Object.keys(border).length > 0) take.border = border;
        if (Object.keys(take).length > 3) cells.push(take);
      }
    }
    return { cells };
  }

  // Writes the plan into the model; `patch(ref, mutate)` is the format module's cell edit. Returns the number of cells.
  function apply(planned, patch) {
    for (const take of planned.cells) {
      patch(take.ref, (st) => {
        if (take.fill) st.fill = take.fill;
        if (take.fontColor) st.fontColor = take.fontColor;
        if (take.bold) st.fontWeight = BOLD;
        if (take.italic) st.fontStyle = "italic";
        if (take.border) st.border = { ...(st.border || {}), ...take.border };
      });
    }
    return planned.cells.length;
  }

  host.__EASYOFFICE_SHEET_TABLE_BAKE__ = { MAX_BAKE_CELLS, plan, apply };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
