import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { REDUCED_MOTION } from "./test-support/copilot-accept.mjs";

// PD-387: the copilot panel as a chat screen. The conversation title and the menu in the header, the user's bubble, the
// answer as plain text under a folded "N초 동안 작업" line, the small row under each message, and the composer card.

async function open({ run }) {
  const seen = [];
  const copied = [];
  const harness = await createWorkspaceHarness({
    search: "?module=DOC&win=w-copilot-panel-ui",
    matchMedia: REDUCED_MOTION,
    clipboard: { writeText: async (text) => { copied.push(text); } },
    testIpc: async (command, payload) => {
      if (command === "run_smart_assistant") {
        const parsed = JSON.parse(payload);
        seen.push(parsed);
        if (parsed.action === "copilot_run") return run(parsed, seen);
      }
      const cat = harness.sandbox.__EASYOFFICE_RIBBON_CATALOG__;
      const offline = harness.sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
        getActiveModule: () => "DOC",
        ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
        ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
        contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
        moduleSpecs: cat.MODULE_SPECS,
        localRibbonState: cat.createDefaultRibbonState(),
      });
      return offline[command](payload);
    },
  });
  const { document, userClick } = harness;
  await flushAsyncTurns(12);
  userClick(document.getElementById("btn-copilot-toggle"));
  userClick(document.getElementById("btn-copilot-model"));
  const ollama = Array.from(document.getElementById("copilot-model-menu").querySelectorAll('button[role="menuitemradio"]')).find((b) => b.dataset.quickKind === "ollama");
  userClick(ollama);
  await flushAsyncTurns(6);
  const input = document.getElementById("copilot-input");
  const send = async (text, turns = 14) => {
    input.value = text;
    input.dispatchEvent({ type: "input", target: input });
    userClick(document.getElementById("btn-copilot-send"));
    await flushAsyncTurns(turns);
  };
  const runs = () => seen.filter((r) => r.action === "copilot_run");
  return { ...harness, send, runs, copied };
}

const toolThenAnswer = (_, seen) => (seen.filter((r) => r.action === "copilot_run").length === 1
  ? { status: "ok", mode: "ask", reply: "", operations: [], locations: [], toolCalls: [{ name: "read_doc_blocks", start: 0, count: 3 }] }
  : { status: "ok", mode: "ask", reply: "세 문단입니다.", operations: [], locations: [] });

test("relativeTime reads in Korean from the clock: 지금, 분, 시간, 일", () => {
  const env = {};
  loadUiModule(fileURLToPath(new URL("../crates/easyoffice-app/ui/copilot-thread-view.js", import.meta.url)), env);
  const { relativeTime } = env.__EASYOFFICE_COPILOT_THREAD_VIEW__;
  const now = 1_700_000_000_000;
  assert.equal(relativeTime(now - 10_000, now), "지금");
  assert.equal(relativeTime(now - 120_000, now), "2분 전");
  assert.equal(relativeTime(now - 3 * 3_600_000, now), "3시간 전");
  assert.equal(relativeTime(now - 3 * 86_400_000, now), "3일 전");
});

test("step labels name the read in Korean with numbers and ranges only, and fall back for an unknown tool", () => {
  const env = {};
  loadUiModule(fileURLToPath(new URL("../crates/easyoffice-app/ui/copilot-step-labels.js", import.meta.url)), env);
  const { describeToolCall, FALLBACK } = env.__EASYOFFICE_COPILOT_STEP_LABELS__;
  assert.equal(describeToolCall({ name: "read_doc_blocks", startIndex: 0, endIndex: 2 }), "문단 1~3 읽음");
  assert.equal(describeToolCall({ name: "read_doc_blocks", startIndex: 4, count: 3 }), "문단 5~7 읽음");
  assert.equal(describeToolCall({ name: "read_hwp_paragraphs", startIndex: 9, endIndex: 9 }), "문단 10 읽음");
  assert.equal(describeToolCall({ name: "read_sheet_range", range: "a1:c10", sheet: "비밀 시트" }), "A1:C10 범위 읽음");
  assert.equal(describeToolCall({ name: "read_sheet_range", range: "주민번호 열" }), "시트 범위 읽음", "a range that is not cells is not echoed");
  assert.equal(describeToolCall({ name: "read_base_records", offset: 0, limit: 20 }), "레코드 1~20 읽음");
  const secret = describeToolCall({ name: "find_sheet_cells", query: "홍길동 010-1234-5678" });
  assert.equal(secret, "시트에서 셀 찾음");
  assert.equal(describeToolCall({ name: "search_pdf_pages", query: "계약서" }), "PDF 쪽 검색");
  assert.equal(describeToolCall({ name: "something_new", query: "x" }), FALLBACK);
  assert.equal(FALLBACK, "문서 내용 확인");
  assert.equal(describeToolCall(null), FALLBACK);
});

test("a turn shows the title, a right-side bubble row, a folded work line and the rows under each message", async () => {
  const { document, userClick, send, runs, copied } = await open({ run: toolThenAnswer });
  assert.equal(document.getElementById("copilot-title").textContent, "새 대화");
  await send("문서를 요약해 주세요");
  assert.equal(runs().length, 2, "the read tool ran and the loop asked again");
  assert.equal(document.getElementById("copilot-title").textContent, "문서를 요약해 주세요");

  const rows = document.querySelectorAll("#copilot-thread .eo-copilot-row");
  assert.equal(rows.length, 2);
  assert.ok(rows[0].classList.contains("eo-copilot-row--user"));
  assert.equal(rows[0].querySelector(".eo-copilot-msg--user").textContent.trim(), "문서를 요약해 주세요", "the time and buttons are outside the bubble");
  assert.equal(rows[0].querySelectorAll(".eo-copilot-meta-btn").length, 2, "copy and resend under the question");
  assert.ok(rows[0].querySelector("[data-at]"), "the question carries its time");

  const toggle = rows[1].querySelector(".eo-copilot-steps-toggle");
  assert.match(toggle.textContent, /^\d+초 동안 작업$/);
  const panel = rows[1].querySelector(".eo-copilot-steps-panel");
  assert.equal(toggle.getAttribute("aria-expanded"), "false");
  assert.equal(panel.hidden, true, "the steps are folded");
  userClick(toggle);
  assert.equal(toggle.getAttribute("aria-expanded"), "true");
  assert.equal(panel.hidden, false);
  assert.match(panel.textContent, /문단 1~3 읽음/);
  assert.doesNotMatch(panel.textContent, /read_doc_blocks/, "no internal tool name in the steps");

  const meta = rows[1].querySelector(".eo-copilot-meta");
  const [copy, up, down] = meta.querySelectorAll(".eo-copilot-meta-btn");
  userClick(copy);
  await flushAsyncTurns(2);
  assert.deepEqual(copied, ["세 문단입니다."]);
  userClick(up);
  assert.equal(up.getAttribute("aria-pressed"), "true");
  userClick(down);
  assert.equal(up.getAttribute("aria-pressed"), "false", "one rating at a time");
  assert.equal(down.getAttribute("aria-pressed"), "true");
  userClick(down);
  assert.equal(down.getAttribute("aria-pressed"), "false", "pressing again takes it back");
});

test("다시 보내기 asks the same question again as a new turn", async () => {
  const { document, userClick, send, runs } = await open({ run: () => ({ status: "ok", mode: "ask", reply: "답", operations: [], locations: [] }) });
  await send("같은 질문");
  userClick(document.querySelector("#copilot-thread .eo-copilot-resend-btn"));
  await flushAsyncTurns(14);
  assert.equal(runs().length, 2);
  assert.equal(runs()[1].prompt, "같은 질문");
  assert.equal(document.querySelectorAll("#copilot-thread .eo-copilot-msg--user").length, 2);
});

test("the waiting line shimmers while the answer is pending and goes away with it", async () => {
  let release;
  const held = new Promise((resolve) => { release = resolve; });
  const { document, send } = await open({ run: () => held });
  await send("기다리는 질문", 4);
  const waiting = document.querySelector("#copilot-thread .eo-copilot-shimmer");
  assert.equal(waiting.textContent, "응답을 기다리는 중…");
  assert.ok(waiting.classList.contains("eo-copilot-loading"));
  assert.equal(document.querySelectorAll("#copilot-thread .eo-copilot-meta-btn[data-feedback]").length, 0, "no rating row before the answer");
  release({ status: "ok", mode: "ask", reply: "끝", operations: [], locations: [] });
  await flushAsyncTurns(8);
  assert.equal(document.querySelector("#copilot-thread .eo-copilot-shimmer"), null);
});

test("the header menu opens with 새 대화 and 모델 설정, and 새 대화 empties the thread and the title", async () => {
  const { document, userClick, send } = await open({ run: () => ({ status: "ok", mode: "ask", reply: "답", operations: [], locations: [] }) });
  await send("지울 대화");
  const more = document.getElementById("btn-copilot-more");
  const menu = document.getElementById("copilot-more-menu");
  assert.equal(menu.hidden, true);
  userClick(more);
  assert.equal(menu.hidden, false);
  assert.equal(more.getAttribute("aria-expanded"), "true");
  assert.deepEqual(Array.from(menu.querySelectorAll("button")).map((b) => b.textContent.trim()), ["새 대화", "모델 설정"]);
  userClick(document.getElementById("btn-copilot-new"));
  assert.equal(menu.hidden, true, "picking an item closes the menu");
  assert.equal(document.getElementById("copilot-thread").children.length, 0);
  assert.equal(document.getElementById("copilot-title").textContent, "새 대화");
});

test("the + menu lists the example questions and fills the input without sending", async () => {
  const { document, userClick, runs } = await open({ run: toolThenAnswer });
  const plus = document.getElementById("btn-copilot-plus");
  userClick(plus);
  const menu = document.getElementById("copilot-plus-menu");
  assert.equal(menu.hidden, false);
  const first = menu.querySelector("button");
  assert.ok(first, "the active module has examples");
  userClick(first);
  assert.equal(menu.hidden, true);
  assert.notEqual(document.getElementById("copilot-input").value, "");
  assert.equal(runs().length, 0, "an example is not sent");
});
