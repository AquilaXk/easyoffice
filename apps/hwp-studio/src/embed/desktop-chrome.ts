// EasyOffice extension (참조 앱 port: studio-host `core/desktop-chrome.ts` and `core/platform.ts`,
// MIT, Copyright (c) 2025-2026 Edward Kim): desktop chrome behaviour of the editor frame.
//  - On macOS the shortcut text of the static chrome (split-menu items, `title` tooltips) reads
//    ⌘ ⌥ ⇧ like the menu items that the editor already formats itself.
//  - The context menu of the chrome outside the document (menu bar, toolbars, status bar) is the
//    browser's own menu in a webview, so it is suppressed there; the document area keeps its menu.

/** The chrome areas that are not the editor surface. */
export const NON_EDITOR_CHROME_SELECTOR = '#menu-bar, #icon-toolbar, #style-bar, #status-bar';
const SHORTCUT_TEXT_SELECTOR = '.md-shortcut, .tb-split-shortcut';

/** `Ctrl+Shift+H` reads `⌘⇧H` (same replacement as the editor's own menu labels). */
export function macShortcutLabel(label: string): string {
  return label
    .replace(/\bCtrl\+/g, '⌘')
    .replace(/\bAlt\+/g, '⌥')
    .replace(/\bOption\+/g, '⌥')
    .replace(/\bShift\+/g, '⇧');
}

export function applyMacShortcutPresentation(doc: Document): void {
  doc.querySelectorAll<HTMLElement>(SHORTCUT_TEXT_SELECTOR).forEach((element) => {
    const text = element.textContent;
    if (text) element.textContent = macShortcutLabel(text);
  });
  doc.querySelectorAll<HTMLElement>('[title]').forEach((element) => {
    const title = element.getAttribute('title');
    if (!title) return;
    const label = macShortcutLabel(title);
    if (label !== title) element.setAttribute('title', label);
  });
}

export function installNonEditorContextMenuGuards(doc: Document): void {
  const preventContextMenu = (event: Event) => event.preventDefault();
  doc.querySelectorAll<HTMLElement>(NON_EDITOR_CHROME_SELECTOR).forEach((element) => {
    element.addEventListener('contextmenu', preventContextMenu);
  });
}
