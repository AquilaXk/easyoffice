# 개발 문서

사용자 안내(내려받기, 설치 유의사항)는 [README](../README.md)에 있습니다. 이 문서는 개발 환경, 빌드·검증 명령, 오픈소스 이식 규칙, 플랫폼별 동작을 모읍니다. 구조는 [architecture.md](architecture.md), 포맷별 범위와 상한은 [format-support.md](format-support.md), 측정값은 [performance.md](performance.md), 결정 이력은 [policy-decisions.md](policy-decisions.md), 단축키는 [keyboard-shortcuts.md](keyboard-shortcuts.md)에 있습니다.

---

## 핵심 특징

- **macOS·Windows·Linux 단일 코드베이스**: Rust 엔진(`crates/`), WebUI(`crates/easyoffice-app/ui/`), Tauri v2 데스크톱 셸(`crates/easyoffice-desktop`, macOS는 WKWebView·Windows는 WebView2·Linux는 WebKitGTK). 문서마다 개별 창으로 열리고 시작 창에서 새 문서·최근 문서·열기를 고릅니다. 단일 인스턴스, 외부 변경 감지가 있는 제자리 원자적 저장, 파일 연결은 Rust가 처리하며 WebView는 파일 경로를 받지 않습니다.
- **한글(HWP/HWPX) 편집기**: rhwp 엔진(MIT, 고정된 upstream 서브모듈)을 불투명 출처 샌드박스 iframe(`allow-scripts allow-modals`, `allow-same-origin` 없음) 편집기로 띄워 HWP/HWPX 열기, 저장 전 검증, 수정 상태, IndexedDB 자동 저장을 지원하고, 셸이 파일 메뉴·테마·상태 표시줄을 맡습니다.
- **읽기·쓰기 20종 + Pages(읽기, 쓰기는 실험적)**:
  - ZIP 컨테이너(`ZipArchiveCodec`, `ZipSecurityGuard`): `OoxmlCodec`(`.docx`, `.xlsx`, `.pptx`, `.ppsx`), `OdfCodec`(`.odt`, `.ods`, `.odp`), `HwpxCodec`(`.hwpx`, `.hwtx`).
  - OLE2 복합 파일(`Ole2ContainerCodec`, `Ole2SecurityGuard`): `Hwp5Codec`(`.hwp`, `.hwt`), 실제 MS-DOC·MS-PPT 구조와 BIFF8 `.xls`를 쓰는 `LegacyOle2Codec`.
  - `PdfSerializer`(ISO 32000), `CsvCodec`(RFC 4180 `.csv`/`.tsv`), `CommonMarkCodec`(`.md`), `RtfCodec`(`.rtf`), UTF-8 `.txt`.
  - Pages(`.pages`, IWA + Snappy 변형 + protobuf, 자체 디코더)는 보기·변환 원본으로 엽니다. Pages로 저장은 실험적이며 본문 글자·제목·정렬·기본 글자 서식만 씁니다(PD-162).
  - `RichDocument`가 DOCX·HWPX·ODT·RTF·Markdown·TXT·PDF에서 블록 순서와 글자 단위 서식을 유지합니다.
- **비트 수준 엔진 최적화**: 하드웨어 가속·slicing-by-8 `IeeeCrc32`, DEFLATE, SWAR 기반 UTF-16·XML·CSV 스캔, 64비트 `PackedCellCoordinate`(상위 32비트 행·하위 32비트 열), `SectorBitset`, `checked_*` 산술. `unsafe`는 CRC-32 intrinsics, C-ABI 브리지, Quick Look 래퍼, 데스크톱 셸의 OS 호출(프로세스 제한·글꼴 매핑)에만 둡니다. `--self-test`가 벤치마크와 결과 동일성을 검사합니다.
- **시트 엔진**(`SheetEngine`, `WorkbookEngine`): 수학·통계·논리·찾기·텍스트·재무(`PMT`, `NPV`, `IRR` 등)·날짜/시간·정보를 아우르는 **288종 내장 함수**, 슬롯 메모를 쓰는 의존성 DAG, 피벗(`compute_pivot`), 차트, 조건부 서식·유효성 검사. 셀 서식과 열 너비는 XLSX/ODS에 저장·복원됩니다.
- **오프라인 스마트 도구와 보안**: 출력 바이트를 `FormatDetector`로 사후 검증하는 `UniversalFormatConverter` 일괄 변환, `LocalSmartAssistant`(핵심 문장 요약, 문장 진단, 문서 비교), fail-closed `FormatDetector`·`XmlSanitizer`·`DocumentSecurityAuditor`(매직 바이트 위조, ZIP 폭탄, 경로 순회, XXE, 개인정보 비식별화).
- **PDF 스튜디오**: 원본 PDF의 그림·글꼴·링크·북마크·양식·주석·숨긴 레이어·쪽 레이블·첨부·XMP를 보존하는 쪽 편집과 저장(PD-170, PD-245, PD-266). PDF 읽기·쓰기는 셸이 띄우는 별도 워커 프로세스에서 하며, 옮기지 못한 항목은 경고로 알립니다.
- **BASE·BOARD**: 관계형 데이터베이스(`.eobase`)와 무한 캔버스 보드(`.eoboard`)는 ZIP 편집 패키지로 저장합니다. XLSX·CSV·TSV와 PDF는 내보내기입니다(PD-259, PD-260, PD-265).
- **코파일럿**: 사용자가 등록한 AI 모델 서버(클라우드·로컬)와 대화하는 우측 패널. 요청은 셸의 Rust가 SSRF 방어 전송으로 보내고, 중지하면 진행 중인 요청의 소켓을 끊습니다(PD-253, PD-267).
- **참조 앱의 rhwp-studio 크롬 기준 디자인**(PD-098): 단일 SVG 아이콘 세트, 라이트·다크 테마, 테스트로 검사하는 WCAG 2.1 AA 명암비.

---

## 지원 포맷 매트릭스 (20종)

읽기·쓰기·보존 범위와 거부 상한은 [format-support.md](format-support.md)에 있습니다. 아래 표는 모듈별 요약입니다.

| 모듈 | 지원 확장자 | 기술 표준 / 컨테이너 규격 | 핵심 코덱 및 엔진 기능 |
| :--- | :--- | :--- | :--- |
| **Document** | `DOCX`, `DOC`, `ODT`, `RTF`, `MD`, `TXT` | ECMA-376 WordprocessingML (ZIP+XML), OLE2 Word, ISO/IEC 26300 ODF Text, RTF 1.9, CommonMark, UTF-8 | `OOXMLContainerCodec`, `ODFContainerCodec`, `OLE2ContainerCodec`, `RTFCodec`, `CommonMarkCodec`, 다단/구역 조판, 표 계산식, 변경 내용 추적, 메일 머지 |
| **HWP (한글)** | `HWP`, `HWPX`, `HWT`, `HWTX` | HWP 5.0 Binary (OLE2 CFBF `FileHeader`/`DocInfo`/`BodyText`), KS X 6101 OWPML (ZIP+XML) | `HWP5BinaryCodec`, `HWP5RecordHeader`, `HWPXContainerCodec`, 공문서 두문/결재란 표 양식, 머리말/꼬리말, 각주, 서식 템플릿, HWP ↔ HWPX/DOCX/PDF 변환 |
| **Pages (읽기 전용 원본)** | `PAGES` | iWork IWA (ZIP + `Index/*.iwa`: Snappy 변형 청크 + protobuf 레코드). 구형 `index.xml`·암호화 문서는 명확한 안내와 함께 거부 | `pages_iwa`, `pages_proto`, `rich_read_pages` (자체 디코더, 제조사 스키마 파일 미복사): 본문, 굵게/기울임/밑줄/취소선, 정렬, 제목, 단순 목록·표. 건너뛴 요소는 경고로 안내. 열기·변환 원본으로만 사용(저장·변환 대상 아님), PD-087 |
| **Sheet** | `XLSX`, `XLS`, `ODS`, `CSV`, `TSV` | ECMA-376 SpreadsheetML, OLE2 BIFF8, ISO/IEC 26300 ODF Spreadsheet, RFC 4180 CSV/TSV | `PackedCellCoordinate`, 288종 내장 수식 함수, `PivotTableEngine`, `SheetChartBuilder`, `CSVStateMachineCodec`, 다중 정렬, 조건부 서식·유효성 검사 |
| **Slide** | `PPTX`, `PPT`, `PPSX`, `ODP` | ECMA-376 PresentationML, OLE2 Presentation, ISO/IEC 26300 ODF Presentation | `PresentationDeckModel`, 16:9·4:3·A4 캔버스, `SnapGuideCalculator` 스마트 정렬 가이드, 레이어 순서 제어, 전환 효과, `PresenterSession` 발표자 뷰 |
| **PDF Studio** | `PDF` | ISO 32000-1 / 32000-2 (`%PDF-1.x` / `%PDF-2.0`) | `PDFBinarySerializer`, `PDFStudioDocumentModel`, 5종 주석·서명, 북마크 트리, 페이지 병합/분할/재배치/회전, 오피스 ↔ PDF 양방향 변환 |
| **BASE** | `EOBASE` | ZIP 패키지(`mimetype`·`manifest.json`·`model.json`), `application/vnd.easyoffice.base+zip` | `native_package.rs`, 9종 필드, 수식, 데이터시트·보드·일정 보기, XLSX·CSV·TSV 내보내기 |
| **BOARD** | `EOBOARD` | ZIP 패키지(위와 같음, `media/`에 PNG·JPEG), `application/vnd.easyoffice.board+zip` | `native_package.rs`, 도형·글·그림·차트·연결선, PDF 내보내기 |

---

## 아키텍처 개요 (Architecture Overview)

```text
EasyOffice/
├── Cargo.toml                        # Rust 워크스페이스 매니페스트, [patch.crates-io] 세 곳
├── crates/easyoffice-formats/        # 포맷 레지스트리, 코덱, 보안 가드, BASE·BOARD 패키지 (외부 의존 0)
├── crates/easyoffice-sheet/          # 수식 엔진(288종 내장 함수), 피벗, DAG 재계산 (외부 의존 0)
├── crates/easyoffice-layout/         # 쪽·슬라이드 조판 모델 (외부 의존 0)
├── crates/easyoffice-app/            # IPC 라우터, 코파일럿 엔진, `--serve`, `--self-test`, `ui/`의 WebUI (외부 의존 0)
├── crates/easyoffice-desktop/        # Tauri v2 셸: 창, 저장, PDF 내보내기·편집 워커, 코파일럿 전송 (외부 의존 있음)
├── apps/hwp-studio/                  # 고정된 rhwp 위의 한글 편집기 오버레이
├── apps/quicklook/                   # macOS 빠른 보기 확장(별도 워크스페이스)
├── vendor/                           # 패치한 사본(각 폴더의 PATCHES.md 또는 RHWP_PATCHES.md), rhwp WASM
├── third_party/rhwp/                 # 고정 커밋의 sparse 체크아웃(추적하지 않음)
├── scripts/                          # 위생 가드, 번들 계약 검사, WebUI 회귀 테스트
└── docs/                             # 아키텍처, 포맷 지원, 성능, 정책 결정 기록(SSOT), 제품 명세
```

---

## 빠른 시작 (Quick Start)

### 시스템 요구 사항
- macOS 11.0 (Big Sur) 이상 (Apple Silicon `arm64` 및 Intel `x86_64` 유니버설 `.dmg`; 데스크톱 셸 빌드에는 Rust 1.93+, Node.js, Xcode 명령줄 도구 필요)
- Windows 설치형 데스크톱 앱: Windows 10/11 x64 및 WebView2 런타임 (데스크톱 셸 빌드는 MSVC 툴체인의 Rust 1.93+ 필요)
- Linux x64(AppImage·deb·rpm) 또는 arm64(deb): WebKitGTK 4.1과 glibc 2.35 이상, 예를 들어 Ubuntu 22.04, Debian 12, Fedora (데스크톱 셸 빌드에는 Rust 1.93+와 WebKitGTK 4.1 개발 패키지 필요)
- 크로스플랫폼 엔진 크레이트와 개발 전용 로컬 WebUI 서버 빌드는 Rust 1.85+(워크스페이스 MSRV). 데스크톱 셸 크레이트는 1.93이 필요하고 CI는 1.93.1로 고정합니다(`.github/rust-toolchain-version`)

### 고정 툴체인

CI와 릴리스는 `.github/rust-toolchain-version`의 정확한 버전(지금 1.93.1)을 씁니다. 버전은 그 한 파일에만 적고 공용 액션 `setup-rust`가 읽습니다(PD-147). 루트에 `rust-toolchain.toml`은 없어서 로컬은 설치된 툴체인을 쓰므로, CI와 같은 결과를 보려면 같은 버전을 설치해 씁니다.

```bash
rustup toolchain install "$(tr -d '[:space:]' < .github/rust-toolchain-version)" --profile minimal --component clippy,rustfmt
RUSTUP_TOOLCHAIN="$(tr -d '[:space:]' < .github/rust-toolchain-version)" cargo --version
```

버전을 올릴 때는 그 파일 한 줄을 바꾸고 같은 버전으로 `clippy`·`test` 전체를 돌린 뒤 한 커밋으로 올립니다. `node --test scripts/test-rust-toolchain-pin.mjs`가 원천이 하나인지 검사합니다.

### 검증 명령 (CI `checks`·`verify`와 같은 목록)

```bash
node scripts/check-repo-hygiene.mjs
node --test scripts/test-*.mjs
cargo fmt --all -- --check
cargo clippy --workspace --exclude easyoffice-desktop --all-targets -- -D warnings
cargo test --workspace --exclude easyoffice-desktop
cargo run -p easyoffice-app -- --self-test

# 데스크톱 셸을 건드렸을 때
cargo clippy -p easyoffice-desktop --all-targets -- -D warnings
cargo test -p easyoffice-desktop
```

- 위생 가드는 타 서비스·앱·오픈소스 이름, AI 흔적, 줄 수 상한, 수치 정합성(문서 숫자 동기화), XSS·CSP·셸 권한 계약을 검사합니다.
- 커밋 메시지는 `node scripts/check-commit-messages.mjs <base> <head>`(전체 SHA)로 검사합니다. 제목은 한국어 한 줄이고 `[Feat]`·`[Fix]`·`[Test]`·`[Chore]`·`[Docs]` 접두사를 붙입니다.
- `--self-test`는 포맷·수식·보안 불변식과 벤치마크의 결과 동일성을 확인합니다. 속도 하한은 release 빌드에서만 판정합니다(PD-082). 측정값은 [performance.md](performance.md)에 있습니다.

### 한글 편집기 번들 준비

데스크톱 셸 빌드와 한글 편집기 시험은 생성된 번들 `crates/easyoffice-app/ui/hwp-studio/`(추적하지 않음)가 필요합니다. 고정 커밋은 `config/rhwp-upstream.json`이 정합니다.

```bash
node scripts/checkout-rhwp.mjs      # third_party/rhwp를 고정 커밋으로 얕은 sparse 체크아웃(약 5.3 GB 전체는 받지 않음)
node scripts/build-hwp-studio.mjs   # apps/hwp-studio 오버레이를 빌드해 ui/hwp-studio에 생성
cargo run -p easyoffice-desktop     # 데스크톱 셸 실행
```

루트 `Cargo.toml`의 `[patch.crates-io]`가 `third_party/rhwp`를 가리키므로 체크아웃이 없으면 `cargo` 명령이 `third_party/rhwp/Cargo.toml`을 읽지 못하고 실패합니다(`--self-test`도 마찬가지). worktree를 새로 만들면 체크아웃을 다시 받습니다. 번들 `ui/hwp-studio/`가 없으면 데스크톱 셸의 `build.rs`가 빌드를 실패시킵니다. 개발 전용 로컬 WebUI는 `cargo run -p easyoffice-app -- --serve 8787`입니다.

### 오픈소스 이식 규칙

새로 짜기 전에 비슷한 기능을 구현한 오픈소스를 먼저 찾고, 다음 순서로 판단해 결과를 결정 기록(PD)에 적습니다.

1. 허용 라이선스의 유지되는 크레이트·라이브러리를 그대로 쓸 수 있는가. 엔진 크레이트(`formats`·`sheet`·`layout`·`app`)는 외부 의존이 0이라(PD-053, PD-095) 쓸 수 없습니다. 셸 크레이트는 PD-101의 절차(전이 의존 라이선스 확인)를 거쳐 추가합니다.
2. 이미 쓰는 의존성이나 기존 모듈에 같은 기능이 있는가.
3. 없을 때만 참조 구현의 함수를 안전한 Rust(엔진) 또는 바닐라 JS(WebUI)로 이식하고 비트·워드 단위로 최적화합니다. 측정 전후 수치를 남깁니다.

이식할 때 지키는 규칙은 다음과 같습니다.

- **허용 라이선스만.** MIT, Apache-2.0, BSD, Zlib, OFL 등. GPL·LGPL·AGPL·MPL 계열은 복사하지 않습니다.
- **원본 구조를 유지합니다.** 원본의 모듈 경계·함수 이름·알고리즘 순서를 따라 옮기고, 바꾼 점은 주석이나 결정 기록에 적습니다. 원본을 우리 방식으로 다시 설계하지 않습니다.
- **출처 주석.** 파일 머리 주석에 "참조 구현", 라이선스, 저작권자, 원본 경로·줄 범위를 적습니다.
- **`THIRD_PARTY_NOTICES.md`.** 가져온 코드와 데이터의 출처와 라이선스 전문을 올립니다. 이 파일이 타 프로젝트 이름을 쓸 수 있는 유일한 곳입니다. 다른 문서·주석·커밋에서는 "참조 앱"·"참조 구현"처럼 역할로 씁니다.
- **vendor 사본.** 패치한 의존 사본은 `vendor/` 아래에 두고, 폴더의 `PATCHES.md`(svg2pdf 사본은 `RHWP_PATCHES.md`)에 출처 버전·바꾼 파일과 줄 수·다시 적용할 곳을 적습니다. 루트 `Cargo.toml`의 `[patch.crates-io]`로 연결합니다.
- **상한 없는 경로를 따르지 않습니다.** 참조 구현이 입력 크기에 상한을 두지 않았다면 예산을 두고 넘으면 Fail-Closed로 거부하며, 악의적 입력 시험을 함께 넣습니다.
- 정책·기본값을 바꾸면 `docs/policy-decisions.md`를 갱신합니다(마지막 PD 번호 다음).

---

## 릴리스 패키지와 플랫폼별 빌드

**업데이트.** 설치된 앱은 하루 1회 이하로 새 릴리스를 확인합니다(도움말 메뉴에서 끌 수 있습니다). 새 버전이 있으면 카드에 **설치하고 다시 시작**이 나타나며, 앱이 릴리스에서 패키지를 내려받아 앱에 들어 있는 공개 키로 서명을 확인한 뒤 설치하고 다시 시작합니다. 저장하지 않은 변경 사항이 있는 창이 있으면 설치하지 않고, 서명이 맞지 않는 패키지는 설치하지 않습니다. `.deb`·`.rpm` 업데이트는 관리자 권한을 요청하며, 업데이터가 다루지 못하는 설치 형태는 릴리스 페이지에서 직접 받습니다(카드에 버튼이 있습니다). 릴리스에는 패키지마다 `.sig` 파일과 업데이터가 읽는 `latest.json`도 들어 있습니다.

#### macOS

macOS 릴리스는 유니버설 디스크 이미지 `EasyOffice-macos-universal.dmg`(Apple silicon·Intel, macOS 11 이상)로 배포됩니다. 이미지를 열어 **EasyOffice**를 `/Applications`로 끌어다 놓습니다. 앱은 Windows와 같은 Tauri v2 데스크톱 셸(WKWebView)로, WebUI가 내장되어 있고 모든 명령이 Rust 엔진에서 프로세스 안에서 실행되며 네트워크 연결이 필요 없습니다. 표준 메뉴 막대와 Command 단축키(⌘C/⌘V/⌘X/⌘A/⌘Z/⌘S/⌘O/⌘W/⌘Q)가 동작하고, Finder의 "다음으로 열기"와 더블 클릭은 파일을 새 창에서 엽니다. ⌘Q·Dock의 종료·AppleScript `quit`·시스템 종료로 끝내면 각 창의 저장 / 저장 안 함 / 취소 확인을 거칩니다(PD-189). Windows의 로그오프·시스템 종료와 Linux의 `SIGTERM`·`SIGHUP`도 저장 안 된 창이 있으면 같은 확인을 거칩니다(PD-191).

서명은 준비만 되어 있고 꺼져 있습니다. macOS CI·릴리스 작업에 `APPLE_SIGNING_IDENTITY` 저장소 시크릿(과 공증 변수)을 추가하면 됩니다(PD-094). macOS에서 `.dmg`를 소스로 빌드하려면 다음과 같이 합니다.

```bash
cd crates/easyoffice-desktop
npx --yes --ignore-scripts @tauri-apps/cli@2.12.4 build --bundles app,dmg   # target/release/bundle/dmg/EasyOffice_<version>_<arch>.dmg
```

#### Windows

Windows 릴리스는 설치 프로그램 `EasyOffice-windows-x64-setup.exe`(NSIS, 사용자 단위 설치, 관리자 권한 불필요)로 배포됩니다. 설치한 뒤 시작 메뉴에서 **EasyOffice**를 실행합니다. `EasyOffice.exe`는 Windows 내장 WebView2 런타임 위의 데스크톱 창(Tauri v2 셸, `crates/easyoffice-desktop`)으로, WebUI가 실행 파일에 포함되어 있고 모든 명령을 프로세스 안에서 Rust 엔진으로 바로 처리합니다. 로컬 HTTP 서버, 브라우저, 네트워크 연결을 쓰지 않습니다. 저장하면 Windows 기본 저장 대화상자가 열립니다. 제거는 설정 > 앱 또는 시작 메뉴 폴더의 제거 프로그램으로 합니다. WebView2는 Windows 11과 최신 Windows 10에 기본 포함되어 있으며, 없을 때만 설치 프로그램이 런타임 부트스트래퍼를 내려받습니다. 설치 프로그램은 주요 문서 형식(HWP, HWPX, DOCX, ODT, XLSX, ODS, PPTX, ODP, PDF)을 연결하므로 더블클릭하면 실행 중인 앱의 새 창에서 파일이 열립니다(단일 인스턴스). TXT, MD, CSV 같은 일반 텍스트 형식은 연결하지 않습니다. 저장은 임시 파일을 거쳐 이름을 바꾸는 방식으로 열었던 파일에 되쓰고, 그 사이 디스크에서 파일이 바뀌었으면 덮어쓰기 전에 묻습니다.

소스에서 설치 프로그램을 빌드하려면 Windows에서 MSVC Rust 툴체인 1.93+, Visual Studio Build Tools, Node.js가 필요합니다.

```powershell
cd crates/easyoffice-desktop
npx --yes --ignore-scripts @tauri-apps/cli@2.12.4 build --bundles nsis   # target/release/bundle/nsis/EasyOffice_<version>_x64-setup.exe
```

#### Linux

Linux 릴리스는 `EasyOffice-linux-x64.AppImage`, `EasyOffice-linux-x64.deb`, `EasyOffice-linux-x64.rpm`, `EasyOffice-linux-arm64.deb`로 배포됩니다. 배포판에 맞는 네이티브 패키지를 먼저 쓰세요. 한글 입력기(IME)와 WebKitGTK 런타임은 배포판이 제공하는 것을 쓸 때 가장 안정적이고, 패키지가 이를 의존성으로 선언합니다. Ubuntu·Debian은 `sudo apt install ./EasyOffice-linux-x64.deb`, Fedora는 `sudo dnf install ./EasyOffice-linux-x64.rpm`으로 설치합니다. arm64는 `.deb`만 만듭니다. AppImage는 설치가 필요 없지만(`chmod +x EasyOffice-linux-x64.AppImage` 뒤 실행, FUSE 2인 `libfuse2` 패키지 필요) 네이티브 패키지를 쓸 수 없을 때만 권장합니다. 일부 Wayland·IME 환경에서는 한영 전환이나 창 표시가 불안정할 수 있습니다. 패키지는 주요 문서 형식(HWP, HWPX, DOCX, ODT, XLSX, ODS, PPTX, ODP, PDF)을 다른 앱의 기본 프로그램을 가로채지 않고 "다음으로 열기" 대상으로 등록하며, Ubuntu 22.04에서 빌드하므로 glibc 2.35 이상과 WebKitGTK 4.1이 필요합니다. 내려받은 파일은 `sha256sum --check --ignore-missing SHA256SUMS.txt`로 확인합니다. Linux에서 소스로 패키지를 빌드하려면 Rust 1.93+, Node.js, `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libxdo-dev`, `librsvg2-dev`, `libssl-dev`, `patchelf`, `rpm`이 필요합니다.

```bash
cd crates/easyoffice-desktop
npx --yes --ignore-scripts @tauri-apps/cli@2.12.4 build --bundles appimage,deb,rpm   # target/release/bundle/{appimage,deb,rpm}/
```

---

## 실행과 문서 열기

**개발자 전용 로컬 서버(`--serve`).** `--serve`는 일반 사용자에게 배포하지 않으며 어떤 릴리스 자산에도 포함되지 않습니다. 개발과 자동 검증(헤드리스 브라우저 UI 검증, CI 스모크)에만 쓰며, 설치형 앱은 네트워크 포트를 열지 않습니다. 소스 저장소에서 `cargo run -p easyoffice-app -- --self-test`로 엔진 무결성을 검증하거나 `cargo run -p easyoffice-app -- --serve 8787`로 `http://127.0.0.1:8787/` 루프백 WebUI를 열 수 있습니다. 전체 옵션은 `--help`로 확인합니다. 포트 없이 `--serve`를 실행하면 8787을 사용하고, 사용 중이면 8788~8800으로 대체합니다. 포트를 명시하면(`--serve 18787`) 해당 포트만 사용하며 쓸 수 없으면 실패합니다. 잘못된 포트, 알 수 없는 옵션, 파일 경로는 0이 아닌 종료 코드로 끝납니다. Host·Origin 검사, 연결 수·시간 제한, CSP는 개발 중 안전을 위해 그대로 유지합니다(PD-086).

**문서마다 개별 창.** 설치형 앱은 문서 하나를 창 하나로 엽니다. 인수 없이 실행하면 새 문서(DOCX·한글·XLSX·PPTX·PDF·변환기)·파일 열기·최근 문서가 있는 시작 창이 뜨고, **파일 → 새 문서**·**열기…(`Ctrl+O`)**·끌어 놓기는 새 창에서 문서를 엽니다(이미 열린 파일은 그 창을 앞으로 가져옵니다). 창 제목은 `파일 이름 — EasyOffice`이며 저장하지 않은 변경이 있으면 `●`가 붙고, 닫을 때 저장 / 저장 안 함 / 취소를 묻습니다. 마지막 창을 닫으면 앱이 종료됩니다. 파일 경로는 Rust 쪽에만 있고 WebView에는 파일 이름과 바이트만 전달됩니다(PD-088). 개발 전용 `--serve`는 같은 모델을 브라우저 창(`window.open`)으로 흉내 냅니다.

WebUI에서는 **파일 → 열기…(`Ctrl+O`)** 또는 작업 영역에 파일을 끌어 놓아 DOCX/DOC/ODT/RTF/MD/TXT, HWP/HWPX/HWT/HWTX, XLSX/XLS/ODS/CSV/TSV, PPTX/PPT/PPSX/ODP, PDF, Pages(`.pages`, 읽기 전용) 파일(최대 10 MiB)을 해당 모듈에서 열 수 있습니다. 탭 제목은 파일 이름이 되고 열기는 하나의 실행 취소 단위이며, 편집기 모델이 담지 못하는 항목(그림·도형·머리말·셀 서식)은 열기 후 알려 줍니다. 글자 서식(크기·글꼴·글자 색·형광펜·굵게·기울임·밑줄·취소선)은 선택한 글자에만 적용되며 DOCX·HWPX·ODT·RTF 저장·열기에서 유지됩니다. 변환기 패널에서는 편집기 텍스트 대신 선택한 원본 파일의 실제 바이트를 변환할 수도 있습니다.
