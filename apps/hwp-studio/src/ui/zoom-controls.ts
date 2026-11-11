// EasyOffice: the status-bar zoom controls, split out of main.ts.
import type { CommandDispatcher } from '@/command/dispatcher';
import type { CanvasView } from '@/view/canvas-view';
import { detectPlatformKind } from '@/engine/navigation-keymap';
import {
  percentToZoomSliderPosition,
  zoomPercentShortcutTitle,
  zoomSliderPositionToPercent,
} from '@/view/zoom-status-controls';

export function setupZoomControls(canvasView: CanvasView | null, dispatcher: CommandDispatcher): void {
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
