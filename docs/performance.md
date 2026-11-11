# 성능 측정과 병목 (참조 앱 대비, 경로별 프로파일)

이 문서는 측정 결과와 최적화 후보를 적는다. 셸 PDF 내보내기의 후보 2·3·4와 병목 1~4는 PD-163으로 처리했고 결과는 4.1절에 있다. 문서→PDF 경로(PD-167)의 측정은 4.2절에 있다. 병목 10(ODS 열기)과 후보 8, DOCX·ODT·HWPX·XLSX 열기의 XML 읽기는 PD-168로 처리했고 결과는 4.3절에 있다. 시트 조건부 서식·데이터 유효성 평가(PD-169)의 측정은 4.5절에 있다. 나머지는 아직 구현하지 않았다. 아래 수치는 모두 이 저장소의 측정 하네스로 직접 잰 값이고, 못 잰 것은 "못 잼"으로 적었다. 측정 기준 커밋은 `bac25cf`(main)이다. 그 뒤 main에 올라온 커밋(`f729ebb`까지)은 시트·XLS 저장, Pages 읽기, 웹 UI를 바꿨고 PDF·미리보기·Quick Look 경로는 건드리지 않아 다시 재지 않았다(XLS 저장·시트 정렬 값은 달라졌을 수 있다).

## 1. 측정 방법과 환경

### 환경

| 항목 | 값 |
|---|---|
| 기기 | Apple M4 Pro, 12코어, 24 GiB |
| OS | macOS 27.0.1 |
| Rust | 로컬 rustc 1.98.1(릴리스 CI 고정값은 1.93.1이라 같지 않음), `cargo test --release`(기본 release 프로파일, 디버그 정보 없음) |
| rhwp | 고정 커밋 `1a76570`(0.8.7). 참조 앱이 쓰는 rhwp도 같은 커밋이라 엔진은 같다 |
| 입력 | `third_party/rhwp/rhwp-studio/public/samples`의 HWP·HWPX, 합성 300쪽 보고서(`pdf_export.rs`의 `report_page_svg`), 엔진용 합성 문서·시트·슬라이드 |

**측정 중 다른 세션이 같은 기기를 썼다.** 1차 측정은 load average 12~35, 2차는 40~57이었다(12코어). 벽시계 시간은 부하에 따라 흔들리고, 특히 여러 스레드를 쓰는 우리 PDF 경로가 더 크게 흔들린다. 그래서 두 번 잰 값을 모두 적고, 2차부터 `time -l`의 CPU 시간(user+sys)을 함께 적었다. CPU 시간은 부하의 영향을 덜 받는다. 한 줄 요약의 비율은 "부하가 낮을 때 벽시계 비"와 "CPU 시간 비"의 범위로 읽어야 한다.

### 지표

- 시간: 케이스마다 새 프로세스를 5번 띄워 `total_ms`의 중앙값. 폰트 DB 만들기 같은 프로세스당 한 번 비용도 들어 있다(앱에서는 한 번만 낸다). 단계별(`phases`)도 중앙값.
- 최대 RSS: `/usr/bin/time -l`의 `maximum resident set size` 중앙값. 시험 바이너리 자체가 10 MB 안팎(Tauri 링크)이라 작은 문서의 RSS는 바탕값이 섞여 있다.
- 출력 크기: 결과 바이트.

### 하네스 (빠른 CI에는 넣지 않음)

| 구성 | 파일 |
|---|---|
| 데스크톱 셸 경로(`#[ignore]` 시험) | `crates/easyoffice-desktop/src/perf_bench.rs` |
| 엔진 열기·저장·재계산(`#[ignore]`) | `crates/easyoffice-app/src/perf_bench.rs` |
| Quick Look 경로(`#[ignore]`) | `apps/quicklook/rust/src/bench_tests.rs` |
| 실행기(N회, `time -l`, 중앙값) | `scripts/bench-run.mjs`, `scripts/bench-support.mjs` |
| 프로파일(`sample` + 상위 함수) | `scripts/bench-profile.mjs` |

```bash
node scripts/checkout-rhwp.mjs                         # 샘플 문서
node scripts/bench-run.mjs --runs 5 --suite hwp --json out.json
node scripts/bench-run.mjs --runs 5 --suite synthetic
node scripts/bench-run.mjs --runs 5 --suite engine
CARGO_TARGET_DIR=target-ql node scripts/bench-run.mjs --runs 5 --suite quicklook   # 별도 워크스페이스
node scripts/bench-profile.mjs --suite desktop --case pdf_doc --input <hwp> --repeat 12 --seconds 7
```

프로파일은 macOS `sample`(1 ms 간격)과 `cargo install rustfilt`가 필요하다. 비율은 "바쁜 스레드의 샘플 중"이며 스레드를 더하므로, 워커 4개에서 도는 함수는 한 스레드 시간의 100%를 넘을 수 있다(포함 시간).

### 참조 앱과 같은 조건으로 맞춘 방법

참조 앱은 GUI 앱이라 실행하지 않았다. 대신 그 Rust 쪽이 하는 일을 헤드리스로 다시 돌렸다. 두 가지 방식을 썼고, 두 값은 서로 맞아 떨어졌다(표의 `ref`와 `ref-repro`).

1. **참조 앱 사본에서 직접 빌드(`ref`)**: 사본의 문서 어댑터 크레이트와 `third_party/rhwp`(사본에서는 비어 있어 우리 고정 체크아웃을 연결, 같은 커밋)에 경로로 의존하는 일회용 하네스를 만들었다. 글꼴 대체 표(`pdf_font_fallbacks.rs`)와 Quick Look PDF 조립(`pdf.rs`)은 사본의 원본 파일을 그대로 포함했고, 문서 열기(`editable_core_from_bytes`: 파싱 후 `convert_to_editable_native`)와 `atomic_write`는 사본 코드를 옮겨 적었다. 참조 앱의 Quick Look은 `native-skia`(Skia 바이너리를 내려받아 링크)로 빌드했다. 이 하네스는 참조 앱 이름이 들어가 있어 저장소에 넣지 않았다. 같은 `BENCH ...` 한 줄을 찍는 바이너리를 `--ref-bin`/`--ref-skia-bin`으로 넘기면 `bench-run.mjs`가 그대로 쓴다.
2. **저장소 안 재현(`ref-repro`)**: `perf_bench.rs`의 `ref:` 케이스가 같은 rhwp 호출 순서(`from_bytes` → `convert_to_editable_native` → `render_page_svg_native` → `svgs_to_pdf_with_options`)를 돌린다. 글꼴 대체는 우리 `add_font_fallbacks`를 쓴다.

### 비교의 한계

- 참조 앱의 홈 미리보기는 첫 쪽 SVG 문자열까지만 만들고 래스터는 웹뷰가 한다. 우리는 Rust에서 PNG까지 만든다. 그래서 미리보기 행은 같은 일이 아니다(아래 표에 따로 적음).
- 저장 검증은 범위가 다르다. 참조 앱은 저장 바이트를 rhwp로 다시 파싱하고, 우리는 컨테이너 구조만 검사한다(`save_verify.rs` 머리 주석). 빠른 것이 곧 더 좋은 것은 아니다.
- 한글 편집기(쪽 SVG 생성)는 두 앱 모두 웹뷰 안의 같은 rhwp wasm이라 비교하지 않았다. 웹뷰·WebKit 구간은 못 잼.
- Quick Look 시험은 앱 번들 밖에서 도는 헤드리스 호출이라 번들 글꼴(`pdf-fonts`)이 없고 시스템 글꼴만 쓴다. 번들 글꼴이 있을 때의 수치는 못 잼.
- 엔진 입력(DOCX·XLSX·PPTX·ODF·PDF 등)은 이 저장소의 인코더로 만든 합성 파일이다. 실제 사용자 문서(DOCX·XLSX·PPTX·PDF 원본)는 저장소에 없어 못 쟀다. 외부가 만든 PDF를 여는 경로도 못 잼.
- Windows·Linux는 못 잼(macOS만).

## 2. 참조 앱 대비

판정은 중앙값 차이가 10%를 넘으면 빠름/느림, 아니면 같음으로 했고, 몇 ms 이하의 절대 차이는 측정 한계로 보고 같음으로 봤다.

### 2.1 PDF 내보내기 (쪽 SVG에서 검색 가능한 PDF까지)

두 앱이 같은 쪽 SVG를 받는다. 우리는 `PdfAssembler`(쪽 병렬, 글꼴 한 번 매핑), 참조 앱은 rhwp `svgs_to_pdf_with_options`(순차)다. 벽시계 ms, 괄호 안은 2차(부하 높음) 값, CPU는 2차의 user+sys.

| 입력 | 쪽 | 우리 벽시계 | 참조 앱 벽시계 | 판정 | 우리 CPU | 참조 CPU | 우리 RSS | 참조 RSS | 출력(우리/참조 KB) |
|---|---:|---:|---:|---|---:|---:|---:|---:|---|
| shift-return.hwp(작음) | 1 | 26 (44) | 27 (29) | 같음 | 30 | 20 | 44 MB | 35 MB | 12 / 12 |
| blank2010.hwp(빈 문서) | 1 | 24 (29) | 22 (21) | 같음 | 30 | 10 | 17 MB | 9 MB | 1 / 2 |
| biz_plan.hwp | 6 | 80 (96) | 710 (1032) | 빠름 8.8~10.7배 | 220 | 870 | 426 MB | 207 MB | 380 / 380 |
| form-002.hwpx(표) | 10 | 269 (334) | 4210 (4380) | 빠름 13~16배 | 840 | 4340 | 735 MB | 247 MB | 1249 / 1141 |
| kps-ai.hwp(긴 문서) | 77 | 798 (1480) | 11389 (11470) | 빠름 7.7~14배 | 2470 | 11370 | 778 MB | 385 MB | 4854 / 4859 |
| issue1949(표 많음) | 115 | 1890 (2554) | 22596 (18436) | 빠름 7.2~12배 | 5010 | 15170 | 439 MB | 214 MB | 6870 / 6870 |
| KTX.hwp(그림 많음) | 1 | 234 (214) | 1816 (2101) | 빠름 7.8~9.8배 | 200 | 1310 | 293 MB | 310 MB | 13114 / 13114 |
| number-bullet.hwp | 1 | 102 (100) | 350 (432) | 빠름 3.4~4.3배 | 100 | 360 | 221 MB | 134 MB | 152 / 152 |
| 합성 300쪽 | 300 | 1462 (1391) | 97410 | 빠름 약 67~70배 | 4870 | 못 잼 | 425 MB | 97 MB | 11530 / 11243 |

- 합성 300쪽의 참조 앱 값은 5회 중앙값 한 번뿐이다(저장소 안 재현은 149841 ms). 부하가 높을 때 쟀고, CPU 시간은 기록하지 못했다.
- **판정: 2쪽 이상의 문서에서는 빠름(벽시계 7배 이상, CPU 3~6.5배 이상). 1쪽짜리 작은 문서는 같음.** 최대 RSS는 쪽이 많을수록 1.7~3.2배 크다(우리가 더 큼). 출력은 거의 같다.
- 참조 앱이 느린 이유는 우리 프로파일에서도 보인다. 글자 하나를 처리할 때마다 글꼴 파일을 열고 매핑하고 해제한다(참조 앱 PDF 프로파일 샘플의 약 70%가 `open`·`mmap`·`munmap`). 우리는 `with_mapped_faces`로 한 번만 매핑한다(PD-114).

### 2.2 저장 검증 (참조 앱: 전체 재파싱, 우리: 컨테이너 검사)

| 입력 | 쪽 | 우리 | 참조 앱 | 판정 |
|---|---:|---:|---:|---|
| shift-return.hwp | 1 | 0.1 ms | 0.6 ms | 같음(측정 한계) |
| biz_plan.hwp | 6 | 0.1 ms | 5.2 ms | 빠름(검사 범위가 다름) |
| form-002.hwpx | 10 | 0.6 ms | 20 ms | 빠름(검사 범위가 다름) |
| kps-ai.hwp | 77 | 0.1 ms | 83 ms | 빠름(검사 범위가 다름) |
| issue1949 | 115 | 0.2 ms | 609 ms | 빠름(검사 범위가 다름) |
| KTX.hwp | 1 | 0.2 ms | 9.7 ms | 빠름(검사 범위가 다름) |

### 2.3 홈 미리보기·저장 때 미리보기 그림 (같은 일이 아님)

우리는 문서에 들어 있는 미리보기 그림을 먼저 읽고(0.0~0.2 ms), 없을 때만 첫 쪽을 그린다. 참조 앱은 항상 첫 쪽 SVG를 만든다. 아래는 그리는 경로의 값이다(`preview_render`: 파싱 + 첫 쪽 SVG + 480 px PNG).

| 입력 | 쪽 | 우리(PNG까지) | 단계(폰트 DB / 파싱 / SVG / PNG) | 참조 앱(SVG까지) | 판정 |
|---|---:|---:|---|---:|---|
| shift-return.hwp | 1 | 28 ms | 26 / 1 / 0 / 1 | 0.9 ms | 비교 불가(우리는 PNG까지, 폰트 DB는 프로세스당 한 번) |
| biz_plan.hwp | 6 | 29 ms | 21 / 5 / 1 / 2 | 5.8 ms | 비교 불가 |
| form-002.hwpx | 10 | 70 ms | 20 / 20 / 5 / 25 | 24 ms | 비교 불가 |
| kps-ai.hwp | 77 | 115 ms | 27 / 81 / 2 / 4 | 85 ms | 같음(둘 다 rhwp 파싱이 대부분) |
| issue1949 | 115 | 684 ms | 27 / 569 / 49 / 28 | 618~1260 ms | 같음(파싱이 대부분) |
| KTX.hwp(그림·글자 많음) | 1 | 132 ms | 31 / 10 / 26 / 64 | 33 ms | 비교 불가(래스터가 우리 몫) |
| number-bullet.hwp | 1 | 61 ms | 30 / 2 / 3 / 26 | 4.6 ms | 비교 불가 |

저장할 때 미리보기 그림을 넣는 단계(`thumb_embed`, 파싱 + 첫 쪽 SVG + PNG + 삽입 + 검증)는 위와 거의 같다(24~671 ms, 쪽이 많은 표 문서는 파싱이 대부분). 참조 앱에는 같은 기능이 없다.

### 2.4 Quick Look (우리: SVG를 usvg·resvg·svg2pdf로, 참조 앱: Skia로 PNG)

단일 스레드 경로라 부하 영향이 작다(2차 값). 참조 앱의 PDF는 쪽마다 PNG(최대 2048 px)를 만들어 이미지 PDF로 묶고, 우리는 쪽 SVG를 벡터로 변환한다(글자는 윤곽선).

| 입력 | 쪽 | 첫 쪽 PNG 우리 / 참조 | 판정 | 미리보기 PDF 우리 / 참조 | 판정 | PDF 크기 우리 / 참조 KB |
|---|---:|---|---|---|---|---|
| shift-return.hwp | 1 | 29 / 40 ms | 같음 | 20 / 57 ms | 빠름 | 50 / 12 |
| blank2010.hwp | 1 | 20 / 25 ms | 같음 | 13 / 41 ms | 빠름 | 2 / 7 |
| biz_plan.hwp | 6 | 37 / 51 ms | 빠름 | 892 / 247 ms | 느림 3.6배 | 2817 / 342 |
| form-002.hwpx | 10 | 279 / 59 ms | 느림 4.7배 | 3215 / 433 ms | 느림 7.4배 | 10584 / 1644 |
| kps-ai.hwp | 77(우리는 64쪽까지) | 137 / 110 ms | 느림 1.2배 | 6927 / 2409 ms | 느림 2.9배 | 32682 / 7390 |
| issue1949 | 115(우리는 64쪽까지) | 756 / 1142 ms | 같음~빠름 | 6941 / 6625 ms | 같음 | 32484 / 12583 |
| KTX.hwp | 1 | 456 / 71 ms | 느림 6.4배 | 541 / 99 ms | 느림 5.5배 | 13250 / 123 |
| number-bullet.hwp | 1 | 181 / 42 ms | 느림 4.3배 | 306 / 72 ms | 느림 4.3배 | 1277 / 112 |

- 임베디드 미리보기 그림 읽기(`ql_embedded`)는 0.1~0.2 ms이고, 참조 앱도 같은 rhwp 함수(`extract_thumbnail_only`)를 부른다(참조 쪽은 따로 안 쟀다).
- **판정: Quick Look은 작은 문서만 같거나 빠르고, 글자·그림·표가 많은 문서에서 느리다(PNG 1.2~6.4배, PDF 2.9~7.4배, PDF 크기 4~100배).** 원인은 아래 병목 6, 7이다.

#### 2.4.1 Quick Look 적용 결과 (병목 6·7·4, 후보 1·4·5, PD-165)

세 가지를 적용하고 같은 하네스(`scripts/bench-run.mjs --runs 5 --suite quicklook`, 별도 `CARGO_TARGET_DIR`, 5회 중앙값)로 다시 쟀다. 기기 부하가 높아 시간(wall)이 흔들려서 CPU 시간으로 본다. 번들 글꼴 없이 시스템 글꼴만 쓰는 헤드리스 호출이다.

1. `fonts.rs`가 글꼴 면을 프로세스당 한 번 매핑(`make_shared_face_data`, 셸의 `with_mapped_faces`와 같은 방식).
2. 미리보기 PDF가 글자를 글꼴 서브셋으로 포함(`embed_text: true`). 서브셋이 실패한 쪽만 윤곽선으로 다시 변환한다(셸 PDF의 PD-114 5와 같은 재시도).
3. 쪽 콘텐츠 스트림 deflate 수준 6 → 1(`vendor/svg2pdf`의 `ConversionOptions::compression_level`, 기본 6이라 셸 출력은 그대로).
4. (PD-163, 이 작업이 아님) `fill_fonts`가 글꼴 파일을 복사하지 않고 면 매핑을 공유(후보 3). 아래 RSS 표의 1~4번 열은 이 패치를 더한 값이다.

최대 RSS MB (PDF, 적용 전 / 1~3번 / 1~4번): biz_plan 31 / 215 / 22, form-002 64 / 293 / 49, kps-ai 172 / 398 / 91, issue1949 130 / 134 / 67, KTX 83 / 241 / 84, number-bullet 23 / 205 / 21. 첫 쪽 PNG는 모두 적용 전과 같은 수준(number-bullet 17 → 21, kps-ai 28 → 27 MB). 1~4번의 PDF CPU ms: biz_plan 100, form-002 430, kps-ai 1410, KTX 150, number-bullet 60(표의 1+2+3 열과 같은 범위).

미리보기 PDF, CPU ms (괄호는 출력 크기 KB)

| 입력 | 적용 전 | 1만 | 1+2 | 1+2+3 |
|---|---:|---:|---:|---:|
| biz_plan(6쪽) | 550 (2817) | 370 | 150 (380) | 120 (399) |
| form-002(10쪽) | 2830 (10584) | 1410 | 580 (1251) | 460 (1294) |
| kps-ai(64쪽) | 7590 (32682) | 3910 | 1880 (3937) | 1480 (4050) |
| issue1949(64쪽) | 7190 (32484) | 5280 | 3180 (3810) | 2440 (3902) |
| KTX(1쪽, 그림 위주) | 450 (13250) | 150 | 170 (13114) | 140 (13126) |
| number-bullet | 230 (1277) | 150 | 70 (152) | 50 (154) |

- 77쪽 문서(kps-ai) 시간(wall)은 9600 → 1493 ms, 크기는 32.7 → 4.0 MB(참조 앱 7.4 MB보다 작다). 윤곽선 대신 글꼴 서브셋이 크기와 시간을 가장 크게 줄였고(2번), 면 매핑이 그 앞의 큰 몫(1번), deflate 수준 1은 추가로 CPU 약 20%를 줄였다(PDF 크기는 약 3% 늘어남, 3번).
- 첫 쪽 PNG(CPU ms, 적용 전 → 후): KTX 380 → 130, form-002 220 → 90, number-bullet 140 → 50, kps-ai 110 → 140(잡음 범위), biz_plan 20 → 40. 글꼴 DB를 만드는 `fonts` 단계가 약 10 ms에서 약 20 ms로 늘어 작은 문서(shift-return, blank2010, biz_plan)는 오히려 10~15 ms 더 걸린다. 면 파일을 미리 모두 매핑하는 비용이다. 쓰인 면만 지연 매핑하면 줄일 수 있으나 `fontdb`가 면 단위 지연 매핑을 주지 않아 구현하지 않았다.
- PNG 바이트 수는 전 입력에서 적용 전과 같다(9494, 5685, 19092, 163263, 24652, 122923, 135972, 102901 B). 글꼴과 그리기 경로가 같아 픽셀이 같다는 정황이며, 바이트 비교는 아니다.
- 최대 RSS: 3번까지만 적용했을 때 PDF가 biz_plan 31 → 215 MB, number-bullet 23 → 205 MB, kps-ai 172 → 398 MB로 늘었다. 원인은 면 매핑이 아니라 `vendor/svg2pdf`의 `fill_fonts`가 쓰인 면의 글꼴 파일 전체(`Vec::from(data)`, 시스템 한국어 글꼴 `AppleSDGothicNeo.ttc`는 55 MB)를 복사하던 것이다(면 매핑만 적용한 측정은 number-bullet 27 MB로 정상). PD-163의 `fill_fonts` 패치(DB가 매핑해 둔 면이면 복사 없이 공유)가 이를 없앤다.

### 2.5 한 줄 요약

| 경로 | 참조 앱 대비 |
|---|---|
| PDF 내보내기 | 빠름(2쪽 이상 7배 이상), 작은 문서 같음. RSS는 4.1절 최적화 전 기준 큼(이후 참조 앱보다 작음) |
| 저장 검증 | 빠름(단, 검사 범위가 좁음) |
| 홈 미리보기 | 임베디드 그림이 있으면 훨씬 빠름. 그리는 경우 일이 달라 비교 불가, rhwp 파싱이 큰 문서는 같음 |
| Quick Look PNG·PDF | 작은 문서 같음·빠름, 큰 문서 느림 |
| 문서·시트·슬라이드 엔진 | 참조 앱에 같은 경로가 없어 비교 못 함 |

## 3. 우리 경로별 측정 (참조 앱에 없는 경로)

2차 측정, 중앙값, 합성 입력. 열기는 `convert_document_format`의 `action: decode`, 저장은 열기 결과를 다시 보내는 구조화 저장이다.

| 입력 | 열기 | 저장(열기 뒤) | 열기 RSS | 비고 |
|---|---:|---:|---:|---|
| 문서 9,700문단 + 표 100개: DOCX | 23 ms | 62 ms | 33 MB | 입력 231 KB |
| 같은 문서: HWPX | 32 ms | 79 ms | 49 MB | 356 KB |
| 같은 문서: HWP | 15 ms | 36 ms | 28 MB | 1.4 MB |
| 같은 문서: ODT | 16 ms | 48 ms | 27 MB | 213 KB |
| 같은 문서: RTF | 16 ms | 53 ms | 30 MB | 2.3 MB |
| 같은 문서: PDF(텍스트) | 34 ms | 83 ms | 33 MB | 2.9 MB |
| 시트 1,000x104(수식 25%): XLSX | 47 ms | 229 ms | 34 MB | 723 KB, 저장 RSS 106 MB |
| 같은 시트: ODS | 83 ms | 331 ms | 66 MB | 저장 RSS 143 MB |
| 같은 시트: XLS | 33 ms | 111 ms | 42 MB | 저장 RSS 105 MB |
| 슬라이드 300장: PPTX | 9.5 ms | 41 ms | 11 MB | |
| 같은 슬라이드: ODP / PPT | 2.3 / 1.8 ms | 7.4 / 8.4 ms | 6~8 MB | |

- 구조화 인코딩 한 번(`encode_structured`): DOCX 11 ms, HWPX 15 ms, XLSX 62 ms, PPTX 13 ms. XLSX 저장 229 ms 중 열기가 47 ms이므로 저장 단계 약 180 ms가 인코딩 한 번(62 ms)의 약 3배다(병목 9).
- 시트 재계산(`recalculate_all`, 수식 5열 구조): 1,000행 0.6 ms, 10,000행 7.5 ms, 50,000행 61 ms(시트 만들기 포함 총 3 / 34 / 223 ms, 50,000행 RSS 193 MB).
- `easyoffice-layout`의 `compute_layout`: 호출 100,000번에 72 ms(호출당 약 0.7 µs). 좌표 계산뿐이라 병목이 아니다.
- 엔진 경로는 사용자가 느낄 만한 값(모두 0.35 초 이하)이 아니다. 실제 사용자 파일과 더 큰 입력은 못 쟀다.
- Pages 파일 열기는 이 하네스에 입력이 없어 못 잼.

## 4. 상위 병목 10개

비율은 해당 경로를 `sample`로 본 바쁜 스레드 샘플 중의 몫이다(자기 시간 = 그 함수 안, 포함 = 하위 호출 포함). 줄 번호는 정의 줄이다(핫 줄은 디버그 정보가 없어 못 구했다). 서드파티 함수는 `~/.cargo/registry` 사본 기준이다.

| # | 경로 | 함수:줄 | 비율 | 설명 |
|---:|---|---|---|---|
| 1 | PDF 내보내기 | `fontdb::Database::query`(fontdb 0.23.0 `src/lib.rs:661`), `usvg::text::FontResolver::default_font_selector`(usvg 0.45.1 `src/text/mod.rs:85`). 호출은 `pdf_synthetic.rs:494`(`parse_page`) | 77쪽: 포함 20.0%(자기 14.7%), 115쪽 표 문서: 포함 28.6%(자기 26.5%), 미리보기 KTX: 포함 14.8% | 글자 요소마다 모든 글꼴 면(수백 개)을 걸러 가며 찾는다. 같은 (글꼴 목록, 굵기, 모양) 질의가 쪽마다 반복된다 |
| 2 | PDF 내보내기 | 글자 한 개마다 `ttf_parser::Face::parse`(`with_face_data` 클로저 안: `outline`, `colr`, `raster`, `svg`, `shape_text_with_font`) | 합성 300쪽: 포함 30.0%(`parse_tables` 16.9%), `outline` 26.0%, `colr` 16.5% | usvg 글자→경로 변환이 글자마다 글꼴 표를 다시 파싱한다. 면 데이터는 이미 매핑돼 있으나 파싱 결과는 재사용하지 않는다 |
| 3 | PDF 내보내기 | `svg2pdf::render::text::fill_fonts`(`vendor/svg2pdf/src/render/text.rs:525`, 복사는 551줄 `Arc::new(Vec::from(data))`), `Context::new` | 합성: `_platform_memmove` 자기 21.2%, `fill_fonts` 포함 12.4%, `Context::new` 11.8%. 77쪽: 10.9% / 13.1% | 쪽마다 쓰인 글꼴 파일 전체를 복사하고 다시 파싱한다. 최대 RSS(425~778 MB)의 한 원인으로 보이나 RSS 분해는 안 했다 |
| 4 | PDF·Quick Look | `miniz_oxide::deflate::compress_to_vec_zlib`(수준 6, `vendor/svg2pdf/src/util/helper.rs:184`) | PDF 합성 12.6%, 77쪽 13.0%, 115쪽 7.3%. Quick Look PDF(77쪽) 자기 40.4%, 포함 42.4% | 쪽 콘텐츠 스트림 압축이 CPU를 많이 쓴다. Quick Look은 글자를 윤곽선으로 내보내 스트림이 크다 |
| 5 | PDF 내보내기 | `roxmltree` 파싱(`tokenizer::parse_element`, `parse_content`)과 `pdf_synthetic::asks_for_emphasis`(`pdf_synthetic.rs:256`) | 77쪽: roxmltree 포함 27.0%(usvg의 파싱 포함). `asks_for_emphasis` 자기 1.8~2.5% | 굵게·기울임이 있는 쪽은 쪽 SVG를 usvg 앞에서 한 번 더 파싱한다. 강조 여부는 단어 네 개를 따로 훑어 찾는다 |
| 6 | Quick Look 첫 쪽 PNG·PDF | `fonts.rs:29`의 `build()`가 면을 매핑하지 않음 → `with_face_data`가 매번 `open`·`mmap`·`munmap` | `ql_png` KTX: munmap 47.5% + open 12.9% + mmap 2.3% = 약 63%. `ql_pdf` 77쪽: 20.5% + 11.9% + 3.0% = 약 35% | 셸은 이미 `shared_font_db`에서 면을 한 번 매핑한다(PD-114). Quick Look 크레이트만 빠져 있다 |
| 7 | Quick Look 미리보기 PDF | `render.rs:126` `embed_text: false`(`pdf_of`는 119줄) | 77쪽 출력 32.7 MB(64쪽), 참조 앱은 7.4 MB, 시간 6.9 초 대 2.4 초 | 글자를 전부 윤곽선 경로로 내보내서 PDF가 크고 느리다. 병목 4와 함께 본다 |
| 8 | HWP 열기·쪽 나누기(rhwp) | `rhwp::renderer::layout::LayoutEngine::advance_row_cut_inner`(`third_party/rhwp/src/renderer/layout/table_layout.rs:16821`), `DocumentCore::from_bytes_inner` | 115쪽 표 문서의 PDF 프로파일에서 자기 7.4%, 포함 9.4%. 열기만 560~580 ms(참조 앱과 같음) | 상위 rhwp 코드다. 참조 앱과 같은 비용이라 격차는 없지만, 큰 표 문서에서 열기·미리보기·Quick Look의 하한이다 |
| 9 | 구조화 저장 | `convert_ipc.rs:253~258`(원본 형식도 한 번 더 `encode_structured`, 크기 용), `conversion_response`(`convert_ipc.rs:297`, 310줄에서 결과를 다시 `decode`) | XLSX 저장 프로파일: `encode_structured` 포함 55.6%, `evaluate_workbook` 27.2%, `conversion_response` 12.9%. 저장 180 ms 대 인코딩 한 번 62 ms | 원본 형식 인코딩은 응답의 `sourceBytes` 숫자 하나를 위해 쓰이고, 응답은 결과를 한 번 더 풀어 문단 수를 센다 |
| 10 | ODS 열기 / 시트 | `rtf_xml_helpers::find_valid_open_tag`(`rtf_xml_helpers.rs:330`), `find_matching_close_tag`(348), `extract_ods_cells_from_row`(518), `sheet_cells_json`(`convert_open.rs:228`) | ODS 열기: `find_valid_open_tag` 자기 21.1% + `find_matching_close_tag` 10.0% + memcmp 7.7%, `extract_ods_cells_from_row` 포함 24.9%, `sheet_cells_json` 15.9% | 셀마다 태그를 다시 찾는다. ODS 83 ms 대 XLSX 47 ms. 같은 줄에서 일반 셀·덮인 셀 태그를 따로 훑는 구조가 남아 있다 |

### 4.1 셸 PDF 내보내기 최적화 결과 (PD-163)

병목 1(글꼴 질의), 3(글꼴 파일 복사), 4(압축 수준)와 병목 2(쪽마다 면 파싱)를 다뤘다. 전(`base`)과 후(`final`)는 같은 입력에 같은 하네스(`perf_bench.rs`의 `pdf_doc`, 합성은 `pdf_svgs`)로 5번 돌린 중앙값이다. 두 바이너리를 번갈아 돌렸고 측정 때 기기 부하는 31~49였다(12코어). 그래서 CPU 시간(user+sys)과 최대 RSS를 근거로 하고, 벽시계는 참고만 한다. 생성 PDF는 전후가 바이트 단위로 같다(biz_plan, form-002, kps-ai, issue1949, KTX, 합성 300쪽을 sha1로 비교). 쪽 수, 글꼴 포함, 텍스트 추출, 래스터가 모두 같다.

| 입력 | 쪽 | CPU 전 → 후 | RSS 전 → 후 | 벽시계 전 → 후 |
|---|---:|---|---|---|
| shift-return.hwp | 1 | 20 → 20 ms | 44 → 19 MB | 27 → 24 ms |
| biz_plan.hwp | 6 | 190 → 140 ms (-26%) | 426 → 48 MB (-89%) | 81 → 67 ms |
| form-002.hwpx | 10 | 750 → 550 ms (-27%) | 738 → 92 MB (-88%) | 289 → 232 ms |
| kps-ai.hwp | 77 | 2250 → 1690 ms (-25%) | 829 → 182 MB (-78%) | 864 → 700 ms |
| issue1949 | 115 | 4210 → 3170 ms (-25%) | 439 → 201 MB (-54%) | 1893 → 1595 ms |
| KTX.hwp | 1 | 190 → 170 ms (-11%) | 297 → 137 MB (-54%) | 189 → 172 ms |
| 합성 300쪽 | 300 | 4670 → 4140 ms (-11%) | 423 → 71 MB (-83%) | 1399 → 1194 ms |

변경별로 나눈 값(같은 방식, 같은 날 앞선 실행): 글꼴 질의 메모만 넣었을 때 kps-ai CPU 2580 → 2130 ms, issue1949 4990 → 3740 ms(-25%), form-002 790 → 710 ms, 합성 4910 → 4840 ms. 글꼴 데이터 공유까지 넣으면 RSS가 위 표 수준으로 내려가고 CPU가 더 준다(합성 4840 → 4340 ms).

- **병목 1(질의 메모, `pdf_font_select.rs`)**: 표가 많아 글자 요소가 많은 문서(issue1949, kps-ai)에서 CPU 약 25% 감소. 요청 종류가 적은 합성 문서에서는 질의가 원래 작아 변화가 작다.
- **병목 3(글꼴 공유, `vendor/svg2pdf`)**: 최대 RSS가 54~89% 줄었다. 쪽마다, 스레드 4개마다 글꼴 파일(Noto Sans KR 등) 전체를 복사하던 것이 RSS의 대부분이었다. 2.1절의 참조 앱 RSS와 견주면 모든 입력에서 우리가 작아졌다(shift-return 19 vs 35, biz_plan 48 vs 207, form-002 92 vs 247, kps-ai 182 vs 385, issue1949 201 vs 214, KTX 137 vs 310, 합성 71 vs 97 MB).
- **병목 2(쪽마다 `Face::parse`)는 못 줄였다**: 이 호출은 usvg 안(`text::flatten`의 `with_face_data` 클로저)이라 공개 API로 면 파싱 결과를 넘길 수 없다. 합성 300쪽을 다시 `sample`로 보니 `usvg::text::flatten` 포함 66%, 그 안의 `ttf_parser::Face::parse` 포함 약 22%(`parse_tables` 약 11%)다. 줄이려면 usvg 사본을 패치해 면 파싱을 쪽 사이에 공유해야 한다. 상위 갱신 부담이 커서 이번에는 하지 않았다(결정 필요).
- **병목 4(압축 수준)는 바꾸지 않았다**. 수준별 CPU(ms)와 출력 크기(바이트), 5회 중앙값, 수준 6이 현재 값이다.

| 입력 | 수준 1 | 수준 2 | 수준 3 | 수준 4 | 수준 6 |
|---|---|---|---|---|---|
| form-002 | 510 / 1509832 | 530 / 1366266 | 530 / 1315816 | 500 / 1300198 | 530 / 1278597 |
| kps-ai | 1540 / 5908293 | 1610 / 5281340 | 1640 / 5099641 | 1710 / 5055291 | 1690 / 4970382 |
| 합성 300쪽 | 3560 / 14780073 | 3610 / 12948134 | 3770 / 12125739 | 3800 / 11971599 | 3630 / 11806944 |

  수준 1이 가장 빠르지만 줄어드는 CPU는 2~9%이고 PDF는 18~25% 커진다. 수준 2~4는 수준 6과 CPU가 같거나 오히려 느린 값도 나온다(측정 노이즈). 근거가 부족해 6을 둔다.

참고(상위 10에 못 든 것):

- 시트 `set_cell`의 수식 컴파일(`easyoffice-sheet/src/parser.rs:867` `compile_formula_expr`)이 50,000행 시험의 포함 23.3%, `SheetEngine` 해제(`drop_glue`, `Expr` 트리)가 23.3%다. 사용자에게는 작은 값(50,000행 223 ms)이다.
- 프로세스당 한 번 드는 글꼴 DB 만들기는 20~30 ms다(`pdf_fonts.rs:40` `build_font_db`, 면을 전부 매핑하는 비용 포함). 콜드 상태(첫 실행)에서 시스템 글꼴 목록을 처음 읽을 때 360 ms까지 본 적이 있다(디스크 캐시 영향, 한 번 관측).
- 저장·내보내기 단계의 `add_font_fallbacks`(`pdf_font_fallbacks.rs:307`)는 115쪽에 88 ms, 쪽당 약 0.8 ms로 작다.

### 4.2 문서→PDF: 엔진 작성기와 HWPX 쪽 배치 (PD-167)

변환기·다른 이름으로 저장의 문서→PDF가 엔진의 평문 작성기 대신 HWPX → rhwp 쪽 SVG → `PdfAssembler` 경로를 쓴다. 같은 입력을 하네스(`perf_bench.rs`의 `doc_pdf_engine`, `doc_pdf_shell`; `node scripts/bench-run.mjs --suite docpdf --docs <파일,…>`)로 5번 돌린 중앙값이다. 측정 때 기기 부하가 41~57이었으므로 CPU 시간과 RSS를 근거로 하고 벽시계는 참고만 한다. 큰 문서는 엔진 묶음이 만드는 문서(문단 9,700개, 표 100개)의 DOCX·ODT·RTF이고, 작은 것은 제목·목록·표·그림·각주·한글/영문이 든 직접 만든 DOCX·ODT·MD·TXT이다.

| 입력 | 쪽(엔진 → 셸) | 시간 엔진 → 셸 | 셸 CPU | RSS 엔진 → 셸 | 크기 엔진 → 셸 |
|---|---|---|---:|---|---|
| 큰 DOCX | 354 → 428 | 35 ms → 4.2 초 | 15.2 초 | 43 → 385 MB | 3.0 → 9.7 MB |
| 큰 ODT | 354 → 428 | 26 ms → 4.2 초 | 15.2 초 | 32 → 373 MB | 3.0 → 9.7 MB |
| 큰 RTF | 354 → 428 | 26 ms → 6.9 초 | 16.8 초 | 32 → 374 MB | 3.0 → 9.7 MB |
| 작은 DOCX | 3 → 2 | 0.7 → 67 ms | 80 ms | 12 → 42 MB | 76 → 116 KB |
| 작은 ODT | 3 → 2 | 0.6 → 67 ms | 90 ms | 12 → 43 MB | 76 → 115 KB |
| 작은 MD | 2 → 2 | 0.2 → 75 ms | 100 ms | 11 → 45 MB | 16 → 55 KB |
| 작은 TXT | 1 → 2 | 0.2 → 59 ms | 60 ms | 11 → 34 MB | 12 → 37 KB |

- 셸 경로의 시간 중 글꼴 DB 로드가 20~26 ms(프로세스당 한 번)이고, 큰 문서는 428쪽의 레이아웃·SVG 그리기·PDF 변환이 대부분이다. 쪽당 CPU는 약 35 ms로 4.1절의 HWP 문서(쪽당 22~27 ms)보다 조금 크다(합성 문서가 쪽마다 글자가 빽빽하다). PDF가 큰 것은 쪽마다 글꼴 부분집합이 들어가기 때문이고(PD-114와 같음), 글자가 글꼴 글리프로 들어가 검색·복사가 된다.
- **그리기와 변환 겹치기(`doc_pdf.rs`)**: 처음에는 한 스레드가 16쪽을 그린 뒤 변환했다. SVG 그리기(rhwp, 스레드 하나)와 변환(스레드 4개)이 서로를 기다렸으므로, 그리는 스레드를 따로 두고 길이 1의 채널로 변환 스레드에 넘기게 했다. 큰 DOCX 5번 번갈아 돌린 중앙값: 벽시계 6.7 → 5.1 초(-24%), CPU 18.0 → 17.4 초, 최대 RSS 349 → 374 MB(+25 MB, 미리 그려 둔 16쪽 한 묶음). 생성 PDF는 바이트 단위로 같다(큰 DOCX와 작은 DOCX를 `cmp`로 비교).
- 충실도는 같은 입력의 엔진 PDF와 셸 PDF를 `pdftoppm`으로 그려 눈으로 비교했다(이미지는 저장소에 두지 않았다).

### 4.3 XML 스트리밍 읽기 결과 (PD-168)

DOCX·ODT·HWPX 읽기는 이벤트마다 `String`을 만들고 이벤트 전체를 `Vec`에 모은 뒤 처리했다. XLSX는 셀마다 `extract_tags_with_attrs`·`extract_tag_bodies`·`strip_tag_blocks`로 같은 조각을 여러 번 훑었고, ODS는 셀마다 여는 태그와 닫는 태그를 다시 찾았다. 이를 빌린 `&str`/`Cow`를 내보내는 풀 리더(`xml_stream.rs`) 하나와, 그 위의 XLSX 상태 기계(`xlsx_sheet.rs`), ODS 격자 걷기(`ods_grid.rs`, 셀 텍스트와 스타일을 한 번에 읽음)로 바꿨다.

측정: `bench_case`의 `open`(IPC `convert_document_format` decode)을 입력마다 새 프로세스로 15번, 중앙값. 합성 입력(`bench_generate_inputs`: 문서 9,700문단+표 100개, 시트 1,000x104=104,000셀). 할당 횟수·바이트는 측정용으로만 끼운 카운팅 할당자의 값(저장소에는 `unsafe`를 두지 않아 커밋하지 않았다). 측정 때 기기 부하는 27~40이어서 시간은 참고이고, 같은 조건에서 전(`217c89b`)과 후를 번갈아 두 번씩 쟀다(두 번째 값도 같은 폭).

| 입력 | 열기 전 | 열기 후 | 할당 횟수 전→후 | 할당 바이트 전→후 | 최대 RSS 전→후 |
|---|---:|---:|---:|---:|---:|
| DOCX 231 KB(document.xml 약 1.5 MB) | 17.5 ms | 13.2 ms | 363,258 → 131,387 | 45.7 → 31.9 MB | 35 → 23 MB |
| ODT 213 KB | 14.1 ms | 13.5 ms | 235,029 → 146,340 | 38.4 → 32.9 MB | 28~30 → 23 MB |
| HWPX 356 KB | 29.7 ms | 19.6 ms | 763,503 → 219,364 | 69.8 → 41.7 MB | 51 → 25 MB |
| XLSX 10.4만 셀 723 KB | 46.8 ms | 36.5 ms | 910,235 → 754,203 | 93.3 → 64.3 MB | 36 → 36 MB |
| ODS 10.4만 셀 1.1 MB | 78.4 ms | 58.7 ms | 1,266,227 → 936,213 | 133.5 → 90.5 MB | 66 → 65 MB |

- 열린 결과(`decode` 응답 JSON)는 전·후가 바이트 단위로 같다(위 5개와 ODP·PPTX·XLS·참조 HWPX 샘플, 보고서 DOCX·ODT 등 11개 입력을 `cmp`로 비교).
- 파서만 따로 잰 값(`cargo test --release -p easyoffice-formats --lib measure_one_pass -- --ignored --nocapture`, 15번 중 최소): XLSX 시트 3.6 MB 15.7→약 11 ms, ODS 표 14 MB 약 31→약 25 ms. 열기 전체에서 XLSX 시간이 덜 줄어든 것은 시트 JSON 만들기(`sheet_cells_json`)와 압축 해제가 그대로이기 때문이다.
- XLSX·ODS의 남은 할당은 대부분 셀마다 만드는 결과 `String`과 `decode_response`의 JSON 조립이다.
- 못 줄인 것: XLSX 최대 RSS(시트 JSON이 지배), ODS 압축 해제와 JSON 구간.

### 4.4 PDF 읽기, 같은 포맷 저장, 그림 공유, 정적 자산 (PD-173)

점검 보고서의 성능 항목 4개를 재현해 본 결과다. 기기 부하가 47~67이었으므로(12코어) 시간은 최솟값(15번 중)을 쓰고, 전·후를 같은 조건에서 번갈아 두 번 쟀다.

| 항목 | 판정 | 전 | 후 | 측정 |
|---|---|---|---|---|
| 일반 저장(Cmd+S) | 사실 | 원본 형식 인코딩 + 대상 인코딩 + 응답 디코딩 | 같은 포맷이면 대상 인코딩 결과 길이를 `sourceBytes`로 재사용(인코딩 1회 감소) | `perf_bench` `save`(구조화 저장, 열기 결과를 되돌려 보냄): DOCX 34→27 ms, HWPX 47→35, XLSX 190→119~131, PPTX 32→20.5, ODT 32→23, PDF 61→48 |
| 이미지·글꼴 스트림 해제 | 사실 | 40 MiB로 풀리는 이미지 3개를 가진 PDF(122,969 B)는 16 MiB 상한에서 거부(1.5 ms) | 해제하지 않고 열림(0.21 ms) | `PdfSerializer::parse`, 릴리스, 9번 중 최소 |
| `ParenJumpTable` 반복 구축 | 거짓 | 해당 파일에는 `bytes[..pos]`로 만드는 곳이 없고, `span_crosses_object_boundary`가 스트림 구간마다 한 번 만든다(구간 길이에 비례) | 변경 없음 | 스트림 2,000/8,000/32,000개(길이 선언이 틀려 `endstream` 탐색으로 찾는 모양 포함): 1.4 / 4.8 / 31.8 ms. 선형 |
| 같은 그림 다중 참조(DOCX) | 사실 | 참조마다 `to_vec` + PNG CRC | 첫 참조만 검증·복사, 나머지는 `Arc` 공유 | 100 KB PNG 100번 참조 열기 0.72 → 0.19 ms(최소, 릴리스) |
| 정적 자산 `to_string()` | 사실, 영향 미미 | 요청마다 자산 복사(index.html 80,139 B, app.js 45,562 B) | `Cow::Borrowed`로 복사 없음 | 복사 1.0 / 0.6 µs. 요청 하나가 약 19 ms(응답 뒤 소켓 비우기 대기 20 ms가 지배)라 루프백 지연 전후 차이는 보이지 않음 |
| `window-bridge.js`의 Base64 해제 | 사실, 제안 방식은 역효과 | `codePointAt` 루프 | `sheet-utils.js`의 공용 `base64ToBytes`(`Uint8Array.fromBase64` 감지, 없으면 4개씩 푼 `charCodeAt` 루프) | Node 24.19(`fromBase64` 없음): 64 KiB 0.082 / 1 MiB 2.2 / 8 MiB 13.5 ms(`codePointAt` 루프), 1 MiB 1.18 / 8 MiB 11.2 ms(4개씩 푼 루프). `Uint8Array.from(atob(s), f)`는 64 KiB 3.5 / 1 MiB 64 / 8 MiB 704 ms로 30~60배 느려 쓰지 않음. 64 KiB 이하에서는 `codePointAt` 루프가 약간 더 빠름 |

- `Uint8Array.fromBase64`는 이 측정의 Node에 없어 값을 못 쟀다(WebKit·Chromium 쪽은 못 잼).
- 저장 시간 감소는 인코딩 한 번의 비용이라 인코딩이 느린 XLSX에서 가장 크다.

### 4.5 시트 규칙 평가 결과 (PD-169)

시트 하나의 조건부 서식 규칙을 엔진 세션이 평가한다(`crates/easyoffice-sheet/src/rules/`). 셀 모양은 팔레트와 u32 단어 열(base64)로 보내고, 편집 뒤에는 바뀐 셀만 다시 판정해 달라진 셀만 돌려준다.

측정: 숫자 10만 셀(A1:J10000, 0~999 난수)에 규칙 5개를 모두 같은 범위에 걸고 `rules`로 한 번 설정(전체 평가)한 시간과 응답 바이트, 이어서 무작위 셀 1개를 바꾼 `apply` 30번의 중앙값. 기기 부하가 10~15였고(12코어) 세 번 잰 값의 폭을 적었다. `cargo test -p easyoffice-app --release --lib -- --ignored --nocapture rules_bench`.

| 규칙 5개 | 전체 평가(`rules`) | 응답 | 셀 1개 편집 | 편집 응답 |
|---|---:|---:|---:|---:|
| 셀 값 > 900, 상위 10, 3색 색조(최소·50백분위·최대), 데이터 막대(최소~최대), 아이콘 3개(33·67%): 범위 통계를 읽는 규칙 | 21~22 ms | 2,133,677 B | 8.8~9.0 ms | 170 B |
| 셀 값 > 900, 셀 값 사이, 3색 색조(숫자 0·500·1000), 데이터 막대(0~1000), 아이콘 3개(333·667): 자기 셀 값만 읽는 규칙 | 20~22 ms | 2,133,382 B | 0.01 ms 미만 | 170 B |

- 증분 전(편집마다 모든 규칙을 다시 평가하는 방식)의 편집 비용은 전체 평가와 같은 약 21 ms다. 통계를 읽는 규칙은 범위 통계를 한 번(10만 셀 훑기와 정렬본) 다시 만들어 2.4배 빨라지고, 자기 셀 값만 읽는 규칙은 바뀐 셀만 판정해 3자릿수 이상 줄어든다.
- 응답 2.13 MB는 칠해진 10만 셀 전부(셀당 평균 21 B)다. 같은 내용을 셀마다 JSON 객체(`{"ref":"A1","bg":"#RRGGBB","fg":"#RRGGBB","fl":3,...}`)로 보내면 약 8.94 MB로 추정한다(4.2배). 색은 팔레트에 한 번만 있고 셀은 2~4개 단어(8~16 B)다.
- 범위 통계(최솟값·최댓값·합·정렬본·표준편차)는 범위 목록당 한 번 만들어 규칙들이 나눈다. 위 첫 줄에서 4개 규칙이 한 번 훑은 값을 같이 쓴다.
- 식 규칙과 수식 피연산자는 어느 셀이 바뀌어도 규칙 전체를 다시 계산한다(읽는 셀을 모른다). 이 경로는 위 표에 넣지 않았다.
- 못 줄인 것: 통계를 읽는 규칙의 편집은 범위 크기에 비례한다(바뀐 셀이 최솟값·최댓값을 바꾸지 않으면 그 셀만 판정해도 되지만 아직 구현하지 않았다).

## 5. 최적화 후보 (우선순위)

효과는 위 프로파일 비율과 단계 측정에서 낸 추정이다. 구현해서 재지 않은 값은 "추정"이라고 적었다.

| 순위 | 후보 | 예상 효과 | 위험 | 관점 |
|---:|---|---|---|---|
| 1 | **Quick Look 적용함(2.4.1).** Quick Look에서 글꼴 면을 한 번만 매핑(`fonts.rs:build`에 `make_shared_face_data`, 셸의 `with_mapped_faces`와 같은 방식) | **측정함**: 임시로 적용해 재본 값은 첫 쪽 PNG KTX 456→214 ms(CPU 450→130), form-002 279→103, number-bullet 181→90, PDF 77쪽 6927→3625, form-002 3215→1364, biz_plan 892→439. 글꼴 DB 만들기 +약 10 ms | 낮음. 같은 코드가 셸에서 쓰이고 있다. 추출 프로세스 RSS 증가와 모든 면을 미리 매핑하는 비용은 쓰인 면만 지연 매핑하면 줄일 수 있다 | 할당·시스템 호출 제거(문서당 수만 번의 `open/mmap/munmap` 제거) |
| 2 | **처리함(PD-163, 4.1절).** 글꼴 선택 결과 캐시: `usvg::Options.font_resolver.select_font`를 (글꼴 목록, 굵기, 늘임, 모양)→면 ID 메모로 감싼다 | PDF에서 `fontdb::query` 14.7~28.6% 포함 시간 제거(추정 PDF 1.15~1.4배, 표·글꼴 반복이 많을수록 큼) | 낮음~중간. 캐시 키가 글꼴 DB와 한 몸이어야 한다. 내보내기마다 새로 만들고 상한을 둬 악의적 SVG에 대비해야 한다 | 분기·선형 탐색을 해시 조회로 |
| 3 | **처리함(PD-163, 4.1절): CPU -11~27%, RSS -54~89%.** `svg2pdf` `fill_fonts`가 글꼴 파일을 복사하지 않고 매핑된 데이터를 공유(`Arc<dyn AsRef<[u8]>>`, 또는 내보내기 전체에서 글꼴 한 번 파싱) | 합성 300쪽에서 `memmove` 21%와 `fill_fonts` 12%를 줄임(추정 1.2~1.3배), 최대 RSS도 줄 가능성(분해 못 함) | 중간. `vendor/svg2pdf`를 바꾸므로 `scripts/rhwp-svg2pdf.mjs`의 상위와 일치 검사를 통과하게 정책을 정해야 한다(PD-101) | 복사 제거, 파싱 한 번 |
| 4 | **셸 PDF는 측정 후 바꾸지 않음(4.1절). Quick Look만 수준 1로 적용함(2.4.1).** 쪽 스트림 압축을 낮은 수준(1~3)으로, 또는 더 빠른 deflate 구현 | PDF 7~13%, Quick Look PDF 최대 약 40%(추정 Quick Look PDF 1.3~1.6배) | 낮음. PDF가 조금 커진다(수치 못 잼) | CPU 대 크기 교환 |
| 5 | **적용함(2.4.1).** Quick Look 미리보기 PDF에서 글자를 윤곽선 대신 글꼴 포함(`embed_text: true`)으로, 또는 쪽당 비트맵 | 77쪽 32.7 MB → 수 MB, 시간 크게 감소 가능(못 잼) | 중간. 번들 글꼴이 필요하고 글자 모양·`ToUnicode` 검증이 따른다 | 출력 크기, 할당 |
| 6 | **일부 처리함(PD-173, 4.4절): 같은 포맷 저장에서 원본 크기용 인코딩만 제거.** 구조화 저장에서 원본 형식 인코딩을 없애고(`sourceBytes`는 입력 길이로), 응답용 `decode`를 인코더가 세는 문단·표 수로 대체 | XLSX 저장 약 30~40% 감소 추정(180→약 110 ms), 다른 형식은 약 25~35% | 중간. `sourceBytes`가 계약상 무엇이어야 하는지, 응답의 `detectedTarget` 검증 용도를 확인해야 한다 | 중복 계산 제거 |
| 7 | 강조 여부 판정을 한 번 훑기(`asks_for_emphasis`)로, 강조가 있는 쪽의 이중 XML 파싱을 한 번으로 | 합성 PDF에서 2~5% 추정 | 낮음 | 분기·패스 수 줄이기(SWAR 후보) |
| 8 | **처리함(PD-168, 4.3절): ODS 열기 78→59 ms.** ODS 셀 추출의 태그 탐색을 한 번에 훑기 | ODS 열기 83→약 55 ms 추정 | 낮음 | 중복 스캔 제거 |
| 9 | 시트 수식을 `Expr` 트리 대신 아레나 할당으로 | 50,000행 시험에서 해제 약 23% 감소 가능 | 중간 | 할당·해제 횟수 |
| 10 | 상위 rhwp 표 나누기 최적화(`advance_row_cut`) | 큰 표 문서의 열기 560~580 ms 감소 가능(미측정) | 높음. 상위 코드이고 고정 커밋이라 올리는 일이 선행된다 | 알고리즘 |

보는 순서는 1번(효과가 이미 잰 값이고 위험이 낮다) → 2·4(셸 PDF와 Quick Look 공통) → 3 → 5 → 6~8이다. 3번은 최대 RSS 개선 가능성 때문에, 5번은 Quick Look PDF의 크기 때문에 따로 값을 확인한 뒤 정한다.

## 6. 못 쟀거나 더 해야 할 것

- 실제 사용자 DOCX·XLSX·PPTX·PDF와 외부가 만든 PDF 열기, Pages 파일 열기.
- 번들 글꼴이 있을 때의 Quick Look, Windows·Linux, 웹뷰 구간, 한글 편집기 안의 쪽 SVG 생성.
- 최대 RSS가 컸던 원인은 쪽·스레드마다 한 글꼴 파일 전체 복사가 대부분이었다(4.1절에서 제거). 남은 RSS(전 면 매핑, 병렬 스레드, 쪽 객체)의 분해는 하지 않았다.
- 줄 단위 핫스팟(디버그 정보 없이 `sample`만 사용). 줄이 필요하면 `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`로 따로 빌드해야 한다(전체 재빌드).
- 벽시계 값은 측정 중 기기 부하의 영향을 받았다. 낮은 부하에서 다시 재면 우리 쪽 벽시계 수치는 더 낮아질 가능성이 크다(1차와 2차 차이로 보인다).
