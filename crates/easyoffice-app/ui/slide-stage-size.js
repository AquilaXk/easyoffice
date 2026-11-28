(function (host) {
  "use strict";

  // The page size of a slide deck (PD-402). The three presets travel by name ("16:9", "4:3", "A4"); any other size travels as
  // `custom:<cx>x<cy>` in EMU with the long side first, the string `SlideAspect::to_wire` writes (`rich_slide_aspect.rs`). The
  // range is the engine's: 1 inch (914400 EMU) up to 4000 pt (50800000 EMU); a string outside it is not a size, never clamped.
  // 참조 구현: the sizes are the file engine's, whose table follows PptxGenJS `src/pptxgen.ts` 315-324 (MIT, THIRD_PARTY_NOTICES.md).
  const STAGE_SIZES = Object.freeze({
    "16:9": Object.freeze({ w: 960, h: 540 }),
    "4:3": Object.freeze({ w: 720, h: 540 }),
    A4: Object.freeze({ w: 841.89, h: 595.28 }),
  });
  const EMU_PER_PT = 12700;
  const EMU_PER_CM = 360000;
  const MIN_SIZE_EMU = 914400;
  const MAX_SIZE_EMU = 50800000;

  const customSizeOf = (aspect) => {
    const m = /^custom:(\d{1,9})x(\d{1,9})$/.exec(typeof aspect === "string" ? aspect : "");
    if (!m) return null;
    const cx = Number(m[1]);
    const cy = Number(m[2]);
    const ok = (v) => v >= MIN_SIZE_EMU && v <= MAX_SIZE_EMU;
    return ok(cx) && ok(cy) ? { w: cx / EMU_PER_PT, h: cy / EMU_PER_PT } : null;
  };
  const isKnownAspect = (aspect) => Object.hasOwn(STAGE_SIZES, aspect) || customSizeOf(aspect) !== null;
  const stageSizeOf = (aspect, orientation = "landscape") => {
    const base = (Object.hasOwn(STAGE_SIZES, aspect) ? STAGE_SIZES[aspect] : customSizeOf(aspect)) || STAGE_SIZES["16:9"];
    return orientation === "portrait" ? { w: base.h, h: base.w } : base;
  };

  // The wire string of a page of `w` x `h` centimetres (what the size dialog asks), or null outside the range. The long side
  // goes first as the engine keeps it; a portrait page is the same string with the orientation turned.
  function customWire(wCm, hCm) {
    const a = Math.round(Number(wCm) * EMU_PER_CM);
    const b = Math.round(Number(hCm) * EMU_PER_CM);
    if (!Number.isFinite(a) || !Number.isFinite(b)) return null;
    const wire = "custom:" + Math.max(a, b) + "x" + Math.min(a, b);
    return customSizeOf(wire) ? wire : null;
  }
  // The preset a size equals (within 0.6 pt), else its custom wire; `null` outside the range.
  function wireOfSize(wCm, hCm) {
    const wPt = (Number(wCm) * EMU_PER_CM) / EMU_PER_PT;
    const hPt = (Number(hCm) * EMU_PER_CM) / EMU_PER_PT;
    const long = Math.max(wPt, hPt);
    const short = Math.min(wPt, hPt);
    for (const [name, s] of Object.entries(STAGE_SIZES)) {
      if (Math.abs(long - s.w) < 0.6 && Math.abs(short - s.h) < 0.6) return name;
    }
    return customWire(wCm, hCm);
  }

  // The stage element of an aspect. The presets size it from the `data-aspect` rules of the style sheets; a custom size has no
  // rule, so its ratio goes inline (and leaves again when the deck returns to a preset).
  function applyStageSize(el, aspect, orientation = "landscape") {
    el.dataset.aspect = aspect;
    el.dataset.orientation = orientation;
    if (customSizeOf(aspect)) {
      const { w, h } = stageSizeOf(aspect, orientation);
      el.style.setProperty("--slide-ratio", String(Math.round((w / h) * 10000) / 10000));
      el.style.aspectRatio = w + " / " + h;
    } else {
      el.style.removeProperty("--slide-ratio");
      el.style.removeProperty("aspect-ratio");
    }
  }

  host.__EASYOFFICE_SLIDE_STAGE_SIZE__ = Object.freeze({
    STAGE_SIZES, MIN_SIZE_EMU, MAX_SIZE_EMU, EMU_PER_CM, customSizeOf, isKnownAspect, stageSizeOf, customWire, wireOfSize, applyStageSize,
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
