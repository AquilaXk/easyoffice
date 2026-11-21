// 참조 구현 (reference implementation): glide-data-grid (`glideapps/glide-data-grid`, MIT, Copyright (c) 2021 typeguard, Inc.,
// commit 0875d78cc41535a39f8b417ff88ce1ebe129c363; see `THIRD_PARTY_NOTICES.md`) 데이터시트 칸 선택의 키 규칙과 복사·붙여넣기 글:
//   packages/core/src/data-editor/data-editor-keybindings.ts 129-186 (`realizeKeybinds`: 어느 키가 어느 동작인가),
//   packages/core/src/data-editor/data-editor.tsx 3188-3418 (`handleFixedKeybindings`: 지우기, 활성화, 쪽 이동, 처음·끝, 편집 중 키)와
//   3484-3530 (`onKeyDown`의 글자 바로 치기 조건), 3698-3745 (붙여넣기 범위 자르기),
//   packages/core/src/data-editor/copy-paste.ts 130-155 (`escapeIfNeeded`, `createTextBuffer`: 칸을 탭, 줄을 줄바꿈으로 잇고 따옴표로 감쌈),
//   packages/core/src/data-editor/data-editor-fns.ts 100-165 (`descape`, `unquote`: 따옴표 글을 칸으로 가르는 상태 기계).
// 바꾼 점: 원본은 캔버스 격자의 선택 범위(`CompactSelection`, 여러 범위 쌓기, `adjustSelection`)를 쓰지만, 이 파일은 시트의 `sheet-nav.js`가 쓰는
// 선택(활성 칸 + 반대 모서리, 1부터 시작)에 그 키 동작을 붙인다. 이동 계산(한 칸, Ctrl+방향키, 쪽, 처음, 끝, 범위 안 순환)은 `sheet-nav.js`
// 함수를 그대로 호출한다. 따옴표는 칸 맨 앞에서만 열리고(원본은 칸 가운데 따옴표도 열어 줄바꿈을 삼킴), 붙여넣기 글에는 상한이 있다(원본은 없음).
// 원본의 Alt+방향키(선택을 남기고 이동), 줄·열 선택, 채우기, 잘라내기, 찾기 키는 가져오지 않았다.

(function (host) {
  "use strict";

  // A paste over these is refused whole, not cut: the text, and the rows and columns the model can hold (`base-model.js`).
  const MAX_PASTE_CHARS = 8 * 1024 * 1024, MAX_PASTE_ROWS = 10000, MAX_PASTE_COLS = 64;
  const ARROWS = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1] };
  // `data-editor.tsx` 3512: a key types into the cell when it is one character of a letter, mark, number, symbol or punctuation.
  const TYPED_KEY = /^[\p{L}\p{M}\p{N}\p{S}\p{P}]$/u;
  const TAB = 9, NEWLINE = 10, QUOTE = 34;

  // What a key does to the cells, or null when it is not a key of the cells. `editing` is true while a control of the cell has the focus:
  // then only Enter, Tab and Esc are the grid's, the rest belongs to the text.
  function intentOf(ev, editing) {
    const { key, shiftKey: shift } = ev, mod = ev.ctrlKey || ev.metaKey, typed = !mod && TYPED_KEY.test(key);
    if (editing) {
      if (ev.altKey || mod) return null;
      if (key === "Escape") return { t: "cancel" };
      if (key === "Enter") return { t: "walk", dr: shift ? -1 : 1, dc: 0 };
      return key === "Tab" ? { t: "walk", dr: 0, dc: shift ? -1 : 1 } : null;
    }
    // Option+key types a mark on a Mac, so it still starts an edit; every other Alt key is the browser's.
    if (ev.altKey) return typed ? { t: "type", text: key } : null;
    const arrow = ARROWS[key];
    if (arrow) return { t: "move", dr: arrow[0], dc: arrow[1], jump: mod, extend: shift };
    if (key === "Home") return { t: "home", whole: mod, extend: shift };
    if (key === "End") return { t: "end", whole: mod, extend: shift };
    if (mod) return key.toLowerCase() === "a" ? { t: "all" } : null;
    if (key === "PageDown" || key === "PageUp") return { t: "page", dr: key === "PageDown" ? 1 : -1, extend: shift };
    if (key === "Tab") return { t: "walk", dr: 0, dc: shift ? -1 : 1 };
    if (key === "Enter" || key === "F2" || key === " ") return { t: "edit" };
    if (key === "Delete" || key === "Backspace") return { t: "clear" };
    if (key === "Escape") return { t: "escape" };
    return typed ? { t: "type", text: key } : null;
  }

  // The rectangle a selection covers: the range a walk through it keeps (`span`), else the box of the active cell and the far corner.
  const rangeOf = (sel) => sel.span || { r1: Math.min(sel.ar, sel.cr), r2: Math.max(sel.ar, sel.cr), c1: Math.min(sel.ac, sel.cc), c2: Math.max(sel.ac, sel.cc) };
  const single = (r, c) => ({ ar: r, ac: c, cr: r, cc: c, span: null, tabFrom: null });
  const isSingle = (range) => range.r1 === range.r2 && range.c1 === range.c2;

  // The selection after an arrow, Home, End or page key. `view` is the table as `sheet-nav.js` reads it (1-based rows and columns),
  // `span` the rows of a page. With Shift the far corner moves and the active cell stays.
  function selectionAfter(view, sel, intent, span) {
    const nav = host.__EASYOFFICE_SHEET_NAV__, extend = Boolean(intent.extend);
    const base = sel.span ? { ...sel, cr: sel.ar, cc: sel.ac, span: null } : sel;
    const from = extend ? { r: base.cr, c: base.cc } : { r: base.ar, c: base.ac };
    let to = null;
    if (intent.t === "move") to = intent.jump ? nav.jump(view, from, intent.dr, intent.dc) : nav.step(view, from, intent.dr, intent.dc);
    else if (intent.t === "home") to = nav.home(view, { r: base.ar, c: base.ac }, intent.whole);
    else if (intent.t === "end") to = intent.whole ? nav.end(view) : { r: from.r, c: view.maxCols };
    else if (intent.t === "page") to = nav.page(view, from, intent.dr, 0, span);
    if (!to) return sel;
    return extend ? { ...base, cr: to.r, cc: to.c, tabFrom: null } : single(to.r, to.c);
  }

  // Enter and Tab while a cell is edited, and Tab at rest. With several cells selected the active cell walks through them and the range stays;
  // a row of Tabs sends the Enter after it back to the column it started in (both as in the spreadsheet: `sheet-input.js` `walk`).
  function walkAfter(view, sel, dr, dc) {
    const nav = host.__EASYOFFICE_SHEET_NAV__, range = rangeOf(sel), at = { r: sel.ar, c: sel.ac };
    if (!isSingle(range)) {
      const next = nav.cycle(view, range, at, dr, dc);
      return { ...sel, ar: next.r, ac: next.c, cr: next.r, cc: next.c, span: range, tabFrom: null };
    }
    const origin = dc > 0 && !dr ? sel.tabFrom || at : null, base = dr > 0 && sel.tabFrom ? sel.tabFrom : at;
    const to = nav.step(view, base, dr, dc);
    return { ...single(to.r, to.c), tabFrom: origin };
  }

  // The value that empties a cell of this type (Delete): what `validateCellValue` of the model reads as "nothing".
  function emptyValueOf(field) {
    if (field.type === "checkbox") return false;
    return field.type === "multiSelect" || field.type === "relation" ? [] : "";
  }

  // ---- text on the clipboard: tab between cells, newline between rows ----

  const escapeCell = (text) => (/[\t\r\n"]/.test(text) ? '"' + text.replaceAll('"', '""') + '"' : text);
  const toTsv = (rows) => rows.map((row) => row.map(escapeCell).join("\t")).join("\n");

  const descape = (cell) => (cell.length > 1 && cell.charCodeAt(0) === QUOTE && cell.charCodeAt(cell.length - 1) === QUOTE ? cell.slice(1, -1).replaceAll('""', '"') : cell);

  // The cells of clipboard text (rows of strings), or null when it is over the limits. A cell in quotes may hold tabs, newlines and "" for a quote.
  // A quote opens a quoted cell at the start of a cell only. The last newline does not start a row.
  function parseTsv(text) {
    let src = String(text ?? "");
    if (src.length > MAX_PASTE_CHARS) return null;
    if (src.includes("\r")) src = src.replace(/\r\n?/g, "\n");
    const rows = [];
    let row = [], start = 0, quoted = false, closed = false;
    const endCell = (i) => { row.push(closed ? descape(src.slice(start, i)) : src.slice(start, i)); start = i + 1; quoted = false; closed = false; return row.length <= MAX_PASTE_COLS; };
    for (let i = 0; i < src.length; i += 1) {
      const ch = src.charCodeAt(i);
      if (quoted) {
        if (ch !== QUOTE) continue;
        if (src.charCodeAt(i + 1) === QUOTE) i += 1;
        else { quoted = false; closed = true; }
      } else if (ch === QUOTE && i === start) quoted = true;
      else if (ch === TAB || ch === NEWLINE) {
        if (!endCell(i)) return null;
        if (ch === NEWLINE) {
          rows.push(row);
          row = [];
          if (rows.length > MAX_PASTE_ROWS) return null;
        }
      }
    }
    if (start < src.length || row.length > 0) {
      if (!endCell(src.length)) return null;
      rows.push(row);
    }
    return rows.length > MAX_PASTE_ROWS ? null : rows;
  }

  host.__EASYOFFICE_BASE_CELL_NAV__ = Object.freeze({ intentOf, rangeOf, single, isSingle, selectionAfter, walkAfter, emptyValueOf, toTsv, parseTsv, MAX_PASTE_CHARS, MAX_PASTE_ROWS, MAX_PASTE_COLS });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
