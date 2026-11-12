// Number format scenarios of scripts/bench-webui.mjs (--only numfmt): a 5,000 x 20 sheet whose cells all
// carry a number format (three codes), opened, scrolled and re-formatted. On a checkout without number
// formats the engine ignores the formats, so the same run measures the plain sheet.
import fs from "node:fs";
import path from "node:path";
import { click, press, stageFile, openStaged, settle } from "./bench-webui-actions.mjs";

const ROWS = 5000;
const COLS = 20;
const CODES = ["#,##0.00", "0.0%", '"₩"#,##0_);[Red]("₩"#,##0)'];
const colLabel = (c) => String.fromCharCode(65 + c);

function savePayload() {
  const cells = [];
  for (let r = 1; r <= ROWS; r += 1) {
    for (let c = 0; c < COLS; c += 1) {
      cells.push({ ref: `${colLabel(c)}${r}`, raw: `${((r * 131 + c * 7) % 5003) - 2000}.25`, style: { nf: (c % CODES.length) + 1 } });
    }
  }
  return { sourceFormat: "xlsx", targetFormat: "xlsx", title: "bench", sheets: [{ name: "Sheet1", numFmts: CODES, cells }] };
}

export async function run(ctx) {
  const { fresh, record, measure, sleep, frames, probeReset, probeRead, invokeEngine } = ctx;
  const cache = path.join(process.env.BENCH_CACHE || "/tmp", "eo-webui-bench");
  const file = path.join(cache, `numfmt-${ROWS}x${COLS}.xlsx`);
  if (!fs.existsSync(file)) {
    const res = await invokeEngine("convert_document_format", JSON.stringify(savePayload()));
    if (res.status !== "OK") throw new Error("encode failed: " + JSON.stringify(res).slice(0, 200));
    fs.writeFileSync(file, Buffer.from(res.convertedBase64, "base64"));
  }
  const cdp = await fresh("SHEET");
  await stageFile(cdp, file);
  let m = await measure(cdp, () => openStaged(cdp, path.basename(file), frames));
  const opened = JSON.parse(m.value);
  record("numfmt", `open xlsx ${ROWS}x${COLS} with formats (decode + first draw)`, m, { note: opened.ok ? "" : "FAILED" });
  if (!opened.ok) return;
  await sleep(800);
  const sample = await cdp.evalJs(`JSON.stringify(["A1", "B1", "C1"].map((r) => document.querySelector('#pane-sheet td[data-cell="' + r + '"]')?.textContent))`);
  console.log("       first cells:", sample);

  await cdp.evalJs(probeReset);
  m = await measure(cdp, () => cdp.evalJs(`(async () => { const vp = document.getElementById("sheet-viewport"); for (let i = 0; i < 30; i += 1) { vp.scrollTop += 240; await ${frames(1)}; } return vp.scrollTop; })()`));
  record("numfmt", "scroll 30 steps of 10 rows (per step below)", m, { note: `per step ${(m.wallMs / 30).toFixed(1)} ms ${await cdp.evalJs(probeRead)}` });
  m = await measure(cdp, () => cdp.evalJs(`(async () => { const vp = document.getElementById("sheet-viewport"); vp.scrollTop = 3000 * 24; await ${frames(2)}; return vp.scrollTop; })()`));
  record("numfmt", "scroll jump to row 3,000", m);
  await sleep(500);
  const shown = await cdp.evalJs(`JSON.stringify([...document.querySelectorAll('#pane-sheet td[data-cell]')].slice(0, 6).map((td) => td.textContent))`);
  console.log("       window after the jump:", shown);

  // Format the whole used range (Ctrl+A) with the 숫자 preset through the menu action.
  await cdp.evalJs(`document.getElementById("sheet-viewport").scrollTop = 0`);
  await sleep(300);
  await click(cdp, `#pane-sheet td[data-cell="A1"]`);
  await press(cdp, "a", 2);
  await settle(cdp, frames);
  const present = await cdp.evalJs(`Boolean(document.querySelector('[data-sheet-numfmt="number"]'))`);
  if (present) {
    m = await measure(cdp, async () => { await cdp.evalJs(`document.querySelector('[data-sheet-numfmt="number"]').click()`); await sleep(300); await settle(cdp, frames); await cdp.evalJs(`window.__EASYOFFICE_WORKSPACE_API__.whenSettled?.()`); });
    record("numfmt", `apply a preset to all ${ROWS * COLS} cells (model write + repaint)`, m);
    m = await measure(cdp, async () => { await cdp.evalJs(`document.querySelector('[data-sheet-numfmt="decimal-up"]').click()`); await sleep(300); await settle(cdp, frames); });
    record("numfmt", `add a decimal place to all ${ROWS * COLS} cells`, m);
  } else {
    console.log("       (no number format commands on this checkout)");
  }
}
