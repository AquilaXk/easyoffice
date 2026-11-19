import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";

const $ = (doc, id) => doc.getElementById(id);

function verifyMenuAction(doc, act) {
  const el = doc.querySelector(`#menu-bar .md-item[data-verify-act="${act}"]`);
  assert.ok(el, `missing verify-act menu item: ${act}`);
  return el;
}

test("PD-258 SLIDE 문서 검사: 개체 넘침·개체 겹침·빈 자리표시자 3종 검출, 슬라이드·개체 이동, 수정 후 다시 검사", async () => {
  const { document, userClick, host } = await createWorkspaceHarness({
    search: "?module=SLIDE&win=w-pd258-slide",
  });
  await flushAsyncTurns(10);
  switchModule(document, "SLIDE");
  await flushAsyncTurns(6);

  const inspectMenu = verifyMenuAction(document, "doc-inspect");
  assert.equal(inspectMenu.querySelector(".md-label")?.textContent, "문서 검사…");
  assert.equal(inspectMenu.dataset.kinds, "DOC SLIDE");

  const canvasApi = host.__EASYOFFICE_SLIDE_CANVAS__;
  const printApi = host.__EASYOFFICE_OFFICE_PRINT__;
  assert.ok(canvasApi, "slide canvas API exists");
  assert.ok(printApi?.slideDeckState, "officePrint exposes live slideDeckState");
  const deck = printApi.slideDeckState;

  // Slide 1: empty title + background shape (>=90% stage, ignored for overlap) + 2 overlapping shapes + empty & {{날짜}} textboxes.
  deck[0].layout = "titleAndContent";
  deck[0].title = "";
  deck[0].lower = "";
  const h2 = $(document, "slide-title-zone").querySelector("h2");
  if (h2) h2.textContent = "";
  $(document, "slide-title-zone").dispatchEvent({ type: "input", target: $(document, "slide-title-zone") });

  const bgShape = canvasApi.addElement({
    kind: "shape",
    shapeType: "rect",
    x: 0,
    y: 0,
    w: 960,
    h: 540,
    text: "",
  });
  const shapeA = canvasApi.addElement({
    kind: "shape",
    shapeType: "roundRect",
    x: 60,
    y: 120,
    w: 180,
    h: 110,
    text: "개체 A",
  });
  const shapeB = canvasApi.addElement({
    kind: "shape",
    shapeType: "rect",
    x: 160,
    y: 160,
    w: 180,
    h: 110,
    text: "개체 B",
  });
  const tbToken = canvasApi.addElement({
    kind: "textbox",
    x: 400,
    y: 80,
    w: 220,
    h: 60,
    text: "발표일: {{날짜}}",
  });
  const tbEmpty = canvasApi.addElement({
    kind: "textbox",
    x: 400,
    y: 180,
    w: 180,
    h: 50,
    text: "   ",
  });

  // Add Slide 2 with stage-protruding shape and text-overflowing textbox.
  userClick($(document, "btn-add-slide"));
  await flushAsyncTurns(4);
  assert.equal(deck.length, 2);
  canvasApi.setSlideLayout("blank");
  const outShape = canvasApi.addElement({
    kind: "shape",
    shapeType: "rect",
    x: 520,
    y: 70,
    w: 180,
    h: 90,
    text: "경계 밖 도형",
  });
  // Push past 960pt right edge in deck state (simulating imported/resized out-of-stage coordinates).
  const rawOut = deck[1].elements.find((e) => e.id === outShape.id);
  rawOut.x = 820;
  rawOut.w = 180;

  const overflowTb = canvasApi.addElement({
    kind: "textbox",
    x: 40,
    y: 200,
    w: 110,
    h: 28,
    fontSizePt: 18,
    text: "첫째 줄 긴 본문 텍스트입니다\n둘째 줄 긴 본문 텍스트입니다\n셋째 줄 긴 본문 텍스트입니다",
  });

  // Switch back to Slide 1 before opening 문서 검사 so jump-to-slide-2 is verified end-to-end.
  const slide1Thumb = document.querySelector('#slide-thumb-list .slide-thumb[data-slide-idx="0"]');
  userClick(slide1Thumb);
  await flushAsyncTurns(4);
  assert.equal(document.querySelector("#slide-thumb-list .slide-thumb.active")?.dataset.slideIdx, "0");

  // Open 문서 검사 via native menu parity action.
  assert.equal(document.defaultView.__EASYOFFICE_NATIVE_MENU__("tool:doc-inspect"), true);
  await flushAsyncTurns(8);

  const dialog = $(document, "doc-inspect-dialog");
  assert.ok(dialog, "#doc-inspect-dialog exists");
  assert.equal(dialog.open, true);
  assert.equal($(document, "doc-inspect-title")?.textContent, "문서 검사");

  const summary = $(document, "doc-inspect-summary");
  assert.match(summary.textContent, /검사 결과/);
  assert.match(summary.textContent, /개체 넘침\s*[2-9]/);
  assert.match(summary.textContent, /개체 겹침\s*1\b/);
  assert.match(summary.textContent, /빈 자리표시자\s*[3-9]/);

  const items = Array.from(document.querySelectorAll("#doc-inspect-list .doc-inspect-item"));
  assert.ok(items.length >= 6, `expected >=6 issues, got ${items.length}`);
  const kinds = new Set(items.map((li) => li.dataset.kind));
  assert.deepEqual(
    [...kinds].sort((a, b) => a.localeCompare(b)),
    ["emptyPlaceholder", "overflow", "overlap"]
  );
  const badges = new Set(items.map((li) => li.querySelector(".doc-inspect-badge")?.textContent));
  assert.equal(badges.has("개체 넘침"), true);
  assert.equal(badges.has("개체 겹침"), true);
  assert.equal(badges.has("빈 자리표시자"), true);
  assert.equal(
    items.some((li) => li.dataset.kind === "overlap" && li.textContent.includes(bgShape.id)),
    false,
    "90%+ background shape must not be flagged as overlap"
  );

  // Click "이동" on outShape (Slide 2) and verify slide switches to index 1 and outShape is selected.
  const outItem = items.find((li) => li.dataset.targetId === outShape.id && li.dataset.kind === "overflow");
  assert.ok(outItem, "found overflow item for outShape");
  const jumpBtn = outItem.querySelector(".doc-inspect-jump-btn");
  assert.equal(jumpBtn?.textContent, "이동");
  userClick(jumpBtn);
  await flushAsyncTurns(4);

  assert.equal(document.querySelector("#slide-thumb-list .slide-thumb.active")?.dataset.slideIdx, "1");
  assert.equal(canvasApi.getSelectedElement()?.id, outShape.id);

  // Fix Slide 2 issues.
  rawOut.x = 420;
  rawOut.w = 180;
  const rawOverflowTb = deck[1].elements.find((e) => e.id === overflowTb.id);
  rawOverflowTb.w = 340;
  rawOverflowTb.h = 140;
  canvasApi.render();

  // Fix Slide 1 issues (keep bgShape to ensure 90%+ background stays clean).
  userClick(document.querySelector('#slide-thumb-list .slide-thumb[data-slide-idx="0"]'));
  await flushAsyncTurns(4);
  deck[0].title = "분기 실적 요약";
  deck[0].lower = "핵심 성과 지표 정리";
  const s1A = deck[0].elements.find((e) => e.id === shapeA.id);
  const s1B = deck[0].elements.find((e) => e.id === shapeB.id);
  s1A.x = 60;
  s1B.x = 300;
  const s1Token = deck[0].elements.find((e) => e.id === tbToken.id);
  s1Token.text = "발표일: 2026-11-19";
  deck[0].elements = deck[0].elements.filter((e) => e.id !== tbEmpty.id);
  const activeH2 = $(document, "slide-title-zone").querySelector("h2");
  if (activeH2) activeH2.textContent = "분기 실적 요약";
  const activeP = $(document, "slide-lower-zone").querySelector("p");
  if (activeP) activeP.textContent = "핵심 성과 지표 정리";
  canvasApi.render();

  if (!dialog.open) {
    userClick(inspectMenu);
    await flushAsyncTurns(6);
  }
  const rerunBtn = $(document, "btn-doc-inspect-rerun");
  assert.equal(rerunBtn?.textContent, "다시 검사");
  userClick(rerunBtn);
  await flushAsyncTurns(8);

  assert.equal($(document, "doc-inspect-summary").textContent, "문제가 발견되지 않았습니다.");
  assert.equal(document.querySelectorAll("#doc-inspect-list .doc-inspect-item").length, 0);

  const closeBtn = $(document, "btn-doc-inspect-close");
  assert.equal(closeBtn?.textContent, "닫기");
  userClick(closeBtn);
  await flushAsyncTurns(2);
  assert.equal(dialog.open, false);
});

test("PD-258 DOC 문서 검사: 과폭 그림·표, 빈 제목·빈 표·미치환 토큰 검출 및 블록 이동", async () => {
  const { document, userClick, host } = await createWorkspaceHarness({
    search: "?module=DOC&win=w-pd258-doc",
  });
  await flushAsyncTurns(10);

  const editor = $(document, "doc-editor");
  const emptyHeading = document.createElement("h2");
  emptyHeading.textContent = "   ";

  const tokenPara = document.createElement("p");
  tokenPara.id = "doc-token-para";
  tokenPara.textContent = "수신: {{수신자}} 귀하";

  const wideImgPara = document.createElement("p");
  const wideImg = document.createElement("img");
  wideImg.setAttribute("alt", "광폭 배너");
  wideImg.setAttribute("width", "920");
  wideImg.setAttribute("height", "120");
  wideImgPara.appendChild(wideImg);

  const emptyWideTable = document.createElement("table");
  emptyWideTable.dataset.eoColWidths = "5200,5200";
  const tbody = document.createElement("tbody");
  const tr = document.createElement("tr");
  const td1 = document.createElement("td");
  td1.style.width = "460px";
  const td2 = document.createElement("td");
  td2.style.width = "460px";
  tr.replaceChildren(td1, td2);
  tbody.appendChild(tr);
  emptyWideTable.appendChild(tbody);

  const mergeConflictTable = document.createElement("table");
  const mTbody = document.createElement("tbody");
  const mTr1 = document.createElement("tr");
  const mTd11 = document.createElement("td");
  mTd11.rowSpan = 2;
  mTd11.textContent = "병합 A";
  const mTd12 = document.createElement("td");
  mTd12.textContent = "일반 B";
  mTr1.replaceChildren(mTd11, mTd12);
  const mTr2 = document.createElement("tr");
  const mTd21 = document.createElement("td");
  mTd21.colSpan = 2;
  mTd21.textContent = "충돌 병합 C";
  mTr2.replaceChildren(mTd21);
  mTbody.replaceChildren(mTr1, mTr2);
  mergeConflictTable.appendChild(mTbody);

  editor.replaceChildren(emptyHeading, tokenPara, wideImgPara, emptyWideTable, mergeConflictTable);

  // Check review-verify status feedback integration before opening dialog.
  const verifyCmd = document.querySelector('#menu-bar .md-item[data-studio-menu-cmd="review-verify"]');
  userClick(verifyCmd);
  await flushAsyncTurns(20);
  assert.match($(document, "status-command-feedback").textContent, /문서 검사\s*\d+건/);

  userClick(verifyMenuAction(document, "doc-inspect"));
  await flushAsyncTurns(8);

  const dialog = $(document, "doc-inspect-dialog");
  assert.equal(dialog.open, true);

  const items = Array.from(document.querySelectorAll("#doc-inspect-list .doc-inspect-item"));
  const kinds = new Set(items.map((li) => li.dataset.kind));
  assert.equal(kinds.has("overflow"), true, "over-wide image and table detected");
  assert.equal(kinds.has("overlap"), true, "conflicting merged cells detected");
  assert.equal(kinds.has("emptyPlaceholder"), true, "empty heading, empty table and {{수신자}} detected");
  assert.ok(items.filter((li) => li.dataset.kind === "overflow").length >= 2);
  assert.ok(items.filter((li) => li.dataset.kind === "overlap").length >= 1);
  assert.ok(items.filter((li) => li.dataset.kind === "emptyPlaceholder").length >= 3);

  // Click "이동" on the {{수신자}} paragraph issue and verify focus/tracking moves to that block.
  const tokenIssue = items.find((li) => li.textContent.includes("자리표시자") && li.dataset.targetId === "block-1");
  assert.ok(tokenIssue, "token paragraph issue located at block-1");
  userClick(tokenIssue.querySelector(".doc-inspect-jump-btn"));
  await flushAsyncTurns(4);
  assert.equal(tokenPara.dataset.eoLintActive, "true");

  // Also check programmatic API.
  const lintReport = await host.__EASYOFFICE_LAYOUT_LINT__.lintWorkspace();
  assert.ok(lintReport.issues.length >= 6);

  // Fix all DOC issues and rerun.
  emptyHeading.textContent = "1. 검토 개요";
  tokenPara.textContent = "수신: 기획조정실 귀하";
  wideImg.setAttribute("width", "320");
  delete emptyWideTable.dataset.eoColWidths;
  td1.style.width = "200px";
  td2.style.width = "200px";
  td1.textContent = "구분";
  td2.textContent = "내용";
  mergeConflictTable.remove();

  userClick($(document, "btn-doc-inspect-rerun"));
  await flushAsyncTurns(8);
  assert.equal($(document, "doc-inspect-summary").textContent, "문제가 발견되지 않았습니다.");
  userClick($(document, "btn-doc-inspect-close"));
  await flushAsyncTurns(2);
  assert.equal(dialog.open, false);
});

test("PD-258 창 내 실시간 인쇄 미리보기: 참조 앱 한국어 명칭, 비파괴 빌드, DOC·SHEET·SLIDE 실시간 갱신 및 도구 동작", async () => {
  const { document, sandbox, userClick, advanceTimers, host } = await createWorkspaceHarness({
    search: "?module=DOC&win=w-pd258-pp",
  });
  await flushAsyncTurns(10);

  const printed = [];
  host.requestAnimationFrame = (cb) => cb();
  host.print = () => {
    printed.push(document.getElementById("eo-print-root"));
  };

  const pane = $(document, "print-preview-pane");
  assert.ok(pane, "#print-preview-pane exists");
  assert.equal(pane.hidden, true);
  assert.equal(pane.getAttribute("aria-label"), "인쇄 미리보기");

  const toolbar = pane.querySelector(".print-preview-toolbar");
  assert.ok(toolbar);
  assert.equal(toolbar.getAttribute("role"), "toolbar");
  assert.equal(toolbar.getAttribute("aria-label"), "인쇄 미리보기 도구");
  assert.equal(pane.querySelector(".print-preview-title")?.textContent, "인쇄 미리보기");
  assert.equal($(document, "btn-pp-single")?.textContent, "한 쪽");
  assert.equal($(document, "btn-pp-spread")?.textContent, "두 쪽");
  assert.equal($(document, "btn-pp-zoom-out")?.getAttribute("aria-label"), "축소");
  assert.equal($(document, "btn-pp-zoom-in")?.getAttribute("aria-label"), "확대");
  assert.equal($(document, "btn-pp-fit-width")?.textContent, "폭 맞춤");
  assert.equal($(document, "btn-pp-fit-page")?.textContent, "쪽 맞춤");
  assert.equal($(document, "btn-pp-prev")?.getAttribute("aria-label"), "이전 쪽");
  assert.equal($(document, "btn-pp-next")?.getAttribute("aria-label"), "다음 쪽");
  assert.equal($(document, "btn-pp-print")?.textContent, "인쇄");
  assert.equal($(document, "btn-pp-close")?.textContent, "닫기");

  const ppMenu = verifyMenuAction(document, "print-preview");
  assert.equal(ppMenu.querySelector(".md-label")?.textContent, "인쇄 미리보기");
  assert.equal(ppMenu.dataset.kinds, "DOC SHEET SLIDE");

  // 1. Open in DOC via native menu action and verify non-destructive preview + live refresh.
  const editor = $(document, "doc-editor");
  const p1 = document.createElement("p");
  p1.textContent = "초기 문서 내용";
  editor.replaceChildren(p1);

  assert.equal(document.defaultView.__EASYOFFICE_NATIVE_MENU__("file:print-preview"), true);
  await flushAsyncTurns(8);

  assert.equal(pane.hidden, false);
  assert.equal(document.getElementById("eo-print-root"), null, "buildPreview never mutates #eo-print-root");
  const stage = $(document, "print-preview-stage");
  assert.equal(stage.querySelectorAll(".print-preview-sheet").length, 1);
  assert.match(stage.textContent, /초기 문서 내용/);
  assert.equal($(document, "pp-page-status").textContent, "1 / 1 쪽");

  // Mutate #doc-editor and verify 80ms debounced live update in preview stage.
  p1.textContent = "실시간 문서 미리보기 반영 완료";
  editor.dispatchEvent({ type: "input", target: p1 });
  advanceTimers(100);
  await flushAsyncTurns(6);
  assert.match(stage.textContent, /실시간 문서 미리보기 반영 완료/);

  // 2. Switch to SHEET while preview is open and verify sheet preview + live cell mutation update.
  switchModule(document, "SHEET");
  await flushAsyncTurns(8);
  await pickFile(
    document,
    makeFixtureFile("미리보기.xlsx", {
      sheets: [{ name: "Sheet1", cells: [{ ref: "B2", raw: "매출액" }, { ref: "C2", raw: "4200" }] }],
    })
  );
  await flushAsyncTurns(12);
  await host.__EASYOFFICE_PRINT_PREVIEW__.refreshNow();

  assert.equal(pane.hidden, false);
  assert.ok(stage.querySelector(".eo-print-sheet"), "sheet table rendered in preview");
  assert.match(stage.textContent, /매출액/);
  assert.match(stage.textContent, /4,200|4200/);

  const cellC2 = document.querySelector('#pane-sheet td[data-cell="C2"]');
  assert.ok(cellC2, "C2 cell exists in #pane-sheet");
  cellC2.focus();
  cellC2.textContent = "9800";
  cellC2.dispatchEvent({ type: "input", target: cellC2 });
  cellC2.dispatchEvent({ type: "keydown", key: "Enter", target: cellC2 });
  await flushAsyncTurns(4);
  await host.__EASYOFFICE_PRINT_PREVIEW__.refreshNow();
  assert.match(stage.textContent, /9,800|9800/);

  // 3. Switch to SLIDE, add second slide + coordinate element, verify 2 pages + controls.
  switchModule(document, "SLIDE");
  await flushAsyncTurns(8);
  host.__EASYOFFICE_SLIDE_CANVAS__.addElement({
    kind: "shape",
    shapeType: "roundRect",
    x: 80,
    y: 120,
    w: 220,
    h: 90,
    text: "실시간 슬라이드 도형",
  });
  userClick($(document, "btn-add-slide"));
  await flushAsyncTurns(6);
  await host.__EASYOFFICE_PRINT_PREVIEW__.refreshNow();

  const sheets = stage.querySelectorAll(".print-preview-sheet");
  assert.equal(sheets.length, 2);
  assert.equal(sheets[0].dataset.pageNumber, "1");
  assert.equal(sheets[1].dataset.pageNumber, "2");
  assert.match(stage.textContent, /실시간 슬라이드 도형/);
  assert.equal($(document, "pp-page-status").textContent, "1 / 2 쪽");

  // Spread / single toggle
  userClick($(document, "btn-pp-spread"));
  assert.equal(stage.dataset.arrangement, "spread");
  assert.equal($(document, "btn-pp-spread").getAttribute("aria-pressed"), "true");
  assert.equal($(document, "btn-pp-single").getAttribute("aria-pressed"), "false");

  userClick($(document, "btn-pp-single"));
  assert.equal(stage.dataset.arrangement, "single");
  assert.equal($(document, "btn-pp-single").getAttribute("aria-pressed"), "true");

  // Zoom controls
  userClick($(document, "btn-pp-zoom-in"));
  assert.notEqual($(document, "pp-zoom-label").textContent, "100%");
  userClick($(document, "btn-pp-fit-width"));
  assert.match($(document, "pp-zoom-label").textContent, /^\d+%$/);
  userClick($(document, "btn-pp-fit-page"));
  assert.match($(document, "pp-zoom-label").textContent, /^\d+%$/);
  userClick($(document, "btn-pp-zoom-out"));
  assert.match($(document, "pp-zoom-label").textContent, /^\d+%$/);

  // Page navigation
  userClick($(document, "btn-pp-next"));
  assert.equal($(document, "pp-page-status").textContent, "2 / 2 쪽");
  userClick($(document, "btn-pp-prev"));
  assert.equal($(document, "pp-page-status").textContent, "1 / 2 쪽");

  // Print button delegates to officePrint.print()
  host.CSSStyleSheet = class FakePrintSheet {
    constructor() { this.cssRules = []; }
    insertRule(text, at = this.cssRules.length) {
      this.cssRules.splice(at, 0, { cssText: text, style: { props: {}, setProperty(k, v) { this.props[k] = v; } } });
      return at;
    }
  };
  document.adoptedStyleSheets = [];
  userClick($(document, "btn-pp-print"));
  await flushAsyncTurns(8);
  assert.equal(printed.length, 1);
  sandbox.dispatchWindowEvent("afterprint");
  assert.equal(document.getElementById("eo-print-root"), null);

  // Switching to unsupported module (PDF) auto-hides preview pane.
  switchModule(document, "PDF");
  await flushAsyncTurns(6);
  assert.equal(pane.hidden, true);

  // Reopen in DOC and close via 닫기 button.
  switchModule(document, "DOC");
  await flushAsyncTurns(6);
  userClick(ppMenu);
  await flushAsyncTurns(6);
  assert.equal(pane.hidden, false);
  userClick($(document, "btn-pp-close"));
  await flushAsyncTurns(2);
  assert.equal(pane.hidden, true);
});
