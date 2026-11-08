import test from "node:test";
import assert from "node:assert/strict";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { studioItem } from "./test-support/modules.mjs";

// Merged table cells through the editor (PD-130): an opened model's `merges` are drawn as
// colspan/rowspan (covered cells are not drawn) and a save sends them back, shrunk by deleted rows.

const cell = (text) => ({ text });
const merge = (row, col, rowSpan, colSpan) => ({ row, col, rowSpan, colSpan });

// 3x3: a two-column header, a 2x2 block at the bottom left, a vertical block on the right.
const MERGED_TABLE = {
  type: "table",
  rows: [
    [cell("머리"), cell(""), cell("끝")],
    [cell("a"), cell(""), cell("세로")],
    [cell(""), cell(""), cell("")],
  ],
  merges: [merge(0, 0, 1, 2), merge(1, 0, 2, 2), merge(1, 2, 2, 1)],
};

// 4x2: rows 0-2 of column 0 are one merged region.
const TALL_TABLE = {
  type: "table",
  rows: [
    [cell("A"), cell("b0")],
    [cell(""), cell("b1")],
    [cell(""), cell("b2")],
    [cell("d0"), cell("d1")],
  ],
  merges: [merge(0, 0, 3, 1)],
};

async function openHarness(model, name = "표.docx") {
  const { sandbox } = await createWebUiDom();
  const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "DOC",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
  const converts = [];
  const harness = await createWorkspaceHarness({
    testIpc: async (command, payload) => {
      if (command === "convert_document_format" && !String(payload).includes("sourceBase64")) {
        converts.push(JSON.parse(String(payload)));
      }
      return offline[command](payload);
    },
  });
  if (model) await pickFile(harness.document, makeFixtureFile(name, { title: "표", blocks: [model] }));
  return { ...harness, converts };
}

async function savedTable(h) {
  h.userClick(studioItem(h.document, "studio-save"));
  await flushAsyncTurns(10);
  assert.equal(h.converts.length >= 1, true, "a save payload was sent");
  const blocks = h.converts.at(-1).blocks;
  return blocks.find((b) => b.type === "table");
}

const rowsOf = (table) => Array.from(table.querySelectorAll("tr"));
const cellsOf = (tr) => Array.from(tr.children);

test("an opened model draws merges as colspan/rowspan and leaves covered cells out", async () => {
  const { document } = await openHarness(MERGED_TABLE);
  const trs = rowsOf(document.getElementById("doc-editor").querySelector("table"));
  assert.equal(trs.length, 3, "a row whose cells are all covered is still a row");
  assert.deepEqual(cellsOf(trs[0]).map((c) => c.textContent), ["머리", "끝"]);
  assert.equal(cellsOf(trs[0])[0].getAttribute("colspan"), "2");
  assert.equal(cellsOf(trs[0])[1].getAttribute("colspan"), null);
  assert.deepEqual(cellsOf(trs[1]).map((c) => c.textContent), ["a", "세로"]);
  assert.equal(cellsOf(trs[1])[0].getAttribute("rowspan"), "2");
  assert.equal(cellsOf(trs[1])[0].getAttribute("colspan"), "2");
  assert.equal(cellsOf(trs[1])[1].getAttribute("rowspan"), "2");
  assert.equal(cellsOf(trs[2]).length, 0);
});

test("an unedited save sends the opened grid and merges back", async () => {
  const h = await openHarness(MERGED_TABLE);
  const table = await savedTable(h);
  assert.deepEqual(table.rows, MERGED_TABLE.rows);
  assert.deepEqual(table.merges, MERGED_TABLE.merges);
});

test("a table without merges sends no merges field", async () => {
  const h = await openHarness({ type: "table", rows: [[cell("a"), cell("b")], [cell("c"), cell("d")]] });
  const table = await savedTable(h);
  assert.equal("merges" in table, false);
  assert.equal(table.rows.length, 2);
});

test("hostile merges from an opened model are ignored instead of corrupting the table", async () => {
  const hostile = {
    type: "table",
    rows: [[cell("a"), cell("b")], [cell("c"), cell("d")]],
    merges: [
      merge(0, 0, 2, 2), merge(1, 1, 1, 1), merge(5, 5, 2, 2), merge(0, 0, 1e9, 1e9),
      merge(-1, 0, 2, 1), merge(0.5, 0, 1, 2), { row: "0" }, null, 7,
    ],
  };
  const h = await openHarness(hostile);
  const trs = rowsOf(h.document.getElementById("doc-editor").querySelector("table"));
  assert.equal(trs.length, 2);
  assert.deepEqual(cellsOf(trs[0]).map((c) => c.textContent), ["a"]);
  assert.equal(cellsOf(trs[0])[0].getAttribute("colspan"), "2");
  assert.equal(cellsOf(trs[0])[0].getAttribute("rowspan"), "2");
  assert.equal(trs[1].children.length, 0);
});

test("deleting a row inside a merged region shrinks the region", async () => {
  const h = await openHarness(TALL_TABLE);
  rowsOf(h.document.getElementById("doc-editor").querySelector("table"))[1].remove();
  const table = await savedTable(h);
  assert.deepEqual(table.rows, [
    [cell("A"), cell("b0")],
    [cell(""), cell("b2")],
    [cell("d0"), cell("d1")],
  ]);
  assert.deepEqual(table.merges, [merge(0, 0, 2, 1)]);
});

test("deleting the last rows of a region shrinks it, and a one-row region is no merge", async () => {
  const h = await openHarness(TALL_TABLE);
  const trs = rowsOf(h.document.getElementById("doc-editor").querySelector("table"));
  trs[2].remove();
  assert.deepEqual((await savedTable(h)).merges, [merge(0, 0, 2, 1)]);
  trs[1].remove();
  const table = await savedTable(h);
  assert.equal("merges" in table, false);
  assert.deepEqual(table.rows, [[cell("A"), cell("b0")], [cell("d0"), cell("d1")]]);
});

test("deleting the row of a region's top-left cell drops the region and keeps the other cells", async () => {
  const h = await openHarness(TALL_TABLE);
  rowsOf(h.document.getElementById("doc-editor").querySelector("table"))[0].remove();
  const table = await savedTable(h);
  assert.equal("merges" in table, false);
  assert.deepEqual(table.rows.map((r) => r.map((c) => c.text)), [["b1"], ["b2"], ["d0", "d1"]]);
});

test("a pasted table with colspan and rowspan keeps its merges", async () => {
  const h = await openHarness(null);
  const editor = h.document.getElementById("doc-editor");
  editor.innerHTML =
    "<table><tbody><tr><td colspan=\"2\">x</td><td>y</td></tr>" +
    "<tr><td>a</td><td rowspan=\"2\">b</td><td>c</td></tr><tr><td>d</td><td>e</td></tr></tbody></table>";
  const table = await savedTable(h);
  assert.deepEqual(table.rows.map((r) => r.map((c) => c.text)), [["x", "", "y"], ["a", "b", "c"], ["d", "", "e"]]);
  assert.deepEqual(table.merges, [merge(0, 0, 1, 2), merge(1, 1, 2, 1)]);
});

test("hidden cells of the toggled merge are not part of the grid", async () => {
  const h = await openHarness(null);
  const editor = h.document.getElementById("doc-editor");
  editor.innerHTML =
    "<table><tbody><tr><td colspan=\"2\">합</td><td hidden=\"\">감춤</td><td>z</td></tr></tbody></table>";
  const table = await savedTable(h);
  assert.deepEqual(table.rows[0].map((c) => c.text), ["합", "", "z"]);
  assert.deepEqual(table.merges, [merge(0, 0, 1, 2)]);
});

test("huge spans in the page are clamped to the table and a column cap", async () => {
  const h = await openHarness(null);
  const editor = h.document.getElementById("doc-editor");
  editor.innerHTML =
    "<table><tbody><tr><td rowspan=\"65534\" colspan=\"100000\">x</td></tr><tr><td>y</td></tr></tbody></table>";
  const table = await savedTable(h);
  assert.equal(table.merges.length, 1);
  assert.equal(table.merges[0].rowSpan, 2);
  assert.equal(table.merges[0].colSpan <= 1024, true);
  assert.equal(table.rows[0].length, table.merges[0].colSpan);
});

test("spans that would overlap another cell stop at it, so merges never overlap", async () => {
  const h = await openHarness(null);
  const editor = h.document.getElementById("doc-editor");
  editor.innerHTML =
    "<table><tbody><tr><td rowspan=\"3\">a</td><td>b</td><td rowspan=\"2\">c</td></tr>" +
    "<tr><td colspan=\"3\">d</td></tr></tbody></table>";
  const table = await savedTable(h);
  assert.deepEqual(table.rows.map((r) => r.map((c) => c.text)), [["a", "b", "c"], ["", "d", ""]]);
  assert.deepEqual(table.merges, [merge(0, 0, 2, 1), merge(0, 2, 2, 1)]);
});
