(function (host) {
  "use strict";

  // Find and replace core (DOC and SHEET share it). It knows nothing about the DOM: a source hands
  // over blocks (a paragraph, a table cell, a sheet cell) as `{ id, text }`, and the index keeps the
  // matches per block. An edit changes one block's text, and only that block is searched again; a
  // new query searches every block once. The text-node offset table and the range math are plain
  // functions on strings, so the whole path is testable without a browser.

  const MAX_QUERY_CHARS = 256;
  // Matches kept for one query. Past it the count reads "N+" and the rest is not indexed (fail-closed).
  const MAX_MATCHES = 100000;
  // The time budget is checked between blocks: a search that runs over it stops there and the next call continues.
  const SEARCH_BUDGET_MS = 1500;
  const MSG_TOO_LONG = "찾을 내용이 너무 깁니다 (최대 " + MAX_QUERY_CHARS + "자)";

  function escapeRegExp(text) {
    return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  }

  // `{ ok: true, re, key }` or `{ ok: false, error }`. An empty query is not an error: it matches nothing.
  function buildMatcher({ query, matchCase = false }) {
    const text = String(query ?? "").normalize("NFC");
    if (text === "") return { ok: true, empty: true, key: "" };
    if (text.length > MAX_QUERY_CHARS) return { ok: false, error: MSG_TOO_LONG };
    return { ok: true, re: new RegExp(escapeRegExp(text), matchCase ? "gu" : "giu"), key: (matchCase ? "C" : "c") + text };
  }

  // Matches of `text` as `{ start, end }`. `limit` caps how many are returned.
  function matchesOf(text, matcher, limit = MAX_MATCHES) {
    const out = [];
    if (matcher.empty || text === "") return out;
    const re = matcher.re;
    re.lastIndex = 0;
    let m = re.exec(text);
    while (m !== null && out.length < limit) {
      out.push({ start: m.index, end: m.index + m[0].length });
      m = re.exec(text);
    }
    return out;
  }

  // ---- text-node offset table ---------------------------------------------------------------

  // Start offsets of consecutive segments (the text nodes of one block).
  function segmentStarts(segments) {
    const starts = new Array(segments.length);
    let at = 0;
    for (let i = 0; i < segments.length; i += 1) {
      starts[i] = at;
      at += segments[i].length;
    }
    return starts;
  }

  // The segment holding `offset`. A boundary belongs to the segment that starts there, except for
  // `forEnd` (a range end), which belongs to the segment that ends there.
  function segmentAt(starts, offset, forEnd = false) {
    let lo = 0;
    let hi = starts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      const before = forEnd ? starts[mid] < offset : starts[mid] <= offset;
      if (before) lo = mid;
      else hi = mid - 1;
    }
    return lo;
  }

  // Applies non-overlapping edits `{ start, end, value }` (offsets in the joined text) to the segments
  // and returns the new segment strings. Each replacement goes into the segment where its range starts
  // (so it keeps that run's formatting); the covered text in the following segments is cut. Edits run
  // from the last to the first so the offsets of the earlier ones stay valid.
  function applyEdits(segments, edits) {
    const out = segments.slice();
    if (edits.length === 0 || segments.length === 0) return out;
    const starts = segmentStarts(segments);
    for (let e = edits.length - 1; e >= 0; e -= 1) {
      const { start, end, value } = edits[e];
      const first = segmentAt(starts, start);
      const last = Math.max(first, segmentAt(starts, end, true));
      const head = out[first].slice(0, start - starts[first]);
      if (first === last) {
        out[first] = head + value + out[first].slice(end - starts[first]);
        continue;
      }
      out[first] = head + value;
      for (let s = first + 1; s < last; s += 1) out[s] = "";
      out[last] = out[last].slice(end - starts[last]);
    }
    return out;
  }

  // ---- per-block match index ----------------------------------------------------------------

  function createFindIndex(now = () => Date.now()) {
    let ids = [];
    let texts = [];
    let matches = []; // per block: array of matches, or null when stale
    let slotOf = new Map();
    let matcherKey = null;
    let prefix = [0]; // prefix[i] = matches before block i

    // Replaces the block list. A block that keeps its id and text keeps its matches.
    function setBlocks(blocks) {
      const nextIds = new Array(blocks.length);
      const nextTexts = new Array(blocks.length);
      const nextMatches = new Array(blocks.length);
      const nextSlots = new Map();
      for (let i = 0; i < blocks.length; i += 1) {
        const { id, text } = blocks[i];
        const was = slotOf.get(id);
        nextIds[i] = id;
        nextTexts[i] = text;
        nextMatches[i] = was !== undefined && texts[was] === text ? matches[was] : null;
        nextSlots.set(id, i);
      }
      ids = nextIds;
      texts = nextTexts;
      matches = nextMatches;
      slotOf = nextSlots;
    }

    // One block's text changed (typing): only that block is searched again.
    function updateBlock(id, text) {
      const i = slotOf.get(id);
      if (i === undefined || texts[i] === text) return;
      texts[i] = text;
      matches[i] = null;
    }

    // Brings every stale block up to date for `matcher` and rebuilds the prefix sums; returns
    // `{ total, complete }` (`complete` is false when the match cap or the time budget stopped it).
    function search(matcher) {
      if (matcher.key !== matcherKey) {
        matcherKey = matcher.key;
        matches = matches.map(() => null);
      }
      const deadline = now() + SEARCH_BUDGET_MS;
      let total = 0;
      let complete = true;
      prefix = new Array(matches.length + 1);
      for (let i = 0; i < matches.length; i += 1) {
        prefix[i] = total;
        if (matches[i] === null) {
          if (total >= MAX_MATCHES || now() > deadline) {
            complete = false;
            continue;
          }
          matches[i] = matchesOf(texts[i], matcher, MAX_MATCHES - total);
        }
        total += matches[i].length;
      }
      prefix[matches.length] = total;
      return { total, complete: complete && total < MAX_MATCHES };
    }

    function total() {
      return prefix[prefix.length - 1];
    }

    // The n-th match overall as `{ index, id, start, end, m }`, or null.
    function at(n) {
      if (n < 0 || n >= total()) return null;
      let lo = 0;
      let hi = matches.length - 1;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (prefix[mid] <= n) lo = mid;
        else hi = mid - 1;
      }
      const hit = matches[lo]?.[n - prefix[lo]];
      return hit ? { index: n, id: ids[lo], start: hit.start, end: hit.end } : null;
    }

    // Overall index of the first match at or after (`id`, `offset`) going forward, or of the last one
    // before it going back. Wraps around; -1 when there is no match.
    function neighbour(id, offset, forward) {
      const count = total();
      const block = slotOf.get(id);
      if (count === 0 || block === undefined) return count === 0 ? -1 : (forward ? 0 : count - 1);
      const inBlock = matches[block] || [];
      let before = 0;
      while (before < inBlock.length && inBlock[before].start < offset) before += 1;
      return forward ? (prefix[block] + before) % count : (prefix[block] + before - 1 + count) % count;
    }

    function matchesInBlock(id) {
      const i = slotOf.get(id);
      return i === undefined ? [] : matches[i] || [];
    }

    function blockText(id) {
      const i = slotOf.get(id);
      return i === undefined ? "" : texts[i];
    }

    return { setBlocks, updateBlock, search, total, at, neighbour, matchesInBlock, blockText, blockIds: () => ids };
  }

  host.__EASYOFFICE_FIND_CORE__ = {
    MAX_QUERY_CHARS, MAX_MATCHES, SEARCH_BUDGET_MS,
    buildMatcher, matchesOf,
    segmentStarts, segmentAt, applyEdits, createFindIndex,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
