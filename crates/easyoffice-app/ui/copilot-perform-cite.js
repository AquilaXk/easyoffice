// 직접 썼다(PD-387): PDF 답변이 인용한 쪽으로 포인터가 가서 쪽을 강조한다. PDF에는 편집 연산이 없어(질문 전용) 문서를 바꾸는 일은 없다.
// 인용은 답변의 구조화된 위치 칩(`locations`의 `{ module: "PDF", page }`, `copilot-session-anchor.js`의 `normalizeLocations`)을 쓴다. 본문 글에서 쪽 번호를 찾아 읽지 않는다.
// 포인터·몸짓과 되돌리기 도구는 `copilot-perform.js`가 `session`으로 건넨다. 연산 자리에는 `{ op: "pdf.point", page }`를 넣어 같은 연출 틀을 탄다.

(function (host) {
  "use strict";

  // 한 답변에서 가리키는 쪽 수 상한. 넘는 인용은 위치 칩으로만 남는다.
  const MAX_PDF_PAGES = 3;
  const HOLD_MS = 600;
  // 쪽 강조는 쪽 카드 안쪽으로 이만큼 들어온 사각형이다.
  const INSET = 6;
  const POINT_KIND = "pdf.point";

  function createPdfPerformance(deps) {
    const { document, goToPdfPage } = deps;

    /** 인용한 쪽을 한 번씩, 나온 순서대로. */
    function collect(op, opIndex, kind, seen) {
      const page = Number(op?.page);
      if (kind !== POINT_KIND || !Number.isInteger(page) || page < 1 || seen.has(page)) return [];
      seen.add(page);
      return [{ owner: "pdf", opIndex, kind: "pdf-page", page, text: "" }];
    }

    const cut = () => null;

    function createDrivers(session) {
      const { cursor } = session;

      async function pointPage(target) {
        goToPdfPage?.(target.page);
        const card = session.visibleRect(document.getElementById("pdf-page-card"));
        if (!card) return { typed: 0 };
        await session.moveTo(card.left + card.width / 2, card.top + card.height / 2);
        if (session.stopped()) return { typed: 0 };
        cursor.setSelection([{ left: card.left + INSET, top: card.top + INSET, width: card.width - INSET * 2, height: card.height - INSET * 2 }]);
        await session.hold(HOLD_MS);
        cursor.setSelection([]);
        return { typed: 0 };
      }

      return { "pdf-page": pointPage };
    }

    return { collect, cut, createDrivers, MAX_PDF_PAGES };
  }

  host.__EASYOFFICE_COPILOT_PERFORM_PDF__ = Object.freeze({ createPdfPerformance, MAX_PDF_PAGES, POINT_KIND });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
