import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { commandItem, studioItem, switchModule } from "./test-support/modules.mjs";
import { build, createSelection, p, rangeOf } from "./test-support/doc-run-harness.mjs";
import { readText } from "./test-support/read-text.mjs";
import { itemTable } from "./test-support/rust-menu-table.mjs";

const REPO = new URL("../", import.meta.url);
const read = (rel) => readText(fileURLToPath(new URL(rel, REPO)));

function rustItems() {
  const source = read("crates/easyoffice-desktop/src/native_menu/items.rs");
  const table = itemTable(source);
  return [...table.matchAll(/item\(\s*"([^"]+)",\s*"([^"]+)",\s*(None|Some\("([^"]+)"\)),\s*"([^"]+)",?\s*\)/g)].map(
    (m) => ({ id: m[1], label: m[2], accelerator: m[4] ?? null, action: m[5] }),
  );
}

test("verbatim Korean labels, sprite cell, native menu parity, and zero fake placeholder injections", async () => {
  const selection = createSelection();
  const h = await createWorkspaceHarness({ selection, search: "?module=DOC&win=w-pd252-labels" });
  await flushAsyncTurns(10);
  const { document, userClick } = h;
  const editor = document.getElementById("doc-editor");

  const fnBtn = document.querySelector('#icon-toolbar .tb-btn[data-studio-cmd="mod-footnote"]');
  const enBtn = document.querySelector('#icon-toolbar .tb-btn[data-studio-cmd="mod-endnote"]');
  assert.ok(fnBtn && enBtn, "각주 and 미주 toolbar buttons exist");
  assert.equal(fnBtn.getAttribute("title"), "각주");
  assert.equal(fnBtn.querySelector(".tb-label")?.textContent.trim(), "각주");
  assert.equal(enBtn.hidden, false);
  assert.equal(enBtn.getAttribute("title"), "미주");
  assert.equal(enBtn.getAttribute("aria-label"), "미주");
  assert.equal(enBtn.querySelector(".tb-label")?.textContent.trim(), "미주");
  assert.ok(enBtn.querySelector(".tb-sprite.icon-endnote"), "uses .icon-endnote sprite");
  const groupKids = fnBtn.parentElement.children;
  assert.ok(groupKids[groupKids.indexOf(fnBtn) + 1] === enBtn, "미주 sits right next to 각주 in #icon-toolbar");

  const toolbarCss = read("crates/easyoffice-app/ui/toolbar.css");
  assert.match(toolbarCss, /\.icon-endnote\s*\{\s*background-position:\s*calc\(-40px \* 5\)\s*calc\(-40px \* 4\);\s*\}/);

  assert.equal(studioItem(document, "mod-footnote")?.querySelector(".md-label")?.textContent, "각주");
  const enMenu = studioItem(document, "mod-endnote");
  assert.ok(enMenu && !enMenu.hidden, "입력 > 미주 visible in DOC");
  assert.equal(enMenu.querySelector(".md-label")?.textContent, "미주");

  const trackMenu = studioItem(document, "review-track");
  assert.equal(trackMenu?.querySelector(".md-label")?.textContent, "변경 내용 추적");
  assert.equal(trackMenu?.getAttribute("role"), "menuitemcheckbox");
  assert.equal(trackMenu?.getAttribute("aria-checked"), "false");

  const acceptAllMenu = studioItem(document, "review-accept-all");
  const rejectAllMenu = studioItem(document, "review-reject-all");
  assert.ok(acceptAllMenu && !acceptAllMenu.hidden);
  assert.ok(rejectAllMenu && !rejectAllMenu.hidden);
  assert.equal(acceptAllMenu.querySelector(".md-label")?.textContent, "모든 변경 내용 수락");
  assert.equal(rejectAllMenu.querySelector(".md-label")?.textContent, "모든 변경 내용 거부");

  const commentMenus = document.querySelectorAll('.md-item[data-studio-menu-cmd="review-comment"]');
  // The 도구 menu of the Pages window names it 주석 추가 (PD-399).
  assert.equal(commentMenus.length, 3);
  for (const item of commentMenus) {
    assert.equal(item.querySelector(".md-label")?.textContent, item.dataset.kinds === "PAGES" ? "주석 추가" : "새 메모");
  }
  assert.ok(!read("crates/easyoffice-app/ui/index.html").includes("검토 메모 스레드"));
  assert.ok(!read("crates/easyoffice-app/ui/ribbon-catalog.js").includes("검토 메모 스레드"));

  const commentDlg = document.getElementById("doc-comment-dialog");
  assert.ok(commentDlg, "#doc-comment-dialog exists");
  assert.equal(document.getElementById("doc-comment-dialog-title")?.textContent, "새 메모");
  assert.equal(commentDlg.querySelector('label[for="doc-comment-input"]')?.textContent, "내용");
  assert.equal(document.getElementById("btn-doc-comment-ok")?.textContent, "확인");
  assert.equal(document.getElementById("btn-doc-comment-cancel")?.textContent, "취소");

  const reviewPane = document.getElementById("doc-review-pane");
  assert.ok(reviewPane, "#doc-review-pane exists");
  assert.equal(reviewPane.hidden, true, "hidden when 0 comments, 0 revisions, and trackChanges off");
  assert.equal(document.getElementById("btn-doc-review-close")?.textContent, "닫기");

  const rust = new Map(rustItems().map((it) => [it.id, it]));
  assert.deepEqual(
    ["insert:endnote", "tool:review-comment", "tool:review-accept-all", "tool:review-reject-all"].map((id) => [
      rust.get(id)?.label,
      rust.get(id)?.action,
    ]),
    [
      ["미주", "studio-cmd:mod-endnote"],
      ["새 메모", "studio-cmd:review-comment"],
      ["모든 변경 내용 수락", "studio-cmd:review-accept-all"],
      ["모든 변경 내용 거부", "studio-cmd:review-reject-all"],
    ],
  );

  // Verify zero fake placeholder injections on 각주, 새 메모, 변경 내용 추적
  editor.replaceChildren();
  const p1 = build(document, editor, p("실제 본문 문단"));
  selection.addRange(rangeOf(p1.firstChild, 4, p1.firstChild, 4));

  userClick(fnBtn);
  await flushAsyncTurns(6);
  assert.equal(editor.querySelector("aside.eo-doc-footnote"), null, "zero fake <aside> footnote");
  assert.ok(!editor.textContent.includes("표준 행정 문서 관리 지침"));
  assert.ok(!editor.textContent.includes("표준 문서 관리 지침"));

  userClick(trackMenu);
  await flushAsyncTurns(6);
  assert.equal(p1.style.backgroundColor, "", "zero fake purple tint on paragraph");
  assert.equal(p1.dataset.highlighted, undefined);

  selection.addRange(rangeOf(p1.firstChild, 0, p1.firstChild, 2));
  userClick(commandItem(document, "document-review-comment"));
  await flushAsyncTurns(6);
  assert.equal(commentDlg.open, true, "opens #doc-comment-dialog instead of injecting fake [review-comment] paragraph");
  assert.ok(!editor.textContent.includes("[document-review-comment"));
  commentDlg.close();

  // Switch to SHEET and verify DOC-only items hide cleanly
  await switchModule(document, "SHEET");
  await flushAsyncTurns(6);
  assert.equal(enBtn.hidden, true);
  assert.equal(enMenu.hidden, true);
  assert.equal(acceptAllMenu.hidden, true);
  assert.equal(rejectAllMenu.hidden, true);
});

test("각주 and 미주 insertion, document-order renumbering, bidirectional jump, editing, orphan cleanup, and Undo/Redo", async () => {
  const selection = createSelection();
  const h = await createWorkspaceHarness({ selection, search: "?module=DOC&win=w-pd252-notes" });
  await flushAsyncTurns(10);
  const { document, sandbox, userClick } = h;
  const host = document.defaultView;
  const editor = document.getElementById("doc-editor");
  const api = host.__EASYOFFICE_DOC_NOTES_API__;
  assert.ok(api, "__EASYOFFICE_DOC_NOTES_API__ exposed");

  editor.replaceChildren();
  const p1 = build(document, editor, p("첫째 문단 본문"));
  const p2 = build(document, editor, p("둘째 문단 본문"));

  // Insert first footnote in p2
  selection.addRange(rangeOf(p2.firstChild, 5, p2.firstChild, 5));
  userClick(document.querySelector('#icon-toolbar .tb-btn[data-studio-cmd="mod-footnote"]'));
  await flushAsyncTurns(8);

  const fnSection = editor.querySelector('section.eo-doc-notes.eo-doc-footnote[data-eo-notes="footnote"]');
  assert.ok(fnSection, "footnote section created with .eo-doc-footnote and data-eo-notes=footnote");
  let fnRefs = editor.querySelectorAll('sup.eo-note-ref[data-eo-note-kind="footnote"]');
  assert.equal(fnRefs.length, 1);
  assert.equal(fnRefs[0].textContent, "1)");
  const firstNoteId = fnRefs[0].dataset.eoNoteId;
  const firstNoteBody = fnSection.querySelector(`.eo-doc-note-item[data-eo-note-id="${firstNoteId}"] .eo-doc-note-body`);
  assert.ok(firstNoteBody);
  firstNoteBody.textContent = "둘째 문단 각주 해설";
  editor.dispatchEvent({ type: "input", target: firstNoteBody });

  // Insert another footnote BEFORE it in p1 -> must renumber in document order 1), 2)!
  selection.addRange(rangeOf(p1.firstChild, 5, p1.firstChild, 5));
  api.insertNote("footnote", "첫째 문단 앞선 각주");
  await flushAsyncTurns(6);

  fnRefs = editor.querySelectorAll('sup.eo-note-ref[data-eo-note-kind="footnote"]');
  assert.equal(fnRefs.length, 2);
  assert.deepEqual([...fnRefs].map((el) => el.textContent), ["1)", "2)"]);
  const fnItems = fnSection.querySelectorAll(".eo-doc-note-item");
  assert.equal(fnItems.length, 2);
  assert.deepEqual([...fnItems].map((el) => el.querySelector(".eo-doc-note-num")?.textContent), ["1)", "2)"]);
  assert.deepEqual([...fnItems].map((el) => el.querySelector(".eo-doc-note-body")?.textContent), [
    "첫째 문단 앞선 각주",
    "둘째 문단 각주 해설",
  ]);

  // Insert endnote via toolbar button
  selection.addRange(rangeOf(p2.firstChild, 2, p2.firstChild, 2));
  userClick(document.querySelector('#icon-toolbar .tb-btn[data-studio-cmd="mod-endnote"]'));
  await flushAsyncTurns(6);
  const enSection = editor.querySelector('section.eo-doc-notes.eo-doc-endnote[data-eo-notes="endnote"]');
  assert.ok(enSection, "endnote section created");
  const enRef = editor.querySelector('sup.eo-note-ref[data-eo-note-kind="endnote"]');
  assert.equal(enRef?.textContent, "1)");
  const enBody = enSection.querySelector(".eo-doc-note-body");
  enBody.textContent = "문서 끝 미주 참고문헌";
  editor.dispatchEvent({ type: "input", target: enBody });

  // Bidirectional focus jump: clicking .eo-note-ref jumps selection into .eo-doc-note-body; clicking .eo-doc-note-num jumps back after .eo-note-ref
  userClick(fnRefs[0]);
  assert.ok(fnItems[0].querySelector(".eo-doc-note-body").contains(selection.getRangeAt(0).startContainer));
  userClick(fnItems[0].querySelector(".eo-doc-note-num"));
  const jumpedRange = selection.getRangeAt(0);
  assert.equal(jumpedRange.collapsed, true);
  assert.ok(jumpedRange.startContainer === fnRefs[0].parentElement);

  // Deleting first footnote marker cleans up orphan note item and renumbers remaining footnote back to 1)
  editor.dispatchEvent({ type: "beforeinput", target: p1 });
  fnRefs[0].remove();
  editor.dispatchEvent({ type: "input", target: p1 });
  await flushAsyncTurns(6);

  assert.equal(editor.querySelectorAll('sup.eo-note-ref[data-eo-note-kind="footnote"]').length, 1);
  assert.equal(editor.querySelector('sup.eo-note-ref[data-eo-note-kind="footnote"]').textContent, "1)");
  const remainingFnItems = fnSection.querySelectorAll(".eo-doc-note-item");
  assert.equal(remainingFnItems.length, 1);
  assert.equal(remainingFnItems[0].querySelector(".eo-doc-note-num")?.textContent, "1)");
  assert.equal(remainingFnItems[0].querySelector(".eo-doc-note-body")?.textContent, "둘째 문단 각주 해설");

  // Undo restores deleted footnote marker + note item and renumbers 1), 2); Redo removes it again
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(editor.querySelectorAll('sup.eo-note-ref[data-eo-note-kind="footnote"]').length, 2);
  assert.deepEqual(
    [...editor.querySelectorAll('section[data-eo-notes="footnote"] .eo-doc-note-body')].map((el) => el.textContent),
    ["첫째 문단 앞선 각주", "둘째 문단 각주 해설"],
  );

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(editor.querySelectorAll('sup.eo-note-ref[data-eo-note-kind="footnote"]').length, 1);
});

test("새 메모 dialog creation, review pane cards, 삭제 action, auto-hide, and single-step Undo/Redo", async () => {
  const selection = createSelection();
  const h = await createWorkspaceHarness({ selection, search: "?module=DOC&win=w-pd252-comments" });
  await flushAsyncTurns(10);
  const { document, sandbox, userClick } = h;
  const editor = document.getElementById("doc-editor");
  const reviewPane = document.getElementById("doc-review-pane");
  const commentDlg = document.getElementById("doc-comment-dialog");
  const commentInput = document.getElementById("doc-comment-input");

  editor.replaceChildren();
  const p1 = build(document, editor, p("계약 금액 및 납품 일정 검토 요망"));

  // Select "납품 일정" (offsets 8..13) and open 새 메모 dialog
  selection.addRange(rangeOf(p1.firstChild, 8, p1.firstChild, 13));
  userClick(studioItem(document, "review-comment"));
  await flushAsyncTurns(6);
  assert.equal(commentDlg.open, true);

  commentInput.value = "납품 기한을 11월 말로 조정 확인";
  userClick(document.getElementById("btn-doc-comment-ok"));
  await flushAsyncTurns(6);
  assert.equal(commentDlg.open, false);

  const span = editor.querySelector("span.eo-comment-range[data-eo-comment-id]");
  assert.ok(span, "inline comment range span wraps selected text");
  assert.equal(span.textContent, "납품 일정");
  const commentId = span.dataset.eoCommentId;

  const metaArticle = editor.querySelector(`section[data-eo-comments][hidden] article[data-eo-comment-id="${commentId}"]`);
  assert.ok(metaArticle, "metadata stored in hidden section inside #doc-editor");
  assert.equal(metaArticle.textContent, "납품 기한을 11월 말로 조정 확인");
  assert.ok(metaArticle.dataset.eoAuthor.length > 0);
  assert.ok(metaArticle.dataset.eoDate.length > 0);

  assert.equal(reviewPane.hidden, false, "#doc-review-pane shown when a comment exists");
  const cards = reviewPane.querySelectorAll("[data-review-comment-id]");
  assert.equal(cards.length, 1);
  assert.ok(cards[0].textContent.includes("납품 기한을 11월 말로 조정 확인"));
  const delBtn = cards[0].querySelector('button[data-comment-act="delete"]');
  assert.equal(delBtn?.textContent, "삭제");

  // Single-step Undo removes both inline span and metadata + hides review pane; Redo restores both!
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(editor.querySelector("span.eo-comment-range"), null);
  assert.equal(editor.querySelector("section[data-eo-comments] article"), null);
  assert.equal(reviewPane.hidden, true);
  assert.equal(editor.querySelector("p")?.textContent, "계약 금액 및 납품 일정 검토 요망");

  sandbox.dispatchWindowKeydown({ key: "y", ctrlKey: true });
  await flushAsyncTurns(8);
  assert.equal(editor.querySelector("span.eo-comment-range")?.textContent, "납품 일정");
  assert.equal(reviewPane.hidden, false);

  // Clicking 삭제 removes comment while keeping paragraph text intact, and hides pane when 0 comments remain
  const restoredDelBtn = reviewPane.querySelector('button[data-comment-act="delete"]');
  userClick(restoredDelBtn);
  await flushAsyncTurns(6);
  assert.equal(editor.querySelector("span.eo-comment-range"), null);
  assert.equal(editor.querySelector("section[data-eo-comments] article"), null);
  assert.equal(editor.querySelector("p")?.textContent, "계약 금액 및 납품 일정 검토 요망");
  assert.equal(reviewPane.hidden, true);
});

test("변경 내용 추적 live beforeinput insertText & deleteContentBackward, coalescing, formatting safety, 수락/거부, and Undo/Redo", async () => {
  const selection = createSelection();
  const h = await createWorkspaceHarness({ selection, search: "?module=DOC&win=w-pd252-track" });
  await flushAsyncTurns(10);
  const { document, sandbox, userClick } = h;
  const host = document.defaultView;
  const editor = document.getElementById("doc-editor");
  const reviewPane = document.getElementById("doc-review-pane");
  const trackMenu = studioItem(document, "review-track");

  editor.replaceChildren();
  const p1 = build(document, editor, p("원본 문장입니다"));

  // Toggle Track Changes ON via menu
  userClick(trackMenu);
  await flushAsyncTurns(6);
  assert.equal(editor.dataset.eoTrackChanges, "true");
  assert.equal(trackMenu.getAttribute("aria-checked"), "true");
  assert.equal(reviewPane.hidden, false, "review pane visible while trackChanges is ON");

  // Live beforeinput insertText at offset 3 ("원본 " -> insert "신규 ") coalesces consecutive typing!
  selection.addRange(rangeOf(p1.firstChild, 3, p1.firstChild, 3));
  const ev1 = editor.dispatchEvent({ type: "beforeinput", target: p1, inputType: "insertText", data: "신" });
  const ev2 = editor.dispatchEvent({ type: "beforeinput", target: p1, inputType: "insertText", data: "규" });
  const ev3 = editor.dispatchEvent({ type: "beforeinput", target: p1, inputType: "insertText", data: "!" });
  assert.equal(ev1, false, "insertText defaultPrevented");
  assert.equal(ev2, false);
  assert.equal(ev3, false);

  const insEls = editor.querySelectorAll("ins.eo-rev-ins[data-eo-rev-id]");
  assert.equal(insEls.length, 1, "consecutive typing coalesced into one <ins>");
  assert.equal(insEls[0].textContent, "신규!");

  // Backspace inside uncommitted <ins> deletes "!" directly from <ins> instead of creating <del>!
  const evBsIns = editor.dispatchEvent({ type: "beforeinput", target: p1, inputType: "deleteContentBackward" });
  assert.equal(evBsIns, false);
  assert.equal(editor.querySelector("ins.eo-rev-ins")?.textContent, "신규");
  assert.equal(editor.querySelectorAll("del.eo-rev-del").length, 0, "no <del> created when deleting inside <ins>");

  // Backspace on committed text ("원본" at offset 2 -> delete "본") wraps in <del class="eo-rev-del">!
  const firstText = p1.childNodes[0];
  selection.addRange(rangeOf(firstText, 2, firstText, 2));
  const evBsCommitted = editor.dispatchEvent({ type: "beforeinput", target: p1, inputType: "deleteContentBackward" });
  assert.equal(evBsCommitted, false);
  const delEl = editor.querySelector("del.eo-rev-del[data-eo-rev-id]");
  assert.ok(delEl, "committed character wrapped in <del>");
  assert.equal(delEl.textContent, "본");

  // Applying character formatting (doc-runs.js) across the paragraph preserves <ins> and <del>
  const lastNode = p1.lastChild;
  const endOff = lastNode.nodeType === 3 ? lastNode.data.length : lastNode.childNodes.length;
  selection.addRange(rangeOf(p1.firstChild, 0, lastNode, endOff));
  host.__EASYOFFICE_DOC_RUNS__.applyFormat(editor, selection.getRangeAt(0), { bold: true });
  assert.ok(editor.querySelector("ins.eo-rev-ins[data-eo-rev-id]"), "<ins> preserved after applyFormat");
  assert.ok(editor.querySelector("del.eo-rev-del[data-eo-rev-id]"), "<del> preserved after applyFormat");

  // Review pane shows both revision cards with 수락 and 거부 buttons
  const revCards = reviewPane.querySelectorAll("[data-review-rev-id]");
  assert.equal(revCards.length, 2);
  for (const card of revCards) {
    assert.equal(card.querySelector('button[data-rev-act="accept"]')?.textContent, "수락");
    assert.equal(card.querySelector('button[data-rev-act="reject"]')?.textContent, "거부");
  }

  // Individual 수락 on <ins> keeps "신규" and removes <ins>; individual 거부 on <del> restores "본" and removes <del>!
  const insId = editor.querySelector("ins.eo-rev-ins").dataset.eoRevId;
  const delId = editor.querySelector("del.eo-rev-del").dataset.eoRevId;
  userClick(reviewPane.querySelector(`[data-review-rev-id="${insId}"] button[data-rev-act="accept"]`));
  await flushAsyncTurns(6);
  assert.equal(editor.querySelector("ins.eo-rev-ins"), null);
  assert.ok(editor.querySelector("p").textContent.includes("신규"));

  userClick(reviewPane.querySelector(`[data-review-rev-id="${delId}"] button[data-rev-act="reject"]`));
  await flushAsyncTurns(6);
  assert.equal(editor.querySelector("del.eo-rev-del"), null);
  assert.equal(editor.querySelector("p").textContent, "원본 신규문장입니다");

  // Undo both steps, then test bulk 모든 변경 내용 수락 and 모든 변경 내용 거부!
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(6);
  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(6);
  assert.ok(editor.querySelector("ins.eo-rev-ins"));
  assert.ok(editor.querySelector("del.eo-rev-del"));

  userClick(studioItem(document, "review-accept-all"));
  await flushAsyncTurns(6);
  assert.equal(editor.querySelectorAll("ins.eo-rev-ins, del.eo-rev-del").length, 0);
  assert.equal(editor.querySelector("p").textContent, "원 신규문장입니다");

  sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
  await flushAsyncTurns(6);
  assert.equal(editor.querySelectorAll("ins.eo-rev-ins, del.eo-rev-del").length, 2);

  userClick(studioItem(document, "review-reject-all"));
  await flushAsyncTurns(6);
  assert.equal(editor.querySelectorAll("ins.eo-rev-ins, del.eo-rev-del").length, 0);
  assert.equal(editor.querySelector("p").textContent, "원본 문장입니다");
});

test("savePayload (blocks + collectDocAnnotations) and openDocumentFromBytes roundtrip footnotes, endnotes, comments, trackChanges, ins, and del", async () => {
  const converts = [];
  let decodedPayload = null;
  const selection = createSelection();
  const h = await createWorkspaceHarness({
    selection,
    search: "?module=DOC&win=w-pd252-roundtrip",
    hostEnvExtras: { confirm: () => true },
    testIpc: async (command, payload) => {
      if (command === "get_workspace_bootstrap") {
        return { platform: "macOS", shortcutModifier: "⌘", recentFormatFilters: ["전체"], openTabs: [] };
      }
      if (command === "convert_document_format") {
        const req = JSON.parse(payload);
        if (req.action === "decode") return decodedPayload;
        converts.push(req);
        return {
          status: "CONVERTED",
          sourceFormat: req.sourceFormat,
          targetFormat: req.targetFormat,
          sourceBytes: 128,
          outputBytes: 256,
          specification: "ECMA-376",
          convertedBase64: "UEs=",
        };
      }
      return { status: "ok" };
    },
  });
  await flushAsyncTurns(12);
  const { document, userClick } = h;
  const host = document.defaultView;
  const editor = document.getElementById("doc-editor");
  const api = host.__EASYOFFICE_DOC_NOTES_API__;
  const savePayload = host.__EASYOFFICE_SAVE_PAYLOAD__;

  editor.replaceChildren();
  const p1 = build(document, editor, p("제1조 목적 조항 원문"));
  const p2 = build(document, editor, p("제2조 시행일 조항"));

  // 1. Footnote on p1 after "목적" (offset 6)
  selection.addRange(rangeOf(p1.firstChild, 6, p1.firstChild, 6));
  api.insertNote("footnote", "법령 근거 각주");

  // 2. Endnote on p2 at end
  selection.addRange(rangeOf(p2.firstChild, p2.firstChild.data.length, p2.firstChild, p2.firstChild.data.length));
  api.insertNote("endnote", "부칙 미주 설명");

  // 3. Comment on "시행일" in p2
  const p2Text = p2.firstChild;
  selection.addRange(rangeOf(p2Text, 4, p2Text, 7));
  api.addComment("시행일 확정 필요", "김검토", "2026-11-19T09:00:00Z");

  // 4. Turn on trackChanges, insert <ins> and <del> in p1
  api.toggleTrackChanges(true);
  const tailText = p1.lastChild;
  selection.addRange(rangeOf(tailText, tailText.data.length - 2, tailText, tailText.data.length));
  api.recordDeletion(selection.getRangeAt(0), "박수정", "2026-11-19T10:00:00Z");
  const afterDel = p1.querySelector("del.eo-rev-del");
  const idxAfterDel = Array.prototype.indexOf.call(p1.childNodes, afterDel) + 1;
  selection.addRange(rangeOf(p1, idxAfterDel, p1, idxAfterDel));
  api.recordInsertion("개정안", "박수정", "2026-11-19T10:05:00Z");

  // Verify direct savePayload contract AND full studio-save conversion payload!
  const blocks = savePayload.collectDocBlocks(editor);
  const annotations = savePayload.collectDocAnnotations(editor);
  assert.equal(blocks.length, 2, "note and comment sections excluded from body blocks");
  assert.equal(annotations.trackChanges, true);
  assert.deepEqual(annotations.notes, [
    { id: 1, kind: "footnote", text: "법령 근거 각주" },
    { id: 1, kind: "endnote", text: "부칙 미주 설명" },
  ]);
  assert.deepEqual(annotations.comments, [
    { id: 1, author: "김검토", date: "2026-11-19T09:00:00Z", text: "시행일 확정 필요" },
  ]);

  const p1Runs = blocks[0].runs;
  assert.ok(p1Runs.some((r) => r.text === "" && r.noteRef?.kind === "footnote" && r.noteRef?.id === 1));
  assert.ok(
    p1Runs.some(
      (r) =>
        r.text === "원문" &&
        r.revision?.kind === "delete" &&
        r.revision?.author === "박수정" &&
        r.revision?.date === "2026-11-19T10:00:00Z",
    ),
  );
  assert.ok(
    p1Runs.some(
      (r) =>
        r.text === "개정안" &&
        r.revision?.kind === "insert" &&
        r.revision?.author === "박수정" &&
        r.revision?.date === "2026-11-19T10:05:00Z",
    ),
  );

  const p2Runs = blocks[1].runs;
  assert.ok(p2Runs.some((r) => r.text === "시행일" && r.commentId === 1));
  assert.ok(p2Runs.some((r) => r.text === "" && r.noteRef?.kind === "endnote" && r.noteRef?.id === 1));

  userClick(studioItem(document, "studio-save"));
  await flushAsyncTurns(10);
  assert.equal(converts.length, 1);
  assert.equal(converts[0].trackChanges, true);
  assert.deepEqual(converts[0].notes, annotations.notes);
  assert.deepEqual(converts[0].comments, annotations.comments);

  // Roundtrip via openDocumentFromBytes into a fresh state!
  editor.replaceChildren(document.createElement("p"));
  delete editor.dataset.eoTrackChanges;
  decodedPayload = {
    status: "OK",
    module: "DOC",
    format: "docx",
    kind: "document",
    itemCount: converts[0].blocks.length,
    blocks: structuredClone(converts[0].blocks),
    trackChanges: converts[0].trackChanges,
    notes: structuredClone(converts[0].notes),
    comments: structuredClone(converts[0].comments),
  };

  const openRes = await host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes(
    "검토왕복.docx",
    new Uint8Array([0x50, 0x4b, 0x03, 0x04]),
  );
  await flushAsyncTurns(16);
  assert.equal(openRes.ok, true);

  assert.equal(editor.dataset.eoTrackChanges, "true");
  assert.equal(studioItem(document, "review-track")?.getAttribute("aria-checked"), "true");
  assert.equal(document.getElementById("doc-review-pane").hidden, false);

  assert.equal(editor.querySelector('sup.eo-note-ref[data-eo-note-kind="footnote"]')?.textContent, "1)");
  assert.equal(editor.querySelector('sup.eo-note-ref[data-eo-note-kind="endnote"]')?.textContent, "1)");
  assert.equal(editor.querySelector('section[data-eo-notes="footnote"] .eo-doc-note-body')?.textContent, "법령 근거 각주");
  assert.equal(editor.querySelector('section[data-eo-notes="endnote"] .eo-doc-note-body')?.textContent, "부칙 미주 설명");
  assert.equal(editor.querySelector("span.eo-comment-range")?.textContent, "시행일");
  assert.equal(editor.querySelector("del.eo-rev-del")?.textContent, "원문");
  assert.equal(editor.querySelector("ins.eo-rev-ins")?.textContent, "개정안");
  assert.equal(document.querySelectorAll("#doc-review-pane [data-review-comment-id]").length, 1);
  assert.equal(document.querySelectorAll("#doc-review-pane [data-review-rev-id]").length, 2);

  // Re-collecting save payload after reopen must match 1:1!
  assert.deepEqual(savePayload.collectDocAnnotations(editor), annotations);
  assert.deepEqual(savePayload.collectDocBlocks(editor), blocks);
});

test("deleteContentForward, point comment roundtrip, review pane 닫기, and malformed decoded annotation bounds", async () => {
  let decodedPayload = null;
  const selection = createSelection();
  const h = await createWorkspaceHarness({
    selection,
    search: "?module=DOC&win=w-pd252-edges",
    hostEnvExtras: { confirm: () => true },
    testIpc: async (command, payload) => {
      if (command === "get_workspace_bootstrap") {
        return { platform: "macOS", shortcutModifier: "⌘", recentFormatFilters: ["전체"], openTabs: [] };
      }
      if (command === "convert_document_format") {
        const req = JSON.parse(payload);
        if (req.action === "decode") return decodedPayload;
      }
      return { status: "ok" };
    },
  });
  await flushAsyncTurns(10);
  const { document, userClick } = h;
  const host = document.defaultView;
  const editor = document.getElementById("doc-editor");
  const api = host.__EASYOFFICE_DOC_NOTES_API__;
  const savePayload = host.__EASYOFFICE_SAVE_PAYLOAD__;
  const reviewPane = document.getElementById("doc-review-pane");

  editor.replaceChildren();
  const emptyP = document.createElement("p");
  emptyP.appendChild(document.createElement("br"));
  editor.appendChild(emptyP);
  selection.addRange(rangeOf(emptyP, 0, emptyP, 0));
  api.addComment("빈 문단 지점 메모", "검토자", "2026-11-19T11:00:00Z");
  assert.ok(editor.querySelector("span.eo-comment-range.eo-comment-point"), "point comment created on empty paragraph");
  assert.equal(reviewPane.hidden, false);

  // 닫기 button hides review pane until next annotation action
  userClick(document.getElementById("btn-doc-review-close"));
  assert.equal(reviewPane.hidden, true);

  // deleteContentForward under trackChanges marks forward character as <del>
  const p2 = build(document, editor, p("가나다"));
  api.toggleTrackChanges(true);
  assert.equal(reviewPane.hidden, false, "toggling trackChanges re-opens review pane");
  selection.addRange(rangeOf(p2.firstChild, 1, p2.firstChild, 1));
  const fwdEv = editor.dispatchEvent({ type: "beforeinput", target: p2, inputType: "deleteContentForward" });
  assert.equal(fwdEv, false);
  assert.equal(editor.querySelector("del.eo-rev-del")?.textContent, "나");

  const savedBlocks = savePayload.collectDocBlocks(editor);
  const savedAnn = savePayload.collectDocAnnotations(editor);
  assert.deepEqual(savedBlocks[0].runs, [{ text: "", commentId: 1 }]);

  // Open payload with malformed entries (negative IDs, unknown note kind, oversized arrays) drops invalid entries cleanly
  decodedPayload = {
    status: "OK",
    module: "DOC",
    format: "docx",
    kind: "document",
    itemCount: 2,
    blocks: [
      ...savedBlocks,
      {
        type: "paragraph",
        runs: [
          { text: "정상", revision: { id: -5, kind: "insert" }, commentId: 0 },
          { text: "", noteRef: { kind: "bogus", id: 1 } },
        ],
      },
    ],
    trackChanges: false,
    notes: [{ id: -1, kind: "footnote", text: "무시" }, { id: 1, kind: "bogus", text: "무시" }],
    comments: [...savedAnn.comments, { id: 0, author: "무시", date: "", text: "무시" }],
  };
  const openRes = await host.__EASYOFFICE_WORKSPACE_API__.openDocumentFromBytes(
    "경계검증.docx",
    new Uint8Array([0x50, 0x4b, 0x03, 0x04]),
  );
  await flushAsyncTurns(12);
  assert.equal(openRes.ok, true);
  assert.equal(editor.dataset.eoTrackChanges, undefined);
  assert.ok(editor.querySelector('span.eo-comment-point[data-eo-comment-id="1"]'), "point comment roundtripped");
  assert.equal(editor.querySelectorAll("sup.eo-note-ref").length, 0, "invalid noteRef rejected");
  assert.equal(editor.querySelectorAll("[data-eo-rev-id='-5']").length, 0, "negative rev id rejected");
});
