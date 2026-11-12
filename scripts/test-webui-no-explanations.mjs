import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { readText } from "./test-support/read-text.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, commandItem } from "./test-support/modules.mjs";
import { openDemoWorkbook } from "./test-support/demo-fixtures.mjs";

// QA rule (PD-176, docs/design-system.md): the screen carries function labels and status or error
// messages, never sentences that explain or instruct. These guards keep such lines from coming back.

const UI = (name) => readText(fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url)));
const INDEX_HTML = UI("index.html");

// Paragraphs of the tool dialogs that show a result or a status, by id.
const STATUS_PARAGRAPHS = new Set([
  "smart-stats", "smart-summary-output", "pii-result-text", "security-status-text", "bench-result-box", "pdf-splitmerge-error",
  "sheet-cf-status", "sheet-dv-status", "sheet-pivot-status", "sheet-dv-alert-message",
]);

test("the home screen is the action buttons and the recent documents only", () => {
  const source = UI("home-screen.js");
  for (const gone of ["home-title", "home-subtitle", "home-drop-hint", "home-recent-hint", "home-action-description", "EMPTY_HINT", "FILLED_HINT"]) {
    assert.equal(source.includes(gone), false, gone + " explained the screen");
  }
  assert.doesNotMatch(source, /element\("(?:p|h1)"/);
});

test("a dialog paragraph is a result or a status, never an explanation", () => {
  const dialogs = INDEX_HTML.match(/<dialog [\s\S]*?<\/dialog>/g) ?? [];
  assert.ok(dialogs.length > 5);
  for (const dialog of dialogs) {
    for (const [, attrs, text] of dialog.matchAll(/<p\b([^>]*)>([\s\S]*?)<\/p>/g)) {
      if (attrs.includes("about-notice")) continue;
      const id = /id="([^"]*)"/.exec(attrs)?.[1] ?? "";
      assert.ok(STATUS_PARAGRAPHS.has(id), `a dialog paragraph explains: ${id || text.trim()}`);
    }
  }
  assert.equal(/<p id="pdf-splitmerge-error"[^>]*>\s*<\/p>/.test(INDEX_HTML), true, "the split and merge note starts empty");
});

test("empty panes carry no instruction line", () => {
  assert.equal(INDEX_HTML.includes("pdf-empty-hint"), false);
  assert.doesNotMatch(INDEX_HTML, /<p[^>]*slide-show-hint/);
  assert.doesNotMatch(INDEX_HTML, /placeholder="[^"]*(?:하세요|합니다)"/);
  const styles = UI("styles.css");
  assert.doesNotMatch(styles, /content:\s*"[^"]*(?:하세요|합니다)"/);
});

const INTERNAL_STATUS = /sheet-|\[|컨텍스트/;

test("a ribbon command never puts its id, tab or context on the status lines", async () => {
  const { document, userClick } = await createWorkspaceHarness();
  switchModule(document, "SHEET");
  await openDemoWorkbook(document);
  const feedback = document.getElementById("status-command-feedback");
  const evalResult = document.getElementById("sheet-eval-result");
  const before = feedback.textContent;
  for (const cmd of ["sheet-mod-condfmt", "sheet-mod-pivot"]) {
    userClick(commandItem(document, cmd));
    await flushAsyncTurns();
    assert.equal(feedback.textContent, before, `${cmd} opens a dialog and leaves the status line alone`);
    assert.equal(evalResult.textContent, "", `${cmd} leaves the formula result empty`);
  }
  for (const cmd of ["sheet-mod-freeze", "sheet-data-sort"]) {
    userClick(commandItem(document, cmd));
    await flushAsyncTurns();
    assert.doesNotMatch(feedback.textContent, INTERNAL_STATUS, cmd);
    assert.doesNotMatch(evalResult.textContent, INTERNAL_STATUS, cmd);
  }
});
