import assert from "node:assert/strict";
import test from "node:test";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { acceptProposal, reviewButton } from "./test-support/copilot-accept.mjs";
import { open, answer } from "./test-support/copilot-open.mjs";

// PD-387 on the page: the pointer plays for a sheet's format, chart and row changes and for the PDF page an answer cites, and what ends up
// written is the same with and without it (reduced motion writes at once with no pointer).

const layerOf = (document) => document.getElementById("eo-copilot-cursor-layer");
const cellOf = (document, coord) => document.querySelector(`#pane-sheet td[data-cell="${coord}"]`);

// What a sheet shows after a reply: the text and the look of the cells a test cares about and the charts that were added.
function sheetState(document) {
  const cells = ["A1", "B1", "A2", "B2", "A3", "B3", "A4"].map((coord) => {
    const cell = cellOf(document, coord);
    return [coord, cell.textContent, cell.getAttribute("style") || ""];
  });
  return JSON.stringify({ cells, charts: document.getElementById("sheet-chart-layer").children.length });
}

async function runSheet(operations, motion) {
  const h = await open({ module: "SHEET", motion, run: () => answer(operations) });
  await h.send("고쳐 주세요");
  await acceptProposal(h.document, h.userClick);
  const during = layerOf(h.document) !== null;
  await h.play(12000);
  return { h, during, state: sheetState(h.document) };
}

const data = { op: "sheet.write", cells: [{ coord: "A1", raw: "항목" }, { coord: "B1", raw: "값" }, { coord: "A2", raw: "가" }, { coord: "B2", raw: "10" }, { coord: "A3", raw: "나" }, { coord: "B3", raw: "20" }] };

const cases = [
  ["a format", [data, { op: "sheet.format", range: "A1:B1", bold: true, fill: "#ffff00" }]],
  ["a chart", [data, { op: "sheet.chart", range: "A1:B3", chartType: "column", title: "값" }]],
  ["a row insert", [data, { op: "sheet.structure", action: "insert_rows", at: 2, count: 1 }]],
];

for (const [name, operations] of cases) {
  test(`${name} is shown with the pointer and ends the same as when it is written at once`, async () => {
    const plain = await runSheet(operations, false);
    const shown = await runSheet(operations, true);
    assert.equal(plain.during, false, "reduced motion draws no pointer");
    assert.equal(shown.during, true, "the pointer is on the sheet while the change is played");
    assert.equal(layerOf(shown.h.document), null, "and gone at the end");
    assert.equal(shown.state, plain.state);
    assert.notEqual(shown.state, sheetState((await open({ module: "SHEET", run: () => answer([]) })).document), "the change did land");
  });
}

test("a hand on the page stops a sheet format and writes nothing of it", async () => {
  const h = await open({ module: "SHEET", motion: true, run: () => answer([{ op: "sheet.format", range: "A1:B1", bold: true, fill: "#ffff00" }]) });
  const before = sheetState(h.document);
  await h.send("서식을 주세요");
  await acceptProposal(h.document, h.userClick);
  await h.play(200);
  assert.ok(layerOf(h.document), "playing");
  h.document.dispatchEvent({ type: "pointerdown", target: h.document.body });
  await flushAsyncTurns(8);
  await h.play(2000);
  assert.equal(layerOf(h.document), null);
  assert.equal(sheetState(h.document), before, "the half shown format is not written");
});

test("a PDF answer that cites a page takes the pointer there and leaves the document alone", async () => {
  const ask = (motion) => open({
    module: "PDF",
    motion,
    run: () => ({ status: "ok", mode: "ask", reply: "2쪽에 있습니다", operations: [], locations: [{ module: "PDF", label: "2쪽", page: 2 }] }),
  });
  const shown = await ask(true);
  const wentTo = [];
  const hooks = shown.host.__EASYOFFICE_MODULE_FIND_HOOKS__;
  const goToPdfPage = hooks.goToPdfPage;
  hooks.goToPdfPage = (page) => {
    wentTo.push(page);
    return goToPdfPage(page);
  };
  await shown.send("어디에 있나요?");
  assert.ok(layerOf(shown.document), "the pointer goes to the cited page");
  assert.deepEqual(wentTo, [2], "the viewer is sent to the cited page");
  await shown.play(4000);
  assert.equal(layerOf(shown.document), null);
  const plain = await ask(false);
  await plain.send("어디에 있나요?");
  assert.equal(layerOf(plain.document), null, "reduced motion draws no pointer");
  assert.equal(reviewButton(shown.document, "accept"), undefined, "an answer to a question has nothing to accept");
});

// ---- the helpers the pointer reads from the HWP editor and the board

const PARAGRAPHS = ["첫째", "둘째", "셋째"];

async function extOpsWithEditor(caret, focus = { focused: true, page: 1 }) {
  const h = await open({ module: "DOC", run: () => answer([]) });
  const calls = [];
  const frame = { getBoundingClientRect: () => ({ left: 100, top: 50, right: 900, bottom: 650, width: 800, height: 600 }) };
  const hwpHost = {
    usesLegacy: () => false,
    getStatus: () => "ready",
    onChange() {},
    onStatus() {},
    getFrame: () => frame,
    call: async (method, params) => {
      calls.push([method, params]);
      if (method === "getCaretRect") return caret;
      if (method === "focusTarget") return focus;
      return {
        documentGeneration: 1,
        pageCount: 1,
        totalParagraphs: PARAGRAPHS.length,
        paragraphs: PARAGRAPHS.map((text, index) => ({ index, section: 0, paragraph: index, text })),
        selection: { hasSelection: false },
      };
    },
  };
  const ops = h.host.__EASYOFFICE_CREATE_COPILOT_EXT_OPS__({ document: h.document, getHwpHost: () => hwpHost });
  await ops.refreshHwpSnapshot();
  calls.length = 0;
  return { ops, calls };
}

test("the paragraph rect moves the editor to the paragraph, reads the caret read only and adds the frame offset", async () => {
  const { ops, calls } = await extOpsWithEditor({ visible: true, left: 30, top: 70, width: 2, height: 18 });
  assert.deepEqual(await ops.hwpParagraphRect(1), { left: 130, top: 120, width: 320, height: 18 });
  assert.deepEqual(calls.map(([method]) => method), ["focusTarget", "getCaretRect"]);
  assert.deepEqual(calls[0][1].target, { kind: "body_paragraph", section: 0, paragraph: 1, charOffset: 0, length: 2 });
  assert.equal((await ops.hwpParagraphRect("end")) !== null, true);
  assert.equal(calls.at(-2)[1].target.paragraph, 2, "the end is the last paragraph");
});

test("the paragraph rect is empty when the editor shows no caret or does not know the paragraph", async () => {
  const hidden = await extOpsWithEditor({ visible: false, left: 0, top: 0, width: 0, height: 0 });
  assert.equal(await hidden.ops.hwpParagraphRect(0), null);
  const known = await extOpsWithEditor({ visible: true, left: 1, top: 1, width: 2, height: 10 });
  assert.equal(await known.ops.hwpParagraphRect(40), null);
  assert.equal(known.calls.length, 0, "an unknown paragraph asks the editor nothing");
});

test("a jump the editor refuses gives no rect: the caret would still be where it was before", async () => {
  const refused = await extOpsWithEditor({ visible: true, left: 30, top: 70, width: 2, height: 18 }, { focused: false, page: 1 });
  assert.equal(await refused.ops.hwpParagraphRect(1), null);
  assert.deepEqual(refused.calls.map(([method]) => method), ["focusTarget"], "the stale caret is not even read");
});

test("the board frames of a graph are the ones the apply lays out, below the nodes that are there", async () => {
  const h = await open({ module: "BOARD", run: () => answer([]) });
  const ops = h.host.__EASYOFFICE_CREATE_COPILOT_EXT_OPS__({ document: h.document });
  const graph = { nodes: [{ id: "a" }, { id: "b" }], edges: [{ from: "a", to: "b" }] };
  const first = ops.boardNewFrames(graph, []);
  assert.deepEqual(first.map((f) => [f.w, f.h]), [[164, 84], [164, 84]]);
  const below = ops.boardNewFrames(graph, [{ x: 0, y: 0, w: 164, h: 84 }]);
  assert.ok(below.every((f) => f.y >= 84 + 96), "starts under the existing node");
});

// ---- the editor's read-only caret query

test("the editor answers the caret query from the caret element only and the router takes no parameters for it", async () => {
  const { readCaretRect } = await import("../apps/hwp-studio/src/embed/copilot-bridge.ts");
  const root = (caret) => ({ querySelector: (selector) => (selector === ".caret" ? caret : null) });
  const box = (rect, display = "block") => ({ getBoundingClientRect: () => rect, style: { display } });
  assert.deepEqual(readCaretRect(root(box({ left: 3, top: 4, width: 2, height: 16 }))), { visible: true, left: 3, top: 4, width: 2, height: 16 });
  const hidden = { visible: false, left: 0, top: 0, width: 0, height: 0 };
  assert.deepEqual(readCaretRect(root(null)), hidden);
  assert.deepEqual(readCaretRect(root(box({ left: 3, top: 4, width: 2, height: 16 }, "none"))), hidden);
  assert.deepEqual(readCaretRect(root(box({ left: Number.NaN, top: 4, width: 2, height: 16 }))), hidden);
  assert.deepEqual(readCaretRect(root(box({ left: 3, top: 4, width: 2, height: 0 }))), hidden);
  const fs = await import("node:fs");
  const read = (rel) => fs.readFileSync(new URL("../apps/hwp-studio/src/" + rel, import.meta.url), "utf8");
  assert.match(read("embed/rpc-router.ts"), /case 'getCaretRect':\s+assertEmptyParams\(params, 'getCaretRect params'\);\s+return handlers\.getCaretRect\(\);/);
  assert.match(read("main.ts"), /async getCaretRect\(\) \{\s+await initPromise;\s+return readCaretRect\(document\);/);
  const { EMBED_CAPABILITIES } = await import("../apps/hwp-studio/src/embed/protocol.ts");
  assert.ok(EMBED_CAPABILITIES.includes("caret-rect-v1"));
});

test("the shell names target-navigation-v1, without which the editor refuses every focusTarget (found in a real WebKit run)", async () => {
  const fs = await import("node:fs");
  const host = fs.readFileSync(new URL("../crates/easyoffice-app/ui/hwp-host.js", import.meta.url), "utf8");
  assert.match(host, /const CAPABILITIES = \[[^\]]*"target-navigation-v1"[^\]]*\]/);
  const { EMBED_CAPABILITIES } = await import("../apps/hwp-studio/src/embed/protocol.ts");
  assert.ok(EMBED_CAPABILITIES.includes("target-navigation-v1"), "the editor offers it");
});

test("a PDF answer that cites no page draws no pointer", async () => {
  const h = await open({ module: "PDF", motion: true, run: () => ({ status: "ok", mode: "ask", reply: "요약입니다", operations: [], locations: [] }) });
  await h.send("요약해 주세요");
  assert.equal(layerOf(h.document), null);
});
