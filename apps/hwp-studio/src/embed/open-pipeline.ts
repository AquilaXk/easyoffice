// EasyOffice: the document open / new-document pipeline, split out of main.ts. The editor objects
// stay owned by main.ts and are read through getters, so each step sees the current instance.
import type { WasmBridge } from '@/core/wasm-bridge';
import type { EventBus } from '@/core/event-bus';
import type { DocumentDirtyState } from '@/core/document-dirty-state';
import type { DocumentInfo } from '@/core/types';
import type { CommandServices } from '@/command/types';
import type { CanvasView } from '@/view/canvas-view';
import type { InputHandler } from '@/engine/input-handler';
import type { Toolbar } from '@/ui/toolbar';
import type { FileSystemFileHandleLike } from '@/command/file-system-access';
import { confirmSaveBeforeReplacingDocument } from '@/command/commands/file';
import { forgetConvertedHmlSaveHandle } from '@/command/save-target';
import { syncClipMenu, syncTextMarkMenu } from '@/command/commands/view';
import { loadWebFonts } from '@/core/font-loader';
import { analyzeDocumentFonts } from '@/core/document-font-status';
import { setDocumentFontSubstitutions } from '@/core/font-substitution';
import { getLocalFontState, loadStoredLocalFonts } from '@/core/local-fonts';
import { userSettings } from '@/core/user-settings';
import { resolveZoomFitZoom, type ZoomFitMode } from '@/view/zoom-fit';
import { CENTER_ZOOM_ANCHOR } from '@/view/zoom-anchor';
import { withBusyCursor } from '@/view/busy-cursor';
import { showHmlImportWarning } from '@/ui/hml-import-warning';
import { showToast } from '@/ui/toast';
import { isDocumentOpenCancelled, loadDocumentForOpen } from './password-open.ts';
import { t } from '@/i18n/index.ts';

/** View state the open pipeline writes and the status-bar listeners read. */
export interface OpenViewState {
  totalSections: number;
  lastAppliedLocalFontGeneration: string | null;
}

export interface OpenPipelineDeps {
  wasm: WasmBridge;
  eventBus: EventBus;
  documentState: DocumentDirtyState;
  commandServices: CommandServices;
  chromeMode: 'embed' | 'full';
  /** EasyOffice: external web fonts are never loaded (bundled fonts only). */
  extensionViewerSettings: { readonly disableExternalWebFonts: true };
  viewState: OpenViewState;
  getCanvasView: () => CanvasView | null;
  getInputHandler: () => InputHandler | null;
  getToolbar: () => Toolbar | null;
  sbMessage: () => HTMLElement;
  sbSection: () => HTMLElement;
}

export interface OpenPipeline {
  loadFile: (
    file: File,
    options?: { skipUnsavedGuard?: boolean; fileHandle?: FileSystemFileHandleLike | null },
  ) => Promise<boolean>;
  loadBytes: (
    data: Uint8Array,
    fileName: string,
    fileHandle: WasmBridge['currentFileHandle'],
    startTime?: number,
    options?: { dataReadProgressShown?: boolean; suppressDialogs?: boolean },
  ) => Promise<void>;
  createNewDocument: () => Promise<void>;
  canReplaceCurrentDocument: (skipUnsavedGuard?: boolean) => Promise<boolean>;
}

export function createOpenPipeline(deps: OpenPipelineDeps): OpenPipeline {
  const {
    wasm, eventBus, documentState, commandServices, chromeMode, extensionViewerSettings,
    viewState, getCanvasView, getInputHandler, getToolbar, sbMessage, sbSection,
  } = deps;

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

  function describeError(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
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
    const vm = getCanvasView()?.getViewportManager();
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
      viewState.lastAppliedLocalFontGeneration = null;
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
      // 이미 승인된 exact local face를 놓치지 않도록 문서 view보다 먼저 준비한다.
      await loadStoredLocalFonts();
      await updateLoadProgress(75, '문서 상태 적용 중...');
      viewState.totalSections = docInfo.sectionCount ?? 1;
      sbSection().textContent = t('ui.sbSection.text', { p1: 1, p2: viewState.totalSections });
      applySavedTextMarkSettings();
      console.log('[initDoc] 3. inputHandler deactivate');
      getInputHandler()?.deactivate();
      console.log('[initDoc] 4. canvasView loadDocument');
      await updateLoadProgress(82, '페이지 렌더 준비 중...');
      const savedZoomFitMode = userSettings.getViewSettings().zoomFitMode;
      await getCanvasView()?.loadDocument();
      // 쪽 크기를 알 수 있는 첫 시점이다 — 저장된 맞춤은 이 문서의 쪽으로 다시 계산한다.
      applySavedZoomFitMode(savedZoomFitMode);
      console.log('[initDoc] 5. toolbar setEnabled');
      await updateLoadProgress(90, '도구 모음 준비 중...');
      getToolbar()?.setEnabled(true);
      console.log('[initDoc] 6. toolbar initFontDropdown + initStyleDropdown');
      getToolbar()?.initFontDropdown(docInfo.fontsUsed);
      getToolbar()?.initStyleDropdown();
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
            console.log(`[validation] ${report.count} warnings — 그대로 보기`, report.summary);
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
      getInputHandler()?.activateWithCaretPosition();
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
   * 문서 열기가 실패하면 빈 쪽 상태로 남는다. WASM이 아직 이전 문서를 쥐고 있으면 그 뷰를
   * 되살려, 열기 실패가 이미 열어 둔 문서를 잃는 일로 번지지 않게 한다.
   */
  function restoreViewAfterFailedOpen(): void {
    const canvasView = getCanvasView();
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
    getCanvasView()?.prepareDocumentLoad();
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
      getCanvasView()?.showBlankPage();
      if (!options.dataReadProgressShown) {
        await updateLoadProgress(0, '문서 데이터 준비 중...');
      }
      await updateLoadProgress(25, '문서 파싱 및 쪽 계산 중...');
      const docInfo = await loadDocumentForOpen(wasm, data, fileName);
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


  /**
   * 파일 로드 실패 시 사용자에게 에러를 명확히 알린다.
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

  return { loadFile, loadBytes, createNewDocument, canReplaceCurrentDocument };
}
