(function (host) {
  "use strict";

  // What a sheet tab shows and answers to (PD-209): the delete confirmation and the list of hidden
  // sheets (static markup of `index.html`, laid out like the editor's other dialogs), the name
  // typed in place of the tab, and dragging a tab to another place.
  //
  // 참조 구현: the reference engine's `sheet-bar-tabs/utils/sheet-tab-name-editor.ts` (see THIRD_PARTY_NOTICES.md)
  // (the name is edited in the tab: Enter keeps it, Escape leaves it, a name over 31 units is cut,
  // blanks of a pasted name go) and `.../utils/sheet-tab-drag-sort.ts` lines 17-90 (the dragged tab
  // follows the pointer and the tabs it passes make room, the target is where its edge passes the
  // middle of a tab), Apache-2.0, Copyright 2023-present DreamNum Co., Ltd., commit
  // 5a06a7842c8e13fa0646d0733dc3662cf4b330b9. Modifications: plain DOM elements and mouse events;
  // the target is counted among the tabs that are shown.
  //
  // The slot a Ctrl drag copies to is shown as another reference does, a 2px mark on the left edge of the tab the
  // pointer is before and the drop that puts the sheet before that tab, or last (`packages/react/src/components/
  // SheetTab/SheetItem.tsx` lines 99-140 and 209, MIT, Copyright (c) 2022 Suzhou Ruilisi Technology Co., Ltd,
  // commit 94346608877db4747406707a177c4b8f3bacdbf9). That reference moves the dragged sheet only; the copy by
  // the held key is added here.

  const names = host.__EASYOFFICE_SHEET_TAB_NAMES__;
  const DRAG_START_PX = 4;

  // The slot a pointer at `x` drops a copy in: the index of the first tab whose middle is right of it (the count of
  // tabs for the end).
  function dropSlot(items, x) {
    const at = items.findIndex((item) => x < item.left + item.width / 2);
    return at < 0 ? items.length : at;
  }

  // Where a tab of `items` (`{ left, width }`) dragged `dragX` pixels lands, and how far each other
  // tab moves to make room.
  function dragTarget(items, activeIndex, dragX) {
    const offsets = items.map(() => 0);
    const active = items[activeIndex];
    if (!active) return { target: activeIndex, offsets };
    offsets[activeIndex] = dragX;
    const left = active.left + dragX;
    const right = left + active.width;
    const middle = (item) => item.left + item.width / 2;
    let target = activeIndex;
    if (dragX > 0) {
      for (let i = activeIndex + 1; i < items.length; i += 1) {
        if (right > middle(items[i])) {
          offsets[i] = -active.width;
          target = i;
        }
      }
    } else if (dragX < 0) {
      for (let i = activeIndex - 1; i >= 0; i -= 1) {
        if (left < middle(items[i])) {
          offsets[i] = active.width;
          target = i;
        }
      }
    }
    return { target, offsets };
  }

  function createSheetTabUi({ document, kit, getTabs, say }) {
    const { $, modal } = kit;
    const bar = () => $("sheet-tab-bar");
    const tabButtons = () => Array.from(bar()?.querySelectorAll(".sheet-tab-btn[data-sheet-tab]") || []);
    const tabOf = (name) => tabButtons().find((b) => b.dataset.sheetTab === name) || null;

    // ---- dialogs ------------------------------------------------------------------------------------
    // Asks the question of `#sheet-tab-delete-dialog`; resolves true on 확인, false on anything else.
    async function confirmDelete(label) {
      if (!$("sheet-tab-delete-dialog")) return true;
      $("sheet-tab-delete-message").textContent = "'" + label + "' 시트를 삭제하시겠습니까?";
      const answer = await modal("sheet-tab-delete-dialog", "btn-sheet-tab-delete-ok", () => true, null, () => $("btn-sheet-tab-delete-ok").focus());
      return answer === true;
    }

    // A list of `<li>` items that one or several can be picked from: a click, the arrow keys, Space, and Enter or a
    // double click that finish. `multiple` lets Ctrl, Shift and Space add to the pick. `bindList(finish)` binds the
    // events and returns what unbinds them; `picked()` gives the `data-sheet` of the picked items.
    function listPicker(list, multiple) {
      const items = () => Array.from(list.querySelectorAll("li"));
      const select = (item, additive) => {
        if (!additive) items().forEach((other) => other.setAttribute("aria-selected", "false"));
        item.setAttribute("aria-selected", additive ? String(item.getAttribute("aria-selected") !== "true") : "true");
        item.focus();
      };
      const picked = () => items().filter((i) => i.getAttribute("aria-selected") === "true").map((i) => i.dataset.sheet);
      const bindList = (finish) => {
        const onClick = (ev) => {
          const item = ev.target?.closest?.("li");
          if (item) select(item, multiple && (ev.ctrlKey || ev.metaKey || ev.shiftKey));
        };
        const onDouble = (ev) => {
          const item = ev.target?.closest?.("li");
          if (!item) return;
          select(item, false);
          finish(picked());
        };
        const onKey = (ev) => {
          const all = items();
          const at = all.indexOf(document.activeElement);
          if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
            ev.preventDefault();
            select(all[Math.max(0, Math.min(all.length - 1, at + (ev.key === "ArrowDown" ? 1 : -1)))], false);
          } else if (ev.key === " " && at >= 0 && multiple) {
            ev.preventDefault();
            select(all[at], true);
          } else if (ev.key === "Enter" && picked().length > 0) {
            ev.preventDefault();
            finish(picked());
          }
        };
        list.addEventListener("click", onClick);
        list.addEventListener("dblclick", onDouble);
        list.addEventListener("keydown", onKey);
        return () => {
          list.removeEventListener("click", onClick);
          list.removeEventListener("dblclick", onDouble);
          list.removeEventListener("keydown", onKey);
        };
      };
      return { items, select, picked, bindList };
    }

    function fillList(list, entries) {
      list.replaceChildren();
      for (const [name, label] of entries) {
        const item = document.createElement("li");
        item.setAttribute("role", "option");
        item.setAttribute("aria-selected", "false");
        item.tabIndex = -1;
        item.dataset.sheet = name;
        item.textContent = label;
        list.appendChild(item);
      }
    }

    // Lists `hidden` in `#sheet-tab-unhide-dialog`; resolves the names the user picked, or null.
    function pickHidden(hidden) {
      const list = $("sheet-tab-unhide-list");
      if (!$("sheet-tab-unhide-dialog") || !list || hidden.length === 0) return Promise.resolve(null);
      fillList(list, hidden.map((name) => [name, names.labelOf(name)]));
      const { items, select, picked, bindList } = listPicker(list, true);
      select(items()[0], false);
      return modal("sheet-tab-unhide-dialog", "btn-sheet-tab-unhide-ok", () => (picked().length > 0 ? picked() : null), bindList, () => items()[0].focus());
    }

    // Asks where `sheet` goes in `#sheet-tab-move-dialog` (이동/복사): the sheet it goes before, `shown` being the
    // sheets with a tab and the last entry 끝으로 이동, and whether to copy. `before` is the entry that is picked
    // first. Resolves `{ before, copy }` (`before` null for the end) or null.
    function pickPlace(shown, before) {
      const list = $("sheet-tab-move-list");
      if (!$("sheet-tab-move-dialog") || !list) return Promise.resolve(null);
      fillList(list, [...shown.map((name) => [name, names.labelOf(name)]), ["", "(끝으로 이동)"]]);
      $("sheet-tab-move-copy").checked = false;
      const { items, select, picked, bindList } = listPicker(list, false);
      const first = items().find((i) => i.dataset.sheet === (before ?? "")) || items().at(-1);
      select(first, false);
      const answer = () => {
        const [name] = picked();
        return name === undefined ? null : { before: name === "" ? null : name, copy: $("sheet-tab-move-copy").checked };
      };
      return modal("sheet-tab-move-dialog", "btn-sheet-tab-move-ok", answer, (finish) => bindList(() => finish(answer())), () => first.focus());
    }

    // ---- the name, typed in place of the tab -----------------------------------------------------------
    let editing = null;

    function startRename(sheet) {
      if (host.__EASYOFFICE_SHEET_PROTECT__?.guardStructure() === false) return false;
      const tab = tabOf(sheet);
      if (!tab || editing) return false;
      const input = document.createElement("input");
      input.type = "text";
      input.className = "sheet-tab-edit";
      input.value = names.labelOf(sheet);
      input.setAttribute("aria-label", "시트 이름");
      input.maxLength = names.MAX_NAME_UNITS;
      input.style.width = Math.max(tab.getBoundingClientRect().width, 72) + "px";
      tab.hidden = true;
      tab.before(input);
      let finished = false;
      const finish = async (commit) => {
        if (finished) return;
        finished = true;
        editing = null;
        input.removeEventListener("blur", onBlur);
        const text = input.value;
        input.remove();
        tab.hidden = false;
        if (!commit) {
          tab.focus();
          return;
        }
        if (!(await getTabs().rename(sheet, text))) tabOf(sheet)?.focus();
      };
      const onBlur = () => {
        // A name that is not good is left, with the reason in the status text.
        const problem = getTabs().checkRename(sheet, input.value.trim());
        if (problem) say(problem);
        void finish(!problem);
      };
      input.addEventListener("keydown", (ev) => {
        ev.stopPropagation?.();
        if (ev.key === "Escape") {
          ev.preventDefault();
          void finish(false);
        } else if (ev.key === "Enter") {
          ev.preventDefault();
          const problem = getTabs().checkRename(sheet, input.value.trim());
          if (problem) {
            say(problem);
            input.setAttribute("aria-invalid", "true");
          } else void finish(true);
        } else if (input.getAttribute("aria-invalid") === "true") {
          input.removeAttribute("aria-invalid");
        }
      });
      // A pasted name loses its blanks at the ends.
      input.addEventListener("paste", (ev) => {
        const text = ev.clipboardData?.getData?.("text/plain");
        if (typeof text !== "string") return;
        ev.preventDefault();
        const start = input.selectionStart ?? input.value.length;
        const end = input.selectionEnd ?? start;
        input.value = (input.value.slice(0, start) + text.trim() + input.value.slice(end)).slice(0, names.MAX_NAME_UNITS);
      });
      input.addEventListener("blur", onBlur);
      editing = { input, finish };
      input.focus();
      input.select?.();
      return true;
    }

    function focusTab(sheet) {
      tabOf(sheet)?.focus();
    }

    // ---- dragging a tab -----------------------------------------------------------------------------
    // A press on a tab that moves more than a few pixels drags it; the tabs it passes shift to make
    // room, and letting go moves the sheet (or copies it, with Ctrl held). The click that ends the drag is not a tab switch.
    let drag = null;
    let swallowClick = false;

    function clearDrag() {
      bar()?.classList.remove("copying");
      for (const btn of tabButtons()) {
        btn.style.transform = "";
        btn.classList.remove("dragging", "drop-before", "drop-end");
      }
    }

    function onPress(ev) {
      const tab = ev.target?.closest?.(".sheet-tab-btn[data-sheet-tab]");
      if (!tab || ev.button > 0 || editing) return;
      const buttons = tabButtons();
      drag = {
        tab,
        index: buttons.indexOf(tab),
        startX: ev.clientX,
        items: buttons.map((b) => {
          const box = b.getBoundingClientRect();
          return { left: box.left, width: box.width };
        }),
        moved: false,
        target: buttons.indexOf(tab),
      };
    }

    function onMove(ev) {
      if (!drag) return;
      const dx = ev.clientX - drag.startX;
      if (!drag.moved && Math.abs(dx) < DRAG_START_PX) return;
      drag.moved = true;
      // With Ctrl (Option on a Mac, as that program's help says) held the tab is copied: the tabs stay and a mark shows
      // the slot (PD-209).
      drag.copy = Boolean(ev.ctrlKey || ev.altKey);
      bar()?.classList.toggle("copying", drag.copy);
      if (drag.copy) {
        drag.slot = dropSlot(drag.items, ev.clientX);
        tabButtons().forEach((btn, i) => {
          btn.style.transform = "";
          btn.classList.remove("dragging");
          btn.classList.toggle("drop-before", i === drag.slot);
          btn.classList.toggle("drop-end", drag.slot === drag.items.length && i === drag.items.length - 1);
        });
        return;
      }
      const { target, offsets } = dragTarget(drag.items, drag.index, dx);
      drag.target = target;
      tabButtons().forEach((btn, i) => {
        btn.style.transform = offsets[i] ? "translateX(" + offsets[i] + "px)" : "";
        btn.classList.toggle("dragging", i === drag.index);
      });
    }

    // The key that is held as the button goes up decides between a copy and a move (the key goes up after the button).
    function onRelease(ev) {
      if (!drag) return;
      const { moved, tab, index, target, items } = drag;
      const buttons = tabButtons();
      drag = null;
      if (!moved) return;
      swallowClick = true;
      clearDrag();
      if (ev?.ctrlKey || ev?.altKey) void getTabs().moveOrCopy(tab.dataset.sheetTab, buttons[dropSlot(items, ev.clientX)]?.dataset.sheetTab ?? null, true);
      else if (target !== index) void getTabs().moveTo(tab.dataset.sheetTab, target);
    }

    // True once for the click that ends a drag.
    function takeSwallowedClick() {
      const was = swallowClick;
      swallowClick = false;
      return was;
    }

    function bind() {
      const tabs = bar();
      tabs?.addEventListener("mousedown", onPress);
      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", onRelease);
      tabs?.addEventListener(
        "click",
        (ev) => {
          if (takeSwallowedClick()) {
            ev.stopImmediatePropagation();
            ev.preventDefault();
          }
        },
        true
      );
      tabs?.addEventListener("dblclick", (ev) => {
        const tab = ev.target?.closest?.(".sheet-tab-btn[data-sheet-tab]");
        if (tab) startRename(tab.dataset.sheetTab);
      });
      tabs?.addEventListener("keydown", (ev) => {
        const tab = ev.target?.closest?.(".sheet-tab-btn[data-sheet-tab]");
        if (tab && ev.key === "F2") {
          ev.preventDefault();
          startRename(tab.dataset.sheetTab);
        }
      });
    }

    return { bind, confirmDelete, pickHidden, pickPlace, startRename, focusTab };
  }

  host.__EASYOFFICE_CREATE_SHEET_TAB_UI__ = createSheetTabUi;
  host.__EASYOFFICE_SHEET_TAB_DRAG__ = { dragTarget, dropSlot };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
