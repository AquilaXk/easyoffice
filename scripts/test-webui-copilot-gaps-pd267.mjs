import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { createSelection } from "./test-support/doc-run-harness.mjs";
import { REDUCED_MOTION, acceptProposal } from "./test-support/copilot-accept.mjs";

// PD-267: stopping a request for real, asking again for the last answer, the dropped-operation notice and the
// selection commands in the copilot panel.

async function open({ run, module = "DOC", selection }) {
  const seen = [];
  const harness = await createWorkspaceHarness({
    search: `?module=${module}&win=w-copilot-gaps-${module}`,
    matchMedia: REDUCED_MOTION,
    selection,
    testIpc: async (command, payload) => {
      if (command === "run_smart_assistant") {
        const parsed = JSON.parse(payload);
        seen.push(parsed);
        if (parsed.action === "copilot_run") return run(parsed, seen);
        if (parsed.action === "copilot_cancel") return { status: "ok", cancelled: true };
      }
      const cat = harness.sandbox.__EASYOFFICE_RIBBON_CATALOG__;
      const offline = harness.sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
        getActiveModule: () => module,
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
  const assistants = () => document.querySelectorAll("#copilot-thread .eo-copilot-msg--assistant");
  const button = (scope, label) => Array.from(scope.querySelectorAll("button")).find((b) => b.textContent.trim() === label);
  const accept = () => acceptProposal(document, userClick);
  return { ...harness, seen, send, runs, assistants, button, input, accept };
}

test("stop sends a cancel for the running request and shows it stopped", async () => {
  let release;
  const held = new Promise((resolve) => { release = resolve; });
  const { document, userClick, send, runs, seen } = await open({ run: () => held });
  await send("문서를 요약해 주세요", 4);
  const sendBtn = document.getElementById("btn-copilot-send");
  assert.equal(sendBtn.getAttribute("aria-label"), "중지");
  assert.match(runs()[0].requestId, /^[A-Za-z0-9_-]{1,64}$/);
  userClick(sendBtn);
  await flushAsyncTurns(6);
  const cancels = seen.filter((r) => r.action === "copilot_cancel");
  assert.equal(cancels.length, 1);
  assert.equal(cancels[0].requestId, runs()[0].requestId);
  assert.equal(sendBtn.getAttribute("aria-label"), "보내기");
  assert.match(document.getElementById("copilot-thread").textContent, /요청을 중지했습니다/);
  release({ status: "ok", mode: "ask", reply: "늦은 답", operations: [] });
  await flushAsyncTurns(6);
  assert.doesNotMatch(document.getElementById("copilot-thread").textContent, /늦은 답/, "a late answer after stop is not shown");
});

test("operations the engine dropped are reported on the status line", async () => {
  const { document, send } = await open({ run: () => ({ status: "ok", mode: "edit", reply: "답", updatedInPlace: false, droppedOperations: 2, operations: [], locations: [] }) });
  await send("문단을 추가해 주세요");
  assert.equal(document.getElementById("status-command-feedback").textContent, "형식이 맞지 않는 작업 2개를 건너뛰었습니다");
  const { document: other, send: sendOther } = await open({ run: () => ({ status: "ok", mode: "edit", reply: "답", droppedOperations: 0, operations: [] }) });
  await sendOther("문단을 추가해 주세요");
  assert.doesNotMatch(other.getElementById("status-command-feedback").textContent, /건너뛰었/);
});

test("regenerate asks the same prompt again and replaces the last answer", async () => {
  let n = 0;
  const { document, userClick, send, runs, assistants, button } = await open({
    run: () => ({ status: "ok", mode: "ask", reply: "답 " + (n += 1), operations: [], locations: [] }),
  });
  await send("핵심을 알려 주세요");
  assert.equal(button(assistants()[0], "다시 생성") !== undefined, true);
  userClick(button(assistants()[0], "다시 생성"));
  await flushAsyncTurns(14);
  assert.equal(runs().length, 2);
  assert.equal(runs()[1].prompt, "핵심을 알려 주세요");
  assert.deepEqual(runs()[1].history, [], "the answer being replaced is not part of the history");
  assert.equal(document.querySelectorAll("#copilot-thread .eo-copilot-msg--user").length, 1);
  assert.equal(assistants().length, 1);
  assert.match(assistants()[0].textContent, /답 2/);
  assert.notEqual(runs()[0].requestId, runs()[1].requestId);
  await send("다음 질문");
  assert.equal(button(assistants()[0], "다시 생성"), undefined, "only the last answer can be regenerated");
  assert.equal(button(assistants()[1], "다시 생성") !== undefined, true);
});

test("regenerate in edit mode takes the earlier edit back before applying the new one", async () => {
  let n = 0;
  const { document, userClick, send, assistants, button, accept } = await open({
    run: () => ({ status: "ok", mode: "edit", updatedInPlace: true, reply: "ok", operations: [{ op: "doc.append", text: "추가 " + (n += 1) }] }),
  });
  const editor = document.getElementById("doc-editor");
  await send("문단을 추가해 주세요");
  assert.equal(editor.textContent.includes("추가 1"), false, "nothing is written until 수락");
  await accept();
  assert.ok(editor.textContent.includes("추가 1"));
  userClick(button(assistants()[0], "다시 생성"));
  await flushAsyncTurns(24);
  await accept();
  assert.ok(editor.textContent.includes("추가 2"));
  assert.equal(editor.textContent.includes("추가 1"), false, "the first answer's edit was undone");
});

test("regenerate and undo are gone once the user edited after the answer", async () => {
  const { document, send, assistants, button, accept } = await open({
    run: () => ({ status: "ok", mode: "edit", updatedInPlace: true, reply: "ok", operations: [{ op: "doc.append", text: "추가" }] }),
  });
  const editor = document.getElementById("doc-editor");
  await send("문단을 추가해 주세요");
  await accept();
  assert.ok(button(assistants()[0], "실행 취소") && !button(assistants()[0], "실행 취소").disabled);
  const p = editor.querySelector("p");
  editor.dispatchEvent({ type: "focus", target: editor });
  p.textContent = p.textContent + " 직접 고침";
  editor.dispatchEvent({ type: "input", target: p });
  document.dispatchEvent({ type: "keyup" });
  await flushAsyncTurns(4);
  assert.equal(button(assistants()[0], "실행 취소").disabled, true, "undo is disabled after a user edit");
  assert.equal(button(assistants()[0], "다시 생성").disabled, true, "regenerate needs the undo");
});

function rangeOver(node, text) {
  return { startContainer: node, endContainer: node, commonAncestorContainer: node, toString: () => text };
}

test("a text selection shows the selection commands and runs the chosen one on that selection", async () => {
  const selection = createSelection();
  Object.defineProperty(selection, "isCollapsed", { get() { return !this._range || this._range.collapsed === true; } });
  const { document, userClick, runs, assistants, seen, accept } = await open({
    selection,
    run: (req) => ({ status: "ok", mode: "edit", updatedInPlace: true, reply: "다듬음", operations: [{ op: "doc.replace_paragraphs", replacements: [{ index: req.selection.paragraphIndex, text: "새 문장" }] }] }),
  });
  const editor = document.getElementById("doc-editor");
  const row = document.getElementById("copilot-command-row");
  assert.equal(row.hidden, true, "no selection, no commands");

  const target = editor.querySelectorAll("p")[0];
  selection.addRange(rangeOver(target, "고칠 문장"));
  document.dispatchEvent({ type: "keyup" });
  await flushAsyncTurns(3);
  assert.equal(row.hidden, false);
  const labels = Array.from(row.querySelectorAll("button")).map((b) => b.textContent.trim());
  assert.deepEqual(labels, ["다시 쓰기", "요약", "더 짧게", "더 길게", "맞춤법 수정", "번역"]);

  userClick(Array.from(row.querySelectorAll("button")).find((b) => b.textContent.trim() === "요약"));
  await flushAsyncTurns(16);
  const req = runs().at(-1);
  assert.equal(req.command, "summarize");
  assert.equal(req.mode, "edit");
  assert.equal(req.prompt, "요약");
  assert.equal(req.selection.paragraphIndex, 0);
  assert.equal(req.selection.paragraphEnd, 0);
  assert.notEqual(editor.querySelectorAll("p")[0].textContent, "새 문장", "the proposal waits for 수락");
  await accept();
  assert.equal(editor.querySelectorAll("p")[0].textContent, "새 문장", "the result went through the replace_paragraphs path");
  assert.match(assistants()[0].textContent, /다듬음/);

  // Ask mode has no edits, so no commands.
  userClick(document.getElementById("btn-copilot-mode"));
  document.dispatchEvent({ type: "keyup" });
  await flushAsyncTurns(3);
  assert.equal(row.hidden, true);
  assert.equal(seen.filter((r) => r.action === "copilot_run").length, 1);
});

test("regenerate of a command asks again only while its blocks are what they were", async () => {
  const selection = createSelection();
  Object.defineProperty(selection, "isCollapsed", { get() { return !this._range || this._range.collapsed === true; } });
  const { document, userClick, runs, assistants, button } = await open({
    selection,
    run: () => ({ status: "ok", mode: "edit", updatedInPlace: false, reply: "손볼 곳이 없습니다", operations: [] }),
  });
  const editor = document.getElementById("doc-editor");
  const target = editor.querySelectorAll("p")[0];
  selection.addRange(rangeOver(target, "고칠 문장"));
  document.dispatchEvent({ type: "keyup" });
  await flushAsyncTurns(3);
  const row = document.getElementById("copilot-command-row");
  userClick(Array.from(row.querySelectorAll("button")).find((b) => b.textContent.trim() === "맞춤법 수정"));
  await flushAsyncTurns(16);
  assert.equal(runs().length, 1);

  userClick(button(assistants()[0], "다시 생성"));
  await flushAsyncTurns(16);
  assert.equal(runs().length, 2, "the blocks are the same, so it asks again");
  assert.equal(runs()[1].command, "fix_spelling");
  assert.deepEqual(runs()[1].selection, runs()[0].selection);

  target.textContent = target.textContent + " 직접 고침";
  userClick(button(assistants()[0], "다시 생성"));
  await flushAsyncTurns(16);
  assert.equal(runs().length, 2, "the target changed, nothing is sent");
  assert.equal(document.getElementById("status-command-feedback").textContent, "선택한 내용이 바뀌어 다시 생성할 수 없습니다");
  assert.equal(assistants().length, 1, "the answer stays");
});

test("BASE and BOARD questions go to the engine as table and board context, and the panel stays in ask mode", async () => {
  for (const [module, key] of [["BASE", "base"], ["BOARD", "board"]]) {
    const { document, send, runs } = await open({ module, run: () => ({ status: "ok", mode: "ask", reply: "답", operations: [] }) });
    const modeBtn = document.getElementById("btn-copilot-mode");
    assert.equal(modeBtn.textContent.trim(), "질문", module);
    assert.equal(modeBtn.disabled, true, module);
    await send("현재 내용을 요약해 주세요");
    const req = runs()[0];
    assert.equal(req.module, module);
    assert.equal(req.mode, "ask");
    assert.ok(req.context[key], module + " context present");
    assert.equal(req.context.doc, undefined);
  }
});
