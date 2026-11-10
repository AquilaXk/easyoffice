import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";

async function openSheet(options = {}) {
  const h = await createWorkspaceHarness(options);
  switchModule(h.document, "SHEET");
  await flushAsyncTurns();
  return h;
}

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const activeRef = (d) => d.getElementById("active-cell-ref").textContent;
const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });
const agg = (d) => d.getElementById("status-sheet-agg").textContent;

// Emulates what the browser does after an un-prevented printable keydown on a focused editable cell.
function typeText(d, text) {
  for (const ch of text) {
    const td = d.activeElement;
    const notPrevented = key(td, ch);
    assert.equal(notPrevented, true, "printable keys must not be cancelled");
    td.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: ch, target: td });
    td.textContent = td.textContent + ch;
    td.dispatchEvent({ type: "input", inputType: "insertText", data: ch, target: td });
  }
}

function bigSheetModel(rows, cols = 8) {
  const cells = [];
  for (let r = 1; r <= rows; r += 1) {
    for (let c = 0; c < cols; c += 1) {
      const ref = String.fromCodePoint(65 + c) + r;
      cells.push({ ref, raw: r === 1 ? "열" + (c + 1) : String(r * 10 + c) });
    }
  }
  return { sheets: [{ name: "큰시트", cells }] };
}

test("typing into a selected cell replaces the old value instead of appending", async () => {
  const { document } = await openSheet();
  const b3 = cell(document, "B3");
  b3.focus();
  assert.equal(b3.textContent, "920");
  typeText(document, "1000");
  assert.equal(b3.textContent, "1000", "printable typing overwrites (never 9201000)");
  key(b3, "Enter");
  await flushAsyncTurns();
  assert.equal(cell(document, "B3").textContent, "1000");
  assert.equal(activeRef(document), "B4", "Enter commits and moves down");
  assert.equal(document.activeElement, cell(document, "B4"));
});

test("typing =B3 into a cell makes a formula, not appended text", async () => {
  const { document } = await openSheet();
  const c9 = cell(document, "C9");
  c9.focus();
  typeText(document, "=B3");
  key(c9, "Enter");
  await flushAsyncTurns(12);
  assert.equal(c9.dataset.formula, "=B3");
  assert.equal(c9.textContent, "920");
  const b3 = cell(document, "B3");
  b3.focus();
  typeText(document, "=B3");
  assert.equal(b3.textContent, "=B3", "typing over a value never prefixes the old content");
});

test("Enter, Shift+Enter, Tab and Esc follow the spreadsheet editing contract", async () => {
  const { document } = await openSheet();
  const b4 = cell(document, "B4");
  b4.focus();
  typeText(document, "700");
  key(b4, "Tab");
  await flushAsyncTurns();
  assert.equal(activeRef(document), "C4");
  assert.equal(cell(document, "B4").textContent, "700");

  const c4 = cell(document, "C4");
  typeText(document, "55");
  key(c4, "Enter", { shiftKey: true });
  await flushAsyncTurns();
  assert.equal(activeRef(document), "C3", "Shift+Enter commits and moves up");
  assert.equal(cell(document, "C4").textContent, "55");

  const c3 = cell(document, "C3");
  const before = c3.textContent;
  typeText(document, "999");
  key(c3, "Escape");
  await flushAsyncTurns();
  assert.equal(c3.textContent, before, "Esc cancels the edit and restores the old value");
  assert.equal(activeRef(document), "C3");
  key(c3, "ArrowDown");
  assert.equal(activeRef(document), "C4", "arrow keys keep working after Esc");
});

test("arrow keys: ready mode moves; F2 edit mode keeps the caret keys; enter mode commits and moves", async () => {
  const { document } = await openSheet();
  const b2 = cell(document, "B2");
  b2.focus();
  key(b2, "ArrowRight");
  assert.equal(activeRef(document), "C2");
  key(cell(document, "C2"), "ArrowDown");
  assert.equal(activeRef(document), "C3");
  key(cell(document, "C3"), "ArrowLeft");
  key(cell(document, "B3"), "ArrowUp");
  assert.equal(activeRef(document), "B2");

  const f = cell(document, "B2");
  key(f, "F2");
  assert.equal(f.dataset.editMode, "edit");
  assert.equal(key(f, "ArrowLeft"), true, "edit mode leaves ArrowLeft to the caret");
  assert.equal(key(f, "ArrowRight"), true, "edit mode leaves ArrowRight to the caret");
  assert.equal(activeRef(document), "B2");
  key(f, "Escape");

  typeText(document, "5");
  assert.equal(f.dataset.editMode, "enter");
  key(f, "ArrowRight");
  await flushAsyncTurns();
  assert.equal(activeRef(document), "C2", "enter mode: arrows commit and move");
  assert.equal(cell(document, "B2").textContent, "5");
});

test("IME composition: Enter during composition neither commits nor moves; Hangul text commits", async () => {
  const { document } = await openSheet();
  const a9 = cell(document, "A9");
  a9.focus();
  key(a9, "Process", { isComposing: true });
  a9.dispatchEvent({ type: "compositionstart", target: a9 });
  a9.textContent = "한";
  a9.dispatchEvent({ type: "input", inputType: "insertCompositionText", target: a9 });
  const enterNotPrevented = key(a9, "Enter", { isComposing: true });
  assert.equal(activeRef(document), "A9", "no move while composing");
  assert.equal(enterNotPrevented, true, "composition Enter is left to the IME");
  a9.textContent = "한글";
  a9.dispatchEvent({ type: "compositionend", target: a9 });
  a9.dispatchEvent({ type: "input", inputType: "insertText", target: a9 });
  key(a9, "Enter");
  await flushAsyncTurns();
  assert.equal(cell(document, "A9").textContent, "한글");
  assert.equal(activeRef(document), "A10");
});

test("range selection by Shift+arrows, Shift+click, drag and Ctrl+A with a visible highlight", async () => {
  const { document } = await openSheet();
  const b2 = cell(document, "B2");
  b2.focus();
  key(b2, "ArrowRight", { shiftKey: true });
  key(b2, "ArrowDown", { shiftKey: true });
  const inRange = () => [...document.querySelectorAll("#pane-sheet td.in-range")].map((t) => t.dataset.cell).sort((a, b) => a.localeCompare(b));
  assert.deepEqual(inRange(), ["B2", "B3", "C2", "C3"]);
  assert.equal(activeRef(document), "B2", "the anchor stays the active cell");

  key(b2, "ArrowDown");
  assert.equal(document.querySelectorAll("#pane-sheet td.in-range").length, 0, "plain arrow collapses the range");

  const d3 = cell(document, "D3");
  d3.dispatchEvent({ type: "mousedown", target: d3, shiftKey: true, button: 0 });
  d3.dispatchEvent({ type: "mouseup", target: d3 });
  assert.deepEqual(inRange(), ["B3", "C3", "D3"], "Shift+click extends from the anchor B3 to D3");

  const a2 = cell(document, "A2");
  const c4 = cell(document, "C4");
  a2.dispatchEvent({ type: "mousedown", target: a2, button: 0 });
  c4.dispatchEvent({ type: "mouseover", target: c4 });
  c4.dispatchEvent({ type: "mouseup", target: c4 });
  assert.deepEqual(inRange(), ["A2", "A3", "A4", "B2", "B3", "B4", "C2", "C3", "C4"]);

  key(document.activeElement, "a", { ctrlKey: true });
  assert.ok(inRange().length >= 8 * 8, "Ctrl+A selects the used data region");
});

test("selection aggregate reflects the selected range only", async () => {
  const { document } = await openSheet();
  const b2 = cell(document, "B2");
  b2.focus();
  key(b2, "ArrowRight", { shiftKey: true });
  key(b2, "ArrowDown", { shiftKey: true });
  const text = agg(document);
  assert.match(text, /합계 4,250/);
  assert.match(text, /평균 1,062\.5/);
  assert.match(text, /개수 4/);
  assert.match(text, /최소 920/);
  assert.match(text, /최대 1,200/);
});

test("copy/cut/paste exchange TSV with the clipboard, grow the sheet and adjust relative references", async () => {
  const { document } = await openSheet();
  const store = {};
  const clip = { setData: (t, v) => (store[t] = v), getData: (t) => store[t] || "" };
  const b2 = cell(document, "B2");
  b2.focus();
  key(b2, "ArrowRight", { shiftKey: true });
  key(b2, "ArrowDown", { shiftKey: true });
  const copyEv = { type: "copy", target: b2, clipboardData: clip };
  assert.equal(b2.dispatchEvent(copyEv), false, "copy is handled");
  assert.equal(store["text/plain"], "1150\t1200\n920\t980");

  const a9 = cell(document, "A9");
  a9.focus();
  a9.dispatchEvent({ type: "paste", target: a9, clipboardData: clip });
  await flushAsyncTurns(12);
  assert.equal(cell(document, "A9").textContent, "1150");
  assert.equal(cell(document, "B9").textContent, "1200");
  assert.equal(cell(document, "A10").textContent, "920");
  assert.equal(cell(document, "B10").textContent, "980");

  // multi-row paste past the last visible row grows the sheet
  const a12 = cell(document, "A12");
  a12.focus();
  const multi = { getData: () => "x1\nx2\nx3\n", setData() {} };
  a12.dispatchEvent({ type: "paste", target: a12, clipboardData: multi });
  await flushAsyncTurns(12);
  assert.equal(cell(document, "A12").textContent, "x1");
  key(document.activeElement, "ArrowDown");
  key(document.activeElement, "ArrowDown");
  assert.equal(activeRef(document), "A14");
  assert.equal(document.activeElement.textContent, "x3");

  // formulas shift relative A1 references when pasted
  const f2 = cell(document, "F2");
  f2.focus();
  const fc = {};
  f2.dispatchEvent({ type: "copy", target: f2, clipboardData: { setData: (t, v) => (fc[t] = v), getData: (t) => fc[t] || "" } });
  assert.equal(fc["text/plain"], "=SUM(B2:E2)", "copy keeps the raw formula");
  const f9 = cell(document, "F9");
  f9.focus();
  f9.dispatchEvent({ type: "paste", target: f9, clipboardData: { getData: (t) => fc[t] || "", setData() {} } });
  await flushAsyncTurns(15);
  assert.equal(cell(document, "F9").dataset.formula, "=SUM(B9:E9)");
  assert.equal(cell(document, "F9").textContent, "1200", "pasted formula is recalculated (B9 is 1200, C9:E9 are empty)");

  // cut empties the source and the dependent formula recalculates
  const b9 = cell(document, "B9");
  b9.focus();
  const cut = {};
  b9.dispatchEvent({ type: "cut", target: b9, clipboardData: { setData: (t, v) => (cut[t] = v), getData: () => "" } });
  await flushAsyncTurns(15);
  assert.equal(cut["text/plain"], "1200");
  assert.equal(cell(document, "B9").textContent, "");
  assert.equal(cell(document, "F9").textContent, "0");
});

test("Delete and Backspace clear every selected cell", async () => {
  const { document } = await openSheet();
  const a9 = cell(document, "A9");
  a9.focus();
  typeText(document, "q");
  key(a9, "Tab");
  typeText(document, "w");
  key(cell(document, "B9"), "Enter");
  await flushAsyncTurns();
  const first = cell(document, "A9");
  first.focus();
  key(first, "ArrowRight", { shiftKey: true });
  key(first, "Backspace");
  await flushAsyncTurns(8);
  assert.equal(cell(document, "A9").textContent, "");
  assert.equal(cell(document, "B9").textContent, "");
});

test("large sheet: 10,000 rows stay virtualized, navigable past the old 12x8 grid, and responsive", async () => {
  const { document } = await openSheet();
  await pickFile(document, makeFixtureFile("큰시트.xlsx", bigSheetModel(10000)));
  const tdCount = () => document.querySelectorAll("#pane-sheet td[data-cell]").length;
  assert.ok(tdCount() > 0 && tdCount() < 800, "renders only a window (got " + tdCount() + ")");
  const a1 = cell(document, "A1");
  a1.focus();

  key(a1, "ArrowDown", { ctrlKey: true });
  assert.equal(activeRef(document), "A10000", "Ctrl+Down jumps to the last populated row");
  assert.ok(cell(document, "A10000"), "the target row is materialized");
  assert.equal(cell(document, "A10000").textContent, "100000");
  assert.ok(tdCount() < 800);

  key(document.activeElement, "ArrowRight", { ctrlKey: true });
  assert.equal(activeRef(document), "H10000");
  key(document.activeElement, "ArrowRight");
  assert.equal(activeRef(document), "I10000", "columns grow beyond H");
  assert.ok(cell(document, "I10000"));
  key(document.activeElement, "Home", { ctrlKey: true });
  assert.equal(activeRef(document), "A1");

  const times = [];
  for (let i = 0; i < 40; i += 1) {
    const t0 = performance.now();
    key(document.activeElement, "ArrowDown");
    times.push(performance.now() - t0);
  }
  const worst = Math.max(...times);
  console.log("[perf] 10,000-row arrow-down handler: avg " + (times.reduce((a, b) => a + b, 0) / times.length).toFixed(2) + " ms, max " + worst.toFixed(2) + " ms");
  // Complexity is asserted structurally (DOM window stays bounded after the
  // navigation); timing uses the median plus a generous backstop so that
  // scheduler or GC pauses under load cannot fail the test.
  assert.ok(tdCount() > 0 && tdCount() < 800, "window stays bounded after 40 arrow-downs (got " + tdCount() + ")");
  const median = [...times].sort((a, b) => a - b)[Math.floor(times.length / 2)];
  assert.ok(median < 1500, "arrow key handler median stays bounded on the slow DOM stub (" + median.toFixed(1) + " ms)");
  assert.ok(worst < 10000, "no single arrow key handler call stalls (" + worst.toFixed(1) + " ms)");

  const t1 = performance.now();
  const pageTd = document.activeElement;
  key(pageTd, "PageDown");
  assert.ok(tdCount() > 0 && tdCount() < 800, "window stays bounded after PageDown (got " + tdCount() + ")");
  console.log("[perf] PageDown (window re-render): " + (performance.now() - t1).toFixed(2) + " ms");
});

test("numbers are right-aligned with a General display format; the raw value stays editable", async () => {
  const { document } = await openSheet();
  const rawNumber = "1266.66666666667";
  await pickFile(
    document,
    makeFixtureFile("수치.xlsx", {
      sheets: [{ name: "수치", cells: [{ ref: "A1", raw: rawNumber }, { ref: "A2", raw: "텍스트" }, { ref: "A3", raw: "42" }] }],
    })
  );
  const a1 = cell(document, "A1");
  assert.equal(a1.textContent, "1266.666667");
  assert.ok(a1.classList.contains("num"), "numbers get the right-aligned class");
  assert.ok(cell(document, "A3").classList.contains("num"));
  assert.ok(!cell(document, "A2").classList.contains("num"));
  a1.focus();
  assert.equal(document.getElementById("formula-input").value, rawNumber, "the formula bar shows the raw value");
  key(a1, "F2");
  assert.equal(a1.textContent, rawNumber, "edit mode exposes the raw value");
  key(a1, "Escape");
  assert.equal(a1.textContent, "1266.666667");
});

test("column header drag resizes a column with a stable fixed width", async () => {
  const { document } = await openSheet();
  const th = document.querySelector('#pane-sheet th[data-col="B"]');
  const handle = th.querySelector(".col-resizer");
  assert.ok(handle, "each column header has a resize handle");
  const w0 = Number.parseInt(th.style.width, 10);
  assert.ok(w0 > 0, "header widths are explicit (fixed layout)");
  handle.dispatchEvent({ type: "mousedown", target: handle, clientX: 200, button: 0 });
  const vp = document.getElementById("sheet-viewport");
  vp.dispatchEvent({ type: "mousemove", target: vp, clientX: 260 });
  vp.dispatchEvent({ type: "mouseup", target: vp, clientX: 260 });
  const w1 = Number.parseInt(document.querySelector('#pane-sheet th[data-col="B"]').style.width, 10);
  assert.equal(w1, w0 + 60);
  const c2 = cell(document, "B2");
  c2.focus();
  key(c2, "F2");
  assert.equal(Number.parseInt(document.querySelector('#pane-sheet th[data-col="B"]').style.width, 10), w1, "entering edit mode does not change widths");
});

test("formula bar shows the raw formula and the result cannot be clipped silently", async () => {
  const { document } = await openSheet();
  const f2 = cell(document, "F2");
  f2.focus();
  assert.equal(document.getElementById("formula-input").value, "=SUM(B2:E2)");
  const out = document.getElementById("sheet-eval-result");
  assert.ok(out.textContent.length > 0);
  assert.equal(out.getAttribute("title"), out.textContent, "full result is available as a tooltip");
});

test("outline sheet names match the tab labels", async () => {
  const { document } = await openSheet();
  const tabs = [...document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]")].map((b) => b.textContent.trim());
  const outline = [...document.querySelectorAll("#outline-list .outline-item")].map((li) => li.textContent);
  for (const label of tabs) {
    assert.ok(outline.some((o) => o.includes(label)), `outline lists "${label}": ${outline.join(" | ")}`);
  }
  assert.ok(!outline.some((o) => o.includes("(Sheet1)")), "internal sheet ids are not shown");
});

test("SHEET home shows spreadsheet commands only (while mod-symbol is enabled in SHEET)", async () => {
  const { document } = await openSheet();
  const hiddenCmds = ["para-numbering", "para-bullet", "mod-footnote", "indent-in"];
  for (const cmd of hiddenCmds) {
    const btn = document.querySelector(`#icon-toolbar [data-studio-cmd="${cmd}"]`);
    assert.ok(btn.hidden, `${cmd} is hidden in SHEET`);
  }
  assert.equal(document.querySelector('#icon-toolbar [data-studio-cmd="mod-symbol"]').hidden, false, "mod-symbol is available in SHEET");
  const save = studioItem(document, "studio-save");
  assert.equal(save.hidden, false);
  assert.equal(document.getElementById("btn-toggle-ctrl-marks").hidden, true);
  switchModule(document, "DOC");
  await flushAsyncTurns();
  assert.equal(document.querySelector('#icon-toolbar [data-studio-cmd="para-numbering"]').hidden, false, "DOC shows them again");
});

test("formatted currency, grouped numbers and percentages participate in SUM/AVERAGE and selection aggregates, and sheet ops cover populated rows past row 6", async () => {
  const { document, userClick } = await openSheet();
  await pickFile(
    document,
    makeFixtureFile("포맷집계.xlsx", {
      sheets: [
        {
          name: "Sheet1",
          cells: [
            { ref: "A1", raw: "항목" },
            { ref: "B1", raw: "값" },
            { ref: "B2", raw: "₩1,000" },
            { ref: "B3", raw: "1,500" },
            { ref: "B4", raw: "50.0%" },
            { ref: "B9", raw: "2,000" },
            { ref: "C2", raw: "=SUM(B2:B9)" },
          ],
        },
      ],
    })
  );
  await flushAsyncTurns(12);
  assert.equal(cell(document, "C2").textContent, "4550", "SUM includes ₩1,000 + 1,500 + 50.0% + 2,000 (row 9)");

  const b2 = cell(document, "B2");
  b2.focus();
  key(b2, "ArrowDown", { shiftKey: true });
  key(b2, "ArrowDown", { shiftKey: true });
  const text = agg(document);
  assert.match(text, /합계 2,550/, "selection aggregate parses ₩1,000 + 1,500 + 50.0%");
  assert.match(text, /개수 3/);

  userClick(document.querySelector('.md-item[data-studio-menu-cmd="sheet-mod-pivot"]'));
  await flushAsyncTurns(10);
  const evalOut = document.getElementById("sheet-eval-result").textContent;
  assert.match(evalOut, /합계 4550.*최대 2000/, "pivot summary scans populated rows beyond row 6");

  userClick(document.querySelector('.md-item[data-studio-menu-cmd="data-sort"]'));
  await flushAsyncTurns(10);
  assert.equal(cell(document, "B2").textContent, "50.0%", "first sort orders ascending");
  assert.equal(cell(document, "B5").textContent, "2,000", "row 9 (2,000) is included in the sorted range");
  userClick(document.querySelector('.md-item[data-studio-menu-cmd="data-sort"]'));
  await flushAsyncTurns(10);
  assert.equal(cell(document, "B2").textContent, "2,000", "second sort orders descending with row 9 at B2");
});

test("sheet-grid packRef, COL_NAMES cache, getTdAt, and paintSelectionRange update selection without querySelectorAll", async () => {
  const { document } = await openSheet();
  const gridUtils = document.defaultView.__EASYOFFICE_SHEET_GRID_UTILS__;
  assert.equal(typeof gridUtils?.packRef, "function", "packRef exposed on __EASYOFFICE_SHEET_GRID_UTILS__");
  assert.equal(gridUtils.packRef("A1"), gridUtils.packCoord(1, 1));
  assert.equal(gridUtils.packRef("C12"), gridUtils.packCoord(12, 3));
  assert.equal(gridUtils.packRef("AA25"), gridUtils.packCoord(25, 27));
  assert.equal(gridUtils.packRef("$B$4"), gridUtils.packCoord(4, 2));
  assert.equal(gridUtils.packRef("XFD1048576"), gridUtils.packCoord(1048576, 16384));
  assert.notEqual(gridUtils.packRef("XFD1048576"), gridUtils.packRef("XFD524288"));
  assert.equal(gridUtils.packRef("invalid"), 0);
  assert.equal(gridUtils.colName(1), "A");
  assert.equal(gridUtils.colName(26), "Z");
  assert.equal(gridUtils.colName(27), "AA");
  assert.equal(gridUtils.colName(256), "IV");

  const b2 = cell(document, "B2");
  b2.focus();
  const table = document.querySelector("#pane-sheet .sheet-grid");
  let qsaCount = 0;
  const origQsa = table.querySelectorAll.bind(table);
  table.querySelectorAll = (...args) => {
    qsaCount += 1;
    return origQsa(...args);
  };
  key(b2, "ArrowRight", { shiftKey: true });
  key(b2, "ArrowDown", { shiftKey: true });
  table.querySelectorAll = origQsa;
  assert.equal(qsaCount, 0, "selection painting does not call table.querySelectorAll");
  const selected = [...document.querySelectorAll("#pane-sheet td.in-range")].map((td) => td.dataset.cell).sort((a, b) => a.localeCompare(b));
  assert.deepEqual(selected, ["B2", "B3", "C2", "C3"]);

  const sheetUtils = document.defaultView.__EASYOFFICE_SHEET_UTILS__;
  assert.equal(typeof sheetUtils?.bytesToBase64, "function", "bytesToBase64 exposed on __EASYOFFICE_SHEET_UTILS__");
  for (const len of [0, 1, 2, 3, 4, 255, 4097]) {
    const src = new Uint8Array(len);
    for (let i = 0; i < len; i += 1) src[i] = (i * 73 + 19) & 0xff;
    const b64 = sheetUtils.bytesToBase64(src);
    const decoded = sheetUtils.base64ToBytes(b64);
    assert.deepEqual(Array.from(decoded), Array.from(src), `round-trip base64 for length ${len}`);
  }
});
