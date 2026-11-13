import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem, commandItem, createMemoryStorage } from "./test-support/modules.mjs";
import { demoHtml, openDemoWorkbook } from "./test-support/demo-fixtures.mjs";

// Sheet and workbook protection (PD-216): the 시트 보호, 통합 문서 보호 and 범위 편집 허용 commands of the 도구 menu with
// their dialogs, the 보호 pane of the cell format dialog, the passwords (only digests are kept), the refusal of every edit
// entry point on a protected sheet and its undo.

const SECRET = "비밀-pw-9";
// The digest the offline engine of the harness makes (a stand-in with the shape of the real one): the page keeps it, never the password.
const isDigest = (pwd) => pwd?.kind === "both" && /^OFFLINE-\d+-\d+$/.test(pwd.hash) && !JSON.stringify(pwd).includes(SECRET);

const BLOCKED = "변경하려는 셀 또는 차트가 보호된 시트에 있습니다. 변경하려면 시트의 보호를 해제하세요. 암호를 입력해야 할 수도 있습니다.";
// A refused edit opens the message window with the whole warning; it is closed for the next one.
function blocked(t) {
  const dialog = t.byId("sheet-protect-alert-dialog");
  assert.equal(dialog.open, true, "the message window is open");
  assert.equal(t.byId("sheet-protect-alert-message").textContent, BLOCKED);
  assert.equal(dialog.querySelectorAll(".dialog-footer button").length, 1, "one 확인 button");
  dialog.close();
}

// `engine`: a page whose engine answers the session actions (row and column edits), over the sample table written into the page.
async function open({ engine = false, localStorage } = {}) {
  const requests = [];
  const harness = await createWorkspaceHarness({
    ...(localStorage ? { localStorage } : {}),
    ...(engine
      ? {
          transformHtml: demoHtml,
          tauriInvoke: async (command, args) => {
            if (command === "get_workspace_bootstrap") return JSON.stringify({ platform: "macOS", shortcutModifier: "⌘", sheetSessions: true, openTabs: [] });
            if (command === "evaluate_sheet") {
              const body = JSON.parse(args.payload);
              requests.push(body);
              if (body.sessionAction === "structure") return { session: "s1", formulas: [], changed: [], circular: false, removed: 0, protect: body.protect };
              return { open: { session: "s1" }, apply: { session: "s1", changed: [], circular: false }, close: { closed: true } }[body.sessionAction] ?? {};
            }
            return { status: "ok" };
          },
        }
      : {}),
  });
  if (!engine) await openDemoWorkbook(harness.document);
  switchModule(harness.document, "SHEET");
  await flushAsyncTurns();
  const { document, userClick, host } = harness;
  const act = async (name) => {
    userClick(document.querySelector(`.md-item[data-sheet-act="${name}"]`));
    await flushAsyncTurns(20);
  };
  const byId = (id) => document.getElementById(id);
  const click = async (id) => {
    byId(id).click();
    await flushAsyncTurns(20);
  };
  const fire = (el) => el.dispatchEvent({ type: "change", target: el, bubbles: true });
  const check = (id, on) => {
    byId(id).checked = on;
    fire(byId(id));
  };
  const cell = (ref) => document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  const status = () => byId("status-command-feedback").textContent;
  const key = (el, k, extra = {}) => el.dispatchEvent({ type: "keydown", key: k, target: el, ...extra });
  const undo = async () => {
    userClick(studioItem(document, "studio-undo"));
    await flushAsyncTurns(20);
  };
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  const guard = host.__EASYOFFICE_SHEET_PROTECT__;
  return { ...harness, requests, act, byId, click, fire, check, cell, status, key, undo, layout, guard };
}

async function protectWithoutPassword(t, allowed = ["selectLockedCells", "selectUnlockedCells"]) {
  await t.act("sheet-protect");
  for (const flag of t.guard.FLAG_NAMES) t.check("sheet-protect-allow-" + flag, allowed.includes(flag));
  await t.click("btn-sheet-protect-apply");
}

test("the 도구 menu of the sheet window has the three protection commands, and none of the other windows has them", async () => {
  const t = await open();
  const items = ["sheet-protect", "workbook-protect", "range-allow"].map((act) => t.document.querySelector(`.md-item[data-sheet-act="${act}"]`));
  assert.deepEqual(items.map((el) => el.dataset.kinds), ["SHEET", "SHEET", "SHEET"]);
  assert.deepEqual(items.map((el) => el.textContent.trim()), ["시트 보호", "통합 문서 보호", "범위 편집 허용"]);
});

test("시트 보호 opens with the usual actions locked and the password fields waiting for 암호 사용", async () => {
  const t = await open();
  await t.act("sheet-protect");
  assert.equal(t.byId("sheet-protect-dialog").open, true);
  assert.equal(t.byId("sheet-protect-password").getAttribute("type"), "password");
  assert.equal(t.byId("sheet-protect-confirm").getAttribute("type"), "password");
  assert.equal(t.byId("sheet-protect-password").disabled, true);
  const allowed = t.guard.FLAG_NAMES.filter((flag) => t.byId("sheet-protect-allow-" + flag).checked);
  assert.deepEqual(allowed, ["selectLockedCells", "selectUnlockedCells"]);
  assert.deepEqual(
    t.guard.FLAGS.map(([, label]) => label),
    ["잠긴 셀 선택", "잠기지 않은 셀 선택", "셀 서식", "열 서식", "행 서식", "열 삽입", "행 삽입", "하이퍼링크 삽입", "열 삭제", "행 삭제", "정렬", "자동 필터 사용", "피벗 테이블 보고서 사용", "개체 편집", "시나리오 편집"],
  );
  t.check("sheet-protect-use-password", true);
  assert.equal(t.byId("sheet-protect-password").disabled, false);
});

test("설정 without a password protects the sheet, and the menu item then lifts it", async () => {
  const t = await open();
  await protectWithoutPassword(t);
  assert.equal(t.byId("sheet-protect-dialog").open, false);
  const protect = t.layout.protectOf("Sheet1");
  assert.equal(protect.on, true);
  assert.equal(protect.pwd, undefined);
  assert.deepEqual(protect.deny, t.guard.FLAG_NAMES.filter((f) => f !== "selectLockedCells" && f !== "selectUnlockedCells"));
  assert.deepEqual(t.layout.getAll().Sheet1.protect, protect, "the layout JSON of the save payload carries it");
  assert.equal(t.document.querySelector('[data-sheet-act="sheet-protect"] .md-label').textContent, "시트 보호 해제");
  await t.act("sheet-protect");
  assert.equal(t.layout.protectOf("Sheet1"), null, "no password: the same item lifts it at once");
  await t.undo();
  assert.equal(t.layout.protectOf("Sheet1"), null, "lifting it cannot be undone either");
});

test("the password must be typed twice alike and is kept only as the digest the engine made", async () => {
  const t = await open();
  await t.act("sheet-protect");
  t.check("sheet-protect-use-password", true);
  await t.click("btn-sheet-protect-apply");
  assert.equal(t.byId("sheet-protect-status").textContent, "암호를 입력");
  t.byId("sheet-protect-password").value = SECRET;
  t.byId("sheet-protect-confirm").value = SECRET + "x";
  await t.click("btn-sheet-protect-apply");
  assert.equal(t.byId("sheet-protect-status").textContent, "새 암호와 암호 확인이 일치하지 않습니다.");
  assert.equal(t.layout.protectOf("Sheet1"), null);
  t.byId("sheet-protect-password").value = "가".repeat(256);
  t.byId("sheet-protect-confirm").value = "가".repeat(256);
  await t.click("btn-sheet-protect-apply");
  assert.equal(t.byId("sheet-protect-status").textContent, "암호는 255자까지");
  t.byId("sheet-protect-password").value = SECRET;
  t.byId("sheet-protect-confirm").value = SECRET;
  await t.click("btn-sheet-protect-apply");
  const protect = t.layout.protectOf("Sheet1");
  assert.equal(isDigest(protect.pwd), true);
  assert.equal(t.byId("sheet-protect-password").value, "", "the field is cleared");
  assert.equal(t.byId("sheet-protect-confirm").value, "");
  // The password is nowhere the page keeps state: layout JSON, the undo snapshots, the status line.
  const kept = JSON.stringify([t.layout.getAll(), t.host.__EASYOFFICE_SHEET_PROTECT__.getWorkbook()]);
  assert.equal(kept.includes(SECRET), false);
  assert.equal(t.status().includes(SECRET), false);
  // Lifting asks the password in a 암호 dialog and the engine checks it.
  await t.act("sheet-protect");
  assert.equal(t.byId("sheet-password-dialog").open, true);
  assert.equal(t.byId("sheet-password-dialog-title").textContent, "암호");
  assert.equal(t.byId("sheet-password-input").getAttribute("type"), "password");
  t.byId("sheet-password-input").value = "틀린 암호";
  await t.click("btn-sheet-password-apply");
  assert.equal(t.byId("sheet-password-status").textContent, "암호가 일치하지 않습니다. 다시 입력하세요.");
  assert.equal(t.layout.protectOf("Sheet1").on, true);
  t.byId("sheet-password-input").value = SECRET;
  await t.click("btn-sheet-password-apply");
  assert.equal(t.layout.protectOf("Sheet1"), null);
  assert.equal(t.byId("sheet-password-input").value, "");
});

test("a protected sheet refuses typing into a locked cell, and says why", async () => {
  const t = await open();
  await protectWithoutPassword(t);
  assert.equal(t.layout.protectOf("Sheet1").on, true);
  const b3 = t.cell("B3");
  b3.focus();
  t.key(b3, "F2");
  assert.equal(b3.dataset.editMode, undefined, "F2 does not start an edit");
  blocked(t);
  assert.equal(b3.getAttribute("contenteditable"), "false");
  t.key(b3, "Delete");
  await flushAsyncTurns(20);
  assert.equal(t.cell("B3").textContent, "920", "Delete clears nothing");
  assert.equal(t.cell("B3").getAttribute("contenteditable"), "false");
  await t.act("sheet-protect");
  assert.equal(t.cell("B3").getAttribute("contenteditable"), "true", "lifting the protection makes the cells editable again");
  t.cell("B3").focus();
  t.key(t.cell("B3"), "Delete");
  await flushAsyncTurns(20);
  assert.equal(t.cell("B3").textContent, "", "an unprotected sheet clears it");
});

test("an unlocked cell, row or column of a protected sheet can be typed into and the rest cannot", async () => {
  const t = await open();
  const style = { unlocked: true };
  // The model of the sheet: B4 carries its own flag; column D and row 6 are unlocked as a whole.
  t.guard.setWorkbook(null);
  t.layout.setUnlocked("Sheet1", "col", [[4, 4]]);
  t.layout.setUnlocked("Sheet1", "row", [[6, 6]]);
  t.layout.setProtect("Sheet1", { on: true, deny: t.guard.FLAG_NAMES.filter((f) => !t.guard.DEFAULT_ALLOWED.includes(f)) });
  assert.equal(t.guard.cellEditable("Sheet1", 4, 2, style), true, "its own flag");
  assert.equal(t.guard.cellEditable("Sheet1", 5, 4), true, "an unlocked column");
  assert.equal(t.guard.cellEditable("Sheet1", 6, 1), true, "an unlocked row");
  assert.equal(t.guard.cellEditable("Sheet1", 3, 2), false);
  assert.equal(t.guard.cellEditable("Sheet1", 5, 4, { locked: true }), false, "a cell kept locked inside an unlocked column");
  assert.equal(t.guard.cellEditable("Sheet2", 3, 2), true, "another sheet is not protected");
  t.layout.setProtect("Sheet1", { on: false, deny: [] });
  assert.equal(t.guard.cellEditable("Sheet1", 3, 2), true, "a sheet that is not protected allows everything");
});

test("the allowed actions decide which edits go through", async () => {
  const t = await open();
  const guard = t.guard;
  const deny = guard.FLAG_NAMES.filter((f) => !guard.DEFAULT_ALLOWED.includes(f));
  t.layout.setProtect("Sheet1", { on: true, deny });
  for (const flag of ["formatCells", "insertRows", "deleteColumns", "sort", "autoFilter", "pivotTables"]) {
    assert.equal(guard.allows("Sheet1", flag), false, flag);
    assert.equal(guard.guardAction("Sheet1", flag), false, flag);
    blocked(t);
  }
  t.layout.setProtect("Sheet1", { on: true, deny: deny.filter((f) => f !== "insertRows" && f !== "sort") });
  assert.equal(guard.allows("Sheet1", "insertRows"), true);
  assert.equal(guard.allows("Sheet1", "sort"), true);
  assert.equal(guard.allows("Sheet1", "deleteRows"), false);
  // Selecting is a flag of its own.
  t.layout.setProtect("Sheet1", { on: true, deny: ["selectLockedCells"] });
  assert.equal(guard.canSelect("Sheet1", 2, 2, {}), false);
  assert.equal(guard.canSelect("Sheet1", 2, 2, { unlocked: true }), true);
});

test("the structure commands of the sheet refuse an edit the sheet locks, before and after the engine", async () => {
  const t = await open({ engine: true });
  const deny = t.guard.FLAG_NAMES.filter((f) => !t.guard.DEFAULT_ALLOWED.includes(f) && f !== "insertRows");
  t.layout.setProtect("Sheet1", { on: true, deny });
  t.cell("C2").focus();
  await t.act("row-delete");
  blocked(t);
  await t.act("col-insert-left");
  assert.equal(t.requests.filter((r) => r.sessionAction === "structure").length, 0, "nothing reached the engine");
  await t.act("row-insert-above");
  const sent = t.requests.filter((r) => r.sessionAction === "structure");
  assert.equal(sent.length, 1, "an allowed insert goes to the engine");
  assert.equal(sent[0].protect.on, true, "with the protection, so the engine checks it too");
  t.layout.setProtect("Sheet1", { on: true, deny: [...deny, "formatRows"] });
  const rowHeader = t.document.querySelector('#pane-sheet th[data-row="2"]');
  rowHeader?.dispatchEvent({ type: "mousedown", button: 0, target: rowHeader });
  await t.act("hide");
  assert.equal(t.layout.rowHidden("Sheet1", 2), false, "hiding rows needs 행 서식");
});

test("a workbook whose structure is protected refuses sheet commands and its password lifts it", async () => {
  const t = await open();
  await t.act("workbook-protect");
  assert.equal(t.byId("sheet-wb-protect-dialog").open, true);
  assert.equal(t.byId("sheet-wb-protect-password").getAttribute("type"), "password");
  t.byId("sheet-wb-protect-password").value = SECRET;
  t.byId("sheet-wb-protect-confirm").value = SECRET;
  await t.click("btn-sheet-wb-protect-apply");
  const book = t.guard.getWorkbook();
  assert.equal(book.structure, true);
  assert.equal(isDigest(book.pwd), true);
  assert.equal(t.document.querySelector('[data-sheet-act="workbook-protect"] .md-label').textContent, "통합 문서 보호 해제");
  const tabs = ["tab-insert", "tab-delete", "tab-copy", "tab-hide"];
  for (const name of tabs) {
    await t.act(name);
    assert.equal(t.status(), "통합 문서 보호", name);
  }
  assert.equal(t.document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]").length, 2, "no sheet was added or removed");
  await t.undo();
  assert.equal(t.guard.getWorkbook().structure, true, "protecting the workbook cannot be undone");
  t.guard.setWorkbook(null);
  await t.act("workbook-protect");
  await t.click("btn-sheet-wb-protect-apply");
  assert.equal(t.byId("sheet-wb-protect-status").textContent, "암호를 입력");
  t.check("sheet-wb-protect-none", true);
  assert.equal(t.byId("sheet-wb-protect-password").disabled, true);
  await t.click("btn-sheet-wb-protect-apply");
  assert.deepEqual(t.guard.getWorkbook(), { structure: true, windows: false });
  await t.act("workbook-protect");
  assert.equal(t.guard.getWorkbook(), null, "without a password the same item lifts it");
});

test("the 보호 pane of the cell format dialog sets 잠금 and 숨기기 and a protected sheet does not let them change", async () => {
  const t = await open();
  const pane = t.byId("sheet-fmt-pane-protect");
  assert.equal(t.document.querySelector('#sheet-numfmt-dialog [data-fmt-tab="protect"]').textContent, "보호");
  assert.deepEqual([...pane.querySelectorAll("label")].map((l) => l.textContent.trim()), ["잠금", "숨기기"]);
  t.cell("B4").focus();
  await t.act("dialog-protect").catch(() => {});
  const look = t.host.__EASYOFFICE_SHEET_LOOK__;
  assert.ok(look.LOOK_KEYS.includes("unlocked") && look.LOOK_KEYS.includes("hideFormula"));
  const codec = t.host.__EASYOFFICE_SHEET_STYLE_CODEC__;
  const style = codec.styleFromPayload({ unlocked: true, hideFormula: true });
  assert.equal(style.unlocked, true);
  assert.equal(style.hideFormula, true);
  assert.deepEqual(codec.styleToPayload(style), { unlocked: true, hideFormula: true });
  assert.deepEqual(codec.styleToPayload(codec.styleFromPayload({ locked: true })), { locked: true });
});

test("a hidden formula is not shown in the formula bar of a protected sheet and not copied", async () => {
  const t = await open();
  const guard = t.guard;
  const style = { hideFormula: true };
  assert.equal(guard.formulaHidden("Sheet1", 2, 6, style), false, "only while the sheet is protected");
  t.layout.setProtect("Sheet1", { on: true, deny: [] });
  assert.equal(guard.formulaHidden("Sheet1", 2, 6, style), true);
  assert.equal(guard.formulaHidden("Sheet1", 2, 6, {}), false);
});

test("범위 편집 허용 lists the ranges of an unprotected sheet and a range without a password is edited by anyone", async () => {
  const t = await open();
  await t.act("range-allow");
  assert.equal(t.byId("sheet-ranges-dialog").open, true);
  await t.click("btn-sheet-ranges-new");
  assert.equal(t.byId("sheet-range-edit-title").value, "범위1");
  assert.equal(t.byId("sheet-range-edit-password").getAttribute("type"), "password");
  t.byId("sheet-range-edit-cells").value = "zz";
  await t.click("btn-sheet-range-edit-apply");
  assert.equal(t.byId("sheet-range-edit-status").textContent, "셀 범위가 올바르지 않음");
  t.byId("sheet-range-edit-cells").value = "b2:c3, e5";
  await t.click("btn-sheet-range-edit-apply");
  const ranges = t.layout.protectOf("Sheet1").ranges;
  assert.deepEqual(ranges, [{ name: "범위1", sqref: ["B2:C3", "E5"] }]);
  assert.equal(t.layout.protectOf("Sheet1").on, false, "the ranges are kept on a sheet that is not protected");
  t.byId("sheet-ranges-dialog").close();
  await protectWithoutPassword(t);
  assert.equal(t.guard.cellEditable("Sheet1", 2, 2), true, "inside the range");
  assert.equal(t.guard.cellEditable("Sheet1", 5, 5), true);
  assert.equal(t.guard.cellEditable("Sheet1", 4, 4), false, "outside it");
  assert.equal(t.layout.protectOf("Sheet1").ranges.length, 1, "protecting keeps the ranges");
  await t.act("range-allow");
  assert.equal(t.byId("sheet-ranges-dialog").open, false, "a protected sheet keeps its ranges as they are");
  blocked(t);
});

test("a range with a password is opened by it, once, and only for this window", async () => {
  const t = await open();
  await t.act("range-allow");
  await t.click("btn-sheet-ranges-new");
  t.byId("sheet-range-edit-cells").value = "B2:C3";
  t.byId("sheet-range-edit-password").value = SECRET;
  t.byId("sheet-range-edit-confirm").value = SECRET + "x";
  await t.click("btn-sheet-range-edit-apply");
  assert.equal(t.byId("sheet-range-edit-status").textContent, "새 암호와 암호 확인이 일치하지 않습니다.");
  t.byId("sheet-range-edit-confirm").value = SECRET;
  await t.click("btn-sheet-range-edit-apply");
  assert.equal(isDigest(t.layout.protectOf("Sheet1").ranges[0].pwd), true);
  t.byId("sheet-ranges-dialog").close();
  await protectWithoutPassword(t);
  assert.equal(t.guard.cellEditable("Sheet1", 2, 2), false, "a range with a password is locked until it is typed");
  const b2 = t.cell("B2");
  b2.focus();
  t.key(b2, "F2");
  await flushAsyncTurns(20);
  assert.equal(t.byId("sheet-password-dialog").open, true);
  assert.equal(t.byId("sheet-password-dialog-title").textContent, "범위 잠금 해제");
  t.byId("sheet-password-input").value = SECRET;
  await t.click("btn-sheet-password-apply");
  assert.equal(t.guard.cellEditable("Sheet1", 2, 2), true);
  assert.equal(JSON.stringify(t.layout.getAll()).includes(SECRET), false);
  t.guard.forgetUnlocked();
  assert.equal(t.guard.cellEditable("Sheet1", 2, 2), false, "a file opened again forgets it");
});

// ---- every edit entry point on a protected sheet --------------------------------------------------------------------

async function protectedSheet(allowed) {
  const t = await open();
  await protectWithoutPassword(t, allowed);
  return t;
}

// Emulates what the browser does after an un-prevented printable keydown on a focused editable cell.
function typeText(t, text) {
  for (const ch of text) {
    const td = t.document.activeElement;
    t.key(td, ch);
    td.dispatchEvent({ type: "beforeinput", inputType: "insertText", data: ch, target: td });
    td.textContent = td.textContent + ch;
    td.dispatchEvent({ type: "input", inputType: "insertText", data: ch, target: td });
  }
}

const cellText = (t, ref) => t.cell(ref).textContent;
const SNAP = ["B2", "B3", "C2", "F2", "A2"];
const snapshot = (t) => SNAP.map((ref) => cellText(t, ref));

test("typing, F2 and a double click start no edit in a locked cell, and typing goes into an unlocked one", async () => {
  const t = await open();
  const before = snapshot(t);
  // B4 is unlocked on the sheet that is not protected yet: the 보호 pane writes the flag.
  t.cell("B4").focus();
  t.userClick(t.document.querySelector('[data-sheet-numfmt="dialog"]'));
  await flushAsyncTurns(20);
  t.document.querySelector('#sheet-numfmt-dialog [data-fmt-tab="protect"]').click();
  assert.equal(t.byId("sheet-protect-cell-locked").checked, true, "cells are locked by default");
  t.check("sheet-protect-cell-locked", false);
  await t.click("btn-sheet-numfmt-apply");
  await protectWithoutPassword(t);
  const b3 = t.cell("B3");
  b3.focus();
  t.key(b3, "9");
  assert.equal(cellText(t, "B3"), before[1], "typing into a locked cell changes nothing");
  assert.equal(b3.dataset.editMode, undefined, "and starts no edit");
  blocked(t);
  b3.dispatchEvent({ type: "dblclick", target: b3, bubbles: true });
  assert.equal(t.cell("B3").dataset.editMode, undefined);
  const b4 = t.cell("B4");
  assert.equal(b4.getAttribute("contenteditable"), "true");
  b4.focus();
  typeText(t, "77");
  t.key(b4, "Enter");
  await flushAsyncTurns(12);
  assert.equal(cellText(t, "B4"), "77", "an unlocked cell takes the typing");
  assert.equal(t.layout.protectOf("Sheet1").on, true);
});

test("paste, cut, fill and the formula bar leave a locked cell as it is, and copy still works", async () => {
  const t = await protectedSheet();
  const before = snapshot(t);
  const store = {};
  const clip = { setData: (type, v) => (store[type] = v), getData: (type) => store[type] || "" };
  const b2 = t.cell("B2");
  b2.focus();
  assert.equal(b2.dispatchEvent({ type: "copy", target: b2, clipboardData: clip }), false, "copy is handled");
  assert.equal(store["text/plain"], "1150");
  for (const type of ["cut", "paste"]) {
    t.cell("B3").focus();
    t.cell("B3").dispatchEvent({ type, target: t.cell("B3"), clipboardData: type === "cut" ? { setData() {}, getData: () => "" } : clip });
    await flushAsyncTurns(15);
    assert.deepEqual(snapshot(t), before, type);
  }
  blocked(t);
  // The fill handle.
  t.cell("B2").dispatchEvent({ type: "mousedown", target: t.cell("B2"), button: 0, bubbles: true });
  t.cell("B2").dispatchEvent({ type: "mouseup", target: t.cell("B2"), bubbles: true });
  const handle = t.document.querySelector("#sheet-viewport .sheet-fill-handle");
  handle?.dispatchEvent({ type: "mousedown", target: handle, button: 0, bubbles: true });
  t.cell("B4").dispatchEvent({ type: "mouseover", target: t.cell("B4"), bubbles: true });
  t.cell("B4").dispatchEvent({ type: "mouseup", target: t.cell("B4"), bubbles: true });
  await flushAsyncTurns(15);
  assert.deepEqual(snapshot(t), before, "fill");
  // The formula bar writes into the active cell.
  t.cell("B3").focus();
  const bar = t.byId("formula-input");
  bar.value = "5";
  t.byId("btn-eval-formula").click();
  await flushAsyncTurns(15);
  assert.deepEqual(snapshot(t), before, "formula bar");
  blocked(t);
});

test("format commands, sorting, the filter, conditional formatting and the pivot are refused until the sheet allows them", async () => {
  const t = await protectedSheet();
  t.cell("B2").focus();
  for (const act of [() => t.userClick(commandItem(t.document, "sheet-home-bold")), () => t.userClick(t.document.querySelector('[data-sheet-look="align-center"]'))]) {
    act();
    await flushAsyncTurns(15);
    blocked(t);
    assert.equal(t.cell("B2").style.fontWeight || "", "");
    assert.equal(t.cell("B2").style.textAlign || "", "");
  }
  const order = snapshot(t);
  t.cell("B2").focus();
  for (const cmd of ["sheet-data-sort", "sheet-mod-condfmt", "sheet-mod-pivot"]) {
    t.userClick(commandItem(t.document, cmd));
    await flushAsyncTurns(15);
  }
  assert.deepEqual(snapshot(t), order, "sort");
  assert.equal(t.byId("sheet-cf-dialog")?.open ?? false, false, "the conditional formatting dialog stays closed or its changes are refused");
  await t.act("filter-toggle");
  assert.equal(t.layout.filterOf("Sheet1"), null, "no filter on a protected sheet");
  blocked(t);
  // 셀 서식 allowed: bold goes through.
  t.layout.setProtect("Sheet1", { on: true, deny: t.guard.FLAG_NAMES.filter((f) => !["selectLockedCells", "selectUnlockedCells", "formatCells"].includes(f)) });
  t.cell("B2").focus();
  t.userClick(commandItem(t.document, "sheet-home-bold"));
  await flushAsyncTurns(15);
  assert.equal(t.cell("B2").style.fontWeight, "600");
});

test("an undo or a redo does not change a locked cell of a sheet that stays protected", async () => {
  const t = await open();
  // A range with a password on a protected sheet, opened in this window: B2 is edited through it.
  const deny = t.guard.FLAG_NAMES.filter((f) => !t.guard.DEFAULT_ALLOWED.includes(f));
  const range = { name: "r", sqref: ["B2"], pwd: { kind: "both", alg: "SHA-512", hash: "OFFLINE-1-1", salt: "S", spin: 1, key: "K" } };
  t.layout.setProtect("Sheet1", { on: true, deny, ranges: [range] });
  t.guard.unlockRange("Sheet1", "r");
  t.host.__EASYOFFICE_SHEET_GRID__?.refreshCells?.();
  const edit = t.cell("B2");
  edit.focus();
  typeText(t, "5");
  t.key(edit, "Enter");
  await flushAsyncTurns(15);
  assert.equal(cellText(t, "B2"), "5");
  // The window forgets the password: B2 is locked again.
  t.guard.forgetUnlocked();
  await t.undo();
  assert.equal(cellText(t, "B2"), "5", "the undo is refused: B2 is locked");
  blocked(t);
  t.layout.setProtect("Sheet1", null);
  await t.undo();
  assert.equal(cellText(t, "B2"), "1150", "on a sheet that is not protected it goes through");
});

test("a locked cell cannot be selected when the sheet keeps selection from locked cells, and the arrows and Tab skip it", async () => {
  const t = await open();
  t.layout.setUnlocked("Sheet1", "col", [[3, 3]]);
  t.layout.setProtect("Sheet1", { on: true, deny: ["selectLockedCells", "formatCells"] });
  const b5 = t.cell("B5");
  b5.dispatchEvent({ type: "mousedown", target: b5, button: 0, bubbles: true });
  assert.notEqual(t.byId("active-cell-ref").textContent, "B5", "a click on a locked cell selects nothing");
  const c2 = t.cell("C2");
  c2.dispatchEvent({ type: "mousedown", target: c2, button: 0, bubbles: true });
  assert.equal(t.byId("active-cell-ref").textContent, "C2", "an unlocked cell is selected");
  t.key(t.document.activeElement, "Tab");
  assert.equal(t.byId("active-cell-ref").textContent, "C2", "Tab has no unlocked cell to go to in the row, so it stays");
  t.key(t.document.activeElement, "ArrowDown");
  assert.equal(t.byId("active-cell-ref").textContent, "C3", "the arrow goes to the next unlocked cell");
});

test("a hidden formula shows no formula in the formula bar of a protected sheet and a copy carries the value", async () => {
  const t = await open();
  t.cell("F2").focus();
  assert.equal(t.byId("formula-input").value, "=SUM(B2:E2)");
  t.userClick(t.document.querySelector('[data-sheet-numfmt="dialog"]'));
  await flushAsyncTurns(20);
  t.document.querySelector('#sheet-numfmt-dialog [data-fmt-tab="protect"]').click();
  t.check("sheet-protect-cell-hidden", true);
  await t.click("btn-sheet-numfmt-apply");
  assert.equal(t.byId("formula-input").value, "=SUM(B2:E2)", "the sheet is not protected yet");
  await protectWithoutPassword(t);
  t.cell("F2").focus();
  assert.equal(t.byId("formula-input").value, "4950");
  const store = {};
  const f2 = t.cell("F2");
  f2.dispatchEvent({ type: "copy", target: f2, clipboardData: { setData: (type, v) => (store[type] = v), getData: (type) => store[type] || "" } });
  assert.equal(store["text/plain"], "4950", "the copy has the value, not the formula");
  await t.act("sheet-protect");
  t.cell("F2").focus();
  assert.equal(t.byId("formula-input").value, "=SUM(B2:E2)");
});

// ---- the protection is not undone, and the record of steps ends where it changes -----------------------------------------
test("protecting, lifting and the ranges end the record of undo steps, and no undo or redo changes a protection or its password", async () => {
  const t = await open();
  // An edit while the sheet is not protected is an undo step.
  const b3 = t.cell("B3");
  b3.focus();
  typeText(t, "5");
  t.key(b3, "Enter");
  await flushAsyncTurns(15);
  assert.equal(cellText(t, "B3"), "5");
  t.check("sheet-protect-use-password", false);
  await t.act("sheet-protect");
  t.check("sheet-protect-use-password", true);
  t.byId("sheet-protect-password").value = SECRET;
  t.byId("sheet-protect-confirm").value = SECRET;
  await t.click("btn-sheet-protect-apply");
  const protect = t.layout.protectOf("Sheet1");
  assert.equal(isDigest(protect.pwd), true);
  // The record is empty: undo and redo change neither the locked cell nor the protection nor the password.
  for (const history of ["studio-undo", "studio-redo", "studio-undo"]) {
    t.userClick(studioItem(t.document, history));
    await flushAsyncTurns(20);
    assert.deepEqual(t.layout.protectOf("Sheet1"), protect, history);
    assert.equal(cellText(t, "B3"), "5", history);
  }
  // The workbook and the ranges end the record in the same way.
  await t.act("workbook-protect");
  t.byId("sheet-wb-protect-password").value = SECRET;
  t.byId("sheet-wb-protect-confirm").value = SECRET;
  await t.click("btn-sheet-wb-protect-apply");
  const book = t.guard.getWorkbook();
  t.userClick(studioItem(t.document, "studio-undo"));
  await flushAsyncTurns(20);
  assert.deepEqual(t.guard.getWorkbook(), book);
  assert.deepEqual(t.layout.protectOf("Sheet1"), protect);
});

test("a step recorded before still cannot change the protection: a restore keeps the protection and the password of the sheets it meets", async () => {
  const t = await open();
  const deny = t.guard.FLAG_NAMES.filter((f) => !t.guard.DEFAULT_ALLOWED.includes(f));
  const pwd = { kind: "both", alg: "SHA-512", hash: "OFFLINE-1-1", salt: "S", spin: 1, key: "K" };
  t.layout.setProtect("Sheet1", { on: true, deny, pwd });
  const old = { sheetLayout: {}, sheetOrder: ["Sheet1", "Sheet2"], sheets: { Sheet1: { B3: { value: "1", formula: "" } } } };
  const kept = t.guard.keepProtection(old, ["Sheet1", "Sheet2"]);
  assert.deepEqual(kept.Sheet1.protect, t.layout.protectOf("Sheet1"), "a snapshot of a state without it gets the protection back");
  assert.equal(kept.Sheet2, undefined);
  // A sheet the snapshot brings back keeps the snapshot's own.
  const back = t.guard.keepProtection({ sheetLayout: { Gone: { protect: { on: true, deny: [] } } }, sheetOrder: ["Gone"] }, ["Sheet1"]);
  assert.equal(back.Gone.protect.on, true);
  // A restore that would change a locked cell is refused.
  const store = { Sheet1: { B3: { value: "9", formula: "" } } };
  assert.equal(t.guard.restoreWouldChange({ sheets: { Sheet1: { B3: { value: "1", formula: "" } } } }, store), true);
  assert.equal(t.guard.restoreWouldChange({ sheets: { Sheet1: { B3: { value: "9", formula: "" } } } }, store), false);
  assert.equal(t.guard.restoreWouldChange({ sheets: undefined }, store), false, "a snapshot of the layout alone changes no cell");
});

test("a refused edit opens the message window once, with the whole warning and one button", async () => {
  const t = await open();
  await protectWithoutPassword(t);
  const b3 = t.cell("B3");
  b3.focus();
  t.key(b3, "F2");
  const dialog = t.byId("sheet-protect-alert-dialog");
  assert.equal(dialog.open, true);
  assert.equal(t.byId("sheet-protect-alert-message").textContent, BLOCKED);
  let opens = 0;
  const show = dialog.showModal;
  dialog.showModal = () => {
    opens += 1;
    return show.call(dialog);
  };
  t.key(b3, "F2");
  t.key(b3, "x");
  assert.equal(opens, 0, "typing on does not open it again while it is open");
  dialog.close();
  t.key(b3, "F2");
  assert.equal(opens, 1);
});

// ---- hidden formulas ----------------------------------------------------------------------------------------------
async function setCellFlags(t, ref, { locked, hidden }) {
  t.cell(ref).focus();
  t.userClick(t.document.querySelector('[data-sheet-numfmt="dialog"]'));
  await flushAsyncTurns(20);
  t.document.querySelector('#sheet-numfmt-dialog [data-fmt-tab="protect"]').click();
  if (locked !== undefined) t.check("sheet-protect-cell-locked", locked);
  if (hidden !== undefined) t.check("sheet-protect-cell-hidden", hidden);
  await t.click("btn-sheet-numfmt-apply");
}

test("a hidden formula does not leave through a fill, a search or the edit of an unlocked cell", async () => {
  const t = await open();
  await setCellFlags(t, "F2", { hidden: true });
  await setCellFlags(t, "G2", { locked: false });
  await protectWithoutPassword(t);
  // Fill: F2 (hidden) dragged onto the unlocked G2 copies the value.
  t.cell("F2").dispatchEvent({ type: "mousedown", target: t.cell("F2"), button: 0, bubbles: true });
  t.cell("F2").dispatchEvent({ type: "mouseup", target: t.cell("F2"), bubbles: true });
  const handle = t.document.querySelector("#sheet-viewport .sheet-fill-handle");
  handle?.dispatchEvent({ type: "mousedown", target: handle, button: 0, bubbles: true });
  t.cell("G2").dispatchEvent({ type: "mouseover", target: t.cell("G2"), bubbles: true });
  t.cell("G2").dispatchEvent({ type: "mouseup", target: t.cell("G2"), bubbles: true });
  await flushAsyncTurns(15);
  assert.equal(t.cell("G2").dataset.formula ?? "", "", "no formula in G2");
  assert.equal(cellText(t, "G2"), "4950", "the value is filled");
  // Search: a search by formula does not match a hidden one.
  const blocks = [];
  const source = t.host.__EASYOFFICE_CREATE_SHEET_FIND_SOURCE__({
    sheetDataStore: { Sheet1: { F2: { value: "4950", formula: "=SUM(B2:E2)", style: { hideFormula: true } }, F3: { value: "4100", formula: "=SUM(B3:E3)" } } },
    getActiveSheetName: () => "Sheet1",
    sheetController: { snapshotDomSheetCells: () => ({ F2: { value: "4950", formula: "=SUM(B2:E2)", style: { hideFormula: true } }, F3: { value: "4100", formula: "=SUM(B3:E3)" } }) },
    gridUtils: t.host.__EASYOFFICE_SHEET_GRID_UTILS__,
    isFormulaStr: (v) => String(v).startsWith("="),
  });
  source.sync({ setBlocks: (list) => blocks.push(...list) }, { formula: true });
  assert.deepEqual(blocks.map((b) => [b.id, b.text]), [["F2", "4950"], ["F3", "=SUM(B3:E3)"]], "a hidden formula is searched as its value");
});

test("editing an unlocked cell with a hidden formula shows its value, and leaving it as it is keeps the formula", async () => {
  const t = await open();
  await setCellFlags(t, "F2", { locked: false, hidden: true });
  await protectWithoutPassword(t);
  const f2 = t.cell("F2");
  assert.equal(f2.getAttribute("contenteditable"), "true");
  f2.focus();
  t.key(f2, "F2");
  assert.equal(t.cell("F2").textContent, "4950", "the formula is not unfolded");
  t.key(t.cell("F2"), "Enter");
  await flushAsyncTurns(15);
  t.cell("F2").focus();
  assert.equal(t.cell("F2").dataset.formula, "=SUM(B2:E2)", "the formula is still there");
  assert.equal(t.byId("formula-input").value, "4950");
});

// ---- a save that drops a password ------------------------------------------------------------------------------------
test("which formats keep which password", async () => {
  const t = await open();
  const set = (pwd, ranges = []) => t.layout.setProtect("Sheet1", { on: true, deny: [], pwd, ranges });
  const agile = { kind: "agile", alg: "SHA-512", hash: "h", salt: "s", spin: 1 };
  const odf = { kind: "odf", key: "k" };
  const both = { kind: "both", alg: "SHA-512", hash: "h", salt: "s", spin: 1, key: "k" };
  assert.equal(t.guard.lostPassword("xlsx"), false, "no password");
  set(undefined);
  assert.equal(t.guard.lostPassword("xls"), false, "protection without a password drops nothing");
  const lost = (pwd, ext) => (set(pwd), t.guard.lostPassword(ext));
  assert.deepEqual(["xlsx", "ods", "xls", "csv"].map((ext) => lost(agile, ext)), [false, true, true, true]);
  assert.deepEqual(["xlsx", "ods", "xls", "csv"].map((ext) => lost(odf, ext)), [true, false, true, true]);
  assert.deepEqual(["xlsx", "ods", "xls", "csv"].map((ext) => lost(both, ext)), [false, false, true, true]);
  assert.equal(lost({ kind: "legacy", hash: "83AF" }, "xlsx"), false);
  assert.equal(lost({ kind: "legacy", hash: "83AF" }, "ods"), true);
  set(undefined, [{ name: "r", sqref: ["A1"], pwd: odf }]);
  assert.equal(t.guard.lostPassword("xlsx"), true, "a range's password counts");
  t.layout.setProtect("Sheet1", null);
  t.guard.setWorkbook({ structure: true, pwd: agile });
  assert.equal(t.guard.lostPassword("ods"), true, "so does the workbook's");
});

test("a save that drops a password asks first, and declining writes nothing", async () => {
  const t = await open();
  t.layout.setProtect("Sheet1", { on: true, deny: [], pwd: { kind: "odf", key: "k" } });
  const asked = [];
  t.host.confirm = (text) => (asked.push(text), false);
  t.userClick(studioItem(t.document, "studio-save"));
  await flushAsyncTurns(30);
  assert.deepEqual(asked, ["저장한 파일에는 암호가 적용되지 않습니다. 암호 없이 저장하시겠습니까?"]);
  assert.match(t.status(), /문서 저장 취소 \(XLSX\)/);
  // A password the format holds is saved without a question.
  t.layout.setProtect("Sheet1", { on: true, deny: [], pwd: { kind: "both", alg: "SHA-512", hash: "h", salt: "s", spin: 1, key: "k" } });
  asked.length = 0;
  t.userClick(studioItem(t.document, "studio-save"));
  await flushAsyncTurns(30);
  assert.deepEqual(asked, []);
});

// ======================================================================================================================
// The cross-feature checklist (PD-216): one sheet that mixes locked cells, hidden formulas, an allowed range and unlocked cells.
//   locked: everything else; hidden formula, locked: F2; hidden formula, unlocked: F3; unlocked: G2, B5 and B10:B12;
//   an allowed range without a password: A7:B8.
// ======================================================================================================================
async function scene(allowed) {
  const t = await open();
  await setCellFlags(t, "F2", { hidden: true });
  await setCellFlags(t, "F3", { locked: false, hidden: true });
  for (const ref of ["G2", "B5", "B10", "B11", "B12"]) await setCellFlags(t, ref, { locked: false });
  t.layout.setProtect("Sheet1", { on: false, deny: [], ranges: [{ name: "열린", sqref: ["A7:B8"] }] });
  await protectWithoutPassword(t, allowed);
  return t;
}
const typeInto = async (t, ref, text) => {
  const td = t.cell(ref);
  td.focus();
  typeText(t, text);
  t.key(t.cell(ref), "Tab");
  await flushAsyncTurns(15);
};
const closeAlert = (t) => t.byId("sheet-protect-alert-dialog").close();
const press = (t, ref) => {
  const td = t.cell(ref);
  td.dispatchEvent({ type: "mousedown", target: td, button: 0, bubbles: true });
  td.dispatchEvent({ type: "mouseup", target: td, bubbles: true });
};
const redo = async (t) => {
  t.userClick(studioItem(t.document, "studio-redo"));
  await flushAsyncTurns(20);
};

test("checklist: undo and redo chains that mix edits before and after the protection", async () => {
  const t = await open();
  await typeInto(t, "B3", "5");
  await typeInto(t, "A2", "가나");
  await setCellFlags(t, "B10", { locked: false });
  await protectWithoutPassword(t);
  const lockedCells = ["B3", "A2", "C2"];
  const locked = lockedCells.map((ref) => cellText(t, ref));
  await typeInto(t, "B10", "7");
  assert.equal(cellText(t, "B10"), "7", "an unlocked cell takes the edit");
  await t.undo();
  assert.equal(cellText(t, "B10"), "", "its undo works");
  await t.undo();
  assert.deepEqual(lockedCells.map((ref) => cellText(t, ref)), locked, "the steps before the protection are gone: nothing to undo");
  await redo(t);
  assert.equal(cellText(t, "B10"), "7", "its redo works");
  await redo(t);
  assert.deepEqual(lockedCells.map((ref) => cellText(t, ref)), locked);
  assert.equal(t.layout.protectOf("Sheet1").on, true);
});

test("checklist: the fill handle by drag and by double click for every pair of locked and unlocked source and target", async () => {
  const t = await scene();
  const fill = async (from, to, dbl = false) => {
    press(t, from);
    const handle = t.document.querySelector("#sheet-viewport .sheet-fill-handle");
    if (dbl) handle.dispatchEvent({ type: "dblclick", target: handle, button: 0, bubbles: true });
    else {
      handle.dispatchEvent({ type: "mousedown", target: handle, button: 0, bubbles: true });
      t.cell(to).dispatchEvent({ type: "mouseover", target: t.cell(to), bubbles: true });
      t.cell(to).dispatchEvent({ type: "mouseup", target: t.cell(to), bubbles: true });
    }
    await flushAsyncTurns(15);
  };
  await typeInto(t, "B10", "x");
  await fill("B10", "B12");
  assert.deepEqual(["B10", "B11", "B12"].map((r) => cellText(t, r)), ["x", "x", "x"], "unlocked to unlocked");
  await fill("B10", "B9");
  assert.equal(cellText(t, "B9"), "", "a locked cell in the way refuses it all");
  closeAlert(t);
  const before = ["B3", "B4"].map((r) => cellText(t, r));
  await fill("B2", "B4");
  assert.deepEqual(["B3", "B4"].map((r) => cellText(t, r)), before, "locked to locked");
  closeAlert(t);
  await fill("F2", "G2");
  assert.equal(cellText(t, "G2"), "4950", "a locked hidden formula fills an unlocked cell as its value");
  assert.equal(t.cell("G2").dataset.formula ?? "", "", "and no formula");
  const b6 = cellText(t, "B6");
  await fill("B5", null, true);
  assert.equal(cellText(t, "B6"), b6, "the double click fills down into locked B6 as little as the drag does");
});

test("checklist: find and replace by value, by formula and over all sheets leave locked cells and hidden formulas alone", async () => {
  const t = await scene();
  t.sandbox.dispatchWindowKeydown({ key: "F2", ctrlKey: true });
  const find = async (query, replacement, { formula = false, workbook = false } = {}) => {
    for (const [id, value] of [["find-query", query], ["find-replacement", replacement]]) {
      t.byId(id).value = value;
      t.byId(id).dispatchEvent({ type: "input", target: t.byId(id) });
    }
    t.check("find-opt-formula", formula);
    t.check("find-opt-workbook", workbook);
    await flushAsyncTurns(10);
    t.byId("btn-find-replace-all").click();
    await flushAsyncTurns(25);
  };
  await find("승인", "반려");
  assert.deepEqual(["H2", "H3"].map((r) => cellText(t, r)), ["승인", "승인"], "locked cells keep their value");
  await find("84", "85");
  assert.equal(cellText(t, "B5"), "850", "the unlocked B5 is replaced");
  assert.equal(cellText(t, "B3"), "920");
  await find("SUM", "AVERAGE", { formula: true });
  assert.equal(t.cell("F2").dataset.formula, "=SUM(B2:E2)", "a hidden locked formula is untouched");
  assert.equal(t.cell("F3").dataset.formula, "=SUM(B3:E3)", "a hidden unlocked formula is not matched by its text");
  assert.equal(t.cell("F4").dataset.formula, "=SUM(B4:E4)", "a locked visible formula is untouched");
  await find("SUM", "AVERAGE", { formula: true, workbook: true });
  assert.equal(t.cell("F2").dataset.formula, "=SUM(B2:E2)");
  assert.equal(t.layout.protectOf("Sheet2"), null, "another sheet that is not protected is replaced normally");
});

test("checklist: copy, cut, paste and 골라 붙이기 by value, format, formula, column width and turned over", async () => {
  const t = await scene();
  const copyOf = (ref, moves = []) => {
    press(t, ref);
    for (const [k, n] of moves) for (let i = 0; i < n; i += 1) t.key(t.document.activeElement, k, { shiftKey: true });
    const store = {};
    t.document.activeElement.dispatchEvent({ type: "copy", target: t.document.activeElement, clipboardData: { setData: (type, v) => (store[type] = v), getData: (type) => store[type] || "" } });
    return store["text/plain"];
  };
  assert.equal(copyOf("F2"), "4950", "the hidden formula goes out as its value");
  assert.equal(copyOf("F3"), "4100");
  assert.equal(copyOf("F4"), "=SUM(B4:E4)", "a visible formula is copied");
  const special = async (mode, transpose = false) => {
    t.userClick(t.document.querySelector('.md-item[data-sheet-paste="special"]'));
    t.byId("sheet-paste-kind-select").value = mode;
    t.byId("sheet-paste-transpose").checked = transpose;
    t.userClick(t.byId("btn-sheet-paste-apply"));
    await flushAsyncTurns(15);
  };
  copyOf("F2");
  for (const mode of ["all", "values", "formulas"]) {
    press(t, "B11");
    await special(mode);
    assert.equal(t.cell("B11").dataset.formula ?? "", "", `${mode}: no formula reaches B11`);
    assert.equal(cellText(t, "B11"), "4950", mode);
  }
  press(t, "B12");
  await special("all", true);
  assert.equal(t.cell("B12").dataset.formula ?? "", "", "turned over: no formula");
  const locked = ["B3", "C3"].map((r) => cellText(t, r));
  for (const mode of ["all", "values", "formats", "formulas", "widths"]) {
    press(t, "B3");
    await special(mode);
    assert.deepEqual(["B3", "C3"].map((r) => cellText(t, r)), locked, mode);
    closeAlert(t);
  }
  copyOf("B2");
  press(t, "B10");
  await special("formats");
  assert.equal(t.guard.cellEditable("Sheet1", 10, 2, t.cell("B10")._st), true, "the format paste kept B10 unlocked");
  const cut = t.cell("B2");
  cut.focus();
  cut.dispatchEvent({ type: "cut", target: cut, clipboardData: { setData() {}, getData: () => "" } });
  await flushAsyncTurns(15);
  assert.equal(cellText(t, "B2"), "1150", "cut leaves a locked cell");
  closeAlert(t);
});

test("checklist: sorting by the dialog, the ribbon and the filter menu needs 정렬 and no locked cell in the range", async () => {
  const t = await scene(["selectLockedCells", "selectUnlockedCells", "sort"]);
  await typeInto(t, "B10", "3");
  await typeInto(t, "B11", "1");
  await typeInto(t, "B12", "2");
  const select = (from, moves) => {
    press(t, from);
    for (const [k, n] of moves) for (let i = 0; i < n; i += 1) t.key(t.document.activeElement, k, { shiftKey: true });
  };
  const column = (c, rows) => rows.map((r) => cellText(t, c + r));
  const before = column("B", [2, 3, 4, 5]);
  select("B2", [["ArrowDown", 3]]);
  t.userClick(commandItem(t.document, "sheet-data-sort"));
  await flushAsyncTurns(20);
  assert.deepEqual(column("B", [2, 3, 4, 5]), before, "the ribbon sort of locked cells is refused");
  closeAlert(t);
  select("B10", [["ArrowDown", 2]]);
  await t.act("sort-dialog");
  t.userClick(t.byId("btn-sheet-sort-apply"));
  await flushAsyncTurns(25);
  assert.deepEqual(column("B", [10, 11, 12]), ["1", "2", "3"], "a range of unlocked cells sorts when 정렬 is allowed");
  t.layout.setProtect("Sheet1", { ...t.layout.protectOf("Sheet1"), deny: [...t.layout.protectOf("Sheet1").deny, "sort"] });
  select("B10", [["ArrowDown", 2]]);
  t.userClick(commandItem(t.document, "sheet-data-sort"));
  await flushAsyncTurns(20);
  assert.deepEqual(column("B", [10, 11, 12]), ["1", "2", "3"], "no 정렬: nothing moves (the reverse sort would)");
  blocked(t);
});

test("checklist: the auto filter is switched on and off only without protection, and used only with 자동 필터 사용", async () => {
  const t = await scene(["selectLockedCells", "selectUnlockedCells", "autoFilter"]);
  await t.act("filter-toggle");
  assert.equal(t.layout.filterOf("Sheet1"), null, "no filter can be turned on while protected");
  blocked(t);
  t.layout.setFilter("Sheet1", { range: { r1: 1, c1: 1, r2: 8, c2: 6 }, columns: [], hidden: [] });
  await t.act("filter-clear");
  assert.equal(t.byId("sheet-protect-alert-dialog").open, false, "using an existing one is allowed");
  t.layout.setProtect("Sheet1", { ...t.layout.protectOf("Sheet1"), deny: [...t.layout.protectOf("Sheet1").deny, "autoFilter"] });
  await t.act("filter-clear");
  blocked(t);
  await t.act("filter-toggle");
  assert.notEqual(t.layout.filterOf("Sheet1"), null, "it cannot be turned off either");
  blocked(t);
});

test("checklist: names and the name box take no cell and cannot select what the sheet keeps from selection", async () => {
  const t = await scene();
  t.layout.setProtect("Sheet1", { ...t.layout.protectOf("Sheet1"), deny: [...t.layout.protectOf("Sheet1").deny, "selectLockedCells"] });
  press(t, "B10");
  const box = t.byId("active-cell-ref");
  box.textContent = "B3";
  box.dispatchEvent({ type: "keydown", key: "Enter", target: box });
  await flushAsyncTurns(15);
  assert.notEqual(t.byId("active-cell-ref").textContent, "B3", "the name box does not select a locked cell");
  box.textContent = "B11";
  box.dispatchEvent({ type: "keydown", key: "Enter", target: box });
  await flushAsyncTurns(15);
  assert.equal(t.byId("active-cell-ref").textContent, "B11", "it selects an unlocked one");
  await t.act("name-define");
  assert.equal(t.byId("sheet-name-edit-dialog").open, true, "defining a name is allowed");
  t.byId("sheet-name-edit-dialog").close();
  await t.act("name-manage");
  assert.equal(t.byId("sheet-names-dialog").open, true, "managing names is allowed");
});

test("checklist: page setup and the print area are allowed on a protected sheet", async () => {
  const t = await scene();
  await t.act("page-setup");
  assert.equal(t.byId("sheet-page-dialog").open, true);
  t.byId("sheet-page-dialog").close();
  press(t, "B10");
  await t.act("print-area-set");
  assert.deepEqual(t.layout.pageOf("Sheet1")?.printArea, ["B10"]);
});

test("checklist: conditional formatting and validation open only with 셀 서식", async () => {
  const t = await scene();
  t.userClick(commandItem(t.document, "sheet-mod-condfmt"));
  await flushAsyncTurns(15);
  blocked(t);
  t.layout.setProtect("Sheet1", { ...t.layout.protectOf("Sheet1"), deny: t.layout.protectOf("Sheet1").deny.filter((f) => f !== "formatCells") });
  t.userClick(commandItem(t.document, "sheet-mod-condfmt"));
  await flushAsyncTurns(15);
  assert.equal(t.byId("sheet-cf-dialog").open, true);
});

test("checklist: the command search and the native bar reach the same guarded actions", async () => {
  const t = await scene();
  const items = t.host.__EASYOFFICE_NATIVE_MENU_TABLE__.sheetMenuPaletteItems(t.document);
  const titles = items.map((i) => i.title);
  for (const title of ["시트 보호 해제", "통합 문서 보호", "범위 편집 허용"]) assert.ok(titles.includes(title), title);
  press(t, "B2");
  items.find((i) => i.id === "sheet-menu:row-insert-above").run();
  await flushAsyncTurns(20);
  blocked(t);
  assert.equal(t.document.defaultView.__EASYOFFICE_NATIVE_MENU__("sheet:protect"), true);
  await flushAsyncTurns(20);
  assert.equal(t.layout.protectOf("Sheet1").on, false, "the native bar item lifts the protection like the menu item");
});

test("checklist: row and column insert, delete, hide and group follow their own flags, with and without the engine session", async () => {
  const t = await open({ engine: true });
  const base = t.guard.FLAG_NAMES.filter((f) => !t.guard.DEFAULT_ALLOWED.includes(f));
  const protect = (allow) => t.layout.setProtect("Sheet1", { on: true, deny: base.filter((f) => !allow.includes(f)) });
  const sent = () => t.requests.filter((r) => r.sessionAction === "structure").length;
  t.cell("C2").focus();
  const tries = [
    ["row-delete", "deleteRows"], ["col-delete", "deleteColumns"], ["hide", "formatRows"], ["row-insert-above", "insertRows"], ["col-insert-left", "insertColumns"],
  ];
  for (const [act, flag] of tries) {
    protect([]);
    const before = sent();
    await t.act(act);
    blocked(t);
    assert.equal(sent(), before, `${act}: nothing reaches the engine without ${flag}`);
    protect([flag]);
    await t.act(act);
    if (act === "row-delete" || act === "col-delete") {
      // C2 holds a locked value: the delete is refused even where the flag allows it.
      assert.equal(sent(), before, `${act}: a locked cell with content refuses the delete`);
      blocked(t);
    } else if (act === "hide") {
      assert.equal(t.layout.rowHidden("Sheet1", 2), true, "hiding runs with 행 서식");
    } else {
      assert.equal(sent(), before + 1, `${act}: allowed with ${flag}`);
    }
  }
  // The grouping buttons follow 행 서식 and 열 서식.
  protect([]);
  await t.act("group");
  blocked(t);
  assert.equal(t.layout.outlineOf("Sheet1"), null);
  // An empty row far below may be deleted when 행 삭제 is allowed (nothing locked goes with it).
  protect(["deleteRows"]);
  const row = t.document.querySelector('#pane-sheet th[data-row="12"]');
  row.dispatchEvent({ type: "mousedown", button: 0, target: row });
  const before = sent();
  await t.act("row-delete");
  assert.equal(sent(), before + 1, "a row without locked content can be deleted");
});

test("checklist: with the structure protected every sheet tab command is refused or greyed out, by menu, tab menu and keyboard", async () => {
  const t = await open();
  t.guard.setWorkbook({ structure: true, windows: false });
  for (const act of ["tab-insert", "tab-delete", "tab-rename", "tab-copy", "tab-left", "tab-right", "tab-hide", "tab-unhide", "tab-add"]) {
    const before = JSON.stringify([...t.document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]")].map((b) => b.dataset.sheetTab));
    await t.act(act);
    assert.equal(JSON.stringify([...t.document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]")].map((b) => b.dataset.sheetTab)), before, act);
    assert.equal(t.document.querySelector("#sheet-tab-bar input.sheet-tab-edit"), null, `${act}: no rename field`);
  }
  const tab = t.document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet2"]');
  tab.dispatchEvent({ type: "contextmenu", target: tab, bubbles: true, clientX: 10, clientY: 10 });
  const items = [...t.document.querySelectorAll("#sheet-tab-menu [data-tab-act]")];
  assert.ok(items.length > 0);
  assert.deepEqual(items.map((el) => el.getAttribute("aria-disabled")), items.map(() => "true"), "the tab menu is greyed out");
  tab.dispatchEvent({ type: "dblclick", target: tab, bubbles: true });
  assert.equal(t.document.querySelector("#sheet-tab-bar input.sheet-tab-edit"), null, "a double click does not start a rename");
});

// ======================================================================================================================
// The review of the checklist: the autosave, the converter and the undo of an allowed row or column edit.
// ======================================================================================================================
const AUTOSAVE_KEY = "easyoffice.workspace.autosave.v1";

test("the autosave record keeps the protection of the sheet and of the workbook, and a new window restores both", async () => {
  const storage = createMemoryStorage();
  const t = await open({ localStorage: storage });
  await t.act("sheet-protect");
  t.check("sheet-protect-use-password", true);
  t.byId("sheet-protect-password").value = SECRET;
  t.byId("sheet-protect-confirm").value = SECRET;
  await t.click("btn-sheet-protect-apply");
  await t.act("workbook-protect");
  t.byId("sheet-wb-protect-password").value = SECRET;
  t.byId("sheet-wb-protect-confirm").value = SECRET;
  await t.click("btn-sheet-wb-protect-apply");
  const sheet = t.layout.protectOf("Sheet1");
  const book = t.guard.getWorkbook();
  t.advanceTimers(5000);
  await flushAsyncTurns(10);
  const record = storage.getItem(AUTOSAVE_KEY);
  assert.ok(record, "the record is written");
  assert.equal(record.includes(SECRET), false, "no password in the record, only the digests");
  assert.deepEqual(JSON.parse(record).workbookProtect, book);

  const next = await createWorkspaceHarness({ localStorage: storage });
  await flushAsyncTurns(15);
  const restored = next.host.__EASYOFFICE_SHEET_PROTECT__;
  assert.deepEqual(next.host.__EASYOFFICE_SHEET_LAYOUT__.protectOf("Sheet1"), sheet, "the sheet is still protected with its password");
  assert.equal(restored.structureLocked(), true, "so is the workbook");
  assert.deepEqual(restored.getWorkbook(), book);
});

test("a workbook that only has its structure protected is kept by the autosave too", async () => {
  const storage = createMemoryStorage();
  const t = await open({ localStorage: storage });
  await t.act("workbook-protect");
  await t.click("btn-sheet-wb-protect-apply");
  t.check("sheet-wb-protect-none", true);
  await t.click("btn-sheet-wb-protect-apply");
  assert.equal(t.guard.structureLocked(), true);
  t.advanceTimers(5000);
  await flushAsyncTurns(10);
  const next = await createWorkspaceHarness({ localStorage: storage });
  await flushAsyncTurns(15);
  assert.equal(next.host.__EASYOFFICE_SHEET_PROTECT__.structureLocked(), true);
});

test("an undo still keeps the protection a step lacks while the restore of an autosave brings the snapshot's own", async () => {
  const t = await open();
  const b3 = t.cell("B3");
  b3.focus();
  typeText(t, "5");
  t.key(b3, "Enter");
  await flushAsyncTurns(15);
  // The layout is set past the history (as a step recorded before a protection would meet it): the undo does not lift it.
  t.layout.setProtect("Sheet1", { on: true, deny: [] });
  await t.undo();
  assert.equal(t.layout.protectOf("Sheet1")?.on, true, "the undo keeps the protection");
});

// A sheet that holds a password, and the 변환 tab asked to convert the sheets of the editor.
async function convertSheetWithPassword(dstFormat, pwdKind) {
  const t = await open();
  t.layout.setProtect("Sheet1", { on: true, deny: [], pwd: { kind: pwdKind, alg: "SHA-512", hash: "h", salt: "s", spin: 1, key: "k" } });
  const asked = [];
  let answer = false;
  t.host.confirm = (text) => (asked.push(text), answer);
  switchModule(t.document, "CONV");
  await flushAsyncTurns(10);
  t.byId("conv-src-fmt").value = "xlsx";
  t.byId("conv-dst-fmt").value = dstFormat;
  const run = async () => {
    const before = t.downloads.length;
    t.userClick(t.byId("btn-run-convert"));
    await flushAsyncTurns(30);
    return t.downloads.length - before;
  };
  return { t, asked, run, answer: (value) => (answer = value) };
}

test("the convert button asks before it drops a password, and declining converts nothing", async () => {
  const c = await convertSheetWithPassword("csv", "both");
  assert.equal(await c.run(), 0, "declined: no file");
  assert.deepEqual(c.asked, ["저장한 파일에는 암호가 적용되지 않습니다. 암호 없이 저장하시겠습니까?"]);
  assert.match(c.t.byId("conv-result-box").textContent, /변환 취소/);
  c.answer(true);
  assert.equal(await c.run(), 1, "accepted: the file is written");
  assert.equal(c.asked.length, 2);
});

test("the convert button keeps a password the target format holds without a question", async () => {
  const c = await convertSheetWithPassword("xlsx", "both");
  c.answer(true);
  assert.equal(await c.run(), 1);
  assert.deepEqual(c.asked, []);
});

test("Ctrl+S in the converter asks before it drops a password of the sheets it converts", async () => {
  const c = await convertSheetWithPassword("tsv", "odf");
  c.t.userClick(studioItem(c.t.document, "studio-save"));
  await flushAsyncTurns(30);
  assert.equal(c.t.downloads.length, 0, "declined: nothing is written");
  assert.equal(c.asked.length, 1);
  assert.match(c.t.status(), /문서 저장 취소 \(TSV\)/);
});

test("an allowed row insert is undone, the locked values return to their places, and a redo does the edit again", async () => {
  const t = await open({ engine: true });
  const base = t.guard.FLAG_NAMES.filter((f) => !t.guard.DEFAULT_ALLOWED.includes(f));
  t.layout.setProtect("Sheet1", { on: true, deny: base.filter((f) => f !== "insertRows") });
  const before = ["B2", "B3", "C2", "A2"].map((ref) => cellText(t, ref));
  t.cell("B2").focus();
  await t.act("row-insert-above");
  assert.equal(cellText(t, "B3"), before[0], "the row went in above B2: its value moved down");
  assert.equal(cellText(t, "B2"), "");
  await t.undo();
  assert.deepEqual(["B2", "B3", "C2", "A2"].map((ref) => cellText(t, ref)), before, "the undo succeeded");
  assert.equal(t.byId("sheet-protect-alert-dialog").open, false, "no refusal");
  await redo(t);
  assert.equal(cellText(t, "B3"), before[0]);
  await t.undo();
  assert.deepEqual(["B2", "B3", "C2", "A2"].map((ref) => cellText(t, ref)), before);
  assert.equal(t.layout.protectOf("Sheet1").on, true, "the sheet stays protected");
});

test("a restore compares a step of a row edit where the edit put the cells", async () => {
  const t = await open();
  t.layout.setProtect("Sheet1", { on: true, deny: [] });
  const store = { Sheet1: { B3: { value: "7", formula: "" }, C3: { formula: "=B3+1", value: "8" } } };
  const step = (cells, structureEdit) => ({ sheets: { Sheet1: cells }, structureEdit });
  const edit = { sheet: "Sheet1", axis: "row", op: "insert", at: 2, count: 1 };
  // The step before an insert at row 2 held the value in B2 and the formula in C2; now they are one row down.
  const then = { B2: { value: "7", formula: "" }, C2: { formula: "=B2+1", value: "8" } };
  assert.equal(t.guard.restoreWouldChange(step(then), store), true, "by address it looks like a change");
  assert.equal(t.guard.restoreWouldChange(step(then, edit), store), false, "with the edit it is none");
  assert.equal(t.guard.restoreWouldChange(step({ ...then, B2: { value: "9", formula: "" } }, edit), store), true, "a real change is still one");
  const back = t.guard.inverseEdit(edit);
  assert.deepEqual(back, { ...edit, op: "delete" });
  const after = { B3: { value: "7", formula: "" } };
  assert.equal(t.guard.restoreWouldChange(step(after, back), { Sheet1: { B2: { value: "7", formula: "" } } }), false, "the redo of the insert leads the other way");
});

test("a restore of the step of a row edit puts the allowed ranges back where the step had them and keeps their passwords", async () => {
  const t = await open();
  const pwd = { kind: "both", alg: "SHA-512", hash: "OFFLINE-1-1", salt: "S", spin: 1, key: "K" };
  t.layout.setProtect("Sheet1", { on: true, deny: [], ranges: [{ name: "r", sqref: ["B3:B4"], pwd }, { name: "now", sqref: ["D1"] }] });
  const old = { sheetLayout: { Sheet1: { protect: { on: false, deny: [], ranges: [{ name: "r", sqref: ["B2:B3"] }, { name: "gone", sqref: ["A1"] }] } } }, sheetOrder: ["Sheet1"] };
  const kept = t.guard.keepProtection(old, ["Sheet1"]).Sheet1.protect;
  assert.equal(kept.on, true);
  assert.deepEqual(kept.ranges.map((r) => [r.name, r.sqref]), [["r", ["B2:B3"]], ["now", ["D1"]]]);
  assert.deepEqual(kept.ranges[0].pwd, pwd, "the password stays the one the sheet has");
});
