import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem, switchModule } from "./test-support/modules.mjs";
import { build, createSelection, p, rangeOf } from "./test-support/doc-run-harness.mjs";
import { readText } from "./test-support/read-text.mjs";
import { itemTable } from "./test-support/rust-menu-table.mjs";

const REPO = new URL("../", import.meta.url);
const read = (rel) => readText(fileURLToPath(new URL(rel, REPO)));

// Builds a minimal structurally valid PNG (magic + IHDR + IDAT + IEND) with exact pixel dimensions.
function makePngBytes(width, height) {
  const out = new Uint8Array(57);
  const view = new DataView(out.buffer);
  out.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], 0);
  view.setUint32(8, 13, false);
  out.set([0x49, 0x48, 0x44, 0x52], 12); // IHDR
  view.setUint32(16, width, false);
  view.setUint32(20, height, false);
  out.set([8, 2, 0, 0, 0], 24);
  view.setUint32(29, 0, false); // CRC placeholder
  view.setUint32(33, 0, false);
  out.set([0x49, 0x44, 0x41, 0x54], 37); // IDAT
  view.setUint32(41, 0, false);
  view.setUint32(45, 0, false);
  out.set([0x49, 0x45, 0x4e, 0x44], 49); // IEND
  view.setUint32(53, 0xae426082, false);
  return out;
}

// Builds a minimal valid JPEG header (SOI + APP0 + SOF0 + EOI) with exact pixel dimensions.
function makeJpegBytes(width, height) {
  const out = new Uint8Array([
    0xff, 0xd8,
    0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00,
    0xff, 0xc0, 0x00, 0x0b, 0x08, (height >> 8) & 0xff, height & 0xff, (width >> 8) & 0xff, width & 0xff, 0x01, 0x01, 0x11, 0x00,
    0xff, 0xd9,
  ]);
  return out;
}

function rustItems() {
  const source = read("crates/easyoffice-desktop/src/native_menu/items.rs");
  const table = itemTable(source);
  return [...table.matchAll(/item\(\s*"([^"]+)",\s*"([^"]+)",\s*(None|Some\("([^"]+)"\)),\s*"([^"]+)",?\s*\)/g)]
    .map((m) => ({ id: m[1], label: m[2], accelerator: m[4] ?? null, action: m[5] }));
}

const rowSpec = (...cells) => ({
  tag: "tr",
  children: cells.map((text) => ({ tag: "td", children: text === "" ? [{ tag: "br" }] : [text] })),
});

test("toolbar, menus, native menu and dialogs expose verbatim Hangul terms for 그림 and 표 cell styling", async () => {
  const h = await createWorkspaceHarness({ search: "?module=DOC&win=w-pd251-chrome" });
  await flushAsyncTurns(10);
  const { document } = h;

  const tbBtn = document.querySelector('#icon-toolbar .tb-btn[data-studio-cmd="insert-image"]');
  assert.ok(tbBtn, "그림 toolbar button exists");
  assert.equal(tbBtn.hidden, false);
  assert.equal(tbBtn.getAttribute("title"), "그림");
  assert.equal(tbBtn.getAttribute("aria-label"), "그림");
  assert.equal(tbBtn.querySelector(".tb-label")?.textContent.trim(), "그림");
  assert.ok(tbBtn.querySelector(".tb-sprite.icon-image"), "uses .icon-image sprite");

  const insertItem = studioItem(document, "insert-image");
  assert.ok(insertItem, "입력 > 그림 menu item exists");
  assert.equal(insertItem.querySelector(".md-label")?.textContent, "그림");
  assert.equal(insertItem.hidden, false);

  for (const [cmd, label] of [
    ["table-border-bg", "셀 테두리/배경"],
    ["table-cell-props", "표/셀 속성"],
    ["table-col-equal", "셀 너비를 같게"],
  ]) {
    const item = studioItem(document, cmd);
    assert.ok(item, `${cmd} menu item exists`);
    assert.equal(item.querySelector(".md-label")?.textContent, label);
    assert.equal(item.hidden, false);
    assert.equal(item.getAttribute("aria-disabled"), "true", `${cmd} disabled outside table`);
  }

  const rust = new Map(rustItems().map((it) => [it.id, it]));
  assert.deepEqual(
    ["insert:image", "table:border-bg", "table:cell-props", "table:col-equal"].map((id) => [rust.get(id)?.label, rust.get(id)?.action]),
    [
      ["그림", "studio-cmd:insert-image"],
      ["셀 테두리/배경", "studio-cmd:table-border-bg"],
      ["표/셀 속성", "studio-cmd:table-cell-props"],
      ["셀 너비를 같게", "studio-cmd:table-col-equal"],
    ],
  );

  const imgDlg = document.getElementById("doc-image-size-dialog");
  assert.equal(imgDlg.querySelector(".dialog-title span")?.textContent, "그림");
  assert.equal(imgDlg.querySelector(".dialog-section-title")?.textContent, "크기");
  assert.deepEqual([...imgDlg.querySelectorAll(".dialog-label")].map((el) => el.textContent).filter(Boolean), ["너비", "높이"]);
  assert.equal(imgDlg.querySelector(".dialog-checkbox")?.textContent.trim(), "비율 유지");
  assert.equal(document.getElementById("btn-doc-image-reset-size")?.textContent, "원래 크기로");
  assert.deepEqual([...imgDlg.querySelectorAll(".dialog-footer .dialog-btn")].map((b) => b.textContent), ["확인", "취소"]);

  const borderDlg = document.getElementById("doc-cell-border-dialog");
  assert.equal(borderDlg.querySelector(".dialog-title span")?.textContent, "셀 테두리/배경");
  assert.deepEqual([...borderDlg.querySelectorAll(".dialog-tab")].map((b) => b.textContent), ["테두리", "배경"]);
  assert.deepEqual(
    [...borderDlg.querySelectorAll("[data-border-line]")].map((b) => b.getAttribute("title")),
    ["가는 실선", "굵은 실선", "파선", "점선", "이중 실선"],
  );
  assert.deepEqual(
    [...borderDlg.querySelectorAll("[data-border-preset]")].map((b) => b.textContent),
    ["없음", "바깥쪽", "안쪽", "모두"],
  );
  assert.deepEqual(
    [...borderDlg.querySelectorAll("[data-border-pos]")].map((b) => b.textContent),
    ["위쪽", "아래쪽", "왼쪽", "오른쪽", "가로 중심선", "세로 중심선"],
  );
  assert.deepEqual(
    [...borderDlg.querySelectorAll("#doc-cb-pane-bg .dialog-checkbox")].map((l) => l.textContent.trim()),
    ["채우기 없음", "면색"],
  );

  const propsDlg = document.getElementById("doc-table-props-dialog");
  assert.equal(propsDlg.querySelector(".dialog-title span")?.textContent, "표/셀 속성");
  assert.equal(propsDlg.querySelector(".dialog-section-title")?.textContent, "셀 크기");
  assert.equal(propsDlg.querySelector("label.dialog-label")?.textContent, "너비");
  assert.deepEqual([...propsDlg.querySelectorAll(".dialog-footer .dialog-btn")].map((b) => b.textContent), ["확인", "취소"]);
});

test("image insert validates PNG/JPEG headers, scales oversized images, places in body or table cell, resizes, deletes and round-trips Undo/Redo", async () => {
  const selection = createSelection();
  const h = await createWorkspaceHarness({ selection, search: "?module=DOC&win=w-pd251-img" });
  await flushAsyncTurns(10);
  const { document, host, sandbox, userClick } = h;
  const editor = document.getElementById("doc-editor");
  editor.replaceChildren();
  const para = build(document, editor, p("본문 첫 줄"));
  const table = build(document, editor, { tag: "table", children: [{ tag: "tbody", children: [rowSpec("", "우측")] }] });
  table.className = "doc-inline-table";
  build(document, editor, p("본문 끝 줄"));

  const api = host.__EASYOFFICE_DOC_IMAGE_API__;
  assert.equal(typeof api?.insertImageBytes, "function");

  // Corrupt / non-image bytes are rejected without mutating the DOM.
  const beforeHtml = editor.innerHTML;
  const bad1 = api.insertImageBytes(new Uint8Array([0x00, 0x11, 0x22, 0x33]), "bad.png");
  assert.equal(bad1.changed, false);
  const bad2 = api.insertImageBytes(makePngBytes(0, 100), "zero.png");
  assert.equal(bad2.changed, false);
  assert.equal(editor.innerHTML, beforeHtml, "corrupt image input leaves editor DOM untouched");
  assert.match(document.getElementById("status-command-feedback").textContent, /그림/);

  // Insert a 120x60 PNG after the first paragraph.
  selection.addRange(rangeOf(para, 0, para, 0));
  const okBody = api.insertImageBytes(makePngBytes(120, 60), "차트.png");
  assert.equal(okBody.changed, true);
  const bodyImg = editor.querySelector(":scope > img.doc-inline-image");
  assert.ok(bodyImg, "image inserted at top level after caret block");
  assert.equal(bodyImg.getAttribute("src"), "blob:easyoffice-test");
  assert.equal(bodyImg.dataset.eoImageFormat, "png");
  assert.equal(bodyImg.dataset.eoImageWidthEmu, String(120 * 9525));
  assert.equal(bodyImg.dataset.eoImageHeightEmu, String(60 * 9525));
  assert.equal(bodyImg.style.width, "120px");
  assert.equal(bodyImg.style.height, "60px");

  // Insert an oversized 800x400 JPEG inside the first table cell (scaled down proportionally to <= 2_286_000 EMU).
  const firstTd = table.querySelector("td");
  selection.addRange(rangeOf(firstTd, 0, firstTd, 0));
  const okCell = api.insertImageBytes(makeJpegBytes(800, 400), "사진.jpg");
  assert.equal(okCell.changed, true);
  assert.equal(firstTd.querySelector("br"), null, "placeholder <br> removed when cell gets image");
  const cellImg = firstTd.querySelector("img.doc-inline-image");
  assert.ok(cellImg, "image inserted inside td");
  assert.equal(cellImg.dataset.eoImageFormat, "jpg");
  assert.equal(Number(cellImg.dataset.eoImageWidthEmu), 2286000);
  assert.equal(Number(cellImg.dataset.eoImageHeightEmu), 1143000);

  // Resizer lives in .doc-scroll-container outside #doc-editor.
  const resizer = document.getElementById("doc-image-resizer");
  assert.equal(editor.contains(resizer), false);
  assert.equal(resizer.closest(".doc-scroll-container") !== null, true);
  bodyImg.dispatchEvent({ type: "click", target: bodyImg });
  assert.equal(resizer.hidden, false);

  // Dragging corner handle `se` preserves 2:1 aspect ratio as a single Undo step.
  const seHandle = resizer.querySelector('[data-resize-dir="se"]');
  seHandle.dispatchEvent({ type: "pointerdown", target: seHandle, clientX: 100, clientY: 100 });
  document.dispatchEvent({ type: "pointermove", clientX: 180, clientY: 160 });
  document.dispatchEvent({ type: "pointerup", clientX: 180, clientY: 160 });
  assert.equal(bodyImg.style.width, "200px");
  assert.equal(bodyImg.style.height, "100px");
  assert.equal(bodyImg.dataset.eoImageWidthEmu, String(200 * 9525));
  assert.equal(bodyImg.dataset.eoImageHeightEmu, String(100 * 9525));

  // Double-click opens #doc-image-size-dialog; changing width with 비율 유지 updates height in mm (1 mm = 36000 EMU).
  bodyImg.dispatchEvent({ type: "dblclick", target: bodyImg });
  const imgDlg = document.getElementById("doc-image-size-dialog");
  assert.equal(imgDlg.open, true);
  const wInput = document.getElementById("doc-image-width-mm");
  const hInput = document.getElementById("doc-image-height-mm");
  assert.equal(document.getElementById("doc-image-keep-ratio").checked, true);
  wInput.value = "80";
  wInput.dispatchEvent({ type: "input", target: wInput });
  assert.equal(hInput.value, "40");
  userClick(document.getElementById("btn-doc-image-size-ok"));
  await flushAsyncTurns(6);
  assert.equal(imgDlg.open, false);
  assert.equal(editor.querySelector(":scope > img.doc-inline-image").dataset.eoImageWidthEmu, String(80 * 36000));
  assert.equal(editor.querySelector(":scope > img.doc-inline-image").dataset.eoImageHeightEmu, String(40 * 36000));

  // Verify savePayload.collectDocBlocks emits widthEmu/heightEmu on body image and cellImages on table.
  const blocks = host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor);
  const savedImg = blocks.find((b) => b.type === "image");
  assert.equal(savedImg.widthEmu, 80 * 36000);
  assert.equal(savedImg.heightEmu, 40 * 36000);
  const savedTbl = blocks.find((b) => b.type === "table");
  assert.equal(savedTbl.cellImages.length, 1);
  assert.deepEqual(
    { row: savedTbl.cellImages[0].row, col: savedTbl.cellImages[0].col, format: savedTbl.cellImages[0].format, widthEmu: savedTbl.cellImages[0].widthEmu, heightEmu: savedTbl.cellImages[0].heightEmu },
    { row: 0, col: 0, format: "jpg", widthEmu: 2286000, heightEmu: 1143000 },
  );

  // Undo reverts dialog resize back to 200px x 100px; Redo reapplies 80mm x 40mm.
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(20);
  assert.equal(editor.querySelector(":scope > img.doc-inline-image").dataset.eoImageWidthEmu, String(200 * 9525));
  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(20);
  assert.equal(editor.querySelector(":scope > img.doc-inline-image").dataset.eoImageWidthEmu, String(80 * 36000));

  // Select body image and press Delete -> 개체 지우기 완료 in one Undo step.
  const liveBodyImg = editor.querySelector(":scope > img.doc-inline-image");
  liveBodyImg.dispatchEvent({ type: "click", target: liveBodyImg });
  assert.equal(resizer.hidden, false);
  document.dispatchEvent({ type: "keydown", key: "Delete", target: editor, preventDefault() {}, stopPropagation() {} });
  assert.equal(editor.querySelector(":scope > img.doc-inline-image"), null);
  assert.equal(resizer.hidden, true);
  assert.equal(document.getElementById("status-command-feedback").textContent, "개체 지우기 완료");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(20);
  assert.ok(editor.querySelector(":scope > img.doc-inline-image"), "Undo restores deleted image");
});

test("table cell border/background, column width dialog, equal column widths,savePayload and file-open round-trip cleanly", async () => {
  const selection = createSelection();
  let decodedPayload = null;
  const h = await createWorkspaceHarness({
    selection,
    search: "?module=DOC&win=w-pd251-tbl",
    testIpc: async (command, payload) => {
      if (command === "convert_document_format") {
        const req = JSON.parse(payload);
        if (req.action === "decode") return decodedPayload;
      }
      if (command === "get_workspace_bootstrap") return { platform: "macOS", shortcutModifier: "⌘", openTabs: [] };
      return { status: "OK" };
    },
  });
  await flushAsyncTurns(10);
  const { document, host, sandbox, userClick } = h;
  switchModule(document, "DOC");
  await flushAsyncTurns(10);

  const editor = document.getElementById("doc-editor");
  editor.replaceChildren();
  const table = build(document, editor, {
    tag: "table",
    children: [{ tag: "tbody", children: [rowSpec("항목", "수량", "비고"), rowSpec("사과", "10", "국산")] }],
  });
  table.className = "doc-inline-table";
  build(document, editor, p("끝"));

  // Plain table emits NO colWidthsTwips, cellStyles, or cellImages.
  const plainTbl = host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor).find((b) => b.type === "table");
  assert.equal("colWidthsTwips" in plainTbl, false);
  assert.equal("cellStyles" in plainTbl, false);
  assert.equal("cellImages" in plainTbl, false);

  const cellAt = (r, c) => table.querySelectorAll("tr")[r].children[c];
  const selectCells = (r1, c1, r2 = r1, c2 = c1) => {
    selection.addRange(rangeOf(cellAt(r1, c1), 0, cellAt(r2, c2), 0));
    document.querySelector('.menu-item[data-menu="table"]').dispatchEvent({ type: "mouseenter" });
  };

  // 1. Apply 표/셀 속성 width 50mm to column 0, and 30mm to column 1.
  selectCells(0, 0);
  userClick(studioItem(document, "table-cell-props"));
  await flushAsyncTurns(4);
  const propsDlg = document.getElementById("doc-table-props-dialog");
  assert.equal(propsDlg.open, true);
  document.getElementById("doc-table-col-width").value = "50";
  userClick(document.getElementById("btn-doc-table-props-ok"));
  await flushAsyncTurns(6);
  assert.equal(propsDlg.open, false);

  selectCells(0, 1);
  userClick(studioItem(document, "table-cell-props"));
  await flushAsyncTurns(4);
  document.getElementById("doc-table-col-width").value = "30";
  userClick(document.getElementById("btn-doc-table-props-ok"));
  await flushAsyncTurns(6);

  const colsAfterProps = [...table.querySelectorAll("colgroup col")].map((c) => Number(c.dataset.eoColTwips));
  assert.equal(colsAfterProps.length, 3);
  assert.equal(colsAfterProps[0], Math.round(50 * (1440 / 25.4)));
  assert.equal(colsAfterProps[1], Math.round(30 * (1440 / 25.4)));
  assert.equal(table.style.tableLayout, "fixed");

  // 2. 셀 너비를 같게 across columns 0..1 equalizes those two columns in one Undo step.
  selectCells(0, 0, 0, 1);
  userClick(studioItem(document, "table-col-equal"));
  await flushAsyncTurns(6);
  const colsEqual = [...table.querySelectorAll("colgroup col")].map((c) => Number(c.dataset.eoColTwips));
  assert.equal(colsEqual[0], colsEqual[1]);
  assert.notEqual(colsEqual[0], colsAfterProps[0]);

  // Undo reverts 셀 너비를 같게, Redo reapplies it.
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(20);
  const colsUndone = [...editor.querySelectorAll("table colgroup col")].map((c) => Number(c.dataset.eoColTwips));
  assert.equal(colsUndone[0], colsAfterProps[0]);
  assert.equal(colsUndone[1], colsAfterProps[1]);
  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(20);

  // 3. Apply 셀 테두리/배경 on header row (0,0)..(0,2): 없음 preset + 이중 실선 on top & bottom + 면색 #FFF2CC.
  const liveTable = () => editor.querySelector("table.doc-inline-table");
  const liveCell = (r, c) => liveTable().querySelectorAll("tr")[r].children[c];
  selection.addRange(rangeOf(liveCell(0, 0), 0, liveCell(0, 2), 0));
  document.querySelector('.menu-item[data-menu="table"]').dispatchEvent({ type: "mouseenter" });
  userClick(studioItem(document, "table-border-bg"));
  await flushAsyncTurns(4);
  const borderDlg = document.getElementById("doc-cell-border-dialog");
  assert.equal(borderDlg.open, true);

  userClick(borderDlg.querySelector('[data-border-preset="none"]'));
  userClick(borderDlg.querySelector('[data-border-line="double"]'));
  const borderColorInput = document.getElementById("doc-cell-border-color");
  borderColorInput.value = "#1f4e79";
  borderColorInput.dispatchEvent({ type: "change", target: borderColorInput });
  userClick(borderDlg.querySelector('[data-border-pos="t"]'));
  userClick(borderDlg.querySelector('[data-border-pos="b"]'));

  userClick(document.getElementById("doc-cb-tab-bg"));
  const bgColorInput = document.getElementById("doc-cell-bg-color");
  bgColorInput.value = "#fff2cc";
  bgColorInput.dispatchEvent({ type: "change", target: bgColorInput });
  assert.equal(document.getElementById("doc-cell-bg-solid").checked, true);

  userClick(document.getElementById("btn-doc-cell-border-ok"));
  await flushAsyncTurns(6);
  assert.equal(borderDlg.open, false);

  assert.equal(liveCell(0, 0).dataset.eoFill, "FFF2CC");
  assert.equal(liveCell(0, 0).dataset.eoBorders, "double:1F4E79,none,double:1F4E79,none");
  assert.equal(liveCell(0, 0).style.borderTop, "3px double #1F4E79");
  assert.equal(liveCell(0, 0).style.borderLeft, "none");
  // Shared neighbor edge synchronization: row 1's top matches row 0's bottom!
  assert.equal(liveCell(1, 0).style.borderTop, "3px double #1F4E79");

  // Also insert an inline PNG into cell (1, 2).
  selection.addRange(rangeOf(liveCell(1, 2), 0, liveCell(1, 2), 0));
  host.__EASYOFFICE_DOC_IMAGE_API__.insertImageBytes(makePngBytes(48, 24), "배지.png");

  const savedBlocks = host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor);
  const tblBlock = savedBlocks.find((b) => b.type === "table");
  assert.equal(tblBlock.colWidthsTwips.length, 3);
  assert.equal(tblBlock.colWidthsTwips[0], colsEqual[0]);
  const style00 = tblBlock.cellStyles.find((s) => s.row === 0 && s.col === 0);
  assert.deepEqual(style00, {
    row: 0,
    col: 0,
    fill: "FFF2CC",
    borders: { top: "double:1F4E79", right: "none", bottom: "double:1F4E79", left: "none" },
  });
  assert.equal(tblBlock.cellImages.length, 1);
  assert.equal(tblBlock.cellImages[0].row, 1);
  assert.equal(tblBlock.cellImages[0].col, 2);

  // 4. Reopen decoded model via file-open (`openDocumentFromBytes`) using backend `{ kind, color }` border objects and verify full DOM reconstruction!
  const backendDecodedBlocks = structuredClone(savedBlocks);
  const backendTbl = backendDecodedBlocks.find((b) => b.type === "table");
  const backendS00 = backendTbl.cellStyles.find((s) => s.row === 0 && s.col === 0);
  backendS00.borders = {
    top: { kind: "double", color: "1F4E79" },
    right: { kind: "none" },
    bottom: { kind: "double", color: "1F4E79" },
    left: { kind: "none" },
  };
  decodedPayload = {
    status: "OK",
    module: "DOC",
    format: "docx",
    kind: "document",
    itemCount: backendDecodedBlocks.length,
    blocks: backendDecodedBlocks,
  };
  await host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("복원검증.docx", new Uint8Array([0x50, 0x4b, 0x03, 0x04]));
  await flushAsyncTurns(20);

  const reopenedTbl = editor.querySelector("table.doc-inline-table");
  assert.ok(reopenedTbl, "reopened table exists");
  assert.equal(reopenedTbl.style.tableLayout, "fixed");
  assert.deepEqual(
    [...reopenedTbl.querySelectorAll("colgroup col")].map((c) => Number(c.dataset.eoColTwips)),
    tblBlock.colWidthsTwips,
  );
  const reopened00 = reopenedTbl.querySelectorAll("tr")[0].children[0];
  assert.equal(reopened00.dataset.eoFill, "FFF2CC");
  assert.equal(reopened00.style.backgroundColor, "#FFF2CC");
  assert.equal(reopened00.style.borderTop, "3px double #1F4E79");
  assert.equal(reopened00.style.borderLeft, "none");
  const reopenedCellImg = reopenedTbl.querySelectorAll("tr")[1].children[2].querySelector("img.doc-inline-image");
  assert.ok(reopenedCellImg, "nested cell image reconstructed");
  assert.equal(reopenedCellImg.getAttribute("src"), "blob:easyoffice-test");
  assert.equal(reopenedCellImg.style.width, "48px");
  assert.equal(reopenedCellImg.style.height, "24px");
});

test("adversarial inputs, 원래 크기로, column border drag, all-column equalize and colgroup structure edits stay consistent", async () => {
  const selection = createSelection();
  const h = await createWorkspaceHarness({ selection, search: "?module=DOC&win=w-pd251-edges" });
  await flushAsyncTurns(10);
  const { document, host, sandbox, userClick } = h;
  const editor = document.getElementById("doc-editor");
  editor.replaceChildren();
  const p1 = build(document, editor, p("첫 문단"));
  const table = build(document, editor, {
    tag: "table",
    children: [{ tag: "tbody", children: [rowSpec("A", "B", "C"), rowSpec("1", "2", "3")] }],
  });
  table.className = "doc-inline-table";

  const api = host.__EASYOFFICE_DOC_IMAGE_API__;

  // Reject > 32 MiB image and truncated JPEG without mutating DOM.
  const before = editor.innerHTML;
  const huge = { length: 32 * 1024 * 1024 + 1 };
  assert.equal(api.insertImageBytes(huge, "huge.png").changed, false);
  assert.equal(api.insertImageBytes(new Uint8Array([0xff, 0xd8, 0xff, 0xc0, 0x00]), "trunc.jpg").changed, false);
  assert.equal(editor.innerHTML, before);

  // File input change workflow + edge handle `e` clamp + 원래 크기로 button.
  selection.addRange(rangeOf(p1, 0, p1, 0));
  const fileInput = document.getElementById("doc-image-input");
  const pngBytes = makePngBytes(100, 50);
  fileInput.files = [{ name: "원본.png", arrayBuffer: async () => pngBytes.buffer }];
  fileInput.dispatchEvent({ type: "change", target: fileInput });
  await flushAsyncTurns(10);
  const img = editor.querySelector(":scope > img.doc-inline-image");
  assert.ok(img);
  assert.equal(img.style.width, "100px");

  // Edge `e` drag clamps minimum width to 16px and leaves height unchanged.
  const resizer = document.getElementById("doc-image-resizer");
  const eHandle = resizer.querySelector('[data-resize-dir="e"]');
  eHandle.dispatchEvent({ type: "pointerdown", target: eHandle, clientX: 200, clientY: 50 });
  document.dispatchEvent({ type: "pointermove", clientX: 10, clientY: 50 });
  document.dispatchEvent({ type: "pointerup", clientX: 10, clientY: 50 });
  assert.equal(img.style.width, "16px");
  assert.equal(img.style.height, "50px");

  // 원래 크기로 restores 100x50 px natural size.
  img.dispatchEvent({ type: "dblclick", target: img });
  userClick(document.getElementById("btn-doc-image-reset-size"));
  userClick(document.getElementById("btn-doc-image-size-ok"));
  await flushAsyncTurns(6);
  assert.equal(img.dataset.eoImageWidthEmu, String(100 * 9525));
  assert.equal(img.dataset.eoImageHeightEmu, String(50 * 9525));

  // Direct column-border drag within ±5px of cell (0,0) right border (640px in harness) materializes <colgroup> and clamps min 24px / 360 twips.
  const td00 = table.querySelectorAll("tr")[0].children[0];
  td00.dispatchEvent({ type: "pointerdown", target: td00, button: 0, clientX: 638, clientY: 10 });
  document.dispatchEvent({ type: "pointermove", clientX: 200, clientY: 10 });
  document.dispatchEvent({ type: "pointerup", clientX: 200, clientY: 10 });
  const colsDragged = [...table.querySelectorAll("colgroup col")].map((c) => Number(c.dataset.eoColTwips));
  assert.equal(colsDragged.length, 3);
  assert.equal(colsDragged[0], 360);

  // Single-column selection (c1 === c2) 셀 너비를 같게 equalizes ALL 3 columns of the table!
  selection.addRange(rangeOf(td00, 0, td00, 0));
  document.querySelector('.menu-item[data-menu="table"]').dispatchEvent({ type: "mouseenter" });
  userClick(studioItem(document, "table-col-equal"));
  await flushAsyncTurns(6);
  const allEqual = [...table.querySelectorAll("colgroup col")].map((c) => Number(c.dataset.eoColTwips));
  assert.equal(new Set(allEqual).size, 1);

  // insertColumn and deleteColumns keep <colgroup> synchronized.
  selection.addRange(rangeOf(td00, 0, td00, 0));
  userClick(studioItem(document, "table-col-right"));
  await flushAsyncTurns(6);
  assert.equal(table.querySelectorAll("colgroup col").length, 4);
  userClick(studioItem(document, "table-col-delete"));
  await flushAsyncTurns(6);
  assert.equal(table.querySelectorAll("colgroup col").length, 3);

  // 채우기 없음 clears existing eoFill.
  td00.dataset.eoFill = "AABBCC";
  td00.style.backgroundColor = "#AABBCC";
  selection.addRange(rangeOf(td00, 0, td00, 0));
  userClick(studioItem(document, "table-border-bg"));
  await flushAsyncTurns(4);
  userClick(document.getElementById("doc-cb-tab-bg"));
  const noneRadio = document.getElementById("doc-cell-bg-none");
  noneRadio.checked = true;
  noneRadio.dispatchEvent({ type: "change", target: noneRadio });
  userClick(document.getElementById("btn-doc-cell-border-ok"));
  await flushAsyncTurns(6);
  assert.equal(td00.dataset.eoFill, undefined);
  assert.equal(td00.style.backgroundColor, "");

  // Command palette in DOC includes 그림 and 표 cell style items.
  userClick(document.getElementById("btn-command-palette"));
  const search = document.getElementById("cmd-search-input");
  for (const query of ["그림", "셀 테두리/배경", "표/셀 속성", "셀 너비를 같게"]) {
    search.value = query;
    search.dispatchEvent({ type: "input", target: search });
    const labels = [...document.querySelectorAll("#cmd-palette-list .cp-item-label")].map((el) => el.textContent);
    assert.ok(labels.includes(query), `palette includes ${query}`);
  }
  sandbox.dispatchWindowKeydown({ key: "Escape" });
});
