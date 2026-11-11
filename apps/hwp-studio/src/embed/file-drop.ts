// EasyOffice: the file input and the drag-and-drop paths, split out of main.ts.
import type { WasmBridge } from '@/core/wasm-bridge';
import type { InputHandler } from '@/engine/input-handler';
import { isSupportedDocumentFileName } from '@/command/file-system-access';
import { showToast } from '@/ui/toast';
import { showDropConfirmDialog } from '@/ui/drop-confirm-dialog';

export interface FileDropDeps {
  wasm: WasmBridge;
  chromeMode: 'embed' | 'full';
  loadFile: (file: File, options?: { skipUnsavedGuard?: boolean }) => Promise<boolean>;
  getInputHandler: () => InputHandler | null;
}

export function setupFileInput({ wasm, chromeMode, loadFile, getInputHandler }: FileDropDeps): void {
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
      const inputHandler = getInputHandler();
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
