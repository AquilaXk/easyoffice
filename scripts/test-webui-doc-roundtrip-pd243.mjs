import test from "node:test";
import assert from "node:assert/strict";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { createRunHarness, rangeOf, textNodeOf } from "./test-support/doc-run-harness.mjs";

// PD-243: Multi-level list (`listLevel`, `listNumber`, Tab/Shift+Tab), multi-column (`columns`, `docColumns`),
// first/even header & footer sheet rendering + save fidelity, and UTF-8 table cell links in the WebUI.

test("opening a rich DOC preserves nested list levels, custom numbering, columns, first/even headers, and Korean UTF-8 table cell links through save", async () => {
  const cellText = "문서 안내 바로가기";
  const linkBytes = new TextEncoder().encode("바로가기").length;
  const prefixBytes = new TextEncoder().encode("문서 안내 ").length;

  const { sandbox } = await createWebUiDom();
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
    fileName: "guide.docx",
    format: "docx",
    module: "DOC",
    kind: "document",
    title: "가이드",
    sourceBytes: 512,
    specification: "OOXML",
    itemCount: 4,
    warnings: [],
    firstPageDifferent: true,
    evenOddDifferent: true,
    header: [{ type: "paragraph", runs: [{ text: "홀수 머리글" }] }],
    footer: [{ type: "paragraph", runs: [{ text: "기본 꼬리말" }] }],
    firstHeader: [{ type: "paragraph", runs: [{ text: "표지 머리글" }] }],
    evenHeader: [{ type: "paragraph", runs: [{ text: "짝수 머리글" }] }],
    blocks: [
      {
        type: "paragraph",
        list: "ordered",
        listNumber: "upperRoman:rparen",
        columns: { count: 2, equal: true, gapTwips: 720 },
        runs: [{ text: "첫째 대분류" }],
      },
      {
        type: "paragraph",
        list: "ordered",
        listLevel: 2,
        listNumber: "lowerLatin:paren",
        columns: { count: 2, equal: true, gapTwips: 720 },
        runs: [{ text: "하위 번호" }],
      },
      {
        type: "paragraph",
        list: "bullet",
        listLevel: 1,
        columns: { count: 2, equal: true, gapTwips: 720 },
        runs: [{ text: "하위 글머리" }],
      },
      {
        type: "table",
        rows: [[{ text: "항목" }, { text: cellText }]],
        links: [{ row: 0, col: 1, start: prefixBytes, end: prefixBytes + linkBytes, url: "https://example.org/ko" }],
      },
    ],
  };

  const testIpc = async (cmd, rawPayload) => {
    const body = typeof rawPayload === "string" && rawPayload.trimStart().startsWith("{") ? JSON.parse(rawPayload) : null;
    if (cmd === "convert_document_format" && body?.action === "decode") {
      return decodedResponse;
    }
    return offline[cmd](rawPayload);
  };

  const h = await createWorkspaceHarness({ search: "?module=DOC&win=w-pd243-open", testIpc });
  await flushAsyncTurns(6);

  const openRes = await h.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("guide.docx", new Uint8Array([80, 75, 3, 4]));
  assert.equal(openRes.ok, true);
  await flushAsyncTurns(6);

  const editor = h.document.getElementById("doc-editor");
  assert.equal(editor.style.columnCount, "2", "uniform 2-column document applies columnCount to editor");
  assert.equal(editor.dataset.eoFirstPage, "true");
  assert.equal(editor.dataset.eoEvenOdd, "true");

  // Verify table cell link rendered as <a data-eo-link="1"> inside <td>
  const tdLink = editor.querySelector("table.doc-inline-table td a[data-eo-link='1']");
  assert.ok(tdLink, "table cell hyperlink rendered as anchor");
  assert.equal(tdLink.textContent, "바로가기");
  assert.equal(tdLink.getAttribute("href"), "https://example.org/ko");

  // Verify 3-sheet page frame painting: sheet 1 -> firstHeader, sheet 2 -> evenHeader, sheet 3 -> odd header
  let layer = editor.querySelector(":scope > footer.doc-page-layer");
  if (!layer) {
    layer = h.document.createElement("footer");
    layer.className = "doc-page-layer";
    editor.appendChild(layer);
  }
  layer.innerHTML = "";
  for (let i = 0; i < 3; i += 1) {
    layer.appendChild(Object.assign(h.document.createElement("i"), { className: "doc-page-sheet" }));
  }
  const pageFrame = h.host.__EASYOFFICE_DOC_PAGE_FRAME__;
  pageFrame.paint();
  const sheets = layer.querySelectorAll(".doc-page-sheet");
  assert.equal(sheets[0].querySelector('.doc-page-hf[data-hf="header"]').textContent, "표지 머리글");
  assert.equal(sheets[1].querySelector('.doc-page-hf[data-hf="header"]').textContent, "짝수 머리글");
  assert.equal(sheets[2].querySelector('.doc-page-hf[data-hf="header"]').textContent, "홀수 머리글");
  assert.equal(pageFrame.printCopy("header", 1, 3).textContent, "표지 머리글");
  assert.equal(pageFrame.printCopy("header", 2, 3).textContent, "짝수 머리글");
  assert.equal(pageFrame.printCopy("header", 3, 3).textContent, "홀수 머리글");

  // Verify structured save payload preserves all fields losslessly
  const savePayloads = h.host.__EASYOFFICE_SAVE_PAYLOAD__;
  const saveOpts = {
    firstPageDifferent: editor.dataset.eoFirstPage === "true",
    evenOddDifferent: editor.dataset.eoEvenOdd === "true",
  };
  const blocks = savePayloads.collectDocBlocks(editor, saveOpts);
  const frame = savePayloads.collectPageFrame(editor, saveOpts);

  assert.equal(blocks[0].list, "ordered");
  assert.equal(blocks[0].listNumber, "upperRoman:rparen");
  assert.equal(blocks[0].listLevel, undefined);
  assert.deepEqual(blocks[0].columns, { count: 2, equal: true, gapTwips: 720 });

  assert.equal(blocks[1].listLevel, 2);
  assert.equal(blocks[1].listNumber, "lowerLatin:paren");

  assert.equal(blocks[2].list, "bullet");
  assert.equal(blocks[2].listLevel, 1);

  assert.deepEqual(blocks[3].links, [
    { row: 0, col: 1, start: prefixBytes, end: prefixBytes + linkBytes, url: "https://example.org/ko" },
  ]);

  assert.equal(frame.firstPageDifferent, true);
  assert.equal(frame.evenOddDifferent, true);
  assert.equal(frame.firstHeader[0].runs[0].text, "표지 머리글");
  assert.equal(frame.evenHeader[0].runs[0].text, "짝수 머리글");
  assert.equal(frame.header[0].runs[0].text, "홀수 머리글");
});

test("Tab and Shift+Tab inside list items shift listLevel within 0..8 and undo cleanly", async () => {
  const h = await createRunHarness([
    { tag: "ul", children: [{ tag: "li", children: ["첫 항목"] }, { tag: "li", children: ["둘째 항목"] }] },
  ]);
  const secondLi = h.editor.querySelectorAll("li")[1];
  h.selection.addRange(rangeOf(textNodeOf(secondLi), 0, textNodeOf(secondLi), 0));
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(6);

  const notPrevented = h.editor.dispatchEvent({
    type: "keydown",
    key: "Tab",
    shiftKey: false,
  });
  await flushAsyncTurns(4);
  assert.equal(notPrevented, false, "Tab inside <li> prevents default focus traversal");
  assert.equal(secondLi.dataset.eoListLevel, "1");

  const blocksAfterTab = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor);
  assert.equal(blocksAfterTab[0].listLevel, undefined);
  assert.equal(blocksAfterTab[1].listLevel, 1);

  // Shift+Tab brings it back to level 0
  h.editor.dispatchEvent({
    type: "keydown",
    key: "Tab",
    shiftKey: true,
  });
  await flushAsyncTurns(4);
  assert.equal(secondLi.dataset.eoListLevel, undefined);

  // Undo restores level 1
  h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(h.editor.querySelectorAll("li")[1].dataset.eoListLevel, "1");
});
