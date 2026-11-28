(function (host) {
  "use strict";

  // Object formatting of the slide editor (PD-394): group and ungroup, rotate and flip, the "도형 서식"
  // dialog (fill, line, shadow, rotation) and the "단락" dialog (alignment, bullets and numbers, line and
  // paragraph spacing). Menu items reach it through `run(act)`; the layout follows the Korean office
  // suite's menus and dialogs, the controls come from the dialog kit.
  const groupApi = () => host.__EASYOFFICE_SLIDE_GROUP__;
  const FORMAT_KINDS = new Set(["textbox", "shape", "image"]);
  const TEXT_KINDS = new Set(["textbox", "shape"]);
  const ROTATE_STEP = 90;

  function createSlideFormat(deps, canvas) {
    const { document, pushUndoSnapshot } = deps;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const codec = host.__EASYOFFICE_SLIDE_CANVAS_CODEC__;
    const num = (id, fallback) => {
      const v = Number(kit.val(id));
      return Number.isFinite(v) && kit.val(id) !== "" ? v : fallback;
    };
    const say = (msg) => {
      const fb = document.getElementById("status-command-feedback");
      if (fb) fb.textContent = msg;
    };
    const selected = (kinds = FORMAT_KINDS) => canvas.getElements().filter((e) => canvas.getSelectedIds().includes(e.id) && kinds.has(e.kind));

    // ---- group, rotate, flip ---------------------------------------------------------------------

    function groupSelection() {
      const slide = canvas.currentSlide();
      const ids = canvas.getSelectedIds();
      if (ids.length < 2) return say("그룹으로 묶을 개체를 둘 이상 선택하세요");
      let gid = null;
      canvas.mutateWithUndo(() => {
        gid = groupApi().group(slide, ids);
        return gid !== null;
      });
      if (gid) canvas.setSelectedIds(groupApi().leavesOf(slide, gid));
      return gid;
    }

    function ungroupSelection() {
      const slide = canvas.currentSlide();
      const ids = canvas.getSelectedIds();
      let freed = [];
      canvas.mutateWithUndo(() => {
        freed = groupApi().ungroup(slide, ids);
        return freed.length > 0;
      });
      if (freed.length > 0) canvas.setSelectedIds(freed);
      return freed;
    }

    // Turns or flips everything selected about its common center, as one change.
    function transformSelection(change) {
      const els = selected();
      if (els.length === 0) return say("개체를 먼저 선택하세요");
      return canvas.mutateWithUndo(() => groupApi().transform(els, change));
    }

    // ---- 도형 서식 -------------------------------------------------------------------------------

    function openFormatShape() {
      const els = selected();
      const el = canvas.getSelectedElement();
      if (!el || els.length === 0) return say("개체를 먼저 선택하세요");
      const hex = (c, d) => "#" + codec.normalizeHex6(c, d).toLowerCase();
      const isShape = el.kind === "shape";
      const line = isShape ? { color: el.stroke, width: el.strokeWidthPt } : { color: el.outline?.color, width: el.outline?.widthPt };
      const hasLine = isShape ? Boolean(el.stroke) && el.strokeWidthPt > 0 : Boolean(el.outline);
      kit.note("slide-fmt-status", "");
      kit.setSelect("slide-fmt-fill-mode", el.fill ? "solid" : "none");
      kit.setVal("slide-fmt-fill", hex(el.fill, "2563EB"));
      kit.setSelect("slide-fmt-line-mode", hasLine ? el.dash || "solid" : "none");
      kit.setVal("slide-fmt-line", hex(line.color, "1D4ED8"));
      kit.setVal("slide-fmt-line-w", String(line.width ?? (isShape ? 2 : 0.75)));
      const sh = el.shadow;
      kit.setChecked("slide-fmt-shadow", Boolean(sh));
      kit.setVal("slide-fmt-shadow-color", hex(sh?.color, "000000"));
      kit.setVal("slide-fmt-shadow-alpha", String(sh?.alphaPct ?? 40));
      kit.setVal("slide-fmt-shadow-blur", String(sh?.blurPt ?? 4));
      kit.setVal("slide-fmt-shadow-angle", String(sh?.angleDeg ?? 45));
      kit.setVal("slide-fmt-shadow-dist", String(sh?.distPt ?? 3));
      kit.setVal("slide-fmt-rot", String(Math.round(el.rot || 0)));
      kit.setChecked("slide-fmt-flip-h", Boolean(el.flipH));
      kit.setChecked("slide-fmt-flip-v", Boolean(el.flipV));
      const section = document.getElementById("slide-fmt-fill-section");
      if (section) section.hidden = el.kind === "image";
      kit.openDialog("slide-fmt-dialog");
      document.getElementById("btn-slide-fmt-ok")?.focus?.();
    }

    function shapePatch(el) {
      const hex = (id, d) => codec.normalizeHex6(kit.val(id), d);
      const patch = {};
      if (TEXT_KINDS.has(el.kind)) patch.fill = kit.val("slide-fmt-fill-mode") === "none" ? "" : hex("slide-fmt-fill", "2563EB");
      const mode = kit.val("slide-fmt-line-mode");
      const width = Math.min(24, Math.max(0, num("slide-fmt-line-w", 0.75)));
      patch.dash = mode === "dash" || mode === "dot" ? mode : "solid";
      if (el.kind === "shape") {
        patch.stroke = mode === "none" ? "" : hex("slide-fmt-line", "1D4ED8");
        patch.strokeWidthPt = width;
      } else {
        patch.outline = mode === "none" ? undefined : { color: hex("slide-fmt-line", "000000"), widthPt: width };
      }
      patch.shadow = kit.checked("slide-fmt-shadow") ? {
        color: hex("slide-fmt-shadow-color", "000000"), alphaPct: num("slide-fmt-shadow-alpha", 40), blurPt: num("slide-fmt-shadow-blur", 4),
        angleDeg: num("slide-fmt-shadow-angle", 45), distPt: num("slide-fmt-shadow-dist", 3),
      } : undefined;
      patch.rot = num("slide-fmt-rot", 0);
      patch.flipH = kit.checked("slide-fmt-flip-h");
      patch.flipV = kit.checked("slide-fmt-flip-v");
      return patch;
    }

    function applyFormatShape() {
      const els = selected();
      kit.closeDialog("slide-fmt-dialog");
      if (els.length === 0) return;
      pushUndoSnapshot(false);
      for (const el of els) canvas.updateElement(el.id, shapePatch(el), { recordUndo: false });
    }

    // ---- 단락 ------------------------------------------------------------------------------------

    function openParagraph() {
      const els = selected(TEXT_KINDS);
      const el = els.find((e) => e.id === canvas.getSelectedElement()?.id) || els[0];
      if (!el) return say("텍스트 상자나 도형을 먼저 선택하세요");
      const para = el.para || {};
      kit.note("slide-para-status", "");
      kit.setSelect("slide-para-align", para.justify ? "justify" : el.align || "left");
      kit.setSelect("slide-para-bullet", para.bullet?.kind === "char" ? para.bullet.char : "");
      kit.setSelect("slide-para-number", para.bullet?.kind === "number" ? para.bullet.style : "");
      kit.setVal("slide-para-line", String((para.lineSpacingPct || 100) / 100));
      kit.setVal("slide-para-before", String(para.spaceBeforePt || 0));
      kit.setVal("slide-para-after", String(para.spaceAfterPt || 0));
      kit.openDialog("slide-para-dialog");
      document.getElementById("btn-slide-para-ok")?.focus?.();
    }

    function paragraphPatch() {
      const align = kit.val("slide-para-align");
      const number = kit.val("slide-para-number"), char = kit.val("slide-para-bullet");
      const lines = Math.min(5, Math.max(0.5, num("slide-para-line", 1)));
      const bullet = number ? { kind: "number", style: number } : char ? { kind: "char", char } : undefined;
      return {
        align: align === "justify" ? "left" : align,
        para: {
          justify: align === "justify", bullet, lineSpacingPct: Math.round(lines * 100),
          spaceBeforePt: Math.max(0, num("slide-para-before", 0)), spaceAfterPt: Math.max(0, num("slide-para-after", 0)),
        },
      };
    }

    function applyParagraph() {
      const els = selected(TEXT_KINDS);
      kit.closeDialog("slide-para-dialog");
      if (els.length === 0) return;
      const patch = paragraphPatch();
      pushUndoSnapshot(false);
      for (const el of els) canvas.updateElement(el.id, patch, { recordUndo: false });
    }

    // ---- menu entry ------------------------------------------------------------------------------

    const ACTS = {
      group: groupSelection,
      ungroup: ungroupSelection,
      "rotate-right": () => transformSelection({ angle: ROTATE_STEP }),
      "rotate-left": () => transformSelection({ angle: -ROTATE_STEP }),
      "flip-h": () => transformSelection({ flipH: true }),
      "flip-v": () => transformSelection({ flipV: true }),
      "format-shape": openFormatShape,
      "format-para": openParagraph,
    };

    // Runs a menu act of this module; false when the act is not one of its own.
    function run(act) {
      const fn = Object.hasOwn(ACTS, act) ? ACTS[act] : null;
      if (!fn) return false;
      fn();
      return true;
    }

    function bind() {
      kit.bindColors(["slide-fmt-fill", "slide-fmt-line", "slide-fmt-shadow-color"]);
      document.getElementById("btn-slide-fmt-ok")?.addEventListener("click", applyFormatShape);
      document.getElementById("btn-slide-para-ok")?.addEventListener("click", applyParagraph);
      for (const id of ["slide-fmt-dialog", "slide-para-dialog"]) {
        document.getElementById(id)?.querySelectorAll?.("[data-dialog-close]").forEach((btn) => btn.addEventListener("click", () => kit.closeDialog(id)));
      }
    }

    return { bind, run, groupSelection, ungroupSelection, transformSelection, openFormatShape, openParagraph, closeAll: () => ["slide-fmt-dialog", "slide-para-dialog"].forEach((id) => kit.closeDialog(id)) };
  }

  host.__EASYOFFICE_CREATE_SLIDE_FORMAT__ = createSlideFormat;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
