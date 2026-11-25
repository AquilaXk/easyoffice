import test from "node:test";
import assert from "node:assert/strict";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";

// The 머리말 and 꼬리말 commands (PD-211): each enters the page's header or footer for editing in place (made on the
// first use, one undo step), like the reference editor. The header and footer are sections of the editor that are
// drawn on every page and saved as parts of their own, never as body paragraphs.

const toolbarButton = (document, cmd) => document.querySelector(`#icon-toolbar [data-studio-cmd="${cmd}"]`);
const section = (editor, kind) => editor.querySelector(`:scope > section[data-eo-hf="${kind}"]`);

async function openDoc(win) {
  const harness = await createWorkspaceHarness({ search: `?module=DOC&win=${win}` });
  await flushAsyncTurns(6);
  const editor = harness.document.getElementById("doc-editor");
  editor.innerHTML = "<p>첫 문단</p><p>둘째 문단</p>";
  harness.host.__EASYOFFICE_DOC_PAGES__.layoutNow();
  return { ...harness, editor };
}

test("the header button makes the header section once and edits it in place; the footer is a section of its own", async () => {
  const { document, editor, host, userClick } = await openDoc("w-hf-buttons");
  userClick(toolbarButton(document, "mod-header"));
  await flushAsyncTurns();
  assert.equal(editor.querySelectorAll(':scope > section[data-eo-hf="header"]').length, 1);
  assert.equal(editor.dataset.eoHfEdit, "header", "the editor is in header edit state");
  assert.equal(editor.getAttribute("contenteditable"), "false", "the body is not editable meanwhile");
  assert.equal(section(editor, "header").getAttribute("contenteditable"), "true");
  assert.equal(document.querySelector("#icon-toolbar .tb-headerfooter-group").hidden, false);
  assert.match(document.querySelector("#icon-toolbar .tb-hf-label").textContent, /머리글/);
  assert.equal(editor.querySelectorAll("p").length, 3, "the header's paragraph is not a body paragraph of the flow");
  assert.equal(document.getElementById("status-command-feedback").textContent, "머리글 편집 중");
});

test("closing keeps a header with text and drops an empty one; the page layer draws the kept one on every sheet", async () => {
  const { document, editor, host, userClick } = await openDoc("w-hf-close");
  const frame = host.__EASYOFFICE_DOC_PAGE_FRAME__;
  userClick(toolbarButton(document, "mod-header"));
  await flushAsyncTurns();
  document.querySelector('#icon-toolbar [data-hf-action="close"]').click();
  await flushAsyncTurns();
  assert.equal(section(editor, "header"), null, "an empty header is not kept");
  assert.equal(editor.getAttribute("contenteditable"), "true");
  userClick(toolbarButton(document, "mod-footer"));
  await flushAsyncTurns();
  section(editor, "footer").querySelector("p").textContent = "꼬리말 글";
  frame.endEdit();
  await flushAsyncTurns();
  assert.equal(frame.has("footer"), true);
  // The page layer is made by the page layout (a real layout in the browser); two sheets stand in for it here.
  const layer = document.createElement("footer");
  layer.className = "doc-page-layer";
  for (let i = 0; i < 2; i += 1) layer.appendChild(Object.assign(document.createElement("i"), { className: "doc-page-sheet" }));
  editor.appendChild(layer);
  frame.paint();
  const sheets = layer.querySelectorAll(".doc-page-sheet");
  assert.equal(sheets.length, 2);
  for (const sheet of sheets) {
    assert.equal(sheet.querySelector('.doc-page-hf[data-hf="footer"]').textContent, "꼬리말 글");
  }
  assert.equal(layer.hasAttribute("data-hf-footer"), true);
  assert.equal(sheets[0].querySelector('.doc-page-hf[data-hf="header"]'), null, "no header, no header copy");
});

test("the menu item runs the same command and one undo takes the header section back out", async () => {
  const { document, editor, host, sandbox, userClick } = await openDoc("w-hf-menu");
  assert.equal(studioItem(document, "mod-footer").querySelector(".md-label").textContent, "바닥글");
  userClick(studioItem(document, "mod-footer"));
  await flushAsyncTurns();
  assert.ok(section(editor, "footer"));
  host.__EASYOFFICE_DOC_PAGE_FRAME__.endEdit();
  await flushAsyncTurns();
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(section(editor, "footer"), null, "a single undo step");
  assert.equal(editor.querySelectorAll("p").length, 2, "the body is back as it was");
});

test("a page-number field is inserted into the section being edited and saves as a field run beside the body", async () => {
  const { document, editor, host, userClick } = await openDoc("w-hf-field");
  userClick(toolbarButton(document, "mod-footer"));
  await flushAsyncTurns();
  section(editor, "footer").querySelector("p").textContent = "쪽 ";
  document.querySelector('#icon-toolbar [data-hf-action="page-number"]').click();
  await flushAsyncTurns();
  assert.equal(section(editor, "footer").querySelectorAll(".eo-page-field").length, 1);
  host.__EASYOFFICE_DOC_PAGE_FRAME__.endEdit();
  const payload = host.__EASYOFFICE_SAVE_PAYLOAD__;
  const blocks = payload.collectDocBlocks(editor);
  assert.deepEqual(blocks.map((b) => b.runs.map((r) => r.text).join("")), ["첫 문단", "둘째 문단"], "the footer is not body text");
  const frame = payload.collectPageFrame(editor);
  assert.deepEqual(frame.footer[0].runs, [{ text: "쪽 " }, { text: "", field: "page" }]);
  assert.equal("header" in frame, false);
});

test("deleting removes the section as one undo step", async () => {
  const { document, editor, host, userClick } = await openDoc("w-hf-delete");
  userClick(toolbarButton(document, "mod-header"));
  await flushAsyncTurns();
  section(editor, "header").querySelector("p").textContent = "지울 글";
  document.querySelector('#icon-toolbar [data-hf-action="delete"]').click();
  await flushAsyncTurns();
  assert.equal(section(editor, "header"), null);
  assert.equal(host.__EASYOFFICE_DOC_PAGE_FRAME__.isEditing(), null);
  assert.equal(document.querySelector("#icon-toolbar .tb-headerfooter-group").hidden, true);
});

test("the command palette lists 머리글 and 바닥글 in the document window and runs them", async () => {
  const { document, editor, host, userClick } = await openDoc("w-hf-palette");
  userClick(document.getElementById("btn-command-palette"));
  const search = document.getElementById("cmd-search-input");
  search.value = "바닥글";
  search.dispatchEvent({ type: "input", target: search });
  await flushAsyncTurns();
  const rows = [...document.querySelectorAll("#cmd-palette-list li")].filter((li) => li.textContent.includes("바닥글"));
  assert.equal(rows.length, 1);
  userClick(rows[0]);
  await flushAsyncTurns();
  assert.ok(section(editor, "footer"));
  assert.equal(editor.dataset.eoHfEdit, "footer");
});

test("a document opened with a header and footer shows them as sections and no old fake block is added", async () => {
  const { editor, host } = await openDoc("w-hf-open");
  const frame = host.__EASYOFFICE_DOC_PAGE_FRAME__;
  editor.innerHTML = '<p>본문</p><section class="eo-page-frame" data-eo-hf="header"><p>머리</p></section>';
  await flushAsyncTurns();
  assert.equal(frame.has("header"), true);
  assert.equal(frame.has("footer"), false);
  assert.equal(editor.querySelector(".eo-hwp-header"), null);
});

test("the footer command is a document command: the other windows have neither the button nor an enabled menu item", async () => {
  const { document } = await openDoc("w-hf-modules");
  for (const mod of ["SHEET", "SLIDE", "PDF"]) {
    switchModule(document, mod);
    await flushAsyncTurns(6);
    assert.equal(studioItem(document, "mod-footer").getAttribute("aria-disabled"), "true", `${mod} menu item`);
    assert.equal(toolbarButton(document, "mod-footer").hidden, true, `${mod} button`);
  }
});

test("the page setup places the areas and sets how far the body keeps away; without one the default margins apply", async () => {
  const { document, editor, host } = await openDoc("w-hf-geometry");
  const frame = host.__EASYOFFICE_DOC_PAGE_FRAME__;
  const layer = document.createElement("footer");
  layer.className = "doc-page-layer";
  layer.appendChild(Object.assign(document.createElement("i"), { className: "doc-page-sheet" }));
  editor.appendChild(layer);
  editor.insertBefore(
    Object.assign(document.createElement("section"), { className: "eo-page-frame" }),
    layer
  );
  const header = editor.querySelector(":scope > section.eo-page-frame");
  header.dataset.eoHf = "header";
  header.innerHTML = "<p>머리</p>";
  frame.paint();
  const get = (name) => editor.style.getPropertyValue(name);
  // Defaults (`DOC_DEFAULT`): header 15 mm from the top, body at 30 mm (851 and 1701 twips, 15 twips to the pixel).
  assert.equal(get("--hf-hd"), "56.73px");
  assert.equal(get("--hf-band-h"), "56.67px");
  assert.equal(get("--pg-hf-top"), "113.4px");
  assert.equal(get("--pg-hf-bottom"), "", "no footer, no footer room");
  editor.dataset.eoPage = "11906,16838,1701,1701,2000,1800,1000,700";
  const footer = document.createElement("section");
  footer.className = "eo-page-frame";
  footer.dataset.eoHf = "footer";
  footer.innerHTML = "<p>꼬리</p>";
  editor.insertBefore(footer, layer);
  frame.paint();
  assert.equal(get("--hf-hd"), "66.67px");
  assert.equal(get("--pg-hf-top"), "133.33px");
  assert.equal(get("--hf-ft"), "46.67px");
  assert.equal(get("--hf-fband-h"), "73.33px");
  assert.equal(get("--pg-hf-bottom"), "120px");
  header.remove();
  footer.remove();
  frame.paint();
  assert.equal(get("--pg-hf-top"), "", "without a header the geometry is the editor's own");
});

test("the total-pages and file-name buttons insert their fields, which save as their own field runs", async () => {
  const { document, editor, host, userClick } = await openDoc("w-hf-fields");
  userClick(toolbarButton(document, "mod-footer"));
  await flushAsyncTurns();
  section(editor, "footer").querySelector("p").textContent = "";
  for (const action of ["page-number", "total-pages", "file-name"]) {
    document.querySelector(`#icon-toolbar [data-hf-action="${action}"]`).click();
    await flushAsyncTurns();
  }
  host.__EASYOFFICE_DOC_PAGE_FRAME__.endEdit();
  const frame = host.__EASYOFFICE_SAVE_PAYLOAD__.collectPageFrame(editor);
  assert.deepEqual(frame.footer[0].runs.map((r) => r.field), ["page", "pages", "filename"]);
  const labels = ["쪽번호", "총쪽수", "파일명"];
  const shown = [...document.querySelectorAll("#icon-toolbar .tb-headerfooter-group .tb-label")].map((el) => el.textContent);
  for (const label of labels) assert.ok(shown.includes(label), label);
});

test("every sheet's footer copy holds exactly the section's fields, one element per field, from page 1 to the last", async () => {
  const { document, editor, host } = await openDoc("w-hf-copies");
  const frame = host.__EASYOFFICE_DOC_PAGE_FRAME__;
  const layer = document.createElement("footer");
  layer.className = "doc-page-layer";
  for (let i = 0; i < 4; i += 1) layer.appendChild(Object.assign(document.createElement("i"), { className: "doc-page-sheet" }));
  editor.appendChild(layer);
  const footer = Object.assign(document.createElement("section"), { className: "eo-page-frame" });
  footer.dataset.eoHf = "footer";
  footer.innerHTML = '<p>- <span class="eo-page-field" data-eo-field="page"></span> -</p>';
  editor.insertBefore(footer, layer);
  const fieldCounts = () => [...layer.querySelectorAll(".doc-page-sheet")].map((sheet) => sheet.querySelectorAll('.doc-page-hf[data-hf="footer"] .eo-page-field').length);
  frame.paint();
  assert.deepEqual(fieldCounts(), [1, 1, 1, 1]);
  footer.querySelector("p").appendChild(Object.assign(document.createElement("span"), { className: "eo-page-field" }));
  frame.paint();
  assert.deepEqual(fieldCounts(), [2, 2, 2, 2], "a field added to the section appears once more on every sheet, not by page");
});

test("PD-249: 3-zone toggle splits header into left/center/right compartments, navigates with Tab, inserts fields into the focused zone, and saves zone alignments", async () => {
  const { document, editor, host, userClick } = await openDoc("w-hf-zones-pd249");
  const frame = host.__EASYOFFICE_DOC_PAGE_FRAME__;
  userClick(toolbarButton(document, "mod-header"));
  await flushAsyncTurns();

  const toggleZonesBtn = document.querySelector('#icon-toolbar [data-hf-action="toggle-zones"]');
  assert.equal(toggleZonesBtn.getAttribute("aria-pressed"), "false");
  toggleZonesBtn.click();
  await flushAsyncTurns();

  const hdr = section(editor, "header");
  assert.equal(hdr.dataset.eoHfZones, "true");
  assert.equal(toggleZonesBtn.getAttribute("aria-pressed"), "true");
  assert.equal(toggleZonesBtn.getAttribute("title"), "3열 분할");
  assert.equal(document.querySelector("#icon-toolbar .tb-hf-label").textContent, "머리글 · 양쪽 편집 중");

  const zones = [...hdr.querySelectorAll(":scope > p[data-eo-zone]")];
  assert.deepEqual(zones.map((p) => p.dataset.eoZone), ["left", "center", "right"]);

  zones[0].textContent = "연구소";
  zones[1].textContent = "연례 보고서";

  // Tab twice from zone 0 -> zone 1 -> zone 2 (right), then insert page-number field into right zone
  frame.focusZone(hdr, 0);
  editor.dispatchEvent({ type: "keydown", key: "Tab", shiftKey: false });
  editor.dispatchEvent({ type: "keydown", key: "Tab", shiftKey: false });
  zones[2].textContent = "";
  document.querySelector('#icon-toolbar [data-hf-action="page-number"]').click();
  await flushAsyncTurns();
  assert.equal(zones[2].querySelectorAll('.eo-page-field[data-eo-field="page"]').length, 1, "field inserted into right zone");
  assert.equal(zones[0].querySelectorAll(".eo-page-field").length, 0);

  frame.endEdit();
  await flushAsyncTurns();

  const layer = editor.querySelector(":scope > footer.doc-page-layer") || Object.assign(document.createElement("footer"), { className: "doc-page-layer" });
  if (!layer.parentNode) editor.appendChild(layer);
  layer.innerHTML = "";
  layer.appendChild(Object.assign(document.createElement("i"), { className: "doc-page-sheet" }));
  frame.paint();

  const copy = layer.querySelector('.doc-page-sheet > .doc-page-hf[data-hf="header"]');
  assert.equal(copy.dataset.hfZones, "true");
  assert.deepEqual([...copy.querySelectorAll(":scope > p")].map((p) => p.dataset.eoZone), ["left", "center", "right"]);
  assert.equal(frame.printCopy("header", 3, 5).dataset.hfZones, "true");
  assert.equal(frame.printCopy("header", 3, 5).querySelector('[data-eo-zone="right"]').textContent, "3");

  const saved = host.__EASYOFFICE_SAVE_PAYLOAD__.collectPageFrame(editor);
  assert.equal(saved.header.length, 3);
  assert.deepEqual(
    saved.header.map((p) => [p.align, p.runs.map((r) => r.field || r.text).join("")]),
    [["left", "연구소"], ["center", "연례 보고서"], ["right", "page"]]
  );
});

test("PD-249: first-page and even/odd toggles author distinct slots, step through prev/next/switch-band, render per sheet, and undo cleanly", async () => {
  const { document, editor, host, sandbox, userClick } = await openDoc("w-hf-variants-pd249");
  const frame = host.__EASYOFFICE_DOC_PAGE_FRAME__;
  userClick(toolbarButton(document, "mod-header"));
  await flushAsyncTurns();

  section(editor, "header").querySelector("p").textContent = "홀수 머리말";

  // Enable first-page different -> switches into firstHeader
  const firstBtn = document.querySelector('#icon-toolbar [data-hf-action="toggle-first"]');
  firstBtn.click();
  await flushAsyncTurns();
  assert.equal(editor.dataset.eoFirstPage, "true");
  assert.equal(firstBtn.getAttribute("aria-pressed"), "true");
  assert.equal(frame.isEditing(), "firstHeader");
  assert.match(document.querySelector("#icon-toolbar .tb-hf-label").textContent, /첫 쪽/);
  section(editor, "firstHeader").querySelector("p").textContent = "표지 머리말";

  // Enable even/odd different -> switches into evenHeader
  const evenBtn = document.querySelector('#icon-toolbar [data-hf-action="toggle-even-odd"]');
  evenBtn.click();
  await flushAsyncTurns();
  assert.equal(editor.dataset.eoEvenOdd, "true");
  assert.equal(evenBtn.getAttribute("aria-pressed"), "true");
  assert.equal(frame.isEditing(), "evenHeader");
  assert.match(document.querySelector("#icon-toolbar .tb-hf-label").textContent, /짝수 쪽/);
  section(editor, "evenHeader").querySelector("p").textContent = "짝수 머리말";

  // Switch band on evenHeader -> evenFooter (button dynamically flips from 바닥글로 이동 -> 머리글로 이동)
  const switchBtn = document.querySelector('#icon-toolbar [data-hf-action="switch-band"]');
  assert.equal(switchBtn.getAttribute("title"), "바닥글로 이동");
  switchBtn.click();
  await flushAsyncTurns();
  assert.equal(frame.isEditing(), "evenFooter");
  assert.equal(switchBtn.getAttribute("title"), "머리글로 이동");
  section(editor, "evenFooter").querySelector("p").textContent = "짝수 꼬리말";

  // Next wraps around to firstHeader -> Next -> firstFooter (leave empty so cover page has no footer) -> Next -> odd header -> Next -> odd footer
  document.querySelector('#icon-toolbar [data-hf-action="next"]').click();
  await flushAsyncTurns();
  assert.equal(frame.isEditing(), "firstHeader");
  document.querySelector('#icon-toolbar [data-hf-action="next"]').click();
  await flushAsyncTurns();
  assert.equal(frame.isEditing(), "firstFooter");
  document.querySelector('#icon-toolbar [data-hf-action="next"]').click();
  await flushAsyncTurns();
  assert.equal(frame.isEditing(), "header");
  assert.match(document.querySelector("#icon-toolbar .tb-hf-label").textContent, /홀수 쪽/);
  document.querySelector('#icon-toolbar [data-hf-action="next"]').click();
  await flushAsyncTurns();
  assert.equal(frame.isEditing(), "footer");
  section(editor, "footer").querySelector("p").textContent = "홀수 꼬리말";

  frame.endEdit();
  await flushAsyncTurns();
  assert.equal(section(editor, "firstFooter"), null, "empty firstFooter pruned on slot switch");

  const layer = editor.querySelector(":scope > footer.doc-page-layer") || Object.assign(document.createElement("footer"), { className: "doc-page-layer" });
  if (!layer.parentNode) editor.appendChild(layer);
  layer.innerHTML = "";
  for (let i = 0; i < 3; i += 1) layer.appendChild(Object.assign(document.createElement("i"), { className: "doc-page-sheet" }));
  frame.paint();

  const sheets = layer.querySelectorAll(".doc-page-sheet");
  assert.equal(sheets[0].querySelector('.doc-page-hf[data-hf="header"]').textContent, "표지 머리말");
  assert.equal(sheets[0].querySelector('.doc-page-hf[data-hf="footer"]'), null, "cover page hides footer when firstFooter is empty");
  assert.equal(sheets[1].querySelector('.doc-page-hf[data-hf="header"]').textContent, "짝수 머리말");
  assert.equal(sheets[1].querySelector('.doc-page-hf[data-hf="footer"]').textContent, "짝수 꼬리말");
  assert.equal(sheets[2].querySelector('.doc-page-hf[data-hf="header"]').textContent, "홀수 머리말");
  assert.equal(sheets[2].querySelector('.doc-page-hf[data-hf="footer"]').textContent, "홀수 꼬리말");

  const saved = host.__EASYOFFICE_SAVE_PAYLOAD__.collectPageFrame(editor);
  assert.equal(saved.firstPageDifferent, true);
  assert.equal(saved.evenOddDifferent, true);
  assert.equal(saved.firstHeader[0].runs[0].text, "표지 머리말");
  assert.equal(saved.evenHeader[0].runs[0].text, "짝수 머리말");
  assert.equal(saved.evenFooter[0].runs[0].text, "짝수 꼬리말");
  assert.equal(saved.header[0].runs[0].text, "홀수 머리말");
  assert.equal(saved.footer[0].runs[0].text, "홀수 꼬리말");

  // Undo can step back cleanly
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(editor.dataset.eoFirstPage, "true");
});

test("PD-249: opening a decoded document with sparse 2-zone [center, right] header mounts 3-zone grid and roundtrips without empty left paragraph", async () => {
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
  const decoded = {
    status: "OK",
    action: "decode",
    fileName: "sparse-zones.docx",
    format: "docx",
    module: "DOC",
    kind: "document",
    title: "구역",
    sourceBytes: 256,
    specification: "OOXML",
    itemCount: 1,
    warnings: [],
    header: [
      { type: "paragraph", align: "center", runs: [{ text: "가운데 제목" }] },
      { type: "paragraph", align: "right", runs: [{ text: "", field: "page" }] },
    ],
    blocks: [{ type: "paragraph", runs: [{ text: "본문" }] }],
  };
  const testIpc = async (cmd, rawPayload) => {
    const body = typeof rawPayload === "string" && rawPayload.trimStart().startsWith("{") ? JSON.parse(rawPayload) : null;
    if (cmd === "convert_document_format" && body?.action === "decode") return decoded;
    return offline[cmd](rawPayload);
  };
  const h = await createWorkspaceHarness({ search: "?module=DOC&win=w-hf-open-zones", testIpc });
  await flushAsyncTurns(6);
  const res = await h.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("sparse-zones.docx", new Uint8Array([80, 75, 3, 4]));
  assert.equal(res.ok, true);
  await flushAsyncTurns(6);

  const editor = h.document.getElementById("doc-editor");
  const hdr = section(editor, "header");
  assert.equal(hdr.dataset.eoHfZones, "true");
  const zones = [...hdr.querySelectorAll(":scope > p[data-eo-zone]")];
  assert.equal(zones.length, 3);
  assert.equal(zones[0].textContent, "");
  assert.equal(zones[1].textContent, "가운데 제목");
  assert.equal(zones[2].querySelectorAll('.eo-page-field[data-eo-field="page"]').length, 1);

  const saved = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectPageFrame(editor);
  assert.equal(saved.header.length, 2, "blank left compartment omitted from saved paragraphs");
  assert.equal(saved.header[0].align, "center");
  assert.equal(saved.header[1].align, "right");
});
