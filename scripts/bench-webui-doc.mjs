// Document and slide scenarios of scripts/bench-webui.mjs.
import { click, press, insertText, stageFile, openStaged, settle } from "./bench-webui-actions.mjs";

export async function run(ctx) {
  const { fresh, record, measure, sleep, frames, probeReset, probeRead, files, args } = ctx;
  const cdp = await fresh("DOC");
  await stageFile(cdp, files.docFile);
  let m = await measure(cdp, () => openStaged(cdp, "report.docx", frames));
  const opened = JSON.parse(m.value);
  record("doc", `open docx ${args.PARAS} paragraphs (decode + first draw)`, m, { note: opened.ok ? "" : "FAILED " + JSON.stringify(opened).slice(0, 120) });
  if (!opened.ok) return;
  await sleep(2500);
  const stats = JSON.parse(await cdp.evalJs(`JSON.stringify({ blocks: document.getElementById("doc-editor").children.length, height: document.getElementById("doc-editor").scrollHeight })`));
  console.log(`       editor blocks ${stats.blocks}, height ${stats.height}px (~${Math.round(stats.height / 1123)} A4 pages)`);

  // Scroll through the document.
  m = await measure(cdp, () => cdp.evalJs(`(async () => { const s = document.getElementById("doc-editor").closest(".editor-surface") || document.scrollingElement; const el = [document.getElementById("doc-editor"), ...document.querySelectorAll(".doc-page, .editor-surface, #pane-doc")].find((e) => e && e.scrollHeight > e.clientHeight + 10) || document.scrollingElement; for (let i = 0; i < 30; i += 1) { el.scrollTop += 600; await ${frames(1)}; } return el.scrollTop; })()`));
  record("doc", "scroll 30 steps of 600 px (per step below)", m, { note: `per step ${(m.wallMs / 30).toFixed(1)} ms` });

  // Typing in the middle of the document: one character at a time, then the deferred work.
  await cdp.evalJs(`(() => { const ed = document.getElementById("doc-editor"); const p = ed.children[Math.floor(ed.children.length / 2)]; p.scrollIntoView({ block: "center" }); const r = document.createRange(); const t = p.firstChild.nodeType === 3 ? p.firstChild : p.firstChild.firstChild; r.setStart(t, 3); r.collapse(true); ed.focus(); const s = getSelection(); s.removeAllRanges(); s.addRange(r); })()`);
  await sleep(500);
  await cdp.evalJs(probeReset);
  const per = [];
  for (let i = 0; i < 20; i += 1) {
    const one = await measure(cdp, async () => { await insertText(cdp, "가"); await settle(cdp, frames, 1); });
    per.push(one);
  }
  const sum = (k) => per.reduce((a, x) => a + x[k], 0) / per.length;
  record("doc", "type 1 character mid-document (mean of 20)", { wallMs: sum("wallMs"), taskMs: sum("taskMs"), scriptMs: sum("scriptMs"), layoutMs: sum("layoutMs"), styleMs: sum("styleMs"), layouts: sum("layouts"), recalcs: sum("recalcs"), nodes: per.at(-1).nodes, heapMB: per.at(-1).heapMB }, { note: await cdp.evalJs(probeRead) });
  // The page's own handlers for one typed character, without the browser's frame waits (execCommand runs them synchronously).
  const handlers = await cdp.evalJs(`(() => { const t = []; for (let i = 0; i < 12; i += 1) { const a = performance.now(); document.execCommand("insertText", false, "나"); t.push(performance.now() - a); } t.shift(); return t.reduce((x, y) => x + y, 0) / t.length; })()`);
  record("doc", "type 1 character: handlers only (mean of 11, sync)", { wallMs: handlers, taskMs: handlers, scriptMs: handlers, layoutMs: 0, styleMs: 0, layouts: 0, recalcs: 0, nodes: per.at(-1).nodes, heapMB: per.at(-1).heapMB });
  m = await measure(cdp, () => sleep(3000));
  record("doc", "autosave and deferred work after typing (3 s window)", m);

  // Find and replace.
  await cdp.evalJs(`document.getElementById("doc-editor").focus()`);
  await press(cdp, "f", 2);
  await sleep(200);
  await cdp.evalJs(probeReset);
  m = await measure(cdp, async () => { await insertText(cdp, "보고"); await sleep(300); await settle(cdp, frames); return cdp.evalJs(`document.getElementById("find-count").textContent + " " + document.getElementById("find-status").textContent`); });
  record("doc", "find: type a 2-character query", m, { note: m.value });
  m = await measure(cdp, async () => { await press(cdp, "Enter"); await settle(cdp, frames); });
  record("doc", "find: next match", m);
  await press(cdp, "Escape");
  await cdp.evalJs(`document.getElementById("doc-editor").focus()`);
  await press(cdp, "F2", 2);
  await sleep(200);
  m = await measure(cdp, async () => {
    await cdp.evalJs(`(() => { document.getElementById("find-query").value = "계획"; document.getElementById("find-query").dispatchEvent(new Event("input")); })()`);
    await sleep(200);
    const rep = await cdp.evalJs(`(() => { const r = document.getElementById("find-replacement"); const row = document.getElementById("find-replace-row"); if (row.hidden) return "no replace row"; r.value = "계획안"; return "ok"; })()`);
    if (rep === "ok") { await cdp.evalJs(`document.getElementById("btn-find-replace-all").click()`); await sleep(500); }
    await settle(cdp, frames);
    return rep;
  });
  record("doc", "replace all (query 계획)", m, { note: String(m.value) });
}

export async function runSlides(ctx) {
  const { fresh, record, measure, sleep, frames, files, args } = ctx;
  const cdp = await fresh("SLIDE");
  await stageFile(cdp, files.slideFile);
  let m = await measure(cdp, () => openStaged(cdp, "deck.pptx", frames));
  const opened = JSON.parse(m.value);
  record("slide", `open pptx ${args.SLIDES} slides (decode + thumbnails)`, m, { note: opened.ok ? "" : "FAILED " + JSON.stringify(opened).slice(0, 120) });
  if (!opened.ok) return;
  await sleep(1500);
  console.log("       thumbnails:", await cdp.evalJs(`document.querySelectorAll("#slide-thumb-list .slide-thumb").length`));
  const far = Math.floor(args.SLIDES / 2);
  for (const idx of [far, 10]) {
    await cdp.evalJs(`document.getElementById("slide-thumb-list").children[${idx}].scrollIntoView({ block: "center" })`);
    await click(cdp, `#slide-thumb-list [data-slide-idx="${idx}"]`);
    await sleep(100);
  }
  m = await measure(cdp, async () => { await click(cdp, `#slide-thumb-list [data-slide-idx="12"]`); await settle(cdp, frames); });
  record("slide", "select a slide from the thumbnail list", m);
  await click(cdp, "#slide-title-zone");
  const per = [];
  for (let i = 0; i < 15; i += 1) per.push(await measure(cdp, async () => { await insertText(cdp, "가"); await settle(cdp, frames, 1); }));
  const mean = (k) => per.reduce((a, x) => a + x[k], 0) / per.length;
  record("slide", "type 1 character in the title (mean of 15)", { wallMs: mean("wallMs"), taskMs: mean("taskMs"), scriptMs: mean("scriptMs"), layoutMs: mean("layoutMs"), styleMs: mean("styleMs"), layouts: mean("layouts"), recalcs: mean("recalcs"), nodes: per.at(-1).nodes, heapMB: per.at(-1).heapMB });
  m = await measure(cdp, () => sleep(1500));
  record("slide", "deferred thumbnail refresh and autosave (1.5 s window)", m);
  m = await measure(cdp, async () => { await cdp.evalJs(`document.getElementById("btn-add-slide").click()`); await settle(cdp, frames); });
  record("slide", "add a slide", m);
}
