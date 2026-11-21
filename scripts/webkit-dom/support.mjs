// Support for scripts/webkit-dom/run.mjs (PD-281): the WebUI in a real WebKit (the engine of the macOS app's web view)
// against the real engine. `easyoffice-app --serve` serves the page and answers the IPC; the browser is driven by
// `playwright-core`, which is not a dependency of the repository: the CI job installs the pinned version into a
// directory of its own and `EO_PW_DIR` points at it (DEVELOPMENT.md has the local command). Without the engine binary,
// the package or the browser the run fails: this suite is a gate of its own job, not an optional test.
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

export const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const STEP_TIMEOUT_MS = 8000;
const exe =process.platform === "win32" ? "easyoffice-app.exe" : "easyoffice-app";

/** The engine binary: `EO_APP`, else the debug build of `target-193` or `target` in this checkout. */
export function findEngine() {
  const found = [process.env.EO_APP, ...["target-193", "target"].map((dir) => path.join(REPO, dir, "debug", exe))].find((file) => file && fs.existsSync(file));
  if (!found) throw new Error("no easyoffice-app binary: build it (cargo build -p easyoffice-app) or set EO_APP");
  return found;
}

/** `playwright-core` from the directory named by `EO_PW_DIR`. */
export function loadPlaywright() {
  const dir = process.env.EO_PW_DIR;
  if (!dir) throw new Error("EO_PW_DIR is not set: install playwright-core into a directory and point EO_PW_DIR at it");
  return createRequire(path.join(path.resolve(dir), "package.json"))("playwright-core");
}

function freePort() {
  return new Promise((resolve, reject) => {
    const probe = net.createServer();
    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const { port } = probe.address();
      probe.close(() => resolve(port));
    });
  });
}

/** Starts `easyoffice-app --serve` on a free port and waits until it answers; `stop()` ends exactly this process. */
export async function startEngine(app) {
  const port = await freePort();
  const proc = spawn(app, ["--serve", String(port)], { stdio: "ignore" });
  const exited = new Promise((resolve) => proc.once("exit", resolve));
  const stop = async () => {
    if (proc.exitCode === null && proc.signalCode === null) proc.kill();
    const timer = setTimeout(() => proc.kill("SIGKILL"), 3000);
    await exited;
    clearTimeout(timer);
  };
  for (let i = 0; i < 100; i += 1) {
    if (proc.exitCode !== null) throw new Error(`the engine exited with ${proc.exitCode}`);
    try {
      if ((await fetch(`http://127.0.0.1:${port}/`)).ok) return { port, stop };
    } catch { /* still starting */ }
    await sleep(100);
  }
  await stop();
  throw new Error("the engine did not answer");
}

/**
 * Runs the steps in order, each on a fresh page of `module` (a step cannot disturb the next), and keeps going after a
 * failure so one run lists every broken step. A failed step leaves a screenshot, the page text and the console lines in
 * `captureDir`. Returns the number of failed steps.
 */
export async function runSteps({ browser, baseUrl, captureDir, groups }) {
  fs.mkdirSync(captureDir, { recursive: true });
  let failed = 0;
  for (const { module, steps } of groups) {
    for (const [name, body] of steps) {
      const context = await browser.newContext({ viewport: { width: 1100, height: 700 } });
      const page = await context.newPage();
      page.setDefaultTimeout(STEP_TIMEOUT_MS);
      const log = [];
      page.on("console", (msg) => log.push(`[${msg.type()}] ${msg.text()}`));
      page.on("pageerror", (err) => log.push(`[pageerror] ${err.message}`));
      const started = Date.now();
      try {
        await page.goto(`${baseUrl}/?module=${module}&win=webkit-dom`);
        await page.waitForSelector(module === "SHEET" ? "#pane-sheet td[data-cell]" : "#doc-editor");
        await body(page);
        console.log(`ok   ${module.toLowerCase()}: ${name} (${Date.now() - started} ms)`);
      } catch (err) {
        failed += 1;
        console.log(`FAIL ${module.toLowerCase()}: ${name}\n     ${String(err.message).split("\n").join("\n     ")}`);
        const stem = path.join(captureDir, `${module.toLowerCase()}-${name.replace(/[^\w가-힣]+/g, "-")}`);
        await page.screenshot({ path: `${stem}.png` }).catch(() => {});
        const text = await page.evaluate(() => document.body.innerText).catch(() => "");
        fs.writeFileSync(`${stem}.txt`, `${err.stack}\n\n--- console ---\n${log.join("\n")}\n\n--- page text ---\n${text}\n`);
      } finally {
        await context.close();
      }
    }
  }
  return failed;
}

/** Delivers a paste to the focused element with the given clipboard formats; resolves to whether the page took it over. */
export const paste = (page, formats) =>
  page.evaluate((entries) => {
    const data = new DataTransfer();
    for (const [type, value] of entries) data.setData(type, value);
    const event = new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true });
    document.activeElement.dispatchEvent(event);
    return event.defaultPrevented;
  }, Object.entries(formats));

/**
 * The colors of the device pixels `points` (`{ name: [x, y] }`) of a real screenshot of the page, as `{ name: [r, g, b] }`.
 * The screenshot is drawn into a canvas of the page and read back, so what is measured is what the browser painted,
 * not what the style says.
 */
export async function pixelsAt(page, points) {
  const png = (await page.screenshot()).toString("base64");
  return page.evaluate(async ([data, at]) => {
    const image = new Image();
    image.src = `data:image/png;base64,${data}`;
    await image.decode();
    const canvas = document.createElement("canvas");
    canvas.width = image.width;
    canvas.height = image.height;
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    ctx.drawImage(image, 0, 0);
    return Object.fromEntries(Object.entries(at).map(([name, [x, y]]) => [name, [...ctx.getImageData(x, y, 1, 1).data.slice(0, 3)]]));
  }, [png, points]);
}

export const tempCaptureDir =() => fs.mkdtempSync(path.join(os.tmpdir(), "eo-webkit-dom-"));
