# Policy Decisions

이 문서는 `EasyOffice`의 아키텍처, 문서 포맷 처리, UI/UX 디자인 시스템, 보안 한도, 릴리스 및 저장소 운영에 관한 핵심 정책 결정을 기록합니다. 모든 정책 변경이나 기본값 조정은 동일한 PR 내에서 이 문서의 항목을 함께 추가·갱신해야 하며, 외부 상용 서비스명을 직접 언급하지 않고 기술 표준과 도메인 근거로만 서술합니다.

## 기록 형식 (Entry Format)

각 정책 항목은 아래 7가지 필드를 필수로 포함합니다:

- **Status**: `Implemented` (적용 완료) / `Decided` (확정) / `Superseded` (대체됨)
- **Decision**: 시스템이 수행하는 구체적 동작 및 규칙
- **Why**: 실무적·기술적으로 이 선택이 옳은 이유
- **Trade-offs**: 수반되는 비용과 감수하는 제약
- **Alternatives rejected**: 검토했으나 기각한 대안과 기각 사유
- **Verified by**: 이 결정을 강제하고 증명하는 테스트 또는 검증 스크립트
- **Revisit when**: 결정을 재검토해야 하는 조건

---

## 목차 (Contents)

1. [macOS 네이티브 Swift 6 + SwiftUI/AppKit 하이브리드 아키텍처 채택](#1-macos-네이티브-swift-6--swiftuiappkit-하이브리드-아키텍처-채택)
2. [시그니처 라벤더 디자인 시스템과 도메인별 시맨틱 액센트 조화](#2-시그니처-라벤더-디자인-시스템과-도메인별-시맨틱-액센트-조화)
3. [올인원 단일 윈도우 멀티 탭 + 컨텍스트 리본 및 우측 인스펙터 결합 레이아웃](#3-올인원-단일-윈도우-멀티-탭--컨텍스트-리본-및-우측-인스펙터-결합-레이아웃)
4. [파일 확장자 불신 및 매직 바이트 시그니처 기반 Fail-Closed 컨테이너 검증](#4-파일-확장자-불신-및-매직-바이트-시그니처-기반-fail-closed-컨테이너-검증)
5. [한글 문서(HWP 5.0 OLE2 / HWPX KS X 6101) 및 OOXML/ODF/PDF 통합 지원](#5-한글-문서hwp-50-ole2--hwpx-ks-x-6101-및-ooxmlodfpdf-통합-지원)
6. [실무 기준 3계층 이슈 구조(Epic → Sub-issue → PR) 및 GitHub Release 배포 체계](#6-실무-기준-3계층-이슈-구조epic--sub-issue--pr-및-github-release-배포-체계)
7. [비트 수준 바이너리·컨테이너 코덱(ZIP/OLE2/HWP 5.0/XML/CSV) 및 압축 폭탄·XXE·경로 순회 차단 정책](#7-비트-수준-바이너리컨테이너-코덱zipole2hwp-50xmlcsv-및-압축-폭탄xxe경로-순회-차단-정책)
8. [64비트 비트패킹 셀 좌표 및 77종 결정론적 수식·피벗·표 계산 엔진 정책](#8-64비트-비트패킹-셀-좌표-및-77종-결정론적-수식피벗표-계산-엔진-정책)
9. [양방향 포맷 일괄 변환 스튜디오, 오프라인 로컬 스마트 문서 보조 도구 및 메타데이터 비식별화 감사 정책](#9-양방향-포맷-일괄-변환-스튜디오-오프라인-로컬-스마트-문서-보조-도구-및-메타데이터-비식별화-감사-정책)
10. [스플릿 뷰(Split-View) 멀티 탭 워크스페이스, 컨텍스트 리본 명령 카탈로그 및 WCAG AA 접근성 정책](#10-스플릿-뷰split-view-멀티-탭-워크스페이스-컨텍스트-리본-명령-카탈로그-및-wcag-aa-접근성-정책)
11. [전 포맷 라운드트립 패리티·비트 수준 성능 벤치마크 게이트 및 v1.0.0 정식 릴리스 정책 (2026-10-17)](#11-전-포맷-라운드트립-패리티비트-수준-성능-벤치마크-게이트-및-v100-정식-릴리스-정책-2026-10-17)
12. [Slicing-by-8 CRC-32, 하드웨어 비정렬 워드 적재, 무할당 UTF-8 파서, 표준 컨테이너 호환성 및 AST 캐시·메모이제이션 시트 엔진 정책 (2026-10-18)](#12-slicing-by-8-crc-32-하드웨어-비정렬-워드-적재-무할당-utf-8-파서-표준-컨테이너-호환성-및-ast-캐시메모이제이션-시트-엔진-정책-2026-10-18)
13. [ISO 32000-1 바이트 정밀 xref/PDFKit 통합, 140종 컨텍스트 리본 전수 라우팅, Undo/Redo 스택 및 네이티브 파일 I/O 정책 (2026-10-18)](#13-iso-32000-1-바이트-정밀-xrefpdfkit-통합-140종-컨텍스트-리본-전수-라우팅-undoredo-스택-및-네이티브-파일-io-정책-2026-10-18)

---

### 1. macOS 네이티브 Swift 6 + SwiftUI/AppKit 하이브리드 아키텍처 채택

- **Status**: Implemented (초기 아키텍처 설정)
- **Decision**: 애플리케이션 코어(`EasyOfficeCore`), 디자인 시스템(`EasyOfficeDesignSystem`), 데스크톱 앱(`EasyOfficeApp`)을 Swift 6 Strict Concurrency 기반 네이티브 패키지로 구성하고, 선언적 레이아웃(홈 허브, 리본, 인스펙터)은 SwiftUI로, 고밀도 텍스트 조판·대용량 스프레드시트 그리드·PDFKit 캔버스 통합은 AppKit 브리지로 구현합니다.
- **Why**: macOS 데스크톱 오피스 환경에서 즉각적인 콜드 스타트(<100ms), 낮은 메모리 점유율, Retina/ProMotion 고주사율 스크롤, 시스템 다크모드·Vibrancy·단축키·VoiceOver 접근성 통합을 달성하려면 네이티브 프레임워크가 필수적입니다.
- **Trade-offs**: 웹뷰 기반 크로스플랫폼 프레임워크 대비 UI 컴포넌트를 macOS 네이티브로 정밀하게 구현해야 하므로 초기 렌더러 설계 공수가 높습니다.
- **Alternatives rejected**:
  - Electron / Chromium 번들: 기본 유휴 메모리 300MB 이상 및 대용량 시트/문서 스크롤 시 프레임 드랍이 발생하여 기각.
- **Verified by**: `swift build`, `swift test`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`
- **Revisit when**: macOS 외 타 데스크톱 OS용 별도 클라이언트 요구가 공식 로드맵에 추가될 때(코어 엔진만 C-ABI/WASM 모듈로 분리 검토).

---

### 2. 시그니처 라벤더 디자인 시스템과 도메인별 시맨틱 액센트 조화

- **Status**: Implemented (`Sources/EasyOfficeDesignSystem/DesignTokens.swift`)
- **Decision**: 브랜드 기본 액센트로 시그니처 라벤더(`#5C6BC0`, Hover `#4A58A9`, Active `#3B4890`, Light Scaffold `#F7F8FC`, Dark Scaffold `#141724`)를 사용하고, 문서 유형 식별을 위한 보조 시맨틱 액센트(Document Indigo `#5C6BC0`, Sheet Emerald `#0A705A`, Slide Amber-Coral `#D96B27`, PDF Crimson `#D9383A`, HWP Royal Violet `#6E56CF`)를 채도와 명도 대비(WCAG AA 4.5:1 이상)에 맞춰 통합합니다.
- **Why**: 올인원 오피스 워크스페이스에서 현재 열려 있는 탭과 파일 카드가 워드 문서인지, 시트인지, 슬라이드인지, 한글(HWP/HWPX)인지, PDF인지 0.1초 만에 시각적으로 구분하면서도 브랜드 고유의 차분하고 정교한 라벤더 톤앤매너("우리의 결")를 잃지 않게 합니다.
- **Trade-offs**: 임의의 시스템 기본 색상 대신 검증된 디자인 토큰만 사용하도록 강제해야 합니다.
- **Alternatives rejected**:
  - 원색 위주의 고채도 레거시 오피스 색상: 장시간 문서 작업 시 눈의 피로도가 높고 브랜드 일체감을 해쳐 기각.
- **Verified by**: `Tests/EasyOfficeTests/EasyOfficeTests.swift` (`testDesignSystemTokensAndContrast`)
- **Revisit when**: 고대비(High Contrast) 접근성 전용 테마 확장이 필요할 때.

---

### 3. 올인원 단일 윈도우 멀티 탭 + 컨텍스트 리본 및 우측 인스펙터 결합 레이아웃

- **Status**: Implemented (`Sources/EasyOfficeApp/`, `docs/design-system.md`)
- **Decision**: 앱 실행 시 좌측 내비게이션(홈, 최근 문서, 고정 문서, 서식 템플릿, 포맷 일괄 변환, 스마트 보조 도구)을 갖춘 **통합 홈 허브(Home Hub)**를 제공하고, 문서 열기/생성 시 단일 윈도우 내 **멀티 문서 탭 바 + 상단 컴팩트 컨텍스트 리본(Context-Aware Ribbon) + 좌측 구조 내비게이터(썸네일/시트/목차) + 중앙 고해상도 캔버스 + 우측 세부 속성 인스펙터(Inspector) + 하단 상태/줌 바** 구조로 전환합니다.
- **Why**: 상용 올인원 오피스 사용자에게 익숙한 상단 리본 툴바의 기능 탐색성과 macOS 네이티브 생산성 앱의 우측 속성 인스펙터 조작성을 결합하여 학습 비용을 최소화하고 화면 세로 작업 영역을 극대화합니다.
- **Trade-offs**: 리본 명령과 우측 인스펙터 속성 간 양방향 상태 동기화를 일관된 뷰모델(`WorkspaceViewModel`)에서 관리해야 합니다.
- **Alternatives rejected**:
  - 문서 유형마다 별도 애플리케이션 프로세스로 분리: 포맷 간 전환, 통합 검색, 탭 병합이 불가능하여 올인원 오피스 경험이 단절되므로 기각.
  - 3줄 이상의 고정형 대형 리본만 제공: 13~14인치 MacBook 화면에서 문서 편집 세로 공간을 과도하게 잠식하므로 컴팩트 리본 + 우측 인스펙터 조합을 채택.
- **Verified by**: `Tests/EasyOfficeTests/EasyOfficeTests.swift` (`testWorkspaceSessionLifecycleAndTabs`), `Tests/EasyOfficeTests/WorkspaceUIParityTests.swift`
- **Revisit when**: 사용자 정의 단축 툴바 커스터마이징 영속화 스키마를 확장할 때.

---

### 4. 파일 확장자 불신 및 매직 바이트 시그니처 기반 Fail-Closed 컨테이너 검증

- **Status**: Implemented (`Sources/EasyOfficeCore/FormatDetector.swift`)
- **Decision**: 모든 입력 문서는 파일 확장자만 신뢰하지 않고 바이너리 매직 바이트(`PK\x03\x04` ZIP/OOXML/ODF/HWPX, `\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1` OLE2 CFBF/HWP 5.0, `%PDF-` ISO 32000 PDF, UTF-8 텍스트 검증)와 크기 상한을 먼저 검증하며, 시그니처 불일치·빈 페이로드·크기 초과 시 즉시 타입화된 에러(`EasyOfficeError`)로 차단(Fail-Closed)합니다.
- **Why**: 확장자만 바꾼 손상 파일이나 압축 폭탄(ZIP/OLE2 bomb)이 파서 내부로 진입해 크래시나 데이터 오염을 일으키는 것을 입구에서 차단하기 위함입니다.
- **Trade-offs**: 헤더가 손상된 비표준 파일은 자동 복구 모드를 명시적으로 호출하기 전까지 기본 열기가 거부됩니다.
- **Alternatives rejected**:
  - 파싱 실패 시 빈 문서로 대체하거나 텍스트로 강제 디코딩(Silent Fallback): 원본 데이터 유실 및 깨진 문자열 저장 위험이 있어 기각.
- **Verified by**: `Tests/EasyOfficeTests/EasyOfficeTests.swift` (`testFormatDetectorMagicSignaturesAndFailClosed`)
- **Revisit when**: 암호화된 컨테이너(OOXML Agile Encryption / HWP 배포용 문서) 전용 헤더 시그니처 분기를 추가할 때.

---

### 5. 한글 문서(HWP 5.0 OLE2 / HWPX KS X 6101) 및 OOXML/ODF/PDF 통합 지원

- **Status**: Implemented (`Sources/EasyOfficeCore/DocumentFormat.swift`)
- **Decision**: macOS 환경에서 가장 큰 문서 호환성 공백인 한글 문서(`HWP 5.0` OLE2 Compound File 및 `HWPX` KS X 6101 OWPML 표준)를 별도 플러그인이나 유료 티어 분리 없이 기본 1급 시민 포맷으로 `DOCX`, `XLSX`, `PPTX`, `ODT`, `ODS`, `ODP`, `PDF`, `CSV`, `MD`, `TXT`와 동일하게 지원합니다.
- **Why**: 국내 공공·기업·학술 문서를 macOS에서 열람·편집·PDF 변환할 때 서식 깨짐이나 전용 뷰어 부재로 겪는 불편을 단일 네이티브 오피스에서 완전히 해소하기 위함입니다.
- **Trade-offs**: OLE2 스트림(`FileHeader`, `DocInfo`, `BodyText/Section*`, `BinData`)과 OWPML XML 스키마 두 계통의 파서/렌더러를 모두 유지해야 합니다.
- **Alternatives rejected**:
  - 외부 유료 변환 API 의존: 오프라인 동작 불가, 개인정보 유출 위험, 외부 서비스 종속으로 기각.
- **Verified by**: `Tests/EasyOfficeTests/EasyOfficeTests.swift` (`testSupportedFormatCatalogCompleteness`)
- **Revisit when**: 구형 HWP 3.0/97 레거시 바이너리 읽기 전용 임포터 추가 시.

---

### 6. 실무 기준 3계층 이슈 구조(Epic → Sub-issue → PR) 및 GitHub Release 배포 체계

- **Status**: Implemented (`.github/PULL_REQUEST_TEMPLATE.md`, `.github/workflows/release.yml`, `.github/release.yml`)
- **Decision**:
  1. 이슈 트래커는 `Roadmap (#1) → 5대 영역 Epic (#2~#6) → 백로그 항목(GitHub Native Sub-issue) → 자기완결 PR`의 3계층으로 운영합니다.
  2. 측정 수치, 테스트 flake, 코드 리뷰 finding, 소규모 후속 작업마다 새 이슈를 만들지 않고 해당 Sub-issue 또는 Epic 체크리스트 행으로 관리하며, 경미한 유지보수(Class C)는 신규 이슈 없이 PR로 직행합니다.
  3. 릴리스는 `vMAJOR.MINOR.PATCH` 태그 기반으로 `.github/workflows/release.yml`에서 macOS `.app` 번들, `.dmg`, `.zip`, `SHA256SUMS.txt`를 자동 생성하여 GitHub Release에 게시합니다.
- **Why**: 마이크로 이슈·마이크로 PR 난립으로 인한 이슈 번호 고갈, 리뷰 병목, 커밋 히스토리 오염을 방지하고, 사용자가 GitHub Releases 페이지에서 언제든 검증된 최신 macOS 설치 패키지를 바로 내려받을 수 있게 합니다.
- **Trade-offs**: PR 작성 시 연관된 변경(구현 + 테스트 + 문서/정책)을 한 번에 완결성 있게 묶어야 합니다.
- **Alternatives rejected**:
  - 모든 사소한 수정·리뷰 지적마다 개별 Issue와 마이크로 PR 생성: 추적 피로도와 CI 대기 시간을 기하급수적으로 늘려 기각.
- **Verified by**: `node scripts/check-repo-hygiene.mjs`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`
- **Revisit when**: 멀티 아키텍처 공증(Apple Developer ID Notarization) 인증서 시크릿이 CI에 연동될 때.

---

### 7. 비트 수준 바이너리·컨테이너 코덱(ZIP/OLE2/HWP 5.0/XML/CSV) 및 압축 폭탄·XXE·경로 순회 차단 정책

- **Status**: Implemented (`Sources/EasyOfficeCore/BinaryContainerCodec.swift`, `Sources/EasyOfficeCore/TabularAndMarkupCodec.swift`)
- **Decision**:
  1. 모든 바이너리 오피스 컨테이너(`ZIP` 기반 `OOXML`/`ODF`/`HWPX` 및 512바이트 섹터 `OLE2 CFBF` 기반 `HWP 5.0`)는 외부 CLI 프로세스 호출 없이 순수 Swift 비트 연산(`BitPacking`, IEEE 802.3 `CRC32Calculator` 256-entry 룩업 테이블, `HWP5RecordHeader` `10-bit tagID | 10-bit level | 12-bit size` + `0xFFF` 확장 헤더)으로 직렬화·역직렬화합니다.
  2. `ZipContainerCodec`은 엔트리 수 상한(`4,096`), 단일 엔트리 해제 상한(`128 MiB`), 전체 해제 상한(`512 MiB`), 최대 압축 비율(`100:1`)을 강제하고, `..`·절대 경로(`/`)·백슬래시(`\`)·NUL 바이트가 포함된 경로를 즉시 `EasyOfficeError.securityViolation`으로 차단하며, 압축 폭탄 감지 시 `EasyOfficeError.decompressionBombDetected`를 반환합니다.
  3. `XMLSanitizer`는 컨테이너 내부 XML 파싱 전 `<!ENTITY` 및 외부 DTD(`SYSTEM`/`PUBLIC`/내부 서브셋 `[`)를 검사하여 `EasyOfficeError.xxeOrExternalEntityDetected`로 즉시 차단합니다.
  4. `HWP5BinaryCodec`은 256바이트 `FileHeader` 오프셋 36의 속성 비트필드에서 암호화 비트(`bit 1 = 0x02`) 또는 배포용 문서 비트(`bit 2 = 0x04`)가 설정된 경우 가짜 텍스트로 폴백하지 않고 `EasyOfficeError.encryptedDocumentUnsupported`를 반환합니다.
- **Why**: 오프라인·서버리스 데스크톱 앱에서 외부 악성 문서(ZIP/OLE2 압축 폭탄, 경로 순회 덮어쓰기, XXE 파일 유출, FAT 섹터 순환 루프)로부터 사용자 시스템과 메모리를 결정론적으로 보호하고, 비트 수준 패킹으로 파싱 지연을 최소화하기 위함입니다.
- **Trade-offs**: 비표준 외부 DTD를 참조하는 구형 XML 문서나 손상된 CRC-32 엔트리는 자동으로 무시되지 않고 명시적 오류로 거부됩니다.
- **Alternatives rejected**:
  - 시스템 `/usr/bin/unzip` 또는 외부 프로세스 스폰: 임시 디렉터리 경로 순회 취약점 및 프로세스 생성 오버헤드가 발생하여 기각.
- **Verified by**: `Tests/EasyOfficeTests/BinaryContainerCodecTests.swift`
- **Revisit when**: 암호화된 HWP 5.0 및 OOXML Agile Encryption 비밀번호 복호화 키 유도 모듈을 추가할 때.

---

### 8. 64비트 비트패킹 셀 좌표 및 77종 결정론적 수식·피벗·표 계산 엔진 정책

- **Status**: Implemented (`Sources/EasyOfficeCore/SheetEngine.swift`, `Sources/EasyOfficeCore/DocumentEngine.swift`)
- **Decision**:
  1. 스프레드시트 셀 좌표는 `PackedCellCoordinate`(`UInt64`: 상위 32비트 `row`, 하위 32비트 `column`)로 비트패킹하여 문자열 해싱 없이 $O(1)$ 조회 및 행 우선(row-major) 비교를 수행하며, `CellAddress`는 ECMA-376 규격 상한(`maxZeroBasedColumn = 16,383`, `maxZeroBasedRow = 1,048,575`)과 오버플로 검사를 강제합니다.
  2. `CellRange`의 단일 수식 범위 확장은 메모리 고갈 방지를 위해 `maxExpandedCellCount = 100,000` 셀을 상한으로 제한하고 초과 시 `EasyOfficeError.invalidCellRange`를 반환합니다.
  3. `WorksheetModel`의 77종 내장 스프레드시트 함수(수학/삼각 27종, 통계 7종, 논리 10종, 찾기/참조 8종, 텍스트 16종, 재무 4종, 날짜/시간 5종) 및 피벗 테이블 집계 엔진(`PivotTableEngine`)은 `PackedCellCoordinate` 방문 집합(`visited: Set<PackedCellCoordinate>`)으로 순환 참조 및 재귀 깊이 상한(`maxRecursionDepth = 64`) 초과 감지 시 즉시 `#REF!`를 반환하며, 문서/HWP 인라인 표 계산식(`DocumentTableBlock.evaluateTableFormula`)은 방향성(`ABOVE`/`LEFT`) 수치 리터럴만 비재귀적으로 집계하여 `Double?`을 반환합니다.
- **Why**: 대용량 시트 및 HWP/문서 표 계산 시 문자열 키 할당 오버헤드와 악의적인 전체 시트 범위(`A1:XFD1048576`) 확장에 따른 메모리 폭증을 원천 차단하기 위함입니다.
- **Trade-offs**: 단일 수식에서 10만 셀을 초과하는 범위 참조는 명시적으로 분할 집계하거나 피벗 엔진을 사용해야 합니다.
- **Alternatives rejected**:
  - 문자열 키(`"A1"`)만을 사용하는 범위 정렬 및 무제한 배열 확장: 대량 셀 연산 시 힙 할당과 OOM 위험이 커 기각.
- **Verified by**: `Tests/EasyOfficeTests/EditorEnginesParityTests.swift`
- **Revisit when**: 백그라운드 멀티스레드 청크 배열 수식(Dynamic Array Spill) 엔진을 확장할 때.

---

### 9. 양방향 포맷 일괄 변환 스튜디오, 오프라인 로컬 스마트 문서 보조 도구 및 메타데이터 비식별화 감사 정책

- **Status**: Implemented (`Sources/EasyOfficeCore/SmartProductivityTools.swift`, `Sources/EasyOfficeCore/PDFBinarySerializer.swift`)
- **Decision**:
  1. `DocumentConversionOrchestrator`는 모든 오피스 도메인(`.document`, `.sheet`, `.slide`, `.pdf`)과 20종 포맷(`.docx`, `.odt`, `.hwpx`, `.hwtx`, `.hwp`, `.hwt`, `.doc`, `.rtf`, `.md`, `.txt`, `.xlsx`, `.xls`, `.ods`, `.csv`, `.tsv`, `.pptx`, `.ppsx`, `.odp`, `.ppt`, `.pdf`) 간 단건·배치 변환을 수행하며, 변환된 모든 출력 바이트에 대해 반환 전 `FormatDetector.verify(data:fileName:)` 매직 시그니처 검증을 강제합니다.
  2. `LocalSmartDocumentAssistant`는 외부 클라우드 LLM이나 네트워크 API를 전혀 호출하지 않는 100% 오프라인 결정론적 알고리즘(단어 빈도·위치 가중치 문장 중요도 스코어링 요약, 어절 경계 장문 분할·피동 표현·중복 수식어 교정 진단, 사전 컴파일 정적 정규식 기반 기한·예산·수치·조치 항목 추출, 시트 Z-Score 이상치(`|z| >= 1.8`) 및 추세 분석, 1차원 평탄화 `[Int32]` 버퍼 기반 최대 2,048 문단 상한 LCS 동적 계획법 문단 비교)으로 동작합니다.
  3. `DocumentSecurityAuditor`는 문서 공유 전 머리말/꼬리말 PII 마커 제거, 변경 추적 작성자 `"Redacted"` 익명화, 코멘트 일괄 삭제(`scrubMetadata`)를 수행하고, 외부 컨테이너에 대해 매직 시그니처·ZIP 폭탄·경로 순회·XXE·암호화 여부를 종합 진단(`auditContainer`)합니다.
- **Why**: 기업·공공·개인 환경에서 민감한 문서를 외부 서버로 전송하지 않고도 로컬에서 즉시 포맷 일괄 변환, 문서 요약·비교·인사이트 도출, 개인정보 비식별화 및 컨테이너 보안 감사를 완결하기 위함입니다.
- **Trade-offs**: 문단 비교(LCS)는 이차 메모리($O(N \times M)$) 폭증 방지를 위해 비교 대상을 문서당 최대 `2,048` 문단으로 상한 제한합니다.
- **Alternatives rejected**:
  - 외부 원격 AI API 호출 기반 요약/교정: 서버리스·오프라인 불변식 위반 및 문서 내용 유출 위험으로 기각.
- **Verified by**: `Tests/EasyOfficeTests/SmartProductivityToolsTests.swift`
- **Revisit when**: 로컬 CoreML/NaturalLanguage 온디바이스 임베딩 선택적 가속 옵션을 추가할 때.

---

### 10. 스플릿 뷰(Split-View) 멀티 탭 워크스페이스, 컨텍스트 리본 명령 카탈로그 및 WCAG AA 접근성 정책

- **Status**: Implemented (`Sources/EasyOfficeDesignSystem/DesignTokens.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeApp/WorkspaceViews.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`)
- **Decision**:
  1. `WorkspaceViewModel`은 단일 윈도우에서 두 개의 열린 탭을 좌우로 나란히 비교·편집할 수 있는 스플릿 뷰(`splitSecondaryTabID`, 최소 너비 `splitPaneMinimumWidth = 420pt`)를 지원하며, 보조 탭이 닫히거나 활성 탭과 동일해지면 자동으로 단일 캔버스 모드로 복귀합니다.
  2. `RibbonCategory`는 7대 카테고리(`home`, `insert`, `layout`, `formulasAndData`, `moduleSpecific`, `review`, `view`) × 5대 오피스 모듈(`document`, `sheet`, `slide`, `pdf`, `hwp`) 조합에 대해 빈 슬롯 없는 결정론적 `RibbonCommandDescriptor` 카탈로그를 제공하며, `isRibbonCollapsed`(`Cmd+Option+R`) 토글로 컴팩트 리본과 1줄 축소 모드를 즉시 전환합니다.
  3. `WorkspaceThemePalette`(`EasyOfficeDesignTokens`)의 라이트 라벤더(`#F7F8FC`)·다크 인디고(`#141724`) 표면 팔레트와 시맨틱 상태 토큰(`statusSuccess`, `statusWarning`, `statusDanger`, `statusInfo`)은 모두 WCAG 2.1 AA 명도 대비(`>= 4.5:1`)를 충족하며, 모든 핵심 컨트롤에 `AccessibilityTokens` 기반 VoiceOver 레이블·힌트를 부여합니다.
- **Why**: 문서 대조(원본 HWPX와 번역 DOCX, 시트 데이터와 슬라이드 차트 등) 시 다중 윈도우 전환 비용을 없애고, 시그니처 라벤더 디자인 일관성과 시각 접근성을 동시에 보장하기 위함입니다.
- **Trade-offs**: 스플릿 뷰 활성화 시 좌우 캔버스가 각각 최소 420pt 이상을 확보해야 하므로 좁은 창에서는 좌/우 보조 인스펙터를 접어 캔버스 가독성을 우선합니다.
- **Alternatives rejected**:
  - 스플릿 뷰에서 동일 탭 ID를 중복 바인딩 허용: 편집 상태 경합 가능성이 있어 `secondaryTabID == activeTabID`인 경우 스플릿 뷰를 비활성화하도록 결정.
- **Verified by**: `Tests/EasyOfficeTests/WorkspaceUIParityTests.swift`
- **Revisit when**: 3분할 이상 멀티 모니터 도킹 레이아웃 프리셋을 추가할 때.

---

### 11. 전 포맷 라운드트립 패리티·비트 수준 성능 벤치마크 게이트 및 v1.0.0 정식 릴리스 정책 (2026-10-17)

- **Status**: Implemented (`Tests/EasyOfficeTests/CommercialParityBenchmarkTests.swift`, `scripts/package-macos-release.sh`, `.github/workflows/release.yml`)
- **Decision**:
  1. `CommercialParityBenchmarkTests`를 통해 지원 대상 **20종 문서·템플릿 포맷 전수**(`.docx`, `.doc`, `.odt`, `.rtf`, `.md`, `.txt`, `.hwp`, `.hwpx`, `.hwt`, `.hwtx`, `.xlsx`, `.xls`, `.ods`, `.csv`, `.tsv`, `.pptx`, `.ppt`, `.ppsx`, `.odp`, `.pdf`)에 대한 바이너리/컨테이너 직렬화 → `FormatDetector.verify` 매직 바이트 시그니처 검증 → 역직렬화 → 도메인 구조 복원(문단·표·셀·슬라이드·페이지·유니코드/한글 보존) 엔드투엔드 라운드트립 패리티를 상시 검증합니다.
  2. 비트 수준 코덱(`CRC32Calculator` 256-entry 룩업 테이블, `HWP5RecordHeader` 32비트 비트패킹 + `0xFFF` 확장 헤더, `BitPacking` 리틀 엔디언 정수 및 길이 접두 UTF-16LE 인코딩, 64비트 `PackedCellCoordinate`), 77종 내장 수식 밀집 그리드 평가 및 `PivotTableEngine` 집계, `DocumentConversionOrchestrator` 배치 변환, `LocalSmartDocumentAssistant` 오프라인 문서 요약·문장 교정·LCS 비교, `DocumentSecurityAuditor` 컨테이너 보안 감사·메타데이터 비식별화가 모두 서브초(sub-second) 예산 내에 결정론적으로 완료됨을 검증 게이트로 강제합니다.
  3. `v1.0.0` 정식 릴리스는 외부 서버·클라우드 API·원격 LLM·텔레메트리 호출이 전혀 없는 **100% 독립 실행형(Standalone)·서버리스(Serverless)·오프라인 지원 macOS 네이티브 아키텍처**를 보장하며, `scripts/package-macos-release.sh`를 통해 `EasyOffice.app`, `.dmg`, `.zip`, `SHA256SUMS.txt`를 결정론적으로 생성·검증합니다.
- **Why**: 상용 올인원 오피스 수준의 20종 포맷 호환성과 비트 수준 처리 성능, 오프라인 프라이버시 무결성을 추상적 선언이 아닌 실행 가능한 자동화 테스트(`CommercialParityBenchmarkTests`)와 패키징 스크립트로 영구 고정하기 위함입니다.
- **Trade-offs**: 모든 PR 및 릴리스 빌드마다 20종 포맷 라운드트립과 비트 수준 처리량 벤치마크 스위트가 함께 실행됩니다.
- **Alternatives rejected**:
  - 일부 대표 포맷(`.docx`, `.xlsx`, `.pdf`)만 샘플링 테스트하고 레거시/서식 포맷(`.hwt`, `.hwtx`, `.doc`, `.xls`, `.ppt`, `.ppsx`, `.odp`) 검증 생략: 실제 공문서 및 서식 배포 시 컨테이너 시그니처 회귀를 놓칠 수 있어 20종 전수 검증을 채택.
- **Verified by**: `Tests/EasyOfficeTests/CommercialParityBenchmarkTests.swift`, `node scripts/check-repo-hygiene.mjs`, `bash scripts/package-macos-release.sh 1.0.0`
- **Revisit when**: 차기 메이저/마이너 릴리스에서 신규 문서 표준 포맷이나 대용량 스트리밍 병렬 코덱을 추가할 때.

---

### 12. Slicing-by-8 CRC-32, 하드웨어 비정렬 워드 적재, 무할당 UTF-8 파서, 표준 컨테이너 호환성 및 AST 캐시·메모이제이션 시트 엔진 정책 (2026-10-18)

- **Status**: Implemented (`Sources/EasyOfficeCore/BinaryContainerCodec.swift`, `Sources/EasyOfficeCore/StandardZipOfficeCodecs.swift`, `Sources/EasyOfficeCore/OLE2AndLegacyOfficeCodecs.swift`, `Sources/EasyOfficeCore/TabularAndMarkupCodec.swift`, `Sources/EasyOfficeCore/SheetEngine.swift`, `Sources/EasyOfficeCore/SheetAnalyticsAndPivot.swift`)
- **Decision**:
  1. **비트 수준 메모리·바이트 처리량 최적화**:
     - `BitPacking`은 바이트 단위 시프트 루프 대신 `UnsafeRawBufferPointer.loadUnaligned(fromByteOffset:as:)` 및 `storeBytes(of:toByteOffset:as:)` 단일 하드웨어 리틀 엔디언 워드 적재/저장을 사용하며, `CRC32Calculator`는 `8 × 256` 룩업 테이블 기반 **Slicing-by-8** 알고리즘으로 루프당 8바이트씩 처리합니다.
     - `XMLSanitizer.validateSafeXML`은 전체 문자열 `.uppercased()` 복제 없이 UTF-8 바이트 버퍼에서 직접 대소문자 무시(`byte & 0xDF`) `<!DOCTYPE`/`<!ENTITY` 패턴을 스캔하며, `extractElements`/`stripElements`는 중첩 태그 깊이를 추적하고 작은따옴표(`'`)·큰따옴표(`"`) 속성을 모두 파싱합니다.
     - `CSVStateMachineCodec`은 `Array(text)`(`Character`당 16바이트 힙 할당)를 제거하고 UTF-8 바이트 버퍼(`Array(text.utf8)`)에서 직접 필드 경계를 슬라이싱하여 디코딩합니다.
  2. **외부 표준 오피스 컨테이너 양방향 호환성 및 스트림 압축**:
     - `OOXMLContainerCodec`은 내부 라운드트립 확장 속성과 표준 ECMA-376 요소(`<w:pPr>/<w:rPr>/<w:tblGrid>/<w:tcPr>`, 다중 `<row r="N">` 및 `xl/sharedStrings.xml` 공유 문자열 테이블 `t="s"`, DrawingML `<p:cSld><p:spTree><p:sp>`)를 동시에 지원하며, 본문 문단 추출 전 `<w:tbl>` 영역을 제거하여 표 셀 문단 중복 추출을 방지합니다.
     - `HWP5BinaryCodec`은 `FileHeader` `flagCompressed (0x01)` 비트를 지원하여 `DocInfo` 및 `BodyText/Section0` 스트림을 Raw Deflate(`COMPRESSION_ZLIB`)로 압축·해제하며, `LegacyOLE2OfficeCodec`은 `.doc`(`FibBase` `0xA5EC`), `.xls`(BIFF8 `BOF`/`DIMENSIONS`/`NUMBER`/`LABEL`/`EOF`), `.ppt`(`RT_Document`/`RT_Slide`/`RT_TextCharsAtom`) 바이너리 레코드 스트림을 직접 직렬화·역직렬화합니다.
  3. **스프레드시트 64비트 좌표 주 저장소, 사전 컴파일 `FormulaAST` 캐시 및 $O(V+E)$ DAG 메모이제이션**:
     - `WorksheetModel`의 주 저장소를 `cellsByCoordinate: [PackedCellCoordinate: String]`으로 전환하고 `setCell` 시점에 수식을 1회 파싱하여 `compiledFormulas: [PackedCellCoordinate: FormulaAST]`에 캐싱합니다.
     - 단일 평가 패스 내에서 `EvaluationContext`(`visited` 순환 감지 + `memo` 결과 캐시)를 공유하고 함수 인자 사전 중복 평가 루프를 제거하여, 다이아몬드 의존성 체인의 시간 복잡도를 지수 시간($O(2^d)$)에서 선형 시간($O(V+E)$)으로 단축합니다.
- **Why**: 외부 표준 오피스 도구와의 실제 파일 상호운용성을 확보하고, 대용량 컨테이너/CSV/다중 참조 수식 시트에서 발생하던 불필요한 힙 할당과 지수 재평가 병목을 근본적으로 제거하기 위함입니다.
- **Trade-offs**: `WorksheetModel`에 `compiledFormulas` AST 캐시가 함께 유지되어 셀당 약간의 추가 메모리를 사용하나, 반복 평가 시 문자열 재파싱 비용이 완전히 제거됩니다.
- **Alternatives rejected**:
  - 평가 시점마다 원시 수식 문자열을 재토큰화·재파싱하는 방식: 다중 셀 참조 및 차트/피벗 집계 시 CPU 병목을 유발하여 기각.
  - XML 전체 문자열을 `.uppercased()`로 복제하여 XXE를 검사하는 방식: 수십 MB XML 엔트리에서 불필요한 2배 힙 할당을 유발하여 기각.
- **Verified by**: `Tests/EasyOfficeTests/BinaryContainerCodecTests.swift`, `Tests/EasyOfficeTests/EditorEnginesParityTests.swift`
- **Revisit when**: SIMD 벡터화 CSV/XML 스캐너 또는 멀티스레드 백그라운드 수식 재계산 큐를 도입할 때.

---

### 13. ISO 32000-1 바이트 정밀 xref/PDFKit 통합, 140종 컨텍스트 리본 전수 라우팅, Undo/Redo 스택 및 네이티브 파일 I/O 정책 (2026-10-18)

- **Status**: Implemented (`Sources/EasyOfficeCore/PDFBinarySerializer.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeApp/EasyOfficeApp.swift`, `Sources/EasyOfficeApp/WorkspaceViews.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`)
- **Decision**:
  1. **ISO 32000-1 바이트 정밀 `xref` 오프셋 테이블 및 macOS `PDFKit` 브리지 (`PDFBinarySerializer.swift`)**:
     - 각 간접 객체(`N 0 obj`) 직렬화 직전의 실제 `Data.count` 바이트 오프셋을 기록하여 20바이트 표준 `xref` 엔트리(`%010d 00000 n \n`)와 정확한 `startxref` 위치를 생성하고, 각 `/Type /Page` 객체에 `/Resources << /Font << /F1 ... >> >>` 사전을 포함하여 macOS 네이티브 `PDFKit.PDFDocument(data:)` 및 `PDFView`(`PDFKitRepresentedView`)에서 즉시 렌더링·텍스트 추출이 가능하도록 통합합니다.
  2. **140종 컨텍스트 리본 전수 라우팅, 값 타입 스냅샷 `Undo/Redo` 스택 및 실제 디스크 파일 I/O**:
     - `WorkspaceViewModel.executeRibbonCommand(_:)`는 5대 오피스 모듈 × 7대 리본 카테고리의 **140개 전수 명령 ID**를 단 하나의 무동작(no-op) 없이 실제 도메인 모델 변이 및 워크스페이스 액션으로 라우팅합니다.
     - 탭별 최대 50단계 값 타입(`OpenTabPayload`) 스냅샷 `undoStacks`/`redoStacks`(`undo()`, `redo()`)를 제공하고, `openDocument(data:fileName:fileURL:)`, `loadFromDisk(url:)`, `exportTabData(id:targetFormat:)`, `saveTabToDisk(id:to:targetFormat:)` 및 macOS `NSOpenPanel`/`NSSavePanel`을 통해 20종 포맷의 실제 디스크 열기·저장을 수행합니다.
  3. **홈 허브 전용 스튜디오 뷰 및 인터랙티브 에디터 캔버스**:
     - 홈 허브의 `.converter` 섹션에 `BatchConversionStudioHubView`, `.smartAssistant` 섹션에 `SmartDocumentAssistantHubView`를 연결하고, 인라인 문단/표 셀 편집, 동적 스프레드시트 그리드 및 비례 벡터 차트, 16:9 좌표 기반 슬라이드 스테이지 및 점선 스냅 가이드, 네이티브 `PDFKit` 프리뷰를 제공합니다.
- **Why**: 데모성 하드코딩이나 무응답 리본 버튼을 완전히 배제하고, 실제 파일 열기/편집/실행 취소/저장/변환이 완결되는 프로덕션 데스크톱 오피스 경험을 보장하기 위함입니다.
- **Trade-offs**: 탭당 최대 50단계의 `OpenTabPayload` 값 스냅샷을 유지하나, Swift 구조체 Copy-on-Write(CoW) 특성 덕분에 변경된 하위 컬렉션만 복사되어 메모리 오버헤드가 최소화됩니다.
- **Alternatives rejected**:
  - 근사치(`index * 64`)로 PDF `xref` 오프셋을 기록하는 방식: 표준 PDF 파서 및 `PDFKit`에서 `xref` 복구 경고를 유발하므로 실제 바이트 오프셋 기록 방식으로 교체.
- **Verified by**: `Tests/EasyOfficeTests/SmartProductivityToolsTests.swift`, `Tests/EasyOfficeTests/WorkspaceUIParityTests.swift`, `Tests/EasyOfficeTests/CommercialParityBenchmarkTests.swift`
- **Revisit when**: 대용량 이미지 첨부 문서에서 델타(Command Log) 기반 압축 Undo 히스토리를 도입할 때.

---

### 14. 다중 시트 워크북, 셀 서식(`CellStyle`), 런/이미지 직렬화, 외부 표준 `.ods`/BIFF8 `.xls` 위치 기반 파싱 및 무할당(`Zero-Array(String)`) 핫패스 정책 (2026-10-18)

- **Status**: Implemented (`Sources/EasyOfficeCore/SheetEngine.swift`, `Sources/EasyOfficeCore/SheetBuiltInFunctions.swift`, `Sources/EasyOfficeCore/DocumentEngine.swift`, `Sources/EasyOfficeCore/SlideEngine.swift`, `Sources/EasyOfficeCore/StandardZipOfficeCodecs.swift`, `Sources/EasyOfficeCore/OLE2AndLegacyOfficeCodecs.swift`, `Sources/EasyOfficeCore/LocalSmartAssistant.swift`)
- **Decision**:
  1. **다중 시트 워크북(`WorksheetSheetPage`), 일급 셀 서식(`CellStyle`) 및 단일 컨텍스트 그리드 일괄 평가(`evaluateGrid`)**:
     - `WorksheetModel`에 다중 시트 페이지 컬렉션(`additionalSheets`, `activeSheetIndex`, `addSheetPage`, `switchActiveSheet`), 좌표별 셀 서식(`cellStyles: [PackedCellCoordinate: CellStyle]`, `CellNumberFormat`, `CellHorizontalAlignment`), `pageSetup: WorksheetPageSetup`, `sheetComments: [PackedCellCoordinate: String]`, `trackChangesEnabled: Bool`을 도입하여 임의 셀(`E1~F6`) 오염 없이 도메인 상태를 정식 보존합니다.
     - `evaluateGrid(rows:columns:)`는 단일 `EvaluationContext`를 공유하여 그리드 전체 셀을 $O(V+E)$에 일괄 평가하며, `detectedDataRange()` 및 `detectedNumericRange()`가 실제 점유된 셀 경계를 동적으로 계산합니다.
  2. **문단 내 부분 런(`DocumentTextRun`)·인라인 이미지(`EmbeddedImageAttachment`) 및 외부 표준 `.ods`/`.xlsx`/`.pptx`/`.xls` 상호운용성**:
     - `ParagraphBlock.applyRunFormatting`으로 선택 영역 부분 런 서식을 분할·적용하고, `.docx`(`<w:r>`, `word/media/`), `.odt`(`<text:span>`, `Pictures/`), `.hwpx`(`<hp:run>`, `BinData/`)에 런과 이미지를 양방향 직렬화합니다.
     - `ODFContainerCodec.decodeWorksheet`는 비표준 `address`/`raw` 속성이 없는 외부 표준 `.ods` 파일에 대해 `<table:table-row>` 및 `<table:table-cell>` 행·열 위치(`table:number-rows-repeated`/`table:number-columns-repeated` 최대 1,024 클램프, `table:formula="of:=..."` 정규화)로 모든 셀을 복원하며, `LegacyOLE2OfficeCodec.decodeBIFF8Worksheet`는 `SST`(`0x00FC`)+`LABELSST`(`0x00FD`), `RK`(`0x027E`), `FORMULA`(`0x0006`) 레코드를 디코딩합니다.
  3. **`Array(String)` (`[Character]`) 힙 할당 제거 및 코어 파일 모듈 분리**:
     - `SheetEngine.swift`에서 77개 내장 함수를 `SheetBuiltInFunctions.swift`로, `SmartProductivityTools.swift`에서 `LocalSmartDocumentAssistant`를 `LocalSmartAssistant.swift`로 분리하고, `FormulaASTCompiler`, `MID`, `XMLSanitizer.isSelfClosingTag`, `CommonMarkCodec.parsePipeRow`, `splitLongSentenceAtWordBoundary`의 `Array(String)` 할당을 UTF-8 바이트 및 `String.Index` 슬라이싱으로 대체했습니다.
- **Why**: 외부 표준 `.ods`/`.xls`/`.xlsx`/`.pptx` 문서와의 실질적인 호환성을 보장하고, 수식 컴파일 및 마크다운/문장 분석 핫패스에서 발생하던 16바이트 `Character` 배열 할당을 제거하기 위함입니다.
- **Trade-offs**: 다중 시트 전환 시 활성 시트 슬롯과 `additionalSheets` 배열 간 스왑이 수행되나, 값 타입 `Dictionary` 포인터 이동만 발생하므로 $O(1)$에 동작합니다.
- **Alternatives rejected**:
  - `.ods`에서 자체 `address`/`raw` 속성만 파싱하는 방식: 외부 표준 오피스 도구에서 저장한 `.ods` 파일을 열 때 빈 시트로 열리는 데이터 유실을 유발하므로 기각.
- **Verified by**: `Tests/EasyOfficeTests/BinaryContainerCodecTests.swift`, `Tests/EasyOfficeTests/EditorEnginesParityTests.swift`
- **Revisit when**: 시트 간 교차 수식 참조(`Sheet2!A1`) AST 노드를 확장할 때.

---

### 15. Zero-Slop 도메인 리본 라우팅, 인라인 편집 Undo Coalescing, 다중 페이지 가시 PDF 렌더링 및 인터랙티브 UI 모듈 분리 정책 (2026-10-18)

- **Status**: Implemented (`Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeCore/WorkspaceRibbonCommandRouter.swift`, `Sources/EasyOfficeCore/PDFEngine.swift`, `Sources/EasyOfficeCore/PDFBinarySerializer.swift`, `Sources/EasyOfficeApp/WorkspaceViews.swift`, `Sources/EasyOfficeApp/HomeHubAndSmartViews.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`, `Sources/EasyOfficeApp/SlideAndPDFDetailPanes.swift`)
- **Decision**:
  1. **Zero-Slop 140종 컨텍스트 리본 도메인 변이 (`WorkspaceRibbonCommandRouter.swift`)**:
     - 스프레드시트(`.sheet`)의 28개 리본 명령에서 기존 `E1~E8`·`F1~F6` 임의 셀 텍스트 오염을 100% 제거하고, `sheet.cellStyles`(`CellStyle`), `sheet.pageSetup`(`WorksheetPageSetup`), `sheet.sheetComments`, `sheet.addSheetPage`, `sheet.trackChangesEnabled`, `sheet.detectedNumericRange()`, `sheet.detectedDataRange()`, `PivotTableEngine.compute` 등 실제 도메인 상태 변이로 전환합니다.
     - 슬라이드(`.slide`) 홈 서식 명령은 선택 개체의 `isBold`/`isItalic`/`isUnderlined`를 직접 토글하며, `slide-mod-snap`은 개체를 항상 스테이지 정중앙(`(stageWidth - width)/2, (stageHeight - height)/2`)에 정렬한 뒤 `SnapGuideCalculator.computeGuides`로 `activeSnapGuides`를 갱신합니다.
     - `applyFormulaBarEdit()`는 `activeDataValidationRule`이 활성화된 경우 수식/리터럴 입력값을 사전 검증하여 위반 시 셀 덮어쓰기를 거부하고 `lastErrorMessage`를 설정(Fail-Closed)합니다.
  2. **인라인 타이핑 Undo Coalescing (`recordUndoSnapshot(for:coalescingTargetKey:)`)**:
     - 동일 문단(`updateParagraphText`) 또는 동일 표 셀(`updateTableCell`)에서 연속 타이핑 시 편집 타깃 키(`activeInlineEditTargetKey`) 단위로 스냅샷을 병합하여 첫 키 입력 시점에만 1회 Undo 스냅샷을 기록하고, 리본 명령·탭 전환·Undo/Redo 시 타깃 키를 초기화합니다.
  3. **다중 페이지 PDF 페이지네이션 및 ISO 32000-1 한국어 CIDFont 가시 렌더링 (`PDFEngine.swift`, `PDFBinarySerializer.swift`)**:
     - `PDFStudioDocumentModel.fromDocument` 및 `fromWorksheet`는 페이지당 분량(문단·표 블록 `12개`, 시트 `25행`) 기준으로 다중 페이지(`PDFPageDescriptor`)를 자동 분할하고 목차 아웃라인의 `targetPageNumber`를 실제 페이지 번호에 매핑합니다.
     - `PDFBinarySerializer.encode`는 정적 4막대 스켈레톤 사각형을 제거하고, `/F2` 한국어 `/Type0` CIDFont(`/BaseFont /AppleSDGothicNeo-Regular /Encoding /UniKS-UTF16-H` + `Adobe-Korea1-2`)와 UTF-16BE 헥스 스트링(`<FEFF...>`)으로 실제 문서 본문 다중 줄(`0 Tr`) 텍스트와 주석 콜아웃 박스를 페이지에 직접 렌더링합니다.
  4. **인터랙티브 UI 컨트롤 및 뷰 모듈 분리 (`EasyOfficeApp`)**:
     - 찾기 및 바꾸기 바(`⌘F`, `performFindAndReplace()`), 인쇄(`⌘P`, `NSPrintOperation`), 클립보드 복사/붙여넣기(`⌘C`/`⌘V`), 하단 다중 시트 탭 바, 인라인 텍스트 런/이미지 첨부 카드, 슬라이드 개체 클릭 선택·`DragGesture` 이동·리사이즈·발표자 실시간 타이머, `PDFKitRepresentedView` `Coordinator` 모델 캐싱을 제공합니다.
- **Why**: 형식적 상태 변화 통과용 셀 오염(`E1~F6`)과 정적 스켈레톤 PDF 프리뷰를 원천 제거하고, 연속 타이핑 시의 메모리/Undo 오염을 방지하여 실사용 수준의 macOS 오피스 UX를 달성하기 위함입니다.
- **Trade-offs**: `PDFKitRepresentedView`에서 `Coordinator.lastRenderedModel` 비교를 수행하나, 변경이 없는 뷰 갱신 시 PDF 재직렬화를 완전히 생략하여 UI 반응성을 크게 높입니다.
- **Alternatives rejected**:
  - 모든 키 입력마다 전체 `OpenTabPayload`를 `undoStacks`에 push하는 방식: 50자만 입력해도 50단계 Undo 스택이 단일 문단 타이핑으로 가득 차 이전 작업 복원이 불가능해지므로 기각.
- **Verified by**: `Tests/EasyOfficeTests/SmartProductivityToolsTests.swift`, `Tests/EasyOfficeTests/WorkspaceUIParityTests.swift`
- **Revisit when**: PDF 내장 TrueType 서브셋 폰트(`/Type0` `/CIDFontType2`) 바이너리 임베딩을 추가할 때.
