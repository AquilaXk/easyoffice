// 참조 구현: cline/cline (Apache-2.0)의 중복 읽기 도구 결과 SHA-256 영수증 치환·실패 요약 패턴 및
// tribgames/mixdog (Apache-2.0, d5ab088efcc2680f0bb4c43448255ca1653fb3c6) 청크 단위 대화 이력 퇴출 패턴.

(function (host) {
  "use strict";

  const DEFAULT_MAX_HISTORY = 12;
  const DEFAULT_CHUNK_SIZE = 4;
  const DEFAULT_MAX_HISTORY_BYTES = 48 * 1024;
  const DEFAULT_MAX_LOOP_BYTES = 24 * 1024;
  const MAX_HISTORY_ITEM_BYTES = 16 * 1024;
  const COMPACT_RECEIPT_THRESHOLD = 1024;

  const SHA256_K = new Uint32Array([
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
  ]);

  function utf8BytesOf(text) {
    const str = String(text ?? "");
    if (typeof host.TextEncoder === "function") {
      return new host.TextEncoder().encode(str);
    }
    const bytes = [];
    for (let i = 0; i < str.length; i += 1) {
      let cp = str.charCodeAt(i);
      if (cp < 0x80) {
        bytes.push(cp);
      } else if (cp < 0x800) {
        bytes.push(0xc0 | (cp >> 6), 0x80 | (cp & 0x3f));
      } else if (cp >= 0xd800 && cp <= 0xdbff && i + 1 < str.length) {
        const lo = str.charCodeAt(i + 1);
        if (lo >= 0xdc00 && lo <= 0xdfff) {
          cp = 0x10000 + ((cp - 0xd800) << 10) + (lo - 0xdc00);
          i += 1;
          bytes.push(
            0xf0 | (cp >> 18),
            0x80 | ((cp >> 12) & 0x3f),
            0x80 | ((cp >> 6) & 0x3f),
            0x80 | (cp & 0x3f),
          );
          continue;
        }
        bytes.push(0xef, 0xbf, 0xbd);
      } else {
        bytes.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 0x3f), 0x80 | (cp & 0x3f));
      }
    }
    return new Uint8Array(bytes);
  }

  function utf8Len(text) {
    return utf8BytesOf(text).byteLength;
  }

  function clipUtf8Bytes(value, maxBytes) {
    const ext = host.__EASYOFFICE_COPILOT_TEXT__?.clipUtf8Bytes;
    if (typeof ext === "function") return ext(value, maxBytes);
    const str = String(value ?? "");
    if (str.length * 3 <= maxBytes) return str;
    let bytes = 0;
    let end = 0;
    while (end < str.length) {
      const cp = str.codePointAt(end);
      const units = cp > 0xffff ? 2 : 1;
      const cost = cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
      if (bytes + cost > maxBytes) break;
      bytes += cost;
      end += units;
    }
    return end >= str.length ? str : str.slice(0, end);
  }

  function rotr(v, n) {
    return (v >>> n) | (v << (32 - n));
  }

  function sha256Hex(text) {
    const msg = utf8BytesOf(text);
    const bitLenHi = Math.floor((msg.length * 8) / 0x100000000);
    const bitLenLo = (msg.length * 8) >>> 0;
    const paddedLen = Math.ceil((msg.length + 9) / 64) * 64;
    const buf = new Uint8Array(paddedLen);
    buf.set(msg, 0);
    buf[msg.length] = 0x80;
    const view = new DataView(buf.buffer);
    view.setUint32(paddedLen - 8, bitLenHi, false);
    view.setUint32(paddedLen - 4, bitLenLo, false);

    const h = new Uint32Array([
      0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
      0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ]);
    const w = new Uint32Array(64);
    for (let off = 0; off < paddedLen; off += 64) {
      for (let i = 0; i < 16; i += 1) w[i] = view.getUint32(off + i * 4, false);
      for (let i = 16; i < 64; i += 1) {
        const s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >>> 3);
        const s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >>> 10);
        w[i] = (w[i - 16] + s0 + w[i - 7] + s1) >>> 0;
      }
      let [a, b, c, d, e, f, g, hh] = h;
      for (let i = 0; i < 64; i += 1) {
        const s1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
        const ch = (e & f) ^ (~e & g);
        const t1 = (hh + s1 + ch + SHA256_K[i] + w[i]) >>> 0;
        const s0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
        const maj = (a & b) ^ (a & c) ^ (b & c);
        const t2 = (s0 + maj) >>> 0;
        hh = g; g = f; f = e; e = (d + t1) >>> 0;
        d = c; c = b; b = a; a = (t1 + t2) >>> 0;
      }
      h[0] = (h[0] + a) >>> 0; h[1] = (h[1] + b) >>> 0;
      h[2] = (h[2] + c) >>> 0; h[3] = (h[3] + d) >>> 0;
      h[4] = (h[4] + e) >>> 0; h[5] = (h[5] + f) >>> 0;
      h[6] = (h[6] + g) >>> 0; h[7] = (h[7] + hh) >>> 0;
    }
    let out = "";
    for (let i = 0; i < 8; i += 1) out += h[i].toString(16).padStart(8, "0");
    return out;
  }

  function cmpCodeUnits(a, b) {
    return a < b ? -1 : a > b ? 1 : 0;
  }

  /// 대화 이력을 `{role, text}` 단위로 정규화하고 상한 초과 시 앞쪽 청크(기본 4개=2쌍) 단위로 퇴출해 캐시 접두를 유지한다.
  function compactHistory(
    rawHistory,
    maxItems = DEFAULT_MAX_HISTORY,
    chunkSize = DEFAULT_CHUNK_SIZE,
    maxBytes = DEFAULT_MAX_HISTORY_BYTES,
  ) {
    if (!Array.isArray(rawHistory) || rawHistory.length === 0) return [];
    const step = Math.max(2, chunkSize - (chunkSize % 2));
    const clean = [];
    for (const item of rawHistory) {
      if (!item || typeof item !== "object") continue;
      const role = item.role === "assistant" ? "assistant" : "user";
      const text = clipUtf8Bytes(String(item.text ?? item.content ?? "").trim(), MAX_HISTORY_ITEM_BYTES).trim();
      if (!text) continue;
      clean.push({ role, text });
    }
    const measure = (arr) => arr.reduce((sum, m) => sum + utf8Len(m.text) + 24, 0);
    while (clean.length > maxItems || (clean.length > 2 && measure(clean) > maxBytes)) {
      const drop = Math.min(step, Math.max(1, clean.length - 2));
      clean.splice(0, drop);
    }
    return clean;
  }

  function normalizeToolKey(name, tc) {
    const cleanName = String(name || "").trim();
    const args = {};
    if (tc && typeof tc === "object") {
      const keys = Object.keys(tc).filter((k) => k !== "name").sort(cmpCodeUnits);
      for (const k of keys) {
        let v = tc[k];
        if (typeof v === "string") {
          v = v.trim();
          if (k === "range") v = v.toUpperCase();
        }
        args[k] = v;
      }
    }
    return `${cleanName}:${JSON.stringify(args)}`;
  }

  function summarizeOpTarget(op) {
    if (!op || typeof op !== "object") return { kind: "unknown" };
    const summary = { kind: String(op.kind || op.op || op.type || "") };
    if (op.sheet) summary.sheet = String(op.sheet);
    if (op.range) summary.range = String(op.range);
    if (op.coord) summary.coord = String(op.coord);
    if (Number.isInteger(op.index)) summary.index = op.index;
    if (Number.isInteger(op.startIndex)) summary.startIndex = op.startIndex;
    if (Number.isInteger(op.endIndex)) summary.endIndex = op.endIndex;
    if (Number.isInteger(op.slideIndex)) summary.slideIndex = op.slideIndex;
    if (Array.isArray(op.replacements)) summary.count = op.replacements.length;
    if (Array.isArray(op.cells)) summary.cellCount = op.cells.length;
    return summary;
  }

  function createLoopCompactor(opts = {}) {
    const maxLoopBytes = Number(opts.maxLoopBytes) > 0 ? Number(opts.maxLoopBytes) : DEFAULT_MAX_LOOP_BYTES;
    const readCache = new Map();

    async function runCachedReadTool(mod, tc, executorFn) {
      const name = String(tc?.name || "").trim();
      const key = normalizeToolKey(name, tc);
      if (readCache.has(key)) {
        const hit = readCache.get(key);
        return {
          cached: true,
          name,
          receipt: `[도구 캐시 영수증] ${name}(${hit.argsSummary}) sha256=${hit.sha256.slice(0, 12)} (앞선 단계 ${hit.bytes}B 동일 결과 재사용)`,
        };
      }
      const result = await executorFn(mod, name, tc);
      const rawJson = JSON.stringify(result);
      const sha = sha256Hex(rawJson);
      const argsSummary = key.slice(name.length + 1, name.length + 65);
      readCache.set(key, { sha256: sha, bytes: utf8Len(rawJson), argsSummary });
      return { cached: false, name, result, sha256: sha };
    }

    function summarizeValidationFailure(res, normalizedOps, issues) {
      const opsSummary = (Array.isArray(normalizedOps) ? normalizedOps : []).map(summarizeOpTarget);
      const assistantText = JSON.stringify({
        reply: String(res?.reply || ""),
        attempted: opsSummary,
      });
      const issueLine = (Array.isArray(issues) ? issues : []).slice(0, 6).join("; ");
      const userText = `[자동 검증 게이트 오류] ${issueLine}. 오류가 없도록 좌표·크기·수식 참조만 수정해 연산을 다시 생성하십시오.`;
      return { assistantText, userText };
    }

    function pushStepPair(loopSteps, assistantText, userText) {
      if (assistantText) loopSteps.push({ role: "assistant", text: clipUtf8Bytes(String(assistantText), MAX_HISTORY_ITEM_BYTES) });
      if (userText) loopSteps.push({ role: "user", text: clipUtf8Bytes(String(userText), MAX_HISTORY_ITEM_BYTES) });
      const totalBytes = () => loopSteps.reduce((s, item) => s + utf8Len(item.text) + 24, 0);
      // 오래된 대용량 단계는 SHA-256 영수증으로 줄이고 최신 2단계는 원문을 보존한다.
      for (let i = 0; i + 2 < loopSteps.length && totalBytes() > maxLoopBytes; i += 1) {
        const cur = loopSteps[i].text;
        if (utf8Len(cur) > COMPACT_RECEIPT_THRESHOLD && !cur.startsWith("[단계 영수증]")) {
          const digest = sha256Hex(cur).slice(0, 12);
          loopSteps[i].text = `[단계 영수증] sha256=${digest} (${utf8Len(cur)}B 생략)`;
        }
      }
    }

    return {
      runCachedReadTool,
      summarizeValidationFailure,
      pushStepPair,
    };
  }

  host.__EASYOFFICE_COPILOT_CONTEXT_COMPACT__ = {
    sha256Hex,
    compactHistory,
    normalizeToolKey,
    summarizeOpTarget,
    createLoopCompactor,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
