import test from "node:test";
import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { readText } from "./test-support/read-text.mjs";
import { studioItem, switchModule } from "./test-support/modules.mjs";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const CONTENT_CSS = readText(path.join(REPO, "crates/easyoffice-app/ui/content.css"));
const HWPX_PARTS_RS = readText(path.join(REPO, "crates/easyoffice-formats/src/hwpx_parts.rs"));

test("HWP mode CSS sets 함초롬바탕, 1.6 line-height, zero para margin, and 30/20/15mm padding while keeping DOC isolated", () => {
  const docRule = /#doc-editor\s*\{([^}]*padding:\s*113\.4px 96px 96px[^}]*)\}/.exec(CONTENT_CSS)?.[1] || "";
  assert.match(docRule, /font-family:\s*'맑은 고딕'/);
  assert.match(docRule, /line-height:\s*1\.08;/);

  const hwpRule =
    /\.app-shell\[data-module="HWP"\] #doc-editor\s*\{([^}]*)\}/.exec(CONTENT_CSS)?.[1] || "";
  assert.ok(hwpRule, "expected .app-shell[data-module='HWP'] #doc-editor rule");
  assert.match(hwpRule, /padding:\s*132\.27px 113\.4px 113\.4px;/);
  assert.match(hwpRule, /font-family:\s*'함초롬바탕'/);
  assert.match(hwpRule, /line-height:\s*1\.6;/);

  const hwpParaRule =
    /\.app-shell\[data-module="HWP"\] #doc-editor > p:not\(\.manuscript-notice\)\s*\{([^}]*)\}/.exec(
      CONTENT_CSS
    )?.[1] || "";
  assert.match(hwpParaRule, /margin:\s*0;/);
});

test("switching between DOC and HWP updates fallback font, line spacing, and 편집 용지 margins independently", async () => {
  const h = await createWorkspaceHarness({ search: "?module=DOC&win=w-pd311-switch" });
  await flushAsyncTurns(6);
  const { document } = h;

  const fontSelect = document.getElementById("doc-font-select");
  const fontBadge = document.getElementById("doc-style-font-badge");
  const sizeInput = document.getElementById("doc-font-size-input");
  const lsSelect = document.getElementById("doc-linespacing-select");
  const field = (id) => document.getElementById("doc-ps-" + id).value;
  const closePaperDialog = () => document.getElementById("btn-doc-ps-close")?.click();

  assert.equal(fontSelect.value, "맑은 고딕");
  assert.equal(lsSelect.value, "108");

  switchModule(document, "HWP");
  await flushAsyncTurns(6);

  assert.equal(document.getElementById("app").dataset.module, "HWP");
  assert.equal(fontSelect.value, "함초롬바탕");
  assert.equal(fontBadge.textContent, "함초롬바탕");
  assert.equal(sizeInput.value, "10.0");
  assert.equal(lsSelect.value, "160");
  assert.equal(document.getElementById("btn-align-justify").classList.contains("active"), true);

  h.userClick(studioItem(document, "layout-paper"));
  await flushAsyncTurns(4);
  assert.deepEqual(
    {
      paper: field("paper"),
      width: field("width"),
      height: field("height"),
      left: field("left"),
      right: field("right"),
      top: field("top"),
      header: field("header"),
      bottom: field("bottom"),
      footer: field("footer"),
    },
    {
      paper: "A4",
      width: "210.0",
      height: "297.0",
      left: "30.0",
      right: "30.0",
      top: "20.0",
      header: "15.0",
      bottom: "15.0",
      footer: "15.0",
    }
  );
  closePaperDialog();

  switchModule(document, "DOC");
  await flushAsyncTurns(6);
  assert.equal(fontSelect.value, "맑은 고딕");
  assert.equal(fontBadge.textContent, "맑은 고딕");
  assert.equal(lsSelect.value, "108");

  h.userClick(studioItem(document, "layout-paper"));
  await flushAsyncTurns(4);
  assert.deepEqual(
    {
      left: field("left"),
      right: field("right"),
      top: field("top"),
      header: field("header"),
      bottom: field("bottom"),
      footer: field("footer"),
    },
    {
      left: "25.4",
      right: "25.4",
      top: "15.0",
      header: "15.0",
      bottom: "7.9",
      footer: "17.5",
    }
  );
  closePaperDialog();
});

test("HWP_DEFAULT_SIDE, fromDecoded, and HWPX engine constants match Korean HWP baseline", async () => {
  const h = await createWorkspaceHarness({ search: "?module=HWP&win=w-pd311-core" });
  await flushAsyncTurns(6);
  const core = h.host.__EASYOFFICE_DOC_PAGE_SETUP_CORE__;

  assert.deepEqual(core.HWP_DEFAULT_SIDE, {
    top: 1984,
    bottom: 1701,
    left: 1701,
    right: 1701,
    header: 1134,
    footer: 850,
  });

  const fields = core.toFields({ width: 11906, height: 16838, ...core.HWP_DEFAULT_SIDE });
  assert.deepEqual(fields, {
    width: 210,
    height: 297,
    left: 30,
    right: 30,
    top: 20,
    header: 15,
    bottom: 15,
    footer: 15,
    gutter: 0,
  });

  const fromHwp = core.fromDecoded({ paper: { width: 11906, height: 16838 } }, "HWP");
  const fromDoc = core.fromDecoded({ paper: { width: 11906, height: 16838 } }, "DOC");
  assert.equal(fromHwp.left, 1701);
  assert.equal(fromHwp.top, 1984);
  assert.equal(fromDoc.left, 1440);
  assert.equal(fromDoc.top, 1701);

  assert.match(HWPX_PARTS_RS, /const PAGE_WIDTH: u32 = 59_528;/);
  assert.match(HWPX_PARTS_RS, /const PAGE_HEIGHT: u32 = 84_188;/);
  assert.match(HWPX_PARTS_RS, /const MARGIN_SIDE: u32 = 8_504;/);
  assert.match(HWPX_PARTS_RS, /const MARGIN_TOP: u32 = 5_668;/);
  assert.match(HWPX_PARTS_RS, /const MARGIN_OTHER: u32 = 4_252;/);
  assert.match(HWPX_PARTS_RS, /const DEFAULT_FONT_FACE: &str = "함초롬바탕";/);
  assert.match(HWPX_PARTS_RS, /const DEFAULT_CHAR_HEIGHT: u32 = 1000;/);
  assert.match(HWPX_PARTS_RS, /const DEFAULT_LINE_PERCENT: u32 = 160;/);
});
