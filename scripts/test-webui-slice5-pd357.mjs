import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { createSelection, rangeOf } from "./test-support/doc-run-harness.mjs";

const $ = (document, id) => document.getElementById(id);

function dispatchKey(h, opts, target) {
  if (target) h.document.activeElement = target;
  let prevented = false;
  let stopped = false;
  const ev = {
    type: "keydown",
    key: opts.key || "",
    code: opts.code || "",
    ctrlKey: Boolean(opts.ctrlKey),
    metaKey: Boolean(opts.metaKey),
    altKey: Boolean(opts.altKey),
    shiftKey: Boolean(opts.shiftKey),
    isComposing: Boolean(opts.isComposing),
    bubbles: true,
    cancelable: true,
    target: target || h.document.activeElement || h.document.body,
    get defaultPrevented() {
      return prevented;
    },
    preventDefault() {
      prevented = true;
    },
    stopPropagation() {
      stopped = true;
    },
  };
  if (target && target !== h.window && target !== h.document && typeof target.dispatchEvent === "function") {
    const kept = target.dispatchEvent(ev);
    if (kept === false) prevented = true;
  }
  if (!stopped && !prevented) {
    const kept = h.document.dispatchEvent(ev);
    if (kept === false) prevented = true;
  }
  if (!prevented && !stopped) {
    const wEv = h.sandbox.dispatchWindowKeydown(ev);
    if (wEv.defaultPrevented) prevented = true;
  }
  return { defaultPrevented: prevented };
}

function selectTextInNode(selection, h, textNode, start, end) {
  selection.removeAllRanges();
  selection.addRange(rangeOf(textNode, start, textNode, end));
  h.document.activeElement = $(h.document, "doc-editor");
}

function setFindQuery(h, text) {
  const input = $(h.document, "find-query");
  h.document.activeElement = input;
  input.value = text;
  input.dispatchEvent({ type: "input", target: input });
}

test("DOC: sup/sub/strike style bar, Korean IME physical Equal shortcut, paragraph spacing (0..400pt), and rich table-cell roundtrip", async () => {
  const selection = createSelection();
  let decodedReply = null;
  const h = await createWorkspaceHarness({
    selection,
    shellCommands: { confirm_dialog: () => true },
    tauriInvoke: async (cmd) => {
      if (cmd === "convert_document_format" && decodedReply) return decodedReply;
      return { status: "OK" };
    },
  });
  await flushAsyncTurns(10);
  switchModule(h.document, "DOC");
  await flushAsyncTurns(4);

  // 1. Style bar buttons and menu items present for DOC
  for (const id of ["btn-stylebar-strike", "btn-stylebar-sup", "btn-stylebar-sub", "doc-space-before-input", "doc-space-after-input"]) {
    assert.ok($(h.document, id), `missing DOC control #${id}`);
  }
  assert.ok(h.document.querySelector('#menu-bar [data-studio-menu-cmd="home-superscript"]'));
  assert.ok(h.document.querySelector('#menu-bar [data-studio-menu-cmd="home-subscript"]'));
  assert.ok(h.document.querySelector('#menu-bar [data-studio-menu-cmd="home-strike"]'));

  // 2. Sup/sub toggle + Korean IME physical code="Equal" shortcuts inside #doc-editor only
  const editor = $(h.document, "doc-editor");
  editor.innerHTML = "<p>H2O and E=mc2</p>";
  const p = editor.firstChild;
  const textNode = p.firstChild;

  // Select "2" in H2O and trigger Ctrl+= (Korean IME key="Process", code="Equal")
  selectTextInNode(selection, h, textNode, 1, 2);
  const subEv = dispatchKey(h, { key: "Process", code: "Equal", ctrlKey: true }, p);
  assert.equal(subEv.defaultPrevented, true, "Ctrl+Equal inside DOC editor must prevent zoom and toggle subscript");
  const subEl = editor.querySelector('.eo-run[data-eo-subscript="true"], sub');
  assert.ok(subEl, "Ctrl+Equal creates subscript run");
  assert.equal(subEl.textContent, "2");

  // Click superscript button on another span ("2" at end -> toggle sup & ensure mutual exclusion works)
  const lastText = Array.from(p.childNodes).find((n) => n.nodeType === 3 && n.textContent.includes("mc2"));
  assert.ok(lastText);
  const idx2 = lastText.textContent.indexOf("2");
  selectTextInNode(selection, h, lastText, idx2, idx2 + 1);
  $(h.document, "btn-stylebar-sup").click();
  const supEl = editor.querySelector('.eo-run[data-eo-superscript="true"], sup');
  assert.equal(supEl?.textContent, "2");

  // Physical Ctrl+Shift+Equal toggles superscript off when re-triggered on the sup node
  const supText = supEl.firstChild;
  selectTextInNode(selection, h, supText, 0, 1);
  const supEv = dispatchKey(h, { key: "Process", code: "Equal", ctrlKey: true, shiftKey: true }, supEl);
  assert.equal(supEv.defaultPrevented, true);
  assert.equal(editor.querySelector('.eo-run[data-eo-superscript="true"], sup'), null, "repeating Ctrl+Shift+Equal removes superscript");

  // Outside #doc-editor (or in HWP), Ctrl+Equal must NOT hijack into subscript
  h.document.activeElement = h.document.body;
  dispatchKey(h, { key: "=", code: "Equal", ctrlKey: true }, h.document.body);
  assert.equal(editor.querySelectorAll('.eo-run[data-eo-subscript="true"], sub').length, 1);
  // In HWP, Ctrl+Equal does NOT toggle DOC subscript (it falls through to window zoom-in instead).
  switchModule(h.document, "HWP");
  await flushAsyncTurns(4);
  const hwpText = editor.querySelector("p")?.firstChild || editor.firstChild;
  if (hwpText) selectTextInNode(selection, h, hwpText, 0, Math.min(1, hwpText.textContent?.length || 0));
  const zoomBeforeHwp = h.document.getElementById("status-zoom-level")?.textContent || "100%";
  dispatchKey(h, { key: "=", code: "Equal", ctrlKey: true }, editor.firstChild);
  assert.equal(editor.querySelectorAll('.eo-run[data-eo-subscript="true"], sub').length, 0, "HWP must not apply DOC subscript");
  assert.notEqual(h.document.getElementById("status-zoom-level")?.textContent, zoomBeforeHwp, "HWP Ctrl+Equal falls through to zoom-in");
  dispatchKey(h, { key: "-", code: "Minus", ctrlKey: true }, h.document.body);
  switchModule(h.document, "DOC");
  await flushAsyncTurns(4);

  // 3. Paragraph spacing 0..400pt clamping + save-payload -> openDocumentFromBytes roundtrip
  const helper = h.host.__EASYOFFICE_DOC_TABLE_RUNS__;
  assert.ok(helper);
  const beforeInput = $(h.document, "doc-space-before-input");
  const afterInput = $(h.document, "doc-space-after-input");
  const curP = editor.querySelector("p");
  selectTextInNode(selection, h, curP.firstChild, 0, 1);

  beforeInput.value = "12.5";
  beforeInput.dispatchEvent({ type: "change", target: beforeInput });
  afterInput.value = "999";
  afterInput.dispatchEvent({ type: "change", target: afterInput });
  assert.equal(helper.readSpacingPt(curP, "before"), 12.5);
  assert.equal(helper.readSpacingPt(curP, "after"), 400, "paragraph spacing clamps to 400pt max");

  beforeInput.value = "-8";
  beforeInput.dispatchEvent({ type: "change", target: beforeInput });
  assert.equal(helper.readSpacingPt(curP, "before"), 0, "negative paragraph spacing clamps to 0pt");

  // 4. Rich table-cell inner run formatting + paragraph align/line-spacing roundtrip
  editor.innerHTML =
    '<p style="margin-top:24px;margin-bottom:32px" data-eo-space-before-twips="360" data-eo-space-after-twips="480">Intro</p>' +
    '<table><tbody><tr>' +
    '<td><p style="text-align:center;line-height:1.5" data-eo-line-spacing="150"><b>Bold</b> <sub>sub</sub> <span style="color:#b91c1c">Red</span></p><p style="text-align:right">Right<sup>sup</sup></p></td>' +
    '<td>Plain <a href="https://example.org" data-eo-link="true">Link</a></td>' +
    "</tr></tbody></table>";

  const blocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__?.collectDocBlocks?.(editor);
  assert.ok(Array.isArray(blocks) && blocks.length >= 2);
  assert.equal(blocks[0].spaceBeforeTwips, 360);
  assert.equal(blocks[0].spaceAfterTwips, 480);

  const tbl = blocks[1];
  assert.equal(tbl.type, "table");
  const richCell = tbl.cellParagraphs?.find((c) => c.row === 0 && c.col === 0);
  assert.ok(richCell, "cellParagraphs captured for rich cell");
  assert.equal(richCell.paragraphs.length, 2);
  assert.equal(richCell.paragraphs[0].align, "center");
  assert.equal(richCell.paragraphs[0].lineSpacingPercent, 150);
  assert.equal(richCell.paragraphs[0].runs.some((r) => r.bold && r.text === "Bold"), true);
  assert.equal(richCell.paragraphs[0].runs.some((r) => r.script === "subscript" && r.text === "sub"), true);
  assert.equal(richCell.paragraphs[0].runs.some((r) => r.color === "B91C1C" && r.text === "Red"), true);
  assert.equal(richCell.paragraphs[1].align, "right");
  assert.equal(richCell.paragraphs[1].runs.some((r) => r.script === "superscript" && r.text === "sup"), true);
  assert.ok(tbl.links?.some((l) => l.row === 0 && l.col === 1 && l.url === "https://example.org"));

  // Roundtrip restore via openDocumentFromBytes (converting saved blocks back through file-open)
  decodedReply = {
    status: "OK",
    module: "DOC",
    format: "docx",
    title: "slice5-doc",
    blocks,
    warnings: [],
  };
  const openRes = await h.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("slice5-doc.docx", new Uint8Array([80, 75, 3, 4]));
  assert.equal(openRes.ok, true);
  await flushAsyncTurns(6);

  const restoredPs = editor.querySelectorAll("p");
  assert.equal(helper.readSpacingPt(restoredPs[0], "before"), 18);
  assert.equal(helper.readSpacingPt(restoredPs[0], "after"), 24);
  const restoredCell = editor.querySelector("table td");
  assert.equal(restoredCell.querySelectorAll("p").length, 2);
  assert.equal(restoredCell.querySelectorAll("p")[0].style.textAlign, "center");
  assert.equal(restoredCell.querySelector('.eo-run[data-eo-bold="true"], b, strong')?.textContent, "Bold");
  assert.equal(restoredCell.querySelector('.eo-run[data-eo-subscript="true"], sub')?.textContent, "sub");
  assert.equal(restoredCell.querySelector('.eo-run[data-eo-superscript="true"], sup')?.textContent, "sup");
});

test("SLIDE: clipboard Cut/Copy/Paste/Duplicate + 1-step undo, editable isolation, context menu, Z-order, align/distribute, and multiline notes", async () => {
  const h = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  switchModule(h.document, "SLIDE");
  await flushAsyncTurns(6);

  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  const ctxMenu = h.host.__EASYOFFICE_SLIDE_CONTEXT_MENU__;
  assert.ok(canvas && ctxMenu);

  // Multi-line presenter notes preserves newlines and ignores canvas Ctrl+D/X/C
  const notes = $(h.document, "slide-presenter-notes");
  assert.equal(notes.tagName, "TEXTAREA");
  notes.value = "첫째 줄 노트\n둘째 줄 노트";
  notes.dispatchEvent({ type: "input", target: notes });
  assert.equal(notes.value, "첫째 줄 노트\n둘째 줄 노트");

  // Clear default elements if any and add 3 deterministic boxes
  while (canvas.getElements().length > 0) {
    canvas.deleteElement(canvas.getElements()[0].id);
  }
  const e1 = canvas.addElement({ kind: "textbox", x: 40, y: 40, w: 100, h: 40, text: "A" });
  const e2 = canvas.addElement({ kind: "textbox", x: 180, y: 120, w: 80, h: 40, text: "B" });
  const e3 = canvas.addElement({ kind: "textbox", x: 400, y: 80, w: 60, h: 40, text: "C" });
  assert.deepEqual(canvas.getElements().map((e) => e.id), [e1.id, e2.id, e3.id]);

  // Inside presenter notes, Ctrl+D must NOT duplicate slide element!
  dispatchKey(h, { key: "d", code: "KeyD", ctrlKey: true }, notes);
  assert.equal(canvas.getElements().length, 3, "editable textarea must isolate canvas Ctrl+D");

  // Select e1 on stage and Ctrl+C -> Ctrl+V -> offset +16, then 1-step Ctrl+Z undo
  canvas.selectElement(e1.id);
  const stageLayer = $(h.document, "slide-canvas-layer");
  const e1Card = stageLayer.querySelector(`.slide-el[data-element-id="${e1.id}"]`);
  assert.equal(dispatchKey(h, { key: "c", code: "KeyC", ctrlKey: true }, e1Card).defaultPrevented, true);
  assert.equal(dispatchKey(h, { key: "v", code: "KeyV", ctrlKey: true }, e1Card).defaultPrevented, true);
  assert.equal(canvas.getElements().length, 4);
  const pasted = canvas.getElements()[3];
  assert.equal(pasted.x, 56);
  assert.equal(pasted.y, 56);

  // Duplicate & Cut via Ctrl+D / Ctrl+X
  assert.equal(dispatchKey(h, { key: "d", code: "KeyD", ctrlKey: true }, stageLayer).defaultPrevented, true);
  assert.equal(canvas.getElements().length, 5);
  assert.equal(dispatchKey(h, { key: "x", code: "KeyX", ctrlKey: true }, stageLayer).defaultPrevented, true);
  assert.equal(canvas.getElements().length, 4);

  // Undo cut -> 5, undo duplicate -> 4, undo paste -> 3
  dispatchKey(h, { key: "z", code: "KeyZ", ctrlKey: true }, h.document.body);
  await flushAsyncTurns(10);
  assert.equal(canvas.getElements().length, 5);
  dispatchKey(h, { key: "z", code: "KeyZ", ctrlKey: true }, h.document.body);
  await flushAsyncTurns(10);
  assert.equal(canvas.getElements().length, 4);
  dispatchKey(h, { key: "z", code: "KeyZ", ctrlKey: true }, h.document.body);
  await flushAsyncTurns(10);
  assert.equal(canvas.getElements().length, 3);

  // Right-click canvas context menu opens, handles bring-forward, and dismisses on Escape
  canvas.selectElement(e1.id);
  const card1 = stageLayer.querySelector(`.slide-el[data-element-id="${e1.id}"]`);
  card1.dispatchEvent({ type: "contextmenu", bubbles: true, cancelable: true, target: card1, clientX: 120, clientY: 140, preventDefault() {} });
  assert.equal($(h.document, "slide-context-menu").hidden, false);
  assert.equal($(h.document, "slide-context-menu").dataset.mode, "canvas");

  dispatchKey(h, { key: "Escape", code: "Escape" }, h.document.body);
  assert.equal($(h.document, "slide-context-menu").hidden, true, "Escape dismisses slide context menu");

  // Re-open and click bring-forward in context menu
  card1.dispatchEvent({ type: "contextmenu", bubbles: true, cancelable: true, target: card1, clientX: 120, clientY: 140, preventDefault() {} });
  const fwdItem = h.document.querySelector('#slide-ctx-canvas-group [data-slide-canvas-act="bring-forward"]');
  fwdItem.dispatchEvent({ type: "click", bubbles: true, target: fwdItem, stopPropagation() {} });
  assert.deepEqual(canvas.getElements().map((e) => e.id), [e2.id, e1.id, e3.id], "bring-forward swaps only 1 z-order step");
  assert.equal($(h.document, "slide-context-menu").hidden, true);

  // Single-step send-backward brings it back
  assert.equal(canvas.sendBackward(e1.id), true);
  assert.deepEqual(canvas.getElements().map((e) => e.id), [e1.id, e2.id, e3.id]);

  // Alignment & 3+ distribute
  canvas.setSelectedIds([e1.id, e2.id]);
  assert.equal(ctxMenu.alignElements("distribute-h"), false, "distribute requires >= 3 items");
  assert.equal(ctxMenu.alignElements("align-left"), true);
  const afterLeft = canvas.getElements();
  assert.equal(afterLeft[0].x, 40);
  assert.equal(afterLeft[1].x, 40);

  // Undo align-left, reset positions, and distribute-h across all 3 items
  dispatchKey(h, { key: "z", code: "KeyZ", ctrlKey: true }, h.document.body);
  await flushAsyncTurns(10);
  assert.equal(canvas.getElements()[1].x, 180);

  canvas.updateElement(e2.id, { x: 100 });
  canvas.setSelectedIds([e1.id, e2.id, e3.id]);
  assert.equal(ctxMenu.alignElements("distribute-h"), true);
  assert.equal(canvas.getElements().find((e) => e.id === e2.id).x, 230);
});

test("Cross-module Ctrl+F search works in SLIDE, PDF (multi-page + AcroForm), and BOARD while preserving find-query focus", async () => {
  let decodedReply = null;
  const h = await createWorkspaceHarness({
    shellCommands: { confirm_dialog: () => true },
    tauriInvoke: async (cmd) => {
      if (cmd === "convert_document_format" && decodedReply) return decodedReply;
      return { status: "OK" };
    },
  });
  await flushAsyncTurns(10);

  // Menu item must include SLIDE, BOARD, PDF
  const findItem = h.document.querySelector('#menu-bar [data-studio-menu-cmd="find"]');
  assert.ok(findItem);
  const kinds = String(findItem.dataset.kinds || "").split(/\s+/);
  for (const mod of ["SLIDE", "PDF", "BOARD"]) {
    assert.equal(kinds.includes(mod), true, `find menu missing ${mod}`);
  }

  // 1. SLIDE search across title/canvas shape/notes
  switchModule(h.document, "SLIDE");
  await flushAsyncTurns(6);
  const hooks = h.host.__EASYOFFICE_MODULE_FIND_HOOKS__;
  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  h.userClick($(h.document, "btn-add-slide"));
  h.userClick($(h.document, "btn-add-slide"));
  hooks.switchSlide(0);
  const deck = hooks.getSlideDeck();
  deck[0].title = "소개 슬라이드";
  hooks.switchSlide(1);
  const targetShape = canvas.addElement({ kind: "shape", x: 80, y: 80, w: 160, h: 80, text: "분기 목표" });
  hooks.switchSlide(2);
  const notes = $(h.document, "slide-presenter-notes");
  notes.value = "세부 분기 예산 발표 메모";
  notes.dispatchEvent({ type: "input", target: notes });
  hooks.switchSlide(0);

  assert.equal(dispatchKey(h, { key: "f", code: "KeyF", ctrlKey: true }, h.document.body).defaultPrevented, true);
  assert.equal($(h.document, "find-dialog").open, true);
  assert.equal($(h.document, "find-replace-row").hidden, true, "find-only module keeps replace row hidden");

  setFindQuery(h, "분기");
  await flushAsyncTurns(4);
  assert.equal($(h.document, "find-count").textContent, "1 / 2");
  assert.equal(hooks.getSlideIndex(), 1, "SLIDE search jumped to matching shape slide");
  assert.equal(canvas.getSelectedElement()?.id, targetShape.id);
  assert.equal(h.document.activeElement, $(h.document, "find-query"), "live SLIDE find jump must preserve #find-query focus");

  dispatchKey(h, { key: "Enter", code: "Enter" }, $(h.document, "find-query"));
  await flushAsyncTurns(2);
  assert.equal($(h.document, "find-count").textContent, "2 / 2");
  assert.equal(hooks.getSlideIndex(), 2, "Enter advances SLIDE search to presenter notes slide");
  assert.equal($(h.document, "slide-presenter-notes").classList.contains("eo-search-hit"), true);
  assert.equal(h.document.activeElement, $(h.document, "find-query"));

  dispatchKey(h, { key: "Escape", code: "Escape" }, $(h.document, "find-query"));
  assert.equal($(h.document, "find-dialog").open, false, "Escape closes find dialog");

  // 2. PDF multi-page + form field search
  switchModule(h.document, "PDF");
  await flushAsyncTurns(6);
  decodedReply = {
    status: "OK",
    module: "PDF",
    format: "pdf",
    title: "계약서",
    blocks: [{ runs: [{ text: "제1쪽 일반 조건" }] }, { runs: [{ text: "제2쪽 특별 조항" }] }],
    pdfPages: [
      { pageNumber: 1, text: "제1쪽 일반 조건" },
      { pageNumber: 2, text: "제2쪽 특별 조항" },
    ],
    pdfForm: { hasAcroForm: true, applicant: "홍길동", department: "법무팀", fieldCount: 1, fields: [{ name: "담당자", value: "김철수" }] },
    warnings: [],
  };
  const pdfOpenRes = await h.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("multi.pdf", new Uint8Array([37, 80, 68, 70, 45]));
  assert.equal(pdfOpenRes.ok, true);
  await flushAsyncTurns(6);

  dispatchKey(h, { key: "f", code: "KeyF", ctrlKey: true }, h.document.body);
  setFindQuery(h, "특별 조항");
  await flushAsyncTurns(4);
  assert.equal($(h.document, "find-count").textContent, "1 / 1");
  assert.equal($(h.document, "pdf-page-indicator").textContent, "쪽 2 / 2", "PDF search jumps to matching page 2");
  assert.equal(h.document.activeElement, $(h.document, "find-query"));

  setFindQuery(h, "김철수");
  await flushAsyncTurns(4);
  assert.equal($(h.document, "find-count").textContent, "1 / 1");
  assert.equal($(h.document, "pdf-acroform-grid").hidden, false);
  assert.equal(h.document.querySelectorAll("#pdf-form-extracted .pdf-form-extracted-chip")[0].classList.contains("eo-search-hit"), true);
  dispatchKey(h, { key: "Escape", code: "Escape" }, $(h.document, "find-query"));

  // 3. BOARD search selects node/connector and pans viewport to center
  switchModule(h.document, "BOARD");
  await flushAsyncTurns(6);
  const board = h.host.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
  const nearNode = board.addNode({ kind: "shape", x: 100, y: 100, w: 140, h: 70, text: "기준 노드" });
  const offNode = board.addNode({ kind: "shape", x: 2400, y: 1600, w: 180, h: 90, text: "원거리 아키텍처 노드" });
  const addedConn = board.addConnector({ fromId: nearNode.id, toId: offNode.id, label: "데이터 흐름" });
  assert.ok(addedConn);
  // Trigger fresh state via replaceState or read viewport via search
  dispatchKey(h, { key: "f", code: "KeyF", ctrlKey: true }, h.document.body);
  setFindQuery(h, "원거리 아키텍처");
  await flushAsyncTurns(4);
  assert.equal($(h.document, "find-count").textContent, "1 / 1");
  assert.equal(board._styleHooks.getSelection().node?.id, offNode.id);
  assert.notEqual(board.getState().viewport.panX, 0, "BOARD find must pan viewport toward offscreen node");
  assert.equal(h.document.activeElement, $(h.document, "find-query"));

  setFindQuery(h, "데이터 흐름");
  await flushAsyncTurns(4);
  assert.equal($(h.document, "find-count").textContent, "1 / 1");
  assert.ok(board._styleHooks.getSelection().connector);
});

test("BASE import: new/append/replace with 1-step undo, type inference, CSV formula neutralization, and strict fail-closed malicious bounds", async () => {
  const h = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  switchModule(h.document, "BASE");
  await flushAsyncTurns(6);

  const imp = h.host.__EASYOFFICE_BASE_IMPORT__;
  const model = h.host.__EASYOFFICE_BASE_MODEL__;
  const baseStudio = h.host.__EASYOFFICE_BASE_BOARD_HOST__.getBaseStudio();
  assert.ok(imp && model && baseStudio);

  // Neutralization preserves strict negative numbers but prefixes formula-leading strings
  assert.equal(imp.neutralizeFormulaText("-42.5"), "-42.5");
  assert.equal(imp.neutralizeFormulaText("=CMD()"), "'=CMD()");
  assert.equal(imp.neutralizeFormulaText("+1+1"), "'+1+1");
  assert.equal(imp.neutralizeFormulaText("@SUM(A1)"), "'@SUM(A1)");

  // Strict raw precheck rejections
  assert.equal(imp.decodeRawBytesPrecheck("empty.csv", new Uint8Array(0)).ok, false);
  assert.equal(imp.decodeRawBytesPrecheck("bad.exe", new Uint8Array([1, 2])).ok, false);
  const bigBuf = new Uint8Array(imp.MAX_IMPORT_BYTES + 1);
  assert.equal(imp.decodeRawBytesPrecheck("overflow.csv", bigBuf).ok, false);
  assert.equal(imp.decodeRawBytesPrecheck("badutf8.csv", new Uint8Array([0xff, 0xfe, 0x00])).ok, false);
  assert.equal(imp.decodeRawBytesPrecheck("nul.tsv", new TextEncoder().encode("a\tb\n1\t2\0")).ok, false);

  // Rust decode warning truncation must fail closed
  const warnRes = await imp.decodeImportFile("warn.csv", new TextEncoder().encode("a,b\n1,2"), async () => ({
    sheets: [{ name: "s", cells: [{ r: 0, c: 0, raw: "a", t: "s" }] }],
    warnings: ["셀 5개는 가져오지 않았습니다."],
  }));
  assert.equal(warnRes.ok, false);

  // Bounds inside sheetToTableGrid: >64 cols, >10000 rows, >128 header chars, >8192 cell chars
  const tooManyCols = [];
  for (let c = 0; c <= imp.MAX_IMPORT_FIELDS; c += 1) tooManyCols.push({ r: 0, c, raw: `H${c}`, t: "s" });
  assert.equal(imp.sheetToTableGrid({ name: "s", cells: tooManyCols }).ok, false);

  assert.equal(
    imp.sheetToTableGrid({
      name: "s",
      cells: [
        { r: 0, c: 0, raw: "Header", t: "s" },
        { r: imp.MAX_IMPORT_RECORDS + 1, c: 0, raw: "val", t: "s" },
      ],
    }).ok,
    false
  );
  assert.equal(
    imp.sheetToTableGrid({ name: "s", cells: [{ r: 0, c: 0, raw: "X".repeat(imp.MAX_HEADER_CHARS + 1), t: "s" }] }).ok,
    false
  );
  assert.equal(
    imp.sheetToTableGrid({
      name: "s",
      cells: [
        { r: 0, c: 0, raw: "Col", t: "s" },
        { r: 1, c: 0, raw: "Y".repeat(imp.MAX_CELL_CHARS + 1), t: "s" },
      ],
    }).ok,
    false
  );

  // Valid import via controller (`new` mode -> append -> replace) with 1-step Ctrl+Z undo
  const sampleSheet = {
    name: "매출표",
    cells: [
      { r: 0, c: 0, raw: "항목", t: "s" },
      { r: 0, c: 1, raw: "항목", t: "s" },
      { r: 0, c: 2, raw: "금액", t: "s" },
      { r: 0, c: 3, raw: "완료", t: "s" },
      { r: 0, c: 4, raw: "일자", t: "s" },
      { r: 0, c: 5, raw: "링크", t: "s" },
      { r: 1, c: 0, raw: '=HYPERLINK("x")', t: "s" },
      { r: 1, c: 1, raw: "비고", t: "s" },
      { r: 1, c: 2, raw: "-42.5", t: "n" },
      { r: 1, c: 3, raw: "true", t: "b" },
      { r: 1, c: 4, raw: "2026-11-26", t: "s" },
      { r: 1, c: 5, raw: "https://example.com", t: "s" },
    ],
  };

  const ctrl = imp.controller();
  const initialCount = baseStudio.getState().tables.length;
  const rNew = ctrl.importDecodedSheets([sampleSheet], { mode: "new", tableName: "수입내역", hasHeader: true });
  assert.equal(rNew.ok, true);
  assert.equal(baseStudio.getState().tables.length, initialCount + 1);
  const newTbl = model.getTable(baseStudio.getState());
  assert.equal(newTbl.name, "수입내역");
  assert.deepEqual(newTbl.fields.map((f) => f.name), ["항목", "항목 (2)", "금액", "완료", "일자", "링크"]);
  assert.deepEqual(newTbl.fields.map((f) => f.type), ["text", "text", "number", "checkbox", "date", "url"]);
  assert.equal(newTbl.records[0].values[newTbl.fields[0].id], "'=HYPERLINK(\"x\")");
  assert.equal(newTbl.records[0].values[newTbl.fields[2].id], -42.5);
  assert.equal(newTbl.records[0].values[newTbl.fields[3].id], true);

  // Append 1 row
  const rAppend = ctrl.importDecodedSheets([sampleSheet], { mode: "append", hasHeader: true });
  assert.equal(rAppend.ok, true);
  assert.equal(model.getTable(baseStudio.getState()).records.length, 2);

  // Replace records
  const rReplace = ctrl.importDecodedSheets([sampleSheet], { mode: "replace", hasHeader: true });
  assert.equal(rReplace.ok, true);
  assert.equal(model.getTable(baseStudio.getState()).records.length, 1);

  // 1-step undo back to 2 records, 1-step undo back to 1 record, 1-step undo removes new table
  dispatchKey(h, { key: "z", code: "KeyZ", ctrlKey: true }, h.document.body);
  await flushAsyncTurns(10);
  assert.equal(model.getTable(baseStudio.getState()).records.length, 2);
  dispatchKey(h, { key: "z", code: "KeyZ", ctrlKey: true }, h.document.body);
  await flushAsyncTurns(10);
  assert.equal(model.getTable(baseStudio.getState()).records.length, 1);
  dispatchKey(h, { key: "z", code: "KeyZ", ctrlKey: true }, h.document.body);
  await flushAsyncTurns(10);
  assert.equal(baseStudio.getState().tables.length, initialCount);
});

test("BOARD: contextual selection style bar, multi-node/connector clipboard + offset + 1-step undo + limits", async () => {
  const h = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  switchModule(h.document, "BOARD");
  await flushAsyncTurns(6);

  const board = h.host.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
  const barApi = h.host.__EASYOFFICE_BOARD_STYLE_BAR__;
  assert.ok(board && barApi);

  const styleBar = $(h.document, "board-style-bar");
  assert.ok(styleBar);
  board.selectNode(null);
  assert.equal(styleBar.hidden, true, "style bar hides when nothing is selected");

  // Selecting a shape shows all 4 controls and commits style changes with 1-step undo
  const n1 = board.addNode({ kind: "shape", x: 100, y: 100, w: 140, h: 70, text: "노드1" });
  const n2 = board.addNode({ kind: "shape", x: 320, y: 120, w: 140, h: 70, text: "노드2" });
  const conn = board.addConnector({ fromId: n1.id, toId: n2.id, label: "흐름" });
  board.selectNode(n1.id);
  assert.equal(styleBar.hidden, false);
  assert.equal($(h.document, "board-style-fill-wrap").hidden, false);
  assert.equal($(h.document, "board-style-stroke-wrap").hidden, false);

  const fillEl = $(h.document, "board-style-fill");
  fillEl.value = "#fef08a";
  fillEl.dispatchEvent({ type: "change", target: fillEl });
  assert.equal(board.getState().nodes.find((n) => n.id === n1.id).fill, "FEF08A");

  const widthEl = $(h.document, "board-style-width");
  widthEl.value = "4";
  widthEl.dispatchEvent({ type: "change", target: widthEl });
  assert.equal(board.getState().nodes.find((n) => n.id === n1.id).strokeWidthPt, 4);

  const labelEl = $(h.document, "board-style-label");
  labelEl.value = "수정된 노드";
  labelEl.dispatchEvent({ type: "change", target: labelEl });
  assert.equal(board.getState().nodes.find((n) => n.id === n1.id).text, "수정된 노드");

  // While editing labelEl, Ctrl+D must NOT duplicate board nodes
  const countBeforeEditableD = board.getState().nodes.length;
  dispatchKey(h, { key: "d", code: "KeyD", ctrlKey: true }, labelEl);
  assert.equal(board.getState().nodes.length, countBeforeEditableD, "style bar input must isolate board Ctrl+D");

  // Undo label -> restores "노드1" in 1 step
  dispatchKey(h, { key: "z", code: "KeyZ", ctrlKey: true }, h.document.body);
  await flushAsyncTurns(10);
  assert.equal(board.getState().nodes.find((n) => n.id === n1.id).text, "노드1");

  // Connector selection hides fill and edits connector stroke/label
  board.selectConnector(conn.id);
  assert.equal($(h.document, "board-style-fill-wrap").hidden, true);
  assert.equal($(h.document, "board-style-stroke-wrap").hidden, false);
  labelEl.value = "연결 이름 변경";
  labelEl.dispatchEvent({ type: "change", target: labelEl });
  assert.equal(board.getState().connectors.find((c) => c.id === conn.id).label, "연결 이름 변경");

  // Multi-node selection + internal connector copy/paste/duplicate (+16px offset + remapped connector + 1-step undo)
  const vpEl = $(h.document, "board-viewport");
  board.selectNode(n1.id);
  board.selectNode(n2.id, { toggle: true });

  const preNodes = board.getState().nodes.length;
  const preConns = board.getState().connectors.length;
  assert.equal(dispatchKey(h, { key: "c", code: "KeyC", ctrlKey: true }, vpEl).defaultPrevented, true);
  assert.equal(dispatchKey(h, { key: "v", code: "KeyV", ctrlKey: true }, vpEl).defaultPrevented, true);
  assert.equal(board.getState().nodes.length, preNodes + 2);
  assert.equal(board.getState().connectors.length, preConns + 1, "internal connector cloned alongside multi-node paste");

  const pastedConn = board.getState().connectors.at(-1);
  assert.notEqual(pastedConn.fromId, n1.id);
  assert.notEqual(pastedConn.toId, n2.id);
  const pastedFrom = board.getState().nodes.find((n) => n.id === pastedConn.fromId);
  assert.equal(pastedFrom.x, 116);
  assert.equal(pastedFrom.y, 116);

  // Second paste offsets +32px; 1-step Ctrl+Z removes second paste
  dispatchKey(h, { key: "v", code: "KeyV", ctrlKey: true }, vpEl);
  assert.equal(board.getState().nodes.length, preNodes + 4);
  assert.equal(board.getState().nodes.at(-2).x, 132);
  dispatchKey(h, { key: "z", code: "KeyZ", ctrlKey: true }, h.document.body);
  await flushAsyncTurns(10);
  assert.equal(board.getState().nodes.length, preNodes + 2);
});

test("Regression: DOC multi-para table cell links, trailing <br>, and wrapper font/color inheritance", async () => {
  const h = await createWorkspaceHarness({ search: "?module=DOC&win=w-pd357-doc-reg" });
  await flushAsyncTurns(8);
  const editor = $(h.document, "doc-editor");
  editor.innerHTML =
    '<table class="doc-inline-table"><tbody><tr>' +
    '<td><p style="font-size:16pt; color:#ff0000; font-family:NanumGothic; text-align:center">WrapText<br></p></td>' +
    '<td><p><b>First</b></p><p><a href="https://example.org/p2" data-eo-link="1">Second</a></p></td>' +
    "</tr></tbody></table>";

  const blocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor, {});
  const tbl = blocks.find((b) => b.type === "table");
  assert.ok(tbl);
  assert.equal(tbl.rows[0][0].text, "WrapText");
  assert.equal(tbl.rows[0][1].text, "First\nSecond");
  assert.deepEqual(tbl.links, [{ row: 0, col: 1, start: 6, end: 12, url: "https://example.org/p2" }]);

  const cp0 = tbl.cellParagraphs.find((c) => c.row === 0 && c.col === 0);
  assert.equal(cp0.paragraphs.length, 1, "trailing <br> inside centered <p> must not create phantom paragraph");
  assert.equal(cp0.paragraphs[0].align, "center");
  assert.equal(cp0.paragraphs[0].runs[0].sizeHalfPt, 32);
  assert.equal(cp0.paragraphs[0].runs[0].color, "FF0000");
  assert.equal(cp0.paragraphs[0].runs[0].font, "NanumGothic");

  // Reopen with cellLinks while paragraphs omit run.link -> buildCellChildrenSpecs preserves link
  const dtRuns = h.host.__EASYOFFICE_DOC_TABLE_RUNS__;
  const built = dtRuns.buildCellChildrenSpecs("First\nSecond", [{ start: 6, end: 12, url: "https://example.org/p2" }], {
    paragraphs: [
      { runs: [{ text: "First", bold: true }] },
      { runs: [{ text: "Second" }] },
    ],
  }, [], {
    textSpec: (v) => ({ t: "text", v }),
    elSpec: (tag, children, style) => ({ t: "el", tag, children: children || [], style: style || {} }),
    runSpecs: (runs) => runs.map((r) => (r.link ? { t: "el", tag: "a", href: r.link, children: [r.text] } : { t: "text", v: r.text })),
    imageSpec: (x) => x,
    cellInlineSpecs: (t) => [{ t: "text", v: t }],
  });
  assert.equal(built.children.length, 2);
  assert.equal(built.children[1].children[0].tag, "a");
  assert.equal(built.children[1].children[0].href, "https://example.org/p2");
});

test("Regression: SLIDE modal dismissal, focus-scoped clipboard, and multi-object Delete + undo", async () => {
  const h = await createWorkspaceHarness();
  await flushAsyncTurns(8);
  switchModule(h.document, "SLIDE");
  await flushAsyncTurns(6);
  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  while (canvas.getElements().length > 0) canvas.deleteElement(canvas.getElements()[0].id);

  const a = canvas.addElement({ kind: "textbox", x: 30, y: 30, w: 90, h: 40, text: "M1" });
  const b = canvas.addElement({ kind: "textbox", x: 160, y: 30, w: 90, h: 40, text: "M2" });
  const c = canvas.addElement({ kind: "textbox", x: 290, y: 30, w: 90, h: 40, text: "M3" });

  // Focus on toolbar button must NOT hijack Ctrl+D or Ctrl+C
  canvas.selectElement(a.id);
  const addBtn = $(h.document, "btn-add-slide");
  addBtn.focus?.();
  dispatchKey(h, { key: "d", code: "KeyD", ctrlKey: true }, addBtn);
  assert.equal(canvas.getElements().length, 3, "toolbar button focus isolates canvas Ctrl+D");

  // Context menu closes when modal opens (via real Ctrl+/ command palette)
  const layer = $(h.document, "slide-canvas-layer");
  const cardA = layer.querySelector(`.slide-el[data-element-id="${a.id}"]`);
  cardA.dispatchEvent({ type: "contextmenu", bubbles: true, cancelable: true, target: cardA, clientX: 80, clientY: 90, preventDefault() {} });
  assert.equal($(h.document, "slide-context-menu").hidden, false);
  dispatchKey(h, { key: "/", code: "Slash", ctrlKey: true }, h.document.body);
  assert.equal($(h.document, "cmd-palette-modal").open, true);
  assert.equal($(h.document, "slide-context-menu").hidden, true, "modal open dismisses slide context menu");
  $(h.document, "cmd-palette-modal").close?.();

  // Multi-select a & b -> Delete key removes BOTH in 1 undo step; Ctrl+Z restores BOTH
  canvas.setSelectedIds([a.id, b.id]);
  const cardB = layer.querySelector(`.slide-el[data-element-id="${b.id}"]`);
  dispatchKey(h, { key: "Delete", code: "Delete" }, cardB);
  assert.deepEqual(canvas.getElements().map((e) => e.id), [c.id], "Delete removes all multi-selected elements");
  dispatchKey(h, { key: "z", code: "KeyZ", ctrlKey: true }, h.document.body);
  await flushAsyncTurns(8);
  assert.equal(canvas.getElements().length, 3, "1-step undo restores multi-deleted elements");
});

test("Regression: PDF repeated query jumps to Page 2 paragraph, Command Palette completeness, BASE warning specificity, BOARD custom-select sync", async () => {
  const h = await createWorkspaceHarness();
  await flushAsyncTurns(10);

  // Command palette includes DOC sup/sub/strike, SLIDE/PDF/BOARD find, BASE import
  const collect = async (mod, q) => {
    switchModule(h.document, mod);
    await flushAsyncTurns(4);
    dispatchKey(h, { key: "/", code: "Slash", ctrlKey: true }, h.document.body);
    const inp = $(h.document, "cmd-search-input");
    inp.value = q;
    inp.dispatchEvent({ type: "input", target: inp });
    const titles = Array.from(h.document.querySelectorAll("#cmd-palette-list li")).map((li) => li.textContent);
    $(h.document, "cmd-palette-modal").close?.();
    return titles.join("|");
  };
  assert.match(await collect("DOC", "위 첨자"), /위 첨자/);
  assert.match(await collect("SLIDE", "찾기"), /찾기/);
  assert.match(await collect("PDF", "찾기"), /찾기/);
  assert.match(await collect("BOARD", "찾기"), /찾기/);
  assert.match(await collect("BASE", "가져오기"), /가져오기/);

  // PDF repeated query across 2 pages highlights Page 2's <p>, not Page 1's <p>
  const pdfDecoded = {
    status: "OK",
    module: "PDF",
    format: "pdf",
    title: "다중페이지.pdf",
    blocks: [{ runs: [{ text: "공통조항 첫째" }] }, { runs: [{ text: "공통조항 둘째" }] }],
    pdfPages: [{ pageNumber: 1, text: "공통조항 첫째" }, { pageNumber: 2, text: "공통조항 둘째" }],
    pdfForm: { hasAcroForm: true, fieldCount: 1, fields: [{ name: "ContractCode", value: "" }] },
    warnings: [],
  };
  const hPdf = await createWorkspaceHarness({
    search: "?module=PDF&win=w-pd357-pdf-reg",
    testIpc: async (cmd, payload) => {
      if (cmd === "convert_document_format" && JSON.parse(payload || "{}").action === "decode") return pdfDecoded;
      return h.host.__EASYOFFICE_TEST_IPC__ ? h.host.__EASYOFFICE_TEST_IPC__(cmd, payload) : {};
    },
  });
  await flushAsyncTurns(8);
  await hPdf.host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("multi2.pdf", new Uint8Array([37, 80, 68, 70, 45]));
  await flushAsyncTurns(6);
  dispatchKey(hPdf, { key: "f", code: "KeyF", ctrlKey: true }, hPdf.document.body);
  const fq = $(hPdf.document, "find-query");
  fq.value = "공통조항";
  fq.dispatchEvent({ type: "input", target: fq });
  await flushAsyncTurns(4);
  $(hPdf.document, "btn-find-next").click();
  await flushAsyncTurns(4);
  assert.equal($(hPdf.document, "pdf-page-indicator").textContent, "쪽 2 / 2");
  const pdfPs = Array.from(hPdf.document.querySelectorAll("#pdf-page-card p:not(#pdf-form-status)"));
  assert.equal(pdfPs[0].classList.contains("eo-search-hit"), false, "Page 1 paragraph must not stay highlighted on Page 2 match");
  assert.equal(pdfPs[1].classList.contains("eo-search-hit"), true, "Page 2 paragraph must be highlighted on Page 2 match");
  $(hPdf.document, "find-dialog").close?.();

  // BASE benign XLSX warning does not trigger false overflow error; leading space formula is neutralized
  const imp = h.host.__EASYOFFICE_BASE_IMPORT__;
  assert.equal(imp.neutralizeFormulaText(" =CMD()"), "' =CMD()");
  const benign = await imp.decodeImportFile("ok.xlsx", new Uint8Array([80, 75, 3, 4]), async () => ({
    warnings: ["Sheet1 시트의 조건부 서식 규칙은 지원하지 않아 가져오지 않았습니다."],
    sheets: [{ name: "Sheet1", cells: [{ ref: "A1", raw: "이름", t: "s" }, { ref: "A2", raw: "값", t: "s" }] }],
  }));
  assert.equal(benign.ok, true, "benign XLSX rule warning must not fail BASE import");
  const over = await imp.decodeImportFile("big.xlsx", new Uint8Array([80, 75, 3, 4]), async () => ({
    warnings: ["편집기 격자(최대 10002행 × 65열) 밖의 셀 5개는 가져오지 않았습니다."],
    sheets: [{ name: "Sheet1", cells: [] }],
  }));
  assert.equal(over.ok, false, "grid overflow warning must still fail closed");

  // BOARD custom-select trigger label syncs across connector/shape + nested contenteditable isolation
  switchModule(h.document, "BOARD");
  await flushAsyncTurns(4);
  const board = h.host.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
  const s1 = board.addNode({ kind: "shape", x: 40, y: 40, w: 100, h: 60, strokeWidthPt: 3, text: "S1" });
  const s2 = board.addNode({ kind: "shape", x: 240, y: 40, w: 100, h: 60, strokeWidthPt: 2, text: "S2" });
  const cn = board.addConnector({ fromId: s1.id, toId: s2.id, label: "연결" });
  board.selectNode(s1.id);
  const wTrigger = h.document.querySelector('[data-select-id="board-style-width"] .custom-select-btn, [data-select-id="board-style-width"]');
  assert.ok(wTrigger?.textContent?.includes("3pt"), "custom select reflects shape strokeWidthPt=3");
  board.selectConnector(cn.id);
  assert.ok(wTrigger?.textContent?.includes("1.5pt"), "custom select syncs when switching to connector");
  const ceHost = h.document.createElement("div");
  ceHost.setAttribute("contenteditable", "true");
  const ceChild = h.document.createElement("span");
  ceHost.appendChild(ceChild);
  h.document.body.appendChild(ceHost);
  const nCount = board.getState().nodes.length;
  dispatchKey(h, { key: "d", code: "KeyD", ctrlKey: true }, ceChild);
  assert.equal(board.getState().nodes.length, nCount, "nested contenteditable isolates BOARD Ctrl+D");
  ceHost.remove();
});

test("Regression: native menu edit:cut/copy and toolbar clip commands preserve BOARD/SLIDE objects while editing contenteditable, and unstyled <br> cell keeps newline", async () => {
  const h = await createWorkspaceHarness();
  await flushAsyncTurns(10);

  // 1. Unstyled <td>Line1<br>Line2</td> keeps \n separator
  switchModule(h.document, "DOC");
  await flushAsyncTurns(4);
  const editor = $(h.document, "doc-editor");
  editor.innerHTML = '<table class="doc-inline-table"><tbody><tr><td>Line1<br>Line2</td><td>Single<br></td></tr></tbody></table>';
  const docBlocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor, {});
  const tblBlock = docBlocks.find((b) => b.type === "table");
  assert.equal(tblBlock.rows[0][0].text, "Line1\nLine2");
  assert.equal(tblBlock.rows[0][1].text, "Single");

  // 2. BOARD contenteditable isolates native edit:cut/copy and boardStyleBar.runClipboardCommand
  switchModule(h.document, "BOARD");
  await flushAsyncTurns(4);
  const board = h.host.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
  const barApi = h.host.__EASYOFFICE_BOARD_STYLE_BAR__;
  const bn1 = board.addNode({ kind: "text", x: 60, y: 60, w: 160, h: 70, text: "HelloWorld" });
  const bn2 = board.addNode({ kind: "shape", x: 300, y: 60, w: 120, h: 70, text: "Target" });
  board.addConnector({ fromId: bn1.id, toId: bn2.id, label: "link" });
  board.selectNode(bn1.id);

  const execCalls = [];
  h.document.execCommand = (name) => { execCalls.push(name); return true; };

  const textNodeEl = h.document.querySelector(`#board-node-layer .board-node[data-node-id="${bn1.id}"] .board-node-text`);
  assert.ok(textNodeEl);
  textNodeEl.setAttribute("contenteditable", "plaintext-only");
  textNodeEl.focus();
  assert.equal(h.document.activeElement, textNodeEl);

  const preNodes = board.getState().nodes.length;
  const preConns = board.getState().connectors.length;
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("edit:cut"), true);
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("edit:copy"), true);
  assert.equal(barApi.runClipboardCommand("clip-cut"), true);
  assert.equal(barApi.runClipboardCommand("clip-copy"), true);
  assert.equal(barApi.runClipboardCommand("clip-paste"), true);
  assert.deepEqual(execCalls, ["cut", "copy", "cut", "copy", "paste"]);
  assert.equal(board.getState().nodes.length, preNodes, "BOARD node must not be deleted while contenteditable text is focused");
  assert.equal(board.getState().connectors.length, preConns, "BOARD connector must not be deleted while contenteditable text is focused");
  textNodeEl.removeAttribute("contenteditable");

  // 3. SLIDE contenteditable (#slide-title-zone, #slide-lower-zone, .slide-el-textbox) isolates native edit:cut/copy and slideClipboard.runMenuClipboard
  switchModule(h.document, "SLIDE");
  await flushAsyncTurns(4);
  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  const slideClip = h.host.__EASYOFFICE_SLIDE_CLIPBOARD__;
  while (canvas.getElements().length > 0) canvas.deleteElement(canvas.getElements()[0].id);
  const sb = canvas.addElement({ kind: "textbox", x: 50, y: 50, w: 180, h: 60, text: "AlphaBeta" });
  canvas.selectElement(sb.id);

  const tbDiv = h.document.querySelector(`#slide-canvas-layer .slide-el[data-element-id="${sb.id}"] .slide-el-textbox`);
  assert.ok(tbDiv);
  tbDiv.setAttribute("contenteditable", "plaintext-only");
  tbDiv.focus();
  execCalls.length = 0;
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("edit:cut"), true);
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("edit:copy"), true);
  assert.equal(slideClip.runMenuClipboard("clip-cut"), true);
  assert.equal(slideClip.runMenuClipboard("clip-copy"), true);
  assert.equal(slideClip.runMenuClipboard("clip-paste"), true);
  assert.deepEqual(execCalls, ["cut", "copy", "cut", "copy", "paste"]);
  assert.equal(canvas.getElements().length, 1, "SLIDE textbox must not be deleted while contenteditable text is focused");
  tbDiv.removeAttribute("contenteditable");

  const titleZone = $(h.document, "slide-title-zone");
  titleZone.focus();
  execCalls.length = 0;
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("edit:cut"), true);
  assert.equal(slideClip.runMenuClipboard("clip-copy"), true);
  assert.deepEqual(execCalls, ["cut", "copy"]);
  assert.equal(canvas.getElements().length, 1, "SLIDE object stays intact while #slide-title-zone is focused");

  // 4. execTextFieldEdit collapsed selection must NOT delete characters, and <input> setRangeText fallback fires input
  const utils = h.host.__EASYOFFICE_WORKSPACE_SESSION_UTILS__;
  const origExec = h.document.execCommand;
  let deleteCalls = 0;
  h.document.execCommand = (name) => {
    if (name === "delete") deleteCalls += 1;
    return false;
  };
  const labelInp = h.document.createElement("input");
  labelInp.type = "text";
  labelInp.value = "LabelText";
  labelInp.setRangeText = (rep, s, e) => {
    labelInp.value = labelInp.value.slice(0, s) + rep + labelInp.value.slice(e);
    labelInp.selectionStart = s + rep.length;
    labelInp.selectionEnd = s + rep.length;
  };
  h.document.body.appendChild(labelInp);
  labelInp.focus();
  labelInp.selectionStart = 5;
  labelInp.selectionEnd = 5;
  assert.equal(utils.execTextFieldEdit("cut", h.document), false);
  assert.equal(labelInp.value, "LabelText", "collapsed cut must not delete char before caret");
  assert.equal(deleteCalls, 0);

  let inputFired = 0;
  labelInp.addEventListener("input", () => { inputFired += 1; });
  labelInp.selectionStart = 5;
  labelInp.selectionEnd = 9;
  const clipOut = [];
  assert.equal(utils.execTextFieldEdit("cut", h.document, (t) => clipOut.push(t)), true);
  assert.deepEqual(clipOut, ["Text"]);
  assert.equal(labelInp.value, "Label");
  assert.equal(inputFired, 1);
  labelInp.remove();
  h.document.execCommand = origExec;

  // 5. Full unstyled <br> table cell roundtrip via buildEditorNodes -> restoreDomChildren -> collectDocBlocks preserves \n and links
  switchModule(h.document, "DOC");
  await flushAsyncTurns(4);
  const { buildEditorNodes } = h.host.__EASYOFFICE_FILE_OPEN_UTILS__;
  const { restoreDomChildren } = h.host.__EASYOFFICE_SHEET_UTILS__;
  const specs = buildEditorNodes([{
    type: "table",
    rows: [[{ text: "Line1\nLine2" }, { text: "A\nB" }]],
    links: [{ row: 0, col: 1, start: 0, end: 3, url: "https://example.org" }],
  }]);
  restoreDomChildren(editor, specs, h.document);
  const reopenedBlocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor, {});
  const reopenedTbl = reopenedBlocks.find((b) => b.type === "table");
  assert.equal(reopenedTbl.rows[0][0].text, "Line1\nLine2");
  assert.equal(reopenedTbl.rows[0][1].text, "A\nB");
  assert.deepEqual(reopenedTbl.links, [
    { row: 0, col: 1, start: 0, end: 1, url: "https://example.org" },
    { row: 0, col: 1, start: 2, end: 3, url: "https://example.org" },
  ]);

  // 6. Post-audit fixes: DOC native menu cross-module & modal/field gates, tool:palette toggle, export focus, BASE icon/dialog, BOARD swatches
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("format:cell-format"), false, "sheet format:cell-format must fail closed in DOC");
  assert.equal(Boolean($(h.document, "sheet-numfmt-dialog")?.open), false);
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("file:save-as-hwpx"), false, "hwp save-as-hwpx must fail closed in DOC");
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("table:cell-select"), false, "hwp editor-only command must fail closed in DOC");
  switchModule(h.document, "SHEET");
  await flushAsyncTurns(4);
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("tool:doc-inspect"), false, "doc-inspect must fail closed in SHEET");
  switchModule(h.document, "HWP");
  await flushAsyncTurns(4);
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("file:print-preview"), false, "print-preview must fail closed in HWP");
  switchModule(h.document, "START");
  await flushAsyncTurns(4);
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("file:print-preview"), false, "print-preview must fail closed in START");
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("tool:doc-inspect"), false, "doc-inspect must fail closed in START");
  switchModule(h.document, "DOC");
  await flushAsyncTurns(4);

  const symDlg = $(h.document, "symbol-picker-dialog");
  symDlg.showModal();
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("tool:palette"), false);
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("file:exp-dlg"), false, "modal-gated file:exp-dlg must fail closed over open modal");
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("format:strike"), false);
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("format:superscript"), false);
  symDlg.close();
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("tool:palette"), true);
  assert.equal($(h.document, "cmd-palette-modal").open, true);
  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("tool:palette"), true);
  assert.equal($(h.document, "cmd-palette-modal").open, false);

  assert.equal(h.host.__EASYOFFICE_NATIVE_MENU__("file:exp-dlg"), true);
  assert.equal(h.document.activeElement, $(h.document, "export-dialog").querySelector(".custom-select-trigger"));
  $(h.document, "export-dialog").close();

  switchModule(h.document, "BASE");
  await flushAsyncTurns(6);
  assert.ok($(h.document, "btn-base-import")?.querySelector(".tb-icon-svg svg"), "#btn-base-import has SVG icon");
  switchModule(h.document, "BOARD");
  await flushAsyncTurns(6);
  const bCtrl = h.host.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
  const bn = bCtrl.addNode({ kind: "shape", x: 40, y: 40, w: 120, h: 60, fill: "DBEAFE", stroke: "2563EB" });
  bCtrl.selectNode(bn.id);
  const fillHost = $(h.document, "board-style-fill-host");
  const fillInp = $(h.document, "board-style-fill");
  const strokeInp = $(h.document, "board-style-stroke");
  assert.equal(fillInp.classList.contains("eo-color-input"), true);
  assert.equal(strokeInp.classList.contains("eo-color-input"), true);
  assert.equal(fillInp.tabIndex, -1);
  assert.equal(fillInp.getAttribute("aria-hidden"), "true");
  assert.equal(fillHost.querySelector(".dialog-swatch-chip")?.style.getPropertyValue("--dialog-swatch").toUpperCase(), "#DBEAFE");

  // Dragging native picker (input only, no change/blur) then switching to swatch click must preserve Undo step
  fillInp.value = "#ff0000";
  fillInp.dispatchEvent({ type: "input", target: fillInp });
  assert.equal(bCtrl.getState().nodes.find((n) => n.id === bn.id).fill, "FF0000");
  fillHost.querySelector(".dialog-swatch").click();
  const swatchHex = (el) => (el.title || el.getAttribute("aria-label") || "").replace("#", "").toUpperCase();
  const nextSwatch = Array.from(fillHost.querySelectorAll(".sb-hl-swatch")).find((el) => /^[0-9A-F]{6}$/.test(swatchHex(el)) && swatchHex(el) !== "FF0000");
  assert.ok(nextSwatch, "palette exposes alternate fill swatch");
  nextSwatch.click();
  const pickedFill = swatchHex(nextSwatch);
  assert.equal(bCtrl.getState().nodes.find((n) => n.id === bn.id).fill, pickedFill);
  dispatchKey(h, { key: "z", code: "KeyZ", ctrlKey: true }, h.document.body);
  await flushAsyncTurns(6);
  assert.equal(bCtrl.getState().nodes.find((n) => n.id === bn.id).fill, "FF0000", "undo must step back to prior native drag color instead of skipping");
});
