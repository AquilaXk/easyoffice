// 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd.) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)

(function (host) {
  "use strict";

  const SVG_NS = "http://www.w3.org/2000/svg";
  const SUPPORTED_MODULES = new Set(["DOC", "HWP", "SHEET", "SLIDE", "BASE", "BOARD", "PDF"]);
  // Only PDF is ask-only (the engine strips write operations there).
  const ASK_ONLY_MODULES = new Set(["PDF"]);
  const MIN_WIDTH = 280;
  const MAX_WIDTH = 520;
  const DEFAULT_WIDTH = 320;
  const MAX_MESSAGES = 64;
  const MAX_HISTORY_ITEMS = 12;
  // 엔진이 받는 질문의 상한(UTF-8 바이트). 글자 수가 아니라 바이트로 맞춘다(`copilot-text.js` `clipUtf8Bytes`).
  const MAX_PROMPT_BYTES = 16384;

  // The selection commands (the engine writes the instruction for each id, `copilot/prompt.rs`).
  const SELECTION_COMMANDS = Object.freeze([
    { id: "rewrite", label: "다시 쓰기" },
    { id: "summarize", label: "요약" },
    { id: "shorten", label: "더 짧게" },
    { id: "lengthen", label: "더 길게" },
    { id: "fix_spelling", label: "맞춤법 수정" },
    { id: "translate", label: "번역" },
    { id: "visualize_table", label: "표로 시각화", docOnly: true },
  ]);
  const COMMAND_MODULES = new Set(["DOC", "HWP", "SLIDE"]);

  const MODULE_EXAMPLES = Object.freeze({
    SHEET: Object.freeze([
      { label: "목표 대비 % 및 최대 격차 표시", prompt: "목표 대비 %를 추가하고 가장 큰 격차를 표시하세요." },
      { label: "합계 및 평균 추가", prompt: "합계 행을 추가하세요." },
    ]),
    DOC: Object.freeze([
      { label: "문서 핵심 요약", prompt: "문서의 핵심 내용을 요약해 주세요." },
      { label: "피동 표현 다듬기", prompt: "피동 표현을 자연스러운 능동 문장으로 다듬어 주세요." },
    ]),
    HWP: Object.freeze([
      { label: "한글 문서 문장 다듬기", prompt: "문서의 어색한 표현을 자연스러운 문장으로 다듬어 주세요." },
      { label: "문서 핵심 요약 추가", prompt: "문서 끝에 핵심 요약을 추가해 주세요." },
    ]),
    SLIDE: Object.freeze([
      { label: "발표 슬라이드 추가", prompt: "발표 슬라이드를 추가하고 발표자 노트를 작성하세요." },
      { label: "발표자 노트 작성", prompt: "현재 슬라이드의 발표자 노트를 작성해 주세요." },
    ]),
    PDF: Object.freeze([
      { label: "문서 핵심 요약", prompt: "문서의 핵심 내용을 쪽 번호와 함께 요약해 주세요." },
      { label: "주요 항목 정리", prompt: "문서의 주요 항목과 수치를 정리해 주세요." },
    ]),
    BASE: Object.freeze([
      { label: "샘플 레코드 추가", prompt: "현재 테이블 구조에 맞는 예시 레코드 3건을 추가해 주세요." },
      { label: "테이블 데이터 요약", prompt: "현재 테이블 레코드와 집계 수치를 요약해 주세요." },
    ]),
    BOARD: Object.freeze([
      { label: "업무 흐름 다이어그램 생성", prompt: "요청 접수 → 검토 → 승인 → 완료 순서의 흐름도를 추가해 주세요." },
      { label: "보드 흐름 요약", prompt: "현재 보드의 연결 흐름과 핵심 요소를 설명해 주세요." },
    ]),
  });

  function clampWidth(v) {
    const raw = Math.round(Number(v) || DEFAULT_WIDTH);
    const vw = Number(host.innerWidth) || 0;
    const maxAllowed = vw >= 800 ? Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, vw - 520)) : MAX_WIDTH;
    return Math.min(maxAllowed, Math.max(MIN_WIDTH, raw));
  }

  function createSvgIcon(document, paths, size = 14) {
    const svg = document.createElementNS(SVG_NS, "svg");
    svg.setAttribute("width", String(size));
    svg.setAttribute("height", String(size));
    svg.setAttribute("viewBox", "0 0 16 16");
    svg.setAttribute("fill", "none");
    svg.setAttribute("aria-hidden", "true");
    for (const spec of paths) {
      const p = document.createElementNS(SVG_NS, spec.tag || "path");
      for (const [k, v] of Object.entries(spec)) {
        if (k !== "tag") p.setAttribute(k, String(v));
      }
      svg.appendChild(p);
    }
    return svg;
  }

  function buildCheckIcon(document) {
    return createSvgIcon(document, [
      { tag: "circle", cx: "8", cy: "8", r: "6.5", stroke: "currentColor", "stroke-width": "1.4" },
      { d: "M5.2 8.2L7.1 10.1L10.9 6.1", stroke: "currentColor", "stroke-width": "1.5", "stroke-linecap": "round", "stroke-linejoin": "round" },
    ], 14);
  }

  function buildLocationIcon(document, mod) {
    if (mod === "SHEET") {
      return createSvgIcon(document, [
        { tag: "rect", x: "2", y: "2.5", width: "12", height: "11", rx: "1.5", stroke: "currentColor", "stroke-width": "1.3" },
        { d: "M2 6.5H14M6.5 2.5V13.5", stroke: "currentColor", "stroke-width": "1.2" },
      ], 12);
    }
    if (mod === "SLIDE") {
      return createSvgIcon(document, [
        { tag: "rect", x: "1.8", y: "3", width: "12.4", height: "9.5", rx: "1.5", stroke: "currentColor", "stroke-width": "1.3" },
        { d: "M5 6.5H11M5 9.2H8.8", stroke: "currentColor", "stroke-width": "1.2", "stroke-linecap": "round" },
      ], 12);
    }
    return createSvgIcon(document, [
      { tag: "rect", x: "3", y: "2", width: "10", height: "12", rx: "1.5", stroke: "currentColor", "stroke-width": "1.3" },
      { d: "M5.2 5.5H10.8M5.2 8H10.8M5.2 10.5H8.5", stroke: "currentColor", "stroke-width": "1.2", "stroke-linecap": "round" },
    ], 12);
  }

  function buildExampleArrowIcon(document) {
    const svg = createSvgIcon(document, [
      { d: "M5 11L11 5M6.2 5H11V9.8", stroke: "currentColor", "stroke-width": "1.4", "stroke-linecap": "round", "stroke-linejoin": "round" },
    ], 13);
    svg.setAttribute("class", "eo-copilot-example-arrow");
    return svg;
  }

  function buildSendGlyph(document, running) {
    if (running) {
      return createSvgIcon(document, [
        { tag: "rect", x: "4.5", y: "4.5", width: "7", height: "7", rx: "1.2", fill: "currentColor" },
      ], 14);
    }
    return createSvgIcon(document, [
      { d: "M8 12.5V3.8M4.5 7.2L8 3.6L11.5 7.2", stroke: "currentColor", "stroke-width": "1.7", "stroke-linecap": "round", "stroke-linejoin": "round" },
    ], 14);
  }

  function normalizeLocations(rawList, fallbackMod) {
    if (!Array.isArray(rawList)) return [];
    const seen = new Set();
    const out = [];
    for (const item of rawList) {
      const loc = typeof item === "string"
        ? { module: fallbackMod, label: item, range: fallbackMod === "SHEET" ? item : undefined }
        : item;
      if (!loc || typeof loc !== "object") continue;
      const mod = String(loc.module || fallbackMod).toUpperCase();
      const label = String(loc.label || loc.range || "").trim();
      if (!label) continue;
      const key = mod + ":" + (loc.sheet || "") + ":" + (loc.page || "") + ":" + label;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push({
        module: mod,
        label,
        sheet: loc.sheet ? String(loc.sheet) : undefined,
        range: loc.range ? String(loc.range) : undefined,
        paragraphIndex: Number.isInteger(loc.paragraphIndex) ? loc.paragraphIndex : undefined,
        slideIndex: Number.isInteger(loc.slideIndex) ? loc.slideIndex : undefined,
        page: Number.isInteger(loc.page) ? loc.page : undefined,
      });
    }
    return out;
  }

  function createCopilotUi(deps) {
    const {
      document,
      invokeIpc,
      canStreamIpc,
      getActiveModule,
      isImeComposing,
      setStatusFeedback,
    } = deps;
    const copilotActions = deps.copilotActions || host.__EASYOFFICE_CREATE_COPILOT_ACTIONS__(deps);

    const byId = (id) => document.getElementById(id);
    let isOpen = false;
    let panelWidth = DEFAULT_WIDTH;
    let preferredMode = "edit";
    let running = false;
    let activeRunToken = 0;
    let activeRequestId = "";
    let regenerating = false;
    const streamApi = host.__EASYOFFICE_COPILOT_STREAM__;
    let turnCounter = 0;
    let dragState = null;
    const messages = [];

    const modelUi = host.__EASYOFFICE_CREATE_COPILOT_MODEL_UI__({
      document,
      invokeIpc,
      isRunning: () => running,
      onChanged: () => syncComposerControls(),
    });
    const agentLoop = host.__EASYOFFICE_CREATE_COPILOT_AGENT_LOOP__({
      invokeIpc,
      canStreamIpc,
      streamApi,
      copilotActions,
    });
    const copilotGhost = host.__EASYOFFICE_CREATE_COPILOT_GHOST__?.({
      document,
      invokeIpc,
      getActiveModule,
      isModelConfigured: () => modelUi.isConfigured(),
      getProviderPayload: () => modelUi.currentProviderPayload(),
      isPanelBusy: () => running || regenerating,
      isImeComposing,
      captureWorkspaceSnapshot: deps.captureWorkspaceSnapshot,
      commitDeferredUndoSnapshot: deps.commitDeferredUndoSnapshot,
      noteActivity: deps.noteActivity,
      getWorkspacePanels: deps.getWorkspacePanels,
    }) || null;

    function renderReplyContent(replyEl, text, isError) {
      const md = host.__EASYOFFICE_COPILOT_MARKDOWN__;
      if (!md?.renderMarkdownInto) {
        replyEl.textContent = text;
        return;
      }
      md.renderMarkdownInto(document, replyEl, text, {
        plainTextOnly: Boolean(isError),
        onPageJump: (page) => {
          void copilotActions.locateTarget({ module: "PDF", page, label: page + "쪽" }).then(() => syncSelectionBadge());
        },
      });
    }

    function isStartWindow() {
      const mod = getActiveModule();
      return mod === "START" || byId("app")?.dataset.windowKind === "start";
    }

    function isModuleSupported(mod = getActiveModule()) {
      return !isStartWindow() && SUPPORTED_MODULES.has(mod);
    }

    function effectiveMode(mod = getActiveModule()) {
      return ASK_ONLY_MODULES.has(mod) ? "ask" : preferredMode;
    }

    function applyWidthStyle(width) {
      const shell = byId("eo-copilot-shell");
      const resizer = byId("copilot-resizer");
      panelWidth = clampWidth(width);
      shell?.style.setProperty("--copilot-width", panelWidth + "px");
      if (resizer) {
        resizer.setAttribute("aria-valuenow", String(panelWidth));
        resizer.setAttribute("aria-valuetext", panelWidth + "px");
      }
    }

    function syncSelectionBadge() {
      const selRow = byId("copilot-selection-row");
      const selText = byId("copilot-selection-label");
      if (selRow && selText) {
        if (!isOpen || !isModuleSupported()) {
          selRow.hidden = true;
        } else {
          const label = copilotActions.getSelectionLabel(getActiveModule());
          selRow.hidden = !label;
          if (label) selText.textContent = label;
        }
      }
      syncCommandRow();
      syncMessageActions();
    }

    // The selection commands show while a text selection in the document (or slide title or body) can take an edit.
    function syncCommandRow() {
      const row = byId("copilot-command-row");
      if (!row) return;
      const mod = getActiveModule();
      row.hidden = !(
        isOpen && isModuleSupported() && !running && !regenerating && COMMAND_MODULES.has(mod) &&
        effectiveMode(mod) === "edit" && copilotActions.getCommandTarget(mod)
      );
      if (!row.hidden) {
        for (const chip of row.querySelectorAll("[data-doc-only]")) {
          chip.hidden = mod !== "DOC";
        }
      }
    }

    function buildCommandRow() {
      const row = byId("copilot-command-row");
      if (!row) return;
      const chips = SELECTION_COMMANDS.map(({ id, label, docOnly }) => {
        const chip = document.createElement("button");
        chip.type = "button";
        chip.className = "eo-copilot-chip-btn";
        if (docOnly) chip.dataset.docOnly = "true";
        chip.textContent = label;
        // The selection stays in the document while the chip is pressed.
        chip.addEventListener("mousedown", (ev) => ev.preventDefault());
        chip.addEventListener("click", () => {
          const mod = getActiveModule();
          const target = copilotActions.getCommandTarget(mod);
          if (target) void runTurn({ prompt: label, command: id, target, mode: "edit", mod });
        });
        return chip;
      });
      row.replaceChildren(...chips);
    }

    // Undo and regenerate follow the document: a user edit after the answer turns them off without a redraw.
    function actionDisabled(action, msg) {
      if (!msg) return true;
      if (action === "accept" || action === "reject") return running || regenerating;
      const undoGone = Boolean(msg.applied) && !msg.undone && !copilotActions.canUndoTurn(msg.id);
      return action === "undo" ? undoGone : undoGone || running || regenerating;
    }

    function syncMessageActions() {
      const threadEl = byId("copilot-thread");
      if (!threadEl) return;
      for (const className of ["eo-copilot-undo-btn", "eo-copilot-chip-btn"]) {
        for (const btn of threadEl.querySelectorAll("." + className)) {
          const msg = messages.find((m) => m.id === btn.dataset.turn);
          btn.disabled = actionDisabled(btn.dataset.copilotAction, msg);
        }
      }
    }

    function syncComposerControls() {
      const mod = getActiveModule();
      const modeBtn = byId("btn-copilot-mode");
      const sendBtn = byId("btn-copilot-send");
      const inputEl = byId("copilot-input");
      const curMode = effectiveMode(mod);

      if (modeBtn) {
        modeBtn.textContent = curMode === "edit" ? "편집" : "질문";
        modeBtn.dataset.mode = curMode;
        modeBtn.disabled = running || ASK_ONLY_MODULES.has(mod);
        modeBtn.setAttribute("title", curMode === "edit" ? "현재 문서를 직접 편집" : "읽기 전용 질문");
      }
      modelUi.syncModelButton();
      const ghostBtn = byId("btn-copilot-ghost");
      if (ghostBtn) ghostBtn.hidden = mod !== "DOC";
      if (sendBtn) {
        const hasDraft = Boolean(String(inputEl?.value || "").trim());
        sendBtn.disabled = !running && !hasDraft;
        const sendLabel = running ? "중지" : "보내기";
        sendBtn.setAttribute("aria-label", sendLabel);
        sendBtn.setAttribute("title", sendLabel);
        sendBtn.replaceChildren(buildSendGlyph(document, running));
      }
      syncSelectionBadge();
    }

    function renderWelcome() {
      const welcome = byId("copilot-welcome");
      const examplesWrap = byId("copilot-examples");
      const inputEl = byId("copilot-input");
      if (!welcome || !examplesWrap) return;
      const mod = getActiveModule();
      const hasDraft = Boolean(String(inputEl?.value || "").trim());
      const show = messages.length === 0 && !hasDraft && isModuleSupported(mod);
      welcome.hidden = !show;
      if (!show) return;
      const items = MODULE_EXAMPLES[mod] || MODULE_EXAMPLES.DOC;
      const nodes = items.map((ex) => {
        const btn = document.createElement("button");
        btn.type = "button";
        btn.className = "eo-copilot-example-btn";
        const span = document.createElement("span");
        span.textContent = ex.label;
        btn.appendChild(span);
        btn.appendChild(buildExampleArrowIcon(document));
        btn.addEventListener("click", () => {
          if (!inputEl) return;
          preferredMode = ASK_ONLY_MODULES.has(mod) ? "ask" : "edit";
          inputEl.value = ex.prompt;
          autoGrowTextarea(inputEl);
          renderWelcome();
          syncComposerControls();
          inputEl.focus();
        });
        return btn;
      });
      examplesWrap.replaceChildren(...nodes);
    }

    function renderAssistantNode(msg) {
      const wrap = document.createElement("div");
      wrap.className = "eo-copilot-msg eo-copilot-msg--assistant";

      if (msg.reasoning) {
        const reasonEl = document.createElement("div");
        reasonEl.className = "eo-copilot-reasoning";
        reasonEl.textContent = msg.reasoning;
        wrap.appendChild(reasonEl);
      }

      if (msg.updatedInPlace && !msg.undone) {
        const statusRow = document.createElement("div");
        statusRow.className = "eo-copilot-inplace-row";
        statusRow.appendChild(buildCheckIcon(document));
        const label = document.createElement("span");
        label.textContent = "제자리에서 업데이트됨";
        statusRow.appendChild(label);
        wrap.appendChild(statusRow);
      }

      const locs = Array.isArray(msg.locations) ? msg.locations : [];
      const proposing = msg.change?.state === "pending";
      const showUndo = !msg.undone && copilotActions.hasUndoEntry(msg.id);
      const showRegenerate = !msg.pending && Boolean(msg.request) && messages.at(-1) === msg;
      if (locs.length > 0 || showUndo || showRegenerate || proposing) {
        const chipRow = document.createElement("div");
        chipRow.className = "eo-copilot-chip-row";
        if (proposing) {
          for (const [action, label, handler] of [["accept", "수락", acceptChange], ["reject", "거절", rejectChange]]) {
            const btn = document.createElement("button");
            btn.type = "button";
            btn.className = "eo-copilot-chip-btn eo-copilot-review-btn";
            btn.dataset.turn = msg.id;
            btn.dataset.copilotAction = action;
            btn.disabled = actionDisabled(action, msg);
            btn.textContent = label;
            btn.addEventListener("click", () => void handler(msg));
            chipRow.appendChild(btn);
          }
        }
        for (const loc of locs) {
          const chip = document.createElement("button");
          chip.type = "button";
          chip.className = "eo-copilot-loc-chip";
          chip.appendChild(buildLocationIcon(document, loc.module));
          const txt = document.createElement("span");
          txt.textContent = loc.label;
          chip.appendChild(txt);
          chip.addEventListener("click", () => {
            void copilotActions.locateTarget(loc).then(() => syncSelectionBadge());
          });
          chipRow.appendChild(chip);
        }
        if (showUndo) {
          const undoBtn = document.createElement("button");
          undoBtn.type = "button";
          undoBtn.className = "eo-copilot-undo-btn";
          undoBtn.dataset.turn = msg.id;
          undoBtn.dataset.copilotAction = "undo";
          undoBtn.disabled = actionDisabled("undo", msg);
          undoBtn.textContent = "실행 취소";
          undoBtn.addEventListener("click", () => {
            void copilotActions.undoLastTurn(msg.id).then((restored) => {
              if (!restored) return;
              msg.undone = true;
              renderThread();
              setStatusFeedback?.("코파일럿 편집을 실행 취소했습니다");
            });
          });
          chipRow.appendChild(undoBtn);
        }
        if (showRegenerate) {
          const regenBtn = document.createElement("button");
          regenBtn.type = "button";
          regenBtn.className = "eo-copilot-chip-btn";
          regenBtn.dataset.turn = msg.id;
          regenBtn.dataset.copilotAction = "regenerate";
          regenBtn.disabled = actionDisabled("regenerate", msg);
          regenBtn.textContent = "다시 생성";
          regenBtn.addEventListener("click", () => void regenerateLast());
          chipRow.appendChild(regenBtn);
        }
        wrap.appendChild(chipRow);
      }

      if (proposing) {
        wrap.appendChild(copilotActions.renderChangeList(msg.change.changes, (loc) => void copilotActions.locateTarget(loc).then(() => syncSelectionBadge())));
      } else if (msg.change?.state === "rejected") {
        const note = document.createElement("div");
        note.className = "eo-copilot-inplace-row";
        note.textContent = "거절함";
        wrap.appendChild(note);
      }

      if (msg.reply) {
        const replyEl = document.createElement("div");
        replyEl.className = "eo-copilot-reply" + (msg.error ? " eo-copilot-reply--error" : "");
        replyEl.dataset.turnReply = msg.id;
        renderReplyContent(replyEl, msg.reply, msg.error);
        wrap.appendChild(replyEl);
      }
      if (msg.pending) {
        const loading = document.createElement("div");
        loading.className = "eo-copilot-loading";
        loading.textContent = "응답 생성 중…";
        wrap.appendChild(loading);
      }
      return wrap;
    }

    function renderThread() {
      const threadEl = byId("copilot-thread");
      if (!threadEl) return;
      const nodes = messages.map((msg) => {
        if (msg.role === "user") {
          const card = document.createElement("div");
          card.className = "eo-copilot-msg eo-copilot-msg--user";
          const body = document.createElement("div");
          body.className = "eo-copilot-msg-text";
          body.textContent = msg.text;
          card.appendChild(body);
          return card;
        }
        return renderAssistantNode(msg);
      });
      threadEl.replaceChildren(...nodes);
      renderWelcome();
      const bodyEl = byId("copilot-body");
      if (bodyEl) bodyEl.scrollTop = bodyEl.scrollHeight || 0;
    }

    function syncPanelChrome() {
      const shell = byId("eo-copilot-shell");
      const panel = byId("copilot-panel");
      const topBtn = byId("btn-copilot-toggle");
      const supported = isModuleSupported();
      const visible = supported && isOpen;
      if (!visible) modelUi.closeMenu(false);
      if (shell) shell.hidden = !visible;
      if (panel) panel.hidden = !visible;
      if (topBtn) {
        topBtn.hidden = !supported;
        topBtn.setAttribute("aria-pressed", visible ? "true" : "false");
      }
      applyWidthStyle(panelWidth);
      renderWelcome();
      syncComposerControls();
    }

    function refreshHwpBadgeAsync() {
      if (getActiveModule() === "HWP") {
        void copilotActions.refreshHwpSnapshot?.().then(() => syncSelectionBadge()).catch(() => {});
      }
    }

    function setOpen(nextOpen) {
      if (!isModuleSupported()) return false;
      isOpen = Boolean(nextOpen);
      if (isOpen) refreshHwpBadgeAsync();
      syncPanelChrome();
      if (isOpen) byId("copilot-input")?.focus?.();
      return isOpen;
    }

    function toggle() {
      if (!isModuleSupported()) return false;
      return setOpen(!isOpen);
    }

    function autoGrowTextarea(textarea) {
      if (!textarea) return;
      const lines = String(textarea.value || "").split("\n").length;
      const rows = Math.min(6, Math.max(2, lines));
      textarea.setAttribute("rows", String(rows));
    }

    function buildHistoryPayload() {
      const completed = messages.filter((m) => !m.pending && !m.error);
      return completed.slice(-MAX_HISTORY_ITEMS).map((m) => {
        const text = m.role === "user" ? String(m.text || "") : String(m.reply || "");
        return { role: m.role, text, content: text };
      });
    }

    function newRequestId() {
      return "cp-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 8) + "-" + turnCounter;
    }

    // Puts the question and a pending answer in the thread and starts the run. `null` when it cannot start (a run is
    // going, or no model is set up: then the settings open and the draft stays).
    function startTurn(request, { clearInput = false } = {}) {
      if (running || regenerating || !isModuleSupported()) return null;
      modelUi.closeMenu(false);
      if (!modelUi.isConfigured()) {
        modelUi.openDialog(undefined, undefined, "전송하기 전에 API 키를 입력하거나 로컬 모델을 선택해 주세요.");
        return null;
      }
      dismissPreview();
      const mod = request.mod || getActiveModule();
      const mode = request.mode || effectiveMode(mod);
      const history = buildHistoryPayload();
      const frozenSelection = mod === "SHEET" ? copilotActions.freezeSheetSelection() : null;
      const { selection, context } = copilotActions.extractContext(mod, { target: request.target, frozenSelection });

      if (clearInput) {
        const inputEl = byId("copilot-input");
        if (inputEl) {
          inputEl.value = "";
          autoGrowTextarea(inputEl);
        }
      }
      if (messages.length >= MAX_MESSAGES) messages.splice(0, messages.length - MAX_MESSAGES + 2);
      turnCounter += 1;
      const turnId = "turn-" + turnCounter;
      messages.push({ id: turnId + "-u", role: "user", text: request.prompt });
      const assistantEntry = {
        id: turnId,
        role: "assistant",
        pending: true,
        reply: "",
        reasoning: "",
        updatedInPlace: false,
        applied: false,
        locations: [],
        undone: false,
        error: false,
        request: { prompt: request.prompt, command: request.command || "", target: request.target || null, targetSnapshot: copilotActions.snapshotTarget(mod, request.target), mode, mod },
      };
      messages.push(assistantEntry);

      running = true;
      activeRunToken += 1;
      activeRequestId = newRequestId();
      renderThread();
      syncComposerControls();
      return { assistantEntry, turnId, mod, mode, selection, frozenSelection, context, history, token: activeRunToken, requestId: activeRequestId, request };
    }

    // 스트림으로 온 글이 늘 때마다 대기 중인 답글을 그 자리에서 고쳐 쓴다(조각마다 스레드를 다시 그리지 않는다).
    function showStreamed(entry, text, token) {
      if (token !== activeRunToken) return;
      entry.reply = text;
      const node = byId("copilot-thread")?.querySelector(`[data-turn-reply="${entry.id}"]`);
      if (node) {
        renderReplyContent(node, text, entry.error);
        const bodyEl = byId("copilot-body");
        if (bodyEl) bodyEl.scrollTop = bodyEl.scrollHeight || 0;
      } else {
        renderThread();
      }
    }

    // 연산이 있는 편집 응답은 적용하지 않고 바뀔 곳을 보여 준다. 수락해야 적용한다.
    function propose(entry, mod, rawOperations, frozenSelection) {
      const operations = copilotActions.normalizeOperations(mod, rawOperations, frozenSelection);
      const changes = copilotActions.buildChanges(mod, operations);
      entry.change = { state: "pending", mod, operations, changes, fingerprint: copilotActions.fingerprint(mod, operations) };
      if (mod === "DOC") copilotActions.showDocPreview(changes);
    }

    function dismissPreview() {
      for (const msg of messages) {
        if (msg.change?.state === "pending") msg.change.state = "rejected";
      }
      copilotActions.clearPreview();
    }

    async function finishTurn(turn) {
      const { assistantEntry, mod, mode, token } = turn;
      try {
        let selection = turn.selection;
        let context = turn.context;
        if (mod === "HWP" && copilotActions.extractContextAsync) {
          const fresh = await copilotActions.extractContextAsync(mod, { target: turn.request.target });
          if (token !== activeRunToken) return;
          selection = fresh.selection;
          context = fresh.context;
        }
        const baseReq = {
          action: "copilot_run",
          requestId: turn.requestId,
          mode,
          module: mod,
          prompt: turn.request.prompt,
          history: turn.history,
          selection,
          context,
          provider: modelUi.currentProviderPayload(),
        };
        if (turn.request.command) baseReq.command = turn.request.command;
        const res = await agentLoop.runAgentTurn({
          baseReq,
          frozenSelection: turn.frozenSelection,
          newSubRequestId: (stepIdx) => turn.requestId + "-s" + stepIdx,
          isCancelled: () => token !== activeRunToken,
          onStreamText: (text) => showStreamed(assistantEntry, text, token),
          onActiveRequestId: (id) => { activeRequestId = id; },
        });
        if (!res || token !== activeRunToken) return;
        const ops = Array.isArray(res?.operations) ? res.operations : [];
        const explicitLocs = normalizeLocations(res?.locations, mod);
        assistantEntry.pending = false;
        assistantEntry.reply = String(res?.reply || "요청을 처리했습니다.");
        assistantEntry.reasoning = String(res?.reasoning || "");
        assistantEntry.locations = explicitLocs;
        if (mode === "edit" && ops.length > 0) propose(assistantEntry, mod, ops, turn.frozenSelection);
        const dropped = Number(res?.droppedOperations) || 0;
        if (dropped > 0) setStatusFeedback?.("형식이 맞지 않는 작업 " + dropped + "개를 건너뛰었습니다");
      } catch (err) {
        if (token !== activeRunToken) return;
        assistantEntry.pending = false;
        assistantEntry.error = true;
        assistantEntry.reply = String(err?.message || "코파일럿 요청 처리에 실패했습니다.");
      } finally {
        if (token === activeRunToken) {
          running = false;
          renderThread();
          syncComposerControls();
        }
      }
    }

    // 수락: 문서가 미리보기를 본 때와 같으면 포인터·타이핑 연출과 함께 적용한다(되돌리기는 한 단계).
    async function acceptChange(msg) {
      const change = msg?.change;
      if (!change || change.state !== "pending" || running || regenerating) return;
      copilotActions.clearPreview();
      const freshOk = copilotActions.verifyFingerprintAsync
        ? await copilotActions.verifyFingerprintAsync(change.mod, change.operations, change.fingerprint)
        : copilotActions.fingerprint(change.mod, change.operations) === change.fingerprint;
      if (!freshOk) {
        change.state = "stale";
        setStatusFeedback?.("문서가 바뀌어 적용할 수 없습니다");
        renderThread();
        return;
      }
      change.state = "accepted";
      running = true;
      renderThread();
      syncComposerControls();
      try {
        const appliedRes = await copilotActions.applyOperationsAnimated(change.mod, "edit", change.operations, msg.id);
        msg.applied = appliedRes.applied;
        msg.updatedInPlace = appliedRes.applied;
        if (msg.locations.length === 0) msg.locations = appliedRes.locations;
        if (appliedRes.interrupted) setStatusFeedback?.("코파일럿 편집을 중지했습니다");
      } catch (err) {
        msg.error = true;
        msg.reply = String(err?.message || "코파일럿 편집을 적용하지 못했습니다.");
      } finally {
        running = false;
        renderThread();
        syncComposerControls();
      }
    }

    function rejectChange(msg) {
      if (msg?.change?.state !== "pending" || running || regenerating) return;
      msg.change.state = "rejected";
      copilotActions.clearPreview();
      renderThread();
    }

    function runTurn(request, options) {
      const turn = startTurn(request, options);
      return turn ? finishTurn(turn) : undefined;
    }

    function submitPrompt() {
      const inputEl = byId("copilot-input");
      const prompt = host.__EASYOFFICE_COPILOT_TEXT__.clipUtf8Bytes(String(inputEl?.value || "").trim(), MAX_PROMPT_BYTES);
      if (!prompt) return undefined;
      return runTurn({ prompt }, { clearInput: true });
    }

    // The last answer again, for the same request. An edit it made is taken back first (the undo is gone once the
    // user edited, and so is this).
    async function regenerateLast() {
      if (running || regenerating) return;
      const last = messages.at(-1);
      const asked = messages.at(-2);
      if (last?.role !== "assistant" || last.pending || !last.request || asked?.role !== "user") return;
      if (!modelUi.isConfigured()) {
        modelUi.openDialog(undefined, undefined, "전송하기 전에 API 키를 입력하거나 로컬 모델을 선택해 주세요.");
        return;
      }
      regenerating = true;
      syncMessageActions();
      try {
        if (last.change?.state === "pending") {
          last.change.state = "rejected";
          copilotActions.clearPreview();
        }
        if (last.applied && !last.undone) {
          if (!(await copilotActions.undoLastTurn(last.id))) {
            renderThread();
            return;
          }
          last.undone = true;
        }
        // A command is asked again for the blocks or slide field it was asked for. When those changed (an edit moved the
        // index, or the text is not what the answer was written for), nothing is sent.
        if (last.request.target && copilotActions.snapshotTarget(last.request.mod, last.request.target) !== last.request.targetSnapshot) {
          setStatusFeedback?.("선택한 내용이 바뀌어 다시 생성할 수 없습니다");
          renderThread();
          return;
        }
        messages.splice(-2, 2);
      } finally {
        regenerating = false;
      }
      await runTurn(last.request);
    }

    function stopActiveRun() {
      if (!running) return;
      // 연출 중이면 쓰던 부분까지 남기고 멈춘다(요청은 이미 끝났다).
      if (copilotActions.isPerforming()) {
        copilotActions.stopPerformance();
        return;
      }
      activeRunToken += 1;
      running = false;
      // The engine closes the model connection; the answer that may still arrive is dropped by the token.
      void Promise.resolve(invokeIpc("run_smart_assistant", JSON.stringify({ action: "copilot_cancel", requestId: activeRequestId }))).catch(() => {});
      const last = messages.at(-1);
      if (last?.pending) {
        last.pending = false;
        last.reply = "요청을 중지했습니다.";
      }
      renderThread();
      syncComposerControls();
    }

    function bindResizer() {
      const resizer = byId("copilot-resizer");
      const shell = byId("eo-copilot-shell");
      if (!resizer || !shell) return;

      const finishDrag = (cancel) => {
        if (!dragState) return;
        const prev = dragState;
        dragState = null;
        delete shell.dataset.resizing;
        if (cancel) applyWidthStyle(prev.startWidth);
        if (resizer.hasPointerCapture?.(prev.pointerId)) {
          resizer.releasePointerCapture?.(prev.pointerId);
        }
      };

      resizer.addEventListener("pointerdown", (ev) => {
        if (ev.isPrimary === false || (ev.button ?? 0) !== 0) return;
        ev.preventDefault();
        resizer.focus?.();
        const pointerId = ev.pointerId ?? 1;
        resizer.setPointerCapture?.(pointerId);
        dragState = { pointerId, startX: ev.clientX ?? 0, startWidth: panelWidth };
        shell.dataset.resizing = "true";
      });
      resizer.addEventListener("pointermove", (ev) => {
        if (!dragState || (ev.pointerId ?? 1) !== dragState.pointerId) return;
        const delta = dragState.startX - (ev.clientX ?? 0);
        applyWidthStyle(dragState.startWidth + delta);
      });
      resizer.addEventListener("pointerup", () => finishDrag(false));
      resizer.addEventListener("pointercancel", () => finishDrag(true));
      resizer.addEventListener("lostpointercapture", () => finishDrag(false));
      resizer.addEventListener("dblclick", () => applyWidthStyle(DEFAULT_WIDTH));
      resizer.addEventListener("keydown", (ev) => {
        if (ev.key === "Escape" && dragState) {
          ev.preventDefault();
          finishDrag(true);
          return;
        }
        const step = ev.shiftKey ? 40 : 20;
        const map = {
          ArrowLeft: panelWidth + step,
          ArrowRight: panelWidth - step,
          Home: MIN_WIDTH,
          End: MAX_WIDTH,
          Enter: DEFAULT_WIDTH,
        };
        if (Object.hasOwn(map, ev.key)) {
          ev.preventDefault();
          applyWidthStyle(map[ev.key]);
        }
      });
    }

    function bind() {
      modelUi.bind();
      byId("btn-copilot-toggle")?.addEventListener("click", () => toggle());
      byId("btn-copilot-close")?.addEventListener("click", () => setOpen(false));
      byId("btn-copilot-new")?.addEventListener("click", () => {
        // 연출 중이면 쓰던 부분도 적용하지 않고 버린다.
        copilotActions.abortPerformance();
        stopActiveRun();
        dismissPreview();
        messages.length = 0;
        copilotActions.clearUndo();
        const inputEl = byId("copilot-input");
        if (inputEl) {
          inputEl.value = "";
          autoGrowTextarea(inputEl);
        }
        renderThread();
        syncComposerControls();
      });
      byId("btn-copilot-mode")?.addEventListener("click", () => {
        if (running || ASK_ONLY_MODULES.has(getActiveModule())) return;
        preferredMode = preferredMode === "edit" ? "ask" : "edit";
        syncComposerControls();
      });
      byId("btn-copilot-send")?.addEventListener("click", () => {
        if (running) stopActiveRun();
        else void submitPrompt();
      });
      buildCommandRow();

      const inputEl = byId("copilot-input");
      let inputComposing = false;
      let lastCompEndAt = -Infinity;
      inputEl?.addEventListener("compositionstart", () => { inputComposing = true; });
      inputEl?.addEventListener("compositionend", (ev) => {
        inputComposing = false;
        const ts = Number(ev?.timeStamp);
        if (Number.isFinite(ts)) lastCompEndAt = ts;
      });
      inputEl?.addEventListener("input", () => {
        // 질문은 UTF-8 바이트 상한까지만 받는다(예전 `maxlength`는 글자 수라 한글이 엔진 상한을 넘었다).
        const clipped = host.__EASYOFFICE_COPILOT_TEXT__.clipUtf8Bytes(inputEl.value, MAX_PROMPT_BYTES);
        if (clipped !== inputEl.value) inputEl.value = clipped;
        autoGrowTextarea(inputEl);
        renderWelcome();
        syncComposerControls();
      });
      inputEl?.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter" && !ev.shiftKey) {
          const ts = Number(ev.timeStamp);
          const withinPostComp = Number.isFinite(ts) && ts - lastCompEndAt >= 0 && ts - lastCompEndAt < 50;
          if (ev.isComposing || ev.keyCode === 229 || inputComposing || withinPostComp || isImeComposing?.()) return;
          ev.preventDefault();
          void submitPrompt();
        }
      });

      const shellEl = byId("eo-copilot-shell");
      for (const evt of ["pointerenter", "focusin"]) {
        shellEl?.addEventListener(evt, () => refreshHwpBadgeAsync());
      }
      for (const type of ["focusin", "keyup", "input", "click"]) {
        document.addEventListener(type, () => syncSelectionBadge());
      }
      host.addEventListener?.("resize", () => {
        applyWidthStyle(panelWidth);
        modelUi.repositionMenu();
      });
      bindResizer();
      copilotGhost?.bind?.();
      const ghostBtn = byId("btn-copilot-ghost");
      if (ghostBtn && copilotGhost) {
        const syncGhostBtn = () => {
          const on = copilotGhost.isEnabled();
          ghostBtn.setAttribute("aria-pressed", on ? "true" : "false");
          ghostBtn.classList.toggle("active", on);
        };
        syncGhostBtn();
        ghostBtn.addEventListener("click", () => {
          copilotGhost.setEnabled(!copilotGhost.isEnabled());
          syncGhostBtn();
        });
      }
      syncPanelChrome();
    }

    return {
      bind,
      toggle,
      setOpen,
      onModuleChanged: () => {
        dismissPreview();
        copilotGhost?.dismiss?.();
        if (getActiveModule() === "HWP") void copilotActions.refreshHwpSnapshot?.().then(() => syncSelectionBadge()).catch(() => {});
        syncPanelChrome();
      },
      syncSelection: () => syncSelectionBadge(),
      isPerforming: () => copilotActions.isPerforming(),
      whenIdle: () => copilotActions.whenIdle(),
      ghost: copilotGhost,
    };
  }

  host.__EASYOFFICE_CREATE_COPILOT_UI__ = createCopilotUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
