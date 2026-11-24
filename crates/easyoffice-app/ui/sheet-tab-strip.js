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
  const nav = () => host.__EASYOFFICE_SHEET_NAV__;
  let order = [];
  // Sheet group selection state (PD-346): `grouped` holds >= 2 visible sheets including `active`, or is empty when ungrouped.
  let grouped = new Set();
  let anchorSheet = null;

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
  const shownOf = (keys, active) => visibleOf(active ? [...new Set([...keys, active])] : keys);
  const replaceOrder = (list) => {
    order = Array.isArray(list) ? list.filter((n) => typeof n === "string") : [];
  };

  function reconcileGroup(shown, active) {
    const shownSet = new Set(shown);
    if (grouped.size > 0) {
      for (const name of [...grouped]) {
        if (!shownSet.has(name)) grouped.delete(name);
      }
      if (active && shownSet.has(active) && !grouped.has(active)) {
        grouped.clear();
      }
      if (grouped.size < 2) grouped.clear();
    }
    if (!anchorSheet || !shownSet.has(anchorSheet) || (grouped.size > 0 && !grouped.has(anchorSheet))) {
      anchorSheet = active && shownSet.has(active) ? active : shown[0] || null;
    }
  }

  function clearGroup(active = null) {
    grouped.clear();
    anchorSheet = active || null;
  }

  const isGrouped = () => grouped.size >= 2;
  const hasInGroup = (name) => grouped.has(name);

  function getGroup(keys, active) {
    const shown = shownOf(keys, active);
    reconcileGroup(shown, active);
    if (grouped.size >= 2) return shown.filter((name) => grouped.has(name));
    return active && shown.includes(active) ? [active] : [];
  }

  function stepGroup(keys, active, step) {
    const shown = shownOf(keys, active);
    reconcileGroup(shown, active);
    const res = nav()?.sheetGroupRange(shown, anchorSheet, active, step);
    if (!res) return null;
    anchorSheet = res.anchor;
    if (res.selected.length >= 2) grouped = new Set(res.selected);
    else grouped.clear();
    return res;
  }

  function rangeGroup(keys, active, target) {
    const shown = shownOf(keys, active);
    reconcileGroup(shown, active);
    const res = nav()?.sheetSpanBetween(shown, anchorSheet, active, target);
    if (!res) return null;
    anchorSheet = res.anchor;
    if (res.selected.length >= 2) grouped = new Set(res.selected);
    else grouped.clear();
    return res;
  }

  function toggleGroup(keys, active, target) {
    const shown = shownOf(keys, active);
    if (!shown.includes(target)) return null;
    reconcileGroup(shown, active);
    const cur = new Set(grouped.size >= 2 ? grouped : active && shown.includes(active) ? [active] : []);
    if (cur.has(target)) {
      if (cur.size <= 1) return { active, changed: false, switched: false };
      cur.delete(target);
      let nextActive = active;
      if (target === active) {
        const at = shown.indexOf(target);
        nextActive = shown.slice(at + 1).find((name) => cur.has(name))
          ?? shown.slice(0, at).reverse().find((name) => cur.has(name))
          ?? active;
        anchorSheet = nextActive;
      } else {
        anchorSheet = active;
      }
      if (cur.size >= 2) grouped = cur;
      else grouped.clear();
      return { active: nextActive, changed: true, switched: nextActive !== active };
    }
    if (active && shown.includes(active)) cur.add(active);
    cur.add(target);
    if (cur.size >= 2) grouped = cur;
    anchorSheet = target;
    return { active, changed: true, switched: false };
  }

  function selectAllGroup(keys, active) {
    const shown = shownOf(keys, active);
    if (shown.length <= 1) {
      grouped.clear();
      anchorSheet = active && shown.includes(active) ? active : shown[0] || null;
      return { selected: shown, changed: false };
    }
    grouped = new Set(shown);
    anchorSheet = active && shown.includes(active) ? active : shown[0];
    return { selected: shown, changed: true };
  }

  function renameInGroup(from, to) {
    if (grouped.has(from)) {
      grouped.delete(from);
      grouped.add(to);
    }
    if (anchorSheet === from) anchorSheet = to;
  }

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

  function applyTabSelection(buttons, active) {
    for (const btn of buttons) {
      const name = btn.dataset.sheetTab;
      const isActive = name === active;
      const isInactiveGrouped = grouped.size >= 2 && grouped.has(name) && !isActive;
      btn.classList.toggle("active", isActive);
      btn.setAttribute("role", "tab");
      btn.setAttribute("aria-selected", isActive || isInactiveGrouped ? "true" : "false");
      if (isInactiveGrouped) btn.dataset.tabGrouped = "true";
      else delete btn.dataset.tabGrouped;
    }
  }

  function syncSelection(document, keys, active) {
    const bar = document.getElementById("sheet-tab-bar");
    if (!bar) return;
    const shown = shownOf(keys, active);
    reconcileGroup(shown, active);
    const buttons = [...bar.querySelectorAll(".sheet-tab-btn[data-sheet-tab]")];
    applyTabSelection(buttons, active);
    buttons.find((btn) => btn.dataset.sheetTab === active)?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }

  // Draws the tabs of the visible sheets before the add button, in order, reusing the buttons that
  // are there (a button that stays in place keeps its focus) and marking `active` and any grouped tabs.
  function render({ document, keys, active }) {
    const bar = document.getElementById("sheet-tab-bar");
    const add = document.getElementById("btn-add-sheet-tab");
    if (!bar || !add) return;
    const shown = shownOf(keys, active);
    reconcileGroup(shown, active);
    const existing = new Map();
    for (const btn of bar.querySelectorAll(".sheet-tab-btn[data-sheet-tab]")) {
      if (existing.has(btn.dataset.sheetTab) || !shown.includes(btn.dataset.sheetTab)) btn.remove();
      else existing.set(btn.dataset.sheetTab, btn);
    }
    const buttons = shown.map((name) => {
      const btn = existing.get(name) || makeTab(document, name);
      const label = names.labelOf(name);
      if (btn.textContent !== label) btn.textContent = label;
      paintColor(btn, layout.tabColorOf(name));
      return btn;
    });
    applyTabSelection(buttons, active);
    const placed = [...bar.querySelectorAll(".sheet-tab-btn[data-sheet-tab]")];
    if (placed.length !== buttons.length || buttons.some((btn, i) => btn !== placed[i])) {
      buttons.forEach((btn) => bar.insertBefore(btn, add));
    }
    // A strip longer than the window scrolls; the active tab stays in view.
    buttons.find((btn) => btn.dataset.sheetTab === active)?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }

  host.__EASYOFFICE_SHEET_TAB_STRIP__ = {
    reconcile,
    visibleOf,
    replaceOrder,
    getOrder: () => order.slice(),
    clearGroup,
    isGrouped,
    hasInGroup,
    getAnchor: () => anchorSheet,
    getGroup,
    stepGroup,
    rangeGroup,
    toggleGroup,
    selectAllGroup,
    renameInGroup,
    syncSelection,
    render,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
