(function (host) {
  "use strict";

  // 드롭다운 목록에서 선택 of the cell menu (PD-391): a list of the different values the column holds right above and below the
  // active cell (the filled cells that touch it, as the spreadsheet program reads them), sorted by their text. A pick is written
  // into the cell as one undoable edit.
  const MAX_VALUES = 500;
  const MAX_WALK = 10000;
  const MENU_MARGIN = 2;

  function createSheetPickList(deps) {
    const { document, grid, sheetDataStore, getActiveSheetName, getSelection, commitIfEditing, snapshotDomSheetCells, applyCellEdits, focusAnchor, say } = deps;
    const menu = () => document.getElementById("sheet-pick-menu");
    const items = () => Array.from(menu()?.querySelectorAll(".md-item") || []);

    function close(refocus) {
      const el = menu();
      if (!el || el.hidden) return;
      el.hidden = true;
      el.replaceChildren();
      if (refocus) focusAnchor();
    }

    // The values of the cells above and below `row` in `col`, up to the first empty cell on each side.
    function valuesAround(map, row, col) {
      const seen = new Set();
      const read = (r) => {
        const entry = map[grid.refOf(r, col)];
        const text = String(entry?.value ?? "");
        if (text === "") return false;
        seen.add(text);
        return true;
      };
      for (const step of [-1, 1]) {
        let r = row + step;
        for (let n = 0; n < MAX_WALK && r >= 1 && r <= grid.MAX_ROWS; n += 1, r += step) {
          if (!read(r)) break;
        }
      }
      return [...seen].sort((a, b) => a.localeCompare(b, "ko")).slice(0, MAX_VALUES);
    }

    function pick(text) {
      const sel = getSelection();
      close(true);
      return applyCellEdits([{ ref: grid.refOf(sel.ar, sel.ac), value: text, formula: "" }], "드롭다운 목록에서 선택");
    }

    function open() {
      commitIfEditing();
      const sel = getSelection();
      const map = snapshotDomSheetCells();
      sheetDataStore[getActiveSheetName()] = map;
      const values = valuesAround(map, sel.ar, sel.ac);
      const el = menu();
      if (!el) return false;
      if (values.length === 0) {
        say("목록에 쓸 값이 없음");
        return false;
      }
      el.replaceChildren(
        ...values.map((text) => {
          const item = document.createElement("div");
          item.className = "md-item";
          item.setAttribute("role", "option");
          item.tabIndex = -1;
          const label = document.createElement("span");
          label.className = "md-label";
          label.textContent = text;
          item.appendChild(label);
          item.addEventListener("click", () => void pick(text));
          return item;
        })
      );
      host.__EASYOFFICE_CLOSE_MENUS__?.(document);
      el.hidden = false;
      const td = grid.getTdAt?.(sel.ar, sel.ac) || document.querySelector('#pane-sheet td[data-cell="' + grid.refOf(sel.ar, sel.ac) + '"]');
      const box = td?.getBoundingClientRect?.();
      const vw = host.innerWidth || document.documentElement.clientWidth;
      const vh = host.innerHeight || document.documentElement.clientHeight;
      el.style.maxHeight = Math.max(80, vh - MENU_MARGIN * 2) + "px";
      el.style.left = Math.max(0, Math.min(box ? box.left : 20, vw - el.offsetWidth - MENU_MARGIN)) + "px";
      el.style.top = Math.max(0, Math.min(box ? box.bottom : 20, vh - Math.min(el.offsetHeight, vh) - MENU_MARGIN)) + "px";
      items()[0]?.focus();
      return true;
    }

    function onKeydown(ev) {
      const list = items();
      const index = list.indexOf(document.activeElement);
      const move = (to) => {
        ev.preventDefault();
        list[(to + list.length) % list.length]?.focus();
      };
      if (ev.key === "ArrowDown") move(index + 1);
      else if (ev.key === "ArrowUp") move(index - 1);
      else if (ev.key === "Home") move(0);
      else if (ev.key === "End") move(-1);
      else if (ev.key === "Escape" || ev.key === "Tab") {
        ev.preventDefault();
        ev.stopPropagation?.();
        close(true);
      } else if (ev.key === "Enter" || ev.key === " ") {
        ev.preventDefault();
        document.activeElement?.click?.();
      }
    }

    function bind() {
      const el = menu();
      el?.addEventListener("keydown", onKeydown);
      el?.addEventListener("contextmenu", (ev) => ev.preventDefault?.());
      document.addEventListener("mousedown", (ev) => {
        const cur = menu();
        if (cur && !cur.hidden && !cur.contains(ev.target)) close(false);
      });
      host.__EASYOFFICE_ON_MENUS_CLOSED__?.(document, () => close(false));
      host.addEventListener?.("resize", () => close(false));
      host.addEventListener?.("blur", () => close(false));
    }

    return { open, close, bind, valuesAround };
  }

  host.__EASYOFFICE_CREATE_SHEET_PICK_LIST__ = createSheetPickList;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
