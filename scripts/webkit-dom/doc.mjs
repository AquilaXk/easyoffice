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

  // PD-366: the line-height of 최소, 고정 and 줄 사이 is a CSS length the browser really lays out.
  ["최소, 고정 and 줄 사이 line spacing lays the lines out at their lengths", async (page) => {
    await page.evaluate(() => {
      document.querySelector("#doc-editor").innerHTML = ["a", "b", "c", "d"].map((t) => `<p><span>${t}${"가".repeat(3)}<br>${t}${"나".repeat(3)}</span></p>`).join("");
    });
    const { heights, fontPx } = await page.evaluate(() => {
      const kinds = window.__EASYOFFICE_DOC_LINE_SPACING__;
      const paras = [...document.querySelectorAll("#doc-editor > p")];
      kinds.writeSpecial(paras[0], "exact", 600); // 40px
      kinds.writeSpecial(paras[1], "atLeast", 90); // 6px, less than one line of the text
      kinds.writeSpecial(paras[2], "atLeast", 900); // 60px
      kinds.writeSpecial(paras[3], "between", 300); // one line plus 20px
      const base = document.createElement("p");
      base.style.lineHeight = "1";
      base.innerHTML = "가<br>나";
      document.querySelector("#doc-editor").appendChild(base);
      return { heights: [...paras, base].map((p) => p.getBoundingClientRect().height), fontPx: Number.parseFloat(getComputedStyle(base).fontSize) };
    });
    const [exact, atLeastSmall, atLeastLarge, between, single] = heights;
    assert.ok(Math.abs(single - 2 * fontPx) < 1.5, `one line is the font size: ${single} vs ${fontPx}`);
    assert.ok(Math.abs(exact - 80) < 1.5, `exact 40px x 2 lines: ${exact}`);
    assert.ok(Math.abs(atLeastSmall - single) < 1.5, `at least 6px does not shrink the line: ${atLeastSmall} vs ${single}`);
    assert.ok(Math.abs(atLeastLarge - 120) < 1.5, `at least 60px x 2 lines: ${atLeastLarge}`);
    assert.ok(Math.abs(between - (single + 40)) < 1.5, `one line plus 20px per line: ${between} vs ${single}`);
  }],

  // PD-372: 글꼴 > 고급 sets 배율, 위치 and 글꼴 커닝 on the selected run; the real WebKit raises it and the save JSON carries them.
  ["글꼴 대화상자 고급 탭의 배율·위치·글꼴 커닝이 런과 저장 JSON에 닿는다", async (page) => {
    await typeFirstParagraph(page, "가나다라");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.evaluate(() => document.querySelector('.md-item[data-studio-menu-cmd="font-dialog"]').click());
    await page.waitForSelector("#doc-font-dialog[open]");
    await page.click("#doc-font-tab-advanced");
    await page.fill("#doc-font-scale", "150");
    await page.selectOption("#doc-font-position-kind", "raised");
    await page.fill("#doc-font-position-value", "4");
    await page.check("#doc-font-kerning");
    await page.fill("#doc-font-kerning-value", "12");
    await page.click("#btn-doc-font-apply");
    await page.waitForSelector(`${EDITOR} .eo-run`);
    const run = await page.$eval(`${EDITOR} .eo-run`, (el) => {
      const style = getComputedStyle(el);
      const plain = el.parentElement.firstChild;
      const range = document.createRange();
      range.selectNodeContents(plain);
      return {
        data: [el.dataset.eoScale, el.dataset.eoPosition, el.dataset.eoKerning],
        stretch: style.fontStretch,
        kerning: style.fontKerning,
        raised: range.getBoundingClientRect().bottom - el.getBoundingClientRect().bottom,
      };
    });
    assert.deepEqual(run.data, ["150", "8", "24"]);
    assert.equal(run.stretch, "150%");
    assert.equal(run.kerning, "normal");
    assert.ok(run.raised > 4.5, `the run sits 4 pt (5.3 px) above its neighbour's baseline: ${run.raised}`);
    const runs = await page.evaluate(() => window.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(document.querySelector("#doc-editor")).flatMap((b) => b.runs || []));
    assert.deepEqual(runs.filter((r) => r.scalePercent).map((r) => [r.text, r.scalePercent, r.positionHalfPt, r.kerningHalfPt]), [["다라", 150, 8, 24]]);
  }],

  // PD-374: 글꼴 > 고급 > OpenType 기능 and the 줄 눈금에 맞춤 box reach the run, its computed style in the real WebKit and the save JSON;
  // the 배율 combo opens its preset list and the unit sits inside the value field.
  ["글꼴 대화상자 OpenType 기능과 줄 눈금 상자가 런과 저장 JSON에 닿고 값 칸에 단위가 든다", async (page) => {
    await typeFirstParagraph(page, "가나다라 1234");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.evaluate(() => document.querySelector('.md-item[data-studio-menu-cmd="font-dialog"]').click());
    await page.waitForSelector("#doc-font-dialog[open]");
    await page.click("#doc-font-tab-advanced");
    await page.click("#doc-font-pane-advanced .dialog-combo-arrow");
    assert.deepEqual(await page.$$eval("#doc-font-pane-advanced .dialog-combo-list li", (rows) => rows.map((r) => r.textContent)), ["200%", "150%", "100%", "90%", "80%", "66%", "50%", "33%"]);
    await page.click("#doc-font-pane-advanced .dialog-combo-list li:nth-child(2)");
    assert.equal(await page.inputValue("#doc-font-scale"), "150%");
    await page.selectOption("#doc-font-spacing-kind", "expanded");
    await page.fill("#doc-font-spacing-value", "2");
    await page.keyboard.press("Tab");
    assert.equal(await page.inputValue("#doc-font-spacing-value"), "2pt", "a bare number takes its unit");
    await page.selectOption("#doc-font-num-spacing", "2");
    await page.selectOption("#doc-font-num-form", "1");
    await page.selectOption("#doc-font-style-set", "3");
    await page.uncheck("#doc-font-grid");
    await page.click("#btn-doc-font-apply");
    await page.waitForSelector(`${EDITOR} .eo-run`);
    const style = await page.$eval(`${EDITOR} .eo-run`, (el) => ({ numeric: getComputedStyle(el).fontVariantNumeric, features: getComputedStyle(el).fontFeatureSettings }));
    assert.match(style.numeric, /lining-nums/);
    assert.match(style.numeric, /tabular-nums/);
    assert.match(style.features, /ss03/);
    const runs = await page.evaluate(() => window.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(document.querySelector("#doc-editor")).flatMap((b) => b.runs || []));
    const hit = runs.filter((r) => r.openType);
    assert.equal(hit.length, 1);
    assert.equal(hit[0].snapToGrid, false);
    assert.equal(hit[0].letterSpacingTwips, 40);
    assert.equal(hit[0].scalePercent, 150);
  }],

  // PD-374: the 글꼴 탭's colors, underline style, double strikethrough, capitals, hidden text and emphasis reach the run, its computed style
  // in the real WebKit and the save JSON.
  ["글꼴 대화상자 글꼴 탭의 색·밑줄 스타일·이중 취소선·대문자·강조점이 런과 저장 JSON에 닿는다", async (page) => {
    await typeFirstParagraph(page, "가나다라 abc");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.evaluate(() => document.querySelector('.md-item[data-studio-menu-cmd="font-dialog"]').click());
    await page.waitForSelector("#doc-font-dialog[open]");
    await page.click("#doc-font-size ~ .dialog-combo-list li:nth-child(6)");
    assert.equal(await page.inputValue("#doc-font-size"), "12", "a row of the size list fills the field");
    await page.click("#doc-font-color-host [data-color-toggle]");
    await page.click('#doc-font-color-host .sb-hl-swatch[aria-label="#ff0000"]');
    await page.selectOption("#doc-font-underline-style", "14");
    await page.click("#doc-font-underline-color-host [data-color-toggle]");
    await page.click('#doc-font-underline-color-host .sb-hl-swatch[aria-label="#0000ff"]');
    await page.check("#doc-font-dstrike");
    await page.check("#doc-font-allcaps");
    await page.selectOption("#doc-font-emphasis", "3");
    await page.click("#btn-doc-font-apply");
    await page.waitForSelector(`${EDITOR} .eo-run`);
    const style = await page.$eval(`${EDITOR} .eo-run`, (el) => {
      const s = getComputedStyle(el);
      return { size: s.fontSize, color: s.color, line: s.textDecorationLine, kind: s.textDecorationStyle, lineColor: s.textDecorationColor, transform: s.textTransform, mark: s.textEmphasisStyle };
    });
    assert.equal(style.size, "16px");
    assert.equal(style.color, "rgb(255, 0, 0)");
    assert.match(style.line, /underline/);
    assert.match(style.line, /line-through/);
    assert.equal(style.kind, "wavy");
    assert.equal(style.lineColor, "rgb(0, 0, 255)");
    assert.equal(style.transform, "uppercase");
    assert.match(style.mark, /circle/);
    const runs = await page.evaluate(() => window.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(document.querySelector("#doc-editor")).flatMap((b) => b.runs || []));
    const hit = runs.find((r) => r.textFx);
    assert.equal(hit.text, "abc");
    assert.equal(hit.underline, true);
    assert.equal(hit.strike, undefined, "a double line is not a single one");
    assert.equal(hit.underlineColor, "0000FF");
    assert.equal(hit.color, "FF0000");
    assert.equal(hit.sizeHalfPt, 24);
  }],
];
