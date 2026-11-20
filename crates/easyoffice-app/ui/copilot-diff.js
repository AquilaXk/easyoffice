// 참조 구현: udecode/plate (MIT, Copyright (c) Ziad Beyens, Dylan Schiemann, Joe Anderson, Felix Feng, 커밋 7f423b58, see THIRD_PARTY_NOTICES.md)
// `packages/platejs/src/ai/react/aiOutputLanding.internal.ts:212-387`(`ALIGN_CELL_LIMIT`, `diffSequences`, `textTokens`, `textEdits`: 단어·공백·기호 토큰 단위 diff와
// 바뀐 구간 묶기)와 `packages/platejs/src/ai/react/AIChatPlugin.ts:449-470`(`reviewSucceeded`, `decideCurrentChange`: 대기 중인 변경 하나를 수락·거절하고 끝나면 비운다)를 옮겼다.
// 변경 목록의 구성은 dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd., `web/src/features/copilot/turn-changes.tsx`)의 턴별 변경 목록을 따랐다.
// 원본은 LCS 표를 줄마다 `Uint32Array`로 만들지만 여기서는 한 장짜리 `Uint32Array`로 만들어 할당을 줄였고,
// 입력 글자 수에 상한을 두어(`MAX_DIFF_CHARS`) 넘으면 앞뒤 공통부분만 걷어 내는 경로로 간다.

(function (host) {
  "use strict";

  // 한 쪽 글이 이만큼을 넘으면 토큰 diff를 하지 않고 앞뒤 공통부분만 걷어 낸다(적대적 입력에서 CPU·메모리를 묶는다).
  const MAX_DIFF_CHARS = 24000;
  // 옮김[plate-diff] 시작
  const ALIGN_CELL_LIMIT = 1_000_000;

  function diffSequences(before, after) {
    const chunks = [];
    const append = (kind, item) => {
      const previous = chunks.at(-1);
      if (previous?.kind === kind) previous.items.push(item);
      else chunks.push({ items: [item], kind });
    };
    const isSubsequence = (source, target) => {
      let sourceIndex = 0;
      for (const item of target) {
        if (source[sourceIndex] === item) sourceIndex += 1;
      }
      return sourceIndex === source.length;
    };

    if (isSubsequence(before, after)) {
      let beforeIndex = 0;
      for (const item of after) {
        if (before[beforeIndex] === item) {
          append("equal", item);
          beforeIndex += 1;
        } else {
          append("insert", item);
        }
      }
      return chunks;
    }
    if (isSubsequence(after, before)) {
      let afterIndex = 0;
      for (const item of before) {
        if (after[afterIndex] === item) {
          append("equal", item);
          afterIndex += 1;
        } else {
          append("delete", item);
        }
      }
      return chunks;
    }
    if ((before.length + 1) * (after.length + 1) <= ALIGN_CELL_LIMIT) {
      const width = after.length + 1;
      const lengths = new Uint32Array((before.length + 1) * width);
      for (let left = before.length - 1; left >= 0; left -= 1) {
        for (let right = after.length - 1; right >= 0; right -= 1) {
          lengths[left * width + right] =
            before[left] === after[right]
              ? lengths[(left + 1) * width + right + 1] + 1
              : Math.max(lengths[(left + 1) * width + right], lengths[left * width + right + 1]);
        }
      }
      let left = 0;
      let right = 0;
      while (left < before.length && right < after.length) {
        if (before[left] === after[right]) {
          append("equal", before[left]);
          left += 1;
          right += 1;
        } else if (lengths[(left + 1) * width + right] >= lengths[left * width + right + 1]) {
          append("delete", before[left]);
          left += 1;
        } else {
          append("insert", after[right]);
          right += 1;
        }
      }
      for (; left < before.length; left += 1) append("delete", before[left]);
      for (; right < after.length; right += 1) append("insert", after[right]);
      return chunks;
    }

    let prefix = 0;
    while (prefix < before.length && prefix < after.length && before[prefix] === after[prefix]) {
      prefix += 1;
    }
    let suffix = 0;
    while (
      suffix < before.length - prefix &&
      suffix < after.length - prefix &&
      before.at(-suffix - 1) === after.at(-suffix - 1)
    ) {
      suffix += 1;
    }
    for (const item of before.slice(0, prefix)) append("equal", item);
    for (const item of before.slice(prefix, before.length - suffix)) append("delete", item);
    for (const item of after.slice(prefix, after.length - suffix)) append("insert", item);
    for (const item of before.slice(before.length - suffix)) append("equal", item);
    return chunks;
  }

  // 단어, 공백 덩어리, 그 밖의 글자 하나씩. 다시 쓰기는 두 단어의 글자가 섞이지 않고 단어 단위 교체로 나온다.
  const textTokens = (text) => text.match(/\s+|[\p{L}\p{N}_]+|[^\s\p{L}\p{N}_]/gu) ?? [];

  /**
   * `before`를 `after`로 바꾸는 편집들. 각각 `before.slice(from, to)`를 `text`로 바꾼다(오프셋은 `before` 기준).
   */
  function textEdits(before, after) {
    const chunks = diffSequences(textTokens(before), textTokens(after));
    const edits = [];
    let offset = 0;
    let index = 0;

    while (index < chunks.length) {
      if (chunks[index].kind === "equal") {
        offset += chunks[index].items.join("").length;
        index += 1;
        continue;
      }
      const from = offset;
      let text = "";
      for (; index < chunks.length && chunks[index].kind !== "equal"; index += 1) {
        const changed = chunks[index].items.join("");
        if (chunks[index].kind === "delete") offset += changed.length;
        else text += changed;
      }
      const previous = edits.at(-1);
      const gap = previous ? before.slice(previous.to, from) : "";
      // 공백만 사이에 둔 두 편집은 하나의 다시 쓰기로 읽힌다.
      if (previous && /^\s+$/.test(gap)) {
        previous.text += gap + text;
        previous.to = offset;
      } else {
        edits.push({ from, text, to: offset });
      }
    }
    return edits;
  }
  // 옮김[plate-diff] 끝

  // 상한을 넘는 글: 앞뒤 공통부분을 걷어 낸 가운데 한 덩어리만 바뀐 것으로 본다.
  function wholeEdit(before, after) {
    let prefix = 0;
    while (prefix < before.length && prefix < after.length && before.charCodeAt(prefix) === after.charCodeAt(prefix)) prefix += 1;
    let suffix = 0;
    while (suffix < before.length - prefix && suffix < after.length - prefix && before.charCodeAt(before.length - 1 - suffix) === after.charCodeAt(after.length - 1 - suffix)) suffix += 1;
    if (before.length === after.length && prefix === before.length) return [];
    // 서로게이트 쌍 가운데서 자르지 않는다.
    const unsafe = (s, at) => at > 0 && at < s.length && s.charCodeAt(at - 1) >= 0xd800 && s.charCodeAt(at - 1) <= 0xdbff && s.charCodeAt(at) >= 0xdc00 && s.charCodeAt(at) <= 0xdfff;
    while (prefix > 0 && (unsafe(before, prefix) || unsafe(after, prefix))) prefix -= 1;
    while (suffix > 0 && (unsafe(before, before.length - suffix) || unsafe(after, after.length - suffix))) suffix -= 1;
    return [{ from: prefix, text: after.slice(prefix, after.length - suffix), to: before.length - suffix }];
  }

  /** `textEdits`에 글자 수 상한을 둔 것. */
  function boundedEdits(before, after) {
    if (before.length > MAX_DIFF_CHARS || after.length > MAX_DIFF_CHARS) return wholeEdit(before, after);
    return textEdits(before, after);
  }

  /**
   * 화면에 그릴 조각: 같은 글, 지워지는 글, 새로 들어가는 글이 순서대로 이어진다. `equal`과 `delete`를 이으면 `before`, `equal`과 `insert`를 이으면 `after`다.
   */
  function diffSegments(before, after) {
    const edits = boundedEdits(String(before ?? ""), String(after ?? ""));
    const source = String(before ?? "");
    const segments = [];
    let cursor = 0;
    for (const edit of edits) {
      if (edit.from > cursor) segments.push({ kind: "equal", text: source.slice(cursor, edit.from) });
      if (edit.to > edit.from) segments.push({ kind: "delete", text: source.slice(edit.from, edit.to) });
      if (edit.text) segments.push({ kind: "insert", text: edit.text });
      cursor = edit.to;
    }
    if (cursor < source.length) segments.push({ kind: "equal", text: source.slice(cursor) });
    return segments;
  }

  // 옮김[plate-review] 시작
  const reviewSucceeded = (result) => result?.status === "applied" || result?.status === "unchanged";

  /**
   * 대기 중인 변경 하나를 수락·거절한다. `accept(change)`는 적용하고, `withdraw(change)`는 미리보기를 걷어 문서를 그대로 둔다.
   * 둘 다 `{ status: "applied" | "unchanged" | "failed" }`를 돌려주며, 성공하면 대기 변경을 비운다.
   */
  function createChangeReview({ accept, withdraw }) {
    let pending = null;
    const decideCurrentChange = async (action) => {
      const change = pending;
      if (!change) return null;
      const result = action === "reject" ? await withdraw(change) : await accept(change);
      if (reviewSucceeded(result)) pending = null;
      return result;
    };
    return {
      set(change) {
        pending = change;
      },
      get pending() {
        return pending;
      },
      accept: () => decideCurrentChange("accept"),
      reject: () => decideCurrentChange("reject"),
    };
  }
  // 옮김[plate-review] 끝

  host.__EASYOFFICE_COPILOT_DIFF__ = Object.freeze({
    MAX_DIFF_CHARS, textTokens, textEdits, boundedEdits, diffSegments, createChangeReview,
  });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
