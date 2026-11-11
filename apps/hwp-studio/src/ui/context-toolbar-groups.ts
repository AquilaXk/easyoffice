// EasyOffice: the contextual toolbar groups (picture rotate, header/footer, footnote), split out of
// main.ts setupEventListeners().
import type { EventBus } from '@/core/event-bus';
import type { initIconToolbarScroller } from '@/ui/icon-toolbar-scroller';
import { t } from '@/i18n/index.ts';
import {
  headerFooterApplyToLabel,
  parseHeaderFooterModeChanged,
} from '@/engine/header-footer-mode';

function headerFooterModeLabel(mode: string): string {
  return mode === 'header' ? t('ui.tbHfLabel.header') : t('ui.tbHfLabel.footer');
}

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

export function wireContextToolbarGroups(
  eventBus: EventBus,
  iconToolbarScroller: ReturnType<typeof initIconToolbarScroller>,
): void {
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
