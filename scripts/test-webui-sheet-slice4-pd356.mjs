import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { fakeFilterEngine } from "./test-support/fake-filter-engine.mjs";

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const TC = loadUiModule(UI("sheet-text-columns.js"), {});
const MR = loadUiModule(UI("sheet-multi-range.js"), {});
const SZ = loadUiModule(UI("sheet-size-dialogs.js"), {});

async function openSheet(model) {
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
  const filterMock = fakeFilterEngine();
  const h = await createWorkspaceHarness({
    testIpc: (cmd, p) => {
      const body = typeof p === "string" && p.startsWith("{") ? JSON.parse(p) : null;
      if (body?.sessionAction === "filter") return filterMock(body);
      return offline[cmd](p);
    },
  });
  switchModule(h.document, "SHEET");
  await flushAsyncTurns(6);
  if (model) {
    await pickFile(h.document, makeFixtureFile("slice4.xlsx", model));
    await flushAsyncTurns(10);
  }
  return h;
}

const byId = (h, id) => h.document.getElementById(id);
const cell = (h, ref) => h.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, bubbles: true, ...extra });
const undo = async (h) => {
  h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(14);
};

test("splitDelimitedText handles delimiters, collapse, and bounded limits safely", () => {
  const split = TC.__EASYOFFICE_SPLIT_DELIMITED_TEXT__;
  assert.deepEqual(split("a\tb;c,d e|f", { tab: true, semicolon: true, comma: true, space: true, other: true, otherChar: "|" }).cols, ["a", "b", "c", "d", "e", "f"]);
  assert.deepEqual(split("  alpha   beta  ", { space: true, collapse: true }).cols, ["alpha", "beta"]);
  assert.deepEqual(split("a,,b", { comma: true, collapse: false }).cols, ["a", "", "b"]);
  assert.equal(split(",".repeat(70), { comma: true }).refused, "cols");
  assert.equal(split("x".repeat(9000), { comma: true }).refused, "line");
});

test("multi-range helpers deduplicate rectangles, coords, and cap at MAX_MULTI_RANGES", () => {
  const m = MR.__EASYOFFICE_SHEET_MULTI_RANGE__;
  const list = m.rangesOf({ r1: 1, c1: 1, r2: 2, c2: 2 }, [{ r1: 2, c1: 2, r2: 3, c2: 3 }, { r1: 1, c1: 1, r2: 2, c2: 2 }]);
  assert.equal(list.length, 2);
  let count = 0;
  m.forEachUniqueCoord(list, (r, c) => r * 16385 + c, () => { count += 1; });
  assert.equal(count, 7, "overlapping cell (2,2) visited once");
  assert.equal(m.MAX_MULTI_RANGES, 64);
  const cols = SZ.__EASYOFFICE_SHEET_COL_UNITS__;
  const px = cols.charsToPx(12);
  assert.ok(px > 72 && Math.abs(cols.pxToXlsxWidth(px) - 12) <= 0.25);
});

test("shortcuts with IME physical ev.code, stamps, AutoSum toolbar button, Ctrl/Shift+Space, Ctrl+Shift+L, and batch Ctrl+Enter", async () => {
  const h = await openSheet({
    sheets: [{
      name: "시트1",
      cells: [
        { ref: "A1", raw: "10" }, { ref: "A2", raw: "20" },
        { ref: "B1", raw: "원본" }, { ref: "C1", raw: "5" },
      ],
    }],
  });

  // Alt+= even with Option glyph key '≠' & toolbar Σ button
  cell(h, "A3").focus();
  await flushAsyncTurns(4);
  key(cell(h, "A3"), "≠", { code: "Equal", altKey: true });
  await flushAsyncTurns(16);
  assert.equal(cell(h, "A3").textContent, "30");

  byId(h, "A4")?.focus?.();
  cell(h, "C2").focus();
  await flushAsyncTurns(4);
  const autosumBtn = byId(h, "btn-sheet-autosum");
  assert.ok(autosumBtn, "visible toolbar Σ button exists");
  h.userClick(autosumBtn);
  await flushAsyncTurns(16);
  assert.equal(cell(h, "C2").textContent, "5");

  // Ctrl+D & Ctrl+R using Korean IME glyph keys 'ㅇ'/'ㄱ' with ev.code
  cell(h, "B2").focus();
  await flushAsyncTurns(4);
  key(cell(h, "B2"), "ㅇ", { code: "KeyD", ctrlKey: true });
  await flushAsyncTurns(14);
  assert.equal(cell(h, "B2").textContent, "원본");
  await undo(h);
  assert.equal(cell(h, "B2").textContent, "");

  cell(h, "D1").focus();
  await flushAsyncTurns(4);
  key(cell(h, "D1"), "ㄱ", { code: "KeyR", ctrlKey: true });
  await flushAsyncTurns(14);
  assert.equal(cell(h, "D1").textContent, "5");

  // Ctrl+; & Ctrl+Shift+; local date/time stamps (1 undo step each), plus IME composition guard
  cell(h, "E1").focus();
  await flushAsyncTurns(4);
  key(cell(h, "E1"), ";", { code: "Semicolon", ctrlKey: true, isComposing: true });
  await flushAsyncTurns(6);
  assert.equal(cell(h, "E1").textContent, "", "IME composing skips shortcut");

  key(cell(h, "E1"), ";", { code: "Semicolon", ctrlKey: true });
  await flushAsyncTurns(14);
  assert.match(cell(h, "E1").textContent, /^\d{4}-\d{2}-\d{2}$/);
  await undo(h);
  assert.equal(cell(h, "E1").textContent, "");

  cell(h, "E2").focus();
  await flushAsyncTurns(4);
  key(cell(h, "E2"), ":", { code: "Semicolon", ctrlKey: true, shiftKey: true });
  await flushAsyncTurns(14);
  assert.match(cell(h, "E2").textContent, /^\d{2}:\d{2}$/);

  // Ctrl+Space & Shift+Space full column/row selection
  cell(h, "B3").focus();
  await flushAsyncTurns(4);
  key(cell(h, "B3"), " ", { code: "Space", ctrlKey: true });
  await flushAsyncTurns(6);
  assert.equal(cell(h, "B1").classList.contains("in-range"), true);
  assert.equal(cell(h, "A3").classList.contains("in-range"), false);

  cell(h, "B3").focus();
  await flushAsyncTurns(4);
  key(cell(h, "B3"), " ", { code: "Space", shiftKey: true });
  await flushAsyncTurns(6);
  assert.equal(cell(h, "A3").classList.contains("in-range"), true);
  assert.equal(cell(h, "D3").classList.contains("in-range"), true);

  // Ctrl+Shift+L AutoFilter toggle
  cell(h, "A1").focus();
  await flushAsyncTurns(4);
  key(cell(h, "A1"), "L", { code: "KeyL", ctrlKey: true, shiftKey: true });
  await flushAsyncTurns(14);
  assert.equal(cell(h, "A1").classList.contains("filter-head"), true, "Ctrl+Shift+L turns AutoFilter on");
  key(cell(h, "A1"), "L", { code: "KeyL", ctrlKey: true, shiftKey: true });
  await flushAsyncTurns(14);
  assert.equal(cell(h, "A1").classList.contains("filter-head"), false, "Ctrl+Shift+L toggles AutoFilter off");

  // Ctrl+1 opens Format Cells dialog
  key(cell(h, "A1"), "1", { code: "Digit1", ctrlKey: true });
  await flushAsyncTurns(6);
  assert.equal(byId(h, "sheet-numfmt-dialog").open, true);
  byId(h, "sheet-numfmt-dialog").close();

  // Ctrl+Enter batch fill across multi-cell selection with relative formula shift & 1 undo
  cell(h, "F1").focus();
  key(cell(h, "F1"), "ArrowRight", { shiftKey: true });
  key(cell(h, "F1"), "ArrowDown", { shiftKey: true });
  await flushAsyncTurns(4);
  key(cell(h, "F1"), "F2", {});
  const tdF1 = cell(h, "F1");
  tdF1.textContent = "=A1*2";
  tdF1.dispatchEvent({ type: "input", target: tdF1, bubbles: true });
  key(tdF1, "Enter", { code: "Enter", ctrlKey: true });
  await flushAsyncTurns(18);
  assert.equal(cell(h, "F1").textContent, "20");
  assert.equal(cell(h, "F2").textContent, "40");
  assert.equal(cell(h, "G1").dataset.formula, "=B1*2");
  await undo(h);
  assert.equal(cell(h, "F1").textContent, "");
  assert.equal(cell(h, "F2").textContent, "");
});

test("Ctrl+click multi-range selection deduplicates aggregates, paints ranges, refuses multi-copy, and deletes in 1 step", async () => {
  const h = await openSheet({
    sheets: [{
      name: "시트1",
      cells: [
        { ref: "A1", raw: "10" }, { ref: "B1", raw: "20" },
        { ref: "A2", raw: "30" }, { ref: "B2", raw: "40" },
        { ref: "C1", raw: "5" }, { ref: "D3", raw: "100" },
      ],
    }],
  });

  // Select A1:B2
  cell(h, "A1").dispatchEvent({ type: "mousedown", target: cell(h, "A1"), button: 0, bubbles: true });
  cell(h, "B2").dispatchEvent({ type: "mouseover", target: cell(h, "B2"), button: 0, bubbles: true });
  h.document.dispatchEvent({ type: "mouseup", target: cell(h, "B2"), button: 0, bubbles: true });
  // Ctrl+mousedown + drag B1:C2 (overlapping B1, B2!) then Ctrl+click D3
  cell(h, "B1").dispatchEvent({ type: "mousedown", target: cell(h, "B1"), button: 0, ctrlKey: true, bubbles: true });
  cell(h, "C2").dispatchEvent({ type: "mouseover", target: cell(h, "C2"), button: 0, ctrlKey: true, bubbles: true });
  h.document.dispatchEvent({ type: "mouseup", target: cell(h, "C2"), button: 0, bubbles: true });
  cell(h, "D3").dispatchEvent({ type: "mousedown", target: cell(h, "D3"), button: 0, ctrlKey: true, bubbles: true });
  h.document.dispatchEvent({ type: "mouseup", target: cell(h, "D3"), button: 0, bubbles: true });
  await flushAsyncTurns(6);

  assert.equal(cell(h, "A1").classList.contains("in-range"), true);
  assert.equal(cell(h, "C1").classList.contains("in-range"), true);
  assert.equal(cell(h, "D3").classList.contains("selected-cell"), true);
  const summary = byId(h, "status-sheet-agg").textContent;
  assert.match(summary, /합계: 205/, "B1(20) and B2(40) are NOT double-counted: " + summary);
  assert.match(summary, /숫자 개수: 6/);

  // Multi-range Ctrl+C shows refusal message without crash
  const handled = cell(h, "D3").dispatchEvent({ type: "copy", target: cell(h, "D3"), clipboardData: { setData() {} }, bubbles: true });
  assert.equal(handled, false);
  assert.equal(byId(h, "status-command-feedback").textContent, "이 명령은 여러 개의 선택 영역에서 실행할 수 없습니다.");

  // Delete clears all multi-ranges in 1 undo step
  key(cell(h, "D3"), "Delete", {});
  await flushAsyncTurns(16);
  assert.equal(cell(h, "A1").textContent, "");
  assert.equal(cell(h, "D3").textContent, "");
  await undo(h);
  assert.equal(cell(h, "A1").textContent, "10");
  assert.equal(cell(h, "D3").textContent, "100");

  // Escape clears extra ranges
  key(cell(h, "D3"), "Escape", {});
  await flushAsyncTurns(4);
  assert.equal(cell(h, "A1").classList.contains("in-range"), false);
});

test("general cell and header right-click context menus open, dismiss cleanly, and preserve link/note menu", async () => {
  const h = await openSheet({
    sheets: [{ name: "시트1", cells: [{ ref: "B1", raw: "B" }, { ref: "C1", raw: "=A1" }, { ref: "A1", raw: "10" }] }],
  });
  const cellMenu = byId(h, "sheet-cell-menu");
  const hdrMenu = byId(h, "sheet-header-menu");

  // Plain cell opens #sheet-cell-menu and keeps #sheet-header-menu hidden
  cell(h, "B1").dispatchEvent({ type: "contextmenu", target: cell(h, "B1"), button: 2, clientX: 50, clientY: 50, bubbles: true });
  assert.equal(cellMenu.hidden, false);
  assert.equal(hdrMenu.hidden, true);
  const labels = [...cellMenu.querySelectorAll(":scope > .md-item")].map((el) => el.textContent);
  assert.deepEqual(labels, ["잘라내기", "복사", "붙여넣기", "선택하여 붙여넣기…", "삽입…", "삭제…", "내용 지우기", "메모 삽입", "셀 서식…", "드롭다운 목록에서 선택…", "하이퍼링크…"]);

  // Escape dismisses menu
  h.document.dispatchEvent({ type: "keydown", key: "Escape", bubbles: true });
  assert.equal(cellMenu.hidden, true);

  // Horizontal insert shift from cell context menu shifts B1 & C1 without losing C1
  cell(h, "B1").dispatchEvent({ type: "contextmenu", target: cell(h, "B1"), button: 2, clientX: 50, clientY: 50, bubbles: true });
  h.userClick(cellMenu.querySelector('[data-sheet-ctx="cell-insert"]'));
  assert.equal(cellMenu.hidden, true, "opening modal dismisses context menu");
  assert.equal(byId(h, "sheet-cell-shift-dialog").open, true);
  byId(h, "sheet-shift-horiz").checked = true;
  byId(h, "sheet-shift-vert").checked = false;
  h.userClick(byId(h, "btn-sheet-cell-shift-ok"));
  await flushAsyncTurns(16);
  assert.equal(cell(h, "A1").textContent, "10");
  assert.equal(cell(h, "B1").textContent, "");
  assert.equal(cell(h, "C1").textContent, "B");
  assert.equal(cell(h, "D1").dataset.formula, "=B1");

  // Header right-click shows row/col height & width items
  const rowTh = h.document.querySelector('#pane-sheet th[data-row="2"]');
  rowTh.dispatchEvent({ type: "contextmenu", target: rowTh, button: 2, clientX: 20, clientY: 80, bubbles: true });
  assert.equal(hdrMenu.hidden, false);
  const rowItems = [...hdrMenu.querySelectorAll(".md-item")].filter((el) => !el.hidden).map((el) => el.textContent);
  assert.equal(rowItems.includes("행 높이…"), true);
  assert.equal(rowItems.includes("행 높이 자동 맞춤"), true);
  assert.equal(rowItems.includes("열 너비…"), false);

  h.userClick(hdrMenu.querySelector('[data-sheet-act="row-height"]'));
  assert.equal(hdrMenu.hidden, true);
  assert.equal(byId(h, "sheet-size-dialog").open, true);
  byId(h, "sheet-size-input").value = "999";
  h.userClick(byId(h, "btn-sheet-size-ok"));
  assert.equal(byId(h, "sheet-size-dialog").open, true, "out-of-bounds row height rejected");
  assert.match(byId(h, "sheet-size-status").textContent, /0~409/);
  byId(h, "sheet-size-input").value = "36";
  h.userClick(byId(h, "btn-sheet-size-ok"));
  await flushAsyncTurns(14);
  assert.equal(byId(h, "sheet-size-dialog").open, false);
  assert.equal(h.host.__EASYOFFICE_SHEET_LAYOUT__.rowHeightsOf("시트1").get(2), 36);
  await undo(h);
  assert.equal(Boolean(h.host.__EASYOFFICE_SHEET_LAYOUT__.rowHeightsOf("시트1")?.has(2)), false);
});

test("general-range Remove Duplicates, Text-to-Columns collision/preview/undo, and Format Cells font/size/merge/rotation roundtrip", async () => {
  const h = await openSheet({
    sheets: [{
      name: "시트1",
      cells: [
        { ref: "A1", raw: "이름" }, { ref: "B1", raw: "값" },
        { ref: "A2", raw: "사과" }, { ref: "B2", raw: "1" },
        { ref: "A3", raw: "사과" }, { ref: "B3", raw: "1" },
        { ref: "A4", raw: "배" }, { ref: "B4", raw: "2" },
        { ref: "D1", raw: "x,y,10" }, { ref: "D2", raw: "u,v,20" }, { ref: "E1", raw: "기존값" },
      ],
    }],
  });

  // General-range Remove Duplicates on A1:B4
  cell(h, "A1").focus();
  key(cell(h, "A1"), "ArrowRight", { shiftKey: true });
  key(cell(h, "A1"), "ArrowDown", { shiftKey: true });
  key(cell(h, "A1"), "ArrowDown", { shiftKey: true });
  key(cell(h, "A1"), "ArrowDown", { shiftKey: true });
  await flushAsyncTurns(4);
  h.userClick(h.document.querySelector('#menu-bar .md-item[data-sheet-act="dedupe-dialog"]'));
  assert.equal(byId(h, "sheet-table-dedupe-dialog").open, true);
  assert.equal(byId(h, "sheet-dedupe-header-row").hidden, false);
  h.userClick(byId(h, "btn-sheet-dedupe-ok"));
  await flushAsyncTurns(16);
  assert.equal(cell(h, "A3").textContent, "배");
  assert.equal(cell(h, "A4").textContent, "");
  await undo(h);
  assert.equal(cell(h, "A4").textContent, "배");

  // Text-to-Columns on D1 with collision guard then overwrite + 1 undo
  cell(h, "D1").focus();
  await flushAsyncTurns(4);
  h.userClick(h.document.querySelector('#menu-bar .md-item[data-sheet-act="text-to-columns"]'));
  assert.equal(byId(h, "sheet-text-columns-dialog").open, true);
  byId(h, "sheet-tc-comma").checked = true;
  byId(h, "sheet-tc-comma").dispatchEvent({ type: "change", target: byId(h, "sheet-tc-comma"), bubbles: true });
  assert.match(byId(h, "sheet-tc-preview-table").textContent, /x.*y.*10/);
  assert.match(byId(h, "sheet-tc-status").textContent, /덮어쓰기/);
  h.userClick(byId(h, "btn-sheet-tc-apply"));
  assert.equal(byId(h, "sheet-text-columns-dialog").open, true, "collision blocks overwrite unless checked");
  byId(h, "sheet-tc-overwrite").checked = true;
  byId(h, "sheet-tc-overwrite").dispatchEvent({ type: "change", target: byId(h, "sheet-tc-overwrite"), bubbles: true });
  h.userClick(byId(h, "btn-sheet-tc-apply"));
  await flushAsyncTurns(16);
  assert.equal(byId(h, "sheet-text-columns-dialog").open, false);
  assert.equal(cell(h, "D1").textContent, "x");
  assert.equal(cell(h, "E1").textContent, "y");
  assert.equal(cell(h, "F1").textContent, "10");
  await undo(h);
  assert.equal(cell(h, "D1").textContent, "x,y,10");
  assert.equal(cell(h, "E1").textContent, "기존값");

  // Format Cells: font family, font size, merge, rotation + bounds validation + roundtrip
  cell(h, "A1").focus();
  key(cell(h, "A1"), "ArrowRight", { shiftKey: true });
  await flushAsyncTurns(4);
  key(cell(h, "A1"), "1", { code: "Digit1", ctrlKey: true });
  assert.equal(byId(h, "sheet-numfmt-dialog").open, true);
  byId(h, "sheet-font-family-select").value = "나눔고딕";
  byId(h, "sheet-font-family-select").dispatchEvent({ type: "change", target: byId(h, "sheet-font-family-select"), bubbles: true });
  byId(h, "sheet-font-size-select").value = "16";
  byId(h, "sheet-font-size-select").dispatchEvent({ type: "change", target: byId(h, "sheet-font-size-select"), bubbles: true });
  byId(h, "sheet-align-merge").checked = true;
  byId(h, "sheet-align-rotation").value = "120";
  h.userClick(byId(h, "btn-sheet-numfmt-apply"));
  assert.equal(byId(h, "sheet-numfmt-dialog").open, true, "out-of-bounds rotation rejected");
  assert.match(byId(h, "sheet-numfmt-status").textContent, /-90~90/);
  byId(h, "sheet-align-rotation").value = "45";
  byId(h, "sheet-align-rotation").dispatchEvent({ type: "input", target: byId(h, "sheet-align-rotation"), bubbles: true });
  h.userClick(byId(h, "btn-sheet-numfmt-apply"));
  await flushAsyncTurns(18);
  assert.equal(byId(h, "sheet-numfmt-dialog").open, false);
  assert.equal(cell(h, "A1").dataset.rot, "45");
  assert.equal(cell(h, "A1").style.fontFamily, "나눔고딕");
  assert.equal(h.host.__EASYOFFICE_SHEET_LAYOUT__.get("시트1").merges.length, 1);

  // Reopen Format Cells to verify roundtrip readout
  key(cell(h, "A1"), "1", { code: "Digit1", ctrlKey: true });
  assert.equal(byId(h, "sheet-font-family-select").value, "나눔고딕");
  assert.equal(byId(h, "sheet-font-size-select").value, "16");
  assert.equal(byId(h, "sheet-align-rotation").value, "45");
  assert.equal(byId(h, "sheet-align-merge").checked, true);
  byId(h, "sheet-numfmt-dialog").close();
});
