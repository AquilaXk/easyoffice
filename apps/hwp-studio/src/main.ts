import { WasmBridge } from '@/core/wasm-bridge';
import { HwpDocument } from '@wasm/rhwp.js';
import { installDocumentTitle } from '@/ui/document-title';
import type { DocumentInfo, PageInfo } from '@/core/types';
import { EventBus } from '@/core/event-bus';
import { CanvasView } from '@/view/canvas-view';
import { InputHandler } from '@/engine/input-handler';
import { Toolbar } from '@/ui/toolbar';
import { initIconToolbarScroller } from '@/ui/icon-toolbar-scroller';
import { initStyleToolbarOverflow } from '@/ui/style-toolbar-overflow';
import {
  closeToolbarSplitMenus,
  moveToolbarSplitFocus,
  setToolbarSplitOpen,
  toolbarSplitItems,
} from '@/ui/toolbar-split-menu';
import { MenuBar } from '@/ui/menu-bar';
import { loadWebFonts } from '@/core/font-loader';
import { CommandRegistry } from '@/command/registry';
import { AutomationHost } from '@/automation/host';
import { runEditCommand } from './embed/text-field-edit.ts';
import { getChromeVisibility, setChromeVisibility } from '@/automation/chrome';
import type { ChromeVisibility } from '@/automation/types';
import { CommandDispatcher } from '@/command/dispatcher';
import type { EditorContext, CommandServices, EditorEditMode } from '@/command/types';
import { defaultShortcuts, matchShortcut } from '@/command/shortcut-map';
import { handleDocumentSelectAllShortcut, isTextEditingTarget } from '@/command/document-shortcut-guard';
import { confirmSaveBeforeReplacingDocument, fileCommands } from '@/command/commands/file';
import { editCommands } from '@/command/commands/edit';
import { syncClipMenu, syncTextMarkMenu, syncToolboxMenu, viewCommands } from '@/command/commands/view';
import { formatCommands } from '@/command/commands/format';
import { insertCommands } from '@/command/commands/insert';
import { tableCommands } from '@/command/commands/table';
import { pageCommands } from '@/command/commands/page';
import { toolCommands } from '@/command/commands/tool';
import {
  isSupportedDocumentFileName,
  type FileSystemFileHandleLike,
} from '@/command/file-system-access';
import { forgetConvertedHmlSaveHandle } from '@/command/save-target';
import { ContextMenu } from '@/ui/context-menu';
import { CommandPalette } from '@/ui/command-palette';
import { MODAL_DIALOG_CLOSED_EVENT } from '@/ui/dialog';
import { showHmlImportWarning } from '@/ui/hml-import-warning';
import { showToast } from '@/ui/toast';
import { showDropConfirmDialog } from '@/ui/drop-confirm-dialog';
import { showHwpPasswordDialog } from '@/ui/hwp-password-dialog';
import {
  EMBED_HIDDEN_EDIT_COMMAND_IDS,
  EMBED_HIDDEN_FILE_COMMAND_IDS,
  isEmbedSwallowedFileShortcut,
} from '@/ui/chrome-mode';
import { DocumentDirtyState } from '@/core/document-dirty-state';
import { initThemeSync, setThemeMode, getThemeMode } from '@/core/theme';
import { analyzeDocumentFonts } from '@/core/document-font-status';
import { setDocumentFontSubstitutions } from '@/core/font-substitution';
import {
  getLocalFontState,
  loadStoredLocalFonts,
  setHostFontProvider,
  onHostFontsChanged,
  getHostFontState,
} from '@/core/local-fonts';
import { userSettings } from '@/core/user-settings';
import { CellSelectionRenderer } from '@/engine/cell-selection-renderer';
import { cellSelectionPhaseLabel, type CellSelectionPhase } from '@/engine/cell-selection-phase';
import { TableObjectRenderer } from '@/engine/table-object-renderer';
import { TableResizeRenderer } from '@/engine/table-resize-renderer';
import { Ruler } from '@/view/ruler';
import { detectPlatformKind } from '@/engine/navigation-keymap';
import {
  headerFooterApplyToLabel,
  parseHeaderFooterModeChanged,
} from '@/engine/header-footer-mode';
import {
  percentToZoomSliderPosition,
  zoomPercentShortcutTitle,
  zoomSliderPositionToPercent,
} from '@/view/zoom-status-controls';
import { RendererSession, type RendererSessionDiagnostics } from '@/view/renderer-session';
import type { RenderBackendFallbackReason } from '@/view/render-backend';
import { normalizeZoomFitMode, resolveZoomFitZoom, type ZoomFitMode } from '@/view/zoom-fit';
import { CENTER_ZOOM_ANCHOR } from '@/view/zoom-anchor';
import { withBusyCursor } from '@/view/busy-cursor';
import { formatPageIndicator } from '@/view/page-indicator';
import { installEmbedRuntime } from '@/embed/runtime';
import { createShellShortcutRelay, matchShellShortcut, parseShellKeys } from './embed/shell-shortcuts.ts';
import {
  createShellCommandRelay,
  installShellMenu,
  parseShellTheme,
  renderShellRecents,
  sanitizeShellContext,
} from './embed/shell-menu.ts';
import { renderPrintBatch } from './embed/print-batch.ts';
import { createShellFontBridge } from './embed/host-fonts.ts';
import { initToolbarLabels, toolbarLabelsCommand } from './ui/toolbar-labels.ts';
import { cellSelectionEnterCommand } from './command/cell-selection.ts';
import type { EmbedRendererRuntimeRequestV1 } from '@/embed/rpc-router';
import { enrichFontDecisionTrace } from '@/core/font-decision-trace';
import { DocumentAgentController } from '@/document-agent/controller';
import { initI18n, t } from '@/i18n/index.ts';
import { applyMacShortcutPresentation, installNonEditorContextMenuGuards } from './embed/desktop-chrome.ts';

// 언어팩 초기화 — 정적 마크업의 라벨을 결정된 로케일로 갱신한다. 카탈로그에 없는 키는
// 원문(ko)으로 물러나므로 번역이 없는 상태에서도 화면은 도입 전과 같다.
initI18n();

const wasm = new WasmBridge();
installDocumentTitle(wasm);
const eventBus = new EventBus();
const documentState = new DocumentDirtyState(eventBus);
documentState.installBeforeUnload(window);
// EasyOffice: the shell's theme (light, dark or system) is applied before the first paint.
const shellThemeAtStart = parseShellTheme(window.location.search);
if (shellThemeAtStart) setThemeMode(shellThemeAtStart);
initThemeSync((effective, mode) => {
  eventBus.emit('theme-changed', { mode, effective });
  eventBus.emit('command-state-changed');
});
// 저장된 도구 상자(기본/서식) 보이기·숨기기 복원 — 문서 로드와 무관하고, WASM 초기화보다
// 먼저 반영해야 숨긴 도구 모음이 잠깐 보였다 사라지지 않는다(모듈 스크립트라 DOM 은 이미 파싱됨).
syncToolboxMenu();
const iconToolbarScroller = initIconToolbarScroller(document.getElementById('icon-toolbar'));
initStyleToolbarOverflow(document.getElementById('style-bar'));

/**
 * 호스트 저장 완료 통지 (#2660).
 *
 * 호스트가 내보내기 바이트의 영속화(업로드/핸드오프)를 마친 뒤 호출한다.
 * draft 삭제 "완료"까지 await하므로, resolve 이후 팝업을 닫아도 IndexedDB
 * 삭제가 잘리지 않는다. export 시점에는 호출하지 않는다(실패 시 백업 보존).
 */
function completeHostSave(fileName?: string): Promise<{ ok: true; wasDirty: boolean }> {
  return Promise.resolve().then(() => {
    const wasDirty = documentState.isDirty();
    if (fileName) wasm.fileName = fileName;
    documentState.markClean('host-save');
    return { ok: true as const, wasDirty };
  });
}

// 호스트 통합용 공개 API — 팝업/포크 등 SDK 없이 스튜디오 페이지 안에서 통합하는
// 호스트를 위해 프로덕션 빌드에도 항상 노출한다 (iframe 호스트는 embed RPC 사용).
(window as any).rhwpStudio = {
  notifySaved: (fileName?: string) => completeHostSave(fileName),
  fonts: { setProvider: setHostFontProvider, getState: getHostFontState },
};

let canvasView: CanvasView | null = null;
let inputHandler: InputHandler | null = null;
let documentAgent: DocumentAgentController | null = null;
let toolbar: Toolbar | null = null;
let ruler: Ruler | null = null;
let rendererSession: RendererSession | null = null;
let editMode: EditorEditMode = 'normal';
let rendererRuntimeRequest: EmbedRendererRuntimeRequestV1 | null = null;
let renderBackendFallbackReason: RenderBackendFallbackReason | null = null;
let rendererInitializationError: string | null = null;
let rendererInitialized = false;
// EasyOffice: external web fonts are never loaded (bundled fonts only).
const extensionViewerSettings = { disableExternalWebFonts: true } as const;



// ─── UI chrome 프로파일


// ─── UI chrome 프로파일 (#4564) ─────────────────────────────
// 문서 수명주기(열기/저장)를 호스트가 소유하는 임베드 구성용 opt-in 스위치.
// 파라미터가 없거나 미지원 값이면 full — 기존 표면 그대로다.
// EasyOffice: this build is embed-only. The host (parent window) owns open/save/new.
const chromeMode: 'embed' | 'full' = 'embed';

// ─── 커맨드 시스템 ─────────────────────────────
const registry = new CommandRegistry();

function getContext(): EditorContext {
  const hasDoc = wasm.pageCount > 0;
  const canEditFormField = inputHandler?.canEditCurrentFormField() ?? false;
  const isFormMode = editMode === 'form';
  return {
    hasDocument: hasDoc,
    hasSelection: inputHandler?.hasSelection() ?? false,
    hasCopiedFormat: inputHandler?.hasCopiedFormat() ?? false,
    inTable: inputHandler?.isInTable() ?? false,
    inCellSelectionMode: inputHandler?.isInCellSelectionMode() ?? false,
    hasMultiCellSelection: inputHandler?.hasMultiCellSelection() ?? false,
    hasTableTransposeClipboard: wasm.hasTableTransposeClipboard(),
    inTableObjectSelection: inputHandler?.isInTableObjectSelection() ?? false,
    inPictureObjectSelection: inputHandler?.isInPictureObjectSelection() ?? false,
    inField: inputHandler?.isInField() ?? false,
    isEditable: !isFormMode || canEditFormField,
    editMode,
    isFormMode,
    canEditFormField,
    canUndo: inputHandler?.canUndo() ?? false,
    canRedo: inputHandler?.canRedo() ?? false,
    zoom: canvasView?.getViewportManager().getZoom() ?? 1.0,
    showControlCodes: wasm.getShowControlCodes(),
    showParagraphMarks: wasm.getShowParagraphMarks(),
    isDirty: documentState.isDirty(),
    sourceFormat: hasDoc ? (wasm.getSourceFormat() as 'hwp' | 'hwpx' | 'hml') : undefined,
  };
}

function setEditMode(mode: EditorEditMode): void {
  editMode = mode;
  inputHandler?.setEditMode(mode);
  document.documentElement.dataset.editMode = mode;
  document.querySelectorAll('[data-cmd="view:form-mode"]').forEach(el => {
    el.classList.toggle('active', mode === 'form');
  });
  sbMessage().textContent = mode === 'form' ? t('ui.sbMessage.formMode') : t('ui.sbMessage.editMode');
  eventBus.emit('edit-mode-changed', mode);
  eventBus.emit('command-state-changed');
}

const commandServices: CommandServices = {
  eventBus,
  wasm,
  documentState,
  getContext,
  getInputHandler: () => inputHandler,
  getViewportManager: () => canvasView?.getViewportManager() ?? null,
  gotoPage: (globalPage) => canvasView?.gotoPage(globalPage) ?? false,
  refreshDocumentStatus: () => {
    sbMessage().textContent = t('ui.sbMessage.filePages', { p1: wasm.fileName, p2: wasm.pageCount });
  },
  setEditMode,
};

const dispatcher = new CommandDispatcher(registry, commandServices, eventBus);

// 자동화 표면 — 외부 JavaScript 가 커맨드·메뉴를 다루는 자리. 실행은 메뉴·툴바·키보드와 같은
// dispatcher 를 타므로 게이트가 갈리지 않는다. 메뉴 컨테이너는 이 시점에 아직 없을 수 있어
// 함수로 넘긴다.
const automation = new AutomationHost({
  registry,
  dispatcher,
  getContext,
  getMenuContainer: () => document.getElementById('menu-bar'),
});
(window as any).rhwpStudio.automation = automation;


// 모든 내장 커맨드 등록. embed 프로파일에서는 문서 수명주기 커맨드를 등록하지 않는다 —
// registerAll이 메뉴 클릭·단축키·전역 단축키·커맨드 팔레트가 모두 지나는 choke point라
// 이 필터 하나로 충분하다. 파일 수명주기 커맨드에 더해 edit:compare-documents도 거른다:
// 비교 실행이 오른쪽 문서를 현재 에디터에 로드하는, 호스트가 감지할 수 없는 문서 교체
// 진입점이다. shortcut-map의 파일 매핑은 그대로 둔다: 매핑이 남아야 Ctrl+S/Ctrl+P가
// preventDefault로 계속 삼켜져 브라우저 저장/인쇄 대화상자로 빠지지 않는다.
// Ctrl+Shift+S의 셀 블록 문맥 라우터도 Save As가 미등록이면 이벤트만 소비하므로
// table:block-sum으로 폴스루하지 않는다. 미등록 커맨드 dispatch는 무해하게 false를 반환한다.
registry.registerAll(
  chromeMode === 'embed'
    ? fileCommands.filter((cmd) => !EMBED_HIDDEN_FILE_COMMAND_IDS.includes(cmd.id))
    : fileCommands,
);
registry.registerAll(
  chromeMode === 'embed'
    ? editCommands.filter((cmd) => !EMBED_HIDDEN_EDIT_COMMAND_IDS.includes(cmd.id))
    : editCommands,
);
registry.registerAll(viewCommands);
registry.register(toolbarLabelsCommand);
initToolbarLabels();
registry.registerAll(formatCommands);
registry.registerAll(insertCommands);
registry.registerAll(tableCommands);
registry.register(cellSelectionEnterCommand);
registry.registerAll(pageCommands);
registry.registerAll(toolCommands);

/**
 * embed 프로파일의 메뉴·도구막대 정리 — index.html은 그대로 두고 부트 시 런타임에
 * 제거한다(기본 full 프로파일의 정적 마크업·테스트에 무회귀). 숨김 커맨드를 참조하는
 * 모든 표면(파일 메뉴 항목, 편집 메뉴의 문서 비교, 도구막대 버튼·split 메뉴 항목)을
 * data-cmd로 일괄 제거한다. 모듈 스크립트는 문서 파스 후 실행되므로 이 시점의 톱레벨
 * DOM 접근은 안전하고, MenuBar는 이후 initialize()에서 정리된 DOM을 읽는다.
 * `#file-input`은 커맨드 표면이 아니라 입력 채널이므로 건드리지 않는다 —
 * setupFileInput이 존재를 전제한다.
 */
function pruneEmbedChrome(): void {
  for (const id of [...EMBED_HIDDEN_FILE_COMMAND_IDS, ...EMBED_HIDDEN_EDIT_COMMAND_IDS]) {
    document.querySelectorAll(`[data-cmd="${id}"]`).forEach((item) => item.remove());
  }
  const dropdown = document.querySelector('.menu-item[data-menu="file"] .menu-dropdown');
  if (!dropdown) return;
  dropdown.querySelectorAll('.md-sub[data-recent]').forEach((sub) => sub.remove());
  // 고아가 된 구분선 정리: 선행 항목이 없거나 구분선끼리 연속이면 제거하고,
  // 말단에 남은 구분선도 제거한다.
  dropdown.querySelectorAll('.md-sep').forEach((sep) => {
    const prev = sep.previousElementSibling;
    if (!prev || prev.classList.contains('md-sep')) sep.remove();
  });
  const last = dropdown.lastElementChild;
  if (last?.classList.contains('md-sep')) last.remove();
}
if (chromeMode === 'embed') pruneEmbedChrome();

if (chromeMode === 'embed') {
  // 문서 로드 전에는 InputHandler가 없어 shortcut-map 경로가 저장·인쇄 단축키를
  // 삼키지 못하고 브라우저 저장/인쇄 대화상자로 빠진다. 초기화를 기다리지 않는
  // 모듈 최상위 등록이라 WASM 로딩 중에도 새지 않고, capture 단계라 다이얼로그
  // 등의 stopPropagation보다 먼저 돌며, preventDefault만 한다 — 대상 커맨드는
  // embed에서 미등록이고, InputHandler 활성 시의 중복 preventDefault는 무해하다.
  document.addEventListener('keydown', (e) => {
    if (isEmbedSwallowedFileShortcut(e)) e.preventDefault();
  }, true);
}

// EasyOffice: 편집기에 포커스가 있어도 셸의 Ctrl+S·O·N·K·W를 쓸 수 있도록, 셸이 `?shellKeys=`로 알려 준
// 조합만 embed 포트로 전달한다. 전달하는 것은 조합 문자열뿐이며 일반 입력은 건드리지 않는다.
const shellShortcutRelay = createShellShortcutRelay();
if (chromeMode === 'embed') {
  const shellKeys = parseShellKeys(window.location.search);
  if (shellKeys.length > 0) {
    document.addEventListener('keydown', (e) => {
      const combo = matchShellShortcut(e, shellKeys);
      if (combo === null || !shellShortcutRelay.hasListener()) return;
      e.preventDefault();
      e.stopPropagation();
      shellShortcutRelay.forward(combo);
    }, true);
  }
}

// EasyOffice: the 파일 menu is made of shell commands; they travel over the embed port by name only.
const shellCommandRelay = createShellCommandRelay();
if (chromeMode === 'embed') {
  const menuBar = document.getElementById('menu-bar');
  if (menuBar) installShellMenu(menuBar, shellCommandRelay);
}

// EasyOffice (host-fonts-v1): the machine's fonts come from the shell over the embed port (PD-128).
const shellFontBridge = createShellFontBridge();

// 상태 바 요소
const sbMessage = () => document.getElementById('sb-message')!;
const sbPage = () => document.getElementById('sb-page')!;
const sbSection = () => document.getElementById('sb-section')!;
const sbZoomVal = () => document.getElementById('sb-zoom-val')!;
function waitForNextPaint(): Promise<void> {
  return new Promise((resolve) => {
    let done = false;
    const finish = () => {
      if (done) return;
      done = true;
      resolve();
    };
    window.setTimeout(finish, 50);
    requestAnimationFrame(() => requestAnimationFrame(finish));
  });
}

async function updateLoadProgress(percent: number, label: string): Promise<void> {
  const safePercent = Math.max(0, Math.min(100, Math.round(percent)));
  sbMessage().textContent = `파일 로딩 ${safePercent}% - ${label}`;
  await waitForNextPaint();
}

async function initialize(): Promise<void> {
  const msg = sbMessage();
  try {
    msg.textContent = extensionViewerSettings.disableExternalWebFonts
      ? '로컬 폰트 준비 중...'
      : '웹폰트 로딩 중...';
    await loadWebFonts([], undefined, extensionViewerSettings);  // CSS @font-face 등록 + CRITICAL 폰트만 로드
    msg.textContent = 'WASM 로딩 중...';
    await wasm.initialize();
    // EasyOffice: Canvas2D only (CanvasKit is not bundled).
    const renderBackendRequest = { backend: 'canvas2d' as const, source: 'default' as const };
    const canvaskitModeRequest = { mode: 'default' as const, source: 'default' as const };
    const canvaskitSurfaceRequest = { preference: 'auto' as const, requested: 'auto' as const };
    const renderProfile = 'screen' as const;
    rendererRuntimeRequest = {
      backend: renderBackendRequest,
      canvaskitMode: canvaskitModeRequest,
      canvaskitSurface: canvaskitSurfaceRequest,
      renderProfile,
    };
    renderBackendFallbackReason = null;
    rendererSession = new RendererSession(
      renderBackendRequest,
      canvaskitModeRequest,
      canvaskitSurfaceRequest,
      renderProfile,
      async () => {
        throw new Error('CanvasKit is not available in the EasyOffice build');
      },
    );
    msg.textContent = 'HWP 파일을 선택해주세요.';

    const container = document.getElementById('scroll-container')!;
    canvasView = new CanvasView(
      container,
      wasm,
      eventBus,
      rendererSession,
    );

    // [#3313] 외부 연결 그림(HWP3 pic_type=0)의 비동기 주입이 첫 렌더 이후에 끝나면
    // 화면이 이전 프레임(그림 없는 상태)에 머무른다. 주입 완료 시 뷰 문서를 다시
    // 로드해 페이지 트리를 재구성한다 — dirty 마킹 없는 뷰 전용 갱신.
    wasm.onExternalImagesInjected = () => {
      void canvasView?.loadDocument();
    };

    // 눈금자 초기화
    ruler = new Ruler(
      document.getElementById('h-ruler') as HTMLCanvasElement,
      document.getElementById('v-ruler') as HTMLCanvasElement,
      container,
      eventBus,
      wasm,
      canvasView.getVirtualScroll(),
      canvasView.getViewportManager(),
    );

    inputHandler = new InputHandler(
      container, wasm, eventBus,
      canvasView.getVirtualScroll(),
      canvasView.getViewportManager(),
    );
    inputHandler.setEditMode(editMode);
    documentAgent?.dispose();
    documentAgent = new DocumentAgentController({
      wasm,
      input: inputHandler,
      eventBus,
      isDirty: () => documentState.isDirty(),
      render: () => canvasView!.refreshDocumentAgentMutation(),
    });

    // 눈금자 핀 드래그 커밋 — 종류(문단 서식/쪽 여백)에 따라 InputHandler 호출은 다르지만
    // 진입점은 하나다. 콜백 두 개로 나뉘어 있었을 때 한쪽만 executeOperation 커밋 경로를
    // 타고 다른 쪽은 wasm을 직접 호출하는 어긋남이 실제로 발생했었다 — 쪽 여백 핀 드래그가
    // 모델은 바꿨지만 CanvasView를 재플로우시키지 못한 사례. 여기서 그 어긋남을 구조적으로
    // 막는다: 두 갈래 모두 이 한 함수 안에서, 같은 InputHandler 커밋 원리(executeOperation)
    // 를 거친다.
    ruler.onCommitPin = (commit) => {
      if (!inputHandler) return;
      if (commit.kind === 'paraProps') {
        inputHandler.applyParaPropsAtCursor(commit.props);
        return;
      }
      inputHandler.executeOperation({
        kind: 'snapshot',
        operationType: 'pageMargin',
        operation: (wasm) => {
          wasm.setPageMargin(commit.pageIdx, commit.marginKind, commit.hwpunit);
          return inputHandler!.getCursorPosition();
        },
      });
    };

    // [#4180] 저장 시점 캐럿 스탬핑 — 셀/글상자 캐럿은 현행 캐럿 필드(list_id 를
    // 구역 인덱스로 쓰는 rhwp 관례)로 표현 불가 → 호스트 문단 시작으로 강등.
    wasm.onBeforeExport = () => {
      const p = inputHandler?.getCursorPosition();
      if (!p) return;
      wasm.setCaretPosition(
        p.sectionIndex,
        p.parentParaIndex ?? p.paragraphIndex,
        p.parentParaIndex !== undefined ? 0 : p.charOffset,
      );
    };

    toolbar = new Toolbar(document.getElementById('style-bar')!, wasm, eventBus, dispatcher);
    toolbar.setEnabled(false);

    // InputHandler에 커맨드 디스패처 및 컨텍스트 메뉴 주입
    inputHandler.setDispatcher(dispatcher);
    inputHandler.setContextMenu(new ContextMenu(dispatcher, registry));
    inputHandler.setCommandPalette(new CommandPalette(registry, dispatcher));
    inputHandler.setCellSelectionRenderer(
      new CellSelectionRenderer(
        container,
        canvasView.getVirtualScroll(),
        (phase) => eventBus.emit('cell-selection-phase-changed', phase),
      ),
    );
    inputHandler.setTableObjectRenderer(
      new TableObjectRenderer(container, canvasView.getVirtualScroll()),
    );
    inputHandler.setTableResizeRenderer(
      new TableResizeRenderer(container, canvasView.getVirtualScroll()),
    );
    inputHandler.setPictureObjectRenderer(
      new TableObjectRenderer(container, canvasView.getVirtualScroll(), true),
    );

    new MenuBar(document.getElementById('menu-bar')!, eventBus, dispatcher, registry);

    if (detectPlatformKind() === 'mac') applyMacShortcutPresentation(document);
    installNonEditorContextMenuGuards(document);

    // 툴바 내 data-cmd 버튼 클릭 → 커맨드 디스패치
    document.querySelectorAll('.tb-btn[data-cmd]').forEach(btn => {
      btn.addEventListener('mousedown', (e) => {
        e.preventDefault();
        const cmd = (btn as HTMLElement).dataset.cmd;
        if (cmd) dispatcher.dispatch(cmd, { anchorEl: btn as HTMLElement });
      });
      btn.addEventListener('click', (event) => {
        if ((event as MouseEvent).detail !== 0) return;
        const cmd = (btn as HTMLElement).dataset.cmd;
        if (cmd) dispatcher.dispatch(cmd, { anchorEl: btn as HTMLElement });
      });
    });

    document.querySelectorAll('.tb-split').forEach(split => {
      const arrow = split.querySelector<HTMLButtonElement>('.tb-split-arrow');
      const menu = split.querySelector<HTMLElement>('.tb-split-menu');
      if (arrow) {
        arrow.setAttribute('aria-haspopup', 'menu');
        arrow.setAttribute('aria-expanded', 'false');
        arrow.addEventListener('click', (e) => {
          e.preventDefault();
          e.stopPropagation();
          closeToolbarSplitMenus(document, split);
          const open = !split.classList.contains('open');
          setToolbarSplitOpen(split, open, {
            focus: open && (e as MouseEvent).detail === 0 ? 'first' : undefined,
          });
        });
        arrow.addEventListener('keydown', (event) => {
          if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
          event.preventDefault();
          closeToolbarSplitMenus(document, split);
          setToolbarSplitOpen(split, true, {
            focus: event.key === 'ArrowDown' ? 'first' : 'last',
          });
        });
      }
      split.querySelectorAll('.tb-split-item[data-cmd]').forEach(item => {
        item.addEventListener('click', (e) => {
          e.preventDefault();
          setToolbarSplitOpen(split, false, { returnFocus: (e as MouseEvent).detail === 0 });
          const cmd = (item as HTMLElement).dataset.cmd;
          if (cmd) dispatcher.dispatch(cmd, { anchorEl: item as HTMLElement });
        });
      });
      menu?.addEventListener('keydown', (event) => {
        const target = event.target;
        if (!(target instanceof Element)) return;
        if (event.key === 'Escape') {
          event.preventDefault();
          event.stopPropagation();
          setToolbarSplitOpen(split, false, { returnFocus: true });
        } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
          event.preventDefault();
          moveToolbarSplitFocus(split, target, event.key === 'ArrowDown' ? 1 : -1);
        } else if (event.key === 'Home' || event.key === 'End') {
          event.preventDefault();
          const items = toolbarSplitItems(split);
          (event.key === 'Home' ? items[0] : items.at(-1))?.focus({ preventScroll: true });
        }
      });
    });
    // 외부 클릭 시 스플릿 메뉴 닫기
    document.addEventListener('mousedown', () => {
      closeToolbarSplitMenus(document);
    });

    // #780: 도구 모음/서식 도구 모음 영역 mousedown 시 focus 이동 방지
    // — 편집 영역의 텍스트 선택(cursor.anchor)이 보존되어야 서식 적용이 동작함
    for (const id of ['icon-toolbar', 'style-bar']) {
      const el = document.getElementById(id);
      if (el) el.addEventListener('mousedown', (e) => {
        if ((e.target as HTMLElement).tagName !== 'INPUT' && (e.target as HTMLElement).tagName !== 'SELECT') {
          e.preventDefault();
        }
      });
    }

    setupFileInput();
    setupZoomControls();
    setupEventListeners();
    setupModalFocusRestore();
    setupGlobalShortcuts();
    // EasyOffice: no startup document. The host loads or creates the document over the embed protocol.
    rendererInitialized = true;
  } catch (error) {
    rendererInitializationError = error instanceof Error ? error.message : String(error);
    msg.textContent = `WASM 초기화 실패: ${error}`;
    console.error('[main] WASM 초기화 실패:', error);
  }
}

/** 마지막 모달 종료 뒤 활성 편집기의 키보드 진입점을 textarea로 되돌린다 (#3414). */
function setupModalFocusRestore(): void {
  document.addEventListener(MODAL_DIALOG_CLOSED_EVENT, () => {
    if (inputHandler?.isActive()) inputHandler.focus();
  });
}

/** 포커스 주인과 무관하게 문서를 움직여야 하는 키 — 편집기 경로로 넘긴다. */
const DOCUMENT_NAVIGATION_KEYS = new Set(['PageUp', 'PageDown', 'Home', 'End']);
const GLOBAL_VIEW_SHORTCUTS = new Set([
  'view:zoom-in',
  'view:zoom-out',
  'view:zoom-100',
  'view:toolbox-basic',
]);

/** 문서 이동 키(PgUp/PgDn·Home/End)를 처리했으면 true. */
function handleDocumentNavigationShortcut(e: KeyboardEvent): boolean {
  if (!DOCUMENT_NAVIGATION_KEYS.has(e.key) || e.altKey) return false;
  if (inputHandler?.isActive()) {
    inputHandler.handleDocumentNavigationKey(e);
    return true;
  }
  if (e.key === 'PageUp' || e.key === 'PageDown') {
    // 문서는 떠 있지만 편집기가 활성이 아닌 보기 상태 — 화면만 옮긴다.
    e.preventDefault();
    canvasView?.scrollByPage(e.key === 'PageUp' ? -1 : 1);
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
      () => { inputHandler?.focus(); },
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
  if (inputHandler?.isActive()) {
    handleActiveEditorFallbackShortcut(e);
    return;
  }
  handleDocumentlessShortcut(e);
}

/**
 * 전역 단축키 핸들러 — InputHandler.active 여부와 무관하게 동작해야 하는 단축키.
 * 예: 문서 미로드 상태에서도 Alt+N(새 문서), Ctrl+O(열기) 등.
 */
function setupGlobalShortcuts(): void {
  document.addEventListener('keydown', onGlobalKeydown, false);
}

function setupFileInput(): void {
  const fileInput = document.getElementById('file-input') as HTMLInputElement;

  fileInput.addEventListener('change', async (e) => {
    const input = e.target as HTMLInputElement;
    const skipUnsavedGuard = input.dataset.skipUnsavedGuard === 'true';
    delete input.dataset.skipUnsavedGuard;
    const file = input.files?.[0];
    if (!file) return;
    if (!isSupportedDocumentFileName(file.name)) {
      alert('HWP/HWPX/HML 파일만 지원합니다.');
      fileInput.value = '';
      return;
    }
    await loadFile(file, { skipUnsavedGuard });
    fileInput.value = '';
  });

  // 문서 전체에서 브라우저 기본 드롭 동작 방지 (파일 열기/다운로드 방지)
  document.addEventListener('dragover', (e) => e.preventDefault());
  document.addEventListener('drop', (e) => e.preventDefault());

  // 드래그 앤 드롭 지원 (scroll-container 영역)
  const container = document.getElementById('scroll-container')!;
  container.addEventListener('dragover', (e) => {
    e.preventDefault();
    container.classList.add('drag-over');
  });
  container.addEventListener('dragleave', () => {
    container.classList.remove('drag-over');
  });
  container.addEventListener('drop', async (e) => {
    e.preventDefault();
    container.classList.remove('drag-over');
    const file = e.dataTransfer?.files[0];
    if (!file) return;
    const dropName = file.name.toLowerCase();
    const imageExts = ['.png', '.jpg', '.jpeg', '.gif', '.bmp', '.webp'];
    const isImage = imageExts.some(ext => dropName.endsWith(ext));
    const isDoc = isSupportedDocumentFileName(dropName);
    // embed 프로파일: 문서 드롭은 호스트가 감지할 수 없는 문서 교체 경로이므로 무시한다.
    // 이미지 드롭은 수명주기가 아니라 편집 기능이라 그대로 둔다.
    if (chromeMode === 'embed' && isDoc) return;
    if (!isImage && !isDoc) {
      alert('HWP/HWPX/HML 파일 또는 이미지 파일만 지원합니다.');
      return;
    }

    if (isImage) {
      // [#1439] 이미지 드롭은 로컬 파일을 읽어 편집 중인 문서에 끼워 넣는 편집 동작이므로
      // 명시 동의를 유지한다. 문서 드롭은 열기 동작이라 확인 없이 바로 연다.
      if (!await showDropConfirmDialog(file.name)) return;
      if (!inputHandler || wasm.pageCount === 0) return;
      const data = new Uint8Array(await file.arrayBuffer());
      const ext = file.name.split('.').pop()?.toLowerCase() || 'png';
      const img = new Image();
      const url = URL.createObjectURL(file);
      try {
        img.src = url;
        await img.decode();
        const result = inputHandler.insertDroppedImageAtClientPoint(
          data,
          ext,
          img.naturalWidth,
          img.naturalHeight,
          file.name,
          e.clientX,
          e.clientY,
        );
        if (!result.ok) {
          showToast({
            message: `그림 삽입에 실패했습니다.\n${result.error ?? '삽입 위치 또는 이미지 정보를 확인할 수 없습니다.'}`,
            durationMs: 6000,
          });
        }
      } catch {
        console.warn('[drop] 이미지 디코딩 실패:', file.name);
        showToast({
          message: '그림을 삽입할 수 없습니다.\n브라우저가 이 이미지 파일을 읽지 못했습니다.',
          durationMs: 6000,
        });
      } finally {
        URL.revokeObjectURL(url);
      }
      return;
    }

    // HWP/HWPX/HML — 드롭 열기는 확인 대화상자 없이 바로 연다. 드롭 자체가 명시적인
    // 사용자 제스처이고, 파일을 열 때마다 확인을 받으면 열기 흐름이 끊긴다.
    // Finder/Explorer drop에서는 File System Access handle을 capture하지
    // 않는다. macOS Chromium에서 encrypted HWPX drag/drop 시 해당 IPC가 renderer를 종료시키는
    // 사례가 있어, 열기에 충분한 File bytes만 사용한다. 저장은 이후 save-as 경로로 진행한다.
    await loadFile(file);
  });
}

function setupZoomControls(): void {
  if (!canvasView) return;
  const vm = canvasView.getViewportManager();
  const zoomIn = document.getElementById('sb-zoom-in') as HTMLButtonElement;
  const zoomOut = document.getElementById('sb-zoom-out') as HTMLButtonElement;
  const zoomRange = document.getElementById('sb-zoom-range') as HTMLInputElement;
  const platform = detectPlatformKind();

  zoomIn.title = zoomPercentShortcutTitle('zoomIn', 'Ctrl++', platform);
  zoomOut.title = zoomPercentShortcutTitle('zoomOut', 'Ctrl+-', platform);
  zoomIn.addEventListener('click', () => {
    dispatcher.dispatch('view:zoom-in');
  });
  zoomOut.addEventListener('click', () => {
    dispatcher.dispatch('view:zoom-out');
  });
  zoomRange.addEventListener('input', () => {
    const percent = zoomSliderPositionToPercent(Number(zoomRange.value));
    zoomRange.value = String(percentToZoomSliderPosition(percent));
    zoomRange.setAttribute('aria-valuetext', `${percent}%`);
    vm.setZoom(percent / 100);
  });
  zoomRange.addEventListener('keydown', (event) => {
    const current = Math.round(vm.getZoom() * 100);
    let next: number | null = null;
    switch (event.key) {
      case 'ArrowLeft':
      case 'ArrowDown':
        next = current - 1;
        break;
      case 'ArrowRight':
      case 'ArrowUp':
        next = current + 1;
        break;
      case 'PageDown':
        next = current - 10;
        break;
      case 'PageUp':
        next = current + 10;
        break;
      case 'Home':
        next = 10;
        break;
      case 'End':
        next = 500;
        break;
      default:
        return;
    }
    event.preventDefault();
    vm.setZoom(next / 100);
  });

  // 폭 맞춤·쪽 맞춤은 메뉴/단축키와 같은 커맨드를 탄다 — 계산과 저장 자리가 하나여야 한다.
  document.getElementById('sb-zoom-fit-width')!.addEventListener('click', () => {
    dispatcher.dispatch('view:zoom-fit-width');
  });
  document.getElementById('sb-zoom-fit')!.addEventListener('click', () => {
    dispatcher.dispatch('view:zoom-fit-page');
  });

  // 참조 앱의 상황선처럼 돋보기와 배율 표시 전체가 하나의 대화상자 진입점이다.
  document.getElementById('sb-zoom-display')!.addEventListener('click', () => {
    dispatcher.dispatch('view:zoom-dialog');
  });
}

let totalSections = 1;
function headerFooterModeLabel(mode: string): string {
  return mode === 'header' ? t('ui.tbHfLabel.header') : t('ui.tbHfLabel.footer');
}

function describeError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

let lastAppliedLocalFontGeneration: string | null = null;

function syncHeaderFooterLabel(
  hfLabel: HTMLElement | null,
  hfLiveStatus: HTMLElement | null,
  state: ReturnType<typeof parseHeaderFooterModeChanged>,
): void {
  if (!hfLabel) return;
  if (state === 'none') {
    hfLabel.textContent = '';
    hfLabel.dataset.mode = '';
    hfLabel.dataset.applyTo = '';
    if (hfLiveStatus) hfLiveStatus.textContent = t('ui.hfLiveStatus.ended');
    return;
  }
  const kind = headerFooterModeLabel(state.mode);
  const target = headerFooterApplyToLabel(state.applyTo);
  hfLabel.textContent = t('ui.tbHfLabel.editing', { p1: kind, p2: target });
  hfLabel.dataset.mode = state.mode;
  hfLabel.dataset.applyTo = String(state.applyTo);
  if (hfLiveStatus) {
    hfLiveStatus.textContent = t('ui.hfLiveStatus.editing', { p1: kind, p2: target, p3: state.sectionIdx + 1 });
  }
}

function setupEventListeners(): void {
  sbPage().addEventListener('click', () => {
    dispatcher.dispatch('edit:goto');
  });

  eventBus.on('current-page-changed', (page, _total) => {
    const pageIdx = page as number;
    // 쪽 정보 한 번으로 쪽번호와 구역을 함께 갱신한다.
    let pageInfo: PageInfo | null = null;
    if (wasm.pageCount > 0) {
      try {
        pageInfo = wasm.getPageInfo(pageIdx);
      } catch { /* 무시 */ }
    }
    // 현재 쪽은 문서가 매기는 쪽번호를 쓴다 — 한글과 같은 규칙이다 (#5749).
    sbPage().textContent = formatPageIndicator({
      pageIndex: pageIdx,
      totalPages: _total as number,
      documentPageNumber: pageInfo?.pageNumber,
    });
    if (pageInfo) {
      sbSection().textContent = t('ui.sbSection.text', { p1: pageInfo.sectionIndex + 1, p2: totalSections });
    }
  });

  // 맞춤 선택은 배율 수치가 아니라 규칙이므로 그 자체를 저장한다 — 휠·가로바·수치 배율은
  // 뷰포트가 'none' 을 알려 저장된 맞춤을 푼다.
  eventBus.on('zoom-fit-mode-changed', (mode) => {
    userSettings.setZoomFitMode(normalizeZoomFitMode(mode));
  });

  eventBus.on('zoom-level-display', (zoom) => {
    const percent = Math.round((zoom as number) * 100);
    sbZoomVal().textContent = `${percent}%`;
    const range = document.getElementById('sb-zoom-range') as HTMLInputElement | null;
    if (range) {
      range.value = String(percentToZoomSliderPosition(percent));
      range.setAttribute('aria-valuetext', `${percent}%`);
    }
  });

  // 삽입/수정 모드 토글
  eventBus.on('insert-mode-changed', (insertMode) => {
    document.getElementById('sb-mode')!.textContent = (insertMode as boolean) ? t('ui.sbMode.label') : t('ui.sbMode.label.overwrite');
  });

  eventBus.on('cell-selection-phase-changed', (nextPhase) => {
    const selectionStatus = document.getElementById('sb-cell-selection')!;
    const phase = nextPhase as CellSelectionPhase | null;
    selectionStatus.hidden = phase === null;
    const nextLabel = phase === null ? '' : cellSelectionPhaseLabel(phase);
    // zoom·방향키로 동일 단계를 다시 그릴 때 live region을 반복 발화하지 않는다.
    if (selectionStatus.textContent !== nextLabel) selectionStatus.textContent = nextLabel;
  });

  eventBus.on('document-mutated', (reason) => {
    documentState.markDirty(typeof reason === 'string' ? reason : 'document-mutated');
  });

  eventBus.on('document-changed', (reason) => {
    documentState.markDirty(typeof reason === 'string' ? reason : 'document-changed');
  });

  eventBus.on('renderer-selection-changed', (payload) => {
    const diagnostics = payload as RendererSessionDiagnostics;
    renderBackendFallbackReason = diagnostics.fallbackReason;
    if (import.meta.env.DEV) {
      (window as any).__renderBackend = diagnostics.effectiveBackend;
      (window as any).__renderBackendFallbackReason = diagnostics.fallbackReason;
      (window as any).__rendererSelection = diagnostics;
    }
  });

  onHostFontsChanged(() => {
    if (!canvasView || wasm.pageCount === 0) return;
    wasm.invalidateCanvasMetricFonts();
    void canvasView.refreshFontResources().catch(error => {
      console.warn('[HostFonts] View refresh failed:', error);
    });
  });

  eventBus.on('local-fonts-changed', () => {
    if (!canvasView || wasm.pageCount === 0) return;
    const state = getLocalFontState();
    const generation = `${state.detectedAt ?? 'none'}:${state.source ?? 'none'}:${state.count}`;
    if (generation === lastAppliedLocalFontGeneration) return;
    lastAppliedLocalFontGeneration = generation;
    wasm.invalidateCanvasMetricFonts();

    // Canvas2D는 Rust layout과 문서를 다시 열지 않고 현재 보이는 view만 새 family chain으로 그린다.
    eventBus.emit('document-view-changed');
  });

  document.fonts.addEventListener('loadingdone', () => {
    if (!canvasView || wasm.pageCount === 0) return;
    wasm.invalidateCanvasMetricFonts();
    eventBus.emit('document-view-changed');
  });

  eventBus.on('document-dirty-changed', () => {
    eventBus.emit('command-state-changed');
  });

  // 필드 정보 표시
  const sbField = document.getElementById('sb-field');
  eventBus.on('field-info-changed', (info) => {
    if (!sbField) return;
    const fi = info as { fieldId: number; fieldType: string; guideName?: string } | null;
    if (fi) {
      const label = fi.guideName || `#${fi.fieldId}`;
      sbField.textContent = `[누름틀] ${label}`;
      sbField.style.display = '';
    } else {
      sbField.textContent = '';
      sbField.style.display = 'none';
    }
  });

  // 개체 선택 시 회전/대칭 버튼 그룹 표시/숨김
  const rotateGroup = document.querySelector('.tb-rotate-group') as HTMLElement | null;
  let noteToolbarActive = false;
  let headerFooterToolbarActive = false;
  let pictureObjectSelected = false;
  const syncRotateGroup = (): void => {
    if (rotateGroup) {
      rotateGroup.hidden = !pictureObjectSelected || noteToolbarActive || headerFooterToolbarActive;
    }
  };
  if (rotateGroup) {
    eventBus.on('picture-object-selection-changed', (selected) => {
      pictureObjectSelected = selected as boolean;
      syncRotateGroup();
    });
  }

  // 머리말/꼬리말 편집 모드 시 도구상자 전환 + 본문 dimming
  const hfGroup = document.querySelector('.tb-headerfooter-group') as HTMLElement | null;
  const hfLabel = hfGroup?.querySelector('.tb-hf-label') as HTMLElement | null;
  const noteGroup = document.querySelector('.tb-note-group') as HTMLElement | null;
  const defaultTbGroups = document.querySelectorAll('#icon-toolbar .tb-scroll-track > .tb-group:not(.tb-headerfooter-group):not(.tb-note-group):not(.tb-rotate-group), #icon-toolbar .tb-scroll-track > .tb-sep');
  const scrollContainer = document.getElementById('scroll-container');

  const hfLiveStatus = document.getElementById('hf-edit-status-live');
  eventBus.on('headerFooterModeChanged', (payload) => {
    const state = parseHeaderFooterModeChanged(payload);
    const isActive = state !== 'none';
    headerFooterToolbarActive = isActive;
    iconToolbarScroller?.resetToStart();
    // 도구상자 전환
    if (hfGroup) {
      hfGroup.hidden = !isActive;
    }
    syncHeaderFooterLabel(hfLabel, hfLiveStatus, state);
    defaultTbGroups.forEach((el) => {
      (el as HTMLElement).hidden = isActive;
    });
    syncRotateGroup();
    // 서식 도구 모음은 머리말/꼬리말 편집 시에도 유지 (문단/글자 모양 설정 필요)
    // 본문 dimming
    scrollContainer?.classList.toggle('hf-editing', isActive);
  });

  eventBus.on('footnoteModeChanged', (active) => {
    const isActive = active as boolean;
    noteToolbarActive = isActive;
    iconToolbarScroller?.resetToStart();
    if (noteGroup) {
      noteGroup.hidden = !isActive;
    }
    defaultTbGroups.forEach((el) => {
      (el as HTMLElement).hidden = isActive;
    });
    syncRotateGroup();
  });
}

/**
 * 저장된 쪽 맞춤/폭 맞춤을 새로 연 문서에 되돌린다.
 *
 * 저장값은 배율 수치가 아니라 맞춤 규칙이라, 쪽 크기가 다른 문서에서도 그 문서의 쪽으로
 * 다시 계산한다. 수치 배율('none')이면 지금 배율을 건드리지 않는다.
 *
 * 되돌릴 맞춤은 문서를 열기 **전에** 읽어 인자로 받는다 — 좁은 창의 자동 폭 맞춤처럼
 * 로드 중에 배율을 정하는 경로가 저장값을 먼저 'none' 으로 지워 버리기 때문이다.
 */
function applySavedZoomFitMode(mode: ZoomFitMode): void {
  if (mode === 'none') return;
  const vm = canvasView?.getViewportManager();
  const container = document.getElementById('scroll-container');
  if (!vm || !container || wasm.pageCount === 0) return;
  try {
    // getPageInfo 의 width/height 는 이미 px 단위 (96dpi 기준)
    const pageInfo = wasm.getPageInfo(0);
    const zoom = resolveZoomFitZoom(mode, {
      containerWidth: container.clientWidth,
      containerHeight: container.clientHeight,
      pageWidth: pageInfo.width,
      pageHeight: pageInfo.height,
      arrangement: userSettings.getViewSettings().pageArrangement,
    });
    if (zoom !== null) vm.setZoom(zoom, CENTER_ZOOM_ANCHOR, mode);
  } catch (error) {
    console.warn('[main] 저장된 맞춤 배율 복원 실패:', error);
  }
}

/** 문서 초기화 공통 시퀀스 (loadFile, createNewDocument 양쪽에서 사용) */
function applySavedTextMarkSettings(): void {
  const view = userSettings.getViewSettings();
  wasm.setShowControlCodes(view.showControlCodes);
  wasm.setShowParagraphMarks(view.showParagraphMarks);
  syncTextMarkMenu(view.showControlCodes, view.showParagraphMarks);
  // #2204: 짤림보기(잘림 보기) 저장 설정 복원. clipView=켜짐 => clip 미적용(clipEnabled=false).
  const clipEnabled = !view.clipView;
  wasm.setClipEnabled(clipEnabled);
  syncClipMenu(clipEnabled);
}

async function initializeDocument(
  docInfo: DocumentInfo,
  displayName: string,
  options: { suppressDialogs?: boolean } = {},
): Promise<void> {
  const msg = sbMessage();
  try {
    setDocumentFontSubstitutions(docInfo.fontSubstitutions);
    lastAppliedLocalFontGeneration = null;
    console.log('[initDoc] 1. 폰트 로딩 시작');
    await updateLoadProgress(55, '폰트 준비 중...');
    if (docInfo.fontsUsed?.length) {
      await loadWebFonts(docInfo.fontsUsed, (loaded, total) => {
        const fontPercent = total > 0 ? 55 + Math.round((loaded / total) * 20) : 65;
        msg.textContent = `파일 로딩 ${fontPercent}% - 폰트 로딩 중... (${loaded}/${total})`;
      }, extensionViewerSettings);
    }
    console.log('[initDoc] 2. 폰트 로딩 완료');
    // 저장 snapshot은 권한 prompt 없이 읽을 수 있다. Canvas2D 첫 paint의 family 해소가
    // 이미 승인된 exact local face를 놓치지 않도록 문서 view보다 먼저 준비한다 (#4739).
    await loadStoredLocalFonts();
    await updateLoadProgress(75, '문서 상태 적용 중...');
    totalSections = docInfo.sectionCount ?? 1;
    sbSection().textContent = t('ui.sbSection.text', { p1: 1, p2: totalSections });
    applySavedTextMarkSettings();
    console.log('[initDoc] 3. inputHandler deactivate');
    inputHandler?.deactivate();
    console.log('[initDoc] 4. canvasView loadDocument');
    await updateLoadProgress(82, '페이지 렌더 준비 중...');
    const savedZoomFitMode = userSettings.getViewSettings().zoomFitMode;
    await canvasView?.loadDocument();
    // 쪽 크기를 알 수 있는 첫 시점이다 — 저장된 맞춤은 이 문서의 쪽으로 다시 계산한다.
    applySavedZoomFitMode(savedZoomFitMode);
    console.log('[initDoc] 5. toolbar setEnabled');
    await updateLoadProgress(90, '도구 모음 준비 중...');
    toolbar?.setEnabled(true);
    console.log('[initDoc] 6. toolbar initFontDropdown + initStyleDropdown');
    toolbar?.initFontDropdown(docInfo.fontsUsed);
    toolbar?.initStyleDropdown();
    console.log('[initDoc] 7. 사전 검증 및 로컬 글꼴 확인');
    await updateLoadProgress(94, '문서 검증 및 글꼴 확인 중...');

    // #177: HWPX 비표준 lineseg 감지 (진단 로그).
    // #2527: 자동 보정(reflowLinesegs)이 빈-lineseg 문서에서 글리프 좌표를 붕괴시켜
    // 글자가 대량으로 겹치므로, 모달을 띄우지 않고 항상 '그대로 보기'로 연다.
    // reflow 근본 수정 후 모달/자동 보정 재도입을 검토한다.
    try {
      if (wasm.getSourceFormat() === 'hwpx') {
        const report = wasm.getValidationWarnings();
        if (report.count > 0) {
          console.log(`[validation] ${report.count} warnings — 그대로 보기 (#2527)`, report.summary);
        }
      } else if (wasm.getSourceFormat() === 'hml') {
        const metadata = wasm.getHmlOpenMetadata();
        if (metadata) showHmlImportWarning(metadata);
      }
    } catch (e) {
      console.warn('[validation] 감지 실패 (치명적이지 않음):', e);
    }

    if (!options.suppressDialogs) {
      await promptLocalFontsIfNeeded(docInfo, displayName);
    }

    // 로컬 글꼴 감지 결과가 뷰를 갱신한 뒤에 캐럿을 연결해야 입력 포커스가 재설정과 경합하지 않는다.
    console.log('[initDoc] 8. inputHandler activateWithCaretPosition');
    await updateLoadProgress(96, '편집 상태 초기화 중...');
    inputHandler?.activateWithCaretPosition();
    // 최종 단계 뒤에는 비동기 작업이 없으므로 100% progress paint를 기다리지 않는다.
    msg.textContent = displayName;
    console.log('[initDoc] 9. 완료');

    // #2527: 자동 보정을 하지 않으므로 로드 직후 문서는 항상 clean.
    documentState.markClean('document-initialized');
  } catch (error) {
    console.error('[initDoc] 오류:', error);
    if (window.innerWidth < 768) alert(`초기화 오류: ${error}`);
  }
}

async function promptLocalFontsIfNeeded(docInfo: DocumentInfo, displayName: string): Promise<void> {
  if (!docInfo.fontsUsed?.length) return;

  // 문서를 열 때마다 로컬 글꼴 감지 안내 모달을 띄우면 열람 흐름이 끊긴다. 저장된 감지 결과가
  // 있으면 그대로 재사용하고, 없으면 대체 글꼴로 조용히 표시한다. 수동 감지는 옵션 대화상자의
  // '로컬 글꼴 감지하기'로 계속 실행할 수 있다.
  try {
    if (!getLocalFontState().loaded) await loadStoredLocalFonts();
    const report = analyzeDocumentFonts(docInfo.fontsUsed);
    if (!report.shouldPromptLocalAccess) return;
    console.log(
      `[local-fonts] 감지 안내 모달 생략 — 대체 글꼴로 표시 (확인 필요 ${report.summary.needsLocalCheck}개, 문서 ${displayName})`,
    );
  } catch (error) {
    console.warn('[local-fonts] 저장된 감지 결과 로드 실패 (치명적이지 않음):', error);
  }
}

/**
 * 사용자가 암호 입력 대화상자에서 취소한 경우다. 일반 파싱 실패와 달리 오류 토스트나
 * 최근 문서·자동저장 변경을 만들지 않는다 (#3474).
 */
class DocumentOpenCancelledError extends Error {
  constructor() {
    super('문서 열기가 취소되었습니다.');
    this.name = 'DocumentOpenCancelledError';
  }
}

const PASSWORD_REQUIRED_MESSAGE = '비밀번호가 필요한 암호 문서';
const PASSWORD_REJECTED_MESSAGE = '비밀번호가 일치하지 않거나 암호화 데이터가 손상되었습니다';

function isDocumentOpenCancelled(error: unknown): error is DocumentOpenCancelledError {
  return error instanceof DocumentOpenCancelledError;
}

function isPasswordRequiredError(error: unknown): boolean {
  return String(error).includes(PASSWORD_REQUIRED_MESSAGE);
}

function isPasswordRejectedError(error: unknown): boolean {
  return String(error).includes(PASSWORD_REJECTED_MESSAGE);
}

function passwordOpenFailure(error: unknown): Error {
  const message = String(error);
  if (message.includes('지원하지 않는 암호화 방식')) {
    return new Error('지원하지 않는 암호화 방식의 문서입니다. 지원되는 HWP3/HWP5 암호 문서만 열 수 있습니다.');
  }
  if (message.includes('DRM')) {
    return new Error('DRM으로 보호된 문서는 지원하지 않습니다.');
  }
  // 입력값이 포함될 수 있는 원본 오류는 사용자 화면이나 콘솔에 전달하지 않는다. 현재
  // 암호화 포맷은 오입력과 암호문 훼손을 암호학적으로 판별할 수 없으므로 안전한 일반
  // 안내로 축약한다.
  return new Error('암호화된 문서를 열 수 없습니다. 문서가 손상되었는지 확인하세요.');
}

/**
 * 일반 열기를 먼저 시도하고, 지원되는 HWP3/HWP5 암호 문서가 감지된 경우에만 암호
 * 입력 UI로 전환한다. 암호 문자열은 이 함수의 단일 시도 범위를 벗어나 보관하지 않는다.
 */
async function openPasswordProtectedDocument(data: Uint8Array, fileName: string): Promise<void> {
  let retryMessage: string | undefined;

  while (true) {
    let password = await showHwpPasswordDialog(fileName, retryMessage);
    if (password === null) throw new DocumentOpenCancelledError();

    try {
      wasm.loadDocumentWithPassword(data, password, fileName);
      return;
    } catch (error) {
      // CFB 암호문은 인증 태그가 없으므로 오입력과 암호화 데이터 손상을 완전히 구분할 수
      // 없다. 두 경우만 재입력 상태로 안내하고, 지원하지 않는 암호화/DRM 등은 원래의
      // 명시적 거부 오류를 유지한다.
      if (isPasswordRejectedError(error)) {
        retryMessage = '암호가 일치하지 않거나 문서가 손상되었습니다. 다시 입력하세요.';
        continue;
      }
      throw passwordOpenFailure(error);
    } finally {
      // JavaScript 문자열을 확실히 zeroize할 수는 없지만, 대화상자 DOM과 이 지역 참조는
      // 시도 직후 해제한다. 최근 문서·URL·저장소·문서 메타데이터에는 전달하지 않는다.
      password = '';
    }
  }
}

async function loadDocumentForOpen(data: Uint8Array, fileName: string): Promise<DocumentInfo> {
  try {
    return wasm.loadDocument(data, fileName);
  } catch (error) {
    if (!isPasswordRequiredError(error)) throw error;
    await openPasswordProtectedDocument(data, fileName);
    return wasm.getDocumentInfo();
  }
}

/**
 * 문서 열기가 실패하면 빈 쪽 상태로 남는다. WASM이 아직 이전 문서를 쥐고 있으면 그 뷰를
 * 되살려, 열기 실패가 이미 열어 둔 문서를 잃는 일로 번지지 않게 한다.
 */
function restoreViewAfterFailedOpen(): void {
  if (!canvasView || wasm.pageCount === 0) return;
  void canvasView.loadDocument().catch((error) => {
    console.warn('[main] 열기 실패 후 이전 문서 뷰 복구 실패:', error);
  });
}

function showLoadErrorUnlessCancelled(error: unknown): void {
  if (isDocumentOpenCancelled(error)) {
    sbMessage().textContent = '문서 열기를 취소했습니다.';
    restoreViewAfterFailedOpen();
    return;
  }
  showLoadError(error);
}

async function loadFile(
  file: File,
  options: { skipUnsavedGuard?: boolean; fileHandle?: FileSystemFileHandleLike | null } = {},
): Promise<boolean> {
  try {
    if (!await canReplaceCurrentDocument(options.skipUnsavedGuard)) return false;
    // 대기 커서는 저장 여부 확인(모달) 다음부터 — 사용자가 답해야 하는 동안은 평소 커서다.
    return await withBusyCursor(document.documentElement, async () => {
      const startTime = performance.now();
      await updateLoadProgress(0, '파일 읽는 중...');
      const data = new Uint8Array(await file.arrayBuffer());
      await updateLoadProgress(15, '파일 읽기 완료');
      await loadBytes(data, file.name, options.fileHandle ?? null, startTime, { dataReadProgressShown: true });
      return true;
    });
  } catch (error) {
    showLoadErrorUnlessCancelled(error);
    return false;
  }
}

function prepareCanvasRendererDocument(): void {
  canvasView?.prepareDocumentLoad();
}

async function loadBytes(
  data: Uint8Array,
  fileName: string,
  fileHandle: typeof wasm.currentFileHandle,
  startTime = performance.now(),
  options: { dataReadProgressShown?: boolean; suppressDialogs?: boolean } = {},
): Promise<void> {
  // 바이트로 여는 모든 경로(파일 열기 · ?url= · 자동저장 복구 · 호스트 API)의 공통 깔때기다.
  // 파싱·쪽 계산 동안 빈 화면만 보이므로 여기서 대기 커서를 든다.
  await withBusyCursor(document.documentElement, async () => {
    // 파싱 전에 먼저 빈 쪽 상태로 만든다 — 이전 문서를 붙잡고 있다가 한 번에 갈아치우면
    // 화면이 튀어 보인다. 파싱이 실패하면 아래 catch가 이전 문서 뷰를 되살린다.
    canvasView?.showBlankPage();
    if (!options.dataReadProgressShown) {
      await updateLoadProgress(0, '문서 데이터 준비 중...');
    }
    await updateLoadProgress(25, '문서 파싱 및 쪽 계산 중...');
    const docInfo = await loadDocumentForOpen(data, fileName);
    prepareCanvasRendererDocument();
    // [#7194] 같은 사실을 앱 안쪽에도 알린다. 종전에는 문서 교체 신호가
    // `open-document-bytes` 이벤트뿐이었는데, 그 이벤트를 거치는 열기 경로는 여섯 중
    // 하나(`open-document-bytes` 핸들러)뿐이다 — 드롭·파일 input·`?url=`·자동저장 복구·
    // 호스트 API 는 이 깔때기를 직접 부른다. 그래서 표 resize 런타임 캐시가 이전 문서의
    // 칸 좌표를 그대로 들고 살아남았다(`cachedTableRef` 는 `{sec, ppi, ci}` 라 문서가
    // 바뀌어도 신선도 검사를 통과한다 — 3184241).
    eventBus.emit('document-swapped');
    await updateLoadProgress(45, '자동 저장 준비 중...');
    forgetConvertedHmlSaveHandle(fileHandle);
    wasm.currentFileHandle = fileHandle;

    await updateLoadProgress(50, '문서 초기화 중...');
    const elapsed = performance.now() - startTime;
    await initializeDocument(docInfo, t('ui.sbMessage.filePagesTimed', { p1: fileName, p2: docInfo.pageCount, p3: elapsed.toFixed(1) }), {
      suppressDialogs: options.suppressDialogs,
    });
  });
}

async function createNewDocument(): Promise<void> {
  const msg = sbMessage();
  try {
    await withBusyCursor(document.documentElement, async () => {
      msg.textContent = '새 문서 생성 중...';
      const docInfo = wasm.createNewDocument();
      prepareCanvasRendererDocument();
      await initializeDocument(docInfo, t('ui.sbMessage.newDocPages', { p1: docInfo.pageCount }));
    });
  } catch (error) {
    msg.textContent = `새 문서 생성 실패: ${error}`;
    console.error('[main] 새 문서 생성 실패:', error);
  }
}

async function canReplaceCurrentDocument(skipUnsavedGuard?: boolean): Promise<boolean> {
  return skipUnsavedGuard === true || await confirmSaveBeforeReplacingDocument(commandServices, {
    // embed: unsaved guard의 '저장'은 registry를 우회한 직접 호출이라 커맨드 필터로는
    // 닫히지 않고, 로컬 저장은 호스트 저장소에 반영되지 않은 채 dirty만 해제한다 —
    // 다이얼로그에서 로컬 저장 선택지를 막고, 버릴지/취소할지는 사용자가 고른다.
    allowLocalSave: chromeMode !== 'embed',
  });
}

// 커맨드에서 새 문서 생성 호출
eventBus.on('create-new-document', (payload) => {
  void (async () => {
    const options = payload as { skipUnsavedGuard?: boolean } | undefined;
    if (!await canReplaceCurrentDocument(options?.skipUnsavedGuard)) return;
    await createNewDocument();
  })();
});
eventBus.on('open-document-bytes', async (payload) => {
  const data = payload as {
    bytes: Uint8Array;
    fileName: string;
    fileHandle: typeof wasm.currentFileHandle;
    skipUnsavedGuard?: boolean;
    /** 문서 비교 등: 로드 완료를 기다리는 쪽과 짝을 맞출 때만 전달 */
    requestId?: string;
  };
  const notifyDone = (ok: boolean, error?: string) => {
    if (!data.requestId) return;
    eventBus.emit('open-document-bytes:done', { requestId: data.requestId, ok, error });
  };
  try {
    if (!await canReplaceCurrentDocument(data.skipUnsavedGuard)) {
      notifyDone(false, '문서 열기가 취소되었습니다.');
      return;
    }
    await loadBytes(data.bytes, data.fileName, data.fileHandle);
    notifyDone(true);
  } catch (error) {
    // #265: WASM 파서 에러 (예: HWP 3.0 미지원) 를 사용자에게 전파
    showLoadErrorUnlessCancelled(error);
    const msg = isDocumentOpenCancelled(error) ? '문서 열기가 취소되었습니다.' : describeError(error);
    notifyDone(false, msg);
  }
});

// 수식 더블클릭 → 수식 편집 대화상자
eventBus.on('equation-edit-request', () => {
  dispatcher.dispatch('insert:equation-edit');
});

// [#4694] 차트 더블클릭 → 차트 데이터 편집 대화상자
eventBus.on('chart-data-edit-request', () => {
  dispatcher.dispatch('insert:chart-data-edit');
});

/**
 * 파일 로드 실패 시 사용자에게 에러를 명확히 알린다 (#265).
 *
 * 상태 표시줄은 22px 한 줄로 긴 에러 메시지가 ellipsis 로 잘리므로,
 * 우상단 토스트 (긴 메시지 줄바꿈 지원 · 사용자 닫기 · action 링크) 를
 * 병행 사용한다.
 */
function showLoadError(error: unknown): void {
  const raw = String(error).replace(/^Error:\s*/, '');
  const errMsg = `파일 로드 실패: ${raw}`;
  const sb = sbMessage();
  if (sb) sb.textContent = errMsg;
  console.error('[main] 파일 로드 실패:', error);
  restoreViewAfterFailedOpen();
  showToast({
    message: errMsg,
    durationMs: 0, // 에러는 자동 페이드 없음 — 사용자가 읽고 닫기
    confirmLabel: '확인',
  });
}

const initPromise = initialize();

installEmbedRuntime({
  hostWindow: window,
  parentWindow: window.parent,
  // EasyOffice: besides agent changes, the host is told about dirty-state transitions and every
  // edit (reason 'dirty_changed' / 'edited'), so it can drive its unsaved-changes guard and autosave.
  subscribeDocumentChanged: (listener) => {
    const offs = [
      shellShortcutRelay.add(listener),
      shellCommandRelay.add(listener),
      shellFontBridge.relay.add((request) => listener({ ...request })),
      eventBus.on('document-agent-changed', listener),
      eventBus.on('document-dirty-changed', (change) => {
        listener({ schemaVersion: 1, reason: 'dirty_changed', dirty: (change as { dirty?: boolean })?.dirty === true });
      }),
      eventBus.on('document-mutated', () => listener({ schemaVersion: 1, reason: 'edited' })),
      eventBus.on('document-changed', () => listener({ schemaVersion: 1, reason: 'edited' })),
    ];
    return () => offs.forEach((off) => off());
  },
  handlers: {
    async ready() {
      await initPromise;
      return true;
    },
    async loadFile(data, fileName, skipUnsavedGuard, suppressDialogs) {
      await initPromise;
      if (!await canReplaceCurrentDocument(skipUnsavedGuard)) {
        throw new Error('문서 열기가 취소되었습니다.');
      }
      await loadBytes(data, fileName, null, undefined, { suppressDialogs });
      return { pageCount: wasm.pageCount };
    },
    async pageCount() {
      await initPromise;
      return wasm.pageCount;
    },
    // EasyOffice extension (save-protection-v1): the editor exports plain bytes, so a document that
    // was opened with a password would be saved without it. Only this flag crosses the port.
    async getSaveProtection() {
      await initPromise;
      return { passwordProtected: wasm.requiresPasswordForSave };
    },
    async clearSaveProtection() {
      await initPromise;
      wasm.requiresPasswordForSave = false;
      return { cleared: true as const };
    },
    // EasyOffice extension (host-fonts-v1): the shell lists the installed fonts and reads one on
    // request; the editor draws the document in them when it has them and keeps its bundled fonts
    // otherwise. The catalog loads in the background, so this answers at once.
    async enableHostFonts() {
      await initPromise;
      void setHostFontProvider(shellFontBridge.provider);
      return { enabled: true as const };
    },
    async hostFontsReply(params) {
      return shellFontBridge.reply(params);
    },
    // EasyOffice extension (drag-over-v1): the shell tells the editor a droppable file is over the window.
    async setDragOver(active) {
      document.getElementById('scroll-container')?.classList.toggle('drag-over', active);
      return { applied: true as const };
    },
    async getRendererDiagnostics(pageIndex) {
      await initPromise;
      const selection = canvasView?.getRendererSessionDiagnostics() ?? null;
      return {
        schemaVersion: 1 as const,
        request: rendererRuntimeRequest,
        initialized: rendererInitialized,
        initializationError: rendererInitializationError,
        effectiveBackend: selection?.effectiveBackend ?? null,
        backendFallbackReason: selection?.fallbackReason ?? renderBackendFallbackReason,
        selection,
        page: {
          index: pageIndex,
          canvaskit: canvasView?.getCanvasKitRenderDiagnostics(pageIndex) ?? null,
        },
      };
    },
    async getFontDecisionTrace(pageIndex, maxCharacters) {
      await initPromise;
      return enrichFontDecisionTrace(
        wasm.getFontDecisionTrace(pageIndex, maxCharacters),
        {
          canvasKitEvidence: record =>
            canvasView?.getCanvasKitFontDecisionEvidence(pageIndex, record) ?? null,
        },
      );
    },
    async getPageSvg(page) {
      await initPromise;
      return wasm.renderPageSvg(page);
    },
    // EasyOffice extension (print-pages-v1): the shell prints from batches of print-profile pages.
    async getPrintPages(from, count) {
      await initPromise;
      if (from === 0) commandServices.getInputHandler()?.flushDeferredPaginationIfNeeded('print');
      return renderPrintBatch(wasm, from, count);
    },
    async exportHwp() {
      await initPromise;
      return wasm.exportHwp();
    },
    async exportHwpx() {
      await initPromise;
      return wasm.exportHwpx();
    },
    async exportHml() {
      await initPromise;
      return wasm.exportHml();
    },
    async getHmlSaveState() {
      await initPromise;
      return wasm.getHmlSaveState();
    },
    async exportHwpVerify() {
      await initPromise;
      return JSON.parse(wasm.exportHwpVerify());
    },
    async notifySaved(fileName) {
      await initPromise;
      return completeHostSave(fileName);
    },
    async getDocumentState() {
      await initPromise;
      if (!documentAgent) throw new Error('Document agent is not initialized');
      return documentAgent.getDocumentState();
    },
    async getSelectionContext() {
      await initPromise;
      if (!documentAgent) throw new Error('Document agent is not initialized');
      return documentAgent.getSelectionContext();
    },
    async applyTextCommand(command) {
      await initPromise;
      if (!documentAgent) throw new Error('Document agent is not initialized');
      return documentAgent.applyTextCommand(command);
    },
    async revertTextCommand(command) {
      await initPromise;
      if (!documentAgent) throw new Error('Document agent is not initialized');
      return documentAgent.revertTextCommand(command);
    },
    async focusTarget(target) {
      await initPromise;
      if (!documentAgent) throw new Error('Document agent is not initialized');
      return documentAgent.focusTarget(target);
    },

    // ── 브리지 확장 (P4) — 자동화·플러그인·창 제어 ─────────
    // 부모가 보내는 것은 데이터뿐이다. 함수를 받는 표면(확장 커맨드 등록)은 iframe 안의
    // 플러그인만 쓸 수 있고, 그래서 RPC 에 없다.
    async automationList() { await initPromise; return automation.listCommands(); },
    async automationMenuModel() { await initPromise; return automation.getMenuModel(); },
    async automationIsEnabled(id) { await initPromise; return automation.isEnabled(id); },
    async automationExecute(id, params, options) {
      await initPromise;
      // A dialog's text field with the focus takes the shell's edit commands (never the document).
      return runEditCommand(
        id,
        document.activeElement,
        (name) => document.execCommand(name),
        () => automation.execute(id, params, options),
      );
    },
    async automationContext() { await initPromise; return automation.getContext(); },
    async createBlankDocument(skipUnsavedGuard) {
      await initPromise;
      if (!await canReplaceCurrentDocument(skipUnsavedGuard)) {
        throw new Error('문서 열기가 취소되었습니다.');
      }
      await createNewDocument();
      return { pageCount: wasm.pageCount };
    },
    // EasyOffice extension (verify-bytes-v1): parse exported bytes in a throw-away document so the
    // host can refuse to deliver a file that this build cannot read back.
    async verifyBytes(data) {
      await initPromise;
      const probe = new HwpDocument(data);
      try {
        return { ok: true, pageCount: probe.pageCount(), format: probe.getSourceFormat() };
      } finally {
        probe.free();
      }
    },
    // EasyOffice extension (shell-context-v1): one theme and a recent-documents list from the shell.
    setShellContext(raw) {
      return Promise.resolve().then(() => {
        const context = sanitizeShellContext(raw);
        if (context.theme && context.theme !== getThemeMode()) setThemeMode(context.theme);
        const panel = document.getElementById('shell-recents-panel');
        if (context.recents && panel) renderShellRecents(panel, context.recents);
        // "PDF로 내보내기" is shown only when the shell says it can do it (desktop app).
        const pdfItem = document.querySelector<HTMLElement>('[data-shell-cmd="export-pdf"]');
        if (pdfItem && context.pdfExport !== null) pdfItem.hidden = !context.pdfExport;
        return { applied: true as const };
      });
    },
    async chromeGet() { return getChromeVisibility(); },
    async chromeSet(next) { return setChromeVisibility(next as Partial<ChromeVisibility>); },
  },
});
