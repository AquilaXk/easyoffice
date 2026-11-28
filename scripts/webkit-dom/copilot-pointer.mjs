import assert from "node:assert/strict";
import os from "node:os";
import path from "node:path";
import { acceptChange, askCopilot, finishPlay } from "./copilot.mjs";

// The copilot's pointer on a sheet's format, chart and row change, on the datasheet and on the board (PD-387), in a real WebKit with
// real layout. What is measured is where the pointer's selection boxes really are: over the cells, the header, the button and the
// canvas the change is about. A frame in the middle of each performance is kept as a screenshot in the capture directory.

const TICK = { timeout: 20000 };

const shot = (page, name) => page.screenshot({ path: path.join(process.argv[2] || os.tmpdir(), `pointer-${name}.png`) });

const rectOf = (page, selector) => page.$eval(selector, (el) => {
  const r = el.getBoundingClientRect();
  return { left: r.left, top: r.top, right: r.right, bottom: r.bottom, width: r.width, height: r.height };
});

const near = (a, b, slack, label) => assert.ok(Math.abs(a - b) <= slack, `${label}: ${a} is not within ${slack} of ${b}`);

/** Waits until a selection box matches `kind` against `arg` (the page's policy forbids sending a function, so the kinds live in the page) and returns that box. */
async function waitForSelection(page, kind, arg) {
  const handle = await page.waitForFunction(([name, value]) => {
    const close = (a, b) => Math.abs(a - b) < 3;
    const kinds = {
      // across the cells `value.left` to `value.right` (the drag has reached the far cell)
      across: (b) => close(b.left, value.left) && close(b.right, value.right),
      // a box to the right of `value` (a data range's right edge), big enough to be a chart frame
      right: (b) => b.left > value && b.width > 150,
      // over the rect `value` at its top left corner
      at: (b) => close(b.left, value.left) && close(b.top, value.top),
      // over a header: its corner and not wider than it
      header: (b) => close(b.left, value.left) && close(b.top, value.top) && b.width < value.width + 3,
      // over a row of the same top and wider than a cell
      row: (b) => close(b.top, value.top) && b.width > 100,
    };
    for (const node of document.querySelectorAll("#eo-copilot-cursor-layer .eo-copilot-selection")) {
      const r = node.getBoundingClientRect();
      const box = { left: r.left, top: r.top, right: r.right, bottom: r.bottom, width: r.width, height: r.height };
      if (kinds[name](box)) return box;
    }
    return null;
  }, [kind, arg], TICK);
  return handle.jsonValue();
}

const cellRect = (page, coord) => rectOf(page, `#pane-sheet td[data-cell="${coord}"]`);
const DATA = { op: "sheet.write", cells: [{ coord: "A1", raw: "항목" }, { coord: "B1", raw: "값" }, { coord: "A2", raw: "가" }, { coord: "B2", raw: "10" }, { coord: "A3", raw: "나" }, { coord: "B3", raw: "20" }] };

export const copilotSheetPointerSteps = [
  ["the pointer drags over the cells a format is about and the bold and fill buttons are pressed", async (page) => {
    await askCopilot(page, "머리글에 서식을 주세요", [[DATA, { op: "sheet.format", range: "A1:B1", bold: true, fill: "#ffff00" }]]);
    await acceptChange(page);
    const a1 = await cellRect(page, "A1");
    const b1 = await cellRect(page, "B1");
    // The typing of the cells selects one cell at a time; the format step drags over the two cells together.
    const box = await waitForSelection(page, "across", { left: a1.left, right: b1.right });
    near(box.left, a1.left, 3, "selection left");
    near(box.right, b1.right, 3, "selection right");
    near(box.top, a1.top, 3, "selection top");
    await shot(page, "sheet-format");
    await finishPlay(page);
    assert.match(await page.$eval('#pane-sheet td[data-cell="A1"]', (el) => el.getAttribute("style") || ""), /font-weight:\s*(600|bold)/);
  }],

  ["the pointer draws a chart's place to the right of its data range", async (page) => {
    await askCopilot(page, "차트를 넣어 주세요", [[DATA, { op: "sheet.chart", range: "A1:B3", chartType: "column", title: "값" }]]);
    await acceptChange(page);
    const b3 = await cellRect(page, "B3");
    const frame = await waitForSelection(page, "right", b3.right);
    assert.ok(frame.left - b3.right < 60, "the frame starts just right of the range");
    await shot(page, "sheet-chart");
    await finishPlay(page);
    assert.equal(await page.locator("#sheet-chart-layer > *").count(), 1);
  }],

  ["the pointer selects the row header a row insert is about", async (page) => {
    await askCopilot(page, "둘째 줄 앞에 줄을 넣어 주세요", [[DATA, { op: "sheet.structure", action: "insert_rows", at: 2, count: 1 }]]);
    await acceptChange(page);
    const header = await rectOf(page, '#pane-sheet th[data-row="2"]');
    const box = await waitForSelection(page, "header", header);
    near(box.height, header.height, 3, "header height");
    await shot(page, "sheet-row-insert");
    await finishPlay(page);
    assert.equal(await page.$eval('#pane-sheet td[data-cell="A3"]', (el) => el.textContent), "가", "the row moved down");
  }],
];

const TABLE = { version: 1, activeTableId: "t1", tables: [{ id: "t1", name: "표", fields: [{ id: "f1", name: "이름", type: "text" }, { id: "f2", name: "수량", type: "number" }], records: [{ id: "r1", values: { f1: "첫째", f2: 1 } }, { id: "r2", values: { f1: "둘째", f2: 2 } }] }] };
const seedBase = (page) => page.evaluate((state) => window.__EASYOFFICE_BASE_STUDIO__.setState(state).ok, TABLE).then((ok) => assert.equal(ok, true));
const recordCount = (page) => page.evaluate(() => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].records.length);

export const copilotBasePointerSteps = [
  ["the pointer presses the new record button and a row is added", async (page) => {
    await seedBase(page);
    await askCopilot(page, "레코드를 추가해 주세요", [[{ op: "base.add_records", records: [{ 이름: "셋째", 수량: 3 }] }]]);
    await acceptChange(page);
    const button = await rectOf(page, ".base-new-record-row .base-inline-add-btn");
    const box = await waitForSelection(page, "at", button);
    near(box.width, button.width, 3, "button width");
    await shot(page, "base-record");
    await finishPlay(page);
    assert.equal(await recordCount(page), 3);
  }],

  ["the pointer goes to the cell of the row a record edit is about", async (page) => {
    await seedBase(page);
    await askCopilot(page, "둘째 수량을 고쳐 주세요", [[{ op: "base.update_records", updates: [{ rowIndex: 1, values: { 수량: 20 } }] }]]);
    // An edit of what is there is previewed first.
    await page.click('#copilot-thread button[data-copilot-action="accept"]');
    await page.waitForSelector("#eo-copilot-cursor-layer:not([hidden]) .eo-copilot-label");
    const row = await rectOf(page, "tr.base-grid-row >> nth=1");
    const box = await waitForSelection(page, "row", row);
    near(box.height, row.height, 3, "row height");
    await shot(page, "base-row");
    await finishPlay(page);
    assert.equal(await page.evaluate(() => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].records[1].values.f2), 20);
  }],
];

export const copilotBoardPointerSteps = [
  ["the pointer draws each node of a graph as a dragged box inside the canvas", async (page) => {
    const graph = { op: "board.add_graph", nodes: [{ id: "a", text: "접수" }, { id: "b", text: "검토" }, { id: "c", text: "승인" }], edges: [{ from: "a", to: "b" }, { from: "b", to: "c" }] };
    await askCopilot(page, "흐름도를 그려 주세요", [[graph]]);
    await acceptChange(page);
    const canvas = await rectOf(page, "#board-viewport");
    const boxes = await page.waitForFunction(() => {
      const nodes = Array.from(document.querySelectorAll("#eo-copilot-cursor-layer .eo-copilot-selection"));
      return nodes.length >= 2 ? nodes.map((n) => n.getBoundingClientRect().toJSON()) : null;
    }, undefined, TICK).then((h) => h.jsonValue());
    for (const box of boxes) assert.ok(box.left >= canvas.left - 1 && box.right <= canvas.right + 1 && box.top >= canvas.top - 1 && box.bottom <= canvas.bottom + 1, "a drawn box is inside the canvas");
    await shot(page, "board-graph");
    await finishPlay(page);
    assert.equal(await page.evaluate(() => window.__EASYOFFICE_BOARD_STUDIO__.getState().nodes.length), 3);
  }],
];
