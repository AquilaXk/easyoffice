// 참조 구현 (reference implementation): openpyxl 3.1.5 (https://foss.heptapod.net/openpyxl/openpyxl, MIT, Copyright (c) 2010 openpyxl),
// `openpyxl/styles/builtins.py` lines 9-1322 (the 47 built-in cell styles "as defined in Part 4 Annex G.2" of ECMA-376 with their
// `builtinId`, the ids of ECMA-376 Part 1 18.8.7): the number format code, the font (bold, italic, size, color), the fill and the
// four borders of each style. Changes: the theme colors (`theme="3"`, `"4"` .. `"9"` and the `tint` of the accents) are written as the
// RGB of the Office theme that those indexes give (text 2 `44546A`, accents `4472C4 ED7D31 A5A5A5 FFC000 5B9BD5 70AD47`, the tint
// as the standard's `tint` rule: the color moves toward white by that fraction), the font size is left to the workbook's default
// font unless the style sets it (title, headings), the border kind `medium` is the sheet's `thick`, and the Korean names are the
// program's. See THIRD_PARTY_NOTICES.md (PD-400).
(function (host) {
  "use strict";

  // 서식 > 셀 스타일 of the sheet window (PD-400): the official default list of cell styles, applied to the selection as the cell format
  // they stand for (number format, font, fill, borders: the parts a style has replace those of the cell, the others stay). A style
  // is not kept by name in the file: the XLSX writer writes the formats of the cells (see PD-400: 새 셀 스타일 and `cellStyles` are open).
  const ACCENTS = ["4472C4", "ED7D31", "A5A5A5", "FFC000", "5B9BD5", "70AD47"];
  const mix = (rgb, tint) => {
    const part = (i) => Math.round(parseInt(rgb.slice(i, i + 2), 16) + (255 - parseInt(rgb.slice(i, i + 2), 16)) * tint);
    return [0, 2, 4].map((i) => part(i).toString(16).padStart(2, "0")).join("");
  };
  const TEXT2 = "44546A";
  const edge = (line, color) => line + ":" + color.toLowerCase();
  const all = (line, color) => ({ t: edge(line, color), r: edge(line, color), b: edge(line, color), l: edge(line, color) });

  // `name`, `id` (builtinId), then the parts: `num` (format code), `bold`, `italic`, `pt` (font size), `color`, `fill`, `border`.
  const NORMAL = { name: "표준", id: 0 };
  const GOOD_BAD = [
    { name: "좋음", id: 26, color: "006100", fill: "C6EFCE" },
    { name: "나쁨", id: 27, color: "9C0006", fill: "FFC7CE" },
    { name: "보통", id: 28, color: "9C6500", fill: "FFEB9C" },
  ];
  const DATA = [
    { name: "계산", id: 22, bold: true, color: "FA7D00", fill: "F2F2F2", border: all("thin", "7F7F7F") },
    { name: "메모", id: 10, fill: "FFFFCC", border: all("thin", "B2B2B2") },
    { name: "설명 텍스트", id: 53, italic: true, color: "7F7F7F" },
    { name: "연결된 셀", id: 24, color: "FA7D00", border: { b: edge("double", "FF8001") } },
    { name: "입력", id: 20, color: "3F3F76", fill: "FFCC99", border: all("thin", "7F7F7F") },
    { name: "출력", id: 21, bold: true, color: "3F3F3F", fill: "F2F2F2", border: all("thin", "3F3F3F") },
    { name: "확인 셀", id: 23, bold: true, color: "FFFFFF", fill: "A5A5A5", border: all("double", "3F3F3F") },
    { name: "경고 텍스트", id: 11, color: "FF0000" },
  ];
  const HEADINGS = [
    { name: "제목", id: 15, bold: true, pt: 18, color: TEXT2 },
    { name: "제목 1", id: 16, bold: true, pt: 15, color: TEXT2, border: { b: edge("thick", ACCENTS[0]) } },
    { name: "제목 2", id: 17, bold: true, pt: 13, color: TEXT2, border: { b: edge("thick", mix(ACCENTS[0], 0.5)) } },
    { name: "제목 3", id: 18, bold: true, pt: 11, color: TEXT2, border: { b: edge("thick", mix(ACCENTS[0], 0.4)) } },
    { name: "제목 4", id: 19, bold: true, pt: 11, color: TEXT2 },
    { name: "요약", id: 25, bold: true, border: { t: edge("thin", ACCENTS[0]), b: edge("double", ACCENTS[0]) } },
  ];
  // The accent styles: the solid accent with white text, and the 60%, 40% and 20% tints (the first id of an accent is 29 + 4 n).
  const THEMED = ACCENTS.flatMap((accent, n) => [
    { name: "20% - 강조색" + (n + 1), id: 30 + 4 * n, fill: mix(accent, 0.8) },
    { name: "40% - 강조색" + (n + 1), id: 31 + 4 * n, fill: mix(accent, 0.6) },
    { name: "60% - 강조색" + (n + 1), id: 32 + 4 * n, fill: mix(accent, 0.4), color: "FFFFFF" },
    { name: "강조색" + (n + 1), id: 29 + 4 * n, fill: accent, color: "FFFFFF" },
  ]);
  const NUMBERS = [
    { name: "백분율", id: 5, num: "0%" },
    { name: "쉼표", id: 3, num: '_-* #,##0.00_-;\\-* #,##0.00_-;_-* "-"??_-;_-@_-' },
    { name: "쉼표 [0]", id: 6, num: '_-* #,##0_-;\\-* #,##0_-;_-* "-"_-;_-@_-' },
    { name: "통화", id: 4, num: '_-"₩"* #,##0.00_-;\\-"₩"* #,##0.00_-;_-"₩"* "-"??_-;_-@_-' },
    { name: "통화 [0]", id: 7, num: '_-"₩"* #,##0_-;\\-"₩"* #,##0_-;_-"₩"* "-"_-;_-@_-' },
  ];
  // The groups of the gallery, as the program names them.
  const GROUPS = Object.freeze([
    { title: "좋음, 나쁨 및 중립", styles: [NORMAL, ...GOOD_BAD] },
    { title: "데이터 및 모델", styles: DATA },
    { title: "제목 및 표제", styles: HEADINGS },
    { title: "주제 셀 스타일", styles: THEMED },
    { title: "숫자 서식", styles: NUMBERS },
  ]);
  const BY_NAME = new Map(GROUPS.flatMap((group) => group.styles).map((style) => [style.name, style]));
  const PT_PER_PX = 0.75;
  const MAX_CELLS = 500000;

  // The style keys of a cell style: what `format.editStyles` writes over a cell (`null` clears the key).
  function keysOf(style) {
    if (style === NORMAL) return null;
    const keys = {};
    if (style.num) keys.numFmt = style.num;
    if (style.bold) keys.fontWeight = "600";
    if (style.italic) keys.fontStyle = "italic";
    if (style.pt) keys.fontSize = Math.round((style.pt / PT_PER_PX) * 100) / 100 + "px";
    if (style.color) keys.fontColor = style.color.toLowerCase();
    if (style.fill) keys.fill = style.fill.toLowerCase();
    if (style.border) keys.border = Object.freeze({ t: "", r: "", b: "", l: "", ...style.border });
    return keys;
  }

  // The list rows show each style in its own look: the text color, the fill and the weight.
  function preview(row, style) {
    if (style.color) row.style.color = "#" + style.color.toLowerCase();
    if (style.fill) row.style.backgroundColor = "#" + style.fill.toLowerCase();
    if (style.bold) row.style.fontWeight = "600";
    if (style.italic) row.style.fontStyle = "italic";
  }

  function createSheetCellStyles(deps) {
    const { document, kit, say, format, gridUtils, getActiveSheetName } = deps;
    const $ = (id) => document.getElementById(id);
    let bound = false;
    let chosen = null;

    function render() {
      const list = $("sheet-cellstyle-list");
      list.replaceChildren();
      for (const group of GROUPS) {
        const heading = document.createElement("li");
        heading.className = "dialog-label";
        heading.setAttribute("role", "presentation");
        heading.textContent = group.title;
        list.appendChild(heading);
        for (const style of group.styles) {
          const row = document.createElement("li");
          row.setAttribute("role", "option");
          row.dataset.cellStyle = style.name;
          row.setAttribute("aria-selected", "false");
          row.textContent = style.name;
          preview(row, style);
          list.appendChild(row);
        }
      }
    }

    function choose(name) {
      chosen = name;
      for (const row of $("sheet-cellstyle-list").querySelectorAll("[data-cell-style]")) {
        row.setAttribute("aria-selected", row.dataset.cellStyle === name ? "true" : "false");
      }
    }

    function open() {
      bound ||= bind();
      render();
      choose(null);
      kit.note("sheet-cellstyle-status", "");
      kit.openDialog("sheet-cellstyle-dialog");
      return true;
    }

    function bind() {
      $("sheet-cellstyle-list").addEventListener("click", (ev) => {
        const row = ev.target?.closest?.("[data-cell-style]");
        if (row) choose(row.dataset.cellStyle);
      });
      $("sheet-cellstyle-list").addEventListener("dblclick", (ev) => {
        const row = ev.target?.closest?.("[data-cell-style]");
        if (row) void apply(row.dataset.cellStyle);
      });
      $("btn-sheet-cellstyle-ok").addEventListener("click", () => void apply(chosen));
      return true;
    }

    // One undo step over the selection: the parts of the style replace those of each cell; 표준 takes the format off.
    async function apply(name) {
      const style = BY_NAME.get(name);
      if (!style) {
        kit.note("sheet-cellstyle-status", "적용할 스타일을 고르세요");
        return false;
      }
      const keys = keysOf(style);
      kit.closeDialog("sheet-cellstyle-dialog");
      const done = await format.editStyles("셀 스타일", (ctx) => {
        let cells = 0;
        for (const rg of ctx.ranges || [ctx.rg]) cells += (rg.r2 - rg.r1 + 1) * (rg.c2 - rg.c1 + 1);
        if (cells > MAX_CELLS) return "선택 범위가 너무 큼";
        for (const rg of ctx.ranges || [ctx.rg]) {
          for (let r = rg.r1; r <= rg.r2; r += 1) {
            for (let c = rg.c1; c <= rg.c2; c += 1) {
              ctx.patch(gridUtils.refOf(r, c), (st) => {
                if (keys === null) {
                  for (const key of ["fontWeight", "fontStyle", "textDecoration", "fontSize", "numFmt", "fill", "fontColor", "border"]) {
                    if (key in st) st[key] = key === "border" ? null : "";
                  }
                  return;
                }
                for (const [key, value] of Object.entries(keys)) st[key] = value;
              });
            }
          }
        }
        return cells;
      });
      if (done) say("셀 스타일: " + name + " (" + getActiveSheetName() + ")");
      return Boolean(done);
    }

    return { open, apply, actions: { "cell-styles": () => Promise.resolve(open()) } };
  }

  host.__EASYOFFICE_SHEET_CELL_STYLES__ = Object.freeze({ GROUPS, BY_NAME, keysOf, mix });
  host.__EASYOFFICE_CREATE_SHEET_CELL_STYLES__ = createSheetCellStyles;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
