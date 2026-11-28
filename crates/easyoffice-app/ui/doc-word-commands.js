(function (host) {
  "use strict";

  // The studio commands that only the word processor window (the Word program's menus) has: 서식 > 단락…, the format
  // painter and 서식 지우기, the date and time, bookmarks and cross references, section breaks, and the others in
  // `doc-*` files that call `register`. The command dispatcher (`workspace-tools.js`) asks `run` before it falls back to
  // the shared commands; `run` answers true when it handled the command. The Hangul and Pages windows never reach it.

  const handlers = new Map();

  const isWordWindow = (env) => env.module === "DOC" && env.editor?.dataset?.eoDocFlavor !== "pages";

  function register(cmd, handler) {
    handlers.set(cmd, handler);
  }

  function run(cmd, env) {
    const handler = handlers.get(cmd);
    if (!handler) return false;
    if (isWordWindow(env)) handler(env);
    return true;
  }

  register("para-dialog", (env) => host.__EASYOFFICE_DOC_LINE_SPACING__.openParagraphDialog(env.document, env.runs));

  host.__EASYOFFICE_DOC_WORD_COMMANDS__ = { register, run, has: (cmd) => handlers.has(cmd) };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
