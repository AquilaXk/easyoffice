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
    words.push((e.row | (e.border ? 0x80000000 : 0)) >>> 0, head >>> 0);
    if (e.bg !== undefined || e.fg !== undefined) words.push(((e.bg !== undefined ? id(e.bg) : 0) | ((e.fg !== undefined ? id(e.fg) : 0) << 16)) >>> 0);
    if (e.border) words.push(setId(e.border));
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
