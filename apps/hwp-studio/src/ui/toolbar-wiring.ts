// EasyOffice: toolbar button, split-menu and focus wiring, split out of main.ts initialize().
import type { CommandDispatcher } from '@/command/dispatcher';
import {
  closeToolbarSplitMenus,
  moveToolbarSplitFocus,
  setToolbarSplitOpen,
  toolbarSplitItems,
} from '@/ui/toolbar-split-menu';

export function wireToolbars(dispatcher: CommandDispatcher): void {
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
}
