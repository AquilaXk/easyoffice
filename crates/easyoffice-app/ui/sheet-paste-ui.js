(function (host) {
  "use strict";

  // The 골라 붙이기 dialog of the sheet window (PD-206): the part of the copied cells to
  // paste (모두, 값, 서식, 수식, 열 너비) and 행/열 바꿈. The menu items carry
  // `data-sheet-paste="special"`; Ctrl/Cmd+Alt+V opens the same dialog.
  function createSheetPasteUi(deps) {
    const { document, clip, getRange, commitIfEditing, kit } = deps;
    const { $, val, checked, setChecked, setSelect, openDialog, closeDialog } = kit;
    let ready = false;

    async function apply() {
      const options = { mode: val("sheet-paste-kind-select") || "all", transpose: checked("sheet-paste-transpose") };
      closeDialog("sheet-paste-dialog");
      await clip.pasteSpecial(getRange(), options).catch(() => {});
    }

    function open() {
      if (!ready) {
        ready = true;
        $("btn-sheet-paste-apply")?.addEventListener("click", () => void apply());
      }
      commitIfEditing();
      setSelect("sheet-paste-kind-select", "all");
      setChecked("sheet-paste-transpose", false);
      openDialog("sheet-paste-dialog");
    }

    for (const el of document.querySelectorAll("[data-sheet-paste]")) {
      el.addEventListener("click", (ev) => {
        ev.stopPropagation?.();
        host.__EASYOFFICE_CLOSE_MENUS__?.(document);
        if (el.getAttribute("aria-disabled") !== "true") open();
      });
    }

    return { open };
  }

  host.__EASYOFFICE_CREATE_SHEET_PASTE_UI__ = createSheetPasteUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
