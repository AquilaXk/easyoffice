// The WebUI in a real WebKit (PD-281): `node scripts/webkit-dom/run.mjs [captureDir]`. See support.mjs for what it needs
// (`EO_APP` or a debug build of the engine, `EO_PW_DIR` with playwright-core, the browser in PLAYWRIGHT_BROWSERS_PATH).
// Exit code 0 when every step passes. Captures of failed steps go to `captureDir` (default: a temp directory).
import { baseSteps } from "./base.mjs";
import { copilotDocSteps, copilotSlideSteps } from "./copilot.mjs";
import { docSteps } from "./doc.mjs";
import { sheetSteps } from "./sheet.mjs";
import { findEngine, loadPlaywright, runSteps, startEngine, tempCaptureDir } from "./support.mjs";

const captureDir = process.argv[2] || tempCaptureDir();
const playwright = loadPlaywright();
const engine = await startEngine(findEngine());
let browser = null;
let failed = 1;
try {
  // EO_BROWSER=chromium runs the same steps in Chromium (EO_CHROMIUM: the binary, when the cached build is not the one of the package).
  browser = process.env.EO_BROWSER === "chromium"
    ? await playwright.chromium.launch(process.env.EO_CHROMIUM ? { executablePath: process.env.EO_CHROMIUM } : {})
    : await playwright.webkit.launch();
  failed = await runSteps({
    browser,
    baseUrl: `http://127.0.0.1:${engine.port}`,
    captureDir,
    groups: [{ module: "SHEET", steps: sheetSteps }, { module: "DOC", steps: [...docSteps, ...copilotDocSteps] }, { module: "SLIDE", steps: copilotSlideSteps }, { module: "BASE", steps: baseSteps }],
  });
  if (failed) console.log(`${failed} step(s) failed, captures in ${captureDir}`);
} finally {
  await browser?.close();
  await engine.stop();
}
process.exit(failed ? 1 : 0);
