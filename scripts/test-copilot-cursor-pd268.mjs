import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createFakeClock, settle } from "./test-support/fake-clock.mjs";
import { createWorkspaceHarness } from "./webui-dom-harness.mjs";

// PD-268: the copilot's pointer (the collaborator pointer shape, a "코파일럿" label, a glide between places, the edge rule),
// its caret and its selection highlight.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));
const env = {};
loadUiModule(UI("copilot-cursor.js"), env);
const api = env.__EASYOFFICE_COPILOT_CURSOR__;

async function makeCursor() {
  const { document } = await createWorkspaceHarness();
  const clock = createFakeClock();
  const win = { innerWidth: 1000, innerHeight: 700 };
  const cursor = api.createCopilotCursor({ document, win, now: clock.now, scheduler: clock });
  return { document, clock, cursor, win };
}

const transformOf = (document) => document.querySelector(".eo-copilot-pointer").style.transform;

test("the pointer is the collaborator arrow with the one label", async () => {
  const { document, cursor } = await makeCursor();
  cursor.show();
  const layer = document.getElementById("eo-copilot-cursor-layer");
  assert.ok(layer && layer.hidden === false);
  assert.equal(layer.getAttribute("aria-hidden"), "true");
  const polygons = layer.querySelectorAll("polygon");
  assert.equal(polygons.length, 2, "a white outline and the body");
  for (const polygon of polygons) assert.equal(polygon.getAttribute("points"), "0,0 0,14 4,9 11,8");
  assert.equal(layer.querySelector(".eo-copilot-label").textContent, "코파일럿");
  assert.equal(layer.textContent.trim(), "코파일럿", "no other wording");
});

test("a glide ends exactly at the target, eases out and can be taken over", async () => {
  const { document, clock, cursor } = await makeCursor();
  cursor.show();
  cursor.place(100, 100);
  const arrived = cursor.moveTo(500, 300, { duration: 400 });
  clock.advance(48);
  const early = cursor.position;
  assert.ok(early.x > 150 && early.x < 400, `eased start moves fast: ${early.x}`);
  clock.advance(200);
  const middle = cursor.position.x;
  assert.ok(middle > 400, "an ease-out is past the middle at half the time: " + middle);
  clock.advance(400);
  assert.equal(await arrived, true);
  assert.deepEqual(cursor.position, { x: 500, y: 300 });
  assert.equal(transformOf(document), "translate(500px, 300px)");

  const first = cursor.moveTo(50, 50, { duration: 400 });
  clock.advance(32);
  const second = cursor.moveTo(10, 10, { duration: 100 });
  assert.equal(await first, false, "a take-over ends the first glide");
  clock.drain();
  assert.equal(await second, true);
  assert.deepEqual(cursor.position, { x: 10, y: 10 });
  assert.equal(clock.pending, 0, "no frame is left running");
});

test("a pointer outside the view sticks to the edge, dims and drops its label", async () => {
  const { document, cursor } = await makeCursor();
  cursor.show();
  cursor.place(5000, -40);
  const pointer = document.querySelector(".eo-copilot-pointer");
  assert.equal(pointer.style.transform, "translate(989px, 0px)", "clamped to width - 11 and 0");
  assert.ok(pointer.classList.contains("is-off"));
  assert.equal(document.querySelector(".eo-copilot-label").hidden, true);
  cursor.place(200, 200);
  assert.ok(!pointer.classList.contains("is-off"));
  assert.equal(document.querySelector(".eo-copilot-label").hidden, false);
  assert.deepEqual(api.clampPointer(-1, 700, 1000, 700), { x: 0, y: 686, isOutOfBounds: true });
  assert.equal(api.clampPointer(0, 0, 1000, 700).isOutOfBounds, false);
});

test("the selection highlight draws one box per rectangle and the caret one bar", async () => {
  const { document, cursor } = await makeCursor();
  cursor.show();
  cursor.setSelection([{ left: 10, top: 20, width: 100, height: 18 }, { left: 10, top: 38, width: 0, height: 18 }, { left: 10, top: 56, width: 40, height: 18 }]);
  const boxes = document.querySelectorAll(".eo-copilot-selection");
  assert.equal(boxes.length, 2, "an empty rectangle is left out");
  assert.equal(boxes[0].style.width, "100px");
  cursor.setCaret({ left: 33, top: 21, height: 17 });
  const caret = document.querySelector(".eo-copilot-text-caret");
  assert.equal(caret.hidden, false);
  assert.equal(caret.style.left, "33px");
  cursor.setCaret(null);
  assert.equal(caret.hidden, true);
  cursor.press(true);
  assert.equal(document.querySelector(".eo-copilot-ring").hasAttribute("hidden"), false);
  cursor.press(false);
  assert.equal(document.querySelector(".eo-copilot-ring").hasAttribute("hidden"), true);
  cursor.hide();
  assert.equal(document.querySelectorAll(".eo-copilot-selection").length, 0);
  assert.equal(document.getElementById("eo-copilot-cursor-layer").hidden, true);
});

test("the animation controller runs one animation per key with the time between frames", () => {
  const clock = createFakeClock();
  const controller = api.createAnimationController({ scheduler: clock, now: clock.now });
  const deltas = [];
  controller.start("a", ({ deltaTime, state }) => {
    deltas.push(deltaTime);
    return deltas.length < 4 ? { n: (state?.n ?? 0) + 1 } : null;
  });
  controller.start("a", () => assert.fail("the same key is not started twice"));
  assert.equal(controller.running("a"), true);
  clock.drain();
  assert.deepEqual(deltas.slice(0, 2), [0, 0], "the start call and the first frame carry no delta");
  assert.ok(deltas.slice(2).every((d) => d === 16));
  assert.equal(controller.running("a"), false);
  assert.equal(clock.pending, 0);
});

test("destroy takes the layer out of the document", async () => {
  const { document, cursor } = await makeCursor();
  cursor.show();
  cursor.destroy();
  assert.equal(document.getElementById("eo-copilot-cursor-layer"), null);
  await settle();
});
