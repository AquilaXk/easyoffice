// WebView 주도 경량 멀티턴 읽기·검증 루프 (PD-074 이벤트 권한 불허 및 ipc_commands = 15 준수, PD-360).
// 최대 6턴(MAX_TURNS = 6) 및 누적 요청 바이트 상한(MAX_CUMULATIVE_BYTES)을 엄격히 강제하고,
// 온디맨드 읽기 도구(read_doc_blocks, read_sheet_range, find_sheet_cells, search_pdf_pages, read_base_records, read_slides)와
// 미리보기 제시 전 사후 검증 게이트(SLIDE·DOC layout_lint PD-258, SHEET 수식 오류 신규 발생 점검)를 수행한다.

(function (host) {
  "use strict";

  const MAX_TURNS = 6;
  const MAX_LINT_RETRIES = 2;
  const MAX_CUMULATIVE_BYTES = 384 * 1024;
  const MAX_TOOL_OUTPUT_CHARS = 12000;
  const MAX_TOOL_OUTPUT_BYTES = 15000;
  const MAX_HISTORY_SEND = 16;
  const MAX_DRY_RUN_EVAL_CELLS = 10000;
  const DEFAULT_A4_CONTENT_W_PT = 481.89;
  const PT_PER_TWIP = 0.05;

  function utf8ByteLength(str) {
    if (typeof TextEncoder === "function") {
      return new TextEncoder().encode(String(str ?? "")).byteLength;
    }
    return String(str ?? "").length * 3;
  }

  function normalizeIssueSignature(it) {
    const kind = String(it?.kind || "lint");
    const msg = String(it?.message || "").replace(/\(블록 \d+\)/g, "(블록 *)");
    return `${kind}\u0000${msg}`;
  }

  function countIssueSignatures(issues) {
    const counts = new Map();
    for (const it of issues) {
      if (!it || !it.message) continue;
      const sig = normalizeIssueSignature(it);
      counts.set(sig, (counts.get(sig) || 0) + 1);
    }
    return counts;
  }

  function diffNewLintIssues(baseIssues, afterIssues) {
    const baseCounts = countIssueSignatures(baseIssues);
    const seenCounts = new Map();
    const added = [];
    for (const it of afterIssues) {
      if (!it || !it.message) continue;
      const sig = normalizeIssueSignature(it);
      const nextCount = (seenCounts.get(sig) || 0) + 1;
      seenCounts.set(sig, nextCount);
      if (nextCount > (baseCounts.get(sig) || 0)) {
        added.push(String(it.message));
      }
    }
    return added;
  }

  async function collectSlideLintIssues(ctx, lintApi) {
    const merged = [];
    const seen = new Set();
    const pushIssue = (it) => {
      if (!it || !it.message) return;
      const key = `${it.kind || ""}:${it.targetId || ""}:${it.message}`;
      if (seen.has(key)) return;
      seen.add(key);
      merged.push(it);
    };
    for (const it of lintApi?.lintWorkspaceSync?.()?.issues || []) pushIssue(it);
    if (typeof ctx.invokeIpc === "function") {
      try {
        const codec = host.__EASYOFFICE_SLIDE_CANVAS_CODEC__;
        const slidePayload = codec?.serializeDeckPayload?.(ctx.slideDeckState, ctx.document) || { slides: ctx.slideDeckState };
        const res = await ctx.invokeIpc("convert_document_format", JSON.stringify({
          action: "inspect",
          sourceFormat: "pptx",
          ...slidePayload,
        }));
        for (const it of res?.layoutLint?.issues || []) pushIssue(it);
      } catch {
        // IPC 미구현 모의 환경에서는 로컬 layout_lint 결과만 사용한다.
      }
    }
    return merged;
  }

  function docContentWidthPt(editor) {
    const frame = host.__EASYOFFICE_SAVE_PAYLOAD__?.collectPageFrame?.(editor);
    if (frame?.margins) {
      const paperW = frame.orientation === "landscape" ? 841.89 : 595.28;
      return Math.max(36, paperW - (Number(frame.margins.left) + Number(frame.margins.right)) * PT_PER_TWIP);
    }
    return DEFAULT_A4_CONTENT_W_PT;
  }

  function tableExplicitWidthPt(tableEl) {
    const rawTwips = String(tableEl.dataset?.eoColWidths || "").trim();
    if (!rawTwips) return 0;
    const parts = rawTwips.split(",").map((v) => Number(v.trim())).filter((n) => Number.isFinite(n) && n > 0);
    return parts.length > 0 ? parts.reduce((a, b) => a + b, 0) * PT_PER_TWIP : 0;
  }

  async function collectDocContainerLintIssues(container, ctx, lintApi) {
    const merged = [];
    const seen = new Set();
    const pushIssue = (it) => {
      if (!it || !it.message) return;
      const key = `${it.kind || ""}:${it.targetId || ""}:${it.message}`;
      if (seen.has(key)) return;
      seen.add(key);
      merged.push(it);
    };
    const hasToken = lintApi?.containsTemplateToken || ((s) => /\{\{[^}\r\n]{1,64}\}\}/.test(String(s || "")));
    const contentWPt = docContentWidthPt(container);
    const blocks = ctx.docBlocksOf ? ctx.docBlocksOf(container) : Array.from(container.children || []);
    blocks.forEach((el, idx) => {
      const tag = String(el?.tagName || "").toUpperCase();
      const txt = String(el?.textContent || "");
      if (tag === "TABLE") {
        const wPt = tableExplicitWidthPt(el);
        const firstRowCols = el.querySelector?.("tr")?.cells?.length || 0;
        if (wPt > contentWPt + 1) {
          pushIssue({ kind: "overflow", targetId: `table-${idx}`, message: `표(블록 ${idx}) 전체 열 너비(${wPt.toFixed(1)}pt)가 본문 폭(${contentWPt.toFixed(1)}pt)을 초과합니다.` });
        } else if (firstRowCols * 48 > contentWPt + 1) {
          pushIssue({ kind: "overflow", targetId: `table-${idx}`, message: `표(블록 ${idx}) 열 개수(${firstRowCols}열)가 본문 가용 폭(${contentWPt.toFixed(1)}pt)을 초과합니다.` });
        }
        const cells = Array.from(el.querySelectorAll?.("td, th") || []);
        if (cells.length > 0 && cells.every((c) => !String(c.textContent || "").trim())) {
          pushIssue({ kind: "emptyPlaceholder", targetId: `table-${idx}`, message: `표(블록 ${idx})의 모든 셀이 비어 있습니다.` });
        } else if (cells.some((c) => hasToken(c.textContent))) {
          pushIssue({ kind: "emptyPlaceholder", targetId: `table-${idx}`, message: `표(블록 ${idx}) 셀에 치환되지 않은 템플릿 자리표시자가 있습니다.` });
        }
      } else if (/^H[1-6]$/.test(tag) && !txt.trim()) {
        pushIssue({ kind: "emptyPlaceholder", targetId: `block-${idx}`, message: `제목 문단(블록 ${idx})이 비어 있습니다.` });
      } else if (txt.trim() && hasToken(txt)) {
        pushIssue({ kind: "emptyPlaceholder", targetId: `block-${idx}`, message: `문단(블록 ${idx})에 치환되지 않은 템플릿 자리표시자가 있습니다.` });
      }
    });
    const savePayload = host.__EASYOFFICE_SAVE_PAYLOAD__;
    if (typeof ctx.invokeIpc === "function" && savePayload?.collectDocBlocks) {
      try {
        const serializedBlocks = savePayload.collectDocBlocks(container);
        const pageFrame = savePayload.collectPageFrame?.(container);
        const pageSetup = host.__EASYOFFICE_DOC_PAGE_SETUP_CORE__?.payloadOf?.(container.dataset?.eoPage);
        const res = await ctx.invokeIpc("convert_document_format", JSON.stringify({
          action: "inspect",
          sourceFormat: "docx",
          blocks: serializedBlocks,
          ...(pageFrame || {}),
          ...(pageSetup || {}),
        }));
        for (const it of res?.layoutLint?.issues || []) pushIssue(it);
      } catch {
        // IPC 미구현 모의 환경에서는 로컬 layout_lint 결과만 사용한다.
      }
    }
    return merged;
  }

  function applyDocOpsToClone(clone, operations, ctx) {
    const docHtml = host.__EASYOFFICE_COPILOT_DOC_HTML__;
    for (const op of operations) {
      const kind = String(op?.op || op?.type || op?.kind || "").trim();
      if (kind !== "doc.append" && kind !== "doc.replace_paragraphs" && kind !== "doc.replace_span") continue;
      const existing = ctx.docBlocksOf ? ctx.docBlocksOf(clone) : Array.from(clone.children || []);
      if (kind === "doc.replace_span") {
        const idx = Number(op?.index);
        if (!Number.isInteger(idx) || idx < 0 || idx >= existing.length) {
          throw new Error("대상을 찾을 수 없는 문단 인덱스입니다.");
        }
        const targetEl = existing[idx];
        if (docHtml?.replaceSpanWithTrackChanges) {
          docHtml.replaceSpanWithTrackChanges(ctx.document, clone, targetEl, op.search, op.replace);
        } else {
          const cleanSearch = String(op?.search ?? "");
          const cleanReplace = String(op?.replace ?? "");
          const cur = String(targetEl.textContent || "");
          const firstAt = cleanSearch ? cur.indexOf(cleanSearch) : -1;
          if (firstAt < 0 || cur.indexOf(cleanSearch, firstAt + 1) >= 0) {
            throw new Error("부분 치환 대상 문구를 유일하게 찾을 수 없습니다.");
          }
          targetEl.textContent = cur.slice(0, firstAt) + cleanReplace + cur.slice(firstAt + cleanSearch.length);
        }
        continue;
      }
      if (kind === "doc.replace_paragraphs" && Array.isArray(op.replacements)) {
        for (const rep of op.replacements) {
          const idx = Number(rep?.index);
          if (!Number.isInteger(idx) || idx < 0) continue;
          const nextEl = ctx.buildDocBlockElement(ctx.document, String(rep?.text ?? ""));
          if (!nextEl) continue;
          if (idx < existing.length && (existing[idx]?.parentNode === clone || existing[idx]?.parentElement === clone)) {
            clone.insertBefore(nextEl, existing[idx]);
            existing[idx].remove();
          } else {
            clone.appendChild(nextEl);
          }
        }
        continue;
      }
      const items = Array.isArray(op.blocks)
        ? op.blocks
        : Array.isArray(op.paragraphs)
          ? op.paragraphs
          : op.text !== undefined ? [String(op.text)] : [];
      if (kind === "doc.replace_paragraphs") {
        const start = Number.isInteger(op.startIndex) ? Math.max(0, op.startIndex) : 0;
        const end = Number.isInteger(op.endIndex) ? Math.min(existing.length, op.endIndex + 1) : existing.length;
        const anchor = existing[end] || null;
        for (let i = start; i < end; i += 1) existing[i]?.remove();
        for (const item of items) {
          const el = ctx.buildDocBlockElement(ctx.document, item);
          if (el) clone.insertBefore(el, anchor);
        }
      } else {
        if (existing.length === 1 && String(existing[0].textContent || "").trim() === "") {
          existing[0].remove();
        }
        for (const item of items) {
          const el = ctx.buildDocBlockElement(ctx.document, item);
          if (el) clone.appendChild(el);
        }
      }
    }
  }

  async function verifyProposedOperations(mod, operations, ctx) {
    if (!Array.isArray(operations) || operations.length === 0) return { ok: true, issues: [] };
    const lintApi = host.__EASYOFFICE_LAYOUT_LINT__;
    const newIssues = [];

    if (mod === "SLIDE") {
      const baseIssues = await collectSlideLintIssues(ctx, lintApi);
      const savedDeck = JSON.parse(JSON.stringify(ctx.slideDeckState));
      try {
        for (const op of operations) {
          const kind = String(op?.op || op?.type || op?.kind || "").trim();
          if (kind === "slide.add" || kind === "slide.update" || kind === "slide.add_element") {
            ctx.executeSlideOperation(kind, op, []);
          }
        }
        const afterIssues = await collectSlideLintIssues(ctx, lintApi);
        newIssues.push(...diffNewLintIssues(baseIssues, afterIssues));
      } catch (err) {
        newIssues.push(String(err?.message || err));
      } finally {
        ctx.slideDeckState.splice(0, ctx.slideDeckState.length, ...savedDeck);
      }
      return { ok: newIssues.length === 0, issues: newIssues };
    }

    if (mod === "HWP") {
      const snap = ctx.getHwpSnapshot?.();
      if (!snap) return { ok: true, issues: [] };
      const total = Number(snap.totalParagraphs || 0);
      const byIndex = snap.byIndex instanceof Map ? snap.byIndex : new Map();
      const hasHtmlTag = (text) => /<\/?[a-z][a-z0-9]*\b[^>]*>/i.test(String(text || ""));
      for (const op of operations) {
        const kind = String(op?.op || op?.type || op?.kind || "").trim();
        if (kind === "doc.replace_span") {
          const idx = Number(op?.index);
          if (!Number.isInteger(idx) || idx < 0 || idx >= total) {
            newIssues.push(`HWP 문단 인덱스 ${op?.index}가 범위(0..${total})를 벗어났습니다.`);
            continue;
          }
          const entry = byIndex.get(idx);
          if (entry?.hasControls) {
            newIssues.push(`HWP 문단 [${idx}]에는 표·그림 등 개체가 있어 부분 치환할 수 없습니다.`);
            continue;
          }
          const search = String(op?.search ?? "");
          const replace = String(op?.replace ?? "");
          if (!search.trim()) {
            newIssues.push(`HWP doc.replace_span(index=${idx}): search가 비어 있습니다.`);
            continue;
          }
          if (hasHtmlTag(replace)) {
            newIssues.push(`HWP doc.replace_span(index=${idx}): HWP 연산에 HTML 태그를 넣을 수 없습니다.`);
            continue;
          }
          if (entry && typeof entry.text === "string") {
            const at = entry.text.indexOf(search);
            if (at < 0) {
              newIssues.push(`HWP doc.replace_span(index=${idx}): search='${search.slice(0, 40)}'를 찾지 못했습니다.`);
            } else if (entry.text.indexOf(search, at + 1) >= 0) {
              newIssues.push(`HWP doc.replace_span(index=${idx}): search='${search.slice(0, 40)}'가 여러 번 일치합니다.`);
            }
          }
        } else if (kind === "doc.replace_paragraphs") {
          if (Array.isArray(op.replacements)) {
            for (const rep of op.replacements) {
              const idx = Number(rep?.index);
              if (!Number.isInteger(idx) || idx < 0 || idx >= total) {
                newIssues.push(`HWP doc.replace_paragraphs: 인덱스 ${rep?.index} 범위 초과(0..${total})`);
                continue;
              }
              if (byIndex.get(idx)?.hasControls) {
                newIssues.push(`HWP 문단 [${idx}] 개체 포함 문단은 수정할 수 없습니다.`);
              }
              if (hasHtmlTag(rep?.text)) {
                newIssues.push(`HWP 문단 [${idx}] 평문 전용(HTML 금지)`);
              }
            }
          } else {
            const start = Number.isInteger(op.startIndex) ? op.startIndex : 0;
            const end = Number.isInteger(op.endIndex) ? op.endIndex : total - 1;
            if (start < 0 || end < start || end >= total) {
              newIssues.push(`HWP doc.replace_paragraphs: 범위 [${start}..${end}] 오류(0..${total})`);
            } else {
              for (let i = start; i <= end; i += 1) {
                if (byIndex.get(i)?.hasControls) {
                  newIssues.push(`HWP 문단 [${i}] 개체 포함 문단은 범위 교체할 수 없습니다.`);
                  break;
                }
              }
            }
            const blocks = Array.isArray(op.blocks) ? op.blocks : [];
            if (blocks.some((b) => Boolean(b?.table || b?.rows))) {
              newIssues.push("HWP 문서는 doc.replace_paragraphs에 표 블록을 지원하지 않습니다.");
            }
            if (blocks.some((b) => hasHtmlTag(typeof b === "string" ? b : b?.text))) {
              newIssues.push("HWP doc.replace_paragraphs는 평문만 허용합니다(HTML 금지).");
            }
          }
        } else if (kind === "doc.append") {
          const blocks = Array.isArray(op.blocks) ? op.blocks : [];
          if (blocks.some((b) => Boolean(b?.table || b?.rows))) {
            newIssues.push("HWP 문서는 doc.append에 표 블록을 지원하지 않습니다.");
          }
          if (blocks.some((b) => hasHtmlTag(typeof b === "string" ? b : b?.text))) {
            newIssues.push("HWP doc.append는 평문만 허용합니다(HTML 금지).");
          }
        }
      }
      return { ok: newIssues.length === 0, issues: newIssues };
    }

    if (mod === "DOC") {
      const liveEditor = ctx.document?.getElementById?.("doc-editor");
      const clone = ctx.document.createElement("div");
      if (liveEditor?.dataset) {
        for (const [k, v] of Object.entries(liveEditor.dataset)) clone.dataset[k] = v;
      }
      const liveBlocks = liveEditor && ctx.docBlocksOf ? ctx.docBlocksOf(liveEditor) : [];
      for (const blk of liveBlocks) {
        if (typeof blk.cloneNode === "function") clone.appendChild(blk.cloneNode(true));
      }
      try {
        const baseIssues = await collectDocContainerLintIssues(clone, ctx, lintApi);
        applyDocOpsToClone(clone, operations, ctx);
        const afterIssues = await collectDocContainerLintIssues(clone, ctx, lintApi);
        newIssues.push(...diffNewLintIssues(baseIssues, afterIssues));
      } catch (err) {
        newIssues.push(String(err?.message || err));
      }
      return { ok: newIssues.length === 0, issues: newIssues };
    }

    const sheetOps = ctx.sheetOps;
    if (mod === "SHEET" && sheetOps?.collectSheetFormulaErrors && typeof ctx.invokeIpc === "function") {
      const needsEval = operations.some((op) => {
        const kind = String(op?.op || op?.type || op?.kind || "").trim();
        if (kind === "sheet.structure" && op?.action !== "freeze" && op?.action !== "unfreeze") return true;
        if (kind === "sheet.table" && (op?.action === "total_row" || ((op?.action || "create") === "create" && op?.totalRow === true))) return true;
        if (kind === "sheet.write" && Array.isArray(op.cells)) {
          return op.cells.some((c) => String(c?.raw ?? c?.formula ?? "").startsWith("="));
        }
        return false;
      });
      if (!needsEval) return { ok: true, issues: [] };
      const sc = ctx.getSheetController();
      const activeName = ctx.getActiveSheetName();
      const liveMap = sc?.snapshotDomSheetCells ? sc.snapshotDomSheetCells() : (ctx.sheetDataStore[activeName] || {});
      const candidateStore = JSON.parse(JSON.stringify({ ...ctx.sheetDataStore, [activeName]: liveMap }));
      const candidateRows = { ...ctx.sheetRowCounts };
      const baselineErrs = new Set(sheetOps.collectSheetFormulaErrors(candidateStore).map((e) => e.key));
      try {
        for (const op of operations) {
          const kind = String(op?.op || op?.type || op?.kind || "").trim();
          const sheetName = String(op.sheet || activeName);
          if (!Object.hasOwn(candidateStore, sheetName)) {
            if (sheetName !== activeName) throw new Error("없는 시트입니다: " + sheetName);
            candidateStore[sheetName] = Object.create(null);
          }
          if (kind === "sheet.write") {
            for (const c of Array.isArray(op.cells) ? op.cells : []) {
              const coord = String(c?.coord || "").trim().toUpperCase();
              if (!coord) continue;
              const rawStr = String(c.raw ?? c.formula ?? c.value ?? "");
              const isF = rawStr.startsWith("=");
              candidateStore[sheetName][coord] = { value: isF ? "" : rawStr, formula: isF ? rawStr : "" };
            }
          } else if (kind === "sheet.structure" || kind === "sheet.table") {
            sheetOps.executeExtendedSheetOp(kind, op, {
              sheetName,
              sheetDataStore: candidateStore,
              sheetRowCounts: candidateRows,
              locations: [],
            }, { dryRun: true });
          }
        }
        const cells = [];
        const targets = [];
        const normNum = host.__EASYOFFICE_SHEET_UTILS__?.normalizeFormattedNumericCell || ((v) => String(v ?? ""));
        for (const shName of Object.keys(candidateStore)) {
          const snap = candidateStore[shName] || {};
          for (const ref of Object.keys(snap)) {
            const entry = snap[ref];
            if (!entry) continue;
            const f = String(entry.formula || "");
            if (f.startsWith("=")) targets.push({ sheet: shName, cell: ref });
            if (f || entry.value) {
              const val = f ? f : entry.t === "s" ? String(entry.value) : normNum(entry.value);
              cells.push({ sheet: shName, cell: ref, value: val });
            }
          }
        }
        if (targets.length > 0 && cells.length <= MAX_DRY_RUN_EVAL_CELLS) {
          let rpcRes = null;
          try {
            rpcRes = await ctx.invokeIpc("evaluate_sheet", JSON.stringify({
              sheet: activeName,
              sheets: Object.keys(candidateStore),
              targets,
              cells,
            }));
          } catch {
            rpcRes = null;
          }
          if (rpcRes) {
            const tVals = Array.isArray(rpcRes?.targetValues) ? rpcRes.targetValues : null;
            const bVals = rpcRes?.values && typeof rpcRes.values === "object" ? rpcRes.values : null;
            targets.forEach((t, idx) => {
              const outVal = tVals && idx < tVals.length
                ? tVals[idx]
                : bVals ? (bVals[t.sheet + "!" + t.cell] ?? bVals[t.sheet.toUpperCase() + "!" + t.cell.toUpperCase()])
                  : (targets.length === 1 ? rpcRes?.value : undefined);
              if (outVal !== undefined && candidateStore[t.sheet]?.[t.cell]) {
                candidateStore[t.sheet][t.cell].value = String(outVal);
              }
            });
          }
        }
        for (const e of sheetOps.collectSheetFormulaErrors(candidateStore)) {
          if (!baselineErrs.has(e.key)) {
            newIssues.push(`${e.sheet}!${e.coord} 셀 수식 오류 ${e.error} (${e.formula})`);
          }
        }
      } catch (err) {
        newIssues.push(String(err?.message || err));
      }
    }
    return { ok: newIssues.length === 0, issues: newIssues };
  }

  function createCopilotAgentLoop(deps) {
    const {
      invokeIpc,
      canStreamIpc,
      streamApi,
      copilotActions,
    } = deps;
    const { clipCodePoints, clipUtf8Bytes } = host.__EASYOFFICE_COPILOT_TEXT__;

    function formatToolArgsSummary(tc) {
      if (!tc || typeof tc !== "object") return "";
      return Object.entries(tc)
        .filter(([k, v]) => k !== "name" && v !== undefined && v !== null && v !== "")
        .slice(0, 4)
        .map(([k, v]) => `${k}=${clipCodePoints(String(v), 48)}`)
        .join(",");
    }

    function formatDenseToolResult(name, result) {
      if (!result || typeof result !== "object") return String(result ?? "");
      if (typeof result.tsv === "string" && result.tsv) return result.tsv;
      if (Array.isArray(result.lines)) {
        const hdr = Number.isInteger(result.totalParagraphs)
          ? `[문단 전체 ${result.totalParagraphs}개]`
          : Number.isInteger(result.totalSlides)
            ? `[슬라이드 전체 ${result.totalSlides}장]`
            : result.query ? `[검색: ${result.query}${result.truncated ? ", 일부 생략" : ""}]` : "";
        return [hdr, ...result.lines].filter(Boolean).join("\n");
      }
      if (Array.isArray(result.blocks)) {
        const hdr = Number.isInteger(result.totalParagraphs) ? `[문단 전체 ${result.totalParagraphs}개]` : "";
        const ls = result.blocks.map((b) => `  [${b.index}] ${String(b.text ?? "").replace(/[\r\n]+/g, " ")}`);
        return [hdr, ...ls].filter(Boolean).join("\n");
      }
      if (Array.isArray(result.matches)) {
        return result.matches.map((m) => (m.page ? `[${m.page}쪽] ${m.snippet || ""}` : `${m.sheet || ""}!${m.coord || ""}: ${m.raw ?? m.value ?? ""}`)).join("\n");
      }
      return JSON.stringify(result);
    }

    async function executeToolCalls(mod, toolCalls, compactor) {
      const chunks = [];
      for (const tc of toolCalls) {
        if (!tc || typeof tc !== "object") continue;
        const name = String(tc.name || "");
        const argStr = formatToolArgsSummary(tc);
        try {
          if (compactor) {
            const entry = await compactor.runCachedReadTool(mod, tc, copilotActions.executeReadTool);
            if (entry.cached) {
              chunks.push(`[${name}(${argStr})] ${entry.receipt}`);
            } else {
              const body = formatDenseToolResult(name, entry.result);
              chunks.push(`[${name}(${argStr}) sha256=${entry.sha256.slice(0, 12)}]\n${body}`);
            }
          } else {
            const result = await copilotActions.executeReadTool(mod, name, tc);
            chunks.push(`[${name}(${argStr})]\n${formatDenseToolResult(name, result)}`);
          }
        } catch (err) {
          chunks.push(`[${name}(${argStr}) 오류] ${String(err?.message || err)}`);
        }
      }
      const combined = chunks.join("\n\n");
      const capped = clipCodePoints(combined, MAX_TOOL_OUTPUT_CHARS);
      return typeof clipUtf8Bytes === "function" ? clipUtf8Bytes(capped, MAX_TOOL_OUTPUT_BYTES) : capped;
    }

    async function runAgentTurn(params) {
      const {
        baseReq,
        newSubRequestId,
        isCancelled,
        onStreamText,
        onActiveRequestId,
        onToolStep,
      } = params;

      const streaming = Boolean(canStreamIpc?.());
      const compactApi = host.__EASYOFFICE_COPILOT_CONTEXT_COMPACT__;
      const priorHistory = compactApi
        ? compactApi.compactHistory(baseReq.history, MAX_HISTORY_SEND)
        : (Array.isArray(baseReq.history) ? baseReq.history.slice(-MAX_HISTORY_SEND) : []).map((h) => ({
          role: h?.role === "assistant" ? "assistant" : "user",
          text: typeof clipUtf8Bytes === "function"
            ? clipUtf8Bytes(String(h?.text ?? h?.content ?? ""), 16384)
            : String(h?.text ?? h?.content ?? ""),
        }));
      const loopSteps = [];
      const compactor = compactApi ? compactApi.createLoopCompactor() : null;
      let cumulativeBytes = 0;
      let totalDropped = 0;
      let dropRetries = 0;
      let lintRetries = 0;
      let finalRes = null;
      let hasUsage = false;
      const totalUsage = {
        inputTokens: 0,
        outputTokens: 0,
        cachedTokens: 0,
        cacheWriteTokens: 0,
        reasoningTokens: 0,
      };

      for (let turnIdx = 0; turnIdx < MAX_TURNS; turnIdx += 1) {
        if (isCancelled()) return null;
        const reqId = turnIdx === 0 ? baseReq.requestId : newSubRequestId(turnIdx);
        onActiveRequestId?.(reqId);

        const payloadObj = {
          ...baseReq,
          requestId: reqId,
          history: priorHistory,
        };
        if (loopSteps.length > 0) payloadObj.loopSteps = loopSteps;
        if (streaming) payloadObj.stream = true;
        const payloadJson = JSON.stringify(payloadObj);
        cumulativeBytes += utf8ByteLength(payloadJson);
        if (cumulativeBytes > MAX_CUMULATIVE_BYTES) {
          throw new Error("멀티턴 코파일럿 누적 요청 크기가 허용 상한(384KiB)을 초과했습니다.");
        }

        const session = streaming
          ? streamApi.createStreamSession({
            requestId: reqId,
            onText: (text) => onStreamText?.(text),
          })
          : null;

        const res = await invokeIpc(
          "run_smart_assistant",
          payloadJson,
          session ? { stream: { onPiece: (piece) => session.accept(piece) } } : undefined,
        );
        if (isCancelled()) return null;
        session?.finish();

        totalDropped += Number(res?.droppedOperations) || 0;
        if (res?.usage && typeof res.usage === "object") {
          hasUsage = true;
          totalUsage.inputTokens += Number(res.usage.inputTokens) || 0;
          totalUsage.outputTokens += Number(res.usage.outputTokens) || 0;
          totalUsage.cachedTokens += Number(res.usage.cachedTokens) || 0;
          totalUsage.cacheWriteTokens += Number(res.usage.cacheWriteTokens) || 0;
          totalUsage.reasoningTokens += Number(res.usage.reasoningTokens) || 0;
        }
        finalRes = res;

        const toolCalls = Array.isArray(res?.toolCalls) ? res.toolCalls : [];
        const ops = Array.isArray(res?.operations) ? res.operations : [];

        // 1. On-demand read tool execution when the model asked to inspect more context first.
        if (toolCalls.length > 0 && ops.length === 0 && turnIdx + 1 < MAX_TURNS) {
          const toolJson = await executeToolCalls(baseReq.module, toolCalls, compactor);
          if (isCancelled()) return null;
          const describeToolCall = host.__EASYOFFICE_COPILOT_STEP_LABELS__.describeToolCall;
          for (const tc of toolCalls) onToolStep?.(describeToolCall(tc));
          const calledNames = toolCalls
            .map((tc) => `${String(tc?.name || "")}(${formatToolArgsSummary(tc)})`)
            .join(", ");
          const stepAssistant = res?.reply
            ? `${String(res.reply).trim()}\n[조회 호출] ${calledNames}`
            : `[조회 호출] ${calledNames}`;
          const stepUser = `[도구 실행 결과]\n${toolJson}\n위 조회 결과를 바탕으로 최종 답변·연산을 작성하십시오.`;
          if (compactor) {
            compactor.pushStepPair(loopSteps, stepAssistant, stepUser);
          } else {
            loopSteps.push({ role: "assistant", text: stepAssistant });
            loopSteps.push({ role: "user", text: stepUser });
          }
          continue;
        }

        // 2. Rust droppedOperations feedback retry loop when engine returned droppedReasons.
        if (
          baseReq.mode === "edit"
          && ops.length === 0
          && (Number(res?.droppedOperations) || 0) > 0
          && Array.isArray(res?.droppedReasons)
          && res.droppedReasons.length > 0
          && dropRetries < MAX_LINT_RETRIES
          && !baseReq.command
          && turnIdx + 1 < MAX_TURNS
        ) {
          dropRetries += 1;
          if (compactor?.summarizeDroppedOperations) {
            const pair = compactor.summarizeDroppedOperations(res);
            compactor.pushStepPair(loopSteps, pair.assistantText, pair.userText);
          } else {
            const reasons = res.droppedReasons.slice(0, 8).join("; ");
            loopSteps.push({ role: "assistant", text: String(res?.reply || "연산 검증 재시도") });
            loopSteps.push({ role: "user", text: `[엔진 연산 검증 거부] ${reasons}. 유효한 연산을 다시 작성하십시오.` });
          }
          continue;
        }

        // 3. Deterministic post-verification gate before presenting edit preview (DOC / HWP / SLIDE / SHEET).
        if (
          baseReq.mode === "edit"
          && ops.length > 0
          && !baseReq.command
          && lintRetries < MAX_LINT_RETRIES
          && turnIdx + 1 < MAX_TURNS
          && (baseReq.module === "DOC" || baseReq.module === "HWP" || baseReq.module === "SLIDE" || baseReq.module === "SHEET")
        ) {
          const normalizedOps = copilotActions.normalizeOperations(baseReq.module, ops, params.frozenSelection);
          const gate = await copilotActions.dryRunVerifyOperations(baseReq.module, normalizedOps, invokeIpc);
          if (isCancelled()) return null;
          if (gate && !gate.ok && gate.issues.length > 0) {
            lintRetries += 1;
            if (compactor) {
              const pair = compactor.summarizeValidationFailure(res, normalizedOps, gate.issues);
              compactor.pushStepPair(loopSteps, pair.assistantText, pair.userText);
            } else {
              const summary = gate.issues.slice(0, 6).join("; ");
              loopSteps.push({
                role: "assistant",
                text: JSON.stringify({ reply: res?.reply || "", operations: normalizedOps }),
              });
              loopSteps.push({
                role: "user",
                text: `[자동 검증 게이트 오류] ${summary}. 좌표·크기·수식 참조만 수정해 연산을 다시 생성하십시오.`,
              });
            }
            continue;
          }
        }

        break;
      }

      if (!finalRes) return null;
      const lastContextTokens = finalRes?.usage && typeof finalRes.usage === "object"
        ? (Number(finalRes.usage.inputTokens) || 0) + (Number(finalRes.usage.outputTokens) || 0)
        : 0;
      return {
        ...finalRes,
        droppedOperations: totalDropped,
        ...(hasUsage ? { usage: totalUsage, lastContextTokens } : {}),
      };
    }

    return {
      MAX_TURNS,
      MAX_CUMULATIVE_BYTES,
      runAgentTurn,
    };
  }

  host.__EASYOFFICE_COPILOT_VERIFY_OPS__ = verifyProposedOperations;
  host.__EASYOFFICE_CREATE_COPILOT_AGENT_LOOP__ = createCopilotAgentLoop;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
