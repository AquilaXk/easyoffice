import test from "node:test";
import assert from "node:assert/strict";
import { createWorkspaceHarness, flushAsyncTurns } from "./webui-dom-harness.mjs";
import { makeFixtureFile, pickFile } from "./test-support/fixture-files.mjs";
import { createRunHarness, rangeOf, p, span, textNodeOf } from "./test-support/doc-run-harness.mjs";

// Document hyperlinks (doc-link.js): the address policy, add / update / remove on the editor DOM, the
// dialog, and the save payload and the reopened file that carry the link.

const URL = "https://example.com/a?b=1";
const links = (h) => h.host.__EASYOFFICE_DOC_LINK__;
const byId = (h, id) => h.document.getElementById(id);
const linkElements = (h) => h.editor.querySelectorAll("a[data-eo-link]");
const text = (el) => el.textContent;

test("the address policy takes web and mail targets only and refuses control characters and long input", async () => {
  const h = await createRunHarness([p("x")]);
  const { normalizeUrl, MAX_URL_CHARS } = links(h);
  assert.equal(normalizeUrl("  https://example.com/a?b=1  "), URL);
  assert.equal(normalizeUrl("http://a.b"), "http://a.b");
  assert.equal(normalizeUrl("mailto:me@example.com"), "mailto:me@example.com");
  for (const bad of [
    "", "javascript:alert(1)", " JaVaScRiPt:alert(1)", "file:///etc/passwd", "data:text/html,x", "vbscript:x", "ftp://a.b", "tel:1",
    "https://", "http:///x", "https://a b", "https://a.b/\u0007", "https://a.b/\u0085", "mailto:", "example.com", "//example.com",
    "https://a.b/" + "x".repeat(MAX_URL_CHARS),
  ]) {
    assert.equal(normalizeUrl(bad), null, JSON.stringify(bad));
  }
});

test("adding a link to part of a paragraph wraps only those characters and keeps the run formatting inside", async () => {
  const h = await createRunHarness([p("앞 ", { tag: "span", children: ["굵은 글"] }, " 뒤")]);
  const [first] = h.blocks;
  h.editor.querySelectorAll("span")[0].classList.add("eo-run");
  h.editor.querySelectorAll("span")[0].dataset.eoBold = "true";
  const run = first.childNodes[1];
  const range = rangeOf(textNodeOf(first), 1, run.childNodes[0], 2);
  const res = links(h).addLink(h.editor, range, URL);
  assert.equal(res.changed, true);
  const [a] = linkElements(h);
  assert.equal(a.getAttribute("href"), URL);
  assert.equal(text(a), " 굵은");
  assert.equal(text(first), "앞 굵은 글 뒤", "the text is unchanged");
  assert.equal(a.querySelector(".eo-run").dataset.eoBold, "true", "the bold run stays inside the link");
  assert.equal(h.editor.querySelectorAll(".eo-run").length, 2, "the unselected rest of the run keeps its own run");
});

test("a link refuses an unsafe address, a selection over two paragraphs and an overlap with another link", async () => {
  const h = await createRunHarness([p("첫째 문단"), p("둘째 문단")]);
  const [one, two] = h.blocks;
  const inFirst = rangeOf(textNodeOf(one), 0, textNodeOf(one), 2);
  assert.deepEqual(links(h).addLink(h.editor, inFirst, "javascript:alert(1)"), { changed: false, error: "허용되지 않는 주소" });
  assert.equal(linkElements(h).length, 0);
  const across = rangeOf(textNodeOf(one), 1, textNodeOf(two), 2);
  assert.equal(links(h).addLink(h.editor, across, URL).error, "한 문단 안에서 선택");
  assert.equal(links(h).addLink(h.editor, inFirst, URL).changed, true);
  const overlap = rangeOf(one.childNodes[0].childNodes[0], 1, one.childNodes[1], 2);
  assert.equal(links(h).addLink(h.editor, overlap, URL).error, "다른 링크와 겹침");
});

test("a caret inserts the label (or the address) as a new link; update changes address and label; remove keeps the text", async () => {
  const h = await createRunHarness([p("앞뒤")]);
  const [first] = h.blocks;
  const caret = rangeOf(textNodeOf(first), 1, textNodeOf(first), 1);
  const added = links(h).addLink(h.editor, caret, URL, "여기");
  assert.equal(text(first), "앞여기뒤");
  assert.equal(text(added.link), "여기");
  const updated = links(h).updateLink(h.editor, added.link, "mailto:a@b.co", "메일");
  assert.equal(updated.changed, true);
  assert.equal(added.link.getAttribute("href"), "mailto:a@b.co");
  assert.equal(text(first), "앞메일뒤");
  assert.equal(links(h).updateLink(h.editor, added.link, "javascript:x", "").error, "허용되지 않는 주소");
  assert.equal(added.link.getAttribute("href"), "mailto:a@b.co", "a refused update changes nothing");
  assert.equal(links(h).removeLink(h.editor, added.link).changed, true);
  assert.equal(linkElements(h).length, 0);
  assert.equal(text(first), "앞메일뒤");
  const bare = links(h).addLink(h.editor, rangeOf(textNodeOf(first), 0, textNodeOf(first), 0), URL);
  assert.equal(text(bare.link), URL, "no label: the address is shown");
});

test("the dialog adds, edits and removes a link, with the policy message for a bad address", async () => {
  const h = await createRunHarness([p("링크 만들기 문장")]);
  const [first] = h.blocks;
  h.selection.addRange(rangeOf(textNodeOf(first), 0, textNodeOf(first), 2));
  h.userClick(h.document.querySelector('[data-studio-cmd="insert-link"]'));
  await flushAsyncTurns(30);
  const dialog = byId(h, "doc-link-dialog");
  assert.equal(dialog.open, true);
  assert.equal(byId(h, "doc-link-address").value, "");
  assert.equal(byId(h, "doc-link-label").value, "링크");
  assert.equal(byId(h, "btn-doc-link-remove").hidden, true);

  byId(h, "doc-link-address").value = "javascript:alert(1)";
  byId(h, "btn-doc-link-apply").click();
  assert.equal(byId(h, "doc-link-status").textContent, "허용되지 않는 주소");
  assert.equal(dialog.open, true);
  assert.equal(linkElements(h).length, 0);

  byId(h, "doc-link-address").value = URL;
  byId(h, "btn-doc-link-apply").click();
  assert.equal(dialog.open, false);
  assert.equal(linkElements(h).length, 1);
  assert.equal(text(first), "링크 만들기 문장");

  // Reopen from inside the link: the fields show it and the remove button is there.
  const [a] = linkElements(h);
  h.selection.addRange(rangeOf(a.childNodes[0], 1, a.childNodes[0], 1));
  h.userClick(h.document.querySelector('[data-studio-cmd="insert-link"]'));
  await flushAsyncTurns(30);
  assert.equal(byId(h, "doc-link-address").value, URL);
  assert.equal(byId(h, "btn-doc-link-remove").hidden, false);
  byId(h, "doc-link-address").value = "https://example.org/";
  byId(h, "doc-link-label").value = "새 이름";
  byId(h, "btn-doc-link-apply").click();
  assert.equal(a.getAttribute("href"), "https://example.org/");
  assert.equal(text(first), "새 이름 만들기 문장");

  h.selection.addRange(rangeOf(a.childNodes[0], 0, a.childNodes[0], 0));
  h.userClick(h.document.querySelector('[data-studio-cmd="insert-link"]'));
  await flushAsyncTurns(30);
  byId(h, "btn-doc-link-remove").click();
  assert.equal(linkElements(h).length, 0);
  assert.equal(text(first), "새 이름 만들기 문장");
});

test("the save payload carries the link of a run and drops an unsafe one", async () => {
  const h = await createRunHarness([p("앞 ")]);
  const [first] = h.blocks;
  const a = h.document.createElement("a");
  a.setAttribute("href", URL);
  a.dataset.eoLink = "1";
  a.appendChild(h.document.createTextNode("링크"));
  first.appendChild(a);
  const bad = h.document.createElement("a");
  bad.setAttribute("href", "javascript:alert(1)");
  bad.dataset.eoLink = "1";
  bad.appendChild(h.document.createTextNode("위험"));
  first.appendChild(bad);
  const payload = h.host.__EASYOFFICE_SAVE_PAYLOAD__;
  const blocks = payload.collectDocBlocks(h.editor);
  const runs = blocks[0].runs;
  assert.deepEqual(runs.map((r) => [r.text, r.link]), [["앞 ", undefined], ["링크", URL], ["위험", undefined]]);
});

test("a file opened with link runs shows them as links, an unsafe address as plain text, and saving keeps them", async () => {
  const h = await createWorkspaceHarness();
  await flushAsyncTurns(10);
  const model = { blocks: [{ type: "paragraph", runs: [{ text: "앞 " }, { text: "링크", link: URL, bold: true }, { text: "위험", link: "javascript:alert(1)" }] }] };
  await pickFile(h.document, makeFixtureFile("링크.docx", model));
  const editor = byId(h, "doc-editor");
  const anchors = editor.querySelectorAll("a[data-eo-link]");
  assert.equal(anchors.length, 1);
  assert.equal(anchors[0].getAttribute("href"), URL);
  assert.equal(anchors[0].textContent, "링크");
  assert.match(editor.textContent, /위험/);
  const blocks = h.host.__EASYOFFICE_SAVE_PAYLOAD__.collectDocBlocks(editor);
  const linked = blocks[0].runs.filter((r) => r.link);
  assert.deepEqual(linked.map((r) => [r.text, r.link, r.bold]), [["링크", URL, true]]);
});
