import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { switchModule } from "./test-support/modules.mjs";

// A printed sheet draws what the conditional formats give its cells (PD-289): fill, text color, font flags and edges, from
// the same answers the grid paints from. The cells of the page are `Sheet1!B2:D3`.

class FakeStyleSheet {
  constructor() { this.cssRules = []; }
  insertRule(text, at) {
    this.cssRules.splice(at, 0, { cssText: text, style: { props: {}, setProperty(k, v) { this.props[k] = v; } } });
    return at;
  }
}

// The packed answer of the engine for the given appearances, as `pack.rs` writes it (see `test-webui-sheet-rules.mjs`).
function pack(entries) {
  const palette = [];
  const id = (rgb) => {
    let i = palette.indexOf(rgb);
    if (i < 0) i = palette.push(rgb) - 1;
    return i;
  };
  const LINES = ["", "thin", "thick", "dashed", "dotted", "double"];
  const table = [];
  const setId = (border) => {
    const set = ["t", "r", "b", "l"].map((side) => {
      const [line, rgb] = (border[side] || ":").split(":");
      return line ? (LINES.indexOf(line) << 24) + parseInt(rgb, 16) : 0;
    });
    const key = JSON.stringify(set);
    let i = table.findIndex((s) => JSON.stringify(s) === key);
    if (i < 0) i = table.push(set) - 1;
    return i;
  };
  const words = [];
  for (const e of entries) {
    let head = e.col | ((e.flags || 0) << 14);
    if (e.bg !== undefined) head |= 1 << 18;
    if (e.fg !== undefined) head |= 1 << 19;
    if (e.bar) head |= (1 << 20) | (e.bar.gradient ? 1 << 23 : 0);
    if (e.icon) head |= (1 << 21) | (((e.icon.set << 3) | e.icon.index) << 24);
    if (e.hide) head |= 1 << 22;
    words.push((e.row | (e.border ? 0x80000000 : 0)) >>> 0, head >>> 0);
    if (e.bg !== undefined || e.fg !== undefined) words.push(((e.bg !== undefined ? id(e.bg) : 0) | ((e.fg !== undefined ? id(e.fg) : 0) << 16)) >>> 0);
    if (e.border) words.push(setId(e.border));
    if (e.bar) words.push((id(e.bar.color) | ((e.bar.pct + 128) << 16) | (e.bar.start << 24)) >>> 0);
  }
  const bytes = Buffer.alloc(words.length * 4);
  words.forEach((w, i) => bytes.writeUInt32LE(w >>> 0, i * 4));
  return { pal: palette, ...(table.length > 0 ? { bd: table } : {}), w: bytes.toString("base64") };
}

const RED = "thin:ff0000";

async function open(appearances) {
  const h = await createWorkspaceHarness();
  const d = h.document;
  h.host.CSSStyleSheet = FakeStyleSheet;
  h.host.requestAnimationFrame = (cb) => cb();
  h.host.getComputedStyle = () => ({ getPropertyValue: (p) => (p === "font-size" ? "11pt" : "") });
  const printed = [];
  h.host.print = () => { printed.push(d.getElementById("eo-print-root")); };
  d.adoptedStyleSheets = [];
  switchModule(d, "SHEET");
  await flushAsyncTurns();
  await pickFile(
    d,
    makeFixtureFile("규칙.xlsx", {
      sheets: [{ name: "Sheet1", cells: [
        { ref: "B2", raw: "5", style: { fill: "ffeb9c", fc: "0000ff" } },
        { ref: "C2", raw: "6" },
        { ref: "D2", raw: "7" },
        { ref: "B3", raw: "8" },
        { ref: "D3", raw: "9", style: { bd: { r: "thick:000000" } } },
      ] }],
    })
  );
  await flushAsyncTurns(12);
  // Rows and columns of the packed answer count from 0.
  h.host.__EASYOFFICE_SHEET_RULES__.applyPacked("Sheet1", pack(appearances), true);
  const print = async () => {
    d.defaultView.__EASYOFFICE_NATIVE_MENU__("file:print");
    await flushAsyncTurns(8);
    return printed[printed.length - 1];
  };
  return { h, print, rules: h.host.__EASYOFFICE_SHEET_RULES__ };
}

const cell = (root, row, col) => {
  const rows = [...root.querySelectorAll("tr")];
  return rows[row - 2].querySelectorAll("td")[col - 2];
};

test("the fill and text color of a rule replace the cell's own, and the font flags are added", async () => {
  const t = await open([{ row: 1, col: 1, bg: 0xffc7ce, fg: 0x9c0006, flags: 1 | 2 }, { row: 1, col: 2, flags: 4 | 8 }]);
  const root = await t.print();
  const b2 = cell(root, 2, 2);
  assert.equal(b2.style.backgroundColor, "#ffc7ce", "the cell's own fill ffeb9c is under the rule's");
  assert.equal(b2.style.color, "#9c0006");
  assert.ok(b2.classList.contains("eo-print-b") && b2.classList.contains("eo-print-i"));
  const c2 = cell(root, 2, 3);
  assert.equal(c2.style.backgroundColor, "");
  assert.ok(c2.classList.contains("eo-print-u") && c2.classList.contains("eo-print-s"));
  assert.equal(cell(root, 2, 4).className, "eo-print-num", "a cell no rule touches prints as before");
});

test("a rule's answer prints the same as the grid paints", async () => {
  const t = await open([{ row: 1, col: 1, bg: 0xffc7ce, fg: 0x9c0006, flags: 1 }]);
  const td = t.h.document.querySelector('#pane-sheet td[data-cell="B2"]');
  const root = await t.print();
  const b2 = cell(root, 2, 2);
  assert.equal(b2.style.backgroundColor, td.style.getPropertyValue("--cf-bg"));
  assert.equal(b2.style.color, td.style.getPropertyValue("--cf-fg"));
  assert.equal(td.classList.contains("cf-bold"), b2.classList.contains("eo-print-b"));
});

test("a cell with a rule and nothing else is part of the page and prints its fill", async () => {
  const t = await open([{ row: 2, col: 2, bg: 0x00b050 }]);
  const root = await t.print();
  const c3 = cell(root, 3, 3);
  assert.ok(c3, "C3 holds nothing, the page has it between B3 and D3");
  assert.equal(c3.style.backgroundColor, "#00b050");
  assert.equal(c3.textContent, "");
});

test("the edges of a rule print on the cell, and a neighbour's facing edge reaches the cell beside it", async () => {
  const t = await open([{ row: 2, col: 1, border: { r: RED, b: RED } }]);
  const root = await t.print();
  const b3 = cell(root, 3, 2);
  assert.equal(b3.style.borderRightColor, "#ff0000");
  assert.equal(b3.style.borderRightStyle, "solid");
  assert.equal(b3.style.borderBottomColor, "#ff0000");
  const c3 = cell(root, 3, 3);
  assert.equal(c3.style.borderLeftColor, "#ff0000", "the cell B3 draws its right edge, which is C3's left");
  assert.equal(cell(root, 3, 4).style.borderRightColor, "#000000", "a cell's own edge stays where no rule draws");
});

test("a rule edge goes over the cell's own edge on that side only", async () => {
  const t = await open([{ row: 2, col: 3, border: { r: RED } }]);
  const d3 = cell(await t.print(), 3, 4);
  assert.equal(d3.style.borderRightColor, "#ff0000");
  assert.equal(d3.style.borderRightWidth.length > 0, true);
});

test("a sheet no rule applies to prints as before", async () => {
  const t = await open([]);
  const root = await t.print();
  const b2 = cell(root, 2, 2);
  assert.equal(b2.style.backgroundColor, "#ffeb9c");
  assert.equal(b2.style.color, "#0000ff");
  assert.equal(b2.classList.contains("eo-print-b"), false);
});

test("the look of a cell outside the sheet's grid is none", async () => {
  const t = await open([{ row: 1, col: 1, bg: 0xffc7ce }]);
  for (const [row, col] of [[0, 1], [1, 0], [-1, -1], [Number.NaN, 1], [1.5, 1], [2 ** 31, 1], [1, 2 ** 31]]) {
    assert.equal(t.rules.lookAt("Sheet1", row, col), undefined, `${row},${col}`);
  }
  assert.equal(t.rules.lookAt("Other", 2, 2), undefined);
  assert.equal(t.rules.lookAt("Sheet1", 2, 2).bg, "#ffc7ce");
});

// ---- data bars and icon sets (PD-300) ----------------------------------------------------------------------------------

const BAR = { color: 0x638ec6, pct: 60, start: 10, gradient: false };
const gridCell = (t, ref) => t.h.document.querySelector(`#pane-sheet td[data-cell="${ref}"]`);

test("a data bar prints as the grid draws it, positive and negative, solid and gradient", async () => {
  const negative = { color: 0xff0000, pct: -40, start: 50, gradient: true };
  const t = await open([{ row: 1, col: 1, bar: BAR }, { row: 1, col: 2, bar: negative }]);
  const root = await t.print();
  for (const [ref, col] of [["B2", 2], ["C2", 3]]) {
    const printed = cell(root, 2, col).style.backgroundImage;
    assert.equal(printed, t.rules.barBackground(t.rules.lookAt("Sheet1", 2, col).bar), ref);
    assert.equal(printed, gridCell(t, ref).style.getPropertyValue("--cf-bar"), `${ref}: the grid paints the same bar`);
  }
  assert.match(cell(root, 2, 2).style.backgroundImage, /linear-gradient\(90deg, transparent 10%, #638ec6 10%, #638ec6 64%, transparent 64%\)/);
  assert.match(cell(root, 2, 3).style.backgroundImage, /color-mix/, "a gradient bar fades to a quarter of its color");
  assert.equal(cell(root, 2, 2).textContent, "5", "the value is still in the cell");
});

test("an icon prints the shape and color the grid draws, before the value", async () => {
  const t = await open([{ row: 1, col: 1, icon: { set: 0, index: 2 } }, { row: 1, col: 2, icon: { set: 3, index: 0 } }]);
  const root = await t.print();
  for (const [ref, col] of [["B2", 2], ["C2", 3]]) {
    const printed = cell(root, 2, col);
    const grid = gridCell(t, ref);
    assert.ok(printed.dataset.cfIcon, ref);
    assert.equal(printed.dataset.cfIcon, grid.dataset.cfIcon);
    assert.equal(printed.style.getPropertyValue("--cf-icon-mask"), grid.style.getPropertyValue("--cf-icon-mask"));
    assert.equal(printed.style.getPropertyValue("--cf-icon-color"), grid.style.getPropertyValue("--cf-icon-color"));
  }
  assert.notEqual(cell(root, 2, 2).dataset.cfIcon, cell(root, 2, 3).dataset.cfIcon, "two sets, two shapes");
  assert.equal(cell(root, 2, 2).textContent, "5");
});

test("a value a bar or an icon hides stays in the cell and is not drawn", async () => {
  const t = await open([{ row: 1, col: 1, bar: BAR, hide: true }, { row: 1, col: 2, icon: { set: 0, index: 0 }, hide: true }, { row: 1, col: 3, bar: BAR }]);
  const root = await t.print();
  assert.ok(cell(root, 2, 2).classList.contains("eo-print-cf-hide"));
  assert.ok(cell(root, 2, 3).classList.contains("eo-print-cf-hide"));
  assert.equal(cell(root, 2, 2).textContent, "5");
  assert.equal(cell(root, 2, 4).classList.contains("eo-print-cf-hide"), false, "a bar that shows its value does not hide it");
  assert.equal(gridCell(t, "B2").classList.contains("cf-hide-value"), true, "the grid hides the same cell");
});

test("a bar or an icon with nothing else is part of the page, and a cell with neither prints as before", async () => {
  const t = await open([{ row: 2, col: 2, bar: BAR }]);
  const root = await t.print();
  const c3 = cell(root, 3, 3);
  assert.equal(c3.style.backgroundImage, t.rules.barBackground(t.rules.lookAt("Sheet1", 3, 3).bar));
  assert.equal(cell(root, 2, 2).style.backgroundImage, "");
  assert.equal(cell(root, 2, 2).dataset.cfIcon, undefined);
  assert.equal(cell(root, 2, 2).classList.contains("eo-print-cf-hide"), false);
});

test("an icon index or set the page does not know prints no icon", async () => {
  const t = await open([{ row: 1, col: 1, icon: { set: 31, index: 7 } }]);
  const root = await t.print();
  assert.equal(cell(root, 2, 2).dataset.cfIcon, undefined);
});

test("extended data bar direction, solid border, midpoint/none axis match grid and print cells", async () => {
  const palette = [0x638ec6, 0x112233, 0x445566, 0xff0000, 0x990000];
  // B2 (row=1,col=1): RightToLeft, border=#112233, Midpoint axis=#445566, pct=+60, start=50
  const w0a = 1 | (2 << 20) | (1 << 22) | (1 << 23) | (1 << 25);
  const ha = 1 | (1 << 20);
  const ba = 0 | ((60 + 128) << 16) | (50 << 24);
  const ea = 1 | (2 << 16);
  // C2 (row=1,col=2): LeftToRight, border=#990000, None axis, pct=-50, start=0
  const w0b = 1 | (1 << 20) | (1 << 22) | (2 << 23) | (1 << 25);
  const hb = 2 | (1 << 20);
  const bb = 3 | ((-50 + 128) << 16) | (0 << 24);
  const eb = 4 | (2 << 16);
  const words = [w0a, ha, ba, ea, w0b, hb, bb, eb];
  const bytes = Buffer.alloc(words.length * 4);
  words.forEach((w, i) => bytes.writeUInt32LE(w >>> 0, i * 4));

  const t = await open([]);
  t.rules.applyPacked("Sheet1", { pal: palette, w: bytes.toString("base64") }, true);
  const root = await t.print();

  const b2Print = cell(root, 2, 2);
  const b2Grid = gridCell(t, "B2");
  assert.equal(b2Print.style.backgroundImage, b2Grid.style.getPropertyValue("--cf-bar"));
  assert.equal(b2Print.style.backgroundPosition, b2Grid.style.getPropertyValue("--cf-bar-pos"));
  assert.equal(b2Print.style.backgroundSize, b2Grid.style.getPropertyValue("--cf-bar-size"));
  assert.equal(b2Print.style.backgroundRepeat, "no-repeat");
  assert.match(b2Print.style.backgroundImage, /#445566/);
  assert.match(b2Print.style.backgroundImage, /#112233/);
  assert.match(b2Print.style.backgroundImage, /transparent 20%, #638ec6 20%, #638ec6 50%, transparent 50%/);

  const c2Print = cell(root, 2, 3);
  assert.equal(c2Print.style.backgroundImage, gridCell(t, "C2").style.getPropertyValue("--cf-bar"));
  assert.doesNotMatch(c2Print.style.backgroundImage, /repeating-linear-gradient/, "axis=none omits axis line");
  assert.match(c2Print.style.backgroundImage, /transparent 0%, #ff0000 0%, #ff0000 50%, transparent 50%/);
});
