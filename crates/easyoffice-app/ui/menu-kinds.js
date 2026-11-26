(function (host) {
  "use strict";

  // Page menu bar per window kind (PD-190). One menu row holds the menus of every kind; the elements
  // that belong to some kinds only carry `data-kinds="DOC SHEET"`, and the kind of the window decides
  // which of them are shown. An element without the attribute is in every kind. Separators are not
  // marked: after the items are filtered, a separator is shown only between two visible items.
  // Whether a shown item applies right now stays with `studioCommandApplies` (`aria-disabled`).

  const KINDS = Object.freeze(["DOC", "PAGES", "HWP", "SHEET", "SLIDE", "PDF", "BASE", "BOARD", "START", "CONV"]);
  const KIND_SET = new Set(KINDS);
  const KIND_MENU_TITLES = {
    insert: { HWP: "입력", DOC: "삽입", PAGES: "삽입" },
    page: { DOC: "레이아웃", HWP: "쪽", PDF: "쪽" },
    format: { DOC: "서식", PAGES: "포맷", HWP: "서식", SHEET: "서식", SLIDE: "서식" },
  };

  // The menu kind of a command module: `HWP_EMBED` -> `HWP`; `DOC` with `eoDocFlavor === "pages"` -> `PAGES`.
  function menuKindOf(commandModule, doc) {
    if (commandModule === "HWP_EMBED") return "HWP";
    if (commandModule === "DOC" && (doc || host.document)?.getElementById?.("doc-editor")?.dataset?.eoDocFlavor === "pages") {
      return "PAGES";
    }
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
    const kind = menuKindOf(commandModule, document);
    const bar = document.querySelector(".studio-menubar");
    if (!bar) return kind;
    const hwpEmbed = document.getElementById("app")?.dataset.hwpEmbed === "true";
    for (const el of [
      ...bar.querySelectorAll("[data-kinds]"),
      ...document.querySelectorAll("#icon-toolbar [data-kinds], #doc-style-bar [data-kinds]"),
    ]) {
      const embedHwpOnly = el.dataset.hwpSaveAs !== undefined && !hwpEmbed;
      el.hidden = !el.dataset.kinds.split(" ").includes(kind) || Boolean(el.disabled) || embedHwpOnly;
    }
    for (const menu of bar.querySelectorAll(":scope > .menu-item")) {
      const titleMap = KIND_MENU_TITLES[menu.dataset.menu];
      if (titleMap) {
        const titleBtn = menu.querySelector(":scope > .menu-title");
        if (titleBtn && titleMap[kind]) titleBtn.textContent = titleMap[kind];
      }
      if (menu.hidden) menu.classList.remove("open");
      const dropdown = menu.querySelector(":scope > .menu-dropdown");
      if (dropdown) collapseSeparators(dropdown);
      for (const panel of menu.querySelectorAll(".md-sub-panel")) collapseSeparators(panel);
    }
    return kind;
  }

  host.__EASYOFFICE_MENU_KINDS__ = { KINDS, menuKindOf, applyMenuKind };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
