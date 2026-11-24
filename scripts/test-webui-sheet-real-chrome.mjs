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
  Enter: [13, "Enter"], Tab: [9, "Tab"], Backspace: [8, "Backspace"], Escape: [27, "Escape"], Home: [36, "Home"], End: [35, "End"], PageUp: [33, "PageUp"], PageDown: [34, "PageDown"], F2: [113, "F2"], F3: [114, "F3"],
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
const mouse = (type, x, y, clickCount = 1) => ctx.cdp.send("Input.dispatchMouseEvent", { type, x, y, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount });
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

test("Ctrl+Home on frozen panes goes to the top-left shown cell of the unfrozen pane and resets scroll (PD-350)", { skip }, async () => {
  await click("B3");
  await js(`document.querySelector('.md-item[data-studio-menu-cmd="sheet-mod-freeze"]').click()`);
  await sleep(150);
  try {
    await click("G30");
    await js(`(() => {
      const vp = document.getElementById("sheet-viewport");
      vp.scrollTop = 600;
      vp.scrollLeft = 240;
    })()`);
    await sleep(80);
    await key("Home", CTRL);
    await sleep(80);
    assert.equal(await active(), "B3", "Ctrl+Home moves to B3 when rows 1-2 and col A are frozen");
    const scrolled = await js(`JSON.stringify((() => {
      const vp = document.getElementById("sheet-viewport");
      return { top: vp.scrollTop, left: vp.scrollLeft };
    })())`).then(JSON.parse);
    assert.deepEqual(scrolled, { top: 0, left: 0 }, "Ctrl+Home resets scrollable pane offsets back to the freeze lines");

    await click("A1");
    await key("Home", CTRL);
    assert.equal(await active(), "B3", "Ctrl+Home from inside the frozen pane moves to B3");

    await click("D4");
    await key("Home");
    assert.equal(await active(), "A4", "Home without Ctrl returns to column A");

    await click("D4");
    await key("Home", SHIFT);
    assert.deepEqual(await inRange(), ["A4", "B4", "C4", "D4"], "Shift+Home highlights frozen column A in range");

    await click("A1");
    await key("Home", CTRL | SHIFT);
    assert.deepEqual(await inRange(), ["A1", "A2", "A3", "B1", "B2", "B3"], "Ctrl+Shift+Home from A1 highlights frozen and unfrozen cells");

    await click("D4");
    await key("Home", CTRL | SHIFT);
    assert.deepEqual(await inRange(), ["B3", "B4", "C3", "C4", "D3", "D4"], "Ctrl+Shift+Home extends from D4 to B3");

    await js(`window.__EASYOFFICE_SHEET_LAYOUT__.addMerge("Sheet1", 2, 1, 4, 3)`);
    await click("G30");
    await js(`(() => {
      const vp = document.getElementById("sheet-viewport");
      vp.scrollTop = 600;
      vp.scrollLeft = 240;
    })()`);
    await sleep(80);
    await key("Home", CTRL);
    await sleep(80);
    assert.equal(await active(), "A2", "cross-freeze merge lands on A2");
    const mergeScroll = await js(`JSON.stringify((() => {
      const vp = document.getElementById("sheet-viewport");
      return { top: vp.scrollTop, left: vp.scrollLeft };
    })())`).then(JSON.parse);
    assert.deepEqual(mergeScroll, { top: 0, left: 0 }, "cross-freeze merge resets scrollable offsets on Ctrl+Home");
  } finally {
    await js(`window.__EASYOFFICE_SHEET_LAYOUT__.removeMerges("Sheet1", 1, 1, 10, 10)`);
    await js(`document.querySelector('.md-item[data-sheet-act="freeze-clear"]').click()`);
    await sleep(120);
    await click("A1");
    await key("Home", CTRL);
    assert.equal(await active(), "A1");
  }
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
  assert.equal(await rowHeight("H16"), 22, "the row keeps its height");
  await key("Enter");
  await sleep(100);
  assert.equal((await rectOf(td("H16"))).height, 22);
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

test("F3 (이름 붙여넣기) handles empty focus, arrow keys, double-click, menu aria-expanded, and outside click in real Chrome", { skip }, async () => {
  await click("E2");
  await key("F3");
  await sleep(100);
  assert.equal(await js(`document.getElementById("sheet-name-paste-dialog").open`), true);
  assert.equal(await js(`document.activeElement?.classList?.contains("dialog-close")`), false, "empty F3 never focuses the close button");
  await key("Escape");
  await sleep(80);
  assert.equal(await js(`document.getElementById("sheet-name-paste-dialog").open`), false);

  const defineViaNameBox = async (ref, name) => {
    await click(ref);
    const nb = await rectOf("#active-cell-ref");
    assert.ok(nb);
    await mouse("mouseMoved", nb.left + nb.width / 2, nb.top + nb.height / 2);
    await mouse("mousePressed", nb.left + nb.width / 2, nb.top + nb.height / 2);
    await mouse("mouseReleased", nb.left + nb.width / 2, nb.top + nb.height / 2);
    await js(`(() => { const el = document.getElementById("active-cell-ref"); el.focus(); const r = document.createRange(); r.selectNodeContents(el); const s = getSelection(); s.removeAllRanges(); s.addRange(r); })()`);
    await text(name);
    await key("Enter");
    await sleep(120);
  };
  await defineViaNameBox("B2", "가_첫째");
  await defineViaNameBox("B4", "나_둘째");

  await click("E2");
  await text("=10+");
  await sleep(80);
  await key("F3");
  await sleep(120);
  assert.equal(await js(`document.getElementById("sheet-name-paste-dialog").open`), true);
  assert.equal(await js(`document.querySelector('#sheet-name-paste-list li[aria-selected="true"]')?.dataset?.index`), "0");
  await key("ArrowDown");
  await sleep(60);
  assert.equal(await js(`document.querySelector('#sheet-name-paste-list li[aria-selected="true"]')?.dataset?.index`), "1");
  await key("ArrowUp");
  await sleep(60);
  assert.equal(await js(`document.querySelector('#sheet-name-paste-list li[aria-selected="true"]')?.dataset?.index`), "0");

  const row1 = await rectOf('#sheet-name-paste-list li[data-index="1"]');
  assert.ok(row1, "second defined name row is rendered");
  const rx = row1.left + row1.width / 2;
  const ry = row1.top + row1.height / 2;
  await mouse("mouseMoved", rx, ry, 1);
  await mouse("mousePressed", rx, ry, 1);
  await mouse("mouseReleased", rx, ry, 1);
  await mouse("mousePressed", rx, ry, 2);
  await mouse("mouseReleased", rx, ry, 2);
  await sleep(120);

  assert.equal(await js(`document.getElementById("sheet-name-paste-dialog").open`), false, "double-click confirms and closes dialog");
  assert.equal(await cellText("E2"), "=10+나_둘째", "double-clicked name inserts without premature commit");
  await key("Enter");
  await sleep(150);
  assert.equal(await cellText("E2"), "13", "real engine evaluates 10 + B4 (3)");

  // Opening 수식 -> 수식에서 사용 -> 이름 붙여넣기 resets aria-expanded cleanly and outside click closes Name Box dropdown.
  const openUseMenu = async () => {
    const formulaMenu = await rectOf('.menu-item[data-menu="formula"] .menu-title');
    assert.ok(formulaMenu);
    await mouse("mouseMoved", formulaMenu.left + formulaMenu.width / 2, formulaMenu.top + formulaMenu.height / 2, 1);
    await mouse("mousePressed", formulaMenu.left + formulaMenu.width / 2, formulaMenu.top + formulaMenu.height / 2, 1);
    await mouse("mouseReleased", formulaMenu.left + formulaMenu.width / 2, formulaMenu.top + formulaMenu.height / 2, 1);
    await sleep(80);
    assert.equal(await js(`document.querySelector('.menu-item[data-menu="formula"] .menu-title')?.getAttribute("aria-expanded")`), "true");
    const subRect = await js(`(() => { const el = document.getElementById("sheet-name-use-menu")?.closest(".md-sub"); if (!el) return null; const r = el.getBoundingClientRect(); return { left: r.left, top: r.top, width: r.width, height: r.height }; })()`);
    assert.ok(subRect && subRect.width > 0, "수식에서 사용 submenu trigger is visible");
    await mouse("mouseMoved", subRect.left + subRect.width / 2, subRect.top + subRect.height / 2, 1);
    await sleep(80);
  };

  await click("F2");
  await text("=20+");
  await sleep(60);
  await openUseMenu();
  const useItem = await rectOf('#sheet-name-use-menu .md-item[data-name-use="가_첫째"]');
  assert.ok(useItem && useItem.width > 0, "가_첫째 item visible in 수식에서 사용");
  await mouse("mouseMoved", useItem.left + useItem.width / 2, useItem.top + useItem.height / 2, 1);
  await mouse("mousePressed", useItem.left + useItem.width / 2, useItem.top + useItem.height / 2, 1);
  await mouse("mouseReleased", useItem.left + useItem.width / 2, useItem.top + useItem.height / 2, 1);
  await sleep(100);
  assert.equal(await cellText("F2"), "=20+가_첫째");
  await key("Enter");
  await sleep(140);
  assert.equal(await cellText("F2"), "21", "real engine evaluates 20 + B2 (1)");

  await openUseMenu();
  const pasteItem = await rectOf('.md-item[data-sheet-act="name-paste"]');
  assert.ok(pasteItem && pasteItem.width > 0, "이름 붙여넣기 item visible in 수식에서 사용");
  await mouse("mouseMoved", pasteItem.left + pasteItem.width / 2, pasteItem.top + pasteItem.height / 2, 1);
  await mouse("mousePressed", pasteItem.left + pasteItem.width / 2, pasteItem.top + pasteItem.height / 2, 1);
  await mouse("mouseReleased", pasteItem.left + pasteItem.width / 2, pasteItem.top + pasteItem.height / 2, 1);
  await sleep(100);
  assert.equal(await js(`document.querySelector('.menu-item[data-menu="formula"] .menu-title')?.getAttribute("aria-expanded")`), "false");
  assert.equal(await js(`document.getElementById("sheet-name-paste-dialog").open`), true);
  await key("Escape");
  await sleep(80);

  const dropBtn = await rectOf("#btn-name-box-list");
  assert.ok(dropBtn);
  await click("G2");
  await text("=1+");
  await sleep(50);
  await mouse("mouseMoved", dropBtn.left + dropBtn.width / 2, dropBtn.top + dropBtn.height / 2, 1);
  await mouse("mousePressed", dropBtn.left + dropBtn.width / 2, dropBtn.top + dropBtn.height / 2, 1);
  await mouse("mouseReleased", dropBtn.left + dropBtn.width / 2, dropBtn.top + dropBtn.height / 2, 1);
  await sleep(60);
  assert.equal(await js(`document.getElementById("sheet-name-box-list").hidden`), false);
  await key("Escape");
  await sleep(60);
  assert.equal(await js(`document.getElementById("sheet-name-box-list").hidden`), true, "Escape closes Name Box dropdown");
  assert.equal(await cellText("G2"), "=1+", "Escape closing Name Box dropdown preserves active formula edit");
  await mouse("mouseMoved", dropBtn.left + dropBtn.width / 2, dropBtn.top + dropBtn.height / 2, 1);
  await mouse("mousePressed", dropBtn.left + dropBtn.width / 2, dropBtn.top + dropBtn.height / 2, 1);
  await mouse("mouseReleased", dropBtn.left + dropBtn.width / 2, dropBtn.top + dropBtn.height / 2, 1);
  await sleep(60);
  assert.equal(await js(`document.getElementById("sheet-name-box-list").hidden`), false);
  await click("C5");
  await sleep(60);
  assert.equal(await js(`document.getElementById("sheet-name-box-list").hidden`), true, "outside click closes Name Box dropdown");
});

test("Ctrl+Shift+PageDown/PageUp, Shift/Ctrl+click, and tab menu group selection work in real Chrome against the engine (PD-346)", { skip }, async () => {
  await click("A1");
  await js(`document.getElementById("btn-add-sheet-tab").click()`);
  await sleep(160);
  await js(`document.getElementById("btn-add-sheet-tab").click()`);
  await sleep(160);
  await js(`document.querySelector('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet1"]').click()`);
  await sleep(140);
  await click("A1");

  const snapTabs = async () =>
    JSON.parse(
      await js(`JSON.stringify([...document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]")].map((b) => ({
        name: b.dataset.sheetTab,
        active: b.classList.contains("active"),
        grouped: b.dataset.tabGrouped || null,
        selected: b.getAttribute("aria-selected"),
      })))`)
    );

  // Ctrl+Shift+PageDown twice: Sheet1 -> Sheet2 -> Sheet3; active moves and anchor keeps Sheet1..Sheet3 grouped.
  await key("PageDown", CTRL | SHIFT);
  await sleep(140);
  let tabs = await snapTabs();
  assert.deepEqual(tabs, [
    { name: "Sheet1", active: false, grouped: "true", selected: "true" },
    { name: "Sheet2", active: true, grouped: null, selected: "true" },
    { name: "Sheet3", active: false, grouped: null, selected: "false" },
  ]);

  // Visual style on grouped inactive tab uses design tokens and differs from unselected tabs.
  const visuals = JSON.parse(
    await js(`JSON.stringify((() => {
      const s1 = getComputedStyle(document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]'));
      const s3 = getComputedStyle(document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet3"]'));
      return { s1Bg: s1.backgroundColor, s1Shadow: s1.boxShadow, s3Bg: s3.backgroundColor, s3Shadow: s3.boxShadow };
    })())`)
  );
  assert.notEqual(visuals.s1Bg, visuals.s3Bg, "grouped inactive tab background distinguishes it from unselected tabs");
  assert.notEqual(visuals.s1Shadow, visuals.s3Shadow, "grouped inactive tab shows bottom group indicator shadow");

  await key("PageDown", CTRL | SHIFT);
  await sleep(140);
  tabs = await snapTabs();
  assert.deepEqual(tabs.map((t) => [t.name, t.active, t.selected]), [
    ["Sheet1", false, "true"],
    ["Sheet2", false, "true"],
    ["Sheet3", true, "true"],
  ]);

  // Ctrl+Shift+PageUp shrinks back toward anchor Sheet1 -> [Sheet1, Sheet2].
  await key("PageUp", CTRL | SHIFT);
  await sleep(140);
  tabs = await snapTabs();
  assert.deepEqual(tabs.map((t) => [t.name, t.active, t.grouped, t.selected]), [
    ["Sheet1", false, "true", "true"],
    ["Sheet2", true, null, "true"],
    ["Sheet3", false, null, "false"],
  ]);

  // Plain Ctrl+PageUp clears group and moves to Sheet1.
  await key("PageUp", CTRL);
  await sleep(140);
  tabs = await snapTabs();
  assert.deepEqual(tabs.map((t) => [t.name, t.active, t.grouped, t.selected]), [
    ["Sheet1", true, null, "true"],
    ["Sheet2", false, null, "false"],
    ["Sheet3", false, null, "false"],
  ]);

  // Right-click tab -> 모든 시트 선택 -> verify focus returns to grid and Ctrl+Shift+PageDown/PageUp still works immediately!
  const tabRect = await rectOf('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet1"]');
  assert.ok(tabRect);
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x: tabRect.left + tabRect.width / 2, y: tabRect.top + tabRect.height / 2, button: "right", buttons: 2, clickCount: 1 });
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: tabRect.left + tabRect.width / 2, y: tabRect.top + tabRect.height / 2, button: "right", buttons: 0, clickCount: 1 });
  await sleep(80);
  await js(`document.querySelector('#sheet-tab-menu [data-tab-act="select-all"]').click()`);
  await sleep(100);
  tabs = await snapTabs();
  assert.equal(tabs.every((t) => t.selected === "true"), true, "모든 시트 선택 selects every visible tab");
  assert.equal(await js(`document.activeElement?.tagName`), "TD", "focus restores to grid after 모든 시트 선택");
  await key("PageDown", CTRL | SHIFT);
  await sleep(120);
  tabs = await snapTabs();
  assert.deepEqual(tabs.map((t) => [t.name, t.active, t.grouped, t.selected]), [
    ["Sheet1", false, "true", "true"],
    ["Sheet2", true, null, "true"],
    ["Sheet3", false, null, "false"],
  ]);
  await key("PageUp", CTRL);
  await sleep(120);

  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x: tabRect.left + tabRect.width / 2, y: tabRect.top + tabRect.height / 2, button: "right", buttons: 2, clickCount: 1 });
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: tabRect.left + tabRect.width / 2, y: tabRect.top + tabRect.height / 2, button: "right", buttons: 0, clickCount: 1 });
  await sleep(80);
  await js(`document.querySelector('#sheet-tab-menu [data-tab-act="select-all"]').click()`);
  await sleep(100);
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x: tabRect.left + tabRect.width / 2, y: tabRect.top + tabRect.height / 2, button: "right", buttons: 2, clickCount: 1 });
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: tabRect.left + tabRect.width / 2, y: tabRect.top + tabRect.height / 2, button: "right", buttons: 0, clickCount: 1 });
  await sleep(80);
  assert.equal(await js(`document.querySelector('#sheet-tab-menu [data-tab-act="delete"]').getAttribute("aria-disabled")`), "true");
  assert.equal(await js(`document.querySelector('#sheet-tab-menu [data-tab-act="hide"]').getAttribute("aria-disabled")`), "true");
  await js(`document.querySelector('#sheet-tab-menu [data-tab-act="ungroup"]').click()`);
  await sleep(100);
  tabs = await snapTabs();
  assert.deepEqual(tabs.map((t) => [t.name, t.active, t.grouped, t.selected]), [
    ["Sheet1", true, null, "true"],
    ["Sheet2", false, null, "false"],
    ["Sheet3", false, null, "false"],
  ]);

  // Shift+click Sheet3 groups all 3 via mouse; Cmd/Ctrl+click Sheet2 toggles it out cleanly.
  const r3 = await rectOf('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet3"]');
  assert.ok(r3);
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x: r3.left + r3.width / 2, y: r3.top + r3.height / 2, button: "left", buttons: 1, clickCount: 1, modifiers: SHIFT });
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: r3.left + r3.width / 2, y: r3.top + r3.height / 2, button: "left", buttons: 0, clickCount: 1, modifiers: SHIFT });
  await sleep(120);
  tabs = await snapTabs();
  assert.deepEqual(tabs.map((t) => [t.name, t.active, t.grouped, t.selected]), [
    ["Sheet1", false, "true", "true"],
    ["Sheet2", false, "true", "true"],
    ["Sheet3", true, null, "true"],
  ]);

  // Tab color + grouped + .drop-before must still show drop indicator shadow (CSS specificity check).
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x: tabRect.left + tabRect.width / 2, y: tabRect.top + tabRect.height / 2, button: "right", buttons: 2, clickCount: 1 });
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: tabRect.left + tabRect.width / 2, y: tabRect.top + tabRect.height / 2, button: "right", buttons: 0, clickCount: 1 });
  await sleep(80);
  await js(`document.querySelector('#sheet-tab-color-panel .sb-hl-swatch[title="#ff0000"]').click()`);
  await sleep(140);
  const dropCssCheck = JSON.parse(
    await js(`JSON.stringify((() => {
      const btn = document.querySelector('.sheet-tab-btn[data-sheet-tab="Sheet1"]');
      const before = getComputedStyle(btn).boxShadow;
      btn.classList.add("drop-before");
      const withDrop = getComputedStyle(btn).boxShadow;
      btn.classList.remove("drop-before");
      return { grouped: btn.dataset.tabGrouped, color: btn.dataset.tabColor, before, withDrop };
    })())`)
  );
  assert.equal(dropCssCheck.grouped, "true");
  assert.equal(dropCssCheck.color, "ff0000");
  assert.notEqual(dropCssCheck.withDrop, dropCssCheck.before, "drop-before remains visible on grouped colored tab");

  const modToggle = process.platform === "darwin" ? 4 : CTRL;
  const r2 = await rectOf('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet2"]');
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x: r2.left + r2.width / 2, y: r2.top + r2.height / 2, button: "left", buttons: 1, clickCount: 1, modifiers: modToggle });
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: r2.left + r2.width / 2, y: r2.top + r2.height / 2, button: "left", buttons: 0, clickCount: 1, modifiers: modToggle });
  await sleep(100);
  tabs = await snapTabs();
  assert.deepEqual(tabs.map((t) => [t.name, t.active, t.grouped, t.selected]), [
    ["Sheet1", false, "true", "true"],
    ["Sheet2", false, null, "false"],
    ["Sheet3", true, null, "true"],
  ]);

  // Clicking outside group (Sheet2) clears group cleanly.
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x: r2.left + r2.width / 2, y: r2.top + r2.height / 2, button: "left", buttons: 1, clickCount: 1 });
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: r2.left + r2.width / 2, y: r2.top + r2.height / 2, button: "left", buttons: 0, clickCount: 1 });
  await sleep(100);
  tabs = await snapTabs();
  assert.deepEqual(tabs.map((t) => [t.name, t.active, t.grouped, t.selected]), [
    ["Sheet1", false, null, "false"],
    ["Sheet2", true, null, "true"],
    ["Sheet3", false, null, "false"],
  ]);

  // Formula 3D point mode clears any active group without mixing data-tab-grouped and data-point-span; editing cell blocks Ctrl+Shift+PageDown.
  await click("A1");
  await key("PageDown", CTRL | SHIFT);
  await sleep(100);
  assert.deepEqual((await snapTabs()).filter((t) => t.selected === "true").map((t) => t.name), ["Sheet2", "Sheet3"]);
  await text("=SUM(");
  await sleep(60);
  await key("PageDown", CTRL | SHIFT);
  await sleep(80);
  assert.equal((await snapTabs()).find((t) => t.active).name, "Sheet3", "editing cell does not switch sheets on Ctrl+Shift+PageDown");
  await click("B2");
  await sleep(60);
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x: tabRect.left + tabRect.width / 2, y: tabRect.top + tabRect.height / 2, button: "left", buttons: 1, clickCount: 1, modifiers: SHIFT });
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: tabRect.left + tabRect.width / 2, y: tabRect.top + tabRect.height / 2, button: "left", buttons: 0, clickCount: 1, modifiers: SHIFT });
  await sleep(120);
  const pointCheck = JSON.parse(
    await js(`JSON.stringify([...document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]")].map((b) => ({
      name: b.dataset.sheetTab,
      pointSpan: b.dataset.pointSpan || null,
      grouped: b.dataset.tabGrouped || null,
    })))`)
  );
  assert.equal(pointCheck.some((t) => t.pointSpan === "true"), true, "formula 3D span marks tabs");
  assert.equal(pointCheck.every((t) => t.grouped === null), true, "formula 3D span leaves no data-tab-grouped");
  await key("Escape");
  await sleep(80);
});

test("screen default font applies Malgun Gothic 11pt stack, separates headers and doc tables, syncs large defaultFont row heights and print CSS", { skip }, async () => {
  await click("A1");
  const baseCheck = JSON.parse(
    await js(`JSON.stringify((() => {
      const tdEl = document.querySelector('#pane-sheet td[data-cell="A1"]');
      const thEl = document.querySelector('#pane-sheet th[data-col="A"]');
      const tbl = document.createElement("table");
      tbl.className = "sheet-table doc-inline-table";
      const tr = document.createElement("tr");
      const dth = document.createElement("th");
      const dtd = document.createElement("td");
      tr.append(dth, dtd);
      tbl.appendChild(tr);
      document.body.appendChild(tbl);
      const docTdCs = getComputedStyle(dtd);
      const docTdFontSize = parseFloat(docTdCs.fontSize);
      const docTdFontFamily = docTdCs.fontFamily;
      tbl.remove();
      const tdCs = getComputedStyle(tdEl);
      const thCs = getComputedStyle(thEl);
      return {
        tdFontSize: parseFloat(tdCs.fontSize),
        tdFontFamily: tdCs.fontFamily,
        thFontSize: parseFloat(thCs.fontSize),
        docTdFontSize,
        docTdFontFamily,
        rowH: tdEl.closest("tr").getBoundingClientRect().height,
      };
    })())`)
  );
  assert.ok(Math.abs(baseCheck.tdFontSize - (11 * 96) / 72) < 0.1, `sheet td defaults to 11pt (~14.67px), got ${baseCheck.tdFontSize}`);
  assert.ok(baseCheck.tdFontFamily.includes("맑은 고딕") || baseCheck.tdFontFamily.includes("Malgun Gothic"), baseCheck.tdFontFamily);
  assert.equal(baseCheck.thFontSize, 12, "sheet headers keep 12px UI font");
  assert.equal(baseCheck.docTdFontSize, 12, "doc-inline-table cells keep 12px UI font");
  assert.equal(baseCheck.rowH, 22, "11pt keeps 22px default row height");

  const openedLarge = await js(`(async () => {
    const api = window.__EASYOFFICE_WORKSPACE_API__;
    const resp = await fetch("/api/ipc", {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-EasyOffice-Ipc": "1" },
      body: JSON.stringify({
        command: "convert_document_format",
        payload: JSON.stringify({
          sourceFormat: "xlsx",
          targetFormat: "xlsx",
          title: "normal-font-20pt",
          forSave: true,
          sheets: [{ name: "Sheet1", cells: [{ ref: "A1", raw: "큰 기본 글꼴" }, { ref: "A2", raw: "줄바꿈 글자입니다 길게 씁니다", style: { wrap: true } }] }],
          defaultFont: { name: "Arial", size: 20 },
        }),
      }),
    });
    const res = await resp.json();
    if (!res.convertedBase64) throw new Error("convert failed: " + JSON.stringify(res));
    const bin = Uint8Array.from(atob(res.convertedBase64), (c) => c.charCodeAt(0));
    const origConfirm = window.confirm;
    window.confirm = () => true;
    try {
      await api.openDocumentFromBytes("normal-font-20pt.xlsx", bin);
    } finally {
      window.confirm = origConfirm;
    }
    await new Promise((r) => setTimeout(r, 150));
    const td1 = document.querySelector('#pane-sheet td[data-cell="A1"]');
    const td2 = document.querySelector('#pane-sheet td[data-cell="A2"]');
    const td1EditMeasure = (() => {
      td1.classList.add("edit-measure");
      const sz = parseFloat(getComputedStyle(td1).fontSize);
      td1.classList.remove("edit-measure");
      return sz;
    })();
    const preview = await window.__EASYOFFICE_OFFICE_PRINT__.buildPreview();
    const { root } = window.__EASYOFFICE_OFFICE_PRINT__.prepare();
    const preparedTd = root.querySelector(".eo-print-sheet td");
    const preparedCs = getComputedStyle(preparedTd);
    const previewFontSize = parseFloat(preparedCs.fontSize);
    const previewFontFamily = preparedCs.fontFamily;
    window.__EASYOFFICE_OFFICE_PRINT__.remove();
    return JSON.stringify({
      td1FontSize: parseFloat(getComputedStyle(td1).fontSize),
      td1EditMeasure,
      td1FontFamily: getComputedStyle(td1).fontFamily,
      row1H: td1.closest("tr").getBoundingClientRect().height,
      td2LineH: parseFloat(getComputedStyle(td2).lineHeight),
      printRuleMatch: preview.rulesCss.some((r) => r.includes("eo-print-sheet") && r.includes("20pt") && r.includes("Arial")),
      previewFontSize,
      previewFontFamily,
    });
  })()`).then(JSON.parse);

  assert.ok(Math.abs(openedLarge.td1FontSize - (20 * 96) / 72) < 0.1, `20pt defaultFont renders at ~26.67px, got ${openedLarge.td1FontSize}`);
  assert.ok(Math.abs(openedLarge.td1EditMeasure - (20 * 96) / 72) < 0.1, "edit measurement box uses active defaultFont size");
  assert.match(openedLarge.td1FontFamily, /Arial/);
  assert.equal(openedLarge.row1H, 35, "large 20pt defaultFont auto-expands row height from 22px to 35px");
  assert.equal(openedLarge.td2LineH, 36, "wrapped cell uses 36px line-height at 20pt");
  assert.equal(openedLarge.printRuleMatch, true);
  assert.ok(Math.abs(openedLarge.previewFontSize - (20 * 96) / 72) < 0.1, `print table inherits 20pt (~26.67px), got ${openedLarge.previewFontSize}`);
  assert.match(openedLarge.previewFontFamily, /Arial/);

  // Undo restores the prior standard 11pt workbook, 22px row height, and CSS custom properties.
  await js(`document.querySelector('[data-studio-menu-cmd="studio-undo"]').click()`);
  await sleep(200);
  const afterUndo = await js(`JSON.stringify((() => {
    const td1 = document.querySelector('#pane-sheet td[data-cell="A1"]');
    return {
      td1FontSize: parseFloat(getComputedStyle(td1).fontSize),
      row1H: td1.closest("tr").getBoundingClientRect().height,
      cssVarSize: document.documentElement.style.getPropertyValue("--sheet-cell-font-size"),
    };
  })())`).then(JSON.parse);
  assert.ok(Math.abs(afterUndo.td1FontSize - (11 * 96) / 72) < 0.1);
  assert.equal(afterUndo.row1H, 22);
  assert.equal(afterUndo.cssVarSize, "");
});

test("defaultFont editing syncs rows, keeps empty/single-wrap/manual rows aligned, floats tall edit box, wraps preview, resets multi-sheet cache, and fits columns in real Chrome (PD-348)", { skip }, async () => {
  async function openXlsxFixture(title, sheets, defaultFont) {
    await js(`(async () => {
      const resp = await fetch("/api/ipc", {
        method: "POST",
        headers: { "Content-Type": "application/json", "X-EasyOffice-Ipc": "1" },
        body: JSON.stringify({
          command: "convert_document_format",
          payload: JSON.stringify({
            sourceFormat: "xlsx",
            targetFormat: "xlsx",
            title: ${JSON.stringify(title)},
            forSave: true,
            sheets: ${JSON.stringify(sheets)},
            ...( ${JSON.stringify(defaultFont || null)} ? { defaultFont: ${JSON.stringify(defaultFont || null)} } : {}),
          }),
        }),
      });
      const res = await resp.json();
      if (!res.convertedBase64) throw new Error("convert failed: " + JSON.stringify(res));
      const bin = Uint8Array.from(atob(res.convertedBase64), (c) => c.charCodeAt(0));
      const origConfirm = window.confirm;
      window.confirm = () => true;
      try {
        await window.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes(${JSON.stringify(title + ".xlsx")}, bin);
      } finally {
        window.confirm = origConfirm;
      }
    })()`);
    await sleep(200);
  }

  // 1) Multi-sheet 20pt workbook: row 1 populated (35px), row 2 long unbroken wrap, row 3 1-line wrap (35px), row 4 empty (22px), C1 wide string.
  await openXlsxFixture("multi-20pt", [
    {
      name: "Sheet1",
      cells: [
        { ref: "A1", raw: "큰글꼴" },
        { ref: "A2", raw: "WordWithoutSpacesForOverflowWrapAnywhereInPrintPreview", style: { wrap: true } },
        { ref: "A3", raw: "Hi", style: { wrap: true } },
        { ref: "C5", raw: "MMMMMMMMMM" },
      ],
    },
    { name: "Sheet2", cells: [{ ref: "B1", raw: "둘째시트" }] },
  ], { name: "Arial", size: 20 });

  const initLayout = await js(`JSON.stringify({
    r1: document.querySelector('#pane-sheet td[data-cell="A1"]').closest("tr").getBoundingClientRect().height,
    r3: document.querySelector('#pane-sheet td[data-cell="A3"]').closest("tr").getBoundingClientRect().height,
    r3Style: document.querySelector('#pane-sheet td[data-cell="A3"]').closest("tr").style.height,
    r4: document.querySelector('#pane-sheet td[data-cell="A4"]').closest("tr").getBoundingClientRect().height,
    r4Style: document.querySelector('#pane-sheet td[data-cell="A4"]').closest("tr").style.height,
  })`).then(JSON.parse);
  assert.equal(initLayout.r1, 35);
  assert.equal(initLayout.r3, 35, "1-line wrapped cell at 20pt stays capped at 35px without 43px overflow");
  assert.equal(initLayout.r3Style, "35px");
  assert.equal(initLayout.r4, 22, "empty rows stay aligned at 22px without 34px CSS drift");
  assert.equal(initLayout.r4Style, "");

  // 2) Column auto-fit via dblclick on .col-resizer expands column C for 20pt text.
  await js(`document.querySelector('#pane-sheet thead th[data-col="C"] .col-resizer').dispatchEvent(new MouseEvent("dblclick", { bubbles: true }))`);
  await sleep(250);
  const colCFit = await js(`JSON.stringify((() => {
    const th = document.querySelector('#pane-sheet thead th[data-col="C"]');
    const c5 = document.querySelector('#pane-sheet td[data-cell="C5"]');
    return { width: th.getBoundingClientRect().width, scrollW: c5.scrollWidth, clientW: c5.clientWidth };
  })())`).then(JSON.parse);
  assert.ok(colCFit.width > 180, `20pt MMMMMMMMMM col auto-fit expected > 180px, got ${colCFit.width}`);
  assert.ok(colCFit.scrollW <= colCFit.clientW + 1, "col auto-fit leaves no horizontal clip");

  // 3) Docked Print Preview (#print-preview-pane) wraps unbroken words with overflow-wrap: anywhere.
  await js(`window.__EASYOFFICE_PRINT_PREVIEW__.open()`);
  await sleep(400);
  const previewWrap = await js(`JSON.stringify((() => {
    const cells = Array.from(document.querySelectorAll('#print-preview-pane .eo-print-sheet td:not(.eo-print-head)'));
    const target = cells.find((c) => c.textContent.includes("WordWithoutSpaces"));
    if (!target) return null;
    const cs = getComputedStyle(target);
    return { ws: cs.whiteSpace, ovWrap: cs.overflowWrap, scrollW: target.scrollWidth, clientW: target.clientWidth };
  })())`).then(JSON.parse);
  assert.ok(previewWrap, "wrapped target cell found in print preview");
  assert.equal(previewWrap.ws, "pre-wrap");
  assert.equal(previewWrap.ovWrap, "anywhere");
  assert.ok(previewWrap.scrollW <= previewWrap.clientW + 1, `print preview unbroken word must wrap inside column (${previewWrap.scrollW} <= ${previewWrap.clientW})`);
  await js(`window.__EASYOFFICE_PRINT_PREVIEW__.close()`);
  await sleep(100);

  // 4) Typing into empty 22px row A4 under 20pt floats .edit-grow vertically without shifting row 5, then Enter syncs row 4 to 35px.
  const a4Rect = await rectOf(td("A4"));
  const a5TopBefore = (await rectOf(td("A5"))).top;
  await mouse("mouseMoved", a4Rect.left + a4Rect.width / 2, a4Rect.top + a4Rect.height / 2);
  await mouse("mousePressed", a4Rect.left + a4Rect.width / 2, a4Rect.top + a4Rect.height / 2);
  await mouse("mouseReleased", a4Rect.left + a4Rect.width / 2, a4Rect.top + a4Rect.height / 2);
  await text("Hi");
  await sleep(100);
  const whileTypingA4 = await js(`JSON.stringify((() => {
    const el = document.querySelector('#pane-sheet td[data-cell="A4"]');
    return {
      grow: el.classList.contains("edit-grow"),
      eh: el.style.getPropertyValue("--eh"),
      ch: el.style.getPropertyValue("--ch"),
      offsetH: el.offsetHeight,
      scrollH: el.scrollHeight,
      clientH: el.clientHeight,
      trH: el.closest("tr").getBoundingClientRect().height,
      a5Top: document.querySelector('#pane-sheet td[data-cell="A5"]').getBoundingClientRect().top,
    };
  })())`).then(JSON.parse);
  assert.equal(whileTypingA4.grow, true);
  assert.equal(whileTypingA4.eh, "34px");
  assert.equal(whileTypingA4.ch, "22px");
  assert.equal(whileTypingA4.offsetH, 34);
  assert.equal(whileTypingA4.scrollH, whileTypingA4.clientH);
  assert.equal(whileTypingA4.trH, 22);
  assert.equal(whileTypingA4.a5Top, a5TopBefore);

  await key("Enter");
  await sleep(200);
  assert.equal(await rowHeight("A4"), 35);
  assert.equal(await js(`document.querySelector('#pane-sheet td[data-cell="A4"]').closest("tr").style.height`), "35px");

  // 5) Clearing populated A1 via F2 + Backspace + Enter shrinks row 1 back to 22px.
  await click("A1");
  await key("F2");
  for (let i = 0; i < 4; i += 1) await key("Backspace");
  await key("Enter");
  await sleep(200);
  assert.equal(await rowHeight("A1"), 22);
  assert.equal(await js(`document.querySelector('#pane-sheet td[data-cell="A1"]').closest("tr").style.height`), "");

  // 6) Dragging row 3 (35px) down to 23px renders at 23px (not stretched back to 34px).
  const rz3 = await rectOf('#pane-sheet tbody tr[data-row="3"] th .row-resizer');
  await mouse("mouseMoved", rz3.left + rz3.width / 2, rz3.top + rz3.height / 2);
  await mouse("mousePressed", rz3.left + rz3.width / 2, rz3.top + rz3.height / 2);
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: rz3.left + rz3.width / 2, y: rz3.top + rz3.height / 2 - 12, button: "left", buttons: 1 });
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: rz3.left + rz3.width / 2, y: rz3.top + rz3.height / 2 - 12, button: "left", buttons: 0, clickCount: 1 });
  await sleep(200);
  const manualR3 = await js(`JSON.stringify((() => {
    const tr = document.querySelector('#pane-sheet tbody tr[data-row="3"]');
    return { styleH: tr.style.height, rectH: tr.getBoundingClientRect().height, cap: tr.classList.contains("row-cap") };
  })())`).then(JSON.parse);
  assert.equal(manualR3.styleH, "23px");
  assert.equal(manualR3.rectH, 23);
  assert.equal(manualR3.cap, true);

  // 7) Visit Sheet2 (35px), switch back to Sheet1, then open an 11pt workbook with empty Sheet1 and Sheet2 B1: Sheet2 must reset to 22px.
  const tab2 = await rectOf('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet2"]');
  await mouse("mousePressed", tab2.left + tab2.width / 2, tab2.top + tab2.height / 2);
  await mouse("mouseReleased", tab2.left + tab2.width / 2, tab2.top + tab2.height / 2);
  await sleep(200);
  assert.equal(await rowHeight("B1"), 35);

  const tab1 = await rectOf('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet1"]');
  await mouse("mousePressed", tab1.left + tab1.width / 2, tab1.top + tab1.height / 2);
  await mouse("mouseReleased", tab1.left + tab1.width / 2, tab1.top + tab1.height / 2);
  await sleep(150);

  await openXlsxFixture("multi-11pt", [
    { name: "Sheet1", cells: [] },
    { name: "Sheet2", cells: [{ ref: "B1", raw: "표준" }] },
  ], null);

  const tab2Std = await rectOf('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet2"]');
  await mouse("mousePressed", tab2Std.left + tab2Std.width / 2, tab2Std.top + tab2Std.height / 2);
  await mouse("mouseReleased", tab2Std.left + tab2Std.width / 2, tab2Std.top + tab2Std.height / 2);
  await sleep(200);
  assert.equal(await rowHeight("B1"), 22);
  assert.equal(await js(`document.querySelector('#pane-sheet td[data-cell="B1"]').closest("tr").style.height`), "");
});
