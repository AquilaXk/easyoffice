import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

// PD-098 step 3b: the ribbon, the quick-access toolbar and the side panels are gone; the smart
// assistant, PII scrub, security audit, benchmark and outline live in dialogs under the menus.

const REMOVED_IDS = [
  "ribbon-tabs",
  "ribbon-tool-strip",
  "contextual-tool-badge",
  "btn-ribbon-style",
  "btn-chrome-toggle",
  "qat-save",
  "qat-undo",
  "qat-redo",
  "panel-left",
  "panel-right",
];

const TOOL_DIALOG_MENU = [
  ["smart-diagnose", "smart-dialog", "btn-smart-diagnose"],
  ["scrub-pii", "pii-dialog", "btn-scrub-pii"],
  ["security-audit", "security-dialog", "btn-run-security-audit"],
  ["bench-run", "bench-dialog", "btn-run-benchmark"],
  ["view-outline", "outline-dialog", null],
];

test("the ribbon, quick-access toolbar and side panels no longer exist in the document", async () => {
  const { document } = await createWorkspaceHarness();
  for (const id of REMOVED_IDS) assert.equal(document.getElementById(id), null, id);
  assert.equal(document.querySelector(".ribbon-btn"), null);
  assert.equal(document.querySelector(".qat-btn"), null);
  assert.equal(document.querySelector(".workspace-grid > aside"), null, "no side panel is left");
  for (const id of ["smart-stats", "pii-result-text", "security-status-text", "bench-result-box", "outline-list"]) {
    assert.equal(document.getElementById(id).closest("dialog") !== null, true, `${id} lives in a dialog`);
  }
  assert.equal(document.querySelector('.md-item[data-studio-menu-cmd="view-ribbon"]'), null, "no ribbon collapse item");
});

test("every tool dialog opens from its menu item, reuses the dialog chrome and closes with its buttons", async () => {
  const { document, userClick } = await createWorkspaceHarness();
  for (const [cmd, dialogId] of TOOL_DIALOG_MENU) {
    const dialog = document.getElementById(dialogId);
    assert.ok(dialog, dialogId);
    assert.ok(dialog.querySelector(".dialog-wrap .dialog-title"), `${dialogId} title`);
    assert.ok(dialog.querySelector(".dialog-body"), `${dialogId} body`);
    assert.ok(dialog.querySelector(".dialog-footer"), `${dialogId} footer`);
    assert.equal(dialog.open, false);
    const item = studioItem(document, cmd);
    assert.ok(item, `${cmd} has a menu item`);
    userClick(item);
    assert.equal(dialog.open, true, `${cmd} opens ${dialogId}`);
    userClick(dialog.querySelector(".dialog-footer [data-dialog-close]"));
    assert.equal(dialog.open, false, `${dialogId} closes`);
  }
});

test("menu items run the same actions the side panels ran", async () => {
  const { document, userClick } = await createWorkspaceHarness();
  userClick(studioItem(document, "security-audit"));
  await flushAsyncTurns(12);
  assert.notEqual(document.getElementById("security-status-text").textContent, "압축 컨테이너 및 문서 구조 보호 활성");
  userClick(studioItem(document, "bench-run"));
  await flushAsyncTurns(30);
  assert.notEqual(document.getElementById("bench-result-box").textContent, "[성능 진단] 버튼을 눌러 측정하세요.");
  userClick(studioItem(document, "smart-diagnose"));
  await flushAsyncTurns(12);
  assert.notEqual(document.getElementById("smart-summary-output").textContent, "요약 대기 중");
});

test("the dialog buttons keep working after the dialog is opened", async () => {
  const { document, userClick } = await createWorkspaceHarness();
  userClick(studioItem(document, "scrub-pii"));
  await flushAsyncTurns(12);
  assert.equal(document.getElementById("pii-dialog").open, true);
  userClick(document.getElementById("btn-scrub-pii"));
  await flushAsyncTurns(12);
  assert.ok(document.getElementById("pii-result-text").textContent.length > 0);
});

test("the outline dialog lists the document headings", async () => {
  const { document, userClick } = await createWorkspaceHarness({ transformHtml: demoHtml });
  userClick(studioItem(document, "view-outline"));
  assert.equal(document.getElementById("outline-dialog").open, true);
  assert.ok(document.getElementById("outline-list").querySelectorAll("li").length > 0);
});
