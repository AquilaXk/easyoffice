import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

// How a table looks (PD-231): the 60 built-in styles, a file's own definition, stripes, the first and last column, the header
// and totals rows, the edges, the lookup of the table that holds a cell, and the paint of a grid cell.

const table = (extra = {}) => ({
  name: "T",
  range: "B2:E8",
  header: true,
  totals: false,
  columns: ["a", "b", "c", "d"],
  style: "TableStyleMedium2",
  firstColumn: false,
  lastColumn: false,
  rowStripes: true,
  columnStripes: false,
  ...extra,
});

async function setup(tables = [table()]) {
  const { host, document } = await createWorkspaceHarness();
  host.__EASYOFFICE_SHEET_LAYOUT__.replaceAll({ S: { tables } });
  return { style: host.__EASYOFFICE_SHEET_TABLE_STYLE__, layout: host.__EASYOFFICE_SHEET_LAYOUT__, document, host };
}

// A color within two steps of each channel (the HLS shift is in floating point; the spreadsheet program's integer one differs by a step or two).
const near = (actual, expected) => {
  assert.ok(actual && expected, `${actual} ${expected}`);
  for (let i = 0; i < 3; i += 1) {
    const a = Number.parseInt(actual.slice(2 * i, 2 * i + 2), 16);
    const e = Number.parseInt(expected.slice(2 * i, 2 * i + 2), 16);
    assert.ok(Math.abs(a - e) <= 2, `${actual} is not near ${expected}`);
  }
};

const look = (api, layout, row, col, sheet = "S") => {
  const t = layout.tableBox && api.tableAt(sheet, row, col);
  return api.lookOf(t, api.geometryOf(t), api.styleOf(t), row, col);
};

test("the page defines the 60 built-in styles and every one compiles", async () => {
  const { style } = await setup();
  assert.equal(style.BUILT_IN.light.length, 21);
  assert.equal(style.BUILT_IN.medium.length, 28);
  assert.equal(style.BUILT_IN.dark.length, 11);
  for (const name of [...style.BUILT_IN.light, ...style.BUILT_IN.medium, ...style.BUILT_IN.dark]) {
    const cells = style.previewCells(name, { header: true, rowStripes: true });
    assert.equal(cells.length, style.PREVIEW_ROWS, name);
    for (const row of cells) for (const cell of row) assert.match(cell.fill ?? "000000", /^[0-9a-f]{6}$/, name);
  }
  assert.equal(style.previewCells("NoSuchStyle", {}), null);
});

test("the Office theme tints are the spreadsheet program's colors", async () => {
  const { style } = await setup();
  assert.equal(style.colorOf("t4"), "4472c4");
  near(style.colorOf("t4:0.7999816888943144"), "d9e1f2");
  near(style.colorOf("t4:0.3999755851924192"), "8eaadb");
  near(style.colorOf("t0:-0.1499984740745262"), "d9d9d9");
  near(style.colorOf("t1:0.34998626667073579"), "595959");
  near(style.colorOf("t5:-0.249977111117893"), "c55a11");
  assert.equal(style.colorOf("abcdef"), "abcdef");
  assert.equal(style.colorOf(null), null);
});

test("Medium2: a blue header with white bold text, bands in the light blue, thin lines around and between rows", async () => {
  const { style, layout } = await setup();
  const head = look(style, layout, 2, 3);
  assert.equal(head.fill, "4472c4");
  assert.equal(head.color, "ffffff");
  assert.equal(head.bold, true);
  // The data rows below the header: the first stripe (rows 3, 5, 7) is banded, the second (4, 6, 8) is not.
  near(look(style, layout, 3, 3).fill, "d9e1f2");
  assert.equal(look(style, layout, 4, 3).fill, null);
  assert.equal(look(style, layout, 5, 3).fill, look(style, layout, 3, 3).fill);
  assert.equal(look(style, layout, 4, 3).bold, false);
  // The table's edges: the first row draws its top, the first column its left.
  assert.equal(look(style, layout, 2, 2).edges[0].px, 1);
  near(look(style, layout, 2, 2).edges[0].color, "8eaadb");
  assert.equal(look(style, layout, 4, 2).edges[2].px, 1);
  assert.equal(look(style, layout, 4, 3).edges[2], null, "an inner left edge belongs to the cell on its left");
  // A line between two rows is the horizontal one; the last column's right edge is the table's.
  assert.equal(look(style, layout, 4, 3).edges[1].px, 1);
  assert.equal(look(style, layout, 4, 5).edges[3].px, 1);
  assert.equal(look(style, layout, 4, 3).edges[3], null, "Medium2 has no vertical line");
});

test("the first and last column, the totals row and the column stripes follow their options", async () => {
  const { style, layout } = await setup([table({ firstColumn: true, lastColumn: true, totals: true, columnStripes: true, rowStripes: false })]);
  assert.equal(look(style, layout, 4, 2).bold, true);
  assert.equal(look(style, layout, 4, 5).bold, true);
  assert.equal(look(style, layout, 4, 3).bold, false);
  // The totals row: bold, with the double line above it from the style.
  const total = look(style, layout, 8, 3);
  assert.equal(total.bold, true);
  assert.equal(look(style, layout, 7, 3).edges[1].px, 2, "the double line above the totals row");
  // The first column stripe is banded (the first, third column), the second is not; the header keeps its own fill.
  near(look(style, layout, 4, 2).fill, "d9e1f2");
  assert.equal(look(style, layout, 4, 3).fill, null);
  near(look(style, layout, 4, 4).fill, "d9e1f2");
  assert.equal(look(style, layout, 2, 4).fill, "4472c4");
});

test("a table without a header row stripes from its first row", async () => {
  const { style, layout } = await setup([table({ header: false })]);
  near(look(style, layout, 2, 3).fill, "d9e1f2");
  assert.equal(look(style, layout, 3, 3).fill, null);
  assert.equal(look(style, layout, 2, 3).bold, false);
});

test("a file's own definition is drawn: stripe size, colors and the weight of an edge", async () => {
  const styleDef = {
    w: { c: "202020" },
    h: { f: "1f4e78", c: "ffffff", b: 1, bd: { b: "medium:ff0000" } },
    r1: { f: "ddebf7", sz: 2 },
    r2: { f: "fff2cc" },
  };
  const { style, layout } = await setup([table({ style: "Mine", styleDef })]);
  const rows = [3, 4, 5, 6, 7, 8].map((r) => look(style, layout, r, 3).fill);
  // Two banded rows, one of the second kind, two banded again, one of the second kind.
  assert.deepEqual(rows, ["ddebf7", "ddebf7", "fff2cc", "ddebf7", "ddebf7", "fff2cc"]);
  assert.equal(look(style, layout, 3, 3).color, "202020");
  assert.deepEqual(look(style, layout, 2, 3).edges[1], { px: 2, color: "ff0000" });
  // A name nobody defined has no look.
  layout.replaceAll({ S: { tables: [table({ style: "Nobody" })] } });
  assert.equal(style.styleOf(layout.tablesOf("S")[0]), null);
});

test("the table that holds a cell is found among a thousand", async () => {
  const many = Array.from({ length: 1000 }, (_, i) => table({ name: "T" + i, range: `A${3 * i + 1}:B${3 * i + 3}`, columns: ["a", "b"] }));
  const { style, layout } = await setup(many);
  assert.equal(style.tableAt("S", 1, 1).name, "T0");
  assert.equal(style.tableAt("S", 2999, 2).name, "T999");
  assert.equal(style.tableAt("S", 3000, 2).name, "T999");
  assert.equal(style.tableAt("S", 3001, 1), null);
  assert.equal(style.tableAt("S", 5, 3), null);
  assert.equal(style.tableAt("Other", 1, 1), null);
  layout.setTables("S", []);
  assert.equal(style.tableAt("S", 1, 1), null, "a changed layout is looked at again");
});

test("a grid cell takes the table's fill and text colors, and a cell's own fill and font color win", async () => {
  const { style, document } = await setup();
  const td = document.createElement("td");
  td._row = 2;
  td._col = 3;
  td._st = undefined;
  style.paintTd(td, "S");
  assert.equal(td.dataset.tsF, "1");
  assert.equal(td.style.getPropertyValue("--ts-fill"), "#4472c4");
  assert.equal(td.style.getPropertyValue("--ts-fg"), "#ffffff");
  assert.equal(td.dataset.tsB, "1");
  // A banded body cell: the black text of the whole table is the automatic one on no fill, and the dark text on the light band.
  td._row = 3;
  style.paintTd(td, "S");
  near(td.style.getPropertyValue("--ts-fill").slice(1), "d9e1f2");
  assert.equal(td.style.getPropertyValue("--ts-fg"), "#000000");
  td._row = 4;
  style.paintTd(td, "S");
  assert.equal(td.dataset.tsF, undefined);
  assert.equal(td.dataset.tsC, undefined, "black text on no fill is the theme's text");
  // Text with its own color keeps it; a cell with its own fill keeps the automatic text rule of that fill.
  td._row = 2;
  td._st = { fontColor: "ff0000" };
  style.paintTd(td, "S");
  assert.equal(td.dataset.tsC, undefined);
  td._st = { fill: "ffff00" };
  style.paintTd(td, "S");
  assert.equal(td.dataset.tsC, "1", "the header's white text stays when only the fill is the cell's");
  // A cell outside every table is cleared.
  td._row = 20;
  style.paintTd(td, "S");
  assert.equal(td.dataset.tsF, undefined);
  assert.equal(td.style.getPropertyValue("--ts-fill"), "");
});

test("a table cell draws its edges and its grid line as shadows, so a line runs unbroken to the cell's edge", async () => {
  const { style, document } = await setup([table({ totals: true })]);
  const at = (row, col) => {
    const td = document.createElement("td");
    td._row = row;
    td._col = col;
    style.paintTd(td, "S");
    return td;
  };
  const shadow = (td) => td.style.getPropertyValue("--ts-shadow");
  // Row 4 (no band) has the thin line of Medium2 under it and no vertical line: the line, then the grid line on the right.
  const plain = at(4, 3);
  assert.equal(plain.dataset.ts, "1", "the cell has no border width of its own (CSS)");
  assert.match(shadow(plain), /^inset 0 -1px 0 0 #[0-9a-f]{6}, inset -1px 0 0 0 var\(--ui-border-light\)$/);
  // A banded row hides the grid line: only the table's line.
  const band = at(3, 3);
  assert.match(shadow(band), /^inset 0 -1px 0 0 #[0-9a-f]{6}$/);
  // The row above the totals row has the double line of the style under it: two pixels.
  assert.match(shadow(at(7, 3)), /^inset 0 -2px 0 0 #[0-9a-f]{6}/);
  // The table's first row draws its top line; the first column its left line.
  assert.match(shadow(at(2, 2)), /^inset 0 1px 0 0 #[0-9a-f]{6}.*inset 1px 0 0 0 #[0-9a-f]{6}/);
  // A side the cell has a border of its own on keeps it and gets no grid line from the style.
  const own = document.createElement("td");
  own._row = 4;
  own._col = 3;
  own.style.borderRightStyle = "solid";
  style.paintTd(own, "S");
  assert.doesNotMatch(shadow(own), /ui-border-light/);
  // A cell that leaves the table loses all of it.
  plain._row = 20;
  style.paintTd(plain, "S");
  assert.equal(plain.dataset.ts, undefined);
  assert.equal(shadow(plain), "");
});

test("the table style lies under the cell's own colors and the conditional format in the sheet's CSS", async () => {
  const fs = await import("node:fs");
  const css = fs.readFileSync(new URL("../crates/easyoffice-app/ui/styles.css", import.meta.url), "utf8");
  const at = (text) => {
    const i = css.indexOf(text);
    assert.ok(i >= 0, text);
    return i;
  };
  assert.ok(at(".sheet-grid td.in-range { background") < at("td[data-ts-f] { background-color"), "the table fill replaces the plain selection tint");
  assert.ok(at("td[data-ts-f] { background-color") < at("td[data-fill] { background-color"), "the cell's own fill is above the table's");
  assert.ok(at("td[data-ts-c]:where") < at("td[data-fg] { color"), "the cell's own text color is above the table's");
  assert.ok(at("td[data-ts-c]:where") > at("td[data-fill]:where(:not([data-fg])"), "the table's text color is above the automatic text of a cell's own fill");
  assert.ok(at("td[data-ts-c]:where") < at("td.nf-colored"), "the number format color is above the table's");
});
