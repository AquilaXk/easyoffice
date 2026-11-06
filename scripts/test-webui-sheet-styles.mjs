import test from "node:test";
import assert from "node:assert/strict";
import { createWebUiDom, createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";

// Cell formatting and column widths in the save payload and on open (PD-089, #148).

async function createSaveHarness() {
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
  const converts = [];
  const harness = await createWorkspaceHarness({
    testIpc: async (command, payload) => {
      if (command === "convert_document_format") converts.push(JSON.parse(String(payload)));
      return offline[command](payload);
    },
  });
  harness.userClick(harness.document.querySelector('.mod-btn[data-module="SHEET"]'));
  await flushAsyncTurns();
  return { ...harness, converts };
}

const cell = (d, ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });

async function selectRange(d, from, keys) {
  cell(d, from).focus();
  for (const [k, n] of keys) for (let i = 0; i < n; i += 1) key(d.activeElement, k, { shiftKey: true });
  await flushAsyncTurns(2);
}

function dragColumn(document, col, dx) {
  const handle = document.querySelector(`#pane-sheet th[data-col="${col}"] .col-resizer`);
  handle.dispatchEvent({ type: "mousedown", target: handle, clientX: 200, button: 0 });
  const vp = document.getElementById("sheet-viewport");
  vp.dispatchEvent({ type: "mousemove", target: vp, clientX: 200 + dx });
  vp.dispatchEvent({ type: "mouseup", target: vp, clientX: 200 + dx });
}

async function saveSheet(h) {
  h.userClick(h.document.getElementById("qat-save"));
  await flushAsyncTurns(20);
  return h.converts.at(-1);
}

test("a bolded range and a widened column are sent in the sheets payload", async () => {
  const h = await createSaveHarness();
  const { document, sandbox } = h;
  await pickFile(document, makeFixtureFile("빈.xlsx", { sheets: [{ name: "빈", cells: [{ ref: "A1", raw: "x" }] }] }));
  await selectRange(document, "D5", [["ArrowRight", 1], ["ArrowDown", 1]]);
  sandbox.dispatchWindowKeydown({ key: "b", ctrlKey: true });
  await flushAsyncTurns(14);
  dragColumn(document, "C", 60);
  const save = await saveSheet(h);
  const sheet = save.sheets[0];
  const byRef = Object.fromEntries(sheet.cells.map((c) => [c.ref, c]));
  for (const ref of ["D5", "E5", "D6", "E6"]) {
    assert.deepEqual(byRef[ref].style, { bold: true }, `${ref} carries the bold style`);
    assert.equal(byRef[ref].raw, "", `${ref} is a styled blank cell`);
  }
  assert.equal(byRef.G5, undefined, "unformatted blank cells are not sent");
  assert.deepEqual(sheet.columns, [{ index: 3, widthPx: 156 }]);
});

test("italic, underline, strikethrough and size are sent as flags and points", async () => {
  const h = await createSaveHarness();
  const { document, sandbox } = h;
  cell(document, "A1").focus();
  await flushAsyncTurns(2);
  for (const k of ["i", "u"]) {
    sandbox.dispatchWindowKeydown({ key: k, ctrlKey: true });
    await flushAsyncTurns(14);
  }
  key(document.getElementById("sheet-viewport"), "5", { ctrlKey: true });
  await flushAsyncTurns(14);
  const save = await saveSheet(h);
  const a1 = save.sheets[0].cells.find((c) => c.ref === "A1");
  assert.equal(a1.style.italic, true);
  assert.equal(a1.style.underline, true);
  assert.equal(a1.style.strike, true);
  assert.equal(a1.style.bold, undefined);
});

test("sheets without formatting keep the old payload shape", async () => {
  const h = await createSaveHarness();
  const save = await saveSheet(h);
  for (const sheet of save.sheets) {
    assert.equal(sheet.columns, undefined);
    for (const c of sheet.cells) assert.deepEqual(Object.keys(c), ["ref", "raw"]);
  }
});

const STYLED_MODEL = {
  sheets: [
    {
      name: "서식",
      cells: [
        { ref: "A1", raw: "굵게", style: { bold: true } },
        { ref: "B1", raw: "=1+2", style: { italic: true, underline: true, strike: true, sizePt: 11.25 } },
        { ref: "C3", raw: "", style: { bold: true } },
        { ref: "D4", raw: "평범" },
      ],
      columns: [{ index: 1, widthPx: 150 }, { index: 3, widthPx: 60 }, { index: 0, widthPx: 90 }, { index: 2, widthPx: -5 }],
    },
    { name: "둘째", cells: [{ ref: "A1", raw: "x" }] },
  ],
};

test("opening a file applies cell styles, styled blank cells and column widths", async () => {
  const h = await createSaveHarness();
  const { document } = h;
  await pickFile(document, makeFixtureFile("서식.xlsx", STYLED_MODEL));
  assert.equal(cell(document, "A1").style.fontWeight, "600");
  const b1 = cell(document, "B1");
  assert.equal(b1.style.fontStyle, "italic");
  assert.match(b1.style.textDecoration, /underline/);
  assert.match(b1.style.textDecoration, /line-through/);
  assert.equal(b1.style.fontSize, "15px", "11.25 pt = 15 px");
  assert.equal(cell(document, "C3").style.fontWeight, "600", "styled blank cell");
  assert.equal(cell(document, "D4").style.fontWeight, "", "plain cell untouched");
  assert.equal(document.querySelector('#pane-sheet th[data-col="A"]').style.width, "150px");
  assert.equal(document.querySelector('#pane-sheet th[data-col="C"]').style.width, "60px");
  assert.equal(document.querySelector('#pane-sheet th[data-col="B"]').style.width, "96px", "invalid entries are skipped");
});

test("open -> edit -> save sends the file's formatting and widths again", async () => {
  const h = await createSaveHarness();
  const { document } = h;
  await pickFile(document, makeFixtureFile("서식.xlsx", STYLED_MODEL));
  cell(document, "D4").focus();
  await flushAsyncTurns(2);
  h.sandbox.dispatchWindowKeydown({ key: "b", ctrlKey: true });
  await flushAsyncTurns(14);
  const save = await saveSheet(h);
  const sheet = save.sheets.find((s) => s.name === "서식");
  const byRef = Object.fromEntries(sheet.cells.map((c) => [c.ref, c]));
  assert.deepEqual(byRef.A1.style, { bold: true });
  assert.deepEqual(byRef.B1.style, { italic: true, underline: true, strike: true, sizePt: 11.25 });
  assert.deepEqual(byRef.C3, { ref: "C3", raw: "", style: { bold: true } });
  assert.deepEqual(byRef.D4.style, { bold: true }, "edit after open is saved");
  assert.deepEqual(sheet.columns, [{ index: 1, widthPx: 150 }, { index: 3, widthPx: 60 }]);
  assert.equal(save.sheets.find((s) => s.name === "둘째").columns, undefined);
});

test("opening a different file drops the previous file's column widths", async () => {
  const h = await createSaveHarness();
  const { document } = h;
  await pickFile(document, makeFixtureFile("서식.xlsx", STYLED_MODEL));
  await pickFile(document, makeFixtureFile("다른.xlsx", { sheets: [{ name: "서식", cells: [{ ref: "A1", raw: "새" }] }] }));
  assert.equal(document.querySelector('#pane-sheet th[data-col="A"]').style.width, "96px");
  const save = await saveSheet(h);
  assert.equal(save.sheets[0].columns, undefined);
});
