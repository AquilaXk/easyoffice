import test, { after, before } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { REPO, sleep, startBrowser, startEngine, startUiServer } from "./bench-webui-support.mjs";
import { findChrome } from "./hwp-studio-harness.mjs";

const exe = process.platform === "win32" ? "easyoffice-app.exe" : "easyoffice-app";
const APP = [
  process.env.EO_APP,
  path.join(REPO, "../target-193/debug", exe),
  path.join(REPO, "target-193/debug", exe),
  path.join(REPO, "target/debug", exe),
].find((file) => file && fs.existsSync(file));

let CHROME = null;
try {
  CHROME = findChrome();
} catch {
  CHROME = null;
}
const skip = !APP || !CHROME ? "needs easyoffice-app and Chrome (EO_APP, CHROME_PATH)" : false;

const CTRL = 2;
const SHIFT = 8;
let ctx = null;
let uiPort = 0;

before(async () => {
  if (skip) return;
  const port = 24000 + Math.floor(Math.random() * 16000);
  const engine = startEngine(APP, port);
  await sleep(900);
  uiPort = port + 1;
  const server = await startUiServer({ enginePort: port, port: uiPort });
  const browser = await startBrowser({
    chrome: CHROME,
    debugPort: port + 2,
    url: `http://127.0.0.1:${uiPort}/?module=DOC&win=slice5-doc-chrome`,
    width: 1320,
    height: 840,
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

const js = (expr) => ctx.cdp.evalJs(expr);

async function waitFor(fnExpr, timeoutMs = 4000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      if (await js(fnExpr)) return;
    } catch {
      // still loading
    }
    await sleep(60);
  }
  throw new Error(`Timed out waiting for: ${fnExpr}`);
}

async function openModule(mod) {
  await ctx.cdp.send("Page.navigate", { url: `http://127.0.0.1:${uiPort}/?module=${mod}&win=slice5-${mod}-chrome` });
  await waitFor(`document.readyState === "complete" && document.getElementById("app")?.dataset?.module === ${JSON.stringify(mod)}`);
  await sleep(300);
}

async function sendKey(keyName, code, vk, modifiers = 0) {
  const base = { key: keyName, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers };
  await ctx.cdp.send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
  await ctx.cdp.send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
}

async function mouseClick(sel, button = "left", modifiers = 0) {
  const r = await js(`(() => {
    const el = document.querySelector(${JSON.stringify(sel)});
    if (!el) return null;
    const b = el.getBoundingClientRect();
    return b.width > 0 && b.height > 0 ? { left: b.left, top: b.top, width: b.width, height: b.height } : null;
  })()`);
  assert.ok(r, `element visible for click: ${sel}`);
  const x = r.left + r.width / 2;
  const y = r.top + r.height / 2;
  const buttons = button === "right" ? 2 : 1;
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y, modifiers });
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button, buttons, clickCount: 1, modifiers });
  await ctx.cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button, buttons: 0, clickCount: 1, modifiers });
}

test("real Chrome: DOC sub/sup/spacing & DOCX table-cell roundtrip, SLIDE UX/search, PDF page-jump search, BASE import+undo, BOARD style+clipboard", { skip }, async () => {
  // 1. DOC: sup/sub buttons, Ctrl+= / Ctrl+Shift+=, spacing inputs, and real DOCX roundtrip via /api/ipc
  await waitFor(`Boolean(document.getElementById("btn-stylebar-sup") && window.__EASYOFFICE_SAVE_PAYLOAD__)`);
  await js(`(() => {
    const ed = document.getElementById("doc-editor");
    ed.innerHTML = "<p>H2O and E=mc2</p>";
    const p = ed.firstChild;
    const tn = p.firstChild;
    const sel = window.getSelection();
    const r = document.createRange();
    r.setStart(tn, 1);
    r.setEnd(tn, 2);
    sel.removeAllRanges();
    sel.addRange(r);
    ed.focus();
  })()`);
  await sleep(60);
  await sendKey("=", "Equal", 187, CTRL);
  await sleep(120);
  assert.equal(await js(`document.querySelector('#doc-editor .eo-run[data-eo-subscript="true"]')?.textContent`), "2");

  // Click superscript button on last char
  await js(`(() => {
    const ed = document.getElementById("doc-editor");
    const tn = Array.from(ed.firstChild.childNodes).find((n) => n.nodeType === 3 && n.textContent.includes("mc2"));
    const idx = tn.textContent.indexOf("2");
    const sel = window.getSelection();
    const r = document.createRange();
    r.setStart(tn, idx);
    r.setEnd(tn, idx + 1);
    sel.removeAllRanges();
    sel.addRange(r);
  })()`);
  await mouseClick("#btn-stylebar-sup");
  await sleep(100);
  assert.equal(await js(`document.querySelector('#doc-editor .eo-run[data-eo-superscript="true"]')?.textContent`), "2");

  // Set spacing + rich table-cell formatting, convert to real DOCX via Rust IPC, then reopen
  const rtRes = await js(`(async () => {
    const ed = document.getElementById("doc-editor");
    ed.innerHTML =
      '<p style="margin-top:16px;margin-bottom:32px" data-eo-space-before-twips="240" data-eo-space-after-twips="480">문단 간격 시험</p>' +
      '<table><tbody><tr>' +
      '<td><p style="text-align:center;line-height:1.5" data-eo-line-spacing="150"><b>Bold</b> <sub>sub</sub> <span style="color:#b91c1c">Red</span></p></td>' +
      '<td>Plain</td>' +
      '</tr></tbody></table>';
    const blocks = window.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(ed);
    const resp = await fetch("/api/ipc", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        command: "convert_document_format",
        payload: JSON.stringify({ sourceFormat: "docx", targetFormat: "docx", title: "slice5-rt", blocks })
      })
    });
    const parsed = await resp.json();
    const bin = atob(parsed.convertedBase64 || "");
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i += 1) bytes[i] = bin.charCodeAt(i);
    window.confirm = () => true;
    const opened = await window.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("slice5-rt.docx", bytes);
    const ps = ed.querySelectorAll("p");
    const td = ed.querySelector("table td");
    return {
      ok: opened.ok,
      before: ps[0]?.dataset?.eoSpaceBeforeTwips,
      after: ps[0]?.dataset?.eoSpaceAfterTwips,
      align: td?.querySelector("p")?.style?.textAlign || td?.style?.textAlign || "",
      bold: td?.querySelector('.eo-run[data-eo-bold="true"], b, strong')?.textContent || "",
      sub: td?.querySelector('.eo-run[data-eo-subscript="true"], sub')?.textContent || "",
      red: td?.querySelector('.eo-run[data-eo-color="B91C1C"]')?.textContent || ""
    };
  })()`);
  assert.equal(rtRes.ok, true);
  assert.equal(rtRes.before, "240");
  assert.equal(rtRes.after, "480");
  assert.equal(rtRes.align, "center");
  assert.equal(rtRes.bold, "Bold");
  assert.equal(rtRes.sub, "sub");
  assert.equal(rtRes.red, "Red");

  // 2. SLIDE: multiline notes isolating Ctrl+D, context menu duplicate/bring-forward, align, and Ctrl+F cross-slide jump
  await openModule("SLIDE");
  await waitFor(`Boolean(window.__EASYOFFICE_SLIDE_CANVAS__ && document.getElementById("slide-presenter-notes"))`);
  await mouseClick("#slide-presenter-notes");
  await ctx.cdp.send("Input.insertText", { text: "첫 줄\n둘째 줄" });
  await sleep(60);
  const preNotesCount = await js(`window.__EASYOFFICE_SLIDE_CANVAS__.getElements().length`);
  await sendKey("d", "KeyD", 68, CTRL);
  await sleep(80);
  assert.equal(await js(`window.__EASYOFFICE_SLIDE_CANVAS__.getElements().length`), preNotesCount, "textarea notes isolates Ctrl+D");
  assert.equal(await js(`document.getElementById("slide-presenter-notes").value`), "첫 줄\n둘째 줄");

  // Add deterministic boxes on Slide 1, right-click box 1, duplicate via context menu
  await js(`(() => {
    const c = window.__EASYOFFICE_SLIDE_CANVAS__;
    while (c.getElements().length > 0) c.deleteElement(c.getElements()[0].id);
    c.addElement({ kind: "textbox", x: 50, y: 60, w: 120, h: 48, text: "상자A" });
    c.addElement({ kind: "textbox", x: 240, y: 120, w: 120, h: 48, text: "상자B" });
    c.selectElement(c.getElements()[0].id);
  })()`);
  await sleep(80);
  await mouseClick('#slide-canvas-layer .slide-el', "right");
  await sleep(80);
  assert.equal(await js(`document.getElementById("slide-context-menu").hidden`), false);
  await mouseClick('#slide-ctx-canvas-group [data-slide-canvas-act="duplicate"]');
  await sleep(100);
  assert.equal(await js(`window.__EASYOFFICE_SLIDE_CANVAS__.getElements().length`), 3);
  assert.equal(await js(`window.__EASYOFFICE_SLIDE_CANVAS__.getElements()[2].x`), 66);

  // Add Slide 2 with searchable shape, switch back to Slide 1, press Ctrl+F -> find jumps to Slide 2 keeping #find-query focused
  await mouseClick("#btn-add-slide");
  await sleep(100);
  await js(`(() => {
    window.__EASYOFFICE_SLIDE_CANVAS__.addElement({ kind: "shape", x: 100, y: 100, w: 180, h: 80, text: "분기핵심표적" });
    window.__EASYOFFICE_MODULE_FIND_HOOKS__.switchSlide(0);
  })()`);
  await sleep(100);
  await sendKey("f", "KeyF", 70, CTRL);
  await sleep(80);
  assert.equal(await js(`document.getElementById("find-dialog").open`), true);
  await ctx.cdp.send("Input.insertText", { text: "분기핵심표적" });
  await sleep(150);
  assert.equal(await js(`document.getElementById("find-count").textContent`), "1 / 1");
  assert.equal(await js(`window.__EASYOFFICE_MODULE_FIND_HOOKS__.getSlideIndex()`), 1);
  assert.equal(await js(`document.activeElement?.id`), "find-query");
  await sendKey("Escape", "Escape", 27);
  await sleep(60);

  // 3. PDF: generate real multi-page PDF via /api/ipc, open it, Ctrl+F jumps to page 2 while keeping #find-query focus
  await openModule("PDF");
  const pdfOpen = await js(`(async () => {
    const blocks = [];
    for (let i = 0; i < 85; i += 1) {
      blocks.push({ type: "paragraph", runs: [{ text: i === 80 ? "SecondPageTargetToken" : "Body line number " + i + " sample text." }] });
    }
    const resp = await fetch("/api/ipc", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        command: "convert_document_format",
        payload: JSON.stringify({ sourceFormat: "docx", targetFormat: "pdf", title: "multipage", blocks })
      })
    });
    const parsed = await resp.json();
    const bin = atob(parsed.convertedBase64 || "");
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i += 1) bytes[i] = bin.charCodeAt(i);
    window.confirm = () => true;
    return window.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("multipage.pdf", bytes);
  })()`);
  assert.equal(pdfOpen.ok, true);
  await sleep(150);
  await sendKey("f", "KeyF", 70, CTRL);
  await sleep(80);
  await ctx.cdp.send("Input.insertText", { text: "SecondPageTargetToken" });
  await sleep(150);
  assert.equal(await js(`document.getElementById("find-count").textContent`), "1 / 1");
  assert.match(await js(`document.getElementById("pdf-page-indicator").textContent`), /^쪽\s+2\s+\/\s+\d+/);
  assert.equal(await js(`document.activeElement?.id`), "find-query");

  // 4. BASE: open #base-import-dialog button, run real CSV decode IPC import + formula neutralization + Ctrl+Z undo + oversized reject
  await openModule("BASE");
  await mouseClick("#btn-base-import");
  await sleep(80);
  assert.equal(await js(`document.getElementById("base-import-dialog").open`), true);
  await sendKey("Escape", "Escape", 27);
  await sleep(60);

  const baseImp = await js(`(async () => {
    const imp = window.__EASYOFFICE_BASE_IMPORT__;
    const model = window.__EASYOFFICE_BASE_MODEL__;
    const bs = window.__EASYOFFICE_BASE_BOARD_HOST__.getBaseStudio();
    const beforeCount = bs.getState().tables.length;
    const csv = new TextEncoder().encode("이름,금액,완료\\n=CMD(),-42.5,true\\n홍길동,1500,false\\n");
    const res = await imp.controller().importBytes("sales.csv", csv, { mode: "new", tableName: "실제수입", hasHeader: true });
    const tbl = model.getTable(bs.getState());
    const oversize = await imp.controller().importBytes("too-big.csv", new Uint8Array(imp.MAX_IMPORT_BYTES + 1), { mode: "new" });
    return {
      ok: res.ok,
      beforeCount,
      afterCount: bs.getState().tables.length,
      formulaCell: tbl.records[0].values[tbl.fields[0].id],
      negNumber: tbl.records[0].values[tbl.fields[1].id],
      oversizeOk: oversize.ok,
    };
  })()`);
  assert.equal(baseImp.ok, true);
  assert.equal(baseImp.afterCount, baseImp.beforeCount + 1);
  assert.equal(baseImp.formulaCell, "'=CMD()");
  assert.equal(baseImp.negNumber, -42.5);
  assert.equal(baseImp.oversizeOk, false);
  await sendKey("z", "KeyZ", 90, CTRL);
  await sleep(200);
  assert.equal(await js(`window.__EASYOFFICE_BASE_BOARD_HOST__.getBaseStudio().getState().tables.length`), baseImp.beforeCount);

  // Also test real XLSX encode -> decode -> BASE import roundtrip against real IPC
  const xlsxImp = await js(`(async () => {
    const imp = window.__EASYOFFICE_BASE_IMPORT__;
    const model = window.__EASYOFFICE_BASE_MODEL__;
    const bs = window.__EASYOFFICE_BASE_BOARD_HOST__.getBaseStudio();
    const resp = await fetch("/api/ipc", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        command: "convert_document_format",
        payload: JSON.stringify({
          sourceFormat: "xlsx",
          targetFormat: "xlsx",
          title: "xlsx-src",
          sheets: [{
            name: "시트1",
            cells: [
              { ref: "A1", raw: "제품", t: "s" },
              { ref: "B1", raw: "수량", t: "s" },
              { ref: "A2", raw: "문서엔진", t: "s" },
              { ref: "B2", raw: "7", t: "n" }
            ]
          }]
        })
      })
    });
    const parsed = await resp.json();
    const bin = atob(parsed.convertedBase64 || "");
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i += 1) bytes[i] = bin.charCodeAt(i);
    const res = await imp.controller().importBytes("real.xlsx", bytes, { mode: "new", tableName: "XLSX수입", hasHeader: true });
    const tbl = model.getTable(bs.getState());
    return { ok: res.ok, count: tbl.records.length, val: tbl.records[0]?.values[tbl.fields[1]?.id] };
  })()`);
  assert.equal(xlsxImp.ok, true);
  assert.equal(xlsxImp.count, 1);
  assert.equal(xlsxImp.val, 7);

  // 5. BOARD: style bar controls, Ctrl+D multi-node/connector clone + Ctrl+Z undo, Ctrl+F select + viewport pan
  await openModule("BOARD");
  await waitFor(`Boolean(window.__EASYOFFICE_BASE_BOARD_HOST__?.getBoardStudio?.())`);
  const boardSetup = await js(`(() => {
    const b = window.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
    b.selectNode(null);
    const barHidden = document.getElementById("board-style-bar").hidden;
    const n1 = b.addNode({ kind: "shape", x: 100, y: 100, w: 140, h: 70, text: "기준" });
    const n2 = b.addNode({ kind: "shape", x: 2200, y: 1500, w: 160, h: 80, text: "보드타겟노드" });
    b.addConnector({ fromId: n1.id, toId: n2.id, label: "링크" });
    b.selectNode(n1.id);
    const barVisible = !document.getElementById("board-style-bar").hidden;
    const lbl = document.getElementById("board-style-label");
    lbl.value = "수정됨";
    lbl.dispatchEvent(new Event("change", { bubbles: true }));
    const editedText = b.getState().nodes.find((n) => n.id === n1.id).text;
    b.selectNode(n1.id);
    b.selectNode(n2.id, { toggle: true });
    return { barHidden, barVisible, editedText, nodes: b.getState().nodes.length, conns: b.getState().connectors.length, n1Id: n1.id, n2Id: n2.id };
  })()`);
  assert.equal(boardSetup.barHidden, true);
  assert.equal(boardSetup.barVisible, true);
  assert.equal(boardSetup.editedText, "수정됨");

  await mouseClick("#board-viewport");
  await js(`(() => {
    const b = window.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
    b.selectNode(${JSON.stringify(boardSetup.n1Id)});
    b.selectNode(${JSON.stringify(boardSetup.n2Id)}, { toggle: true });
  })()`);
  await sleep(60);
  await sendKey("d", "KeyD", 68, CTRL);
  await sleep(120);
  assert.equal(await js(`window.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio().getState().nodes.length`), boardSetup.nodes + 2);
  assert.equal(await js(`window.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio().getState().connectors.length`), boardSetup.conns + 1);

  await sendKey("z", "KeyZ", 90, CTRL);
  await sleep(200);
  assert.equal(await js(`window.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio().getState().nodes.length`), boardSetup.nodes);

  await sendKey("f", "KeyF", 70, CTRL);
  await sleep(80);
  await ctx.cdp.send("Input.insertText", { text: "보드타겟노드" });
  await sleep(150);
  assert.equal(await js(`document.getElementById("find-count").textContent`), "1 / 1");
  assert.equal(await js(`window.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio()._styleHooks.getSelection().node?.id`), boardSetup.n2Id);
  assert.notEqual(await js(`window.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio().getState().viewport.panX`), 0);
  await sendKey("Escape", "Escape", 27);
  await sleep(60);

  // Real Chrome regression: BOARD custom-select trigger sync + SLIDE Shift-select multi-Delete + undo
  await js(`(() => {
    const b = window.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
    const s = b.getState().nodes.find((n) => n.kind === "shape");
    if (s) { b.selectNode(s.id); document.getElementById("board-style-width").value = "3"; document.getElementById("board-style-width").dispatchEvent(new Event("change", { bubbles: true })); }
    const c = b.getState().connectors[0];
    if (c) b.selectConnector(c.id);
  })()`);
  await sleep(60);
  assert.match(await js(`document.querySelector('[data-select-id="board-style-width"]')?.textContent || ""`), /1\.5pt/);

  await openModule("SLIDE");
  await waitFor(`Boolean(window.__EASYOFFICE_SLIDE_CANVAS__)`);
  await js(`(() => {
    const c = window.__EASYOFFICE_SLIDE_CANVAS__;
    while (c.getElements().length > 0) c.deleteElement(c.getElements()[0].id);
    const x1 = c.addElement({ kind: "textbox", x: 40, y: 40, w: 90, h: 40, text: "R1" });
    const x2 = c.addElement({ kind: "textbox", x: 180, y: 40, w: 90, h: 40, text: "R2" });
    c.addElement({ kind: "textbox", x: 320, y: 40, w: 90, h: 40, text: "R3" });
    c.setSelectedIds([x1.id, x2.id]);
    document.querySelector('#slide-canvas-layer .slide-el[data-element-id="' + x2.id + '"]')?.focus();
  })()`);
  await sleep(60);
  await sendKey("Delete", "Delete", 46);
  await sleep(100);
  assert.equal(await js(`window.__EASYOFFICE_SLIDE_CANVAS__.getElements().length`), 1, "real Chrome multi-select Delete deletes all selected shapes");
  await sendKey("z", "KeyZ", 90, CTRL);
  await sleep(150);
  assert.equal(await js(`window.__EASYOFFICE_SLIDE_CANVAS__.getElements().length`), 3, "1-step undo restores all multi-deleted shapes");

  // Real Chrome regression: native edit:cut while editing BOARD node contenteditable cuts only text selection and preserves node + connector
  await openModule("BOARD");
  await waitFor(`Boolean(window.__EASYOFFICE_BASE_BOARD_HOST__?.getBoardStudio?.())`);
  const boardCut = await js(`(() => {
    const b = window.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
    const n1 = b.addNode({ kind: "text", x: 100, y: 100, w: 160, h: 70, text: "HelloWorld" });
    const n2 = b.addNode({ kind: "shape", x: 340, y: 100, w: 120, h: 70, text: "KeepMe" });
    b.addConnector({ fromId: n1.id, toId: n2.id, label: "edge" });
    const card = document.querySelector('#board-node-layer .board-node[data-node-id="' + n1.id + '"]');
    card.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    const editable = card.querySelector('.board-node-text[contenteditable="plaintext-only"]');
    if (!editable || !editable.firstChild) return { ready: false };
    const sel = window.getSelection();
    const r = document.createRange();
    r.setStart(editable.firstChild, 5);
    r.setEnd(editable.firstChild, 10);
    sel.removeAllRanges();
    sel.addRange(r);
    const beforeNodes = b.getState().nodes.length;
    const beforeConns = b.getState().connectors.length;
    const ok = window.__EASYOFFICE_NATIVE_MENU__("edit:cut");
    const afterNode = b.getState().nodes.find((n) => n.id === n1.id);
    return {
      ready: true,
      ok,
      beforeNodes,
      afterNodes: b.getState().nodes.length,
      beforeConns,
      afterConns: b.getState().connectors.length,
      domText: editable.textContent,
      modelText: afterNode?.text,
    };
  })()`);
  assert.equal(boardCut.ready, true);
  assert.equal(boardCut.ok, true);
  assert.equal(boardCut.afterNodes, boardCut.beforeNodes, "BOARD node must survive native edit:cut during text edit");
  assert.equal(boardCut.afterConns, boardCut.beforeConns, "BOARD connector must survive native edit:cut during text edit");
  assert.equal(boardCut.domText, "Hello");
  assert.equal(boardCut.modelText, "Hello");

  const boardShapeCut = await js(`(() => {
    const b = window.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
    const shape = b.getState().nodes.find((n) => n.kind === "shape" && n.text === "KeepMe");
    const card = document.querySelector('#board-node-layer .board-node[data-node-id="' + shape.id + '"]');
    card.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    const editable = card.querySelector('.board-node-label[contenteditable="plaintext-only"]');
    if (!editable || !editable.firstChild) return { ready: false };
    const sel = window.getSelection();
    const r = document.createRange();
    r.setStart(editable.firstChild, 4);
    r.setEnd(editable.firstChild, 6);
    sel.removeAllRanges();
    sel.addRange(r);
    const beforeNodes = b.getState().nodes.length;
    const ok = window.__EASYOFFICE_NATIVE_MENU__("edit:cut");
    return { ready: true, ok, beforeNodes, afterNodes: b.getState().nodes.length, text: b.getState().nodes.find((n) => n.id === shape.id)?.text };
  })()`);
  assert.equal(boardShapeCut.ready, true);
  assert.equal(boardShapeCut.ok, true);
  assert.equal(boardShapeCut.afterNodes, boardShapeCut.beforeNodes);
  assert.equal(boardShapeCut.text, "Keep");

  // Real Chrome regression: native edit:cut while editing SLIDE textbox contenteditable cuts only text selection and preserves textbox
  await openModule("SLIDE");
  await waitFor(`Boolean(window.__EASYOFFICE_SLIDE_CANVAS__)`);
  const slideCut = await js(`(() => {
    const c = window.__EASYOFFICE_SLIDE_CANVAS__;
    while (c.getElements().length > 0) c.deleteElement(c.getElements()[0].id);
    const tb = c.addElement({ kind: "textbox", x: 60, y: 60, w: 200, h: 70, text: "AlphaBeta" });
    const card = document.querySelector('#slide-canvas-layer .slide-el[data-element-id="' + tb.id + '"]');
    card.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    const editable = card.querySelector('.slide-el-textbox[contenteditable="plaintext-only"]');
    if (!editable || !editable.firstChild) return { ready: false };
    const sel = window.getSelection();
    const r = document.createRange();
    r.setStart(editable.firstChild, 5);
    r.setEnd(editable.firstChild, 9);
    sel.removeAllRanges();
    sel.addRange(r);
    const ok = window.__EASYOFFICE_NATIVE_MENU__("edit:cut");
    const afterEl = c.getElements().find((e) => e.id === tb.id);
    return {
      ready: true,
      ok,
      count: c.getElements().length,
      domText: editable.textContent,
      modelText: afterEl?.text,
    };
  })()`);
  assert.equal(slideCut.ready, true);
  assert.equal(slideCut.ok, true);
  assert.equal(slideCut.count, 1, "SLIDE textbox must survive native edit:cut during text edit");
  assert.equal(slideCut.domText, "Alpha");
  assert.equal(slideCut.modelText, "Alpha");

  const slideTitleCut = await js(`(() => {
    const c = window.__EASYOFFICE_SLIDE_CANVAS__;
    const sh = c.addElement({ kind: "shape", x: 300, y: 120, w: 120, h: 60, text: "Shape" });
    c.selectElement(sh.id);
    const tz = document.getElementById("slide-title-zone");
    const h2 = tz.querySelector("h2") || tz;
    h2.textContent = "SlideTitle";
    tz.focus();
    const sel = window.getSelection();
    const r = document.createRange();
    r.setStart(h2.firstChild, 5);
    r.setEnd(h2.firstChild, 10);
    sel.removeAllRanges();
    sel.addRange(r);
    const beforeCount = c.getElements().length;
    const ok = window.__EASYOFFICE_NATIVE_MENU__("edit:cut");
    return { ok, beforeCount, afterCount: c.getElements().length, titleText: tz.textContent };
  })()`);
  assert.equal(slideTitleCut.ok, true);
  assert.equal(slideTitleCut.afterCount, slideTitleCut.beforeCount, "SLIDE shape must survive native edit:cut in #slide-title-zone");
  assert.equal(slideTitleCut.titleText, "Slide");

  // Collapsed caret edit:cut in BOARD contenteditable and <input> must NOT delete characters before caret; selection <input>/<textarea> cuts cleanly
  await openModule("BOARD");
  await waitFor(`Boolean(window.__EASYOFFICE_BASE_BOARD_HOST__?.getBoardStudio?.())`);
  const collapsedAndInput = await js(`(() => {
    const b = window.__EASYOFFICE_BASE_BOARD_HOST__.getBoardStudio();
    const n1 = b.addNode({ kind: "text", x: 100, y: 100, w: 160, h: 70, text: "HelloWorld" });
    const card = document.querySelector('#board-node-layer .board-node[data-node-id="' + n1.id + '"]');
    card.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    const editable = card.querySelector('.board-node-text[contenteditable="plaintext-only"]');
    const sel = window.getSelection();
    const r = document.createRange();
    r.setStart(editable.firstChild, 5);
    r.collapse(true);
    sel.removeAllRanges();
    sel.addRange(r);
    window.__EASYOFFICE_NATIVE_MENU__("edit:cut");
    const afterCollapsedCe = b.getState().nodes.find((n) => n.id === n1.id)?.text;

    const inp = document.getElementById("board-style-label");
    inp.value = "LabelText";
    inp.focus();
    inp.setSelectionRange(5, 5);
    window.__EASYOFFICE_NATIVE_MENU__("edit:cut");
    const afterCollapsedInp = inp.value;

    inp.setSelectionRange(5, 9);
    window.__EASYOFFICE_NATIVE_MENU__("edit:cut");
    const afterSelectedInp = inp.value;
    return { afterCollapsedCe, afterCollapsedInp, afterSelectedInp };
  })()`);
  assert.equal(collapsedAndInput.afterCollapsedCe, "HelloWorld", "collapsed cut in BOARD contenteditable must not delete char");
  assert.equal(collapsedAndInput.afterCollapsedInp, "LabelText", "collapsed cut in <input> must not delete char");
  assert.equal(collapsedAndInput.afterSelectedInp, "Label", "selection cut in <input> cuts selected slice");

  // Unstyled <td>Line1<br>Line2</td> full roundtrip (DOC -> DOCX -> reopen -> collectDocBlocks) preserves \n
  await openModule("DOC");
  await waitFor(`Boolean(window.__EASYOFFICE_SAVE_PAYLOAD__ && window.__EASYOFFICE_WORKSPACE_API__)`);
  const brRt = await js(`(async () => {
    const ed = document.getElementById("doc-editor");
    ed.innerHTML = '<table class="doc-inline-table"><tbody><tr><td>Line1<br>Line2</td></tr></tbody></table>';
    const b1 = window.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(ed, {});
    const saved = await fetch("/api/ipc", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ command: "convert_document_format", payload: JSON.stringify({ targetFormat: "docx", title: "brrt", blocks: b1 }) }),
    }).then((r) => r.json());
    const bytes = window.__EASYOFFICE_SHEET_UTILS__.base64ToBytes(saved.convertedBase64);
    await window.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes("brrt.docx", bytes);
    const tdHtml = document.querySelector("#doc-editor td")?.innerHTML;
    const b2 = window.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(ed, {});
    return { tdHtml, t1: b1.find((b) => b.type === "table")?.rows?.[0]?.[0]?.text, t2: b2.find((b) => b.type === "table")?.rows?.[0]?.[0]?.text };
  })()`);
  assert.equal(brRt.t1, "Line1\nLine2");
  assert.equal(brRt.tdHtml, "Line1<br>Line2");
  assert.equal(brRt.t2, "Line1\nLine2");
});
