import test, { after, before } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { REPO, sleep, startBrowser, startEngine, startUiServer } from "./bench-webui-support.mjs";
import { findChrome } from "./hwp-studio-harness.mjs";

// Real headless Chromium CDP suite for modal keyboard capture->bubble routing, color-picker / custom-select
// Escape handling, window.getComputedStyle focus exclusion, nested modals, Space/Enter on controls, BOARD zoom,
// and DOC table-sort / shape / stub guards.

const exe = process.platform === "win32" ? "easyoffice-app.exe" : "easyoffice-app";
const APP = [
  process.env.EO_APP,
  ...["target-193", "target"].map((dir) => path.join(REPO, dir, "debug", exe)),
  path.resolve(REPO, "..", "target-193", "debug", exe),
].find((file) => file && fs.existsSync(file));
let CHROME = null;
try {
  CHROME = findChrome();
} catch {
  CHROME = null;
}
const skip = !APP || !CHROME ? "needs a built easyoffice-app and a Chrome (EO_APP, CHROME_PATH)" : false;

const KEYS = {
  Enter: [13, "Enter", "\r"],
  Tab: [9, "Tab", ""],
  Escape: [27, "Escape", ""],
  " ": [32, "Space", " "],
  b: [66, "KeyB", "b"],
  ArrowDown: [40, "ArrowDown", ""],
};
const CTRL = 2;
const SHIFT = 8;

let ctx = null;
let baseUrl = "";

before(async () => {
  if (skip) return;
  const port = 20000 + Math.floor(Math.random() * 20000);
  let engine = null;
  let server = null;
  let browser = null;
  try {
    engine = startEngine(APP, port);
    await sleep(800);
    server = await startUiServer({ enginePort: port, port: port + 1 });
    baseUrl = `http://127.0.0.1:${port + 1}`;
    browser = await startBrowser({
      chrome: CHROME,
      debugPort: port + 2,
      url: `${baseUrl}/?module=DOC&win=modals-real-chrome`,
      width: 1280,
      height: 860,
    });
    ctx = { engine, server, browser, cdp: browser.cdp };
    await sleep(800);
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

const js = (expression) => ctx.cdp.evalJs(expression);
const text = (value) => ctx.cdp.send("Input.insertText", { text: value });
const mouse = (type, x, y, clickCount = 1) =>
  ctx.cdp.send("Input.dispatchMouseEvent", { type, x, y, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount });

async function key(name, modifiers = 0) {
  const [vk, code, ch] = KEYS[name];
  const base = { key: name, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers };
  if (ch && modifiers === 0) {
    await ctx.cdp.send("Input.dispatchKeyEvent", { type: "keyDown", ...base, text: ch, unmodifiedText: ch });
  } else {
    await ctx.cdp.send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
  }
  await ctx.cdp.send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
}

async function clickCenter(selector) {
  const rect = await js(`(() => {
    const el = document.querySelector(${JSON.stringify(selector)});
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 ? { x: r.left + r.width / 2, y: r.top + r.height / 2 } : null;
  })()`);
  assert.ok(rect, `visible element for ${selector}`);
  await mouse("mouseMoved", rect.x, rect.y);
  await mouse("mousePressed", rect.x, rect.y);
  await mouse("mouseReleased", rect.x, rect.y);
  await sleep(80);
}

async function navigateModule(mod) {
  await ctx.cdp.send("Page.navigate", { url: `${baseUrl}/?module=${mod}&win=modals-real-${mod.toLowerCase()}` });
  await sleep(900);
  const ready = await js(`document.getElementById("app")?.dataset?.module`);
  assert.equal(ready, mod, `navigated to module ${mod}`);
}

test("DOC #doc-shape-dialog hides color inputs, keeps modal open on color-picker/custom-select Escape, and inserts shape", { skip }, async () => {
  await navigateModule("DOC");
  await js(`document.querySelector('.md-item[data-studio-menu-cmd="insert-shape"]').click()`);
  await sleep(100);
  assert.equal(await js(`document.getElementById("doc-shape-dialog").open`), true);

  const colorState = JSON.parse(await js(`JSON.stringify((() => {
    const fill = document.getElementById("doc-shape-fill");
    const stroke = document.getElementById("doc-shape-stroke");
    const dlg = document.getElementById("doc-shape-dialog");
    const swatch = document.querySelector("#doc-shape-fill-host [data-color-toggle]").getBoundingClientRect();
    return {
      fillOpacity: getComputedStyle(fill).opacity,
      strokeOpacity: getComputedStyle(stroke).opacity,
      fillHidden: fill.getAttribute("aria-hidden"),
      fillTab: fill.tabIndex,
      swatchW: swatch.width,
      swatchH: swatch.height,
      noHOverflow: dlg.scrollWidth <= dlg.clientWidth,
    };
  })())`));
  assert.equal(colorState.fillOpacity, "0");
  assert.equal(colorState.strokeOpacity, "0");
  assert.equal(colorState.fillHidden, "true");
  assert.equal(colorState.fillTab, -1);
  assert.ok(colorState.swatchW > 12 && colorState.swatchH > 12, "swatch button is visible");
  assert.equal(colorState.noHOverflow, true, "#doc-shape-dialog does not overflow horizontally");

  // Open .sb-hl-palette inside #doc-shape-dialog -> real Chrome Escape closes ONLY palette, not modal!
  await clickCenter("#doc-shape-fill-host [data-color-toggle]");
  assert.equal(await js(`document.getElementById("doc-shape-fill-palette").hidden`), false);
  await key("Escape");
  await sleep(80);
  assert.equal(await js(`document.getElementById("doc-shape-fill-palette").hidden`), true, "Escape closes color palette");
  assert.equal(await js(`document.getElementById("doc-shape-dialog").open`), true, "dialog stays open after palette Escape");
  assert.equal(
    await js(`document.activeElement === document.querySelector("#doc-shape-fill-host [data-color-toggle]")`),
    true,
    "focus returns to swatch toggle button"
  );

  // ArrowDown on swatch toggle opens palette and focuses first swatch; Escape closes it cleanly
  await key("ArrowDown");
  await sleep(80);
  assert.equal(await js(`document.getElementById("doc-shape-fill-palette").hidden`), false);
  assert.equal(
    await js(`document.activeElement?.closest?.("#doc-shape-fill-palette") !== null`),
    true,
    "ArrowDown moves focus into palette"
  );
  await key("Escape");
  await sleep(80);
  assert.equal(await js(`document.getElementById("doc-shape-fill-palette").hidden`), true);
  assert.equal(await js(`document.getElementById("doc-shape-dialog").open`), true);

  // Pick a swatch from the popover palette
  await clickCenter("#doc-shape-fill-host [data-color-toggle]");
  await clickCenter('#doc-shape-fill-palette .sb-hl-swatch[title="#ff0000"]');
  assert.equal(await js(`document.getElementById("doc-shape-fill-palette").hidden`), true);
  assert.equal(await js(`document.getElementById("doc-shape-fill").value`), "#ff0000");
  assert.equal(
    await js(`document.querySelector("#doc-shape-fill-host .dialog-swatch-chip").style.getPropertyValue("--dialog-swatch")`),
    "#ff0000"
  );

  // Focus .custom-select-trigger -> Space opens .custom-select.open -> Escape closes ONLY select, not modal!
  await js(`document.querySelector('#doc-shape-dialog [data-select-id="doc-shape-type"] .custom-select-trigger').focus()`);
  await key(" ");
  await sleep(80);
  const selectOpenInfo = JSON.parse(await js(`JSON.stringify((() => {
    const dlg = document.getElementById("doc-shape-dialog");
    const root = dlg.querySelector('[data-select-id="doc-shape-type"]');
    const menu = root.querySelector(".custom-select-menu");
    const r = menu.getBoundingClientRect();
    return {
      open: root.classList.contains("open"),
      menuH: r.height,
      dlgOverflow: getComputedStyle(dlg).overflowY,
    };
  })())`));
  assert.equal(selectOpenInfo.open, true);
  assert.ok(selectOpenInfo.menuH > 40, "custom-select-menu renders unclipped options");
  assert.equal(selectOpenInfo.dlgOverflow, "visible");
  await key("Escape");
  await sleep(80);
  assert.equal(
    await js(`document.querySelector('#doc-shape-dialog [data-select-id="doc-shape-type"]').classList.contains("open")`),
    false
  );
  assert.equal(await js(`document.getElementById("doc-shape-dialog").open`), true, "dialog stays open after custom-select Escape");

  // Mouse-opened .custom-select keeps focus on text input -> Escape STILL closes .custom-select and keeps dialog open!
  await js(`document.getElementById("doc-shape-text").focus()`);
  await clickCenter('#doc-shape-dialog [data-select-id="doc-shape-type"] .custom-select-trigger');
  assert.equal(
    await js(`document.querySelector('#doc-shape-dialog [data-select-id="doc-shape-type"]').classList.contains("open")`),
    true
  );
  await key("Escape");
  await sleep(80);
  assert.equal(
    await js(`document.querySelector('#doc-shape-dialog [data-select-id="doc-shape-type"]').classList.contains("open")`),
    false,
    "Escape closes mouse-opened .custom-select even while focus was on text input"
  );
  assert.equal(await js(`document.getElementById("doc-shape-dialog").open`), true, "dialog stays open after mouse-opened custom-select Escape");

  // Tabbing away from an open .custom-select closes it via focusin
  await key(" ");
  await sleep(80);
  assert.equal(
    await js(`document.querySelector('#doc-shape-dialog [data-select-id="doc-shape-type"]').classList.contains("open")`),
    true
  );
  await key("Tab");
  await sleep(80);
  assert.equal(
    await js(`document.querySelector('#doc-shape-dialog [data-select-id="doc-shape-type"]').classList.contains("open")`),
    false,
    "Tab moving focus away closes open .custom-select"
  );

  await js(`(() => { const el = document.getElementById("doc-shape-text"); el.focus(); el.select(); })()`);
  await text("실브라우저 도형");
  await clickCenter("#btn-doc-shape-ok");
  assert.equal(await js(`document.getElementById("doc-shape-dialog").open`), false);
  assert.equal(await js(`document.querySelector("#doc-editor .eo-doc-shape svg")?.firstChild?.getAttribute("fill")`), "#ff0000");

  // Submitting modal via Enter while nested .sb-hl-palette is open closes palette and does not block next modal's Escape
  await js(`document.querySelector('.md-item[data-studio-menu-cmd="insert-shape"]').click()`);
  await sleep(80);
  await clickCenter("#doc-shape-fill-host [data-color-toggle]");
  assert.equal(await js(`document.getElementById("doc-shape-fill-palette").hidden`), false);
  await js(`document.getElementById("doc-shape-text").focus()`);
  await key("Enter");
  await sleep(100);
  assert.equal(await js(`document.getElementById("doc-shape-dialog").open`), false);
  assert.equal(await js(`document.getElementById("doc-shape-fill-palette").hidden`), true, "closing dialog closes open color palette");
  await js(`document.querySelector('.md-item[data-studio-menu-cmd="review-comment"]').click()`);
  await sleep(80);
  assert.equal(await js(`document.getElementById("doc-comment-dialog").open`), true);
  await key("Escape");
  await sleep(80);
  assert.equal(await js(`document.getElementById("doc-comment-dialog").open`), false, "next modal closes on first Escape");

  // Reopen #doc-shape-dialog -> palette starts hidden and Escape directly closes dialog
  await js(`document.querySelector('.md-item[data-studio-menu-cmd="insert-shape"]').click()`);
  await sleep(80);
  assert.equal(await js(`document.getElementById("doc-shape-dialog").open`), true);
  assert.equal(await js(`document.getElementById("doc-shape-fill-palette").hidden`), true);
  await key("Escape");
  await sleep(80);
  assert.equal(await js(`document.getElementById("doc-shape-dialog").open`), false);

  // Chart insertion uses clean feedback without stale palette count
  await js(`document.querySelector('.md-item[data-studio-menu-cmd="insert-chart"]').click()`);
  await sleep(80);
  await clickCenter("#btn-doc-chart-ok");
  assert.equal(await js(`document.getElementById("status-command-feedback").textContent`), "차트 삽입 완료");
});

test("DOC capture->bubble Ctrl+Enter in #doc-comment-dialog, background shortcut block, and outside-table sort guard", { skip }, async () => {
  await navigateModule("DOC");
  await js(`(() => {
    const ed = document.getElementById("doc-editor");
    ed.replaceChildren();
    const p = document.createElement("p");
    p.textContent = "문서 본문 문단";
    ed.appendChild(p);
    const r = document.createRange();
    r.setStart(p.firstChild, 0);
    r.setEnd(p.firstChild, 2);
    const s = getSelection();
    s.removeAllRanges();
    s.addRange(r);
  })()`);
  await js(`document.querySelector('.md-item[data-studio-menu-cmd="review-comment"]').click()`);
  await sleep(100);
  assert.equal(await js(`document.getElementById("doc-comment-dialog").open`), true);

  // Background Ctrl+B must not toggle bold while modal is open
  await key("b", CTRL);
  await sleep(80);
  assert.equal(await js(`document.querySelector("#doc-editor p .eo-run[data-eo-bold='true']")`), null);

  // Real 2-phase capture -> bubble: Ctrl+Enter inside #doc-comment-input submits and closes dialog cleanly without leaking
  await js(`(() => {
    window.__leakedEnterCount = 0;
    window.addEventListener("keydown", (e) => { if (e.key === "Enter") window.__leakedEnterCount += 1; });
    document.getElementById("doc-comment-input").focus();
  })()`);
  await text("실브라우저 메모");
  await key("Enter", CTRL);
  await sleep(120);
  assert.equal(await js(`document.getElementById("doc-comment-dialog").open`), false);
  assert.equal(await js(`window.__leakedEnterCount`), 0, "child-closed modal keydown never leaks to window");
  assert.match(await js(`document.getElementById("doc-editor").textContent`), /실브라우저 메모/);

  // Outside-table sortTableAt refuses to sort any table when caret is in paragraph outside tables
  await js(`(() => {
    const ed = document.getElementById("doc-editor");
    ed.replaceChildren();
    const mk = (vals) => {
      const t = document.createElement("table");
      const tb = document.createElement("tbody");
      for (const v of vals) {
        const tr = document.createElement("tr");
        const td = document.createElement("td");
        td.textContent = v;
        tr.appendChild(td);
        tb.appendChild(tr);
      }
      t.appendChild(tb);
      return t;
    };
    const t1 = mk(["다", "가"]);
    const t2 = mk(["나", "가"]);
    const p = document.createElement("p");
    p.textContent = "표 밖";
    ed.append(t1, t2, p);
    const r = document.createRange();
    r.setStart(p.firstChild, 0);
    r.setEnd(p.firstChild, 0);
    const s = getSelection();
    s.removeAllRanges();
    s.addRange(r);
  })()`);
  await js(`document.querySelector('.md-item[data-studio-menu-cmd="data-sort"]').click()`);
  await sleep(120);
  assert.deepEqual(
    JSON.parse(await js(`JSON.stringify([...document.querySelectorAll("#doc-editor table td")].map((td) => td.textContent))`)),
    ["다", "가", "나", "가"]
  );
  assert.equal(await js(`document.getElementById("status-command-feedback").textContent`), "표 안에서 사용");

  // Caret moves from t1 into .eo-doc-notes (non-paragraph studio container) -> refuses sort instead of using stale activeDocBlockEl
  await js(`(() => {
    const ed = document.getElementById("doc-editor");
    const t1 = ed.querySelector("table");
    const notes = document.createElement("aside");
    notes.className = "eo-doc-notes";
    notes.dataset.eoNotes = "";
    const np = document.createElement("p");
    np.textContent = "각주";
    notes.appendChild(np);
    ed.appendChild(notes);
    const s = getSelection();
    const r1 = document.createRange();
    r1.setStart(t1.querySelector("td").firstChild, 0);
    r1.setEnd(t1.querySelector("td").firstChild, 0);
    s.removeAllRanges();
    s.addRange(r1);
    document.dispatchEvent(new Event("selectionchange"));
    const r2 = document.createRange();
    r2.setStart(np.firstChild, 0);
    r2.setEnd(np.firstChild, 0);
    s.removeAllRanges();
    s.addRange(r2);
    document.dispatchEvent(new Event("selectionchange"));
  })()`);
  await js(`document.querySelector('.md-item[data-studio-menu-cmd="data-sort"]').click()`);
  await sleep(120);
  assert.deepEqual(
    JSON.parse(await js(`JSON.stringify([...document.querySelectorAll("#doc-editor table td")].map((td) => td.textContent))`)),
    ["다", "가", "나", "가"]
  );
  assert.equal(await js(`document.getElementById("status-command-feedback").textContent`), "표 안에서 사용");

  // Empty selection when first block is a table -> refuses sort instead of falling back to firstBlock
  await js(`(() => {
    const ed = document.getElementById("doc-editor");
    const t1 = ed.querySelector("table");
    ed.replaceChildren(t1);
    getSelection().removeAllRanges();
  })()`);
  await js(`document.querySelector('.md-item[data-studio-menu-cmd="data-sort"]').click()`);
  await sleep(120);
  assert.deepEqual(
    JSON.parse(await js(`JSON.stringify([...document.querySelectorAll("#doc-editor table td")].map((td) => td.textContent))`)),
    ["다", "가"]
  );
  assert.equal(await js(`document.getElementById("status-command-feedback").textContent`), "표 안에서 사용");
});

test("BOARD single zoom model and PDF active pane fit zoom work in real Chrome", { skip }, async () => {
  await navigateModule("BOARD");
  await clickCenter("#btn-zoom-in");
  const boardZoom = JSON.parse(await js(`JSON.stringify({
    label: document.getElementById("status-zoom-label").textContent,
    stageZoom: document.querySelector(".canvas-stage").style.zoom,
    worldTransform: document.getElementById("board-world").style.transform,
  })`));
  assert.equal(boardZoom.label, "125%");
  assert.equal(boardZoom.stageZoom, "", "BOARD must not double-apply stage.style.zoom");
  assert.match(boardZoom.worldTransform, /scale\(1\.25\)/);

  await navigateModule("PDF");
  await clickCenter("#btn-zoom-fit-width");
  const pdfFit = Number.parseInt(await js(`document.getElementById("status-zoom-label").textContent`), 10);
  assert.ok(pdfFit >= 50 && pdfFit <= 200, `PDF fit width computes valid zoom (${pdfFit}%)`);
});

test("SHEET modal Tab exclusion via getComputedStyle, Space on button, Enter on checkbox, nested modals, and native cancel", { skip }, async () => {
  await navigateModule("SHEET");

  // 1. Space on <button> closes dialog; Enter on <input type="checkbox"> clicks primary button
  await js(`document.querySelector('.md-item[data-studio-menu-cmd="sheet-mod-pivot"]').click()`);
  await sleep(100);
  assert.equal(await js(`document.getElementById("sheet-pivot-dialog").open`), true);
  await js(`document.querySelector("#sheet-pivot-dialog .dialog-footer [data-dialog-close]").focus()`);
  await key(" ");
  await sleep(100);
  assert.equal(await js(`document.getElementById("sheet-pivot-dialog").open`), false, "Space activates cancel button");

  await js(`document.querySelector('.md-item[data-studio-menu-cmd="sheet-mod-pivot"]').click()`);
  await sleep(100);
  await js(`(() => {
    window.__pivotPrimaryCount = 0;
    const btn = document.getElementById("btn-sheet-pivot-apply");
    window.__pivotSpy = () => { window.__pivotPrimaryCount += 1; };
    btn.addEventListener("click", window.__pivotSpy);
    document.getElementById("sheet-pivot-header").focus();
  })()`);
  await key("Enter");
  await sleep(100);
  assert.equal(await js(`window.__pivotPrimaryCount`), 1, "Enter on checkbox triggers primary button");
  await js(`(() => {
    const btn = document.getElementById("btn-sheet-pivot-apply");
    if (window.__pivotSpy) btn.removeEventListener("click", window.__pivotSpy);
    document.getElementById("sheet-pivot-dialog").close?.();
  })()`);

  // 2. window.getComputedStyle exclusion in #sheet-cf-dialog (data-cf-kinds hidden rows, closed select options, CSS visibility/display probes)
  await js(`document.querySelector('.md-item[data-studio-menu-cmd="sheet-mod-condfmt"]').click()`);
  await sleep(120);
  assert.equal(await js(`document.getElementById("sheet-cf-dialog").open`), true);
  await js(`(() => {
    const body = document.querySelector("#sheet-cf-dialog .dialog-body");
    const cssDispProbe = document.createElement("button");
    cssDispProbe.id = "probe-css-display-none";
    cssDispProbe.type = "button";
    cssDispProbe.textContent = "숨김1";
    cssDispProbe.style.display = "none";
    const cssVisProbe = document.createElement("button");
    cssVisProbe.id = "probe-css-visibility-hidden";
    cssVisProbe.type = "button";
    cssVisProbe.textContent = "숨김2";
    cssVisProbe.style.visibility = "hidden";
    body.append(cssDispProbe, cssVisProbe);
  })()`);

  const hiddenRowDisplay = await js(`getComputedStyle(document.querySelector('#sheet-cf-dialog [data-cf-kinds="scale"]')).display`);
  assert.equal(hiddenRowDisplay, "none");

  const visited = [];
  for (let i = 0; i < 40; i += 1) {
    await key("Tab");
    const cur = JSON.parse(await js(`JSON.stringify((() => {
      const el = document.activeElement;
      return {
        id: el?.id || "",
        cls: el?.className || "",
        inHiddenKind: Boolean(el?.closest?.("[data-cf-kinds]") && getComputedStyle(el.closest("[data-cf-kinds]")).display === "none"),
        inClosedSelectMenu: Boolean(el?.closest?.(".custom-select-menu") && !el?.closest?.(".custom-select.open")),
      };
    })())`));
    assert.equal(cur.inHiddenKind, false, `Tab must skip hidden data-cf-kinds row (${cur.id})`);
    assert.equal(cur.inClosedSelectMenu, false, "Tab must skip closed .custom-select-menu options");
    assert.notEqual(cur.id, "probe-css-display-none", "Tab skips CSS display:none element without [hidden]");
    assert.notEqual(cur.id, "probe-css-visibility-hidden", "Tab skips CSS visibility:hidden element");
    visited.push(cur.id || cur.cls);
  }
  assert.ok(visited.includes("btn-sheet-cf-apply"), `Tab visited visible primary button (${visited.join(",")})`);

  // Cycle from last focusable button back to first (.dialog-close), and Shift+Tab back to last
  await js(`document.querySelector("#sheet-cf-dialog .dialog-footer [data-dialog-close]").focus()`);
  await key("Tab");
  assert.equal(await js(`document.activeElement?.classList?.contains("dialog-close")`), true, "Tab wraps from last to first");
  await key("Tab", SHIFT);
  assert.equal(
    await js(`document.activeElement === document.querySelector("#sheet-cf-dialog .dialog-footer [data-dialog-close]")`),
    true,
    "Shift+Tab wraps from first to last"
  );
  await js(`(() => {
    document.getElementById("probe-css-display-none")?.remove();
    document.getElementById("probe-css-visibility-hidden")?.remove();
  })()`);
  // Native cancel event while nested .custom-select.open is open on top dialog is prevented
  await clickCenter('#sheet-cf-dialog [data-select-id="sheet-cf-type-select"] .custom-select-trigger');
  assert.equal(
    await js(`document.querySelector('#sheet-cf-dialog [data-select-id="sheet-cf-type-select"]').classList.contains("open")`),
    true
  );
  const popupCancelPrevented = await js(`(() => {
    const ev = new Event("cancel", { cancelable: true });
    document.getElementById("sheet-cf-dialog").dispatchEvent(ev);
    return ev.defaultPrevented;
  })()`);
  assert.equal(popupCancelPrevented, true, "native cancel prevented while nested popup is open");
  await key("Escape");
  await sleep(80);
  assert.equal(
    await js(`document.querySelector('#sheet-cf-dialog [data-select-id="sheet-cf-type-select"]').classList.contains("open")`),
    false
  );
  assert.equal(await js(`document.getElementById("sheet-cf-dialog").open`), true);
  await key("Escape");
  await sleep(80);
  assert.equal(await js(`document.getElementById("sheet-cf-dialog").open`), false);

  // 3. Nested modals (#sheet-names-dialog -> #sheet-name-edit-dialog) Escape / Enter / native cancel
  await js(`document.querySelector('.md-item[data-sheet-act="name-manage"]').click()`);
  await sleep(100);
  assert.equal(await js(`document.getElementById("sheet-names-dialog").open`), true);
  await clickCenter("#btn-sheet-names-new");
  assert.equal(await js(`document.getElementById("sheet-name-edit-dialog").open`), true);
  assert.equal(await js(`document.getElementById("sheet-names-dialog").open`), true);

  // Native cancel event on background non-top modal is prevented
  const parentCancelPrevented = await js(`(() => {
    const ev = new Event("cancel", { cancelable: true });
    document.getElementById("sheet-names-dialog").dispatchEvent(ev);
    return ev.defaultPrevented;
  })()`);
  assert.equal(parentCancelPrevented, true, "non-top parent modal prevents native cancel event");
  assert.equal(await js(`document.getElementById("sheet-names-dialog").open`), true);

  // Enter inside child #sheet-name-edit-name submits only child modal
  await js(`document.getElementById("sheet-name-edit-name").focus()`);
  await text("실테스트이름");
  await key("Enter");
  await sleep(140);
  assert.equal(await js(`document.getElementById("sheet-name-edit-dialog").open`), false);
  assert.equal(await js(`document.getElementById("sheet-names-dialog").open`), true);
  assert.match(await js(`document.getElementById("sheet-names-list").textContent`), /실테스트이름/);

  // Reopen child modal, press Escape twice -> first closes child, second closes parent
  await clickCenter("#btn-sheet-names-new");
  assert.equal(await js(`document.getElementById("sheet-name-edit-dialog").open`), true);
  await key("Escape");
  await sleep(80);
  assert.equal(await js(`document.getElementById("sheet-name-edit-dialog").open`), false);
  assert.equal(await js(`document.getElementById("sheet-names-dialog").open`), true);
  await key("Escape");
  await sleep(80);
  assert.equal(await js(`document.getElementById("sheet-names-dialog").open`), false);
});

test("Slice 2 real Chrome: BASE custom-select hydration + unclipped menu, BOARD SVG icons + chart combo, toolbar-labels 34px layout", { skip }, async () => {
  await navigateModule("BASE");
  await js(`(() => {
    const host = window.__EASYOFFICE_BASE_BOARD_HOST__.getBaseStudio();
    const M = window.__EASYOFFICE_BASE_MODEL__;
    const st = M.createInitialBaseState();
    const dept = st.tables[0];
    dept.name = "부서";
    dept.records = [
      { id: "d1", values: { [dept.fields[0].id]: "개발팀" } },
      { id: "d2", values: { [dept.fields[0].id]: "디자인팀" } },
    ];
    const staff = M.addTable(st, {
      name: "직원",
      fields: [
        { id: "f_name", name: "이름", type: "text" },
        { id: "f_role", name: "직무", type: "select", options: ["엔지니어", "PM"] },
        { id: "f_dept", name: "소속", type: "relation", relationTableId: dept.id },
      ],
    }).table;
    staff.records = Array.from({ length: 130 }, (_, i) => ({
      id: "r_" + (i + 1),
      values: { f_name: "사원 " + (i + 1), f_role: i % 2 === 0 ? "엔지니어" : "PM", f_dept: [] },
    }));
    st.activeTableId = staff.id;
    host.setState(st);
  })()`);
  await sleep(160);

  // Check every select in BASE viewport uses custom-select and relation popup renders unclipped
  const baseInitial = await js(`(() => {
    const selects = [...document.querySelectorAll("#pane-base select")];
    return {
      total: selects.length,
      unenhanced: selects.filter((s) => !s.classList.contains("native-select-hidden") || !s.parentElement?.classList?.contains("custom-select")).length,
    };
  })()`);
  assert.ok(baseInitial.total > 0);
  assert.equal(baseInitial.unenhanced, 0, "all BASE selects enhanced initially");

  await clickCenter("#pane-base .base-relation-cell .custom-select-trigger");
  await sleep(100);
  const relPopup = await js(`(() => {
    const custom = document.querySelector("#pane-base .base-relation-cell .custom-select");
    const menu = custom?.querySelector(".custom-select-menu");
    const menuRect = menu?.getBoundingClientRect();
    const cellRect = custom?.closest(".base-relation-cell")?.getBoundingClientRect();
    const cs = custom ? getComputedStyle(custom.closest(".base-relation-cell")) : null;
    return {
      open: Boolean(custom?.classList?.contains("open")),
      options: [...(custom?.querySelectorAll(".custom-select-option") || [])].map((b) => b.textContent),
      menuH: menuRect?.height || 0,
      cellH: cellRect?.height || 0,
      overflow: cs?.overflow || "",
    };
  })()`);
  assert.equal(relPopup.open, true);
  assert.ok(relPopup.options.includes("개발팀") && relPopup.options.includes("디자인팀"), "real Chrome relation options populated");
  assert.ok(relPopup.menuH > 40, "relation custom-select menu renders unclipped in real Chrome");
  assert.equal(relPopup.overflow, "visible", "relation cell lifts overflow clip while menu is open");

  // Scroll 130-row virtual table and confirm all newly created selects are enhanced
  await js(`(() => {
    const w = document.querySelector("#pane-base .base-grid-wrap");
    w.scrollTop = 2000;
    w.dispatchEvent(new Event("scroll"));
  })()`);
  await sleep(160);
  const baseScrolled = await js(`(() => {
    const selects = [...document.querySelectorAll("#pane-base select")];
    return {
      total: selects.length,
      unenhanced: selects.filter((s) => !s.classList.contains("native-select-hidden") || !s.parentElement?.classList?.contains("custom-select")).length,
    };
  })()`);
  assert.ok(baseScrolled.total > 0);
  assert.equal(baseScrolled.unenhanced, 0, "all BASE selects enhanced after virtual scroll");

  // Navigate to BOARD: check icons + chart dialog custom-select sync + toolbar-labels 34px toggle
  await navigateModule("BOARD");
  const boardCheck = await js(`(() => {
    const board = window.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
    board.addNode({ kind: "chart", chartType: "bar", x: 40, y: 40, w: 280, h: 160, title: "차트" });
    const nodeEl = document.querySelector("#pane-board .board-node");
    nodeEl.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    const dlg = document.getElementById("board-chart-dialog");
    const val = dlg.querySelector('[data-select-id="board-chart-type"] .custom-select-value')?.textContent;
    dlg.close();
    const icons = ["#btn-board-export-pdf", "#btn-board-add-chart", "#btn-board-delete"].map((id) => {
      const r = document.querySelector(id + " svg.eo-icon")?.getBoundingClientRect();
      return { id, w: r?.width || 0, h: r?.height || 0 };
    });
    return { val, icons };
  })()`);
  assert.equal(boardCheck.val, "묶은 가로 막대형");
  for (const ic of boardCheck.icons) {
    assert.ok(ic.w >= 16 && ic.h >= 16, `${ic.id} renders visible SVG icon`);
  }

  // Toggle toolbar-labels off and verify #icon-toolbar compact height (34px) in real Chrome
  await navigateModule("DOC");
  const tbToggle = await js(`(() => {
    const beforeH = document.getElementById("icon-toolbar").getBoundingClientRect().height;
    document.querySelector("[data-view-toolbar-labels]").click();
    const hiddenH = document.getElementById("icon-toolbar").getBoundingClientRect().height;
    const lblDisplay = getComputedStyle(document.querySelector("#icon-toolbar .tb-label")).display;
    document.querySelector("[data-view-toolbar-labels]").click();
    const restoredH = document.getElementById("icon-toolbar").getBoundingClientRect().height;
    return { beforeH, hiddenH, lblDisplay, restoredH };
  })()`);
  assert.equal(tbToggle.lblDisplay, "none");
  assert.ok(tbToggle.hiddenH <= 36 && tbToggle.hiddenH < tbToggle.beforeH, `compact toolbar ${tbToggle.hiddenH} < ${tbToggle.beforeH}`);
  assert.equal(tbToggle.restoredH, tbToggle.beforeH);
});
