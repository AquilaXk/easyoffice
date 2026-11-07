// EasyOffice-only module, adapted from hop `apps/studio-host/src/host/toolbar-labels.ts` (MIT; see
// THIRD_PARTY_NOTICES.md): the 보기 menu's "도구 상자 라벨 표시" check item. The choice is a window
// display preference, kept in localStorage; the page-side rules are in eo-overrides.css.
import type { CommandDef } from '@/command/types';

export const TOOLBAR_LABELS_STORAGE_KEY = 'toolbar-labels';
export const TOOLBAR_LABELS_COMMAND_ID = 'view:toolbar-labels';
const HIDDEN = 'hidden';
const SHOWN = 'shown';

/** The two places this module touches, so the module can run against a fake page in tests. */
export interface ToolbarLabelsHost {
  root: { dataset: DOMStringMap };
  menuItems(): Iterable<{ classList: { toggle(name: string, force: boolean): unknown }; setAttribute(name: string, value: string): void }>;
  storage(): { getItem(key: string): string | null; setItem(key: string, value: string): void };
}

function browserHost(): ToolbarLabelsHost {
  return {
    root: document.documentElement,
    menuItems: () => document.querySelectorAll(`[data-cmd="${TOOLBAR_LABELS_COMMAND_ID}"]`),
    storage: () => localStorage,
  };
}

export function applyToolbarLabels(visible: boolean, host: ToolbarLabelsHost = browserHost()): void {
  host.root.dataset.toolbarLabels = visible ? SHOWN : HIDDEN;
  for (const item of host.menuItems()) {
    item.classList.toggle('active', visible);
    item.setAttribute('aria-checked', String(visible));
  }
}

/** Labels are shown unless the stored choice says hidden; an unreadable store means shown. */
export function readToolbarLabels(host: ToolbarLabelsHost = browserHost()): boolean {
  try {
    return host.storage().getItem(TOOLBAR_LABELS_STORAGE_KEY) !== HIDDEN;
  } catch {
    return true;
  }
}

export function toggleToolbarLabels(host: ToolbarLabelsHost = browserHost()): boolean {
  const visible = host.root.dataset.toolbarLabels === HIDDEN;
  applyToolbarLabels(visible, host);
  try {
    host.storage().setItem(TOOLBAR_LABELS_STORAGE_KEY, visible ? SHOWN : HIDDEN);
  } catch {
    // Keep the current window usable when storage is unavailable.
  }
  return visible;
}

export function initToolbarLabels(): void {
  applyToolbarLabels(readToolbarLabels());
  window.addEventListener('storage', (event) => {
    if (event.key === TOOLBAR_LABELS_STORAGE_KEY || event.key === null) applyToolbarLabels(readToolbarLabels());
  });
}

export const toolbarLabelsCommand: CommandDef = {
  id: TOOLBAR_LABELS_COMMAND_ID,
  label: '도구 상자 라벨 표시',
  execute() {
    toggleToolbarLabels();
  },
};
