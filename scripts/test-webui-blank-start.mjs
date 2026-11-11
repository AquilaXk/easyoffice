import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { fileURLToPath } from "node:url";
import { switchModule } from "./test-support/modules.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { readText } from "./test-support/read-text.mjs";

// A new window starts empty, like the reference app's new window: no sample report, table,
// slides, names or results (PD-176).

const byId = (document, id) => document.getElementById(id);

test("a new spreadsheet is one empty sheet with A1 selected", async () => {
  const { document } = await createWorkspaceHarness();
  switchModule(document, "SHEET");
  await flushAsyncTurns();
  const filled = [...document.querySelectorAll("#pane-sheet td[data-cell]")].filter((td) => td.textContent.trim() !== "");
  assert.deepEqual(filled.map((td) => td.dataset.cell), []);
  assert.equal(document.querySelectorAll("#pane-sheet td[data-formula]").length, 0);
  const tabs = [...document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]")];
  assert.deepEqual(tabs.map((b) => [b.dataset.sheetTab, b.textContent]), [["Sheet1", "시트 1"]]);
  assert.equal(byId(document, "active-cell-ref").textContent, "A1");
  assert.equal(byId(document, "formula-input").value, "");
  assert.equal(byId(document, "sheet-eval-result").textContent, "");
  assert.equal(byId(document, "status-sheet-agg").textContent.includes("1,300"), false);
});

test("a new document is one empty paragraph named 제목 없음", async () => {
  const { document } = await createWorkspaceHarness();
  const editor = byId(document, "doc-editor");
  const body = [...editor.children].filter((el) => el.tagName === "P" && !el.hidden);
  assert.equal(body.length, 1);
  assert.equal(body[0].textContent.trim(), "");
  assert.equal(editor.querySelectorAll("h1, h2, table").length, 0);
  assert.match(byId(document, "status-doc-filename").textContent, /^제목 없음/);
});

test("a new presentation is one empty slide", async () => {
  const { document } = await createWorkspaceHarness();
  switchModule(document, "SLIDE");
  await flushAsyncTurns();
  assert.equal(document.querySelectorAll("#slide-thumb-list .slide-thumb").length, 1);
  assert.equal(byId(document, "slide-title-zone").textContent.trim(), "");
  assert.equal(byId(document, "slide-lower-zone").textContent.trim(), "");
  assert.equal(byId(document, "slide-presenter-notes").value, "");
  assert.match(byId(document, "slide-meta-status").textContent, /슬라이드 1 \/ 1/);
});

test("the PDF screen without a file shows only the home screen's empty hint, and a file brings the card back", async () => {
  const { document } = await createWorkspaceHarness();
  switchModule(document, "PDF");
  await flushAsyncTurns();
  const pane = byId(document, "pane-pdf");
  assert.equal(pane.dataset.empty, "true");
  const hint = byId(document, "pdf-empty-hint");
  assert.ok(hint.classList.contains("home-drop-hint"));
  assert.match(hint.textContent, /열린 PDF가 없습니다/);
  assert.match(hint.textContent, /끌어다 놓으세요/);
  for (const id of ["pdf-form-applicant", "pdf-form-dept", "pdf-watermark-input"]) {
    assert.equal(byId(document, id).value, "", id);
  }
  assert.equal(byId(document, "pdf-stamp-badge").textContent, "");
  assert.equal(byId(document, "pdf-page-card").hidden, true);
  const controls = ["btn-pdf-rotate", "pdf-stamp-select", "pdf-watermark-input", "btn-pdf-apply"];
  for (const id of controls) assert.equal(byId(document, id).disabled, true, id + " is disabled without a file");
  const css = readText(fileURLToPath(new URL("../crates/easyoffice-app/ui/styles.css", import.meta.url)));
  assert.match(css, /\.pdf-page-card\[hidden\][^{]*\{\s*display:\s*none/);
  assert.match(css, /#pdf-empty-hint\s*\{[^}]*margin:\s*auto/, "the hint sits in the middle of the work area");
  assert.match(css, /#pane-pdf\[data-empty="true"\] \.pdf-page-card[^{]*\{\s*display:\s*none/);
  assert.match(css, /#pane-pdf\[data-empty="false"\] #pdf-empty-hint[^{]*\{\s*display:\s*none/);

  await pickFile(document, makeFixtureFile("계약서.pdf", { title: "", blocks: [] }));
  assert.equal(pane.dataset.empty, "false");
  assert.equal(document.querySelector("#pdf-page-card h3").textContent, "계약서.pdf");
  assert.equal(byId(document, "pdf-page-card").hidden, false);
  for (const id of controls) assert.equal(byId(document, id).disabled, false, id + " works with a file");
});
