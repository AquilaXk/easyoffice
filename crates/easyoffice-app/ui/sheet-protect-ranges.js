(function (host) {
  "use strict";

  // 범위 편집 허용 of the sheet window (PD-216): the ranges of a sheet that may be edited while the sheet is protected.
  // The names are those of the Korean spreadsheet help (범위 편집 허용: the dialog lists the ranges by
  // 제목 and 셀 범위 and has 새로 만들기, 수정 and 삭제; a new range takes 제목, 셀 범위 and 범위 암호; a cell of a range with a
  // password asks it in the 범위 잠금 해제 dialog when it is edited). The ranges can be set only while the sheet is not
  // protected. A range without a password may be edited by anyone; with one, whoever types it, until the window closes
  // or the file is opened again. The ranges are the `ranges` of the sheet's `protect` JSON (`sheet_protect_json.rs`);
  // each change is one undo step (`sheet-protect-ui.js`).

  const MAX_RANGES = 1000;
  const MAX_TITLE = 255;
  const MESSAGES = {
    title: "제목을 입력",
    titleLong: "제목은 255자까지",
    duplicate: "이미 있는 제목",
    cells: "셀 범위가 올바르지 않음",
    limit: "범위는 1,000개까지",
  };

  function createSheetProtectRanges(deps) {
    const { document, getActiveSheetName, ui, grid } = deps;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const guard = host.__EASYOFFICE_SHEET_PROTECT__;
    const byId = (id) => document.getElementById(id);
    const state = { picked: -1, editing: -1, bound: false };

    const rangesOf = () => layoutApi.protectOf(getActiveSheetName())?.ranges || [];

    // The sheet's protection with `ranges`: not protected, the file's usual locked actions.
    function withRanges(sheet, ranges) {
      const kept = layoutApi.protectOf(sheet);
      const deny = kept?.deny || guard.FLAG_NAMES.filter((flag) => !guard.DEFAULT_ALLOWED.includes(flag));
      layoutApi.setProtect(sheet, ranges.length > 0 || kept?.on ? { ...(kept || {}), on: kept?.on === true, deny, ranges } : null);
    }

    function renderList() {
      const list = byId("sheet-ranges-list");
      if (!list) return;
      list.replaceChildren();
      rangesOf().forEach((range, index) => {
        const li = document.createElement("li");
        li.setAttribute("role", "option");
        li.dataset.index = String(index);
        li.setAttribute("aria-selected", String(index === state.picked));
        const a = document.createElement("span");
        a.textContent = range.name;
        const b = document.createElement("span");
        b.textContent = (range.sqref || []).join(" ");
        li.appendChild(a);
        li.appendChild(b);
        list.appendChild(li);
      });
    }

    // 범위 편집 허용: a protected sheet keeps its ranges as they are.
    function open() {
      if (!guard.guardUnprotected(getActiveSheetName())) return Promise.resolve(false);
      state.picked = -1;
      renderList();
      kit.openDialog("sheet-ranges-dialog");
      byId("btn-sheet-ranges-new")?.focus?.();
      return Promise.resolve(true);
    }

    function openEdit(index) {
      const range = index >= 0 ? rangesOf()[index] : null;
      state.editing = index;
      kit.setVal("sheet-range-edit-title", range ? range.name : "범위" + (rangesOf().length + 1));
      kit.setVal("sheet-range-edit-cells", range ? (range.sqref || []).join(", ") : "");
      kit.setVal("sheet-range-edit-password", "");
      kit.setVal("sheet-range-edit-confirm", "");
      kit.note("sheet-range-edit-status", "");
      kit.openDialog("sheet-range-edit-dialog");
      byId("sheet-range-edit-title")?.focus?.();
    }

    async function confirmEdit() {
      const title = kit.val("sheet-range-edit-title").trim();
      const cells = host.__EASYOFFICE_SHEET_PAGE_CORE__.parseRanges(kit.val("sheet-range-edit-cells"));
      const others = rangesOf().filter((_, i) => i !== state.editing);
      let message = "";
      if (title === "") message = MESSAGES.title;
      else if ([...title].length > MAX_TITLE) message = MESSAGES.titleLong;
      else if (others.some((r) => r.name === title)) message = MESSAGES.duplicate;
      else if (!cells || cells.length === 0) message = MESSAGES.cells;
      else if (state.editing < 0 && others.length >= MAX_RANGES) message = MESSAGES.limit;
      const typed = kit.val("sheet-range-edit-password");
      let pwd = null;
      if (!message && typed !== "") {
        const read = await ui.readPassword("sheet-range-edit", true);
        if (read.message) message = read.message;
        else pwd = read.pwd;
      }
      if (message) {
        kit.note("sheet-range-edit-status", message);
        return false;
      }
      kit.closeDialog("sheet-range-edit-dialog");
      ui.clearFields("sheet-range-edit");
      const index = state.editing;
      return ui.step((sheet) => {
        const next = rangesOf().map((r) => ({ ...r }));
        // A range edited without a new password keeps the one it has.
        const range = { name: title, sqref: cells, ...(pwd ? { pwd } : index >= 0 && next[index].pwd ? { pwd: next[index].pwd } : {}) };
        if (index >= 0) next[index] = range;
        else next.push(range);
        withRanges(sheet, next);
        renderList();
        return "범위 편집 허용: " + title;
      });
    }

    function remove() {
      const picked = rangesOf()[state.picked];
      if (!picked) return Promise.resolve(false);
      return ui.step((sheet) => {
        withRanges(sheet, rangesOf().filter((_, i) => i !== state.picked));
        state.picked = -1;
        renderList();
        return "범위 삭제: " + picked.name;
      });
    }

    // The 범위 잠금 해제 dialog for a cell of a range with a password.
    async function askUnlock(sheet, name) {
      const range = (layoutApi.protectOf(sheet)?.ranges || []).find((r) => r.name === name);
      if (!range?.pwd) return false;
      const accepted = await ui.askPassword("범위 잠금 해제", (password) => ui.checkPassword(range.pwd, password));
      if (accepted) {
        guard.unlockRange(sheet, name);
        grid.layoutChanged();
        grid.refreshCells();
      }
      return accepted;
    }

    function bind() {
      if (state.bound) return;
      state.bound = true;
      byId("btn-sheet-ranges-new")?.addEventListener("click", () => openEdit(-1));
      byId("btn-sheet-ranges-edit")?.addEventListener("click", () => state.picked >= 0 && openEdit(state.picked));
      byId("btn-sheet-ranges-delete")?.addEventListener("click", () => void remove());
      byId("btn-sheet-range-edit-apply")?.addEventListener("click", () => void confirmEdit());
      byId("sheet-ranges-list")?.addEventListener("click", (ev) => {
        const li = ev.target.closest?.("li");
        if (!li) return;
        state.picked = Number(li.dataset.index);
        renderList();
      });
      byId("sheet-ranges-list")?.addEventListener("dblclick", (ev) => {
        const li = ev.target.closest?.("li");
        if (li) openEdit(Number(li.dataset.index));
      });
      for (const id of ["sheet-range-edit-title", "sheet-range-edit-cells", "sheet-range-edit-password", "sheet-range-edit-confirm"]) {
        byId(id)?.addEventListener("keydown", (ev) => {
          if (ev.key !== "Enter") return;
          ev.preventDefault();
          void confirmEdit();
        });
      }
      byId("sheet-range-edit-dialog")?.addEventListener("close", () => ui.clearFields("sheet-range-edit"));
      guard.askRangeUnlock = (sheet, name) => void askUnlock(sheet, name);
    }

    const actions = { "range-allow": open };
    return { actions, bind, open, askUnlock, state: () => state };
  }

  host.__EASYOFFICE_SHEET_PROTECT_RANGES_MESSAGES__ = MESSAGES;
  host.__EASYOFFICE_CREATE_SHEET_PROTECT_RANGES__ = createSheetProtectRanges;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
