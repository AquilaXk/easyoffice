(function (host) {
  "use strict";

  // 서식 > 테두리 및 음영…: the border and the shading of a paragraph (PD-401). The dialog is the one the word processor has
  // (the 테두리 and 음영 tabs, 설정 없음/상자, 스타일, 색, 너비, 미리 보기 with the four side buttons) in the markup of the table
  // cell border dialog (`#doc-cell-border-dialog`, `doc-table-style.js`), whose line list and swatch classes it uses. The
  // paragraph shows the border as inline border styles, as the DOCX renderer docx-preview does (`parseBorderProperties`, `src/document-parser.ts`
  // lines 1536 and on, and its `pBdr` case at 1304, commit 28682e30: the `w:pBdr` sides become CSS borders of the paragraph), and keeps the values in `data-eo-border` and `data-eo-shade`
  // (`doc-word-blocks.js`). Reuse (구현 원칙 3단계): (1) no library does this on the editor's DOM; (2) the dialog kit, the
  // undo wrapper, the swatch classes and the border model of PD-401 are used; (3) the preview is a box with the same CSS.

  const WIDTHS = [2, 4, 6, 8, 12, 18, 24, 36, 48];
  const SIDES = ["top", "bottom", "left", "right"];
  const blocks = () => host.__EASYOFFICE_DOC_WORD_BLOCKS__;
  const feedback = (env, text) => {
    const el = env.document.getElementById("status-command-feedback");
    if (el) el.textContent = text;
  };

  function install(env0) {
    const document = env0.document;
    const kit = host.__EASYOFFICE_CREATE_SHEET_DIALOG_KIT__(document);
    const byId = (id) => document.getElementById(id);
    let env = env0;
    let targets = [];
    let sides = new Set();
    let style = "single";
    let eighths = 4;
    let color = "000000";
    let shade = "";

    const lineOf = () => ({ style, eighths, color: color === "000000" ? "" : color, space: Number(kit.val("doc-pb-space")) || 1 });
    function paint() {
      const preview = byId("doc-pb-preview");
      if (preview) {
        const line = lineOf();
        for (const side of SIDES) preview.style["border" + side[0].toUpperCase() + side.slice(1)] = sides.has(side) ? blocks().borderCss(line) : "";
        preview.style.backgroundColor = shade ? "#" + shade : "";
      }
      const state = sides.size === 0 ? "none" : sides.size === SIDES.length ? "box" : "custom";
      for (const btn of document.querySelectorAll("#doc-para-border-dialog [data-pb-preset]")) btn.setAttribute("aria-pressed", String(btn.dataset.pbPreset === state));
      for (const btn of document.querySelectorAll("#doc-para-border-dialog [data-pb-side]")) btn.setAttribute("aria-pressed", String(sides.has(btn.dataset.pbSide)));
      for (const item of document.querySelectorAll("#doc-para-border-dialog [data-pb-style]")) item.setAttribute("aria-checked", String(item.dataset.pbStyle === style));
    }
    const showPane = (name) => {
      for (const tab of document.querySelectorAll("#doc-para-border-dialog [data-pb-tab]")) {
        const on = tab.dataset.pbTab === name;
        tab.classList.toggle("active", on);
        tab.setAttribute("aria-selected", String(on));
      }
      for (const pane of document.querySelectorAll("#doc-para-border-dialog [data-pb-pane]")) pane.hidden = pane.dataset.pbPane !== name;
    };

    function open() {
      const range = env.runs.currentRange();
      targets = range ? host.__EASYOFFICE_DOC_RUNS__.blocksInRange(env.editor, range).filter((b) => /^(P|H[1-6]|LI)$/.test(b.tagName)) : [];
      if (targets.length === 0) return feedback(env, "테두리를 넣을 문단 안에 커서를 놓으세요");
      const first = blocks().decodeBorders(targets[0].dataset.eoBorder);
      sides = new Set(SIDES.filter((s) => first[s]));
      const any = first[[...sides][0]];
      style = any?.style ?? "single";
      eighths = any?.eighths ?? 4;
      color = any?.color || "000000";
      shade = /^[0-9A-F]{6}$/.test(targets[0].dataset.eoShade ?? "") ? targets[0].dataset.eoShade : "";
      kit.setVal("doc-pb-color", "#" + color.toLowerCase());
      kit.setSelect("doc-pb-width", String(eighths));
      kit.setVal("doc-pb-space", String(any?.space ?? 1));
      kit.setChecked("doc-pb-shade-on", shade !== "");
      kit.setVal("doc-pb-shade", "#" + (shade || "FFFF00").toLowerCase());
      showPane("border");
      paint();
      kit.openDialog("doc-para-border-dialog");
    }

    function apply() {
      const space = Math.round(Number(kit.val("doc-pb-space")));
      if (!(space >= 0 && space <= 31)) return kit.note("doc-pb-status", "텍스트와의 간격은 0에서 31pt 사이여야 합니다");
      kit.note("doc-pb-status", "");
      const line = lineOf();
      const borders = Object.fromEntries(SIDES.filter((s) => sides.has(s)).map((s) => [s, { ...line, space }]));
      const fill = kit.checked("doc-pb-shade-on") ? kit.val("doc-pb-shade").replace("#", "").toUpperCase() : "";
      env.runs.withUndo(() => {
        for (const p of targets) {
          blocks().showLook(p.style, p.dataset, { borders, shade: fill, tabs: blocks().decodeTabs(p.dataset.eoTabs) });
        }
        return { changed: true };
      });
      kit.closeDialog("doc-para-border-dialog");
      feedback(env, Object.keys(borders).length > 0 || fill ? "테두리 및 음영 적용 완료" : "테두리 및 음영을 지웠습니다");
    }

    for (const tab of document.querySelectorAll("#doc-para-border-dialog [data-pb-tab]")) tab.addEventListener("click", () => showPane(tab.dataset.pbTab));
    for (const btn of document.querySelectorAll("#doc-para-border-dialog [data-pb-side]")) {
      btn.addEventListener("click", () => {
        sides[sides.has(btn.dataset.pbSide) ? "delete" : "add"](btn.dataset.pbSide);
        paint();
      });
    }
    for (const btn of document.querySelectorAll("#doc-para-border-dialog [data-pb-preset]")) {
      btn.addEventListener("click", () => {
        // 사용자 지정 names the state of the four side buttons; it changes nothing by itself.
        if (btn.dataset.pbPreset === "custom") return;
        sides = new Set(btn.dataset.pbPreset === "box" ? SIDES : []);
        paint();
      });
    }
    for (const item of document.querySelectorAll("#doc-para-border-dialog [data-pb-style]")) {
      item.addEventListener("click", () => {
        style = item.dataset.pbStyle;
        paint();
      });
    }
    byId("doc-pb-width")?.addEventListener("change", () => {
      eighths = Number(kit.val("doc-pb-width")) || 4;
      paint();
    });
    byId("doc-pb-color")?.addEventListener("input", () => {
      color = kit.val("doc-pb-color").replace("#", "").toUpperCase();
      paint();
    });
    byId("doc-pb-shade")?.addEventListener("input", () => {
      shade = kit.val("doc-pb-shade").replace("#", "").toUpperCase();
      kit.setChecked("doc-pb-shade-on", true);
      paint();
    });
    byId("doc-pb-shade-on")?.addEventListener("change", () => {
      shade = kit.checked("doc-pb-shade-on") ? kit.val("doc-pb-shade").replace("#", "").toUpperCase() : "";
      paint();
    });
    byId("btn-doc-pb-ok")?.addEventListener("click", apply);
    host.__EASYOFFICE_DOC_WORD_COMMANDS__.register("format-borders", (e) => ((env = e), open()));
    return open;
  }

  const commands = host.__EASYOFFICE_DOC_WORD_COMMANDS__;
  let installed = null;
  commands?.register("format-borders", (env) => {
    if (!installed) installed = install(env);
    commands.run("format-borders", env);
  });
  host.__EASYOFFICE_DOC_PARA_BORDER__ = { WIDTHS };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
