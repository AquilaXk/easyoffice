// 직접 썼다(PD-392): 한 턴(응답 하나의 연산들)을 실제 조작과 내부 경로로 나눠 적용하는 흐름이다. `copilot-actions.js`의 `applyOperationsAnimated`가 부른다.
//
// 연산을 순서대로 구간으로 가른다(`real.segment`): 화면 조작으로 할 수 있는 구간은 `copilot-real.js`가 하고, 나머지는 기존 연출(`copilot-perform.js`) 뒤 내부 경로로 적용한다.
// 화면 조작이 실패하거나 사용자가 손을 대면 하던 연산은 되돌려지고, 그 연산부터 끝까지는 연출 없이 내부 경로로 적용한다(최종 결과는 내부 경로만 썼을 때와 같다).
// 중지 단추는 하던 연산을 되돌리고 거기서 끝낸다. 새 대화(버리기)는 턴 전체를 처음 스냅숏으로 되돌린다.
// 되돌리기: 턴이 시작될 때 뜬 스냅숏이 한 단계다. 구간 안에서 앱의 처리기가 남기는 기록은 역사 묶음이 삼키고(`beginHistoryGroup`), 끝에서 한 번 남긴다.

(function (host) {
  "use strict";

  function createRealTurn(deps) {
    const { real, performer, applyOperations, restoreSnapshot, beginGroup, endGroup, commit, afterReal, setActiveRun, endRun } = deps;

    async function apply(mod, mode, operations, turnId, segments, beforeSnap) {
      const state = { stopped: false, discarded: false };
      let current = null;
      let applied = false;
      let interrupted = false;
      let handed = false;
      const locations = [];
      setActiveRun({
        stop() {
          state.stopped = true;
          current?.stop();
        },
        discard() {
          state.discarded = true;
          current?.discard();
        },
      });
      const internal = async (ops) => {
        const res = await applyOperations(mod, mode, ops, turnId, { beforeSnap, noCommit: true });
        locations.push(...res.locations);
        applied = applied || res.applied;
      };
      beginGroup();
      try {
        for (let i = 0; i < segments.length && !state.stopped && !state.discarded; i += 1) {
          const { real: byHand, ops } = segments[i];
          if (byHand) {
            current = real.run(mod, ops);
            const res = await current.done;
            current = null;
            locations.push(...res.locations);
            applied = applied || res.completed > 0;
            handed = handed || res.completed > 0;
            if (res.aborted === "stop") interrupted = true;
            if (res.aborted === "discard") state.discarded = true;
            if (res.aborted === "discard" || res.aborted === "stop") break;
            // 실패하거나 손을 댄 연산부터 끝까지는 내부 경로로 적용한다.
            if (res.rest.length > 0) {
              await internal([...res.rest, ...segments.slice(i + 1).flatMap((segment) => segment.ops)]);
              break;
            }
          } else {
            current = performer.perform(mod, ops);
            const res = await current.done;
            current = null;
            if (res.discarded) state.discarded = true;
            else if (res.operations.length > 0) await internal(res.operations);
            if (res.interrupted) interrupted = true;
            if (res.interrupted || res.discarded) break;
          }
        }
        if (state.stopped) interrupted = true;
        if (handed) afterReal(mod);
      } catch (error) {
        endGroup();
        endRun();
        throw error;
      }
      endGroup();
      try {
        if (state.discarded && applied) {
          await restoreSnapshot(beforeSnap, undefined, { keepProtections: true });
          return { applied: false, locations: [], interrupted: true, discarded: true };
        }
        if (state.discarded) return { applied: false, locations: [], interrupted: true, discarded: true };
        if (applied) commit(beforeSnap, turnId);
        return { applied, locations, interrupted };
      } finally {
        endRun();
      }
    }

    return { apply };
  }

  // 모듈별 계획 표(`copilot-real-*.js`)가 있는 모듈: [전역 이름, 만드는 함수].
  const PLAN_PARTS = {
    DOC: ["__EASYOFFICE_COPILOT_REAL_DOC__", "createDocPlanner"],
    SHEET: ["__EASYOFFICE_COPILOT_REAL_SHEET__", "createSheetPlanner"],
    BASE: ["__EASYOFFICE_COPILOT_REAL_EXT__", "createBasePlanner"],
    BOARD: ["__EASYOFFICE_COPILOT_REAL_EXT__", "createBoardPlanner"],
    SLIDE: ["__EASYOFFICE_COPILOT_REAL_SLIDE__", "createSlidePlanner"],
  };

  /** `copilot-actions.js`가 쓰는 한 덩어리: 구간 가르기(`segment`)와 한 턴 적용(`apply`). */
  function createRealPerformance(deps) {
    const { document, performer, createCursor, createTypist = host.__EASYOFFICE_COPILOT_TYPING__?.createTypist, isEmbedHwp } = deps;
    const planners = {};
    for (const [mod, [key, factory]] of Object.entries(PLAN_PARTS)) {
      if (host[key]) planners[mod] = host[key][factory](deps);
    }
    // 내장 한글 편집기에는 문서 편집 영역이 없다: 옛 편집기 한글만 문서 계획을 쓴다.
    Object.defineProperty(planners, "HWP", { enumerable: true, get: () => (isEmbedHwp() ? null : planners.DOC) });
    const real = host.__EASYOFFICE_COPILOT_REAL__.createRealRunner({
      document,
      win: host,
      createCursor,
      createTypist,
      captureSnapshot: () => deps.captureWorkspaceSnapshot(false),
      restoreSnapshot: deps.restoreWorkspaceSnapshot,
      settle: async () => deps.getSheetController()?.drainInFlightSheetWork?.(),
      planners,
    });
    const turn = createRealTurn({
      real,
      performer,
      applyOperations: deps.applyOperations,
      restoreSnapshot: deps.restoreWorkspaceSnapshot,
      beginGroup: () => deps.beginHistoryGroup?.(),
      endGroup: () => deps.endHistoryGroup?.(),
      commit: deps.commit,
      // 화면 조작이 바꾼 문서의 뒷정리(내부 경로의 `touchedDoc`와 같다).
      afterReal: (mod) => {
        if (mod !== "DOC" && mod !== "HWP") return;
        host.__EASYOFFICE_DOC_NOTES_API__?.syncAll?.();
        const panels = deps.getWorkspacePanels();
        panels?.saveActiveDocTabState?.();
        panels?.computeSmartStats?.();
        panels?.renderModuleOutline?.(mod);
      },
      setActiveRun: deps.setActiveRun,
      endRun: deps.endRun,
    });
    return { segment: real.segment, apply: turn.apply };
  }

  host.__EASYOFFICE_COPILOT_REAL_TURN__ = Object.freeze({ createRealTurn, createRealPerformance });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
