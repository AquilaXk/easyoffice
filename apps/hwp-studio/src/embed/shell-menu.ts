// EasyOffice extension: the editor's 파일 menu is made of shell commands (#158). The shell owns
// documents and windows (new, open, recent, save, save as, close), so the editor only forwards the
// command name over the embed port; the shell re-checks it against its own whitelist. The shell also
// pushes its theme and a recent-documents list (opaque id + display name, never a path) into the
// editor (`setShellContext`), so the window has one menu row and one theme.

export const SHELL_COMMAND_REASON = 'shell_command' as const;
export const SHELL_COMMANDS = ['new', 'open', 'recent', 'save', 'save-as', 'export-pdf', 'close'] as const;
export type ShellCommand = (typeof SHELL_COMMANDS)[number];
export const SHELL_THEMES = ['light', 'dark', 'system'] as const;
export type ShellTheme = (typeof SHELL_THEMES)[number];
export const SHELL_SAVE_FORMATS = ['hwp', 'hwpx'] as const;
const ID_PATTERN = /^[A-Za-z0-9_-]{1,64}$/;
const MAX_RECENTS = 10;
const MAX_NAME_LENGTH = 200;

export interface ShellRecent {
  id: string;
  name: string;
}

export interface ShellContext {
  theme: ShellTheme | null;
  recents: ShellRecent[] | null;
  /** Whether the shell can export a searchable PDF (desktop app only); null = not stated. */
  pdfExport: boolean | null;
}

export interface ShellCommandPayload {
  schemaVersion: 1;
  reason: typeof SHELL_COMMAND_REASON;
  command: ShellCommand;
  id?: string;
  format?: (typeof SHELL_SAVE_FORMATS)[number];
}

export function isShellTheme(value: unknown): value is ShellTheme {
  return typeof value === 'string' && (SHELL_THEMES as readonly string[]).includes(value);
}

/** Theme named in the frame URL (first paint), if valid. */
export function parseShellTheme(search: string): ShellTheme | null {
  const value = new URLSearchParams(search).get('theme');
  return isShellTheme(value) ? value : null;
}

/** Validates what the shell pushed; anything malformed is dropped rather than applied. */
export function sanitizeShellContext(raw: unknown): ShellContext {
  const params = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
  let recents: ShellRecent[] | null = null;
  if (Array.isArray(params.recents)) {
    recents = [];
    for (const item of params.recents.slice(0, MAX_RECENTS)) {
      const entry = item as Record<string, unknown> | null;
      if (!entry || typeof entry.id !== 'string' || !ID_PATTERN.test(entry.id)) continue;
      if (typeof entry.name !== 'string' || entry.name.trim() === '') continue;
      recents.push({ id: entry.id, name: entry.name.slice(0, MAX_NAME_LENGTH) });
    }
  }
  const pdfExport = typeof params.pdfExport === 'boolean' ? params.pdfExport : null;
  return { theme: isShellTheme(params.theme) ? params.theme : null, recents, pdfExport };
}

export type ShellCommandSink = (payload: ShellCommandPayload) => void;

/** Holds the embed port's event listeners and delivers menu commands to them. */
export function createShellCommandRelay() {
  const sinks = new Set<ShellCommandSink>();
  return {
    add(sink: ShellCommandSink): () => void {
      sinks.add(sink);
      return () => {
        sinks.delete(sink);
      };
    },
    hasListener: () => sinks.size > 0,
    forward(payload: ShellCommandPayload): void {
      for (const sink of sinks) sink(payload);
    },
  };
}

/** The command a menu item stands for, or null when it names none of the whitelisted commands. */
export function payloadForItem(item: { dataset: DOMStringMap }): ShellCommandPayload | null {
  const command = item.dataset.shellCmd;
  if (!command || !(SHELL_COMMANDS as readonly string[]).includes(command)) return null;
  const payload: ShellCommandPayload = {
    schemaVersion: 1,
    reason: SHELL_COMMAND_REASON,
    command: command as ShellCommand,
  };
  if (command === 'recent') {
    const id = item.dataset.shellId ?? '';
    if (!ID_PATTERN.test(id)) return null;
    payload.id = id;
  }
  if (command === 'save-as') {
    const format = item.dataset.shellFormat ?? 'hwp';
    if (!(SHELL_SAVE_FORMATS as readonly string[]).includes(format)) return null;
    payload.format = format as (typeof SHELL_SAVE_FORMATS)[number];
  }
  return payload;
}

/** Fills the recent-documents submenu with the shell's list (text only, never markup). */
export function renderShellRecents(panel: HTMLElement, recents: readonly ShellRecent[]): void {
  panel.replaceChildren();
  const make = (label: string, disabled: boolean): HTMLElement => {
    const item = document.createElement('div');
    item.className = disabled ? 'md-item disabled' : 'md-item';
    const icon = document.createElement('span');
    icon.className = 'md-icon';
    const text = document.createElement('span');
    text.className = 'md-label';
    text.textContent = label;
    item.append(icon, text);
    return item;
  };
  if (recents.length === 0) {
    panel.append(make('(최근 문서 없음)', true));
    return;
  }
  for (const entry of recents) {
    const item = make(entry.name, false);
    item.dataset.shellCmd = 'recent';
    item.dataset.shellId = entry.id;
    item.title = entry.name;
    panel.append(item);
  }
}

/** Wires the 파일 menu items (`data-shell-cmd`) to the relay and closes the menu after a click. */
export function installShellMenu(menuBar: HTMLElement, relay: ReturnType<typeof createShellCommandRelay>): void {
  menuBar.addEventListener('click', (event) => {
    const target = (event.target as HTMLElement | null)?.closest<HTMLElement>('[data-shell-cmd]');
    if (!target || target.classList.contains('disabled')) return;
    const payload = payloadForItem(target);
    if (payload && relay.hasListener()) relay.forward(payload);
    // The editor's menu bar closes on a mousedown outside itself.
    document.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
  });
}
