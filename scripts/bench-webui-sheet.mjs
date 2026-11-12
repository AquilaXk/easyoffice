// Sheet scenarios of scripts/bench-webui.mjs.
import { click, press, typeChar, stageFile, openStaged, runCommand, settle } from "./bench-webui-actions.mjs";

export async function run(ctx) {
  const { fresh, record, measure, sleep, frames, probeReset, probeRead, files, args, maybeProfile } = ctx;
  const cdp = await fresh("SHEET");
  const label = `${args.ROWS}x${args.COLS}`;

  // Open through the real engine: decode, store build, first formula evaluation, first window.
  await stageFile(cdp, files.sheetFile);
  let m = await measure(cdp, () => openStaged(cdp, "sheet.xlsx", frames));
  const opened = JSON.parse(m.value);
  record("sheet", `open xlsx ${label} (decode + first draw)`, m, { note: opened.ok ? "" : "FAILED " + JSON.stringify(opened).slice(0, 120) });
  if (!opened.ok) return;
  await sleep(500);
  const tds = await cdp.evalJs(`document.querySelectorAll("#pane-sheet td[data-cell]").length`);
  console.log("       window tds:", tds);

  // Scroll: 30 wheel-sized steps, then one long jump.
  await cdp.evalJs(probeReset);
  m = await measure(cdp, () => cdp.evalJs(`(async () => { const vp = document.getElementById("sheet-viewport"); for (let i = 0; i < 30; i += 1) { vp.scrollTop += 240; await ${frames(1)}; } return vp.scrollTop; })()`));
  record("sheet", "scroll 30 steps of 10 rows (per step below)", m, { note: `per step ${(m.wallMs / 30).toFixed(1)} ms ${await cdp.evalJs(probeRead)}` });
  m = await measure(cdp, () => cdp.evalJs(`(async () => { const vp = document.getElementById("sheet-viewport"); vp.scrollTop = 60000 * 24; await ${frames(2)}; return vp.scrollTop; })()`));
  record("sheet", "scroll jump to row 60,000", m);
  await cdp.evalJs(`document.getElementById("sheet-viewport").scrollTop = 0`);
  await sleep(300);

  // Edit one cell: click, type, Enter; wait until the recalculation answer is in and painted.
  const editCell = async (ref, text) => {
    await click(cdp, `#pane-sheet td[data-cell="${ref}"]`);
    await cdp.evalJs(probeReset);
    return measure(cdp, async () => {
      for (const ch of text) await typeChar(cdp, ch);
      const t0 = await cdp.evalJs("performance.now()");
      await press(cdp, "Enter");
      await settle(cdp, frames);
      const probe = JSON.parse(await cdp.evalJs(probeRead));
      return { settledMs: probe.ipcEnd ? probe.ipcEnd - t0 : 0, ...probe };
    });
  };
  for (let i = 0; i < 2; i += 1) {
    m = await editCell("A5", "9" + i);
    record("sheet", `edit one cell, Enter (${i === 0 ? "first" : "again"})`, m, { note: `answer in ${m.value.settledMs.toFixed(0)} ms, ipc ${m.value.ipcCalls}, qsa ${m.value.qsa}/${m.value.qsaNodes}` });
  }
  m = await editCell("C5", "7");
  record("sheet", "edit a cell a formula column reads", m, { note: `answer in ${m.value.settledMs.toFixed(0)} ms` });

  const toTop = async () => { await cdp.evalJs(`document.getElementById("sheet-viewport").scrollTop = 0`); await settle(cdp, frames); await sleep(200); };
  // Selection: Ctrl+A takes the aggregate over the whole used range; Shift+Arrow moves one edge.
  await click(cdp, `#pane-sheet td[data-cell="A1"]`);
  await cdp.evalJs(probeReset);
  m = await measure(cdp, async () => { await press(cdp, "a", 2); await settle(cdp, frames); return cdp.evalJs(`document.getElementById("status-sheet-agg").textContent`); });
  record("sheet", "Ctrl+A over the used range (aggregate)", m, { note: String(m.value).slice(0, 50) });
  await click(cdp, `#pane-sheet td[data-cell="A1"]`);
  await press(cdp, "ArrowDown", 8);
  await press(cdp, "ArrowDown", 8);
  m = await measure(cdp, async () => { for (let i = 0; i < 5; i += 1) { await press(cdp, "ArrowDown", 8); await settle(cdp, frames, 1); } });
  record("sheet", "Shift+ArrowDown x5 from A1 (per key below)", m, { note: `per key ${(m.wallMs / 5).toFixed(1)} ms` });
  await cdp.evalJs(probeReset);
  m = await measure(cdp, async () => { await press(cdp, "ArrowDown", 10); await settle(cdp, frames); });
  record("sheet", "Ctrl+Shift+ArrowDown from A1 (down to last row)", m);

  await toTop();
  // Sort: no selection = every column below the header, keyed on B.
  await click(cdp, `#pane-sheet td[data-cell="A1"]`);
  m = await measure(cdp, async () => { await runCommand(cdp, "SHEET", "sheet-data-sort"); await sleep(300); await settle(cdp, frames); await cdp.evalJs(`window.__EASYOFFICE_WORKSPACE_API__.whenSettled?.()`); });
  record("sheet", "sort (descending, whole data range)", m);
  m = await measure(cdp, async () => { await runCommand(cdp, "SHEET", "sheet-data-sort"); await sleep(300); await settle(cdp, frames); });
  record("sheet", "sort (ascending, whole data range)", m);

  await toTop();
  // Conditional format: a rule over the whole used range; the engine answers with every formatted cell.
  const lastCol = String.fromCharCode(64 + Math.min(args.COLS, 26));
  const cf = JSON.stringify({ cf: [{ r: `A1:${lastCol}${args.ROWS}`, t: "cellIs", op: "greaterThan", a: "500000", bg: "#ffeeaa", fl: 1 }], dv: [] });
  await cdp.evalJs(probeReset);
  m = await measure(cdp, async () => { await cdp.evalJs(`window.__EASYOFFICE_SHEET_RULES__.set("Sheet1", ${cf})`); await sleep(400); await settle(cdp, frames); return cdp.evalJs(probeRead); });
  record("sheet", "conditional format rule over all cells (apply + repaint)", m, { note: m.value.slice(0, 80) });
  m = await measure(cdp, () => cdp.evalJs(`(async () => { const vp = document.getElementById("sheet-viewport"); for (let i = 0; i < 10; i += 1) { vp.scrollTop += 480; await ${frames(1)}; } return document.querySelectorAll("#pane-sheet td[style*=background]").length; })()`));
  record("sheet", "scroll 10 steps of 20 rows with the rule (per step below)", m, { note: `per step ${(m.wallMs / 10).toFixed(1)} ms, painted cells ${m.value}` });
  await toTop();
  await click(cdp, `#pane-sheet td[data-cell="A5"]`);
  m = await measure(cdp, async () => { await press(cdp, "z", 2); await sleep(300); await settle(cdp, frames); });
  record("sheet", "Ctrl+Z (undo the last change)", m);
}
