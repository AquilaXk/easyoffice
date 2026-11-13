// The hyperlink dialog of the sheet window and what a click on a link cell does (PD-218).
//
// The dialog is the reference editor's hyperlink dialog (`rhwp-studio/src/ui/hyperlink-dialog.ts`: 표시할 문자열,
// 연결 대상 with its tabs, 넣기 / 고치기) with the spreadsheet help's targets: 웹 주소 and 현재 문서 (한셀 도움말
// 입력 > 하이퍼링크: 연결 대상 현재 문서 · 웹 주소 · 전자 우편 주소, 넣기), whose 시트 and 셀 참조 follow the spreadsheet
// help's 이 문서에 배치 (셀 참조, 정의된 이름). A mail address goes in the 웹 주소 field, as in the document editor.
// A link cell opens on a click (한셀: one click of the left button; the spreadsheet help: holding the button down selects
// the cell instead), a cell that is part of a larger selection or clicked with a modifier only selects. Applying and
// removing are one undo step each and go through the cell writer, so the protection of the sheet decides (the flag
// 하이퍼링크 삽입 and the lock of the cell).
(function (host) {
  "use strict";

  // Holding the button down this long (ms) or moving the pointer this far (px) selects the cell instead of following it.
  const HOLD_MS = 500;
  const DRAG_PX = 4;
  const MESSAGES = {
    address: "허용되지 않는 주소",
    reference: "셀 참조가 올바르지 않음",
    missing: "연결 대상 없음",
    failed: "열지 못함",
    nothing: "넣을 칸 없음",
  };

  function createSheetLinkUi(deps) {
    const {
      document, sheetDataStore, getActiveSheetName, getSelection, getRange, selectRange, switchSheet, commitIfEditing,
      focusAnchor, snapshotDomSheetCells, applyCellEdits, gridUtils,
    } = deps;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const links = host.__EASYOFFICE_SHEET_LINK__;
    const names = host.__EASYOFFICE_SHEET_NAMES__;
    const byId = (id) => document.getElementById(id);
    const state = { ref: "", sheet: "", entry: null, tab: "web" };

    function say(text) {
      const fb = byId("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    const sheetKeys = () => Object.keys(sheetDataStore);
    const sameSheet = (a, b) => names.fold(a) === names.fold(b);

    // ---- following a link ----------------------------------------------------------------------------------

    // Selects `at` ({ r1, c1, r2, c2 }, sheet optional) on its sheet; false when the sheet is not in the workbook.
    async function jump(at, fallbackSheet) {
      const sheet = sheetKeys().find((key) => sameSheet(key, at.sheet ?? fallbackSheet));
      if (!sheet) return false;
      if (sheet !== getActiveSheetName()) await switchSheet(sheet);
      selectRange(at);
      return true;
    }

    // The place an in-workbook target names: `Sheet1!B2:C3`, `B2` (on the sheet the link is on) or a defined name.
    async function followInternal(target, from) {
      const location = links.locationOf(target);
      const qualified = names.parseRange(location);
      if (qualified) return jump(qualified, from);
      const bare = names.parseAddress(location);
      if (bare) return jump(bare, from);
      const def = names.find(location, from);
      const at = def ? names.parseRange(def.refersTo) : null;
      return at ? jump(at, from) : false;
    }

    // Opens an address in the system's browser or mail program through the shell's own command, which applies the
    // same policy again; outside the shell (the preview server) the page opens it.
    async function openExternal(target) {
      const bridge = host.__EASYOFFICE_DESKTOP_BRIDGE__;
      if (bridge?.isAvailable?.()) {
        await bridge.openLink(target);
        return true;
      }
      return Boolean(host.open?.(target, "_blank", "noopener,noreferrer"));
    }

    // Follows the link of a cell on `sheet`; says what went wrong on the status line.
    async function follow(link, sheet) {
      const target = links.normalizeTarget(link?.target);
      if (target === null) {
        say(MESSAGES.address);
        return false;
      }
      try {
        const done = links.isInternal(target) ? await followInternal(target, sheet) : await openExternal(target);
        if (!done) say(links.isInternal(target) ? MESSAGES.missing : MESSAGES.failed);
        return done;
      } catch {
        say(MESSAGES.failed);
        return false;
      }
    }

    // ---- a click on a link cell ----------------------------------------------------------------------------

    let press = null;

    function onMouseDown(ev) {
      const td = ev.target?.closest?.("#pane-sheet td[data-link]");
      press = td && ev.button === 0 && !ev.shiftKey && !ev.ctrlKey && !ev.metaKey && !ev.altKey
        ? { td, at: ev.timeStamp, x: ev.clientX, y: ev.clientY }
        : null;
    }

    function onClick(ev) {
      const down = press;
      press = null;
      const td = ev.target?.closest?.("#pane-sheet td[data-link]");
      if (!down || down.td !== td || ev.button !== 0 || td.classList.contains("editing")) return;
      if (ev.timeStamp - down.at >= HOLD_MS || Math.hypot(ev.clientX - down.x, ev.clientY - down.y) > DRAG_PX) return;
      const rg = getRange();
      if (rg.r1 !== rg.r2 || rg.c1 !== rg.c2 || gridUtils.refOf(rg.r1, rg.c1) !== td.dataset.cell) return;
      void follow(td._link, getActiveSheetName());
    }

    // ---- the dialog ----------------------------------------------------------------------------------------

    function showTab(tab) {
      state.tab = tab;
      for (const name of ["web", "doc"]) {
        const on = name === tab;
        const button = byId("sheet-link-tab-" + name);
        button?.classList.toggle("active", on);
        button?.setAttribute("aria-selected", on ? "true" : "false");
        button?.setAttribute("tabindex", on ? "0" : "-1");
        const pane = byId("sheet-link-pane-" + name);
        if (pane) pane.hidden = !on;
      }
    }

    function fillSheets(current) {
      const select = byId("sheet-link-sheet");
      if (!select) return;
      select.replaceChildren();
      for (const key of sheetKeys()) {
        const option = document.createElement("option");
        option.value = key;
        option.textContent = key;
        select.appendChild(option);
      }
      host.__EASYOFFICE_CUSTOM_SELECT__?.enhanceCustomSelects?.(document, select.parentElement);
      kit.setSelect("sheet-link-sheet", current);
    }

    // The 현재 문서 fields for an in-workbook location: the sheet and what follows it (a cell reference or a name).
    function splitLocation(location, own) {
      const at = names.parseRange(location);
      if (at) {
        const sheet = sheetKeys().find((key) => sameSheet(key, at.sheet)) ?? own;
        return { sheet, ref: location.slice(location.lastIndexOf("!") + 1).replaceAll("$", "") };
      }
      return { sheet: own, ref: location };
    }

    // The active cell, read from the live sheet: what a dialog, a removal or a click works on.
    function loadCell() {
      commitIfEditing();
      const sheet = getActiveSheetName();
      const map = snapshotDomSheetCells();
      sheetDataStore[sheet] = map;
      const sel = getSelection();
      const ref = gridUtils.refOf(sel.ar, sel.ac);
      Object.assign(state, { ref, sheet, entry: map[ref] ?? null });
      return state;
    }

    // The dialog for the active cell: empty for a new link, filled for the cell's own.
    function open() {
      const { sheet, ref, entry } = loadCell();
      const link = links.of(entry);
      const internal = link ? links.isInternal(link.target) : false;
      const place = internal ? splitLocation(links.locationOf(link.target), sheet) : { sheet, ref: "" };
      kit.setVal("sheet-link-text", entry ? String(entry.value || "") : "");
      const text = byId("sheet-link-text");
      if (text) text.disabled = Boolean(entry?.formula);
      kit.setVal("sheet-link-address", link && !internal ? link.target : "");
      fillSheets(place.sheet);
      kit.setVal("sheet-link-ref", place.ref);
      showTab(internal ? "doc" : "web");
      kit.note("sheet-link-status", "");
      byId("sheet-link-dialog-title").textContent = link ? "하이퍼링크 고치기" : "하이퍼링크";
      byId("btn-sheet-link-apply").textContent = link ? "고치기" : "넣기";
      const remove = byId("btn-sheet-link-remove");
      if (remove) remove.hidden = !link;
      kit.openDialog("sheet-link-dialog");
      byId(internal ? "sheet-link-ref" : "sheet-link-address")?.focus?.();
      return true;
    }

    // The in-workbook target the 현재 문서 fields name, or null: a cell reference on the chosen sheet, or a defined name.
    function internalTargetOf(sheet, text) {
      const typed = String(text ?? "").trim();
      const at = names.parseAddress(typed);
      if (at) return links.internalTarget(sheet, at);
      const def = typed ? names.find(typed, sheet) : null;
      return def ? "#" + def.name : null;
    }

    // The address the dialog holds, or null with the reason on the status line.
    function chosenTarget() {
      const inside = state.tab === "doc";
      const target = inside
        ? internalTargetOf(kit.val("sheet-link-sheet") || state.sheet, kit.val("sheet-link-ref"))
        : links.normalizeTarget(kit.val("sheet-link-address"));
      const kept = target === null ? null : links.normalizeTarget(target);
      if (kept === null || links.isInternal(kept) !== inside) {
        kit.note("sheet-link-status", inside ? MESSAGES.reference : MESSAGES.address);
        return null;
      }
      return kept;
    }

    function textOf(target) {
      const typed = kit.val("sheet-link-text");
      if (typed !== "" || state.entry?.formula) return typed;
      return links.isInternal(target) ? links.locationOf(target) : target;
    }

    async function apply() {
      const target = chosenTarget();
      if (target === null) return false;
      const guard = host.__EASYOFFICE_SHEET_PROTECT__;
      if (guard && !guard.guardAction(state.sheet, "insertHyperlinks")) return false;
      const link = links.make(target, state.entry?.link?.tip);
      const formula = state.entry?.formula || "";
      const value = formula ? state.entry.value || "" : textOf(target);
      kit.closeDialog("sheet-link-dialog");
      const done = await applyCellEdits([{ ref: state.ref, value, formula, link }], state.entry?.link ? "하이퍼링크 고침" : "하이퍼링크");
      focusAnchor();
      return done;
    }

    async function remove() {
      const guard = host.__EASYOFFICE_SHEET_PROTECT__;
      kit.closeDialog("sheet-link-dialog");
      const { sheet, ref, entry } = loadCell();
      if (guard && !guard.guardAction(sheet, "insertHyperlinks")) return false;
      if (!entry?.link) {
        say(MESSAGES.nothing);
        return false;
      }
      const done = await applyCellEdits([{ ref, value: entry.value || "", formula: entry.formula || "" }], "하이퍼링크 제거");
      focusAnchor();
      return done;
    }

    function bind() {
      const viewport = byId("sheet-viewport");
      viewport?.addEventListener("mousedown", onMouseDown, true);
      viewport?.addEventListener("click", onClick);
      byId("btn-sheet-link-apply")?.addEventListener("click", () => void apply());
      byId("btn-sheet-link-remove")?.addEventListener("click", () => void remove());
      for (const tab of ["web", "doc"]) byId("sheet-link-tab-" + tab)?.addEventListener("click", () => showTab(tab));
      for (const id of ["sheet-link-address", "sheet-link-ref", "sheet-link-text"]) {
        byId(id)?.addEventListener("keydown", (ev) => {
          if (ev.key !== "Enter") return;
          ev.preventDefault();
          void apply();
        });
      }
    }

    return { open, follow, apply, remove, bind, actions: { "link-remove": remove } };
  }

  host.__EASYOFFICE_CREATE_SHEET_LINK_UI__ = createSheetLinkUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
