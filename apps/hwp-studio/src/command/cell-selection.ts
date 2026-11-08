// EasyOffice-only module: the `table:cell-selection-enter` command the native 표 menu's 셀 선택 item
// sends (the reference app's menu event id, `apps/desktop/src-tauri/src/menu.rs`, MIT; see
// THIRD_PARTY_NOTICES.md). The editor enters cell selection with F5 only, so the command replays that
// key through the editor's own keyboard path instead of copying its branches.
import type { CommandDef } from '@/command/types';

export const CELL_SELECTION_ENTER_COMMAND_ID = 'table:cell-selection-enter';

export const cellSelectionEnterCommand: CommandDef = {
  id: CELL_SELECTION_ENTER_COMMAND_ID,
  label: '셀 선택',
  shortcutLabel: 'F5',
  canExecute: (ctx) => ctx.inTable && !ctx.inCellSelectionMode,
  execute(services) {
    services.getInputHandler()?.handleDocumentNavigationKey(new KeyboardEvent('keydown', { key: 'F5' }));
  },
};
