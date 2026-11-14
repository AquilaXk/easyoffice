import test from "node:test";
import assert from "node:assert/strict";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { readText } from "./test-support/read-text.mjs";

// HWP printing (PD-104): the shell fetches pages in batches from the editor, builds a hidden root
// with one @page size per paper size, and prints the window. The DOM is a small fake; the editor
// is a function that answers `getPrintPages`.

const PRINT_JS = new URL("../crates/easyoffice-app/ui/hwp-print.js", import.meta.url);
const UI = (name) => readText(new URL(`../crates/easyoffice-app/ui/${name}`, import.meta.url));
const A4 = { widthMm: 210, heightMm: 297 };
const LANDSCAPE = { widthMm: 297, heightMm: 210 };

class FakeSvg {
  constructor(text) {
    this.text = text;
    this.tagName = "svg";
    this.localName = "svg";
    this.namespaceURI = "http://www.w3.org/2000/svg";
    this.attrs = new Map([["id", "g1"], ["fill", "url(#g1)"], ["onload", "x()"], ["href", "https://evil.example/a"]]);
    this.dropped = [];
  }
  get attributes() { return [...this.attrs].map(([name, value]) => ({ name, value })); }
  hasAttribute(k) { return this.attrs.has(k); }
  getAttribute(k) { return this.attrs.get(k) ?? null; }
  setAttribute(k, v) { this.attrs.set(k, v); }
  removeAttribute(k) { this.attrs.delete(k); }
  querySelectorAll() { return []; }
  remove() { this.removed = true; }
}

const SVG_NS = "http://www.w3.org/2000/svg";

// An element tree with the parts of the DOM the sanitizer uses, so the lists are tried on trees
// shaped like the attacks (the parsing itself is the browser's).
class El {
  constructor(localName, attrs = {}, children = [], namespaceURI = SVG_NS) {
    this.localName = localName;
    this.namespaceURI = namespaceURI;
    this.attrs = new Map(Object.entries(attrs));
    this.children = children;
    children.forEach((c) => { c.parent = this; });
  }
  get attributes() { return [...this.attrs].map(([name, value]) => ({ name, value })); }
  get childNodes() { return [...this.children]; }
  removeAttribute(k) { this.attrs.delete(k); }
  querySelectorAll() { return this.children.flatMap((c) => [c, ...c.querySelectorAll()]); }
  remove() { this.parent.children = this.parent.children.filter((c) => c !== this); }
  replaceWith(...nodes) {
    const at = this.parent.children.indexOf(this);
    nodes.forEach((n) => { n.parent = this.parent; });
    this.parent.children.splice(at, 1, ...nodes);
  }
  names() { return this.children.map((c) => c.localName); }
}

class FakeNode {
  constructor(tag) { this.tag = tag; this.children = []; this.className = ""; this.id = ""; this.attrs = new Map(); this.listeners = new Map(); this.removed = false; this.textContent = ""; }
  setAttribute(k, v) { this.attrs.set(k, v); }
  appendChild(n) { this.children.push(n); return n; }
  append(...ns) { this.children.push(...ns); }
  addEventListener(t, fn) { this.listeners.set(t, fn); }
  querySelector() { return this; }
  remove() { this.removed = true; }
}

class FakeSheet {
  constructor() { this.cssRules = []; }
  insertRule(rule, index) { this.cssRules.splice(index, 0, rule); return index; }
}

function setup({ pages, capabilities = ["print-pages-v1"], printImpl, desktopBridge } = {}) {
  const body = new FakeNode("body");
  const byId = new Map();
  const doc = {
    body, adoptedStyleSheets: [], listeners: new Map(),
    createElement: (tag) => new FakeNode(tag),
    importNode: (n) => n,
    getElementById: (id) => [...byId.values()].find((n) => n.id === id && !n.removed) ?? null,
    addEventListener(t, fn) { this.listeners.set(t, fn); },
    removeEventListener(t) { this.listeners.delete(t); },
  };
  const appendChild = body.appendChild.bind(body);
  body.appendChild = (n) => { byId.set(n.id || String(byId.size), n); return appendChild(n); };
  const env = {
    CSSStyleSheet: FakeSheet,
    DOMParser: class { parseFromString(text) { return { documentElement: new FakeSvg(text), querySelector: () => null }; } },
    setTimeout: (fn) => { setImmediate(fn); return 1; }, clearTimeout() {},
    requestAnimationFrame: (fn) => setImmediate(fn),
    addEventListener(t, fn) { env.after = fn; }, removeEventListener() {},
  };
  const requests = [];
  const hwpHost = {
    getCapabilities: () => capabilities,
    call: async (method, params) => {
      requests.push([method, params]);
      const slice = pages.slice(params.from, params.from + params.count);
      return { pageCount: pages.length, pages: slice };
    },
  };
  const notes = [];
  const printed = [];
  loadUiModule(PRINT_JS, env);
  const deps = { document: doc, hwpHost, env, notify: (t) => notes.push(t), desktopBridge };
  if (!desktopBridge) deps.print = printImpl ?? (() => { printed.push(true); });
  else env.print = () => { printed.push("window"); };
  const print = env.__EASYOFFICE_CREATE_HWP_PRINT__(deps);
  return { print, doc, env, requests, notes, printed, byId, internals: env.__EASYOFFICE_HWP_PRINT_INTERNALS__ };
}

const makePages = (n, size = A4) => Array.from({ length: n }, (_, i) => ({ svg: `<svg data-i="${i}"/>`, ...size }));

test("pages are fetched in batches of ten and printed once from a hidden root", async () => {
  const t = setup({ pages: makePages(23) });
  assert.equal(await t.print.print(), true);
  assert.deepEqual(t.requests.map(([, p]) => [p.from, p.count]), [[0, 10], [10, 10], [20, 10]]);
  const root = [...t.byId.values()].find((n) => n.id === t.internals.ROOT_ID);
  assert.equal(root.children.length, 23);
  assert.equal(t.printed.length, 1);
  assert.ok(t.notes.includes("인쇄 대화상자를 여는 중…"));
});

test("one @page rule and page box per paper size, with the first size as the default", async () => {
  const t = setup({ pages: [...makePages(2), ...makePages(1, LANDSCAPE), ...makePages(1)] });
  await t.print.print();
  const rules = t.doc.adoptedStyleSheets[0].cssRules;
  assert.equal(rules.filter((r) => r.startsWith("@page eo-paper-")).length, 2);
  assert.ok(rules.includes("@page { size: 210mm 297mm; margin: 0; }"));
  assert.ok(rules.includes("@page eo-paper-1 { size: 297mm 210mm; margin: 0; }"));
  assert.ok(rules.some((r) => r.startsWith(".eo-paper-0 { page: eo-paper-0; width: 210mm; height: calc(297mm - 0.1mm);")));
});

test("the page SVG is sanitized and its ids are namespaced per page", async () => {
  const t = setup({ pages: makePages(1) });
  await t.print.print();
  const root = [...t.byId.values()].find((n) => n.id === t.internals.ROOT_ID);
  const svg = root.children[0].children[0];
  assert.equal(svg.attrs.has("onload"), false);
  assert.equal(svg.attrs.has("href"), false, "an external reference is dropped");
  assert.equal(svg.attrs.get("id"), "eo-p0-g1");
  assert.equal(svg.attrs.get("fill"), "url(#eo-p0-g1)");
});

test("cancel stops fetching and leaves nothing behind; the next print works", async () => {
  const t = setup({ pages: makePages(40) });
  const first = t.print.print();
  await new Promise((r) => setImmediate(r));
  t.print.cancel();
  assert.equal(await first, false);
  assert.equal(t.printed.length, 0);
  assert.ok(t.notes.includes("인쇄를 취소했습니다."));
  assert.equal(t.print.isBusy(), false);
  assert.equal(t.doc.adoptedStyleSheets.length, 0);
  assert.equal(await t.print.print(), true);
  assert.equal(t.printed.length, 1);
});

test("a second print while one is preparing is refused; Esc cancels the running one", async () => {
  const t = setup({ pages: makePages(40) });
  const first = t.print.print();
  await new Promise((r) => setImmediate(r));
  assert.equal(await t.print.print(), false);
  assert.ok(t.notes.includes("이미 인쇄를 준비하고 있습니다."));
  t.doc.listeners.get("keydown")({ key: "Escape" });
  assert.equal(await first, false);
});

test("progress text counts pages and the unsupported, empty and oversized cases fail with a message", async () => {
  const { internals } = setup({ pages: makePages(1) });
  assert.equal(internals.progressText(3, 12), "인쇄 준비 중… (3/12)");
  await assert.rejects(setup({ pages: makePages(1), capabilities: [] }).print.print(), /지원하지 않습니다/);
  await assert.rejects(setup({ pages: [] }).print.print(), /인쇄할 쪽이 없습니다/);
  const huge = [{ svg: "x".repeat(internals.MAX_SVG_CHARS + 1), ...A4 }];
  await assert.rejects(setup({ pages: huge }).print.print(), /문서가 커서/);
  await assert.rejects(setup({ pages: [{ svg: "<svg/>", widthMm: 0, heightMm: 297 }] }).print.print(), /올바르지 않은 쪽/);
});

test("a failing print dialog removes the pages and reports the error; afterprint also cleans up", async () => {
  const t = setup({ pages: makePages(2), printImpl: () => { throw new Error("no printer"); } });
  await assert.rejects(t.print.print(), /no printer/);
  assert.equal(t.doc.adoptedStyleSheets.length, 0);
  const ok = setup({ pages: makePages(2) });
  await ok.print.print();
  assert.equal(ok.doc.adoptedStyleSheets.length, 1, "the pages stay while the dialog is open");
  ok.env.after();
  assert.equal(ok.doc.adoptedStyleSheets.length, 0);
});

test("the smoke step checks the prepared pages and rules but never opens the print dialog", async () => {
  const t = setup({ pages: makePages(12) });
  assert.equal(await t.print.smoke(), "pages=12 rules=3");
  assert.deepEqual(t.requests.map(([, p]) => [p.from, p.count]), [[0, 10], [10, 10]]);
  assert.equal(t.printed.length, 0, "print() of the window is not called");
  assert.equal(t.notes.includes("인쇄 대화상자를 여는 중…"), false);
  assert.equal(t.doc.adoptedStyleSheets.length, 0, "the hidden root and its rules are removed again");
  assert.equal([...t.byId.values()].some((n) => n.id === t.internals.ROOT_ID && !n.removed), false);
  assert.equal(t.print.isBusy(), false);
  assert.equal(await t.print.print(), true, "a real print still works afterwards");
  assert.equal(t.printed.length, 1);
});

test("the smoke step fails with a message when printing is unsupported, empty, cancelled or busy", async () => {
  await assert.rejects(setup({ pages: makePages(1), capabilities: [] }).print.smoke(), /지원하지 않습니다/);
  await assert.rejects(setup({ pages: [] }).print.smoke(), /인쇄할 쪽이 없습니다/);
  await assert.rejects(setup({ pages: [{ svg: "<svg/>", widthMm: 0, heightMm: 297 }] }).print.smoke(), /올바르지 않은 쪽/);
  const t = setup({ pages: makePages(40) });
  const running = t.print.print();
  await new Promise((r) => setImmediate(r));
  await assert.rejects(t.print.smoke(), /인쇄 준비가 끝나지 않았습니다/);
  t.print.cancel();
  assert.equal(await running, false);
});

test("the shell wires printing: menu item, shortcut, no new shell command, only inline-free CSS", () => {
  const app = UI("app.js");
  assert.ok(app.includes("__EASYOFFICE_CREATE_HWP_PRINT__") && app.includes("hwpPrint.print()"));
  const catalog = loadUiModule(new URL("../crates/easyoffice-app/ui/ribbon-catalog.js", import.meta.url), {}).__EASYOFFICE_RIBBON_CATALOG__;
  const binding = catalog.WORKSPACE_SHORTCUT_BINDINGS.find((b) => b.keys === "Ctrl+P");
  assert.deepEqual(
    { action: binding.action, module: binding.module, forward: binding.forwardFromEditor, desktopOnly: binding.desktopOnly },
    { action: "print", module: undefined, forward: true, desktopOnly: true },
    "Ctrl+P is browser-reserved, so only the desktop shell takes it over; the action picks the print of the module in front",
  );
  const js = UI("hwp-print.js");
  assert.ok(!/\.style\.|setAttribute\("style"|createElement\("style"\)|innerHTML|eval\(/.test(js), "no inline style, no markup injection");
  const css = UI("shell.css");
  assert.ok(css.includes("body > :not(#eo-print-root)") && css.includes("#eo-print-root { display: none; }"));
  assert.match(app, /__EASYOFFICE_CREATE_HWP_PRINT__\(\{[^}]*desktopBridge/s, "the print module gets the desktop bridge");
  const caps = readText(new URL("../crates/easyoffice-desktop/capabilities/main-window.json", import.meta.url));
  assert.ok(caps.includes('"allow-print-webview"'), "the native print command is granted");
  assert.ok(UI("desktop-bridge.js").includes('printWebview: "print_webview"'));
});

const shellBridge = (calls, { available = true, fail = false } = {}) => ({
  isAvailable: () => available,
  windowCommand: async (key, args) => {
    calls.push([key, args]);
    if (fail) throw new Error("인쇄 대화상자를 열지 못했습니다.");
  },
});

test("in the desktop shell the pages are printed by the native print command, not window.print", async () => {
  const calls = [];
  const t = setup({ pages: makePages(12), desktopBridge: shellBridge(calls) });
  assert.equal(await t.print.print(), true);
  assert.deepEqual(calls, [["printWebview", undefined]], "no argument: the shell prints the caller's own document");
  assert.deepEqual(t.printed, [], "window.print is not used");
  const root = [...t.byId.values()].find((n) => n.id === t.internals.ROOT_ID);
  assert.equal(root.children.length, 12, "every page is in the hidden root before the command runs");
});

test("without the shell (--serve) the page falls back to window.print", async () => {
  const calls = [];
  const t = setup({ pages: makePages(2), desktopBridge: shellBridge(calls, { available: false }) });
  assert.equal(await t.print.print(), true);
  assert.deepEqual(calls, []);
  assert.deepEqual(t.printed, ["window"]);
});

test("a cancelled preparation never reaches the native print command", async () => {
  const calls = [];
  const t = setup({ pages: makePages(40), desktopBridge: shellBridge(calls) });
  const running = t.print.print();
  await new Promise((r) => setImmediate(r));
  t.print.cancel();
  assert.equal(await running, false);
  assert.deepEqual(calls, []);
  assert.deepEqual(t.printed, []);
});

test("a native print command that fails removes the pages and reports its message", async () => {
  const calls = [];
  const t = setup({ pages: makePages(1), desktopBridge: shellBridge(calls, { fail: true }) });
  await assert.rejects(t.print.print(), /인쇄 대화상자를 열지 못했습니다/);
  assert.equal(t.doc.adoptedStyleSheets.length, 0);
  assert.equal([...t.byId.values()].find((n) => n.id === t.internals.ROOT_ID).removed, true);
});

test("the smoke step never calls the native print command", async () => {
  const calls = [];
  const t = setup({ pages: makePages(3), desktopBridge: shellBridge(calls) });
  assert.match(await t.print.smoke(), /pages=3/);
  assert.deepEqual(calls, []);
});

test("the sanitizer keeps what the page renderer writes and drops everything else", async () => {
  const { internals } = setup({ pages: makePages(1) });
  const other = "http://www.w3.org/1999/xhtml";
  const tree = new El("svg", { viewBox: "0 0 10 10", width: "10", height: "10", onload: "x()", class: "c", style: "fill:red" }, [
    new El("style"), new El("script"), new El("foreignObject"), new El("set", { attributeName: "href", to: "javascript:x()" }),
    new El("animate", { attributeName: "href", values: "javascript:x()" }), new El("base", { href: "https://evil.example/" }),
    new El("use", { href: "#a" }), new El("script", {}, [], other), new El("rect", { x: "1", fill: "url(#g)", onclick: "x()" }),
    new El("a", { href: "https://evil.example/" }, [new El("text", { x: "0" })]),
    new El("image", { href: "data:image/png;base64,AAAA", "xlink:href": "#p" }),
    new El("g", {}, [new El("style"), new El("path", { d: "M0 0", stroke: "url(https://evil.example/s.svg#x)" })]),
  ]);
  internals.sanitizeSvg(tree);
  assert.deepEqual(tree.names(), ["rect", "text", "image", "g"], "listed elements stay, the link keeps its text");
  assert.deepEqual([...tree.attrs.keys()], ["viewBox", "width", "height"], "no handler, class or style");
  const [rect, , image, group] = tree.children;
  assert.deepEqual([...rect.attrs.keys()], ["x", "fill"]);
  assert.deepEqual([...image.attrs.keys()], ["href", "xlink:href"]);
  assert.deepEqual(group.names(), ["path"]);
  assert.deepEqual([...group.children[0].attrs.keys()], ["d"], "a value may not point outside the page");
});

test("a reference is kept only when it points inside the page or is a plain image", async () => {
  const { internals } = setup({ pages: makePages(1) });
  const hrefs = ["#a", "data:image/jpeg;base64,/9j/", "data:image/svg+xml;base64,PHN2Zz4=", "", "https://evil.example/a.png",
    "javascript:alert(1)", "java\tscript:alert(1)", " java\nscript:alert(1)", "JaVaScRiPt:alert(1)", "data:text/html;base64,PHNjcmlwdD4=",
    "data:image/png,not-base64", "//evil.example/a.png", "vbscript:x"];
  const tree = new El("svg", {}, hrefs.map((href) => new El("image", { href })));
  internals.sanitizeSvg(tree);
  assert.deepEqual(tree.children.filter((c) => c.attrs.has("href")).map((c) => c.attrs.get("href")),
    ["#a", "data:image/jpeg;base64,/9j/", "data:image/svg+xml;base64,PHN2Zz4=", ""]);
  for (const value of ["\\75rl(//evil.example/x.svg#a)", "\\75 rl(https://evil.example/x)", "ur\\6c(//evil.example/x)", "java\\73cript:x()", "java\tscript:x()", "url( 'https://evil.example/x' )", "URL(data:image/svg+xml;base64,AA==)", "url(\"javascript:x\")"]) {
    const el = new El("rect", { fill: value });
    internals.sanitizeSvg(new El("svg", {}, [el]));
    assert.equal(el.attrs.has("fill"), false, value);
  }
  const kept = new El("rect", { fill: "url(#g) none", stroke: "url('#g')", filter: "url(#f)", "font-family": "Noto Sans KR, serif" });
  internals.sanitizeSvg(new El("svg", {}, [kept]));
  assert.deepEqual([...kept.attrs.keys()], ["fill", "stroke", "filter", "font-family"]);
});

test("a page with a document type declaration, entities or another root is refused", async () => {
  for (const svg of ['<!DOCTYPE svg [<!ENTITY a "aaaa">]><svg>&a;</svg>', '<!doctype svg><svg/>', '<svg><!ENTITY x "y"></svg>']) {
    await assert.rejects(setup({ pages: [{ svg, ...A4 }] }).print.print(), /올바르지 않은 쪽/, svg);
  }
});

test("svg data urls carrying scripts, handlers, javascript schemes, or external http urls are stripped", async () => {
  const { internals } = setup({ pages: makePages(1) });
  const b64 = (s) => "data:image/svg+xml;base64," + Buffer.from(s, "utf8").toString("base64");
  const pct = (s) => "data:image/svg+xml;charset=utf-8," + encodeURIComponent(s);
  const safeNs = '<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><rect width="8" height="8"/></svg>';
  const unsafePayloads = [
    b64('<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>'),
    pct('<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>'),
    b64('<svg onload="alert(1)"/>'),
    pct('<svg><rect onmouseover="fetch(1)"/></svg>'),
    b64('<svg><a href="javascript:alert(1)"><rect/></a></svg>'),
    b64('<svg><image href="https://evil.example/leak.png"/></svg>'),
    pct('<svg><rect fill="url(http://evil.example/x)"/></svg>'),
    "data:image/svg+xml;base64,!!!invalid-base64!!!",
  ];
  const safeUrls = [b64(safeNs), pct(safeNs)];
  const tree = new El("svg", {}, [...unsafePayloads, ...safeUrls].map((href) => new El("image", { href })));
  internals.sanitizeSvg(tree);
  const kept = tree.children.filter((c) => c.attrs.has("href")).map((c) => c.attrs.get("href"));
  assert.deepEqual(kept, safeUrls);
});
