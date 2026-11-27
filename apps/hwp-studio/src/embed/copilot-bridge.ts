// EasyOffice extension (copilot-v1): bounded plain-text paragraph range reading and single-snapshot
// batch paragraph replacement/append for the shell Copilot panel, plus the 도구 -> 코파일럿 relay.
// Pure structural interfaces so Node's built-in TypeScript loader can exercise this module directly.

export const COPILOT_MAX_READ_PARAGRAPHS = 200;
export const COPILOT_MAX_READ_BYTES = 64 * 1024;
export const COPILOT_MAX_SELECTION_BYTES = 16 * 1024;
export const COPILOT_MAX_PARAGRAPH_CHARS = 8_000;
export const COPILOT_MAX_BATCH_STEPS = 64;
export const COPILOT_MAX_STEP_PARAGRAPHS = 128;
export const COPILOT_MAX_BATCH_WRITE_BYTES = 128 * 1024;
export const SHELL_COPILOT_TOGGLE_REASON = 'shell_copilot_toggle' as const;

const utf8Encoder = new TextEncoder();

export interface CopilotCursorPosition {
  sectionIndex: number;
  paragraphIndex: number;
  charOffset: number;
  parentParaIndex?: number;
}

export interface CopilotWasmSource {
  readonly pageCount: number;
  readonly documentGeneration: number;
  getSectionCount(): number;
  getParagraphCount(sec: number): number;
  getParagraphLength(sec: number, para: number): number;
  getTextRange(sec: number, para: number, offset: number, count: number): string;
  getControlTextPositions(sec: number, para: number): number[];
  getFieldInfoAt?(pos: CopilotCursorPosition): { inField: boolean };
  replaceText(
    sec: number,
    para: number,
    offset: number,
    length: number,
    newText: string,
  ): { ok: boolean; charOffset?: number; newLength?: number };
  insertText(sec: number, para: number, offset: number, text: string): unknown;
  deleteText(sec: number, para: number, offset: number, count: number): unknown;
  splitParagraph(sec: number, para: number, offset: number): unknown;
  mergeParagraph(sec: number, para: number): unknown;
  beginDeferredPagination?(budget?: number): unknown;
  flushDeferredPagination?(): unknown;
  cancelDeferredPagination?(): unknown;
}

export interface CopilotInputBridge {
  getCursorPosition(): CopilotCursorPosition;
  getSelection(): { start: CopilotCursorPosition; end: CopilotCursorPosition } | null;
  executeOperation(desc: {
    kind: 'snapshot';
    operationType: string;
    operation: (wasm: CopilotWasmSource) => CopilotCursorPosition | null;
    meta?: { refresh?: 'full' };
  }): void;
}

export interface ReadParagraphRangeParams {
  startIndex: number;
  count: number;
}

export interface HwpParagraphEntry {
  index: number;
  section: number;
  paragraph: number;
  text: string;
  hasControls: boolean;
}

export interface HwpSelectionSnapshot {
  hasSelection: boolean;
  text: string;
  startParagraphIndex: number;
  endParagraphIndex: number;
}

export interface ReadParagraphRangeResult {
  documentGeneration: number;
  pageCount: number;
  totalParagraphs: number;
  truncatedByBytes: boolean;
  paragraphs: HwpParagraphEntry[];
  selection: HwpSelectionSnapshot;
}

export type CopilotEditStep =
  | { kind: 'replace_paragraphs'; replacements: Array<{ index: number; text: string }> }
  | { kind: 'replace_span'; index: number; search: string; replace: string }
  | { kind: 'replace_range'; startIndex: number; endIndex: number; texts: string[] }
  | { kind: 'insert_after'; index: number; texts: string[] }
  | { kind: 'append'; texts: string[] };

export interface ApplyCopilotEditsParams {
  expectedDocumentGeneration?: number;
  steps: CopilotEditStep[];
}

export interface ApplyCopilotEditsResult {
  ok: true;
  documentGeneration: number;
  pageCountBefore: number;
  pageCountAfter: number;
  totalParagraphsAfter: number;
  focusTarget: {
    kind: 'body_paragraph';
    section: number;
    paragraph: number;
    charOffset: 0;
    length: number;
    globalIndex: number;
  };
}

interface FlatParagraphCoord {
  index: number;
  section: number;
  paragraph: number;
  length: number;
}

function compareDesc(a: { index: number }, b: { index: number }): number {
  return b.index - a.index;
}

function compareCodeUnits(a: string, b: string): number {
  if (a < b) return -1;
  return a > b ? 1 : 0;
}

function assertOnlyKeys(obj: Record<string, unknown>, allowed: readonly string[], label: string): void {
  const unknown = Object.keys(obj).filter(k => !allowed.includes(k));
  if (unknown.length > 0) {
    unknown.sort(compareCodeUnits);
    throw new Error(`${label} contains unknown field: ${unknown[0]}`);
  }
}

function asNonNegativeSafeInt(value: unknown, label: string): number {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw new Error(`${label} must be a non-negative safe integer`);
  }
  return value as number;
}

function cleanPlainText(raw: string): string {
  // Strip control codes other than \t and \n so control anchors never leak into prompt or replace text.
  let out = '';
  for (let i = 0; i < raw.length; i += 1) {
    const code = raw.charCodeAt(i);
    if (code === 0x09 || code === 0x0a || code >= 0x20) {
      out += raw[i];
    } else if (code === 0x0d) {
      if (raw.charCodeAt(i + 1) !== 0x0a) out += '\n';
    }
  }
  return out;
}

function clipUtf8(raw: string, maxBytes: number): { text: string; truncated: boolean } {
  const bytes = utf8Encoder.encode(raw);
  if (bytes.byteLength <= maxBytes) return { text: raw, truncated: false };
  let acc = 0;
  let end = 0;
  for (const ch of raw) {
    const len = utf8Encoder.encode(ch).byteLength;
    if (acc + len > maxBytes) break;
    acc += len;
    end += ch.length;
  }
  return { text: raw.slice(0, end), truncated: true };
}

function buildFlatParagraphTable(wasm: CopilotWasmSource): {
  coords: FlatParagraphCoord[];
  sectionOffsets: number[];
} {
  const coords: FlatParagraphCoord[] = [];
  const sectionOffsets: number[] = [];
  const secCount = wasm.getSectionCount();
  for (let sec = 0; sec < secCount; sec += 1) {
    sectionOffsets.push(coords.length);
    const paraCount = wasm.getParagraphCount(sec);
    for (let para = 0; para < paraCount; para += 1) {
      coords.push({
        index: coords.length,
        section: sec,
        paragraph: para,
        length: wasm.getParagraphLength(sec, para),
      });
    }
  }
  return { coords, sectionOffsets };
}

function globalIndexOfPos(
  pos: CopilotCursorPosition,
  sectionOffsets: number[],
  totalParagraphs: number,
): number {
  if (totalParagraphs <= 0) return 0;
  const sec = Math.max(0, Math.min(sectionOffsets.length - 1, pos.sectionIndex));
  const localPara = pos.parentParaIndex ?? pos.paragraphIndex;
  const idx = (sectionOffsets[sec] ?? 0) + Math.max(0, localPara);
  return Math.max(0, Math.min(totalParagraphs - 1, idx));
}

function readSelectionSnapshot(
  wasm: CopilotWasmSource,
  input: CopilotInputBridge | null,
  coords: FlatParagraphCoord[],
  sectionOffsets: number[],
): HwpSelectionSnapshot {
  if (!input || coords.length === 0) {
    return { hasSelection: false, text: '', startParagraphIndex: 0, endParagraphIndex: 0 };
  }
  const cursorIdx = globalIndexOfPos(input.getCursorPosition(), sectionOffsets, coords.length);
  const sel = input.getSelection();
  if (!sel) {
    return { hasSelection: false, text: '', startParagraphIndex: cursorIdx, endParagraphIndex: cursorIdx };
  }
  // Sub-control selections (inside table cells/footnotes) carry parentParaIndex; treat only body selections as range text.
  if (sel.start.parentParaIndex !== undefined || sel.end.parentParaIndex !== undefined) {
    return { hasSelection: false, text: '', startParagraphIndex: cursorIdx, endParagraphIndex: cursorIdx };
  }
  let a = sel.start;
  let b = sel.end;
  const idxA = globalIndexOfPos(a, sectionOffsets, coords.length);
  const idxB = globalIndexOfPos(b, sectionOffsets, coords.length);
  if (idxA > idxB || (idxA === idxB && a.charOffset > b.charOffset)) {
    const tmp = a;
    a = b;
    b = tmp;
  }
  const startIdx = Math.min(idxA, idxB);
  const endIdx = Math.max(idxA, idxB);
  if (startIdx === endIdx && a.charOffset === b.charOffset) {
    return { hasSelection: false, text: '', startParagraphIndex: startIdx, endParagraphIndex: endIdx };
  }
  const pieces: string[] = [];
  let usedBytes = 0;
  for (let i = startIdx; i <= endIdx; i += 1) {
    const c = coords[i];
    const from = i === startIdx ? Math.min(c.length, Math.max(0, a.charOffset)) : 0;
    const to = i === endIdx ? Math.min(c.length, Math.max(from, b.charOffset)) : c.length;
    const count = Math.min(COPILOT_MAX_PARAGRAPH_CHARS, Math.max(0, to - from));
    const chunk = count > 0 ? cleanPlainText(wasm.getTextRange(c.section, c.paragraph, from, count)) : '';
    const remain = COPILOT_MAX_SELECTION_BYTES - usedBytes;
    if (remain <= 0) break;
    const clipped = clipUtf8(chunk, remain);
    pieces.push(clipped.text);
    usedBytes += utf8Encoder.encode(clipped.text).byteLength + 1;
    if (clipped.truncated) break;
  }
  const text = pieces.join('\n');
  return {
    hasSelection: text.trim().length > 0,
    text,
    startParagraphIndex: startIdx,
    endParagraphIndex: endIdx,
  };
}

export function parseReadParagraphRangeParams(raw: Record<string, unknown>): ReadParagraphRangeParams {
  assertOnlyKeys(raw, ['startIndex', 'count'], 'readParagraphRange params');
  const startIndex = asNonNegativeSafeInt(raw.startIndex ?? 0, 'startIndex');
  const count = asNonNegativeSafeInt(raw.count ?? COPILOT_MAX_READ_PARAGRAPHS, 'count');
  if (count < 1 || count > COPILOT_MAX_READ_PARAGRAPHS) {
    throw new Error(`count must be in 1..=${COPILOT_MAX_READ_PARAGRAPHS}`);
  }
  return { startIndex, count };
}

export function readParagraphRange(
  wasm: CopilotWasmSource,
  input: CopilotInputBridge | null,
  params: ReadParagraphRangeParams,
): ReadParagraphRangeResult {
  const { coords, sectionOffsets } = buildFlatParagraphTable(wasm);
  const totalParagraphs = coords.length;
  const endExclusive = Math.min(totalParagraphs, params.startIndex + params.count);
  const paragraphs: HwpParagraphEntry[] = [];
  let usedBytes = 0;
  let truncatedByBytes = false;

  for (let idx = params.startIndex; idx < endExclusive; idx += 1) {
    const c = coords[idx];
    const readLen = Math.min(c.length, COPILOT_MAX_PARAGRAPH_CHARS);
    const rawText = readLen > 0 ? wasm.getTextRange(c.section, c.paragraph, 0, readLen) : '';
    const cleaned = cleanPlainText(rawText);
    const hasControls = wasm.getControlTextPositions(c.section, c.paragraph).length > 0;
    const remain = COPILOT_MAX_READ_BYTES - usedBytes;
    if (remain <= 0) {
      truncatedByBytes = true;
      break;
    }
    const clipped = clipUtf8(cleaned, remain);
    usedBytes += utf8Encoder.encode(clipped.text).byteLength;
    paragraphs.push({
      index: idx,
      section: c.section,
      paragraph: c.paragraph,
      text: clipped.text,
      hasControls,
    });
    if (clipped.truncated) {
      truncatedByBytes = true;
      break;
    }
  }

  return {
    documentGeneration: wasm.documentGeneration,
    pageCount: wasm.pageCount,
    totalParagraphs,
    truncatedByBytes,
    paragraphs,
    selection: readSelectionSnapshot(wasm, input, coords, sectionOffsets),
  };
}

function parseTextsList(raw: unknown, label: string): string[] {
  if (!Array.isArray(raw) || raw.length === 0 || raw.length > COPILOT_MAX_STEP_PARAGRAPHS) {
    throw new Error(`${label} must be a non-empty array of at most ${COPILOT_MAX_STEP_PARAGRAPHS} strings`);
  }
  return raw.map((item, idx) => {
    if (typeof item !== 'string') throw new Error(`${label}[${idx}] must be a string`);
    const cleaned = cleanPlainText(item);
    if (cleaned.length > COPILOT_MAX_PARAGRAPH_CHARS) {
      throw new Error(`${label}[${idx}] exceeds ${COPILOT_MAX_PARAGRAPH_CHARS} characters`);
    }
    return cleaned;
  });
}

export function parseApplyCopilotEditsParams(raw: Record<string, unknown>): ApplyCopilotEditsParams {
  assertOnlyKeys(raw, ['expectedDocumentGeneration', 'steps'], 'applyCopilotEdits params');
  let expectedDocumentGeneration: number | undefined;
  if (raw.expectedDocumentGeneration !== undefined) {
    expectedDocumentGeneration = asNonNegativeSafeInt(raw.expectedDocumentGeneration, 'expectedDocumentGeneration');
  }
  if (!Array.isArray(raw.steps) || raw.steps.length === 0 || raw.steps.length > COPILOT_MAX_BATCH_STEPS) {
    throw new Error(`steps must be an array of 1..=${COPILOT_MAX_BATCH_STEPS} steps`);
  }
  let totalBytes = 0;
  const chargeText = (t: string): void => {
    totalBytes += utf8Encoder.encode(t).byteLength;
    if (totalBytes > COPILOT_MAX_BATCH_WRITE_BYTES) {
      throw new Error(`batch write text exceeds ${COPILOT_MAX_BATCH_WRITE_BYTES} bytes`);
    }
  };

  const steps: CopilotEditStep[] = raw.steps.map((entry, stepIdx) => {
    if (typeof entry !== 'object' || entry === null || Array.isArray(entry)) {
      throw new Error(`steps[${stepIdx}] must be an object`);
    }
    const obj = entry as Record<string, unknown>;
    const kind = obj.kind;
    if (kind === 'replace_paragraphs') {
      assertOnlyKeys(obj, ['kind', 'replacements'], `steps[${stepIdx}]`);
      if (!Array.isArray(obj.replacements) || obj.replacements.length === 0 || obj.replacements.length > COPILOT_MAX_STEP_PARAGRAPHS) {
        throw new Error(`steps[${stepIdx}].replacements must have 1..=${COPILOT_MAX_STEP_PARAGRAPHS} items`);
      }
      const seen = new Set<number>();
      const replacements = obj.replacements.map((rep, rIdx) => {
        if (typeof rep !== 'object' || rep === null || Array.isArray(rep)) {
          throw new Error(`replacements[${rIdx}] must be an object`);
        }
        const rObj = rep as Record<string, unknown>;
        assertOnlyKeys(rObj, ['index', 'text'], `replacements[${rIdx}]`);
        const index = asNonNegativeSafeInt(rObj.index, `replacements[${rIdx}].index`);
        if (seen.has(index)) {
          throw new Error(`duplicate replacement paragraph index: ${index}`);
        }
        seen.add(index);
        if (typeof rObj.text !== 'string') throw new Error(`replacements[${rIdx}].text must be a string`);
        const text = cleanPlainText(rObj.text);
        if (text.length > COPILOT_MAX_PARAGRAPH_CHARS) {
          throw new Error(`replacements[${rIdx}].text exceeds ${COPILOT_MAX_PARAGRAPH_CHARS} characters`);
        }
        chargeText(text);
        return { index, text };
      });
      return { kind: 'replace_paragraphs', replacements };
    }
    if (kind === 'replace_span') {
      assertOnlyKeys(obj, ['kind', 'index', 'search', 'replace'], `steps[${stepIdx}]`);
      const index = asNonNegativeSafeInt(obj.index, `steps[${stepIdx}].index`);
      if (typeof obj.search !== 'string' || typeof obj.replace !== 'string') {
        throw new Error(`steps[${stepIdx}] search/replace must be strings`);
      }
      const search = cleanPlainText(obj.search);
      const replace = cleanPlainText(obj.replace);
      if (!search.trim()) {
        throw new Error(`steps[${stepIdx}].search must not be empty`);
      }
      if (search.length > COPILOT_MAX_PARAGRAPH_CHARS || replace.length > COPILOT_MAX_PARAGRAPH_CHARS) {
        throw new Error(`steps[${stepIdx}] span text exceeds ${COPILOT_MAX_PARAGRAPH_CHARS} characters`);
      }
      chargeText(replace);
      return { kind: 'replace_span', index, search, replace };
    }
    if (kind === 'replace_range') {
      assertOnlyKeys(obj, ['kind', 'startIndex', 'endIndex', 'texts'], `steps[${stepIdx}]`);
      const startIndex = asNonNegativeSafeInt(obj.startIndex, 'startIndex');
      const endIndex = asNonNegativeSafeInt(obj.endIndex, 'endIndex');
      if (endIndex < startIndex || endIndex - startIndex + 1 > COPILOT_MAX_STEP_PARAGRAPHS) {
        throw new Error('replace_range span is invalid');
      }
      const texts = parseTextsList(obj.texts, `steps[${stepIdx}].texts`);
      for (const t of texts) chargeText(t);
      return { kind: 'replace_range', startIndex, endIndex, texts };
    }
    if (kind === 'insert_after') {
      assertOnlyKeys(obj, ['kind', 'index', 'texts'], `steps[${stepIdx}]`);
      const index = asNonNegativeSafeInt(obj.index, `steps[${stepIdx}].index`);
      const texts = parseTextsList(obj.texts, `steps[${stepIdx}].texts`);
      for (const t of texts) chargeText(t);
      return { kind: 'insert_after', index, texts };
    }
    if (kind === 'append') {
      assertOnlyKeys(obj, ['kind', 'texts'], `steps[${stepIdx}]`);
      const texts = parseTextsList(obj.texts, `steps[${stepIdx}].texts`);
      for (const t of texts) chargeText(t);
      return { kind: 'append', texts };
    }
    throw new Error(`Unknown copilot edit step kind: ${String(kind)}`);
  });

  return { expectedDocumentGeneration, steps };
}

function assertEditableParagraph(wasm: CopilotWasmSource, sec: number, para: number, len: number): void {
  if (wasm.getControlTextPositions(sec, para).length > 0) {
    throw new Error('컨트롤 또는 표·그림이 포함된 문단은 코파일럿으로 덮어쓸 수 없습니다.');
  }
  if (typeof wasm.getFieldInfoAt === 'function') {
    const checkCount = Math.min(len, COPILOT_MAX_PARAGRAPH_CHARS);
    for (let offset = 0; offset <= checkCount; offset += 1) {
      if (wasm.getFieldInfoAt({ sectionIndex: sec, paragraphIndex: para, charOffset: offset }).inField) {
        throw new Error('누름틀·필드가 포함된 문단은 코파일럿으로 덮어쓸 수 없습니다.');
      }
    }
  }
}

function replaceSingleParagraphWithLines(
  wasm: CopilotWasmSource,
  sec: number,
  para: number,
  rawText: string,
): { paragraph: number; charOffset: number } {
  const lines = rawText.split('\n');
  const curLen = wasm.getParagraphLength(sec, para);
  assertEditableParagraph(wasm, sec, para, curLen);
  const firstLine = lines[0] ?? '';
  const replaced = wasm.replaceText(sec, para, 0, curLen, firstLine);
  if (!replaced.ok) {
    throw new Error('문단 텍스트 교체에 실패했습니다.');
  }
  let curPara = para;
  let curOffset = wasm.getParagraphLength(sec, curPara);
  for (let i = 1; i < lines.length; i += 1) {
    wasm.splitParagraph(sec, curPara, curOffset);
    curPara += 1;
    const line = lines[i] ?? '';
    if (line.length > 0) {
      wasm.insertText(sec, curPara, 0, line);
    }
    curOffset = wasm.getParagraphLength(sec, curPara);
  }
  return { paragraph: curPara, charOffset: curOffset };
}

// 글 목록을 줄 단위 문단 목록으로 편다. 줄 수가 상한을 넘으면 `overflowMessage`로 거부한다.
function splitTextLines(rawTexts: string[], overflowMessage: string): string[] {
  const lines: string[] = [];
  for (const t of rawTexts) {
    for (const piece of t.split('\n')) {
      lines.push(piece);
      if (lines.length > COPILOT_MAX_STEP_PARAGRAPHS * 2) {
        throw new Error(overflowMessage);
      }
    }
  }
  return lines;
}

function applyRangeReplaceStep(
  wasm: CopilotWasmSource,
  startIndex: number,
  endIndex: number,
  rawTexts: string[],
): CopilotCursorPosition {
  const { coords } = buildFlatParagraphTable(wasm);
  if (startIndex >= coords.length || endIndex >= coords.length) {
    throw new Error('교체할 문단 범위가 문서 범위를 벗어났습니다.');
  }
  const first = coords[startIndex];
  const last = coords[endIndex];
  if (first.section !== last.section) {
    throw new Error('구역 경계를 넘는 문단 범위는 한 번에 교체할 수 없습니다.');
  }
  const sec = first.section;
  const p0 = first.paragraph;
  const oldCount = endIndex - startIndex + 1;
  for (let k = 0; k < oldCount; k += 1) {
    const pLen = wasm.getParagraphLength(sec, p0 + k);
    assertEditableParagraph(wasm, sec, p0 + k, pLen);
  }

  const lines = splitTextLines(rawTexts, '교체 문단 줄 수가 허용 상한을 초과했습니다.');
  if (lines.length === 0) lines.push('');

  const common = Math.min(oldCount, lines.length);
  for (let k = 0; k < common; k += 1) {
    const pLen = wasm.getParagraphLength(sec, p0 + k);
    const res = wasm.replaceText(sec, p0 + k, 0, pLen, lines[k]);
    if (!res.ok) throw new Error('문단 범위 교체에 실패했습니다.');
  }

  if (lines.length > oldCount) {
    let curPara = p0 + oldCount - 1;
    let curLen = wasm.getParagraphLength(sec, curPara);
    for (let k = oldCount; k < lines.length; k += 1) {
      wasm.splitParagraph(sec, curPara, curLen);
      curPara += 1;
      if (lines[k].length > 0) {
        wasm.insertText(sec, curPara, 0, lines[k]);
      }
      curLen = wasm.getParagraphLength(sec, curPara);
    }
    return { sectionIndex: sec, paragraphIndex: curPara, charOffset: curLen };
  }

  if (lines.length < oldCount) {
    const removeStart = p0 + lines.length;
    const toRemove = oldCount - lines.length;
    for (let r = 0; r < toRemove; r += 1) {
      const remLen = wasm.getParagraphLength(sec, removeStart);
      if (remLen > 0) {
        wasm.deleteText(sec, removeStart, 0, remLen);
      }
      wasm.mergeParagraph(sec, removeStart);
    }
  }
  const endPara = p0 + lines.length - 1;
  return {
    sectionIndex: sec,
    paragraphIndex: endPara,
    charOffset: wasm.getParagraphLength(sec, endPara),
  };
}

// 아래에 삽입: 문단 `index`의 글과 서식은 그대로 두고, 그 바로 아래에 줄마다 새 문단을 넣는다.
function applyInsertAfterStep(wasm: CopilotWasmSource, index: number, rawTexts: string[]): CopilotCursorPosition {
  const { coords } = buildFlatParagraphTable(wasm);
  if (index >= coords.length) {
    throw new Error(`문단 번호(${index + 1})가 문서 범위를 벗어났습니다.`);
  }
  const lines = splitTextLines(rawTexts, '삽입 문단 줄 수가 허용 상한을 초과했습니다.');
  const sec = coords[index].section;
  let curPara = coords[index].paragraph;
  let curLen = wasm.getParagraphLength(sec, curPara);
  for (const line of lines) {
    wasm.splitParagraph(sec, curPara, curLen);
    curPara += 1;
    if (line.length > 0) {
      wasm.insertText(sec, curPara, 0, line);
    }
    curLen = wasm.getParagraphLength(sec, curPara);
  }
  return { sectionIndex: sec, paragraphIndex: curPara, charOffset: curLen };
}

function applyAppendStep(wasm: CopilotWasmSource, rawTexts: string[]): CopilotCursorPosition {
  const secCount = wasm.getSectionCount();
  if (secCount <= 0) throw new Error('문서가 로드되지 않았습니다.');
  const lines = splitTextLines(rawTexts, '추가 문단 줄 수가 허용 상한을 초과했습니다.');
  if (lines.length === 0) return { sectionIndex: 0, paragraphIndex: 0, charOffset: 0 };

  const lastSec = secCount - 1;
  let curPara = Math.max(0, wasm.getParagraphCount(lastSec) - 1);
  let curLen = wasm.getParagraphLength(lastSec, curPara);
  const singleBlankDoc =
    secCount === 1
    && wasm.getParagraphCount(0) === 1
    && curLen === 0
    && wasm.getControlTextPositions(0, 0).length === 0;

  let startIdx = 0;
  if (singleBlankDoc) {
    if (lines[0].length > 0) {
      wasm.insertText(0, 0, 0, lines[0]);
    }
    curLen = wasm.getParagraphLength(0, 0);
    startIdx = 1;
  }

  for (let i = startIdx; i < lines.length; i += 1) {
    wasm.splitParagraph(lastSec, curPara, curLen);
    curPara += 1;
    if (lines[i].length > 0) {
      wasm.insertText(lastSec, curPara, 0, lines[i]);
    }
    curLen = wasm.getParagraphLength(lastSec, curPara);
  }
  return { sectionIndex: lastSec, paragraphIndex: curPara, charOffset: curLen };
}

export function applyCopilotEdits(
  wasm: CopilotWasmSource,
  input: CopilotInputBridge | null,
  params: ApplyCopilotEditsParams,
): ApplyCopilotEditsResult {
  if (!input) {
    throw new Error('편집기가 아직 준비되지 않았습니다.');
  }
  if (
    params.expectedDocumentGeneration !== undefined
    && wasm.documentGeneration !== params.expectedDocumentGeneration
  ) {
    throw new Error('미리보기 생성 이후 문서가 변경되어 적용을 중단했습니다.');
  }
  const pageCountBefore = wasm.pageCount;
  let finalPos: CopilotCursorPosition = input.getCursorPosition();

  input.executeOperation({
    kind: 'snapshot',
    operationType: 'copilot_batch_edit',
    meta: { refresh: 'full' },
    operation: (targetWasm) => {
      let deferred = false;
      if (typeof targetWasm.beginDeferredPagination === 'function') {
        targetWasm.beginDeferredPagination(1);
        deferred = true;
      }
      try {
        let pos: CopilotCursorPosition = input.getCursorPosition();
        for (const step of params.steps) {
          if (step.kind === 'replace_paragraphs') {
            const { coords } = buildFlatParagraphTable(targetWasm);
            const ordered = step.replacements.slice().sort(compareDesc);
            for (const item of ordered) {
              if (item.index >= coords.length) {
                throw new Error(`문단 번호(${item.index + 1})가 문서 범위를 벗어났습니다.`);
              }
              const target = coords[item.index];
              const res = replaceSingleParagraphWithLines(
                targetWasm,
                target.section,
                target.paragraph,
                item.text,
              );
              pos = { sectionIndex: target.section, paragraphIndex: res.paragraph, charOffset: res.charOffset };
            }
          } else if (step.kind === 'replace_span') {
            const { coords } = buildFlatParagraphTable(targetWasm);
            if (step.index >= coords.length) {
              throw new Error(`문단 번호(${step.index + 1})가 문서 범위를 벗어났습니다.`);
            }
            const target = coords[step.index];
            const curLen = target.length;
            const curText = curLen > 0
              ? cleanPlainText(targetWasm.getTextRange(target.section, target.paragraph, 0, curLen))
              : '';
            const firstAt = curText.indexOf(step.search);
            if (firstAt < 0) {
              throw new Error('변경할 대상 구간을 문단에서 찾을 수 없습니다.');
            }
            if (curText.indexOf(step.search, firstAt + 1) >= 0) {
              throw new Error('동일한 문구가 여러 번 나타나 바꿀 위치를 특정할 수 없습니다.');
            }
            const nextText = curText.slice(0, firstAt) + step.replace + curText.slice(firstAt + step.search.length);
            if (nextText.length > COPILOT_MAX_PARAGRAPH_CHARS) {
              throw new Error(`치환 후 문단 길이가 ${COPILOT_MAX_PARAGRAPH_CHARS}자를 초과했습니다.`);
            }
            const res = replaceSingleParagraphWithLines(targetWasm, target.section, target.paragraph, nextText);
            pos = { sectionIndex: target.section, paragraphIndex: res.paragraph, charOffset: res.charOffset };
          } else if (step.kind === 'replace_range') {
            pos = applyRangeReplaceStep(targetWasm, step.startIndex, step.endIndex, step.texts);
          } else if (step.kind === 'insert_after') {
            pos = applyInsertAfterStep(targetWasm, step.index, step.texts);
          } else if (step.kind === 'append') {
            pos = applyAppendStep(targetWasm, step.texts);
          }
        }
        if (deferred && typeof targetWasm.flushDeferredPagination === 'function') {
          targetWasm.flushDeferredPagination();
          deferred = false;
        }
        finalPos = pos;
        return pos;
      } catch (err) {
        if (deferred && typeof targetWasm.cancelDeferredPagination === 'function') {
          try { targetWasm.cancelDeferredPagination(); } catch { /* snapshot rollback restores wasm state */ }
        }
        throw err;
      }
    },
  });

  const afterTable = buildFlatParagraphTable(wasm);
  const globalIndex = globalIndexOfPos(finalPos, afterTable.sectionOffsets, afterTable.coords.length);
  const coord = afterTable.coords[globalIndex] ?? {
    index: 0,
    section: 0,
    paragraph: 0,
    length: wasm.getSectionCount() > 0 && wasm.getParagraphCount(0) > 0 ? wasm.getParagraphLength(0, 0) : 0,
  };

  return {
    ok: true,
    documentGeneration: wasm.documentGeneration,
    pageCountBefore,
    pageCountAfter: wasm.pageCount,
    totalParagraphsAfter: afterTable.coords.length,
    focusTarget: {
      kind: 'body_paragraph',
      section: coord.section,
      paragraph: coord.paragraph,
      charOffset: 0,
      length: coord.length,
      globalIndex: coord.index,
    },
  };
}

export interface CopilotToggleRelay {
  add(listener: (event: { schemaVersion: 1; reason: typeof SHELL_COPILOT_TOGGLE_REASON }) => void): () => void;
  toggle(): void;
}

export function createCopilotToggleRelay(): CopilotToggleRelay {
  const listeners = new Set<(event: { schemaVersion: 1; reason: typeof SHELL_COPILOT_TOGGLE_REASON }) => void>();
  return {
    add(listener) {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
    toggle() {
      const payload = { schemaVersion: 1 as const, reason: SHELL_COPILOT_TOGGLE_REASON };
      for (const listener of listeners) listener(payload);
    },
  };
}
