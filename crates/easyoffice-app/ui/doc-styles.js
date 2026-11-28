(function (host) {
  "use strict";

  // 서식 > 스타일…: the style pane of the word processor window (PD-401): the built-in 표준 and 제목 1~6, and the styles the user
  // makes with 새 스타일… and changes with 수정…. A style of the user is a name with a look (font, size, bold, italic, underline,
  // color, alignment, indents, spacing). Applying it puts the look on the paragraphs as direct formatting and names the style
  // in `data-eo-style`; changing it puts the new look on every paragraph that names it. The definitions are the document's
  // `eoWordMeta.styles` (`save-payload-doc-meta.js`), saved as `w:style` elements with `w:customStyle` (the paragraph style the
  // DOCX writer library `docx` writes, `src/file/styles/style/paragraph-style.ts`) and read back from them. Reuse (구현 원칙
  // 3단계): (1) no library does this on the editor's DOM; (2) the dialog kit, the undo wrapper, the run formatter
  // (`doc-runs.js` `applyFormat`) and the paragraph writers (`doc-table-runs.js`) are used; (3) the list and the form are new.

  const TWIPS_PER_MM = 1440 / 25.4;
  const TWIPS_PER_PX = 15;
  const BUILT_IN = ["표준", "제목 1", "제목 2", "제목 3", "제목 4", "제목 5", "제목 6"];
  const MAX_STYLES = 64;
  const meta = () => host.__EASYOFFICE_SAVE_DOC_META__;
  const feedback = (env, text) => {
    const el = env.document.getElementById("status-command-feedback");
    if (el) el.textContent = text;
  };

  const stylesOf = (editor) => meta().readWordMeta(editor.dataset.eoWordMeta).styles ?? [];

  function storeStyles(env, styles) {
    const root = env.editor;
    const settings = meta().readWordMeta(root.dataset.eoWordMeta);
    if (styles.length > 0) settings.styles = styles;
    else delete settings.styles;
    const packed = meta().packWordMeta(settings);
    if (packed) root.dataset.eoWordMeta = packed;
    else delete root.dataset.eoWordMeta;
  }

  // Puts the look of `def` on a paragraph and names the style on it.
  function look(env, block, def) {
    const helper = host.__EASYOFFICE_DOC_TABLE_RUNS__;
    const R = host.__EASYOFFICE_DOC_RUNS__;
    block.dataset.eoStyle = def.name;
    if (def.align) block.style.textAlign = def.align;
    if (def.leftIndentTwips !== undefined) {
      block.style.paddingLeft = Math.round(def.leftIndentTwips / TWIPS_PER_PX) + "px";
      block.dataset.eoLeftTwips = String(def.leftIndentTwips);
    }
    if (def.firstLineIndentTwips !== undefined) {
      block.style.textIndent = Math.round(def.firstLineIndentTwips / TWIPS_PER_PX) + "px";
      block.dataset.eoFirstLineTwips = String(def.firstLineIndentTwips);
    }
    const lines = host.__EASYOFFICE_DOC_LINE_SPACING__;
    const rule = def.lineSpacingKind ? lines?.fromJson?.(def) : null;
    if (rule) lines.applyToSpec({ style: block.style, data: block.dataset }, rule);
    else if (def.lineSpacingPercent !== undefined) {
      block.style.lineHeight = String(def.lineSpacingPercent / 100);
      block.dataset.eoLineSpacing = String(def.lineSpacingPercent);
    }
    if (def.spaceBeforeTwips !== undefined) helper?.writeSpacingPt?.(block, "before", def.spaceBeforeTwips / 20);
    if (def.spaceAfterTwips !== undefined) helper?.writeSpacingPt?.(block, "after", def.spaceAfterTwips / 20);
    const patch = {};
    if (def.font) patch.font = def.font;
    if (def.sizeHalfPt) patch.size = def.sizeHalfPt;
    if (def.bold) patch.bold = true;
    if (def.italic) patch.italic = true;
    if (def.underline) patch.underline = true;
    if (def.color) patch.color = "#" + def.color;
    if (Object.keys(patch).length > 0 && block.textContent !== "") {
      const range = env.document.createRange();
      range.setStart(block, 0);
      range.setEnd(block, block.childNodes.length);
      R.applyFormat(env.editor, range, patch);
    }
  }

  function apply(env, name, targets) {
    const def = stylesOf(env.editor).find((s) => s.name === name);
    if (BUILT_IN.includes(name)) return env.runs.setStyle(name).changed;
    if (!def || targets.length === 0) return false;
    env.runs.withUndo(() => {
      for (const b of targets) look(env, b, def);
      return { changed: true };
    });
    return true;
  }

  const FIELDS = [
    ["name", "doc-st-name"], ["font", "doc-st-font"], ["size", "doc-st-size"], ["color", "doc-st-color"], ["align", "doc-st-align"],
    ["left", "doc-st-left"], ["first", "doc-st-first"], ["before", "doc-st-before"], ["after", "doc-st-after"], ["line", "doc-st-line"],
  ];

  // The form's values as a style, or the sentence that says what is wrong.
  function readForm(kit) {
    const num = (id) => (kit.val(id).trim() === "" ? undefined : Number(kit.val(id)));
    const style = { name: kit.val("doc-st-name").trim() };
    if (!style.name) return { problem: "스타일 이름을 입력하세요" };
    if (host.__EASYOFFICE_DOC_WORD_BLOCKS__.isReservedStyleName(style.name)) return { problem: "기본 스타일과 같은 이름은 쓸 수 없습니다" };
    const font = kit.val("doc-st-font").trim();
    if (font) style.font = font;
    const size = num("doc-st-size");
    if (size !== undefined) {
      if (!(size >= 1 && size <= 819)) return { problem: "글자 크기는 1pt에서 819pt 사이여야 합니다" };
      style.sizeHalfPt = Math.round(size * 2);
    }
    for (const [key, id] of [["bold", "doc-st-bold"], ["italic", "doc-st-italic"], ["underline", "doc-st-underline"]]) if (kit.checked(id)) style[key] = true;
    if (kit.checked("doc-st-color-on")) style.color = kit.val("doc-st-color").replace("#", "").toUpperCase();
    if (kit.val("doc-st-align")) style.align = kit.val("doc-st-align");
    const left = num("doc-st-left");
    const first = num("doc-st-first");
    const before = num("doc-st-before");
    const after = num("doc-st-after");
    const line = num("doc-st-line");
    if (left !== undefined) style.leftIndentTwips = Math.round(left * TWIPS_PER_MM);
    if (first !== undefined) style.firstLineIndentTwips = Math.round(first * TWIPS_PER_MM);
    if (before !== undefined) style.spaceBeforeTwips = Math.round(before * 20);
    if (after !== undefined) style.spaceAfterTwips = Math.round(after * 20);
    if (line !== undefined) style.lineSpacingPercent = Math.round(line);
    const clean = meta().readWordMeta(JSON.stringify({ styles: [style] })).styles?.[0];
    // A value the check dropped (out of range, a color that is none) is a mistake of the form.
    if (!clean || !Object.keys(style).every((k) => clean[k] === style[k])) return { problem: "스타일 값이 범위를 벗어났습니다" };
    return { style: clean };
  }

  function showForm(kit, def) {
    kit.setVal("doc-st-name", def?.name ?? "");
    kit.setVal("doc-st-font", def?.font ?? "");
    kit.setVal("doc-st-size", def?.sizeHalfPt ? String(def.sizeHalfPt / 2) : "");
    kit.setChecked("doc-st-bold", def?.bold === true);
    kit.setChecked("doc-st-italic", def?.italic === true);
    kit.setChecked("doc-st-underline", def?.underline === true);
    kit.setChecked("doc-st-color-on", Boolean(def?.color));
    kit.setVal("doc-st-color", "#" + (def?.color ?? "000000").toLowerCase());
    kit.setSelect("doc-st-align", def?.align ?? "");
    kit.setVal("doc-st-left", def?.leftIndentTwips !== undefined ? (def.leftIndentTwips / TWIPS_PER_MM).toFixed(1) : "");
    kit.setVal("doc-st-first", def?.firstLineIndentTwips !== undefined ? (def.firstLineIndentTwips / TWIPS_PER_MM).toFixed(1) : "");
    kit.setVal("doc-st-before", def?.spaceBeforeTwips !== undefined ? String(def.spaceBeforeTwips / 20) : "");
    kit.setVal("doc-st-after", def?.spaceAfterTwips !== undefined ? String(def.spaceAfterTwips / 20) : "");
    kit.setVal("doc-st-line", def?.lineSpacingPercent !== undefined ? String(def.lineSpacingPercent) : "");
    kit.note("doc-st-status", "");
  }

  function install(env0) {
    const document = env0.document;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const byId = (id) => document.getElementById(id);
    let env = env0;
    let editing = null;
    let targets = [];

    const names = () => [...BUILT_IN, ...stylesOf(env.editor).map((s) => s.name)];
    const showList = (chosen) => {
      byId("doc-styles-list")?.replaceChildren(
        ...names().map((n) => {
          const option = document.createElement("option");
          option.value = n;
          option.textContent = n;
          return option;
        }),
      );
      if (chosen) kit.setSelect("doc-styles-list", chosen);
    };
    const selected = () => kit.val("doc-styles-list");
    const paragraphs = () => {
      const range = env.runs.currentRange();
      return range ? host.__EASYOFFICE_DOC_RUNS__.blocksInRange(env.editor, range).filter((b) => /^(P|LI)$/.test(b.tagName)) : [];
    };
    const openForm = (def) => {
      editing = def ?? null;
      byId("doc-st-title").textContent = def ? "스타일 수정" : "서식에서 새 스타일 만들기";
      byId("doc-st-name").readOnly = Boolean(def);
      showForm(kit, def);
      kit.openDialog("doc-style-edit-dialog");
    };

    byId("btn-doc-styles-apply")?.addEventListener("click", () => {
      const name = selected();
      if (apply(env, name, targets)) {
        kit.closeDialog("doc-styles-dialog");
        feedback(env, "스타일 적용 완료: " + name);
      } else {
        kit.note("doc-styles-status", targets.length === 0 ? "스타일을 적용할 문단 안에 커서를 놓으세요" : "적용할 수 없는 스타일입니다");
      }
    });
    byId("btn-doc-styles-new")?.addEventListener("click", () => {
      if (stylesOf(env.editor).length >= MAX_STYLES) return kit.note("doc-styles-status", "스타일은 문서에 " + MAX_STYLES + "개까지 만들 수 있습니다");
      openForm(null);
    });
    byId("btn-doc-styles-edit")?.addEventListener("click", () => {
      const def = stylesOf(env.editor).find((s) => s.name === selected());
      if (!def) return kit.note("doc-styles-status", "만든 스타일만 수정할 수 있습니다");
      openForm(def);
    });
    byId("btn-doc-styles-delete")?.addEventListener("click", () => {
      const name = selected();
      if (!stylesOf(env.editor).some((s) => s.name === name)) return kit.note("doc-styles-status", "만든 스타일만 삭제할 수 있습니다");
      env.runs.withUndo(() => {
        storeStyles(env, stylesOf(env.editor).filter((s) => s.name !== name));
        // The paragraphs keep the look they have and no longer name the style.
        for (const p of env.editor.querySelectorAll("[data-eo-style]")) if (p.dataset.eoStyle === name) delete p.dataset.eoStyle;
        return { changed: true };
      });
      showList();
      kit.note("doc-styles-status", "");
    });
    byId("btn-doc-st-ok")?.addEventListener("click", () => {
      const { style, problem } = readForm(kit);
      if (problem) return kit.note("doc-st-status", problem);
      const all = stylesOf(env.editor);
      if (!editing && all.some((s) => s.name === style.name)) return kit.note("doc-st-status", "같은 이름의 스타일이 있습니다");
      env.runs.withUndo(() => {
        storeStyles(env, editing ? all.map((s) => (s.name === style.name ? style : s)) : [...all, style]);
        // A changed style puts its new look on the paragraphs that name it.
        if (editing) for (const p of env.editor.querySelectorAll("[data-eo-style]")) if (p.dataset.eoStyle === style.name) look(env, p, style);
        return { changed: true };
      });
      kit.closeDialog("doc-style-edit-dialog");
      showList(style.name);
      feedback(env, editing ? "스타일 수정 완료: " + style.name : "새 스타일 만들기 완료: " + style.name);
    });

    host.__EASYOFFICE_DOC_WORD_COMMANDS__.register("format-styles", (e) => {
      env = e;
      targets = paragraphs();
      showList(targets[0]?.dataset?.eoStyle ?? e.runs.styleNameOf?.(targets[0]) ?? "표준");
      kit.note("doc-styles-status", "");
      kit.openDialog("doc-styles-dialog");
    });
  }

  const commands = host.__EASYOFFICE_DOC_WORD_COMMANDS__;
  let installed = false;
  commands?.register("format-styles", (env) => {
    if (!installed) {
      installed = true;
      install(env);
    }
    commands.run("format-styles", env);
  });
  host.__EASYOFFICE_DOC_STYLES__ = { BUILT_IN, stylesOf, FIELDS };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
