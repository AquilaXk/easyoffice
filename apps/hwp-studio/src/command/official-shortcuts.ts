// EasyOffice extension: the keys and key labels the official 한글 menu pages name (PD-396).
// The upstream editor binds the character table to Alt+F10 and the row/column dialog to Alt+Enter, and writes
// the chords of 하이퍼링크 and 필드 입력 as Ctrl+K+H and Ctrl+K+E. The official menus read Ctrl+F10,
// Alt+Insert (줄/칸 추가하기, with Alt+Delete for 줄/칸 지우기) and Ctrl+K,H / Ctrl+K,E. Only bindings and
// labels change here; every command is the upstream one.
import type { CommandRegistry } from '@/command/registry';
import type { ShortcutDef } from '@/command/shortcut-map';

type ShortcutTable = [ShortcutDef, string][];

interface Rebinding {
  readonly command: string;
  readonly from: ShortcutDef;
  readonly to: ShortcutDef;
}

const REBINDINGS: readonly Rebinding[] = [
  { command: 'insert:symbols', from: { key: 'f10', alt: true }, to: { key: 'f10', ctrl: true } },
  { command: 'table:insert-row-col', from: { key: 'enter', alt: true }, to: { key: 'insert', alt: true } },
];

/** Key labels of the registry (what the menu items show), by command id. */
export const OFFICIAL_SHORTCUT_LABELS: Readonly<Record<string, string>> = {
  'insert:symbols': 'Ctrl+F10',
  'insert:hyperlink': 'Ctrl+K,H',
  'insert:field': 'Ctrl+K,E',
  'table:insert-row-col': 'Alt+Insert',
};

const sameKey = (a: ShortcutDef, b: ShortcutDef): boolean =>
  a.key === b.key && Boolean(a.ctrl) === Boolean(b.ctrl) && Boolean(a.alt) === Boolean(b.alt) && Boolean(a.shift) === Boolean(b.shift);

/** Rebinds the shortcut table in place and rewrites the labels of the registered commands. */
export function applyOfficialShortcuts(shortcuts: ShortcutTable, registry: CommandRegistry): void {
  for (const { command, from, to } of REBINDINGS) {
    const at = shortcuts.findIndex(([def, id]) => id === command && sameKey(def, from));
    if (at >= 0) shortcuts.splice(at, 1);
    shortcuts.push([to, command]);
  }
  for (const [id, shortcutLabel] of Object.entries(OFFICIAL_SHORTCUT_LABELS)) {
    const command = registry.get(id);
    if (command) registry.register({ ...command, shortcutLabel });
  }
}
