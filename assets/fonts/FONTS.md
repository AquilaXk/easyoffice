# Bundled fonts

Every font shipped with EasyOffice is licensed under the SIL Open Font License 1.1 (OFL). Families whose
license reserves a font name (Nanum, D2Coding, Spoqa Han Sans, Pretendard) are the official distribution files
of each family, either unchanged or with WOFF2 compression only (font tables identical to the official TTF
except the `head` flags/checksum and the dropped `DSIG`, which WOFF2 requires; OFL FAQ 2.2.1, PD-092). The other
families are copied byte for byte from the pinned rhwp checkout (`third_party/rhwp/assets/fonts`). File names
match what the vendored editor's font catalog expects. The OFL text
that accompanies each family is in `licenses/` and is also copied next to the fonts in the built editor
bundle (`hwp-studio/fonts/licenses/`). Fonts are never sold on their own.

Integrity and licensing are enforced by `scripts/verify-fonts.mjs` (called from the repository hygiene
guard and the bundle build): every file is listed in `fonts.json` with size and SHA-256, every entry has an
OFL 1.1 text, and a font that is not listed fails the check. When the license reserves a font name, the entry
must also record `upstream` (official https URL, SHA-256 of the official file, and `conversion`: `none`, where
the bundled file must equal the official file, or `woff2-compression`).

| File | Family | Style | License | Size (MiB) | Source |
|---|---|---|---|---|---|
| D2Coding-Bold.woff2 | D2Coding | Bold | SIL OFL 1.1 (`licenses/D2Coding-OFL.txt`) | 1.70 | [D2CodingBold-Ver1.3.2-20180524.ttf](https://github.com/naver/d2codingfont/releases/download/VER1.3.2/D2Coding-Ver1.3.2-20180524.zip) in the official release zip (official TTF, WOFF2 compression only) |
| D2Coding-Regular.woff2 | D2Coding | Regular | SIL OFL 1.1 (`licenses/D2Coding-OFL.txt`) | 1.60 | [D2Coding-Ver1.3.2-20180524.ttf](https://github.com/naver/d2codingfont/releases/download/VER1.3.2/D2Coding-Ver1.3.2-20180524.zip) in the official release zip (official TTF, WOFF2 compression only) |
| GowunBatang-Bold.woff2 | Gowun Batang | Bold | SIL OFL 1.1 (`licenses/GowunBatang-OFL.txt`) | 0.43 | Google Fonts (google/fonts ofl/gowunbatang) |
| GowunBatang-Regular.woff2 | Gowun Batang | Regular | SIL OFL 1.1 (`licenses/GowunBatang-OFL.txt`) | 0.47 | Google Fonts (google/fonts ofl/gowunbatang) |
| GowunDodum-Regular.woff2 | Gowun Dodum | Regular | SIL OFL 1.1 (`licenses/GowunDodum-OFL.txt`) | 0.39 | Google Fonts (google/fonts ofl/gowundodum) |
| NanumGothic-Bold.woff2 | Nanum Gothic | Bold | SIL OFL 1.1 (`licenses/NanumGothic-OFL.txt`) | 0.42 | [NanumGothic-Bold.ttf](https://raw.githubusercontent.com/google/fonts/main/ofl/nanumgothic/NanumGothic-Bold.ttf) (official TTF, WOFF2 compression only) |
| NanumGothic-ExtraBold.woff2 | Nanum Gothic | ExtraBold | SIL OFL 1.1 (`licenses/NanumGothic-OFL.txt`) | 0.38 | [NanumGothic-ExtraBold.ttf](https://raw.githubusercontent.com/google/fonts/main/ofl/nanumgothic/NanumGothic-ExtraBold.ttf) (official TTF, WOFF2 compression only) |
| NanumGothic-Regular.woff2 | Nanum Gothic | Regular | SIL OFL 1.1 (`licenses/NanumGothic-OFL.txt`) | 0.35 | [NanumGothic-Regular.ttf](https://raw.githubusercontent.com/google/fonts/main/ofl/nanumgothic/NanumGothic-Regular.ttf) (official TTF, WOFF2 compression only) |
| NanumGothicCoding-Bold.woff2 | Nanum Gothic Coding | Bold | SIL OFL 1.1 (`licenses/NanumGothicCoding-OFL.txt`) | 0.76 | [NanumGothicCoding-Bold.ttf](https://raw.githubusercontent.com/google/fonts/main/ofl/nanumgothiccoding/NanumGothicCoding-Bold.ttf) (official TTF, WOFF2 compression only) |
| NanumGothicCoding-Regular.woff2 | Nanum Gothic Coding | Regular | SIL OFL 1.1 (`licenses/NanumGothicCoding-OFL.txt`) | 0.73 | [NanumGothicCoding-Regular.ttf](https://raw.githubusercontent.com/google/fonts/main/ofl/nanumgothiccoding/NanumGothicCoding-Regular.ttf) (official TTF, WOFF2 compression only) |
| NanumMyeongjo-Bold.woff2 | Nanum Myeongjo | Bold | SIL OFL 1.1 (`licenses/NanumMyeongjo-OFL.txt`) | 0.58 | [NanumMyeongjo-Bold.ttf](https://raw.githubusercontent.com/google/fonts/main/ofl/nanummyeongjo/NanumMyeongjo-Bold.ttf) (official TTF, WOFF2 compression only) |
| NanumMyeongjo-ExtraBold.woff2 | Nanum Myeongjo | ExtraBold | SIL OFL 1.1 (`licenses/NanumMyeongjo-OFL.txt`) | 0.69 | [NanumMyeongjo-ExtraBold.ttf](https://raw.githubusercontent.com/google/fonts/main/ofl/nanummyeongjo/NanumMyeongjo-ExtraBold.ttf) (official TTF, WOFF2 compression only) |
| NanumMyeongjo-Regular.woff2 | Nanum Myeongjo | Regular | SIL OFL 1.1 (`licenses/NanumMyeongjo-OFL.txt`) | 0.51 | [NanumMyeongjo-Regular.ttf](https://raw.githubusercontent.com/google/fonts/main/ofl/nanummyeongjo/NanumMyeongjo-Regular.ttf) (official TTF, WOFF2 compression only) |
| NotoSansKR-Bold.woff2 | Noto Sans KR | Bold | SIL OFL 1.1 (`licenses/NotoSansKR-OFL.txt`) | 0.53 | Google Fonts (google/fonts ofl/notosanskr) |
| NotoSansKR-ExtraLight.woff2 | Noto Sans KR | ExtraLight | SIL OFL 1.1 (`licenses/NotoSansKR-OFL.txt`) | 0.57 | Google Fonts (google/fonts ofl/notosanskr) |
| NotoSansKR-Regular.woff2 | Noto Sans KR | Regular | SIL OFL 1.1 (`licenses/NotoSansKR-OFL.txt`) | 0.54 | Google Fonts (google/fonts ofl/notosanskr) |
| NotoSerifKR-Bold.woff2 | Noto Serif KR | Bold | SIL OFL 1.1 (`licenses/NotoSerifKR-OFL.txt`) | 0.99 | Google Fonts (google/fonts ofl/notoserifkr) |
| NotoSerifKR-Regular.woff2 | Noto Serif KR | Regular | SIL OFL 1.1 (`licenses/NotoSerifKR-OFL.txt`) | 0.93 | Google Fonts (google/fonts ofl/notoserifkr) |
| PretendardVariable.woff2 | Pretendard | Variable (wght 100-900) | SIL OFL 1.1 (`licenses/Pretendard-OFL.txt`) | 1.96 | [web/variable/woff2/PretendardVariable.woff2](https://github.com/orioncactus/pretendard/releases/download/v1.3.9/Pretendard-1.3.9.zip) in the official v1.3.9 release zip (byte-identical) |
| SpoqaHanSans-Regular.woff2 | Spoqa Han Sans | Regular | SIL OFL 1.1 (`licenses/SpoqaHanSans-OFL.txt`) | 0.29 | [SpoqaHanSansRegular.woff2](https://raw.githubusercontent.com/spoqa/spoqa-han-sans/master/Subset/SpoqaHanSans/SpoqaHanSansRegular.woff2) (byte-identical) |

Total: 20 files, 14.82 MiB (15544008 bytes).

## Notes

- Pretendard is one variable woff2 (weights 100 to 900) instead of nine static files (PD-298). The editor's font
  catalog still names one file per weight, so `apps/hwp-studio/src/core/pretendard-variable.ts` serves each name
  ("Pretendard Bold" and so on) from the variable file with the FontFace weight descriptor pinned to that weight.
- Reserved-name families: Pretendard and Spoqa Han Sans are the official woff2 files unchanged; Nanum Gothic,
  Nanum Myeongjo and Nanum Gothic Coding are WOFF2 compressions of the TTFs in google/fonts, and D2Coding of the
  TTFs in the official 1.3.2 release zip (the earlier rhwp copies were subsets, i.e. Modified Versions, #144).
  Nanum Gothic Coding and D2Coding use the WOFF2 null `glyf` transform so the decoded `glyf`/`loca` tables are
  byte-identical to the official TTF. The other families are the web copies published in the rhwp repository; `NotoSansKR-Regular` and `NotoSansKR-ExtraLight`
  are the weight instances of Noto Sans KR that rhwp generated (Regular additionally carries box-drawing and
  geometric-shape glyphs). Noto Sans KR's license reserves only the name "Source",
  which this family does not use.
- Not bundled, on purpose: fonts under custom "free to use" terms (Cafe24 Ssurround, Cafe24 Supermagic,
  Happiness Sans), Latin Modern Math (GUST Font License, not OFL), the Source Han Serif K old-Hangul subset
  (a modified subset of a font that reserves the name "Source"), and every HWP default, commercial or operating
  system font. Documents that use such fonts fall back to a bundled family or to a font installed on the
  user's PC.
