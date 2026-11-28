(function (host) {
  "use strict";

  // 디자인 > 슬라이드 크기 of the slide editor (PD-402): 표준(4:3), 와이드스크린(16:9) and 사용자 지정 슬라이드 크기…, a dialog with the
  // size list, 너비, 높이 (centimetres, the unit of the Korean presentation program) and 방향 (가로, 세로). Width and height are the
  // page as shown, so a portrait page has the larger height; changing the orientation swaps them. A size equal to a preset
  // (within 0.6 pt) is saved as that preset, any other size as `custom:<cx>x<cy>` (`slide-stage-size.js`); a size outside 1 inch
  // up to 56 inches is refused with a message and nothing changes (the file engine rejects the same range).
  const DIALOG = "slide-size-dialog";
  const PRESET_ITEMS = Object.freeze([["16:9", "와이드스크린(16:9)"], ["4:3", "표준(4:3)"], ["A4", "A4 용지(210x297 mm)"], ["custom", "사용자 지정"]]);
  const PRESET_ACTS = Object.freeze({ "slide-size-4x3": "4:3", "slide-size-16x9": "16:9" });
  const fmt = (v) => String(Math.round(v * 100) / 100);

  function createSlideSize(deps, canvas, ops) {
    const { document } = deps;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const size = host.__EASYOFFICE_SLIDE_STAGE_SIZE__;
    const $ = (id) => document.getElementById(id);
    const cmOf = (pt) => (pt * 12700) / size.EMU_PER_CM;
    const limitsCm = () => [size.MIN_SIZE_EMU / size.EMU_PER_CM, size.MAX_SIZE_EMU / size.EMU_PER_CM];

    function show(aspect, orientation) {
      const stage = size.stageSizeOf(aspect, orientation);
      kit.setVal("slide-size-w", fmt(cmOf(stage.w)));
      kit.setVal("slide-size-h", fmt(cmOf(stage.h)));
      kit.setChecked("slide-size-landscape", orientation !== "portrait");
      kit.setChecked("slide-size-portrait", orientation === "portrait");
    }

    function open() {
      const aspect = ops.deckAspect();
      const orientation = ops.deckOrientation();
      kit.fillOptions("slide-size-kind", PRESET_ITEMS);
      kit.setSelect("slide-size-kind", size.customSizeOf(aspect) ? "custom" : aspect);
      kit.note("slide-size-status", "");
      show(aspect, orientation);
      kit.openDialog(DIALOG);
      $("btn-slide-size-ok")?.focus?.();
    }

    const orientationNow = () => (kit.checked("slide-size-portrait") ? "portrait" : "landscape");

    // The list picks a preset: its width and height fill the fields in the chosen orientation.
    function onKind() {
      const kind = kit.val("slide-size-kind");
      if (kind !== "custom") show(kind, orientationNow());
    }

    // The orientation swaps width and height, as the presentation program does.
    function onOrientation() {
      const w = kit.val("slide-size-w");
      kit.setVal("slide-size-w", kit.val("slide-size-h"));
      kit.setVal("slide-size-h", w);
      kit.setSelect("slide-size-kind", "custom");
      onFields();
    }

    // Typing a size makes the list say 사용자 지정 (or the preset the numbers equal), and sets the orientation radios.
    function onFields() {
      const w = Number(kit.val("slide-size-w"));
      const h = Number(kit.val("slide-size-h"));
      if (!(w > 0) || !(h > 0)) return;
      kit.setChecked("slide-size-portrait", h > w);
      kit.setChecked("slide-size-landscape", h <= w);
      const wire = size.wireOfSize(w, h);
      kit.setSelect("slide-size-kind", wire && !wire.startsWith("custom:") ? wire : "custom");
    }

    function apply() {
      const w = Number(kit.val("slide-size-w"));
      const h = Number(kit.val("slide-size-h"));
      const wire = Number.isFinite(w) && Number.isFinite(h) && w > 0 && h > 0 ? size.wireOfSize(w, h) : null;
      if (!wire) {
        const [lo, hi] = limitsCm();
        kit.note("slide-size-status", "너비와 높이는 " + fmt(lo) + " cm에서 " + fmt(hi) + " cm 사이여야 합니다");
        return;
      }
      kit.closeDialog(DIALOG);
      ops.setDeckSize(wire, h > w ? "portrait" : "landscape");
    }

    function run(act) {
      if (act === "slide-size") {
        open();
        return true;
      }
      if (Object.hasOwn(PRESET_ACTS, act)) {
        ops.setDeckSize(PRESET_ACTS[act], ops.deckOrientation());
        return true;
      }
      return false;
    }

    function bind() {
      $("btn-slide-size-ok")?.addEventListener("click", apply);
      $("slide-size-kind")?.addEventListener("change", onKind);
      for (const id of ["slide-size-landscape", "slide-size-portrait"]) $(id)?.addEventListener("change", onOrientation);
      for (const id of ["slide-size-w", "slide-size-h"]) $(id)?.addEventListener("input", onFields);
      $(DIALOG)?.querySelectorAll?.("[data-dialog-close]").forEach((btn) => btn.addEventListener("click", () => kit.closeDialog(DIALOG)));
    }

    return { bind, run, closeAll: () => kit.closeDialog(DIALOG) };
  }

  host.__EASYOFFICE_CREATE_SLIDE_SIZE__ = createSlideSize;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
