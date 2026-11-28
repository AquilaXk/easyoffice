// 참조 구현: 같은 폴더의 `copilot-cursor.js`(excalidraw 협업자 포인터와 y-prosemirror 캐럿·선택 강조), `copilot-typing.js`(typed.js 타이핑 루프),
// `copilot-timeline.js`(rrweb 재생기 `Timer`와 motion 시퀀스의 시각 읽기)를 문서·슬라이드·시트에 엮는 연출이다.
// 누르기·끌기·놓기·쉼 같은 몸짓은 `copilot-timeline.js`의 장면(단계 목록 + `at` 위치)으로 짜고 재생기가 시각에 맞춰 돌린다. 이 파일의 나머지(연산에서 목표 뽑기,
// 쓴 곳까지 연산 자르기, 임시로 쓴 것 되돌리기)는 직접 썼다: 포인터가 선택하고 글을 쳐 넣는 문서 편집이라는 원본이 따로 없다.
//
// 슬라이드 쪽(새 슬라이드, 개체 놓기·옮기기, 레이아웃·전환, 노트)은 `copilot-perform-slide.js`가 맡고, 이 파일은 문서·시트 셀 쓰기와 공통 도구(포인터, 타이핑, 드래그 선택,
// 임시로 쓴 것의 되돌리기)를 둔다. 문서의 범위 교체(`doc.replace_paragraphs`의 `startIndex`·`endIndex`)는 옛 범위를 끌어 선택하고 가린 뒤 새 블록을 그 자리에 친다(PD-304).
// 글을 치지 않는 연출(PD-387)은 모듈별 파일이 맡는다: 시트 서식·차트·행/열(`copilot-perform-sheet.js`), 데이터베이스·보드(`copilot-perform-ext.js`),
// 내장 한글 편집기(`copilot-perform-hwp.js`), PDF 인용 쪽(`copilot-perform-cite.js`). 옛 편집기 한글은 문서 연출을 그대로 탄다.
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
  const CLICK_MS = 160;
  const opKind = (op) => String(op?.op || op?.type || op?.kind || "").trim();

  function createCopilotPerformer(deps) {
    const { document, win = host, docBlocksOf, getWorkspacePanels, slideDeckState, getActiveSlideIndex, getActiveSheetName, getSheetController, isReducedMotion, createCursor, createTypist } = deps;
    const { blockText, hwpApi = null, boardApi = null, goToPdfPage } = deps;
    const slidePerf = host.__EASYOFFICE_COPILOT_PERFORM_SLIDE__.createSlidePerformance({ document, slideDeckState, getActiveSlideIndex, getWorkspacePanels });
    const sheetPerf = host.__EASYOFFICE_COPILOT_PERFORM_SHEET__.createSheetPerformance({ document, getActiveSheetName });
    const extPerf = host.__EASYOFFICE_COPILOT_PERFORM_EXT__.createExtPerformance({ document, boardApi });
    const hwpPerf = host.__EASYOFFICE_COPILOT_PERFORM_HWP__.createHwpPerformance({ hwpApi });
    const pdfPerf = host.__EASYOFFICE_COPILOT_PERFORM_PDF__.createPdfPerformance({ document, goToPdfPage });
    // 글을 치지 않는 단계가 잘리는 방식은 모듈이 정한다(`target.owner`).
    const owners = { sheet: sheetPerf, ext: extPerf, hwp: hwpPerf, pdf: pdfPerf };
    // 모듈별 연출 단계 수 상한(앞 이름이 단계 종류의 앞부분이다). 셀 쓰기(`sheet-cell`)는 `MAX_ANIMATED_CELLS`가 따로 센다.
    const STEP_BUDGETS = {
      sheet: host.__EASYOFFICE_COPILOT_PERFORM_SHEET__.MAX_SHEET_STEPS,
      base: host.__EASYOFFICE_COPILOT_PERFORM_EXT__.MAX_BASE_STEPS,
      board: host.__EASYOFFICE_COPILOT_PERFORM_EXT__.MAX_BOARD_NODES,
      hwp: host.__EASYOFFICE_COPILOT_PERFORM_HWP__.MAX_HWP_STEPS,
      pdf: host.__EASYOFFICE_COPILOT_PERFORM_PDF__.MAX_PDF_PAGES,
    };
    const budgetKey = (kind) => {
      const family = kind.split("-")[0];
      return kind !== "sheet-cell" && Object.hasOwn(STEP_BUDGETS, family) ? family : null;
    };
    const withinBudgets = (targets) => {
      const used = {};
      return targets.filter((target) => {
        const key = budgetKey(target.kind);
        if (!key) return true;
        used[key] = (used[key] || 0) + 1;
        return used[key] <= STEP_BUDGETS[key];
      });
    };

    const isTableDocItem = (item) => {
      if (typeof item === "string") return /^<table\b/i.test(item.trim());
      if (!item || typeof item !== "object") return false;
      return Boolean(item.table || String(item.kind || item.tag || "").toLowerCase() === "table" || Array.isArray(item.rows));
    };
    const hasInlineMarkup = (str) => typeof str === "string" && /<\/?(?:strong|b|em|i|u|s|del|strike|a|table|br)\b/i.test(str);

    // ---- 목표: 연산에서 한 자씩 칠 글을 순서대로 뽑는다. `cut(clone, typed)`가 쓰던 곳까지로 연산 복사본을 자른다.
    function collectTargets(mod, operations) {
      const targets = [];
      // 앞 연산이 더한 슬라이드까지 센 슬라이드 수(그 슬라이드는 아직 없다).
      const deck = mod === "SLIDE" ? { length: slideDeckState.length } : null;
      // 내장 한글 편집기는 문서 편집기 DOM이 없다. 옛 편집기 한글은 문서와 같은 연출을 탄다.
      const embedHwp = mod === "HWP" && Boolean(hwpApi?.isEmbed?.());
      // 응답 하나 안에서 연산 사이로 이어지는 값: 보드에 앞 그래프가 놓은 틀들, PDF에서 이미 가리킨 쪽.
      const carried = { placed: [], pages: new Set() };
      operations.forEach((op, opIndex) => {
        const kind = opKind(op);
        if (mod === "DOC" || (mod === "HWP" && !embedHwp)) {
          if (kind === "doc.replace_span") {
            const index = Number(op?.index);
            if (Number.isInteger(index) && index >= 0) {
              const docHtml = host.__EASYOFFICE_COPILOT_DOC_HTML__;
              const blockEl = docBlocksOf(document.getElementById("doc-editor"))[index];
              const cur = blockEl
                ? (docHtml?.plainActiveTextOfBlock ? docHtml.plainActiveTextOfBlock(blockEl) : String(blockEl.textContent || ""))
                : "";
              const search = String(op?.search ?? "");
              const replace = String(op?.replace ?? "");
              const at = search ? cur.indexOf(search) : -1;
              const unique = at >= 0 && cur.indexOf(search, at + 1) < 0;
              const before = unique ? cur.slice(0, at) : cur;
              const after = unique ? cur.slice(at + search.length) : "";
              const hasMarkup = Boolean(blockEl?.querySelector?.("span.eo-run, a, strong, em, u, s, del, ins")) || hasInlineMarkup(replace);
              targets.push({ opIndex, kind: "doc-replace-span", entry: 0, index, before, after, text: replace, atomicDoc: hasMarkup });
            }
          } else if (kind === "doc.replace_paragraphs" && Array.isArray(op.replacements)) {
            op.replacements.forEach((rep, entry) => {
              const index = Number(rep?.index);
              const raw = String(rep?.text ?? "");
              if (Number.isInteger(index) && index >= 0) {
                targets.push({ opIndex, kind: "doc-replace", entry, index, text: blockText(raw), atomicDoc: isTableDocItem(raw) || hasInlineMarkup(raw) });
              }
            });
          } else if (kind === "doc.append" || kind === "doc.replace_paragraphs") {
            const key = Array.isArray(op.blocks) ? "blocks" : Array.isArray(op.paragraphs) ? "paragraphs" : null;
            const range = kind === "doc.append" ? {} : { startIndex: op.startIndex, endIndex: op.endIndex };
            (key ? op[key] : []).forEach((item, entry) => {
              const rawItemText = typeof item === "string" ? item : String(item?.text ?? item?.content ?? "");
              targets.push({
                opIndex,
                kind: kind === "doc.append" ? "doc-append" : "doc-range",
                key,
                entry,
                text: blockText(item),
                item,
                atomicDoc: isTableDocItem(item) || hasInlineMarkup(rawItemText),
                ...range,
              });
            });
          }
        } else if (mod === "SLIDE") {
          targets.push(...slidePerf.collect(op, opIndex, kind, deck));
        } else if (mod === "SHEET" && kind === "sheet.write") {
          if (Array.isArray(op.cells)) {
            op.cells.slice(0, MAX_ANIMATED_CELLS).forEach((cell, entry) => {
              const coord = String(cell?.coord || "").trim().toUpperCase();
              if (coord) targets.push({ opIndex, kind: "sheet-cell", entry, coord, sheet: String(op.sheet || getActiveSheetName()), text: String(cell.raw ?? cell.formula ?? cell.value ?? "") });
            });
          }
        } else if (mod === "SHEET") {
          targets.push(...sheetPerf.collect(op, opIndex, kind));
        } else if (embedHwp) {
          targets.push(...hwpPerf.collect(op, opIndex, kind));
        } else if (mod === "BASE" || mod === "BOARD") {
          targets.push(...extPerf.collect(mod, op, opIndex, kind, carried));
        } else if (mod === "PDF") {
          targets.push(...pdfPerf.collect(op, opIndex, kind, carried.pages));
        }
      });
      return withinBudgets(targets).slice(0, MAX_ANIMATED_TARGETS);
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
        if (target.kind.startsWith("slide")) {
          const kept = slidePerf.cut(op, opKind(op), target, typed, targets.slice(0, targets.indexOf(target)));
          if (kept) out.push(kept);
          return;
        }
        const owner = owners[target.owner];
        if (owner) {
          const kept = owner.cut(op, opKind(op), target);
          if (kept) out.push(kept);
          return;
        }
        const keepFirst = (arr, partial) => arr.slice(0, target.entry).concat(partial ? [partial] : []);
        const fullAtomic = !stop.unstarted && typed >= target.text.length;
        if (target.kind === "doc-replace") {
          const origRep = op.replacements[target.entry];
          const partial = target.atomicDoc
            ? (fullAtomic ? origRep : null)
            : (prefix ? { ...origRep, text: prefix } : null);
          op.replacements = keepFirst(op.replacements, partial);
        } else if (target.kind === "doc-replace-span") {
          if (stop.unstarted || (!fullAtomic && (target.atomicDoc || !prefix))) return;
          if (!fullAtomic) op.replace = prefix;
        } else if (target.kind === "doc-append" || target.kind === "doc-range") {
          const item = op[target.key][target.entry];
          const partial = target.atomicDoc
            ? (fullAtomic ? item : null)
            : (!prefix ? null : typeof item === "string" ? prefix : Array.isArray(item?.items) ? { ...item, items: prefix.split("\n") } : { ...item, text: prefix });
          op[target.key] = keepFirst(op[target.key], partial);
        } else if (target.kind === "sheet-cell") {
          op.cells = keepFirst(op.cells, prefix && !isAtomicText(target.text) ? { ...op.cells[target.entry], raw: prefix, formula: undefined, value: undefined } : null);
        }
        // 쓴 것이 하나도 없는 연산은 뺀다(빈 목록이나 빈 슬라이드 변경은 엔진 쪽 검사에서 걸린다).
        const lists = ["replacements", "blocks", "paragraphs", "cells"].filter((key) => Array.isArray(op[key]));
        if (!(lists.length > 0 && lists.every((key) => op[key].length === 0))) out.push(op);
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
    // 클래스·속성값처럼 자식이 아닌 것을 바꾼 연출은 되돌리는 함수를 남긴다(나중에 바꾼 것부터 돌린다).
    const reverts = [];
    const onRevert = (fn) => reverts.push(fn);

    function revertCosmetics() {
      for (const fn of reverts.splice(0).reverse()) fn();
      for (const el of createdEls.splice(0)) el.remove();
      for (const { el, nodes } of touched.splice(0)) el.replaceChildren(...nodes);
    }

    function toggleClass(el, name, on) {
      const was = el.classList.contains(name);
      el.classList.toggle(name, on);
      onRevert(() => el.classList.toggle(name, was));
    }

    function setProp(el, prop, value) {
      const was = el[prop];
      el[prop] = value;
      onRevert(() => {
        el[prop] = was;
      });
    }

    // 임시로 가린 요소(`copilot.css`의 `.eo-copilot-ghost`). 문서 블록의 번호는 그대로 둔다.
    const hide = (el) => toggleClass(el, "eo-copilot-ghost", true);

    function perform(mod, operations) {
      const targets = collectTargets(mod, operations);
      const reduced = Boolean(isReducedMotion?.());
      if (reduced || targets.length === 0) {
        return { done: Promise.resolve({ operations: cutOperations(operations, targets, null), completed: true, discarded: false }), stop() {}, discard() {}, active: false };
      }
      const cursor = createCursor();
      const scenes = host.__EASYOFFICE_COPILOT_TIMELINE__.createScenePlayer(cursor.timing);
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
        scenes.clear();
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

      // 멈춘 뒤에는 새 장면을 시작하지 않는다(시작하면 타이머가 끝날 때까지 연출이 남는다). 끝까지 갔으면 `true`.
      const play = (steps) => (stopped ? Promise.resolve(false) : scenes.play(steps));

      // 누르기 → 끌기 → 놓기: 마우스를 누른 채 `ms` 동안 `step(eased)`를 프레임마다 돌리고 뗀다.
      const gesture = (ms, step) => play([
        { run: () => cursor.press(true) },
        { at: "<", duration: ms, run: () => cursor.animate(ms, step) },
        { run: () => cursor.press(false) },
      ]);

      // 요소들의 글을 처음부터 끝까지 끌어 선택하는 몸짓. 글이 없는 요소는 건너뛰고, 모두 없으면 하지 않는다.
      async function drag(els) {
        const parts = els.map((el) => {
          const node = textNodeOf(el);
          return { el, node, length: node ? node.data.length : String(el.textContent || "").length };
        }).filter((part) => part.length > 0);
        const total = parts.reduce((sum, part) => sum + part.length, 0);
        if (total <= 0 || stopped) return;
        // 글 위치를 잴 수 없는 요소(직접 글 노드가 없는 목록, 서식 조각이 섞인 문단)는 통째로 칠한다.
        const rectsOf = (part, take) => {
          const rects = part.node ? rangeRects(part.node, 0, take) : [];
          return rects.length > 0 ? rects : [elementRect(part.el)];
        };
        await gesture(DRAG_MS, (t) => {
          let left = Math.max(1, Math.round(total * t));
          const rects = [];
          for (const part of parts) {
            const take = Math.min(part.length, left);
            if (take > 0) rects.push(...rectsOf(part, take));
            left -= take;
          }
          cursor.setSelection(rects);
          const last = rects.at(-1);
          if (last) cursor.place(last.right, last.top + last.height / 2);
        });
      }

      async function typeInto(el, text, setText, { speed, totalMs, caret: caretFor } = {}) {
        typing = createTypist({ onBlink: (idle) => cursor.setIdle(idle), ...(speed ? { typeSpeed: speed, maxTotalMs: totalMs, sentencePauseMs: 40 } : {}) });
        // 타이머 안에서 던진 오류는 약속이 받지 못한다: 붙잡아 입력을 끝내고 다 기다린 뒤 다시 던진다.
        let failure = null;
        const result = await typing.type(text, (prefix) => {
          try {
            setText(prefix);
            const caret = caretFor ? caretFor(prefix) : caretAt(el);
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

      // 선택한 글을 지우고 `el`에 새 글을 한 자씩 쳐 넣는다.
      async function typeOver(el, t) {
        if (stopped) return { typed: 0 };
        cursor.setSelection([]);
        el.textContent = "";
        return typeInto(el, t.text, (prefix) => {
          el.textContent = prefix;
        });
      }

      // 임시로 쓸 새 블록(제목이면 제목 태그). 쓴 글은 연출이 끝나면 지워진다.
      function tempBlock(t, before) {
        const heading = t.item && typeof t.item === "object" && Number.isInteger(t.item.heading) ? t.item.heading : 0;
        const el = document.createElement(heading ? "h" + Math.min(4, Math.max(1, heading)) : "p");
        document.getElementById("doc-editor").insertBefore(el, before);
        createdEls.push(el);
        return el;
      }

      async function typeInNewBlock(t, before) {
        const block = tempBlock(t, before);
        block.scrollIntoView?.({ block: "nearest" });
        await glideTo(caretAt(block));
        return typeOver(block, t);
      }

      async function docTarget(t) {
        const el = docBlocksOf(document.getElementById("doc-editor"))[t.index];
        if (!el) return typeInNewBlock(t, null);
        remember(el);
        el.scrollIntoView?.({ block: "nearest" });
        await glideTo(caretAt(el));
        await drag([el]);
        return typeOver(el, t);
      }

      async function docSpanTarget(t) {
        const el = docBlocksOf(document.getElementById("doc-editor"))[t.index];
        if (!el) return typeInNewBlock(t, null);
        remember(el);
        el.scrollIntoView?.({ block: "nearest" });
        await glideTo(caretAt(el));
        await drag([el]);
        if (stopped) return { typed: 0 };
        cursor.setSelection([]);
        el.textContent = t.before + t.after;
        return typeInto(el, t.text, (prefix) => {
          el.textContent = t.before + prefix + t.after;
        });
      }

      // 범위 교체: 처음에 옛 범위를 끌어 선택해 가리고, 새 블록들을 그 자리에 차례로 쳐 넣는다.
      const rangeAnchors = new Map();

      async function docRangeTarget(t) {
        if (t.entry === 0) {
          const blocks = docBlocksOf(document.getElementById("doc-editor"));
          const start = Number.isInteger(t.startIndex) ? Math.max(0, t.startIndex) : 0;
          const end = Number.isInteger(t.endIndex) ? Math.min(blocks.length - 1, t.endIndex) : blocks.length - 1;
          const old = blocks.slice(start, end + 1);
          rangeAnchors.set(t.opIndex, blocks[end + 1] || null);
          if (old.length > 0) {
            old[0].scrollIntoView?.({ block: "nearest" });
            await glideTo(caretAt(old[0]));
            await drag(old);
            if (stopped) return { typed: 0 };
            old.forEach(hide);
          }
        }
        return typeInNewBlock(t, rangeAnchors.get(t.opIndex) ?? null);
      }

      async function sheetTarget(t) {
        if (t.sheet && t.sheet !== getActiveSheetName()) return { typed: t.text.length, skipped: true };
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

      const session = {
        cursor, drag, typeInto, remember, onRevert, toggleClass, setProp, hide,
        addTemp: (el) => createdEls.push(el),
        revert: revertCosmetics,
        stopped: () => stopped,
        glideTo, caretAt, rectOf: elementRect,
        moveTo: (x, y) => (stopped ? Promise.resolve(false) : cursor.moveTo(x, y)),
        play, gesture,
        click: (ms) => play([{ run: () => cursor.press(true) }, { at: ms, run: () => cursor.press(false) }]),
        hold: (ms) => play([{ duration: ms }]),
        // 화면에 보이는 요소의 사각형. 없거나 가려져 크기가 없으면 `null`(그 단계는 연출 없이 지나간다).
        visibleRect: (el) => {
          if (!el) return null;
          const rect = elementRect(el);
          return rect.width > 0 && rect.height > 0 ? rect : null;
        },
        // 사각형 가운데로 가서 누른다.
        clickAt: async (rect) => {
          await session.moveTo(rect.left + rect.width / 2, rect.top + rect.height / 2);
          await session.click(CLICK_MS);
        },
      };
      const drivers = {
        "doc-replace": docTarget,
        "doc-replace-span": docSpanTarget,
        "doc-append": (t) => typeInNewBlock(t, null),
        "doc-range": docRangeTarget,
        "sheet-cell": sheetTarget,
        ...slidePerf.createDrivers(session),
        ...sheetPerf.createDrivers(session),
        ...extPerf.createDrivers(session),
        ...hwpPerf.createDrivers(session),
        ...pdfPerf.createDrivers(session),
      };

      const done = (async () => {
        let cutAt = null;
        try {
          cursor.show();
          const rect = document.getElementById("eo-copilot-shell")?.getBoundingClientRect?.();
          cursor.place(rect ? rect.left + 24 : 640, rect ? rect.top + 120 : 200);
          for (const target of targets) {
            if (stopped) {
              cutAt = { target, typed: 0, unstarted: true };
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

    return {
      perform, collectTargets, cutOperations, MAX_ANIMATED_CELLS,
      MAX_SHEET_STEPS: STEP_BUDGETS.sheet, MAX_BASE_STEPS: STEP_BUDGETS.base, MAX_BOARD_NODES: STEP_BUDGETS.board, MAX_HWP_STEPS: STEP_BUDGETS.hwp, MAX_PDF_PAGES: STEP_BUDGETS.pdf,
    };
  }

  host.__EASYOFFICE_CREATE_COPILOT_PERFORMER__ = createCopilotPerformer;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
