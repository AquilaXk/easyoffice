// SLIDE 문맥과 읽기 도구 `read_slides`(PD-390). 직접 썼다: 문서의 `read_doc_blocks`, 시트 범위 읽기, 베이스 레코드 읽기와 같은 구조(번호 범위 + 상한 + 줄 목록)다.
// 요청에는 슬라이드가 `MAX_CONTEXT_SLIDES`장까지 실리고 활성 슬라이드만 자세하다. 나머지를 읽고 싶을 때 모델이 `read_slides(start, count)`를 부른다.
// 한 슬라이드를 문맥에 싣는 모양(`slideItem`)과 도구가 돌려주는 줄(`slideLine`)은 엔진의 문맥 줄(`copilot/compact.rs` `slide_line`)과 같은 항목을 쓴다.

(function (host) {
  "use strict";

  // 엔진이 프롬프트에 싣는 슬라이드 수(`MAX_PROMPT_SLIDES`)와 덱 상한(`MAX_SLIDES`), `read_slides` 한 번의 상한(`MAX_READ_SLIDES`, `DEFAULT_READ_SLIDES`).
  const MAX_CONTEXT_SLIDES = 64;
  const MAX_DECK_SLIDES = 256;
  const MAX_READ_SLIDES = 20;
  const DEFAULT_READ_SLIDES = 5;
  const MAX_ELEMENTS = 32;
  const MAX_ELEMENT_TEXT = 160;

  // 인수가 없으면 기본값이고, 있는데 음수·소수·문자열이면 거부한다(엔진 `read_slides_call`과 같은 기준).
  function boundedArg(args, key, fallback) {
    const raw = args?.[key];
    if (raw === undefined || raw === null) return fallback;
    if (!Number.isSafeInteger(raw) || raw < 0) throw new Error(`read_slides: ${key}는 0 이상의 정수여야 합니다.`);
    return raw;
  }

  function createSlideReader({ slideDeckState, getWorkspacePanels, getActiveSlideIndex }) {
    const { clipCodePoints } = host.__EASYOFFICE_COPILOT_TEXT__;

    function slideItem(slide, index) {
      const bullets = String(slide?.lower || "").split(/\n+/).map((t) => t.trim()).filter(Boolean);
      const elements = (Array.isArray(slide?.elements) ? slide.elements : []).slice(0, MAX_ELEMENTS).map((el) => ({
        id: String(el.id || ""),
        kind: String(el.kind || ""),
        x: Number(el.x || 0),
        y: Number(el.y || 0),
        w: Number(el.w || 0),
        h: Number(el.h || 0),
        text: clipCodePoints(el.text || el.title || el.alt || "", MAX_ELEMENT_TEXT),
      }));
      const item = { index, title: String(slide?.title || ""), bullets };
      const notes = String(slide?.notes || "");
      if (notes.trim()) item.notes = notes;
      const layout = String(slide?.layout || "titleAndContent");
      if (layout && layout !== "titleAndContent") item.layout = layout;
      if (elements.length > 0) item.elements = elements;
      return item;
    }

    function slideLine(item) {
      const parts = [`  [${item.index}] 제목=${item.title}`];
      if (item.bullets.length > 0) parts.push(`항목=[${item.bullets.map((b) => JSON.stringify(b)).join(", ")}]`);
      if (item.notes) parts.push(`노트=${item.notes}`);
      if (item.layout) parts.push(`레이아웃=${item.layout}`);
      if (item.elements) parts.push(`개체=[${item.elements.map((el) => `${el.id}:${el.kind}(${el.x},${el.y},${el.w}x${el.h}):${el.text}`).join(", ")}]`);
      return parts.join(" | ");
    }

    // 화면에서 방금 바뀐 스타일이 덱에 반영된 뒤의 슬라이드를 읽는다.
    const syncedDeck = () => {
      getWorkspacePanels()?.syncActiveSlideDomStylesToDeck?.();
      return slideDeckState;
    };

    function extractSlideContext() {
      const slides = syncedDeck().slice(0, MAX_CONTEXT_SLIDES).map(slideItem);
      return { activeIndex: getActiveSlideIndex(), aspect: String(slideDeckState[0]?.aspect || "16:9"), slides };
    }

    // `start`번(0부터)부터 `count`장. 덱 끝을 넘으면 있는 데까지만 돌려주고, 상한(`MAX_READ_SLIDES`)을 넘는 `count`는 줄인다.
    function readSlidesTool(args) {
      const start = boundedArg(args, "start", 0);
      const count = boundedArg(args, "count", DEFAULT_READ_SLIDES);
      if (start >= MAX_DECK_SLIDES) throw new Error(`read_slides: start는 ${MAX_DECK_SLIDES}보다 작아야 합니다.`);
      if (count < 1) throw new Error("read_slides: count는 1 이상이어야 합니다.");
      const deck = syncedDeck();
      const end = Math.min(deck.length, start + Math.min(count, MAX_READ_SLIDES));
      const lines = [];
      for (let index = start; index < end; index += 1) lines.push(slideLine(slideItem(deck[index], index)));
      return { totalSlides: deck.length, lines };
    }

    return { extractSlideContext, readSlidesTool };
  }

  host.__EASYOFFICE_COPILOT_SLIDE_READ__ = Object.freeze({ createSlideReader, MAX_READ_SLIDES, DEFAULT_READ_SLIDES });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
