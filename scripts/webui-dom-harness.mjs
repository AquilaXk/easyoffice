import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { compileSelectorList, matchesSelectorList, selectorAttributeNames } from "./webui-css-selector.mjs";

const cjsRequire = createRequire(import.meta.url);
const INDEX_HTML_PATH = fileURLToPath(
  new URL("../crates/easyoffice-app/ui/index.html", import.meta.url)
);
const INDEX_HTML_RAW = fs.readFileSync(INDEX_HTML_PATH, "utf8");
const COMPILED_MODULES = new Map();

function runCachedModule(resolvedPath) {
  let entry = COMPILED_MODULES.get(resolvedPath);
  if (!entry) {
    const code = fs.readFileSync(resolvedPath, "utf8");
    const dir = path.dirname(resolvedPath);
    const req = createRequire(resolvedPath);
    const fn = vm.compileFunction(code, ["exports", "require", "module", "__filename", "__dirname"], {
      filename: resolvedPath,
    });
    entry = { fn, req, dir };
    COMPILED_MODULES.set(resolvedPath, entry);
  }
  const modObj = { exports: {} };
  entry.fn(modObj.exports, entry.req, modObj, resolvedPath, entry.dir);
  return modObj.exports;
}
const PROD_PRE_SCRIPTS = [
  "../crates/easyoffice-app/ui/theme.js",
  "../crates/easyoffice-app/ui/zoom-range.js",
  "../crates/easyoffice-app/ui/icons.js",
  "../crates/easyoffice-app/ui/sheet-utils.js",
].map((relPath) => ({
  relPath,
  resolvedPath: cjsRequire.resolve(relPath),
}));

const TEST_SUPPORT_MODULES = [
  "./test-support/offline-formula-ast.js",
  "./test-support/offline-formula-eval.js",
  "./test-support/offline-sort.js",
  "./test-support/offline-rpc.js",
].map((relPath) => ({
  relPath,
  resolvedPath: cjsRequire.resolve(relPath),
}));

const PROD_APP_SCRIPTS = [
  "../crates/easyoffice-app/ui/home-screen.js",
  "../crates/easyoffice-app/ui/recent-documents-dialog.js",
  "../crates/easyoffice-app/ui/update-notice.js",
  "../crates/easyoffice-app/ui/pii-dom-scrubber.js",
  "../crates/easyoffice-app/ui/sheet-rules.js",
  "../crates/easyoffice-app/ui/sheet-names.js",
  "../crates/easyoffice-app/ui/sheet-link.js",
  "../crates/easyoffice-app/ui/sheet-spill.js",
  "../crates/easyoffice-app/ui/sheet-calc-options.js",
  "../crates/easyoffice-app/ui/sheet-store.js",
  "../crates/easyoffice-app/ui/ribbon-catalog.js",
  "../crates/easyoffice-app/ui/save-payload.js",
  "../crates/easyoffice-app/ui/doc-runs.js",
  "../crates/easyoffice-app/ui/doc-ime.js",
  "../crates/easyoffice-app/ui/doc-paste-sanitize.js",
  "../crates/easyoffice-app/ui/doc-clipboard-images.js",
  "../crates/easyoffice-app/ui/doc-paste-model.js",
  "../crates/easyoffice-app/ui/doc-paste.js",
  "../crates/easyoffice-app/ui/doc-line-spacing.js",
  "../crates/easyoffice-app/ui/doc-runs-ui.js",
  "../crates/easyoffice-app/ui/doc-link.js",
  "../crates/easyoffice-app/ui/doc-toc.js",
  "../crates/easyoffice-app/ui/doc-table-grid.js",
  "../crates/easyoffice-app/ui/doc-image.js",
  "../crates/easyoffice-app/ui/doc-symbol-dialog.js",
  "../crates/easyoffice-app/ui/doc-dialogs-ext.js",
  "../crates/easyoffice-app/ui/doc-notes.js",
  "../crates/easyoffice-app/ui/doc-notes-ui.js",
  "../crates/easyoffice-app/ui/doc-table-style.js",
  "../crates/easyoffice-app/ui/doc-table-runs.js",
  "../crates/easyoffice-app/ui/doc-table-tab.js",
  "../crates/easyoffice-app/ui/doc-table.js",
  "../crates/easyoffice-app/ui/doc-pages-core.js",
  "../crates/easyoffice-app/ui/doc-pages.js",
  "../crates/easyoffice-app/ui/doc-page-frame.js",
  "../crates/easyoffice-app/ui/doc-page-setup.js",
  "../crates/easyoffice-app/ui/doc-page-print.js",
  "../crates/easyoffice-app/ui/desktop-bridge.js",
  "../crates/easyoffice-app/ui/window-bridge.js",
  "../crates/easyoffice-app/ui/ipc-transport.js",
  "../crates/easyoffice-app/ui/menu-keyboard.js",
  "../crates/easyoffice-app/ui/menu-kinds.js",
  "../crates/easyoffice-app/ui/custom-select.js",
  "../crates/easyoffice-app/ui/color-picker.js",
  "../crates/easyoffice-app/ui/workspace-scrub.js",
  "../crates/easyoffice-app/ui/find-core.js",
  "../crates/easyoffice-app/ui/find-doc.js",
  "../crates/easyoffice-app/ui/find-sheet.js",
  "../crates/easyoffice-app/ui/find-base.js",
  "../crates/easyoffice-app/ui/module-search.js",
  "../crates/easyoffice-app/ui/find-replace.js",
  "../crates/easyoffice-app/ui/slide-canvas.js",
  "../crates/easyoffice-app/ui/slide-clipboard.js",
  "../crates/easyoffice-app/ui/slide-context-menu.js",
  "../crates/easyoffice-app/ui/slide-dialogs.js",
  "../crates/easyoffice-app/ui/workspace-tools.js",
  "../crates/easyoffice-app/ui/workspace-panels.js",
  "../crates/easyoffice-app/ui/sheet-numfmt.js",
  "../crates/easyoffice-app/ui/sheet-numfmt-display.js",
  "../crates/easyoffice-app/ui/sheet-pivot-model.js",
  "../crates/easyoffice-app/ui/sheet-row-height.js",
  "../crates/easyoffice-app/ui/sheet-default-font.js",
  "../crates/easyoffice-app/ui/sheet-layout.js",
  "../crates/easyoffice-app/ui/sheet-table-presets.js",
  "../crates/easyoffice-app/ui/sheet-table-style.js",
  "../crates/easyoffice-app/ui/sheet-table-ops.js",
  "../crates/easyoffice-app/ui/sheet-table-gallery.js",
  "../crates/easyoffice-app/ui/sheet-hidden-rows.js",
  "../crates/easyoffice-app/ui/sheet-table-bake.js",
  "../crates/easyoffice-app/ui/sheet-table-totals.js",
  "../crates/easyoffice-app/ui/sheet-table-style-ui.js",
  "../crates/easyoffice-app/ui/sheet-table-dedupe.js",
  "../crates/easyoffice-app/ui/sheet-slicer.js",
  "../crates/easyoffice-app/ui/sheet-table-ui.js",
  "../crates/easyoffice-app/ui/sheet-view-state.js",
  "../crates/easyoffice-app/ui/sheet-protect.js",
  "../crates/easyoffice-app/ui/sheet-tab-names.js",
  "../crates/easyoffice-app/ui/sheet-tab-strip.js",
  "../crates/easyoffice-app/ui/sheet-outline.js",
  "../crates/easyoffice-app/ui/sheet-refs.js",
  "../crates/easyoffice-app/ui/sheet-ref-tokens.js",
  "../crates/easyoffice-app/ui/sheet-look.js",
  "../crates/easyoffice-app/ui/sheet-row-fit.js",
  "../crates/easyoffice-app/ui/sheet-freeze.js",
  "../crates/easyoffice-app/ui/sheet-grid-text.js",
  "../crates/easyoffice-app/ui/sheet-grid-selection.js",
  "../crates/easyoffice-app/ui/sheet-grid-layout-paint.js",
  "../crates/easyoffice-app/ui/sheet-grid.js",
  "../crates/easyoffice-app/ui/sheet-paste-core.js",
  "../crates/easyoffice-app/ui/sheet-fill-core.js",
  "../crates/easyoffice-app/ui/sheet-clip-html.js",
  "../crates/easyoffice-app/ui/sheet-clipboard.js",
  "../crates/easyoffice-app/ui/sheet-col-fit.js",
  "../crates/easyoffice-app/ui/sheet-fill-ui.js",
  "../crates/easyoffice-app/ui/sheet-paste-ui.js",
  "../crates/easyoffice-app/ui/sheet-formula-point.js",
  "../crates/easyoffice-app/ui/sheet-point-ui.js",
  "../crates/easyoffice-app/ui/sheet-nav.js",
  "../crates/easyoffice-app/ui/sheet-multi-range.js",
  "../crates/easyoffice-app/ui/sheet-shortcuts.js",
  "../crates/easyoffice-app/ui/sheet-edit-box.js",
  "../crates/easyoffice-app/ui/sheet-selection-chrome.js",
  "../crates/easyoffice-app/ui/sheet-mouse.js",
  "../crates/easyoffice-app/ui/sheet-input.js",
  "../crates/easyoffice-app/ui/sheet-sort.js",
  "../crates/easyoffice-app/ui/sheet-ops.js",
  "../crates/easyoffice-app/ui/sheet-dialog-kit.js",
  "../crates/easyoffice-app/ui/sheet-pivot-ui.js",
  "../crates/easyoffice-app/ui/sheet-dv-widgets.js",
  "../crates/easyoffice-app/ui/sheet-cf-presets.js",
  "../crates/easyoffice-app/ui/sheet-cf-preview.js",
  "../crates/easyoffice-app/ui/sheet-cf-quick-ui.js",
  "../crates/easyoffice-app/ui/sheet-cf-bar-ui.js",
  "../crates/easyoffice-app/ui/sheet-rules-ui.js",
  "../crates/easyoffice-app/ui/sheet-format.js",
  "../crates/easyoffice-app/ui/sheet-numfmt-ui.js",
  "../crates/easyoffice-app/ui/sheet-look-ops.js",
  "../crates/easyoffice-app/ui/sheet-color-ui.js",
  "../crates/easyoffice-app/ui/sheet-look-ui.js",
  "../crates/easyoffice-app/ui/sheet-filter.js",
  "../crates/easyoffice-app/ui/sheet-filter-ui.js",
  "../crates/easyoffice-app/ui/sheet-chart-svg.js",
  "../crates/easyoffice-app/ui/sheet-chart-ui.js",
  "../crates/easyoffice-app/ui/sheet-sort-ui.js",
  "../crates/easyoffice-app/ui/sheet-view-ui.js",
  "../crates/easyoffice-app/ui/sheet-page.js",
  "../crates/easyoffice-app/ui/sheet-page-print.js",
  "../crates/easyoffice-app/ui/sheet-names-ui.js",
  "../crates/easyoffice-app/ui/sheet-link-ui.js",
  "../crates/easyoffice-app/ui/sheet-protect-ui.js",
  "../crates/easyoffice-app/ui/sheet-protect-ranges.js",
  "../crates/easyoffice-app/ui/sheet-outline-ui.js",
  "../crates/easyoffice-app/ui/sheet-tabs.js",
  "../crates/easyoffice-app/ui/sheet-tab-ui.js",
  "../crates/easyoffice-app/ui/sheet-tab-menu.js",
  "../crates/easyoffice-app/ui/sheet-media-ui.js",
  "../crates/easyoffice-app/ui/sheet-note-ui.js",
  "../crates/easyoffice-app/ui/sheet-row-resize.js",
  "../crates/easyoffice-app/ui/sheet-size-dialogs.js",
  "../crates/easyoffice-app/ui/sheet-context-menu.js",
  "../crates/easyoffice-app/ui/sheet-text-columns.js",
  "../crates/easyoffice-app/ui/sheet-structure.js",
  "../crates/easyoffice-app/ui/sheet-cell-write.js",
  "../crates/easyoffice-app/ui/sheet-view-live.js",
  "../crates/easyoffice-app/ui/sheet-cell-edit.js",
  "../crates/easyoffice-app/ui/sheet-formula-bar.js",
  "../crates/easyoffice-app/ui/sheet-controller.js",
  "../crates/easyoffice-app/ui/sheet-delta.js",
  "../crates/easyoffice-app/ui/sheet-history.js",
  "../crates/easyoffice-app/ui/command-history.js",
  "../crates/easyoffice-app/ui/workspace-history.js",
  "../crates/easyoffice-app/ui/ribbon-history.js",
  "../crates/easyoffice-app/ui/workspace-session.js",
  "../crates/easyoffice-app/ui/file-open.js",
  "../crates/easyoffice-app/ui/pdf-edit.js",
  "../crates/easyoffice-app/ui/slide-studio.js",
  "../crates/easyoffice-app/ui/hwp-host.js",
  "../crates/easyoffice-app/ui/hwp-print.js",
  "../crates/easyoffice-app/ui/office-print.js",
  "../crates/easyoffice-app/ui/hwp-session.js",
  "../crates/easyoffice-app/ui/shell-chrome.js",
  "../crates/easyoffice-app/ui/copilot-catalog.js",
  "../crates/easyoffice-app/ui/copilot-text.js",
  "../crates/easyoffice-app/ui/copilot-diff.js",
  "../crates/easyoffice-app/ui/copilot-markdown.js",
  "../crates/easyoffice-app/ui/copilot-doc-html.js",
  "../crates/easyoffice-app/ui/copilot-sheet-ops.js",
  "../crates/easyoffice-app/ui/copilot-ext-ops.js",
  "../crates/easyoffice-app/ui/copilot-typing.js",
  "../crates/easyoffice-app/ui/copilot-cursor.js",
  "../crates/easyoffice-app/ui/copilot-timeline.js",
  "../crates/easyoffice-app/ui/copilot-stream.js",
  "../crates/easyoffice-app/ui/copilot-preview.js",
  "../crates/easyoffice-app/ui/copilot-perform-slide.js",
  "../crates/easyoffice-app/ui/copilot-perform.js",
  "../crates/easyoffice-app/ui/copilot-actions.js",
  "../crates/easyoffice-app/ui/copilot-context-compact.js",
  "../crates/easyoffice-app/ui/copilot-session-anchor.js",
  "../crates/easyoffice-app/ui/copilot-agent-loop.js",
  "../crates/easyoffice-app/ui/copilot-ghost.js",
  "../crates/easyoffice-app/ui/copilot-model-ui.js",
  "../crates/easyoffice-app/ui/copilot-ui.js",
  "../crates/easyoffice-app/ui/base-model.js",
  "../crates/easyoffice-app/ui/base-query-cache.js",
  "../crates/easyoffice-app/ui/base-virtual.js",
  "../crates/easyoffice-app/ui/base-grid-window.js",
  "../crates/easyoffice-app/ui/base-cell-nav.js",
  "../crates/easyoffice-app/ui/base-clipboard.js",
  "../crates/easyoffice-app/ui/base-cell-select.js",
  "../crates/easyoffice-app/ui/base-grid-size.js",
  "../crates/easyoffice-app/ui/base-import.js",
  "../crates/easyoffice-app/ui/base-studio.js",
  "../crates/easyoffice-app/ui/board-router.js",
  "../crates/easyoffice-app/ui/board-style-bar.js",
  "../crates/easyoffice-app/ui/board-studio.js",
  "../crates/easyoffice-app/ui/base-board-host.js",
  "../crates/easyoffice-app/ui/export-dialog.js",
  "../crates/easyoffice-app/ui/toolbar-scroll.js",
  "../crates/easyoffice-app/ui/pages-ui.js",
  "../crates/easyoffice-app/ui/app.js",
  "../crates/easyoffice-app/ui/print-preview.js",
  "../crates/easyoffice-app/ui/layout-lint-ui.js",
].map((relPath) => ({
  relPath,
  resolvedPath: cjsRequire.resolve(relPath),
}));

const VOID_TAGS = new Set(["meta", "link", "input", "br", "hr", "img", "col"]);

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
    getPropertyValue(k) {
      return parseStyleMap().get(String(k)) || "";
    },
    removeProperty(k) {
      const map = parseStyleMap();
      map.delete(String(k));
      writeStyleMap(map);
    },
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

// What a cached query answer depends on, counted for changes inside the document only: `structureEpoch` moves
// when an element gets another parent, and `attributeEpochs` holds, per attribute name, how often a value of
// that name changed. A selector reads only the attribute names `selectorAttributeNames` lists, so its answer
// stays valid until the structure or one of those names moves (see `querySelectorAll`). Changes to a tree that
// is not under the document element do not count: a query on the document never sees them, and a query on a
// node outside the document is not cached.
let structureEpoch = 0;
const attributeEpochs = new Map();
const QUERY_CACHE = new WeakMap();

/** True when `node` is the document element or lies below it. */
function isUnderDocument(node) {
  const root = node?.ownerDocument?.documentElement;
  for (let cur = node; cur; cur = cur._parent) {
    if (cur === root) return true;
  }
  return false;
}

function isQueryEntryFresh(entry) {
  if (entry.structure !== structureEpoch) return false;
  for (let i = 0; i < entry.names.length; i += 1) {
    if ((attributeEpochs.get(entry.names[i]) ?? 0) !== entry.seen[i]) return false;
  }
  return true;
}

class TrackedAttrs extends Map {
  constructor(owner) {
    super();
    this._owner = owner;
  }

  _changed(name) {
    if (isUnderDocument(this._owner)) attributeEpochs.set(name, (attributeEpochs.get(name) ?? 0) + 1);
  }

  set(name, value) {
    if (!super.has(name) || super.get(name) !== value) this._changed(name);
    return super.set(name, value);
  }

  delete(name) {
    if (super.has(name)) this._changed(name);
    return super.delete(name);
  }

  clear() {
    for (const name of super.keys()) this._changed(name);
    super.clear();
  }
}

class MiniNode {
  constructor(nodeType, ownerDocument) {
    this.nodeType = nodeType;
    this.ownerDocument = ownerDocument;
    this._parent = null;
  }

  get parentElement() {
    return this._parent;
  }

  set parentElement(next) {
    if (next === this._parent) return;
    if (this.nodeType === 1 && (isUnderDocument(this._parent) || isUnderDocument(next))) structureEpoch += 1;
    this._parent = next;
  }

  get parentNode() {
    return this.parentElement;
  }

  get nextSibling() {
    const p = this.parentElement;
    return p ? (p.childNodes[p.childNodes.indexOf(this) + 1] ?? null) : null;
  }

  get previousSibling() {
    const p = this.parentElement;
    return p ? (p.childNodes[p.childNodes.indexOf(this) - 1] ?? null) : null;
  }

  remove() {
    if (this.parentElement) this.parentElement._removeChildRef(this);
  }

  before(newNode) {
    if (!this.parentElement) return;
    const p = this.parentElement;
    p._adopt(newNode);
    const idx = p.childNodes.indexOf(this);
    if (idx >= 0) p.childNodes.splice(idx, 0, newNode);
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

  splitText(offset) {
    const tail = new MiniTextNode(this._text.slice(offset), this.ownerDocument);
    this._text = this._text.slice(0, offset);
    const p = this.parentElement;
    if (p) {
      tail.parentElement = p;
      p.childNodes.splice(p.childNodes.indexOf(this) + 1, 0, tail);
    }
    return tail;
  }
}

// The class tokens of one `class` attribute text, shared by every element that carries the same text. Selector
// matching asks `contains` of each element of the tree for every query, so splitting the text anew each time
// dominated the harness setup. The shared sets are read-only; the mutators below copy before they change one.
const CLASS_TOKEN_SETS = new Map();
const CLASS_TOKEN_SETS_LIMIT = 4096;

function classTokens(raw) {
  let tokens = CLASS_TOKEN_SETS.get(raw);
  if (!tokens) {
    if (CLASS_TOKEN_SETS.size >= CLASS_TOKEN_SETS_LIMIT) CLASS_TOKEN_SETS.clear();
    tokens = new Set(raw.split(/\s+/).filter(Boolean));
    CLASS_TOKEN_SETS.set(raw, tokens);
  }
  return tokens;
}

class MiniClassList {
  constructor(el) {
    this._el = el;
  }

  _set() {
    return new Set(classTokens(this._el._attrs.get("class") || ""));
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
    return classTokens(this._el._attrs.get("class") || "").has(name);
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

/** Error carrying the `{ status, message }` shape of a rejected desktop shell command. */
export function shellError(message) {
  return Object.assign(new Error(message), { status: "error" });
}

/** Nearest inclusive ancestor of `start` (elements only) matching `sel`, scoped to `scope`. */
function closestFrom(start, sel, scope) {
  for (let cur = start; cur?.nodeType === 1; cur = cur.parentElement) {
    if (matchesSelectorList(cur, sel, scope)) return cur;
  }
  return null;
}

/** Calls the listeners of `eventObj.type` from `start` up through its ancestors (focus/blur/mouseenter/mouseleave do not bubble). */
function dispatchFrom(start, eventObj) {
  for (let cur = start; cur; cur = cur.parentElement) {
    const fns = cur._listeners.get(eventObj.type) || [];
    for (const fn of fns) {
      fn.call(cur, eventObj);
      if (eventObj.immediateStopped) break;
    }
    if (
      eventObj.type === "focus" ||
      eventObj.type === "blur" ||
      eventObj.type === "mouseenter" ||
      eventObj.type === "mouseleave" ||
      eventObj.propagationStopped
    ) break;
  }
}

class MiniElement extends MiniNode {
  constructor(tagName, ownerDocument) {
    super(1, ownerDocument);
    this.tagName = String(tagName || "DIV").toUpperCase();
    this._attrs = new TrackedAttrs(this);
    this.childNodes = [];
    this._listeners = new Map();
    this.style = createStyleProxy(this);
    this.classList = new MiniClassList(this);
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
    const prev = this._attrs.get("id");
    if (prev && this.ownerDocument?._byId?.get(prev) === this) this.ownerDocument._byId.delete(prev);
    if (v) {
      const next = String(v);
      this._attrs.set("id", next);
      if (this.ownerDocument?._byId && !this.ownerDocument._byId.has(next)) {
        this.ownerDocument._byId.set(next, this);
      }
    } else {
      this._attrs.delete("id");
    }
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

  get disabled() {
    return this._attrs.has("disabled");
  }

  set disabled(v) {
    if (v) this._attrs.set("disabled", "");
    else this._attrs.delete("disabled");
  }

  getAttribute(name) {
    const k = String(name || "").toLowerCase();
    return this._attrs.has(k) ? this._attrs.get(k) : null;
  }

  hasAttribute(name) {
    return this._attrs.has(String(name || "").toLowerCase());
  }

  removeAttribute(name) {
    const k = String(name || "").toLowerCase();
    if (k === "id") {
      const prev = this._attrs.get("id");
      if (prev && this.ownerDocument?._byId?.get(prev) === this) this.ownerDocument._byId.delete(prev);
    }
    this._attrs.delete(k);
  }

  setAttribute(name, value) {
    const k = String(name || "").toLowerCase();
    const strVal = String(value ?? "");
    if (k === "id") {
      const prev = this._attrs.get("id");
      if (prev && prev !== strVal && this.ownerDocument?._byId?.get(prev) === this) {
        this.ownerDocument._byId.delete(prev);
      }
      if (strVal && this.ownerDocument?._byId && !this.ownerDocument._byId.has(strVal)) {
        this.ownerDocument._byId.set(strVal, this);
      }
    }
    this._attrs.set(k, strVal);
  }

  // Table cell spans reflect their attributes like the real DOM (default 1).
  get colSpan() {
    return Math.max(1, Math.trunc(Number(this._attrs.get("colspan"))) || 1);
  }

  set colSpan(v) {
    this._attrs.set("colspan", String(v));
  }

  get rowSpan() {
    return Math.max(1, Math.trunc(Number(this._attrs.get("rowspan"))) || 1);
  }

  set rowSpan(v) {
    this._attrs.set("rowspan", String(v));
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

  get firstChild() {
    return this.childNodes[0] ?? null;
  }

  get lastChild() {
    return this.childNodes.at(-1) ?? null;
  }

  insertBefore(newNode, refNode) {
    if (!refNode) return this.appendChild(newNode);
    newNode.parentElement?._removeChildRef(newNode);
    newNode.parentElement = this;
    this.childNodes.splice(this.childNodes.indexOf(refNode), 0, newNode);
    return newNode;
  }

  removeChild(node) {
    this._removeChildRef(node);
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
    return closestFrom(this, sel, this);
  }

  // The page asks the same few selectors again and again while the tree stays as it is, so the matches of a
  // selector are kept per root until the structure or an attribute the selector reads changes (see
  // `structureEpoch`). A root outside the document has no entry (its changes are not counted).
  _queryEntry(sel) {
    if (!isUnderDocument(this)) return null;
    let entries = QUERY_CACHE.get(this);
    if (!entries) {
      entries = new Map();
      QUERY_CACHE.set(this, entries);
    }
    let entry = entries.get(sel);
    if (entry && isQueryEntryFresh(entry)) return entry;
    const names = selectorAttributeNames(sel);
    entry = { structure: structureEpoch, names, seen: names.map((name) => attributeEpochs.get(name) ?? 0), all: null, first: undefined };
    entries.set(sel, entry);
    return entry;
  }

  // The caller gets its own array.
  querySelectorAll(sel) {
    const entry = this._queryEntry(sel);
    if (!entry) return this._collectMatches(sel);
    entry.all ??= this._collectMatches(sel);
    return entry.all.slice();
  }

  _collectMatches(sel) {
    const out = [];
    const matches = compileSelectorList(sel, this);
    const walk = (parent) => {
      for (const child of parent.childNodes) {
        if (child.nodeType === 1) {
          if (matches(child)) out.push(child);
          walk(child);
        }
      }
    };
    walk(this);
    return out;
  }

  // First match in document order; a list that is already known gives it at once, otherwise the walk stops
  // at the first match instead of listing the whole tree.
  querySelector(sel) {
    const entry = this._queryEntry(sel);
    if (entry?.all) return entry.all[0] ?? null;
    if (entry && entry.first !== undefined) return entry.first;
    const matches = compileSelectorList(sel, this);
    const find = (parent) => {
      for (const child of parent.childNodes) {
        if (child.nodeType !== 1) continue;
        if (matches(child)) return child;
        const inner = find(child);
        if (inner) return inner;
      }
      return null;
    };
    const first = find(this);
    if (entry) entry.first = first;
    return first;
  }

  addEventListener(type, fn) {
    if (!this._listeners.has(type)) this._listeners.set(type, []);
    this._listeners.get(type).push(fn);
  }

  removeEventListener(type, fn) {
    this._listeners.set(type, (this._listeners.get(type) || []).filter((f) => f !== fn));
  }

  dispatchEvent(ev) {
    const eventObj = {
      type: ev.type,
      target: ev.target || this,
      relatedTarget: ev.relatedTarget ?? null,
      key: ev.key || "",
      code: ev.code || "",
      keyCode: ev.keyCode ?? 0,
      which: ev.which ?? ev.keyCode ?? 0,
      metaKey: Boolean(ev.metaKey),
      ctrlKey: Boolean(ev.ctrlKey),
      shiftKey: Boolean(ev.shiftKey),
      altKey: Boolean(ev.altKey),
      isComposing: Boolean(ev.isComposing),
      dataTransfer: ev.dataTransfer,
      clipboardData: ev.clipboardData,
      button: ev.button ?? 0,
      inputType: ev.inputType || "",
      data: ev.data ?? null,
      clientX: ev.clientX ?? 0,
      clientY: ev.clientY ?? 0,
      pointerId: ev.pointerId ?? 1,
      isPrimary: ev.isPrimary ?? true,
      timeStamp: ev.timeStamp ?? 0,
      defaultPrevented: false,
      preventDefault() {
        this.defaultPrevented = true;
      },
      stopPropagation() {
        this.propagationStopped = true;
      },
      stopImmediatePropagation() {
        this.propagationStopped = true;
        this.immediateStopped = true;
      },
    };
    dispatchFrom(this, eventObj);
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

  show() {
    this.open = true;
  }

  select() {}

  close() {
    this.open = false;
    const doc = this.ownerDocument;
    for (let cur = doc?.activeElement; cur; cur = cur.parentNode || cur.parentElement) {
      if (cur === this) { doc.activeElement.blur(); break; }
    }
  }

  scrollIntoView() {
    /* layout-free harness: nothing to scroll */
  }

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
    const val = decodeHtmlEntities(m[2] ?? m[3] ?? m[4] ?? "");
    el._attrs.set(name, val);
    if (name === "id" && val && el.ownerDocument?._byId && !el.ownerDocument._byId.has(val)) {
      el.ownerDocument._byId.set(val, el);
    }
  }
}

function popToClosedTag(stack, closeTagUpper) {
  for (let i = stack.length - 1; i >= 1; i--) {
    if (stack[i].tagName === closeTagUpper) {
      stack.length = i;
      return;
    }
  }
}

function pushOpenedTag(stack, match, ownerDocument) {
  const tagLower = match[2].toLowerCase();
  const el = new MiniElement(tagLower, ownerDocument);
  parseAttributesInto(match[3] || "", el);
  stack.at(-1).appendChild(el);
  if (!VOID_TAGS.has(tagLower) && match[4] !== "/") {
    stack.push(el);
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
      stack.at(-1).appendChild(new MiniTextNode(txt, ownerDocument));
    }
    lastIdx = tagRe.lastIndex;
    if (m[1]) {
      popToClosedTag(stack, m[1].toUpperCase());
    } else if (m[2]) {
      pushOpenedTag(stack, m, ownerDocument);
    }
  }
  if (lastIdx < html.length) {
    const tailTxt = decodeHtmlEntities(html.slice(lastIdx));
    stack.at(-1).appendChild(new MiniTextNode(tailTxt, ownerDocument));
  }
  return root.childNodes;
}

class MiniDocument {
  activeElement = null;

  constructor(htmlSource) {
    this._listeners = new Map();
    this._byId = new Map();
    const nodes = parseHtmlFragment(htmlSource, this);
    this.documentElement =
      nodes.find((n) => n.nodeType === 1 && n.tagName === "HTML") ||
      new MiniElement("html", this);
    this.body =
      this.documentElement.querySelector("body") || new MiniElement("body", this);
    this.activeElement = this.body;
  }

  _isConnected(node) {
    for (let cur = node; cur; cur = cur.parentElement) {
      if (cur === this.documentElement) return true;
    }
    return false;
  }

  createElement(tag) {
    return new MiniElement(tag, this);
  }

  createElementNS(_namespace, tag) {
    return new MiniElement(tag, this);
  }

  createTextNode(text) {
    return new MiniTextNode(text, this);
  }

  createRange() {
    return {
      startContainer: null,
      startOffset: 0,
      endContainer: null,
      endOffset: 0,
      setStart(node, offset) {
        this.startContainer = node;
        this.startOffset = offset;
      },
      setEnd(node, offset) {
        this.endContainer = node;
        this.endOffset = offset;
      },
      get collapsed() {
        return this.startContainer === this.endContainer && this.startOffset === this.endOffset;
      },
    };
  }

  getElementById(id) {
    const key = String(id ?? "");
    if (!key) return null;
    const hit = this._byId.get(key);
    if (hit && hit._attrs.get("id") === key && this._isConnected(hit)) return hit;
    this._byId.delete(key);
    const found = this.documentElement.querySelector(`#${key}`);
    if (found) this._byId.set(key, found);
    return found;
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

  removeEventListener(type, fn) {
    this._listeners.set(type, (this._listeners.get(type) || []).filter((f) => f !== fn));
  }

  dispatchEvent(ev) {
    const fns = [...(this._listeners.get(ev.type) || [])];
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

// Desktop shell stub. `tauriInternals` installs a raw `__TAURI_INTERNALS__` object (bridge tests);
// `tauriInvoke(command, { payload })` is a per-engine-command convenience handler that is routed
// through the shell's single `ipc_dispatch` command exactly like the real WebView bridge.
// Token and header of the shell's command check (`shell_caller.rs`): the stub shell refuses a
// call that does not carry the window's token, like the real one.
export const SHELL_TOKEN = "test-shell-token-0123456789abcdef";
export const SHELL_TOKEN_HEADER = "x-easyoffice-shell";
export function requireShellToken(options) {
  if (options?.headers?.[SHELL_TOKEN_HEADER] !== SHELL_TOKEN) {
    throw shellError("이 호출은 셸 창의 문서에서 온 요청이 아니어서 거부했습니다.");
  }
}

function resolveShellInternals(options) {
  if (options.tauriInternals) return options.tauriInternals;
  if (!options.tauriInvoke && !options.shellCommands) return undefined;
  return {
    invoke: async (shellCommand, args = {}, callOptions) => {
      requireShellToken(callOptions);
      if (options.shellCommands && typeof options.shellCommands[shellCommand] === "function") {
        return options.shellCommands[shellCommand](args);
      }
      if (shellCommand !== "ipc_dispatch" || !options.tauriInvoke) {
        throw shellError("등록되지 않은 데스크톱 셸 명령: " + shellCommand);
      }
      return options.tauriInvoke(args.command, { payload: args.payload });
    },
  };
}

export async function createWorkspaceHarness(options = {}) {
  const document = new MiniDocument(options.transformHtml ? options.transformHtml(INDEX_HTML_RAW) : INDEX_HTML_RAW);
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

  let defaultProtocol = "app:";
  if (options.protocol) {
    defaultProtocol = options.protocol;
  } else if (options.httpFetch) {
    defaultProtocol = "http:";
  } else if (options.disconnectedHost) {
    defaultProtocol = "file:";
  }

  const hostEnv = {
    document,
    navigator: { platform: options.platform || "Win32", clipboard: options.clipboard },
    localStorage: options.localStorage,
    confirm: options.confirm,
    location: { protocol: defaultProtocol },
    fetch: options.httpFetch || undefined,
    atob: (b64) => Buffer.from(b64, "base64").toString("binary"),
    btoa: (bin) => Buffer.from(bin, "binary").toString("base64"),
    URL: urlShim,
    Blob: globalThis.Blob,
    matchMedia: options.matchMedia,
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
    removeEventListener: (type, fn) => {
      const fns = windowListeners.get(type) || [];
      const idx = fns.indexOf(fn);
      if (idx >= 0) fns.splice(idx, 1);
    },
    __TAURI_INTERNALS__: resolveShellInternals(options),
    ...(options.shellToken === null ? {} : { __EASYOFFICE_SHELL_TOKEN__: options.shellToken ?? SHELL_TOKEN }),
  };
  if (options.search !== undefined) hostEnv.location.search = options.search;
  if (options.open) hostEnv.open = options.open;
  if (options.close) hostEnv.close = options.close;
  if (options.sessionStorage) hostEnv.sessionStorage = options.sessionStorage;
  document.defaultView = hostEnv;

  Object.assign(hostEnv, options.hostEnvExtras || {});

  let offlineHandlers = null;
  function getDefaultOfflineHandlers() {
    if (!offlineHandlers) {
      const cat = hostEnv.__EASYOFFICE_RIBBON_CATALOG__;
      offlineHandlers = hostEnv.__EASYOFFICE_CREATE_OFFLINE_RPC__({
        getActiveModule: () => {
          const activeBtn = document.querySelector(".nav-btn.active, .tab-item.active");
          return activeBtn?.dataset?.module || "DOC";
        },
        ribbonCategoryTools: cat.RIBBON_CATEGORY_TOOLS,
        ribbonStyleLabels: cat.RIBBON_STYLE_LABELS,
        contextualToolBadges: cat.CONTEXTUAL_TOOL_BADGES,
        moduleSpecs: cat.MODULE_SPECS,
        localRibbonState: cat.createDefaultRibbonState(),
      });
    }
    return offlineHandlers;
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

  const modulesToLoad = [...PROD_PRE_SCRIPTS];
  if (!options.disconnectedHost) {
    modulesToLoad.push(...TEST_SUPPORT_MODULES);
  }
  modulesToLoad.push(...PROD_APP_SCRIPTS);

  if (!options.disconnectedHost && !options.tauriInvoke && !options.tauriInternals && !options.httpFetch) {
    hostEnv.__EASYOFFICE_TEST_IPC__ =
      options.testIpc ||
      (async (command, payload) => {
        const handlers = getDefaultOfflineHandlers();
        const fn = handlers[command];
        if (typeof fn !== "function") {
          throw new TypeError("알 수 없는 IPC 명령: " + command);
        }
        return await fn(payload);
      });
  }

  globalThis.__EASYOFFICE_HOST_ENV__ = hostEnv;
  try {
    for (const mod of modulesToLoad) {
      runCachedModule(mod.resolvedPath);
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
    __EASYOFFICE_INVOKE_BACKEND__: hostEnv.__EASYOFFICE_INVOKE_BACKEND__,
    __EASYOFFICE_DESKTOP_BRIDGE__: hostEnv.__EASYOFFICE_DESKTOP_BRIDGE__,
    __EASYOFFICE_WORKSPACE_API__: hostEnv.__EASYOFFICE_WORKSPACE_API__,
    __EASYOFFICE_FILE_OPEN_UTILS__: hostEnv.__EASYOFFICE_FILE_OPEN_UTILS__,
    __EASYOFFICE_DOC_RUNS__: hostEnv.__EASYOFFICE_DOC_RUNS__,
    __EASYOFFICE_COPILOT_CATALOG__: hostEnv.__EASYOFFICE_COPILOT_CATALOG__,
    dispatchWindowKeydown: (ev) => {
      const eventObj = {
        type: "keydown",
        key: ev.key,
        code: ev.code || "",
        target: ev.target || document.activeElement,
        metaKey: Boolean(ev.metaKey),
        ctrlKey: Boolean(ev.ctrlKey),
        altKey: Boolean(ev.altKey),
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
    getWindowListenerCount: (type) => (windowListeners.get(type) || []).length,
    dispatchWindowEvent: (type, init = {}) => {
      const eventObj = {
        ...init,
        type,
        returnValue: undefined,
        defaultPrevented: false,
        preventDefault() {
          this.defaultPrevented = true;
        },
      };
      for (const fn of [...(windowListeners.get(type) || [])]) {
        const ret = fn(eventObj);
        if (typeof ret === "string") eventObj.returnValue = ret;
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
    moveFocusForClick(document, focusable);
    el.click();
  }

  function advanceTimers(ms = 2000) {
    const targetMs = virtualNowMs + Math.max(0, Number(ms) || 0);
    while (true) {
      const next = pickNextDueTimer(delayedTimers, cancelledTimers, targetMs);
      if (!next) break;
      delayedTimers.delete(next.id);
      virtualNowMs = next.entry.dueAt;
      if (!cancelledTimers.has(next.id)) {
        next.entry.fn();
      }
    }
    virtualNowMs = targetMs;
  }

  return { document, sandbox, downloads, revokedUrls, userClick, advanceTimers, host: hostEnv };
}

function moveFocusForClick(document, focusable) {
  const active = document.activeElement;
  if (active && active !== document.body && active !== focusable) {
    active.blur();
  }
  if (focusable && document.activeElement !== focusable) {
    focusable.focus();
  }
}

// Earliest due (then lowest sequence) pending timer at or before targetMs; purges cancelled ones.
function pickNextDueTimer(delayedTimers, cancelledTimers, targetMs) {
  let best = null;
  for (const [id, entry] of delayedTimers.entries()) {
    if (cancelledTimers.has(id)) {
      delayedTimers.delete(id);
      continue;
    }
    if (entry.dueAt > targetMs) continue;
    const earlier =
      !best ||
      entry.dueAt < best.entry.dueAt ||
      (entry.dueAt === best.entry.dueAt && entry.seq < best.entry.seq);
    if (earlier) best = { id, entry };
  }
  return best;
}

export async function createWebUiDom(options = {}) {
  return createWorkspaceHarness(options);
}
