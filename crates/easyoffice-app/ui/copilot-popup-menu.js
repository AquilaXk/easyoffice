// 참조 구현: continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)
// `gui/src/components/ModeSelect/ModeSelect.tsx` 90-165: 현재 모드를 보이는 단추와 그 아래 선택지 목록(고른 항목에 체크 표시, 127·144·163).
// 같은 모양을 모드 선택(질문·편집)에 쓰고, 헤드리스 Listbox 대신 `aria-checked` 항목으로 옮겼다. 모델 선택 메뉴는 `copilot-model-ui.js`다.
// 메뉴 모양 값은 assistant-ui(MIT) `ModelSelector`·`ActionBarMore`의 것이고(`copilot.css`), 자리 뒤집기는 floating-ui(MIT) `flip.ts`·`shift.ts`에서 옮겼다(아래 `placeMenu`).

(function (host) {
  "use strict";

  // 코파일럿 패널의 작은 팝업 메뉴(머리글 더보기, 입력 카드의 +). 단추 옆에 고정 위치로 띄우고, 바깥을 누르거나 Esc를 누르거나 항목을 고르면 닫는다.
  const GAP_PX = 6; // assistant-ui `sideOffset={6}`(ActionBarMore·ModelSelector 메뉴)
  const EDGE_PX = 8;

  /**
   * 메뉴 자리 계산: 원하는 쪽(`preferred`: "down"|"up")이 화면 밖으로 넘치면 반대쪽으로 뒤집고, 양쪽 다 넘치면 덜 넘치는 쪽을 쓴다(처음 쪽이 같으면 처음 쪽).
   * 가로는 화면 안으로 민다. 참조 구현: floating-ui (MIT, Copyright (c) 2021-present Floating UI contributors) 커밋 c3042f1f
   * `packages/core/src/middleware/flip.ts` 98-178(기본 대체 자리 = 반대쪽, `bestFit`: 넘친 양의 합이 가장 작은 자리)과 `shift.ts`(교차축을 경계 안으로 민다).
   */
  function placeMenu({ view, menu, box, preferred, gap = GAP_PX, align }) {
    const width = menu.offsetWidth || 0;
    const height = menu.offsetHeight || 0;
    const vw = Number(view.innerWidth) || 0;
    const vh = Number(view.innerHeight) || 0;
    const overflow = {
      down: Math.max(0, box.bottom + gap + height + EDGE_PX - vh),
      up: Math.max(0, height + gap + EDGE_PX - box.top),
    };
    const other = preferred === "down" ? "up" : "down";
    const side = overflow[preferred] <= 0 || overflow[preferred] <= overflow[other] ? preferred : other;
    const left = align === "end" ? box.right - width : align === "center" ? box.left + box.width / 2 - width / 2 : box.left;
    menu.style.left = Math.max(EDGE_PX, Math.min(left, vw - width - EDGE_PX)) + "px";
    menu.style.bottom = "auto";
    menu.style.top = (side === "down" ? box.bottom + gap : Math.max(EDGE_PX, box.top - gap - height)) + "px";
    menu.dataset.side = side === "down" ? "bottom" : "top";
  }

  function createPopupMenu({ document, trigger, menu, placement = "down", align = "end", onOpen }) {
    const view = document.defaultView || host;
    let open = false;

    const items = () => Array.from(menu.querySelectorAll("button")).filter((btn) => !btn.disabled && !btn.hidden);

    const position = () => placeMenu({ view, menu, box: trigger.getBoundingClientRect(), preferred: placement, align });

    function close(restoreFocus = false) {
      if (!open) return;
      open = false;
      menu.hidden = true;
      trigger.setAttribute("aria-expanded", "false");
      if (restoreFocus) trigger.focus?.();
    }

    function show() {
      onOpen?.();
      open = true;
      menu.hidden = false;
      trigger.setAttribute("aria-expanded", "true");
      position();
      items()[0]?.focus?.();
    }

    trigger.addEventListener("click", () => (open ? close(false) : show()));
    menu.addEventListener("click", (ev) => {
      if (ev.target?.closest?.("button")) close(false);
    });
    menu.addEventListener("keydown", (ev) => {
      if (ev.key === "Escape") {
        ev.preventDefault();
        close(true);
        return;
      }
      if (ev.key !== "ArrowDown" && ev.key !== "ArrowUp") return;
      const list = items();
      if (list.length === 0) return;
      ev.preventDefault();
      const at = list.indexOf(document.activeElement);
      const step = ev.key === "ArrowDown" ? 1 : -1;
      list[(at + step + list.length) % list.length].focus?.();
    });
    document.addEventListener("mousedown", (ev) => {
      if (open && !menu.contains(ev.target) && !trigger.contains(ev.target)) close(false);
    });
    view.addEventListener?.("resize", () => close(false));

    return { close, isOpen: () => open };
  }

  /**
   * 눌린 단추 바로 아래(위쪽 자리가 모자라면 위)에 뜨는 메뉴 하나(싫어요 이유 목록). 단추는 대화를 다시 그릴 때마다 새로 만들어지므로
   * 메뉴는 단추 소유가 아니라 `open(anchor, items)`로 그때그때 연다. 바깥을 누르거나 Esc, 항목 선택, 창 크기 변경에 닫는다.
   * 자리는 원본 Feedback.tsx의 `Ariakit.Popover placement="bottom" gutter={8}`(단추 아래 가운데, 8px 띄움)을 따른다.
   */
  const FEEDBACK_GUTTER_PX = 8;

  function createAnchoredMenu({ document, menu }) {
    const view = document.defaultView || host;
    let anchorEl = null;

    function close(restoreFocus = false) {
      if (!anchorEl) return;
      const anchor = anchorEl;
      anchorEl = null;
      menu.hidden = true;
      anchor.setAttribute?.("aria-expanded", "false");
      if (restoreFocus) anchor.focus?.();
    }

    function open(anchor, items) {
      close();
      anchorEl = anchor;
      menu.replaceChildren(...items);
      menu.hidden = false;
      anchor.setAttribute?.("aria-expanded", "true");
      placeMenu({ view, menu, box: anchor.getBoundingClientRect(), preferred: "down", gap: FEEDBACK_GUTTER_PX, align: "center" });
      menu.querySelector("button")?.focus?.();
    }

    menu.addEventListener("click", (ev) => {
      if (ev.target?.closest?.("button")) close(false);
    });
    menu.addEventListener("keydown", (ev) => {
      if (ev.key !== "Escape") return;
      ev.preventDefault();
      close(true);
    });
    document.addEventListener("mousedown", (ev) => {
      if (anchorEl && !menu.contains(ev.target) && !anchorEl.contains(ev.target)) close(false);
    });
    view.addEventListener?.("resize", () => close(false));

    return { open, close, isOpen: () => anchorEl !== null };
  }

  host.__EASYOFFICE_COPILOT_POPUP__ = { createPopupMenu, createAnchoredMenu, placeMenu };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
