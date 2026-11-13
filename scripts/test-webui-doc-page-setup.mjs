import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { studioItem } from "./test-support/modules.mjs";

// The page setup dialog of the document window (PD-213): the reference editor's edit-paper dialog, applied as one
// undo step to the editor's page (paper, padding), refused when it leaves no body, carried by the save payload.

async function openDoc(win) {
  const harness = await createWorkspaceHarness({ search: `?module=DOC&win=${win}` });
  await flushAsyncTurns(6);
  const editor = harness.document.getElementById("doc-editor");
  editor.innerHTML = "<p>첫 문단</p><p>둘째 문단</p>";
  harness.host.__EASYOFFICE_DOC_PAGES__.layoutNow();
  const core = harness.host.__EASYOFFICE_DOC_PAGE_SETUP_CORE__;
  const dialog = harness.document.getElementById("doc-page-setup-dialog");
  const field = (id) => harness.document.getElementById("doc-ps-" + id);
  const fire = (el) => el.dispatchEvent({ type: "change", target: el, bubbles: true });
  const type = (id, value) => {
    field(id).value = String(value);
    fire(field(id));
  };
  return { ...harness, editor, core, dialog, field, type, fire };
}

const openDialog = (h) => h.userClick(studioItem(h.document, "layout-paper"));

test("the 편집 용지 item opens the dialog with the paper and margins the page has now", async () => {
  const h = await openDoc("w-ps-open");
  openDialog(h);
  await flushAsyncTurns();
  assert.equal(h.dialog.open, true);
  assert.equal(h.field("paper").value, "A4");
  assert.equal(h.field("width").value, "210.0");
  assert.equal(h.field("height").value, "297.0");
  assert.equal(h.field("width").disabled, true, "a listed paper has a fixed size");
  assert.equal(h.field("portrait").checked, true);
  // The editor's own padding (48px 56px) is the margin of a page without a setup.
  assert.equal(h.field("left").value, "14.8");
  assert.equal(h.field("top").value, "12.7");
});

test("확인 applies the setup as one undo step: the editor's setup, padding and page size follow, undo brings the old ones back", async () => {
  const h = await openDoc("w-ps-apply");
  openDialog(h);
  await flushAsyncTurns();
  h.field("paper").value = "B5";
  h.fire(h.field("paper"));
  assert.equal(h.field("width").value, "181.7", "the paper list sets the width and length");
  h.field("landscape").checked = true;
  h.fire(h.field("landscape"));
  assert.equal(h.field("width").value, "257.0", "turning the page swaps the width and length");
  h.type("left", 30);
  h.type("right", 20);
  h.type("top", 20);
  h.type("header", 15);
  h.type("bottom", 10);
  h.type("footer", 12);
  h.document.getElementById("btn-doc-ps-apply").click();
  await flushAsyncTurns(4);
  assert.equal(h.dialog.open, false);
  const setup = h.core.parse(h.editor.dataset.eoPage);
  assert.ok(setup.width > setup.height, "landscape");
  assert.equal(h.core.twipsToMm(setup.left), 30);
  assert.equal(h.core.twipsToMm(setup.header), 20, "the engine's header distance is the dialog's top margin");
  assert.equal(h.core.twipsToMm(setup.top), 35, "the body starts below the header area");
  assert.equal(h.core.twipsToMm(setup.footer), 12);
  assert.equal(h.core.twipsToMm(setup.bottom), 22);
  assert.equal(h.editor.style.padding.split(" ").length, 4);
  const pages = h.host.__EASYOFFICE_DOC_PAGES_CORE__;
  const geo = pages.pageGeometry({ ...pages.paperOf(h.editor), padding: {} });
  assert.deepEqual([geo.width, geo.height], [Math.round(setup.width / 15 * 100) / 100, Math.round(setup.height / 15 * 100) / 100], "the page layout reads the setup's paper");
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(4);
  assert.equal(h.editor.dataset.eoPage ?? "", "", "one undo takes the whole setup back");
});

test("a setup that leaves less than 10 mm of body is refused with the reference reason and the dialog stays open", async () => {
  const h = await openDoc("w-ps-refuse");
  openDialog(h);
  await flushAsyncTurns();
  h.type("left", 105);
  h.type("right", 105);
  h.document.getElementById("btn-doc-ps-apply").click();
  assert.equal(h.dialog.open, true);
  assert.match(h.document.getElementById("doc-ps-status").textContent, /^좌우 여백이 너무 커서 본문이 남지 않습니다 \(본문 너비 0\.0mm, 최소 10mm\)/);
  assert.equal(h.editor.dataset.eoPage ?? "", "");
  h.type("left", 15);
  h.type("right", 15);
  h.type("top", 140);
  h.type("bottom", 140);
  h.document.getElementById("btn-doc-ps-apply").click();
  assert.match(h.document.getElementById("doc-ps-status").textContent, /^위아래 여백이 너무 커서/);
});

test("a value over the engine's bound is clamped by the field and refused by the check", async () => {
  const h = await openDoc("w-ps-bound");
  const { core } = h;
  assert.equal(core.violation({ width: 11906, height: 16838, left: 31681, right: 0, top: 0, bottom: 0, header: 0, footer: 0 }), "용지 크기와 여백은 558.8mm 이하여야 합니다.");
  assert.equal(core.violation({ width: 11906, height: 16838, left: 1, right: 1, top: 1, bottom: 1, header: 0, footer: 0 }), null);
  assert.notEqual(core.violation({ width: Number.NaN, height: 1, left: 0, right: 0, top: 0, bottom: 0, header: 0, footer: 0 }), null);
  openDialog(h);
  await flushAsyncTurns();
  h.type("left", 9999);
  assert.equal(h.field("left").value, "558.8");
  h.type("top", -5);
  assert.equal(h.field("top").value, "0");
});

test("the save payload carries the setup as pageSetup in the engine's form, and the open answer's setup comes back as layout fields", async () => {
  const h = await openDoc("w-ps-payload");
  const { core } = h;
  const setup = { width: 16838, height: 11906, left: 1700, right: 1500, top: 1985, bottom: 1590, header: 1134, footer: 680 };
  assert.deepEqual(core.payloadOf(core.encode(setup)), {
    pageSetup: { paper: { width: 16838, height: 11906 }, margins: { top: 1985, bottom: 1590, header: 1134, footer: 680, left: 1700, right: 1500 } },
  });
  assert.deepEqual(core.payloadOf(""), {});
  assert.deepEqual(core.payloadOf("1,2,3"), {});
  assert.deepEqual(core.payloadOf(core.encode({ ...setup, header: 2000 })), {}, "a header distance past its body distance is not sent");
  const decoded = core.fromDecoded({ paper: { width: 12240, height: 15840 }, margins: { top: 1440, bottom: 1440, header: 720, footer: 720, left: 1440, right: 1440 } });
  assert.deepEqual(core.layoutFields(decoded), { eoPage: "12240,15840,1440,1440,1440,1440,720,720", padding: "96px 96px 96px 96px", maxWidth: "", paperSize: "" });
  assert.equal(core.fromDecoded(null), null);
  assert.equal(core.fromDecoded({ margins: { top: 1, bottom: 1, header: 1, footer: 1, left: 9000, right: 9000 } }), null, "a setup with no body is not taken");
  assert.deepEqual(core.layoutFields(null), { eoPage: "", padding: "", maxWidth: "", paperSize: "" });
});

test("the page layout draws the paper of the setup and the header follows its margins", async () => {
  const h = await openDoc("w-ps-layout");
  h.core.applyToEditor(h.editor, { width: 16838, height: 11906, left: 1701, right: 1701, top: 1984, bottom: 1700, header: 1134, footer: 850 });
  const pages = h.host.__EASYOFFICE_DOC_PAGES_CORE__;
  const geo = pages.pageGeometry({ ...pages.paperOf(h.editor), padding: {} });
  assert.deepEqual([geo.width, geo.height], [1122.53, 793.73]);
  assert.equal(h.host.__EASYOFFICE_DOC_PAGE_FRAME__.margins().top, 1984);
});

test("fields left as shown keep their exact twips, an edited field changes alone, and the engine's distances come back from the dialog's", async () => {
  const { core } = await openDoc("w-ps-exact");
  const original = { width: 11906, height: 16838, left: 1001, right: 999, top: 1984, bottom: 1700, header: 1134, footer: 850 };
  assert.deepEqual(core.fromFields(core.toFields(original), original), original, "pressing 확인 on an untouched dialog changes nothing");
  const edited = core.fromFields({ ...core.toFields(original), left: 20 }, original);
  assert.equal(edited.left, core.mmToTwips(20));
  assert.deepEqual({ ...edited, left: original.left }, original);
  assert.deepEqual(core.toFields(original), { width: 210, height: 297, left: 17.7, right: 17.6, top: 20, header: 15, bottom: 15, footer: 15 });
});

test("용지 방향 전환 turns the page and 여백 전환 goes through the margin sets, each as one undo step", async () => {
  const h = await openDoc("w-ps-quick");
  h.userClick(studioItem(h.document, "layout-orientation"));
  await flushAsyncTurns(4);
  let setup = h.core.parse(h.editor.dataset.eoPage);
  assert.deepEqual([setup.width, setup.height], [16838, 11906], "A4 turned");
  h.userClick(studioItem(h.document, "layout-orientation"));
  await flushAsyncTurns(4);
  assert.deepEqual([h.core.parse(h.editor.dataset.eoPage).width, h.core.parse(h.editor.dataset.eoPage).height], [11906, 16838]);
  h.userClick(studioItem(h.document, "layout-margins"));
  await flushAsyncTurns(4);
  setup = h.core.parse(h.editor.dataset.eoPage);
  assert.equal(setup.top, 1134, "a page with none of the sets goes to the second (narrow)");
  h.userClick(studioItem(h.document, "layout-margins"));
  await flushAsyncTurns(4);
  assert.equal(h.core.parse(h.editor.dataset.eoPage).top, 2551, "then the wide one");
  h.userClick(studioItem(h.document, "layout-margins"));
  await flushAsyncTurns(4);
  assert.equal(h.core.parse(h.editor.dataset.eoPage).top, 1984, "and round to the reference editor's new-document margins");
  h.userClick(studioItem(h.document, "studio-undo"));
  await flushAsyncTurns(4);
  assert.equal(h.core.parse(h.editor.dataset.eoPage).top, 2551, "undo takes back the last step only");
});

test("a turn that leaves no body is refused and the page stays as it was", async () => {
  const h = await openDoc("w-ps-turn-refused");
  h.core.applyToEditor(h.editor, { width: 3000, height: 16838, left: 1200, right: 1200, top: 1500, bottom: 1500, header: 500, footer: 500 });
  h.userClick(studioItem(h.document, "layout-orientation"));
  await flushAsyncTurns(4);
  assert.deepEqual([h.core.parse(h.editor.dataset.eoPage).width, h.core.parse(h.editor.dataset.eoPage).height], [3000, 16838]);
});
