// 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd.) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)

(function (host) {
  "use strict";

  const SVG_NS = "http://www.w3.org/2000/svg";
  const SUPPORTED_MODULES = new Set(["DOC", "SHEET", "SLIDE", "PDF"]);
  const MIN_WIDTH = 280;
  const MAX_WIDTH = 520;
  const DEFAULT_WIDTH = 320;
  const MAX_MESSAGES = 64;
  const MAX_HISTORY_ITEMS = 12;
  const MAX_PROMPT_CHARS = 8000;

  const MODULE_EXAMPLES = Object.freeze({
    SHEET: Object.freeze([
      { label: "목표 대비 % 및 최대 격차 표시", prompt: "목표 대비 %를 추가하고 가장 큰 격차를 표시하세요." },
      { label: "합계 및 평균 추가", prompt: "합계 행을 추가하세요." },
    ]),
    DOC: Object.freeze([
      { label: "문서 핵심 요약", prompt: "문서의 핵심 내용을 요약해 주세요." },
      { label: "피동 표현 다듬기", prompt: "피동 표현을 자연스러운 능동 문장으로 다듬어 주세요." },
    ]),
    SLIDE: Object.freeze([
      { label: "발표 슬라이드 추가", prompt: "발표 슬라이드를 추가하고 발표자 노트를 작성하세요." },
      { label: "발표자 노트 작성", prompt: "현재 슬라이드의 발표자 노트를 작성해 주세요." },
    ]),
    PDF: Object.freeze([
      { label: "문서 핵심 요약", prompt: "문서의 핵심 내용을 요약해 주세요." },
      { label: "주요 항목 정리", prompt: "문서의 주요 항목과 수치를 정리해 주세요." },
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
      const key = mod + ":" + (loc.sheet || "") + ":" + label;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push({
        module: mod,
        label,
        sheet: loc.sheet ? String(loc.sheet) : undefined,
        range: loc.range ? String(loc.range) : undefined,
        paragraphIndex: Number.isInteger(loc.paragraphIndex) ? loc.paragraphIndex : undefined,
        slideIndex: Number.isInteger(loc.slideIndex) ? loc.slideIndex : undefined,
      });
    }
    return out;
  }

  function createCopilotUi(deps) {
    const {
      document,
      invokeIpc,
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
    let turnCounter = 0;
    let dragState = null;
    const messages = [];

    const modelUi = host.__EASYOFFICE_CREATE_COPILOT_MODEL_UI__({
      document,
      invokeIpc,
      isRunning: () => running,
      onChanged: () => syncComposerControls(),
    });

    function isStartWindow() {
      const mod = getActiveModule();
      return mod === "START" || byId("app")?.dataset.windowKind === "start";
    }

    function isModuleSupported(mod = getActiveModule()) {
      return !isStartWindow() && SUPPORTED_MODULES.has(mod);
    }

    function effectiveMode(mod = getActiveModule()) {
      return mod === "PDF" ? "ask" : preferredMode;
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
      if (!selRow || !selText) return;
      if (!isOpen || !isModuleSupported()) {
        selRow.hidden = true;
        return;
      }
      const label = copilotActions.getSelectionLabel(getActiveModule());
      selRow.hidden = !label;
      if (label) selText.textContent = label;
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
        modeBtn.disabled = running || mod === "PDF";
        modeBtn.setAttribute("title", curMode === "edit" ? "현재 문서를 직접 편집" : "읽기 전용 질문");
      }
      modelUi.syncModelButton();
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
          preferredMode = mod === "PDF" ? "ask" : "edit";
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
      const showUndo = !msg.undone && copilotActions.canUndoTurn(msg.id);
      if (locs.length > 0 || showUndo) {
        const chipRow = document.createElement("div");
        chipRow.className = "eo-copilot-chip-row";
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
        wrap.appendChild(chipRow);
      }

      if (msg.pending) {
        const loading = document.createElement("div");
        loading.className = "eo-copilot-loading";
        loading.textContent = "응답 생성 중…";
        wrap.appendChild(loading);
      } else if (msg.reply) {
        const replyEl = document.createElement("div");
        replyEl.className = "eo-copilot-reply" + (msg.error ? " eo-copilot-reply--error" : "");
        replyEl.textContent = msg.reply;
        wrap.appendChild(replyEl);
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

    function setOpen(nextOpen) {
      if (!isModuleSupported()) return false;
      isOpen = Boolean(nextOpen);
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

    async function submitPrompt() {
      if (running || !isModuleSupported()) return;
      const inputEl = byId("copilot-input");
      const prompt = String(inputEl?.value || "").trim().slice(0, MAX_PROMPT_CHARS);
      if (!prompt) return;

      modelUi.closeMenu(false);
      if (!modelUi.isConfigured()) {
        modelUi.openDialog(undefined, undefined, "전송하기 전에 API 키를 입력하거나 로컬 모델을 선택해 주세요.");
        return;
      }
      const mod = getActiveModule();
      const mode = effectiveMode(mod);
      const history = buildHistoryPayload();
      const { selection, context } = copilotActions.extractContext(mod);

      if (inputEl) {
        inputEl.value = "";
        autoGrowTextarea(inputEl);
      }
      if (messages.length >= MAX_MESSAGES) messages.splice(0, messages.length - MAX_MESSAGES + 2);
      turnCounter += 1;
      const turnId = "turn-" + turnCounter;
      messages.push({ id: turnId + "-u", role: "user", text: prompt });
      const assistantEntry = {
        id: turnId,
        role: "assistant",
        pending: true,
        reply: "",
        reasoning: "",
        updatedInPlace: false,
        locations: [],
        undone: false,
        error: false,
      };
      messages.push(assistantEntry);

      running = true;
      activeRunToken += 1;
      const myToken = activeRunToken;
      renderThread();
      syncComposerControls();

      try {
        const req = {
          action: "copilot_run",
          mode,
          module: mod,
          prompt,
          history,
          selection,
          context,
          provider: modelUi.currentProviderPayload(),
        };
        const res = await invokeIpc("run_smart_assistant", JSON.stringify(req));
        if (myToken !== activeRunToken) return;
        const ops = Array.isArray(res?.operations) ? res.operations : [];
        const appliedRes = await copilotActions.applyOperations(mod, mode, ops, turnId);
        const explicitLocs = normalizeLocations(res?.locations, mod);
        assistantEntry.pending = false;
        assistantEntry.reply = String(res?.reply || "요청을 처리했습니다.");
        assistantEntry.reasoning = String(res?.reasoning || "");
        assistantEntry.updatedInPlace = mode === "edit" && (Boolean(res?.updatedInPlace) || appliedRes.applied);
        assistantEntry.locations = explicitLocs.length > 0 ? explicitLocs : appliedRes.locations;
      } catch (err) {
        if (myToken !== activeRunToken) return;
        assistantEntry.pending = false;
        assistantEntry.error = true;
        assistantEntry.reply = String(err?.message || "코파일럿 요청 처리에 실패했습니다.");
      } finally {
        if (myToken === activeRunToken) {
          running = false;
          renderThread();
          syncComposerControls();
        }
      }
    }

    function stopActiveRun() {
      if (!running) return;
      activeRunToken += 1;
      running = false;
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
        stopActiveRun();
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
        if (running || getActiveModule() === "PDF") return;
        preferredMode = preferredMode === "edit" ? "ask" : "edit";
        syncComposerControls();
      });
      byId("btn-copilot-send")?.addEventListener("click", () => {
        if (running) stopActiveRun();
        else void submitPrompt();
      });

      const inputEl = byId("copilot-input");
      inputEl?.addEventListener("input", () => {
        autoGrowTextarea(inputEl);
        renderWelcome();
        syncComposerControls();
      });
      inputEl?.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter" && !ev.shiftKey) {
          if (ev.isComposing || isImeComposing?.()) return;
          ev.preventDefault();
          void submitPrompt();
        }
      });

      document.addEventListener("focusin", () => syncSelectionBadge());
      document.addEventListener("keyup", () => syncSelectionBadge());
      host.addEventListener?.("resize", () => {
        applyWidthStyle(panelWidth);
        modelUi.repositionMenu();
      });
      bindResizer();
      syncPanelChrome();
    }

    return {
      bind,
      toggle,
      setOpen,
      onModuleChanged: () => syncPanelChrome(),
      syncSelection: () => syncSelectionBadge(),
    };
  }

  host.__EASYOFFICE_CREATE_COPILOT_UI__ = createCopilotUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
