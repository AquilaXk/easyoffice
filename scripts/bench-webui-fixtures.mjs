// Inputs for scripts/bench-webui.mjs. The workbook is written here (a stored/deflated ZIP of minimal
// OOXML parts) so its size is not limited by the 500,000-item save budget; the document and slide
// inputs are encoded by the running engine from structured content, like a save.
import zlib from "node:zlib";

function zip(entries) {
  const locals = [];
  const centrals = [];
  let offset = 0;
  for (const [name, text] of entries) {
    const raw = Buffer.from(text, "utf8");
    const data = zlib.deflateRawSync(raw, { level: 6 });
    const crc = zlib.crc32(raw);
    const nameBuf = Buffer.from(name);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(8, 8);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(raw.length, 22);
    local.writeUInt16LE(nameBuf.length, 26);
    locals.push(local, nameBuf, data);
    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(20, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(8, 10);
    central.writeUInt32LE(crc, 16);
    central.writeUInt32LE(data.length, 20);
    central.writeUInt32LE(raw.length, 24);
    central.writeUInt16LE(nameBuf.length, 28);
    central.writeUInt32LE(offset, 42);
    centrals.push(central, nameBuf);
    offset += local.length + nameBuf.length + data.length;
  }
  const centralBytes = Buffer.concat(centrals);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(entries.length, 8);
  end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(centralBytes.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, centralBytes, end]);
}

const colLabel = (c) => {
  let n = c;
  let out = "";
  while (n > 0) {
    out = String.fromCharCode(65 + ((n - 1) % 26)) + out;
    n = Math.floor((n - 1) / 26);
  }
  return out;
};

const cellValue = (r, c) => (r * 131 + c * 7) % 5003;

/** `rows` x `cols` numbers; every fourth column (from the 4th) sums the two columns before it. */
export function makeWorkbook(rows, cols) {
  const labels = Array.from({ length: cols + 1 }, (_, c) => colLabel(c));
  const parts = ['<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>'];
  for (let r = 1; r <= rows; r += 1) {
    let row = `<row r="${r}">`;
    for (let c = 1; c <= cols; c += 1) {
      row += c % 4 === 0
        ? `<c><f>${labels[c - 2]}${r}+${labels[c - 1]}${r}</f><v>${cellValue(r, c - 2) + cellValue(r, c - 1)}</v></c>`
        : `<c><v>${cellValue(r, c)}</v></c>`;
    }
    parts.push(row + "</row>");
  }
  parts.push("</sheetData></worksheet>");
  return zip([
    ["[Content_Types].xml", '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>'],
    ["_rels/.rels", '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>'],
    ["xl/workbook.xml", '<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>'],
    ["xl/_rels/workbook.xml.rels", '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>'],
    ["xl/worksheets/sheet1.xml", parts.join("")],
  ]);
}

const WORDS = ["보고", "계획", "예산", "검토", "일정", "성과", "분석", "시장", "고객", "품질", "개선", "목표", "전략", "운영", "자료", "결과"];
const sentence = (i) => Array.from({ length: 18 }, (_, k) => WORDS[(i * 7 + k * 3) % WORDS.length]).join(" ") + ".";

/** Structured content like the page's save payload: `paragraphs` paragraphs plus a table every 40. */
export function documentContent(paragraphs) {
  const blocks = [];
  for (let i = 0; i < paragraphs; i += 1) {
    const runs = [{ text: sentence(i) }];
    if (i % 3 === 0) runs.push({ text: WORDS[i % WORDS.length], bold: true });
    blocks.push({ type: "paragraph", runs, ...(i % 50 === 0 ? { heading: 2 } : {}) });
    if (i % 40 === 39) {
      const rows = Array.from({ length: 4 }, (_, r) => Array.from({ length: 4 }, (_, c) => ({ text: `${WORDS[(i + r + c) % WORDS.length]}-${r}-${c}` })));
      blocks.push({ type: "table", rows });
    }
  }
  return { blocks };
}

export function slidesContent(count) {
  return { slides: Array.from({ length: count }, (_, i) => ({ title: `슬라이드 ${i + 1}`, bullets: Array.from({ length: 5 }, (_, k) => sentence(i + k)), notes: sentence(i) })) };
}

/** Encodes structured content through the engine's save command and returns the file bytes. */
export async function encodeWithEngine(invoke, content, extension) {
  const res = await invoke("convert_document_format", JSON.stringify({ sourceFormat: extension, targetFormat: extension, title: "bench", ...content }));
  if (res.status !== "OK") throw new Error("encode failed: " + JSON.stringify(res).slice(0, 200));
  return Buffer.from(res.convertedBase64, "base64");
}
