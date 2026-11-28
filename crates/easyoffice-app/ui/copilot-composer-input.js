// 참조 구현: Andarist/react-textarea-autosize (MIT, Copyright (c) 2013 Andrey Popp, see THIRD_PARTY_NOTICES.md)
// `src/calculateNodeHeight.ts` 56-72(한 줄 높이를 재고 `minRows`·`maxRows`로 높이를 가둠). assistant-ui/assistant-ui(MIT)의
// `packages/react/src/primitives/composer/ComposerInput.tsx` 17-20·131·482가 입력칸을 이 자동 높이 textarea로 쓴다. 숨은 textarea로
// 재는 부분은 옮기지 않고 입력칸 자신의 `scrollHeight`와 계산된 줄 높이로 같은 값을 얻는다.

(function (host) {
  "use strict";

  /**
   * 입력칸 높이를 내용에 맞추되 `minRows`줄 이상 `maxRows`줄 이하로 둔다. 레이아웃이 없는 환경(`scrollHeight` 0)에서는 높이를 건드리지 않는다.
   * 최대 줄을 넘는 내용은 입력칸 안에서 스크롤한다.
   */
  function fitTextareaHeight(textarea, { minRows = 1, maxRows = 5 } = {}) {
    const view = textarea.ownerDocument?.defaultView;
    textarea.style.height = "auto";
    const style = view?.getComputedStyle?.(textarea);
    const rowHeight = Number.parseFloat(style?.lineHeight);
    const padding = (Number.parseFloat(style?.paddingTop) || 0) + (Number.parseFloat(style?.paddingBottom) || 0);
    const content = Number(textarea.scrollHeight) || 0;
    if (!(content > 0) || !(rowHeight > 0)) return;
    const height = Math.min(rowHeight * maxRows + padding, Math.max(rowHeight * minRows + padding, content));
    textarea.style.height = height + "px";
  }

  host.__EASYOFFICE_COPILOT_COMPOSER_INPUT__ = { fitTextareaHeight };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
