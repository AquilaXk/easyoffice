import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule, createMemoryStorage } from "./test-support/modules.mjs";

const COPILOT_CSS = fs.readFileSync(new URL("../crates/easyoffice-app/ui/copilot.css", import.meta.url), "utf8");

function seedSheetCell(document, coord, raw) {
  const td = document.querySelector(`#pane-sheet td[data-cell="${coord}"]`);
  assert.ok(td, `missing cell ${coord}`);
  td.focus();
  td.textContent = raw;
  td.dispatchEvent({ type: "input", target: td });
  td.blur();
}

async function seedRevenueSheet(document) {
  const rows = [
    ["A1", "지역"], ["B1", "목표"], ["C1", "실제"],
    ["A2", "서부"], ["B2", "4.20"], ["C2", "3.90"],
    ["A3", "남부"], ["B3", "3.30"], ["C3", "3.12"],
    ["A4", "북동부"], ["B4", "2.70"], ["C4", "2.16"],
    ["A5", "중서부"], ["B5", "1.80"], ["C5", "1.62"],
  ];
  for (const [coord, val] of rows) {
    seedSheetCell(document, coord, val);
    await flushAsyncTurns(3);
  }
  document.querySelector('#pane-sheet td[data-cell="A1"]').focus();
  await flushAsyncTurns(4);
}

async function selectLocalOllama(document, userClick) {
  const modelBtn = document.getElementById("btn-copilot-model");
  const modelMenu = document.getElementById("copilot-model-menu");
  if (modelMenu.hidden) userClick(modelBtn);
  const ollamaBtn = Array.from(modelMenu.querySelectorAll('button[role="menuitemradio"]')).find(
    (b) => b.dataset.quickKind === "ollama"
  );
  assert.ok(ollamaBtn, "Ollama local provider available in model menu");
  userClick(ollamaBtn);
  await flushAsyncTurns(6);
}

test("코파일럿 패널은 기본 닫힘이며 모듈 가시성, 진입점 토글, 너비 리사이즈, 시작 예시 입력을 지원한다", async () => {
  const startHarness = await createWorkspaceHarness({
    search: "?window=start&win=w-copilot-start",
    shellCommands: {
      list_recent_documents: () => [],
      open_document_window: () => null,
      open_file_in_new_window: () => null,
      take_pending_open: () => null,
      report_window_state: () => null,
      close_current_window: () => null,
    },
  });
  await flushAsyncTurns(12);
  assert.equal(startHarness.document.getElementById("eo-copilot-shell")?.hidden, true, "start window hides copilot shell");

  const { document, sandbox, userClick } = await createWorkspaceHarness({ search: "?module=DOC&win=w-copilot-doc" });
  await flushAsyncTurns(10);

  assert.equal(document.querySelector(".workspace-grid > aside"), null, "no direct aside inside .workspace-grid");
  const stageRow = document.querySelector(".workspace-grid > .workspace-stage-row");
  assert.ok(stageRow, ".workspace-stage-row wraps canvas and copilot");
  const shell = document.getElementById("eo-copilot-shell");
  const panel = document.getElementById("copilot-panel");
  const topBtn = document.getElementById("btn-copilot-toggle");
  assert.ok(shell && panel && topBtn, "copilot chrome elements exist");
  assert.equal(document.getElementById("btn-copilot-edge"), null, "no protruding edge button overlapping toolbar or ruler");
  assert.equal(shell.querySelectorAll("p").length, 0, "no <p> tags inside #eo-copilot-shell");

  assert.equal(shell.hidden, true, "closed copilot shell is completely hidden (0px width)");
  assert.equal(panel.hidden, true);
  assert.equal(topBtn.getAttribute("aria-pressed"), "false");

  switchModule(document, "HWP");
  await flushAsyncTurns(4);
  assert.equal(shell.hidden, true, "hidden in HWP module");
  assert.equal(topBtn.hidden, true, "topbar toggle hidden in HWP module");

  switchModule(document, "CONV");
  await flushAsyncTurns(4);
  assert.equal(shell.hidden, true, "hidden in CONV module");

  switchModule(document, "SHEET");
  await flushAsyncTurns(4);
  assert.equal(shell.hidden, true, "still closed until toggled open");
  assert.equal(topBtn.hidden, false, "topbar toggle visible in SHEET module");

  userClick(topBtn);
  await flushAsyncTurns(2);
  assert.equal(shell.hidden, false);
  assert.equal(panel.hidden, false);
  assert.equal(topBtn.getAttribute("aria-pressed"), "true");
  assert.equal(shell.style.getPropertyValue("--copilot-width"), "320px");

  const resizer = document.getElementById("copilot-resizer");
  assert.ok(resizer);
  resizer.dispatchEvent({ type: "keydown", key: "ArrowLeft" });
  assert.equal(shell.style.getPropertyValue("--copilot-width"), "340px");
  resizer.dispatchEvent({ type: "keydown", key: "ArrowLeft", shiftKey: true });
  assert.equal(shell.style.getPropertyValue("--copilot-width"), "380px");
  resizer.dispatchEvent({ type: "keydown", key: "End" });
  assert.equal(shell.style.getPropertyValue("--copilot-width"), "520px");
  resizer.dispatchEvent({ type: "keydown", key: "Home" });
  assert.equal(shell.style.getPropertyValue("--copilot-width"), "280px");
  resizer.dispatchEvent({ type: "dblclick" });
  assert.equal(shell.style.getPropertyValue("--copilot-width"), "320px");

  resizer.dispatchEvent({ type: "pointerdown", button: 0, clientX: 700, pointerId: 3 });
  resizer.dispatchEvent({ type: "pointermove", clientX: 610, pointerId: 3 });
  assert.equal(shell.style.getPropertyValue("--copilot-width"), "410px");
  resizer.dispatchEvent({ type: "keydown", key: "Escape" });
  assert.equal(shell.style.getPropertyValue("--copilot-width"), "320px", "Escape cancels active drag");

  resizer.dispatchEvent({ type: "pointerdown", button: 0, clientX: 700, pointerId: 4 });
  resizer.dispatchEvent({ type: "pointermove", clientX: 620, pointerId: 4 });
  resizer.dispatchEvent({ type: "pointerup", clientX: 620, pointerId: 4 });
  assert.equal(shell.style.getPropertyValue("--copilot-width"), "400px");
  resizer.dispatchEvent({ type: "keydown", key: "Enter" });
  assert.equal(shell.style.getPropertyValue("--copilot-width"), "320px");

  const welcome = document.getElementById("copilot-welcome");
  const examples = welcome.querySelectorAll(".eo-copilot-example-btn");
  assert.equal(welcome.hidden, false);
  assert.equal(examples.length, 2);
  assert.equal(examples[0].textContent.trim(), "목표 대비 % 및 최대 격차 표시");
  userClick(examples[0]);
  const input = document.getElementById("copilot-input");
  assert.equal(input.value, "목표 대비 %를 추가하고 가장 큰 격차를 표시하세요.");
  assert.equal(welcome.hidden, true, "draft hides welcome");
  assert.equal(document.getElementById("copilot-thread").children.length, 0, "example does not auto-send");
  assert.equal(document.getElementById("btn-copilot-mode").textContent.trim(), "편집");

  userClick(document.getElementById("btn-copilot-close"));
  assert.equal(shell.hidden, true);

  const menuCmd = document.querySelector('.md-item[data-studio-menu-cmd="copilot-toggle"]');
  assert.ok(menuCmd);
  userClick(menuCmd);
  assert.equal(shell.hidden, false);

  const keyEv = sandbox.dispatchWindowKeydown({ key: "K", ctrlKey: true, shiftKey: true });
  assert.equal(keyEv.defaultPrevented, true);
  assert.equal(shell.hidden, true);
});

test("SHEET 편집 모드는 목표 대비 % 수식·백분율 서식·최대 격차 강조를 제자리에서 반영하고 D2:D5 칩 선택 이동과 실행 취소를 수행한다", async () => {
  const { document, userClick } = await createWorkspaceHarness({ search: "?module=SHEET&win=w-copilot-sheet" });
  await flushAsyncTurns(12);
  await seedRevenueSheet(document);

  userClick(document.getElementById("btn-copilot-toggle"));
  assert.equal(document.documentElement.innerHTML.includes("내장 오피스 엔진"), false, "no fake built-in AI engine in DOM");
  assert.equal(document.getElementById("copilot-model-btn-text").textContent.trim(), "모델 선택");

  const firstExample = document.querySelector("#copilot-welcome .eo-copilot-example-btn");
  userClick(firstExample);
  const input = document.getElementById("copilot-input");

  // Sending before configuring any model opens Model Settings Fail-Closed and keeps the user's draft prompt intact
  input.dispatchEvent({ type: "keydown", key: "Enter" });
  await flushAsyncTurns(3);
  const dlg = document.getElementById("copilot-model-dialog");
  assert.equal(dlg.open, true, "sending without configured model opens Model Settings");
  assert.equal(input.value, "목표 대비 %를 추가하고 가장 큰 격차를 표시하세요.", "draft prompt preserved");
  assert.equal(document.getElementById("copilot-thread").children.length, 0, "no message sent before model configuration");
  dlg.close();

  await selectLocalOllama(document, userClick);

  input.dispatchEvent({ type: "keydown", key: "Enter", isComposing: true });
  await flushAsyncTurns(4);
  assert.equal(document.getElementById("copilot-thread").children.length, 0, "IME composition Enter must not send");

  input.dispatchEvent({ type: "keydown", key: "Enter" });
  await flushAsyncTurns(20);

  const userMsg = document.querySelector("#copilot-thread .eo-copilot-msg--user");
  const assistantMsg = document.querySelector("#copilot-thread .eo-copilot-msg--assistant");
  assert.ok(userMsg && assistantMsg);
  assert.equal(userMsg.textContent.trim(), "목표 대비 %를 추가하고 가장 큰 격차를 표시하세요.");

  const inplaceRow = assistantMsg.querySelector(".eo-copilot-inplace-row");
  assert.ok(inplaceRow, "updatedInPlace status row rendered");
  assert.equal(inplaceRow.textContent.trim(), "제자리에서 업데이트됨");

  const cellText = (coord) => document.querySelector(`#pane-sheet td[data-cell="${coord}"]`).textContent;
  assert.equal(cellText("D1"), "목표 대비 %");
  assert.equal(cellText("D2"), "92.9%");
  assert.equal(cellText("D3"), "94.5%");
  assert.equal(cellText("D4"), "80.0%");
  assert.equal(cellText("D5"), "90.0%");

  const d4 = document.querySelector('#pane-sheet td[data-cell="D4"]');
  d4.focus();
  await flushAsyncTurns(3);
  assert.equal(document.getElementById("formula-input").value, "=C4/B4");

  const a4 = document.querySelector('#pane-sheet td[data-cell="A4"]');
  assert.equal(a4.style.getPropertyValue("--cell-fill"), "#fff3cd", "biggest gap row highlighted");
  assert.equal(a4.dataset.fill, "1");
  assert.ok(d4.style.fontWeight === "600" || d4.style.fontWeight === "bold", "gap percentage bolded");

  const chips = assistantMsg.querySelectorAll(".eo-copilot-loc-chip");
  const chipLabels = chips.map((c) => c.textContent.trim());
  assert.ok(chipLabels.includes("D2:D5"), `expected D2:D5 chip in ${JSON.stringify(chipLabels)}`);
  assert.ok(chipLabels.includes("A4:D4"), `expected A4:D4 chip in ${JSON.stringify(chipLabels)}`);

  const rangeChip = chips.find((c) => c.textContent.trim() === "D2:D5");
  userClick(rangeChip);
  await flushAsyncTurns(4);
  for (const coord of ["D2", "D3", "D4", "D5"]) {
    const cell = document.querySelector(`#pane-sheet td[data-cell="${coord}"]`);
    assert.ok(
      cell.classList.contains("in-range") || cell.classList.contains("selected-cell"),
      `${coord} must be selected after clicking D2:D5 chip`
    );
  }
  assert.equal(
    document.querySelector('#pane-sheet td[data-cell="A2"]').classList.contains("in-range"),
    false,
    "selection moved strictly to D2:D5"
  );

  const undoBtn = assistantMsg.querySelector(".eo-copilot-undo-btn");
  assert.ok(undoBtn, "undo button rendered for latest applied turn");
  assert.equal(undoBtn.textContent.trim(), "실행 취소");
  userClick(undoBtn);
  await flushAsyncTurns(12);

  assert.equal(cellText("D1"), "");
  assert.equal(cellText("D2"), "");
  assert.equal(cellText("D4"), "");
  assert.equal(cellText("A4"), "북동부");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="A4"]').style.getPropertyValue("--cell-fill"), "");
});

test("질문 모드는 시트를 변경하지 않으며 PDF 모듈은 질문 모드로 고정되고 기존 문장 진단 대화상자와 간섭하지 않는다", async () => {
  const { document, userClick } = await createWorkspaceHarness({ search: "?module=SHEET&win=w-copilot-ask" });
  await flushAsyncTurns(12);
  await seedRevenueSheet(document);

  userClick(document.getElementById("btn-copilot-toggle"));
  await selectLocalOllama(document, userClick);
  const modeBtn = document.getElementById("btn-copilot-mode");
  assert.equal(modeBtn.textContent.trim(), "편집");
  userClick(modeBtn);
  assert.equal(modeBtn.textContent.trim(), "질문");

  const input = document.getElementById("copilot-input");
  input.value = "목표 대비 %를 추가하고 가장 큰 격차를 표시하세요.";
  input.dispatchEvent({ type: "input", target: input });
  userClick(document.getElementById("btn-copilot-send"));
  await flushAsyncTurns(15);

  const assistantMsg = document.querySelector("#copilot-thread .eo-copilot-msg--assistant");
  assert.ok(assistantMsg);
  assert.equal(assistantMsg.querySelector(".eo-copilot-inplace-row"), null, "ask mode must not show updatedInPlace");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="D1"]').textContent, "", "ask mode must not mutate cells");
  assert.equal(document.getElementById("smart-summary-output").textContent, "요약 대기 중", "smart-dialog output untouched");

  switchModule(document, "PDF");
  await flushAsyncTurns(6);
  assert.equal(modeBtn.textContent.trim(), "질문");
  userClick(modeBtn);
  assert.equal(modeBtn.textContent.trim(), "질문", "PDF locks mode to ask");
});

test("DOC 및 SLIDE 편집 모드는 인플레이스 문서·슬라이드 반영, 위치 칩 이동, 실행 취소 복원을 수행한다", async () => {
  const { document, userClick } = await createWorkspaceHarness({ search: "?module=DOC&win=w-copilot-doc-slide" });
  await flushAsyncTurns(12);

  const editor = document.getElementById("doc-editor");
  const p1 = editor.querySelector("p");
  p1.textContent = "초안 문단입니다. 담당자 연락처는 010-1234-5678 입니다.";
  editor.dispatchEvent({ type: "input", target: p1 });
  await flushAsyncTurns(4);

  userClick(document.getElementById("btn-copilot-toggle"));
  await selectLocalOllama(document, userClick);
  const input = document.getElementById("copilot-input");
  input.value = "개인정보를 마스킹하고 핵심 요약 문단을 추가하세요.";
  input.dispatchEvent({ type: "input", target: input });
  userClick(document.getElementById("btn-copilot-send"));
  await flushAsyncTurns(16);

  assert.ok(!editor.textContent.includes("010-1234-5678"), "PII phone number masked in place");
  assert.ok(editor.querySelectorAll("p, h2, ul, ol").length >= 2, "summary block appended");
  const docMsg = document.querySelector("#copilot-thread .eo-copilot-msg--assistant");
  assert.equal(docMsg.querySelector(".eo-copilot-inplace-row")?.textContent.trim(), "제자리에서 업데이트됨");
  const docChip = docMsg.querySelector(".eo-copilot-loc-chip");
  assert.ok(docChip);
  userClick(docChip);

  userClick(docMsg.querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(12);
  assert.ok(editor.textContent.includes("010-1234-5678"), "DOC undo restores original unmasked text");

  switchModule(document, "SLIDE");
  await flushAsyncTurns(6);
  userClick(document.getElementById("btn-copilot-new"));
  assert.equal(document.querySelectorAll("#slide-thumb-list .slide-thumb").length, 1);

  input.value = "발표 슬라이드를 추가하고 발표자 노트를 작성하세요.";
  input.dispatchEvent({ type: "input", target: input });
  userClick(document.getElementById("btn-copilot-send"));
  await flushAsyncTurns(16);

  const thumbs = document.querySelectorAll("#slide-thumb-list .slide-thumb");
  assert.equal(thumbs.length, 2, "new slide added in place");
  assert.equal(thumbs[1].getAttribute("aria-current"), "true", "focused onto newly added slide 2");
  assert.ok(document.getElementById("slide-title-zone").textContent.trim().length > 0);
  assert.ok(document.getElementById("slide-presenter-notes").value.trim().length > 0);

  const slideMsg = document.querySelector("#copilot-thread .eo-copilot-msg--assistant");
  const slideChip = slideMsg.querySelector(".eo-copilot-loc-chip");
  assert.ok(slideChip);
  thumbs[0].click();
  await flushAsyncTurns(3);
  assert.equal(document.querySelectorAll("#slide-thumb-list .slide-thumb")[0].getAttribute("aria-current"), "true");
  userClick(slideChip);
  await flushAsyncTurns(3);
  assert.equal(document.querySelectorAll("#slide-thumb-list .slide-thumb")[1].getAttribute("aria-current"), "true", "slide chip focuses slide 2");

  userClick(slideMsg.querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(12);
  assert.equal(document.querySelectorAll("#slide-thumb-list .slide-thumb").length, 1, "SLIDE undo restores 1-slide deck");
});

test("모델 설정 대화상자와 하단 모델 선택 메뉴는 회사별 모델 카탈로그·목록 불러오기·추론 수준을 지원하고 API 키를 클로저 메모리에만 유지한다", async () => {
  const localStore = createMemoryStorage();
  const sessionStore = createMemoryStorage();
  const seenRequests = [];
  const { document, sandbox, userClick } = await createWorkspaceHarness({
    search: "?module=DOC&win=w-copilot-model",
    localStorage: localStore,
    sessionStorage: sessionStore,
    testIpc: async (command, payload) => {
      if (command === "run_smart_assistant") {
        const parsed = JSON.parse(payload);
        seenRequests.push(parsed);
        if (parsed.action === "copilot_list_models" && parsed.discover && parsed.provider?.kind === "anthropic") {
          return {
            status: "ok",
            provider: "anthropic",
            live: true,
            models: [
              { id: "claude-sonnet-4-6", name: "Claude Sonnet 4.6", contextWindow: 200000, supportsReasoning: true },
              { id: "claude-opus-4-6", name: "Claude Opus 4.6", contextWindow: 200000, supportsReasoning: true },
            ],
          };
        }
      }
      const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
      const offline = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
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
  await flushAsyncTurns(12);

  userClick(document.getElementById("btn-copilot-toggle"));

  // 0. Zero hardcoded model arrays in catalog; only 3 reasoning effort tiers (낮음 / 보통 / 높음, no '없음'/'끄기')
  const catalog = sandbox.__EASYOFFICE_COPILOT_CATALOG__;
  for (const prov of catalog.PROVIDERS) {
    assert.equal(prov.models, undefined, `provider ${prov.id} must not hardcode static models`);
    assert.equal(prov.defaultModel, undefined, `provider ${prov.id} must not hardcode defaultModel`);
  }
  assert.deepEqual(
    catalog.REASONING_EFFORTS.map((e) => e.label),
    ["낮음", "보통", "높음"]
  );

  // 1. In-composer quick provider menu (lists providers before API key is registered, never hardcoded models)
  const modelBtn = document.getElementById("btn-copilot-model");
  const modelMenu = document.getElementById("copilot-model-menu");
  assert.equal(modelMenu.hidden, true);
  userClick(modelBtn);
  assert.equal(modelMenu.hidden, false);
  assert.equal(modelBtn.getAttribute("aria-expanded"), "true");
  const quickItems = modelMenu.querySelectorAll('button[role="menuitemradio"]');
  assert.ok(quickItems.length >= 6, "provider items rendered in composer menu");

  // Selecting local Ollama queries live /v1/models and switches immediately to the discovered model
  const ollamaQuick = Array.from(quickItems).find((b) => b.dataset.quickKind === "ollama");
  assert.ok(ollamaQuick);
  userClick(ollamaQuick);
  await flushAsyncTurns(6);
  assert.equal(modelMenu.hidden, true);
  assert.match(modelBtn.textContent, /qwen3:8b · 보통/);

  // Selecting Anthropic without a key opens model dialog pre-selected on Anthropic with empty model ID
  userClick(modelBtn);
  const anthropicQuick = Array.from(modelMenu.querySelectorAll('button[role="menuitemradio"]')).find(
    (b) => b.dataset.quickKind === "anthropic"
  );
  assert.ok(anthropicQuick);
  assert.ok(anthropicQuick.textContent.includes("키 필요"));
  userClick(anthropicQuick);

  const dlg = document.getElementById("copilot-model-dialog");
  assert.ok(dlg?.open, "model dialog opened for Anthropic key setup");

  const kindSelect = document.getElementById("copilot-provider-kind");
  const presetSelect = document.getElementById("copilot-provider-preset");
  const urlInput = document.getElementById("copilot-provider-url");
  const modelInput = document.getElementById("copilot-provider-model");
  const effortSelect = document.getElementById("copilot-reasoning-effort");
  const keyInput = document.getElementById("copilot-provider-key");
  assert.equal(kindSelect.value, "anthropic");
  assert.equal(urlInput.value, "https://api.anthropic.com/v1");
  assert.equal(modelInput.value, "", "no hardcoded default model before API key discovery");
  assert.equal(effortSelect.value, "medium", "default reasoning effort is medium (보통)");
  assert.equal(keyInput.getAttribute("type"), "password");

  const secretKey = "sk-ant-mem-only-secret-8472";
  keyInput.value = secretKey;

  // Simply clicking 확인 with the API key auto-fetches live /v1/models and picks the first available model!
  userClick(document.getElementById("btn-copilot-model-ok"));
  await flushAsyncTurns(8);
  assert.equal(dlg.open, false);
  assert.equal(seenRequests.some((r) => r.action === "copilot_list_models" && r.provider?.kind === "anthropic"), true);
  assert.match(modelBtn.textContent, /Claude Sonnet 4\.6 · 보통/);

  // Reopen Model Settings to test explicit connection check & preset switching
  userClick(document.getElementById("btn-copilot-settings"));
  presetSelect.value = "claude-opus-4-6";
  presetSelect.dispatchEvent({ type: "change", target: presetSelect });
  assert.equal(modelInput.value, "claude-opus-4-6");
  effortSelect.value = "high";
  effortSelect.dispatchEvent({ type: "change", target: effortSelect });

  userClick(document.getElementById("btn-copilot-test-provider"));
  await flushAsyncTurns(8);
  assert.equal(seenRequests.at(-1)?.action, "copilot_test_provider");
  assert.equal(seenRequests.at(-1)?.provider?.apiKey, secretKey);
  assert.match(document.getElementById("copilot-model-status").textContent, /확인|연결/);

  userClick(document.getElementById("btn-copilot-model-ok"));
  await flushAsyncTurns(6);
  assert.equal(dlg.open, false);
  assert.match(modelBtn.textContent, /Claude Opus 4\.6 · 높음/);

  // 2. Once the company API key is registered, opening the chat composer menu exposes ALL live-discovered models of that company + 3-tier reasoning effort (낮음/보통/높음) directly in chat!
  userClick(modelBtn);
  assert.equal(modelMenu.hidden, false);
  const anthropicGroup = modelMenu.querySelector('[role="group"][aria-label="Anthropic"]');
  assert.ok(anthropicGroup, "registered company group rendered at top of chat composer menu");
  const anthropicChoices = Array.from(anthropicGroup.querySelectorAll('button[role="menuitemradio"]'));
  assert.equal(anthropicChoices.length, 2, "all live-discovered Anthropic models selectable directly in chat");
  const sonnetInChat = anthropicChoices.find((b) => b.dataset.quickModel === "claude-sonnet-4-6");
  assert.ok(sonnetInChat);
  userClick(sonnetInChat);
  assert.equal(modelMenu.hidden, true);
  assert.match(modelBtn.textContent, /Claude Sonnet 4\.6 · 높음/);

  // Switch reasoning effort directly from the chat composer menu ("낮음")
  userClick(modelBtn);
  const effortGroup = modelMenu.querySelector('[role="group"][aria-label="추론 수준"]');
  assert.ok(effortGroup, "reasoning effort group rendered in chat composer menu");
  assert.ok(modelMenu.querySelector(".eo-copilot-menu-scroll"), "scrollable option body separated from pinned footer");
  const footer = modelMenu.querySelector(".eo-copilot-menu-footer");
  assert.ok(footer, "pinned footer container rendered");
  assert.equal(
    footer.querySelector("#btn-copilot-menu-manage .eo-copilot-menu-item-title"),
    null,
    "footer action items must use plain action label, not bold model title class"
  );
  assert.ok(footer.querySelector("#btn-copilot-menu-manage .eo-copilot-menu-action-label"));
  const effortButtons = Array.from(effortGroup.querySelectorAll('button[role="menuitemradio"]'));
  assert.deepEqual(
    effortButtons.map((b) => b.textContent.trim()),
    ["낮음", "보통", "높음"]
  );
  assert.match(COPILOT_CSS, /\.eo-copilot-welcome-title\s*\{[^}]*font-size:\s*var\(--font-size-xl\)/);
  assert.match(COPILOT_CSS, /\.eo-copilot-textarea\s*\{[^}]*font-size:\s*var\(--font-size-lg\)/);
  assert.match(COPILOT_CSS, /\.eo-copilot-reply\s*\{[^}]*font-size:\s*var\(--font-size-lg\)/);
  assert.match(COPILOT_CSS, /\.eo-copilot-menu-heading\s*\{[^}]*font-size:\s*var\(--font-size-sm\)[^}]*font-weight:\s*500/s);
  assert.match(COPILOT_CSS, /\.eo-copilot-menu-item-title\s*\{[^}]*font-size:\s*var\(--font-size-md\)[^}]*font-weight:\s*500/s);
  assert.match(COPILOT_CSS, /\.eo-copilot-menu-action-label\s*\{[^}]*font-size:\s*var\(--font-size-md\)[^}]*font-weight:\s*400/s);
  const lowEffortBtn = effortButtons.find((b) => b.dataset.effortChoice === "low");
  assert.ok(lowEffortBtn);
  userClick(lowEffortBtn);
  assert.equal(modelMenu.hidden, true);
  assert.match(modelBtn.textContent, /Claude Sonnet 4\.6 · 낮음/);

  assert.equal(keyInput.value, "", "password input cleared after closing dialog");
  assert.equal(keyInput.getAttribute("value"), null, "no value attribute on password input");
  assert.equal(document.documentElement.innerHTML.includes(secretKey), false, "secret absent from serialized DOM");
  assert.equal(JSON.stringify(localStore.dump?.() ?? {}).includes(secretKey), false, "secret absent from localStorage");
  assert.equal(JSON.stringify(sessionStore.dump?.() ?? {}).includes(secretKey), false, "secret absent from sessionStorage");

  const promptInput = document.getElementById("copilot-input");
  promptInput.value = "문서의 핵심 내용을 요약해 주세요.";
  promptInput.dispatchEvent({ type: "input", target: promptInput });
  userClick(document.getElementById("btn-copilot-send"));
  await flushAsyncTurns(12);
  const runReq = seenRequests.find((r) => r.action === "copilot_run");
  assert.ok(runReq);
  assert.equal(runReq.provider.kind, "anthropic");
  assert.equal(runReq.provider.model, "claude-sonnet-4-6");
  assert.equal(runReq.provider.reasoningEffort, "low");
  assert.equal(runReq.provider.apiKey, secretKey, "closure-held API key sent on copilot_run");
});

test("상한 초과 및 허용되지 않은 작업 페이로드는 Fail-Closed로 거부하고 문서를 변경하지 않는다", async () => {
  let stubResponse = null;
  const { document, sandbox, userClick } = await createWorkspaceHarness({
    search: "?module=SHEET&win=w-copilot-bounds",
    testIpc: async (command, payload) => {
      if (command === "run_smart_assistant" && stubResponse) {
        return stubResponse;
      }
      const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
      const offline = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
        getActiveModule: () => "SHEET",
        ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
        ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
        contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
        moduleSpecs: cat.MODULE_SPECS,
        localRibbonState: cat.createDefaultRibbonState(),
      });
      return offline[command](payload);
    },
  });
  await flushAsyncTurns(10);
  userClick(document.getElementById("btn-copilot-toggle"));
  await selectLocalOllama(document, userClick);
  const input = document.getElementById("copilot-input");

  const runPrompt = async (resp) => {
    stubResponse = resp;
    input.value = "악의적 페이로드 검증";
    input.dispatchEvent({ type: "input", target: input });
    userClick(document.getElementById("btn-copilot-send"));
    await flushAsyncTurns(12);
    return document.querySelectorAll("#copilot-thread .eo-copilot-msg--assistant").at(-1);
  };

  const tooManyOps = Array.from({ length: 65 }, (_, i) => ({
    op: "sheet.write",
    cells: [{ coord: "A" + (i + 1), raw: "1" }],
  }));
  const msg1 = await runPrompt({ status: "ok", mode: "edit", updatedInPlace: true, operations: tooManyOps });
  assert.equal(msg1.querySelector(".eo-copilot-inplace-row"), null);
  assert.ok(msg1.querySelector(".eo-copilot-reply--error"), "exceeding MAX_OPERATIONS fails closed");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="A1"]').textContent, "");

  const msg2 = await runPrompt({
    status: "ok",
    mode: "edit",
    updatedInPlace: true,
    operations: [{ op: "sheet.format", range: "A1:Z500", fill: "#ff0000" }],
  });
  assert.ok(msg2.querySelector(".eo-copilot-reply--error"), "exceeding MAX_RANGE_CELLS fails closed");
  assert.equal(document.querySelector('#pane-sheet td[data-cell="A1"]').style.getPropertyValue("--cell-fill"), "");

  const msg3 = await runPrompt({
    status: "ok",
    mode: "edit",
    updatedInPlace: true,
    operations: [{ op: "shell.exec", cmd: "id" }],
  });
  assert.ok(msg3.querySelector(".eo-copilot-reply--error"), "unknown operation kind fails closed");
});


test("엔드포인트 주소를 바꾸고 새 키를 넣지 않으면 저장된 키를 지우고 새 주소로 보내지 않는다 (PD-253)", async () => {
  const seenRequests = [];
  const { document, sandbox, userClick } = await createWorkspaceHarness({
    search: "?module=DOC&win=w-copilot-key-url",
    testIpc: async (command, payload) => {
      if (command === "run_smart_assistant") {
        const parsed = JSON.parse(payload);
        seenRequests.push(parsed);
        if (parsed.action === "copilot_list_models") {
          return { status: "ok", provider: "remote_https", live: true, models: [{ id: "m1", name: "m1", contextWindow: 8000, supportsReasoning: false }] };
        }
      }
      const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
      const offline = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
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
  await flushAsyncTurns(12);
  userClick(document.getElementById("btn-copilot-toggle"));

  const dlg = document.getElementById("copilot-model-dialog");
  const field = (id, value) => {
    const el = document.getElementById(id);
    el.value = value;
    el.dispatchEvent({ type: "change", target: el });
  };
  const configure = async (url, key) => {
    userClick(document.getElementById("btn-copilot-settings"));
    field("copilot-provider-kind", "remote_https");
    field("copilot-provider-url", url);
    field("copilot-provider-model", "m1");
    document.getElementById("copilot-provider-key").value = key;
    userClick(document.getElementById("btn-copilot-model-ok"));
    await flushAsyncTurns(8);
    assert.equal(dlg.open, false);
  };
  const send = async (text) => {
    const input = document.getElementById("copilot-input");
    input.value = text;
    input.dispatchEvent({ type: "input", target: input });
    userClick(document.getElementById("btn-copilot-send"));
    await flushAsyncTurns(12);
    return seenRequests.filter((r) => r.action === "copilot_run").at(-1);
  };

  await configure("https://a.example/v1", "sk-for-a-only-5521");
  const toA = await send("첫째 질문");
  assert.equal(toA.provider.baseUrl, "https://a.example/v1");
  assert.equal(toA.provider.apiKey, "sk-for-a-only-5521", "the key goes to the address it was entered for");

  await configure("https://b.example/v1", "");
  const toB = await send("둘째 질문");
  assert.equal(toB.provider.baseUrl, "https://b.example/v1");
  assert.equal(toB.provider.apiKey, "", "the saved key is not sent to another address");

  await configure("https://a.example/v1", "");
  const back = await send("셋째 질문");
  assert.equal(back.provider.apiKey, "", "the key was dropped when the address changed, not parked");
});

test("작업 목록 중간에서 실패하면 앞서 한 문서 변경을 되돌리고 되돌리기 단계를 남기지 않으며, 성공하면 한 단계로 되돌린다", async () => {
  let stubResponse = null;
  const { document, sandbox, userClick } = await createWorkspaceHarness({
    search: "?module=DOC&win=w-copilot-atomic",
    testIpc: async (command, payload) => {
      if (command === "run_smart_assistant" && stubResponse) return stubResponse;
      const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
      const offline = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
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
  await flushAsyncTurns(10);
  userClick(document.getElementById("btn-copilot-toggle"));
  await selectLocalOllama(document, userClick);
  const input = document.getElementById("copilot-input");
  const editor = document.getElementById("doc-editor");
  const runPrompt = async (operations) => {
    stubResponse = { status: "ok", mode: "edit", updatedInPlace: true, operations };
    input.value = "문단을 추가해 주세요";
    input.dispatchEvent({ type: "input", target: input });
    userClick(document.getElementById("btn-copilot-send"));
    await flushAsyncTurns(12);
    return document.querySelectorAll("#copilot-thread .eo-copilot-msg--assistant").at(-1);
  };
  const before = editor.textContent;

  const failed = await runPrompt([
    { op: "doc.append", text: "앞서 추가한 문단" },
    { op: "shell.exec", cmd: "id" },
  ]);
  assert.ok(failed.querySelector(".eo-copilot-reply--error"), "a failing operation reports an error");
  assert.equal(failed.querySelector(".eo-copilot-undo-btn"), null, "nothing was applied, so there is nothing to undo");
  assert.equal(editor.textContent, before, "the paragraph of the earlier operation is gone");

  const ok = await runPrompt([
    { op: "doc.append", text: "첫 문단" },
    { op: "doc.append", text: "둘째 문단" },
  ]);
  assert.ok(ok.querySelector(".eo-copilot-undo-btn"));
  assert.ok(editor.textContent.includes("첫 문단") && editor.textContent.includes("둘째 문단"));
  userClick(ok.querySelector(".eo-copilot-undo-btn"));
  await flushAsyncTurns(12);
  assert.equal(editor.textContent, before, "one undo takes both operations back");
});
