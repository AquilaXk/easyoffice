# Bundled fonts

Every font shipped with EasyOffice is licensed under the SIL Open Font License 1.1 (OFL). The files are
copied byte for byte from the pinned rhwp checkout (`third_party/rhwp/assets/fonts`), which is what the
vendored editor's font catalog expects; they are not modified, renamed or re-subsetted here. The OFL text
that accompanies each family is in `licenses/` and is also copied next to the fonts in the built editor
bundle (`hwp-studio/fonts/licenses/`). Fonts are never sold on their own.

Integrity and licensing are enforced by `scripts/verify-fonts.mjs` (called from the repository hygiene
guard and the bundle build): every file is listed in `fonts.json` with size and SHA-256, every entry has an
OFL 1.1 text, and a font that is not listed fails the check.

| File | Family | Style | License | Size (MiB) | Source |
|---|---|---|---|---|---|
| D2Coding-Bold.woff2 | D2Coding | Bold | SIL OFL 1.1 (`licenses/D2Coding-OFL.txt`) | 1.51 | naver/d2codingfont |
| D2Coding-Regular.woff2 | D2Coding | Regular | SIL OFL 1.1 (`licenses/D2Coding-OFL.txt`) | 1.42 | naver/d2codingfont |
| GowunBatang-Bold.woff2 | Gowun Batang | Bold | SIL OFL 1.1 (`licenses/GowunBatang-OFL.txt`) | 0.43 | Google Fonts (google/fonts ofl/gowunbatang) |
| GowunBatang-Regular.woff2 | Gowun Batang | Regular | SIL OFL 1.1 (`licenses/GowunBatang-OFL.txt`) | 0.47 | Google Fonts (google/fonts ofl/gowunbatang) |
| GowunDodum-Regular.woff2 | Gowun Dodum | Regular | SIL OFL 1.1 (`licenses/GowunDodum-OFL.txt`) | 0.39 | Google Fonts (google/fonts ofl/gowundodum) |
| NanumGothic-Bold.woff2 | Nanum Gothic | Bold | SIL OFL 1.1 (`licenses/NanumGothic-OFL.txt`) | 0.40 | Google Fonts (google/fonts ofl/nanumgothic) |
| NanumGothic-ExtraBold.woff2 | Nanum Gothic | ExtraBold | SIL OFL 1.1 (`licenses/NanumGothic-OFL.txt`) | 0.36 | Google Fonts (google/fonts ofl/nanumgothic) |
| NanumGothic-Regular.woff2 | Nanum Gothic | Regular | SIL OFL 1.1 (`licenses/NanumGothic-OFL.txt`) | 0.32 | Google Fonts (google/fonts ofl/nanumgothic) |
| NanumGothicCoding-Bold.woff2 | Nanum Gothic Coding | Bold | SIL OFL 1.1 (`licenses/NanumGothicCoding-OFL.txt`) | 0.51 | Google Fonts (google/fonts ofl/nanumgothiccoding) |
| NanumGothicCoding-Regular.woff2 | Nanum Gothic Coding | Regular | SIL OFL 1.1 (`licenses/NanumGothicCoding-OFL.txt`) | 0.50 | Google Fonts (google/fonts ofl/nanumgothiccoding) |
| NanumMyeongjo-Bold.woff2 | Nanum Myeongjo | Bold | SIL OFL 1.1 (`licenses/NanumMyeongjo-OFL.txt`) | 0.55 | Google Fonts (google/fonts ofl/nanummyeongjo) |
| NanumMyeongjo-ExtraBold.woff2 | Nanum Myeongjo | ExtraBold | SIL OFL 1.1 (`licenses/NanumMyeongjo-OFL.txt`) | 0.66 | Google Fonts (google/fonts ofl/nanummyeongjo) |
| NanumMyeongjo-Regular.woff2 | Nanum Myeongjo | Regular | SIL OFL 1.1 (`licenses/NanumMyeongjo-OFL.txt`) | 0.48 | Google Fonts (google/fonts ofl/nanummyeongjo) |
| NotoSansKR-Bold.woff2 | Noto Sans KR | Bold | SIL OFL 1.1 (`licenses/NotoSansKR-OFL.txt`) | 0.53 | Google Fonts (google/fonts ofl/notosanskr) |
| NotoSansKR-ExtraLight.woff2 | Noto Sans KR | ExtraLight | SIL OFL 1.1 (`licenses/NotoSansKR-OFL.txt`) | 0.57 | Google Fonts (google/fonts ofl/notosanskr) |
| NotoSansKR-Regular.woff2 | Noto Sans KR | Regular | SIL OFL 1.1 (`licenses/NotoSansKR-OFL.txt`) | 0.54 | Google Fonts (google/fonts ofl/notosanskr) |
| NotoSerifKR-Bold.woff2 | Noto Serif KR | Bold | SIL OFL 1.1 (`licenses/NotoSerifKR-OFL.txt`) | 0.99 | Google Fonts (google/fonts ofl/notoserifkr) |
| NotoSerifKR-Regular.woff2 | Noto Serif KR | Regular | SIL OFL 1.1 (`licenses/NotoSerifKR-OFL.txt`) | 0.93 | Google Fonts (google/fonts ofl/notoserifkr) |
| Pretendard-Black.woff2 | Pretendard | Black | SIL OFL 1.1 (`licenses/Pretendard-OFL.txt`) | 0.76 | orioncactus/pretendard release (byte-identical woff2) |
| Pretendard-Bold.woff2 | Pretendard | Bold | SIL OFL 1.1 (`licenses/Pretendard-OFL.txt`) | 0.75 | orioncactus/pretendard release (byte-identical woff2) |
| Pretendard-ExtraBold.woff2 | Pretendard | ExtraBold | SIL OFL 1.1 (`licenses/Pretendard-OFL.txt`) | 0.76 | orioncactus/pretendard release (byte-identical woff2) |
| Pretendard-ExtraLight.woff2 | Pretendard | ExtraLight | SIL OFL 1.1 (`licenses/Pretendard-OFL.txt`) | 0.70 | orioncactus/pretendard release (byte-identical woff2) |
| Pretendard-Light.woff2 | Pretendard | Light | SIL OFL 1.1 (`licenses/Pretendard-OFL.txt`) | 0.72 | orioncactus/pretendard release (byte-identical woff2) |
| Pretendard-Medium.woff2 | Pretendard | Medium | SIL OFL 1.1 (`licenses/Pretendard-OFL.txt`) | 0.74 | orioncactus/pretendard release (byte-identical woff2) |
| Pretendard-Regular.woff2 | Pretendard | Regular | SIL OFL 1.1 (`licenses/Pretendard-OFL.txt`) | 0.73 | orioncactus/pretendard release (byte-identical woff2) |
| Pretendard-SemiBold.woff2 | Pretendard | SemiBold | SIL OFL 1.1 (`licenses/Pretendard-OFL.txt`) | 0.75 | orioncactus/pretendard release (byte-identical woff2) |
| Pretendard-Thin.woff2 | Pretendard | Thin | SIL OFL 1.1 (`licenses/Pretendard-OFL.txt`) | 0.66 | orioncactus/pretendard release (byte-identical woff2) |
| SpoqaHanSans-Regular.woff2 | Spoqa Han Sans | Regular | SIL OFL 1.1 (`licenses/SpoqaHanSans-OFL.txt`) | 0.26 | spoqa/spoqa-han-sans |

Total: 28 files, 18.38 MiB (19272084 bytes).

## Notes

- Pretendard woff2 files are byte-identical to the official release. The other families are the web copies
  published in the rhwp repository from the upstream TTF sources; `NotoSansKR-Regular` and `NotoSansKR-ExtraLight`
  are the weight instances of Noto Sans KR that rhwp generated (Regular additionally carries box-drawing and
  geometric-shape glyphs). Several families declare Reserved Font Names (Nanum, D2Coding, Spoqa Han Sans,
  Pretendard); the files keep the original family names and glyph outlines. Whether a pure container
  conversion is a "Modified Version" under OFL section 3 should be confirmed before a public release.
- Not bundled, on purpose: fonts under custom "free to use" terms (Cafe24 Ssurround, Cafe24 Supermagic,
  Happiness Sans), Latin Modern Math (GUST Font License, not OFL), the Source Han Serif K old-Hangul subset
  (a modified subset of a font that reserves the name "Source"), and every Hancom, Microsoft or operating
  system font. Documents that use such fonts fall back to a bundled family or to a font installed on the
  user's PC.
