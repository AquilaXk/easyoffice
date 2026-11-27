import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem, switchModule } from "./test-support/modules.mjs";
import { createSelection, rangeOf } from "./test-support/doc-run-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const readUi = (rel) => fs.readFileSync(path.join(REPO, "crates/easyoffice-app/ui", rel), "utf8");
const readHwp = (rel) => fs.readFileSync(path.join(REPO, "apps/hwp-studio", rel), "utf8");

function parseHexColor(hex) {
  const clean = hex.trim().replace(/^#/, "");
  const full = clean.length === 3 ? clean.replace(/(.)/g, "$1$1") : clean;
  if (full.length !== 6) throw new Error(`Unsupported hex color: ${hex}`);
  return [
    parseInt(full.slice(0, 2), 16),
    parseInt(full.slice(2, 4), 16),
    parseInt(full.slice(4, 6), 16),
  ];
}

function relativeLuminance(hex) {
  const chan = parseHexColor(hex).map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * chan[0] + 0.7152 * chan[1] + 0.0722 * chan[2];
}

function contrastRatio(fg, bg) {
  const l1 = relativeLuminance(fg);
  const l2 = relativeLuminance(bg);
  return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
}

function extractTokens(cssBlock) {
  const map = new Map();
  for (const m of cssBlock.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/gi)) {
    map.set(m[1], m[2].trim());
  }
  const resolve = (val, depth = 0) => {
    if (depth > 8) return val;
    const varMatch = /^var\((--[a-z0-9-]+)\)$/i.exec(val);
    if (varMatch && map.has(varMatch[1])) return resolve(map.get(varMatch[1]), depth + 1);
    return val;
  };
  const resolved = new Map();
  for (const [k, v] of map.entries()) resolved.set(k, resolve(v));
  return resolved;
}

test("PD-361: light and dark tokens satisfy WCAG AA contrast for primary buttons, text, and paper guides", () => {
  const tokensCss = readUi("tokens.css");
  const splitIdx = tokensCss.lastIndexOf(':root[data-theme-effective="dark"]');
  assert.ok(splitIdx > 0, "tokens.css defines dark overrides");
  const light = extractTokens(tokensCss.slice(0, splitIdx));
  const darkVars = extractTokens(tokensCss.slice(splitIdx));
  const dark = new Map(light);
  for (const [k, v] of darkVars.entries()) dark.set(k, v);

  assert.ok(contrastRatio(light.get("--ui-text-on-accent"), light.get("--accent-strong")) >= 4.5, "light primary button contrast >= 4.5:1");
  assert.ok(contrastRatio(dark.get("--ui-bg"), dark.get("--accent-strong")) >= 4.5, "dark primary button contrast >= 4.5:1");
  assert.ok(contrastRatio(light.get("--ui-text-secondary"), light.get("--ui-surface")) >= 4.5, "light secondary text >= 4.5:1");
  assert.ok(contrastRatio(dark.get("--ui-text-secondary"), dark.get("--ui-surface")) >= 4.5, "dark secondary text >= 4.5:1");
  assert.ok(contrastRatio(light.get("--ui-text-muted"), light.get("--ui-surface")) >= 4.5, "light muted text >= 4.5:1");
  assert.ok(contrastRatio(dark.get("--ui-text-muted"), dark.get("--ui-bg-light")) >= 4.5, "dark muted text >= 4.5:1");
  assert.ok(contrastRatio(light.get("--doc-guide-stroke"), light.get("--doc-paper")) >= 4.5, "light guide stroke >= 4.5:1");
  assert.ok(contrastRatio(dark.get("--doc-guide-stroke"), dark.get("--doc-paper")) >= 4.5, "dark guide stroke >= 4.5:1");

  assert.ok(contrastRatio(dark.get("--caret-color"), dark.get("--doc-paper")) >= 4.5, "dark paper ink >= 4.5:1");

  const eoCss = readHwp("src/eo-overrides.css");
  assert.match(eoCss, /\.cs-preview\s*\{[^}]*color:\s*var\(--caret-color\)/);
  assert.match(eoCss, /\.eq-preview\s*\{[^}]*background:\s*var\(--doc-paper\)/);
  assert.match(eoCss, /\.eq-preview-empty\s*\{[^}]*color:\s*var\(--ui-text-secondary\)/);
  assert.match(eoCss, /:root\[data-theme-effective="dark"\]\s+\.eq-preview-empty[\s\S]*?\{[^}]*color:\s*var\(--caret-color\)/);
  assert.match(eoCss, /\.eq-preview-error\s*\{[^}]*color:\s*var\(--eo-paper-danger\)/);
  assert.match(eoCss, /\.pp-dir-spacer\s*\{[^}]*color:\s*var\(--ui-text-secondary\)/);
  assert.match(eoCss, /:root\[data-theme-effective="dark"\]\s+\.cs-icon-btn\s*>\s*span\[style\*="text-stroke"\]\s*\{/);
  assert.match(eoCss, /\.eq-script-error/);
  assert.match(eoCss, /\.bm-empty[\s\S]*?color:\s*var\(--ui-text-muted\)/);
  assert.match(eoCss, /\.pp-wrap-btn:disabled[\s\S]*?opacity:\s*1;[\s\S]*?color:\s*var\(--ui-text-muted\)/);
  assert.match(eoCss, /\.dialog-btn:disabled[\s\S]*?color:\s*var\(--ui-text-muted\)/);
  const hwpSelectCss = readHwp("src/styles/custom-select.css");
  assert.match(hwpSelectCss, /\.custom-select\.disabled\s*\{\s*opacity:\s*1;/);
  assert.match(hwpSelectCss, /\.custom-select\.open\s+\.custom-select-arrow\s*\{[^}]*color:\s*var\(--color-text-secondary\)/);
  const shellCss = readUi("shell.css");
  assert.match(shellCss, /\.sheet-rot-stage\s*\{/);
  assert.match(shellCss, /\.custom-select\.open\s+\.custom-select-menu\.slide-layout-grid-menu\s*\{/);
});

test("PD-361: START and CONV keep a minimal 5-menu chrome and clear docFlavor across transitions", async () => {
  const start = await createWorkspaceHarness({ search: "?module=START&win=w-pd361-start" });
  await flushAsyncTurns(10);
  const visibleMenus = (doc) =>
    doc.querySelectorAll("#menu-bar .menu-item")
      .filter((el) => !el.hidden)
      .map((el) => el.querySelector(".menu-title").textContent);
  assert.deepEqual(visibleMenus(start.document), ["파일", "편집", "보기", "도구", "도움말"]);
  assert.equal(start.document.getElementById("app").dataset.docFlavor, undefined);

  const conv = await createWorkspaceHarness({ search: "?module=CONV&win=w-pd361-conv" });
  await flushAsyncTurns(10);
  assert.deepEqual(visibleMenus(conv.document), ["파일", "편집", "보기", "도구", "도움말"]);

  // Opening .pages activates PAGES inspector + menu bar, and switching to DOC/SLIDE isolates flavor cleanly
  const h = await createWorkspaceHarness({
    search: "?module=DOC&win=w-pd361-pages",
    testIpc: async (cmd, raw) => {
      if (cmd !== "convert_document_format") return null;
      const req = JSON.parse(raw || "{}");
      if (req.action === "decode" && String(req.fileName || "").endsWith(".pages")) {
        return {
          status: "OK",
          module: "DOC",
          format: "pages",
          kind: "blocks",
          itemCount: 1,
          title: "spec.pages",
          blocks: [{ type: "paragraph", heading: 1, runs: [{ text: "개요" }] }],
        };
      }
      return null;
    },
  });
  await flushAsyncTurns(12);
  assert.deepEqual(visibleMenus(h.document), ["파일", "편집", "보기", "삽입", "서식", "레이아웃", "표", "도구", "도움말"]);
  assert.equal(h.document.getElementById("ruler-corner").textContent, "cm");
  assert.equal(h.document.getElementById("btn-align-distribute").hidden, true);
  assert.equal(h.document.getElementById("btn-align-divide").hidden, true);

  await pickFile(h.document, makeFixtureFile("spec.pages", "pages-bytes"));
  await flushAsyncTurns(16);
  const app = h.document.getElementById("app");
  assert.equal(app.dataset.docFlavor, "pages");
  assert.deepEqual(visibleMenus(h.document), ["파일", "편집", "보기", "삽입", "포맷", "정렬", "도구", "도움말"]);
  assert.equal(h.document.getElementById("pages-inspector").hidden, false);

  // The toolbar's 문서 button opens the document sidebar in place of the format sidebar; its button again closes it.
  h.userClick(h.document.querySelector('[data-pages-sidebar-toggle="document"]'));
  assert.equal(h.document.getElementById("pages-document-sidebar").hidden, false);
  assert.equal(h.document.getElementById("pages-format-sidebar").hidden, true);
  assert.equal(h.document.getElementById("pages-inspector-title").textContent, "문서");
  h.userClick(h.document.querySelector('[data-pages-sidebar-toggle="document"]'));
  assert.equal(h.document.getElementById("pages-inspector").hidden, true);
  h.userClick(h.document.querySelector('[data-pages-sidebar-toggle="format"]'));
  assert.equal(h.document.getElementById("pages-inspector").hidden, false);
  assert.equal(h.document.getElementById("pages-inspector-title").textContent, "포맷");

  switchModule(h.document, "HWP");
  await flushAsyncTurns(8);
  assert.equal(app.dataset.docFlavor, undefined);
  assert.equal(h.document.getElementById("pages-inspector").hidden, true);
  assert.equal(h.document.getElementById("ruler-corner").textContent, "mm");
  assert.deepEqual(visibleMenus(h.document), ["파일", "편집", "보기", "입력", "서식", "쪽", "표", "도구", "도움말"]);
});

test("PD-361: SHEET uses spreadsheet labels, BOARD omits duplicate zoom chrome, and toolbar scroller wraps wide toolbars", async () => {
  const sheet = await createWorkspaceHarness({ search: "?module=SHEET&win=w-pd361-sheet" });
  await flushAsyncTurns(10);
  const totalsMod = sheet.host.__EASYOFFICE_SHEET_TABLE_TOTALS__;
  const fnLabels = Object.fromEntries(totalsMod.FUNCTIONS.map(([id, label]) => [id, label]));
  assert.equal(fnLabels.max, "최대");
  assert.equal(fnLabels.min, "최소");
  assert.ok(sheet.document.querySelector(".sheet-rot-stage #sheet-align-rotation-preview"), "rotation preview wrapped in .sheet-rot-stage");

  for (const id of ["icon-toolbar", "doc-style-bar", "sheet-table-bar", "sheet-pivot-bar", "slide-toolbar", "pdf-toolbar", "board-toolbar"]) {
    const bar = sheet.document.getElementById(id);
    assert.ok(bar, `#${id} exists`);
    assert.ok(bar.querySelector(".tb-scroll-viewport .tb-scroll-track"), `#${id} mounts horizontal scroller track`);
    assert.ok(bar.querySelector(".tb-scroll-nav-prev"), `#${id} mounts prev nav`);
    assert.ok(bar.querySelector(".tb-scroll-nav-next"), `#${id} mounts next nav`);
  }
  const base = await createWorkspaceHarness({ search: "?module=BASE&win=w-pd361-base-tb" });
  await flushAsyncTurns(10);
  const baseToolbar = base.document.getElementById("base-toolbar");
  assert.ok(baseToolbar?.querySelector(".tb-scroll-viewport .tb-scroll-track"), "#base-toolbar mounts horizontal scroller track");

  const slide = await createWorkspaceHarness({ search: "?module=SLIDE&win=w-pd361-slide" });
  await flushAsyncTurns(10);
  const layoutMenu = slide.document.querySelector('.custom-select[data-select-id="slide-layout-select"] .custom-select-menu');
  assert.ok(layoutMenu.classList.contains("slide-layout-grid-menu"));
  assert.ok(layoutMenu.querySelectorAll(".slide-layout-thumb").length >= 5, "slide layout dropdown owns visual wireframes");
  assert.equal(slide.document.querySelector('#menu-bar [data-menu="format"] [data-slide-act="bring-front"]')?.hidden, false);
  assert.equal(slide.document.querySelector('#menu-bar [data-menu="slide"] [data-studio-menu-cmd="layout-margins"]'), null);
  const slideTableLabels = [...slide.document.querySelectorAll("#slide-table-dialog .dialog-label")].map((el) => el.textContent.trim());
  assert.deepEqual(slideTableLabels.slice(0, 2), ["행 수", "열 수"]);

  const koOverrides = JSON.parse(fs.readFileSync(path.join(REPO, "config/rhwp-studio-ko-overrides.json"), "utf8"));
  assert.equal(Object.values(koOverrides).includes("옅은 테두리"), true, "rhwp ko overrides patches 옅은 테두리");
  assert.equal(Object.values(koOverrides).includes("열은 테두리"), false, "no 열은 테두리 typo");

  const shellCss = readUi("shell.css");
  const toolbarCss = readUi("toolbar.css");
  const copilotCss = readUi("copilot.css");
  const contentCss = readUi("content.css");
  const hwpCss = readHwp("src/eo-overrides.css");
  assert.match(shellCss, /:root\[data-theme-effective="dark"\] \.eo-glyph-btn\.active[\s\S]*?color:\s*var\(--ui-text\)/);
  assert.match(toolbarCss, /\.tb-btn\.active,\s*\.tb-btn\[aria-pressed="true"\]\s*\{[^}]*color:\s*var\(--ui-text\)/);
  assert.match(toolbarCss, /\.tb-scrollable:has\(>\s*\.tb-scroll-nav-prev:not\(\[hidden\]\)\)/);
  assert.match(toolbarCss, /#slide-toolbar\.tb-scrollable:has\(>\s*\.tb-scroll-nav-prev:not\(\[hidden\]\)\)/);
  assert.match(toolbarCss, /\.custom-select\.open\s+\.custom-select-arrow\s*\{[^}]*color:\s*var\(--color-text-secondary\)/);
  assert.match(toolbarCss, /\.sb-color-host\.open\s+\.sb-color-arrow\s*\{[^}]*color:\s*var\(--color-text-secondary\)/);
  assert.match(toolbarCss, /\.custom-select\.disabled\s*\{\s*opacity:\s*1;/);
  const stylesCss = readUi("styles.css");
  assert.doesNotMatch(stylesCss, /\.slide-thumb-snippet\s*\{[^}]*opacity:/);
  assert.doesNotMatch(stylesCss, /\.slide-thumb-title\.is-empty\s*\{[^}]*opacity:/);
  assert.match(copilotCss, /:root\[data-theme-effective="dark"\] \.eo-copilot-send-btn:not\(:disabled\)/);
  assert.match(contentCss, /#doc-editor\[data-eo-doc-flavor="pages"\]\[data-eo-hf-edit\] > section\[data-eo-hf="header"\]::after/);
  const menuBarCss = readUi("menu-bar.css");
  assert.match(menuBarCss, /\.md-sub\[hidden\]\s*\{\s*display:\s*none;\s*\}/);
  assert.match(hwpCss, /\.stb-message\s*\{\s*color:\s*var\(--accent-strong\)/);
  assert.match(hwpCss, /\.dialog-close\s*\{[\s\S]*?min-width:\s*22px[\s\S]*?min-height:\s*22px/);
  assert.match(hwpCss, /\.eq-mode-btn\s*\{[^}]*min-height:\s*22px/);

  // Non-HWP table/symbol feedback and Pages margin violation terminology checks.
  const selection = createSelection();
  const doc = await createWorkspaceHarness({ selection, search: "?module=DOC&win=w-pd361-doc-labels" });
  await flushAsyncTurns(10);
  const editor = doc.document.getElementById("doc-editor");
  editor.innerHTML = "<table class='doc-inline-table'><tbody><tr><td>A</td><td>B</td></tr></tbody></table>";
  const cells = editor.querySelectorAll("td");
  selection.addRange(rangeOf(cells[0], 0, cells[1], 0));
  doc.document.querySelector('.menu-item[data-menu="table"]').dispatchEvent({ type: "mouseenter" });
  doc.userClick(studioItem(doc.document, "table-col-equal"));
  await flushAsyncTurns(4);
  assert.equal(doc.document.getElementById("status-command-feedback").textContent, "열 너비를 같게 완료");
  selection.addRange(rangeOf(cells[0], 0, cells[1], 0));
  doc.userClick(studioItem(doc.document, "table-merge"));
  await flushAsyncTurns(4);
  assert.equal(doc.document.getElementById("status-command-feedback").textContent, "셀 병합 완료");
  doc.userClick(studioItem(doc.document, "table-split"));
  await flushAsyncTurns(4);
  assert.equal(doc.document.getElementById("status-command-feedback").textContent, "셀 분할 완료");

  const cellA1 = sheet.document.querySelector('[data-cell-ref="A1"]') || sheet.document.querySelector("#sheet-grid td[tabindex]");
  cellA1?.focus?.();
  sheet.userClick(sheet.document.querySelector('.md-item[data-studio-menu-cmd="mod-symbol"]'));
  await flushAsyncTurns(4);
  sheet.userClick(sheet.document.querySelector('#symbol-picker-grid [data-symbol-char="※"]'));
  sheet.userClick(sheet.document.getElementById("btn-symbol-insert"));
  await flushAsyncTurns(6);
  assert.equal(sheet.document.getElementById("status-command-feedback").textContent, "기호 삽입 완료 (※)");

  const docSetup = doc.host.__EASYOFFICE_DOC_PAGE_SETUP_CORE__;
  assert.match(docSetup.violation({ width: 11906, height: 16838, top: 8000, bottom: 9000, left: 1000, right: 1000, header: 500, footer: 500 }, "DOC", true), /머리말·꼬리말/);
  let pageSetupErr = "";
  editor.dataset.eoDocFlavor = "pages";
  editor.dataset.eoPage = "11906,16838,1000,1000,1000,11500,500,500";
  const psApi = doc.host.__EASYOFFICE_CREATE_DOC_PAGE_SETUP__({
    document: doc.document,
    activeMod: () => "DOC",
    setFeedback: (msg) => { pageSetupErr = String(msg || ""); },
  });
  assert.equal(psApi.turn(), false);
  assert.match(pageSetupErr, /머리말·꼬리말/);
});

test("PD-361: SHEET font preview caps oversized pt, fits narrow containers, and numeric inputs keep spinner clearance", async () => {
  const sheet = await createWorkspaceHarness({ search: "?module=SHEET&win=w-pd361-font-preview" });
  await flushAsyncTurns(10);
  sheet.userClick(sheet.document.querySelector('[data-sheet-numfmt="dialog"]'));
  await flushAsyncTurns(4);
  sheet.userClick(sheet.document.getElementById("sheet-fmt-tab-font"));
  await flushAsyncTurns(2);

  const sizeSel = sheet.document.getElementById("sheet-font-size-select");
  const sample = sheet.document.getElementById("sheet-font-sample");
  const chooseSize = async (val) => {
    sizeSel.value = val;
    sizeSel.dispatchEvent({ type: "change", bubbles: true });
    await flushAsyncTurns(2);
  };

  await chooseSize("72");
  assert.equal(sample.textContent, "가나다 AaBb", "font sample keeps full preview text");
  assert.equal(sample.style.getPropertyValue("--sample-size"), "28pt", "72pt font preview is capped inside the 76px box");

  await chooseSize("18");
  assert.equal(sample.style.getPropertyValue("--sample-size"), "18pt", "normal font sizes under the cap keep exact pt");

  await chooseSize("");
  assert.equal(sample.style.getPropertyValue("--sample-size"), "", "default font size clears custom override");

  // Overflow guard scales down further when rendered width/height cannot hold the capped size.
  Object.defineProperties(sample, {
    clientWidth: { configurable: true, get: () => 180 },
    clientHeight: { configurable: true, get: () => 74 },
    scrollWidth: { configurable: true, get: () => 300 },
    scrollHeight: { configurable: true, get: () => 74 },
  });
  await chooseSize("48");
  const fittedPt = Number.parseFloat(sample.style.getPropertyValue("--sample-size"));
  assert.ok(fittedPt >= 8 && fittedPt < 28, `overflow guard scales 48pt down for narrow container (got ${fittedPt}pt)`);

  // Inner text bounding box with padding converges so text width fits inside clientWidth - padX.
  const textSpan = sample.querySelector(".sheet-font-sample-text");
  assert.ok(textSpan, "sheet-font-sample wraps preview text for accurate inner measurement");
  const prevGetCS = sheet.host.getComputedStyle;
  sheet.host.getComputedStyle = () => ({ paddingLeft: "10px", paddingRight: "10px", paddingTop: "4px", paddingBottom: "4px" });
  Object.defineProperties(sample, {
    clientWidth: { configurable: true, get: () => 196 },
    clientHeight: { configurable: true, get: () => 74 },
    scrollWidth: { configurable: true, get: () => 196 },
    scrollHeight: { configurable: true, get: () => 74 },
  });
  textSpan.getBoundingClientRect = () => {
    const pt = Number.parseFloat(sample.style.getPropertyValue("--sample-size")) || 11;
    return { width: pt * 7.5, height: pt * 1.4 };
  };
  try {
    await chooseSize("72");
    const convPt = Number.parseFloat(sample.style.getPropertyValue("--sample-size"));
    assert.ok(convPt >= 8 && convPt * 7.5 <= 174, `72pt Courier-like width converges inside padded inner box (got ${convPt}pt -> ${convPt * 7.5}px <= 174px)`);
  } finally {
    sheet.host.getComputedStyle = prevGetCS;
  }

  // Opening directly on "font" tab re-runs fit check after openDialog makes dimensions non-zero.
  let dlgOpen = false;
  sheet.host.getComputedStyle = () => ({ paddingLeft: "10px", paddingRight: "10px", paddingTop: "4px", paddingBottom: "4px" });
  try {
    const numfmtUi = sheet.host.__EASYOFFICE_CREATE_SHEET_NUMFMT_UI__;
    assert.equal(typeof numfmtUi, "function");
    const kit = sheet.host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(sheet.document);
    let refreshedAfterOpen = false;
    let wasOpenDuringRefresh = false;
    const api = numfmtUi({
      document: sheet.document,
      format: { anchorCell: () => ({ code: "", raw: "" }), formatNumbers: async () => ({}) },
      kit: { ...kit, openDialog: (id) => { dlgOpen = true; kit.openDialog(id); } },
      look: { open: () => { dlgOpen = false; }, refreshFontSample: () => { refreshedAfterOpen = true; wasOpenDuringRefresh = dlgOpen; } },
    });
    api.openNumfmtDialog("font");
    assert.ok(refreshedAfterOpen && wasOpenDuringRefresh, "openNumfmtDialog refreshes font sample after openDialog makes dimensions visible");
  } finally {
    sheet.host.getComputedStyle = prevGetCS;
  }

  const shellCss = readUi("shell.css");
  const toolbarCss = readUi("toolbar.css");
  const hwpCss = readHwp("src/eo-overrides.css");
  assert.match(shellCss, /#sheet-font-sample\s*\{[^}]*justify-content:\s*center/);
  assert.match(shellCss, /\.pages-insp-size\s*\{[^}]*padding:\s*0 4px 0 6px;/);
  assert.match(shellCss, /::-webkit-inner-spin-button[\s\S]*?margin-inline-start:\s*4px/);
  assert.match(toolbarCss, /\.custom-select-value\s*\{[^}]*padding-right:\s*6px;/);
  assert.match(toolbarCss, /\.sb-spacing-input::-webkit-inner-spin-button[\s\S]*?margin-inline-start:\s*4px/);
  assert.match(hwpCss, /\.dialog-wrap input\[type="number"\]::-webkit-inner-spin-button[\s\S]*?margin-inline-start:\s*4px/);
  assert.match(hwpCss, /\.dialog-wrap input\[type="number"\]\.dialog-input:not\(\.pp-sync-spinner\)\s*\{[^}]*min-width:\s*60px;/);
  assert.match(hwpCss, /\.cs-dialog input\[type="number"\]\[step="0\.5"\]\.dialog-input\s*\{[^}]*min-width:\s*72px;/);
  assert.match(hwpCss, /\.cs-dialog input\[type="number"\]\[step="0\.5"\]\.dialog-input\s*\{[^}]*text-align:\s*center;/);
});

test("Official Korean terminology separation holds across DOC, PAGES, SHEET, SLIDE, and HWP", async () => {
  const h = await createWorkspaceHarness({
    search: "?module=DOC&win=w-terms-official",
    testIpc: async (cmd, raw) => {
      if (cmd !== "convert_document_format") return null;
      const req = JSON.parse(raw || "{}");
      if (req.action === "decode" && String(req.fileName || "").endsWith(".pages")) {
        return {
          status: "OK",
          module: "DOC",
          format: "pages",
          kind: "blocks",
          itemCount: 1,
          title: "terms.pages",
          blocks: [{ type: "paragraph", heading: 2, runs: [{ text: "부제 본문" }] }],
        };
      }
      return null;
    },
  });
  await flushAsyncTurns(10);
  const doc = h.document;
  const shownLabel = (cmd) => doc.querySelector(`#menu-bar .md-item[data-studio-menu-cmd="${cmd}"]:not([hidden]) .md-label`)?.textContent?.trim();

  // DOC
  assert.deepEqual(["btn-align-left", "btn-align-center", "btn-align-right", "btn-align-justify"].map((id) => doc.getElementById(id).getAttribute("title")), ["왼쪽 맞춤", "가운데 맞춤", "오른쪽 맞춤", "양쪽 맞춤"]);
  assert.equal(shownLabel("para-shape"), "양쪽 맞춤");
  assert.equal(shownLabel("indent-in"), "들여쓰기 늘림");
  assert.equal(shownLabel("indent-out"), "들여쓰기 줄임");
  assert.equal(shownLabel("ctrl-marks"), "서식 기호");
  assert.equal(shownLabel("para-marks"), "편집 기호");
  assert.equal(shownLabel("doc-grid"), "눈금선");

  // PAGES (Apple Pages macOS Korean)
  await pickFile(doc, makeFixtureFile("terms.pages", "pages-bytes"));
  await flushAsyncTurns(16);
  assert.deepEqual(["btn-align-left", "btn-align-center", "btn-align-right", "btn-align-justify"].map((id) => doc.getElementById(id).getAttribute("title")), ["왼쪽 정렬", "가운데 정렬", "오른쪽 정렬", "좌우 정렬"]);
  assert.equal(doc.querySelector('[data-pages-align-btn="justify"]').getAttribute("title"), "좌우 정렬");
  assert.equal(doc.querySelector('[data-pages-style-btn="bold"]').getAttribute("title"), "볼드체");
  assert.equal(doc.querySelector('[data-pages-style-btn="italic"]').getAttribute("title"), "이탤릭체");
  assert.equal(doc.querySelector('[data-pages-style-btn="underline"]').getAttribute("title"), "밑줄체");
  assert.equal(shownLabel("para-shape"), "좌우 정렬");
  assert.equal(shownLabel("print"), "프린트…");
  assert.equal(shownLabel("studio-redo"), "실행 복귀");
  assert.equal(shownLabel("clip-cut"), "오려두기");
  assert.equal(shownLabel("replace"), "찾기 및 대치");
  assert.equal(shownLabel("para-marks"), "보이지 않는 요소 보기");
  assert.equal(shownLabel("insert-image"), "이미지");
  assert.equal(shownLabel("home-bold"), "볼드체");
  assert.equal(shownLabel("home-italic"), "이탤릭체");
  assert.equal(shownLabel("home-underline"), "밑줄체");
  assert.equal(shownLabel("indent-in"), "들여쓰기 단계 증가");
  assert.equal(shownLabel("indent-out"), "들여쓰기 단계 감소");
  assert.equal(shownLabel("table-row-above"), "위에 행 추가");
  assert.equal(shownLabel("table-add-row"), "아래에 행 추가");
  assert.equal(shownLabel("table-col-left"), "앞에 열 추가");
  assert.equal(shownLabel("table-col-right"), "뒤에 열 추가");
  assert.equal(shownLabel("table-split"), "셀 병합 해제");
  assert.equal(shownLabel("table-col-equal"), "같은 너비로 열 조절");
  assert.match(doc.getElementById("pages-insp-margin-summary").textContent, /상단 .*하단 .*왼쪽 .*오른쪽/);
  h.userClick(doc.getElementById("btn-pages-insp-setup"));
  await flushAsyncTurns(4);
  assert.equal(doc.getElementById("doc-ps-margin-top-label").textContent, "상단");
  assert.equal(doc.getElementById("doc-ps-margin-bottom-label").textContent, "하단");
  assert.equal(doc.getElementById("doc-ps-facing-row").hidden, false);
  assert.match(doc.getElementById("doc-ps-facing-row").textContent, /페이지 마주보기/);
  doc.getElementById("doc-page-setup-dialog").close();

  // HWP
  switchModule(doc, "HWP");
  await flushAsyncTurns(8);
  assert.deepEqual(["btn-align-left", "btn-align-center", "btn-align-right", "btn-align-justify"].map((id) => doc.getElementById(id).getAttribute("title")), ["왼쪽 정렬", "가운데 정렬", "오른쪽 정렬", "양쪽 정렬"]);
  assert.equal(shownLabel("para-shape"), "양쪽 정렬");
  assert.equal(shownLabel("ctrl-marks"), "조판 부호");
  assert.equal(shownLabel("para-marks"), "문단 부호");
  assert.equal(shownLabel("doc-grid"), "격자 설정");

  // SHEET
  switchModule(doc, "SHEET");
  await flushAsyncTurns(8);
  assert.equal(doc.querySelector('[data-sheet-act="freeze-clear"] .md-label').textContent, "틀 고정 취소");
  assert.equal(doc.querySelector('label[for="sheet-align-rotation"]').textContent, "방향");
  assert.match(doc.querySelector('input#sheet-align-wrap').parentElement.textContent, /자동 줄 바꿈/);
  assert.match(doc.querySelector('input#sheet-numfmt-thousands').parentElement.textContent, /천 단위 구분 기호 사용/);
  h.userClick(doc.querySelector('[data-sheet-numfmt="dialog"]'));
  await flushAsyncTurns(4);
  assert.deepEqual([...doc.querySelectorAll("#sheet-align-h-select option")].map((o) => o.textContent), ["일반", "왼쪽", "가운데", "오른쪽"]);
  assert.equal(doc.querySelector("#sheet-font-size-select option").textContent, "자동");
  doc.getElementById("sheet-numfmt-dialog").close();

  // SLIDE
  switchModule(doc, "SLIDE");
  await flushAsyncTurns(8);
  assert.equal(doc.querySelector('[data-slide-act="insert-textbox"] .md-label').textContent, "텍스트 상자");
  assert.equal(doc.querySelector('[data-slide-act="distribute-h"] .md-label').textContent, "가로 간격을 동일하게");
  assert.equal(doc.querySelector('[data-slide-act="distribute-v"] .md-label').textContent, "세로 간격을 동일하게");
  assert.equal(doc.querySelector('[data-slide-act="delete-element"] .md-label').textContent, "개체 삭제");
  assert.equal(doc.getElementById("btn-slide-delete").getAttribute("aria-label"), "슬라이드 삭제");
  assert.equal(doc.querySelector('label[for="slide-transition-select"]').textContent, "전환");
  assert.equal(shownLabel("slide-mod-transition"), "전환 효과");
  assert.equal(shownLabel("slide-mod-presenter"), "발표자 보기");
  assert.deepEqual([...doc.querySelectorAll('#menu-bar [data-menu="format"] > .menu-dropdown > .md-sub')].filter((el) => !el.hidden).map((el) => el.querySelector(".md-label").textContent), ["맞춤"]);
  assert.deepEqual([...doc.querySelectorAll("#slide-layout-select option")].map((o) => o.textContent), ["제목 슬라이드", "제목 및 내용", "구역 머리글", "콘텐츠 2개", "빈 화면"]);
  switchModule(doc, "DOC");
  await flushAsyncTurns(6);
  assert.deepEqual([...doc.querySelectorAll('#menu-bar [data-menu="format"] > .menu-dropdown > .md-sub')].filter((el) => !el.hidden), []);
});

// The Pages format sidebar follows the product's own guide: the toolbar's 포맷 and 문서 buttons each open a sidebar, the format sidebar
// has the 스타일 and 레이아웃 buttons, 스타일 holds 단락 스타일, 서체 (with the 볼드체·이탤릭체·밑줄체·취소선 buttons and 텍스트 색상),
// 정렬 (icon buttons and 기본 들여쓰기) and 간격, and 레이아웃 holds 들여쓰기. Every control applies to the selected paragraphs.
test("The Pages format sidebar has the product's structure and its controls format the selection", async () => {
  const selection = createSelection();
  const h = await createWorkspaceHarness({
    selection,
    search: "?module=DOC&win=w-pd361-sidebar",
    testIpc: async (cmd, raw) => {
      if (cmd !== "convert_document_format") return null;
      const req = JSON.parse(raw || "{}");
      if (req.action === "decode" && String(req.fileName || "").endsWith(".pages")) {
        return { status: "OK", module: "DOC", format: "pages", kind: "blocks", itemCount: 1, title: "sb.pages", blocks: [{ type: "paragraph", runs: [{ text: "본문" }] }] };
      }
      return null;
    },
  });
  await flushAsyncTurns(10);
  await pickFile(h.document, makeFixtureFile("sb.pages", "pages-bytes"));
  await flushAsyncTurns(16);
  const doc = h.document;
  const byId = (id) => doc.getElementById(id);
  const attr = (sel, name) => doc.querySelector(sel).getAttribute(name);
  const editor = byId("doc-editor");

  // Structure and words
  assert.deepEqual(["format", "document"].map((kind) => attr(`[data-pages-sidebar-toggle="${kind}"]`, "title")), ["포맷", "문서"]);
  assert.deepEqual([...doc.querySelectorAll("#pages-format-sidebar .pages-insp-tab")].map((el) => el.textContent), ["스타일", "레이아웃"]);
  const sectionLabels = (pane) => [...doc.querySelectorAll(`#pages-pane-${pane} > .pages-insp-section > .pages-insp-label`)].map((el) => el.textContent);
  assert.deepEqual(sectionLabels("style"), ["단락 스타일", "서체", "정렬", "간격", "구분점 및 목록"]);
  assert.deepEqual([...byId("pages-insp-list").querySelectorAll("option")].map((o) => o.textContent), ["없음", "텍스트 구분점", "숫자"]);
  assert.deepEqual(sectionLabels("layout"), ["들여쓰기"]);
  assert.deepEqual([...doc.querySelectorAll("[data-pages-style-btn]")].map((el) => [el.getAttribute("title"), el.getAttribute("aria-label"), el.dataset.icon]), [
    ["볼드체", "볼드체", "bold"], ["이탤릭체", "이탤릭체", "italic"], ["밑줄체", "밑줄체", "underline"], ["취소선", "취소선", "strikethrough"],
  ]);
  assert.deepEqual([...doc.querySelectorAll("[data-pages-align-btn]")].map((el) => [el.getAttribute("title"), el.dataset.icon, el.textContent.trim()]), [
    ["왼쪽 정렬", "align-left", ""], ["가운데 정렬", "align-center", ""], ["오른쪽 정렬", "align-right", ""], ["좌우 정렬", "align-justify", ""],
  ]);
  assert.deepEqual([...doc.querySelectorAll("[data-pages-indent-btn]")].map((el) => el.getAttribute("title")), ["들여쓰기 단계 감소", "들여쓰기 단계 증가"]);
  assert.equal(attr("#pages-insp-textcolor", "title"), "텍스트 색상");
  h.userClick(doc.querySelector('[data-pages-tab="layout"]'));
  assert.equal(byId("pages-pane-layout").hidden, false);
  assert.equal(byId("pages-pane-style").hidden, true);
  h.userClick(doc.querySelector('[data-pages-tab="style"]'));
  assert.equal(byId("pages-pane-style").hidden, false);

  // The controls format the selected paragraph
  editor.innerHTML = "<p>본문 글</p>";
  const text = editor.querySelector("p").firstChild;
  const select = () => selection.addRange(rangeOf(text, 0, text, 2));
  const para = () => editor.querySelector("p");
  select();
  h.userClick(doc.querySelector('[data-pages-style-btn="bold"]'));
  await flushAsyncTurns(4);
  assert.ok(editor.querySelector('[data-eo-bold="true"]'), "볼드체 button makes the selection bold");
  select();
  h.userClick(doc.querySelector('[data-pages-align-btn="center"]'));
  await flushAsyncTurns(4);
  assert.equal(para().style.textAlign, "center");
  select();
  h.userClick(doc.querySelector('[data-pages-align-btn="justify"]'));
  await flushAsyncTurns(4);
  assert.equal(para().style.textAlign, "justify");
  select();
  const color = byId("pages-insp-textcolor");
  color.value = "#ff0000";
  color.dispatchEvent({ type: "change", target: color, bubbles: true });
  await flushAsyncTurns(4);
  assert.ok(editor.querySelector('[data-eo-color="FF0000"]'), "텍스트 색상 colors the selection");
  select();
  h.userClick(doc.querySelector('[data-pages-indent-btn="indent-in"]'));
  await flushAsyncTurns(4);
  assert.ok(Number.parseInt(para().style.paddingLeft, 10) > 0, "기본 들여쓰기 moves the paragraph");
  select();
  const left = byId("pages-insp-indent-left");
  left.value = "1";
  left.dispatchEvent({ type: "change", target: left, bubbles: true });
  await flushAsyncTurns(4);
  assert.equal(para().style.paddingLeft, "38px", "1 cm");
  select();
  const first = byId("pages-insp-indent-first");
  first.value = "0.5";
  first.dispatchEvent({ type: "change", target: first, bubbles: true });
  await flushAsyncTurns(4);
  assert.equal(para().style.textIndent, "19px", "0.5 cm");
  select();
  const before = byId("pages-insp-space-before");
  before.value = "12";
  before.dispatchEvent({ type: "change", target: before, bubbles: true });
  await flushAsyncTurns(4);
  assert.equal(Number.parseFloat(para().style.marginTop) > 0, true, "단락 앞 spacing");

  // 간격: the popup names the kind (줄, 최소, 고정, 줄 사이) and the field beside it takes lines or points (PD-366)
  const lineKind = byId("pages-insp-linekind");
  const lineAmount = byId("pages-insp-lineamount");
  assert.deepEqual([...lineKind.querySelectorAll("option")].map((o) => o.textContent), ["줄", "최소", "고정", "줄 사이"]);
  const setLine = async (kindValue, amount) => {
    select();
    if (kindValue !== undefined) {
      lineKind.value = kindValue;
      lineKind.dispatchEvent({ type: "change", target: lineKind, bubbles: true });
      await flushAsyncTurns(4);
    }
    if (amount !== undefined) {
      lineAmount.value = amount;
      lineAmount.dispatchEvent({ type: "change", target: lineAmount, bubbles: true });
      await flushAsyncTurns(4);
    }
  };
  await setLine("exact", "18");
  assert.deepEqual([para().dataset.eoLineKind, para().dataset.eoLineTwips, para().style.lineHeight], ["exact", "360", "24px"], "고정 18 pt");
  assert.equal(byId("pages-insp-lineunit").hidden, false, "points show their unit");
  await setLine("between", "6");
  assert.deepEqual([para().dataset.eoLineKind, para().dataset.eoLineTwips], ["between", "120"], "줄 사이 6 pt");
  await setLine("atLeast");
  assert.deepEqual([para().dataset.eoLineKind, para().dataset.eoLineTwips], ["atLeast", "320"], "최소 starts at 16 pt");
  await setLine(undefined, "99999");
  assert.equal(para().dataset.eoLineTwips, "320", "an amount out of range changes nothing");
  await setLine("multiple");
  assert.equal(para().dataset.eoLineKind, undefined, "줄 is a multiple again");
  assert.equal(para().style.lineHeight, "1.08", "줄 starts at the document's own spacing");
  assert.equal(byId("pages-insp-lineunit").hidden, true);
  await setLine(undefined, "1.5");
  assert.equal(para().style.lineHeight, "1.50", "줄 1.5");
  select();
  doc.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
  assert.deepEqual([lineKind.value, lineAmount.value], ["multiple", "1.5"], "the controls show the paragraph's own spacing");
  const listSelect = byId("pages-insp-list");
  const pickList = async (value) => {
    select();
    listSelect.value = value;
    listSelect.dispatchEvent({ type: "change", target: listSelect, bubbles: true });
    await flushAsyncTurns(4);
  };
  await pickList("bullet");
  assert.equal(editor.querySelector("ul > li") !== null, true, "텍스트 구분점 makes a bulleted list");
  await pickList("ordered");
  assert.equal(editor.querySelector("ol > li") !== null && editor.querySelector("ul > li") === null, true, "숫자 makes a numbered list");
  await pickList("");
  assert.equal(editor.querySelector("li"), null, "없음 takes the paragraph out of its list");
});

// The page setup dialog shows the items of the product it belongs to: HWP the reference editor's 편집 용지 (제본: 한쪽·맞쪽·위로),
// Word 용지 설정 (여러 페이지, 제본용 여백 위치), Pages 페이지 설정 (문서 여백, 페이지 마주보기; no gutter and no top binding).
test("The page setup dialog shows each product's own items and words", async () => {
  const h = await createWorkspaceHarness({
    search: "?module=DOC&win=w-ps-products",
    testIpc: async (cmd, raw) => {
      if (cmd !== "convert_document_format") return null;
      const req = JSON.parse(raw || "{}");
      if (req.action === "decode" && String(req.fileName || "").endsWith(".pages")) {
        return { status: "OK", module: "DOC", format: "pages", kind: "blocks", itemCount: 1, title: "ps.pages", blocks: [{ type: "paragraph", runs: [{ text: "본문" }] }] };
      }
      return null;
    },
  });
  await flushAsyncTurns(10);
  const doc = h.document;
  const byId = (id) => doc.getElementById(id);
  const text = (id) => byId(id).textContent.trim();
  const shown = (id) => !byId(id).hidden;
  const openDialog = async () => {
    h.userClick(studioItem(doc, "layout-paper"));
    await flushAsyncTurns(4);
  };
  const closeDialog = () => byId("doc-page-setup-dialog").close();
  const sidePair = () => [text("doc-ps-left-label"), text("doc-ps-right-label")];
  const change = (id, value) => {
    const el = byId(id);
    if (typeof value === "boolean") el.checked = value;
    else el.value = String(value);
    el.dispatchEvent({ type: "change", target: el, bubbles: true });
  };

  // Word
  await openDialog();
  assert.equal(text("doc-page-setup-title"), "용지 설정");
  assert.deepEqual(["doc-ps-paper-title", "doc-ps-width-label", "doc-ps-height-label", "doc-ps-margin-title", "doc-ps-gutter-label"].map(text), ["용지 크기", "너비", "높이", "여백", "제본용 여백"]);
  assert.deepEqual(["doc-ps-binding-word", "doc-ps-gutter", "doc-ps-binding-cards", "doc-ps-binding-title", "doc-ps-facing-row"].map(shown), [true, true, false, false, false]);
  assert.deepEqual([...byId("doc-ps-multi").querySelectorAll("option")].map((o) => o.textContent), ["기본", "여백 미러"]);
  assert.deepEqual([...byId("doc-ps-gutter-pos").querySelectorAll("option")].map((o) => o.textContent), ["왼쪽", "위쪽"]);
  assert.deepEqual(sidePair(), ["왼쪽", "오른쪽"]);
  change("doc-ps-multi", 1);
  assert.deepEqual(sidePair(), ["안쪽", "바깥쪽"]);
  assert.equal(byId("doc-ps-gutter-pos").disabled, true);
  closeDialog();

  // Pages
  await pickFile(doc, makeFixtureFile("ps.pages", "pages-bytes"));
  await flushAsyncTurns(16);
  await openDialog();
  assert.equal(text("doc-page-setup-title"), "페이지 설정");
  assert.deepEqual(["doc-ps-paper-title", "doc-ps-margin-title", "doc-ps-margin-top-label", "doc-ps-margin-bottom-label"].map(text), ["용지 크기", "문서 여백", "상단", "하단"]);
  assert.deepEqual(["doc-ps-binding-section", "doc-ps-gutter", "doc-ps-gutter-label", "doc-ps-facing-row"].map(shown), [false, false, false, true]);
  assert.deepEqual(sidePair(), ["왼쪽", "오른쪽"]);
  change("doc-ps-facing", true);
  assert.deepEqual(sidePair(), ["안쪽", "바깥쪽"]);
  byId("btn-doc-ps-apply").click();
  await flushAsyncTurns(4);
  const setup = h.host.__EASYOFFICE_DOC_PAGE_SETUP_CORE__.parse(byId("doc-editor").dataset.eoPage);
  assert.deepEqual([setup.binding, setup.gutter], [1, 0], "facing pages is the duplex binding with no gutter");
  await openDialog();
  assert.equal(byId("doc-ps-facing").checked, true);
  assert.deepEqual(sidePair(), ["안쪽", "바깥쪽"]);
  closeDialog();

  // HWP
  switchModule(doc, "HWP");
  await flushAsyncTurns(8);
  await openDialog();
  assert.equal(text("doc-page-setup-title"), "편집 용지");
  assert.deepEqual(["doc-ps-paper-title", "doc-ps-width-label", "doc-ps-height-label", "doc-ps-margin-title", "doc-ps-gutter-label", "doc-ps-binding-title"].map(text), ["용지 종류", "폭", "길이", "용지 여백", "제본", "제본"]);
  assert.deepEqual(["doc-ps-single-label", "doc-ps-facing-label", "doc-ps-top-label"].map(text), ["한쪽", "맞쪽", "위로"]);
  assert.deepEqual(["doc-ps-binding-cards", "doc-ps-binding-title", "doc-ps-gutter"].map(shown), [true, true, true]);
  assert.deepEqual(["doc-ps-binding-word", "doc-ps-facing-row"].map(shown), [false, false]);
  assert.deepEqual(sidePair(), ["왼쪽", "오른쪽"]);
  closeDialog();
});
