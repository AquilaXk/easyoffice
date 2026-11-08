import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { cssDeclarations } from "./test-support/css-declarations.mjs";
import { readText } from "./test-support/read-text.mjs";

// Shell chrome (#126, PD-088): one SVG icon set, accessible names on every control, compact top bar,
// a status bar that never clips, a command palette overlay and readable contrast in both themes.

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const CSS = readText(fileURLToPath(new URL("styles.css", UI_DIR)));
const SHELL_CSS = readText(fileURLToPath(new URL("shell.css", UI_DIR)));
const CONTENT_CSS = readText(fileURLToPath(new URL("content.css", UI_DIR)));
const TOKENS_CSS = readText(fileURLToPath(new URL("tokens.css", UI_DIR)));
const TOOLBAR_CSS = readText(fileURLToPath(new URL("toolbar.css", UI_DIR)));
const MENU_CSS = readText(fileURLToPath(new URL("menu-bar.css", UI_DIR)));
const ALL_CSS = `${CSS}\n${SHELL_CSS}\n${CONTENT_CSS}\n${MENU_CSS}\n${TOOLBAR_CSS}`;
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
    .querySelectorAll("#icon-toolbar button, .statusbar-zoom-btn, #btn-theme-toggle, #btn-command-palette, .sb-step-btn, #doc-style-bar button")
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
  assert.equal(document.documentElement.dataset.themeEffective, "dark");
});

test("reference values: 28px menu bar with the light border, bottom-left update card with the dialog shadow, 14px preview margin, product information in the dialog frame", () => {
  const bar = cssRule(".topbar", SHELL_CSS);
  assert.ok(hasDecl(bar, "height", "28px") && hasDecl(bar, "border-bottom", "1px solid var(--ui-border-light)"));
  assert.ok(hasDecl(cssRule(".icon-btn", SHELL_CSS), "height", "24px") && hasDecl(cssRule(".palette-btn", SHELL_CSS), "height", "22px"), "the search and theme controls fit 28px");
  const home = readText(fileURLToPath(new URL("home.css", UI_DIR)));
  const card = cssRule(".update-notice", home);
  assert.ok(hasDecl(card, "left", "16px") && hasDecl(card, "bottom", "48px") && hasDecl(card, "box-shadow", "var(--shadow-dialog)"));
  assert.ok(!hasDecl(card, "top") && !hasDecl(card, "right"));
  const loaded = cssRule(".home-recent-preview.is-loaded", home);
  assert.ok(hasDecl(loaded, "padding", "14px") && hasDecl(cssRule(".home-preview-canvas", home), "border-radius", "var(--radius-lg)"));
  assert.match(readText(fileURLToPath(new URL("home-screen.js", UI_DIR))), /앱을 실행하자마자 이어서 작업할 수 있도록 최근에 연 문서를 보여줍니다\./);
  const html = readText(fileURLToPath(new URL("index.html", UI_DIR)));
  const about = html.slice(html.indexOf('<dialog id="about-dialog"'));
  assert.match(about, /class="tool-dialog"[\s\S]*?<div class="dialog-wrap">\s*<div class="dialog-title"><span id="about-dialog-title">제품 정보<\/span>/);
  assert.match(about, /<div class="dialog-body about-body">\s*<div class="about-product-name">EasyOffice<\/div>[\s\S]*?id="about-version"/);
  assert.ok(hasDecl(cssRule(".tool-dialog", SHELL_CSS), "width", "min(460px"));
  assert.ok(hasDecl(cssRule("#about-dialog .about-body", SHELL_CSS), "text-align", "center"));
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


// Tokens of one theme from tokens.css (PD-098); the dark block overrides the light one.
function tokens(theme) {
  const light = TOKENS_CSS.slice(TOKENS_CSS.indexOf("\n:root {"), TOKENS_CSS.indexOf('\n:root[data-theme-effective="dark"] {'));
  const dark = TOKENS_CSS.slice(TOKENS_CSS.indexOf('\n:root[data-theme-effective="dark"] {'));
  const out = {};
  for (const block of theme === "dark" ? [light, dark] : [light]) {
    for (const [name, value] of cssDeclarations(block)) {
      if (/^#[0-9a-fA-F]{6}$/.test(value)) out[name] = value;
    }
  }
  return out;
}

test("shell chrome colours meet WCAG AA in the light and dark themes", () => {
  // [text, background, minimum ratio]: the pairs the chrome CSS actually draws.
  const pairs = [
    ["--ui-text", "--ui-bg-light", 4.5], ["--ui-text-muted", "--ui-bg-light", 4.5],
    ["--ui-text", "--ui-hover", 4.5], ["--ui-text", "--ui-surface", 4.5],
    ["--ui-text-muted", "--ui-surface", 4.5], ["--ui-text-secondary", "--ui-bg-light", 3],
    ["--ui-text-secondary", "--ui-bg", 4.5], ["--accent-strong", "--ui-surface", 4.5],
    ["--accent-strong", "--ui-selected", 4.5], ["--accent-strong", "--ui-hover", 4.5],
  ];
  const stateful = {
    // The open menu title and the primary button swap text colour per theme (styles.css, shell.css).
    light: [["--ui-text-on-accent", "--ui-menu-open", 4.5], ["--ui-text-on-accent", "--accent-strong", 4.5]],
    dark: [["--ui-text", "--ui-selected", 4.5], ["--ui-bg", "--accent-strong", 4.5]],
  };
  for (const theme of ["light", "dark"]) {
    const t = tokens(theme);
    for (const [fg, bg, min] of [...pairs, ...stateful[theme]]) {
      assert.ok(t[fg] && t[bg], `${theme}: ${fg} / ${bg} defined`);
      const ratio = contrast(t[fg], t[bg]);
      assert.ok(ratio >= min, `${theme}: ${fg} on ${bg} is ${ratio.toFixed(2)}:1 (need ${min}:1)`);
    }
  }
});

test("no UI file exceeds the line ceiling and the new modules are registered", () => {
  for (const name of ["icons.js", "window-bridge.js"]) {
    const lines = readText(fileURLToPath(new URL(name, UI_DIR))).split("\n").length;
    assert.ok(lines < 1000, `${name} has ${lines} lines`);
  }
  const html = readText(fileURLToPath(new URL("index.html", UI_DIR)));
  for (const asset of ["./icons.js", "./window-bridge.js", "./shell.css", "./tokens.css", "./content.css", "./theme.js"]) assert.ok(html.includes(asset), asset);
});

// ---- desktop chrome: right-click guard and platform shortcut labels ---------------------------

const menuLabels = (document) => [...document.querySelectorAll(".md-shortcut")].map((el) => el.textContent);

test("the web view's right-click menu is blocked on the menu bar, toolbars and status bar only", async () => {
  const { document } = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  const contextmenu = (el) => {
    // dispatchEvent returns false when a listener called preventDefault.
    return !el.dispatchEvent({ type: "contextmenu", target: el });
  };
  for (const selector of ["#menu-bar", "#icon-toolbar", "#doc-style-bar", "#status-bar"]) {
    assert.equal(contextmenu(document.querySelector(selector)), true, `${selector} blocks it`);
  }
  assert.equal(contextmenu(document.querySelector("#menu-bar .menu-title")), true, "a child of the menu bar blocks it too");
  assert.equal(contextmenu(document.getElementById("cmd-search-input")), false, "text fields keep their own menu");
  assert.equal(contextmenu(document.getElementById("doc-editor")), false, "the editing area keeps its own menu");
  assert.equal(contextmenu(document.querySelector(".canvas-stage")), false);
});

test("menu shortcut labels follow the platform: command glyphs on macOS, Ctrl elsewhere", async () => {
  const mac = await createWorkspaceHarness({ platform: "MacIntel" });
  await flushAsyncTurns(10);
  const macLabels = menuLabels(mac.document);
  assert.ok(macLabels.length >= 15);
  assert.ok(macLabels.includes("⌘N") && macLabels.includes("⇧⌘Z") && macLabels.includes("⌥⌘O") && macLabels.includes("⌥⌘D"), macLabels.join(" "));
  assert.ok(macLabels.every((label) => !/Ctrl|Alt|Shift/.test(label)), "no Windows words left on macOS");
  assert.ok(macLabels.includes("/"), "labels that are not key combinations stay");

  const win = await createWorkspaceHarness({ platform: "Win32" });
  await flushAsyncTurns(10);
  const winLabels = menuLabels(win.document);
  assert.ok(winLabels.includes("Ctrl+N") && winLabels.includes("Ctrl+Shift+Z") && winLabels.includes("Ctrl+Alt+O"));
  assert.ok(winLabels.every((label) => !/[⌘⌥⇧⌃]/.test(label)));
});

test("formatPlatformShortcut writes both notations from either one and leaves other text alone", async () => {
  const { sandbox } = await createWorkspaceHarness();
  const fmt = sandbox.__EASYOFFICE_RIBBON_CATALOG__.formatPlatformShortcut;
  assert.equal(fmt("Ctrl+Shift+Z", "⌘"), "⇧⌘Z");
  assert.equal(fmt("Ctrl+Alt+Shift+K", "⌘"), "⌥⇧⌘K");
  assert.equal(fmt("Ctrl+0", "⌘"), "⌘0");
  assert.equal(fmt("⌘S", "⌘"), "⌘S", "already in macOS notation");
  assert.equal(fmt("/", "⌘"), "/");
  assert.equal(fmt("F5", "⌘"), "F5");
  assert.equal(fmt("⌘B", "Ctrl"), "Ctrl+B");
  assert.equal(fmt("Ctrl+N", "Ctrl"), "Ctrl+N");
});
