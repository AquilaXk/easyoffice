import assert from "node:assert/strict";

// The copilot's pointer on a slide and on a range of paragraphs (PD-304), in a real WebKit with real layout. The model's
// answers are a fixture: the page's `run_smart_assistant` calls for `copilot_run` and `copilot_list_models` are answered
// here and every other request goes to the real engine. What is measured is where the pointer's boxes really are.

const PANEL_MODEL = "#btn-copilot-model";
const ACCEPT = '#copilot-thread button[data-copilot-action="accept"]';
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

/** Opens the panel, picks a local provider (no key needed) and sends `prompt`; resolves when 수락 is on screen. */
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
  await page.waitForSelector(ACCEPT);
}

export async function acceptChange(page) {
  await page.click(ACCEPT);
  await page.waitForSelector(`${CURSOR}:not([hidden]) .eo-copilot-label`);
}

export const finishPlay = (page) => page.waitForSelector(CURSOR, { state: "detached", timeout: FULL_RUN_MS });
export const undoLast = (page) => page.click("#copilot-thread .eo-copilot-undo-btn >> nth=-1");

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
    assert.equal(await page.locator("#slide-canvas-layer .slide-el").count(), 0, "nothing is on the slide before 수락");
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

export const copilotDocSteps = [
  ["the pointer selects a range of paragraphs across their real lines before it types the replacement", async (page) => {
    await page.evaluate((texts) => {
      document.getElementById("doc-editor").innerHTML = texts.map((t) => `<p>${t}</p>`).join("");
    }, PARAGRAPHS);
    const op = { op: "doc.replace_paragraphs", startIndex: 1, endIndex: 2, blocks: [{ text: "새로 쓴 둘째 문단" }, { text: "새로 쓴 셋째 문단" }] };
    await askCopilot(page, "둘째와 셋째 문단을 바꿔 주세요", [[op]]);
    assert.deepEqual(await paragraphTexts(page), PARAGRAPHS, "the document is untouched before 수락");
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
];
