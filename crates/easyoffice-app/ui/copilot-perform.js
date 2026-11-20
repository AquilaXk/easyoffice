// 참조 구현: 같은 폴더의 `copilot-cursor.js`(excalidraw 협업자 포인터와 y-prosemirror 캐럿·선택 강조)와 `copilot-typing.js`(typed.js 타이핑 루프)를 문서·슬라이드·시트에 엮는 연출이다.
// 이 파일은 직접 썼다: 둘을 엮는 원본(포인터가 선택하고 글을 쳐 넣는 문서 편집)이 따로 없어서 찾아본 저장소는 excalidraw(`packages/excalidraw/clients.ts`),
// y-prosemirror(`src/cursor-plugin.js`), typed.js(`src/typed.js`)다(보고에 적었다).
//
// 연출은 눈에 보이는 임시 입력일 뿐이다. 글자는 편집 DOM에 한 자씩 써 넣지만 되돌리기 기록은 만들지 않고(글자마다 스냅숏을 뜨면 한 턴이 한 단계라는 규칙이 깨진다),
// 끝나거나 멈추면 임시로 쓴 것을 원래대로 돌린 다음 한 번만 기존 적용 경로(`applyOperations`)로 쓴다. 멈추면 쓰던 부분까지만 연산으로 잘라 적용한다.

(function (host) {
  "use strict";

  // 한 연산 안에서 글자를 한 자씩 치는 셀은 이만큼까지이고 나머지는 한꺼번에 적용한다.
  const MAX_ANIMATED_CELLS = 40;
  const MAX_ANIMATED_TARGETS = 60;
  const DRAG_MS = 380;
  // 셀 하나를 채우는 시간 상한(ms)과 글자 간격.
  const CELL_TYPE_MS = 500;
  const CELL_TYPE_SPEED = 12;
  const opKind = (op) => String(op?.op || op?.type || op?.kind || "").trim();

  function createCopilotPerformer(deps) {
    const { document, win = host, docBlocksOf, getWorkspacePanels, slideDeckState, getActiveSlideIndex, getActiveSheetName, getSheetController, isReducedMotion, createCursor, createTypist } = deps;
    const { blockText } = deps;

    // ---- 목표: 연산에서 한 자씩 칠 글을 순서대로 뽑는다. `cut(clone, typed)`가 쓰던 곳까지로 연산 복사본을 자른다.
    function collectTargets(mod, operations) {
      const targets = [];
      operations.forEach((op, opIndex) => {
        const kind = opKind(op);
        if (mod === "DOC") {
          if (kind === "doc.replace_paragraphs" && Array.isArray(op.replacements)) {
            op.replacements.forEach((rep, entry) => {
              const index = Number(rep?.index);
              if (Number.isInteger(index) && index >= 0) targets.push({ opIndex, kind: "doc-replace", entry, index, text: String(rep.text ?? "") });
            });
          } else if (kind === "doc.append") {
            const key = Array.isArray(op.blocks) ? "blocks" : Array.isArray(op.paragraphs) ? "paragraphs" : null;
            (key ? op[key] : []).forEach((item, entry) => targets.push({ opIndex, kind: "doc-append", key, entry, text: blockText(item), item }));
          }
        } else if (mod === "SLIDE" && kind === "slide.update") {
          const bulletKey = Array.isArray(op.bullets) ? "bullets" : op.body !== undefined ? "body" : op.lower !== undefined ? "lower" : null;
          const index = Number.isInteger(op.index) ? op.index : Number.isInteger(op.slideIndex) ? op.slideIndex : getActiveSlideIndex();
          if (op.title !== undefined) targets.push({ opIndex, kind: "slide-title", index, text: String(op.title) });
          if (bulletKey) targets.push({ opIndex, kind: "slide-body", index, key: bulletKey, text: bulletKey === "bullets" ? op.bullets.map((b) => String(b ?? "")).join("\n") : String(op[bulletKey] ?? "") });
        } else if (mod === "SHEET" && kind === "sheet.write" && Array.isArray(op.cells)) {
          op.cells.slice(0, MAX_ANIMATED_CELLS).forEach((cell, entry) => {
            const coord = String(cell?.coord || "").trim().toUpperCase();
            if (coord) targets.push({ opIndex, kind: "sheet-cell", entry, coord, sheet: String(op.sheet || getActiveSheetName()), text: String(cell.raw ?? cell.formula ?? cell.value ?? "") });
          });
        }
      });
      return targets.slice(0, MAX_ANIMATED_TARGETS);
    }

    // 셀 값으로 읽히는 글(수식, 숫자, 백분율, 통화, 괄호 음수, 날짜, 시간, 참거짓)은 쓰다 만 것이 다른 값이 되므로 통째로 쓰이거나 아예 쓰이지 않는다.
    // 시트의 기존 판정(`inferTag`, `normalizeFormattedNumericCell`)을 먼저 쓰고, 그것이 모르는 날짜·시간·괄호 음수는 아래 모양으로 본다.
    const VALUE_LIKE = /^[(+-]?\p{Sc}?\s*\d[\d,.\s]*\)?\s*%?$|^\d{1,4}[-/.]\d{1,2}[-/.]\d{1,4}(?:[ T]\d{1,2}:\d{2}(?::\d{2})?)?$|^\d{1,2}:\d{2}(?::\d{2})?(?:\s?[ap]m)?$|^(?:true|false)$/iu;
    function isAtomicText(text) {
      const trimmed = text.trim();
      if (trimmed.startsWith("=")) return true;
      const tag = host.__EASYOFFICE_SHEET_CELL_TYPE__?.inferTag?.(text);
      if (tag !== undefined && tag !== "s") return true;
      const normalized = host.__EASYOFFICE_SHEET_UTILS__?.normalizeFormattedNumericCell?.(text);
      if (normalized !== undefined && normalized !== trimmed) return true;
      return VALUE_LIKE.test(trimmed);
    }

    /** `stop`이 가리키는 곳까지만 남긴 연산 복사본. `stop`이 없으면 전부 남긴다. */
    function cutOperations(operations, targets, stop) {
      const clone = JSON.parse(JSON.stringify(operations));
      if (!stop) return clone;
      const { target, typed } = stop;
      const prefix = target.text.slice(0, typed);
      const out = [];
      clone.forEach((op, opIndex) => {
        if (opIndex < target.opIndex) return out.push(op);
        if (opIndex > target.opIndex) return;
        const keepFirst = (arr, partial) => arr.slice(0, target.entry).concat(partial ? [partial] : []);
        if (target.kind === "doc-replace") {
          op.replacements = keepFirst(op.replacements, prefix ? { ...op.replacements[target.entry], text: prefix } : null);
        } else if (target.kind === "doc-append") {
          const item = op[target.key][target.entry];
          const partial = !prefix ? null : typeof item === "string" ? prefix : Array.isArray(item?.items) ? { ...item, items: prefix.split("\n") } : { ...item, text: prefix };
          op[target.key] = keepFirst(op[target.key], partial);
        } else if (target.kind === "sheet-cell") {
          op.cells = keepFirst(op.cells, prefix && !isAtomicText(target.text) ? { ...op.cells[target.entry], raw: prefix, formula: undefined, value: undefined } : null);
        } else if (target.kind === "slide-title") {
          if (prefix) op.title = prefix;
          else delete op.title;
          for (const key of ["bullets", "body", "lower", "notes", "layout", "transition", "elements"]) delete op[key];
        } else if (target.kind === "slide-body") {
          if (!prefix) delete op[target.key];
          else op[target.key] = target.key === "bullets" ? prefix.split("\n") : prefix;
          for (const key of ["notes", "layout", "transition", "elements"]) delete op[key];
        }
        // 쓴 것이 하나도 없는 연산은 뺀다(빈 목록이나 빈 슬라이드 변경은 엔진 쪽 검사에서 걸린다).
        const lists = ["replacements", "blocks", "paragraphs", "cells"].filter((key) => Array.isArray(op[key]));
        const textless = target.kind.startsWith("slide") && ["title", "bullets", "body", "lower"].every((key) => op[key] === undefined);
        if (!textless && !(lists.length > 0 && lists.every((key) => op[key].length === 0))) out.push(op);
      });
      return out;
    }

    // ---- 화면 계산: 글자 위치의 사각형들(뷰포트 좌표). 레이아웃이 없으면(시험) 요소 사각형으로 대신한다.
    const plain = (r) => ({ left: r.left, top: r.top, width: r.width, height: r.height, right: r.right ?? r.left + r.width, bottom: r.bottom ?? r.top + r.height });
    const elementRect = (el) => plain(el.getBoundingClientRect());

    function textNodeOf(el) {
      for (const child of el.childNodes || []) if (child.nodeType === 3) return child;
      return null;
    }

    function rangeRects(node, from, to) {
      if (!node || !document.createRange) return [];
      try {
        const range = document.createRange();
        range.setStart(node, from);
        range.setEnd(node, to);
        return Array.from(range.getClientRects?.() || []).map(plain);
      } catch {
        return [];
      }
    }

    /** 글자 사이 캐럿이 설 자리: 마지막 글자의 오른쪽 가장자리. 글이 없으면 요소의 왼쪽 위. */
    function caretAt(el) {
      const node = textNodeOf(el);
      const length = node?.data?.length || 0;
      if (length > 0) {
        const rects = rangeRects(node, length - 1, length);
        const last = rects.at(-1);
        if (last && last.height > 0) return { left: last.right, top: last.top, height: last.height };
      }
      const rect = elementRect(el);
      return { left: rect.left, top: rect.top, height: Math.min(rect.height || 18, 22) };
    }

    // ---- 임시로 쓴 것의 기록과 되돌리기
    const touched = [];
    const remember = (el) => {
      if (!touched.some((t) => t.el === el)) touched.push({ el, nodes: Array.from(el.childNodes).map((n) => n.cloneNode(true)) });
    };
    const createdEls = [];

    function revertCosmetics() {
      for (const el of createdEls.splice(0)) el.remove();
      for (const { el, nodes } of touched.splice(0)) el.replaceChildren(...nodes);
    }

    function perform(mod, operations) {
      const targets = collectTargets(mod, operations);
      const reduced = Boolean(isReducedMotion?.());
      if (reduced || targets.length === 0) {
        return { done: Promise.resolve({ operations: cutOperations(operations, targets, null), completed: true, discarded: false }), stop() {}, discard() {}, active: false };
      }
      const cursor = createCursor();
      let stopped = false;
      let discarded = false;
      let typing = null;

      const onHands = (event) => {
        if (event.target?.closest?.("#eo-copilot-shell")) return;
        if (event.type === "keydown" && ["Shift", "Control", "Alt", "Meta"].includes(event.key)) return;
        stop();
      };
      const handEvents = ["pointerdown", "mousedown", "keydown"];
      for (const type of handEvents) document.addEventListener(type, onHands, true);

      function stop() {
        if (stopped) return;
        stopped = true;
        typing?.stop();
        cursor.interrupt();
      }

      // 쓰던 부분도 적용하지 않고 버린다(새 대화).
      function discard() {
        discarded = true;
        stop();
      }

      async function glideTo(rect) {
        if (stopped) return;
        await cursor.moveTo(rect.left, rect.top + Math.min(rect.height || 16, 20) / 2);
      }

      async function drag(el, oldLength) {
        const node = textNodeOf(el);
        if (!node || oldLength <= 0 || stopped) return;
        cursor.press(true);
        await cursor.animate(DRAG_MS, (t) => {
          const end = Math.max(1, Math.round(oldLength * t));
          const rects = rangeRects(node, 0, end);
          cursor.setSelection(rects.length > 0 ? rects : [elementRect(el)]);
          const last = rects.at(-1);
          if (last) cursor.place(last.right, last.top + last.height / 2);
        });
        cursor.press(false);
      }

      async function typeInto(el, text, setText, { speed, totalMs } = {}) {
        typing = createTypist({ onBlink: (idle) => cursor.setIdle(idle), ...(speed ? { typeSpeed: speed, maxTotalMs: totalMs, sentencePauseMs: 40 } : {}) });
        // 타이머 안에서 던진 오류는 약속이 받지 못한다: 붙잡아 입력을 끝내고 다 기다린 뒤 다시 던진다.
        let failure = null;
        const result = await typing.type(text, (prefix) => {
          try {
            setText(prefix);
            const caret = caretAt(el);
            cursor.setCaret(caret);
            cursor.place(caret.left + 2, caret.top + caret.height / 2);
          } catch (error) {
            failure = error;
            typing.stop();
          }
        });
        typing = null;
        if (failure) throw failure;
        return result;
      }

      async function docTarget(t) {
        const editor = document.getElementById("doc-editor");
        let el;
        let oldLength = 0;
        if (t.kind === "doc-replace") {
          el = docBlocksOf(editor)[t.index];
          if (el) {
            remember(el);
            oldLength = textNodeOf(el)?.data?.length || 0;
          }
        }
        if (!el) {
          el = t.kind === "doc-append" && t.item && typeof t.item === "object" && Number.isInteger(t.item.heading) ? document.createElement("h" + Math.min(4, Math.max(1, t.item.heading))) : document.createElement("p");
          editor.appendChild(el);
          createdEls.push(el);
        }
        el.scrollIntoView?.({ block: "nearest" });
        await glideTo(caretAt(el));
        await drag(el, oldLength);
        if (stopped) return { typed: 0 };
        cursor.setSelection([]);
        el.textContent = "";
        return typeInto(el, t.text, (prefix) => {
          el.textContent = prefix;
        });
      }

      async function slideTarget(t) {
        const panels = getWorkspacePanels();
        // 슬라이드를 바꾸기 전에 임시로 쓴 것을 원래대로 돌린다. 영역(제목·본문)은 슬라이드마다 같은 요소를 쓰므로,
        // 나중에 돌리면 앞 슬라이드의 원래 글이 지금 슬라이드에 덮인다.
        if (t.index !== getActiveSlideIndex()) {
          revertCosmetics();
          panels?.renderSlideDeck?.(t.index);
        }
        const el = document.getElementById(t.kind === "slide-title" ? "slide-title-zone" : "slide-lower-zone");
        if (!el) return { typed: 0 };
        remember(el);
        await glideTo(caretAt(el));
        await drag(el, textNodeOf(el)?.data?.length || 0);
        if (stopped) return { typed: 0 };
        cursor.setSelection([]);
        el.textContent = "";
        return typeInto(el, t.text, (prefix) => {
          el.textContent = prefix;
        });
      }

      async function sheetTarget(t) {
        const cell = document.querySelector?.(`#sheet-grid td[data-cell="${t.coord}"]`);
        if (!cell) return { typed: t.text.length, skipped: true };
        remember(cell);
        cell.scrollIntoView?.({ block: "nearest" });
        const rect = elementRect(cell);
        await glideTo({ left: rect.left + 8, top: rect.top, height: rect.height });
        cursor.setSelection([rect]);
        cell.textContent = "";
        const result = await typeInto(cell, t.text, (prefix) => {
          cell.textContent = prefix;
        }, { speed: CELL_TYPE_SPEED, totalMs: CELL_TYPE_MS });
        cursor.setSelection([]);
        return result;
      }

      const drivers = { "doc-replace": docTarget, "doc-append": docTarget, "slide-title": slideTarget, "slide-body": slideTarget, "sheet-cell": sheetTarget };

      const done = (async () => {
        let cutAt = null;
        try {
          cursor.show();
          const rect = document.getElementById("eo-copilot-shell")?.getBoundingClientRect?.();
          cursor.place(rect ? rect.left + 24 : 640, rect ? rect.top + 120 : 200);
          for (const target of targets) {
            if (stopped) {
              cutAt = { target, typed: 0 };
              break;
            }
            const result = await drivers[target.kind](target);
            if (stopped || result.completed === false) {
              cutAt = { target, typed: result.typed ?? 0 };
              break;
            }
          }
        } finally {
          // 오류가 나도 손 감지 리스너, 포인터, 임시로 쓴 글은 정리한다.
          for (const type of handEvents) document.removeEventListener(type, onHands, true);
          cursor.hide();
          cursor.destroy();
          revertCosmetics();
        }
        return { operations: discarded ? [] : cutOperations(operations, targets, cutAt), completed: cutAt === null, interrupted: cutAt !== null, discarded };
      })();

      return { done, stop, discard, active: true };
    }

    return { perform, collectTargets, cutOperations, MAX_ANIMATED_CELLS };
  }

  host.__EASYOFFICE_CREATE_COPILOT_PERFORMER__ = createCopilotPerformer;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
