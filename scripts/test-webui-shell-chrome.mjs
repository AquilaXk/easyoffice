import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

// Shell chrome (#126, PD-088): one SVG icon set, accessible names on every control, compact top bar,
// a status bar that never clips, a command palette overlay and readable contrast in both themes.

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const CSS = fs.readFileSync(fileURLToPath(new URL("styles.css", UI_DIR)), "utf8");
const SHELL_CSS = fs.readFileSync(fileURLToPath(new URL("shell.css", UI_DIR)), "utf8");
const ALL_CSS = `${CSS}\n${SHELL_CSS}`;
const ICONS_JS = new URL("icons.js", UI_DIR);

// Pictographs, arrows, geometric shapes and symbols that used to stand in for icons.
const GLYPH_ICON = /[←-⇿⌀-⏿■-➿⤀-⯿\u{1F000}-\u{1FAFF}¶※]/u;

function cssRule(selector, source = ALL_CSS) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, String.raw`\$&`);
  const re = new RegExp(String.raw`(?:^|\})\s*${escaped}\s*\{([^}]*)\}`, "gm");
  const bodies = [];
  let m;
  while ((m = re.exec(source)) !== null) bodies.push(m[1]);
  return bodies.join(";");
}

function hasDecl(body, prop, value) {
  const re = new RegExp(String.raw`(?:^|;|\s)${prop}\s*:\s*([^;]+)`, "g");
  let m;
  while ((m = re.exec(body)) !== null) {
    if (value === undefined || m[1].trim().startsWith(value)) return true;
  }
  return false;
}

function accessibleName(el, document) {
  const aria = (el.getAttribute("aria-label") || "").trim();
  if (aria) return aria;
  const by = (el.getAttribute("aria-labelledby") || "").trim();
  if (by) {
    const text = by.split(/\s+/).map((id) => document.getElementById(id)?.textContent || "").join(" ").trim();
    if (text) return text;
  }
  if (el.id) {
    const label = document.querySelectorAll("label").find((l) => l.getAttribute("for") === el.id);
    if (label?.textContent.trim()) return label.textContent.trim();
  }
  const wrapping = el.closest("label");
  if (wrapping && textWithoutIcons(wrapping).trim()) return textWithoutIcons(wrapping).trim();
  const visible = textWithoutIcons(el).trim();
  if (visible) return visible;
  return (el.getAttribute("title") || "").trim();
}

function textWithoutIcons(el) {
  let out = "";
  for (const node of el.childNodes) {
    if (node.nodeType === 3) out += node.textContent;
    else if (!["svg", "path"].includes(String(node.tagName).toLowerCase()) && node.getAttribute?.("aria-hidden") !== "true") out += textWithoutIcons(node);
  }
  return out;
}

const CONTROL_SELECTOR = 'button, a[href], select, textarea, input:not([type="hidden"]), [role="button"], [role="menuitem"], [role="tab"]';

function unlabeledControls(document) {
  return document.querySelectorAll(CONTROL_SELECTOR).filter((el) => accessibleName(el, document) === "").map((el) => el.id || el.className || el.tagName);
}

function iconOnlyControlsWithoutLabel(document) {
  return document.querySelectorAll(CONTROL_SELECTOR)
    .filter((el) => el.querySelector("[data-icon], svg") && textWithoutIcons(el).trim() === "" && !(el.getAttribute("aria-label") || "").trim())
    .map((el) => el.id || el.className);
}

test("icon registry: one frozen set of safe 24px stroke icons", () => {
  const env = {};
  loadUiModule(ICONS_JS, env);
  const api = env.__EASYOFFICE_ICONS__;
  assert.ok(api && Object.isFrozen(api.ICONS));
  const names = Object.keys(api.ICONS);
  assert.ok(names.length >= 40, `enough icons (${names.length})`);
  for (const [name, paths] of Object.entries(api.ICONS)) {
    assert.ok(/^[a-z0-9-]+$/.test(name), name);
    for (const d of [paths].flat()) assert.match(d, /^[MmLlHhVvCcSsQqTtAaZz0-9\s.,-]+$/, `${name} path data is plain`);
  }
  for (const needed of ["save", "undo", "redo", "search", "sun", "moon", "chevron-down", "bold", "italic", "underline", "zoom-in", "zoom-out", "plus", "close"]) {
    assert.ok(names.includes(needed), needed);
  }
});

test("icons are inserted as SVG elements and replace glyph text", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=DOC&win=w-icons" });
  const hosts = document.querySelectorAll("[data-icon]");
  assert.ok(hosts.length > 30, `many icon hosts (${hosts.length})`);
  const env = document.defaultView;
  for (const host of hosts) {
    assert.ok(env.__EASYOFFICE_ICONS__.ICONS[host.dataset.icon], `known icon ${host.dataset.icon}`);
    const svg = host.querySelector("svg");
    assert.ok(svg, `svg inside ${host.dataset.icon}`);
    assert.equal(svg.getAttribute("aria-hidden"), "true");
    assert.equal(svg.getAttribute("viewBox"), "0 0 24 24");
  }
  const glyphButtons = document
    .querySelectorAll("#icon-toolbar button, .qat-btn, .statusbar-zoom-btn, .menu-toolbox-toggle, #btn-theme-toggle, #btn-command-palette, .sb-step-btn, #doc-style-bar button")
    .filter((b) => GLYPH_ICON.test(textWithoutIcons(b)));
  assert.deepEqual(glyphButtons.map((b) => b.id || b.title), [], "no emoji or symbol characters stand in for icons");
});

test("every control has an accessible name (0 unlabeled controls in every module and theme)", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=DOC&win=w-a11y" });
  for (const mod of ["DOC", "HWP", "SHEET", "SLIDE", "PDF", "CONV"]) {
    switchModule(document, mod);
    await flushAsyncTurns(10);
    assert.deepEqual(unlabeledControls(document), [], `unlabeled controls in ${mod}`);
    assert.deepEqual(iconOnlyControlsWithoutLabel(document), [], `icon-only controls need aria-label in ${mod}`);
  }
  const start = await createWorkspaceHarness({ search: "?module=START&win=main" });
  assert.deepEqual(unlabeledControls(start.document), [], "start window");
  assert.deepEqual(iconOnlyControlsWithoutLabel(start.document), []);
});

test("theme toggle keeps an accessible name and announces its state", async () => {
  const { document, userClick } = await createWorkspaceHarness({ search: "?module=DOC&win=w-theme" });
  const btn = document.getElementById("btn-theme-toggle");
  assert.equal(btn.getAttribute("aria-label"), "다크 모드로 전환");
  userClick(btn);
  await flushAsyncTurns(10);
  assert.equal(btn.getAttribute("aria-label"), "라이트 모드로 전환");
  assert.equal(document.documentElement.dataset.theme, "dark");
});

test("top bar: menus on the left, compact right-side controls that never shrink or wrap", () => {
  const bar = cssRule(".topbar");
  assert.ok(hasDecl(bar, "flex-wrap", "nowrap") || !hasDecl(bar, "flex-wrap"), "no wrapping");
  assert.ok(hasDecl(bar, "min-width", "0") || hasDecl(bar, "overflow", "hidden") || hasDecl(bar, "overflow-x", "hidden"));
  assert.ok(hasDecl(cssRule(".topbar-actions"), "flex-shrink", "0"));
  assert.ok(hasDecl(cssRule(".studio-menubar"), "flex-shrink", "0"));
  assert.ok(hasDecl(cssRule(".topbar-actions"), "margin-left", "auto"), "controls are pushed to the right edge");
  const iconBtn = cssRule(".icon-btn");
  assert.ok(hasDecl(iconBtn, "width") && hasDecl(iconBtn, "height"), "square icon buttons");
});

test("status bar items shrink with an ellipsis instead of clipping, and carry tooltips", async () => {
  for (const sel of ["#status-command-feedback", ".statusbar-doc-path"]) {
    const body = cssRule(sel);
    assert.ok(hasDecl(body, "text-overflow", "ellipsis"), `${sel} ellipsis`);
    assert.ok(hasDecl(body, "overflow", "hidden"), `${sel} overflow hidden`);
    assert.ok(hasDecl(body, "white-space", "nowrap"), `${sel} nowrap`);
    assert.ok(hasDecl(body, "min-width", "0"), `${sel} can shrink`);
  }
  assert.ok(hasDecl(cssRule(".statusbar"), "overflow", "hidden"));
  const { document } = await createWorkspaceHarness({ search: "?module=DOC&win=w-status" });
  const fb = document.getElementById("status-command-feedback");
  fb.textContent = "아주 긴 안내 문구".repeat(8);
  fb.dispatchEvent({ type: "mouseover", target: fb });
  await flushAsyncTurns();
  assert.equal(fb.getAttribute("title") || fb.title, fb.textContent, "long status text is available as a tooltip");
});

test("command palette is an overlay panel and the slash icon cannot overlap the placeholder", () => {
  const modal = cssRule(".cmd-modal[open]");
  assert.ok(hasDecl(modal, "position", "fixed") || hasDecl(cssRule(".cmd-modal"), "position", "fixed"));
  const wrap = cssRule(".cp-input-wrap");
  assert.ok(hasDecl(wrap, "position", "relative"));
  const slash = cssRule(".cp-slash");
  assert.ok(hasDecl(slash, "position", "absolute"));
  const input = cssRule(".cmd-input.cp-input");
  const pad = /padding(?:-left)?\s*:\s*(?:(?:[\d.]+px\s+){3})?([\d.]+)px/.exec(input);
  assert.ok(pad && Number(pad[1]) >= 36, `input left padding clears the icon (${pad?.[1]})`);
});

test("toolbar labels share one size and the presenter panel never wraps its labels", () => {
  const label = cssRule(".formula-bar label");
  assert.ok(hasDecl(label, "font-size", "12px"), "toolbar labels use the toolbar font size");
  assert.ok(hasDecl(label, "white-space", "nowrap"));
  const row = cssRule(".slide-show-presenter-row");
  assert.ok(hasDecl(row, "white-space", "nowrap") || hasDecl(cssRule(".slide-show-presenter-row strong"), "white-space", "nowrap"));
  assert.ok(hasDecl(cssRule(".slide-show-presenter-row strong"), "flex-shrink", "0"));
});

function luminance(hex) {
  const n = Number.parseInt(hex.slice(1), 16);
  const lin = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2];
}

function contrast(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

function tokens(scopeSelector) {
  const body = cssRule(scopeSelector, SHELL_CSS);
  const out = {};
  for (const m of body.matchAll(/(--eo-[a-z0-9-]+)\s*:\s*(#[0-9a-fA-F]{6})\s*;?/g)) out[m[1]] = m[2];
  return out;
}

test("shell chrome colours meet WCAG AA in the light and dark themes", () => {
  for (const [scope, theme] of [[":root", "light"], ['html[data-theme="dark"]', "dark"]]) {
    const t = tokens(scope);
    for (const needed of ["--eo-chrome-bg", "--eo-chrome-text", "--eo-chrome-muted", "--eo-chrome-hover", "--eo-chrome-hover-text", "--eo-chrome-icon", "--eo-chrome-open", "--eo-chrome-open-text", "--eo-status-bg", "--eo-status-text"]) {
      assert.ok(t[needed], `${theme}: ${needed}`);
    }
    const pairs = [
      ["--eo-chrome-text", "--eo-chrome-bg", 4.5], ["--eo-chrome-muted", "--eo-chrome-bg", 4.5],
      ["--eo-chrome-hover-text", "--eo-chrome-hover", 4.5], ["--eo-chrome-open-text", "--eo-chrome-open", 4.5],
      ["--eo-chrome-icon", "--eo-chrome-bg", 3], ["--eo-status-text", "--eo-status-bg", 4.5],
    ];
    for (const [fg, bg, min] of pairs) {
      const ratio = contrast(t[fg], t[bg]);
      assert.ok(ratio >= min, `${theme}: ${fg} on ${bg} is ${ratio.toFixed(2)}:1 (need ${min}:1)`);
    }
  }
});

test("no UI file exceeds the line ceiling and the new modules are registered", () => {
  for (const name of ["icons.js", "window-bridge.js"]) {
    const lines = fs.readFileSync(fileURLToPath(new URL(name, UI_DIR)), "utf8").split("\n").length;
    assert.ok(lines < 1000, `${name} has ${lines} lines`);
  }
  const html = fs.readFileSync(fileURLToPath(new URL("index.html", UI_DIR)), "utf8");
  for (const asset of ["./icons.js", "./window-bridge.js", "./shell.css"]) assert.ok(html.includes(asset), asset);
});
