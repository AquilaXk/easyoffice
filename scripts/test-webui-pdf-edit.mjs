import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, createWebUiDom, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { studioItem, commandItem } from "./test-support/modules.mjs";

// The PDF studio on the opened file's own objects (PD-170): in the desktop shell the opened PDF is
// handed to the shell once, the rotation, stamp, watermark, merge and save controls then work on it,
// and a file the shell refuses leaves the old text-model behaviour untouched. The shell is a stub
// that records its calls; the Rust side has its own tests (`pdf_edit_*`).

const feedback = (document) => document.getElementById("status-command-feedback").textContent;
const PDF_MODEL = { title: "계약서", blocks: [{ type: "paragraph", runs: [{ text: "제1조 목적" }] }] };

async function createPdfShell({ open, save, splitMerge } = {}) {
  const { sandbox } = await createWebUiDom();
  const cat = sandbox.__EASYOFFICE_RIBBON_CATALOG__;
  const offline = sandbox.__EASYOFFICE_CREATE_OFFLINE_RPC__({
    getActiveModule: () => "PDF",
    ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
    ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
    contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
    moduleSpecs: cat.MODULE_SPECS,
    localRibbonState: cat.createDefaultRibbonState(),
  });
  const calls = [];
  const record = (name, args, answer) => {
    calls.push({ name, args });
    return answer;
  };
  const state = (extra = {}) => ({ pages: 3, sources: 1, turn: 0, stamp: null, watermark: null, ...extra });
  const commands = {
    ipc_dispatch: async (args) => JSON.stringify(await offline[args.command](args.payload)),
    report_menu_state: () => null,
    take_pending_open: () => null,
    report_window_state: () => null,
    save_document_file: (args) => record("save_document_file", args, { status: "saved", name: "텍스트.pdf", bytes: 10 }),
    pdf_edit_open: (args) => {
      calls.push({ name: "pdf_edit_open", args });
      if (open) return open(args);
      return state();
    },
    pdf_edit_apply: (args) => record("pdf_edit_apply", args, state()),
    pdf_edit_split_merge: (args) =>
      record("pdf_edit_split_merge", args, splitMerge ? splitMerge() : { action: "merged", count: 2, pages: 7, ...state({ pages: 7 }) }),
    pdf_edit_save: (args) =>
      record("pdf_edit_save", args, save ? save() : { status: "saved", name: "계약서-편집.pdf", bytes: 4096, pages: 3 }),
  };
  const harness = await createWorkspaceHarness({ search: "?module=PDF&win=w-pdf", shellCommands: commands });
  const named = (name) => calls.filter((c) => c.name === name);
  return { ...harness, calls, named };
}

const open = (document) => pickFile(document, makeFixtureFile("계약서.pdf", PDF_MODEL));

test("an opened PDF is handed to the shell once, as base64 of its own bytes", async () => {
  const { document, named } = await createPdfShell();
  await open(document);
  const calls = named("pdf_edit_open");
  assert.equal(calls.length, 1);
  assert.equal(calls[0].args.fileName, "계약서.pdf");
  const bytes = Buffer.from(calls[0].args.base64, "base64").toString("utf8");
  assert.ok(bytes.startsWith("EOFIXTURE:"), "the bytes the page read");
  assert.match(feedback(document), /원본 PDF 편집 사용 \(3쪽 · 그림·글꼴·배치 보존\)/);
});

test("saving sends the page's rotation and overlay first and writes through the shell, not the text model", async () => {
  const { document, named, userClick, calls } = await createPdfShell();
  await open(document);
  userClick(document.getElementById("btn-pdf-rotate"));
  await flushAsyncTurns(12);
  assert.equal(document.getElementById("pdf-rotation-badge").textContent, "회전: 90°");

  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(12);

  assert.deepEqual(named("pdf_edit_apply").map((c) => c.args), [
    { op: { kind: "turn", degrees: 90 } },
    { op: { kind: "overlay", stamp: "", watermark: "" } },
  ]);
  assert.equal(named("pdf_edit_save").length, 1);
  assert.equal(named("save_document_file").length, 0, "the text model is not saved over the original");
  const order = calls.map((c) => c.name).filter((n) => n.startsWith("pdf_edit_"));
  assert.deepEqual(order, ["pdf_edit_open", "pdf_edit_apply", "pdf_edit_apply", "pdf_edit_save"]);
  assert.match(feedback(document), /문서 저장 완료 \(계약서-편집\.pdf · 4096B · 3쪽 · 원본 PDF의 그림·글꼴·배치 보존\)/);
});

test("the stamp and watermark the apply button put on the card are what the shell lays over the pages", async () => {
  const { document, named, userClick } = await createPdfShell();
  await open(document);
  const select = document.getElementById("pdf-stamp-select");
  select.value = "대외비";
  document.getElementById("pdf-watermark-input").value = "초안 · 검토용";
  userClick(document.getElementById("btn-pdf-apply"));
  await flushAsyncTurns(12);
  assert.match(feedback(document), /저장할 때 원본 PDF 위에 겹쳐 씁니다/);

  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(12);
  const ops = named("pdf_edit_apply").map((c) => c.args.op);
  assert.deepEqual(ops.at(-1), { kind: "overlay", stamp: "대외비", watermark: "초안 · 검토용" });
});

test("undoing the rotation before a save sends the undone rotation", async () => {
  const { document, named, userClick } = await createPdfShell();
  await open(document);
  userClick(document.getElementById("btn-pdf-rotate"));
  await flushAsyncTurns(12);
  userClick(studioItem(document, "studio-undo"));
  await flushAsyncTurns(12);
  assert.equal(document.getElementById("pdf-rotation-badge").textContent, "회전: 0°");
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(12);
  assert.deepEqual(named("pdf_edit_apply")[0].args, { op: { kind: "turn", degrees: 0 } });
});

test("a cancelled save dialog is reported as a cancel and nothing else is written", async () => {
  const { document, named, userClick } = await createPdfShell({ save: () => ({ status: "cancelled", name: null, bytes: 0, pages: 0 }) });
  await open(document);
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(12);
  assert.equal(named("save_document_file").length, 0);
  assert.match(feedback(document), /문서 저장 취소 \(PDF\)/);
});

test("a failed save says why and is not reported as saved", async () => {
  const { document, userClick } = await createPdfShell({
    save: () => {
      throw { status: "error", message: "디스크 공간이 부족합니다." };
    },
  });
  await open(document);
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(12);
  assert.match(feedback(document), /문서 저장 실패 \(PDF\): 디스크 공간이 부족합니다\./);
});

const dialogOf = (document) => document.getElementById("pdf-splitmerge-dialog");
const badgeOf = (document) => document.querySelector("#pdf-page-card .pdf-splitmerge-badge")?.textContent;

function choose(document, mode, ranges = "") {
  const select = document.getElementById("pdf-splitmerge-mode");
  select.value = mode;
  select.dispatchEvent({ type: "change", target: select });
  document.getElementById("pdf-splitmerge-range").value = ranges;
}

test("페이지 분할·병합 opens the tool dialog on the opened file and merging asks the shell", async () => {
  const { document, named, userClick } = await createPdfShell();
  await open(document);
  userClick(commandItem(document, "pdf-mod-splitmerge"));
  await flushAsyncTurns(12);
  const dialog = dialogOf(document);
  assert.equal(dialog.open, true);
  assert.equal(named("pdf_edit_split_merge").length, 0, "nothing is done before the dialog is confirmed");
  assert.equal(document.getElementById("pdf-splitmerge-mode").value, "merge");
  assert.equal(document.getElementById("pdf-splitmerge-range").disabled, true, "the range field waits for 범위 분할");
  assert.equal(document.getElementById("pdf-splitmerge-error").hidden, true, "no note until something is wrong");
  userClick(document.getElementById("btn-pdf-splitmerge-run"));
  await flushAsyncTurns(12);
  assert.equal(dialog.open, false);
  assert.deepEqual(named("pdf_edit_split_merge").map((c) => c.args), [{ mode: "merge", ranges: null }]);
  assert.match(badgeOf(document), /PDF 2개를 이어 붙였습니다 \(총 7쪽 · 실행 취소로 되돌릴 수 없습니다\)/);
});

test("a range split needs a range, sends the text as typed and reports the files written", async () => {
  const { document, named, userClick } = await createPdfShell({ splitMerge: () => ({ action: "split", count: 3, pages: 9, sources: 1, turn: 0, stamp: null, watermark: null }) });
  await open(document);
  userClick(commandItem(document, "pdf-mod-splitmerge"));
  await flushAsyncTurns(12);
  choose(document, "range", "");
  assert.equal(document.getElementById("pdf-splitmerge-range").disabled, false);
  userClick(document.getElementById("btn-pdf-splitmerge-run"));
  await flushAsyncTurns(6);
  assert.equal(dialogOf(document).open, true, "an empty range keeps the dialog open");
  assert.equal(document.getElementById("pdf-splitmerge-error").hidden, false);
  assert.match(document.getElementById("pdf-splitmerge-error").textContent, /쪽 범위를 입력해 주세요/);
  assert.equal(named("pdf_edit_split_merge").length, 0);
  choose(document, "range", " 1-3, 5, 7- ");
  userClick(document.getElementById("btn-pdf-splitmerge-run"));
  await flushAsyncTurns(12);
  assert.deepEqual(named("pdf_edit_split_merge").map((c) => c.args), [{ mode: "range", ranges: "1-3, 5, 7-" }]);
  assert.match(badgeOf(document), /쪽 범위 3개를 파일로 나누어 저장했습니다/);
});

test("a split by page sends no range even if one was typed before the mode changed", async () => {
  const { document, named, userClick } = await createPdfShell({ splitMerge: () => ({ action: "split", count: 3, pages: 3, sources: 1, turn: 0, stamp: null, watermark: null }) });
  await open(document);
  userClick(commandItem(document, "pdf-mod-splitmerge"));
  await flushAsyncTurns(12);
  choose(document, "range", "2-3");
  choose(document, "each", "2-3");
  assert.equal(document.getElementById("pdf-splitmerge-range").disabled, true);
  userClick(document.getElementById("btn-pdf-splitmerge-run"));
  await flushAsyncTurns(12);
  assert.deepEqual(named("pdf_edit_split_merge").map((c) => c.args), [{ mode: "each", ranges: null }]);
  assert.match(badgeOf(document), /쪽마다 3개 파일로 나누어 저장했습니다/);
});

test("cancelling the dialog, its close button and Esc do nothing to the file", async () => {
  for (const how of ["cancel", "x", "esc"]) {
    const { document, named, userClick } = await createPdfShell();
    await open(document);
    userClick(commandItem(document, "pdf-mod-splitmerge"));
    await flushAsyncTurns(12);
    const dialog = dialogOf(document);
    assert.equal(dialog.open, true, how);
    if (how === "cancel") userClick(dialog.querySelector(".dialog-footer [data-dialog-close]"));
    else if (how === "x") userClick(dialog.querySelector(".dialog-title [data-dialog-close]"));
    else {
      dialog.close();
      dialog.dispatchEvent({ type: "close", target: dialog });
    }
    await flushAsyncTurns(12);
    assert.equal(dialog.open, false, how);
    assert.equal(named("pdf_edit_split_merge").length, 0, how);
    assert.match(badgeOf(document), /분할·병합을 취소했습니다/, how);
  }
});

test("a merge the shell cancels or refuses is reported, not mistaken for a merge", async () => {
  const cancelled = await createPdfShell({ splitMerge: () => ({ action: "cancelled", count: 0, pages: 3, sources: 1, turn: 0, stamp: null, watermark: null }) });
  await open(cancelled.document);
  cancelled.userClick(commandItem(cancelled.document, "pdf-mod-splitmerge"));
  await flushAsyncTurns(12);
  cancelled.userClick(cancelled.document.getElementById("btn-pdf-splitmerge-run"));
  await flushAsyncTurns(12);
  assert.match(badgeOf(cancelled.document), /분할·병합을 취소했습니다/);

  const refused = await createPdfShell({ splitMerge: () => { throw { status: "error", message: "쪽 범위를 읽을 수 없습니다." }; } });
  await open(refused.document);
  refused.userClick(commandItem(refused.document, "pdf-mod-splitmerge"));
  await flushAsyncTurns(12);
  choose(refused.document, "range", "x");
  refused.userClick(refused.document.getElementById("btn-pdf-splitmerge-run"));
  await flushAsyncTurns(12);
  assert.match(badgeOf(refused.document), /페이지 분할·병합 실패: 쪽 범위를 읽을 수 없습니다\./);
});

test("a PDF the shell refuses keeps the text model path: the reason is shown and saving is the old save", async () => {
  const { document, named, userClick } = await createPdfShell({
    open: () => {
      throw { status: "error", message: "암호화된 PDF는 편집할 수 없습니다. 암호를 해제한 사본을 만든 뒤 다시 열어 주세요." };
    },
  });
  await open(document);
  assert.match(feedback(document), /원본 PDF 편집 불가: 암호화된 PDF는 편집할 수 없습니다/);
  assert.match(feedback(document), /읽어 온 텍스트만 편집합니다/);
  userClick(document.getElementById("btn-pdf-rotate"));
  await flushAsyncTurns(12);
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(12);
  assert.equal(named("pdf_edit_save").length, 0);
  assert.equal(named("pdf_edit_apply").length, 0);
  assert.equal(named("save_document_file").length, 1, "the old path");
});

test("a second PDF replaces the first one's session, and a refused second one leaves no edit active", async () => {
  let refuse = false;
  const { document, named, userClick } = await createPdfShell({
    open: () => {
      if (refuse) throw { status: "error", message: "한도 초과" };
      return { pages: 2, sources: 1, turn: 0, stamp: null, watermark: null };
    },
  });
  await open(document);
  refuse = true;
  await open(document);
  assert.equal(named("pdf_edit_open").length, 2);
  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(12);
  assert.equal(named("pdf_edit_save").length, 0, "the first file's session is not used for the second");
});

test("outside the desktop shell nothing changes: no shell call is made and the apply button works as before", async () => {
  const harness = await createWorkspaceHarness({ search: "?module=PDF&win=w-pdf" });
  const { document, userClick } = harness;
  await pickFile(document, makeFixtureFile("계약서.pdf", PDF_MODEL));
  assert.doesNotMatch(feedback(document), /원본 PDF 편집/);
  userClick(document.getElementById("btn-pdf-apply"));
  await flushAsyncTurns(12);
  assert.doesNotMatch(feedback(document), /겹쳐 씁니다/);
});

test("the error line of the split and merge dialog uses the reference note style, not the blue result style", async () => {
  const { document } = await createWorkspaceHarness({ search: "?module=PDF&win=w-pdf" });
  const note = document.getElementById("pdf-splitmerge-error");
  assert.ok(note.classList.contains("dialog-pdf-note"));
  assert.ok(!note.classList.contains("formula-result"));
  assert.equal(document.querySelector("#pdf-highlight-dialog .dialog-body > p"), null, "the highlight dialog has no note line");
});
