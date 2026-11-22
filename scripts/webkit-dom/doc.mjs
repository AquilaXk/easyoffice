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

  ["Word list markers and GIF/SVG data images paste cleanly in WebKit", async (page) => {
    await page.evaluate(() => {
      const ed = document.querySelector("#doc-editor");
      ed.innerHTML = "<p><br></p>";
      const r = document.createRange();
      r.setStart(ed.querySelector("p"), 0);
      r.collapse(true);
      const s = getSelection();
      s.removeAllRanges();
      s.addRange(r);
      ed.focus();
    });
    const gif1x1 = "R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
    const svgB64 = Buffer.from('<svg xmlns="http://www.w3.org/2000/svg" width="12" height="10"><rect width="12" height="10"/></svg>', "utf8").toString("base64");
    const html = [
      '<p class="MsoListParagraph" style="mso-list:l0 level1 lfo1"><![if !supportLists]><span>(가)</span><![endif]>가나다 항목<o:p></o:p></p>',
      '<p class="MsoListParagraph" style="mso-list:l1 level1 lfo2"><![if !supportLists]><span style="font-family:Wingdings">l</span><![endif]>글머리 항목<o:p></o:p></p>',
      `<p><img src="data:image/gif;base64,${gif1x1}"><img src="data:image/svg+xml;base64,${svgB64}"></p>`,
    ].join("");
    await paste(page, { "text/html": html, "text/plain": "가나다 항목\n글머리 항목" });
    await page.waitForFunction(() => document.querySelectorAll("#doc-editor img.doc-inline-image").length === 2);
    const result = await page.evaluate(() => ({
      olNum: document.querySelector("#doc-editor ol > li")?.dataset?.eoListNum || null,
      olText: document.querySelector("#doc-editor ol > li")?.textContent || "",
      ulText: document.querySelector("#doc-editor ul > li")?.textContent || "",
      imgFormats: Array.from(document.querySelectorAll("#doc-editor img.doc-inline-image")).map((im) => im.dataset.eoImageFormat),
    }));
    assert.deepEqual(result, {
      olNum: "hangul:paren",
      olText: "가나다 항목",
      ulText: "글머리 항목",
      imgFormats: ["png", "png"],
    });
  }],
];
