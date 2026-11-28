import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem } from "./test-support/modules.mjs";

// The word processor window's wording and counts (QA 2026-11-28): the menus, the status bar and the 단어 개수 dialog
// follow the Word program's Korean screens.

async function openDoc(html) {
  const h = await createWorkspaceHarness({ search: "?module=DOC&win=w-word-terms" });
  await flushAsyncTurns(8);
  h.editor = h.document.getElementById("doc-editor");
  h.editor.innerHTML = html;
  h.editor.dispatchEvent({ type: "input", target: h.editor });
  await flushAsyncTurns(4);
  return h;
}

const rows = (document) => {
  const dts = Array.from(document.querySelectorAll("#doc-stats-grid dt"), (el) => el.textContent);
  const dds = Array.from(document.querySelectorAll("#doc-stats-grid dd"), (el) => el.textContent);
  return Object.fromEntries(dts.map((label, i) => [label, dds[i]]));
};

test("the menu titles are 삽입 and 레이아웃, 기호 opens its submenu with 기타 기호…, 서식 has 단락…", async () => {
  const { document } = await openDoc("<p>가</p>");
  const titles = Array.from(document.querySelectorAll("#menu-bar .menu-item > .menu-title"), (el) => el.textContent).filter(Boolean);
  assert.ok(titles.includes("삽입") && titles.includes("레이아웃"), titles.join());
  assert.ok(!titles.includes("입력") && !titles.includes("쪽"));
  const symbol = studioItem(document, "mod-symbol");
  assert.equal(symbol.querySelector(".md-label").textContent, "기타 기호…");
  assert.equal(symbol.closest(".md-sub").querySelector(":scope > .md-label").textContent, "기호");
  assert.equal(studioItem(document, "para-dialog").querySelector(".md-label").textContent, "단락…");
});

test("서식 > 단락… opens the paragraph dialog on the paragraph of the caret", async () => {
  const h = await openDoc("<p>가나다</p>");
  h.userClick(studioItem(h.document, "para-dialog"));
  await flushAsyncTurns(4);
  const dialog = h.document.getElementById("doc-paragraph-dialog");
  assert.equal(Boolean(dialog.open), true);
  assert.equal(h.document.getElementById("doc-paragraph-dialog-title").textContent, "단락");
});

test("the header and footer tools call the fields 페이지 번호 and 페이지 수", async () => {
  const { document } = await openDoc("<p>가</p>");
  const label = (action) => document.querySelector(`[data-hf-action="${action}"] .tb-label`).textContent;
  assert.equal(label("page-number"), "페이지 번호");
  assert.equal(label("total-pages"), "페이지 수");
  assert.equal(document.querySelector('[data-hf-action="total-pages"]').getAttribute("title"), "페이지 수 삽입");
});

test("the ribbon names the word count 단어 개수 in the document window", async () => {
  const { host } = await openDoc("<p>가</p>");
  const review = host.__EASYOFFICE_RIBBON_CATALOG__.RIBBON_CATEGORY_TOOLS.DOC.review;
  assert.equal(review.find((item) => item.id === "document-review-stats").title, "단어 개수");
  assert.equal(host.__EASYOFFICE_RIBBON_CATALOG__.RIBBON_CATEGORY_TOOLS.SHEET.review.find((item) => item.id === "sheet-review-stats").title, "문서 통계·글자 수");
});

test("the status bar shows N/M 페이지 and 단어 N개, and the word count opens the 단어 개수 dialog", async () => {
  const h = await openDoc("<p>하나 둘 셋</p><p>넷</p>");
  const { document } = h;
  assert.match(document.getElementById("status-studio-pos").textContent, /^1\/\d+ 페이지$/);
  assert.doesNotMatch(document.getElementById("status-studio-pos").textContent, /단|줄|칸|구역/);
  const word = document.getElementById("status-word-count");
  assert.equal(word.hidden, false);
  assert.equal(word.textContent, "단어 4개");
  h.userClick(word);
  await flushAsyncTurns(4);
  assert.equal(Boolean(document.getElementById("doc-stats-dialog").open), true);
  assert.equal(document.getElementById("doc-stats-title").textContent, "단어 개수");
});

test("the 단어 개수 dialog lists the Word program's rows and counts the table cells and the notes the checkbox includes", async () => {
  const h = await openDoc('<p>가나 다<sup class="eo-note-ref" data-eo-note-kind="footnote" data-eo-note-id="1">1)</sup></p><table><tr><td>라마</td><td>바</td></tr></table><section data-eo-notes="footnote"><div class="eo-doc-note-item"><span class="eo-doc-note-num">1)</span><p class="eo-doc-note-body">각주 글</p></div></section><section data-eo-hf="footer"><p>바닥 글</p></section>');
  const { document } = h;
  // Loading the notes section rebuilds its items from the references; the text of the note is typed afterwards.
  h.editor.querySelector(".eo-doc-note-body").textContent = "각주 글";
  h.userClick(studioItem(document, "review-stats"));
  await flushAsyncTurns(6);
  assert.deepEqual(Object.keys(rows(document)), ["페이지", "단어", "문자(공백 제외)", "문자(공백 포함)", "단락", "줄"]);
  const checkbox = document.getElementById("doc-stats-notes");
  assert.equal(document.getElementById("doc-stats-notes-row").textContent.trim(), "텍스트 상자, 각주 및 미주 포함");
  assert.equal(checkbox.checked, true);
  // 가나, 다, 라마, 바 and the note's 각주, 글; the footer text is not counted.
  assert.deepEqual(rows(document), { 페이지: "1", 단어: "6", "문자(공백 제외)": "9", "문자(공백 포함)": "11", 단락: "4", 줄: "4" });
  checkbox.checked = false;
  checkbox.dispatchEvent({ type: "change", target: checkbox });
  assert.deepEqual(rows(document), { 페이지: "1", 단어: "4", "문자(공백 제외)": "6", "문자(공백 포함)": "7", 단락: "3", 줄: "3" });
});

test("the Hangul window keeps its own status line and the shared statistics dialog", async () => {
  const h = await createWorkspaceHarness({ search: "?module=SHEET&win=w-word-terms-sheet" });
  await flushAsyncTurns(8);
  const { statusTexts } = h.host.__EASYOFFICE_DOC_STATUS_TEXT__;
  assert.equal(statusTexts({ word: false, current: 2, total: 5, cols: 1, line: 3, col: 4, words: 0, insertMode: "삽입" }).pos, "2/5 쪽   1단   3줄   4칸 | 구역: 1 / 1 |");
});

// ---- 페이지 설정: the tabs 여백, 용지, 레이아웃 and 여러 페이지 > 책 접기

test("페이지 설정 has the tabs 여백, 용지 and 레이아웃, and each tab shows its own sections", async () => {
  const h = await openDoc("<p>가</p>");
  const { document } = h;
  const byId = (id) => document.getElementById(id);
  h.userClick(studioItem(document, "layout-paper"));
  await flushAsyncTurns(4);
  assert.equal(byId("doc-ps-tabs").hidden, false);
  assert.deepEqual(Array.from(document.querySelectorAll("#doc-ps-tabs [data-ps-tab]"), (el) => el.textContent), ["여백", "용지", "레이아웃"]);
  const visible = () => Array.from(document.querySelectorAll("#doc-page-setup-dialog [data-ps-pane]")).filter((el) => !el.hidden).map((el) => el.dataset.psPane);
  assert.deepEqual(visible(), ["margin", "margin"], "the page opens on 여백: the orientation and binding, then the margins");
  h.userClick(byId("doc-ps-tab-paper"));
  assert.deepEqual(visible(), ["paper"]);
  assert.equal(byId("doc-ps-tab-paper").getAttribute("aria-selected"), "true");
  h.userClick(byId("doc-ps-tab-layout"));
  assert.deepEqual(visible(), ["layout"]);
  assert.equal(byId("doc-ps-layout-title").textContent, "머리글 및 바닥글");
});

test("여러 페이지 > 책 접기 turns the page to landscape and is saved as the book fold binding", async () => {
  const h = await openDoc("<p>가</p>");
  const { document } = h;
  const byId = (id) => document.getElementById(id);
  h.userClick(studioItem(document, "layout-paper"));
  await flushAsyncTurns(4);
  assert.deepEqual(Array.from(byId("doc-ps-multi").querySelectorAll("option"), (o) => o.textContent), ["보통", "대칭 여백", "책 접기"]);
  byId("doc-ps-multi").value = "3";
  byId("doc-ps-multi").dispatchEvent({ type: "change", target: byId("doc-ps-multi"), bubbles: true });
  assert.equal(byId("doc-ps-landscape").checked, true);
  assert.deepEqual([byId("doc-ps-left-label").textContent, byId("doc-ps-right-label").textContent], ["안쪽", "바깥쪽"]);
  byId("doc-ps-gutter").value = "10";
  byId("btn-doc-ps-apply").click();
  await flushAsyncTurns(4);
  const payload = h.host.__EASYOFFICE_DOC_PAGE_SETUP_CORE__.payloadOf(h.editor.dataset.eoPage);
  assert.equal(payload.pageSetup.binding.kind, "bookfold");
  assert.ok(payload.pageSetup.paper.width > payload.pageSetup.paper.height);
  // The dialog opens on what the document has.
  h.userClick(studioItem(document, "layout-paper"));
  await flushAsyncTurns(4);
  assert.equal(byId("doc-ps-multi").value, "3");
});
