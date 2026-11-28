// 직접 썼다(PD-387): 내장 한글 편집기(`hwp-studio` iframe)의 문단 변경에 포인터가 하는 몸짓이다. 포인터·몸짓과 되돌리기 도구는 `copilot-perform.js`가 `session`으로 건넨다.
// iframe 안의 DOM은 셸이 직접 볼 수 없다. 대상 문단의 화면 자리는 편집기 쪽 읽기 전용 조회(`getCaretRect`, `copilot-bridge.ts`와 `main.ts`)로 받는다:
// 셸이 그 문단으로 이동시킨 뒤 편집기가 캐럿 상자를 돌려주면 `hwpApi.paragraphRect`가 iframe 위치를 더해 셸 화면 좌표로 건넨다.
// 글을 한 자씩 치는 흉내는 내지 않는다(편집기 문서를 임시로 고칠 수 없다): 문단으로 가서 선택 표시를 하고 누른 뒤 적용한다.
//
// 연출은 문서를 바꾸지 않는다. 문단은 연출이 끝난 뒤 기존 적용 경로(`applyHwpOperations`) 한 번에 바뀐다. 멈추면 끝낸 문단까지만 연산으로 잘라 적용한다.
// 비임베드(옛 편집기) 한글은 문서 편집기 DOM을 쓰므로 이 파일이 아니라 문서 연출(`copilot-perform.js`)을 탄다.

(function (host) {
  "use strict";

  // 한 응답에서 연출하는 한글 문단 수 상한. 넘는 것은 연출 없이 마지막에 한꺼번에 적용한다.
  const MAX_HWP_STEPS = 12;
  const CLICK_MS = 160;
  const HOLD_MS = 220;
  const MARK_PAD = 4;

  const listOf = (value) => (Array.isArray(value) ? value : []);
  const isIndex = (value) => Number.isInteger(value) && value >= 0;

  function createHwpPerformance(deps) {
    const { hwpApi } = deps;

    // ---- 목표: 연산에서 문단 단계를 순서대로 뽑는다. `index`는 문단 번호이고 `"end"`는 문서 끝이다.

    function collect(op, opIndex, kind) {
      const base = { owner: "hwp", opIndex, kind: "hwp-paragraph", entry: 0, text: "" };
      if (kind === "doc.replace_span") return isIndex(Number(op?.index)) ? [{ ...base, index: Number(op.index) }] : [];
      if (kind === "doc.append") return [{ ...base, index: "end" }];
      if (kind === "doc.insert_below") return listOf(op.inserts).map((entry, at) => ({ ...base, entry: at, index: Number(entry?.after) })).filter((t) => isIndex(t.index));
      if (kind !== "doc.replace_paragraphs") return [];
      if (Array.isArray(op.replacements)) return op.replacements.map((rep, entry) => ({ ...base, entry, index: Number(rep?.index) })).filter((t) => isIndex(t.index));
      return [{ ...base, index: isIndex(op.startIndex) ? op.startIndex : 0, endIndex: isIndex(op.endIndex) ? op.endIndex : undefined }];
    }

    // ---- 자르기: 멈춘 곳까지 끝낸 문단만 남긴다. 남길 것이 없으면 `null`.

    function cut(op, kind, target) {
      const key = kind === "doc.replace_paragraphs" && Array.isArray(op.replacements) ? "replacements" : kind === "doc.insert_below" ? "inserts" : null;
      if (!key) return null;
      const kept = listOf(op[key]).slice(0, target.entry);
      return kept.length > 0 ? { ...op, [key]: kept } : null;
    }

    // ---- 연출

    const inside = (rect, frame) => rect.right > frame.left && rect.left < frame.right && rect.bottom > frame.top && rect.top < frame.bottom;

    function createDrivers(session) {
      const { cursor } = session;

      async function askRect(index) {
        try {
          return (await hwpApi.paragraphRect(index)) || null;
        } catch {
          return null;
        }
      }

      async function pointParagraph(target) {
        const frame = session.visibleRect(hwpApi.frame());
        if (!frame) return { typed: 0 };
        const asked = await askRect(target.index);
        if (session.stopped()) return { typed: 0 };
        const rect = asked && asked.width > 0 && asked.height > 0 ? { ...asked, right: asked.left + asked.width, bottom: asked.top + asked.height } : null;
        if (rect && inside(rect, frame)) {
          cursor.setSelection([rect]);
          await session.moveTo(rect.left + MARK_PAD, rect.top + rect.height / 2);
        } else {
          // 문단 자리를 알 수 없으면 편집기 가운데로 가서 누른다.
          await session.moveTo(frame.left + frame.width / 2, frame.top + frame.height / 2);
        }
        if (!session.stopped()) await session.click(CLICK_MS);
        if (!session.stopped()) await session.hold(HOLD_MS);
        cursor.setSelection([]);
        return { typed: 0 };
      }

      return { "hwp-paragraph": pointParagraph };
    }

    return { collect, cut, createDrivers, MAX_HWP_STEPS };
  }

  host.__EASYOFFICE_COPILOT_PERFORM_HWP__ = Object.freeze({ createHwpPerformance, MAX_HWP_STEPS });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
