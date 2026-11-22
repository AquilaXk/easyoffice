// 참조 구현: rrweb-io/rrweb (MIT, Copyright (c) 2018 Contributors, 커밋 d6926830, see THIRD_PARTY_NOTICES.md)
// `packages/rrweb/src/replay/timer.ts:8-123`(`Timer`: 지연 시간(`delay`)순으로 정렬한 동작 목록을 프레임마다 시계와 견줘 때가 된 것부터 실행하는 재생기,
// 이진 탐색으로 끼워 넣는 `addAction`, `start`, `rafCheck`, `clear`)을 옮겼다.
// 참조 구현: motiondivision/motion (MIT, Copyright (c) 2024 Motion B.V., 커밋 1b374802)
// `packages/framer-motion/src/animation/sequence/utils/calc-time.ts:21-39`(`calcNextTime`: 단계의 `at`을 절대 숫자, `+N`·`-N`(앞 단계 끝 기준), `<`·`<N`(앞 단계 시작 기준)으로 읽는다)과
// `sequence/create.ts:58-118, 413-415`(`resolveSequence`: `at`이 없으면 앞 단계가 끝난 때에 이어 붙이고 `prevTime`·`currentTime`을 이어 가는 순서)를 옮겼다.
// 원본의 `speed`·실시간 모드·`isActive`, `setSpeed`, `addDelay`(녹화 이벤트의 지연 계산)와 라벨, 중첩 시퀀스는 쓸 데가 없어 옮기지 않았다.
// 코파일럿 연출은 한 번에 한 장면만 재생하므로 장면 재생기(`createScenePlayer`)는 직접 짰다: 장면의 단계들을 위 시간표로 풀어 타이머에 올리고,
// 단계가 시작한 애니메이션(약속)이 모두 끝나야 장면이 끝나게 하며, 멈추면 타이머를 비우고 `false`로 푼다.

(function (host) {
  "use strict";

  // 옮김[rrweb-timer] 시작
  /** 동작 목록 재생기. `now`·`requestAnimationFrame`·`cancelAnimationFrame`은 시계와 프레임 요청(시험에서 갈아 끼운다). */
  function createTimer({ now, requestAnimationFrame, cancelAnimationFrame }) {
    const actions = [];
    let timeOffset = 0;
    // 다음 프레임 번호. 목록이 비어 쉬는 중이면 `true`(원본의 "was active"), 서 있으면 `null`.
    let raf = null;
    let lastTimestamp = 0;

    // 같은 지연의 동작이 이미 있으면 그 뒤에 끼운다(이진 탐색).
    function findActionIndex(action) {
      let start = 0;
      let end = actions.length - 1;
      while (start <= end) {
        const mid = Math.floor((start + end) / 2);
        if (actions[mid].delay < action.delay) start = mid + 1;
        else if (actions[mid].delay > action.delay) end = mid - 1;
        else return mid + 1;
      }
      return start;
    }

    function addAction(action) {
      const rafWasActive = raf === true;
      if (!actions.length || actions[actions.length - 1].delay <= action.delay) {
        actions.push(action);
      } else {
        actions.splice(findActionIndex(action), 0, action);
      }
      if (rafWasActive) raf = requestAnimationFrame(rafCheck);
    }

    function start() {
      timeOffset = 0;
      lastTimestamp = now();
      raf = requestAnimationFrame(rafCheck);
    }

    function rafCheck() {
      const time = now();
      timeOffset += time - lastTimestamp;
      lastTimestamp = time;
      while (actions.length) {
        const action = actions[0];
        if (timeOffset >= action.delay) {
          actions.shift();
          action.doAction();
        } else {
          break;
        }
      }
      // 동작이 `clear`를 불렀으면(raf가 `null`) 다시 걸지 않는다.
      if (raf === null) return;
      raf = actions.length > 0 ? requestAnimationFrame(rafCheck) : true;
    }

    function clear() {
      if (raf) {
        if (raf !== true) cancelAnimationFrame(raf);
        raf = null;
      }
      actions.length = 0;
    }

    return { addAction, start, clear };
  }
  // 옮김[rrweb-timer] 끝

  // 옮김[motion-sequence] 시작
  /** `at`을 절대 시각(ms)으로 읽는다. `current`는 앞 단계가 끝난 때, `prev`는 앞 단계가 시작한 때. */
  function calcNextTime(current, next, prev) {
    if (typeof next === "number") return next;
    if (next.startsWith("-") || next.startsWith("+")) return Math.max(0, current + parseFloat(next));
    if (next === "<") return prev;
    if (next.startsWith("<")) return Math.max(0, prev + parseFloat(next.slice(1)));
    return current;
  }

  /** 단계 `{ at?, duration?, run? }`들의 시작 시각을 구한다. `at`이 없으면 앞 단계가 끝난 때에 이어 붙는다. */
  function resolveSteps(steps) {
    let prevTime = 0;
    let currentTime = 0;
    let end = 0;
    const timed = steps.map((step) => {
      if (step.at !== undefined) currentTime = calcNextTime(currentTime, step.at, prevTime);
      const start = currentTime;
      prevTime = currentTime;
      currentTime += step.duration ?? 0;
      end = Math.max(end, currentTime);
      return { start, run: step.run };
    });
    return { timed, end };
  }
  // 옮김[motion-sequence] 끝

  /**
   * 장면 재생기. `play(steps)`는 장면이 끝나면 `true`로, `clear()`가 끊으면 `false`로 풀린다.
   * 단계의 `run()`이 약속을 돌려주면(애니메이션) 그것도 끝나야 장면이 끝나고, 거절되면 장면이 그 오류로 거절된다.
   */
  function createScenePlayer(timing) {
    let abort = null;

    function play(steps) {
      return new Promise((resolve, reject) => {
        const timer = createTimer(timing);
        const tasks = [];
        const settle = (done, error) => {
          if (abort !== stopScene) return;
          abort = null;
          timer.clear();
          if (error) reject(error);
          else resolve(done);
        };
        const stopScene = () => settle(false);
        abort?.();
        abort = stopScene;
        const { timed, end } = resolveSteps(steps);
        for (const { start, run } of timed) {
          if (!run) continue;
          timer.addAction({
            delay: start,
            doAction: () => {
              // 동기로 던진 오류도 약속의 거절로 모은다.
              const task = new Promise((ok) => ok(run()));
              task.catch((error) => settle(false, error));
              tasks.push(task);
            },
          });
        }
        timer.addAction({ delay: end, doAction: () => Promise.all(tasks).then(() => settle(true), () => {}) });
        timer.start();
      });
    }

    return { play, clear: () => abort?.() };
  }

  host.__EASYOFFICE_COPILOT_TIMELINE__ = Object.freeze({ createTimer, calcNextTime, resolveSteps, createScenePlayer });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
