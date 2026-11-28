(function (host) {
  "use strict";

  // 삽입 > 텍스트 상자 of the word processor window (PD-401): a box with paragraphs of its own, anchored to the paragraph at
  // the caret and floated beside the text the way a picture is (PD-395: `square`, `topBottom`, `behind`, `front`). The wrap
  // commands of the picture menu (서식 > 그림 서식 > 정사각형 ...) apply to the text box that holds the caret, and fall back to
  // the picture when the caret is in none. The file keeps the box as `w:pict` with `v:textbox` (the shape the DOCX writer
  // library `docx` writes, `src/file/textbox/textbox.ts`) and reads `wps:txbx` too. The paragraphs inside are the editor's
  // paragraphs (`doc-word-blocks.js` reads them back); a box inside a box is refused. Reuse (구현 원칙 3단계): (1) no library
  // does this on the editor's DOM; (2) the undo wrapper, the picture wrap commands and their float classes are reused;
  // (3) the insertion is plain DOM work.

  const EMU_PER_PX = 9525;
  const DEFAULT_WIDTH_EMU = 225 * EMU_PER_PX;
  const DEFAULT_HEIGHT_EMU = 75 * EMU_PER_PX;
  const MAX_BOXES = 256;
  const WRAP_LABELS = { square: "정사각형", topBottom: "위/아래", behind: "텍스트 뒤", front: "텍스트 앞" };
  const feedback = (env, text) => {
    const el = env.document.getElementById("status-command-feedback");
    if (el) el.textContent = text;
  };

  const boxAtCaret = (env) => {
    const node = env.runs.currentRange()?.startContainer;
    const box = (node?.nodeType === 1 ? node : node?.parentNode)?.closest?.(".eo-textbox");
    return box && env.editor.contains(box) ? box : null;
  };

  function insertBox(env) {
    const range = env.runs.currentRange();
    const block = range && host.__EASYOFFICE_DOC_RUNS__.blocksInRange(env.editor, range)[0];
    if (!block || !/^(P|H[1-6])$/.test(block.tagName) || block.parentNode !== env.editor) return feedback(env, "텍스트 상자를 넣을 문단 안에 커서를 놓으세요");
    if (env.editor.querySelectorAll(".eo-textbox").length >= MAX_BOXES) return feedback(env, "텍스트 상자는 문서에 " + MAX_BOXES + "개까지 넣을 수 있습니다");
    const doc = env.document;
    const box = doc.createElement("div");
    box.className = "eo-textbox";
    box.setAttribute("contenteditable", "true");
    Object.assign(box.dataset, { eoTextbox: "1", eoTbWrap: "square", eoTbW: String(DEFAULT_WIDTH_EMU), eoTbH: String(DEFAULT_HEIGHT_EMU), eoTbBorder: "1" });
    box.style.width = DEFAULT_WIDTH_EMU / EMU_PER_PX + "px";
    box.style.height = DEFAULT_HEIGHT_EMU / EMU_PER_PX + "px";
    const inner = doc.createElement("p");
    inner.appendChild(doc.createElement("br"));
    box.appendChild(inner);
    env.runs.withUndo(() => {
      // The box goes in at the caret: the text node is cut there, or the box goes in front of the child at the offset.
      const { startContainer: at, startOffset: offset } = range;
      if (at.nodeType === 3) at.parentNode.insertBefore(box, offset > 0 ? at.splitText(offset) : at);
      else at.insertBefore(box, at.childNodes[offset] ?? null);
      return { changed: true };
    });
    const caret = doc.createRange();
    caret.setStart(inner, 0);
    caret.setEnd(inner, 0);
    const sel = host.getSelection?.();
    sel?.removeAllRanges();
    sel?.addRange(caret);
    feedback(env, "텍스트 상자 삽입 완료");
  }

  const commands = host.__EASYOFFICE_DOC_WORD_COMMANDS__;
  commands?.register("insert-textbox", insertBox);

  // The wrap commands: a text box that holds the caret takes the wrap, any other caret leaves it to the picture.
  for (const wrap of ["inline", "square", "topBottom", "behind", "front"]) {
    const cmd = "image-wrap-" + wrap;
    const picture = commands?.get?.(cmd);
    commands?.register(cmd, (env) => {
      const box = boxAtCaret(env);
      if (!box) return picture?.(env);
      if (wrap === "inline") return feedback(env, "텍스트 상자는 줄 안에 둘 수 없습니다");
      env.runs.withUndo(() => ((box.dataset.eoTbWrap = wrap), { changed: true }));
      feedback(env, "텍스트 줄 바꿈: " + WRAP_LABELS[wrap]);
    });
  }
  host.__EASYOFFICE_DOC_TEXT_BOX__ = { boxAtCaret, DEFAULT_WIDTH_EMU, DEFAULT_HEIGHT_EMU };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
