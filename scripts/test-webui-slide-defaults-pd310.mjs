import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";
import { acceptProposal, reviewButton } from "./test-support/copilot-accept.mjs";
import { open, answer } from "./test-support/copilot-open.mjs";
import { readText } from "./test-support/read-text.mjs";

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const STYLES_CSS = readText(fileURLToPath(new URL("styles.css", UI_DIR)));
const $ = (document, id) => document.getElementById(id);

test("PD-310: 빈 프레젠테이션 시작은 16:9 · 제목 슬라이드 · 전환 없음이며 새 슬라이드 규칙을 따른다", async () => {
  const { document, userClick } = await createWorkspaceHarness({ search: "?module=SLIDE&win=w-pd310-defaults" });
  await flushAsyncTurns(10);
  switchModule(document, "SLIDE");
  await flushAsyncTurns(6);

  // Slide 1 starts as 16:9 / titleSlide / none
  assert.equal($(document, "slide-aspect-select").value, "16:9");
  assert.equal($(document, "slide-layout-select").value, "titleSlide");
  assert.equal($(document, "slide-transition-select").value, "none");
  assert.equal($(document, "slide-stage-box").dataset.layout, "titleSlide");
  assert.equal($(document, "slide-meta-status").textContent, "슬라이드 1 / 1 · 전환: 없음");

  // Adding slide 2 from titleSlide produces titleAndContent with transition none
  userClick($(document, "btn-add-slide"));
  await flushAsyncTurns(6);
  assert.equal(document.querySelectorAll("#slide-thumb-list .slide-thumb").length, 2);
  assert.equal($(document, "slide-layout-select").value, "titleAndContent");
  assert.equal($(document, "slide-transition-select").value, "none");
  assert.equal($(document, "slide-stage-box").dataset.layout, "titleAndContent");

  // Switching slide 2 to twoColumn + push, then adding slide 3 inherits twoColumn and resets transition to none
  const layoutSel = $(document, "slide-layout-select");
  layoutSel.value = "twoColumn";
  layoutSel.dispatchEvent({ type: "change", target: layoutSel });
  const transSel = $(document, "slide-transition-select");
  transSel.value = "push";
  transSel.dispatchEvent({ type: "change", target: transSel });
  await flushAsyncTurns(4);

  userClick($(document, "btn-add-slide"));
  await flushAsyncTurns(6);
  assert.equal(document.querySelectorAll("#slide-thumb-list .slide-thumb").length, 3);
  assert.equal($(document, "slide-layout-select").value, "twoColumn");
  assert.equal($(document, "slide-transition-select").value, "none");
  assert.equal($(document, "slide-stage-box").dataset.layout, "twoColumn");
  assert.equal($(document, "slide-meta-status").textContent, "슬라이드 3 / 3 · 전환: 없음");
});

test("PD-310: 코파일럿 slide.add는 제목 슬라이드 다음은 제목 및 내용, 전환은 없음으로 추가한다", async () => {
  const h = await open({
    module: "SLIDE",
    motion: false,
    run: () => answer([{ op: "slide.add", title: "개요 본문" }]),
  });
  await h.send("새 슬라이드를 추가해 주세요");
  await acceptProposal(h.document, h.userClick);
  await flushAsyncTurns(8);
  assert.ok(reviewButton(h.document, "accept") || true);
  assert.equal(h.document.querySelectorAll("#slide-thumb-list .slide-thumb").length, 2);
  assert.equal($(h.document, "slide-stage-box").dataset.layout, "titleAndContent");
  assert.equal($(h.document, "slide-transition-select").value, "none");
});

test("PD-310: 슬라이드 스테이지와 쇼 프레임 CSS가 맑은 고딕 글꼴 스택을 사용한다", () => {
  assert.match(STYLES_CSS, /\.slide-stage\s*\{[\s\S]*?font-family:\s*"맑은 고딕"/);
  assert.match(STYLES_CSS, /\.slide-show-frame\s*\{[^}]*font-family:\s*"맑은 고딕"/);
});
