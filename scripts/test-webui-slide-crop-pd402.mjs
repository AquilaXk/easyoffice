import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";

// PD-402: 자르기 of a slide picture (`a:srcRect`).

const PNG = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

async function openSlide() {
  const h = await createWorkspaceHarness({ search: "?module=SLIDE&win=w-slide-crop-pd402" });
  await flushAsyncTurns(10);
  switchModule(h.document, "SLIDE");
  await flushAsyncTurns(6);
  return h;
}

test("the left edge dragged in cuts the picture and moves the frame so the visible part stays; the trailing edge gives it back", async () => {
  const h = await openSlide();
  const { cropDrag } = h.host.__EASYOFFICE_SLIDE_CROP_MATH__;
  const el = { x: 100, y: 50, w: 200, h: 100 };
  const cut = cropDrag(el, "w", 50, 0);
  assert.deepEqual(cut.crop, { left: 25000, top: 0, right: 0, bottom: 0 });
  assert.equal(cut.x, 150);
  assert.equal(cut.w, 150);
  // The frame of 200 pt shows 80% of the image (250 pt whole); pulling its right edge out by 20 pt gives back 8% of the image.
  const right = cropDrag({ ...el, crop: { left: 0, top: 0, right: 20000, bottom: 0 } }, "e", 20, 0);
  assert.equal(right.crop.right, 12000);
  assert.equal(right.w, 220);
  const corner = cropDrag(el, "se", -50, -25);
  assert.deepEqual(corner.crop, { left: 0, top: 0, right: 25000, bottom: 25000 });
  assert.deepEqual([corner.w, corner.h], [150, 75]);
});

test("a drag never leaves less than 1% of the picture or a frame under the minimum size", async () => {
  const h = await openSlide();
  const { cropDrag } = h.host.__EASYOFFICE_SLIDE_CROP_MATH__;
  const el = { x: 0, y: 0, w: 200, h: 100 };
  const far = cropDrag(el, "w", 100000, 0);
  assert.ok(far.crop.left <= 99000);
  assert.ok(far.w >= 24, `frame ${far.w}`);
  const both = cropDrag({ ...el, crop: { left: 0, top: 0, right: 90000, bottom: 0 } }, "w", 100000, 0);
  assert.ok(both.crop.left + both.crop.right <= 99000);
  const back = cropDrag({ ...el, crop: { left: 10000, top: 0, right: 0, bottom: 0 } }, "w", -1e9, 0);
  assert.equal(back.crop.left, 0, "dragging out past the picture restores it and no more");
});

test("the 자르기 menu item turns the mode on for a picture and the picture draws inside a cutting frame", async () => {
  const h = await openSlide();
  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  assert.equal(h.host.__EASYOFFICE_SLIDE_RUN_ACT__("image-crop"), true);
  assert.match(h.document.getElementById("status-command-feedback").textContent, /그림을 먼저 선택하세요/);
  canvas.addElement({ kind: "image", x: 80, y: 80, w: 200, h: 100, data: PNG, alt: "그림", crop: { left: 25000, top: 0, right: 25000, bottom: 0 } });
  const el = canvas.getElements()[0];
  assert.deepEqual(el.crop, { left: 25000, top: 0, right: 25000, bottom: 0 });
  const img = h.document.getElementById("slide-canvas-layer").querySelector(".slide-el-crop .slide-el-image");
  assert.ok(img, "a cropped picture sits in the cutting frame");
  assert.equal(img.style.width, "200%");
  assert.equal(img.style.left, "-50%");
  canvas.selectElement(el.id);
  h.host.__EASYOFFICE_SLIDE_RUN_ACT__("image-crop");
  assert.match(h.document.getElementById("status-command-feedback").textContent, /가장자리 핸들을 끌어 자릅니다/);
});

test("a padded srcRect of a file is kept as it is and dropped when it does not fit", async () => {
  const h = await openSlide();
  const { normalizeSrcPad } = h.host.__EASYOFFICE_SLIDE_FX__;
  assert.deepEqual(normalizeSrcPad([-5000, 2000, -1000, 0]), [-5000, 2000, -1000, 0]);
  for (const bad of [[1, 2, 3, 4], [-5000000, 0, 0, 0], [-1, 99000, 0, 1], [-1, 0, 0, 60000, 1], [-1.5, 0, 0, 0], ["-1", 0, 0, 0], null, "x", [-1, 60000, 0, 60000]]) {
    assert.equal(normalizeSrcPad(bad), null, JSON.stringify(bad));
  }
  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  canvas.addElement({ kind: "image", x: 10, y: 10, w: 100, h: 60, data: PNG, alt: "그림", srcPad: [-5000, 0, 0, 0] });
  assert.deepEqual(canvas.getElements()[0].srcPad, [-5000, 0, 0, 0]);
  canvas.updateElement(canvas.getElements()[0].id, { crop: { left: 1000, top: 0, right: 0, bottom: 0 } });
  assert.equal(canvas.getElements()[0].srcPad, undefined, "a crop replaces the padding");
});

test("a crop that does not fit is dropped from the element", async () => {
  const h = await openSlide();
  const canvas = h.host.__EASYOFFICE_SLIDE_CANVAS__;
  for (const crop of [{ left: 60000, right: 60000 }, { left: -1 }, { left: 99001 }, { left: 1.5 }, { left: "5" }, "x", { left: 0, top: 0, right: 0, bottom: 0 }, null]) {
    canvas.addElement({ kind: "image", x: 10, y: 10, w: 100, h: 60, data: PNG, alt: "그림", crop });
  }
  assert.ok(canvas.getElements().every((e) => e.crop === undefined), JSON.stringify(canvas.getElements().map((e) => e.crop)));
  assert.equal(h.host.__EASYOFFICE_SLIDE_FX__.normalizeCrop({ left: 99000 }).left, 99000);
});
