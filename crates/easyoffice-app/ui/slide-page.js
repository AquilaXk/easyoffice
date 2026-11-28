(function (host) {
  "use strict";

  // Slide-level formatting of the slide editor (PD-394): hide or show a slide, the "배경 서식" dialog
  // (solid, gradient or picture fill for one slide or all of them) and the "머리글 및 바닥글" dialog
  // (date, slide number and footer text for one slide or all of them). Every change is one undo step
  // on the slide objects in `slideDeckState`.
  const MAX_PICTURE_BYTES = 32 * 1024 * 1024;
  const DIALOGS = ["slide-bg-dialog", "slide-footer-dialog"];

  function createSlidePage(deps, canvas) {
    const { document, slideDeckState: deck, getActiveSlideIndex, getPanels, pushUndoSnapshot, noteActivity } = deps;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const codec = host.__EASYOFFICE_SLIDE_CANVAS_CODEC__;
    const $ = (id) => document.getElementById(id);
    const say = (msg) => {
      const fb = $("status-command-feedback");
      if (fb) fb.textContent = msg;
    };
    let pictureBackground = null;

    // Changes the slides at `indexes` as one undo step, then draws the deck again.
    function change(indexes, apply) {
      pushUndoSnapshot(false);
      for (const i of indexes) if (deck[i]) apply(deck[i], i);
      getPanels().renderSlideDeck(getActiveSlideIndex());
      noteActivity();
    }

    const allIndexes = () => deck.map((_, i) => i);

    function toggleHidden(idx = getActiveSlideIndex()) {
      if (!deck[idx]) return;
      const next = !deck[idx].hidden;
      change([idx], (slide) => { slide.hidden = next; });
      say(next ? "슬라이드를 숨겼습니다" : "슬라이드 숨기기를 해제했습니다");
    }

    // ---- 배경 서식 -------------------------------------------------------------------------------

    function showBackgroundRows(mode) {
      const rows = { "slide-bg-solid-row": "solid", "slide-bg-gradient-row": "gradient", "slide-bg-angle-row": "gradient", "slide-bg-picture-row": "picture" };
      for (const [id, forMode] of Object.entries(rows)) {
        const row = $(id);
        if (row) row.hidden = mode !== forMode;
      }
    }

    function openBackground() {
      const slide = deck[getActiveSlideIndex()];
      if (!slide) return;
      const bg = slide.background;
      const hex = (c, d) => "#" + codec.normalizeHex6(c, d).toLowerCase();
      pictureBackground = bg?.kind === "picture" ? bg : null;
      kit.note("slide-bg-status", "");
      kit.setSelect("slide-bg-mode", bg?.kind || "none");
      kit.setVal("slide-bg-color", hex(bg?.color, "FFFFFF"));
      kit.setVal("slide-bg-from", hex(bg?.from, "FFFFFF"));
      kit.setVal("slide-bg-to", hex(bg?.to, "CCCCCC"));
      kit.setVal("slide-bg-angle", String(bg?.angleDeg ?? 90));
      const name = $("slide-bg-file-name");
      if (name) name.textContent = pictureBackground ? "그림" : "";
      showBackgroundRows(kit.val("slide-bg-mode"));
      kit.openDialog("slide-bg-dialog");
      $("btn-slide-bg-ok")?.focus?.();
    }

    function chosenBackground() {
      const mode = kit.val("slide-bg-mode");
      const hex = (id, d) => codec.normalizeHex6(kit.val(id), d);
      if (mode === "solid") return { kind: "solid", color: hex("slide-bg-color", "FFFFFF") };
      if (mode === "gradient") {
        const angle = Math.round(Number(kit.val("slide-bg-angle")) || 0);
        return { kind: "gradient", from: hex("slide-bg-from", "FFFFFF"), to: hex("slide-bg-to", "CCCCCC"), angleDeg: ((angle % 360) + 360) % 360 };
      }
      return mode === "picture" ? pictureBackground : null;
    }

    function applyBackground(everySlide) {
      const bg = chosenBackground();
      if (kit.val("slide-bg-mode") === "picture" && !bg) {
        kit.note("slide-bg-status", "배경으로 쓸 그림 파일을 고르세요");
        return;
      }
      kit.closeDialog("slide-bg-dialog");
      change(everySlide ? allIndexes() : [getActiveSlideIndex()], (slide) => { slide.background = bg ? structuredClone(bg) : null; });
    }

    async function pickPicture(file) {
      const utils = host.__EASYOFFICE_SHEET_UTILS__;
      const fail = (msg) => kit.note("slide-bg-status", msg);
      const bytes = new Uint8Array(await file.arrayBuffer());
      const header = codec.parseImageHeader(bytes);
      if (bytes.length === 0 || bytes.length > MAX_PICTURE_BYTES) return fail("그림은 32 MiB까지 넣을 수 있습니다");
      if (!header || utils?.imageAreaProblem?.(header)) return fail("지원하지 않거나 손상된 그림 파일입니다");
      const jpeg = header.format === "jpg";
      const imageId = utils.registerDocImage(utils.bytesToBase64(bytes), header.format);
      pictureBackground = { kind: "picture", imageId, mime: jpeg ? "image/jpeg" : "image/png" };
      $("slide-bg-file-name").textContent = file.name.slice(0, 48);
      kit.note("slide-bg-status", "");
      return undefined;
    }

    // ---- 머리글 및 바닥글 ------------------------------------------------------------------------

    function openFooter() {
      const slide = deck[getActiveSlideIndex()];
      if (!slide) return;
      const f = slide.footer || {};
      kit.note("slide-footer-status", "");
      kit.setChecked("slide-footer-date", Boolean(f.date));
      kit.setSelect("slide-footer-date-kind", f.date?.kind || "auto");
      kit.setVal("slide-footer-date-text", f.date?.kind === "fixed" ? f.date.text : "");
      kit.setChecked("slide-footer-number", Boolean(f.number));
      kit.setChecked("slide-footer-text-on", typeof f.text === "string");
      kit.setVal("slide-footer-text", f.text || "");
      kit.setChecked("slide-footer-skip-title", false);
      kit.openDialog("slide-footer-dialog");
      $("btn-slide-footer-ok")?.focus?.();
    }

    function chosenFooter() {
      const out = {};
      if (kit.checked("slide-footer-date")) {
        const fixed = kit.val("slide-footer-date-kind") === "fixed" && kit.val("slide-footer-date-text") !== "";
        out.date = fixed ? { kind: "fixed", text: kit.val("slide-footer-date-text") } : { kind: "auto" };
      }
      if (kit.checked("slide-footer-number")) out.number = true;
      if (kit.checked("slide-footer-text-on") && kit.val("slide-footer-text") !== "") out.text = kit.val("slide-footer-text");
      return Object.keys(out).length > 0 ? out : null;
    }

    function applyFooter(everySlide) {
      const footer = chosenFooter();
      const skipTitle = kit.checked("slide-footer-skip-title");
      kit.closeDialog("slide-footer-dialog");
      const targets = everySlide ? allIndexes() : [getActiveSlideIndex()];
      change(targets, (slide) => {
        slide.footer = skipTitle && slide.layout === "titleSlide" ? null : footer && structuredClone(footer);
      });
    }

    // ---- menu entry ------------------------------------------------------------------------------

    const ACTS = { "hide-slide": () => toggleHidden(), "format-background": openBackground, "header-footer": openFooter };

    // Runs a menu act of this module; false when the act is not one of its own.
    function run(act) {
      const fn = Object.hasOwn(ACTS, act) ? ACTS[act] : null;
      if (!fn) return false;
      fn();
      return true;
    }

    // The checked state of the hide item follows the current slide.
    function refresh(idx) {
      const item = document.querySelector('#menu-bar [data-slide-act="hide-slide"]');
      if (!item) return;
      const hidden = Boolean(deck[idx]?.hidden);
      item.setAttribute("aria-checked", String(hidden));
      item.classList.toggle("active", hidden);
    }

    function bind() {
      kit.bindColors(["slide-bg-color", "slide-bg-from", "slide-bg-to"]);
      $("slide-bg-mode")?.addEventListener("change", () => showBackgroundRows(kit.val("slide-bg-mode")));
      $("btn-slide-bg-ok")?.addEventListener("click", () => applyBackground(false));
      $("btn-slide-bg-all")?.addEventListener("click", () => applyBackground(true));
      $("btn-slide-bg-file")?.addEventListener("click", () => $("slide-bg-file-input")?.click());
      const input = $("slide-bg-file-input");
      input?.addEventListener("change", () => {
        const file = input.files?.[0];
        input.value = "";
        if (file) void pickPicture(file).catch(() => {});
      });
      $("btn-slide-footer-ok")?.addEventListener("click", () => applyFooter(false));
      $("btn-slide-footer-all")?.addEventListener("click", () => applyFooter(true));
      for (const id of DIALOGS) $(id)?.querySelectorAll?.("[data-dialog-close]").forEach((btn) => btn.addEventListener("click", () => kit.closeDialog(id)));
    }

    return { bind, run, refresh, toggleHidden, openBackground, openFooter, closeAll: () => DIALOGS.forEach((id) => kit.closeDialog(id)) };
  }

  host.__EASYOFFICE_CREATE_SLIDE_PAGE__ = createSlidePage;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
