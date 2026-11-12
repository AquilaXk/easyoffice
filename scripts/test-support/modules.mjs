// Shared helpers for the WebUI DOM tests.

// Windows host exactly one module chosen at launch, so there is no on-screen module switcher;
// tests use the same activation entry the launch code uses.
export function switchModule(document, moduleName) {
  return document.defaultView.__EASYOFFICE_ACTIVATE_MODULE__(moduleName);
}

export function typeIntoDocParagraph(document, text) {
  const docEditor = document.getElementById("doc-editor");
  docEditor.focus();
  const p = docEditor.querySelector("p");
  docEditor.dispatchEvent({ type: "beforeinput", target: p });
  p.textContent = text;
  docEditor.dispatchEvent({ type: "input", target: p });
  return p;
}

export function createMemoryStorage(seed = {}) {
  const map = new Map(Object.entries(seed));
  return {
    map,
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => {
      map.set(k, String(v));
    },
    removeItem: (k) => {
      map.delete(k);
    },
  };
}

const COMMAND_PREFIX_MODULE = { document: "DOC", doc: "DOC", hwp: "HWP", sheet: "SHEET", slide: "SLIDE", pdf: "PDF" };

/** Menu item that runs the named studio command (`studio-save`, `studio-undo`, ...). */
export function studioItem(document, cmd) {
  return document.querySelector(`.md-item[data-studio-menu-cmd="${cmd}"]`);
}

/** Menu item that runs one catalog ribbon command id (the ribbon strip no longer exists). */
export function commandItem(document, commandId) {
  const catalog = document.defaultView.__EASYOFFICE_RIBBON_CATALOG__;
  const mod = COMMAND_PREFIX_MODULE[commandId.split("-")[0]];
  const items = document.querySelectorAll(".md-item[data-studio-menu-cmd]");
  return items.find((el) => catalog.resolveMenuCommand(el.dataset.studioMenuCmd, mod) === commandId) || null;
}

/** Puts the caret in the first cell of the first table of the document editor (the table menu's items need one), making a table when there is none. */
export function putCaretInFirstTable(document, selection) {
  const editor = document.getElementById("doc-editor");
  if (!editor.querySelector("td, th")) {
    const table = document.createElement("table");
    const body = table.appendChild(document.createElement("tbody"));
    body.appendChild(document.createElement("tr")).appendChild(document.createElement("td"));
    editor.appendChild(table);
  }
  const cell = editor.querySelector("td, th");
  selection.addRange({ startContainer: cell, startOffset: 0, endContainer: cell, endOffset: 0, collapsed: true });
  document.querySelector('.menu-item[data-menu="table"]').dispatchEvent({ type: "mouseenter" });
  return cell;
}
