import test from "node:test";
import assert from "node:assert/strict";
import { allElements, clipboard, openPasteEditor, parserCalls, shapes } from "./test-support/paste-harness.mjs";

// Pasted HTML is untrusted input. Nothing of it reaches the page except text and the values the editor itself validates;
// what goes over a limit is refused as a whole and the plain text is pasted instead (doc-paste-model.js).

const ALLOWED_TAGS = new Set(["P", "H1", "H2", "H3", "H4", "H5", "H6", "SPAN", "A", "BR", "UL", "OL", "LI", "TABLE", "TBODY", "TR", "TD", "IMG", "SECTION", "FOOTER"]);
const ALLOWED_ATTRIBUTES = new Set(["class", "style", "href", "src", "alt", "title", "contenteditable", "colspan", "rowspan", "data-eo-hf"]);
const PNG_1x1 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg==";

// Nothing but the editor's own element kinds and attributes: no event attribute, no script, no outside address.
function assertCleanTree(editor) {
  for (const el of allElements(editor)) {
    assert.ok(ALLOWED_TAGS.has(el.tagName), "tag " + el.tagName);
    for (const name of el._attrs.keys()) {
      assert.ok(ALLOWED_ATTRIBUTES.has(name) || name.startsWith("data-eo-"), `attribute ${name} on ${el.tagName}`);
      assert.ok(!name.startsWith("on"), "event attribute " + name);
    }
    const href = el.getAttribute("href");
    if (href !== null) assert.match(href, /^(?:https?:\/\/|mailto:)/i);
    const src = el.getAttribute("src");
    if (src !== null) assert.ok(!/^(?:https?|file|ftp|javascript):/i.test(src), "src " + src);
    const style = el.getAttribute("style") ?? "";
    assert.ok(!/url\(|expression|javascript|@import/i.test(style), "style " + style);
  }
}

async function editorWithCaret(options) {
  const t = await openPasteEditor({ paragraphs: ["앞뒤"], ...options });
  t.caretAt(t.editor.childNodes[0].firstChild, 1);
  return t;
}

test("scripts, handlers, frames, objects and forms leave nothing in the editor", async () => {
  const t = await editorWithCaret();
  const html = [
    "<p onclick=\"alert(1)\" onmouseover=alert(2)>본문<script>alert('s')</script></p>",
    "<img src=x onerror=alert(3)><img src=\"javascript:alert(4)\" onload=alert(5)>",
    "<svg onload=alert(6)><script>alert(7)</script><a href=javascript:alert(8)>svg</a></svg>",
    "<iframe src=\"javascript:alert(9)\"></iframe><object data=\"x\"></object><embed src=\"y\"><applet code=z></applet>",
    "<form action=\"https://evil.example\"><input autofocus onfocus=alert(10) value=v><button onclick=alert(11)>b</button><textarea>t</textarea><select><option>o</option></select></form>",
    "<a href=\"javascript:alert(12)\" onclick=alert(13) target=_blank>링크</a><a href=\"JaVaScRiPt:alert(14)\">링크2</a><a href=\"data:text/html,<script>alert(15)</script>\">링크3</a>",
    "<math><mi xlink:href=javascript:alert(16)>x</mi></math><link rel=stylesheet href=//evil.example/a.css><base href=//evil.example/><meta http-equiv=refresh content=0>",
    "<div style=\"background:url(javascript:alert(17));width:expression(alert(18));color:#00f\">스타일</div>",
    "<template><p>템플릿</p></template><noscript><p>노스크립트</p></noscript><video src=v onerror=alert(19)></video><audio src=a></audio>",
    "<style>@import url(//evil.example/x.css);p{background:url(javascript:alert(20))}</style>",
  ].join("");
  await t.paste(clipboard({ html, text: "x" }));
  assertCleanTree(t.editor);
  const text = t.editor.textContent;
  for (const bad of ["alert", "evil.example", "템플릿", "노스크립트", "javascript"]) assert.ok(!text.includes(bad), bad + " in " + text);
  assert.ok(text.includes("본문") && text.includes("링크") && text.includes("스타일"), "the text content is still pasted");
  assert.equal(t.editor.querySelectorAll("a").length, 0, "no address of these passed the policy");
  assert.equal(t.editor.querySelectorAll("img").length, 0);
});

test("the HTML goes through the browser's inert parser, after the clean-up", async () => {
  const t = await editorWithCaret();
  parserCalls.length = 0;
  await t.paste(clipboard({ html: "<p>a</p><script>alert(1)</script><!--c--><style>x</style>" }));
  assert.equal(parserCalls.length, 1);
  assert.equal(parserCalls[0].type, "text/html");
  assert.ok(!/<script|<!--|<style/i.test(parserCalls[0].html), "the parser is given the cleaned text: " + parserCalls[0].html);
});

test("a style value is read through the format validators: only the known properties come out", async () => {
  const t = await editorWithCaret();
  await t.paste(clipboard({ html: '<p><span style="color:#12ab34;background:url(javascript:alert(1));font-family:evil;font-size:99999pt;position:fixed;font-weight:bold">글</span></p>' }));
  assertCleanTree(t.editor);
  const run = t.editor.querySelector(".eo-run");
  assert.equal(run.dataset.eoColor, "12AB34");
  assert.equal(run.dataset.eoBold, "true");
  assert.equal(run.dataset.eoFont, "evil");
  assert.ok(Number(run.dataset.eoSize) <= 8192, "the size is held to the editor's bound");
  assert.ok(!run.getAttribute("style").includes("position"));
});

test("a span count or size that is absurd is held to the span bound, not followed", async () => {
  const t = await editorWithCaret();
  await t.paste(clipboard({ html: '<table><tr><td colspan="999999999" rowspan="-5">a</td></tr></table>' }));
  const [a] = t.editor.querySelectorAll("td");
  assert.equal(a.getAttribute("colspan"), "100");
  assert.equal(a.getAttribute("rowspan"), null);
  await t.paste(clipboard({ html: '<table><tr><td colspan="x" rowspan="1e9">b</td></tr></table>' }));
  const b = t.editor.querySelectorAll("td")[1];
  assert.equal(b.getAttribute("colspan"), null);
  assert.equal(b.getAttribute("rowspan"), "100");
});

// ---- the limits ----------------------------------------------------------------------------------------------

async function limited(overrides, html, text = "글자만", options = {}) {
  const t = await editorWithCaret(options);
  const limits = t.host.__EASYOFFICE_DOC_PASTE_MODEL__.LIMITS;
  const saved = { ...limits };
  Object.assign(limits, overrides);
  try {
    await t.paste(clipboard({ html, text }));
  } finally {
    Object.assign(limits, saved);
  }
  return t;
}

const refused = (t) => {
  assert.equal(t.feedback(), "붙여넣을 내용이 너무 커서 글자만 붙였습니다");
  assert.equal(t.editor.textContent.includes("글자만"), true, "the plain text was pasted instead");
  assert.equal(t.editor.querySelectorAll("table, img, ul, ol, h1, h2, a, span").length, 0, "none of the HTML was kept");
};

test("the default limits are the documented ones", async () => {
  const t = await editorWithCaret();
  const { LIMITS: limits } = t.host.__EASYOFFICE_DOC_PASTE_MODEL__;
  assert.deepEqual(
    [limits.MAX_HTML_CHARS, limits.MAX_NODES, limits.MAX_DEPTH, limits.MAX_IMAGES, limits.MAX_SPAN, limits.MAX_TABLE_CELLS],
    [4 * 1024 * 1024, 300000, 100, 32, 100, 20000]
  );
});

test("HTML over the size bound is refused as a whole", async () => {
  refused(await limited({ MAX_HTML_CHARS: 200 }, "<h1>제목</h1><p>" + "가".repeat(300) + "</p>"));
});

test("HTML over the node bound is refused as a whole", async () => {
  refused(await limited({ MAX_NODES: 20 }, "<h1>제목</h1>" + "<b>x</b>".repeat(30)));
});

test("nesting over the depth bound is refused, also when it is thousands deep", async () => {
  refused(await limited({}, "<h1>제목</h1>" + "<div>".repeat(150) + "x" + "</div>".repeat(150)));
  refused(await limited({}, "<h1>제목</h1>" + "<span>".repeat(20000) + "x"));
});

test("HTML over the block, run or text bound is refused as a whole", async () => {
  refused(await limited({ MAX_BLOCKS: 5 }, "<h1>제목</h1>" + "<p>a</p>".repeat(10)));
  refused(await limited({ MAX_RUNS: 5 }, "<h1>제목</h1><p>" + "<b>a</b><i>b</i>".repeat(10) + "</p>"));
  refused(await limited({ MAX_TEXT_CHARS: 50 }, "<h1>제목</h1><p>" + "가".repeat(80) + "</p>", "글자만"));
});

test("a table over the row, column or cell bound is refused as a whole", async () => {
  const table = (rows, cols) => "<h1>제목</h1><table>" + `<tr>${"<td>a</td>".repeat(cols)}</tr>`.repeat(rows) + "</table>";
  refused(await limited({ MAX_TABLE_ROWS: 3 }, table(4, 2)));
  refused(await limited({ MAX_TABLE_COLS: 3 }, table(2, 4)));
  refused(await limited({ MAX_TABLE_CELLS: 5 }, table(3, 2)));
});

test("more pictures than the bound, or a picture over the size bound, refuses the paste", async () => {
  const img = `<img src="data:image/png;base64,${PNG_1x1}">`;
  refused(await limited({ MAX_IMAGES: 2 }, "<h1>제목</h1><p>" + img.repeat(3) + "</p>"));
  refused(await limited({ MAX_IMAGE_BYTES: 10 }, "<h1>제목</h1><p>" + img + "</p>"));
  refused(await limited({ MAX_TOTAL_IMAGE_BYTES: 100 }, "<h1>제목</h1><p>" + img.repeat(3) + "</p>"));
});

test("a nonsense base64 picture is left out and the rest is pasted", async () => {
  const t = await editorWithCaret();
  await t.paste(clipboard({ html: '<p>글<img src="data:image/png;base64,@@@@@@@@">자</p>' }));
  assert.equal(t.editor.querySelectorAll("img").length, 0);
  assert.equal(t.editor.textContent, "앞글자뒤");
});

test("a picture that is no PNG or JPEG inside its address is refused by the editor's header check, the rest stays", async () => {
  const t = await editorWithCaret();
  await t.paste(clipboard({ html: `<p>글<img src="data:image/png;base64,${Buffer.from("not a picture at all, just text").toString("base64")}">자</p>` }));
  assert.equal(t.editor.querySelectorAll("img").length, 0);
  assert.equal(t.editor.textContent, "앞글자뒤");
});

test("plain text over the bound is refused, with a message", async () => {
  const t = await editorWithCaret();
  const limits = t.host.__EASYOFFICE_DOC_PASTE_MODEL__.LIMITS;
  const saved = limits.MAX_TEXT_CHARS;
  limits.MAX_TEXT_CHARS = 10;
  try {
    await t.paste(clipboard({ text: "0123456789ABC" }));
  } finally {
    limits.MAX_TEXT_CHARS = saved;
  }
  assert.equal(t.editor.textContent, "앞뒤");
  assert.equal(t.feedback(), "붙여넣을 내용이 너무 큽니다");
});

test("control characters of the text are dropped, line ends of every kind split paragraphs", async () => {
  const t = await editorWithCaret();
  await t.paste(clipboard({ text: "a\u0000b\u0007c\rd\r\ne f" }));
  assert.equal(t.editor.textContent.includes("\u0000"), false);
  assert.deepEqual(shapes(t.editor), ["p[앞abc]", "p[d]", "p[e f뒤]"]);
});

test("character formats are read once per element, not once per ancestor of every text", async () => {
  const t = await editorWithCaret();
  const runs = t.host.__EASYOFFICE_DOC_RUNS__;
  const original = runs.readLegacyFormat;
  let reads = 0;
  runs.readLegacyFormat = (el) => {
    reads += 1;
    return original(el);
  };
  try {
    await t.paste(clipboard({ html: "<span>".repeat(98) + "x<i></i>".repeat(500) + "</span>".repeat(98), text: "t" }));
  } finally {
    runs.readLegacyFormat = original;
  }
  assert.ok(t.editor.textContent.includes("x"), "the content was pasted");
  assert.ok(reads <= 98 + 500 + 10, "reads " + reads);
});

test("the element and format reads have a budget: over it the paste is refused as a whole", async () => {
  refused(await limited({ MAX_FORMAT_READS: 50 }, "<h1>제목</h1>" + "<b>x</b>".repeat(100)));
});

test("HTML over the size bound is not even cleaned: the check is the first thing the paste does", async () => {
  const t = await editorWithCaret();
  const sanitize = t.host.__EASYOFFICE_DOC_PASTE_SANITIZE__;
  const original = sanitize.sanitizeOfficeHtml;
  let calls = 0;
  sanitize.sanitizeOfficeHtml = (html) => {
    calls += 1;
    return original(html);
  };
  try {
    await t.paste(clipboard({ html: "<p>" + "가".repeat(4 * 1024 * 1024 + 1) + "</p>", text: "글자만", rtf: "{\\rtf1 \\pict}" }));
  } finally {
    sanitize.sanitizeOfficeHtml = original;
  }
  assert.equal(calls, 0);
  assert.equal(t.editor.textContent, "앞글자만뒤");
  assert.equal(t.feedback(), "붙여넣을 내용이 너무 커서 글자만 붙였습니다");
});

// ---- random markup -------------------------------------------------------------------------------------------

test("random tag soup never throws and never leaves anything but the editor's own elements", async () => {
  const pieces = [
    "<p>", "</p>", "<b>", "</b>", "<i>", "<a href=\"https://example.com\">", "<a href=\"javascript:1\">", "</a>", "<table>", "<tr>", "<td colspan=2>", "</td>", "</tr>", "</table>",
    "<ul>", "<li>", "</li>", "</ul>", "<ol>", "</ol>", "<h1>", "</h1>", "<br>", "<script>", "</script>", "<style>", "</style>", "<!--", "-->", "<img src=\"https://e.example/a.png\">",
    `<img src="data:image/png;base64,${PNG_1x1}">`, "<span style=\"color:#f00;font-size:12pt\">", "</span>", "<div onclick=x>", "</div>", "<svg>", "</svg>", "<iframe>", "text", " ", "\n", "&amp;", "<", ">", "<o:p>", "</o:p>",
    "<td>", "<th>", "<tbody>", "<thead>", "<pre>", "</pre>", "<center>", "<font color=red>", "<section data-eo-hf=\"x\">",
  ];
  let seed = 20260721;
  const next = () => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed;
  };
  const t = await editorWithCaret();
  for (let round = 0; round < 120; round += 1) {
    t.editor.replaceChildren();
    const para = t.document.createElement("p");
    para.appendChild(t.document.createTextNode("앞뒤"));
    t.editor.appendChild(para);
    t.caretAt(para.firstChild, 1);
    const html = Array.from({ length: 1 + (next() % 40) }, () => pieces[next() % pieces.length]).join("");
    await t.paste(clipboard({ html, text: "대체" }));
    assertCleanTree(t.editor);
    assert.ok(!t.editor.textContent.includes("alert"));
  }
});

test("hostile SVG data URLs and pixel-bomb GIFs leave zero pictures and a clean editor tree", async () => {
  const t = await editorWithCaret();
  const bombGif = Buffer.from([
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0xff, 0xff, 0xff, 0xff, 0x80, 0, 0,
    ...Array.from({ length: 16 }, () => 0), 0x3b,
  ]).toString("base64");
  const svgBomb = [
    '<svg><script>alert(1)</script><rect/></svg>',
    '<svg onload="alert(2)"><rect/></svg>',
    '<svg><foreignObject><body/></foreignObject></svg>',
    '<!DOCTYPE svg [<!ENTITY e "x">]><svg><text>&e;</text></svg>',
    '<svg><image href="https://evil.example/x.png"/></svg>',
    `<svg><image href="data:image/svg+xml;base64,${Buffer.from("<svg><rect/></svg>").toString("base64")}"/></svg>`,
  ].map((s) => `<img src="data:image/svg+xml;base64,${Buffer.from(s, "utf8").toString("base64")}">`).join("");
  await t.paste(clipboard({ html: `<p>안전<img src="data:image/gif;base64,${bombGif}">${svgBomb}</p>`, text: "x" }));
  assert.equal(t.editor.querySelectorAll("img").length, 0);
  assert.equal(t.editor.textContent, "앞안전뒤");
  assertCleanTree(t.editor);
});

test("hostile !supportLists conditionals strip active payloads cleanly", async () => {
  const t = await editorWithCaret();
  const html = '<p class="MsoListParagraph" style="mso-list:l0 level1 lfo1"><![if !supportLists]><script>alert(1)</script><img src=x onerror=alert(2)><span>1.</span><![endif]>목록글</p>';
  await t.paste(clipboard({ html, text: "목록글" }));
  assertCleanTree(t.editor);
  assert.equal(t.editor.querySelectorAll("img").length, 0);
  assert.ok(!t.editor.textContent.includes("alert"));
  assert.ok(t.editor.textContent.includes("목록글"));
});
