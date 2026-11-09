// EasyOffice-only module (참조 앱 port, see clipboard-cut.ts): the `edit:cut` and `edit:copy` commands
// for text selections. `edit:cut` deletes a text selection only after the system clipboard accepted it,
// and `edit:copy` writes both WASM and system clipboards without relying on `document.execCommand('copy')`.
// Object, header/footer, footnote, cell-block and form-mode selections keep the editor's own commands.
import type { CommandDef, CommandServices } from '@/command/types';
import { prepareRhwpInternalClipboardHtml, writeTextHtmlToClipboard } from '@/engine/input-handler-keyboard';
import {
  cutAfterWrite,
  textCopyPlan,
  textCutPlan,
  writeTextHtmlWithPlainFallback,
  type ClipboardWasm,
  type CutHandler,
} from './clipboard-cut.ts';

type CursorProbe = { cursor?: { isInHeaderFooter?: () => boolean; isInFootnote?: () => boolean } };

function writeWithFallback(text: string, html: string): Promise<void> {
  const plainWrite =
    typeof navigator !== 'undefined' && typeof navigator.clipboard?.writeText === 'function'
      ? (plain: string) => navigator.clipboard.writeText(plain)
      : null;
  return writeTextHtmlWithPlainFallback(text, html, writeTextHtmlToClipboard, plainWrite);
}

function isPlainTextSelection(handler: unknown, ctx: ReturnType<CommandServices['getContext']>): boolean {
  const cursor = (handler as CursorProbe | null)?.cursor;
  return Boolean(
    handler &&
      ctx.hasSelection &&
      !ctx.inPictureObjectSelection &&
      !ctx.inTableObjectSelection &&
      !ctx.inCellSelectionMode &&
      !ctx.isFormMode &&
      !cursor?.isInHeaderFooter?.() &&
      !cursor?.isInFootnote?.(),
  );
}

export function safeCutCommand(base: CommandDef): CommandDef {
  return {
    ...base,
    execute(services, params) {
      const handler = services.getInputHandler();
      const ctx = services.getContext();
      if (!handler || !isPlainTextSelection(handler, ctx)) {
        base.execute(services, params);
        return;
      }
      const plan = textCutPlan(services.wasm as unknown as ClipboardWasm, handler as unknown as CutHandler, {
        markInternalHtml: (html, text) => prepareRhwpInternalClipboardHtml(handler, html, text),
        writeTextHtml: writeWithFallback,
      });
      if (!plan) return;
      void cutAfterWrite(plan)
        .then((result) => {
          if (result === 'not-written') {
            console.warn('[edit:cut] 시스템 클립보드에 쓰지 못해 선택한 내용을 지우지 않았습니다.');
            const sb = typeof document !== 'undefined' ? document.querySelector('#sb-message') : null;
            if (sb) sb.textContent = '클립보드에 쓰지 못해 선택한 내용을 지우지 않았습니다.';
          }
        })
        .catch((error) => console.warn('[edit:cut] 오려 두기 실패:', error));
    },
  };
}

export function safeCopyCommand(base: CommandDef): CommandDef {
  return {
    ...base,
    execute(services, params) {
      const handler = services.getInputHandler();
      const ctx = services.getContext();
      if (!handler || !isPlainTextSelection(handler, ctx)) {
        base.execute(services, params);
        return;
      }
      const plan = textCopyPlan(services.wasm as unknown as ClipboardWasm, handler as unknown as CutHandler, {
        markInternalHtml: (html, text) => prepareRhwpInternalClipboardHtml(handler, html, text),
        writeTextHtml: writeWithFallback,
      });
      if (!plan) return;
      void plan.write().catch((error) => console.warn('[edit:copy] 클립보드 복사 실패:', error));
    },
  };
}
