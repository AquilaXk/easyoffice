(function (host) {
  "use strict";

  // 레이아웃 > 단 (하나, 둘, 셋, 왼쪽, 오른쪽, 기타 단…) and 단 나누기 of the word processor window (PD-401). The editor shows the
  // columns the way the DOCX renderer docx-preview does (`section.columns` of its `html-renderer.ts`: CSS `column-count`,
  // `column-gap` and, with a separator, `column-rule`; the widths of unequal columns are not drawn), and the file keeps what the
  // CSS cannot show: the narrow side of the two-column presets, in `eoWordMeta.columns` (`save-payload-doc-meta.js`). A column
  // break is a paragraph's `data-eo-col-break`, drawn as `break-before: column`. Reuse (구현 원칙 3단계): (1) no library does
  // this on the editor's DOM; (2) the dialog kit, the undo wrapper and the `eoWordMeta` store are used; (3) nothing is ported
  // but docx-preview's three CSS properties.

  const TWIPS_PER_MM = 1440 / 25.4;
  const TWIPS_PER_PX = 15;
  const MAX_COLUMNS = 12;
  const MAX_GAP_TWIPS = 5000;
  const DEFAULT_GAP = 720;
  const RULE = "1px solid #000000";
  const PRESETS = {
    "layout-columns-1": { count: 1, narrow: "none" },
    "layout-columns-2": { count: 2, narrow: "none" },
    "layout-columns-3": { count: 3, narrow: "none" },
    "layout-columns-left": { count: 2, narrow: "first" },
    "layout-columns-right": { count: 2, narrow: "last" },
  };
  const meta = () => host.__EASYOFFICE_SAVE_DOC_META__;
  const feedback = (env, text) => {
    const el = env.document.getElementById("status-command-feedback");
    if (el) el.textContent = text;
  };

  const columnsOf = (editor) => meta().readWordMeta(editor.dataset.eoWordMeta).columns ?? { gap: DEFAULT_GAP, sep: false, narrow: "none" };

  // Draws the document's columns from the editor's `column-count` and the settings in `eoWordMeta` (also after an undo, a tab
  // switch or an open, which restore the count and the settings but not the CSS gap and rule).
  function sync(editor) {
    if (!editor) return;
    const count = Number.parseInt(editor.style.columnCount || "1", 10) || 1;
    const { gap, sep } = columnsOf(editor);
    editor.style.columnGap = count > 1 ? Math.round(gap / TWIPS_PER_PX) + "px" : "";
    editor.style.columnRule = count > 1 && sep ? RULE : "";
  }

  function setColumns(env, { count, gap, sep, narrow }) {
    const root = env.editor;
    const settings = meta().readWordMeta(root.dataset.eoWordMeta);
    const cols = { gap, sep, narrow: count === 2 ? narrow : "none" };
    const now = columnsOf(root);
    const same = Number.parseInt(root.style.columnCount || "1", 10) === count && now.gap === cols.gap && now.sep === cols.sep && now.narrow === cols.narrow;
    if (same && !root.querySelector("[data-eo-cols]")) return false;
    env.runs.withUndo(() => {
      settings.columns = cols;
      const packed = meta().packWordMeta(settings);
      if (packed) root.dataset.eoWordMeta = packed;
      else delete root.dataset.eoWordMeta;
      root.style.columnCount = count > 1 ? String(count) : "";
      // The columns the file gave to single paragraphs give way to the document's.
      for (const el of root.querySelectorAll("[data-eo-cols], [data-eo-cols-look]")) {
        delete el.dataset.eoCols;
        delete el.dataset.eoColLook;
      }
      sync(root);
      return { changed: true };
    });
    return true;
  }

  function openDialog(env, kit) {
    const root = env.editor;
    const count = Math.max(1, Number.parseInt(root.style.columnCount || "1", 10) || 1);
    const { gap, sep, narrow } = columnsOf(root);
    kit.setVal("doc-col-count", String(count));
    kit.setChecked("doc-col-sep", sep);
    kit.setVal("doc-col-gap", (gap / TWIPS_PER_MM).toFixed(1));
    kit.setSelect("doc-col-narrow", narrow);
    kit.note("doc-col-status", "");
    kit.openDialog("doc-columns-dialog");
  }

  function applyDialog(env, kit) {
    const count = Math.round(Number(kit.val("doc-col-count")));
    const gap = Math.round(Number(kit.val("doc-col-gap")) * TWIPS_PER_MM);
    if (!(count >= 1 && count <= MAX_COLUMNS)) return kit.note("doc-col-status", "단 개수는 1에서 " + MAX_COLUMNS + " 사이여야 합니다");
    if (!(gap >= 0 && gap <= MAX_GAP_TWIPS)) return kit.note("doc-col-status", "간격은 0에서 " + (MAX_GAP_TWIPS / TWIPS_PER_MM).toFixed(0) + "mm 사이여야 합니다");
    const narrow = count === 2 ? kit.val("doc-col-narrow") : "none";
    setColumns(env, { count, gap, sep: kit.checked("doc-col-sep"), narrow });
    kit.closeDialog("doc-columns-dialog");
    feedback(env, count > 1 ? "단 " + count + "개 적용 완료" : "한 단으로 되돌렸습니다");
  }

  // 단 나누기: the paragraph at the caret starts in the next column.
  function columnBreak(env) {
    const range = env.runs.currentRange();
    const block = range && host.__EASYOFFICE_DOC_RUNS__.blocksInRange(env.editor, range)[0];
    if (!block || block.parentNode !== env.editor) return feedback(env, "단 나누기를 넣을 문단 안에 커서를 놓으세요");
    const on = block.dataset.eoColBreak !== "1";
    env.runs.withUndo(() => {
      if (on) block.dataset.eoColBreak = "1";
      else delete block.dataset.eoColBreak;
      return { changed: true };
    });
    feedback(env, on ? "단 나누기 삽입 완료" : "단 나누기를 지웠습니다");
  }

  function install(env0) {
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(env0.document);
    let env = env0;
    env0.document.getElementById("btn-doc-col-ok")?.addEventListener("click", () => applyDialog(env, kit));
    const commands = host.__EASYOFFICE_DOC_WORD_COMMANDS__;
    for (const [cmd, preset] of Object.entries(PRESETS)) {
      commands.register(cmd, (e) => {
        env = e;
        const { gap, sep } = columnsOf(e.editor);
        if (setColumns(e, { count: preset.count, gap, sep: preset.count === 1 ? false : sep, narrow: preset.narrow })) {
          feedback(e, preset.count > 1 ? "단 " + preset.count + "개 적용 완료" : "한 단으로 되돌렸습니다");
        }
      });
    }
    // The ribbon's 단 command (`doc-effect` in `ribbon-history.js`) and the menu's 기타 단… open the dialog through `open`.
    commands.register("layout-columns-dialog", (e) => ((env = e), openDialog(e, kit)));
    commands.register("layout-column-break", columnBreak);
  }

  const installed = { done: false };
  for (const cmd of [...Object.keys(PRESETS), "layout-columns-dialog", "layout-column-break"]) {
    host.__EASYOFFICE_DOC_WORD_COMMANDS__?.register(cmd, (env) => {
      if (!installed.done) {
        installed.done = true;
        install(env);
      }
      host.__EASYOFFICE_DOC_WORD_COMMANDS__.run(cmd, env);
    });
  }

  // Opens the 단 dialog for `env` (`{ document, editor, runs }`): the ribbon command and the menu's 기타 단… call it.
  const open = (env) => host.__EASYOFFICE_DOC_WORD_COMMANDS__.run("layout-columns-dialog", { module: "DOC", ...env });
  host.__EASYOFFICE_DOC_WORD_COLUMNS__ = { sync, open, PRESETS };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
