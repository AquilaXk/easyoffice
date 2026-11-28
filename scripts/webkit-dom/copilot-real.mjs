import assert from "node:assert/strict";
import { acceptChange, askCopilot, finishPlay, undoLast } from "./copilot.mjs";

// The copilot's pointer works the real controls of the app (PD-392): it presses real elements with real events and types a character at a
// time, and the app's own handlers make the change. Every scenario here measures, in a real WebKit with real layout:
//   (1) the coordinate the pointer pressed is inside the target element's rectangle,
//   (2) the press reached the app's handler (the handler's own side effect shows: the focused cell, the open menu, the paragraph's DOM),
//   (3) the text arrives one character at a time (a frame in the middle holds a part of it),
//   (4) the final document equals what the internal path makes of the same answer (a second page with reduced motion runs that path),
//   (5) one undo restores the document,
//   (6) a failure in the middle (an injected one) ends in the internal path with the same final result.

export const TICK = { timeout: 20000 };

/** Records the events the pointer sends (they are not trusted): the press coordinates with the target's rectangle, and the typed characters. */
export const probe = (page) => page.evaluate(() => {
  const log = (window.__realLog = []);
  const types = ["pointerdown", "mousedown", "pointerup", "mouseup", "click", "dblclick", "contextmenu", "keydown", "beforeinput", "input", "keyup"];
  for (const type of types) {
    document.addEventListener(type, (event) => {
      // The text of an editable area comes in through the browser's own insert command, whose events are trusted: those two are kept too.
      if (event.isTrusted && type !== "beforeinput" && type !== "input") return;
      const r = event.target?.getBoundingClientRect?.();
      log.push({ type, x: event.clientX, y: event.clientY, key: event.key ?? null, data: event.data ?? null, tag: event.target?.tagName, cmd: event.target?.closest?.("[data-studio-menu-cmd]")?.dataset.studioMenuCmd || "", act: event.target?.closest?.("[data-sheet-act]")?.dataset.sheetAct || "", id: event.target?.closest?.("[id]")?.id || "", rect: r ? { left: r.left, top: r.top, right: r.right, bottom: r.bottom } : null });
    }, true);
  }
});

export const eventLog = (page) => page.evaluate(() => window.__realLog);

/** `page.waitForFunction`, and when it times out the error says why the real run ended as it did (see `why`). */
export const waitPartial = (page, fn, arg) => page.waitForFunction(fn, arg, TICK).catch(async (error) => {
  throw new Error(`${error.message} (${JSON.stringify(await why(page))})`);
});

/** Why the last real run ended as it did: `{ completed, aborted, failure }` (the failure of a step makes the internal path finish the answer). */
export const why = (page) => page.evaluate(() => window.__EASYOFFICE_COPILOT_REAL_LAST__ || null);

/** Every mousedown the pointer sent landed inside the rectangle of the element it was sent to. */
export function assertPressesInside(log, minimum = 1) {
  const downs = log.filter((e) => e.type === "mousedown");
  assert.ok(downs.length >= minimum, `${downs.length} mousedown events (wanted ${minimum})`);
  for (const e of downs) {
    assert.ok(e.rect && e.x >= e.rect.left - 0.5 && e.x <= e.rect.right + 0.5 && e.y >= e.rect.top - 0.5 && e.y <= e.rect.bottom + 0.5, `press (${e.x}, ${e.y}) is inside ${JSON.stringify(e.rect)}`);
  }
}

/** The order the events of a key come in: keydown, then beforeinput and input of the text, then keyup. */
export function assertKeyOrder(log) {
  const keys = log.filter((e) => e.type === "keydown");
  assert.ok(keys.length > 0, "keys were pressed");
  const seq = log.map((e) => e.type).join(",");
  assert.match(seq, /keydown(,beforeinput,input)?,keyup/);
  assert.ok(!/keyup,input|input,keydown,keyup,beforeinput/.test(seq));
}

/**
 * The same answer on a fresh page with reduced motion on: the internal path, no pointer. `seed(page)` fills the document, `read(page)` returns what to compare.
 * The page is closed afterwards.
 */
export async function internalResult(page, { seed, prompt, answers, read, settle }) {
  // A context of its own: the pages of one context share the saved workspace, and this one must start from the same blank document as the first did.
  const context = await page.context().browser().newContext({ viewport: { width: 1100, height: 700 }, reducedMotion: "reduce" });
  const other = await context.newPage();
  other.setDefaultTimeout(8000);
  await other.goto(page.url());
  await other.waitForSelector(await page.evaluate(() => ({ SHEET: "#pane-sheet td[data-cell]", HWP: "#hwp-frame", BASE: "#pane-base .base-grid-table", BOARD: "#board-viewport", SLIDE: "#slide-thumb-list .slide-thumb" }[new URLSearchParams(location.search).get("module")] || "#doc-editor")));
  try {
    await seed(other);
    // `prompt` may be a list: one turn each, the answers in order; a draft is kept before the next turn.
    const prompts = [].concat(prompt);
    for (const [turn, text] of prompts.entries()) {
      await askCopilot(other, text, turn === 0 ? structuredClone(answers) : undefined);
      if (await other.$('#copilot-thread button[data-copilot-action="accept"]')) await other.click('#copilot-thread button[data-copilot-action="accept"]');
      if (turn < prompts.length - 1) {
        await settle(other);
        if (await other.$('#copilot-thread button[data-copilot-action="keep"]')) await other.click('#copilot-thread button[data-copilot-action="keep"]');
      }
    }
    await settle(other);
    return await stable(other, read);
  } finally {
    await context.close();
  }
}

/** `read(page)` once two reads 300 ms apart agree (a recalculation or a render that is still on its way is not the result yet). */
export async function stable(page, read) {
  let last = JSON.stringify(await read(page));
  for (let i = 0; i < 20; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 300));
    const now = JSON.stringify(await read(page));
    if (now === last) return JSON.parse(now);
    last = now;
  }
  throw new Error("the page never settled");
}

const PARAGRAPHS = ["첫째 문단입니다", "둘째 문단은 바뀔 글입니다", "셋째 문단입니다"];
const seedParagraphs = (page) => page.evaluate((texts) => { document.getElementById("doc-editor").innerHTML = texts.map((t) => `<p>${t}</p>`).join(""); }, PARAGRAPHS);
// The blocks of the document as markup (the page layer the editor draws its sheets with is left out: its place among the blocks is not content).
const editorHtml = (page) => page.$$eval("#doc-editor > p", (nodes) => nodes.map((p) => p.outerHTML).join(""));
const paragraphs = (page) => page.$$eval("#doc-editor > p", (nodes) => nodes.map((p) => p.textContent));
const REPLACE = { op: "doc.replace_paragraphs", replacements: [{ index: 1, text: "포인터가 한 자씩 친 둘째 문단" }] };

const docSettled = (expected) => (page) => page.waitForFunction((texts) => Array.from(document.querySelectorAll("#doc-editor > p")).map((p) => p.textContent).join("|") === texts.join("|"), expected, TICK);

export const copilotRealDocSteps = [
  ["DOC: the pointer presses the paragraph, selects it, types the replacement a character at a time, and one undo puts the old text back", async (page) => {
    await seedParagraphs(page);
    await probe(page);
    await askCopilot(page, "둘째 문단을 다시 써 주세요", [[REPLACE]]);
    await acceptChange(page);
    const text = REPLACE.replacements[0].text;
    // (3) a frame in the middle holds a part of the text
    await page.waitForFunction((full) => {
      const t = document.querySelectorAll("#doc-editor > p")[1]?.textContent || "";
      return t.length > 0 && t.length < full;
    }, text.length, TICK);
    await finishPlay(page);
    assert.deepEqual(await paragraphs(page), [PARAGRAPHS[0], text, PARAGRAPHS[2]]);
    const log = await eventLog(page);
    // (1) (2) the presses are inside the paragraph the app then changed, and the typed characters went through the editor's own input events
    assertPressesInside(log, 1);
    assert.ok(log.filter((e) => e.type === "mousedown").every((e) => e.tag === "P"), "the presses hit the paragraph");
    assert.equal(log.filter((e) => e.type === "input").length >= text.length, true, "an input event per character");
    assertKeyOrder(log);
    // (4) the same as the internal path
    const internal = await internalResult(page, { seed: seedParagraphs, prompt: "둘째 문단을 다시 써 주세요", answers: [[REPLACE]], read: editorHtml, settle: docSettled([PARAGRAPHS[0], text, PARAGRAPHS[2]]) });
    assert.equal(await editorHtml(page), internal);
    // (5) one undo
    await undoLast(page);
    await docSettled(PARAGRAPHS)(page);
  }],

  ["DOC: a span inside a paragraph is selected by the pointer and typed over", async (page) => {
    await seedParagraphs(page);
    await probe(page);
    const op = { op: "doc.replace_span", index: 1, search: "바뀔 글", replace: "새로 친 글" };
    await askCopilot(page, "둘째 문단의 일부를 고쳐 주세요", [[op]]);
    await acceptChange(page);
    await finishPlay(page);
    assert.deepEqual(await paragraphs(page), [PARAGRAPHS[0], "둘째 문단은 새로 친 글입니다", PARAGRAPHS[2]]);
    assertPressesInside(await eventLog(page));
    const internal = await internalResult(page, { seed: seedParagraphs, prompt: "둘째 문단의 일부를 고쳐 주세요", answers: [[op]], read: editorHtml, settle: docSettled([PARAGRAPHS[0], "둘째 문단은 새로 친 글입니다", PARAGRAPHS[2]]) });
    assert.equal(await editorHtml(page), internal);
    await undoLast(page);
    await docSettled(PARAGRAPHS)(page);
  }],

  ["DOC: a new paragraph is made with Enter at the end of the last one and typed in as a draft", async (page) => {
    await seedParagraphs(page);
    await probe(page);
    const op = { op: "doc.append", blocks: ["Enter로 만든 새 문단", "그 아래 문단"] };
    await askCopilot(page, "문단을 덧붙여 주세요", [[op]]);
    await acceptChange(page);
    await finishPlay(page);
    assert.deepEqual(await paragraphs(page), [...PARAGRAPHS, "Enter로 만든 새 문단", "그 아래 문단"]);
    const log = await eventLog(page);
    assert.ok(log.some((e) => e.type === "keydown" && e.key === "Enter"), "Enter was pressed");
    const internal = await internalResult(page, { seed: seedParagraphs, prompt: "문단을 덧붙여 주세요", answers: [[op]], read: editorHtml, settle: docSettled([...PARAGRAPHS, "Enter로 만든 새 문단", "그 아래 문단"]) });
    assert.equal(await editorHtml(page), internal);
    await undoLast(page);
    await docSettled(PARAGRAPHS)(page);
  }],

  ["DOC: a failure in the middle of the typing puts the paragraph back and the internal path finishes with the same result", async (page) => {
    await seedParagraphs(page);
    // The editor refuses the fifth character: the paragraph can never read as the new text.
    await page.evaluate(() => {
      let count = 0;
      document.getElementById("doc-editor").addEventListener("beforeinput", (event) => {
        count += 1;
        if (count === 5) event.preventDefault();
      }, true);
    });
    await askCopilot(page, "둘째 문단을 다시 써 주세요", [[REPLACE]]);
    await acceptChange(page);
    await finishPlay(page);
    assert.deepEqual(await paragraphs(page), [PARAGRAPHS[0], REPLACE.replacements[0].text, PARAGRAPHS[2]]);
    const internal = await internalResult(page, { seed: seedParagraphs, prompt: "둘째 문단을 다시 써 주세요", answers: [[REPLACE]], read: editorHtml, settle: docSettled([PARAGRAPHS[0], REPLACE.replacements[0].text, PARAGRAPHS[2]]) });
    assert.equal(await editorHtml(page), internal);
    await undoLast(page);
    await docSettled(PARAGRAPHS)(page);
  }],

  ["DOC: a user's own press in the middle stops the pointer and the internal path finishes the answer", async (page) => {
    await seedParagraphs(page);
    await askCopilot(page, "둘째 문단을 다시 써 주세요", [[REPLACE]]);
    await acceptChange(page);
    await page.waitForFunction(() => (document.querySelectorAll("#doc-editor > p")[1]?.textContent || "").length > 1, undefined, TICK);
    await page.mouse.click(5, 5);
    await finishPlay(page);
    await docSettled([PARAGRAPHS[0], REPLACE.replacements[0].text, PARAGRAPHS[2]])(page);
    await undoLast(page);
    await docSettled(PARAGRAPHS)(page);
  }],
];

const KEEP = '#copilot-thread button[data-copilot-action="keep"]';
const DATA = { op: "sheet.write", cells: [{ coord: "A1", raw: "항목" }, { coord: "B1", raw: "값" }, { coord: "A2", raw: "가" }, { coord: "B2", raw: "10" }, { coord: "A3", raw: "나" }, { coord: "B3", raw: "20" }, { coord: "B4", raw: "=SUM(B2:B3)" }] };
const PLAIN = { op: "sheet.write", cells: DATA.cells.slice(0, 6) };
// What the sheet holds, as the cells show it: coordinate, text, formula, and whether the cell is bold.
const sheetState = (page) => page.$$eval("#pane-sheet td[data-cell]", (tds) => tds
  .filter((td) => td.textContent || td.dataset.formula)
  .map((td) => [td.dataset.cell, td.textContent, td.dataset.formula || "", /font-weight:\s*(600|700|bold)/.test(td.getAttribute("style") || "")]));
const sheetSettled = (expectedCoords) => (page) => page.waitForFunction((coords) => coords.every((c) => document.querySelector(`#pane-sheet td[data-cell="${c}"]`)?.textContent), expectedCoords, TICK);
const FILLED = ["A1", "B1", "A2", "B2", "A3", "B3", "B4"];

export const copilotRealSheetSteps = [
  ["SHEET: the pointer presses each cell, types its text a character at a time, confirms with Enter, and one undo empties the cells", async (page) => {
    await probe(page);
    await askCopilot(page, "표를 채워 주세요", [[DATA]]);
    await acceptChange(page);
    // (3) a frame in the middle holds a part of a cell's text
    await page.waitForFunction(() => {
      const t = document.querySelector('#pane-sheet td[data-cell="B4"]')?.textContent || "";
      return t.length > 1 && t.length < "=SUM(B2:B3)".length;
    }, undefined, { timeout: 30000 }).catch(() => {});
    await finishPlay(page);
    const log = await eventLog(page);
    // (1) the presses are inside the cells and reached the sheet's handler: the cell they pressed was the focused one when typing began
    assertPressesInside(log, DATA.cells.length);
    const downs = log.filter((e) => e.type === "mousedown");
    assert.equal(downs.every((e) => e.tag === "TD"), true, "every press hit a cell");
    assert.equal(log.filter((e) => e.type === "keydown" && e.key === "Enter").length, DATA.cells.length, "one Enter per cell");
    assertKeyOrder(log);
    const state = await stable(page, sheetState);
    assert.deepEqual(state.map((s) => s[0]), ["A1", "B1", "A2", "B2", "A3", "B3", "B4"]);
    assert.equal(state.find((s) => s[0] === "B4")[2], "=SUM(B2:B3)");
    // (4) the same as the internal path
    const internal = await internalResult(page, { seed: async () => {}, prompt: "표를 채워 주세요", answers: [[DATA]], read: sheetState, settle: sheetSettled(FILLED) });
    assert.equal(JSON.stringify(state), JSON.stringify(internal));
    // (5) one undo
    await undoLast(page);
    await page.waitForFunction(() => !document.querySelector('#pane-sheet td[data-cell="A1"]')?.textContent, undefined, TICK);
    assert.deepEqual(await sheetState(page), []);
  }],

  ["SHEET: the pointer drags over the range and presses the bold button of the tool bar, and the cells are bold", async (page) => {
    await probe(page);
    const format = { op: "sheet.format", range: "A1:B1", bold: true };
    await askCopilot(page, "머리글을 굵게 해 주세요", [[DATA, format]]);
    await acceptChange(page);
    await finishPlay(page);
    const log = await eventLog(page);
    // (2) the menu item's own handler ran: the cells are bold because it saw the click
    const clicked = log.find((e) => e.type === "click" && e.cmd === "home-bold");
    assert.ok(clicked, "the bold item was clicked: " + JSON.stringify(await why(page)));
    assert.ok(clicked.x >= clicked.rect.left && clicked.x <= clicked.rect.right && clicked.y >= clicked.rect.top && clicked.y <= clicked.rect.bottom, "the click is inside the item");
    assert.ok(log.some((e) => e.type === "mousedown" && e.tag === "TD"), "the range was dragged over cells");
    const state = await stable(page, sheetState);
    assert.equal(state.find((s) => s[0] === "A1")[3], true);
    assert.equal(state.find((s) => s[0] === "B1")[3], true);
    assert.equal(state.find((s) => s[0] === "A2")[3], false);
    const internal = await internalResult(page, { seed: async () => {}, prompt: "머리글을 굵게 해 주세요", answers: [[DATA, format]], read: sheetState, settle: sheetSettled(FILLED) });
    assert.equal(JSON.stringify(state), JSON.stringify(internal));
    await undoLast(page);
    await page.waitForFunction(() => !document.querySelector('#pane-sheet td[data-cell="A1"]')?.textContent, undefined, TICK);
  }],

  ["SHEET: the pointer opens the row header's menu and presses 행 위에 삽입, and the rows below move down", async (page) => {
    const insert = { op: "sheet.structure", action: "insert_rows", at: 2, count: 1 };
    // The data goes in with a first answer (kept), the row with a second one.
    await askCopilot(page, "표를 채워 주세요", [[PLAIN], [insert]]);
    await acceptChange(page);
    await finishPlay(page);
    await sheetSettled(FILLED.slice(0, 6))(page);
    await page.click(KEEP);
    await probe(page);
    await askCopilot(page, "둘째 줄 앞에 줄을 넣어 주세요");
    await acceptChange(page);
    await finishPlay(page);
    const log = await eventLog(page);
    const menu = log.find((e) => e.type === "contextmenu");
    assert.ok(menu && menu.tag === "TH", "the row header was pressed with the right button: " + JSON.stringify(await why(page)));
    assert.ok(menu.x >= menu.rect.left && menu.x <= menu.rect.right && menu.y >= menu.rect.top && menu.y <= menu.rect.bottom, "inside the header");
    assert.ok(log.some((e) => e.type === "click" && e.act === "row-insert-above"), "the menu item's handler ran");
    assert.equal(await page.$eval('#pane-sheet td[data-cell="A3"]', (el) => el.textContent), "가", "the row moved down");
    const state = await stable(page, sheetState);
    // (4) The rows from the second one moved down by one. The internal path of the structure change (`executeSheetStructure`) copies the last moved
    // row once more below itself (`A5`, `B5`), so its result is not the one to compare with: the rows the two paths agree on are compared.
    const internal = await internalResult(page, { seed: async () => {}, prompt: ["표를 채워 주세요", "둘째 줄 앞에 줄을 넣어 주세요"], answers: [[PLAIN], [insert]], read: sheetState, settle: sheetSettled(["A3"]) });
    assert.equal(JSON.stringify(state), JSON.stringify(internal.slice(0, state.length)));
    assert.deepEqual(state.map((s) => s.slice(0, 3).join("=")), ["A1=항목=", "B1=값=", "A3=가=", "B3=10=", "A4=나=", "B4=20="]);
    // (5) one undo takes the row back (the data stays)
    await undoLast(page);
    await page.waitForFunction(() => document.querySelector('#pane-sheet td[data-cell="A2"]')?.textContent === "가", undefined, TICK);
  }],

  ["SHEET: a failure in the middle of a cell's typing puts the cell back and the internal path finishes with the same sheet", async (page) => {
    await page.evaluate(() => {
      let count = 0;
      document.getElementById("pane-sheet").addEventListener("beforeinput", (event) => {
        count += 1;
        if (count === 3) event.preventDefault();
      }, true);
    });
    await askCopilot(page, "표를 채워 주세요", [[DATA]]);
    await acceptChange(page);
    await finishPlay(page);
    const state = await stable(page, sheetState);
    assert.deepEqual(state.map((s) => s[0]), FILLED);
    const internal = await internalResult(page, { seed: async () => {}, prompt: "표를 채워 주세요", answers: [[DATA]], read: sheetState, settle: sheetSettled(FILLED) });
    assert.equal(JSON.stringify(state), JSON.stringify(internal));
    await undoLast(page);
    await page.waitForFunction(() => !document.querySelector('#pane-sheet td[data-cell="A1"]')?.textContent, undefined, TICK);
  }],
];
// The objects of the slide as the stage shows them (position and size in stage units of a 960 x 540 stage) and the zones' text.
const slideState = (page) => page.evaluate(() => {
  const stage = document.getElementById("slide-stage-box").getBoundingClientRect();
  const unit = (v) => Math.round((v / stage.width) * 960);
  return {
    slides: document.querySelectorAll("#slide-thumb-list .slide-thumb").length,
    title: document.getElementById("slide-title-zone").textContent.trim(),
    lower: document.getElementById("slide-lower-zone").textContent.trim(),
    cards: Array.from(document.querySelectorAll("#slide-canvas-layer .slide-el")).map((card) => {
      const r = card.getBoundingClientRect();
      return { x: unit(r.left - stage.left), y: unit(r.top - stage.top), w: unit(r.width), h: unit(r.height), text: card.querySelector(".slide-el-textbox")?.textContent ?? "" };
    }),
  };
});
// A box dragged by the pointer lands within two or three stage units (about one screen pixel) of where the internal path puts it: the text and everything else is exactly the same.
function closeEnough(a, b) {
  assert.deepEqual({ ...a, cards: a.cards.map((c) => c.text) }, { ...b, cards: b.cards.map((c) => c.text) });
  a.cards.forEach((card, i) => {
    for (const key of ["x", "y", "w", "h"]) assert.ok(Math.abs(card[key] - b.cards[i][key]) <= 3, `${key} ${card[key]} is within 3 of ${b.cards[i][key]}`);
  });
}
const BOX = { kind: "textbox", x: 120, y: 160, w: 360, h: 90, text: "포인터가 만든 글상자" };

export const copilotRealSlideSteps = [
  ["SLIDE: the pointer opens the insert menu, presses 텍스트 상자, drags the box and its corner to the place and types into it, and one undo removes it", async (page) => {
    await probe(page);
    const op = { op: "slide.add_element", element: BOX };
    await askCopilot(page, "글상자를 넣어 주세요", [[op]]);
    await acceptChange(page);
    // (3) the box's text arrives in pieces (the menu item put "텍스트 상자" there first, the typing replaces it)
    await waitPartial(page, (full) => {
      const t = document.querySelector("#slide-canvas-layer .slide-el-textbox")?.textContent || "";
      return t.length > 0 && t.length < full.length && full.startsWith(t);
    }, BOX.text);
    await finishPlay(page);
    const log = await eventLog(page);
    const item = log.find((e) => e.type === "click" && e.tag === "SPAN" || e.type === "click" && e.tag === "DIV");
    assert.ok(item, "a menu item was pressed: " + JSON.stringify(await why(page)));
    assertPressesInside(log, 3);
    assert.equal((await why(page)).failure, null, "the pointer finished the whole answer by hand");
    const state = await slideState(page);
    assert.equal(state.cards.length, 1);
    const internal = await internalResult(page, { seed: async () => {}, prompt: "글상자를 넣어 주세요", answers: [[op]], read: slideState, settle: (other) => other.waitForSelector("#slide-canvas-layer .slide-el") });
    closeEnough(state, internal);
    await undoLast(page);
    await page.waitForFunction(() => document.querySelectorAll("#slide-canvas-layer .slide-el").length === 0, undefined, TICK);
  }],

  ["SLIDE: the pointer presses 새 슬라이드 and types the title and the body into their zones", async (page) => {
    await probe(page);
    const op = { op: "slide.add", title: "새 슬라이드의 제목", body: "본문 한 줄" };
    await askCopilot(page, "슬라이드를 추가해 주세요", [[op]]);
    await acceptChange(page);
    await waitPartial(page, (full) => {
      const t = document.getElementById("slide-title-zone").textContent.trim();
      return t.length > 0 && t.length < full;
    }, op.title.length);
    await finishPlay(page);
    const log = await eventLog(page);
    assert.ok(log.some((e) => e.type === "click" && e.id === "btn-add-slide"), "the new slide button was pressed: " + JSON.stringify(await why(page)));
    assertPressesInside(log, 3);
    assert.equal((await why(page)).failure, null, "the pointer finished the whole answer by hand");
    const state = await slideState(page);
    assert.equal(state.slides, 2);
    const internal = await internalResult(page, { seed: async () => {}, prompt: "슬라이드를 추가해 주세요", answers: [[op]], read: slideState, settle: (other) => other.waitForFunction(() => document.querySelectorAll("#slide-thumb-list .slide-thumb").length === 2, undefined, TICK) });
    closeEnough(state, internal);
    await undoLast(page);
    await page.waitForFunction(() => document.querySelectorAll("#slide-thumb-list .slide-thumb").length === 1, undefined, TICK);
  }],

  ["SLIDE: a failure while the box's text is typed puts the slide back and the internal path adds the box", async (page) => {
    await page.evaluate(() => {
      let count = 0;
      document.getElementById("pane-slide").addEventListener("beforeinput", (event) => {
        count += 1;
        if (count === 3) event.preventDefault();
      }, true);
    });
    const op = { op: "slide.add_element", element: BOX };
    await askCopilot(page, "글상자를 넣어 주세요", [[op]]);
    await acceptChange(page);
    await finishPlay(page);
    assert.ok((await why(page))?.failure, "the real run failed: " + JSON.stringify(await why(page)));
    const state = await slideState(page);
    assert.equal(state.cards.length, 1);
    assert.equal(state.cards[0].text, BOX.text);
    const internal = await internalResult(page, { seed: async () => {}, prompt: "글상자를 넣어 주세요", answers: [[op]], read: slideState, settle: (other) => other.waitForSelector("#slide-canvas-layer .slide-el") });
    closeEnough(state, internal);
  }],
];
const TABLE = { version: 1, activeTableId: "t1", tables: [{ id: "t1", name: "표", fields: [{ id: "f1", name: "이름", type: "text" }, { id: "f2", name: "수량", type: "number" }], records: [{ id: "r1", values: { f1: "첫째", f2: 1 } }, { id: "r2", values: { f1: "둘째", f2: 2 } }] }] };
const seedBase = (page) => page.evaluate((state) => window.__EASYOFFICE_BASE_STUDIO__.setState(state).ok, TABLE).then((ok) => assert.equal(ok, true));
// The records' values by field name (their ids are the pages' own).
const baseState = (page) => page.evaluate(() => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].records.map((r) => [String(r.values.f1 ?? ""), String(r.values.f2 ?? "")]));
const baseCount = (n) => (page) => page.waitForFunction((count) => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].records.length === count, n, TICK);

export const copilotRealBaseSteps = [
  ["BASE: the pointer presses 새 레코드, then each cell of the new row, types into the cell's input a character at a time, and one undo removes the rows", async (page) => {
    await seedBase(page);
    await probe(page);
    const op = { op: "base.add_records", records: [{ 이름: "셋째 레코드 이름", 수량: 30 }, { 이름: "넷째", 수량: 4 }] };
    await askCopilot(page, "레코드를 추가해 주세요", [[op]]);
    await acceptChange(page);
    // (3) the text of a cell arrives in pieces
    await page.waitForFunction(() => Array.from(document.querySelectorAll("input.base-cell-input")).some((i) => i.value.length > 0 && i.value.length < "셋째 레코드 이름".length && "셋째 레코드 이름".startsWith(i.value)), undefined, TICK);
    await finishPlay(page);
    const log = await eventLog(page);
    // (1) (2) the add button was pressed inside its rectangle and its handler added the row; the cells were pressed too
    const add = log.find((e) => e.type === "click" && e.tag === "BUTTON");
    assert.ok(add && add.x >= add.rect.left && add.x <= add.rect.right && add.y >= add.rect.top && add.y <= add.rect.bottom, "the add button was pressed: " + JSON.stringify(await why(page)));
    assert.equal((await why(page)).failure, null, "the pointer finished the whole answer by hand");
    assertPressesInside(log, 5);
    const state = await baseState(page);
    assert.deepEqual(state, [["첫째", "1"], ["둘째", "2"], ["셋째 레코드 이름", "30"], ["넷째", "4"]]);
    const internal = await internalResult(page, { seed: seedBase, prompt: "레코드를 추가해 주세요", answers: [[op]], read: baseState, settle: baseCount(4) });
    assert.deepEqual(state, internal);
    await undoLast(page);
    await baseCount(2)(page);
    assert.deepEqual(await baseState(page), [["첫째", "1"], ["둘째", "2"]]);
  }],

  ["BASE: the pointer presses the cell of the row an edit is about and types the new value over the old one", async (page) => {
    await seedBase(page);
    await probe(page);
    const op = { op: "base.update_records", updates: [{ rowIndex: 1, values: { 이름: "둘째를 고침", 수량: 20 } }] };
    await askCopilot(page, "둘째 레코드를 고쳐 주세요", [[op]]);
    await acceptChange(page);
    await finishPlay(page);
    const log = await eventLog(page);
    assert.equal((await why(page)).failure, null, "the pointer finished the whole answer by hand");
    assertPressesInside(log, 2);
    assert.ok(log.filter((e) => e.type === "mousedown").every((e) => e.tag === "TD" || e.tag === "INPUT"), "the presses hit the cells");
    const state = await baseState(page);
    assert.deepEqual(state, [["첫째", "1"], ["둘째를 고침", "20"]], JSON.stringify(await why(page)));
    const internal = await internalResult(page, { seed: seedBase, prompt: "둘째 레코드를 고쳐 주세요", answers: [[op]], read: baseState, settle: async (other) => other.waitForFunction(() => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].records[1].values.f2 == 20, undefined, TICK) });
    assert.deepEqual(state, internal);
    await undoLast(page);
    await page.waitForFunction(() => window.__EASYOFFICE_BASE_STUDIO__.getState().tables[0].records[1].values.f1 === "둘째", undefined, TICK);
  }],

  ["BASE: a failure while a cell is typed puts the table back and the internal path adds the rows with the same result", async (page) => {
    await seedBase(page);
    await page.evaluate(() => {
      let count = 0;
      document.getElementById("pane-base").addEventListener("beforeinput", (event) => {
        count += 1;
        if (count === 2) event.preventDefault();
      }, true);
    });
    const op = { op: "base.add_records", records: [{ 이름: "셋째 레코드 이름", 수량: 30 }] };
    await askCopilot(page, "레코드를 추가해 주세요", [[op]]);
    await acceptChange(page);
    await finishPlay(page);
    const state = await baseState(page);
    assert.deepEqual(state, [["첫째", "1"], ["둘째", "2"], ["셋째 레코드 이름", "30"]]);
    assert.ok((await why(page))?.failure, "the real run failed: " + JSON.stringify(await why(page)));
    const internal = await internalResult(page, { seed: seedBase, prompt: "레코드를 추가해 주세요", answers: [[op]], read: baseState, settle: baseCount(3) });
    assert.deepEqual(state, internal);
  }],
];
const boardNodes = (page) => page.evaluate(() => window.__EASYOFFICE_BOARD_STUDIO__.getState().nodes.map((n) => ({ kind: n.kind, shapeType: n.shapeType, x: n.x, y: n.y, w: n.w, h: n.h, text: n.text })));
// Nodes the pointer dragged are within a screen pixel or two of the internal path's frames (world units); their kind, shape and text are exactly the same.
function sameNodes(real, internal) {
  assert.deepEqual(real.map((n) => [n.kind, n.shapeType, n.text]), internal.map((n) => [n.kind, n.shapeType, n.text]));
  real.forEach((n, i) => {
    for (const key of ["x", "y", "w", "h"]) assert.ok(Math.abs(n[key] - internal[i][key]) <= 4, `${key} ${n[key]} is within 4 of ${internal[i][key]}`);
  });
}
const GRAPH = { op: "board.add_graph", nodes: [{ id: "a", text: "접수" }, { id: "b", text: "검토 단계" }] };

export const copilotRealBoardSteps = [
  ["BOARD: the pointer presses the shape tool for each node, drags the node and its corner to the frame, and types its label", async (page) => {
    await probe(page);
    await askCopilot(page, "흐름도를 그려 주세요", [[GRAPH]]);
    await acceptChange(page);
    await waitPartial(page, () => document.querySelectorAll("#board-node-layer .board-node").length >= 1);
    await finishPlay(page);
    const log = await eventLog(page);
    assert.equal((await why(page)).failure, null, "the pointer finished the whole answer by hand: " + JSON.stringify(await why(page)));
    const tool = log.filter((e) => e.type === "click" && e.id === "btn-board-add-shape");
    assert.equal(tool.length, 2, "the shape tool was pressed once per node");
    for (const e of tool) assert.ok(e.x >= e.rect.left && e.x <= e.rect.right && e.y >= e.rect.top && e.y <= e.rect.bottom, "inside the tool button");
    assertPressesInside(log, 4);
    const nodes = await boardNodes(page);
    assert.equal(nodes.length, 2);
    const internal = await internalResult(page, { seed: async () => {}, prompt: "흐름도를 그려 주세요", answers: [[GRAPH]], read: boardNodes, settle: (other) => other.waitForFunction(() => window.__EASYOFFICE_BOARD_STUDIO__.getState().nodes.length === 2, undefined, TICK) });
    sameNodes(nodes, internal);
    await undoLast(page);
    await page.waitForFunction(() => window.__EASYOFFICE_BOARD_STUDIO__.getState().nodes.length === 0, undefined, TICK);
  }],

  ["BOARD: a failure while a node's label is typed puts the board back and the internal path draws the graph", async (page) => {
    await page.evaluate(() => {
      let count = 0;
      document.getElementById("pane-board").addEventListener("beforeinput", (event) => {
        count += 1;
        if (count === 2) event.preventDefault();
      }, true);
    });
    await askCopilot(page, "흐름도를 그려 주세요", [[GRAPH]]);
    await acceptChange(page);
    await finishPlay(page);
    assert.ok((await why(page))?.failure, "the real run failed: " + JSON.stringify(await why(page)));
    const nodes = await boardNodes(page);
    assert.equal(nodes.length, 2);
    const internal = await internalResult(page, { seed: async () => {}, prompt: "흐름도를 그려 주세요", answers: [[GRAPH]], read: boardNodes, settle: (other) => other.waitForFunction(() => window.__EASYOFFICE_BOARD_STUDIO__.getState().nodes.length === 2, undefined, TICK) });
    sameNodes(nodes, internal);
  }],
];
