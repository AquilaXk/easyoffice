import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// PD-267: the copilot page never sends a lone surrogate (the engine refuses the whole request for one), asks a command
// again only for a target that did not change, and paints a selection on paper in a translucent blue in both themes.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const wellFormed = (text) => text.isWellFormed();

function loadText() {
  const env = {};
  loadUiModule(UI("copilot-text.js"), env);
  loadUiModule(UI("copilot-diff.js"), env);
  loadUiModule(UI("copilot-typing.js"), env);
  loadUiModule(UI("copilot-cursor.js"), env);
  loadUiModule(UI("copilot-timeline.js"), env);
  loadUiModule(UI("copilot-preview.js"), env);
  loadUiModule(UI("copilot-perform-slide.js"), env);
  loadUiModule(UI("copilot-perform.js"), env);
  return env.__EASYOFFICE_COPILOT_TEXT__.clipCodePoints;
}

test("clipCodePoints cuts at a code point and never leaves half a surrogate pair", () => {
  const clip = loadText();
  assert.equal(clip("abcdef", 3), "abc");
  assert.equal(clip("abc", 3), "abc");
  assert.equal(clip("가나다", 10), "가나다");
  assert.equal(clip(null, 5), "");
  assert.equal(clip(undefined, 5), "");
  // `slice(0, 2)` of "a😀b" is "a" and the high half of the pair.
  assert.equal("a😀b".slice(0, 2).isWellFormed(), false, "the old cut broke the pair");
  assert.equal(clip("a😀b", 2), "a😀");
  assert.equal(clip("a😀b", 1), "a");
  const text = "😀".repeat(300);
  for (const max of [1, 2, 199, 200, 201]) {
    const out = clip(text, max);
    assert.ok(wellFormed(out), `max ${max}`);
    assert.equal(Array.from(out).length, max);
  }
  // At the 200-unit and 24,000-unit limits the page used before.
  assert.ok(wellFormed(clip("a".repeat(199) + "😀😀", 200)));
  assert.ok(wellFormed(clip("나".repeat(23_999) + "😀😀", 24_000)));
});

function makeActions(texts, selectedText = "") {
  const env = {};
  loadUiModule(UI("copilot-text.js"), env);
  loadUiModule(UI("copilot-diff.js"), env);
  loadUiModule(UI("copilot-typing.js"), env);
  loadUiModule(UI("copilot-cursor.js"), env);
  loadUiModule(UI("copilot-timeline.js"), env);
  loadUiModule(UI("copilot-preview.js"), env);
  loadUiModule(UI("copilot-perform-slide.js"), env);
  loadUiModule(UI("copilot-perform.js"), env);
  loadUiModule(UI("sheet-refs.js"), env);
  loadUiModule(UI("copilot-edit-mode.js"), env);
  loadUiModule(UI("copilot-actions.js"), env);
  const blocks = [];
  const editor = { contains: () => true, querySelectorAll: () => blocks };
  texts.forEach((textContent) => blocks.push({ nodeType: 1, textContent, hidden: false, classList: { contains: () => false }, parentElement: editor }));
  env.getSelection = () => ({ rangeCount: 1, isCollapsed: false, getRangeAt: () => ({ commonAncestorContainer: {}, toString: () => selectedText }) });
  const slides = [];
  const actions = env.__EASYOFFICE_CREATE_COPILOT_ACTIONS__({
    document: { getElementById: (id) => (id === "doc-editor" ? editor : null) },
    sheetDataStore: {},
    sheetRowCounts: {},
    slideDeckState: slides,
    getActiveModule: () => "DOC",
    getActiveSheetName: () => "S",
    getActiveSlideIndex: () => 0,
    getSheetController: () => null,
    getWorkspacePanels: () => ({}),
    getPdfEdit: () => null,
    extractEditableDocumentText: () => "",
  });
  return { actions, blocks, slides };
}

test("document and slide context never carry a lone surrogate, however long the text", () => {
  // The 24,000th unit is the high half of a pair.
  const tail = "나".repeat(23_999) + "😀😀";
  const { actions, slides } = makeActions([tail], tail);
  const { context } = actions.extractContext("DOC");
  assert.ok(wellFormed(context.doc.paragraphs[0]));
  assert.ok(wellFormed(context.doc.selectedText));
  assert.equal(Array.from(context.doc.paragraphs[0]).length, 24_000);

  slides.push({ title: "t", lower: "", elements: [{ id: "e", kind: "text", text: "가".repeat(159) + "😀😀" }] });
  const slide = actions.extractContext("SLIDE").context.slide.slides[0];
  assert.ok(wellFormed(slide.elements[0].text));
});

test("a block a selection command works on is sent past 24 KiB so the engine refuses it", () => {
  const long = "a".repeat(40_000);
  const { actions } = makeActions([long, long]);
  const target = { selection: { paragraphIndex: 0, paragraphEnd: 0 } };
  const { context } = actions.extractContext("DOC", { target });
  assert.equal(context.doc.paragraphs[0].length, 24 * 1024 + 1, "over the engine's limit, so the command is refused");
  assert.equal(context.doc.paragraphs[1].length, 24_000, "the other blocks keep the ordinary cut");
});

test("snapshotTarget tells whether the blocks or the slide field a command was asked for are still the same", () => {
  const { actions, blocks, slides } = makeActions(["제목", "첫째", "둘째"]);
  const target = { selection: { paragraphIndex: 1, paragraphEnd: 2 } };
  const first = actions.snapshotTarget("DOC", target);
  assert.equal(first, actions.snapshotTarget("DOC", target));
  blocks[2].textContent = "둘째를 고침";
  assert.notEqual(actions.snapshotTarget("DOC", target), first, "a changed block");
  blocks[2].textContent = "둘째";
  assert.equal(actions.snapshotTarget("DOC", target), first);
  blocks.splice(0, 0, { ...blocks[0], textContent: "맨 앞에 끼어듦" });
  assert.notEqual(actions.snapshotTarget("DOC", target), first, "an insert before the target moved it");
  blocks.length = 2;
  assert.equal(actions.snapshotTarget("DOC", target), null, "the target is gone");
  assert.equal(actions.snapshotTarget("DOC", null), null, "a question has no target");

  slides.push({ title: "발표 제목", lower: "본문" });
  const title = { selection: { slideIndex: 0, slideField: "title" } };
  const body = { selection: { slideIndex: 0, slideField: "body" } };
  const titleSnap = actions.snapshotTarget("SLIDE", title);
  const bodySnap = actions.snapshotTarget("SLIDE", body);
  slides[0].lower = "본문을 고침";
  assert.equal(actions.snapshotTarget("SLIDE", title), titleSnap, "the other field does not matter");
  assert.notEqual(actions.snapshotTarget("SLIDE", body), bodySnap);
  assert.equal(actions.snapshotTarget("SLIDE", { selection: { slideIndex: 5, slideField: "title" } }), null);
});

test("BASE and BOARD context cut every text at a code point", () => {
  const long = "가".repeat(199) + "😀😀";
  const base = { tables: [{ id: "t1", name: long }], activeTableId: "t1" };
  const board = { nodes: [{ id: long, kind: "text", text: long }], connectors: [{ fromId: long, toId: long }] };
  const env = {
    document: {},
    __EASYOFFICE_BASE_MODEL__: {
      getTable: (st) => st.tables[0],
      queryTableView: () => ({ fields: [{ id: "f", name: long, type: "text" }], rows: [{ displays: { f: long } }] }),
    },
    __EASYOFFICE_CREATE_BASE_STUDIO__: () => ({ bind() {}, getState: () => base }),
    __EASYOFFICE_CREATE_BOARD_STUDIO__: () => ({ getState: () => board, analyze: () => ({ totalNodes: 1, connectorCount: 1 }) }),
  };
  loadUiModule(UI("copilot-text.js"), env);
  loadUiModule(UI("copilot-diff.js"), env);
  loadUiModule(UI("copilot-typing.js"), env);
  loadUiModule(UI("copilot-cursor.js"), env);
  loadUiModule(UI("copilot-timeline.js"), env);
  loadUiModule(UI("copilot-preview.js"), env);
  loadUiModule(UI("copilot-perform-slide.js"), env);
  loadUiModule(UI("copilot-perform.js"), env);
  loadUiModule(UI("base-board-host.js"), env);
  env.__EASYOFFICE_BASE_BOARD_HOST__.init({ document: { getElementById: () => null }, env: { addEventListener() {} }, invokeIpc: async () => ({}) });
  const texts = (value) => JSON.stringify(value).match(/"[^"]*"/g).map((q) => JSON.parse(q));
  for (const mod of ["BASE", "BOARD"]) {
    const { context } = env.__EASYOFFICE_BASE_BOARD_HOST__.extractCopilotContext(mod);
    const all = texts(context);
    assert.ok(all.length > 3);
    for (const text of all) assert.ok(wellFormed(text), mod + " " + text.slice(-4));
    assert.ok(all.some((text) => Array.from(text).length === 200), mod + " the long text was cut to 200 code points");
  }
});

function cssRules(file) {
  const css = fs.readFileSync(UI(file), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  return [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(([, selector, body]) => ({ selector: selector.trim(), body }));
}

test("a selection on paper (page, slide stage, PDF card) is the reference app's translucent blue in both themes", () => {
  // `rhwp-studio/src/engine/selection-renderer.ts:94`: the selection overlay colour. The reference
  // checkout is optional (CI `checks` has none), so the pinned value is checked against it only when present.
  const colour = "rgba(51,144,255,0.35)";
  const referencePath = new URL("../third_party/rhwp/rhwp-studio/src/engine/selection-renderer.ts", import.meta.url);
  if (fs.existsSync(referencePath)) {
    assert.ok(fs.readFileSync(referencePath, "utf8").includes(`background:${colour}`), "the reference overlay colour is in the reference source");
  }
  const rules = cssRules("content.css").filter((r) => r.selector.includes("::selection"));
  assert.ok(rules.length >= 1, "content.css paints the paper selection");
  const selectors = rules.flatMap((r) => r.selector.split(",").map((s) => s.trim()));
  for (const paper of ["#doc-editor", ".slide-stage", ".pdf-page-card"]) {
    assert.ok(selectors.some((s) => s.startsWith(paper) && s.endsWith("::selection")), `${paper} has a ::selection rule`);
  }
  for (const rule of rules) {
    assert.match(rule.body, new RegExp("background:\\s*" + colour.replace(/[().]/g, "\\$&").replace(/,/g, ",\\s*")), "the reference colour, translucent");
    assert.doesNotMatch(rule.body, /var\(--selection-fill\)|#315f91/, "not the opaque dark navy");
  }
  // No theme-specific override re-colours it: the paper is white in both themes.
  const dark = cssRules("content.css").filter((r) => r.selector.includes("data-theme-effective") && r.selector.includes("::selection"));
  assert.deepEqual(dark, []);
});
