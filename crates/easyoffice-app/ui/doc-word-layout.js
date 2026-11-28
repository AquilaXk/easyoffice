(function (host) {
  "use strict";

  // 문서 속성, 줄 번호, 워터마크, 구역 나누기, 머리글 행 반복 and the conversion between text and a table of the word processor
  // window (PD-395). The settings of the document (properties, line numbers, watermark) are the editor root's `eoWordMeta`
  // (`save-payload-doc-meta.js`), so one undo step takes one dialog back and the workspace snapshots carry them. The text and
  // table conversion follows the Word program's 텍스트를 표로 변환 and 표를 텍스트로 변환 (separator: paragraph, comma, tab or one
  // character of the user's). Reuse (구현 원칙 3단계): (1) no library does this on the editor's DOM; (2) the table builder
  // of `doc-table.js`, the dialog kit and the undo wrapper are used; (3) the rest is plain DOM work.

  const TWIPS_PER_MM = 1440 / 25.4;
  const MAX_CONVERT_BLOCKS = 20000;
  const MAX_TABLE_COLUMNS = 63;
  const meta = () => host.__EASYOFFICE_SAVE_DOC_META__;
  const feedback = (env, text) => {
    const el = env.document.getElementById("status-command-feedback");
    if (el) el.textContent = text;
  };

  // The settings the editor holds now; `change(settings)` edits a copy and the editor keeps what comes back.
  function updateMeta(env, change) {
    const root = env.editor;
    const before = root.dataset.eoWordMeta || "";
    const settings = meta().readWordMeta(before);
    change(settings);
    const after = meta().packWordMeta(settings);
    if (after === before) return false;
    env.runs.withUndo(() => {
      if (after) root.dataset.eoWordMeta = after;
      else delete root.dataset.eoWordMeta;
      showWatermark(root, settings.watermark);
      return { changed: true };
    });
    return true;
  }

  function showWatermark(root, text) {
    if (text) root.dataset.eoWatermarkText = text;
    else delete root.dataset.eoWatermarkText;
  }

  const fileTitle = (document) => String(document.getElementById("status-doc-filename")?.textContent ?? "").split(" — ")[0].replace(/\.[^.]+$/, "").trim();

  // ---- 문서 속성

  function openProperties(env, kit) {
    const props = meta().readWordMeta(env.editor.dataset.eoWordMeta).props ?? {};
    const initial = props.title ?? fileTitle(env.document);
    for (const key of ["title", "subject", "author", "keywords", "comments"]) {
      kit.setVal("doc-prop-" + key, key === "title" ? initial : props[key] ?? "");
    }
    env.propertiesDefaultTitle = initial;
    kit.openDialog("doc-properties-dialog");
  }

  function applyProperties(env, kit) {
    const changed = updateMeta(env, (settings) => {
      const next = {};
      for (const key of ["title", "subject", "author", "keywords", "comments"]) next[key] = kit.val("doc-prop-" + key).trim();
      // The title the user did not change stays the file's name.
      if (next.title === fileTitle(env.document) && !settings.props?.title) next.title = "";
      settings.props = next;
    });
    kit.closeDialog("doc-properties-dialog");
    feedback(env, changed ? "문서 속성 적용 완료" : "문서 속성이 그대로입니다");
  }

  // ---- 줄 번호

  function openLineNumbers(env, kit) {
    const numbers = meta().readWordMeta(env.editor.dataset.eoWordMeta).lineNumbers;
    kit.setChecked("doc-ln-on", Boolean(numbers));
    kit.setVal("doc-ln-start", String(numbers?.start ?? 1));
    kit.setVal("doc-ln-by", String(numbers?.countBy ?? 1));
    kit.setVal("doc-ln-distance", ((numbers?.distance ?? 0) / TWIPS_PER_MM).toFixed(1));
    kit.setSelect("doc-ln-restart", numbers?.restart ?? "newPage");
    kit.openDialog("doc-linenumbers-dialog");
  }

  function applyLineNumbers(env, kit) {
    const on = kit.checked("doc-ln-on");
    const lineNumbers = {
      start: Math.round(Number(kit.val("doc-ln-start"))),
      countBy: Math.round(Number(kit.val("doc-ln-by"))),
      distance: Math.round(Number(kit.val("doc-ln-distance") || 0) * TWIPS_PER_MM),
      restart: kit.val("doc-ln-restart"),
    };
    const settingsOk = !on || Boolean(meta().readWordMeta(JSON.stringify({ lineNumbers })).lineNumbers);
    if (!settingsOk) return feedback(env, "줄 번호 값이 범위를 벗어났습니다");
    updateMeta(env, (settings) => {
      if (on) settings.lineNumbers = lineNumbers;
      else delete settings.lineNumbers;
    });
    kit.closeDialog("doc-linenumbers-dialog");
    feedback(env, on ? "줄 번호 적용 완료" : "줄 번호를 껐습니다");
  }

  // ---- 워터마크

  function openWatermark(env, kit) {
    kit.setVal("doc-wm-text", meta().readWordMeta(env.editor.dataset.eoWordMeta).watermark ?? "");
    kit.openDialog("doc-watermark-dialog");
  }

  function applyWatermark(env, kit, remove) {
    const text = remove ? "" : kit.val("doc-wm-text").trim();
    updateMeta(env, (settings) => {
      if (text) settings.watermark = text;
      else delete settings.watermark;
    });
    kit.closeDialog("doc-watermark-dialog");
    feedback(env, text ? "워터마크 적용 완료" : "워터마크를 지웠습니다");
  }

  // ---- 표: 머리글 행 반복, 텍스트와 표의 변환

  const tableAtCaret = (env) => {
    const node = env.runs.currentRange()?.startContainer;
    const element = node?.nodeType === 1 ? node : node?.parentNode;
    const table = element?.closest?.("table");
    return table && env.editor.contains(table) ? { table, element } : null;
  };

  // 머리글 행 반복: the rows from the top through the caret's row repeat on every page; a second use turns it off.
  function toggleHeaderRows(env) {
    const hit = tableAtCaret(env);
    if (!hit) return feedback(env, "머리글 행으로 반복할 표 안에 커서를 놓으세요");
    const row = hit.element.closest("tr");
    const rows = Array.from(hit.table.querySelectorAll("tr"));
    const count = rows.indexOf(row) + 1;
    const on = !hit.table.dataset.eoHeaderRows;
    env.runs.withUndo(() => {
      if (on) hit.table.dataset.eoHeaderRows = String(count);
      else delete hit.table.dataset.eoHeaderRows;
      return { changed: true };
    });
    feedback(env, on ? "머리글 행 반복 적용 완료" : "머리글 행 반복을 껐습니다");
  }

  const SEPARATORS = { comma: ",", tab: "\t" };

  const topBlocks = (env, range) =>
    host.__EASYOFFICE_DOC_RUNS__.blocksInRange(env.editor, range).filter((b) => /^(P|H[1-6])$/.test(b.tagName) && b.parentNode === env.editor).slice(0, MAX_CONVERT_BLOCKS);

  // The cells of the paragraphs `blocks` for a separator: a paragraph per cell, or a row per paragraph split at the separator.
  function gridOf(texts, sep, columns, other) {
    if (sep === "paragraph") {
      const cols = Math.max(1, Math.min(MAX_TABLE_COLUMNS, columns));
      const rows = [];
      for (let i = 0; i < texts.length; i += cols) rows.push(texts.slice(i, i + cols));
      return rows;
    }
    const char = sep === "other" ? other : SEPARATORS[sep];
    const rows = texts.map((text) => text.split(char).slice(0, MAX_TABLE_COLUMNS));
    return rows;
  }

  function textToTable(env, kit) {
    const range = env.runs.currentRange();
    const blocks = range ? topBlocks(env, range) : [];
    if (blocks.length === 0) return "표로 변환할 문단을 먼저 선택하세요";
    const sep = kit.val("doc-tc-sep");
    const other = kit.val("doc-tc-other");
    if (sep === "other" && !other) return "기타 구분 기호를 입력하세요";
    const grid = gridOf(blocks.map((b) => String(b.textContent ?? "").replace(/\n/g, " ")), sep, Math.round(Number(kit.val("doc-tc-cols"))) || 1, other);
    const columns = Math.max(...grid.map((r) => r.length));
    const table = host.__EASYOFFICE_DOC_TABLE__.buildTable(env.document, grid.length, columns);
    Array.from(table.querySelectorAll("tr")).forEach((tr, r) => {
      Array.from(tr.querySelectorAll("td")).forEach((td, c) => {
        if (grid[r][c]) td.replaceChildren(env.document.createTextNode(grid[r][c]));
      });
    });
    env.runs.withUndo(() => {
      env.editor.insertBefore(table, blocks[0]);
      for (const block of blocks) block.remove();
      return { changed: true };
    });
    return "";
  }

  function tableToText(env, kit) {
    const hit = tableAtCaret(env);
    if (!hit) return "텍스트로 변환할 표 안에 커서를 놓으세요";
    const sep = kit.val("doc-tc-sep");
    const char = sep === "other" ? kit.val("doc-tc-other") : SEPARATORS[sep];
    if (sep === "other" && !char) return "기타 구분 기호를 입력하세요";
    const paragraphs = [];
    for (const tr of Array.from(hit.table.querySelectorAll("tr"))) {
      const cells = Array.from(tr.querySelectorAll("td, th")).map((c) => String(c.textContent ?? "").trim());
      if (sep === "paragraph") paragraphs.push(...cells);
      else paragraphs.push(cells.join(char));
    }
    env.runs.withUndo(() => {
      const parent = hit.table.parentNode;
      for (const text of paragraphs) {
        const p = env.document.createElement("p");
        if (text) p.appendChild(env.document.createTextNode(text));
        else p.appendChild(env.document.createElement("br"));
        parent.insertBefore(p, hit.table);
      }
      hit.table.remove();
      return { changed: true };
    });
    return "";
  }

  // ---- the commands

  function install(env0) {
    const document = env0.document;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const byId = (id) => document.getElementById(id);
    let env = env0;
    let convert = "text-to-table";
    const syncSeparator = () => {
      const sep = kit.val("doc-tc-sep");
      byId("doc-tc-other-row").hidden = sep !== "other";
      byId("doc-tc-cols-row").hidden = convert !== "text-to-table" || sep !== "paragraph";
    };
    byId("btn-doc-prop-ok")?.addEventListener("click", () => applyProperties(env, kit));
    byId("btn-doc-ln-ok")?.addEventListener("click", () => applyLineNumbers(env, kit));
    byId("btn-doc-wm-ok")?.addEventListener("click", () => applyWatermark(env, kit, false));
    byId("btn-doc-wm-remove")?.addEventListener("click", () => applyWatermark(env, kit, true));
    byId("doc-tc-sep")?.addEventListener("change", syncSeparator);
    byId("btn-doc-tc-ok")?.addEventListener("click", () => {
      const problem = convert === "text-to-table" ? textToTable(env, kit) : tableToText(env, kit);
      kit.note("doc-tc-status", problem);
      if (!problem) kit.closeDialog("doc-table-convert-dialog");
    });

    const commands = host.__EASYOFFICE_DOC_WORD_COMMANDS__;
    const openConvert = (kind) => (e) => {
      env = e;
      convert = kind;
      byId("doc-tc-title").textContent = kind === "text-to-table" ? "텍스트를 표로 변환" : "표를 텍스트로 변환";
      kit.setVal("doc-tc-cols", "2");
      kit.setSelect("doc-tc-sep", kind === "text-to-table" ? "paragraph" : "tab");
      kit.setVal("doc-tc-other", "");
      kit.note("doc-tc-status", "");
      syncSeparator();
      kit.openDialog("doc-table-convert-dialog");
    };
    commands.register("doc-properties", (e) => ((env = e), openProperties(e, kit)));
    commands.register("layout-linenumbers", (e) => ((env = e), openLineNumbers(e, kit)));
    commands.register("layout-watermark", (e) => ((env = e), openWatermark(e, kit)));
    commands.register("table-header-repeat", toggleHeaderRows);
    commands.register("table-from-text", openConvert("text-to-table"));
    commands.register("table-to-text", openConvert("table-to-text"));
  }

  const installed = { done: false };
  const lazy = ["doc-properties", "layout-linenumbers", "layout-watermark", "table-header-repeat", "table-from-text", "table-to-text"];
  for (const cmd of lazy) {
    host.__EASYOFFICE_DOC_WORD_COMMANDS__?.register(cmd, (env) => {
      if (!installed.done) {
        installed.done = true;
        install(env);
      }
      host.__EASYOFFICE_DOC_WORD_COMMANDS__.run(cmd, env);
    });
  }

  // 구역 나누기: split the paragraph at the caret and end the section there.
  for (const kind of ["nextPage", "continuous", "evenPage", "oddPage"]) {
    host.__EASYOFFICE_DOC_WORD_COMMANDS__?.register("section-break-" + kind, (env) => {
      const pages = env.getPages?.() ?? host.__EASYOFFICE_DOC_PAGES__;
      if (!pages?.insertSectionBreak?.(kind)) feedback(env, "구역 나누기를 넣을 문단 안에 커서를 놓으세요");
      else feedback(env, "구역 나누기 삽입 완료");
    });
  }

  host.__EASYOFFICE_DOC_WORD_LAYOUT__ = { gridOf, showWatermark, SEPARATORS };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
