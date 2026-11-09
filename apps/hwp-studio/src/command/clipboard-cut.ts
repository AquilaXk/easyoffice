// EasyOffice-only module (참조 앱 port: studio-host `command/commands/edit.ts`, lines 20-150, MIT,
// Copyright (c) 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md): a cut removes the selected text only
// after the system clipboard has accepted it. The editor's own cut runs the browser's cut command,
// which deletes the selection even when the clipboard refuses a write that the user did not trigger
// (a menu item sends the command, so the frame has no user gesture). Differences from the reference
// app: a selection inside a nested table is copied by its cell path (as the editor's copy event does),
// and the clipboard write is injected so the ordering is testable without a browser.
// No runtime imports: the editor modules arrive through `ClipboardCutDeps`.
import type { DocumentPosition } from '@/core/types';

export interface SelectionRange {
  start: DocumentPosition;
  end: DocumentPosition;
}

/** The slice of the editor's WASM bridge that copying a text selection uses. */
export interface ClipboardWasm {
  getClipboardText(): string;
  copySelection(sec: number, startPara: number, startOffset: number, endPara: number, endOffset: number): unknown;
  exportSelectionHtml(sec: number, startPara: number, startOffset: number, endPara: number, endOffset: number): string;
  copySelectionInCell(
    sec: number, parentPara: number, controlIndex: number, cellIndex: number,
    startCellPara: number, startOffset: number, endCellPara: number, endOffset: number,
  ): unknown;
  exportSelectionInCellHtml(
    sec: number, parentPara: number, controlIndex: number, cellIndex: number,
    startCellPara: number, startOffset: number, endCellPara: number, endOffset: number,
  ): string;
  copySelectionInCellByPath(
    sec: number, parentPara: number, pathJson: string,
    startCellPara: number, startOffset: number, endCellPara: number, endOffset: number,
  ): unknown;
  exportSelectionInCellHtmlByPath(
    sec: number, parentPara: number, pathJson: string,
    startCellPara: number, startOffset: number, endCellPara: number, endOffset: number,
  ): string;
}

/** The slice of the editor's input handler that a cut uses. */
export interface CutHandler {
  getSelection(): SelectionRange | null;
  performDelete(): void;
}

export interface ClipboardCutDeps {
  /** Marks the HTML as the editor's own clipboard content (so a paste inside the editor keeps formatting). */
  markInternalHtml(html: string, text: string): string;
  /** Writes text and HTML to the system clipboard; rejects when the system refuses. */
  writeTextHtml(text: string, html: string): Promise<void>;
}

export interface CutPlan {
  write(): Promise<void>;
  /** True while the selection the clipboard was filled from is still the selection. */
  unchanged(): boolean;
  remove(): void;
}

export type CutResult = 'cut' | 'not-written' | 'selection-changed';

/** Writes first; removes only when the write was accepted and nothing moved meanwhile. */
export async function cutAfterWrite(plan: CutPlan): Promise<CutResult> {
  try {
    await plan.write();
  } catch {
    return 'not-written';
  }
  if (!plan.unchanged()) return 'selection-changed';
  plan.remove();
  return 'cut';
}

function cellParaIndexOf(position: DocumentPosition): number {
  const path = position.cellPath;
  return path && path.length > 0 ? path[path.length - 1].cellParaIndex : position.cellParaIndex!;
}

function isNestedCellPosition(position: DocumentPosition): boolean {
  return position.parentParaIndex !== undefined && (position.cellPath?.length ?? 0) > 1;
}

function exportOrEmpty(read: () => string): string {
  try {
    return read();
  } catch {
    return '';
  }
}

/** Copies the selection into the editor's own clipboard and returns its HTML ('' when the export fails). */
function copySelectionToWasm(wasm: ClipboardWasm, { start, end }: SelectionRange): string {
  if (isNestedCellPosition(start)) {
    const path = JSON.stringify(start.cellPath);
    const args = [start.sectionIndex, start.parentParaIndex!, path, cellParaIndexOf(start), start.charOffset, cellParaIndexOf(end), end.charOffset] as const;
    wasm.copySelectionInCellByPath(...args);
    return exportOrEmpty(() => wasm.exportSelectionInCellHtmlByPath(...args));
  }
  if (start.parentParaIndex !== undefined) {
    const args = [
      start.sectionIndex, start.parentParaIndex, start.controlIndex!, start.cellIndex!,
      start.cellParaIndex!, start.charOffset, end.cellParaIndex!, end.charOffset,
    ] as const;
    wasm.copySelectionInCell(...args);
    return exportOrEmpty(() => wasm.exportSelectionInCellHtml(...args));
  }
  const args = [start.sectionIndex, start.paragraphIndex, start.charOffset, end.paragraphIndex, end.charOffset] as const;
  wasm.copySelection(...args);
  return exportOrEmpty(() => wasm.exportSelectionHtml(...args));
}

function selectionKey(selection: SelectionRange): string {
  const keyOf = (p: DocumentPosition) => ({
    sectionIndex: p.sectionIndex,
    paragraphIndex: p.paragraphIndex,
    charOffset: p.charOffset,
    parentParaIndex: p.parentParaIndex,
    controlIndex: p.controlIndex,
    cellIndex: p.cellIndex,
    cellParaIndex: p.cellParaIndex,
    cellPath: p.cellPath,
    isTextBox: p.isTextBox,
  });
  return JSON.stringify({ start: keyOf(selection.start), end: keyOf(selection.end) });
}

/** A cut of the current text selection, or null when nothing is selected. */
export function textCutPlan(wasm: ClipboardWasm, handler: CutHandler, deps: ClipboardCutDeps): CutPlan | null {
  const selection = handler.getSelection();
  if (!selection) return null;
  const html = copySelectionToWasm(wasm, selection);
  const text = wasm.getClipboardText();
  const marked = deps.markInternalHtml(html, text);
  const key = selectionKey(selection);
  return {
    write: () => deps.writeTextHtml(text, marked),
    unchanged() {
      const now = handler.getSelection();
      return now !== null && selectionKey(now) === key;
    },
    remove: () => handler.performDelete(),
  };
}
