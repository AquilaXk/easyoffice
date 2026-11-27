import assert from "node:assert/strict";
import test from "node:test";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { createSelection, rangeOf } from "./test-support/doc-run-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";

// PD-368: the Pages sidebar's style popup lists the variants of the selection's font from the shell's installed fonts, and a
// chosen variant is the run's font name and face (the PostScript name a `.pages` file keeps).

const GROUP = {
  family: "Helvetica Neue",
  styles: [
    { face: "HelveticaNeue-Light", fullName: "Helvetica Neue Light", style: "가는체", weight: 300, width: 100, italic: false },
    { face: "HelveticaNeue", fullName: "Helvetica Neue", style: "일반체", weight: 400, width: 100, italic: false },
    { face: "HelveticaNeue-Bold", fullName: "Helvetica Neue Bold", style: "볼드체", weight: 700, width: 100, italic: false },
    { face: "HelveticaNeue-CondensedBold", fullName: "Helvetica Neue Condensed Bold", style: "자간 축소 볼드체", weight: 700, width: 75, italic: false },
  ],
};

const FACED_RUN = { text: "본문 글", font: "Helvetica Neue Light", fontFace: "HelveticaNeue-Light" };

async function pagesHarness({ shell, run: modelRun = FACED_RUN } = {}) {
  const selection = createSelection();
  const requested = [];
  const h = await createWorkspaceHarness({
    selection,
    search: "?module=DOC&win=w-pd368",
    ...(shell ? { shellCommands: { list_font_styles: async ({ name }) => (requested.push(name), shell(name)) } } : {}),
    testIpc: async (cmd, raw) => {
      if (cmd !== "convert_document_format") return null;
      const req = JSON.parse(raw || "{}");
      if (req.action === "decode" && String(req.fileName || "").endsWith(".pages")) {
        return {
          status: "OK", module: "DOC", format: "pages", kind: "blocks", itemCount: 1, title: "f.pages",
          blocks: [{ type: "paragraph", runs: [modelRun] }],
        };
      }
      return null;
    },
  });
  await flushAsyncTurns(10);
  await pickFile(h.document, makeFixtureFile("f.pages", "pages-bytes"));
  await flushAsyncTurns(16);
  const editor = h.document.getElementById("doc-editor");
  const run = () => editor.querySelector(".eo-run");
  const select = async () => {
    const text = run().firstChild;
    selection.addRange(rangeOf(text, 0, text, text.textContent.length));
    h.document.dispatchEvent({ type: "selectionchange" });
    await flushAsyncTurns(12);
  };
  return { h, doc: h.document, editor, run, select, requested, face: () => h.document.getElementById("pages-insp-face") };
}

const labels = (select) => [...select.querySelectorAll("option")].map((o) => o.textContent);

test("a run's face is kept on open, drawn first in the font list and saved as fontFace", async () => {
  const { h, editor, run } = await pagesHarness();
  assert.equal(run().dataset.eoFace, "HelveticaNeue-Light");
  assert.equal(run().dataset.eoFont, "Helvetica Neue Light");
  assert.equal(run().style.fontFamily, '"HelveticaNeue-Light", "Helvetica Neue Light"');
  const saved = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor).flatMap((b) => b.runs || []);
  assert.equal(saved[0].fontFace, "HelveticaNeue-Light");
  assert.equal(saved[0].font, "Helvetica Neue Light");
});

test("a run with only a letter spacing or only a face is a run of its own on open", async () => {
  const spaced = await pagesHarness({ run: { text: "자간", letterSpacingTwips: 40 } });
  assert.equal(spaced.run()?.dataset.eoSpacing, "40");
  const faced = await pagesHarness({ run: { text: "서체", fontFace: "AppleSDGothicNeo-Light" } });
  assert.equal(faced.run()?.dataset.eoFace, "AppleSDGothicNeo-Light");
  assert.equal(faced.run()?.style.fontFamily, '"AppleSDGothicNeo-Light"');
});

test("hostile face names are dropped", async () => {
  const { h } = await pagesHarness();
  const R = h.host.__EASYOFFICE_DOC_RUNS__;
  for (const bad of ['a"b', "a b", "a;b", "a/b", "한글", "a\u0000b", "x".repeat(129), "<b>", "a{b}"]) {
    assert.deepEqual(R.fromModelRun({ fontFace: bad }), {}, JSON.stringify(bad));
  }
  assert.deepEqual(R.fromModelRun({ fontFace: "x".repeat(128) }), { face: "x".repeat(128) });
});

test("without the shell the popup holds 보통 alone and stays off", async () => {
  const { select, face } = await pagesHarness();
  await select();
  assert.deepEqual(labels(face()), ["보통"]);
  assert.equal(face().disabled, true);
  const siblings = [...face().closest(".pages-insp-section").children].map((el) => el.querySelector?.("#pages-insp-face") ? "face" : el.className);
  assert.ok(siblings.indexOf("face") === siblings.indexOf("pages-insp-seg") - 1, "directly above the buttons: " + siblings.join(","));
});

test("the popup lists the font's variants with their own names and shows the run's", async () => {
  const { select, face, requested } = await pagesHarness({ shell: () => GROUP });
  await select();
  assert.deepEqual(labels(face()), ["가는체", "일반체", "볼드체", "자간 축소 볼드체"]);
  assert.equal(face().value, "HelveticaNeue-Light", "the run's face is shown");
  assert.equal(face().disabled, false);
  assert.deepEqual(requested, ["HelveticaNeue-Light"], "asked once by the face the run has");
});

test("choosing a variant sets the font name and face, and the plain one is the family itself", async () => {
  const { h, doc, editor, select, face } = await pagesHarness({ shell: () => GROUP });
  await select();
  const choose = async (value) => {
    await select();
    face().value = value;
    face().dispatchEvent({ type: "change", target: face(), bubbles: true });
    await flushAsyncTurns(12);
  };
  await choose("HelveticaNeue-CondensedBold");
  const run = editor.querySelector('[data-eo-face="HelveticaNeue-CondensedBold"]');
  assert.ok(run, "the variant is on the run");
  assert.equal(run.dataset.eoFont, "Helvetica Neue Condensed Bold");
  const saved = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor).flatMap((b) => b.runs || []);
  assert.deepEqual([saved[0].font, saved[0].fontFace], ["Helvetica Neue Condensed Bold", "HelveticaNeue-CondensedBold"]);

  await choose("HelveticaNeue");
  assert.equal(editor.querySelector("[data-eo-face]"), null, "the plain variant takes the face off");
  assert.equal(editor.querySelector('[data-eo-font="Helvetica Neue"]') !== null, true, "and names the family");
  assert.ok(doc.getElementById("pages-insp-face"));
});

test("picking another font in the font list drops the variant", async () => {
  const { h, editor, select } = await pagesHarness({ shell: () => GROUP });
  await select();
  const fontSelect = h.document.getElementById("doc-font-select");
  const option = h.document.createElement("option");
  option.value = "Arial";
  option.textContent = "Arial";
  fontSelect.appendChild(option);
  await select();
  fontSelect.value = "Arial";
  fontSelect.dispatchEvent({ type: "change", target: fontSelect, bubbles: true });
  await flushAsyncTurns(8);
  assert.equal(editor.querySelector("[data-eo-face]"), null);
  assert.ok(editor.querySelector('[data-eo-font="Arial"]'));
});

test("a shell answer that is not what it should be, or a failing shell, leaves 보통", async () => {
  for (const answer of [
    () => ({ family: "X", styles: [{ face: "a b", fullName: "X", style: "s", weight: 400, width: 100 }] }),
    () => ({ family: "X", styles: [{ face: "X-Light", fullName: "X Light", style: "<b>\u0000", weight: "300", width: 100 }] }),
    () => ({ family: "", styles: [] }),
    () => null,
    () => {
      throw new Error("no shell");
    },
  ]) {
    const { select, face } = await pagesHarness({ shell: answer });
    await select();
    assert.deepEqual(labels(face()), ["보통"]);
    assert.equal(face().disabled, true);
  }
});

test("at most 64 variants of an answer are shown", async () => {
  const many = {
    family: "Many",
    styles: Array.from({ length: 200 }, (_, i) => ({ face: `Many-W${i}`, fullName: `Many W${i}`, style: `굵기 ${i}`, weight: 100 + (i % 9) * 100, width: 100, italic: false })),
  };
  const { select, face } = await pagesHarness({ shell: () => many });
  await select();
  assert.equal(face().querySelectorAll("option").length, 64);
});
