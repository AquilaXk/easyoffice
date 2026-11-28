import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// PD-387: the pointer performance of the sheet's format, chart and structure changes, of the BASE and BOARD changes, of the embedded
// HWP editor and of a PDF page the answer cites. What a step is, where a stopped performance cuts, and that the performance never
// changes what is written.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const MODULES = ["sheet-refs.js", "copilot-timeline.js", "copilot-perform-slide.js", "copilot-perform-sheet.js", "copilot-perform-ext.js", "copilot-perform-hwp.js", "copilot-perform-cite.js", "copilot-perform.js"];

const rectEl = (left, top, width, height, extra = {}) => ({
  getBoundingClientRect: () => ({ left, top, width, height, right: left + width, bottom: top + height }),
  scrollIntoView() {},
  dataset: {},
  ...extra,
});

// Time that jumps a second per reading and frames that come at once: a scene of any length plays in a few ticks.
function instantTiming() {
  let t = 0;
  return { now: () => (t += 1000), requestAnimationFrame: (fn) => setImmediate(() => fn(t)), cancelAnimationFrame: (id) => clearImmediate(id) };
}

function stubCursor(log, { hangOnMove = false } = {}) {
  let release = null;
  return {
    timing: instantTiming(),
    show() {},
    hide() {},
    destroy() {},
    place() {},
    moveTo(x, y) {
      log.push(["move", Math.round(x), Math.round(y)]);
      if (!hangOnMove) return Promise.resolve(true);
      return new Promise((resolve) => {
        release = resolve;
      });
    },
    animate(_ms, step) {
      step(1);
      return Promise.resolve(true);
    },
    press: (down) => log.push(["press", down]),
    setSelection: (rects) => log.push(["select", rects.length]),
    setCaret() {},
    setIdle() {},
    interrupt() {
      release?.(false);
      release = null;
    },
  };
}

function build({ elements = {}, lists = {}, hwpApi = null, boardApi = null, goToPdfPage = () => {}, reduced = false, log = [], hang = false, deck = [] } = {}) {
  const env = {};
  for (const name of MODULES) loadUiModule(UI(name), env);
  const handlers = new Map();
  const document = {
    addEventListener: (type, fn) => handlers.set(type, fn),
    removeEventListener: (type) => handlers.delete(type),
    getElementById: (id) => elements["#" + id] ?? null,
    querySelector: (selector) => elements[selector] ?? null,
    querySelectorAll: (selector) => lists[selector] ?? [],
  };
  const performer = env.__EASYOFFICE_CREATE_COPILOT_PERFORMER__({
    document,
    docBlocksOf: () => [],
    getWorkspacePanels: () => null,
    slideDeckState: deck,
    getActiveSlideIndex: () => 0,
    getActiveSheetName: () => "Sheet1",
    getSheetController: () => null,
    blockText: (x) => String(x?.text ?? x),
    isReducedMotion: () => reduced,
    createCursor: () => stubCursor(log, { hangOnMove: hang }),
    createTypist: () => ({ type: async () => ({ typed: 0 }), stop() {} }),
    hwpApi,
    boardApi,
    goToPdfPage,
  });
  const touch = () => handlers.get("pointerdown")?.({ type: "pointerdown", target: { closest: () => null } });
  return { performer, touch, log };
}

const kinds = (targets) => targets.map((t) => t.kind);
const sheetCells = () => {
  const out = {};
  for (let r = 1; r <= 6; r += 1) for (const [c, letter] of [[0, "A"], [1, "B"], [2, "C"]]) out[`#sheet-grid td[data-cell="${letter}${r}"]`] = rectEl(60 + c * 80, 40 + r * 22, 80, 22);
  return out;
};

// ---- SHEET

test("a sheet format, chart, structure change and other range changes each become one step, and a freeze does not", () => {
  const { performer } = build();
  const ops = [
    { op: "sheet.write", cells: [{ coord: "A1", raw: "x" }] },
    { op: "sheet.format", range: "A1:B2", bold: true, fill: "#ffff00" },
    { op: "sheet.chart", range: "A1:B5", chartType: "column" },
    { op: "sheet.structure", action: "insert_rows", at: 3, count: 2 },
    { op: "sheet.structure", action: "freeze", freezeRows: 1 },
    { op: "sheet.sort_filter", action: "sort", range: "A1:C9" },
    { op: "sheet.note", coord: "b2", text: "메모" },
  ];
  const targets = performer.collectTargets("SHEET", ops);
  assert.deepEqual(kinds(targets), ["sheet-cell", "sheet-format", "sheet-chart", "sheet-structure", "sheet-range", "sheet-range"]);
  assert.deepEqual(targets[1].box, { r1: 1, c1: 1, r2: 2, c2: 2 });
  assert.deepEqual(targets[1].buttons, ["bold", "fill"]);
  assert.deepEqual(targets[3], { ...targets[3], axis: "row", at: 3, count: 2, insert: true });
  assert.deepEqual(targets[5].box, { r1: 2, c1: 2, r2: 2, c2: 2 });
});

test("a range that does not read as a range is left to the final write, which reports it", () => {
  const { performer } = build();
  const targets = performer.collectTargets("SHEET", [{ op: "sheet.format", range: "여기", bold: true }, { op: "sheet.chart", range: "" }]);
  assert.deepEqual(targets, []);
});

test("a range that names another sheet keeps that sheet", () => {
  const { performer } = build();
  const [target] = performer.collectTargets("SHEET", [{ op: "sheet.format", sheet: "Data", range: "Data!C3:D4", bold: true }]);
  assert.equal(target.sheet, "Data");
  assert.deepEqual(target.box, { r1: 3, c1: 3, r2: 4, c2: 4 });
});

test("the sheet steps of one reply are capped", () => {
  const { performer } = build();
  const ops = Array.from({ length: 40 }, (_, i) => ({ op: "sheet.format", range: "A" + (i + 1), bold: true }));
  assert.equal(performer.collectTargets("SHEET", ops).length, performer.MAX_SHEET_STEPS);
  assert.equal(performer.MAX_SHEET_STEPS, 24);
});

test("a stopped sheet step drops its change and keeps the ones before it", () => {
  const { performer } = build();
  const ops = [{ op: "sheet.format", range: "A1", bold: true }, { op: "sheet.chart", range: "A1:B5" }, { op: "sheet.structure", action: "delete_cols", at: 2 }];
  const targets = performer.collectTargets("SHEET", ops);
  assert.deepEqual(performer.cutOperations(ops, targets, { target: targets[1], typed: 0 }), [ops[0]]);
  assert.deepEqual(performer.cutOperations(ops, targets, null), ops, "no stop keeps every change as it was");
});

test("the pointer drags the range, presses the format buttons and leaves the written change as it was", async () => {
  const log = [];
  const elements = { ...sheetCells(), "#btn-stylebar-bold": rectEl(10, 10, 24, 24), "#btn-sheet-fill": rectEl(50, 10, 24, 24) };
  const { performer } = build({ elements, log });
  const ops = [{ op: "sheet.format", range: "A1:B2", bold: true, fill: "#ffff00" }];
  const run = performer.perform("SHEET", ops);
  assert.equal(run.active, true);
  const result = await run.done;
  assert.deepEqual(result.operations, ops);
  assert.equal(result.completed, true);
  const moves = log.filter((e) => e[0] === "move");
  assert.ok(moves.some(([, x, y]) => x < 140 && y > 60), "went to the first cell");
  assert.ok(moves.some(([, x, y]) => x === 22 && y === 22), "went to the bold button");
  assert.ok(moves.some(([, x, y]) => x === 62 && y === 22), "went to the fill button");
  assert.ok(log.some((e) => e[0] === "press" && e[1] === true), "pressed");
  assert.ok(log.some((e) => e[0] === "select" && e[1] === 1), "selected the range");
  assert.deepEqual(log.at(-1), ["select", 0], "the selection is cleared at the end");
});

test("a step whose place is not on the screen is skipped without a pointer move and the change is still written", async () => {
  const log = [];
  const { performer } = build({ log });
  const ops = [{ op: "sheet.format", range: "A1:B2", bold: true }, { op: "sheet.structure", action: "insert_cols", at: 4 }];
  const result = await performer.perform("SHEET", ops).done;
  assert.deepEqual(result.operations, ops);
  assert.equal(log.filter((e) => e[0] === "move").length, 0);
});

test("a chart draws its place to the right of the data range and a row change selects the header", async () => {
  const log = [];
  const elements = {
    ...sheetCells(),
    "#sheet-viewport": rectEl(0, 0, 900, 500),
    '[data-menu="insert-object"] > .menu-title': rectEl(300, 4, 40, 20),
    '#pane-sheet th[data-row="3"]': rectEl(0, 106, 56, 22),
    '#pane-sheet th[data-row="4"]': rectEl(0, 128, 56, 22),
  };
  const { performer } = build({ elements, log });
  const ops = [{ op: "sheet.chart", range: "A1:B5" }, { op: "sheet.structure", action: "delete_rows", at: 3, count: 2 }];
  const result = await performer.perform("SHEET", ops).done;
  assert.deepEqual(result.operations, ops);
  assert.ok(log.some(([k, x, y]) => k === "move" && x === 320 && y === 14), "clicked the insert menu");
  const header = log.filter((e) => e[0] === "move").at(-1);
  assert.equal(header[1], 28);
  assert.equal(header[2], 128, "the middle of the two row headers");
});

test("an interrupted sheet performance writes only what was finished", async () => {
  const log = [];
  const elements = { ...sheetCells(), "#btn-stylebar-bold": rectEl(10, 10, 24, 24) };
  const { performer, touch } = build({ elements, log, hang: true });
  const ops = [{ op: "sheet.format", range: "A1", bold: true }, { op: "sheet.format", range: "B2", bold: true }];
  const run = performer.perform("SHEET", ops);
  await new Promise((resolve) => setImmediate(resolve));
  touch();
  const result = await run.done;
  assert.equal(result.interrupted, true);
  assert.deepEqual(result.operations, [], "the format that was being shown is not written half way");
});

test("reduced motion and an empty reply write at once without a pointer", async () => {
  const { performer, log } = build({ reduced: true });
  const ops = [{ op: "sheet.format", range: "A1", bold: true }];
  const run = performer.perform("SHEET", ops);
  assert.equal(run.active, false);
  assert.deepEqual((await run.done).operations, ops);
  assert.equal(log.length, 0);
  assert.equal(build().performer.perform("SHEET", [{ op: "sheet.structure", action: "freeze" }]).active, false);
});

// ---- HWP

test("a legacy hwp editor takes the document steps and an embedded one takes paragraph steps", () => {
  const ops = [{ op: "doc.replace_span", index: 2, search: "가", replace: "나" }, { op: "doc.replace_paragraphs", replacements: [{ index: 4, text: "다" }, { index: 6, text: "라" }] }, { op: "doc.append", blocks: ["끝"] }];
  const legacy = build({ hwpApi: { isEmbed: () => false } });
  assert.deepEqual(kinds(legacy.performer.collectTargets("HWP", ops)), ["doc-replace-span", "doc-replace", "doc-replace", "doc-append"]);
  const embed = build({ hwpApi: { isEmbed: () => true } });
  const targets = embed.performer.collectTargets("HWP", ops);
  assert.deepEqual(kinds(targets), ["hwp-paragraph", "hwp-paragraph", "hwp-paragraph", "hwp-paragraph"]);
  assert.deepEqual(targets.map((t) => t.index), [2, 4, 6, "end"]);
});

test("the embedded editor steps are capped and a stop keeps the paragraphs that were finished", () => {
  const { performer } = build({ hwpApi: { isEmbed: () => true } });
  const many = [{ op: "doc.replace_paragraphs", replacements: Array.from({ length: 30 }, (_, i) => ({ index: i, text: "글" + i })) }];
  const targets = performer.collectTargets("HWP", many);
  assert.equal(targets.length, 12);
  const cut = performer.cutOperations(many, targets, { target: targets[3], typed: 0 });
  assert.deepEqual(cut[0].replacements.map((r) => r.index), [0, 1, 2]);
  assert.deepEqual(performer.cutOperations(many, targets, { target: targets[0], typed: 0 }), []);
});

test("the embedded editor performance goes to the paragraph the editor reports and writes the change unchanged", async () => {
  const log = [];
  const asked = [];
  const hwpApi = {
    isEmbed: () => true,
    frame: () => rectEl(100, 50, 800, 600),
    paragraphRect: async (index) => {
      asked.push(index);
      return { left: 140, top: 200, width: 400, height: 20 };
    },
  };
  const { performer } = build({ hwpApi, log });
  const ops = [{ op: "doc.replace_paragraphs", replacements: [{ index: 3, text: "새 글" }] }, { op: "doc.append", blocks: ["끝"] }];
  const result = await performer.perform("HWP", ops).done;
  assert.deepEqual(result.operations, ops);
  assert.deepEqual(asked, [3, "end"]);
  assert.ok(log.some(([k, x, y]) => k === "move" && x === 144 && y === 210), "went to the paragraph");
});

test("an editor that cannot tell the paragraph place is pointed at over its frame", async () => {
  const log = [];
  const hwpApi = { isEmbed: () => true, frame: () => rectEl(100, 50, 800, 600), paragraphRect: async () => null };
  const { performer } = build({ hwpApi, log });
  const ops = [{ op: "doc.replace_span", index: 0, search: "가", replace: "나" }];
  const result = await performer.perform("HWP", ops).done;
  assert.deepEqual(result.operations, ops);
  assert.ok(log.some(([k, x, y]) => k === "move" && x === 500 && y === 350), "the middle of the frame");
});

// ---- BASE

test("a BASE reply becomes a step for each new record, each edited row and each new field", () => {
  const { performer } = build();
  const ops = [
    { op: "base.add_field", name: "메모", fieldType: "text" },
    { op: "base.add_records", records: [{ 이름: "가" }, { 이름: "나" }] },
    { op: "base.update_records", updates: [{ rowIndex: 1, values: { 이름: "다" } }] },
    { op: "base.set_view", view: "kanban" },
  ];
  const targets = performer.collectTargets("BASE", ops);
  assert.deepEqual(kinds(targets), ["base-field", "base-record", "base-record", "base-row"]);
  assert.deepEqual(targets[3], { ...targets[3], rowIndex: 1, fieldName: "이름" });
});

test("the BASE steps are capped and a stop keeps the records that were finished", () => {
  const { performer } = build();
  const ops = [{ op: "base.add_records", records: Array.from({ length: 30 }, (_, i) => ({ 이름: "r" + i })) }, { op: "base.update_records", updates: [{ rowIndex: 0, values: { 이름: "x" } }] }];
  const targets = performer.collectTargets("BASE", ops);
  assert.equal(targets.length, performer.MAX_BASE_STEPS);
  assert.equal(performer.MAX_BASE_STEPS, 20);
  const cut = performer.cutOperations(ops, targets, { target: targets[5], typed: 0 });
  assert.equal(cut.length, 1);
  assert.equal(cut[0].records.length, 5);
  assert.deepEqual(performer.cutOperations(ops, targets, { target: targets[0], typed: 0 }), []);
});

test("the pointer clicks the new record row, the header end and an edited row, and the written change stays the same", async () => {
  const log = [];
  const header = (id, name, left) => rectEl(left, 100, 100, 24, { dataset: { fieldId: id }, querySelector: () => ({ textContent: name }) });
  const row = rectEl(0, 130, 300, 24, { querySelector: (sel) => (sel === 'td[data-field-id="f1"]' ? rectEl(40, 130, 100, 24) : null) });
  const elements = { ".base-new-record-row .base-inline-add-btn": rectEl(10, 300, 80, 24) };
  const lists = { "th.base-grid-th": [header("f1", "이름", 40), header("f2", "메모", 140)], "tr.base-grid-row": [row] };
  const { performer } = build({ elements, lists, log });
  const ops = [{ op: "base.add_field", name: "새 필드" }, { op: "base.add_records", records: [{ 이름: "가" }] }, { op: "base.update_records", updates: [{ rowIndex: 0, values: { 이름: "다" } }] }];
  const result = await performer.perform("BASE", ops).done;
  assert.deepEqual(result.operations, ops);
  const moves = log.filter((e) => e[0] === "move");
  assert.ok(moves.some(([, x, y]) => x === 252 && y === 112), "the end of the last header");
  assert.ok(moves.some(([, x, y]) => x === 50 && y === 312), "the new record button");
  assert.ok(moves.some(([, x, y]) => x === 90 && y === 142), "the cell of the edited row");
});

// ---- BOARD

test("a board graph becomes a step for each node it draws, capped", () => {
  const boardApi = { frames: (op, existing) => op.nodes.map((_, i) => ({ x: existing.length * 10 + i * 200, y: 0, w: 164, h: 84 })), existing: () => [] };
  const { performer } = build({ boardApi });
  const ops = [{ op: "board.add_graph", nodes: Array.from({ length: 20 }, (_, i) => ({ id: "n" + i, text: "노드" + i })), edges: [{ from: "n0", to: "n1" }] }];
  const targets = performer.collectTargets("BOARD", ops);
  assert.equal(targets.length, performer.MAX_BOARD_NODES);
  assert.equal(performer.MAX_BOARD_NODES, 12);
  assert.deepEqual(kinds(targets.slice(0, 2)), ["board-node", "board-node"]);
});

test("a stopped board graph keeps the nodes that were drawn and the connections between them", () => {
  const boardApi = { frames: (op) => op.nodes.map((_, i) => ({ x: i * 200, y: 0, w: 164, h: 84 })), existing: () => [] };
  const { performer } = build({ boardApi });
  const ops = [{ op: "board.add_graph", nodes: [{ id: "a" }, { id: "b" }, { id: "c" }], edges: [{ from: "a", to: "b" }, { from: "b", to: "c" }] }];
  const targets = performer.collectTargets("BOARD", ops);
  const cut = performer.cutOperations(ops, targets, { target: targets[2], typed: 0 });
  assert.deepEqual(cut, [{ op: "board.add_graph", nodes: [{ id: "a" }, { id: "b" }], edges: [{ from: "a", to: "b" }] }]);
  assert.deepEqual(performer.cutOperations(ops, targets, { target: targets[0], typed: 0 }), []);
});

test("the board pointer draws each node as a dragged box inside the canvas and writes the graph unchanged", async () => {
  const log = [];
  const boardApi = { frames: (op) => op.nodes.map((_, i) => ({ x: i * 200, y: 0, w: 164, h: 84 })), existing: () => [] };
  const elements = { "#board-viewport": rectEl(200, 100, 800, 500) };
  const { performer } = build({ boardApi, elements, log });
  const ops = [{ op: "board.add_graph", nodes: [{ id: "a" }, { id: "b" }], edges: [{ from: "a", to: "b" }] }];
  const result = await performer.perform("BOARD", ops).done;
  assert.deepEqual(result.operations, ops);
  const moves = log.filter((e) => e[0] === "move");
  assert.equal(moves.length >= 2, true);
  for (const [, x, y] of moves) assert.ok(x >= 200 && x <= 1000 && y >= 100 && y <= 600, "inside the canvas");
  assert.ok(log.some((e) => e[0] === "select" && e[1] === 2), "the first node stays marked while the second is drawn");
});

// ---- PDF

test("a PDF answer points at the pages it cites once each, in order, up to the cap", () => {
  const { performer } = build();
  const ops = [3, 3, 1, 9, 7, 5].map((page) => ({ op: "pdf.point", page }));
  const targets = performer.collectTargets("PDF", ops);
  assert.deepEqual(targets.map((t) => t.page), [3, 1, 9]);
  assert.equal(performer.MAX_PDF_PAGES, 3);
});

test("the pointer goes to the cited page, marks the page card and leaves the PDF as it was", async () => {
  const log = [];
  const pages = [];
  const elements = { "#pdf-page-card": rectEl(300, 120, 500, 650) };
  const { performer } = build({ elements, log, goToPdfPage: (page) => pages.push(page) });
  const run = performer.perform("PDF", [{ op: "pdf.point", page: 2 }]);
  assert.equal(run.active, true);
  await run.done;
  assert.deepEqual(pages, [2]);
  assert.ok(log.some(([k, x, y]) => k === "move" && x === 550 && y === 445));
  assert.ok(log.some((e) => e[0] === "select" && e[1] === 1));
  assert.deepEqual(log.at(-1), ["select", 0]);
  assert.equal(build({ reduced: true }).performer.perform("PDF", [{ op: "pdf.point", page: 2 }]).active, false);
});
