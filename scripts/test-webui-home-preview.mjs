import test from "node:test";
import assert from "node:assert/strict";
import { loadUiModule } from "./test-support/load-ui-module.mjs";
import { readText } from "./test-support/read-text.mjs";

// Start-window previews (PD-106, PD-142): the shell returns the picture inside a file, or else the
// drawn first page (a PNG), as Base64; the page decodes it with createImageBitmap and draws it into
// a canvas on the card. Any failure keeps the
// skeleton. The DOM is a small fake; the shell and the decoder are functions.

const HOME_JS = new URL("../crates/easyoffice-app/ui/home-screen.js", import.meta.url);
const UI = (name) => readText(new URL(`../crates/easyoffice-app/ui/${name}`, import.meta.url));
const PNG_B64 = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13]).toString("base64");
const MAX_BASE64_CHARS = Math.ceil((1024 * 1024) / 3) * 4;
const GIF_B64 = Buffer.from("GIF89a\x01\x00\x01\x00\x00\x00\x00;", "latin1").toString("base64");

class FakeEl {
  constructor(tag) {
    this.tag = tag; this.children = []; this.parent = null; this.className = ""; this.dataset = {};
    this.attrs = new Map(); this.listeners = new Map(); this.textContent = ""; this.hidden = false;
    const classes = new Set();
    this.classList = {
      add: (c) => classes.add(c), contains: (c) => classes.has(c) || this.className.split(" ").includes(c),
      toggle: (c, force) => (force ? classes.add(c) : classes.delete(c)),
    };
    this.drawn = [];
  }
  get firstChild() { return this.children[0] ?? null; }
  appendChild(kid) { kid.parent = this; this.children.push(kid); return kid; }
  remove() {
    if (this.parent) {
      this.parent.children = this.parent.children.filter((c) => c !== this);
    }
    this.parent = null;
  }
  setAttribute(k, v) { this.attrs.set(k, v); }
  addEventListener(t, fn) { this.listeners.set(t, fn); }
  getContext() { return this.noContext ? null : { drawImage: (...a) => this.drawn.push(a) }; }
  all(pred, out = []) {
    for (const c of this.children) {
      if (pred(c)) {
        out.push(c);
      }
      c.all(pred, out);
    }
    return out;
  }
}

function setup({ loadPreview, bitmap = () => ({ width: 8, height: 6, close() { this.closed = true; } }), noDecoder = false } = {}) {
  const document = { createElement: (tag) => new FakeEl(tag) };
  const blobs = [];
  const timers = [];
  const env = {
    setTimeout: (fn) => { timers.push(fn); return timers.length; },
    atob: (s) => {
      if (!/^[A-Za-z0-9+/]*={0,2}$/.test(s)) {
        throw new Error("InvalidCharacterError");
      }
      return Buffer.from(s, "base64").toString("latin1");
    },
    Blob: function FakeBlob(parts, opts) {
      blobs.push({ bytes: [...parts[0]], type: opts.type });
    },
    createImageBitmap: noDecoder ? undefined : async (blob) => { const made = bitmap(blob); return made; },
  };
  if (noDecoder) delete env.createImageBitmap;
  loadUiModule(HOME_JS, env);
  const home = env.__EASYOFFICE_CREATE_HOME_SCREEN__({ document, env, onOpenRecent() {}, loadPreview });
  const settle = (turns) => (turns === 0 ? Promise.resolve() : new Promise((r) => setImmediate(r)).then(() => settle(turns - 1)));
  const run = async () => {
    if (timers.length === 0) return;
    timers.shift()();
    await settle(20);
    await run();
  };
  const boxes = () => home.root.all((c) => c.className === "home-recent-preview" || c.className === "home-recent-preview is-loaded");
  const canvases = () => home.root.all((c) => c.tag === "canvas");
  return { home, run, boxes, canvases, blobs, timers };
}

const entries = (n) => Array.from({ length: n }, (_, i) => ({ id: `${i}`.padStart(16, "0"), name: `문서${i}.hwp`, kind: "HWP" }));

test("previews load one after the other after the cards are drawn, and the canvas replaces the skeleton", async () => {
  const asked = [];
  const t = setup({ loadPreview: async (id) => { asked.push(id); return { mime: "image/gif", base64: GIF_B64 }; } });
  t.home.setRecents(entries(3));
  assert.deepEqual(asked, [], "nothing is requested while the cards are being built");
  assert.equal(t.canvases().length, 0);
  await t.run();
  assert.deepEqual(asked, entries(3).map((e) => e.id));
  assert.equal(t.canvases().length, 3);
  const [first] = t.canvases();
  assert.deepEqual([first.width, first.height, first.drawn.length], [8, 6, 1]);
  assert.equal(t.blobs[0].type, "image/gif");
  assert.equal(String.fromCodePoint(...t.blobs[0].bytes.slice(0, 6)), "GIF89a");
  assert.ok(first.parent.classList.contains("is-loaded"));
  assert.equal(first.parent.children.length, 1, "the skeleton lines are gone");
});

test("every failure keeps the skeleton of that card and the others still load", async () => {
  const replies = [null, "boom", { mime: "image/svg+xml", base64: GIF_B64 }, { mime: "image/gif", base64: "!!!" }, { mime: "image/gif", base64: "" }, { mime: "image/gif", base64: GIF_B64 }];
  let call = 0;
  const t = setup({
    loadPreview: async () => {
      const reply = replies[call];
      call += 1;
      if (reply === "boom") {
        throw new Error("denied");
      }
      return reply;
    },
  });
  t.home.setRecents(entries(5));
  await t.run();
  assert.equal(call, 5);
  assert.equal(t.canvases().length, 0, "no reply, an error, a wrong type and bad Base64 draw nothing");
  assert.ok(t.boxes().every((box) => box.children.length > 1), "the skeletons stay");
});

test("a decode error, a missing 2d context or an empty bitmap leaves the skeleton", async () => {
  const reply = async () => ({ mime: "image/gif", base64: GIF_B64 });
  const failing = setup({ loadPreview: reply, bitmap: () => { throw new Error("cannot decode"); } });
  failing.home.setRecents(entries(2));
  await failing.run();
  assert.equal(failing.canvases().length, 0);
  const empty = setup({ loadPreview: reply, bitmap: () => ({ width: 0, height: 0, close() {} }) });
  empty.home.setRecents(entries(1));
  await empty.run();
  assert.equal(empty.canvases().length, 0);
});

test("a drawn first page up to the shell's 1 MiB limit is shown, one byte-group over is refused undecoded", async () => {
  const atLimit = "A".repeat(MAX_BASE64_CHARS);
  const shown = setup({ loadPreview: async () => ({ mime: "image/png", base64: atLimit }) });
  shown.home.setRecents(entries(1));
  await shown.run();
  assert.equal(shown.blobs.length, 1, "a reply at the limit is decoded");
  assert.equal(shown.canvases().length, 1);
  const t = setup({ loadPreview: async () => ({ mime: "image/png", base64: atLimit + "AAAA" }) });
  t.home.setRecents(entries(1));
  await t.run();
  assert.equal(t.blobs.length, 0, "an oversized reply is refused before it is decoded");
  assert.equal(t.canvases().length, 0);
});

test("the PNG of a drawn first page is decoded like an embedded picture and fills the card", async () => {
  const t = setup({ loadPreview: async () => ({ mime: "image/png", base64: PNG_B64 }) });
  t.home.setRecents(entries(2));
  await t.run();
  assert.equal(t.canvases().length, 2);
  assert.equal(t.blobs[0].type, "image/png");
  assert.deepEqual(t.blobs[0].bytes.slice(0, 4), [0x89, 0x50, 0x4e, 0x47]);
  assert.ok(t.canvases()[0].parent.classList.contains("is-loaded"));
});

test("without a shell command or a decoder nothing is requested; a newer list cancels the run", async () => {
  const asked = [];
  const none = setup({ loadPreview: null });
  none.home.setRecents(entries(2));
  assert.equal(none.timers.length, 0);
  const noDecoder = setup({ loadPreview: async (id) => { asked.push(id); return null; }, noDecoder: true });
  noDecoder.home.setRecents(entries(2));
  assert.equal(noDecoder.timers.length, 0);
  assert.deepEqual(asked, []);
  const t = setup({ loadPreview: async (id) => { asked.push(id); return null; } });
  t.home.setRecents(entries(2));
  t.home.setRecents([{ id: "ffffffffffffffff", name: "새것.hwp", kind: "HWP" }]);
  await t.run();
  assert.deepEqual(asked, ["ffffffffffffffff"], "the first run was cancelled before it asked");
});

test("the window bridge passes the opaque id only, the command is granted, and the CSS has no colours", () => {
  const bridge = UI("window-bridge.js");
  assert.ok(bridge.includes('shell("recentPreview", { id })'));
  assert.ok(UI("desktop-bridge.js").includes('recentPreview: "get_recent_preview"'));
  const capability = readText(new URL("../crates/easyoffice-desktop/capabilities/main-window.json", import.meta.url));
  assert.ok(capability.includes('"allow-get-recent-preview"'));
  const home = UI("home-screen.js");
  assert.ok(!/\.src\s*=|createObjectURL|data:image|innerHTML/.test(home), "no URL is made for the picture");
  const css = UI("home.css");
  const block = css.slice(css.indexOf(".home-recent-preview.is-loaded"), css.indexOf(".home-drop-hint {"));
  assert.ok(block.includes(".home-preview-canvas") && !/#[0-9a-fA-F]{3,8}\b|rgb/.test(block));
});
