// `--name: value` declarations of a CSS block, split on the punctuation instead of a regex.
export function cssDeclarations(block) {
  const out = [];
  let text = block;
  for (let start = text.indexOf("/*"); start >= 0; start = text.indexOf("/*")) {
    const end = text.indexOf("*/", start + 2);
    text = text.slice(0, start) + (end < 0 ? "" : text.slice(end + 2));
  }
  for (const part of text.split(/[;{}]/)) {
    const colon = part.indexOf(":");
    if (colon < 0) continue;
    const name = part.slice(0, colon).trim();
    if (name.startsWith("--")) out.push([name, part.slice(colon + 1).trim()]);
  }
  return out;
}
