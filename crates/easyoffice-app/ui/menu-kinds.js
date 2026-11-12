(function (host) {
  "use strict";

  // Page menu bar per window kind (PD-190). One menu row holds the menus of every kind; the elements
  // that belong to some kinds only carry `data-kinds="DOC SHEET"`, and the kind of the window decides
  // which of them are shown. An element without the attribute is in every kind. Separators are not
  // marked: after the items are filtered, a separator is shown only between two visible items.
  // Whether a shown item applies right now stays with `studioCommandApplies` (`aria-disabled`).

  const KINDS = Object.freeze(["DOC", "HWP", "SHEET", "SLIDE", "PDF"]);
  const KIND_SET = new Set(KINDS);

  // The menu kind of a command module: the Hangul editor's embedded editor is `HWP_EMBED`; the start
  // screen and the converter have no menu of their own and use the document one.
  function menuKindOf(commandModule) {
    if (commandModule === "HWP_EMBED") return "HWP";
    return KIND_SET.has(commandModule) ? commandModule : "DOC";
  }

  function collapseSeparators(dropdown) {
    let pending = null;
    let anyItem = false;
    for (const child of Array.from(dropdown.children)) {
      if (child.classList.contains("md-sep")) {
        child.hidden = true;
        if (anyItem) pending = child;
      } else if (!child.hidden) {
        if (pending) pending.hidden = false;
        pending = null;
        anyItem = true;
      }
    }
  }

  function applyMenuKind(document, commandModule) {
    const kind = menuKindOf(commandModule);
    const bar = document.querySelector(".studio-menubar");
    if (!bar) return kind;
    for (const el of [...bar.querySelectorAll("[data-kinds]"), ...document.querySelectorAll("#icon-toolbar [data-kinds]")]) {
      el.hidden = !el.dataset.kinds.split(" ").includes(kind);
    }
    for (const menu of bar.querySelectorAll(":scope > .menu-item")) {
      if (menu.hidden) menu.classList.remove("open");
      const dropdown = menu.querySelector(":scope > .menu-dropdown");
      if (dropdown) collapseSeparators(dropdown);
    }
    return kind;
  }

  host.__EASYOFFICE_MENU_KINDS__ = { KINDS, menuKindOf, applyMenuKind };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
