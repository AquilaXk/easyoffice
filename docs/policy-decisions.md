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
- **Decision**: 앱 실행 시 좌측 내비게이션(홈, 최근 문서, 고정 문서, 서식 템플릿, 포맷 일괄 변환)을 갖춘 **통합 홈 허브(Home Hub)**를 제공하고, 문서 열기/생성 시 단일 윈도우 내 **멀티 문서 탭 바 + 상단 컴팩트 컨텍스트 리본(Context-Aware Ribbon) + 좌측 구조 내비게이터(썸네일/시트/목차) + 중앙 고해상도 캔버스 + 우측 세부 속성 인스펙터(Inspector) + 하단 상태/줌 바** 구조로 전환합니다.
- **Why**: 상용 올인원 오피스 사용자에게 익숙한 상단 리본 툴바의 기능 탐색성과 macOS 네이티브 생산성 앱의 우측 속성 인스펙터 조작성을 결합하여 학습 비용을 최소화하고 화면 세로 작업 영역을 극대화합니다.
- **Trade-offs**: 리본 명령과 우측 인스펙터 속성 간 양방향 상태 동기화를 일관된 뷰모델(`WorkspaceSession`)에서 관리해야 합니다.
- **Alternatives rejected**:
  - 문서 유형마다 별도 애플리케이션 프로세스로 분리: 포맷 간 전환, 통합 검색, 탭 병합이 불가능하여 올인원 오피스 경험이 단절되므로 기각.
  - 3줄 이상의 고정형 대형 리본만 제공: 13~14인치 MacBook 화면에서 문서 편집 세로 공간을 과도하게 잠식하므로 컴팩트 리본 + 우측 인스펙터 조합을 채택.
- **Verified by**: `Tests/EasyOfficeTests/EasyOfficeTests.swift` (`testWorkspaceSessionLifecycleAndTabs`)
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
  3. `WorksheetModel`의 77종 내장 스프레드시트 함수(수학/삼각 27종, 통계 7종, 논리 10종, 찾기/참조 8종, 텍스트 16종, 재무 4종, 날짜/시간 5종) 및 피벗 테이블 집계 엔진(`PivotTableEngine`)은 `PackedCellCoordinate` 방문 집합(`visited: Set<PackedCellCoordinate>`)으로 순환 참조 및 재귀 깊이 상한(`maxFormulaRecursionDepth = 256`) 초과 감지 시 즉시 `#REF!`를 반환하며, 문서/HWP 인라인 표 계산식(`DocumentTableBlock.evaluateTableFormula`)은 방향성(`ABOVE`/`LEFT`) 수치 리터럴만 비재귀적으로 집계하여 `Double?`을 반환합니다.
- **Why**: 대용량 시트 및 HWP/문서 표 계산 시 문자열 키 할당 오버헤드와 악의적인 전체 시트 범위(`A1:XFD1048576`) 확장에 따른 메모리 폭증을 원천 차단하기 위함입니다.
- **Trade-offs**: 단일 수식에서 10만 셀을 초과하는 범위 참조는 명시적으로 분할 집계하거나 피벗 엔진을 사용해야 합니다.
- **Alternatives rejected**:
  - 문자열 키(`"A1"`)만을 사용하는 범위 정렬 및 무제한 배열 확장: 대량 셀 연산 시 힙 할당과 OOM 위험이 커 기각.
- **Verified by**: `Tests/EasyOfficeTests/EditorEnginesParityTests.swift`
- **Revisit when**: 백그라운드 멀티스레드 청크 배열 수식(Dynamic Array Spill) 엔진을 확장할 때.

---

### 9. 양방향 포맷 일괄 변환 스튜디오, 오프라인 로컬 스마트 문서 보조 도구 및 메타데이터 비식별화 감사 정책

- **Status**: Implemented (`Sources/EasyOfficeCore/SmartProductivityTools.swift`)
- **Decision**:
  1. `DocumentConversionOrchestrator`는 모든 오피스 도메인(`.document`, `.sheet`, `.slide`, `.pdf`)과 20종 포맷(`.docx`, `.odt`, `.hwpx`, `.hwtx`, `.hwp`, `.hwt`, `.doc`, `.rtf`, `.md`, `.txt`, `.xlsx`, `.xls`, `.ods`, `.csv`, `.tsv`, `.pptx`, `.ppsx`, `.odp`, `.ppt`, `.pdf`) 간 단건·배치 변환을 수행하며, 변환된 모든 출력 바이트에 대해 반환 전 `FormatDetector.verify(data:fileName:)` 매직 시그니처 검증을 강제합니다.
  2. `LocalSmartDocumentAssistant`는 외부 클라우드 LLM이나 네트워크 API를 전혀 호출하지 않는 100% 오프라인 결정론적 알고리즘(단어 빈도·위치 가중치 문장 중요도 스코어링 요약, 문장 길이·피동 표현·중복 수식어 교정 진단, 기한·예산·수치·조치 항목 추출, 시트 Z-Score 이상치(`|z| >= 1.8`) 및 추세 분석, 최대 2,048 문단 상한 LCS 동적 계획법 문단 비교)으로 동작합니다.
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

