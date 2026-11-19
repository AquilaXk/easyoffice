// 참조 구현 (reference implementation): rhwp-studio `src/i18n/locales/ko.ts` lines 763-775
// (MIT, Copyright (c) 2025-2026 Edward Kim; see `THIRD_PARTY_NOTICES.md`) for Korean note dialog labels,
// paired with standard Korean spreadsheet cell note commands (`메모 삽입`, `메모 편집`, `메모 삭제`, `메모 표시/숨기기`, `새 메모`).

(function (host) {
  "use strict";

  const DEFAULT_AUTHOR = "작성자";
  const CARD_OFFSET_X = 6;
  const CARD_OFFSET_Y = -4;

  function createSheetNoteUi(deps) {
    const {
      document,
      grid,
      getActiveSheetName,
      getActiveModule,
      getSelection,
      getRange,
      selectRange,
      commitIfEditing,
      focusAnchor,
      enqueueWorkspaceHistoryOp,
      drainInFlightSheetWork,
      sheetHistory,
    } = deps;

    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const $ = (id) => document.getElementById(id);

    let bound = false;
    let windowKeyBound = false;
    let editingRef = "";
    let hoveredRef = "";
    let showAllWhenEmpty = false;

    function say(text) {
      const fb = $("status-command-feedback");
      if (fb) fb.textContent = text;
    }

    function setDialogStatus(msg) {
      const el = $("sheet-note-status");
      if (el) el.textContent = msg || "";
    }

    function isProtected(sheet, notifyUser) {
      const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;
      if (!protectApi?.isOn(sheet)) return false;
      const p = layoutApi.protectOf(sheet);
      if (Array.isArray(p?.deny) && !p.deny.includes("objects")) return false;
      if (notifyUser) {
        protectApi.alertBlocked?.();
        say("보호된 시트에서는 메모를 편집할 수 없습니다.");
      }
      return true;
    }

    function canonicalRef(r, c) {
      const m = grid.mergeAt(r, c);
      return m ? grid.refOf(m.r1, m.c1) : grid.refOf(r, c);
    }

    function activeCellRef() {
      const sel = getSelection?.() || getRange?.() || { ar: 1, ac: 1, r1: 1, c1: 1 };
      return canonicalRef(sel.ar || sel.r1 || 1, sel.ac || sel.c1 || 1);
    }

    function totalWorkbookNotes(excludeSheet) {
      const all = layoutApi.getAll();
      let count = 0;
      for (const sh of Object.keys(all)) {
        if (sh !== excludeSheet) count += (all[sh]?.notes || []).length;
      }
      return count;
    }

    function commitNotesMutation(sheet, nextNotes) {
      return enqueueWorkspaceHistoryOp(async () => {
        await drainInFlightSheetWork();
        commitIfEditing?.();
        const step = sheetHistory.begin({ parts: ["sheetLayout"] });
        layoutApi.setNotes(sheet, nextNotes);
        sheetHistory.commit(step);
      });
    }

    function openNoteDialog(targetRef = "") {
      ensureBound();
      const sheet = getActiveSheetName();
      if (isProtected(sheet, true)) return false;
      commitIfEditing?.();
      const ref = targetRef || activeCellRef();
      const existing = layoutApi.noteAtRef(sheet, ref);
      editingRef = ref;

      const titleEl = $("sheet-note-dialog-title");
      if (titleEl) titleEl.textContent = existing ? "메모 편집" : "메모 삽입";
      const delBtn = $("btn-sheet-note-delete");
      if (delBtn) delBtn.hidden = !existing;

      kit.setVal("sheet-note-author", existing ? existing.author || DEFAULT_AUTHOR : DEFAULT_AUTHOR);
      kit.setVal("sheet-note-text", existing ? existing.text : "");
      kit.setChecked("sheet-note-visible", existing ? existing.visible === true : showAllWhenEmpty);
      setDialogStatus("");

      if (!kit.openDialog("sheet-note-dialog")) return false;
      $("sheet-note-text")?.focus?.();
      return true;
    }

    async function applyNoteDialog() {
      const sheet = getActiveSheetName();
      if (isProtected(sheet, true)) {
        kit.closeDialog("sheet-note-dialog");
        return;
      }
      const ref = editingRef || activeCellRef();
      const author = layoutApi.trimUtf8(kit.val("sheet-note-author").trim() || DEFAULT_AUTHOR, layoutApi.MAX_NOTE_AUTHOR_BYTES);
      const text = layoutApi.trimUtf8(kit.val("sheet-note-text"), layoutApi.MAX_NOTE_TEXT_BYTES);
      if (!text.trim()) {
        setDialogStatus("메모 내용을 입력하세요.");
        return;
      }
      const visible = kit.checked("sheet-note-visible");
      const current = layoutApi.notesOf(sheet);
      const idx = current.findIndex((n) => n.ref === ref);
      if (idx < 0) {
        if (current.length >= layoutApi.MAX_NOTES_PER_SHEET) {
          setDialogStatus("시트당 셀 메모 수 한도(" + layoutApi.MAX_NOTES_PER_SHEET + "개)를 초과했습니다.");
          return;
        }
        if (totalWorkbookNotes(sheet) + current.length + 1 > layoutApi.MAX_NOTES_WORKBOOK) {
          setDialogStatus("통합 문서 셀 메모 수 한도(" + layoutApi.MAX_NOTES_WORKBOOK + "개)를 초과했습니다.");
          return;
        }
      }
      const entry = { ref, author, text, visible };
      const nextNotes = current.slice();
      if (idx >= 0) nextNotes[idx] = entry;
      else nextNotes.push(entry);

      await commitNotesMutation(sheet, nextNotes);
      const wasExisting = idx >= 0;
      editingRef = "";
      kit.closeDialog("sheet-note-dialog");
      focusAnchor?.();
      say(wasExisting ? "메모 편집" : "메모 삽입");
    }

    async function deleteNoteAt(targetRef = "") {
      const sheet = getActiveSheetName();
      if (isProtected(sheet, true)) return false;
      const ref = targetRef || activeCellRef();
      const current = layoutApi.notesOf(sheet);
      const next = current.filter((n) => n.ref !== ref);
      if (next.length === current.length) return false;
      if (hoveredRef === ref) hoveredRef = "";
      await commitNotesMutation(sheet, next);
      focusAnchor?.();
      say("메모 삭제");
      return true;
    }

    async function toggleSingleNote(targetRef = "") {
      const sheet = getActiveSheetName();
      if (isProtected(sheet, true)) return false;
      const ref = targetRef || activeCellRef();
      const current = layoutApi.notesOf(sheet);
      const idx = current.findIndex((n) => n.ref === ref);
      if (idx < 0) return false;
      const next = current.slice();
      const nextVis = !current[idx].visible;
      next[idx] = { ...current[idx], visible: nextVis };
      await commitNotesMutation(sheet, next);
      focusAnchor?.();
      say(nextVis ? "메모 표시" : "메모 숨기기");
      return true;
    }

    async function toggleAllNotes() {
      const sheet = getActiveSheetName();
      if (isProtected(sheet, true)) return false;
      const current = layoutApi.notesOf(sheet);
      if (!current.length) {
        showAllWhenEmpty = !showAllWhenEmpty;
        refreshMenu();
        say(showAllWhenEmpty ? "메모 모두 표시" : "메모 숨기기");
        return true;
      }
      const wantVisible = current.some((n) => !n.visible);
      const next = current.map((n) => ({ ...n, visible: wantVisible }));
      await commitNotesMutation(sheet, next);
      refreshMenu();
      focusAnchor?.();
      say(wantVisible ? "메모 모두 표시" : "메모 숨기기");
      return true;
    }

    function refreshMenu() {
      const item = document.querySelector('#menu-bar [data-sheet-act="note-toggle-all"]');
      if (!item) return;
      const notes = layoutApi.notesOf(getActiveSheetName());
      const checked = notes.length > 0 ? notes.every((n) => n.visible === true) : showAllWhenEmpty;
      item.setAttribute("aria-checked", checked ? "true" : "false");
    }

    function syncCellIndicators() {
      const sheet = getActiveSheetName();
      grid.forEachMaterializedTd((td) => {
        if (layoutApi.noteAtRef(sheet, td.dataset.cell)) td.dataset.note = "true";
        else if (td.dataset.note !== undefined) delete td.dataset.note;
      });
    }

    function cardPositionForRef(ref) {
      const p = grid.parseRef(ref);
      if (!p || grid.rowHidden(p.row) || grid.colHidden(p.col)) return null;
      const m = grid.mergeAt(p.row, p.col);
      const rightCol = m && m.r1 === p.row && m.c1 === p.col ? m.c2 + 1 : p.col + 1;
      const vp = $("sheet-viewport");
      return {
        left: grid.ROW_HEAD_W + grid.colLeft(rightCol) + CARD_OFFSET_X - (vp?.scrollLeft || 0),
        top: Math.max(2, grid.HEAD_H + grid.rowTop(p.row) + CARD_OFFSET_Y - (vp?.scrollTop || 0)),
      };
    }

    function renderNoteCard(note) {
      const pos = cardPositionForRef(note.ref);
      if (!pos) return null;
      const card = document.createElement("div");
      card.className = "sheet-note-card" + (note.visible ? " is-pinned" : "");
      card.dataset.noteRef = note.ref;
      card.style.left = pos.left + "px";
      card.style.top = pos.top + "px";

      const authorEl = document.createElement("strong");
      authorEl.className = "sheet-note-author";
      authorEl.textContent = (note.author || DEFAULT_AUTHOR) + ":";

      const bodyEl = document.createElement("div");
      bodyEl.className = "sheet-note-body";
      bodyEl.textContent = note.text;

      card.addEventListener("click", (ev) => {
        ev.stopPropagation?.();
        const p = grid.parseRef(note.ref);
        if (p) selectRange?.({ r1: p.row, c1: p.col, r2: p.row, c2: p.col });
        openNoteDialog(note.ref);
      });

      card.appendChild(authorEl);
      card.appendChild(bodyEl);
      return card;
    }

    function focusedGridRef() {
      const ae = document.activeElement;
      if (!ae || ae.tagName !== "TD" || !ae.dataset?.cell) return "";
      const p = grid.parseRef(ae.dataset.cell);
      return p ? canonicalRef(p.row, p.col) : "";
    }

    function syncScrollPositions() {
      const layer = $("sheet-note-layer");
      if (!layer || layer.hidden) return;
      for (const card of layer.querySelectorAll(".sheet-note-card")) {
        const pos = cardPositionForRef(card.dataset.noteRef || "");
        if (!pos) continue;
        card.style.left = pos.left + "px";
        card.style.top = pos.top + "px";
      }
    }

    function syncLayer() {
      syncCellIndicators();
      refreshMenu();
      const layer = $("sheet-note-layer");
      if (!layer) return;
      if ((getActiveModule && getActiveModule() !== "SHEET") || $("pane-sheet")?.hidden) {
        layer.replaceChildren();
        layer.hidden = true;
        return;
      }
      const notes = layoutApi.notesOf(getActiveSheetName());
      if (!notes.length) {
        layer.replaceChildren();
        layer.hidden = true;
        return;
      }
      const focusRef = focusedGridRef();
      const cards = [];
      for (const note of notes) {
        if (note.visible || note.ref === hoveredRef || note.ref === focusRef) {
          const card = renderNoteCard(note);
          if (card) cards.push(card);
        }
      }
      layer.replaceChildren(...cards);
      layer.hidden = cards.length === 0;
    }

    function handleShiftF2(ev) {
      if (ev.defaultPrevented || ev.__eoNoteHandled) return;
      if (!(!ev.ctrlKey && !ev.metaKey && !ev.altKey && ev.shiftKey && ev.key === "F2")) return;
      if ((getActiveModule && getActiveModule() !== "SHEET") || $("pane-sheet")?.hidden) return;
      if (document.querySelector?.("dialog[open]")) return;
      ev.__eoNoteHandled = true;
      ev.preventDefault?.();
      ev.stopImmediatePropagation?.();
      openNoteDialog();
    }

    function ensureBound() {
      if (bound) return;
      bound = true;
      const vp = $("sheet-viewport");
      vp?.addEventListener("keydown", handleShiftF2);
      vp?.addEventListener("scroll", syncScrollPositions);
      vp?.addEventListener("mouseover", (ev) => {
        const td = ev.target?.closest?.("td[data-cell]");
        const p = td ? grid.parseRef(td.dataset.cell) : null;
        const next = p ? canonicalRef(p.row, p.col) : "";
        if (next !== hoveredRef) {
          hoveredRef = next;
          syncLayer();
        }
      });
      vp?.addEventListener("mouseleave", () => {
        if (hoveredRef) {
          hoveredRef = "";
          syncLayer();
        }
      });
      vp?.addEventListener("focusin", syncLayer);
      vp?.addEventListener("focusout", syncLayer);

      if (!windowKeyBound && typeof host.addEventListener === "function") {
        windowKeyBound = true;
        host.addEventListener("keydown", (ev) => host.__EASYOFFICE_SHEET_NOTE__?._handleShiftF2?.(ev));
      }

      $("btn-sheet-note-apply")?.addEventListener("click", () => {
        void applyNoteDialog().catch(() => {});
      });
      $("btn-sheet-note-delete")?.addEventListener("click", () => {
        const ref = editingRef;
        editingRef = "";
        kit.closeDialog("sheet-note-dialog");
        if (ref) void deleteNoteAt(ref).catch(() => {});
      });
      layoutApi.onChange(syncLayer);
    }

    const actions = {
      "note-insert": () => Promise.resolve(openNoteDialog()),
      "note-edit": () => Promise.resolve(openNoteDialog()),
      "note-delete": () => deleteNoteAt(),
      "note-toggle": () => toggleSingleNote(),
      "note-toggle-all": () => toggleAllNotes(),
    };

    const api = {
      bind: ensureBound,
      actions,
      openNoteDialog,
      deleteNoteAt,
      toggleSingleNote,
      toggleAllNotes,
      refreshMenu,
      syncLayer,
      _handleShiftF2: handleShiftF2,
    };
    host.__EASYOFFICE_SHEET_NOTE__ = api;
    return api;
  }

  host.__EASYOFFICE_CREATE_SHEET_NOTE_UI__ = createSheetNoteUi;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
