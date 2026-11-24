(function (host) {
  "use strict";

  const {
    normalizeA1Token,
    jsonSheetNameOrDefaultJs,
    isFormulaStringJs,
    resolveCanonicalSheetUpper,
    resolveDefaultSheetFromDocJs,
    buildLocalWorkbookFromJson,
    readWorkbookTypedCell,
    formatFormulaValue,
    evaluateFormulaExpression,
  } = host.__EASYOFFICE_OFFLINE_FORMULA_EVAL__;
  const { localScrubPii: scrubSinglePii } = host.__EASYOFFICE_SHEET_UTILS__;
  const MAX_SCRUB_BATCH_ITEMS = 4096;
  const MAX_SCRUB_BATCH_BYTES = 4 * 1048576;

  function localScrubPii(rawInput) {
    const raw = String(rawInput ?? "");
    if (raw.trimStart().startsWith("{")) {
      let parsed = null;
      try {
        parsed = JSON.parse(raw);
      } catch (_err) {
        parsed = null;
      }
      if (parsed && typeof parsed === "object" && Object.prototype.hasOwnProperty.call(parsed, "texts")) {
        if (!Array.isArray(parsed.texts)) {
          throw new Error("JSON 'texts' 필드는 문자열 배열이어야 합니다.");
        }
        if (parsed.texts.length > MAX_SCRUB_BATCH_ITEMS) {
          throw new Error("개인정보 마스킹 배치는 최대 4096개까지 지원합니다.");
        }
        let totalBytes = 0;
        const encoder = typeof TextEncoder === "function" ? new TextEncoder() : null;
        for (const item of parsed.texts) {
          if (typeof item !== "string") {
            throw new Error("JSON 'texts' 항목은 문자열이어야 합니다.");
          }
          totalBytes += encoder ? encoder.encode(item).length : item.length * 3;
          if (totalBytes > MAX_SCRUB_BATCH_BYTES) {
            throw new Error("개인정보 마스킹 배치 크기가 허용 한도를 초과했습니다.");
          }
        }
        const meta = parsed.metadata && typeof parsed.metadata === "object" ? parsed.metadata : undefined;
        let totalMasked = 0;
        const results = parsed.texts.map((text) => {
          const itemRes = scrubSinglePii(text, meta);
          totalMasked += Number(itemRes.totalMasked || 0);
          return itemRes;
        });
        return { results, totalMasked, count: results.length };
      }
    }
    return scrubSinglePii(rawInput);
  }

  function tryParseAssistantEnvelope(rawText) {
    if (!rawText.trimStart().startsWith("{")) {
      return null;
    }
    try {
      const parsed = JSON.parse(rawText);
      return parsed && typeof parsed === "object" && typeof parsed.text === "string"
        ? parsed.text
        : null;
    } catch (_err) {
      // Malformed JSON is treated as absent input.
      return null;
    }
  }

  const LOCAL_KINDS = new Set(["ollama", "lmstudio", "local_http"]);
  const CLOUD_KINDS = new Set(["openai", "anthropic", "google", "deepseek", "openrouter", "mistral", "groq", "xai"]);

  function offlineCopilotTestProvider(doc) {
    const prov = doc.provider && typeof doc.provider === "object" ? doc.provider : {};
    const kind = String(prov.kind || "");
    const url = String(prov.baseUrl || "").trim();
    if (LOCAL_KINDS.has(kind)) {
      if (!/^http:\/\/(?:127\.0\.0\.1|\[::1\]):\d{1,5}(?:\/|$)/i.test(url)) {
        throw new Error("로컬 서버 주소는 http://127.0.0.1:<포트> 또는 http://[::1]:<포트>만 허용됩니다.");
      }
      return { status: "ok", message: `로컬 서버 연결 확인 완료 (${prov.model || kind})` };
    }
    if (CLOUD_KINDS.has(kind) || kind === "remote_https" || kind === "custom") {
      if (!url.startsWith("https://")) throw new Error("원격 HTTPS 엔드포인트가 필요합니다.");
      if (CLOUD_KINDS.has(kind) && !String(prov.apiKey || "").trim()) {
        throw new Error("API 키를 입력해 주세요.");
      }
      return { status: "ok", message: `${kind} 연결 확인 완료 (${prov.model || "기본"})` };
    }
    throw new Error("지원하지 않는 제공자 종류입니다.");
  }

  function offlineCopilotListModels(doc) {
    const prov = doc.provider && typeof doc.provider === "object" ? doc.provider : {};
    const kind = String(prov.kind || "");
    if (!kind) throw new Error("제공자를 먼저 선택해 주세요.");
    if (CLOUD_KINDS.has(kind) && !String(prov.apiKey || "").trim()) {
      throw new Error("API 키를 입력해 주세요.");
    }
    const mockLiveByKind = {
      ollama: [{ id: "qwen3:8b", name: "qwen3:8b", contextWindow: 131072, supportsReasoning: true }],
      lmstudio: [{ id: "local-instruct", name: "local-instruct", contextWindow: 131072, supportsReasoning: false }],
      openai: [{ id: "o3", name: "o3", contextWindow: 200000, supportsReasoning: true }],
      anthropic: [{ id: "claude-sonnet-4-6", name: "Claude Sonnet 4.6", contextWindow: 200000, supportsReasoning: true }],
      google: [{ id: "gemini-3.8-flash", name: "Gemini 3.8 Flash", contextWindow: 1048576, supportsReasoning: true }],
      deepseek: [{ id: "deepseek-reasoner", name: "DeepSeek R1", contextWindow: 128000, supportsReasoning: true }],
    };
    const models = (mockLiveByKind[kind] || []).map((m) => ({ ...m }));
    return { status: "ok", provider: kind, live: true, models };
  }

  function offlineCopilotSheet(mode, prompt, sheetCtx) {
    const sheetName = String(sheetCtx?.name || "Sheet1");
    const cells = Array.isArray(sheetCtx?.cells) ? sheetCtx.cells : [];
    const byCoord = new Map(cells.map((c) => [String(c.coord || "").toUpperCase(), c]));
    let maxRow = 1;
    for (const c of cells) {
      const m = /^([A-Z]+)(\d+)$/.exec(String(c.coord || "").toUpperCase());
      if (m) maxRow = Math.max(maxRow, Number(m[2]));
    }
    if (/목표\s*대비|격차|%/.test(prompt) && maxRow >= 2) {
      let worstRow = 2;
      let worstRatio = Infinity;
      for (let r = 2; r <= maxRow; r += 1) {
        const b = Number(byCoord.get("B" + r)?.value ?? byCoord.get("B" + r)?.raw);
        const c = Number(byCoord.get("C" + r)?.value ?? byCoord.get("C" + r)?.raw);
        if (Number.isFinite(b) && b !== 0 && Number.isFinite(c)) {
          const ratio = c / b;
          if (ratio < worstRatio) {
            worstRatio = ratio;
            worstRow = r;
          }
        }
      }
      const worstLabel = String(byCoord.get("A" + worstRow)?.value || byCoord.get("A" + worstRow)?.raw || worstRow + "행");
      const pctText = Number.isFinite(worstRatio) ? (worstRatio * 100).toFixed(1) + "%" : "확인됨";
      const reply = `${worstLabel} 항목이 목표 대비 ${pctText}로 가장 큰 격차를 보입니다.`;
      if (mode !== "edit") {
        return { status: "ok", mode: "ask", reply, reasoning: "", updatedInPlace: false, operations: [], locations: [] };
      }
      const writeCells = [{ coord: "D1", raw: "목표 대비 %", bold: true }];
      for (let r = 2; r <= maxRow; r += 1) {
        writeCells.push({ coord: "D" + r, raw: `=C${r}/B${r}`, numFmt: "0.0%", bold: r === worstRow });
      }
      const formulaRange = `D2:D${maxRow}`;
      const gapRange = `A${worstRow}:D${worstRow}`;
      return {
        status: "ok",
        mode: "edit",
        reply,
        reasoning: "",
        updatedInPlace: true,
        operations: [
          { op: "sheet.write", sheet: sheetName, cells: writeCells },
          { op: "sheet.format", sheet: sheetName, range: gapRange, fill: "#fff3cd", bold: true },
        ],
        locations: [
          { module: "SHEET", label: formulaRange, sheet: sheetName, range: formulaRange },
          { module: "SHEET", label: gapRange, sheet: sheetName, range: gapRange },
        ],
      };
    }
    if (/합계|평균/.test(prompt) && maxRow >= 2 && mode === "edit") {
      const sumRow = maxRow + 1;
      const range = `A${sumRow}:C${sumRow}`;
      return {
        status: "ok",
        mode: "edit",
        reply: `${sumRow}행에 합계 수식을 추가했습니다.`,
        reasoning: "",
        updatedInPlace: true,
        operations: [
          {
            op: "sheet.write",
            sheet: sheetName,
            cells: [
              { coord: "A" + sumRow, raw: "합계", bold: true },
              { coord: "B" + sumRow, raw: `=SUM(B2:B${maxRow})`, bold: true },
              { coord: "C" + sumRow, raw: `=SUM(C2:C${maxRow})`, bold: true },
            ],
          },
        ],
        locations: [{ module: "SHEET", label: range, sheet: sheetName, range }],
      };
    }
    return {
      status: "ok",
      mode,
      reply: `현재 시트(${sheetName})의 데이터 셀 ${cells.length}개를 확인했습니다.`,
      reasoning: "",
      updatedInPlace: false,
      operations: [],
      locations: [],
    };
  }

  function offlineCopilotRun(doc) {
    const mod = String(doc.module || "DOC").toUpperCase();
    const mode = mod === "PDF" || doc.mode !== "edit" ? "ask" : "edit";
    const prompt = String(doc.prompt || "").trim();
    const ctx = doc.context && typeof doc.context === "object" ? doc.context : {};

    if (mod === "SHEET") return offlineCopilotSheet(mode, prompt, ctx.sheet);
    if (mod === "DOC") {
      const paragraphs = Array.isArray(ctx.doc?.paragraphs) ? ctx.doc.paragraphs : [];
      if (mode === "edit") {
        const ops = [];
        if (/개인정보|마스킹/.test(prompt)) ops.push({ op: "doc.scrub_pii" });
        if (/피동|다듬/.test(prompt) && paragraphs.length > 0) {
          ops.push({
            op: "doc.replace_paragraphs",
            startIndex: 0,
            endIndex: paragraphs.length - 1,
            paragraphs: paragraphs.map((p) => String(p).replaceAll("되어지다", "되다").replaceAll("에 의해 ", "")),
          });
        } else {
          ops.push({ op: "doc.append", blocks: [{ tag: "p", text: "핵심 요약: 주요 안건과 실행 항목을 정리했습니다." }] });
        }
        return {
          status: "ok",
          mode: "edit",
          reply: "문서 내용을 제자리에서 업데이트했습니다.",
          reasoning: "",
          updatedInPlace: true,
          operations: ops,
          locations: [{ module: "DOC", label: "문단 1", paragraphIndex: 0 }],
        };
      }
      return {
        status: "ok",
        mode: "ask",
        reply: "문서 핵심 요약: " + (paragraphs[0] || "작성된 문단이 없습니다."),
        reasoning: "",
        updatedInPlace: false,
        operations: [],
        locations: [],
      };
    }
    if (mod === "SLIDE") {
      const slides = Array.isArray(ctx.slide?.slides) ? ctx.slide.slides : [];
      const activeIdx = Number(ctx.slide?.activeIndex || 0);
      if (mode === "edit") {
        if (/도형|글상자|차트|표|레이아웃/.test(prompt)) {
          const ops = [];
          if (/두\s*개의\s*내용/.test(prompt)) ops.push({ op: "slide.update", slideIndex: activeIdx, layout: "twoColumn" });
          else if (/제목\s*슬라이드/.test(prompt)) ops.push({ op: "slide.update", slideIndex: activeIdx, layout: "titleSlide" });
          else if (/빈\s*화면/.test(prompt)) ops.push({ op: "slide.update", slideIndex: activeIdx, layout: "blank" });
          if (/도형/.test(prompt)) {
            ops.push({ op: "slide.add_element", slideIndex: activeIdx, element: { kind: "shape", shapeType: "roundRect", x: 480, y: 150, w: 220, h: 96, text: "핵심 지표" } });
          } else if (/차트/.test(prompt)) {
            ops.push({ op: "slide.add_element", slideIndex: activeIdx, element: { kind: "chart", chartType: "column", x: 460, y: 240, w: 360, h: 190, title: "실적 요약" } });
          } else if (/글상자/.test(prompt)) {
            ops.push({ op: "slide.add_element", slideIndex: activeIdx, element: { kind: "textbox", x: 100, y: 140, w: 320, h: 80, text: "요약 설명" } });
          }
          if (ops.length > 0) {
            return {
              status: "ok",
              mode: "edit",
              reply: `${activeIdx + 1}번 슬라이드의 레이아웃과 개체를 업데이트했습니다.`,
              reasoning: "",
              updatedInPlace: true,
              operations: ops,
              locations: [{ module: "SLIDE", label: `슬라이드 ${activeIdx + 1}`, slideIndex: activeIdx }],
            };
          }
        }
        if (/추가|새\s*슬라이드/.test(prompt)) {
          const nextIdx = slides.length;
          return {
            status: "ok",
            mode: "edit",
            reply: `${nextIdx + 1}번 발표 슬라이드와 발표자 노트를 추가했습니다.`,
            reasoning: "",
            updatedInPlace: true,
            operations: [
              {
                op: "slide.add",
                title: "핵심 성과 요약",
                body: "• 목표 달성 현황 점검\n• 차기 분기 중점 과제",
                notes: "핵심 지표 변화와 실행 일정을 중심으로 발표합니다.",
              },
            ],
            locations: [{ module: "SLIDE", label: `슬라이드 ${nextIdx + 1}`, slideIndex: nextIdx }],
          };
        }
        return {
          status: "ok",
          mode: "edit",
          reply: `${activeIdx + 1}번 슬라이드의 발표자 노트를 작성했습니다.`,
          reasoning: "",
          updatedInPlace: true,
          operations: [{ op: "slide.update", slideIndex: activeIdx, notes: "슬라이드 핵심 요점을 간결하게 전달합니다." }],
          locations: [{ module: "SLIDE", label: `슬라이드 ${activeIdx + 1}`, slideIndex: activeIdx }],
        };
      }
      return {
        status: "ok",
        mode: "ask",
        reply: `총 ${slides.length || 1}장의 슬라이드 구성을 확인했습니다.`,
        reasoning: "",
        updatedInPlace: false,
        operations: [],
        locations: [],
      };
    }
    const pdfText = String(ctx.pdf?.text || "").trim();
    return {
      status: "ok",
      mode: "ask",
      reply: "PDF 요약: " + (pdfText ? pdfText.slice(0, 120) : `${ctx.pdf?.pageCount || 1}쪽 문서를 확인했습니다.`),
      reasoning: "",
      updatedInPlace: false,
      operations: [],
      locations: [],
    };
  }

  function localSmartAssistant(rawInput) {
    let rawText = String(rawInput ?? "");
    if (rawText.trimStart().startsWith("{")) {
      let parsedObj = null;
      try {
        parsedObj = JSON.parse(rawText);
      } catch (_err) {
        parsedObj = null;
      }
      if (parsedObj && typeof parsedObj === "object") {
        if (parsedObj.action === "copilot_test_provider") return offlineCopilotTestProvider(parsedObj);
        if (parsedObj.action === "copilot_list_models") return offlineCopilotListModels(parsedObj);
        if (parsedObj.action === "copilot_run") return offlineCopilotRun(parsedObj);
      }
    }
    const envelopeText = tryParseAssistantEnvelope(rawText);
    if (envelopeText !== null) {
      rawText = envelopeText;
    }
    const text = rawText.trim();
    const chars = Array.from(text).length;
    const words = text ? text.split(/\s+/).length : 0;
    const sentences = text ? text.split(/[.!?。]+/).filter((s) => s.trim().length > 0).length : 0;
    const readingTimeSeconds = Math.max(1, Math.ceil((words / 220) * 60));
    const lines = text ? text.split(/\n+/).map((l) => l.trim()).filter(Boolean) : [];
    const summaryLines = lines.slice().sort((a, b) => b.length - a.length).slice(0, 2);
    const actionItems = lines.filter((l) => /^(- \[ \]|\[TODO\]|TODO:|할일:|조치:)/.test(l));
    const styleSuggestions = lines.some((l) => Array.from(l).length > 120)
      ? ["120자를 초과하는 긴 문장이 있습니다. 두 문장으로 나누세요."]
      : [];
    return {
      summaryCount: summaryLines.length,
      charCount: chars,
      wordCount: words,
      sentenceCount: sentences,
      readingTimeSeconds,
      actionItemCount: actionItems.length,
      suggestionCount: styleSuggestions.length,
      summaryLines,
      styleSuggestions,
      actionItems,
    };
  }

  function isValidZipEntryPath(entryPath) {
    if (!entryPath || entryPath.includes("\0") || entryPath.startsWith("/") || entryPath.includes("\\")) {
      return false;
    }
    if (/^[A-Za-z]:/.test(entryPath)) return false;
    const trimmed = entryPath.endsWith("/") ? entryPath.slice(0, -1) : entryPath;
    if (!trimmed) return false;
    for (const seg of trimmed.split("/")) {
      if (!seg || seg === "." || seg === ".." || seg.includes(":")) return false;
    }
    return true;
  }

  function hasDangerousXmlDirective(xmlText) {
    return /<!\s*(?:DOCTYPE|ENTITY)\b/i.test(String(xmlText ?? ""));
  }

  function parseRequiredRpcJsonObject(payload) {
    const trimmed = String(payload ?? "")
      .replace(/^\uFEFF/, "")
      .trim();
    if (!trimmed.startsWith("{")) {
      throw new Error("InvalidArgument: RPC 페이로드는 JSON 객체여야 합니다.");
    }
    const doc = JSON.parse(trimmed);
    if (!doc || typeof doc !== "object" || Array.isArray(doc)) {
      throw new Error("InvalidArgument: RPC JSON 루트는 객체여야 합니다.");
    }
    return doc;
  }

  const ZIP_MAX_ENTRIES = 4096;
  const ZIP_MAX_SINGLE_UNCOMPRESSED = 128 * 1024 * 1024;
  const ZIP_MAX_TOTAL_UNCOMPRESSED = 512 * 1024 * 1024;
  const ZIP_MAX_RATIO = 100;
  const XML_DIRECTIVE_MESSAGE = "SecurityViolation: 허용되지 않은 XML DOCTYPE 또는 ENTITY 선언이 감지되었습니다.";
  const EMPTY_AUDIT_MESSAGE = "InvalidArgument: 보안 감사 대상 엔트리가 비어 있습니다.";

  const isNonNegativeSafeInteger = (value) => typeof value === "number" && Number.isSafeInteger(value) && value >= 0;

  function readZipEntryFields(item) {
    if (!item || typeof item !== "object" || Array.isArray(item)) {
      throw new Error("InvalidArgument: audit_security 'entries' 항목은 객체여야 합니다.");
    }
    const rawName = item.name !== undefined && item.name !== null ? item.name : item.path;
    if (typeof rawName !== "string" || !rawName.trim()) {
      throw new Error("InvalidArgument: ZIP 엔트리 이름('name')이 비어 있습니다.");
    }
    const rawComp = item.compressed !== undefined ? item.compressed : item.compressedSize;
    const rawUncomp = item.uncompressed !== undefined ? item.uncompressed : item.uncompressedSize;
    if (!isNonNegativeSafeInteger(rawComp) || !isNonNegativeSafeInteger(rawUncomp)) {
      throw new Error("InvalidArgument: ZIP 엔트리 크기 필드는 음수 아닌 정수여야 합니다.");
    }
    const enc = item.encrypted === undefined || item.encrypted === null ? false : item.encrypted;
    if (typeof enc !== "boolean") {
      throw new TypeError("InvalidArgument: 유효하지 않은 encrypted 불리언 값");
    }
    return { entryPath: rawName.trim(), rawComp, rawUncomp, enc };
  }

  function isUnsafeZipEntry(fields, seenPaths, totalUncompressed) {
    const { entryPath, rawComp, rawUncomp, enc } = fields;
    return (
      !isValidZipEntryPath(entryPath) ||
      seenPaths.has(entryPath) ||
      (entryPath.endsWith("/") && (rawComp > 0 || rawUncomp > 0)) ||
      enc ||
      rawUncomp > ZIP_MAX_SINGLE_UNCOMPRESSED ||
      totalUncompressed > ZIP_MAX_TOTAL_UNCOMPRESSED ||
      (rawComp === 0 && rawUncomp > 0) ||
      (rawComp > 0 && rawUncomp / rawComp > ZIP_MAX_RATIO)
    );
  }

  function validateZipEntriesList(entries) {
    if (entries.length === 0) throw new Error(EMPTY_AUDIT_MESSAGE);
    if (entries.length > ZIP_MAX_ENTRIES) {
      throw new Error("SecurityViolation: ZIP 엔트리 개수가 보안 한도를 초과했습니다.");
    }
    let totalUncompressed = 0;
    const seenPaths = new Set();
    for (const item of entries) {
      const fields = readZipEntryFields(item);
      totalUncompressed += fields.rawUncomp;
      if (isUnsafeZipEntry(fields, seenPaths, totalUncompressed)) {
        throw new Error("SecurityViolation: 안전하지 않은 ZIP 컨테이너 엔트리가 감지되었습니다: " + fields.entryPath);
      }
      seenPaths.add(fields.entryPath);
    }
  }

  function parseLegacyAuditToken(tok) {
    const parts = tok.split(":");
    if (parts.length !== 4) {
      throw new Error("InvalidArgument: 잘못된 보안 감사 엔트리 구문");
    }
    const comp = Number(parts[1]);
    const uncomp = Number(parts[2]);
    if (!Number.isSafeInteger(comp) || !Number.isSafeInteger(uncomp)) {
      throw new TypeError("InvalidArgument: ZIP 엔트리 크기 필드는 음수 아닌 정수여야 합니다.");
    }
    if (parts[3] !== "true" && parts[3] !== "false") {
      throw new Error("InvalidArgument: 유효하지 않은 encrypted 불리언 값");
    }
    return { name: parts[0], compressed: comp, uncompressed: uncomp, encrypted: parts[3] === "true" };
  }

  function auditSafeResult(checkedEntries) {
    return { status: "SAFE", verdict: "SAFE", xxeGuard: "PASSED", zipGuard: "PASSED", checkedEntries };
  }

  // Compact form: "name:compressed:uncompressed:encrypted,...|<xml>".
  function auditLegacyPayload(trimmed) {
    const pipeIdx = trimmed.indexOf("|");
    const entryPart = pipeIdx >= 0 ? trimmed.slice(0, pipeIdx) : trimmed;
    const xmlPart = pipeIdx >= 0 ? trimmed.slice(pipeIdx + 1) : "";
    if (hasDangerousXmlDirective(xmlPart)) throw new Error(XML_DIRECTIVE_MESSAGE);
    const rawItems = entryPart
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    if (rawItems.length === 0) throw new Error(EMPTY_AUDIT_MESSAGE);
    const parsedEntries = rawItems.map(parseLegacyAuditToken);
    validateZipEntriesList(parsedEntries);
    return auditSafeResult(parsedEntries.length);
  }

  function auditContainerBase64(containerBase64) {
    if (typeof containerBase64 !== "string" || !containerBase64.trim()) throw new Error(EMPTY_AUDIT_MESSAGE);
    let decoded = "";
    try {
      decoded = typeof host.atob === "function" ? host.atob(containerBase64.trim()) : "";
    } catch (_err) {
      throw new Error("InvalidArgument: 유효하지 않은 Base64 컨테이너 데이터입니다.");
    }
    if (!decoded) throw new Error(EMPTY_AUDIT_MESSAGE);
    if (hasDangerousXmlDirective(decoded)) throw new Error(XML_DIRECTIVE_MESSAGE);
    return auditSafeResult(1);
  }

  function localAuditSecurity(payload) {
    const trimmed = String(payload ?? "")
      .replace(/^\uFEFF/, "")
      .trim();
    if (!trimmed) throw new Error(EMPTY_AUDIT_MESSAGE);
    if (!trimmed.startsWith("{")) return auditLegacyPayload(trimmed);

    const doc = parseRequiredRpcJsonObject(trimmed);
    if (doc.xml !== undefined && doc.xml !== null && typeof doc.xml !== "string") {
      throw new Error("InvalidArgument: audit_security 'xml' 필드는 문자열이어야 합니다.");
    }
    if (hasDangerousXmlDirective(doc.xml ?? "")) throw new Error(XML_DIRECTIVE_MESSAGE);
    if (doc.containerBase64 !== undefined && doc.containerBase64 !== null) {
      return auditContainerBase64(doc.containerBase64);
    }
    if (!Array.isArray(doc.entries)) {
      throw new TypeError("InvalidArgument: audit_security 'entries' 필드는 배열이어야 합니다.");
    }
    validateZipEntriesList(doc.entries);
    return auditSafeResult(doc.entries.length);
  }

  // The engine's judgment of a save that drops a password (the rule of `convert_password.rs`): XLSX holds an agile or
// legacy digest, ODS an ODS key, `both` fits either, every other format holds none; without `acceptPasswordLoss` it is refused.
const OFFLINE_HOLDS = { xlsx: ["agile", "both", "legacy"], ods: ["odf", "both"] };
function offlinePasswordLoss(request) {
  if (request.acceptPasswordLoss === true) return false;
  const holds = OFFLINE_HOLDS[String(request.targetFormat || "").toLowerCase()] || [];
  const found = [request.workbookProtect?.pwd];
  for (const sheet of request.sheets || []) {
    found.push(sheet.protect?.pwd);
    for (const range of sheet.protect?.ranges || []) found.push(range.pwd);
  }
  return found.some((pwd) => pwd && !holds.includes(pwd.kind));
}

  function hasOfflineTemplateToken(text) {
    const str = String(text ?? "");
    const m = /\{\{([^\r\n{}]{1,64})\}\}/.exec(str);
    return Boolean(m && m[1].trim().length > 0);
  }

  function buildOfflineLayoutLint(model) {
    const issues = [];
    const counts = { overflow: 0, overlap: 0, emptyPlaceholder: 0 };
    const pushIssue = (kind, pageOrSlide, blockIndex, targetId, message) => {
      counts[kind] += 1;
      issues.push({ kind, pageOrSlide, blockIndex, targetId, message });
    };

    if (Array.isArray(model.slides)) {
      const aspectDims = { "16:9": [960, 540], "4:3": [720, 540], A4: [841.89, 595.28] };
      const [stageW, stageH] = aspectDims[model.aspect] || [960, 540];
      const stageArea = stageW * stageH;
      model.slides.forEach((s, sIdx) => {
        const slideNo = sIdx + 1;
        const els = Array.isArray(s?.elements) ? s.elements : [];
        els.forEach((el) => {
          if (!el || !el.id) return;
          const x = Number(el.x), y = Number(el.y), w = Number(el.w), h = Number(el.h);
          if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(w) || !Number.isFinite(h)
              || x < -0.5 || y < -0.5 || x + w > stageW + 0.5 || y + h > stageH + 0.5) {
            pushIssue("overflow", slideNo, null, el.id, `슬라이드 ${slideNo} 개체 '${el.id}'가 스테이지 경계 밖으로 벗어났습니다.`);
          }
          if (el.kind === "textbox") {
            const txt = String(el.text ?? "");
            if (!txt.trim() || hasOfflineTemplateToken(txt)) {
              pushIssue("emptyPlaceholder", slideNo, null, el.id, `슬라이드 ${slideNo} 글상자 '${el.id}' 자리표시자가 비어 있습니다.`);
            }
          }
        });
        const boxes = els
          .map((el, idx) => ({ el, idx, x0: Number(el?.x), y0: Number(el?.y), x1: Number(el?.x) + Number(el?.w), y1: Number(el?.y) + Number(el?.h) }))
          .filter((b) => b.el?.id && Number.isFinite(b.x0) && Number.isFinite(b.y0) && Number.isFinite(b.x1) && Number.isFinite(b.y1)
            && !(b.el.kind === "shape" && (b.x1 - b.x0) * (b.y1 - b.y0) >= stageArea * 0.9))
          .sort((a, b) => (a.x0 - b.x0) || (a.idx - b.idx));
        for (let i = 0; i < boxes.length; i += 1) {
          for (let j = i + 1; j < boxes.length; j += 1) {
            if (boxes[j].x0 >= boxes[i].x1 - 1) break;
            const ix = Math.min(boxes[i].x1, boxes[j].x1) - Math.max(boxes[i].x0, boxes[j].x0);
            const iy = Math.min(boxes[i].y1, boxes[j].y1) - Math.max(boxes[i].y0, boxes[j].y0);
            if (ix > 1 && iy > 1 && ix * iy > 4) {
              const first = els[Math.min(boxes[i].idx, boxes[j].idx)];
              pushIssue("overlap", slideNo, null, first.id, `슬라이드 ${slideNo} 개체 '${first.id}'가 다른 개체와 겹쳤습니다.`);
            }
          }
        }
        const layout = String(s?.layout || "titleAndContent");
        const title = String(s?.title ?? "");
        const bullets = Array.isArray(s?.bullets) ? s.bullets : [];
        if (layout !== "blank" && !title.trim()) {
          pushIssue("emptyPlaceholder", slideNo, null, `slide-${slideNo}-title`, `슬라이드 ${slideNo}의 제목이 비어 있습니다.`);
        }
        if ((layout === "titleAndContent" || layout === "twoColumn") && !bullets.some((b) => String(b || "").trim()) && els.length === 0) {
          pushIssue("emptyPlaceholder", slideNo, null, `slide-${slideNo}-body`, `슬라이드 ${slideNo}의 본문이 비어 있습니다.`);
        }
      });
    } else if (Array.isArray(model.blocks)) {
      const contentWPt = 481.89;
      model.blocks.forEach((b, idx) => {
        if (!b || typeof b !== "object") return;
        if (b.type === "image") {
          const wPt = Number(b.widthEmu || 0) / 12700;
          if (wPt > contentWPt + 1) {
            pushIssue("overflow", 1, idx, `block-${idx}`, `그림(블록 ${idx})이 본문 가용 폭을 초과합니다.`);
          }
          return;
        }
        if (b.type === "table") {
          const widths = Array.isArray(b.colWidthsTwips) ? b.colWidthsTwips : [];
          const sumPt = widths.reduce((acc, w) => acc + (Number(w) || 0) / 20, 0);
          if (sumPt > contentWPt + 1) {
            pushIssue("overflow", 1, idx, `table-${idx}`, `표(블록 ${idx}) 열 너비가 본문 폭을 초과합니다.`);
          }
          const rows = Array.isArray(b.rows) ? b.rows : [];
          if (rows.length > 0) {
            let allEmpty = true;
            let hasTok = false;
            for (const r of rows) {
              for (const c of Array.isArray(r) ? r : []) {
                const txt = String(typeof c === "string" ? c : c?.value ?? c?.text ?? "");
                if (txt.trim()) {
                  allEmpty = false;
                  if (!hasTok && hasOfflineTemplateToken(txt)) hasTok = true;
                }
              }
            }
            if (allEmpty) pushIssue("emptyPlaceholder", 1, idx, `table-${idx}`, `표(블록 ${idx})의 모든 셀이 비어 있습니다.`);
            else if (hasTok) pushIssue("emptyPlaceholder", 1, idx, `table-${idx}`, `표(블록 ${idx})에 미치환 자리표시자가 있습니다.`);
          }
          return;
        }
        const text = (Array.isArray(b.runs) ? b.runs : []).map((r) => String(r?.text || "")).join("");
        if (Number(b.heading) > 0 && !text.trim()) {
          pushIssue("emptyPlaceholder", 1, idx, `block-${idx}`, `제목 문단(블록 ${idx})이 비어 있습니다.`);
        } else if (text.trim() && hasOfflineTemplateToken(text)) {
          pushIssue("emptyPlaceholder", 1, idx, `block-${idx}`, `문단(블록 ${idx})에 미치환 자리표시자가 있습니다.`);
        }
      });
    }

    return {
      issueCount: issues.length,
      overflowCount: counts.overflow,
      overlapCount: counts.overlap,
      emptyPlaceholderCount: counts.emptyPlaceholder,
      truncated: false,
      issues,
    };
  }

function offlineSourceConversion(parsed) {
    const decoded = offlineDecodeSource(parsed);
    if (parsed.action === "decode") return decoded;
    if (parsed.action === "inspect" || parsed.action === "readback") {
      return { ...decoded, action: parsed.action, layoutLint: buildOfflineLayoutLint(decoded) };
    }
    const b64 = encodeUtf8Base64(JSON.stringify(decoded.blocks || decoded.sheets || decoded.slides || []));
    return {
      sourceFormat: decoded.format, targetFormat: String(parsed.targetFormat || "hwpx"),
      sourceBytes: decoded.sourceBytes, outputBytes: 10, targetBytes: 10,
      detectedTarget: String(parsed.targetFormat || "hwpx"), tableCount: 0,
      specification: "오프라인 테스트 변환", compatibility: "standard", compatibilityWarning: null,
      warnings: decoded.warnings, convertedBase64: b64, status: "OK",
    };
  }

  const blockToText = (b) =>
    b?.type === "table"
      ? (b.rows || []).map((r) => r.map((c) => (typeof c === "string" ? c : c.value || c.text || "")).join("\t")).join("\n")
      : (b?.runs || []).map((r) => String(r.text || "")).join("");

  // Structured save payload (PD-073): literal text only, never parsed as markup.
  function structuredSaveBody(parsed) {
    if (Array.isArray(parsed.blocks)) {
      return {
        structuredTables: parsed.blocks.filter((b) => b?.type === "table").length,
        body: parsed.blocks.map(blockToText).join("\n"),
      };
    }
    if (Array.isArray(parsed.sheets)) {
      return { structuredTables: 0, body: parsed.sheets.map((s) => (s.cells || []).map((c) => c.ref + "=" + c.raw).join("\n")).join("\n") };
    }
    if (Array.isArray(parsed.slides)) {
      const structuredTables = parsed.slides.reduce((acc, s) => acc + (Array.isArray(s.elements) ? s.elements.filter((e) => e?.kind === "table").length : 0), 0);
      const body = parsed.slides.map((s) => {
        const elTxt = Array.isArray(s.elements) ? s.elements.map((e) => e?.text || e?.title || e?.alt || "").filter(Boolean) : [];
        return [s.title, ...(s.bullets || []), ...elTxt, s.notes].join("\n");
      }).join("\n");
      return { structuredTables, body };
    }
    if (parsed.blocks !== undefined || parsed.sheets !== undefined || parsed.slides !== undefined) {
      throw new Error("InvalidArgument: 구조화 저장 필드는 배열이어야 합니다.");
    }
    return null;
  }

  function parseConvertJsonFields(parsed) {
    const fields = {
      src: String(parsed.sourceFormat || "docx").trim().toLowerCase(),
      dst: String(parsed.targetFormat || "hwpx").trim().toLowerCase(),
      title: String(parsed.title || "통합_변환_문서"),
      body: String(parsed.body || "본문 텍스트"),
      structuredTables: null,
    };
    const structured = structuredSaveBody(parsed);
    const merged = structured ? { ...fields, ...structured } : fields;
    return merged.dst === "pdf" ? { ...merged, body: merged.body + pdfPageSettingLines(parsed.pdf) } : merged;
  }

  // The real PDF writer prints the watermark and the stamp as `[WM]` / `[STAMP]` text on the page.
  function pdfPageSettingLines(pdf) {
    const lines = [];
    if (pdf?.watermark) lines.push("[WM] " + pdf.watermark);
    if (pdf?.stamp) lines.push("[STAMP] " + pdf.stamp);
    return lines.map((line) => "\n" + line).join("");
  }

  function parseConvertPipeFields(raw) {
    const firstPipe = raw.indexOf("|");
    const firstSeg = firstPipe >= 0 ? raw.slice(0, firstPipe) : raw;
    const restSeg = firstPipe >= 0 ? raw.slice(firstPipe + 1) : "통합_변환_문서|본문";
    const secondPipe = restSeg.indexOf("|");
    const pair = (firstSeg || "docx->hwpx").split("->");
    return {
      src: (pair[0] || "docx").trim().toLowerCase(),
      dst: (pair[1] || "hwpx").trim().toLowerCase(),
      title: (secondPipe >= 0 ? restSeg.slice(0, secondPipe) : restSeg) || "통합_변환_문서",
      body: (secondPipe >= 0 ? restSeg.slice(secondPipe + 1) : "") || "본문 텍스트",
      structuredTables: null,
    };
  }

  function encodeUtf8Base64(text) {
    const bytes = new TextEncoder().encode(String(text || ""));
    let binary = "";
    for (const byte of bytes) {
      binary += String.fromCodePoint(byte);
    }
    return typeof host.btoa === "function" ? host.btoa(binary) : "";
  }

  // Source-file requests (`sourceBase64`, PD-075). The offline backend cannot parse real
  // containers, so binary files must carry an `EOFIXTURE:` + JSON model (what the Rust decoder
  // returns); text formats (txt/md/csv/tsv) are decoded for real; anything else is "corrupt".
  const SOURCE_MODULES = {
    docx: "DOC", doc: "DOC", odt: "DOC", rtf: "DOC", md: "DOC", markdown: "DOC", txt: "DOC", pages: "DOC",
    hwp: "HWP", hwpx: "HWP", hwt: "HWP", hwtx: "HWP",
    xlsx: "SHEET", xls: "SHEET", ods: "SHEET", csv: "SHEET", tsv: "SHEET",
    pptx: "SLIDE", ppt: "SLIDE", ppsx: "SLIDE", odp: "SLIDE", pdf: "PDF",
    eobase: "BASE", eoboard: "BOARD",
  };
  const FIXTURE_PREFIX = "EOFIXTURE:";

  function decodeBase64Bytes(b64) {
    const bin = typeof host.atob === "function" ? host.atob(String(b64 || "")) : "";
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.codePointAt(i);
    return out;
  }

  function offlineDecodeSource(parsed) {
    const fileName = String(parsed.fileName || "");
    const ext = (/\.([A-Za-z0-9]+)$/.exec(fileName)?.[1] || "").toLowerCase();
    if (!SOURCE_MODULES[ext]) throw new Error("지원하지 않는 확장자입니다: " + (ext || fileName));
    const bytes = decodeBase64Bytes(parsed.sourceBase64);
    if (bytes.length === 0) throw new Error("파일 데이터가 비어 있어 열 수 없습니다.");
    const text = new TextDecoder("utf-8").decode(bytes);
    const base = { status: "OK", action: "decode", fileName, format: ext, module: SOURCE_MODULES[ext],
      title: "", sourceBytes: bytes.length, specification: "오프라인 테스트 디코더", warnings: [] };
    // The edit package the offline `encode_package` made: its model and pictures as JSON (the real container is the engine's).
    if (ext === "eobase" || ext === "eoboard") {
      const pkg = JSON.parse(text);
      return { ...base, kind: ext === "eobase" ? "base" : "board", model: pkg.model, media: (pkg.media || []).map((m) => ({ ...m, mediaType: m.path.endsWith(".jpg") ? "image/jpeg" : "image/png" })) };
    }
    if (text.startsWith(FIXTURE_PREFIX)) {
      const model = JSON.parse(text.slice(FIXTURE_PREFIX.length));
      let kind = "document";
      if (model.sheets) kind = "workbook";
      else if (model.slides) kind = "slides";
      const count = (model.sheets || model.slides || model.blocks || []).length;
      return { ...base, kind, itemCount: count, ...model };
    }
    if (["docx", "xlsx", "pptx", "hwpx", "hwp", "doc", "xls", "ppt", "odt", "ods", "odp", "pdf", "hwt", "hwtx", "ppsx", "pages"].includes(ext)) {
      throw new Error("컨테이너 무결성 검증 실패: 올바른 " + ext.toUpperCase() + " 파일이 아닙니다.");
    }
    if (ext === "csv" || ext === "tsv") {
      const delim = ext === "csv" ? "," : "\t";
      const cells = [];
      text.replace(/^﻿/, "").split(/\r?\n/).filter(Boolean).forEach((line, r) => {
        line.split(delim).forEach((v, c) => {
          if (v) cells.push({ ref: String.fromCodePoint(65 + c) + (r + 1), raw: v });
        });
      });
      return { ...base, kind: "workbook", itemCount: 1, sheets: [{ name: "Sheet1", cells }] };
    }
    const lines = text.replace(/^﻿/, "").split(/\r?\n/).filter((l) => l.trim());
    return { ...base, kind: "document", itemCount: lines.length,
      blocks: lines.map((l) => ({ type: "paragraph", runs: [{ text: l.replace(/^#+\s*/, "") }] })) };
  }


  function parseRibbonPayloadFields(payload) {
    const raw = String(payload || "").trim();
    return raw.startsWith("{") ? parseRequiredRpcJsonObject(raw) : null;
  }

  function createOfflineRpcHandlers(ctx) {
    const {
      getActiveModule,
      ribbonCategoryTools,
      ribbonStyleLabels,
      contextualToolBadges,
      moduleSpecs,
      localRibbonState,
    } = ctx;

    const backendBaseState = {
        activeModule: "DOC",
        ribbonStyle: "standard",
        isBold: false,
        isItalic: false,
        isUnderline: false,
        stylePresetIndex: 0,
        insertedTableCount: 0,
        insertedChartCount: 0,
        insertedShapeCount: 0,
        insertedLinkCount: 0,
        paperSize: "A4",
        columnCount: 1,
        isLandscape: false,
        marginPresetIndex: 0,
        autosumAppliedCount: 0,
        formulaWizardOpen: false,
        sortAscending: true,
        validationGuardEnabled: true,
        tocGenerated: false,
        footnoteCount: 0,
        mailMergeAppliedCount: 0,
        tableFormulaEvaluatedCount: 0,
        manuscriptGridEnabled: false,
        approvalHeaderEnabled: true,
        clickHereFieldCount: 0,
        doubleDiagonalBorderEnabled: false,
        pivotSummaryEnabled: false,
        conditionalFormatEnabled: false,
        freezePanesEnabled: false,
        masterLayoutIndex: 0,
        snapGuideEnabled: true,
        slideTransitionIndex: 0,
        presenterModeActive: false,
        pdfHighlightCount: 0,
        pdfRotationDegrees: 0,
        pdfSplitMergeCount: 0,
        pdfConvertedToOfficeCount: 0,
        trackChangesEnabled: false,
        reviewCommentCount: 0,
        securityVerifiedCount: 0,
        statsInspectedCount: 0,
        splitViewEnabled: false,
        ribbonCollapsed: false,
        darkThemeEnabled: false,
        zoomPercent: 100,
        executedCount: 0,
        ...structuredClone(localRibbonState || {}),
    };
    const MAX_BACKEND_HISTORY = 64;
    // Mirrors crates/easyoffice-app/src/ribbon_session.rs: ribbon state and undo/redo stacks are
    // kept per client session (`sessionId`, missing → "default") and per module within a session.
    const DEFAULT_RIBBON_SESSION_ID = "default";
    const MAX_RIBBON_SESSIONS = 32;
    const RIBBON_MODULE_KEYS = new Set(["DOC", "HWP", "SHEET", "SLIDE", "PDF"]);
    const ribbonSessions = new Map();

    function normalizeRibbonModuleKey(raw) {
      const key = String(raw || "").trim().toUpperCase();
      if (key === "CONV" || key === "DOCUMENT") return "DOC";
      if (RIBBON_MODULE_KEYS.has(key)) return key;
      throw new Error("InvalidArgument: 지원하지 않는 리본 모듈입니다: " + key);
    }

    function normalizeRibbonSessionId(raw) {
      if (raw !== undefined && raw !== null && typeof raw !== "string") {
        throw new Error("InvalidArgument: JSON 'sessionId' 필드는 문자열이어야 합니다.");
      }
      const sid = String(raw || "").trim();
      if (!sid) return DEFAULT_RIBBON_SESSION_ID;
      if (sid.length > 64 || !/^[A-Za-z0-9_-]+$/.test(sid)) {
        throw new Error("InvalidArgument: 리본 세션 식별자 형식이 올바르지 않습니다.");
      }
      return sid;
    }

    function ribbonSession(sessionId) {
      let session = ribbonSessions.get(sessionId);
      if (session) {
        ribbonSessions.delete(sessionId);
      } else {
        if (ribbonSessions.size >= MAX_RIBBON_SESSIONS) {
          ribbonSessions.delete(ribbonSessions.keys().next().value);
        }
        session = {
          ribbonStyle: backendBaseState.ribbonStyle || "standard",
          lastModule: "DOC",
          slots: {},
        };
      }
      ribbonSessions.set(sessionId, session);
      return session;
    }

    function ribbonSlot(session, mod) {
      if (!session.slots[mod]) {
        session.slots[mod] = {
          state: { ...structuredClone(backendBaseState), activeModule: mod },
          undo: [],
          redo: [],
        };
      }
      session.slots[mod].state.ribbonStyle = session.ribbonStyle;
      return session.slots[mod];
    }

    function ribbonStateResponse(slot, extra) {
      return {
        ...extra,
        catalogCount: 140,
        ...structuredClone(slot.state),
        canUndo: slot.undo.length > 0,
        canRedo: slot.redo.length > 0,
      };
    }

    function localRibbonHistoryStep(payload, isUndo) {
      const raw = String(payload || "").trim();
      const parsed = parseRibbonPayloadFields(raw);
      const session = ribbonSession(normalizeRibbonSessionId(parsed ? parsed.sessionId : ""));
      const moduleRaw = parsed ? parsed.module : raw;
      const mod = moduleRaw ? normalizeRibbonModuleKey(moduleRaw) : session.lastModule;
      const slot = ribbonSlot(session, mod);
      const from = isUndo ? slot.undo : slot.redo;
      const to = isUndo ? slot.redo : slot.undo;
      if (from.length > 0) {
        if (to.length >= MAX_BACKEND_HISTORY) to.shift();
        to.push(structuredClone(slot.state));
        slot.state = from.pop();
        slot.state.ribbonStyle = session.ribbonStyle;
      }
      return ribbonStateResponse(slot, {
        commandId: isUndo ? "undo" : "redo",
        summary: isUndo ? "실행 취소 완료" : "다시 실행 완료",
      });
    }

    function inferCommandModuleKey(cmdId) {
      if (cmdId.startsWith("hwp-")) return "HWP";
      if (cmdId.startsWith("sheet-")) return "SHEET";
      if (cmdId.startsWith("slide-")) return "SLIDE";
      if (cmdId.startsWith("pdf-")) return "PDF";
      if (cmdId.startsWith("document-") || cmdId.startsWith("doc-")) return "DOC";
      return getActiveModule();
    }

    function resolveOptionalAnchorCell(doc) {
      if (doc.cell !== undefined && doc.cell !== null && typeof doc.cell !== "string") {
        throw new Error("InvalidArgument: JSON 'cell' 필드는 문자열이어야 합니다.");
      }
      if (typeof doc.cell === "string" && doc.cell.trim().length > 0) {
        const normAnchor = normalizeA1Token(doc.cell.trim());
        if (!normAnchor) {
          throw new Error("InvalidArgument: 유효하지 않은 셀 좌표('cell')입니다.");
        }
        return normAnchor;
      }
      return null;
    }

    function evaluateSingleDocExpr(doc) {
      const sheetName = resolveDefaultSheetFromDocJs(doc);
      const explicitAnchor = resolveOptionalAnchorCell(doc);
      const anchorCell = explicitAnchor || "A1";
      const rawExpr = doc.expr !== undefined && doc.expr !== null ? doc.expr : doc.query;
      if (rawExpr !== undefined && rawExpr !== null && typeof rawExpr !== "string") {
        throw new Error("InvalidArgument: JSON 'expr' 필드는 문자열이어야 합니다.");
      }
      const fallbackCell = explicitAnchor || "A1";
      const defaultQuery = fallbackCell;
      const chosenQuery = rawExpr === undefined || rawExpr === null ? defaultQuery : rawExpr;
      const q = String(chosenQuery).trim() || defaultQuery;
      const workbook = buildLocalWorkbookFromJson(doc, sheetName);
      const evalSheetUpper = resolveCanonicalSheetUpper(workbook.knownSheets, sheetName);
      if (!evalSheetUpper) {
        return { sheetName, expr: q, value: "#REF!", circular: false };
      }
      if (isFormulaStringJs(q)) {
        return {
          sheetName,
          expr: q,
          value: String(evaluateFormulaExpression(q, workbook, evalSheetUpper, anchorCell)),
          circular: workbook.circular,
        };
      }
      const cellValue = formatFormulaValue(readWorkbookTypedCell(workbook, evalSheetUpper, q, 0));
      return { sheetName, expr: q, value: cellValue, circular: workbook.circular };
    }

    function evaluateLocalWorkbookFormulaRpc(payload) {
      const doc = parseRequiredRpcJsonObject(payload);
      const res = evaluateSingleDocExpr(doc);
      return { sheet: res.sheetName, formula: res.expr, value: res.value, circular: res.circular };
    }

    // Stand-in for the engine's `format` action (sheet_format_ipc.rs): the few codes the tests use,
    // `0`, `0.00`, `0.0%` and the like, a won sign with grouping, and a red negative section.
    function offlineFormatCells(doc) {
      const group = (n) => String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
      const digits = /^0(?:\.(0+))?(%)?$/;
      const render = (code, n) => {
        const m = digits.exec(code);
        if (m) return [(m[2] ? n * 100 : n).toFixed(m[1] ? m[1].length : 0) + (m[2] ? "%" : ""), ""];
        if (code === '"₩"#,##0') return ["₩" + group(Math.round(n)), ""];
        if (code === "#,##0;[Red]-#,##0") return [group(Math.round(n)), n < 0 ? "#ff0000" : ""];
        if (code === '_("$"* #,##0.00_)') return [" $" + n.toFixed(2) + " ", "", [2, " "]];
        return null;
      };
      return {
        items: doc.items.map(({ code, values }) => {
          if (code.length > 255) return { bad: true };
          const t = [];
          const c = {};
          const f = {};
          values.forEach((text, i) => {
            const n = Number(text);
            const out = text === "" || !Number.isFinite(n) ? null : render(code, n);
            if (out?.[1]) c[i] = out[1];
            if (out?.[2]) f[i] = out[2];
            t.push(out ? out[0] : null);
          });
          return Object.keys(f).length > 0 ? { t, c, f } : { t, c };
        }),
      };
    }

    // The password digests of the sheet protection (PD-216) as a stand-in: the page only keeps what the answer carries
    // and asks the engine to compare, so the stand-in is a checksum of the password with the shape of the real digest.
    const offlineDigest = (password) => "OFFLINE-" + [...password].length + "-" + [...password].reduce((sum, ch) => (sum * 31 + ch.codePointAt(0)) % 1000003, 7);

    function offlineProtect(doc) {
      const password = String(doc.password ?? "");
      if (doc.sessionAction === "protectHash") {
        if (password === "") return { problem: "empty" };
        if ([...password].length > 255) return { problem: "long" };
        return { pwd: { kind: "both", alg: "SHA-512", hash: offlineDigest(password), salt: "SALT", spin: 100000, key: "KEY", keyAlg: "uri" } };
      }
      if ([...password].length > 255) return { unsupported: "long" };
      return { ok: !doc.pwd || doc.pwd.hash === offlineDigest(password) };
    }

    function offlinePivot(doc) {
      const model = host.__EASYOFFICE_SHEET_PIVOT_MODEL__;
      if (!model) throw new Error("피벗 모델을 불러올 수 없습니다.");
      const defaultSheet = resolveDefaultSheetFromDocJs(doc);
      const workbook = buildLocalWorkbookFromJson(doc, defaultSheet);
      const canonicalUpper = resolveCanonicalSheetUpper(workbook.knownSheets, defaultSheet);
      if (!canonicalUpper) throw new Error("InvalidArgument: 피벗 대상 시트를 찾을 수 없습니다.");
      const rulesApi = host.__EASYOFFICE_SHEET_RULES__;
      return model.computePivotWithReader(doc, (row0, col0) => {
        const ref = rulesApi.refOf(row0, col0);
        const typed = readWorkbookTypedCell(workbook, canonicalUpper, ref, 0);
        return { ...typed, display: formatFormulaValue(typed) };
      });
    }

    function shiftSpan0(start, end, op, at0, count, limit0) {
      if (start === 0 && end === limit0) return [0, limit0];
      if (op === "insert") {
        if (end < at0) return [start, end];
        if (start >= at0) return start + count > limit0 ? null : [start + count, Math.min(limit0, end + count)];
        return [start, Math.min(limit0, end + count)];
      }
      const de = at0 + count - 1;
      if (end < at0) return [start, end];
      if (start > de) return [start - count, end === limit0 ? limit0 : end - count];
      if (start >= at0 && end <= de) return null;
      if (start < at0) return [start, end > de ? end - count : at0 - 1];
      return [at0, end - count];
    }

    // The start and end of a chart anchor after an edit, as the engine moves them: a deleted anchor stays on the edit.
    function shiftChartSpan0(start, end, op, at0, count, limit0) {
      if (op === "insert") {
        if (start >= at0) return [Math.min(limit0, start + count), Math.min(limit0, end + count)];
        return [start, end >= at0 ? Math.min(limit0, end + count) : end];
      }
      const de = at0 + count - 1;
      if (end < at0) return [start, end];
      if (start > de) return [start - count, end - count];
      if (start >= at0 && end <= de) return [at0, at0];
      if (start >= at0) return [at0, end - count];
      return [start, end <= de ? Math.max(start, at0 - 1) : end - count];
    }

    // Charts on the default 24 x 96 px grid: a two-cell chart moves with both ends, a one-cell chart with its start, an
    // absolute one stays (PD-302).
    function offlineStructureCharts(doc, edit, at0, limit0) {
      return (doc.charts || []).map((chart) => {
        const a = { ...chart.anchor };
        if (a.editAs !== "absolute") {
          const [from, to] = edit.axis === "row" ? [a.fromRow, a.toRow] : [a.fromCol, a.toCol];
          const [nextFrom, nextTo] = shiftChartSpan0(from, to, edit.op, at0, edit.count, limit0);
          if (edit.axis === "row") [a.fromRow, a.toRow] = [nextFrom, nextTo];
          else [a.fromCol, a.toCol] = [nextFrom, nextTo];
          if (edit.axis === "row") a.top = a.fromRow * 24 + Math.round((a.rowOffEmu || 0) / 9525);
          else a.left = a.fromCol * 96 + Math.round((a.colOffEmu || 0) / 9525);
        }
        return { ...chart, anchor: a };
      });
    }

    // The references of the charts of the other sheets to the edited sheet, as the engine moves them with a formula's: only the
    // insert of rows and cell references that carry the sheet name (`S!$B$2:$B$5`) here; the sheets whose references stay are
    // left out (PD-303).
    function offlineStructureChartRefs(doc) {
      if (!doc.chartRefs) return undefined;
      const at = Number(doc.at), count = Number(doc.count);
      const name = String(doc.sheet).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      const cell = new RegExp(`(${name}!\\$?[A-Z]+\\$?)(\\d+)(?::(\\$?[A-Z]+\\$?)(\\d+))?`, "g");
      const row = (n) => (Number(n) >= at ? Number(n) + count : Number(n));
      const shift = (text) => (typeof text === "string" && doc.axis === "row" && doc.op === "insert" ? text.replace(cell, (_, a, r1, b, r2) => a + row(r1) + (b === undefined ? "" : ":" + b + row(r2))) : text);
      const out = {};
      for (const [sheet, charts] of Object.entries(doc.chartRefs)) {
        let changed = false;
        const move = (text) => {
          const next = shift(text);
          changed ||= next !== text;
          return next ?? null;
        };
        const moved = charts.map((c) => ({
          id: c.id,
          dataRange: move(c.dataRange),
          series: (c.series || []).map((s) => ({ nameRef: move(s.nameRef), categoriesRef: move(s.categoriesRef), valuesRef: move(s.valuesRef) })),
        }));
        if (changed) out[sheet] = moved;
      }
      return out;
    }

    function offlineStructure(doc) {
      const layoutApi = host.__EASYOFFICE_SHEET_LAYOUT__;
      const gridUtils = host.__EASYOFFICE_SHEET_GRID_UTILS__;
      const edit = {
        axis: doc.axis,
        op: doc.op,
        at: Number(doc.at),
        count: Number(doc.count),
        ...(Array.isArray(doc.cols) ? { cols: doc.cols } : {}),
      };
      const at0 = Math.max(0, edit.at - 1);
      const limit0 = edit.axis === "row" ? 1048575 : 16383;
      const coversCols0 = (c0, c1) => !edit.cols || (c0 >= edit.cols[0] - 1 && c1 <= edit.cols[1] - 1);

      const images = (doc.images || []).flatMap((img) => {
        const a = img?.anchor || {};
        const fc = Number(a.fromCol) || 0, fr = Number(a.fromRow) || 0;
        const tc = Math.max(fc, Number(a.toCol) || fc), tr = Math.max(fr, Number(a.toRow) || fr);
        if (edit.axis === "row" && !coversCols0(fc, tc)) return [img];
        const shifted = edit.axis === "row"
          ? shiftSpan0(fr, tr, edit.op, at0, edit.count, limit0)
          : shiftSpan0(fc, tc, edit.op, at0, edit.count, limit0);
        if (!shifted) return [];
        const [ns, ne] = shifted;
        const nextAnchor = { ...a };
        if (edit.axis === "row") {
          if (edit.op === "delete" && fr >= at0 && fr < at0 + edit.count) nextAnchor.rowOffEmu = 0;
          nextAnchor.fromRow = ns;
          nextAnchor.toRow = ne;
          nextAnchor.topPx = ns * 24 + Math.round((Number(nextAnchor.rowOffEmu) || 0) / 9525);
        } else {
          if (edit.op === "delete" && fc >= at0 && fc < at0 + edit.count) nextAnchor.colOffEmu = 0;
          nextAnchor.fromCol = ns;
          nextAnchor.toCol = ne;
          nextAnchor.leftPx = ns * 96 + Math.round((Number(nextAnchor.colOffEmu) || 0) / 9525);
        }
        return [{ ...img, anchor: nextAnchor }];
      });

      const notes = (doc.notes || []).flatMap((n) => {
        const to = layoutApi ? layoutApi.remapRef(edit, n.ref) : n.ref;
        return to === null ? [] : [{ ...n, ref: to }];
      });

      const merges = (doc.merges || []).flatMap((text) => {
        const [ra, rb] = String(text).split(":");
        const p = gridUtils?.parseRef(ra), q = gridUtils?.parseRef(rb || ra);
        if (!p || !q) return [];
        if (edit.axis === "row" && !coversCols0(p.col - 1, q.col - 1)) return [text];
        const s0 = (edit.axis === "row" ? p.row : p.col) - 1;
        const e0 = (edit.axis === "row" ? q.row : q.col) - 1;
        const sh = shiftSpan0(s0, e0, edit.op, at0, edit.count, limit0);
        if (!sh) return [];
        const r1 = edit.axis === "row" ? sh[0] + 1 : p.row, r2 = edit.axis === "row" ? sh[1] + 1 : q.row;
        const c1 = edit.axis === "col" ? sh[0] + 1 : p.col, c2 = edit.axis === "col" ? sh[1] + 1 : q.col;
        return r1 === r2 && c1 === c2 ? [] : [gridUtils.refOf(r1, c1) + ":" + gridUtils.refOf(r2, c2)];
      });

      const widths = (doc.widths || []).flatMap(([col, px]) => {
        if (edit.axis !== "col" || !layoutApi) return [[col, px]];
        const mapped = layoutApi.mapIndex(edit, Number(col));
        return mapped === null ? [] : [[mapped, px]];
      });

      // The heights follow a row edit as the engine moves them: an inserted row copies the user's height of the row above.
      const rowHeights = (doc.rowHeights || []).flatMap(([row, pt, flag]) => {
        if (edit.axis !== "row" || edit.cols || !layoutApi) return [[row, pt, ...(flag === undefined ? [] : [flag])]];
        const mapped = layoutApi.mapIndex(edit, Number(row));
        return mapped === null ? [] : [[mapped, pt, ...(flag === undefined ? [] : [flag])]];
      });
      if (edit.axis === "row" && !edit.cols && edit.op === "insert") {
        const above = (doc.rowHeights || []).find(([row, , flag]) => row === edit.at - 1 && flag !== 0);
        for (let i = 0; above && i < edit.count; i += 1) rowHeights.push([edit.at + i, above[1]]);
        rowHeights.sort((a, b) => a[0] - b[0]);
      }

      return {
        session: doc.session || "offline-s1",
        formulas: [],
        changed: [],
        circular: false,
        removed: 0,
        spillAreas: [],
        merges,
        rowHeights,
        hiddenRows: doc.hiddenRows || [],
        hiddenCols: doc.hiddenCols || [],
        filter: doc.filter ?? null,
        outline: doc.outline,
        page: doc.page,
        protect: doc.protect ?? null,
        unlockedRows: doc.unlockedRows || [],
        unlockedCols: doc.unlockedCols || [],
        tables: doc.tables || [],
        charts: offlineStructureCharts(doc, edit, at0, limit0),
        chartRefs: offlineStructureChartRefs(doc),
        images,
        notes,
        widths,
        rules: doc.rules || {},
      };
    }

    function evaluateLocalSheetRpc(payload) {
      const doc = parseRequiredRpcJsonObject(payload);
      if (doc.sessionAction === "protectHash" || doc.sessionAction === "protectVerify") return offlineProtect(doc);
      if (doc.sessionAction === "format") return offlineFormatCells(doc);
      if (doc.sessionAction === "sort") return host.__EASYOFFICE_OFFLINE_SORT__(doc);
      if (doc.sessionAction === "pivot") return offlinePivot(doc);
      if (doc.sessionAction === "open") return { session: "offline-s1" };
      if (doc.sessionAction === "close") return { closed: true };
      if (doc.sessionAction === "structure") return offlineStructure(doc);
      if (doc.sessionAction === "rules") return { session: doc.session || "offline-s1", cf: { cells: [], bars: [], icons: [], scales: [] } };
      if (doc.sessionAction === "apply" || doc.sessionAction === "merges" || doc.sessionAction === "tables" || doc.sessionAction === "locks" || doc.sessionAction === "hiddenRows" || doc.sessionAction === "names" || doc.sessionAction === "cellMeta") {
        return { session: doc.session || "offline-s1", changed: [], spills: [], circular: false };
      }
      const defaultSheet = resolveDefaultSheetFromDocJs(doc);
      const explicitAnchor = resolveOptionalAnchorCell(doc);
      const anchorCell = explicitAnchor || "A1";
      if (doc.targets !== undefined) {
        if (!Array.isArray(doc.targets)) {
          throw new TypeError("InvalidArgument: JSON 'targets' 필드는 배열이어야 합니다.");
        }
        if (doc.targets.length > 0) {
          const workbook = buildLocalWorkbookFromJson(doc, defaultSheet);
          const outMap = {};
          const targetValues = [];
          let firstExpr = "A1";
          let firstVal = "";
          doc.targets.forEach((item, idx) => {
            if (!item || typeof item !== "object" || Array.isArray(item)) {
              throw new Error("InvalidArgument: JSON 'targets' 항목은 객체여야 합니다.");
            }
            if (item.sheet !== undefined && item.sheet !== null && typeof item.sheet !== "string") {
              throw new Error("InvalidArgument: JSON 'targets' 시트명은 문자열이어야 합니다.");
            }
            if (typeof item.cell !== "string" || !item.cell.trim()) {
              throw new Error("InvalidArgument: JSON 'targets' 항목에 'cell' 필드가 필요합니다.");
            }
            const sh = jsonSheetNameOrDefaultJs(item.sheet ?? defaultSheet, defaultSheet);
            const expr = item.cell.trim();
            const canonicalUpper = resolveCanonicalSheetUpper(workbook.knownSheets, sh);
            let val = "#REF!";
            if (canonicalUpper) {
              val = isFormulaStringJs(expr)
                ? String(evaluateFormulaExpression(expr, workbook, canonicalUpper, anchorCell))
                : formatFormulaValue(readWorkbookTypedCell(workbook, canonicalUpper, expr, 0));
            }
            outMap[sh + "!" + expr] = val;
            targetValues.push(val);
            if (idx === 0) {
              firstExpr = expr;
              firstVal = val;
            }
          });
          return {
            query: firstExpr,
            value: firstVal,
            values: outMap,
            targetValues,
            circular: workbook.circular,
          };
        }
      }
      const single = evaluateSingleDocExpr(doc);
      return {
        query: single.expr,
        value: single.value,
        values: { [single.sheetName + "!" + single.expr]: single.value },
        targetValues: [single.value],
        circular: single.circular,
      };
    }

    const RIBBON_EXACT_MUTATORS = {
      "hwp-mod-manuscript": (s) => { s.manuscriptGridEnabled = !s.manuscriptGridEnabled; },
      "hwp-mod-header": (s) => { s.approvalHeaderEnabled = !s.approvalHeaderEnabled; },
      "hwp-mod-border": (s) => { s.doubleDiagonalBorderEnabled = !s.doubleDiagonalBorderEnabled; },
      "pdf-mod-rotate": (s) => { s.pdfRotationDegrees = (s.pdfRotationDegrees + 90) % 360; },
    };
    const RIBBON_SUFFIX_MUTATORS = [
      ["-home-bold", (s) => { s.isBold = !s.isBold; }],
      ["-home-italic", (s) => { s.isItalic = !s.isItalic; }],
      ["-home-underline", (s) => { s.isUnderline = !s.isUnderline; }],
      ["-home-style", (s) => { s.stylePresetIndex = (s.stylePresetIndex + 1) % 4; }],
      ["-layout-paper", (s) => { s.paperSize = s.paperSize === "A4" ? "Letter" : "A4"; }],
      ["-layout-orientation", (s) => { s.isLandscape = !s.isLandscape; }],
      ["-layout-columns", (s) => { s.columnCount = s.columnCount >= 3 ? 1 : s.columnCount + 1; }],
      ["-layout-margins", (s) => { s.marginPresetIndex = (s.marginPresetIndex + 1) % 3; }],
      ["-data-sort", (s) => { s.sortAscending = !s.sortAscending; }],
      ["-data-validation", (s) => { s.validationGuardEnabled = !s.validationGuardEnabled; }],
      ["-review-track", (s) => { s.trackChangesEnabled = !s.trackChangesEnabled; }],
      ["-review-verify", (s) => { s.securityVerifiedCount += 1; }],
      ["-view-manuscript", (s) => { s.manuscriptGridEnabled = !s.manuscriptGridEnabled; }],
      ["-view-split", (s) => { s.splitViewEnabled = !s.splitViewEnabled; }],
      ["-view-ribbon", (s) => { s.ribbonCollapsed = !s.ribbonCollapsed; }],
      ["-view-theme", (s) => { s.darkThemeEnabled = !s.darkThemeEnabled; }],
      ["-view-zoom", (s) => { s.zoomPercent = 100; }],
    ];

    function localExecuteRibbonCommand(payload) {
      let cmdId = String(payload || "").trim();
      const parsed = parseRibbonPayloadFields(cmdId);
      if (parsed) {
        cmdId = String(parsed.commandId || parsed.id || "").trim();
      }
      const session = ribbonSession(normalizeRibbonSessionId(parsed ? parsed.sessionId : ""));
      const mod = normalizeRibbonModuleKey(inferCommandModuleKey(cmdId));
      session.lastModule = mod;
      const slot = ribbonSlot(session, mod);
      const backendState = slot.state;
      if (slot.undo.length >= MAX_BACKEND_HISTORY) {
        slot.undo.shift();
      }
      slot.undo.push(structuredClone(backendState));
      slot.redo.length = 0;

      backendState.activeModule = mod;
      backendState.executedCount += 1;
      const exactFn = RIBBON_EXACT_MUTATORS[cmdId];
      if (exactFn) {
        exactFn(backendState);
      } else {
        const suffixPair = RIBBON_SUFFIX_MUTATORS.find((pair) => cmdId.endsWith(String(pair[0])));
        if (suffixPair) suffixPair[1](backendState);
      }
      return ribbonStateResponse(slot, {
        commandId: cmdId,
        summary: "명령 실행",
        contextualBadge: contextualToolBadges[mod] || "표·개체 도구",
      });
    }

    return {
      get_workspace_bootstrap: () => {
        const isMac = Boolean(host.navigator && String(host.navigator.platform || "").includes("Mac"));
        return {
          platform: isMac ? "macOS" : "Desktop",
          shortcutModifier: isMac ? "⌘" : "Ctrl",
          appVersion: "1.2.3",
          recentFormatFilters: ["전체", "DOCX", "HWP/HWPX", "XLSX", "PPTX", "PDF", "ODF"],
          openTabs: [
            { id: "tab-doc", title: "분기_경영실적_보고서.docx", module: "DOC", isPinned: true },
            { id: "tab-hwp", title: "행정안전부_기안문.hwpx", module: "HWP", isPinned: true },
            { id: "tab-sheet", title: "예산집행_총괄표.xlsx", module: "SHEET", isPinned: false },
            { id: "tab-slide", title: "전략기획_발표자료.pptx", module: "SLIDE", isPinned: false },
            { id: "tab-pdf", title: "최종계약서_검토본.pdf", module: "PDF", isPinned: false },
          ],
        };
      },
      get_ribbon_category_commands: (payload) => {
        const raw = String(payload || "DOC|home").trim();
        let modPart;
        let catPart;
        if (raw.startsWith("{")) {
          const parsed = parseRequiredRpcJsonObject(raw);
          modPart = String(parsed.module || "DOC").trim();
          catPart = String(parsed.category || "home").trim();
        } else {
          const parts = raw.split("|");
          modPart = parts[0] || "DOC";
          catPart = parts[1] || "home";
        }
        const mKey = modPart === "CONV" ? "DOC" : modPart;
        const modMap = ribbonCategoryTools[mKey] || ribbonCategoryTools.DOC;
        return { commands: modMap[catPart] || modMap.home };
      },
      detect_format: (payload) => {
        const raw = String(payload || "").trim();
        let fileName;
        if (raw.startsWith("{")) {
          const parsed = parseRequiredRpcJsonObject(raw);
          fileName = String(parsed.fileName || "document.docx");
        } else {
          fileName = raw.split("|")[0] || "document.docx";
        }
        const dotIdx = fileName.lastIndexOf(".");
        const ext = dotIdx >= 0 ? fileName.slice(dotIdx + 1).toLowerCase() : "docx";
        return {
          format: ext.toUpperCase(),
          extension: ext,
          specification: moduleSpecs[ext.toUpperCase()] || "표준 문서 포맷",
        };
      },
      convert_document_format: (payload) => {
        const raw = String(payload || "docx->hwpx|문서|본문");
        let fields;
        if (raw.trimStart().startsWith("{")) {
          const parsed = parseRequiredRpcJsonObject(raw);
          if (parsed.action === "encode_package") {
            const json = JSON.stringify({ kind: parsed.kind, model: parsed.model, media: parsed.media });
            return { status: "OK", format: parsed.kind === "base" ? "eobase" : "eoboard", outputBytes: json.length, convertedBase64: encodeUtf8Base64(json), warnings: [] };
          }
          if (parsed.sourceBase64 !== undefined) return offlineSourceConversion(parsed);
          if (parsed.action === "inspect" || parsed.action === "readback") {
            return {
              status: "OK",
              action: parsed.action,
              kind: Array.isArray(parsed.slides) ? "slideDeck" : "document",
              format: String(parsed.sourceFormat || "docx").toLowerCase(),
              layoutLint: buildOfflineLayoutLint(parsed),
            };
          }
          if (offlinePasswordLoss(parsed)) return { refused: "passwordLoss" };
          fields = parseConvertJsonFields(parsed);
        } else {
          fields = parseConvertPipeFields(raw);
        }
        const { src, dst, title, body, structuredTables } = fields;
        const encoder = new TextEncoder();
        const rawBytes = encoder.encode(title + "\n" + body).length;
        const containerOverhead = {
          docx: 640, doc: 1024, hwpx: 720, hwtx: 720, hwp: 1024, hwt: 1024, odt: 600,
          xlsx: 680, xls: 1024, ods: 620, pptx: 760, ppsx: 760, ppt: 1024, odp: 640,
          pdf: 480, rtf: 180, md: 16, csv: 24, tsv: 24, txt: 0,
        };
        const sizeSrc = src === "pages" ? "docx" : src;
        if (!(sizeSrc in containerOverhead) || !(dst in containerOverhead)) {
          throw new Error("지원하지 않는 포맷: " + src + "->" + dst);
        }
        const srcBytes = rawBytes + (containerOverhead[sizeSrc] ?? 256);
        const dstBytes = rawBytes + (containerOverhead[dst] ?? 256);
        const b64 = encodeUtf8Base64(title + "\n" + body);
        let tableCount = structuredTables;
        if (tableCount === null) {
          tableCount = body.includes("[[TABLE_ROW]]\t") ? Math.max(1, (body.match(/\[\[TABLE_NEW\]\]/g) || []).length) : 0;
        }
        return {
          sourceFormat: src,
          targetFormat: dst,
          sourceBytes: srcBytes,
          outputBytes: dstBytes,
          targetBytes: dstBytes,
          detectedTarget: dst,
          tableCount,
          specification: moduleSpecs[(dst || "").toUpperCase()] || "표준 문서 포맷",
          compatibility: "standard",
          compatibilityWarning: null,
          convertedBase64: b64,
          status: "OK",
        };
      },
      run_engine_benchmark: () => ({
        status: "ok",
        iterations: 5,
        allEquivalent: true,
        buildProfile: "release",
        timingGated: true,
        realPaths: [
          { caseId: "docx_to_hwpx_1mb", elapsedUs: 45000, outputBytes: 234482, equivalenceVerified: true },
        ],
        suites: [
          { suiteId: "crc32_slicing_by_8", baselineP50Us: 2068, optimizedP50Us: 464, speedupFactor: 4.46 },
          { suiteId: "deflate_zlib_roundtrip", baselineP50Us: 234, optimizedP50Us: 59, speedupFactor: 3.97 },
          { suiteId: "utf16_swar_and_xml_unescape", baselineP50Us: 50, optimizedP50Us: 29, speedupFactor: 1.72 },
          { suiteId: "sheet_dag_bitset_eval", baselineP50Us: 878, optimizedP50Us: 308, speedupFactor: 2.85 },
          { suiteId: "swar_csv_xml_and_zero_alloc_range_agg", baselineP50Us: 640, optimizedP50Us: 195, speedupFactor: 3.28 },
        ],
      }),
      run_smart_assistant: (payload) => localSmartAssistant(payload),
      scrub_pii: (payload) => localScrubPii(payload),
      evaluate_workbook_formula: (payload) => evaluateLocalWorkbookFormulaRpc(payload),
      evaluate_sheet: (payload) => evaluateLocalSheetRpc(payload),
      evaluate_sheet_formula: (payload) => ({
        value: evaluateSingleDocExpr(parseRequiredRpcJsonObject(payload)).value,
      }),
      audit_security: (payload) => localAuditSecurity(payload),
      switch_ribbon_style: (payload) => {
        let styleVal = String(payload || "");
        const parsed = parseRibbonPayloadFields(styleVal);
        if (parsed) {
          styleVal = String(parsed.style || "");
        }
        const isHangul = styleVal.includes("hangul");
        const session = ribbonSession(normalizeRibbonSessionId(parsed ? parsed.sessionId : ""));
        session.ribbonStyle = isHangul ? "hangul" : "standard";
        return {
          style: isHangul ? "한글(HWP) 7탭 모드" : "표준 오피스 7탭 모드",
          categories: isHangul ? ribbonStyleLabels.hangul : ribbonStyleLabels.standard,
        };
      },
      execute_ribbon_command: (payload) => localExecuteRibbonCommand(payload),
      undo_ribbon_command: (payload) => localRibbonHistoryStep(payload, true),
      redo_ribbon_command: (payload) => localRibbonHistoryStep(payload, false),
    };
  }

  host.__EASYOFFICE_CREATE_OFFLINE_RPC__ = createOfflineRpcHandlers;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
