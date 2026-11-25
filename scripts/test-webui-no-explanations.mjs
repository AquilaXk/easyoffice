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
  "sheet-cf-status", "sheet-dv-status", "sheet-pivot-status", "sheet-dv-alert-message", "sheet-numfmt-status",
  "sheet-filter-status", "sheet-sort-status", "sheet-tab-delete-message", "sheet-names-status", "sheet-name-edit-status", "sheet-name-paste-status", "sheet-name-delete-message", "sheet-name-create-status",
  "sheet-link-status", "sheet-table-status", "sheet-ts-status", "sheet-dedupe-status", "sheet-slicer-status", "sheet-chart-status",
  "sheet-image-status", "sheet-note-status",
  "slide-layout-status", "slide-shape-status", "slide-table-status", "slide-chart-status", "slide-image-status",
  "doc-stats-status", "doc-mailmerge-status", "doc-shape-status", "doc-chart-status", "doc-fn-status",
  "export-dialog-status",
  "sheet-protect-status", "sheet-protect-alert-message", "sheet-wb-protect-status", "sheet-password-status", "sheet-range-edit-status",
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

test("the status line never shows an internal module id in parentheses", () => {
  const MODULE_ID = /\((?:SHEET|DOC|HWP|SLIDE|PDF)\)/;
  for (const name of ["ribbon-history.js", "workspace-history.js", "sheet-controller.js", "workspace-tools.js", "workspace-panels.js", "app.js"]) {
    const source = UI(name);
    assert.doesNotMatch(source, /(?:실행 취소|다시 실행) 완료 \(/, name + " built the history status from a prefix");
    assert.doesNotMatch(source, /textContent\s*=[^;\n]*getActiveModule\(\)/, name + " writes the module id to a text");
  }
  assert.match(UI("workspace-history.js"), /say\(isUndo \? "실행 취소" : "다시 실행"\)/);
  assert.doesNotMatch(UI("ribbon-history.js"), MODULE_ID);
});

// The Copilot panel and the BASE and BOARD windows carry labels, values and status or error messages, never an instruction (PD-265).

const INSTRUCTION = /(?:하세요|하십시오|해 주세요|눌러|클릭해|입력 후|직접 입력|하려면|예:)/;
const stringLiterals = (source) => [...source.matchAll(/"((?:[^"\\\n]|\\.)*)"|`((?:[^`\\]|\\.)*)`/g)].map((m) => m[1] ?? m[2]);

test("the Copilot panel has no welcome sentence and no placeholder that instructs", () => {
  assert.equal(INDEX_HTML.includes("eo-copilot-welcome-sub"), false, "the welcome sub line explained the panel");
  assert.equal(UI("copilot.css").includes("eo-copilot-welcome-sub"), false);
  const copilotHtml = INDEX_HTML.slice(INDEX_HTML.indexOf('id="eo-copilot-shell"'), INDEX_HTML.indexOf('id="eo-copilot-shell"') + 9000);
  assert.doesNotMatch(copilotHtml, /title="[^"]*(?:드래그|더블클릭)/);
  const dialog = INDEX_HTML.match(/<dialog id="copilot-model-dialog"[\s\S]*?<\/dialog>/)?.[0] ?? "";
  assert.ok(dialog.length > 0);
  for (const [, text] of dialog.matchAll(/placeholder="([^"]*)"/g)) assert.doesNotMatch(text, INSTRUCTION, "model dialog placeholder: " + text);
  for (const [, text] of dialog.matchAll(/<p\b[^>]*>([\s\S]*?)<\/p>/g)) assert.doesNotMatch(text, INSTRUCTION, "model dialog paragraph: " + text.trim());
});

test("the placeholders and messages written by the Copilot, BASE and BOARD scripts do not instruct", () => {
  for (const name of ["copilot-model-ui.js", "copilot-ui.js", "base-studio.js", "board-studio.js", "base-board-host.js"]) {
    const source = UI(name);
    for (const m of source.matchAll(/placeholder\s*=\s*([^;]+);/g)) {
      for (const literal of stringLiterals(m[1])) assert.doesNotMatch(literal, INSTRUCTION, name + " placeholder: " + literal);
    }
    for (const m of source.matchAll(/(?:notify|setFeedback)\(([^;\n]*)\)/g)) {
      for (const literal of stringLiterals(m[1])) assert.doesNotMatch(literal, INSTRUCTION, name + " message: " + literal);
    }
    for (const m of source.matchAll(/el\("p", "base-empty-hint", ("[^"]*")\)/g)) assert.doesNotMatch(m[1], INSTRUCTION, name + " hint: " + m[1]);
  }
  assert.equal(/mkInput\([^)]*"예:/.test(UI("base-studio.js")), false, "the field dialog fields carry no example text");
});
