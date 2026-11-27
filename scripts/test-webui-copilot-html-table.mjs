import assert from "node:assert/strict";
import test from "node:test";
import { open, answer } from "./test-support/copilot-open.mjs";

// The HTML table a copilot answer carries as text is read in one pass over the text. The first version used lazy regular
// expressions that scan to the end of the text from every `<tr>` and `<td>` that has no end tag, which is quadratic.

const MAX_TABLE_ROWS = 64;
const MAX_TABLE_COLS = 16;

/** The regular-expression reader this one replaced: the oracle for the inputs that both can answer. */
function oracle(rawHtml) {
  const trimmed = String(rawHtml ?? "").trim();
  if (!/^<table\b/i.test(trimmed)) return null;
  const rows = [];
  let headerRow = false;
  const trRe = /<tr\b[^>]*>([\s\S]*?)<\/tr>/gi;
  let trMatch;
  while ((trMatch = trRe.exec(trimmed)) !== null && rows.length < MAX_TABLE_ROWS) {
    const rowBody = trMatch[1];
    const cellRe = /<(th|td)\b[^>]*>([\s\S]*?)<\/\1>/gi;
    const row = [];
    let cellMatch;
    while ((cellMatch = cellRe.exec(rowBody)) !== null && row.length < MAX_TABLE_COLS) {
      if (rows.length === 0 && cellMatch[1].toLowerCase() === "th") headerRow = true;
      row.push(cellMatch[2].trim());
    }
    if (row.length > 0) rows.push(row);
  }
  return rows.length > 0 ? { rows, headerRow } : null;
}

async function parser() {
  const h = await open({ module: "DOC", run: () => answer([]) });
  return h.host.__EASYOFFICE_COPILOT_DOC_HTML__.parseHtmlTableString;
}

const SAMPLES = [
  "<table><tr><th>이름</th><th>값</th></tr><tr><td>가</td><td>1</td></tr></table>",
  "  <TABLE class='a'>\n<TR ID=1><TD>A</TD><TD colspan=2>B <b>굵게</b></TD></TR>\n</TABLE>",
  "<table><tr><td>a</td></tr><tr></tr><tr><th>late header</th></tr></table>",
  "<table><tr><td>a<td>b</td></tr></table>",
  "<table><tr><td>one</td><th>two</th></tr></table>",
  "<table><tr><td>a</td></tr><tr><td>unclosed",
  "<table><tr><td>a</td>",
  "<table><tr><td>x</td></tr><tr x='>'><td>y</td></tr></table>",
  "<table><trx><td>no</td></trx><tr><td>yes</td></tr></table>",
  "<table><tr><td>a</td></tr></table><p>after</p>",
  "<table><thead><tr><th>h</th></tr></thead><tbody><tr><td>c</td></tr></tbody></table>",
  "<table></table>",
  "<p>not a table</p>",
  "",
  "<table><tr><td>a</td></tr",
  `<table>${"<tr><td>r</td></tr>".repeat(80)}</table>`,
  `<table><tr>${"<td>c</td>".repeat(30)}</tr></table>`,
  "<table><tr><td>nested <table><tr><td>inner</td></tr></table> end</td></tr></table>",
];

test("the one-pass reader answers what the regular-expression reader answered", async () => {
  const parse = await parser();
  for (const sample of SAMPLES) {
    assert.deepEqual(parse(sample), oracle(sample), JSON.stringify(sample).slice(0, 120));
  }
  assert.equal(parse(SAMPLES[0]).headerRow, true);
  assert.equal(parse(`<table>${"<tr><td>r</td></tr>".repeat(80)}</table>`).rows.length, MAX_TABLE_ROWS);
});

test("many rows and cells without an end tag are read in linear time", async () => {
  const parse = await parser();
  const unclosed = [
    `<table>${"<tr><td>x".repeat(40_000)}</table>`,
    `<table><tr>${"<td>x".repeat(80_000)}</tr></table>`,
    `<table>${"<tr ".repeat(80_000)}</table>`,
    `<table><tr>${"<td ".repeat(80_000)}</tr></table>`,
    `<table>${"<tr><td>a</td>".repeat(40_000)}</table>`,
  ];
  for (const input of unclosed) {
    const started = Date.now();
    parse(input);
    const took = Date.now() - started;
    assert.ok(took < 1000, `${input.length} characters took ${took} ms`);
  }
});
