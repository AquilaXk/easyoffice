(function (host) {
  "use strict";

  // 삽입 > 드롭 캡 (없음, 텍스트에 넣기, 여백에 넣기, 드롭 캡 옵션…) of the word processor window (PD-401). As the word
  // processor does, the drop cap is a paragraph of its own that holds the first letter and the next paragraph wraps around it
  // (DOCX `w:framePr w:dropCap`). The editor floats that paragraph left, which is the rule of the DOCX renderer docx-preview
  // (`parseFrame` of `src/document-parser.ts`: `dropCap="drop"` is `float: left`); its size follows its line count in CSS
  // (`content.css`). Removing the drop cap puts the letter back in front of the next paragraph. Reuse (구현 원칙 3단계): (1) no
  // library does this on the editor's DOM; (2) the dialog kit and the undo wrapper are used; (3) the split and the merge are
  // plain DOM work.

  const DEFAULT_LINES = 3;
  const feedback = (env, text) => {
    const el = env.document.getElementById("status-command-feedback");
    if (el) el.textContent = text;
  };

  const capOf = (p) => /^(\d+),(drop|margin)$/.exec(p?.dataset?.eoDropcap ?? "");

  // The drop cap paragraph and the body paragraph for the paragraph at the caret (the caret may be in either).
  function pairAt(env) {
    const range = env.runs.currentRange();
    const block = range && host.__EASYOFFICE_DOC_RUNS__.blocksInRange(env.editor, range)[0];
    if (!block || block.parentNode !== env.editor || block.tagName !== "P") return null;
    const siblings = Array.from(env.editor.children);
    const at = siblings.indexOf(block);
    if (capOf(block)) return { cap: block, body: siblings[at + 1] ?? null };
    const before = siblings[at - 1];
    return capOf(before) ? { cap: before, body: block } : { cap: null, body: block };
  }

  // The first letter of a paragraph and the text node it is in (white space in front is left).
  function firstLetter(parent) {
    for (const node of Array.from(parent.childNodes)) {
      if (node.nodeType === 3) {
        const at = node.nodeValue.search(/\S/);
        if (at >= 0) return { node, at };
      } else if (node.nodeType === 1 && !node.classList.contains("eo-textbox") && !node.classList.contains("eo-math")) {
        const found = firstLetter(node);
        if (found) return found;
      }
    }
    return null;
  }

  function removeCap(env, cap, body) {
    const text = cap.textContent;
    if (body) {
      const first = firstLetter(body);
      if (first) first.node.nodeValue = text + first.node.nodeValue;
      else body.insertBefore(env.document.createTextNode(text), body.firstChild);
    }
    cap.remove();
  }

  function setCap(env, kind, lines) {
    const pair = pairAt(env);
    if (!pair?.body) return feedback(env, "드롭 캡을 넣을 문단 안에 커서를 놓으세요");
    const { cap, body } = pair;
    const want = kind === "none" ? "" : lines + "," + kind;
    if (cap && (!want || want !== cap.dataset.eoDropcap)) {
      if (want) {
        env.runs.withUndo(() => ((cap.dataset.eoDropcap = want), { changed: true }));
        return feedback(env, "드롭 캡 적용 완료");
      }
      env.runs.withUndo(() => (removeCap(env, cap, body), { changed: true }));
      return feedback(env, "드롭 캡을 지웠습니다");
    }
    if (!want || cap) return feedback(env, want ? "드롭 캡이 그대로입니다" : "드롭 캡이 없습니다");
    const first = firstLetter(body);
    if (!first) return feedback(env, "첫 글자가 없는 문단에는 드롭 캡을 넣을 수 없습니다");
    env.runs.withUndo(() => {
      const letter = String.fromCodePoint(first.node.nodeValue.codePointAt(first.at));
      first.node.nodeValue = first.node.nodeValue.slice(0, first.at) + first.node.nodeValue.slice(first.at + letter.length);
      const p = env.document.createElement("p");
      p.dataset.eoDropcap = want;
      p.appendChild(env.document.createTextNode(letter));
      body.before(p);
      if (body.textContent === "") body.appendChild(env.document.createElement("br"));
      return { changed: true };
    });
    feedback(env, "드롭 캡 적용 완료");
  }

  function install(env0) {
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(env0.document);
    let env = env0;
    const radio = () => Array.from(env.document.querySelectorAll("input[name=\"doc-dc-pos\"]")).find((input) => input.checked)?.value ?? "none";
    env0.document.getElementById("btn-doc-dc-ok")?.addEventListener("click", () => {
      const lines = Math.round(Number(kit.val("doc-dc-lines")));
      if (!(lines >= 2 && lines <= 10)) return kit.note("doc-dc-status", "줄 수는 2에서 10 사이여야 합니다");
      kit.closeDialog("doc-dropcap-dialog");
      setCap(env, radio(), lines);
    });
    const commands = host.__EASYOFFICE_DOC_WORD_COMMANDS__;
    commands.register("insert-dropcap-none", (e) => setCap(e, "none", DEFAULT_LINES));
    commands.register("insert-dropcap-drop", (e) => setCap(e, "drop", Number(capOf(pairAt(e)?.cap)?.[1]) || DEFAULT_LINES));
    commands.register("insert-dropcap-margin", (e) => setCap(e, "margin", Number(capOf(pairAt(e)?.cap)?.[1]) || DEFAULT_LINES));
    commands.register("insert-dropcap-options", (e) => {
      env = e;
      const found = capOf(pairAt(e)?.cap);
      for (const input of e.document.querySelectorAll("input[name=\"doc-dc-pos\"]")) input.checked = input.value === (found?.[2] ?? "none");
      kit.setVal("doc-dc-lines", found?.[1] ?? String(DEFAULT_LINES));
      kit.note("doc-dc-status", "");
      kit.openDialog("doc-dropcap-dialog");
    });
  }

  const COMMANDS = ["insert-dropcap-none", "insert-dropcap-drop", "insert-dropcap-margin", "insert-dropcap-options"];
  let installed = false;
  for (const cmd of COMMANDS) {
    host.__EASYOFFICE_DOC_WORD_COMMANDS__?.register(cmd, (env) => {
      if (!installed) {
        installed = true;
        install(env);
      }
      host.__EASYOFFICE_DOC_WORD_COMMANDS__.run(cmd, env);
    });
  }
  host.__EASYOFFICE_DOC_DROP_CAP__ = { capOf };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
