// EasyOffice extension: shell shortcut forwarding (PD-088). While the editor owns the keyboard focus,
// the page that hosts it cannot see key events, so Ctrl+S, Ctrl+O, Ctrl+N, Ctrl+K and Ctrl+W would
// never reach the shell. The shell tells the editor which combos to hand over (`?shellKeys=` in the
// frame URL, built from the shell's single shortcut table); the editor forwards only the combo
// string, over the embed port, and only for combos that carry the Ctrl/Cmd modifier. Typed text and
// every other key stay inside the editor.

export const SHELL_SHORTCUT_REASON = 'shell_shortcut' as const;
const MAX_KEYS = 16;
const COMBO_PATTERN = /^Ctrl\+(?:Alt\+)?(?:Shift\+)?[A-Z0-9]$/;
const KEY_CODE_RE = /^Key([A-Z])$/;

export interface ShellKeyEvent {
  key: string;
  code?: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  isComposing?: boolean;
  getModifierState?: (keyArg: string) => boolean;
}

/** Resolves a key event to an uppercase Latin letter ('A'..'Z') or 1-char uppercase key; null when composing. */
export function physicalLatinKey(event: ShellKeyEvent): string | null {
  if (event.isComposing) return null;
  const key = String(event.key || '');
  const code = String(event.code || '');
  const modified = event.ctrlKey || event.metaKey || event.altKey;
  if (modified && !(event.getModifierState?.('AltGraph') && !event.metaKey)) {
    if (
      event.altKey ||
      key === 'Process' ||
      key === 'Unidentified' ||
      (key.length === 1 && key.charCodeAt(0) > 0x7f)
    ) {
      const m = KEY_CODE_RE.exec(code);
      if (m) return m[1];
    }
  }
  return key.length === 1 ? key.toUpperCase() : null;
}

/** Combos the shell asked for, from the frame URL query. Anything malformed is ignored. */
export function parseShellKeys(search: string): string[] {
  const raw = new URLSearchParams(search).get('shellKeys') ?? '';
  const keys = raw
    .split(',')
    .map((key) => key.trim())
    .filter((key) => COMBO_PATTERN.test(key));
  return [...new Set(keys)].slice(0, MAX_KEYS);
}

/** Same normalisation as the shell (`Ctrl` stands for Ctrl or Cmd; a single character is upper-cased). */
export function comboFromKeyEvent(event: ShellKeyEvent): string | null {
  if (event.isComposing || !(event.ctrlKey || event.metaKey)) return null;
  const key = physicalLatinKey(event);
  if (!key || key.length !== 1) return null;
  const parts = ['Ctrl'];
  if (event.altKey) parts.push('Alt');
  if (event.shiftKey) parts.push('Shift');
  parts.push(key);
  return parts.join('+');
}

/** The combo to forward for this key event, or null when the editor keeps the key. */
export function matchShellShortcut(
  event: ShellKeyEvent,
  allowed: ReadonlySet<string> | readonly string[],
): string | null {
  if (event.isComposing) return null;
  const combo = comboFromKeyEvent(event);
  if (combo === null) return null;
  const matched = 'has' in allowed ? allowed.has(combo) : allowed.includes(combo);
  return matched ? combo : null;
}

export type ShellShortcutSink = (payload: { schemaVersion: 1; reason: typeof SHELL_SHORTCUT_REASON; shortcut: string }) => void;

/** Holds the embed port's event listeners and delivers forwarded combos to them. */
export function createShellShortcutRelay() {
  const sinks = new Set<ShellShortcutSink>();
  return {
    add(sink: ShellShortcutSink): () => void {
      sinks.add(sink);
      return () => {
        sinks.delete(sink);
      };
    },
    hasListener: () => sinks.size > 0,
    forward(combo: string): void {
      for (const sink of sinks) sink({ schemaVersion: 1, reason: SHELL_SHORTCUT_REASON, shortcut: combo });
    },
  };
}
