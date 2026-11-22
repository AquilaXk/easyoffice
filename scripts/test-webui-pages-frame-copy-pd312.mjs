import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

test("PD-312: multi-section Pages frameCopy string survives untouched saves and clears on user edit, while boolean true stays compatible", async () => {
  const { sandbox } = await (await import("./webui-dom-harness.mjs")).createWebUiDom();
  const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "DOC",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });

  const decodedResponse = {
    status: "OK",
    action: "decode",
    fileName: "multi-section.pages",
    format: "pages",
    module: "DOC",
    kind: "document",
    title: "multi-section",
    sourceBytes: 1,
    specification: "Pages",
    itemCount: 4,
    warnings: [],
    header: [{ type: "paragraph", runs: [{ text: "1장 머리글" }] }],
    blocks: [
      { type: "paragraph", runs: [{ text: "본문 문단" }] },
      { type: "paragraph", heading: 2, frameCopy: "머리글", runs: [{ text: "머리글" }] },
      { type: "paragraph", frameCopy: "2장 다른 머리글", runs: [{ text: "2장 다른 머리글" }] },
      { type: "paragraph", frameCopy: true, runs: [{ text: "1장 머리글" }] },
    ],
  };

  const selection = { focusNode: null, rangeCount: 0 };
  const testIpc = async (cmd, rawPayload) => {
    const body = typeof rawPayload === "string" && rawPayload.trimStart().startsWith("{") ? JSON.parse(rawPayload) : null;
    if (cmd === "convert_document_format" && body?.action === "decode") return decodedResponse;
    return offline[cmd](rawPayload);
  };
  const h = await createWorkspaceHarness({ search: "?module=DOC&win=w-pd312-copy", testIpc, selection });
  await flushAsyncTurns(6);
  const res = await h.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("multi-section.pages", new Uint8Array([80, 75, 3, 4]));
  assert.equal(res.ok, true);
  await flushAsyncTurns(6);

  const editor = h.document.getElementById("doc-editor");
  const initialBlocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor, {});
  assert.deepEqual(
    initialBlocks.map((b) => [b.runs.map((r) => r.text).join(""), b.frameCopy ?? false]),
    [
      ["본문 문단", false],
      ["머리글", "머리글"],
      ["2장 다른 머리글", "2장 다른 머리글"],
      ["1장 머리글", true],
    ]
  );

  const copies = [...editor.querySelectorAll("[data-eo-frame-copy]")];
  assert.equal(copies.length, 3);
  const sec2Copy = copies[1];
  assert.equal(sec2Copy.textContent, "2장 다른 머리글");
  sec2Copy.textContent = "사용자가 고친 글";
  selection.focusNode = sec2Copy.firstChild;
  editor.dispatchEvent({ type: "input", target: editor, inputType: "insertText" });
  await flushAsyncTurns(3);

  const editedBlocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor, {});
  assert.deepEqual(
    editedBlocks.map((b) => [b.runs.map((r) => r.text).join(""), b.frameCopy ?? false]),
    [
      ["본문 문단", false],
      ["머리글", "머리글"],
      ["사용자가 고친 글", false],
      ["1장 머리글", true],
    ]
  );
});
