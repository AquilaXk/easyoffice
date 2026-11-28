(function (host) {
  "use strict";

  // 대/소문자 바꾸기 of the word processor window (the Word program's 홈 > 글꼴 > Aa: 문장의 첫 글자를 대문자로, 소문자로, 대문자로,
  // 각 단어의 첫 글자를 대문자로, 대/소문자 전환) and Shift+F3, and of the Pages window's 대문자 > 첫 글자만 대문자 (PD-400). The
  // command changes the characters of the selection (or the word at the caret) and leaves the run formats where they are: only
  // the `data` of the text nodes is written, so a bold word stays bold.
  // Reuse (구현 원칙 3단계): (1) no library: the conversion is the standard `String` case API (`toLocaleUpperCase` and
  // `toLocaleLowerCase` with the document language, so the Turkish dotted I and the German sharp s follow the language);
  // (2) the text node walk, the point order and the word at the caret are `doc-runs.js`, the undo step is `runs.withUndo`;
  // (3) only the sentence and word rules are new, written after the programs' documented behaviour: the Word program's
  // sentence case capitalizes the first letter after . ! ? and a space (and at the start of a paragraph) and lowercases the
  // others; each word capitalizes the first letter of every word and leaves the rest (the open-source suite's "Capitalize every word" too).

  const R = () => host.__EASYOFFICE_DOC_RUNS__;
  const LETTER = /\p{L}/u;
  // A word goes on over letters, digits and marks, and over the apostrophe inside "don't".
  const WORD_CHAR = /[\p{L}\p{N}\p{M}]/u;
  const APOSTROPHE = /['’]/u;
  const SENTENCE_END = /[.!?]/;
  // A closing quote or bracket after the full stop keeps the sentence ending open.
  const CLOSERS = /["')\]’”]/;
  const MSG_NO_TEXT = "대/소문자를 바꿀 글을 먼저 선택하세요";
  // Shift+F3: lowercase becomes each-word-capitalized, then UPPERCASE, then lowercase again.
  const CYCLE = Object.freeze({ title: "upper", upper: "lower" });

  const isUpper = (text, lang) => text === text.toLocaleUpperCase(lang) && text !== text.toLocaleLowerCase(lang);

  // `state` carries the position in a sentence and in a word from one text node to the next, so a word split over runs is one word.
  const freshState = () => ({ sentenceStart: true, endMark: false, inWord: false });

  function convert(mode, text, state, lang) {
    let out = "";
    for (const ch of text) {
      const upper = ch.toLocaleUpperCase(lang);
      const lower = ch.toLocaleLowerCase(lang);
      let next = ch;
      if (mode === "upper") next = upper;
      else if (mode === "lower") next = lower;
      else if (mode === "toggle") next = upper !== ch ? upper : lower;
      else if (mode === "title") next = !state.inWord && LETTER.test(ch) ? upper : ch;
      else if (mode === "sentence" && LETTER.test(ch)) next = state.sentenceStart ? upper : lower;
      out += next;
      advance(state, ch);
    }
    return out;
  }

  // Where `ch` leaves the sentence and the word: a full stop and a space end a sentence, a letter or a digit starts its words.
  function advance(state, ch) {
    if (SENTENCE_END.test(ch)) state.endMark = true;
    else if (/\s/u.test(ch)) {
      if (state.endMark) state.sentenceStart = true;
      state.endMark = false;
    } else if (!CLOSERS.test(ch)) state.endMark = false;
    if (LETTER.test(ch) || /\p{N}/u.test(ch)) state.sentenceStart = false;
    state.inWord = WORD_CHAR.test(ch) || (APOSTROPHE.test(ch) && state.inWord);
  }

  // The range to convert: the selection, or the word at the caret.
  function targetRange(editor, runs) {
    const range = runs.currentRange();
    if (!range) return null;
    return range.collapsed ? R().wordRangeAt(editor, range.startContainer, range.startOffset) : range;
  }

  // The pieces `[textNode, from, to]` of the range, in document order.
  function piecesOf(editor, range) {
    const r = R();
    const [a, b] = r.comparePoints(editor, range.startContainer, range.startOffset, range.endContainer, range.endOffset) <= 0
      ? [[range.startContainer, range.startOffset], [range.endContainer, range.endOffset]]
      : [[range.endContainer, range.endOffset], [range.startContainer, range.startOffset]];
    const pieces = [];
    for (const node of r.collectTextNodes(editor, editor)) {
      const length = String(node.data ?? "").length;
      if (length === 0 || r.comparePoints(editor, node, length, a[0], a[1]) <= 0 || r.comparePoints(editor, node, 0, b[0], b[1]) >= 0) continue;
      pieces.push([node, node === a[0] ? a[1] : 0, node === b[0] ? b[1] : length]);
    }
    return pieces;
  }

  const sliceOf = ([node, from, to]) => String(node.data).slice(from, to);

  // The mode Shift+F3 applies to this text.
  function nextInCycle(text, lang) {
    if (isUpper(text, lang)) return CYCLE.upper;
    const eachWord = convert("title", text.toLocaleLowerCase(lang), freshState(), lang);
    return text === eachWord ? CYCLE.title : "title";
  }

  function changeCase(mode, env) {
    const { editor, runs } = env;
    const range = targetRange(editor, runs);
    const pieces = range ? piecesOf(editor, range) : [];
    if (pieces.length === 0) {
      const el = env.document.getElementById("status-command-feedback");
      if (el) el.textContent = MSG_NO_TEXT;
      return false;
    }
    const lang = editor.lang || env.document.documentElement?.lang || undefined;
    const chosen = mode === "cycle" ? nextInCycle(pieces.map(sliceOf).join(""), lang) : mode;
    runs.withUndo(() => {
      const state = freshState();
      let changed = false;
      let block = null;
      for (const piece of pieces) {
        const [node, from, to] = piece;
        // A new paragraph starts a new sentence and a new word.
        const own = R().blockOf(node, editor);
        if (own !== block) {
          block = own;
          Object.assign(state, freshState());
        }
        const before = String(node.data);
        const after = before.slice(0, from) + convert(chosen, before.slice(from, to), state, lang) + before.slice(to);
        if (after !== before) {
          node.data = after;
          changed = true;
        }
      }
      return { changed };
    });
    return true;
  }

  const MODES = Object.freeze({
    "text-case-sentence": "sentence",
    "text-case-lower": "lower",
    "text-case-upper": "upper",
    "text-case-title": "title",
    "text-case-toggle": "toggle",
  });
  const commands = host.__EASYOFFICE_DOC_WORD_COMMANDS__;
  for (const [cmd, mode] of Object.entries(MODES)) commands?.register(cmd, (env) => changeCase(mode, env));

  commands?.register("text-case-cycle", (env) => changeCase("cycle", env));

  // Shift+F3 (`doc-case-cycle` in the shortcut table, the word processor window only): the cycle on the selection.
  const shortcutActions = () => ({
    "doc-case-cycle": () => {
      const document = host.document;
      const runs = host.__EASYOFFICE_DOC_RUNS_UI__;
      const editor = document?.getElementById("doc-editor");
      if (runs && editor) commands.run("text-case-cycle", { editor, document, runs, module: "DOC" });
    },
  });

  host.__EASYOFFICE_DOC_TEXT_CASE__ = { convert, freshState, changeCase, nextInCycle, shortcutActions };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
