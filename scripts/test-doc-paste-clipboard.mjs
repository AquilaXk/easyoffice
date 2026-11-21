import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { loadUiModule } from "./test-support/load-ui-module.mjs";

// The clean-up of pasted office HTML (doc-paste-sanitize.js) and the pictures of an RTF clipboard
// (doc-clipboard-images.js): the cases of the reference code, and the hostile inputs it has no bound for.

const UI = (name) => fileURLToPath(new URL(`../crates/easyoffice-app/ui/${name}`, import.meta.url));
const env = {
  __EASYOFFICE_SHEET_UTILS__: { bytesToBase64: (bytes) => Buffer.from(bytes).toString("base64") },
};
loadUiModule(UI("doc-paste-sanitize.js"), env);
loadUiModule(UI("doc-clipboard-images.js"), env);
const { sanitizeOfficeHtml } = env.__EASYOFFICE_DOC_PASTE_SANITIZE__;
const images = env.__EASYOFFICE_DOC_CLIPBOARD_IMAGES__;

const timed = (fn) => {
  const start = process.hrtime.bigint();
  const result = fn();
  return { result, ms: Number(process.hrtime.bigint() - start) / 1e6 };
};

// ---- sanitizeOfficeHtml --------------------------------------------------------------------------------------

test("comments, conditional comments and the fragment markers of the clipboard format go", () => {
  const html = "<html><body><!--StartFragment--><p>a</p><!--[if gte mso 9]><xml><o:x/></xml><![endif]--><p>b</p><!--EndFragment--></body></html>";
  assert.equal(sanitizeOfficeHtml(html), "<html><body><p>a</p><p>b</p></body></html>");
});

test("overlapping comment openers leave no marker behind", () => {
  assert.equal(sanitizeOfficeHtml("x<!--<!-- -->y-->z"), "xy-->z");
  assert.equal(sanitizeOfficeHtml("x<!--[data-hwpjson]{\"a\":1}-->y"), "xy");
});

test("a style, script or xml element goes with its content, also with a space in its end tag", () => {
  assert.equal(sanitizeOfficeHtml("a<style>p{color:red}</style>b"), "ab");
  assert.equal(sanitizeOfficeHtml("a<script>alert(1)</script >b"), "ab");
  assert.equal(sanitizeOfficeHtml("a<SCRIPT type=x>alert(1)</script\n>b"), "ab");
  assert.equal(sanitizeOfficeHtml("a<xml><w:x>1</w:x></xml>b"), "ab");
  assert.equal(sanitizeOfficeHtml("a<xml/>b"), "ab");
});

test("a similar tag name is kept: <styles> is not <style>", () => {
  assert.equal(sanitizeOfficeHtml("<styles>x</styles>"), "<styles>x</styles>");
  assert.equal(sanitizeOfficeHtml("<scripts>x</scripts>"), "<scripts>x</scripts>");
});

test("downlevel-revealed conditional blocks go", () => {
  assert.equal(sanitizeOfficeHtml("<p><![if !supportLists]><span>1.</span><![endif]>item</p>"), "<p>item</p>");
});

test("a start marker without an end marker takes the rest of the text (fail-closed)", () => {
  assert.equal(sanitizeOfficeHtml("a<!-- never closed <script>b"), "a");
  assert.equal(sanitizeOfficeHtml("a<script>never closed"), "a");
  assert.equal(sanitizeOfficeHtml("a<style"), "a");
  assert.equal(sanitizeOfficeHtml("a<![if never"), "a");
  assert.equal(sanitizeOfficeHtml("a<p>b</p><script"), "a<p>b</p>");
});

test("no script, comment or conditional start survives a mix of hostile fragments", () => {
  const parts = ["<!--", "-->", "<script>", "</script>", "</script >", "<style>", "</style>", "<![if x]>", "<![endif]>", "<", ">", "a", "<xml>", "</xml>", "<p>"];
  let seed = 7;
  const next = () => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed;
  };
  for (let round = 0; round < 400; round += 1) {
    const html = Array.from({ length: 12 }, () => parts[next() % parts.length]).join("");
    const out = sanitizeOfficeHtml(html).toLowerCase();
    assert.ok(!out.includes("<script") && !out.includes("<!--") && !out.includes("<![if") && !out.includes("<style") && !out.includes("<xml"), JSON.stringify([html, out]));
  }
});

test("the clean-up is linear: a megabyte of nested openers and of unfinished end tags", () => {
  for (const unit of ["<!--<!--", "<script></script x", "<![if <![if", "<", "<style></style x"]) {
    const html = unit.repeat(Math.ceil((1024 * 1024) / unit.length));
    const { ms } = timed(() => sanitizeOfficeHtml(html));
    assert.ok(ms < 1500, `${JSON.stringify(unit)} took ${ms} ms`);
  }
});

// ---- the pictures of the RTF ---------------------------------------------------------------------------------

const hex = (bytes) => Buffer.from(bytes).toString("hex");
const PNG = Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, ...Array.from({ length: 24 }, (_, i) => i)]);
const JPG = Uint8Array.from([0xff, 0xd8, 0xff, 0xe0, ...Array.from({ length: 28 }, (_, i) => 100 + i)]);
const pict = (control, bytes) => `{\\pict\\picw100\\pich100\\picwgoal1000${control}\r\n${hex(bytes)}}`;

test("RTF pictures come out in document order with their type; a group that is no picture keeps its place", () => {
  const rtf = `{\\rtf1 ${pict("\\pngblip", PNG)} text ${pict("\\jpegblip", JPG)} ${pict("\\dibitmap0", PNG)} ${pict("\\wmetafile8", JPG)}}`;
  const found = images.extractRtfPictures(rtf);
  assert.deepEqual(found.map((p) => p.mime), ["image/png", "image/jpeg", "", "image/emf"]);
  assert.deepEqual(Array.from(found[0].bytes), Array.from(PNG));
  assert.deepEqual(Array.from(found[1].bytes), Array.from(JPG));
  assert.equal(found[2].bytes.length, 0);
});

test("control words are not read as data: letters, minus and digits after a backslash", () => {
  const rtf = `{\\pict\\picw3439\\pich-1234\\bliptag-77\\pngblip abcdef${hex(PNG)}}`;
  const [only] = images.extractRtfPictures(rtf);
  // `abcdef` is data (six hex digits): three bytes in front of the PNG
  assert.deepEqual(Array.from(only.bytes), [0xab, 0xcd, 0xef, ...PNG]);
});

test("a group with fewer than 32 hex digits keeps its place with no bytes, an odd digit at the end is dropped", () => {
  const short = images.extractRtfPictures("{\\pict\\pngblip 0123456789abcdef}");
  assert.deepEqual(short.map((p) => p.bytes.length), [0]);
  const [odd] = images.extractRtfPictures(`{\\pict\\pngblip ${hex(PNG)}f}`);
  assert.equal(odd.bytes.length, PNG.length);
});

test("at most 32 pictures are read, and a start with no end does not make the scan quadratic", () => {
  const many = Array.from({ length: 40 }, () => pict("\\pngblip", PNG)).join("");
  assert.equal(images.extractRtfPictures(`{\\rtf1 ${many}}`).length, images.LIMITS.MAX_RTF_PICTURES);
  const open = "\\pict ".repeat(200000);
  const { result, ms } = timed(() => images.extractRtfPictures(open));
  assert.equal(result.length, 0);
  assert.ok(ms < 1000, `took ${ms} ms`);
});

test("the HTML pictures that a page cannot open take the RTF pictures in order", async () => {
  const rtf = `{\\rtf1 ${pict("\\pngblip", PNG)}${pict("\\jpegblip", JPG)}}`;
  const html = '<p><img src="file:///C:\\a\\clip_image001.png" width=1><img src=\'data:image/gif;base64,AAAA\'><img src="file:///C:\\a\\clip_image002.jpg"></p>';
  assert.equal(images.needsRtfImageInlining(html, rtf), true);
  const out = await images.inlineOfficeClipboardImages(html, rtf, {});
  assert.match(out, new RegExp(`<img src="data:image/png;base64,${Buffer.from(PNG).toString("base64")}" width=1>`));
  assert.ok(out.includes("data:image/gif;base64,AAAA"), "a data picture stays");
  assert.match(out, new RegExp(`<img src="data:image/jpeg;base64,${Buffer.from(JPG).toString("base64")}">`));
  assert.ok(!out.includes("file:///"));
});

test("a tag without a picture left in the RTF stays as it is, and an unquoted address is replaced", async () => {
  const rtf = `{\\rtf1 ${pict("\\pngblip", PNG)}}`;
  const out = await images.inlineOfficeClipboardImages("<img src=file:///x.png><img src=\"file:///y.png\">", rtf, {});
  assert.match(out, /^<img src="data:image\/png;base64,[^"]+"><img src="file:\/\/\/y\.png">$/);
});

test("needsRtfImageInlining is false without RTF pictures, without HTML pictures, or when all are data", () => {
  const rtf = `{\\rtf1 ${pict("\\pngblip", PNG)}}`;
  assert.equal(images.needsRtfImageInlining("<p>x</p>", rtf), false);
  assert.equal(images.needsRtfImageInlining('<img src="data:image/png;base64,AA==">', rtf), false);
  assert.equal(images.needsRtfImageInlining('<img src="file:///a.png">', "{\\rtf1 text}"), false);
  assert.equal(images.needsRtfImageInlining('<img src="file:///a.png">', ""), false);
});

test("an RTF over the length bound is not read (fail-closed)", async () => {
  const big = "{\\rtf1 " + "x".repeat(images.LIMITS.MAX_RTF_CHARS) + pict("\\pngblip", PNG) + "}";
  assert.equal(images.needsRtfImageInlining('<img src="file:///a.png">', big), false);
  const html = '<img src="file:///a.png">';
  assert.equal(await images.inlineOfficeClipboardImages(html, big, {}), html);
});

test("an <img start without an end, repeated, is read in one pass", async () => {
  const html = "<img ".repeat(400000);
  const rtf = `{\\rtf1 ${pict("\\pngblip", PNG)}}`;
  const start = process.hrtime.bigint();
  assert.equal(images.needsRtfImageInlining(html, rtf), false);
  assert.equal(await images.inlineOfficeClipboardImages(html, rtf, {}), html);
  assert.ok(Number(process.hrtime.bigint() - start) / 1e6 < 1500);
});

// ---- EMF -----------------------------------------------------------------------------------------------------

function emfWith(...records) {
  const header = new Uint8Array(88);
  const hv = new DataView(header.buffer);
  hv.setUint32(0, 1, true);
  hv.setUint32(4, 88, true);
  hv.setInt32(8, 0, true);
  hv.setInt32(12, 0, true);
  hv.setInt32(16, 99, true);
  hv.setInt32(20, 99, true);
  const eof = new Uint8Array(20);
  new DataView(eof.buffer).setUint32(0, 14, true);
  new DataView(eof.buffer).setUint32(4, 20, true);
  const parts = [header, ...records, eof];
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out;
}

function dibRecord({ type = 81, x = 0, y = 0, w = 2, h = 2, cx = 0, cy = 0, sizeOverride, offBmi = 80, cbBmi = 40, bits = 16 } = {}) {
  const size = sizeOverride ?? 80 + cbBmi + bits;
  const rec = new Uint8Array(80 + cbBmi + bits);
  const v = new DataView(rec.buffer);
  v.setUint32(0, type, true);
  v.setUint32(4, size, true);
  v.setInt32(24, x, true);
  v.setInt32(28, y, true);
  v.setUint32(48, offBmi, true);
  v.setUint32(52, cbBmi, true);
  v.setUint32(56, offBmi + cbBmi, true);
  v.setUint32(60, bits, true);
  v.setInt32(72, cx, true);
  v.setInt32(76, cy, true);
  v.setUint32(80, 40, true);
  v.setInt32(84, w, true);
  v.setInt32(88, h, true);
  return rec;
}

test("the bitmap records of an EMF come out with their rectangles", () => {
  const emf = emfWith(dibRecord({ x: 10, y: 20, w: 4, h: 3, cx: 8, cy: 6 }), dibRecord({ type: 80, x: 1, y: 2, w: 5, h: 7 }));
  const { dibs, bounds } = images.extractEmfDibs(emf);
  assert.equal(dibs.length, 2);
  assert.deepEqual([dibs[0].x, dibs[0].y, dibs[0].w, dibs[0].h], [10, 20, 8, 6], "STRETCHDIBITS has its own size");
  assert.deepEqual([dibs[1].x, dibs[1].y, dibs[1].w, dibs[1].h], [1, 2, 5, 7], "SETDIBITSTODEVICE keeps the source size");
  assert.deepEqual(bounds, { x: 0, y: 0, w: 100, h: 100 });
  const bmp = images.pieceBmp(emf, dibs[0]);
  assert.deepEqual(Array.from(bmp.subarray(0, 2)), [0x42, 0x4d]);
  assert.equal(new DataView(bmp.buffer, bmp.byteOffset).getUint32(10, true), 14 + 40);
});

test("a hostile EMF yields nothing and throws nothing", () => {
  const cases = {
    "record size under 8": emfWith(Uint8Array.from([81, 0, 0, 0, 4, 0, 0, 0])),
    "record bigger than the file": emfWith(dibRecord({ sizeOverride: 1 << 30 })),
    "record shorter than its fields": emfWith(dibRecord({ sizeOverride: 12 })),
    "bitmap header outside the record": emfWith(dibRecord({ offBmi: 4000 })),
    "bitmap header under 12 bytes": emfWith(dibRecord({ cbBmi: 4 })),
    "a billion pixels": emfWith(dibRecord({ w: 100000, h: 100000 })),
    "zero size": emfWith(dibRecord({ w: 0, h: 0 })),
    "shorter than a header": new Uint8Array(40),
  };
  for (const [name, emf] of Object.entries(cases)) assert.equal(images.extractEmfDibs(emf).dibs.length, 0, name);
});

test("an EMF with thousands of bitmap records is refused (fail-closed)", () => {
  const records = Array.from({ length: images.LIMITS.MAX_EMF_PIECES + 50 }, () => dibRecord());
  assert.equal(images.extractEmfDibs(emfWith(...records)).dibs.length, 0);
});

// ---- amplification: records that point at the same bytes -------------------------------------------------------

// An EMF whose `count` bitmap records all point at one `bitsBytes` long block at its end.
function sharedBitsEmf(count, bitsBytes, { w = 2, h = 2 } = {}) {
  const recBytes = 128;
  const head = emfWith();
  const blobStart = head.length - 20 + count * recBytes;
  const out = new Uint8Array(blobStart + bitsBytes + 20);
  out.set(head.subarray(0, 88), 0);
  for (let i = 0; i < count; i += 1) {
    const at = 88 + i * recBytes;
    const v = new DataView(out.buffer, at, recBytes);
    v.setUint32(0, 81, true);
    v.setUint32(4, recBytes, true);
    v.setUint32(48, 80, true);
    v.setUint32(52, 40, true);
    v.setUint32(56, blobStart - at, true);
    v.setUint32(60, bitsBytes, true);
    v.setUint32(80, 40, true);
    v.setInt32(84, w, true);
    v.setInt32(88, h, true);
  }
  new DataView(out.buffer).setUint32(blobStart + bitsBytes, 14, true);
  new DataView(out.buffer).setUint32(blobStart + bitsBytes + 4, 20, true);
  return out;
}

test("256 records that point at one 16 MiB block would copy 4 GiB: the EMF is refused by the sum of its BMP bytes", () => {
  const emf = sharedBitsEmf(256, 16 * 1024 * 1024);
  const { result, ms } = timed(() => images.extractEmfDibs(emf));
  assert.equal(result.dibs.length, 0);
  assert.ok(ms < 200, `took ${ms} ms`);
  assert.equal(images.extractEmfDibs(sharedBitsEmf(1, 16 * 1024 * 1024)).dibs.length, 1, "one such piece is fine");
});

test("pieces are bounded by the sum of their pixels, not only one by one", () => {
  const piece = (n) => emfWith(...Array.from({ length: n }, () => dibRecord({ w: 4000, h: 4000 })));
  assert.equal(images.extractEmfDibs(piece(2)).dibs.length, 2);
  assert.equal(images.extractEmfDibs(piece(3)).dibs.length, 0);
});

// A canvas and an image decoder that only record what they are asked.
function fakeBrowser() {
  const log = { canvases: [], live: 0, maxLive: 0, blobs: 0 };
  class Blob {
    constructor(parts) {
      this.size = parts.reduce((n, p) => n + p.byteLength, 0);
    }
    async arrayBuffer() {
      return PNG.buffer.slice(PNG.byteOffset, PNG.byteOffset + PNG.byteLength);
    }
  }
  const dims = { width: 1, height: 1 };
  return {
    log,
    dims,
    env: {
      Blob,
      createImageBitmap: async () => {
        log.live += 1;
        log.maxLive = Math.max(log.maxLive, log.live);
        return { width: dims.width, height: dims.height, close: () => { log.live -= 1; } };
      },
      document: {
        createElement: () => {
          const canvas = { width: 0, height: 0, getContext: () => ({ drawImage() {} }), toBlob: (done) => done(new Blob([PNG])) };
          log.canvases.push(canvas);
          return canvas;
        },
      },
    },
  };
}

test("one big piece is held to the canvas side limit too, and every piece is let go as soon as it is drawn", async () => {
  const fake = fakeBrowser();
  fake.dims.width = 5000;
  fake.dims.height = 5000;
  const emf = emfWith(dibRecord({ w: 5000, h: 5000 }));
  const out = await images.inlineOfficeClipboardImages('<img src="file:///a.emz">', `{\\rtf1 ${pict("\\emfblip", emf)}}`, fake.env);
  assert.match(out, /data:image\/png;base64,/);
  assert.ok(fake.log.canvases[0].width <= images.LIMITS.MAX_CANVAS_SIDE && fake.log.canvases[0].height <= images.LIMITS.MAX_CANVAS_SIDE, JSON.stringify(fake.log.canvases[0]));
  assert.equal(fake.log.live, 0);
  const several = emfWith(dibRecord({ x: 0, w: 4, h: 4 }), dibRecord({ x: 10, w: 4, h: 4 }), dibRecord({ x: 20, w: 4, h: 4 }));
  await images.inlineOfficeClipboardImages('<img src="file:///a.emz">', `{\\rtf1 ${pict("\\emfblip", several)}}`, fake.env);
  assert.equal(fake.log.maxLive, 1, "one decoded piece at a time");
});

test("a group that is too short keeps its place: the picture after it does not move to the first tag", async () => {
  const rtf = `{\\rtf1 {\\pict\\pngblip 00ff}${pict("\\pngblip", PNG)}}`;
  const html = '<img src="file:///a.png"><img src="file:///b.png">';
  const out = await images.inlineOfficeClipboardImages(html, rtf, {});
  assert.ok(out.startsWith('<img src="file:///a.png"><img src="data:image/png;base64,'), out);
});

test("a letter with a different lower case form does not shift the scan of the clean-up", () => {
  const out = sanitizeOfficeHtml("\u0130".repeat(20) + "<script>alert(1)</script>x<style>p{}</style>y");
  assert.ok(!out.toLowerCase().includes("<script") && !out.includes("alert") && !out.toLowerCase().includes("<style"), out);
  assert.ok(out.endsWith("xy"));
});
