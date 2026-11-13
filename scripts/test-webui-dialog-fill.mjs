import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem, switchModule } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

// Opening a page setup dialog fills every field with the current value, with the combo boxes' choice lists in place.

const DOC_FIELDS = ["paper", "width", "height", "top", "bottom", "left", "right", "header", "footer", "gutter"];
const SHEET_FIELDS = ["paper", "percent", "top", "bottom", "left", "right", "header", "footer"];

test("the 편집 용지 dialog opens with the paper, size, margins and binding filled", async () => {
  const h = await createWorkspaceHarness({ search: "?module=DOC&win=w-fill-doc" });
  await flushAsyncTurns(6);
  h.document.getElementById("doc-editor").innerHTML = "<p>첫 문단</p>";
  h.host.__EASYOFFICE_DOC_PAGES__.layoutNow();
  h.userClick(studioItem(h.document, "layout-paper"));
  await flushAsyncTurns();
  assert.equal(h.document.getElementById("doc-page-setup-dialog").open, true);
  for (const id of DOC_FIELDS) assert.notEqual(h.document.getElementById("doc-ps-" + id).value, "", `${id} is filled`);
  assert.equal(["portrait", "landscape"].filter((id) => h.document.getElementById("doc-ps-" + id).checked).length, 1, "one orientation");
  assert.equal(["single", "duplex", "top-binding"].filter((id) => h.document.getElementById("doc-ps-" + id).checked).length, 1, "one binding");
  const sizes = [...h.document.getElementById("doc-ps-paper").parentElement.querySelectorAll(".custom-select-sizer > span")].map((s) => s.textContent);
  assert.deepEqual(sizes, ["A4", "A3", "B4", "B5", "Letter", "Legal", "사용자 정의"]);
});

test("the 쪽 설정 dialog opens with the paper, margins and one scale choice filled", async () => {
  const h = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "evaluate_sheet") {
        const body = JSON.parse(args.payload);
        return { open: { session: "s1" }, apply: { session: "s1", changed: [], circular: false }, close: { closed: true } }[body.sessionAction] ?? {};
      }
      return { status: "ok" };
    },
  });
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  h.userClick(h.document.querySelector('.md-item[data-sheet-act="page-setup"]'));
  await flushAsyncTurns(20);
  assert.equal(h.document.getElementById("sheet-page-dialog").open, true);
  for (const id of SHEET_FIELDS) assert.notEqual(h.document.getElementById("sp-" + id).value, "", `${id} is filled`);
  assert.equal(["scale-percent", "scale-fit"].filter((id) => h.document.getElementById("sp-" + id).checked).length, 1, "one scale choice");
  assert.equal(h.document.getElementById("sp-paper").parentElement.querySelectorAll(".custom-select-sizer > span").length, h.document.getElementById("sp-paper").querySelectorAll("option").length);
});
