(function (host) {
  "use strict";

  // The row of sheet tabs (PD-209): the order of the sheets and the tab buttons drawn from it. A
  // hidden sheet (`layout.hiddenOf`) has no button. The order is a list of names, kept apart from
  // the store (whose key order a name like "2024" would reorder) and carried by every snapshot, so
  // undo, open and save keep it. `reconcile` follows the store: a sheet that is gone leaves the
  // list and a sheet nobody placed (the pivot's, a restored older snapshot's) goes last.
  //
  // 참조 구현: the reference engine's `packages/sheets-ui/src/views/sheet-bar/sheet-bar-tabs/SheetBarItem.tsx` lines
  // 38-99 and `packages/core/src/shared/color/color-kit.ts` lines 369-385 (a tab that is not selected takes the colour
  // as its background and a light or dark text by the colour's brightness; a selected tab shows the colour as a 2px
  // bar at its lower edge), Apache-2.0, Copyright 2023-present
  // DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9. Modifications: plain DOM and CSS, the
  // text colours are white and black.

  const names = host.__EASYOFFICE_SHEET_TAB_NAMES__;
  const layout = host.__EASYOFFICE_SHEET_LAYOUT__;
  let order = [];

  // Every sheet in tab order, hidden ones included; `keys` are the names the store holds.
  function reconcile(keys) {
    const known = new Set(keys);
    const seen = new Set();
    const out = [];
    for (const name of order) {
      if (known.has(name) && !seen.has(name)) {
        out.push(name);
        seen.add(name);
      }
    }
    for (const key of keys) {
      if (!seen.has(key)) {
        out.push(key);
        seen.add(key);
      }
    }
    order = out;
    return out.slice();
  }

  const visibleOf = (keys) => reconcile(keys).filter((name) => !layout.hiddenOf(name));
  const replaceOrder = (list) => {
    order = Array.isArray(list) ? list.filter((n) => typeof n === "string") : [];
  };

  function makeTab(document, name) {
    const tab = document.createElement("button");
    tab.className = "sheet-tab-btn";
    tab.setAttribute("role", "tab");
    tab.dataset.sheetTab = name;
    return tab;
  }

  // The text on a tab colour: light on a dark colour, dark on a light one. A colour is dark when its brightness
  // (r * 299 + g * 587 + b * 114) / 1000 is under 128, as the reference's `ColorKit.getBrightness` and `isDark` have
  // it (`packages/core/src/shared/color/color-kit.ts` lines 369-385). The rule of a cell with a fill (PD-227) is for
  // the cells and stays as it is; its threshold of 62 leaves black text on mid colours such as `3366cc`, too dark to read on a tab.
  function tabTextColor(rgb) {
    const n = parseInt(rgb, 16);
    const brightness = (((n >> 16) & 255) * 299 + ((n >> 8) & 255) * 587 + (n & 255) * 114) / 1000;
    return brightness < 128 ? "ffffff" : "000000";
  }

  // The tab's colour (PD-229): a tab that is not chosen has it for its background and the text that reads on it
  // (`tabTextColor`); the chosen tab keeps its own look and shows the colour as a bar along its lower edge. The look
  // is in `shell.css` (`data-tab-color`); this sets the two values it uses.
  function paintColor(btn, rgb) {
    if (!rgb) {
      delete btn.dataset.tabColor;
      btn.style.removeProperty("--tab-color");
      btn.style.removeProperty("--tab-text");
      return;
    }
    btn.dataset.tabColor = rgb;
    btn.style.setProperty("--tab-color", "#" + rgb);
    btn.style.setProperty("--tab-text", "#" + tabTextColor(rgb));
  }

  // Draws the tabs of the visible sheets before the add button, in order, reusing the buttons that
  // are there (a button that stays in place keeps its focus) and marking `active`.
  function render({ document, keys, active }) {
    const bar = document.getElementById("sheet-tab-bar");
    const add = document.getElementById("btn-add-sheet-tab");
    if (!bar || !add) return;
    const shown = visibleOf([...new Set([...keys, active])]);
    const existing = new Map();
    for (const btn of bar.querySelectorAll(".sheet-tab-btn[data-sheet-tab]")) {
      if (existing.has(btn.dataset.sheetTab) || !shown.includes(btn.dataset.sheetTab)) btn.remove();
      else existing.set(btn.dataset.sheetTab, btn);
    }
    const buttons = shown.map((name) => {
      const btn = existing.get(name) || makeTab(document, name);
      const label = names.labelOf(name);
      if (btn.textContent !== label) btn.textContent = label;
      const isActive = name === active;
      btn.classList.toggle("active", isActive);
      btn.setAttribute("aria-selected", isActive ? "true" : "false");
      paintColor(btn, layout.tabColorOf(name));
      return btn;
    });
    const placed = [...bar.querySelectorAll(".sheet-tab-btn[data-sheet-tab]")];
    if (placed.length !== buttons.length || buttons.some((btn, i) => btn !== placed[i])) {
      buttons.forEach((btn) => bar.insertBefore(btn, add));
    }
    // A strip longer than the window scrolls; the active tab stays in view.
    buttons.find((btn) => btn.dataset.sheetTab === active)?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }

  host.__EASYOFFICE_SHEET_TAB_STRIP__ = { reconcile, visibleOf, replaceOrder, getOrder: () => order.slice(), render };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
