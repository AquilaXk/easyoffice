// WebView 주도 경량 멀티턴 읽기·검증 루프 (PD-074 이벤트 권한 불허 및 ipc_commands = 15 준수, PD-359).
// 최대 6턴(MAX_TURNS = 6) 및 누적 요청 바이트 상한(MAX_CUMULATIVE_BYTES)을 엄격히 강제하고,
// 온디맨드 읽기 도구(read_doc_blocks, read_sheet_range, find_sheet_cells, search_pdf_pages)와
// 미리보기 제시 전 사후 검증 게이트(SLIDE·DOC layout_lint PD-258, SHEET 수식 오류 신규 발생 점검)를 수행한다.

(function (host) {
  "use strict";

  const MAX_TURNS = 6;
  const MAX_LINT_RETRIES = 2;
  const MAX_CUMULATIVE_BYTES = 384 * 1024;
  const MAX_TOOL_OUTPUT_CHARS = 12000;
  const MAX_HISTORY_SEND = 12;

  function utf8ByteLength(str) {
    if (typeof TextEncoder === "function") {
      return new TextEncoder().encode(String(str ?? "")).byteLength;
    }
    return String(str ?? "").length * 3;
  }

  async function verifyProposedOperations(mod, operations, ctx) {
    if (!Array.isArray(operations) || operations.length === 0) return { ok: true, issues: [] };
    const lintApi = host.__EASYOFFICE_LAYOUT_LINT__;
    const newIssues = [];

    if (mod === "SLIDE") {
      const baseline = new Set((lintApi?.lintWorkspaceSync?.()?.issues || []).map((it) => `${it.kind}:${it.targetId}:${it.message}`));
      const savedDeck = JSON.parse(JSON.stringify(ctx.slideDeckState));
      try {
        for (const op of operations) {
          const kind = String(op?.op || op?.type || op?.kind || "").trim();
          if (kind === "slide.add" || kind === "slide.update" || kind === "slide.add_element") {
            ctx.executeSlideOperation(kind, op, []);
          }
        }
        for (const it of lintApi?.lintWorkspaceSync?.()?.issues || []) {
          const k = `${it.kind}:${it.targetId}:${it.message}`;
          if (!baseline.has(k)) newIssues.push(it.message);
        }
      } catch (err) {
        newIssues.push(String(err?.message || err));
      } finally {
        ctx.slideDeckState.splice(0, ctx.slideDeckState.length, ...savedDeck);
      }
      return { ok: newIssues.length === 0, issues: newIssues };
    }

    if (mod === "DOC") {
      const hasToken = lintApi?.containsTemplateToken || ((s) => /\{\{[^}\r\n]{1,64}\}\}/.test(String(s || "")));
      for (const op of operations) {
        const kind = String(op?.op || op?.type || op?.kind || "").trim();
        if (kind !== "doc.append" && kind !== "doc.replace_paragraphs") continue;
        const items = Array.isArray(op.replacements)
          ? op.replacements.map((r) => r?.text)
          : Array.isArray(op.blocks)
            ? op.blocks
            : Array.isArray(op.paragraphs)
              ? op.paragraphs
              : op.text !== undefined ? [op.text] : [];
        for (let idx = 0; idx < items.length; idx += 1) {
          const el = ctx.buildDocBlockElement(ctx.document, items[idx]);
          if (!el) continue;
          const tag = String(el.tagName || "").toUpperCase();
          const txt = String(el.textContent || "");
          if (tag === "TABLE") {
            const cells = Array.from(el.querySelectorAll?.("td, th") || []);
            if (cells.length > 0 && cells.every((c) => !String(c.textContent || "").trim())) {
              newIssues.push(`표(제안 블록 ${idx + 1})의 모든 셀이 비어 있습니다.`);
            } else if (cells.some((c) => hasToken(c.textContent))) {
              newIssues.push(`표(제안 블록 ${idx + 1}) 셀에 치환되지 않은 템플릿 자리표시자가 있습니다.`);
            }
          } else if (/^H[1-6]$/.test(tag) && !txt.trim()) {
            newIssues.push(`제목 문단(제안 블록 ${idx + 1})이 비어 있습니다.`);
          } else if (txt.trim() && hasToken(txt)) {
            newIssues.push(`문단(제안 블록 ${idx + 1})에 치환되지 않은 템플릿 자리표시자가 있습니다.`);
          }
        }
      }
      return { ok: newIssues.length === 0, issues: newIssues };
    }

    const sheetOps = ctx.sheetOps;
    if (mod === "SHEET" && sheetOps?.collectSheetFormulaErrors && typeof ctx.invokeIpc === "function") {
      const needsEval = operations.some((op) => {
        const kind = String(op?.op || op?.type || op?.kind || "").trim();
        if (kind === "sheet.structure" && op?.action !== "freeze" && op?.action !== "unfreeze") return true;
        if (kind === "sheet.table" && op?.action === "total_row") return true;
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
          } else if (kind === "sheet.structure") {
            sheetOps.executeExtendedSheetOp(kind, op, {
              sheetName,
              sheetDataStore: candidateStore,
              sheetRowCounts: candidateRows,
              locations: [],
            });
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
        if (targets.length > 0) {
          const rpcRes = await ctx.invokeIpc("evaluate_sheet", JSON.stringify({
            sheet: activeName,
            sheets: Object.keys(candidateStore),
            targets,
            cells,
          }));
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
    const { clipCodePoints } = host.__EASYOFFICE_COPILOT_TEXT__;

    async function executeToolCalls(mod, toolCalls) {
      const outputs = [];
      for (const tc of toolCalls) {
        if (!tc || typeof tc !== "object") continue;
        const name = String(tc.name || "");
        try {
          const result = await copilotActions.executeReadTool(mod, name, tc);
          outputs.push({ tool: name, ok: true, result });
        } catch (err) {
          outputs.push({ tool: name, ok: false, error: String(err?.message || err) });
        }
      }
      const rawJson = JSON.stringify(outputs);
      return clipCodePoints(rawJson, MAX_TOOL_OUTPUT_CHARS);
    }

    async function runAgentTurn(params) {
      const {
        baseReq,
        newSubRequestId,
        isCancelled,
        onStreamText,
        onActiveRequestId,
      } = params;

      const streaming = Boolean(canStreamIpc?.());
      const workingHistory = Array.isArray(baseReq.history) ? baseReq.history.slice(-MAX_HISTORY_SEND) : [];
      let cumulativeBytes = 0;
      let totalDropped = 0;
      let lintRetries = 0;
      let finalRes = null;

      for (let turnIdx = 0; turnIdx < MAX_TURNS; turnIdx += 1) {
        if (isCancelled()) return null;
        const reqId = turnIdx === 0 ? baseReq.requestId : newSubRequestId(turnIdx);
        onActiveRequestId?.(reqId);

        const payloadObj = {
          ...baseReq,
          requestId: reqId,
          history: workingHistory.slice(-MAX_HISTORY_SEND),
        };
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
        finalRes = res;

        const toolCalls = Array.isArray(res?.toolCalls) ? res.toolCalls : [];
        const ops = Array.isArray(res?.operations) ? res.operations : [];

        // 1. On-demand read tool execution when the model asked to inspect more context first.
        if (toolCalls.length > 0 && ops.length === 0 && turnIdx + 1 < MAX_TURNS) {
          const toolJson = await executeToolCalls(baseReq.module, toolCalls);
          if (isCancelled()) return null;
          workingHistory.push({
            role: "assistant",
            text: String(res?.reply || "문서 범위를 조회합니다."),
            content: String(res?.reply || "문서 범위를 조회합니다."),
          });
          workingHistory.push({
            role: "user",
            text: `[도구 실행 결과]\n${toolJson}\n위 조회 결과를 바탕으로 원래 요청("${clipCodePoints(baseReq.prompt, 400)}")에 최종 답변·연산을 작성하십시오.`,
            content: `[도구 실행 결과]\n${toolJson}\n위 조회 결과를 바탕으로 원래 요청("${clipCodePoints(baseReq.prompt, 400)}")에 최종 답변·연산을 작성하십시오.`,
          });
          continue;
        }

        // 2. Deterministic post-verification gate before presenting edit preview (DOC / SLIDE / SHEET).
        if (
          baseReq.mode === "edit"
          && ops.length > 0
          && !baseReq.command
          && lintRetries < MAX_LINT_RETRIES
          && turnIdx + 1 < MAX_TURNS
          && (baseReq.module === "DOC" || baseReq.module === "SLIDE" || baseReq.module === "SHEET")
        ) {
          const normalizedOps = copilotActions.normalizeOperations(baseReq.module, ops);
          const gate = await copilotActions.dryRunVerifyOperations(baseReq.module, normalizedOps, invokeIpc);
          if (isCancelled()) return null;
          if (gate && !gate.ok && gate.issues.length > 0) {
            lintRetries += 1;
            const summary = gate.issues.slice(0, 6).join("; ");
            workingHistory.push({
              role: "assistant",
              text: JSON.stringify({ reply: res?.reply || "", operations: normalizedOps }),
              content: JSON.stringify({ reply: res?.reply || "", operations: normalizedOps }),
            });
            workingHistory.push({
              role: "user",
              text: `[자동 검증 게이트 오류] 제안한 연산을 적용하면 다음 문제가 새로 발생합니다: ${summary}. 오류가 발생하지 않도록 좌표·크기·수식 참조를 수정해 전체 연산을 다시 생성하십시오.`,
              content: `[자동 검증 게이트 오류] 제안한 연산을 적용하면 다음 문제가 새로 발생합니다: ${summary}. 오류가 발생하지 않도록 좌표·크기·수식 참조를 수정해 전체 연산을 다시 생성하십시오.`,
            });
            continue;
          }
        }

        break;
      }

      if (!finalRes) return null;
      return {
        ...finalRes,
        droppedOperations: totalDropped,
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
