// 참조 구현 (reference implementation): the document window's picture crop of this repository (`doc-word-image.js` `dragCrop`, PD-395):
// the unit (thousandths of a percent of the whole image per side), the limits (each side up to 99000, at least 1% left) and the
// clamp-then-fix-the-other-side rule are taken from it. The drag is rewritten for a slide object, whose frame shrinks with the crop
// so that the visible part of the picture stays where it is (the behavior of the presentation program's 자르기).

(function (host) {
  "use strict";

  // 서식 > 자르기 of the slide editor (PD-402): the picture is cut from its edges. While 자르기 is on, the side handles of the selected
  // picture move its edges into the picture (cutting it) or back out (restoring it); the corner handles do two sides. The crop is the
  // element's `crop`; the file form is `a:srcRect` (`pptx_rich.rs`).
  const MAX_SIDE = host.__EASYOFFICE_SLIDE_FX__.MAX_CROP_SIDE;
  const MIN_FRAME_PT = 24;
  const UNIT = 100000;
  const ZERO = Object.freeze({ left: 0, top: 0, right: 0, bottom: 0 });
  const round2 = (v) => Math.round(v * 100) / 100;

  // The frame and crop of picture `el` after its edge `dir` ("n", "e", "se", ...) is dragged by (`dxPt`, `dyPt`).
  function cropDrag(el, dir, dxPt, dyPt) {
    const crop = { ...ZERO, ...(el.crop || {}) };
    const out = { x: el.x, y: el.y, w: el.w, h: el.h, crop };
    // The whole image, in points: the frame is what is left of it after the crop.
    const fullW = el.w / (1 - (crop.left + crop.right) / UNIT);
    const fullH = el.h / (1 - (crop.top + crop.bottom) / UNIT);
    // The leading edge (left, top) moving in by `d` cuts `d` off the start of the frame; the trailing edge (right, bottom) moving
    // out by `d` gives back what the crop of that side had cut. Returns the points the frame start and size change by.
    const edge = (side, other, full, d, size, leading) => {
      const want = crop[side] + ((leading ? d : -d) * UNIT) / full;
      let moved = ((Math.max(0, Math.min(MAX_SIDE - crop[other], want)) - crop[side]) * full) / UNIT;
      moved = Math.min(moved, size - MIN_FRAME_PT);
      crop[side] = Math.round(crop[side] + moved * UNIT / full);
      return leading ? moved : -moved;
    };
    if (dir.includes("w")) {
      const d = edge("left", "right", fullW, dxPt, out.w, true);
      out.x = round2(out.x + d);
      out.w = round2(out.w - d);
    }
    if (dir.includes("e")) out.w = round2(out.w + edge("right", "left", fullW, dxPt, out.w, false));
    if (dir.includes("n")) {
      const d = edge("top", "bottom", fullH, dyPt, out.h, true);
      out.y = round2(out.y + d);
      out.h = round2(out.h - d);
    }
    if (dir.includes("s")) out.h = round2(out.h + edge("bottom", "top", fullH, dyPt, out.h, false));
    return out;
  }

  function createSlideCrop(deps, canvas) {
    const { document } = deps;
    let on = false;
    const say = (msg) => {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = msg;
    };
    const layer = () => document.getElementById("slide-canvas-layer");
    const selectedPicture = () => {
      const el = canvas.getSelectedElement();
      return el?.kind === "image" ? el : null;
    };

    function toggle() {
      if (!on && !selectedPicture()) {
        say("그림을 먼저 선택하세요");
        return;
      }
      on = !on;
      layer()?.classList.toggle("is-cropping", on);
      say(on ? "자르기: 가장자리 핸들을 끌어 자릅니다" : "자르기를 마쳤습니다");
    }

    function startDrag(ev, el, dir) {
      ev.preventDefault?.();
      ev.stopPropagation?.();
      const gesture = canvas.beginGesture();
      const x0 = Number(ev.clientX) || 0, y0 = Number(ev.clientY) || 0;
      const rect = ev.target?.closest?.(".slide-el")?.getBoundingClientRect?.();
      // Screen pixels to stage points: the card is `el.w` points wide.
      const scale = rect && rect.width > 0 ? el.w / rect.width : 1;
      const base = { ...el, crop: el.crop ? { ...el.crop } : undefined };
      let changed = false;
      const onMove = (e) => {
        const next = cropDrag(base, dir, ((Number(e.clientX) || 0) - x0) * scale, ((Number(e.clientY) || 0) - y0) * scale);
        canvas.updateElement(el.id, next, { recordUndo: false });
        changed = true;
      };
      const onUp = () => {
        document.removeEventListener("mousemove", onMove);
        document.removeEventListener("mouseup", onUp);
        if (changed) gesture.commit();
      };
      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", onUp);
    }

    function bind() {
      layer()?.addEventListener("mousedown", (ev) => {
        if (!on || ev.button !== 0) return;
        const handle = ev.target?.closest?.(".slide-el-handle");
        const el = selectedPicture();
        if (!handle || !el || handle.closest(".slide-el")?.dataset.elementId !== el.id) return;
        startDrag(ev, el, handle.dataset.dir || "se");
      }, true);
    }

    function run(act) {
      if (act !== "image-crop") return false;
      toggle();
      return true;
    }

    return { bind, run, closeAll: () => { on = false; layer()?.classList.remove("is-cropping"); }, isOn: () => on };
  }

  host.__EASYOFFICE_SLIDE_CROP_MATH__ = { cropDrag };
  host.__EASYOFFICE_CREATE_SLIDE_CROP__ = createSlideCrop;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
