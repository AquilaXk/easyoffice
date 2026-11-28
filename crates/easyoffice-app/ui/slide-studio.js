(function (host) {
  "use strict";

  // Slide studio: deck management (add / duplicate / delete / reorder with undo), live thumbnail
  // list, deck aspect ratio, and the slideshow (full screen with in-window fallback, navigation,
  // black/white screen, presenter view). Every mutation goes through the workspace undo snapshot
  // and autosave activity hook; the slide model itself stays in `slideDeckState` so save,
  // open and autosave keep reading one source of truth.
  const ASPECTS = new Set(["16:9", "4:3", "A4"]);
  const ASPECT_CYCLE = ["16:9", "4:3", "A4"];
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
    hide: "toggleHidden",
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

  // Writes a thumbnail's text and state; a thumbnail whose slide, position and state are unchanged is left alone.
  function fillThumb(btn, slide, idx, isActive) {
    const label = slideLabel(slide);
    const snippet = slideSnippet(slide);
    const hidden = Boolean(slide?.hidden);
    const key = idx + (isActive ? "\u0001" : "\u0000") + (hidden ? "\u0001" : "\u0000") + label + "\u0000" + snippet;
    if (btn.thumbKey === key) return;
    btn.thumbKey = key;
    btn.className = "slide-thumb" + (isActive ? " active" : "") + (hidden ? " is-hidden-slide" : "");
    btn.dataset.slideIdx = String(idx);
    btn.setAttribute("title", idx + 1 + ". " + label + (hidden ? " (숨김)" : ""));
    btn.setAttribute("aria-label", idx + 1 + "번 슬라이드" + (hidden ? "(숨김)" : "") + ": " + label);
    if (isActive) btn.setAttribute("aria-current", "true");
    else btn.removeAttribute("aria-current");
    const titleEl = btn.querySelector(".slide-thumb-title");
    titleEl.textContent = label;
    titleEl.classList.toggle("is-empty", label === EMPTY_TITLE_LABEL);
    btn.querySelector(".slide-thumb-snippet").textContent = snippet;
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
    let arrivedBack = false;
    let showing = false;
    let fullscreenEntered = false;
    let elapsedSeconds = 0;
    let clockTimer = null;
    let returnFocusEl = null;
    const slideCanvas = host.__EASYOFFICE_CREATE_SLIDE_CANVAS__?.(deps, {
      scheduleThumbRefresh: () => scheduleThumbRefresh(),
      startShow: (fromCur, opts) => startShow(fromCur, opts),
      onLeave: () => {
        slideDialogs?.closeAll?.();
        [slideFormat, slidePage, slideAnim].forEach((m) => m?.closeAll?.());
        endShow();
      },
    });
    const slideDialogs = slideCanvas ? host.__EASYOFFICE_CREATE_SLIDE_DIALOGS__?.(deps, slideCanvas) : null;
    const slideFormat = slideCanvas ? host.__EASYOFFICE_CREATE_SLIDE_FORMAT__?.(deps, slideCanvas) : null;
    const slidePage = slideCanvas ? host.__EASYOFFICE_CREATE_SLIDE_PAGE__?.(deps, slideCanvas) : null;
    const slideAnim = slideCanvas ? host.__EASYOFFICE_CREATE_SLIDE_ANIM__?.(deps, slideCanvas) : null;
    const showAnim = host.__EASYOFFICE_CREATE_SHOW_ANIM__?.();
    host.__EASYOFFICE_SLIDE_RUN_ACT__ = (act) => [slideFormat, slidePage, slideAnim].some((m) => m?.run?.(act) === true);
    if (slideCanvas) {
      slideCanvas.dialogs = slideDialogs;
      host.__EASYOFFICE_SLIDE_CANVAS__ = slideCanvas;
    }

    function setFeedback(text) {
      const fb = byId("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    const slideClipboard = host.__EASYOFFICE_CREATE_SLIDE_CLIPBOARD__?.({
      document, getActiveModule, getCanvas: () => slideCanvas, setFeedback,
    });
    if (slideClipboard) host.__EASYOFFICE_SLIDE_CLIPBOARD__ = slideClipboard;
    const slideContextMenu = host.__EASYOFFICE_CREATE_SLIDE_CONTEXT_MENU__?.({
      document, getActiveModule, getCanvas: () => slideCanvas, getClipboard: () => slideClipboard, setFeedback,
    });
    if (slideContextMenu) host.__EASYOFFICE_SLIDE_CONTEXT_MENU__ = slideContextMenu;

    function deckAspect() {
      const raw = deck[0]?.aspect;
      return ASPECTS.has(raw) ? raw : "16:9";
    }

    function deckOrientation() {
      return deck[0]?.orientation === "portrait" ? "portrait" : "landscape";
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
        const nextLayout = base?.layout && base.layout !== "titleSlide" ? base.layout : "titleAndContent";
        deck.push({
          title: "",
          lower: "",
          notes: "",
          transition: "none",
          ...empty,
          layout: nextLayout,
          aspect: deckAspect(),
          orientation: deckOrientation(),
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
      setFeedback("슬라이드를 삭제했습니다");
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
    const toggleHidden = (idx = getActiveSlideIndex()) => slidePage?.toggleHidden?.(idx);
    const operations = { add: addSlide, duplicate: duplicateSlide, remove: removeSlide, moveUp, moveDown, toggleHidden };

    function setDeckAspect(value, options = {}) {
      if (!ASPECTS.has(value) || value === deckAspect()) return;
      if (options.recordUndo !== false) pushUndoSnapshot(false);
      deck.forEach((s) => {
        s.aspect = value;
      });
      getPanels().renderSlideDeck(getActiveSlideIndex());
      noteActivity();
    }

    function cycleDeckAspect(options = {}) {
      const cur = deckAspect();
      const idx = ASPECT_CYCLE.indexOf(cur);
      const next = ASPECT_CYCLE[(idx + 1) % ASPECT_CYCLE.length];
      setDeckAspect(next, options);
      setFeedback("슬라이드 비율: " + next);
      return next;
    }

    function toggleDeckOrientation(options = {}) {
      const next = deckOrientation() === "portrait" ? "landscape" : "portrait";
      if (options.recordUndo !== false) pushUndoSnapshot(false);
      deck.forEach((s) => {
        s.orientation = next;
      });
      getPanels().renderSlideDeck(getActiveSlideIndex());
      noteActivity();
      setFeedback("슬라이드 용지 방향: " + (next === "portrait" ? "세로" : "가로"));
      return next;
    }

    function appendLink(url, label, elementId) {
      const linkText = label ? label + " (" + url + ")" : url;
      if (elementId && slideCanvas) {
        const cur = slideCanvas.getElements().find((e) => e.id === elementId);
        if (cur) {
          const prev = String(cur.text || "").trim();
          slideCanvas.updateElement(elementId, { text: prev ? prev + " " + linkText : linkText });
          setFeedback("하이퍼링크 적용: " + url);
          return;
        }
      }
      const curSlide = deck[getActiveSlideIndex()];
      if (!curSlide) return;
      pushUndoSnapshot(false);
      curSlide.lower = curSlide.lower ? curSlide.lower + "\n" + linkText : linkText;
      getPanels().renderSlideDeck(getActiveSlideIndex());
      noteActivity();
      setFeedback("하이퍼링크 적용: " + url);
    }

    function appendNote(text) {
      const clean = String(text || "").trim();
      const curSlide = deck[getActiveSlideIndex()];
      if (!clean || !curSlide) return;
      pushUndoSnapshot(false);
      curSlide.notes = curSlide.notes ? curSlide.notes + "\n" + clean : clean;
      getPanels().renderSlideDeck(getActiveSlideIndex());
      noteActivity();
      byId("slide-presenter-notes")?.focus?.();
      setFeedback("슬라이드 메모 추가 완료");
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

    // The buttons of a list with the deck's length are reused: only the ones that changed are written.
    function renderThumbs(activeIdx) {
      const list = byId("slide-thumb-list");
      if (!list) return;
      const items = list.children;
      if (items.length !== deck.length) {
        list.replaceChildren(...deck.map((s, i) => buildThumb(s, i, i === activeIdx)));
        return;
      }
      for (let i = 0; i < items.length; i += 1) fillThumb(items[i], deck[i], i, i === activeIdx);
    }

    function refreshThumbText() {
      thumbTimer = null;
      renderThumbs(getActiveSlideIndex());
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
      const orientation = deckOrientation();
      const layout = deck[activeIdx]?.layout || "titleAndContent";
      const stage = byId("slide-stage-box");
      if (stage) {
        stage.dataset.aspect = aspect;
        stage.dataset.orientation = orientation;
        stage.dataset.layout = layout;
      }
      const select = byId("slide-aspect-select");
      if (select) select.value = aspect;
      const layoutSel = byId("slide-layout-select");
      if (layoutSel) layoutSel.value = layout;
      if (stage) host.__EASYOFFICE_SLIDE_EXT__?.paintStage(document, stage, deck[activeIdx], activeIdx, slideCanvas?.stageSize?.() || { w: 960, h: 540 });
      slidePage?.refresh?.(activeIdx);
      slideCanvas?.render?.();
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
      if (menu && menuIdx >= 0) {
        menu.hidden = true;
        delete menu.dataset.mode;
      }
      menuIdx = -1;
    }

    function openMenu(idx, ev) {
      const menu = byId("slide-context-menu");
      if (!menu) return;
      slideContextMenu?.closeCanvasMenu?.();
      if (idx !== getActiveSlideIndex()) getPanels().switchToSlideIndex(idx);
      const thumbGrp = byId("slide-ctx-thumb-group");
      const canvasGrp = byId("slide-ctx-canvas-group");
      if (thumbGrp) thumbGrp.hidden = false;
      if (canvasGrp) canvasGrp.hidden = true;
      delete menu.dataset.mode;
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

    function handleThumbKeydown(ev) {
      if (ev.defaultPrevented || ev.altKey || ev.ctrlKey || ev.metaKey || ev.shiftKey) return false;
      if (!deck.length) return false;
      const cur = getActiveSlideIndex();
      let next = -1;
      if (ev.key === "ArrowUp") next = Math.max(0, cur - 1);
      else if (ev.key === "ArrowDown") next = Math.min(deck.length - 1, cur + 1);
      else if (ev.key === "Home") next = 0;
      else if (ev.key === "End") next = deck.length - 1;
      else return false;
      ev.preventDefault();
      getPanels().switchToSlideIndex(next);
      focusThumb(next);
      return true;
    }

    function bindThumbList() {
      const list = byId("slide-thumb-list");
      if (!list) return;
      list.addEventListener("keydown", handleThumbKeydown);
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
      viewport.dataset.orientation = deckOrientation();
      const frame = document.createElement("div");
      frame.id = "slide-show-frame";
      frame.className = "slide-show-frame";
      frame.dataset.transition = slide.transition || "none";
      frame.dataset.layout = slide.layout || "titleAndContent";
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
      if (Array.isArray(slide.elements) && slide.elements.length > 0) {
        const layer = host.__EASYOFFICE_SLIDE_CANVAS_CODEC__?.renderStaticElementsLayer?.(document, slide, deckAspect(), { orientation: deckOrientation() });
        if (layer) frame.appendChild(layer);
      }
      host.__EASYOFFICE_SLIDE_EXT__?.paintStage(document, frame, slide, showIdx, host.__EASYOFFICE_SLIDE_CANVAS_CODEC__.stageSizeOf(deckAspect(), deckOrientation()));
      viewport.replaceChildren(frame);
      showAnim?.start(frame, slide, { complete: arrivedBack });
      arrivedBack = false;
      paintShowChrome();
    }

    function paintShowChrome() {
      const slide = deck[showIdx];
      const next = deck[showIdx + 1];
      byId("slide-show-counter").textContent = showIdx + 1 + " / " + deck.length;
      byId("slide-show-notes").textContent = String(slide?.notes || "").trim() || "(노트 없음)";
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

    function startShow(fromCurrent, options = {}) {
      if (showing || getActiveModule() !== "SLIDE" || deck.length === 0) return;
      getPanels().syncActiveSlideDomStylesToDeck();
      closeMenu();
      showIdx = fromCurrent ? Math.min(getActiveSlideIndex(), deck.length - 1) : Math.min(nextShown(0), deck.length - 1);
      showStartIdx = showIdx;
      showing = true;
      elapsedSeconds = 0;
      returnFocusEl = document.activeElement;
      const overlay = byId("slide-show-overlay");
      delete overlay.dataset.screen;
      byId("slide-show-presenter").hidden = !options.presenter;
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

    // Hidden slides are skipped when moving through the show.
    const nextShown = (from) => {
      let i = from;
      while (i < deck.length && deck[i].hidden) i += 1;
      return i;
    };

    const prevShown = (from) => {
      let i = from;
      while (i >= 0 && deck[i].hidden) i -= 1;
      return i;
    };

    // The next click of the slide's animations, or the next slide once they have all played.
    function advance() {
      if (showAnim?.next()) return;
      goTo(nextShown(showIdx + 1));
    }

    // The last played click taken back, or the previous slide shown as it ends.
    function retreat() {
      if (showAnim?.back()) return;
      const prev = prevShown(showIdx - 1);
      if (prev < 0) return;
      arrivedBack = true;
      goTo(prev);
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
      else if (NEXT_KEYS.has(k)) advance();
      else if (PREV_KEYS.has(k)) retreat();
      else if (k === "Home") goTo(Math.min(nextShown(0), deck.length - 1));
      else if (k === "End") goTo(Math.max(prevShown(deck.length - 1), 0));
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
      const list = byId("slide-thumb-list");
      const target = e.target || document.activeElement;
      if (getActiveModule() === "SLIDE" && list && target && list.contains?.(target)) {
        if (handleThumbKeydown(e)) return true;
      }
      if (slideClipboard?.handleKeydown?.(e)) return true;
      if (slideCanvas?.handleKeydown?.(e)) return true;
      return false;
    }

    function bindShow() {
      byId("slide-show-viewport")?.addEventListener("click", () => advance());
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
      slideCanvas?.bind?.();
      slideDialogs?.bind?.();
      slideFormat?.bind?.();
      slidePage?.bind?.();
      slideAnim?.bind?.();
      slideContextMenu?.bind?.();
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
      setDeckAspect,
      cycleDeckAspect,
      toggleDeckOrientation,
      appendLink,
      appendNote,
      runAct: (act) => host.__EASYOFFICE_SLIDE_RUN_ACT__?.(act) ?? false,      isShowing: () => showing,
    };
  }

  host.__EASYOFFICE_CREATE_SLIDE_STUDIO__ = createSlideStudio;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
