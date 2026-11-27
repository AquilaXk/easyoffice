import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { acceptProposal } from "./test-support/copilot-accept.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { open } from "./test-support/copilot-open.mjs";

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));

test("PD-369 Phase 1: session anchor keeps <=25% delta, re-anchors >25% or new chat/PDF change, and isolates selection commands", async () => {
  const docH = await open({
    module: "DOC",
    run: (req) => {
      if (req.prompt.includes("재시도 유발")) {
        return {
          status: "ok",
          mode: "edit",
          reply: "실패 테스트",
          operations: [],
          droppedOperations: 1,
          droppedReasons: ['op[0] doc.replace_span: 대상 문단(0)에서 "존재하지않는구문"을 찾지 못했습니다.'],
        };
      }
      return { status: "ok", mode: req.mode, reply: "확인", operations: [], locations: [] };
    },
  });

  const paras = Array.from({ length: 10 }, (_, i) => `<p>기본 문단 ${i}의 내용입니다.</p>`).join("");
  docH.editor.innerHTML = paras;

  // Turn 1: establishes anchor
  await docH.send("첫 질문입니다", 16);
  const req1 = docH.seen.filter((r) => r.action === "copilot_run")[0];
  const anchorKey1 = req1.promptCacheKey;
  assert.ok(anchorKey1 && anchorKey1.startsWith("s-"));
  assert.equal(req1.contextDelta, undefined);

  // Small edit (<25%: change 1 paragraph of 10): Turn 2 sends anchor context + contextDelta with SAME promptCacheKey
  docH.editor.querySelectorAll("p")[1].textContent = "수정 문단 1입니다.";
  await docH.send("둘째 질문입니다", 16);
  const runsAfterSmall = docH.seen.filter((r) => r.action === "copilot_run");
  const req2 = runsAfterSmall[1];
  assert.equal(req2.promptCacheKey, anchorKey1, "small delta <=25% preserves session anchor & promptCacheKey");
  assert.equal(req2.context.doc.paragraphs[1], "기본 문단 1의 내용입니다.", "anchor Tier 2 snapshot kept stable");
  assert.ok(req2.contextDelta?.summary?.includes("수정 문단 1"), "contextDelta carries latest change");

  // Selection command must isolate history: [] even with prior chat history
  const p0 = docH.editor.querySelectorAll("p")[0];
  p0.textContent = "오타 있는 문단";
  docH.host.getSelection = () => ({
    rangeCount: 1,
    isCollapsed: false,
    toString: () => "오타",
    getRangeAt: () => ({
      collapsed: false,
      startContainer: p0,
      startOffset: 0,
      endContainer: p0,
      endOffset: 1,
      commonAncestorContainer: p0,
      toString: () => "오타",
    }),
  });
  docH.document.dispatchEvent({ type: "keyup" });
  await flushAsyncTurns(3);
  const cmdRow = docH.document.getElementById("copilot-command-row");
  const fixChip = Array.from(cmdRow.querySelectorAll("button"))
    .find((b) => b.textContent.trim() === "맞춤법 수정");
  assert.ok(fixChip && !cmdRow.hidden, "selection command chip visible when text selected");
  docH.userClick(fixChip);
  await flushAsyncTurns(14);
  const lastCmd = docH.seen.filter((x) => x.action === "copilot_run" && x.command === "fix_spelling").at(-1);
  assert.ok(lastCmd, "command request dispatched");
  assert.deepEqual(lastCmd.history, [], "selection command isolates prior conversation history");

  // Large edit (>25%): re-anchors Tier 2 snapshot & clears delta while preserving same-doc session key until 새 대화/문서 전환
  const allP = docH.editor.querySelectorAll("p");
  for (let i = 0; i < 6; i += 1) allP[i].textContent = `대폭 수정된 새 문단 ${i} 내용입니다.`;
  await docH.send("세번째 질문입니다", 16);
  const reqLarge = docH.seen.filter((x) => x.action === "copilot_run" && x.prompt.includes("세번째")).at(-1);
  assert.equal(reqLarge.promptCacheKey, anchorKey1, ">25% same-doc re-anchor keeps session cache key while updating baseline");
  assert.equal(reqLarge.context.doc.paragraphs[0], "대폭 수정된 새 문단 0 내용입니다.", ">25% re-anchors Tier 2 baseline");
  assert.equal(reqLarge.contextDelta, undefined, "re-anchored turn sends fresh baseline without delta");

  // 새 대화 button rotates promptCacheKey and clears session anchor
  const beforeNewKey = reqLarge.promptCacheKey;
  docH.userClick(docH.document.getElementById("btn-copilot-new"));
  await flushAsyncTurns(6);
  await docH.send("새 대화 첫 질문", 16);
  const reqNew = docH.seen.filter((x) => x.action === "copilot_run").at(-1);
  assert.notEqual(reqNew.promptCacheKey, beforeNewKey, "새 대화 rotates promptCacheKey");
  assert.deepEqual(reqNew.history, []);

  // Verify session anchor PDF page-0 signature differentiation (two PDFs with equal pageCount re-anchor),
  // large PDF scroll without getPdfPages keeps anchor, SHEET out-of-window selection supplements Tier 4,
  // SLIDE navigation preserves anchorIndex while sending live activeIndex, and oversized doc delta triggers re-anchor.
  const anchorEnv = {};
  loadUiModule(UI("copilot-session-anchor.js"), anchorEnv);
  const sa = anchorEnv.__EASYOFFICE_COPILOT_SESSION_ANCHOR__.createCopilotSessionAnchor();
  const pdfA = sa.resolveTurnPayload({ mod: "PDF", command: "", liveContext: { pdf: { text: "[1쪽] 문서 A 매출", pageCount: 3, activePage: 1 } } });
  const pdfB = sa.resolveTurnPayload({ mod: "PDF", command: "", liveContext: { pdf: { text: "[1쪽] 문서 B 계약", pageCount: 3, activePage: 1 } } });
  assert.notEqual(pdfA.promptCacheKey, pdfB.promptCacheKey, "different PDF with same pageCount rotates anchor");

  const saPdfBig = anchorEnv.__EASYOFFICE_COPILOT_SESSION_ANCHOR__.createCopilotSessionAnchor();
  const bigP1 = saPdfBig.resolveTurnPayload({ mod: "PDF", command: "", liveContext: { pdf: { text: "[35쪽] 35페이지 본문", pageCount: 50, activePage: 45 } } });
  const bigP2 = saPdfBig.resolveTurnPayload({ mod: "PDF", command: "", liveContext: { pdf: { text: "[1쪽] 1페이지 본문", pageCount: 50, activePage: 1 } } });
  assert.equal(bigP2.promptCacheKey, bigP1.promptCacheKey, "large PDF without getPdfPages keeps session on scroll");
  assert.equal(bigP2.context.pdf.anchorPage, 45);
  assert.equal(bigP2.context.pdf.activePage, 1);
  assert.match(bigP2.contextDelta?.summary || "", /\[1쪽\] 1페이지 본문/);

  const saSheet = anchorEnv.__EASYOFFICE_COPILOT_SESSION_ANCHOR__.createCopilotSessionAnchor();
  const s1Cells = Array.from({ length: 300 }, (_, i) => ({ coord: `A${i + 1}`, raw: String(i + 1), value: String(i + 1) }));
  saSheet.resolveTurnPayload({ mod: "SHEET", command: "", liveContext: { sheet: { name: "S1", tabs: ["S1", "S2"], cells: s1Cells, totalCells: 500 } }, liveSelection: { range: "A1" } });
  const s2Cells = [{ coord: "Z250", raw: "999", value: "999" }, ...s1Cells.slice(0, 299)];
  const s2Res = saSheet.resolveTurnPayload({ mod: "SHEET", command: "", liveContext: { sheet: { name: "S1", tabs: ["S1", "S2"], cells: s2Cells, totalCells: 500 } }, liveSelection: { range: "Z250" } });
  assert.equal(s2Res.reanchored, false, "out-of-window sheet selection keeps anchor pinned");
  assert.ok(s2Res.contextDelta?.summary?.includes("Z250: 999"), "out-of-window selected cell supplemented via contextDelta");
  const sTabRes = saSheet.resolveTurnPayload({ mod: "SHEET", command: "", liveContext: { sheet: { name: "S1", tabs: ["S1", "S2", "S3"], cells: s1Cells, totalCells: 500 } }, liveSelection: { range: "A1" } });
  assert.equal(sTabRes.reanchored, true, "sheet.tabs change re-anchors");

  const saSlide = anchorEnv.__EASYOFFICE_COPILOT_SESSION_ANCHOR__.createCopilotSessionAnchor();
  const sl1 = saSlide.resolveTurnPayload({ mod: "SLIDE", command: "", liveContext: { slide: { activeIndex: 0, slides: [{ title: "S0" }, { title: "S1" }, { title: "S2", notes: "메모2" }] } } });
  const sl2 = saSlide.resolveTurnPayload({ mod: "SLIDE", command: "", liveContext: { slide: { activeIndex: 2, slides: [{ title: "S0" }, { title: "S1" }, { title: "S2", notes: "메모2" }] } } });
  assert.equal(sl1.context.slide.anchorIndex, 0);
  assert.equal(sl2.context.slide.anchorIndex, 0, "SLIDE anchorIndex pinned across slide navigation");
  assert.equal(sl2.context.slide.activeIndex, 2, "SLIDE activeIndex reflects live slide");

  // Large doc with >8 KiB of updatedParagraphs text (<25% of 60 KiB doc) must re-anchor rather than exceeding MAX_DELTA_TEXT_BYTES
  const saDocBig = anchorEnv.__EASYOFFICE_COPILOT_SESSION_ANCHOR__.createCopilotSessionAnchor();
  const bigParas = Array.from({ length: 60 }, (_, i) => `P${i}_` + "A".repeat(1000));
  saDocBig.resolveTurnPayload({ mod: "DOC", command: "", liveContext: { doc: { paragraphs: bigParas, omittedParagraphs: 0 } } });
  const editedParas = bigParas.map((p, i) => (i < 10 ? `EDIT${i}_` + "B".repeat(900) : p));
  const bigDeltaRes = saDocBig.resolveTurnPayload({ mod: "DOC", command: "", liveContext: { doc: { paragraphs: editedParas, omittedParagraphs: 0 } } });
  assert.equal(bigDeltaRes.reanchored, true, "delta exceeding MAX_DELTA_BYTES re-anchors instead of failing server budget");
  assert.equal(bigDeltaRes.contextDelta, null);
});

test("PD-369 Phase 1 & 2: droppedReasons loop retry, HWP pre-validation, dense sheet TSV, usage rollup, and 75% preemption", async () => {
  const seenTurns = [];
  const h = await open({
    module: "SHEET",
    run: (req) => {
      seenTurns.push(req);
      // 1. Trigger droppedReasons loop retry once, then succeed with tool call
      if (req.prompt.includes("드롭 검증") && (!req.loopSteps || req.loopSteps.length === 0)) {
        return {
          status: "ok",
          mode: "edit",
          reply: "1차 시도",
          operations: [],
          droppedOperations: 1,
          droppedReasons: ['op[0] sheet.write: 좌표 "ZZ99" 형식이 잘못되었습니다.'],
          usage: { inputTokens: 1000, outputTokens: 20, cachedTokens: 0, cacheWriteTokens: 0, reasoningTokens: 0 },
        };
      }
      if (req.prompt.includes("드롭 검증") && req.loopSteps?.some((s) => s.text.includes("엔진 연산 검증 거부"))) {
        return {
          status: "ok",
          mode: "edit",
          reply: "수정 완료",
          operations: [{
            kind: "sheet.write",
            cells: [
              { coord: "A1", raw: "항목" },
              { coord: "B1", raw: "금액" },
              { coord: "A2", raw: "사과" },
              { coord: "B2", raw: "100" },
            ],
          }],
          usage: { inputTokens: 1200, outputTokens: 30, cachedTokens: 900, cacheWriteTokens: 0, reasoningTokens: 5 },
        };
      }
      // 2. Test read_sheet_range TSV compression & tool step summary
      if (req.prompt.includes("범위 조회") && (!req.loopSteps || req.loopSteps.length === 0)) {
        return {
          status: "ok",
          mode: "edit",
          reply: "",
          toolCalls: [{ name: "read_sheet_range", range: "A1:B3" }],
          operations: [],
          usage: { inputTokens: 800, outputTokens: 15, cachedTokens: 0, cacheWriteTokens: 200, reasoningTokens: 0 },
        };
      }
      return {
        status: "ok",
        mode: "edit",
        reply: "조회 확인",
        operations: [],
        usage: { inputTokens: 900, outputTokens: 25, cachedTokens: 700, cacheWriteTokens: 0, reasoningTokens: 0 },
      };
    },
  });

  await h.send("드롭 검증 테스트", 20);
  const retryReq = seenTurns.find((r) => r.loopSteps?.some((s) => s.text.includes("ZZ99")));
  assert.ok(retryReq, "droppedReasons triggers loop retry carrying exact error diagnosis");
  assert.ok(await acceptProposal(h.document, h.userClick), "accepted A1:B2 cells into Sheet1");

  seenTurns.length = 0;
  await h.send("범위 조회 테스트", 20);
  const toolFollowup = seenTurns.find((r) => r.loopSteps?.some((s) => s.text.includes("도구 실행 결과")));
  assert.ok(toolFollowup, "read_sheet_range sub-turn executed");
  assert.match(toolFollowup.loopSteps[0].text, /^\[조회 호출\] read_sheet_range\(range=A1:B3\)/);
  assert.match(toolFollowup.loopSteps[1].text, /\t/, "read_sheet_range serialized as TSV");

  // Direct agentLoop test for multi-turn usage rollup + HWP pre-validation gate on cachedHwpSnapshot
  const loopEnv = {};
  loadUiModule(UI("copilot-text.js"), loopEnv);
  loadUiModule(UI("copilot-context-compact.js"), loopEnv);
  loadUiModule(UI("copilot-doc-html.js"), loopEnv);
  loadUiModule(UI("copilot-agent-loop.js"), loopEnv);
  let subTurn = 0;
  const loopReqs = [];
  const loop = loopEnv.__EASYOFFICE_CREATE_COPILOT_AGENT_LOOP__({
    invokeIpc: async (_cmd, payload) => {
      const req = JSON.parse(payload);
      loopReqs.push(req);
      subTurn += 1;
      if (subTurn === 1) {
        return {
          status: "ok",
          mode: "edit",
          reply: "HWP 수정",
          operations: [{ kind: "doc.replace_span", index: 0, search: "없는단어", replace: "새단어" }],
          usage: { inputTokens: 1500, outputTokens: 20, cachedTokens: 800, cacheWriteTokens: 100, reasoningTokens: 4 },
        };
      }
      return {
        status: "ok",
        mode: "edit",
        reply: "수정 완료",
        operations: [{ kind: "doc.replace_span", index: 0, search: "원본문구", replace: "새단어" }],
        usage: { inputTokens: 1700, outputTokens: 30, cachedTokens: 1100, cacheWriteTokens: 0, reasoningTokens: 6 },
      };
    },
    canStreamIpc: () => false,
    copilotActions: {
      normalizeOperations: (_mod, ops) => ops,
      dryRunVerifyOperations: async (mod, ops, ipc) => loopEnv.__EASYOFFICE_COPILOT_VERIFY_OPS__(mod, ops, {
        getHwpSnapshot: () => ({
          totalParagraphs: 1,
          byIndex: new Map([[0, { text: "원본문구입니다.", hasControls: false }]]),
        }),
        invokeIpc: ipc,
      }),
    },
  });
  const loopRes = await loop.runAgentTurn({
    baseReq: { action: "copilot_run", requestId: "r-hwp-gate", mode: "edit", module: "HWP", prompt: "수정", history: [], selection: {}, context: { doc: { paragraphs: ["원본문구입니다."] } }, provider: { kind: "ollama", model: "qwen3" } },
    newSubRequestId: (n) => `r-hwp-gate-${n}`,
    isCancelled: () => false,
  });
  assert.equal(loopReqs.length, 2, "HWP pre-validation gate caught nonexistent search on cachedHwpSnapshot and retried");
  assert.ok(loopReqs[1].loopSteps[0].text.includes("없는단어"), "validation failure preserved search term");
  assert.deepEqual(loopRes.usage, { inputTokens: 3200, outputTokens: 50, cachedTokens: 1900, cacheWriteTokens: 100, reasoningTokens: 10 });
  assert.equal(loopRes.lastContextTokens, 1730, "lastContextTokens reflects last sub-turn window occupancy, not summed sub-turns");

  // Preemptive 75% window eviction test on usageTracker: preserves UI messages and persists lowered limit
  const anchorEnv = {};
  loadUiModule(UI("copilot-session-anchor.js"), anchorEnv);
  const tracker = anchorEnv.__EASYOFFICE_COPILOT_SESSION_ANCHOR__.createUsageTracker();
  tracker.record(loopRes.usage, loopRes.lastContextTokens);
  assert.equal(tracker.preemptForWindow(6, 64000, 16), 16, "sub-turn sum 3250 doesn't confuse 1730 window check");
  tracker.record({ inputTokens: 98000, outputTokens: 4000, cachedTokens: 0, cacheWriteTokens: 0, reasoningTokens: 0 }, 51000);
  const msgs = Array.from({ length: 20 }, (_, i) => ({ role: i % 2 === 0 ? "user" : "assistant", text: `m${i}` }));
  const nextLimit = tracker.preemptForWindow(msgs, 64000, 16);
  assert.equal(msgs.length, 20, "UI chat scrollback preserved when preempting LLM history");
  assert.equal(nextLimit, 8, "historyLimit lowered by 8-item chunk");
  tracker.record({ inputTokens: 20000, outputTokens: 500 }, 20500);
  assert.equal(tracker.preemptForWindow(msgs, 64000, 16), 8, "lowered historyLimit persists so evicted history does not re-enter cache");
});

test("PD-369 Phase 3: fast model routing with 1x primary fallback, ghost prefix reuse & 64-LRU, and BASE 50-cap + read_base_records", async () => {
  // 1. Model UI fast sibling mapping (verified only when discovered; keeps primary when unqueried)
  const docEnv = await open({
    module: "DOC",
    run: (req) => ({ status: "ok", mode: req.mode, reply: " 세계로 나아갑니다.", operations: [] }),
  });

  // 2. Direct Ghost unit harness for fast routing + 1x primary fallback, prefix reuse (0 IPC), and 64-LRU eviction
  const env = {};
  loadUiModule(UI("copilot-text.js"), env);
  loadUiModule(UI("copilot-doc-html.js"), env);
  loadUiModule(UI("copilot-catalog.js"), env);
  loadUiModule(UI("copilot-model-ui.js"), env);
  loadUiModule(UI("copilot-ghost.js"), env);

  const ipcCalls = [];
  let failFastOnce = false;
  const fakeDoc = {
    body: {},
    createElement: (tag) => ({
      tagName: tag.toUpperCase(),
      className: "",
      dataset: {},
      style: { setProperty: () => {} },
      setAttribute: () => {},
      appendChild: () => {},
      remove: () => {},
      getBoundingClientRect: () => ({ left: 0, top: 0, width: 100, height: 20 }),
    }),
    createTextNode: (t) => ({ nodeType: 3, data: t, textContent: t }),
    createRange: null,
    getElementById: (id) => (id === "doc-editor" ? fakeEditor : null),
    querySelector: () => null,
  };
  let curBlock = {
    nodeType: 1,
    tagName: "P",
    parentElement: null,
    textContent: "안녕하세요",
    childNodes: [],
    lastChild: null,
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 100, height: 20 }),
    appendChild: (n) => curBlock.childNodes.push(n),
  };
  let onInput;
  const fakeEditor = {
    dataset: {},
    parentElement: { getBoundingClientRect: () => ({ left: 0, top: 0, width: 500, height: 500 }), appendChild: () => {} },
    contains: (n) => n === curBlock,
    children: [curBlock],
    addEventListener: (ev, fn) => { if (ev === "input") onInput = fn; },
  };
  curBlock.parentElement = fakeEditor;

  const win = {
    setTimeout: (fn) => { fn(); return 1; },
    clearTimeout: () => {},
    getSelection: () => ({
      rangeCount: 1,
      isCollapsed: true,
      getRangeAt: () => ({ startContainer: curBlock, endContainer: curBlock, endOffset: 0 }),
    }),
    getComputedStyle: () => ({ fontFamily: "sans-serif", fontSize: "14px", lineHeight: "20px" }),
  };

  let useFastModel = true;
  let fastShouldFail = true;
  const ghost2 = env.__EASYOFFICE_CREATE_COPILOT_GHOST__({
    document: fakeDoc,
    win,
    invokeIpc: async (cmd, payload) => {
      const req = JSON.parse(payload);
      ipcCalls.push(req);
      if (fastShouldFail && req.provider?.model === "claude-haiku-4-5") {
        throw new Error("fast model down");
      }
      return { status: "ok", reply: " 세계로 나아갑니다." };
    },
    getActiveModule: () => "DOC",
    isModelConfigured: () => true,
    getProviderPayload: () => ({ kind: "anthropic", baseUrl: "https://api.anthropic.com/v1", model: "claude-sonnet-4", apiKey: "sk-1", reasoningEffort: "high" }),
    getFastProviderPayload: () => ({
      kind: "anthropic",
      baseUrl: "https://api.anthropic.com/v1",
      model: useFastModel ? "claude-haiku-4-5" : "claude-sonnet-4",
      apiKey: "sk-1",
      reasoningEffort: "low",
    }),
    isPanelBusy: () => false,
    isImeComposing: () => false,
  });
  ghost2.setEnabled(true);
  ghost2.bind();
  assert.ok(ghost2.getPromptCacheKey().startsWith("ghost-"), "pinned ghost promptCacheKey");
  curBlock.textContent = "새로운 미래";
  onInput();
  await flushAsyncTurns(6);
  // First attempted claude-haiku-4-5 (threw) then fell back 1x to claude-sonnet-4!
  assert.equal(ipcCalls.length, 2);
  assert.equal(ipcCalls[0].provider.model, "claude-haiku-4-5");
  assert.equal(ipcCalls[1].provider.model, "claude-sonnet-4");
  assert.equal(ghost2.getSuggestionText(), "세계로 나아갑니다.");

  // Prefix typing ("세") advances active suggestion in place with ZERO IPC calls
  const countBeforeTyping = ipcCalls.length;
  curBlock.textContent = "새로운 미래세";
  onInput();
  await flushAsyncTurns(4);
  assert.equal(ipcCalls.length, countBeforeTyping, "matching typed prefix consumes ZERO IPC tokens");
  assert.equal(ghost2.getSuggestionText(), "계로 나아갑니다.");

  // 64-entry LRU bound + cache hit (0 IPC)
  fastShouldFail = false;
  for (let i = 0; i < 70; i += 1) {
    curBlock.textContent = `고유 접두 문구 번호 ${i}`;
    onInput();
    await flushAsyncTurns(4);
  }
  assert.equal(ghost2.getCacheSize(), 64, "LRU cache bounded to 64 entries");
  const beforeHitCalls = ipcCalls.length;
  curBlock.textContent = "고유 접두 문구 번호 69";
  onInput();
  await flushAsyncTurns(4);
  assert.equal(ipcCalls.length, beforeHitCalls, "LRU cache hit consumes 0 IPC calls");

  // 3. Verify catalog.findFastSiblingModel maps only when live discovered models present
  const disc = new Map([["anthropic", [{ id: "claude-sonnet-4" }, { id: "claude-haiku-4-5" }]]]);
  assert.equal(env.__EASYOFFICE_COPILOT_CATALOG__.findFastSiblingModel("anthropic", "claude-sonnet-4", disc), "claude-haiku-4-5");
  assert.equal(env.__EASYOFFICE_COPILOT_CATALOG__.findFastSiblingModel("anthropic", "claude-sonnet-4", new Map()), null, "unqueried provider stays on primary model");

  // 4. Verify BASE 50-record initial cap & on-demand read_base_records(offset, limit, query)
  const baseH = await createWorkspaceHarness({ search: "?module=BASE&win=w-base-pd369" });
  await flushAsyncTurns(10);
  const baseApi = baseH.host.__EASYOFFICE_BASE_BOARD_HOST__;
  const baseModel = baseH.host.__EASYOFFICE_BASE_MODEL__;
  const bs = baseApi.getBaseStudio();
  const st = JSON.parse(JSON.stringify(bs.getState()));
  const tbl = baseModel.getTable(st, st.activeTableId);
  const f0 = tbl.fields[0].id;
  for (let i = tbl.records.length; i < 75; i += 1) {
    tbl.records.push({ id: `r-${i}`, values: { [f0]: i === 68 ? "특수타겟항목" : `고객_${i}` } });
  }
  bs.setState(st);
  await flushAsyncTurns(4);
  const extCtx = baseApi.extractCopilotContext("BASE");
  assert.equal(extCtx.context.base.records.length, 50, "initial BASE context capped to 50 records");
  assert.equal(extCtx.context.base.totalRecords, 75, "totalRecords still reflects 75");
  const toolRes = baseApi.readBaseRecordsTool({ offset: 0, limit: 10, query: "특수타겟" });
  assert.ok(toolRes.includes("특수타겟항목") && toolRes.includes("일치 1건"), `read_base_records query match: ${toolRes}`);

  // 5. Verify fast sibling routing + 1x primary fallback on fix_spelling via live UI model discovery
  const cmdModelsTried = [];
  const uiH = await createWorkspaceHarness({
    search: "?module=DOC&win=w-fast-pd369",
    testIpc: async (cmd, payload) => {
      if (cmd === "run_smart_assistant") {
        const parsed = JSON.parse(payload);
        if (parsed.action === "copilot_list_models") {
          return {
            status: "ok",
            models: [
              { id: "claude-sonnet-4", name: "Claude Sonnet 4", contextWindow: 200000, supportsReasoning: true },
              { id: "claude-haiku-4-5", name: "Claude Haiku 4.5", contextWindow: 200000, supportsReasoning: true },
            ],
          };
        }
        if (parsed.action === "copilot_run") {
          cmdModelsTried.push(parsed.provider?.model);
          if (parsed.provider?.model === "claude-haiku-4-5") {
            throw new Error("fast sibling temporary failure");
          }
          return {
            status: "ok",
            mode: "edit",
            reply: "맞춤법을 고쳤습니다.",
            operations: [{ kind: "doc.replace_paragraphs", replacements: [{ index: 0, text: "교정 문장" }] }],
          };
        }
      }
      const cat = uiH.sandbox.__EASYOFFICE_RIBBON_CATALOG__;
      const offline = uiH.sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
        getActiveModule: () => "DOC",
        ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
        ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
        contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
        moduleSpecs: cat.MODULE_SPECS,
        localRibbonState: cat.createDefaultRibbonState(),
      });
      return offline[cmd](payload);
    },
  });
  await flushAsyncTurns(12);
  uiH.userClick(uiH.document.getElementById("btn-copilot-toggle"));
  uiH.userClick(uiH.document.getElementById("btn-copilot-model"));
  const manageBtn = Array.from(uiH.document.getElementById("copilot-model-menu").querySelectorAll("button"))
    .find((b) => b.textContent.includes("모델 설정"));
  uiH.userClick(manageBtn);
  const kindSel = uiH.document.getElementById("copilot-provider-kind");
  kindSel.value = "anthropic";
  kindSel.dispatchEvent({ type: "change", target: kindSel });
  uiH.document.getElementById("copilot-provider-key").value = "sk-ant-test";
  uiH.document.getElementById("copilot-provider-model").value = "claude-sonnet-4";
  uiH.userClick(uiH.document.getElementById("btn-copilot-model-ok"));
  await flushAsyncTurns(8);

  const uP0 = uiH.document.getElementById("doc-editor").querySelectorAll("p")[0];
  uP0.textContent = "틀린 문장";
  uiH.host.getSelection = () => ({
    rangeCount: 1,
    isCollapsed: false,
    toString: () => "틀린",
    getRangeAt: () => ({
      collapsed: false,
      startContainer: uP0,
      startOffset: 0,
      endContainer: uP0,
      endOffset: 1,
      commonAncestorContainer: uP0,
      toString: () => "틀린",
    }),
  });
  uiH.document.dispatchEvent({ type: "keyup" });
  await flushAsyncTurns(3);
  const uFix = Array.from(uiH.document.getElementById("copilot-command-row").querySelectorAll("button"))
    .find((b) => b.textContent.trim() === "맞춤법 수정");
  uiH.userClick(uFix);
  await flushAsyncTurns(16);
  assert.deepEqual(cmdModelsTried, ["claude-haiku-4-5", "claude-sonnet-4"], "fix_spelling tried fast sibling then fell back 1x to primary");
  assert.match(uiH.document.getElementById("copilot-thread").textContent, /맞춤법을 고쳤습니다/);
  void docEnv;
  void failFastOnce;
});
