// Unit and wiring tests for the hwp-studio authoring font policy and font-set dialog stylesheet.
import test from "node:test";
import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { readText } from "./test-support/read-text.mjs";
import {
  AUTHORING_FONT_BLOCKED_BIT,
  AUTHORING_FONT_SERIF_BIT,
  classifyAuthoringFontBits,
  filterAuthoringFontFamilies,
  isAuthoringBlockedFontFamily,
  sanitizeAuthoringFontFamily,
} from "../apps/hwp-studio/src/core/font-authoring-policy.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..");

const BLOCKED_SERIF_FAMILIES = [
  "휴먼명조",
  "휴먼바탕",
  "휴먼궁서",
  "HY견명조",
  "HY신명조",
  "HY궁서",
  "HY궁서B",
  "HY바탕",
  "HYMyeongJo-Extra",
  "hy-myeongjo",
  "HCI Serif",
  "  \"'HY_MyeongJo-Extra'\"  ",
  "\t휴 먼 명 조\n",
];

const BLOCKED_SANS_FAMILIES = [
  "휴먼고딕",
  "휴먼옛체",
  "휴먼매직체",
  "휴먼편지체",
  "휴먼둥근헤드라인",
  "HCI Poppy",
  "HCI Hollyhock",
  "__hci-poppy__",
  "HY헤드라인M",
  "HY견고딕",
  "HY중고딕",
  "HY그래픽",
  "HY그래픽M",
  "HY울릉도M",
  "HYHeadLine-Medium",
  "HYHeadLine M",
  "HYGothic-Extra",
  "HYGraphic-Medium",
  "HYmj",
  "HYgt",
  "HYgp",
  "HYsn",
  "HYsm",
  "\"'HY-HeadLine_Medium'\"",
];

const ALLOWED_FAMILIES = [
  ["함초롬바탕", AUTHORING_FONT_SERIF_BIT],
  ["함초롬돋움", 0],
  ["맑은 고딕", 0],
  ["나눔고딕", 0],
  ["나눔명조", AUTHORING_FONT_SERIF_BIT],
  ["바탕", AUTHORING_FONT_SERIF_BIT],
  ["바탕체", AUTHORING_FONT_SERIF_BIT],
  ["돋움", 0],
  ["돋움체", 0],
  ["굴림", 0],
  ["궁서", AUTHORING_FONT_SERIF_BIT],
  ["궁서체", AUTHORING_FONT_SERIF_BIT],
  ["Pretendard", 0],
  ["Noto Sans KR", 0],
  ["Noto Serif KR", AUTHORING_FONT_SERIF_BIT],
  ["Atkinson Hyperlegible", 0],
  ["Hyperlegible", 0],
  ["Hygiene Sans", 0],
  ["Hyacinth", 0],
  ["Hybrid", 0],
  ["HCL Gothic", 0],
  ["휴식체", 0],
  ["휴가바탕", AUTHORING_FONT_SERIF_BIT],
];

test("isAuthoringBlockedFontFamily blocks restricted commercial prefixes and allows open/system families", () => {
  for (const family of [...BLOCKED_SERIF_FAMILIES, ...BLOCKED_SANS_FAMILIES]) {
    assert.equal(isAuthoringBlockedFontFamily(family), true, `expected blocked: ${family}`);
  }
  for (const [family] of ALLOWED_FAMILIES) {
    assert.equal(isAuthoringBlockedFontFamily(family), false, `expected allowed: ${family}`);
  }
  assert.equal(isAuthoringBlockedFontFamily(""), false);
  assert.equal(isAuthoringBlockedFontFamily("   \t\n "), false);
  assert.equal(isAuthoringBlockedFontFamily(null), false);
  assert.equal(isAuthoringBlockedFontFamily(undefined), false);
});

test("sanitizeAuthoringFontFamily maps blocked families to HCR Batang or HCR Dotum and preserves allowed families", () => {
  for (const family of BLOCKED_SERIF_FAMILIES) {
    assert.equal(sanitizeAuthoringFontFamily(family), "함초롬바탕", `serif fallback for ${family}`);
    assert.equal(
      classifyAuthoringFontBits(family),
      AUTHORING_FONT_BLOCKED_BIT | AUTHORING_FONT_SERIF_BIT,
      `bitmask for ${family}`,
    );
  }
  for (const family of BLOCKED_SANS_FAMILIES) {
    assert.equal(sanitizeAuthoringFontFamily(family), "함초롬돋움", `sans fallback for ${family}`);
    assert.equal(classifyAuthoringFontBits(family), AUTHORING_FONT_BLOCKED_BIT, `bitmask for ${family}`);
  }
  for (const [family, expectedBits] of ALLOWED_FAMILIES) {
    assert.equal(sanitizeAuthoringFontFamily(`  ${family}  `), family);
    assert.equal(classifyAuthoringFontBits(family), expectedBits, `bitmask for ${family}`);
  }
  assert.equal(sanitizeAuthoringFontFamily("   "), "");
  assert.equal(classifyAuthoringFontBits(null), 0);
});

test("filterAuthoringFontFamilies strips blank and blocked entries while keeping allowed families in order", () => {
  const input = [
    "  함초롬바탕 ",
    "HY헤드라인M",
    "",
    "휴먼명조",
    "맑은 고딕",
    "HCI Poppy",
    "HYHeadLine-Medium",
    "나눔명조",
    "   ",
    "Atkinson Hyperlegible",
  ];
  assert.deepEqual(filterAuthoringFontFamilies(input), [
    "함초롬바탕",
    "맑은 고딕",
    "나눔명조",
    "Atkinson Hyperlegible",
  ]);
});

test("hwp-studio wires font-authoring-policy into toolbar and font-loader and imports font-set-dialog.css", () => {
  const styleCss = readText(path.join(REPO, "apps/hwp-studio/src/style.css"));
  assert.match(styleCss, /@import\s+"\.\/styles\/font-set-dialog\.css";/);

  const dialogCss = readText(path.join(REPO, "apps/hwp-studio/src/styles/font-set-dialog.css"));
  assert.match(dialogCss, /\.fs-list-item:hover\s*\{\s*background:\s*var\(--color-hover-bg\);\s*\}/);
  assert.match(dialogCss, /\.fse-row\s*\{[\s\S]*?gap:\s*8px;[\s\S]*?padding:\s*3px\s+0;/);

  const toolbarTs = readText(path.join(REPO, "apps/hwp-studio/src/ui/toolbar.ts"));
  assert.match(toolbarTs, /from\s+'\.\.\/core\/font-authoring-policy'/);
  assert.match(toolbarTs, /filterAuthoringFontFamilies\(getLocalFonts\(\)\)/);
  assert.match(toolbarTs, /const\s+authoringName\s*=\s*sanitizeAuthoringFontFamily\(name\)/);
  assert.match(toolbarTs, /langKeys\.map\(\(key\)\s*=>\s*sanitizeAuthoringFontFamily\(fs\[key\]\)\)/);

  const fontLoaderTs = readText(path.join(REPO, "apps/hwp-studio/src/core/font-loader.ts"));
  assert.match(fontLoaderTs, /from\s+'\.\/font-authoring-policy'/);
  assert.match(fontLoaderTs, /filterAuthoringFontFamilies\(FONT_LIST\.map\(\(?f\)?\s*=>\s*f\.name\)\)/);
  assert.match(fontLoaderTs, /if\s*\(isAuthoringBlockedFontFamily\(name\)\)\s*continue;/);
});
