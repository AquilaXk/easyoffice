import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// `read_slides(start, count)`: the reader of a deck, and its place in the agent loop (the same cache receipt and compaction as the
// other read tools). The engine side is `crates/easyoffice-app/tests/copilot_read_slides_pd390.rs`.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));

function readerFor(deck, { active = 0 } = {}) {
  const env = {};
  loadUiModule(UI("copilot-text.js"), env);
  loadUiModule(UI("copilot-slide-read.js"), env);
  let syncs = 0;
  const reader = env.__EASYOFFICE_COPILOT_SLIDE_READ__.createSlideReader({
    slideDeckState: deck,
    getWorkspacePanels: () => ({ syncActiveSlideDomStylesToDeck: () => { syncs += 1; } }),
    getActiveSlideIndex: () => active,
  });
  return { reader, syncs: () => syncs, limits: env.__EASYOFFICE_COPILOT_SLIDE_READ__ };
}

const deckOf = (n) => Array.from({ length: n }, (_, i) => ({ title: `제목 ${i}`, lower: `첫째 ${i}\n둘째 ${i}` }));

test("the reader returns the slides from start for count with title, items, notes, layout and objects, and counts the whole deck", () => {
  const deck = deckOf(8);
  deck[3].notes = "발표자 노트";
  deck[3].layout = "twoColumn";
  deck[3].elements = [{ id: "el-1", kind: "textbox", x: 10, y: 20, w: 300, h: 80, text: "상자 글" }];
  const { reader, syncs } = readerFor(deck);
  const read = reader.readSlidesTool({ start: 2, count: 3 });
  assert.equal(read.totalSlides, 8);
  assert.deepEqual(read.lines, [
    '  [2] 제목=제목 2 | 항목=["첫째 2", "둘째 2"]',
    '  [3] 제목=제목 3 | 항목=["첫째 3", "둘째 3"] | 노트=발표자 노트 | 레이아웃=twoColumn | 개체=[el-1:textbox(10,20,300x80):상자 글]',
    '  [4] 제목=제목 4 | 항목=["첫째 4", "둘째 4"]',
  ]);
  assert.equal(syncs(), 1, "the deck is synced with the screen before it is read");
});

test("no arguments read the first five and a read past the end of the deck returns what is there", () => {
  const { reader } = readerFor(deckOf(7));
  assert.equal(reader.readSlidesTool({}).lines.length, 5);
  assert.deepEqual(reader.readSlidesTool({ start: 5, count: 20 }).lines.map((l) => l.slice(0, 5)), ["  [5]", "  [6]"]);
  assert.deepEqual(reader.readSlidesTool({ start: 200, count: 3 }), { totalSlides: 7, lines: [] });
});

test("a huge count is cut to the limit of one read, and a negative, fractional or text argument and a start past the deck limit are refused", () => {
  const { reader, limits } = readerFor(deckOf(100));
  assert.equal(limits.MAX_READ_SLIDES, 20);
  assert.equal(reader.readSlidesTool({ start: 0, count: 1e9 }).lines.length, 20);
  assert.equal(reader.readSlidesTool({ start: 90, count: Number.MAX_SAFE_INTEGER }).lines.length, 10);
  for (const args of [{ start: -1 }, { count: -3 }, { start: 1.5 }, { count: 2.5 }, { start: "2" }, { count: "3" }, { count: 0 }, { start: Number.NaN }, { start: Infinity }, { start: 256 }, { start: 1e12 }]) {
    assert.throws(() => reader.readSlidesTool(args), /read_slides/, JSON.stringify(args));
  }
});

test("the context of a request is the first 64 slides with the same fields the reader gives", () => {
  const { reader } = readerFor(deckOf(80), { active: 5 });
  const context = reader.extractSlideContext();
  assert.equal(context.activeIndex, 5);
  assert.equal(context.aspect, "16:9");
  assert.equal(context.slides.length, 64);
  assert.deepEqual(context.slides[2], { index: 2, title: "제목 2", bullets: ["첫째 2", "둘째 2"] });
});

test("the agent loop runs read_slides, names the step, and replaces a repeated read with a cache receipt", async () => {
  const env = {};
  for (const file of ["copilot-text.js", "copilot-context-compact.js", "copilot-step-labels.js", "copilot-agent-loop.js"]) loadUiModule(UI(file), env);
  const { reader } = readerFor(deckOf(30));
  const call = { name: "read_slides", start: 12, count: 2 };
  let turn = 0;
  const sent = [];
  const steps = [];
  const loop = env.__EASYOFFICE_CREATE_COPILOT_AGENT_LOOP__({
    invokeIpc: async (_cmd, payload) => {
      turn += 1;
      sent.push(JSON.parse(payload));
      return turn <= 2
        ? { status: "ok", reply: "", toolCalls: [call], operations: [] }
        : { status: "ok", reply: "완료", operations: [] };
    },
    canStreamIpc: () => false,
    copilotActions: { executeReadTool: async (_mod, name, args) => (name === "read_slides" ? reader.readSlidesTool(args) : assert.fail(name)) },
  });
  await loop.runAgentTurn({
    baseReq: { action: "copilot_run", requestId: "r-slides", mode: "ask", module: "SLIDE", prompt: "13번 슬라이드를 요약해 주세요", history: [], context: {}, provider: { kind: "ollama", model: "qwen3" } },
    newSubRequestId: (i) => `r-slides-s${i}`,
    isCancelled: () => false,
    onToolStep: (text) => steps.push(text),
  });
  assert.deepEqual(steps, ["슬라이드 13~14 읽음", "슬라이드 13~14 읽음"]);
  const firstRead = sent[1].loopSteps.at(-1).text;
  assert.match(firstRead, /\[슬라이드 전체 30장\]/);
  assert.match(firstRead, /\[12\] 제목=제목 12/);
  assert.match(firstRead, /\[13\] 제목=제목 13/);
  const repeated = sent[2].loopSteps.at(-1).text;
  assert.match(repeated, /도구 캐시 영수증/, "the same read twice is a receipt");
  assert.doesNotMatch(repeated, /제목 12/);
});

test("a refused read_slides call comes back to the model as an error line and stops nothing", async () => {
  const env = {};
  for (const file of ["copilot-text.js", "copilot-context-compact.js", "copilot-step-labels.js", "copilot-agent-loop.js"]) loadUiModule(UI(file), env);
  const { reader } = readerFor(deckOf(3));
  let turn = 0;
  const sent = [];
  const loop = env.__EASYOFFICE_CREATE_COPILOT_AGENT_LOOP__({
    invokeIpc: async (_cmd, payload) => {
      turn += 1;
      sent.push(JSON.parse(payload));
      return turn === 1 ? { status: "ok", reply: "", toolCalls: [{ name: "read_slides", start: -5, count: 2 }], operations: [] } : { status: "ok", reply: "완료", operations: [] };
    },
    canStreamIpc: () => false,
    copilotActions: { executeReadTool: async (_mod, _name, args) => reader.readSlidesTool(args) },
  });
  const res = await loop.runAgentTurn({
    baseReq: { action: "copilot_run", requestId: "r-slides-bad", mode: "ask", module: "SLIDE", prompt: "읽어 주세요", history: [], context: {}, provider: { kind: "ollama", model: "qwen3" } },
    newSubRequestId: (i) => `r-slides-bad-s${i}`,
    isCancelled: () => false,
  });
  assert.equal(res.reply, "완료");
  assert.match(sent[1].loopSteps.at(-1).text, /오류\] read_slides: start는 0 이상의 정수여야 합니다/);
});
