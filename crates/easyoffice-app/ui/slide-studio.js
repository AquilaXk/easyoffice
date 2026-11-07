(function (host) {
  "use strict";

  // Slide studio: deck management (add / duplicate / delete / reorder with undo), live thumbnail
  // list, deck aspect ratio, and the slideshow (full screen with in-window fallback, navigation,
  // black/white screen, presenter view). Every mutation goes through the workspace undo snapshot
  // and autosave activity hook; the slide model itself stays in `slideDeckState` so save (#104),
  // open (#111) and autosave keep reading one source of truth.
  const ASPECTS = new Set(["16:9", "4:3", "A4"]);
  const THUMB_REFRESH_MS = 150;
  const EMPTY_TITLE_LABEL = "(제목 없음)";
  const NEXT_KEYS = new Set(["ArrowRight", "ArrowDown", " ", "PageDown", "Enter", "n", "N"]);
  const PREV_KEYS = new Set(["ArrowLeft", "ArrowUp", "PageUp", "Backspace"]);
  const MENU_ACTIONS = {
    new: "add",
    duplicate: "duplicate",
    delete: "remove",
    up: "moveUp",
    down: "moveDown",
  };

  function slideLabel(slide) {
    return String(slide?.title || "").trim() || EMPTY_TITLE_LABEL;
  }

  function slideSnippet(slide) {
    return String(slide?.lower || "").replace(/\s+/g, " ").trim();
  }

  function formatElapsed(totalSeconds) {
    const mm = String(Math.floor(totalSeconds / 60)).padStart(2, "0");
    const ss = String(totalSeconds % 60).padStart(2, "0");
    return mm + ":" + ss;
  }

  function thumbIndexFromEvent(ev) {
    const btn = ev.target?.closest?.(".slide-thumb");
    return btn ? Number(btn.dataset.slideIdx) : -1;
  }

  function fillThumb(btn, slide, idx, isActive) {
    const label = slideLabel(slide);
    btn.className = "slide-thumb" + (isActive ? " active" : "");
    btn.dataset.slideIdx = String(idx);
    btn.setAttribute("title", idx + 1 + ". " + label);
    btn.setAttribute("aria-label", idx + 1 + "번 슬라이드: " + label);
    if (isActive) btn.setAttribute("aria-current", "true");
    else btn.removeAttribute("aria-current");
    const titleEl = btn.querySelector(".slide-thumb-title");
    titleEl.textContent = label;
    titleEl.classList.toggle("is-empty", label === EMPTY_TITLE_LABEL);
    btn.querySelector(".slide-thumb-snippet").textContent = slideSnippet(slide);
    btn.querySelector(".slide-thumb-num").textContent = String(idx + 1);
  }

  function createSlideStudio(deps) {
    const {
      document,
      slideDeckState: deck,
      getActiveSlideIndex,
      getActiveModule,
      getPanels,
      pushUndoSnapshot,
      captureWorkspaceSnapshot,
      pushRawUndoSnapshot,
      getHistoryMutationEpoch,
      noteActivity,
    } = deps;
    const { restoreDomChildren } = host.__EASYOFFICE_SHEET_UTILS__;
    const byId = (id) => document.getElementById(id);

    let dragFrom = -1;
    let thumbTimer = null;
    let menuIdx = -1;
    let showIdx = 0;
    let showStartIdx = 0;
    let showing = false;
    let fullscreenEntered = false;
    let elapsedSeconds = 0;
    let clockTimer = null;
    let returnFocusEl = null;

    function setFeedback(text) {
      const fb = byId("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    function deckAspect() {
      const raw = deck[0]?.aspect;
      return ASPECTS.has(raw) ? raw : "16:9";
    }

    // ---- deck operations -------------------------------------------------------------------

    function applyDeckChange(change) {
      const hadListFocus = Boolean(byId("slide-thumb-list")?.contains(document.activeElement));
      pushUndoSnapshot(false);
      const nextIdx = change();
      getPanels().renderSlideDeck(nextIdx);
      noteActivity();
      if (hadListFocus) focusThumb(nextIdx);
    }

    function focusThumb(idx) {
      byId("slide-thumb-list")?.querySelector('[data-slide-idx="' + idx + '"]')?.focus();
    }

    function addSlide() {
      applyDeckChange(() => {
        const base = deck[getActiveSlideIndex()] || deck[0];
        const empty = host.__EASYOFFICE_RIBBON_CATALOG__.createEmptySlideStyleFields();
        deck.push({
          title: "",
          lower: "",
          notes: "",
          transition: base?.transition || "fade",
          ...empty,
          aspect: deckAspect(),
        });
        return deck.length - 1;
      });
    }

    function duplicateSlide(idx = getActiveSlideIndex()) {
      if (!deck[idx]) return;
      applyDeckChange(() => {
        deck.splice(idx + 1, 0, structuredClone(deck[idx]));
        return idx + 1;
      });
    }

    function removeSlide(idx = getActiveSlideIndex()) {
      if (!deck[idx]) return;
      if (deck.length <= 1) {
        setFeedback("마지막 슬라이드는 삭제할 수 없습니다. 새 슬라이드를 추가한 뒤 삭제하세요");
        return;
      }
      applyDeckChange(() => {
        deck.splice(idx, 1);
        return Math.min(idx, deck.length - 1);
      });
      setFeedback("슬라이드를 삭제했습니다. Ctrl+Z로 되돌릴 수 있습니다");
    }

    function moveSlide(from, to) {
      if (!deck[from] || to < 0 || to >= deck.length || from === to) return;
      applyDeckChange(() => {
        const [moved] = deck.splice(from, 1);
        deck.splice(to, 0, moved);
        return to;
      });
    }

    const moveUp = (idx = getActiveSlideIndex()) => moveSlide(idx, idx - 1);
    const moveDown = (idx = getActiveSlideIndex()) => moveSlide(idx, idx + 1);
    const operations = { add: addSlide, duplicate: duplicateSlide, remove: removeSlide, moveUp, moveDown };

    function setDeckAspect(value) {
      if (!ASPECTS.has(value) || value === deckAspect()) return;
      pushUndoSnapshot(false);
      deck.forEach((s) => {
        s.aspect = value;
      });
      getPanels().renderSlideDeck(getActiveSlideIndex());
      noteActivity();
    }

    // ---- thumbnails ------------------------------------------------------------------------

    function buildThumb(slide, idx, isActive) {
      const btn = document.createElement("button");
      btn.setAttribute("type", "button");
      btn.setAttribute("draggable", "true");
      const num = document.createElement("span");
      num.className = "slide-thumb-num";
      const text = document.createElement("span");
      text.className = "slide-thumb-text";
      const title = document.createElement("span");
      title.className = "slide-thumb-title";
      const snippet = document.createElement("span");
      snippet.className = "slide-thumb-snippet";
      text.appendChild(title);
      text.appendChild(snippet);
      btn.appendChild(num);
      btn.appendChild(text);
      fillThumb(btn, slide, idx, isActive);
      return btn;
    }

    function renderThumbs(activeIdx) {
      const list = byId("slide-thumb-list");
      if (!list) return;
      list.replaceChildren(...deck.map((s, i) => buildThumb(s, i, i === activeIdx)));
    }

    function refreshThumbText() {
      thumbTimer = null;
      const list = byId("slide-thumb-list");
      const items = list ? Array.from(list.querySelectorAll(".slide-thumb")) : [];
      if (items.length !== deck.length) return renderThumbs(getActiveSlideIndex());
      items.forEach((btn, i) => fillThumb(btn, deck[i], i, i === getActiveSlideIndex()));
    }

    function scheduleThumbRefresh() {
      if (thumbTimer !== null) host.clearTimeout?.(thumbTimer);
      thumbTimer = host.setTimeout?.(refreshThumbText, THUMB_REFRESH_MS) ?? null;
    }

    function updateToolbarState(activeIdx) {
      const setDisabled = (id, off) => {
        const el = byId(id);
        if (el) el.disabled = off;
      };
      setDisabled("btn-slide-delete", deck.length <= 1);
      setDisabled("btn-slide-move-up", activeIdx <= 0);
      setDisabled("btn-slide-move-down", activeIdx >= deck.length - 1);
    }

    // Called by the panels after every slide render (switch, undo/redo, open, management).
    function afterRender(activeIdx) {
      renderThumbs(activeIdx);
      updateToolbarState(activeIdx);
      const aspect = deckAspect();
      const stage = byId("slide-stage-box");
      if (stage) stage.dataset.aspect = aspect;
      const select = byId("slide-aspect-select");
      if (select) select.value = aspect;
    }

    // ---- editing: typing-granular undo snapshots for the stage zones and notes -----------------

    function bindEditing() {
      let snapEpoch = -1;
      let snapTarget = null;
      let snapSlide = -1;
      const reset = () => {
        snapEpoch = -1;
        snapTarget = null;
        snapSlide = -1;
      };
      const ensureSnapshot = (target) => {
        if (
          snapTarget === target &&
          snapSlide === getActiveSlideIndex() &&
          snapEpoch === getHistoryMutationEpoch()
        ) {
          return;
        }
        const preSnap = captureWorkspaceSnapshot(false);
        preSnap.slides = structuredClone(deck);
        pushRawUndoSnapshot(preSnap);
        snapEpoch = getHistoryMutationEpoch();
        snapTarget = target;
        snapSlide = getActiveSlideIndex();
      };
      const current = () => deck[getActiveSlideIndex()];
      const fields = [
        ["slide-presenter-notes", (el) => { current().notes = el.value; }],
        ["slide-title-zone", (el) => { current().title = el.textContent || ""; }],
        ["slide-lower-zone", () => getPanels().syncActiveSlideDomStylesToDeck()],
      ];
      for (const [id, apply] of fields) {
        const el = byId(id);
        if (!el) continue;
        el.addEventListener("focus", reset);
        el.addEventListener("blur", reset);
        el.addEventListener("beforeinput", () => ensureSnapshot(el));
        el.addEventListener("input", () => {
          ensureSnapshot(el);
          if (current()) apply(el);
          scheduleThumbRefresh();
        });
      }
    }

    // ---- thumbnail list: select, drag and drop, context menu ---------------------------------

    function clearDropMarks() {
      byId("slide-thumb-list")
        ?.querySelectorAll(".slide-thumb")
        .forEach((b) => b.classList.remove("dragging", "drop-above", "drop-below"));
    }

    function onDragOver(ev) {
      const idx = thumbIndexFromEvent(ev);
      if (dragFrom < 0 || idx < 0) return;
      ev.preventDefault();
      if (ev.dataTransfer) ev.dataTransfer.dropEffect = "move";
      clearDropMarks();
      const btn = ev.target.closest(".slide-thumb");
      if (idx !== dragFrom) btn.classList.add(dragFrom > idx ? "drop-above" : "drop-below");
    }

    function closeMenu() {
      const menu = byId("slide-context-menu");
      if (menu) menu.hidden = true;
      menuIdx = -1;
    }

    function openMenu(idx, ev) {
      const menu = byId("slide-context-menu");
      if (!menu) return;
      if (idx !== getActiveSlideIndex()) getPanels().switchToSlideIndex(idx);
      menuIdx = idx;
      menu.style.left = Math.max(0, ev.clientX || 0) + "px";
      menu.style.top = Math.max(0, ev.clientY || 0) + "px";
      const off = { delete: deck.length <= 1, up: idx <= 0, down: idx >= deck.length - 1 };
      menu.querySelectorAll("[data-slide-action]").forEach((b) => {
        b.disabled = Boolean(off[b.dataset.slideAction]);
      });
      menu.hidden = false;
      menu.querySelector("button:not([disabled])")?.focus();
    }

    function bindThumbList() {
      const list = byId("slide-thumb-list");
      if (!list) return;
      list.addEventListener("click", (ev) => {
        const idx = thumbIndexFromEvent(ev);
        if (idx < 0) return;
        getPanels().switchToSlideIndex(idx);
        focusThumb(idx);
      });
      list.addEventListener("dragstart", (ev) => {
        const idx = thumbIndexFromEvent(ev);
        if (idx < 0) return;
        dragFrom = idx;
        if (ev.dataTransfer) {
          ev.dataTransfer.effectAllowed = "move";
          ev.dataTransfer.setData("text/plain", String(idx));
        }
        ev.target.closest(".slide-thumb").classList.add("dragging");
      });
      list.addEventListener("dragover", onDragOver);
      list.addEventListener("drop", (ev) => {
        const idx = thumbIndexFromEvent(ev);
        const from = dragFrom;
        dragFrom = -1;
        clearDropMarks();
        if (from < 0 || idx < 0) return;
        ev.preventDefault();
        moveSlide(from, idx);
      });
      list.addEventListener("dragend", () => {
        dragFrom = -1;
        clearDropMarks();
      });
      list.addEventListener("contextmenu", (ev) => {
        const idx = thumbIndexFromEvent(ev);
        if (idx < 0) return;
        ev.preventDefault();
        openMenu(idx, ev);
      });
    }

    function bindMenu() {
      const menu = byId("slide-context-menu");
      if (!menu) return;
      menu.addEventListener("click", (ev) => {
        const btn = ev.target?.closest?.("[data-slide-action]");
        if (!btn || btn.disabled) return;
        const target = menuIdx;
        closeMenu();
        operations[MENU_ACTIONS[btn.dataset.slideAction]]?.(target);
      });
      document.addEventListener("click", (ev) => {
        if (menuIdx >= 0 && !menu.contains(ev.target)) closeMenu();
      });
    }

    // ---- slideshow ---------------------------------------------------------------------------

    function paintShowSlide() {
      const slide = deck[showIdx];
      const viewport = byId("slide-show-viewport");
      if (!slide || !viewport) return;
      viewport.dataset.aspect = deckAspect();
      const frame = document.createElement("div");
      frame.id = "slide-show-frame";
      frame.className = "slide-show-frame";
      frame.dataset.transition = slide.transition || "fade";
      const title = document.createElement("h2");
      title.id = "slide-show-title";
      title.textContent = slide.title || "";
      Object.assign(title.style, {
        fontWeight: slide.titleStyle?.fontWeight || "",
        fontStyle: slide.titleStyle?.fontStyle || "",
        textDecoration: slide.titleStyle?.textDecoration || "",
      });
      const body = document.createElement("div");
      body.id = "slide-show-body";
      body.className = "slide-show-body";
      const text = document.createElement("p");
      text.textContent = slide.lower || "";
      body.appendChild(text);
      if (Array.isArray(slide.insertedNodes) && slide.insertedNodes.length > 0) {
        const holder = document.createElement("div");
        restoreDomChildren(holder, slide.insertedNodes, document);
        Array.from(holder.childNodes).forEach((node) => body.appendChild(node));
      }
      frame.appendChild(title);
      frame.appendChild(body);
      viewport.replaceChildren(frame);
      paintShowChrome();
    }

    function paintShowChrome() {
      const slide = deck[showIdx];
      const next = deck[showIdx + 1];
      byId("slide-show-counter").textContent = showIdx + 1 + " / " + deck.length;
      byId("slide-show-notes").textContent = String(slide?.notes || "").trim() || "(발표자 노트 없음)";
      byId("slide-show-next").textContent = next ? slideLabel(next) : "(마지막 슬라이드)";
      byId("slide-show-timer").textContent = formatElapsed(elapsedSeconds);
    }

    function tickClock() {
      clockTimer = host.setTimeout?.(() => {
        elapsedSeconds += 1;
        const timer = byId("slide-show-timer");
        if (timer) timer.textContent = formatElapsed(elapsedSeconds);
        if (showing) tickClock();
      }, 1000) ?? null;
    }

    function enterFullscreen() {
      const root = document.documentElement;
      if (typeof root.requestFullscreen !== "function") return;
      try {
        Promise.resolve(root.requestFullscreen()).then(
          () => {
            fullscreenEntered = showing;
          },
          () => {
            fullscreenEntered = false;
          }
        );
      } catch {
        fullscreenEntered = false;
      }
    }

    function startShow(fromCurrent) {
      if (showing || getActiveModule() !== "SLIDE" || deck.length === 0) return;
      getPanels().syncActiveSlideDomStylesToDeck();
      closeMenu();
      showIdx = fromCurrent ? Math.min(getActiveSlideIndex(), deck.length - 1) : 0;
      showStartIdx = showIdx;
      showing = true;
      elapsedSeconds = 0;
      returnFocusEl = document.activeElement;
      const overlay = byId("slide-show-overlay");
      delete overlay.dataset.screen;
      byId("slide-show-presenter").hidden = true;
      overlay.hidden = false;
      document.activeElement?.blur?.();
      overlay.focus?.();
      paintShowSlide();
      tickClock();
      enterFullscreen();
    }

    function endShow() {
      if (!showing) return;
      showing = false;
      fullscreenEntered = false;
      if (clockTimer !== null) host.clearTimeout?.(clockTimer);
      clockTimer = null;
      byId("slide-show-overlay").hidden = true;
      byId("slide-show-viewport").replaceChildren();
      if (document.fullscreenElement && typeof document.exitFullscreen === "function") {
        Promise.resolve(document.exitFullscreen()).catch(() => {});
      }
      if (showIdx !== showStartIdx && deck[showIdx]) getPanels().switchToSlideIndex(showIdx);
      returnFocusEl?.focus?.();
      returnFocusEl = null;
    }

    function goTo(idx) {
      if (idx >= deck.length) return endShow();
      showIdx = Math.max(0, idx);
      delete byId("slide-show-overlay").dataset.screen;
      paintShowSlide();
    }

    function toggleScreen(color) {
      const overlay = byId("slide-show-overlay");
      if (overlay.dataset.screen === color) delete overlay.dataset.screen;
      else overlay.dataset.screen = color;
    }

    function handleShowKey(e) {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const k = e.key;
      const lower = k.length === 1 ? k.toLowerCase() : k;
      e.preventDefault();
      if (k === "Escape") endShow();
      else if (NEXT_KEYS.has(k)) goTo(showIdx + 1);
      else if (PREV_KEYS.has(k)) goTo(showIdx - 1);
      else if (k === "Home") goTo(0);
      else if (k === "End") goTo(deck.length - 1);
      else if (lower === "b") toggleScreen("black");
      else if (lower === "w") toggleScreen("white");
      else if (lower === "p") {
        const panel = byId("slide-show-presenter");
        panel.hidden = !panel.hidden;
      }
    }

    // Keyboard entry used by app.js before the shortcut table: owns every key while a show runs
    // and Esc for the thumbnail context menu. Returns true when the event was consumed.
    function handleKeydown(e) {
      if (showing) {
        handleShowKey(e);
        return true;
      }
      if (menuIdx >= 0 && e.key === "Escape") {
        e.preventDefault();
        closeMenu();
        return true;
      }
      return false;
    }

    function bindShow() {
      byId("slide-show-viewport")?.addEventListener("click", () => goTo(showIdx + 1));
      byId("slide-show-exit")?.addEventListener("click", () => endShow());
      document.addEventListener("fullscreenchange", () => {
        if (showing && fullscreenEntered && !document.fullscreenElement) endShow();
      });
    }

    function bindToolbar() {
      const clicks = {
        "btn-slide-duplicate": () => duplicateSlide(),
        "btn-slide-delete": () => removeSlide(),
        "btn-slide-move-up": () => moveUp(),
        "btn-slide-move-down": () => moveDown(),
        "btn-slide-show": () => startShow(false),
        "btn-slide-show-current": () => startShow(true),
      };
      for (const [id, run] of Object.entries(clicks)) byId(id)?.addEventListener("click", run);
      const aspect = byId("slide-aspect-select");
      aspect?.addEventListener("change", () => setDeckAspect(aspect.value));
    }

    function bind() {
      bindToolbar();
      bindThumbList();
      bindMenu();
      bindShow();
      bindEditing();
    }

    return {
      bind,
      afterRender,
      scheduleThumbRefresh,
      handleKeydown,
      startShow,
      addSlide,
      duplicateSlide,
      removeSlide,
      moveUp,
      moveDown,
      isShowing: () => showing,
    };
  }

  host.__EASYOFFICE_CREATE_SLIDE_STUDIO__ = createSlideStudio;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
