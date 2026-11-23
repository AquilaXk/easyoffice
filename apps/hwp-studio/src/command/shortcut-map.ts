// EasyOffice extension: physical KeyA..KeyZ normalization fallback for non-ASCII IMEs (PD-326).
import {
  defaultShortcuts,
  matchShortcut as upstreamMatchShortcut,
  type ShortcutDef,
} from '@upstream/command/shortcut-map';
import { detectPlatformKind, type PlatformKind } from '@upstream/engine/navigation-keymap';
import { physicalLatinKey } from '../embed/shell-shortcuts.ts';

export { defaultShortcuts, type ShortcutDef };

/**
 * Matches a keyboard shortcut against `shortcuts`, falling back to physical `KeyA..KeyZ`
 * when a non-ASCII IME emits Jamo, `Process`, or `Unidentified` while not composing.
 */
export function matchShortcut(
  e: KeyboardEvent,
  shortcuts: [ShortcutDef, string][],
  platform: PlatformKind = detectPlatformKind(),
): string | null {
  if ((e as { isComposing?: boolean }).isComposing) return null;
  const direct = upstreamMatchShortcut(e, shortcuts, platform);
  if (direct !== null) return direct;
  if (!(e.ctrlKey || e.metaKey || e.altKey)) return null;
  if (e.getModifierState?.('AltGraph') && !e.metaKey) return null;
  const rawKey = String(e.key || '');
  if (!(e.altKey || rawKey === 'Process' || rawKey === 'Unidentified' || (rawKey.length === 1 && rawKey.charCodeAt(0) > 0x7f))) {
    return null;
  }
  const latin = physicalLatinKey(e);
  if (!latin || latin.length !== 1) return null;
  const normalized = {
    ctrlKey: e.ctrlKey,
    metaKey: e.metaKey,
    shiftKey: e.shiftKey,
    altKey: e.altKey,
    key: latin.toLowerCase(),
    code: e.code,
  } as KeyboardEvent;
  return upstreamMatchShortcut(normalized, shortcuts, platform);
}
