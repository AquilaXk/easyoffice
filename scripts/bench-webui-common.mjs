// Shell-level scenarios of scripts/bench-webui.mjs: menus, status bar, selector scans, listener growth.
import { click, press, settle } from "./bench-webui-actions.mjs";

export async function run(ctx) {
  const { fresh, record, measure, sleep, frames, probeReset, probeRead } = ctx;
  const cdp = await fresh("DOC");
  await sleep(500);
  console.log("       DOM nodes at start:", await cdp.evalJs(`document.getElementsByTagName("*").length`));
  const titles = await cdp.evalJs(`JSON.stringify([...document.querySelectorAll(".menu-item[data-menu] .menu-title")].map((e) => e.dataset.menuCat))`);
  const cats = JSON.parse(titles);
  await cdp.evalJs(probeReset);
  let m = await measure(cdp, async () => {
    for (const cat of cats) {
      await click(cdp, `.menu-title[data-menu-cat="${cat}"]`);
      await settle(cdp, frames, 2);
    }
  });
  record("common", `open each of ${cats.length} menus in turn (per menu below)`, m, { note: `per menu ${(m.wallMs / cats.length).toFixed(1)} ms ${await cdp.evalJs(probeRead)}` });
  await press(cdp, "Escape");
  // The same menu opened and closed repeatedly: listeners and nodes must not grow.
  const counters = async () => { await cdp.evalJs(`(async () => { window.gc?.(); })()`); await sleep(200); return (await cdp.send("Memory.getDOMCounters")).result; };
  const before = (await counters());
  await cdp.evalJs(probeReset);
  for (let i = 0; i < 30; i += 1) {
    await click(cdp, `.menu-title[data-menu-cat="${cats[0]}"]`);
    await settle(cdp, frames, 1);
    await press(cdp, "Escape");
  }
  const after = (await counters());
  record("common", "open and close one menu 30 times", { wallMs: 0, taskMs: 0, scriptMs: 0, layoutMs: 0, styleMs: 0, layouts: 0, recalcs: 0, nodes: after.nodes, heapMB: 0 }, { note: `listeners ${before.jsEventListeners} -> ${after.jsEventListeners}, nodes ${before.nodes} -> ${after.nodes}, probe ${await cdp.evalJs(probeRead)}` });
  // Dialogs: open and close the find dialog repeatedly (listener growth).
  const b2 = await counters();
  for (let i = 0; i < 20; i += 1) {
    await cdp.evalJs(`document.getElementById("doc-editor").focus()`);
    await press(cdp, "f", 2);
    await sleep(30);
    await press(cdp, "Escape");
  }
  const a2 = await counters();
  record("common", "open and close the find dialog 20 times", { wallMs: 0, taskMs: 0, scriptMs: 0, layoutMs: 0, styleMs: 0, layouts: 0, recalcs: 0, nodes: a2.nodes, heapMB: 0 }, { note: `listeners ${b2.jsEventListeners} -> ${a2.jsEventListeners}` });
  // Status bar and zoom: the zoom slider re-lays out the page.
  m = await measure(cdp, async () => { await cdp.evalJs(`(() => { const s = document.getElementById("status-zoom-slider"); s.value = 150; s.dispatchEvent(new Event("input")); })()`); await settle(cdp, frames); });
  record("common", "zoom to 150% (status bar and layout)", m);
}
