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
