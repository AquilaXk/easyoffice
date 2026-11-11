import { WasmBridge } from '@/core/wasm-bridge';
import { HwpDocument } from '@wasm/rhwp.js';
import { installDocumentTitle } from '@/ui/document-title';
import type { PageInfo } from '@/core/types';
import { EventBus } from '@/core/event-bus';
import { CanvasView } from '@/view/canvas-view';
import { InputHandler } from '@/engine/input-handler';
import { Toolbar } from '@/ui/toolbar';
import { enhanceCustomSelects } from './ui/custom-select.ts';
import { initIconToolbarScroller } from '@/ui/icon-toolbar-scroller';
import { initStyleToolbarOverflow } from '@/ui/style-toolbar-overflow';
import { MenuBar } from '@/ui/menu-bar';
import { loadWebFonts } from '@/core/font-loader';
import { CommandRegistry } from '@/command/registry';
import { AutomationHost } from '@/automation/host';
import { runEditCommand } from './embed/text-field-edit.ts';
import { getChromeVisibility, setChromeVisibility } from '@/automation/chrome';
import type { ChromeVisibility } from '@/automation/types';
import { CommandDispatcher } from '@/command/dispatcher';
import type { EditorContext, CommandServices, EditorEditMode } from '@/command/types';
import { defaultShortcuts } from '@/command/shortcut-map';
import { fileCommands } from '@/command/commands/file';
import { editCommands } from '@/command/commands/edit';
import { syncToolboxMenu, viewCommands } from '@/command/commands/view';
import { formatCommands } from '@/command/commands/format';
import { insertCommands } from '@/command/commands/insert';
import { tableCommands } from '@/command/commands/table';
import { pageCommands } from '@/command/commands/page';
import { toolCommands } from '@/command/commands/tool';
import { ContextMenu } from '@/ui/context-menu';
import { CommandPalette } from '@/ui/command-palette';
import { MODAL_DIALOG_CLOSED_EVENT } from '@/ui/dialog';
import {
  EMBED_HIDDEN_EDIT_COMMAND_IDS,
  EMBED_HIDDEN_FILE_COMMAND_IDS,
  isEmbedSwallowedFileShortcut,
} from '@/ui/chrome-mode';
import { DocumentDirtyState } from '@/core/document-dirty-state';
import { initThemeSync, setThemeMode, getThemeMode } from '@/core/theme';
import {
  getLocalFontState,
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
import { percentToZoomSliderPosition } from '@/view/zoom-status-controls';
import { RendererSession, type RendererSessionDiagnostics } from '@/view/renderer-session';
import type { RenderBackendFallbackReason } from '@/view/render-backend';
import { normalizeZoomFitMode } from '@/view/zoom-fit';
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
import { safeCopyCommand, safeCutCommand } from './command/safe-cut.ts';
import type { EmbedRendererRuntimeRequestV1 } from '@/embed/rpc-router';
import { enrichFontDecisionTrace } from '@/core/font-decision-trace';
import { DocumentAgentController } from '@/document-agent/controller';
import { initI18n, t } from '@/i18n/index.ts';
import { applyMacShortcutPresentation, installNonEditorContextMenuGuards } from './embed/desktop-chrome.ts';
import { setupGlobalShortcuts } from './embed/global-shortcuts.ts';
import { setupFileInput } from './embed/file-drop.ts';
import { createOpenPipeline, type OpenViewState } from './embed/open-pipeline.ts';
import { setupZoomControls } from './ui/zoom-controls.ts';
import { wireToolbars } from './ui/toolbar-wiring.ts';
import { wireContextToolbarGroups } from './ui/context-toolbar-groups.ts';

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
// Written by the open pipeline, read by the status-bar listeners.
const viewState: OpenViewState = { totalSections: 1, lastAppliedLocalFontGeneration: null };
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
// 오려 두기는 시스템 클립보드가 받은 뒤에만 선택 영역을 지운다 (메뉴가 보낸 명령에는 사용자 제스처가 없다).
const upstreamCut = registry.get('edit:cut');
if (upstreamCut) registry.register(safeCutCommand(upstreamCut));
const upstreamCopy = registry.get('edit:copy');
if (upstreamCopy) registry.register(safeCopyCommand(upstreamCopy));
registry.registerAll(viewCommands);
registry.register(toolbarLabelsCommand);
initToolbarLabels();
registry.registerAll(formatCommands);
registry.registerAll(insertCommands);
registry.registerAll(tableCommands);
registry.register(cellSelectionEnterCommand);
// 참조 앱 표 메뉴의 Alt+Insert(왼쪽에 칸 추가하기). Alt+Delete는 표 안에서 줄/칸 지우기 대화상자를 연다.
defaultShortcuts.push([{ key: 'insert', alt: true }, 'table:insert-col-left']);
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
    const shellKeySet: ReadonlySet<string> = new Set(shellKeys);
    document.addEventListener('keydown', (e) => {
      const combo = matchShellShortcut(e, shellKeySet);
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

    enhanceCustomSelects(document);

    new MenuBar(document.getElementById('menu-bar')!, eventBus, dispatcher, registry);

    if (detectPlatformKind() === 'mac') applyMacShortcutPresentation(document);
    installNonEditorContextMenuGuards(document);

    wireToolbars(dispatcher);

    setupFileInput({ wasm, chromeMode, loadFile, getInputHandler: () => inputHandler });
    setupZoomControls(canvasView, dispatcher);
    setupEventListeners();
    setupModalFocusRestore();
    setupGlobalShortcuts({ dispatcher, getInputHandler: () => inputHandler, getCanvasView: () => canvasView });
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
      sbSection().textContent = t('ui.sbSection.text', { p1: pageInfo.sectionIndex + 1, p2: viewState.totalSections });
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
    if (generation === viewState.lastAppliedLocalFontGeneration) return;
    viewState.lastAppliedLocalFontGeneration = generation;
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

  wireContextToolbarGroups(eventBus, iconToolbarScroller);
}

// 수식 더블클릭 → 수식 편집 대화상자
eventBus.on('equation-edit-request', () => {
  dispatcher.dispatch('insert:equation-edit');
});

// [#4694] 차트 더블클릭 → 차트 데이터 편집 대화상자
eventBus.on('chart-data-edit-request', () => {
  dispatcher.dispatch('insert:chart-data-edit');
});

const { loadFile, loadBytes, createNewDocument, canReplaceCurrentDocument } = createOpenPipeline({
  wasm,
  eventBus,
  documentState,
  commandServices,
  chromeMode,
  extensionViewerSettings,
  viewState,
  getCanvasView: () => canvasView,
  getInputHandler: () => inputHandler,
  getToolbar: () => toolbar,
  sbMessage,
  sbSection,
});

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
        if (context.recents && panel) {
          renderShellRecents(panel, context.recents);
          if (detectPlatformKind() === 'mac') {
            applyMacShortcutPresentation(panel as unknown as Document);
          }
        }
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
