import assert from "node:assert/strict";
import test from "node:test";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// PD-294 the pure parts of the cell selection of the BASE datasheet: the scroll offset of an item (ported from virtual-core of TanStack Virtual,
// `base-virtual.js`), the keys and the moves (`base-cell-nav.js`, on the movement rules of `sheet-nav.js`), and the text of the clipboard.

const UI = "../crates/easyoffice-app/ui/";
const host = {};
for (const name of ["sheet-nav.js", "base-model.js", "base-virtual.js", "base-cell-nav.js"]) loadUiModule(new URL(UI + name, import.meta.url), host);
const V = host.__EASYOFFICE_BASE_VIRTUAL__, N = host.__EASYOFFICE_BASE_CELL_NAV__, M = host.__EASYOFFICE_BASE_MODEL__;

function lcg(seed) {
  let s = seed >>> 0;
  return () => { s = (Math.imul(s, 1664525) + 1013904223) >>> 0; return s / 2 ** 32; };
}

// ---- offsetForIndex ----

test("offsetForIndex leaves the offset alone for an item that shows fully between the sticky parts, and brings the others to a side", () => {
  const flat = V.layoutItems(100, () => 24, 26, null); // item i starts at 26 + 24 i
  const view = (offset) => ({ offset, size: 240, padStart: 26, padEnd: 24, max: V.listEnd(flat, 100) + 48 - 240 });
  assert.equal(V.offsetForIndex(flat, 100, 3, "auto", view(0)), 0, "item 3 is inside the view");
  // Item 9 ends at 26 + 240 = 266, past the 240 - 24 = 216 the footer leaves: it goes to the end.
  assert.equal(V.offsetForIndex(flat, 100, 9, "auto", view(0)), 266 + 24 - 240);
  // Item 2 starts at 74 and the view is at 100 with the header over its first 26 px: the item is under the header, so it goes to the start.
  assert.equal(V.offsetForIndex(flat, 100, 2, "auto", view(100)), 74 - 26);
  assert.equal(V.offsetForIndex(flat, 100, 50, "start", view(0)), 26 + 24 * 50 - 26);
  assert.equal(V.offsetForIndex(flat, 100, 50, "end", view(0)), 26 + 24 * 51 + 24 - 240);
});

test("offsetForIndex keeps to the ends of the element: the first item at 0, the last one at the largest offset (the rows that follow the list show)", () => {
  const flat = V.layoutItems(100, () => 24, 26, null), max = V.listEnd(flat, 100) + 48 - 240;
  const view = { offset: 500, size: 240, padStart: 26, padEnd: 24, max };
  assert.equal(V.offsetForIndex(flat, 100, 0, "start", view), 0);
  assert.equal(V.offsetForIndex(flat, 100, 99, "end", view), max);
  assert.equal(V.offsetForIndex(flat, 100, 99, "auto", { ...view, offset: 0 }), max);
  assert.equal(V.offsetForIndex(flat, 100, 5000, "auto", { ...view, offset: 0 }), max, "an index past the end is the last item");
  assert.equal(V.offsetForIndex(flat, 100, -3, "auto", view), 0, "an index before the start is the first item");
  assert.equal(V.offsetForIndex(flat, 0, 0, "auto", view), null, "an empty list has no offset");
  assert.equal(V.offsetForIndex(V.layoutItems(2, () => 24, 26, null), 2, 1, "end", { ...view, max: 0 }), 0, "a list that fits does not scroll");
});

test("offsetForIndex brings every item into the visible band, for mixed sizes and any offset (against the sizes of a plain layout)", () => {
  const rand = lcg(294);
  for (let round = 0; round < 60; round += 1) {
    const count = 30 + Math.floor(rand() * 300), size = 120 + Math.floor(rand() * 500), padStart = 26, padEnd = 24, tail = 48;
    const sizes = Array.from({ length: count }, () => 20 + Math.floor(rand() * 30)), flat = V.layoutItems(count, (i) => sizes[i], padStart, null);
    const max = Math.max(V.listEnd(flat, count) + tail - size, 0);
    for (let k = 0; k < 40; k += 1) {
      const index = Math.floor(rand() * count), offset = Math.floor(rand() * max);
      const align = ["auto", "start", "end"][k % 3], to = V.offsetForIndex(flat, count, index, align, { offset, size, padStart, padEnd, max });
      assert.ok(to >= 0 && to <= max, `offset ${to} is inside 0..${max}`);
      const start = flat[index * 2], end = start + flat[index * 2 + 1];
      // The item is under no sticky part, unless the element is at one of its ends (the header over the first rows, the rows after the last one).
      assert.ok(start >= to + padStart || to === 0, `round ${round}: item ${index} starts at ${start}, the header ends at ${to + padStart}`);
      assert.ok(end <= to + size - padEnd || to === max, `round ${round}: item ${index} ends at ${end}, the footer starts at ${to + size - padEnd}`);
    }
  }
});

// ---- keys ----

const key = (k, extra = {}) => ({ key: k, shiftKey: false, ctrlKey: false, metaKey: false, altKey: false, ...extra });

test("the keys of the cells: arrows, Ctrl+arrows, Home, End, pages, Enter, F2, Space, Delete, Tab, Esc, Ctrl+A", () => {
  assert.deepEqual(N.intentOf(key("ArrowDown")), { t: "move", dr: 1, dc: 0, jump: false, extend: false });
  assert.deepEqual(N.intentOf(key("ArrowLeft", { shiftKey: true })), { t: "move", dr: 0, dc: -1, jump: false, extend: true });
  assert.deepEqual(N.intentOf(key("ArrowUp", { ctrlKey: true })), { t: "move", dr: -1, dc: 0, jump: true, extend: false });
  assert.deepEqual(N.intentOf(key("ArrowRight", { metaKey: true, shiftKey: true })), { t: "move", dr: 0, dc: 1, jump: true, extend: true });
  assert.deepEqual(N.intentOf(key("Home")), { t: "home", whole: false, extend: false });
  assert.deepEqual(N.intentOf(key("Home", { ctrlKey: true })), { t: "home", whole: true, extend: false });
  assert.deepEqual(N.intentOf(key("End", { shiftKey: true })), { t: "end", whole: false, extend: true });
  assert.deepEqual(N.intentOf(key("PageDown")), { t: "page", dr: 1, extend: false });
  assert.deepEqual(N.intentOf(key("PageUp", { shiftKey: true })), { t: "page", dr: -1, extend: true });
  assert.equal(N.intentOf(key("PageDown", { ctrlKey: true })), null, "Ctrl+PageDown is not a key of the cells");
  for (const k of ["Enter", "F2", " "]) assert.deepEqual(N.intentOf(key(k)), { t: "edit" });
  assert.deepEqual(N.intentOf(key("Delete")), { t: "clear" });
  assert.deepEqual(N.intentOf(key("Backspace")), { t: "clear" });
  assert.deepEqual(N.intentOf(key("Tab")), { t: "walk", dr: 0, dc: 1 });
  assert.deepEqual(N.intentOf(key("Tab", { shiftKey: true })), { t: "walk", dr: 0, dc: -1 });
  assert.deepEqual(N.intentOf(key("Escape")), { t: "escape" });
  assert.deepEqual(N.intentOf(key("a", { ctrlKey: true })), { t: "all" });
  assert.deepEqual(N.intentOf(key("A", { metaKey: true })), { t: "all" });
});

test("a typed character starts an edit, a key with Ctrl or Meta does not, and Alt keeps only what types a mark", () => {
  assert.deepEqual(N.intentOf(key("a")), { t: "type", text: "a" });
  assert.deepEqual(N.intentOf(key("가")), { t: "type", text: "가" });
  assert.deepEqual(N.intentOf(key("7")), { t: "type", text: "7" });
  assert.deepEqual(N.intentOf(key("$")), { t: "type", text: "$" });
  assert.deepEqual(N.intentOf(key("™", { altKey: true })), { t: "type", text: "™" }, "Option+2 on a Mac");
  assert.equal(N.intentOf(key("c", { ctrlKey: true })), null, "copy is the clipboard event's");
  assert.equal(N.intentOf(key("v", { metaKey: true })), null);
  assert.equal(N.intentOf(key("ArrowDown", { altKey: true })), null, "Alt+arrow is the browser's");
  assert.equal(N.intentOf(key("Shift")), null);
  assert.equal(N.intentOf(key("F5")), null);
  assert.equal(N.intentOf(key("Tab", { ctrlKey: true })), null, "Ctrl+Tab switches the browser's tabs");
  assert.equal(N.intentOf(key("Enter", { ctrlKey: true })), null);
});

test("while a cell is edited only Enter, Tab and Esc are the grid's", () => {
  assert.deepEqual(N.intentOf(key("Enter"), true), { t: "walk", dr: 1, dc: 0 });
  assert.deepEqual(N.intentOf(key("Enter", { shiftKey: true }), true), { t: "walk", dr: -1, dc: 0 });
  assert.deepEqual(N.intentOf(key("Tab"), true), { t: "walk", dr: 0, dc: 1 });
  assert.deepEqual(N.intentOf(key("Escape"), true), { t: "cancel" });
  for (const k of ["ArrowDown", "ArrowLeft", "Home", "End", "Delete", "Backspace", "a", " ", "PageDown", "F2"]) assert.equal(N.intentOf(key(k), true), null, k);
  assert.equal(N.intentOf(key("a", { ctrlKey: true }), true), null, "select all of the text");
  assert.equal(N.intentOf(key("Enter", { altKey: true }), true), null);
});

// ---- moves ----

// A table of `rows` items and `cols` fields; the items in `groups` are group rows (hidden to the keys); `filled(r, c)` says which cells hold a value.
const viewOf = (rows, cols, groups = [], filled = () => true) => ({
  maxRows: rows, maxCols: cols, dataRows: rows, dataCols: cols, hasValue: filled, mergeAt: () => null, rowHidden: (r) => groups.includes(r), colHidden: () => false,
});
const at = (r, c) => N.single(r, c);
const cell = (sel) => [sel.ar, sel.ac];

test("arrows step one cell and stop at the edges; a group row is passed over", () => {
  const view = viewOf(10, 4, [4]);
  assert.deepEqual(cell(N.selectionAfter(view, at(3, 2), { t: "move", dr: 1, dc: 0 }, 5)), [5, 2], "row 4 is a group row");
  assert.deepEqual(cell(N.selectionAfter(view, at(5, 2), { t: "move", dr: -1, dc: 0 }, 5)), [3, 2]);
  assert.deepEqual(cell(N.selectionAfter(view, at(1, 1), { t: "move", dr: -1, dc: 0 }, 5)), [1, 1]);
  assert.deepEqual(cell(N.selectionAfter(view, at(10, 4), { t: "move", dr: 0, dc: 1 }, 5)), [10, 4]);
});

test("Shift moves the far corner and keeps the active cell; a move without Shift ends the range", () => {
  const view = viewOf(10, 4);
  let sel = N.selectionAfter(view, at(3, 2), { t: "move", dr: 1, dc: 0, extend: true }, 5);
  sel = N.selectionAfter(view, sel, { t: "move", dr: 0, dc: 1, extend: true }, 5);
  assert.deepEqual(cell(sel), [3, 2]);
  assert.deepEqual(N.rangeOf(sel), { r1: 3, r2: 4, c1: 2, c2: 3 });
  sel = N.selectionAfter(view, sel, { t: "move", dr: -1, dc: 0, extend: true }, 5);
  assert.deepEqual(N.rangeOf(sel), { r1: 3, r2: 3, c1: 2, c2: 3 }, "the corner went back to the row of the active cell");
  assert.deepEqual(N.rangeOf(N.selectionAfter(view, sel, { t: "move", dr: 1, dc: 0 }, 5)), { r1: 4, r2: 4, c1: 2, c2: 2 });
});

test("Ctrl+arrow walks a run of values to its end and goes on to the next run, as the spreadsheet does; with no gap it reaches the edge", () => {
  const filled = (r, c) => c !== 1 || r <= 4 || r >= 8;
  const view = viewOf(12, 3, [], filled);
  assert.deepEqual(cell(N.selectionAfter(view, at(1, 1), { t: "move", dr: 1, dc: 0, jump: true }, 5)), [4, 1], "the run 1..4");
  assert.deepEqual(cell(N.selectionAfter(view, at(4, 1), { t: "move", dr: 1, dc: 0, jump: true }, 5)), [8, 1], "over the gap to the next value");
  assert.deepEqual(cell(N.selectionAfter(view, at(8, 1), { t: "move", dr: 1, dc: 0, jump: true }, 5)), [12, 1]);
  assert.deepEqual(cell(N.selectionAfter(view, at(1, 2), { t: "move", dr: 1, dc: 0, jump: true }, 5)), [12, 2], "a full column goes to the last row");
  assert.deepEqual(cell(N.selectionAfter(view, at(5, 3), { t: "move", dr: 0, dc: -1, jump: true }, 5)), [5, 2], "column 1 is empty in row 5: the run ends at column 2");
});

test("Home is the first column of the row, Ctrl+Home the first cell, End the last column, Ctrl+End the last record's last cell; the page keys move a page", () => {
  const view = viewOf(100, 5, [1, 100]);
  assert.deepEqual(cell(N.selectionAfter(view, at(40, 3), { t: "home", whole: false }, 5)), [40, 1]);
  assert.deepEqual(cell(N.selectionAfter(view, at(40, 3), { t: "home", whole: true }, 5)), [2, 1], "row 1 is a group row");
  assert.deepEqual(cell(N.selectionAfter(view, at(40, 3), { t: "end", whole: false }, 5)), [40, 5]);
  const last = N.selectionAfter({ ...view, dataRows: 99 }, at(40, 3), { t: "end", whole: true }, 5);
  assert.deepEqual(cell(last), [99, 5], "the last row is a group row: the last record is the one before it");
  assert.deepEqual(cell(N.selectionAfter(view, at(40, 3), { t: "page", dr: 1 }, 20)), [60, 3]);
  assert.deepEqual(cell(N.selectionAfter(view, at(40, 3), { t: "page", dr: -1 }, 20)), [20, 3]);
  assert.deepEqual(cell(N.selectionAfter(view, at(90, 3), { t: "page", dr: 1 }, 20)), [99, 3], "stops before the group row at the end");
  const grown = N.selectionAfter(view, at(40, 3), { t: "page", dr: 1, extend: true }, 20);
  assert.deepEqual(N.rangeOf(grown), { r1: 40, r2: 60, c1: 3, c2: 3 });
});

test("Enter and Tab walk: a step, the range cycle, and the Enter after a row of Tabs goes back to the column it started in", () => {
  const view = viewOf(10, 5);
  assert.deepEqual(cell(N.walkAfter(view, at(3, 2), 1, 0)), [4, 2]);
  assert.deepEqual(cell(N.walkAfter(view, at(3, 5), 0, 1)), [3, 5], "Tab at the last column does not wrap");
  let sel = at(3, 2);
  for (let i = 0; i < 3; i += 1) sel = N.walkAfter(view, sel, 0, 1);
  assert.deepEqual(cell(sel), [3, 5]);
  assert.deepEqual(cell(N.walkAfter(view, sel, 1, 0)), [4, 2], "the Enter after three Tabs returns to column 2");
  // In a range the active cell cycles through it and the range stays.
  let ranged = { ...at(2, 2), cr: 3, cc: 3 };
  const seen = [];
  for (let i = 0; i < 5; i += 1) { ranged = N.walkAfter(view, ranged, 1, 0); seen.push(cell(ranged).join(",")); }
  assert.deepEqual(seen, ["3,2", "2,3", "3,3", "2,2", "3,2"]);
  assert.deepEqual(N.rangeOf(ranged), { r1: 2, r2: 3, c1: 2, c2: 3 });
});

test("the value that clears a cell is the one the model reads as nothing", () => {
  assert.equal(N.emptyValueOf({ type: "text" }), "");
  assert.equal(N.emptyValueOf({ type: "number" }), "");
  assert.equal(N.emptyValueOf({ type: "checkbox" }), false);
  assert.deepEqual(N.emptyValueOf({ type: "multiSelect" }), []);
  assert.deepEqual(N.emptyValueOf({ type: "relation" }), []);
});

// ---- many cells at once (the model) ----

function twoTables(count) {
  const fields = [{ id: "f1", name: "이름", type: "text", validation: { required: true } }, { id: "f2", name: "수량", type: "number" }, { id: "f3", name: "합계", type: "formula", formula: "{수량} * 2" }, { id: "f4", name: "고객", type: "relation", relationTableId: "tc" }];
  const records = Array.from({ length: count }, (_, i) => ({ id: "r" + i, values: { f1: "행 " + i, f2: i, f4: [] } }));
  const customers = { id: "tc", name: "고객", fields: [{ id: "c1", name: "이름", type: "text" }], records: Array.from({ length: 50 }, (_, i) => ({ id: "c" + i, values: { c1: "고객 " + i } })) };
  return M.hydrateBaseState({ version: 1, activeTableId: "t1", tables: [{ id: "t1", name: "표", fields, records }, customers] }).state;
}

test("updateCells writes the edits the fields accept and counts the others, as updateCell does one by one", () => {
  const state = twoTables(5), table = state.tables[0];
  const res = M.updateCells(state, "t1", [
    { recordId: "r0", fieldId: "f2", value: "42" }, { recordId: "r1", fieldId: "f2", value: "abc" }, { recordId: "r2", fieldId: "f1", value: "" },
    { recordId: "r3", fieldId: "f3", value: "9" }, { recordId: "nope", fieldId: "f2", value: "1" }, { recordId: "r4", fieldId: "none", value: "1" },
    { recordId: "r4", fieldId: "f4", value: ["c3", "c7"] }, { recordId: "r3", fieldId: "f4", value: ["missing"] },
  ]);
  assert.equal(res.ok, true);
  assert.deepEqual([res.applied, res.failed], [2, 6]);
  assert.equal(table.records[0].values.f2, 42, "a text of a number field is read as the model reads it");
  assert.equal(table.records[1].values.f2, 1, "a refused value leaves the cell");
  assert.equal(table.records[2].values.f1, "행 2", "a required field refuses the empty text");
  assert.deepEqual(table.records[4].values.f4, ["c3", "c7"]);
  assert.equal(table.records[3].values.f3, undefined, "a formula is not written");
  const single = twoTables(5);
  M.updateCell(single, "t1", "r0", "f2", "42");
  assert.deepEqual(single.tables[0].records[0].values, table.records[0].values, "the same value as a single update");
});

test("updateCells with nothing that can be written is a failure with the first reason, and an unknown table is refused", () => {
  const state = twoTables(3);
  const res = M.updateCells(state, "t1", [{ recordId: "r0", fieldId: "f1", value: "" }, { recordId: "r1", fieldId: "f2", value: "x" }]);
  assert.equal(res.ok, false);
  assert.match(res.error, /필수/);
  assert.equal(M.updateCells(state, "t1", []).ok, false);
  assert.equal(M.updateCells(state, "nothing", [{ recordId: "r0", fieldId: "f2", value: 1 }]).ok, true, "an unknown table id is the active table, as for every call of the model");
  assert.equal(M.updateCells({ tables: [] }, "x", []).ok, false);
});

test("updateCells checks a relation without building the index again for each cell: 10,000 edits of a table of 10,000 records", () => {
  const state = twoTables(10000), edits = Array.from({ length: 10000 }, (_, i) => ({ recordId: "r" + i, fieldId: "f4", value: ["c" + (i % 50)] }));
  const started = performance.now(), res = M.updateCells(state, "t1", edits), took = performance.now() - started;
  assert.deepEqual([res.applied, res.failed], [10000, 0]);
  assert.ok(took < 1500, `${took.toFixed(0)} ms`);
});

// ---- the text of the clipboard ----

test("the text of the clipboard goes out and comes back the same, whatever the cells hold", () => {
  const rows = [["a", "b\tc", "d\ne"], ['say "hi"', "", "한글   x"], ["", "", ""], ["=1+1", " lead", "trail "]];
  const text = N.toTsv(rows);
  assert.deepEqual(N.parseTsv(text), rows);
  assert.equal(N.toTsv([["a", "b"], ["c", "d"]]), "a\tb\nc\td", "plain cells are not quoted");
  assert.equal(N.toTsv([['x"y']]), '"x""y"');
});

test("parseTsv reads what a spreadsheet puts on the clipboard: CRLF, a last newline, quoted cells, a quote inside a cell", () => {
  assert.deepEqual(N.parseTsv("a\tb\r\nc\td\r\n"), [["a", "b"], ["c", "d"]]);
  assert.deepEqual(N.parseTsv("a\tb\n"), [["a", "b"]], "the last newline does not start a row");
  assert.deepEqual(N.parseTsv("a\t\n\tb"), [["a", ""], ["", "b"]]);
  assert.deepEqual(N.parseTsv('"a\tb"\t"c\r\nd"'), [["a\tb", "c\nd"]]);
  assert.deepEqual(N.parseTsv('5" pipe\tx'), [['5" pipe', "x"]], "a quote inside a cell is a quote, not the start of a quoted cell");
  assert.deepEqual(N.parseTsv('"a ""q"" b"'), [['a "q" b']]);
  assert.deepEqual(N.parseTsv(""), []);
  assert.deepEqual(N.parseTsv(null), []);
  assert.deepEqual(N.parseTsv("x"), [["x"]]);
  assert.deepEqual(N.parseTsv('"open\tcell'), [['"open\tcell']], "an unclosed quote keeps the text as it is");
});

test("a paste over the limits is refused whole: too long, too many rows, too many columns", () => {
  assert.equal(N.parseTsv("x".repeat(N.MAX_PASTE_CHARS + 1)), null);
  assert.equal(N.parseTsv("x\n".repeat(N.MAX_PASTE_ROWS + 1)), null);
  assert.equal(N.parseTsv("x\n".repeat(N.MAX_PASTE_ROWS)).length, N.MAX_PASTE_ROWS, "exactly the limit is taken");
  assert.equal(N.parseTsv("x\t".repeat(N.MAX_PASTE_COLS + 1)), null);
  assert.equal(N.parseTsv(Array(N.MAX_PASTE_COLS).fill("x").join("\t"))[0].length, N.MAX_PASTE_COLS);
  assert.equal(N.parseTsv("\t".repeat(100000)), null, "100,000 empty cells in a row");
  assert.equal(N.parseTsv("\n".repeat(N.MAX_PASTE_ROWS + 5)), null, "empty rows count as rows");
});

test("hostile clipboard text is read in one pass: quotes, no end, a million cells of one character", () => {
  const started = performance.now();
  assert.equal(N.parseTsv('"'.repeat(2_000_000)).length, 1);
  assert.equal(N.parseTsv('""'.repeat(1_000_000)).length, 1);
  assert.deepEqual(N.parseTsv('"' + "a\t".repeat(1_000_000)).map((row) => row.length), [1], "an open quote swallows the tabs: one cell");
  const lines = N.parseTsv("x\ty\n".repeat(5000));
  assert.equal(lines.length, 5000);
  assert.ok(performance.now() - started < 3000, `parsed in ${(performance.now() - started).toFixed(0)} ms`);
});
