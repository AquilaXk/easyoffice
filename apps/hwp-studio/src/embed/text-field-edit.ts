// EasyOffice extension: the shell's edit menu (macOS binds ⌘Z, ⇧⌘Z, ⌘X, ⌘C to menu items, so those keys
// never reach this frame) sends `edit:undo`, `edit:redo`, `edit:cut` and `edit:copy` over the embed port.
// When a text field of a dialog has the focus, the command belongs to that field: running it on the
// document would cut or copy the document's selection while the user edits a field.

export const TEXT_FIELD_EDIT_COMMANDS = {
  'edit:undo': 'undo',
  'edit:redo': 'redo',
  'edit:cut': 'cut',
  'edit:copy': 'copy',
} as const;

/** The editor's own hidden typing target (rhwp-studio `InputHandler`): it is the document, not a field. */
export const EDITOR_INPUT_LABEL = '문서 편집 입력';

/** Input types that hold no text. */
const NON_TEXT_INPUT_TYPES = new Set([
  'button', 'checkbox', 'color', 'file', 'hidden', 'image', 'radio', 'range', 'reset', 'submit',
]);

export interface FocusedElement {
  tagName: string;
  type?: string;
  getAttribute(name: string): string | null;
}

/** The `execCommand` name for an edit command when a text field (not the editor's own input) has the focus. */
export function textFieldEditCommand(id: string, active: FocusedElement | null): string | null {
  if (!Object.hasOwn(TEXT_FIELD_EDIT_COMMANDS, id) || !active) return null;
  const tag = active.tagName.toUpperCase();
  const isField = tag === 'TEXTAREA' || (tag === 'INPUT' && !NON_TEXT_INPUT_TYPES.has((active.type ?? 'text').toLowerCase()));
  if (!isField || active.getAttribute('aria-label') === EDITOR_INPUT_LABEL) return null;
  return TEXT_FIELD_EDIT_COMMANDS[id as keyof typeof TEXT_FIELD_EDIT_COMMANDS];
}

/** Runs an edit command on the focused text field if there is one, otherwise `runDocument`. */
export function runEditCommand<T>(
  id: string,
  active: FocusedElement | null,
  execCommand: (name: string) => boolean,
  runDocument: () => T,
): T | { ok: true } {
  const fieldCommand = textFieldEditCommand(id, active);
  if (!fieldCommand) return runDocument();
  execCommand(fieldCommand);
  return { ok: true };
}
