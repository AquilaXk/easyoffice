import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, createWebUiDom, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { readText } from "./test-support/read-text.mjs";

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const STYLES_CSS = readText(fileURLToPath(new URL("styles.css", UI_DIR)));
const AUTOSAVE_KEY = "easyoffice.workspace.autosave.v1";

function createMemoryStorage() {
  const map = new Map();
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => {
      map.set(k, String(v));
    },
    removeItem: (k) => {
      map.delete(k);
    },
  };
}

async function openSlideModule(options = {}) {
  const h = await createWorkspaceHarness(options);
  switchModule(h.document, "SLIDE");
  await flushAsyncTurns();
  return h;
}

const $ = (document, id) => document.getElementById(id);
const thumbs = (document) => Array.from(document.querySelectorAll("#slide-thumb-list .slide-thumb"));
const thumbTitles = (document) =>
  thumbs(document).map((t) => t.querySelector(".slide-thumb-title").textContent);
const isShowing = (document) => !$(document, "slide-show-overlay").hidden;

function typeTitle(document, text) {
  const zone = $(document, "slide-title-zone");
  zone.focus();
  const h2 = zone.querySelector("h2");
  zone.dispatchEvent({ type: "beforeinput", target: h2 });
  h2.textContent = text;
  zone.dispatchEvent({ type: "input", target: h2 });
  return zone;
}

test("F5 / Shift+F5 are registered in the single shortcut table, scoped to the slide module and text-safe", async () => {
  const { sandbox } = await createWorkspaceHarness();
  const table = sandbox.__EASYOFFICE_RIBBON_CATALOG__.WORKSPACE_SHORTCUT_BINDINGS;
  for (const keys of ["F5", "Shift+F5", "Alt+ArrowUp", "Alt+ArrowDown", "Delete"]) {
    const b = table.find((x) => x.keys === keys);
    assert.ok(b, keys + " must be in the table");
    assert.equal(b.module, "SLIDE", keys + " is slide-module scoped");
  }
  assert.equal(table.find((x) => x.keys === "F5").bareKey, true, "F5 never steals reload from text fields");
  assert.equal(table.find((x) => x.keys === "Shift+F5").bareKey, true);
  assert.equal(table.some((x) => x.keys === "Ctrl+F5"), false);
});

test("F5 starts the show from slide 1, Shift+F5 from the current slide; reload is only prevented in the slide module outside text fields", async () => {
  const { document, sandbox, userClick } = await createWorkspaceHarness();
  assert.equal(sandbox.dispatchWindowKeydown({ key: "F5" }).defaultPrevented, false, "DOC module keeps browser reload");
  assert.equal(isShowing(document), false);

  switchModule(document, "SLIDE");
  await flushAsyncTurns();
  userClick(document.querySelector('.slide-thumb[data-slide-idx="2"]'));
  await flushAsyncTurns();

  const notes = $(document, "slide-presenter-notes");
  notes.focus();
  assert.equal(sandbox.dispatchWindowKeydown({ key: "F5", target: notes }).defaultPrevented, false, "text field keeps reload");
  assert.equal(isShowing(document), false);
  notes.blur();

  const ev = sandbox.dispatchWindowKeydown({ key: "F5" });
  assert.equal(ev.defaultPrevented, true);
  assert.equal(isShowing(document), true);
  assert.equal($(document, "slide-show-counter").textContent, "1 / 3");
  assert.match($(document, "slide-show-title").textContent, /연간 제품 전략/);
  sandbox.dispatchWindowKeydown({ key: "Escape" });
  assert.equal(isShowing(document), false);

  assert.equal(sandbox.dispatchWindowKeydown({ key: "F5", shiftKey: true }).defaultPrevented, true);
  assert.equal($(document, "slide-show-counter").textContent, "3 / 3");
  assert.match($(document, "slide-show-title").textContent, /대용량 문서/);
  sandbox.dispatchWindowKeydown({ key: "Escape" });

  userClick($(document, "btn-slide-show"));
  assert.equal($(document, "slide-show-counter").textContent, "1 / 3");
  sandbox.dispatchWindowKeydown({ key: "Escape" });
  userClick($(document, "btn-slide-show-current"));
  assert.equal($(document, "slide-show-counter").textContent, "3 / 3");
});

test("slideshow navigation keys, click, black/white screen, end of show and Esc", async () => {
  const { document, sandbox, userClick } = await openSlideModule();
  const key = (k, extra = {}) => sandbox.dispatchWindowKeydown({ key: k, ...extra });
  const counter = () => $(document, "slide-show-counter").textContent;
  userClick($(document, "btn-slide-show"));

  assert.equal(key("ArrowRight").defaultPrevented, true);
  assert.equal(counter(), "2 / 3");
  key(" ");
  assert.equal(counter(), "3 / 3");
  key("ArrowLeft");
  key("PageUp");
  assert.equal(counter(), "1 / 3");
  key("PageDown");
  assert.equal(counter(), "2 / 3");
  key("End");
  assert.equal(counter(), "3 / 3");
  key("Home");
  assert.equal(counter(), "1 / 3");
  key("ArrowLeft");
  assert.equal(counter(), "1 / 3", "does not wrap before the first slide");

  $(document, "slide-show-viewport").click();
  assert.equal(counter(), "2 / 3");

  key("b");
  assert.equal($(document, "slide-show-overlay").dataset.screen, "black");
  key("b");
  assert.equal($(document, "slide-show-overlay").dataset.screen, undefined);
  key("w");
  assert.equal($(document, "slide-show-overlay").dataset.screen, "white");
  key("ArrowRight");
  assert.equal($(document, "slide-show-overlay").dataset.screen, undefined, "navigating restores the slide");

  key("End");
  key("ArrowRight");
  assert.equal(isShowing(document), false, "advancing past the last slide ends the show");

  userClick($(document, "btn-slide-show"));
  key("Escape");
  assert.equal(isShowing(document), false);
  assert.equal(document.activeElement !== $(document, "slide-show-overlay"), true);
});

test("show content is the live slide text with the slide's transition", async () => {
  const { document, sandbox, userClick } = await openSlideModule();
  typeTitle(document, "수정한 제목");
  userClick($(document, "btn-slide-show"));
  assert.equal($(document, "slide-show-title").textContent, "수정한 제목");
  assert.equal($(document, "slide-show-frame").dataset.transition, "fade");
  sandbox.dispatchWindowKeydown({ key: "ArrowRight" });
  assert.equal($(document, "slide-show-frame").dataset.transition, "push");
  sandbox.dispatchWindowKeydown({ key: "ArrowRight" });
  assert.equal($(document, "slide-show-frame").dataset.transition, "zoom");
  assert.match(STYLES_CSS, /\.slide-show-frame\[data-transition="push"\]/);
  assert.match(STYLES_CSS, /prefers-reduced-motion/);
});

test("full screen uses the Fullscreen API when present, falls back in-window, and leaving full screen ends the show", async () => {
  const { document, sandbox, userClick } = await openSlideModule();
  const root = document.documentElement;
  let requests = 0;
  let exits = 0;
  root.requestFullscreen = () => {
    requests += 1;
    document.fullscreenElement = root;
    return Promise.resolve();
  };
  document.exitFullscreen = () => {
    exits += 1;
    document.fullscreenElement = null;
    return Promise.resolve();
  };
  userClick($(document, "btn-slide-show"));
  await flushAsyncTurns();
  assert.equal(requests, 1);
  assert.equal(isShowing(document), true);
  sandbox.dispatchWindowKeydown({ key: "Escape" });
  assert.equal(exits, 1);
  assert.equal(isShowing(document), false);

  userClick($(document, "btn-slide-show"));
  await flushAsyncTurns();
  document.fullscreenElement = null;
  document.dispatchEvent({ type: "fullscreenchange" });
  assert.equal(isShowing(document), false, "browser-level Esc out of full screen ends the show");

  root.requestFullscreen = () => Promise.reject(new Error("denied"));
  userClick($(document, "btn-slide-show"));
  await flushAsyncTurns();
  assert.equal(isShowing(document), true, "rejected full screen keeps the in-window show");
  sandbox.dispatchWindowKeydown({ key: "Escape" });

  delete root.requestFullscreen;
  userClick($(document, "btn-slide-show"));
  await flushAsyncTurns();
  assert.equal(isShowing(document), true, "no Fullscreen API keeps the in-window show");
});

test("presenter view shows notes, the next slide and an elapsed timer", async () => {
  const { document, sandbox, userClick, advanceTimers } = await openSlideModule();
  userClick($(document, "btn-slide-show"));
  const panel = $(document, "slide-show-presenter");
  assert.equal(panel.hidden, true);
  sandbox.dispatchWindowKeydown({ key: "p" });
  assert.equal(panel.hidden, false);
  assert.match($(document, "slide-show-notes").textContent, /핵심 메시지/);
  assert.match($(document, "slide-show-next").textContent, /문서 포맷 호환성/);
  assert.equal($(document, "slide-show-timer").textContent, "00:00");
  advanceTimers(65000);
  assert.equal($(document, "slide-show-timer").textContent, "01:05");
  sandbox.dispatchWindowKeydown({ key: "End" });
  assert.match($(document, "slide-show-next").textContent, /마지막/);
  sandbox.dispatchWindowKeydown({ key: "Escape" });
});

test("new slides start with empty placeholders instead of copying defaults", async () => {
  const { document, userClick } = await openSlideModule();
  userClick($(document, "btn-add-slide"));
  await flushAsyncTurns();
  assert.equal(thumbs(document).length, 4);
  assert.equal($(document, "slide-title-zone").textContent.trim(), "");
  assert.equal($(document, "slide-lower-zone").textContent.trim(), "");
  assert.equal($(document, "slide-presenter-notes").value, "");
  assert.doesNotMatch(document.getElementById("pane-slide").textContent, /핵심 요약 및 다이어그램 영역/);
  assert.equal(thumbs(document)[3].classList.contains("active"), true);
  assert.match(thumbTitles(document)[3], /제목 없음/);
  assert.match(STYLES_CSS, /slide-title-zone[^{]*:empty::before/);
});

test("duplicate, delete with undo, and reorder via buttons keep selection, snapshot and save order", async () => {
  const { document, sandbox, userClick } = await openSlideModule();
  userClick(document.querySelector('.slide-thumb[data-slide-idx="1"]'));
  await flushAsyncTurns();

  userClick($(document, "btn-slide-duplicate"));
  await flushAsyncTurns();
  assert.equal(thumbs(document).length, 4);
  assert.equal(thumbTitles(document)[2], thumbTitles(document)[1]);
  assert.equal(thumbs(document)[2].classList.contains("active"), true);
  assert.equal($(document, "slide-title-zone").textContent, "문서 포맷 호환성 및 일괄 변환 흐름");

  userClick($(document, "btn-slide-delete"));
  await flushAsyncTurns();
  assert.equal(thumbs(document).length, 3);
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(thumbs(document).length, 4, "undo restores the deleted slide");
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(thumbs(document).length, 3, "undo restores the pre-duplicate deck");
  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(thumbs(document).length, 4, "redo re-applies the duplicate");

  const before = thumbTitles(document);
  userClick($(document, "btn-slide-move-up"));
  await flushAsyncTurns();
  assert.deepEqual(thumbTitles(document), [before[0], before[2], before[1], before[3]]);
  assert.equal(thumbs(document)[1].classList.contains("active"), true, "selection follows the moved slide");
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.deepEqual(thumbTitles(document), before);

  userClick($(document, "btn-slide-move-down"));
  await flushAsyncTurns();
  assert.deepEqual(thumbTitles(document), [before[0], before[1], before[3], before[2]]);

  const last = document.querySelector('.slide-thumb[data-slide-idx="3"]');
  userClick(last);
  for (let i = 0; i < 5; i += 1) userClick($(document, "btn-slide-delete"));
  assert.equal(thumbs(document).length, 1, "last slide cannot be deleted");
  assert.equal($(document, "btn-slide-delete").disabled, true);
});

test("Alt+Up / Alt+Down / Delete work from the focused thumbnail and never inside text fields", async () => {
  const { document, sandbox, userClick } = await openSlideModule();
  const original = thumbTitles(document);
  userClick(document.querySelector('.slide-thumb[data-slide-idx="1"]'));
  await flushAsyncTurns();
  const second = document.activeElement;
  assert.equal(second.dataset.slideIdx, "1", "clicking a thumbnail keeps keyboard focus on it");
  const ev = sandbox.dispatchWindowKeydown({ key: "ArrowUp", altKey: true, target: second });
  assert.equal(ev.defaultPrevented, true);
  assert.deepEqual(thumbTitles(document), [original[1], original[0], original[2]]);
  const focused = document.activeElement;
  assert.equal(focused.dataset.slideIdx, "0", "focus follows the moved thumbnail");
  sandbox.dispatchWindowKeydown({ key: "ArrowDown", altKey: true, target: focused });
  assert.deepEqual(thumbTitles(document), original);

  const notes = $(document, "slide-presenter-notes");
  notes.focus();
  const typing = sandbox.dispatchWindowKeydown({ key: "ArrowUp", altKey: true, target: notes });
  assert.equal(typing.defaultPrevented, false);
  assert.deepEqual(thumbTitles(document), original);
  const del = sandbox.dispatchWindowKeydown({ key: "Delete", target: notes });
  assert.equal(del.defaultPrevented, false);
  assert.equal(thumbs(document).length, 3);
  notes.blur();

  const first = document.querySelector('.slide-thumb[data-slide-idx="0"]');
  first.focus();
  sandbox.dispatchWindowKeydown({ key: "Delete", target: first });
  assert.equal(thumbs(document).length, 2);
});

test("drag and drop reorders thumbnails and the right-click menu offers every operation", async () => {
  const { document, sandbox, userClick } = await openSlideModule();
  const original = thumbTitles(document);
  const drag = (from, to) => {
    const dataTransfer = { data: {}, setData(t, v) { this.data[t] = v; }, getData(t) { return this.data[t]; }, effectAllowed: "" };
    const src = document.querySelector(`.slide-thumb[data-slide-idx="${from}"]`);
    const dst = document.querySelector(`.slide-thumb[data-slide-idx="${to}"]`);
    assert.equal(src.getAttribute("draggable"), "true");
    src.dispatchEvent({ type: "dragstart", target: src, dataTransfer });
    dst.dispatchEvent({ type: "dragover", target: dst, dataTransfer, clientY: 0 });
    dst.dispatchEvent({ type: "drop", target: dst, dataTransfer, clientY: 0 });
    src.dispatchEvent({ type: "dragend", target: src, dataTransfer });
  };
  drag(0, 2);
  await flushAsyncTurns();
  assert.deepEqual(thumbTitles(document), [original[1], original[2], original[0]]);
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.deepEqual(thumbTitles(document), original);

  const menu = $(document, "slide-context-menu");
  assert.equal(menu.hidden, true);
  const target = document.querySelector('.slide-thumb[data-slide-idx="1"]');
  const cm = target.dispatchEvent({ type: "contextmenu", target, clientX: 40, clientY: 60 });
  assert.equal(cm, false, "native context menu is replaced");
  assert.equal(menu.hidden, false);
  for (const action of ["new", "duplicate", "delete", "up", "down"]) {
    assert.ok(menu.querySelector(`[data-slide-action="${action}"]`), action);
  }
  userClick(menu.querySelector('[data-slide-action="duplicate"]'));
  await flushAsyncTurns();
  assert.equal(menu.hidden, true);
  assert.equal(thumbs(document).length, 4);
  assert.equal(thumbs(document)[2].classList.contains("active"), true);

  const t3 = document.querySelector('.slide-thumb[data-slide-idx="0"]');
  t3.dispatchEvent({ type: "contextmenu", target: t3, clientX: 1, clientY: 1 });
  assert.equal(menu.hidden, false);
  sandbox.dispatchWindowKeydown({ key: "Escape" });
  assert.equal(menu.hidden, true);
});

test("thumbnails follow title and body edits (debounced), keep full titles in a tooltip and ellipsize in CSS", async () => {
  const { document, advanceTimers } = await openSlideModule();
  const long = "아주 긴 슬라이드 제목입니다 그래서 목록에서는 말줄임표로 잘려야 합니다";
  typeTitle(document, long);
  assert.notEqual(thumbTitles(document)[0], long, "not refreshed synchronously on every keystroke");
  advanceTimers(400);
  await flushAsyncTurns();
  const t0 = thumbs(document)[0];
  assert.equal(t0.querySelector(".slide-thumb-title").textContent, long, "JS never hard-truncates the title");
  assert.match(t0.getAttribute("title"), /아주 긴 슬라이드 제목/);
  assert.match(STYLES_CSS, /\.slide-thumb-title\s*\{[^}]*text-overflow:\s*ellipsis/);
  assert.match(STYLES_CSS, /\.slide-thumb-title\s*\{[^}]*white-space:\s*nowrap/);
  assert.equal(t0.classList.contains("active"), true);

  const lower = $(document, "slide-lower-zone");
  lower.focus();
  const p = lower.querySelector("p");
  lower.dispatchEvent({ type: "beforeinput", target: p });
  p.textContent = "본문 미리보기 텍스트";
  lower.dispatchEvent({ type: "input", target: p });
  advanceTimers(400);
  assert.match(thumbs(document)[0].textContent, /본문 미리보기 텍스트/);
  assert.equal(thumbs(document).length, 3);
});

test("stage keeps the deck aspect ratio (16:9, 4:3, A4) and scales its text with the stage", async () => {
  const { document, userClick, sandbox } = await openSlideModule();
  const stage = $(document, "slide-stage-box");
  const select = $(document, "slide-aspect-select");
  assert.deepEqual(
    select.querySelectorAll("option").map((o) => o.getAttribute("value")),
    ["16:9", "4:3", "A4"]
  );
  assert.equal(stage.dataset.aspect, "16:9");
  select.value = "4:3";
  select.dispatchEvent({ type: "change", target: select });
  await flushAsyncTurns();
  assert.equal(stage.dataset.aspect, "4:3");
  userClick($(document, "btn-add-slide"));
  await flushAsyncTurns();
  assert.equal(stage.dataset.aspect, "4:3", "new slides inherit the deck ratio");
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns();
  assert.equal(stage.dataset.aspect, "16:9", "aspect change is part of undo");

  assert.match(STYLES_CSS, /\.slide-stage\s*\{[^}]*aspect-ratio/);
  for (const a of ["4:3", "A4"]) {
    assert.match(STYLES_CSS, new RegExp(String.raw`\.slide-stage\[data-aspect="${a}"\]\s*\{[^}]*aspect-ratio`));
  }
  assert.match(STYLES_CSS, /\.slide-stage\s*\{[^}]*container-type:\s*inline-size/);
  assert.match(STYLES_CSS, /\.slide-title-zone[^{]*\{[^}]*cqw/);
  assert.doesNotMatch(STYLES_CSS, /\.slide-stage,\s*\.pdf-page-card\s*\{[^}]*min-height:\s*330px/);
});

test("autosave and save payload carry the managed slide order, titles, bullets and notes", async () => {
  const storage = createMemoryStorage();
  const { sandbox: base } = await createWebUiDom();
  const cat = base.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = base.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "SLIDE",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
  const converts = [];
  const { document, sandbox, userClick, advanceTimers } = await createWebUiDom({
    localStorage: storage,
    testIpc: async (command, payload) => {
      if (command === "convert_document_format") converts.push(JSON.parse(String(payload)));
      return offline[command](payload);
    },
  });
  switchModule(document, "SLIDE");
  await flushAsyncTurns();
  userClick(document.querySelector('.slide-thumb[data-slide-idx="2"]'));
  await flushAsyncTurns();
  document.querySelector('.slide-thumb[data-slide-idx="2"]').focus();
  sandbox.dispatchWindowKeydown({ key: "ArrowUp", altKey: true });
  await flushAsyncTurns();
  userClick($(document, "btn-add-slide"));
  await flushAsyncTurns();
  typeTitle(document, "마지막 새 슬라이드");

  advanceTimers(5000);
  await flushAsyncTurns(10);
  const record = JSON.parse(storage.getItem(AUTOSAVE_KEY));
  assert.deepEqual(
    record.snapshot.slides.map((s) => s.title),
    [
      "연간 제품 전략 발표 자료 (16:9 와이드)",
      "대용량 문서 및 다중 시트 처리 성능 요약",
      "문서 포맷 호환성 및 일괄 변환 흐름",
      "마지막 새 슬라이드",
    ]
  );

  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(10);
  const payload = converts.at(-1);
  assert.equal(payload.targetFormat, "pptx");
  assert.deepEqual(
    payload.slides.map((s) => s.title),
    record.snapshot.slides.map((s) => s.title)
  );
  assert.deepEqual(payload.slides[3].bullets, []);
  assert.match(payload.slides[0].notes, /핵심 메시지/);
});
