// The WebUI in a real WebKit (PD-281): `node scripts/webkit-dom/run.mjs [captureDir]`. See support.mjs for what it needs
// (`EO_APP` or a debug build of the engine, `EO_PW_DIR` with playwright-core, the browser in PLAYWRIGHT_BROWSERS_PATH).
// Exit code 0 when every step passes. Captures of failed steps go to `captureDir` (default: a temp directory).
import { docSteps } from "./doc.mjs";
import { sheetSteps } from "./sheet.mjs";
import { findEngine, loadPlaywright, runSteps, startEngine, tempCaptureDir } from "./support.mjs";

const captureDir = process.argv[2] || tempCaptureDir();
const { webkit } = loadPlaywright();
const engine = await startEngine(findEngine());
let browser = null;
let failed = 1;
try {
  browser = await webkit.launch();
  failed = await runSteps({
    browser,
    baseUrl: `http://127.0.0.1:${engine.port}`,
    captureDir,
    groups: [{ module: "SHEET", steps: sheetSteps }, { module: "DOC", steps: docSteps }],
  });
  if (failed) console.log(`${failed} step(s) failed, captures in ${captureDir}`);
} finally {
  await browser?.close();
  await engine.stop();
}
process.exit(failed ? 1 : 0);
