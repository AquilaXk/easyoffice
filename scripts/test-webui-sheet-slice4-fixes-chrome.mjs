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
const skip = !APP || !CHROME ? "needs a built easyoffice-app and a Chrome (EO_APP, CHROME_PATH)" : false;

const KEYS = {
  Enter: [13, "Enter"],
  Escape: [27, "Escape"],
  F2: [113, "F2"],
  Digit1: [49, "Digit1"],
  Digit5: [53, "Digit5"],
  KeyZ: [90, "KeyZ"],
};
const CTRL = 2;
const META = 4;
const CLICK_MOD = process.platform === "darwin" ? META : CTRL;

let ctx = null;

before(async () => {
  if (skip) return;
  const port = 22000 + Math.floor(Math.random() * 18000);
  const engine = startEngine(APP, port);
  await sleep(800);
  const server = await startUiServer({ enginePort: port, port: port + 1 });
  const browser = await startBrowser({
    chrome: CHROME,
    debugPort: port + 2,
    url: `http://127.0.0.1:${port + 1}/?module=SHEET&win=slice4-fixes-chrome`,
    width: 1100,
    height: 700,
  });
  ctx = { engine, server, browser, cdp: browser.cdp };
  await sleep(1000);
});

after(async () => {
  if (!ctx) return;
  await ctx.browser.stop();
  ctx.server.close();
  ctx.engine.kill();
});

const js = (expression) => ctx.cdp.evalJs(expression);

async function key(name, modifiers = 0) {
  const [vk, code] = KEYS[name];
  const base = { key: name.length === 6 && name.startsWith("Digit") ? name[5] : name.startsWith("Key") ? name[3].toLowerCase() : name, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers };
  await ctx.cdp.send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
  await ctx.cdp.send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
}

async function mouseClick(selector, modifiers = 0) {
  const box = await js(`(() => {
    const el = document.querySelector(${JSON.stringify(selector)});
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  })()`);
  assert.ok(box, `element must exist: ${selector}`);
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x: box.x, y: box.y, button: "left", clickCount: 1, modifiers });
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: box.x, y: box.y, button: "left", clickCount: 1, modifiers });
}

async function typeCell(ref, value) {
  await mouseClick(`#pane-sheet td[data-cell="${ref}"]`);
  await key("F2");
  await ctx.cdp.send("Input.insertText", { text: value });
  await key("Enter");
  await sleep(150);
}

test("real Chrome: multi-range autofitRows, column width dialog + autofit, Ctrl+Enter via formula bar, multi-range Ctrl+5 undo, multi-range copy refusal, and rotated Esc cancel", { skip }, async () => {
  // 1. Multi-range autofitRows: A1 short, A4 multi-line wrapped; Ctrl+click A4 and run row-autofit.
  await typeCell("A1", "ok");
  await typeCell("A4", "line1 line2 line3 line4 line5 line6 line7 line8");
  await mouseClick('#pane-sheet td[data-cell="A4"]');
  await js(`document.querySelector('#menu-bar .md-item[data-sheet-look="wrap"]').click()`);
  await sleep(180);

  // Shrink row 4 manually via row-height dialog, then verify multi-range autofitRows expands row 4.
  await mouseClick('#pane-sheet td[data-cell="A1"]');
  await mouseClick('#pane-sheet td[data-cell="A4"]', CLICK_MOD);
  await js(`document.querySelector('#menu-bar .md-item[data-sheet-act="row-height"]').click()`);
  await sleep(150);
  await js(`(() => { const i = document.getElementById("sheet-size-input"); i.value = "16.5"; i.dispatchEvent(new Event("input", { bubbles: true })); })()`);
  await mouseClick("#btn-sheet-size-ok");
  await sleep(200);
  const row4Before = await js(`document.querySelector('#pane-sheet td[data-cell="A4"]').getBoundingClientRect().height`);
  assert.ok(row4Before <= 30, `manual row height 16.5pt should clamp row 4 height <= 30px, got ${row4Before}`);

  await mouseClick('#pane-sheet td[data-cell="A1"]');
  await mouseClick('#pane-sheet td[data-cell="A4"]', CLICK_MOD);
  await js(`document.querySelector('#menu-bar .md-item[data-sheet-act="row-autofit"]').click()`);
  await sleep(220);
  const row4After = await js(`document.querySelector('#pane-sheet td[data-cell="A4"]').getBoundingClientRect().height`);
  assert.ok(row4After > row4Before * 2, `multi-range autofit must expand row 4 (${row4Before} -> ${row4After})`);

  // 2. Column width numeric dialog and autofit on col B.
  await typeCell("B1", "WIDE-HEADER-TEXT-VALUE");
  await mouseClick('#pane-sheet td[data-cell="B1"]');
  await js(`document.querySelector('#menu-bar .md-item[data-sheet-act="col-width"]').click()`);
  await sleep(150);
  await js(`(() => { const i = document.getElementById("sheet-size-input"); i.value = "20"; i.dispatchEvent(new Event("input", { bubbles: true })); })()`);
  await mouseClick("#btn-sheet-size-ok");
  await sleep(180);
  const colBSet = await js(`document.querySelector('#pane-sheet th[data-col="B"]').getBoundingClientRect().width`);
  assert.ok(colBSet >= 150, `col B width 20 chars should be >= 150px, got ${colBSet}`);

  await js(`document.querySelector('#menu-bar .md-item[data-sheet-act="col-autofit"]').click()`);
  await sleep(200);
  const colBFit = await js(`document.querySelector('#pane-sheet th[data-col="B"]').getBoundingClientRect().width`);
  assert.ok(colBFit >= 100, `col B autofit width should fit header text, got ${colBFit}`);

  // 3. Ctrl+5 strikethrough across multi-range A1 + C3 and single-step Ctrl+Z undo.
  await typeCell("C3", "300");
  await mouseClick('#pane-sheet td[data-cell="A1"]');
  await mouseClick('#pane-sheet td[data-cell="C3"]', CLICK_MOD);
  await key("Digit5", CTRL);
  await sleep(200);
  const strikes = await js(`[document.querySelector('#pane-sheet td[data-cell="A1"]').style.textDecorationLine, document.querySelector('#pane-sheet td[data-cell="C3"]').style.textDecorationLine]`);
  assert.deepEqual(strikes, ["line-through", "line-through"]);
  await key("KeyZ", CTRL);
  await sleep(200);
  const afterUndo = await js(`[document.querySelector('#pane-sheet td[data-cell="A1"]').style.textDecorationLine, document.querySelector('#pane-sheet td[data-cell="C3"]').style.textDecorationLine]`);
  assert.deepEqual(afterUndo, ["", ""]);

  // 4. Ctrl+Enter via formula bar fills both A2 and C2 in 1 undo step.
  await mouseClick('#pane-sheet td[data-cell="A2"]');
  await mouseClick('#pane-sheet td[data-cell="C2"]', CLICK_MOD);
  await mouseClick("#formula-input");
  await ctx.cdp.send("Input.insertText", { text: "BATCH9" });
  await key("Enter", CTRL);
  await sleep(220);
  const batchVals = await js(`[document.querySelector('#pane-sheet td[data-cell="A2"]').textContent, document.querySelector('#pane-sheet td[data-cell="C2"]').textContent]`);
  assert.deepEqual(batchVals, ["BATCH9", "BATCH9"]);

  // 5. Multi-range copy refusal message via Ctrl+C.
  await mouseClick('#pane-sheet td[data-cell="A2"]');
  await mouseClick('#pane-sheet td[data-cell="C2"]', CLICK_MOD);
  await js(`(() => { const td = document.querySelector('#pane-sheet td[data-cell="C2"]'); td.dispatchEvent(new ClipboardEvent("copy", { bubbles: true, cancelable: true })); })()`);
  await sleep(120);
  const copyFeedback = await js(`document.getElementById("status-command-feedback").textContent`);
  assert.match(copyFeedback, /여러 개의 선택 영역/);

  // 6. Rotated cell 45 deg -> F2 -> Esc preserves .cell-rot in real Chrome.
  await typeCell("D1", "ROT45");
  await mouseClick('#pane-sheet td[data-cell="D1"]');
  await key("Digit1", CTRL);
  await sleep(180);
  await mouseClick("#sheet-fmt-tab-align");
  await sleep(80);
  await mouseClick('[data-rot-preset="45"]');
  await mouseClick("#btn-sheet-numfmt-apply");
  await sleep(220);
  assert.equal(await js(`Boolean(document.querySelector('#pane-sheet td[data-cell="D1"] .cell-rot'))`), true);
  await key("F2");
  await sleep(120);
  await key("Escape");
  await sleep(160);
  assert.equal(await js(`Boolean(document.querySelector('#pane-sheet td[data-cell="D1"] .cell-rot'))`), true);
});
