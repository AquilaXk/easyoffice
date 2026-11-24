import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule, studioItem } from "./test-support/modules.mjs";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { rulesFor, ruleWithSelector } from "./test-support/css-rules.mjs";
import { readText } from "./test-support/read-text.mjs";

// PD-348: 시트 화면·측정·줄바꿈·인쇄·행 높이 기준이 통합 문서의 Normal 글꼴(defaultFont, 없으면 맑은 고딕 11pt = 14.67px)을 함께 따른다.

const UI_DIR = new URL("../crates/easyoffice-app/ui/", import.meta.url);
const ui = (name) => fileURLToPath(new URL(name, UI_DIR));
const readUi = (name) => readText(ui(name));

class FakeStyleSheet {
  constructor() {
    this.cssRules = [];
  }
  insertRule(text, at = this.cssRules.length) {
    this.cssRules.splice(at, 0, { cssText: text, style: { props: {}, setProperty(k, v) { this.props[k] = v; } } });
    return at;
  }
}

test("CSS tokens and rules separate sheet cells (Normal font vars) from UI headers, doc tables, and slide tables", () => {
  const tokens = readUi("tokens.css");
  assert.match(tokens, /--sheet-cell-font-family:\s*'맑은 고딕',\s*'Malgun Gothic',\s*'Apple SD Gothic Neo',\s*'Noto Sans KR',\s*sans-serif;/);
  assert.match(tokens, /--sheet-cell-font-size:\s*11pt;/);

  const styles = readUi("styles.css");
  const cellBodies = rulesFor(styles, [".sheet-table td"]).filter((r) => r.selector.trim() === ".sheet-table td").map((r) => r.body).join(" ");
  assert.match(cellBodies, /font-family:\s*var\(--sheet-cell-font-family\)/);
  assert.match(cellBodies, /font-size:\s*var\(--sheet-cell-font-size\)/);

  const uiRule = rulesFor(styles, [".sheet-table th"]).find((r) => r.selector.includes(".slide-table-obj td"));
  assert.ok(uiRule, "shared UI table selector keeps UI font rule");
  for (const sel of [".sheet-table th", ".doc-inline-table th", ".doc-inline-table td", ".slide-table-obj td"]) {
    assert.ok(uiRule.selector.split(",").some((part) => part.trim() === sel), `${sel} in UI table rule`);
  }
  assert.match(uiRule.body, /font-family:\s*var\(--font-family-ui\)/);
  assert.match(uiRule.body, /font-size:\s*12px/);
  const gridTdBodies = rulesFor(styles, [".sheet-grid td"]).filter((r) => r.selector.trim() === ".sheet-grid td").map((r) => r.body).join(" ");
  assert.match(gridTdBodies, /line-height:\s*max\(21px,\s*calc\(1\.275em\s*-\s*1px\)\)/);

  const shell = readUi("shell.css");
  const printRule = ruleWithSelector(shell, "#eo-print-root .eo-print-sheet-page .eo-print-sheet");
  assert.ok(printRule);
  assert.match(printRule.body, /font-family:\s*var\(--sheet-cell-font-family\)/);
  assert.match(printRule.body, /font-size:\s*var\(--sheet-cell-font-size\)/);
  const previewRule = ruleWithSelector(shell, ".print-preview-pane .eo-print-sheet-page .eo-print-sheet");
  assert.ok(previewRule);
  assert.match(previewRule.body, /font-family:\s*var\(--sheet-cell-font-family\)/);
  assert.match(previewRule.body, /font-size:\s*var\(--sheet-cell-font-size\)/);
  const printHeadRule = ruleWithSelector(shell, "#eo-print-root .eo-print-sheet-page .eo-print-sheet td.eo-print-head");
  assert.ok(printHeadRule && /font-size:\s*12px/.test(printHeadRule.body));
});

test("sheet-default-font enforces [1pt, 409pt], CSS injection bounds, CSS string quoting, and root variable sync", async () => {
  const { host, document } = await createWorkspaceHarness();
  const df = host.__EASYOFFICE_SHEET_DEFAULT_FONT__;
  assert.equal(df.baseFontPt(), 11);
  assert.equal(df.baseFontPx(), 14.67);
  assert.equal(df.cssFontSize(), "11pt");
  assert.equal(df.baseFontFamily(), "'맑은 고딕', 'Malgun Gothic', 'Apple SD Gothic Neo', 'Noto Sans KR', sans-serif");

  df.set({ name: "Arial", size: 20 }, 112);
  assert.equal(df.baseFontPt(), 20);
  assert.equal(df.baseFontPx(), 26.67);
  assert.equal(df.cssFontSize(), "20pt");
  assert.match(df.baseFontFamily(), /^'Arial', '맑은 고딕'/);
  assert.equal(document.documentElement.style.getPropertyValue("--sheet-cell-font-size"), "20pt");
  assert.match(document.documentElement.style.getPropertyValue("--sheet-cell-font-family"), /^'Arial', '맑은 고딕'/);

  // Generic keyword stays unquoted and boundary sizes [1pt, 409pt] succeed while sub-1pt or over-409pt fail closed.
  df.set({ name: "sans-serif", size: 1 }, 40);
  assert.equal(df.baseFontPx(), 1.33);
  assert.match(df.baseFontFamily(), /^sans-serif, '맑은 고딕'/);

  df.set({ name: "Calibri", size: 409 }, 2000);
  assert.equal(df.baseFontPx(), 545.33);

  const prevVer = df.version();
  const badNames = ["Bad;font-size:99px", "Bad'Quote", "Bad\"Quote", "Bad\\Slash", "Bad{Brace}", "Bad<Tag>", "Bad\nLine", "F".repeat(65)];
  for (const name of badNames) {
    assert.equal(df.quoteCssFont(name), "", `quoteCssFont rejects ${name}`);
  }
  for (const invalid of [
    { name: "Arial", size: 0.99 },
    { name: "Arial", size: 0 },
    { name: "Arial", size: 409.01 },
    ...badNames.map((name) => ({ name, size: 14 })),
  ]) {
    df.set(invalid, 90);
    assert.equal(df.get(), null, JSON.stringify(invalid));
    assert.equal(df.baseFontPt(), 11);
    assert.equal(df.baseFontPx(), 14.67);
    assert.equal(document.documentElement.style.getPropertyValue("--sheet-cell-font-size"), "");
    assert.equal(document.documentElement.style.getPropertyValue("--sheet-cell-font-family"), "");
  }
  assert.ok(df.version() > prevVer);
});

test("col-fit and row-fit invalidate caches and recompute when defaultFont changes or resets", () => {
  const dfEnv = {};
  loadUiModule(ui("sheet-default-font.js"), dfEnv);
  const df = dfEnv.__EASYOFFICE_SHEET_DEFAULT_FONT__;

  const seenCol = [];
  const fitCol = loadUiModule(ui("sheet-col-fit.js"), dfEnv).__EASYOFFICE_CREATE_SHEET_COL_FIT__({
    getMap: () => ({ A1: { value: "가나다" }, A2: { value: "굵은글", style: Object.freeze({ fontWeight: "600" }) } }),
    packRef: (ref) => (ref === "A1" ? 32768 + 1 : 65536 + 1),
    measure: (font, family, text) => {
      seenCol.push([font, family, text]);
      const px = Number.parseFloat(/(\d+(?:\.\d+)?)px/.exec(font)?.[1] || "0");
      return [...text].length * px * 0.6;
    },
    isColHidden: () => false,
    isRowHidden: () => false,
    mergeAt: () => null,
    headerCells: () => false,
    formatGeneral: (t) => t,
    defaultWidth: 72,
    minWidth: 36,
    maxWidth: 720,
  });

  return (async () => {
    const wDefault = await fitCol.widthsFor([1]);
    assert.equal(wDefault.widths[0].width, Math.ceil(3 * 14.67 * 0.6 + 13));
    assert.ok(seenCol[0][0].includes("14.67px") && seenCol[0][1].includes("맑은 고딕"));

    df.set({ name: "Arial", size: 20 }, 112);
    const wLarge = await fitCol.widthsFor([1]);
    assert.equal(wLarge.widths[0].width, Math.ceil(3 * 26.67 * 0.6 + 13));
    assert.ok(seenCol.at(-1)[0].includes("26.67px") && seenCol.at(-1)[1].includes("Arial"));

    // Reset back to standard must also bypass memo/WeakMap cache cleanly.
    df.set(null);
    const wBack = await fitCol.widthsFor([1]);
    assert.equal(wBack.widths[0].width, Math.ceil(3 * 14.67 * 0.6 + 13));

    const lookEnv = { __EASYOFFICE_SHEET_DEFAULT_FONT__: df };
    loadUiModule(ui("sheet-look.js"), lookEnv);
    const refs = loadUiModule(ui("sheet-refs.js"), {}).__EASYOFFICE_SHEET_REFS__;
    const seenRow = [];
    const map = {
      A1: { value: "기본 글꼴 칸" },
      A2: { value: "줄바꿈 긴 내용입니다", style: Object.freeze({ wrap: true }) },
    };
    const rowFit = loadUiModule(ui("sheet-row-fit.js"), {
      __EASYOFFICE_SHEET_LOOK__: lookEnv.__EASYOFFICE_SHEET_LOOK__,
      __EASYOFFICE_SHEET_DEFAULT_FONT__: df,
    }).__EASYOFFICE_CREATE_ROW_FIT__({
      rowH: () => 22,
      gridRowH: () => 22,
      maxRows: 1048576,
      getSheetName: () => "Sheet1",
      getMap: () => map,
      columnWidth: () => 100,
      isRowHidden: () => false,
      isMerged: () => false,
      measure: (text, width, font, family) => {
        seenRow.push([font, family]);
        return 2;
      },
      packRef: refs.packRef,
    });

    assert.equal(rowFit.baseTall(), false);
    assert.equal(rowFit.sync(), true);
    assert.equal(rowFit.heightOf(1), 22, "11pt keeps 22px baseline row height");
    assert.equal(rowFit.heightOf(2), 2 * 20 + 7, "11pt wrapped cell uses Math.round(14.67 * 1.35) = 20px line height");
    assert.ok(seenRow[0][0].includes("14.67px"));

    df.set({ name: "Arial", size: 20 }, 112);
    assert.equal(rowFit.baseTall(), true);
    assert.equal(rowFit.sync(), true);
    assert.equal(rowFit.heightOf(1), Math.ceil(26.67 * 1.275), "unstyled cell expands row for large defaultFont");
    assert.equal(rowFit.heightOf(2), 2 * 36 + 7, "20pt wrapped cell uses Math.round(26.67 * 1.35) = 36px line height");
    assert.ok(seenRow.at(-1)[0].includes("26.67px") && seenRow.at(-1)[1].includes("Arial"));

    df.set(null);
    assert.equal(rowFit.baseTall(), false);
    assert.equal(rowFit.sync(), true);
    assert.equal(rowFit.heightOf(1), 22);
    assert.equal(rowFit.heightOf(2), 2 * 20 + 7);
  })();
});

test("opening a workbook with large defaultFont syncs grid row height, wrapped line-height, canvas measurer, print CSS, and Undo", async () => {
  const h = await createWorkspaceHarness();
  const d = h.document;
  const seenFonts = [];
  const make = d.createElement.bind(d);
  d.createElement = (tag) => {
    const el = make(tag);
    if (tag === "canvas") {
      el.getContext = () => ({
        _font: "",
        set font(v) {
          this._font = v;
          seenFonts.push(v);
        },
        get font() {
          return this._font;
        },
        measureText(t) {
          const px = Number.parseFloat(/(\d+(?:\.\d+)?)px/.exec(this._font)?.[1] || "0") || (Number.parseFloat(/(\d+(?:\.\d+)?)pt/.exec(this._font)?.[1] || "11") * 96) / 72;
          return { width: [...t].length * Math.max(6, px * 0.55) };
        },
      });
    }
    return el;
  };
  h.host.CSSStyleSheet = FakeStyleSheet;
  h.host.requestAnimationFrame = (cb) => cb();
  const printedRoots = [];
  h.host.print = () => {
    printedRoots.push(d.getElementById("eo-print-root"));
  };
  d.adoptedStyleSheets = [];
  switchModule(d, "SHEET");
  await flushAsyncTurns();

  const row = (n) => d.querySelector(`#pane-sheet tbody tr[data-row="${n}"]`);
  const cell = (ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);

  // 1) Open file with non-standard large defaultFont (Arial 20pt -> 26.67px -> 35px single-line height).
  await pickFile(
    d,
    makeFixtureFile("큰글꼴.xlsx", {
      defaultFont: { name: "Arial", size: 20 },
      defaultColPx: 112,
      sheets: [
        {
          name: "Sheet1",
          cells: [
            { ref: "A1", raw: "일반 큰 칸" },
            { ref: "A2", raw: "줄바꿈 테스트 문구 길게 작성합니다", style: { wrap: true } },
          ],
        },
      ],
    })
  );
  await flushAsyncTurns(14);

  assert.equal(d.documentElement.style.getPropertyValue("--sheet-cell-font-size"), "20pt");
  assert.match(d.documentElement.style.getPropertyValue("--sheet-cell-font-family"), /^'Arial'/);
  assert.equal(row(1).style.height, "35px", "unstyled cell expands row to ceil(26.67 * 1.275) = 35px");
  assert.equal(cell("A2").style.lineHeight, "36px", "wrapped line-height matches 20pt = 26.67px * 1.35");
  assert.ok(seenFonts.some((f) => f.includes("Arial") && (f.includes("26.67px") || f.includes("20pt"))));

  // Print & preview rules carry Arial 20pt.
  const preview = await h.host.__EASYOFFICE_OFFICE_PRINT__.buildPreview();
  assert.ok(preview.rulesCss.some((r) => r.includes("eo-print-sheet") && r.includes("font-size: 20pt;") && r.includes("Arial")));
  d.defaultView.__EASYOFFICE_NATIVE_MENU__("file:print");
  await flushAsyncTurns(8);
  assert.ok(printedRoots.at(-1));
  const adopted = d.adoptedStyleSheets.flatMap((s) => s.cssRules.map((r) => r.cssText));
  assert.ok(adopted.some((r) => r.includes("eo-print-sheet") && r.includes("font-size: 20pt;") && r.includes("Arial")));
  h.sandbox.dispatchWindowEvent("afterprint");

  // 2) Open standard file (no defaultFont -> Malgun Gothic 11pt = 14.67px) and confirm cache & DOM reset cleanly.
  await pickFile(
    d,
    makeFixtureFile("표준.xlsx", {
      sheets: [
        {
          name: "Sheet1",
          cells: [
            { ref: "A1", raw: "표준 칸" },
            { ref: "A2", raw: "줄바꿈 테스트 문구 길게 작성합니다", style: { wrap: true } },
          ],
        },
      ],
    })
  );
  await flushAsyncTurns(14);

  assert.equal(d.documentElement.style.getPropertyValue("--sheet-cell-font-size"), "");
  assert.equal(row(1).style.height, "", "11pt keeps standard 22px row height");
  assert.equal(cell("A2").style.lineHeight, "20px", "11pt wrapped cell uses 20px line height");

  const stdPreview = await h.host.__EASYOFFICE_OFFICE_PRINT__.buildPreview();
  assert.ok(stdPreview.rulesCss.some((r) => r.includes("eo-print-sheet") && r.includes("font-size: 11pt;") && r.includes("맑은 고딕")));

  // 3) Undo restores the previous large-defaultFont workbook and its CSS/row heights/wrap line-height.
  h.userClick(studioItem(d, "studio-undo"));
  await flushAsyncTurns(14);
  assert.equal(d.documentElement.style.getPropertyValue("--sheet-cell-font-size"), "20pt");
  assert.equal(row(1).style.height, "35px");
  assert.equal(cell("A2").style.lineHeight, "36px");
});

test("row-fit resets inactive sheets across font switch, printStyle sets overflowWrap, and plain/formula edits re-sync row heights", async () => {
  // 1) printStyle includes overflowWrap: anywhere on wrap cells.
  const df = loadUiModule(ui("sheet-default-font.js"), {}).__EASYOFFICE_SHEET_DEFAULT_FONT__;
  const refs = loadUiModule(ui("sheet-refs.js"), {}).__EASYOFFICE_SHEET_REFS__;
  const lookEnv = { __EASYOFFICE_SHEET_DEFAULT_FONT__: df };
  loadUiModule(ui("sheet-look.js"), lookEnv);
  const wrapTd = { style: {} };
  lookEnv.__EASYOFFICE_SHEET_LOOK__.printStyle(wrapTd, { wrap: true });
  assert.equal(wrapTd.style.whiteSpace, "pre-wrap");
  assert.equal(wrapTd.style.overflowWrap, "anywhere");

  // 2) Multi-sheet rowFit invalidation + isCapped flags on single-line wrap and short manual rows.
  let active = "Sheet1";
  const sheets = {
    Sheet1: { A1: { value: "SingleWrap", style: { wrap: true } }, A2: { value: "Line1\nLine2", style: { wrap: true } } },
    Sheet2: { B1: { value: "Tab2Text" } },
  };
  let custom = [];
  const rowFit = loadUiModule(ui("sheet-row-fit.js"), {
    __EASYOFFICE_SHEET_LOOK__: lookEnv.__EASYOFFICE_SHEET_LOOK__,
    __EASYOFFICE_SHEET_DEFAULT_FONT__: df,
  }).__EASYOFFICE_CREATE_ROW_FIT__({
    rowH: () => 22,
    gridRowH: () => 22,
    maxRows: 1048576,
    getSheetName: () => active,
    getMap: () => sheets[active],
    columnWidth: () => 100,
    isRowHidden: () => false,
    isMerged: () => false,
    customHeights: () => custom,
    measure: (text) => (text.includes("\n") ? 2 : 1),
    packRef: refs.packRef,
  });

  df.set({ name: "Arial", size: 20 }, 112);
  active = "Sheet2";
  assert.equal(rowFit.sync(), true);
  assert.equal(rowFit.heightOf(1), 35, "Sheet2 populated row expands to 35px under 20pt");
  active = "Sheet1";
  assert.equal(rowFit.sync(), true);
  assert.equal(rowFit.isCapped(1), true, "single-line wrap at 20pt caps padding/lineHeight inside 35px");
  assert.equal(rowFit.isCapped(2), false, "multi-line wrap row keeps wrap line-height and vertical padding");

  // Switch back to 11pt while active Sheet1 is empty: Sheet2's cached 35px must clear before switch.
  sheets.Sheet1 = {};
  sheets.Sheet2 = { B1: { value: "Plain11pt" } };
  df.set(null);
  assert.equal(rowFit.any(), false, "stateOf clears all stale sheets immediately on font change");
  active = "Sheet2";
  assert.equal(rowFit.heightOf(1), 22, "Sheet2 does not keep stale 35px height after font reset");
  assert.equal(rowFit.sync(), true);
  assert.equal(rowFit.heightOf(1), 22);

  // Explicit 80px custom row is not marked capped, while 23px row under 20pt is marked capped.
  df.set({ name: "Arial", size: 20 }, 112);
  custom = [[1, 80], [2, 23]];
  assert.equal(rowFit.sync(), true);
  assert.equal(rowFit.isCapped(1), false);
  assert.equal(rowFit.isCapped(2), true);

  // 3) Workspace integration: direct cell edit (insert & clear) and formula recalc immediately sync row heights in 20pt sheet.
  const h = await createWorkspaceHarness();
  const d = h.document;
  switchModule(d, "SHEET");
  await flushAsyncTurns();
  await pickFile(
    d,
    makeFixtureFile("편집동기화.xlsx", {
      defaultFont: { name: "Arial", size: 20 },
      sheets: [{ name: "Sheet1", cells: [{ ref: "A1", raw: "제목" }] }],
    })
  );
  await flushAsyncTurns(14);
  const row = (n) => d.querySelector(`#pane-sheet tbody tr[data-row="${n}"]`);
  const cell = (ref) => d.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
  assert.equal(row(1).style.height, "35px");
  assert.equal(row(3).style.height, "");

  const press = (el, k) => el.dispatchEvent({ type: "keydown", key: k, target: el, bubbles: true });
  // Plain edit into empty A3 -> Enter expands row 3 to 35px without full sheet reopen.
  cell("A3").focus();
  await flushAsyncTurns(2);
  cell("A3").dispatchEvent({ type: "beforeinput", inputType: "insertText", data: "새값", target: cell("A3") });
  cell("A3").textContent = "새값";
  cell("A3").dispatchEvent({ type: "input", inputType: "insertText", data: "새값", target: cell("A3") });
  press(cell("A3"), "Enter");
  await flushAsyncTurns(8);
  assert.equal(row(3).style.height, "35px");

  // Formula entry =1+2 into A4 recalculates and expands row 4 to 35px.
  cell("A4").focus();
  await flushAsyncTurns(2);
  cell("A4").dispatchEvent({ type: "beforeinput", inputType: "insertText", data: "=1+2", target: cell("A4") });
  cell("A4").textContent = "=1+2";
  cell("A4").dispatchEvent({ type: "input", inputType: "insertText", data: "=1+2", target: cell("A4") });
  press(cell("A4"), "Enter");
  await flushAsyncTurns(8);
  assert.equal(row(4).style.height, "35px");

  // Clearing A1 back to "" via F2 + empty input + Enter shrinks row 1 back to default 22px ("").
  cell("A1").focus();
  await flushAsyncTurns(2);
  press(cell("A1"), "F2");
  cell("A1").textContent = "";
  cell("A1").dispatchEvent({ type: "input", inputType: "deleteContentBackward", target: cell("A1") });
  press(cell("A1"), "Enter");
  await flushAsyncTurns(8);
  assert.equal(row(1).style.height, "");
});
