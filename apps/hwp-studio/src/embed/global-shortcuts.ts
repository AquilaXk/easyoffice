// EasyOffice: the document-wide keydown handler, split out of main.ts. It acts whatever owns the
// focus (navigation keys, zoom keys, undo/redo fallback, Alt+N and Ctrl/Cmd+O without a document).
import { defaultShortcuts, matchShortcut } from '@/command/shortcut-map';
import { handleDocumentSelectAllShortcut, isTextEditingTarget } from '@/command/document-shortcut-guard';
import type { CommandDispatcher } from '@/command/dispatcher';
import type { InputHandler } from '@/engine/input-handler';
import type { CanvasView } from '@/view/canvas-view';

/** 포커스 주인과 무관하게 문서를 움직여야 하는 키 — 편집기 경로로 넘긴다. */
const DOCUMENT_NAVIGATION_KEYS = new Set(['PageUp', 'PageDown', 'Home', 'End']);
const GLOBAL_VIEW_SHORTCUTS = new Set([
  'view:zoom-in',
  'view:zoom-out',
  'view:zoom-100',
  'view:toolbox-basic',
]);

export interface GlobalShortcutDeps {
  dispatcher: CommandDispatcher;
  getInputHandler: () => InputHandler | null;
  getCanvasView: () => CanvasView | null;
}

/**
 * 전역 단축키 핸들러 — InputHandler.active 여부와 무관하게 동작해야 하는 단축키.
 * 예: 문서 미로드 상태에서도 Alt+N(새 문서), Ctrl+O(열기) 등.
 */
export function setupGlobalShortcuts({ dispatcher, getInputHandler, getCanvasView }: GlobalShortcutDeps): void {
  /** 문서 이동 키(PgUp/PgDn·Home/End)를 처리했으면 true. */
  function handleDocumentNavigationShortcut(e: KeyboardEvent): boolean {
    if (!DOCUMENT_NAVIGATION_KEYS.has(e.key) || e.altKey) return false;
    const inputHandler = getInputHandler();
    if (inputHandler?.isActive()) {
      inputHandler.handleDocumentNavigationKey(e);
      return true;
    }
    if (e.key === 'PageUp' || e.key === 'PageDown') {
      // 문서는 떠 있지만 편집기가 활성이 아닌 보기 상태 — 화면만 옮긴다.
      e.preventDefault();
      getCanvasView()?.scrollByPage(e.key === 'PageUp' ? -1 : 1);
      return true;
    }
    return false;
  }

  /**
   * textarea가 아닌 곳에 포커스가 빠진 활성 편집기는 자체 keydown을 받지 못한다.
   * 이때 undo/redo(와 전체 선택)만 dispatcher로 보완한다. textarea가 target이면 호출 전에
   * 이미 return하므로 InputHandler와 이중 실행되지 않으며, 다른 단축키의 전역 소유 범위도 넓히지 않는다.
   */
  function handleActiveEditorFallbackShortcut(e: KeyboardEvent): void {
    const commandId = matchShortcut(e, defaultShortcuts);
    if (commandId === 'edit:undo' || commandId === 'edit:redo') {
      const result = dispatcher.dispatchWithResult(commandId);
      if (result.ok || result.reason === 'threw') e.preventDefault();
    } else if (commandId === 'edit:select-all') {
      handleDocumentSelectAllShortcut(
        e,
        document.querySelector('.modal-overlay'),
        () => { dispatcher.dispatchWithResult(commandId); },
        () => { getInputHandler()?.focus(); },
      );
    }
  }

  /** 문서 미로드 상태에서도 동작하는 Alt+N(새 문서)·Ctrl/Cmd+O(열기). */
  function handleDocumentlessShortcut(e: KeyboardEvent): void {
    const ctrlOrMeta = e.ctrlKey || e.metaKey;
    if (e.altKey && !ctrlOrMeta && !e.shiftKey && (e.key === 'n' || e.key === 'N' || e.key === 'ㅜ')) {
      e.preventDefault();
      dispatcher.dispatch('file:new-doc');
      return;
    }
    if (ctrlOrMeta && !e.altKey && !e.shiftKey && (e.key === 'o' || e.key === 'O' || e.key === 'ㅐ')) {
      e.preventDefault();
      dispatcher.dispatch('file:open');
    }
  }

  function onGlobalKeydown(e: KeyboardEvent): void {
    // 네이티브 입력 및 contentEditable 내부의 키는 해당 요소가 소유한다.
    if (isTextEditingTarget(e.target)) return;

    // PgUp/PgDn·Home/End 는 문서를 보며 움직이는 키다. 툴바 버튼·서식 콤보를 한 번
    // 누르면 포커스가 편집기 textarea 를 떠나 InputHandler 가 키를 받지 못하고, 스크롤
    // 컨테이너도 포커스 대상이 아니라 브라우저 기본 동작조차 없어 통째로 무동작이 된다.
    // 편집기가 활성이면 keydown 을 그대로 편집기 경로에 넘겨, 포커스가 어디에 있든 같은
    // 분기·같은 결과(캐럿 이동 + 화면 이동)를 준다 — 여기에 로직을 복제하지 않는다.
    // (글자 편집 요소가 target 이면 위에서 이미 return 하므로 이중 실행되지 않고,
    //  모달이 떠 있으면 Dialog 의 capture 핸들러가 먼저 전파를 끊는다.)
    // select 등 이 키를 자체 소비하는 위젯보다 문서 이동을 우선한다 — studio 에서
    // chrome 위젯은 스쳐 가는 대상이고 사용자가 보고 있는 것은 문서다.
    if (handleDocumentNavigationShortcut(e)) return;
    // 배율·도구 상자 키는 편집 textarea에서는 InputHandler가 소유하고, 그 밖의 포커스에서는
    // 이 전역 경로가 같은 커맨드를 한 번만 실행한다. 브라우저 기본 페이지 줌은 막는다.
    const globalShortcutId = matchShortcut(e, defaultShortcuts);
    if (globalShortcutId && GLOBAL_VIEW_SHORTCUTS.has(globalShortcutId)) {
      e.preventDefault();
      dispatcher.dispatch(globalShortcutId);
      return;
    }
    if (getInputHandler()?.isActive()) {
      handleActiveEditorFallbackShortcut(e);
      return;
    }
    handleDocumentlessShortcut(e);
  }

  document.addEventListener('keydown', onGlobalKeydown, false);
}
