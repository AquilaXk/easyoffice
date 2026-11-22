import { deflateSync } from "node:zlib";
import { createWorkspaceHarness, flushAsyncTurns } from "../webui-dom-harness.mjs";
import { switchModule } from "./modules.mjs";

// A document editor with a caret and a clipboard for the paste tests (doc-paste.js). The DOM of the harness has no HTML
// parser object and no real ranges, so this file gives it both: `DOMParser` builds an inert tree from the harness's own
// markup reader, and `PasteRange` is a range that can delete its contents.

const tagOf = (node) => String(node.tagName || "").toLowerCase();

function removeBetween(parent, from, to) {
  for (let i = to - 1; i >= from; i -= 1) parent.childNodes[i].remove();
}

// Nodes of `container` from the child at `offset` to the end, or from the start to the child at `offset`.
function cutPartial(node, offset, keepBefore) {
  if (node.nodeType === 3) node.data = keepBefore ? node.data.slice(0, offset) : node.data.slice(offset);
  else if (keepBefore) removeBetween(node, offset, node.childNodes.length);
  else removeBetween(node, 0, offset);
}

function ancestors(node) {
  const chain = [];
  for (let cur = node; cur; cur = cur.parentNode) chain.push(cur);
  return chain;
}

export class PasteRange {
  constructor(startContainer, startOffset, endContainer = startContainer, endOffset = startOffset) {
    Object.assign(this, { startContainer, startOffset, endContainer, endOffset });
  }

  get collapsed() {
    return this.startContainer === this.endContainer && this.startOffset === this.endOffset;
  }

  setStart(node, offset) {
    this.startContainer = node;
    this.startOffset = offset;
  }

  setEnd(node, offset) {
    this.endContainer = node;
    this.endOffset = offset;
  }

  // The contents between the ends, as the DOM does it: the boundary nodes are cut, what lies between them goes.
  deleteContents() {
    const { startContainer: sc, startOffset: so, endContainer: ec, endOffset: eo } = this;
    if (sc === ec) {
      if (sc.nodeType === 3) sc.data = sc.data.slice(0, so) + sc.data.slice(eo);
      else removeBetween(sc, so, eo);
    } else {
      const startChain = ancestors(sc);
      const endChain = ancestors(ec);
      const common = startChain.find((node) => endChain.includes(node));
      // The child of the common ancestor on each side, or null when the boundary is on the common ancestor itself.
      const startBranch = sc === common ? null : startChain[startChain.indexOf(common) - 1];
      const endBranch = ec === common ? null : endChain[endChain.indexOf(common) - 1];
      if (startBranch) {
        cutPartial(sc, so, true);
        for (let cur = sc; cur !== startBranch; cur = cur.parentNode) removeBetween(cur.parentNode, cur.parentNode.childNodes.indexOf(cur) + 1, cur.parentNode.childNodes.length);
      }
      if (endBranch) {
        cutPartial(ec, eo, false);
        for (let cur = ec; cur !== endBranch; cur = cur.parentNode) removeBetween(cur.parentNode, 0, cur.parentNode.childNodes.indexOf(cur));
      }
      const kids = common.childNodes;
      const from = startBranch ? kids.indexOf(startBranch) + 1 : so;
      const to = endBranch ? kids.indexOf(endBranch) : eo;
      removeBetween(common, from, to);
    }
    // The range collapses as the DOM says: at the start when the start node holds the end node, else after the first
    // ancestor of the start node that does.
    if (sc === ec || ancestors(ec).includes(sc)) {
      this.endContainer = sc;
      this.endOffset = so;
    } else {
      let reference = sc;
      while (reference.parentNode && !ancestors(ec).includes(reference.parentNode)) reference = reference.parentNode;
      this.startContainer = this.endContainer = reference.parentNode;
      this.startOffset = this.endOffset = reference.parentNode.childNodes.indexOf(reference) + 1;
    }
  }
}

export function createPasteSelection() {
  return {
    _range: null,
    get rangeCount() {
      return this._range ? 1 : 0;
    },
    getRangeAt() {
      return this._range;
    },
    removeAllRanges() {
      this._range = null;
    },
    addRange(range) {
      this._range = range;
    },
  };
}

export const parserCalls = [];

// The paste data of a clipboard event.
export function clipboard({ html = "", text = "", rtf = "", files = [] } = {}) {
  const data = { "text/html": html, "text/plain": text, "text/rtf": rtf };
  return {
    getData: (type) => data[type] ?? "",
    items: files.map((file) => ({ kind: "file", type: file.type, getAsFile: () => file })),
  };
}

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let i = 0; i < 256; i += 1) {
    let c = i;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[i] = c >>> 0;
  }
  return t;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i += 1) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const out = Buffer.alloc(12 + data.length);
  out.writeUInt32BE(data.length, 0);
  out.write(type, 4, 4, "ascii");
  data.copy(out, 8);
  out.writeUInt32BE(crc32(out.subarray(4, 8 + data.length)), 8 + data.length);
  return out;
}

export function makeTestPng(width = 2, height = 3) {
  const sig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  const raw = Buffer.alloc(height * (1 + width * 4));
  const idat = deflateSync(raw);
  return new Uint8Array(Buffer.concat([sig, pngChunk("IHDR", ihdr), pngChunk("IDAT", idat), pngChunk("IEND", Buffer.alloc(0))]));
}

export function makeTestGif(width = 4, height = 5) {
  const head = Buffer.from("GIF89a", "ascii");
  const lsd = Buffer.alloc(7);
  lsd.writeUInt16LE(width, 0);
  lsd.writeUInt16LE(height, 2);
  lsd[4] = 0x80;
  const body = Buffer.from([0, 0, 0, 255, 255, 255, 0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x4c, 0x01, 0, 0x3b]);
  return new Uint8Array(Buffer.concat([head, lsd, body]));
}

class TestBlob {
  constructor(parts = [], opts = {}) {
    const bufs = parts.map((p) => (typeof p === "string" ? Buffer.from(p, "utf8") : Buffer.from(p)));
    this._bytes = new Uint8Array(Buffer.concat(bufs));
    this.type = opts.type || "";
    this.size = this._bytes.byteLength;
  }
  async arrayBuffer() {
    return this._bytes.buffer.slice(this._bytes.byteOffset, this._bytes.byteOffset + this._bytes.byteLength);
  }
  async text() {
    return Buffer.from(this._bytes).toString("utf8");
  }
}

function measureBytes(bytes, mime = "") {
  if (bytes.byteLength >= 10 && bytes[0] === 0x47 && bytes[1] === 0x49 && bytes[2] === 0x46) {
    return { width: bytes[6] | (bytes[7] << 8), height: bytes[8] | (bytes[9] << 8) };
  }
  if (mime.includes("svg") || Buffer.from(bytes.subarray(0, 32)).toString("utf8").includes("<svg")) {
    const s = Buffer.from(bytes).toString("utf8");
    const w = Number(/\bwidth\s*=\s*["']?(\d+(?:\.\d+)?)/i.exec(s)?.[1] ?? 12);
    const h = Number(/\bheight\s*=\s*["']?(\d+(?:\.\d+)?)/i.exec(s)?.[1] ?? 8);
    return { width: Math.max(1, Math.round(w)), height: Math.max(1, Math.round(h)) };
  }
  return { width: 2, height: 2 };
}

class TestImage {
  set src(val) {
    const comma = String(val || "").indexOf(",");
    const header = comma >= 0 ? val.slice(0, comma).toLowerCase() : "";
    const body = comma >= 0 ? val.slice(comma + 1) : "";
    let ok = false;
    if (header.startsWith("data:image/gif;base64")) {
      const u8 = new Uint8Array(Buffer.from(body, "base64"));
      ok = u8.byteLength >= 26 && u8[0] === 0x47 && u8[1] === 0x49 && u8[2] === 0x46;
      if (ok) Object.assign(this, measureBytes(u8, "image/gif"));
    } else if (header.startsWith("data:image/svg+xml")) {
      const svg = header.includes(";base64") ? Buffer.from(body, "base64").toString("utf8") : decodeURIComponent(body);
      ok = svg.includes("<svg");
      if (ok) Object.assign(this, measureBytes(Buffer.from(svg, "utf8"), "image/svg+xml"));
    }
    queueMicrotask(() => (ok ? this.onload?.() : this.onerror?.()));
  }
}

// Opens the document editor with `paragraphs` (strings) and returns helpers. `markup` replaces them with the editor's HTML.
export async function openPasteEditor({ paragraphs = [], markup = null, module = "DOC", parserOptions = {}, clipboard: systemClipboard = null } = {}) {
  const selection = createPasteSelection();
  const holder = { document: null };
  class TestDOMParser {
    parseFromString(html, type) {
      parserCalls.push({ html, type });
      const body = holder.document.createElement("body");
      // The harness's markup reader does not know a colon in a tag name (`o:p`); a real parser keeps it as part of the name.
      // It also does not know the void elements beyond its own short list.
      body.innerHTML = html
        .replace(/<(\/?)([a-z0-9]+):([a-z0-9]+)/gi, "<$1$2-$3")
        .replace(/<(base|embed|source|track|wbr|param|area)\b([^>]*?)\/?>/gi, "<$1$2></$1>");
      return { body, documentElement: body };
    }
  }
  const harness = await createWorkspaceHarness({
    selection,
    clipboard: systemClipboard,
    hostEnvExtras: {
      DOMParser: TestDOMParser,
      Blob: TestBlob,
      Image: TestImage,
      createImageBitmap: async (blob) => {
        const bytes = blob?._bytes ?? new Uint8Array(await blob.arrayBuffer());
        const dims = measureBytes(bytes, blob?.type || "");
        return { ...dims, close() {} };
      },
      ...parserOptions,
    },
  });
  holder.document = harness.document;
  const origCreateElement = harness.document.createElement.bind(harness.document);
  harness.document.createElement = (tagName) => {
    const el = origCreateElement(tagName);
    if (String(tagName).toLowerCase() === "canvas") {
      el.width = 0;
      el.height = 0;
      el.getContext = () => ({ drawImage() {} });
      el.toBlob = (done) => {
        const png = makeTestPng(Math.max(1, el.width || 1), Math.max(1, el.height || 1));
        done(new TestBlob([png], { type: "image/png" }));
      };
    }
    return el;
  };
  switchModule(harness.document, module);
  await flushAsyncTurns(10);
  const { document } = harness;
  const editor = document.getElementById("doc-editor");
  editor.replaceChildren();
  if (markup !== null) editor.innerHTML = markup;
  else {
    for (const text of paragraphs) {
      const p = document.createElement("p");
      p.appendChild(document.createTextNode(text));
      editor.appendChild(p);
    }
  }
  editor.focus();
  const caretAt = (node, offset = 0) => selection.addRange(new PasteRange(node, offset));
  const select = (sc, so, ec, eo) => selection.addRange(new PasteRange(sc, so, ec, eo));
  const events = [];
  const paste = async (data, init = {}) => {
    const event = { type: "paste", clipboardData: data, ...init };
    const delivered = editor.dispatchEvent(event);
    await flushAsyncTurns(20);
    events.push(delivered);
    return delivered;
  };
  const feedback = () => document.getElementById("status-command-feedback")?.textContent ?? "";
  const history = () => harness.host.__EASYOFFICE_WORKSPACE_HISTORY__;
  const undo = async () => {
    harness.sandbox.dispatchWindowKeydown({ key: "z", ctrlKey: true });
    await flushAsyncTurns(20);
  };
  const caret = () => {
    const r = selection.getRangeAt();
    return r ? { node: r.startContainer, offset: r.startOffset, collapsed: r.collapsed } : null;
  };
  const blocks = () => editor.childNodes.filter((n) => n.nodeType === 1);
  return { ...harness, editor, selection, caretAt, select, paste, feedback, history, undo, caret, blocks, steps: () => history().sizes().undo };
}

// The structure of a node as compact text for assertions: `p[abc]`, `p[a,b(bold)]`, `ul[li[x]]`, `table[tr[td[a],td[b]]]`.
export function shape(node) {
  if (node.nodeType === 3) return node.data;
  const tag = tagOf(node);
  if (tag === "br") return "<br>";
  const inner = node.childNodes.map(shape).join("|");
  const marks = [];
  if (node.dataset?.eoBold === "true") marks.push("b");
  if (node.dataset?.eoItalic === "true") marks.push("i");
  if (node.dataset?.eoUnderline === "true") marks.push("u");
  if (node.dataset?.eoSize) marks.push("s" + node.dataset.eoSize);
  if (node.dataset?.eoColor) marks.push("c" + node.dataset.eoColor);
  if (node.getAttribute?.("href")) marks.push("@" + node.getAttribute("href"));
  if (node.style?.textAlign) marks.push("al:" + node.style.textAlign);
  if (node.dataset?.eoListLevel) marks.push("L" + node.dataset.eoListLevel);
  if (node.getAttribute?.("colspan")) marks.push("cs" + node.getAttribute("colspan"));
  if (node.getAttribute?.("rowspan")) marks.push("rs" + node.getAttribute("rowspan"));
  return `${tag}${marks.length ? "(" + marks.join(",") + ")" : ""}[${inner}]`;
}

export const shapes = (editor) => editor.childNodes.filter((n) => n.nodeType === 1).map(shape);

// Every element below `node` (including nested ones).
export function allElements(node, out = []) {
  for (const child of node.childNodes) {
    if (child.nodeType === 1) {
      out.push(child);
      allElements(child, out);
    }
  }
  return out;
}
