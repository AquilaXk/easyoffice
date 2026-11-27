// 세션 기준 스냅샷(Tier 2 anchor) 고정, 25% 임계치 기반 증분 변경분(delta) 조립 및 세션 키 회전기 (PD-369).

(function (host) {
  "use strict";

  const REANCHOR_DIFF_RATIO = 0.25;
  const MAX_DELTA_BYTES = 7680;
  const MAX_DELTA_PARAGRAPHS = 64;

  function cmpCodeUnits(a, b) {
    return a < b ? -1 : a > b ? 1 : 0;
  }

  function newSessionKey() {
    return "s-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 8);
  }

  function cloneJson(value) {
    if (value == null) return value;
    return JSON.parse(JSON.stringify(value));
  }

  function utf8Len(text) {
    const str = String(text ?? "");
    if (typeof host.TextEncoder === "function") {
      return new host.TextEncoder().encode(str).byteLength;
    }
    return str.length * 3;
  }

  function clipUtf8(value, maxBytes) {
    const fn = host.__EASYOFFICE_COPILOT_TEXT__?.clipUtf8Bytes;
    if (typeof fn === "function") return fn(value, maxBytes);
    const s = String(value ?? "");
    return s.length * 3 <= maxBytes ? s : s.slice(0, Math.floor(maxBytes / 3));
  }

  function measureContextBytes(mod, ctx) {
    if (!ctx || typeof ctx !== "object") return 0;
    if ((mod === "DOC" || mod === "HWP") && ctx.doc) {
      let sum = 0;
      for (const p of ctx.doc.paragraphs || []) {
        if (typeof p === "string") sum += utf8Len(p);
      }
      return sum;
    }
    if (mod === "SHEET" && ctx.sheet) {
      let sum = 0;
      for (const c of ctx.sheet.cells || []) {
        sum += utf8Len(c?.coord) + utf8Len(c?.raw) + utf8Len(c?.value);
      }
      return sum;
    }
    if (mod === "SLIDE" && ctx.slide) {
      return utf8Len(JSON.stringify(ctx.slide.slides || []));
    }
    if (mod === "PDF" && ctx.pdf) {
      return utf8Len(ctx.pdf.text || "");
    }
    if (mod === "BASE" && ctx.base) {
      return utf8Len(JSON.stringify(ctx.base.records || []));
    }
    if (mod === "BOARD" && ctx.board) {
      return utf8Len(JSON.stringify(ctx.board.nodes || [])) + utf8Len(JSON.stringify(ctx.board.connectors || []));
    }
    return utf8Len(JSON.stringify(ctx));
  }

  function parseCellCoordJs(coord) {
    const clean = String(coord || "").trim().toUpperCase().replace(/^\$/, "");
    const m = clean.match(/^([A-Z]+)\$?(\d+)$/);
    if (!m) return null;
    let col = 0;
    for (let i = 0; i < m[1].length; i += 1) col = col * 26 + (m[1].charCodeAt(i) - 64);
    const row = Number.parseInt(m[2], 10);
    return row >= 1 && col >= 1 ? { row, col } : null;
  }

  function parseRangeBoundsJs(rangeStr) {
    const clean = String(rangeStr || "").trim();
    if (!clean) return null;
    const parts = clean.split(":");
    if (parts.length > 2) return null;
    const p1 = parseCellCoordJs(parts[0]);
    const p2 = parts.length === 2 ? parseCellCoordJs(parts[1]) : p1;
    if (!p1 || !p2) return null;
    return {
      r1: Math.min(p1.row, p2.row),
      r2: Math.max(p1.row, p2.row),
      c1: Math.min(p1.col, p2.col),
      c2: Math.max(p1.col, p2.col),
    };
  }

  function getPdfPageOneSig(ctx) {
    const firstPage = host.__EASYOFFICE_MODULE_FIND_HOOKS__?.getPdfPages?.()?.[0];
    if (firstPage) {
      const txt = typeof firstPage === "string" ? firstPage : String(firstPage?.text ?? "");
      return txt ? txt.slice(0, 64) : "";
    }
    const rawText = String(ctx?.pdf?.text || "");
    if (!rawText.includes("[1쪽]")) return null;
    return extractPdfPageText(rawText, 1).slice(0, 64);
  }

  function computeIdentity(mod, ctx) {
    if (!ctx || typeof ctx !== "object") return mod;
    if (mod === "DOC") {
      const domId = typeof host.document?.getElementById === "function"
        ? host.document.getElementById("doc-editor")?.dataset?.docId || ""
        : "";
      return `${mod}:${domId}`;
    }
    if (mod === "HWP") {
      return `${mod}:hwp`;
    }
    if (mod === "SHEET" && ctx.sheet) {
      const rawTabs = Array.isArray(ctx.sheet.tabs) ? ctx.sheet.tabs : ctx.sheet.sheetTabs;
      const tabs = Array.isArray(rawTabs) ? rawTabs.join("|") : "";
      return `${mod}:${ctx.sheet.name || ""}:${tabs}`;
    }
    if (mod === "PDF" && ctx.pdf) {
      return `${mod}:pages=${ctx.pdf.pageCount || 0}`;
    }
    if (mod === "BASE" && ctx.base) {
      const tables = Array.isArray(ctx.base.tables) ? ctx.base.tables.join("|") : "";
      return `${mod}:${ctx.base.table || ""}:${tables}`;
    }
    if (mod === "SLIDE" && ctx.slide) {
      return `${mod}:deck`;
    }
    if (mod === "BOARD") {
      return `${mod}:board`;
    }
    return mod;
  }

  function computeDocDiff(anchorDoc, liveDoc) {
    const aParas = Array.isArray(anchorDoc?.paragraphs) ? anchorDoc.paragraphs : [];
    const lParas = Array.isArray(liveDoc?.paragraphs) ? liveDoc.paragraphs : [];
    const anchorOmitted = Number(anchorDoc?.omittedParagraphs) || 0;
    const liveOmitted = Number(liveDoc?.omittedParagraphs) || 0;
    if (anchorOmitted !== liveOmitted && lParas.length < aParas.length) {
      return { changedBytes: Infinity, canEncodeDelta: false };
    }
    const maxLen = Math.max(aParas.length, lParas.length);
    const lines = [];
    const updatedParagraphs = [];
    let updatedBytes = 0;
    let truncatedUpdatedPara = false;
    let changedBytes = 0;
    const pushUpdatedPara = (idx, text) => {
      if (updatedParagraphs.length >= MAX_DELTA_PARAGRAPHS) {
        truncatedUpdatedPara = true;
        return;
      }
      const len = utf8Len(text);
      if (len > MAX_DELTA_BYTES) {
        truncatedUpdatedPara = true;
        return;
      }
      updatedBytes += len;
      updatedParagraphs.push({ index: idx, text });
    };
    for (let i = 0; i < maxLen; i += 1) {
      const aVal = i < aParas.length ? aParas[i] : undefined;
      const lVal = i < lParas.length ? lParas[i] : undefined;
      if (aVal === lVal) continue;
      if (aVal === null || lVal === null) {
        if (aVal === null && lVal === null) continue;
        if (typeof lVal === "string") {
          changedBytes += utf8Len(lVal);
          lines.push(`  [${i}] 로드됨: ${clipUtf8(lVal, 240)}`);
          pushUpdatedPara(i, lVal);
        }
        continue;
      }
      if (aVal === undefined && typeof lVal === "string") {
        changedBytes += utf8Len(lVal);
        lines.push(`  [${i}] 추가: ${clipUtf8(lVal, 240)}`);
        pushUpdatedPara(i, lVal);
      } else if (lVal === undefined && typeof aVal === "string") {
        changedBytes += utf8Len(aVal);
        lines.push(`  [${i}] 삭제됨 (기존: ${clipUtf8(aVal, 120)})`);
      } else {
        const lStr = String(lVal ?? "");
        const aStr = String(aVal ?? "");
        changedBytes += Math.max(utf8Len(lStr), utf8Len(aStr));
        lines.push(`  [${i}] 변경: ${clipUtf8(lStr, 240)}`);
        pushUpdatedPara(i, lStr);
      }
    }
    if (changedBytes === 0) return { changedBytes: 0, canEncodeDelta: true, delta: null };
    const summaryRaw = lines.join("\n");
    const summaryBytes = utf8Len(summaryRaw);
    const totalParas = lParas.length + liveOmitted;
    return {
      changedBytes,
      canEncodeDelta: !truncatedUpdatedPara
        && lines.length <= MAX_DELTA_PARAGRAPHS
        && summaryBytes + updatedBytes <= MAX_DELTA_BYTES,
      delta: {
        summary: clipUtf8(summaryRaw, MAX_DELTA_BYTES),
        docTotalParagraphs: totalParas,
        updatedParagraphs,
      },
    };
  }

  function computeSheetDiff(anchorSheet, liveSheet, liveSelection) {
    const aMap = new Map();
    for (const c of anchorSheet?.cells || []) {
      aMap.set(String(c.coord || "").toUpperCase(), `${c.raw ?? ""}\u0000${c.value ?? ""}`);
    }
    const lMap = new Map();
    for (const c of liveSheet?.cells || []) {
      lMap.set(String(c.coord || "").toUpperCase(), `${c.raw ?? ""}\u0000${c.value ?? ""}`);
    }
    const selBounds = parseRangeBoundsJs(liveSelection?.range);
    const inSelection = (coord) => {
      if (!selBounds) return false;
      const pt = parseCellCoordJs(coord);
      return Boolean(pt && pt.row >= selBounds.r1 && pt.row <= selBounds.r2 && pt.col >= selBounds.c1 && pt.col <= selBounds.c2);
    };
    const coords = new Set([...aMap.keys(), ...lMap.keys()]);
    const lines = [];
    let changedBytes = 0;
    const aWindowClipped = Number(anchorSheet?.totalCells || 0) > (anchorSheet?.cells?.length || 0);
    const lWindowClipped = Number(liveSheet?.totalCells || 0) > (liveSheet?.cells?.length || 0);
    const sorted = Array.from(coords).sort(cmpCodeUnits);
    for (const coord of sorted) {
      const aV = aMap.get(coord);
      const lV = lMap.get(coord);
      if (aV === lV) continue;
      if (lV === undefined) {
        if (lWindowClipped) continue;
        changedBytes += utf8Len(aV);
        lines.push(`  ${coord}: 삭제됨`);
      } else {
        const [raw, val] = lV.split("\u0000");
        const disp = val && val !== raw ? `${raw}=${val}` : raw || val;
        if (aV === undefined && aWindowClipped) {
          if (inSelection(coord)) {
            lines.push(`  ${coord}: ${clipUtf8(disp, 160)}`);
            continue;
          }
          if (lWindowClipped) continue;
        }
        const prev = aV ? utf8Len(aV) : 0;
        changedBytes += Math.max(utf8Len(lV), prev);
        lines.push(`  ${coord}: ${clipUtf8(disp, 160)}`);
      }
    }
    if (lines.length === 0) return { changedBytes: 0, canEncodeDelta: true, delta: null };
    const text = lines.join("\n");
    return {
      changedBytes,
      canEncodeDelta: utf8Len(text) <= MAX_DELTA_BYTES,
      delta: { summary: clipUtf8(text, MAX_DELTA_BYTES) },
    };
  }

  function extractPdfPageText(fullText, targetPage) {
    const lines = String(fullText || "").split("\n");
    let cur = 1;
    const out = [];
    for (const line of lines) {
      const m = line.trimStart().match(/^\[(\d+)쪽\]/);
      if (m) cur = Number.parseInt(m[1], 10) || cur;
      if (cur === targetPage) out.push(line);
    }
    return clipUtf8(out.join("\n"), MAX_DELTA_BYTES);
  }

  function computeSlideDiff(anchorSlide, liveSlide) {
    const aSlides = anchorSlide?.slides || [];
    const lSlides = liveSlide?.slides || [];
    const maxLen = Math.max(aSlides.length, lSlides.length);
    let changedBytes = 0;
    const lines = [];
    for (let i = 0; i < maxLen; i += 1) {
      const aJson = i < aSlides.length ? JSON.stringify(aSlides[i]) : "";
      const lJson = i < lSlides.length ? JSON.stringify(lSlides[i]) : "";
      if (aJson === lJson) continue;
      changedBytes += Math.max(utf8Len(aJson), utf8Len(lJson));
      const title = lSlides[i]?.title ? clipUtf8(lSlides[i].title, 80) : "(제목 없음)";
      const bullets = Array.isArray(lSlides[i]?.bullets) ? lSlides[i].bullets.join(" / ") : "";
      lines.push(lJson
        ? `  [${i}] ${title}${bullets ? " | " + clipUtf8(bullets, 180) : ""}`
        : `  [${i}] 삭제됨`);
    }
    if (changedBytes === 0) return { changedBytes: 0, canEncodeDelta: true, delta: null };
    const text = lines.join("\n");
    return {
      changedBytes,
      canEncodeDelta: utf8Len(text) <= MAX_DELTA_BYTES,
      delta: { summary: clipUtf8(text, MAX_DELTA_BYTES), slideCount: lSlides.length },
    };
  }

  function computeGenericDiff(anchorCtx, liveCtx) {
    const aStr = JSON.stringify(anchorCtx || {});
    const lStr = JSON.stringify(liveCtx || {});
    if (aStr === lStr) return { changedBytes: 0, canEncodeDelta: true, delta: null };
    return {
      changedBytes: Math.abs(utf8Len(lStr) - utf8Len(aStr)) + 128,
      canEncodeDelta: utf8Len(lStr) <= MAX_DELTA_BYTES,
      delta: { summary: clipUtf8(lStr, MAX_DELTA_BYTES) },
    };
  }

  function buildHistoryPayload(messages, maxHistoryItems) {
    const completed = [];
    for (let i = 0; i < messages.length; i += 1) {
      const m = messages[i];
      if (!m || m.pending || m.error) continue;
      if (m.role === "user") {
        const next = messages[i + 1];
        if (next && next.role === "assistant" && (next.pending || next.error)) continue;
      }
      completed.push(m);
    }
    const raw = completed.map((m) => ({
      role: m.role,
      text: m.role === "user" ? String(m.text || "") : String(m.reply || ""),
    }));
    const compactApi = host.__EASYOFFICE_COPILOT_CONTEXT_COMPACT__;
    return compactApi ? compactApi.compactHistory(raw, maxHistoryItems) : raw.slice(-maxHistoryItems);
  }

  function evictAlignedMessages(messages, maxMessages, chunkSize = 8) {
    if (!Array.isArray(messages) || messages.length < maxMessages) return 0;
    const step = Math.max(2, chunkSize - (chunkSize % 2));
    const overflow = messages.length - maxMessages + step;
    const drop = Math.min(messages.length, Math.ceil(overflow / step) * step);
    messages.splice(0, drop);
    return drop;
  }

  function createCopilotSessionAnchor() {
    let promptCacheSession = newSessionKey();
    let anchor = null;

    function resetSession(rotateKey = true) {
      anchor = null;
      if (rotateKey) promptCacheSession = newSessionKey();
    }

    function syncActiveModule(mod, liveContext = null) {
      if (!anchor) return false;
      const nextId = liveContext ? computeIdentity(mod, liveContext) : `${mod}:`;
      if (anchor.mod !== mod || (liveContext && anchor.docKey !== nextId)) {
        resetSession(true);
        return true;
      }
      return false;
    }

    function setAnchor(mod, docKey, liveContext) {
      const cloned = cloneJson(liveContext || {});
      const anchorPdfPage = mod === "PDF"
        ? Math.max(1, Number(cloned?.pdf?.anchorPage || cloned?.pdf?.activePage || 1))
        : null;
      if (mod === "PDF" && cloned?.pdf) {
        cloned.pdf.anchorPage = anchorPdfPage;
      }
      const anchorSlideIndex = mod === "SLIDE"
        ? Math.max(0, Number(cloned?.slide?.anchorIndex ?? cloned?.slide?.activeIndex ?? 0))
        : null;
      if (mod === "SLIDE" && cloned?.slide) {
        cloned.slide.anchorIndex = anchorSlideIndex;
      }
      anchor = {
        mod,
        docKey,
        pdfP1Sig: mod === "PDF" ? getPdfPageOneSig(cloned) : null,
        context: cloned,
        snapshotSize: Math.max(1, measureContextBytes(mod, cloned)),
        anchorPdfPage,
        anchorSlideIndex,
      };
      return cloneJson(anchor.context);
    }

    function applyLiveSlideActive(outCtx, liveContext) {
      if (outCtx?.slide && liveContext?.slide) {
        outCtx.slide.anchorIndex = anchor.anchorSlideIndex;
        outCtx.slide.activeIndex = Math.max(0, Number(liveContext.slide.activeIndex || 0));
      }
      return outCtx;
    }

    function resolveTurnPayload({ mod, command, liveContext, liveSelection }) {
      if (command) {
        return {
          promptCacheKey: promptCacheSession,
          context: cloneJson(liveContext || {}),
          contextDelta: null,
          anchored: false,
          reanchored: false,
        };
      }
      const nextDocKey = computeIdentity(mod, liveContext);
      const nextPdfP1Sig = mod === "PDF" ? getPdfPageOneSig(liveContext) : null;
      const pdfDocChanged = Boolean(
        mod === "PDF" && anchor && anchor.pdfP1Sig !== null && nextPdfP1Sig !== null && anchor.pdfP1Sig !== nextPdfP1Sig,
      );
      if (!anchor || anchor.mod !== mod || anchor.docKey !== nextDocKey || pdfDocChanged) {
        if (anchor && (anchor.mod !== mod || anchor.docKey !== nextDocKey || pdfDocChanged)) {
          promptCacheSession = newSessionKey();
        }
        const pinned = setAnchor(mod, nextDocKey, liveContext);
        return {
          promptCacheKey: promptCacheSession,
          context: pinned,
          contextDelta: null,
          anchored: true,
          reanchored: true,
        };
      }

      if (mod === "PDF" && liveContext?.pdf) {
        const outCtx = cloneJson(anchor.context);
        const curPage = Math.max(1, Number(liveContext.pdf.activePage || 1));
        if (outCtx.pdf) {
          outCtx.pdf.anchorPage = anchor.anchorPdfPage;
          outCtx.pdf.activePage = curPage;
        }
        const marker = `[${curPage}쪽]`;
        const anchorHasPage = String(anchor.context?.pdf?.text || "").includes(marker);
        let contextDelta = null;
        if (!anchorHasPage) {
          const excerpt = extractPdfPageText(liveContext.pdf.text, curPage);
          if (excerpt) contextDelta = { summary: excerpt };
        }
        return {
          promptCacheKey: promptCacheSession,
          context: outCtx,
          contextDelta,
          anchored: true,
          reanchored: false,
        };
      }

      let diff;
      if (mod === "DOC" || mod === "HWP") {
        diff = computeDocDiff(anchor.context?.doc, liveContext?.doc);
      } else if (mod === "SHEET") {
        diff = computeSheetDiff(anchor.context?.sheet, liveContext?.sheet, liveSelection);
      } else if (mod === "SLIDE") {
        diff = computeSlideDiff(anchor.context?.slide, liveContext?.slide);
      } else {
        diff = computeGenericDiff(anchor.context, liveContext);
      }

      if (diff.changedBytes === 0 && !diff.delta) {
        const outCtx = mod === "SLIDE" ? applyLiveSlideActive(cloneJson(anchor.context), liveContext) : cloneJson(anchor.context);
        return {
          promptCacheKey: promptCacheSession,
          context: outCtx,
          contextDelta: null,
          anchored: true,
          reanchored: false,
        };
      }

      const ratio = diff.changedBytes / anchor.snapshotSize;
      if (ratio <= REANCHOR_DIFF_RATIO && diff.canEncodeDelta && diff.delta) {
        const outCtx = mod === "SLIDE" ? applyLiveSlideActive(cloneJson(anchor.context), liveContext) : cloneJson(anchor.context);
        return {
          promptCacheKey: promptCacheSession,
          context: outCtx,
          contextDelta: diff.delta,
          anchored: true,
          reanchored: false,
        };
      }

      const reanchored = setAnchor(mod, nextDocKey, liveContext);
      return {
        promptCacheKey: promptCacheSession,
        context: reanchored,
        contextDelta: null,
        anchored: true,
        reanchored: true,
      };
    }

    return {
      getCacheSessionKey: () => promptCacheSession,
      hasAnchor: () => anchor !== null,
      resetSession,
      syncActiveModule,
      resolveTurnPayload,
    };
  }

  const MODULE_EXAMPLES = Object.freeze({
    SHEET: Object.freeze([
      { label: "목표 대비 % 및 최대 격차 표시", prompt: "목표 대비 %를 추가하고 가장 큰 격차를 표시하세요." },
      { label: "합계 및 평균 추가", prompt: "합계 행을 추가하세요." },
    ]),
    DOC: Object.freeze([
      { label: "문서 핵심 요약", prompt: "문서의 핵심 내용을 요약해 주세요." },
      { label: "피동 표현 다듬기", prompt: "피동 표현을 자연스러운 능동 문장으로 다듬어 주세요." },
    ]),
    HWP: Object.freeze([
      { label: "한글 문서 문장 다듬기", prompt: "문서의 어색한 표현을 자연스러운 문장으로 다듬어 주세요." },
      { label: "문서 핵심 요약 추가", prompt: "문서 끝에 핵심 요약을 추가해 주세요." },
    ]),
    SLIDE: Object.freeze([
      { label: "발표 슬라이드 추가", prompt: "발표 슬라이드를 추가하고 발표자 노트를 작성하세요." },
      { label: "발표자 노트 작성", prompt: "현재 슬라이드의 발표자 노트를 작성해 주세요." },
    ]),
    PDF: Object.freeze([
      { label: "문서 핵심 요약", prompt: "문서의 핵심 내용을 쪽 번호와 함께 요약해 주세요." },
      { label: "주요 항목 정리", prompt: "문서의 주요 항목과 수치를 정리해 주세요." },
    ]),
    BASE: Object.freeze([
      { label: "샘플 레코드 추가", prompt: "현재 테이블 구조에 맞는 예시 레코드 3건을 추가해 주세요." },
      { label: "테이블 데이터 요약", prompt: "현재 테이블 레코드와 집계 수치를 요약해 주세요." },
    ]),
    BOARD: Object.freeze([
      { label: "업무 흐름 다이어그램 생성", prompt: "요청 접수 → 검토 → 승인 → 완료 순서의 흐름도를 추가해 주세요." },
      { label: "보드 흐름 요약", prompt: "현재 보드의 연결 흐름과 핵심 요소를 설명해 주세요." },
    ]),
  });

  function normalizeLocations(rawList, fallbackMod) {
    if (!Array.isArray(rawList)) return [];
    const seen = new Set();
    const out = [];
    for (const item of rawList) {
      const loc = typeof item === "string"
        ? { module: fallbackMod, label: item, range: fallbackMod === "SHEET" ? item : undefined }
        : item;
      if (!loc || typeof loc !== "object") continue;
      const mod = String(loc.module || fallbackMod).toUpperCase();
      const label = String(loc.label || loc.range || "").trim();
      if (!label) continue;
      const key = mod + ":" + (loc.sheet || "") + ":" + (loc.page || "") + ":" + label;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push({
        module: mod,
        label,
        sheet: loc.sheet ? String(loc.sheet) : undefined,
        range: loc.range ? String(loc.range) : undefined,
        paragraphIndex: Number.isInteger(loc.paragraphIndex) ? loc.paragraphIndex : undefined,
        slideIndex: Number.isInteger(loc.slideIndex) ? loc.slideIndex : undefined,
        page: Number.isInteger(loc.page) ? loc.page : undefined,
      });
    }
    return out;
  }

  function createUsageTracker() {
    let totals = { inputTokens: 0, outputTokens: 0, cachedTokens: 0, cacheWriteTokens: 0, reasoningTokens: 0 };
    let lastContextTokens = 0;
    let preemptedHistoryLimit = null;

    return {
      reset() {
        totals = { inputTokens: 0, outputTokens: 0, cachedTokens: 0, cacheWriteTokens: 0, reasoningTokens: 0 };
        lastContextTokens = 0;
        preemptedHistoryLimit = null;
      },
      record(usage, contextTokensOpt) {
        if (!usage || typeof usage !== "object") return;
        const inp = Number(usage.inputTokens) || 0;
        const out = Number(usage.outputTokens) || 0;
        totals.inputTokens += inp;
        totals.outputTokens += out;
        totals.cachedTokens += Number(usage.cachedTokens) || 0;
        totals.cacheWriteTokens += Number(usage.cacheWriteTokens) || 0;
        totals.reasoningTokens += Number(usage.reasoningTokens) || 0;
        const ctxTok = Number(contextTokensOpt);
        lastContextTokens = Number.isFinite(ctxTok) && ctxTok > 0 ? ctxTok : inp + out;
      },
      preemptForWindow(historyOrCount, contextWindow, currentHistoryLimit) {
        const baseLimit = preemptedHistoryLimit !== null
          ? Math.min(currentHistoryLimit, preemptedHistoryLimit)
          : currentHistoryLimit;
        const win = Number(contextWindow) || 0;
        const count = Array.isArray(historyOrCount)
          ? historyOrCount.length
          : Math.max(0, Number(historyOrCount) || 0);
        if (win <= 0 || lastContextTokens <= win * 0.75 || count <= 2 || baseLimit <= 2) {
          return baseLimit;
        }
        // 문맥 창 75% 초과 시 UI 대화 스레드는 보존하고 전송 이력 한도만 8개 청크 단위로 영구 하향한다.
        const nextLimit = Math.max(2, baseLimit - 8);
        preemptedHistoryLimit = nextLimit;
        lastContextTokens = Math.round(lastContextTokens * 0.65);
        return nextLimit;
      },
      snapshot() {
        return { ...totals, lastContextTokens, preemptedHistoryLimit };
      },
    };
  }

  host.__EASYOFFICE_COPILOT_SESSION_ANCHOR__ = {
    REANCHOR_DIFF_RATIO,
    MODULE_EXAMPLES,
    createCopilotSessionAnchor,
    buildHistoryPayload,
    evictAlignedMessages,
    normalizeLocations,
    createUsageTracker,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
