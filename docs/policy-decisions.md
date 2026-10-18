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
8. [64비트 비트패킹 셀 좌표 및 92종 결정론적 수식·피벗·표 계산 엔진 정책](#8-64비트-비트패킹-셀-좌표-및-92종-결정론적-수식피벗표-계산-엔진-정책)
9. [양방향 포맷 일괄 변환 스튜디오, 오프라인 로컬 스마트 문서 보조 도구 및 메타데이터 비식별화 감사 정책](#9-양방향-포맷-일괄-변환-스튜디오-오프라인-로컬-스마트-문서-보조-도구-및-메타데이터-비식별화-감사-정책)
10. [스플릿 뷰(Split-View) 멀티 탭 워크스페이스, 컨텍스트 리본 명령 카탈로그 및 WCAG AA 접근성 정책](#10-스플릿-뷰split-view-멀티-탭-워크스페이스-컨텍스트-리본-명령-카탈로그-및-wcag-aa-접근성-정책)
11. [전 포맷 라운드트립 패리티·비트 수준 성능 벤치마크 게이트 및 v1.0.0 정식 릴리스 정책 (2026-10-17)](#11-전-포맷-라운드트립-패리티비트-수준-성능-벤치마크-게이트-및-v100-정식-릴리스-정책-2026-10-17)
12. [Slicing-by-8 CRC-32, 하드웨어 비정렬 워드 적재, 무할당 UTF-8 파서, 표준 컨테이너 호환성 및 AST 캐시·메모이제이션 시트 엔진 정책 (2026-10-18)](#12-slicing-by-8-crc-32-하드웨어-비정렬-워드-적재-무할당-utf-8-파서-표준-컨테이너-호환성-및-ast-캐시메모이제이션-시트-엔진-정책-2026-10-18)
13. [ISO 32000-1 바이트 정밀 xref/PDFKit 통합, 140종 컨텍스트 리본 전수 라우팅, Undo/Redo 스택 및 네이티브 파일 I/O 정책 (2026-10-18)](#13-iso-32000-1-바이트-정밀-xrefpdfkit-통합-140종-컨텍스트-리본-전수-라우팅-undoredo-스택-및-네이티브-파일-io-정책-2026-10-18)
14. [다중 시트 워크북, 셀 서식(CellStyle), 런/이미지 직렬화, 외부 표준 .ods/BIFF8 .xls 위치 기반 파싱 및 무할당 핫패스 정책 (2026-10-18)](#14-다중-시트-워크북-셀-서식cellstyle-런이미지-직렬화-외부-표준-odsbiff8-xls-위치-기반-파싱-및-무할당zero-arraystring-핫패스-정책-2026-10-18)
15. [Zero-Slop 도메인 리본 라우팅, 인라인 편집 Undo Coalescing, 다중 페이지 가시 PDF 렌더링 및 인터랙티브 UI 모듈 분리 정책 (2026-10-18)](#15-zero-slop-도메인-리본-라우팅-인라인-편집-undo-coalescing-다중-페이지-가시-pdf-렌더링-및-인터랙티브-ui-모듈-분리-정책-2026-10-18)
16. [비파괴적 리본 도메인 변이(Non-Destructive Ribbon Mutations), 5대 모듈 양방향 우측 인스펙터, 60fps 실시간 슬라이드 드래그·1Hz 발표자 타이머 및 SRP 기반 도메인 모듈 분할 정책 (2026-10-18)](#16-비파괴적-리본-도메인-변이non-destructive-ribbon-mutations-5대-모듈-양방향-우측-인스펙터-60fps-실시간-슬라이드-드래그1hz-발표자-타이머-및-srp-기반-도메인-모듈-분할-정책-2026-10-18)
17. [PD-026: AppKit NSTextView UTF-16 부분 선택 런 분할 및 DTP 2단·수식·텍스트 어울림·워터마크 조판 정책 (2026-10-18)](#17-pd-026-appkit-nstextview-utf-16-부분-선택-런-분할-및-dtp-2단수식텍스트-어울림워터마크-조판-정책-2026-10-18)
18. [PD-027: 스프레드시트 다중 셀 범위 드래그 선택, $ 절대·상대 참조 이동 자동 채우기(Auto-Fill) 엔진 및 가변 행·열 리사이즈 정책 (2026-10-18)](#18-pd-027-스프레드시트-다중-셀-범위-드래그-선택--절대상대-참조-이동-자동-채우기auto-fill-엔진-및-가변-행열-리사이즈-정책-2026-10-18)
19. [PD-028: 슬라이드 8방향 리사이즈 핸들, 360도 회전 각도 정규화 및 전체화면 발표 스테이지(레이저 포인터·노트 HUD) 정책 (2026-10-18)](#19-pd-028-슬라이드-8방향-리사이즈-핸들-360도-회전-각도-정규화-및-전체화면-발표-스테이지레이저-포인터노트-hud-정책-2026-10-18)
20. [PD-029: PDF 스튜디오 ISO 32000-1 벡터 잉크 획(m / l / S) 직렬화 및 자유곡선 서명·하이라이트 오버레이 정책 (2026-10-18)](#20-pd-029-pdf-스튜디오-iso-32000-1-벡터-잉크-획m--l--s-직렬화-및-자유곡선-서명하이라이트-오버레이-정책-2026-10-18)
21. [PD-030 ~ PD-034: 인라인 형광펜/링크·표 구조 편집·이름 정의/목표값 찾기/92종 함수·PDF 비가역 리댁션 및 1D CRC32·따옴표 인식 XML 파서 정책 (2026-10-18)](#21-pd-030--pd-034-인라인-형광펜링크표-구조-편집이름-정의목표값-찾기92종-함수pdf-비가역-리댁션-및-1d-crc32따옴표-인식-xml-파서-정책-2026-10-18)
22. [PD-032 & PD-035: 7종 벡터 차트·7종 슬라이드 도형/정렬 뷰·NSTextView 문단 스타일/인라인 속성 및 PDF 리댁션/AcroForm/탭 병합 UI 정책 (2026-10-18)](#22-pd-032--pd-035-7종-벡터-차트7종-슬라이드-도형정렬-뷰nstextview-문단-스타일인라인-속성-및-pdf-리댁션acroform탭-병합-ui-정책-2026-10-18)
23. [PD-037 ~ PD-041: 시맨틱 다크모드 배지 명도 대비, 200자 원고지 격자·문단/표/메일머지 제어, 시트 데이터 막대·첫 열 고정·데이터 정제, 슬라이드 마스터 테마·개체 정렬/배분·단조 Z-Order 및 PDF 결재 도장·계층형 책갈피 정책 (2026-10-18)](#23-pd-037--pd-041-시맨틱-다크모드-배지-명도-대비-200자-원고지-격자문단표메일머지-제어-시트-데이터-막대첫-열-고정데이터-정제-슬라이드-마스터-테마개체-정렬배분단조-z-order-및-pdf-결재-도장계층형-책갈피-정책-2026-10-18)
24. [PD-040 ~ PD-043: 6차 적대적 감사 기반 다중 그룹 리본·문서/슬라이드 수평 눈금자·무간격 1px 세선 시트 그리드·122종 수식 함수·자간/장평 타이포그래피·16:9 슬라이드/홈 템플릿 미니어처 및 PDF 보안 점검·워터마크 정책 (2026-10-19)](#24-pd-040--pd-043-6차-적대적-감사-기반-다중-그룹-리본문서슬라이드-수평-눈금자무간격-1px-세선-시트-그리드122종-수식-함수자간장평-타이포그래피169-슬라이드홈-템플릿-미니어처-및-pdf-보안-점검워터마크-정책-2026-10-19)

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

### 8. 64비트 비트패킹 셀 좌표 및 92종 결정론적 수식·피벗·표 계산 엔진 정책

- **Status**: Implemented (`Sources/EasyOfficeCore/SheetEngine.swift`, `Sources/EasyOfficeCore/SheetBuiltInFunctions.swift`, `Sources/EasyOfficeCore/SheetLookupTextAndFinancialFunctions.swift`, `Sources/EasyOfficeCore/DocumentEngine.swift`)
- **Decision**:
  1. 스프레드시트 셀 좌표는 `PackedCellCoordinate`(`UInt64`: 상위 32비트 `row`, 하위 32비트 `column`)로 비트패킹하여 문자열 해싱 없이 $O(1)$ 조회 및 행 우선(row-major) 비교를 수행하며, `CellAddress`는 ECMA-376 규격 상한(`maxZeroBasedColumn = 16,383`, `maxZeroBasedRow = 1,048,575`)과 오버플로 검사를 강제합니다.
  2. `CellRange`의 단일 수식 범위 확장은 메모리 고갈 방지를 위해 `maxExpandedCellCount = 100,000` 셀을 상한으로 제한하고 초과 시 `EasyOfficeError.invalidCellRange`를 반환합니다.
  3. `WorksheetModel`의 92종 내장 스프레드시트 함수(`SUMIFS`, `COUNTIFS`, `AVERAGEIFS`, `MINIFS`, `MAXIFS`, `IFNA`, `TEXTJOIN`, `SEARCH`, `REPLACE`, `ISBLANK`, `ISERROR`, `ISNUMBER`, `ISTEXT`, `ROWS`, `COLUMNS` 포함) 및 피벗 테이블 집계 엔진(`PivotTableEngine`)은 `PackedCellCoordinate` 방문 집합(`visited: Set<PackedCellCoordinate>`)으로 순환 참조 및 재귀 깊이 상한 초과 감지 시 즉시 `#REF!`를 반환하며, 문서/HWP 인라인 표 계산식(`DocumentTableBlock.evaluateTableFormula`)은 방향성(`ABOVE`/`LEFT`) 수치 리터럴만 비재귀적으로 집계하여 `Double?`을 반환합니다.
- **Why**: 대용량 시트 및 HWP/문서 표 계산 시 문자열 키 할당 오버헤드와 악의적인 전체 시트 범위(`A1:XFD1048576`) 확장에 따른 메모리 폭증을 원천 차단하기 위함입니다.
- **Trade-offs**: 단일 수식에서 10만 셀을 초과하는 범위 참조는 명시적으로 분할 집계하거나 피벗 엔진을 사용해야 합니다.
- **Alternatives rejected**:
  - 문자열 키(`"A1"`)만을 사용하는 범위 정렬 및 무제한 배열 확장: 대량 셀 연산 시 힙 할당과 OOM 위험이 커 기각.
- **Verified by**: `Tests/EasyOfficeTests/EditorEnginesParityTests.swift`, `Tests/EasyOfficeTests/EnterpriseParityAndOptimizationTests.swift`
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
  2. 비트 수준 코덱(`CRC32Calculator` 1차원 2,048-entry `flatSlicingTable`, `HWP5RecordHeader` 32비트 비트패킹 + `0xFFF` 확장 헤더, `BitPacking` 리틀 엔디언 정수 및 길이 접두 UTF-16LE 인코딩, 64비트 `PackedCellCoordinate`), 92종 내장 수식 밀집 그리드 평가 및 `PivotTableEngine` 집계, `DocumentConversionOrchestrator` 배치 변환, `LocalSmartDocumentAssistant` 오프라인 문서 요약·문장 교정·LCS 비교, `DocumentSecurityAuditor` 컨테이너 보안 감사·메타데이터 비식별화가 모두 서브초(sub-second) 예산 내에 결정론적으로 완료됨을 검증 게이트로 강제합니다.
  3. `v1.0.0` 정식 릴리스는 외부 서버·클라우드 API·원격 LLM·텔레메트리 호출이 전혀 없는 **100% 독립 실행형(Standalone)·서버리스(Serverless)·오프라인 지원 macOS 네이티브 아키텍처**를 보장하며, `scripts/package-macos-release.sh`를 통해 `EasyOffice.app`, `.dmg`, `.zip`, `SHA256SUMS.txt`를 결정론적으로 생성·검증합니다.
- **Why**: 상용 올인원 오피스 수준의 20종 포맷 호환성과 비트 수준 처리 성능, 오프라인 프라이버시 무결성을 추상적 선언이 아닌 실행 가능한 자동화 테스트(`CommercialParityBenchmarkTests`)와 패키징 스크립트로 영구 고정하기 위함입니다.
- **Trade-offs**: 모든 PR 및 릴리스 빌드마다 20종 포맷 라운드트립과 비트 수준 처리량 벤치마크 스위트가 함께 실행됩니다.
- **Alternatives rejected**:
  - 일부 대표 포맷(`.docx`, `.xlsx`, `.pdf`)만 샘플링 테스트하고 레거시/서식 포맷(`.hwt`, `.hwtx`, `.doc`, `.xls`, `.ppt`, `.ppsx`, `.odp`) 검증 생략: 실제 공문서 및 서식 배포 시 컨테이너 시그니처 회귀를 놓칠 수 있어 20종 전수 검증을 채택.
- **Verified by**: `Tests/EasyOfficeTests/CommercialParityBenchmarkTests.swift`, `node scripts/check-repo-hygiene.mjs`, `bash scripts/package-macos-release.sh 1.0.0`
- **Revisit when**: 차기 메이저/마이너 릴리스에서 신규 문서 표준 포맷이나 대용량 스트리밍 병렬 코덱을 추가할 때.

---

### 12. Slicing-by-8 CRC-32, 하드웨어 비정렬 워드 적재, 무할당 UTF-8 파서, 표준 컨테이너 호환성 및 AST 캐시·메모이제이션 시트 엔진 정책 (2026-10-18)

- **Status**: Implemented (`Sources/EasyOfficeCore/BinaryContainerCodec.swift`, `Sources/EasyOfficeCore/XMLSanitizer.swift`, `Sources/EasyOfficeCore/StandardZipOfficeCodecs.swift`, `Sources/EasyOfficeCore/ODFAndHWPXCodecs.swift`, `Sources/EasyOfficeCore/OLE2AndHWP5Codec.swift`, `Sources/EasyOfficeCore/LegacyOLE2OfficeCodec.swift`, `Sources/EasyOfficeCore/TabularAndMarkupCodec.swift`, `Sources/EasyOfficeCore/SheetEngine.swift`, `Sources/EasyOfficeCore/SheetAnalyticsAndPivot.swift`)
- **Decision**:
  1. **비트 수준 메모리·바이트 처리량 최적화**:
     - `BitPacking`은 바이트 단위 시프트 루프 대신 `UnsafeRawBufferPointer.loadUnaligned(fromByteOffset:as:)` 및 `storeBytes(of:toByteOffset:as:)` 단일 하드웨어 리틀 엔디언 워드 적재/저장을 사용하며, `CRC32Calculator`는 `8 × 256` 1차원 평탄화 `flatSlicingTable` 기반 **Slicing-by-8** 알고리즘으로 루프당 8바이트씩 처리합니다.
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

- **Status**: Implemented (`Sources/EasyOfficeCore/SheetEngine.swift`, `Sources/EasyOfficeCore/SheetBuiltInFunctions.swift`, `Sources/EasyOfficeCore/SheetLookupTextAndFinancialFunctions.swift`, `Sources/EasyOfficeCore/DocumentEngine.swift`, `Sources/EasyOfficeCore/SlideEngine.swift`, `Sources/EasyOfficeCore/StandardZipOfficeCodecs.swift`, `Sources/EasyOfficeCore/ODFAndHWPXCodecs.swift`, `Sources/EasyOfficeCore/OLE2AndHWP5Codec.swift`, `Sources/EasyOfficeCore/LegacyOLE2OfficeCodec.swift`, `Sources/EasyOfficeCore/LocalSmartAssistant.swift`)
- **Decision**:
  1. **다중 시트 워크북(`WorksheetSheetPage`), 일급 셀 서식(`CellStyle`) 및 단일 컨텍스트 그리드 일괄 평가(`evaluateGrid`)**:
     - `WorksheetModel`에 다중 시트 페이지 컬렉션(`additionalSheets`, `activeSheetIndex`, `addSheetPage`, `switchActiveSheet`), 좌표별 셀 서식(`cellStyles: [PackedCellCoordinate: CellStyle]`, `CellNumberFormat`, `CellHorizontalAlignment`), `pageSetup: WorksheetPageSetup`, `sheetComments: [PackedCellCoordinate: String]`, `trackChangesEnabled: Bool`을 도입하여 임의 셀(`E1~F6`) 오염 없이 도메인 상태를 정식 보존합니다.
     - `evaluateGrid(rows:columns:)`는 단일 `EvaluationContext`를 공유하여 그리드 전체 셀을 $O(V+E)$에 일괄 평가하며, `detectedDataRange()` 및 `detectedNumericRange()`가 실제 점유된 셀 경계를 동적으로 계산합니다.
  2. **문단 내 부분 런(`DocumentTextRun`)·인라인 이미지(`EmbeddedImageAttachment`) 및 외부 표준 `.ods`/`.xlsx`/`.pptx`/`.xls` 상호운용성**:
     - `ParagraphBlock.applyRunFormatting`으로 선택 영역 부분 런 서식을 분할·적용하고, `.docx`(`<w:r>`, `word/media/`), `.odt`(`<text:span>`, `Pictures/`), `.hwpx`(`<hp:run>`, `BinData/`)에 런과 이미지를 양방향 직렬화합니다.
     - `ODFContainerCodec.decodeWorksheet`는 비표준 `address`/`raw` 속성이 없는 외부 표준 `.ods` 파일에 대해 `<table:table-row>` 및 `<table:table-cell>` 행·열 위치(`table:number-rows-repeated`/`table:number-columns-repeated` 최대 1,024 클램프, `table:formula="of:=..."` 정규화)로 모든 셀을 복원하며, `LegacyOLE2OfficeCodec.decodeBIFF8Worksheet`는 `SST`(`0x00FC`)+`LABELSST`(`0x00FD`), `RK`(`0x027E`), `FORMULA`(`0x0006`) 레코드를 디코딩합니다.
  3. **`Array(String)` (`[Character]`) 힙 할당 제거 및 코어 파일 모듈 분리**:
     - `SheetEngine.swift`에서 내장 함수를 `SheetBuiltInFunctions.swift` 및 `SheetLookupTextAndFinancialFunctions.swift`로, `SmartProductivityTools.swift`에서 `LocalSmartDocumentAssistant`를 `LocalSmartAssistant.swift`로 분리하고, `FormulaASTCompiler`, `MID`, `XMLSanitizer.isSelfClosingTag`, `CommonMarkCodec.parsePipeRow`, `splitLongSentenceAtWordBoundary`의 `Array(String)` 할당을 UTF-8 바이트 및 `String.Index` 슬라이싱으로 대체했습니다.
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

---

### 16. 비파괴적 리본 도메인 변이(Non-Destructive Ribbon Mutations), 5대 모듈 양방향 우측 인스펙터, 60fps 실시간 슬라이드 드래그·1Hz 발표자 타이머 및 SRP 기반 도메인 모듈 분할 정책 (2026-10-18)

- **Status**: Implemented (`Sources/EasyOfficeCore/WorkspaceCatalogAndModels.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeCore/WorkspaceRibbonCommandRouter.swift`, `Sources/EasyOfficeApp/RightContextInspectorView.swift`, `Sources/EasyOfficeApp/WorkspaceViews.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`, `Sources/EasyOfficeApp/SlideAndPDFDetailPanes.swift`, `Sources/EasyOfficeApp/HomeHubAndSmartViews.swift`)
- **Decision**:
  1. **사이드채널 문자열 오염 0건 및 비파괴적 도메인 상태 변이 (`WorkspaceRibbonCommandRouter.swift`)**:
     - 문서(`.document`, `.hwp`)의 메일 머지(`doc-mod-mailmerge`), 원고지 매수 검산(`hwp-mod-manuscript`), 수식·데이터 명령(`-data-autosum`, `-data-fn-browser`, `-data-sort`, `-data-validation`)이 `doc.footerText`를 덮어쓰던 부작용을 제거하고, 표 요약 행 추가(`=SUM(ABOVE)`, `=AVERAGE(ABOVE)`) 및 `doc.recordRevision(...)` 이력 기록으로 비파괴적 변이를 수행합니다.
     - `doc-insert-chart`/`hwp-insert-chart`는 브래킷 문자열 문단 대신 정식 `DocumentTableBlock` 차트 요약 표를 삽입하고, `doc-insert-shape`/`hwp-insert-shape`는 `.quote` 스타일 콜아웃 문단을 삽입합니다.
     - 시트(`.sheet`)의 `sheet-mod-pivot`은 선택 셀 코멘트 오염 대신 전용 워크시트 탭(`"피벗 요약"`, `sheet.addSheetPage`)을 생성·갱신하며, `sheet-review-security`/`-review-verify`에서 사용자 셀 코멘트를 일괄 삭제하던 `sheet.sheetComments.removeAll()`을 제거합니다.
     - 슬라이드(`.slide`)의 `-review-track`, `-review-comment`, `-data-*`는 `speakerNotes`에 태그 문자열을 덧붙이지 않고 `SlideCanvasElement`(`.calloutShape`, `.chartEmbed`, `.tableGrid`)를 정식 변이합니다.
     - PDF(`.pdf`)의 `pdf-mod-splitmerge`는 고정 1페이지가 아닌 현재 선택된 `selectedPDFPageNumber` 페이지를 추출하며, `-home-*` 및 `-layout-*` 명령이 가짜 주석이나 책갈피 노드를 추가하지 않도록 정리합니다.
  2. **5대 모듈 양방향 인터랙티브 우측 컨텍스트 인스펙터 (`RightContextInspectorView.swift`)**:
     - 문서/HWP(용지 규격·방향 `Picker`, 다단 `Stepper`, 변경 추적 `Toggle`, 머리말/꼬리말 `TextField`), 시트(숫자 표시 형식·가로 정렬 `Picker`, 셀 메모 `TextField`, 셀 병합 토글, 머리글 행 고정 `Toggle`), 슬라이드(화면 비율·마스터 레이아웃·전환 효과 `Picker`, 발표자 노트 `TextField`, 선택 개체 제목/본문·X/Y/W/H·Z-Order 제어), PDF(대상 페이지 `Stepper`, 빈 페이지 추가·선택 페이지 삭제·분할 추출)를 양방향 바인딩으로 제어합니다.
  3. **부분 구절(`inlineRunTargetPhrase`) 런 분할, 실제 `160×48` 멀티픽셀 PNG 비트맵 렌더링, 무제한 시트 그리드 + 1행 고정(`isHeaderRowFrozen`), 60fps 실시간 슬라이드 드래그(`@GestureState`) 및 1Hz 발표자 타이머**:
     - `applyInlineRunFormattingToActiveParagraph`는 `inlineRunTargetPhrase` 부분 문자열 범위를 계산하여 `[앞 구간, 강조 구절, 뒤 구간]` 다중 `DocumentTextRun`으로 정밀 분할하며, `insertSampleImageAttachment`는 `NSBitmapImageRep`로 실제 `160×48` 라벤더 배너 PNG를 생성하고 `DocumentEditorDetailPaneView`에서 `NSImage(data:)` 비트맵으로 렌더링합니다(`presentOpenImagePanel` 디스크 이미지 불러오기 포함).
     - `SheetEditorDetailPaneView`는 기존의 `24행 × 12열` 하드 캡을 제거하여 임의 좌표 셀을 모두 표시하고, `isHeaderRowFrozen` 활성 시 1행을 수직 스크롤 영역 상단에 고정합니다.
     - `SlideEditorDetailPaneView`는 `@GestureState`로 드래그 중인 개체의 실시간 위치 및 스냅 가이드를 60fps로 반영하고, 발표자 모니터 활성 시 `Timer.publish(every: 1.0, on: .main, in: .common).autoconnect()`로 `tickPresenterTimer(seconds: 1)`를 구동합니다(`advancePresenterSlide()`의 인위적 `+15초` 점프 제거).
  4. **단일 책임 원칙(SRP) 기반 도메인 모듈 분할 및 전수 `xcrun swift-format` 적용 (인위적 줄 수 상한 및 세미콜론 압축 금지)**:
     - 인위적인 파일 줄 수 제한(`< 1,000 LOC`)으로 인해 한 줄에 세미콜론(`;`)으로 다중 구문을 압축하는 안티패턴을 전면 금지하고, 도메인 책임 단위로 파일을 분리(`XMLSanitizer.swift`, `DocumentSubmodelsAndNotes.swift`, `SheetLookupTextAndFinancialFunctions.swift`, `WorkspaceCatalogAndModels.swift`, `RightContextInspectorView.swift`)한 뒤 `xcrun swift-format format --in-place`로 가독성을 보장합니다.
- **Why**: 리본 명령 실행 시 문서 꼬리말·발표자 노트·셀 코멘트가 오염되거나 삭제되는 결함을 근절하고, 우측 인스펙터와 에디터 캔버스의 실시간 조작성 및 코드 가독성을 데스크톱 네이티브 수준으로 끌어올리기 위함입니다.
- **Trade-offs**: 피벗 요약 실행 시 워크북에 `"피벗 요약"` 시트 탭이 추가되나, 원본 시트의 셀이나 코멘트를 훼손하지 않고 구조화된 표로 집계 결과를 보존합니다.
- **Alternatives rejected**:
  - 리본 명령 실행 여부를 증명하기 위해 `footerText`나 `speakerNotes`에 임의 텍스트를 덧붙이는 방식: 사용자의 실제 문서 바닥글과 발표자 노트를 훼손하므로 기각.
  - 인위적인 줄 수 상한을 맞추기 위해 세미콜론(`;`)으로 여러 구문을 한 줄에 나열하는 방식: 클린 코드와 가독성을 훼손하므로 기각.
- **Verified by**: `Tests/EasyOfficeTests/WorkspaceUIParityTests.swift` (`testThirdAuditZeroSlopInspectorLiveCanvasAndNonDestructiveCommands`)
- **Revisit when**: 다중 셀 드래그 마키(Marquee) 선택 오버레이를 시트 캔버스에 추가할 때.

---

### 17. PD-026: AppKit NSTextView UTF-16 부분 선택 런 분할 및 DTP 2단·수식·텍스트 어울림·워터마크 조판 정책 (2026-10-18)

- **Status**: Implemented (`Sources/EasyOfficeCore/DocumentEngine.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`)
- **Decision**:
  1. `SelectableParagraphTextView`(`NSViewRepresentable`)는 선택된 문단에 대해 AppKit `NSTextView` 및 `NSTextViewDelegate.textViewDidChangeSelection(_:)`을 연결하여 커서 드래그 선택 영역(`selectedRange()`)을 UTF-16 오프셋 범위(`selectedParagraphUTF16Range: Range<Int>?`)로 실시간 수집합니다.
  2. `ParagraphBlock.applyRunFormatting(utf16Range:...)`은 UTF-16 오프셋을 `String.Index(utf16Offset:in:)` 및 `rangeOfComposedCharacterSequences(for:)`로 정렬하여 한글 자모·복합 문자 경계를 안전하게 보존하면서 `DocumentTextRun`을 분할·병합하며, 리본 홈 서식(`-home-bold`/`-home-italic`/`-home-underline`) 및 인라인 런 강조 명령이 선택된 UTF-16 부분 범위를 최우선으로 반영합니다.
  3. `DocumentEditorDetailPaneView`는 `section.columnCount >= 2`일 때 문단 컬렉션을 좌·우 2단(`HStack` + 수직 `Divider` 컬럼 구분선) DTP 레이아웃으로 배치하고, `DocumentEquationBlock`(`formatMathSymbols` 유니코드 수학 기호 치환), `EmbeddedImageAttachment.wrapMode`(`.inline`, `.squareLeft`, `.squareRight`, `.topAndBottom` 텍스트 어울림 순환), 대각선(`-32°`) `watermarkText` 오버레이를 렌더링합니다.
- **Why**: 문단 전체 또는 수동 구절 검색에만 의존하던 한계를 넘어 마우스 커서 드래그로 선택한 정확한 부분 구간에 서식을 부여하고, 학술·공문서 조판에 필수적인 2단 컬럼·수식·이미지 어울림·워터마크를 지원하기 위함입니다.
- **Trade-offs**: 활성 문단에만 `NSTextView`를 마운트하고 비활성 문단은 경량 SwiftUI 뷰로 유지하여 수백 문단 문서에서도 스크롤 성능을 보존합니다.
- **Alternatives rejected**:
  - 모든 문단에 개별 `NSTextView` 인스턴스를 상시 생성하는 방식: AppKit 뷰 계층과 `NSTextStorage` 레이아웃 오버헤드가 문단 수에 비례해 증가하므로 선택 문단 집중 마운트 방식을 채택.
- **Verified by**: `Tests/EasyOfficeTests/DeepDesktopInteractionsTests.swift` (`testUTF16SubSelectionRunSplittingAndRibbonBoldFormatting`, `testDocumentEquationsImageWrapModeAndWatermark`)
- **Revisit when**: 인라인 수식 베이스라인 정렬 및 MathML XML 양방향 직렬화를 확장할 때.

---

### 18. PD-027: 스프레드시트 다중 셀 범위 드래그 선택, $ 절대·상대 참조 이동 자동 채우기(Auto-Fill) 엔진 및 가변 행·열 리사이즈 정책 (2026-10-18)

- **Status**: Implemented (`Sources/EasyOfficeCore/SheetAutoFillAndLayout.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeApp/SheetEditorDetailPaneView.swift`)
- **Decision**:
  1. `WorkspaceViewModel`은 단일 활성 셀(`selectedSheetCell`)과 함께 다중 셀 선택 범위(`selectedSheetRange`, `selectSheetCellOrRange`, `extendSheetSelection`)를 유지하며, 리본 서식 명령(`sheet-home-bold`/`-italic`/`-underline`)은 `updateCellStyleInRange`를 통해 선택된 사각형 범위 전체에 일괄 적용됩니다.
  2. `WorksheetModel.autofill(sourceRange:targetRange:)` 및 `shiftFormulaReferences(_:rowDelta:columnDelta:)`는 수식 내 `$` 절대 행·열 고정(`$A$1`, `$A1`, `A$1`)과 상대 참조(`A1`)를 구분하여 오프셋만큼 이동시키고, 2개 이상의 선형 등차수열(`100, 250 -> 400, 550`) 및 후행 정수 라벨(`항목 1 -> 항목 2`)을 자동 외삽합니다.
  3. `SheetEditorDetailPaneView`는 열 머리글 우측 `4pt` 드래그 핸들(`columnWidths`, `48...420pt`), 행 머리글 하단 `4pt` 드래그 핸들(`rowHeights`, `22...180pt`), 선택 범위 우하단 코너(`◢`) 자동 채우기 드래그 핸들, 열 값 자동 필터(`filterMatchingRowIndices`), 하단 실시간 범위 집계 바(`summarizeRange`: 합계·평균·비어있지 않은 셀 수·수치 셀 수·최소·최대)를 제공합니다.
- **Why**: 데스크톱 스프레드시트의 핵심 작업 흐름인 다중 셀 블록 서식 지정, 수식/수열 드래그 자동 채우기, 열 너비·행 높이 조절, 즉시 요약 통계 확인을 완결하기 위함입니다.
- **Trade-offs**: 자동 채우기 대상 범위는 메모리 고갈 방지를 위해 `CellRange.maxExpandedCellCount (100,000)` 상한 내에서만 실행되며, 비연속·역방향 범위는 `EasyOfficeError.invalidCellRange`로 차단(Fail-Closed)됩니다.
- **Alternatives rejected**:
  - 정규식 치환으로 수식 내 셀 주소를 이동하는 방식: 문자열 리터럴(`"A1"`)이나 함수명(`LOG10`)을 오탐할 위험이 있어 UTF-8 바이트 경계 파서(`shiftFormulaReferences`)를 채택.
- **Verified by**: `Tests/EasyOfficeTests/DeepDesktopInteractionsTests.swift` (`testSheetRangeSelectionAutoFillAndColumnRowResize`)
- **Revisit when**: 다중 열 복합 조건 필터 및 대각선 양방향 자동 채우기를 추가할 때.

---

### 19. PD-028: 슬라이드 8방향 리사이즈 핸들, 360도 회전 각도 정규화 및 전체화면 발표 스테이지(레이저 포인터·노트 HUD) 정책 (2026-10-18)

- **Status**: Implemented (`Sources/EasyOfficeCore/SlideEngine.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeApp/SlideAndPDFDetailPanes.swift`, `Sources/EasyOfficeApp/SlideshowAndPDFCanvasOverlays.swift`, `Sources/EasyOfficeApp/WorkspaceViews.swift`)
- **Decision**:
  1. `SlideModel.resizeElement(id:handle:deltaX:deltaY:stageSize:)`는 8방향 리사이즈 핸들(`SlideResizeHandle`: `NW`, `N`, `NE`, `E`, `SE`, `S`, `SW`, `W`) 드래그 시 반대편 고정 모서리(anchor edge)를 기준으로 최소 크기(`40×24 pt`) 및 스테이지 경계를 클램핑하며, `rotateElement(id:toDegrees:)`는 음수·초과 회전 각도를 `[0, 360)` 범위로 정규화합니다.
  2. `SlideEditorDetailPaneView`는 선택된 개체 둘레에 8개 방향 리사이즈 핸들과 상단 회전 핸들(`↻`), 등장 애니메이션 배지(`SlideElementAnimation`: `.none`, `.fadeIn`, `.flyInUp`, `.zoomIn`)를 렌더링합니다.
  3. `SlideshowFullScreenOverlayView`는 `isFullScreenSlideshowActive` 활성 시 메인 워크스페이스 최상단 `ZStack`을 덮는 다크 발표 스테이지를 표시하며, 개체 등장 애니메이션 전환, 마우스/드래그 추적 적색 발광 레이저 포인터(`isLaserPointerEnabled`), 발표자 노트 HUD 드로어, 경과 타이머, `ESC` 즉시 종료를 지원합니다.
- **Why**: 우측하단 단일 크기 조절만 가능하던 제약을 해소하여 8방향 자유 리사이즈와 각도 회전을 지원하고, 별도 외부 도구 없이 즉시 전체화면 발표와 레이저 포인터 강조를 수행할 수 있게 하기 위함입니다.
- **Trade-offs**: 좌측/상단 핸들(`NW`, `N`, `W`, `SW`, `NE`) 드래그 시 개체가 최소 너비(`40pt`)·높이(`24pt`) 이하로 줄어들면 반대편 모서리가 밀리지 않도록 이동량(`clampedLeftDX`, `clampedTopDY`)을 고정 클램핑합니다.
- **Alternatives rejected**:
  - 좌측/상단 핸들 드래그 시 최소 크기 도달 후에도 `x`/`y` 좌표를 계속 이동시키는 방식: 반대편 고정 모서리가 함께 밀려나는 비정상 이동이 발생하므로 기각.
- **Verified by**: `Tests/EasyOfficeTests/DeepDesktopInteractionsTests.swift` (`testSlideResizeHandlesRotationAnimationAndFullScreenSlideshow`)
- **Revisit when**: 듀얼 디스플레이 분리형 발표자 모니터 윈도우(`NSWindow`) 출력을 추가할 때.

---

### 20. PD-029: PDF 스튜디오 ISO 32000-1 벡터 잉크 획(m / l / S) 직렬화 및 자유곡선 서명·하이라이트 오버레이 정책 (2026-10-18)

- **Status**: Implemented (`Sources/EasyOfficeCore/PDFEngine.swift`, `Sources/EasyOfficeCore/PDFBinarySerializer.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeApp/SlideAndPDFDetailPanes.swift`, `Sources/EasyOfficeApp/SlideshowAndPDFCanvasOverlays.swift`)
- **Decision**:
  1. `PDFStudioDocumentModel`은 페이지별 자유곡선 벡터 잉크 획 컬렉션(`inkStrokes: [PDFInkStroke]`, `PDFInkPoint`)을 보유하며, 페이지 삭제·추출·병합·순서 변경 시 주석(`annotations`)과 동일하게 페이지 번호를 자동 재매핑합니다.
  2. `PDFBinarySerializer.encode(_:)`는 각 페이지의 `inkStrokes`(`points.count >= 2`)를 ISO 32000-1 그래픽 상태 블록(`q <R> <G> <B> RG <lineWidth> w 1 J 1 j <x0> <y0> m <x1> <y1> l ... S Q`)으로 페이지 콘텐츠 스트림에 직접 직렬화하고, 스튜디오 메타데이터 스트림을 통해 무손실 라운드트립 복원을 보장합니다.
  3. `PDFInteractiveInkOverlayView`는 PDF 캔버스 상에서 실시간 마우스 드래그(`DragGesture(minimumDistance: 1)`) 궤적을 `Canvas` 벡터 경로로 즉시 미리 보여주고 `.onEnded` 시점에 `addInkStrokeToActivePDFPage`로 커밋하며, 본문 구절 선택 하이라이트(`addHighlightToActivePDFPage`)를 함께 제공합니다.
- **Why**: 텍스트 스탬프 주석뿐 아니라 실제 마우스/트랙패드 필기 궤적을 표준 PDF 벡터 스트림 연산자(`m`/`l`/`S`)로 영구 보존하여 네이티브 `PDFKit` 및 외부 PDF 뷰어에서도 동일한 자필 서명·검토 획이 표시되도록 하기 위함입니다.
- **Trade-offs**: 페이지 경계를 벗어나는 극단적 좌표 입력을 방지하기 위해 직렬화 시 점 좌표를 미디어 박스 안전 영역(`x: 20...575`, `y: 20...820`)으로 클램핑합니다.
- **Alternatives rejected**:
  - 자유곡선 서명을 래스터 이미지(PNG)로 구워 넣는 방식: 확대 시 픽셀 깨짐이 발생하고 파일 용량이 불필요하게 증가하므로 ISO 32000-1 벡터 경로 연산자(`m`/`l`/`S`) 방식을 채택.
- **Verified by**: `Tests/EasyOfficeTests/DeepDesktopInteractionsTests.swift` (`testPDFInkStrokesHighlightAndBinaryRoundTrip`)
- **Revisit when**: 필압(pressure) 기반 가변 선 굵기 베지어 곡선(`c` 연산자) 스무딩을 추가할 때.

---

### 21. PD-030 ~ PD-034: 인라인 형광펜/링크·표 구조 편집·이름 정의/목표값 찾기/92종 함수·PDF 비가역 리댁션 및 1D CRC32·따옴표 인식 XML 파서 정책 (2026-10-18)

- **Status**: Implemented (`Sources/EasyOfficeCore/XMLSanitizer.swift`, `Sources/EasyOfficeCore/BinaryContainerCodec.swift`, `Sources/EasyOfficeCore/DocumentEngine.swift`, `Sources/EasyOfficeCore/DocumentSubmodelsAndNotes.swift`, `Sources/EasyOfficeCore/SheetEngine.swift`, `Sources/EasyOfficeCore/SheetAutoFillAndLayout.swift`, `Sources/EasyOfficeCore/SheetLookupTextAndFinancialFunctions.swift`, `Sources/EasyOfficeCore/PDFEngine.swift`, `Sources/EasyOfficeCore/PDFBinarySerializer.swift`)
- **Decision**:
  1. **PD-030 (문서 인라인 취소선·형광펜·하이퍼링크·문단 들여쓰기·표 행/열 편집 및 페이지 토큰)**:
     - `DocumentTextRun`에 `isStrikethrough`, `backgroundHex`, `linkURL`을 추가하고, `ParagraphBlock`에 `firstLineIndentPt`·`leftIndentPt`를 추가하여 `.docx`, `.odt`, `.hwpx`, `.hwp`, `.rtf`, `.md`, `.html` 전 포맷 라운드트립을 지원합니다.
     - `DocumentTableBlock`에 `insertRow(at:)`, `deleteRow(at:)`, `insertColumn(at:)`, `deleteColumn(at:)`를 구현하고 `cellSpans` 좌표를 자동 이동·클램핑하며, `OfficeDocumentModel.resolvePageTokens(in:pageNumber:totalPages:)`로 `{{PAGE}}` / `{{NUMPAGES}}` 동적 치환을 지원합니다.
  2. **PD-031 (스프레드시트 이름 정의·행/열 삽입 시 참조 자동 이동·목표값 찾기·92종 내장 함수)**:
     - `WorksheetModel.namedRanges` 및 미정의 식별자 `#NAME?` 판정을 지원하고, `insertRow`/`deleteRow`/`insertColumn`/`deleteColumn` 수행 시 셀 좌표·병합 범위·수식 참조·`namedRanges` 좌표를 함께 이동시킵니다.
     - 할선법(Secant Method) 기반 수치 해법 `goalSeek(targetCell:targetValue:variableCell:)`과 `sparklineNormalizedValues(in:)`를 제공하고, `SUMIFS`, `COUNTIFS`, `AVERAGEIFS`, `MINIFS`, `MAXIFS`, `IFNA`, `TEXTJOIN`, `SEARCH`, `REPLACE`, `ISBLANK`, `ISERROR`, `ISNUMBER`, `ISTEXT`, `ROWS`, `COLUMNS` 15종을 추가하여 총 92종 내장 함수를 지원합니다(`MATCH` 3번째 인자 `1`/`0`/`-1` 및 `TEXT` 천 단위 구분 기호 `"#,##0"` 포함).
  3. **PD-033 (PDF 비가역 리댁션 및 AcroForm 양식 필드 직렬화)**:
     - `PDFStudioDocumentModel.redactText(_:)`는 본문(`extractedText`), 문서 제목(`title`), 주석(`annotations`), 양식 필드(`formFields`), 목차(`outline`)에서 대상 민감 구문을 대소문자 무시로 `"████"`로 영구 치환하며, `redactedPhrases`에는 원문 평문 대신 비가역 마스킹 요약 토큰(`"████ (N자 마스킹)"`)만 기록하여 `%EasyOfficePDFModelBase64` 메타데이터 스트림에도 평문이 절대 남지 않도록 보장합니다.
     - `PDFBinarySerializer`는 리댁션된 페이지에 불투명 검정 사각형(`0 0 0 rg ... re f`)과 `/Redacted true` 카탈로그 플래그를, 양식 필드에 `[FORM] name: value` 스트림을 기록하고 역직렬화 시 정밀 복원합니다.
  4. **PD-034 (1차원 평탄화 `CRC32Calculator.flatSlicingTable`, `OLE2ContainerCodec.pack` 단일 버퍼 직렬화, 따옴표 인식 `XMLSanitizer`)**:
     - `CRC32Calculator.flatSlicingTable`을 2,048 원소 1차원 `[UInt32]` 연속 버퍼로 평탄화하고, `OLE2ContainerCodec.pack`에서 단일 `allStreamsData` 버퍼와 `fatSectors` 사전 예약으로 섹터 루프 할당을 제거하며, `XMLSanitizer.findTagClosingBracket`이 따옴표(`"..."`, `'...'`) 내부의 `>` 문자를 무시하도록 보호합니다.
- **Why**: 엔터프라이즈 문서 조판, 조건부 다중 집계(`SUMIFS`/`COUNTIFS`)·목표값 찾기·이름 정의, PDF 민감정보 비가역 삭제(CWE-200/CWE-212 방어) 및 저수준 파서 견고성을 완결하기 위함입니다.
- **Trade-offs**: `redactText(_:)` 수행 후에는 원본 민감 문자열이 메모리와 직렬화 스트림 어디에도 남지 않으므로 Undo 스택을 통하지 않고는 원문을 역산할 수 없습니다(보안 의도에 부합).
- **Alternatives rejected**:
  - `redactedPhrases`에 마스킹 대상 원문 평문을 그대로 보관하는 방식: Base64 메타데이터 스트림을 디코딩하면 삭제한 민감 정보가 노출되므로 비가역 마스킹 요약 토큰 방식으로 대체.
- **Verified by**: `Tests/EasyOfficeTests/EnterpriseParityAndOptimizationTests.swift`, `Tests/EasyOfficeTests/CommercialParityBenchmarkTests.swift`
- **Revisit when**: 정규식 패턴(주민등록번호·카드번호·이메일 자동 탐지) 일괄 리댁션 프리셋을 추가할 때.

---

### 22. PD-032 & PD-035: 7종 벡터 차트·7종 슬라이드 도형/정렬 뷰·NSTextView 문단 스타일/인라인 속성 및 PDF 리댁션/AcroForm/탭 병합 UI 정책 (2026-10-18)

- **Status**: Implemented (`Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeCore/WorkspaceHubEditorActions.swift`, `Sources/EasyOfficeCore/WorkspaceRibbonCommandRouter.swift`, `Sources/EasyOfficeApp/RightContextInspectorView.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`, `Sources/EasyOfficeApp/SheetEditorDetailPaneView.swift`, `Sources/EasyOfficeApp/SlideAndPDFDetailPanes.swift`, `Sources/EasyOfficeApp/SlideshowAndPDFCanvasOverlays.swift`)
- **Decision**:
  1. **PD-032 (7종 `SheetChartKind` 벡터 캔버스·스파크라인 및 7종 `SlideElementKind` 도형·슬라이드 정렬 뷰)**:
     - `SheetVectorChartCanvasView`는 `SheetChartKind` 7종(`.column`, `.stackedColumn`, `.bar`, `.line`, `.area`, `.pie`, `.scatter`) 각각에 대해 별도의 `Canvas` 기하학(막대, 누적 막대, 수평 막대, 꺾은선+점, 반투명 영역+상단 윤곽선, 원형 호 섹터, 산점도 마커)과 유한 실수 안전 포맷터(`safeFinite`/`formatMetric`)를 적용하고, 조건부 서식 배지 카드에 `sparklineNormalizedValues(for:using:)` 기반 미니 스파크라인 `Canvas`를 함께 표시합니다.
     - `SlideElementShapeContentView`는 편집 캔버스와 전체화면 슬라이드쇼 오버레이(`SlideshowFullScreenOverlayView`) 양쪽에서 `.ellipseShape`, `.arrowShape`, `.calloutShape`, `.tableGrid`, `.chartEmbed`, `.roundedRectangleShape`, `.rectangleShape`, `.textBox`를 각기 고유한 벡터 도형/격자/미니 차트로 렌더링하며, `isSlideSorterViewEnabled` 토글 시 반응형 `LazyVGrid` 슬라이드 정렬 뷰(추가·복제·순서 이동·삭제)로 전환합니다.
  2. **PD-035 (문서 NSTextView 문단 스타일/인라인 속성·표/변경추적/미주 UI 및 PDF 리댁션/AcroForm/탭 병합 바인딩)**:
     - `SelectableParagraphTextView.buildAttributedString(from:)`는 `NSMutableParagraphStyle`(`lineHeightMultiple`, `firstLineHeadIndent`, `headIndent`, `alignment`)과 런별 `.strikethroughStyle`, `.backgroundColor`, 안전 URI 스킴 화이트리스트(`https`, `http`, `mailto` — `WorkspaceViewModel.sanitizeSafeHyperlinkURL`)를 거친 `.link`를 적용하고, 비편집 문단 행(`paragraphRow`)에서도 `AttributedString`으로 동일한 인라인 서식을 표시합니다.
     - 문서 편집기에서 머리글/바닥글을 `resolvePageTokens(in:pageNumber:totalPages:)`로 치환해 표시하고, 표 카드(`documentTableView`)에서 비원점 병합 셀(`isCoveredByNonOriginSpan`) 생략·원점 병합 배지(`rowSpan×columnSpan`)·셀 테두리 스타일(`effectiveBorderStyle`: `.solid`, `.doubleLine`, `.dashed`, `.diagonalDown`, `.diagonalUp`, `.none`) 및 `+행`/`-행`/`+열`/`-열` 버튼을 제공하며, 변경 추적 카드에 개별 `"승인"`(`acceptDocumentRevision`)/`"반려"`(`rejectDocumentRevision`) 버튼을, 각주와 미주 카드를 분리 배치합니다.
     - 시트 편집기 및 인스펙터에서 다중 셀 병합/해제(`toggleMergeOnSelectedSheetCell`/`mergeSelectedSheetRange`), 행/열 삽입·삭제(`insertSheetRowAtSelection`/`deleteSheetRowAtSelection`/`insertSheetColumnAtSelection`/`deleteSheetColumnAtSelection`), 이름 정의(`defineNamedRangeOnActiveSheet`/`defineSheetNamedRange`), 목표값 찾기(`runGoalSeekOnActiveSheet`)를 직접 실행할 수 있도록 연결합니다.
     - PDF 스튜디오 툴바 및 인스펙터에 비가역 리댁션(`redactSensitiveTextOnActivePDF`/`redactTextOnActivePDF`), 페이지 위/아래 순서 이동(`moveActivePDFPage`/`reorderActivePDFPage`), 열린 타 PDF 탭 병합(`mergeActivePDFWithAnotherOpenTab`/`mergeActivePDFWithTab`), 대화형 AcroForm 양식 필드 추가·편집(`addFormFieldToActivePDF`, `updatePDFFormField`/`setActivePDFFormFieldValue`) UI를 바인딩합니다.
- **Why**: 코어 엔진(`PD-030`~`PD-034`)에 구현된 엔터프라이즈 문서·시트·슬라이드·PDF 도메인 모델과 네이티브 데스크톱 UI/인스펙터/리본/캔버스 사이의 표시·조작 간극을 완전히 해소하기 위함입니다.
- **Trade-offs**: 7종 벡터 차트와 7종 슬라이드 도형을 네이티브 SwiftUI `Canvas`/`Path`로 직접 그리므로 뷰 코드 분량이 증가하지만, 외부 차트 라이브러리에 의존하지 않고도 즉각적인 60fps 렌더링과 슬라이드쇼/편집기 간 100% 시각적 일관성을 확보합니다.
- **Alternatives rejected**:
  - 모든 차트 종류를 단일 세로 막대 뷰로만 표시하거나 슬라이드 도형을 모두 사각형 테두리로만 그리는 방식: 도메인 모델의 `kind` 속성이 화면에 반영되지 않아 시각적 피드백이 누락되므로 기각.
- **Verified by**: `Tests/EasyOfficeTests/DeepDesktopInteractionsTests.swift`
- **Revisit when**: 3D 원근 차트 또는 복합 이중 축(Dual-Axis Combo Chart) 요구사항이 추가될 때.

---

### 23. PD-037 ~ PD-041: 시맨틱 다크모드 배지 명도 대비, 200자 원고지 격자·문단/표/메일머지 제어, 시트 데이터 막대·첫 열 고정·데이터 정제, 슬라이드 마스터 테마·개체 정렬/배분·단조 Z-Order 및 PDF 결재 도장·계층형 책갈피 정책 (2026-10-18)

- **Status**: Implemented (`Sources/EasyOfficeDesignSystem/DesignTokens.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeCore/WorkspaceHubDocumentAndSlideActions.swift`, `Sources/EasyOfficeCore/WorkspaceHubSheetAndPDFActions.swift`, `Sources/EasyOfficeCore/WorkspaceRibbonCommandRouter.swift`, `Sources/EasyOfficeApp/RightContextInspectorView.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`, `Sources/EasyOfficeApp/SheetEditorDetailPaneView.swift`, `Sources/EasyOfficeApp/SlideAndPDFDetailPanes.swift`, `Sources/EasyOfficeApp/SlideshowAndPDFCanvasOverlays.swift`)
- **Decision**:
  1. **PD-037 (시맨틱 다크모드 배지 및 인스펙터 명도 대비 토큰화)**:
     - `WorkspaceThemePalette`의 `accentBadgeFill` / `accentBadgeText` 시맨틱 토큰을 우측 컨텍스트 인스펙터(`RightContextInspectorView.swift`) 및 상세 편집 패널 전반에 일괄 적용하고, 다크모드 배경 위에서 가독성을 저하시키던 고정 `brand50`·`brand100`·`brand700` 직접 참조를 제거하여 WCAG AA 이상의 명도 대비를 보장합니다.
  2. **PD-038 (문서/HWP 200자 원고지 20×10 격자 뷰, 문단 정렬·줄간격·다단계 목록, 표 셀 병합/분할/테두리, 메일 머지 치환 및 차트/도형 벡터 그래픽 첨부)**:
     - `WorkspaceViewModel.isManuscriptGridModeEnabled` 및 `hwp-mod-manuscript` 리본 명령을 통해 문서를 `20열 × 10행` 200자 원고지 격자 뷰로 실시간 전환할 수 있도록 지원합니다.
     - 우측 인스펙터와 `WorkspaceHubDocumentAndSlideActions.swift`를 통해 문단 정렬(`ParagraphAlignment`), 줄간격(`100%`/`115%`/`160%`/`200%`), 다단계 목록(`ParagraphListKind` + 수준 증감), 표 셀 병합/분할(`mergeActiveDocumentTableCells`/`splitActiveDocumentTableCell`) 및 테두리 스타일(`setActiveDocumentTableBorderStyle`), 메일 머지(`{{KEY}}` 치환 `applyMailMergeFieldReplacement`)를 즉시 제어하며, 문서/HWP에서 `doc-insert-chart`/`doc-insert-shape` 실행 시 표/문단 삽입과 함께 실제 벡터 그래픽 PNG(`EmbeddedImageAttachment`)를 첨부합니다.
  3. **PD-039 (스프레드시트 셀 내 비례 데이터 막대, 첫 열 고정, 중복 행 제거·구분자 텍스트 나누기, 피벗 집계 함수 선택 및 단일 사본 수식바 검증)**:
     - `isFirstColumnFrozen`(A열 틀 고정), `isDataBarsEnabled`(`normalizedDataBarRatios` 기반 셀 내 비례 가로 막대 오버레이), `selectedPivotAggregation`(`PivotAggregationFunction`: 합계·평균·개수·최소·최대), `removeDuplicatesInActiveSheet()`, `splitActiveSheetColumnByDelimiter(_:)`를 제공합니다.
     - `applyFormulaBarEdit()`에서 불필요한 이중 `WorksheetModel` 사본(`var probe = sheet`) 생성을 제거하여 단일 `var updatedSheet = sheet` 사본 위에서 수식/유효성 검증 후 즉시 반영합니다.
  4. **PD-040 (슬라이드 마스터 테마 4종, 다중 개체 정렬·균등 배분·복제·삭제 및 단조 증가 `zIndex` 정규화)**:
     - `applyThemeToActiveDeck(_:)`(`SlideDeckTheme.allCases`), `alignSlideElements(_:)`(`.alignLeft`, `.alignCenter`, `.alignRight`, `.distributeHorizontally`), `duplicateActiveSlideElement()`, `deleteActiveSlideElement()`, `updateActiveSlideSpeakerNotes(_:)`를 제공합니다.
     - `moveSlideElementZOrder(slideIndex:elementID:bringForward:)`는 개체들을 `(zIndex, enumerations offset)` 기준으로 안정 정렬한 뒤 인접 개체와 위치를 교환하고 `0, 1, 2, ...`의 엄격한 단조 증가 `zIndex`를 재할당하여 동일 `zIndex`(예: 모두 `0`) 개체들 사이에서도 1회 클릭으로 즉각적인 레이어 순서 변경이 일어나도록 보장합니다.
  5. **PD-041 (PDF 전자 결재 도장 4종 프리셋 및 계층형 책갈피 목차 생성·삭제, 도메인 액션 파일 분리)**:
     - `PDFStampPreset`(`승인 (APPROVED)`, `대외비 (CONFIDENTIAL)`, `검토필 (REVIEWED)`, `긴급 (URGENT)`) 기반 전자 결재 스탬프 주석 추가(`addStampToActivePDFPage`)와 계층형 `PDFOutlineNode` 책갈피 추가·삭제(`addBookmarkToActivePDF`, `removeBookmarkFromActivePDF`)를 지원합니다.
     - 비대해진 `WorkspaceHubEditorActions.swift`를 삭제하고 도메인 응집도에 따라 `WorkspaceHubDocumentAndSlideActions.swift`(문서·HWP·슬라이드)와 `WorkspaceHubSheetAndPDFActions.swift`(시트·PDF) 2개 파일로 분리했습니다.
- **Why**: 다크모드 시인성 결함과 슬라이드 동일 `zIndex` 레이어 이동 불능 버그를 해결하고, 코어 엔진에 구현된 원고지 격자·데이터 정제·개체 정렬·결재 스탬프·책갈피 기능을 데스크톱 인스펙터와 리본 전반에 직결하기 위함입니다.
- **Trade-offs**: 문서 차트/도형 삽입 시 `EmbeddedImageAttachment` PNG 데이터(~1KB)가 함께 추가되나, 인라인 이미지 갤러리와 컨테이너 직렬화(`.docx`/`.odt`/`.hwpx`)에서 시각적 그래픽 개체로 즉시 확인·보존할 수 있습니다.
- **Alternatives rejected**:
  - `moveSlideElementZOrder`에서 단순 `zIndex += 1` / `zIndex -= 1` 증감만 수행하는 방식: 초기 `zIndex`가 모두 `0`인 다중 개체에서 역전이 발생하지 않거나 중복 `zIndex`가 남으므로 인덱스 재정규화(`0..<count`) 방식을 채택.
- **Verified by**: `Tests/EasyOfficeTests/DeepDesktopInteractionsTests.swift`
- **Revisit when**: 슬라이드 수직 균등 배분(`.distributeVertically`) 단축 버튼을 리본 상단에 추가할 때.

---

### 24. PD-040 ~ PD-043: 6차 적대적 감사 기반 다중 그룹 리본·문서/슬라이드 수평 눈금자·무간격 1px 세선 시트 그리드·122종 수식 함수·자간/장평 타이포그래피·16:9 슬라이드/홈 템플릿 미니어처 및 PDF 보안 점검·워터마크 정책 (2026-10-19)

- **Status**: Implemented (`Sources/EasyOfficeCore/DocumentEngine.swift`, `Sources/EasyOfficeCore/SheetCoordinatesAndAST.swift`, `Sources/EasyOfficeCore/SheetEngine.swift`, `Sources/EasyOfficeCore/SheetBuiltInFunctions.swift`, `Sources/EasyOfficeCore/SheetLookupTextAndFinancialFunctions.swift`, `Sources/EasyOfficeCore/PDFEngine.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeCore/WorkspaceHubDocumentAndSlideActions.swift`, `Sources/EasyOfficeCore/WorkspaceHubSheetAndPDFActions.swift`, `Sources/EasyOfficeApp/WorkspaceViews.swift`, `Sources/EasyOfficeApp/HomeHubAndSmartViews.swift`, `Sources/EasyOfficeApp/RightContextInspectorView.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`, `Sources/EasyOfficeApp/SheetEditorDetailPaneView.swift`, `Sources/EasyOfficeApp/SlideAndPDFDetailPanes.swift`, `Sources/EasyOfficeApp/SlideshowAndPDFCanvasOverlays.swift`)
- **Decision**:
  1. **PD-040 (다중 그룹 컨텍스트 리본 아키텍처, 문서/슬라이드 수평 눈금자 및 단일 행 무줄바꿈 툴바)**:
     - 상단 컨텍스트 리본을 수직 구분선(`Divider`)과 하단 그룹 캡션(`편집·실행`, `핵심 리본 도구`, `서식·문서 구조`, `빠른 조판·분석`)을 갖춘 **다중 그룹 리본(`ContextRibbonBarView`)**으로 구조화하고, 모든 툴바·상태 배지에 `.lineLimit(1)` + `.fixedSize(horizontal: true, vertical: false)` 및 수평 `ScrollView`를 적용해 `"A4 (210 × 297 mm)"` 등의 텍스트나 한글 버튼이 세로로 꺾이거나 잘리지 않도록 보장합니다.
     - 문서/HWP 편집기 상단에 좌우 여백 음영 구간, `0..16 cm` 눈금(mm 보조 눈금 포함), 선택 문단의 첫 줄/왼쪽 들여쓰기 마커(`▽`/`△`)를 표시하는 `24pt` 높이의 `DocumentHorizontalRulerView`를 배치하고, 슬라이드 편집 스테이지 상단에도 `0..960 pt` 수평 좌표 눈금자(`slideStageHorizontalRuler`)를 배치합니다.
     - 문서 편집기에서 중복 렌더링되던 하단 가로 스크롤 인라인 런 미리보기 행과 디버그용 `선택 구간: 0..<12` / `인라인 런 N구간` 배지를 제거하고, 각주 상단에 표준 `140pt`(`5cm`) 수평 각주 구분선(`각주 구분선 (Footnote Separator)`) 및 다중 구역 문서의 `--- 구역 나눔 (Section Break) ---` 시각 구분선을 표시합니다.
  2. **PD-041 (무간격 0.5pt 세선 스프레드시트 그리드, 첫 열 고정 통합 수직 스크롤, 조건부 서식 범위 사전 계산 및 122종 수식 함수 확장)**:
     - 카드형 여백 그리드를 제거하고 `spacing: 0` 및 `0.5pt` 세선 테두리, `2.0pt` 브랜드 액센트(`#3B4890`) 선택 셀 테두리, 우하단 `6×6` 자동 채우기 핸들(`■`), `"A"`·`"B"`·`"C"` 단일 문자 열 머리글(`26pt`), 기본 행 높이 `30pt`(`wrapText` 활성 시 `44pt`)의 정통 스프레드시트 격자로 전환합니다.
     - `isFirstColumnFrozen == true`일 때 좌우 별도 수직 스크롤 대신 단일 수직 `ScrollView` 내부 `HStack(spacing: 0)`에 고정 열(`# + A열`, `.fixedSize(horizontal: true, vertical: false)`), `2pt` 액센트 고정선, `B..G열` 수평 `ScrollView`를 직결하여 빈 여백(`dead space`)을 0pt로 제거하고 수직 스크롤을 동기화합니다.
     - `SheetEditorDetailPaneView.body` 당 조건부 서식 대상 범위 평가 배열(`condRangeValues`)을 활성 규칙 존재 시 1회만 사전 계산해 셀당 중복 범위 평가를 제거(`PERF-02`)하고, 시트 상단 바에 1클릭 숫자 서식 버튼(`₩`, `%`, `,`, `.0←`, `.00→`, `줄바꿈`) 및 차트 제목/범위 사용자 정의 입력창을 제공합니다.
     - `WorksheetModel.supportedFunctionNames`를 **122종**(기존 92종 + 통계·수학·텍스트·날짜·재무·논리·배열 심화 30종)으로 확장합니다.
  3. **PD-042 (한글 DTP 자간·장평 타이포그래피 엔진 및 NSTextView `.kern`/`.expansion` 실시간 반영)**:
     - `ParagraphBlock`에 `letterSpacingPercent: Double` (`-50.0...50.0`, 기본 `0.0`) 및 `charWidthScalePercent: Double` (`50.0...200.0`, 기본 `100.0`)을 추가하고, `SelectableParagraphTextView.buildAttributedString(from:)`에서 `.kern`(`fontSizePt * (letterSpacingPercent / 100.0)`)과 `.expansion`(`log(max(0.5, charWidthScalePercent / 100.0))`)을, 비편집 문단 행(`paragraphRow`)에서 `.tracking(kernPt)`을 즉시 적용합니다.
     - 문서/HWP 빠른 서식 바 및 우측 인스펙터에서 `adjustActiveParagraphTypography(letterSpacingDelta:charScaleDelta:reset:)`(`자간 -5%`, `자간 +5%`, `장평 95%`, `장평 105%`, `기본`)를 1클릭으로 조작할 수 있도록 연결합니다.
  4. **PD-043 (16:9 슬라이드 미니 썸네일 레일, 홈 허브 시각적 문서 프리뷰 카드, PDF 보안 점검 배지·워터마크 바 및 1Hz 발표자 타이머 프로퍼티화)**:
     - 슬라이드 편집기 좌측 레일을 `160×90` 16:9 미니 슬라이드 캔버스 카드(`slideMiniatureThumbnailCard`: 마스터 테마 배경, 액센트 상단 바, 슬라이드 번호 배지, 제목/부제 및 개체 미니 도형 프리뷰)로 업그레이드하고, 스테이지 하단의 중복 불릿/개체 텍스트 편집 폼을 제거하여 16:9 스테이지(`360pt`)와 하단 발표자 노트 바의 시각적 집중도를 높입니다.
     - 홈 허브(`HomeHubAndSmartViews.swift`)의 템플릿 및 최근 문서 카드를 도메인별 실제 문서 축소판(문서 본문 줄, 시트 미니 격자, 슬라이드 16:9 미니 스테이지, PDF 스탬프/페이지, HWP 결재란 격자) 그래픽 프리뷰로 렌더링합니다.
     - `PDFStudioDetailPaneView`에 `pdf.securityInspectionSummary` 배지와 워터마크 입력/적용 바(`applyWatermarkOnActivePDF()`)를 추가하고, `PDFKitRepresentedView` 높이를 `380pt`로 확장하며 자유곡선 서명 캔버스(`PDFInteractiveInkOverlayView`, `96pt`)를 토글(`isPDFInkOverlayExpanded`)로 접고 펼칠 수 있게 합니다.
     - `SlideEditorDetailPaneView`와 `SlideshowFullScreenOverlayView`에서 `Timer.publish(every: 1.0, on: .main, in: .common).autoconnect()`를 `body` 내부 인라인 생성 대신 뷰 구조체의 저장 프로퍼티(`private let presenterTimer`)로 분리하여 뷰 재평가 시 타이머 퍼블리셔 재생성을 방지합니다(`PERF-01`).
- **Why**: 6차 적대적 UI/UX 감사에서 식별된 툴바 줄바꿈·문서 이중 텍스트 표시·시트 카드 간격 및 첫 열 고정 여백·슬라이드 텍스트형 목록·PDF 캔버스 협소함·타이머 재생성 문제를 근본적으로 해결하고 상용 데스크톱 오피스 수준의 조판·그리드·프레젠테이션·PDF 밀도를 완성하기 위함입니다.
- **Trade-offs**: 눈금자, 미니 캔버스 프리뷰, 다중 그룹 리본 렌더링이 추가되나, SwiftUI `Canvas` 및 사전 계산된 조건부 서식 캐시(`condRangeValuesByRuleID`)를 사용해 런타임 오버헤드를 최소화합니다.
- **Alternatives rejected**:
  - 첫 열 고정 시 고정 열과 가변 열을 별도 `ScrollView(.vertical)`로 분리 유지: 스크롤 오프셋 불일치와 우측 빈 여백이 발생하므로 단일 수직 `ScrollView` 내 `HStack(spacing: 0)` 결합 구조로 대체.
- **Verified by**: `Tests/EasyOfficeTests/DeepDesktopInteractionsTests.swift` (`testSixthAuditTypographyAdjustmentsQuickNumberFormatsAndPDFWatermarkActions`), `Tests/EasyOfficeTests/WorkspaceUIParityTests.swift`
- **Revisit when**: 문서 세로 눈금자(Vertical Page Ruler) 및 탭 정지점(Tab Stop) 드래그 마커를 추가할 때.


