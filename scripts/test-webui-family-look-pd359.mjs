import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
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

test("PD-359: light and dark tokens satisfy WCAG AA contrast for primary buttons, text, and paper guides", () => {
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

  const eoCss = readHwp("src/eo-overrides.css");
  assert.match(eoCss, /\.cs-preview\s*\{[^}]*color:\s*var\(--caret-color\)/);
  assert.match(eoCss, /\.eq-preview-empty\s*\{[^}]*color:\s*var\(--ui-text-secondary\)/);
  assert.match(eoCss, /:root\[data-theme-effective="dark"\]\s+\.eq-preview-empty\s*\{[^}]*color:\s*var\(--ui-border-strong\)/);
  const shellCss = readUi("shell.css");
  assert.match(shellCss, /\.sheet-rot-stage\s*\{/);
  assert.match(shellCss, /\.custom-select\.open\s+\.custom-select-menu\.slide-layout-grid-menu\s*\{/);
});

test("PD-359: START and CONV keep a minimal 5-menu chrome and clear docFlavor across transitions", async () => {
  const start = await createWorkspaceHarness({ search: "?module=START&win=w-pd359-start" });
  await flushAsyncTurns(10);
  const visibleMenus = (doc) =>
    doc.querySelectorAll("#menu-bar .menu-item")
      .filter((el) => !el.hidden)
      .map((el) => el.querySelector(".menu-title").textContent);
  assert.deepEqual(visibleMenus(start.document), ["파일", "편집", "보기", "도구", "도움말"]);
  assert.equal(start.document.getElementById("app").dataset.docFlavor, undefined);

  const conv = await createWorkspaceHarness({ search: "?module=CONV&win=w-pd359-conv" });
  await flushAsyncTurns(10);
  assert.deepEqual(visibleMenus(conv.document), ["파일", "편집", "보기", "도구", "도움말"]);

  // Opening .pages activates PAGES inspector + menu bar, and switching to DOC/SLIDE isolates flavor cleanly
  const h = await createWorkspaceHarness({
    search: "?module=DOC&win=w-pd359-pages",
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

  h.userClick(h.document.querySelector('[data-pages-tab="document"]'));
  assert.equal(h.document.querySelector('[data-pages-pane="document"]').hidden, false);
  assert.equal(h.document.querySelector('[data-pages-pane="text"]').hidden, true);

  switchModule(h.document, "HWP");
  await flushAsyncTurns(8);
  assert.equal(app.dataset.docFlavor, undefined);
  assert.equal(h.document.getElementById("pages-inspector").hidden, true);
  assert.equal(h.document.getElementById("ruler-corner").textContent, "mm");
  assert.deepEqual(visibleMenus(h.document), ["파일", "편집", "보기", "입력", "서식", "쪽", "표", "도구", "도움말"]);
});

test("PD-359: SHEET uses spreadsheet labels, BOARD omits duplicate zoom chrome, and toolbar scroller wraps wide toolbars", async () => {
  const sheet = await createWorkspaceHarness({ search: "?module=SHEET&win=w-pd359-sheet" });
  await flushAsyncTurns(10);
  const totalsMod = sheet.host.__EASYOFFICE_SHEET_TABLE_TOTALS__;
  const fnLabels = Object.fromEntries(totalsMod.FUNCTIONS.map(([id, label]) => [id, label]));
  assert.equal(fnLabels.max, "최대");
  assert.equal(fnLabels.min, "최소");
  assert.ok(sheet.document.querySelector(".sheet-rot-stage #sheet-align-rotation-preview"), "rotation preview wrapped in .sheet-rot-stage");

  for (const id of ["icon-toolbar", "sheet-table-bar", "board-toolbar"]) {
    const bar = sheet.document.getElementById(id);
    assert.ok(bar, `#${id} exists`);
    assert.ok(bar.querySelector(".tb-scroll-viewport .tb-scroll-track"), `#${id} mounts horizontal scroller track`);
    assert.ok(bar.querySelector(".tb-scroll-nav-prev"), `#${id} mounts prev nav`);
    assert.ok(bar.querySelector(".tb-scroll-nav-next"), `#${id} mounts next nav`);
  }

  const slide = await createWorkspaceHarness({ search: "?module=SLIDE&win=w-pd359-slide" });
  await flushAsyncTurns(10);
  const layoutMenu = slide.document.querySelector('.custom-select[data-select-id="slide-layout-select"] .custom-select-menu');
  assert.ok(layoutMenu.classList.contains("slide-layout-grid-menu"));
  assert.ok(layoutMenu.querySelectorAll(".slide-layout-thumb").length >= 5, "slide layout dropdown owns visual wireframes");
  assert.equal(slide.document.querySelector('#menu-bar [data-menu="format"] [data-slide-act="bring-front"]')?.hidden, false);
  assert.equal(slide.document.querySelector('#menu-bar [data-menu="slide"] [data-studio-menu-cmd="layout-margins"]'), null);

  const koOverrides = JSON.parse(fs.readFileSync(path.join(REPO, "config/rhwp-studio-ko-overrides.json"), "utf8"));
  assert.equal(Object.values(koOverrides).includes("옅은 테두리"), true, "rhwp ko overrides patches 옅은 테두리");
  assert.equal(Object.values(koOverrides).includes("열은 테두리"), false, "no 열은 테두리 typo");
});
