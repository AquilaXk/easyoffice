import assert from "node:assert/strict";
import { paste, pixelsAt } from "./support.mjs";

// The sheet steps of the WebKit suite: what a real browser does that the test doubles of `scripts/test-*.mjs` do not
// (a NodeList without array methods, real focus, real key and mouse input, layout measured by the engine).

const td = (ref) => `#pane-sheet td[data-cell="${ref}"]`;
const cellText = (page, ref) => page.textContent(td(ref));
const active = (page) => page.textContent("#active-cell-ref");
const widthOf = (page, ref) => page.$eval(td(ref), (el) => el.getBoundingClientRect().width);
const sortedRefs = (page) => page.$$eval("#pane-sheet td.in-range, #pane-sheet td.selected-cell", (cells) => cells.map((c) => c.dataset.cell).sort((a, b) => a.localeCompare(b)));

// A table the way a spreadsheet program puts it on the clipboard: classes in a style sheet, `x:` attributes, a merge.
const TABLE_HTML = `<html xmlns:x="urn:schemas-microsoft-com:office:excel"><head><meta name=ProgId content=excel.sheet><style>
td { font-weight:400; mso-number-format:General; }
.xl65 { font-weight:700; background:yellow; }
</style></head><body><table>
<tr><td class=xl65>이름</td><td class=xl65>점수</td></tr>
<tr><td>가나</td><td align=right x:num="3.14159">3.14</td></tr>
<tr><td colspan=2>병합 글</td></tr>
</table></body></html>`;

const asText = (rows) => rows.map((row) => row.join("\t")).join("\n");

async function pasteTable(page) {
  await page.click(td("B2"));
  assert.equal(await paste(page, { "text/html": TABLE_HTML, "text/plain": asText([["글자만", "글자만"]]) }), true, "the page takes the paste over");
  await page.waitForFunction(() => document.querySelector('#pane-sheet td[data-cell="B2"]')?.textContent === "이름");
}

// Two blocks with an outline: the pen's border around C3:D4 (the automatic color follows the theme) and the red edges of a
// conditional format on B8:B9. `above` and `left` own the top and left line (a shared edge is drawn in the cell before).
const OUTLINES = [
  { name: "the border of C3:D4", tl: "C3", tr: "D3", bl: "C4", above: "C2", left: "B3" },
  { name: "the red rule edges of B8:B9", tl: "B8", tr: "B8", bl: "B9", above: "B7", left: "A8" },
];
const RED_EDGE = "thin:ff0000";

async function drawOutlines(page) {
  await page.click(td("C3"));
  await page.keyboard.press("Shift+ArrowDown");
  await page.keyboard.press("Shift+ArrowRight");
  await page.evaluate(() => document.querySelector('[data-sheet-look="border-outer"]').click());
  for (const [ref, value] of [["B8", "4"], ["B9", "5"]]) {
    await page.click(td(ref));
    await page.keyboard.type(value);
    await page.keyboard.press("Enter");
  }
  await page.evaluate((edge) => window.__EASYOFFICE_SHEET_RULES__.set("Sheet1", {
    cf: [{ r: "B8:B9", t: "cellIs", op: "greaterThan", a: "3", b: "", stop: false, fl: 0, bd: { t: edge, r: edge, b: edge, l: edge } }],
    dv: [],
  }), RED_EDGE);
  await page.click(td("H14"));
  // The rule's edges arrive with the engine's answer, after the call returns; sample only once B7 and B9 carry them.
  await page.waitForFunction(() => ["B7", "B9"].every((ref) => {
    const el = document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
    return el && getComputedStyle(el).borderBottomColor === "rgb(255, 0, 0)";
  }));
}

// Where the four lines of `outline` run in device pixels (the last device row or column of each line, in the middle of
// the cell next to it) and the color of the border that draws each.
const edgePoints = (page, outline, scale) =>
  page.evaluate(([o, s]) => {
    const cell = (ref) => document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);
    const box = (ref) => cell(ref).getBoundingClientRect();
    const color = (ref, prop) => getComputedStyle(cell(ref))[prop].match(/\d+/g).slice(0, 3).map(Number);
    const [tl, tr, bl] = [box(o.tl), box(o.tr), box(o.bl)];
    const midX = Math.round(((tl.left + tl.right) / 2) * s);
    const midY = Math.round(((tl.top + tl.bottom) / 2) * s);
    return {
      points: {
        top: [midX, Math.round(tl.top * s) - 1],
        bottom: [midX, Math.round(bl.bottom * s) - 1],
        left: [Math.round(tl.left * s) - 1, midY],
        right: [Math.round(tr.right * s) - 1, midY],
      },
      owner: { top: color(o.above, "borderBottomColor"), bottom: color(o.bl, "borderBottomColor"), left: color(o.left, "borderRightColor"), right: color(o.tr, "borderRightColor") },
      grid: color("H16", "borderBottomColor"),
    };
  }, [outline, scale]);

async function assertOutlinesPainted(page, scale, label) {
  for (const theme of ["light", "dark"]) {
    await page.evaluate((mode) => window.__EASYOFFICE_THEME__.setMode(mode), theme);
    await page.waitForFunction((mode) => document.documentElement.dataset.themeEffective === mode, theme);
    await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
    for (const outline of OUTLINES) {
      const { points, owner, grid } = await edgePoints(page, outline, scale);
      const painted = await pixelsAt(page, points);
      for (const side of Object.keys(points)) {
        const near = painted[side].every((channel, i) => Math.abs(channel - owner[side][i]) <= 2);
        assert.ok(near, `${label} ${theme}: ${outline.name}, ${side} line at ${points[side]} is rgb(${painted[side]}), the border is rgb(${owner[side]})`);
        assert.notDeepEqual(owner[side], grid, `${label} ${theme}: ${outline.name}, the ${side} line has the grid line's color, the test cannot tell them apart`);
      }
    }
  }
}

// The page again in a window of device pixel ratio `scale`, with the same outlines drawn.
async function atScale(page, scale) {
  const context = await page.context().browser().newContext({ viewport: { width: 1100, height: 700 }, deviceScaleFactor: scale });
  try {
    const screen = await context.newPage();
    screen.setDefaultTimeout(8000);
    await screen.goto(page.url().replace("win=webkit-dom", `win=webkit-dom-${scale}x`));
    await screen.waitForSelector("#pane-sheet td[data-cell]");
    await drawOutlines(screen);
    await assertOutlinesPainted(screen, scale, `${scale}x`);
  } finally {
    await context.close();
  }
}

export const sheetSteps = [
  ["a table of HTML is pasted with its cells, numbers and formats (NodeList rows)", async (page) => {
    await pasteTable(page);
    assert.deepEqual([await cellText(page, "C2"), await cellText(page, "B3"), await cellText(page, "B4")], ["점수", "가나", "병합 글"]);
    assert.equal(await cellText(page, "C3"), "3.14159", "the number comes from x:num");
    assert.ok(Number(await page.$eval(td("B2"), (el) => getComputedStyle(el).fontWeight)) >= 600, "the style sheet class reaches the cell");
    assert.equal(await page.$eval(td("B4"), (el) => el.classList.contains("merge-anchor")), true, "the merge travels with the table");
    assert.equal(await page.$eval(td("C4"), (el) => el.classList.contains("merge-cover")), true);
  }],

  ["the same table pasted a second time elsewhere reads the same", async (page) => {
    await pasteTable(page);
    await page.click(td("E8"));
    await paste(page, { "text/html": TABLE_HTML, "text/plain": "글자만" });
    await page.waitForFunction(() => document.querySelector('#pane-sheet td[data-cell="E8"]')?.textContent === "이름");
    assert.deepEqual([await cellText(page, "F8"), await cellText(page, "E9"), await cellText(page, "E10")], ["점수", "가나", "병합 글"]);
    assert.equal(await cellText(page, "B2"), "이름", "the first paste stays");
  }],

  ["a copied block repeats over a selection that is a whole multiple of it", async (page) => {
    await page.click(td("B2"));
    await page.keyboard.press("Shift+ArrowDown");
    for (let i = 0; i < 3; i += 1) await page.keyboard.press("Shift+ArrowRight");
    assert.equal((await sortedRefs(page)).length, 8, "B2:E3 is selected");
    await paste(page, { "text/plain": "a\tb" });
    await page.waitForFunction(() => document.querySelector('#pane-sheet td[data-cell="E3"]')?.textContent === "b");
    assert.deepEqual(await Promise.all(["B2", "C2", "D2", "E2", "B3", "C3", "D3", "E3"].map((ref) => cellText(page, ref))), ["a", "b", "a", "b", "a", "b", "a", "b"]);
  }],

  ["a click after = writes the reference and F4 cycles its $", async (page) => {
    await page.click(td("E5"));
    await page.keyboard.type("=");
    await page.click(td("C3"));
    assert.equal(await cellText(page, "E5"), "=C3", "the click pointed instead of ending the edit");
    assert.equal(await page.textContent("#status-sheet-mode"), "참조");
    const cycle = [];
    for (let i = 0; i < 4; i += 1) {
      await page.keyboard.press("F4");
      cycle.push(await cellText(page, "E5"));
    }
    assert.deepEqual(cycle, ["=$C$3", "=C$3", "=$C3", "=C3"]);
    await page.keyboard.type("+");
    await page.click(td("D4"));
    assert.equal(await cellText(page, "E5"), "=C3+D4");
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => document.querySelector('#pane-sheet td[data-cell="E5"]').textContent === "0", null, { timeout: 3000 });
  }],

  ["a drag over the column or row headers while a formula is typed writes the columns or the rows (PD-296)", async (page) => {
    const center = async (selector) => {
      const box = await page.locator(selector).boundingBox();
      return [box.x + box.width / 2, box.y + box.height / 2];
    };
    const dragBetween = async (from, to) => {
      const [x, y] = await center(from);
      const [x2, y2] = await center(to);
      await page.mouse.move(x, y);
      await page.mouse.down();
      await page.mouse.move(x2, y2, { steps: 6 });
      await page.mouse.up();
    };
    await page.click(td("E5"));
    await page.keyboard.type("=SUM(");
    await dragBetween('#pane-sheet th[data-col="B"]', '#pane-sheet th[data-col="D"]');
    assert.equal(await cellText(page, "E5"), "=SUM(B:D");
    await dragBetween('#pane-sheet th[data-row="2"]', '#pane-sheet th[data-row="4"]');
    assert.equal(await cellText(page, "E5"), "=SUM(2:4", "a second drag replaces the reference");
    assert.equal(await page.textContent("#status-sheet-mode"), "참조");
  }],

  ["a click on a merged cell while a formula is typed writes the whole merged range (PD-296)", async (page) => {
    await page.evaluate(() => window.__EASYOFFICE_SHEET_LAYOUT__.addMerge("Sheet1", 8, 3, 9, 4));
    await page.click(td("B12"));
    await page.keyboard.type("=SUM(");
    await page.click(td("D9"));
    assert.equal(await cellText(page, "B12"), "=SUM(C8:D9", "a click on the cell at the bottom right takes the range");
    await page.keyboard.type(")");
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => document.querySelector('#pane-sheet td[data-cell="B12"]').dataset.formula === "=SUM(C8:D9)");
  }],

  ["the cells of a table column are pointed as its structured reference, and the engine reads it back (PD-296)", async (page) => {
    const rows = [["지역", "분기", "매출", "배수"], ["East", "Q1", "10", "1"], ["East", "Q2", "20", "2"], ["West", "Q1", "5", "3"], ["West", "Q2", "7", "4"]];
    await page.click(td("A1"));
    await paste(page, { "text/plain": asText(rows) });
    await page.waitForFunction(() => document.querySelector('#pane-sheet td[data-cell="D5"]')?.textContent === "4");
    await page.evaluate(() => window.__EASYOFFICE_SHEET_LAYOUT__.setTables("Sheet1", [{ name: "표1", range: "A1:D5", header: true, totals: false, columns: ["지역", "분기", "매출", "배수"], style: "TableStyleMedium2", rowStripes: true }]));
    const shows = (ref, text) => page.waitForFunction(([r, t]) => document.querySelector(`#pane-sheet td[data-cell="${r}"]`)?.textContent === t, [ref, text], { timeout: 5000 });
    const formulaOf = (ref) => page.$eval(td(ref), (el) => el.dataset.formula);
    // The whole data of a column, dragged with the real mouse from outside the table.
    await page.click(td("E8"));
    await page.keyboard.type("=SUM(");
    const first = await page.locator(td("C2")).boundingBox();
    const last = await page.locator(td("C5")).boundingBox();
    await page.mouse.move(first.x + first.width / 2, first.y + first.height / 2);
    await page.mouse.down();
    await page.mouse.move(last.x + last.width / 2, last.y + last.height / 2, { steps: 6 });
    await page.mouse.up();
    assert.equal(await cellText(page, "E8"), "=SUM(표1[매출]");
    await page.keyboard.type(")");
    await page.keyboard.press("Enter");
    await shows("E8", "42");
    assert.equal(await formulaOf("E8"), "=SUM(표1[매출])");
    // A cell of the formula's own row outside the table is the `#This Row` of the table.
    await page.click(td("E3"));
    await page.keyboard.type("=");
    await page.click(td("C3"));
    assert.equal(await cellText(page, "E3"), "=표1[[#This Row],[매출]]");
    await page.keyboard.press("Enter");
    await shows("E3", "20");
    // A cell of the table's own row, from a cell inside the table, is `[@Column]`.
    await page.click(td("D3"));
    await page.keyboard.type("=");
    await page.click(td("C3"));
    assert.equal(await cellText(page, "D3"), "=[@매출]");
    await page.keyboard.press("Enter");
    await shows("D3", "20");
    assert.equal(await formulaOf("D3"), "=[@매출]");
  }],

  ["a click after a finished formula on another sheet enters the formula and selects the cell clicked (PD-296)", async (page) => {
    await page.click("#btn-add-sheet-tab");
    await page.waitForFunction(() => document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab === "Sheet2");
    await page.click('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet1"]');
    await page.waitForFunction(() => document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab === "Sheet1");
    await page.click(td("E5"));
    await page.keyboard.type("=SUM(");
    await page.click('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet2"]');
    await page.waitForFunction(() => document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab === "Sheet2");
    await page.click(td("A1"));
    await page.keyboard.type(")");
    assert.equal(await page.inputValue("#formula-input"), "=SUM(Sheet2!A1)");
    await page.click(td("C3"));
    await page.waitForFunction(() => document.querySelector("#active-cell-ref")?.textContent === "C3");
    assert.equal(await page.$eval("#sheet-tab-bar .sheet-tab-btn.active", (el) => el.dataset.sheetTab), "Sheet2", "the sheet clicked stays");
    assert.equal(await page.$eval("#status-sheet-mode", (el) => el.hidden), true);
    await page.click('#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="Sheet1"]');
    await page.waitForFunction(() => document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab === "Sheet1");
    await page.waitForFunction(() => document.querySelector('#pane-sheet td[data-cell="E5"]')?.dataset.formula === "=SUM(Sheet2!A1)");
  }],

  ["the cell being edited widens with its text and goes back on Esc", async (page) => {
    const before = await widthOf(page, "B14");
    await page.click(td("B14"));
    await page.keyboard.type("글자가 칸보다 길어지면 편집 칸이 오른쪽으로 넓어진다 abcdefghij");
    const grown = await widthOf(page, "B14");
    assert.ok(grown > before + 150, `the box grew from ${before} to ${grown}`);
    assert.equal(await page.$eval(td("B14"), (el) => el.classList.contains("edit-grow")), true);
    await page.keyboard.press("Escape");
    assert.equal(await widthOf(page, "B14"), before);
  }],

  ["cell border and rule edge lines show in the pixels of a 1x screenshot, light and dark", async (page) => {
    await drawOutlines(page);
    await assertOutlinesPainted(page, 1, "1x");
  }],

  ...[2, 1.5, 1.25, 1.75, 3].map((scale) => [`the same lines show at ${scale}x`, (page) => atScale(page, scale)]),

  ["Enter and Tab walk a selected range and keep it", async (page) => {
    await page.click(td("B2"));
    await page.keyboard.press("Shift+ArrowDown");
    await page.keyboard.press("Shift+ArrowRight");
    assert.deepEqual(await sortedRefs(page), ["B2", "B3", "C2", "C3"]);
    const seen = [];
    for (let i = 0; i < 4; i += 1) {
      await page.keyboard.press("Enter");
      seen.push(await active(page));
    }
    assert.deepEqual(seen, ["B3", "C2", "C3", "B2"]);
    await page.keyboard.press("Tab");
    assert.equal(await active(page), "C2");
    await page.keyboard.type("7");
    await page.keyboard.press("Tab");
    assert.equal(await cellText(page, "C2"), "7", "the value typed in the range is committed");
    assert.equal(await active(page), "B3");
    assert.deepEqual(await sortedRefs(page), ["B2", "B3", "C2", "C3"]);
  }],

  ["a rule's border comes back in the engine's answer and is drawn in the slots of the cells that share each edge (PD-284)", async (page) => {
    const RED = "rgb(255, 0, 0)";
    for (const [ref, value] of [["G10", "1"], ["G11", "5"]]) {
      await page.click(td(ref));
      await page.keyboard.type(value);
      await page.keyboard.press("Enter");
    }
    const outline = { t: "thin:ff0000", r: "thin:ff0000", b: "thin:ff0000", l: "thin:ff0000" };
    await page.evaluate((bd) => window.__EASYOFFICE_SHEET_RULES__.set("Sheet1", { cf: [{ r: "G10:G11", t: "cellIs", op: "greaterThan", a: "3", b: "", stop: false, fl: 0, bd }], dv: [] }), outline);
    const color = (ref, prop) => page.$eval(td(ref), (el, name) => getComputedStyle(el)[name], prop);
    await page.waitForFunction(() => getComputedStyle(document.querySelector('#pane-sheet td[data-cell="G11"]')).borderRightColor === "rgb(255, 0, 0)", null, { timeout: 5000 });
    assert.equal(await color("G10", "borderBottomColor"), RED, "the top edge of G11 sits in the slot of the cell above");
    assert.equal(await color("F11", "borderRightColor"), RED, "and its left edge in the slot of the cell before it");
    assert.equal(await color("G11", "borderBottomColor"), RED);
    assert.notEqual(await color("G10", "borderRightColor"), RED, "G10 is not greater than 3");
    await page.evaluate(() => window.__EASYOFFICE_SHEET_RULES__.replaceAll({}));
    await page.waitForFunction(() => getComputedStyle(document.querySelector('#pane-sheet td[data-cell="G11"]')).borderRightColor !== "rgb(255, 0, 0)", null, { timeout: 5000 });
    assert.notEqual(await color("G10", "borderBottomColor"), RED, "replacing the rules takes the edges off");
  }],

  ["the default row height sets the height of every row in the real layout, and a drag with every row selected changes it (PD-286)", async (page) => {
    const height = (ref) => page.$eval(td(ref), (el) => el.getBoundingClientRect().height);
    const top = (ref) => page.$eval(td(ref), (el) => el.getBoundingClientRect().top);
    assert.deepEqual([await height("B3"), await height("B9")], [24, 24]);
    // Ctrl+A, then the edge of row 3 dragged 16 px down with the real mouse.
    const drag = async (dy) => {
      const edge = await page.locator('#pane-sheet tbody tr[data-row="3"] .row-resizer').boundingBox();
      const [x, y] = [edge.x + edge.width / 2, edge.y + edge.height / 2];
      await page.mouse.move(x, y);
      await page.mouse.down();
      await page.mouse.move(x, y + dy, { steps: 4 });
      await page.mouse.up();
    };
    await page.click(td("B2"));
    await page.keyboard.press(process.platform === "darwin" ? "Meta+A" : "Control+A");
    await drag(16);
    await page.waitForFunction(() => window.__EASYOFFICE_SHEET_LAYOUT__.defaultRowHeightOf("Sheet1") === 30);
    assert.equal(await height("B3"), 40);
    assert.equal(await height("B9"), 40, "every row, not the pressed one only");
    assert.equal((await top("B9")) - (await top("B3")), 6 * 40, "the rows are stacked at the default height");
    assert.deepEqual(await page.evaluate(() => window.__EASYOFFICE_SHEET_LAYOUT__.getAll().Sheet1), { defaultRowHeight: [30] });
    // Without every row selected the same drag sets that row only.
    await page.click(td("B2"));
    await drag(20);
    await page.waitForFunction(() => window.__EASYOFFICE_SHEET_LAYOUT__.rowHeightsOf("Sheet1")?.has(3));
    assert.equal(await height("B3"), 60);
    assert.equal(await height("B4"), 40);
    assert.equal(await page.evaluate(() => window.__EASYOFFICE_SHEET_LAYOUT__.defaultRowHeightOf("Sheet1")), 30);
  }],

  ["Ctrl+PageUp and Ctrl+PageDown move between the sheets with the real keyboard, Option+Arrow does on a Mac (PD-291)", async (page) => {
    const tab = () => page.$eval("#sheet-tab-bar .sheet-tab-btn.active", (el) => el.dataset.sheetTab);
    await page.click("#btn-add-sheet-tab");
    await page.waitForFunction(() => document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab === "Sheet2");
    await page.click(td("B2"));
    await page.keyboard.press("Control+PageUp");
    await page.waitForFunction(() => document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab === "Sheet1");
    await page.keyboard.press("Control+PageUp");
    assert.equal(await tab(), "Sheet1", "the first sheet stays");
    await page.keyboard.press("Control+PageDown");
    await page.waitForFunction(() => document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab === "Sheet2");
    await page.keyboard.press("Control+PageDown");
    assert.equal(await tab(), "Sheet2", "the last sheet stays, and the keys work again in the sheet that was switched to");
    const mac = await page.evaluate(() => /mac/i.test(navigator.platform));
    await page.keyboard.press("Alt+ArrowLeft");
    if (mac) await page.waitForFunction(() => document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab === "Sheet1");
    else assert.equal(await tab(), "Sheet2", "Alt+Left is not a sheet key outside a Mac");
  }],

  ["GETPIVOTDATA reads the pivot table the page made, and its value follows the source once the pivot table is refreshed (PD-292)", async (page) => {
    const rows = [["지역", "분기", "매출"], ["East", "Q1", "10"], ["East", "Q2", "20"], ["West", "Q1", "5"], ["West", "Q2", "7"]];
    await page.click(td("A1"));
    await paste(page, { "text/plain": asText(rows) });
    await page.waitForFunction(() => document.querySelector('#pane-sheet td[data-cell="C5"]')?.textContent === "7");
    const tab = () => page.$eval("#sheet-tab-bar .sheet-tab-btn.active", (el) => el.dataset.sheetTab);
    const enter = async (ref, text) => {
      await page.click(td(ref));
      await page.keyboard.type(text);
      await page.keyboard.press("Enter");
    };
    const shows = (ref, text) => page.waitForFunction(([r, t]) => document.querySelector(`#pane-sheet td[data-cell="${r}"]`)?.textContent === t, [ref, text], { timeout: 5000 });
    // The pivot table: region down, quarter across, the sum of the sales. It lands on a new sheet of its own.
    await page.evaluate(() => window.__EASYOFFICE_SHEET_PIVOT__.openPivotDialog());
    assert.equal(await page.inputValue("#sheet-pivot-range"), "A1:C5");
    await page.selectOption("#sheet-pivot-col-select", "1");
    await page.click("#btn-sheet-pivot-apply");
    await page.waitForFunction(() => window.__EASYOFFICE_SHEET_LAYOUT__.pivotsOf(document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab).length === 1);
    const report = await tab();
    assert.notEqual(report, "Sheet1", "the pivot table is on its own sheet");
    // Formulas outside the table: the grand total, a region, a region and a quarter, and what is not in the table.
    const formulas = [
      ["H2", '=GETPIVOTDATA("매출",$A$1)', "42"],
      ["H3", '=GETPIVOTDATA("합계: 매출",$B$3)', "42"],
      ["H4", '=GETPIVOTDATA("매출",$A$1,"지역","East")', "30"],
      ["H5", '=GETPIVOTDATA("매출",$A$1,"지역","west","분기","Q2")', "7"],
      ["H6", '=GETPIVOTDATA("매출",$A$1,"지역","North")', "#REF!"],
      ["H7", '=GETPIVOTDATA("매출",$K$20)', "#REF!"],
    ];
    for (const [ref, formula, value] of formulas) {
      await enter(ref, formula);
      await shows(ref, value);
    }
    // A source value changes: the table says the old number until it is refreshed; after the refresh the formulas follow.
    await page.click("#sheet-tab-bar .sheet-tab-btn[data-sheet-tab='Sheet1']");
    await page.waitForFunction(() => document.querySelector('#pane-sheet td[data-cell="C2"]')?.textContent === "10");
    await enter("C2", "110");
    await page.click(`#sheet-tab-bar .sheet-tab-btn[data-sheet-tab="${report}"]`);
    await page.waitForFunction(() => document.querySelector('#pane-sheet td[data-cell="H2"]') !== null);
    await page.evaluate(() => window.__EASYOFFICE_SHEET_PIVOT__.refreshAllPivots());
    await shows("H2", "142");
    await shows("H4", "130");
    assert.equal(await cellText(page, "H5"), "7");
    // The pivot table is deleted: the formulas have nothing to read.
    await page.click(td("A1"));
    await page.evaluate(() => window.__EASYOFFICE_SHEET_PIVOT__.deletePivot());
    await shows("H2", "#REF!");
    await shows("H4", "#REF!");
  }],

  ["a pivot table with a long name and a long filter item answers #REF! alone and the other formulas keep their values (PD-292)", async (page) => {
    const rows = [["지역", "분기", "매출"], ["East", "Q1", "10"], ["East", "Q2", "20"], ["West", "Q1", "5"], ["West", "Q2", "7"]];
    await page.click(td("A1"));
    await paste(page, { "text/plain": asText(rows) });
    await page.waitForFunction(() => document.querySelector('#pane-sheet td[data-cell="C5"]')?.textContent === "7");
    await page.evaluate(() => window.__EASYOFFICE_SHEET_PIVOT__.openPivotDialog());
    await page.click("#btn-sheet-pivot-apply");
    await page.waitForFunction(() => window.__EASYOFFICE_SHEET_LAYOUT__.pivotsOf(document.querySelector("#sheet-tab-bar .sheet-tab-btn.active")?.dataset.sheetTab).length === 1);
    // A second pivot table with a name of 171 Korean letters (over 512 bytes) and a filter item of 300 letters, the way a long cell
    // reaches a filter. It lies where A20 is.
    const kept = await page.evaluate(() => {
      const layout = window.__EASYOFFICE_SHEET_LAYOUT__;
      const sheet = document.querySelector("#sheet-tab-bar .sheet-tab-btn.active").dataset.sheetTab;
      const [first] = layout.pivotsOf(sheet);
      const long = { ...first, name: "가".repeat(171), location: "A20:D23", filters: [{ col: 0, values: ["East", "West", "가".repeat(300)], blank: false }] };
      layout.setPivots(sheet, [first, long]);
      return layout.pivotsOf(sheet).length;
    });
    assert.equal(kept, 2, "the page keeps both tables");
    const enter = async (ref, text, shown) => {
      await page.click(td(ref));
      await page.keyboard.type(text);
      await page.keyboard.press("Enter");
      await page.waitForFunction(([r, t]) => document.querySelector(`#pane-sheet td[data-cell="${r}"]`)?.textContent === t, [ref, shown], { timeout: 5000 });
    };
    await enter("H2", '=GETPIVOTDATA("매출",$A$1)', "42");
    await enter("H3", '=GETPIVOTDATA("매출",$A$20)', "#REF!");
    await enter("H4", "=1+2", "3");
  }],

  ["a printed sheet carries the fill, text color and bold a conditional format gave its cells (PD-289)", async (page) => {
    for (const [ref, value] of [["B2", "1"], ["B3", "5"]]) {
      await page.click(td(ref));
      await page.keyboard.type(value);
      await page.keyboard.press("Enter");
    }
    await page.evaluate(() => window.__EASYOFFICE_SHEET_RULES__.set("Sheet1", { cf: [{ r: "B2:B3", t: "cellIs", op: "greaterThan", a: "3", b: "", stop: false, fl: 1, bg: "#ffc7ce", fg: "#9c0006" }], dv: [] }));
    await page.waitForFunction(() => document.querySelector('#pane-sheet td[data-cell="B3"]').style.getPropertyValue("--cf-bg") === "#ffc7ce", null, { timeout: 5000 });
    await page.evaluate(() => {
      window.print = () => {};
      window.__EASYOFFICE_NATIVE_MENU__("file:print");
    });
    await page.waitForSelector("#eo-print-root td", { state: "attached" });
    const printed = await page.$$eval("#eo-print-root td", (cells) => cells.map((c) => ({ text: c.textContent, fill: getComputedStyle(c).backgroundColor, color: getComputedStyle(c).color, bold: c.classList.contains("eo-print-b") })));
    const five = printed.find((c) => c.text === "5");
    assert.deepEqual([five.fill, five.color], ["rgb(255, 199, 206)", "rgb(156, 0, 6)"]);
    assert.equal(five.bold, true, "the rule's bold flag is the print class of a bold cell (the print rules apply on paper only)");
    const one = printed.find((c) => c.text === "1");
    assert.notEqual(one.fill, "rgb(255, 199, 206)", "a cell the rule does not match prints plain");
  }],
];
