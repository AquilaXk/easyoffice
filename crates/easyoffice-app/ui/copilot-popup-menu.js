(function (host) {
  "use strict";

  // 코파일럿 패널의 작은 팝업 메뉴(머리글 더보기, 입력 카드의 +). 단추 옆에 고정 위치로 띄우고, 바깥을 누르거나 Esc를 누르거나 항목을 고르면 닫는다.
  const GAP_PX = 4;
  const EDGE_PX = 8;

  function createPopupMenu({ document, trigger, menu, placement = "down", align = "end", onOpen }) {
    const view = document.defaultView || host;
    let open = false;

    const items = () => Array.from(menu.querySelectorAll("button:not(:disabled)"));

    function position() {
      const box = trigger.getBoundingClientRect();
      const width = menu.offsetWidth || 0;
      const vw = Number(view.innerWidth) || 0;
      const left = align === "end" ? box.right - width : box.left;
      menu.style.left = Math.max(EDGE_PX, Math.min(left, vw - width - EDGE_PX)) + "px";
      if (placement === "up") {
        menu.style.top = "auto";
        menu.style.bottom = (Number(view.innerHeight) || 0) - box.top + GAP_PX + "px";
      } else {
        menu.style.bottom = "auto";
        menu.style.top = box.bottom + GAP_PX + "px";
      }
    }

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

  host.__EASYOFFICE_COPILOT_POPUP__ = { createPopupMenu };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
