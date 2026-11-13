(function (host) {
  "use strict";

  // The name box and the 이름 관리 and 이름 정의 dialogs of the sheet window (PD-214). The labels are those of the
  // Korean spreadsheet help (수식 > 이름 > 이름 정의 / 이름 관리: 이름, 범위, 설명, 참조 영역; the list shows the
  // name, its scope and what it refers to; 새로 만들기, 편집, 삭제, 닫기). The name box is the cell address field of the
  // formula bar: a name typed there selects what it refers to, an address selects that range, and a new valid name
  // names the selection; its list opens with the arrow and ends with the 이름 관리 entry. Each change of the list is
  // one undo step (`captureWorkspaceSnapshot` carries the names).

  const MESSAGES = {
    empty: "이름을 입력",
    tooLong: "이름은 255자까지",
    name: "이름이 올바르지 않음",
    reserved: "함수나 시트 이름은 쓸 수 없음",
    duplicate: "이미 있는 이름",
    scope: "범위가 올바르지 않음",
    formula: "참조 영역이 올바르지 않음",
    refersEmpty: "참조 영역을 입력",
    comment: "설명은 255자까지",
    cycle: "자기 자신을 참조하는 이름",
    size: "너무 길거나 깊게 이어진 이름",
    notRange: "참조 영역이 아닌 이름",
    unknownSheet: "참조할 시트가 없음",
    limit: "이름은 10,000개까지",
    unchecked: "이름을 확인하지 못함",
  };
  const WORKBOOK = "통합 문서";

  function createSheetNamesUi(deps) {
    const {
      document, sheetDataStore, sheetStoreHelpers, getActiveSheetName, getRange, selectRange, switchSheet, focusAnchor, commitIfEditing,
      enqueueWorkspaceHistoryOp, drainInFlightSheetWork, pushRawUndoSnapshot, captureWorkspaceSnapshot,
    } = deps;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const names = host.__EASYOFFICE_SHEET_NAMES__;
    const byId = (id) => document.getElementById(id);
    const state = { picked: -1, editing: null, bound: false };

    function say(text) {
      const fb = byId("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    const sheetKeys = () => Object.keys(sheetDataStore);
    const scopeLabel = (def) => def.scope || WORKBOOK;

    // ---- the engine's check of one new or changed name ------------------------------------------------

    // `null` when `candidate` can be defined (it replaces `replacing`, a `{ name, scope }`), else the problem's code.
    async function checkCandidate(candidate, replacing) {
      const local = names.limitProblem(candidate);
      if (local) return local;
      if (names.getAll().length >= names.MAX_NAMES && !replacing) return "limit";
      try {
        const res = await sheetStoreHelpers.callSheetAction({
          sessionAction: "nameCheck",
          sheets: sheetKeys(),
          names: names.getAll(),
          candidate,
          replace: replacing ?? null,
        });
        return res && "problem" in res ? res.problem : "unchecked";
      } catch {
        return "unchecked";
      }
    }

    // One undo step over the list: `change(list)` returns the next list.
    function step(change, text) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing();
        const before = captureWorkspaceSnapshot(false, true);
        names.replaceAll(change(names.getAll()));
        pushRawUndoSnapshot(before);
        refreshNameBox();
        renderManager();
        say(text);
        return true;
      });
    }

    // ---- the name box -------------------------------------------------------------------------------

    const box = () => byId("active-cell-ref");
    const popup = () => byId("sheet-name-box-list");

    function refreshNameBox() {
      const el = box();
      if (!el || document.activeElement === el) return;
      const rg = getRange();
      const named = names.nameOfRange(getActiveSheetName(), rg);
      el.textContent = named || host.__EASYOFFICE_SHEET_REFS__.refOf(rg.r1, rg.c1);
    }

    function closeList() {
      const list = popup();
      if (list && !list.hidden) list.hidden = true;
      box()?.setAttribute("aria-expanded", "false");
    }

    function renderList(filter) {
      const list = popup();
      if (!list) return 0;
      list.replaceChildren();
      const needle = names.fold(filter);
      const shown = names.visibleFor(getActiveSheetName()).filter((def) => !needle || names.fold(def.name).includes(needle));
      for (const def of shown) {
        const li = document.createElement("li");
        li.setAttribute("role", "option");
        li.dataset.name = def.name;
        if (def.scope) li.dataset.scope = def.scope;
        const a = document.createElement("span");
        a.textContent = def.name;
        const b = document.createElement("span");
        b.textContent = def.refersTo;
        li.appendChild(a);
        li.appendChild(b);
        list.appendChild(li);
      }
      const manage = document.createElement("li");
      manage.setAttribute("role", "option");
      manage.dataset.manage = "1";
      manage.textContent = "이름 관리…";
      list.appendChild(manage);
      return shown.length;
    }

    function openList(filter) {
      const list = popup();
      if (!list) return;
      const count = renderList(filter);
      if (filter && count === 0) {
        closeList();
        return;
      }
      list.hidden = false;
      box()?.setAttribute("aria-expanded", "true");
    }

    // Selects what `def` refers to, on its sheet.
    async function goTo(def) {
      const at = names.parseRange(def.refersTo);
      if (!at) {
        say(MESSAGES.notRange);
        return false;
      }
      const sheet = sheetKeys().find((key) => names.fold(key) === names.fold(at.sheet));
      if (!sheet) {
        say(MESSAGES.unknownSheet);
        return false;
      }
      if (sheet !== getActiveSheetName()) await switchSheet(sheet);
      selectRange(at);
      say(def.name);
      return true;
    }

    // Enter in the name box: a name selects what it refers to, an address selects that range, any other valid text
    // names the selection.
    async function commitNameBox() {
      const el = box();
      const text = String(el?.textContent ?? "").trim();
      const sheet = getActiveSheetName();
      if (!text) return false;
      const known = names.find(text, sheet);
      if (known) return goTo(known);
      const address = names.parseAddress(text);
      if (address) {
        selectRange(address);
        return true;
      }
      const rg = getRange();
      const candidate = { name: text, refersTo: names.refersToOf(sheet, rg) };
      const problem = await checkCandidate(candidate, null);
      if (problem) {
        say(MESSAGES[problem] ?? MESSAGES.name);
        return false;
      }
      await step((list) => [...list, candidate], "이름 정의: " + text);
      return true;
    }

    function bindNameBox() {
      const el = box();
      if (!el) return;
      el.addEventListener("focus", () => {
        const sel = document.defaultView?.getSelection?.();
        if (sel && document.createRange) {
          const range = document.createRange();
          range.selectNodeContents(el);
          sel.removeAllRanges();
          sel.addRange(range);
        }
      });
      el.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter") {
          ev.preventDefault();
          closeList();
          void commitNameBox().then(() => {
            el.blur?.();
            focusAnchor();
            refreshNameBox();
          });
        } else if (ev.key === "Escape") {
          ev.preventDefault();
          closeList();
          el.blur?.();
          focusAnchor();
          refreshNameBox();
        } else if (ev.key === "ArrowDown") {
          ev.preventDefault();
          openList("");
        }
      });
      el.addEventListener("input", () => openList(el.textContent.trim()));
      el.addEventListener("blur", () => {
        // A click in the list happens before the box loses the pointer (`mousedown` prevents the blur).
        closeList();
        refreshNameBox();
      });
      byId("btn-name-box-list")?.addEventListener("mousedown", (ev) => {
        ev.preventDefault();
        if (popup()?.hidden === false) closeList();
        else openList("");
      });
      popup()?.addEventListener("mousedown", (ev) => {
        ev.preventDefault();
        const li = ev.target.closest?.("li");
        if (!li) return;
        closeList();
        if (li.dataset.manage) {
          openManager();
          return;
        }
        const def = names.find(li.dataset.name, li.dataset.scope || getActiveSheetName());
        if (def) void goTo(def).then(() => focusAnchor());
      });
      names.on("changed", () => deps.onNamesChanged?.());
    }

    // ---- 이름 관리 ---------------------------------------------------------------------------------------

    const manageable = () => names.getAll().filter((def) => !def.hidden);

    function renderManager() {
      const list = byId("sheet-names-list");
      if (!list) return;
      list.replaceChildren();
      const shown = manageable();
      shown.forEach((def, i) => {
        const li = document.createElement("li");
        li.setAttribute("role", "option");
        li.setAttribute("aria-selected", i === state.picked ? "true" : "false");
        li.dataset.index = String(i);
        for (const text of [def.name, scopeLabel(def), def.refersTo]) {
          const span = document.createElement("span");
          span.textContent = text;
          li.appendChild(span);
        }
        li.title = def.comment || "";
        list.appendChild(li);
      });
      const has = state.picked >= 0 && state.picked < shown.length;
      for (const id of ["btn-sheet-names-edit", "btn-sheet-names-delete"]) {
        const button = byId(id);
        if (button) button.disabled = !has;
      }
    }

    function openManager() {
      state.picked = -1;
      kit.note("sheet-names-status", "");
      renderManager();
      kit.openDialog("sheet-names-dialog");
      return true;
    }

    function deleteName() {
      const def = manageable()[state.picked];
      if (!def) return Promise.resolve(false);
      state.picked = -1;
      return step((list) => list.filter((other) => !(names.fold(other.name) === names.fold(def.name) && (other.scope ?? "") === (def.scope ?? ""))), "이름 삭제: " + def.name);
    }

    // ---- 이름 정의 (new and edit) -------------------------------------------------------------------------

    function fillScope(current) {
      const select = byId("sheet-name-edit-scope");
      if (!select) return;
      select.replaceChildren();
      for (const [value, label] of [["", WORKBOOK], ...sheetKeys().map((key) => [key, key])]) {
        const option = document.createElement("option");
        option.value = value;
        option.textContent = label;
        select.appendChild(option);
      }
      host.__EASYOFFICE_CUSTOM_SELECT__?.enhanceCustomSelects?.(document, select.parentElement);
      kit.setSelect("sheet-name-edit-scope", current ?? "");
    }

    // The dialog for a new name (prefilled with the selection) or for `def` (the manager's pick).
    function openEdit(def) {
      commitIfEditing();
      state.editing = def ? { name: def.name, scope: def.scope ?? null, hidden: def.hidden === true } : null;
      kit.setVal("sheet-name-edit-name", def?.name ?? "");
      fillScope(def?.scope ?? "");
      kit.setVal("sheet-name-edit-comment", def?.comment ?? "");
      kit.setVal("sheet-name-edit-refers", def?.refersTo ?? names.refersToOf(getActiveSheetName(), getRange()));
      kit.note("sheet-name-edit-status", "");
      byId("sheet-name-edit-dialog-title").textContent = def ? "이름 편집" : "이름 정의";
      kit.openDialog("sheet-name-edit-dialog");
      byId("sheet-name-edit-name")?.focus?.();
      return true;
    }

    async function confirmEdit() {
      const candidate = {
        name: kit.val("sheet-name-edit-name").trim(),
        refersTo: kit.val("sheet-name-edit-refers").trim().replace(/^=/, ""),
      };
      const scope = kit.val("sheet-name-edit-scope");
      if (scope) candidate.scope = scope;
      const comment = kit.val("sheet-name-edit-comment");
      if (comment) candidate.comment = comment;
      if (state.editing?.hidden) candidate.hidden = true;
      const replacing = state.editing ? { name: state.editing.name, scope: state.editing.scope } : null;
      const problem = await checkCandidate(candidate, replacing);
      if (problem) {
        kit.note("sheet-name-edit-status", MESSAGES[problem] ?? MESSAGES.name);
        return false;
      }
      kit.closeDialog("sheet-name-edit-dialog");
      const old = state.editing;
      const done = await step((list) => {
        if (!old) return [...list, candidate];
        return list.map((def) => (names.fold(def.name) === names.fold(old.name) && (def.scope ?? null) === old.scope ? candidate : def));
      }, (old ? "이름 편집: " : "이름 정의: ") + candidate.name);
      state.editing = null;
      return done;
    }

    function bindDialogs() {
      byId("sheet-names-list")?.addEventListener("click", (ev) => {
        const li = ev.target.closest?.("li");
        if (!li) return;
        state.picked = Number(li.dataset.index);
        renderManager();
      });
      byId("sheet-names-list")?.addEventListener("dblclick", (ev) => {
        const li = ev.target.closest?.("li");
        if (li) openEdit(manageable()[Number(li.dataset.index)]);
      });
      byId("btn-sheet-names-new")?.addEventListener("click", () => openEdit(null));
      byId("btn-sheet-names-edit")?.addEventListener("click", () => {
        const def = manageable()[state.picked];
        if (def) openEdit(def);
      });
      byId("btn-sheet-names-delete")?.addEventListener("click", () => void deleteName());
      byId("btn-sheet-name-edit-apply")?.addEventListener("click", () => void confirmEdit());
      byId("sheet-name-edit-name")?.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter") {
          ev.preventDefault();
          void confirmEdit();
        }
      });
    }

    function bind() {
      if (state.bound) return;
      state.bound = true;
      bindNameBox();
      bindDialogs();
    }

    const actions = {
      "name-define": () => Promise.resolve(openEdit(null)),
      "name-manage": () => Promise.resolve(openManager()),
    };
    return { actions, bind, refreshNameBox, openManager, openEdit, checkCandidate, commitNameBox, goTo, state: () => state };
  }

  host.__EASYOFFICE_SHEET_NAMES_MESSAGES__ = MESSAGES;
  host.__EASYOFFICE_CREATE_SHEET_NAMES_UI__ = createSheetNamesUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
