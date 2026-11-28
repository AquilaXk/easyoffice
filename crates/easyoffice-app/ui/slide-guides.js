(function (host) {
  "use strict";

  // 보기 > 눈금자, 눈금선, 안내선 of the slide editor (PD-402). 눈금자 and 눈금선 only draw (a centimetre ruler along the top and
  // left edge of the stage, a centimetre grid over it). 안내선 are lines objects snap to: two lines through the center of the
  // slide when the check box is first ticked (as the presentation program places them), one more by dragging a line with Ctrl
  // held, a line moved by dragging it and dropped off the stage to delete it. They belong to the deck (`guides`, `showGuides` on
  // the first slide like the other deck settings of `slide-ext.js`) and are saved as `p:guideLst` of `ppt/viewProps.xml`
  // (`rich_slide_guides.rs`). The snap itself is `slide-snap.js`: the guides are extra targets of its smart guides.
  const MAX_GUIDES = 64;
  const MAX_POS_PT = 4000;
  const PT_PER_CM = 72 / 2.54;
  const round2 = (v) => Math.round(v * 100) / 100;

  // The guides of a deck as the wire and the file keep them: `{ orient: "vert" | "horz", posPt }`, at most 64, within the page cap.
  function normalizeGuides(raw) {
    if (!Array.isArray(raw)) return [];
    const out = [];
    for (const g of raw.slice(0, MAX_GUIDES)) {
      const pos = typeof g?.posPt === "number" ? g.posPt : NaN;
      if ((g?.orient !== "vert" && g?.orient !== "horz") || !Number.isFinite(pos) || Math.abs(pos) > MAX_POS_PT) continue;
      out.push({ orient: g.orient, posPt: Math.round(pos * 8) / 8 });
    }
    return out;
  }

  // The snap targets of the guides: the x of the vertical ones and the y of the horizontal ones.
  const targetsOf = (guides) => ({
    v: guides.filter((g) => g.orient === "vert").map((g) => g.posPt),
    h: guides.filter((g) => g.orient === "horz").map((g) => g.posPt),
  });

  function createSlideGuides(deps, canvas) {
    const { document, slideDeckState: deck, getActiveSlideIndex, getPanels, pushUndoSnapshot, noteActivity } = deps;
    const $ = (id) => document.getElementById(id);
    const state = { ruler: false, grid: false };
    const first = () => deck[0];
    const guides = () => normalizeGuides(first()?.guides);
    const shown = () => first()?.showGuides === true;
    const stageSize = () => canvas.stageSize();

    function setGuides(next, options = {}) {
      if (options.recordUndo !== false) pushUndoSnapshot(false);
      for (const s of deck) {
        s.guides = next.length > 0 ? next : undefined;
        s.showGuides = options.show ?? s.showGuides;
      }
      noteActivity?.();
      paint();
    }

    function layer() {
      let el = $("slide-guide-layer");
      if (!el) {
        const box = $("slide-stage-box");
        if (!box) return null;
        el = document.createElement("div");
        el.id = "slide-guide-layer";
        el.className = "slide-guide-layer";
        el.setAttribute("aria-hidden", "true");
        box.appendChild(el);
      }
      return el;
    }

    // Draws the ruler, the grid and the guides of the current stage.
    function paint() {
      const el = layer();
      if (!el) return;
      const stage = stageSize();
      el.replaceChildren();
      const step = { x: round2(((PT_PER_CM / stage.w) * 100)), y: round2(((PT_PER_CM / stage.h) * 100)) };
      if (state.grid) {
        const grid = document.createElement("div");
        grid.className = "slide-grid";
        grid.style.backgroundSize = step.x + "% " + step.y + "%";
        el.appendChild(grid);
      }
      if (state.ruler) {
        for (const axis of ["h", "v"]) {
          const bar = document.createElement("div");
          bar.className = "slide-ruler slide-ruler-" + axis;
          bar.style.backgroundSize = axis === "h" ? step.x + "% 100%" : "100% " + step.y + "%";
          el.appendChild(bar);
        }
      }
      if (shown()) {
        guides().forEach((g, i) => {
          const line = document.createElement("div");
          line.className = "slide-guide slide-guide-" + (g.orient === "vert" ? "v" : "h");
          line.dataset.guideIdx = String(i);
          if (g.orient === "vert") line.style.left = round2((g.posPt / stage.w) * 100) + "%";
          else line.style.top = round2((g.posPt / stage.h) * 100) + "%";
          el.appendChild(line);
        });
      }
      syncMenu();
    }

    function syncMenu() {
      const flags = { "view-ruler": state.ruler, "view-grid": state.grid, "view-guides": shown() };
      for (const [act, on] of Object.entries(flags)) {
        document.querySelectorAll(`[data-slide-act="${act}"]`).forEach((item) => item.setAttribute("aria-checked", on ? "true" : "false"));
      }
    }

    function toggleGuides() {
      const on = !shown();
      const list = guides();
      const stage = stageSize();
      // The first time, the two center lines appear.
      const next = on && list.length === 0 ? [{ orient: "vert", posPt: round2(stage.w / 2) }, { orient: "horz", posPt: round2(stage.h / 2) }] : list;
      setGuides(next, { show: on });
    }

    // Dragging a guide line: it follows the pointer, Ctrl leaves a copy behind, and a line dropped off the stage is deleted.
    function dragGuide(ev, idx) {
      const box = $("slide-stage-box");
      const rect = box?.getBoundingClientRect?.();
      if (!rect || rect.width <= 0 || rect.height <= 0) return;
      ev.preventDefault?.();
      ev.stopPropagation?.();
      const stage = stageSize();
      const list = guides();
      const start = list[idx];
      if (!start) return;
      const copy = ev.ctrlKey || ev.metaKey || ev.altKey;
      if (copy && list.length >= MAX_GUIDES) return;
      pushUndoSnapshot(false);
      const working = copy ? [...list, { ...start }] : [...list];
      const at = copy ? working.length - 1 : idx;
      const move = (e) => {
        const raw = start.orient === "vert" ? ((Number(e.clientX) - rect.left) / rect.width) * stage.w : ((Number(e.clientY) - rect.top) / rect.height) * stage.h;
        working[at] = { ...start, posPt: Math.round(raw * 8) / 8 };
        setGuides([...working], { recordUndo: false });
      };
      const up = () => {
        document.removeEventListener("mousemove", move);
        document.removeEventListener("mouseup", up);
        const final = working.filter((g) => inside(g, stage));
        setGuides(final, { recordUndo: false });
      };
      document.addEventListener("mousemove", move);
      document.addEventListener("mouseup", up);
    }

    const inside = (g, stage) => g.posPt >= 0 && g.posPt <= (g.orient === "vert" ? stage.w : stage.h);

    function run(act) {
      if (act === "view-ruler") state.ruler = !state.ruler;
      else if (act === "view-grid") state.grid = !state.grid;
      else if (act === "view-guides") {
        toggleGuides();
        return true;
      } else return false;
      paint();
      return true;
    }

    function bind() {
      $("slide-stage-box")?.addEventListener("mousedown", (ev) => {
        const line = ev.target?.closest?.(".slide-guide");
        if (!line || ev.button !== 0) return;
        dragGuide(ev, Number(line.dataset.guideIdx));
      }, true);
    }

    const api = { bind, run, closeAll: () => {}, onRender: paint, targets: () => (shown() ? targetsOf(guides()) : { v: [], h: [] }) };
    host.__EASYOFFICE_SLIDE_GUIDES__ = api;
    return api;
  }

  host.__EASYOFFICE_SLIDE_GUIDES_MATH__ = { normalizeGuides, targetsOf, MAX_GUIDES, MAX_POS_PT };
  host.__EASYOFFICE_CREATE_SLIDE_GUIDES__ = createSlideGuides;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
