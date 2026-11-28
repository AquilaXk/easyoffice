(function (host) {
  "use strict";

  // 서식 > 글꼴 of the slide editor (PD-398): the word processor's 글꼴 dialog (`doc-font-dialog.js`, 한글 글꼴, 영어 글꼴,
  // 글꼴 스타일, 크기, 글꼴 색, 밑줄) opened on the selected text boxes and shapes. The dialog works on a "runs" object with
  // `getState()` (what the selection has, `"mixed"` where the objects differ) and `applyPatch(patch)` (only what the user
  // changed); this file is that object for slide objects. A slide text object has one run, so the font, the size, the color,
  // bold, italic and underline of the whole object are the properties (`font`, `eastFont`, `fontSizePt`, `color`, `bold`,
  // `italic`, `underline`). The effects, the 고급 tab and the underline styles other than 한 줄 stay off (`limited`).
  const TEXT_KINDS = new Set(["textbox", "shape"]);
  const MIXED = "mixed";
  const HALF_PT = 2;
  const DEFAULT_COLOR = { textbox: "1F2937", shape: "FFFFFF" };

  // The value all of `els` share for `read`, or MIXED.
  const shared = (els, read) => {
    const first = read(els[0]);
    return els.every((e) => read(e) === first) ? first : MIXED;
  };

  // What `getState()` hands the dialog for the selected objects.
  function stateOf(els, codec) {
    const color = (e) => codec.normalizeHex6(e.color, DEFAULT_COLOR[e.kind] || "1F2937");
    return {
      font: shared(els, (e) => e.font || ""),
      eastfont: shared(els, (e) => e.eastFont || ""),
      bold: shared(els, (e) => Boolean(e.bold)),
      italic: shared(els, (e) => Boolean(e.italic)),
      underline: shared(els, (e) => Boolean(e.underline)),
      size: shared(els, (e) => Math.round((e.fontSizePt || 18) * HALF_PT)),
      color: shared(els, color),
    };
  }

  // The element patch of one dialog patch (`null` clears a property).
  function elementPatch(patch) {
    const out = {};
    if ("font" in patch) out.font = patch.font || "";
    if ("eastfont" in patch) out.eastFont = patch.eastfont || "";
    if ("bold" in patch) out.bold = patch.bold === true;
    if ("italic" in patch) out.italic = patch.italic === true;
    if ("underline" in patch) out.underline = patch.underline === true;
    if (Number.isInteger(patch.size)) out.fontSizePt = patch.size / HALF_PT;
    if ("color" in patch) out.color = patch.color || "";
    return out;
  }

  function createSlideFont(deps, canvas) {
    const { document, pushUndoSnapshot } = deps;
    const codec = host.__EASYOFFICE_SLIDE_CANVAS_CODEC__;

    function selectedTexts() {
      const ids = canvas.getSelectedIds();
      return canvas.getElements().filter((e) => ids.includes(e.id) && TEXT_KINDS.has(e.kind));
    }

    function openDialog() {
      const els = selectedTexts();
      const fb = document.getElementById("status-command-feedback");
      if (els.length === 0) {
        if (fb) fb.textContent = "텍스트 상자나 도형을 먼저 선택하세요";
        return;
      }
      const runs = {
        getState: () => stateOf(els, codec),
        applyPatch: (patch) => {
          pushUndoSnapshot(false);
          els.forEach((el) => {
            const own = elementPatch(patch);
            // "자동" is the default color of the kind.
            if (own.color === "") own.color = DEFAULT_COLOR[el.kind];
            canvas.updateElement(el.id, own, { recordUndo: false });
          });
          return { changed: true };
        },
      };
      host.__EASYOFFICE_DOC_FONT_DIALOG__.open(runs, document, { limited: true });
    }

    function run(act) {
      if (act !== "font-dialog") return false;
      openDialog();
      return true;
    }

    return { run, bind: () => {}, closeAll: () => document.getElementById("doc-font-dialog")?.close?.(), openDialog };
  }

  host.__EASYOFFICE_CREATE_SLIDE_FONT__ = createSlideFont;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
