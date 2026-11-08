#!/usr/bin/env node
/**
 * Generates the desktop shell icons (PNG, multi-size ICO for Windows, ICNS for macOS) from the EasyOffice favicon
 * design (`crates/easyoffice-app/ui/favicon.svg`, PD-108: rhwp accent tile #6182D6 -> #3E62BB in the
 * reference shell's icon proportions, a white page with a folded corner and three rounded text
 * lines). The glyph is rasterized analytically in the same 1024-unit coordinates as the SVG with
 * 8x8 supersampling, so no image toolchain is needed and the output is byte-for-byte deterministic.
 *
 *   node scripts/generate-desktop-icons.mjs          # (re)write crates/easyoffice-desktop/icons
 *   node scripts/generate-desktop-icons.mjs --check  # fail if committed icons are out of date
 */

import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const ICON_DIR = path.join(ROOT, "crates/easyoffice-desktop/icons");

// Design units of favicon.svg (viewBox 0 0 1024 1024).
const DESIGN = 1024;
const TILE_TOP = [0x61, 0x82, 0xd6];
const TILE_BOTTOM = [0x3e, 0x62, 0xbb];
const PAPER = [0xff, 0xff, 0xff];
const FOLD = [0xd6, 0xdf, 0xf5];
const LINE = [0x33, 0x50, 0x95];
const LINE_SHORT = [0x61, 0x82, 0xd6];
const TILE = { left: 64, top: 64, right: 960, bottom: 960, radius: 214 };
const PAGE = { left: 316, top: 222, right: 712, bottom: 802, radius: 36 };
// The folded corner: the page is cut along x - y = FOLD_CUT and the triangle right of FOLD_X and
// above FOLD_Y (inside the cut) is the fold.
const FOLD_X = 572;
const FOLD_Y = 362;
const FOLD_CUT = FOLD_X - PAGE.top;
const LINE_HEIGHT = 48;
const LINES = [
  { left: 392, top: 452, width: 240, color: LINE },
  { left: 392, top: 552, width: 240, color: LINE },
  { left: 392, top: 652, width: 150, color: LINE_SHORT },
];

/** Point inside an axis-aligned rectangle with circular corners of `radius`. */
function insideRoundedRect(x, y, { left, top, right, bottom, radius }) {
  if (x < left || y < top || x > right || y > bottom) return false;
  const cx = Math.min(Math.max(x, left + radius), right - radius);
  const cy = Math.min(Math.max(y, top + radius), bottom - radius);
  return (x - cx) ** 2 + (y - cy) ** 2 <= radius * radius;
}

function lineColor(x, y) {
  const half = LINE_HEIGHT / 2;
  const hit = LINES.find((line) =>
    insideRoundedRect(x, y, { left: line.left, top: line.top, right: line.left + line.width, bottom: line.top + LINE_HEIGHT, radius: half })
  );
  return hit ? hit.color : null;
}

/** Vertical gradient of the tile, top to bottom. */
function tileColor(y) {
  const t = Math.min(Math.max((y - TILE.top) / (TILE.bottom - TILE.top), 0), 1);
  return TILE_TOP.map((top, i) => Math.round(top + (TILE_BOTTOM[i] - top) * t));
}

// Same geometry as favicon.svg, evaluated front-to-back.
function sampleColor(x, y) {
  if (insideRoundedRect(x, y, PAGE) && x - y <= FOLD_CUT) {
    if (x >= FOLD_X && y <= FOLD_Y) return FOLD;
    return lineColor(x, y) ?? PAPER;
  }
  if (insideRoundedRect(x, y, TILE)) return tileColor(y);
  return null;
}

const SUPERSAMPLE = 8;

/** Supersamples one pixel; returns summed colour channels and the covered sample count. */
function samplePixel(px, py, scale) {
  const acc = { r: 0, g: 0, b: 0, covered: 0 };
  for (let sy = 0; sy < SUPERSAMPLE; sy += 1) {
    for (let sx = 0; sx < SUPERSAMPLE; sx += 1) {
      const c = sampleColor((px + (sx + 0.5) / SUPERSAMPLE) * scale, (py + (sy + 0.5) / SUPERSAMPLE) * scale);
      if (!c) continue;
      acc.r += c[0];
      acc.g += c[1];
      acc.b += c[2];
      acc.covered += 1;
    }
  }
  return acc;
}

function writePixel(rgba, offset, { r, g, b, covered }) {
  if (covered === 0) return;
  rgba[offset] = Math.round(r / covered);
  rgba[offset + 1] = Math.round(g / covered);
  rgba[offset + 2] = Math.round(b / covered);
  rgba[offset + 3] = Math.round((covered / (SUPERSAMPLE * SUPERSAMPLE)) * 255);
}

function rasterize(size) {
  const rgba = Buffer.alloc(size * size * 4);
  const scale = DESIGN / size;
  for (let py = 0; py < size; py += 1) {
    for (let px = 0; px < size; px += 1) {
      writePixel(rgba, (py * size + px) * 4, samplePixel(px, py, scale));
    }
  }
  return rgba;
}

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (const byte of buf) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const typed = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(typed));
  return Buffer.concat([len, typed, crc]);
}

function scanlines(size) {
  const rgba = rasterize(size);
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let y = 0; y < size; y += 1) {
    raw[y * (size * 4 + 1)] = 0;
    rgba.copy(raw, y * (size * 4 + 1) + 1, y * size * 4, (y + 1) * size * 4);
  }
  return raw;
}

// Decoded IHDR + scanlines of a PNG produced by this script. Comparing decoded pixels (not the
// compressed bytes) keeps `--check` stable across zlib builds of different Node.js versions.
function decodePngPixels(png) {
  let pos = 8;
  let ihdr = null;
  const idat = [];
  while (pos + 8 <= png.length) {
    const len = png.readUInt32BE(pos);
    const type = png.toString("ascii", pos + 4, pos + 8);
    const data = png.subarray(pos + 8, pos + 8 + len);
    if (type === "IHDR") ihdr = Buffer.from(data);
    if (type === "IDAT") idat.push(data);
    pos += 12 + len;
  }
  return Buffer.concat([ihdr || Buffer.alloc(0), zlib.inflateSync(Buffer.concat(idat))]);
}

function decodeIcoPixels(ico) {
  const count = ico.readUInt16LE(4);
  // Directory entries without their byte-size/offset fields (those depend on compression).
  const parts = [ico.subarray(0, 6 + 16 * count).map((b, i) => (i >= 6 && (i - 6) % 16 >= 8 ? 0 : b))];
  for (let i = 0; i < count; i += 1) {
    const len = ico.readUInt32LE(6 + 16 * i + 8);
    const off = ico.readUInt32LE(6 + 16 * i + 12);
    parts.push(decodePngPixels(ico.subarray(off, off + len)));
  }
  return Buffer.concat(parts);
}

// ICNS elements: type code, then the pixel size of the PNG stored under it.
const ICNS_ELEMENTS = [
  ["ic07", 128],
  ["ic08", 256],
  ["ic09", 512],
  ["ic13", 256], // 128x128 @2x
  ["ic14", 512], // 256x256 @2x
];

const ICNS_HEADER_BYTES = 8; // 4-byte type + 4-byte big-endian length

function pngElement(type, data) {
  const head = Buffer.alloc(ICNS_HEADER_BYTES);
  head.write(type, 0, "ascii");
  head.writeUInt32BE(ICNS_HEADER_BYTES + data.length, 4);
  return Buffer.concat([head, data]);
}

// macOS icon container with PNG-compressed elements (macOS 10.7 and later).
function encodeIcns() {
  const body = Buffer.concat(ICNS_ELEMENTS.map(([type, size]) => pngElement(type, encodePng(size))));
  const head = Buffer.alloc(ICNS_HEADER_BYTES);
  head.write("icns", 0, "ascii");
  head.writeUInt32BE(ICNS_HEADER_BYTES + body.length, 4);
  return Buffer.concat([head, body]);
}

function decodeIcnsPixels(icns) {
  const parts = [];
  for (let off = ICNS_HEADER_BYTES; off < icns.length; ) {
    const len = icns.readUInt32BE(off + 4);
    parts.push(icns.subarray(off, off + 4), decodePngPixels(icns.subarray(off + ICNS_HEADER_BYTES, off + len)));
    off += len;
  }
  return Buffer.concat(parts);
}

function encodePng(size) {
  const raw = scanlines(size);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", zlib.deflateSync(raw, { level: 9 })),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

// ICO container with PNG-compressed entries (supported by Windows Vista and later).
function encodeIco(sizes) {
  const images = sizes.map((s) => ({ size: s, data: encodePng(s) }));
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);
  let offset = 6 + 16 * images.length;
  const entries = images.map(({ size, data }) => {
    const e = Buffer.alloc(16);
    e[0] = size >= 256 ? 0 : size;
    e[1] = size >= 256 ? 0 : size;
    e.writeUInt16LE(1, 4); // planes
    e.writeUInt16LE(32, 6); // bits per pixel
    e.writeUInt32LE(data.length, 8);
    e.writeUInt32LE(offset, 12);
    offset += data.length;
    return e;
  });
  return Buffer.concat([header, ...entries, ...images.map((i) => i.data)]);
}

const OUTPUTS = {
  "32x32.png": () => encodePng(32),
  "128x128.png": () => encodePng(128),
  "128x128@2x.png": () => encodePng(256),
  "icon.png": () => encodePng(512),
  "icon.ico": () => encodeIco([16, 24, 32, 48, 64, 128, 256]),
  "icon.icns": () => encodeIcns(),
};

/** Pixel decoder per output extension; PNG is the default. */
const PIXEL_DECODERS = Object.freeze({ ".ico": decodeIcoPixels, ".icns": decodeIcnsPixels });
const checkOnly = process.argv.includes("--check");
const stale = [];
fs.mkdirSync(ICON_DIR, { recursive: true });
for (const [name, build] of Object.entries(OUTPUTS)) {
  const target = path.join(ICON_DIR, name);
  const bytes = build();
  if (checkOnly) {
    const decode = PIXEL_DECODERS[path.extname(name)] ?? decodePngPixels;
    if (!fs.existsSync(target) || !decode(fs.readFileSync(target)).equals(decode(bytes))) stale.push(name);
  } else {
    fs.writeFileSync(target, bytes);
    console.log(`wrote ${path.relative(ROOT, target).replaceAll("\\", "/")} (${bytes.length} bytes)`);
  }
}
if (stale.length > 0) {
  console.error(`Desktop icons are out of date: ${stale.join(", ")}. Run node scripts/generate-desktop-icons.mjs`);
  process.exit(1);
}
if (checkOnly) console.log(`Desktop icons are up to date (${Object.keys(OUTPUTS).length} files).`);
