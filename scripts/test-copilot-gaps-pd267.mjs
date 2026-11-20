import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// PD-267: what the copilot page code sends and what it refuses. Context indexes, the omitted-paragraph count, the
// selection command target, undo after the user edited, and a sheet name that does not exist.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));

function fakeDocument(texts) {
  const blocks = [];
  const editor = {
    contains: (node) => node === editor || blocks.includes(node) || blocks.some((b) => node?.parentElement === b),
    querySelectorAll: () => blocks,
  };
  texts.forEach((text) => {
    const textNode = { nodeType: 3, data: text };
    const block = { nodeType: 1, textContent: text, hidden: false, classList: { contains: () => false }, parentElement: editor, childNodes: [textNode] };
    textNode.parentElement = block;
    block.textNode = textNode;
    blocks.push(block);
  });
  const zones = {};
  const zone = (id, text) => {
    const textNode = { nodeType: 3, data: text };
    const el = { nodeType: 1, id, textContent: text, contains: (n) => n === el || n === textNode, parentElement: null, childNodes: [textNode] };
    textNode.parentElement = el;
    el.textNode = textNode;
    zones[id] = el;
    return el;
  };
  zone("slide-title-zone", "발표 제목");
  zone("slide-lower-zone", "첫째 줄\n둘째 줄");
  const document = { getElementById: (id) => (id === "doc-editor" ? editor : zones[id] || null) };
  return { document, editor, blocks, zones };
}

function selectionOver(startNode, endNode, text = "선택") {
  const range = {
    startContainer: startNode,
    endContainer: endNode,
    commonAncestorContainer: startNode,
    toString: () => text,
  };
  return { rangeCount: 1, isCollapsed: false, getRangeAt: () => range };
}

function createActions({ texts = [], epoch = () => 1, selection = null, slides = [], sheetStore = { S: {} }, activeModule = "DOC", restore = async () => {} } = {}) {
  const env = {};
  loadUiModule(UI("sheet-refs.js"), env);
  loadUiModule(UI("copilot-text.js"), env);
  loadUiModule(UI("copilot-actions.js"), env);
  const dom = fakeDocument(texts);
  env.getSelection = () => selection;
  const restored = [];
  const instance = env.__EASYOFFICE_CREATE_COPILOT_ACTIONS__({
    document: dom.document,
    sheetDataStore: sheetStore,
    sheetRowCounts: {},
    slideDeckState: slides,
    getActiveModule: () => activeModule,
    getActiveSheetName: () => "S",
    getActiveSlideIndex: () => 0,
    getSheetController: () => ({
      restoreDomSheetCells() {},
      async recalculateActiveSheetFormulas() {},
      syncSheetFormatToolbar() {},
      snapshotDomSheetCells: () => ({}),
    }),
    getWorkspacePanels: () => ({}),
    getPdfEdit: () => null,
    activatePane: async () => {},
    captureWorkspaceSnapshot: () => ({ snapshot: true }),
    restoreWorkspaceSnapshot: async (snap) => {
      restored.push(snap);
      return restore();
    },
    commitDeferredUndoSnapshot: () => {},
    getHistoryMutationEpoch: () => epoch(),
    noteActivity: () => {},
    extractEditableDocumentText: () => "",
    localScrubPii: () => ({ totalMasked: 0, scrubbedText: "" }),
  });
  return { instance, dom, restored, env };
}

test("document context keeps one entry per block so an index is a block index, and counts what it left out", () => {
  const texts = Array.from({ length: 205 }, (_, i) => (i === 1 ? "" : "문단 " + i));
  const { instance } = createActions({ texts });
  const { context } = instance.extractContext("DOC");
  assert.equal(context.doc.paragraphs.length, 200, "the window matches the 200 paragraphs the engine puts in the prompt");
  assert.equal(context.doc.paragraphs[1], "", "an empty block keeps its place");
  assert.equal(context.doc.paragraphs[2], "문단 2");
  assert.equal(context.doc.omittedParagraphs, 5);

  const small = createActions({ texts: ["가", "나"] }).instance.extractContext("DOC");
  assert.equal(small.context.doc.omittedParagraphs, 0);
});

test("a text selection command targets the blocks the selection touches and reaches past the context window", () => {
  const texts = Array.from({ length: 260 }, (_, i) => "문단 " + i);
  const near = createActions({ texts });
  near.env.getSelection = () => selectionOver(near.dom.blocks[3].textNode, near.dom.blocks[4].textNode);
  assert.deepEqual(near.instance.getCommandTarget("DOC").selection, { range: "선택 텍스트 2자", paragraphIndex: 3, paragraphEnd: 4 });
  near.env.getSelection = () => selectionOver({ nodeType: 3, parentElement: null }, { nodeType: 3, parentElement: null });
  assert.equal(near.instance.getCommandTarget("DOC"), null, "a selection outside the document is no target");

  const far = createActions({ texts });
  far.env.getSelection = () => selectionOver(far.dom.blocks[230].textNode, far.dom.blocks[231].textNode);
  const farTarget = far.instance.getCommandTarget("DOC");
  const { selection: sel, context } = far.instance.extractContext("DOC", { target: farTarget });
  assert.equal(sel.paragraphIndex, 230);
  assert.equal(context.doc.paragraphs.length, 232, "padded up to the last selected block");
  assert.equal(context.doc.paragraphs[231], "문단 231");
  assert.equal(context.doc.paragraphs[210], "", "outside the window and the selection stays empty");
  assert.equal(context.doc.omittedParagraphs, 28);
});

test("a selection inside the slide title or body is a command target for that field", () => {
  const slides = [{ title: "발표 제목", lower: "첫째 줄\n둘째 줄" }];
  const title = createActions({ slides, activeModule: "SLIDE" });
  title.env.getSelection = () => selectionOver(title.dom.zones["slide-title-zone"].textNode, title.dom.zones["slide-title-zone"].textNode);
  assert.deepEqual(title.instance.getCommandTarget("SLIDE").selection, { range: "슬라이드 1", slideIndex: 0, slideField: "title" });
  const body = createActions({ slides, activeModule: "SLIDE" });
  body.env.getSelection = () => selectionOver(body.dom.zones["slide-lower-zone"].textNode, body.dom.zones["slide-lower-zone"].textNode);
  assert.equal(body.instance.getCommandTarget("SLIDE").selection.slideField, "body");
  assert.equal(createActions({ slides, activeModule: "SLIDE" }).instance.getCommandTarget("SLIDE"), null, "no selection, no target");
});

const writeCell = [{ op: "sheet.write", sheet: "S", cells: [{ coord: "A1", raw: "1" }] }];

test("undo is available right after applying and gone once the user edited", async () => {
  let epoch = 1;
  const { instance, restored } = createActions({ epoch: () => epoch, activeModule: "SHEET" });
  await instance.applyOperations("SHEET", "edit", writeCell, "t1");
  assert.equal(instance.canUndoTurn("t1"), true);
  epoch += 1;
  assert.equal(instance.canUndoTurn("t1"), false, "the history moved");
  assert.equal(await instance.undoLastTurn("t1"), false);
  assert.equal(restored.length, 0, "nothing is restored over the user's edit");
  assert.equal(instance.canUndoTurn("t1"), false);

  const fresh = createActions({ epoch: () => 7, activeModule: "SHEET" });
  await fresh.instance.applyOperations("SHEET", "edit", writeCell, "t2");
  assert.equal(await fresh.instance.undoLastTurn("t2"), true);
  assert.equal(fresh.restored.length, 1);
});

test("an operation on a sheet that does not exist is refused and creates no sheet", async () => {
  const sheetStore = { S: {} };
  const { instance } = createActions({ sheetStore, activeModule: "SHEET" });
  for (const sheet of ["없는 시트", "constructor", "__proto__"]) {
    const ops = [{ op: "sheet.write", sheet, cells: [{ coord: "A1", raw: "1" }] }];
    await assert.rejects(instance.applyOperations("SHEET", "edit", ops, "t"), /시트/);
  }
  assert.deepEqual(Object.keys(sheetStore), ["S"]);
  await instance.applyOperations("SHEET", "edit", writeCell, "ok");
  assert.equal(sheetStore.S.A1.value, "1");
});
