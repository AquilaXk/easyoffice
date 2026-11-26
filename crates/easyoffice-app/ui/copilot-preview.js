// 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd., 커밋 3dfa757d, see THIRD_PARTY_NOTICES.md)
// `web/src/features/copilot/turn-changes.tsx:84-141`(`ChangeDetails`의 변경 목록: 항목마다 위치 이름표와 종류 표지(추가·삭제·변경), 필드별 이전·이후 두 칸, "더 보기" 상한)와
// `:309-318`(위치로 가기 단추)를 바닐라 DOM으로 옮겼다. 이전·이후 칸 글에는 udecode/plate (MIT)의 단어 diff(`copilot-diff.js`)로 삭제·삽입 표시를 얹고,
// 표시 색은 문서의 변경 추적 표시(`ins.eo-rev-ins`, `del.eo-rev-del`)의 토큰을 그대로 쓴다(`copilot.css`).
// 원본은 저장된 파일 버전 둘을 비교한 결과를 그리지만 여기서는 모델 연산에서 바뀔 곳을 읽는다. 그 읽기, 문서 위 삭제·삽입 덧씌움, 적용 전 문서 상태 지문은 직접 썼다.
//
// 덧씌움은 편집 DOM 밖(`.doc-scroll-container`)에 그린다. 편집 DOM에 표시를 넣으면 저장·인쇄·찾기·되돌리기 기록이 그 표시를 문서 글로 읽는다.

(function (host) {
  "use strict";

  // 변경 목록은 이만큼까지 그리고 나머지는 개수로만 알린다. 한 칸의 글도 이만큼에서 자른다.
  const MAX_ENTRIES = 40;
  const MAX_ENTRY_CHARS = 600;
  const ELEMENT_LABELS = Object.freeze({ textbox: "글상자", textBox: "글상자", shape: "도형", image: "그림", table: "표", chart: "차트" });

  function createCopilotPreview(deps) {
    const { document, win = host, docBlocksOf, slideDeckState, sheetDataStore, getActiveSheetName, getSheetController, extOps } = deps;
    const { diffSegments } = host.__EASYOFFICE_COPILOT_DIFF__;
    const { clipCodePoints } = host.__EASYOFFICE_COPILOT_TEXT__;
    const docHtml = host.__EASYOFFICE_COPILOT_DOC_HTML__;
    const sheetOps = host.__EASYOFFICE_COPILOT_SHEET_OPS__;
    let overlays = [];
    // 덧씌움이 원래 문단보다 길어질 때 뒤 문단을 미는 규칙들. 편집 DOM의 인라인 스타일이 아니라 따로 둔 스타일 시트에 넣는다.
    let pushRules = [];
    let pushSheet = null;

    function applyPushRules() {
      if (typeof win.CSSStyleSheet !== "function" || !("adoptedStyleSheets" in document)) return;
      if (!pushSheet) {
        pushSheet = new win.CSSStyleSheet();
        document.adoptedStyleSheets = [...document.adoptedStyleSheets, pushSheet];
      }
      pushSheet.replaceSync(pushRules.join("\n"));
    }

    const clip = (text) => clipCodePoints(String(text ?? ""), MAX_ENTRY_CHARS);

    function blockText(item) {
      if (docHtml?.plainPreviewTextOfBlock) return docHtml.plainPreviewTextOfBlock(item);
      if (typeof item === "string") return item;
      if (!item || typeof item !== "object") return "";
      if (Array.isArray(item.items)) {
        return item.items.map((e) => (e && typeof e === "object" ? String(e.text ?? "") : String(e ?? ""))).join("\n");
      }
      return String(item.text ?? item.content ?? "");
    }

    function docItems(op) {
      if (Array.isArray(op.blocks)) return op.blocks;
      if (Array.isArray(op.paragraphs)) return op.paragraphs;
      return op.text !== undefined ? [String(op.text)] : [];
    }

    const editorBlocks = () => docBlocksOf(document.getElementById("doc-editor"));

    function change(kind, label, before, after, location, extra = {}) {
      const segments = kind === "replace" || kind === "insert" ? diffSegments(clip(before), clip(after)) : [];
      return { kind, label, segments, location, ...extra };
    }

    function docEntries(op, entries) {
      const blocks = editorBlocks();
      const kind = String(op.op || op.type || op.kind || "");
      if (kind === "doc.scrub_pii") {
        entries.push(change("mask", "개인정보 가리기", "", "", { module: "DOC", paragraphIndex: 0 }));
        return;
      }
      if (kind === "doc.replace_paragraphs" && Array.isArray(op.replacements)) {
        for (const rep of op.replacements) {
          const index = Number(rep?.index);
          if (!Number.isInteger(index) || index < 0) continue;
          const before = index < blocks.length ? blockText(docHtml ? docHtml.serializeBlockForContext(blocks[index]) : (blocks[index].textContent || "")) : "";
          entries.push(change(index < blocks.length ? "replace" : "insert", "문단 " + (index + 1), before, blockText(rep?.text), { module: "DOC", paragraphIndex: index }, { docIndex: index }));
        }
        return;
      }
      const items = docItems(op);
      if (kind === "doc.replace_paragraphs") {
        const start = Number.isInteger(op.startIndex) ? Math.max(0, op.startIndex) : 0;
        const end = Number.isInteger(op.endIndex) ? Math.min(blocks.length - 1, op.endIndex) : blocks.length - 1;
        const before = blocks.slice(start, end + 1).map((b) => blockText(docHtml ? docHtml.serializeBlockForContext(b) : (b.textContent || ""))).join("\n");
        entries.push(change("replace", "문단 " + (start + 1) + (end > start ? "~" + (end + 1) : ""), before, items.map(blockText).join("\n"), { module: "DOC", paragraphIndex: start }, { docIndex: start, docEnd: end }));
        return;
      }
      entries.push(change("insert", "문서 끝에 추가", "", items.map(blockText).join("\n"), { module: "DOC", paragraphIndex: Math.max(0, blocks.length - 1) }, { docAppend: true }));
    }

    function slideEntries(op, entries) {
      const kind = String(op.op || op.type || op.kind || "");
      const bullets = Array.isArray(op.bullets) ? op.bullets.map((b) => String(b ?? "")).join("\n") : undefined;
      if (kind === "slide.add") {
        entries.push(change("insert", "슬라이드 추가", "", [op.title, bullets ?? op.body ?? op.lower].filter((x) => x).join("\n"), { module: "SLIDE", slideIndex: slideDeckState.length }));
        return;
      }
      if (kind === "slide.add_element") {
        const element = op.element && typeof op.element === "object" ? op.element : op;
        entries.push(change("format", "개체 추가" + (element.kind ? ` (${ELEMENT_LABELS[element.kind] || element.kind})` : ""), "", "", { module: "SLIDE", slideIndex: Number.isInteger(op.slideIndex) ? op.slideIndex : 0 }));
        return;
      }
      const index = Number.isInteger(op.index) ? op.index : Number.isInteger(op.slideIndex) ? op.slideIndex : 0;
      const slide = slideDeckState[index] || {};
      const location = { module: "SLIDE", slideIndex: index };
      const fields = [["제목", op.title, slide.title], ["본문", bullets ?? op.body ?? op.lower, slide.lower], ["노트", op.notes, slide.notes]];
      for (const [name, next, prev] of fields) {
        if (next !== undefined) entries.push(change(prev ? "replace" : "insert", `슬라이드 ${index + 1} ${name}`, prev || "", next, location));
      }
      // 글이 아닌 변경(레이아웃, 화면 전환, 개체 목록 교체)도 목록에 올려 수락 전에 무엇이 바뀌는지 보이게 한다.
      for (const [name, next] of [["레이아웃", op.layout], ["화면 전환", op.transition], ["개체", Array.isArray(op.elements) ? op.elements : undefined]]) {
        if (next !== undefined) entries.push(change("format", `슬라이드 ${index + 1} ${name}`, "", "", location));
      }
    }

    function sheetEntries(op, entries) {
      const kind = String(op.op || op.type || op.kind || "");
      const sheet = String(op.sheet || getActiveSheetName());
      const store = sheetDataStore[sheet] || {};
      if (kind === "sheet.write") {
        for (const cell of Array.isArray(op.cells) ? op.cells : []) {
          const coord = String(cell?.coord || "").toUpperCase();
          const prev = store[coord];
          const before = prev ? String(prev.formula || prev.value || "") : "";
          const after = String(cell?.raw ?? cell?.formula ?? cell?.value ?? "");
          entries.push(change(before ? "replace" : "insert", coord, before, after, { module: "SHEET", sheet, range: coord }));
        }
        return;
      }
      if (kind === "sheet.chart") {
        entries.push(change("format", "차트 " + (op.title || op.dataRange || ""), "", "", { module: "SHEET", sheet, range: String(op.dataRange || "") }));
        return;
      }
      const extEntry = sheetOps?.previewEntryForExtendedSheetOp?.(op, change, getActiveSheetName);
      if (extEntry) {
        entries.push(extEntry);
        return;
      }
      entries.push(change("format", "서식 " + String(op.range || ""), "", "", { module: "SHEET", sheet, range: String(op.range || "") }));
    }

    /** 연산이 바꿀 곳의 목록. 건수와 글자 수는 상한을 둔다. */
    function buildChanges(mod, operations) {
      const entries = [];
      for (const op of Array.isArray(operations) ? operations : []) {
        if (!op || typeof op !== "object") continue;
        if (mod === "DOC") docEntries(op, entries);
        else if (mod === "HWP") {
          if (extOps?.getEmbedHwpHost?.()) extOps.hwpEntries(op, entries, change);
          else docEntries(op, entries);
        }
        else if (mod === "SLIDE") slideEntries(op, entries);
        else if (mod === "SHEET") sheetEntries(op, entries);
        else if (mod === "BASE") extOps?.baseEntries?.(op, entries, change);
        else if (mod === "BOARD") extOps?.boardEntries?.(op, entries, change);
        if (entries.length > MAX_ENTRIES * 8) break;
      }
      return { entries: entries.slice(0, MAX_ENTRIES), omitted: Math.max(0, entries.length - MAX_ENTRIES) };
    }

    /** 연산이 건드리는 곳의 지금 내용. 미리보기를 본 뒤 그 곳이 바뀌었으면 적용하지 않는다. */
    function fingerprint(mod, operations) {
      const ops = Array.isArray(operations) ? operations : [];
      if (mod === "HWP" && extOps?.getEmbedHwpHost?.()) {
        return extOps.hwpFingerprint();
      }
      if (mod === "DOC" || mod === "HWP") {
        const blocks = editorBlocks();
        const touched = new Set();
        for (const op of ops) {
          for (const rep of Array.isArray(op?.replacements) ? op.replacements : []) touched.add(Number(rep?.index));
          if (String(op?.op || op?.type || op?.kind || "") === "doc.replace_paragraphs" && !Array.isArray(op.replacements)) {
            // 범위 교체(범위를 생략하면 문서 전체)는 범위 안 모든 문단이 바뀌지 않았어야 한다.
            const start = Number.isInteger(op.startIndex) ? Math.max(0, op.startIndex) : 0;
            const end = Number.isInteger(op.endIndex) ? Math.min(blocks.length - 1, op.endIndex) : blocks.length - 1;
            for (let i = start; i <= end; i += 1) touched.add(i);
          }
        }
        return JSON.stringify({
          n: blocks.length,
          t: [...touched].filter(Number.isInteger).sort((a, b) => a - b).map((i) => [
            i,
            blocks[i] ? (docHtml ? docHtml.serializeBlockForContext(blocks[i]) : (blocks[i].textContent ?? null)) : null,
          ]),
        });
      }
      if (mod === "SLIDE") {
        return JSON.stringify({ n: slideDeckState.length, t: ops.map((op) => { const s = slideDeckState[Number.isInteger(op?.index) ? op.index : op?.slideIndex ?? 0]; return s ? [s.title, s.lower, s.notes, s.layout, s.transition, s.elements] : null; }) });
      }
      if (mod === "SHEET") {
        if (sheetOps?.computeSheetFingerprint) {
          return sheetOps.computeSheetFingerprint(ops, { document, sheetDataStore, getActiveSheetName, getSheetController });
        }
        return JSON.stringify(ops.map((op) => {
          const store = sheetDataStore[String(op?.sheet || getActiveSheetName())] || {};
          return (Array.isArray(op?.cells) ? op.cells : []).map((c) => { const e = store[String(c?.coord || "").toUpperCase()]; return e ? [e.formula || "", e.value ?? ""] : null; });
        }));
      }
      if (mod === "BASE") return extOps?.baseFingerprint?.() || "";
      if (mod === "BOARD") return extOps?.boardFingerprint?.() || "";
      return "";
    }

    function segmentNodes(segments) {
      return segments.map((segment) => {
        const el = document.createElement(segment.kind === "insert" ? "ins" : segment.kind === "delete" ? "del" : "span");
        if (segment.kind === "insert") el.className = "eo-copilot-ins";
        else if (segment.kind === "delete") el.className = "eo-copilot-del";
        el.textContent = segment.text;
        return el;
      });
    }

    // 옮김[turn-changes] 시작
    const KIND_LABELS = { replace: "변경", insert: "추가", format: "서식", mask: "가림" };

    function side(name, caption, segments, keep) {
      const box = document.createElement("div");
      box.className = "eo-copilot-change-side";
      box.dataset.side = name;
      const title = document.createElement("div");
      title.className = "eo-copilot-change-caption";
      title.textContent = caption;
      const text = document.createElement("div");
      text.className = "eo-copilot-diff";
      for (const node of segmentNodes(segments.filter((s) => s.kind === "equal" || s.kind === keep))) text.appendChild(node);
      box.appendChild(title);
      box.appendChild(text);
      return box;
    }

    /** 패널의 변경 목록 한 항목: 위치 단추와 종류 표지, 이전·이후 칸(삭제·삽입 표시가 얹힌다). */
    function renderEntry(entry, onLocate) {
      const row = document.createElement("section");
      row.className = "eo-copilot-change";
      const head = document.createElement("div");
      head.className = "eo-copilot-change-head";
      const where = document.createElement("button");
      where.type = "button";
      where.className = "eo-copilot-change-label";
      where.textContent = entry.label;
      where.addEventListener("click", () => onLocate?.(entry.location));
      const kind = document.createElement("span");
      kind.className = "eo-copilot-change-kind";
      kind.textContent = KIND_LABELS[entry.kind] || "";
      head.appendChild(where);
      head.appendChild(kind);
      row.appendChild(head);
      if (entry.segments.length > 0) {
        if (entry.kind === "replace") row.appendChild(side("before", "이전", entry.segments, "delete"));
        row.appendChild(side("after", "이후", entry.segments, "insert"));
      }
      return row;
    }

    // 옮김[turn-changes] 끝

    function renderList(changes, onLocate) {
      const list = document.createElement("div");
      list.className = "eo-copilot-changes";
      for (const entry of changes.entries) list.appendChild(renderEntry(entry, onLocate));
      if (changes.omitted > 0) {
        const more = document.createElement("div");
        more.className = "eo-copilot-change-more";
        more.textContent = `외 ${changes.omitted}건`;
        list.appendChild(more);
      }
      return list;
    }

    const toPlain = (r) => ({ left: r.left, top: r.top, width: r.width, height: r.height });

    /** 문서 위에 바뀔 문단의 삭제·삽입 표시를 그린다. 편집 DOM은 건드리지 않는다. */
    function showDocOverlays(changes) {
      clear();
      const editor = document.getElementById("doc-editor");
      const container = editor?.parentElement;
      if (!editor || !container) return;
      const blocks = docBlocksOf(editor);
      const hostRect = toPlain(container.getBoundingClientRect());
      const scrollTop = Number(container.scrollTop) || 0;
      const scrollLeft = Number(container.scrollLeft) || 0;
      for (const entry of changes.entries) {
        if (entry.docIndex === undefined && !entry.docAppend) continue;
        const target = entry.docAppend ? blocks.at(-1) : blocks[entry.docIndex];
        if (!target) continue;
        const rect = toPlain(target.getBoundingClientRect());
        const last = entry.docEnd === undefined ? target : blocks[entry.docEnd] || target;
        const lastRect = toPlain(last.getBoundingClientRect());
        const scale = target.offsetWidth ? rect.width / target.offsetWidth : 1;
        const style = win.getComputedStyle?.(target);
        const box = document.createElement("div");
        box.className = "eo-copilot-diff-block eo-copilot-transient";
        box.setAttribute("aria-hidden", "true");
        const top = entry.docAppend ? lastRect.top + lastRect.height + 8 : rect.top;
        const height = entry.docAppend ? 0 : lastRect.top + lastRect.height - rect.top;
        box.style.setProperty("left", `${rect.left - hostRect.left + scrollLeft}px`);
        box.style.setProperty("top", `${top - hostRect.top + scrollTop}px`);
        box.style.setProperty("width", `${rect.width}px`);
        box.style.setProperty("min-height", `${height}px`);
        if (style) {
          for (const prop of ["font-family", "font-weight", "text-align", "padding-left", "padding-right"]) {
            const value = style.getPropertyValue?.(prop);
            if (value) box.style.setProperty(prop, value);
          }
          const size = Number.parseFloat(style.fontSize);
          if (Number.isFinite(size)) box.style.setProperty("font-size", `${size * scale}px`);
          const line = Number.parseFloat(style.lineHeight);
          box.style.setProperty("line-height", Number.isFinite(line) ? `${line * scale}px` : "1.5");
        }
        for (const node of segmentNodes(entry.segments)) box.appendChild(node);
        container.appendChild(box);
        overlays.push(box);
        // 새 글이 더 길면 덧씌움이 뒤 문단을 가린다: 마지막 문단의 아래 여백을 규칙으로 늘려 자리를 만든다(문서 DOM은 건드리지 않는다).
        const extra = entry.docAppend ? 0 : box.offsetHeight - height;
        if (Number.isFinite(extra) && extra > 1) {
          const bottom = Number.parseFloat(win.getComputedStyle?.(last)?.marginBottom) || 0;
          const nth = Array.prototype.indexOf.call(editor.children, last) + 1;
          if (nth > 0) {
            pushRules.push(`#doc-editor > :nth-child(${nth}) { margin-bottom: ${bottom + extra}px !important; }`);
            applyPushRules();
          }
        }
      }
    }

    function clear() {
      for (const box of overlays) box.remove();
      overlays = [];
      pushRules = [];
      if (pushSheet) {
        document.adoptedStyleSheets = document.adoptedStyleSheets.filter((sheet) => sheet !== pushSheet);
        pushSheet = null;
      }
    }

    return { buildChanges, fingerprint, renderList, showDocOverlays, clear, blockText, MAX_ENTRIES };
  }

  host.__EASYOFFICE_CREATE_COPILOT_PREVIEW__ = createCopilotPreview;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
