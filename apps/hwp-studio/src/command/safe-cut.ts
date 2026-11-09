// EasyOffice-only module (참조 앱 port, see clipboard-cut.ts): the `edit:cut` command that deletes a
// text selection only after the system clipboard accepted it. Every way to cut (the menu the shell
// forwards, Ctrl+X, the toolbar, `automation.execute`) runs this one command. Object, header/footer,
// footnote, cell-block and form-mode cuts keep the editor's own command.
import type { CommandDef } from '@/command/types';
import { prepareRhwpInternalClipboardHtml, writeTextHtmlToClipboard } from '@/engine/input-handler-keyboard';
import { cutAfterWrite, textCutPlan, type ClipboardWasm, type CutHandler } from './clipboard-cut.ts';

type CursorProbe = { cursor?: { isInHeaderFooter?: () => boolean; isInFootnote?: () => boolean } };

export function safeCutCommand(base: CommandDef): CommandDef {
  return {
    ...base,
    execute(services, params) {
      const handler = services.getInputHandler();
      const ctx = services.getContext();
      const cursor = (handler as unknown as CursorProbe | null)?.cursor;
      const plainTextSelection =
        ctx.hasSelection && !ctx.inPictureObjectSelection && !ctx.inTableObjectSelection && !ctx.inCellSelectionMode && !ctx.isFormMode &&
        !cursor?.isInHeaderFooter?.() && !cursor?.isInFootnote?.();
      if (!handler || !plainTextSelection) {
        base.execute(services, params);
        return;
      }
      const plan = textCutPlan(services.wasm as unknown as ClipboardWasm, handler as unknown as CutHandler, {
        markInternalHtml: (html, text) => prepareRhwpInternalClipboardHtml(handler, html, text),
        writeTextHtml: writeTextHtmlToClipboard,
      });
      if (!plan) return;
      void cutAfterWrite(plan)
        .then((result) => {
          if (result === 'not-written') console.warn('[edit:cut] 시스템 클립보드에 쓰지 못해 선택한 내용을 지우지 않았습니다.');
        })
        .catch((error) => console.warn('[edit:cut] 오려 두기 실패:', error));
    },
  };
}
