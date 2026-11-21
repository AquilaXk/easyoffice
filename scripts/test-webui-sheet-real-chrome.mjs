import test, { after, before } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { REPO, sleep, startBrowser, startEngine, startUiServer } from "./bench-webui-support.mjs";
import { findChrome } from "./hwp-studio-harness.mjs";

// The sheet in a real headless browser against the real engine (PD-280): the edit box that grows with its text and the
// movement keys, driven by key and mouse events over CDP with the browser's own profile. It needs a built
// `easyoffice-app` (`EO_APP`, or `target-193/debug` or `target/debug` of this checkout) and a Chrome (`CHROME_PATH` or the
// usual install places); without them the tests are skipped.

const exe = process.platform === "win32" ? "easyoffice-app.exe" : "easyoffice-app";
const APP = [process.env.EO_APP, ...["target-193", "target"].map((dir) => path.join(REPO, dir, "debug", exe))].find((file) => file && fs.existsSync(file));
let CHROME = null;
try {
  CHROME = findChrome();
} catch {
  CHROME = null;
}
const skip = !APP || !CHROME ? "needs a built easyoffice-app and a Chrome (EO_APP, CHROME_PATH)" : false;

const KEYS = {
  Enter: [13, "Enter"], Tab: [9, "Tab"], Backspace: [8, "Backspace"], Escape: [27, "Escape"], Home: [36, "Home"], End: [35, "End"], PageUp: [33, "PageUp"], PageDown: [34, "PageDown"],
  ArrowLeft: [37, "ArrowLeft"], ArrowUp: [38, "ArrowUp"], ArrowRight: [39, "ArrowRight"], ArrowDown: [40, "ArrowDown"],
};
const CTRL = 2;
const SHIFT = 8;
const ALT = 1;

let ctx = null;

before(async () => {
  if (skip) return;
  const port = 20000 + Math.floor(Math.random() * 20000);
  const engine = startEngine(APP, port);
  await sleep(800);
  const server = await startUiServer({ enginePort: port, port: port + 1 });
  const browser = await startBrowser({ chrome: CHROME, debugPort: port + 2, url: `http://127.0.0.1:${port + 1}/?module=SHEET&win=real-chrome`, width: 1100, height: 700 });
  ctx = { engine, server, browser, cdp: browser.cdp };
  await sleep(1000);
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
const rectOf = (selector) => js(`(() => { const el = document.querySelector(${JSON.stringify(selector)}); if (!el) return null; const r = el.getBoundingClientRect(); return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, width: r.width, height: r.height }; })()`);
const td = (ref) => `#pane-sheet td[data-cell="${ref}"]`;
const rowHeight = (ref) => js(`document.querySelector(${JSON.stringify(td(ref))}).closest("tr").getBoundingClientRect().height`);
const mouse = (type, x, y) => ctx.cdp.send("Input.dispatchMouseEvent", { type, x, y, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1 });
// Scrolls the grid so the row of `ref` is near the top (rows are 24px until a test sets one), then waits for the window.
async function show(ref) {
  const row = Number(ref.replace(/^[A-Z]+/, ""));
  await js(`(() => { const vp = document.getElementById("sheet-viewport"); vp.scrollTop = ${Math.max(0, row - 6) * 24}; vp.scrollLeft = 0; })()`);
  await sleep(80);
}
async function click(ref) {
  await show(ref);
  const r = await rectOf(td(ref));
  assert.ok(r, `${ref} is on the screen`);
  await mouse("mouseMoved", r.left + r.width / 2, r.top + r.height / 2);
  await mouse("mousePressed", r.left + r.width / 2, r.top + r.height / 2);
  await mouse("mouseReleased", r.left + r.width / 2, r.top + r.height / 2);
}
const active = () => js(`document.getElementById("active-cell-ref").textContent`);
const cellText = (ref) => js(`(document.querySelector(${JSON.stringify(td(ref))}) || {}).textContent`);
const inRange = () => js(`JSON.stringify([...document.querySelectorAll("#pane-sheet td.in-range, #pane-sheet td.selected-cell")].map((c) => c.dataset.cell).sort((a, b) => a.localeCompare(b)))`).then(JSON.parse);
// Types `value` into `ref` and ends with `end` (Enter or Tab).
async function put(ref, value, end = "Enter") {
  await click(ref);
  await text(value);
  await key(end);
  await sleep(60);
}

const CELLS = { B2: "1", B3: "2", B4: "3", B7: "4", B8: "5", C2: "x", D2: "y", G2: "z" };

test("the cells of the sheet are typed with the keys a user presses", { skip }, async () => {
  for (const [ref, value] of Object.entries(CELLS)) await put(ref, value);
  for (const [ref, value] of Object.entries(CELLS)) assert.equal(await cellText(ref), value, ref);
});

test("Ctrl+Down and Ctrl+Right walk runs and gaps; Ctrl+Shift+Down selects them", { skip }, async () => {
  await click("B2");
  const seen = [];
  for (let i = 0; i < 3; i += 1) {
    await key("ArrowDown", CTRL);
    seen.push(await active());
  }
  assert.deepEqual(seen, ["B4", "B7", "B8"]);
  await key("ArrowDown", CTRL);
  assert.match(await active(), /^B\d{6,7}$/, "the last row of the sheet");
  await key("ArrowUp", CTRL);
  assert.equal(await active(), "B8");
  await click("B2");
  await key("ArrowRight", CTRL);
  assert.equal(await active(), "D2");
  await key("ArrowRight", CTRL);
  assert.equal(await active(), "G2");
  await click("B2");
  await key("ArrowDown", CTRL | SHIFT);
  assert.deepEqual(await inRange(), ["B2", "B3", "B4"]);
});

test("Enter and Tab walk a range; typed values are committed in it and the range stays", { skip }, async () => {
  await click("B2");
  await key("ArrowDown", SHIFT);
  await key("ArrowRight", SHIFT);
  assert.deepEqual(await inRange(), ["B2", "B3", "C2", "C3"]);
  const seen = [];
  for (let i = 0; i < 4; i += 1) {
    await key("Enter");
    seen.push(await active());
  }
  assert.deepEqual(seen, ["B3", "C2", "C3", "B2"]);
  await key("Tab");
  assert.equal(await active(), "C2");
  await text("7");
  await key("Tab");
  await sleep(60);
  assert.equal(await cellText("C2"), "7", "the value typed in the range is committed");
  assert.equal(await active(), "B3");
  assert.deepEqual(await inRange(), ["B2", "B3", "C2", "C3"]);
});

test("a row of Tabs sends the Enter back to the column it started in", { skip }, async () => {
  await put("B10", "a", "Tab");
  await text("b");
  await key("Tab");
  await text("c");
  await key("Enter");
  await sleep(60);
  assert.equal(await active(), "B11");
  assert.deepEqual([await cellText("B10"), await cellText("C10"), await cellText("D10")], ["a", "b", "c"]);
});

test("Home, Ctrl+Home, Ctrl+End and the page keys", { skip }, async () => {
  await click("E5");
  await key("Home");
  assert.equal(await active(), "A5");
  await key("End", CTRL);
  assert.equal(await active(), "G10", "the last used row holds the Tab row, the last used column is G");
  await key("Home", CTRL);
  assert.equal(await active(), "A1");
  await key("PageDown");
  const down = Number((await active()).slice(1));
  assert.ok(down > 10 && down < 40, `a screen of rows (${down})`);
  await key("PageUp");
  assert.equal(await active(), "A1");
  await key("PageDown", ALT);
  const across = await active();
  assert.match(across, /^[B-Z]1$/, `a screen of columns (${across})`);
  await key("PageUp", ALT);
  assert.equal(await active(), "A1");
});

test("the cell being edited grows to the right over its neighbours, keeps the rows where they are, and goes back with Esc", { skip }, async () => {
  await show("B14");
  const before = await rectOf(td("B14"));
  const below = await rectOf(td("B15"));
  await click("B14");
  await text("글자가 칸보다 길어지면 편집 칸이 오른쪽으로 넓어진다 abcdefghij");
  const grown = await rectOf(td("B14"));
  assert.ok(grown.width > before.width + 150, `the box grew from ${before.width} to ${grown.width}`);
  assert.equal(grown.height, before.height);
  assert.equal(grown.left, before.left, "it grows from its left edge");
  assert.equal((await rectOf(td("B15"))).top, below.top, "the row below stays");
  assert.equal(await js(`document.querySelector(${JSON.stringify(td("B14"))}).classList.contains("edit-grow")`), true);
  await key("Backspace");
  await key("Escape");
  const back = await rectOf(td("B14"));
  assert.equal(back.width, before.width);
  assert.equal(await js(`document.querySelector(${JSON.stringify(td("B14"))}).style.getPropertyValue("--ew")`), "");
});

test("a long text reaching the right edge of the view wraps and the box grows down, over the rows below", { skip }, async () => {
  await click("H16");
  await text("가".repeat(150));
  const box = await rectOf(td("H16"));
  const view = await rectOf("#sheet-viewport");
  const below = await rectOf(td("H18"));
  assert.ok(box.right <= view.right, `the box (${box.right}) stays in the view (${view.right})`);
  assert.ok(box.height > 60, `the box wrapped to ${box.height}px`);
  assert.ok(below.top < box.bottom, "it lies over the rows below");
  assert.equal(await rowHeight("H16"), 24, "the row keeps its height");
  await key("Enter");
  await sleep(100);
  assert.equal((await rectOf(td("H16"))).height, 24);
  assert.equal(await cellText("H16"), "가".repeat(150));
});

test("a wrapping cell in a row whose height is set by hand grows down over the rows below", { skip }, async () => {
  await show("B20");
  const head = await rectOf('#pane-sheet th[data-row="20"] .row-resizer');
  assert.ok(head, "the row resizer of row 20");
  const x = head.left + head.width / 2;
  const y = head.top + head.height / 2;
  await mouse("mouseMoved", x, y);
  await mouse("mousePressed", x, y);
  await mouse("mouseMoved", x, y + 12);
  await mouse("mouseReleased", x, y + 12);
  await click("B20");
  await js(`[...document.querySelectorAll("#icon-toolbar .tb-btn")].find((b) => (b.textContent || "").replace(/\\s+/g, "").includes("줄바꿈")).click()`);
  await sleep(200);
  await click("B20");
  const fixed = await rectOf(td("B20"));
  assert.ok(fixed.height > 30 && fixed.height < 40, `the row is 36px (${fixed.height})`);
  const below = await rectOf(td("B22"));
  await text("줄바꿈 서식 칸은 높이를 정한 행에서도 아래로 넓어진다 아래로 넓어진다");
  const grown = await rectOf(td("B20"));
  assert.ok(grown.height > fixed.height + 20, `the box grew from ${fixed.height} to ${grown.height}`);
  assert.equal(grown.width, fixed.width, "a wrapping cell keeps its width");
  assert.equal((await rectOf(td("B22"))).top, below.top, "the rows below stay");
  await key("Escape");
});

test("the cell format dialog lists the categories in order and shows what the engine makes of the 기타 and fraction codes", { skip }, async () => {
  await click("B2");
  await js(`document.querySelector('[data-sheet-numfmt="dialog"]').click()`);
  await sleep(500);
  const kinds = JSON.parse(await js(`JSON.stringify([...document.querySelectorAll("#sheet-numfmt-kind-select option")].map((o) => o.textContent))`));
  assert.deepEqual(kinds, ["일반", "숫자", "통화", "회계", "날짜", "시간", "백분율", "분수", "지수", "텍스트", "기타", "사용자 지정"]);
  const sampleOf = async (kind, code) => {
    await js(`(() => { const k = document.getElementById("sheet-numfmt-kind-select"); k.value = ${JSON.stringify(kind)}; k.dispatchEvent(new Event("change", { bubbles: true })); })()`);
    await sleep(300);
    await js(`(() => { const l = document.getElementById("sheet-numfmt-list-select"); l.value = ${JSON.stringify(code)}; l.dispatchEvent(new Event("change", { bubbles: true })); })()`);
    await sleep(400);
    return js(`document.getElementById("sheet-numfmt-sample").textContent`);
  };
  assert.equal(await sampleOf("special", "00000"), "00001", "B2 holds 1");
  assert.equal(await sampleOf("special", "000000-0000000"), "000000-0000001");
  assert.equal(await sampleOf("special", "[DBNum4]General"), "일");
  assert.equal((await sampleOf("fraction", "# ?/?")).trim(), "1", "a whole number keeps the blanks of its fraction part");
  await js(`document.getElementById("sheet-numfmt-dialog").close?.()`);
});
