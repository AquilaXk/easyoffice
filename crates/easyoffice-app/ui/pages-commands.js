(function (host) {
  "use strict";

  // The Pages document window's own commands and names (PD-399). The commands are thin calls on what the document editor
  // already has: 하이라이트 is the 형광펜 of the style bar, and the three 대문자 choices set the 작은 대문자 and 모두 대문자 bits
  // of the packed text effects that the font dialog sets (`doc-text-fx.js`). The date and time, the bookmark, the watermark
  // and the picture layout commands are the word processor's (`doc-word-*.js`); `doc-word-commands.js` lets this window run them.
  // What `.pages` cannot hold of them is counted by the save warnings (`doc_save_loss`, `pages_write_loss`).

  const CAPS = { none: {}, all: { allCaps: true }, small: { smallCaps: true } };

  function setCapitals(kind, env) {
    const fx = host.__EASYOFFICE_DOC_TEXT_FX__;
    if (!fx || !env.runs?.applyPatch) return;
    env.runs.applyPatch({ textfx: { mask: fx.maskOf(["smallcaps", "allcaps"]), value: fx.pack(CAPS[kind]) } });
  }

  const commands = host.__EASYOFFICE_DOC_WORD_COMMANDS__;
  commands?.register("home-highlight", (env) => env.runs?.toggleColor?.("highlight"));
  for (const kind of Object.keys(CAPS)) commands?.register("text-case-" + kind, (env) => setCapitals(kind, env));
  // A native menu action argument has no capital letters, so 위/아래 has a second name for the word processor's `image-wrap-topBottom`.
  commands?.register("image-wrap-top-bottom", (env) => commands.run("image-wrap-topBottom", env));

  // The names of the shared ribbon commands in the command search of this window, and the commands only this window has
  // (`id`: the native menu item that runs it; `keys`: the reference app's key).
  const PALETTE_TITLES = Object.freeze({
    "home-bold": "볼드체",
    "home-italic": "이탤릭체",
    "home-underline": "밑줄체",
    "insert-shape": "도형",
    "insert-link": "링크",
    "layout-paper": "페이지 설정",
    "review-comment": "주석 추가",
    "review-stats": "단어 수 보기",
  });
  const PALETTE_KEYS = Object.freeze({ "review-stats": "⇧⌘W" });
  const PALETTE_EXTRA = Object.freeze([
    { id: "page:break", title: "페이지 나누기", keys: "⌘Enter", section: "삽입", english: "page break" },
    { id: "insert:datetime", title: "날짜 및 시간…", keys: "", section: "삽입", english: "date time" },
    { id: "insert:pages-bookmark", title: "북마크…", keys: "⌥⌘B", section: "삽입", english: "bookmark" },
    { id: "format:font-dialog", title: "서체 윈도우", keys: "⌘T", section: "포맷", english: "font window" },
    { id: "format:highlight", title: "하이라이트", keys: "⇧⌘H", section: "포맷", english: "highlight" },
    { id: "format:case-none", title: "대문자: 없음", keys: "", section: "포맷", english: "capitalization none" },
    { id: "format:case-all", title: "대문자: 모두 대문자", keys: "", section: "포맷", english: "capitalization all caps" },
    { id: "format:case-small", title: "대문자: 작은 대문자", keys: "", section: "포맷", english: "capitalization small caps" },
    { id: "arrange:watermark", title: "워터마크…", keys: "", section: "정렬", english: "watermark" },
    { id: "tool:bench-run", title: "문서 엔진 성능 진단", keys: "", section: "도구", english: "benchmark" },
  ]);
  // The key bindings with `flavor: "pages"` (`ribbon-catalog.js`) by action name, each running the native menu item of the same command.
  const SHORTCUT_ITEMS = Object.freeze({
    "pages-font-window": "format:font-dialog",
    "pages-highlight": "format:highlight",
    "pages-bookmark": "insert:pages-bookmark",
    "pages-word-count": "tool:review-stats",
    "pages-marks": "view:para-marks",
  });

  // The command search entries only this window has; `run` runs a native menu item (`runNativeMenu` of app.js), `fmt` writes a key.
  const paletteItems = (fmt, run) =>
    PALETTE_EXTRA.map(({ id, title, keys, section, english }) => ({ id: "pages:" + id, module: "DOC", title, shortcut: fmt(keys), searchKey: section + " " + title + " " + english + " " + fmt(keys), run: () => run(id) }));
  const shortcutActions = (run) => Object.fromEntries(Object.entries(SHORTCUT_ITEMS).map(([action, id]) => [action, () => void run(id)]));

  host.__EASYOFFICE_PAGES_COMMANDS__ = Object.freeze({ PALETTE_TITLES, PALETTE_KEYS, PALETTE_EXTRA, SHORTCUT_ITEMS, paletteItems, shortcutActions });
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
