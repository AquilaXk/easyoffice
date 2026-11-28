(function (host) {
  "use strict";

  // General cell right-click context menu and cell insert/delete shift dialog (PD-356).
  // Link/note-only cells retain `#sheet-header-menu` for compatibility with link/note invariants,
  // while ordinary cells open `#sheet-cell-menu` matching `.context-menu` tokens.
  const MENU_MARGIN = 2;
  const MAX_SHIFT_CELLS = 100000;

  function createContextMenu(deps) {
    const {
      document,
      kit,
      grid,
      sheetDataStore,
      getActiveSheetName,
      getRange,
      getRanges,
      getSelection,
      selectRange,
      clearExtras,
      commitIfEditing,
      focusAnchor,
      snapshotDomSheetCells,
      applyCellEdits,
      shiftFormula,
      runStructureAct,
      editAxis,
      runClipboardCommand,
      openPasteSpecial,
      openFormatDialog,
      clearSelectionContents,
      filter,
      quickSort,
      pickList,
      say,
    } = deps;
    const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
    const protectApi = host.__EASYOFFICE_SHEET_PROTECT__;
    const multiApi = host.__EASYOFFICE_SHEET_MULTI_RANGE__;
    const { $, openDialog, closeDialog, note } = kit;
    const menuEl = () => $("sheet-cell-menu");
    let shiftMode = "insert";
    let shiftBound = false;

    // The lines of the menu itself; the items of a submenu are reached through its line (ArrowRight).
    const visibleItems = () => Array.from(menuEl()?.querySelectorAll(":scope > .md-item, :scope > .md-sub") || []).filter((el) => !el.hidden && el.getAttribute("aria-disabled") !== "true");
    const subItems = (sub) => Array.from(sub.querySelectorAll(".md-sub-panel .md-item")).filter((el) => el.getAttribute("aria-disabled") !== "true");

    function close(refocus) {
      const el = menuEl();
      if (!el || el.hidden) return;
      el.hidden = true;
      for (const sub of el.querySelectorAll(".md-sub.open")) sub.classList.remove("open");
      if (refocus) focusAnchor();
    }

    function place(el, x, y) {
      el.style.maxHeight = "";
      el.style.left = "0px";
      el.style.top = "0px";
      const w = el.offsetWidth;
      const h = el.offsetHeight;
      const vw = host.innerWidth || document.documentElement.clientWidth;
      const vh = host.innerHeight || document.documentElement.clientHeight;
      if (h > vh - MENU_MARGIN) el.style.maxHeight = vh - MENU_MARGIN + "px";
      const height = Math.min(h, vh - MENU_MARGIN);
      el.style.left = Math.max(0, Math.min(x, vw - w - MENU_MARGIN)) + "px";
      el.style.top = Math.max(0, Math.min(y, vh - height - MENU_MARGIN)) + "px";
    }

    function openAt(x, y) {
      const el = menuEl();
      if (!el) return false;
      host.__EASYOFFICE_CLOSE_MENUS__?.(document);
      syncSortLabels();
      el.hidden = false;
      place(el, x, y);
      visibleItems()[0]?.focus();
      return true;
    }

    function onCellContextMenu(ev) {
      const td = ev.target?.closest?.("#pane-sheet td[data-cell]");
      if (!td) return false;
      const hasLink = Boolean(td.dataset?.link && td._link);
      const hasNote = td.dataset?.note === "true";
      if (hasLink || hasNote) return false;
      ev.preventDefault();
      commitIfEditing();
      const row = td._row || grid.parseRef(td.dataset.cell)?.row || 1;
      const col = td._col || grid.parseRef(td.dataset.cell)?.col || 1;
      const ranges = getRanges();
      const inside = multiApi ? multiApi.anyContainsCoord(ranges, row, col) : (row >= ranges[0].r1 && row <= ranges[0].r2 && col >= ranges[0].c1 && col <= ranges[0].c2);
      if (!inside) {
        clearExtras();
        selectRange({ r1: row, c1: col, r2: row, c2: col });
      }
      return openAt(ev.clientX, ev.clientY);
    }

    function onKeydownViewport(ev) {
      const sel = getSelection();
      if (sel.axis) return;
      if (!(ev.key === "ContextMenu" || (ev.key === "F10" && ev.shiftKey))) return;
      const ae = document.activeElement;
      if (ae?.closest?.("input, textarea, select") || (ae?.dataset?.cell && ae.dataset?.editMode)) return;
      ev.preventDefault();
      const td = grid.getTdAt?.(sel.ar, sel.ac) || document.querySelector('#pane-sheet td[data-cell="' + grid.refOf(sel.ar, sel.ac) + '"]');
      const box = td?.getBoundingClientRect?.();
      openAt(box ? box.left + 8 : 20, box ? box.bottom : 20);
    }

    // Inside a submenu the arrows walk its items, Left and Escape go back to its line.
    function onSubKeydown(ev, sub) {
      const items = subItems(sub);
      const index = items.indexOf(document.activeElement);
      const move = (to) => {
        ev.preventDefault();
        items[(to + items.length) % items.length]?.focus();
      };
      if (ev.key === "ArrowDown") move(index + 1);
      else if (ev.key === "ArrowUp") move(index - 1);
      else if (ev.key === "Home") move(0);
      else if (ev.key === "End") move(-1);
      else if (ev.key === "ArrowLeft" || ev.key === "Escape") {
        ev.preventDefault();
        ev.stopPropagation?.();
        sub.classList.remove("open");
        sub.focus?.();
      } else if (ev.key === "Enter" || ev.key === " ") {
        ev.preventDefault();
        document.activeElement?.click?.();
      }
    }

    function onMenuKeydown(ev) {
      const inSub = document.activeElement?.closest?.(".md-sub");
      if (inSub && document.activeElement !== inSub && menuEl()?.contains(inSub)) {
        onSubKeydown(ev, inSub);
        return;
      }
      const items = visibleItems();
      if (items.length === 0) return;
      const index = items.indexOf(document.activeElement);
      const line = document.activeElement;
      if (line?.classList?.contains("md-sub") && (ev.key === "ArrowRight" || ev.key === "Enter" || ev.key === " ")) {
        ev.preventDefault();
        line.classList.add("open");
        subItems(line)[0]?.focus();
        return;
      }
      const move = (to) => {
        ev.preventDefault();
        items[(to + items.length) % items.length]?.focus();
      };
      if (ev.key === "ArrowDown") move(index + 1);
      else if (ev.key === "ArrowUp") move(index - 1);
      else if (ev.key === "Home") move(0);
      else if (ev.key === "End") move(-1);
      else if (ev.key === "Escape") {
        ev.preventDefault();
        close(true);
      } else if (ev.key === "Tab") {
        ev.preventDefault();
        close(true);
      } else if (ev.key === "Enter" || ev.key === " ") {
        ev.preventDefault();
        document.activeElement?.click?.();
      }
    }

    function shiftHorizontal(dir) {
      const sheet = getActiveSheetName();
      const rg = getRange();
      const count = rg.c2 - rg.c1 + 1;
      const insert = dir === "right";
      if (protectApi && !protectApi.guardAction(sheet, insert ? "insertColumns" : "deleteColumns")) {
        return Promise.resolve(false);
      }
      const map = snapshotDomSheetCells();
      sheetDataStore[sheet] = map;
      if (layoutApi?.mergesIn(sheet, rg.r1, rg.c1, rg.r2, grid.MAX_COLS).length > 0 || layoutApi?.tablesIn(sheet, rg.r1, rg.c1, rg.r2, grid.MAX_COLS).length > 0) {
        say("병합 셀 또는 표와 겹쳐 이동할 수 없음");
        return Promise.resolve(false);
      }
      let maxCol = rg.c2;
      const rowEntries = [];
      for (const ref of Object.keys(map)) {
        const p = grid.parseRef(ref);
        if (!p || p.row < rg.r1 || p.row > rg.r2 || p.col < rg.c1) continue;
        if (p.col > maxCol) maxCol = p.col;
        const entry = map[ref];
        if (entry?.spillOf) {
          say("배열 수식 셀은 이동할 수 없음");
          return Promise.resolve(false);
        }
        rowEntries.push({ row: p.row, col: p.col, entry });
        if (rowEntries.length > MAX_SHIFT_CELLS) {
          say("이동할 셀이 너무 많음");
          return Promise.resolve(false);
        }
      }
      if (insert && maxCol + count > grid.MAX_COLS) {
        say("시트 열 범위 초과");
        return Promise.resolve(false);
      }
      const editByRef = new Map();
      for (const item of rowEntries) {
        const srcRef = grid.refOf(item.row, item.col);
        if (!editByRef.has(srcRef)) editByRef.set(srcRef, { ref: srcRef, value: "", formula: "", style: null, link: null });
        if (!insert && item.col <= rg.c2) continue;
        const dstCol = insert ? item.col + count : item.col - count;
        const dstRef = grid.refOf(item.row, dstCol);
        const dstFormula = item.entry.formula ? shiftFormula(item.entry.formula, 0, insert ? count : -count) : "";
        editByRef.set(dstRef, {
          ref: dstRef,
          value: dstFormula ? "" : String(item.entry.value ?? ""),
          formula: dstFormula,
          style: item.entry.style ?? null,
          link: item.entry.link ?? null,
          ...(item.entry.t && !dstFormula ? { t: item.entry.t } : {}),
        });
      }
      const edits = Array.from(editByRef.values());
      if (edits.length === 0) return Promise.resolve(false);
      return applyCellEdits(edits, insert ? "오른쪽으로 셀 밀기" : "왼쪽으로 셀 밀기");
    }

    function openShiftDialog(nextMode) {
      ensureShiftBound();
      shiftMode = nextMode;
      const isIns = nextMode === "insert";
      const title = $("sheet-cell-shift-dialog-title");
      const hLabel = $("sheet-shift-horiz-label");
      const vLabel = $("sheet-shift-vert-label");
      const vRadio = $("sheet-shift-vert");
      if (title) title.textContent = isIns ? "삽입" : "삭제";
      if (hLabel) hLabel.textContent = isIns ? "오른쪽으로 셀 밀기" : "왼쪽으로 셀 밀기";
      if (vLabel) vLabel.textContent = isIns ? "아래로 셀 밀기" : "위로 셀 밀기";
      if (vRadio) vRadio.checked = true;
      note("sheet-cell-shift-status", "");
      openDialog("sheet-cell-shift-dialog");
    }

    function submitShiftDialog() {
      const checked = Array.from(document.querySelectorAll('input[name="sheet-cell-shift-kind"]')).find((el) => el.checked)?.value || "vert";
      const rg = getRange();
      const isIns = shiftMode === "insert";
      closeDialog("sheet-cell-shift-dialog");
      if (checked === "row") {
        void editAxis("row", isIns ? "insert" : "delete", rg.r1, rg.r2 - rg.r1 + 1);
      } else if (checked === "col") {
        void editAxis("col", isIns ? "insert" : "delete", rg.c1, rg.c2 - rg.c1 + 1);
      } else if (checked === "vert") {
        void editAxis("row", isIns ? "insert" : "delete", rg.r1, rg.r2 - rg.r1 + 1, null, [rg.c1, rg.c2]);
      } else {
        void shiftHorizontal(isIns ? "right" : "left");
      }
    }

    function ensureShiftBound() {
      if (shiftBound) return;
      shiftBound = true;
      $("btn-sheet-cell-shift-ok")?.addEventListener("click", submitShiftDialog);
    }

    // 필터 > 선택한 셀의 값, 색, 글꼴 색으로 필터링: the filter turns on for the region around the cell when it is off, and the
    // column of the active cell keeps the rows that look like it.
    async function filterBy(kind) {
      commitIfEditing();
      const sel = getSelection();
      const entry = snapshotDomSheetCells()[grid.refOf(sel.ar, sel.ac)];
      const text = String(entry?.value ?? "");
      const rgb = String(entry?.style?.[kind === "font" ? "fontColor" : "fill"] ?? "").replace(/^#/, "").toLowerCase();
      const criteria =
        kind === "value"
          ? text === "" ? { blank: true, values: [] } : { values: [text] }
          : { color: { ...(kind === "font" ? { font: true } : {}), ...(rgb ? { rgb } : {}) } };
      if (!filter.current() && !(await filter.run("filter-toggle"))) return false;
      return filter.setCriteria(sel.ac, criteria);
    }

    // A column of numbers sorts "smallest to largest"; any other, as text.
    function syncSortLabels() {
      const sel = getSelection();
      const entry = sheetDataStore[getActiveSheetName()]?.[grid.refOf(sel.ar, sel.ac)];
      const numeric =Boolean(entry) && !entry.formula && String(entry.value ?? "").trim() !== "" && Number.isFinite(Number(String(entry.value).replaceAll(",", "")));
      const labels = numeric ? ["가장 작은 값에서 가장 큰 값으로 정렬", "가장 큰 값에서 가장 작은 값으로 정렬"] : ["텍스트 오름차순 정렬", "텍스트 내림차순 정렬"];
      for (const [id, text] of [["sheet-ctx-sort-asc", labels[0]], ["sheet-ctx-sort-desc", labels[1]]]) {
        const el = $(id);
        if (el) el.textContent = text;
      }
    }

    const ACTIONS = {
      "filter-clear": () => void runStructureAct("filter-clear"),
      "filter-reapply": () => void runStructureAct("filter-reapply"),
      "filter-by-value": () => void filterBy("value"),
      "filter-by-fill": () => void filterBy("fill"),
      "filter-by-font": () => void filterBy("font"),
      "sort-asc": () => void quickSort(true),
      "sort-desc": () => void quickSort(false),
      "sort-custom": () => void runStructureAct("sort-dialog"),
      "pick-list": () => void pickList.open(),
      "clip-cut": () => void runClipboardCommand("clip-cut"),
      "clip-copy": () => void runClipboardCommand("clip-copy"),
      "clip-paste": () => void runClipboardCommand("clip-paste"),
      "paste-special": () => void openPasteSpecial(),
      "cell-insert": () => openShiftDialog("insert"),
      "cell-delete": () => openShiftDialog("delete"),
      "clear-contents": () => void clearSelectionContents(),
      "cell-format": () => void openFormatDialog(),
      "note-insert": () => void runStructureAct("note-insert"),
      "link-edit": () => void runStructureAct("link-edit"),
    };

    function bind() {
      const el = menuEl();
      const viewport = $("sheet-viewport");
      viewport?.addEventListener("keydown", onKeydownViewport);
      viewport?.addEventListener("scroll", () => close(false));
      el?.addEventListener("keydown", onMenuKeydown);
      el?.addEventListener("contextmenu", (ev) => ev.preventDefault?.());
      for (const item of el?.querySelectorAll(".md-item[data-sheet-ctx]") || []) {
        item.addEventListener("click", (ev) => {
          ev.stopPropagation?.();
          close(true);
          if (item.getAttribute("aria-disabled") === "true") return;
          ACTIONS[item.dataset.sheetCtx]?.();
        });
      }
      document.addEventListener("mousedown", (ev) => {
        const cur = menuEl();
        if (cur && !cur.hidden && !cur.contains(ev.target)) close(false);
      });
      document.addEventListener("keydown", (ev) => {
        if (ev.key === "Escape" && !menuEl()?.hidden) {
          ev.preventDefault?.();
          ev.stopPropagation?.();
          close(true);
        }
      });
      host.__EASYOFFICE_ON_MENUS_CLOSED__?.(document, () => close(false));
      host.addEventListener?.("resize", () => close(false));
      host.addEventListener?.("blur", () => close(false));
    }

    return { bind, close, onCellContextMenu, openAt };
  }

  host.__EASYOFFICE_CREATE_SHEET_CONTEXT_MENU__ = createContextMenu;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
