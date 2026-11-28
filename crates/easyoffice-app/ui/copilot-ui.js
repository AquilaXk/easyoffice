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
  const MAX_HISTORY_ITEMS = 16;
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
  const FAST_SELECTION_COMMANDS = new Set(["fix_spelling", "shorten", "translate"]);

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

  const buildCheckIcon = (d) => createSvgIcon(d, [
    { tag: "circle", cx: "8", cy: "8", r: "6.5", stroke: "currentColor", "stroke-width": "1.4" },
    { d: "M5.2 8.2L7.1 10.1L10.9 6.1", stroke: "currentColor", "stroke-width": "1.5", "stroke-linecap": "round", "stroke-linejoin": "round" },
  ], 14);

  function buildLocationIcon(d, mod) {
    if (mod === "SHEET") return createSvgIcon(d, [
      { tag: "rect", x: "2", y: "2.5", width: "12", height: "11", rx: "1.5", stroke: "currentColor", "stroke-width": "1.3" },
      { d: "M2 6.5H14M6.5 2.5V13.5", stroke: "currentColor", "stroke-width": "1.2" },
    ], 12);
    if (mod === "SLIDE") return createSvgIcon(d, [
      { tag: "rect", x: "1.8", y: "3", width: "12.4", height: "9.5", rx: "1.5", stroke: "currentColor", "stroke-width": "1.3" },
      { d: "M5 6.5H11M5 9.2H8.8", stroke: "currentColor", "stroke-width": "1.2", "stroke-linecap": "round" },
    ], 12);
    return createSvgIcon(d, [
      { tag: "rect", x: "3", y: "2", width: "10", height: "12", rx: "1.5", stroke: "currentColor", "stroke-width": "1.3" },
      { d: "M5.2 5.5H10.8M5.2 8H10.8M5.2 10.5H8.5", stroke: "currentColor", "stroke-width": "1.2", "stroke-linecap": "round" },
    ], 12);
  }

  function buildExampleArrowIcon(d) {
    const svg = createSvgIcon(d, [{ d: "M5 11L11 5M6.2 5H11V9.8", stroke: "currentColor", "stroke-width": "1.4", "stroke-linecap": "round", "stroke-linejoin": "round" }], 13);
    svg.setAttribute("class", "eo-copilot-example-arrow");
    return svg;
  }

  const TITLE_MAX_CHARS = 40;

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
    const editMode = host.__EASYOFFICE_COPILOT_EDIT_MODE__;
    const icons = host.__EASYOFFICE_ICONS__;

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
    let moreMenu = null;
    let plusMenu = null;
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
      getFastProviderPayload: () => modelUi.getFastProviderPayload(),
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
      if (editMode.REVIEW_ACTIONS.has(action)) return running || regenerating || (action === "discard" && !copilotActions.canUndoTurn(msg.id));
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
      threadView.sync(threadEl);
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
        icons.setIcon(document, sendBtn, running ? "stop" : "arrow-up");
      }
      const plusBtn = byId("btn-copilot-plus");
      if (plusBtn) plusBtn.disabled = running || !isModuleSupported(mod);
      syncSelectionBadge();
    }

    const mkEl = (tag, cls, txt) => {
      const n = document.createElement(tag);
      if (cls) n.className = cls;
      if (txt !== undefined) n.textContent = txt;
      return n;
    };

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
      const nodes = examplesFor(mod).map((ex) => {
        const btn = mkEl("button", "eo-copilot-example-btn");
        btn.type = "button";
        btn.appendChild(mkEl("span", undefined, ex.label));
        btn.appendChild(buildExampleArrowIcon(document));
        btn.addEventListener("click", () => useExample(ex));
        return btn;
      });
      examplesWrap.replaceChildren(...nodes);
    }

    function examplesFor(mod) {
      const examplesMap = host.__EASYOFFICE_COPILOT_SESSION_ANCHOR__?.MODULE_EXAMPLES || {};
      return examplesMap[mod] || examplesMap.DOC || [];
    }

    // 예시 질문은 입력칸에 채울 뿐 보내지 않는다.
    function useExample(ex) {
      const inputEl = byId("copilot-input");
      if (!inputEl) return;
      preferredMode = ASK_ONLY_MODULES.has(getActiveModule()) ? "ask" : "edit";
      inputEl.value = ex.prompt;
      autoGrowTextarea(inputEl);
      renderWelcome();
      syncComposerControls();
      inputEl.focus();
    }

    function buildPlusMenu() {
      const menu = byId("copilot-plus-menu");
      const items = examplesFor(getActiveModule()).map((ex) => {
        const btn = mkEl("button", "eo-copilot-popup-item", ex.label);
        btn.type = "button";
        btn.setAttribute("role", "menuitem");
        btn.addEventListener("click", () => useExample(ex));
        return btn;
      });
      menu.replaceChildren(mkEl("div", "eo-copilot-popup-heading", "예시 질문"), ...items);
    }

    // 머리글 제목은 첫 질문(한 줄로 줄여서), 대화가 없으면 "새 대화".
    function syncTitle() {
      const titleEl = byId("copilot-title");
      if (!titleEl) return;
      const first = messages.find((m) => m.role === "user");
      const text = first ? first.text.replace(/\s+/g, " ").trim() : "";
      titleEl.textContent = Array.from(text).slice(0, TITLE_MAX_CHARS).join("") || "새 대화";
      titleEl.title = text;
    }

    function renderThread() {
      const threadEl = byId("copilot-thread");
      if (!threadEl) return;
      threadEl.replaceChildren(...messages.map((msg) => threadView.renderTurn(msg)));
      syncTitle();
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
      if (!visible) {
        modelUi.closeMenu(false);
        moreMenu?.close();
        plusMenu?.close();
      }
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

    const anchorApi = host.__EASYOFFICE_COPILOT_SESSION_ANCHOR__;
    const sessionAnchor = anchorApi?.createCopilotSessionAnchor?.() || null;
    const usageTracker = anchorApi?.createUsageTracker?.() || null;
    const normalizeLocations = (rawList, fallbackMod) => (
      anchorApi?.normalizeLocations ? anchorApi.normalizeLocations(rawList, fallbackMod) : []
    );

    function buildHistoryPayload(limit = MAX_HISTORY_ITEMS) {
      if (anchorApi?.buildHistoryPayload) {
        return anchorApi.buildHistoryPayload(messages, limit);
      }
      return [];
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
      if (anchorApi?.evictAlignedMessages) {
        anchorApi.evictAlignedMessages(messages, MAX_MESSAGES, 8);
      } else if (messages.length >= MAX_MESSAGES) {
        messages.splice(0, 8);
      }
      const historyLimit = usageTracker
        ? usageTracker.preemptForWindow(messages, modelUi.getContextWindow?.(), MAX_HISTORY_ITEMS)
        : MAX_HISTORY_ITEMS;
      const history = request.command ? [] : buildHistoryPayload(historyLimit);
      const frozenSelection = mod === "SHEET" ? copilotActions.freezeSheetSelection() : null;
      const { selection, context } = copilotActions.extractContext(mod, { target: request.target, frozenSelection });

      if (clearInput) {
        const inputEl = byId("copilot-input");
        if (inputEl) {
          inputEl.value = "";
          autoGrowTextarea(inputEl);
        }
      }
      turnCounter += 1;
      const turnId = "turn-" + turnCounter;
      const startedAt = Date.now();
      messages.push({ id: turnId + "-u", role: "user", text: request.prompt, at: startedAt });
      const assistantEntry = {
        id: turnId,
        role: "assistant",
        at: startedAt,
        startedAt,
        durationMs: 0,
        steps: [],
        stepsOpen: false,
        feedback: null,
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

    // 답이 끝난(또는 멈춘) 때를 적는다. 머리 접힘 줄의 "N초 동안 작업"과 아래 행의 상대 시간이 이 값을 쓴다.
    function settleTurnClock(entry) {
      entry.at = Date.now();
      entry.durationMs = entry.at - entry.startedAt;
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

    // 기존 내용을 고치는 편집 응답은 적용하지 않고 바뀔 곳을 보여 준다. 바꾸기나 아래에 삽입을 눌러야 적용한다.
    function propose(entry, mod, operations) {
      const changes = copilotActions.buildChanges(mod, operations);
      entry.change = { state: "pending", mod, operations, changes, fingerprint: copilotActions.fingerprint(mod, operations), insertBelow: editMode.canInsertBelow(mod, operations) };
      if (mod === "DOC") copilotActions.showDocPreview(changes);
    }

    // 새 내용만 더하는 응답은 미리보기 없이 문서에 초안으로 바로 넣는다(포인터·타이핑 연출 포함). 유지하거나 버릴 때까지 되돌리기 한 단계로 둔다.
    async function applyDraft(entry, mod, operations) {
      entry.change = { state: "draft", mod, operations };
      renderThread();
      try {
        const res = await copilotActions.applyOperationsAnimated(mod, "edit", operations, entry.id);
        entry.applied = res.applied;
        entry.updatedInPlace = res.applied;
        if (entry.locations.length === 0) entry.locations = res.locations;
        if (!res.applied) entry.change.state = "none";
        if (res.interrupted) setStatusFeedback?.("코파일럿 편집을 중지했습니다");
      } catch (err) {
        entry.change.state = "none";
        entry.error = true;
        entry.reply = String(err?.message || "코파일럿 편집을 적용하지 못했습니다.");
      }
    }

    function presentEdit(entry, mod, rawOperations, frozenSelection) {
      const operations = copilotActions.normalizeOperations(mod, rawOperations, frozenSelection);
      if (editMode.classifyEditMode(mod, operations, copilotActions.editProbe) === editMode.DRAFT) return applyDraft(entry, mod, operations);
      return propose(entry, mod, operations);
    }

    function dismissPreview() {
      for (const msg of messages) editMode.settle(msg.change);
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
        const resolved = sessionAnchor
          ? sessionAnchor.resolveTurnPayload({ mod, command: turn.request.command, liveContext: context, liveSelection: selection })
          : { promptCacheKey: undefined, context, contextDelta: null };
        const primaryProvider = modelUi.currentProviderPayload();
        const useFastForCmd = Boolean(turn.request.command && FAST_SELECTION_COMMANDS.has(turn.request.command));
        const fastProvider = useFastForCmd && modelUi.getFastProviderPayload
          ? modelUi.getFastProviderPayload()
          : primaryProvider;
        const canFallbackPrimary = Boolean(
          useFastForCmd && fastProvider?.model && primaryProvider?.model && fastProvider.model !== primaryProvider.model,
        );
        const baseReq = {
          action: "copilot_run",
          requestId: turn.requestId,
          promptCacheKey: resolved.promptCacheKey,
          mode,
          module: mod,
          prompt: turn.request.prompt,
          history: turn.history,
          selection,
          context: resolved.context,
          provider: fastProvider,
        };
        if (resolved.contextDelta) baseReq.contextDelta = resolved.contextDelta;
        if (turn.request.command) baseReq.command = turn.request.command;
        const turnArgs = {
          baseReq,
          frozenSelection: turn.frozenSelection,
          newSubRequestId: (stepIdx) => turn.requestId + "-s" + stepIdx,
          isCancelled: () => token !== activeRunToken,
          onStreamText: (text) => showStreamed(assistantEntry, text, token),
          onActiveRequestId: (id) => { activeRequestId = id; },
          onToolStep: (label) => {
            if (token !== activeRunToken) return;
            assistantEntry.steps.push(label);
            renderThread();
          },
        };
        let res;
        try {
          res = await agentLoop.runAgentTurn(turnArgs);
        } catch (err) {
          if (!canFallbackPrimary || token !== activeRunToken) throw err;
          showStreamed(assistantEntry, "", token);
          res = await agentLoop.runAgentTurn({
            ...turnArgs,
            baseReq: { ...baseReq, requestId: turn.requestId + "-fb", provider: primaryProvider },
          });
        }
        if (!res || token !== activeRunToken) return;
        usageTracker?.record(res.usage, res.lastContextTokens);
        const ops = Array.isArray(res?.operations) ? res.operations : [];
        const explicitLocs = normalizeLocations(res?.locations, mod);
        assistantEntry.pending = false;
        assistantEntry.reply = String(res?.reply || "요청을 처리했습니다.");
        assistantEntry.reasoning = String(res?.reasoning || "");
        assistantEntry.locations = explicitLocs;
        if (mod === "PDF") void copilotActions.pointAtPdfPages(explicitLocs);
        if (mode === "edit" && ops.length > 0) await presentEdit(assistantEntry, mod, ops, turn.frozenSelection);
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
          settleTurnClock(assistantEntry);
          renderThread();
          syncComposerControls();
        }
      }
    }

    // 바꾸기: 문서가 미리보기를 본 때와 같으면 포인터·타이핑 연출과 함께 적용한다(되돌리기는 한 단계).
    // 아래에 삽입(`below`): 같은 지문 검사 뒤 원문은 두고 새 글을 원 문단 아래 새 문단으로 넣는다(연출 없이).
    async function acceptChange(msg, { below = false } = {}) {
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
        const appliedRes = below
          ? await copilotActions.applyOperations(change.mod, "edit", copilotActions.insertBelowOperations(change.mod, change.operations), msg.id)
          : await copilotActions.applyOperationsAnimated(change.mod, "edit", change.operations, msg.id);
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

    // 유지: 초안을 문서에 두고 결정을 닫는다(이어서 실행 취소 칩이 남는다).
    function keepDraft(msg) {
      if (msg?.change?.state !== "draft" || running || regenerating) return;
      msg.change.state = "kept";
      renderThread();
    }

    // 버리기(초안): 넣은 것을 되돌린다. 그 사이 문서를 고쳤으면 되돌릴 수 없다.
    async function discardDraft(msg) {
      if (msg?.change?.state !== "draft" || running || regenerating) return;
      if (!(await copilotActions.undoLastTurn(msg.id))) {
        setStatusFeedback?.("문서가 바뀌어 초안을 버릴 수 없습니다");
        renderThread();
        return;
      }
      msg.undone = true;
      msg.change.state = "discarded";
      renderThread();
    }

    const reviewHandlers = {
      accept: acceptChange,
      "insert-below": (msg) => acceptChange(msg, { below: true }),
      reject: rejectChange,
      keep: keepDraft,
      discard: discardDraft,
    };

    function undoTurn(msg) {
      void copilotActions.undoLastTurn(msg.id).then((restored) => {
        if (!restored) return;
        msg.undone = true;
        renderThread();
        setStatusFeedback?.("코파일럿 편집을 실행 취소했습니다");
      });
    }

    const threadView = host.__EASYOFFICE_COPILOT_THREAD_VIEW__.createThreadView({
      document,
      copilotActions,
      editMode,
      mkEl,
      hooks: {
        actionDisabled,
        review: reviewHandlers,
        regenerate: () => regenerateLast(),
        undo: undoTurn,
        locate: (loc) => void copilotActions.locateTarget(loc).then(() => syncSelectionBadge()),
        locationIcon: (mod) => buildLocationIcon(document, mod),
        checkIcon: () => buildCheckIcon(document),
        renderReply: renderReplyContent,
        isLastTurn: (msg) => messages.at(-1) === msg,
        isBusy: () => running || regenerating,
        resend: (msg) => void runTurn({ prompt: msg.text }),
        notify: (text) => setStatusFeedback?.(text),
      },
    });

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
        if (last.change?.state === "pending") copilotActions.clearPreview();
        editMode.settle(last.change);
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
        settleTurnClock(last);
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
      const { createPopupMenu } = host.__EASYOFFICE_COPILOT_POPUP__;
      moreMenu = createPopupMenu({ document, trigger: byId("btn-copilot-more"), menu: byId("copilot-more-menu"), align: "end" });
      plusMenu = createPopupMenu({ document, trigger: byId("btn-copilot-plus"), menu: byId("copilot-plus-menu"), placement: "up", align: "start", onOpen: buildPlusMenu });
      byId("btn-copilot-new")?.addEventListener("click", () => {
        // 연출 중이면 쓰던 부분도 적용하지 않고 버린다.
        copilotActions.abortPerformance();
        stopActiveRun();
        dismissPreview();
        sessionAnchor?.resetSession(true);
        usageTracker?.reset();
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
      onDocumentOpened: () => {
        dismissPreview();
        copilotGhost?.dismiss?.();
        sessionAnchor?.resetSession(true);
        usageTracker?.reset();
        syncPanelChrome();
      },
      onModuleChanged: () => {
        dismissPreview();
        copilotGhost?.dismiss?.();
        sessionAnchor?.syncActiveModule(getActiveModule());
        usageTracker?.reset();
        if (getActiveModule() === "HWP") void copilotActions.refreshHwpSnapshot?.().then(() => syncSelectionBadge()).catch(() => {});
        syncPanelChrome();
      },
      syncSelection: () => syncSelectionBadge(),
      isPerforming: () => copilotActions.isPerforming(),
      whenIdle: () => copilotActions.whenIdle(),
      getSessionUsage: () => usageTracker?.snapshot?.() || null,
      ghost: copilotGhost,
    };
  }

  host.__EASYOFFICE_CREATE_COPILOT_UI__ = createCopilotUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
