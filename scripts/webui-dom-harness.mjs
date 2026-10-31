import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";

const ROOT = process.cwd();
const INDEX_HTML_PATH = path.join(ROOT, "crates/easyoffice-app/ui/index.html");
const OFFLINE_RPC_JS_PATH = path.join(ROOT, "crates/easyoffice-app/ui/offline-rpc.js");
const APP_JS_PATH = path.join(ROOT, "crates/easyoffice-app/ui/app.js");

const VOID_TAGS = new Set(["meta", "link", "input", "br", "hr", "img"]);

function decodeHtmlEntities(str) {
  return String(str || "")
    .replaceAll("&amp;", "&")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&quot;", '"')
    .replaceAll("&#39;", "'");
}

function camelToDataAttr(prop) {
  return (
    "data-" +
    String(prop).replaceAll(/[A-Z]/g, (m) => "-" + m.toLowerCase())
  );
}

function dataAttrToCamel(attr) {
  return attr
    .slice(5)
    .replaceAll(/-([a-z])/g, (_, ch) => ch.toUpperCase());
}

class MiniNode {
  constructor(nodeType, ownerDocument) {
    this.nodeType = nodeType;
    this.ownerDocument = ownerDocument;
    this.parentElement = null;
  }
}

class MiniTextNode extends MiniNode {
  constructor(text, ownerDocument) {
    super(3, ownerDocument);
    this._text = String(text ?? "");
  }

  get nodeValue() {
    return this._text;
  }

  set nodeValue(val) {
    this._text = String(val ?? "");
  }

  get data() {
    return this._text;
  }

  set data(val) {
    this._text = String(val ?? "");
  }

  get textContent() {
    return this._text;
  }

  set textContent(val) {
    this._text = String(val ?? "");
  }

  cloneNode() {
    return new MiniTextNode(this._text, this.ownerDocument);
  }
}

function parseSimpleSelector(rawSel) {
  let rest = rawSel.trim();
  const notConditions = [];
  rest = rest.replaceAll(/:not\(([^)]+)\)/g, (_, inner) => {
    notConditions.push(parseSimpleSelector(inner));
    return "";
  });
  const attrs = [];
  rest = rest.replaceAll(/\[([A-Za-z0-9_-]+)(?:="([^"]*)")?\]/g, (_, name, val) => {
    attrs.push({ name: name.toLowerCase(), value: val });
    return "";
  });
  let id = null;
  rest = rest.replaceAll(/#([A-Za-z0-9_-]+)/g, (_, m) => {
    id = m;
    return "";
  });
  const classes = [];
  rest = rest.replaceAll(/\.([A-Za-z0-9_-]+)/g, (_, m) => {
    classes.push(m);
    return "";
  });
  const tag = rest.trim() ? rest.trim().toUpperCase() : null;
  return { tag, id, classes, attrs, notConditions };
}

function matchesSimple(el, spec) {
  if (!el || el.nodeType !== 1) return false;
  if (spec.tag && spec.tag !== "*" && el.tagName !== spec.tag) return false;
  if (spec.id && el.id !== spec.id) return false;
  for (const cls of spec.classes) {
    if (!el.classList.contains(cls)) return false;
  }
  for (const attr of spec.attrs) {
    if (!el._attrs.has(attr.name)) return false;
    if (attr.value !== undefined && el._attrs.get(attr.name) !== attr.value) return false;
  }
  for (const neg of spec.notConditions) {
    if (matchesSimple(el, neg)) return false;
  }
  return true;
}

function pushSelectorToken(tokens, rawBuf) {
  const trimmed = rawBuf.trim();
  if (!trimmed) return;
  if (trimmed === ":scope") {
    tokens.push({ type: "scope" });
    return;
  }
  tokens.push({ type: "sel", spec: parseSimpleSelector(trimmed) });
}

function tokenizeComplexSelector(sel, scopeEl) {
  const working = sel.trim();
  const tokens = [];
  let buf = "";
  let bracketDepth = 0;
  let parenDepth = 0;
  for (let i = 0; i < working.length; i++) {
    const ch = working[i];
    if (ch === "[") bracketDepth += 1;
    else if (ch === "]" && bracketDepth > 0) bracketDepth -= 1;
    else if (ch === "(") parenDepth += 1;
    else if (ch === ")" && parenDepth > 0) parenDepth -= 1;

    if (bracketDepth === 0 && parenDepth === 0 && (ch === ">" || /\s/.test(ch))) {
      if (buf.trim()) {
        pushSelectorToken(tokens, buf);
        buf = "";
      }
      if (ch === ">") {
        if (tokens.length > 0 && tokens[tokens.length - 1].type === "comb") {
          tokens[tokens.length - 1].comb = ">";
        } else {
          tokens.push({ type: "comb", comb: ">" });
        }
      } else if (tokens.length > 0 && tokens[tokens.length - 1].type !== "comb") {
        tokens.push({ type: "comb", comb: " " });
      }
      continue;
    }
    buf += ch;
  }
  if (buf.trim()) {
    pushSelectorToken(tokens, buf);
  }
  if (tokens.length > 0 && tokens[tokens.length - 1].type === "comb") {
    tokens.pop();
  }
  return { scopeEl, tokens };
}

function matchesToken(el, tok, scopeEl) {
  if (!el || el.nodeType !== 1 || !tok) return false;
  if (tok.type === "scope") return el === scopeEl;
  if (tok.type === "sel") return matchesSimple(el, tok.spec);
  return false;
}

function matchChainAt(el, idx, tokens, scopeEl) {
  if (idx < 0 || !matchesToken(el, tokens[idx], scopeEl)) return false;
  if (idx === 0) return true;
  const combTok = tokens[idx - 1];
  if (!combTok || combTok.type !== "comb") return false;
  if (combTok.comb === ">") {
    return Boolean(el.parentElement) && matchChainAt(el.parentElement, idx - 2, tokens, scopeEl);
  }
  for (let p = el.parentElement; p; p = p.parentElement) {
    if (matchChainAt(p, idx - 2, tokens, scopeEl)) return true;
  }
  return false;
}

function matchesParsedChain(el, parsed) {
  const { scopeEl, tokens } = parsed;
  if (tokens.length === 0) return false;
  return matchChainAt(el, tokens.length - 1, tokens, scopeEl);
}

function matchesSelectorList(el, selectorList, scopeEl) {
  const parts = [];
  let buf = "";
  let parenDepth = 0;
  let bracketDepth = 0;
  for (let i = 0; i < selectorList.length; i++) {
    const ch = selectorList[i];
    if (ch === "(") parenDepth += 1;
    else if (ch === ")" && parenDepth > 0) parenDepth -= 1;
    else if (ch === "[") bracketDepth += 1;
    else if (ch === "]" && bracketDepth > 0) bracketDepth -= 1;
    if (ch === "," && parenDepth === 0 && bracketDepth === 0) {
      if (buf.trim()) parts.push(buf.trim());
      buf = "";
      continue;
    }
    buf += ch;
  }
  if (buf.trim()) parts.push(buf.trim());
  return parts.some((p) => matchesParsedChain(el, tokenizeComplexSelector(p, scopeEl)));
}

class MiniClassList {
  constructor(el) {
    this._el = el;
  }

  _set() {
    const raw = this._el._attrs.get("class") || "";
    return new Set(raw.split(/\s+/).filter(Boolean));
  }

  _write(set) {
    const joined = Array.from(set).join(" ");
    if (joined) this._el._attrs.set("class", joined);
    else this._el._attrs.delete("class");
  }

  add(...names) {
    const s = this._set();
    names.forEach((n) => s.add(n));
    this._write(s);
  }

  remove(...names) {
    const s = this._set();
    names.forEach((n) => s.delete(n));
    this._write(s);
  }

  contains(name) {
    return this._set().has(name);
  }

  toggle(name, force) {
    const s = this._set();
    const shouldAdd = force === undefined ? !s.has(name) : Boolean(force);
    if (shouldAdd) s.add(name);
    else s.delete(name);
    this._write(s);
    return shouldAdd;
  }
}

class MiniElement extends MiniNode {
  constructor(tagName, ownerDocument) {
    super(1, ownerDocument);
    this.tagName = String(tagName || "DIV").toUpperCase();
    this._attrs = new Map();
    this.childNodes = [];
    this._listeners = new Map();
    this.style = {
      setProperty(k, v) {
        this[k] = v;
      },
    };
    this.classList = new MiniClassList(this);
    this.colSpan = 1;
    this.open = false;
    this._explicitValue = undefined;
    this.dataset = new Proxy(
      {},
      {
        get: (_, prop) => {
          if (typeof prop !== "string") return undefined;
          return this._attrs.get(camelToDataAttr(prop));
        },
        set: (_, prop, value) => {
          if (typeof prop === "string") {
            this._attrs.set(camelToDataAttr(prop), String(value));
          }
          return true;
        },
        deleteProperty: (_, prop) => {
          if (typeof prop === "string") {
            this._attrs.delete(camelToDataAttr(prop));
          }
          return true;
        },
        has: (_, prop) => {
          if (typeof prop !== "string") return false;
          return this._attrs.has(camelToDataAttr(prop));
        },
        ownKeys: () =>
          Array.from(this._attrs.keys())
            .filter((k) => k.startsWith("data-"))
            .map(dataAttrToCamel),
        getOwnPropertyDescriptor: (_, prop) => {
          if (typeof prop === "string" && this._attrs.has(camelToDataAttr(prop))) {
            return {
              enumerable: true,
              configurable: true,
              value: this._attrs.get(camelToDataAttr(prop)),
            };
          }
          return undefined;
        },
      }
    );
  }

  get id() {
    return this._attrs.get("id") || "";
  }

  set id(v) {
    if (v) this._attrs.set("id", String(v));
    else this._attrs.delete("id");
  }

  get className() {
    return this._attrs.get("class") || "";
  }

  set className(v) {
    if (v) this._attrs.set("class", String(v));
    else this._attrs.delete("class");
  }

  get hidden() {
    return this._attrs.has("hidden");
  }

  set hidden(v) {
    if (v) this._attrs.set("hidden", "");
    else this._attrs.delete("hidden");
  }

  get value() {
    if (this._explicitValue !== undefined) return this._explicitValue;
    if (this.tagName === "SELECT") {
      const options = this.querySelectorAll("option");
      const sel = options.find((o) => o._attrs.has("selected")) || options[0];
      return sel ? (sel._attrs.get("value") ?? sel.textContent.trim()) : "";
    }
    return this._attrs.get("value") ?? "";
  }

  set value(v) {
    this._explicitValue = String(v ?? "");
  }

  get children() {
    return this.childNodes.filter((n) => n.nodeType === 1);
  }

  get cells() {
    return this.children.filter((c) => c.tagName === "TD" || c.tagName === "TH");
  }

  get textContent() {
    return this.childNodes.map((c) => c.textContent).join("");
  }

  set textContent(val) {
    this.childNodes.forEach((c) => {
      c.parentElement = null;
    });
    const s = String(val ?? "");
    this.childNodes = s.length > 0 ? [this._adopt(new MiniTextNode(s, this.ownerDocument))] : [];
  }

  get innerHTML() {
    return serializeNodesToHtml(this.childNodes);
  }

  set innerHTML(html) {
    this.childNodes.forEach((c) => {
      c.parentElement = null;
    });
    this.childNodes = [];
    const parsed = parseHtmlFragment(String(html ?? ""), this.ownerDocument);
    parsed.forEach((n) => this.appendChild(n));
  }

  _adopt(node) {
    if (node.parentElement) {
      node.parentElement._removeChildRef(node);
    }
    node.parentElement = this;
    return node;
  }

  _removeChildRef(node) {
    const idx = this.childNodes.indexOf(node);
    if (idx >= 0) this.childNodes.splice(idx, 1);
    node.parentElement = null;
  }

  appendChild(node) {
    this.childNodes.push(this._adopt(node));
    return node;
  }

  replaceChildren(...nodes) {
    this.childNodes.forEach((c) => {
      c.parentElement = null;
    });
    this.childNodes = [];
    nodes.forEach((n) => {
      const child = typeof n === "string" ? new MiniTextNode(n, this.ownerDocument) : n;
      this.appendChild(child);
    });
  }

  before(newNode) {
    if (!this.parentElement) return;
    const p = this.parentElement;
    p._adopt(newNode);
    const idx = p.childNodes.indexOf(this);
    if (idx >= 0) p.childNodes.splice(idx, 0, newNode);
  }

  remove() {
    if (this.parentElement) {
      this.parentElement._removeChildRef(this);
    }
  }

  cloneNode(deep = false) {
    const copy = new MiniElement(this.tagName, this.ownerDocument);
    for (const [k, v] of this._attrs.entries()) {
      copy._attrs.set(k, v);
    }
    copy.colSpan = this.colSpan;
    copy._explicitValue = this._explicitValue;
    if (deep) {
      this.childNodes.forEach((c) => copy.appendChild(c.cloneNode(true)));
    }
    return copy;
  }

  contains(other) {
    let cur = other;
    while (cur) {
      if (cur === this) return true;
      cur = cur.parentElement;
    }
    return false;
  }

  matches(sel) {
    return matchesSelectorList(this, sel, null);
  }

  closest(sel) {
    let cur = this;
    while (cur && cur.nodeType === 1) {
      if (matchesSelectorList(cur, sel, null)) return cur;
      cur = cur.parentElement;
    }
    return null;
  }

  querySelectorAll(sel) {
    const out = [];
    const walk = (parent) => {
      for (const child of parent.childNodes) {
        if (child.nodeType === 1) {
          if (matchesSelectorList(child, sel, this)) {
            out.push(child);
          }
          walk(child);
        }
      }
    };
    walk(this);
    return out;
  }

  querySelector(sel) {
    return this.querySelectorAll(sel)[0] || null;
  }

  addEventListener(type, fn) {
    if (!this._listeners.has(type)) this._listeners.set(type, []);
    this._listeners.get(type).push(fn);
  }

  dispatchEvent(ev) {
    const eventObj = {
      type: ev.type,
      target: ev.target || this,
      key: ev.key || "",
      metaKey: Boolean(ev.metaKey),
      ctrlKey: Boolean(ev.ctrlKey),
      shiftKey: Boolean(ev.shiftKey),
      isComposing: Boolean(ev.isComposing),
      clientX: ev.clientX ?? 0,
      clientY: ev.clientY ?? 0,
      defaultPrevented: false,
      preventDefault() {
        this.defaultPrevented = true;
      },
    };
    let cur = this;
    while (cur) {
      const fns = cur._listeners.get(eventObj.type) || [];
      for (const fn of fns) {
        fn.call(cur, eventObj);
      }
      if (eventObj.type === "focus" || eventObj.type === "blur") break;
      cur = cur.parentElement;
    }
    return !eventObj.defaultPrevented;
  }

  focus() {
    const doc = this.ownerDocument;
    if (doc.activeElement === this) {
      return;
    }
    if (doc.activeElement && doc.activeElement !== doc.body) {
      doc.activeElement.blur();
    }
    doc.activeElement = this;
    this.dispatchEvent({ type: "focus", target: this });
  }

  blur() {
    const doc = this.ownerDocument;
    if (doc.activeElement !== this) {
      return;
    }
    doc.activeElement = doc.body || null;
    this.dispatchEvent({ type: "blur", target: this });
  }

  click() {
    this.dispatchEvent({ type: "click", target: this });
  }

  showModal() {
    this.open = true;
  }

  close() {
    this.open = false;
  }

  scrollIntoView() {}

  getBoundingClientRect() {
    return { left: 0, top: 0, width: 640, height: 360 };
  }
}

function escapeHtmlText(str) {
  return String(str || "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function escapeHtmlAttr(str) {
  return escapeHtmlText(str).replaceAll('"', "&quot;");
}

function serializeNodesToHtml(nodes) {
  return nodes
    .map((n) => {
      if (n.nodeType === 3) return escapeHtmlText(n.textContent);
      const tag = n.tagName.toLowerCase();
      const attrParts = [];
      for (const [k, v] of n._attrs.entries()) {
        attrParts.push(v === "" ? k : `${k}="${escapeHtmlAttr(v)}"`);
      }
      const open = attrParts.length > 0 ? `<${tag} ${attrParts.join(" ")}>` : `<${tag}>`;
      if (VOID_TAGS.has(tag)) return open;
      return `${open}${serializeNodesToHtml(n.childNodes)}</${tag}>`;
    })
    .join("");
}

function parseAttributesInto(attrText, el) {
  const attrRe = /([A-Za-z0-9_:-]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?/g;
  let m;
  while ((m = attrRe.exec(attrText)) !== null) {
    const name = m[1].toLowerCase();
    const val = m[2] ?? m[3] ?? m[4] ?? "";
    el._attrs.set(name, decodeHtmlEntities(val));
  }
}

function parseHtmlFragment(html, ownerDocument) {
  const root = { childNodes: [], _adopt: (n) => n, appendChild(n) { this.childNodes.push(n); } };
  const stack = [root];
  const tagRe =
    /<!--[\s\S]*?-->|<!doctype[^>]*>|<\/([A-Za-z0-9-]+)\s*>|<([A-Za-z0-9-]+)((?:"[^"]*"|'[^']*'|[^>"'])*)(\/?)>/gi;
  let lastIdx = 0;
  let m;
  while ((m = tagRe.exec(html)) !== null) {
    if (m.index > lastIdx) {
      const txt = decodeHtmlEntities(html.slice(lastIdx, m.index));
      const top = stack[stack.length - 1];
      top.appendChild(new MiniTextNode(txt, ownerDocument));
    }
    lastIdx = tagRe.lastIndex;
    if (m[1]) {
      const closeTag = m[1].toUpperCase();
      for (let i = stack.length - 1; i >= 1; i--) {
        if (stack[i].tagName === closeTag) {
          stack.length = i;
          break;
        }
      }
    } else if (m[2]) {
      const tagLower = m[2].toLowerCase();
      const el = new MiniElement(tagLower, ownerDocument);
      parseAttributesInto(m[3] || "", el);
      stack[stack.length - 1].appendChild(el);
      if (!VOID_TAGS.has(tagLower) && m[4] !== "/") {
        stack.push(el);
      }
    }
  }
  if (lastIdx < html.length) {
    const tailTxt = decodeHtmlEntities(html.slice(lastIdx));
    stack[stack.length - 1].appendChild(new MiniTextNode(tailTxt, ownerDocument));
  }
  return root.childNodes;
}

class MiniDocument {
  constructor(htmlSource) {
    this.activeElement = null;
    this._listeners = new Map();
    const nodes = parseHtmlFragment(htmlSource, this);
    this.documentElement =
      nodes.find((n) => n.nodeType === 1 && n.tagName === "HTML") ||
      new MiniElement("html", this);
    this.body =
      this.documentElement.querySelector("body") || new MiniElement("body", this);
    this.activeElement = this.body;
  }

  createElement(tag) {
    return new MiniElement(tag, this);
  }

  createTextNode(text) {
    return new MiniTextNode(text, this);
  }

  getElementById(id) {
    return this.documentElement.querySelector(`#${id}`);
  }

  querySelector(sel) {
    return this.documentElement.querySelector(sel);
  }

  querySelectorAll(sel) {
    return this.documentElement.querySelectorAll(sel);
  }

  addEventListener(type, fn) {
    if (!this._listeners.has(type)) this._listeners.set(type, []);
    this._listeners.get(type).push(fn);
  }

  dispatchEvent(ev) {
    const fns = this._listeners.get(ev.type) || [];
    for (const fn of fns) {
      fn(ev);
    }
  }
}

export async function flushAsyncTurns(count = 6) {
  for (let i = 0; i < count; i++) {
    await new Promise((resolve) => setImmediate(resolve));
  }
}

export async function createWorkspaceHarness(options = {}) {
  const html = fs.readFileSync(INDEX_HTML_PATH, "utf8");
  const offlineRpcCode = fs.readFileSync(OFFLINE_RPC_JS_PATH, "utf8");
  const jsCode = fs.readFileSync(APP_JS_PATH, "utf8");
  const document = new MiniDocument(html);
  const windowListeners = new Map();
  const downloads = [];
  let nextTimerId = 1;
  const cancelledTimers = new Set();

  const sandbox = {
    document,
    navigator: { platform: options.platform || "Win32" },
    location: { protocol: "app:" },
    TextEncoder,
    TextDecoder,
    Blob,
    Uint8Array,
    Map,
    Set,
    WeakMap,
    Promise,
    JSON,
    Math,
    Number,
    String,
    Boolean,
    Array,
    Object,
    RegExp,
    Error,
    console,
    atob: (b64) => Buffer.from(b64, "base64").toString("binary"),
    btoa: (bin) => Buffer.from(bin, "binary").toString("base64"),
    URL: {
      createObjectURL: () => "blob:easyoffice-test",
      revokeObjectURL: () => {},
    },
    setTimeout: (fn) => {
      const id = nextTimerId;
      nextTimerId += 1;
      setImmediate(() => {
        if (!cancelledTimers.has(id)) {
          fn();
        }
      });
      return id;
    },
    clearTimeout: (id) => {
      cancelledTimers.add(id);
    },
    getSelection: () => options.selection || null,
    addEventListener: (type, fn) => {
      if (!windowListeners.has(type)) windowListeners.set(type, []);
      windowListeners.get(type).push(fn);
    },
    dispatchWindowKeydown: (ev) => {
      const eventObj = {
        type: "keydown",
        key: ev.key,
        metaKey: Boolean(ev.metaKey),
        ctrlKey: Boolean(ev.ctrlKey),
        shiftKey: Boolean(ev.shiftKey),
        isComposing: Boolean(ev.isComposing),
        defaultPrevented: false,
        preventDefault() {
          this.defaultPrevented = true;
        },
      };
      for (const fn of windowListeners.get("keydown") || []) {
        fn(eventObj);
      }
      return eventObj;
    },
  };

  if (options.tauriInvoke) {
    sandbox.__TAURI_INTERNALS__ = { invoke: options.tauriInvoke };
  }

  const origCreateElement = document.createElement.bind(document);
  document.createElement = (tag) => {
    const el = origCreateElement(tag);
    if (String(tag).toLowerCase() === "a") {
      el.click = () => {
        downloads.push({ href: el.href, download: el.download });
      };
    }
    return el;
  };

  sandbox.globalThis = sandbox;
  vm.createContext(sandbox);
  vm.runInContext(offlineRpcCode, sandbox, { filename: "offline-rpc.js" });
  vm.runInContext(jsCode, sandbox, { filename: "app.js" });
  document.dispatchEvent({ type: "DOMContentLoaded" });
  await flushAsyncTurns();

  function userClick(el) {
    if (!el) return;
    const focusable =
      el.closest(
        'button, input, select, textarea, a, [contenteditable="true"], [tabindex]'
      ) || null;
    if (
      document.activeElement &&
      document.activeElement !== document.body &&
      document.activeElement !== focusable
    ) {
      document.activeElement.blur();
    }
    if (focusable && document.activeElement !== focusable) {
      focusable.focus();
    }
    el.click();
  }

  return { document, sandbox, downloads, userClick };
}
