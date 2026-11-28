import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { switchModule } from "./test-support/modules.mjs";

// PD-394: groups, rotation and flips, shape and paragraph formatting, slide background, header and footer
// fields, hidden slides and object animations in the slide editor.

const UI = (name) => fileURLToPath(new URL("../crates/easyoffice-app/ui/" + name, import.meta.url));

function loadModules() {
  const env = {};
  for (const part of ["slide-fx.js", "slide-ext.js", "slide-group.js", "slide-show-anim.js"]) loadUiModule(UI(part), env);
  return env;
}

const el = (id, x, y, w = 100, h = 60, extra = {}) => ({ id, kind: "textbox", x, y, w, h, text: id, ...extra });

test("grouping ties the outermost groups, gathers their elements and expands a selection to the whole group", () => {
  const env = loadModules();
  const g = env.__EASYOFFICE_SLIDE_GROUP__;
  const slide = { elements: [el("a", 0, 0), el("x", 300, 0), el("b", 100, 0), el("c", 200, 0)], groups: [] };
  const first = g.group(slide, ["a", "b"]);
  assert.equal(first, "grp-1");
  const order = slide.elements.map((e) => e.id);
  assert.equal(Math.abs(order.indexOf("a") - order.indexOf("b")), 1, "members sit next to each other");
  const byId = (p, q) => p.localeCompare(q);
  assert.deepEqual(g.expand(slide, "a").sort(byId), ["a", "b"]);
  const outer = g.group(slide, ["grp-1", "c"].flatMap((id) => g.leavesOf(slide, id)));
  assert.equal(outer, "grp-2");
  assert.deepEqual(slide.groups[1].members, ["grp-1", "c"], "a group inside a group stays a group");
  assert.deepEqual(g.expand(slide, "b").sort(byId), ["a", "b", "c"]);
  assert.deepEqual(g.ungroup(slide, ["a"]).sort(byId), ["a", "b", "c"]);
  assert.deepEqual(slide.groups.map((x) => x.id), ["grp-1"], "ungrouping opens the outermost group only");
  assert.equal(g.group(slide, ["x"]), null, "one element is not a group");
});

test("a quarter turn moves the elements about their common center and a flip mirrors them", () => {
  const env = loadModules();
  const g = env.__EASYOFFICE_SLIDE_GROUP__;
  const a = el("a", 0, 0, 100, 40), b = el("b", 200, 0, 100, 40);
  g.transform([a, b], { angle: 90 });
  assert.equal(a.rot, 90);
  assert.equal(b.rot, 90);
  assert.ok(Math.abs(a.x + a.w / 2 - 150) < 0.02 && Math.abs(a.y + a.h / 2 - (20 - 100)) < 0.02, "the left element turns up");
  const c = el("c", 0, 0, 100, 40, { rot: 30 });
  g.transform([c], { flipH: true });
  assert.equal(c.flipH, true);
  assert.equal(c.rot, 330, "a mirror negates the element's own turn");
  g.transform([c], { flipH: true });
  assert.equal(c.flipH, undefined);
  assert.equal(c.rot, 30);
  const d = el("d", 0, 0, 100, 40), e2 = el("e", 200, 0, 100, 40);
  g.transform([d, e2], { flipH: true });
  assert.equal(d.x, 200);
  assert.equal(e2.x, 0);
});

test("effects are kept for text boxes, shapes and pictures, cleaned up and left off tables and charts", () => {
  const { __EASYOFFICE_SLIDE_FX__: fx } = loadModules();
  const raw = { rot: 400, flipH: true, dash: "dash", outline: { color: "#f00", widthPt: 99 }, shadow: { color: "000", alphaPct: 140, blurPt: 4, distPt: 3, angleDeg: -45 }, para: { justify: true, bullet: { kind: "number", style: "bogus" }, lineSpacingPct: 150, spaceBeforePt: 6 } };
  const box = fx.normalize(raw, "textbox");
  assert.equal(box.rot, 40);
  assert.deepEqual(box.outline, { color: "FF0000", widthPt: 24 });
  assert.equal(box.shadow.alphaPct, 100);
  assert.equal(box.shadow.angleDeg, 315);
  assert.deepEqual(box.para, { justify: true, bullet: { kind: "number", style: "arabicPeriod" }, lineSpacingPct: 150, spaceBeforePt: 6 });
  assert.equal(fx.normalize(raw, "shape").outline, undefined, "a shape keeps its line on the element");
  assert.equal(fx.normalize(raw, "image").para, undefined);
  assert.deepEqual(fx.normalize(raw, "table"), {});
  assert.deepEqual(fx.normalize({ rot: 0, dash: "solid", para: { lineSpacingPct: 100 } }, "textbox"), {});
});

test("numbered paragraphs count from one in the chosen style", () => {
  const { __EASYOFFICE_SLIDE_FX__: fx } = loadModules();
  const marks = (style, n) => Array.from({ length: n }, (_, i) => fx.markerFor({ kind: "number", style }, i));
  assert.deepEqual(marks("arabicPeriod", 3), ["1.", "2.", "3."]);
  assert.deepEqual(marks("arabicParenR", 2), ["1)", "2)"]);
  assert.deepEqual(marks("alphaUcPeriod", 28).slice(-3), ["Z.", "AA.", "AB."]);
  assert.deepEqual(marks("romanUcPeriod", 4), ["I.", "II.", "III.", "IV."]);
  assert.equal(fx.markerFor({ kind: "char", char: "•" }, 5), "•");
});

test("slide state keeps what is valid and dissolves groups and animations that lost their targets", () => {
  const { __EASYOFFICE_SLIDE_EXT__: ext } = loadModules();
  const ids = new Set(["a", "b", "c"]);
  const out = ext.normalize({
    hidden: true,
    background: { kind: "gradient", from: "#123", to: "abcdef", angleDeg: -90 },
    footer: { number: true, date: { kind: "fixed", text: "2026-11-28" }, text: "" },
    groups: [{ id: "g1", members: ["a", "b"] }, { id: "g2", members: ["g1", "c"] }, { id: "g3", members: ["a"] }],
    anims: [
      { target: "g2", class: "entrance", effect: "fly", direction: "left", durationMs: 99999 },
      { target: "gone", class: "exit", effect: "fade" },
      { target: "a", class: "emphasis", effect: "fly" },
    ],
  }, ids);
  assert.equal(out.hidden, true);
  assert.deepEqual(out.background, { kind: "gradient", from: "112233", to: "ABCDEF", angleDeg: 270 });
  assert.deepEqual(out.footer, { number: true, date: { kind: "fixed", text: "2026-11-28" } });
  assert.deepEqual(out.groups.map((g) => g.id), ["g1", "g2"]);
  assert.deepEqual(out.anims.map((a) => a.target), ["g2"]);
  assert.equal(out.anims[0].durationMs, 60000);
  const pruned = ext.pruneRefs([{ id: "g1", members: ["a", "b"] }], [{ target: "g1", class: "exit", effect: "appear" }], new Set(["a"]));
  assert.deepEqual(pruned.groups, [], "a group down to one member dissolves");
  assert.equal(pruned.anims[0].target, "a", "an animation of a dissolved group follows its member");
});

test("animation clicks: each click starts a step, with-previous joins it and after-previous starts a later group", () => {
  const { __EASYOFFICE_SHOW_CLICK_STEPS__: steps } = loadModules();
  const anim = (target, start) => ({ target, start });
  const clicks = steps([anim("a", "onClick"), anim("b", "withPrevious"), anim("c", "afterPrevious"), anim("d", "onClick")]);
  assert.equal(clicks.length, 2);
  assert.deepEqual(clicks[0].map((group) => group.map((a) => a.target)), [["a", "b"], ["c"]]);
  assert.deepEqual(steps([anim("x", "afterPrevious"), anim("y", "onClick")]).map((c) => c.length), [1, 1], "an effect that does not start on click opens the first step");
});

async function openSlide() {
  const h = await createWorkspaceHarness({ search: "?module=SLIDE&win=w-slide-pd394" });
  await flushAsyncTurns(10);
  switchModule(h.document, "SLIDE");
  await flushAsyncTurns(6);
  return h;
}

const act = (doc, name) => {
  const item = doc.querySelector(`#menu-bar [data-slide-act="${name}"]`);
  assert.ok(item, `missing menu item ${name}`);
  return item;
};

test("the editor groups, turns and flips the selection and keeps it through the save payload", async () => {
  const { document, host, userClick } = await openSlide();
  const canvas = host.__EASYOFFICE_SLIDE_CANVAS__;
  canvas.addElement({ kind: "textbox", x: 100, y: 100, w: 200, h: 80, text: "하나" });
  canvas.addElement({ kind: "shape", x: 400, y: 100, w: 160, h: 80, shapeType: "ellipse", text: "둘" });
  canvas.setSelectedIds(canvas.getElements().map((e) => e.id));
  userClick(act(document, "group"));
  await flushAsyncTurns(4);
  const slide = canvas.currentSlide();
  assert.equal(slide.groups.length, 1);
  canvas.selectElement(canvas.getElements()[0].id);
  assert.equal(canvas.getSelectedIds().length, 2, "clicking one member selects the group");
  userClick(act(document, "rotate-right"));
  await flushAsyncTurns(4);
  assert.deepEqual(canvas.getElements().map((e) => e.rot), [90, 90]);
  const card = document.querySelector("#slide-canvas-layer .slide-el");
  assert.equal(card.style.transform, "rotate(90deg)");
  assert.ok(card.querySelector(".slide-el-rotate"), "a selected element has the rotation handle");
  userClick(act(document, "flip-h"));
  await flushAsyncTurns(4);
  assert.deepEqual(canvas.getElements().map((e) => Boolean(e.flipH)), [true, true]);
  const codec = host.__EASYOFFICE_SLIDE_CANVAS_CODEC__;
  const payload = codec.serializeDeckPayload([slide], document, null);
  assert.equal(payload.slides[0].elements[0].rot, 270);
  assert.equal(payload.slides[0].elements[0].flipH, true);
  assert.equal(payload.slides[0].groups[0].members.length, 2);
  userClick(act(document, "ungroup"));
  await flushAsyncTurns(4);
  assert.equal(canvas.currentSlide().groups.length, 0);
});

test("hiding a slide dims its thumbnail and the show skips it", async () => {
  const { document, sandbox, userClick } = await openSlide();
  const add = document.getElementById("btn-add-slide");
  userClick(add);
  userClick(add);
  await flushAsyncTurns(4);
  const thumbs = () => Array.from(document.querySelectorAll("#slide-thumb-list .slide-thumb"));
  assert.equal(thumbs().length, 3);
  userClick(thumbs()[1]);
  await flushAsyncTurns(3);
  userClick(act(document, "hide-slide"));
  await flushAsyncTurns(4);
  assert.equal(thumbs()[1].classList.contains("is-hidden-slide"), true);
  assert.match(thumbs()[1].getAttribute("aria-label"), /숨김/);
  assert.equal(act(document, "hide-slide").getAttribute("aria-checked"), "true");
  userClick(document.getElementById("btn-slide-show"));
  const counter = () => document.getElementById("slide-show-counter").textContent;
  assert.equal(counter(), "1 / 3");
  sandbox.dispatchWindowKeydown({ key: "ArrowRight" });
  assert.equal(counter(), "3 / 3", "the hidden second slide is skipped");
  sandbox.dispatchWindowKeydown({ key: "ArrowLeft" });
  assert.equal(counter(), "1 / 3");
  sandbox.dispatchWindowKeydown({ key: "Escape" });
});
