import test, { after, before } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { REPO, sleep, startBrowser, startEngine, startUiServer } from "./bench-webui-support.mjs";
import { findChrome } from "./hwp-studio-harness.mjs";

const exe = process.platform === "win32" ? "easyoffice-app.exe" : "easyoffice-app";
const APP = [process.env.EO_APP, ...["target-193", "target"].map((dir) => path.join(REPO, dir, "debug", exe))].find((file) => file && fs.existsSync(file));
let CHROME = null;
try {
  CHROME = findChrome();
} catch {
  CHROME = null;
}
const skip = !APP || !CHROME ? "needs easyoffice-app and Chrome (EO_APP, CHROME_PATH)" : false;

const CTRL = 2;
const META = 4;
const SHIFT = 8;
const MULTI_MOD = process.platform === "darwin" ? META : CTRL;
let ctx = null;

before(async () => {
  if (skip) return;
  const port = 22000 + Math.floor(Math.random() * 18000);
  const engine = startEngine(APP, port);
  await sleep(800);
  const server = await startUiServer({ enginePort: port, port: port + 1 });
  const browser = await startBrowser({ chrome: CHROME, debugPort: port + 2, url: `http://127.0.0.1:${port + 1}/?module=SHEET&win=slice4-chrome`, width: 1300, height: 760 });
  ctx = { engine, server, browser, cdp: browser.cdp };
  await sleep(1000);
});

after(async () => {
  if (!ctx) return;
  await ctx.browser.stop();
  ctx.server.close();
  ctx.engine.kill();
});

const js = (expr) => ctx.cdp.evalJs(expr);
const td = (ref) => `#pane-sheet td[data-cell="${ref}"]`;
const cellText = (ref) => js(`(document.querySelector(${JSON.stringify(td(ref))}) || {}).textContent`);
const rectOf = (sel) => js(`(() => { const el = document.querySelector(${JSON.stringify(sel)}); if (!el) return null; const r = el.getBoundingClientRect(); return { left: r.left, top: r.top, width: r.width, height: r.height }; })()`);

async function sendKey(keyName, code, vk, modifiers = 0) {
  const base = { key: keyName, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers };
  await ctx.cdp.send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
  await ctx.cdp.send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
}

async function mouseClick(sel, button = "left", modifiers = 0) {
  const r = await rectOf(sel);
  assert.ok(r, `element visible for click: ${sel}`);
  const x = r.left + r.width / 2;
  const y = r.top + r.height / 2;
  const buttons = button === "right" ? 2 : 1;
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y, modifiers });
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button, buttons, clickCount: 1, modifiers });
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button, buttons: 0, clickCount: 1, modifiers });
}

async function put(ref, value) {
  await mouseClick(td(ref));
  await ctx.cdp.send("Input.insertText", { text: value });
  await sendKey("Enter", "Enter", 13);
  await sleep(80);
}

test("real Chrome: shortcuts, toolbar Σ, multi-range status aggregate, right-click menus, row height, data tools, and format cells", { skip }, async () => {
  await put("A1", "10");
  await put("A2", "20");

  // Toolbar Σ AutoSum button on A3
  await mouseClick(td("A3"));
  await sleep(50);
  await mouseClick("#btn-sheet-autosum");
  await sleep(200);
  assert.equal(await cellText("A3"), "30");

  // Ctrl+; inserts local YYYY-MM-DD and Ctrl+D copies A1 into B1? (C1->C2)
  await put("C1", "값");
  await mouseClick(td("C2"));
  await sendKey("d", "KeyD", 68, CTRL);
  await sleep(160);
  assert.equal(await cellText("C2"), "값");

  await mouseClick(td("D1"));
  await sendKey(";", "Semicolon", 186, CTRL);
  await sleep(160);
  assert.match(await cellText("D1"), /^\d{4}-\d{2}-\d{2}$/);

  // Non-contiguous Ctrl/Cmd+click across A1, A2 (with duplicate guard), and A3 -> deduplicated status aggregate
  await mouseClick(td("A1"));
  await mouseClick(td("A2"), "left", MULTI_MOD);
  await mouseClick(td("A2"), "left", MULTI_MOD);
  await mouseClick(td("A3"), "left", MULTI_MOD);
  await sleep(100);
  const summary = await js(`document.getElementById("status-sheet-agg").textContent`);
  assert.match(summary, /합계: 60/);
  assert.match(summary, /숫자 개수: 3/);

  // Right-click general cell opens #sheet-cell-menu; Escape closes it
  await mouseClick(td("B2"), "right");
  await sleep(60);
  assert.equal(await js(`document.getElementById("sheet-cell-menu").hidden`), false);
  await sendKey("Escape", "Escape", 27);
  await sleep(60);
  assert.equal(await js(`document.getElementById("sheet-cell-menu").hidden`), true);

  // Right-click B2 -> 셀 서식… sets font, size, and 45deg rotation
  await mouseClick(td("B2"), "right");
  await sleep(60);
  await mouseClick('#sheet-cell-menu [data-sheet-ctx="cell-format"]');
  await sleep(120);
  assert.equal(await js(`document.getElementById("sheet-numfmt-dialog").open`), true);
  await js(`(() => {
    const ff = document.getElementById("sheet-font-family-select");
    ff.value = "나눔고딕";
    ff.dispatchEvent(new Event("change", { bubbles: true }));
    const fs = document.getElementById("sheet-font-size-select");
    fs.value = "16";
    fs.dispatchEvent(new Event("change", { bubbles: true }));
    const rot = document.getElementById("sheet-align-rotation");
    rot.value = "45";
    rot.dispatchEvent(new Event("input", { bubbles: true }));
  })()`);
  await mouseClick("#btn-sheet-numfmt-apply");
  await sleep(200);
  assert.equal(await js(`document.getElementById("sheet-numfmt-dialog").open`), false);
  assert.equal(await js(`document.querySelector('#pane-sheet td[data-cell="B2"]').dataset.rot`), "45");
  assert.equal(await js(`document.querySelector('#pane-sheet td[data-cell="B2"]').style.fontFamily`), "나눔고딕");

  // Right-click row header -> 행 높이… sets height in 1 undo step
  await mouseClick('#pane-sheet th[data-row="2"]', "right");
  await sleep(60);
  await mouseClick('#sheet-header-menu [data-sheet-act="row-height"]');
  await sleep(80);
  assert.equal(await js(`document.getElementById("sheet-size-dialog").open`), true);
  await js(`document.getElementById("sheet-size-input").value = "32"`);
  await mouseClick("#btn-sheet-size-ok");
  await sleep(160);
  const h2 = await rectOf(td("B2"));
  assert.ok(h2.height >= 40, `row 2 height scaled from 32pt: ${h2.height}`);

  // Text-to-Columns on F1 ("k,m,9")
  await put("F1", "k,m,9");
  await mouseClick(td("F1"));
  await js(`document.querySelector('#menu-bar .md-item[data-sheet-act="text-to-columns"]').click()`);
  await sleep(100);
  assert.equal(await js(`document.getElementById("sheet-text-columns-dialog").open`), true);
  await js(`(() => { const c = document.getElementById("sheet-tc-comma"); c.checked = true; c.dispatchEvent(new Event("change", { bubbles: true })); })()`);
  await mouseClick("#btn-sheet-tc-apply");
  await sleep(180);
  assert.equal(await cellText("F1"), "k");
  assert.equal(await cellText("G1"), "m");
  assert.equal(await cellText("H1"), "9");

  // General range Remove Duplicates on I1:I4
  await put("I1", "항목");
  await put("I2", "하나");
  await put("I3", "하나");
  await put("I4", "둘");
  await mouseClick(td("I2"));
  await js(`document.querySelector('#menu-bar .md-item[data-sheet-act="dedupe-dialog"]').click()`);
  await sleep(100);
  assert.equal(await js(`document.getElementById("sheet-table-dedupe-dialog").open`), true);
  await mouseClick("#btn-sheet-dedupe-ok");
  await sleep(180);
  assert.equal(await cellText("I3"), "둘");
  assert.equal(await cellText("I4"), "");

  // Ctrl+Space, Shift+Space, and Ctrl+Shift+L in real Chrome
  await mouseClick(td("I2"));
  await sendKey(" ", "Space", 32, CTRL);
  await sleep(60);
  assert.equal(await js(`document.querySelector('#pane-sheet td[data-cell="I1"]').classList.contains("in-range")`), true);
  await mouseClick(td("I2"));
  await sendKey(" ", "Space", 32, SHIFT);
  await sleep(60);
  assert.equal(await js(`document.querySelector('#pane-sheet td[data-cell="A2"]').classList.contains("in-range")`), true);
  await mouseClick(td("I1"));
  await sendKey("L", "KeyL", 76, CTRL | SHIFT);
  await sleep(180);
  assert.equal(await js(`document.querySelector('#pane-sheet td[data-cell="I1"]').classList.contains("filter-head")`), true);
});
