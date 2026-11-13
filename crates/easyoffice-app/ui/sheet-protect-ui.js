(function (host) {
  "use strict";

  // The protection commands of the sheet window (PD-216): 시트 보호 and 통합 문서 보호 with their password dialogs, in the
  // 도구 menu. The names are those of the Korean spreadsheet help (시트 보호 with 암호 사용, 암호 설정
  // (암호, 암호 확인) and 워크시트에서 허용할 내용; the dialog that asks the password again is 암호; 통합 문서 보호 with 암호,
  // 암호 확인 and 암호 없이 보호, and 통합 문서 보호 해제; 설정 and 확인 are the buttons). The same menu item protects and
  // lifts the protection. A password is typed once into a password field, goes to the engine (`protectHash`,
  // `protectVerify`) and only the digest it answers is kept: in the layout store, the undo snapshots and the file. Each
  // change is one undo step.

  const MESSAGES = {
    empty: "암호를 입력",
    mismatch: "새 암호와 암호 확인이 일치하지 않습니다.",
    long: "암호는 255자까지",
    wrong: "암호가 올바르지 않습니다.",
    failed: "암호를 확인하지 못함",
  };
  const MAX_PASSWORD = 255;
  const T_SHEET = "시트 보호";
  const T_SHEET_OFF = "시트 보호 해제";
  const T_BOOK = "통합 문서 보호";
  const T_BOOK_OFF = "통합 문서 보호 해제";

  function createSheetProtectUi(deps) {
    const { document, grid, getActiveSheetName, sheetStoreHelpers, commitIfEditing, enqueueWorkspaceHistoryOp, drainInFlightSheetWork, pushRawUndoSnapshot, captureWorkspaceSnapshot, refreshChrome } = deps;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const guard = host.__EASYOFFICE_SHEET_PROTECT__;
    const byId = (id) => document.getElementById(id);
    const say = (text) => guard.say(text);
    const state = { bound: false, pending: null };

    // ---- the engine -----------------------------------------------------------------------------------
    // The digest of `password` (an XLSX digest and an ODS key; the file keeps its own), or `{ problem }`.
    async function hashPassword(password) {
      const res = await sheetStoreHelpers.callSheetAction({ sessionAction: "protectHash", password, kind: "both" });
      return res && typeof res === "object" ? res : { problem: "failed" };
    }

    // `""` when `password` opens `pwd`, else the message to show.
    async function checkPassword(pwd, password) {
      let res;
      try {
        res = await sheetStoreHelpers.callSheetAction({ sessionAction: "protectVerify", pwd, password });
      } catch {
        return MESSAGES.failed;
      }
      if (res?.ok === true) return "";
      if (res?.ok === false) return MESSAGES.wrong;
      return guard.UNSUPPORTED[res?.unsupported] || MESSAGES.failed;
    }

    // One undo step over the layout store and the workbook protection: `change(name)` returns the status text, or
    // null to leave everything as it is.
    function step(change) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        const name = getActiveSheetName();
        const before = captureWorkspaceSnapshot(false, true);
        const text = change(name);
        if (text === null) return false;
        pushRawUndoSnapshot(before);
        grid.layoutChanged();
        // The cells already on the page take the lock of the new state (their rows are kept by a redraw).
        grid.refreshCells();
        refreshChrome();
        refreshMenu();
        say(text);
        return true;
      });
    }

    // ---- the password dialog (암호, 통합 문서 보호 해제, 범위 잠금 해제) -------------------------------------------------
    // Asks for a password; `check(password)` answers "" to accept or the message to show. Resolves whether it was accepted.
    function askPassword(title, check) {
      return new Promise((resolve) => {
        state.pending?.(false);
        const input = byId("sheet-password-input");
        byId("sheet-password-dialog-title").textContent = title;
        input.value = "";
        kit.note("sheet-password-status", "");
        state.pending = (accepted) => {
          state.pending = null;
          input.value = "";
          kit.closeDialog("sheet-password-dialog");
          resolve(accepted);
        };
        state.check = check;
        kit.openDialog("sheet-password-dialog");
        input.focus?.();
      });
    }

    async function confirmPassword() {
      const input = byId("sheet-password-input");
      const message = await state.check(input.value);
      if (message) {
        kit.note("sheet-password-status", message);
        return;
      }
      state.pending?.(true);
    }

    // ---- the sheet ------------------------------------------------------------------------------------
    const flagBox = (name) => "sheet-protect-allow-" + name;

    function openProtect(name) {
      const existing = layoutApi.protectOf(name);
      const allowed = existing ? guard.FLAG_NAMES.filter((flag) => !existing.deny.includes(flag)) : guard.DEFAULT_ALLOWED;
      kit.setChecked("sheet-protect-use-password", false);
      kit.setVal("sheet-protect-password", "");
      kit.setVal("sheet-protect-confirm", "");
      for (const flag of guard.FLAG_NAMES) kit.setChecked(flagBox(flag), allowed.includes(flag));
      syncPasswordFields("sheet-protect");
      kit.note("sheet-protect-status", "");
      kit.openDialog("sheet-protect-dialog");
      byId("sheet-protect-use-password")?.focus?.();
      return true;
    }

    // The password fields follow the check box that turns them on.
    function syncPasswordFields(prefix) {
      const use = prefix === "sheet-protect" ? kit.checked("sheet-protect-use-password") : !kit.checked("sheet-wb-protect-none");
      for (const id of [prefix + "-password", prefix + "-confirm"]) {
        const el = byId(id);
        if (el) el.disabled = !use;
      }
    }

    // The digest the fields ask for: `{ pwd }` (absent when no password is used) or `{ message }`.
    async function readPassword(prefix, use) {
      if (!use) return {};
      const password = kit.val(prefix + "-password");
      if (password === "") return { message: MESSAGES.empty };
      if (password !== kit.val(prefix + "-confirm")) return { message: MESSAGES.mismatch };
      if ([...password].length > MAX_PASSWORD) return { message: MESSAGES.long };
      let hashed;
      try {
        hashed = await hashPassword(password);
      } catch {
        return { message: MESSAGES.failed };
      }
      if (hashed.problem === "long") return { message: MESSAGES.long };
      if (hashed.problem === "empty") return { message: MESSAGES.empty };
      return hashed.pwd ? { pwd: hashed.pwd } : { message: MESSAGES.failed };
    }

    function clearFields(prefix) {
      kit.setVal(prefix + "-password", "");
      kit.setVal(prefix + "-confirm", "");
    }

    async function applyProtect() {
      const name = getActiveSheetName();
      const result = await readPassword("sheet-protect", kit.checked("sheet-protect-use-password"));
      if (result.message) {
        kit.note("sheet-protect-status", result.message);
        return false;
      }
      kit.closeDialog("sheet-protect-dialog");
      clearFields("sheet-protect");
      const deny = guard.FLAG_NAMES.filter((flag) => !kit.checked(flagBox(flag)));
      return step((sheet) => {
        const next = { on: true, deny, ranges: layoutApi.protectOf(sheet)?.ranges || [] };
        if (result.pwd) next.pwd = result.pwd;
        layoutApi.setProtect(sheet, next);
        return T_SHEET;
      });
    }

    // Lifts the protection of the sheet: its password is the one thing asked; the allowed ranges stay.
    function liftProtect() {
      return step((sheet) => {
        const kept = layoutApi.protectOf(sheet);
        layoutApi.setProtect(sheet, kept?.ranges?.length ? { on: false, deny: kept.deny, ranges: kept.ranges } : null);
        guard.forgetUnlocked();
        return T_SHEET_OFF;
      });
    }

    async function toggleSheet() {
      const name = getActiveSheetName();
      const p = layoutApi.protectOf(name);
      if (!p || p.on !== true) return openProtect(name);
      if (p.pwd && !(await askPassword("암호", (password) => checkPassword(p.pwd, password)))) return false;
      return liftProtect();
    }

    // ---- the workbook ---------------------------------------------------------------------------------
    function openWorkbook() {
      kit.setChecked("sheet-wb-protect-none", false);
      clearFields("sheet-wb-protect");
      syncPasswordFields("sheet-wb-protect");
      kit.note("sheet-wb-protect-status", "");
      kit.openDialog("sheet-wb-protect-dialog");
      byId("sheet-wb-protect-password")?.focus?.();
      return true;
    }

    async function applyWorkbook() {
      const result = await readPassword("sheet-wb-protect", !kit.checked("sheet-wb-protect-none"));
      if (result.message) {
        kit.note("sheet-wb-protect-status", result.message);
        return false;
      }
      kit.closeDialog("sheet-wb-protect-dialog");
      clearFields("sheet-wb-protect");
      return step(() => {
        guard.setWorkbook({ structure: true, windows: false, ...(result.pwd ? { pwd: result.pwd } : {}) });
        return T_BOOK;
      });
    }

    async function toggleWorkbook() {
      const book = guard.getWorkbook();
      if (!book?.structure) return openWorkbook();
      if (book.pwd && !(await askPassword(T_BOOK_OFF, (password) => checkPassword(book.pwd, password)))) return false;
      return step(() => {
        guard.setWorkbook(null);
        return T_BOOK_OFF;
      });
    }

    // The menu items say which of the two things they do now.
    function refreshMenu() {
      const sheetOn = layoutApi.protectOf(getActiveSheetName())?.on === true;
      for (const [act, on, off, onText] of [["sheet-protect", sheetOn, T_SHEET, T_SHEET_OFF], ["workbook-protect", guard.structureLocked(), T_BOOK, T_BOOK_OFF]]) {
        for (const label of document.querySelectorAll('[data-sheet-act="' + act + '"] .md-label')) label.textContent = on ? onText : off;
      }
    }

    function bind() {
      if (state.bound) return;
      state.bound = true;
      byId("btn-sheet-protect-apply")?.addEventListener("click", () => void applyProtect());
      byId("btn-sheet-wb-protect-apply")?.addEventListener("click", () => void applyWorkbook());
      byId("btn-sheet-password-apply")?.addEventListener("click", () => void confirmPassword());
      byId("sheet-protect-use-password")?.addEventListener("change", () => syncPasswordFields("sheet-protect"));
      byId("sheet-wb-protect-none")?.addEventListener("change", () => syncPasswordFields("sheet-wb-protect"));
      for (const [id, run] of [["sheet-protect-confirm", applyProtect], ["sheet-wb-protect-confirm", applyWorkbook], ["sheet-password-input", confirmPassword]]) {
        byId(id)?.addEventListener("keydown", (ev) => {
          if (ev.key !== "Enter") return;
          ev.preventDefault();
          void run();
        });
      }
      // Closing a dialog by any way leaves no password in a field and settles the question that waits.
      for (const id of ["sheet-protect-dialog", "sheet-wb-protect-dialog", "sheet-password-dialog"]) {
        byId(id)?.addEventListener("close", () => {
          clearFields("sheet-protect");
          clearFields("sheet-wb-protect");
          kit.setVal("sheet-password-input", "");
          state.pending?.(false);
        });
      }
      layoutApi.onChange(refreshMenu);
      guard.onWorkbookChange(refreshMenu);
      refreshMenu();
    }

    const actions = {
      "sheet-protect": toggleSheet,
      "workbook-protect": toggleWorkbook,
    };
    return { actions, bind, refreshMenu, askPassword, checkPassword, hashPassword, step, readPassword, syncPasswordFields, clearFields, MESSAGES };
  }

  host.__EASYOFFICE_CREATE_SHEET_PROTECT_UI__ = createSheetProtectUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
