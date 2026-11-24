import test, { after, before } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { REPO, sleep, startBrowser, startEngine, startUiServer } from "./bench-webui-support.mjs";
import { findChrome } from "./hwp-studio-harness.mjs";

// The conditional formatting menu in a real headless browser against the real engine (PD-282): the nested submenus opened by
// the mouse, the quick dialogs with their live preview, the galleries, clearing, undo and the rule manager, driven over CDP with
// the browser's own profile. It needs a built `easyoffice-app` (`EO_APP`, or `target-193/debug` or `target/debug` of this
// checkout) and a Chrome (`CHROME_PATH` or the usual install places); without them the tests are skipped. With `EO_SHOTS` set to
// a directory the run also saves its screens there, in the light and in the dark theme.

const exe = process.platform === "win32" ? "easyoffice-app.exe" : "easyoffice-app";
const APP = [process.env.EO_APP, ...["target-193", "target"].map((dir) => path.join(REPO, dir, "debug", exe))].find((file) => file && fs.existsSync(file));
let CHROME = null;
try {
  CHROME = findChrome();
} catch {
  CHROME = null;
}
const skip = !APP || !CHROME ? "needs a built easyoffice-app and a Chrome (EO_APP, CHROME_PATH)" : false;
const SHOTS = process.env.EO_SHOTS || "";

const KEYS = { Enter: [13, "Enter"], Escape: [27, "Escape"], ArrowRight: [39, "ArrowRight"], ArrowDown: [40, "ArrowDown"], z: [90, "KeyZ"], y: [89, "KeyY"] };
const CTRL = 2;
const SHIFT = 8;

let ctx = null;

before(async () => {
  if (skip) return;
  const port = 20000 + Math.floor(Math.random() * 20000);
  let engine = null, server = null, browser = null;
  try {
    engine = startEngine(APP, port);
    await sleep(800);
    // `EO_UI_DIR` serves another copy of the page (the before-and-after shots of a change run the same steps on both).
    server = await startUiServer({ enginePort: port, port: port + 1, ui: process.env.EO_UI_DIR || undefined });
    browser = await startBrowser({ chrome: CHROME, debugPort: port + 2, url: `http://127.0.0.1:${port + 1}/?module=SHEET&win=cf-real-chrome`, width: 1280, height: 860 });
    ctx = { engine, server, browser, cdp: browser.cdp };
    await sleep(1000);
  } catch (err) {
    try { await browser?.stop?.(); } catch {}
    try { server?.close?.(); } catch {}
    try { engine?.kill?.(); } catch {}
    throw err;
  }
});

after(async () => {
  if (!ctx) return;
  await ctx.browser.stop();
  ctx.server.close();
  ctx.engine.kill();
});

const key = async (name, modifiers = 0) => {
  const [vk, code] = KEYS[name];
  const base = { key: name, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers };
  await ctx.cdp.send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
  await ctx.cdp.send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
};
const text = (value) => ctx.cdp.send("Input.insertText", { text: value });
const js = (expression) => ctx.cdp.evalJs(expression);
const mouse = (type, x, y) => ctx.cdp.send("Input.dispatchMouseEvent", { type, x, y, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1 });
const td = (ref) => `#pane-sheet td[data-cell="${ref}"]`;
const rectOf = (selector) => js(`(() => { const el = document.querySelector(${JSON.stringify(selector)}); if (!el) return null; const r = el.getBoundingClientRect(); return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, width: r.width, height: r.height }; })()`);

async function click(ref) {
  const r = await rectOf(td(ref));
  assert.ok(r, `${ref} is on the screen`);
  const [x, y] = [r.left + r.width / 2, r.top + r.height / 2];
  await mouse("mouseMoved", x, y);
  await mouse("mousePressed", x, y);
  await mouse("mouseReleased", x, y);
}
async function put(ref, value) {
  await click(ref);
  await text(value);
  await key("Enter");
  await sleep(60);
}
const until = async (check, what, ms = 8000) => {
  const end = Date.now() + ms;
  let last;
  while (Date.now() < end) {
    last = await check();
    if (last) return last;
    await sleep(80);
  }
  assert.fail(`timed out: ${what} (${JSON.stringify(last)})`);
};

// The element of the menu path: `labels` name the rows from the 서식 menu down, a leaf may be a selector instead.
const menuEl = (labels, leaf) => `(() => {
  let scope = document.querySelector('.menu-item[data-menu="format"] > .menu-dropdown');
  let el = null;
  for (const text of ${JSON.stringify(labels)}) {
    el = [...scope.querySelectorAll(':scope > .md-item, :scope > .md-sub')].find((c) => c.querySelector(':scope > .md-label').textContent === text);
    if (!el) return null;
    scope = el.querySelector(':scope > .md-sub-panel') || scope;
  }
  return ${leaf ? `scope.querySelector(${JSON.stringify(leaf)})` : "el"};
})()`;
const centerOf = (expression) => js(`(() => { const el = ${expression}; if (!el) return null; const r = el.getBoundingClientRect(); return r.width === 0 ? null : { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`);

// Opens the 서식 menu and walks `submenus` with the pointer, then clicks the leaf (a label of the last panel or a selector in it).
async function chooseFromMenu(submenus, leaf) {
  const title = await rectOf('.menu-item[data-menu="format"] > .menu-title');
  await mouse("mouseMoved", title.left + title.width / 2, title.top + title.height / 2);
  await mouse("mousePressed", title.left + title.width / 2, title.top + title.height / 2);
  await mouse("mouseReleased", title.left + title.width / 2, title.top + title.height / 2);
  for (let i = 1; i <= submenus.length; i += 1) {
    const at = await until(() => centerOf(menuEl(submenus.slice(0, i))), `menu row ${submenus[i - 1]}`);
    await mouse("mouseMoved", at.x, at.y);
  }
  const labelled = leaf.startsWith("[") || leaf.startsWith(".");
  const expression = labelled ? menuEl(submenus, leaf) : menuEl([...submenus, leaf]);
  const at = await until(() => centerOf(expression), `menu item ${leaf}`);
  await mouse("mouseMoved", at.x, at.y);
  await mouse("mousePressed", at.x, at.y);
  await mouse("mouseReleased", at.x, at.y);
}

// Saves the screen in the current theme (a no-op unless EO_SHOTS is set).
async function shoot(name) {
  if (!SHOTS) return;
  fs.mkdirSync(SHOTS, { recursive: true });
  for (const mode of ["light", "dark"]) {
    await js(`window.__EASYOFFICE_THEME__.setMode(${JSON.stringify(mode)})`);
    await sleep(250);
    const shot = await ctx.cdp.send("Page.captureScreenshot", { format: "png" });
    fs.writeFileSync(path.join(SHOTS, `${name}-${mode}.png`), Buffer.from(shot.result.data, "base64"));
  }
  await js(`window.__EASYOFFICE_THEME__.setMode("light")`);
}

const rulesOf = () => js(`JSON.stringify(Object.values(window.__EASYOFFICE_SHEET_RULES__.getAll()).flatMap((r) => r.cf))`).then(JSON.parse);
const look = (ref) => js(`(() => { const td = document.querySelector(${JSON.stringify(td(ref))}); return td ? { cf: td.dataset.cf || "", bg: td.style.getPropertyValue("--cf-bg"), fg: td.style.getPropertyValue("--cf-fg"), bar: td.style.getPropertyValue("--cf-bar"), icon: td.dataset.cfIcon || "" } : null; })()`);
const painted = (ref, field) => until(async () => (await look(ref))?.[field], `${ref} has ${field}`);
const unpainted = (ref) => until(async () => (await look(ref))?.cf === "", `${ref} has no rule look`);
const dialogOpen = (id) => js(`document.getElementById(${JSON.stringify(id)}).open`);
const field = (id) => js(`document.getElementById(${JSON.stringify(id)}).value`);
async function typeInto(id, value) {
  await js(`(() => { const el = document.getElementById(${JSON.stringify(id)}); el.focus(); el.select(); })()`);
  await text(value);
  await sleep(250);
}
const ok = async () => {
  const at = await until(() => centerOf(`document.getElementById("btn-sheet-cfq-ok")`), "quick dialog OK button");
  await mouse("mouseMoved", at.x, at.y);
  await mouse("mousePressed", at.x, at.y);
  await mouse("mouseReleased", at.x, at.y);
  await sleep(150);
};

const VALUES = { B2: "1", B3: "2", B4: "3", B5: "4", B6: "5" };

test("the cells are typed and the 서식 menu has the conditional formatting submenu with its nested panels inside the window", { skip }, async () => {
  for (const [ref, value] of Object.entries(VALUES)) await put(ref, value);
  await click("B2");
  for (let i = 0; i < 4; i += 1) await key("ArrowDown", SHIFT);
  await chooseFromMenu(["조건부 서식", "셀 강조 규칙"], "다음 값의 사이에 있음…");
  assert.equal(await dialogOpen("sheet-cfq-dialog"), true, "the item opened its dialog");
  await js(`document.getElementById("sheet-cfq-dialog").close()`);
  await sleep(100);
  // The panels are shown by hover only; every level stays inside the window.
  const title = await rectOf('.menu-item[data-menu="format"] > .menu-title');
  await mouse("mouseMoved", title.left + 4, title.top + 4);
  await mouse("mousePressed", title.left + 4, title.top + 4);
  await mouse("mouseReleased", title.left + 4, title.top + 4);
  const sub = await until(() => centerOf(menuEl(["조건부 서식"])), "the submenu row");
  await mouse("mouseMoved", sub.x, sub.y);
  for (const [row, name] of [["셀 강조 규칙", "menu-highlight"], ["데이터 막대", "menu-gallery-bars"], ["색조", "menu-gallery-scales"]]) {
    const at = await until(() => centerOf(menuEl(["조건부 서식", row])), `the ${row} row`);
    await mouse("mouseMoved", at.x, at.y);
    await shoot(name);
  }
  const icons = await until(() => centerOf(menuEl(["조건부 서식", "아이콘 집합"])), "the icon set row");
  await mouse("mouseMoved", icons.x, icons.y);
  await shoot("menu-gallery-icons");
  const frame = JSON.parse(await js(`JSON.stringify((() => { const panel = [...document.querySelectorAll(".md-sub-panel")].find((p) => p.id === "sheet-cf-gallery-icons"); const r = panel.getBoundingClientRect(); return { right: r.right, bottom: r.bottom, width: window.innerWidth, height: window.innerHeight, tiles: panel.querySelectorAll(".cfg-tile").length }; })())`));
  assert.equal(frame.tiles, 20);
  assert.ok(frame.right <= frame.width && frame.bottom <= frame.height, `the icon set gallery is inside the window ${JSON.stringify(frame)}`);
  await key("Escape");
  await mouse("mouseMoved", 5, 700);
  await mouse("mousePressed", 5, 700);
  await mouse("mouseReleased", 5, 700);
});

test("보다 큼 previews on the sheet while the value and format change; Escape takes the preview away", { skip }, async () => {
  await click("B2");
  for (let i = 0; i < 4; i += 1) await key("ArrowDown", SHIFT);
  await chooseFromMenu(["조건부 서식", "셀 강조 규칙"], "보다 큼…");
  assert.equal(await dialogOpen("sheet-cfq-dialog"), true);
  assert.equal(await field("sheet-cfq-a"), "1", "the active cell's value");
  assert.equal(await painted("B3", "bg"), "#ffc7ce", "the preview is on the cells that are greater than 1");
  assert.equal((await look("B2")).cf, "", "B2 is not greater than itself");
  await shoot("dialog-greater");
  await typeInto("sheet-cfq-a", "3");
  await painted("B5", "bg");
  await unpainted("B4");
  await js(`(() => { const s = document.getElementById("sheet-cfq-format-select"); s.value = "green-fill-dark-green"; s.dispatchEvent(new Event("change", { bubbles: true })); })()`);
  assert.equal(await until(async () => (await look("B5")).bg === "#c6efce" && "#c6efce", "the green format"), "#c6efce");
  assert.equal((await rulesOf()).length, 1);
  await key("Escape");
  await sleep(200);
  assert.equal(await dialogOpen("sheet-cfq-dialog"), false);
  assert.deepEqual(await rulesOf(), [], "Escape puts the rules back");
  await unpainted("B5");
  assert.equal(await js(`document.activeElement && document.activeElement.id !== "sheet-cfq-a"`), true);
});

test("accepting the dialog is one undo step; undo and redo take the rule away and bring it back", { skip }, async () => {
  await click("B2");
  for (let i = 0; i < 4; i += 1) await key("ArrowDown", SHIFT);
  await chooseFromMenu(["조건부 서식", "셀 강조 규칙"], "보다 큼…");
  await typeInto("sheet-cfq-a", "3");
  await ok();
  assert.equal(await dialogOpen("sheet-cfq-dialog"), false);
  assert.equal((await rulesOf()).length, 1);
  await painted("B6", "bg");
  await click("B2");
  await key("z", CTRL);
  await sleep(300);
  assert.deepEqual(await rulesOf(), [], "one undo removes the rule the dialog made");
  await unpainted("B6");
  await key("y", CTRL);
  await sleep(300);
  assert.equal((await rulesOf()).length, 1, "redo brings it back");
  await painted("B6", "bg");
});

test("the data bar, color scale and icon set galleries add a rule at once, and each is one undo step", { skip }, async () => {
  await js(`window.__EASYOFFICE_SHEET_RULES__.replaceAll({})`);
  await click("B2");
  for (let i = 0; i < 4; i += 1) await key("ArrowDown", SHIFT);
  await chooseFromMenu(["조건부 서식", "데이터 막대"], '[data-sheet-cf="bar-grad-blue"]');
  assert.match(await painted("B5", "bar"), /linear-gradient/, "the bar is drawn from the engine's answer");
  await chooseFromMenu(["조건부 서식", "색조"], '[data-sheet-cf="scale-gyr"]');
  await painted("B3", "bg");
  await chooseFromMenu(["조건부 서식", "아이콘 집합"], '[data-sheet-cf="icons-3arrows"]');
  assert.ok(await painted("B2", "icon"), "an icon on the cells");
  assert.deepEqual((await rulesOf()).map((r) => r.t), ["icons", "scale", "bar"], "the newest rule is first");
  await shoot("galleries-applied");
  await key("z", CTRL);
  await sleep(300);
  assert.deepEqual((await rulesOf()).map((r) => r.t), ["scale", "bar"]);
  await key("z", CTRL);
  await key("z", CTRL);
  await sleep(300);
  assert.deepEqual(await rulesOf(), []);
});

test("the rule manager lists the rules of the selection or of the sheet with a preview of each, and clearing from the menu empties them", { skip }, async () => {
  await click("B2");
  for (let i = 0; i < 4; i += 1) await key("ArrowDown", SHIFT);
  await chooseFromMenu(["조건부 서식", "색조"], '[data-sheet-cf="scale-bwr"]');
  await chooseFromMenu(["조건부 서식", "상위/하위 규칙"], "평균 초과…");
  await ok();
  await click("D9");
  await chooseFromMenu(["조건부 서식", "데이터 막대"], '[data-sheet-cf="bar-solid-green"]');
  await click("B3");
  await chooseFromMenu(["조건부 서식"], "규칙 관리…");
  assert.equal(await dialogOpen("sheet-cf-dialog"), true);
  const rows = (selector) => js(`JSON.stringify([...document.querySelectorAll(${JSON.stringify(selector)})].map((li) => ({ kinds: li.querySelector(".cfp").dataset.kind, parts: li.querySelectorAll(".cfp > *").length, index: li.dataset.index })))`).then(JSON.parse);
  assert.equal(await field("sheet-cf-scope-select"), "selection");
  const shown = await rows("#sheet-cf-list li");
  assert.deepEqual(shown.map((r) => r.kinds), ["style", "scale"], "the bar on D9 does not meet the selection");
  assert.ok(shown.every((r) => r.parts > 0), "each row draws its preview");
  await shoot("manager-selection");
  await js(`(() => { const s = document.getElementById("sheet-cf-scope-select"); s.value = "sheet"; s.dispatchEvent(new Event("change", { bubbles: true })); })()`);
  assert.deepEqual((await rows("#sheet-cf-list li")).map((r) => r.kinds), ["bar", "style", "scale"]);
  await shoot("manager-sheet");
  await js(`document.getElementById("sheet-cf-dialog").close()`);
  await sleep(100);
  await click("B2");
  for (let i = 0; i < 4; i += 1) await key("ArrowDown", SHIFT);
  await chooseFromMenu(["조건부 서식", "규칙 지우기"], "선택한 셀에서 규칙 지우기");
  assert.deepEqual((await rulesOf()).map((r) => r.t), ["bar"], "only the bar elsewhere is left");
  await chooseFromMenu(["조건부 서식", "규칙 지우기"], "전체 시트에서 규칙 지우기");
  assert.deepEqual(await rulesOf(), []);
  await key("z", CTRL);
  await sleep(300);
  assert.equal((await rulesOf()).length, 1, "clearing the sheet is one undo step");
});

// ---- the border of a format (PD-284) -------------------------------------------------------------------------------------

const RED = "rgb(255, 0, 0)";
const computed = (ref, prop) => js(`getComputedStyle(document.querySelector(${JSON.stringify(td(ref))}))[${JSON.stringify(prop)}]`);
// Opens the drop-down of a dialog's format list (the page draws its own, so it is in the screenshots).
async function openFormatList() {
  const at = await centerOf(`document.getElementById("sheet-cfq-format-select").parentElement.querySelector(".custom-select-trigger")`);
  await mouse("mouseMoved", at.x, at.y);
  await mouse("mousePressed", at.x, at.y);
  await mouse("mouseReleased", at.x, at.y);
  await sleep(150);
}

test("the format list of the quick dialogs draws a thin red outline from the engine's answer, in one undo step", { skip }, async () => {
  await js(`window.__EASYOFFICE_SHEET_RULES__.replaceAll({})`);
  await click("B2");
  for (let i = 0; i < 4; i += 1) await key("ArrowDown", SHIFT);
  await chooseFromMenu(["조건부 서식", "셀 강조 규칙"], "보다 큼…");
  await typeInto("sheet-cfq-a", "3");
  await openFormatList();
  const labels = JSON.parse(await js(`JSON.stringify([...document.querySelectorAll("#sheet-cfq-dialog .custom-select-option")].map((o) => o.textContent))`));
  assert.deepEqual(labels.slice(-2), ["빨강 테두리", "사용자 지정 형식…"]);
  // The drop-down is longer than the dialog under it, so the dialog is scrolled to show the whole list.
  await js(`document.querySelector('#sheet-cfq-dialog .custom-select-option[data-value="custom"]').scrollIntoView({ block: "nearest" })`);
  await sleep(100);
  await shoot("format-list");
  const option = await centerOf(`document.querySelector('#sheet-cfq-dialog .custom-select-option[data-value="red-border"]')`);
  await mouse("mouseMoved", option.x, option.y);
  await mouse("mousePressed", option.x, option.y);
  await mouse("mouseReleased", option.x, option.y);
  await until(async () => (await computed("B5", "borderRightColor")) === RED, "the outline of B5 is drawn");
  assert.equal(await computed("B4", "borderBottomColor"), RED, "the top edge of B5 sits in the slot of the cell above");
  assert.equal(await computed("A5", "borderRightColor"), RED, "and its left edge in the slot of the cell before it");
  assert.equal(await computed("B6", "borderBottomColor"), RED);
  assert.notEqual(await computed("B3", "borderRightColor"), RED, "B3 is not greater than 3");
  assert.equal((await look("B5")).bg, "", "a border alone paints no fill");
  assert.equal((await rulesOf())[0].bd.t, "thin:ff0000");
  await shoot("red-border-preview");
  await ok();
  await click("B2");
  await shoot("red-border-sheet");
  await key("z", CTRL);
  await sleep(300);
  assert.deepEqual(await rulesOf(), [], "one undo takes the rule away");
  await until(async () => (await computed("B5", "borderRightColor")) !== RED, "the outline is gone after undo");
  assert.notEqual(await computed("B4", "borderBottomColor"), RED);
  await key("y", CTRL);
  await sleep(300);
  assert.equal((await rulesOf()).length, 1, "redo brings the rule back");
  await until(async () => (await computed("B5", "borderRightColor")) === RED, "the outline is back after redo");
});

test("the custom format shows a rule's border row and the manager draws what it stores", { skip }, async () => {
  await js(`window.__EASYOFFICE_SHEET_RULES__.replaceAll({})`);
  await click("B2");
  for (let i = 0; i < 4; i += 1) await key("ArrowDown", SHIFT);
  await chooseFromMenu(["조건부 서식"], "새 규칙…");
  assert.equal(await dialogOpen("sheet-cf-dialog"), true);
  await js(`(() => { for (const [id, value] of [["sheet-cf-bd-on", true], ["sheet-cf-bd-line", "dashed"], ["sheet-cf-bd", "#0000ff"], ["sheet-cf-bg-on", false]]) { const el = document.getElementById(id); if (typeof value === "boolean") el.checked = value; else el.value = value; el.dispatchEvent(new Event("change", { bubbles: true })); } })()`);
  await typeInto("sheet-cf-a", "3");
  await shoot("manager-border-row");
  await js(`document.getElementById("btn-sheet-cf-apply").click()`);
  await until(async () => (await computed("B5", "borderRightColor")) === "rgb(0, 0, 255)", "the dashed blue outline");
  assert.equal(await computed("B5", "borderRightStyle"), "dashed");
  assert.equal(await computed("B4", "borderBottomStyle"), "dashed", "the top edge of B5 sits in the slot of the cell above");
  await js(`document.getElementById("sheet-cf-dialog").close()`);
  await js(`window.__EASYOFFICE_SHEET_RULES__.replaceAll({})`);
  await until(async () => (await computed("B5", "borderRightColor")) !== "rgb(0, 0, 255)", "replacing the rules takes the edges off");
});

test("the custom format of a quick dialog opens the manager with the rule", { skip }, async () => {
  await js(`window.__EASYOFFICE_SHEET_RULES__.replaceAll({})`);
  await click("B2");
  for (let i = 0; i < 4; i += 1) await key("ArrowDown", SHIFT);
  await chooseFromMenu(["조건부 서식", "상위/하위 규칙"], "상위 10%…");
  await typeInto("sheet-cfq-n", "40");
  assert.equal((await rulesOf())[0].percent, true);
  await js(`(() => { const s = document.getElementById("sheet-cfq-format-select"); s.value = "custom"; s.dispatchEvent(new Event("change", { bubbles: true })); })()`);
  await until(async () => (await rulesOf()).length === 0, "the preview is off for a custom format");
  await ok();
  assert.equal(await dialogOpen("sheet-cf-dialog"), true);
  assert.equal(await field("sheet-cf-type-select"), "top");
  assert.equal(await field("sheet-cf-n"), "40");
  await shoot("manager-new-rule");
  await js(`document.getElementById("sheet-cf-dialog").close()`);
  assert.deepEqual(await rulesOf(), []);
});

test("extended data bar options draw RTL border and midpoint axis in real engine and clear-table/clear-pivot cut only their own ranges", { skip }, async () => {
  await js(`window.__EASYOFFICE_SHEET_RULES__.replaceAll({}); window.__EASYOFFICE_SHEET_LAYOUT__.replaceAll({});`);
  for (const [ref, value] of [["B2", "-10"], ["B3", "0"], ["B4", "10"]]) await put(ref, value);
  await click("B2");
  for (let i = 0; i < 2; i += 1) await key("ArrowDown", SHIFT);
  await chooseFromMenu(["조건부 서식"], "새 규칙…");
  await js(`(() => {
    const set = (id, v) => { const el = document.getElementById(id); el.value = v; el.dispatchEvent(new Event("change", { bubbles: true })); };
    set("sheet-cf-type-select", "bar");
    set("sheet-cf-bar-dir", "rightToLeft");
    set("sheet-cf-bar-border", "solid");
    set("sheet-cf-bar-bpos", "#112233");
    set("sheet-cf-bar-bneg", "#990000");
    set("sheet-cf-bar-axis", "midpoint");
    set("sheet-cf-bar-axis-color", "#445566");
  })()`);
  await js(`document.getElementById("btn-sheet-cf-apply").click()`);
  const barCss = await painted("B4", "bar");
  assert.match(barCss, /repeating-linear-gradient\(180deg, #445566/);
  assert.match(barCss, /#112233/);
  assert.match(barCss, /transparent 0%, .*50%, transparent 50%/);
  await until(async () => (await js(`document.querySelector(${JSON.stringify(td("B3"))}).style.getPropertyValue("--cf-bar")`)).includes("#445566"), "zero value still draws midpoint axis");
  await shoot("databar-rtl-border-axis");
  await js(`document.getElementById("sheet-cf-dialog").close()`);

  // Add a table on B2:C3 and a pivot on E2:F4, and verify 규칙 지우기 sub-items enable/clear per active range.
  await js(`(() => {
    const L = window.__EASYOFFICE_SHEET_LAYOUT__;
    L.setTables("Sheet1", [{ name: "T1", range: "B2:C3", columns: ["B", "C"] }]);
    L.setPivots("Sheet1", [{ name: "P1", sourceSheet: "Sheet1", sourceRange: "B2:C3", location: "E2:F4", fieldNames: ["B", "C"], rowFields: [0] }]);
  })()`);
  await click("D5");
  await until(async () => (await js(`document.querySelector('[data-sheet-cf="clear-table"]').getAttribute("aria-disabled")`)) === "true", "clear-table disabled on D5");
  assert.equal(await js(`document.querySelector('[data-sheet-cf="clear-pivot"]').getAttribute("aria-disabled")`), "true");

  await click("B2");
  await until(async () => (await js(`document.querySelector('[data-sheet-cf="clear-table"]').getAttribute("aria-disabled")`)) === "false", "clear-table enabled on B2");
  await chooseFromMenu(["조건부 서식", "규칙 지우기"], "이 표에서 규칙 지우기");
  await until(async () => (await rulesOf())[0].r === "B4", "table rows B2:B3 cut out, B4 stays");

  await key("z", CTRL);
  await until(async () => (await rulesOf())[0].r === "B2:B4", "clearing a table is one undo step");

  // Also test clear-pivot and manage-rules table/pivot scope in real Chrome.
  await click("D2");
  await key("ArrowRight", SHIFT);
  await key("ArrowRight", SHIFT);
  await key("ArrowDown", SHIFT);
  await chooseFromMenu(["조건부 서식", "상위/하위 규칙"], "평균 초과…");
  await ok();
  await click("E2");
  await until(async () => (await js(`document.querySelector('[data-sheet-cf="clear-pivot"]').getAttribute("aria-disabled")`)) === "false", "clear-pivot enabled inside pivot E2");
  await chooseFromMenu(["조건부 서식", "규칙 지우기"], "이 피벗 테이블에서 규칙 지우기");
  await until(async () => (await rulesOf())[0].r === "D2:D3", "pivot area E2:F3 cut out, D2:D3 stays");
  await chooseFromMenu(["조건부 서식"], "규칙 관리…");
  assert.equal(await js(`document.querySelector('#sheet-cf-scope-select option[value="pivot"]').disabled`), false, "pivot scope enabled on E2");
  assert.equal(await js(`document.querySelector('#sheet-cf-scope-select option[value="table"]').disabled`), true, "table scope disabled on E2");
  await js(`document.getElementById("sheet-cf-dialog").close()`);
  await click("B2");
  await chooseFromMenu(["조건부 서식"], "규칙 관리…");
  assert.equal(await js(`document.querySelector('#sheet-cf-scope-select option[value="table"]').disabled`), false, "table scope enabled on B2");
  assert.equal(await js(`document.querySelector('#sheet-cf-scope-select option[value="pivot"]').disabled`), true, "pivot scope disabled on B2");
  await js(`(() => { const s = document.getElementById("sheet-cf-scope-select"); s.value = "table"; s.dispatchEvent(new Event("change", { bubbles: true })); })()`);
  const tableList = JSON.parse(await js(`JSON.stringify([...document.querySelectorAll("#sheet-cf-list li")].map((li) => li.textContent))`));
  assert.equal(tableList.length, 1);
  assert.match(tableList[0], /B2:B4 · 데이터 막대/);
  await js(`document.getElementById("sheet-cf-dialog").close()`);
  await js(`window.__EASYOFFICE_SHEET_RULES__.replaceAll({}); window.__EASYOFFICE_SHEET_LAYOUT__.replaceAll({});`);
});
