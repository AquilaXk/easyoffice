import assert from "node:assert/strict";
import os from "node:os";
import path from "node:path";

// The copilot's pointer on a slide and on a range of paragraphs (PD-304), in a real WebKit with real layout. The model's
// answers are a fixture: the page's `run_smart_assistant` calls for `copilot_run` and `copilot_list_models` are answered
// here and every other request goes to the real engine. What is measured is where the pointer's boxes really are.

const PANEL_MODEL = "#btn-copilot-model";
const ACCEPT = '#copilot-thread button[data-copilot-action="accept"]';
const KEEP = '#copilot-thread button[data-copilot-action="keep"]';
const REVIEW_BUTTONS = `${ACCEPT}, ${KEEP}`;
const CURSOR = "#eo-copilot-cursor-layer";
const FULL_RUN_MS = 25000;

/** Answers the copilot's requests from `answers` (one per `copilot_run`, in order). */
async function fakeProvider(page, answers) {
  await page.route("**/api/ipc", async (route) => {
    const body = JSON.parse(route.request().postData() || "{}");
    const payload = body.command === "run_smart_assistant" ? JSON.parse(body.payload || "{}") : {};
    if (payload.action === "copilot_run") return route.fulfill({ json: { status: "ok", mode: "edit", reply: "고쳤습니다", updatedInPlace: true, locations: [], operations: answers.shift() } });
    if (payload.action === "copilot_list_models") return route.fulfill({ json: { status: "ok", models: [{ id: "fixture-model", name: "fixture-model" }] } });
    return route.continue();
  });
}

/**
 * Opens the panel, picks a local provider (no key needed) and sends `prompt`; resolves when the answer's buttons are on screen:
 * 바꾸기 for an edit of what is there (previewed), 유지 for new content (already typed into the document as a draft, PD-386).
 */
export async function askCopilot(page, prompt, answers) {
  if (answers) await fakeProvider(page, answers);
  if (!(await page.isVisible(PANEL_MODEL))) await page.click("#btn-copilot-toggle");
  const configured = await page.$eval(PANEL_MODEL, (el) => !/모델 선택/.test(el.textContent));
  if (!configured) {
    await page.click(PANEL_MODEL);
    await page.click('#copilot-model-menu button[data-quick-kind="ollama"]');
  }
  await page.fill("#copilot-input", prompt);
  await page.click("#btn-copilot-send");
  await page.waitForSelector(REVIEW_BUTTONS);
}

/** 바꾸기 for a previewed edit; a draft is being typed in already. Resolves once the pointer shows. */
export async function acceptChange(page) {
  if (await page.$(ACCEPT)) await page.click(ACCEPT);
  await page.waitForSelector(`${CURSOR}:not([hidden]) .eo-copilot-label`);
}

export const finishPlay = (page) => page.waitForSelector(CURSOR, { state: "detached", timeout: FULL_RUN_MS });

/** 실행 취소 of the latest answer (a draft is kept first: the chip comes after the decision). */
export async function undoLast(page) {
  if (await page.$(KEEP)) await page.click(KEEP);
  await page.click("#copilot-thread .eo-copilot-undo-btn >> nth=-1");
}

const boxOf = (page, selector) => page.$eval(selector, (el) => {
  const r = el.getBoundingClientRect();
  return { left: r.left, top: r.top, width: r.width, height: r.height };
});
const near = (a, b, slack, label) => assert.ok(Math.abs(a - b) <= slack, `${label}: ${a} is not within ${slack} of ${b}`);

const textbox = (text, extra = {}) => ({ kind: "textbox", x: 120, y: 160, w: 360, h: 90, text, ...extra });

export const copilotSlideSteps = [
  ["the pointer draws a text box over the stage, selects it where it really is, types into it, and one undo removes it", async (page) => {
    const text = "포인터가 그린 글상자에 쓰는 내용";
    await askCopilot(page, "글상자를 넣어 주세요", [[{ op: "slide.add_element", element: textbox(text) }]]);
    assert.equal(await page.locator(ACCEPT).count(), 0, "new content has no 바꾸기: it is typed in as a draft");
    await acceptChange(page);
    assert.equal(await page.textContent(`${CURSOR} .eo-copilot-label`), "코파일럿");
    // The box stands where the answer says, as a share of the stage, and the selection is drawn over exactly that box.
    await page.waitForFunction((full) => {
      const t = document.querySelector("#slide-canvas-layer .slide-el-textbox")?.textContent || "";
      return t.length > 0 && t.length < full;
    }, text.length);
    const stage = await boxOf(page, "#slide-stage-box");
    const card = await boxOf(page, "#slide-canvas-layer .slide-el");
    near(card.left, stage.left + (120 / 960) * stage.width, 2, "card left");
    near(card.top, stage.top + (160 / 540) * stage.height, 2, "card top");
    near(card.width, (360 / 960) * stage.width, 2, "card width");
    const selection = await boxOf(page, `${CURSOR} .eo-copilot-selection`);
    near(selection.left, card.left, 2, "selection left");
    near(selection.top, card.top, 2, "selection top");
    near(selection.width, card.width, 2, "selection width");
    await finishPlay(page);
    assert.equal(await page.locator("#slide-canvas-layer .slide-el").count(), 1);
    assert.equal(await page.textContent("#slide-canvas-layer .slide-el-textbox"), text);
    await undoLast(page);
    await page.waitForFunction(() => document.querySelectorAll("#slide-canvas-layer .slide-el").length === 0);
  }],

  ["the pointer grabs a text box and carries it to its new place, and the old one is hidden meanwhile", async (page) => {
    const answers = [
      [{ op: "slide.add_element", element: textbox("옮길 글상자", { x: 60, y: 80 }) }],
      [{ op: "slide.update", elements: [{ id: "el-1", ...textbox("옮길 글상자", { x: 520, y: 340 }) }] }],
    ];
    await askCopilot(page, "넣어 주세요", answers);
    await acceptChange(page);
    await finishPlay(page);
    await page.click(KEEP);
    await askCopilot(page, "오른쪽 아래로 옮겨 주세요");
    const before = await boxOf(page, "#slide-canvas-layer .slide-el");
    await acceptChange(page);
    await page.waitForSelector("#slide-canvas-layer .slide-el.eo-copilot-ghost", { state: "attached" });
    assert.equal(await page.locator("#slide-canvas-layer .slide-el:not(.eo-copilot-ghost)").count(), 1, "one copy is on the stage while the original is hidden");
    await finishPlay(page);
    const after = await boxOf(page, "#slide-canvas-layer .slide-el");
    assert.ok(after.left > before.left + 100 && after.top > before.top + 100, "the box is at its new place");
    await undoLast(page);
    await page.waitForFunction((left) => Math.abs(document.querySelector("#slide-canvas-layer .slide-el").getBoundingClientRect().left - left) < 2, before.left);
  }],

  ["a new slide opens as a blank thumbnail with its layout, its title is typed, and the real slide replaces it", async (page) => {
    const title = "새 슬라이드의 제목";
    await askCopilot(page, "슬라이드를 추가해 주세요", [[{ op: "slide.add", title, bullets: ["첫째", "둘째"], layout: "twoColumn" }]]);
    assert.equal(await page.locator("#slide-thumb-list .slide-thumb").count(), 1);
    await acceptChange(page);
    await page.waitForFunction(() => document.querySelectorAll("#slide-thumb-list .slide-thumb").length === 2);
    assert.equal(await page.$eval("#slide-stage-box", (el) => el.dataset.layout), "twoColumn");
    await page.waitForFunction((full) => {
      const t = document.getElementById("slide-title-zone").textContent.trim();
      return t.length > 0 && t.length < full;
    }, title.length);
    await finishPlay(page);
    assert.equal(await page.locator("#slide-thumb-list .slide-thumb").count(), 2);
    assert.equal((await page.textContent("#slide-title-zone")).trim(), title);
    await undoLast(page);
    await page.waitForFunction(() => document.querySelectorAll("#slide-thumb-list .slide-thumb").length === 1);
  }],
];

const PARAGRAPHS = ["첫째 문단입니다", "둘째 문단은 바뀔 범위의 처음입니다", "셋째 문단은 바뀔 범위의 끝입니다", "넷째 문단입니다"];
const paragraphTexts = (page) => page.$$eval("#doc-editor > p:not(.manuscript-notice):not(.eo-copilot-ghost)", (nodes) => nodes.map((p) => p.textContent));

const buttonLabels = (page) => page.$$eval("#copilot-thread .eo-copilot-review-btn", (nodes) => nodes.map((b) => b.textContent.trim()));
// The panel's button states are kept as screenshots in the capture directory (PD-386).
const shot = (page, name) => page.locator("#eo-copilot-shell").screenshot({ path: path.join(process.argv[2] || os.tmpdir(), `copilot-edit-${name}.png`) });
const seedParagraphs = (page) => page.evaluate((texts) => {
  document.getElementById("doc-editor").innerHTML = texts.map((t) => `<p>${t}</p>`).join("");
}, PARAGRAPHS);

export const copilotDocSteps = [
  ["an edit of what is there waits as 바꾸기 / 아래에 삽입 / 버리기, and 아래에 삽입 keeps the original paragraph", async (page) => {
    await seedParagraphs(page);
    await askCopilot(page, "둘째 문단을 다시 써 주세요", [[{ op: "doc.replace_paragraphs", replacements: [{ index: 1, text: "다시 쓴 둘째 문단" }] }]]);
    assert.deepEqual(await buttonLabels(page), ["바꾸기", "아래에 삽입", "버리기"]);
    assert.deepEqual(await paragraphTexts(page), PARAGRAPHS, "the document is untouched until a button is pressed");
    await shot(page, "review-buttons");
    await page.click('#copilot-thread button[data-copilot-action="insert-below"]');
    await page.waitForFunction((n) => document.querySelectorAll("#doc-editor > p:not(.manuscript-notice)").length === n, PARAGRAPHS.length + 1);
    assert.deepEqual(await paragraphTexts(page), [PARAGRAPHS[0], PARAGRAPHS[1], "다시 쓴 둘째 문단", PARAGRAPHS[2], PARAGRAPHS[3]]);
    await shot(page, "inserted-below");
    await page.click("#copilot-thread .eo-copilot-undo-btn >> nth=-1");
    await page.waitForFunction((n) => document.querySelectorAll("#doc-editor > p:not(.manuscript-notice)").length === n, PARAGRAPHS.length);
    assert.deepEqual(await paragraphTexts(page), PARAGRAPHS);
  }],

  ["new content is typed in as a draft with 유지 / 버리기, and 버리기 puts the document back", async (page) => {
    await seedParagraphs(page);
    await askCopilot(page, "문단을 추가해 주세요", [[{ op: "doc.append", text: "새로 덧붙인 문단" }]]);
    assert.equal(await page.locator(ACCEPT).count(), 0, "no 바꾸기 and no preview for new content");
    await finishPlay(page);
    assert.deepEqual(await paragraphTexts(page), [...PARAGRAPHS, "새로 덧붙인 문단"]);
    assert.deepEqual(await buttonLabels(page), ["유지", "버리기"]);
    await shot(page, "draft-buttons");
    await page.click('#copilot-thread button[data-copilot-action="discard"]');
    await page.waitForFunction((n) => document.querySelectorAll("#doc-editor > p:not(.manuscript-notice)").length === n, PARAGRAPHS.length);
    assert.deepEqual(await paragraphTexts(page), PARAGRAPHS);
    assert.match(await page.textContent("#copilot-thread"), /버림/);
    await shot(page, "draft-discarded");
  }],

  ["the pointer selects a range of paragraphs across their real lines before it types the replacement", async (page) => {
    await page.evaluate((texts) => {
      document.getElementById("doc-editor").innerHTML = texts.map((t) => `<p>${t}</p>`).join("");
    }, PARAGRAPHS);
    const op = { op: "doc.replace_paragraphs", startIndex: 1, endIndex: 2, blocks: [{ text: "새로 쓴 둘째 문단" }, { text: "새로 쓴 셋째 문단" }] };
    await askCopilot(page, "둘째와 셋째 문단을 바꿔 주세요", [[op]]);
    assert.deepEqual(await paragraphTexts(page), PARAGRAPHS, "the document is untouched before 바꾸기");
    await acceptChange(page);
    // While the pointer drags, the selection boxes cover both old paragraphs: two lines at different heights.
    await page.waitForFunction(() => {
      const tops = new Set(Array.from(document.querySelectorAll("#eo-copilot-cursor-layer .eo-copilot-selection")).map((el) => Math.round(el.getBoundingClientRect().top)));
      return tops.size >= 2;
    });
    await page.waitForFunction(() => document.querySelectorAll("#doc-editor > p.eo-copilot-ghost").length === 2);
    await finishPlay(page);
    assert.deepEqual(await paragraphTexts(page), [PARAGRAPHS[0], "새로 쓴 둘째 문단", "새로 쓴 셋째 문단", PARAGRAPHS[3]]);
    await undoLast(page);
    await page.waitForFunction((texts) => Array.from(document.querySelectorAll("#doc-editor > p:not(.manuscript-notice)")).map((p) => p.textContent).join("|") === texts.join("|"), PARAGRAPHS);
  }],

  ["text prediction renders outside the editor, dismisses on Escape, accepts on Tab, and chat Markdown renders safe tables", async (page) => {
    await page.route("**/api/ipc", async (route) => {
      const body = JSON.parse(route.request().postData() || "{}");
      const payload = body.command === "run_smart_assistant" ? JSON.parse(body.payload || "{}") : {};
      if (payload.action === "copilot_list_models") {
        return route.fulfill({ json: { status: "ok", models: [{ id: "fixture-model", name: "fixture-model" }] } });
      }
      if (payload.action === "copilot_run") {
        if (payload.selection?.range === "텍스트 예측") {
          return route.fulfill({ json: { status: "ok", mode: "ask", reply: "이어지는 예측 구절입니다.", operations: [], locations: [] } });
        }
        return route.fulfill({
          json: {
            status: "ok",
            mode: "ask",
            reply: "### 검토 결과\n핵심 수식은 `SUM(B2:B5)` 입니다.\n\n| 항목 | 수치 |\n| --- | --- |\n| 매출 | 120 |",
            operations: [],
            locations: [],
          },
        });
      }
      return route.continue();
    });

    if (!(await page.isVisible(PANEL_MODEL))) await page.click("#btn-copilot-toggle");
    await page.click(PANEL_MODEL);
    await page.click('#copilot-model-menu button[data-quick-kind="ollama"]');

    // 텍스트 예측은 ⋮ 메뉴의 켜기/끄기 항목이다(PD-387).
    await page.click("#btn-copilot-more");
    assert.equal((await page.textContent("#btn-copilot-ghost")).trim(), "텍스트 예측");
    await page.click("#btn-copilot-ghost");
    assert.equal(await page.getAttribute("#btn-copilot-ghost", "aria-pressed"), "true");
    assert.equal(await page.getAttribute("#btn-copilot-ghost", "aria-checked"), "true");

    await page.evaluate(() => {
      const ed = document.getElementById("doc-editor");
      ed.innerHTML = "<p><br></p>";
    });
    await page.click("#doc-editor p");
    await page.keyboard.type("초안 문장 ");
    await page.waitForSelector(".eo-copilot-ghost-overlay");
    assert.equal(await page.$eval("#doc-editor", (ed) => Boolean(ed.querySelector(".eo-copilot-ghost-overlay"))), false);

    await page.keyboard.press("Escape");
    await page.waitForSelector(".eo-copilot-ghost-overlay", { state: "detached" });
    assert.deepEqual(await paragraphTexts(page), ["초안 문장 "]);

    await page.keyboard.type("계속 ");
    await page.waitForSelector(".eo-copilot-ghost-overlay");

    // Verify coordinate alignment at 150% zoom and immediate reposition back to 100%
    const zoomDelta = await page.evaluate(() => {
      globalThis.__EASYOFFICE_ZOOM__.apply(150);
      const ov = document.querySelector(".eo-copilot-ghost-overlay").getBoundingClientRect();
      const cr = window.getSelection().getRangeAt(0).getBoundingClientRect();
      const dTop = Math.abs(ov.top - cr.top);
      const dLeft = Math.abs(ov.left - cr.left);
      globalThis.__EASYOFFICE_ZOOM__.apply(100);
      return { dTop, dLeft };
    });
    assert.ok(zoomDelta.dTop <= 6 && zoomDelta.dLeft <= 12, `zoomed ghost overlay misaligned: ${JSON.stringify(zoomDelta)}`);

    // ArrowLeft dismisses stale prediction, ArrowRight + text re-triggers, Tab accepts cleanly without stray <br>
    await page.keyboard.press("ArrowLeft");
    await page.waitForSelector(".eo-copilot-ghost-overlay", { state: "detached" });
    await page.keyboard.press("ArrowRight");
    await page.keyboard.type("더 ");
    await page.waitForSelector(".eo-copilot-ghost-overlay");
    await page.keyboard.press("Tab");
    await page.waitForSelector(".eo-copilot-ghost-overlay", { state: "detached" });
    assert.deepEqual(await paragraphTexts(page), ["초안 문장 계속 더 이어지는 예측 구절입니다."]);
    assert.equal(await page.$eval("#doc-editor p", (p) => p.querySelectorAll("br").length), 0);

    await page.click("#btn-copilot-mode");
    await page.click('#copilot-mode-menu [data-mode-choice="ask"]');
    await page.fill("#copilot-input", "표로 요약해 주세요");
    await page.click("#btn-copilot-send");
    await page.waitForSelector("#copilot-thread .eo-copilot-md-table-wrap");
    assert.equal(await page.locator("#eo-copilot-shell p").count(), 0);
    assert.equal(await page.getAttribute("#copilot-thread .eo-copilot-md-table-wrap", "role"), "region");
  }],
];
