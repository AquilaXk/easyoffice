import assert from "node:assert/strict";
import path from "node:path";
import { acceptChange, askCopilot, finishPlay, undoLast } from "./copilot.mjs";
import { REPO } from "./support.mjs";

// The embedded Hangul editor (the real hwp-studio bundle, a real HWP document) under the Copilot's two editor-side features, in a real
// WebKit: 아래에 삽입 (PD-386, `doc.insert_below` -> `insert_after`) and the pointer's caret lookup (PD-387, `focusTarget` and
// `getCaretRect`). The model's answers are a fixture. The editor's answers are read over its own message port, which a hook installed
// before the page loads keeps (`window.__eoCall`), so what is compared is what the editor reports, not what the shell remembers.
// The document is a sample of the pinned rhwp checkout (`scripts/checkout-rhwp.mjs`); the editor bundle comes from
// `scripts/build-hwp-studio.mjs`.

const SAMPLE = path.join(REPO, "third_party/rhwp/rhwp-studio/public/samples/biz_plan.hwp");
const UNDO_CHIP = "#copilot-thread .eo-copilot-undo-btn";
const READY_MS = 40000;

/** Runs before the page's own scripts: keeps the shell's end of the editor's message port and offers `__eoCall(method, params)` over it. */
function keepEditorPort() {
  const Native = window.MessageChannel;
  window.MessageChannel = class extends Native {
    constructor() {
      super();
      const port = this.port1;
      const post = port.postMessage.bind(port);
      port.postMessage = (message, transfer) => {
        if (message && message.sessionId) window.__eoSession = message.sessionId;
        return post(message, transfer);
      };
      window.__eoPort = port;
    }
  };
  window.__eoCall = (method, params) => new Promise((resolve, reject) => {
    window.__eoNext = (window.__eoNext || 1000000) + 1;
    const id = window.__eoNext;
    const onMessage = ({ data }) => {
      if (data?.type !== "rhwp-response" || data.id !== id) return;
      window.__eoPort.removeEventListener("message", onMessage);
      if (data.error) reject(new Error(data.error.message));
      else resolve(data.result);
    };
    window.__eoPort.addEventListener("message", onMessage);
    window.__eoPort.postMessage({ type: "rhwp-request", version: 1, sessionId: window.__eoSession, id, method, params });
  });
}

const editorCall = (page, method, params) => page.evaluate(([name, args]) => window.__eoCall(name, args), [method, params]);

/** What the editor reports of the document: every paragraph's text and whether it holds a control. */
async function documentOf(page) {
  const read = await editorCall(page, "readParagraphRange", { startIndex: 0, count: 200 });
  assert.ok(read.totalParagraphs <= 200, "the sample fits one read");
  return { total: read.totalParagraphs, paragraphs: read.paragraphs.map((p) => ({ text: p.text, hasControls: p.hasControls })) };
}

/** Opens the sample in the editor through the shell's own file input, with the panel's menu entry of the editor (도구 > 코파일럿) pressed. */
async function openSampleWithPanel(page) {
  await page.addInitScript(keepEditorPort);
  await page.reload();
  await page.waitForSelector("#hwp-frame");
  await page.waitForFunction(() => document.querySelector(".hwp-host-status")?.hidden === true, undefined, { timeout: READY_MS });
  // The page is a managed window, which opens a file in a new window; without `open` the picker opens it in place.
  await page.evaluate(() => { window.open = undefined; });
  await page.setInputFiles("#file-open-input", SAMPLE);
  await page.waitForFunction(() => /열기 완료/.test(document.getElementById("status-command-feedback")?.textContent || ""), undefined, { timeout: READY_MS });
  const studio = page.frameLocator("#hwp-frame");
  await studio.locator('.menu-item[data-menu="tool"] .menu-title').click();
  await studio.locator('[data-cmd="tool:copilot"]').click();
  await page.waitForSelector("#btn-copilot-model");
  return documentOf(page);
}

const plainParagraph = (doc) => doc.paragraphs.findIndex((p) => !p.hasControls && p.text.length > 20);
const textWithControl = (doc) => doc.paragraphs.findIndex((p) => p.hasControls && p.text.trim().length > 0);
const emptyControlAnchor = (doc) => doc.paragraphs.findIndex((p, i) => p.hasControls && p.text === "" && i > 2);

function insertAfter(index, texts) {
  return { kind: "insert_after", index, texts };
}

export const hwpInsertBelowSteps = [
  ["아래에 삽입 keeps the paragraph, adds the new ones right under it, and one undo restores the document", async (page) => {
    const before = await openSampleWithPanel(page);
    const at = plainParagraph(before);
    assert.ok(at >= 0, "the sample has a plain paragraph");
    const answer = [{ op: "doc.replace_paragraphs", replacements: [{ index: at, text: "새로 넣은 첫 줄\n새로 넣은 둘째 줄" }] }];
    await askCopilot(page, "다듬어 주세요", [answer]);
    assert.equal(await page.locator('#copilot-thread button[data-copilot-action="insert-below"]').count(), 1, "the choice is offered");
    await page.click('#copilot-thread button[data-copilot-action="insert-below"]');
    await page.waitForSelector(UNDO_CHIP);
    const after = await documentOf(page);
    assert.equal(after.total, before.total + 2, "one paragraph per line");
    assert.deepEqual(after.paragraphs[at], before.paragraphs[at], "the original paragraph is as it was");
    assert.equal(after.paragraphs[at + 1].text, "새로 넣은 첫 줄");
    assert.equal(after.paragraphs[at + 2].text, "새로 넣은 둘째 줄");
    assert.deepEqual(after.paragraphs.slice(at + 3), before.paragraphs.slice(at + 1), "what followed is pushed down, unchanged");
    await undoLast(page);
    await page.waitForFunction(async (total) => (await window.__eoCall("readParagraphRange", { startIndex: 0, count: 1 })).totalParagraphs === total, before.total);
    const restored = await documentOf(page);
    assert.equal(restored.total, before.total);
    assert.deepEqual(restored.paragraphs, before.paragraphs, "one undo is the document before");
  }],

  ["아래에 삽입 under a paragraph that holds a table or a control leaves that paragraph whole, in one undo step", async (page) => {
    const before = await openSampleWithPanel(page);
    const targets = [
      ["text and a control", textWithControl(before)],
      ["an empty paragraph that anchors a control", emptyControlAnchor(before)],
      ["the last paragraph of the document", before.total - 1],
    ];
    for (const [label, at] of targets) {
      assert.ok(at >= 0 && before.paragraphs[at].hasControls, `the sample has ${label}`);
      await editorCall(page, "applyCopilotEdits", { steps: [insertAfter(at, ["표 아래에 넣은 글"])] });
      const after = await documentOf(page);
      assert.equal(after.total, before.total + 1, `${label}: one paragraph added`);
      assert.deepEqual(after.paragraphs[at], before.paragraphs[at], `${label}: the paragraph keeps its text and its control`);
      assert.deepEqual([after.paragraphs[at + 1].text, after.paragraphs[at + 1].hasControls], ["표 아래에 넣은 글", false], `${label}: the new text is a paragraph of its own`);
      assert.deepEqual(after.paragraphs.slice(at + 2), before.paragraphs.slice(at + 1), `${label}: the rest is as it was`);
      await editorCall(page, "automation.execute", { id: "edit:undo" });
      const restored = await documentOf(page);
      assert.deepEqual(restored.paragraphs, before.paragraphs, `${label}: one undo restores the paragraphs`);
    }
  }],

  ["a text appended after a last paragraph that holds a control leaves the control where it was", async (page) => {
    const before = await openSampleWithPanel(page);
    const last = before.total - 1;
    assert.ok(before.paragraphs[last].hasControls, "the sample ends with a control");
    await editorCall(page, "applyCopilotEdits", { steps: [{ kind: "append", texts: ["끝에 붙인 글"] }] });
    const after = await documentOf(page);
    assert.equal(after.total, before.total + 1);
    assert.deepEqual(after.paragraphs[last], before.paragraphs[last]);
    assert.equal(after.paragraphs[last + 1].text, "끝에 붙인 글");
    assert.equal(after.paragraphs[last + 1].hasControls, false);
  }],
];

/** The caret box after `focusTarget`, asked at once like the shell does and again once the editor has settled. */
async function caretAfterFocus(page, entry) {
  return page.evaluate(async (paragraph) => {
    const target = { kind: "body_paragraph", section: paragraph.section, paragraph: paragraph.paragraph, charOffset: 0, length: Array.from(paragraph.text).length };
    const focus = await window.__eoCall("focusTarget", { target });
    const immediate = await window.__eoCall("getCaretRect", {});
    await new Promise((resolve) => setTimeout(resolve, 400));
    return { focus, immediate, settled: await window.__eoCall("getCaretRect", {}) };
  }, entry);
}

const near = (a, b, slack, label) => assert.ok(Math.abs(a - b) <= slack, `${label}: ${a} is not within ${slack} of ${b}`);

export const hwpPointerSteps = [
  ["focusTarget takes the editor to the paragraph at once: the caret box is the target's, the same right away and once settled", async (page) => {
    await openSampleWithPanel(page);
    const read = await editorCall(page, "readParagraphRange", { startIndex: 0, count: 200 });
    // Paragraphs that end at different places, so the caret box can only match when it really moved.
    const picks = read.paragraphs.filter((p) => !p.hasControls && p.text.length > 0);
    const spread = [picks[0], picks[Math.floor(picks.length / 2)], picks[picks.length - 1], picks[Math.floor(picks.length / 4)]];
    const seen = [];
    for (const entry of spread) {
      const { focus, immediate, settled } = await caretAfterFocus(page, entry);
      assert.equal(focus.focused, true, "the editor took the jump");
      assert.equal(immediate.visible, true);
      assert.deepEqual(immediate, settled, "the box is already where it settles");
      const context = await editorCall(page, "readParagraphRange", { startIndex: entry.index, count: 1 });
      assert.deepEqual([context.selection.startParagraphIndex, context.selection.endParagraphIndex, context.selection.hasSelection], [entry.index, entry.index, true], "the editor's selection is the target paragraph");
      seen.push(immediate);
    }
    for (let i = 1; i < seen.length; i += 1) {
      assert.ok(Math.abs(seen[i].left - seen[i - 1].left) > 1 || Math.abs(seen[i].top - seen[i - 1].top) > 1, "the box moved with the target (not the previous place)");
    }
  }],

  ["the pointer's selection box lands on the caret box of the paragraph the Copilot is editing", async (page) => {
    const doc = await openSampleWithPanel(page);
    const at = plainParagraph(doc);
    await askCopilot(page, "다듬어 주세요", [[{ op: "doc.replace_paragraphs", replacements: [{ index: at, text: "포인터가 찾아간 문단" }] }]]);
    await acceptChange(page);
    const box = await page.waitForFunction(() => {
      const node = document.querySelector("#eo-copilot-cursor-layer .eo-copilot-selection");
      return node ? node.getBoundingClientRect().toJSON() : null;
    }, undefined, { timeout: 20000 }).then((handle) => handle.jsonValue());
    const frame = await page.$eval("#hwp-frame", (el) => el.getBoundingClientRect().toJSON());
    const caret = await editorCall(page, "getCaretRect", {});
    assert.equal(caret.visible, true);
    near(box.left, frame.left + caret.left, 3, "box left");
    near(box.top, frame.top + caret.top, 3, "box top");
    near(box.height, caret.height, 3, "box height");
    await finishPlay(page);
    assert.equal((await documentOf(page)).paragraphs[at].text, "포인터가 찾아간 문단");
  }],
];
