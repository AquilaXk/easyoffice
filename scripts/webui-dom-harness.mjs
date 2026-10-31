import fs from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { matchesSelectorList } from "./webui-css-selector.mjs";

const cjsRequire = createRequire(import.meta.url);
const INDEX_HTML_PATH = fileURLToPath(
  new URL("../crates/easyoffice-app/ui/index.html", import.meta.url)
);
const UI_SCRIPT_MODULES = [
  "../crates/easyoffice-app/ui/offline-formula-ast.js",
  "../crates/easyoffice-app/ui/offline-formula-eval.js",
  "../crates/easyoffice-app/ui/offline-rpc.js",
  "../crates/easyoffice-app/ui/pii-dom-scrubber.js",
  "../crates/easyoffice-app/ui/sheet-store.js",
  "../crates/easyoffice-app/ui/ribbon-catalog.js",
  "../crates/easyoffice-app/ui/workspace-tools.js",
  "../crates/easyoffice-app/ui/workspace-panels.js",
  "../crates/easyoffice-app/ui/sheet-controller.js",
  "../crates/easyoffice-app/ui/ribbon-history.js",
  "../crates/easyoffice-app/ui/app.js",
].map((relPath) => ({
  relPath,
  resolvedPath: cjsRequire.resolve(relPath),
}));

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
  return "data-" + String(prop).replaceAll(/[A-Z]/g, (m) => "-" + m.toLowerCase());
}

function dataAttrToCamel(attr) {
  return attr.slice(5).replaceAll(/-([a-z])/g, (_, ch) => ch.toUpperCase());
}

function createStyleProxy(el) {
  const parseStyleMap = () => {
    const map = new Map();
    const raw = el._attrs.get("style") || "";
    raw.split(";").forEach((decl) => {
      const colonIdx = decl.indexOf(":");
      if (colonIdx > 0) {
        const k = decl.slice(0, colonIdx).trim();
        const v = decl.slice(colonIdx + 1).trim();
        if (k && v) map.set(k, v);
      }
    });
    return map;
  };
  const writeStyleMap = (map) => {
    const parts = [];
    for (const [k, v] of map.entries()) {
      parts.push(`${k}: ${v}`);
    }
    if (parts.length > 0) el._attrs.set("style", parts.join("; "));
    else el._attrs.delete("style");
  };
  const toKebab = (prop) =>
    prop.startsWith("--") ? prop : prop.replaceAll(/[A-Z]/g, (m) => "-" + m.toLowerCase());
  const methods = {
    setProperty(k, v) {
      const map = parseStyleMap();
      const strV = String(v ?? "").trim();
      if (strV) map.set(String(k), strV);
      else map.delete(String(k));
      writeStyleMap(map);
    },
  };
  return new Proxy(methods, {
    get(target, prop) {
      if (prop in target) return target[prop];
      if (typeof prop !== "string") return "";
      return parseStyleMap().get(toKebab(prop)) || "";
    },
    set(target, prop, value) {
      if (typeof prop === "string" && !(prop in target)) {
        target.setProperty(toKebab(prop), value);
      }
      return true;
    },
  });
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
    this.style = createStyleProxy(this);
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
    return matchesSelectorList(this, sel, this);
  }

  closest(sel) {
    let cur = this;
    while (cur && cur.nodeType === 1) {
      if (matchesSelectorList(cur, sel, this)) return cur;
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
      if (n.nodeType === 3) {
        return escapeHtmlText(n.textContent);
      }
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
  const root = {
    childNodes: [],
    _adopt: (n) => n,
    appendChild(n) {
      this.childNodes.push(n);
    },
  };
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
  const document = new MiniDocument(html);
  const windowListeners = new Map();
  const downloads = [];
  const revokedUrls = [];
  let nextTimerId = 1;
  let nextTimerSeq = 1;
  let virtualNowMs = 0;
  const cancelledTimers = new Set();
  const delayedTimers = new Map();
  const NativeURL = globalThis.URL;

  const urlShim = Object.assign(
    function URLShim(...args) {
      return new NativeURL(...args);
    },
    {
      createObjectURL: () => "blob:easyoffice-test",
      revokeObjectURL: (url) => {
        revokedUrls.push(url);
      },
    }
  );

  const hostEnv = {
    document,
    navigator: { platform: options.platform || "Win32" },
    location: { protocol: "app:" },
    atob: (b64) => Buffer.from(b64, "base64").toString("binary"),
    btoa: (bin) => Buffer.from(bin, "binary").toString("base64"),
    URL: urlShim,
    setTimeout: (fn, delay = 0) => {
      const id = nextTimerId;
      nextTimerId += 1;
      const seq = nextTimerSeq;
      nextTimerSeq += 1;
      const delayMs = Math.max(0, Number(delay) || 0);
      delayedTimers.set(id, { seq, fn, dueAt: virtualNowMs + delayMs });
      if (delayMs === 0) {
        setImmediate(() => {
          if (delayedTimers.has(id) && !cancelledTimers.has(id)) {
            delayedTimers.delete(id);
            fn();
          }
        });
      }
      return id;
    },
    clearTimeout: (id) => {
      cancelledTimers.add(id);
      delayedTimers.delete(id);
    },
    getSelection: () => options.selection || null,
    addEventListener: (type, fn) => {
      if (!windowListeners.has(type)) windowListeners.set(type, []);
      windowListeners.get(type).push(fn);
    },
    __TAURI_INTERNALS__: options.tauriInvoke ? { invoke: options.tauriInvoke } : undefined,
  };

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

  globalThis.__EASYOFFICE_HOST_ENV__ = hostEnv;
  try {
    for (const mod of UI_SCRIPT_MODULES) {
      delete cjsRequire.cache[mod.resolvedPath];
    }
    for (const mod of UI_SCRIPT_MODULES) {
      cjsRequire(mod.relPath);
    }
  } finally {
    delete globalThis.__EASYOFFICE_HOST_ENV__;
  }

  const sandbox = {
    setTimeout: hostEnv.setTimeout,
    clearTimeout: hostEnv.clearTimeout,
    __EASYOFFICE_FORMULA_AST__: hostEnv.__EASYOFFICE_FORMULA_AST__,
    __EASYOFFICE_OFFLINE_FORMULA_EVAL__: hostEnv.__EASYOFFICE_OFFLINE_FORMULA_EVAL__,
    __EASYOFFICE_SHEET_UTILS__: hostEnv.__EASYOFFICE_SHEET_UTILS__,
    __EASYOFFICE_CREATE_OFFLINE_RPC__: hostEnv.__EASYOFFICE_CREATE_OFFLINE_RPC__,
    __EASYOFFICE_CREATE_SHEET_STORE__: hostEnv.__EASYOFFICE_CREATE_SHEET_STORE__,
    __EASYOFFICE_RIBBON_CATALOG__: hostEnv.__EASYOFFICE_RIBBON_CATALOG__,
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

  document.dispatchEvent({ type: "DOMContentLoaded" });
  await flushAsyncTurns();

  function userClick(el) {
    if (!el) return;
    const focusable =
      el.closest('button, input, select, textarea, a, [contenteditable="true"], [tabindex]') ||
      null;
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

  function advanceTimers(ms = 2000) {
    const targetMs = virtualNowMs + Math.max(0, Number(ms) || 0);
    while (true) {
      let nextId = null;
      let nextEntry = null;
      for (const [id, entry] of delayedTimers.entries()) {
        if (cancelledTimers.has(id)) {
          delayedTimers.delete(id);
          continue;
        }
        if (entry.dueAt > targetMs) continue;
        if (
          !nextEntry ||
          entry.dueAt < nextEntry.dueAt ||
          (entry.dueAt === nextEntry.dueAt && entry.seq < nextEntry.seq)
        ) {
          nextId = id;
          nextEntry = entry;
        }
      }
      if (!nextEntry || nextId === null) break;
      delayedTimers.delete(nextId);
      virtualNowMs = nextEntry.dueAt;
      if (!cancelledTimers.has(nextId)) {
        nextEntry.fn();
      }
    }
    virtualNowMs = targetMs;
  }

  return { document, sandbox, downloads, revokedUrls, userClick, advanceTimers };
}
