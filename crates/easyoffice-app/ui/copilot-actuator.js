// 참조 구현: testing-library/user-event (MIT, Copyright (c) 2020 Giorgio Polvara, 태그 v14.6.1 커밋 d42954be, see THIRD_PARTY_NOTICES.md)
// 이벤트를 보내는 순서만 옮겼다(코드는 직접 썼다): 포인터 이동은 `src/system/pointer/index.ts:129-134`(pointerout/leave → pointerover/enter → pointermove, 마우스 쪽은 같은 순서),
// 누르기는 `src/system/pointer/index.ts:105-108`과 `pointer.ts:93-108`(pointerdown이 막히지 않으면 mousedown), `mouse.ts:114-143`(mousedown이 막히지 않으면 선택 시작과 포커스,
// 오른쪽 단추는 contextmenu), 떼기는 `index.ts:158-179`와 `pointer.ts:110-122`, `mouse.ts:146-198`(pointerup → mouseup → click, 두 번째 누름의 click 뒤에 dblclick),
// 키 입력은 `src/system/keyboard.ts:121-185`(keydown이 막히지 않으면 keypress → 글 반영 → keyup)이다. 글 반영 안에서 `beforeinput` → (글 바뀜) → `input` 순서는 입력칸이 직접 내고,
// 편집 영역(contenteditable)은 브라우저의 `execCommand("insertText")`가 같은 순서로 낸다.
//
// 코파일럿 포인터가 앱의 실제 화면 요소에 실제 이벤트를 보내 앱의 처리기가 변경을 만들게 하는 조작기다(PD-392). 합성 이벤트는 브라우저가 기본 동작(포커스, 글 선택, 글 넣기)을 하지 않으므로
// 기본 동작은 여기서 같은 자리에서 한다: 막히지 않은 mousedown은 포커스와 캐럿 자리를 정하고, 끌면 선택을 넓히고, 막히지 않은 keydown은 글자를 넣는다.
// 한글 조합(IME)은 흉내 내지 않는다. 글자는 완성된 글자 단위 `insertText`로 넣는다.

(function (host) {
  "use strict";

  const FOCUSABLE = 'input, textarea, select, button, a[href], [tabindex], [contenteditable]:not([contenteditable="false"])';
  const PRINTABLE = /^[^\p{C}]$/u;

  function createActuator({ document, win = host }) {
    let position = { x: 0, y: 0 };
    let hovered = null;
    // 누른 단추의 상태: 누른 곳의 요소와 mousedown이 기본 동작을 막았는지, 연속 누름 횟수.
    let pressed = null;
    let clicks = { count: 0, at: 0, key: "" };
    let lastPress = null;

    const view = () => document.defaultView || win;
    const finite = (n) => (Number.isFinite(n) ? n : 0);

    function make(type, init) {
      const W = view();
      const common = { bubbles: true, cancelable: true, composed: true, view: W, ...init };
      if (type.startsWith("pointer") && typeof W.PointerEvent === "function") {
        return new W.PointerEvent(type, { pointerId: 1, pointerType: "mouse", isPrimary: true, ...common });
      }
      if (type.startsWith("key")) return new W.KeyboardEvent(type, common);
      if (type === "beforeinput" || type === "input") return new W.InputEvent(type, common);
      return new W.MouseEvent(type.replace("pointer", "mouse"), common);
    }

    // 막히지 않았으면 `true`(브라우저의 `dispatchEvent`와 같다).
    const send = (target, type, init = {}) => target.dispatchEvent(make(type, init));
    const sendEnter = (target, type, init) => target.dispatchEvent(make(type, { ...init, bubbles: false, cancelable: false }));

    function mouseInit(extra = {}) {
      return { clientX: position.x, clientY: position.y, screenX: position.x, screenY: position.y, button: 0, buttons: pressed ? 1 : 0, detail: 0, ...extra };
    }

    const hitAt = (x, y) => document.elementFromPoint(x, y);

    // ---- 포인터 이동: pointerout/leave → pointerover/enter → pointermove, 같은 순서로 mouse 쪽.
    function moveTo(x, y) {
      position = { x, y };
      const next = hitAt(x, y);
      if (!next) return null;
      const init = mouseInit();
      if (next !== hovered) {
        if (hovered) {
          send(hovered, "pointerout", init);
          send(hovered, "mouseout", init);
          sendEnter(hovered, "pointerleave", init);
          sendEnter(hovered, "mouseleave", init);
        }
        send(next, "pointerover", init);
        sendEnter(next, "pointerenter", init);
        send(next, "mouseover", init);
        sendEnter(next, "mouseenter", init);
        hovered = next;
      }
      send(next, "pointermove", init);
      // pointerdown가 막히면 mouse 이벤트가 오지 않는다. mousedown이 막히면 mouse 이벤트는 오지만 글 선택은 하지 않는다.
      if (pressed?.pointerPrevented !== true) send(next, "mousemove", init);
      if (pressed && !pressed.pointerPrevented && !pressed.mousePrevented) extendSelection(x, y);
      return next;
    }

    // ---- 글 선택: 막히지 않은 mousedown이 캐럿을 정하고 끌면 넓힌다(브라우저의 기본 동작).
    function caretAt(x, y) {
      if (document.caretRangeFromPoint) {
        const range = document.caretRangeFromPoint(x, y);
        return range ? { node: range.startContainer, offset: range.startOffset } : null;
      }
      const pos = document.caretPositionFromPoint?.(x, y);
      return pos ? { node: pos.offsetNode, offset: pos.offset } : null;
    }

    function extendSelection(x, y) {
      if (!pressed?.anchor) return;
      const to = caretAt(x, y);
      const sel = view().getSelection?.();
      if (!to || !sel) return;
      sel.setBaseAndExtent(pressed.anchor.node, pressed.anchor.offset, to.node, to.offset);
    }

    function focusFor(target) {
      const next = target.closest?.(FOCUSABLE);
      const prev = document.activeElement;
      if (next) {
        if (prev !== next) flushChange(prev, () => next.focus({ preventScroll: true }));
      } else if (prev && prev !== document.body) {
        flushChange(prev, () => prev.blur?.());
      }
    }

    // ---- 누르기와 떼기
    function down({ button = 0, shift = false, ctrl = false, meta = false, alt = false } = {}) {
      const target = hitAt(position.x, position.y);
      if (!target || pressed) return null;
      const key = `${button}`;
      const now = Date.now();
      clicks = clicks.key === key && now - clicks.at < 500 ? { count: clicks.count + 1, at: now, key } : { count: 1, at: now, key };
      const mods = { shiftKey: shift, ctrlKey: ctrl, metaKey: meta, altKey: alt };
      const buttons = button === 2 ? 2 : 1;
      pressed = { target, button, pointerPrevented: false, mousePrevented: false, anchor: null, mods };
      pressed.pointerPrevented = !send(target, "pointerdown", mouseInit({ button, buttons, detail: 0, ...mods }));
      lastPress = { x: position.x, y: position.y, target };
      if (!pressed.pointerPrevented) {
        const open = send(target, "mousedown", mouseInit({ button, buttons, detail: clicks.count, ...mods }));
        if (!open) pressed.mousePrevented = true;
        else {
          pressed.anchor = button === 0 ? caretAt(position.x, position.y) : null;
          if (pressed.anchor && !shift) view().getSelection?.()?.collapse(pressed.anchor.node, pressed.anchor.offset);
          focusFor(target);
        }
      }
      if (button === 2) send(target, "contextmenu", mouseInit({ button, buttons, detail: 0, ...mods }));
      return target;
    }

    function up() {
      const state = pressed;
      if (!state) return null;
      const target = hitAt(position.x, position.y) || state.target;
      const { button, mods } = state;
      pressed = null;
      send(target, "pointerup", mouseInit({ button, buttons: 0, ...mods }));
      if (!state.pointerPrevented) send(target, "mouseup", mouseInit({ button, buttons: 0, detail: clicks.count, ...mods }));
      // 누른 요소와 뗀 요소의 공통 조상에 click(오른쪽 단추는 auxclick)을 보낸다.
      let common = state.target;
      while (common && !common.contains(target)) common = common.parentElement;
      if (common) {
        send(common, button === 0 ? "click" : "auxclick", mouseInit({ button, buttons: 0, detail: clicks.count, ...mods }));
        if (button === 0 && clicks.count === 2) send(common, "dblclick", mouseInit({ button, buttons: 0, detail: 2, ...mods }));
      }
      return target;
    }

    // ---- 키 입력
    const activeTarget = () => document.activeElement || document.body;
    const isField = (el) => /^(INPUT|TEXTAREA)$/.test(el?.tagName || "");

    function insertIntoField(el, text) {
      let start = el.value.length;
      let end = start;
      try {
        start = el.selectionStart ?? start;
        end = el.selectionEnd ?? end;
      } catch { /* 숫자·날짜 입력칸은 선택 범위를 읽지 못한다 */ }
      const value = el.value;
      el.value = value.slice(0, start) + text + value.slice(end);
      try {
        el.setSelectionRange(start + text.length, start + text.length);
      } catch { /* 같은 이유 */ }
    }

    // 글자 하나(또는 붙여 넣은 짧은 글)를 `target`에 넣는다. 브라우저의 글 넣기 명령이 사용자가 친 것과 같은 beforeinput → 글 바뀜 → input을 내고, 입력칸에는 "사용자가 고쳤음"
    // 표시도 남겨 blur 때 `change`가 난다. 명령이 없는 환경(false)에서만 입력칸 값을 직접 바꾸고 같은 순서로 이벤트를 보낸다.
    function insertText(target, text) {
      if (document.execCommand("insertText", false, text)) return true;
      if (!isField(target)) return false;
      const init = { inputType: "insertText", data: text };
      if (!send(target, "beforeinput", init)) return false;
      insertIntoField(target, text);
      send(target, "input", { ...init, cancelable: false });
      return true;
    }

    // 입력칸의 `change`: 포커스를 잃거나 Enter를 칠 때 값이 포커스를 받았을 때와 다르면 한 번 난다. 브라우저가 이미 냈으면 다시 내지 않는다
    // (글을 바로 값으로 넣은 입력칸과 한 글자 값은 브라우저가 "사용자가 고쳤음"으로 보지 않을 수 있다).
    const baseline = new WeakMap();
    const onFocusIn = (event) => {
      if (isField(event.target)) baseline.set(event.target, event.target.value);
    };
    document.addEventListener("focusin", onFocusIn, true);

    function flushChange(field, action) {
      if (!isField(field) || !baseline.has(field)) return action();
      let seen = false;
      const mark = () => {
        seen = true;
      };
      field.addEventListener("change", mark, { once: true });
      const result = action();
      field.removeEventListener("change", mark);
      if (!seen && field.value !== baseline.get(field)) send(field, "change", { cancelable: false });
      baseline.set(field, field.value);
      return result;
    }

    function deleteBackward(target) {
      if (isField(target)) {
        const init = { inputType: "deleteContentBackward" };
        if (!send(target, "beforeinput", init)) return;
        const start = target.selectionStart ?? target.value.length;
        const end = target.selectionEnd ?? start;
        const from = start === end ? Math.max(0, start - 1) : start;
        target.value = target.value.slice(0, from) + target.value.slice(end);
        target.setSelectionRange?.(from, from);
        send(target, "input", { ...init, cancelable: false });
        return;
      }
      document.execCommand("delete");
    }

    /** 키 하나: keydown → (막히지 않으면) keypress → 글 반영 → keyup. `key`는 `KeyboardEvent.key`. */
    function press(key, { shift = false, ctrl = false, meta = false, alt = false } = {}) {
      const target = activeTarget();
      const init = { key, code: key.length === 1 ? `Key${key.toUpperCase()}` : key, shiftKey: shift, ctrlKey: ctrl, metaKey: meta, altKey: alt };
      const strike = () => {
        const open = send(target, "keydown", init);
        if (open) {
          const text = key === "Enter" ? "\n" : key;
          const typed = !ctrl && !meta && !alt && (PRINTABLE.test(key) || key === "Enter");
          if ((ctrl || meta) && key === "a") document.execCommand("selectAll");
          if (typed) send(target, "keypress", { ...init, charCode: text.codePointAt(0) });
          if (typed && key === "Enter") {
            if (isField(target)) {
              if (target.tagName === "TEXTAREA") insertText(target, "\n");
            } else {
              document.execCommand(shift ? "insertLineBreak" : "insertParagraph");
            }
          } else if (typed) insertText(target, text);
          else if (key === "Backspace") deleteBackward(target);
          else if (key === "Delete" && !isField(target)) document.execCommand("forwardDelete");
        } else if (PRINTABLE.test(key) && !ctrl && !meta && !alt && isField(activeTarget()) && activeTarget() !== target) {
          // 앱이 키를 받아 입력칸을 열고 첫 글자를 직접 넣었다: 그 값은 사용자가 친 것이므로 확정할 때 `change`가 나야 한다.
          baseline.set(activeTarget(), undefined);
        }
        send(activeTarget(), "keyup", init);
        return open;
      };
      return key === "Enter" || key === "Tab" ? flushChange(target, strike) : strike();
    }

    /** 글을 한 글자(코드 포인트)씩 친다. 줄바꿈은 Enter 키다. */
    function typeText(text) {
      for (const ch of String(text)) press(ch === "\n" ? "Enter" : ch);
    }

    // ---- 화면 위치: 요소 안에서 실제로 그 요소가 맞는 지점(다른 것이 가리면 다른 지점을 찾는다). 없으면 `null`.
    function pointIn(el, hint) {
      const r = el.getBoundingClientRect();
      const W = view();
      const left = Math.max(r.left, 0);
      const top = Math.max(r.top, 0);
      const right = Math.min(r.right, finite(W.innerWidth) || r.right);
      const bottom = Math.min(r.bottom, finite(W.innerHeight) || r.bottom);
      if (right <= left || bottom <= top) return null;
      const w = right - left;
      const h = bottom - top;
      const spots = [hint, [0.5, 0.5], [0.15, 0.5], [0.85, 0.5], [0.5, 0.2], [0.5, 0.8], [0.05, 0.05]].filter(Boolean);
      for (const [fx, fy] of spots) {
        const x = left + w * fx;
        const y = top + h * fy;
        const hit = hitAt(x, y);
        if (hit && (hit === el || el.contains(hit))) return { x, y, hit };
      }
      return null;
    }

    return {
      moveTo, down, up, press, typeText, pointIn, caretAt, insertText,
      dispose: () => document.removeEventListener("focusin", onFocusIn, true),
      get position() { return { ...position }; },
      get lastPress() { return lastPress; },
    };
  }

  host.__EASYOFFICE_COPILOT_ACTUATOR__ = Object.freeze({ createActuator });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
