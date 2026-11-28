import test from "node:test";
import assert from "node:assert/strict";
import { flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem } from "./test-support/modules.mjs";
import { createRunHarness, rangeOf, p } from "./test-support/doc-run-harness.mjs";

// The Pages document window follows the reference Pages app's Korean menus (PD-399): the order of the menus, the names the web
// menus, the toolbar and the command search share with the native bar, and the commands it takes from the word processor.

const openPages = async (content = [p("가나다라")]) => {
  const h = await createRunHarness(content);
  h.editor.dataset.eoDocFlavor = "pages";
  h.document.defaultView.__EASYOFFICE_ACTIVATE_MODULE__("DOC");
  await flushAsyncTurns(10);
  return h;
};
const shown = (el) => !el.hidden && !el.closest("[hidden]");
const rows = (container) =>
  container.children
    .filter((el) => !el.hidden)
    .map((el) => {
      if (el.classList.contains("md-sep")) return "-";
      const label = el.querySelector(":scope > .md-label").textContent;
      const panel = el.querySelector(":scope > .md-sub-panel");
      return panel ? `${label} > ${rows(panel).join(" | ")}` : label;
    });
const menuRows = (h, title) => {
  const menu = h.document.querySelectorAll(".studio-menubar > .menu-item").find((m) => !m.hidden && m.querySelector(".menu-title").textContent === title);
  return rows(menu.querySelector(":scope > .menu-dropdown"));
};
const byId = (h, id) => h.document.getElementById(id);
const run = async (h, cmd) => {
  h.userClick(studioItem(h.document, cmd));
  await flushAsyncTurns(8);
};
const select = async (h, range) => {
  h.selection.addRange(range);
  h.document.dispatchEvent({ type: "selectionchange" });
  await flushAsyncTurns(10);
};
const runsOf = (h) => h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor)[0].runs;

test("the menus come in the reference app's order and hold its items under its names", async () => {
  const h = await openPages();
  const titles = h.document.querySelectorAll(".studio-menubar > .menu-item").filter((m) => !m.hidden).map((m) => m.querySelector(".menu-title").textContent);
  assert.deepEqual(titles, ["파일", "편집", "삽입", "포맷", "정렬", "보기", "도구", "도움말"]);
  assert.deepEqual(menuRows(h, "삽입"), ["도형", "이미지", "차트", "표 삽입", "-", "링크", "북마크…", "날짜 및 시간…", "-", "페이지 나누기", "-", "각주", "미주", "-", "자동 목차 생성", "목차 업데이트", "머리말", "꼬리말"]);
  assert.deepEqual(menuRows(h, "포맷"), [
    "볼드체", "이탤릭체", "밑줄체", "취소선", "위 첨자", "아래 첨자", "-",
    "서체 윈도우", "하이라이트", "대문자 > 없음 | 모두 대문자 | 작은 대문자 | 첫 글자만 대문자", "-",
    "스타일 적용", "좌우 정렬", "-", "체크리스트", "들여쓰기 단계 증가", "들여쓰기 단계 감소", "-",
    "표 > 위에 행 추가 | 아래에 행 추가 | 앞에 열 추가 | 뒤에 열 추가 | 행 삭제 | 열 삭제 | - | 셀 병합 해제 | 셀 병합 | 같은 너비로 열 조절 | - | 테두리 및 음영 | 표 속성 | - | 블록 합계 | 오름차순·내림차순 정렬",
  ]);
  assert.deepEqual(menuRows(h, "정렬"), ["텍스트 줄 바꿈 > 줄 안 | 정사각형 | 위/아래 | 텍스트 뒤 | 텍스트 앞", "-", "자르기", "오른쪽으로 90° 회전", "왼쪽으로 90° 회전", "-", "워터마크…"]);
  const view = menuRows(h, "보기");
  assert.deepEqual(view.slice(-2), ["보이지 않는 요소 보기", "단어 수 보기"]);
  assert.ok(menuRows(h, "도구").includes("주석 추가"));
  assert.ok(!menuRows(h, "도구").includes("문서 엔진 성능 진단"), "the engine diagnostic is in the command search only");
  assert.ok(!menuRows(h, "도구").includes("문서 통계·글자 수"));
  const file = menuRows(h, "파일");
  assert.deepEqual(file.slice(-5), ["페이지 설정", "프린트 미리 보기", "프린트…", "-", "닫기"]);
  assert.equal(menuRows(h, "편집").includes("찾기 및 대치"), true);
  assert.equal(menuRows(h, "편집").includes("실행 복귀"), true);
});

test("no menu, button or tooltip of the Pages window uses a word of another product", async () => {
  const h = await openPages();
  const other = /인쇄|다시 실행|잘라내기|찾기 및 바꾸기|서식 기호|편집 기호|문서 통계|글자 수|새 메모|양쪽 맞춤|표 만들기|책갈피|하이퍼링크|성능 진단|그림(?! 서식)/;
  const seen = [];
  for (const el of h.document.querySelectorAll(".studio-menubar .md-label")) if (shown(el)) seen.push(el.textContent);
  for (const btn of h.document.querySelectorAll("#icon-toolbar .tb-btn")) {
    if (!shown(btn)) continue;
    seen.push(btn.getAttribute("title") || "", btn.getAttribute("aria-label") || "", btn.querySelector(".tb-label")?.textContent || "");
  }
  for (const id of ["btn-align-justify", "btn-stylebar-highlight", "btn-stylebar-textcolor", "btn-size-up", "btn-size-down"]) {
    seen.push(byId(h, id).getAttribute("title") || "", byId(h, id).getAttribute("aria-label") || "");
  }
  assert.deepEqual(seen.filter((text) => other.test(text)), []);
  assert.equal(byId(h, "btn-toggle-ctrl-marks").hidden, true, "one button shows the formatting symbols: 보이지 않는 요소 보기");
  assert.equal(byId(h, "btn-toggle-para-marks").getAttribute("title"), "보이지 않는 요소 보기");
  assert.equal(byId(h, "btn-stylebar-highlight").getAttribute("title"), "하이라이트");
  assert.equal(byId(h, "doc-bookmark-title").textContent, "북마크");
  assert.equal(byId(h, "doc-wm-title").textContent, "워터마크");
  assert.equal(byId(h, "doc-comment-dialog-title").textContent, "주석 추가");
  assert.equal(h.document.querySelector(".print-preview-title").textContent, "프린트 미리 보기");
});

test("the command search of the Pages window uses the same names and keeps the engine diagnostic", async () => {
  const h = await openPages();
  h.userClick(byId(h, "btn-command-palette"));
  const input = byId(h, "cmd-search-input");
  const search = (text) => {
    input.value = text;
    input.dispatchEvent({ type: "input" });
    return [...h.document.querySelectorAll("#cmd-palette-list .cp-item")].map((li) => li.querySelector(".cp-item-label").textContent);
  };
  assert.deepEqual(search("성능 진단"), ["문서 엔진 성능 진단"]);
  assert.deepEqual(search("단어 수"), ["단어 수 보기"]);
  assert.deepEqual(search("주석"), ["주석 추가"]);
  assert.deepEqual(search("바꾸기"), []);
  assert.deepEqual(search("새 메모"), []);
  assert.ok(search("페이지 나누기").includes("페이지 나누기"));
  for (const title of ["날짜 및 시간…", "북마크…", "서체 윈도우", "하이라이트", "워터마크…"]) assert.ok(search(title.replace("…", "")).includes(title), title);
  assert.ok(search("대문자").length >= 3);
  assert.deepEqual(search("인쇄"), [], "no entry says 인쇄");
});

test("the Pages window keys (하이라이트, 북마크) work in the Pages window only", async () => {
  const pages = await openPages();
  const text = pages.blocks[0].firstChild;
  await select(pages, rangeOf(text, 0, text, 4));
  pages.sandbox.dispatchWindowKeydown({ key: "H", ctrlKey: true, shiftKey: true });
  await flushAsyncTurns(8);
  assert.ok(runsOf(pages).some((r) => r.highlight), "Ctrl+Shift+H highlights in the Pages window");
  pages.sandbox.dispatchWindowKeydown({ key: "b", ctrlKey: true, altKey: true });
  await flushAsyncTurns(8);
  assert.equal(byId(pages, "doc-bookmark-dialog").open, true, "Ctrl+Alt+B opens 북마크");

  const word = await createRunHarness([p("가나다라")]);
  const wordText = word.blocks[0].firstChild;
  await select(word, rangeOf(wordText, 0, wordText, 4));
  word.sandbox.dispatchWindowKeydown({ key: "H", ctrlKey: true, shiftKey: true });
  word.sandbox.dispatchWindowKeydown({ key: "b", ctrlKey: true, altKey: true });
  await flushAsyncTurns(8);
  assert.ok(!runsOf(word).some((r) => r.highlight), "the Word window leaves the key alone");
  assert.equal(byId(word, "doc-bookmark-dialog").open, false);
});

test("날짜 및 시간, 북마크 and 워터마크 are the word processor's commands, opened from the Pages menus", async () => {
  const h = await openPages();
  await select(h, rangeOf(h.blocks[0].firstChild, 2, h.blocks[0].firstChild, 2));
  await run(h, "insert-datetime");
  assert.equal(byId(h, "doc-datetime-dialog").open, true);
  byId(h, "doc-dt-update").checked = true;
  h.userClick(byId(h, "btn-doc-dt-ok"));
  await flushAsyncTurns(6);
  assert.ok(h.editor.querySelector("[data-eo-field-code]"), "the date is a field the file keeps");
  await run(h, "insert-bookmark");
  assert.equal(byId(h, "doc-bookmark-dialog").open, true);
  assert.equal(byId(h, "doc-bookmark-title").textContent, "북마크");
  h.userClick(byId(h, "doc-bookmark-dialog").querySelector("[data-dialog-close]"));
  await run(h, "layout-watermark");
  assert.equal(byId(h, "doc-watermark-dialog").open, true);
  byId(h, "doc-wm-text").value = "초안";
  h.userClick(byId(h, "btn-doc-wm-ok"));
  await flushAsyncTurns(6);
  assert.equal(h.host.__EASYOFFICE_SAVE_DOC_META__.collectDocAnnotations(h.editor).watermark, "초안");
});

test("하이라이트 toggles the highlight of the selection, and 대문자 sets the capitals bits of the run", async () => {
  const h = await openPages();
  const text = h.blocks[0].firstChild;
  await select(h, rangeOf(text, 0, text, 4));
  await run(h, "home-highlight");
  assert.ok(runsOf(h).some((r) => r.highlight), "the selection is highlighted");
  await select(h, rangeOf(h.blocks[0].firstChild, 0, h.blocks[0].lastChild, h.blocks[0].lastChild.textContent?.length ?? 0));
  await run(h, "home-highlight");
  assert.ok(!runsOf(h).some((r) => r.highlight), "the second press takes it off");
  const textFx = () => runsOf(h).map((r) => r.textFx ?? 0);
  const whole = async () => select(h, rangeOf(h.blocks[0].firstChild, 0, h.blocks[0].lastChild, h.blocks[0].lastChild.textContent?.length ?? 0));
  await whole();
  await run(h, "text-case-all");
  assert.ok(textFx().every((fx) => (fx & 4) !== 0 && (fx & 2) === 0), "모두 대문자");
  await whole();
  await run(h, "text-case-small");
  assert.ok(textFx().every((fx) => (fx & 2) !== 0 && (fx & 4) === 0), "작은 대문자 replaces 모두 대문자");
  await whole();
  await run(h, "text-case-none");
  assert.ok(textFx().every((fx) => (fx & 6) === 0), "없음");
});

test("the word processor's commands that the Pages window does not name stay out of it", async () => {
  const h = await openPages();
  const commands = h.host.__EASYOFFICE_DOC_WORD_COMMANDS__;
  const env = { editor: h.editor, document: h.document, runs: h.runs, module: "DOC" };
  for (const cmd of ["layout-linenumbers", "doc-properties"]) {
    assert.equal(commands.run(cmd, env), true, `${cmd} is a known command`);
    await flushAsyncTurns(4);
  }
  for (const id of ["doc-linenumbers-dialog", "doc-properties-dialog"]) assert.notEqual(byId(h, id)?.open, true, `${id} stays shut in the Pages window`);
  for (const [cmd, dialog] of [["insert-datetime", "doc-datetime-dialog"], ["insert-bookmark", "doc-bookmark-dialog"], ["layout-watermark", "doc-watermark-dialog"]]) {
    commands.run(cmd, env);
    await flushAsyncTurns(4);
    assert.equal(byId(h, dialog).open, true, cmd);
    byId(h, dialog).close();
  }
});
