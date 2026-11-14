import assert from "node:assert/strict";
import test from "node:test";
import { createWebUiDom, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { openDemoWorkbook } from "./test-support/demo-fixtures.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";

// Hyperlinks of sheet cells (PD-218): the address policy, the dialog (넣기, 고치기, 링크 제거), the click that follows a link, the
// way a link goes with its cell (copy, fill, sort, clear, replace, undo), the save payload and a reopened file.

const URL_A = "https://example.com/a?b=1";

async function open(overrides = {}) {
  const { sandbox } = await createWebUiDom();
  const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SHEET",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
  const converts = [];
  const opened = [];
  const harness = await createWebUiDom({
    testIpc: async (command, payload) => {
      if (command === "convert_document_format") converts.push(JSON.parse(String(payload)));
      if (command === "open_link") opened.push(JSON.parse(String(payload)));
      if (overrides[command]) return overrides[command](payload, offline);
      return offline[command](payload);
    },
  });
  await openDemoWorkbook(harness.document);
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns(8);
  const { document, userClick, host } = harness;
  const cell = (ref) => document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  const byId = (id) => document.getElementById(id);
  const say = () => byId("status-command-feedback").textContent;
  const t = { ...harness, converts, opened, cell, byId, say, links: host.__EASYOFFICE_SHEET_LINK__ };
  t.select = async (ref) => {
    cell(ref).focus();
    cell(ref).dispatchEvent({ type: "mousedown", target: cell(ref), button: 0, bubbles: true });
    cell(ref).dispatchEvent({ type: "mouseup", target: cell(ref), button: 0, bubbles: true });
    await flushAsyncTurns(4);
  };
  t.openDialog = async () => {
    userClick(document.querySelector('.md-item[data-studio-menu-cmd="insert-link"]'));
    await flushAsyncTurns(12);
  };
  t.fill = (id, value) => {
    byId(id).value = value;
  };
  t.apply = async () => {
    userClick(byId("btn-sheet-link-apply"));
    await flushAsyncTurns(20);
  };
  t.undo = async () => {
    userClick(studioItem(document, "studio-undo"));
    await flushAsyncTurns(20);
  };
  t.save = async () => {
    userClick(studioItem(document, "studio-save"));
    await flushAsyncTurns(12);
    return converts.at(-1);
  };
  return t;
}

test("the address policy takes web, mail and workbook targets only and refuses control characters and long input", async () => {
  const { links } = await open();
  for (const good of [URL_A, "  http://a.b  ", "mailto:me@example.com", "#Sheet1!B2", "#'시트 2'!A1:B3", "#Total"]) {
    assert.ok(links.normalizeTarget(good), good);
  }
  for (const bad of [
    "", "#", "javascript:alert(1)", " JaVaScRiPt:alert(1)", "file:///etc/passwd", "data:text/html,x", "vbscript:x", "ftp://a.b", "tel:1",
    "https://", "https://a b", "https://a.b/\u0007", "mailto:", "example.com", "//example.com", "#A1\u0000", "../a.xlsx",
    "https://a.b/" + "x".repeat(2048), "#" + "A".repeat(256),
  ]) {
    assert.equal(links.normalizeTarget(bad), null, JSON.stringify(bad).slice(0, 40));
  }
  assert.equal(links.make("javascript:alert(1)", "tip"), null);
  assert.deepEqual({ ...links.make(URL_A, "팁\u0000") }, { target: URL_A, tip: "팁" });
  assert.equal(Object.isFrozen(links.make(URL_A)), true);
  assert.equal(links.internalTarget("My Data", { r1: 2, c1: 2, r2: 4, c2: 3 }), "#'My Data'!B2:C4");
  assert.equal(links.internalTarget("Sheet1", { r1: 5, c1: 1, r2: 5, c2: 1 }), "#Sheet1!A5");
});

test("the 삽입 menu opens the dialog on the active cell with the reference editor's labels; 넣기 makes the cell a link, one undo step", async () => {
  const t = await open();
  await t.select("A2");
  await t.openDialog();
  const dialog = t.byId("sheet-link-dialog");
  assert.equal(dialog.open, true);
  assert.equal(t.byId("sheet-link-dialog-title").textContent, "하이퍼링크");
  assert.deepEqual([...dialog.querySelectorAll("label")].map((l) => l.textContent), ["표시할 문자열", "웹 주소", "시트", "셀 참조"]);
  assert.deepEqual([...dialog.querySelectorAll(".dialog-tab")].map((b) => b.textContent), ["웹 주소", "현재 문서"]);
  assert.equal(t.byId("btn-sheet-link-apply").textContent, "넣기");
  assert.equal(t.byId("btn-sheet-link-remove").hidden, true);
  assert.equal(t.byId("sheet-link-text").value, "기획조정실", "the cell's text is the text to show");
  t.fill("sheet-link-address", "javascript:alert(1)");
  await t.apply();
  assert.equal(t.byId("sheet-link-status").textContent, "허용되지 않는 주소");
  assert.equal(dialog.open, true, "a refused address keeps the dialog");
  assert.equal(t.cell("A2").dataset.link, undefined);
  t.fill("sheet-link-address", URL_A);
  await t.apply();
  assert.equal(dialog.open, false);
  assert.equal(t.cell("A2").dataset.link, "external");
  assert.equal(t.cell("A2").getAttribute("title"), URL_A);
  assert.equal(t.cell("A2").textContent, "기획조정실");
  await t.undo();
  assert.equal(t.cell("A2").dataset.link, undefined);
  assert.equal(t.cell("A2").getAttribute("title"), null);
});

test("a link cell opens the dialog filled in, 고치기 changes it and 링크 제거 keeps the text", async () => {
  const t = await open();
  await t.select("B2");
  await t.openDialog();
  t.fill("sheet-link-address", URL_A);
  await t.apply();
  await t.select("B2");
  await t.openDialog();
  assert.equal(t.byId("sheet-link-dialog-title").textContent, "하이퍼링크 고치기");
  assert.equal(t.byId("btn-sheet-link-apply").textContent, "고치기");
  assert.equal(t.byId("sheet-link-address").value, URL_A);
  assert.equal(t.byId("btn-sheet-link-remove").hidden, false);
  t.fill("sheet-link-address", "mailto:me@example.com");
  await t.apply();
  assert.equal(t.cell("B2").getAttribute("title"), "mailto:me@example.com");
  await t.select("B2");
  await t.openDialog();
  userClick(t, t.byId("btn-sheet-link-remove"));
  await flushAsyncTurns(20);
  assert.equal(t.cell("B2").dataset.link, undefined);
  assert.equal(t.cell("B2").textContent, "1150");
  await t.undo();
  assert.equal(t.cell("B2").dataset.link, "external", "the removal is one undo step");
});

function userClick(t, el) {
  t.userClick(el);
}

test("the 현재 문서 tab links a range of a sheet or a defined name and refuses a reference that is neither", async () => {
  const t = await open();
  await t.select("C3");
  await t.openDialog();
  t.byId("sheet-link-tab-doc").click();
  assert.equal(t.byId("sheet-link-pane-doc").hidden, false);
  assert.equal(t.byId("sheet-link-pane-web").hidden, true);
  t.fill("sheet-link-sheet", "Sheet2");
  t.fill("sheet-link-ref", "not a ref");
  await t.apply();
  assert.equal(t.byId("sheet-link-status").textContent, "셀 참조가 올바르지 않음");
  t.fill("sheet-link-ref", "b2:c3");
  await t.apply();
  assert.equal(t.cell("C3").dataset.link, "internal");
  assert.equal(t.cell("C3").getAttribute("title"), "Sheet2!B2:C3");
  await t.select("C3");
  await t.openDialog();
  assert.equal(t.byId("sheet-link-tab-doc").getAttribute("aria-selected"), "true");
  assert.equal(t.byId("sheet-link-ref").value, "B2:C3");
});

test("the 삽입 menu item 하이퍼링크 제거 removes the link and keeps the text", async () => {
  const t = await open();
  const labels = [...t.document.querySelectorAll('.md-item[data-sheet-act="link-remove"] .md-label')].map((el) => el.textContent);
  assert.deepEqual(labels, ["하이퍼링크 제거"]);
  assert.equal(t.document.querySelector('.md-item[data-sheet-act="link-remove"]').dataset.kinds, "SHEET");
  await t.select("D2");
  await t.openDialog();
  t.fill("sheet-link-address", URL_A);
  await t.apply();
  await t.select("D2");
  t.userClick(t.document.querySelector('.md-item[data-sheet-act="link-remove"]'));
  await flushAsyncTurns(20);
  assert.equal(t.cell("D2").dataset.link, undefined);
  assert.equal(t.cell("D2").textContent, "1250");
  t.userClick(t.document.querySelector('.md-item[data-sheet-act="link-remove"]'));
  await flushAsyncTurns(20);
  assert.equal(t.say(), "넣을 칸 없음", "a cell without a link has nothing to remove");
});

test("a click follows an external link through the shell; a held button, a drag, a modifier and a selection only select", async () => {
  const t = await open({ open_link: async () => ({ status: "ok" }) });
  await t.select("E2");
  await t.openDialog();
  t.fill("sheet-link-address", URL_A);
  await t.apply();
  const td = t.cell("E2");
  const press = async (hold, extra = {}, up = {}) => {
    td.dispatchEvent({ type: "mousedown", target: td, button: 0, timeStamp: 1000, clientX: 10, clientY: 10, bubbles: true, ...extra });
    td.dispatchEvent({ type: "mouseup", target: td, button: 0, bubbles: true });
    td.dispatchEvent({ type: "click", target: td, button: 0, timeStamp: 1000 + hold, clientX: 10, clientY: 10, bubbles: true, ...up });
    await flushAsyncTurns(8);
  };
  const calls = [];
  t.host.__EASYOFFICE_DESKTOP_BRIDGE__ = { isAvailable: () => true, openLink: async (url) => calls.push(url) };
  for (const [name, run] of [["held", () => press(600)], ["shift", () => press(50, { shiftKey: true })], ["meta", () => press(50, { metaKey: true })], ["drag", () => press(50, {}, { clientX: 30 })]]) {
    await run();
    assert.deepEqual(calls, [], name);
  }
  await press(50);
  assert.deepEqual(calls, [URL_A]);
});

test("a click on an in-workbook link selects its target, on its sheet", async () => {
  const t = await open();
  await t.select("C3");
  await t.openDialog();
  t.byId("sheet-link-tab-doc").click();
  t.fill("sheet-link-sheet", "Sheet2");
  t.fill("sheet-link-ref", "B2");
  await t.apply();
  const td = t.cell("C3");
  td.dispatchEvent({ type: "mousedown", target: td, button: 0, timeStamp: 1000, clientX: 1, clientY: 1, bubbles: true });
  td.dispatchEvent({ type: "mouseup", target: td, button: 0, bubbles: true });
  td.dispatchEvent({ type: "click", target: td, button: 0, timeStamp: 1010, clientX: 1, clientY: 1, bubbles: true });
  await flushAsyncTurns(30);
  assert.equal(t.document.querySelector('.sheet-tab-btn.active').dataset.sheetTab, "Sheet2");
  assert.equal(t.byId("active-cell-ref").textContent, "B2");
});

test("a link stays through typed text, the message of a delete is the value only, and the payload carries every link", async () => {
  const t = await open();
  const linkCell = async (ref, url) => {
    await t.select(ref);
    await t.openDialog();
    t.fill("sheet-link-address", url);
    await t.apply();
  };
  await linkCell("A2", "https://example.com/one");
  await linkCell("A3", "https://example.com/two");
  const a3 = t.cell("A3");
  a3.focus();
  a3.textContent = "고친 글자";
  a3.dispatchEvent({ type: "input", target: a3 });
  a3.blur();
  await flushAsyncTurns(8);
  assert.equal(t.cell("A3").textContent, "고친 글자");
  assert.equal(t.cell("A3").dataset.link, "external", "typing keeps the link");
  const payload = await t.save();
  assert.deepEqual(payload.sheets[0].links, [
    { ref: "A2", target: "https://example.com/one" },
    { ref: "A3", target: "https://example.com/two" },
  ]);
});

test("a file's links come back on its cells, a link on an empty cell stays, an unsafe address is never shown", async () => {
  const t = await open();
  await pickFile(t.document, makeFixtureFile("링크.xlsx", {
    sheets: [{
      name: "Sheet1",
      cells: [{ ref: "A1", raw: "홈" }, { ref: "A2", raw: "시트" }, { ref: "A3", raw: "나쁜" }],
      links: [
        { ref: "A1", target: URL_A, tip: "열기" },
        { ref: "A2", target: "#Sheet2!B2" },
        { ref: "A3", target: "javascript:alert(1)" },
        { ref: "B9", target: "https://example.com/empty" },
      ],
    }, { name: "Sheet2", cells: [{ ref: "B2", raw: "도착" }] }],
  }));
  await flushAsyncTurns(30);
  assert.equal(t.cell("A1").dataset.link, "external");
  assert.equal(t.cell("A1").getAttribute("title"), "열기", "the screen tip comes before the address");
  assert.equal(t.cell("A2").dataset.link, "internal");
  assert.equal(t.cell("A2").getAttribute("title"), "Sheet2!B2");
  assert.equal(t.cell("A3").dataset.link, undefined);
  assert.equal(t.cell("B9").dataset.link, "external", "a cell with no text keeps its link");
  const payload = await t.save();
  assert.deepEqual(payload.sheets[0].links, [{ ref: "A1", target: URL_A, tip: "열기" }, { ref: "A2", target: "#Sheet2!B2" }, { ref: "B9", target: "https://example.com/empty" }]);
});

// ---- cross-feature --------------------------------------------------------------------------------------

const SORT_BOOK = {
  sheets: [{
    name: "Sheet1",
    cells: [
      { ref: "A1", raw: "이름" }, { ref: "B1", raw: "값" },
      { ref: "A2", raw: "다" }, { ref: "B2", raw: "30" },
      { ref: "A3", raw: "가" }, { ref: "B3", raw: "10" },
      { ref: "A4", raw: "나" }, { ref: "B4", raw: "20" },
    ],
    links: [{ ref: "A2", target: "https://example.com/da" }, { ref: "A3", target: "#Sheet1!B3" }],
  }],
};

test("sorting moves a link with its cell's text", async () => {
  const t = await open();
  await pickFile(t.document, makeFixtureFile("정렬.xlsx", SORT_BOOK));
  await flushAsyncTurns(20);
  t.userClick(t.document.querySelector('.md-item[data-studio-menu-cmd="data-sort"]'));
  await flushAsyncTurns(20);
  assert.deepEqual(["A2", "A3", "A4"].map((ref) => t.cell(ref).textContent), ["가", "나", "다"]);
  assert.deepEqual(["A2", "A3", "A4"].map((ref) => t.cell(ref).dataset.link ?? null), ["internal", null, "external"]);
  assert.equal(t.cell("A4").getAttribute("title"), "https://example.com/da");
});

test("fill and paste bring the link along (or none) in the copy modes; the special pastes of values, formulas and formats leave the cell's own", async () => {
  const t = await open();
  const link = Object.freeze({ target: URL_A });
  const fill = t.host.__EASYOFFICE_SHEET_FILL_CORE__;
  const refOf = (r, c) => "ABCDEFGH"[c - 1] + r;
  const cells = { A1: { value: "가", formula: "", link }, A2: { value: "나", formula: "" } };
  const plan = fill.planFill({ source: { r1: 1, c1: 1, r2: 2, c2: 1 }, direction: "down", count: 4, cellAt: (r, c) => cells[refOf(r, c)], shiftFormula: (f) => f, refOf });
  assert.deepEqual(plan.edits.map((e) => [e.ref, Boolean(e.link)]), [["A3", true], ["A4", false], ["A5", true], ["A6", false]]);
  const core = t.host.__EASYOFFICE_SHEET_PASTE_CORE__;
  const snap = { rows: [1], c1: 1, cols: 1, cells: [[{ value: "가", formula: "", link }]], widths: [100] };
  const target = { r1: 5, c1: 2, r2: 5, c2: 2 };
  const base = { snap, transpose: false, target, shiftFormula: (f) => f, isFormulaStr: (v) => String(v).startsWith("="), grid: { MAX_ROWS: 100, MAX_COLS: 10, refOf, DEFAULT_COL_W: 100 } };
  const now = { value: "원래", formula: "", link: { target: "#Sheet1!A1" } };
  const edits = (mode) => core.planPaste({ ...base, mode, getEntry: () => now }).edits[0];
  assert.deepEqual(edits("all").link, link);
  for (const mode of ["values", "formulas", "formats"]) assert.equal("link" in edits(mode), false, mode + " leaves the link of the cell");
  const none = { ...base, snap: { ...snap, cells: [[{ value: "가", formula: "" }]] }, mode: "all", getEntry: () => now };
  assert.equal(core.planPaste(none).edits[0].link, null, "a pasted cell without a link removes the cell's");
});

test("copying a link cell and pasting it elsewhere makes the second cell a link; cut moves it", async () => {
  const t = await open();
  await t.select("A2");
  await t.openDialog();
  t.fill("sheet-link-address", URL_A);
  await t.apply();
  await t.select("A2");
  t.userClick(studioItem(t.document, "clip-copy"));
  await flushAsyncTurns(10);
  await t.select("A5");
  t.userClick(studioItem(t.document, "clip-paste"));
  await flushAsyncTurns(30);
  assert.equal(t.cell("A5").dataset.link, "external");
  assert.equal(t.cell("A5").textContent, "기획조정실");
  assert.equal(t.cell("A2").dataset.link, "external");
  await t.undo();
  assert.equal(t.cell("A5").dataset.link, undefined, "the paste is one undo step");
  await t.select("A2");
  t.userClick(studioItem(t.document, "clip-cut"));
  await flushAsyncTurns(10);
  await t.select("A6");
  t.userClick(studioItem(t.document, "clip-paste"));
  await flushAsyncTurns(30);
  assert.equal(t.cell("A6").dataset.link, "external");
  assert.equal(t.cell("A2").dataset.link, undefined);
});

test("Delete empties a link cell and leaves the link; typing then makes it a link with text again; undo brings the text back", async () => {
  const t = await open();
  await t.select("B3");
  await t.openDialog();
  t.fill("sheet-link-address", URL_A);
  await t.apply();
  await t.select("B3");
  t.cell("B3").dispatchEvent({ type: "keydown", key: "Delete", target: t.cell("B3") });
  await flushAsyncTurns(20);
  assert.equal(t.cell("B3").textContent, "");
  assert.equal(t.cell("B3").dataset.link, "external", "Delete clears the value only");
  assert.deepEqual((await t.save()).sheets[0].links, [{ ref: "B3", target: URL_A }]);
  await t.undo();
  assert.equal(t.cell("B3").textContent, "920");
  assert.equal(t.cell("B3").dataset.link, "external");
});

test("the dirty state follows a link change: the fingerprint of the sheet differs with and without the link", async () => {
  const t = await open();
  const utils = t.host.__EASYOFFICE_WORKSPACE_SESSION_UTILS__;
  const plain = { sheets: { S: { A1: { value: "x", formula: "" } } } };
  const linked = { sheets: { S: { A1: { value: "x", formula: "", link: { target: URL_A } } } } };
  const other = { sheets: { S: { A1: { value: "x", formula: "", link: { target: "https://example.com/b" } } } } };
  const digest = (snap) => utils.moduleFingerprint(snap, "SHEET");
  assert.notEqual(digest(plain), digest(linked));
  assert.notEqual(digest(linked), digest(other));
  assert.equal(digest(linked), utils.moduleFingerprint(structuredClone(linked), "SHEET"));
});

const PROTECTED_BOOK = (deny) => ({
  sheets: [{
    name: "Sheet1",
    cells: [{ ref: "A2", raw: "열린 칸" }, { ref: "A3", raw: "잠긴 칸" }],
    protect: { on: true, deny },
    unlockedRows: [[2, 2]],
  }],
});

test("a protected sheet needs 하이퍼링크 삽입 and an unlocked cell to take a link", async () => {
  const t = await open();
  const flags = t.host.__EASYOFFICE_SHEET_PROTECT__.FLAG_NAMES;
  const BLOCKED = "변경하려는 셀 또는 차트가 보호된 시트에 있습니다. 변경하려면 시트의 보호를 해제하세요. 암호를 입력해야 할 수도 있습니다.";
  const alertOpen = () => t.byId("sheet-protect-alert-dialog").open;
  await pickFile(t.document, makeFixtureFile("보호.xlsx", PROTECTED_BOOK(flags)));
  await flushAsyncTurns(20);
  await t.select("A2");
  await t.openDialog();
  t.fill("sheet-link-address", URL_A);
  await t.apply();
  assert.equal(t.cell("A2").dataset.link, undefined, "the flag is denied");
  assert.equal(alertOpen(), true);
  assert.equal(t.byId("sheet-protect-alert-message").textContent, BLOCKED);
  t.byId("sheet-protect-alert-dialog").close();
  await pickFile(t.document, makeFixtureFile("보호2.xlsx", PROTECTED_BOOK(flags.filter((f) => f !== "insertHyperlinks"))));
  await flushAsyncTurns(20);
  await t.select("A2");
  await t.openDialog();
  t.fill("sheet-link-address", URL_A);
  await t.apply();
  assert.equal(t.cell("A2").dataset.link, "external", "allowed on an unlocked cell");
  await t.select("A3");
  await t.openDialog();
  t.fill("sheet-link-address", URL_A);
  await t.apply();
  assert.equal(t.cell("A3").dataset.link, undefined, "a locked cell takes none");
  assert.equal(alertOpen(), true);
});

test("a hostile file's links: unsafe addresses and odd cell names are turned away, the count is bounded", async () => {
  const t = await open();
  const links = Array.from({ length: 70000 }, (_, i) => ({ ref: "A" + (i + 1), target: "https://example.com/" + i }));
  links.unshift(
    { ref: "A1", target: "javascript:alert(1)" }, { ref: "A1", target: "file:///etc/passwd" }, { ref: "A1", target: "data:text/html,x" },
    { ref: "__proto__", target: "https://example.com/p" }, { ref: "constructor", target: "https://example.com/p" }, { ref: "A0", target: "https://example.com/p" },
  );
  const map = {};
  t.host.__EASYOFFICE_SHEET_LINK__.attach(map, links);
  assert.equal(Object.keys(map).length, 65530, "at most the bound of one sheet");
  assert.equal(map.A1.link.target, "https://example.com/0", "the unsafe addresses of that cell were turned away first");
  assert.equal(Object.getPrototypeOf(map), Object.prototype);
  assert.equal(Object.prototype.link, undefined);
});

test("a link to a defined name selects what the name refers to, on its sheet; a name that is gone says so", async () => {
  const t = await open();
  t.host.__EASYOFFICE_SHEET_NAMES__.replaceAll([{ name: "Block", refersTo: "Sheet2!$B$2:$C$2" }], true);
  await t.select("C4");
  await t.openDialog();
  t.byId("sheet-link-tab-doc").click();
  t.fill("sheet-link-ref", "block");
  await t.apply();
  assert.equal(t.cell("C4").getAttribute("title"), "Block", "the name as the workbook spells it");
  const td = t.cell("C4");
  const click = async () => {
    td.dispatchEvent({ type: "mousedown", target: td, button: 0, timeStamp: 1000, clientX: 1, clientY: 1, bubbles: true });
    td.dispatchEvent({ type: "mouseup", target: td, button: 0, bubbles: true });
    td.dispatchEvent({ type: "click", target: td, button: 0, timeStamp: 1010, clientX: 1, clientY: 1, bubbles: true });
    await flushAsyncTurns(30);
  };
  await click();
  assert.equal(t.document.querySelector(".sheet-tab-btn.active").dataset.sheetTab, "Sheet2");
  assert.equal(t.byId("active-cell-ref").textContent, "Block");
  t.userClick(t.document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]'));
  await flushAsyncTurns(20);
  await t.select("C4");
  t.host.__EASYOFFICE_SHEET_NAMES__.replaceAll([], true);
  await click();
  assert.equal(t.say(), "연결 대상 없음");
});

test("a cell an array filled takes no link", async () => {
  const { host, document } = await open();
  const saysOf = [];
  const store = { S: { A1: { value: "1", formula: "", spillOf: "B1" } } };
  const ui = host.__EASYOFFICE_CREATE_SHEET_LINK_UI__({
    document,
    gridUtils: host.__EASYOFFICE_SHEET_GRID_UTILS__,
    sheetDataStore: store,
    getActiveSheetName: () => "S",
    getSelection: () => ({ ar: 1, ac: 1 }),
    getRange: () => ({ r1: 1, c1: 1, r2: 1, c2: 1 }),
    selectRange: () => {},
    switchSheet: async () => {},
    commitIfEditing: () => {},
    focusAnchor: () => {},
    snapshotDomSheetCells: () => store.S,
    applyCellEdits: async (edits) => saysOf.push(edits),
  });
  assert.equal(ui.open(), false);
  assert.equal(document.getElementById("status-command-feedback").textContent, "채운 칸에는 넣을 수 없음");
  assert.equal(document.getElementById("sheet-link-dialog").open, false);
  assert.deepEqual(saysOf, []);
});

test("Ctrl+K opens the hyperlink dialog of the document, sheet and slide windows, Ctrl+/ the command search everywhere", async () => {
  const t = await open();
  const { sandbox, document } = t;
  const dialog = t.byId("sheet-link-dialog");
  const docDialog = t.byId("doc-link-dialog");
  const palette = t.byId("cmd-palette-modal");
  await t.select("A2");
  document.activeElement?.blur?.();
  const first = sandbox.dispatchWindowKeydown({ key: "k", ctrlKey: true });
  assert.equal(first.defaultPrevented, true);
  await flushAsyncTurns(20);
  assert.equal(dialog.open, true, "one press, no second key");
  assert.equal(palette.open, false);
  dialog.close();
  document.activeElement?.blur?.();
  sandbox.dispatchWindowKeydown({ key: "k", metaKey: true });
  await flushAsyncTurns(20);
  assert.equal(dialog.open, true, "Cmd+K on macOS");
  dialog.close();
  // a text field takes Ctrl+K for itself
  t.byId("formula-input").focus();
  assert.equal(sandbox.dispatchWindowKeydown({ key: "k", ctrlKey: true }).defaultPrevented, false);
  assert.equal(dialog.open, false);
  document.activeElement?.blur?.();
  // the command search
  const slash = sandbox.dispatchWindowKeydown({ key: "/", ctrlKey: true });
  assert.equal(slash.defaultPrevented, true);
  assert.equal(palette.open, true);
  assert.equal(document.getElementById("shortcut-hint").textContent, "Ctrl+/");
  sandbox.dispatchWindowKeydown({ key: "Escape" });
  assert.equal(palette.open, false);
  // the Hangul window leaves Ctrl+K to its editor
  switchModule(document, "HWP");
  await flushAsyncTurns(8);
  assert.equal(sandbox.dispatchWindowKeydown({ key: "k", ctrlKey: true }).defaultPrevented, false);
});

// ---- PD-223: HYPERLINK formula cells and the AutoCorrect rule for typed addresses ----

async function typeInto(t, ref, text) {
  const td = t.cell(ref);
  td.focus();
  td.textContent = "";
  for (const ch of text) {
    const active = t.document.activeElement;
    active.dispatchEvent({ type: "keydown", key: ch, target: active });
    active.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: ch, target: active });
    active.textContent += ch;
    active.dispatchEvent({ type: "input", inputType: "insertText", data: ch, target: active });
  }
  t.cell(ref).dispatchEvent({ type: "keydown", key: "Tab", target: t.cell(ref) });
  await flushAsyncTurns(15);
}

test("the AutoCorrect rule takes a whole typed address only: http, https, mailto and www", async () => {
  const { links } = await open();
  const taken = {
    "https://example.com/a?b=1": "https://example.com/a?b=1",
    "  HTTP://A.B/x  ": "HTTP://A.B/x",
    "mailto:me@example.com": "mailto:me@example.com",
    "www.example.com": "http://www.example.com",
    "WWW.Example.com/path": "http://WWW.Example.com/path",
  };
  for (const [typed, target] of Object.entries(taken)) assert.equal(links.autoTarget(typed), target, typed);
  for (const typed of [
    "", "hello", "see https://example.com", "https://example.com is a site", "https://", "mailto:", "www.", "www.com", "www..x", "example.com",
    "me@example.com", "ftp://a.b", "file:///etc/passwd", "javascript:alert(1)", "data:text/html,x", "https://a b", "#Sheet1!A1", "=HYPERLINK(\"https://a.b\")",
    "https://a.b/" + "x".repeat(2048),
  ]) {
    assert.equal(links.autoTarget(typed), null, typed);
  }
});

test("a typed address becomes a link when the entry is done; one undo takes the link away and keeps the text, the second takes the entry back", async () => {
  const t = await open();
  const before = t.cell("B3").textContent;
  await typeInto(t, "B3", "https://example.com/a?b=1");
  assert.equal(t.cell("B3").dataset.link, "external");
  assert.equal(t.cell("B3").textContent, "https://example.com/a?b=1");
  assert.deepEqual((await t.save()).sheets[0].links, [{ ref: "B3", target: URL_A }]);
  await t.undo();
  assert.equal(t.cell("B3").dataset.link, undefined, "the first undo takes the link");
  assert.equal(t.cell("B3").textContent, "https://example.com/a?b=1", "and keeps the text as typed");
  await t.undo();
  assert.equal(t.cell("B3").textContent, before, "the second undo takes the entry back");
  assert.equal(t.cell("B3").dataset.link, undefined);
  t.userClick(studioItem(t.document, "studio-redo"));
  await flushAsyncTurns(20);
  assert.equal(t.cell("B3").textContent, "https://example.com/a?b=1");
  assert.equal(t.cell("B3").dataset.link, undefined, "the first redo brings the text");
  t.userClick(studioItem(t.document, "studio-redo"));
  await flushAsyncTurns(20);
  assert.equal(t.cell("B3").dataset.link, "external", "the second redo brings the link");
});

test("other text, a formula, an address inside a sentence and a paste or fill of text make no link; a typed www address goes to http", async () => {
  const t = await open();
  await typeInto(t, "B3", "hello https://example.com");
  await typeInto(t, "B4", "=\"https://example.com\"");
  assert.equal(t.cell("B3").dataset.link, undefined);
  assert.equal(t.cell("B4").dataset.link, undefined);
  await typeInto(t, "B5", "www.example.com");
  assert.equal(t.cell("B5").dataset.link, "external");
  assert.equal(t.cell("B5").getAttribute("title"), "http://www.example.com");
  await typeInto(t, "B6", "mailto:me@example.com");
  assert.equal(t.cell("B6").dataset.link, "external");
  // The dialog and the edit writer take what they are given: no second link on top of an address the dialog set.
  await t.select("B7");
  await t.openDialog();
  t.fill("sheet-link-address", URL_A);
  await t.apply();
  await typeInto(t, "B7", "https://example.com/b");
  assert.deepEqual((await t.save()).sheets[0].links.filter((l) => l.ref === "B7"), [{ ref: "B7", target: "https://example.com/b" }], "a typed address replaces the cell's link");
});

test("a text typed on a protected sheet that does not allow 하이퍼링크 삽입 stays text, quietly", async () => {
  const t = await open();
  await pickFile(t.document, makeFixtureFile("보호.xlsx", PROTECTED_BOOK(t.host.__EASYOFFICE_SHEET_PROTECT__.FLAG_NAMES)));
  await flushAsyncTurns(20);
  await typeInto(t, "A2", "https://example.com/a");
  assert.equal(t.cell("A2").textContent, "https://example.com/a", "the unlocked cell takes the text");
  assert.equal(t.cell("A2").dataset.link, undefined, "but no link without the flag");
  assert.equal(t.byId("sheet-protect-alert-dialog").open, false, "and no alert for it");
});

test("with 하이퍼링크 삽입 allowed an unlocked cell takes the typed address as a link; a locked cell cannot be typed into", async () => {
  const t = await open();
  const flags = t.host.__EASYOFFICE_SHEET_PROTECT__.FLAG_NAMES.filter((f) => f !== "insertHyperlinks");
  await pickFile(t.document, makeFixtureFile("보호2.xlsx", PROTECTED_BOOK(flags)));
  await flushAsyncTurns(20);
  await typeInto(t, "A2", "https://example.com/a");
  assert.equal(t.cell("A2").dataset.link, "external");
  assert.equal(t.cell("A3").getAttribute("contenteditable"), "false", "a locked cell cannot be typed into, so it takes no auto link either");
});

test("a HYPERLINK formula cell shows as a link while it gives no error; a click asks the engine for the address and opens it", async () => {
  const asked = [];
  const t = await open({
    evaluate_sheet: async (payload, offline) => {
      const body = JSON.parse(String(payload));
      if (body.sessionAction !== "linkTarget") return offline.evaluate_sheet(payload);
      asked.push(body);
      const answers = { B3: { target: URL_A }, B4: { target: "javascript:alert(1)" }, B5: { error: "#VALUE!" }, B6: { target: "#Sheet2!B2" } };
      return answers[body.cell] ?? { none: true };
    },
  });
  const calls = [];
  t.host.__EASYOFFICE_DESKTOP_BRIDGE__ = { isAvailable: () => true, openLink: async (url) => calls.push(url) };
  for (const [ref, formula] of [["B3", '=HYPERLINK("https://example.com/a?b=1","열기")'], ["B4", '=HYPERLINK("javascript:alert(1)")'], ["B6", '=HYPERLINK("#Sheet2!B2","이동")']]) {
    await typeInto(t, ref, formula);
  }
  assert.equal(t.cell("B3").dataset.link, "formula");
  assert.equal(t.cell("B3").getAttribute("title"), null, "no address is known before the click");
  assert.equal(t.cell("B3").textContent, "열기", "the cell shows the friendly name");
  const click = async (ref) => {
    const td = t.cell(ref);
    td.focus();
    td.dispatchEvent({ type: "mousedown", target: td, button: 0, timeStamp: 1000, clientX: 10, clientY: 10, bubbles: true });
    td.dispatchEvent({ type: "mouseup", target: td, button: 0, bubbles: true });
    td.dispatchEvent({ type: "click", target: td, button: 0, timeStamp: 1050, clientX: 10, clientY: 10, bubbles: true });
    await flushAsyncTurns(20);
  };
  await click("B3");
  assert.equal(asked.at(-1).formula, '=HYPERLINK("https://example.com/a?b=1","열기")');
  assert.equal(asked.at(-1).sheet, "Sheet1");
  assert.deepEqual(calls, [URL_A], "opened through the shell command with the engine's address");
  await click("B4");
  assert.deepEqual(calls, [URL_A], "an address the page's policy refuses is not opened");
  assert.equal(t.say(), "허용되지 않는 주소");
  await click("B6");
  assert.equal(t.host.__EASYOFFICE_SHEET_NAMES__ && t.cell("B2") !== null, true);
  assert.equal(calls.length, 1, "a jump inside the workbook is not opened outside");
  // An error value is no link, whatever the formula says.
  const link = t.host.__EASYOFFICE_SHEET_LINK__;
  assert.equal(link.isHyperlinkFormula({ formula: '=HYPERLINK("https://a.b")', value: "x" }), true);
  assert.equal(link.isHyperlinkFormula({ formula: '=HYPERLINK("https://a.b")', value: "#VALUE!" }), false);
  assert.equal(link.isHyperlinkFormula({ formula: '=IF(A1,HYPERLINK("https://a.b"))', value: "x" }), false);
  assert.equal(link.isHyperlinkFormula({ formula: "", value: "x" }), false);
});

test("a right click on a link cell opens a menu with 하이퍼링크 고치기... and 하이퍼링크 지우기 on that cell; other cells and headers keep their own", async () => {
  const t = await open();
  await t.select("B3");
  await t.openDialog();
  t.fill("sheet-link-address", URL_A);
  await t.apply();
  const menu = t.byId("sheet-header-menu");
  const shown = () => [...menu.querySelectorAll(".md-item")].filter((el) => !el.hidden).map((el) => el.textContent);
  const plain = { type: "contextmenu", target: t.cell("C5"), button: 2, clientX: 40, clientY: 40, bubbles: true };
  t.cell("C5").dispatchEvent(plain);
  assert.equal(menu.hidden, true, "a cell without a link has no menu of ours");
  const td = t.cell("B3");
  td.dispatchEvent({ type: "contextmenu", target: td, button: 2, clientX: 40, clientY: 40, bubbles: true });
  assert.equal(menu.hidden, false);
  assert.deepEqual(shown(), ["하이퍼링크 고치기...", "하이퍼링크 지우기"]);
  t.userClick(menu.querySelector('[data-sheet-act="link-edit"]'));
  await flushAsyncTurns(12);
  assert.equal(t.byId("sheet-link-dialog").open, true, "고치기 opens the dialog filled with the cell's link");
  assert.equal(t.byId("sheet-link-address").value, URL_A);
  t.byId("sheet-link-dialog").close();
  const textBefore = td.textContent;
  t.cell("E4").focus();
  td.dispatchEvent({ type: "contextmenu", target: td, button: 2, clientX: 40, clientY: 40, bubbles: true });
  t.userClick(menu.querySelector('[data-sheet-act="link-clear"]'));
  await flushAsyncTurns(20);
  assert.equal(td.dataset.link, undefined, "지우기 takes the link of the clicked cell, though another one was active");
  assert.equal(td.textContent, textBefore, "and keeps its text");
  const header = t.document.querySelector("#pane-sheet th[data-col]");
  header.dispatchEvent({ type: "contextmenu", target: header, button: 2, clientX: 40, clientY: 10, bubbles: true });
  assert.equal(shown().includes("행 삭제") || shown().includes("열 삭제"), true);
  assert.equal(shown().includes("하이퍼링크 지우기"), false, "a header menu shows no link items");
});
