(function (host) {
  "use strict";

  // The right-click menu of a sheet tab (PD-208): "시트 추가" (the page's add button) and the "탭 색" command and of the
  // sheet menu, which opens the shell's palette at the tab, and the coloured line under every
  // tab that has one. The colour is part of the layout store, so undo, save and open carry it.

  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
  const MENU_MARGIN = 2;

  function createSheetTabColor({ document, view, getActiveSheetName, onTabsChanged }) {
    const $ = (id) => document.getElementById(id);
    const tabs = () => Array.from(document.querySelectorAll("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab]"));
    const tabOf = (name) => tabs().find((t) => t.dataset.sheetTab === name) || null;
    let menuSheet = null;
    let picker = null;

    // A tab with a colour shows it as a line under its name; the name keeps the theme's text colour.
    function paint() {
      for (const tab of tabs()) {
        const color = layoutApi.tabColorOf(tab.dataset.sheetTab);
        if (color) {
          tab.dataset.tabColor = color;
          tab.style.setProperty("--tab-color", "#" + color);
        } else if (tab.dataset.tabColor) {
          delete tab.dataset.tabColor;
          tab.style.removeProperty("--tab-color");
        }
      }
    }

    function ensurePicker() {
      if (picker) return picker;
      const hostEl = $("sheet-tab-color-host");
      picker = host.__EASYOFFICE_COLOR_PICKER__?.setupColorPicker(
        hostEl,
        (color) => {
          const rgb = color.replace(/^#/, "").toLowerCase();
          void view.setTabColor(menuSheet, /^[0-9a-f]{6}$/.test(rgb) ? rgb : "");
        },
        { clear: true, remember: true }
      );
      return picker;
    }

    // The palette opens under the tab of `name`.
    function openPalette(name) {
      const tab = tabOf(name);
      const hostEl = $("sheet-tab-color-host");
      if (!tab || !hostEl) return;
      menuSheet = name;
      const box = tab.getBoundingClientRect();
      hostEl.style.left = box.left + "px";
      hostEl.style.top = box.top + "px";
      const toggle = hostEl.querySelector("[data-color-toggle]");
      toggle.style.width = box.width + "px";
      toggle.style.height = box.height + "px";
      const current = layoutApi.tabColorOf(name);
      if (current) hostEl.querySelector("input").value = "#" + current;
      ensurePicker()?.open();
    }

    // ---- the right-click menu of a tab --------------------------------------------------------------
    const menu = () => $("sheet-tab-menu");
    const items = () => Array.from(menu().querySelectorAll(".md-item")).filter((el) => !el.hidden);

    function closeMenu() {
      const el = menu();
      if (el && !el.hidden) el.hidden = true;
    }

    function openMenu(tab, x, y) {
      const el = menu();
      if (!el) return;
      menuSheet = tab.dataset.sheetTab;
      el.hidden = false;
      el.style.left = "0px";
      el.style.top = "0px";
      const vw = host.innerWidth || document.documentElement.clientWidth;
      const vh = host.innerHeight || document.documentElement.clientHeight;
      // A tab sits at the bottom of the window: the menu opens upward from the pointer.
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
        openMenu(tab, ev.clientX, ev.clientY);
      });
      bar?.addEventListener("keydown", (ev) => {
        const tab = ev.target?.closest?.(".sheet-tab-btn[data-sheet-tab]");
        if (!tab || !(ev.key === "ContextMenu" || (ev.key === "F10" && ev.shiftKey))) return;
        ev.preventDefault();
        const box = tab.getBoundingClientRect();
        openMenu(tab, box.left, box.bottom);
      });
      const el = menu();
      el?.addEventListener("keydown", onMenuKeydown);
      el?.addEventListener("contextmenu", (ev) => ev.preventDefault());
      // The one tab command the page already has besides the colour: the add button at the end of the strip.
      el?.querySelector('[data-tab-act="add"]')?.addEventListener("click", () => {
        closeMenu();
        $("btn-add-sheet-tab")?.click();
      });
      el?.querySelector('[data-tab-act="color"]')?.addEventListener("click", () => {
        closeMenu();
        openPalette(menuSheet);
      });
      document.addEventListener("mousedown", (ev) => {
        if (el && !el.hidden && !el.contains(ev.target)) closeMenu();
      });
      host.addEventListener?.("resize", closeMenu);
      host.addEventListener?.("blur", closeMenu);
      layoutApi.onChange(paint);
      // Tabs come and go with a file, and the active one changes with a switch or an open.
      if (bar && typeof host.MutationObserver === "function") {
        const watch = new host.MutationObserver(() => {
          paint();
          onTabsChanged?.();
        });
        watch.observe(bar, { childList: true, subtree: true, attributes: true, attributeFilter: ["class"] });
      }
      paint();
    }

    return { bind, paint, openFor: (name) => openPalette(name || getActiveSheetName()), closeMenu };
  }

  host.__EASYOFFICE_CREATE_SHEET_TAB_COLOR__ = createSheetTabColor;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
