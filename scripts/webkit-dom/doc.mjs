import assert from "node:assert/strict";
import { paste } from "./support.mjs";

// The document steps of the WebKit suite: real selection, caret and key handling of the editor surface.

const EDITOR = "#doc-editor";
const paragraphs = (page) => page.$$eval(`${EDITOR} > p:not(.manuscript-notice)`, (nodes) => nodes.map((p) => p.textContent));

// What an office suite puts on the clipboard: a namespace, a style block, `o:p` tags, a fragment marker.
const OFFICE_HTML = `<html xmlns:o="urn:schemas-microsoft-com:office:office"><head><style><!-- p.MsoNormal { margin:0cm; font-size:11.0pt } --></style></head><body lang=KO><!--StartFragment--><p class=MsoNormal><b><span style='font-size:14.0pt;color:#C00000'>제목</span></b><o:p></o:p></p><p class=MsoNormal>본문<o:p></o:p></p><!--EndFragment--></body></html>`;

async function typeFirstParagraph(page, text) {
  await page.click(`${EDITOR} p`);
  await page.keyboard.type(text);
}

async function caretCell(page) {
  return page.evaluate(() => {
    const node = getSelection().anchorNode;
    const el = node?.nodeType === 1 ? node : node?.parentElement;
    return el?.closest("td")?.textContent ?? null;
  });
}

export const docSteps = [
  ["an Office HTML paste keeps the runs and drops the markup around them", async (page) => {
    await typeFirstParagraph(page, "앞");
    await paste(page, { "text/html": OFFICE_HTML, "text/plain": "제목\n본문" });
    await page.waitForFunction(() => document.querySelector("#doc-editor .eo-run"));
    assert.deepEqual(await paragraphs(page), ["앞제목", "본문"]);
    const run = await page.$eval(`${EDITOR} .eo-run`, (el) => ({ text: el.textContent, bold: el.dataset.eoBold, color: el.dataset.eoColor, size: el.dataset.eoSize }));
    assert.deepEqual(run, { text: "제목", bold: "true", color: "C00000", size: "28" });
    assert.equal(await page.$eval(EDITOR, (el) => /Mso|<o:p|StartFragment/.test(el.innerHTML)), false, "no Office markup is left");
  }],

  ["a paste over a selected text replaces it", async (page) => {
    await typeFirstParagraph(page, "앞뒤 본문 글");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("Shift+ArrowLeft");
    assert.equal(await page.evaluate(() => getSelection().toString()), " 글");
    await paste(page, { "text/plain": "교체" });
    await page.waitForFunction(() => document.querySelector("#doc-editor p")?.textContent === "앞뒤 본문교체");
    assert.deepEqual(await paragraphs(page), ["앞뒤 본문교체"]);
  }],

  ["Tab walks the cells of a table and adds a row after the last one", async (page) => {
    await typeFirstParagraph(page, "앞");
    await paste(page, { "text/html": "<table><tr><td>a</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>", "text/plain": "a\tb\nc\td" });
    await page.waitForSelector(`${EDITOR} table td`);
    await page.click(`${EDITOR} table tr:first-child td:first-child`);
    const seen = [];
    for (let i = 0; i < 3; i += 1) {
      await page.keyboard.press("Tab");
      seen.push(await caretCell(page));
    }
    assert.deepEqual(seen, ["b", "c", "d"]);
    assert.equal(await page.$$eval(`${EDITOR} table tr`, (rows) => rows.length), 2);
    await page.keyboard.press("Tab");
    assert.equal(await page.$$eval(`${EDITOR} table tr`, (rows) => rows.length), 3, "a new row after the last cell");
    await page.keyboard.press("Shift+Tab");
    assert.equal(await caretCell(page), "d", "Shift+Tab goes back to the previous cell");
  }],
];
