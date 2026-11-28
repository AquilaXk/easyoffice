(function (host) {
  "use strict";

  // 슬라이드 쇼 > 쇼 설정 and 화면 전환 of the slide editor (PD-398). 쇼 설정 holds what belongs to the whole show: repeat until Esc
  // (`showLoop`) and the slide advance, by hand or by the set times (`manualAdvance`). 화면 전환 holds what belongs to one
  // slide: the effect, its side, 마우스를 클릭할 때 (`noClickAdvance` off) and 다음 시간 후 (`advanceAfterMs`). The effects and their
  // side go to `p:transition`; the show plays them with `slide-show-timing.js` (reveal.js's auto-slide and loop) and the
  // transition styles of `styles.css`.
  const AXIS_EFFECTS = new Set(["split", "randomBar"]);
  const NO_DIRECTION = new Set(["none", "fade", "zoom"]);
  const MAX_SECONDS = 86400;
  const SHOW_DIALOG = "slide-show-dialog";
  const TRANS_DIALOG = "slide-trans-dialog";

  function createSlideShowSetup(deps, canvas) {
    const { document, slideDeckState: deck, getActiveSlideIndex, getPanels, pushUndoSnapshot, noteActivity } = deps;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const codec = host.__EASYOFFICE_SLIDE_CANVAS_CODEC__;
    const $ = (id) => document.getElementById(id);
    const slide = () => deck[getActiveSlideIndex()];

    // ---- 쇼 설정 -----------------------------------------------------------------------------------

    function openShowSetup() {
      const first = deck[0] || {};
      kit.note("slide-show-status", "");
      kit.setChecked("slide-show-loop", first.showLoop === true);
      kit.setChecked("slide-show-manual", first.manualAdvance === true);
      kit.setChecked("slide-show-timed", first.manualAdvance !== true);
      kit.openDialog(SHOW_DIALOG);
      $("btn-slide-show-ok")?.focus?.();
    }

    function applyShowSetup() {
      const loop = kit.checked("slide-show-loop");
      const manual = kit.checked("slide-show-manual");
      kit.closeDialog(SHOW_DIALOG);
      pushUndoSnapshot(false);
      for (const s of deck) {
        s.showLoop = loop || undefined;
        s.manualAdvance = manual || undefined;
      }
      noteActivity();
    }

    // ---- 화면 전환 ---------------------------------------------------------------------------------

    // The side controls follow the effect: none for an effect without a side, a direction list, or the axis of a split.
    function syncDirection() {
      const effect = kit.val("slide-trans-effect");
      const axis = AXIS_EFFECTS.has(effect);
      if ($("slide-trans-dir")) $("slide-trans-dir").hidden = axis;
      if ($("slide-trans-axis")) $("slide-trans-axis").hidden = !axis;
      for (const id of ["slide-trans-dir", "slide-trans-axis"]) if ($(id)) $(id).disabled = NO_DIRECTION.has(effect);
    }

    function openTransition() {
      const s = slide();
      if (!s) return;
      kit.note("slide-trans-status", "");
      kit.setSelect("slide-trans-effect", s.transition || "none");
      kit.setSelect("slide-trans-dir", s.transitionDir || "left");
      kit.setSelect("slide-trans-axis", s.transitionDir === "top" || s.transitionDir === "bottom" ? "top" : "left");
      kit.setChecked("slide-trans-click", s.noClickAdvance !== true);
      kit.setChecked("slide-trans-timed", Number.isFinite(s.advanceAfterMs));
      kit.setVal("slide-trans-seconds", String((Number.isFinite(s.advanceAfterMs) ? s.advanceAfterMs : 5000) / 1000));
      syncDirection();
      kit.openDialog(TRANS_DIALOG);
      $("btn-slide-trans-ok")?.focus?.();
    }

    // The fields the dialog sets, as they go on a slide.
    function transitionFields() {
      const effect = kit.val("slide-trans-effect");
      const seconds = Math.min(MAX_SECONDS, Math.max(0, Number(kit.val("slide-trans-seconds")) || 0));
      const dir = AXIS_EFFECTS.has(effect) ? kit.val("slide-trans-axis") : kit.val("slide-trans-dir");
      return {
        transition: codec.normalizeTransition(effect),
        transitionDir: NO_DIRECTION.has(effect) ? undefined : dir,
        noClickAdvance: kit.checked("slide-trans-click") ? undefined : true,
        advanceAfterMs: kit.checked("slide-trans-timed") ? Math.round(seconds * 1000) : undefined,
      };
    }

    function applyTransition(all) {
      const fields = transitionFields();
      kit.closeDialog(TRANS_DIALOG);
      pushUndoSnapshot(false);
      for (const s of all ? deck : [slide()]) if (s) Object.assign(s, fields);
      getPanels().renderSlideDeck(getActiveSlideIndex());
      noteActivity();
    }

    const ACTS = { "show-setup": openShowSetup, "transition-dialog": openTransition };

    function run(act) {
      if (!Object.hasOwn(ACTS, act)) return false;
      ACTS[act]();
      return true;
    }

    function bind() {
      $("btn-slide-show-ok")?.addEventListener("click", applyShowSetup);
      $("btn-slide-trans-ok")?.addEventListener("click", () => applyTransition(false));
      $("btn-slide-trans-all")?.addEventListener("click", () => applyTransition(true));
      $("slide-trans-effect")?.addEventListener("change", syncDirection);
      for (const id of [SHOW_DIALOG, TRANS_DIALOG]) {
        $(id)?.querySelectorAll?.("[data-dialog-close]").forEach((btn) => btn.addEventListener("click", () => kit.closeDialog(id)));
      }
    }

    return { bind, run, closeAll: () => [SHOW_DIALOG, TRANS_DIALOG].forEach((id) => kit.closeDialog(id)), canvas };
  }

  host.__EASYOFFICE_CREATE_SLIDE_SHOW_SETUP__ = createSlideShowSetup;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
