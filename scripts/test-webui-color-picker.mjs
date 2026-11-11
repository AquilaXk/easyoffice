import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { readText } from "./test-support/read-text.mjs";

// The style bar's colour pickers (PD-165): palette popup, recent colours, keyboard, outside press.

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const read = (name) => readText(fileURLToPath(new URL(name, UI_DIR)));
const RECENT_KEY = "easyoffice.recent-text-colors";

function memoryStorage(initial = {}) {
  const map = new Map(Object.entries(initial));
  return { map, getItem: (k) => (map.has(k) ? map.get(k) : null), setItem: (k, v) => map.set(k, String(v)) };
}

async function openDoc(localStorage = memoryStorage()) {
  const harness = await createWorkspaceHarness({ search: "?module=DOC&win=w-color-picker", localStorage });
  await flushAsyncTurns(10);
  return { ...harness, localStorage };
}

const parts = (document, kind) => {
  const host = document.getElementById(`doc-${kind}-host`);
  return {
    host,
    toggle: host.querySelector("[data-color-toggle]"),
    panel: host.querySelector(".sb-hl-palette"),
    input: host.querySelector("input"),
  };
};

test("the visible native colour inputs are gone: a toggle arrow, a palette and a hidden input per picker", async () => {
  const { document } = await openDoc();
  for (const kind of ["textcolor", "highlight"]) {
    const { toggle, panel, input } = parts(document, kind);
    assert.equal(toggle.getAttribute("aria-expanded"), "false");
    assert.equal(toggle.getAttribute("aria-controls"), panel.id);
    assert.ok(panel.hidden, "the palette starts closed");
    assert.ok(input.classList.contains("eo-color-input") && input.getAttribute("aria-hidden") === "true");
    assert.equal(input.tabIndex, -1);
    assert.equal(panel.querySelectorAll(".sb-hl-swatch").length, 7 * 7, "gray row plus six palette rows");
  }
  assert.ok(!read("index.html").includes("sb-color-input"));
});

test("a palette swatch applies the colour to the selection, closes the popup and fills the recent row", async () => {
  const { document, userClick, localStorage } = await openDoc();
  const { toggle, panel, input } = parts(document, "textcolor");
  userClick(toggle);
  assert.ok(!panel.hidden);
  assert.equal(toggle.getAttribute("aria-expanded"), "true");
  assert.equal(panel.querySelector(".eo-recent-colors .sb-hl-palette-row").textContent, "아직 없음");
  const swatch = [...panel.querySelectorAll(".sb-hl-swatch")].find((s) => s.getAttribute("aria-label") === "#0000ff");
  userClick(swatch);
  assert.ok(panel.hidden, "choosing closes the popup");
  assert.equal(input.value, "#0000ff");
  assert.equal(document.getElementById("btn-stylebar-textcolor").style.borderBottomColor, "#0000ff");
  assert.deepEqual(JSON.parse(localStorage.getItem(RECENT_KEY)), ["#0000ff"]);
  assert.equal(panel.querySelectorAll(".eo-recent-colors .sb-hl-swatch").length, 1);
});

test("recent colours: newest first, no duplicates, seven at most, bad stored entries dropped", async () => {
  const stored = JSON.stringify(["#111111", "not-a-colour", "#111111", "#222222", 5]);
  const { document, userClick, localStorage } = await openDoc(memoryStorage({ [RECENT_KEY]: stored }));
  const { toggle, panel } = parts(document, "textcolor");
  const recent = () => [...panel.querySelectorAll(".eo-recent-colors .sb-hl-swatch")].map((s) => s.getAttribute("aria-label"));
  assert.deepEqual(recent(), ["#111111", "#222222"]);
  for (const color of ["#ff0000", "#00ff00", "#0000ff", "#ff00ff", "#00ffff", "#ffff00"]) {
    userClick(toggle);
    userClick([...panel.querySelectorAll(".sb-hl-palette-row .sb-hl-swatch")].find((s) => s.getAttribute("aria-label") === color));
  }
  assert.equal(recent().length, 7);
  assert.equal(recent()[0], "#ffff00");
  userClick(toggle);
  userClick([...panel.querySelectorAll(".sb-hl-swatch")].find((s) => s.getAttribute("aria-label") === "#ff0000"));
  assert.equal(recent()[0], "#ff0000", "a repeated colour moves to the front");
  assert.equal(new Set(recent()).size, recent().length);
  assert.equal(JSON.parse(localStorage.getItem(RECENT_KEY)).length, 7);
});

test("recent colours work without storage: the choice still applies and shows in the row", async () => {
  const broken = { getItem() { throw new Error("blocked"); }, setItem() { throw new Error("blocked"); } };
  const { document, userClick } = await openDoc(broken);
  const { toggle, panel, input } = parts(document, "textcolor");
  userClick(toggle);
  userClick([...panel.querySelectorAll(".sb-hl-swatch")].find((s) => s.getAttribute("aria-label") === "#cc0000"));
  assert.equal(input.value, "#cc0000");
  assert.equal(panel.querySelectorAll(".eo-recent-colors .sb-hl-swatch").length, 1);
});

test("the native chooser commit ('다른 색...') applies like a swatch", async () => {
  const { document } = await openDoc();
  const { panel, input } = parts(document, "textcolor");
  const other = [...panel.querySelectorAll(".sb-hl-palette-actions button")].find((b) => b.textContent === "다른 색...");
  assert.ok(other, "the palette offers the native chooser");
  input.value = "#123456";
  input.dispatchEvent({ type: "change", target: input });
  await flushAsyncTurns();
  assert.equal(document.getElementById("btn-stylebar-textcolor").style.borderBottomColor, "#123456");
});

test("the highlight picker has '색 없음' and no recent row", async () => {
  const { document, userClick } = await openDoc();
  const { toggle, panel } = parts(document, "highlight");
  assert.equal(panel.querySelector(".eo-recent-colors"), null);
  userClick(toggle);
  const swatch = [...panel.querySelectorAll(".sb-hl-swatch")].find((s) => s.getAttribute("aria-label") === "#ffff00");
  userClick(swatch);
  assert.equal(document.getElementById("btn-stylebar-highlight").style.borderBottomColor, "#ffff00");
  userClick(toggle);
  userClick([...panel.querySelectorAll(".sb-hl-palette-actions button")].find((b) => b.textContent === "색 없음"));
  assert.ok(panel.hidden);
  assert.equal(document.getElementById("btn-stylebar-highlight").style.borderBottomColor, "");
});

test("keyboard: ArrowDown opens and focuses the first swatch, Escape closes and returns focus", async () => {
  const { document, userClick } = await openDoc();
  const { toggle, panel } = parts(document, "textcolor");
  toggle.focus();
  toggle.dispatchEvent({ type: "keydown", key: "ArrowDown" });
  assert.ok(!panel.hidden);
  assert.equal(document.activeElement, panel.querySelector("button"));
  document.dispatchEvent({ type: "keydown", key: "Escape", target: document.activeElement, stopPropagation() {} });
  assert.ok(panel.hidden);
  assert.equal(toggle.getAttribute("aria-expanded"), "false");
  assert.equal(document.activeElement, toggle);
  userClick(toggle);
  document.dispatchEvent({ type: "keydown", key: "a", target: toggle });
  assert.ok(!panel.hidden, "other keys leave the popup open");
});

test("an outside press closes the popup, a press inside does not, and only one popup is open", async () => {
  const { document, userClick } = await openDoc();
  const text = parts(document, "textcolor");
  const highlight = parts(document, "highlight");
  userClick(text.toggle);
  document.dispatchEvent({ type: "mousedown", target: text.panel.querySelector(".sb-hl-swatch") });
  assert.ok(!text.panel.hidden, "a press inside the picker keeps it open");
  userClick(highlight.toggle);
  assert.ok(text.panel.hidden && !highlight.panel.hidden, "opening one closes the other");
  document.dispatchEvent({ type: "mousedown", target: document.getElementById("doc-editor") });
  assert.ok(highlight.panel.hidden);
  userClick(highlight.toggle);
  userClick(highlight.toggle);
  assert.ok(highlight.panel.hidden, "the arrow toggles");
});

test("color-picker.js names the reference app as its source and is in the notices", () => {
  const source = read("color-picker.js");
  assert.match(source, /reference app/);
  assert.match(source, /MIT/);
  assert.ok(readText(fileURLToPath(new URL("../THIRD_PARTY_NOTICES.md", import.meta.url))).includes("ui/color-picker.js"));
});
