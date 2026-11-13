(function (host) {
  "use strict";

  // The tab colour (PD-208): the shell's palette opened at a tab (the "탭 색" command of the tab menu and
  // of the sheet menu) and the coloured line under every tab that has one. The colour is part of the
  // layout store, so undo, save and open carry it.

  const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;

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

    function bind() {
      const bar = $("sheet-tab-bar");
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

    return { bind, paint, openFor: (name) => openPalette(name || getActiveSheetName()) };
  }

  host.__EASYOFFICE_CREATE_SHEET_TAB_COLOR__ = createSheetTabColor;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
