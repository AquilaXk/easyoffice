// 직접 썼다(PD-392): 코파일럿 포인터가 연산을 실제 화면 조작으로 해 보이는 실행기다. 연산마다 모듈별 계획 표(`copilot-real-*.js`)가 "UI 단계 목록"을 만들고
// (대상 요소를 찾는 함수 + click/dblclick/contextmenu/drag/select/type/key), 이 실행기가 단계마다 포인터를 대상 위로 보낸 뒤 `copilot-actuator.js`로 실제 이벤트를 보내
// 앱의 원래 처리기가 변경을 만들게 한다. 단계 뒤에는 기대 상태를 검사한다.
//
// 안전: 단계가 실패하거나(요소 없음, 가려짐, 기대 상태가 다름) 사용자가 손을 대면(믿을 수 있는 pointerdown·mousedown·keydown) 멈추고, 하던 연산이 낸 변경은 연산 시작 때 뜬
// 작업공간 스냅숏으로 되돌린 뒤 남은 연산을 돌려준다. 부르는 쪽(`copilot-actions.js`)이 남은 연산을 내부 경로(`applyOperations`)로 적용하므로 최종 결과는 내부 경로만 썼을 때와 같다.
// 끝난 연산의 되돌리기 기록은 부르는 쪽이 만든 역사 묶음(`beginHistoryGroup`)이 삼키고 한 턴을 한 단계로 남긴다.

(function (host) {
  "use strict";

  // 한 응답에서 UI로 하는 단계(셀·문단·레코드 단위) 수 상한. 기존 연출 상한(`copilot-perform.js`의 `MAX_ANIMATED_TARGETS`)과 같다.
  const MAX_REAL_UNITS = 60;
  const PRESS_MS = 90;
  const GAP_MS = 60;
  const DRAG_MS = 380;
  const EXPECT_MS = 1500;
  const POLL_MS = 25;
  const CELL_TYPE_SPEED = 12;
  const CELL_TYPE_MS = 500;

  class Aborted extends Error {
    constructor(kind) {
      super("copilot-real aborted: " + kind);
      this.kind = kind;
    }
  }
  class StepFailed extends Error {}

  const box = (r) => ({ left: r.left, top: r.top, width: r.width, height: r.height, right: r.right, bottom: r.bottom });

  /** 글 위치(문자 수)를 요소 안의 글 노드 위치로. */
  function textPoint(el, offset) {
    const walker = el.ownerDocument.createTreeWalker(el, 4);
    let left = offset;
    let last = null;
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      last = node;
      if (left <= node.data.length) return { node, offset: left };
      left -= node.data.length;
    }
    return last ? { node: last, offset: last.data.length } : { node: el, offset: 0 };
  }

  function textRange(el, from, to) {
    const range = el.ownerDocument.createRange();
    const a = textPoint(el, from);
    const b = textPoint(el, to);
    range.setStart(a.node, a.offset);
    range.setEnd(b.node, b.offset);
    return range;
  }

  function createRealRunner(deps) {
    const { document, win = host, createCursor, createTypist, captureSnapshot, restoreSnapshot, settle = async () => {}, planners = {} } = deps;
    const sleep = (ms) => new Promise((resolve) => win.setTimeout(resolve, ms));

    /** 응답의 연산을 UI로 하는 구간과 내부 경로로 하는 구간으로 가른다. 순서는 그대로다. */
    function segment(mod, operations) {
      const planner = planners[mod];
      // 화면 배치와 글 넣기 명령이 없는 환경(레이아웃 없는 시험용 DOM)에서는 조작할 수 없다: 전부 내부 경로(연출)다.
      if (typeof document.elementFromPoint !== "function" || typeof document.execCommand !== "function") return [{ real: false, ops: operations }];
      const out = [];
      let used = 0;
      for (const op of operations) {
        const units = planner?.supports(op) || 0;
        const real = units > 0 && used + units <= MAX_REAL_UNITS;
        if (real) used += units;
        const last = out.at(-1);
        if (last && last.real === real) last.ops.push(op);
        else out.push({ real, ops: [op] });
      }
      return out;
    }

    function run(mod, operations) {
      const planner = planners[mod];
      const cursor = createCursor();
      const actuator = host.__EASYOFFICE_COPILOT_ACTUATOR__.createActuator({ document, win });
      let abort = null;
      let typing = null;
      const guard = () => {
        if (abort) throw new Aborted(abort);
      };
      const trigger = (kind) => {
        if (abort) return;
        abort = kind;
        typing?.stop();
        cursor.interrupt();
      };
      const onHands = (event) => {
        if (!event.isTrusted || event.target?.closest?.("#eo-copilot-shell")) return;
        if (event.type === "keydown" && ["Shift", "Control", "Alt", "Meta"].includes(event.key)) return;
        trigger("hands");
      };
      const handEvents = ["pointerdown", "mousedown", "keydown"];

      const hold = async (ms) => {
        await sleep(ms);
        guard();
      };

      async function waitFor(check, label) {
        for (let waited = 0; ; waited += POLL_MS) {
          if (check()) return;
          if (waited >= EXPECT_MS) throw new StepFailed("기대한 상태가 되지 않았습니다: " + (label || ""));
          await hold(POLL_MS);
        }
      }

      const need = (step) => {
        const el = step.find?.();
        if (!el || !el.isConnected) throw new StepFailed("대상 요소를 찾지 못했습니다: " + (step.label || ""));
        return el;
      };

      async function pointAt(el, hint) {
        el.scrollIntoView?.({ block: "nearest", inline: "nearest" });
        const spot = actuator.pointIn(el, hint);
        if (!spot) throw new StepFailed("대상 요소가 가려졌거나 화면 밖입니다");
        await cursor.moveTo(spot.x, spot.y);
        guard();
        actuator.moveTo(spot.x, spot.y);
        return spot;
      }

      async function pressOnce(opts) {
        cursor.press(true);
        actuator.down(opts);
        await hold(PRESS_MS);
        actuator.up();
        cursor.press(false);
      }

      async function dragBetween(from, to, onFrame) {
        cursor.press(true);
        actuator.down();
        await cursor.animate(DRAG_MS, (k) => {
          const x = from.x + (to.x - from.x) * k;
          const y = from.y + (to.y - from.y) * k;
          cursor.place(x, y);
          actuator.moveTo(x, y);
          onFrame?.();
        });
        guard();
        actuator.up();
        cursor.press(false);
      }

      const liveSelectionRects = () => {
        const sel = win.getSelection?.();
        return sel?.rangeCount ? Array.from(sel.getRangeAt(0).getClientRects()).map(box) : [];
      };

      function showCaret() {
        const sel = win.getSelection?.();
        const active = document.activeElement;
        if (!sel?.rangeCount || /^(INPUT|TEXTAREA)$/.test(active?.tagName || "")) return cursor.setCaret(null);
        const range = sel.getRangeAt(0).cloneRange();
        range.collapse(false);
        const rect = Array.from(range.getClientRects()).at(-1);
        if (!rect || rect.height <= 0) return cursor.setCaret(null);
        cursor.setCaret({ left: rect.right, top: rect.top, height: rect.height });
        cursor.place(rect.right + 2, rect.top + rect.height / 2);
      }

      async function selectText(step) {
        const el = need(step);
        const length = el.textContent.length;
        const from = step.from ?? 0;
        const to = Math.min(step.to ?? length, length);
        const rects = to > from ? Array.from(textRange(el, from, to).getClientRects()) : [];
        if (rects.length === 0) {
          const spot = await pointAt(el, step.hint);
          cursor.setSelection([box(el.getBoundingClientRect())]);
          await pressOnce();
          const caret = textPoint(el, from);
          win.getSelection().collapse(caret.node, caret.offset);
          return spot;
        }
        const first = rects[0];
        const last = rects.at(-1);
        const start = { x: first.left + 1, y: first.top + first.height / 2 };
        const end = { x: last.right - 1, y: last.top + last.height / 2 };
        const hit = document.elementFromPoint(start.x, start.y);
        if (!hit || !(hit === el || el.contains(hit))) throw new StepFailed("글 위에서 시작할 수 없습니다");
        await cursor.moveTo(start.x, start.y);
        guard();
        actuator.moveTo(start.x, start.y);
        await dragBetween(start, end, () => cursor.setSelection(liveSelectionRects()));
        // 끌어 잡은 범위를 글자 단위로 바로잡는다(끝점 반올림으로 한 글자가 모자라거나 넘칠 수 있다).
        const exact = textRange(el, from, to);
        win.getSelection().setBaseAndExtent(exact.startContainer, exact.startOffset, exact.endContainer, exact.endOffset);
        cursor.setSelection(liveSelectionRects());
        return start;
      }

      async function typeText(step) {
        const target = step.find ? need(step) : document.activeElement;
        if (target && document.activeElement !== target) target.focus?.({ preventScroll: true });
        // 글을 치는 동안 치는 자리(칸·문단·상자)를 선택 강조로 보인다. 요소가 개체 틀 안의 글 영역이면 틀 전체를 보인다.
        const frame = target?.closest?.(".slide-el, .board-node") || target;
        if (frame?.getBoundingClientRect) cursor.setSelection([box(frame.getBoundingClientRect())]);
        let typed = 0;
        let failure = null;
        typing = createTypist({ onBlink: (idle) => cursor.setIdle(idle), ...(step.fast ? { typeSpeed: CELL_TYPE_SPEED, maxTotalMs: CELL_TYPE_MS, sentencePauseMs: 40 } : {}) });
        await typing.type(step.text, (prefix) => {
          try {
            actuator.typeText(prefix.slice(typed));
            typed = prefix.length;
            showCaret();
          } catch (error) {
            failure = error;
            typing.stop();
          }
        });
        typing = null;
        if (failure) throw failure;
        guard();
        cursor.setCaret(null);
      }

      async function runStep(step) {
        guard();
        if (step.act === "click" || step.act === "contextmenu" || step.act === "dblclick") {
          const el = need(step);
          await pointAt(el, step.hint);
          if (step.mark !== false) cursor.setSelection([box(el.getBoundingClientRect())]);
          const opts = { ...step.mods, ...(step.act === "contextmenu" ? { button: 2 } : {}) };
          await pressOnce(opts);
          if (step.act === "dblclick") {
            await hold(GAP_MS);
            await pressOnce(opts);
          }
        } else if (step.act === "select") {
          await selectText(step);
        } else if (step.act === "drag") {
          const el = need(step);
          const from = await pointAt(el, step.hint);
          const to = step.to(el, from);
          await dragBetween(from, to);
        } else if (step.act === "type") {
          await typeText(step);
        } else if (step.act === "key") {
          actuator.press(step.key, step.mods);
          await hold(GAP_MS);
        } else if (step.act === "wait") {
          await hold(step.ms);
        }
        if (step.expect) await waitFor(step.expect, step.label);
        await settle();
        guard();
      }

      async function execute(plan) {
        for (const step of plan.steps) await runStep(step);
        await hold(GAP_MS);
        cursor.setSelection([]);
      }

      // 사용자의 포커스와 글 선택을 기억했다가 끝난 뒤 되돌린다(요소가 이미 없으면 건너뛴다).
      function saveUserState() {
        const sel = win.getSelection?.();
        const range = sel?.rangeCount ? sel.getRangeAt(0).cloneRange() : null;
        const active = document.activeElement;
        return () => {
          if (active?.isConnected && active !== document.body) active.focus?.({ preventScroll: true });
          if (range?.startContainer?.isConnected && range.endContainer.isConnected) {
            const live = win.getSelection?.();
            live?.removeAllRanges();
            live?.addRange(range);
          }
        };
      }

      async function rollback(snap) {
        document.activeElement?.blur?.();
        await restoreSnapshot(snap, undefined, { keepProtections: true });
      }

      const done = (async () => {
        const restoreUser = saveUserState();
        const locations = [];
        let completed = 0;
        let failure = null;
        for (const type of handEvents) document.addEventListener(type, onHands, true);
        try {
          cursor.show();
          const shell = document.getElementById("eo-copilot-shell")?.getBoundingClientRect?.();
          cursor.place(shell ? shell.left + 24 : 640, shell ? shell.top + 120 : 200);
          for (const op of operations) {
            guard();
            const plan = planner.plan(op);
            if (!plan) {
              failure = new StepFailed("연산을 화면 조작으로 바꿀 수 없습니다");
              break;
            }
            const snap = captureSnapshot();
            try {
              await execute(plan);
            } catch (error) {
              // 변경은 연산 단위로 되돌린다. 끝난 연산은 그대로 남는다.
              await rollback(snap);
              failure = error;
              break;
            }
            completed += 1;
            locations.push(...plan.locations);
          }
        } finally {
          for (const type of handEvents) document.removeEventListener(type, onHands, true);
          cursor.hide();
          cursor.destroy();
          actuator.dispose();
          restoreUser();
        }
        const kind = abort || (failure instanceof Aborted ? failure.kind : null);
        // 마지막 실행이 왜 내부 경로로 넘어갔는지 시험과 진단이 읽는다.
        host.__EASYOFFICE_COPILOT_REAL_LAST__ = { completed, aborted: kind, failure: failure && !(failure instanceof Aborted) ? String(failure.message || failure) : null };
        return { completed, locations, rest: operations.slice(completed), aborted: kind, failure };
      })();

      return { done, stop: () => trigger("stop"), discard: () => trigger("discard") };
    }

    return { segment, run };
  }

  host.__EASYOFFICE_COPILOT_REAL__ = Object.freeze({ createRealRunner, MAX_REAL_UNITS, textRange, textPoint });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
