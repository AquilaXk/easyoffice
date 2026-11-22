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
  // refused above a length, the pictures per clipboard are counted, and an EMF is refused when the sum of its pieces' BMP
  // bytes or pixels, or their number, is over a bound (records can point at the same bytes), a piece is built only
  // when it is drawn, the canvas keeps the reference's 4000 pixel side for one piece too, and the scans are single passes (the reference's `\pict` pattern and
  // `<img\b[^>]*>` pattern re-scan to the end of the text for every start that has no end, which is quadratic).

  const EMR_SETDIBITSTODEVICE = 80;
  const EMR_STRETCHDIBITS = 81;
  const EMR_EOF = 14;
  const MAX_RTF_CHARS = 32 * 1024 * 1024;
  const MAX_RTF_PICTURES = 32;
  const MAX_EMF_PIECES = 256;
  const MAX_PIECE_PIXELS = 32 * 1024 * 1024;
  const MAX_EMF_TOTAL_PIXELS = 32 * 1024 * 1024;
  const MAX_EMF_BMP_BYTES = 32 * 1024 * 1024;
  const MAX_CANVAS_SIDE = 4000;
  const MAX_COMPOSE_SCALE = 4;
  const MIN_PICTURE_HEX_DIGITS = 32;
  // The record header and the DIB fields up to `cbBits` end at byte 64.
  const MIN_DIB_RECORD_BYTES = 64;
  const MAX_IMAGE_BYTES = 32 * 1024 * 1024;
  const MAX_TOTAL_IMAGE_BYTES = 64 * 1024 * 1024;
  const MAX_IMAGE_PIXELS = 50_000_000;
  const MAX_DIMENSION_PX = 65535;
  const MAX_SVG_BYTES = 2 * 1024 * 1024;
  const MAX_SVG_TAGS = 10_000;
  const SAFE_SVG_XMLNS = /\bxmlns(?::[a-z0-9_-]+)?\s*=\s*(["'])http:\/\/(?:www\.w3\.org\/2000\/svg|www\.w3\.org\/1999\/xlink|www\.w3\.org\/XML\/1998\/namespace)\1/gi;

  const LIMITS = {
    MAX_RTF_CHARS,
    MAX_RTF_PICTURES,
    MAX_EMF_PIECES,
    MAX_PIECE_PIXELS,
    MAX_EMF_TOTAL_PIXELS,
    MAX_EMF_BMP_BYTES,
    MAX_CANVAS_SIDE,
    MAX_IMAGE_BYTES,
    MAX_TOTAL_IMAGE_BYTES,
    MAX_IMAGE_PIXELS,
    MAX_SVG_BYTES,
    MAX_SVG_TAGS,
  };

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
  // A piece is only described here (where its header and bits are); the BMP is built when the piece is drawn (`pieceBmp`).
  // Records may point at the same bytes, so the pieces are counted by what they would cost: the sum of their BMP bytes
  // and the sum of their pixels are bounded, and an EMF over either bound gives no piece at all (fail-closed).
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
    let bytes = 0;
    let pixels = 0;
    while (offset + 8 <= emf.byteLength) {
      const type = view.getUint32(offset, true);
      const size = view.getUint32(offset + 4, true);
      if (size < 8 || offset + size > emf.byteLength) break;
      if ((type === EMR_STRETCHDIBITS || type === EMR_SETDIBITSTODEVICE) && size >= MIN_DIB_RECORD_BYTES) {
        const piece = readDibRecord(emf, view, offset, type);
        if (piece) {
          bytes += 14 + piece.bmiLength + piece.bitsLength;
          pixels += piece.srcW * piece.srcH;
          if (dibs.length >= MAX_EMF_PIECES || bytes > MAX_EMF_BMP_BYTES || pixels > MAX_EMF_TOTAL_PIXELS) return { dibs: [], bounds };
          dibs.push(piece);
        }
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
    const bmiView = new DataView(emf.buffer, emf.byteOffset + offset + offBmi, cbBmi);
    const srcW = Math.abs(bmiView.getInt32(4, true));
    const srcH = Math.abs(bmiView.getInt32(8, true));
    if (srcW <= 0 || srcH <= 0 || srcW * srcH > MAX_PIECE_PIXELS) return null;
    return {
      bmiAt: offset + offBmi, bmiLength: cbBmi, bitsAt: offset + offBits, bitsLength: cbBits, srcW, srcH,
      x: xDest, y: yDest, w: cxDest > 0 ? cxDest : srcW, h: cyDest > 0 ? cyDest : srcH,
    };
  }

  // The BMP file of a piece: copied from the EMF when the piece is drawn and let go right after it is decoded.
  const pieceBmp = (emf, d) => dibToBmpFile(emf.subarray(d.bmiAt, d.bmiAt + d.bmiLength), emf.subarray(d.bitsAt, d.bitsAt + d.bitsLength));

  const toBlob = (canvas) => new Promise((resolve) => canvas.toBlob((blob) => resolve(blob), "image/png"));

  // The bitmap pieces of an EMF drawn as one PNG blob at their places; null when the EMF has no bitmap.
  async function emfToPngBlob(emf, env) {
    const { dibs } = extractEmfDibs(emf);
    if (dibs.length === 0) return null;
    // The canvas comes from the descriptions, so a piece can be drawn and let go as soon as it is decoded. One piece keeps
    // its own pixels (no resampling) unless it is over the side limit; several pieces use the union of their rectangles
    // (the header's `rclBounds` includes the empty margin around the picture and makes a pasted picture loose).
    let minX = 0;
    let minY = 0;
    let spanW = dibs[0].srcW;
    let spanH = dibs[0].srcH;
    let scale = 1;
    if (dibs.length > 1) {
      minX = Math.min(...dibs.map((d) => d.x));
      minY = Math.min(...dibs.map((d) => d.y));
      spanW = Math.max(1, Math.round(Math.max(...dibs.map((d) => d.x + d.w)) - minX));
      spanH = Math.max(1, Math.round(Math.max(...dibs.map((d) => d.y + d.h)) - minY));
      // The scale keeps the source resolution of the pieces (at most 4 times) inside the side limit.
      scale = Math.max(1, dibs.reduce((most, d) => Math.max(most, d.w > 0 ? d.srcW / d.w : 1), 1));
      scale = Math.min(MAX_COMPOSE_SCALE, scale);
    }
    scale = Math.min(scale, MAX_CANVAS_SIDE / Math.max(spanW, spanH));
    const canvas = env.document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(spanW * scale));
    canvas.height = Math.max(1, Math.round(spanH * scale));
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    let drawn = 0;
    for (const d of dibs) {
      let bitmap = null;
      try {
        bitmap = await env.createImageBitmap(new env.Blob([pieceBmp(emf, d)], { type: "image/bmp" }));
        if (dibs.length === 1) ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
        else ctx.drawImage(bitmap, (d.x - minX) * scale, (d.y - minY) * scale, d.w * scale, d.h * scale);
        drawn += 1;
      } catch {
        // a piece that does not decode is left out; the others are still drawn
      } finally {
        bitmap?.close?.();
      }
    }
    return drawn > 0 ? await toBlob(canvas) : null;
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
      // A group with too little data keeps its place with empty bytes: the n-th picture stays the n-th tag.
      if (!bytes) pictures.push({ bytes: new Uint8Array(0), mime: "" });
      else if (/\\pngblip\b/.test(body)) pictures.push({ bytes, mime: "image/png" });
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

  const SRC_ATTRIBUTE = /src\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))/i;
  const DATA_SRC = /src\s*=\s*["']?data:/i;
  const DATA_GIF_OR_SVG_IMG = /src\s*=\s*["']?\s*data:image\/(?:gif\b|svg\+xml\b)/i;

  function utf8BytesOf(text) {
    if (typeof TextEncoder === "function") return new TextEncoder().encode(text);
    const out = [];
    for (let i = 0; i < text.length; i += 1) {
      const c = text.charCodeAt(i);
      if (c < 0x80) out.push(c);
      else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 0x3f));
      else out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 0x3f), 0x80 | (c & 0x3f));
    }
    return new Uint8Array(out);
  }

  function decodeBase64Bytes(cleaned, env) {
    const utils = host.__EASYOFFICE_SHEET_UTILS__;
    const dec = env?.atob ?? host.atob ?? globalThis.atob;
    if (utils?.base64ToBytes) return utils.base64ToBytes(cleaned, typeof dec === "function" ? dec : undefined);
    if (typeof dec !== "function") throw new Error("no atob");
    const bin = dec(cleaned);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
    return out;
  }

  function encodeBytesToBase64(bytes, env) {
    const utils = host.__EASYOFFICE_SHEET_UTILS__;
    if (utils?.bytesToBase64) return utils.bytesToBase64(bytes);
    const enc = env?.btoa ?? host.btoa ?? globalThis.btoa;
    let bin = "";
    for (let i = 0; i < bytes.length; i += 1) bin += String.fromCharCode(bytes[i]);
    return enc(bin);
  }

  function checkGifBytes(u8) {
    if (!(u8 instanceof Uint8Array) || u8.byteLength < 26 || u8.byteLength > MAX_IMAGE_BYTES) return { reason: "invalid" };
    const is87 = u8[0] === 0x47 && u8[1] === 0x49 && u8[2] === 0x46 && u8[3] === 0x38 && u8[4] === 0x37 && u8[5] === 0x61;
    const is89 = u8[0] === 0x47 && u8[1] === 0x49 && u8[2] === 0x46 && u8[3] === 0x38 && u8[4] === 0x39 && u8[5] === 0x61;
    if (!is87 && !is89) return { reason: "invalid" };
    const widthPx = u8[6] | (u8[7] << 8);
    const heightPx = u8[8] | (u8[9] << 8);
    if (widthPx < 1 || heightPx < 1 || widthPx > MAX_DIMENSION_PX || heightPx > MAX_DIMENSION_PX) return { reason: "invalid" };
    if (widthPx * heightPx > MAX_IMAGE_PIXELS) return { reason: "area" };
    return { header: { widthPx, heightPx } };
  }

  function parseGifHeader(u8) {
    return checkGifBytes(u8).header ?? null;
  }

  function decodeNumericEntities(text) {
    return text
      .replace(/&#x([0-9a-fA-F]+);?/g, (_, hex) => {
        const cp = Number.parseInt(hex, 16);
        return Number.isFinite(cp) && cp > 0 && cp <= 0x10ffff ? String.fromCodePoint(cp) : "";
      })
      .replace(/&#(\d+);?/g, (_, dec) => {
        const cp = Number.parseInt(dec, 10);
        return Number.isFinite(cp) && cp > 0 && cp <= 0x10ffff ? String.fromCodePoint(cp) : "";
      });
  }

  function parseSvgLengthPx(attrText, name) {
    const match = new RegExp(`(?:^|\\s)${name}\\s*=\\s*(?:"([^"]*)"|'([^']*)'|([^\\s>]+))`, "i").exec(attrText);
    const raw = (match?.[1] ?? match?.[2] ?? match?.[3] ?? "").trim();
    if (!raw || raw.endsWith("%")) return null;
    const parsed = /^([+]?(?:\d+(?:\.\d+)?|\.\d+))\s*(px|pt|mm|cm|in)?$/i.exec(raw);
    if (!parsed) return null;
    const value = Number(parsed[1]);
    if (!Number.isFinite(value) || value <= 0) return null;
    const unit = (parsed[2] || "px").toLowerCase();
    const factor = unit === "pt" ? 4 / 3 : unit === "mm" ? 96 / 25.4 : unit === "cm" ? 96 / 2.54 : unit === "in" ? 96 : 1;
    return value * factor;
  }

  function parseSvgDimensions(attrText) {
    let w = parseSvgLengthPx(attrText, "width");
    let h = parseSvgLengthPx(attrText, "height");
    if (w === null || h === null) {
      const vbMatch = /(?:^|\s)viewbox\s*=\s*(?:"([^"]*)"|'([^']*)')/i.exec(attrText);
      const vbParts = (vbMatch?.[1] ?? vbMatch?.[2] ?? "").trim().split(/[\s,]+/).map(Number);
      if (vbParts.length === 4 && vbParts.every(Number.isFinite) && vbParts[2] > 0 && vbParts[3] > 0) {
        w ??= vbParts[2];
        h ??= vbParts[3];
      }
    }
    const widthPx = Math.round(w ?? 300);
    const heightPx = Math.round(h ?? 150);
    if (!Number.isFinite(widthPx) || !Number.isFinite(heightPx) || widthPx < 1 || heightPx < 1) return null;
    if (widthPx > MAX_DIMENSION_PX || heightPx > MAX_DIMENSION_PX || widthPx * heightPx > MAX_IMAGE_PIXELS) return null;
    return { widthPx, heightPx };
  }

  function hasUnsafeSvgContent(candidate) {
    if (/<!(?:DOCTYPE|ENTITY)/i.test(candidate)) return true;
    if (/<\s*(?:script|foreignobject|iframe|object|embed|applet|audio|video|canvas)\b/i.test(candidate)) return true;
    if (/(?:^|[\s"'/<>])on[a-z]+\s*=/i.test(candidate)) return true;
    const stripped = candidate.replace(SAFE_SVG_XMLNS, "");
    if (/(?:https?|file|ftp|blob):/i.test(stripped) || /@import/i.test(stripped)) return true;
    if (/style\s*=\s*(?:"[^"]*\\|'[^']*\\)/i.test(stripped)) return true;
    const compacted = stripped.replace(/[\u0000-\u0020\u007f-\u009f]+/g, "").toLowerCase();
    return /(?:javascript|vbscript):|data:text\/html|data:image\/svg\+xml/.test(compacted);
  }

  function validateSvgSource(rawSvg) {
    if (typeof rawSvg !== "string" || !rawSvg) return null;
    if (utf8BytesOf(rawSvg).byteLength > MAX_SVG_BYTES) return null;
    let tagCount = 0;
    for (let i = 0; i < rawSvg.length; i += 1) {
      if (rawSvg.charCodeAt(i) === 0x3c && ++tagCount > MAX_SVG_TAGS) return null;
    }
    const expanded = decodeNumericEntities(rawSvg);
    if (hasUnsafeSvgContent(rawSvg) || hasUnsafeSvgContent(expanded)) return null;
    const openMatch = /<svg\b([^>]*)>/i.exec(rawSvg);
    if (!openMatch || openMatch[0].endsWith("/>")) return null;
    const contentStart = openMatch.index + openMatch[0].length;
    const closeIdx = rawSvg.toLowerCase().lastIndexOf("</svg>");
    if (closeIdx < contentStart || !rawSvg.slice(contentStart, closeIdx).trim()) return null;
    const dims = parseSvgDimensions(openMatch[1]);
    if (!dims) return null;
    const svg = /\bxmlns\s*=/i.test(openMatch[1])
      ? rawSvg
      : rawSvg.slice(0, openMatch.index + 4) + ' xmlns="http://www.w3.org/2000/svg"' + rawSvg.slice(openMatch.index + 4);
    return { svg, widthPx: dims.widthPx, heightPx: dims.heightPx };
  }

  function decodeSvgDataUrl(url, env) {
    const str = String(url ?? "").trim();
    const comma = str.indexOf(",");
    if (comma < 0) return null;
    const header = str.slice(0, comma).replace(/[\u0000-\u0020]+/g, "").toLowerCase();
    if (!header.startsWith("data:image/svg+xml")) return null;
    const meta = header.slice("data:image/svg+xml".length);
    if (meta !== "" && !meta.startsWith(";")) return null;
    const params = meta ? meta.slice(1).split(";").filter(Boolean) : [];
    const isBase64 = params.includes("base64");
    for (const p of params) {
      if (p !== "base64" && p !== "utf8" && !p.startsWith("charset=")) return null;
    }
    const body = str.slice(comma + 1);
    if (body.length > MAX_SVG_BYTES * 3) return null;
    try {
      let raw;
      if (isBase64) {
        const cleaned = body.replace(/\s+/g, "");
        if (!cleaned || cleaned.length % 4 !== 0 || !/^[A-Za-z0-9+/]+={0,2}$/.test(cleaned)) return null;
        const bytes = decodeBase64Bytes(cleaned, env);
        if (bytes.byteLength > MAX_SVG_BYTES) return null;
        raw = typeof TextDecoder === "function" ? new TextDecoder("utf-8", { fatal: true }).decode(bytes) : String.fromCharCode(...bytes);
      } else {
        raw = /%[0-9a-f]{2}/i.test(body) ? decodeURIComponent(body) : body;
      }
      return validateSvgSource(raw);
    } catch {
      return null;
    }
  }

  async function loadImageElement(dataUrl, env) {
    const Img = env?.Image ?? host.Image ?? globalThis.Image;
    if (typeof Img !== "function") return null;
    return await new Promise((resolve) => {
      const img = new Img();
      let settled = false;
      const finish = (val) => {
        if (settled) return;
        settled = true;
        resolve(val);
      };
      img.onload = () => finish(img);
      img.onerror = () => finish(null);
      img.src = dataUrl;
      if (typeof img.decode === "function") Promise.resolve(img.decode()).then(() => finish(img), () => finish(null));
    });
  }

  async function rasterizeToPngBytes(source, widthPx, heightPx, env) {
    const doc = env?.document ?? host.document;
    const canvas = doc?.createElement?.("canvas");
    const ctx = canvas?.getContext?.("2d");
    if (!ctx) return null;
    const scale = Math.min(1, MAX_CANVAS_SIDE / Math.max(widthPx, heightPx));
    canvas.width = Math.max(1, Math.round(widthPx * scale));
    canvas.height = Math.max(1, Math.round(heightPx * scale));
    let bitmap = null;
    try {
      let drawable = null;
      const makeBitmap = env?.createImageBitmap ?? host.createImageBitmap;
      const BlobCtor = env?.Blob ?? host.Blob ?? globalThis.Blob;
      if (source.kind === "gif") {
        if (typeof makeBitmap === "function" && typeof BlobCtor === "function") {
          try {
            bitmap = await makeBitmap(new BlobCtor([source.bytes], { type: "image/gif" }));
            drawable = bitmap;
          } catch {
            bitmap = null;
          }
        }
        if (!drawable) drawable = await loadImageElement(`data:image/gif;base64,${encodeBytesToBase64(source.bytes, env)}`, env);
      } else {
        const svgBytes = utf8BytesOf(source.svg);
        drawable = await loadImageElement(`data:image/svg+xml;base64,${encodeBytesToBase64(svgBytes, env)}`, env);
        if (!drawable && typeof makeBitmap === "function" && typeof BlobCtor === "function") {
          bitmap = await makeBitmap(new BlobCtor([svgBytes], { type: "image/svg+xml" }));
          drawable = bitmap;
        }
      }
      if (!drawable) return null;
      ctx.drawImage(drawable, 0, 0, canvas.width, canvas.height);
      const blob = await toBlob(canvas);
      return blob ? new Uint8Array(await blob.arrayBuffer()) : null;
    } catch {
      return null;
    } finally {
      bitmap?.close?.();
    }
  }

  // The HTML of a clipboard that carries RTF pictures, and has a picture the page could not open (not a `data:` one).
  function needsRtfImageInlining(html, rtf) {
    if (!html || !rtf || rtf.length > MAX_RTF_CHARS || !rtf.includes("\\pict")) return false;
    return imgTagRanges(html).some(([from, to]) => {
      const tag = html.slice(from, to);
      return SRC_ATTRIBUTE.test(tag) && !DATA_SRC.test(tag);
    });
  }

  function needsAsyncImagePrep(html, rtf) {
    if (needsRtfImageInlining(html, rtf)) return true;
    return Boolean(html && imgTagRanges(html).some(([from, to]) => DATA_GIF_OR_SVG_IMG.test(html.slice(from, to))));
  }

  async function rasterizeDataTag(rawSrc, budget, env) {
    const trimmed = String(rawSrc ?? "").trim();
    if (/^data:image\/gif\s*;\s*base64,/i.test(trimmed)) {
      const cleaned = trimmed.slice(trimmed.indexOf(",") + 1).replace(/\s+/g, "");
      if (!cleaned || cleaned.length > Math.ceil((MAX_IMAGE_BYTES * 4) / 3) + 4) return null;
      let bytes;
      try {
        bytes = decodeBase64Bytes(cleaned, env);
      } catch {
        return null;
      }
      const hdr = parseGifHeader(bytes);
      if (!hdr || budget.count >= MAX_RTF_PICTURES || budget.bytes + bytes.byteLength > MAX_TOTAL_IMAGE_BYTES) return null;
      budget.count += 1;
      budget.bytes += bytes.byteLength;
      return rasterizeToPngBytes({ kind: "gif", bytes }, hdr.widthPx, hdr.heightPx, env);
    }
    if (/^data:image\/svg\+xml(?:;|,)/i.test(trimmed)) {
      const decoded = decodeSvgDataUrl(trimmed, env);
      if (!decoded) return null;
      const inBytes = utf8BytesOf(decoded.svg).byteLength;
      if (budget.count >= MAX_RTF_PICTURES || budget.bytes + inBytes > MAX_TOTAL_IMAGE_BYTES) return null;
      budget.count += 1;
      budget.bytes += inBytes;
      return rasterizeToPngBytes({ kind: "svg", svg: decoded.svg }, decoded.widthPx, decoded.heightPx, env);
    }
    return null;
  }

  async function prepareClipboardHtmlImages(html, rtf, env) {
    let working = needsRtfImageInlining(html, rtf) ? await inlineOfficeClipboardImages(html, rtf, env) : html;
    if (!working) return working;
    const ranges = imgTagRanges(working);
    if (ranges.length === 0) return working;
    let out = "";
    let at = 0;
    const budget = { count: 0, bytes: 0 };
    for (const [from, to] of ranges) {
      const tag = working.slice(from, to);
      out += working.slice(at, from);
      at = to;
      if (!DATA_GIF_OR_SVG_IMG.test(tag)) {
        out += tag;
        continue;
      }
      const srcMatch = SRC_ATTRIBUTE.exec(tag);
      const rawSrc = srcMatch ? (srcMatch[1] ?? srcMatch[2] ?? srcMatch[3] ?? "") : "";
      const pngBytes = rawSrc ? await rasterizeDataTag(rawSrc, budget, env) : null;
      out += pngBytes ? tag.replace(SRC_ATTRIBUTE, () => `src="data:image/png;base64,${encodeBytesToBase64(pngBytes, env)}"`) : tag;
    }
    return out + working.slice(at);
  }

  async function normalizeClipboardImageBytes(bytes, mime, env) {
    const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes || []);
    if (u8.byteLength === 0) return { error: "그림을 넣지 못했습니다: 빈 파일입니다" };
    if (u8.byteLength > MAX_IMAGE_BYTES) return { error: "그림을 넣지 못했습니다: 최대 32 MiB까지 넣을 수 있습니다" };
    const isPng = u8.byteLength >= 8 && u8[0] === 0x89 && u8[1] === 0x50 && u8[2] === 0x4e && u8[3] === 0x47;
    const isJpg = u8.byteLength >= 3 && u8[0] === 0xff && u8[1] === 0xd8 && u8[2] === 0xff;
    if (isPng || isJpg) return { bytes: u8 };
    const isGifMagic = u8.byteLength >= 6 && u8[0] === 0x47 && u8[1] === 0x49 && u8[2] === 0x46;
    if (isGifMagic || String(mime || "").toLowerCase().includes("gif")) {
      const checked = checkGifBytes(u8);
      if (checked.reason === "area") return { error: "그림을 넣지 못했습니다: 그림 화소 수가 한도(5천만)를 넘습니다" };
      if (!checked.header) return { error: "그림을 넣지 못했습니다: 지원하지 않거나 손상된 그림 파일입니다" };
      const png = await rasterizeToPngBytes({ kind: "gif", bytes: u8 }, checked.header.widthPx, checked.header.heightPx, env);
      return png ? { bytes: png } : { error: "그림을 넣지 못했습니다: 지원하지 않거나 손상된 그림 파일입니다" };
    }
    const head = String.fromCharCode(...u8.subarray(0, Math.min(32, u8.byteLength))).trimStart().toLowerCase();
    if (String(mime || "").toLowerCase().includes("svg") || head.startsWith("<svg") || head.startsWith("<?xml")) {
      if (u8.byteLength > MAX_SVG_BYTES) return { error: "그림을 넣지 못했습니다: 지원하지 않거나 손상된 그림 파일입니다" };
      try {
        const raw = typeof TextDecoder === "function" ? new TextDecoder("utf-8", { fatal: true }).decode(u8) : String.fromCharCode(...u8);
        const decoded = validateSvgSource(raw);
        if (!decoded) return { error: "그림을 넣지 못했습니다: 지원하지 않거나 손상된 그림 파일입니다" };
        const png = await rasterizeToPngBytes({ kind: "svg", svg: decoded.svg }, decoded.widthPx, decoded.heightPx, env);
        return png ? { bytes: png } : { error: "그림을 넣지 못했습니다: 지원하지 않거나 손상된 그림 파일입니다" };
      } catch {
        return { error: "그림을 넣지 못했습니다: 지원하지 않거나 손상된 그림 파일입니다" };
      }
    }
    return { error: "그림을 넣지 못했습니다: 지원하지 않거나 손상된 그림 파일입니다" };
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
      const picture = pictures[pictIndex];
      pictIndex += 1;
      const url = picture ? await bytesToDataUrl(picture.bytes, picture.mime, env) : null;
      if (!url) out += tag;
      else if (SRC_ATTRIBUTE.test(tag)) out += tag.replace(SRC_ATTRIBUTE, () => `src="${url}"`);
      else out += tag.replace(/<img\b/i, () => `<img src="${url}"`);
    }
    return out + html.slice(at);
  }

  host.__EASYOFFICE_DOC_CLIPBOARD_IMAGES__ = {
    LIMITS,
    extractEmfDibs,
    pieceBmp,
    extractRtfPictures,
    needsRtfImageInlining,
    inlineOfficeClipboardImages,
    parseGifHeader,
    validateSvgSource,
    decodeSvgDataUrl,
    needsAsyncImagePrep,
    prepareClipboardHtmlImages,
    normalizeClipboardImageBytes,
  };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
