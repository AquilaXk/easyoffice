// EasyOffice: 보안 ▸ 문서 암호 설정 (PD-396). The dialog is the reference editor's own save-password dialog
// (rhwp-studio `ui/hwp-password-dialog.ts` `showHwpSavePasswordDialog`, MIT) and the export is its WASM
// `exportHwpWithPassword` / `exportHwpxWithPassword` (HWP 5.0 EncryptVersion 4, ODF AES-256-CBC HWPX). The editor
// has no file system here: the shell saves, so the shell asks this frame for the bytes (`exportHwpProtected`,
// `exportHwpxProtected`) and checks them with `verifyProtectedBytes`. main.ts passes the dialog in.
//
// The password is typed in this frame, kept in one closure variable and used only by the export and the check.
// It is never returned, stored, logged, put in an error message or sent over the embed port; the shell learns
// only whether a password is set (`passwordSet`). It belongs to the document it was set for: a new, replaced or
// opened document (a new `documentGeneration` of the bridge) drops it, and a creation that failed does not.
import type { CommandDef } from '@/command/types';
import type { WasmBridge } from '@/core/wasm-bridge';

export const SET_DOCUMENT_PASSWORD_COMMAND_ID = 'file:set-password';

const NOT_SET_MESSAGE = '문서 암호가 설정되어 있지 않습니다.';
// The engine's own message may repeat what it was given, so the real one is never passed on.
const EXPORT_FAILED_MESSAGE = '암호 문서를 만들지 못했습니다.';
const VERIFY_FAILED_MESSAGE = '암호 문서를 다시 열지 못했습니다.';
const OPENS_WITHOUT_PASSWORD_MESSAGE = '암호 문서가 암호 없이도 열립니다.';

export type ProtectedFormat = 'hwp' | 'hwpx';

/** What the check needs of a document the engine opened (its `HwpDocument`). */
export interface ProtectedProbe {
  pageCount(): number;
  getSourceFormat(): string;
  free(): void;
}

/** The part of the bridge a password belongs to. */
type DocumentBridge = Pick<WasmBridge, 'documentGeneration'>;

export interface SavePassword {
  readonly command: CommandDef;
  isSet(wasm: DocumentBridge): boolean;
  clear(): void;
  exportBytes(wasm: WasmBridge, format: ProtectedFormat): Uint8Array;
  /**
   * Reads `data` back with the password (`open`) and requires that it does not open without one
   * (`openPlain` must throw): a file that opens without the password is not protected.
   */
  verifyBytes(
    wasm: DocumentBridge,
    data: Uint8Array,
    open: (data: Uint8Array, password: string) => ProtectedProbe,
    openPlain: (data: Uint8Array) => ProtectedProbe,
  ): { ok: true; pageCount: number; format: string };
}

/**
 * `ask` is the reference editor's `showHwpSavePasswordDialog` (main.ts passes it): it takes the file name and
 * answers the new password once it was typed twice, or null when the dialog was closed. `onSet` runs after a
 * password was set (main.ts tells the shell, which drops its plain autosave copy at once).
 */
export function createSavePassword(
  ask: (fileName: string) => Promise<string | null>,
  onSet: () => void = () => {},
): SavePassword {
  let held: { password: string; generation: number } | null = null;

  const passwordOf = (wasm: DocumentBridge): string | null => {
    if (held !== null && held.generation !== wasm.documentGeneration) held = null;
    return held === null ? null : held.password;
  };
  const requirePassword = (wasm: DocumentBridge): string => {
    const password = passwordOf(wasm);
    if (password === null) throw new Error(NOT_SET_MESSAGE);
    return password;
  };

  const command: CommandDef = {
    id: SET_DOCUMENT_PASSWORD_COMMAND_ID,
    label: '문서 암호 설정…',
    opensDialog: true,
    canExecute: (ctx) => ctx.hasDocument,
    async execute(services) {
      const next = await ask(services.wasm.fileName);
      if (next === null) return;
      held = { password: next, generation: services.wasm.documentGeneration };
      // The file written next differs from the one on disk, so the document counts as changed.
      services.documentState.markDirty('document-password');
      onSet();
    },
  };

  return {
    command,
    isSet: (wasm) => passwordOf(wasm) !== null,
    clear() {
      held = null;
    },
    exportBytes(wasm, format) {
      const secret = requirePassword(wasm);
      try {
        return format === 'hwpx' ? wasm.exportHwpxWithPassword(secret) : wasm.exportHwpWithPassword(secret);
      } catch {
        throw new Error(EXPORT_FAILED_MESSAGE);
      }
    },
    verifyBytes(wasm, data, open, openPlain) {
      const secret = requirePassword(wasm);
      let probe: ProtectedProbe;
      try {
        probe = open(data, secret);
      } catch {
        throw new Error(VERIFY_FAILED_MESSAGE);
      }
      try {
        const verdict = { ok: true as const, pageCount: probe.pageCount(), format: probe.getSourceFormat() };
        if (opensWithoutPassword(data, openPlain)) throw new Error(OPENS_WITHOUT_PASSWORD_MESSAGE);
        return verdict;
      } catch (error) {
        if (error instanceof Error && error.message === OPENS_WITHOUT_PASSWORD_MESSAGE) throw error;
        throw new Error(VERIFY_FAILED_MESSAGE);
      } finally {
        probe.free();
      }
    },
  };
}

function opensWithoutPassword(data: Uint8Array, openPlain: (data: Uint8Array) => ProtectedProbe): boolean {
  try {
    openPlain(data).free();
    return true;
  } catch {
    return false;
  }
}
