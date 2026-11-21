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
];
