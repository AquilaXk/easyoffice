import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem } from "./test-support/modules.mjs";
import { build, createSelection, p, rangeOf } from "./test-support/doc-run-harness.mjs";

// 텍스트 줄 바꿈, 자르기 and 회전 of a picture in the word processor window (PD-395).

function pngBytes(width, height) {
  const out = new Uint8Array(57);
  const view = new DataView(out.buffer);
  out.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], 0);
  view.setUint32(8, 13, false);
  out.set([0x49, 0x48, 0x44, 0x52], 12);
  view.setUint32(16, width, false);
  view.setUint32(20, height, false);
  out.set([8, 2, 0, 0, 0], 24);
  out.set([0x49, 0x44, 0x41, 0x54], 37);
  out.set([0x49, 0x45, 0x4e, 0x44], 49);
  view.setUint32(53, 0xae426082, false);
  return out;
}

async function pictureHarness() {
  const selection = createSelection();
  const h = await createWorkspaceHarness({ selection, search: "?module=DOC&win=w-pd395-image" });
  await flushAsyncTurns(10);
  const editor = h.document.getElementById("doc-editor");
  editor.replaceChildren();
  const para = build(h.document, editor, p("본문"));
  selection.addRange(rangeOf(para, 0, para, 0));
  h.host.__EASYOFFICE_DOC_IMAGE_API__.insertImageBytes(pngBytes(100, 50), "도표.png");
  const img = editor.querySelector("img.doc-inline-image");
  assert.ok(img, "a picture to work on");
  return { ...h, editor, img };
}

const click = (h, img) => img.dispatchEvent({ type: "click", target: img });
const run = async (h, cmd) => {
  h.userClick(studioItem(h.document, cmd));
  await flushAsyncTurns(8);
};
const saved = (h) => h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(h.editor).find((b) => b.type === "image");

test("the 그림 서식 menu lists the wraps, 자르기 and the two turns", async () => {
  const h = await pictureHarness();
  const sub = [...h.document.querySelectorAll(".md-sub")].find((s) => s.querySelector(":scope > .md-label")?.textContent === "그림 서식");
  assert.ok(sub, "the submenu");
  assert.deepEqual(
    [...sub.querySelectorAll(".md-item .md-label")].map((l) => l.textContent),
    ["줄 안", "정사각형", "위/아래", "텍스트 뒤", "텍스트 앞", "자르기", "오른쪽으로 90도 회전", "왼쪽으로 90도 회전"],
  );
});

test("a wrap is kept on the picture, saved, and undone as one step", async () => {
  const h = await pictureHarness();
  click(h, h.img);
  await run(h, "image-wrap-square");
  assert.equal(h.img.dataset.eoImageWrap, "square");
  assert.match(h.img.className, /eo-wrap-square/);
  assert.equal(saved(h).wrap, "square");
  await run(h, "image-wrap-inline");
  assert.equal(h.img.dataset.eoImageWrap, undefined, "the line is the default and leaves no data");
  assert.equal(saved(h).wrap, undefined);
  h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(20);
  assert.equal(h.editor.querySelector("img.doc-inline-image").dataset.eoImageWrap, "square");
});

test("the turn commands add 90 degrees and wrap around 360", async () => {
  const h = await pictureHarness();
  click(h, h.img);
  await run(h, "image-rotate-right");
  assert.equal(h.img.dataset.eoImageRotation, "90");
  assert.equal(h.img.style.transform, "rotate(90deg)");
  await run(h, "image-rotate-left");
  await run(h, "image-rotate-left");
  assert.equal(h.img.dataset.eoImageRotation, "270");
  assert.equal(saved(h).rotation, 270);
});

test("a command without a selected picture says so and changes nothing", async () => {
  const h = await pictureHarness();
  h.host.__EASYOFFICE_DOC_IMAGE_API__.clearSelection();
  const before = h.editor.innerHTML;
  await run(h, "image-rotate-right");
  assert.equal(h.editor.innerHTML, before);
  assert.equal(h.document.getElementById("status-command-feedback").textContent, "그림을 먼저 선택하세요");
});

test("dragging a cut handle cuts the side, stays inside the bound, and is one undo step", async () => {
  const h = await pictureHarness();
  click(h, h.img);
  await run(h, "image-crop");
  const frame = h.document.getElementById("doc-image-resizer");
  assert.equal(frame.classList.contains("is-cropping"), true);
  const west = frame.querySelector('[data-crop-dir="w"]');
  west.dispatchEvent({ type: "pointerdown", target: west, clientX: 0, clientY: 10 });
  h.document.dispatchEvent({ type: "pointermove", clientX: 25, clientY: 10 });
  h.document.dispatchEvent({ type: "pointerup", clientX: 25, clientY: 10 });
  assert.deepEqual(saved(h).crop, { left: 25000, top: 0, right: 0, bottom: 0 }, "25 of 100 px is a quarter of the width");
  assert.match(h.img.style.clipPath, /inset\(0% 0% 0% 25%\)/);
  west.dispatchEvent({ type: "pointerdown", target: west, clientX: 0, clientY: 10 });
  h.document.dispatchEvent({ type: "pointermove", clientX: 100000, clientY: 10 });
  h.document.dispatchEvent({ type: "pointerup", clientX: 100000, clientY: 10 });
  assert.equal(saved(h).crop.left, 99000, "a side never cuts the whole picture");
  h.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(20);
  assert.equal(h.editor.querySelector("img.doc-inline-image").dataset.eoImageCrop, "25000,0,0,0");
});

test("the rotate handle turns the picture toward the pointer and Shift snaps to 15 degrees", async () => {
  const h = await pictureHarness();
  h.img.getBoundingClientRect = () => ({ left: 0, top: 0, width: 100, height: 100 });
  click(h, h.img);
  const handle = h.document.querySelector("[data-rotate-handle]");
  handle.dispatchEvent({ type: "pointerdown", target: handle, clientX: 50, clientY: -20 });
  h.document.dispatchEvent({ type: "pointermove", clientX: 150, clientY: 50, shiftKey: false });
  h.document.dispatchEvent({ type: "pointerup", clientX: 150, clientY: 50 });
  assert.equal(h.img.dataset.eoImageRotation, "90", "the pointer is to the right of the centre");
  handle.dispatchEvent({ type: "pointerdown", target: handle, clientX: 150, clientY: 50 });
  h.document.dispatchEvent({ type: "pointermove", clientX: 150, clientY: 10, shiftKey: true });
  h.document.dispatchEvent({ type: "pointerup", clientX: 150, clientY: 10 });
  assert.equal(h.img.dataset.eoImageRotation, String(Math.round(Math.atan2(100, 40) * (180 / Math.PI) / 15) * 15));
});

test("an opened picture shows its wrap, crop and turn, and a hostile value reads as the default", async () => {
  const h = await pictureHarness();
  const model = h.host.__EASYOFFICE_DOC_WORD_MODEL__;
  const spec = model.image({ attrs: {}, data: {}, style: {} }, { wrap: "front", crop: { left: 10000, top: 0, right: 20000, bottom: 5000 }, rotation: 90 });
  assert.equal(spec.attrs.class, "doc-inline-image eo-wrap-front");
  assert.equal(spec.data.eoImageCrop, "10000,0,20000,5000");
  assert.equal(spec.style.clipPath, "inset(0% 20% 5% 10%)");
  assert.equal(spec.style.transform, "rotate(90deg)");
  const hostile = model.image({ attrs: {}, data: {}, style: {} }, { wrap: "tight", crop: { left: 99000, right: 99000 }, rotation: 400 });
  assert.equal(hostile.attrs.class, "doc-inline-image");
  assert.deepEqual(hostile.data, {});
  assert.deepEqual(hostile.style, {});
  const node = { dataset: { eoImageWrap: "front", eoImageCrop: "1,2,3", eoImageRotation: "-5" } };
  assert.deepEqual(model.imageLayoutOf(node), { wrap: "front", crop: { left: 0, top: 0, right: 0, bottom: 0 }, rotation: 0 });
});
