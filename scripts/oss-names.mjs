// Regression guard for open-source project names (QA 2026-11-12): documents, comments, tests and log
// strings describe a source by its role and point to THIRD_PARTY_NOTICES.md. The names themselves are
// stored only as SHA-256 digests of the lowercase word, so this file does not contain what it rejects.

import { createHash } from "node:crypto";

/** Names rejected in every tracked text file outside the exempt paths and source headers. */
export const NAME_HASHES = new Set([
  "ae60c9f2c692f51a08cdf4a2e74fc9c1cfca6573dc02848bcd56a4ce35e7531f",
  "86a3381e40ff3052771afc7ad7e945bed670b88072049353b3bce67adb5cdc6b",
  "c84571aacaeb100f48d40177463aff9572b07d2405ce2b23fb8b5424157b143d",
  "2bf7cf76a6c689d745dd4b181ab341c4f93792685f2bb72db5e57856ee48935a",
  "f5d3dd4cbe3bbe127d051bc2b21f3349d2f902e2acde01a48e1862221d7a71aa",
  "4011393f356a69582a25dfd73603b974b88e7df6fec0b75793c6155c365e691c",
  "0e0aa517799753a3604fea6c483a46d67ea8f70c4603d96904522cdc4f34d732",
  "962b721c62e4993ce413b8cdf6a47431b7f2bb672bd8838ff8fa08d006c5def2",
  "e9ea23dc2338535b08b5f64d1aa1741271f0ad71ff531cf68683509f4f8d1175",
  "ac67efeecac10252ee8650d0c88c736f1dfb4c07e8d80077b871f7fba15b95a1",
  "fd62f5a90b38c84574f4f3c69caaa71bd338c0b114749899faf87bdee961c17e",
  "45a5228a1dff9cf91975187ae187f0c3d074500995d043abedf430345e343600",
  "ee9021c84f33366bde80b75efa132836e47afa18b9ce58e8077f4c25d039666c",
  "a5a5f449a249941b91422c26c06608edf82edd8058e718e52531208bcfb5c56b",
  "474318d3a5218b18020cd07c5197cdc1ec472f705ff59e48fc6849f7e75f63e0",
  "f61aa7b2b3ae05e5cc8193e0342812b5b40d07e735c40a0711fa9e63b992b9dd",
  "c92ec3b5a8225e4e7e1b5f157762502fb7220add464fdbd6f08237e93fc87add",
  "b9c24d4ff49fc42477660aa25b4dc871e1b45df6d18fa5813702f3bffa10ad61",
  "1bca179f86fe2cf14eae9905e62fcdb798bf9ecfeacd2661f5481c78e1b5feab",
  "920beb523db2e52b4aef852816b160bd737668b3530bcbbf28f582354870b5eb",
  "a0b56d9f409f682827fb9ce93a7c697f74bfc9d1671522ec543de07c1e136605",
  "33b98ff5d4f01e2df65dcecffc9a6920f6b8bdcaee52e33bec1accd98a144f09",
  "c1fa00cda12647ce3261d2310c286ea3efb80aea78bebf85354feb47d4bec933",
  "96fd1c63dfeae65e4e322d470b76a8e75b114c371035337024363d53a2ab1c16",
  "205634c6dcb323fc879f335b2e64b67d53aa43b28615fbdc8f1b3069c663e63b",
  "da4b50eb5e28a5efd88cfafc59739fac3dfec8d333f418785a08277e9db629a5",
  "faba1e00af8e6c89837f711eeb6f67c0d1bdb573c63abcdcdb2fbf7ab5ad2295",
  "750fda7b74e35f2168cc9d25246d17d22b7c8aafad0395e68a36742ca143c547",
  "9fbf261b62c1d7c00db73afb81dd97fdf20b3442e36e338cb9359b856a03bdc8",
  "9b8f8a7d29275bf790876d4da3b74185e0da91387e2822f0bac7516b6f78297f",
  "88b1deea6a18cfd0d1df1f0048d0946332fe61d1adc2bacb25e5f6a774b56cc5",
  "3b52d97f0c96e295f7110338918f4bdbe41a6fb683641e042281d25c7fcd0501",
  "1510daa2447dfdc6c652ae73c569861c26dc73fd32bdb4eba3b13929afa45999",
  "1926e546e68e62050e4cb3952d185d6173f9056527fea6b3a8bf7c43e4299ef4",
  "e08541a2264a5823ab1b0911556e21da38d91cbe2fbd8902c8c45580ff38c816",
  "57529f20c3adf829c4cc123099e64ce3477c0900df657d9c01ccadb543a5cca7",
]);

/** Crate names rejected in Markdown only; code, Cargo manifests and vendored sources need them to build. */
export const MARKDOWN_NAME_HASHES = new Set([
  "26ad9c47dd4c00ae4de281d2e7608d16f67810ee1b8c5bad301ea780af2df7aa",
  "cba664e970cf9df2f113417f94070f32cfdb7b726f0874348a45f468b9e36598",
  "24d7e1e68f06be6457d28679000bd0c44edbed16d00e4443562a706b6bb2bc37",
  "32b5d76f87c0c77aba334fb607c96e7e2456cc2e509aaa68428dea7054b2715b",
  "c5abcf2eb1193e5891e98dbc1b2e9c5ff1d5417aaffa45b488e08d6a71bb2582",
  "d19a77e4769b8e483607b2f8e24291c1e6bd9227d06eab6692acbec7029eb28d",
  "9237cdac28ccfac7c092f4fadc90a9cac0087d400f43dc32797dc056925a3c5e",
  "f105e0369fb82969b02741f0953a8af23aac0e9ceda5044a36e84c116392ef06",
  "49f475324eb38c9f7a59b7fd7466c8e2b1c05ca0f054867351fefb4ec0871fbe",
  "572c5162ac839524f1c41781395a68d0ed3a7d4ab86d18d7d124cbe3fee987c8",
  "1ea77cdbaa2f2cc9bdc8d58fe0dd3c51ce9d8dae8971e203a395c6fa3fe099c0",
  "c72f91cd98ee720799c06d7891c71f9db0cfb4306809aab645520fd95268b32c",
  "f769de2e06e88974079226ca432883571600d7fd208c1573f8195e56a34a0b67",
  "81ae28ffeb82d02c3cc00a58e0020046ee3633a56c21fab765e7dffc20c0ca0b",
  "e3701be76fe99578448c1cd93edb12fb5a9d27c2d58eaa9e89f880c2875a7537",
  "587a4f56e495ff8516e615421508c6e0b99233b4f912f747ed19b2df5ff76590",
  "bd8dc2d96c00a89e8bf78cfdb589d6a771a9e5c415ba771c05b16798f86fd185",
  "7f4ba095654cb6ee5cfa4f5a2fe18a0ba61edffd5e53f9c464e6334d06119aa0",
  "d04215fa6330906a6b1be2e8be0464aeafca97ae253672e991e93a56bef76aa4",
  "2077936fddd31f98439650d12f2b041313ac0c4234d9d07c5ad132a538c80342",
  "b8634f81755433276b074d976b20f1d773a784f446ddbf52a42514338938554c",
  "7e5756ac471926973f76a8f29d594b2632503112c76887fb82ba815c2c99ee63",
  "575d03df813bf3a1c9e27cc77869e2a87768486c6a86ef8c46f3fd24db1eafc6",
  "46d48d67bdfa624f83416bd5a6ed8bc8833fcc939040d6e8e555d7b3ab309f73",
  "c285445dd957560e9374d73e70ae4e71a41bf775590dbf7b08b35ba68793a2ce",
  "529af2725c5f0293b15cc3b4f546eb913b7274715548d74febeb82a615ef4fcc",
  "b8b21232d83177bf10c791d2513f28bf65b8c0b0a4fdc1fbbad77e6db10fec05",
  "d686f21395867f07c8954937f50b717e3ae11ce76e827cd03f60ad7687dba475",
]);

// Paths that carry names by obligation: license notices, vendored or submodule sources, font provenance,
// and the About dialog license table (checked against THIRD_PARTY_NOTICES.md by test-about-notice.mjs).
export const EXEMPT_BASENAMES = ["Cargo.toml", "Cargo.lock"];

export const EXEMPT_PREFIXES = [
  "THIRD_PARTY_NOTICES.md",
  "third_party/",
  "vendor/",
  "assets/fonts/",
  "crates/easyoffice-app/ui/index.html",
];

// Standard identifiers (PDF character collection and CMap names, a PDF dictionary value, an OLE2 stream
// name of the binary presentation format) that spell a product or company name inside a format
// specification; Base64 so they stay out of the source text.
export const STANDARD_IDENTIFIERS_B64 = [
  "QWRvYmUtS29yZWEx",
  "QWRvYmUtSWRlbnRpdHktVUNT",
  "QWRvYmUtSWRlbnRpdHktMA==",
  "L1JlZ2lzdHJ5IChBZG9iZSk=",
  "UG93ZXJQb2ludCBEb2N1bWVudA==",
];

// A file that adapts a published implementation names it in its leading comment block (license and
// copyright duty). Only that block is exempt, and only when it states the adaptation.
const ATTRIBUTION_MARKER = /참조 구현|구조 참조|reference implementation|adapted from/i;
const COMMENT_LINE = /^\s*(\/\/|\/\*|\*|#)/;
const STANDARD_IDENTIFIERS = STANDARD_IDENTIFIERS_B64.map((b64) => Buffer.from(b64, "base64").toString("utf8"));

// A Rust symbol path (`crate::module::item`) in a profile table is a code identifier, not prose.
const SYMBOL_PATH_PREFIX = /\b[A-Za-z0-9_]+(?=::)/g;

const digest = (word) => createHash("sha256").update(word.toLowerCase()).digest("hex");

/** Content with its attribution header (if any) blanked, so line numbers stay the same. */
export function withoutAttributionHeader(content) {
  const lines = content.split("\n");
  let end = 0;
  while (end < lines.length && COMMENT_LINE.test(lines[end])) end += 1;
  if (!ATTRIBUTION_MARKER.test(lines.slice(0, end).join("\n"))) return content;
  return [...lines.slice(0, end).map(() => ""), ...lines.slice(end)].join("\n");
}

/** Every word a run of word characters can be read as: the whole run and each part cut at `.` or `-`. */
function* wordCandidates(run) {
  const parts = run.split(/([.-])/);
  for (let from = 0; from < parts.length; from += 2) {
    for (let to = from; to < parts.length; to += 2) yield parts.slice(from, to + 1).join("");
  }
}

/** 1-based line numbers and digest prefixes of rejected names in `content` of the tracked file `rel` (one hit per word run). */
export function ossNameHits(rel, content) {
  if (EXEMPT_BASENAMES.includes(rel.split("/").pop())) return [];
  if (EXEMPT_PREFIXES.some((prefix) => rel === prefix || rel.startsWith(prefix))) return [];
  const markdown = rel.endsWith(".md");
  const rejected = markdown ? [NAME_HASHES, MARKDOWN_NAME_HASHES] : [NAME_HASHES];
  let text = withoutAttributionHeader(content);
  if (markdown) text = text.replace(SYMBOL_PATH_PREFIX, "");
  for (const identifier of STANDARD_IDENTIFIERS) text = text.split(identifier).join("");
  const hits = [];
  text.split("\n").forEach((line, index) => {
    for (const [run] of line.matchAll(/[A-Za-z0-9_.-]+/g)) {
      const hash = [...wordCandidates(run)].map(digest).find((d) => rejected.some((set) => set.has(d)));
      if (hash) hits.push({ line: index + 1, hash: hash.slice(0, 8) });
    }
  });
  return hits;
}
