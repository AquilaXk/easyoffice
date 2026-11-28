(function (host) {
  "use strict";

  // 서식 복사 of the sheet window (PD-400): the brush of the toolbar, 서식 > 서식 복사 / 서식 붙여넣기, and Ctrl+Shift+C / Ctrl+Shift+V
  // (⌘⇧C / ⌘⇧V). It has the structure of the document window's `doc-format-tools.js` (copy the format at the selection, apply it to
  // the next selection, Esc or a second press drops it) and takes the cells' side from the 선택하여 붙여넣기 > 서식 path: the
  // copied range is a block of the clipboard module (`copyFormats`) and the paste writes formats only (`pasteFormats`, which
  // repeats the block over a selection that is a whole multiple of it, carries the merges, and keeps the text of the target).
  // The system clipboard is not touched.
  // Reuse (구현 원칙 3단계): (1) no library; (2) `sheet-clipboard.js` (the block and the format paste) and `sheet-paste-core.js` (`planPaste`
  // with the format mode, whose source and license are named in that file's header); (3) only the arming of the next selection is new.

  const MESSAGES = {
    several: "여러 영역에서는 서식을 복사할 수 없습니다",
    none: "붙여넣을 서식이 없습니다",
    armed: "서식을 적용할 셀을 선택하세요",
    cancelled: "서식 복사를 취소했습니다",
    pasted: "서식 붙여넣기 완료",
  };

  function createSheetFormatPainter(deps) {
    const { document, getRange, getRanges, copyFormats, pasteFormats, commitIfEditing, focusAnchor, say } = deps;
    let copied = null;
    let armed = null;

    const buttons = () => document.querySelectorAll('.tb-btn[data-sheet-act="format-painter"]');
    const showArmed = (on) => buttons().forEach((el) => el.setAttribute("aria-pressed", on ? "true" : "false"));

    function copy() {
      commitIfEditing();
      if (getRanges().length > 1) {
        say(MESSAGES.several);
        return false;
      }
      copied = copyFormats(getRange());
      return true;
    }

    async function paste() {
      disarm();
      if (!copied) {
        say(MESSAGES.none);
        return false;
      }
      commitIfEditing();
      const done = await pasteFormats(copied, getRange());
      if (done) say(MESSAGES.pasted);
      focusAnchor();
      return done === true;
    }

    function disarm() {
      if (!armed) return;
      armed.viewport.removeEventListener("mouseup", armed.onMouseUp);
      document.removeEventListener("keydown", armed.onKey, true);
      armed = null;
      showArmed(false);
    }

    // The brush: copies the format at the selection and waits for the next selection (Esc or a second press drops it).
    function brush() {
      if (armed) {
        disarm();
        say(MESSAGES.cancelled);
        return Promise.resolve(false);
      }
      const viewport = document.getElementById("sheet-viewport");
      if (!viewport || !copy()) return Promise.resolve(false);
      const onMouseUp = () => void paste();
      const onKey = (ev) => {
        if (ev.key !== "Escape") return;
        disarm();
        say(MESSAGES.cancelled);
      };
      armed = { viewport, onMouseUp, onKey };
      viewport.addEventListener("mouseup", onMouseUp);
      document.addEventListener("keydown", onKey, true);
      showArmed(true);
      say(MESSAGES.armed);
      return Promise.resolve(true);
    }

    function bind() {
      for (const el of buttons()) {
        el.addEventListener("click", () => void brush());
      }
    }

    return {
      bind,
      actions: {
        "format-painter": brush,
        "format-paste": paste,
      },
      isArmed: () => armed !== null,
    };
  }

  host.__EASYOFFICE_CREATE_SHEET_FORMAT_PAINTER__ = createSheetFormatPainter;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
