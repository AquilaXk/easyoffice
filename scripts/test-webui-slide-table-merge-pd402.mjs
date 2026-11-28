import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";

// PD-402: 셀 병합 and 셀 분할 of the slide table.

async function openSlide() {
  const h = await createWorkspaceHarness({ search: "?module=SLIDE&win=w-slide-table-pd402" });
  await flushAsyncTurns(10);
  switchModule(h.document, "SLIDE");
  await flushAsyncTurns(6);
  return h;
}

const act = (document, name) => {
  const item = document.querySelector(`#menu-bar [data-slide-act="${name}"]`);
  assert.ok(item, `missing menu item ${name}`);
  return item;
};
const cellAt = (h, r, c) => Array.from(h.document.getElementById("slide-canvas-layer").querySelectorAll("td, th")).find((td) => td.dataset.rowIdx === String(r) && td.dataset.colIdx === String(c));
const click = (h, r, c, shiftKey = false) => {
  const cell = cellAt(h, r, c);
  assert.ok(cell, `no cell ${r},${c}`);
  cell.dispatchEvent({ type: "click", target: cell, shiftKey });
};
const feedback = (h) => h.document.getElementById("status-command-feedback").textContent;

test("셀 병합 joins the cells from the clicked cell to the Shift+clicked one and draws one cell with colSpan and rowSpan", async () => {
  const h = await openSlide();
  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  canvas.addElement({ kind: "table", x: 40, y: 40, w: 400, h: 200, headerRow: false, rows: [["가", "나", "다"], ["라", "", "바"], ["사", "아", "자"]] });
  canvas.selectElement(canvas.getElements()[0].id);
  assert.deepEqual(["table-merge", "table-split"].map((n) => act(h.document, n).querySelector(".md-label").textContent), ["셀 병합", "셀 분할"]);
  click(h, 0, 0);
  h.userClick(act(h.document, "table-merge"));
  assert.match(feedback(h), /병합할 셀을 선택하세요/, "one cell is no range");
  click(h, 0, 0);
  click(h, 1, 1, true);
  h.userClick(act(h.document, "table-merge"));
  const el = canvas.getElements()[0];
  assert.deepEqual(el.merges, [{ row: 0, col: 0, rowSpan: 2, colSpan: 2 }]);
  assert.equal(el.rows[0][0], "가\n나\n라", "the texts join in reading order");
  assert.deepEqual([el.rows[0][1], el.rows[1][0], el.rows[1][1]], ["", "", ""]);
  const origin = cellAt(h, 0, 0);
  assert.equal(origin.colSpan, 2);
  assert.equal(origin.rowSpan, 2);
  assert.equal(cellAt(h, 1, 1), undefined, "a covered cell is not drawn");
  assert.ok(cellAt(h, 2, 2) && cellAt(h, 0, 2));

  click(h, 0, 0);
  h.userClick(act(h.document, "table-split"));
  assert.deepEqual(canvas.getElements()[0].merges, []);
  assert.ok(cellAt(h, 1, 1), "the cells are separate again");
  click(h, 2, 2);
  h.userClick(act(h.document, "table-split"));
  assert.match(feedback(h), /병합된 셀을 선택하세요/);
});

test("a row or column inserted through a merge grows it and one before it moves it", async () => {
  const h = await openSlide();
  const span = h.host.__EASYOFFICE_SLIDE_TABLE_SPAN__;
  const m = [{ row: 1, col: 1, rowSpan: 2, colSpan: 2 }];
  assert.deepEqual(span.shiftMerges(m, "row", 1, 1), [{ row: 2, col: 1, rowSpan: 2, colSpan: 2 }]);
  assert.deepEqual(span.shiftMerges(m, "row", 2, 1), [{ row: 1, col: 1, rowSpan: 3, colSpan: 2 }]);
  assert.deepEqual(span.shiftMerges(m, "col", 3, 1), m, "after the merge nothing changes");
  assert.deepEqual(span.shiftMerges(m, "col", 0, -1), [{ row: 1, col: 0, rowSpan: 2, colSpan: 2 }]);
  assert.deepEqual(span.shiftMerges(m, "row", 2, -1), [{ row: 1, col: 1, rowSpan: 1, colSpan: 2 }]);
});

test("a hostile merge list is cut down to merges that fit and do not overlap", async () => {
  const h = await openSlide();
  const span = h.host.__EASYOFFICE_SLIDE_TABLE_SPAN__;
  const hostile = [
    { row: 0, col: 0, rowSpan: 2, colSpan: 2 }, { row: 1, col: 1, rowSpan: 2, colSpan: 2 }, { row: 0, col: 2, rowSpan: 1, colSpan: 9 },
    { row: -1, col: 0, rowSpan: 2, colSpan: 2 }, { row: 0, col: 0, rowSpan: 0, colSpan: 5 }, { row: 1.5, col: 0, rowSpan: 2, colSpan: 2 },
    { row: 0, col: 0, rowSpan: 1, colSpan: 1 }, { row: 1e9, col: 1e9, rowSpan: 1e9, colSpan: 1e9 }, null, "x", { row: "0", col: "0", rowSpan: "2", colSpan: "2" },
  ];
  assert.deepEqual(span.normalizeMerges(hostile, 3, 3), [{ row: 0, col: 0, rowSpan: 2, colSpan: 2 }]);
  assert.deepEqual(span.normalizeMerges("nope", 3, 3), []);
  assert.deepEqual(span.normalizeMerges(Array(5000).fill({ row: 0, col: 0, rowSpan: 2, colSpan: 2 }), 3, 3).length, 1);
  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  canvas.addElement({ kind: "table", x: 40, y: 40, w: 300, h: 150, rows: [["a", "b"], ["c", "d"]], merges: hostile });
  assert.deepEqual(canvas.getElements()[0].merges, [{ row: 0, col: 0, rowSpan: 2, colSpan: 2 }]);
});
