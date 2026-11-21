import assert from "node:assert/strict";
import test from "node:test";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

// PD-290 the rows of the BASE datasheet: only the rows around the visible ones are in the DOM, a spacer row on each side stands for
// the rest, and everything that reads the records (print, export, sort, filter, group, edit) still sees all of them.
// The DOM harness has no layout: a row is 24 px by the stylesheet, and the view is as high as the test says `clientHeight` is.
// `childNodes` of the harness is an array, a NodeList of a browser is not: the code under test reads rows by index and by node, never by array method.

const UI = "../crates/easyoffice-app/ui/";
const ROW_PX = 24, HEAD_PX = 26;

// The harness answers every getBoundingClientRect with one box. A row, the header and the sticky parts answer with the heights the
// style sheet gives them, and `rowHeight(recordId)` lets a test make a row taller (the relation chips of a real row do).
let rowHeight = () => ROW_PX;
function giveBoxes(document) {
  const proto = Object.getPrototypeOf(document.createElement("tr"));
  if (proto.__boxesGiven) return;
  const box = proto.getBoundingClientRect;
  proto.getBoundingClientRect = function () {
    if (this.tagName === "THEAD") return { left: 0, top: 0, width: 640, height: HEAD_PX };
    if (this.className.includes("base-grid-row")) return { left: 0, top: 0, width: 640, height: rowHeight(this.dataset.recordId) };
    if (this.className.includes("base-group-row")) return { left: 0, top: 0, width: 640, height: ROW_PX };
    return box.call(this);
  };
  proto.__boxesGiven = true;
}

async function openBase(extra = {}) {
  const { document, host } = await createWorkspaceHarness();
  giveBoxes(document);
  rowHeight = () => ROW_PX;
  for (const name of ["base-model.js", "base-virtual.js", "base-grid-window.js", "base-studio.js"]) loadUiModule(new URL(UI + name, import.meta.url), host);
  const root = document.createElement("section");
  root.id = "pane-base";
  root.className = "editor-pane base-pane";
  document.querySelector(".canvas-stage").appendChild(root);
  root.hidden = false;
  const log = { undo: 0, feedback: [] };
  const studio = host.__EASYOFFICE_CREATE_BASE_STUDIO__({
    document, rootEl: root, invokeIpc: () => null, deliverExport: () => ({ ok: true }),
    setFeedback: (text) => log.feedback.push(text), recordUndo: () => { log.undo += 1; }, noteActivity: () => {}, ...extra,
  });
  studio.bind();
  return { document, host, root, studio, log };
}

function bigState(count, extraFields = []) {
  const fields = [{ id: "f1", name: "이름", type: "text" }, { id: "f2", name: "수량", type: "number" }, { id: "f3", name: "상태", type: "select", options: ["대기", "진행", "완료"] }, ...extraFields];
  const records = [];
  for (let i = 0; i < count; i += 1) records.push({ id: "r" + i, values: { f1: "항목 " + i, f2: i, f3: ["대기", "진행", "완료"][i % 3] } });
  return { version: 1, activeTableId: "t1", tables: [{ id: "t1", name: "표", fields, records }] };
}

const wrapOf = (root) => root.querySelector(".base-grid-wrap");
const dataRows = (root) => root.querySelectorAll("tr.base-grid-row");
const spacers = (root) => root.querySelectorAll("tr.base-grid-spacer");
const spacerPx = (tr) => (tr.hidden ? 0 : Number.parseFloat(tr.firstChild.style.height));
const indexOfRow = (tr) => Number(tr.getAttribute("aria-rowindex")) - 2;
const scrollTo = (root, px, clientHeight = 240) => {
  const wrap = wrapOf(root);
  wrap.clientHeight = clientHeight;
  wrap.scrollTop = px;
  wrap.dispatchEvent({ type: "scroll", target: wrap });
};

// The rows in the DOM plus the two spacers always add up to the height of the whole list.
function assertCovers(root, count) {
  const rows = dataRows(root), [top, bottom] = spacers(root);
  const first = indexOfRow(rows[0]), last = indexOfRow(rows[rows.length - 1]);
  assert.equal(last - first + 1, rows.length, "the rows in the DOM follow each other");
  assert.equal(spacerPx(top), first * ROW_PX, "top spacer");
  assert.equal(spacerPx(bottom), (count - 1 - last) * ROW_PX, "bottom spacer");
  return { first, last, rows };
}

test("a short table is drawn whole, with the row numbers of a table", async () => {
  const { root, studio } = await openBase();
  studio.setState(bigState(100));
  const rows = dataRows(root);
  assert.equal(rows.length, 100);
  assert.ok(spacers(root).every((tr) => tr.hidden), "no room is kept for rows that are not there");
  assert.equal(rows[99].querySelector(".base-row-num").textContent, "100");
  assert.equal(root.querySelector("table").getAttribute("aria-rowcount"), "103", "the header, 100 rows, the new record row and the footer");
  assert.equal(root.querySelector("thead tr").getAttribute("aria-rowindex"), "1");
  assert.equal(rows[0].getAttribute("aria-rowindex"), "2");
  assert.equal(root.querySelector(".base-new-record-row").getAttribute("aria-rowindex"), "102");
  assert.equal(root.querySelector(".base-grid-footer").getAttribute("aria-rowindex"), "103");
});

test("10,000 records put a few dozen rows in the DOM and a spacer for the rest", async () => {
  const { root, studio } = await openBase();
  studio.setState(bigState(10000));
  const { first, last, rows } = assertCovers(root, 10000);
  assert.equal(first, 0);
  assert.ok(rows.length < 80, "rows in the DOM: " + rows.length);
  assert.ok(last * ROW_PX > 800, "the view of 800 px and the rows beyond it");
  assert.equal(root.querySelector("table").getAttribute("aria-rowcount"), "10003");
  assert.equal(root.querySelector(".base-footer-count").textContent, "개수: 10000건");
  assert.equal(rows[0].querySelector(".base-row-num").textContent, "1");
});

test("a scroll moves the window: the rows that are in view stay, the others are built, and the spacers follow", async () => {
  const { root, studio } = await openBase();
  studio.setState(bigState(10000));
  const kept = dataRows(root)[10];
  scrollTo(root, 5000 * ROW_PX + HEAD_PX);
  let { first, last, rows } = assertCovers(root, 10000);
  assert.ok(first <= 5000 && last >= 5000 + 9, `window around 5000: ${first}..${last}`);
  assert.ok(rows.length < 60);
  assert.equal(rows.find((tr) => indexOfRow(tr) === 5000).dataset.recordId, "r5000");
  assert.equal(rows.find((tr) => indexOfRow(tr) === 5000).querySelector(".base-row-num").textContent, "5001");
  assert.ok(!rows.includes(kept), "a row far above the window is gone");

  // A step of a few rows inside the rows that are kept builds nothing new: the same nodes stay.
  const before = dataRows(root).slice();
  scrollTo(root, 5000 * ROW_PX + HEAD_PX + 3 * ROW_PX);
  assert.deepEqual(dataRows(root), before);

  scrollTo(root, 9999 * ROW_PX);
  ({ last, rows } = assertCovers(root, 10000));
  assert.equal(last, 9999, "the window reaches the last row");
  scrollTo(root, 0);
  assert.equal(assertCovers(root, 10000).first, 0);
});

test("a window that scrolls by less than its extra rows and one that jumps both cover the view", async () => {
  const { root, studio } = await openBase();
  studio.setState(bigState(10000));
  let at = 0;
  for (const step of [1, 7, 31, 200, 3, 4000, 17, 9000, 5]) {
    at = (at + step * ROW_PX) % (9990 * ROW_PX);
    scrollTo(root, at + HEAD_PX, 240);
    const { first, last } = assertCovers(root, 10000);
    const viewFirst = Math.floor(at / ROW_PX), viewLast = Math.floor((at + 240) / ROW_PX);
    assert.ok(first <= viewFirst && last >= viewLast, `view ${viewFirst}..${viewLast} is inside ${first}..${last}`);
  }
});

test("an edit in a table far down draws the same window again, with the new value, at the same scroll offset", async () => {
  const { root, studio } = await openBase();
  studio.setState(bigState(10000));
  scrollTo(root, 6000 * ROW_PX + HEAD_PX);
  const res = studio.updateCell("r6000", "f1", "바뀐 값");
  assert.equal(res.ok, true);
  const wrap = wrapOf(root);
  assert.equal(wrap.scrollTop, 6000 * ROW_PX + HEAD_PX, "the scroll offset of the view that was replaced");
  const { rows } = assertCovers(root, 10000);
  assert.equal(rows.find((tr) => indexOfRow(tr) === 6000).querySelector('td[data-field-id="f1"] input').value, "바뀐 값");
  assert.ok(indexOfRow(rows[0]) > 5000, "not the top of the table");
});

test("a new document or another table starts at the top", async () => {
  const { root, studio } = await openBase();
  studio.setState(bigState(10000));
  scrollTo(root, 6000 * ROW_PX);
  studio.setState(bigState(10000));
  assert.equal(wrapOf(root).scrollTop, 0);
  assert.equal(indexOfRow(dataRows(root)[0]), 0);
});

test("sort, filter and group keep working on all the records, and the window shows the result", async () => {
  const { root, studio } = await openBase();
  studio.setState(bigState(10000));
  studio.setSorts([{ fieldId: "f2", dir: "desc" }]);
  assert.equal(dataRows(root)[0].dataset.recordId, "r9999");
  assert.equal(dataRows(root)[0].querySelector(".base-row-num").textContent, "1");
  studio.setFilters([{ fieldId: "f3", op: "eq", value: "완료" }], "and");
  assert.equal(root.querySelector(".base-footer-count").textContent, "개수: 3333 / 10000건");
  assertCovers(root, 3333);
  studio.setFilters([{ fieldId: "f2", op: "lt", value: "50" }], "and");
  assert.equal(dataRows(root).length, 50, "a result of 50 rows is drawn whole");
  assert.ok(spacers(root).every((tr) => tr.hidden));
  studio.setFilters([], "and");
  studio.setSorts([]);
  studio.setGroupBy("f3");
  assert.equal(root.querySelectorAll("tr.base-group-row").length, 1, "the group row of the first group; the other two are far below");
  assert.equal(root.querySelector("table").getAttribute("aria-rowcount"), String(10000 + 3 + 3));
});

test("a group row is an item of the list: its number, and the rows after it", async () => {
  const { root, studio } = await openBase();
  studio.setState(bigState(1000));
  studio.setGroupBy("f3");
  // 3 group rows and 1000 records: item 0 is the first group row, the next rows are its records.
  const first = root.querySelector("tr.base-group-row");
  assert.equal(first.getAttribute("aria-rowindex"), "2");
  assert.equal(dataRows(root)[0].getAttribute("aria-rowindex"), "3");
  assert.equal(dataRows(root)[0].querySelector(".base-row-num").textContent, "1");
  assert.equal(root.querySelector("table").getAttribute("aria-rowcount"), String(1000 + 3 + 3));
  // The first group holds 334 records (item 0 is its row), so item 335 is the row of the second group and item 340 its fifth record.
  scrollTo(root, 340 * ROW_PX + HEAD_PX);
  const groupRowsInView = root.querySelectorAll("tr.base-group-row");
  assert.equal(groupRowsInView.length, 1);
  assert.equal(indexOfRow(groupRowsInView[0]), 335);
  const number = dataRows(root).find((tr) => indexOfRow(tr) === 340).querySelector(".base-row-num").textContent;
  assert.equal(number, "339", "item 340 follows two group rows: it is record 339");
  // Collapsing a group takes its 333 records out of the list.
  groupRowsInView[0].querySelector(".base-group-toggle").click();
  assert.equal(root.querySelector("table").getAttribute("aria-rowcount"), String(1000 + 3 + 3 - 333));
});

test("print and export read every record, whatever the window holds", async () => {
  const { host, root, studio } = await openBase({ invokeIpc: undefined });
  studio.setState(bigState(10000));
  assert.ok(dataRows(root).length < 80);
  const pages = studio.buildPrintPages();
  const printed = pages.reduce((n, page) => n + page.querySelectorAll("tbody tr").length, 0);
  assert.equal(printed, 10000);
  const csv = host.__EASYOFFICE_BASE_MODEL__.toDelimitedText(studio.getState(), "t1", ",");
  assert.equal(csv.split("\n").filter(Boolean).length, 10001, "the header and 10,000 records");
  const exported = await studio.exportAs("csv");
  assert.equal(exported.ok, true);
  assert.equal(Buffer.from(exported.base64, "base64").toString("utf8").split("\n").filter(Boolean).length, 10001);
});

test("a control that is being edited in a row that leaves the window is left first, so its text is committed", async () => {
  const { document, root, studio } = await openBase();
  studio.setState(bigState(10000));
  const input = dataRows(root)[2].querySelector('td[data-field-id="f1"] input');
  input.focus();
  let blurred = 0;
  input.addEventListener("blur", () => { blurred += 1; });
  scrollTo(root, 6000 * ROW_PX);
  assert.equal(blurred, 1);
  assert.notEqual(document.activeElement, input);
  assertCovers(root, 10000);

  // A control inside the rows that stay is not touched.
  scrollTo(root, 0);
  const stay = dataRows(root)[5].querySelector('td[data-field-id="f1"] input');
  stay.focus();
  let stayBlurred = 0;
  stay.addEventListener("blur", () => { stayBlurred += 1; });
  scrollTo(root, 2 * ROW_PX);
  assert.equal(stayBlurred, 0);
});

test("an edit that fails to commit from a row of the window keeps its mark", async () => {
  const { root, studio } = await openBase();
  studio.setState(bigState(10000, [{ id: "f4", name: "주소", type: "url" }]));
  scrollTo(root, 3000 * ROW_PX);
  const row = dataRows(root).find((tr) => indexOfRow(tr) === 3000);
  const input = row.querySelector('td[data-field-id="f4"] input');
  input.value = "not a url";
  input.dispatchEvent({ type: "change", target: input });
  const again = dataRows(root).find((tr) => indexOfRow(tr) === 3000);
  assert.equal(again.querySelector('td[data-field-id="f4"]').dataset.invalid, "true");
});

test("a record added to a long table is counted, and the rows of the window follow the table", async () => {
  const { root, studio } = await openBase();
  studio.setState(bigState(9990));
  scrollTo(root, 4000 * ROW_PX);
  studio.addRecord({ f1: "새 항목" });
  assert.equal(root.querySelector(".base-footer-count").textContent, "개수: 9991건");
  assert.equal(root.querySelector("table").getAttribute("aria-rowcount"), "9994");
  assertCovers(root, 9991);
});

// The geometry of the DOM as a browser lays it out: the header, the top spacer, then the rows in the DOM, each of its own height.
function rowTops(root) {
  let y = HEAD_PX + spacerPx(spacers(root)[0]);
  return dataRows(root).map((tr) => {
    const top = y;
    y += rowHeight(tr.dataset.recordId);
    return { index: indexOfRow(tr), top, bottom: y };
  });
}
const rowAt = (root, offset) => rowTops(root).find((r) => offset >= r.top && offset < r.bottom)?.index;
const recordNumber = (id) => Number(id.slice(1));

test("rows of two heights leave no gap in the view, wherever the scroll jumps", async () => {
  const { root, studio } = await openBase();
  rowHeight = (id) => (recordNumber(id) % 3 === 0 ? 46 : ROW_PX);
  studio.setState(bigState(10000));
  for (const offset of [0, 777, 50000, 150000, 25, 220000, 90000, 10]) {
    scrollTo(root, offset, 240);
    const tops = rowTops(root), view = wrapOf(root).scrollTop;
    assert.ok(tops[0].top <= view + HEAD_PX, `rows start at ${tops[0].top} for a view at ${view}`);
    assert.ok(tops.at(-1).bottom >= view + 240, `rows end at ${tops.at(-1).bottom} for a view that ends at ${view + 240}`);
  }
});

test("rows built above the view that are taller than estimated do not move what is on screen", async () => {
  const { root, studio } = await openBase();
  // The first rows are 24 px and set the estimate; every row from the 100th on is 46 px.
  rowHeight = (id) => (recordNumber(id) >= 100 ? 46 : ROW_PX);
  studio.setState(bigState(10000));
  scrollTo(root, 120000, 240);
  let wrap = wrapOf(root);
  for (let step = 0; step < 60; step += 1) {
    const before = rowAt(root, wrap.scrollTop + HEAD_PX);
    scrollTo(root, wrap.scrollTop - 100, 240);
    wrap = wrapOf(root);
    const after = rowAt(root, wrap.scrollTop + HEAD_PX);
    assert.ok(before !== undefined && after !== undefined, `step ${step}: a row is under the top of the view`);
    assert.ok(after <= before && before - after <= 4, `step ${step}: the row under the top went from ${before} to ${after}`);
    const tops = rowTops(root);
    assert.ok(tops[0].top <= wrap.scrollTop + HEAD_PX && tops.at(-1).bottom >= wrap.scrollTop + 240, `step ${step}: a gap`);
  }
});
