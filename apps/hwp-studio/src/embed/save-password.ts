// EasyOffice: 보안 ▸ 문서 암호 설정 (PD-396). The dialog is the reference editor's own save-password dialog
// (rhwp-studio `ui/hwp-password-dialog.ts` `showHwpSavePasswordDialog`, MIT) and the export is its WASM
// `exportHwpWithPassword` / `exportHwpxWithPassword` (HWP 5.0 EncryptVersion 4, ODF AES-256-CBC HWPX). The editor
// has no file system here: the shell saves, so the shell asks this frame for the bytes (`exportHwpProtected`,
// `exportHwpxProtected`) and checks them with `verifyProtectedBytes`. main.ts passes the dialog in.
//
// The password is typed in this frame, kept in one closure variable and used only by the export and the check.
// It is never returned, stored, logged, put in an error message or sent over the embed port; the shell learns
// only whether a password is set (`passwordSet`). A new, replaced or opened document drops it.
import type { CommandDef } from '@/command/types';
import type { WasmBridge } from '@/core/wasm-bridge';

export const SET_DOCUMENT_PASSWORD_COMMAND_ID = 'file:set-password';

const NOT_SET_MESSAGE = '문서 암호가 설정되어 있지 않습니다.';
// The engine's own message may repeat what it was given, so the real one is never passed on.
const EXPORT_FAILED_MESSAGE = '암호 문서를 만들지 못했습니다.';
const VERIFY_FAILED_MESSAGE = '암호 문서를 다시 열지 못했습니다.';

export type ProtectedFormat = 'hwp' | 'hwpx';

/** What the check needs of a document opened with a password (the engine's `HwpDocument`). */
export interface ProtectedProbe {
  pageCount(): number;
  getSourceFormat(): string;
  free(): void;
}

export interface SavePassword {
  readonly command: CommandDef;
  isSet(): boolean;
  clear(): void;
  exportBytes(wasm: WasmBridge, format: ProtectedFormat): Uint8Array;
  verifyBytes(
    data: Uint8Array,
    open: (data: Uint8Array, password: string) => ProtectedProbe,
  ): { ok: true; pageCount: number; format: string };
}

/**
 * `ask` is the reference editor's `showHwpSavePasswordDialog` (main.ts passes it): it takes the file name and
 * answers the new password once it was typed twice, or null when the dialog was closed.
 */
export function createSavePassword(ask: (fileName: string) => Promise<string | null>): SavePassword {
  let password: string | null = null;

  const requirePassword = (): string => {
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
      password = next;
      // The file written next differs from the one on disk, so the document counts as changed.
      services.documentState.markDirty('document-password');
    },
  };

  return {
    command,
    isSet: () => password !== null,
    clear() {
      password = null;
    },
    exportBytes(wasm, format) {
      const secret = requirePassword();
      try {
        return format === 'hwpx' ? wasm.exportHwpxWithPassword(secret) : wasm.exportHwpWithPassword(secret);
      } catch {
        throw new Error(EXPORT_FAILED_MESSAGE);
      }
    },
    verifyBytes(data, open) {
      const secret = requirePassword();
      let probe: ProtectedProbe;
      try {
        probe = open(data, secret);
      } catch {
        throw new Error(VERIFY_FAILED_MESSAGE);
      }
      try {
        return { ok: true, pageCount: probe.pageCount(), format: probe.getSourceFormat() };
      } catch {
        throw new Error(VERIFY_FAILED_MESSAGE);
      } finally {
        probe.free();
      }
    },
  };
}
