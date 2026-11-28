// 참조 구현: danny-avila/LibreChat (MIT, Copyright (c) 2026 LibreChat, see THIRD_PARTY_NOTICES.md)
// `packages/data-provider/src/feedback.ts` 3-14(평가 두 가지와 이유 키 목록), 103-105(`getTagsForRating`), 110-125(이유가 평가에 맞는지 검사),
// 135-145(`toMinimalFeedback`, 평가와 이유만 남긴 기록). 같은 구조를 바닐라 JS로 옮겼다. 저장은 대화 기록(`msg`)에만 하고 어디로도 보내지 않는다.
// 바꾼 점: 이유는 싫어요에만 있고(4가지, 글자는 한국어 고정 칩), 좋아요는 이유 없이 "방식 유지" 신호다. 평가 줄은 다음 질문 끝에 한 줄로 붙는다.

(function (host) {
  "use strict";

  const RATINGS = Object.freeze(["up", "down"]);

  // 이유 키는 LibreChat의 `inaccurate`·`not_matched`·`other`와 같고 `too_long`은 이 앱의 것이다.
  const FEEDBACK_TAGS = Object.freeze([
    { key: "inaccurate", label: "부정확함", rating: "down" },
    { key: "too_long", label: "너무 김", rating: "down" },
    { key: "not_matched", label: "지시와 다름", rating: "down" },
    { key: "other", label: "기타", rating: "down" },
  ]);

  const getTagsForRating = (rating) => FEEDBACK_TAGS.filter((tag) => tag.rating === rating);
  const getTagByKey = (key) => FEEDBACK_TAGS.find((tag) => tag.key === key);

  /** 같은 평가를 다시 누르면 지우고, 다른 평가로 바꾸면 이유를 비운다. 알 수 없는 평가는 무시한다. */
  function rate(msg, rating) {
    if (!RATINGS.includes(rating)) return;
    msg.feedback = msg.feedback === rating ? null : rating;
    msg.feedbackTag = "";
  }

  /** 이유는 지금 평가에 맞는 것만 받는다(`feedback.ts` 116-120). 같은 이유를 다시 누르면 지운다. */
  function pickTag(msg, key) {
    if (!getTagsForRating(msg.feedback).some((tag) => tag.key === key)) return;
    msg.feedbackTag = msg.feedbackTag === key ? "" : key;
  }

  /** 다음 질문 끝에 붙는 평가 한 줄. 평가가 없으면 빈 글. */
  function feedbackNote(answer) {
    if (answer?.feedback === "up") return "[직전 답변 평가: 좋아요 - 이 방식 유지]";
    if (answer?.feedback !== "down") return "";
    const tag = getTagByKey(answer.feedbackTag);
    return "[직전 답변 평가: 싫어요" + (tag ? " - " + tag.label : "") + "]";
  }

  host.__EASYOFFICE_COPILOT_FEEDBACK__ = { RATINGS, FEEDBACK_TAGS, getTagsForRating, getTagByKey, rate, pickTag, feedbackNote };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
