import assert from "node:assert/strict";
import test from "node:test";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

// PD-294 cell selection of the BASE datasheet in the DOM harness: a click selects, Shift and a drag grow a range, the keys move it (and the
// window follows), Enter, F2, a double click and a typed key edit, Esc cancels, Delete clears, Ctrl+C and Ctrl+V go through TSV, and the table
// is one tab stop with `aria-selected` and `aria-activedescendant`.
// The harness has no layout and its `childNodes` is an array, not a NodeList: the code under test reads cells by index. A row is 24 px, the
// header 26, the sticky footer 24, and the view is as high as the test says `clientHeight` is. A browser fires `change` when a control with
// a changed value loses the focus, the harness does not: `changeOnBlur` makes it do so.

const UI = "../crates/easyoffice-app/ui/";
const ROW_PX = 24, HEAD_PX = 26, FOOT_PX = 24, VIEW_PX = 240;

let rowHeight = () => ROW_PX;
function patchHarness(document) {
  const proto = Object.getPrototypeOf(document.createElement("tr"));
  if (proto.__cellSelectPatched) return;
  const box = proto.getBoundingClientRect, focus = proto.focus, blur = proto.blur, click = proto.click;
  proto.getBoundingClientRect = function () {
    if (this.tagName === "THEAD") return { left: 0, top: 0, width: 640, height: HEAD_PX };
    if (this.className.includes("base-grid-row")) return { left: 0, top: 0, width: 640, height: rowHeight(this.dataset.recordId) };
    if (this.className.includes("base-group-row")) return { left: 0, top: 0, width: 640, height: ROW_PX };
    return box.call(this);
  };
  proto.focus = function (...args) {
    if (this.ownerDocument.activeElement !== this) this._valueAtFocus = this.value;
    return focus.apply(this, args);
  };
  proto.blur = function () {
    if (this.ownerDocument.activeElement === this && this.tagName === "INPUT" && this._valueAtFocus !== undefined && this.value !== this._valueAtFocus) {
      this._valueAtFocus = this.value;
      this.dispatchEvent({ type: "change", target: this });
    }
    return blur.call(this);
  };
  // A click on a checkbox toggles it and fires `change`.
  proto.click = function () {
    if (this.className.includes("base-cell-checkbox")) {
      this.checked = !this.checked;
      this.dispatchEvent({ type: "change", target: this });
    }
    return click.call(this);
  };
  proto.__cellSelectPatched = true;
}

async function openBase(extra = {}) {
  const { document, host } = await createWorkspaceHarness();
  patchHarness(document);
  rowHeight = () => ROW_PX;
  for (const name of ["sheet-nav.js", "base-model.js", "base-virtual.js", "base-grid-window.js", "base-cell-nav.js", "base-clipboard.js", "base-cell-select.js", "base-studio.js"]) loadUiModule(new URL(UI + name, import.meta.url), host);
  const root = document.createElement("section");
  root.id = "pane-base";
  root.className = "editor-pane base-pane";
  document.querySelector(".canvas-stage").appendChild(root);
  root.hidden = false;
  const log = { undo: 0, feedback: [] };
  const studio = host.__EASYOFFICE_CREATE_BASE_STUDIO__({
    document, rootEl: root, invokeIpc: () => null, deliverExport: () => ({ ok: true }),
    // The shell takes the snapshot before a change and keeps it only when the change was made (`captureUndo`, `commitUndo`): a refused change leaves no step.
    setFeedback: (text) => log.feedback.push(text), recordUndo: () => { log.undo += 1; }, captureUndo: () => ({}), commitUndo: () => { log.undo += 1; }, noteActivity: () => {}, ...extra,
  });
  studio.bind();
  return { document, host, root, studio, log };
}

const SELECT_OPTIONS = ["대기", "진행", "완료"];
function bigState(count) {
  const fields = [{ id: "f1", name: "이름", type: "text" }, { id: "f2", name: "수량", type: "number" }, { id: "f3", name: "상태", type: "select", options: SELECT_OPTIONS }];
  const records = [];
  for (let i = 0; i < count; i += 1) records.push({ id: "r" + i, values: { f1: "항목 " + i, f2: i, f3: SELECT_OPTIONS[i % 3] } });
  return { version: 1, activeTableId: "t1", tables: [{ id: "t1", name: "표", fields, records }] };
}

// Every type of field, five records, and a table of two customers for the relation.
function mixedState() {
  const fields = [
    { id: "f1", name: "이름", type: "text" }, { id: "f2", name: "수량", type: "number", validation: { min: 0 } },
    { id: "f3", name: "상태", type: "select", options: SELECT_OPTIONS }, { id: "f4", name: "날짜", type: "date" },
    { id: "f5", name: "완료", type: "checkbox" }, { id: "f6", name: "고객", type: "relation", relationTableId: "tc" },
    { id: "f7", name: "합계", type: "formula", formula: "{수량} * 2" }, { id: "f8", name: "태그", type: "multiSelect", options: ["a", "b", "c"] },
    { id: "f9", name: "주소", type: "url" },
  ];
  const records = Array.from({ length: 5 }, (_, i) => ({ id: "r" + i, values: { f1: "행 " + i, f2: i + 1, f3: "대기", f4: "2026-11-0" + (i + 1), f5: false, f6: [], f8: [], f9: "" } }));
  const customers = { id: "tc", name: "고객", fields: [{ id: "c1", name: "이름", type: "text" }], records: [{ id: "c0", values: { c1: "김" } }, { id: "c1", values: { c1: "이" } }] };
  return { version: 1, activeTableId: "t1", tables: [{ id: "t1", name: "표", fields, records }, customers] };
}

const tableOf = (root) => root.querySelector("table.base-grid-table");
const wrapOf = (root) => root.querySelector(".base-grid-wrap");
const dataRows = (root) => root.querySelectorAll("tr.base-grid-row");
const indexOfRow = (tr) => Number(tr.getAttribute("aria-rowindex")) - 2;
const cellOf = (root, rec, field) => root.querySelector(`tr[data-record-id="${rec}"] td[data-field-id="${field}"]`);
const selected = (root) => root.querySelectorAll("td[data-sel]");
const activeCell = (root) => root.querySelector("td[data-sel~='a']");
const activeOf = (root) => { const td = activeCell(root); return td ? [td.parentElement.dataset.recordId, td.dataset.fieldId] : null; };
const stateValues = (studio, rec) => studio.getState().tables[0].records.find((r) => r.id === rec).values;
function view(root, clientHeight = VIEW_PX) {
  const wrap = wrapOf(root);
  wrap.clientHeight = clientHeight;
  wrap.scrollTop ||= 0;
  wrap.dispatchEvent({ type: "scroll", target: wrap });
  return wrap;
}

// What a user does. `press` sends the key to the element that has the focus, as a browser does, and says whether the page took the key over.
const press = (document, key, mods = {}) => {
  const target = document.activeElement;
  return !target.dispatchEvent({ type: "keydown", key, target, ...mods });
};
const mousedown = (td, mods = {}) => !td.dispatchEvent({ type: "mousedown", target: td, button: 0, ...mods });
function clipboard(initial = "") {
  const data = new Map(initial === "" ? [] : [["text/plain", initial]]);
  return { data, setData: (type, text) => data.set(type, text), getData: (type) => data.get(type) ?? "" };
}
const copy = (root) => { const clipboardData = clipboard(), t = tableOf(root); t.dispatchEvent({ type: "copy", target: t, clipboardData }); return clipboardData.data.get("text/plain"); };
const paste = (root, text) => { const t = tableOf(root); return !t.dispatchEvent({ type: "paste", target: t, clipboardData: clipboard(text) }); };

async function openTable(count = 200) {
  const ctx = await openBase();
  ctx.studio.setState(bigState(count));
  view(ctx.root);
  return ctx;
}

// ---- selecting ----

test("a click selects a cell: a border on it, aria-selected, the table has the focus and names the cell, no control takes the click", async () => {
  const { document, root, studio } = await openTable();
  const td = cellOf(root, "r3", "f2");
  assert.equal(mousedown(td), true, "the click is taken over: the input does not get the focus");
  assert.equal(td.getAttribute("data-sel"), "s t b l r a");
  assert.equal(td.getAttribute("aria-selected"), "true");
  assert.equal(cellOf(root, "r3", "f1").getAttribute("aria-selected"), "false");
  assert.equal(cellOf(root, "r4", "f2").getAttribute("aria-selected"), "false");
  assert.equal(selected(root).length, 1);
  assert.equal(document.activeElement, tableOf(root));
  assert.equal(tableOf(root).getAttribute("aria-activedescendant"), td.getAttribute("id"));
  assert.equal(td.querySelector("input").readOnly, true, "the cell is not in edit");
  assert.deepEqual(studio.selection(), { ar: 4, ac: 2, cr: 4, cc: 2, span: null, tabFrom: null });
  mousedown(cellOf(root, "r5", "f3"));
  assert.equal(td.getAttribute("aria-selected"), "false", "the old cell is let go");
  assert.equal(td.hasAttribute("data-sel"), false);
  assert.equal(td.getAttribute("id"), null);
});

test("the table is the one tab stop: it is a grid, the controls of the cells are out of the tab order, a text input is read-only until edited", async () => {
  const { root } = await openTable();
  const tbl = tableOf(root);
  assert.equal(tbl.getAttribute("role"), "grid");
  assert.equal(tbl.getAttribute("aria-multiselectable"), "true");
  assert.equal(tbl.getAttribute("tabindex"), "0");
  const row = dataRows(root)[0];
  const controls = row.querySelectorAll("input, select, button");
  assert.ok(controls.length >= 4, "the cells and the delete button of the row");
  assert.ok(controls.every((el) => el.getAttribute("tabindex") === "-1"));
  assert.ok(row.querySelectorAll("input.base-cell-input").every((el) => el.readOnly === true));
  assert.ok(dataRows(root).every((tr) => tr.querySelectorAll("td.base-grid-cell").every((td) => td.getAttribute("aria-selected") === "false")), "every cell says whether it is selected");
});

test("the delete button of the active row and the link of the active cell are the controls left in the tab order", async () => {
  const { document, root, studio } = await openBase();
  const state = mixedState();
  state.tables[0].records[1].values.f9 = "https://example.com/1";
  state.tables[0].records[2].values.f9 = "https://example.com/2";
  studio.setState(state);
  view(root);
  const button = (rec) => root.querySelector(`tr[data-record-id="${rec}"] button.base-row-delete-btn`);
  const link = (rec) => root.querySelector(`tr[data-record-id="${rec}"] a.base-url-link`);
  const tabbable = () => root.querySelectorAll("button, a, input, select").filter((el) => el.getAttribute("tabindex") === "0");
  assert.equal(tabbable().filter((el) => el.closest("tbody")).length, 0, "nothing in the rows before a cell is selected");
  mousedown(cellOf(root, "r1", "f9"));
  assert.deepEqual(tabbable().filter((el) => el.closest("tbody")), [button("r1"), link("r1")]);
  press(document, "ArrowDown");
  assert.deepEqual(tabbable().filter((el) => el.closest("tbody")), [button("r2"), link("r2")], "the tab stops follow the active cell");
  press(document, "ArrowLeft");
  assert.deepEqual(tabbable().filter((el) => el.closest("tbody")), [button("r2")], "no link in the active cell");
  button("r2").click();
  assert.equal(studio.getState().tables[0].records.length, 4, "the button deletes its record");
});

test("Shift+click selects the range from the active cell; the edges and the active cell are marked", async () => {
  const { root } = await openTable();
  mousedown(cellOf(root, "r2", "f1"));
  mousedown(cellOf(root, "r4", "f3"), { shiftKey: true });
  assert.equal(selected(root).length, 9);
  assert.equal(cellOf(root, "r2", "f1").getAttribute("data-sel"), "s t l a");
  assert.equal(cellOf(root, "r3", "f2").getAttribute("data-sel"), "s");
  assert.equal(cellOf(root, "r4", "f3").getAttribute("data-sel"), "s b r");
  assert.equal(cellOf(root, "r2", "f3").getAttribute("data-sel"), "s t r");
  assert.deepEqual(activeOf(root), ["r2", "f1"]);
  mousedown(cellOf(root, "r0", "f3"), { shiftKey: true });
  assert.equal(selected(root).length, 9, "the range is from the active cell to the new corner: rows 0..2, three fields");
  assert.equal(cellOf(root, "r4", "f3").hasAttribute("data-sel"), false);
  assert.equal(cellOf(root, "r2", "f1").getAttribute("data-sel"), "s b l a");
});

test("a drag selects a range, follows the pointer, and ends at mouseup", async () => {
  const { document, root } = await openTable();
  const start = cellOf(root, "r1", "f1");
  mousedown(start);
  document.dispatchEvent({ type: "mousemove", target: cellOf(root, "r3", "f2"), clientY: 100 });
  assert.equal(selected(root).length, 6);
  document.dispatchEvent({ type: "mousemove", target: cellOf(root, "r2", "f1"), clientY: 90 });
  assert.equal(selected(root).length, 2);
  document.dispatchEvent({ type: "mouseup", target: start });
  document.dispatchEvent({ type: "mousemove", target: cellOf(root, "r9", "f3"), clientY: 200 });
  assert.equal(selected(root).length, 2, "after mouseup the pointer moves nothing");
});

test("a click on a link, a button or a checkbox goes to it; the cell is selected all the same", async () => {
  const { root, studio } = await openBase();
  studio.setState(mixedState());
  view(root);
  const box = cellOf(root, "r1", "f5");
  assert.equal(mousedown(box.querySelector("input")), false, "the checkbox keeps its click");
  assert.deepEqual(activeOf(root), ["r1", "f5"]);
  const pill = cellOf(root, "r2", "f8").querySelector("button");
  assert.equal(mousedown(pill), false);
  assert.deepEqual(activeOf(root), ["r2", "f8"]);
  const sel = cellOf(root, "r3", "f3").querySelector("select");
  assert.equal(mousedown(sel), true, "the first click on a select selects the cell");
  assert.equal(mousedown(sel), false, "the click on a selected cell opens the select");
});

// ---- moving ----

test("the arrow keys move one cell and stop at the edges, and the page does not scroll", async () => {
  const { document, root } = await openTable();
  mousedown(cellOf(root, "r0", "f1"));
  assert.equal(press(document, "ArrowDown"), true);
  assert.deepEqual(activeOf(root), ["r1", "f1"]);
  press(document, "ArrowRight"); press(document, "ArrowRight");
  assert.deepEqual(activeOf(root), ["r1", "f3"]);
  assert.equal(press(document, "ArrowRight"), true, "an arrow at the edge is taken as well");
  assert.deepEqual(activeOf(root), ["r1", "f3"]);
  press(document, "ArrowUp"); press(document, "ArrowUp");
  assert.deepEqual(activeOf(root), ["r0", "f3"]);
  press(document, "ArrowLeft");
  assert.deepEqual(activeOf(root), ["r0", "f2"]);
  assert.equal(selected(root).length, 1);
});

test("Tab and Shift+Tab move across; at the two ends of the table they are left to the browser, so the focus goes on", async () => {
  const { document, root } = await openTable();
  mousedown(cellOf(root, "r0", "f1"));
  assert.equal(press(document, "Tab"), true);
  assert.deepEqual(activeOf(root), ["r0", "f2"]);
  assert.equal(press(document, "Tab", { shiftKey: true }), true);
  assert.deepEqual(activeOf(root), ["r0", "f1"]);
  assert.equal(press(document, "Tab", { shiftKey: true }), false, "Shift+Tab at the first cell leaves the grid");
  assert.deepEqual(activeOf(root), ["r0", "f1"]);
  mousedown(cellOf(root, "r2", "f3"));
  assert.equal(press(document, "Tab"), false, "Tab at the end of a row does not wrap and leaves the grid");
  assert.deepEqual(activeOf(root), ["r2", "f3"]);
});

test("Shift+arrows grow the range from the active cell; Esc ends the range, and an arrow without Shift does too", async () => {
  const { document, root, studio } = await openTable();
  mousedown(cellOf(root, "r2", "f1"));
  press(document, "ArrowDown", { shiftKey: true }); press(document, "ArrowDown", { shiftKey: true }); press(document, "ArrowRight", { shiftKey: true });
  assert.equal(selected(root).length, 6);
  assert.deepEqual(activeOf(root), ["r2", "f1"], "the active cell stays");
  assert.deepEqual(studio.selection(), { ar: 3, ac: 1, cr: 5, cc: 2, span: null, tabFrom: null });
  press(document, "ArrowUp", { shiftKey: true });
  assert.equal(selected(root).length, 4);
  press(document, "Escape");
  assert.equal(selected(root).length, 1);
  assert.deepEqual(activeOf(root), ["r2", "f1"]);
  press(document, "ArrowDown", { shiftKey: true }); press(document, "ArrowDown");
  assert.deepEqual(activeOf(root), ["r3", "f1"]);
  assert.equal(selected(root).length, 1);
});

test("Ctrl+A selects every cell of the records; Home and End go to the ends of the row, Ctrl+Home and Ctrl+End to the ends of the table", async () => {
  const { document, root, studio } = await openTable(50);
  mousedown(cellOf(root, "r4", "f2"));
  press(document, "Home");
  assert.deepEqual(activeOf(root), ["r4", "f1"]);
  press(document, "End");
  assert.deepEqual(activeOf(root), ["r4", "f3"]);
  press(document, "End", { ctrlKey: true });
  assert.deepEqual(activeOf(root), ["r49", "f3"]);
  press(document, "Home", { ctrlKey: true });
  assert.deepEqual(activeOf(root), ["r0", "f1"]);
  press(document, "a", { ctrlKey: true });
  assert.deepEqual(studio.selection(), { ar: 1, ac: 1, cr: 50, cc: 3, span: null, tabFrom: null });
  assert.equal(selected(root).length, dataRows(root).length * 3, "every built row is selected");
});

test("Ctrl+arrow walks a run of values in the sheet's way: to the end of the run, over a gap to the next run, to the edge when there is none", async () => {
  const { document, root, studio } = await openTable(30);
  for (let i = 10; i < 14; i += 1) studio.updateCell("r" + i, "f1", "");
  view(root);
  mousedown(cellOf(root, "r0", "f1"));
  press(document, "ArrowDown", { ctrlKey: true });
  assert.deepEqual(activeOf(root), ["r9", "f1"], "the run before the empty cells");
  press(document, "ArrowDown", { ctrlKey: true });
  assert.deepEqual(activeOf(root), ["r14", "f1"], "over the empty cells to the next value");
  press(document, "ArrowDown", { ctrlKey: true });
  assert.deepEqual(activeOf(root), ["r29", "f1"]);
  press(document, "ArrowRight", { ctrlKey: true });
  assert.deepEqual(activeOf(root), ["r29", "f3"]);
  press(document, "ArrowUp", { ctrlKey: true });
  assert.deepEqual(activeOf(root), ["r0", "f3"]);
});

test("group rows are passed over by the keys, and the selection does not stay on a group row", async () => {
  const { document, root, studio } = await openTable(20);
  studio.setGroupBy("f3");
  view(root);
  const rows = dataRows(root);
  const firstGroup = rows.filter((tr) => tr.dataset.recordId && true);
  assert.ok(firstGroup.length > 0);
  mousedown(cellOf(root, "r0", "f1"));
  // r0, r3, r6, ... are the first group "대기": 7 records, then the group row of "진행".
  for (let i = 0; i < 6; i += 1) press(document, "ArrowDown");
  assert.deepEqual(activeOf(root), ["r18", "f1"]);
  press(document, "ArrowDown");
  assert.deepEqual(activeOf(root), ["r1", "f1"], "the group row between is passed");
  press(document, "ArrowUp");
  assert.deepEqual(activeOf(root), ["r18", "f1"]);
  press(document, "End", { ctrlKey: true });
  assert.deepEqual(activeOf(root), ["r17", "f3"], "the last record of the last group (the third group holds r2, r5, ... r17)");
  studio.setGroupBy("");
  assert.equal(activeCell(root), null, "another group setting forgets the selection");
});

test("the keys scroll the list: an arrow at the bottom of the view brings the next row in, PageDown moves a page, Ctrl+arrow goes to the end of 10,000 rows", async () => {
  const { document, root, studio } = await openBase();
  studio.setState(bigState(10000));
  const wrap = view(root);
  const inBand = (i) => {
    const top = HEAD_PX + ROW_PX * i;
    return top >= wrap.scrollTop + HEAD_PX && top + ROW_PX <= wrap.scrollTop + VIEW_PX - FOOT_PX;
  };
  mousedown(cellOf(root, "r0", "f1"));
  for (let i = 0; i < 40; i += 1) {
    press(document, "ArrowDown");
    assert.ok(inBand(i + 1), `row ${i + 1} is in view at ${wrap.scrollTop}`);
  }
  assert.deepEqual(activeOf(root), ["r40", "f1"]);
  assert.ok(wrap.scrollTop > 0);
  const before = wrap.scrollTop;
  press(document, "PageDown");
  assert.deepEqual(activeOf(root), ["r47", "f1"], "a page is the seven rows that fit between the header and the footer");
  assert.ok(wrap.scrollTop > before && inBand(47));
  press(document, "PageUp"); press(document, "PageUp");
  assert.deepEqual(activeOf(root), ["r33", "f1"]);
  assert.ok(inBand(33));
  press(document, "ArrowDown", { ctrlKey: true });
  assert.deepEqual(activeOf(root), ["r9999", "f1"]);
  assert.equal(wrap.scrollTop, HEAD_PX + ROW_PX * 10000 - VIEW_PX, "the end of the list");
  assert.equal(activeCell(root).parentElement.dataset.recordId, "r9999", "the row is built and painted");
  assert.ok(dataRows(root).length < 80, "the window did not grow: " + dataRows(root).length);
  press(document, "Home", { ctrlKey: true });
  assert.deepEqual(activeOf(root), ["r0", "f1"]);
  assert.equal(wrap.scrollTop, 0);
});

test("the selection is painted on the rows the window builds later, and the table names the active cell only while its row is in the DOM", async () => {
  const { document, root, studio } = await openBase();
  studio.setState(bigState(10000));
  const wrap = view(root);
  mousedown(cellOf(root, "r2", "f2"));
  press(document, "ArrowDown", { shiftKey: true });
  const tbl = tableOf(root);
  assert.ok(tbl.getAttribute("aria-activedescendant"));
  wrap.scrollTop = 6000 * ROW_PX;
  wrap.dispatchEvent({ type: "scroll", target: wrap });
  assert.equal(root.querySelectorAll("td[data-sel]").length, 0, "the selected rows are far above");
  wrap.scrollTop = 0;
  wrap.dispatchEvent({ type: "scroll", target: wrap });
  assert.equal(selected(root).length, 2, "the rows come back painted");
  assert.deepEqual(activeOf(root), ["r2", "f2"]);
  assert.equal(document.activeElement, tbl, "scrolling does not take the focus away");
});

// ---- editing ----

test("Enter, F2 and a double click edit the cell; the text stays and the caret is at the end", async () => {
  const { document, root } = await openTable();
  for (const open of [() => press(document, "Enter"), () => press(document, "F2"), () => cellOf(root, "r2", "f1").dispatchEvent({ type: "dblclick", target: cellOf(root, "r2", "f1") })]) {
    mousedown(cellOf(root, "r2", "f1"));
    open();
    const input = cellOf(root, "r2", "f1").querySelector("input");
    assert.equal(document.activeElement, input);
    assert.equal(input.readOnly, false);
    assert.equal(input.value, "항목 2");
    press(document, "Escape");
    assert.equal(document.activeElement, tableOf(root));
    assert.equal(cellOf(root, "r2", "f1").querySelector("input").readOnly, true);
  }
});

test("a typed character replaces the text and edits; a key with Ctrl does not", async () => {
  const { document, root, studio, log } = await openTable();
  mousedown(cellOf(root, "r2", "f1"));
  assert.equal(press(document, "x", { ctrlKey: true }), false);
  assert.equal(document.activeElement, tableOf(root));
  assert.equal(press(document, "가"), true, "the key is taken: the character is put in by the grid");
  const input = cellOf(root, "r2", "f1").querySelector("input");
  assert.equal(document.activeElement, input);
  assert.equal(input.value, "가");
  input.value = "가나";
  assert.equal(log.undo, 0, "nothing is committed yet");
  press(document, "Enter");
  assert.equal(stateValues(studio, "r2").f1, "가나");
  assert.equal(log.undo, 1);
  assert.deepEqual(activeOf(root), ["r3", "f1"], "Enter commits and goes down");
  assert.equal(document.activeElement, tableOf(root), "the table that was drawn again has the focus");
});

test("Esc cancels the edit: the old text is back, nothing is written, the focus returns to the table", async () => {
  const { document, root, studio, log } = await openTable();
  mousedown(cellOf(root, "r2", "f1"));
  press(document, "F2");
  const input = document.activeElement;
  input.value = "ändert";
  press(document, "Escape");
  assert.equal(stateValues(studio, "r2").f1, "항목 2");
  assert.equal(cellOf(root, "r2", "f1").querySelector("input").value, "항목 2");
  assert.equal(log.undo, 0);
  assert.equal(document.activeElement, tableOf(root));
  assert.deepEqual(activeOf(root), ["r2", "f1"]);
});

test("Tab commits and goes right, Shift+Enter commits and goes up, a click on another cell commits and selects it", async () => {
  const { document, root, studio } = await openTable();
  mousedown(cellOf(root, "r4", "f1"));
  press(document, "a");
  document.activeElement.value = "Tab 값";
  press(document, "Tab");
  assert.equal(stateValues(studio, "r4").f1, "Tab 값");
  assert.deepEqual(activeOf(root), ["r4", "f2"]);
  press(document, "Enter");
  document.activeElement.value = "77";
  press(document, "Enter", { shiftKey: true });
  assert.equal(stateValues(studio, "r4").f2, 77);
  assert.deepEqual(activeOf(root), ["r3", "f2"]);
  press(document, "F2");
  document.activeElement.value = "55";
  mousedown(cellOf(root, "r8", "f1"));
  assert.equal(stateValues(studio, "r3").f2, 55);
  assert.deepEqual(activeOf(root), ["r8", "f1"]);
  assert.equal(document.activeElement, tableOf(root));
});

test("an edit keeps the selection across the redraw, and a refused value marks the cell", async () => {
  const { document, root } = await openTable();
  mousedown(cellOf(root, "r2", "f2"));
  mousedown(cellOf(root, "r3", "f3"), { shiftKey: true });
  mousedown(cellOf(root, "r2", "f2"));
  press(document, "9");
  document.activeElement.value = "not a number";
  press(document, "Enter");
  assert.equal(cellOf(root, "r2", "f2").dataset.invalid, "true");
  assert.deepEqual(activeOf(root), ["r3", "f2"]);
  assert.equal(document.activeElement, tableOf(root));
});

test("a checkbox toggles with Space, Enter and F2; a select takes the focus; a formula and a date follow their own rules", async () => {
  const { document, root, studio } = await openBase();
  studio.setState(mixedState());
  view(root);
  mousedown(cellOf(root, "r0", "f5"));
  press(document, " ");
  assert.equal(stateValues(studio, "r0").f5, true);
  press(document, "Enter");
  assert.equal(stateValues(studio, "r0").f5, false);
  press(document, "F2");
  assert.equal(stateValues(studio, "r0").f5, true);
  mousedown(cellOf(root, "r0", "f3"));
  press(document, "Enter");
  const select = cellOf(root, "r0", "f3").querySelector("select");
  assert.equal(document.activeElement, select);
  assert.equal(press(document, "Tab"), true, "Tab leaves the select and goes right");
  assert.deepEqual(activeOf(root), ["r0", "f4"]);
  assert.equal(document.activeElement, tableOf(root));
  mousedown(cellOf(root, "r0", "f7"));
  assert.equal(press(document, "Enter"), true);
  assert.equal(document.activeElement, tableOf(root), "a formula cell is not edited");
  press(document, "q");
  assert.equal(document.activeElement, tableOf(root));
  mousedown(cellOf(root, "r0", "f4"));
  press(document, "2");
  const date = cellOf(root, "r0", "f4").querySelector("input");
  assert.equal(document.activeElement, date);
  assert.equal(date.value, "", "a date takes the key itself: the control is emptied and left to type");
});

test("a key of an input method starts the edit without taking the key", async () => {
  const { document, root } = await openTable();
  mousedown(cellOf(root, "r2", "f1"));
  const prevented = press(document, "Process", { keyCode: 229 });
  assert.equal(prevented, false);
  const input = cellOf(root, "r2", "f1").querySelector("input");
  assert.equal(document.activeElement, input);
  assert.equal(input.value, "", "the text is cleared: the composition replaces it");
  assert.equal(press(document, "ArrowDown", { isComposing: true }), false, "the keys of the composition are the control's");
  assert.deepEqual(activeOf(root), ["r2", "f1"]);
});

test("a control that takes the focus without the grid's keys (a screen reader) selects its cell and is edited", async () => {
  const { document, root } = await openTable();
  const input = cellOf(root, "r6", "f1").querySelector("input");
  input.focus();
  input.dispatchEvent({ type: "focusin", target: input });
  assert.deepEqual(activeOf(root), ["r6", "f1"]);
  assert.equal(input.readOnly, false);
  assert.equal(press(document, "ArrowLeft"), false, "an arrow is the text's while the cell is edited");
  press(document, "Escape");
  assert.equal(document.activeElement, tableOf(root));
});

// ---- Delete, copy, paste ----

test("Delete empties the selected cells in one undo step; a formula and a required field are left, and the refusals are counted", async () => {
  const { document, root, studio, log } = await openBase();
  const state = mixedState();
  state.tables[0].fields[0].validation = { required: true };
  studio.setState(state);
  view(root);
  mousedown(cellOf(root, "r1", "f1"));
  mousedown(cellOf(root, "r3", "f8"), { shiftKey: true });
  press(document, "Delete");
  const values = (r) => stateValues(studio, r);
  for (const r of ["r1", "r2", "r3"]) {
    assert.equal(values(r).f2, null, "a number is emptied");
    assert.equal(values(r).f3, "");
    assert.equal(values(r).f4, "");
    assert.equal(values(r).f5, false);
    assert.deepEqual(values(r).f8, []);
    assert.equal(values(r).f1, "행 " + r.slice(1), "the required field keeps its text");
  }
  assert.equal(values("r0").f2, 1, "a row outside the range is not touched");
  assert.equal(log.undo, 1, "nine fields... three rows: one undo step");
  assert.match(log.feedback.at(-1), /3칸.*필수/);
  assert.equal(selected(root).length, 3 * 8, "the selection is kept");
  log.undo = 0;
  press(document, "Backspace");
  assert.equal(log.undo, 1, "Backspace is Delete again: the cells are empty already, the model still counts one step");
});

test("Delete with nothing to clear writes nothing and adds no undo step", async () => {
  const { document, root, studio, log } = await openBase();
  studio.setState(mixedState());
  view(root);
  mousedown(cellOf(root, "r0", "f7"));
  press(document, "Delete");
  assert.equal(log.undo, 0, "a formula cannot be cleared");
});

test("Ctrl+C puts the selected cells on the clipboard as TSV, in the shown text of each field", async () => {
  const { root, studio } = await openBase();
  const state = mixedState();
  state.tables[0].records[1].values.f1 = "탭\t줄\n따옴표\"";
  state.tables[0].records[1].values.f6 = ["c1"];
  studio.setState(state);
  view(root);
  mousedown(cellOf(root, "r0", "f1"));
  mousedown(cellOf(root, "r1", "f9"), { shiftKey: true });
  const lines = copy(root).split("\n");
  assert.equal(lines[0], "행 0\t1\t대기\t2026-11-01\tFALSE\t\t2\t\t");
  assert.ok(copy(root).startsWith('"행 0\t'.slice(1)), "plain text is not quoted");
  assert.equal(copy(root), "행 0\t1\t대기\t2026-11-01\tFALSE\t\t2\t\t\n" + '"탭\t줄\n따옴표""\"\t2\t대기\t2026-11-02\tFALSE\t이\t4\t\t');
});

test("Ctrl+C of a range that spans group rows leaves the group rows out", async () => {
  const { root, studio } = await openTable(12);
  studio.setGroupBy("f3");
  view(root);
  mousedown(cellOf(root, "r0", "f1"));
  mousedown(cellOf(root, "r1", "f1"), { shiftKey: true });
  assert.equal(copy(root), "항목 0\n항목 3\n항목 6\n항목 9\n항목 1", "the first group's four records, then the first of the next");
});

test("Ctrl+V writes a block from the active cell: each text goes through the check of its field, and it is one undo step", async () => {
  const { root, studio, log } = await openBase();
  studio.setState(mixedState());
  view(root);
  mousedown(cellOf(root, "r1", "f1"));
  const text = ["새 이름\t7\t진행\t2026-12-01\tTRUE\t김, 이\tskipped\ta, b\thttps://example.com/x", "둘째\t8\t완료\t2026-12-02\t0\t이\t\tc\t"].join("\r\n") + "\r\n";
  assert.equal(paste(root, text), true, "the paste is taken over");
  assert.deepEqual(stateValues(studio, "r1"), { f1: "새 이름", f2: 7, f3: "진행", f4: "2026-12-01", f5: true, f6: ["c0", "c1"], f8: ["a", "b"], f9: "https://example.com/x" });
  assert.deepEqual(stateValues(studio, "r2"), { f1: "둘째", f2: 8, f3: "완료", f4: "2026-12-02", f5: false, f6: ["c1"], f8: ["c"], f9: "" });
  assert.equal(log.undo, 1);
  assert.deepEqual(studio.selection(), { ar: 2, ac: 1, cr: 3, cc: 9, span: null, tabFrom: null }, "the pasted block is selected");
  assert.equal(stateValues(studio, "r0").f1, "행 0", "the rows above are not touched");
});

test("a paste with values a field refuses writes the others and counts the refused", async () => {
  const { root, studio, log } = await openBase();
  studio.setState(mixedState());
  view(root);
  mousedown(cellOf(root, "r0", "f1"));
  paste(root, "ok\t-5\t없는 상태\t2026-13-45\tmaybe\t없는 고객\t\tz\tnot a url");
  assert.deepEqual(stateValues(studio, "r0"), { f1: "ok", f2: 1, f3: "대기", f4: "2026-11-01", f5: false, f6: [], f8: [], f9: "" });
  assert.equal(log.undo, 1);
  assert.match(log.feedback.at(-1), /^7칸은 값이 맞지 않아/);
  log.undo = 0;
  mousedown(cellOf(root, "r0", "f2"));
  paste(root, "-1");
  assert.equal(log.undo, 0, "a paste that writes nothing adds no step");
  assert.equal(stateValues(studio, "r0").f2, 1);
});

test("one text pasted over a range fills the range; a block appends records past the end, cuts columns, and passes group rows", async () => {
  const { root, studio } = await openBase();
  studio.setState(bigState(8));
  view(root);
  mousedown(cellOf(root, "r1", "f1"));
  mousedown(cellOf(root, "r3", "f2"), { shiftKey: true });
  paste(root, "같은 값");
  assert.deepEqual([1, 2, 3].map((i) => stateValues(studio, "r" + i).f1), ["같은 값", "같은 값", "같은 값"]);
  assert.equal(stateValues(studio, "r2").f2, 2, "a text that a number field refuses leaves it");
  mousedown(cellOf(root, "r6", "f2"));
  // Two fields are left from "수량" on: the third text of a line is cut; the third line appends a new record.
  paste(root, "1\t완료\t9\n5\t대기\t9\n7\t진행\t9");
  assert.deepEqual([stateValues(studio, "r6").f2, stateValues(studio, "r6").f3], [1, "완료"]);
  assert.deepEqual([stateValues(studio, "r7").f2, stateValues(studio, "r7").f3], [5, "대기"]);
  const recs = studio.getState().tables[0].records;
  assert.equal(recs.length, 9, "the third pasted line creates a new record");
  assert.deepEqual(recs[8].values, { f1: "", f2: 7, f3: "진행" });
  assert.equal(stateValues(studio, "r6").f1, "항목 6", "the field before the active cell is not touched");
});

test("a paste lands on records only: the group rows between are passed", async () => {
  const { root, studio } = await openTable(9);
  studio.setGroupBy("f3");
  view(root);
  mousedown(cellOf(root, "r6", "f1"));
  paste(root, "A\nB\nC");
  // The group of "대기" holds r0, r3, r6; the next item is the group row of "진행", then r1.
  assert.equal(stateValues(studio, "r6").f1, "A");
  assert.equal(stateValues(studio, "r1").f1, "B");
  assert.equal(stateValues(studio, "r4").f1, "C");
});

test("a paste over the limits is refused whole, with a message, and nothing changes", async () => {
  const { root, studio, log } = await openTable(20);
  mousedown(cellOf(root, "r0", "f1"));
  assert.equal(paste(root, "x\n".repeat(10001)), true);
  assert.equal(log.undo, 0);
  assert.match(log.feedback.at(-1), /붙여 넣을 수 없습니다/);
  assert.equal(stateValues(studio, "r0").f1, "항목 0");
  paste(root, "y\t".repeat(70));
  assert.equal(log.undo, 0);
  paste(root, "a".repeat(9 * 1024 * 1024));
  assert.equal(log.undo, 0);
  assert.equal(stateValues(studio, "r0").f1, "항목 0");
});

test("copy and paste leave the text of a cell that is being edited to the input", async () => {
  const { document, root, studio } = await openTable();
  mousedown(cellOf(root, "r2", "f1"));
  press(document, "F2");
  const input = document.activeElement;
  const data = clipboard();
  assert.equal(!input.dispatchEvent({ type: "copy", target: input, clipboardData: data }), false);
  assert.equal(data.data.size, 0);
  assert.equal(!input.dispatchEvent({ type: "paste", target: input, clipboardData: clipboard("입력 칸에") }), false);
  assert.equal(stateValues(studio, "r3").f1, "항목 3");
});

// ---- when the selection is let go ----

test("another table, a filter, a sort or a group forgets the selection; an edit and a new record keep it", async () => {
  const { root, studio } = await openTable(30);
  mousedown(cellOf(root, "r2", "f1"));
  studio.updateCell("r5", "f1", "다른 칸");
  assert.deepEqual(activeOf(root), ["r2", "f1"]);
  studio.addRecord({ f1: "새 행" });
  assert.deepEqual(activeOf(root), ["r2", "f1"]);
  studio.setFilters([{ fieldId: "f2", op: "lt", value: "10" }], "and");
  assert.equal(activeCell(root), null);
  mousedown(cellOf(root, "r2", "f1"));
  studio.setSorts([{ fieldId: "f2", dir: "desc" }]);
  assert.equal(activeCell(root), null);
  mousedown(cellOf(root, "r2", "f1"));
  studio.setFilters([], "and");
  assert.equal(activeCell(root), null);
});

test("a selection past the end of a shorter table is dropped, and a view without a grid has no selection", async () => {
  const { document, root, studio } = await openTable(30);
  mousedown(cellOf(root, "r25", "f1"));
  studio.setState(bigState(10));
  assert.equal(activeCell(root), null);
  assert.equal(studio.selection(), null);
  mousedown(cellOf(root, "r2", "f1"));
  studio.setView("kanban");
  studio.setView("grid");
  view(root);
  assert.deepEqual(activeOf(root), ["r2", "f1"], "the same table comes back with its selection");
  assert.equal(press(document, "ArrowDown"), true);
});

test("the table takes the first cell when it gets the focus with nothing selected, and does not take a key when it is empty", async () => {
  const { document, root, studio } = await openTable(10);
  tableOf(root).focus();
  tableOf(root).dispatchEvent({ type: "focusin", target: tableOf(root) });
  assert.deepEqual(activeOf(root), ["r0", "f1"]);
  studio.setState(bigState(0));
  tableOf(root).focus();
  tableOf(root).dispatchEvent({ type: "focusin", target: tableOf(root) });
  assert.equal(activeCell(root), null);
  assert.equal(press(document, "ArrowDown"), false);
});
