// 참조 구현: excalidraw/excalidraw (MIT, Copyright (c) 2020 Excalidraw, 커밋 2c3dcc06, see THIRD_PARTY_NOTICES.md)
// `packages/excalidraw/clients.ts:60-203`(`renderRemoteCursors`: 협업자 포인터의 화살표 꼴(0,0 → 0,14 → 4,9 → 11,8), 흰 테두리 6·본색 2 두 겹, 이름표 자리와 안쪽 여백,
// 뷰포트 밖이면 가장자리로 붙이고 투명도 0.3에 이름표 생략, 마우스 누름 고리), `packages/common/src/utils.ts:253-255`(`easeOut`),
// `packages/excalidraw/renderer/animation.ts:30-211`(`AnimationController`: 프레임마다 `deltaTime`을 넘기는 키별 애니메이션 루프)를 옮겼다.
// 협업자 포인터 위치를 부드럽게 잇는 코드는 원본에 없어서(수신한 좌표로 바로 그린다) 같은 컨트롤러와 `easeOut`으로 이동을 보간했다.
// 참조 구현: yjs/y-prosemirror (MIT, Copyright (c) 2019 Kevin Jahns, 커밋 47a73852, `src/cursor-plugin.js:25-62`와 `demo/prosemirror.html:36-68`):
// 원격 커서의 캐럿 막대(양옆 1px 줄), 이름 표지, 선택 강조(색 + 투명도 0.3)와 위치를 문서 길이 안으로 자르는 규칙(`maxsize`)을 옮겼다.
// 원본은 ProseMirror 장식으로 편집 가능한 DOM 안에 위젯을 넣지만 여기서는 편집 DOM을 건드리지 않으려 뷰포트 위 덧씌움 층에 그린다.

(function (host) {
  "use strict";

  const SVG_NS = "http://www.w3.org/2000/svg";
  const LABEL = "코파일럿";
  // 옮김[excalidraw-geometry] 시작
  const POINTER_WIDTH = 11;
  const POINTER_HEIGHT = 14;
  // 화살표 꼴(x, y 순서의 꼭짓점). 흰 테두리와 본색이 같은 꼴을 겹쳐 그린다.
  const ARROW_POINTS = "0,0 0,14 4,9 11,8";
  const LABEL_PADDING_X = 5;
  const LABEL_PADDING_Y = 3;
  const INACTIVE_ALPHA = 0.3;
  const DOWN_RING_RADIUS = 15;

  const easeOut = (k) => 1 - Math.pow(1 - k, 4);

  /** 뷰포트 안에서 화살표가 설 자리와, 원래 자리가 밖이었는지. */
  function clampPointer(x, y, width, height) {
    const isOutOfBounds = x < 0 || x > width - POINTER_WIDTH || y < 0 || y > height - POINTER_HEIGHT;
    return {
      x: Math.min(Math.max(x, 0), width - POINTER_WIDTH),
      y: Math.min(Math.max(y, 0), height - POINTER_HEIGHT),
      isOutOfBounds,
    };
  }
  // 옮김[excalidraw-geometry] 끝

  // 옮김[excalidraw-animation] 시작
  /** 키마다 하나씩 도는 프레임 애니메이션. `animation({ deltaTime, state })`가 상태를 돌려주는 동안 이어지고 거짓을 돌려주면 끝난다. */
  function createAnimationController({ scheduler, now }) {
    const animations = new Map();
    let frame = null;

    const cancelIfIdle = () => {
      if (animations.size > 0 || frame === null) return;
      scheduler.cancelAnimationFrame(frame);
      frame = null;
    };

    const scheduleNextFrame = () => {
      if (frame !== null) return;
      frame = scheduler.requestAnimationFrame(tick);
    };

    function tick() {
      frame = null;
      // 콜백이 애니메이션을 더하거나 빼도 이 프레임은 시작할 때의 목록으로 돈다.
      for (const [key, record] of [...animations]) {
        if (animations.get(key) !== record) continue;
        const time = now();
        const deltaTime = record.lastTime === 0 ? 0 : time - record.lastTime;
        const state = record.animation({ deltaTime, state: record.state });
        if (animations.get(key) !== record) continue;
        if (!state) {
          animations.delete(key);
        } else {
          record.lastTime = time;
          record.state = state;
        }
      }
      if (animations.size === 0) return;
      scheduleNextFrame();
    }

    return {
      start(key, animation) {
        if (animations.has(key)) return;
        const record = { animation, lastTime: 0, state: undefined };
        animations.set(key, record);
        let initialState;
        try {
          initialState = animation({ deltaTime: 0, state: undefined });
        } catch (error) {
          if (animations.get(key) === record) animations.delete(key);
          cancelIfIdle();
          throw error;
        }
        // 첫 호출이 자기 자신을 취소하거나 바꿨으면 되살리지 않는다.
        if (animations.get(key) !== record) return;
        if (!initialState) {
          animations.delete(key);
          cancelIfIdle();
          return;
        }
        record.state = initialState;
        scheduleNextFrame();
      },
      running: (key) => animations.has(key),
      cancel(key) {
        animations.delete(key);
        cancelIfIdle();
      },
    };
  }
  // 옮김[excalidraw-animation] 끝

  function svgElement(document, tag, attrs) {
    const el = document.createElementNS(SVG_NS, tag);
    for (const [name, value] of Object.entries(attrs)) el.setAttribute(name, String(value));
    return el;
  }

  function createCopilotCursor({ document, win = host, now, scheduler, label = LABEL } = {}) {
    const clock = now || (() => (win.performance?.now ? win.performance.now() : Date.now()));
    const frames = scheduler || {
      requestAnimationFrame: (fn) => (win.requestAnimationFrame ? win.requestAnimationFrame(fn) : win.setTimeout(fn, 16)),
      cancelAnimationFrame: (id) => (win.cancelAnimationFrame ? win.cancelAnimationFrame(id) : win.clearTimeout(id)),
    };
    const controller = createAnimationController({ scheduler: frames, now: clock });
    const MOVE = Symbol("copilot-pointer-move");

    const layer = document.createElement("div");
    layer.id = "eo-copilot-cursor-layer";
    layer.className = "eo-copilot-cursor-layer eo-copilot-transient";
    layer.setAttribute("aria-hidden", "true");
    layer.hidden = true;
    const selectionLayer = document.createElement("div");
    selectionLayer.className = "eo-copilot-selection-layer";
    const caret = document.createElement("div");
    caret.className = "eo-copilot-text-caret";
    caret.hidden = true;
    const pointer = document.createElement("div");
    pointer.className = "eo-copilot-pointer";
    const arrow = svgElement(document, "svg", { width: POINTER_WIDTH, height: POINTER_HEIGHT, viewBox: `0 0 ${POINTER_WIDTH} ${POINTER_HEIGHT}`, "aria-hidden": "true" });
    arrow.setAttribute("class", "eo-copilot-arrow");
    arrow.appendChild(svgElement(document, "polygon", { class: "eo-copilot-arrow-outline", points: ARROW_POINTS }));
    arrow.appendChild(svgElement(document, "polygon", { class: "eo-copilot-arrow-body", points: ARROW_POINTS }));
    // 마우스를 누른 고리(원본의 반지름 15 원): 옅은 바깥 줄 3과 본색 줄 1 두 겹.
    const ring = svgElement(document, "svg", { width: DOWN_RING_RADIUS * 2, height: DOWN_RING_RADIUS * 2, viewBox: `${-DOWN_RING_RADIUS} ${-DOWN_RING_RADIUS} ${DOWN_RING_RADIUS * 2} ${DOWN_RING_RADIUS * 2}`, "aria-hidden": "true" });
    ring.setAttribute("class", "eo-copilot-ring");
    ring.appendChild(svgElement(document, "circle", { class: "eo-copilot-ring-outline", cx: 0, cy: 0, r: DOWN_RING_RADIUS }));
    ring.appendChild(svgElement(document, "circle", { class: "eo-copilot-ring-body", cx: 0, cy: 0, r: DOWN_RING_RADIUS }));
    ring.setAttribute("hidden", "");
    const name = document.createElement("span");
    name.className = "eo-copilot-label";
    name.textContent = label;
    for (const part of [ring, arrow, name]) pointer.appendChild(part);
    for (const part of [selectionLayer, caret, pointer]) layer.appendChild(part);

    let position = { x: 0, y: 0 };
    let installed = false;
    // 진행 중인 이동이 풀어 줄 약속. 다른 이동이나 `hide`가 가로채면 `false`로 푼다.
    let pendingMove = null;

    function interruptMove() {
      controller.cancel(MOVE);
      const resolve = pendingMove;
      pendingMove = null;
      resolve?.(false);
    }

    function viewport() {
      const root = document.documentElement;
      return {
        width: Number(win.innerWidth) || root?.clientWidth || 1280,
        height: Number(win.innerHeight) || root?.clientHeight || 800,
      };
    }

    function render() {
      const { width, height } = viewport();
      const placed = clampPointer(position.x, position.y, width, height);
      pointer.style.setProperty("transform", `translate(${placed.x}px, ${placed.y}px)`);
      pointer.classList.toggle("is-off", placed.isOutOfBounds);
      name.hidden = placed.isOutOfBounds;
    }

    function install() {
      if (installed) return;
      installed = true;
      (document.body || document.documentElement).appendChild(layer);
    }

    function setPosition(x, y) {
      position = { x, y };
      render();
    }

    return {
      get element() { return layer; },
      get position() { return { ...position }; },
      get alpha() { return INACTIVE_ALPHA; },
      show() {
        install();
        layer.hidden = false;
        render();
      },
      hide() {
        interruptMove();
        layer.hidden = true;
        selectionLayer.replaceChildren();
        caret.hidden = true;
        ring.setAttribute("hidden", "");
      },
      destroy() {
        interruptMove();
        layer.remove();
        installed = false;
      },
      /** 애니메이션 없이 그 자리로 간다. */
      place: setPosition,
      /** 그 자리까지 부드럽게 간다. 도착하면 `true`로 풀리고, 다른 이동이 가로채면 `false`로 풀린다. */
      moveTo(x, y, { duration } = {}) {
        const from = { ...position };
        const distance = Math.hypot(x - from.x, y - from.y);
        const total = duration ?? Math.min(620, Math.max(160, 120 + distance * 0.8));
        if (total <= 0 || distance === 0) {
          interruptMove();
          setPosition(x, y);
          return Promise.resolve(true);
        }
        return this.animate(total, (eased) => setPosition(from.x + (x - from.x) * eased, from.y + (y - from.y) * eased));
      },
      /** `duration` ms 동안 `step(eased)`를 프레임마다 부른다(`eased`는 0에서 1로 `easeOut`을 타고 간다). 가로채이면 `false`로 풀린다. */
      animate(duration, step) {
        interruptMove();
        return new Promise((resolve) => {
          pendingMove = resolve;
          controller.start(MOVE, ({ deltaTime, state }) => {
            const elapsed = (state?.elapsed ?? 0) + deltaTime;
            const t = Math.min(1, elapsed / duration);
            step(easeOut(t));
            if (t < 1) return { elapsed };
            pendingMove = null;
            resolve(true);
            return null;
          });
        });
      },
      /** 진행 중인 이동·애니메이션을 끊는다(`false`로 풀린다). */
      interrupt: interruptMove,
      /** 마우스를 누른 채(드래그) 있는 동안 고리를 보인다(원본 `remotePointerButton === "down"`). */
      press(down) {
        if (down) ring.removeAttribute("hidden");
        else ring.setAttribute("hidden", "");
      },
      // 옮김[yprosemirror-widgets] 시작
      /** 선택 강조: 뷰포트 좌표의 사각형들. 비우면 지운다. */
      setSelection(rects) {
        const nodes = [];
        for (const rect of rects || []) {
          if (!(rect.width > 0 && rect.height > 0)) continue;
          const box = document.createElement("div");
          box.className = "eo-copilot-selection";
          box.style.setProperty("left", `${rect.left}px`);
          box.style.setProperty("top", `${rect.top}px`);
          box.style.setProperty("width", `${rect.width}px`);
          box.style.setProperty("height", `${rect.height}px`);
          nodes.push(box);
        }
        selectionLayer.replaceChildren(...nodes);
      },
      /** 글자 사이의 캐럿 막대. `null`이면 숨긴다. */
      setCaret(rect) {
        if (!rect) {
          caret.hidden = true;
          return;
        }
        caret.hidden = false;
        caret.style.setProperty("left", `${rect.left}px`);
        caret.style.setProperty("top", `${rect.top}px`);
        caret.style.setProperty("height", `${rect.height || 16}px`);
      },
      /** 캐럿이 쉬는 동안 깜박인다(`typed.js` 쉼 표시). */
      setIdle(idle) {
        caret.classList.toggle("is-idle", Boolean(idle));
      },
      // 옮김[yprosemirror-widgets] 끝
    };
  }

  host.__EASYOFFICE_COPILOT_CURSOR__ = Object.freeze({
    LABEL, POINTER_WIDTH, POINTER_HEIGHT, ARROW_POINTS, LABEL_PADDING_X, LABEL_PADDING_Y, INACTIVE_ALPHA, DOWN_RING_RADIUS,
    easeOut, clampPointer, createAnimationController, createCopilotCursor,
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
