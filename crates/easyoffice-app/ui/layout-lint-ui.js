(function (host) {
  "use strict";

  const MAX_LINT_ISSUES = 1024;
  const MAX_SLIDE_ELEMENTS = 256;
  const DEFAULT_A4_CONTENT_W_PT = 481.89;
  const DEFAULT_A4_CONTENT_H_PT = 728.5;
  const PT_PER_PX = 0.75;
  const PT_PER_TWIP = 0.05;
  const EMU_PER_PT = 12700;

  const KIND_LABELS = Object.freeze({
    overflow: "개체 넘침",
    overlap: "개체 겹침",
    emptyPlaceholder: "빈 자리표시자",
  });

  const DEFAULT_SLIDE_PROMPTS = new Set([
    "제목을 입력하려면 클릭하십시오",
    "부제목을 입력하려면 클릭하십시오",
    "텍스트를 입력하려면 클릭하십시오",
    "제목을 입력하십시오",
    "부제목을 입력하십시오",
    "내용을 입력하십시오",
    "click to add title",
    "click to add subtitle",
    "click to add text",
  ]);

  const SKIP_DOC_TAGS = new Set(["FOOTER", "SCRIPT", "STYLE", "BUTTON", "TEMPLATE", "NOSCRIPT"]);

  const doc = host.document;
  if (!doc) return;
  const $ = (id) => doc.getElementById(id);

  let lastDocBlocks = [];
  let bound = false;

  function activeModuleCode() {
    const officePrint = host.__EASYOFFICE_OFFICE_PRINT__;
    const appEl = $("app");
    if (appEl?.dataset?.hwpEmbed === "true") return "HWP_EMBED";
    return officePrint?.getActiveModule?.() || appEl?.dataset?.module || "DOC";
  }

  function containsTemplateToken(text) {
    const str = String(text ?? "");
    let pos = 0;
    while (pos < str.length) {
      const open = str.indexOf("{{", pos);
      if (open < 0) return false;
      const afterOpen = open + 2;
      const close = str.indexOf("}}", afterOpen);
      if (close < 0) return false;
      const inner = str.slice(afterOpen, close);
      const nested = inner.indexOf("{{");
      if (nested >= 0) {
        pos = afterOpen + nested;
        continue;
      }
      if (!inner.includes("\n") && !inner.includes("\r")) {
        const nonWs = Array.from(inner).filter((ch) => ch.trim() !== "").length;
        if (nonWs >= 1 && nonWs <= 64) return true;
      }
      pos = close + 2;
    }
    return false;
  }

  function isDefaultSlidePrompt(trimmed) {
    return DEFAULT_SLIDE_PROMPTS.has(String(trimmed || "").toLowerCase());
  }

  function estimateWrappedLines(text, availW, fontPt) {
    const cleaned = String(text ?? "").replace(/[\r\n]+$/, "");
    const parts = cleaned.split("\n");
    let lines = 0;
    for (let i = 0; i < parts.length; i += 1) {
      const seg = parts[i].replace(/\r$/, "");
      lines += 1;
      if (!seg) continue;
      let lineW = 0;
      for (const ch of seg) {
        const code = ch.codePointAt(0) || 0;
        const cw = code <= 0x7f ? fontPt * 0.55 : fontPt;
        if (lineW > 0 && lineW + cw > availW + 1e-6) {
          lines += 1;
          lineW = cw;
        } else {
          lineW += cw;
        }
      }
    }
    return lines;
  }

  function createCollector() {
    const issues = [];
    const seen = new Set();
    const counts = { overflow: 0, overlap: 0, emptyPlaceholder: 0 };
    let truncated = false;

    function push(issue) {
      if (!issue || !KIND_LABELS[issue.kind]) return true;
      const key = String(issue.targetId || "") + "\u0000" + issue.kind;
      if (seen.has(key)) return true;
      if (issues.length >= MAX_LINT_ISSUES) {
        truncated = true;
        return false;
      }
      seen.add(key);
      counts[issue.kind] += 1;
      issues.push(issue);
      return true;
    }

    return {
      push,
      finish: (mod) => ({
        module: mod,
        issueCount: issues.length,
        overflowCount: counts.overflow,
        overlapCount: counts.overlap,
        emptyPlaceholderCount: counts.emptyPlaceholder,
        counts,
        truncated,
        issues,
      }),
    };
  }

  function lintSlides(collector) {
    const deck = host.__EASYOFFICE_OFFICE_PRINT__?.slideDeckState || [];
    const codec = host.__EASYOFFICE_SLIDE_CANVAS_CODEC__;
    const aspect = deck[0]?.aspect || "16:9";
    const stage = codec?.stageSizeOf?.(aspect) || { w: 960, h: 540 };
    const stageArea = stage.w * stage.h;

    for (let sIdx = 0; sIdx < deck.length; sIdx += 1) {
      const slide = deck[sIdx] || {};
      const slideNo = sIdx + 1;
      const els = (Array.isArray(slide.elements) ? slide.elements : []).slice(0, MAX_SLIDE_ELEMENTS);

      for (let eIdx = 0; eIdx < els.length; eIdx += 1) {
        const el = els[eIdx];
        if (!el || !el.id) continue;
        const x = Number(el.x);
        const y = Number(el.y);
        const w = Number(el.w);
        const h = Number(el.h);
        if (
          !Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(w) || !Number.isFinite(h)
          || x < -0.5 || y < -0.5 || x + w > stage.w + 0.5 || y + h > stage.h + 0.5
        ) {
          if (!collector.push({
            kind: "overflow",
            pageOrSlide: slideNo,
            blockIndex: null,
            slideIndex: sIdx,
            elementId: el.id,
            targetId: el.id,
            message: `슬라이드 ${slideNo} 개체 '${el.id}'가 스테이지 경계(${Math.round(stage.w)}x${Math.round(stage.h)}pt) 밖으로 벗어났습니다.`,
          })) return;
        }

        if (el.kind === "textbox") {
          const text = String(el.text ?? "");
          const trimmed = text.trim();
          if (!trimmed || isDefaultSlidePrompt(trimmed) || containsTemplateToken(text)) {
            if (!collector.push({
              kind: "emptyPlaceholder",
              pageOrSlide: slideNo,
              blockIndex: null,
              slideIndex: sIdx,
              elementId: el.id,
              targetId: el.id,
              message: `슬라이드 ${slideNo} 글상자 '${el.id}'에 미입력 또는 치환되지 않은 자리표시자가 있습니다.`,
            })) return;
          } else if (Number.isFinite(w) && Number.isFinite(h)) {
            const fontPt = Number(el.fontSizePt) > 0 ? Number(el.fontSizePt) : 18;
            const lines = estimateWrappedLines(text, Math.max(12, w - 12), fontPt);
            const estH = lines * fontPt * 1.25;
            if (estH > h + fontPt * 0.35) {
              if (!collector.push({
                kind: "overflow",
                pageOrSlide: slideNo,
                blockIndex: null,
                slideIndex: sIdx,
                elementId: el.id,
                targetId: el.id,
                message: `슬라이드 ${slideNo} 글상자 '${el.id}' 텍스트 예상 높이(${estH.toFixed(1)}pt)가 상자 높이(${h.toFixed(1)}pt)를 초과했습니다.`,
              })) return;
            }
          }
        } else if (el.kind === "shape") {
          const text = String(el.text ?? "");
          const trimmed = text.trim();
          if (containsTemplateToken(text)) {
            if (!collector.push({
              kind: "emptyPlaceholder",
              pageOrSlide: slideNo,
              blockIndex: null,
              slideIndex: sIdx,
              elementId: el.id,
              targetId: el.id,
              message: `슬라이드 ${slideNo} 도형 '${el.id}'에 치환되지 않은 자리표시자가 있습니다.`,
            })) return;
          }
          if (trimmed && Number.isFinite(w) && Number.isFinite(h)) {
            const fontPt = Number(el.fontSizePt) > 0 ? Number(el.fontSizePt) : 16;
            const lines = estimateWrappedLines(text, Math.max(12, w - 12), fontPt);
            const estH = lines * fontPt * 1.25;
            if (estH > h + fontPt * 0.35) {
              if (!collector.push({
                kind: "overflow",
                pageOrSlide: slideNo,
                blockIndex: null,
                slideIndex: sIdx,
                elementId: el.id,
                targetId: el.id,
                message: `슬라이드 ${slideNo} 도형 '${el.id}' 내부 텍스트가 높이(${h.toFixed(1)}pt)를 초과했습니다.`,
              })) return;
            }
          }
        } else if (el.kind === "table") {
          const rows = Array.isArray(el.rows) ? el.rows : [];
          const needH = rows.length * 16;
          if (Number.isFinite(h) && needH > h + 4) {
            if (!collector.push({
              kind: "overflow",
              pageOrSlide: slideNo,
              blockIndex: null,
              slideIndex: sIdx,
              elementId: el.id,
              targetId: el.id,
              message: `슬라이드 ${slideNo} 표 '${el.id}' 행 높이(${needH.toFixed(1)}pt)가 상자 높이(${h.toFixed(1)}pt)를 초과했습니다.`,
            })) return;
          }
          if (rows.length > 0) {
            let allEmpty = true;
            let hasToken = false;
            for (const row of rows) {
              for (const cell of Array.isArray(row) ? row : []) {
                const txt = String(cell ?? "");
                if (txt.trim()) allEmpty = false;
                if (!hasToken && containsTemplateToken(txt)) hasToken = true;
              }
            }
            if (allEmpty || hasToken) {
              if (!collector.push({
                kind: "emptyPlaceholder",
                pageOrSlide: slideNo,
                blockIndex: null,
                slideIndex: sIdx,
                elementId: el.id,
                targetId: el.id,
                message: `슬라이드 ${slideNo} 표 '${el.id}'에 빈 셀 또는 미치환 자리표시자가 있습니다.`,
              })) return;
            }
          }
        }
      }

      // Sweep-line overlap check (excludes >=90% background shapes).
      const boxes = [];
      for (let i = 0; i < els.length; i += 1) {
        const el = els[i];
        const x = Number(el?.x), y = Number(el?.y), w = Number(el?.w), h = Number(el?.h);
        if (!el?.id || !Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(w) || !Number.isFinite(h)) continue;
        if (el.kind === "shape" && w * h >= stageArea * 0.9) continue;
        boxes.push({ x0: x, y0: y, x1: x + w, y1: y + h, idx: i });
      }
      boxes.sort((a, b) => (a.x0 - b.x0) || (a.idx - b.idx));
      for (let p = 0; p < boxes.length; p += 1) {
        const a = boxes[p];
        for (let q = p + 1; q < boxes.length; q += 1) {
          const b = boxes[q];
          if (b.x0 >= a.x1 - 1) break;
          const ix = Math.min(a.x1, b.x1) - Math.max(a.x0, b.x0);
          if (ix <= 1) continue;
          const iy = Math.min(a.y1, b.y1) - Math.max(a.y0, b.y0);
          if (iy <= 1) continue;
          const area = ix * iy;
          if (area <= 4) continue;
          const first = els[Math.min(a.idx, b.idx)];
          const second = els[Math.max(a.idx, b.idx)];
          if (!collector.push({
            kind: "overlap",
            pageOrSlide: slideNo,
            blockIndex: null,
            slideIndex: sIdx,
            elementId: first.id,
            targetId: first.id,
            message: `슬라이드 ${slideNo} 개체 '${first.id}'와 '${second.id}'가 ${area.toFixed(1)}pt² 영역에서 겹쳤습니다.`,
          })) return;
        }
      }

      const titleText = String(slide.title ?? "");
      const lowerText = String(slide.lower ?? "");
      const bodyLines = lowerText
        .split(/\r?\n/)
        .filter((line) => line.trim() !== "").length;
      const bodyChars = Array.from(lowerText).length;
      if (bodyLines > 10 || bodyChars > 500) {
        if (!collector.push({
          kind: "overflow",
          pageOrSlide: slideNo,
          blockIndex: null,
          slideIndex: sIdx,
          elementId: null,
          targetId: `slide-${slideNo}-body`,
          message: `슬라이드 ${slideNo} 본문이 허용량(${bodyLines}줄, ${bodyChars}자)을 초과해 넘칠 수 있습니다.`,
        })) return;
      }

      if (containsTemplateToken(titleText)) {
        if (!collector.push({
          kind: "emptyPlaceholder",
          pageOrSlide: slideNo,
          blockIndex: null,
          slideIndex: sIdx,
          elementId: null,
          targetId: `slide-${slideNo}-title`,
          message: `슬라이드 ${slideNo} 제목에 치환되지 않은 자리표시자가 있습니다.`,
        })) return;
      }
      if (containsTemplateToken(lowerText)) {
        if (!collector.push({
          kind: "emptyPlaceholder",
          pageOrSlide: slideNo,
          blockIndex: null,
          slideIndex: sIdx,
          elementId: null,
          targetId: `slide-${slideNo}-body`,
          message: `슬라이드 ${slideNo} 본문에 치환되지 않은 자리표시자가 있습니다.`,
        })) return;
      }

      const layout = codec?.normalizeLayout?.(slide.layout) || slide.layout || "titleAndContent";
      const hasTitle = titleText.trim() !== ""
        || els.some((e) => e?.kind === "textbox" && e.role === "title" && String(e.text || "").trim() !== "");
      const hasBody = lowerText.trim() !== ""
        || (Array.isArray(slide.insertedNodes) && slide.insertedNodes.length > 0)
        || els.length > 0;

      if ((layout === "titleSlide" || layout === "sectionHeader") && !hasTitle) {
        if (!collector.push({
          kind: "emptyPlaceholder",
          pageOrSlide: slideNo,
          blockIndex: null,
          slideIndex: sIdx,
          elementId: null,
          targetId: `slide-${slideNo}-title`,
          message: `슬라이드 ${slideNo}의 제목 영역이 비어 있습니다.`,
        })) return;
      } else if (layout === "titleAndContent" || layout === "twoColumn") {
        if (!hasTitle) {
          if (!collector.push({
            kind: "emptyPlaceholder",
            pageOrSlide: slideNo,
            blockIndex: null,
            slideIndex: sIdx,
            elementId: null,
            targetId: `slide-${slideNo}-title`,
            message: `슬라이드 ${slideNo}의 제목이 비어 있습니다.`,
          })) return;
        }
        if (!hasBody) {
          if (!collector.push({
            kind: "emptyPlaceholder",
            pageOrSlide: slideNo,
            blockIndex: null,
            slideIndex: sIdx,
            elementId: null,
            targetId: `slide-${slideNo}-body`,
            message: `슬라이드 ${slideNo}의 본문 영역이 비어 있습니다.`,
          })) return;
        }
      }
    }
  }

  function parseCssLengthPt(raw) {
    const s = String(raw || "").trim().toLowerCase();
    if (!s) return 0;
    const m = /^([\d.]+)\s*(pt|px|mm|cm|in)?$/.exec(s);
    if (!m) return 0;
    const val = Number(m[1]);
    if (!Number.isFinite(val) || val <= 0) return 0;
    const unit = m[2] || "px";
    if (unit === "pt") return val;
    if (unit === "px") return val * PT_PER_PX;
    if (unit === "mm") return (val * 72) / 25.4;
    if (unit === "cm") return (val * 72) / 2.54;
    if (unit === "in") return val * 72;
    return 0;
  }

  function isSkippedDocChild(el) {
    const tag = String(el?.tagName || "").toUpperCase();
    if (!tag || SKIP_DOC_TAGS.has(tag)) return true;
    if (el.id === "hwp-manuscript-preview") return true;
    if (el.classList?.contains("doc-table-toolbar") || el.classList?.contains("doc-page-layer")) return true;
    const ds = el.dataset;
    if (ds && (ds.eoHf !== undefined || ds.eoNotes !== undefined || ds.eoComments !== undefined)) return true;
    return typeof el.hasAttribute === "function" && el.hasAttribute("hidden");
  }

  function docContentBoundsPt(editor) {
    const savePayload = host.__EASYOFFICE_SAVE_PAYLOAD__;
    const frame = savePayload?.collectPageFrame?.(editor);
    if (frame?.margins) {
      const m = frame.margins;
      const isLandscape = frame.orientation === "landscape";
      const paperW = isLandscape ? 841.89 : 595.28;
      const paperH = isLandscape ? 595.28 : 841.89;
      return {
        wPt: Math.max(36, paperW - (Number(m.left) + Number(m.right)) * PT_PER_TWIP),
        hPt: Math.max(36, paperH - (Number(m.top) + Number(m.bottom)) * PT_PER_TWIP),
        frame,
      };
    }
    return { wPt: DEFAULT_A4_CONTENT_W_PT, hPt: DEFAULT_A4_CONTENT_H_PT, frame };
  }

  function tableWidthPt(tableEl) {
    const rawTwips = String(tableEl.dataset?.eoColWidths || "").trim();
    if (rawTwips) {
      const parts = rawTwips.split(",").map((v) => Number(v.trim())).filter((n) => Number.isFinite(n) && n > 0);
      if (parts.length > 0) return parts.reduce((a, b) => a + b, 0) * PT_PER_TWIP;
    }
    const cols = tableEl.querySelectorAll("col");
    if (cols.length > 0) {
      const sum = cols.reduce((acc, col) => acc + (parseCssLengthPt(col.style?.width) || parseCssLengthPt(col.getAttribute("width"))), 0);
      if (sum > 0) return sum;
    }
    const firstRow = tableEl.querySelector("tr");
    if (firstRow) {
      let rowSum = 0;
      for (const cell of firstRow.cells || []) {
        rowSum += parseCssLengthPt(cell.style?.width) || parseCssLengthPt(cell.getAttribute("width"));
      }
      if (rowSum > 0) return rowSum;
    }
    return parseCssLengthPt(tableEl.style?.width) || (Number(tableEl.offsetWidth) > 0 ? Number(tableEl.offsetWidth) * PT_PER_PX : 0);
  }

  function hasTableMergeCollision(tableEl) {
    const occupied = new Set();
    const rows = tableEl.querySelectorAll("tr");
    if (rows.length === 0) return false;
    const rowWidths = new Array(rows.length).fill(0);
    let hasMerge = false;
    for (let r = 0; r < rows.length; r += 1) {
      let colCursor = 0;
      for (const cell of rows[r].cells || []) {
        while (occupied.has(r + ":" + colCursor)) colCursor += 1;
        const rs = Math.max(1, Number(cell.rowSpan) || 1);
        const cs = Math.max(1, Number(cell.colSpan) || 1);
        if (rs > 1 || cs > 1) hasMerge = true;
        if (r + rs > rows.length) return true;
        for (let dr = 0; dr < rs; dr += 1) {
          rowWidths[r + dr] = Math.max(rowWidths[r + dr], colCursor + cs);
          for (let dc = 0; dc < cs; dc += 1) {
            const key = (r + dr) + ":" + (colCursor + dc);
            if (occupied.has(key)) return true;
            occupied.add(key);
          }
        }
        colCursor += cs;
      }
      while (occupied.has(r + ":" + colCursor)) colCursor += 1;
      rowWidths[r] = Math.max(rowWidths[r], colCursor);
    }
    return hasMerge && rowWidths.some((w) => w !== rowWidths[0]);
  }

  function imageSizePt(imgEl) {
    const wPt = parseCssLengthPt(imgEl.style?.width)
      || (Number(imgEl.dataset?.eoW) > 0 ? Number(imgEl.dataset.eoW) / EMU_PER_PT : 0)
      || parseCssLengthPt(imgEl.getAttribute("width"))
      || (Number(imgEl.naturalWidth) > 0 ? Number(imgEl.naturalWidth) * PT_PER_PX : 0);
    const hPt = parseCssLengthPt(imgEl.style?.height)
      || (Number(imgEl.dataset?.eoH) > 0 ? Number(imgEl.dataset.eoH) / EMU_PER_PT : 0)
      || parseCssLengthPt(imgEl.getAttribute("height"))
      || (Number(imgEl.naturalHeight) > 0 ? Number(imgEl.naturalHeight) * PT_PER_PX : 0);
    return { wPt, hPt };
  }

  function lintDocument(collector) {
    lastDocBlocks = [];
    const editor = $("doc-editor");
    if (!editor) return;
    const { wPt: contentWPt, hPt: contentHPt, frame } = docContentBoundsPt(editor);

    if (frame?.margins) {
      const m = frame.margins;
      const hasHeader = (frame.header?.length || frame.firstHeader?.length || frame.evenHeader?.length) > 0;
      if (hasHeader && Number(m.header) > Number(m.top)) {
        if (!collector.push({
          kind: "overlap",
          pageOrSlide: 1,
          blockIndex: null,
          targetId: "header",
          message: "머리말 여백이 본문 상단 여백보다 커서 본문과 겹칩니다.",
        })) return;
      }
      const hasFooter = (frame.footer?.length || frame.firstFooter?.length || frame.evenFooter?.length) > 0;
      if (hasFooter && Number(m.footer) > Number(m.bottom)) {
        if (!collector.push({
          kind: "overlap",
          pageOrSlide: 1,
          blockIndex: null,
          targetId: "footer",
          message: "꼬리말 여백이 본문 하단 여백보다 커서 본문과 겹칩니다.",
        })) return;
      }
    }

    let pageNo = 1;
    let blockIdx = 0;
    for (const child of editor.children || []) {
      if (isSkippedDocChild(child)) continue;
      const idx = blockIdx++;
      lastDocBlocks[idx] = child;
      if (idx > 0 && child.dataset?.eoBreak === "page") pageNo += 1;
      const tag = String(child.tagName || "").toUpperCase();

      if (tag === "TABLE") {
        const wPt = tableWidthPt(child);
        if (wPt > contentWPt + 1) {
          if (!collector.push({
            kind: "overflow",
            pageOrSlide: pageNo,
            blockIndex: idx,
            targetId: `table-${idx}`,
            message: `표(블록 ${idx}) 전체 열 너비(${wPt.toFixed(1)}pt)가 본문 폭(${contentWPt.toFixed(1)}pt)을 초과합니다.`,
          })) return;
        }
        if (hasTableMergeCollision(child)) {
          if (!collector.push({
            kind: "overlap",
            pageOrSlide: pageNo,
            blockIndex: idx,
            targetId: `table-${idx}`,
            message: `표(블록 ${idx})의 병합 셀 영역이 서로 충돌합니다.`,
          })) return;
        }
        const cells = child.querySelectorAll("td, th");
        if (cells.length > 0) {
          let allEmpty = child.querySelectorAll("img").length === 0;
          let hasToken = false;
          for (const cell of cells) {
            const txt = String(cell.textContent || "");
            if (txt.trim()) {
              allEmpty = false;
              if (!hasToken && containsTemplateToken(txt)) hasToken = true;
            }
          }
          if (allEmpty) {
            if (!collector.push({
              kind: "emptyPlaceholder",
              pageOrSlide: pageNo,
              blockIndex: idx,
              targetId: `table-${idx}`,
              message: `표(블록 ${idx})의 모든 셀이 비어 있습니다.`,
            })) return;
          } else if (hasToken) {
            if (!collector.push({
              kind: "emptyPlaceholder",
              pageOrSlide: pageNo,
              blockIndex: idx,
              targetId: `table-${idx}`,
              message: `표(블록 ${idx}) 셀에 치환되지 않은 템플릿 자리표시자가 있습니다.`,
            })) return;
          }
        }
        continue;
      }

      const imgs = tag === "IMG" ? [child] : child.querySelectorAll("img");
      for (const img of imgs) {
        const { wPt, hPt } = imageSizePt(img);
        if (wPt > contentWPt + 1 || hPt > contentHPt + 1) {
          if (!collector.push({
            kind: "overflow",
            pageOrSlide: pageNo,
            blockIndex: idx,
            targetId: `block-${idx}`,
            message: `그림(블록 ${idx}, ${wPt.toFixed(1)}pt)이 본문 가용 폭(${contentWPt.toFixed(1)}pt)을 초과합니다.`,
          })) return;
          break;
        }
      }

      const text = String(child.textContent || "");
      const trimmed = text.trim();
      if (/^H[1-6]$/.test(tag) && !trimmed && imgs.length === 0) {
        if (!collector.push({
          kind: "emptyPlaceholder",
          pageOrSlide: pageNo,
          blockIndex: idx,
          targetId: `block-${idx}`,
          message: `제목 문단(블록 ${idx})이 비어 있습니다.`,
        })) return;
      } else if (trimmed && containsTemplateToken(text)) {
        if (!collector.push({
          kind: "emptyPlaceholder",
          pageOrSlide: pageNo,
          blockIndex: idx,
          targetId: `block-${idx}`,
          message: `문단(블록 ${idx})에 치환되지 않은 템플릿 자리표시자가 있습니다.`,
        })) return;
      }
    }
  }

  function buildLocalWorkspaceLint() {
    const mod = activeModuleCode();
    const collector = createCollector();
    if (mod === "SLIDE") lintSlides(collector);
    else if (mod === "DOC") lintDocument(collector);
    return { mod, collector };
  }

  function lintWorkspaceSync() {
    const { mod, collector } = buildLocalWorkspaceLint();
    return collector.finish(mod);
  }

  async function fetchBackendInspectIssues(mod) {
    const invoke = host.__EASYOFFICE_INVOKE_BACKEND__;
    if (typeof invoke !== "function") return [];
    try {
      if (mod === "SLIDE") {
        const deck = host.__EASYOFFICE_OFFICE_PRINT__?.slideDeckState || [];
        const codec = host.__EASYOFFICE_SLIDE_CANVAS_CODEC__;
        const slidePayload = codec?.serializeDeckPayload?.(deck, doc) || { slides: [] };
        const res = await invoke("convert_document_format", JSON.stringify({
          action: "inspect",
          sourceFormat: "pptx",
          ...slidePayload,
        }));
        return Array.isArray(res?.layoutLint?.issues) ? res.layoutLint.issues : [];
      }
      if (mod === "DOC") {
        const editor = $("doc-editor");
        const savePayload = host.__EASYOFFICE_SAVE_PAYLOAD__;
        if (!editor || !savePayload?.collectDocBlocks) return [];
        const blocks = savePayload.collectDocBlocks(editor);
        const pageFrame = savePayload.collectPageFrame?.(editor);
        const pageSetup = host.__EASYOFFICE_DOC_PAGE_SETUP_CORE__?.payloadOf?.(editor.dataset?.eoPage);
        const res = await invoke("convert_document_format", JSON.stringify({
          action: "inspect",
          sourceFormat: "docx",
          blocks,
          ...(pageFrame || {}),
          ...(pageSetup || {}),
        }));
        return Array.isArray(res?.layoutLint?.issues) ? res.layoutLint.issues : [];
      }
    } catch {
      return [];
    }
    return [];
  }

  async function lintWorkspace() {
    const { mod, collector } = buildLocalWorkspaceLint();
    const remoteIssues = await fetchBackendInspectIssues(mod);
    for (const remote of remoteIssues) {
      if (!remote || !KIND_LABELS[remote.kind]) continue;
      const pageNo = Math.max(1, Number(remote.pageOrSlide) || 1);
      const targetId = String(remote.targetId || "");
      collector.push({
        kind: remote.kind,
        pageOrSlide: pageNo,
        blockIndex: Number.isInteger(remote.blockIndex) ? remote.blockIndex : null,
        slideIndex: mod === "SLIDE" ? pageNo - 1 : undefined,
        elementId: mod === "SLIDE" && !targetId.startsWith("slide-") ? targetId : null,
        targetId,
        message: String(remote.message || ""),
      });
    }
    return collector.finish(mod);
  }

  function jumpToIssue(issue) {
    if (!issue) return false;
    const mod = activeModuleCode();
    if (mod === "SLIDE" || issue.slideIndex !== undefined) {
      const idx = Math.max(0, Number(issue.slideIndex ?? ((Number(issue.pageOrSlide) || 1) - 1)));
      const thumb = doc.querySelector(`#slide-thumb-list .slide-thumb[data-slide-idx="${idx}"]`);
      if (thumb && !thumb.classList.contains("active")) thumb.click();
      const canvasApi = host.__EASYOFFICE_SLIDE_CANVAS__;
      const targetId = String(issue.targetId || "");
      if (issue.elementId || (targetId && !targetId.startsWith("slide-"))) {
        const elId = issue.elementId || targetId;
        canvasApi?.selectElement?.(elId);
        $("slide-canvas-layer")?.querySelector(`.slide-el[data-element-id="${elId}"]`)?.focus?.();
      } else if (targetId.endsWith("-title")) {
        canvasApi?.clearSelection?.();
        $("slide-title-zone")?.focus?.();
      } else {
        canvasApi?.clearSelection?.();
        $("slide-lower-zone")?.focus?.();
      }
      return true;
    }

    const editor = $("doc-editor");
    if (!editor) return false;
    for (const prev of editor.querySelectorAll("[data-eo-lint-active]")) {
      delete prev.dataset.eoLintActive;
    }
    let targetEl = Number.isInteger(issue.blockIndex) ? lastDocBlocks[issue.blockIndex] : null;
    if (!targetEl && issue.targetId === "header") targetEl = editor.querySelector('[data-eo-hf*="Header"], [data-eo-hf="header"]') || editor;
    if (!targetEl && issue.targetId === "footer") targetEl = editor.querySelector('[data-eo-hf*="Footer"], [data-eo-hf="footer"]') || editor;
    if (!targetEl) return false;
    targetEl.dataset.eoLintActive = "true";
    targetEl.scrollIntoView?.({ block: "center" });
    targetEl.focus?.();
    editor.dispatchEvent({ type: "click", target: targetEl });
    return true;
  }

  function renderReport(report) {
    const summaryEl = $("doc-inspect-summary");
    const listEl = $("doc-inspect-list");
    if (!summaryEl || !listEl) return;

    if (!report || report.issueCount === 0) {
      summaryEl.textContent = "문제가 발견되지 않았습니다.";
      listEl.replaceChildren();
      return;
    }

    const c = report.counts;
    summaryEl.textContent =
      `검사 결과 ${report.issueCount}건 · 개체 넘침 ${c.overflow} · 개체 겹침 ${c.overlap} · 빈 자리표시자 ${c.emptyPlaceholder}`;

    const rows = report.issues.map((issue) => {
      const li = doc.createElement("li");
      li.className = "doc-inspect-item";
      li.dataset.kind = issue.kind;
      li.dataset.targetId = String(issue.targetId || "");

      const badge = doc.createElement("span");
      badge.className = "doc-inspect-badge";
      badge.dataset.kind = issue.kind;
      badge.textContent = KIND_LABELS[issue.kind] || issue.kind;

      const desc = doc.createElement("span");
      desc.className = "doc-inspect-desc";
      desc.textContent = issue.message;

      const btn = doc.createElement("button");
      btn.type = "button";
      btn.className = "dialog-btn doc-inspect-jump-btn";
      btn.textContent = "이동";
      btn.addEventListener("click", () => {
        jumpToIssue(issue);
      });

      li.appendChild(badge);
      li.appendChild(desc);
      li.appendChild(btn);
      return li;
    });
    listEl.replaceChildren(...rows);
  }

  async function open() {
    const mod = activeModuleCode();
    if (mod !== "DOC" && mod !== "SLIDE") {
      host.__EASYOFFICE_OFFICE_PRINT__?.notify?.("문서 검사는 문서 및 프레젠테이션 창에서 사용할 수 있습니다.");
      return null;
    }
    const dialog = $("doc-inspect-dialog");
    if (!dialog) return null;
    const report = await lintWorkspace();
    renderReport(report);
    if (!dialog.open) {
      if (typeof dialog.showModal === "function") dialog.showModal();
      else dialog.open = true;
    }
    return report;
  }

  function close() {
    const dialog = $("doc-inspect-dialog");
    if (!dialog?.open) return;
    if (typeof dialog.close === "function") dialog.close();
    else dialog.open = false;
  }

  function appendLintFeedbackAfterVerify() {
    const mod = activeModuleCode();
    if (mod !== "DOC" && mod !== "SLIDE") return;
    const report = lintWorkspaceSync();
    if (report.issueCount <= 0) return;
    const fb = $("status-command-feedback");
    if (fb && !String(fb.textContent || "").includes("문서 검사")) {
      fb.textContent = String(fb.textContent || "").trim() + ` · 문서 검사 ${report.issueCount}건`;
    }
  }

  function bind() {
    if (bound) return;
    const dialog = $("doc-inspect-dialog");
    if (!dialog) return;
    bound = true;

    for (const item of doc.querySelectorAll('.md-item[data-verify-act="doc-inspect"]')) {
      item.addEventListener("click", () => {
        host.__EASYOFFICE_CLOSE_MENUS__?.(doc);
        void open();
      });
    }

    $("btn-doc-inspect-rerun")?.addEventListener("click", () => {
      void lintWorkspace().then((report) => renderReport(report));
    });
    $("btn-doc-inspect-close")?.addEventListener("click", () => close());

    for (const closeEl of dialog.querySelectorAll("[data-dialog-close]")) {
      closeEl.addEventListener("click", () => close());
    }

    for (const verifyEl of doc.querySelectorAll('[data-studio-menu-cmd="review-verify"]')) {
      verifyEl.addEventListener("click", () => {
        void (async () => {
          for (let i = 0; i < 40; i += 1) await Promise.resolve();
          appendLintFeedbackAfterVerify();
        })();
      });
    }
  }

  const api = {
    open,
    close,
    lintWorkspace,
    lintWorkspaceSync,
    jumpToIssue,
  };
  host.__EASYOFFICE_LAYOUT_LINT__ = api;

  bind();
  doc.addEventListener("DOMContentLoaded", bind);
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
