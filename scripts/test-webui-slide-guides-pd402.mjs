import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";

// PD-402: 보기 > 눈금자, 눈금선, 안내선 of the slide editor.

async function openSlide() {
  const h = await createWorkspaceHarness({ search: "?module=SLIDE&win=w-slide-guides-pd402" });
  await flushAsyncTurns(10);
  switchModule(h.document, "SLIDE");
  await flushAsyncTurns(6);
  return h;
}

const item = (h, act) => {
  const el = h.document.querySelector(`#menu-bar [data-slide-act="${act}"]`);
  assert.ok(el, `missing menu item ${act}`);
  return el;
};

test("the view menu has 눈금자, 눈금선 and 안내선 as check items", async () => {
  const h = await openSlide();
  assert.deepEqual(["view-ruler", "view-grid", "view-guides"].map((a) => item(h, a).querySelector(".md-label").textContent), ["눈금자", "눈금선", "안내선"]);
  assert.ok(["view-ruler", "view-grid", "view-guides"].every((a) => item(h, a).getAttribute("role") === "menuitemcheckbox"));
});

test("안내선 puts two lines through the center the first time, and they are the deck's guides", async () => {
  const h = await openSlide();
  assert.equal(h.host.__EASYOFFICE_SLIDE_RUN_ACT__("view-guides"), true);
  const first = h.host.__EASYOFFICE_SLIDE_CANVAS__.currentSlide();
  assert.equal(first.showGuides, true);
  assert.deepEqual(first.guides, [{ orient: "vert", posPt: 480 }, { orient: "horz", posPt: 270 }]);
  assert.equal(item(h, "view-guides").getAttribute("aria-checked"), "true");
  assert.equal(h.document.getElementById("slide-guide-layer").querySelectorAll(".slide-guide").length, 2);
  assert.deepEqual(h.host.__EASYOFFICE_SLIDE_GUIDES__.targets(), { v: [480], h: [270] });
  h.host.__EASYOFFICE_SLIDE_RUN_ACT__("view-guides");
  assert.deepEqual(h.host.__EASYOFFICE_SLIDE_GUIDES__.targets(), { v: [], h: [] }, "hidden guides do not snap");
  assert.equal(first.guides.length, 2, "hiding keeps them");
});

test("a moved object snaps to a guide and 눈금자 and 눈금선 only draw", async () => {
  const h = await openSlide();
  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  h.host.__EASYOFFICE_SLIDE_RUN_ACT__("view-guides");
  const { targets } = h.host.__EASYOFFICE_SLIDE_GUIDES__;
  const snap = h.host.__EASYOFFICE_CREATE_SLIDE_SNAP__({ clamp: (v, a, b) => Math.max(a, Math.min(b, v)), round2: (v) => Math.round(v * 100) / 100, SNAP_TOLERANCE_PT: 6, MIN_EL_W: 24, MIN_EL_H: 20 });
  const moved = snap.computeSmartSnap({ x: 75, y: 60, w: 100, h: 50 }, [], { w: 960, h: 540 }, true, "move", "", { v: [123], h: [] });
  assert.equal(moved.x, 73, "the center of the box (125) is pulled onto the guide at 123");
  assert.equal(moved.vLine, 123);
  assert.equal(snap.computeSmartSnap({ x: 432, y: 60, w: 100, h: 50 }, [], { w: 960, h: 540 }, true, "move", "", { v: [], h: [] }).vLine, 480, "a guide-free move still snaps to the center");
  assert.deepEqual(targets(), { v: [480], h: [270] });
  h.host.__EASYOFFICE_SLIDE_RUN_ACT__("view-ruler");
  h.host.__EASYOFFICE_SLIDE_RUN_ACT__("view-grid");
  const layer = h.document.getElementById("slide-guide-layer");
  assert.equal(layer.querySelectorAll(".slide-ruler").length, 2);
  assert.equal(layer.querySelectorAll(".slide-grid").length, 1);
  assert.equal(canvas.getElements().length, 0, "nothing in the slide changes");
});

test("hostile guide lists are cut down to guides that fit", async () => {
  const h = await openSlide();
  const { normalizeGuides, MAX_GUIDES } = h.host.__EASYOFFICE_SLIDE_GUIDES_MATH__;
  const hostile = [
    { orient: "vert", posPt: 10 }, { orient: "diag", posPt: 10 }, { orient: "horz", posPt: 1e9 }, { orient: "horz", posPt: "5" },
    { orient: "horz", posPt: NaN }, { orient: "vert", posPt: -4001 }, null, "x", { orient: "horz", posPt: 4000 }, { orient: "vert", posPt: 0.06 },
  ];
  assert.deepEqual(normalizeGuides(hostile), [{ orient: "vert", posPt: 10 }, { orient: "horz", posPt: 4000 }, { orient: "vert", posPt: 0 }]);
  assert.equal(normalizeGuides(Array(5000).fill({ orient: "vert", posPt: 1 })).length, MAX_GUIDES);
  assert.deepEqual(normalizeGuides("all"), []);
  const fields = h.host.__EASYOFFICE_SLIDE_EXT__.deckFields({ guides: hostile, showGuides: true });
  assert.equal(fields.guides.length, 3);
  assert.equal(fields.showGuides, true);
});
