// 참조 구현 (reference implementation): reveal.js (MIT, Copyright (C) 2011-2025 Hakim El Hattab, https://github.com/hakimel/reveal.js),
// commit e240e175232fa2de660a02bc0d7bf5f655b159bb, `js/controllers/overview.js` lines 98-135 (`layout`, `update`: every slide is laid out on a
// grid of equal cells and scaled by `max(vmin / 5, 150) / vmin`) and 230-253 (`onSlideClicked`: a click on a slide leaves the overview
// and shows that slide). See `THIRD_PARTY_NOTICES.md`.

(function (host) {
  "use strict";

  // 보기 > 슬라이드 정렬 of the slide editor (PD-398): every slide as a thumbnail on a grid, in the order of the deck. A click selects a
  // slide, a double click or Enter shows it in the 기본 view, a drag moves it to the place of another one (the studio's
  // `moveSlide`, the same path as the thumbnail list), the arrow keys step through the grid, Esc leaves.
  // Reuse (구현 원칙 3단계): (1) the grid of an overview is a few lines of CSS, a component library adds nothing; (2) the show's frame
  // builder draws the thumbnails (`buildFrame`), the studio's `moveSlide` moves them; (3) from reveal.js the cell scale (`scale` below)
  // and the "click leaves the overview and shows the slide" behaviour are ported. reveal.js lays the slides on one row; the editor
  // wraps them into rows of as many cells as fit.
  const MIN_THUMB_PX = 150;
  const THUMB_VMIN_DIVISOR = 5;
  const GAP_PX = 16;

  // The cell width of reveal.js's overview: a fifth of the smaller window side, at least 150 px.
  function thumbWidth(vmin) {
    return Math.max(vmin / THUMB_VMIN_DIVISOR, MIN_THUMB_PX);
  }

  // Columns that fit `width` px (at least one).
  function columnsFor(width, cell) {
    return Math.max(1, Math.floor((width + GAP_PX) / (cell + GAP_PX)));
  }

  // The index the arrow key `key` leads to from `from` on a grid of `cols` columns (`null` for another key).
  function stepTarget(key, from, cols, count) {
    const step = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -cols, ArrowDown: cols }[key];
    if (step === undefined) return key === "Home" ? 0 : key === "End" ? count - 1 : null;
    return Math.max(0, Math.min(count - 1, from + step));
  }

  function createSlideOverview(deps, canvas, ops) {
    const { document, slideDeckState: deck, getActiveSlideIndex, getPanels } = deps;
    const env = host;
    const $ = (id) => document.getElementById(id);
    let dragFrom = -1;
    let cols = 1;

    const isActive = () => $("slide-sorter") !== null && !$("slide-sorter").hidden;

    // The cells drawn so far, in deck order: `{ slide, sig, cell }`. A thumbnail is drawn once and again only when its slide is a
    // different object or its content changed; moving the selection, or moving a slide, only updates marks and numbers.
    let entries = [];
    let selectionOnly = false;

    // What a thumbnail shows of a slide: when this text is the same the frame is the same.
    // A slide number in the footer makes the position part of it.
    const sigOf = (s, i) => JSON.stringify([s.title, s.lower, s.hidden, s.layout, s.elements, s.background, s.footer, s.insertedNodes, s.titleStyle, s.footer?.number ? i : 0]);

    function buildCell(slide, idx) {
      const cell = document.createElement("button");
      cell.type = "button";
      cell.draggable = true;
      const thumb = document.createElement("span");
      thumb.className = "slide-sorter-thumb";
      host.__EASYOFFICE_SLIDE_STAGE_SIZE__.applyStageSize(thumb, ops.deckAspect(), ops.deckOrientation());
      thumb.appendChild(ops.buildFrame(slide, idx));
      const num = document.createElement("span");
      num.className = "slide-sorter-num";
      cell.appendChild(thumb);
      cell.appendChild(num);
      return cell;
    }

    // Number, label and selection mark of one cell.
    function mark({ slide, cell }, idx, active) {
      cell.className = "slide-sorter-cell" + (active ? " active" : "") + (slide.hidden ? " is-hidden-slide" : "");
      cell.dataset.slideIdx = String(idx);
      cell.setAttribute("aria-label", idx + 1 + "번 슬라이드" + (slide.hidden ? "(숨김)" : "") + ": " + (String(slide.title || "").trim() || "(제목 없음)"));
      if (active) cell.setAttribute("aria-current", "true");
      else cell.removeAttribute("aria-current");
      const num = cell.querySelector(".slide-sorter-num");
      if (num) num.textContent = String(idx + 1);
    }

    // Brings the grid up to date. `full` also draws the frames of the changed slides and follows the window width.
    function render(full = true) {
      const sorter = $("slide-sorter");
      if (!sorter) return;
      const active = getActiveSlideIndex();
      if (full) {
        const vmin = Math.min(env.innerWidth || 1280, env.innerHeight || 800);
        const width = Math.round(thumbWidth(vmin));
        sorter.style.setProperty("--slide-sorter-cell", width + "px");
        cols = columnsFor(sorter.clientWidth || env.innerWidth || 1280, width);
        const before = new Map(entries.map((e) => [e.slide, e]));
        entries = deck.map((s, i) => {
          const sig = sigOf(s, i);
          const kept = before.get(s);
          return kept && kept.sig === sig ? kept : { slide: s, sig, cell: buildCell(s, i) };
        });
        if (entries.length !== sorter.children.length || entries.some((e, i) => sorter.children[i] !== e.cell)) {
          sorter.replaceChildren(...entries.map((e) => e.cell));
        }
      }
      entries.forEach((e, i) => mark(e, i, i === active));
    }

    function focusCell(idx) {
      $("slide-sorter")?.querySelector('[data-slide-idx="' + idx + '"]')?.focus?.();
    }

    function select(idx) {
      if (!deck[idx]) return;
      selectionOnly = true;
      getPanels().switchToSlideIndex(idx);
      selectionOnly = false;
      focusCell(idx);
    }

    function open() {
      if (isActive()) return;
      getPanels().syncActiveSlideDomStylesToDeck();
      $("slide-sorter").hidden = false;
      $("slide-workspace").hidden = true;
      render();
      focusCell(getActiveSlideIndex());
      const fb = $("status-command-feedback");
      if (fb) fb.textContent = "슬라이드 정렬 보기";
    }

    function close() {
      if (!isActive()) return;
      $("slide-sorter").hidden = true;
      $("slide-workspace").hidden = false;
      $("slide-sorter").replaceChildren();
      entries = [];
    }

    function cellOf(ev) {
      const cell = ev.target?.closest?.(".slide-sorter-cell");
      return cell ? Number(cell.dataset.slideIdx) : -1;
    }

    function onKey(ev) {
      if (!isActive() || ev.ctrlKey || ev.metaKey || ev.altKey) return;
      const from = cellOf(ev) >= 0 ? cellOf(ev) : getActiveSlideIndex();
      if (ev.key === "Escape") {
        ev.preventDefault();
        close();
      } else if (ev.key === "Enter") {
        ev.preventDefault();
        select(from);
        close();
      } else {
        const to = stepTarget(ev.key, from, cols, deck.length);
        if (to === null) return;
        ev.preventDefault();
        select(to);
      }
    }

    function bind() {
      const sorter = $("slide-sorter");
      if (!sorter) return;
      sorter.addEventListener("keydown", onKey);
      sorter.addEventListener("click", (ev) => {
        const idx = cellOf(ev);
        if (idx >= 0) select(idx);
      });
      // As reveal.js's `onSlideClicked`: the slide that is double clicked is shown.
      sorter.addEventListener("dblclick", (ev) => {
        const idx = cellOf(ev);
        if (idx < 0) return;
        select(idx);
        close();
      });
      sorter.addEventListener("dragstart", (ev) => {
        dragFrom = cellOf(ev);
        if (dragFrom < 0) return;
        if (ev.dataTransfer) {
          ev.dataTransfer.effectAllowed = "move";
          ev.dataTransfer.setData("text/plain", String(dragFrom));
        }
      });
      sorter.addEventListener("dragover", (ev) => {
        if (dragFrom >= 0 && cellOf(ev) >= 0) ev.preventDefault();
      });
      sorter.addEventListener("drop", (ev) => {
        const to = cellOf(ev);
        const from = dragFrom;
        dragFrom = -1;
        if (from < 0 || to < 0) return;
        ev.preventDefault();
        ops.moveSlide(from, to);
        focusCell(to);
      });
      sorter.addEventListener("dragend", () => {
        dragFrom = -1;
      });
      env.addEventListener?.("resize", () => {
        if (isActive()) render();
      });
    }

    const ACTS = { "view-sorter": open, "view-normal": close };
    function run(act) {
      if (!Object.hasOwn(ACTS, act)) return false;
      ACTS[act]();
      return true;
    }

    // Every slide render (switch, move, undo) draws the grid again while it is shown.
    return { bind, run, closeAll: close, isActive, render, onRender: () => { if (isActive()) render(!selectionOnly); } };
  }

  host.__EASYOFFICE_SLIDE_OVERVIEW_MATH__ = { thumbWidth, columnsFor, stepTarget };
  host.__EASYOFFICE_CREATE_SLIDE_OVERVIEW__ = createSlideOverview;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
