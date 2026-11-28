(function (host) {
  "use strict";

  // The studio commands that only the word processor window (the Word program's menus) has: 서식 > 단락…, the format
  // painter and 서식 지우기, the date and time, bookmarks and cross references, section breaks, and the others in
  // `doc-*` files that call `register`. The command dispatcher (`workspace-tools.js`) asks `run` before it falls back to
  // the shared commands; `run` answers true when it handled the command. The Hangul window never reaches it, and the Pages
  // window reaches only the ones in `PAGES_COMMANDS` (PD-399): the commands its menus name, each one the reference app's.

  const handlers = new Map();

  const PAGES_COMMANDS = new Set([
    "insert-datetime", "insert-bookmark", "layout-watermark", "home-highlight", "text-case-none", "text-case-all", "text-case-small", "text-case-title",
    "image-wrap-inline", "image-wrap-square", "image-wrap-topBottom", "image-wrap-top-bottom", "image-wrap-behind", "image-wrap-front",
    "image-crop", "image-rotate-right", "image-rotate-left",
  ]);
  const isPagesWindow = (env) => env.module === "DOC" && env.editor?.dataset?.eoDocFlavor === "pages";
  const isWordWindow = (env) => env.module === "DOC" && env.editor?.dataset?.eoDocFlavor !== "pages";

  function register(cmd, handler) {
    handlers.set(cmd, handler);
  }

  function run(cmd, env) {
    const handler = handlers.get(cmd);
    if (!handler) return false;
    if (isWordWindow(env) || (isPagesWindow(env) && PAGES_COMMANDS.has(cmd))) handler(env);
    return true;
  }

  register("para-dialog", (env) => host.__EASYOFFICE_DOC_LINE_SPACING__.openParagraphDialog(env.document, env.runs));

  host.__EASYOFFICE_DOC_WORD_COMMANDS__ = { register, run, has: (cmd) => handlers.has(cmd) };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
