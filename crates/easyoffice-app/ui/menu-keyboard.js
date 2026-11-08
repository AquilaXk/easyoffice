(function (host) {
  "use strict";

  // Page menu bar: open state, mouse behaviour and keyboard navigation (WAI-ARIA menubar pattern).
  // The reference app's menu items are plain `div`s with mouse handling only; here every item is
  // focusable (`tabindex="-1"`, roving by the arrow keys) so the menu is usable without a pointer:
  //   menu title   ArrowLeft/Right other menu, Home/End first/last menu, ArrowDown/Up/Enter/Space open
  //   menu item    ArrowDown/Up/Home/End move, ArrowRight opens a submenu or the next menu,
  //                ArrowLeft leaves a submenu or goes to the previous menu, Enter/Space runs the item
  //   Escape       closes the open submenu or menu and hands focus back to the menu title
  // Disabled items (`aria-disabled="true"`) stay reachable but do nothing.

  const TITLE_SELECTOR = ".studio-menu-item[data-menu-cat]";
  const MENU_SELECTOR = ".studio-menubar > .menu-item[data-menu]";
  const ITEM_SELECTOR = ".md-item, .md-sub";
  const SUBMENU_CLASS = "md-sub-panel";
  const HOVER_GUARD_MS = 80;
  const ACTIVATION_KEYS = ["Enter", " "];

  // Closes every open menu and keeps the titles' `aria-expanded` in step (also used by the actions
  // that open a dialog or window from a menu item).
  function closeAllMenus(document) {
    for (const menu of document.querySelectorAll(".menu-item.open")) {
      menu.classList.remove("open");
      delete menu.dataset.openedByHover;
      menu.querySelector(TITLE_SELECTOR)?.setAttribute("aria-expanded", "false");
    }
  }

  function createMenuKeyboard({ document }) {
    const menuList = () => Array.from(document.querySelectorAll(MENU_SELECTOR));
    const titleOf = (menu) => menu.querySelector(TITLE_SELECTOR);
    const openMenu = () => document.querySelector(".menu-item.open");
    const isDisabled = (el) => el.getAttribute("aria-disabled") === "true";

    // Direct items of one `role="menu"` list (nested submenu items belong to their own list).
    function itemsOf(listEl) {
      return Array.from(listEl.querySelectorAll(ITEM_SELECTOR)).filter(
        (el) => el.parentElement.closest('[role="menu"]') === listEl && !el.hidden
      );
    }

    function dropdownOf(menu) {
      return menu.querySelector(':scope > [role="menu"]');
    }

    const closeAll = () => closeAllMenus(document);

    function open(menu) {
      closeAll();
      menu.classList.add("open");
      titleOf(menu)?.setAttribute("aria-expanded", "true");
    }

    function focusAt(items, index) {
      items[(index + items.length) % items.length]?.focus();
    }

    function openAndFocus(menu, last) {
      open(menu);
      const items = itemsOf(dropdownOf(menu));
      focusAt(items, last ? -1 : 0);
    }

    function neighbour(menu, step) {
      const menus = menuList();
      return menus[(menus.indexOf(menu) + step + menus.length) % menus.length];
    }

    function onTitleKey(ev, title) {
      const menu = title.closest(".menu-item");
      const menus = menuList();
      const moveTo = (target) => {
        ev.preventDefault();
        if (openMenu()) open(target);
        titleOf(target).focus();
      };
      if (ev.key === "ArrowRight") return moveTo(neighbour(menu, 1));
      if (ev.key === "ArrowLeft") return moveTo(neighbour(menu, -1));
      if (ev.key === "Home") return moveTo(menus[0]);
      if (ev.key === "End") return moveTo(menus[menus.length - 1]);
      if (ev.key === "ArrowDown" || ACTIVATION_KEYS.includes(ev.key)) {
        ev.preventDefault();
        openAndFocus(menu, false);
      } else if (ev.key === "ArrowUp") {
        ev.preventDefault();
        openAndFocus(menu, true);
      }
    }

    function openSubmenu(sub) {
      const items = itemsOf(sub.querySelector('[role="menu"]'));
      focusAt(items, 0);
    }

    function activate(item) {
      if (isDisabled(item)) return;
      if (item.classList.contains("md-sub")) return openSubmenu(item);
      const title = titleOf(item.closest(".menu-item"));
      closeAll();
      title?.focus();
      item.click();
    }

    function onItemKey(ev, item) {
      const listEl = item.parentElement.closest('[role="menu"]');
      const items = itemsOf(listEl);
      const index = items.indexOf(item);
      const inSubmenu = listEl.classList.contains(SUBMENU_CLASS);
      const menu = item.closest(".menu-item");
      const keys = {
        ArrowDown: () => focusAt(items, index + 1),
        ArrowUp: () => focusAt(items, index - 1),
        Home: () => focusAt(items, 0),
        End: () => focusAt(items, -1),
        ArrowRight: () => (item.classList.contains("md-sub") && !isDisabled(item) ? openSubmenu(item) : openAndFocus(neighbour(menu, 1), false)),
        ArrowLeft: () => (inSubmenu ? listEl.parentElement.focus() : openAndFocus(neighbour(menu, -1), false)),
        Enter: () => activate(item),
        " ": () => activate(item),
        Tab: () => {
          closeAll();
          titleOf(menu)?.focus();
        },
      };
      const run = keys[ev.key];
      if (!run) return;
      if (ev.key !== "Tab") ev.preventDefault();
      run();
    }

    function onMenubarKeydown(ev) {
      const title = ev.target?.closest?.(TITLE_SELECTOR);
      if (title) return onTitleKey(ev, title);
      const item = ev.target?.closest?.(ITEM_SELECTOR);
      if (item) onItemKey(ev, item);
    }

    // Escape closes the innermost open level; focus goes back only when it was inside the menu bar.
    function onEscape(ev) {
      const menu = openMenu();
      if (ev.key !== "Escape" || !menu) return;
      const sub = ev.target?.closest?.('.md-sub-panel[role="menu"]');
      if (sub) {
        ev.preventDefault();
        return sub.parentElement.focus();
      }
      const inside = menu.contains(ev.target);
      closeAll();
      if (inside) titleOf(menu)?.focus();
    }

    function bindTitle(title, host) {
      const menu = title.closest(".menu-item") || title.parentElement;
      title.addEventListener("click", (ev) => {
        ev.stopPropagation?.();
        if (menu.dataset.openedByHover === "true") {
          delete menu.dataset.openedByHover;
          return;
        }
        const wasOpen = menu.classList.contains("open");
        closeAll();
        if (!wasOpen) open(menu);
      });
      menu.addEventListener("mouseenter", () => {
        if (openMenu() && !menu.classList.contains("open")) {
          open(menu);
          menu.dataset.openedByHover = "true";
          host.setTimeout?.(() => { delete menu.dataset.openedByHover; }, HOVER_GUARD_MS);
        }
      });
      menu.addEventListener("mouseleave", () => { delete menu.dataset.openedByHover; });
    }

    function bind() {
      for (const title of document.querySelectorAll(TITLE_SELECTOR)) bindTitle(title, host);
      document.querySelector(".studio-menubar")?.addEventListener("keydown", onMenubarKeydown);
      document.addEventListener("click", (ev) => {
        if (!ev?.target?.closest?.(".studio-menubar")) closeAll();
      });
      host.addEventListener("keydown", onEscape);
    }

    return { bind, closeAll, open };
  }

  host.__EASYOFFICE_CREATE_MENU_KEYBOARD__ = createMenuKeyboard;
  host.__EASYOFFICE_CLOSE_MENUS__ = closeAllMenus;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
