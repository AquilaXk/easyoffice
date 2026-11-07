import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";

// Theme tokens and mode (PD-098): rhwp-studio tokens in one file, the mode on <html>, no literal
// colours outside tokens.css and content.css, no leftovers of the old lavender variables.

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const read = (name) => fs.readFileSync(fileURLToPath(new URL(name, UI_DIR)), "utf8");
const STORAGE_KEY = "easyoffice.theme-mode";

function memoryStorage() {
  const map = new Map();
  return {
    map,
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, String(v)),
    removeItem: (k) => map.delete(k),
  };
}

function fakeMediaQuery(initialDark) {
  const listeners = [];
  const query = {
    matches: initialDark,
    addEventListener: (_type, fn) => listeners.push(fn),
    flip(dark) {
      query.matches = dark;
      listeners.forEach((fn) => fn({ matches: dark }));
    },
  };
  return query;
}

const stripComments = (css) => css.replaceAll(/\/\*[\s\S]*?\*\//g, "");


// `--name: value` declarations of a CSS block, split on the punctuation instead of a regex.
function cssDeclarations(block) {
  const out = [];
  let text = block;
  for (let start = text.indexOf("/*"); start >= 0; start = text.indexOf("/*")) {
    const end = text.indexOf("*/", start + 2);
    text = text.slice(0, start) + (end < 0 ? "" : text.slice(end + 2));
  }
  for (const part of text.split(/[;{}]/)) {
    const colon = part.indexOf(":");
    if (colon < 0) continue;
    const name = part.slice(0, colon).trim();
    if (name.startsWith("--")) out.push([name, part.slice(colon + 1).trim()]);
  }
  return out;
}

test("the stored mode and the OS scheme decide the effective theme at load", async () => {
  const dark = await createWorkspaceHarness({ localStorage: memoryStorage(), matchMedia: () => fakeMediaQuery(true) });
  assert.equal(dark.document.documentElement.dataset.themeMode, "system");
  assert.equal(dark.document.documentElement.dataset.themeEffective, "dark");
  assert.equal(dark.document.documentElement.style.colorScheme, "only dark");

  const storage = memoryStorage();
  storage.setItem(STORAGE_KEY, "light");
  const light = await createWorkspaceHarness({ localStorage: storage, matchMedia: () => fakeMediaQuery(true) });
  assert.equal(light.document.documentElement.dataset.themeMode, "light");
  assert.equal(light.document.documentElement.dataset.themeEffective, "light");

  const bad = memoryStorage();
  bad.setItem(STORAGE_KEY, "neon");
  const fallback = await createWorkspaceHarness({ localStorage: bad });
  assert.equal(fallback.document.documentElement.dataset.themeMode, "system");
  assert.equal(fallback.document.documentElement.dataset.themeEffective, "light");
});

test("the theme submenu is a radio group that stores the choice and follows the OS only in system mode", async () => {
  const storage = memoryStorage();
  const media = fakeMediaQuery(false);
  const { document, userClick } = await createWorkspaceHarness({ localStorage: storage, matchMedia: () => media });
  const html = document.documentElement;
  const choice = (mode) => document.querySelector(`[data-theme-mode-choice="${mode}"]`);
  assert.deepEqual(
    [...document.querySelectorAll("[data-theme-mode-choice]")].map((el) => el.getAttribute("role")),
    ["menuitemradio", "menuitemradio", "menuitemradio"]
  );
  assert.equal(choice("system").getAttribute("aria-checked"), "true");

  media.flip(true);
  assert.equal(html.dataset.themeEffective, "dark", "system mode follows the OS");

  userClick(choice("light"));
  await flushAsyncTurns(10);
  assert.equal(storage.getItem(STORAGE_KEY), "light");
  assert.equal(html.dataset.themeEffective, "light");
  assert.equal(choice("light").getAttribute("aria-checked"), "true");
  assert.equal(choice("system").getAttribute("aria-checked"), "false");

  media.flip(false);
  media.flip(true);
  assert.equal(html.dataset.themeEffective, "light", "an explicit mode ignores OS changes");

  userClick(choice("dark"));
  await flushAsyncTurns(10);
  assert.equal(html.dataset.themeEffective, "dark");
  userClick(choice("system"));
  await flushAsyncTurns(10);
  assert.equal(storage.getItem(STORAGE_KEY), "system");
  assert.equal(html.dataset.themeEffective, "dark", "back to following the OS (dark now)");
});

test("the toggle makes the choice explicit and unknown modes are ignored", async () => {
  const storage = memoryStorage();
  const { document } = await createWorkspaceHarness({ localStorage: storage });
  const api = document.defaultView.__EASYOFFICE_THEME__;
  assert.equal(api.getEffective(), "light");
  api.toggle();
  assert.equal(storage.getItem(STORAGE_KEY), "dark");
  assert.equal(document.documentElement.dataset.themeEffective, "dark");
  api.setMode("neon");
  assert.equal(api.getMode(), "dark");
});

test("the CSS uses tokens only: no literal colours outside tokens.css and content.css, no lavender variables", () => {
  const hex = /#[0-9a-fA-F]{3,8}\b/;
  for (const name of ["styles.css", "shell.css"]) {
    const css = stripComments(read(name));
    assert.ok(!hex.test(css), `${name} has a literal colour: ${hex.exec(css)?.[0]}`);
    assert.ok(!/\brgba?\(\s*\d/.test(css.replaceAll(/rgba\(\s*0\s*,\s*0\s*,\s*0\s*,/g, "")), `${name} has a literal rgb()`);
    assert.ok(!css.includes("data-theme=\""), `${name} still keys on data-theme`);
  }
  for (const name of ["styles.css", "shell.css", "content.css", "tokens.css"]) {
    assert.ok(!read(name).includes("--eo-"), `${name} still uses an --eo- variable`);
  }
  for (const name of ["app.js", "ribbon-history.js", "ribbon-catalog.js", "shell-chrome.js", "workspace-panels.js", "slide-studio.js"]) {
    assert.ok(!/--eo-|dataset\.theme\b/.test(read(name)), `${name} uses a removed theme variable`);
  }
});

test("every var(--x) in the CSS is a token, or a custom property the CSS defines itself", () => {
  const tokenText = read("tokens.css");
  const defined = new Set(cssDeclarations(tokenText).map(([name]) => name));
  const all = ["styles.css", "shell.css", "content.css"].map((n) => stripComments(read(n))).join("\n");
  for (const [name] of cssDeclarations(all)) defined.add(name);
  // A var() with a fallback is an optional hook; only bare references must be defined.
  for (const m of all.matchAll(/var\((--[a-z0-9-]+)\s*\)/g)) assert.ok(defined.has(m[1]), `${m[1]} is not defined`);
});

test("tokens.css keeps the rhwp-studio values: light default, dark under data-theme-effective", () => {
  const css = read("tokens.css");
  assert.match(css, /--ui-surface:\s*#ffffff;/);
  assert.match(css, /--accent-primary:\s*#6182d6;/);
  assert.match(css, /--doc-paper:\s*#ffffff;/);
  assert.match(css, /:root\[data-theme-effective="dark"\]\s*\{[^}]*--ui-bg:\s*#25292f;/);
  assert.match(css, /--font-family-ui:[^;]*Malgun Gothic/);
  assert.match(css, /Adapted from rhwp-studio/);
});

test("the document page keeps the light palette in the dark theme", () => {
  const tokens = read("tokens.css");
  const light = tokens.slice(tokens.indexOf("\n:root {"), tokens.indexOf('\n:root[data-theme-effective="dark"] {'));
  const content = read("content.css");
  const pageStart = content.indexOf("#doc-editor {");
  const page = content.slice(content.indexOf("{", pageStart) + 1, content.indexOf("\n}", pageStart));
  const pageTokens = Object.fromEntries([...page.matchAll(/(--[a-z-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
  assert.ok(Object.keys(pageTokens).length > 30, "the page redeclares the palette");
  for (const [name, value] of Object.entries(pageTokens)) {
    const lightValue = new RegExp(String.raw`${name}:\s*([^;]+);`).exec(light)?.[1].trim();
    assert.equal(value, lightValue, `${name} on the page matches the light token`);
  }
});

test("index.html loads theme.js before the style sheets and has the three theme choices", () => {
  const html = read("index.html");
  assert.ok(html.indexOf("./theme.js") < html.indexOf("./tokens.css"));
  assert.ok(html.indexOf("./tokens.css") < html.indexOf("./styles.css"));
  assert.ok(html.indexOf("./content.css") > html.indexOf("./shell.css"));
  for (const mode of ["system", "light", "dark"]) assert.ok(html.includes(`data-theme-mode-choice="${mode}"`), mode);
  assert.ok(!html.includes("data-theme="), "the old data-theme attribute is gone");
});
