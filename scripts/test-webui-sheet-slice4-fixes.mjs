import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";

const tdOf = (document, ref) => document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);

function mousedown(el, extra = {}) {
  return el.dispatchEvent({
    type: "mousedown",
    target: el,
    button: 0,
    bubbles: true,
    cancelable: true,
    preventDefault() {},
    stopPropagation() {},
    ...extra,
  });
}

function keydown(el, key, code, extra = {}) {
  return el.dispatchEvent({
    type: "keydown",
    target: el,
    key,
    code,
    bubbles: true,
    cancelable: true,
    preventDefault() {},
    stopPropagation() {},
    ...extra,
  });
}

async function editCell(h, ref, value) {
  const cell = tdOf(h.document, ref);
  mousedown(cell);
  cell.focus();
  cell.textContent = value;
  keydown(cell, "Enter", "Enter");
  await flushAsyncTurns(6);
}

test("multi-range undo reverts strikethrough, number format, borders, and mixed look/protect states", async () => {
  const h = await createWorkspaceHarness({ search: "?module=SHEET" });
  switchModule(h.document, "SHEET");
  await flushAsyncTurns(10);
  const { document, sandbox } = h;

  await editCell(h, "A1", "100");
  await editCell(h, "C3", "200");

  // Ctrl+5 on non-contiguous A1 + C3, then undo must revert BOTH cells.
  mousedown(tdOf(document, "A1"));
  mousedown(tdOf(document, "C3"), { ctrlKey: true });
  tdOf(document, "C3").focus();
  keydown(tdOf(document, "C3"), "5", "Digit5", { ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(tdOf(document, "A1").style.textDecoration, "line-through");
  assert.equal(tdOf(document, "C3").style.textDecoration, "line-through");

  sandbox.dispatchWindowKeydown({ key: "z", code: "KeyZ", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(tdOf(document, "A1").style.textDecoration, "");
  assert.equal(tdOf(document, "C3").style.textDecoration, "");

  // Number format via Format Cells on multi-range A1 + C3 must revert BOTH on undo.
  mousedown(tdOf(document, "A1"));
  mousedown(tdOf(document, "C3"), { ctrlKey: true });
  tdOf(document, "C3").focus();
  keydown(tdOf(document, "C3"), "1", "Digit1", { ctrlKey: true });
  await flushAsyncTurns(4);
  document.getElementById("sheet-numfmt-code").value = "#,##0.00";
  document.getElementById("sheet-numfmt-code").dispatchEvent({ type: "input", bubbles: true });
  h.userClick(document.getElementById("btn-sheet-numfmt-apply"));
  await flushAsyncTurns(8);
  assert.equal(tdOf(document, "A1")._nf, "#,##0.00");
  assert.equal(tdOf(document, "C3")._nf, "#,##0.00");

  sandbox.dispatchWindowKeydown({ key: "z", code: "KeyZ", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(tdOf(document, "A1")._nf, "");
  assert.equal(tdOf(document, "C3")._nf, "");

  // Multi-range border applies to both A1 and C3, and borderState reports mixed when combined with unbordered D1.
  mousedown(tdOf(document, "A1"));
  mousedown(tdOf(document, "C3"), { ctrlKey: true });
  h.userClick(document.querySelector('#menu-bar .md-item[data-sheet-look="border-all"]'));
  await flushAsyncTurns(8);
  assert.ok(tdOf(document, "A1").style.getPropertyValue("--lk-shadow"));
  assert.equal(tdOf(document, "C3").style.borderBottomStyle, "solid");

  mousedown(tdOf(document, "A1"));
  mousedown(tdOf(document, "D1"), { ctrlKey: true });
  tdOf(document, "D1").focus();
  keydown(tdOf(document, "D1"), "1", "Digit1", { ctrlKey: true });
  await flushAsyncTurns(4);
  assert.equal(document.querySelector('#sheet-numfmt-dialog [data-border-pos="t"]').getAttribute("aria-pressed"), "false");
  h.userClick(document.querySelector('#sheet-numfmt-dialog [data-dialog-close]'));
  await flushAsyncTurns(4);

  // protectStates reports indeterminate when A1 is unlocked and C3 is locked.
  mousedown(tdOf(document, "A1"));
  tdOf(document, "A1").focus();
  keydown(tdOf(document, "A1"), "1", "Digit1", { ctrlKey: true });
  await flushAsyncTurns(4);
  document.getElementById("sheet-protect-cell-locked").checked = false;
  document.getElementById("sheet-protect-cell-locked").dispatchEvent({ type: "change", bubbles: true });
  h.userClick(document.getElementById("btn-sheet-numfmt-apply"));
  await flushAsyncTurns(6);

  mousedown(tdOf(document, "A1"));
  mousedown(tdOf(document, "C3"), { ctrlKey: true });
  tdOf(document, "C3").focus();
  keydown(tdOf(document, "C3"), "1", "Digit1", { ctrlKey: true });
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("sheet-protect-cell-locked").checked, false);
  assert.equal(document.getElementById("sheet-protect-cell-locked").indeterminate, true);
  h.userClick(document.querySelector('#sheet-numfmt-dialog [data-dialog-close]'));
});

test("Format Cells guards partial commits on invalid table merge and recalculates formulas after valid merge", async () => {
  const h = await createWorkspaceHarness({ search: "?module=SHEET" });
  switchModule(h.document, "SHEET");
  await flushAsyncTurns(10);
  const { document } = h;

  await editCell(h, "B2", "10");
  await editCell(h, "C2", "20");
  h.host.__EASYOFFICE_SHEET_LAYOUT__.setTables("Sheet1", [{ name: "T1", range: "B2:C3", header: true, columns: ["C1", "C2"] }]);

  mousedown(tdOf(document, "B2"));
  mousedown(tdOf(document, "C3"), { shiftKey: true });
  tdOf(document, "B2").focus();
  keydown(tdOf(document, "B2"), "1", "Digit1", { ctrlKey: true });
  await flushAsyncTurns(4);
  document.getElementById("sheet-numfmt-code").value = "0.00";
  document.getElementById("sheet-numfmt-code").dispatchEvent({ type: "input", bubbles: true });
  document.getElementById("sheet-align-merge").checked = true;
  document.getElementById("sheet-align-merge").dispatchEvent({ type: "change", bubbles: true });
  h.userClick(document.getElementById("btn-sheet-numfmt-apply"));
  await flushAsyncTurns(8);

  assert.equal(document.getElementById("sheet-numfmt-dialog").open, true);
  assert.match(document.getElementById("sheet-numfmt-status").textContent, /병합할 수 없음/);
  assert.equal(tdOf(document, "B2")._nf || "", "", "number format must not commit when merge is refused");
  h.userClick(document.querySelector('#sheet-numfmt-dialog [data-dialog-close]'));
  await flushAsyncTurns(4);

  // Valid merge outside table (D1:E2) cleans secondary cells and recalculates dependent formulas.
  await editCell(h, "D1", "5");
  await editCell(h, "E1", "10");
  await editCell(h, "F1", "=E1+1");
  assert.equal(tdOf(document, "F1").textContent, "11");

  mousedown(tdOf(document, "D1"));
  mousedown(tdOf(document, "E2"), { shiftKey: true });
  tdOf(document, "D1").focus();
  keydown(tdOf(document, "D1"), "1", "Digit1", { ctrlKey: true });
  await flushAsyncTurns(4);
  document.getElementById("sheet-align-merge").checked = true;
  document.getElementById("sheet-align-merge").dispatchEvent({ type: "change", bubbles: true });
  h.userClick(document.getElementById("btn-sheet-numfmt-apply"));
  await flushAsyncTurns(8);
  assert.equal(tdOf(document, "F1").textContent, "1");
});

test("IME modifier shortcuts, toolbar-focus shortcuts, AutoSum A2, Esc on rotated cell, and merge/dedupe/text-columns guards", async () => {
  const h = await createWorkspaceHarness({ search: "?module=SHEET" });
  switchModule(h.document, "SHEET");
  await flushAsyncTurns(10);
  const { document, host } = h;

  // Windows Korean IME Process/229 with Ctrl/Alt modifiers still runs shortcuts.
  await editCell(h, "E1", "99");
  mousedown(tdOf(document, "E1"));
  mousedown(tdOf(document, "E2"), { shiftKey: true });
  tdOf(document, "E1").focus();
  keydown(tdOf(document, "E1"), "Process", "KeyD", { keyCode: 229, ctrlKey: true, isComposing: false });
  await flushAsyncTurns(8);
  assert.equal(tdOf(document, "E2").textContent, "99");

  mousedown(tdOf(document, "E1"));
  mousedown(tdOf(document, "F1"), { shiftKey: true });
  tdOf(document, "E1").focus();
  keydown(tdOf(document, "E1"), "Process", "KeyR", { keyCode: 229, ctrlKey: true, isComposing: false });
  await flushAsyncTurns(8);
  assert.equal(tdOf(document, "F1").textContent, "99");

  // AutoSum on A2 with A1=10 and B2=20 sums A1:A1 vertically.
  await editCell(h, "A1", "10");
  await editCell(h, "B2", "20");
  mousedown(tdOf(document, "A2"));
  tdOf(document, "A2").focus();
  h.userClick(document.getElementById("btn-sheet-autosum"));
  await flushAsyncTurns(8);
  assert.equal(tdOf(document, "A2").dataset.formula, "=SUM(A1:A1)");
  assert.equal(tdOf(document, "A2").textContent, "10");

  // Global non-input toolbar focus runs Alt+= and Ctrl+1, and ignores text inputs.
  await editCell(h, "D1", "5");
  await editCell(h, "D2", "7");
  mousedown(tdOf(document, "D3"));
  const wrapBtn = document.querySelector('#icon-toolbar button[data-sheet-look="wrap"]');
  wrapBtn.focus();
  keydown(wrapBtn, "=", "Equal", { altKey: true });
  await flushAsyncTurns(8);
  assert.equal(tdOf(document, "D3").dataset.formula, "=SUM(D1:D2)");

  keydown(wrapBtn, "1", "Digit1", { ctrlKey: true });
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("sheet-numfmt-dialog").open, true);
  h.userClick(document.querySelector('#sheet-numfmt-dialog [data-dialog-close]'));
  await flushAsyncTurns(4);

  // Esc during inline edit of a rotated cell keeps .cell-rot wrapper intact.
  await editCell(h, "G1", "ROT");
  mousedown(tdOf(document, "G1"));
  tdOf(document, "G1").focus();
  keydown(tdOf(document, "G1"), "1", "Digit1", { ctrlKey: true });
  await flushAsyncTurns(4);
  h.userClick(document.querySelector('[data-rot-preset="45"]'));
  h.userClick(document.getElementById("btn-sheet-numfmt-apply"));
  await flushAsyncTurns(8);
  assert.ok(tdOf(document, "G1").querySelector(".cell-rot"));
  keydown(tdOf(document, "G1"), "F2", "F2");
  await flushAsyncTurns(4);
  keydown(tdOf(document, "G1"), "Escape", "Escape");
  await flushAsyncTurns(6);
  assert.ok(tdOf(document, "G1").querySelector(".cell-rot"), "Esc cancel must restore .cell-rot span");

  // Ctrl+D over merged range refuses without corrupting merge secondary cell.
  await editCell(h, "B1", "TOP");
  mousedown(tdOf(document, "B2"));
  mousedown(tdOf(document, "B3"), { shiftKey: true });
  document.querySelector('#menu-bar .md-item[data-sheet-act="merge"]')?.click();
  await flushAsyncTurns(6);
  mousedown(tdOf(document, "B1"));
  mousedown(tdOf(document, "B3"), { shiftKey: true });
  tdOf(document, "B1").focus();
  keydown(tdOf(document, "B1"), "d", "KeyD", { ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(tdOf(document, "B3").textContent, "");
  assert.match(document.getElementById("status-command-feedback").textContent, /병합된 셀/);

  // Ctrl+Enter over range containing merge writes only to merge anchor, leaving secondary empty.
  mousedown(tdOf(document, "B2"));
  mousedown(tdOf(document, "B4"), { shiftKey: true });
  tdOf(document, "B2").focus();
  keydown(tdOf(document, "B2"), "F2", "F2");
  tdOf(document, "B2").textContent = "ANCHOR";
  keydown(tdOf(document, "B2"), "Enter", "Enter", { ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(tdOf(document, "B2").textContent, "ANCHOR");
  assert.equal(tdOf(document, "B4").textContent, "ANCHOR");
  assert.equal(tdOf(document, "B3").textContent, "");

  // Remove Duplicates refuses multi-range selection.
  mousedown(tdOf(document, "A1"));
  mousedown(tdOf(document, "C3"), { ctrlKey: true });
  document.querySelector('#menu-bar .md-item[data-sheet-act="dedupe-dialog"]')?.click();
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("sheet-table-dedupe-dialog").open, false);
  assert.match(document.getElementById("status-command-feedback").textContent, /여러 선택 영역/);

  // Text-to-Columns grid overflow displays sheet column limit error instead of empty data message.
  const tcListeners = new Map();
  const makeNode = (init = {}) => {
    const ls = new Map();
    return {
      ...init,
      style: {},
      textContent: init.textContent || "",
      addEventListener(type, fn) { (ls.get(type) || ls.set(type, []).get(type)).push(fn); tcListeners.set(this, ls); },
      dispatch(type) { for (const fn of ls.get(type) || []) fn({ target: this, preventDefault() {} }); },
      replaceChildren(...kids) { this.children = kids; },
      appendChild(kid) { (this.children ||= []).push(kid); return kid; },
      querySelector() { return makeNode({}); },
      querySelectorAll() { return []; },
      focus() {},
    };
  };
  const tcNodes = {
    "sheet-text-columns-dialog": makeNode({ open: false, showModal() { this.open = true; }, close() { this.open = false; } }),
    "sheet-tc-tab": makeNode({ checked: false }),
    "sheet-tc-semicolon": makeNode({ checked: false }),
    "sheet-tc-comma": makeNode({ checked: false }),
    "sheet-tc-space": makeNode({ checked: false }),
    "sheet-tc-other": makeNode({ checked: false }),
    "sheet-tc-other-char": makeNode({ value: "" }),
    "sheet-tc-collapse": makeNode({ checked: false }),
    "sheet-tc-overwrite": makeNode({ checked: false }),
    "sheet-tc-preview-table": makeNode({ children: [] }),
    "sheet-tc-status": makeNode({ textContent: "" }),
    "btn-sheet-tc-apply": makeNode({}),
  };
  const tcDoc = {
    getElementById: (id) => tcNodes[id] || null,
    createElement: () => makeNode({}),
    createTextNode: (t) => ({ textContent: t }),
    querySelector: () => null,
    querySelectorAll: () => [],
  };
  const tcUi = host.__EASYOFFICE_CREATE_SHEET_TEXT_COLUMNS__({
    document: tcDoc,
    kit: host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(tcDoc),
    grid: host.__EASYOFFICE_SHEET_REFS__ ? { ...host.__EASYOFFICE_SHEET_REFS__, MAX_COLS: 16384 } : null,
    sheetDataStore: { Sheet1: { XFC1: { value: "a,b,c", formula: "" } } },
    snapshotDomSheetCells: () => ({ XFC1: { value: "a,b,c", formula: "" } }),
    getActiveSheetName: () => "Sheet1",
    getRange: () => ({ r1: 1, c1: 16383, r2: 1, c2: 16383 }),
    commitIfEditing: () => {},
    say: () => {},
  });
  assert.equal(tcUi.open(), true);
  tcNodes["sheet-tc-comma"].checked = true;
  tcNodes["btn-sheet-tc-apply"].dispatch("click");
  await flushAsyncTurns(4);
  assert.match(tcNodes["sheet-tc-status"].textContent, /시트 열 범위/);
});
