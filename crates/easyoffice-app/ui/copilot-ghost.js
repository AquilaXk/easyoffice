// 참조 구현: udecode/plate (MIT, Copyright (c) Ziad Beyens, Dylan Schiemann, Joe Anderson, Felix Feng,
// 커밋 e401aaf9, `packages/ai/src/react/copilot/CopilotPlugin.tsx`, see THIRD_PARTY_NOTICES.md) —
// 문서 편집기(#doc-editor) 선택적 텍스트 예측(Tab 수락 / Esc 닫기).
// 편집 DOM 밖(.doc-scroll-container) 오버레이에만 그려 수락 전까지 저장·자동저장·Undo 기록을 오염시키지 않는다.

(function (host) {
  "use strict";

  const STORAGE_KEY = "easyoffice.copilot.ghost.v1";
  const DEBOUNCE_MS = 950;
  const MAX_GHOST_CHARS = 160;
  const CONTEXT_TAIL_CHARS = 600;

  function createCopilotGhost(deps) {
    const {
      document,
      win = host,
      invokeIpc,
      getActiveModule,
      isModelConfigured,
      getProviderPayload,
      isPanelBusy,
      isImeComposing,
      captureWorkspaceSnapshot,
      commitDeferredUndoSnapshot,
      noteActivity,
      getWorkspacePanels,
    } = deps;
    const { clipCodePoints } = host.__EASYOFFICE_COPILOT_TEXT__;
    const docHtml = host.__EASYOFFICE_COPILOT_DOC_HTML__;

    let enabled = readStoredEnabled();
    let timerId = null;
    let activeSuggestion = null;
    let overlayEl = null;
    let ghostSheet = null;
    let requestSeq = 0;
    let activeReqId = "";

    function syncParaMarkMask(editor, block) {
      if (typeof win.CSSStyleSheet !== "function" || !("adoptedStyleSheets" in document)) return;
      if (!editor || !block) {
        if (ghostSheet) {
          document.adoptedStyleSheets = document.adoptedStyleSheets.filter((s) => s !== ghostSheet);
          ghostSheet = null;
        }
        return;
      }
      const nth = Array.prototype.indexOf.call(editor.children || [], block) + 1;
      if (nth <= 0) return;
      if (!ghostSheet) {
        ghostSheet = new win.CSSStyleSheet();
        document.adoptedStyleSheets = [...document.adoptedStyleSheets, ghostSheet];
      }
      ghostSheet.replaceSync(
        `#doc-editor[data-eo-ghost-active="true"] > :nth-child(${nth})::after { content: none !important; }`,
      );
    }

    function readStoredEnabled() {
      try {
        return win.localStorage?.getItem?.(STORAGE_KEY) === "on";
      } catch {
        return false;
      }
    }

    function writeStoredEnabled(val) {
      try {
        win.localStorage?.setItem?.(STORAGE_KEY, val ? "on" : "off");
      } catch {
        // Ignore storage quota/opaque-origin errors.
      }
    }

    function clearTimer() {
      if (timerId !== null) {
        win.clearTimeout?.(timerId);
        timerId = null;
      }
    }

    function dismiss() {
      clearTimer();
      requestSeq += 1;
      if (activeReqId) {
        const cancelId = activeReqId;
        activeReqId = "";
        void Promise.resolve(
          invokeIpc("run_smart_assistant", JSON.stringify({ action: "copilot_cancel", requestId: cancelId })),
        ).catch(() => {});
      }
      activeSuggestion = null;
      const ed = document.getElementById("doc-editor");
      if (ed?.dataset) delete ed.dataset.eoGhostActive;
      syncParaMarkMask(null, null);
      if (overlayEl) {
        overlayEl.remove();
        overlayEl = null;
      }
    }

    function setEnabled(next) {
      enabled = Boolean(next);
      writeStoredEnabled(enabled);
      if (!enabled) dismiss();
      return enabled;
    }

    function caretContextAtBlockEnd() {
      if (getActiveModule?.() !== "DOC") return null;
      const editor = document.getElementById("doc-editor");
      if (!editor) return null;
      const sel = win.getSelection?.();
      if (!sel || sel.rangeCount === 0 || !sel.isCollapsed) return null;
      const range = sel.getRangeAt(0);
      if (!editor.contains(range.startContainer)) return null;

      let block = range.startContainer.nodeType === 1 ? range.startContainer : range.startContainer.parentElement;
      while (block && block.parentElement !== editor) block = block.parentElement;
      if (!block) return null;
      const tag = (block.tagName || "").toLowerCase();
      if (tag !== "p" && tag !== "li" && tag !== "blockquote") return null;

      const fullText = String(block.textContent || "");
      if (fullText.trim().length < 4) return null;

      // Only suggest when caret sits at the end of the current block text.
      if (document.createRange) {
        const tailRange = document.createRange();
        if (typeof tailRange.selectNodeContents === "function") {
          tailRange.selectNodeContents(block);
          tailRange.setStart(range.endContainer, range.endOffset);
          if (String(tailRange.toString?.() || "").trim().length > 0) return null;
        } else if (range.endContainer?.nodeType === 3) {
          if (range.endOffset < String(range.endContainer.data || "").length) return null;
        }
      }

      const savedRange = typeof range.cloneRange === "function" ? range.cloneRange() : range;
      return { editor, block, range: savedRange, blockText: fullText };
    }

    const NAV_KEYS = new Set([
      "ArrowLeft",
      "ArrowRight",
      "ArrowUp",
      "ArrowDown",
      "Home",
      "End",
      "PageUp",
      "PageDown",
    ]);

    function renderGhostOverlay(target, text) {
      if (overlayEl) {
        overlayEl.remove();
        overlayEl = null;
      }
      const container = target.editor.parentElement;
      if (!container) return;

      const box = document.createElement("div");
      box.className = "eo-copilot-ghost-overlay eo-copilot-transient";
      box.dataset.copilotGhost = "true";
      box.setAttribute("aria-hidden", "true");
      box.textContent = text + "  [Tab]";

      const hostRect = container.getBoundingClientRect?.() || { left: 0, top: 0, width: 0 };
      const blockRect = target.block.getBoundingClientRect?.() || { left: 0, top: 0, width: 320, height: 20 };
      const caretRect = target.range.getBoundingClientRect?.();
      const hasCaretRect = caretRect && (caretRect.width > 0 || caretRect.height > 0);
      const zoomScale =
        Number(container.offsetWidth) > 0 && Number(hostRect.width) > 0
          ? hostRect.width / container.offsetWidth
          : 1;
      const rawLeft = hasCaretRect ? caretRect.left : blockRect.left + Math.min(blockRect.width * 0.6, 240);
      const rawTop = hasCaretRect ? caretRect.top : blockRect.top;
      const leftPx = (rawLeft - hostRect.left) / zoomScale + (Number(container.scrollLeft) || 0);
      const topPx = (rawTop - hostRect.top) / zoomScale + (Number(container.scrollTop) || 0);

      const styleTarget =
        target.range?.endContainer?.nodeType === 1
          ? target.range.endContainer
          : target.range?.endContainer?.parentElement || target.block;
      const style = win.getComputedStyle?.(styleTarget) || win.getComputedStyle?.(target.block);
      if (style) {
        const family = style.getPropertyValue?.("font-family") || style.fontFamily;
        if (family) box.style.setProperty("font-family", family);
        const size = Number.parseFloat(style.fontSize);
        if (Number.isFinite(size) && size > 0) box.style.setProperty("font-size", `${size}px`);
        if (hasCaretRect && Number(caretRect.height) > 0) {
          box.style.setProperty("line-height", `${caretRect.height / zoomScale}px`);
        } else if (style.lineHeight) {
          box.style.setProperty("line-height", style.lineHeight);
        }
      }

      box.style.setProperty("left", `${Math.max(8, Math.round(leftPx))}px`);
      box.style.setProperty("top", `${Math.max(4, Math.round(topPx))}px`);
      container.appendChild(box);
      if (target.editor?.dataset) target.editor.dataset.eoGhostActive = "true";
      syncParaMarkMask(target.editor, target.block);
      overlayEl = box;
    }

    function repositionActiveOverlay() {
      if (!activeSuggestion) return;
      const liveTarget = caretContextAtBlockEnd();
      if (
        !liveTarget ||
        liveTarget.block !== activeSuggestion.block ||
        liveTarget.blockText !== activeSuggestion.expectedBlockText
      ) {
        dismiss();
        return;
      }
      renderGhostOverlay(liveTarget, activeSuggestion.text);
    }

    function cleanGhostCompletion(rawReply, prefixText) {
      let txt = String(rawReply ?? "").trim();
      if (!txt) return "";
      txt = txt.replace(/^["'“”‘’]+|["'“”‘’]+$/g, "").replace(/\r?\n[\s\S]*$/, "").trim();
      const tail = prefixText.trim().slice(-24);
      if (tail && txt.startsWith(tail)) {
        txt = txt.slice(tail.length).trimStart();
      }
      if (!txt) return "";
      const needsLeadingSpace = prefixText.length > 0 && !/\s$/.test(prefixText) && /^[A-Za-z0-9]/.test(txt);
      return clipCodePoints((needsLeadingSpace ? " " : "") + txt, MAX_GHOST_CHARS);
    }

    async function requestSuggestion() {
      if (!enabled || isPanelBusy?.() || isImeComposing?.() || !isModelConfigured?.()) return;
      const target = caretContextAtBlockEnd();
      if (!target) return;

      const mySeq = ++requestSeq;
      const reqId = "cp-ghost-" + Date.now().toString(36) + "-" + mySeq;
      activeReqId = reqId;
      const snippet = clipCodePoints(target.blockText, CONTEXT_TAIL_CHARS);

      const req = {
        action: "copilot_run",
        requestId: reqId,
        mode: "ask",
        module: "DOC",
        prompt: "현재 문단의 마지막 문장에 자연스럽게 이어질 다음 구절만 한 줄(80자 이내, 따옴표·설명 없이 이어질 텍스트만)로 제안하세요.",
        history: [],
        selection: { range: "텍스트 예측" },
        context: {
          doc: {
            paragraphs: [snippet],
            selectedText: snippet,
            omittedParagraphs: 0,
          },
        },
        provider: getProviderPayload(),
      };

      try {
        const res = await invokeIpc("run_smart_assistant", JSON.stringify(req));
        if (mySeq !== requestSeq) return;
        activeReqId = "";
        const liveTarget = caretContextAtBlockEnd();
        if (!liveTarget || liveTarget.block !== target.block || liveTarget.blockText !== target.blockText) return;
        const suggestion = cleanGhostCompletion(res?.reply, liveTarget.blockText);
        if (!suggestion) return;
        activeSuggestion = {
          text: suggestion,
          block: liveTarget.block,
          expectedBlockText: liveTarget.blockText,
        };
        renderGhostOverlay(liveTarget, suggestion);
      } catch {
        if (mySeq === requestSeq) activeReqId = "";
      }
    }

    function scheduleSuggestion() {
      dismiss();
      if (!enabled || getActiveModule?.() !== "DOC" || !isModelConfigured?.()) return;
      timerId = win.setTimeout?.(() => {
        timerId = null;
        void requestSuggestion();
      }, DEBOUNCE_MS) ?? null;
    }

    function appendBeforeTrailingBr(block, node) {
      const last = block.lastChild;
      if (last && last.nodeType === 1 && String(last.tagName || "").toUpperCase() === "BR") {
        const prev = last.previousSibling;
        block.insertBefore(node, last);
        if (!prev || prev.nodeType !== 1 || String(prev.tagName || "").toUpperCase() !== "BR") {
          last.remove();
        }
        return;
      }
      block.appendChild(node);
    }

    function acceptSuggestion() {
      if (!activeSuggestion) return false;
      const editor = document.getElementById("doc-editor");
      const { text, block, expectedBlockText, skipCaretCheck } = activeSuggestion;
      const liveTarget = skipCaretCheck ? { block } : caretContextAtBlockEnd();
      dismiss();
      if (
        !editor ||
        !editor.contains(block) ||
        !liveTarget ||
        liveTarget.block !== block ||
        String(block.textContent || "") !== expectedBlockText
      ) {
        return false;
      }

      const beforeSnap = captureWorkspaceSnapshot?.(false);
      if (docHtml?.isTrackChangesActive?.(editor)) {
        const revId = docHtml.nextRevisionId(editor);
        const ins = document.createElement("ins");
        ins.className = "eo-rev-ins";
        ins.dataset.eoRevId = String(revId);
        ins.dataset.eoRevAuthor = "코파일럿";
        ins.dataset.eoRevDate = new Date().toISOString();
        ins.textContent = text;
        appendBeforeTrailingBr(block, ins);
      } else {
        appendBeforeTrailingBr(block, document.createTextNode(text));
      }

      const sel = win.getSelection?.();
      if (sel && document.createRange) {
        const r = document.createRange();
        const last = block.lastChild;
        const count = block.childNodes?.length || 0;
        const endIdx =
          last && last.nodeType === 1 && String(last.tagName || "").toUpperCase() === "BR"
            ? Math.max(0, count - 1)
            : count;
        if (typeof r.setStart === "function") {
          r.setStart(block, endIdx);
          r.setEnd(block, endIdx);
        } else if (typeof r.selectNodeContents === "function") {
          r.selectNodeContents(block);
          r.collapse(false);
        }
        sel.removeAllRanges?.();
        sel.addRange?.(r);
      }

      if (beforeSnap) commitDeferredUndoSnapshot?.(beforeSnap);
      host.__EASYOFFICE_DOC_NOTES_API__?.syncAll?.();
      noteActivity?.();
      const panels = getWorkspacePanels?.();
      panels?.saveActiveDocTabState?.();
      panels?.computeSmartStats?.();
      return true;
    }

    function showSuggestionForTest(text) {
      const target = caretContextAtBlockEnd();
      const editor = document.getElementById("doc-editor");
      const block = target?.block || editor?.querySelector?.("p, h1, h2, h3, h4, li, blockquote");
      if (!editor || !block) return false;
      const blockText = String(block.textContent || "");
      activeSuggestion = { text: String(text), block, expectedBlockText: blockText, skipCaretCheck: !target };
      renderGhostOverlay(
        target || { editor, block, range: { getBoundingClientRect: () => ({ left: 0, top: 0, width: 0, height: 0 }) } },
        String(text),
      );
      return true;
    }

    function bind() {
      const editor = document.getElementById("doc-editor");
      if (!editor) return;
      let lastCompEndAt = -Infinity;
      editor.addEventListener("compositionend", (ev) => {
        const ts = Number(ev?.timeStamp);
        if (Number.isFinite(ts)) lastCompEndAt = ts;
        scheduleSuggestion();
      });
      editor.addEventListener("keydown", (ev) => {
        if (!activeSuggestion) return;
        const ts = Number(ev.timeStamp);
        const withinPostComp = Number.isFinite(ts) && ts - lastCompEndAt >= 0 && ts - lastCompEndAt < 50;
        if (ev.isComposing || ev.keyCode === 229 || withinPostComp || isImeComposing?.()) return;
        if (ev.key === "Tab" && !ev.shiftKey && !ev.ctrlKey && !ev.metaKey && !ev.altKey) {
          if (acceptSuggestion()) {
            ev.preventDefault?.();
            ev.stopPropagation?.();
            ev.stopImmediatePropagation?.();
          }
          return;
        }
        if (ev.key === "Escape") {
          ev.preventDefault?.();
          ev.stopPropagation?.();
          ev.stopImmediatePropagation?.();
          dismiss();
          return;
        }
        if (NAV_KEYS.has(ev.key)) {
          dismiss();
        }
      }, true);

      editor.addEventListener("input", () => {
        if (isImeComposing?.()) {
          dismiss();
          return;
        }
        scheduleSuggestion();
      });

      for (const evt of ["click", "blur"]) {
        editor.addEventListener(evt, () => dismiss());
      }

      win.addEventListener?.("resize", repositionActiveOverlay);
      const stage = document.querySelector?.(".canvas-stage");
      if (stage && typeof win.MutationObserver === "function") {
        const zoomObserver = new win.MutationObserver(() => repositionActiveOverlay());
        zoomObserver.observe(stage, { attributes: true, attributeFilter: ["style"] });
      }
    }

    return {
      bind,
      isEnabled: () => enabled,
      setEnabled,
      hasSuggestion: () => activeSuggestion !== null,
      getSuggestionText: () => activeSuggestion?.text || "",
      acceptSuggestion,
      dismiss,
      showSuggestionForTest,
    };
  }

  host.__EASYOFFICE_CREATE_COPILOT_GHOST__ = createCopilotGhost;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
