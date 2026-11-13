import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

// The sheet's page setup (PD-213): the 쪽 설정 dialog with its 용지 and 시트 tabs, the print area commands, the limits of
// each field, one undo step each, and the setup kept in the layout store (and so in the save payload).

async function open() {
  const harness = await createWorkspaceHarness({
    transformHtml: demoHtml,
    tauriInvoke: async (command, args) => {
      if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
      if (command === "evaluate_sheet") {
        const body = JSON.parse(args.payload);
        return { open: { session: "s1" }, apply: { session: "s1", changed: [], circular: false }, close: { closed: true } }[body.sessionAction] ?? {};
      }
      return { status: "ok" };
    },
  });
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  const { document, userClick, host } = harness;
  const viewport = document.getElementById("sheet-viewport");
  viewport.clientHeight = 520;
  viewport.clientWidth = 960;
  const act = async (name) => {
    userClick(document.querySelector(`.md-item[data-sheet-act="${name}"]`));
    await flushAsyncTurns(20);
  };
  const field = (id) => document.getElementById("sp-" + id);
  const fire = (el) => el.dispatchEvent({ type: "change", target: el, bubbles: true });
  const type = (id, value) => {
    field(id).value = String(value);
    fire(field(id));
  };
  const core = host.__EASYOFFICE_SHEET_PAGE_CORE__;
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  const dialog = document.getElementById("sheet-page-dialog");
  const apply = async () => {
    document.getElementById("btn-sheet-page-apply").click();
    await flushAsyncTurns(20);
  };
  return { ...harness, act, field, fire, type, core, layout, dialog, apply, status: () => document.getElementById("sp-status").textContent };
}

test("쪽 설정 opens on the 용지 tab with a sheet's defaults: A4, portrait, the usual margins and 100%", async () => {
  const t = await open();
  await t.act("page-setup");
  assert.equal(t.dialog.open, true);
  assert.equal(t.field("paper").value, "9");
  assert.equal(t.field("portrait").checked, true);
  assert.deepEqual(["top", "bottom", "left", "right", "header", "footer"].map((k) => t.field(k).value), ["19.1", "19.1", "17.8", "17.8", "7.6", "7.6"]);
  assert.equal(t.field("scale-percent").checked, true);
  assert.equal(t.field("percent").value, "100");
  assert.equal(t.field("fit-width").disabled, true, "the fit fields wait for their choice");
  assert.equal(t.document.getElementById("sp-pane-paper").hidden, false);
  assert.equal(t.document.getElementById("sp-pane-sheet").hidden, true);
});

test("확인 stores the setup in the layout as one undo step, and undo brings the default back", async () => {
  const t = await open();
  await t.act("page-setup");
  t.type("paper", 8);
  t.field("landscape").checked = true;
  t.fire(t.field("landscape"));
  t.type("left", 10);
  t.field("scale-fit").checked = true;
  t.fire(t.field("scale-fit"));
  t.type("fit-width", 1);
  t.type("fit-height", 0);
  t.field("center-h").checked = true;
  t.document.querySelector('#sheet-page-dialog [data-sp-tab="sheet"]').click();
  t.type("print-area", "a1:d10, f15");
  t.type("title-rows", "1:2");
  t.type("title-cols", "A");
  t.field("grid").checked = true;
  t.field("headings").checked = true;
  await t.apply();
  assert.equal(t.dialog.open, false);
  assert.deepEqual(t.layout.pageOf("Sheet1"), {
    landscape: true, paper: 8, margins: { top: 750, bottom: 750, left: 394, right: 700, header: 300, footer: 300 },
    scale: { fitWidth: 1, fitHeight: 0 }, centerH: true, grid: true, headings: true, printArea: ["A1:D10", "F15"], titleRows: [1, 2], titleCols: [1, 1],
  });
  assert.deepEqual(t.layout.getAll().Sheet1.page, t.layout.pageOf("Sheet1"), "the layout JSON of the save payload carries it");
  assert.equal(t.document.getElementById("status-command-feedback").textContent, "쪽 설정 적용");
  t.userClick(studioItem(t.document, "studio-undo"));
  await flushAsyncTurns(20);
  assert.equal(t.layout.pageOf("Sheet1"), null);
});

test("a field that is wrong keeps the dialog open with its reason and changes nothing", async () => {
  const t = await open();
  for (const [id, value, message] of [
    ["percent", 9, "배율은 10에서 400 사이여야 합니다"],
    ["percent", 401, "배율은 10에서 400 사이여야 합니다"],
    ["percent", 12.5, "배율은 10에서 400 사이여야 합니다"],
    ["left", 600, "여백은 558.8mm 이하여야 합니다"],
    ["top", -1, "여백은 558.8mm 이하여야 합니다"],
  ]) {
    await t.act("page-setup");
    t.field(id).value = String(value);
    await t.apply();
    assert.equal(t.dialog.open, true, `${id} ${value}`);
    assert.equal(t.status(), message, `${id} ${value}`);
    t.dialog.close();
  }
  for (const [id, value, message] of [
    ["print-area", "A1:", "인쇄 영역이 올바르지 않습니다 (예: A1:D10, F15)"],
    ["print-area", "B2:A1:C3", "인쇄 영역이 올바르지 않습니다 (예: A1:D10, F15)"],
    ["print-area", Array.from({ length: 65 }, (_, i) => "A" + (i + 1)).join(","), "인쇄 영역이 올바르지 않습니다 (예: A1:D10, F15)"],
    ["title-rows", "3:2", "반복할 행이 올바르지 않습니다 (예: 1:2)"],
    ["title-rows", "1:257", "반복할 행이 올바르지 않습니다 (예: 1:2)"],
    ["title-rows", "1:5000", "반복할 행이 올바르지 않습니다 (예: 1:2)"],
    ["title-cols", "A:BM", "반복할 열이 올바르지 않습니다 (예: A:B)"],
    ["title-cols", "1", "반복할 열이 올바르지 않습니다 (예: A:B)"],
  ]) {
    await t.act("page-setup");
    t.field(id).value = value;
    await t.apply();
    assert.equal(t.dialog.open, true, `${id} ${value}`);
    assert.equal(t.status(), message, `${id} ${value}`);
    t.dialog.close();
  }
  await t.act("page-setup");
  t.field("scale-fit").checked = true;
  t.fire(t.field("scale-fit"));
  t.type("fit-width", 1001);
  await t.apply();
  assert.equal(t.status(), "맞출 쪽 수는 1000 이하여야 합니다");
  assert.equal(t.layout.pageOf("Sheet1"), null, "nothing was applied");
});

test("the limits sit exactly at the engine's bounds: 64 areas, 256 repeated rows, 64 repeated columns, 10 to 400 percent", async () => {
  const t = await open();
  const { core } = t;
  const sixtyFour = Array.from({ length: 64 }, (_, i) => "A" + (i + 1)).join(", ");
  assert.equal(core.parseRanges(sixtyFour).length, 64);
  assert.equal(core.parseRanges(sixtyFour + ", B1"), null);
  assert.deepEqual(core.parseLines("1:256", "row"), [1, 256]);
  assert.equal(core.parseLines("1:257", "row"), null);
  assert.deepEqual(core.parseLines("A:BL", "col"), [1, 64]);
  assert.equal(core.parseLines("A:BM", "col"), null);
  assert.deepEqual(core.parseRanges("$A$1:$B$2 ; c3"), ["A1:B2", "C3"]);
  assert.equal(core.parseRanges("A1:XFE5"), null);
  assert.equal(core.parseRanges("A0"), null);
  assert.deepEqual(core.parseLines("", "row"), []);
  assert.equal(core.fromFields({ ...core.toFields(null), percent: 400 }, null).page.scale.percent, 400);
  assert.equal(core.fromFields({ ...core.toFields(null), percent: 10 }, null).page.scale.percent, 10);
});

test("margins left as shown keep their exact thousandths of an inch; only an edited one changes", async () => {
  const t = await open();
  const { core } = t;
  const original = { margins: { left: 733, right: 700, top: 750, bottom: 750, header: 300, footer: 300 }, landscape: true };
  const same = core.fromFields(core.toFields(original), original).page;
  assert.deepEqual(same.margins, original.margins, "an untouched dialog does not round a file's margins");
  const edited = core.fromFields({ ...core.toFields(original), margins: { ...core.toFields(original).margins, top: 20 } }, original).page;
  assert.equal(edited.margins.left, 733);
  assert.equal(edited.margins.top, core.mmToMils(20));
  assert.equal(core.fromFields(core.toFields(null), null).page.margins, undefined, "the default margins are not written");
  assert.deepEqual(core.fromFields({ ...core.toFields(null), paper: 118 }, null).page.paper, 118, "a paper code of a file stays");
});

test("인쇄 영역 설정 makes the selection the print area and 해제 takes it away, each as one undo step", async () => {
  const t = await open();
  await t.act("print-area-set");
  const ref = t.layout.pageOf("Sheet1")?.printArea?.[0];
  assert.match(ref, /^[A-Z]+\d+(:[A-Z]+\d+)?$/);
  assert.equal(t.document.getElementById("status-command-feedback").textContent, "인쇄 영역 설정 " + ref);
  await t.act("print-area-clear");
  assert.equal(t.layout.pageOf("Sheet1"), null, "nothing else was set, so the setup is the default again");
  t.userClick(studioItem(t.document, "studio-undo"));
  await flushAsyncTurns(20);
  assert.deepEqual(t.layout.pageOf("Sheet1")?.printArea, [ref], "undo brings the area back");
  await t.act("print-area-clear");
  await t.act("print-area-clear");
  assert.equal(t.layout.pageOf("Sheet1"), null, "clearing without an area changes nothing");
});

test("the paper's size for printing follows the list and the orientation, A4 for a code it does not have", async () => {
  const { core } = await open();
  assert.deepEqual(core.paperMm(null), { width: 210, height: 297 });
  assert.deepEqual(core.paperMm({ paper: 8, landscape: true }), { width: 420, height: 297 });
  assert.deepEqual(core.paperMm({ paper: 1 }), { width: 215.9, height: 279.4 });
  assert.deepEqual(core.paperMm({ paper: 118 }), { width: 210, height: 297 });
});

test("the sheet's paper, orientation and margin commands are the page setup: no menu items of their own, one command search entry", async () => {
  const t = await open();
  const labels = [...t.document.querySelectorAll(".md-item[data-studio-menu-cmd]")].filter((el) => !el.closest("[hidden]")).map((el) => el.querySelector(".md-label").textContent);
  assert.equal(labels.includes("용지 방향 전환") || labels.includes("여백 전환"), false);
  const items = t.host.__EASYOFFICE_NATIVE_MENU_TABLE__.sheetMenuPaletteItems(t.document).map((i) => i.title);
  assert.ok(items.includes("쪽 설정…") && items.includes("인쇄 영역 설정") && items.includes("인쇄 영역 해제"));
});
