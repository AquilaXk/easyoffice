import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { cssDeclarations } from "./test-support/css-declarations.mjs";
import { readText } from "./test-support/read-text.mjs";
import { demoHtml } from "./test-support/demo-fixtures.mjs";

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
    dark: [["--ui-text", "--ui-selected", 4.5], ["--ui-bg", "--accent-strong", 4.5], ["--ui-bg", "--accent-primary", 4.5]],
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

test("the update notice's primary button swaps to dark text in the dark theme (--color-primary is light there)", () => {
  const HOME_CSS = readText(fileURLToPath(new URL("home.css", UI_DIR)));
  const dark = cssRule(':root[data-theme-effective="dark"] .update-notice__button--primary', HOME_CSS);
  assert.ok(hasDecl(dark, "color", "var(--ui-bg)"));
  assert.ok(hasDecl(cssRule(".update-notice__button--primary", HOME_CSS), "background", "var(--color-primary)"));
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
  assert.equal(mac.document.getElementById("btn-stylebar-bold").getAttribute("title"), "굵게 (⌘B)");
  assert.equal(mac.document.getElementById("btn-slide-move-up").getAttribute("title"), "위로 이동 · ⌥↑");

  const win = await createWorkspaceHarness({ platform: "Win32" });
  await flushAsyncTurns(10);
  const winLabels = menuLabels(win.document);
  assert.ok(winLabels.includes("Ctrl+N") && winLabels.includes("Ctrl+Shift+Z") && winLabels.includes("Ctrl+Alt+O"));
  assert.ok(winLabels.every((label) => !/[⌘⌥⇧⌃]/.test(label)));
  assert.equal(win.document.getElementById("btn-stylebar-bold").getAttribute("title"), "굵게 (Ctrl+B)");
});

test("doc-style-bar selects are enhanced as custom-select combo boxes and style-bar overflow stays visible", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=DOC&win=w-doc-combos" });
  for (const id of ["doc-style-select", "doc-font-select", "doc-linespacing-select"]) {
    const select = document.getElementById(id);
    assert.ok(select.classList.contains("sb-combo"), `${id} is a combo box`);
    assert.ok(select.classList.contains("native-select-hidden"), `${id} stays as hidden state holder`);
    assert.equal(select.parentElement.dataset.selectId, id, `${id} is wrapped by custom-select`);
  }
  const styleBarRule = cssRule(".eo-doc-style-bar", CSS);
  assert.ok(hasDecl(styleBarRule, "overflow", "visible"), "style-bar allows custom-select dropdowns to overflow");
  assert.match(
    CSS,
    /\.app-shell:not\(\[data-module="DOC"\]\):not\(\[data-module="HWP"\]\)\s+\.statusbar-left-group,\s*\.app-shell:not\(\[data-module="SHEET"\]\)\s+#status-sheet-agg\s*\{\s*display:\s*none;\s*\}/
  );
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

test("reference chrome tokens, grid resize ratchet fix, and status bar bevel/zoom mark match rhwp-studio", async () => {
  assert.match(TOKENS_CSS, /:root\s*\{[\s\S]*?--ui-align-icon-filter:\s*none;/);
  assert.match(TOKENS_CSS, /:root\[data-theme-effective="dark"\]\s*\{[\s\S]*?--ui-align-icon-filter:\s*invert\(1\)\s+brightness\(1\.6\);/);
  const vhRule = cssRule(".visually-hidden", TOKENS_CSS);
  assert.ok(hasDecl(vhRule, "position", "absolute") && hasDecl(vhRule, "clip", "rect(0, 0, 0, 0)"));

  const gridRule = cssRule(".doc-studio-grid", CSS);
  assert.ok(hasDecl(gridRule, "grid-template-columns", "20px minmax(0, 1fr)"));
  assert.ok(hasDecl(gridRule, "grid-template-rows", "20px minmax(0, 1fr)"));
  const cornerRule = cssRule(".ruler-corner", CSS);
  assert.ok(hasDecl(cornerRule, "background", "var(--ruler-bg)"));

  const dividerRule = cssRule(".stb-divider", TOOLBAR_CSS);
  assert.ok(hasDecl(dividerRule, "border-left", "1px solid var(--ui-border-strong)"));
  assert.ok(hasDecl(dividerRule, "border-right", "1px solid var(--doc-paper)"));
  const markRule = cssRule(".stb-zoom-neutral-mark", TOOLBAR_CSS);
  assert.ok(hasDecl(markRule, "left", "33.33%"));

  const { document } = await createWorkspaceHarness({ search: "?module=DOC&win=w-zoom-keys" });
  const slider = document.getElementById("status-zoom-slider");
  assert.ok(slider.parentElement.classList.contains("stb-zoom-range-wrap"));
  assert.ok(slider.parentElement.querySelector(".stb-zoom-neutral-mark"));

  const sendKey = (key) => !slider.dispatchEvent({ type: "keydown", key, target: slider });
  assert.equal(sendKey("PageUp"), true);
  assert.equal(slider.value, "110");
  assert.equal(sendKey("PageDown"), true);
  assert.equal(slider.value, "100");
  assert.equal(sendKey("End"), true);
  assert.equal(slider.value, "200");
  assert.equal(sendKey("Home"), true);
  assert.equal(slider.value, "50");
});

test("icon toolbar wheel/focus scroller, custom-select dblclick/aria/keys, and dialog title drag work", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=DOC&win=w-chrome-interactions" });
  const toolbar = document.getElementById("icon-toolbar");
  toolbar.clientWidth = 200;
  toolbar.scrollWidth = 600;
  toolbar.scrollLeft = 10;
  let wheelPrevented = false;
  for (const fn of toolbar._listeners.get("wheel") || []) {
    fn({
      type: "wheel",
      deltaX: 0,
      deltaY: 45,
      target: toolbar,
      preventDefault() { wheelPrevented = true; },
    });
  }
  assert.equal(wheelPrevented, true);
  assert.equal(toolbar.scrollLeft, 55);

  const fontSelect = document.getElementById("doc-font-select");
  const root = fontSelect.parentElement;
  const trigger = root.querySelector(".custom-select-trigger");
  assert.match(trigger.getAttribute("aria-label") || "", /^글꼴 선택:/);

  let dblClicked = 0;
  fontSelect.addEventListener("dblclick", () => { dblClicked += 1; });
  const optionEls = fontSelect.querySelectorAll("option");
  trigger.dispatchEvent({ type: "mousedown", target: trigger });
  assert.equal(root.classList.contains("open"), true);
  trigger.dispatchEvent({ type: "keydown", key: "End", target: trigger });
  assert.equal(fontSelect.value, optionEls.at(-1).value);
  trigger.dispatchEvent({ type: "keydown", key: "Home", target: trigger });
  assert.equal(fontSelect.value, optionEls[0].value);
  trigger.dispatchEvent({ type: "dblclick", target: trigger });
  assert.equal(root.classList.contains("open"), false);
  assert.equal(dblClicked, 1);

  const dialog = document.getElementById("about-dialog");
  const wrap = dialog.querySelector(".dialog-wrap");
  const title = dialog.querySelector(".dialog-title");
  dialog.showModal();
  title.dispatchEvent({ type: "mousedown", button: 0, clientX: 100, clientY: 120, target: title });
  document.dispatchEvent({ type: "mousemove", clientX: 135, clientY: 145 });
  assert.equal(wrap.style.transform, "translate(35px, 25px)");
  document.dispatchEvent({ type: "mouseup", clientX: 135, clientY: 145 });
  dialog.close();
  assert.equal(wrap.style.transform, "translate(35px, 25px)", "the dragged position is kept after closing, as in the reference");
});

test("font-mono token, status bar interactive position/zoom, proportional fit zoom, style-bar custom inputs, and recent-dialog drag match rhwp-studio", async () => {
  assert.match(
    TOKENS_CSS,
    /--font-mono:\s*'SFMono-Regular',\s*Consolas,\s*'Liberation Mono',\s*Menlo,\s*monospace;/
  );
  const posRule = cssRule("#status-studio-pos, #status-zoom-label", SHELL_CSS);
  assert.ok(hasDecl(posRule, "cursor", "pointer") && hasDecl(posRule, "border-radius", "var(--radius-xs)") && hasDecl(posRule, "padding", "1px 6px"));

  const docSelection = { _range: null, get rangeCount() { return this._range ? 1 : 0; }, getRangeAt() { return this._range; }, removeAllRanges() { this._range = null; }, addRange(r) { this._range = r; } };
  const { document, userClick, sandbox } = await createWorkspaceHarness({ search: "?module=DOC&win=w-parity-controls", selection: docSelection, transformHtml: demoHtml });
  const outlineDialog = document.getElementById("outline-dialog");
  assert.equal(Boolean(outlineDialog.open), false);
  userClick(document.getElementById("status-studio-pos"));
  assert.equal(Boolean(outlineDialog.open), true);
  outlineDialog.close();

  const zoomSlider = document.getElementById("status-zoom-slider");
  const zoomLabel = document.getElementById("status-zoom-label");
  zoomSlider.value = "155";
  zoomSlider.dispatchEvent({ type: "input" });
  assert.equal(zoomLabel.textContent, "155%");
  userClick(zoomLabel);
  assert.equal(zoomLabel.textContent, "100%");
  assert.equal(zoomSlider.value, "100");

  const docScroll = document.querySelector(".doc-scroll-container");
  const docEditor = document.getElementById("doc-editor");
  docEditor.offsetWidth = 800;
  docEditor.offsetHeight = 1000;
  docScroll.clientWidth = 1000;
  docScroll.clientHeight = 820;
  userClick(document.getElementById("btn-zoom-fit-width"));
  assert.equal(zoomLabel.textContent, "120%");
  userClick(document.getElementById("btn-zoom-fit-page"));
  assert.equal(zoomLabel.textContent, "80%");
  docScroll.clientWidth = 0;
  docScroll.clientHeight = 0;

  const titleText = document.querySelector("#doc-editor .eo-h-title").childNodes[0];
  docSelection.addRange({ startContainer: titleText, startOffset: 1, endContainer: titleText, endOffset: 3, collapsed: false });
  const sizeInput = document.getElementById("doc-font-size-input");
  sizeInput.value = "144";
  sizeInput.dispatchEvent({ type: "keydown", key: "Enter", target: sizeInput, preventDefault() {} });
  assert.equal(sizeInput.value, "144.0");
  assert.equal(docEditor.querySelector(".eo-run").dataset.eoSize, "288");

  const lsSelect = document.getElementById("doc-linespacing-select");
  const lsRoot = lsSelect.parentElement;
  lsSelect.dispatchEvent({ type: "dblclick", target: lsSelect, preventDefault() {} });
  const lsInput = lsRoot.parentElement.querySelector("input.sb-ls-select");
  assert.ok(lsInput, "double-clicking line spacing opens inline custom input");
  lsInput.value = "225";
  lsInput.dispatchEvent({ type: "keydown", key: "Enter", target: lsInput, preventDefault() {} });
  assert.equal(lsSelect.value, "225");
  assert.equal(titleText.parentElement.closest("h1").style.lineHeight, "2.25", "the custom value applies to the selected paragraph");

  const recentPromise = document.defaultView.__EASYOFFICE_RECENT_DIALOG__.open(document, []);
  const recentDlg = document.querySelector(".recent-documents-dialog.recent-dialog");
  assert.ok(recentDlg);
  const recentHeader = recentDlg.querySelector(".dialog-header");
  assert.ok(recentHeader);
  recentHeader.dispatchEvent({ type: "mousedown", button: 0, clientX: 50, clientY: 60, target: recentHeader });
  document.dispatchEvent({ type: "mousemove", clientX: 90, clientY: 110 });
  assert.equal(recentDlg.style.transform, "translate(40px, 50px)");
  document.dispatchEvent({ type: "mouseup", clientX: 90, clientY: 110 });
  recentDlg.querySelector(".dialog-close").click();
  await recentPromise;
  assert.equal(document.querySelector(".recent-documents-dialog"), null);
});

test("menu-bar .md-icon & menuitemcheckbox centering, .sb-ls-icon in #doc-linespacing-group, and zoom-aware ruler scale match rhwp-studio", async () => {
  const mdIconRule = cssRule(".md-icon", MENU_CSS);
  assert.ok(hasDecl(mdIconRule, "top", "50%"), ".md-icon top: 50%");
  assert.ok(hasDecl(mdIconRule, "transform", "translateY(-50%)"), ".md-icon translateY(-50%)");
  assert.ok(hasDecl(mdIconRule, "display", "inline-flex"), ".md-icon display: inline-flex");
  assert.ok(hasDecl(mdIconRule, "align-items", "center"), ".md-icon align-items: center");
  assert.ok(hasDecl(mdIconRule, "justify-content", "center"), ".md-icon justify-content: center");

  assert.ok(!MENU_CSS.includes("\u2713"), "no check glyph in the menu");
  const activeRule = /\.md-item\.active,\s*\.md-item\[role="menuitemradio"\]\[aria-checked="true"\],\s*\.md-item\[role="menuitemcheckbox"\]\[aria-checked="true"\]\s*\{([^}]*)\}/.exec(MENU_CSS);
  assert.ok(activeRule && hasDecl(activeRule[1], "background", "var(--ui-selected)") && hasDecl(activeRule[1], "color", "var(--color-primary-dark)"), "checked items share the active look");
  assert.ok(!readText(fileURLToPath(new URL("update-notice.js", UI_DIR))).includes("\u2713"), "the update toggle shows no check glyph");

  const lsIconRule = cssRule(".sb-ls-icon", TOOLBAR_CSS);
  assert.ok(hasDecl(lsIconRule, "display", "inline-flex"), ".sb-ls-icon styled in toolbar.css");

  const docSelection = { _range: null, get rangeCount() { return this._range ? 1 : 0; }, getRangeAt() { return this._range; }, removeAllRanges() { this._range = null; }, addRange(r) { this._range = r; } };
  const { document } = await createWorkspaceHarness({ search: "?module=DOC&win=w-ruler-zoom", selection: docSelection });
  const lsGroup = document.getElementById("doc-linespacing-group");
  assert.ok(lsGroup, "#doc-linespacing-group exists");
  const lsIcon = lsGroup.querySelector(".sb-ls-icon");
  assert.ok(lsIcon && lsIcon.dataset.icon === "line-spacing" && lsIcon.querySelector("svg"), ".sb-ls-icon rendered with line-spacing SVG");
  const lsSelect = document.getElementById("doc-linespacing-select");
  assert.equal(lsSelect.parentElement.parentElement, lsGroup, "custom-select wrapper stays inside #doc-linespacing-group");

  const docRunsUi = document.defaultView.__EASYOFFICE_DOC_RUNS_UI__;
  assert.equal(typeof docRunsUi?.resolveRulerScale, "function", "resolveRulerScale exported");
  assert.deepEqual(docRunsUi.resolveRulerScale(1.0), { labelStepMm: 10, tickStepMm: 1 });
  assert.deepEqual(docRunsUi.resolveRulerScale(0.5), { labelStepMm: 20, tickStepMm: 2 });
  assert.deepEqual(docRunsUi.resolveRulerScale(0.25), { labelStepMm: 50, tickStepMm: 5 });
  assert.deepEqual(docRunsUi.resolveRulerScale(2.0), { labelStepMm: 10, tickStepMm: 0.5 });
  assert.deepEqual(docRunsUi.resolveRulerScale(0), docRunsUi.resolveRulerScale(1));

  const hTrack = document.querySelector("#h-ruler .ruler-track");
  const vSpans = () => [...document.querySelectorAll("#v-ruler span")];
  assert.equal(vSpans().length, 13);
  assert.ok(hTrack.textContent.startsWith("1 · · · 2 · · · 3"), hTrack.textContent);
  assert.deepEqual(vSpans().slice(0, 3).map((s) => s.textContent), ["1", "2", "3"]);

  const zoomSlider = document.getElementById("status-zoom-slider");
  zoomSlider.value = "50";
  zoomSlider.dispatchEvent({ type: "input" });
  assert.equal(vSpans().length, 13, "#v-ruler keeps 13 spans at 50% zoom");
  assert.ok(hTrack.textContent.startsWith("2 · · · 4 · · · 6"), `50% zoom h-ruler: ${hTrack.textContent}`);
  assert.deepEqual(vSpans().slice(0, 3).map((s) => s.textContent), ["2", "4", "6"]);

  zoomSlider.value = "200";
  zoomSlider.dispatchEvent({ type: "input" });
  assert.ok(hTrack.textContent.startsWith("1 · · · 2 · · · 3"));
  const docEditor = document.getElementById("doc-editor");
  const p = docEditor.querySelector("p");
  docSelection.addRange({ startContainer: p, startOffset: 0, endContainer: p, endOffset: 0, collapsed: true });
  const leftPin = document.getElementById("ruler-pin-left-margin");
  leftPin.dispatchEvent({ type: "pointerdown", button: 0, clientX: 100, target: leftPin, preventDefault() {} });
  leftPin.dispatchEvent({ type: "pointermove", clientX: 132, target: leftPin });
  leftPin.dispatchEvent({ type: "pointerup", clientX: 132, target: leftPin });
  assert.equal(p.style.paddingLeft, "16px", "32px drag at 200% zoom equals 1 indent step (16px)");
});

test("reference app CSS parity (disabled menu shortcuts/arrows, style-bar sep, zoom slider thumb, glyph hover/active, ruler tokens) and O(1) palette/format helpers", async () => {
  assert.match(
    MENU_CSS,
    /\.md-item\[aria-disabled="true"\]\s+\.md-shortcut,\s*\.md-item\.disabled\s+\.md-shortcut,\s*\.md-item:disabled\s+\.md-shortcut\s*\{\s*color:\s*var\(--ui-border\);\s*\}/
  );
  assert.match(
    MENU_CSS,
    /\.md-sub\[aria-disabled="true"\]\s*>\s*\.md-arrow,\s*\.md-sub\.disabled\s*>\s*\.md-arrow\s*\{\s*color:\s*var\(--ui-border-light\);\s*\}/
  );
  const sepRule = cssRule("#doc-style-bar > .eo-style-sep", TOOLBAR_CSS);
  assert.ok(hasDecl(sepRule, "width", "1px") && hasDecl(sepRule, "height", "20px") && hasDecl(sepRule, "background", "var(--ui-border)"));
  const sliderRule = cssRule(".statusbar-zoom-slider", TOOLBAR_CSS);
  assert.ok(hasDecl(sliderRule, "width", "88px") && hasDecl(sliderRule, "height", "2px") && hasDecl(sliderRule, "background", "var(--ui-border-strong)"));
  const thumbRule = cssRule(".statusbar-zoom-slider::-webkit-slider-thumb", TOOLBAR_CSS);
  assert.ok(hasDecl(thumbRule, "width", "12px") && hasDecl(thumbRule, "height", "12px") && hasDecl(thumbRule, "border-radius", "var(--radius-pill)"));
  const cpShortcutRule = cssRule(".cp-item-shortcut", TOOLBAR_CSS);
  assert.ok(hasDecl(cpShortcutRule, "border-radius", "var(--radius-md)"));

  const glyphHoverRule = cssRule(".eo-glyph-btn:hover", SHELL_CSS);
  assert.ok(hasDecl(glyphHoverRule, "background", "var(--ui-hover-strong)") && hasDecl(glyphHoverRule, "outline", "1px solid var(--ui-border-light)"));
  const glyphActiveRule = cssRule('.eo-glyph-btn.active, .eo-glyph-btn[aria-pressed="true"]', SHELL_CSS);
  assert.ok(hasDecl(glyphActiveRule, "background", "var(--ui-active)") && hasDecl(glyphActiveRule, "color", "var(--accent-ink)") && hasDecl(glyphActiveRule, "outline", "1px solid var(--ui-border-strong)"));
  const zoomBtnHover = cssRule(".statusbar-zoom-btn:hover", SHELL_CSS);
  assert.ok(hasDecl(zoomBtnHover, "outline", "1px solid var(--ui-border-strong)"));
  const pageRulerRule = cssRule(".page-ruler", CSS);
  assert.ok(hasDecl(pageRulerRule, "background", "var(--ruler-bg)") && hasDecl(pageRulerRule, "color", "var(--ruler-text)"));
  const rulerTrackRule = cssRule(".ruler-track", CSS);
  assert.ok(hasDecl(rulerTrackRule, "background", "var(--ruler-body)") && hasDecl(rulerTrackRule, "color", "var(--ruler-text)"));
  const rulerPinRule = cssRule(".ruler-pin", CSS);
  assert.ok(hasDecl(rulerPinRule, "color", "var(--ruler-marker)"));
  const vRulerRule = cssRule(".v-ruler", CSS);
  assert.ok(hasDecl(vRulerRule, "color", "var(--ruler-text)"));

  const { document, userClick, sandbox } = await createWorkspaceHarness({ search: "?module=DOC&win=w-o1-helpers" });
  userClick(document.getElementById("btn-command-palette"));
  const listEl = document.getElementById("cmd-palette-list");
  const firstLiBefore = listEl.children[0];
  const secondLiBefore = listEl.children[1];
  assert.ok(firstLiBefore && secondLiBefore, "command palette renders items");
  assert.equal(firstLiBefore.getAttribute("aria-selected"), "true");
  sandbox.dispatchWindowKeydown({ key: "ArrowDown" });
  assert.equal(listEl.children[0], firstLiBefore, "ArrowDown preserves existing DOM nodes in O(1)");
  assert.equal(listEl.children[1], secondLiBefore, "ArrowDown preserves second DOM node in O(1)");
  assert.equal(firstLiBefore.getAttribute("aria-selected"), "false");
  assert.equal(secondLiBefore.getAttribute("aria-selected"), "true");
  assert.equal(secondLiBefore.classList.contains("cp-item--selected"), true);

  const tbActiveRule = cssRule(".tb-btn:active:not(:disabled)", TOOLBAR_CSS);
  assert.ok(hasDecl(tbActiveRule, "background", "var(--ui-active)") && hasDecl(tbActiveRule, "border-color", "var(--ui-border)"));
  const zoomHoverNotDisabled = cssRule(".statusbar-zoom-btn:hover:not(:disabled)", TOOLBAR_CSS);
  assert.ok(hasDecl(zoomHoverNotDisabled, "background", "var(--ui-hover)") && hasDecl(zoomHoverNotDisabled, "color", "var(--color-text)"));
  const zoomActiveNotDisabled = cssRule(".statusbar-zoom-btn:active:not(:disabled)", TOOLBAR_CSS);
  assert.ok(hasDecl(zoomActiveNotDisabled, "background", "var(--ui-active)"));
  const zoomFocusVisible = cssRule(".statusbar-zoom-slider:focus-visible", TOOLBAR_CSS);
  assert.ok(hasDecl(zoomFocusVisible, "outline", "1px solid var(--color-focus-border)") && hasDecl(zoomFocusVisible, "outline-offset", "2px"));
  const dlgPlaceholder = cssRule(".tool-dialog input::placeholder, .tool-dialog textarea::placeholder", SHELL_CSS);
  assert.ok(hasDecl(dlgPlaceholder, "color", "var(--color-text-placeholder)"));
  const dlgDisabled = cssRule(".tool-dialog input:disabled, .tool-dialog select:disabled, .tool-dialog textarea:disabled", SHELL_CSS);
  assert.ok(
    hasDecl(dlgDisabled, "background", "var(--ui-surface-muted)") &&
      hasDecl(dlgDisabled, "border-color", "var(--color-border-lighter)") &&
      hasDecl(dlgDisabled, "color", "var(--color-text-disabled)") &&
      hasDecl(dlgDisabled, "cursor", "not-allowed")
  );
  const cpSelectedLabel = cssRule(".cp-item:hover .cp-item-label, .cp-item--selected .cp-item-label", SHELL_CSS);
  assert.ok(hasDecl(cpSelectedLabel, "color", "var(--color-primary-dark)"));
});

