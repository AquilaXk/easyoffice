(function (host) {
  "use strict";

  // 서식 지우기 and 서식 복사 of the word processor window (the Word program's 모든 서식 지우기 and 서식 복사). Both work on
  // the character runs (`doc-runs.js`: `applyFormat` with a patch, `null` clears a property) and on the paragraph look
  // (alignment, indents, spacing and line spacing: the inline style and the exact twips the editor keeps next to it).
  // The format painter takes the format at the start of the selection (or the caret) and applies it to the next selection
  // the pointer makes; Esc or a second use of the command drops it.
  // Reuse (구현 원칙 3단계): (1) ProseMirror (MIT) copies marks and node attributes, but the editor has no schema, its
  // formats are the `eo-run` spans of `doc-runs.js`, so none of its code fits; (2) the existing `applyFormat` patch (`null`
  // clears) and `setStyle` do the character and style work; (3) only the paragraph look copy is new, and it is a plain read
  // and write of the inline style and twips data the paragraph buttons already keep.

  const PARA_STYLE = ["textAlign", "paddingLeft", "paddingRight", "textIndent", "marginTop", "marginBottom", "lineHeight"];
  const PARA_DATA = ["eoAlign", "eoLeftTwips", "eoRightTwips", "eoFirstLineTwips", "eoLineSpacing", "eoLineKind", "eoLineTwips", "eoSpaceBeforeTwips", "eoSpaceAfterTwips"];
  const MSG_NO_SELECTION = "서식을 적용할 글을 먼저 선택하세요";
  const R = () => host.__EASYOFFICE_DOC_RUNS__;

  const feedback = (env, text) => {
    const el = env.document.getElementById("status-command-feedback");
    if (el) el.textContent = text;
  };

  const isParagraph = (block) => /^(P|H[1-6])$/.test(String(block?.tagName || "").toUpperCase());

  // The paragraph look a block carries.
  function paragraphLook(block) {
    const style = {};
    for (const prop of PARA_STYLE) if (block.style?.[prop]) style[prop] = block.style[prop];
    const data = {};
    for (const key of PARA_DATA) if (block.dataset?.[key] !== undefined) data[key] = block.dataset[key];
    return { style, data };
  }

  // Replaces the paragraph look of `block` by `look` (empty: no look). True when something changed.
  function setParagraphLook(block, look) {
    const before = JSON.stringify(paragraphLook(block));
    for (const prop of PARA_STYLE) block.style[prop] = look.style[prop] ?? "";
    for (const key of PARA_DATA) {
      if (look.data[key] === undefined) delete block.dataset[key];
      else block.dataset[key] = look.data[key];
    }
    if (block.getAttribute?.("style") === "") block.removeAttribute("style");
    return JSON.stringify(paragraphLook(block)) !== before;
  }

  // ---- 서식 지우기

  const CLEAR_PATCH = () => Object.fromEntries(R().PROPS.map((prop) => [prop, null]));

  // The whole text of the block the caret is in, for a caret without a selection.
  const blockRange = (block) => ({ startContainer: block, startOffset: 0, endContainer: block, endOffset: block.childNodes.length, collapsed: false });

  function clearFormatting(env) {
    const { runs, editor } = env;
    const range = runs.currentRange();
    if (!range) return feedback(env, MSG_NO_SELECTION);
    const first = R().blocksInRange(editor, range)[0];
    const target = range.collapsed && first ? blockRange(first) : range;
    runs.withUndo(() => {
      let changed = runs.applyPatch(CLEAR_PATCH(), { range: target, undo: false }).changed;
      // A heading becomes the body style, as the Word program's 모든 서식 지우기 sets the style to 표준.
      if (runs.setStyle("표준", { undo: false }).changed) changed = true;
      const live = runs.currentRange() ?? range;
      for (const block of R().blocksInRange(editor, live).filter(isParagraph)) {
        if (setParagraphLook(block, { style: {}, data: {} })) changed = true;
      }
      return { changed };
    });
    feedback(env, "서식 지우기 완료");
  }

  // ---- 서식 복사

  let copied = null;
  let armed = null;

  // The character patch of the selection start: a property the text has stays, one it lacks is cleared on the target.
  function characterPatch(editor, range, runs) {
    const point = { startContainer: range.startContainer, startOffset: range.startOffset, endContainer: range.startContainer, endOffset: range.startOffset, collapsed: true };
    const state = R().queryFormat(editor, point, { baseHalfPt: R().DEFAULT_HALF_PT });
    const patch = {};
    for (const prop of R().PROPS) {
      const value = state?.[prop];
      if (value === R().MIXED) continue;
      if (R().BOOL_PROP_SET.has(prop)) patch[prop] = value === true ? true : null;
      else patch[prop] = value === "" || value === undefined || value === 0 ? null : value;
    }
    // A size equal to the default is no run size at all.
    if (patch.size === R().DEFAULT_HALF_PT) patch.size = null;
    return patch;
  }

  function copyFormat(env) {
    const { runs, editor } = env;
    const range = runs.currentRange();
    if (!range) return false;
    const block = R().blocksInRange(editor, range)[0];
    copied = { character: characterPatch(editor, range, runs), paragraph: block ? paragraphLook(block) : { style: {}, data: {} } };
    return true;
  }

  function pasteFormat(env) {
    const { runs, editor } = env;
    const range = runs.currentRange();
    if (!copied || !range) return feedback(env, MSG_NO_SELECTION);
    const target = range.collapsed ? null : range;
    runs.withUndo(() => {
      let changed = target ? runs.applyPatch(copied.character, { range: target, undo: false }).changed : false;
      for (const block of R().blocksInRange(editor, range).filter(isParagraph)) {
        if (setParagraphLook(block, copied.paragraph)) changed = true;
      }
      return { changed };
    });
    feedback(env, "서식 복사 적용 완료");
  }

  function disarm() {
    if (!armed) return;
    armed.editor.removeEventListener("mouseup", armed.onMouseUp);
    armed.editor.removeEventListener("keydown", armed.onKey);
    armed = null;
  }

  // 서식 복사: copies the format and waits for the next selection (or Esc).
  function painter(env) {
    if (armed) {
      disarm();
      return feedback(env, "서식 복사를 취소했습니다");
    }
    if (!copyFormat(env)) return feedback(env, "서식을 복사할 글을 먼저 선택하세요");
    const onMouseUp = () => {
      const range = env.runs.currentRange();
      if (!range || range.collapsed) return;
      disarm();
      pasteFormat(env);
    };
    const onKey = (ev) => {
      if (ev.key === "Escape") {
        disarm();
        feedback(env, "서식 복사를 취소했습니다");
      }
    };
    armed = { editor: env.editor, onMouseUp, onKey };
    env.editor.addEventListener("mouseup", onMouseUp);
    env.editor.addEventListener("keydown", onKey);
    feedback(env, "서식을 적용할 글을 선택하세요");
  }

  // 서식 붙여넣기 (Ctrl+Shift+V): the copied format on the selection now.
  function pasteCopied(env) {
    disarm();
    pasteFormat(env);
  }

  // Ctrl+Shift+C (⌘⇧C) copies the format and Ctrl+Shift+V (⌘⇧V) pastes it, as in the Word program. They run the menu
  // items of the same commands, so the word processor window (where the items are shown) is the only place they act.
  const SHORTCUT_COMMANDS = { c: "format-painter", v: "format-paste" };
  function onShortcut(ev) {
    if (!(ev.ctrlKey || ev.metaKey) || !ev.shiftKey || ev.altKey) return;
    const cmd = SHORTCUT_COMMANDS[String(ev.key ?? "").toLowerCase()];
    const document = host.document;
    if (!cmd || !document?.getElementById("doc-editor")?.contains?.(ev.target)) return;
    const item = document.querySelector(`.md-item[data-studio-menu-cmd="${cmd}"]`);
    if (!item || item.hidden || item.closest?.("[hidden]")) return;
    ev.preventDefault?.();
    item.click();
  }
  host.document?.addEventListener?.("keydown", onShortcut);

  const commands = host.__EASYOFFICE_DOC_WORD_COMMANDS__;
  commands?.register("format-clear", clearFormatting);
  commands?.register("format-painter", painter);
  commands?.register("format-paste", pasteCopied);

  host.__EASYOFFICE_DOC_FORMAT_TOOLS__ = { clearFormatting, painter, copyFormat, pasteFormat, paragraphLook, isArmed: () => armed !== null };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
