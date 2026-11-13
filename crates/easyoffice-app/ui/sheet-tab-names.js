(function (host) {
  "use strict";

  // Sheet names (PD-209). The rules are the ones the workbook files impose and the engine checks
  // again (`sheet_name_problem`): 31 UTF-16 units, not blank, none of : \ / ? * [ ] or a control
  // character, no apostrophe at either end, not `History`, and no name another sheet has (case
  // aside). The page shows a sheet whose name is `Sheet<N>` as `시트 <N>`; both spellings are taken.

  const MAX_NAME_UNITS = 31;
  const MAX_SHEETS = 256; // what a save takes (`MAX_SHEETS` of the structured request)
  const FORBIDDEN = new Set([":", "\\", "/", "?", "*", "[", "]"]);
  const CONTROL = /\p{Cc}/u;

  const labelOf = (name) => {
    const m = /^Sheet(\d+)$/.exec(String(name));
    return m ? "시트 " + m[1] : String(name);
  };
  const fold = (text) => String(text).toLowerCase();
  const asciiFold = (text) => String(text).replace(/[A-Z]/g, (c) => c.toLowerCase());

  // The first rule `name` breaks as a code, or null. `char` is the forbidden character.
  function problemOf(name) {
    const text = String(name ?? "");
    if (text.trim() === "") return { code: "empty" };
    if (text.length > MAX_NAME_UNITS) return { code: "too-long" };
    for (const ch of text) {
      if (FORBIDDEN.has(ch) || CONTROL.test(ch)) return { code: "forbidden", char: ch };
    }
    if (text.startsWith("'") || text.endsWith("'")) return { code: "apostrophe" };
    if (asciiFold(text) === "history") return { code: "reserved" };
    return null;
  }

  const MESSAGES = {
    empty: "시트 이름을 입력",
    "too-long": "시트 이름은 " + MAX_NAME_UNITS + "자까지",
    apostrophe: "시트 이름은 작은따옴표로 시작하거나 끝날 수 없음",
    reserved: "예약된 시트 이름",
  };

  function messageOf(problem) {
    if (!problem) return "";
    if (problem.code === "forbidden") {
      return "시트 이름에 쓸 수 없는 글자: " + (CONTROL.test(problem.char) ? "제어 문자" : problem.char);
    }
    return MESSAGES[problem.code];
  }

  // Whether another sheet of `names` (keys) has `name`, as its key or as the label the page shows.
  function isTaken(name, names, except) {
    const want = fold(name);
    return names.some((key) => key !== except && (fold(key) === want || fold(labelOf(key)) === want));
  }

  // The first problem of `name` as a message, "" when it is a good name for the sheet `except`.
  function checkName(name, names, except) {
    const bad = problemOf(name);
    if (bad) return messageOf(bad);
    return isTaken(name, names, except) ? "이미 있는 시트 이름" : "";
  }

  // "Sheet<N>" for the new sheet: the first number from `names.length + 1` that no sheet uses.
  function nextDefault(names) {
    let n = names.length + 1;
    while (isTaken("Sheet" + n, names) || isTaken("시트 " + n, names)) n += 1;
    return "Sheet" + n;
  }

  // The longest start of `text` that fits `units` UTF-16 units without splitting a character.
  function fit(text, units) {
    let out = "";
    for (const ch of text) {
      if (out.length + ch.length > units) break;
      out += ch;
    }
    return out;
  }

  // "<name> (2)", "<name> (3)", ...: the copy of the sheet shown as `label`, cut to fit.
  function copyName(label, names) {
    const base = String(label).replace(/[:\\/?*[\]\p{Cc}]/gu, "_");
    for (let n = 2; ; n += 1) {
      const suffix = " (" + n + ")";
      const name = fit(base, MAX_NAME_UNITS - suffix.length).replace(/'+$/, "") + suffix;
      if (!isTaken(name, names) && !problemOf(name)) return name;
    }
  }

  host.__EASYOFFICE_SHEET_TAB_NAMES__ = { MAX_NAME_UNITS, MAX_SHEETS, labelOf, problemOf, messageOf, isTaken, checkName, nextDefault, copyName };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
