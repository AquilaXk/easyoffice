(function (host) {
  "use strict";

  // The row of sheet tabs (PD-209): the order of the sheets and the tab buttons drawn from it. A
  // hidden sheet (`layout.hiddenOf`) has no button. The order is a list of names, kept apart from
  // the store (whose key order a name like "2024" would reorder) and carried by every snapshot, so
  // undo, open and save keep it. `reconcile` follows the store: a sheet that is gone leaves the
  // list and a sheet nobody placed (the pivot's, a restored older snapshot's) goes last.

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
