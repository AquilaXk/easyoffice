(function (host) {
  "use strict";

  // The right-click menu of a sheet tab (PD-208, PD-209): the commands of `sheet-tabs.js` on the
  // tab that was pressed, and "탭 색". Shift+F10 and the menu key open it at the focused tab. A
  // command that cannot run now (the last shown sheet cannot go, the first one cannot move left) is
  // greyed out and skipped by the arrow keys.

  const MENU_MARGIN = 2;

  function createSheetTabMenu({ document, getTabs, tabColor, ui }) {
    const $ = (id) => document.getElementById(id);
    const menu = () => $("sheet-tab-menu");
    const tabOf = (name) => Array.from(document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]")).find((t) => t.dataset.sheetTab === name) || null;
    const items = () => Array.from(menu().querySelectorAll(".md-item")).filter((el) => !el.hidden && el.getAttribute("aria-disabled") !== "true");
    let menuSheet = null;

    // Which command each item runs on the sheet that was pressed.
    const COMMANDS = {
      insert: (tabs, sheet) => tabs.insert(sheet),
      delete: (tabs, sheet) => tabs.remove(sheet),
      rename: (tabs, sheet) => ui.startRename(sheet),
      copy: (tabs, sheet) => tabs.copy(sheet),
      "move-left": (tabs, sheet) => tabs.move(sheet, -1),
      "move-right": (tabs, sheet) => tabs.move(sheet, 1),
      hide: (tabs, sheet) => tabs.hide(sheet),
      unhide: (tabs) => tabs.unhide(),
      color: (tabs, sheet) => tabColor.openFor(sheet),
    };

    // The states of `tabs.can` that grey an item out.
    const ENABLED_BY = {
      insert: "insert",
      delete: "delete",
      rename: "rename",
      copy: "copy",
      "move-left": "moveLeft",
      "move-right": "moveRight",
      hide: "hide",
      unhide: "unhide",
    };

    function closeMenu() {
      const el = menu();
      if (el && !el.hidden) el.hidden = true;
    }

    function openMenu(tab, x, y) {
      const el = menu();
      if (!el) return;
      menuSheet = tab.dataset.sheetTab;
      const can = getTabs().can(menuSheet);
      for (const item of el.querySelectorAll("[data-tab-act]")) {
        const key = ENABLED_BY[item.dataset.tabAct];
        const off = key !== undefined && !can[key];
        if (off) item.setAttribute("aria-disabled", "true");
        else item.removeAttribute("aria-disabled");
      }
      el.hidden = false;
      el.style.left = "0px";
      el.style.top = "0px";
      const vw = host.innerWidth || document.documentElement.clientWidth;
      const vh = host.innerHeight || document.documentElement.clientHeight;
      // The strip sits at the bottom of the window: the menu opens upward from the top of the tab.
      el.style.left = Math.max(0, Math.min(x, vw - el.offsetWidth - MENU_MARGIN)) + "px";
      el.style.top = Math.max(0, Math.min(y - el.offsetHeight, vh - el.offsetHeight - MENU_MARGIN)) + "px";
      items()[0]?.focus();
    }

    function onMenuKeydown(ev) {
      const list = items();
      const at = list.indexOf(document.activeElement);
      if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
        ev.preventDefault();
        list[(at + (ev.key === "ArrowDown" ? 1 : -1) + list.length) % list.length]?.focus();
      } else if (ev.key === "Home" || ev.key === "End") {
        ev.preventDefault();
        list[ev.key === "Home" ? 0 : list.length - 1]?.focus();
      } else if (ev.key === "Escape" || ev.key === "Tab") {
        ev.preventDefault();
        closeMenu();
        tabOf(menuSheet)?.focus();
      } else if (ev.key === "Enter" || ev.key === " ") {
        ev.preventDefault();
        document.activeElement?.click?.();
      }
    }

    function bind() {
      const bar = $("sheet-tab-bar");
      bar?.addEventListener("contextmenu", (ev) => {
        const tab = ev.target?.closest?.(".sheet-tab-btn[data-sheet-tab]");
        if (!tab) return;
        ev.preventDefault();
        openMenu(tab, ev.clientX, tab.getBoundingClientRect().top);
      });
      bar?.addEventListener("keydown", (ev) => {
        const tab = ev.target?.closest?.(".sheet-tab-btn[data-sheet-tab]");
        if (!tab || !(ev.key === "ContextMenu" || (ev.key === "F10" && ev.shiftKey))) return;
        ev.preventDefault();
        const box = tab.getBoundingClientRect();
        openMenu(tab, box.left, box.top);
      });
      const el = menu();
      el?.addEventListener("keydown", onMenuKeydown);
      el?.addEventListener("contextmenu", (ev) => ev.preventDefault());
      for (const item of el?.querySelectorAll("[data-tab-act]") || []) {
        item.addEventListener("click", () => {
          if (item.getAttribute("aria-disabled") === "true") return;
          const run = COMMANDS[item.dataset.tabAct];
          const sheet = menuSheet;
          closeMenu();
          if (run && sheet) void Promise.resolve(run(getTabs(), sheet)).catch(() => {});
        });
      }
      document.addEventListener("mousedown", (ev) => {
        if (el && !el.hidden && !el.contains(ev.target)) closeMenu();
      });
      host.addEventListener?.("resize", closeMenu);
      host.addEventListener?.("blur", closeMenu);
    }

    return { bind, closeMenu };
  }

  host.__EASYOFFICE_CREATE_SHEET_TAB_MENU__ = createSheetTabMenu;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
