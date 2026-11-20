// 참조 구현 (reference implementation): TanStack Table (`TanStack/table`, MIT, Copyright (c) 2016 Tanner Linsley,
// commit 1cfb2e57cbfecf25e3d4ee1bda32fbd65e4e0327, `packages/table-core/src/features/row-sorting/sortFns.ts`,
// `column-filtering/filterFns.ts`, `column-grouping/createGroupedRowModel.ts`; see `THIRD_PARTY_NOTICES.md`) 및
// 참조 앱 (`rhwp-studio`, MIT, Copyright (c) 2025-2026 Edward Kim) · 마이크로소프트 오피스 (Access / Lists) 표준 한국어 스키마.

(function (host) {
  "use strict";

  const MAX_TABLES = 32, MAX_FIELDS_PER_TABLE = 64, MAX_RECORDS_PER_TABLE = 10000;
  const MAX_CELL_CHARS = 8192, MAX_FORMULA_DEPTH = 32, MAX_FORMULA_CHARS = 4096;
  const MAX_JSON_BYTES = 64 * 1024 * 1024, MAX_FILTERS = 64, MAX_SORTS = 16, MAX_OPTIONS = 256;
  const MAX_FORMULA_CACHE = 1024, MS_PER_DAY = 86400000;

  const VIEW_MODES = new Set(["grid", "kanban", "calendar"]);
  const FIELD_TYPES = Object.freeze(["text", "number", "select", "multiSelect", "date", "checkbox", "url", "relation", "formula"]);
  const FIELD_TYPE_SET = new Set(FIELD_TYPES);
  const FIELD_TYPE_LABELS = Object.freeze({
    text: "텍스트", number: "숫자", select: "단일 선택", multiSelect: "다중 선택",
    date: "날짜", checkbox: "확인란", url: "웹 주소", relation: "관계", formula: "수식",
  });
  const FILTER_OPS = Object.freeze(["contains", "eq", "neq", "gt", "gte", "lt", "lte", "empty", "notEmpty"]);
  const FILTER_OP_SET = new Set(FILTER_OPS);
  const FILTER_OP_LABELS = Object.freeze({
    contains: "포함", eq: "일치", neq: "일치하지 않음", gt: ">", gte: ">=", lt: "<", lte: "<=", empty: "비어 있음", notEmpty: "비어 있지 않음",
  });

  const FORMULA_ERRORS = new Set(["#CYCLE!", "#DIV/0!", "#ERROR!"]);
  const FORBIDDEN_KEYS = new Set(["__proto__", "constructor", "prototype"]);
  const GROUPED_NUM_RE = /^[+-]?\d{1,3}(?:,\d{3})+(?:\.\d+)?$/;
  const PLAIN_NUM_RE = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;
  const ISO_DATE_RE = /^(\d{4})-(\d{2})-(\d{2})$/, MONTH_RE = /^(\d{4})-(0[1-9]|1[0-2])$/;
  const CTRL_OR_WS_RE = /[\u0000-\u001f\u007f\s]/;
  const FORMULA_CACHE = new Map();

  const fail = (error) => ({ ok: false, error });
  const cmpCode = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
  const isEmpty = (v) => v === null || v === undefined || v === "" || (Array.isArray(v) && v.length === 0);

  function colIndexToLetters(colIdx) {
    const ext = host.__EASYOFFICE_SHEET_UTILS__?.colIndexToLetters;
    if (typeof ext === "function") return ext(colIdx);
    let n = Math.max(0, Number(colIdx) || 0) + 1, out = "";
    for (; n > 0; n = Math.floor((n - 1) / 26)) out = String.fromCodePoint(65 + ((n - 1) % 26)) + out;
    return out || "A";
  }

  function hasForbiddenKeysDeep(val, depth) {
    if (depth > MAX_FORMULA_DEPTH) return true;
    if (!val || typeof val !== "object") return false;
    return Array.isArray(val)
      ? val.some((x) => hasForbiddenKeysDeep(x, depth + 1))
      : Object.getOwnPropertyNames(val).some((k) => FORBIDDEN_KEYS.has(k) || hasForbiddenKeysDeep(val[k], depth + 1));
  }

  function parseIsoDateUtcMs(raw) {
    const m = ISO_DATE_RE.exec(String(raw ?? "").trim());
    if (!m) return null;
    const year = Number(m[1]), month = Number(m[2]), day = Number(m[3]);
    if (month < 1 || month > 12 || day < 1 || day > 31) return null;
    const d = new Date(Date.UTC(year, month - 1, day));
    d.setUTCFullYear(year);
    return d.getUTCFullYear() === year && d.getUTCMonth() === month - 1 && d.getUTCDate() === day ? d.getTime() : null;
  }

  function parseStrictNumber(raw) {
    if (typeof raw === "number") return Number.isFinite(raw) ? raw : null;
    if (typeof raw === "boolean") return null;
    const s = String(raw ?? "").trim();
    if (!s) return null;
    const norm = PLAIN_NUM_RE.test(s) ? s : GROUPED_NUM_RE.test(s) ? s.replaceAll(",", "") : null;
    if (norm === null) return null;
    const n = Number(norm);
    return Number.isFinite(n) ? n : null;
  }

  function normalizeUrlValue(raw) {
    const s = String(raw ?? "").trim();
    if (!s) return { ok: true, value: "" };
    if (s.length > MAX_CELL_CHARS) return fail("웹 주소 길이가 상한(8,192자)을 초과했습니다.");
    if (CTRL_OR_WS_RE.test(s)) return fail("웹 주소에 공백이나 제어 문자를 포함할 수 없습니다.");
    let parsed;
    try { parsed = new URL(s); } catch { return fail("유효한 웹 주소 형식이 아닙니다."); }
    const proto = parsed.protocol.toLowerCase();
    if (proto === "http:" || proto === "https:") return parsed.hostname ? { ok: true, value: s } : fail("웹 주소에 호스트 이름이 필요합니다.");
    if (proto === "mailto:") return (parsed.pathname || "").includes("@") ? { ok: true, value: s } : fail("유효한 전자우편 주소가 아닙니다.");
    return fail("허용되지 않는 URL 스킴입니다(http, https, mailto만 허용).");
  }

  function normalizeOptions(rawOptions) {
    const arr = Array.isArray(rawOptions) ? rawOptions : String(rawOptions ?? "").split(",").map((s) => s.trim());
    const out = [], seen = new Set();
    for (let i = 0; i < arr.length && out.length < MAX_OPTIONS; i += 1) {
      const item = String(arr[i] ?? "").trim();
      if (item && !seen.has(item)) { seen.add(item); out.push(item.slice(0, 256)); }
    }
    return out;
  }

  function sanitizeValidation(rawVal) {
    if (!rawVal || typeof rawVal !== "object") return {};
    const out = {};
    if (rawVal.required) out.required = true;
    for (const k of ["min", "max"]) if (rawVal[k] !== undefined && rawVal[k] !== "" && rawVal[k] !== null && Number.isFinite(Number(rawVal[k]))) out[k] = Number(rawVal[k]);
    const ml = rawVal.maxLength !== undefined && rawVal.maxLength !== "" && rawVal.maxLength !== null ? Math.trunc(Number(rawVal.maxLength)) : NaN;
    if (Number.isFinite(ml) && ml >= 1) out.maxLength = Math.min(ml, MAX_CELL_CHARS);
    return out;
  }

  function defaultFields() {
    return [
      { id: "fld-1", name: "제목", type: "text", validation: {} },
      { id: "fld-2", name: "상태", type: "select", options: ["대기", "진행 중", "완료"], validation: {} },
      { id: "fld-3", name: "마감일", type: "date", validation: {} },
      { id: "fld-4", name: "금액", type: "number", validation: {} },
    ];
  }

  const createInitialBaseState = () => ({
    version: 1, activeTableId: "tbl-1", nextTableSeq: 2, nextFieldSeq: 5, nextRecordSeq: 1,
    tables: [{
      id: "tbl-1", name: "테이블 1", view: "grid", kanbanFieldId: "fld-2", calendarFieldId: "fld-3", calendarMonth: "2026-11",
      filters: [], filterConjunction: "and", sorts: [], groupByFieldId: "", collapsedGroups: [], fields: defaultFields(), records: [],
    }],
  });

  function buildStateIndex(state) {
    const tablesById = new Map(), recordMapsByTableId = new Map(), fieldsByTableId = new Map();
    for (const tbl of state?.tables || []) {
      if (!tbl?.id) continue;
      tablesById.set(tbl.id, tbl);
      const recMap = new Map(), byId = new Map(), byName = new Map(), byNameLower = new Map(), fields = tbl.fields || [];
      for (const rec of tbl.records || []) if (rec?.id) recMap.set(rec.id, rec);
      for (const fld of fields) if (fld?.id) {
        const nm = String(fld.name || "");
        byId.set(fld.id, fld); byName.set(nm, fld); byNameLower.set(nm.trim().toLowerCase(), fld);
      }
      recordMapsByTableId.set(tbl.id, recMap);
      fieldsByTableId.set(tbl.id, { byId, byName, byNameLower, primaryField: fields[0] || null });
    }
    return { tablesById, recordMapsByTableId, fieldsByTableId };
  }

  const getTable = (state, tableId) => (state?.tables || []).find((t) => t.id === (tableId || state?.activeTableId)) || state?.tables?.[0] || null;

  const CELL_VALIDATORS = {
    text(field, raw, v) {
      const str = raw == null ? "" : String(raw);
      if (str.length > MAX_CELL_CHARS) return fail(`셀 값은 최대 ${MAX_CELL_CHARS}자까지 허용됩니다.`);
      if (v.required && !str.trim()) return fail(`'${field.name}' 항목은 필수 입력입니다.`);
      if (v.maxLength !== undefined && Array.from(str).length > Number(v.maxLength)) return fail(`'${field.name}' 항목은 최대 ${v.maxLength}자까지 입력할 수 있습니다.`);
      return { ok: true, value: str };
    },
    number(field, raw, v) {
      if (raw == null || (typeof raw === "string" && !raw.trim())) return v.required ? fail(`'${field.name}' 숫자 항목은 필수 입력입니다.`) : { ok: true, value: null };
      const num = parseStrictNumber(raw);
      if (num === null) return fail(`'${field.name}' 항목에는 올바른 숫자를 입력해 주세요.`);
      if (v.min !== undefined && num < Number(v.min)) return fail(`'${field.name}' 값은 최솟값(${v.min}) 이상이어야 합니다.`);
      if (v.max !== undefined && num > Number(v.max)) return fail(`'${field.name}' 값은 최댓값(${v.max}) 이하여야 합니다.`);
      return { ok: true, value: num };
    },
    select(field, raw, v) {
      const str = raw == null ? "" : String(raw).trim();
      if (!str) return v.required ? fail(`'${field.name}' 항목을 선택해 주세요.`) : { ok: true, value: "" };
      return (field.options || []).includes(str) ? { ok: true, value: str } : fail(`'${field.name}'에 허용되지 않은 선택 값입니다: ${str}`);
    },
    multiSelect(field, raw, v) {
      const arr = !raw ? [] : Array.isArray(raw) ? raw.map((x) => String(x ?? "").trim()).filter(Boolean) : String(raw).split(",").map((s) => s.trim()).filter(Boolean);
      const opts = new Set(field.options || []), deduped = [], seen = new Set();
      let charSum = 0;
      for (const item of arr) {
        if (!opts.has(item)) return fail(`'${field.name}'에 허용되지 않은 항목입니다: ${item}`);
        if (!seen.has(item)) { seen.add(item); deduped.push(item); if ((charSum += item.length + 2) > MAX_CELL_CHARS) return fail("다중 선택 항목 크기가 상한을 초과했습니다."); }
      }
      return v.required && !deduped.length ? fail(`'${field.name}' 항목을 하나 이상 선택해 주세요.`) : { ok: true, value: deduped };
    },
    date(field, raw, v) {
      const str = raw == null ? "" : String(raw).trim();
      if (!str) return v.required ? fail(`'${field.name}' 날짜 항목은 필수 입력입니다.`) : { ok: true, value: "" };
      return parseIsoDateUtcMs(str) === null ? fail(`'${field.name}' 날짜가 유효한 YYYY-MM-DD 달력 일자가 아닙니다: ${str}`) : { ok: true, value: str };
    },
    checkbox(field, raw, v) {
      let b = typeof raw === "boolean" ? raw : (raw == null || raw === "" || raw === 0) ? false : raw === 1 ? true : null;
      if (b === null && typeof raw === "string") {
        const low = raw.trim().toLowerCase();
        b = (low === "true" || low === "1" || low === "예") ? true : (low === "false" || low === "0" || low === "아니오") ? false : null;
      }
      if (b === null) return fail(`'${field.name}' 확인란 값이 올바르지 않습니다.`);
      return v.required && !b ? fail(`'${field.name}' 항목을 선택해 주세요.`) : { ok: true, value: b };
    },
    url(field, raw, v) {
      const res = normalizeUrlValue(raw);
      if (!res.ok) return res;
      if (v.required && !res.value) return fail(`'${field.name}' 웹 주소 항목은 필수 입력입니다.`);
      if (v.maxLength !== undefined && Array.from(res.value).length > Number(v.maxLength)) return fail(`'${field.name}' 웹 주소가 최대 글자 수(${v.maxLength})를 초과했습니다.`);
      return res;
    },
    relation(field, raw, v, state, indexOpt) {
      const idx = indexOpt || buildStateIndex(state);
      const recMap = field.relationTableId ? idx.recordMapsByTableId.get(field.relationTableId) : null;
      if (!recMap) return fail(`'${field.name}'의 관계 대상 테이블이 존재하지 않습니다.`);
      const isArr = Array.isArray(raw);
      const ids = raw == null || raw === "" ? [] : isArr ? raw.map((x) => String(x ?? "").trim()).filter(Boolean) : [String(raw).trim()].filter(Boolean);
      const deduped = [], seen = new Set();
      for (const id of ids) {
        if (id.length > 128 || !recMap.has(id)) return fail(`'${field.name}'에 존재하지 않는 관계 레코드 ID입니다: ${id}`);
        if (!seen.has(id)) { seen.add(id); deduped.push(id); }
      }
      return v.required && deduped.length === 0 ? fail(`'${field.name}' 관계 항목을 선택해 주세요.`) : { ok: true, value: isArr ? deduped : (deduped[0] || "") };
    },
  };

  function validateCellValue(field, rawValue, state, indexOpt) {
    if (!field || !FIELD_TYPE_SET.has(field.type)) return fail("알 수 없는 필드 유형입니다.");
    if (field.type === "formula") return fail("수식 필드는 직접 편집할 수 없습니다.");
    if (typeof rawValue === "string" && rawValue.length > MAX_CELL_CHARS) return fail(`셀 값은 최대 ${MAX_CELL_CHARS}자까지 허용됩니다.`);
    return CELL_VALIDATORS[field.type](field, rawValue, field.validation || {}, state, indexOpt);
  }

  // ---- Deterministic Formula Tokenizer, Parser & Evaluator ----

  function tokenizeFormula(src) {
    const tokens = [];
    for (let i = 0; i < src.length;) {
      const ch = src[i];
      if (/\s/.test(ch)) { i += 1; continue; }
      if (ch === '"' || ch === "'") {
        let j = i + 1, out = "", closed = false;
        for (; j < src.length;) {
          const c = src[j];
          if (c === "\\" && j + 1 < src.length) { const e = src[j + 1]; out += e === "n" ? "\n" : e === "t" ? "\t" : e; j += 2; }
          else if (c === ch) { closed = true; j += 1; break; }
          else { out += c; j += 1; }
        }
        if (!closed) return fail("닫히지 않은 문자열 리터럴입니다.");
        tokens.push({ t: "str", v: out }); i = j;
      } else if (ch === "{") {
        const close = src.indexOf("}", i + 1), name = close >= 0 ? src.slice(i + 1, close) : "";
        if (close < 0) return fail("닫히지 않은 필드 참조입니다.");
        if (!name.trim() || name.includes("{")) return fail("유효하지 않은 필드 참조입니다.");
        tokens.push({ t: "field", v: name }); i = close + 1;
      } else if (/[0-9]/.test(ch) || (ch === "." && i + 1 < src.length && /[0-9]/.test(src[i + 1]))) {
        const m = /^(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?/.exec(src.slice(i));
        if (!m) return fail("유효하지 않은 숫자 리터럴입니다.");
        tokens.push({ t: "num", v: Number(m[0]) }); i += m[0].length;
      } else if (/[A-Za-z_]/.test(ch)) {
        let j = i + 1;
        while (j < src.length && /[A-Za-z0-9_]/.test(src[j])) j += 1;
        tokens.push({ t: "ident", v: src.slice(i, j).toUpperCase() }); i = j;
      } else if (["==", "!=", "<>", ">=", "<="].includes(src.slice(i, i + 2))) {
        tokens.push({ t: "op", v: src.slice(i, i + 2) }); i += 2;
      } else if ("+-*/&=<>".includes(ch)) { tokens.push({ t: "op", v: ch }); i += 1; }
      else if (ch === "(" || ch === ")" || ch === ",") { tokens.push({ t: ch, v: ch }); i += 1; }
      else return fail("알 수 없는 수식 토큰: " + ch);
    }
    return { ok: true, tokens };
  }

  function compileFormula(rawFormula) {
    const src = String(rawFormula ?? ""), cached = FORMULA_CACHE.get(src);
    if (cached) return cached;
    if (src.length > MAX_FORMULA_CHARS) return { ok: false, depthExceeded: true, error: `수식은 최대 ${MAX_FORMULA_CHARS}자까지 허용됩니다.` };
    const trimmed = src.trim();
    if (!trimmed) return { ok: true, ast: { kind: "lit", v: "" } };
    const lex = tokenizeFormula(trimmed);
    if (!lex.ok) { const r = { ok: false, depthExceeded: false, error: lex.error }; if (FORMULA_CACHE.size < MAX_FORMULA_CACHE) FORMULA_CACHE.set(src, r); return r; }
    const tokens = lex.tokens;
    let pos = 0, depthExceeded = false, result;
    const peek = () => tokens[pos] || null, take = () => tokens[pos++] || null;
    const enter = (d) => { if (d > MAX_FORMULA_DEPTH) { depthExceeded = true; throw new Error("수식 중첩 깊이 상한(32)을 초과했습니다."); } };
    const parseBin = (d, ops, nextFn) => {
      let left = nextFn(d);
      while (peek()?.t === "op" && ops.includes(peek().v)) left = { kind: "bin", op: take().v, left, right: nextFn(d) };
      return left;
    };
    const parseCmp = (d) => (enter(d), parseBin(d, ["=", "==", "!=", "<>", "<", "<=", ">", ">="], parseConcat));
    const parseConcat = (d) => parseBin(d, ["&"], parseAdd), parseAdd = (d) => parseBin(d, ["+", "-"], parseMul);
    const parseMul = (d) => parseBin(d, ["*", "/"], parseUnary);
    const parseUnary = (d) => (enter(d), peek()?.t === "op" && (peek().v === "-" || peek().v === "+") ? { kind: "unary", op: take().v, arg: parseUnary(d + 1) } : parsePrimary(d));
    function parsePrimary(d) {
      enter(d);
      const tok = take();
      if (!tok) throw new Error("예기치 않은 수식 끝입니다.");
      if (tok.t === "num" || tok.t === "str") return { kind: "lit", v: tok.v };
      if (tok.t === "field") return { kind: "field", name: tok.v };
      if (tok.t === "(") { const expr = parseCmp(d + 1); if (take()?.t !== ")") throw new Error("닫는 괄호가 필요합니다."); return expr; }
      if (tok.t === "ident" && peek()?.t === "(") {
        take(); const args = [];
        if (peek()?.t !== ")") do { args.push(parseCmp(d + 1)); } while (peek()?.t === "," && take());
        if (take()?.t !== ")") throw new Error("함수 호출 닫는 괄호가 필요합니다.");
        return { kind: "call", name: tok.v, args };
      }
      if (tok.t === "ident" && (tok.v === "TRUE" || tok.v === "FALSE")) return { kind: "lit", v: tok.v === "TRUE" };
      throw new Error(tok.t === "ident" ? "알 수 없는 식별자: " + tok.v : "잘못된 수식 구문입니다.");
    }
    try { const ast = parseCmp(1); if (pos < tokens.length) throw new Error("수식 뒤에 예상치 못한 토큰이 있습니다."); result = { ok: true, ast }; }
    catch (err) { result = { ok: false, depthExceeded, error: err?.message || "수식 구문 오류입니다." }; }
    if (FORMULA_CACHE.size >= MAX_FORMULA_CACHE) FORMULA_CACHE.clear();
    FORMULA_CACHE.set(src, result);
    return result;
  }

  const fStr = (v) => (v == null ? "" : typeof v === "boolean" ? (v ? "TRUE" : "FALSE") : Array.isArray(v) ? v.join(", ") : String(v));
  const fNum = (v) => (typeof v === "number" ? (Number.isFinite(v) ? v : null) : typeof v === "boolean" ? (v ? 1 : 0) : v == null || v === "" ? 0 : typeof v === "string" ? parseStrictNumber(v) : null);
  const fTruthy = (v) => (typeof v === "boolean" ? v : typeof v === "number" ? v !== 0 && Number.isFinite(v) : v == null ? false : Array.isArray(v) ? v.length > 0 : !["", "FALSE", "false", "0"].includes(String(v)));
  const UNARY_STR_FNS = { LEN: (s) => Array.from(s).length, UPPER: (s) => s.toUpperCase(), LOWER: (s) => s.toLowerCase(), TRIM: (s) => s.trim() };

  function evalFormulaAst(node, ctx, depth) {
    if (depth > MAX_FORMULA_DEPTH) return "#CYCLE!";
    if (!node) return "#ERROR!";
    if (node.kind === "lit") return node.v;
    if (node.kind === "field") {
      const m = ctx.fieldsMeta, target = m.byName.get(node.name) || m.byNameLower.get(node.name.trim().toLowerCase()) || m.byId.get(node.name);
      if (!target) return "#ERROR!";
      if (target.type === "formula") {
        if (ctx.visiting.has(target.id) || ctx.fieldDepth >= MAX_FORMULA_DEPTH) return "#CYCLE!";
        if (ctx.evalCache.has(target.id)) return ctx.evalCache.get(target.id);
        ctx.visiting.add(target.id);
        const c = compileFormula(target.formula);
        const val = c.ok ? evalFormulaAst(c.ast, { ...ctx, fieldDepth: ctx.fieldDepth + 1 }, 1) : "#ERROR!";
        ctx.visiting.delete(target.id); ctx.evalCache.set(target.id, val);
        return val;
      }
      const raw = ctx.record?.values?.[target.id];
      return target.type === "relation" ? resolveRelationDisplay(ctx.state, target, raw, ctx.index)
        : target.type === "multiSelect" ? (Array.isArray(raw) ? raw.join(", ") : String(raw ?? "")) : (raw === undefined ? null : raw);
    }
    if (node.kind === "unary") {
      const v = evalFormulaAst(node.arg, ctx, depth + 1);
      if (FORMULA_ERRORS.has(v)) return v;
      const n = fNum(v);
      return n === null ? "#ERROR!" : node.op === "-" ? (n === 0 ? 0 : -n) : n;
    }
    if (node.kind === "bin") {
      const l = evalFormulaAst(node.left, ctx, depth + 1); if (FORMULA_ERRORS.has(l)) return l;
      const r = evalFormulaAst(node.right, ctx, depth + 1); if (FORMULA_ERRORS.has(r)) return r;
      const op = node.op;
      if (op === "&") return fStr(l) + fStr(r);
      if ("+-*/".includes(op)) {
        const ln = fNum(l), rn = fNum(r);
        if (ln === null || rn === null) return "#ERROR!";
        return op === "+" ? ln + rn : op === "-" ? ln - rn : op === "*" ? ln * rn : (rn === 0 || !Number.isFinite(ln / rn) ? "#DIV/0!" : ln / rn);
      }
      const ln = typeof l === "number" ? l : (l == null || l === "" ? (op === "=" || op === "==" || op === "!=" || op === "<>" ? null : 0) : parseStrictNumber(l));
      const rn = typeof r === "number" ? r : (r == null || r === "" ? (op === "=" || op === "==" || op === "!=" || op === "<>" ? null : 0) : parseStrictNumber(r));
      if (op === "=" || op === "==" || op === "!=" || op === "<>") {
        const eq = ln !== null && rn !== null ? ln === rn : typeof l === "boolean" && typeof r === "boolean" ? l === r : fStr(l) === fStr(r);
        return op === "!=" || op === "<>" ? !eq : eq;
      }
      const cmp = ln !== null && rn !== null && !(typeof l === "string" && typeof r === "string" && parseStrictNumber(l) === null)
        ? (ln < rn ? -1 : ln > rn ? 1 : 0) : cmpCode(fStr(l), fStr(r));
      return op === "<" ? cmp < 0 : op === "<=" ? cmp <= 0 : op === ">" ? cmp > 0 : cmp >= 0;
    }
    return node.kind === "call" ? evalFormulaCall(node.name, node.args, ctx, depth) : "#ERROR!";
  }

  function evalFormulaCall(fn, argNodes, ctx, depth) {
    if (fn === "IF") {
      if (argNodes.length < 2 || argNodes.length > 3) return "#ERROR!";
      const cond = evalFormulaAst(argNodes[0], ctx, depth + 1);
      if (FORMULA_ERRORS.has(cond)) return cond;
      return fTruthy(cond) ? evalFormulaAst(argNodes[1], ctx, depth + 1) : (argNodes.length === 3 ? evalFormulaAst(argNodes[2], ctx, depth + 1) : "");
    }
    const vals = [];
    for (const a of argNodes) { const v = evalFormulaAst(a, ctx, depth + 1); if (FORMULA_ERRORS.has(v)) return v; vals.push(v); }
    if (fn === "AND") return vals.length ? vals.every(fTruthy) : "#ERROR!";
    if (fn === "OR") return vals.length ? vals.some(fTruthy) : "#ERROR!";
    if (fn === "NOT") return vals.length === 1 ? !fTruthy(vals[0]) : "#ERROR!";
    if (fn === "ROUND") {
      if (vals.length < 1 || vals.length > 2) return "#ERROR!";
      const n = fNum(vals[0]), d = vals.length === 2 ? fNum(vals[1]) : 0;
      if (n === null || d === null) return "#ERROR!";
      const dig = Math.max(-15, Math.min(15, Math.trunc(d))), f = Math.pow(10, dig);
      return dig >= 0 ? Math.round((n + Math.sign(n) * 1e-12) * f) / f : Math.round(n * f) / f;
    }
    if (fn === "ABS") { const n = vals.length === 1 ? fNum(vals[0]) : null; return n === null ? "#ERROR!" : Math.abs(n); }
    if (fn === "SUM" || fn === "AVERAGE" || fn === "MIN" || fn === "MAX") {
      if (!vals.length) return "#ERROR!";
      const nums = [];
      for (const v of vals) { if (v == null || v === "") { if (fn === "SUM") nums.push(0); continue; } const n = fNum(v); if (n === null) return "#ERROR!"; nums.push(n); }
      if (!nums.length) return fn === "AVERAGE" ? "#DIV/0!" : 0;
      if (fn === "SUM") return nums.reduce((a, b) => a + b, 0);
      if (fn === "AVERAGE") return nums.reduce((a, b) => a + b, 0) / nums.length;
      return fn === "MIN" ? Math.min(...nums) : Math.max(...nums);
    }
    if (fn === "CONCAT") return vals.map(fStr).join("");
    if (UNARY_STR_FNS[fn]) return vals.length === 1 ? UNARY_STR_FNS[fn](fStr(vals[0])) : "#ERROR!";
    if (fn === "DAYS") {
      if (vals.length !== 2) return "#ERROR!";
      const s1 = fStr(vals[0]).trim(), s2 = fStr(vals[1]).trim();
      if (!s1 || !s2) return null;
      const ms1 = parseIsoDateUtcMs(s1), ms2 = parseIsoDateUtcMs(s2);
      return ms1 === null || ms2 === null ? "#ERROR!" : Math.round((ms1 - ms2) / MS_PER_DAY);
    }
    if (fn === "COALESCE") { const hit = vals.find((v) => v !== null && v !== undefined && v !== ""); return hit === undefined ? "" : hit; }
    return "#ERROR!";
  }

  function evaluateFormula(formulaStr, record, table, state, options = {}) {
    const compiled = compileFormula(formulaStr);
    if (!compiled.ok) return "#ERROR!";
    const index = options.index || buildStateIndex(state);
    const fieldsMeta = options.fieldsMeta || index.fieldsByTableId.get(table.id) || { byId: new Map(), byName: new Map(), byNameLower: new Map() };
    return evalFormulaAst(compiled.ast, {
      state, table, record, index, fieldsMeta,
      visiting: options.visiting || new Set(), evalCache: options.evalCache || new Map(), fieldDepth: options.fieldDepth || 0,
    }, 1);
  }

  // ---- O(1) Relation Display & Rollup Aggregation ----

  function lookupRelationTarget(state, field, rawValue, indexOpt) {
    if (!field?.relationTableId) return null;
    const ids = Array.isArray(rawValue) ? rawValue : (rawValue ? [String(rawValue)] : []);
    if (!ids.length) return null;
    const index = indexOpt || buildStateIndex(state);
    const targetTable = index.tablesById.get(field.relationTableId);
    const recMap = index.recordMapsByTableId.get(field.relationTableId);
    const meta = index.fieldsByTableId.get(field.relationTableId);
    return targetTable && recMap && meta ? { ids, index, targetTable, recMap, meta } : null;
  }

  const readTargetVal = (state, ctx, fld, rec) => (
    fld.type === "formula" ? evaluateFormula(fld.formula, rec, ctx.targetTable, state, { index: ctx.index, fieldsMeta: ctx.meta }) : rec.values?.[fld.id]
  );

  function resolveRelationItems(state, field, rawValue, indexOpt) {
    const ctx = lookupRelationTarget(state, field, rawValue, indexOpt);
    if (!ctx) return [];
    const primary = ctx.meta.primaryField, items = [];
    for (const recId of ctx.ids) {
      const rec = ctx.recMap.get(recId);
      if (!rec) continue;
      if (!primary) { items.push({ id: recId, label: recId }); continue; }
      items.push({ id: recId, label: formatCellDisplay(primary, readTargetVal(state, ctx, primary, rec), state, ctx.index) || recId });
    }
    return items;
  }

  const resolveRelationDisplay = (state, field, rawValue, indexOpt) => resolveRelationItems(state, field, rawValue, indexOpt).map((i) => i.label).join(", ");

  function rollupSum(state, relationField, rawValue, targetFieldIdOrName, indexOpt) {
    const ctx = lookupRelationTarget(state, relationField, rawValue, indexOpt);
    if (!ctx) return 0;
    const targetField = targetFieldIdOrName
      ? (ctx.meta.byId.get(targetFieldIdOrName) || ctx.meta.byName.get(targetFieldIdOrName) || ctx.meta.byNameLower.get(String(targetFieldIdOrName).trim().toLowerCase()))
      : ctx.targetTable.fields.find((f) => f.type === "number");
    if (!targetField) return 0;
    let sum = 0;
    for (const id of ctx.ids) {
      const rec = ctx.recMap.get(id);
      if (!rec) continue;
      const raw = readTargetVal(state, ctx, targetField, rec);
      const n = typeof raw === "number" ? raw : parseStrictNumber(raw);
      if (n !== null && Number.isFinite(n)) sum += n;
    }
    return sum;
  }

  function formatCellDisplay(field, val, state, index) {
    if (val === null || val === undefined) return "";
    if (field?.type === "checkbox" || typeof val === "boolean") return val ? "TRUE" : "FALSE";
    if (field?.type === "multiSelect") return Array.isArray(val) ? val.join(", ") : String(val);
    if (field?.type === "relation") return resolveRelationDisplay(state, field, val, index);
    if (typeof val === "number") return Number.isFinite(val) ? String(val) : "";
    return String(val);
  }

  // ---- Single-Pass Filter, Multi-Sort, Grouping & Aggregation ----

  function matchesFilterRule(rule, row, field) {
    const val = row.values[field.id], disp = row.displays[field.id], op = rule.op || "contains";
    if (op === "empty") return isEmpty(val);
    if (op === "notEmpty") return !isEmpty(val);
    const targetRaw = String(rule.value ?? ""), targetTrim = targetRaw.trim();
    if (op === "contains") return disp.toLowerCase().includes(targetTrim.toLowerCase());
    if (op === "eq" || op === "neq") {
      let equal = false;
      if (field.type === "checkbox" || typeof val === "boolean") {
        const low = targetTrim.toLowerCase(), isYes = low === "true" || low === "1" || low === "예";
        equal = (isYes || low === "false" || low === "0" || low === "아니오") && Boolean(val) === isYes;
      } else if (field.type === "number" || typeof val === "number") {
        const n1 = typeof val === "number" ? val : parseStrictNumber(val), n2 = parseStrictNumber(targetRaw);
        equal = n1 !== null && n2 !== null ? n1 === n2 : disp === targetTrim;
      } else equal = (Array.isArray(val) && val.includes(targetTrim)) || disp === targetTrim;
      return op === "eq" ? equal : !equal;
    }
    if (isEmpty(val) || FORMULA_ERRORS.has(val)) return false;
    const n1 = typeof val === "number" ? val : parseStrictNumber(val), n2 = parseStrictNumber(targetRaw);
    const cmp = n1 !== null && n2 !== null ? (n1 < n2 ? -1 : n1 > n2 ? 1 : 0) : cmpCode(disp, targetTrim);
    return op === "gt" ? cmp > 0 : op === "gte" ? cmp >= 0 : op === "lt" ? cmp < 0 : op === "lte" ? cmp <= 0 : true;
  }

  // 무할당 단일 패스 영숫자 청크 비교 (`item2` < `item10`, 한국어 로캘 청크 폴백 — 출처는 파일 상단 및 `THIRD_PARTY_NOTICES.md` 참조)
  const isAsciiDigit = (cc) => cc >= 48 && cc <= 57;
  const findChunkEnd = (s, start, num) => {
    let e = start + 1;
    while (e < s.length && isAsciiDigit(s.charCodeAt(e)) === num) e += 1;
    return e;
  };
  const parseSmallInt = (s, start, end) => {
    let out = 0;
    for (let i = start; i < end; i += 1) out = out * 10 + (s.charCodeAt(i) - 48);
    return out;
  };
  function compareNumericChunks(aStr, aStart, aEnd, bStr, bStart, bEnd) {
    let as = aStart, bs = bStart;
    while (as < aEnd && aStr.charCodeAt(as) === 48) as += 1;
    while (bs < bEnd && bStr.charCodeAt(bs) === 48) bs += 1;
    const al = aEnd - as, bl = bEnd - bs;
    if (al === 0 && bl === 0) return 0;
    if (al !== bl) return al > bl ? 1 : -1;
    if (al <= 15) return cmpCode(parseSmallInt(aStr, as, aEnd), parseSmallInt(bStr, bs, bEnd));
    for (let i = 0; i < al; i += 1) { const d = aStr.charCodeAt(as + i) - bStr.charCodeAt(bs + i); if (d !== 0) return d > 0 ? 1 : -1; }
    return 0;
  }
  const countRemainingChunks = (s, start) => {
    let c = 0, i = start;
    for (; i < s.length; c += 1) i = findChunkEnd(s, i, isAsciiDigit(s.charCodeAt(i)));
    return c;
  };
  function compareAlphanumeric(aStr, bStr) {
    for (let ai = 0, bi = 0; ;) {
      if (ai >= aStr.length || bi >= bStr.length) return countRemainingChunks(aStr, ai) - countRemainingChunks(bStr, bi);
      const aNum = isAsciiDigit(aStr.charCodeAt(ai)), bNum = isAsciiDigit(bStr.charCodeAt(bi));
      const aEnd = findChunkEnd(aStr, ai, aNum), bEnd = findChunkEnd(bStr, bi, bNum);
      const c = !aNum && !bNum ? aStr.slice(ai, aEnd).localeCompare(bStr.slice(bi, bEnd), "ko")
        : aNum !== bNum ? (aNum ? 1 : -1) : compareNumericChunks(aStr, ai, aEnd, bStr, bi, bEnd);
      if (c !== 0) return c;
      ai = aEnd; bi = bEnd;
    }
  }

  function compareFieldValues(aVal, bVal, aDisp, bDisp, field) {
    if (field.type === "checkbox" || (typeof aVal === "boolean" && typeof bVal === "boolean")) return (aVal ? 1 : 0) - (bVal ? 1 : 0);
    if (field.type === "number" || (typeof aVal === "number" && typeof bVal === "number")) {
      const an = typeof aVal === "number" ? aVal : parseStrictNumber(aVal), bn = typeof bVal === "number" ? bVal : parseStrictNumber(bVal);
      if (an !== null && bn !== null) return an - bn;
      if (an !== null || bn !== null) return an !== null ? -1 : 1;
    }
    return field.type === "date" ? cmpCode(aDisp, bDisp) : compareAlphanumeric(aDisp, bDisp);
  }

  function addRowSums(row, sumFields, bucketSums, totalSums) {
    for (const sf of sumFields) {
      const v = row.values[sf.id];
      if (typeof v === "number" && Number.isFinite(v)) { if (bucketSums) bucketSums[sf.id] += v; totalSums[sf.id] += v; }
    }
  }

  function queryTableView(state, tableId) {
    const table = getTable(state, tableId);
    if (!table) return { table: null, fields: [], totalCount: 0, filteredCount: 0, rows: [], groups: [], numericSums: {}, index: buildStateIndex(state) };
    const index = buildStateIndex(state), fieldsMeta = index.fieldsByTableId.get(table.id), fields = table.fields, records = table.records;
    const formulaFields = [], sumFields = [], compiledById = new Map();
    for (const fld of fields) {
      if (fld.type === "formula") { formulaFields.push(fld); sumFields.push(fld); compiledById.set(fld.id, compileFormula(fld.formula)); }
      else if (fld.type === "number") sumFields.push(fld);
    }
    const activeFilters = (table.filters || []).map((rule) => ({ rule, field: fieldsMeta.byId.get(rule?.fieldId) })).filter((x) => x.field);
    const useOr = table.filterConjunction === "or", filteredRows = [];

    for (let r = 0; r < records.length; r += 1) {
      const rec = records[r], values = Object.create(null), displays = Object.create(null), evalCache = new Map(), visiting = new Set();
      for (const fld of fields) if (fld.type !== "formula") {
        const val = rec.values?.[fld.id];
        values[fld.id] = val === undefined ? null : val;
        displays[fld.id] = formatCellDisplay(fld, val, state, index);
      }
      if (formulaFields.length) {
        const ctx = { state, table, record: rec, index, fieldsMeta, visiting, evalCache, fieldDepth: 0 };
        for (const ff of formulaFields) {
          let val = evalCache.has(ff.id) ? evalCache.get(ff.id) : undefined;
          if (val === undefined) {
            visiting.add(ff.id);
            const c = compiledById.get(ff.id);
            val = c?.ok ? evalFormulaAst(c.ast, ctx, 1) : "#ERROR!";
            visiting.delete(ff.id); evalCache.set(ff.id, val);
          }
          values[ff.id] = val; displays[ff.id] = formatCellDisplay(ff, val, state, index);
        }
      }
      const row = { record: rec, values, displays, origIdx: r };
      if (!activeFilters.length) { filteredRows.push(row); continue; }
      let ok = !useOr;
      for (const { rule, field } of activeFilters) {
        const hit = matchesFilterRule(rule, row, field);
        if (useOr && hit) { ok = true; break; }
        if (!useOr && !hit) { ok = false; break; }
      }
      if (ok) filteredRows.push(row);
    }

    const activeSorts = (table.sorts || []).map((s) => ({ dir: s?.dir === "desc" ? -1 : 1, field: fieldsMeta.byId.get(s?.fieldId) })).filter((x) => x.field);
    if (activeSorts.length) filteredRows.sort((a, b) => {
      for (const { field, dir } of activeSorts) {
        const ae = isEmpty(a.values[field.id]), be = isEmpty(b.values[field.id]);
        if (ae && be) continue;
        if (ae || be) return ae ? 1 : -1;
        const cmp = compareFieldValues(a.values[field.id], b.values[field.id], a.displays[field.id], b.displays[field.id], field);
        if (cmp !== 0) return cmp * dir;
      }
      return a.origIdx - b.origIdx;
    });

    const makeSums = () => Object.fromEntries(sumFields.map((f) => [f.id, 0])), numericSums = makeSums();
    const groupField = table.groupByFieldId ? fieldsMeta.byId.get(table.groupByFieldId) : null;
    if (!groupField) {
      for (const row of filteredRows) addRowSums(row, sumFields, null, numericSums);
      return {
        table, fields, totalCount: records.length, filteredCount: filteredRows.length, rows: filteredRows,
        groups: [{ key: "__all__", label: "", records: filteredRows, count: filteredRows.length, numericSums, collapsed: false }], numericSums, index,
      };
    }
    const collapsedSet = new Set(table.collapsedGroups || []), groupMap = new Map();
    for (const row of filteredRows) {
      const rawVal = row.values[groupField.id], disp = row.displays[groupField.id];
      const gKey = groupField.type === "checkbox" ? (rawVal ? "true" : "false") : isEmpty(rawVal) ? "__empty__" : disp;
      const gLabel = groupField.type === "checkbox" ? (rawVal ? "선택됨" : "선택 안 됨") : isEmpty(rawVal) ? "미분류" : disp;
      let bucket = groupMap.get(gKey);
      if (!bucket) { bucket = { key: gKey, label: gLabel, records: [], count: 0, numericSums: makeSums(), collapsed: collapsedSet.has(gKey) }; groupMap.set(gKey, bucket); }
      bucket.records.push(row); bucket.count += 1; addRowSums(row, sumFields, bucket.numericSums, numericSums);
    }
    const groups = Array.from(groupMap.values());
    if (groupField.type === "select" && Array.isArray(groupField.options)) {
      const orderMap = new Map(groupField.options.map((opt, idx) => [opt, idx]));
      groups.sort((a, b) => (a.key === "__empty__" ? 1e9 : (orderMap.get(a.key) ?? 1e8)) - (b.key === "__empty__" ? 1e9 : (orderMap.get(b.key) ?? 1e8)) || cmpCode(a.label, b.label));
    }
    return { table, fields, totalCount: records.length, filteredCount: filteredRows.length, rows: filteredRows, groups, numericSums, index };
  }

  // ---- Mutations (Strict Budget Guards) ----

  function normalizeFieldConfig(state, type, spec, prev = {}) {
    if (type === "relation") {
      const relId = String(spec.relationTableId ?? prev.relationTableId ?? "");
      return state.tables.some((t) => t.id === relId) ? { ok: true, extra: { relationTableId: relId } } : fail("유효한 관계 대상 테이블을 선택해 주세요.");
    }
    if (type === "formula") {
      const fStr = String(spec.formula ?? prev.formula ?? "");
      if (fStr.length > MAX_FORMULA_CHARS) return fail("수식 길이 상한을 초과했습니다.");
      const c = compileFormula(fStr);
      return !c.ok && c.depthExceeded ? fail(c.error) : { ok: true, extra: { formula: fStr } };
    }
    return type === "select" || type === "multiSelect" ? { ok: true, extra: { options: normalizeOptions(spec.options ?? prev.options ?? []) } } : { ok: true, extra: {} };
  }

  function addTable(state, spec = {}) {
    if (!state || !Array.isArray(state.tables)) return fail("유효하지 않은 데이터베이스 상태입니다.");
    if (state.tables.length >= MAX_TABLES) return fail(`테이블 수 상한(${MAX_TABLES}개)을 초과했습니다.`);
    const rawFields = Array.isArray(spec.fields) && spec.fields.length ? spec.fields : defaultFields().map(({ name, type, options }) => ({ name, type, options }));
    if (rawFields.length > MAX_FIELDS_PER_TABLE) return fail(`필드 수 상한(${MAX_FIELDS_PER_TABLE}개)을 초과했습니다.`);
    const seq = state.nextTableSeq || state.tables.length + 1; state.nextTableSeq = seq + 1;
    const fields = rawFields.map((fSpec = {}) => {
      const type = FIELD_TYPE_SET.has(fSpec.type) ? fSpec.type : "text", fSeq = state.nextFieldSeq || 1; state.nextFieldSeq = fSeq + 1;
      const fld = { id: String(fSpec.id || `fld-${fSeq}`), name: String(fSpec.name || `필드 ${fSeq}`).trim().slice(0, 128) || `필드 ${fSeq}`, type, validation: sanitizeValidation(fSpec.validation) };
      if (type === "select" || type === "multiSelect") fld.options = normalizeOptions(fSpec.options);
      else if (type === "relation") fld.relationTableId = String(fSpec.relationTableId || "");
      else if (type === "formula") fld.formula = String(fSpec.formula ?? "");
      return fld;
    });
    const table = {
      id: String(spec.id || `tbl-${seq}`), name: String(spec.name || `테이블 ${seq}`).trim().slice(0, 128) || `테이블 ${seq}`,
      view: VIEW_MODES.has(spec.view) ? spec.view : "grid", kanbanFieldId: String(spec.kanbanFieldId || fields.find((f) => f.type === "select")?.id || ""),
      calendarFieldId: String(spec.calendarFieldId || fields.find((f) => f.type === "date")?.id || ""),
      calendarMonth: MONTH_RE.test(String(spec.calendarMonth || "")) ? String(spec.calendarMonth) : "2026-11",
      filters: [], filterConjunction: "and", sorts: [], groupByFieldId: "", collapsedGroups: [], fields, records: [],
    };
    state.tables.push(table); if (spec.activate !== false) state.activeTableId = table.id;
    return { ok: true, table };
  }

  function addField(state, tableId, spec = {}) {
    const table = getTable(state, tableId);
    if (!table) return fail("대상 테이블을 찾을 수 없습니다.");
    if (table.fields.length >= MAX_FIELDS_PER_TABLE) return fail(`테이블당 최대 필드 수(${MAX_FIELDS_PER_TABLE}개)를 초과했습니다.`);
    const type = String(spec.type || "text"), name = String(spec.name ?? "").trim().slice(0, 128);
    if (!FIELD_TYPE_SET.has(type)) return fail(`지원하지 않는 필드 유형입니다: ${type}`);
    if (!name) return fail("필드 이름을 입력해 주세요.");
    const cfg = normalizeFieldConfig(state, type, spec);
    if (!cfg.ok) return cfg;
    const fSeq = state.nextFieldSeq || table.fields.length + 1; state.nextFieldSeq = fSeq + 1;
    const field = { id: String(spec.id || `fld-${fSeq}`), name, type, validation: sanitizeValidation(spec.validation), ...cfg.extra };
    table.fields.push(field);
    if (type === "select" && !table.kanbanFieldId) table.kanbanFieldId = field.id;
    if (type === "date" && !table.calendarFieldId) table.calendarFieldId = field.id;
    return { ok: true, field };
  }

  function updateField(state, tableId, fieldId, patch = {}) {
    const field = getTable(state, tableId)?.fields.find((f) => f.id === fieldId);
    if (!field) return fail("대상 필드를 찾을 수 없습니다.");
    const nextType = patch.type !== undefined ? String(patch.type) : field.type;
    const nextName = patch.name !== undefined ? String(patch.name).trim().slice(0, 128) : field.name;
    if (!FIELD_TYPE_SET.has(nextType)) return fail(`지원하지 않는 필드 유형입니다: ${nextType}`);
    if (!nextName) return fail("필드 이름을 입력해 주세요.");
    const cfg = normalizeFieldConfig(state, nextType, patch, field);
    if (!cfg.ok) return cfg;
    delete field.relationTableId; delete field.formula; delete field.options;
    Object.assign(field, { name: nextName, type: nextType }, cfg.extra);
    if (patch.validation !== undefined) field.validation = sanitizeValidation(patch.validation);
    return { ok: true, field };
  }

  function deleteField(state, tableId, fieldId) {
    const table = getTable(state, tableId);
    if (!table) return fail("대상 테이블을 찾을 수 없습니다.");
    if (table.fields.length <= 1) return fail("마지막 필드는 삭제할 수 없습니다.");
    const idx = table.fields.findIndex((f) => f.id === fieldId);
    if (idx < 0) return fail("대상 필드를 찾을 수 없습니다.");
    table.fields.splice(idx, 1);
    for (const rec of table.records) delete rec.values[fieldId];
    table.filters = (table.filters || []).filter((f) => f.fieldId !== fieldId);
    table.sorts = (table.sorts || []).filter((s) => s.fieldId !== fieldId);
    if (table.groupByFieldId === fieldId) table.groupByFieldId = "";
    if (table.kanbanFieldId === fieldId) table.kanbanFieldId = table.fields.find((f) => f.type === "select")?.id || "";
    if (table.calendarFieldId === fieldId) table.calendarFieldId = table.fields.find((f) => f.type === "date")?.id || "";
    return { ok: true };
  }

  function validateRecordValues(fields, rawValues, state, idx) {
    const clean = {};
    for (const fld of fields) if (fld.type !== "formula") {
      const def = fld.type === "checkbox" ? false : fld.type === "multiSelect" ? [] : "";
      const res = validateCellValue(fld, rawValues && Object.hasOwn(rawValues, fld.id) ? rawValues[fld.id] : def, state, idx);
      if (!res.ok) return res;
      clean[fld.id] = res.value;
    }
    return { ok: true, values: clean };
  }

  function addRecord(state, tableId, rawValues = {}) {
    const table = getTable(state, tableId);
    if (!table) return fail("대상 테이블을 찾을 수 없습니다.");
    if (table.records.length >= MAX_RECORDS_PER_TABLE) return fail(`테이블당 최대 레코드 수(${MAX_RECORDS_PER_TABLE}건)를 초과했습니다.`);
    const vRes = validateRecordValues(table.fields, rawValues, state, buildStateIndex(state));
    if (!vRes.ok) return vRes;
    const rSeq = state.nextRecordSeq || table.records.length + 1; state.nextRecordSeq = rSeq + 1;
    const record = { id: `rec-${rSeq}`, values: vRes.values };
    table.records.push(record);
    return { ok: true, record };
  }

  function updateCell(state, tableId, recordId, fieldId, rawValue) {
    const table = getTable(state, tableId), field = table?.fields.find((f) => f.id === fieldId), record = table?.records.find((r) => r.id === recordId);
    if (!field || !record) return fail("대상 셀을 찾을 수 없습니다.");
    const res = validateCellValue(field, rawValue, state);
    if (!res.ok) return res;
    record.values[fieldId] = res.value;
    return { ok: true, value: res.value };
  }

  function deleteRecord(state, tableId, recordId) {
    const table = getTable(state, tableId), idx = table ? table.records.findIndex((r) => r.id === recordId) : -1;
    if (idx < 0) return fail("삭제할 레코드를 찾을 수 없습니다.");
    table.records.splice(idx, 1);
    return { ok: true };
  }

  // ---- Export Serialization & State Persistence ----

  const uniqueSheetName = (rawName, usedUpper) => {
    const base = String(rawName || "시트").replaceAll(/[\[\]:*?/\\]/g, "_").trim().slice(0, 31) || "시트";
    for (let i = 1; i < 1000; i += 1) {
      const cand = i === 1 ? base : base.slice(0, Math.max(1, 31 - String(i).length - 1)) + "_" + i;
      if (!usedUpper.has(cand.toUpperCase())) { usedUpper.add(cand.toUpperCase()); return cand; }
    }
    return base;
  };

  const cellExportScalar = (fld, rowData) => {
    const val = rowData.values[fld.id];
    if (fld.type === "checkbox" || typeof val === "boolean") return { raw: val ? "TRUE" : "FALSE", t: "b" };
    return typeof val === "number" && Number.isFinite(val) ? { raw: String(val), t: "n" } : { raw: rowData.displays[fld.id], t: "s" };
  };

  function buildSheetForTable(state, table, usedUpper) {
    const { fields, rows } = queryTableView(state, table.id);
    const cells = fields.map((f, c) => ({ ref: colIndexToLetters(c) + "1", raw: String(f.name || ""), t: "s", style: { bold: true } }));
    for (let r = 0; r < rows.length; r += 1) {
      const rowNum = String(r + 2), rowData = rows[r];
      for (let c = 0; c < fields.length; c += 1) {
        const sc = cellExportScalar(fields[c], rowData);
        if (sc.raw !== "") cells.push({ ref: colIndexToLetters(c) + rowNum, raw: sc.raw, t: sc.t });
      }
    }
    return { name: uniqueSheetName(table.name, usedUpper), cells };
  }

  function toSheetsPayload(state, options = {}) {
    const usedUpper = new Set();
    if (options.tableId && options.single) { const tbl = getTable(state, options.tableId); return tbl ? [buildSheetForTable(state, tbl, usedUpper)] : []; }
    return (state?.tables || []).map((tbl) => buildSheetForTable(state, tbl, usedUpper));
  }

  // OWASP CSV injection: a text value a spreadsheet program could read as a formula gets a leading apostrophe; a number keeps its sign.
  const escapeCsvField = (text, delim, isNumber = false) => {
    const t = String(text ?? ""), s = !isNumber && /^[=+\-@\t\r]/.test(t) ? "'" + t : t;
    return s.includes(delim) || s.includes('"') || s.includes("\r") || s.includes("\n") ? '"' + s.replaceAll('"', '""') + '"' : s;
  };

  function toDelimitedText(state, tableId, delimiter = ",") {
    const delim = delimiter === "\t" ? "\t" : ",", { fields, rows } = queryTableView(state, tableId);
    const lines = [fields.map((f) => escapeCsvField(f.name || "", delim)).join(delim)];
    for (const rowData of rows) lines.push(fields.map((fld) => { const sc = cellExportScalar(fld, rowData); return escapeCsvField(sc.raw, delim, sc.t !== "s"); }).join(delim));
    return "\uFEFF" + lines.join("\r\n");
  }

  const sanitizeFilters = (raw) => (Array.isArray(raw) ? raw.slice(0, MAX_FILTERS).map((x) => ({
    fieldId: String(x?.fieldId || ""), op: FILTER_OP_SET.has(x?.op) ? x.op : "contains", value: String(x?.value ?? "").slice(0, MAX_CELL_CHARS),
  })) : []);
  const sanitizeSorts = (raw) => (Array.isArray(raw) ? raw.slice(0, MAX_SORTS).map((x) => ({
    fieldId: String(x?.fieldId || ""), dir: x?.dir === "desc" ? "desc" : "asc",
  })) : []);

  function cloneField(f, normalizeOpts) {
    const out = { id: String(f.id), name: String(f.name || "").trim().slice(0, 128), type: f.type, validation: sanitizeValidation(f.validation) };
    if (f.type === "select" || f.type === "multiSelect") out.options = normalizeOpts ? normalizeOptions(f.options || []) : [...(f.options || [])];
    if (f.type === "relation") out.relationTableId = String(f.relationTableId || "");
    if (f.type === "formula") out.formula = String(f.formula ?? "");
    return out;
  }

  function cloneTableProps(t, fallbackName, fields, records) {
    return {
      id: String(t.id), name: String(t.name || fallbackName).trim().slice(0, 128) || fallbackName,
      view: VIEW_MODES.has(t.view) ? t.view : "grid",
      kanbanFieldId: String(t.kanbanFieldId || ""), calendarFieldId: String(t.calendarFieldId || ""),
      calendarMonth: MONTH_RE.test(String(t.calendarMonth || "")) ? String(t.calendarMonth) : "2026-11",
      filters: sanitizeFilters(t.filters), filterConjunction: t.filterConjunction === "or" ? "or" : "and",
      sorts: sanitizeSorts(t.sorts), groupByFieldId: String(t.groupByFieldId || ""),
      collapsedGroups: Array.isArray(t.collapsedGroups) ? t.collapsedGroups.map(String) : [], fields, records,
    };
  }

  function serializeBaseState(state) {
    if (!state || typeof state !== "object") return createInitialBaseState();
    return {
      version: 1, activeTableId: String(state.activeTableId || state.tables?.[0]?.id || "tbl-1"),
      nextTableSeq: Number(state.nextTableSeq) || 2, nextFieldSeq: Number(state.nextFieldSeq) || 1, nextRecordSeq: Number(state.nextRecordSeq) || 1,
      tables: (state.tables || []).map((t, i) => cloneTableProps(t, `테이블 ${i + 1}`, (t.fields || []).map((f) => cloneField(f, false)),
        (t.records || []).map((r) => ({ id: String(r.id), values: Object.fromEntries(Object.entries(r.values || {}).map(([k, v]) => [k, Array.isArray(v) ? [...v] : v])) })))),
    };
  }

  function hydrateBaseState(raw) {
    let parsed = raw;
    if (typeof raw === "string") {
      if (raw.length > MAX_JSON_BYTES) return fail("상태 JSON 크기가 상한을 초과했습니다.");
      try { parsed = JSON.parse(raw); } catch { return fail("유효한 JSON 형식이 아닙니다."); }
    }
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed) || hasForbiddenKeysDeep(parsed, 0)) return fail("허용되지 않는 상태 형식입니다.");
    if (!Array.isArray(parsed.tables) || !parsed.tables.length || parsed.tables.length > MAX_TABLES) return fail(`테이블 수는 1~${MAX_TABLES}개 범위여야 합니다.`);

    const tables = [];
    for (let t = 0; t < parsed.tables.length; t += 1) {
      const rt = parsed.tables[t];
      if (!rt || typeof rt !== "object" || !rt.id) return fail("테이블 항목이 올바르지 않습니다.");
      if (!Array.isArray(rt.fields) || !rt.fields.length || rt.fields.length > MAX_FIELDS_PER_TABLE) return fail(`필드 수는 1~${MAX_FIELDS_PER_TABLE}개 범위여야 합니다.`);
      if (!Array.isArray(rt.records) || rt.records.length > MAX_RECORDS_PER_TABLE) return fail(`레코드 수는 최대 ${MAX_RECORDS_PER_TABLE}건까지 허용됩니다.`);
      if (rt.view !== undefined && !VIEW_MODES.has(rt.view)) return fail(`알 수 없는 보기 모드입니다: ${rt.view}`);
      const fields = [];
      for (const rf of rt.fields) {
        if (!rf || !rf.id || !FIELD_TYPE_SET.has(rf.type)) return fail("유효하지 않은 필드 정의입니다.");
        const fld = cloneField(rf, true);
        if (!fld.name) return fail("필드 이름이 비어 있습니다.");
        if (fld.type === "formula") { const c = compileFormula(fld.formula); if (!c.ok && c.depthExceeded) return fail(c.error); }
        fields.push(fld);
      }
      tables.push(cloneTableProps(rt, `테이블 ${t + 1}`, fields,
        rt.records.map((rec, rIdx) => ({ id: String(rec?.id || `rec-${rIdx + 1}`), values: rec?.values && typeof rec.values === "object" ? { ...rec.values } : {} }))));
    }
    const nextState = {
      version: 1, activeTableId: tables.some((t) => t.id === parsed.activeTableId) ? String(parsed.activeTableId) : tables[0].id,
      nextTableSeq: Math.max(tables.length + 1, Number(parsed.nextTableSeq) || tables.length + 1),
      nextFieldSeq: Math.max(1, Number(parsed.nextFieldSeq) || 1), nextRecordSeq: Math.max(1, Number(parsed.nextRecordSeq) || 1), tables,
    };
    const idx = buildStateIndex(nextState);
    for (const tbl of tables) {
      for (const rec of tbl.records) {
        const vRes = validateRecordValues(tbl.fields, rec.values, nextState, idx);
        if (!vRes.ok) return vRes;
        rec.values = vRes.values;
      }
    }
    return { ok: true, state: nextState };
  }

  host.__EASYOFFICE_BASE_MODEL__ = {
    MAX_TABLES, MAX_FIELDS_PER_TABLE, MAX_RECORDS_PER_TABLE, MAX_CELL_CHARS, MAX_FORMULA_DEPTH,
    VIEW_MODES, FIELD_TYPES, FIELD_TYPE_LABELS, FILTER_OPS, FILTER_OP_LABELS,
    createInitialBaseState, buildStateIndex, getTable, validateCellValue, compileFormula, evaluateFormula,
    resolveRelationDisplay, resolveRelationItems, rollupSum, formatCellDisplay, queryTableView,
    addTable, addField, updateField, deleteField, addRecord, updateCell, deleteRecord,
    toSheetsPayload, toDelimitedText, serializeBaseState, hydrateBaseState,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
