(function (host) {
  "use strict";

  // The pictures of a Hangul or office clipboard, put into the pasted HTML. Reference implementation: rhwp-studio
  // (https://github.com/edwardkim/rhwp, commit 1a76570e833917d15817415a53c09ad61ab3203f,
  // `rhwp-studio/src/engine/office-clipboard-images.ts`, MIT, Copyright (c) 2025-2026 Edward Kim): `hexToBytes` and
  // `dibToBmpFile` (lines 26-54), `extractEmfDibs` (56-105), `emfToPngBlob` (108-158), `extractRtfPictures` (185-204),
  // `bytesToDataUrl` (206-223), `inlineOfficeClipboardImages` (246-281) and `needsRtfImageInlining` (324-327).
  //
  // The Hangul editor writes a picture into the clipboard HTML only as `<img src="file:///C:\...\clip_image001.png">`, which a
  // page cannot open. The `text/rtf` of the same clipboard holds the same pictures in order, so the pixels are taken from
  // there and put in as `data:` addresses. RTF picture forms: `\pngblip` and `\jpegblip` are used as they are; `\wmetafile8`
  // and `\emfblip` hold a DIB inside the EMF (records `EMR_STRETCHDIBITS` and `EMR_SETDIBITSTODEVICE`), which is built into a
  // BMP and drawn into one PNG at the places the records give (the pieces of a grouped picture); an EMF of vector shapes only
  // has no pixels and is skipped (that picture only).
  //
  // Not moved: `imagePixelSize` and `liftImagesToBlockLevel` (the document model here takes a picture out of its
  // paragraph while it reads the HTML, `doc-paste-model.js`), the BMP branch of `bytesToDataUrl` (`extractRtfPictures` never
  // answers a BMP), the console logging.
  // The clipboard is not trusted input, and the reference has no bounds, so they are added (fail-closed): the RTF text is
  // refused above a length, the pictures per clipboard, the pieces per EMF and the pixels of a piece are counted, the
  // canvas keeps the reference's 4000 pixel side, and the scans are single passes (the reference's `\pict` pattern and
  // `<img\b[^>]*>` pattern re-scan to the end of the text for every start that has no end, which is quadratic).

  const EMR_SETDIBITSTODEVICE = 80;
  const EMR_STRETCHDIBITS = 81;
  const EMR_EOF = 14;
  const MAX_RTF_CHARS = 32 * 1024 * 1024;
  const MAX_RTF_PICTURES = 32;
  const MAX_EMF_PIECES = 256;
  const MAX_PIECE_PIXELS = 64 * 1024 * 1024;
  const MAX_CANVAS_SIDE = 4000;
  const MAX_COMPOSE_SCALE = 4;
  const MIN_PICTURE_HEX_DIGITS = 32;
  // The record header and the DIB fields up to `cbBits` end at byte 64.
  const MIN_DIB_RECORD_BYTES = 64;

  const LIMITS = { MAX_RTF_CHARS, MAX_RTF_PICTURES, MAX_EMF_PIECES, MAX_PIECE_PIXELS, MAX_CANVAS_SIDE };

  // Value of an ASCII hex digit by character code, -1 for anything else.
  const HEX_VALUE = new Int8Array(128).fill(-1);
  for (let i = 0; i < 10; i += 1) HEX_VALUE[0x30 + i] = i;
  for (let i = 0; i < 6; i += 1) {
    HEX_VALUE[0x41 + i] = 10 + i;
    HEX_VALUE[0x61 + i] = 10 + i;
  }

  const isAsciiLetter = (c) => (c >= 0x41 && c <= 0x5a) || (c >= 0x61 && c <= 0x7a);
  const isAsciiDigit = (c) => c >= 0x30 && c <= 0x39;

  // The hex digits of an RTF picture group as bytes. The reference removes the control words (`\picw3439 `: letters, an
  // optional minus, digits) and then every character that is not a hex digit, and decodes pairs; this is the same in one
  // pass over the text without the two intermediate strings. An odd digit at the end is dropped.
  function pictureBytes(body) {
    const out = new Uint8Array(body.length >> 1);
    let count = 0;
    let high = -1;
    for (let i = 0; i < body.length; i += 1) {
      const c = body.charCodeAt(i);
      if (c === 0x5c && isAsciiLetter(body.charCodeAt(i + 1))) {
        let j = i + 1;
        while (isAsciiLetter(body.charCodeAt(j))) j += 1;
        if (body.charCodeAt(j) === 0x2d) j += 1;
        while (isAsciiDigit(body.charCodeAt(j))) j += 1;
        i = j - 1;
        continue;
      }
      const value = c < 128 ? HEX_VALUE[c] : -1;
      if (value < 0) continue;
      if (high < 0) high = value;
      else {
        out[count] = (high << 4) | value;
        count += 1;
        high = -1;
      }
    }
    return count * 2 < MIN_PICTURE_HEX_DIGITS ? null : out.subarray(0, count);
  }

  function dibToBmpFile(bmi, bits) {
    const out = new Uint8Array(14 + bmi.byteLength + bits.byteLength);
    const view = new DataView(out.buffer);
    out[0] = 0x42;
    out[1] = 0x4d; // 'BM'
    view.setUint32(2, out.byteLength, true);
    view.setUint32(10, 14 + bmi.byteLength, true);
    out.set(bmi, 14);
    out.set(bits, 14 + bmi.byteLength);
    return out;
  }

  // Every bitmap record of an EMF with its destination rectangle. A grouped picture (logo and letters) comes as several
  // pieces in one EMF; the largest piece alone would leave half a picture, so all are returned and drawn where they belong.
  function extractEmfDibs(emf) {
    const dibs = [];
    if (emf.byteLength < 44) return { dibs, bounds: null };
    const view = new DataView(emf.buffer, emf.byteOffset, emf.byteLength);
    // `rclBounds` of the header, in device units
    const bx = view.getInt32(8, true);
    const by = view.getInt32(12, true);
    const bx2 = view.getInt32(16, true);
    const by2 = view.getInt32(20, true);
    const bounds = bx2 >= bx && by2 >= by ? { x: bx, y: by, w: bx2 - bx + 1, h: by2 - by + 1 } : null;
    let offset = 0;
    while (offset + 8 <= emf.byteLength && dibs.length < MAX_EMF_PIECES) {
      const type = view.getUint32(offset, true);
      const size = view.getUint32(offset + 4, true);
      if (size < 8 || offset + size > emf.byteLength) break;
      if ((type === EMR_STRETCHDIBITS || type === EMR_SETDIBITSTODEVICE) && size >= MIN_DIB_RECORD_BYTES) {
        const piece = readDibRecord(emf, view, offset, type);
        if (piece) dibs.push(piece);
      }
      offset += size;
      if (type === EMR_EOF) break;
    }
    return { dibs, bounds };
  }

  function readDibRecord(emf, view, offset, type) {
    const xDest = view.getInt32(offset + 24, true);
    const yDest = view.getInt32(offset + 28, true);
    const offBmi = view.getUint32(offset + 48, true);
    const cbBmi = view.getUint32(offset + 52, true);
    const offBits = view.getUint32(offset + 56, true);
    const cbBits = view.getUint32(offset + 60, true);
    // Only STRETCHDIBITS has `cxDest` and `cyDest`; SETDIBITSTODEVICE keeps the size of the source.
    const stretched = type === EMR_STRETCHDIBITS && offset + 80 <= emf.byteLength;
    const cxDest = stretched ? view.getInt32(offset + 72, true) : 0;
    const cyDest = stretched ? view.getInt32(offset + 76, true) : 0;
    if (cbBmi < 12 || cbBits <= 0 || offset + offBmi + cbBmi > emf.byteLength || offset + offBits + cbBits > emf.byteLength) return null;
    const bmi = emf.subarray(offset + offBmi, offset + offBmi + cbBmi);
    const bits = emf.subarray(offset + offBits, offset + offBits + cbBits);
    const bmiView = new DataView(bmi.buffer, bmi.byteOffset, bmi.byteLength);
    const srcW = Math.abs(bmiView.getInt32(4, true));
    const srcH = Math.abs(bmiView.getInt32(8, true));
    if (srcW <= 0 || srcH <= 0 || srcW * srcH > MAX_PIECE_PIXELS) return null;
    return { bmp: dibToBmpFile(bmi, bits), x: xDest, y: yDest, w: cxDest > 0 ? cxDest : srcW, h: cyDest > 0 ? cyDest : srcH };
  }

  const toBlob = (canvas) => new Promise((resolve) => canvas.toBlob((blob) => resolve(blob), "image/png"));

  // The bitmap pieces of an EMF drawn as one PNG blob at their places; null when the EMF has no bitmap.
  async function emfToPngBlob(emf, env) {
    const { dibs } = extractEmfDibs(emf);
    if (dibs.length === 0) return null;
    const bitmaps = [];
    try {
      for (const d of dibs) {
        try {
          bitmaps.push({ bmp: await env.createImageBitmap(new env.Blob([d.bmp], { type: "image/bmp" })), d });
        } catch {
          // a piece that does not decode is left out; the others are still drawn
        }
      }
      if (bitmaps.length === 0) return null;
      // One piece: the original pixels, no resampling.
      if (bitmaps.length === 1 && dibs.length === 1) {
        const only = bitmaps[0];
        const canvas = env.document.createElement("canvas");
        canvas.width = only.bmp.width;
        canvas.height = only.bmp.height;
        canvas.getContext("2d")?.drawImage(only.bmp, 0, 0);
        return await toBlob(canvas);
      }
      // Several pieces: the union of their destination rectangles is the canvas. The header's `rclBounds` includes the
      // empty margin around the picture and makes a pasted picture loose, so it is not used.
      let minX = Infinity;
      let minY = Infinity;
      let maxX = -Infinity;
      let maxY = -Infinity;
      for (const { d } of bitmaps) {
        minX = Math.min(minX, d.x);
        minY = Math.min(minY, d.y);
        maxX = Math.max(maxX, d.x + d.w);
        maxY = Math.max(maxY, d.y + d.h);
      }
      const spanW = Math.max(1, Math.round(maxX - minX));
      const spanH = Math.max(1, Math.round(maxY - minY));
      // The scale keeps the source resolution of the pieces (at most 4 times) inside the side limit.
      const natural = bitmaps.reduce((most, { bmp, d }) => Math.max(most, d.w > 0 ? bmp.width / d.w : 1), 1);
      const scale = Math.min(MAX_COMPOSE_SCALE, Math.max(1, natural), MAX_CANVAS_SIDE / Math.max(spanW, spanH));
      const canvas = env.document.createElement("canvas");
      canvas.width = Math.max(1, Math.round(spanW * scale));
      canvas.height = Math.max(1, Math.round(spanH * scale));
      const ctx = canvas.getContext("2d");
      if (!ctx) return null;
      for (const { bmp, d } of bitmaps) ctx.drawImage(bmp, (d.x - minX) * scale, (d.y - minY) * scale, d.w * scale, d.h * scale);
      return await toBlob(canvas);
    } finally {
      for (const { bmp } of bitmaps) bmp.close?.();
    }
  }

  // The pictures of an RTF text in document order. A group that is no picture form keeps its place with empty bytes, so the
  // n-th picture of the RTF stays the n-th picture of the HTML.
  function extractRtfPictures(rtf) {
    const pictures = [];
    let from = 0;
    while (pictures.length < MAX_RTF_PICTURES) {
      const start = rtf.indexOf("\\pict", from);
      if (start < 0) break;
      const end = rtf.indexOf("}", start + 5);
      if (end < 0) break;
      const body = rtf.slice(start + 5, end);
      from = end + 1;
      const bytes = pictureBytes(body);
      if (!bytes) continue;
      if (/\\pngblip\b/.test(body)) pictures.push({ bytes, mime: "image/png" });
      else if (/\\jpegblip\b/.test(body)) pictures.push({ bytes, mime: "image/jpeg" });
      else if (/\\(?:wmetafile\d*|emfblip)\b/.test(body)) pictures.push({ bytes, mime: "image/emf" });
      else pictures.push({ bytes: new Uint8Array(0), mime: "" });
    }
    return pictures;
  }

  async function bytesToDataUrl(bytes, mime, env) {
    if (!bytes.byteLength || !mime) return null;
    const utils = host.__EASYOFFICE_SHEET_UTILS__;
    if (mime !== "image/emf") return `data:${mime};base64,${utils.bytesToBase64(bytes)}`;
    const png = await emfToPngBlob(bytes, env);
    return png ? `data:image/png;base64,${utils.bytesToBase64(new Uint8Array(await png.arrayBuffer()))}` : null;
  }

  // The `<img ...>` tags of the HTML as `[start, end)` ranges in one pass; a start without `>` ends the list.
  function imgTagRanges(html) {
    const ranges = [];
    const start = /<img\b/gi;
    let match;
    while ((match = start.exec(html)) !== null) {
      const end = html.indexOf(">", match.index);
      if (end < 0) break;
      ranges.push([match.index, end + 1]);
      start.lastIndex = end + 1;
    }
    return ranges;
  }

  const SRC_ATTRIBUTE = /src\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)/i;
  const DATA_SRC = /src\s*=\s*["']?data:/i;

  // The HTML of a clipboard that carries RTF pictures, and has a picture the page could not open (not a `data:` one).
  function needsRtfImageInlining(html, rtf) {
    if (!html || !rtf || rtf.length > MAX_RTF_CHARS || !rtf.includes("\\pict")) return false;
    return imgTagRanges(html).some(([from, to]) => {
      const tag = html.slice(from, to);
      return SRC_ATTRIBUTE.test(tag) && !DATA_SRC.test(tag);
    });
  }

  // The `<img src>` of the HTML that a page cannot open (`file://` and the like) as the `data:` address of the RTF picture
  // in the same place. A `data:` picture stays, and a tag with no picture left in the RTF stays as it is.
  async function inlineOfficeClipboardImages(html, rtf, env) {
    if (!rtf || rtf.length > MAX_RTF_CHARS || !rtf.includes("\\pict")) return html;
    const pictures = extractRtfPictures(rtf);
    const ranges = imgTagRanges(html);
    if (pictures.length === 0 || ranges.length === 0) return html;

    // The HTML writes its pictures in the order of the `\pict` groups of the RTF: the n-th tag takes the n-th picture. Pairing
    // by size or aspect ratio is wrong (a small logo with the same ratio takes the place of a big diagram).
    let out = "";
    let at = 0;
    let pictIndex = 0;
    for (const [from, to] of ranges) {
      const tag = html.slice(from, to);
      out += html.slice(at, from);
      at = to;
      if (DATA_SRC.test(tag)) {
        out += tag;
        continue;
      }
      while (pictIndex < pictures.length && !pictures[pictIndex].bytes.byteLength) pictIndex += 1;
      const picture = pictures[pictIndex];
      pictIndex += 1;
      const url = picture ? await bytesToDataUrl(picture.bytes, picture.mime, env) : null;
      if (!url) out += tag;
      else if (SRC_ATTRIBUTE.test(tag)) out += tag.replace(SRC_ATTRIBUTE, () => `src="${url}"`);
      else out += tag.replace(/<img\b/i, () => `<img src="${url}"`);
    }
    return out + html.slice(at);
  }

  host.__EASYOFFICE_DOC_CLIPBOARD_IMAGES__ = { LIMITS, extractEmfDibs, extractRtfPictures, needsRtfImageInlining, inlineOfficeClipboardImages };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
