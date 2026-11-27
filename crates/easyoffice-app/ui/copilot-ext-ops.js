// HWP · PDF · BASE · BOARD 코파일럿 통합 실행기, 미리보기·지문·위치 이동 및 온디맨드 읽기 도구 (PD-360).

(function (host) {
  "use strict";

  const MAX_CONTEXT_PARAGRAPHS = 200;
  const MAX_CONTEXT_TEXT_CHARS = 24000;
  const MAX_PDF_SEARCH_HITS = 20;
  const HWP_SURROUNDING_WINDOW = 16;

  function plainTextFromDocItem(item) {
    const docHtml = host.__EASYOFFICE_COPILOT_DOC_HTML__;
    if (docHtml?.plainPreviewTextOfBlock) return docHtml.plainPreviewTextOfBlock(item);
    if (typeof item === "string") return item.replace(/<[^>]+>/g, "");
    if (!item || typeof item !== "object") return "";
    if (Array.isArray(item.rows)) return item.rows.map((r) => (Array.isArray(r) ? r.join("\t") : "")).join("\n");
    return String(item.text ?? item.content ?? "").replace(/<[^>]+>/g, "");
  }

  function createCopilotExtOps(deps) {
    const {
      document,
      getHwpHost,
      getPdfEdit,
      extractEditableDocumentText,
      getActiveModule,
      activatePane,
      sheetDataStore,
      getActiveSheetName,
      getSheetController,
      getWorkspacePanels,
      slideDeckState,
    } = deps;
    const { clipCodePoints } = host.__EASYOFFICE_COPILOT_TEXT__;

    // Most recent HWP read snapshot for synchronous selection badge, preview diff, and fingerprint checks.
    let cachedHwpSnapshot = null;
    let boundHwpHost = null;

    function ensureHwpHostListeners(h) {
      if (!h || h === boundHwpHost) return;
      boundHwpHost = h;
      h.onChange?.((ev) => {
        const gen = Number(ev?.payload?.documentGeneration);
        if (cachedHwpSnapshot && Number.isFinite(gen) && gen >= 0) {
          cachedHwpSnapshot.documentGeneration = gen;
        } else if (cachedHwpSnapshot && ev?.payload?.reason !== "dirty_changed") {
          cachedHwpSnapshot.documentGeneration += 1;
        }
        void refreshHwpSnapshot().catch(() => {});
      });
      h.onStatus?.((status) => {
        if (status === "ready") void refreshHwpSnapshot().catch(() => {});
        else cachedHwpSnapshot = null;
      });
    }

    function getEmbedHwpHost() {
      const h = getHwpHost?.();
      ensureHwpHostListeners(h);
      if (!h || h.usesLegacy?.() || h.getStatus?.() !== "ready") return null;
      return h;
    }
    ensureHwpHostListeners(getHwpHost?.());

    async function refreshHwpSnapshot(span = null) {
      const hwp = getEmbedHwpHost();
      if (!hwp) {
        cachedHwpSnapshot = null;
        return null;
      }
      const head = await hwp.call("readParagraphRange", { startIndex: 0, count: MAX_CONTEXT_PARAGRAPHS });
      const byIndex = new Map();
      for (const p of head?.paragraphs || []) {
        byIndex.set(p.index, p);
      }
      const sel = head?.selection || { hasSelection: false, text: "", startParagraphIndex: 0, endParagraphIndex: 0 };
      const targetStart = span ? span.start : (sel.hasSelection ? sel.startParagraphIndex : -1);
      const targetEnd = span ? span.end : (sel.hasSelection ? sel.endParagraphIndex : -1);
      const missingTarget = targetStart >= 0 && (!byIndex.has(targetStart) || (targetEnd >= targetStart && !byIndex.has(targetEnd)));

      if ((targetEnd >= MAX_CONTEXT_PARAGRAPHS || (head?.truncatedByBytes && missingTarget)) && head?.totalParagraphs > 0) {
        const winStart = Math.max(0, targetStart - HWP_SURROUNDING_WINDOW);
        const winEnd = Math.min(head.totalParagraphs - 1, targetEnd + HWP_SURROUNDING_WINDOW);
        const count = Math.min(MAX_CONTEXT_PARAGRAPHS, Math.max(1, winEnd - winStart + 1));
        const extra = await hwp.call("readParagraphRange", { startIndex: winStart, count });
        for (const p of extra?.paragraphs || []) {
          byIndex.set(p.index, p);
        }
      }

      cachedHwpSnapshot = {
        documentGeneration: Number(head?.documentGeneration || 0),
        pageCount: Number(head?.pageCount || 1),
        totalParagraphs: Number(head?.totalParagraphs || 0),
        truncatedByBytes: Boolean(head?.truncatedByBytes),
        selection: sel,
        byIndex,
      };
      return cachedHwpSnapshot;
    }

    function buildHwpContextFromSnapshot(snap, target) {
      if (!snap) return { selection: {}, context: { doc: { paragraphs: [], selectedText: "", omittedParagraphs: 0 } } };
      const sel = target?.selection;
      const hasSel = sel
        ? true
        : Boolean(snap.selection?.hasSelection && snap.selection.text);
      const spanStart = sel ? sel.paragraphIndex : (hasSel ? snap.selection.startParagraphIndex : -1);
      const spanEnd = sel ? sel.paragraphEnd : (hasSel ? snap.selection.endParagraphIndex : -1);
      const winEnd = spanEnd >= 0 ? Math.min(snap.totalParagraphs - 1, spanEnd + HWP_SURROUNDING_WINDOW) : -1;
      const lastSent = Math.min(2047, Math.max(MAX_CONTEXT_PARAGRAPHS - 1, winEnd));
      const count = Math.min(snap.totalParagraphs, lastSent + 1);
      const winStart = spanStart >= 0 ? Math.max(0, spanStart - HWP_SURROUNDING_WINDOW) : count;

      const paragraphs = [];
      for (let i = 0; i < count; i += 1) {
        const entry = snap.byIndex.get(i);
        const keep = i < MAX_CONTEXT_PARAGRAPHS || (i >= winStart && i <= winEnd);
        paragraphs.push(keep && snap.byIndex.has(i) ? clipCodePoints(entry?.text || "", MAX_CONTEXT_TEXT_CHARS) : null);
      }
      let trimmedTail = 0;
      while (paragraphs.length > 0 && paragraphs[paragraphs.length - 1] === null) {
        paragraphs.pop();
        trimmedTail += 1;
      }

      const selectedText = sel?.selectedText
        ? String(sel.selectedText)
        : (snap.selection?.hasSelection ? clipCodePoints(snap.selection.text || "", MAX_CONTEXT_TEXT_CHARS) : "");
      const selectionPayload = sel
        ? { range: sel.range, paragraphIndex: sel.paragraphIndex, paragraphEnd: sel.paragraphEnd }
        : hasSel
          ? {
            range: "선택 텍스트 " + Math.min(Array.from(selectedText.trim()).length, 999) + "자",
            paragraphIndex: snap.selection.startParagraphIndex,
            paragraphEnd: snap.selection.endParagraphIndex,
          }
          : {};

      return {
        selection: selectionPayload,
        context: {
          doc: {
            paragraphs,
            selectedText,
            omittedParagraphs: Math.max(0, snap.totalParagraphs - count) + trimmedTail,
          },
        },
      };
    }

    async function extractHwpContextAsync(target) {
      const span = target?.selection
        ? { start: target.selection.paragraphIndex, end: target.selection.paragraphEnd }
        : null;
      const snap = await refreshHwpSnapshot(span);
      return buildHwpContextFromSnapshot(snap, target);
    }

    function getHwpSelectionLabel() {
      const sel = cachedHwpSnapshot?.selection;
      if (!sel?.hasSelection || !sel.text) return "";
      const len = Array.from(String(sel.text).trim()).length;
      return len > 0 ? "선택 텍스트 " + Math.min(len, 999) + "자" : "";
    }

    function getHwpCommandTarget() {
      const sel = cachedHwpSnapshot?.selection;
      if (!sel?.hasSelection || !sel.text || sel.endParagraphIndex > 2047) return null;
      const len = Array.from(String(sel.text).trim()).length;
      if (len === 0) return null;
      return {
        selection: {
          range: "선택 텍스트 " + Math.min(len, 999) + "자",
          paragraphIndex: sel.startParagraphIndex,
          paragraphEnd: sel.endParagraphIndex,
          selectedText: sel.text,
        },
      };
    }

    function hwpEntries(op, entries, change) {
      const kind = String(op.op || op.type || op.kind || "");
      const byIndex = cachedHwpSnapshot?.byIndex || new Map();
      const total = cachedHwpSnapshot?.totalParagraphs || 0;
      if (kind === "doc.replace_span") {
        const idx = Number(op?.index);
        if (!Number.isInteger(idx) || idx < 0) return;
        const before = byIndex.get(idx)?.text ?? "";
        const search = String(op?.search ?? "");
        const replace = String(op?.replace ?? "");
        const at = search ? before.indexOf(search) : -1;
        const after = at >= 0 ? before.slice(0, at) + replace + before.slice(at + search.length) : replace;
        entries.push(change(idx < total ? "replace" : "insert", "문단 " + (idx + 1), before, after, { module: "HWP", paragraphIndex: idx }));
        return;
      }
      if (kind === "doc.replace_paragraphs" && Array.isArray(op.replacements)) {
        for (const rep of op.replacements) {
          const idx = Number(rep?.index);
          if (!Number.isInteger(idx) || idx < 0) continue;
          const before = byIndex.get(idx)?.text ?? "";
          const after = plainTextFromDocItem(String(rep?.text ?? ""));
          entries.push(change(idx < total ? "replace" : "insert", "문단 " + (idx + 1), before, after, { module: "HWP", paragraphIndex: idx }));
        }
        return;
      }
      const items = Array.isArray(op.blocks) ? op.blocks : Array.isArray(op.paragraphs) ? op.paragraphs : op.text !== undefined ? [String(op.text)] : [];
      const afterText = items.map(plainTextFromDocItem).join("\n");
      if (kind === "doc.replace_paragraphs") {
        const start = Number.isInteger(op.startIndex) ? Math.max(0, op.startIndex) : 0;
        const end = Number.isInteger(op.endIndex) ? Math.max(start, op.endIndex) : Math.max(0, total - 1);
        const beforeParts = [];
        for (let i = start; i <= end; i += 1) beforeParts.push(byIndex.get(i)?.text ?? "");
        entries.push(change("replace", "문단 " + (start + 1) + (end > start ? "~" + (end + 1) : ""), beforeParts.join("\n"), afterText, { module: "HWP", paragraphIndex: start }));
        return;
      }
      entries.push(change("insert", "문서 끝에 추가", "", afterText, { module: "HWP", paragraphIndex: Math.max(0, total - 1) }));
    }

    function hwpFingerprint() {
      if (!cachedHwpSnapshot) return "hwp:unloaded";
      return `hwp:${cachedHwpSnapshot.documentGeneration}:${cachedHwpSnapshot.totalParagraphs}`;
    }

    async function verifyHwpFingerprintAsync(expectedFp) {
      const hwp = getEmbedHwpHost();
      if (!hwp) return false;
      const state = await hwp.call("readParagraphRange", { startIndex: 0, count: 1 });
      const actual = `hwp:${Number(state?.documentGeneration || 0)}:${Number(state?.totalParagraphs || 0)}`;
      return actual === expectedFp;
    }

    async function applyHwpOperations(operations) {
      const hwp = getEmbedHwpHost();
      if (!hwp) throw new Error("한글 편집기가 아직 준비되지 않았습니다.");
      const steps = [];
      const locations = [];

      for (const op of operations) {
        const kind = String(op?.op || op?.type || op?.kind || "").trim();
        if (kind === "doc.replace_span") {
          const idx = Number(op?.index);
          if (!Number.isInteger(idx) || idx < 0) throw new Error("한글 부분 교체 대상 문단 번호가 올바르지 않습니다.");
          const search = plainTextFromDocItem(String(op?.search ?? ""));
          const replace = plainTextFromDocItem(String(op?.replace ?? ""));
          steps.push({ kind: "replace_span", index: idx, search, replace });
          locations.push({ module: "HWP", label: "문단 " + (idx + 1), paragraphIndex: idx });
        } else if (kind === "doc.replace_paragraphs" && Array.isArray(op.replacements)) {
          const replacements = op.replacements.map((r) => {
            const idx = Number(r?.index);
            locations.push({ module: "HWP", label: "문단 " + (idx + 1), paragraphIndex: idx });
            return { index: idx, text: plainTextFromDocItem(String(r?.text ?? "")) };
          });
          steps.push({ kind: "replace_paragraphs", replacements });
        } else if (kind === "doc.replace_paragraphs") {
          const items = Array.isArray(op.blocks) ? op.blocks : Array.isArray(op.paragraphs) ? op.paragraphs : [String(op.text ?? "")];
          const texts = items.map(plainTextFromDocItem);
          const startIndex = Number.isInteger(op.startIndex) ? Math.max(0, op.startIndex) : 0;
          const endIndex = Number.isInteger(op.endIndex) ? Math.max(startIndex, op.endIndex) : Math.max(0, (cachedHwpSnapshot?.totalParagraphs || 1) - 1);
          steps.push({ kind: "replace_range", startIndex, endIndex, texts });
          locations.push({ module: "HWP", label: "문단 " + (startIndex + 1), paragraphIndex: startIndex });
        } else if (kind === "doc.append") {
          const items = Array.isArray(op.blocks) ? op.blocks : Array.isArray(op.paragraphs) ? op.paragraphs : [String(op.text ?? "")];
          const texts = items.map(plainTextFromDocItem);
          steps.push({ kind: "append", texts });
        } else {
          throw new Error("한글 편집기에서 지원하지 않는 코파일럿 작업입니다: " + kind);
        }
      }

      const expectedDocumentGeneration = cachedHwpSnapshot ? cachedHwpSnapshot.documentGeneration : undefined;
      const res = await hwp.call("applyCopilotEdits", {
        ...(expectedDocumentGeneration !== undefined ? { expectedDocumentGeneration } : {}),
        steps,
      });
      await refreshHwpSnapshot();
      if (locations.length === 0 && res?.focusTarget) {
        const gIdx = Number(res.focusTarget.globalIndex || 0);
        locations.push({ module: "HWP", label: "문단 " + (gIdx + 1), paragraphIndex: gIdx });
      }
      return {
        applied: true,
        locations,
        afterDocumentGeneration: Number(res?.documentGeneration || cachedHwpSnapshot?.documentGeneration || 0),
      };
    }

    function canUndoHwpSync(expectedAfterGeneration) {
      if (!getEmbedHwpHost() || !cachedHwpSnapshot) return false;
      return cachedHwpSnapshot.documentGeneration === expectedAfterGeneration;
    }

    async function canUndoHwpTurnAsync(expectedAfterGeneration) {
      const hwp = getEmbedHwpHost();
      if (!hwp) return false;
      const state = await hwp.call("readParagraphRange", { startIndex: 0, count: 1 });
      return Number(state?.documentGeneration || -1) === expectedAfterGeneration;
    }

    async function undoHwpTurn(expectedAfterGeneration) {
      const hwp = getEmbedHwpHost();
      if (!hwp) return false;
      if (!(await canUndoHwpTurnAsync(expectedAfterGeneration))) return false;
      await hwp.call("automation.execute", { id: "edit:undo" });
      await refreshHwpSnapshot();
      return true;
    }

    async function locateHwpTarget(loc) {
      const hwp = getEmbedHwpHost();
      if (!hwp) return false;
      const idx = Number.isInteger(loc.paragraphIndex) ? loc.paragraphIndex : 0;
      const snap = cachedHwpSnapshot || (await refreshHwpSnapshot({ start: idx, end: idx }));
      let entry = snap?.byIndex?.get(idx);
      if (!entry) {
        const single = await hwp.call("readParagraphRange", { startIndex: idx, count: 1 });
        entry = single?.paragraphs?.[0];
      }
      if (!entry) return false;
      await hwp.call("focusTarget", {
        target: {
          kind: "body_paragraph",
          section: entry.section,
          paragraph: entry.paragraph,
          charOffset: 0,
          length: Array.from(entry.text || "").length,
        },
      });
      return true;
    }

    async function readDocBlocksTool(mod, args, docBlocksOf) {
      const startIndex = Math.max(0, Number(args.startIndex) || 0);
      const rawSpan = Number.isInteger(args.endIndex) ? (args.endIndex - startIndex + 1) : (Number(args.count) || 50);
      const count = Math.max(1, Math.min(200, rawSpan));
      if (mod === "HWP" && getEmbedHwpHost()) {
        const res = await getEmbedHwpHost().call("readParagraphRange", { startIndex, count });
        return {
          totalParagraphs: res?.totalParagraphs || 0,
          blocks: (res?.paragraphs || []).map((p) => ({ index: p.index, text: p.text })),
        };
      }
      const docHtml = host.__EASYOFFICE_COPILOT_DOC_HTML__;
      const editor = document.getElementById("doc-editor");
      const blocks = docBlocksOf(editor);
      const slice = blocks.slice(startIndex, startIndex + count).map((el, offset) => ({
        index: startIndex + offset,
        text: docHtml ? docHtml.serializeBlockForContext(el) : String(el.textContent || "").trim(),
      }));
      return { totalParagraphs: blocks.length, blocks: slice };
    }

    // ---- PDF context, page search & page locate ----------------------------------------------------

    function extractPdfContext() {
      const hooks = host.__EASYOFFICE_MODULE_FIND_HOOKS__;
      const pages = hooks?.getPdfPages?.() || [];
      const activePage = Math.max(1, Number(document.getElementById("pdf-page-card")?.dataset?.activePage || 1) || 1);
      if (Array.isArray(pages) && pages.length > 0) {
        const formatted = pages
          .map((p, idx) => {
            const pageNo = Number(p?.page) || idx + 1;
            const txt = typeof p === "string" ? p : String(p?.text ?? "");
            return `[${pageNo}쪽] ${txt.trim()}`;
          })
          .join("\n\n");
        return {
          text: clipCodePoints(formatted, MAX_CONTEXT_TEXT_CHARS),
          pageCount: pages.length,
          activePage: Math.min(activePage, pages.length),
        };
      }
      const rawText = clipCodePoints(
        String(extractEditableDocumentText?.("PDF") || document.getElementById("pdf-page-card")?.textContent || "").trim(),
        MAX_CONTEXT_TEXT_CHARS,
      );
      const pageCount = Math.max(1, Number(getPdfEdit?.()?.pageCount?.() || pages.length || 1) || 1);
      return { text: rawText ? `[1쪽] ${rawText}` : "", pageCount, activePage: Math.min(activePage, pageCount) };
    }

    function searchPdfPagesTool(args) {
      const query = String(args?.query || "").trim();
      if (!query) return { query: "", matches: [] };
      const qLower = query.toLowerCase();
      const hooks = host.__EASYOFFICE_MODULE_FIND_HOOKS__;
      const pages = hooks?.getPdfPages?.() || [];
      const sourcePages = Array.isArray(pages) && pages.length > 0
        ? pages.map((p, i) => ({ page: Number(p?.page) || i + 1, text: typeof p === "string" ? p : String(p?.text ?? "") }))
        : [{ page: 1, text: String(extractEditableDocumentText?.("PDF") || document.getElementById("pdf-page-card")?.textContent || "") }];

      const matches = [];
      for (const entry of sourcePages) {
        const lower = entry.text.toLowerCase();
        const pos = lower.indexOf(qLower);
        if (pos < 0) continue;
        const start = Math.max(0, pos - 60);
        const end = Math.min(entry.text.length, pos + query.length + 100);
        matches.push({
          page: entry.page,
          snippet: entry.text.slice(start, end).replaceAll(/\s+/g, " ").trim(),
        });
        if (matches.length >= MAX_PDF_SEARCH_HITS) break;
      }
      return { query, totalPages: sourcePages.length, matches };
    }

    function locatePdfTarget(loc) {
      const pageNum = Number.isInteger(loc.page)
        ? loc.page
        : Number.parseInt(String(loc.label || "").replace(/\D+/g, ""), 10) || 1;
      const hooks = host.__EASYOFFICE_MODULE_FIND_HOOKS__;
      if (typeof hooks?.goToPdfPage === "function") {
        hooks.goToPdfPage(pageNum);
      } else {
        const card = document.getElementById("pdf-page-card");
        if (card) card.dataset.activePage = String(pageNum);
      }
      document.getElementById("pdf-page-card")?.focus?.();
      return true;
    }

    // ---- BASE operations, preview & fingerprint ----------------------------------------------------

    function coerceBaseFieldRawValue(field, rawStr) {
      if (field.type === "checkbox") {
        const low = String(rawStr).trim().toLowerCase();
        return low === "true" || low === "1" || low === "yes" || low === "예" || low === "o";
      }
      if (field.type === "multiSelect") {
        return String(rawStr)
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean);
      }
      return String(rawStr);
    }

    function applyBaseOperationsBatch(operations, locations) {
      const bb = host.__EASYOFFICE_BASE_BOARD_HOST__;
      const baseStudio = bb?.getBaseStudio?.();
      const M = host.__EASYOFFICE_BASE_MODEL__;
      if (!baseStudio || !M) throw new Error("데이터베이스 모듈을 사용할 수 없습니다.");

      const working = structuredClone(baseStudio.getState());
      const table = M.getTable(working, working.activeTableId) || working.tables?.[0];
      if (!table) throw new Error("활성 테이블을 찾을 수 없습니다.");

      for (const op of operations) {
        const kind = String(op.op || op.type || op.kind || "").trim();
        if (kind === "base.add_field") {
          const spec = {
            name: String(op.name || "").trim(),
            type: String(op.fieldType || "text").trim(),
            ...(Array.isArray(op.options) && op.options.length > 0
              ? { options: op.options.map((o) => String(o).trim()).filter(Boolean) }
              : {}),
          };
          const res = M.addField(working, table.id, spec);
          if (!res.ok) throw new Error(res.error);
          locations.push({ module: "BASE", label: `${table.name} · ${res.field.name}`, range: table.name });
        } else if (kind === "base.add_records") {
          const records = Array.isArray(op.records) ? op.records : [];
          for (const recObj of records) {
            if (!recObj || typeof recObj !== "object") continue;
            const byIdValues = {};
            for (const [fName, rawVal] of Object.entries(recObj)) {
              const fld = table.fields.find((f) => f.name === fName || f.name.toLowerCase() === fName.toLowerCase());
              if (!fld || fld.type === "formula") continue;
              if ((fld.type === "select" || fld.type === "multiSelect") && Array.isArray(fld.options)) {
                const tokens = fld.type === "multiSelect" ? coerceBaseFieldRawValue(fld, rawVal) : [String(rawVal).trim()];
                for (const tok of tokens) {
                  if (tok && !fld.options.includes(tok)) {
                    fld.options.push(tok);
                  }
                }
              }
              byIdValues[fld.id] = coerceBaseFieldRawValue(fld, rawVal);
            }
            const res = M.addRecord(working, table.id, byIdValues);
            if (!res.ok) throw new Error(res.error);
          }
          locations.push({ module: "BASE", label: `${table.name} (+${records.length}건)`, range: table.name });
        } else if (kind === "base.update_records") {
          const q = M.queryTableView(working, table.id);
          const edits = [];
          for (const item of Array.isArray(op.updates) ? op.updates : []) {
            const rowIdx = Number(item?.rowIndex);
            const targetRow = q.rows[rowIdx] || table.records[rowIdx];
            if (!targetRow) throw new Error(`레코드 행(${rowIdx + 1})을 찾을 수 없습니다.`);
            const recId = targetRow.record ? targetRow.record.id : targetRow.id;
            for (const [fName, rawVal] of Object.entries(item?.values || {})) {
              const fld = table.fields.find((f) => f.name === fName || f.name.toLowerCase() === fName.toLowerCase());
              if (!fld || fld.type === "formula") continue;
              edits.push({ recordId: recId, fieldId: fld.id, value: coerceBaseFieldRawValue(fld, rawVal) });
            }
          }
          if (edits.length > 0) {
            const res = M.updateCells(working, table.id, edits);
            if (!res.ok) throw new Error(res.error);
          }
          locations.push({ module: "BASE", label: table.name, range: table.name });
        } else if (kind === "base.set_view") {
          if (op.view && M.VIEW_MODES.has(op.view)) table.view = op.view;
          const groupKey = op.groupField ?? op.groupBy;
          if (groupKey !== undefined) {
            const gf = table.fields.find((f) => f.name === groupKey || f.name.toLowerCase() === String(groupKey).toLowerCase());
            table.groupByFieldId = gf ? gf.id : "";
          }
          if (op.sortField) {
            const sf = table.fields.find((f) => f.name === op.sortField || f.name.toLowerCase() === String(op.sortField).toLowerCase());
            if (sf) table.sorts = [{ fieldId: sf.id, dir: op.sortDir === "desc" ? "desc" : "asc" }];
          }
          if (op.filterField && op.filterOp) {
            const ff = table.fields.find((f) => f.name === op.filterField || f.name.toLowerCase() === String(op.filterField).toLowerCase());
            if (ff) {
              const normOp = op.filterOp === "not_empty" ? "notEmpty" : String(op.filterOp);
              table.filters = [{ fieldId: ff.id, op: normOp, value: String(op.filterValue ?? "") }];
            }
          }
          locations.push({ module: "BASE", label: `${table.name} 보기 설정`, range: table.name });
        } else {
          throw new Error("지원하지 않는 데이터베이스 작업입니다: " + kind);
        }
      }

      const hydrated = M.hydrateBaseState(working);
      if (!hydrated.ok) throw new Error(hydrated.error);
      baseStudio.setState(hydrated.state);
      return true;
    }

    const BASE_VIEW_LABELS = Object.freeze({
      grid: "데이터시트 보기",
      kanban: "보드",
      calendar: "일정",
    });

    function baseEntries(op, entries, change) {
      const kind = String(op.op || op.type || op.kind || "");
      const loc = { module: "BASE", label: "현재 테이블" };
      const fieldLabels = host.__EASYOFFICE_BASE_MODEL__?.FIELD_TYPE_LABELS || {};
      if (kind === "base.add_field") {
        const fType = String(op.fieldType || "text");
        entries.push(change("insert", `필드 추가: ${op.name || ""}`, "", fieldLabels[fType] || fType, loc));
      } else if (kind === "base.add_records") {
        const count = Array.isArray(op.records) ? op.records.length : 0;
        const preview = (op.records || []).slice(0, 3).map((r) => JSON.stringify(r)).join("\n");
        entries.push(change("insert", `레코드 ${count}건 추가`, "", preview, loc));
      } else if (kind === "base.update_records") {
        const count = Array.isArray(op.updates) ? op.updates.length : 0;
        entries.push(change("replace", `레코드 ${count}건 수정`, "", JSON.stringify((op.updates || []).slice(0, 3)), loc));
      } else if (kind === "base.set_view") {
        const viewLabel = op.view ? (BASE_VIEW_LABELS[op.view] || op.view) : "";
        const detail = [viewLabel, op.sortField, op.filterField].filter(Boolean).join(" · ");
        entries.push(change("format", detail ? `보기 설정 (${detail})` : "보기·정렬·필터 변경", "", "", loc));
      }
    }

    function baseFingerprint() {
      const st = host.__EASYOFFICE_BASE_BOARD_HOST__?.getBaseStudio?.()?.getState?.();
      return st ? JSON.stringify(st) : "";
    }

    // ---- BOARD graph layout + orthogonal router (PD-260) -------------------------------------------

    function computeLayeredGraphCoordinates(specNodes, specEdges, existingNodes, direction = "LR") {
      const router = host.__EASYOFFICE_BOARD_ROUTER__;
      const analysis = router?.analyzeBoard ? router.analyzeBoard({ nodes: existingNodes, connectors: [] }) : null;
      const baseX = analysis?.bounds ? Math.round(analysis.bounds.minX) : 80;
      const baseY = analysis?.bounds && existingNodes.length > 0 ? Math.round(analysis.bounds.maxY + 96) : 80;

      const idToIdx = new Map();
      specNodes.forEach((n, idx) => idToIdx.set(String(n.id), idx));
      const inDeg = new Int32Array(specNodes.length);
      const adj = Array.from({ length: specNodes.length }, () => []);

      for (const e of specEdges) {
        const u = idToIdx.get(String(e.from));
        const v = idToIdx.get(String(e.to));
        if (u !== undefined && v !== undefined && u !== v) {
          adj[u].push(v);
          inDeg[v] += 1;
        }
      }

      // BFS longest-path rank assignment with cycle guard.
      const rank = new Int32Array(specNodes.length);
      const queue = [];
      for (let i = 0; i < specNodes.length; i += 1) {
        if (inDeg[i] === 0) queue.push(i);
      }
      if (queue.length === 0 && specNodes.length > 0) queue.push(0);

      const visited = new Uint8Array(specNodes.length);
      let head = 0;
      while (head < queue.length) {
        const u = queue[head++];
        visited[u] = 1;
        for (const v of adj[u]) {
          if (!visited[v]) {
            rank[v] = Math.min(specNodes.length - 1, Math.max(rank[v], rank[u] + 1));
          }
          inDeg[v] -= 1;
          if (inDeg[v] <= 0 && !visited[v]) {
            visited[v] = 1;
            queue.push(v);
          }
        }
      }
      for (let i = 0; i < specNodes.length; i += 1) {
        if (!visited[i]) rank[i] = 0;
      }

      const layers = new Map();
      for (let i = 0; i < specNodes.length; i += 1) {
        const r = rank[i];
        if (!layers.has(r)) layers.set(r, []);
        layers.get(r).push(i);
      }

      const NODE_W = 164;
      const NODE_H = 84;
      const COL_STEP = 236;
      const ROW_STEP = 132;
      const isTopBottom = String(direction || "LR").toUpperCase() === "TB";
      const coords = new Array(specNodes.length);
      const sortedRanks = [...layers.keys()].sort((a, b) => a - b);
      sortedRanks.forEach((r, rankIdx) => {
        const indices = layers.get(r);
        indices.forEach((nodeIdx, crossIdx) => {
          coords[nodeIdx] = {
            x: baseX + (isTopBottom ? crossIdx : rankIdx) * COL_STEP,
            y: baseY + (isTopBottom ? rankIdx : crossIdx) * ROW_STEP,
            w: NODE_W,
            h: NODE_H,
          };
        });
      });
      return coords;
    }

    function applyBoardOperationsBatch(operations, locations) {
      const bb = host.__EASYOFFICE_BASE_BOARD_HOST__;
      const boardStudio = bb?.getBoardStudio?.();
      const codec = host.__EASYOFFICE_BOARD_CODEC__;
      const router = host.__EASYOFFICE_BOARD_ROUTER__;
      if (!boardStudio || !codec) throw new Error("보드 모듈을 사용할 수 없습니다.");

      const cur = boardStudio.getState();
      const allNodes = cur.nodes.map((n) => ({ ...n }));
      const allConns = cur.connectors.map((c) => ({ ...c }));
      const takenIds = new Set(allNodes.map((n) => n.id));
      const takenConnIds = new Set(allConns.map((c) => c.id));
      let addedCount = 0;

      for (const op of operations) {
        const kind = String(op.op || op.type || op.kind || "").trim();
        if (kind !== "board.add_graph") throw new Error("지원하지 않는 보드 작업입니다: " + kind);
        const specNodes = Array.isArray(op.nodes) ? op.nodes : [];
        const specEdges = Array.isArray(op.edges) ? op.edges : [];
        const coords = computeLayeredGraphCoordinates(specNodes, specEdges, allNodes, op.direction);
        const idRemap = new Map();

        specNodes.forEach((sn, idx) => {
          const rawId = String(sn.id || `g-${idx + 1}`).trim();
          let finalId = rawId;
          let suffix = 1;
          while (takenIds.has(finalId)) {
            suffix += 1;
            finalId = `${rawId}-${suffix}`;
          }
          takenIds.add(finalId);
          idRemap.set(rawId, finalId);
          const box = coords[idx];
          const shapeMap = { roundedRect: "rect", roundRect: "rect" };
          const shapeType = shapeMap[sn.shape] || sn.shape || "rect";
          const normalized = codec.normalizeBoardNode({
            id: finalId,
            kind: sn.kind === "text" ? "text" : "shape",
            shapeType,
            x: box.x,
            y: box.y,
            w: box.w,
            h: box.h,
            text: String(sn.text || sn.label || finalId),
            ...(sn.fill ? { fill: sn.fill } : {}),
          }, finalId);
          allNodes.push(normalized);
          addedCount += 1;
        });

        const nodeMap = new Map(allNodes.map((n) => [n.id, n]));
        specEdges.forEach((se, eIdx) => {
          const fromId = idRemap.get(String(se.from)) || String(se.from);
          const toId = idRemap.get(String(se.to)) || String(se.to);
          if (!nodeMap.has(fromId) || !nodeMap.has(toId) || fromId === toId) return;
          let cid = `gc-${allConns.length + eIdx + 1}`;
          while (takenConnIds.has(cid)) cid += "x";
          takenConnIds.add(cid);
          const ports = router?.resolveAnchorPorts
            ? router.resolveAnchorPorts(nodeMap.get(fromId), nodeMap.get(toId), "auto", "auto")
            : { fromPort: "auto", toPort: "auto" };
          allConns.push({
            id: cid,
            fromId,
            toId,
            fromPort: ports.fromPort || "auto",
            toPort: ports.toPort || "auto",
            routing: se.style === "curved" || se.kind === "curved" ? "curved" : "orthogonal",
            label: String(se.label || ""),
          });
        });
      }

      const checked = codec.normalizeBoardState({
        nodes: allNodes,
        connectors: allConns,
        viewport: cur.viewport,
      }, true);
      boardStudio.replaceState(checked);
      boardStudio.fitAll?.();
      locations.push({ module: "BOARD", label: `다이어그램 요소 ${addedCount}개`, range: `요소 ${checked.nodes.length}개` });
      return true;
    }

    function boardEntries(op, entries, change) {
      const nodes = Array.isArray(op.nodes) ? op.nodes : [];
      const edges = Array.isArray(op.edges) ? op.edges : [];
      const summary = nodes.slice(0, 6).map((n) => n.text || n.label || n.id).join(" → ");
      entries.push(change("insert", `다이어그램 추가 (노드 ${nodes.length}개 · 연결선 ${edges.length}개)`, "", summary, { module: "BOARD", label: "보드 캔버스" }));
    }

    function boardFingerprint() {
      const st = host.__EASYOFFICE_BASE_BOARD_HOST__?.getBoardStudio?.()?.getState?.();
      return st ? JSON.stringify({ n: st.nodes?.length || 0, c: st.connectors?.length || 0, ids: (st.nodes || []).map((x) => [x.id, x.text]) }) : "";
    }

    async function locateTarget(loc, { docBlocksOf }) {
      if (!loc || typeof loc !== "object") return false;
      const mod = String(loc.module || getActiveModule()).toUpperCase();
      if (mod !== getActiveModule() && typeof activatePane === "function") {
        await activatePane(mod);
      }
      if (mod === "HWP" && getEmbedHwpHost()) return locateHwpTarget(loc);
      if (mod === "PDF") return locatePdfTarget(loc);
      if (mod === "SHEET") {
        const sc = getSheetController();
        const sheetName = String(loc.sheet || getActiveSheetName());
        if (sheetName && sheetName !== getActiveSheetName() && sheetDataStore?.[sheetName]) {
          await sc?.switchSheetTab?.(sheetName);
        }
        const rangeText = String(loc.range || loc.label || "").trim().toUpperCase();
        const names = host.__EASYOFFICE_SHEET_NAMES__;
        const nameBox = document.getElementById("active-cell-ref");
        if (rangeText && names?.parseAddress(rangeText) && nameBox) {
          nameBox.textContent = rangeText;
          const ev = typeof host.KeyboardEvent === "function"
            ? new host.KeyboardEvent("keydown", { key: "Enter", bubbles: true })
            : { type: "keydown", key: "Enter", target: nameBox };
          nameBox.dispatchEvent(ev);
          return true;
        }
        return false;
      }
      if (mod === "DOC" || mod === "HWP") {
        const editor = document.getElementById("doc-editor");
        const blocks = docBlocksOf(editor);
        const idx = Number.isInteger(loc.paragraphIndex) ? loc.paragraphIndex : 0;
        const target = blocks[Math.max(0, Math.min(blocks.length - 1, idx))];
        if (!target) return false;
        target.scrollIntoView?.({ block: "center" });
        editor.focus?.();
        const sel = host.getSelection?.();
        if (sel && document.createRange) {
          const range = document.createRange();
          range.setStart(target, 0);
          range.setEnd(target, 0);
          sel.removeAllRanges?.();
          sel.addRange?.(range);
        }
        return true;
      }
      if (mod === "SLIDE") {
        const idx = Number.isInteger(loc.slideIndex)
          ? loc.slideIndex
          : Math.max(0, (Number.parseInt(String(loc.label || "").replace(/\D+/g, ""), 10) || 1) - 1);
        if (idx < 0 || idx >= (slideDeckState?.length || 0)) return false;
        getWorkspacePanels()?.renderSlideDeck?.(idx);
        document.getElementById("slide-thumb-list")?.querySelector(`[data-slide-idx="${idx}"]`)?.focus?.();
        return true;
      }
      return mod === "BASE" || mod === "BOARD";
    }

    return {
      getEmbedHwpHost,
      refreshHwpSnapshot,
      extractHwpContextAsync,
      getHwpSelectionLabel,
      getHwpCommandTarget,
      hwpEntries,
      hwpFingerprint,
      verifyHwpFingerprintAsync,
      applyHwpOperations,
      canUndoHwpSync,
      canUndoHwpTurnAsync,
      undoHwpTurn,
      locateHwpTarget,
      readDocBlocksTool,
      extractPdfContext,
      searchPdfPagesTool,
      locatePdfTarget,
      locateTarget,
      applyBaseOperationsBatch,
      baseEntries,
      baseFingerprint,
      applyBoardOperationsBatch,
      boardEntries,
      boardFingerprint,
    };
  }

  host.__EASYOFFICE_CREATE_COPILOT_EXT_OPS__ = createCopilotExtOps;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
