(function (host) {
  "use strict";

  // The name box and the 이름 관리 and 이름 정의 dialogs of the sheet window (PD-214). The labels are those of the
  // Korean spreadsheet help (수식 > 이름 > 이름 정의 / 이름 관리: 이름, 범위, 설명, 참조 영역; the list shows the
  // name, its scope and what it refers to; 이름 추가, 이름 편집, 이름 삭제, 닫기; the 수식 menu opens 이름 정의). The name box is the cell address field of the
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
    nothing: "만들 이름이 없음",
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
    const state = { picked: -1, editing: null, bound: false, values: [], valuesStamp: 0 };

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

    // The names the list shows (not the hidden ones) with their place in the whole list, which the values follow.
    const manageable = () => names.getAll().map((def, index) => ({ def, index })).filter(({ def }) => !def.hidden);

    function renderManager() {
      const list = byId("sheet-names-list");
      if (!list) return;
      list.replaceChildren();
      const shown = manageable();
      shown.forEach(({ def, index }, i) => {
        const li = document.createElement("li");
        li.setAttribute("role", "option");
        li.setAttribute("aria-selected", i === state.picked ? "true" : "false");
        li.dataset.index = String(i);
        for (const text of [def.name, state.values[index] ?? "", def.refersTo, scopeLabel(def), def.comment ?? ""]) {
          const span = document.createElement("span");
          span.textContent = text;
          span.title = text;
          li.appendChild(span);
        }
        list.appendChild(li);
      });
      const has = state.picked >= 0 && state.picked < shown.length;
      for (const id of ["btn-sheet-names-edit", "btn-sheet-names-delete"]) {
        const button = byId(id);
        if (button) button.disabled = !has;
      }
    }

    // The values come from the engine, which has the cells: the list shows without them first and fills in.
    async function loadValues() {
      state.values = [];
      const stamp = (state.valuesStamp += 1);
      const result = await sheetStoreHelpers.nameValues?.(sheetDataStore).catch(() => null);
      if (stamp !== state.valuesStamp || !result?.ok || !Array.isArray(result.answer?.values)) return;
      state.values = result.answer.values;
      if (byId("sheet-names-dialog")?.open) renderManager();
    }

    function openManager() {
      state.picked = -1;
      kit.note("sheet-names-status", "");
      renderManager();
      kit.openDialog("sheet-names-dialog");
      void loadValues();
      return true;
    }

    // "<name> 이름을 지울까요?" with 지움 and 취소 (the spreadsheet help's question and button), true on 지움.
    function confirmDelete(name) {
      const dialog = byId("sheet-name-delete-dialog");
      if (!dialog) return Promise.resolve(true);
      byId("sheet-name-delete-message").textContent = name + " 이름을 지울까요?";
      const ok = byId("btn-sheet-name-delete-ok");
      return new Promise((resolve) => {
        let settled = false;
        const closers = Array.from(dialog.querySelectorAll("[data-dialog-close]"));
        const finish = (value) => {
          if (settled) return;
          settled = true;
          ok.removeEventListener("click", onOk);
          closers.forEach((closer) => closer.removeEventListener("click", onNo));
          dialog.removeEventListener("close", onNo);
          kit.closeDialog("sheet-name-delete-dialog");
          resolve(value);
        };
        const onOk = () => finish(true);
        const onNo = () => finish(false);
        ok.addEventListener("click", onOk);
        closers.forEach((closer) => closer.addEventListener("click", onNo));
        dialog.addEventListener("close", onNo);
        kit.openDialog("sheet-name-delete-dialog");
        ok.focus?.();
      });
    }

    async function deleteName() {
      const picked = manageable()[state.picked];
      if (!picked) return false;
      const { def } = picked;
      if (!(await confirmDelete(def.name))) return false;
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
    // `fromManager` titles a new name 이름 추가 (the 이름 관리 button); from the 수식 menu it is 이름 정의.
    function openEdit(def, fromManager) {
      commitIfEditing();
      state.editing = def ? { name: def.name, scope: def.scope ?? null, hidden: def.hidden === true } : null;
      kit.setVal("sheet-name-edit-name", def?.name ?? "");
      fillScope(def?.scope ?? "");
      kit.setVal("sheet-name-edit-comment", def?.comment ?? "");
      kit.setVal("sheet-name-edit-refers", def?.refersTo ?? names.refersToOf(getActiveSheetName(), getRange()));
      kit.note("sheet-name-edit-status", "");
      byId("sheet-name-edit-dialog-title").textContent = def ? "이름 편집" : fromManager ? "이름 추가" : "이름 정의";
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

    // ---- 선택 영역에서 이름 만들기 -----------------------------------------------------------------------

    const EDGE_IDS = { top: "sheet-name-create-top", bottom: "sheet-name-create-bottom", left: "sheet-name-create-left", right: "sheet-name-create-right" };
    const cellText = (map, r, c) => {
      const entry = map[host.__EASYOFFICE_SHEET_REFS__.refOf(r, c)];
      return entry ? String(entry.value || entry.formula || "") : "";
    };

    // Which edges hold labels, as a guess: the first row when it is all text, else the left column when it is, else the first row.
    function guessEdges(map, rg) {
      const isText = (r, c) => {
        const t = cellText(map, r, c).trim();
        return t !== "" && !Number.isFinite(Number(t.replaceAll(",", "")));
      };
      const rowText = (r) => Array.from({ length: rg.c2 - rg.c1 + 1 }, (_, i) => isText(r, rg.c1 + i)).every(Boolean);
      const colText = (c) => Array.from({ length: rg.r2 - rg.r1 + 1 }, (_, i) => isText(rg.r1 + i, c)).every(Boolean);
      const left = !rowText(rg.r1) && colText(rg.c1);
      return { top: !left, bottom: false, left, right: false };
    }

    function openCreate() {
      commitIfEditing();
      const sheet = getActiveSheetName();
      sheetDataStore[sheet] = deps.snapshotDomSheetCells();
      const rg = getRange();
      if (rg.r1 === rg.r2 && rg.c1 === rg.c2) {
        say(MESSAGES.nothing);
        return false;
      }
      const edges = guessEdges(sheetDataStore[sheet] || {}, rg);
      for (const key of Object.keys(EDGE_IDS)) kit.setChecked(EDGE_IDS[key], edges[key]);
      kit.note("sheet-name-create-status", "");
      kit.openDialog("sheet-name-create-dialog");
      return true;
    }

    async function confirmCreate() {
      const sheet = getActiveSheetName();
      const edges = Object.fromEntries(Object.keys(EDGE_IDS).map((key) => [key, kit.checked(EDGE_IDS[key])]));
      const map = sheetDataStore[sheet] || {};
      const candidates = names.namesFromSelection(sheet, getRange(), edges, (r, c) => cellText(map, r, c));
      let usable = [];
      if (candidates.length > 0) {
        try {
          const res = await sheetStoreHelpers.callSheetAction({ sessionAction: "nameCheck", sheets: sheetKeys(), names: names.getAll(), candidates });
          usable = Array.isArray(res?.problems) ? candidates.filter((_, i) => res.problems[i] === null) : [];
        } catch {
          usable = [];
        }
      }
      usable = usable.slice(0, Math.max(0, names.MAX_NAMES - names.getAll().length));
      if (usable.length === 0) {
        kit.note("sheet-name-create-status", MESSAGES.nothing);
        return false;
      }
      kit.closeDialog("sheet-name-create-dialog");
      return step((list) => [...list, ...usable], "이름 " + usable.length + "개 만듦");
    }

    function bindDialogs() {
      byId("btn-sheet-name-create-apply")?.addEventListener("click", () => void confirmCreate());
      byId("sheet-names-list")?.addEventListener("click", (ev) => {
        const li = ev.target.closest?.("li");
        if (!li) return;
        state.picked = Number(li.dataset.index);
        renderManager();
      });
      byId("sheet-names-list")?.addEventListener("dblclick", (ev) => {
        const li = ev.target.closest?.("li");
        if (li) openEdit(manageable()[Number(li.dataset.index)]?.def);
      });
      byId("btn-sheet-names-new")?.addEventListener("click", () => openEdit(null, true));
      byId("btn-sheet-names-edit")?.addEventListener("click", () => {
        const picked = manageable()[state.picked];
        if (picked) openEdit(picked.def);
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
      "name-create": () => Promise.resolve(openCreate()),
    };
    return { actions, bind, refreshNameBox, openManager, openEdit, openCreate, confirmCreate, deleteName, checkCandidate, commitNameBox, goTo, state: () => state };
  }

  host.__EASYOFFICE_SHEET_NAMES_MESSAGES__ = MESSAGES;
  host.__EASYOFFICE_CREATE_SHEET_NAMES_UI__ = createSheetNamesUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
