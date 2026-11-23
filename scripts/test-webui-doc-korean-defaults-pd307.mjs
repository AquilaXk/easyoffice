import test from "node:test";
import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { readText } from "./test-support/read-text.mjs";
import { studioItem } from "./test-support/modules.mjs";
import { createRunHarness, rangeOf, p, textNodeOf, flatRuns } from "./test-support/doc-run-harness.mjs";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const CONTENT_CSS = readText(path.join(REPO, "crates/easyoffice-app/ui/content.css"));
const STYLES_CSS = readText(path.join(REPO, "crates/easyoffice-app/ui/styles.css"));

test("document editor CSS matches Korean word processor defaults and 602px ruler track", () => {
  const editorRule = /#doc-editor\s*\{([^}]*padding:\s*113\.4px 96px 96px[^}]*)\}/.exec(CONTENT_CSS)?.[1] || "";
  assert.ok(editorRule, "expected #doc-editor layout block in content.css");
  assert.match(editorRule, /font-family:\s*'맑은 고딕'/);
  assert.match(editorRule, /font-size:\s*10pt;/);
  assert.match(editorRule, /line-height:\s*1\.08;/);
  assert.match(editorRule, /text-align:\s*justify;/);

  const paraRule = /#doc-editor > p:not\(\.manuscript-notice\)\s*\{([^}]*)\}/.exec(CONTENT_CSS)?.[1] || "";
  assert.match(paraRule, /margin:\s*0 0 8pt;/);

  assert.match(CONTENT_CSS, /white-space:\s*pre-wrap;\s*\n\s*tab-size:\s*53\.33px;/);
  assert.match(STYLES_CSS, /\.ruler-track\s*\{[^}]*width:\s*602px;\s*flex:\s*0 0 602px;/);
});

test("initial DOC toolbar and 편집 용지 dialog open with Korean word processor defaults", async () => {
  const h = await createWorkspaceHarness({ search: "?module=DOC&win=w-pd307-init" });
  await flushAsyncTurns(6);
  const { document } = h;

  const fontSelect = document.getElementById("doc-font-select");
  const fonts = Array.from(fontSelect.querySelectorAll("option")).map((o) => o.value);
  assert.equal(fonts[0], "맑은 고딕");
  assert.equal(fonts.includes("휴먼명조"), false, "restricted font is removed");
  assert.equal(fontSelect.value, "맑은 고딕");
  assert.equal(document.getElementById("doc-style-font-badge").textContent, "맑은 고딕");
  assert.equal(document.getElementById("doc-font-size-input").value, "10.0");
  assert.equal(document.getElementById("doc-style-size-badge").textContent, "10.0 pt");

  const lsSelect = document.getElementById("doc-linespacing-select");
  assert.equal(lsSelect.value, "108");
  assert.deepEqual(
    Array.from(lsSelect.querySelectorAll("option")).map((o) => o.value),
    ["100", "108", "115", "130", "160", "180", "200", "300"]
  );
  assert.equal(document.getElementById("btn-align-justify").classList.contains("active"), true);

  h.userClick(studioItem(document, "layout-paper"));
  await flushAsyncTurns(4);
  const field = (id) => document.getElementById("doc-ps-" + id).value;
  assert.deepEqual(
    {
      paper: field("paper"),
      width: field("width"),
      height: field("height"),
      left: field("left"),
      right: field("right"),
      top: field("top"),
      header: field("header"),
      bottom: field("bottom"),
      footer: field("footer"),
    },
    {
      paper: "A4",
      width: "210.0",
      height: "297.0",
      left: "25.4",
      right: "25.4",
      top: "30.0",
      header: "15.0",
      bottom: "25.4",
      footer: "17.5",
    }
  );
});

test("Tab in body paragraph inserts a tab character, respects pending formatting, and undoes/redoes cleanly", async () => {
  const h = await createRunHarness([p("앞뒤"), p("가나다라")]);
  const [first] = h.blocks;

  // Collapsed caret inserts a literal tab character at the caret.
  h.selection.addRange(rangeOf(textNodeOf(first), 1, textNodeOf(first), 1));
  const notPrevented = h.editor.dispatchEvent({ type: "keydown", key: "Tab", target: h.editor });
  await flushAsyncTurns(8);
  assert.equal(notPrevented, false);
  assert.equal(first.textContent, "앞\t뒤");

  // Pending bold formatting on a collapsed caret wraps the inserted tab.
  h.selection.addRange(rangeOf(textNodeOf(first), 2, textNodeOf(first), 2));
  h.userClick(h.document.getElementById("btn-stylebar-bold"));
  await flushAsyncTurns(8);
  h.editor.dispatchEvent({ type: "keydown", key: "Tab", target: h.editor });
  await flushAsyncTurns(8);
  assert.deepEqual(flatRuns(first), [
    { text: "앞\t" },
    { text: "\t", eoBold: "true" },
    { text: "뒤" },
  ]);

  // Undo removes the second tab, redo restores it (re-read #doc-editor children after snapshot restore).
  h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(40);
  assert.equal(h.editor.children[0].textContent, "앞\t뒤");
  h.sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(40);
  assert.equal(h.editor.children[0].textContent, "앞\t\t뒤");

  // Non-collapsed selection deletes selected text before inserting tab.
  const para2 = h.editor.children[1];
  const t2 = textNodeOf(para2);
  const selRange = rangeOf(t2, 1, t2, 3);
  selRange.deleteContents = () => {
    t2.data = t2.data.slice(0, 1) + t2.data.slice(3);
    selRange.startOffset = 1;
    selRange.endOffset = 1;
  };
  h.selection.addRange(selRange);
  h.editor.dispatchEvent({ type: "keydown", key: "Tab", target: h.editor });
  await flushAsyncTurns(8);
  assert.equal(para2.textContent, "가\t라");
});

test("Tab keeps list indentation and avoids hijacking table cells or header/footer edit mode", async () => {
  const h = await createRunHarness([
    { tag: "ul", children: [{ tag: "li", children: ["목록"] }] },
    { tag: "table", children: [{ tag: "tbody", children: [{ tag: "tr", children: [{ tag: "td", children: ["표칸"] }, { tag: "td", children: ["다음칸"] }] }] }] },
    p("머리글 테스트"),
  ]);
  const [ul, table, para] = h.blocks;
  const li = ul.querySelector("li");
  const [td, tdNext] = table.querySelectorAll("td");

  // List item indentation handles Tab instead of inserting literal tab text.
  h.selection.addRange(rangeOf(textNodeOf(li), 0, textNodeOf(li), 1));
  const liNotPrevented = h.editor.dispatchEvent({ type: "keydown", key: "Tab", target: li });
  await flushAsyncTurns(8);
  assert.equal(liNotPrevented, false);
  assert.equal(li.dataset.eoListLevel, "1");
  assert.equal(li.textContent, "목록");

  // Table cell delegates Tab to table cell navigation (doc-table-tab.js) rather than inserting `\t`.
  h.selection.addRange(rangeOf(textNodeOf(td), 0, textNodeOf(td), 0));
  const tdNotPrevented = h.editor.dispatchEvent({ type: "keydown", key: "Tab", target: td });
  await flushAsyncTurns(8);
  assert.equal(tdNotPrevented, false);
  assert.equal(td.textContent, "표칸");
  assert.equal(h.selection.getRangeAt(0).startContainer === tdNext, true);
  assert.equal(h.selection.getRangeAt(0).startOffset, 0);

  // Header/footer edit mode skips body Tab handler.
  h.editor.dataset.eoHfEdit = "header";
  h.selection.addRange(rangeOf(textNodeOf(para), 1, textNodeOf(para), 1));
  const hfNotPrevented = h.editor.dispatchEvent({ type: "keydown", key: "Tab", target: para });
  await flushAsyncTurns(8);
  assert.equal(hfNotPrevented, true);
  assert.equal(para.textContent, "머리글 테스트");
});
