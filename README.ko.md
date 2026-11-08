# EasyOffice

[English](README.md) | [한국어](README.ko.md)

EasyOffice (`v0.1.0`)는 Rust 엔진과 오프라인 WebUI를 하나의 Tauri v2 데스크톱 셸에 담은 **100% 독립 실행형(Standalone)·서버리스(Serverless)·오프라인 지원 macOS·Windows 올인원 오피스 스위트**입니다. 참조 앱의 rhwp-studio 크롬을 기준으로 한 라이트·다크 테마 위에서 **워드 문서(Document)**, **스프레드시트(Sheet)**, **프레젠테이션(Slide)**, **한글 문서(HWP / HWPX)**, **PDF 스튜디오(PDF Studio)**를 하나의 워크스페이스에서 통합 제공하며, 비트 수준 컨테이너 코덱과 결정론적 수식 연산, 온디바이스 스마트 문서 보조 도구, Fail-Closed 보안 가드를 내장합니다.

---

## 핵심 특징

- **macOS·Windows 단일 코드베이스**: Rust 엔진(`crates/`), WebUI(`crates/easyoffice-app/ui/`), Tauri v2 데스크톱 셸(`crates/easyoffice-desktop`, macOS는 WKWebView·Windows는 WebView2). 문서마다 개별 창으로 열리고 시작 창에서 새 문서·최근 문서·열기를 고릅니다. 단일 인스턴스, 외부 변경 감지가 있는 제자리 원자적 저장, 파일 연결은 Rust가 처리하며 WebView는 파일 경로를 받지 않습니다.
- **한글(HWP/HWPX) 편집기**: rhwp 엔진(MIT, 고정된 upstream 서브모듈)을 같은 출처 iframe 편집기로 띄워 HWP/HWPX 열기, 저장 전 검증, 수정 상태, IndexedDB 자동 저장을 지원하고, 셸이 파일 메뉴·테마·상태 표시줄을 맡습니다.
- **읽기·쓰기 20종 + Pages(읽기 전용)**:
  - ZIP 컨테이너(`ZipArchiveCodec`, `ZipSecurityGuard`): `OoxmlCodec`(`.docx`, `.xlsx`, `.pptx`, `.ppsx`), `OdfCodec`(`.odt`, `.ods`, `.odp`), `HwpxCodec`(`.hwpx`, `.hwtx`).
  - OLE2 복합 파일(`Ole2ContainerCodec`, `Ole2SecurityGuard`): `Hwp5Codec`(`.hwp`, `.hwt`), 실제 MS-DOC·MS-PPT 구조와 BIFF8 `.xls`를 쓰는 `LegacyOle2Codec`.
  - `PdfSerializer`(ISO 32000), `CsvCodec`(RFC 4180 `.csv`/`.tsv`), `CommonMarkCodec`(`.md`), `RtfCodec`(`.rtf`), UTF-8 `.txt`.
  - Apple Pages(`.pages`, IWA + Snappy 변형 + protobuf, 자체 디코더)는 보기·변환 원본으로 엽니다.
  - `RichDocument`가 DOCX·HWPX·ODT·RTF·Markdown·TXT·PDF에서 블록 순서와 글자 단위 서식을 유지합니다.
- **비트 수준 엔진 최적화**: 하드웨어 가속·slicing-by-8 `IeeeCrc32`, DEFLATE, SWAR 기반 UTF-16·XML·CSV 스캔, 64비트 `PackedCellCoordinate`(상위 32비트 행·하위 32비트 열), `SectorBitset`, 모든 경로의 `checked_*` 산술과 `unsafe` 미사용. `--self-test`가 벤치마크와 결과 동일성을 검사합니다.
- **시트 엔진**(`SheetEngine`, `WorkbookEngine`): 수학·통계·논리·찾기·텍스트·재무(`PMT`, `NPV`, `IRR` 등)·날짜/시간·정보를 아우르는 **122종 내장 함수**, 슬롯 메모를 쓰는 의존성 DAG, 피벗(`compute_pivot`), 차트, 조건부 서식·유효성 검사. 셀 서식과 열 너비는 XLSX/ODS에 저장·복원됩니다.
- **오프라인 스마트 도구와 보안**: 출력 바이트를 `FormatDetector`로 사후 검증하는 `UniversalFormatConverter` 일괄 변환, `LocalSmartAssistant`(핵심 문장 요약, 문장 진단, 문서 비교), fail-closed `FormatDetector`·`XmlSanitizer`·`DocumentSecurityAuditor`(매직 바이트 위조, ZIP 폭탄, 경로 순회, XXE, 개인정보 비식별화).
- **참조 앱의 rhwp-studio 크롬 기준 디자인**(PD-098): 단일 SVG 아이콘 세트, 라이트·다크 테마, 테스트로 검사하는 WCAG 2.1 AA 명암비.

---

## 지원 포맷 매트릭스 (20종)

| 모듈 | 지원 확장자 | 기술 표준 / 컨테이너 규격 | 핵심 코덱 및 엔진 기능 |
| :--- | :--- | :--- | :--- |
| **Document** | `DOCX`, `DOC`, `ODT`, `RTF`, `MD`, `TXT` | ECMA-376 WordprocessingML (ZIP+XML), OLE2 Word, ISO/IEC 26300 ODF Text, RTF 1.9, CommonMark, UTF-8 | `OOXMLContainerCodec`, `ODFContainerCodec`, `OLE2ContainerCodec`, `RTFCodec`, `CommonMarkCodec`, 다단/구역 조판, 표 계산식, 변경 내용 추적, 메일 머지 |
| **HWP (한글)** | `HWP`, `HWPX`, `HWT`, `HWTX` | HWP 5.0 Binary (OLE2 CFBF `FileHeader`/`DocInfo`/`BodyText`), KS X 6101 OWPML (ZIP+XML) | `HWP5BinaryCodec`, `HWP5RecordHeader`, `HWPXContainerCodec`, 공문서 두문/결재란 표 양식, 머리말/꼬리말, 각주, 서식 템플릿, HWP ↔ HWPX/DOCX/PDF 변환 |
| **Apple Pages (읽기 전용 원본)** | `PAGES` | Apple iWork IWA (ZIP + `Index/*.iwa`: Snappy 변형 청크 + protobuf 레코드). 구형 `index.xml`·암호화 문서는 명확한 안내와 함께 거부 | `pages_iwa`, `pages_proto`, `rich_read_pages` (자체 디코더, Apple 스키마 파일 미복사): 본문, 굵게/기울임/밑줄/취소선, 정렬, 제목, 단순 목록·표. 건너뛴 요소는 경고로 안내. 열기·변환 원본으로만 사용(저장·변환 대상 아님), PD-087 |
| **Sheet** | `XLSX`, `XLS`, `ODS`, `CSV`, `TSV` | ECMA-376 SpreadsheetML, OLE2 BIFF8, ISO/IEC 26300 ODF Spreadsheet, RFC 4180 CSV/TSV | `PackedCellCoordinate`, 122종 내장 수식 함수, `PivotTableEngine`, `SheetChartBuilder`, `CSVStateMachineCodec`, 다중 정렬, 조건부 서식·유효성 검사 |
| **Slide** | `PPTX`, `PPT`, `PPSX`, `ODP` | ECMA-376 PresentationML, OLE2 Presentation, ISO/IEC 26300 ODF Presentation | `PresentationDeckModel`, 16:9·4:3·A4 캔버스, `SnapGuideCalculator` 스마트 정렬 가이드, 레이어 순서 제어, 전환 효과, `PresenterSession` 발표자 뷰 |
| **PDF Studio** | `PDF` | ISO 32000-1 / 32000-2 (`%PDF-1.x` / `%PDF-2.0`) | `PDFBinarySerializer`, `PDFStudioDocumentModel`, 5종 주석·서명, 북마크 트리, 페이지 병합/분할/재배치/회전, 오피스 ↔ PDF 양방향 변환 |

---

## 아키텍처 개요 (Architecture Overview)

```text
EasyOffice/
├── Cargo.toml                        # Rust 워크스페이스 매니페스트
├── crates/easyoffice-formats/        # 20종 포맷 레지스트리, 바이너리·컨테이너 코덱, 매직 바이트 가드
├── crates/easyoffice-sheet/          # 수식 엔진(122종 내장 함수), 피벗, DAG 재계산
├── crates/easyoffice-layout/         # 페이지 조판 엔진
├── crates/easyoffice-app/            # IPC 라우터, CLI(`--serve`, `--self-test`), `ui/`의 WebUI
├── crates/easyoffice-desktop/        # Windows(WebView2)·macOS(WKWebView)용 Tauri v2 데스크톱 셸
├── apps/hwp-studio/                  # 고정된 rhwp 업스트림 위에 만든 한글 편집기 오버레이
├── scripts/                          # 위생 가드, 번들 계약 검사, WebUI 회귀 테스트
└── docs/                             # 아키텍처, 디자인 시스템, 정책 결정 기록(SSOT), 제품 명세
```

---

## 빠른 시작 (Quick Start)

### 시스템 요구 사항
- macOS 11.0 (Big Sur) 이상 (Apple Silicon `arm64` 및 Intel `x86_64` 유니버설 `.dmg`; 데스크톱 셸 빌드에는 Rust 1.90+, Node.js, Xcode 명령줄 도구 필요)
- Windows 설치형 데스크톱 앱: Windows 10/11 x64 및 WebView2 런타임 (데스크톱 셸 빌드는 MSVC 툴체인의 Rust 1.90+ 필요)
- 크로스플랫폼 엔진 크레이트와 개발 전용 로컬 WebUI 서버 빌드는 Rust 1.85+

### 로컬 위생 검증 · 빌드 · 테스트 · 실행

```bash
# 1. 저장소 위생 및 필수 계약 검증
node scripts/check-repo-hygiene.mjs

# 2. Rust 엔진: 서식, 린트, 테스트, 자체 시험
cargo fmt --all -- --check
cargo clippy --workspace --exclude easyoffice-desktop --all-targets -- -D warnings
cargo test --workspace --exclude easyoffice-desktop
cargo run -p easyoffice-app -- --self-test

# 3. WebUI 회귀 테스트
node --test scripts/test-webui-*.mjs scripts/test-desktop-file-associations.mjs

# 4. 데스크톱 셸 (생성된 편집기 번들 필요, 위의 한글 모듈 설명 참고)
node scripts/checkout-rhwp.mjs && node scripts/build-hwp-studio.mjs
cargo run -p easyoffice-desktop
```

### 릴리스 실행 안내

#### macOS

macOS 릴리스는 유니버설 디스크 이미지 `EasyOffice-<version>-macos-universal.dmg`(Apple silicon·Intel, macOS 11 이상)와 체크섬 파일 `SHA256SUMS-macos-universal.txt`로 배포됩니다. 이미지를 열어 **EasyOffice**를 `/Applications`로 끌어다 놓습니다. 앱은 Windows와 같은 Tauri v2 데스크톱 셸(WKWebView)로, WebUI가 내장되어 있고 모든 명령이 Rust 엔진에서 프로세스 안에서 실행되며 네트워크 연결이 필요 없습니다. 표준 메뉴 막대와 Command 단축키(⌘C/⌘V/⌘X/⌘A/⌘Z/⌘S/⌘O/⌘W/⌘Q)가 동작하고, Finder의 "다음으로 열기"와 더블 클릭은 파일을 새 창에서 엽니다. ⌘Q로 종료하면 각 창의 저장 / 저장 안 함 / 취소 확인을 거칩니다.

이 배포본은 Apple Developer ID로 **서명·공증되어 있지 않으므로** 처음 실행할 때 macOS Gatekeeper가 막습니다("개발자를 확인할 수 없기 때문에 열 수 없습니다"). 한 번만 허용하면 됩니다.

1. Finder에서 `/Applications`의 **EasyOffice**를 우클릭(또는 Control-클릭)하여 **열기**를 선택하고, 대화상자에서 다시 **열기**를 누릅니다.
2. 또는 한 번 열기를 시도한 뒤 **시스템 설정 > 개인정보 보호 및 보안**의 보안 항목에서 EasyOffice의 **그래도 열기**를 누릅니다(macOS 15는 우클릭 > 열기 대신 이 경로가 필요합니다).
3. 앱이 손상되었다고 표시되거나 실행되지 않으면(내려받을 때 붙은 격리 속성 때문) 격리 속성을 한 번 제거합니다.

```bash
xattr -dr com.apple.quarantine /Applications/EasyOffice.app
```

이후에는 평소처럼 열립니다. 서명은 준비만 되어 있고 꺼져 있습니다. macOS CI·릴리스 작업에 `APPLE_SIGNING_IDENTITY` 저장소 시크릿(과 공증 변수)을 추가하면 됩니다(PD-094). macOS에서 `.dmg`를 소스로 빌드하려면 다음과 같이 합니다.

```bash
cd crates/easyoffice-desktop
npx --yes --ignore-scripts @tauri-apps/cli@2.12.4 build --bundles app,dmg   # target/release/bundle/dmg/EasyOffice_<version>_<arch>.dmg
```

#### Windows

Windows 릴리스는 설치 프로그램 `EasyOffice-<version>-windows-x64-setup.exe`(NSIS, 사용자 단위 설치, 관리자 권한 불필요)로 배포됩니다. 설치한 뒤 시작 메뉴에서 **EasyOffice**를 실행합니다. `EasyOffice.exe`는 Windows 내장 WebView2 런타임 위의 데스크톱 창(Tauri v2 셸, `crates/easyoffice-desktop`)으로, WebUI가 실행 파일에 포함되어 있고 모든 명령을 프로세스 안에서 Rust 엔진으로 바로 처리합니다. 로컬 HTTP 서버, 브라우저, 네트워크 연결을 쓰지 않습니다. 저장하면 Windows 기본 저장 대화상자가 열립니다. 제거는 설정 > 앱 또는 시작 메뉴 폴더의 제거 프로그램으로 합니다. WebView2는 Windows 11과 최신 Windows 10에 기본 포함되어 있으며, 없을 때만 설치 프로그램이 런타임 부트스트래퍼를 내려받습니다. 문서 파일 연결(`.docx`/`.hwpx` 등 더블클릭으로 열기)은 앱에서 파일 열기 기능(#111)이 준비될 때까지 일부러 등록하지 않습니다.

소스에서 설치 프로그램을 빌드하려면 Windows에서 MSVC Rust 툴체인 1.90+, Visual Studio Build Tools, Node.js가 필요합니다.

```powershell
cd crates/easyoffice-desktop
npx --yes --ignore-scripts @tauri-apps/cli@2.12.4 build --bundles nsis   # target/release/bundle/nsis/EasyOffice_<version>_x64-setup.exe
```

**개발자 전용 로컬 서버(`--serve`).** `--serve`는 일반 사용자에게 배포하지 않으며 어떤 릴리스 자산에도 포함되지 않습니다. 개발과 자동 검증(헤드리스 브라우저 UI 검증, CI 스모크)에만 쓰며, 설치형 앱은 네트워크 포트를 열지 않습니다. 소스 저장소에서 `cargo run -p easyoffice-app -- --self-test`로 엔진 무결성을 검증하거나 `cargo run -p easyoffice-app -- --serve 8787`로 `http://127.0.0.1:8787/` 루프백 WebUI를 열 수 있습니다. 전체 옵션은 `--help`로 확인합니다. 포트 없이 `--serve`를 실행하면 8787을 사용하고, 사용 중이면 8788~8800으로 대체합니다. 포트를 명시하면(`--serve 18787`) 해당 포트만 사용하며 쓸 수 없으면 실패합니다. 잘못된 포트, 알 수 없는 옵션, 파일 경로는 0이 아닌 종료 코드로 끝납니다. Host·Origin 검사, 연결 수·시간 제한, CSP는 개발 중 안전을 위해 그대로 유지합니다(PD-086).

**문서마다 개별 창.** 설치형 앱은 문서 하나를 창 하나로 엽니다. 인수 없이 실행하면 새 문서(DOCX·한글·XLSX·PPTX·PDF·변환기)·파일 열기·최근 문서가 있는 시작 창이 뜨고, **파일 → 새 문서**·**열기…(`Ctrl+O`)**·끌어 놓기는 새 창에서 문서를 엽니다(이미 열린 파일은 그 창을 앞으로 가져옵니다). 창 제목은 `파일 이름 — EasyOffice`이며 저장하지 않은 변경이 있으면 `●`가 붙고, 닫을 때 저장 / 저장 안 함 / 취소를 묻습니다. 마지막 창을 닫으면 앱이 종료됩니다. 파일 경로는 Rust 쪽에만 있고 WebView에는 파일 이름과 바이트만 전달됩니다(PD-088). 개발 전용 `--serve`는 같은 모델을 브라우저 창(`window.open`)으로 흉내 냅니다.

WebUI에서는 **파일 → 열기…(`Ctrl+O`)** 또는 작업 영역에 파일을 끌어 놓아 DOCX/DOC/ODT/RTF/MD/TXT, HWP/HWPX/HWT/HWTX, XLSX/XLS/ODS/CSV/TSV, PPTX/PPT/PPSX/ODP, PDF, Apple Pages(`.pages`, 읽기 전용) 파일(최대 10 MiB)을 해당 모듈에서 열 수 있습니다. 탭 제목은 파일 이름이 되고 열기는 하나의 실행 취소 단위이며, 편집기 모델이 담지 못하는 항목(그림·도형·머리말·셀 서식)은 열기 후 알려 줍니다. 글자 서식(크기·글꼴·글자 색·형광펜·굵게·기울임·밑줄·취소선)은 선택한 글자에만 적용되며 DOCX·HWPX·ODT·RTF 저장·열기에서 유지됩니다. 변환기 패널에서는 편집기 텍스트 대신 선택한 원본 파일의 실제 바이트를 변환할 수도 있습니다.

---

## 라이선스

[MIT License](LICENSE)에 따라 배포됩니다.
