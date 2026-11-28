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

  // PD-376: 글꼴 > 고급 > OpenType 기능 and the 줄 눈금에 맞춤 box reach the run, its computed style in the real WebKit and the save JSON;
  // the 배율 field has its presets and the unit sits inside the value field.
  ["글꼴 대화상자 OpenType 기능과 줄 눈금 상자가 런과 저장 JSON에 닿고 값 칸에 단위가 든다", async (page) => {
    await typeFirstParagraph(page, "가나다라 1234");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.evaluate(() => document.querySelector('.md-item[data-studio-menu-cmd="font-dialog"]').click());
    await page.waitForSelector("#doc-font-dialog[open]");
    await page.click("#doc-font-tab-advanced");
    assert.deepEqual(await page.$$eval("#doc-font-scale-presets option", (rows) => rows.map((r) => r.value)), ["200", "150", "100", "90", "80", "66", "50", "33"]);
    await page.fill("#doc-font-scale", "150");
    await page.selectOption("#doc-font-spacing-kind", "expanded");
    await page.fill("#doc-font-spacing-value", "");
    await page.keyboard.type("2pt");
    assert.equal(await page.inputValue("#doc-font-spacing-value"), "2", "a number field takes the digits; the unit is the label beside it");
    assert.equal(await page.textContent(".eo-stepper:has(#doc-font-spacing-value) + .dialog-unit"), "pt");
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

  // PD-376, PD-378: every field, combo box and color button of the 글꼴 dialog has the Spectrum S box (24px high, 12px text), the three
  // controls under 한글 글꼴, 글꼴 스타일 and 크기 start on one line with no empty area below them.
  ["글꼴 대화상자의 모든 컨트롤이 Spectrum S의 높이·글자 크기를 쓰고 한글 글꼴·글꼴 스타일·크기가 한 줄에서 시작한다", async (page) => {
    await typeFirstParagraph(page, "가나다라");
    await page.evaluate(() => document.querySelector('.md-item[data-studio-menu-cmd="font-dialog"]').click());
    await page.waitForSelector("#doc-font-dialog[open]");
    for (const tab of ["font", "advanced"]) {
      await page.click(`#doc-font-tab-${tab}`);
      const pane = `#doc-font-pane-${tab}`;
      const boxes = await page.$$eval(`${pane} .eo-stepper, ${pane} .custom-select, ${pane} .dialog-swatch`, (els) => els.filter((el) => el.getBoundingClientRect().height > 0).map((el) => {
        const text = el.matches(".custom-select") ? el.querySelector(".custom-select-value") : el.matches(".dialog-swatch") ? el.querySelector(".dialog-swatch-text") : el.querySelector("input");
        return { id: el.id || el.dataset.selectId, height: el.getBoundingClientRect().height, font: getComputedStyle(text).fontSize };
      }));
      assert.ok(boxes.length >= 4, `${tab}: the controls are found`);
      for (const box of boxes) assert.deepEqual([box.height, box.font], [24, "12px"], `${tab}: ${box.id}`);
    }
    await page.click("#doc-font-tab-font");
    const tops = await page.$$eval('.custom-select[data-select-id="doc-font-family"], .custom-select[data-select-id="doc-font-style"], .eo-stepper:has(#doc-font-size)', (els) => els.map((el) => el.getBoundingClientRect().top));
    assert.equal(new Set(tops).size, 1, `한글 글꼴, 글꼴 스타일 and 크기 start on one line: ${tops}`);
    // 크기 is a 68px field (the 9px text padding on both sides, the 20px button cell and room for four characters such as 10.5) and its spin buttons sit inside it: nothing is cut. The block holds no empty rows: it ends where 영어 글꼴 ends.
    const size = await page.$eval("#doc-font-size", (el) => ({ width: el.parentNode.getBoundingClientRect().width, clipped: el.scrollWidth > el.clientWidth }));
    assert.deepEqual(size, { width: 68, clipped: false });
    const block = await page.evaluate(() => ({
      end: document.querySelector("#doc-font-pane-font .dialog-font-top").getBoundingClientRect().bottom,
      latin: document.querySelector('.custom-select[data-select-id="doc-font-latin"]').getBoundingClientRect().bottom,
      colors: document.querySelector("#doc-font-pane-font .dialog-font-colors").getBoundingClientRect().top,
    }));
    assert.deepEqual([block.end - block.latin, block.colors - block.end], [0, 12], "no empty area under 영어 글꼴, and the next row is a row gap (12px) below");
    // Every group is the same distance from the next on both tabs, the last group of a tab from the preview group included.
    for (const tab of ["font", "advanced"]) {
      await page.click(`#doc-font-tab-${tab}`);
      const gaps = await page.$$eval(`#doc-font-pane-${tab} > fieldset, #doc-font-dialog .doc-font-preview-body fieldset`, (els) => els.slice(1).map((el, i) => Math.round(el.getBoundingClientRect().top - els[i].getBoundingClientRect().bottom)));
      assert.ok(gaps.length >= 1 && gaps.every((gap) => gap === 8), `${tab}: the gaps between groups are 8px (--dialog-group-gap): ${gaps}`);
    }
    // The 고급 tab's boxes all start at one left edge, the 글꼴 커닝 box (96px, like the 값 boxes) included, with its 포인트 이상 label right after it;
    // the label column is as wide as its widest cell (the 글꼴 커닝 check box and label), so every label ends before the boxes start.
    const edges = await page.evaluate(() => {
      const left = (selector) => document.querySelector(selector).getBoundingClientRect().left;
      const stepper = (id) => `#doc-font-pane-advanced .eo-stepper:has(#${id})`;
      const combo = (id) => `#doc-font-pane-advanced .custom-select[data-select-id="${id}"]`;
      const kern = document.querySelector(stepper("doc-font-kerning-value")).getBoundingClientRect();
      const unit = document.querySelector("#doc-font-kerning-unit").getBoundingClientRect();
      const value = document.querySelector(stepper("doc-font-spacing-value")).getBoundingClientRect();
      const labels = [...document.querySelectorAll("#doc-font-pane-advanced .dialog-grid .dialog-label, #doc-font-pane-advanced .dialog-grid-wide .dialog-label")]
        .filter((l) => l.getBoundingClientRect().width > 0 && l.parentElement.firstElementChild === l || l.matches("[for=doc-font-style-set]"));
      return {
        lefts: [stepper("doc-font-scale"), combo("doc-font-spacing-kind"), combo("doc-font-position-kind"), stepper("doc-font-kerning-value"), combo("doc-font-ligatures"), combo("doc-font-num-spacing"), combo("doc-font-style-set")].map(left),
        kernWidth: [kern.width, value.width], unitGap: unit.left - kern.right,
        labelEnds: labels.map((l) => { const r = document.createRange(); r.selectNodeContents(l); return left(stepper("doc-font-scale")) - r.getBoundingClientRect().right; }),
      };
    });
    assert.equal(new Set(edges.lefts.map((x) => Math.round(x * 2))).size, 1, `the boxes of the 고급 tab start at one left edge, the 글꼴 커닝 box included: ${edges.lefts}`);
    assert.deepEqual(edges.kernWidth[0], edges.kernWidth[1], "the 글꼴 커닝 box is as wide as the 값 boxes");
    assert.ok(Math.abs(edges.unitGap - 8) <= 1, `포인트 이상 follows the 글꼴 커닝 box by the 8px dialog gap: ${edges.unitGap}`);
    assert.ok(edges.labelEnds.every((gap) => gap >= 8), `every label ends at least 8px before the boxes: ${edges.labelEnds}`);
  }],

  // PD-377: the number field is the stepper (a press steps and clamps in the real engine, the cell is as high as the field and holds two
  // bordered fills of Spectrum's stacked in-field buttons, PD-378) and the chevrons are drawn and centered, also in the combo box.
  ["숫자 칸의 위·아래 단추가 값을 바꾸고 단추 열과 쉐브론이 칸과 선택 화살표 칸의 가운데에 맞는다", async (page) => {
    await typeFirstParagraph(page, "가나다라");
    await page.evaluate(() => document.querySelector('.md-item[data-studio-menu-cmd="font-dialog"]').click());
    await page.waitForSelector("#doc-font-dialog[open]");
    const buttons = ".eo-stepper:has(#doc-font-size) .eo-stepper-button";
    await page.fill("#doc-font-size", "10");
    await page.click(`${buttons} >> nth=0`);
    assert.equal(await page.inputValue("#doc-font-size"), "10.5", "up steps by half a point");
    await page.click(`${buttons} >> nth=1`);
    await page.click(`${buttons} >> nth=1`);
    assert.equal(await page.inputValue("#doc-font-size"), "9.5", "down steps back");
    await page.fill("#doc-font-size", "1638");
    await page.click(`${buttons} >> nth=0`);
    assert.equal(await page.inputValue("#doc-font-size"), "1638", "max clamps");
    await page.fill("#doc-font-size", "1");
    await page.click(`${buttons} >> nth=1`);
    assert.equal(await page.inputValue("#doc-font-size"), "1", "min clamps");
    await page.mouse.move(0, 0);
    const geometry = await page.evaluate(() => {
      const input = document.querySelector("#doc-font-size");
      const box = input.parentNode;
      const cell = box.querySelector(".eo-stepper-buttons");
      const [up, down] = [...cell.querySelectorAll(".eo-stepper-button")];
      const rect = (el) => el.getBoundingClientRect();
      const middle = (r) => [(r.left + r.right) / 2, (r.top + r.bottom) / 2];
      const style = (el, name) => getComputedStyle(el)[name];
      const inside = (inner, outer) => inner.left >= outer.left && inner.right <= outer.right && inner.top >= outer.top && inner.bottom <= outer.bottom;
      const chevrons = [up, down].map((b) => ({ button: middle(rect(b)), chevron: middle(rect(b.querySelector(".eo-chevron"))) }));
      const arrow = document.querySelector('.custom-select[data-select-id="doc-font-family"] .custom-select-arrow');
      const before = getComputedStyle(arrow, "::before");
      return {
        box: rect(box).height, cell: rect(cell).height, cellWidth: rect(cell).width, cellRight: rect(cell).right, boxRight: rect(box).right,
        native: getComputedStyle(input).appearance,
        // Spectrum's stacked fills: bordered fills on the gray-100 tone, touching (both gaps are 0), the cell's left border is the line in front of
        // them and the lower fill's top border the one line between them (the upper fill has no bottom border); the border color is the field's.
        fills: {
          inCell: [inside(rect(up), rect(cell)), inside(rect(down), rect(cell))],
          halves: [rect(up).height, rect(down).height], gap: rect(down).top - rect(up).bottom,
          cellLeft: style(cell, "borderLeftWidth"), upBorders: [style(up, "borderTopWidth"), style(up, "borderBottomWidth")], downTop: style(down, "borderTopWidth"),
          shadows: [style(up, "boxShadow"), style(down, "boxShadow")],
          colors: [style(cell, "borderLeftColor") === style(box, "borderLeftColor"), style(down, "borderTopColor") === style(box, "borderLeftColor")],
          background: [style(up, "backgroundColor"), style(down, "backgroundColor"), style(box, "backgroundColor")],
          chevron: [style(up.querySelector(".eo-chevron"), "width"), style(down.querySelector(".eo-chevron"), "width")],
        },
        styleBar: (() => {
          // The style bar's size stepper has the same fills: its two buttons are exactly its column, with the line in front and between them.
          const col = document.querySelector("#btn-size-up").parentNode;
          const [high, low] = [document.querySelector("#btn-size-up"), document.querySelector("#btn-size-down")];
          return {
            column: rect(col).width, spans: [rect(high).left - rect(col).left, rect(low).left - rect(col).left, rect(col).right - rect(high).right, rect(col).right - rect(low).right],
            high: rect(high).height, low: rect(low).height, gap: rect(low).top - rect(high).bottom, colLeft: style(col, "borderLeftWidth"), lowTop: style(low, "borderTopWidth"),
            background: style(col, "backgroundColor"),
          };
        })(),
        chevrons,
        arrowLeft: getComputedStyle(arrow).borderLeftWidth, arrowChevron: [before.width, before.height, before.maskImage || before.webkitMaskImage],
        arrowCenter: (() => { const a = rect(arrow); return a.left + a.width / 2; })(),
      };
    });
    assert.ok(Math.abs(geometry.box - geometry.cell - 2) <= 1, `the cell is as high as the field inside its frame: ${geometry.box} ${geometry.cell}`);
    assert.equal(geometry.cellWidth, 20, "the cell has the token width (Spectrum S in-field-button-width-stacked-small)");
    assert.ok(Math.abs(geometry.boxRight - geometry.cellRight - 1) <= 1, "the cell is at the end of the field");
    const { fills } = geometry;
    assert.deepEqual(fills.inCell, [true, true], "each fill is inside the cell");
    assert.equal(fills.halves[0], fills.halves[1], "the two fills are equally high");
    assert.equal(fills.gap, 0, "the fills touch: the gap between them is in-field-button-stacked-inner-edge-to-fill, 0px");
    assert.deepEqual([fills.cellLeft, fills.upBorders, fills.downTop], ["1px", ["0px", "0px"], "1px"], "the line in front is the cell's left border; the upper fill has no top or bottom border of its own (the field's top line is its edge), the lower fill's top border is the one line between");
    assert.deepEqual(fills.shadows, ["none", "none"], "no drawn divider: the lines are borders");
    assert.deepEqual(fills.colors, [true, true], "the lines have the field's border color");
    assert.equal(fills.background[0], fills.background[1], "both fills have the fill tone");
    assert.notEqual(fills.background[0], fills.background[2], "the fill tone differs from the field's");
    assert.deepEqual(fills.chevron, ["6px", "6px"], "the S stepper chevron is chevron-icon-size-50");
    assert.equal(geometry.styleBar.column, 20, "the style bar's stepper column has the token width");
    assert.deepEqual(geometry.styleBar.spans, [1, 1, 0, 0], "the style bar's buttons are exactly its column inside the 1px line in front of it");
    assert.equal(geometry.styleBar.high, geometry.styleBar.low, "the style bar's two buttons are equally high");
    assert.equal(geometry.styleBar.gap, 0, "the style bar's buttons touch");
    assert.deepEqual([geometry.styleBar.colLeft, geometry.styleBar.lowTop], ["1px", "1px"], "the style bar's line in front of the column and the one between its buttons");
    assert.equal(geometry.styleBar.background, fills.background[0], "the style bar's column has the same fill tone");
    assert.equal(geometry.native, "textfield", "the browser's own spin button is gone");
    for (const { button, chevron } of geometry.chevrons) assert.ok(Math.abs(button[0] - chevron[0]) <= 1 && Math.abs(button[1] - chevron[1]) <= 1, `a chevron is centered in its button: ${button} ${chevron}`);
    assert.equal(geometry.arrowLeft, "0px", "no divider in front of the combo box's chevron");
    assert.deepEqual(geometry.arrowChevron.slice(0, 2), ["10px", "10px"]);
    assert.match(geometry.arrowChevron[2], /svg/, "the chevron is an image mask");
  }],

  // PD-376: the 글꼴 탭's colors, underline style, double strikethrough, capitals, hidden text and emphasis reach the run, its computed style
  // in the real WebKit and the save JSON.
  ["글꼴 대화상자 글꼴 탭의 색·밑줄 스타일·이중 취소선·대문자·강조점이 런과 저장 JSON에 닿는다", async (page) => {
    await typeFirstParagraph(page, "가나다라 abc");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("Shift+ArrowLeft");
    await page.evaluate(() => document.querySelector('.md-item[data-studio-menu-cmd="font-dialog"]').click());
    await page.waitForSelector("#doc-font-dialog[open]");
    await page.fill("#doc-font-size", "12");
    await page.selectOption("#doc-font-style", "굵게");
    await page.click("#doc-font-color-host [data-color-toggle]");
    await page.click('#doc-font-color-host .sb-hl-swatch[aria-label="#ff0000"]');
    await page.selectOption("#doc-font-underline-style", "14");
    await page.click("#doc-font-underline-color-host [data-color-toggle]");
    await page.click('#doc-font-underline-color-host .sb-hl-swatch[aria-label="#0000ff"]');
    await page.check("#doc-font-dstrike");
    await page.check("#doc-font-allcaps");
    await page.selectOption("#doc-font-emphasis", "3");
    await page.selectOption("#doc-font-latin", "Pretendard");
    await page.selectOption("#doc-font-family", "바탕");
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
    assert.equal(hit.bold, true, "the chosen 글꼴 스타일 reaches the run");
    assert.deepEqual([hit.font, hit.fontEastAsia], ["Pretendard", "바탕"], "영어 글꼴 and 한글 글꼴 are two run values");
    assert.equal(await page.$eval(`${EDITOR} .eo-run`, (el) => el.style.fontFamily.replaceAll('"', "")), "Pretendard, 바탕", "Latin first, then East Asian");
  }],

  // The Word-style features (PD-395) on the real selection: a section break splits the paragraph at the caret, a bookmark and a
  // field are markers and a chip in the real DOM, and the save payload carries them.
  ["구역 나누기 splits the paragraph at the caret and 책갈피, 날짜 및 시간 and 캡션 reach the save payload", async (page) => {
    await typeFirstParagraph(page, "앞뒤 본문");
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("ArrowLeft");
    await page.evaluate(() => document.querySelector('[data-studio-menu-cmd="section-break-oddPage"]').click());
    assert.deepEqual(await paragraphs(page), ["앞뒤 ", "본문"]);
    assert.deepEqual(await page.$$eval(`${EDITOR} > p[data-eo-section-break]`, (nodes) => nodes.map((p) => [p.textContent, p.dataset.eoSectionBreak])), [["앞뒤 ", "oddPage"]]);
    await page.keyboard.press("Shift+ArrowRight");
    await page.keyboard.press("Shift+ArrowRight");
    await page.evaluate(() => document.querySelector('[data-studio-menu-cmd="insert-bookmark"]').click());
    await page.fill("#doc-bm-name", "본문표시");
    await page.click("#btn-doc-bm-add");
    await page.waitForFunction(() => !document.getElementById("doc-bookmark-dialog").open);
    await page.evaluate(() => document.querySelector('[data-studio-menu-cmd="insert-datetime"]').click());
    await page.check("#doc-dt-update");
    await page.click("#btn-doc-dt-ok");
    await page.waitForSelector(`${EDITOR} [data-eo-field-code^="DATE"]`);
    const blocks = await page.evaluate(() => window.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(document.getElementById("doc-editor")));
    assert.equal(blocks[0].sectionBreak, "oddPage");
    const runs = blocks.flatMap((b) => b.runs ?? []);
    assert.ok(runs.some((r) => r.bookmarkStart === "본문표시") && runs.some((r) => r.bookmarkEnd === "본문표시"), JSON.stringify(runs));
    assert.ok(runs.some((r) => r.fieldCode?.startsWith("DATE")), "the date is a field of the payload");
  }],
];
