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
25. [PD-047 ~ PD-051: 7차 적대적 감사 기반 무클리핑 단일 컨텍스트 툴바·수직 눈금자/탭 정지점·단일 A4 WYSIWYG 조판·9열 시트/외곽 2pt 범위 테두리·수식 연산자(&/%)·슬라이드 단일 필름스트립/전환 타이밍·PDF 상단 렌더링 및 찾기/바꾸기 내비게이션 정책 (2026-10-19)](#25-pd-047--pd-051-7차-적대적-감사-기반-무클리핑-단일-컨텍스트-툴바수직-눈금자탭-정지점단일-a4-wysiwyg-조판9열-시트외곽-2pt-범위-테두리수식-연산자슬라이드-단일-필름스트립전환-타이밍pdf-상단-렌더링-및-찾기바꾸기-내비게이션-정책-2026-10-19)

---

### 1. macOS 네이티브 Swift 6 + SwiftUI/AppKit 하이브리드 아키텍처 채택

- **Status**: Superseded (초기 아키텍처 설정)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
- **Decision**: 애플리케이션 코어(`EasyOfficeCore`), 디자인 시스템(`EasyOfficeDesignSystem`), 데스크톱 앱(`EasyOfficeApp`)을 Swift 6 Strict Concurrency 기반 네이티브 패키지로 구성하고, 선언적 레이아웃(홈 허브, 리본, 인스펙터)은 SwiftUI로, 고밀도 텍스트 조판·대용량 스프레드시트 그리드·PDFKit 캔버스 통합은 AppKit 브리지로 구현합니다.
- **Why**: macOS 데스크톱 오피스 환경에서 즉각적인 콜드 스타트(<100ms), 낮은 메모리 점유율, Retina/ProMotion 고주사율 스크롤, 시스템 다크모드·Vibrancy·단축키·VoiceOver 접근성 통합을 달성하려면 네이티브 프레임워크가 필수적입니다.
- **Trade-offs**: 웹뷰 기반 크로스플랫폼 프레임워크 대비 UI 컴포넌트를 macOS 네이티브로 정밀하게 구현해야 하므로 초기 렌더러 설계 공수가 높습니다.
- **Alternatives rejected**:
  - Electron / Chromium 번들: 기본 유휴 메모리 300MB 이상 및 대용량 시트/문서 스크롤 시 프레임 드랍이 발생하여 기각.
- **Verified by**: `swift build`, `swift test`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`
- **Revisit when**: macOS 외 타 데스크톱 OS용 별도 클라이언트 요구가 공식 로드맵에 추가될 때(코어 엔진만 C-ABI/WASM 모듈로 분리 검토).

---

### 2. 시그니처 라벤더 디자인 시스템과 도메인별 시맨틱 액센트 조화

- **Status**: Superseded (`Sources/EasyOfficeDesignSystem/DesignTokens.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
- **Decision**: 브랜드 기본 액센트로 시그니처 라벤더(`#5C6BC0`, Hover `#4A58A9`, Active `#3B4890`, Light Scaffold `#F7F8FC`, Dark Scaffold `#141724`)를 사용하고, 문서 유형 식별을 위한 보조 시맨틱 액센트(Document Indigo `#5C6BC0`, Sheet Emerald `#0A705A`, Slide Amber-Coral `#D96B27`, PDF Crimson `#D9383A`, HWP Royal Violet `#6E56CF`)를 채도와 명도 대비(WCAG AA 4.5:1 이상)에 맞춰 통합합니다.
- **Why**: 올인원 오피스 워크스페이스에서 현재 열려 있는 탭과 파일 카드가 워드 문서인지, 시트인지, 슬라이드인지, 한글(HWP/HWPX)인지, PDF인지 0.1초 만에 시각적으로 구분하면서도 브랜드 고유의 차분하고 정교한 라벤더 톤앤매너("우리의 결")를 잃지 않게 합니다.
- **Trade-offs**: 임의의 시스템 기본 색상 대신 검증된 디자인 토큰만 사용하도록 강제해야 합니다.
- **Alternatives rejected**:
  - 원색 위주의 고채도 레거시 오피스 색상: 장시간 문서 작업 시 눈의 피로도가 높고 브랜드 일체감을 해쳐 기각.
- **Verified by**: `Tests/EasyOfficeTests/EasyOfficeTests.swift` (`testDesignSystemTokensAndContrast`)
- **Revisit when**: 고대비(High Contrast) 접근성 전용 테마 확장이 필요할 때.

---

### 3. 올인원 단일 윈도우 멀티 탭 + 컨텍스트 리본 및 우측 인스펙터 결합 레이아웃

- **Status**: Superseded (`Sources/EasyOfficeApp/`, `docs/design-system.md`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/FormatDetector.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
- **Decision**: 모든 입력 문서는 파일 확장자만 신뢰하지 않고 바이너리 매직 바이트(`PK\x03\x04` ZIP/OOXML/ODF/HWPX, `\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1` OLE2 CFBF/HWP 5.0, `%PDF-` ISO 32000 PDF, UTF-8 텍스트 검증)와 크기 상한을 먼저 검증하며, 시그니처 불일치·빈 페이로드·크기 초과 시 즉시 타입화된 에러(`EasyOfficeError`)로 차단(Fail-Closed)합니다.
- **Why**: 확장자만 바꾼 손상 파일이나 압축 폭탄(ZIP/OLE2 bomb)이 파서 내부로 진입해 크래시나 데이터 오염을 일으키는 것을 입구에서 차단하기 위함입니다.
- **Trade-offs**: 헤더가 손상된 비표준 파일은 자동 복구 모드를 명시적으로 호출하기 전까지 기본 열기가 거부됩니다.
- **Alternatives rejected**:
  - 파싱 실패 시 빈 문서로 대체하거나 텍스트로 강제 디코딩(Silent Fallback): 원본 데이터 유실 및 깨진 문자열 저장 위험이 있어 기각.
- **Verified by**: `Tests/EasyOfficeTests/EasyOfficeTests.swift` (`testFormatDetectorMagicSignaturesAndFailClosed`)
- **Revisit when**: 암호화된 컨테이너(OOXML Agile Encryption / HWP 배포용 문서) 전용 헤더 시그니처 분기를 추가할 때.

---

### 5. 한글 문서(HWP 5.0 OLE2 / HWPX KS X 6101) 및 OOXML/ODF/PDF 통합 지원

- **Status**: Superseded (`Sources/EasyOfficeCore/DocumentFormat.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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
- **개정 (2026-11-08, QA 승인)**: 개인 프로젝트 수준으로 절차를 줄입니다. PR 템플릿은 요약·검증·미실행 세 칸만 두고(Class 등급·위험도·체크리스트 칸 삭제), 이슈 템플릿의 필수 항목을 없애고 빈 이슈를 허용합니다. 병합은 squash만 쓰고, `main` 병합 필수 체크는 `checks`·`verify`·`Verify Rust Core & Tauri Shell (Windows x64)` 세 job입니다(리포 ruleset).
- **개정 2 (2026-11-08, QA 결정)**: 1인 프로젝트 방식으로 바꿉니다. 이슈·PR·라벨·템플릿·automerge를 쓰지 않고, 격리 worktree에서 검증한 커밋을 `main`에 바로 push합니다. 남은 일은 루트 `TODO.md` 한 파일로 관리합니다. `main` 규칙은 삭제 금지와 force-push 금지만 남기고, CI는 push에서 돕니다. 기존 이슈는 모두 삭제했습니다.
- **Superseded by PD-094 (부분)**: 이 항목 중 Swift 앱·`swift build`/`swift test`·`scripts/package-macos-release.sh`·Swift 대응 규칙에 해당하는 부분은 PD-094로 대체되었습니다. Rust 엔진·WebUI·Windows 관련 결정은 그대로 유효합니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/BinaryContainerCodec.swift`, `Sources/EasyOfficeCore/TabularAndMarkupCodec.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/SheetEngine.swift`, `Sources/EasyOfficeCore/SheetBuiltInFunctions.swift`, `Sources/EasyOfficeCore/SheetLookupTextAndFinancialFunctions.swift`, `Sources/EasyOfficeCore/DocumentEngine.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/SmartProductivityTools.swift`, `Sources/EasyOfficeCore/PDFBinarySerializer.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeDesignSystem/DesignTokens.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeApp/WorkspaceViews.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Tests/EasyOfficeTests/CommercialParityBenchmarkTests.swift`, `scripts/package-macos-release.sh`, `.github/workflows/release.yml`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/BinaryContainerCodec.swift`, `Sources/EasyOfficeCore/XMLSanitizer.swift`, `Sources/EasyOfficeCore/StandardZipOfficeCodecs.swift`, `Sources/EasyOfficeCore/ODFAndHWPXCodecs.swift`, `Sources/EasyOfficeCore/OLE2AndHWP5Codec.swift`, `Sources/EasyOfficeCore/LegacyOLE2OfficeCodec.swift`, `Sources/EasyOfficeCore/TabularAndMarkupCodec.swift`, `Sources/EasyOfficeCore/SheetEngine.swift`, `Sources/EasyOfficeCore/SheetAnalyticsAndPivot.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/PDFBinarySerializer.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeApp/EasyOfficeApp.swift`, `Sources/EasyOfficeApp/WorkspaceViews.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/SheetEngine.swift`, `Sources/EasyOfficeCore/SheetBuiltInFunctions.swift`, `Sources/EasyOfficeCore/SheetLookupTextAndFinancialFunctions.swift`, `Sources/EasyOfficeCore/DocumentEngine.swift`, `Sources/EasyOfficeCore/SlideEngine.swift`, `Sources/EasyOfficeCore/StandardZipOfficeCodecs.swift`, `Sources/EasyOfficeCore/ODFAndHWPXCodecs.swift`, `Sources/EasyOfficeCore/OLE2AndHWP5Codec.swift`, `Sources/EasyOfficeCore/LegacyOLE2OfficeCodec.swift`, `Sources/EasyOfficeCore/LocalSmartAssistant.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeCore/WorkspaceRibbonCommandRouter.swift`, `Sources/EasyOfficeCore/PDFEngine.swift`, `Sources/EasyOfficeCore/PDFBinarySerializer.swift`, `Sources/EasyOfficeApp/WorkspaceViews.swift`, `Sources/EasyOfficeApp/HomeHubAndSmartViews.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`, `Sources/EasyOfficeApp/SlideAndPDFDetailPanes.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/WorkspaceCatalogAndModels.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeCore/WorkspaceRibbonCommandRouter.swift`, `Sources/EasyOfficeApp/RightContextInspectorView.swift`, `Sources/EasyOfficeApp/WorkspaceViews.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`, `Sources/EasyOfficeApp/SlideAndPDFDetailPanes.swift`, `Sources/EasyOfficeApp/HomeHubAndSmartViews.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/DocumentEngine.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/SheetAutoFillAndLayout.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeApp/SheetEditorDetailPaneView.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/SlideEngine.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeApp/SlideAndPDFDetailPanes.swift`, `Sources/EasyOfficeApp/SlideshowAndPDFCanvasOverlays.swift`, `Sources/EasyOfficeApp/WorkspaceViews.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/PDFEngine.swift`, `Sources/EasyOfficeCore/PDFBinarySerializer.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeApp/SlideAndPDFDetailPanes.swift`, `Sources/EasyOfficeApp/SlideshowAndPDFCanvasOverlays.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/XMLSanitizer.swift`, `Sources/EasyOfficeCore/BinaryContainerCodec.swift`, `Sources/EasyOfficeCore/DocumentEngine.swift`, `Sources/EasyOfficeCore/DocumentSubmodelsAndNotes.swift`, `Sources/EasyOfficeCore/SheetEngine.swift`, `Sources/EasyOfficeCore/SheetAutoFillAndLayout.swift`, `Sources/EasyOfficeCore/SheetLookupTextAndFinancialFunctions.swift`, `Sources/EasyOfficeCore/PDFEngine.swift`, `Sources/EasyOfficeCore/PDFBinarySerializer.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeCore/WorkspaceHubEditorActions.swift`, `Sources/EasyOfficeCore/WorkspaceRibbonCommandRouter.swift`, `Sources/EasyOfficeApp/RightContextInspectorView.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`, `Sources/EasyOfficeApp/SheetEditorDetailPaneView.swift`, `Sources/EasyOfficeApp/SlideAndPDFDetailPanes.swift`, `Sources/EasyOfficeApp/SlideshowAndPDFCanvasOverlays.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeDesignSystem/DesignTokens.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeCore/WorkspaceHubDocumentAndSlideActions.swift`, `Sources/EasyOfficeCore/WorkspaceHubSheetAndPDFActions.swift`, `Sources/EasyOfficeCore/WorkspaceRibbonCommandRouter.swift`, `Sources/EasyOfficeApp/RightContextInspectorView.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`, `Sources/EasyOfficeApp/SheetEditorDetailPaneView.swift`, `Sources/EasyOfficeApp/SlideAndPDFDetailPanes.swift`, `Sources/EasyOfficeApp/SlideshowAndPDFCanvasOverlays.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

- **Status**: Superseded (`Sources/EasyOfficeCore/DocumentEngine.swift`, `Sources/EasyOfficeCore/SheetCoordinatesAndAST.swift`, `Sources/EasyOfficeCore/SheetEngine.swift`, `Sources/EasyOfficeCore/SheetBuiltInFunctions.swift`, `Sources/EasyOfficeCore/SheetLookupTextAndFinancialFunctions.swift`, `Sources/EasyOfficeCore/PDFEngine.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeCore/WorkspaceHubDocumentAndSlideActions.swift`, `Sources/EasyOfficeCore/WorkspaceHubSheetAndPDFActions.swift`, `Sources/EasyOfficeApp/WorkspaceViews.swift`, `Sources/EasyOfficeApp/HomeHubAndSmartViews.swift`, `Sources/EasyOfficeApp/RightContextInspectorView.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`, `Sources/EasyOfficeApp/SheetEditorDetailPaneView.swift`, `Sources/EasyOfficeApp/SlideAndPDFDetailPanes.swift`, `Sources/EasyOfficeApp/SlideshowAndPDFCanvasOverlays.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
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

---

### 25. PD-047 ~ PD-051: 7차 적대적 감사 기반 무클리핑 단일 컨텍스트 툴바·수직 눈금자(`0..24cm`)·단일 A4 WYSIWYG 조판·9열 시트/외곽 2pt 범위 테두리·수식 연산자(`+`/`%`)/`ISBLANK`·슬라이드 단일 필름스트립/전환 타이밍·PDF 상단 렌더링 및 찾기/바꾸기 내비게이션 정책 (2026-10-19)

- **Status**: Superseded (`Sources/EasyOfficeCore/SlideEngine.swift`, `Sources/EasyOfficeCore/SheetCoordinatesAndAST.swift`, `Sources/EasyOfficeCore/SheetEngine.swift`, `Sources/EasyOfficeCore/SheetBuiltInFunctions.swift`, `Sources/EasyOfficeCore/SheetAutoFillAndLayout.swift`, `Sources/EasyOfficeCore/PDFBinarySerializer.swift`, `Sources/EasyOfficeCore/StandardZipOfficeCodecs.swift`, `Sources/EasyOfficeCore/ODFAndHWPXCodecs.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `Sources/EasyOfficeCore/WorkspaceHubDocumentAndSlideActions.swift`, `Sources/EasyOfficeCore/WorkspaceHubSheetAndPDFActions.swift`, `Sources/EasyOfficeApp/WorkspaceViews.swift`, `Sources/EasyOfficeApp/EditorDetailPanes.swift`, `Sources/EasyOfficeApp/SheetEditorDetailPaneView.swift`, `Sources/EasyOfficeApp/SlideAndPDFDetailPanes.swift`, `Sources/EasyOfficeApp/RightContextInspectorView.swift`, `Sources/EasyOfficeApp/HomeHubAndSmartViews.swift`)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
- **Decision**:
  1. **PD-047 (무클리핑 단일 컨텍스트 툴바 및 접이식 고급 도구 서랍 아키텍처 — `UX-08`)**:
     - 상단 `ContextRibbonBarView` 전체를 `ScrollView(.horizontal, showsIndicators: false)`로 감싸고 그룹 간격을 컴팩트화(`spacing: 8`, 내부 `spacing: 5`, 패딩 `horizontal: 10, vertical: 6`)하여 표준 데스크톱 창 및 2-Pane 스플릿 뷰에서 우측 끝 버튼이나 캡션이 1px도 잘리지 않도록 보장합니다.
     - 문서/HWP, 시트, 슬라이드, PDF 스튜디오 상단에 적층되어 세로 뷰포트를 잠식하던 다중 툴바 행들을 **단일 32~34pt 컴팩트 기본 컨텍스트 바(`ScrollView(.horizontal, showsIndicators: false)`) + 선택적 접이식 고급 도구 서랍(`viewModel.isAdvancedToolbarExpanded`)** 구조로 통합하고, `WorkspaceViewModel.setActiveParagraphStyle(_:)`가 `.document`와 `.hwp` 탭 모두에서 문단 기본 속성(`fontSizePt`, `isBold`, `isItalic`)과 인라인 `runs`를 일관되게 동기화합니다.
  2. **PD-048 (문서/슬라이드 수직 눈금자 및 단일 A4 WYSIWYG 조판 밀도 — `UX-09`)**:
     - 문서/HWP A4 용지 좌측에 `0..24 cm` 세로 눈금자(`DocumentVerticalRulerView`, `width: 22`)를, 16:9 슬라이드 스테이지 좌측에 `0..540 pt` 세로 눈금자(`SlideVerticalRulerView`, `width: 22`)를 배치하여 상단 가로 눈금자와 함께 2축 정밀 조판 프레임을 구성합니다.
     - A4 용지 본문 내 문단 위 디버그 배지를 제거하고 수식 블록(`수식 1`)과 벡터 다이어그램(`첨부 이미지`)을 2열 가로 배치하여, 첫 화면(`1520×960`) 안에 머리말·자동 생성 목차·제목/본문/인용문·수식/다이어그램·표(`표 1`)·`140pt` 각주 구분선 및 각주·꼬리말이 스크롤 없이 한눈에 조판되도록 일원화합니다.
  3. **PD-049 (9열 풀와이드 스프레드시트 격자, 외곽 2pt 다중 셀 선택 테두리 및 2차 열 다중 정렬 — `UX-10`, `FUNC-05`)**:
     - 기본 표시 열을 `A..F`(6열)에서 `A..I`(9열, `max(9, sheet.columnCount)`) 및 `1..10`(10행)으로 확장하여 데스크톱 가로 뷰포트의 우측 유휴 여백을 해소하고, 첫 열 고정(`isFirstColumnFrozen`) 시에도 행 전체 열(`0..<columnLabels.count`)의 `wrapText` 여부를 기준으로 고정 열과 스크롤 열의 행 높이를 동기화합니다.
     - 다중 셀 범위 선택(`B2:C3` 등) 시 개별 셀마다 4면 굵은 테두리를 그리던 방식을 폐기하고, 선택 범위 내부 셀에는 연한 라벤더 틴트만 채운 뒤 **선택 범위(`isInRange`) 전체의 최외곽 경계선(Top/Bottom/Leading/Trailing Perimeter)에만 `2.0pt` 액센트 테두리와 우하단(`selMaxRow, selMaxCol`) 단일 자동 채우기 핸들(`6×6`)**을 렌더링합니다.
     - `WorksheetModel.sortRows(byColumn:ascending:hasHeader:secondaryColumn:secondaryAscending:)` 및 `WorkspaceViewModel.sortActiveSheetBySecondaryColumn(...)`을 통해 1차 기준 열 동률 시 2차 기준 열로 결정론적 정렬을 수행합니다.
  4. **PD-050 (단일 소스 슬라이드 필름스트립·전환 타이밍/격자선 및 PDF 상단 직렬화·A4 페이지 미니어처 — `UX-11`, `UX-12`, `FUNC-07`)**:
     - 좌측 내비게이터(`viewModel.showLeftNavigator == true`)가 열려 있을 때는 `SlideEditorDetailPaneView` 내부의 중복 슬라이드 썸네일 레일을 숨겨 화면 위 슬라이드 필름스트립을 **단일 소스(Single Source of Truth)**로 유지합니다.
     - `SlideModel`에 `transitionDurationSeconds: Double`(`0.2...5.0`, 기본 `0.8`) 및 `autoAdvanceSeconds: Int?`를 추가하고 `.pptx`(`eo:transitionDuration`, `eo:autoAdvance`) 및 `.odp` 컨테이너 직렬화 라운드트립과 스테이지 격자 안내선(`isSlideGridlinesVisible`)을 지원합니다.
     - `PDFBinarySerializer.encode(_:)`의 페이지 본문/주석/결재 날인 박스(`386 576 160 46 re B`)/AcroForm/전자서명 박스(`48 452 280 46 re B`) 좌표를 상·중단 가시 영역(`y = 804..452`)으로 배치하여 `PDFKitRepresentedView`(`minHeight: 520`)에서 페이지 전체가 빈 상단 없이 균형 있게 렌더링되도록 개선하고, 좌측 내비게이터에 A4 비례 페이지 미니어처 카드를 추가합니다.
  5. **PD-051 (찾기/바꾸기 실시간 매치 카운터·내비게이션, 수식 연산자/`ISBLANK` 패리티 및 하단 상태바·홈 필터 — `FUNC-05`, `FUNC-06`, `FUNC-08`)**:
     - `FormulaTokenizer` 및 `FormulaParser`가 단항 `+` 연산자(`=+A1+B1`)와 후위 백분율 연산자 `%`(`=200*15%` → `30.0`)를 파싱·평가하고, `ISBLANK`가 ECMA-376 / OpenFormula 표준에 따라 진짜 빈 셀(`.empty`)에만 `TRUE`를 반환하며(`=""` 빈 문자열 수식은 `FALSE`), 총 **122종** 내장 수식 함수를 제공합니다.
     - `FindAndReplaceBarView`에 실시간 매치 카운터(`currentFindMatchIndex / findMatchCount 건`), 대소문자 구분 토글(`isFindCaseSensitive`), `◀ 이전`(`findPreviousMatch()`), `다음 ▶`(`findNextMatch()`), `바꾸기(1건)`(`replaceCurrentMatch()`), `모두 바꾸기`를 연결합니다.
     - 하단 상태바(`BottomWorkspaceStatusBarView`)에 모듈별 실시간 지표를 표시하고, 홈 허브 템플릿 및 최근 문서 섹션에 카테고리/포맷 필터 필(`selectedHomeTemplateCategoryFilter`, `selectedHomeRecentFormatFilter`)을 제공합니다.
- **Why**: 7차 적대적 실사용 감사에서 확인된 상단 리본·툴바 우측 잘림, 다중 툴바 적층으로 인한 캔버스 협소화, 문서 A4 용지 첫 화면 조밀도 부족, 시트 다중 선택 시 내부 격자 테두리 겹침 및 우측 여백, 슬라이드 이중 필름스트립, PDF 페이지 하단 쏠림 문제를 전면 해소하기 위함입니다.
- **Trade-offs**: 수직 눈금자(`DocumentVerticalRulerView`, `SlideVerticalRulerView`)와 좌측 내비게이터 미니어처 렌더링이 추가되나, 경량 SwiftUI 도형과 값 타입 모델을 사용하여 60fps 반응성을 유지합니다.
- **Alternatives rejected**:
  - 다중 셀 범위 선택 시 각 셀에 `border(width: 2)`를 개별 적용하는 기존 방식: 범위 내부 격자선이 `4pt` 두께로 겹쳐 보여 상용 스프레드시트 UX와 괴리가 크므로 최외곽 경계선(Perimeter) 조건부 렌더링 방식으로 대체.
- **Verified by**: `Tests/EasyOfficeTests/DeepDesktopInteractionsTests.swift` (`testSeventhAuditInteractiveFindReplaceSlideTransitionGridlinesAndMultiColumnSort`), `Tests/EasyOfficeTests/EnterpriseParityAndOptimizationTests.swift` (`testSeventhAuditFormulaUnaryPlusPostfixPercentIsBlankAndMultiColumnSort`)
- **Revisit when**: 사용자 정의 리본 탭 순서 편집 또는 다중 윈도우 분리(Tear-off Tab Window) 요구사항이 추가될 때.

### 26. 8차 심층 적대적 감사 기반 무결점 CoreGraphics 스냅샷 래스터화, 올인원 데스크톱 UI/UX 정밀 개편 및 코어·레거시 코덱 동기화 정책 (2026-10-24)

- **Status**: Superseded (이전: Implemented)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
- **Decision (`PD-052`)**:
  1. **무결점 CoreGraphics 스냅샷 래스터화 호환성 및 순수 SwiftUI 컨트롤 일원화 (`UX-01`~`UX-12`)**:
     - `WorkspaceViewModel`에 `isCanvasSnapshotMode: Bool = false` 플래그를 도입하고, `AdaptiveScrollContainer`, `LavenderMenuPill`, `LavenderTextFieldPill`, `LavenderTogglePill`, `LavenderStepperPill` 공통 컴포넌트를 통해 오프스크린 `NSView.cacheDisplay(in:to:)` 캡처 시 빈 사각형이나 잘림을 유발하던 AppKit `NSScrollView`/`NSTextView`/`PDFView`/`NSPopUpButton`/`NSTextField(.roundedBorder)` 브리지를 순수 SwiftUI/CoreGraphics 벡터 렌더링으로 일원화합니다.
     - 상단 문서 탭 스트립(`TopDocumentTabStripView`), 컨텍스트 리본(`ContextRibbonBarView`), 에디터 2차 포맷 바, 우측 컨텍스트 인스펙터(`RightContextInspectorView`), 좌측 내비게이터(`LeftNavigatorPanelView`), 하단 상태바(`BottomWorkspaceStatusBarView`) 전 구간에서 컨트롤 잘림과 텍스트 말줄임표를 해소합니다.
  2. **모듈별 데스크톱 오피스 전용 UX 정밀 고도화**:
     - **홈 허브 (`HomeHubAndSmartViews.swift`)**: `selectedHomeNavSection`이 `.converter` 또는 `.smartAssistant`일 때 하단 최근 문서 테이블 중복 노출을 차단하고, 최근 문서 포맷 필터를 `["전체", "DOCX", "HWP/HWPX", "XLSX", "PPTX", "PDF", "ODF"]`로 세분화하며 최종 열람 시각(`lastOpenedISO8601`)과 즐겨찾기(`★`)·상단 고정(`📌`) 즉시 토글을 제공합니다.
     - **문서 및 HWP/HWPX 에디터 (`EditorDetailPanes.swift`)**: `.hwp` 탭 상단에 KS X 6101 / HWP 5.0 공공·행정 문서 규격 안내 배너와 전자결재 협조란을 렌더링하고, 페이지 가로/세로 방향(`isLandscape`) 및 다단 조판(`pageColumnCount >= 2`)을 A4 캔버스 레이아웃에 실시간 반영합니다.
     - **스프레드시트 (`SheetEditorDetailPaneView.swift`)**: 수식 입력줄 좌측 이름 상자(`Name Box`) 옆에 취소(`×`), 입력 확인(`✓`), 함수 마법사(`fx`) 버튼을 배치하고, 스냅샷 모드 및 64행 이하 시트에서 `VStack` 즉시 렌더링으로 하단 요약 행까지 누락 없이 표시합니다.
     - **슬라이드 및 PDF 스튜디오 (`SlideAndPDFDetailPanes.swift`)**: 16:9 슬라이드 스테이지에서 상단 제목/불릿 영역(`y: 0..215pt`)과 하단 도형/차트/표 스테이지 영역(`y >= 240pt`)을 분리하여 겹침을 원천 차단하고, PDF 스튜디오 우측 본문에 네이티브 A4 벡터 페이지 시트(`pdfNativePageSheetView`)를 배치하여 본문 텍스트, 하이라이트, 주석 스티커, 전자결재 승인 도장(`APPROVED · 결재완료`), AcroForm 위젯, 대각선 워터마크, 벡터 자유곡선 주석이 백지 현상 없이 선명하게 렌더링되도록 보장합니다.
  3. **찾기/바꾸기 양방향 동기화, 시트별 조건부 서식·유효성 규칙 영속화 및 레거시 바이너리 코덱 패리티**:
     - `findQueryText` ↔ `findQueryDraft` 및 `replaceQueryText` ↔ `replaceQueryDraft`를 양방향 동기화하고, `performFindAndReplace()`가 `isFindCaseSensitive` 및 문단/슬라이드 `inlineRuns`·`runs`·`speakerNotes`를 모두 갱신하도록 통합합니다.
     - `WorksheetModel` 및 `WorksheetSheetPage`에 `conditionalFormatRules: [SheetConditionalFormatRule]`와 `dataValidationRule: SheetDataValidationRule?`을 내장하여 탭 전환·시트 탭 전환·실행 취소(`undo`)/다시 실행(`redo`) 및 `.xlsx`/`.xls`(BIFF8)/`.ods` 직렬화 라운드트립에서 손실 없이 보존합니다.
     - 레거시 바이너리 프레젠테이션(`.ppt`) 코덱(`LegacyOLE2OfficeCodec`)이 `deck.deckTheme`(`0x0020` 인스턴스 태그)과 슬라이드별 `transitionDurationSeconds`·`autoAdvanceSeconds`를 무손실 라운드트립하도록 확장합니다.
- **Why**: 8차 적대적 실사용 및 오프스크린 스냅샷 감사에서 발견된 AppKit 브리지 뷰 백지화/클리핑, 슬라이드 도형과 본문 텍스트 겹침, 탭 전환 시 시트 조건부 서식·데이터 유효성 규칙 유실, 찾기/바꾸기 프로퍼티 이원화 버그를 근본적으로 해결하기 위함입니다.
- **Trade-offs**: 순수 SwiftUI/CoreGraphics 기반 커스텀 컨트롤(`LavenderMenuPill`, `LavenderTextFieldPill` 등)과 PDF 벡터 페이지 시트 렌더러를 유지 관리해야 하나, macOS 온스크린 대화형 환경과 오프스크린 스냅샷 검증 환경 모두에서 100% 결정론적 시각 품질을 보장합니다.
- **Alternatives rejected**:
  - `PDFKitRepresentedView`나 `NSScrollView`에 타이머 지연을 추가하여 스냅샷 백지 현상을 우회하려는 시도: 비결정론적 플레이키니스를 유발하고 근본 원인을 해결하지 못하므로 배제.
- **Verified by**: `Tests/EasyOfficeTests/DeepDesktopInteractionsTests.swift` (`testEighthAuditFindReplaceSyncSheetRulesPersistencePPTThemeRoundTripAndSnapshotMode`)
- **Revisit when**: 외부 플러그인 기반 서드파티 렌더러 확장 인터페이스를 도입할 때.

### 27. Mac·Windows 크로스플랫폼 Rust 멀티 크레이트 코어 엔진(`crates/easyoffice-*`) 및 Tauri v2 데스크톱 아키텍처 정책 (2026-10-24)

- **Superseded by PD-094 (부분)**: 이 항목 중 Swift 앱·`swift build`/`swift test`·`scripts/package-macos-release.sh`·Swift 대응 규칙에 해당하는 부분은 PD-094로 대체되었습니다. Rust 엔진·WebUI·Windows 관련 결정은 그대로 유효합니다.
- **Decision (`PD-053`)**:
  1. **무외부의존 순수 Rust 멀티 크레이트 워크스페이스(`Cargo.toml`, `crates/easyoffice-*`)**:
     - macOS와 Windows 양대 데스크톱 플랫폼에서 20종 표준 오피스 포맷 검증, 수식 계산, 조판 레이아웃, Fail-Closed 보안 경계를 단일 코드베이스로 공유하기 위해 루트 `Cargo.toml`(`resolver = "2"`, `edition = "2021"`, 외부 크레이트 의존성 `0`개) 아래 4개의 전문 크레이트를 구성합니다.
     - `crates/easyoffice-formats`: 20종 `DocumentFormat` 및 `ContainerFamily` 메타데이터, 매직 바이트 시그니처 검증(`FormatDetector::detect`, `MAX_PAYLOAD_BYTES = 512 MiB`), `ZipSecurityGuard`(`4,096` 엔트리, 단일 `128 MiB`, 총합 `512 MiB`, `100:1` 압축비, `..`/선행 슬래시/NUL/Windows 드라이브 문자 경로 순회 차단), `Ole2SecurityGuard`(`512`바이트 섹터 정렬, `MAX_FAT_SECTORS = 109`, `HashSet<u32>` 기반 FAT 순환 체인 차단, HWP 5.0 `FileHeader` `0x02`/`0x04` 암호화·배포용 문서 차단), `XmlSanitizer`(`<!ENTITY` 및 외부 DTD `SYSTEM`/`PUBLIC`/`[` 차단), `CsvCodec`(RFC 4180 상태 기계), `CommonMarkCodec`, `PdfSerializer`(`%PDF-1.4` 바이트 오프셋 `xref` 직렬화)를 제공합니다.
     - `crates/easyoffice-sheet`: 64비트 패킹 셀 좌표 `PackedCellCoordinate`(`row: 32bit, col: 32bit`, `MAX_ROWS = 1,000,000`, `MAX_COLS = 16,384`), 단일 범위 확장 상한(`MAX_EXPANDED_CELL_COUNT = 100,000`), 단항 `+`/`-`·후위 `%`·산술·비교·문자열 결합(`&`) 연산자 파서, 순환 참조 탐지(`FormulaError::Cycle`) 및 재귀 상한(`MAX_RECURSION_DEPTH = 64`), ECMA-376 호환 `ISBLANK`를 포함한 핵심 내장 함수(`SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT`, `COUNTA`, `IF`, `IFERROR`, `ISBLANK`, `ROUND`, `ABS`, `SQRT`, `POWER`, `MOD`, `VLOOKUP`, `INDEX`, `MATCH`, `LEN`, `CONCAT`, `NPV`, `STDEV`, `MEDIAN`), `PivotTableSummary` 집계를 제공합니다.
     - `crates/easyoffice-layout`: 시그니처 라벤더 팔레트(`#5C6BC0`, `#F7F8FC`, `#141724`), `DocumentPageLayout`(A4 가로/세로 스왑, 1~4단 다단 폭 계산, 200자 원고지 그리드, KS X 6101 행정 공문서 헤더 및 결재란), `SlideStageLayout`(`840.0 × 472.5 pt` 16:9 스테이지 및 상단 본문·하단 오브젝트 비겹침 영역 `y >= 240.0 pt`), `PdfStudioLayout`(`y = 804.0..452.0` 가시 좌표 및 결재 날인 박스)을 제공합니다.
  2. **Tauri v2 크로스플랫폼 데스크톱 IPC 라우터 및 WebUI(`crates/easyoffice-app`)**:
     - `EasyOfficeIpcRouter::dispatch`가 `get_workspace_bootstrap`, `detect_format`, `evaluate_sheet`, `compute_layout`, `audit_security` 명령을 처리하고, `cargo run -p easyoffice-app -- --self-test`로 전 크레이트 통합 불변식을 1초 내에 검증합니다.
     - `crates/easyoffice-app/ui/{index.html,styles.css,app.js}`는 시그니처 라벤더 다크/라이트 워크스페이스, 플랫폼별 단축키(`Cmd` vs `Ctrl`) 자동 매핑, 한국어 두벌식 IME 조합(`compositionstart`/`compositionupdate`/`compositionend`) 무결성을 제공합니다(이후 `PD-062`에서 미사용 데스크톱 번들 설정 파일을 제거하고 루프백 `--serve` WebUI 프리뷰 패키징으로 일원화함).
  3. **문서/HWP 2차 포맷 바 다단·용지 방향 변경 시 실행 취소(`Undo`) 스냅샷 누락 보완 (`Sources/EasyOfficeApp/EditorDetailPanes.swift`)**:
     - 문서 및 HWP/HWPX 에디터 2차 포맷 바의 `다단 구성`(`1단`~`3단`)과 `용지 (세로/가로)` 메뉴 액션에서 페이로드 갱신 직전에 `viewModel.recordUndoSnapshot(for: tabID)`를 호출하여 인스펙터·리본과 동일한 실행 취소/다시 실행 무결성을 보장합니다.
- **Why**: macOS 네이티브 Swift 엔진의 도메인 모델과 보안 상한을 Windows(`WebView2`)와 macOS(`WKWebView`) 공통 Rust 코어로 이식하여 크로스플랫폼 일관성을 확립하고, 오프라인/폐쇄망 빌드 환경에서도 즉시 컴파일·검증되도록 하기 위함입니다.
- **Trade-offs**: Swift 코어(`Sources/EasyOfficeCore`)와 Rust 워크스페이스(`crates/easyoffice-*`) 양쪽의 상한 상수와 보안 불변식을 동기화해야 하나, `check-repo-hygiene.mjs`와 CI `verify` 잡(`cargo test --workspace` + `--self-test`)이 두 계층을 함께 검증합니다.
- **Alternatives rejected**:
  - 수십 개의 서드파티 크레이트에 의존하는 방식: 오프라인/샌드박스 환경에서 네트워크 크레이트 다운로드 실패 위험과 공급망 공격 표면을 넓히므로 `std` 기반 결정론적 구현으로 채택.
- **Verified by**: `cargo test --workspace`, `cargo run -p easyoffice-app -- --self-test`, `node scripts/check-repo-hygiene.mjs`
- **Revisit when**: Windows 네이티브 코드 서명(Authenticode) 및 MSIX 스토어 배포 파이프라인을 추가할 때.

### 28. Rust 5대 모듈 도메인·140종 리본·오프라인 스마트 보조·C-ABI FFI 완비, 표준/한글(HWP) 리본 스위처 및 Windows CI/Release 파이프라인 정책 (2026-10-24)

- **Superseded by PD-094 (부분)**: 이 항목 중 Swift 앱·`swift build`/`swift test`·`scripts/package-macos-release.sh`·Swift 대응 규칙에 해당하는 부분은 PD-094로 대체되었습니다. Rust 엔진·WebUI·Windows 관련 결정은 그대로 유효합니다.
- **Decision (`PD-054`)**:
  1. **Rust 5대 도메인 엔진 및 140종 리본·스마트 보조·PII 감사·C-ABI FFI 완비 (`crates/easyoffice-layout/src/domain.rs`, `crates/easyoffice-app/src/{ribbon.rs,smart_tools.rs,ffi.rs}`)**:
     - `crates/easyoffice-layout/src/domain.rs`에 `RibbonLayoutStyle { StandardOffice, HangulHwp }`, `ContextualObjectToolKind`, `DocumentDomainEngine`(병합 표 `DocumentTableModel` 및 `=SUM(ABOVE)`/`=AVERAGE(LEFT)` 표 셀 수식, 변경 내용 추적 `RevisionRecord`, 각주·미주 `FootnoteEntry`, 1~3단계 자동 목차 생성, `{{필드}}` 메일 머지), `SlideDomainEngine`(5종 마스터 레이아웃, 전환 타이밍, 발표자 레이저 포인터·자유곡선 잉크 `PresenterSessionState`), `PdfDomainEngine`(90도 페이지 회전, 분할·병합, 결재 스탬프, 워터마크, AcroForm 필드)을 구현합니다.
     - `crates/easyoffice-app/src/ribbon.rs`에 7개 카테고리 × 20개 명령(`140`종) `RibbonCommandRouter`와 최대 64스냅샷 `UndoRedoStack<T>`를 구현하고, `crates/easyoffice-app/src/smart_tools.rs`에 100% 오프라인 `LocalSmartAssistant`(TF 요약, 문장 진단, 액션 추출, LCS 문서 비교) 및 `DocumentSecurityAuditor`(주민등록번호·전화번호·이메일 PII 마스킹)를, `crates/easyoffice-app/src/ffi.rs`에 Null-Safe C-ABI 브리지(`easyoffice_ffi_version`, `easyoffice_ffi_dispatch_json`, `easyoffice_ffi_free_string`)를 구현합니다.
  2. **macOS SwiftUI 및 Tauri v2 WebUI 표준 오피스 ↔ 한글(HWP) 리본 레이아웃 스위처 & 컨텍스트 개체 도구 배지 (`Sources/EasyOfficeCore`, `Sources/EasyOfficeApp`, `crates/easyoffice-app/ui/*`)**:
     - `RibbonLayoutStyle`(`표준 오피스` ↔ `한글(HWP) 쪽·편집`: `편집 · 입력 · 쪽·원고지 · 표·개체 · 시트·차트 · 보안·도구 · 보기`)과 `WorkspaceViewModel.activeContextualObjectToolBadge`(`표·개체 도구`, `쪽·결재·원고지 도구`, `수식·피벗 분석`, `발표자·레이저 도구`, `전자결재·주석 도구`)를 macOS `ContextRibbonBarView`와 Tauri v2 데스크톱 WebUI 양쪽에 동일하게 배치합니다.
  3. **GitHub Actions `windows-latest` 병렬 검증 및 릴리스 패키징 파이프라인 (`.github/workflows/ci.yml`, `.github/workflows/release.yml`)**:
     - `.github/workflows/ci.yml`에 `verify-windows` 잡(`runs-on: windows-latest`)을 추가하여 `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo run -p easyoffice-app -- --self-test`를 상시 검증합니다.
     - `.github/workflows/release.yml`에 `build-windows-release` 잡을 추가하여 `EasyOffice-${VERSION}-windows-x64.zip`과 `SHA256SUMS-windows-x64.txt`를 macOS 릴리스 자산과 함께 GitHub Release에 업로드합니다.
- **Why**: Swift 코어와 Rust 멀티 크레이트 워크스페이스의 도메인·UI·보안 패리티를 100%로 일치시키고, macOS와 Windows 양대 데스크톱 환경에서 표준 오피스 리본과 한국형 공문서(HWP) 쪽·편집 리본을 즉시 전환할 수 있도록 하기 위함입니다.
- **Trade-offs**: Windows CI 러너(`windows-latest`) 잡이 추가되나, 외부 크레이트 의존성이 0개이므로 클린 빌드와 테스트가 수십 초 내에 완료됩니다.
- **Alternatives rejected**:
  - 리본 레이아웃을 단일 고정 문자열로만 두는 방식: 글로벌 오피스 사용자와 공공·행정 한글(HWP/HWPX) 문서 사용자의 메뉴 탐색 습관을 동시에 만족시키기 위해 실시간 전환 스위처 채택.
- **Verified by**: `cargo test -p easyoffice-layout -p easyoffice-app`, `cargo run -p easyoffice-app -- --self-test`, `swift test --filter testNinthAudit`, `node scripts/check-repo-hygiene.mjs`
- **Revisit when**: 사용자 정의 리본 단축키 프로필 내보내기/가져오기 기능을 추가할 때.

### 29. 순수 Rust RFC 1951 DEFLATE·HWP 5.0 압축 스트림 코덱, Swift/Rust 교차 엔진 바이너리 호환성 및 Tauri WebUI 7탭 리본·일괄 변환 스튜디오 정책 (2026-10-24)

- **Superseded by PD-094 (부분)**: 이 항목 중 Swift 앱·`swift build`/`swift test`·`scripts/package-macos-release.sh`·Swift 대응 규칙에 해당하는 부분은 PD-094로 대체되었습니다. Rust 엔진·WebUI·Windows 관련 결정은 그대로 유효합니다.
- **Decision (`PD-055`)**:
  1. **순수 Rust `std` RFC 1951 DEFLATE 압축·해제 엔진 및 PKZIP `METHOD_DEFLATE (8)` 통합 (`crates/easyoffice-formats/src/{deflate.rs,zip_codec.rs,lib.rs}`)**:
     - 외부 크레이트 의존성이나 `unsafe` 블록 없이 `crates/easyoffice-formats/src/deflate.rs`에 RFC 1951 비압축(`BTYPE=00`)·고정 허프만(`BTYPE=01`)·동적 허프만(`BTYPE=10`) 인플레이터(`inflate_raw`), RFC 1950 2바이트 zlib 래퍼 자동 감지 인플레이터(`inflate_auto`), 해시 체인 LZ77 + 블록별 동적 허프만 디플레이터(`deflate_raw`, `deflate/compress.rs`; 블록마다 저장·고정·동적 중 비트 비용이 가장 작은 형식을 고르며 miniz_oxide의 빠른 레벨 설계를 참고해 안전한 Rust로 다시 구현, 고지는 `THIRD_PARTY_NOTICES.md`)를 구현합니다.
     - 단일 엔트리 해제 예산(`MAX_SINGLE_ENTRY_UNCOMPRESSED_BYTES = 128 MiB`) 또는 누적 해제 예산(`MAX_TOTAL_UNCOMPRESSED_BYTES = 512 MiB`)을 초과하는 스트림은 즉시 `EasyOfficeError::ResourceBudgetExceeded`로 거부(Fail-Closed)합니다.
     - `ZipArchiveCodec`은 `pack_deflate`와 `unpack`에서 `METHOD_STORE (0)` 및 `METHOD_DEFLATE (8)`을 모두 처리하고 원본 바이트에 대한 IEEE 802.3 CRC-32를 검증하며, `FormatDetector::detect`는 Deflate 압축된 `.hwtx`(`Contents/header.xml` `isTemplate="1"`)와 `.ppsx`(`presentationml.slideshow` / `slideshow="1"`) 아카이브를 압축 해제하여 정확히 식별합니다.
  2. **HWP 5.0 압축 스트림(`flags = 0x01`) 및 Swift/Rust 교차 엔진 OLE2 바이너리 호환성 (`crates/easyoffice-formats/src/{ole2_codec.rs,legacy_ole2_codec.rs}`)**:
     - `Hwp5Codec::pack_compressed`는 `FileHeader` 오프셋 36의 `flags = 0x01` 비트를 설정하고 `DocInfo`와 `BodyText/Section0` 스트림을 `deflate_raw`로 압축하며, `Hwp5Codec::unpack`은 `flags & 0x01 != 0`일 때 `inflate_auto`로 스트림을 복원합니다.
     - Swift 코어(`OLE2AndHWP5Codec.swift`, `LegacyOLE2OfficeCodec.swift`)가 생성한 확장 HWP 5.0 레코드(`HWPTAG_DOCUMENT_PROPERTIES = 16`, `HWPTAG_PARA_HEADER = 66`, `HWPTAG_TABLE = 77`) 및 레거시 OLE2 컨테이너(`.doc` 확장 트레일러, `.xls` `BIFF8_FORMULA` 오프셋 16 UTF-16LE 수식, `.ppt` `0x0040` 바이너리 캔버스 아톰)를 손실 없이 디코딩합니다.
  3. **Tauri v2 데스크톱 WebUI 실사용 마이크로카피 정화, 7개 리본 카테고리 도구 전환 및 20종 포맷 일괄 변환 스튜디오 (`crates/easyoffice-app/src/lib.rs`, `crates/easyoffice-app/ui/*`)**:
     - `EasyOfficeIpcRouter::dispatch`에 `convert_document_format`, `get_ribbon_category_commands`, `evaluate_workbook_formula`를 추가합니다.
     - WebUI(`index.html`, `styles.css`, `app.js`)에서 내부 엔지니어링 지표를 제거하고, 7개 리본 카테고리(`홈`, `삽입`, `레이아웃`, `모듈 도구`, `검토`, `보기`, `생산성·보안`) 동적 도구 전환, 문서 표 편집기(`+행 추가`, `셀 병합 토글`, `=SUM(ABOVE)`) 및 200자 원고지 격자 뷰, 다중 시트 탭 바(`Sheet1`, `Sheet2`, `+ 시트 추가`, `=Sheet2!A1+50`), 20종 포맷 일괄 변환 스튜디오(`#pane-converter`), 하단 상태바 보기 모드(`편집`/`열람`/`발표`) 및 빠른 배율(`80%`/`100%`/`125%`) 컨트롤을 연결합니다.
- **Why**: 상용 오피스 도구와 HWP 5.0이 기본 생성하는 RFC 1951 Deflate 압축 컨테이너와 Swift/Rust 양대 엔진 간 바이너리 호환성을 완비하고, Tauri 데스크톱 UI의 모든 리본 카테고리와 표·원고지·다중 시트·일괄 변환 워크플로를 실제 동작하도록 완성하기 위함입니다.
- **Trade-offs**: 순수 Rust `std`로 허프만 트리와 LZ77 매처를 직접 유지 관리해야 하나, 외부 크레이트 0개 원칙을 지키면서 약 550줄의 간결한 모듈(`deflate.rs`)로 3가지 RFC 1951 블록 타입과 압축 폭탄 방어를 모두 충족합니다.
- **Alternatives rejected**:
  - 외부 `flate2`/`miniz_oxide` 크레이트 도입: 오프라인/폐쇄망 빌드 무결성과 `Cargo.toml` 외부 의존성 0개 원칙에 위배되므로 기각.
- **Verified by**: `cargo test --workspace`, `cargo run -p easyoffice-app -- --self-test`, `node scripts/check-repo-hygiene.mjs`
- **Revisit when**: 64비트 대용량 아카이브용 ZIP64 확장 헤더 지원이 필요할 때.

### 30. Rust 비트·워드 수준 코어 엔진 최적화(Slicing-by-8 CRC32·9비트 LUT Huffman·SWAR UTF-16LE·Bitset DAG 시트) 및 Tauri 2.x WebUI 완결 정책 (2026-10-24)

- **Superseded by PD-094 (부분)**: 이 항목 중 Swift 앱·`swift build`/`swift test`·`scripts/package-macos-release.sh`·Swift 대응 규칙에 해당하는 부분은 PD-094로 대체되었습니다. Rust 엔진·WebUI·Windows 관련 결정은 그대로 유효합니다.
- **Decision (`PD-056`)**:
  1. **`crates/easyoffice-formats` 비트·워드 수준 코덱 최적화 및 RFC 1950 Adler-32 Fail-Closed 검증 (`zip_codec.rs`, `deflate.rs`, `rtf_xml_helpers.rs`, `ole2_codec.rs`, `legacy_ole2_codec.rs`)**:
     - `IeeeCrc32`에 컴파일 타임 생성 `TABLES: [[u32; 256]; 8]` Slicing-by-8 테이블을 도입하여 8바이트 스트라이드로 IEEE 802.3 CRC-32를 처리합니다(1 MiB 스트림 기준 스칼라 1바이트 테이블 대비 `2,068µs -> 464µs`, `4.46x` 가속).
     - `deflate.rs`에 64비트 워드 리필 `BitReader::fill` 및 `byte_pos()`, 하드웨어 `u16::reverse_bits`, 9비트 1단계 `fast_lut: [u16; 512]` 고정(`OnceLock` 캐시)·동적 허프만 디코더, 비압축 블록 슬라이스 복사(`copy_aligned_bytes`), LZ77 `distance == 1` 바이트 반복(`out.resize`) 및 비겹침(`distance >= match_len`, `out.extend_from_within`) 고속 경로, 8바이트 `u64` SWAR XOR + `trailing_zeros() >> 3` 매칭, $O(1)$ `LENGTH_CODE_LUT: [u8; 259]`·`DIST_CODE_LUT: [u8; 512]`, RFC 1950 `NMAX = 5552` 지연 모듈로 `adler32` 및 `deflate_zlib`·`inflate_zlib`·`inflate_raw_with_consumed`를 구현하고, `inflate_auto`에서 유효한 Zlib 헤더 뒤 4바이트 Adler-32 트레일러 불일치 시 raw DEFLATE로 폴백하지 않고 `EasyOfficeError::CorruptedContainer`로 즉시 거부(Fail-Closed)합니다(64 KiB OOXML 1:1 공정 왕복 압축·해제 기준 `234µs -> 59µs`, `3.97x` 가속).
     - `rtf_xml_helpers.rs`에 64비트 SWAR ASCII 배치 경로(`u64 & 0xFF80_FF80_FF80_FF80 == 0`) 기반 `decode_utf16le_swar`·`encode_utf16le_swar`, 단일 패스 `memchr` 벌크 복사 엔티티 디코더 `unescape_xml_single_pass`, 1비트=1섹터 `SectorBitset`(`Vec<u64>`, `Ole2SecurityGuard` 및 `Ole2ContainerCodec` FAT 체인 $O(1)$ 순환 탐지)을 구현하고, `LegacyOle2Codec::pack_xls`에 `HashMap<String, u32>` 기반 $O(1)$ BIFF8 공유 문자열 테이블(SST) 인덱싱을 적용합니다(UTF-16LE + XML 언이스케이프 기준 `50µs -> 29µs`, `1.72x` 가속).
  2. **`crates/easyoffice-sheet` 제로할당 토크나이저·사전 컴파일 AST·비트셋 DAG 평가기 및 16비트 `sheet_id` 충돌 해소 (`parser.rs`, `lib.rs`, `workbook.rs`)**:
     - `tokenize_formula`에서 `Vec<char>` 할당을 제거하고 UTF-8 바이트 슬라이스 스캐너로 전환하며, `SheetEngine::set_cell` 시점에 `compiled_asts`로 AST를 1회 사전 컴파일합니다.
     - `EvalContext`의 방문 집합과 메모이제이션 키를 문자열 대신 `PackedWorkbookCoord(u64)`(`(sheet_id << 48) | ((row & 0xFF_FFFF) << 24) | (col & 0xFF_FFFF)`)와 1,024셀 인라인 비트셋 `VisitingTracker`(`bits: [u64; 16]`, `stack: [u64; 66]`)로 교체하고, `WorkbookEngine::add_sheet`에서 16비트 FNV-1a 폴딩 `sheet_id` 충돌 시 비충돌 ID로 결정론적 재할당(`set_sheet_id`)하여 거짓 `#CYCLE!`을 원천 방지합니다(500셀 × 4회 재계산 시 공유 `HashMap` 메모이제이션 베이스라인 대비 `878µs -> 308µs`, `2.85x` 가속).
  3. **결정론적 1:1 공정 벤치마크 하니스, Tauri 2.x WebUI 및 macOS SwiftUI 탈박스(Zero-Box De-Slop) 평면 디자인과 저장소 위생 가드 (`crates/easyoffice-app/src/{benchmark.rs,lib.rs,ffi.rs}`, `crates/easyoffice-app/ui/*`, `Sources/EasyOfficeApp/*.swift`, `Sources/EasyOfficeCore/WorkspaceHub.swift`, `scripts/check-repo-hygiene.mjs`)**:
     - `crates/easyoffice-app/src/benchmark.rs`에서 모든 베이스라인과 최적화 경로가 동일한 작업량과 알고리즘 복잡도($O(N)$ vs $O(N)$)로 경쟁하도록 구성하고 `--self-test` 실행 시 4대 스위트 동등성(`all_equivalent == true`)과 `p50` 배속 요약을 검증합니다.
     - Tauri WebUI(`index.html`, `styles.css`, `app.js`)에서 140개 정본 리본 명령 ID 동기화, `#btn-smart-diagnose`(`run_smart_assistant`) 연결, `⌘K`/`Ctrl+K` 명령 팔레트 단축키 복원, `#btn-scrub-pii` 실행 시 `#hwp-manuscript-preview` 보존을 완결하고, 카드-인-카드 테두리/반경(`border:|border-radius:|box-shadow:` 47개 → 18개) 및 내부 게이트 문구를 제거합니다.
     - macOS SwiftUI(`Sources/EasyOfficeApp/*.swift`)에서 버튼·입력창·메뉴·단락·표·인스펙터 행을 감싸던 알약·카드 테두리 프리미티브를 `617`개에서 `27`개(`-95.6%`)로 감축하고, `FlatToolbarMenu`·`FlatUnderlineTextField`·`FlatToggleControl`·`FlatStepperControl`과 `1pt` 구분선 기반 평면 오피스 레이아웃으로 재설계하며, 시트 데이터 막대 기본 비활성화(`isDataBarsEnabled = false`)·하단 피벗/차트 분석 서랍 기본 숨김(`showAnalyticsDrawer = false`)·`closeOtherTabs(keeping:)` 고정 탭 보존을 적용한 뒤 `scripts/check-repo-hygiene.mjs`에 구조적 상한 가드로 고정합니다.
- **Why**: 외부 크레이트나 `unsafe` 코드 없이 순수 Rust `std` 비트·워드 연산과 캐시 친화적 자료구조만으로 컨테이너 압축·해제, 문자열·XML 파싱, 스프레드시트 재계산 성능을 공정하게 가속하고, macOS SwiftUI와 Tauri 2.x 데스크톱 WebUI 양쪽에서 중첩 박스와 내부 게이트 문구를 제거한 평면 상용 오피스 디자인 일관성을 보장하기 위함입니다.
- **Trade-offs**: 벤치마크 전용 스칼라·비컴파일 베이스라인 함수를 `benchmark.rs` 내에 분리 유지해야 하나, 프로덕션 `easyoffice-sheet` 코어를 오염시키지 않으면서 `run_self_test()`에서 매 실행마다 최적화 경로와 베이스라인 경로의 출력 일치(`eq=true`)를 결정론적으로 증명합니다.
- **Alternatives rejected**:
  - 플랫폼 종속 SIMD 인트린직(`std::arch`) 및 `unsafe` 포인터 캐스팅 사용은 메모리 안전성 불변식과 크로스플랫폼 이식성을 해치므로 안전한 `u64` SWAR 및 컴파일 타임 LUT 방식으로 구현.
- **Verified by**: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo run --release -p easyoffice-app -- --self-test`, `swift test`, `node scripts/check-repo-hygiene.mjs`
- **Revisit when**: 멀티코어 워커 스레드 풀 기반 대규모 워크북 병렬 재계산 스케줄러를 도입할 때.

### 31. 64비트 SWAR XML·CSV 바이트 스캐너, 무할당 스트리밍 시트 범위 집계 및 데스크톱 워크스페이스 UI/UX 무결성 완결 정책 (2026-10-24)

- **Status**: Superseded (이전: Implemented)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
- **Decision (`PD-057`)**:
  1. **`Sources/EasyOfficeCore` 64비트 SWAR 바이트 스캐너 및 4-Way 언롤 시트 집계 (`XMLSanitizer.swift`, `TabularAndMarkupCodec.swift`, `SheetBuiltInFunctions.swift`, `SheetAnalyticsAndPivot.swift`)**
     - `XMLSanitizer.scanUnsafeXMLBuffer`와 `XMLSanitizer.escape`에서 64비트 워드(`UInt64`) 단위 SWAR 제로바이트 마스크(`0x0101_0101_0101_0101`, `0x8080_8080_8080_8080`)로 안전 구간을 8바이트씩 건너뛰고 메타문자가 없는 입력은 무할당으로 즉시 반환하며, 이스케이프 대상이 있는 경우에만 연속 슬라이스 단위로 복사합니다.
     - `CSVStateMachineCodec.parseRows`에서 `Array(text.utf8)` 전체 복사를 제거하고 `withUTF8` 연속 버퍼 위에서 비인용 필드 내부를 64비트 SWAR 워드 단위로 스캔하여 구분자, 따옴표, 개행 바이트가 없는 8바이트 구간을 한 번에 전진시킵니다.
     - `WorksheetModel.evaluateMathAndTrigFunction`(`SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT`, `COUNTA`, `COUNTBLANK`)과 `PivotTableEngine.aggregate`에서 중간 `compactMap`·`filter` 배열 할당을 제거하고 4-way 루프 언롤링(`s0, s1, s2, s3`) 단일 패스 스트리밍 누산으로 교체합니다.
  2. **macOS SwiftUI 워크스페이스 12차 적대적 시각 감사 결함 전수 해소 및 상한 가드 강화 (`Sources/EasyOfficeApp/*.swift`, `Sources/EasyOfficeCore/{WorkspaceCatalogAndModels.swift,SlideEngine.swift}`, `scripts/check-repo-hygiene.mjs`)**
     - 상단 문서 탭 바(`TopDocumentTabStripView`)의 6글자 강제 절단(`shortTabTitle`)을 제거하고 전체 파일명을 `.truncationMode(.middle)`과 최대 너비 `188pt`로 표시합니다.
     - 좌측 탐색기 헤더의 한영 혼용 배지(`DOC 탐색` 등)를 순수 한국어 명칭(`문서 탐색`, `한글 문서 탐색`, `시트 탐색`, `슬라이드 탐색`, `PDF 탐색`)으로 통일하고, 토글·시트 버튼·표 헤더·눈금자·슬라이드 썸네일의 보라색 박스 배경(`selectedSurface`)을 제거합니다.
     - 시트 뷰포트(`SheetEditorDetailPaneView`) 기본 그리드 범위를 `22행 × 10열(A..J)`로 확장하여 캔버스 하단 공백을 제거하고, 단일 셀 선택 시 수식 입력줄의 중복 주소 표기를 숨기며, 샘플 문서·슬라이드·PDF 모델을 `WorkspaceCatalogAndModels` 정본에서 직접 정의해 렌더링 시점 문자열 치환 없이 100% 데이터 기반으로 표시합니다.
     - 슬라이드 캔버스 도형(`SlideshowAndPDFCanvasOverlays`)의 좌측 강조선이 외곽 테두리와 겹치지 않도록 내부 패딩 안쪽으로 배치하고, `SlideModel.duplicateElement` 실행 시 수평 여유 공간이 있으면 2열 나란히 배치(`elem.x + elem.width + 24pt`)하며 기본 과제 카드를 `2분기 실행 지표`로 분화합니다.
     - PDF 스튜디오(`SlideAndPDFDetailPanes`)의 가로형 단문 박스를 A4 세로형 문서 용지(`maxWidth: 620, minHeight: 720`, 파일명·쪽 번호 상단/하단 메타데이터, `page.extractedText`의 `|` 구분 표 행 및 장/절 소제목 100% 데이터 기반 렌더링)로 재설계하고, 우측 인스펙터(`RightContextInspectorView`) 하단 실행 버튼을 수직 행 구조와 우측 셰브론(`chevron.right`) 어포던스로 정렬합니다.
     - `Sources/EasyOfficeApp/*.swift` 전체의 박스·스트로크 프리미티브 수를 `11`개(`<= 12`)로 감축하고 `scripts/check-repo-hygiene.mjs`의 상한을 SwiftUI `12`, WebUI CSS `18`로 고정하며 자기홍보·게이트 문구 차단 목록을 확장합니다.
- **Why**: XML·CSV 바이트 파서와 시트 범위 집계의 불필요한 중간 배열 할당 및 바이트별 분기를 줄이고, macOS 네이티브 워크스페이스 5대 화면의 잔여 시각 결함과 자기홍보 문구를 제거하기 위함입니다.
- **Trade-offs**: 64비트 SWAR 루프와 8바이트 미만 꼬리(tail) 루프를 함께 유지해야 하나, 분기 예측 실패와 힙 할당을 줄이면서 모든 경계 길이에 대해 바이트 단위 정확성을 보존합니다.
- **Alternatives rejected**:
  - `Foundation.XMLParser`나 정규식 기반 치환 도입은 전체 문자열 복사와 객채 할당 비용이 크므로 기각.
- **Verified by**: `swift test`, `node scripts/check-repo-hygiene.mjs`
- **Revisit when**: 대용량 CSV·XML 스트림의 멀티스레드 청크 분할 파서를 도입할 때.

### 32. 무할당 조건부 범위 집계·64비트 SWAR XML 언이스케이프 및 데스크톱 수식 조판·인스펙터 현지화 완결 정책 (2026-10-25)

- **Status**: Superseded (이전: Implemented)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
- **Decision (`PD-058`)**:
  1. **무할당 스트리밍 시트 범위 집계 및 64비트 SWAR XML 언이스케이프 (`SheetBuiltInFunctions.swift`, `XMLSanitizer.swift`, `TabularAndMarkupCodec.swift`)**
     - `EvalFrame`에 `streamNumericStats(in:)`와 `streamCountStats(in:)`를 도입하여 `SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT`, `COUNTA`, `COUNTBLANK`, `PRODUCT` 계산 시 `[EvaluatedCellValue]`와 `[Double]` 배열 할당 없이 행·열 좌표를 직접 순회하고 4-way 루프 언롤 누산기(`s0, s1, s2, s3`)로 합계를 계산합니다.
     - `XMLSanitizer.unescape`는 `withUTF8` 연속 바이트 버퍼에서 64비트 SWAR 제로바이트 마스크(`zeroByteMask(word ^ ampMask)`)로 `&` 바이트를 8바이트 단위로 탐지하고, 엔티티가 없는 구간을 슬라이스 단위로 일괄 복사하며 `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`, `&#...;`, `&#x...;`를 바이트 수준에서 디코딩합니다.
     - `CSVStateMachineCodec.encodeWorksheet`는 2차원 문자열 그리드(`[[String]]`)와 행 배열(`[String]`) 할당을 제거하고 사전 예약된 단일 문자열 버퍼에 직접 직렬화합니다.
  2. **수식 조판 파서, 샘플 문서 실무 표, 시트 인스펙터 한국어 표시명 및 일괄 변환 파이프라인 요약 (`DocumentSubmodelsAndNotes.swift`, `WorkspaceCatalogAndModels.swift`, `SheetCoordinatesAndAST.swift`, `WorkspaceHubSheetAndPDFActions.swift`, `RightContextInspectorView.swift`, `WorkspaceViews.swift`, `SlideAndPDFDetailPanes.swift`, `HomeHubAndSmartViews.swift`, `scripts/check-repo-hygiene.mjs`)**
     - `DocumentEquationBlock.formatMathSymbols`는 중첩 중괄호 파서를 통해 `\frac{num}{den}`, `\sqrt{...}`, `_{...}`, `^{...}`를 유니코드 분수·근호·첨자 표기로 변환하여 원시 LaTeX 토큰이 화면에 노출되지 않도록 합니다.
     - 샘플 `.docx` 문서의 내부 엔진 지연시간 표를 전사 부문별 문서 표준화 지표 표로 교체하고, 머리말과 리비전·주석 작성자 명칭을 실무 부서명으로 정리하며, 내부 엔진 용어를 `scripts/check-repo-hygiene.mjs` 차단 목록에 등록합니다.
     - `CellNumberFormat`과 `CellHorizontalAlignment`는 직렬화 호환을 위해 `rawValue`를 유지하면서 한국어 `displayTitle`을 제공하고, 우측 인스펙터의 범위 통계(`합계 · 평균 · 개수`, `최소 · 최대`)와 PDF 쪽 번호·제목 추출(`제N쪽`, 본문 첫 줄 제목 추출)을 한국어 기준으로 통일합니다.
     - `BatchConversionStudioHubView` 하단에 `1pt` 구분선 기반 2단 요약 영역(`변환 파이프라인 및 보존 규격`, `포맷 패밀리별 변환 검증 현황`)을 추가하여 테두리 박스 추가 없이 변환 규격과 출력 용량 정보를 표시합니다.
- **Why**: 범위 수식 평가와 XML·CSV 직렬화 경로의 중간 힙 할당을 제거하고, macOS 워크스페이스 전 화면에서 원시 LaTeX 구문, 영문 enum 식별자, 내부 엔진 용어를 제거하기 위함입니다.
- **Trade-offs**: 수식 조판 파서가 중첩 중괄호 깊이를 직접 추적해야 하나, 외부 수식 렌더링 의존성 없이 선형 시간 안에 깔끔한 유니코드 조판 결과를 생성합니다.
- **Alternatives rejected**:
  - 뷰 레이어에서 문자열 치환으로 샘플 데이터를 가리는 방식은 데이터 모델과 화면 표시의 일관성을 해치므로 기각.
- **Verified by**: `swift test`, `node scripts/check-repo-hygiene.mjs`
- **Revisit when**: 다중 행 행렬(`\begin{matrix}`) 수식 블록 조판을 추가할 때.

### 33. 14차 적대적 감사 기반 무할당 조회·통계·와일드카드 커널, 컨테이너 무결성 하드닝 및 상용 오피스 UI/UX 완결 정책 (2026-10-25)

- **Status**: Superseded (이전: Implemented)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
- **Decision (`PD-059`)**:
  1. **무할당 조회(`INDEX`/`VLOOKUP`/`HLOOKUP`)·Welford 온라인 분산(`VAR`/`STDEV`)·무할당 다중 조건부 스트리밍(`SUMIFS`/`COUNTIFS`/`AVERAGEIFS`/`MINIFS`/`MAXIFS`)·투포인터 와일드카드 매처 (`SheetLookupTextAndFinancialFunctions.swift`, `SheetBuiltInFunctions.swift`, `SheetCoordinatesAndAST.swift`, `SheetEngine.swift`)**
     - `INDEX`는 `evaluateRangeGrid` 2차원 배열 할당 없이 목표 셀 `(startRow + rIdx, startCol + cIdx)` 단 1개만 $O(1)$로 평가하며, `VLOOKUP`·`HLOOKUP`은 탐색 열·행만 스트리밍 비교한 뒤 일치한 반환 셀 1개만 평가합니다.
     - `SUMIF`, `COUNTIF`, `AVERAGEIF`, `SUMIFS`, `COUNTIFS`, `AVERAGEIFS`, `MINIFS`, `MAXIFS`는 `[Bool]` 마스크 배열 할당을 제거하고 `streamMultiCriteriaStats` 단일 패스로 조건을 직접 판별·집계합니다.
     - `VAR`, `STDEV`, `VARP`, `STDEVP`, `SUMSQ`, `AVEDEV`, `GEOMEAN`, `HARMEAN` 및 논리 집계(`AND`, `OR`, `XOR`)는 `forEachResolvedCell` 스트리밍과 Welford 온라인 알고리즘으로 중간 `[Double]` 할당을 제거합니다.
     - `matchesWildcardPattern`은 `NSRegularExpression` 컴파일을 제거하고 `~*`, `~?`, `~~` 이스케이프와 대소문자 무시 비교를 지원하는 투포인터 백트래킹 루프로 교체합니다.
     - 수식 컴파일러에 입력 길이(`maxFormulaUTF8Bytes = 8_192`), 토큰 수(`maxFormulaTokenCount = 2_048`), 괄호·함수 중첩 깊이(`maxFormulaParseDepth = 64`) 상한과 `&`(문자열 결합)·`^`(거듭제곱) 연산자, 엄격한 타입 비교(`"1" = 1`은 `FALSE`, 빈 셀은 `""` 및 `0`과 동치), `1e-12` 부동소수점 반올림 보정, `IRR` 현금흐름 스케일 정규화, 1900 직렬 날짜 및 연·월·일 롤오버를 적용합니다.
  2. **ZIP·OLE2·XML·HWP5·PDF 컨테이너 보안 및 라운드트립 보존 강화 (`BinaryContainerCodec.swift`, `XMLSanitizer.swift`, `StandardZipOfficeCodecs.swift`, `ODFAndHWPXCodecs.swift`, `OLE2AndHWP5Codec.swift`, `PDFBinarySerializer.swift`, `SmartProductivityTools.swift`, `TabularAndMarkupCodec.swift`, `DocumentSubmodelsAndNotes.swift`)**
     - `ZipContainerCodec.unpack`은 디렉터리 엔트리(`/`)를 허용하되 중복 경로와 Local/Central 헤더 파일명 불일치 및 `:` 경로를 즉시 거부하며, OOXML·ODF·HWPX 코덱은 비표준 아카이브 엔트리를 `preservedArchiveEntries`에 보존하고 XML 검증을 `.xml`·`.rels`·`.hpf` 엔트리로 한정합니다.
     - `XMLSanitizer.escape`는 64비트 SWAR 고속 경로에서 XML 1.0 금지 C0 제어문자(`0x00..0x08`, `0x0B..0x0C`, `0x0E..0x1F`)를 검출·제거하고, `OLE2ContainerCodec.unpack`은 DIFAT 섹터 체인과 MiniStream/MiniFAT를 지원하며, `HWP5BinaryCodec`은 `maxHWP5Records = 65_536` 상한과 안전한 정수 클램핑을 적용합니다.
     - `TableCellSpan.maxSpan = 256`, `CSVStateMachineCodec.maxExportRows = 65_536` 상한을 적용하고, `PDFBinarySerializer`의 26줄 본문 절단을 해제하며, `DocumentSecurityAuditor`는 주민등록번호·운전면허·여권·신용카드·전화번호·이메일·사업자번호·IPv4·API 토큰 패턴을 문서·시트·슬라이드·PDF 전 페이로드에서 마스킹합니다.
  3. **macOS 워크스페이스 UI/UX 정밀 교정 (`SlideAndPDFDetailPanes.swift`, `RightContextInspectorView.swift`, `WorkspaceViews.swift`, `EditorDetailPanes.swift`, `HomeHubAndSmartViews.swift`, `EasyOfficeApp.swift`, `WorkspaceHub.swift`, `WorkspaceRibbonCommandRouter.swift`)**
     - 슬라이드 상단 툴바의 간격과 레이블을 압축하여 `1440x900` 해상도에서 `쇼 시작 ▶` 버튼 잘림을 해소하고, 드래그 종료·정렬·선택 시 `activeSnapGuides`를 즉시 초기화하여 유령 스냅 가이드라인을 제거합니다.
     - 우측 인스펙터의 다단 인라인 버튼에서 불필요한 우측 셰브론(`>`)을 제거하고, HWPX 인스펙터 빠른 실행 명령 접두사(`hwp-*`), PDF 펜 색상 한글 명칭(`라벤더`, `적색`, `흑색`), PDF 레이아웃 명령 분기, 용지 방향 순수 한글 표기(`세로`, `가로`), 스마트 보조 도구 전체 제안 노출을 적용합니다.
     - 수정된 탭 닫기 시 인라인 저장 확인 바(`requestCloseTab`)와 최근 닫은 탭 복원(`reopenLastClosedTab`, `⇧⌘T`), 이종 포맷 내보내기 시 활성 탭 페이로드 보존, 텍스트 입력 포커스 시 네이티브 `⌘C`/`⌘V` 전달을 보장합니다.
- **Why**: 스프레드시트 조회·통계·조건부 집계 함수의 힙 할당과 정규식 오버헤드를 제거하고, 컨테이너 파서의 경계 조건 및 macOS 워크스페이스의 사용성 결함을 근본 원인으로 해결하기 위함입니다.
- **Trade-offs**: 컨테이너 모델에 `preservedArchiveEntries` 사전을 유지해야 하나, 외부 오피스 도구에서 생성한 테마·확장 메타데이터 스트림을 손실 없이 라운드트립 보존할 수 있습니다.
- **Alternatives rejected**:
  - 와일드카드 매칭에 정규식 캐시를 두는 방식은 패턴마다 여전히 객체 할당과 정규식 엔진 호출 비용이 발생하므로 무할당 투포인터 스캐너로 대체.
- **Verified by**: `swift build`, `swift test`, `node scripts/check-repo-hygiene.mjs`
- **Revisit when**: 외부 수식 플러그인 인터페이스 또는 스트리밍 대용량 XLSX/ODS 청크 파서를 확장할 때.

### 34. Swift 코어 수식 문자열 상한, 희소 시트 RLE 직렬화, OLE2 MiniFAT 격리, Codable 좌표 불변식 및 찾기·바꾸기 정합성 정책 (2026-10-25)

- **Status**: Superseded (이전: Implemented)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
- **Decision (`PD-025` / `PD-059`)**:
  1. **ECMA-376 셀 문자열 상한(`maxCellTextLength = 32_767`) 및 이중 따옴표(`""`) 이스케이프 파싱 (`SheetEngine.swift`, `SheetLookupTextAndFinancialFunctions.swift`, `SheetCoordinatesAndAST.swift`)**
     - `SheetEngine.maxCellTextLength = 32_767`(ECMA-376 Part 1 §18.18.11)을 단일 진실 원천으로 정의하고, `&` 연산자, `CONCAT`/`CONCATENATE`, `TEXTJOIN`, `SUBSTITUTE`, `REPLACE` 및 `evaluateInternal`/`evaluateOnSheet` 평가 경로에서 문자열 길이가 `32,767`자를 초과하기 전에 즉시 `#VALUE!`(`.error(.value)`)로 거부하여 체인 수식에 의한 지수적 문자열 증폭을 차단합니다.
     - 수식 문자열 리터럴 파서(`parseQuotedStringLiteral`)는 ECMA-376 및 OASIS OpenFormula 규격에 따라 내부 이중 따옴표(`""`)를 단일 `"`로 언이스케이프하고, 이스케이프되지 않은 내부 따옴표나 닫히지 않은 문자열 리터럴을 `#VALUE!`로 거부합니다.
  2. **희소 시트 ODS RLE 직렬화 및 CSV·문서 변환 밀집 그리드 예산(`maxDenseExportCells = 200_000`) (`ODFAndHWPXCodecs.swift`, `TabularAndMarkupCodec.swift`, `SmartProductivityTools.swift`, `FormatDetector.swift`)**
     - `ODFContainerCodec.encodeWorksheet`는 `0...maxRow` 및 `0...maxCol` 밀집 루프를 제거하고 실제 데이터가 존재하는 행·열 좌표만 정렬 순회하며, 빈 행·열 간격을 OASIS OpenDocument 1.3 `table:number-rows-repeated` 및 `table:number-columns-repeated` 속성으로 RLE 압축합니다.
     - `CSVStateMachineCodec.encodeWorksheet`와 `DocumentConversionOrchestrator.extractGridRows`는 `(maxRow + 1) * (maxCol + 1)`을 `multipliedReportingOverflow`로 계산하여 `CSVStateMachineCodec.maxDenseExportCells = 200_000`을 초과하면 조용한 셀 누락이나 메모리 고갈 대신 `EasyOfficeError.resourceExhausted`로 즉시 거부(Fail-Closed)하며, PDF 시트 변환(`convertPayloadToPDF`)은 희소 렌더러(`PDFStudioDocumentModel.fromWorksheet`)를 직접 호출합니다.
  3. **OLE2 Root Entry 64비트 스트림 크기 검증 및 컨테이너 전역 MiniFAT 섹터 독점 보장 (`OLE2AndHWP5Codec.swift`)**
     - `OLE2ContainerCodec.unpack`은 루트 엔트리의 64비트 `rawRootStreamSize`를 `Int`로 변환하기 전에 `UInt64(maxTotalExtractedBytes)`(`64 MiB`) 이하인지 먼저 검증하여 부호 있는 64비트 정수 오버플로를 차단합니다.
     - `claimedMiniSectors: Set<UInt32>`를 스트림 루프 바깥에서 컨테이너 단위로 유지하여 서로 다른 미니스트림이 동일 MiniFAT 섹터를 공유하거나 순환 참조하는 악성 컨테이너를 `EasyOfficeError.malformedContainer`로 거부합니다.
  4. **ECMA-376 7자리 행 참조 이동·`OFFSET` 전체 그리드 지원, `Codable` 좌표 불변식 및 찾기·바꾸기 범위 통일 (`SheetAutoFillAndLayout.swift`, `SheetLookupTextAndFinancialFunctions.swift`, `SheetCoordinatesAndAST.swift`, `DocumentSubmodelsAndNotes.swift`, `WorkspaceRibbonCommandRouter.swift`, `WorkspaceHubSheetAndPDFActions.swift`)**
     - `parseRawCellRef`의 행 숫자 자릿수 상한을 5자리에서 7자리(`1..1,048,576`)로 확장하고, `resolveOffsetRange`의 행·열 경계를 `CellAddress.maxZeroBasedRow`(`1,048,575`) 및 `CellAddress.maxZeroBasedColumn`(`16,383`)으로 통일합니다.
     - `PackedCellCoordinate`, `CellAddress`, `CellRange`, `TableCellSpan`에 커스텀 `init(from decoder: Decoder) throws`를 구현하여 역직렬화 시점에 ECMA-376 좌표 범위, `CellRange.maxExpandedCellCount`(`100,000`), `TableCellSpan.maxSpan`(`256`) 불변식을 강제합니다.
     - `replaceAllMatches`의 중복 `inlineRuns` 치환 루프를 제거하고, `findMatchCount`·`navigateSelectionToFindMatch`·`replaceCurrentMatch`가 문서 표(`doc.tables`) 셀과 슬라이드 부제목(`slide.subtitle`)을 일관되게 순회하도록 통일합니다.
- **Why**: 수식 평가, 희소 시트 직렬화, OLE2 미니스트림 파싱, JSON 역직렬화 경계의 메모리 증폭 및 불변식 우회 경로를 원천 차단하고, 전체 그리드 좌표계와 찾기·바꾸기 상태 머신의 정합성을 보장하기 위함입니다.
- **Trade-offs**: `CSVStateMachineCodec.encodeWorksheet`가 `throws` 함수가 되어 호출부에서 오류를 전파해야 하나, 원거리 단일 셀(`XFD1048576`)이 포함된 시트를 내보낼 때 조용한 데이터 유실이나 다중 기가바이트 할당을 결정론적으로 방지합니다.
- **Alternatives rejected**:
  - `CSVStateMachineCodec`에서 `65,536 x 1,024` 바깥 셀을 무시하고 조용히 잘라내는 방식은 사용자 데이터를 침묵 속에 유실하므로 기각.
- **Verified by**: `swift build`, `swift test`, `node scripts/check-repo-hygiene.mjs`
- **Revisit when**: 대용량 CSV 청크 스트리밍 익스포터를 별도 비동기 파일 파이프라인으로 분리할 때.

### 35. 15차 적대적 감사 기반 정수 변환·컨테이너 규격·시트 수식·PDF 페이지네이션·데이터 손실 방어 완결 정책 (2026-10-25)

- **Status**: Superseded (이전: Implemented)
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
- **Decision (`PD-060`)**:
  1. **Swift/Rust 수치 변환 및 고정폭 정수 오버플로 트랩 원천 차단 (`LegacyOLE2OfficeCodec.swift`, `TabularAndMarkupCodec.swift`, `OLE2AndHWP5Codec.swift`, `StandardZipOfficeCodecs.swift`, `SheetBuiltInFunctions.swift`, `SheetLookupTextAndFinancialFunctions.swift`, `SheetEngine.swift`, `DocumentSubmodelsAndNotes.swift`, `BinaryContainerCodec.swift`, `crates/**`)**
     - `.doc` FibBase, `.rtf`, `.hwp` 직렬화 시 부동소수점(`fontSizePt`, `lineSpacingPercent`, `widthPt`, `heightPt`, `letterSpacingPercent`)에서 `Int`/`UInt32`로의 직접 변환을 제거하고 `2^63` 경계 가드가 포함된 `safeClampedInt` 및 `safeClampedUInt32` 헬퍼로 통일합니다.
     - `.doc`와 `.hwp` 표 직렬화 시 행(`<= 4,096`)과 열(`<= 256`) 상한을 검증하고, `.xls` BIFF8 직렬화 시 시트 이름 길이(`<= 255` UTF-16 코드 유닛), 행(`0..<65,536`), 열(`0..<256`), 셀 문자열 길이(`<= 65,535`) 상한을 초과하면 조용한 절단 없이 `EasyOfficeError.corruptedContainer`로 거부합니다.
     - `ZipContainerCodec.pack`은 엔트리 수와 UTF-8 경로 바이트 길이가 `UInt16.max`(`65,535`)를 초과할 때 즉시 거부하며, `SheetBuiltInFunctions.safeInt` 및 Rust `checked_f64_to_i64`는 `[-2^63, 2^63)` 경계 검사를 결합하여 거대 배정밀도 실수에서 런타임 트랩 없이 `#NUM!`을 반환합니다.
  2. **OLE2 CFBF 계층 스토리지·MiniFAT·균형 디렉터리 트리 및 BIFF8·PPT·HWP5 표준 호환성 (`OLE2AndHWP5Codec.swift`, `LegacyOLE2OfficeCodec.swift`, `ole2_codec.rs`, `legacy_ole2_codec.rs`)**
     - `OLE2ContainerCodec.pack` 및 Rust `Ole2ContainerCodec::pack`은 `4,096`바이트 미만 스트림을 64바이트 MiniStream과 MiniFAT 섹터에 패킹하고, `"BodyText/Section0"`와 같은 계층 경로를 `Storage`(`objectType = 1`)와 자식 `Stream`(`objectType = 2`) 엔트리 및 균형 이진 탐색 트리(`leftSiblingID`, `rightSiblingID`, `childID`)로 직렬화합니다.
     - `OLE2ContainerCodec.unpack`은 `Storage -> Child` 디렉터리 트리를 순회하여 계층 스트림 경로를 복원하고, `rawRootStreamSize` 경계 검사와 스트림 간 미니섹터 중복 참조(`claimedMiniSectors`) 탐지를 수행합니다.
     - `HWP5BinaryCodec` 및 `Hwp5Codec`은 `maxHWP5Records = 524_288`과 누적 표 셀 상한(`maxTotalHWP5TableCells = 65_536`)을 적용합니다.
     - `LegacyOLE2OfficeCodec.encodeBIFF8Worksheet` 및 `LegacyOle2Codec::pack_xls`는 표준 BIFF8 Workbook Globals 서브스트림(`BOF 0x0005` -> 1바이트 `cch` 및 `grbit=0x01`이 포함된 `BOUNDSHEET 0x0085` -> `EOF 0x000A`)과 Worksheet 서브스트림(`BOF 0x0010` -> `DIMENSIONS` -> 셀 레코드 -> `EOF 0x000A`)을 분리 출력하고 `lbPlyPos` 오프셋을 패치하며, 디코더는 `RK`(`0x027E`), `MULRK`(`0x00BD`), `FORMULA`+`STRING`(`0x0207`) 및 1바이트/2바이트 `cch` 포맷을 모두 지원합니다.
     - `LegacyOLE2OfficeCodec.decodePowerPointBinary` 및 `LegacyOle2Codec::unpack_ppt`는 `RT_TextBytesAtom`(`4008`) 8비트 텍스트 아톰과 중첩 컨테이너 레코드(`(verAndInst & 0x000F) == 0x000F`) 순회를 지원합니다.
  3. **PDF 다중 페이지 자동 분할 및 본문 접두사 보존 (`PDFBinarySerializer.swift`, `text_pdf_codecs.rs`)**
     - PDF 페이지당 본문 렌더링 범위를 `Y = 728..76`(`15pt` 줄간격 기준 페이지당 최대 42줄)으로 확장하고, 42줄을 초과하는 페이지는 자동으로 후속 페이지 객체로 분할하여 60줄 이상의 본문도 손실 없이 출력 및 라운드트립 디코딩합니다.
     - 비가시 메타데이터 라인(`[WM] `, `[FORM] `)은 `0 1 Td`·`0 2 Td` 오프셋에 기록하여 본문 문단(`0 0 Td` / `BT /F1 12 Tf 54`)이 `[WM] `, `[STAMP] `, `[FORM] `으로 시작하더라도 디코딩 시 삭제되지 않도록 보존합니다.
  4. **ZIP 미디어 파일명 중복 방지, 보존 아카이브 화이트리스트, UTF-16 XML 검사 및 한국어 PII 탐지 (`StandardZipOfficeCodecs.swift`, `SmartProductivityTools.swift`, `XMLSanitizer.swift`, `smart_tools.rs`)**
     - OOXML·ODF·HWPX 패킹 시 첨부 이미지 파일명이 충돌하면 고유 접미사(`_2`, `_3`)를 부여하고, `preservedArchiveEntries`는 안전한 경로 화이트리스트만 허용하며, 이종 포맷 변환 시 이전 포맷의 보존 엔트리를 비우고, 메타데이터 정리 시 `thumbnail`, `preview/`, `vbaproject`, `app.xml`, `settings.xml`을 함께 제거합니다.
     - `XMLSanitizer.validateSafeXMLData`는 UTF-16LE(`FF FE`) 및 UTF-16BE(`FE FF`) BOM XML 스트림을 디코딩하여 검사하고, 외부 엔티티나 원격 프로토콜(`SYSTEM`, `<!ENTITY`, `[`, `http://`, `https://`, `file://`, `ftp://`)이 없는 로컬 `PUBLIC` `<!DOCTYPE>` 선언은 허용합니다.
     - `DocumentSecurityAuditor` 및 `PiiScrubber`는 한글 조사와 결합된 개인정보도 누락 없이 마스킹하도록 ASCII `\b` 경계를 숫자·영문 부정 전후방 경계로 교체하고 무하이픈 13자리 주민등록번호 패턴을 포함합니다.
  5. **스프레드시트 수식 평가·조회 기본값·날짜 직렬값·정렬 수식 이동·피벗 빈 키 보존 (`SheetCoordinatesAndAST.swift`, `SheetEngine.swift`, `SheetBuiltInFunctions.swift`, `SheetLookupTextAndFinancialFunctions.swift`, `SheetAutoFillAndLayout.swift`, `SheetAnalyticsAndPivot.swift`, `crates/easyoffice-sheet/**`)**
     - 수식 파서와 평가기는 좌결합 이항 연산 체인을 반복 루프로 처리하여 200개 이상의 항을 가진 평탄한 수식이 재귀 깊이 상한에 걸리지 않도록 하고, `evaluateOnSheet`는 반환 시 자신이 추가한 `crossVisited` 키를 정리하여 300개 이상의 교차 시트 셀 참조 시 가짜 `#CYCLE!`이 발생하지 않도록 합니다.
     - `ROUND`·`ROUNDUP`·`ROUNDDOWN`은 고정 `1e-12` 보정 대신 ULP 비례 보정(`abs(scaled) * Double.ulpOfOne * 2.0`)을 사용하며, `EvaluatedCellValue.description`은 `1e15` 미만의 유한 실수를 과학적 표기법 없이 포맷합니다.
     - 표준 스프레드시트 규격에 맞춰 `VLOOKUP`·`HLOOKUP` 4번째 인자 기본값을 `TRUE`, `MATCH` 3번째 인자 기본값을 `1`로 설정하고, `DATE`·`EDATE`·`EOMONTH`·`WORKDAY`는 1900 직렬 수치(`DATE(2024,1,1) == 45292`, `DATE(1900,2,29) == 60`)를 반환합니다.
     - `sortRows`는 행 이동량만큼 상대 수식 참조를 자동 이동시키고, `PivotTableEngine`은 비어 있지 않은 레코드의 빈 행·열 키를 `"(빈 값)"`으로 집계하며 `.count` 집계 시 텍스트 셀도 개수에 포함합니다.
  6. **워크스페이스 미저장 탭 보호, CSV 전체 열 내보내기 및 WCAG AA 명암비 보장 (`WorkspaceHub.swift`, `WorkspaceViews.swift`, `EasyOfficeApp.swift`, `TabularAndMarkupCodec.swift`, `DesignTokens.swift`, `scripts/check-repo-hygiene.mjs`)**
     - `closeOtherTabs`는 수정된 탭(`isModified == true`)을 삭제하지 않고 `pendingCloseConfirmTabID`로 확인을 요청하며, `confirmCloseTab`과 저장 후 닫기 동작은 이종 포맷(PDF 등) 내보내기로 인해 원본 탭의 `isModified`가 해제되지 않은 경우 탭을 닫지 않습니다.
     - `EasyOfficeAppDelegate`는 미저장 탭이 있을 때 `applicationShouldTerminate`에서 앱 종료를 중단하고 저장 확인 배너를 표시하며, `application(_:open:)` 및 `.onOpenURL`로 외부 파일 열기를 연결합니다.
     - `CSVStateMachineCodec`의 내보내기 좌표 상한을 스프레드시트 전체 그리드(`1,048,576`행 × `16,384`열 `XFD`)까지 허용하고(밀집 셀 총량 `200,000`셀 가드 유지), `EasyOfficeDesignTokens`의 라이트·다크 모드 텍스트 및 모듈 강조색 토큰은 모든 배경 표면에서 WCAG 2.1 AA(`>= 4.5:1`) 명암비를 충족합니다.
- **Why**: 15차 적대적 코드리뷰 및 보안 감사에서 식별된 정수 변환 트랩, 컨테이너 규격 불일치, PDF 1페이지 절단, 한국어 PII 탐지 누락, 수식 엔진 표준 편차, 미저장 문서 손실 경로를 근본 원인으로 해결하기 위함입니다.
- **Trade-offs**: 날짜 함수(`DATE`, `EDATE`, `EOMONTH`, `WORKDAY`)가 문자열 대신 1900 직렬 수치를 반환함에 따라 기존 날짜 수식 단언문을 직렬 수치 기준으로 갱신해야 하나, 날짜 결과에 대한 사칙연산(`=DATE(2024,1,1)+7`)과 비교 연산이 표준 스프레드시트와 동일하게 동작합니다.
- **Alternatives rejected**:
  - 좌결합 수식에서 단순히 `maxFormulaRecursionDepth` 상수만 늘리는 방식은 더 긴 수식에서 실제 호출 스택 오버플로를 유발하므로 반복 루프 언롤링으로 해결.
- **Verified by**: `swift build`, `swift test`, `cargo test --workspace`, `node scripts/check-repo-hygiene.mjs`
- **Revisit when**: 다중 섹션(`Section1..N`) HWP 5.0 스트림 병합 또는 BIFF8 수식 바이트코드(`rgce`) 역컴파일러를 확장할 때.

### 36. 16차 적대적 감사 기반 64비트 SWAR UTF-16LE/CSV/XML 코덱·DEFLATE 32KB 링버퍼·다중 레코드 BIFF8 확장 메타데이터 및 크로스스택 UX 무결성 정책 (2026-10-25)

- **Status**: Implemented (`Sources/EasyOfficeCore/**`, `Sources/EasyOfficeApp/**`, `crates/**`)
- **Superseded by PD-094 (부분)**: 이 항목 중 Swift 앱·`swift build`/`swift test`·`scripts/package-macos-release.sh`·Swift 대응 규칙에 해당하는 부분은 PD-094로 대체되었습니다. Rust 엔진·WebUI·Windows 관련 결정은 그대로 유효합니다.
- **Decision (`PD-061`)**:
  1. **64비트 SWAR Morton 비트 인터리빙 UTF-16LE·CSV·XML 핫패스 및 OLE2 미니섹터 비트셋 최적화 (`BinaryContainerCodec.swift`, `XMLSanitizer.swift`, `OLE2AndHWP5Codec.swift`, `TabularAndMarkupCodec.swift`, `utf16_swar.rs`, `rtf_xml_helpers.rs`, `deflate.rs`, `ole2_codec.rs`)**
     - `BitPacking.appendUTF16LE`·`encodeUTF16LE` 및 Rust `utf16_swar::encode_utf16le_swar`는 8바이트 ASCII 워드(`(word & 0x8080_8080_8080_8080) == 0`)를 Morton 비트 인터리빙(`expandASCII4ToUTF16LE` / `expand_ascii4_to_utf16le_u64`)으로 분기 없이 16바이트 UTF-16LE로 확장하며, `appendLengthPrefixedUTF16LE` 및 `append_len_prefixed_utf16le`는 4바이트 길이 슬롯을 선예약 후 대상 버퍼에 직접 기록하고 백패치하여 중간 버퍼 할당을 제거합니다.
     - `XMLSanitizer.escape`는 이스케이프 문자 발견 시 SWAR 루프를 조기 탈출하지 않고 `matchBits.trailingZeroBitCount >> 3`만큼 전진하여 8바이트 SWAR 루프를 끝까지 유지하며, `CSVStateMachineCodec.parseRows`의 `.inQuotedField`에 64비트 SWAR 스캐닝을 적용합니다.
     - `OLE2ContainerCodec.unpack`의 `claimedMiniSectors`를 `Set<UInt32>` 해시 테이블에서 평탄한 `[UInt64]` 워드 비트셋(`claimedMiniWords`)으로 전환하고, `compareCFBNames` 및 `cmp_cfb_dir_names`는 UTF-16 길이를 우선 비교한 뒤 인라인 대소문자 폴딩을 수행합니다.
     - Rust `deflate_raw`는 `vec![-1i32; input.len()]` 대신 고정 크기 `WINDOW_SIZE = 32,768` 링버퍼(`[-1i32; WINDOW_SIZE]`)를 사용하여 입력 크기와 무관하게 해시 체인 메모리를 128KB로 제한하고, `decode_huffman_block`의 중첩 역참조(`1 < distance < match_len`)는 지수 배증(`extend_from_within`) 복사로 처리하며, `adler32`는 `NMAX = 5552` 청크 내부 루프를 8바이트 단위로 언롤링합니다.
  2. **수식·통계·재무·변환 엔진의 Swift/Rust 크로스스택 표준 호환성 및 BIFF8 청크 직렬화 (`LegacyOLE2OfficeCodec.swift`, `SheetLookupTextAndFinancialFunctions.swift`, `SheetAnalyticsAndPivot.swift`, `crates/easyoffice-sheet/**`, `crates/easyoffice-formats/**`)**
     - `LegacyOLE2OfficeCodec.encodeBIFF8Worksheet`는 `BIFF8ExtendedMetadataEnvelope`(`0x0868`)가 `65,535`바이트를 초과할 때 조용히 버리지 않고 다중 `0x0868` 레코드 청크로 분할 기록하며, `decodeBIFF8Worksheet`는 모든 `0x0868` 청크를 병합하여 복원합니다.
     - `WORKDAY` 및 `NETWORKDAYS`는 선택적 3번째 공휴일(`holidays`) 인자를 지원하고, `IRR` 및 `RATE`는 뉴턴-랩슨 미수렴 시 `[-0.99, 10.0]` 구간 25점 브래킷 이분법 폴백을 수행합니다.
     - Rust `easyoffice-sheet` 파서는 OpenFormula 대괄호 참조(`[.A1:.B2]` -> `A1:B2`), 과학적 표기법(`1.5E-3`), 에러 리터럴(`#N/A` 등)을 지원하며, `VLOOKUP`·`HLOOKUP`·`MATCH`의 에러 키 즉시 전파 및 `VLOOKUP`/`HLOOKUP` 4번째 인자 생략 시 `TRUE`(유사 일치) 기본값을 적용합니다.
     - Rust `Hwp5Codec::unpack`은 `HWPTAG_PARA_TEXT` 디코딩 시 8-WCHAR(16바이트) 인라인·확장 제어문자 스킵 상태 머신을 적용하고, `CommonMarkCodec`은 `MarkdownBlock::Table` 양방향 파싱·직렬화 라운드트립을 수행하며, `UniversalFormatConverter::encode`는 `.md` 및 `.pdf` 변환 시 문서 표(`model.tables`)를 함께 직렬화합니다.
  3. **macOS 네이티브 및 WebUI/Tauri 실사용자 워크스페이스 UX 완결성 (`WorkspaceHub.swift`, `WorkspaceRibbonCommandRouter.swift`, `Sources/EasyOfficeApp/**`, `crates/easyoffice-app/**`)**
     - `WorkspaceViewModel`은 다크 모드 전환(`usesDarkMode`, `toggleDarkMode`), 미저장 탭 닫기 확인 및 폐기(`requestCloseTab`, `confirmDiscardAndClosePendingTab`, `cancelCloseTab`), 디스크 파일 직접 일괄 변환 큐 등록 시 열린 탭 오염 방지(`enqueueFilesFromDiskIntoBatchConversion`), 리본 각주 삽입(`doc-insert-footnote`)을 제공합니다.
     - `SheetEditorDetailPaneView`는 하단 분석 서랍(`showAnalyticsDrawer`)이 닫혀 있을 때 피벗·차트 재계산을 생략하고, `SlideshowFullScreenOverlayView` 및 `SlideAndPDFDetailPanes`는 다크 테마 명도 대비를 보장합니다.
     - 내장 HTTP 서버(`main.rs`)는 JSON `\uXXXX` 유니코드 이스케이프(UTF-16 서로게이트 쌍 포함)를 디코딩하고 `MAX_HTTP_BODY_BYTES`(256KB) 초과 `Content-Length` 및 스트림 요청에 대해 무음 절단 대신 명시적 `413 Payload Too Large`를 반환합니다.
- **Why**: 바이트·비트 수준 핫루프의 불필요한 힙 할당과 선형 탐색을 제거하고, Swift 코어와 Rust 코어 간 포맷·수식 동작 편차 및 실사용 편집 흐름의 UX 단절을 해소하기 위함입니다.
- **Trade-offs**: `BIFF8ExtendedMetadataEnvelope`가 `65,535`바이트를 초과할 때 다중 `0x0868` 레코드 청크로 분할 직렬화하므로 레코드 헤더 오버헤드(청크당 16바이트)가 소폭 증가하나, 대용량 다중 시트 메타데이터의 무손실 라운드트립을 보장합니다.
- **Alternatives rejected**:
  - `deflate_raw`에서 입력 길이 전체 크기로 `prev` 벡터를 할당하는 방식은 대용량 스트림 압축 시 불필요한 `4 * N` 힙 할당을 유발하므로 32KB 고정 링버퍼로 교체.
- **Verified by**: `swift build`, `swift test`, `cargo test --workspace`, `node scripts/check-repo-hygiene.mjs`, `.agents/skills/verify-easyoffice/scripts/eo.sh`
- **Revisit when**: ARM64 NEON / x86_64 AVX2 명시적 SIMD 인트린직 경로를 별도 가속 모듈로 추가할 때.

### 37. Fail-Closed IPC 경계, `--serve` 루프백 HTTP 보안 하드닝, 3대 엔진 선형 시간 PII·이미지 EXIF 비식별화 및 진실한 릴리스 패키징 정책 (2026-11-01)

- **Superseded by PD-094 (부분)**: 이 항목 중 Swift 앱·`swift build`/`swift test`·`scripts/package-macos-release.sh`·Swift 대응 규칙에 해당하는 부분은 PD-094로 대체되었습니다. Rust 엔진·WebUI·Windows 관련 결정은 그대로 유효합니다.
- **Decision (`PD-062`)**
  1. **프로덕션 WebUI 모의 폴백 제거, 구조화된 JSON IPC 전환 및 Stored-XSS 원천 차단 (`crates/easyoffice-app/ui/*`, `scripts/test-support/offline-*.js`, `crates/easyoffice-app/src/{security_ipc.rs,convert_ipc.rs,lib.rs}`)**
     - 프로덕션 `crates/easyoffice-app/ui/`에서 오프라인 모의 RPC 스크립트(`offline-formula-ast.js`, `offline-formula-eval.js`, `offline-rpc.js`)를 `scripts/test-support/`로 이동하고, 순수 클라이언트 좌표·수치 보조 유틸리티만 `crates/easyoffice-app/ui/sheet-utils.js`로 분리합니다.
     - `invokeBackend`는 데스크톱 호스트 브리지나 로컬 `--serve` 서버가 연결되지 않은 환경에서 모의 결과로 대체하지 않고 즉시 실행을 거부하며, 편집 중인 문서·시트·슬라이드 DOM을 변이시키지 않습니다.
     - `audit_security`는 빈 엔트리 입력을 `InvalidArgument`로 즉시 거부하고 실제 인코딩된 컨테이너(`containerBase64`)를 `FormatDetector`·`ZipArchiveCodec`·`Ole2ContainerCodec`·`Hwp5Codec`·`PdfSerializer`·`XmlSanitizer`로 검사하며, 모든 WebUI IPC 호출은 구분자 문자열 대신 구조화된 JSON 객체를 전달합니다.
     - 프로덕션 `index.html`의 인라인 스타일 속성과 `ui/*.js`의 동적 `.innerHTML` 대입을 전수 제거하고 `scripts/check-repo-hygiene.mjs`에 구조적 가드로 고정합니다.
  2. **루프백 `--serve` HTTP 서버 CSP 강제 및 Slowloris·패닉 복구 가드 (`crates/easyoffice-app/src/http_server.rs`, `tests/http_server_security.rs`)**
     - 모든 HTTP 응답(`200`, `400`, `403`, `404`, `413`, `500`, `503`)에 `Content-Security-Policy` 헤더(`default-src 'self'`)를 강제합니다.
     - RAII `ConnectionSlotGuard`(`MAX_CONCURRENT_HTTP_THREADS = 32`), 슬롯 포화 시 수락 스레드 블로킹 없는 즉시 `503` 응답, 워커 패닉 `catch_unwind` 복구(`500`), 연결당 총 `3초` 데드라인을 적용하여 부분 전송 연결에 의한 슬롯 고갈을 차단합니다.
  3. **3대 엔진(Swift·Rust·JS) 선형 시간($O(n)$) PII 스캐너 통일, 바이너리 이미지 EXIF 제거 및 `Mirror` 전수 검증 (`fixtures/pii-scrub-vectors.json`, `crates/easyoffice-app/src/smart_tools.rs`, `Sources/EasyOfficeCore/{DocumentSecurityAuditor.swift,PDFEngine.swift}`, `Tests/EasyOfficeTests/PiiScrubAndSecurityCompletenessTests.swift`)**
     - Rust 이메일 스캐너에 `email_failed_until` 전진 경계를 도입해 적대적 반복 문자열에서도 엄격한 $O(n)$ 선형 시간을 보장하고, 유니코드 IDN 이메일·통신사 접두 보존 휴대전화 번호(`010`/`011`/`016`~`019`)·외국인등록번호(7번째 자리 `1..=8`) 규칙을 `fixtures/pii-scrub-vectors.json` 공통 벡터로 Swift·Rust·JS 3대 엔진에서 동일하게 검증합니다.
     - Swift `DocumentSecurityAuditor`는 4대 도메인 모델의 모든 사용자 문자열·수식 리터럴 필드와 첨부 JPEG(`APP1..APP15`, `COM`)·PNG(`tEXt`, `zTXt`, `iTXt`, `eXIf`, `tIME`)·TIFF(EXIF/GPS/XMP 태그) 바이너리 메타데이터를 제거하며, `Mirror` 리플렉션 전수 워커 테스트로 필드 누락을 구조적으로 방지합니다.
  4. **Linux 플랫폼 식별자, macOS 20종 확장자 `Info.plist` 검증 및 진실한 Windows 프리뷰 패키징 (`crates/easyoffice-layout/src/lib.rs`, `scripts/{package-macos-release.sh,verify-macos-plist.mjs}`, `.github/workflows/{ci.yml,release.yml}`)**
     - `DesktopPlatform::LinuxWebView`를 추가하여 Linux 빌드에서 정확한 플랫폼 레이블(`Linux (WebKitGTK)`)과 단축키 수식어(`Ctrl`)를 반환합니다.
     - `scripts/package-macos-release.sh`는 비시스템 확장자가 `LSItemContentTypes`에 가려지지 않도록 `CFBundleDocumentTypes`를 분리 구성하고 `scripts/verify-macos-plist.mjs`로 20종 전체 확장자 매핑을 검증하며, 애드혹 서명 번들의 Gatekeeper 격리 속성 해제 명령을 문서화합니다.
     - 미사용 데스크톱 번들 설정 파일을 삭제하고, `.github/workflows/release.yml`은 최소 권한(`contents: read`, 발행 잡만 `contents: write`), 정확한 커밋 SHA 바인딩(`--target`), Windows 설치 프로그램 게시(당시의 CLI 프리뷰 아카이브와 `--serve` 스모크는 PD-086에서 릴리스에서 제거)를 수행합니다.
- **Why** 오프라인 모의 폴백과 합성 감사 페이로드로 인해 실제 보안·연결 실패가 성공처럼 보이던 경계를 제거하고, 루프백 서버 자원 보호·3대 엔진 PII 완전성·릴리스 산출물 정직성을 코드와 CI 가드로 일치시키기 위함입니다.
- **Trade-offs**
  1. 프로덕션 UI를 백엔드 없이 정적 파일로 직접 열면 편집 명령이 거부되나, 테스트 환경은 `scripts/webui-dom-harness.mjs`의 명시적 주입 경로로 결정론적 검증을 유지하면서 실사용자 환경의 거짓 성공을 원천 차단합니다.
  2. OWASP XXE 방어 및 ECMA-376 표준에 따라 문서 편집기 본문에 입력되어 `&lt;!DOCTYPE ...&gt;`로 XML 이스케이프된 일반 텍스트는 정상 문자열로 보존하고, 실제 컨테이너 내부 XML 파트의 엔티티 선언(`<!DOCTYPE`, `<!ENTITY`)만 `XmlSanitizer`로 차단하여 기술 문서 작성 시 오탐을 방지합니다.
  3. RFC 5322 이메일 로컬 파트 스캔 시 아톰 내부 단일 스크립트 경계를 유지하여 한글 조사·명사가 영문 이메일 앞에 공백 없이 붙은 문장(`"문의:홍길동user@example.com"`)에서 앞쪽 한글 본문이 과도하게 마스킹되는 부작용을 막습니다.
  4. 루프백(`127.0.0.1`) `--serve` 서버는 브라우저가 전송하는 `Host`/`Origin` 헤더가 존재할 때 루프백 일치 여부를 검증하되, `Origin` 헤더를 보내지 않는 로컬 CLI 및 CI 스모크 자동화 요청은 허용합니다.
- **Alternatives rejected**
  - 백엔드 오류 발생 시 클라이언트 정규식 치환이나 합성 수치로 폴백하는 방식은 미처리 개인정보와 미검증 컨테이너를 정상 처리된 것으로 오인하게 만들므로 기각.
  - 본문 일반 텍스트의 이스케이프된 XML 구문까지 일괄 거부하는 방식은 정상적인 개발·보안 문서 편집을 가로막는 과검증이므로 기각.
- **Verified by** `node scripts/check-repo-hygiene.mjs`, `node scripts/verify-macos-plist.mjs`, `node --test scripts/test-webui-*.mjs`, `cargo test --workspace -- --test-threads=1`, `swift test`
- **Revisit when** Apple Developer ID 공증(Notarization) 인증서 또는 Windows 코드 서명 설치 관리자 파이프라인을 도입할 때.

### 38. 스프레드시트 DAG 위상 정렬 재계산, 단일 강제 변환 헬퍼, IEEE 754 유효숫자 포맷 및 Rust·Swift 수식 엔진 동등성 정책 (2026-11-01)

- **Superseded by PD-094 (부분)**: 이 항목 중 Swift 앱·`swift build`/`swift test`·`scripts/package-macos-release.sh`·Swift 대응 규칙에 해당하는 부분은 PD-094로 대체되었습니다. Rust 엔진·WebUI·Windows 관련 결정은 그대로 유효합니다.
- **Decision (`PD-063`)**
  1. **비재귀 DAG 위상 정렬 재계산 및 부분 합산 없는 평가 예산 강제 (`crates/easyoffice-sheet/src/{lib.rs,evaluator.rs,coordinates.rs,parser.rs}`, `Sources/EasyOfficeCore/{SheetEngine.swift,SheetBuiltInFunctions.swift,SheetStatisticalAndLogicalFunctions.swift,SheetCoordinatesAndAST.swift,SheetFormulaCompiler.swift}`)**
     - 스프레드시트 재계산 경로를 재귀 호출 대신 명시적 작업 스택 기반 후위 위상 정렬(Kahn/Tarjan 비재귀 순회)과 워크북 세대 캐시(`EvaluationContext`)로 전환하여 `2,000`셀 이상의 정방향·역방향 의존 체인을 호출 스택 오버플로 없이 $O(V + E)$에 평가합니다.
     - 범위 집계(`SUM`, `AVERAGE`, `COUNT`, `COUNTIF`, `SUMIF`, `SUMIFS`, `AVERAGEIF`, `AVERAGEIFS`, `COUNTIFS`) 시 `MAX_RANGE_EXPANSION_CELLS`(`100,000`) 상한은 실제 식에서 참조한 셀 영역 단위로 검사하되, 희소 인덱스 스트리밍(`4,096`셀 초과 범위)을 통해 `99,999`개 채워진 셀에 대한 다중 집계 수식이 하나의 시트에서 부분 합산(silent truncation) 없이 정확한 결과를 반환하도록 보장합니다. 예산 초과 시에는 부분 합산 대신 즉시 `#REF!` 또는 `#VALUE!`를 반환합니다.
     - 전체 열·행 참조(`A:A`, `B:C`, `1:1`, `2:3`)를 `1..1,048,576`행(`MAX_ROWS`) 및 `1..16,384`열(`XFD`, `MAX_COLS`)로 정규화하고, 수식 토큰 상한을 `8,192`개로 확장하여 `1,025`개 이상의 항을 가진 수식을 지원합니다.
  2. **단일 값 강제 변환(Coercion) 테이블, 평가 출구 유한성 가드 및 IEEE 754 15자리 유효숫자 포맷 (`crates/easyoffice-sheet/src/{coercion.rs,chart_pivot.rs}`, `Sources/EasyOfficeCore/SheetValueCoercion.swift`, `fixtures/sheet-coercion-parity.json`)**
     - 직접 스칼라 산술·인자 강제 변환(`Empty -> 0`, `Boolean -> 1/0`, 숫자·통화·백분율·회계 괄호·ISO 날짜 문자열 파싱, 그 외 `#VALUE!`), 범위 참조 집계(숫자만 집계하고 텍스트·논리값·빈 셀 무시), 비교 연산(`Number < Text < Boolean` 타입 순위, 대소문자 무시 텍스트 비교, 빈 셀은 `0` 또는 `""`와 동치)을 Rust(`coercion.rs`)와 Swift(`SheetValueCoercion.swift`) 양쪽의 단일 헬퍼로 일원화하고 `fixtures/sheet-coercion-parity.json` 공통 벡터로 검증합니다.
     - 모든 수식 평가 출구에서 `NaN` 및 `±Infinity`를 검사하여 `#NUM!`으로 정규화하고, 일반 수치 포맷(`format_general_number` / `formatGeneralNumber`)은 IEEE 754 배정밀도 유효숫자 15자리 반올림으로 `0.1 + 0.2 -> "0.3"`을 출력하며, 조건부 집계(`>0`)는 `1e-12` 데드존 없이 엄격한 `0.0` 비교를 수행합니다.
     - SVG 차트 렌더러(`render_chart_svg` / `ChartEngine`)는 비유한 값을 필터링하고 유효 데이터에 유한 클램프(`[-1e12, 1e12]`)를 적용하여 어떠한 입력에서도 `NaN`/`Inf` 좌표가 없는 유효한 SVG 마크업을 보장합니다.
     - Swift `safeInt` 헬퍼는 하한을 엄격한 부등호(`t > -9223372036854775808.0`)로 제한하여 `-2^63`(`Int.min`) 입력이 후속 부호 반전(`abs`, `-x`, `%`)에서 산술 오버플로 트랩을 일으키지 않고 `#NUM!`을 반환하도록 고정합니다.
  3. **수식 문법·재무·와일드카드·피벗·내장 함수 패리티 및 모듈당 1,000줄 미만 분할 (`crates/easyoffice-sheet/src/functions_*.rs`, `Sources/EasyOfficeCore/Sheet*.swift`, `Sources/EasyOfficeApp/WorkspaceRibbonCommandRouter.swift`)**
     - 생략된 인자(`=IF(TRUE, , 5)`, `=XLOOKUP("x", A1:A2, B1:B2, , 0)`)를 `Missing` AST 노드로 보존하고, 에러 리터럴(`#N/A+1`), 중첩 비교(`=3>2>1`), `~*`/`~?`/`~~` 와일드카드 이스케이프(`COUNTIF`, `SUMIF`, `MATCH`, `SEARCH`, `VLOOKUP`, `XLOOKUP`), 기초 납입(`type=1`) `IPMT`/`PPMT` 및 거대 원금 `RATE` 수렴, 음수 유의값 `FLOOR`/`CEILING`, `SUBTOTAL` 중첩 더블카운팅 제외, `INDIRECT` 시트 반복 범위(`'Summary'!A1:'Summary'!A2`) 및 `a1` 불리언 인자 검증을 양쪽 엔진에 동일하게 구현합니다.
     - 날짜 직렬 함수를 포함하는 수식 셀은 표시 형식이 `.general`일 때 자동으로 ISO 날짜(`YYYY-MM-DD`)로 렌더링하며, 피벗 테이블 생성 시 기존 `"피벗 요약"` 시트를 재사용하고 소수 값을 보존합니다.
     - `Sources/EasyOfficeCore/Sheet*.swift`(12개 모듈)와 `crates/easyoffice-sheet/src/*.rs`(10개 모듈)의 모든 소스 파일을 예외 없이 1,000줄 미만으로 분할하고 `scripts/check-repo-hygiene.mjs`로 강제합니다.
- **Why** 대규모 스프레드시트 체인의 스택 한계와 희소 범위 집계 오류를 제거하고, ECMA-376 및 OpenFormula 표준에 부합하는 수치·날짜·재무·조회 연산 동등성을 Rust와 Swift 코어 전반에 확립하기 위함입니다.
- **Trade-offs**
  1. `A:A`나 `1:1` 같은 전체 열·행 참조를 허용하되 실제 순회는 채워진 희소 셀 인덱스(`populatedCoordinates` / `BTreeMap`)로 수행하고, 밀집 2차원 배열 물질화가 필요한 연산(`TRANSPOSE`, `MUNIT` 등)은 `MAX_RANGE_EXPANSION_CELLS`(`100,000`) 상한으로 방어하여 호환성과 메모리 안전성을 동시에 확보합니다.
  2. `INDIRECT`는 파서 복잡도와 보안 경계를 단순하게 유지하기 위해 A1 표기법(`a1 = TRUE` 또는 `1`)만 허용하고 `R1C1`(`a1 = FALSE` 또는 `0`) 모드는 명시적 `#VALUE!`로 거부합니다.
- **Alternatives rejected**
  - 재귀 평가기의 호출 깊이 상수만 늘리는 방식은 OS 스레드 스택 크기에 종속되어 깊은 체인에서 프로세스 크래시를 유발하므로 비재귀 위상 정렬로 교체.
  - 부동소수점 비교에서 임의의 고정 허용오차(`1e-12`)로 `0` 근처 양수를 `0`으로 간주하는 방식은 미소 과학 수치(`1E-15`)의 `>0` 집계를 왜곡하므로 평가·비교 단계는 IEEE 754 정밀도를 유지하고 표시 단계에서만 15자리 유효숫자 포맷을 적용.
- **Verified by** `node scripts/check-repo-hygiene.mjs`, `cargo test --workspace -- --test-threads=1`, `cargo clippy --workspace --all-targets -- -D warnings`, `swift build`, `swift test`
- **Revisit when** 다중 스레드 병렬 DAG 레벨 평가기 또는 동적 배열 스필(Spill) 연산자(`#`)를 도입할 때.

### 39. ZIP·OLE2 컨테이너 무결성, OOXML·ODF·HWP·BIFF8·PDF 표준 호환성 및 1,000줄 미만 코덱 분할 정책 (2026-11-01)

- **Status** Implemented
- **Superseded by PD-094 (부분)**: 이 항목 중 Swift 앱·`swift build`/`swift test`·`scripts/package-macos-release.sh`·Swift 대응 규칙에 해당하는 부분은 PD-094로 대체되었습니다. Rust 엔진·WebUI·Windows 관련 결정은 그대로 유효합니다.
- **Decision (`PD-064`)**
  1. **ZIP 및 OLE2 CFBF 컨테이너 경계 검증 강화 (`Sources/EasyOfficeCore/{BinaryContainerCodec.swift,OLE2AndHWP5Codec.swift}`, `crates/easyoffice-formats/src/{zip.rs,ole2.rs}`)**
     - ZIP 패커·언패커는 `BitPacking.checkedUInt16` 및 `BitPacking.checkedUInt32` 범위 검사를 적용하여 정수 절단을 원천 차단하고, 대소문자를 무시한 중복 엔트리 경로(`word/document.xml` 대 `WORD/DOCUMENT.XML`)와 파일-디렉터리 경로 충돌(`a/b` 대 `a/b/c`)을 모두 거부하며, 로컬 파일 헤더(LFH)와 중앙 디렉터리(CD)의 파일명·압축 방식·비스트리밍 크기·CRC-32 일치 여부를 교차 검증합니다.
     - OLE2 CFBF 코덱은 섹터 경계(`127`, `128`, `255`, `256` 섹터)에서 고정점 반복으로 `fatSectorCount`와 `difatSectorCount`를 계산하고 `109`개를 초과하는 FAT 섹터를 DIFAT 섹터 체인으로 연결하여 `20MB` 이상 대용량 스트림 라운드트립을 보장하며, OLE2 v3(`512B`)와 v4(`4096B`) 섹터 시프트, 명시적 스택 기반 디렉터리 트리 순회(`maxDirectoryEntries = 4,096`), 대소문자 무시 형제 스트림 중복 거부를 강제합니다.
  2. **OOXML·ODF·HWPX 스키마 호환성 및 미디어 파일명 위생 처리 (`Sources/EasyOfficeCore/{StandardZipOfficeCodecs.swift,StandardZipSheetSlideCodecs.swift,ODFAndHWPXCodecs.swift}`)**
     - OOXML 커스텀 확장 속성은 `xmlns:eo="https://easyoffice.local/ns/meta"` 및 `mc:Ignorable="eo"` 선언 아래 `eo:` 접두 속성으로 기록하고 레거시 비접두 속성은 읽기 시 하위 호환으로 함께 인식합니다.
     - DOCX `<w:highlight w:val="..."/>`에는 ECMA-376 `ST_HighlightColor` 16색 열거형 이름만 기록하고 원본 16진수 색상은 `eo:highlightHex`에 보존하며, `word/_rels/document.xml.rels` 및 `<w:drawing>` 인라인 이미지 관계, XLSX `xl/_rels/workbook.xml.rels` 타깃 경로 해석과 `<rPh>` 후리가나 제거 및 빈 `<si/>` 보존, PPTX `ppt/slideMasters/slideMaster1.xml` 및 `ppt/slideLayouts/slideLayout1.xml` 필수 패키징을 수행합니다.
     - ODF 코덱은 OpenFormula 수식 정규화(`of:=SUM([.A1:.B2])`와 `=SUM(A1:B2)` 상호 변환), `<table:covered-table-cell>` 열 오프셋 전진, ODT·ODP `meta.xml` `<dc:title>` 보존, ODP `<presentation:notes>` 및 `<text:page-number>` 본문 혼입 차단을 적용하며, 첨부 미디어 파일명은 경로 구분자와 제어문자를 제거하고 UTF-8 `255`바이트 이내 고유 이름으로 정규화합니다.
  3. **HWP 5.0·Word97·PowerPoint97·BIFF8 XLS 및 ISO 32000-1 PDF 정합성 (`Sources/EasyOfficeCore/{HWP5DocumentCodec.swift,LegacyOLE2OfficeCodec.swift,LegacyBIFF8XLSCodec.swift,PDFBinarySerializer.swift,PDFEngine.swift}`)**
     - HWP 5.0 코덱은 다중 섹션(`Section0..SectionN`) 읽기·쓰기와 표준 `HWPTAG_BEGIN(16)` 오프셋 기반 `HWPTAG_TABLE`(`tag 77`) + `HWPTAG_LIST_HEADER`(`tag 72`) + `HWPTAG_PARA_TEXT`(`tag 67`) (비표준 `61`/`56`/`51` 및 레거시 단일 레코드 표 읽기 하위 호환 포함), OpenFormula 따옴표 시트명(`['2026 [Q1]'.A1]`·`['Q1:Q2'.A1]`·3D 범위) 및 셀 경계 `of:=` 보존, PDF 본문·오버레이 비중첩 배치(`y = 445..582` 회피) 및 `\uXXXX`·`\v`·`beginbfrange` 배열 CMap, BIFF8 전 시트 `32,765` UTF-16 코드 유닛 한도 셀 트리 직렬화 및 `maxHWPTableRows = 4,096`(`EasyOfficeError.resourceBudgetExceeded`) 상한을 적용합니다.
     - Word97 바이너리 코덱은 `1Table`/`0Table`의 CLX(`Prc` + `Pcdt` `PlcfPcd`) 조각 테이블(압축 8비트 Latin-1 및 16비트 UTF-16LE 혼합 조각)을 지원하고, PowerPoint97 코덱은 `Slide`(`0x03EE`) 컨테이너와 `SlidePersistAtom`(`0x03F3`) 경계를 모두 인식합니다.
     - BIFF8 XLS 코덱은 `maxBIFF8Row = 65,535`, `maxBIFF8Col = 255`, `maxBIFF8RecordPayload = 8,224` 상한과 `FONT`·`XF`·`BOUNDSHEET`·`SST`+`CONTINUE`·`LABELSST` 레코드 분할·재조립 및 `fHighByte == 0` Latin-1 디코딩을 수행합니다.
     - PDF 코덱은 리터럴·16진수 문자열 외부의 `/Encrypt` 이름 토큰만 감지하고, `/F2`용 `/ToUnicode` CMap과 비중첩 Y 대역(주석·양식 필드·서명 영역), 유한 양수 `lineWidth` 가드, 트레일러 모델과 실제 페이지·주석·양식·리댁션 상태의 정본 일치 검증 및 텍스트 스트림이 없는 비지원 외부 PDF 거부(`EasyOfficeError.unsupportedFormat`)를 수행합니다.
  4. **코어 코덱 소스 파일 1,000줄 미만 분할 (`Sources/EasyOfficeCore/*.swift`)**
     - `HWP5DocumentCodec.swift`, `LegacyBIFF8XLSCodec.swift`, `StandardZipSheetSlideCodecs.swift`를 분리하여 수정된 모든 Swift 코덱 파일을 1,000줄 미만으로 유지합니다.
- **Why** 외부 오피스 구현체와의 상호운용성 결함(스키마 위반 속성, 누락된 마스터·스타일 레코드, 다중 섹션·조각 테이블·SST 연속 레코드 손실, PDF 좌표 겹침 및 암호화 오탐)과 컨테이너 경계 우회를 동시에 해소하기 위함입니다.
- **Trade-offs**
  1. PPTX 슬라이드 마스터·레이아웃 파트와 BIFF8 최소 `FONT`/`XF`/`SST` 레코드, PDF `/ToUnicode` CMap을 항상 포함하므로 최소 출력 파일 크기가 수백 바이트 증가하나 표준 뷰어 및 외부 오피스 도구의 열기 호환성을 보장합니다.
  2. HWP 5.0 표 행 수(`4,096`행)와 BIFF8 그리드(`65,536 x 256`)를 초과하는 입력은 조용히 잘라내지 않고 즉시 `EasyOfficeError.resourceBudgetExceeded`로 거부하여 데이터 유실을 방지합니다.
- **Alternatives rejected**
  - BIFF8이 아닌 XML 텍스트를 `.xls` 스트림에 저장하거나 커스텀 HWP 표 태그(`tag 77`)만 출력하는 방식은 외부 표준 파서에서 열리지 않으므로 표준 레코드 출력으로 대체하고 읽기 시에만 레거시 호환을 유지.
  - PDF 전체 바이트에서 단순 부분 문자열 검색으로 `/Encrypt`를 판정하는 방식은 본문 텍스트에 `/Encrypt` 단어가 포함된 정상 문서를 암호화 문서로 오탐하므로 문자열 리터럴 외부 토큰 파서로 교체.
- **Verified by** `node scripts/check-repo-hygiene.mjs`, `swift build`, `swift test`, `Tests/EasyOfficeTests/ContainerOOXMLODFLegacyPDFInteropTests.swift`
- **Revisit when** PDF 2.0(ISO 32000-2) 확장 객체 스트림(`ObjStm`) 압축 파서 또는 BIFF12(`.xlsb`) 바이너리 워크북 코덱을 추가할 때.

### 40. macOS 워크스페이스 비파괴 파일 수명주기, 자소 안전 문단 분할·병합, 비동기 일괄 변환, WCAG 2.1 AA 명암비 및 1,000줄 미만 모듈 분할 정책 (2026-11-01)

- **Status** Superseded
- **Superseded by PD-094**: macOS 네이티브 Swift 앱(`Sources/`, `Tests/`, `Package.swift`)을 삭제하고 macOS도 Windows와 같은 Tauri 데스크톱 셸로 배포합니다(#136). 이 항목의 본문은 이력으로 보존하며 더 이상 코드로 강제되지 않습니다. 같은 동작은 Rust 엔진(`crates/`)과 WebUI가 이어받습니다.
- **Decision (`PD-065`)**
  1. **비파괴 외부 파일 저장, 동일 모듈 손실 변환 Undo 보존 및 지연 최근 문서 복원 (`WorkspaceHub.swift`, `WorkspaceHubPersistenceAndBatch.swift`, `WorkspaceTabAndSessionModels.swift`, `WorkspaceViews.swift`)**
     - 디스크에서 가져온 외부 문서(`isLosslessNativeOrigin == false`)에서 `⌘S`(`saveTabOverwriteIfBackedByFile`)를 실행할 때는 즉시 원본 파일을 덮어쓰지 않고 `pendingLossyOverwriteConfirmationTabID`를 설정해 덮어쓰기 확인 대화상자 또는 다른 이름으로 저장으로 분기합니다. 사용자가 덮어쓰기를 승인하면 기존 파일 바이트를 `<원본>.bak`으로 먼저 백업한 뒤 저장하고 `isLosslessNativeOrigin = true`로 전환합니다.
     - `saveTabToDisk`에서 동일 모듈 내 손실 포맷 변환(`.docx` -> `.txt`/`.md`, `.xlsx` -> `.csv` 등)으로 활성 탭 페이로드가 교체될 때는 교체 직전에 `recordUndoSnapshot(for:)`을 기록하여 `⌘Z` 한 번으로 서식과 다중 시트 구조를 복원할 수 있게 합니다.
     - `restorePersistedRecentDocuments`는 앱 시작 시 디스크 전체 파일을 즉시 디코딩하지 않고 경량 메타데이터 플레이스홀더(`isMetadataOnlyPlaceholder = true`)만 복원하며, 실제 열람 시점(`activateOrReopenRecentTab`)에 디코딩이 실패하면 빈 탭을 열거나 `fileURL`을 연결하지 않고 실패 사유를 `lastErrorMessage`에 기록한 뒤 해당 항목을 `isUnreadable = true`로 표시합니다.
  2. **유니코드 자소 클러스터 안전 문단 분할·병합, 5대 앵커 재연결 및 원고지 비파괴 편집 (`DocumentEngine.swift`, `WorkspaceHubParagraphAndSlideMutations.swift`, `EditorDetailPanes.swift`)**
     - `ParagraphBlock.safeGraphemeClusterIndex`와 `splitContentAndRuns`는 UTF-16 오프셋을 `rangeOfComposedCharacterSequences` 경계로 정렬하여 한글 자모(NFD)나 다중 스칼라 이모지 ZWJ 시퀀스 내부가 깨지지 않도록 보호하고, 활성 선택 범위(`selectedParagraphUTF16Range`)가 있을 때 `Enter` 입력 시 선택 구간을 삭제하면서 양쪽 `DocumentTextRun` 서식(`isBold`, `isItalic`, `isUnderlined`, `isStrikethrough`, `isSuperscript`, `isSubscript`, `isCode`, `fontName`, `fontSizePt`, `foregroundHex`, `backgroundHex`, `linkURL`)을 보존합니다.
     - `mergeParagraphWithPrevious`는 인접 동일 스타일 런을 `DocumentTextRun.coalesceAdjacentRuns`로 병합하고, 두 번째 문단에 연결된 `images`, `footnotes`, `comments`, `revisions`, `bookmarks` 5대 앵커를 모두 앞 문단 ID로 재연결하며, `pendingFocusParagraphID`와 `pendingCaretUTF16Offset`으로 `NSTextView` 포커스와 캐럿 위치를 정확히 인계합니다.
     - `updateActiveDocumentManuscriptFullText`는 공통 접두·접미 문단과 동일 텍스트 중간 문단의 ID·스타일·런·앵커를 그대로 보존하고 변경된 문단만 갱신합니다.
  3. **일괄 변환 전체 경로 중복 제거, 파일명 충돌 회피, 쓰기 실패 집계 및 비동기 실행 (`SmartProductivityTools.swift`, `WorkspaceHubPDFAndFindActions.swift`, `HomeHubAndSmartViews.swift`)**
     - `enqueueFilesFromDiskIntoBatchConversion`은 파일명이 같더라도 서로 다른 폴더에 위치한 파일들이 누락되지 않도록 `standardizedFileURL.path` 기준으로 중복을 제거하며, 큐 변경·초기화 시 이전 `lastBatchConversionReport`를 즉시 무효화합니다.
     - `DocumentConversionOrchestrator.executeBatch` 및 `executeBatchAsync`는 출력 디렉터리 내 동일 파일명 충돌 시 `_2`, `_3` 접미사를 부여(`disambiguateOutputFileName`)하고, 디스크 쓰기(`write(to:options:.atomic)`)가 실패한 작업은 `succeededResults`에서 제외하고 `failedErrors`에 기록합니다.
     - 문서 비교(`compareOpenDocumentTabs` 및 `SmartDocumentAssistantHubView`)는 최소 2개 이상의 문서 탭이 열려 있을 때만 비교를 수행하며, `2,048`문단 상한 초과 시 `isTruncated = true`와 `truncatedAtParagraphLimit = 2_048`을 명시합니다.
  4. **콜드 런치 URL 큐, 다중 미저장 탭 종료 확인, 시트 경계 붙여넣기 보호, 리본 의미 분리 및 WCAG 2.1 AA 명암비 (`EasyOfficeApp.swift`, `WorkspaceHub.swift`, `WorkspaceRibbonModuleHandlers.swift`, `DesignTokens.swift`, `DocumentSubmodelsAndNotes.swift`)**
     - `EasyOfficeAppDelegate`와 `WorkspaceViewModel`은 워크스페이스 준비 전 수신된 파일 열기 요청을 `pendingLaunchURLs`에 큐잉하고, 앱 종료 시 모든 미저장 탭을 순회 확인하며, 시트 우하단 경계(`XFD1048576`) 바깥으로 넘어가는 TSV 붙여넣기 셀은 마지막 셀에 덮어쓰지 않고 건너뛴 뒤 경고를 표시합니다.
     - PDF 리본의 용지 크기(`pdfPaperSizePreset`)와 다단(`pdfPageColumnCount`) 명령을 배율(`zoomPercent`) 및 열람 전용 모드(`workspaceViewMode`)와 분리하고, 슬라이드 개체 삭제(`deleteActiveSlideElement`)는 명시적으로 선택된 개체(`selectedSlideElementID != nil`)가 있을 때만 동작하며, `TableCellSpan` 역직렬화 시 `addingReportingOverflow`로 정수 오버플로를 차단합니다.
     - `Sources/EasyOfficeCore/*.swift` 전체 34개 파일을 모두 1,000줄 미만으로 분할하고 `scripts/check-repo-hygiene.mjs`로 강제합니다.
- **Why** 외부 원본 파일의 의도치 않은 덮어쓰기나 손실 변환, 문단 분할·병합 시 인라인 서식과 주석·책갈피 유실, 일괄 변환 시 동명 파일 누락과 쓰기 오류 은폐, 다크·라이트 테마 저대비 문제를 근본 원인으로 제거하기 위함입니다.
- **Trade-offs**
  1. 외부 디스크 문서를 최초 `⌘S`로 덮어쓸 때 1회 확인 단계와 `.bak` 백업 파일이 생성되나, 세션 내 저장이 확인된 이후(`isLosslessNativeOrigin == true`)에는 추가 확인 없이 즉시 저장됩니다.
  2. 최근 문서 목록을 시작 시점에 메타데이터 플레이스홀더로 복원하므로 첫 클릭 시점에 디스크 I/O가 수행되나, 앱 초기 구동 속도를 높이고 손상된 파일이 시작 시점에 빈 탭으로 열리는 문제를 원천 차단합니다.
- **Alternatives rejected**
  - 최근 문서 열기 실패 시 빈 문서를 생성해 원본 `fileURL`을 연결하는 방식은 이후 저장 시 원본 파일을 빈 문서로 덮어쓰는 데이터 손실을 유발하므로 기각.
  - 단일 문서 탭만 열려 있을 때 마지막 문단을 제거한 가짜 기준본과 비교하는 방식은 사용자에게 허위 변경 통계를 보여주므로 기각.
- **Verified by** `node scripts/check-repo-hygiene.mjs`, `swift build`, `swift test --filter WorkspaceLifecycleEditingBatchA11yTests`, `swift test`
- **Revisit when** 실시간 파일시스템 이벤트(`FSEvents`) 기반 외부 파일 변경 자동 감지 알림을 추가할 때.

### 41. 상용 데스크톱·웹 오피스 스위트 UI/UX 비교 감사 기반 macOS 네이티브 및 크로스플랫폼 WebUI 인간공학·종단 간 기능 정합성 정책 (2026-11-05)

- **Status** Implemented
- **Superseded by PD-094 (부분)**: 이 항목 중 Swift 앱·`swift build`/`swift test`·`scripts/package-macos-release.sh`·Swift 대응 규칙에 해당하는 부분은 PD-094로 대체되었습니다. Rust 엔진·WebUI·Windows 관련 결정은 그대로 유효합니다.
- **Decision (`PD-066`)**
  1. **macOS 네이티브 시트 키보드 내비게이션, 전 모듈 배율 스케일링, 다중 표 선택 추적 및 스플릿 뷰 보조 창 격리 (`Sources/EasyOfficeCore/Workspace*.swift`, `Sources/EasyOfficeApp/*.swift`)**
     - 시트 그리드에서 인라인 편집 중이 아닐 때 방향키(`↑`, `↓`, `←`, `→`), `Shift` 범위 확장, `Delete` 범위 지우기 단축키를 `moveSheetSelection`과 `clearSelectedSheetRange`에 바인딩하고, 리본 `sheet-data-sort` 단축키 캡션을 리본 접기(`⌥⌘R`)와 충돌하지 않는 `⌃⌥S`로 분리합니다.
     - 하단 상태바 및 보기 리본의 `zoomPercent`(`50%`~`200%`)를 PDF 원본 뷰뿐 아니라 문서 용지 폭, 시트 행·열 헤더 및 셀 치수, 슬라이드 스테이지 높이, PDF 구조화 시트 폭에 일관되게 반영합니다.
     - 상단 탭 스트립의 `통합 검색` 입력창을 편집 모드(`isShowingHomeHub == false`)에서 본문 인라인 찾기(`findQueryText`, `isFindReplaceBarVisible = true`, Enter 시 `performFindNext()`)와 실시간 연동합니다.
     - `WorkspaceViewModel`에 `selectedDocumentTableIndex`와 `selectedDocumentTableCell`을 추가해 다중 표 문서에서 셀 클릭·편집 시 활성 표와 셀 좌표를 추적하고, 툴바·인스펙터·리본의 행·열 삽입·삭제·병합·분할·테두리·자동합계가 선택된 표와 셀을 대상으로 동작하도록 보장합니다.
     - 스플릿 뷰 보조 패널(`isPrimaryPane == false`)에서 `DocumentHorizontalRulerView`, `HWPManuscriptGridPaperView`, PDF 양식 필드 컨트롤이 주 활성 탭(`activeTabID`)을 변이시키지 못하도록 차단하고, 홈 허브 최근 문서 행의 우측 버튼을 열린 탭 `닫기`와 닫힌 기록 `목록 제거`(`removeRecentDocumentRecord`)로 분기하며, 전체화면 슬라이드쇼 오버레이에 좌우 화살표(`←`, `→`) 슬라이드 전환 단축키를 바인딩합니다.
  2. **크로스플랫폼 및 Windows WebUI 디자인 토큰·ARIA·포커스 링·스플릿 뷰·원고지 격자 및 5대 모듈 실동작 DOM 정합성 (`crates/easyoffice-app/ui/*`, `scripts/test-webui-behavior.mjs`)**
     - `styles.css`의 컬러 토큰을 디자인 시스템 정본(`--eo-primary-brand: #5C6BC0`, `--eo-accent-hwp: #6E56CF` / 다크 `#D6CCFF`, `--eo-accent-sheet: #0A705A`, `--eo-accent-slide: #D96B27`, `--eo-accent-pdf: #D9383A`)과 일치시키고, `:focus-visible` 라벤더 포커스 링, `.canvas-stage[data-split-view="true"]` 2단 비교 레이아웃, `#doc-editor.manuscript-grid` 20칸 원고지 격자, `#status-bar` 배율 조절(`#btn-zoom-out`, `#status-zoom-label`, `#btn-zoom-in`), 탭 컴포넌트 ARIA(`role="tablist"`, `role="tab"`, `aria-selected`)를 완비합니다.
     - 문서·한글 리본의 자동 목차(`doc-mod-toc`), 각주(`doc-mod-footnote`), 메일 머지(`doc-mod-mailmerge`), 공문서 표제부(`hwp-mod-header`), 누름틀(`hwp-mod-field`) 명령이 요약 문구만 추가하지 않고 실제 구조화된 DOM 요소(`nav`, `aside`, `header`, `span`)를 생성하도록 연결하며, `#doc-editor` 및 슬라이드 입력 영역 직접 타이핑 시 사전 Undo 스냅샷을 기록해 `Ctrl+Z`/`⌘Z` 시 타이핑 유실을 방지합니다.
     - 시트 그리드 셀에 방향키·`Tab`·`Delete` 키보드 탐색을 바인딩하고, 정렬(`sheet-data-sort`), 피벗(`sheet-mod-pivot`), 차트(`sheet-mod-chart`), 조건부 서식(`sheet-mod-condfmt`), 틀 고정(`sheet-mod-freeze`), 자동 합계(`sheet-data-autosum`)가 실제 시트 테이블과 집계 상태를 갱신하도록 구현합니다.
     - 슬라이드 표·차트·도형 삽입(`slide-insert-table`, `slide-insert-chart`, `slide-insert-shape`)을 새 슬라이드 추가(`slide-insert-slide`)와 분리해 현재 슬라이드 스테이지 내부 개체로 삽입·보존하고, 스마트 보조 도구가 현재 활성 모듈(`DOC`/`HWP`/`SHEET`/`SLIDE`/`PDF`)의 텍스트를 추출해 `styleSuggestions`까지 렌더링하도록 개선합니다.
- **Why** 상용 데스크톱·웹 오피스 스위트의 표준 인터랙션(2모드 시트 키보드 탐색, 전 모듈 줌, 다중 표 컨텍스트 유지, 스플릿 뷰 읽기 전용 격리, 리본 명령의 즉시 시각 피드백)과 비교했을 때 남아 있던 UI/UX 및 기능 단절을 양대 런타임 표면(macOS 네이티브 및 Windows/Linux/macOS WebUI)에서 일거에 해소하기 위함입니다.
- **Trade-offs**
  1. 스플릿 뷰 우측 보조 패널(`isPrimaryPane == false`)을 양방향 동시 편집 대신 참조·비교 전용으로 격리하여 두 패널이 단일 `activeTabID` 상태를 경합하며 주 탭 데이터를 덮어쓰는 사고를 원천 차단합니다.
  2. WebUI의 직접 타이핑 Undo 스냅샷은 매 키 입력마다 전체 DOM을 복제하면 메모리와 지연이 증가하므로 입력 세션 시작(`focusin`/`beforeinput`) 시점과 리본 명령 직전에 스냅샷을 기록해 응답성과 데이터 보존을 양립합니다.
- **Alternatives rejected**
  - 시트 그리드나 슬라이드·문서 캔버스 배율을 CSS `transform: scale()` 단일 래퍼로만 처리하면 스크롤 영역 경계와 히트 테스트 좌표가 어긋나므로 각 모듈의 레이아웃 폭·높이 및 `zoom` 속성에 직접 반영.
  - WebUI에서 동적 `.innerHTML` 문자열 결합으로 목차나 슬라이드 개체를 삽입하는 방식은 CSP 및 Stored-XSS 방어 불변식을 위반하므로 `document.createElement`와 텍스트 노드로만 구성.
- **Verified by** `node scripts/check-repo-hygiene.mjs`, `node --test scripts/test-webui-*.mjs`, `cargo run -p easyoffice-app -- --self-test`, `swift build`, `swift test`, `.agents/skills/verify-easyoffice/scripts/eo.sh`
- **Revisit when** 스플릿 뷰 보조 패널에 독립적인 2번째 활성 커서 세션(`secondaryActiveTabID` 변이 라우팅)을 도입할 때.

## 42. 적대적 아키텍처 리뷰(Red-Team) 기반 모노레포 정합성·파서 무결성·CI 경량화 (PD-067)

- **Status** Implemented
- **Superseded by PD-094 (부분)**: 이 항목 중 Swift 앱·`swift build`/`swift test`·`scripts/package-macos-release.sh`·Swift 대응 규칙에 해당하는 부분은 PD-094로 대체되었습니다. Rust 엔진·WebUI·Windows 관련 결정은 그대로 유효합니다.
- **Context** `@AquilaXk` 엔지니어링 하네스 기준 외부 적대적 아키텍처 리뷰(Red-Team Adversarial Architecture Review, `§2.1`~`§5`)에서 도출된 다중 런타임 단축키 드리프트, 위생 가드 임계치 포화, 라벨 부착 시 CI 중복 실행, 파서·수식·변환 경계 조건을 일괄 정비합니다.
- **Decision (`PD-067`)**
  1. **단일 모노레포 응집성 유지 및 크로스스택 리본 단축키 정합성 (`crates/easyoffice-app/src/ribbon.rs`, `crates/easyoffice-app/ui/ribbon-catalog.js`, `Sources/EasyOfficeCore/WorkspaceTabAndSessionModels.swift`)**
     - Swift macOS 네이티브 엔진, Rust 크로스플랫폼 코어, WebUI 프런트엔드를 단일 모노레포에 유지하여 포맷 스키마·수식 픽스처·140종 리본 카탈로그·15종 IPC 계약을 단일 커밋에서 원자적으로 검증합니다.
     - Rust(`ribbon.rs`)와 WebUI(`ribbon-catalog.js`)의 `data-sort` 단축키를 리본 접기(`view-ribbon`, `⌥⌘R` / `Alt+Cmd+R`)와 충돌하던 값에서 Swift(`PD-066`)와 동일한 `Ctrl+Alt+S` / `⌃⌥S`로 통일합니다.
  2. **위생 가드 경직성 해소 (`scripts/check-repo-hygiene.mjs`)**
     - `crates/easyoffice-app/ui/styles.css`의 박스 속성(`border:|border-radius:|box-shadow:`) 상한을 `18`(`18/18` 100% 포화)에서 `24`로 완화하고, `Sources/EasyOfficeApp`의 박스 프리미티브(`swiftBoxPrimCount`) 상한을 `12`(`12/12` 100% 포화)에서 `16`으로 완화하여 정당한 접근성·상태 표시자 UI 개선이 하드코딩된 임계치에 가로막히지 않도록 여유 버퍼를 부여합니다.
  3. **CI 트리거 경량화 및 릴리스 동시성 보호 (`.github/workflows/ci.yml`, `.github/workflows/release.yml`, `.agents/skills/verify-easyoffice/scripts/*`)**
     - `.github/workflows/ci.yml`의 `pull_request.types`에서 `labeled`를 제거(`types: [opened, synchronize, reopened, ready_for_review]`)하여 `automerge` 라벨 부착 시 이미 통과한 동일 SHA에 대해 macOS/Windows CI가 재실행되는 낭비를 차단합니다(`labeled` 이벤트는 `automerge.yml`이 전담).
     - `.github/workflows/release.yml` 최상단에 `concurrency` 그룹(`release-${{ github.ref_name || inputs.version }}`, `cancel-in-progress: false`)을 추가하여 동일 버전 태그 및 수동 디스패치 간 아티팩트 업로드 경합을 방지합니다.
     - `verify-easyoffice` 런타임 하네스(`eo.sh`, `DriveRunner.swift`)의 패키징 스크립트 경로를 `scripts/package-macos-release.sh`로 바로잡고, `document-hwp` 드라이버가 `hwpTab`에도 검증 문단을 추가한 뒤 재열람된 문서들의 실제 문단 수(`REOPENED_DOC_PARAGRAPHS`, `REOPENED_HWPX_PARAGRAPHS`, `REOPENED_HWP_PARAGRAPHS`)를 기록하도록 강화합니다.
  4. **CommonMark 인라인 구문 분석, 단일 패스 메일 머지, 교차 런 찾기·바꾸기, WebUI 슬라이드 직렬화 및 누적 평가·메모리 예산 강제 (`Sources/EasyOfficeCore/*`, `crates/easyoffice-app/ui/*`)**
     - CommonMark 파서에서 외곽 마커(`**`, `*`, `` ` ``)가 문자열 중간에서 일찍 닫히는 경우(`**bold** and **bold**`) 전체 문장 래핑으로 오판하지 않도록 내부 닫힘 검사를 수행하고 인라인 `**`/`*`/`~~`/`` ` `` 서식 런을 분해합니다.
     - 메일 머지(`MailMergeEngine`) 치환을 순차 다중 패스 `replacingOccurrences` 대신 단일 패스 정규식·토큰 스캔으로 전환하여 치환 값 내부에 포함된 `{{key}}` 문자열이 재귀 확장되는 2차 치환 취약점을 차단합니다.
     - `performFindAndReplace`가 인접 동일 서식 런을 먼저 병합(`DocumentTextRun.coalesceAdjacentRuns`)한 뒤 런 경계를 가로지르는 검색어 일치 구간까지 일관되게 치환하도록 개선합니다.
     - WebUI 슬라이드 파서·직렬화에서 제목·요약 내 가운데점(`·`)이 구분자로 오인되어 잘리지 않도록 첫 번째 ` — ` 구분자만 분리하고, 슬라이드 표 셀의 `rowSpan` 속성을 DOM `td.rowSpan`에 보존합니다.
     - `WorksheetModel.evaluateGrid`에 워크시트당 누적 수식 평가 단계 상한(`WorksheetModel.maxEvaluationSteps = 2_097_152`)을 적용하고, `BatchConversionEngine` 및 `DocumentConversionOrchestrator`에 배치 전체 누적 출력 바이트 예산(`maxCumulativeOutputBytes = 512 MiB`)을 강제하여 대규모 수식 그래프나 다수 문서 일괄 변환 시 프로세스 OOM을 Fail-Closed로 방어합니다.
- **Trade-offs**
  1. 단일 모노레포는 Swift와 Rust 두 툴체인을 한 저장소에서 관리해야 하나, 다중 저장소 분리 시 발생하는 버전 핀 불일치·교차 PR 동기화 비용·표준 픽스처 중복을 원천 제거합니다.
  2. 가드 상한에 약 25~33%의 헤드룸(`18 -> 24`, `12 -> 16`)을 부여하면 소규모 박스 규칙 추가는 허용되나, 과도한 중첩 카드화를 막는 상한선 자체는 유지하면서 100% 포화로 인한 개발 마비를 방지합니다.
  3. 시트 그리드 평가(`2,097,152` 단계)와 일괄 변환(`512 MiB` 누적 출력) 상한을 초과하는 초대형 배치는 분할 실행해야 하나, 단일 작업이 시스템 메모리를 고갈시키는 현상을 결정론적으로 차단합니다.
- **Alternatives rejected**
  - `EasyOffice`를 Swift 전용 저장소와 Rust/WebUI 저장소로 분리하는 다중 리포지토리 구성은 포맷 파서·수식 엔진·리본 계약의 원자적 교차 검증을 깨뜨리고 동기화 오버헤드만 높이므로 기각.
  - 인간 가독 문서(`docs/*.md`)를 폐지하고 JSON 전용 ADR(`trackedHumanDocumentFormat: NONE`)로만 아키텍처와 정책 결정을 관리하는 방식은 공개 오픈소스 저장소의 감사 투명성과 기여자 가독성을 훼손하므로 기각.
  - 일괄 변환 시 모든 변환 결과 바이트를 누적 상한 없이 인메모리 배열에 적재하는 방식은 다수의 대용량 문서 변환 시 OOM 크래시를 유발하므로 기각.
- **Verified by** `node scripts/check-repo-hygiene.mjs`, `node --test scripts/test-webui-*.mjs`, `cargo test --workspace`, `swift test`
- **Revisit when** 스트리밍 디스크 플러시 기반 무제한 배치 변환 모드를 추가할 때.

## 43. Windows(WebView2)·macOS(AppKit/WKWebView) 실런타임 구동 감사 기반 플랫폼 단축키·PDF 회전 레이아웃·크로스스택 122종 수식 카탈로그 정합성 (PD-068)

- **Status** Implemented
- **Superseded by PD-094 (부분)**: 이 항목 중 Swift 앱·`swift build`/`swift test`·`scripts/package-macos-release.sh`·Swift 대응 규칙에 해당하는 부분은 PD-094로 대체되었습니다. Rust 엔진·WebUI·Windows 관련 결정은 그대로 유효합니다.
- **Context** macOS 네이티브 앱(`EasyOfficeApp`)과 Windows/macOS WebUI(`easyoffice-app --serve`)를 실제 런타임에서 구동하여 전 모듈(`DOC`, `HWP`, `SHEET`, `SLIDE`, `PDF`, `CONV`), 라이트·다크 테마, 명령 팔레트, PDF 회전·양식, 일괄 변환을 교차 점검한 결과 도출된 플랫폼 단축키 표기, 레이아웃 줄바꿈·겹침, 탭 중복 개설, Swift/Rust 122종 수식 카탈로그 집합 차이를 일괄 정비합니다.
- **Decision (`PD-068`)**
  1. **Windows(`Ctrl`)·macOS(`⌘`/`Cmd`) 단축키 표기 정규화 및 키보드 바인딩 (`crates/easyoffice-app/ui/{ribbon-catalog.js,ribbon-history.js,app.js}`)**
     - `formatPlatformShortcut(rawShortcut, shortcutModifier)`를 도입하여 `shortcutModifier === "Ctrl"`(Windows/Linux 런타임)일 때 리본 버튼, 명령 팔레트, 빠른 실행 툴바(`#qat-save`, `#qat-undo`, `#qat-redo`)의 단축키 표기를 `Ctrl+B`, `Ctrl+I`, `Ctrl+U`, `Ctrl+Alt+S`, `Ctrl+Shift+P` 등 Windows 표준 표기로 변환하고, `#shortcut-hint` 배지가 `"CtrlK"` 대신 `"Ctrl+K"`(`"⌘"`일 때는 `"⌘K"`)로 표시되도록 정규화합니다.
     - 전역 `keydown` 핸들러에 `Ctrl/Cmd+B`, `Ctrl/Cmd+I`, `Ctrl/Cmd+U`, `Ctrl/Cmd+0`, `Ctrl+Alt+D` 단축키를 바인딩하여 현재 활성 모듈의 리본 명령을 직접 실행합니다.
  2. **WebUI 상단바·상태바 줄바꿈 방지, PDF 90° 회전 겹침 해소, 다크 모드 셀렉트 토큰 및 파비콘 (`crates/easyoffice-app/ui/{styles.css,index.html}`, `crates/easyoffice-app/src/http_server.rs`)**
     - `.topbar`, `.tab-pill`, `.action-btn`, `.statusbar` 요소에 `white-space: nowrap; flex-shrink: 0;`과 `#status-command-feedback` 말줄임(`text-overflow: ellipsis`)을 적용해 Windows/macOS 창 크기에서 한글 단어 중간 줄바꿈을 방지합니다.
     - PDF 90°/270° 회전 시 `.pdf-page-card[data-rotated-quarter="true"]` 상하 여백을 확보하여 회전된 용지가 상단 툴바 및 하단 입력 폼을 침범하지 않도록 하고, `pdf-mod-rotate` 실행 즉시 `#pdf-form-status` 회전 각도 요약을 갱신하며, `#btn-pdf-apply`를 `enqueueWorkspaceHistoryOp` 직렬 큐로 감쌉니다.
     - `#conv-src-fmt`·`#conv-dst-fmt`에 `.cmd-input.conv-fmt-select` 다크 표면 스타일을 적용하고, `http_server.rs`에 `/favicon.ico` SVG 응답을 추가해 브라우저 콘솔 `404` 오류를 제거합니다.
  3. **macOS 네이티브 발표 스테이지 불투명도, PDF 주석 접두어 제거, `.txt` 평문 디코딩 및 동일 `fileURL` 탭 중복 방지 (`Sources/EasyOfficeApp/{SlideshowAndPDFCanvasOverlays.swift,SlideAndPDFDetailPanes.swift}`, `Sources/EasyOfficeCore/WorkspaceHubPersistenceAndBatch.swift`)**
     - 전체화면 발표 스테이지(`SlideshowFullScreenOverlayView`) 배경의 `.opacity(0.97)`을 제거하여 100% 불투명 다크 스테이지로 렌더링하고 하위 편집 크롬 비침을 차단합니다.
     - PDF 스튜디오 검토 주석 표시(`SlideAndPDFDetailPanes.swift`)에서 중복 접두어(`하이라이트 하이라이트:`)를 제거하고, `.txt` 파일 열기 시 CommonMark 마크다운 파서 대신 UTF-8 줄 단위 평문(`.body`)으로 디코딩하며, `openDocument(data:fileName:fileURL:)` 호출 시 동일한 `standardizedFileURL.path`를 가진 열린 탭이 있으면 중복 탭을 생성하지 않고 기존 탭을 갱신·포커스합니다.
  4. **Swift(`SheetEngine.swift`)·Rust(`functions.rs`) 122종 수식 카탈로그 1:1 집합 일치 및 위생 가드 (`Sources/EasyOfficeCore/{SheetEngine.swift,SheetLookupAndTextFunctions.swift,SheetDateAndFinancialFunctions.swift}`, `scripts/check-repo-hygiene.mjs`)**
     - Swift 시트 엔진에 `DDB`(이중체감법 감가상각)와 `TRANSPOSE`(범위 전치 참조) 평가 분기를 추가하고, `SheetEngine.supportedFunctionNames`를 Rust `CANONICAL_122_FUNCTIONS`와 동일한 122개 함수 이름 집합으로 정렬하며(기존 확장 통계·재무·날짜 함수들은 평가기에서 계속 지원), `scripts/check-repo-hygiene.mjs`에서 양쪽 122개 함수 집합의 1:1 동등성을 자동 검증합니다.
  5. **한국어 워드프로세서 3단 고정 상단 스튜디오 크롬·플로팅 드롭다운 메뉴·`/` 커맨드 팔레트·2×2 밀리미터 눈금자 그리드·다크 모드 백색 용지 불변식 (`crates/easyoffice-app/ui/{index.html,styles.css,app.js,workspace-tools.js}`, `Sources/EasyOfficeApp/{DocumentStudioChromeViews.swift,EditorDetailPanes.swift,WorkspaceViews.swift}`)**
     - WebUI(`.app-shell` `height: 100vh; overflow: hidden`)와 macOS 네이티브(`MainWorkspaceWindowView` / `DocumentEditorDetailPaneView`)의 상단 스튜디오 크롬을 상단 메뉴바(`#menu-bar`, `28px`), 빠른 실행 아이콘 툴바(`#icon-toolbar`, `32px`), 문단·글자 서식 도구막대(`#doc-style-bar`, `35px`)의 3단 고정(`flex-shrink: 0`) 구조로 배치하여 모든 뷰 전환 및 배율 변경·문서 스크롤 시에도 상단바가 고정되도록 보장합니다.
     - 상단 8대 메뉴(`파일`, `편집`, `보기`, `입력`, `서식`, `쪽`, `표`, `도구`) 클릭 시 하단 리본 패널을 강제로 펼치지 않고, 각 메뉴 항목 아래에 플로팅 드롭다운 메뉴(`.menu-dropdown` / `.md-item` + `.md-shortcut` + `.md-sep`)를 오버레이로 표시합니다.
     - 커맨드 팔레트(`#cmd-palette-modal.cp-overlay`)를 상단 중앙 오버레이(`padding-top: 80px`, `.cp-panel` `480px`)로 배치하고, 좌측 `/` 프리픽스(`.cp-slash`) 검색창(`#cmd-search-input.cp-input`, `placeholder="커맨드 검색..."`)과 좌측 명령명(`.cp-item-label`)·우측 고정폭 단축키 배지(`.cp-item-shortcut`) 2열 리스트(`.cp-list` / `.cp-item` / `.cp-item--selected` `#e8f0fe`) 및 `ArrowUp`/`ArrowDown`/`Enter`/`Escape` 키보드 탐색을 제공합니다.
     - 문서 편집 영역을 좌상단 코너(`#ruler-corner`, `24×24px`), 상단 가로 눈금자(`#h-ruler`), 좌측 세로 눈금자(`#v-ruler`), 중앙 용지 스크롤 영역(`.doc-scroll-container`)의 2×2 스튜디오 그리드로 배치하고, A4 용지 4모서리에 본문 여백 경계를 안내하는 L자형 여백 표시자(`┌`, `┐`, `└`, `┘`)와 문단 끝 하늘색 엔터 부호(`↵`, `#4ea8de`)를 렌더링합니다.
     - 다크 테마(`html[data-theme="dark"]`)에서도 실제 인쇄물 조판 대비를 유지하기 위해 주변 작업대 배경은 다크 슬레이트(`#1f2329`)로 전환하되 A4 용지 표면(`#doc-editor`)은 백색(`#ffffff`)과 흑색 본문(`#111318`)을 유지하며, 하단 상태 표시줄에 `N / M 쪽`, `구역`, 클릭 가능한 `삽입`/`수정` 토글, `폭 맞춤`·`쪽 맞춤` 프리셋과 연속 배율 슬라이더(`50%`~`200%`)를 배치합니다.
  6. **HWP 5.0 및 KS X 6101 HWPX 수식 스크립트(`HWPTAG_EQEDIT`) AST 기반 LaTeX 변환기, Plane 15 PUA 기호 매핑 및 16바이트 제어문자·서로게이트 복구 (`crates/easyoffice-formats/src/{hwp_eqedit.rs,hwp5_codec.rs}`, `Sources/EasyOfficeCore/{HWPEquationCodec.swift,HWP5DocumentCodec.swift,ODFAndHWPXCodecs.swift}`)**
     - HWP 수식 편집기 스크립트(`OVER`, `SQRT`, `SUM`/`INT`/`PROD`, `SUP`/`SUB`/`LSUP`/`LSUB`, `LEFT`/`RIGHT`, `MATRIX`/`PMATRIX`/`BMATRIX`/`CASES`, `PILE`, `CHOOSE`, `HAT`/`BAR`/`VEC`/`TILDE`, 그리스 문자 및 연산자 사전, `#`/`&`/`~`/`` ` `` 조판 토큰)를 재귀 하강 AST 파서(`MAX_EQEDIT_DEPTH = 64` 깊이 예산 초과 시 Fail-Closed 거부)로 구문 분석하여 표준 LaTeX(`$$...$$`) 수식으로 변환하고, HWP 5.0 바이너리의 표준 `HWPTAG_EQEDIT`(`tag 88`, `HWPTAG_BEGIN + 72`) 레코드와 HWPX `<hp:equation><hp:script>` 요소를 양방향으로 디코딩합니다.
     - HWP 5.0 전용 문자표의 Plane 15 보충 사설 영역(`U+F00E1..U+F0855`) 및 레거시 PUA 코드포인트를 53종 실측 BMP 기호 테이블(`HWP5_PUA_BMP_SYMBOLS` / `puaBMPSymbols`)로 양방향 매핑합니다.
     - `HWPTAG_PARA_TEXT` 16바이트 확장 제어 구간에서 탭(`0x0009`)은 `'\t'`를 보존하고 자동 번호(`0x0012`)는 공백(`' '`)을 방출하며, 인라인 2바이트 제어인 하이픈(`0x0018` -> `\u{00AD}`), 묶음 빈칸(`0x001E` -> `\u{00A0}`), 고정폭 빈칸(`0x001F` -> `\u{2007}`)을 유니코드 표준 문자로 보존하고, 짝이 맞지 않는 단독 UTF-16 서로게이트(`0xD800..0xDFFF`)는 스트림 전체를 폐기하지 않고 `'□'`(`\u{25A1}`)로 치환하여 문단 손실을 방지합니다.
- **Trade-offs**
  1. `openDocument`가 동일 `fileURL`에 대해 기존 탭을 인플레이스 갱신하므로 동일 파일의 별도 사본 탭이 필요한 경우 다른 파일명으로 저장 후 열어야 하나, 반복 열기나 자동화 드라이버 실행 시 동일 파일 탭이 중복 누적되는 UX 결함을 원천 방지합니다.
  2. Swift `supportedFunctionNames`를 Rust `CANONICAL_122_FUNCTIONS`와 1:1로 일치시키면서 기존 추가 함수(`AVEDEV`, `CORREL`, `DATEDIF`, `EFFECT`, `EOMONTH`, `SYD` 등)는 카탈로그 배열 외부의 평가기 분기로 유지하여 수식 호환성과 크로스스택 카탈로그 계약을 동시에 만족합니다.
  3. 수식 스크립트 AST 중첩 깊이를 `64`단계(`MAX_EQEDIT_DEPTH = 64`)로 제한하므로 비정상적으로 깊은 중첩 수식은 거부되나, 악의적으로 조작된 중첩 수식 입력에 의한 호출 스택 오버플로를 결정론적으로 차단합니다.
- **Alternatives rejected**
  - 리본 카탈로그의 원본 단축키 문자열 자체를 `Ctrl+`로 덮어쓰는 방식은 macOS 네이티브 및 기존 macOS 기준 테스트 계약을 깨뜨리므로 런타임 `shortcutModifier` 기반 포매터(`formatPlatformShortcut`)로 해결.
  - PDF 90° 회전 겹침을 축소 배율(`scale(0.6)`)로만 처리하면 본문 가독성이 저하되므로 카드 최대 폭 조정과 쿼터 회전(`90°`/`270°`) 상하 여백 확장 방식을 채택.
- **Verified by** `node scripts/check-repo-hygiene.mjs`, `node --test scripts/test-webui-*.mjs`, `cargo test --workspace`, `swift test`, `.agents/skills/verify-easyoffice/scripts/eo.sh`
- **Revisit when** Linux Wayland 전용 시스템 트레이 및 글로벌 단축키 포털 연동이 필요할 때.

## 44. 리본 서식 상태·실행 취소 스택의 클라이언트 세션별·모듈별 분리 (PD-069, #106)

- **Status** Implemented
- **Context** `--serve` 서버가 전역 `SharedRibbonSession` 하나(서식 플래그 1벌, 실행 취소 스택 1벌)를 공유하여 `document-home-bold` 다음 `sheet-home-italic`을 실행하면 SHEET 응답에 DOC의 굵게가 넘어가고, `undo_ribbon_command`가 방금 한 SHEET 작업이 아닌 DOC 작업을 취소했으며, 브라우저 창 두 개가 같은 스택을 공유했습니다.
- **Decision (`PD-069`)**
  1. **세션·모듈 2단 키 레지스트리 (`crates/easyoffice-app/src/ribbon_session.rs`)**
     - 리본 상태와 `UndoRedoStack`을 `sessionId` → 모듈(`DOC`/`HWP`/`SHEET`/`SLIDE`/`PDF`) 2단 키로 보관합니다. `execute_ribbon_command`는 명령 ID가 속한 모듈의 상태·스택만 변경하고, `undo_ribbon_command`/`redo_ribbon_command`는 페이로드의 `module`(생략 시 해당 세션에서 마지막으로 실행한 모듈)의 스택만 되돌립니다. 리본 레이아웃 모드(`switch_ribbon_style`)는 세션 단위로 유지합니다.
     - 세션 맵은 최대 `32`개(`MAX_RIBBON_SESSIONS`)로 제한하고 초과 시 가장 오래 사용되지 않은 세션(LRU)을 제거합니다. `sessionId`는 영문·숫자·`-`·`_` 조합 `64`자 이하만 허용하며(위반 시 `InvalidArgument`), 생략 시 `default` 세션을 사용해 기존 문자열 페이로드·C-ABI·자체 검증과 하위 호환됩니다.
     - IPC 명령 수는 15종 그대로 유지하고 기존 4개 리본 명령의 JSON 페이로드에 `sessionId`/`module` 필드만 추가합니다.
  2. **WebUI 페이지 단위 세션 식별자 (`crates/easyoffice-app/ui/ribbon-history.js`, `scripts/test-support/offline-rpc.js`)**
     - 페이지 로드마다 `crypto.getRandomValues` 128비트 난수로 `eo-<32 hex>` 세션 식별자를 1회 생성해 리본 실행·실행 취소·다시 실행·레이아웃 전환 페이로드에 싣고, 리본 스냅샷에 실행 모듈(`ribbonModule`)을 기록해 `Ctrl+Z`/`Ctrl+Y` 시 해당 모듈을 함께 전송합니다. 오프라인 RPC 에뮬레이터도 동일한 세션·모듈 분리 의미론을 따릅니다.
- **Trade-offs** 세션 식별자는 인증 토큰이 아닌 상태 네임스페이스이므로 동일 식별자를 아는 클라이언트는 같은 스택에 접근할 수 있으나, 서버는 루프백 전용이며 문서 내용이 아닌 리본 토글 상태만 보관합니다. 최악의 경우 메모리는 `32 세션 × 5 모듈 × 128 스냅샷`으로 상한이 고정됩니다.
- **Alternatives rejected**
  - 세션·모듈별 전용 IPC 명령을 추가하는 방식은 15종 IPC 계약과 위생 검사를 깨뜨리므로 기존 페이로드 확장으로 해결.
  - 상한 없는 세션 맵은 장시간 구동 서버에서 창을 열 때마다 메모리가 증가하므로 LRU 축출로 기각.
- **Verified by** `cargo test -p easyoffice-app ribbon_session`(이슈 재현 순서, 두 세션 격리, 형식 오류 거부, LRU 상한), `node --test scripts/test-webui-behavior.mjs`(`ribbon IPC sends a per-window session id and the undo target module (#106)`), `node scripts/check-repo-hygiene.mjs`
- **Revisit when** 서버가 루프백 외 원격 다중 사용자 접속을 지원하여 세션 식별자에 인증·소유권 검증이 필요해질 때.

## 45. 시트 TEXT 서식 코드·대용량 통합 문서 재계산·없는 시트 `#REF!`·순환 참조 표준 오류값 정합성 (PD-070, #107)

- **Status** Implemented
- **Context** `evaluate_sheet` IPC 점검에서 `TEXT()`가 리터럴·0 채움·`;` 구간·월/요일 이름·시간을 처리하지 못했고, UI가 재계산마다 통합 문서 전체를 보내는데 HTTP 본문 한도(1MiB) 때문에 숫자 20,000개 + 수식 4,000개(약 1.37MB) 문서가 `413`으로 실패했으며, `=SUM(A1:Z10000)`이 범위 확장 상한(`100,000`셀)으로 `#REF!`를 냈습니다. 또한 `evaluate_workbook_formula`가 데모 값(`Sheet1!A1=1200`, `Sheet2!A1=100`)과 암묵적 `Sheet1`/`Sheet2`를 시드해 없는 시트 참조가 가상 값을 냈고, 순환 참조는 규격에 없는 `#CYCLE!`로 표시됐습니다.
- **Decision (`PD-070`)**
  1. **Excel 호환 `TEXT` 서식 코드 (`crates/easyoffice-sheet/src/text_format.rs`)**: 따옴표·`\` 이스케이프·맨 문자(`원`, `₩`, `$`) 리터럴, `[$₩-412]` 통화 기호, `0`/`#`/`?` 자리 표시자와 0 채움, 천 단위 구분 및 끝 쉼표 1/1000 배율, `%`, `E+`/`E-` 지수, 최대 4개 `;` 구간(양수;음수;0;텍스트)과 `@` 치환, `General`, 날짜·시간 코드(`yyyy`/`yy`, `m`~`mmmmm`, `d`~`dddd`, `h`/`hh`, 시 뒤·초 앞의 `m` 분, `ss`, 소수 초, `AM/PM`·`A/P`) 및 경과 시간 `[h]`/`[m]`/`[s]`를 지원합니다. 월·요일 이름은 영어(en-US) 이름을 사용하며, 음수·범위 밖 날짜 일련번호에 날짜 코드를 적용하면 `#VALUE!`입니다. 소수 자리 표시자는 10자리까지 반영합니다.
  2. **IPC 본문 한도 (`crates/easyoffice-app/src/http_server.rs`)**: `POST /api/ipc`만 `MAX_IPC_HTTP_BODY_BYTES = 16MiB`로 올리고, 그 밖의 경로와 헤더 블록은 `MAX_HTTP_BODY_BYTES = 1MiB`를 유지합니다. 한도 초과 `Content-Length`와 스트림은 계속 CSP 헤더가 붙은 `413`을 반환합니다. 연결당 3초 읽기 데드라인은 그대로 두며(루프백에서 16MiB는 수 ms), 선언된 길이만큼 버퍼를 미리 확보하지 않고 최대 1MiB씩 늘려 32슬롯 동시 접속 시 메모리 선점을 막습니다. UI는 이미 비어 있지 않은 셀만 전송합니다.
  3. **넓은 범위 참조 (`crates/easyoffice-sheet/src/{types.rs,range_refs.rs,coercion.rs}`)**: `CellRange::new`는 시트 경계(`1,048,576`행 × `16,384`열) 안의 모든 범위를 허용합니다. `SUM`/`COUNT`/`AVERAGE`/`MIN`/`MAX`/`COUNTA`/`COUNTBLANK`/`PRODUCT`/`SUMIF(S)`/`COUNTIF(S)`/`AVERAGEIF(S)`는 채워진 셀만 순회하고 빈 셀 수는 산술로 셉니다. 실제 벡터를 만드는 `CellRange::expand()`는 `MAX_EXPANDED_CELL_COUNT = 100,000`을 유지하고, 조밀 순회(조회·`SUMPRODUCT`·`SUBTOTAL` 등)는 평가 단계 예산(`MAX_EVAL_STEPS = 2,097,152`)을 넘으면 `#VALUE!`로 중단해 전체 그리드 남용 수식도 시간·메모리가 제한됩니다.
  4. **재계산 일괄 평가 (`WorkbookEngine::evaluate_targets`)**: `evaluate_sheet`의 모든 대상이 메모이제이션 컨텍스트 하나를 공유해 여러 대상이 참조하는 수식 셀을 한 번만 계산합니다(릴리스 빌드에서 숫자 20,000개 + `=SUM($A$1:An)` 4,000개, 1.37MB 페이로드 약 0.45초).
  5. **데모 시드 제거 (`crates/easyoffice-app/src/lib.rs`)**: `evaluate_workbook_formula`/`evaluate_sheet`는 데모 값과 암묵적 `Sheet1`/`Sheet2`를 만들지 않습니다. `sheets`가 없으면 기본 시트와 `cells`에 등장한 시트만 존재하므로 없는 시트 참조(`=Sheet2!A1`)는 `#REF!`이고, `expr`이 없으면 기준 셀(`cell` 또는 `A1`)을 평가합니다.
  6. **순환 참조 표현**: 셀 값은 표준 오류 리터럴 `#NUM!`로 표시·저장하고(ECMA-376에 `#CYCLE!`은 없음), 내부적으로는 `FormulaError::Cycle` 변형을 유지해 IPC 응답에 별도 `"circular": true` 플래그를 싣습니다. WebUI는 이 플래그를 받으면 상태 표시줄(`#status-command-feedback`)에 순환 참조 경고를 띄웁니다. 기존 문서 호환을 위해 입력 수식의 `#CYCLE!` 리터럴은 계속 파싱합니다.
- **Trade-offs**
  1. Excel은 순환 참조 셀을 `0`으로 표시하지만, 0을 저장하면 의존 셀이 오류 없이 잘못된 값을 저장하게 됩니다. `#NUM!`은 반복 계산이 수렴하지 않을 때 쓰는 표준 오류와 의미가 가깝고 의존 셀로 전파되므로 저장된 파일에서 잘못된 숫자가 조용히 퍼지지 않습니다. 대신 `ISERROR`/`IFERROR`가 순환 참조를 일반 오류처럼 처리합니다.
  2. 16MiB IPC 한도는 실사용 대형 시트를 수용하지만 그보다 큰 통합 문서는 여전히 `413`입니다. 셀 단위 증분 전송은 별도 작업입니다.
  3. Swift 코어(`Sources/EasyOfficeCore`)와 Swift 테스트는 아직 `#CYCLE!`을 표시하며, 이번 변경은 Rust 엔진·WebUI·오프라인 JS 미러에만 적용됩니다.
- **Alternatives rejected**
  - 순환 참조를 `#REF!`로 표시하는 방식: 참조 대상이 존재하는데 "잘못된 참조"로 오인시켜 기각했습니다.
  - IPC 본문 한도 무제한 해제: 루프백이라도 32슬롯 × 무제한 버퍼가 메모리 고갈 경로가 되므로 상한을 둡니다.
- **Verified by** `cargo test -p easyoffice-sheet --test sheet_text_format_and_large_ranges`, `crates/easyoffice-app/src/sheet_ipc_tests.rs`, `node --test scripts/test-webui-sheet-atomicity.mjs`
- **Revisit when** 셀 단위 증분 재계산 IPC 또는 로캘별(ko-KR) 월·요일 이름 서식 코드(`[$-412]`)를 도입할 때.

## 46. 출력 컨테이너 필수 파트·스키마 정합화, 표준 한글 CID 글꼴 참조 및 DOC/PPT 쓰기 호환성 제한 표시 (PD-071, #105)

- **Status** Implemented
- **Context** `convert_document_format` 출력을 풀어 각 규격과 대조한 결과(#105), XLSX 셀의 비표준 `row`/`col` 속성과 항상 `0`인 수식 캐시값, PPTX 테마·`p:clrMap`·도형 비시각 속성 누락 및 `<p:sld title notes>` 비표준 속성, 빈 HWPX `header.xml`과 `version.xml`/`settings.xml`/`secPr` 누락, DOCX `<w:document title>` 비표준 속성과 `w:tblPr`/`w:tblGrid` 누락, PDF 한글 글꼴(`EasyOfficeGothic-Medium`)의 프로그램 없는 참조와 UTF-8 바이트 그대로의 `/Title`, BOM 없는 CSV, 규격 구현처럼 표시되는 `.doc`/`.ppt` 출력이 확인되었습니다.
- **Decision (`PD-071`)**
  1. **XLSX (`crates/easyoffice-formats/src/office_codecs.rs`, `crates/easyoffice-app/src/convert_ipc.rs`)**: `<c>`에는 스키마 속성(`r`, `t`)만 기록합니다. `easyoffice-sheet`가 `easyoffice-formats`에 의존하므로 포맷 크레이트는 시트 엔진을 호출할 수 없어(순환 의존), 앱 계층이 시트 엔진(`SheetEngine::recalculate_all`)으로 계산한 결과를 `XlsxFormulaCache`로 넘기고 `OoxmlCodec::pack_xlsx_multi_with_formula_cache`가 수치·문자열(`t="str"`)·논리(`t="b"`) 캐시값을 `<v>`에 씁니다. 계산값이 없거나 오류인 수식 셀은 `<v>`를 생략하고, `workbook.xml`은 항상 `<calcPr fullCalcOnLoad="1"/>`을 지정해 소비자가 열 때 재계산합니다.
  2. **PPTX (`ooxml_presentation.rs`, `ooxml_shared_parts.rs`)**: `ppt/theme/theme1.xml`(색·글꼴·서식 체계)과 마스터→테마 관계, 마스터 `p:clrMap`, 모든 `p:spTree`의 `p:nvGrpSpPr`/`p:grpSpPr`, 모든 `p:sp`의 `p:nvSpPr`/`p:spPr`/`p:txBody`를 기록합니다. 슬라이드 제목은 `p:ph type="title"` 도형, 발표자 노트는 노트 마스터(`theme2.xml`)를 둔 `ppt/notesSlides/notesSlideN.xml`, 프레젠테이션 제목은 `docProps/core.xml`에 저장하고 쇼 여부는 콘텐츠 형식(`slideshow.main+xml`)으로만 표현합니다. 모든 파트는 `[Content_Types].xml` Override와 일치합니다.
  3. **HWPX (`hwpx_codec.rs`, `hwpx_parts.rs`)**: `mimetype`을 첫 엔트리로 무압축 저장하고, `version.xml`, `settings.xml`, 글꼴·글자 모양(`charPr id="0"`)·문단 모양(`paraPr id="0"`)·테두리·스타일(`style id="0"`)을 정의한 `header.xml`을 기록합니다. `section0.xml` 첫 문단 첫 런에 `hp:secPr`(+`hp:colPr`)를 두고 모든 문단·런에 `paraPrIDRef`/`charPrIDRef`를 지정하며, 표는 `hp:p > hp:run > hp:tbl`, 셀은 `hp:tc > hp:subList > hp:p > hp:run > hp:t`로 감쌉니다. 서식 여부·발신 기관·결재선은 `hh:head` 비표준 속성 대신 `content.hpf`의 `opf:meta`에 저장하고, `META-INF/manifest.xml`과 `content.hpf`가 모든 파트를 나열합니다. 이전 출력의 `hh:head` 속성은 읽기 호환을 위해 계속 해석합니다.
  4. **DOCX**: `<w:document>`의 `title` 속성을 제거하고 제목을 `docProps/core.xml`(관계·콘텐츠 형식 포함)에 기록하며, 표마다 `w:tblPr`(자동 너비·단선 테두리)과 열 수만큼의 `w:gridCol`을 가진 `w:tblGrid`를 첫 행 앞에 둡니다.
  5. **PDF 한글 글꼴 (`pdf_writer.rs`)**: 글꼴 바이너리를 저장소·배포물에 포함하지 않고 PDF 1.4 사전 정의 CJK 참조를 사용합니다. `/F2`는 `Type0` 글꼴(`/BaseFont /HYGoThic-Medium-UniKS-UCS2-H`, `/Encoding /UniKS-UCS2-H`)이며, 하위 `CIDFontType0` `HYGoThic-Medium`은 `Adobe-Korea1-1` 문자 모음과 `FontDescriptor`(메트릭)를 가집니다. `UniKS-UCS2-H`는 기존 본문 코드(UTF-16BE 코드 단위)를 그대로 Adobe-Korea1 CID로 사상하는 사전 정의 CMap이므로 콘텐츠 스트림은 바뀌지 않고, `/ToUnicode` CMap으로 텍스트 추출 정확도를 유지합니다. 문서 정보 사전의 `/Title`은 ASCII면 PDFDocEncoding 리터럴, 그 외에는 BOM(`FEFF`)을 붙인 UTF-16BE 16진 문자열로 기록하고, 카탈로그의 비표준 `/Title` 키는 제거합니다.
  6. **CSV/TSV**: 출력 앞에 UTF-8 BOM(`EF BB BF`)을 붙이고(`CsvCodec::serialize_with_bom`), `CsvCodec::parse`/`parse_scalar_baseline`은 입력 앞 BOM을 제거하여 라운드트립을 유지합니다.
  7. **DOC/PPT 쓰기 호환성 표시 (해결됨, `PD-090`)**: 이 결정 시점의 내용입니다. #112에서 규격 바이너리 작성기를 도입해 `.doc`/`.ppt` 제한 표시를 제거했습니다(표시 경로 자체는 유지). 당시 내용: 규격을 충족하는 워드·프레젠테이션 바이너리 작성기는 후속 과제(#112)로 분리하고, `DocumentFormat::output_compatibility()`가 `.doc`/`.ppt`에 `Limited(사유)`를 반환합니다. 변환 응답은 `"specification"`을 `"… 하위 집합 (호환성 제한)"`으로 표시하고 `"compatibility":"limited"`와 `"compatibilityWarning"`을 함께 반환하며, 변환 결과 상자와 저장 상태 메시지가 경고 문구를 그대로 노출합니다.
- **Why** 저장·변환한 파일이 해당 형식을 지원하는 편집기에서 복구 경고 없이 열리고, 계산하지 않는 뷰어에서도 올바른 수식 결과가 보이며, 지원하지 못하는 형식은 사용자가 그 사실을 알 수 있게 하기 위함입니다.
- **Trade-offs**
  1. 한글 글꼴을 임베딩하지 않으므로 글리프 모양은 뷰어가 대체하는 설치 글꼴에 따르며, 사전 정의 CJK 글꼴 대체를 지원하지 않는 최소 구현 뷰어에서는 한글이 표시되지 않을 수 있습니다. 대신 배포물 크기와 글꼴 라이선스 부담이 없습니다. `UniKS-UCS2-H`는 BMP 밖 문자(서로게이트 쌍)를 표현하지 못합니다.
  2. 포맷 크레이트 단독 호출(`UniversalFormatConverter::encode`)은 수식 캐시값을 쓰지 않고 `fullCalcOnLoad`에 의존합니다.
  3. 출력 파트가 늘어 최소 PPTX/HWPX 크기가 수 KB 증가합니다.
- **Alternatives rejected**
  - 공개 한글 TrueType 글꼴 서브셋 임베딩(`CIDFontType2` + `FontFile2`): 글꼴 파서·서브세터와 수 MB 바이너리 번들이 필요해 이번 범위에서 기각(재검토 조건 참조).
  - `Adobe-Identity-0` + 임의 글꼴명 유지: 뷰어가 글꼴을 해석할 근거가 없어 한글 미표시가 지속되므로 기각.
  - 수식 셀에 `0`을 계속 캐시: 재계산하지 않는 뷰어에서 잘못된 값을 보여 주므로 기각.
  - DOC/PPT 출력을 거부: 기존 EasyOffice 왕복 저장과 상호 변환 경로를 깨므로, 출력은 유지하고 호환성 제한을 명시하는 방식을 채택.
- **Verified by** `crates/easyoffice-formats/tests/output_container_spec_validity.rs`, `crates/easyoffice-app/src/convert_ipc.rs` 단위 테스트, `scripts/test-webui-behavior.mjs`(호환성 제한 표시), `cargo test --workspace`
- **Revisit when** 공개 라이선스 한글 글꼴 서브셋 임베딩 또는 규격 충족 워드·프레젠테이션 바이너리 작성기(#112)를 도입할 때.

## 47. WebUI 작업 유실 방지(자동 저장·이탈 경고)·단축키 라벨 1:1 정합성 및 모듈별 도구모음 컨텍스트 (PD-072, #103, #108)

- **Status** Implemented
- **Context** Windows `easyoffice-app.exe --serve` WebUI 실조작 리뷰(#103, #108)에서 새로고침·탭 닫기 시 경고 없이 편집 내용이 사라지고, 메뉴·툴팁이 처리기 없는 단축키나 브라우저 예약 키(`Ctrl+Shift+R` 강력 새로고침, `Ctrl+N` 새 창 등)를 안내하여 안내대로 누르면 작업이 즉시 유실되는 문제, 시트·슬라이드·PDF·변환기 모듈에서도 문서 전용 도구모음과 시트 선택 집계가 그대로 노출되는 문제, 열린 문서 탭 줄 숨김, 선택 영역과 무관한 내부 문자열 클립보드, 부트스트랩 IPC 실패 시 빈 화면과 `macOS · ⌘` 고정 표기가 확인되었습니다.
- **Decision (`PD-072`)**
  1. **워크스페이스 스냅샷 자동 저장·복원 및 수정 상태 추적 (`crates/easyoffice-app/ui/{workspace-session.js,app.js}`)**
     - 실행 취소/다시 실행이 쓰는 워크스페이스 스냅샷(`captureWorkspaceSnapshot` / `restoreWorkspaceSnapshot`, `ribbon-history.js`)을 그대로 SSOT로 재사용하여 `localStorage["easyoffice.workspace.autosave.v1"]`에 `{ version: 1, savedAt, dirty, baseline, snapshot }` 레코드로 1.2초 디바운스 자동 저장하고, 시작 시 직렬 큐 안에서 복원합니다. `pagehide`·`beforeunload` 시점에는 동기 플러시합니다.
     - 복원 전 스냅샷 형식과 직렬화 노드 트리(`script`·`iframe`·`object`·`style`·`form` 등 금지 태그, `on*` 속성, `javascript:`/`data:` URL)를 검증하여 손상되었거나 안전하지 않은 레코드는 복원하지 않습니다.
     - 수정 여부는 문서·시트·슬라이드·PDF 내용 지문(빈 셀·빈 스타일·표 요약 파생 속성을 정규화한 FNV-1a 해시)을 기준선과 비교해 판정하므로 화면·시트 탭·슬라이드 전환만으로는 수정 상태가 되지 않습니다. 명시적 저장(`Ctrl+S`, 저장 버튼, 팔레트) 성공 시 기준선을 갱신합니다.
     - `beforeunload` 처리기는 수정(또는 미확정 활동) 상태일 때만 등록하고, 깨끗한 상태로 돌아오면(실행 취소로 기준선 복귀, 저장) 해제합니다.
  2. **단축키 레지스트리 단일화 및 브라우저 예약 키 배제 (`crates/easyoffice-app/ui/{ribbon-catalog.js,ribbon-history.js,app.js,index.html}`)**
     - `WORKSPACE_SHORTCUT_BINDINGS`를 단축키 표기의 단일 원천으로 두고, 전역 `keydown` 처리기가 이 표로만 분기합니다: `Ctrl/⌘+S`(저장), `Ctrl/⌘+Z`(실행 취소), `Ctrl/⌘+Y`·`Ctrl/⌘+Shift+Z`(다시 실행), `Ctrl/⌘+K`·`Ctrl/⌘+F`·`/`(명령 팔레트), `Ctrl/⌘+B/I/U`(굵게·기울임·밑줄), `Ctrl/⌘+0`(100% 배율), `Ctrl+Alt+D`(테마), `Insert`(삽입/수정). `Ctrl+X/C/V`는 브라우저 기본 편집 키로 그대로 둡니다.
     - 처리기가 없던 메뉴·툴팁 라벨(`Alt+N/C/L/T/U`, `Ctrl+N`, `Ctrl+N,N/H/T`, `Ctrl+G,P/W/C/T`, `Ctrl+K,H`, `Ctrl+F2`, `Ctrl+F10`, `F7`, `Shift/Ctrl+Num ±`, `M`, `P`, `Ctrl+Shift+S`, 정렬 `Ctrl+Shift+L/C/R/M/T/J`, 크기·줄 간격 `Alt+Shift+E/R/Z/A`, 성능 진단 `F9`)을 제거하고, 리본 도구 줄과 명령 팔레트도 레지스트리에 바인딩된 명령의 단축키만 표시합니다(`resolveRibbonShortcutLabel`).
     - `/` 등 단일 키 단축키는 입력 필드·`contenteditable` 편집 중에는 발동하지 않으며, 레지스트리에 없는 조합(`Ctrl+N/T/W/R`, `Ctrl+Shift+R/T/J/C/N/I`, `F5` 등)은 가로채지 않습니다.
  3. **활성 모듈 컨텍스트 도구모음·상태 표시줄·탭 줄 (`crates/easyoffice-app/ui/{workspace-session.js,ribbon-history.js,workspace-tools.js,styles.css,index.html}`)**
     - 모듈 전환(`applyModuleAccent`) 시 `#doc-style-bar`는 `DOC`/`HWP`에서만 표시하고, `#icon-toolbar` 버튼과 상단 메뉴 항목은 명령별 적용 모듈 표에 따라 `disabled` / `aria-disabled="true"`로 비활성화합니다(저장·실행 취소·팔레트·테마·배율·검사 도구는 전 모듈, 복사·글자 모양·표/차트 삽입은 문서·시트·슬라이드, 그 외 문단·쪽·각주·조판 부호 도구는 문서 전용).
     - 시트 선택 집계(`#status-sheet-agg`)는 `SHEET`에서만, 문서 쪽·구역·파일명 그룹은 `DOC`/`HWP`에서만 표시합니다.
     - 열린 문서 탭 줄(`#tab-strip`)을 기본(접힌) 크롬에서도 표시하고 활성 모듈의 탭을 강조하며, 파일 메뉴 항목을 "새 문서" 대신 "… 탭으로 전환"으로 표기합니다.
  4. **선택 영역 기반 시스템 클립보드 (`crates/easyoffice-app/ui/{workspace-session.js,workspace-tools.js}`)**
     - 편집 메뉴·도구모음의 오려 두기/복사하기/붙이기는 현재 모듈 편집 영역 안의 실제 선택 범위(`getSelection()` / 마지막 편집 영역 `selectionchange` 범위)에만 작용하고 `navigator.clipboard.writeText`/`readText`를 우선 사용합니다(권한 거부 시 `execCommand("copy")` 및 앱 내부 버퍼로 폴백). 선택이 없으면 문단을 지우지 않고 안내만 표시하며, 오려 두기·붙이기는 실행 취소 스냅샷을 남깁니다.
  5. **부트스트랩 실패 안내·재시도 및 플랫폼 표기 (`crates/easyoffice-app/ui/{app.js,index.html}`, `crates/easyoffice-app/src/{http_server.rs,self_test.rs}`)**
     - `get_workspace_bootstrap` 실패 시 `#bootstrap-error`(`role="alert"`)에 오류와 `다시 연결` 버튼을 표시하고, 재시도 성공 시 탭·필터·리본을 다시 구성합니다. 부트스트랩 전에는 `navigator` 플랫폼으로 `Windows · Ctrl` 등 실제 플랫폼을 표기하며 `macOS · ⌘`를 고정 표기하지 않습니다.
     - 새 UI 모듈 `workspace-session.js`를 `--serve` 정적 자산과 자가 진단 자산 무결성 검사에 추가합니다.
- **Trade-offs**
  1. 자동 저장본은 같은 브라우저 프로필·같은 출처(루프백 포트)에서만 복원되며, 저장 공간 한도를 넘으면 상태 표시줄에 실패를 안내하고 이탈 경고로 보호합니다.
  2. 명시적 저장은 활성 모듈 문서만 내려받지만 수정 상태 기준선은 워크스페이스 전체로 갱신합니다. 다른 모듈의 미저장 변경은 자동 저장본으로 보존됩니다.
  3. 시트 셀에 포커스가 있는 동안에는 셀 확정 상태 머신과 경합하지 않도록 타이머 자동 저장을 미루고, 다음 활동 또는 이탈 시점 플러시에서 저장합니다.
- **Alternatives rejected**
  - 별도 자동 저장 직렬화 형식 신설: 실행 취소 스냅샷과 이중 관리되어 필드 누락 위험이 있으므로 동일 스냅샷 SSOT를 재사용.
  - 브라우저 예약 키를 `preventDefault`로 강제 점유: 새 창·탭 닫기 등은 페이지가 가로챌 수 없고 사용자 기대와도 충돌하므로 라벨에서 제거.
- **Verified by** `node scripts/check-repo-hygiene.mjs`, `node --test scripts/test-webui-*.mjs`(`scripts/test-webui-session-context.mjs` 포함), `cargo test --workspace`, `cargo run -p easyoffice-app -- --self-test`
- **Revisit when** 서버 측 문서 저장소(다중 문서·버전 기록)를 도입하여 로컬 자동 저장본을 서버 초안으로 대체할 때.

## 48. 저장 서식·표 위치·표 계산값·다중 시트 보존, 저장 실패 표시 및 문서 제목 파일명 (PD-073, #104)

- **Status** Implemented
- **Context** WebUI에서 `Ctrl+S`로 저장한 파일을 풀어 확인한 결과(#104), 문서·한글 저장 본문이 `extractEditableDocumentText` 평문이어서 DOCX에 `w:rPr`/`w:pPr`가 하나도 없고(굵게·기울임·밑줄·정렬·제목 수준 유실), 본문 중간 표가 문서 끝으로 이동하며, 표 셀 `=SUM(ABOVE)`가 계산값 대신 문자열로 저장되었습니다. 한글 저장은 항상 `[[AGENCY]] 행정안전부`를 넣었고, 사용자가 `[[SLIDE]]`·`[[TABLE_ROW]]`로 시작하는 문단을 쓰면 서버가 구조 마크업으로 해석했습니다. 시트는 활성 시트만 저장되었고, `convertedBase64`가 없어도 "문서 저장 완료"를 표시했으며, 파일명은 항상 `EasyOffice_<모듈>_문서.<확장자>`였습니다.
- **Decision (`PD-073`)**
  1. **구조화 저장 페이로드 (`crates/easyoffice-app/ui/{save-payload.js,workspace-tools.js,app.js}`)**: `convert_document_format` JSON에 텍스트 마크업 `body` 대신 다음 필드 하나를 보냅니다. IPC 명령 수(15종)는 그대로입니다.
     - `blocks`: 문서 순서의 `{"type":"paragraph","runs":[{"text","bold","italic","underline","strike"}],"align","heading","list"}`와 `{"type":"table","rows":[[{"text","value"}]]}`. 런 서식은 `strong/b`·`em/i`·`u/ins`·`s/strike/del` 태그와 인라인 `font-weight`(600 이상)·`font-style`·`text-decoration`에서, 정렬은 블록 `text-align` 또는 편집기 정렬에서, 제목 수준은 `h1`~`h6`, 목록은 `ul`/`ol > li`에서 읽습니다. 표 수식 셀(`=SUM|AVERAGE|COUNT|MAX|MIN|PRODUCT(ABOVE|BELOW|LEFT|RIGHT)`)은 계산값을 `value`로 함께 보냅니다. PDF 모듈도 평문 문단 블록으로 보냅니다.
     - `sheets`: 시트 탭 순서의 모든 시트 `{"name","cells":[{"ref":"A1","raw"}]}`(`raw`는 수식이 있으면 수식, 없으면 값). 시트 이름은 수식이 참조하는 시트 키입니다.
     - `slides`: `{"title","bullets":[...],"notes"}`.
  2. **서버 구조화 경로 (`crates/easyoffice-app/src/convert_structured.rs`, `convert_ipc.rs`)**: `blocks`/`sheets`/`slides`가 있으면 모든 문자열을 그대로의 내용으로 취급하고 마크업으로 해석하지 않습니다. 형식 오류(알 수 없는 블록 유형·정렬·목록, 0~6 밖의 제목 수준, 문자열이 아닌 런 텍스트, 중복·빈 시트 이름, 잘못된 셀 주소)는 `InvalidArgument`로 거부하고, 항목 수(50만)·시트 수(256)·문서용 값 표 셀 수(200만)에 상한을 둡니다. 셀의 `value`가 있으면 그 값을, 없으면 `evaluate_table_formulas`로 서버에서 계산한 값을 씁니다. `sheets`는 모든 시트를 `WorkbookEngine`에 올려 시트 간 참조까지 계산한 시트별 수식 캐시로 `pack_xlsx_multi_with_formula_cache`에 전달하고(ODS는 `pack_ods_multi`), 문서 형식 대상이면 시트 이름 제목 + 계산값 표로 기록합니다. 구조화 필드가 없을 때만 기존 `body` 마크업을 하위 호환으로 해석합니다.
  3. **서식 작성기 (`crates/easyoffice-formats/src/{rich_document.rs,rich_writers.rs,hwpx_codec.rs,hwpx_parts.rs}`)**: `RichDocument`를 블록 순서대로 기록합니다. DOCX는 런 `w:rPr`(`w:b`/`w:i`/`w:strike`/`w:u`), 문단 `w:pPr`(`w:pStyle` `Heading1`~`Heading6`·`ListBullet`/`ListNumber`, `w:jc`)와 이를 정의하는 `word/styles.xml`(관계·콘텐츠 형식 포함)을, HWPX는 서식 조합마다 `charPr`(`hh:bold`/`hh:italic`/`hh:underline type="BOTTOM"`/`hh:strikeout shape="SOLID"`, 제목 글자 크기)과 정렬·들여쓰기마다 `paraPr`, 제목 수준마다 `개요 N` 스타일을 `header.xml`에 추가합니다. ODT는 자동 스타일(`fo:font-weight`, `fo:font-style`, 밑줄·취소선, `fo:text-align`)과 `text:h text:outline-level`, RTF는 `\b`·`\i`·`\ul`·`\strike`·`\qc` 등 제어어(디코더는 이 제어어를 걸러 냄), Markdown은 `**`·`*`·`~~`·`<u>`·`#`(사용자 텍스트의 Markdown 특수 문자는 역슬래시 이스케이프), TXT·PDF는 순서 보존 텍스트를 기록합니다. 그 외 형식(DOC, HWP 5.0, 시트·슬라이드 대상)은 평탄화 모델로 기록합니다.
  4. **고정 기관명 제거**: 저장 페이로드에 발신 기관을 넣지 않으며, 구조화 경로의 HWPX `content.hpf`에는 `officialAgency` 메타가 기록되지 않습니다.
  5. **저장 결과 표시와 수정 상태 (`workspace-tools.js`, `app.js`)**: IPC 오류, 빈 `convertedBase64`, 다운로드 시작 실패를 모두 `문서 저장 실패 (<형식>): <사유>`로 표시하고 `false`를 반환하며, 이때는 수정 상태 기준선(PD-072)을 갱신하지 않습니다. 변환기 결과 상자도 내려받지 못하면 그 사실을 덧붙입니다.
  6. **파일명**: 모듈의 문서 탭 제목에서 확장자를 떼고 Windows/macOS 금지 문자(`<>:"/\|?*`, 제어 문자)를 `_`로 바꾸며, 앞뒤 점·공백 제거, 예약 장치 이름(`CON`, `PRN`, `AUX`, `NUL`, `COM1`~`9`, `LPT1`~`9`) 앞 `_`, 120자 상한을 적용합니다. 탭이 없거나 결과가 비면 `EasyOffice_<모듈>_문서`를 씁니다. 같은 이름을 서버 `title`로도 보냅니다.
  7. 새 UI 모듈 `save-payload.js`를 `--serve` 정적 자산, 자가 진단 자산 무결성 검사, 저장소 필수 파일 목록에 추가하고, 오프라인 테스트 백엔드(`scripts/test-support/offline-rpc.js`)도 구조화 필드를 받습니다.
- **Why** 저장한 파일에 편집한 서식·표 위치·표 계산값·모든 시트가 남아야 하고, 사용자 입력은 어떤 내용이든 문서 구조를 바꿀 수 없어야 하며, 실패를 성공으로 표시하거나 매번 같은 이름으로 사본을 쌓지 않기 위함입니다.
- **Trade-offs**
  1. 글꼴 이름·크기·색상, 셀 병합, 이미지는 아직 구조화 페이로드에 포함하지 않습니다(제목 수준의 크기만 반영). **→ 글꼴·크기·글자 색·형광펜은 PD-077(#134)에서 해소되었습니다(런 스키마 `sizeHalfPt`/`font`/`color`/`highlight`). 셀 병합·이미지·표 셀 안 서식은 여전히 제외입니다.**
  2. DOC·HWP 5.0 바이너리와 시트·슬라이드 대상은 서식 없이 평탄화되며, 이 경로의 표는 여전히 문단 뒤에 기록됩니다.
  3. HWPX 제목은 번호 매기기 정의가 필요한 `OUTLINE` 문단 머리 대신 `개요 N` 스타일과 굵은·큰 글자 모양으로 표현합니다.
  4. 서버는 시트 셀 값을 다시 계산하므로 WebUI에 표시된 값과 엔진 계산값이 다르면 엔진 값이 저장됩니다.
- **Alternatives rejected**
  - 기존 마크업에 이스케이프 규칙 추가: 줄 단위 텍스트 프로토콜은 런 서식·블록 순서를 표현할 수 없고 주입 경계가 계속 문자열 규칙에 의존하므로 기각.
  - 편집기 HTML을 그대로 전송해 서버에서 파싱: 서버에 HTML 파서와 정화 규칙이 필요하고 공격 표면이 커져 기각.
  - 새 IPC 명령 추가: 15종 명령 계약을 유지하기 위해 기존 `convert_document_format` 페이로드를 확장.
- **Verified by** `cargo test -p easyoffice-app --test structured_save_fidelity`(DOCX/HWPX 서식 요소·표 순서·계산값·마크업 비해석·고정 기관명 부재, ODT/RTF/Markdown 서식, 다중 시트 XLSX 시트 간 캐시, 형식 오류 거부), `crates/easyoffice-formats/src/{rich_document.rs,rich_writers.rs}` 단위 테스트, `node --test scripts/test-webui-save-fidelity.mjs`(블록 페이로드, 전 시트 전송, 저장 실패 표시, 파일명 정리), `node scripts/check-repo-hygiene.mjs`, `cargo run -p easyoffice-app -- --self-test`
- **Revisit when** 글꼴·크기·색상·셀 병합·이미지 저장을 추가하거나, DOC/HWP 5.0 바이너리 작성기가 문단 서식과 표 위치를 지원하게 될 때.

## 49. Windows 설치형 데스크톱 앱: WebView2 셸·프로세스 내 IPC·NSIS 설치 프로그램 (PD-074, #119)

- **Status** Implemented
- **Context** 제품은 서버 없이 설치해 쓰는 오프라인 데스크톱 프로그램이지만, Windows 배포물은 `easyoffice-app.exe --serve 8787`을 명령줄에서 실행한 뒤 브라우저로 접속하는 CLI 아카이브뿐이었습니다(#119). 데스크톱 창 의존성이 없었고 UI는 `fetch("/api/ipc")`로만 엔진을 호출했으며, 설치 프로그램·시작 메뉴 등록이 없었습니다. 그런데 `lib.rs` 주석("Tauri v2 compatible"), IPC 오류 문구("Tauri IPC 명령"), CI 작업 이름("Tauri Shell")은 데스크톱 셸이 있는 것처럼 표기했습니다.
- **Decision (`PD-074`)**
  1. **데스크톱 셸 크레이트 (`crates/easyoffice-desktop`)**: Tauri v2(WebView2) 바이너리 `EasyOffice.exe`를 추가합니다. `tauri.conf.json`의 `frontendDist`(`../easyoffice-app/ui`)를 빌드 시 실행 파일에 포함하고(`devUrl` 없음), 창은 내장 출처만 표시합니다(`on_navigation`이 `tauri.localhost` 외 이동 거부). `--serve` 서버 모드는 개발·미리보기 경로로 유지합니다.
  2. **프로세스 내 IPC (`ipc_dispatch`)**: 셸 명령 하나가 `EasyOfficeIpcRouter::dispatch`의 15종 명령을 그대로 호출해(블로킹 작업 스레드에서 실행) JSON 문자열을 돌려줍니다. 실패는 `--serve`의 HTTP 400 본문과 같은 `{"status":"error","message"}`로 거부해 UI의 오류 표시 경로가 두 전송 방식에서 같게 동작합니다. IPC 명령 수(15종)는 바뀌지 않습니다.
  3. **UI 브리지 (`ui/desktop-bridge.js`)**: `window.__TAURI_INTERNALS__.invoke`가 있으면 `invoke("ipc_dispatch", {command, payload})`를, 없으면 기존 `fetch("/api/ipc")`를 씁니다. 전역 셸 API(`withGlobalTauri`)는 노출하지 않습니다. 새 모듈은 `--serve` 정적 자산, 자가 진단 자산 검사, 저장소 필수 파일 목록에 추가합니다.
  4. **저장 (`save_document_file`)**: WebView2의 Blob 다운로드는 알림 없이 실패할 수 있으므로, 셸에서는 Rust가 기본 저장 대화상자(`tauri-plugin-dialog`, 주 창에 모달)를 열고 사용자가 고른 경로에 임시 파일 기록 후 이름 바꾸기로 씁니다. WebView는 파일 이름과 Base64 바이트만 보내며 경로를 지정할 수 없습니다(파일 이름은 마지막 경로 요소만 사용, 확장자 영숫자 1~8자, 256MiB 상한). 결과는 `saved`(경로 포함)·`cancelled`·오류이며, UI는 `문서 저장 완료 (<경로> …)`, `문서 저장 취소 (<형식>): 저장 위치를 선택하지 않았습니다`, `문서 저장 실패 (<형식>): <사유>`로 표시하고 취소·실패 시 수정 상태 기준선(PD-072)을 갱신하지 않습니다. 변환기 결과 상자도 같은 경로를 씁니다. `--serve` 미리보기는 Blob 다운로드를 유지합니다.
  5. **보안 경계**: CSP는 WebUI와 같은 `default-src 'self'`에 셸 IPC 출처만 더한 `connect-src 'self' ipc: http://ipc.localhost`입니다. `build.rs`의 `AppManifest`가 두 앱 명령을 기본 거부로 선언하고 `capabilities/main-window.json`이 `main` 창에만 `allow-ipc-dispatch`·`allow-save-document-file`을 허용합니다. core·shell·fs·http·dialog 플러그인 권한은 WebView에 주지 않습니다(대화상자는 Rust에서만 엽니다). `scripts/check-repo-hygiene.mjs`가 CSP 정확값, `devUrl`·전역 API·CSP 수정 비활성화 금지, 파일 연결 미등록, 두 권한 외 권한 부여 금지를 검사합니다.
  6. **설치 프로그램**: Tauri 번들러로 NSIS 설치 프로그램(제품명 EasyOffice, 버전은 워크스페이스 `1.0.0` 또는 릴리스 태그, 사용자 단위 설치, 시작 메뉴 바로 가기, 제거 프로그램, 한국어·영어)을 만듭니다. 아이콘은 `scripts/generate-desktop-icons.mjs`가 파비콘 디자인(`ui/favicon.svg`, 라벤더 `#5C6BC0` 문서 글리프)을 8×8 슈퍼샘플링으로 직접 래스터화해 PNG·다중 크기 ICO로 생성하고 저장소에 커밋합니다(`--check`로 CI에서 일치 검증). 문서 파일 연결은 파일 열기(#111)가 없으므로 등록하지 않습니다. WebView2 런타임이 없는 PC에서만 설치 프로그램이 부트스트래퍼를 내려받습니다.
  7. **CI·릴리스**: macOS·Windows의 엔진 게이트는 `cargo clippy/test --workspace --exclude easyoffice-desktop`으로 같게 실행하고, Windows 작업("Verify Rust Core & Tauri Shell")이 데스크톱 셸 Clippy·단위 테스트, 프리빌트 Tauri CLI(`npx @tauri-apps/cli@2 build --bundles nsis`, 매 실행 소스 컴파일 회피)로 설치 프로그램 빌드, `EasyOffice.exe --smoke`(실제 창에서 내장 WebUI가 `get_workspace_bootstrap` 왕복을 마치면 종료 코드 0, 리본 제거(PD-098) 뒤로는 부트스트랩과 한글 편집기·두 번째 창 왕복으로 판정)를 수행합니다. 릴리스는 `EasyOffice-<version>-windows-x64-setup.exe`를 주 자산으로, CLI 프리뷰 zip을 보조 자산으로 올리고 두 파일의 SHA-256을 `SHA256SUMS-windows-x64.txt`에 기록합니다. 워크스페이스 `default-members`는 엔진 4개 크레이트로 두어 일반 `cargo build`/`cargo test`에는 셸이 포함되지 않습니다.
  8. **표기 정리**: `lib.rs` 라우터 주석과 오류 문구("알 수 없는 IPC 명령입니다"), self-test 주석을 실제 구현에 맞추고, CI 작업 이름 "Tauri Shell"은 이제 실제로 셸을 빌드·실행하므로 유지합니다. `DesktopPlatform::WindowsWebView2`("Windows (WebView2)")는 데스크톱 셸이 생겨 실제와 일치합니다.
  9. **MSRV**: 엔진 크레이트는 워크스페이스 MSRV 1.85를 유지하고, 데스크톱 셸 크레이트만 Tauri 2.x(tauri·tauri-build·tauri-plugin-dialog)가 요구하는 `rust-version = "1.90"`을 명시합니다. 위생 가드는 이 크레이트에 한해 1.85 이상의 명시값을 요구합니다.
- **Why** Windows 사용자가 명령줄·브라우저 없이 설치해 시작 메뉴에서 여는 오프라인 앱을 받고, 셸 안에서도 기존 WebUI 보안 경계(CSP, 원격 콘텐츠 금지)와 15종 IPC 계약을 그대로 유지하며, 저장 성공·취소·실패를 정직하게 표시하기 위함입니다.
- **Trade-offs**
  1. 데스크톱 셸 빌드에는 MSVC 툴체인(Visual Studio Build Tools)과 Node.js(Tauri CLI)가 필요합니다. MinGW 바이너리 유틸리티(`dlltool`이 호출하는 어셈블러)가 없는 windows-gnu 환경에서는 셸 크레이트를 빌드할 수 없으므로 엔진 게이트에서 제외하고 Windows CI에서 검증합니다.
  2. 설치 프로그램은 코드 서명되지 않아 SmartScreen 경고가 표시될 수 있습니다.
  3. 저장 대화상자가 열려 있는 동안 작업 큐의 다른 편집 작업은 대기합니다.
  4. WebView2 런타임이 없는 오프라인 PC에서는 설치 시 런타임을 받을 수 없습니다(Windows 11과 최신 Windows 10에는 기본 포함).
- **Alternatives rejected**
  - 셸에서 루프백 HTTP 서버를 띄우고 WebView로 접속: 포트 점유·다른 로컬 프로세스의 접근 표면이 남아 기각.
  - 엔진 명령마다 셸 명령 15개 등록: 라우터 계약이 두 곳으로 갈라지고 권한 목록이 커져 단일 `ipc_dispatch`로 대체.
  - WebView에 `dialog:allow-save`·`fs:allow-write-file` 권한 부여 후 JS에서 경로 지정: WebView가 임의 경로 쓰기 기능을 갖게 되어 기각. 경로는 Rust가 연 대화상자에서만 나옵니다.
  - MSI(WiX) 설치 프로그램: 사용자 단위 설치·관리자 권한 불필요 측면에서 NSIS가 단순해 우선 NSIS만 제공.
  - 지원 확장자 파일 연결 동시 등록: 셸이 아직 파일을 열지 못해 더블클릭 시 빈 작업 공간이 열리는 오해를 만들어 #111로 미룸.
- **Verified by** `node --test scripts/test-webui-desktop-bridge.mjs`(가짜 `__TAURI_INTERNALS__`로 `ipc_dispatch` 단일 경로·문자열 페이로드·`fetch` 미사용, 셸 거부와 HTTP 400의 동일 표시, 네이티브 저장 명령 사용·경로 미전송·Blob 미사용, 저장 취소·실패 표시, 변환기 저장, 셸 부재 시 HTTP·Blob 유지), 기존 WebUI 스위트 7종, `crates/easyoffice-desktop` 단위 테스트(파일 이름 정리·Base64·원자적 쓰기·직렬화 형태·탐색 출처·스모크 상태 전이), Windows CI의 셸 Clippy·NSIS 빌드·`--smoke`, `node scripts/generate-desktop-icons.mjs --check`, `node scripts/check-repo-hygiene.mjs`, `cargo run -p easyoffice-app -- --self-test`
- **Revisit when** 파일 열기(#111)가 구현되어 파일 연결을 등록할 때, 코드 서명 인증서를 도입할 때, 또는 macOS·Linux에도 같은 셸을 배포하기로 할 때.

## 50. WebUI 파일 열기·가져오기와 실제 원본 파일 기반 변환 (PD-075, #111)

- **Status** Implemented
- **Context** WebUI에는 파일 입력도 `FileReader`도 없어 사용자가 자신의 문서를 열 수 없었고, 통합 포맷 변환기는 편집기 텍스트(`body`)만으로 출력을 만들어 "DOCX → HWPX"가 실제 DOCX를 읽지 않았습니다(#111). 기존 디코더(`UniversalFormatConverter::decode`)는 문단과 표를 따로 평탄화하므로 서식·표 위치·모든 시트를 편집기 모델로 되돌릴 수 없었습니다.
- **Decision (`PD-075`)**
  1. **IPC 설계 — 명령 추가 없음 (`crates/easyoffice-app/src/{convert_open.rs,convert_ipc.rs}`)**: IPC 명령 수는 15종 그대로이며 `convert_document_format` 페이로드를 확장합니다. `sourceBase64`(원본 바이트)가 있으면 편집기 텍스트를 쓰지 않습니다. `"action":"decode"`는 구조화 편집기 모델(`blocks`/`sheets`/`slides`, #104 저장 페이로드와 같은 모양)과 `format`·`module`·`kind`·`itemCount`·`warnings`를 돌려주고, `action` 생략 또는 `"convert"`는 같은 디코딩 결과를 `targetFormat`으로 인코딩합니다(응답의 `sourceBytes`는 실제 원본 크기, `warnings` 추가). 새 명령 대신 확장을 택한 이유는 15종 계약(해시·문서·자가 진단)을 유지하고, 변환과 열기가 같은 디코딩·보안 경로를 공유하게 하기 위해서입니다. `decode` 요청의 `maxRows`/`maxCols`는 편집기 격자(WebUI는 1000행 × 8열) 밖의 셀을 응답에서 제외하고 개수를 `warnings`로 알립니다.
  2. **형식 판별과 디코딩 (`crates/easyoffice-formats/src/{rich_readers.rs,rich_read_*.rs,rich_xml.rs}`)**: `UniversalFormatConverter::decode_rich`는 ZIP·OLE2·PDF·RTF는 매직 바이트로, 텍스트 형식(CSV/TSV/MD/TXT)만 확장자로 결정합니다(예: `.docx`로 이름 붙은 XLSX는 XLSX로 열림, 바이너리 확장자의 일반 텍스트는 서명 오류로 거부). 모든 ZIP 경로는 `ZipArchiveCodec::unpack`(항목 수·크기·압축 비율 상한, 경로 순회 거부)과 `validate_zip_xml_entries`/`XmlSanitizer::validate_safe_xml`(DOCTYPE·외부 엔티티·XXE 거부)을 읽기 전에 거치며, 새 XML 토크나이저는 엔티티를 해석하지 않고 이벤트 수에 상한(600만)을 둡니다.
  3. **형식별 보존 범위**: 아래 표와 같습니다. 모델이 표현하지 못하는 항목은 응답 `warnings`로 사용자에게 표시합니다.
     - **DOCX**: 런 굵게·기울임·밑줄·취소선(`w:b`/`w:i`/`w:u`/`w:strike`, `val=0/none`은 해제), 정렬(`w:jc`), 제목 수준(`styles.xml` 스타일 이름 `heading N`/`제목 N`/`Title`, `w:outlineLvl`), 목록(`ListBullet`/`ListNumber` 스타일, `w:numPr` → `numbering.xml`의 첫 수준 `numFmt`로 글머리/번호), 본문 중간 표(셀 안 여러 문단은 줄바꿈으로 결합, 중첩 표는 바깥 셀로 평탄화), `w:br`/`w:tab`. 삭제된 변경 내용(`w:del`)·변경 이력 속성·`mc:Fallback`은 건너뜁니다. 제외: 그림·도형·OLE, 머리말·꼬리말, 글꼴·크기·색, 각주, 셀 병합.
     - **HWPX/HWTX**: `hh:charPr`(굵게·기울임·밑줄·취소선), `hh:paraPr`(정렬, `hh:heading` 글머리/번호, 쓰기 도구의 목록 들여쓰기 2000), 스타일 이름 `개요 N`·`Outline N` 또는 `OUTLINE` 수준의 제목, `hp:tbl` 표 위치, `hp:tab`/`hp:lineBreak`. 제목 스타일의 기본 굵기는 사용자 서식으로 취급하지 않습니다. 제외: 그림·도형·수식·머리말/꼬리말, 글꼴·크기·색, 번호 매기기 정의, 번호 목록(HWPX 쓰기는 글머리만 표현), 양쪽 정렬과 기본값 구분.
     - **ODT**: 자동 스타일의 `fo:font-weight`/`fo:font-style`/밑줄/취소선, `fo:text-align`, `text:h text:outline-level`, `text:list`와 쓰기 도구의 목록 여백, 표 위치. 제외: 그림·도형, 이름 있는 스타일(`styles.xml`) 상속, 각주.
     - **Markdown/TXT**: Markdown은 ATX 제목, 글머리·번호 목록, 파이프 표, 인용(문단), 코드 블록(일반 문단), `**`·`*`/`_`·`~~`·`<u>`, 링크·그림은 표시 텍스트, 역슬래시 이스케이프를 읽습니다(HTML·각주·중첩 목록 수준은 제외). TXT는 비어 있지 않은 줄마다 한 문단입니다. 첫 `#` 제목은 본문 블록으로 유지하며, Markdown 쓰기는 문서가 `#` 제목으로 시작하면 제목 줄을 중복해서 쓰지 않습니다.
     - **DOC·HWP·HWT·RTF·PDF**: 서식과 표 위치 없이 본문 문단 뒤에 표를 순서대로 붙입니다(`UniversalFormatConverter::decode` 경로). PDF는 텍스트만 추출하며 PDF 모듈 카드에 적재합니다.
     - **XLSX/XLS/ODS**: 모든 시트(이름·순서)와 수식(`=...`)·값. 수식 결과는 엔진이 다시 계산하며 캐시 값·차트·그림은 가져오지 않습니다(날짜는 일련번호). 셀 서식(굵게·기울임·밑줄·취소선·크기)과 열 너비는 PD-089로 XLSX/ODS에서 가져옵니다. **CSV/TSV**: UTF-8(BOM은 제거)만 지원하며 단일 `Sheet1`입니다(CP949 등 비UTF-8은 거부).
     - **PPTX/PPSX/PPT/ODP**: 슬라이드 제목·글머리·발표자 노트. 제외: 도형·이미지·서식·전환.
  4. **WebUI 열기 (`crates/easyoffice-app/ui/file-open.js`, 새 모듈)**: 파일 메뉴 "열기…"와 `Ctrl+O`(`ribbon-catalog.js`의 단일 단축키 표에 `file-open`으로 등록되어 라벨과 핸들러가 함께 결정됨, 이전 테스트의 "브라우저 예약 키" 목록에서 `Ctrl+O` 제외), 작업 영역 끌어 놓기(변환기 모듈이 활성이면 변환 원본으로 선택)를 지원하며 숨김 `<input type="file">`의 `accept`는 지원 확장자와 일치합니다. 모든 경로는 단일 진입점 `openDocumentFromBytes(fileName, Uint8Array)`(`window.__EASYOFFICE_WORKSPACE_API__`)로 모이며 데스크톱 셸도 같은 함수를 호출합니다. 디코딩된 모델은 `docTabs`(`blocks` → `h1`~`h6`/`ul`·`ol`/`p`+`strong`·`em`·`u`·`s`/표 노드), `sheets`(값/수식 + `evaluate_sheet`로 수식 결과 계산, 기본 `Sheet1`/`Sheet2` 탭 제거), `slides`(`lower` = 글머리 줄바꿈 결합), PDF 카드(제목 `h3` + 본문 문단, 기존 워터마크·스탬프·양식 값 초기화)로 적재됩니다.
  5. **되돌리기·확인·저장 연계**: 열기는 `captureWorkspaceSnapshot`으로 이전 상태를 실행 취소 스택에 넣은 뒤 `restoreWorkspaceSnapshot`으로 적용하는 하나의 작업이며, 탭 제목(파일 이름, 스냅샷의 `tabTitles`)도 같은 상태에 포함되어 실행 취소/다시 실행과 자동 저장 복원(PD-072)이 그대로 동작하고, 저장 파일명(PD-073)은 이 탭 제목을 따릅니다. 대상 모듈에 저장하지 않은 편집이 있으면(모듈별 기준선 `moduleFingerprint`, 초기·열기·저장 시 갱신) `confirm`으로 확인하고 취소하면 아무것도 바꾸지 않습니다. 디코딩 실패(손상·미지원 확장자·보안 차단)와 크기 초과는 `파일 열기 실패: …`를 표시하고 문서 상태·변환기 결과 상자를 바꾸지 않습니다.
  6. **크기 한도와 HTTP 본문 (#111 체크리스트)**: 원본은 Base64로 4/3 팽창하므로 WebUI 한도를 파일당 10 MiB(Base64 약 13.4 MiB < `MAX_IPC_HTTP_BODY_BYTES` 16 MiB), 서버 디코딩 한도를 12 MiB로 두었습니다. 한도를 넘으면 요청 전에 `파일이 너무 큽니다 (…). 최대 10 MiB까지 열 수 있습니다`를 표시합니다. 분할 전송이나 본문 한도 상향은 하지 않습니다. 시트 편집 격자(1000행 × 8열) 밖의 셀은 가져오지 않고 개수를 알립니다.
  7. **변환기 원본 파일 (`index.html`, `app.js`)**: 변환기 패널의 "원본 파일"을 고르면 `sourceBase64`/`fileName`/`title`(파일 이름에서 확장자 제거)을 보내 파일 바이트를 변환하고 다운로드 이름도 그에 따릅니다. 선택하지 않으면 기존처럼 편집기 구조화 내용을 변환합니다.
  8. 새 UI 모듈 `file-open.js`를 `--serve` 정적 자산, 자가 진단 자산 무결성 검사, 저장소 필수 파일 목록, 하니스 스크립트 목록에 추가하고, 새 테스트 `scripts/test-webui-file-open.mjs`를 CI의 두 `node --test` 줄에 추가했습니다. 오프라인 테스트 백엔드(`offline-rpc.js`)는 `sourceBase64`를 받아 텍스트 형식은 실제로 디코딩하고 바이너리는 `EOFIXTURE:`+JSON 모델 또는 손상 오류로 응답합니다.
- **Why** 사용자의 실제 파일을 열어 편집·저장·변환할 수 있어야 하고, 열기·변환이 저장 경로와 같은 구조화 모델·보안 가드를 공유해야 라운드트립이 보장되며, 새 IPC 명령 없이 기존 계약을 유지하기 위함입니다.
- **Trade-offs**
  1. 형식별 손실(위 3항): 글꼴·크기·색·그림·도형·머리말·각주·셀 서식은 열고 저장하면 사라집니다. DOC·HWP 5.0·RTF·PDF는 서식과 표 위치를 잃습니다. **→ DOCX·HWPX·ODT·RTF의 글꼴·크기·글자 색·형광펜과 RTF의 서식·표 위치는 PD-077(#134)에서 가져옵니다.**
  2. 편집기 시트는 XLSX 주소 범위(1,048,576행 × 16,384열)까지 적재합니다(`PD-078`로 확장; 이전 A~H·1000행 제한 제거). CSV는 UTF-8만 지원합니다.
  3. HWPX의 번호 목록은 글머리로, 양쪽 정렬은 기본값으로 다시 읽힙니다. 문서 제목(`dc:title`)은 편집기에 표시하지 않고 저장 시 탭 제목(파일 이름)으로 대체됩니다.
  4. 한 번에 한 파일만 열며(여러 개를 끌어 놓으면 첫 번째만), 열기 직후의 상태도 자동 저장·이탈 경고 대상(수정됨)입니다.
- **Alternatives rejected**
  - 새 IPC 명령(`open_document`) 추가: 명령 수 15종 계약(해시·README·아키텍처·자가 진단)을 모두 바꾸어야 하고 변환과 열기의 디코딩 경로가 갈라져 기각.
  - 기존 `decode`(평탄화 모델)로 편집기에 적재: 서식·표 위치·다중 시트가 사라져 열기→편집→저장 라운드트립이 불가능해 기각.
  - 브라우저에서 파일을 직접 파싱: JS ZIP/XML 파서와 별도 보안 가드가 필요하고 변환과 로직이 중복되어 기각.
  - 분할 전송: 10 MiB 한도에서 단일 요청으로 충분하고 서버 상태·재조립 복잡도가 늘어 기각.
- **Verified by** `cargo test -p easyoffice-formats --test rich_decode_roundtrip`(DOCX·HWPX·ODT·Markdown·TXT·XLSX 다중 시트 수식·BOM CSV·PPTX 디코드→인코드→디코드 동치, 레거시·PDF 평문 적재, 매직 바이트 우선, Word 스타일 제목·번호 목록, DOCX/XLSX/PPTX/HWPX/ODT XXE·ZIP 폭탄·경로 순회·손상 입력 거부), `cargo test -p easyoffice-app convert_open`(decode 응답·모듈 라우팅·격자 절단·원본 바이트 변환·보안 거부), `crates/easyoffice-formats/src/{rich_xml.rs,rich_read_text.rs}` 단위 테스트, `node --test scripts/test-webui-file-open.mjs`(`Ctrl+O`·메뉴·accept, 입력·끌어 놓기, 모듈 라우팅, 수정 확인, 크기·손상 오류, 탭 제목, 실행 취소/다시 실행, 자동 저장 복원, 열기→저장 라운드트립, 변환기 원본), 실서버 `--serve 8795` + 헤드리스 Chrome에서 DOCX·XLSX 열기, `node scripts/check-repo-hygiene.mjs`, `cargo run -p easyoffice-app -- --self-test`
- **Revisit when** 글꼴·그림·머리말 가져오기, 시트 격자 확장, 비UTF-8 CSV(CP949) 지원, 다중 문서 탭, 데스크톱 셸의 파일 연결 열기(PD-074)가 추가될 때.

## 51. rhwp upstream 경계: 읽기 전용 서브모듈·위생 가드 제외·라이선스 고지 (PD-076, #128)
- **Decision (`PD-076`)**
  1. **읽기 전용 upstream**: rhwp는 `third_party/rhwp`의 서브모듈(`.gitmodules`의 `https://github.com/edwardkim/rhwp.git`, `shallow = true`)로만 가져오며 EasyOffice가 수정하지 않습니다. 고정 대상(커밋 `1a76570e833917d15817415a53c09ad61ab3203f`, 버전 0.8.7, 툴체인 1.93.1)과 sparse 목록은 `config/rhwp-upstream.json`이 유일한 기준입니다. 필요한 변경은 EasyOffice 쪽 오버레이로 처리하고(이후 단계), 서브모듈 안에서 고치지 않습니다.
  2. **얕은·부분 체크아웃**: upstream 전체는 약 5.3 GB(문서·PDF·샘플·도구)이므로 `scripts/checkout-rhwp.mjs`가 `--depth 1`과 `--filter=blob:none`으로 고정 커밋만 받고 sparse 목록(소스·`rhwp-studio`·라이선스 파일)만 작업 트리에 둡니다. AI 도구 규칙 파일과 `.mcp.json`은 목록에 없어 디스크에도 생기지 않습니다. CI의 기본 체크아웃은 서브모듈을 받지 않습니다(`actions/checkout` 기본값 유지).
  3. **fail-closed 검증 (`scripts/verify-rhwp-upstream.mjs`, Node 내장 모듈만)**: 설정 형식, `.gitmodules`(허용 항목은 고정 URL·경로·`shallow = true` 하나뿐), `git ls-files -s`의 gitlink(모드 `160000`, 커밋 = 설정), 작업 트리가 있으면 HEAD·원격 URL·`Cargo.toml` 버전·`rust-toolchain.toml` 채널 일치와 `git status --porcelain` 비어 있음(추적·미추적 변경 모두 실패)을 확인합니다. 체크아웃이 없으면 상위 저장소 기록만 검사하므로 CI 기본 상태에서 통과합니다. 위생 가드가 같은 로직을 호출합니다.
  4. **위생 가드 제외 범위 (`scripts/check-repo-hygiene.mjs`)**: 디스크 순회에서 루트 기준 경로 `third_party/rhwp` 하나만 제외합니다(이름 기준 `IGNORED_DIRS`와 별도 집합 `IGNORED_ROOT_PATHS`). upstream의 AI 도구 문서·브랜드 토큰·수천 개 JS 파일은 EasyOffice가 작성하지 않았고 위 3항으로 무결성이 고정되기 때문입니다. 그 외 모든 경로는 그대로 검사하고, 금지된 로컬 전용 파일이 EasyOffice 안(루트뿐 아니라 하위 경로)에서 추적되면 계속 실패합니다. 새 필수 파일(`.gitmodules`, 설정, 검증·체크아웃 스크립트, 테스트, 고지, 설계 문서)이 계약 목록에 추가되었습니다.
  5. **업데이트 절차**: (a) 새 커밋·버전을 검토한 뒤 `config/rhwp-upstream.json`의 `commit`·`version`·`rustToolchain`·sparse 목록을 수정하고, (b) `third_party/rhwp`를 그 커밋으로 이동해 gitlink를 갱신하며, (c) `node scripts/verify-rhwp-upstream.mjs`와 위생 가드를 통과시키고, (d) `THIRD_PARTY_NOTICES.md`(Rust 의존성·글꼴 고지)를 다시 확인합니다. 설정과 gitlink는 같은 PR에서만 함께 바꿉니다. 이 절차는 `node scripts/update-rhwp.mjs vX.Y.Z [--dry-run] [--accept-overrides]`가 맡습니다: 명시 태그만 받아 원격에서 커밋으로 해석하고, 새 업스트림 파일로 오버라이드 기준 SHA-256을 다시 계산해 바뀐 오버라이드가 있으면 실패하며(사람이 오버라이드를 맞춘 뒤 `--accept-overrides`로만 기준선 갱신), 계획을 모두 세운 뒤 마지막에만 쓰고 실패하면 설정·gitlink를 되돌립니다. 벤더 WASM은 다시 빌드하지 않고 `vendor-rhwp-core.mjs` 실행을 안내합니다(`scripts/test-update-rhwp.mjs`).
  6. **라이선스 고지**: `THIRD_PARTY_NOTICES.md`에 rhwp MIT 고지(저작권 표기 포함)를 두고, WASM에 컴파일되는 Rust 의존성과 글꼴 고지는 자리만 마련해 2단계에서 완성합니다. 참조 앱은 패턴만 따랐고 복사한 코드가 없음을 명시합니다.
  7. 이 단계는 런타임 동작을 바꾸지 않습니다(빌드·UI·IPC 15종 명령 변화 없음). 설계 전문은 `docs/rhwp-integration-design.md`입니다.
- **Why** upstream을 고정·무수정으로 두어야 출처와 라이선스를 증명할 수 있고, 저장소 위생 규칙을 약화시키지 않으면서 수 GB 서드파티 트리가 CI 시간과 규칙 검사를 망치지 않게 해야 하기 때문입니다.
- **Trade-offs**
  1. 위생 가드가 upstream 내용을 검사하지 않으므로 무결성은 커밋 고정과 clean 검사에 의존합니다.
  2. sparse 목록이 부족하면 이후 빌드가 실패하며 2단계에서 확정합니다.
  3. 서브모듈 체크아웃은 별도 스크립트가 필요해 `git submodule update --init` 단독으로는 전체(5.3 GB)를 받게 됩니다.
- **Alternatives rejected**
  - 전체 서브모듈 체크아웃(`submodules: true`): 5.3 GB와 수만 개 파일로 CI·위생 가드가 느려지고 실패해 기각.
  - upstream 소스를 저장소에 복사: 출처 고정·갱신 검증이 어렵고 라이선스 추적이 약해져 기각.
  - 위생 가드에서 금지 토큰 검사를 완화: 기존 규칙을 약화시키므로 기각(제외는 서드파티 경로 하나로 한정).
- **Verified by** `node --test scripts/test-rhwp-upstream.mjs`(미체크아웃 통과, gitlink 변조·URL 변조·버전 불일치·작업 트리 수정 실패, 위생 가드의 루트 금지 파일 추적 실패와 `third_party/rhwp`만 제외), `node scripts/verify-rhwp-upstream.mjs`, `node scripts/check-repo-hygiene.mjs`
- **Revisit when** upstream를 갱신하거나 `vendor/rhwp-core`·`apps/hwp-studio` 산출물이 추가되어 제외 경로를 늘려야 할 때.

## 52. 문서 모듈 글자 단위 런 모델: 선택 범위 글자 크기·글꼴·색·형광펜·굵게·기울임·밑줄·취소선 (PD-077, #134)

- **Status** Implemented
- **Context** 글자 크기를 바꾸면 편집기 전체(`editor.style.fontSize`)가 커지고, 굵게·기울임·밑줄은 커서가 있는 문단 전체의 인라인 스타일로 적용되었으며(`ribbon-history.js`의 `targetEl.style.fontWeight` 등), 글꼴도 편집기 전체에 걸렸습니다(#134). 글꼴·크기·색은 저장 페이로드와 디코더에도 없어(PD-073 트레이드오프 1, PD-075 트레이드오프 1) 열고 저장하면 사라졌습니다. DOCX·ODT·RTF는 HWP 편집기용 라이브러리 대상이 아니므로 DOC 모듈이 자체 글자 단위 런 모델을 가져야 합니다.
- **Decision (`PD-077`)**
  1. **런 엔진 (`crates/easyoffice-app/ui/doc-runs.js`, 새 모듈)**: 런은 `<span class="eo-run">` 하나이며 속성은 `dataset`(`eoSize`=반포인트, `eoFont`, `eoColor`, `eoHighlight`=`RRGGBB`, `eoBold`/`eoItalic`/`eoUnderline`/`eoStrike`=`true`)과 같은 값을 반영하는 CSSOM `style` 속성으로만 기록합니다(`execCommand`·`innerHTML`·사용자 입력으로 만든 스타일 문자열 없음). 값은 먼저 검증합니다(색 `#RRGGBB`/`#RGB`/`rgb()`, 글꼴 제어·따옴표·`;{}<>` 없는 64자 이하, 크기 2~1638 반포인트). `applyFormat(root, range, patch)`는 범위 경계에서 텍스트 노드를 `splitText`로 나누고, 선택된 글자만 런으로 감싸며(기존 `strong`/`em`/`u`/`s`/붙여넣은 `span style` 조상은 `liftOut`으로 쪼개 효과 서식을 런으로 옮김), 같은 서식의 이웃 런 병합·속성 없는 런 풀기·빈 런·빈 텍스트 노드 제거를 수행하며, 문단·표 셀 여러 개에 걸친 선택, 클래스가 있는 구조용 `span`(예: `eo-h-title`)과 링크 범위를 보존합니다. 선택 범위는 텍스트 오프셋으로 복원합니다. 노드 비교는 인덱스 경로로 하고 오프셋은 범위를 담은 컨테이너 안에서만 계산해, 2000문단 전체 선택 적용 63 ms·큰 문서의 작은 선택 1 ms·넓은 선택 조회 13 ms입니다.
  2. **커서만 있을 때의 규칙 (`doc-runs-ui.js`)**: 커서가 단어 한가운데(양쪽이 글자·숫자·`_`)면 그 **단어**에 적용하고, 단어 경계·빈 문단이면 **대기(pending) 서식**으로 두었다가 다음에 입력하는 글자(`beforeinput` `insertText`)에만 적용합니다(커서가 옮겨지면 폐기, 한 글자 입력 후에는 새 런 안에서 이어 입력). 선택도 커서도 없으면 `서식을 적용할 글자를 먼저 선택하세요`를 표시하고 아무것도 바꾸지 않습니다(전체 문서 크기 변경은 없어짐).
  3. **적용 대상**: 글자 크기(입력 칸·▲/▼·메뉴), 글꼴 목록, 글자 색(버튼은 마지막 색을 적용/해제, 옆 `<input type="color">`는 색 선택), 형광펜(같은 방식), 굵게·기울임·밑줄(툴바·리본·`Ctrl+B/I/U`는 모두 `executeTrackedRibbonCommand` → 같은 엔진), 취소선. 문서와 한글 모듈이 같은 편집기를 쓰므로 둘 다 적용되며 이후 HWP 편집기 교체와 무관합니다. 토글은 선택 전체가 이미 켜져 있으면 끄고, 일부만 켜져 있으면 켭니다.
  4. **툴바 상태**: 선택/커서의 서식을 `queryFormat`으로 읽어 굵게·기울임·밑줄·취소선 버튼의 `aria-pressed`(`true`/`false`/`mixed`), 크기 칸(여러 크기면 빈 칸 + `혼합`, 런 없는 글자는 문서 기본 11.0), 글꼴 목록, 색 버튼의 아래 줄을 갱신합니다. 크기·글꼴 컨트롤을 쓴 뒤에는 편집기로 포커스를 돌려 입력과 `Ctrl+Z`가 편집기에서 동작합니다.
  5. **실행 취소·자동 저장**: 서식 동작 하나가 기존 작업 영역 스냅샷 이력의 한 단계입니다(리본 경로는 `executeTrackedRibbonCommand`의 한 스냅샷, 툴바 경로는 `captureWorkspaceSnapshot` → 변경 → `commitDeferredUndoSnapshot`). 변경 알림은 `input`이 아닌 `eo-doc-mutated` 이벤트로 보내 입력 추적기가 두 번째 실행 취소 단계를 만들지 않게 했습니다. 스냅샷 직렬화에 `fontFamily`·`textIndent`·`paddingRight`를 추가해 런의 `dataset`+`style`이 자동 저장·복원됩니다.
  6. **저장·열기 충실도**: #104 구조화 런 스키마에 `sizeHalfPt`(2~1638), `font`, `color`, `highlight`(`#RRGGBB`/`RRGGBB`)를 더했습니다(필드가 없는 기존 페이로드는 그대로 동작, 형식 오류는 `InvalidArgument`). DOCX는 `w:rFonts`(ascii/hAnsi/eastAsia/cs)·`w:color`·`w:sz`+`w:szCs`·이름 있는 형광색은 `w:highlight`, 그 밖의 색은 `w:shd w:fill`(스키마 순서 준수), HWPX는 서식 조합마다 `charPr`의 `height`(1/100pt)·`textColor`·`shadeColor`·`fontRef`와 글꼴 목록(`fontface`), ODT는 자동 텍스트 스타일(`fo:font-size`·`style:font-name`+`office:font-face-decls`·`fo:color`·`fo:background-color`), RTF는 글꼴 표 `{\fonttbl…}`·색 표 `{\colortbl;…}`와 `\f`·`\fs`·`\cf`·`\highlight`(런 끝은 `\plain`)로 기록합니다. 읽기는 #111 `decode_rich` 경로(`rich_read_ooxml`/`_hwpx`/`_odt`)가 같은 속성을 `RichRun`으로 되돌리고, 디코드 응답 JSON의 런에도 같은 필드를 싣습니다. RTF는 줄 단위 평문 디코더 대신 새 `rich_read_rtf.rs`(그룹 스택 기반, `\fonttbl`·`\colortbl`·`\info\title`·`\u`·`\'hh`·`\trowd` 표·정렬·개요 수준, 알 수 없는 대상과 `\*`는 건너뜀, 중첩 256 상한)가 런·표 위치까지 읽습니다. 제목 문단은 제목 기본 크기와 같은 값을, HWPX 본문은 기본 글자 높이와 같은 값을 "상속"으로 보아 `size` 없이 읽습니다. 이 때문에 HWPX 쓰기 기본 글자 높이를 10 pt에서 편집기 기본과 같은 **11 pt(1100)**로 올렸습니다.
  7. **열기 반영 (`file-open.js`)**: 디코드된 런에 크기·글꼴·색·형광펜이 있으면 `doc-runs.js`의 `buildRunSpec`으로 런 노드를 적재해 열기 → 편집 → 저장 → 다시 열기에서 런이 유지됩니다. 저장은 `save-payload.js`가 런 `dataset`과 붙여넣은 `span style`/`font`에서 같은 필드를 읽습니다(안쪽 요소 우선, 인접 같은 서식 병합).
  8. **들여쓰기·눈금자 (`doc-runs-ui.js`)**: `indent-in/out` 메뉴와 눈금자 핀(첫 줄 `textIndent`, 왼쪽 `paddingLeft`, 오른쪽 `paddingRight`, 4 px 단위 드래그, 한 번의 실행 취소)이 선택이 닿는 문단(표 셀 포함, 빈 문단 포함)에만 적용되고 핀 위치는 현재 문단의 값을 따릅니다. 번호·글머리 접두사 토글은 문단을 `textContent`로 다시 쓰지 않고 텍스트 노드만 넣고 빼서 런이 지워지지 않습니다. 탭 정지는 눈금자 핀(`ruler-pin-*`)이 문단 단위 들여쓰기만 담당하며 별도 탭 정지 모델은 아직 없습니다.
  9. 새 UI 모듈 `doc-runs.js`·`doc-runs-ui.js`를 `--serve` 정적 자산, 자가 진단 자산 무결성 검사, 저장소 필수 파일 목록, 하니스 스크립트 목록, `index.html`에 추가하고 새 테스트 `scripts/test-webui-doc-runs.mjs`를 CI의 두 `node --test` 줄에 추가했습니다. IPC 명령 수는 15종 그대로입니다.
- **Why** 서식은 선택한 글자에만 적용되어야 하고, 한 번의 동작은 한 번의 실행 취소여야 하며, 편집한 런이 저장·열기를 거쳐도 남아야 합니다. DOCX·ODT·RTF는 외부 편집기 엔진이 대신해 주지 않으므로 DOM 범위 기반 자체 런 모델을 택했습니다.
- **Trade-offs**
  1. 표 셀 안의 글자 서식은 편집기에서는 적용되지만 저장 모델의 표가 셀 텍스트만 가지므로 파일에는 기록되지 않습니다. 셀 병합·이미지는 여전히 제외입니다.
  2. 하나의 런 모델에는 위·아래 첨자, 자간, 장평, 글꼴 언어 슬롯별 구분이 없습니다(글꼴은 모든 언어 슬롯에 같은 이름으로 기록, 읽을 때는 한글/ASCII 슬롯 우선).
  3. 가져오기는 스타일 상속을 해석하지 않습니다(DOCX `styles.xml`·ODT 이름 있는 스타일의 크기·글꼴은 무시, 런·자동 스타일에 직접 지정된 값만). HWPX는 기본 글자 높이(11 pt)와 검정 글자색, 기본 글꼴과 같은 값은 "미지정"으로 읽습니다. 제목·표 머리 같은 CSS 기본 굵기는 `Ctrl+B`로 끌 수 없습니다.
  4. HWPX 쓰기 기본 글자 높이가 10 pt에서 11 pt로 바뀌어 일반 문서의 본문 크기가 한글 프로그램에서 11 pt로 보입니다.
  5. 마크다운·TXT·PDF·레거시 DOC/HWP 5.0은 글자 크기·글꼴·색을 기록하지 않습니다(굵게 등 기존 서식은 이전과 같음). RTF의 목록 들여쓰기는 글머리/번호 구분 없이 들여쓰기만 읽습니다.
  6. IME 조합 입력(`insertCompositionText`)에는 대기 서식을 적용하지 않습니다(조합 후 한글 입력은 일반 입력 경로).
- **Alternatives rejected**
  - `document.execCommand`(`bold`/`fontSize`/`foreColor`): 폐기 예정 API이며 `<font size=1..7>`·브라우저별 태그를 만들어 포인트 단위 크기와 병합·정규화를 보장할 수 없고 CSP·XSS 규칙의 속성 화이트리스트 검증과 맞지 않아 기각.
  - 블록 요소에 스타일 덧씌우기(기존 방식): 선택 범위에만 적용한다는 요구를 만족할 수 없어 기각.
  - 선택 자체를 `Range.surroundContents`로 감싸기: 여러 문단·표 셀·기존 인라인 요소에 걸치면 예외가 나고 병합 규칙이 없어 기각.
  - 글자 색·형광펜을 자체 팝업 팔레트로 구현: 색상 입력(`<input type="color">`)이 접근성·플랫폼 통합이 좋고 코드가 작아 기각.
  - 새 IPC 명령 추가: 15종 계약을 유지하기 위해 기존 `convert_document_format` 런 스키마를 확장.
- **Verified by** `node --test scripts/test-webui-doc-runs.mjs`(문단 일부 크기·굵게·색 적용 시 나머지 글자 불변, 글꼴·형광펜·기울임·밑줄·취소선, 여러 문단·표 셀 선택, 인접 런 병합·빈 런 제거, 레거시 `strong` 보존, 잘못된 값 거부, 서식 한 번 = 실행 취소 한 단계와 다시 실행 복원, 크기 칸 혼합 표시, 굵게 `mixed` 상태·토글, 단어 적용/대기 서식·다음 입력에만 적용, 저장 페이로드 런 필드, 자동 저장 스냅샷의 런, 문단 단위 들여쓰기·눈금자 핀, 선택 없음 거부), 갱신한 `test-webui-behavior.mjs`·`test-webui-ux-ergonomics.mjs`(선택 범위 기반 서식·실행 취소), `cargo test -p easyoffice-formats --test rich_run_formatting`(DOCX/HWPX/ODT/RTF 크기·글꼴·색·형광펜 런 왕복과 원시 XML 속성, 제목 런 크기 재정의, RTF 평문·표 왕복), `cargo test -p easyoffice-app convert_open`(14 pt·굵게·빨강 런이 DOCX로 저장되어 다시 열면 같은 런·필드, 잘못된 값 거부, 필드 없는 기존 페이로드), 실서버 `--serve 8871` + 헤드리스 Chrome에서 실제 마우스 드래그 선택(`Input.dispatchMouseEvent` 누름/이동/놓음) 후 크기 20 pt 적용(선택한 글자만 커지고 편집기 전체 크기 불변), 실제 `Ctrl+B`·색 버튼·`Ctrl+Z`/`Ctrl+Y` 세 단계 정확히 되돌림, 다중 문단 드래그 후 16 pt, 대기 굵게 후 입력, 2000문단 성능 측정, `node scripts/check-repo-hygiene.mjs`, `cargo run -p easyoffice-app -- --self-test`
- **Revisit when** 표 셀 서식·셀 병합 저장, 위·아래 첨자·자간·장평, 스타일 상속 가져오기, IME 조합 중 서식, 별도 탭 정지 모델, HWP 편집기를 rhwp로 교체할 때.

## 53. 시트 편집 기본기: 준비/편집 모드, 범위 선택·클립보드, 가상화 격자, 숫자 표시 (PD-078, #123)

- **Status** Implemented
- **Context** 실조작 리뷰(#123)에서 시트 셀에 입력하면 기존 값 뒤에 이어 붙고(`920`+`1000`=`9201000`, `=B3`은 수식이 아님), Enter는 포커스만 잃고 방향키가 멈추며, 범위 선택·`Ctrl+C`/`Ctrl+V`가 없고 "선택 집계"가 시트 전체 합계를 보였습니다. 격자는 12행 × 8열 고정이고 열 너비 조절이 없으며, 숫자는 왼쪽 정렬에 `1266.66666666667`처럼 표시되고, 개요의 시트 이름이 탭과 달랐고, SHEET 홈에 문서 전용 명령이 보였습니다.
- **Decision (`PD-078`)**
  1. **두 가지 모드 (`ui/sheet-input.js`)**: *준비*(셀이 선택된 상태)에서 인쇄 가능한 글자나 IME 시작(`Process` 키·`compositionstart`·`beforeinput`)은 *입력 모드(enter)*로 들어가며 **셀 내용을 덮어씁니다**. `F2`·더블클릭은 *편집 모드(edit)*로 들어가며 내용(수식은 원문, 숫자는 원본 값)을 유지하고 캐럿을 끝에 둡니다. `Delete`/`Backspace`(준비 모드)는 선택 범위 전체를 지웁니다. 편집 모드 표시는 `td[data-edit-mode]`·`.editing`이며 캐럿은 편집 중에만 보입니다.
  2. **키 규칙 (문서화된 관례)**: 준비 모드 방향키·`Tab`·`Enter`는 선택을 옮기고(`Shift`는 범위 확장, `Ctrl`+방향키는 데이터 블록 끝으로 점프, `Home`/`Ctrl+Home`/`Ctrl+End`/`PageUp`/`PageDown`), `Ctrl+A`는 데이터가 있는 영역 전체를 선택합니다. 편집 중 `Enter`는 확정하고 아래로(`Shift+Enter` 위로), `Tab`은 확정하고 오른쪽으로(`Shift+Tab` 왼쪽), `Esc`는 취소하고 원래 값을 복원합니다. **입력 모드에서는 방향키가 확정 후 이동**하고, **편집 모드에서는 좌우 방향키가 캐럿을 움직이며** 상하 방향키는 한 줄 입력이므로 확정 후 이동합니다. `isComposing`/`Process` 키는 해석하지 않으므로 IME 조합 중 `Enter`는 확정·이동하지 않고, `Input.insertText`로 들어온 한글도 정상 입력됩니다.
  3. **범위 선택과 집계**: 마우스 드래그, `Shift`+클릭, `Shift`+방향키로 범위를 만들고 `.in-range`(활성 셀은 `.selected-cell`)·머리글 `active-axis`로 표시합니다. 기준(앵커)이 활성 셀이며 수식 입력줄·셀 주소 배지는 앵커를 따릅니다. 상태 표시줄 집계는 **선택 범위만** 합계·평균·개수·최소·최대로 계산합니다(`선택 집계 · 합계 … · 평균 … · 개수 … · 최소 … · 최대 …`). 20,000칸 이하 범위는 칸을 순회하고 더 큰 범위는 모델의 항목만 순회합니다.
  4. **클립보드 (`ui/sheet-clipboard.js`)**: 복사·잘라내기·붙여넣기는 `copy`/`cut`/`paste` 이벤트로 시스템 클립보드와 **TSV**(탭·줄바꿈·따옴표는 TSV 규칙으로 인용)를 주고받고, 도구 모음/메뉴 명령(`clip-copy`·`clip-cut`·`clip-paste`)은 `navigator.clipboard`를 쓰며 실패하면 앱 내부 복사본으로 대체합니다(상태 표시줄에 "앱 내부 클립보드" 표기). 다중 셀 TSV는 선택 범위의 왼쪽 위 칸부터 채우고 시트가 모자라면 늘립니다(단일 값은 선택 범위 전체에 채움, 시트 주소 범위 밖 칸은 알리고 제외). 붙여넣기·지우기·잘라내기는 **하나의 실행 취소 단위**입니다. 클립보드 텍스트가 앱 내부 복사본과 같을 때 수식의 **단순 A1 상대 참조**(범위·`$` 고정 포함)가 복사→붙여넣기 오프셋만큼 이동하며, 범위를 벗어나면 `#REF!`입니다. 다른 시트 참조(`시트!A1`)·따옴표로 감싼 시트 이름 안의 참조처럼 보이는 글자·외부에서 온 텍스트의 수식은 이동하지 않습니다. 클립보드에는 계산값이 아닌 수식 원문이 실립니다.
  5. **가상화 격자 (`ui/sheet-grid.js`)**: 논리 주소는 1,048,576행 × 16,384열(`XFD1048576`)이며 보이는 행·열(+여유 8행·2열)만 `<tr>`/`<td>`로 만들고(전형적 창에서 약 660~800칸), 위·아래 스페이서 행과 열 스페이서로 스크롤 길이를 유지합니다. 행 높이 24px·머리글 26px·행 머리글 56px·기본 열 너비 96px 고정, `table-layout: fixed`라 편집 중에도 열이 흔들리지 않습니다. 머리글은 항상 `position: sticky`로 고정되고(`틀 고정`의 `frozen-header`는 구분선만 추가), 열 머리글 오른쪽 가장자리를 끌면 너비가 바뀝니다(36~720px, 시트별 메모리 보관; 실행 취소·자동 저장에는 포함하지 않으며, 파일 저장·열기에는 `PD-089`로 XLSX/ODS 열 너비로 기록·복원합니다). 레이아웃을 알 수 없는 환경(숨김 창·헤드리스 테스트)에서는 최대 60행 × 40열의 제한된 창을 렌더링합니다. 모델의 SSOT는 `sheetDataStore`이며 `snapshotDomSheetCells`는 렌더된 창을 모델에 병합합니다(창 밖 셀은 모델이 권위). 정렬·조건부 서식·피벗/차트 요약은 모델 기준으로 동작합니다(`ui/sheet-ops.js`). 행 추가(`sheet-insert-table`)·틀 고정·다중 시트 탭·실행 취소/다시 실행·#104 전체 시트 저장은 그대로입니다. 파일 열기 한도(`maxRows`/`maxCols`)는 `1048576`/`16384`로 올렸습니다.
  6. **숫자 표시**: 숫자 값은 오른쪽 정렬(`.num`)이고, 유효 숫자 10자리를 넘는 소수는 일반(General) 서식으로 10자리까지만 표시합니다(`1266.66666666667` → `1266.666667`). 원본 값은 `data-raw`와 모델에 그대로 있고 수식 입력줄과 편집 모드(F2)에 표시됩니다. 수식 입력줄은 활성 셀의 수식 원문을 보여 주며 결과 영역(`#sheet-eval-result`)은 말줄임표와 `title` 툴팁으로 잘리지 않게 합니다.
  7. **개요·홈**: 개요의 시트 항목은 실제 탭 이름을 씁니다. SHEET 모듈에서는 문서 전용 도구(조판 부호·문단·글머리·머리말·각주·문자표 등)를 숨기고(비활성이 아니라 `hidden`) 빈 그룹·구분선도 숨깁니다. 기존 명령만 재사용하며 IPC 명령은 15종 그대로입니다.
  8. **성능 보호**: 가상 스크롤은 창이 바뀔 때만 행을 추가·제거합니다(`scroll` 처리 p50 약 3~4ms). 항목이 4,000칸을 넘는 시트는 셀 포커스마다 사전 스냅샷을 깊은 복사하지 않고 셀이 0.25초 멈췄을 때(또는 첫 입력 직전) 한 번만 만들며, 스냅샷 복사는 JSON 왕복 대신 항목별 얕은 복사(`cloneSheetStore`)입니다. 수식이 없는 시트는 재계산 페이로드를 만들지 않고, 큰 시트의 재계산은 키 입력이 끝나고 한 틱 뒤에 시작합니다. 20,000칸을 넘는 통합 문서는 자동 저장(핑거프린트 계산과 `localStorage` 한도 초과로 입력이 멈춤)을 건너뛰고 상태 표시줄에 알리되 저장하지 않은 변경 경고는 유지하며 이전 자동 저장 기록은 지웁니다.
  - 셀 편집을 별도 `<input>` 오버레이로 바꾸기: IME·접근성은 단순해지지만 기존 `contenteditable` 셀 상태 기계(실행 취소·재계산·포커스 계약, 다수 회귀 테스트)를 전부 다시 써야 해 기각. 대신 셀을 계속 `contenteditable`로 두고 준비 모드에서 첫 입력 시점에 내용을 비웁니다.
  - 모든 행·열 DOM 생성: 10,000행에서 수만 개 노드로 입력·스크롤이 눈에 띄게 느려져 기각.
  - 열 너비·클립보드 앱 내부 형식을 파일에 저장: 가져오기·저장 라운드트립 계약(#104, #111) 변경이 필요해 이번 범위에서 제외.
- **Verified by** `node --test scripts/test-webui-sheet-editing.mjs`(덮어쓰기 입력·`=B3` 수식·Enter/Shift+Enter/Tab/Esc·방향키 모드·IME 조합·범위 선택 3종·Ctrl+A·선택 범위 집계·TSV 복사/붙여넣기/잘라내기와 상대 참조·범위 지우기·10,000행 가상화와 `Ctrl`+방향키 점프·숫자 서식·열 너비 조절·수식 입력줄·개요 이름·SHEET 홈), 기존 `test-webui-*.mjs` 전체, 실서버 `--serve` + 헤드리스 Chrome 실제 키보드·마우스·클립보드·IME(`Input.imeSetComposition`/`Input.insertText`) 조작과 스크린샷, 10,000행 × 8열 성능 측정(키 처리 p50 3~4ms·p95 약 16~18ms·최대 약 28ms, 스크롤 렌더 p50 약 4ms·p95 약 6~9ms, 휠 스크롤 프레임 p50 16.7ms·p95 약 22~25ms), `node scripts/check-repo-hygiene.mjs`, `cargo run -p easyoffice-app -- --self-test`
- **Revisit when** 병합 셀·열 숨김·행 높이 조절, 열 너비·서식을 파일에 보존, 채우기 핸들, 셀 이동(드래그 앤 드롭), 대용량(수십만 셀) 증분 재계산이 필요할 때.


## 54. ZIP 기반 출력 DEFLATE 압축·변환 응답 단일 base64·실제 경로 벤치마크 (PD-079, #124)
- **Context** 모든 ZIP 기반 출력(DOCX·XLSX·PPTX·PPSX·ODT·ODS·ODP·HWPX·HWTX)이 무압축 `ZipArchiveCodec::pack`만 사용해 약 1MB 텍스트(9,700문단)가 DOCX 1,480,838바이트, HWPX 2,330,134바이트가 되었고, 변환 응답은 같은 base64를 `converted_base64`와 `convertedBase64`에 두 번 담았으며, 벤치마크는 작은 합성 입력만 측정했습니다.
- **Decision (`PD-079`)**
  1. **항목별 압축 정책 (`zip_codec.rs`)**: 모든 ZIP 작성기는 `ZipArchiveCodec::pack_compressed`를 사용합니다. 저장소 내 DEFLATE(`deflate::deflate_raw`, 해시 체인 8단계 + 블록별 동적 허프만, 기본 레벨 2)를 쓰고, 압축 결과가 원본보다 작지 않거나 압축률이 90배를 넘는 항목(읽기 쪽 압축 폭탄 한도 100배 미만 유지)은 무압축으로 둡니다. `ZipArchiveCodec::must_store`가 `mimetype`(ODF/HWPX는 첫 엔트리·무압축·오프셋 30 고정)와 이미 압축된 형식(png·jpg·jpeg·gif·webp·zip·jar·gz·mp3·mp4)을 항상 무압축으로 유지합니다. `pack_deflate`도 같은 예외를 따릅니다. 당시 인코더 프로파일링에서 압축 비용이 변환 전체의 약 15ms/MB 수준이었던 고정 허프만 인코더는 동적 허프만 인코더로 교체되어(1MB DOCX 변환 약 -31%, 10만 셀 XLSX 약 -25%, XLSX 크기 -31%), CRC32·시트 DAG 최적화는 #135로 남깁니다.
  2. **응답 필드**: `convert_document_format` 응답은 `convertedBase64` 하나만 보냅니다(`converted_base64` 제거, 응답 크기 약 2.7배 → 1배). UI·오프라인 RPC·자가 진단은 `convertedBase64`만 읽습니다.
  3. **실제 경로 벤치마크 (`benchmark_real.rs`)**: `run_engine_benchmark` 응답과 `--self-test`에 `realPaths`(DOCX→DOCX·DOCX→HWPX 9,700문단 디코드+인코드, 1,000x104셀 XLSX 패킹·언패킹, 5,000 타깃 재계산)를 추가해 시간과 출력 크기를 보고합니다. 동등성(라운드트립 문단·셀 일치)과 출력 크기 상한(문서 500,000바이트, XLSX 3MB)은 항상 검증하고, 시간 상한(10초)은 release 빌드에서만, 느슨하게 검증합니다. debug 빌드는 `timingGated:false`로 수치만 출력합니다.
- **Measured (release)**
  - DOCX 1,480,838 → 184,948바이트, HWPX 2,330,134 → 234,482바이트, XLSX(104k셀) 3,641,911 → 746,161바이트.
  - 변환 시간(디코드+인코드) DOCX ≈24 → ≈40ms, HWPX ≈28 → ≈45ms, XLSX ≈139 → ≈190ms, 재계산 ≈5ms(변화 없음).
- **Alternatives**
  - 새 DEFLATE 인코더(동적 허프만·지연 매칭): 크기가 이미 기준(0.5MB)의 절반 이하이고 시간 병목이 아니어서 보류.
  - 응답에 두 필드 유지: 호환 소비자가 저장소 안에 없고 전송량이 2배라 기각.
  - 벤치마크 시간 비율 단정: 공유 러너에서 불안정해 기각.
- **Verified by** `cargo test -p easyoffice-formats --test zip_deflate_outputs`(크기 상한, `mimetype` 첫 엔트리·무압축, 9개 ZIP 형식 왕복·CRC, 항목별 정책), `cargo test -p easyoffice-app --lib convert_ipc`(응답 단일 필드), `benchmark_real` 테스트, `--self-test`, .NET `ZipFile`과 `Expand-Archive`로 생성물 열기.
- **Revisit when** 그림 등 미디어 파트 출력이 추가되거나, 동적 허프만이 필요한 더 큰 문서·시간 병목(#135)이 확인될 때.

## 55. 슬라이드 발표자 모드·슬라이드 관리·실시간 썸네일·덱 화면 비율 (PD-080, #125)

- **Status** Implemented
- **Context** 슬라이드 모듈에는 발표 기능(F5)이 없었고 삭제·복제·순서 변경 UI가 없었으며, 제목을 고쳐도 왼쪽 목록이 갱신되지 않고 14자로 잘렸습니다. 새 슬라이드는 이전 기본 문구로 채워졌고 스테이지는 창 너비를 따라 가로로 늘어났습니다(#125).
- **Decision (`PD-080`)**
  1. **새 모듈 `slide-studio.js` (`__EASYOFFICE_CREATE_SLIDE_STUDIO__`)**: 슬라이드 관리·썸네일·화면 비율·슬라이드 쇼를 한 모듈에 둡니다. 데이터는 계속 `slideDeckState` 하나이며 저장(`slides`: 순서·제목·글머리·노트, #104), 열기(#111), 자동 저장은 같은 배열을 읽습니다. `--serve` 경로, 자가 진단 자산 검사, 저장소 필수 파일, 테스트 하니스, `index.html`에 등록했습니다.
  2. **단축키 표(`ribbon-catalog.js` `WORKSPACE_SHORTCUT_BINDINGS`)**: 표 항목에 `module`(활성 모듈 한정)과 `scope`(포커스가 선택자 안에 있을 때만)를 추가했습니다. `F5` 처음부터, `Shift+F5` 현재 슬라이드부터, `Alt+↑/↓` 위·아래 이동(슬라이드 목록 포커스 시), `Delete` 삭제(슬라이드 목록 포커스 시)입니다. **F5는 브라우저 예약 키(새로 고침)이므로 슬라이드 모듈이 활성이고 포커스가 텍스트 입력(input·textarea·select·contenteditable) 밖일 때만 `preventDefault`합니다.** 다른 모듈과 텍스트 입력 중에는 브라우저 기본 동작(새로 고침, 이탈 경고)이 유지되며, `Ctrl+F5` 등 변형 키는 건드리지 않습니다. 복제는 `Ctrl+D`(브라우저 북마크 예약)를 피해 버튼·우클릭 메뉴로만 제공합니다. 도구 막대 툴팁과 단축키 표가 같은 항목을 가리킵니다.
  3. **슬라이드 쇼**: `#slide-show-overlay`(검은 배경, 덱 비율 고정 뷰포트)를 엽니다. 가능하면 `documentElement.requestFullscreen()`으로 전체 화면에 들어가고, API가 없거나 거부되면(권한·정책) 같은 오버레이가 창 안에서 그대로 동작합니다. 전체 화면에서 브라우저가 Esc를 가로채 나가면 `fullscreenchange`로 쇼도 끝냅니다(WebView2 셸에서도 동일 경로). 이동은 `→`·`↓`·`Space`·`PageDown`·`Enter`·클릭(다음), `←`·`↑`·`PageUp`·`Backspace`(이전), `Home`/`End`, `Esc` 종료이며 마지막 슬라이드에서 다음으로 가면 쇼가 끝납니다. `B`/`W`는 검은·흰 화면 토글이고 이동하면 복귀합니다. 슬라이드 번호 `n / N`을 표시하고 전환 효과(없음·페이드·밀어내기·확대)는 새 슬라이드의 기존 전환 선택값을 CSS 애니메이션으로 적용하며 `prefers-reduced-motion`에서는 끕니다. 내용은 `textContent`와 기존 안전 복원기로만 그립니다. 쇼 중에는 이 모듈이 모든 키를 소유해 `Ctrl+Z` 같은 작업 공간 단축키가 편집 내용을 바꾸지 못하고, 이동한 경우에만 종료 시 해당 슬라이드를 선택합니다. 발표자 보기(`P`): 경과 시간, 다음 슬라이드 제목, 발표자 노트를 같은 창 안 패널로 보여 줍니다. 별도 창 두 개 방식(프로젝터용 분리 창)은 구현하지 않았습니다.
  4. **슬라이드 관리**: 추가(빈 제목·본문·노트, 현재 덱 비율·전환 상속), 복제(바로 뒤에 삽입), 삭제(마지막 한 장은 보호), 위·아래 이동, 썸네일 끌어 놓기, 우클릭 메뉴(`role="menu"`, Esc 닫기)를 제공합니다. 모든 변경은 작업 공간 실행 취소 스냅샷(`pushUndoSnapshot`)과 자동 저장 활동 기록을 거치므로 `Ctrl+Z`/`Ctrl+Y`로 되돌리고 되살릴 수 있으며 저장·열기·복원과 동일한 모델을 씁니다. 키보드로 옮기면 포커스가 이동한 썸네일을 따라가고, 썸네일을 클릭해도 포커스를 유지합니다.
  5. **썸네일**: 제목·본문 입력은 150 ms 디바운스로 목록을 제자리 갱신합니다(포커스 유지). 제목은 JS에서 자르지 않고 CSS `text-overflow: ellipsis`로 줄이며 전체 제목은 `title` 툴팁과 `aria-label`에 둡니다. 활성 슬라이드는 `active`·`aria-current`로 강조하고 빈 제목은 "(제목 없음)"으로 표시합니다.
  6. **화면 비율**: 도구 막대 선택값 16:9·4:3·A4(가로 297:210)를 덱 단위로 `aspect` 필드에 저장하고(모든 슬라이드 동일, 실행 취소 대상, 필드 없는 이전 자동 저장은 16:9) 스테이지 `data-aspect`로 반영합니다. 스테이지는 `aspect-ratio`와 `min(100%, (100vh − 크롬) × 비율)` 너비로 사용 가능 영역 안에서 레터박스되고(1024×680~1920×1080에서 비율 유지), 글자는 컨테이너 쿼리 단위(`cqw`)로 스테이지와 함께 커집니다. 슬라이드 쇼 뷰포트도 같은 비율을 씁니다. 비율은 화면용 설정이며 PPTX 쓰기 경로의 슬라이드 크기는 바꾸지 않습니다.
  7. **새 슬라이드 기본값**: 제목·본문·노트가 모두 빈 값이고, 편집 영역은 CSS `:empty::before`로 "제목을 입력하세요"/"내용을 입력하세요" 안내만 보여 줍니다(저장 모델에 들어가지 않음).
- **Alternatives rejected**
  - 쇼를 별도 브라우저 창으로 열기: 팝업 차단·보안 컨텍스트·모델 동기화가 복잡하고 셸 창 정책(#121)과 어긋나 기각.
  - F5를 모든 모듈·모든 포커스에서 가로채기: 다른 모듈과 입력 중 새로 고침을 막아 사용자가 갇히므로 기각.
  - 썸네일을 JS로 14자 절단: 폭과 글꼴에 따라 잘림 위치가 달라지고 전체 제목을 잃어 기각.
  - 스테이지 너비를 `vw` 고정 px로 계산: 레이아웃 변경마다 어긋나 `aspect-ratio`와 컨테이너 단위로 대체.
- **Verified by** `node --test scripts/test-webui-slide-show.mjs`(단축키 표 등록·모듈/텍스트 안전, F5·Shift+F5 시작 위치, 이동·화면 가리기·종료 키, 전환·전체 화면 성공/거부/미지원/해제, 발표자 보기, 빈 새 슬라이드, 복제·삭제·실행 취소·재실행·이동·키보드·끌어 놓기·우클릭 메뉴, 디바운스 썸네일·말줄임·툴팁, 비율 16:9/4:3/A4와 실행 취소, 자동 저장·저장 페이로드 순서), `node --test scripts/test-webui-session-context.mjs`(F5 예외 고정), 실서버 `--serve 8837` + 헤드리스 Chrome 1024×680·1440×900·1920×1080 실제 키·마우스 입력과 스크린샷(스테이지 비율 1.778/1.333/1.414, F5 전체 화면 진입, 슬라이드 쇼 레터박스), `node scripts/check-repo-hygiene.mjs`, `cargo run -p easyoffice-app -- --self-test`
- **Revisit when** 프로젝터용 별도 발표자 창, 슬라이드 크기 PPTX 반영, 레이아웃 선택·개체 틀 편집, 레이저 포인터의 쇼 화면 연동이 추가될 때.

## 56. rhwp WASM 벤더링·hwp-studio 오버레이 빌드·OFL 글꼴 번들 (PD-081, #129)
- **Context** rhwp 통합 2단계입니다(설계 `docs/rhwp-integration-design.md` §3.3~3.5, §4, §9, 작업 2). 1단계(PD-076)가 upstream을 고정했고, 이 단계는 편집기 엔진(WASM)과 임베드 전용 편집기 번들, 번들 가능한 글꼴을 만듭니다. 앱 런타임은 바뀌지 않습니다(UI·IPC 15종·Tauri 셸 불변, 임베딩은 3단계).
- **Decision (`PD-081`)**
  1. **WASM은 미리 빌드해 커밋하고 CI는 빌드하지 않습니다.** `vendor/rhwp-core/`에 `rhwp_bg.wasm`·`rhwp.js`·`rhwp.d.ts`·`rhwp_bg.wasm.d.ts`·`package.json`·`LICENSE`와 `PROVENANCE.json`(rhwp 커밋·버전, 툴체인 1.93.1, wasm-pack 0.14.0, 빌드 명령·호스트, 파일별 크기·SHA-256)를 둡니다. `scripts/verify-rhwp-core.mjs`(Node 내장 모듈만)가 provenance와 `config/rhwp-upstream.json`(`wasmPackVersion` 추가) 일치, 파일 존재·크기·해시, 미등재 파일 부재를 fail-closed로 검사하고 `verify-rhwp-upstream.mjs`·위생 가드에서 호출됩니다. 갱신은 `scripts/vendor-rhwp-core.mjs`가 wasm-pack 출력 폴더에서 복사하고 provenance를 다시 쓰는 방식뿐입니다. `.gitattributes`가 벤더 파일과 글꼴의 줄바꿈 변환을 막아 Windows 체크아웃에서도 해시가 같습니다.
  2. **빌드 명령(로컬 1회)**: `rustup toolchain install 1.93.1 --target wasm32-unknown-unknown --profile minimal`, wasm-pack 0.14.0은 공식 릴리스 바이너리를 받아 SHA-256으로 확인(`d484c8e8…c721f1`), `third_party/rhwp`에서 `wasm-pack build --target web --out-dir <절대 경로> --release`(기본 기능, `CARGO_TARGET_DIR`는 서브모듈 밖). rhwp 워크스페이스가 `tools/*`·`bindings/Native` 멤버를 선언하므로 sparse 목록에 그 매니페스트와 `saved/blank2010.hwp`(`include_bytes!`)를 추가했습니다. `rhwp.js`·`rhwp.d.ts`는 참조 앱이 같은 커밋에서 만든 파일과 바이트 단위로 같고, `.wasm`은 빌드 호스트(Windows·MSVC 호스트 툴체인)에 따라 18,242바이트 더 큽니다(11,821,975 대 11,803,733). 완전 재현은 보장하지 않으므로 벤더 갱신 PR의 원천은 이 provenance가 기록한 로컬 빌드입니다(설계 Q10).
  3. **편집기 오버레이 `apps/hwp-studio`** (참조 앱 `studio-host` 패턴): npm + 커밋된 `package-lock.json` + `npm ci --ignore-scripts`, Vite 설정이 업스트림을 `@`/`@upstream`으로 별칭하고 `@wasm/rhwp.js`를 벤더 WASM으로 지정합니다. `config/rhwp-studio-overrides.json`이 모든 오버라이드(`strategy`·`reason` 필수)와 upstream 원본 SHA-256(줄바꿈 정규화)을 기록하고, `scripts/hwp-studio-overrides.mjs`가 오버레이 파일과 목록의 1:1 대응·원본 해시를 검사합니다(upstream이 바뀌면 검토 전까지 실패). 대체는 별칭이 아니라 resolve 단계에서 해석된 파일 경로로 매칭하므로 업스트림 안의 상대 import(`./font-loader.ts`)도 대체됩니다.
  4. **오버라이드 내용**: (a) `main.ts` 포크 — `chrome=embed` 고정(열기·저장·새 문서·최근·비교는 호스트 소유), `?url=` 임의 URL 로드·자동저장 복구·PWA·플러그인 로더·개발 훅·CanvasKit 제거, Canvas2D 고정, (b) `core/font-loader` 포크 — `FontFace` API만 사용하고 `<style>`을 만들지 않으며 번들된 글꼴 파일만 요청(CDN·미동봉 항목은 요청하지 않음), (c) `command/print-pages` 포크 — 정적 규칙은 `print.css`, 쪽별 `@page` 규칙은 구성형 스타일시트(CSSOM)로 외부화하고 `print.html`의 `<style>`도 파일로 분리, (d) `embed/protocol`·`embed/rpc-router` 포크 — `plugin-loader-v1`·`hwpctrl-v1` 미광고와 `plugin.*` 메서드 제거, `blank-document-v1`과 `createBlankDocument` RPC 추가(새 문서를 호스트가 요청할 방법이 프로토콜에 없기 때문), (e) `index.html` 포크 — 인라인 `style` 속성 제거, 초기화 스크립트 상대 경로, 아이콘 자산 제거. 글꼴 카탈로그의 CDN URL 문자열은 빌드 플러그인이 비활성 스킴(`blocked-external-font:`)으로 바꿔 번들에 외부 URL이 남지 않습니다. `canvaskit-wasm`·`vite-plugin-pwa`·`@tauri-apps/*`는 의존성에 없습니다.
  5. **번들은 빌드하고 커밋하지 않습니다.** 산출물 `crates/easyoffice-app/ui/hwp-studio/`는 `.gitignore` 대상이며 추적되면 위생 가드가 실패합니다. 근거: 압축 전 약 2 MB의 해시 이름 JS가 빌드마다 바뀌어 diff·저장소 크기만 키우고, 입력(커밋된 WASM, 고정 upstream 커밋, 잠금 파일)이 모두 고정이라 `scripts/build-hwp-studio.mjs`(약 1초 vite + 약 50초 `npm ci`)가 같은 결과를 내며, Windows CI가 `ui/hwp-studio/index.html` 산출·크기 기록·정적 검사·하니스를 매번 증명합니다. WASM만 커밋하는 이유는 툴체인(Rust 1.93.1·wasm-pack)이 엔진 MSRV와 달라 CI에 넣으면 빌드가 크게 늘기 때문입니다.
  6. **번들 정적 검사 (`scripts/check-hwp-studio-bundle.mjs`)**: 외부 URL(허용은 `http://www.w3.org/` 네임스페이스와 입력 칸 예시 `https://example.com`뿐), `eval(`·`new Function`·`__TAURI`·서비스 워커·확장 API 참조, 인라인 스크립트·스타일·핸들러, 소스맵·워크박스·`canvaskit*.wasm`, `.wasm`이 벤더 파일과 다름, `asset-manifest.json`(빌드가 쓰는 파일별 크기·SHA-256)과 불일치·미등재 파일, 글꼴 제외 20 MiB 초과를 실패로 처리합니다. 실측: 글꼴 제외 13.46 MiB(WASM 11.27, JS 1.98, CSS 0.11), 글꼴 18.42 MiB.
  7. **글꼴 (`assets/fonts`)**: SIL OFL 1.1 글꼴 28개(Noto Sans/Serif KR, 나눔고딕·명조·고딕코딩, 고운바탕·돋움, Pretendard 9종, D2Coding, 스포카 한 산스)를 rhwp가 게시한 woff2 그대로 복사했고 파일별 OFL 전문(`licenses/`)을 함께 둡니다. `fonts.json`(파일·가족·라이선스·크기·SHA-256)과 `FONTS.md`를 `scripts/verify-fonts.mjs`가 검사합니다(OFL-1.1이 아니거나 전문이 OFL 1.1이 아니거나 미등재·변조·문서 누락이면 실패). 제외: 카페24·행복고딕(독자 무료 약관), Latin Modern Math(GUST, OFL 아님), Source Han Serif K 옛한글 부분집합(이름 `Source`를 예약한 글꼴의 수정 부분집합), 한컴·Microsoft·OS 글꼴. Pretendard woff2는 공식 배포 파일과 바이트 단위로 같음을 확인했습니다.
  8. **위생 가드 (`check-repo-hygiene.mjs`)**: 후보 파일 목록을 디스크 순회 대신 `git ls-files --cached --others --exclude-standard`로 만들어 무시된 빌드 산출물(`target*/`, Tauri `gen/`·`permissions/`, `node_modules`, 생성 번들)을 건드리지 않으면서 아직 추가하지 않은 새 파일은 검사합니다(Git이 없으면 기존 순회로 대체). 내용·JS 구문 검사 제외 경로는 루트 기준 `third_party/rhwp`, `vendor/rhwp-core`, `crates/easyoffice-app/ui/hwp-studio`이며 각각 커밋 고정·크기/해시 고정·정적 검사로 대신 보증합니다. `.ts`를 검사 확장자에 추가해 오버레이 소스도 금지 토큰 검사를 받고, 글꼴·오버라이드 목록 검증과 번들 미추적 검사를 추가했으며 기존 계약은 그대로입니다. `.gitignore`는 `/target*/`, `crates/easyoffice-desktop/permissions/`, `apps/hwp-studio/node_modules/`, 생성 번들을 무시합니다. 또한 비교 함수 없는 `.sort()` 호출을 EasyOffice 소유 JS/MJS/TS(`crates/easyoffice-app/ui`, `scripts`, `apps/hwp-studio/src`)에서 실패로 처리합니다(`scripts/sort-comparator-rule.mjs`: 주석·문자열·템플릿 문자열 부분·정규식 리터럴을 지운 뒤 `.sort(` 바로 뒤가 `)`인 경우를 찾으며, `third_party`·`vendor`·생성 번들·`node_modules`는 제외, 테스트 `scripts/test-sort-comparator-rule.mjs`).
  9. **고지**: `THIRD_PARTY_NOTICES.md`에 wasm32 정상 의존 크레이트 149개의 이름·버전·라이선스 표와 고유 라이선스 텍스트 전문(`scripts/generate-wasm-notices.mjs`가 `cargo metadata`와 각 크레이트 소스의 라이선스 파일에서 오프라인 생성; Apache-2.0 `NOTICE` 파일이 있으면 그대로 포함하나 이 의존성 집합에는 없음), 번들된 npm 패키지(`@noble/hashes`), 글꼴 가족별 저작권 표기를 채웠습니다. 참조 앱 코드는 복사하지 않았습니다.
  10. **CI**: 두 작업 모두 벤더 WASM·글꼴 검증과 `node --test scripts/test-hwp-studio.mjs`를 실행합니다. Windows 작업은 마지막에 `checkout-rhwp.mjs` → `build-hwp-studio.mjs`(정적 검사 포함) → `hwp-studio-harness.mjs` → 번들 크기 기록을 수행하며, Tauri 빌드 뒤에 두어 셸이 번들을 아직 내장하지 않습니다(PD-083이 순서를 바꿔 번들을 Tauri 빌드 앞으로 옮겼습니다).
- **Standalone harness (`scripts/hwp-studio-harness.mjs`)**: 번들을 같은 출처 iframe으로 서빙(설계 §4.2의 CSP `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; img-src 'self' data: blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'self'`)하고 부모 페이지가 임베드 프로토콜(MessagePort)로 연결해, 헤드리스 Chrome을 DevTools 프로토콜의 실제 입력 이벤트로 조작합니다: 빈 문서 → 한글 입력 → 앞 3글자 선택 → 도구 모음 글자 크기 20pt → `exportHwp` → 내보낸 바이트를 벤더 WASM(Node)과 편집기에서 다시 열어 같은 글자 모양(선택 3글자 20pt, 나머지 10pt)·같은 글 확인. CSP 위반·예외·외부 요청이 하나라도 있으면 실패합니다.
- **Why** 편집기 정확도(글자 모양·조판)는 rhwp 엔진에 있고, 엔진과 UI를 출처가 증명된 고정 산출물로 두어야 라이선스·보안 경계(외부 URL·eval 0건, IPC 권한 없음)를 정적으로 증명할 수 있기 때문입니다.
- **Trade-offs**
  1. `.wasm`이 빌드 호스트에 따라 달라 CI 재현 검증이 없고(수동 재빌드 워크플로는 후속), 신뢰는 provenance와 리뷰어의 로컬 재빌드에 있습니다.
  2. 오버레이는 `main.ts` 등 5개 파일을 포크하므로 rhwp 갱신마다 원본 해시 변경을 검토해야 합니다(검사가 강제).
  3. 번들은 CI에서 매번 `npm ci`로 만들기 때문에 npm 공급망(잠금 파일·`--ignore-scripts`)에 의존합니다.
  4. 글꼴 woff2는 upstream이 만든 웹 형식 복사본이라 예약 글꼴 이름(Nanum·D2Coding·Spoqa·Pretendard)을 가진 가족의 형식 변환이 OFL 3항의 "수정"에 해당하는지는 공개 배포 전에 확인이 필요합니다(Pretendard는 공식 파일과 동일).
  5. 하이퍼링크 대화상자의 미리보기는 사용자가 입력한 주소를 새 창으로 엽니다(자동 요청 아님). 3단계에서 부모 정책에 맞출지 결정합니다.
- **Alternatives rejected**
  - CI에서 WASM 빌드: Windows 작업 시간이 크게 늘고 툴체인이 섞여 기각(사용자 결정: 벤더링).
  - 번들 커밋: 생성물 diff 노이즈와 저장소 증가, 입력이 고정이라 이점이 없어 기각.
  - 별칭만으로 대체: 업스트림 내부 상대 import가 포크를 우회해 기각.
  - 비OFL 무료 글꼴·GUST 글꼴 동봉: 약관 확인 비용과 위험이 커 기각.
  - 업스트림 `index.html`을 빌드 때 변환: 인라인 스타일 제거를 검토 가능한 파일로 남기려고 포크(원본 해시로 드리프트 감지).
- **Verified by** `node --test scripts/test-rhwp-upstream.mjs scripts/test-hwp-studio.mjs`(벤더 변조·누락·미등재·provenance 불일치, 번들 정적 검사 각 항목과 오탐 방지, 해시·예산, 오버라이드 목록 구조·원본 해시 드리프트, 글꼴 OFL·해시·문서, 위생 가드의 무시 경로·번들 추적·`.ts` 검사), `node scripts/build-hwp-studio.mjs`, `node scripts/hwp-studio-harness.mjs`, `node scripts/check-repo-hygiene.mjs`
- **Revisit when** rhwp를 갱신할 때(WASM 재빌드·원본 해시 검토), 셸에 번들을 내장하는 3단계(Tauri CSP에 `'wasm-unsafe-eval'` 필요), 저장 가능한 HWPX(4단계)나 macOS 이전(9단계)을 다룰 때.

## 57. 엔진 저수준 성능: CRC-32 하드웨어 가속·시트 슬롯 메모/DAG 계획 캐시·대형 워크로드 벤치와 release 게이트 (PD-082, #135)
- **Context** release 기준 최적화 경로가 기준선 대비 1.6~4.1배였으나 debug에서 CRC-32 0.85배·시트 DAG 0.92배였고, 시트 "최적화" 경로는 위상 사전 평가 뒤 재평가·스택마다 시트 이름 `String` 복제·셀당 SipHash 조회 여러 번·메모 적중마다 값 복제로 오히려 일을 더 했습니다(설계 `docs/rhwp-integration-design.md` §6.2, 작업 8).
- **Decision (`PD-082`)**
  1. **CRC-32 (`crc32.rs`)**: 조회 표를 연관 `const`가 아닌 `static CRC32_TABLES: [[u32; 256]; 16]`로 두고 `chunks_exact(16)` slicing-by-16 스칼라 코어를 쓴다. 입력이 64바이트 이상이면 실행 시간 검출로 x86_64 PCLMULQDQ 폴딩(`is_x86_feature_detected!("pclmulqdq")` + `sse4.1`, `#[target_feature]` 함수 하나)이나 aarch64 `crc` 명령(`is_aarch64_feature_detected!("crc")`)을 쓰고, 그 외에는 항상 스칼라로 폴백한다. `unsafe`는 두 개의 `#[target_feature]` 함수와 그 호출(SAFETY 주석: 검출 직후 호출, 16바이트 경계 검사)로 한정한다. 결과는 기존과 바이트 단위로 같다(`update(crc, bytes)`로 증분 계산 가능). aarch64 경로는 `cargo check --target aarch64-pc-windows-msvc`로 컴파일만 검증했고 이 저장소 CI에서 실행되지는 않는다.
  2. **시트 엔진 (`memo.rs`, `dag.rs`, `agg_stream.rs`)**: ①메모를 `HashMap` 대신 32x16 페이지 단위 슬롯 인덱스(`u32` 아레나 인덱스, 기본 시트 앞 16열)로 두고 그 밖의 셀만 시드된 곱셈 해셔(`CoordHasher`, 프로세스별 난수 시드로 충돌 유도 방지) 맵을 쓴다. ②순환 탐지 방문 집합과 사전 평가의 DFS 상태를 비트셋(방문 1비트, DFS 2비트/셀)으로 바꾼다. ③사전 평가 스택은 시트 이름 `String` 대신 `u16` 슬롯 번호를 담고 이름→시트 해석은 AST 토큰 포인터로 캐시한다(시트 ID는 이미 `u16`). ④큰 범위(256셀 초과)의 의존성은 열별로 정렬된 수식 셀 행 목록(`FormulaIndex`)을 이진 탐색해 빈 셀·리터럴을 건너뛴다. ⑤`recalculate_all`은 첫 실행에서 사전 평가를 하며 후위 순서와 순환 소유자를 `RecalcPlan`으로 기록하고, 이후에는 편집(`set_cell`·`set_coord_number`·`rebuild_compiled_asts`)·이름 변경 전까지 기록된 순서를 재생해 그래프를 다시 걷지 않는다. ⑥메모 적중과 집계 범위 순회는 값을 복제하지 않고 빌려 쓴다(`with_coord_value`). ⑦`IF` 핫 경로·2항 연산 무할당 경로·`dense_numbers` 16,384행 확장·`is_same_sheet_identity`/시트 조회 무할당화를 추가했다. 의미(오류값, PD-070 `#NUM!` 순환 플래그, INDIRECT, 시트 간 참조, 평가 단계·재귀 예산, 4레인 합산 순서)는 바꾸지 않는다.
  3. **순서 결정성**: 큰 범위에서 종전에는 `HashMap` 순회 순서(프로세스마다 무작위)로 의존 셀을 스택에 넣었다. 이제 항상 행 우선 오름차순이므로 순환 참조가 겹친 시트의 결과가 실행마다 달라지지 않는다. 비순환 시트의 값은 순서와 무관하다.
  4. **벤치 하니스**: `benchmark_large.rs`가 `crc32_hardware_1mib`(1 MiB, 검출 경로명을 digest에 기록)와 `sheet_recalc_10k_rows`(1만 행x5열, 5만 셀)를 A/B 교대 순서(라운드마다 뒤집음)로 추가하고, 실제 경로에 `sheet_recalc_10k_rows`·`crc32_64kib_x64`·`crc32_1mib_x8`을 더한다. 동등성은 항상 검증하고, 속도 하한(`crc32_slicing_by_8` 2.0배, `crc32_hardware_1mib` 2.0배, `sheet_dag_bitset_eval` 1.1배, `sheet_recalc_10k_rows` 2.0배)은 release 빌드의 `--self-test`에서만 느슨하게 판정한다. debug는 수치만 출력한다(`timingGated:false`).
  5. **DEFLATE 인코더는 바꾸지 않는다**: 1.27MB 문서 XML 기준 압축 11.2ms(114MB/s), 압축률 11.9%, 해제 1.6ms로 DOCX 디코드+인코드 29ms 중 약 38%이고 이미 크기 상한의 1/3 수준이다. 해시 체인·지연 매칭은 크기를 소폭 줄이는 대신 느려지고, 출력이 바뀌면 `deflate_zlib_roundtrip` 기준선과의 바이트 동등성 검증이 깨지므로 보류한다.
- **Measured (release, `--self-test` 3회 중앙값, x86_64 i7-1165G7, 이전은 `8e550ae` 기준 동일 하니스)**

| 항목 | 이전 기준선→최적화 (배) | 이후 기준선→최적화 (배) |
|---|---|---|
| `crc32_slicing_by_8` (64KiB x16, 스칼라) | 1,879→452µs (4.16x) | 1,866→302µs (6.13x) |
| `crc32_hardware_1mib` (PCLMULQDQ) | 1,975→501µs (4.01x) | 2,112→34µs (61.9x) |
| `deflate_zlib_roundtrip` | 277→76µs (3.70x) | 279→74µs (3.74x) |
| `utf16_swar_and_xml_unescape` | 164→78µs (2.09x) | 146→82µs (1.78x, 코드 변경 없음, 잡음) |
| `sheet_dag_bitset_eval` (200셀) | 1,865→1,133µs (1.68x) | 2,051→242µs (8.52x) |
| `swar_csv_xml_and_zero_alloc_range_agg` | 1,124→430µs (2.61x) | 1,095→336µs (3.26x) |
| `sheet_recalc_10k_rows` (5만 셀) | 92,834→48,920µs (1.81x) | 87,609→10,845µs (7.94x) |

| 실제 경로 | 이전 | 이후 |
|---|---|---|
| `docx_to_docx_1mb` | 37.8ms / 184,948B | 37.2ms / 184,948B |
| `docx_to_hwpx_1mb` | 46.8ms / 234,482B | 42.1ms / 234,482B |
| `xlsx_100k_cells_pack_unpack` | 164.9ms / 746,161B | 166.3ms / 746,161B |
| `sheet_recalc_5000_targets` | 4.5ms | 2.4ms |
| `sheet_recalc_10k_rows` | 49.6ms | 16.4ms |
| `crc32_64kib_x64` | 2.00ms | 0.14ms |
| `crc32_1mib_x8` | 3.82ms | 0.28ms |

  debug 빌드에서는 CRC-32 스칼라 0.94배(검사 비용 때문에 의미 있는 지표 아님), 시트 200셀 7.6배, 1만 행 7.4배이며 비율은 판정하지 않는다.
- **Alternatives**
  - 평가 결과를 `Rc<str>` 텍스트로 바꿔 복제를 O(1)로: `EvaluatedCellValue::Text(String)`이 공개 API 전반(테스트·IPC)에 쓰여 파급이 커서 기각하고, 집계 경로는 참조로 빌려 쓰는 방식으로 대체했다.
  - 결과 맵(`recalculate_all` 반환형)을 고속 해시맵으로 교체: 공개 반환형이 바뀌어 기각(5만 셀 삽입 약 2ms).
  - SSE4.2 `crc32` 명령: Castagnoli 다항식이라 ZIP CRC에 쓸 수 없어 기각.
- **Verified by** `cargo test -p easyoffice-formats --test crc32_equivalence`(무작위 길이 0~4,200·정렬 오프셋 64종·0~700 전 길이·증분 분할·1MiB를 비트 단위 기준 구현과 대조, 하드웨어 경로 포함), `cargo test -p easyoffice-sheet --test sheet_slot_memo_and_plan_cache`(구 엔진에서도 같은 단언이 통과하는 특성화 테스트: 반복 재계산·편집 무효화·순환·깊은 사슬·1만 행·열 인덱스·오버플로 경로·시트 간 참조), 임시 400시드 무작위 시트(재계산·셀 평가·2시트 워크북, 순환 포함)를 구 엔진과 대조해 출력 일치(구 엔진의 큰 범위 순회를 정렬 순서로 고정해 비교, 재생 결과도 첫 실행과 일치), `cargo run --release -p easyoffice-app -- --self-test`(성능 게이트), `benchmark_large` 테스트.
- **Revisit when** 하드웨어 경로가 없는 CPU에서 스칼라 6배 미만으로 떨어지거나, 시트 공개 API를 바꿀 수 있어 결과 맵·텍스트 값 표현을 교체할 때, 문서 압축이 변환 시간의 지배 요인이 될 때.

## 58. 한글 패널 iframe 임베딩·CSP·폴백 (PD-083, #130)
- **Context** rhwp 통합 3단계입니다(설계 `docs/rhwp-integration-design.md` §2, §4). 2단계(PD-081)가 만든 `ui/hwp-studio` 번들을 한글(HWP) 모듈에 넣어 데스크톱 셸과 `--serve`에서 같은 출처 iframe으로 띄웁니다. 파일 열기·HWPX 저장은 4단계(#131)입니다.
- **Decision (`PD-083`)**
  1. **호스트 `ui/hwp-host.js`**: 한글 모듈을 처음 활성화할 때만 `#pane-hwp`에 iframe(`./hwp-studio/index.html`)을 만들고(지연 생성), `rhwp-connect` 핸드셰이크(`MessageChannel`, 대상 출처는 `"*"`(불투명 출처 프레임에는 이것만 가능, 핸드셰이크 본문은 버전·세션 id·capability 이름뿐), 포트는 transfer) 후 MessagePort로 `ready` → `createBlankDocument({skipUnsavedGuard:true})`를 호출합니다. 요청마다 증가하는 id와 타임아웃(일반 15초, 준비·불러오기·내보내기 60초, 연결 60초)이 있고 오류는 `textContent`로만 그립니다(`innerHTML` 없음). 연결 실패는 "다시 연결"(새 iframe)과 "텍스트 전용 편집기 사용" 버튼이 있는 오류 상태입니다. iframe에는 Tauri IPC·엔진 브리지를 주지 않으며 번들에 `__TAURI` 참조가 없음을 정적 검사가 보장합니다.
  2. **폴백**: 번들 요청(`GET ./hwp-studio/index.html`)이 실패하면(자산 없는 빌드) 기존 contenteditable 한글 편집기가 텍스트 전용으로 남고 "한글 편집기 구성 요소가 포함되지 않은 빌드입니다. 텍스트 전용 편집기로 대체합니다."를 상태줄과 편집 영역 위에 표시합니다. 번들은 있으나 연결이 실패한 경우는 폴백이 아니라 오류 상태이며 사용자가 폴백을 고를 수 있습니다.
  3. **활성 중 UI**: 임베드가 활성이면 문서 스타일 바(`#doc-style-bar`)와 문서 전용 도구 상자 단추·상태줄 문서 그룹을 숨기거나 비활성화하고(저장·팔레트·테마·PII 점검만 유지), `hwp-*` 리본 명령은 편집기 도구줄 사용 안내만 표시합니다(숨은 폴백 편집기를 몰래 바꾸지 않기 위해).
  4. **저장**: 임베드 활성 시 저장은 `exportHwp`(HWP 5.0 바이트)를 기존 전달 경로(데스크톱 `save_document_file` 대화상자, `--serve` Blob 다운로드)로 `.hwp`로 저장하고 성공하면 `notifySaved`로 편집기의 변경 표시를 지웁니다. 다른 모듈의 저장은 그대로입니다. HWPX 저장·열기는 4단계입니다.
  5. **iframe 격리 (개정)**: `sandbox="allow-scripts allow-modals"`, 곧 `allow-same-origin`이 없는 **불투명 출처**입니다. 처음에는 `allow-same-origin allow-scripts allow-modals`였고 "격리의 실체는 IPC 권한 없음과 CSP"라고 적었지만 실측하니 사실이 아니었습니다. Tauri v2의 IPC 권한은 창·웹뷰 라벨과 출처(local/remote)로만 정해지고 프레임 단위 구분이 없어서(`tauri` 2.12.5 `src/ipc/authority.rs` `resolve_access`, `src/webview/mod.rs` `on_message`), 셸과 같은 출처의 iframe은 창이 부를 수 있는 앱 명령 14종을 그대로 물려받습니다. 경로는 둘입니다. (a) 어느 OS에서나 `window.parent.__TAURI_INTERNALS__.invoke(...)`: 초기화 스크립트는 메인 프레임에만 주입되지만(`src/manager/webview.rs` `main_frame_script`) 같은 출처면 부모의 전역과 DOM에 그대로 닿습니다. (b) Windows에서는 WebView2가 초기화 스크립트를 하위 프레임에도 넣어(`src/webview/mod.rs` 문서: "Windows: scripts are always added to subframes") 프레임 자신이 invoke 키가 든 `__TAURI_INTERNALS__` 사본을 갖습니다. 어느 쪽이든 요청 출처가 앱 출처이므로 `capabilities/main-window.json`의 `local: true`에 맞아 허용됩니다(Tauri 보안 공지 GHSA-57fm-592m-34r7도 "Windows에서는 iframe과 창의 출처가 같으면 invoke가 동작"한다고 적고 있습니다). 헤드리스 Chrome 하니스에서 옛 설정은 `window.parent.document`와 `parent.__TAURI_INTERNALS__`에 닿았고 새 설정은 둘 다 `SecurityError`입니다. 새 격리는 (1) 부모 DOM·전역 접근이 막히고(전 OS, 하니스로 확인), (2) 프레임이 fetch로 보내는 IPC 요청의 `Origin`이 `null`이라 Tauri가 거부하며(`src/ipc/protocol.rs`의 `Url::parse(Origin)` 실패 처리), (3) 편집기 문서에는 `ipc:`/`ipc.localhost`를 뺀 CSP가 붙어 연결 자체가 막힙니다(아래 6항). 남는 경로는 Windows의 `window.chrome.webview.postMessage` 폴백이고, 이것은 문서만으로 닫혔다고 말할 수 없어 11항·12항으로 따로 다룹니다. macOS·Linux는 프레임에 Tauri 객체가 주입되지 않습니다(`for_main_frame_only`가 지켜짐). 팝업·폼·다운로드·상위 이동은 그대로 허용하지 않아 하이퍼링크 대화상자의 "미리보기"와 링크 클릭 열기(`window.open`)는 편집기 자체의 "팝업이 차단되었습니다" 안내로 끝납니다(셸의 `on_navigation`도 앱 출처 외 이동을 거부). 불투명 출처에서도 편집기는 부팅·입력·글꼴·내보내기가 그대로 동작합니다(하니스의 실제 입력 왕복으로 확인). 대신 `localStorage`·IndexedDB는 쓸 수 없고(접근하면 `SecurityError`), 편집기는 이를 예외 없이 처리합니다(자동 저장·최근 문서는 부모가 소유, PD-085). 또 번들 파일을 같은 출처로 읽을 수 없으므로 `/hwp-studio/*` 응답에만 `Access-Control-Allow-Origin: *`을 붙입니다(데스크톱은 앱 프로토콜 응답 훅 `studio_frame.rs`, `--serve`는 `hwp_bundle::STUDIO_CORS_HEADER`; 번들은 공개 정적 파일이고 다른 응답에는 붙이지 않습니다). 이 부여가 없으면 모듈 스크립트가 CORS로 막혀 편집기가 연결되지 않는 것을 확인했습니다.
  6. **Tauri CSP** (`tauri.conf.json`, 위생 가드가 정확값 검사): `default-src 'self'`, `script-src 'self' 'wasm-unsafe-eval'`, `img-src 'self' data: blob:`, `frame-src 'self'`, `connect-src 'self' ipc: http://ipc.localhost`. `unsafe-eval`·`unsafe-inline`은 없고 capabilities(`allow-ipc-dispatch`, `allow-save-document-file`)는 불변입니다. 셸 CSP는 메인 문서와 iframe 문서에 같이 적용되는 합집합이지만, 이 개정부터 데스크톱은 `/hwp-studio/` 아래 HTML 응답의 CSP를 `--serve`와 같은 `hwp_bundle::STUDIO_CSP`로 바꿔 편집기 문서에서는 `connect-src`가 `'self'`뿐입니다(`ipc:`·`http://ipc.localhost` 없음). 번들은 `frontendDist`(`ui/`) 안에 있어 실행 파일에 내장되고, 번들이 없으면 데스크톱 `build.rs`가 정확한 해결 명령과 함께 빌드를 실패시킵니다(Tauri 빌드·clippy·test 전에 `node scripts/build-hwp-studio.mjs`가 먼저 실행되도록 CI·릴리스 워크플로 순서를 바꿨습니다).
  7. **`--serve` 경로별 CSP**: 메인 페이지·API·오류는 `default-src 'self'` 그대로입니다(같은 출처 iframe은 `frame-src`가 `default-src`로 폴백되어 허용되므로 완화 불필요). `/hwp-studio/*`는 `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; img-src 'self' data: blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'self'`이고 모든 응답에 `X-Content-Type-Options: nosniff`가 붙습니다. 정적 파일은 바이너리 본문과 정확한 MIME(`application/wasm`, `font/woff2`, `text/javascript` …)으로 제공하고, 요청 경로는 `[A-Za-z0-9._-]` 세그먼트만 허용하며(`..`·역슬래시·드라이브·스트림·인코딩된 변형 거부) 디스크 읽기는 정규화한 번들 루트 안으로 제한합니다. IPC 본문 한도(16 MiB)와 기타 1 MiB 한도는 불변입니다(번들 응답은 요청 본문이 없는 GET).
  8. **번들 출처 (CLI 단일 실행 파일)**: cargo 기능 `embed-hwp-studio`(기본 꺼짐)를 켜면 `build.rs`가 번들 전 파일을 `include_bytes!`로 내장해 릴리스 CLI는 `ui/hwp-studio` 폴더 없이 동작하고(당시에는 릴리스 워크플로가 이 기능으로 CLI를 빌드했으나 PD-086에서 릴리스 CLI를 없앴고 기능은 개발·CI용으로 유지), 기능이 꺼진 일반 빌드·테스트는 번들 없이도 통과하며 실행 파일 옆 `ui/hwp-studio`·`hwp-studio` 또는 소스 트리의 `ui/hwp-studio`에서 읽습니다. 기능이 켜졌는데 번들이 없으면 빌드가 실패합니다. 데스크톱 크레이트는 기능을 켜지 않으므로(Tauri가 이미 내장) 번들이 이중으로 들어가지 않습니다. `--self-test`는 새 자산(`hwp-host.js`, `#pane-hwp`)을 검사하고 기능이 켜진 빌드에서는 내장 번들의 `index.html`·`.wasm`·`asset-manifest.json`을 검사합니다.
  9. **`EasyOffice.exe --smoke`**: 셸이 smoke 모드에서만 `window.__EASYOFFICE_SMOKE__`를 주입하면 페이지가 한글 모듈을 활성화해 `ready` → 빈 문서 → `exportHwp` → CFB 서명·비어 있지 않음 → 내보낸 바이트를 편집기가 `verifyBytes`로 다시 열어 쪽 수 1 이상을 확인한 뒤 `bytes=<n> pages=<m>`을 `hwp` 단계 상세로 돌려줍니다(보고와 종료 코드는 PD-088 12항의 단계 보고가 맡습니다). 새 IPC 명령·권한은 없습니다. (이 개정 전에는 같은 출처 DOM에 합성 입력 이벤트를 넣고 `dirty`까지 확인했으나, 불투명 출처 프레임의 DOM은 셸이 읽을 수 없어 모두 포트 RPC로 바꿨습니다. 실제 입력 검증은 `scripts/hwp-studio-harness.mjs`가 프레임 자신의 CDP 세션으로 맡습니다.)
  10. **번들 보완(2단계 결함 수정)**: 업스트림 CSS의 도구 아이콘 스프라이트가 루트 절대 경로(`url(/images/...)`)라 `/hwp-studio/` 아래에서는 404였습니다. 두 SVG를 원본 해시가 고정된 verbatim 자산으로 번들에 추가하고 빌드가 CSS의 해당 경로를 `../images/`로 바꾸며, 번들 검사가 CSS의 루트 절대 `url()`을 실패로 처리합니다(번들 +0.88 MiB).
  11. **편집기 저장소 대체**: 불투명 출처 프레임에서는 `window.localStorage`/`sessionStorage`를 읽는 것만으로 `SecurityError`가 나고 `indexedDB.open`도 동기적으로 던집니다. 편집기는 번들 최상위에서 설정을 읽고 쓰므로(`?theme=dark|light`가 붙는 순간 `setThemeMode` → 설정 저장) 예외 하나로 모듈이 중단돼 "연결 시간 초과"가 되었습니다. 그래서 오버라이드한 `index.html`이 다른 스크립트보다 먼저 동기 외부 스크립트 `storage-shim.js`를 읽어(인라인은 CSP가 금지) 저장소에 닿을 수 없을 때만 두 저장소를 메모리 Storage로, `indexedDB`를 `undefined`로 바꿉니다(번들의 `typeof indexedDB` 분기가 메모리 대체로 감). 저장소가 되는 출처에서는 아무것도 건드리지 않습니다. 값은 프레임이 닫힐 때까지만 살아 있습니다(trade-off 6). 하니스가 `?theme=dark` 부팅, `setShellContext({theme})`, 설정 저장 왕복, 프레임의 예외 없음을 검사하고 `scripts/test-hwp-studio-storage-shim.mjs`가 shim 동작을 검사합니다. 참조 앱의 오버레이에는 이 용도의 shim이 없어(도구 상자 라벨 설정만 저장소를 직접 다룸) 새로 만들었습니다.
  12. **Windows의 프레임 IPC: 셸 호출 토큰과 `frame_ipc` 스모크**: wry 0.57.1은 Windows에서 `for_main_frame_only`를 무시하고 초기화 스크립트(`__TAURI_INTERNALS__`와 invoke 키)를 모든 프레임에 넣습니다(`wry-0.57.1/src/webview2/mod.rs:516-517`, `tauri-2.12.5/src/webview/webview_window.rs:978`). 프레임이 `window.chrome.webview.postMessage(<유효한 invoke 메시지>)`를 보내면 fetch를 거치지 않고, wry는 `args.Source()`를 요청 URI로 씁니다(`webview2/mod.rs:971-1000`). 그 값이 프레임 문서 URL(`http://tauri.localhost/hwp-studio/...`)이면 Tauri는 local로 판정해(`tauri-2.12.5/src/webview/mod.rs:1961-2001`, `on_message`) 14개 명령이 허용됩니다. WebView2 문서는 `add_WebMessageReceived`가 "최상위 문서"의 `postMessage`에 대해 돈다고만 적고 프레임의 메시지가 이 핸들러에 오는지는 적지 않으며, `AddScriptToExecuteOnDocumentCreated`는 "모든 최상위 문서와 하위 프레임"에서 돈다고 적습니다. 따라서 **Windows가 격리되어 있는지는 실측 전에는 알 수 없고, 이 문서는 "Windows도 격리됨"이라고 주장하지 않습니다.**
      * **검토한 대안**: (a) 편집기를 별도 스킴으로 서빙: 사용자 등록 프로토콜은 `is_local_url`의 셋째 절(`webview/mod.rs:1978-2000`, Windows는 `http://<이름>.localhost`의 이름이 등록 목록에 있는지)이 local로 분류해 오히려 권한이 맞고 Windows 프레임에는 invoke 키까지 있으므로 기각했습니다. 등록하지 않은 호스트는 처리기가 없습니다. 출처 판정이 remote가 되려면 앱이 모르는 출처여야 하는데, 그 길은 루프백 HTTP 서버로 번들을 서빙하는 것뿐이고(포트·ATS·방화벽·`frame-src`·CORS가 새 위험이라) 실기기 검증 없이 넣기에는 크다고 보아 채택하지 않았습니다. (b) 명령 처리기에서 요청 URL로 거부: `InvokeRequest.url`은 `on_message`에서 권한 판정에만 쓰이고 `InvokeMessage`/`CommandItem`으로 넘어가지 않아(`webview/mod.rs:135-150`, `ipc/mod.rs:498-507`) 공개 API로는 볼 수 없습니다. (c) 하위 프레임의 Tauri 객체를 지우는 초기화 스크립트: 객체가 `defineProperty`로 동결돼 있고 중첩 프레임으로 우회되므로 기각했습니다. (d) `invoke_system`으로 IPC 스크립트를 교체: Tauri 스크립트를 복사해 유지해야 하므로 기각했습니다.
      * **채택한 설계**: 앱 계층의 호출 토큰. 창마다 무작위 128비트 토큰을 만들어(`shell_caller.rs`) 초기화 스크립트가 `window.top === window`일 때만 전역으로 정의하게 하고(하위 프레임에는 정의되지 않으며 스크립트 원문은 페이지 코드에 보이지 않습니다), 데스크톱 다리(`desktop-bridge.js`의 유일한 `invoke` 호출)가 모든 명령에 `x-easyoffice-shell` 헤더로 실어 보냅니다. 모든 앱 명령(14개)은 `ShellCaller` 인자를 받아 토큰이 창의 것과 같을 때만 실행되고(상수 시간 비교), 구조 테스트가 모든 `#[tauri::command]`에 이 인자가 있는지 확인합니다. 토큰이 없으면 다리가 호출 자체를 하지 않습니다(fail-closed). 불투명 출처 프레임은 상위 문서의 전역을 읽을 수 없으므로 토큰을 얻을 수 없고, 전달 경로(fetch, `postMessage`)나 Windows의 `Source` 판정과 무관하게 명령은 거부됩니다. 같은 이유로 새 IPC 명령이나 권한은 늘지 않았습니다.
      * **실측**: `EasyOffice.exe --smoke`의 새 단계 `frame_ipc`가 편집기와 같은 샌드박스·같은 응답 정책(`studio_frame.rs`가 `frame-probe.html`에도 같은 CSP를 붙임)의 프레임에서 읽기 명령 `get_update_notice`를 호출해 봅니다. `allowed`(명령이 실행됨)면 `EASYOFFICE_SMOKE_STEP_FAIL frame_ipc`, `rejected`·`no-answer`·`absent`면 `EASYOFFICE_SMOKE_STEP_OK frame_ipc internals=… webview=… invoke=…`입니다. 상세 값이 Windows에서 Tauri 객체가 프레임에 들어오는지(`internals=present`)와 메시지가 도달하는지(`rejected`)를 알려 주므로, `verify-windows`의 첫 실행 로그가 이 항목의 실측 근거입니다. 이 단계는 모든 OS에서 필수입니다.
  13. **macOS(WKWebView)의 편집기 문서 CSP는 `'self'` 대신 앱 출처를 직접 적습니다**: 불투명 출처 프레임에서 WebKit은 CSP `'self'`를 맞추지 못해 프레임의 모든 하위 자원(클래식 스크립트 `storage-shim.js` 포함)을 요청을 보내기 전에 막았고, 그래서 macOS 스모크의 `hwp` 단계가 "한글 편집기 연결 시간 초과"로 실패했습니다(앱 프로토콜 응답 훅에 찍은 요청 로그에서 `/hwp-studio/index.html` 이후 하위 자원 요청이 0건, 편집기 문서의 CSP만 제거하면 요청이 이어지고 `hwp`·`pdf`·`print`·`frame_ipc` 통과; Windows(WebView2)·`--serve`의 헤드리스 Chrome에서는 `'self'`가 맞아 드러나지 않았습니다). 데스크톱은 `/hwp-studio/` HTML 응답의 CSP를 `hwp_bundle::studio_csp_for_origin`으로 문서의 실제 출처(macOS·Linux `tauri://localhost`, Windows `http://tauri.localhost`)로 채워 내고(`studio_frame.rs`의 `app_origin`), 지시어와 `ipc:` 부재는 `STUDIO_CSP`와 같습니다. 출처는 요청 URI에서 읽지 않고 플랫폼 상수로 정합니다. 처음에는 요청 URI에서 읽었다가 Windows `hwp` 단계가 "연결 시간 초과"로 실패했는데, wry가 WebView2의 `http://tauri.localhost/...`를 훅에 넘기기 전에 `tauri://localhost/...`로 되돌리기 때문입니다(`wry` 0.57.1 `custom_protocol_workaround::revert_uri_work_around`, `webview2::prepare_request`). 그 결과 CSP가 `tauri://localhost`만 허용해 실제 출처 `http://tauri.localhost`의 하위 자원이 막혔습니다(출처는 `tauri` 2.12.5 `protocol::origin`, 앱은 `useHttpsScheme`을 켜지 않음). 격리(불투명 출처, 토큰)는 그대로이며 로컬 macOS `--smoke` 전 단계 통과로 확인했습니다.
- **Why** 편집기 정확도는 rhwp 엔진에 있고 호스트는 얇게 유지해야 4단계(열기·HWPX 저장)와 후속 갱신이 쉽습니다. 권한 경계(IPC 없음·CSP·정적 검사)를 파일 단위가 아니라 문서 단위(iframe)로 둡니다.
- **Trade-offs**
  1. 앱 전체 CSP에 `wasm-unsafe-eval`이 들어가 메인 문서도 WebAssembly 컴파일이 가능합니다(Tauri는 문서별 CSP를 지원하지 않음). 앱 코드는 WASM을 쓰지 않고 `unsafe-eval`은 여전히 없습니다.
  2. NSIS 설치 파일이 2.69 MiB(2,816,373 B)에서 25.12 MiB(26,344,225 B)로 +22.4 MiB, 릴리스 EasyOffice.exe가 11.15 MiB에서 33.23 MiB로 커졌습니다(Tauri가 내장 자산을 압축하고 NSIS가 다시 압축). 번들 글꼴 18.4 MiB와 WASM 11.3 MiB가 대부분입니다.
  3. smoke의 입력은 합성 이벤트이고 실제 키보드 입력 검증은 헤드리스 Chrome 하니스가 맡습니다.
  4. 임베드 편집기의 키 입력은 iframe 안에서 처리되므로 앱 단축키(Ctrl+S 등)는 iframe 포커스에서 동작하지 않습니다(편집기 자체 저장 단축키는 임베드 모드에서 호스트에 위임되지 않음).
  5. WebView2의 부모 출처가 `http://tauri.localhost`라 임베드 프로토콜의 출처 검사(http/https만 허용)를 통과합니다. Tauri의 `tauri://localhost`(macOS·Linux) 출처는 통과하지 못하므로 이식 시 별도 결정이 필요합니다.
  6. 편집기가 브라우저 저장소를 쓰지 못해(11항의 메모리 대체는 프레임이 열려 있는 동안만 유지) 편집기 안에서만 기억하던 설정(도구 상자 라벨 표시 선택, 최근 입력한 기호, 편집기 로케일)이 세션마다 기본값으로 돌아옵니다. 테마는 셸이 주입합니다. 라벨 표시 선택을 유지하려면 셸 컨텍스트(`setShellContext`)로 값을 왕복하는 후속 작업이 필요합니다.
  7. `--serve`에서도 같은 샌드박스를 쓰므로 편집기 프레임은 서버 API에 `Origin: null`로만 닿고 POST는 `is_allowed_origin`이 거부합니다. 서버는 비밀 값이 없는 루프백 개발·CI용이라(PD-086) 그 밖의 경계는 바꾸지 않았습니다.
- **Alternatives rejected**
  - 부모 문서에 편집기를 직접 마운트: 전역·CSS·키 이벤트 충돌, IPC 권한 공유로 기각.
  - 하이퍼링크 미리보기를 오버레이에서 제거: 업스트림 포크가 늘고 sandbox 팝업 차단만으로 같은 효과가 나서 기각.
  - `--serve` 번들을 항상 디스크에서만 제공: CLI ZIP이 폴더에 의존하고 누락 시 조용히 폴백하므로 릴리스는 내장을 택함.
  - 번들 없는 데스크톱 빌드 허용: 설치본에 편집기가 없는 것을 CI가 놓칠 수 있어 빌드 실패로 기각.
- **Verified by** `node --test scripts/test-webui-hwp-host.mjs`(핸드셰이크·요청/응답·타임아웃·재연결·번들 없음 폴백·도구 숨김·저장), `cargo test -p easyoffice-app`(경로 순회·MIME·바이너리 정확·CSP), `cargo test -p easyoffice-desktop`(smoke 상태 기계), `EasyOffice.exe --smoke`(PD-083 개정부터 포트 RPC만 사용), `cargo test -p easyoffice-desktop`(`studio_frame.rs`: 편집기 응답에만 CORS 부여·IPC 없는 CSP), `node --test scripts/test-webui-security-ipc.mjs`(샌드박스 토큰·프레임 DOM 접근 금지·capability `local` 전용·`invoke` 호출 지점 하나와 토큰 헤더), `scripts/test-hwp-studio-storage-shim.mjs`, `scripts/test-webui-frame-probe.mjs`, `cargo test -p easyoffice-desktop`(`shell_caller.rs`: 토큰 검증과 모든 명령의 `ShellCaller`), Windows CI의 `frame_ipc` 스모크 표식, 헤드리스 Chrome 실제 입력(빈 문서 → 한글 입력 → 앞 3글자만 20pt·굵게 → 앱 저장 → 내보낸 HWP 재해석), 위생 가드의 정확한 CSP 검사.
- **Revisit when** 4단계(HWP/HWPX 열기·저장·더티 가드·`getDocumentState` 연동), 호스트와 편집기 단축키 통합, 앱 CSP를 문서별로 나눌 수 있게 될 때, macOS 이전(출처 검사). Tauri가 프레임 단위 권한을 지원할 때, 편집기 설정을 셸이 대신 기억해야 할 때.

## 59. 시트 범위 서식: 선택 범위 전체에 굵게·기울임·밑줄·취소선·글자 크기 (PD-084, #143)

- **Status** Implemented
- **Context** 시트에서 Shift+방향키로 범위를 고른 뒤 Ctrl+B·리본 서식을 눌러도 활성 셀 하나에만 적용되었습니다. 서식은 렌더된 `td`의 인라인 스타일에만 쓰였고, 가상 스크롤(PD-078)에서는 화면 밖 셀이 DOM에 없어 범위 전체 적용이 불가능했습니다(#143).
- **Decision (`PD-084`)**
  1. **모델에 기록**: 새 모듈 `sheet-format.js`(`__EASYOFFICE_CREATE_SHEET_FORMAT__`)가 `sheetDataStore`의 셀 항목 `style`(`fontWeight`·`fontStyle`·`textDecoration`·`fontSize`)을 범위의 모든 셀에 씁니다. 그 뒤 `refreshWindowFromModel()`로 보이는 창만 다시 그리므로 스크롤·시트 탭 전환·재렌더 후에도 서식이 유지됩니다. `ribbon-history.js`의 셀별 `td.style` 분기는 제거하고(줄 수 감소) `sheetController.applySheetFormatCommand`로 위임합니다.
  2. **토글 규칙**: 범위의 모든 셀이 이미 켜져 있으면 해제, 하나라도 꺼져 있으면(없음·혼합) 전체 설정입니다. 밑줄·취소선은 `textDecoration` 토큰(`underline`, `line-through`)으로 합쳐 서로 지우지 않습니다. 취소선은 `Ctrl+5`(시트 포커스)입니다. 글자 크기는 활성 셀의 현재 크기에서 다음 프리셋(13/15/17/12px)을 골라 범위 전체에 같은 값을 씁니다.
  3. **실행 취소 한 단계**: 리본·단축키 경로는 기존 추적 리본 작업(스냅샷 1회) 안에서 실행되고, `Ctrl+5` 경로는 단일 `pushRawUndoSnapshot` 작업입니다. `Ctrl+Z` 한 번으로 범위 전체가 되돌아가고 `Ctrl+Y`로 다시 적용됩니다.
  4. **도구 막대 상태**: 선택이 바뀔 때(2만 칸 이하는 즉시, 그 이상은 120 ms 지연) 리본 `sheet-home-*` 버튼의 `aria-pressed`를 범위 기준 `true`/`false`/`mixed`로 갱신합니다. 모델이 기준이라 화면 밖 셀도 반영됩니다. 모듈 범위는 `SHEET`로 한정하여 DOC/HWP/SLIDE(`doc-runs-ui.js`·슬라이드 경로)는 바뀌지 않습니다.
  5. **대형 선택**: 범위가 100만 칸을 넘으면(Ctrl+A는 1,048,576×16,384) 사용 범위(행 수·열 수)로 줄여 적용합니다. 서식만 있고 값이 없는 셀은 모델에 항목을 만들며, 서식이 모두 꺼지면 항목을 지웁니다.
  6. **저장 한계 (해결됨, `PD-089`)**: 이 결정 시점에는 저장 `sheets` 페이로드(#104)가 `{ref, raw}`만 담아 셀 서식이 XLSX에 기록되지 않았습니다. #148에서 페이로드에 셀 `style`을 추가해 XLSX(`styles.xml`)와 ODS(자동 스타일)에 기록하고 다시 열 때 복원합니다. 자세한 내용은 `PD-089`를 보세요.
- **Alternatives rejected**
  - 렌더된 `td`만 순회: 가상 스크롤 밖 행이 빠져 기각.
  - 셀마다 스냅샷: 실행 취소가 N단계가 되어 기각.
  - 전체 선택에서 100만 칸 초과 항목 생성: 메모리·지연이 커서 사용 범위로 제한.
- **Verified by** `node --test scripts/test-webui-sheet-format.mjs`(범위 굵게 전체/없음/혼합 토글, 실행 취소·다시 실행 한 단계, 기울임·밑줄·크기·취소선 결합, 1만 행 범위의 화면 밖 행·스크롤 후 유지, 시트 탭 전환 유지, 혼합 `aria-pressed`, DOC 비영향), 헤드리스 Chrome(CDP `Input.*`)에서 Shift+방향키 선택 후 Ctrl+B로 12칸 모두 굵게, 1만×8 범위 굵게 적용이 실제 브라우저에서 약 0.36초(실행 취소 스냅샷 포함)이며 스크롤한 8,326행도 굵게였습니다.
- **Revisit when** XLSX 저장 페이로드에 셀 스타일을 담을 때, 글꼴·색·채우기·테두리·정렬을 범위에 적용할 때, 열·행 단위 서식(전체 열 서식)을 도입할 때.

## 60. 한글 열기·HWP/HWPX 저장·수정 상태·IndexedDB 자동 저장 (PD-085, #131)
- **Context** rhwp 통합 4단계입니다(설계 `docs/rhwp-integration-design.md` §2.4~§2.7). 3단계(PD-083)의 임베드 편집기에 파일 열기·저장·수정 상태·자동 저장을 연결합니다. 사용자 결정: HWPX 저장 허용, HWP 열기 한도 64 MiB, 자동 저장은 EasyOffice 부모가 IndexedDB로 소유, rhwp가 못 여는 파일은 텍스트 전용으로 대체.
- **Decision (`PD-085`)**
  1. **열기** (`ui/hwp-session.js`, 새 모듈): `file-open.js`의 `openDocumentFromBytes`(파일 선택·Ctrl+O·끌어놓기·셸 파일 연결 공용)가 확장자 `hwp/hwpx/hwt/hwtx`이면 세션으로 위임합니다. 순서는 ① 크기 검사(64 MiB, 초과 시 "최대 64 MiB" 안내, 빈 파일 거부) ② `detect_format` IPC(기존 15종 중 하나, 파일 이름 힌트 + 앞 16바이트 hex만 보내 64 MiB를 Base64로 보내지 않음, 매직 바이트 불일치는 fail-closed로 열지 않음) ③ 저장하지 않은 편집이 있으면 확인 ④ 편집기 준비(필요하면 한글 모듈 활성화·재연결) ⑤ `loadFile({data, fileName, skipUnsavedGuard:true})`을 복사본 ArrayBuffer transfer로 호출(호출자의 바이트는 유지, 대기 시간 10분 — 비밀번호 입력 대기). 탭 제목은 파일 이름입니다.
  2. **암호·배포용·DRM**: 비밀번호 입력 대화상자는 편집기(rhwp) 안에서 뜨고 암호 문자열은 부모로 오지 않습니다. 취소는 "열기 취소"로 끝나 이전 문서가 유지됩니다. rhwp가 거부하는 DRM·미지원 암호화·손상된 암호 문서는 명시 메시지로 열기를 거부하고 텍스트 대체를 하지 않습니다(암호 문서를 평문처럼 읽지 않는 보안 입장). 배포용 문서 등 그 밖의 형식 지원 범위는 rhwp를 따릅니다.
  3. **텍스트 전용 대체**: 편집기를 쓸 수 없거나(번들 없음·연결 실패) rhwp가 파싱에 실패하면 기존 #111 디코드 경로(`textOnly`)로 텍스트만 불러오고, 상태줄과 편집 영역 위 안내에 "서식·표·그림은 유지되지 않으며 저장하면 서식이 사라집니다"를 명시합니다. 이 상태에서 "한글 편집기로 돌아가기" 단추로 임베드 편집기를 다시 연결할 수 있습니다. 텍스트 경로의 한도는 기존 10 MiB입니다.
  4. **저장**: Ctrl+S·저장 단추·팔레트 = 문서가 HWPX로 열렸거나 HWPX로 저장한 적이 있으면 `exportHwpx`, 아니면 `exportHwp`. 파일 메뉴의 "한글 문서로 저장 (HWP/HWPX)…"가 형식을 고르는 다른 이름으로 저장이며 이후 저장은 그 형식을 따릅니다. 전달 직전 내보낸 바이트를 편집기에 `verifyBytes`(오버레이 확장 RPC, `verify-bytes-v1`)로 보내 **버려지는 별도 문서로 다시 파싱**하고 쪽 수가 0보다 클 때만 전달합니다. 검증 기능이 없거나 실패하면 저장하지 않으며(fail-closed), HWPX 검증 실패 시 "HWP 형식으로 대신 저장할까요?"를 묻고 승인하면 HWP를 같은 방식으로 검증해 저장합니다. 전달은 기존 경로(데스크톱 네이티브 대화상자 / `--serve` Blob 다운로드)입니다.
  5. **수정 기준선(PD-073/074)**: `notifySaved`는 전달이 실제로 성공했고 저장하는 동안 추가 편집이 없을 때만 호출합니다. 대화상자 취소·전달 실패·검증 실패에서는 수정됨 상태와 자동 저장본이 그대로입니다. 저장 중 편집이 있으면 "수정됨 상태를 유지합니다"를 알립니다. HWPX 성공 메시지에는 "일부 서식이 HWP와 다르게 저장될 수 있어 한글에서 열어 확인하세요" 안내가 붙습니다.
  6. **수정 상태·이탈 경고**: 오버레이 `main.ts`가 편집기의 `document-dirty-changed`와 편집마다의 `document-mutated/changed`를 임베드 이벤트(`documentChanged`, reason `dirty_changed`/`edited`)로 부모에 보냅니다. 업스트림의 `getDocumentState()`는 매 호출마다 문서 전체를 내보내 SHA를 계산하므로 큰 문서에서 폴링할 수 없기 때문입니다. 세션이 이 이벤트로 수정 여부를 갖고 `workspace-session.js`의 `setExternalDirty`로 작업 공간 전체 이탈 경고(`beforeunload`)와 `data-dirty`에 합칩니다. 편집기 연결이 끊기면 수정 상태는 초기화됩니다(자동 저장본은 남음).
  7. **자동 저장(IndexedDB)**: DB `easyoffice.hwp.autosave.v1`의 `records`(`{tabId, fileName, format, savedAt, size, bytes}`, 탭당 1건)와 용량이 작은 `meta`. 수정됨일 때만, 마지막 편집 5초 후(연속 편집은 최대 30초마다) `exportHwp`/`exportHwpx` 바이트를 저장하고, 64 MiB를 넘으면 건너뛰고 기존 본을 지우며 알립니다. 저장 성공·다른 파일 열기·삭제 시 지웁니다. 새로고침 후 같은 탭(`sessionStorage`의 탭 ID)의 본이 있으면 레코드 모양 검사 → `detect_format`(`autosave.<hwp|hwpx>` 힌트) 통과 시에만 "복원할까요?"를 묻고 복원하며(복원 문서는 수정됨 상태), 손상·형식 불일치·거절은 지우고 무시합니다. 7일 지난 다른 탭의 본은 정리합니다. 저장소 오류는 상태줄 메시지일 뿐 편집기에 영향이 없습니다.
  8. **변환기**: 변환기 원본 형식이 `hwp`/`hwpx`이고 파일을 따로 고르지 않았으며 한글 편집기에 문서가 열려 있거나 수정 중이면 편집기가 내보낸 바이트를 `sourceBase64`로 보냅니다(변환기 → DOCX/PDF 등). 빈 편집기만 있으면 기존 동작입니다.
  9. **오버레이 변경**: `verifyBytes` RPC·`verify-bytes-v1` capability·수정 이벤트 전달(`apps/hwp-studio/src/{main.ts,embed/rpc-router.ts,embed/protocol.ts}`, 오버레이 매니페스트에 이유 기재). IPC 명령 수(15)·권한은 불변입니다.
- **Why** 한글 문서는 편집기가 원본 레코드를 보존해 직렬화하므로 #104의 평탄 모델 저장 경로를 쓰지 않아야 서식이 유지됩니다. 저장 전 재파싱은 "저장은 성공했는데 열리지 않는 파일"을 막고, 자동 저장은 5 MB 한도의 `localStorage` 스냅샷에 담을 수 없는 이진을 위한 별도 저장소가 필요합니다.
- **Trade-offs**
  1. 키보드 포커스가 편집기 iframe 안이면 앱 단축키(Ctrl+S·Ctrl+O)가 부모에 전달되지 않습니다(5단계 단축키 포워딩 과제). 메뉴·저장 단추·팔레트는 동작합니다.
  2. 자동 저장은 비동기라 마지막 편집 후 5초 안(연속 편집 중에는 최대 30초)에 창을 닫으면 그 구간은 복원되지 않습니다(`beforeunload` 경고가 먼저 뜹니다). 복원은 같은 탭(새로고침)에서만 되며 앱을 완전히 종료하면 탭 ID가 사라져 복원하지 않습니다(7일 뒤 정리, 6단계에서 재검토).
  3. HWPX 내보내기는 HWP 대비 일부 속성이 달라질 수 있습니다(예: 표 바깥 여백이 0 → 283으로 기록됨, 글자·문단 모양·표 위치는 같음). 저장 시 확인 안내를 붙이고 검증을 통과한 파일만 내보냅니다.
  4. 저장 전 검증은 구문 해석 가능성과 쪽 수만 확인하며 한글 프로그램과의 조판 일치는 보장하지 않습니다.
  5. 편집기 이벤트는 모든 편집마다 포트 메시지를 보냅니다(부모가 디바운스).
  6. 비밀번호로 연 문서를 저장할 때 저장본에 암호가 유지되는지는 이 단계에서 검증하지 않았습니다(미검증). 저장된 파일을 다시 열어 확인해야 합니다.
- **Alternatives rejected**
  - `getDocumentState()` 폴링: 호출마다 전체 문서를 내보내 대형 문서에서 느려 기각(이벤트 전달로 해결).
  - 부모 쪽 WASM으로 재파싱: 앱 CSP를 넓히거나 12 MiB WASM을 이중 적재해야 해서 기각(편집기 안에서 버려지는 문서로 재파싱).
  - 업스트림 `recovery/`(IndexedDB 복구) 사용: 복원 UI·검증을 부모가 통제해야 해서 기각.
  - rhwp 실패 시 조용한 평문 대체: 서식 유실을 숨기므로 기각(명시 안내를 붙인 대체만 허용).
  - HWPX 저장 금지(업스트림 정책): 사용자 결정으로 허용하되 검증·HWP 대안 제안을 둠.
- **Verified by** `node --test scripts/test-webui-hwp-session.mjs`(열기 라우팅·`detect_format` 힌트·64 MiB 한도·빈 파일·거부·텍스트 대체·비밀번호 취소/DRM·번들 없음, HWP/HWPX 저장 선택·검증 실패·HWPX→HWP 제안·취소/실패 기준선 유지·저장 중 편집, 수정 이벤트, 자동 저장 디바운스·크기 상한·저장 후 삭제, 복원 검증·손상 레코드·거절·오래된 본 정리, 변환기 원본), `node --test scripts/test-webui-hwp-host.mjs`, `node scripts/hwp-roundtrip-cdp.mjs`(헤드리스 Chrome 실제 입력: HWP·HWPX 열기 → 앞 3글자 20pt+굵게 → 저장(같은 형식·다른 형식) → 다시 열기, 글자·문단 모양·표 위치 동일, 11 MiB 문서 열기·저장, 편집 → 5초 후 IndexedDB 본 → 새로고침 → 복원 → 저장 → 본 삭제), 위생 가드.
- **Revisit when** 5단계(단축키 포워딩·다중 탭: 탭 ID별 자동 저장 확장), 6단계(네이티브 어댑터·파일 연결·제자리 저장·원시 바이트 저장 IPC: Base64 3회 복사 제거), rhwp 갱신으로 `exportHwpxWithReport` 손실 보고를 노출할 수 있을 때.

## 61. `--serve` 루프백 서버를 개발·테스트 전용으로 한정하고 릴리스 배포 중단 (PD-086, #151)
- **Context** EasyOffice는 서버 없이 설치해 쓰는 프로그램이고 설치형 데스크톱 앱(#121)은 앱 내부 IPC만 쓰며 열린 네트워크 포트가 없습니다. 그런데 릴리스에는 루프백 HTTP 서버를 띄우는 `--serve` CLI zip이 보조 산출물로 남아 있었습니다.
- **Decision (`PD-086`)**
  1. 제품은 서버리스 설치형입니다. `--serve`는 헤드리스 브라우저 UI 검증과 CI 스모크 같은 개발·자동 검증에만 쓰며 사용자에게 배포하지 않습니다.
  2. `release.yml`은 CLI 빌드·zip·체크섬 항목·`--serve` 스모크를 모두 제거했습니다. 릴리스 자산의 구성은 7~9항이 정합니다. `ci.yml`의 `--serve` 스모크와 cargo 기능 `embed-hwp-studio`(테스트 포함)는 유지합니다.
  3. 위생 가드(`scripts/release-serve-rule.mjs`, 테스트 `scripts/test-release-serve-rule.mjs`)가 `release.yml`(주석 제외)에 CLI 서버 산출물 이름·`--serve`·CLI 실행 파일 복사가 있으면 실패합니다(fail-closed, 파일이 없어도 실패).
  4. `--help`와 `--serve` 시작 메시지가 "개발·테스트 전용, 일반 사용자용이 아님"을 한국어·영어로 밝힙니다. 방어(Host·Origin 검사, 연결 수·시간 제한, CSP)는 개발 중 안전을 위해 그대로 유지합니다.
  5. README·README.ko의 사용자 안내는 설치 프로그램 기준이고 `--serve`는 짧은 개발자 절로 옮겼습니다.
  6. (#215) 릴리스 버전 정합: 루트 `[workspace.package] version`이 제품 버전의 정본이고(현재 0.1.0), `tauri.conf.json`은 `version`을 두지 않아 Tauri가 Cargo 버전을 씁니다(릴리스 빌드는 태그 버전을 `--config`로 덮어씀). `release.yml`의 첫 잡 `prepare`는 버전·태그·드래프트 여부를 한 번 정해 모든 잡에 넘기고, 태그 push일 때 태그(`v` 제거)가 워크스페이스 버전과 다르면 빌드 전에 실패시키며, 모든 빌드 잡이 이 잡을 기다립니다. `workflow_dispatch`에는 `publish` 입력(boolean, 기본 true)이 있어 `false`면 빌드와 Actions 아티팩트 업로드까지만 하고 `publish-release` 잡(`gh release create`·upload)을 건너뜁니다. 릴리스 후보는 `publish=false`로 설치 파일만 받아 확인합니다. 수동 실행은 태그가 없으므로 버전 검사 대상이 아닙니다. 정식 출시 전이라 버전을 0.x로 다시 시작합니다(QA 결정): 이전 v0.1.0~v2.0.0 릴리스·태그 14개는 2026-11-08에 삭제했습니다.
  7. 릴리스 채널(QA 요청): macOS 유니버설 `.dmg`, Windows x64 NSIS 설치 프로그램, Linux x64 AppImage·`.deb`·`.rpm`, Linux arm64 `.deb`. Linux 빌드 잡(`build-linux-release`)은 x64를 `ubuntu-22.04`, arm64를 `ubuntu-22.04-arm`에서 만들어 패키지의 glibc 기준선이 2.35입니다. WebKitGTK 4.1·GTK 3·libxdo·OpenSSL 3(업데이트 확인의 TLS)은 `tauri.conf.json`의 `bundle.linux`가 의존성으로 선언합니다(deb는 패키지 이름, rpm은 배포판마다 패키지 이름이 달라 라이브러리 capability). Linux 파일 연결은 `.desktop`의 `MimeType=`(문서 9종 확장자 전부에 MIME 형식을 지정)이고 메뉴 분류는 Office입니다. Tauri 기본 `.desktop` 템플릿은 `Exec`에 파일 필드 코드를 넣지 않아 더블클릭해도 문서가 앱에 전달되지 않으므로, `crates/easyoffice-desktop/linux/easyoffice.desktop`을 `bundle.linux.deb.desktopTemplate`와 `rpm.desktopTemplate`로 지정합니다(AppImage는 번들러가 deb 템플릿을 그대로 씁니다). 템플릿은 `Exec=… %F`로 선택한 파일을 모두 경로로 넘기고, 앱은 시작 인수와 단일 인스턴스로 전달된 인수를 각각 새 창으로 엽니다(`window_commands.rs`의 `open_startup_files`·`handle_second_instance`). Linux GUI 스모크는 아직 돌리지 않습니다(`TODO.md`). 빌드 도구·패키지 목록은 공용 composite action(`.github/actions/install-linux-deps`, `prepare-desktop-build`)에 한 번만 둡니다.
  8. 고정 파일명(QA 요청): 자산 이름에 버전을 넣지 않아 `https://github.com/AquilaXk/easyoffice/releases/latest/download/<이름>` 링크가 릴리스마다 유지됩니다. 이름은 `EasyOffice-macos-universal.dmg`, `EasyOffice-windows-x64-setup.exe`, `EasyOffice-linux-x64.AppImage`, `EasyOffice-linux-x64.deb`, `EasyOffice-linux-x64.rpm`, `EasyOffice-linux-arm64.deb`이고 체크섬은 합친 `SHA256SUMS.txt` 하나입니다. 이름·번들 위치·`--bundles` 값은 `scripts/release-assets.mjs` 한 곳에 있고, 빌드 잡은 번들이 정확히 하나가 아니면 실패하며, 발행 잡은 위 6개가 정확히 있지 않으면 `SHA256SUMS.txt`를 만들지 않고 실패합니다. README·README.ko의 직링크가 이 이름과 같은지는 테스트가 강제합니다. 이전 `EasyOffice-<version>-…` 이름과 플랫폼별 체크섬 파일은 대체되었습니다. 이미 공개된 릴리스에 다시 실행하면 자산이 하나씩 교체되므로 그동안 `SHA256SUMS.txt`와 자산이 잠깐 어긋날 수 있습니다(드래프트는 공개 전이라 영향이 없음).
  9. 드래프트 릴리스(QA 요청): 릴리스는 기본이 드래프트입니다. 수동 실행의 `release_draft` 입력(기본 true)이 `false`일 때만 바로 공개하고(`publish=true`이면 `main`에서 실행한 경우만 허용), 태그 push는 항상 드래프트로 만듭니다. 수동 실행의 `version`은 `1.2.3` 또는 `1.2.3-rc.1` 형태만 받고(`prepare`의 첫 단계), 동시성 그룹은 버전·`publish`·`release_draft`로 나뉘어 공개 요청이 같은 버전의 드래프트 요청에 취소되지 않습니다. QA가 드래프트의 자산을 확인한 뒤 공개합니다. `publish=false`(Actions 아티팩트만)는 그대로입니다. 이미 있는 릴리스에 다시 실행하면 자산만 교체하고, 공개된 릴리스를 드래프트로 되돌리지는 않습니다. **업데이트 알림(PD-103)과의 관계**: 알림은 `releases/latest`를 보고 GitHub는 드래프트를 `latest`에 넣지 않으므로, 드래프트 동안에는 알림도 README 직링크도 새 릴리스를 가리키지 않습니다. 공개하는 순간 둘이 함께 새 릴리스를 가리킵니다.
  10. 배포물 라이선스 고지(MIT 고지 조건): `THIRD_PARTY_NOTICES.md`를 `bundle.resources`로 모든 OS 산출물에 넣습니다(macOS `.app`의 `Contents/Resources`, Windows 설치 프로그램, Linux 패키지의 앱 리소스 폴더). 구성은 `scripts/desktop-file-associations.mjs`의 계약이 검사하고, 산출물은 CI·릴리스가 직접 열어 확인합니다(macOS는 `.app` 안 파일, Windows는 7-Zip으로 설치 프로그램 목록, Linux는 `scripts/verify-linux-bundles.mjs`).
  11. Linux 산출물 검사(`scripts/verify-linux-bundles.mjs`, CI `verify-linux`와 릴리스 Linux 잡): AppImage는 실행 없이 `--appimage-extract`로 풀어 구조를 보고, deb는 control의 아키텍처·의존성과 풀린 내용을, rpm은 아키텍처·Requires·파일 목록과, `.desktop` 파일의 SHA-256(rpm 자체의 파일 다이제스트)이 검사를 통과한 deb의 `.desktop`과 같은지를 확인합니다(같은 템플릿으로 만들어지므로 내용이 같아야 함. 러너의 `rpm2cpio`가 Tauri가 만든 rpm에서 메시지 없이 실패해 페이로드는 풀지 않습니다). 모든 패키지에 실행 파일·`.desktop`·고지 파일이 있어야 하고 `.desktop`은 Office 분류, 파일 인수 필드 코드(`%U` 등), 문서 9종 MIME 형식을 가져야 합니다. CI `verify-linux`는 x64 패키지까지 빌드해 검사하고 Linux 데스크톱 크레이트의 clippy·단위 테스트를 돌립니다(arm64 `.deb`는 릴리스에서만 빌드).
- **Why** 사용자에게 로컬 포트를 여는 별도 실행 파일을 주지 않아 공격면과 안내 혼선을 없애면서, 자동 UI 검증 경로는 잃지 않습니다.
- **Trade-offs** 외부 사용자가 브라우저로 미리 써 보는 경로가 사라지고 소스 빌드가 필요합니다. 릴리스에서는 `--serve` 바이너리 스모크가 빠지고 CI가 같은 코드를 검증합니다.
- **Alternatives rejected** `--serve` 코드와 방어 제거(자동 검증 수단 상실), zip을 계속 배포하되 안내만 변경(사용자가 받을 수 있는 서버 산출물이 남음).
- **Verified by** `node --test scripts/test-release-serve-rule.mjs scripts/test-release-assets.mjs scripts/test-verify-linux-bundles.mjs scripts/test-desktop-file-associations.mjs`, `node scripts/check-repo-hygiene.mjs`, `cargo test -p easyoffice-app`(`--help` 문구), 기존 CI `--serve` 스모크, CI `verify-linux`(Linux 빌드·구조 검사), 릴리스 수동 실행 `publish=false`(세 OS 산출물과 구조 검사).
- **Revisit when** 정식 웹 배포 계획이 생기거나 UI 검증 방식이 `--serve` 없이 가능해질 때, 서명 키를 도입해 자동 설치 업데이터를 만들 때, Linux GUI 스모크가 가능해질 때, arm64에 AppImage·rpm 요구가 생길 때.

## 62. Apple Pages(.pages) 읽기 지원 범위·스키마 검증 상태·법적 접근 (PD-087, #154)

- **Superseded by PD-094 (부분)**: 이 항목 중 Swift 앱·`swift build`/`swift test`·`scripts/package-macos-release.sh`·Swift 대응 규칙에 해당하는 부분은 PD-094로 대체되었습니다. Rust 엔진·WebUI·Windows 관련 결정은 그대로 유효합니다.
- **Context** Pages는 사양이 공개되지 않은 형식입니다. 2013년 이후 문서는 ZIP 안의 `Index/*.iwa`(Snappy 변형 청크로 감싼 protobuf 레코드)이고 객체 종류(type ID)와 필드 번호는 파일 안에 없습니다. 참고 구현 중 일부는 MPL-2.0이거나 문서가 비상업 조건(CC BY-NC-SA)이며, 추출된 `.proto` 파일의 권리 상태가 불명확합니다.
- **Decision (`PD-087`)**
  1. 형식 지원 범위: Pages는 **읽기 전용 원본**입니다. `DocumentFormat::Pages`(확장자 `pages`, 컨테이너 `ZipIwork`)를 추가하되 `DocumentFormat::ALL`(읽기·쓰기 20종)에는 넣지 않고 `READ_ONLY`로 분리합니다. 저장·변환 대상으로 지정하면 `encode`가 명확한 오류를 반환합니다. Swift 쪽 20종 목록·매크로 Info.plist 검증·자체 시험의 "20종"은 그대로 유지되어 Rust↔Swift 목록 정합이 깨지지 않습니다.
  2. 열기: 본문 텍스트, 문단/글자 스타일(굵게·기울임·밑줄·취소선·크기·색), 정렬, 제목(스타일 식별자), 단순 목록, 표(2단계: 텍스트·숫자·날짜·통화·백분율·체크박스·수식 캐시 값을 표시 문자열로 가져오고 병합으로 가려진 칸은 빈 칸)를 `RichDocument`로 가져옵니다. 지원하지 않는 요소(그림, 도형·텍스트 상자, 각주, 댓글, 머리글·바닥글 텍스트, 읽을 수 없는 표 셀 값)는 본문에 섞지 않고 건너뛰며 종류(예: `그림(형식 3005)`, `도형·텍스트 상자(형식 2011)`)와 개수를 경고로 보여 줍니다. 각주·머리글·도형 안 글은 `RichDocument`에 둘 자리가 없어 본문 끝이나 중간에 넣으면 순서가 틀려지므로 가져오지 않기로 했습니다. 암호화 문서는 명확한 오류로 거부하고, 구형 XML(`index.xml`, 2009 이전)은 판별해 "아직 지원하지 않음" 메시지로 거부합니다(v2).
  3. 보안: ZIP은 기존 `ZipArchiveCodec::unpack` 가드를 재사용합니다. IWA 청크 압축 해제 1 MiB·파일 64 MiB·문서 256 MiB, 레코드 1,000,000개, protobuf 깊이 32·메시지당 필드 65,536, 표 행 10,000·열 256·셀 총 4,000,000, 문단 200,000 상한을 모두 오류로 거부하며 `unsafe`는 쓰지 않고 인덱스 계산은 `checked_*`입니다. 참조 순환은 방문 집합과 깊이 제한(스타일 부모 사슬 16단계)으로 끊습니다.
  4. 법적 접근: Apple `.proto` 파일과 비허용 라이선스 코드·문서는 저장소에 복사하지 않습니다. 필요한 type ID·필드 번호만 `pages_schema.rs`에 자체 상수로 적고, 실파일로 확인된 것과 아닌 것을 상수마다 구분합니다(미확인은 `// 검증 필요`). 구조 이해는 공개 문서와 독립 구현에서 관찰되는 와이어 형식을 참고했고 인용한 문구는 없습니다.
  5. 스키마 검증 상태(2단계): 웹판 Pages 문서 1건(파일 형식 버전 184)과 `crates/easyoffice-formats/tests/fixtures/pages/eo-pages-{text,lists-tables,objects}.pages` 3건(메타데이터·템플릿 이미지 제거, 커밋 승인)으로 확인했습니다. 상수마다 `pages_schema.rs`에 확인/`// 검증 필요`를 적었고, 3건에서 새로 확인된 것은 다음과 같습니다.
     - 글자 속성: 스타일 객체에는 필드 11/12(전체 배치)와 필드 2/3(간략 배치) 두 가지가 있습니다. 간략 배치는 굵게 1·기울임 2·밑줄 3·취소선 4·크기 6(float)·색 7·형광펜 19이고 전체 배치는 굵게 1·크기 3·색 7·밑줄 11(하이퍼링크 글자 스타일로 확인)입니다. 정렬 값 0 왼쪽·1 오른쪽·2 가운데·3 양쪽, UTF-16 비BMP 위치(이모지 뒤 굵게 구간), 제목 식별자 `Title`, 목록 문단 적용(글머리표·번호, 단계 표는 저장소 필드 6), 각주·댓글 표(저장소 필드 11·19)와 머리글/바닥글 목록(형식 10143, 필드 1·2), 그림 형식 3005, 도형·텍스트 상자 형식 2011이 확인되었습니다.
     - 표: 타일은 형식 6002 또는 6003이고, 풍부한 텍스트 목록의 자체 형식 번호는 문서마다 달라 검사하지 않습니다. 포장 셀은 두 가지 배치(웹판: 버전 바이트 5·플래그 바이트 8, 3건: 버전 바이트 1·플래그 바이트 4·필드 바이트 8)이며 필드는 플래그 비트 순서입니다. 형식 바이트 2 숫자(decimal128, 값 비트 6)·5 날짜(초 f64, 2001-01-01 기준)·6 체크박스(f64)·9 풍부한 텍스트(키 비트 9)·형식 키 비트 10, 서식 목록(저장소 필드 22: 형식 256 통화·257 숫자·258 백분율·261 날짜 패턴·264 체크박스, 소수 자릿수·통화 코드)이 확인되었습니다.
     - 여전히 미확인(`// 검증 필요`): 기울임·취소선의 전체 배치 필드(3건에서 값이 항상 0), 그림 글머리표 값 1, 천 단위 구분 필드(서식 필드 5)의 의미(숫자·백분율·통화 서식에 모두 1이 있고 화면 대조본이 없어 `1,234.5`처럼 켜진 것으로 표시), 문자열 셀(형식 3)·기간 셀(7)·플래그 비트 7·8의 크기, 암호화 표지, 전체 배치 형광펜 필드. 참고: 제공된 첫 쪽 미리보기 3장은 서로 같은 그림이고 어느 문서의 내용과도 일치하지 않아(다른 문서의 표) 화면 대조에는 쓰지 못했고, 본문 글·표 안 글은 제작 의도(제목·머리글 문구)와 파일 안 텍스트로 대조했습니다.
- **Why** 비공개 형식이므로 "열 수 있는 것은 열고 못 읽는 것은 정직하게 알린다"가 제품 신뢰에 맞고, 스키마 의존 부분을 한 파일에 격리하면 실파일이 늘 때 숫자만 고치면 됩니다.
- **Trade-offs**
  1. 실파일 4건 기준이라 다른 버전·기능(문자열·기간 셀, 병합 정보, 이미지 등)은 건너뛰거나 틀릴 수 있습니다. 표시 문자열은 서식 목록으로 만든 근사이며 Pages 화면과 완전히 같다고 보장하지 않습니다.
  2. 합성 픽스처는 우리가 가정한 스키마를 우리가 검증하는 순환 테스트이므로 호환성의 증거는 실파일 시험(`EASYOFFICE_PAGES_SAMPLE`)뿐입니다.
  3. Pages는 `ALL`에 없으므로 형식 순회 코드(예: 모든 형식 인코딩 시험)는 Pages를 자동으로 포함하지 않습니다.
- **Alternatives rejected**
  - `snap` 크레이트 사용: 청크 프레이밍 변형 때문에 래퍼가 필요하고 unsafe 여부 확인 비용이 커 직접 구현(약 100줄)을 택함.
  - `.proto` 복사 또는 생성 코드: 권리 불명확으로 기각.
  - `ALL`을 21종으로 확대: Swift 정합·"20종" 문서 계약·모든 형식 쓰기 순회 시험을 깨므로 기각.
- **Verified by** `cargo test -p easyoffice-formats --test pages_reader --test pages_security`(프레이밍·Snappy·protobuf·레코드·매핑·판별·거부·결정적 PRNG 퍼징·순환·한도), 실파일 3건 시험 `pages_fixtures`(본문·서식·제목·목록·표 셀 정확한 내용과 건너뛴 개체 경고), 환경변수 지정 실파일 시험 `pages_real_sample`(구조만 확인), `node --test scripts/test-webui-file-open.mjs`.
- **Revisit when** 실파일이 늘어 남은 미확인 상수(기울임·취소선 전체 배치, 문자열·기간 셀, 천 단위 구분)를 확정할 때, 화면 대조본(Pages가 그린 표 쪽 이미지)을 확보했을 때, 구형 XML(v2), 이미지(`Data/`)·각주·머리글 지원, 병합 정보 보존, Pages 쓰기 검토.

## 63. 문서별 개별 창 모델·열기 큐 보안·단축키 전달·아이콘 세트 (PD-088, #132, #126)
- **Context** 문서 탭 줄 시도(모듈 전환 탭, 한 창에 여러 문서)는 결과물이 조잡해 폐기했고, 사용자가 "문서마다 개별 창"으로 방향을 바꿨습니다. 같은 시기에 셸 화면(#126)은 1024px에서 우측 컨트롤이 잘리고, 상태 표시줄이 글을 자르며, 이모지·문자 기호가 아이콘 노릇을 하고, 일부 아이콘 버튼에 이름이 없는 문제가 있었습니다. 참고 구현은 참조 앱의 다중 창 구조(창별 열기 큐, 창 생성, 끌어 놓기)입니다. 코드를 가져오지 않고 구조만 따랐고, 참조 앱과 달리 WebView에는 경로를 주지 않습니다.
- **Decision (`PD-088`)**
  1. **창 모델**: 창 하나는 문서 하나, 변환기 하나 또는 시작 화면 하나만 보여 줍니다. 창이 보여 줄 것은 시작 주소의 `?module=DOC|HWP|SHEET|SLIDE|PDF|CONV|START&win=<창 이름>`이 정합니다. 메뉴 줄의 모듈 전환 탭·포맷 필터 칩·모듈 버튼은 없앴고 `mod-btn`·`tab-pill`·`tab-strip`은 더 이상 없습니다. 인수 없이 실행하면 일반 크기의 시작 창(홈 화면: 새 문서 6종·파일 열기·최근 문서 카드, PD-098로 컴팩트 창에서 바뀜)이 뜨고, 문서 창을 열면 시작 창은 스스로 닫힙니다. 마지막 창을 닫으면 앱이 종료됩니다(Tauri 기본 동작).
  2. **창 제목과 수정 표시**: `파일 이름 — EasyOffice`, 저장하지 않은 변경이 있으면 앞에 `● `를 붙입니다. 새 문서의 이름은 `제목 없음`이고, 열거나 저장한 파일의 이름이 제목이 됩니다. 제목은 페이지가 `report_window_state(name, dirty)`로 알리면 Rust가 네이티브 제목으로 설정하고, `--serve`에서는 `document.title`만 바꿉니다.
  3. **Rust 전용 열기 경로**: 창을 여는 명령은 Rust에만 있습니다. `open_document_window(kind)`는 종류만 받습니다. `open_file_in_new_window()`는 인수가 없고 Rust가 네이티브 열기 대화상자를 보여 주며, 고른 파일을 Rust가 정규 경로로 바꾸고(`canonicalize`), 확장자로 종류를 정하고, 크기(0 초과 256MiB 이하)를 확인한 뒤 읽습니다. 읽은 바이트와 표시용 이름은 **창 이름을 키로 하는 대기 큐**에 넣고 새 창을 만듭니다. 새 창은 `take_pending_open()`을 불러 `{name, base64}`를 **한 번만** 받습니다. 이 모양에는 경로가 없고(구조와 직렬화 테스트로 고정), 큐 항목은 창이 닫히면 버립니다. 끌어 놓기(`DragDrop` 창 이벤트)와 명령줄 인수도 같은 `open_path`를 거치므로 페이지는 경로를 보지 못합니다. 최근 문서는 앱 데이터 폴더의 `recent-documents.json`에 경로로 저장하고(최대 10개), 페이지에는 `{id, name, kind}`만 주며 id는 경로의 FNV-1a 해시(되돌릴 수 없음)입니다. `open_recent_document(id)`가 Rust 쪽에서 경로를 찾아 같은 `open_path`로 엽니다.
  4. **같은 파일 다시 열기**: Rust의 창 레지스트리가 정규 경로(Windows는 소문자 비교)에서 창 이름으로 가는 표를 갖고, 이미 열린 파일이면 새 창 대신 그 창을 복원·앞으로 가져옵니다(`focused`). 다른 이름으로 저장한 경로도 그 창의 것으로 옮깁니다. 이 표와 경로는 Rust 안에만 있습니다.
  5. **창 닫기**: `CloseRequested`에서 Rust가 레지스트리의 수정 상태를 보고, 저장하지 않은 변경이 있으면 닫기를 막은 채 페이지의 `__EASYOFFICE_CLOSE_REQUEST__()`를 실행합니다. 페이지의 확인 창(저장 / 저장 안 함 / 취소)에서 **저장**은 기존 저장 흐름(HWP는 `setExternalDirty`를 통한 수정 상태 포함)을 먼저 실행하고 성공했을 때만, **저장 안 함**은 바로 `close_current_window`로 창을 닫습니다. **취소**와 저장 취소·실패는 창을 그대로 둡니다. 페이지가 응답하지 않으면 두 번째 닫기 요청은 그냥 닫아, 멈춘 페이지가 창을 닫을 수 없게 만들지 않습니다. 닫는 창의 복구 기록은 함께 지웁니다.
  6. **창 단위 상태**: 실행 취소 스택, 수정 상태, 자동 저장은 창마다 따로입니다. `localStorage` 복구 기록은 `easyoffice.workspace.autosave.v1.<창 이름>`, HWP IndexedDB 기록은 `hwp-<창 이름>` 키를 씁니다(창 이름은 앱 실행마다 다른 값이라 이전 실행의 기록과 섞이지 않습니다). 쿼리 문자열이 없는 호스트(단위 테스트)는 예전처럼 하나의 문서 창으로 동작합니다.
  7. **명령과 권한**: 앱 명령은 `crates/easyoffice-desktop/src/app_commands.rs`의 한 목록(`ipc_dispatch`, `save_document_file`, `open_document_window`, `open_file_in_new_window`, `take_pending_open`, `report_window_state`, `close_current_window`, `list_recent_documents`, `open_recent_document`)입니다. 빌드 스크립트가 이 목록을 기본 거부로 선언하고, `capabilities/main-window.json`은 창 `main`과 `doc-*`에 **정확히 이 목록**만 허용합니다(Rust 단위 테스트와 위생 가드가 둘을 대조). core·shell·fs·http·dialog 권한은 여전히 없고, rhwp 편집기 iframe에는 아무 권한도 없습니다. 창 명령 시그니처에 경로가 들어가지 않는다는 것도 소스 검사 테스트로 고정했습니다.
  8. **단축키 단일 표와 전달**: `ribbon-catalog.js`의 `WORKSPACE_SHORTCUT_BINDINGS`가 유일한 표입니다. 새 항목 `Ctrl+N`(새 문서, 현재 창과 같은 종류)과 `Ctrl+W`(창 닫기)는 브라우저 예약 키이므로 `desktopOnly`로 표시해 **데스크톱 셸에서만** 묶고(`--serve`는 브라우저 기본 동작 유지), `forwardFromEditor` 표시가 있는 `Ctrl+S/O/N/K/W`만 한글 편집기가 셸에 넘깁니다. 셸이 이 목록을 iframe 주소(`?shellKeys=`)로 알리고, 편집기(`apps/hwp-studio/src/embed/shell-shortcuts.ts`)는 Ctrl/Cmd가 눌린 단일 문자 조합이 목록에 있을 때만 embed 포트의 `documentChanged` 이벤트(`reason: "shell_shortcut"`, `shortcut: "Ctrl+S"`)로 **조합 문자열만** 보냅니다. 입력 중인 글자·IME 조합·AltGr 입력·목록 밖 조합은 절대 보내지 않고, 셸은 받은 문자열을 같은 표에서 다시 확인한 뒤 키보드와 같은 동작을 실행하며 수정(편집) 이벤트로 취급하지 않습니다.
  9. **한글 편집기 화면**: HWP 활성 시 EasyOffice의 아이콘 도구 상자·서식 도구줄·리본을 숨겨 rhwp 도구모음 하나만 보이게 하고(`#app[data-hwp-embed]`), 셸 메뉴 줄에는 `파일`만 남기며 업스트림의 `파일` 메뉴는 오버레이 CSS로 숨깁니다(편집 용지는 `쪽` 메뉴에 있음). 업스트림 도구줄의 밑줄 견본 글자 `간`은 오버레이 `index.html`에서 `가`로 바꿨습니다(번역 키를 쓰지 않음). 업스트림 파일의 SHA-256 기준값은 바뀌지 않으므로 `config/rhwp-studio-overrides.json`은 이유 문구만 갱신했고 검증(`scripts/build-hwp-studio.mjs`)은 통과합니다.
  10. **아이콘 세트와 접근성(#126)**: 아이콘은 `ui/icons.js`의 24x24 선 아이콘 한 벌이며 `data-icon` 속성을 SVG 요소로 바꿔 넣습니다(`currentColor`, 문자·이모지 없음). 모든 아이콘만 있는 컨트롤은 `aria-label`을 갖고, 테스트가 모든 모듈·시작 창에서 이름 없는 컨트롤 0개를 확인합니다. 셸 색은 `shell.css`의 칩 토큰(`--eo-chrome-*`)으로 모았고 라이트·다크 모두 글자 4.5:1, 아이콘 3:1 이상임을 테스트가 계산으로 확인합니다. 상단 막대는 왼쪽 메뉴·오른쪽 소형 컨트롤(빠른 실행 3개, 명령 검색, 테마, 도구 상자)로 1024px에서도 넘치지 않고, 상태 표시줄은 말줄임표와 툴팁으로 잘림 없이 줄어들며, 명령 팔레트는 어두운 배경 위 오버레이이고 `/` 문자 대신 돋보기 아이콘이 입력란 밖에 놓입니다. "화면 전환", "결재 스탬프", "원본/대상", "비율" 라벨은 12px로 통일하고 발표자 보기의 "다음 슬라이드"는 줄바꿈하지 않습니다.
  11. **`--serve` 모사**: 개발 전용 `--serve`에서는 같은 모델을 `window.open`으로 흉내 냅니다. 새 창은 같은 주소에 `?module=…&win=…`을 붙여 열고, 파일 열기는 고른 파일의 바이트를 열어 준 창(opener)의 메모리에 두었다가 새 창이 `handoff` 번호로 한 번 가져갑니다. 서버(`http_server.rs`)는 쿼리 문자열을 무시하고 라우팅합니다.
  12. **스모크**: `EasyOffice.exe --smoke`는 아래 단계가 모두 성공 보고를 해야 종료 코드 0을 내고, 단계마다 `EASYOFFICE_SMOKE_STEP_OK <이름> <상세>`(표준 출력) 또는 `EASYOFFICE_SMOKE_STEP_FAIL <이름> <사유>`(표준 오류) 한 줄을 남깁니다. 첫 실패나 75초 초과는 즉시 종료 코드 1입니다. 단계 이름·필수 목록은 `smoke_steps.rs`, 장부와 표식은 `smoke.rs`가 가집니다. 페이지가 보고할 수 있는 이름은 `hwp`·`pdf`·`print`·`window_loaded` 넷뿐이고(그 밖의 이름은 실패), 나머지는 셸이 직접 보고합니다.
      * `ipc`: 페이지의 첫 엔진 호출(`get_workspace_bootstrap`)이 다리를 통과했습니다.
      * `hwp`: HWP 문서 창에서 편집기를 기다리고 빈 문서의 HWP 바이트를 내보낸 뒤 편집기가 그 바이트를 다시 열어 쪽 수를 확인합니다(PD-083 개정부터 포트 RPC만 사용, 입력 검증은 Chrome 하니스).
      * `pdf`: 같은 창이 쪽 SVG를 `export_searchable_pdf`로 보내면 셸이 스모크 실행에서는 대화상자도 파일 쓰기도 없이 메모리의 PDF를 `verify_exported_pdf`로 검사하고 `smoke.pdf`를 저장한 것처럼 답합니다. 검사는 `%PDF-` 머리글, `/Count`가 쪽 수와 같음, 쪽마다 `/MediaBox` 하나, 글꼴이 들어 있으면 `/ToUnicode`가 있음입니다. CI 러너에서 한국어 글꼴을 찾지 못해 글자가 윤곽선으로 나가면 `/ToUnicode`가 없으므로, 쪽 SVG에 `<text`가 있는데 글꼴이 없는 경우는 실패가 아니라 경고(`smoke: warning: …`)로 둡니다. 글꼴은 있는데 `/ToUnicode`가 없으면 실패입니다. PDF 구조를 읽는 의존성(lopdf 등)은 추가하지 않았습니다(의존하는 PDF 크레이트는 쓰기 전용 `pdf-writer`·`svg2pdf`뿐이고, 검사 대상이 우리 쓰기 코드의 출력이라 평문 키 검색으로 충분).
      * `print`: `hwp-print.js`의 `smoke()`가 쪽을 10쪽씩 받아 숨은 루트와 `@page` 규칙을 만든 뒤 창 `print()` 직전에 그 쪽 수·규칙 수를 확인하고 지웁니다. 인쇄 대화상자는 열지 않습니다.
      * `frame_ipc`: 편집기와 같은 샌드박스의 프레임(`frame-probe.html`)에서 읽기 명령을 호출해 보고, 실행되면 실패합니다(PD-083 12항). 상세는 `internals=… webview=… invoke=…`.
      * `window`: 위 단계를 마친 뒤 두 번째 문서 창을 열고, 그 창이 로드를 보고(`window_loaded`)하고 스스로 닫히는 것(`Destroyed` 이벤트)까지 확인합니다.
      * `window_state`: 기억하는 창 위치 파일(PD-099)을 이 기계의 임시 폴더에 쓰고 다시 읽어 같은지, 저장한 위치가 모니터 안에서만 재사용되는지 확인합니다.
      * `preview`: 시작 창의 미리보기 읽기(PD-106)가 rhwp 샘플 `shift-return.hwp`에서 그림을 찾습니다. 샘플 경로는 컴파일 때의 `CARGO_MANIFEST_DIR` 기준(`third_party/rhwp/rhwp-studio/public/samples/`)입니다. 스모크는 CI에서만 돌고(`ci.yml`이 `scripts/checkout-rhwp.mjs` 뒤에 같은 작업·같은 체크아웃에서 빌드하고 실행) 설치본은 `--smoke`를 쓰지 않으므로 이 경로가 유효합니다. 샘플이 없으면 `checkout-rhwp.mjs`를 안내하며 실패합니다.
      * `update`: PD-103의 루프백 훅으로 실제 업데이트 요청(앱 버전을 `User-Agent`에 넣는 `fetch_release`)을 가짜 서버에 보내, 릴리스 응답과 404 응답(릴리스 없음)을 둘 다 확인합니다.
      * `menu`: 메뉴 표(PD-102)의 모든 사용자 정의 항목 id가 설치된 메뉴 막대에 있고 쪽 동작 스크립트를 갖습니다.
- **Why** 창 단위 모델은 문서마다 undo·자동 저장·수정 상태를 격리하고(#132), 경로·파일 읽기·대화상자를 Rust에 가둬 PD-074의 fail-closed 경계를 유지하며, 한 벌의 아이콘·토큰·테스트로 셸이 크기·테마·접근성에서 일관되게 합니다.
- **Trade-offs** 창이 늘면 문서마다 WebView 하나와 (HWP는) WASM 인스턴스 하나가 생겨 메모리를 더 씁니다(`docs/rhwp-integration-design.md` §6.1의 동시 4개 상한 논의는 창당 1개로 단순해지지만 상한은 따로 두지 않았습니다). 한 창 안에서는 모듈을 바꿀 수 없어 형식 변환은 변환기 창을 따로 엽니다. `--serve` 모사는 팝업 차단과 창 간 메모리 전달 때문에 데스크톱과 100% 같지는 않습니다. 단일 인스턴스 플러그인은 아직 쓰지 않아 파일 연결로 앱이 이미 실행 중일 때 새 프로세스가 뜹니다(파일 연결 자체가 아직 없음).
- **Alternatives rejected** 한 창 안의 문서 탭 줄(조잡함, 사용자 결정으로 폐기), 창을 여는 `window:allow-create` 같은 일반 권한을 WebView에 주기(경로·URL을 JS가 다루게 됨), 열 파일 경로를 쿼리·이벤트로 새 창에 전달(경로가 WebView에 노출), 경로 대신 파일 핸들 보관(WebView2에서 쓸 수 없음).
- **Verified by** `node --test scripts/test-webui-windows.mjs scripts/test-webui-shell-chrome.mjs scripts/test-webui-hwp-shortcuts.mjs scripts/test-webui-hwp-host.mjs`와 기존 WebUI 묶음, `cargo +stable-x86_64-pc-windows-msvc test -p easyoffice-desktop`(`windows`·`recent`·`window_commands`·`smoke`·`main` 단위 테스트), `node scripts/check-repo-hygiene.mjs`, `node scripts/build-hwp-studio.mjs`, `EasyOffice.exe --smoke`(두 번째 창 왕복, 단계별 표식은 12항), 1024/1440/1920 라이트·다크 화면 확인과 직접 실행한 창의 PrintWindow 캡처.
- **Revisit when** 파일 연결(#111 후속)이나 단일 인스턴스 라우팅을 넣을 때(명령줄·두 번째 실행을 첫 프로세스의 `open_path`로 보내는 것이 자연스러운 확장), 창당 메모리가 문제가 되어 상한이 필요할 때, 편집기와 셸의 테마를 맞출 때.

## 64. 시트 셀 서식·열 너비를 XLSX/ODS에 저장하고 다시 읽기 (PD-089, #148)

- **Status** Implemented
- **Context** 시트 셀 서식(굵게·기울임·밑줄·취소선·글자 크기, `PD-084`)과 열 너비(`PD-078`)는 작업 공간 스냅샷에만 있고 저장 페이로드 `sheets[].cells`가 `{ref, raw}`만 담아 파일에 기록되지 않았습니다. 저장하고 다시 열면 서식이 사라졌습니다(#148).
- **Decision (`PD-089`)**
  1. **저장 페이로드 (`save-payload.js`)**: 셀은 선택 항목 `style`(`{bold, italic, underline, strike, sizePt}`, 켜진 값만)을, 시트는 선택 항목 `columns`(`[{index, widthPx}]`, `index`는 1부터 시작하는 열 번호)를 담습니다. 편집기의 CSS 값(`fontWeight`·`fontStyle`·`textDecoration`·`fontSize` px)은 `sheet-format.js`의 `__EASYOFFICE_SHEET_STYLE_CODEC__`가 변환합니다(1px = 0.75pt, 소수 둘째 자리). 값 없이 서식만 있는 셀도 `raw: ""`로 보냅니다. 서식이 없으면 이전과 같은 모양이라 이전 페이로드도 그대로 동작합니다.
  2. **서버 검증 (`convert_structured.rs`)**: `style`은 객체여야 하고 불리언 필드는 true/false, `sizePt`는 1~409(Excel 한도)의 유한한 수입니다. `columns`는 시트당 최대 16,384개, `index` 1~16384, `widthPx` 8~2000 정수입니다. 모든 셀·열 항목은 기존 구조화 항목 예산(500,000)에 합산되며 잘못된 값은 오류입니다. 일반 서식(켜진 것 없음)은 저장하지 않습니다. 검증된 값은 `SheetStyling`(셀 좌표별 `CellStyle`, 열별 px)으로 formats 크레이트에 전달합니다. 값 모델 `(row, col, raw)`와 기존 코덱 시그니처는 바꾸지 않았습니다.
  3. **XLSX 작성**: `xl/styles.xml`은 중복 없는 `fonts`(0번 = 기본 글꼴)와 `cellXfs`(0번 = 기본 서식, `N`번 서식이 `N`번 글꼴)이며 요소 순서는 ECMA-376 `CT_Stylesheet`(`fonts`, `fills`, `borders`, `cellStyleXfs`, `cellXfs`, `cellStyles`)을 따릅니다(Excel은 순서가 틀리면 파일을 거부). 글꼴 안은 `b, i, strike, u, sz, name` 순서입니다. 셀 `s="N"` 속성을 쓰고 값 없는 서식 셀은 `<c r=".." s=".."/>`로 기록합니다. 같은 서식 2가지를 공유하는 셀 1,000개는 `cellXfs` 3개(기본 + 2)입니다. 수식 캐시 값(`<v>`, #117)과 `fullCalcOnLoad`는 그대로입니다. 열 너비는 `<sheetData>` 앞 `<cols><col min max width customWidth="1"/></cols>`이며 같은 너비의 인접 열은 하나로 합칩니다.
  4. **px ↔ 열 너비 변환**: Excel 열 너비 단위는 기본 글꼴(Calibri 11)의 최대 숫자 너비(MDW = 7px) 개수이고 저장값은 셀 여백 5px을 포함합니다(`width = trunc(px / 7 × 256) / 256`). 따라서 읽을 때는 `px = round(width × 7)`입니다(96px ↔ `13.7109375`). 읽을 때 `customWidth`가 없는 `<col>`은 무시합니다.
  5. **ODS 작성**: `office:automatic-styles`에 서식별 `style:style`(`table-cell`, `style:text-properties`의 `fo:font-weight`·`fo:font-style`·`style:text-underline-style`·`style:text-line-through-style`·`fo:font-size`)과 너비별 열 스타일(`style:column-width`, in 단위)을 쓰고, 셀은 `table:style-name`, 열은 `table:table-column`로 연결합니다. 셀 값 표현은 그대로입니다.
  6. **읽기 (#111 디코드 경로)**: `UniversalFormatConverter::decode_rich` 결과는 바꾸지 않고, 같은 원본 바이트에서 `read_sheet_styling(bytes, format)`로 서식을 읽어 디코드 응답의 시트별 `cells[].style`과 `columns`로 돌려줍니다. XLSX는 `styles.xml`의 `fonts`·`cellXfs`와 셀 `s`를, ODS는 셀·열 자동 스타일을 읽습니다. 기본 글꼴 크기와 같은 글자 크기는 "크기 지정 없음"으로 읽고, `val="0"`·`u val="none"` 같은 Excel 표기를 해석합니다. 서식을 읽다 실패하면 값은 그대로 열고 경고만 남깁니다. 편집기 격자(`maxRows`·`maxCols`) 밖 서식과 열 너비는 가져오지 않습니다. 변환 경로(원본 파일 → 다른 형식)도 서식을 유지합니다.
  7. **열 때 반영 (`file-open.js`)**: 셀 항목에 `style`을 넣고(값 없는 서식 셀 포함) 열 너비는 `sheetController.replaceColumnWidths`로 시트별 너비에 적용합니다(36~720px로 조정). 강제 렌더(`renderWindow(true)`)는 열 머리글도 다시 그려 시트 탭 전환에서도 시트별 너비가 맞습니다. 열기 → 편집 → 저장 → 다시 열기에서 서식과 너비가 유지됩니다.
  8. **상한·안전**: 읽을 때 시트당 서식 셀·열 너비 항목은 500,000개로 제한합니다. ODS에서 행·열 전체에 반복된 서식(1,024칸 초과 블록, "시트 전체 서식")은 셀 서식으로 가져오지 않아 반복 속성이 메모리를 키우지 못합니다. XML은 기존 XXE·ZIP 폭탄 검사를 그대로 거칩니다.
- **Why** 사용자가 지정한 서식과 열 너비가 저장 파일에 남아야 한다는 기본 기대를 충족하면서, 값 모델과 코덱 시그니처를 건드리지 않는 곁가지 구조(`SheetStyling`)로 회귀 위험을 줄입니다.
- **Trade-offs** 글꼴 이름·색·채우기·테두리·정렬·숫자 서식·행 높이는 아직 기록하지 않으며(다른 앱에서 만든 파일을 열었다가 저장하면 이런 서식은 사라집니다), 글꼴은 Calibri 11 기준으로 씁니다. 열 너비 변환은 MDW 7px(Calibri 11) 기준 근사입니다. 열 너비는 작업 공간 스냅샷(실행 취소·자동 저장)에는 여전히 포함하지 않습니다. 다른 앱의 스타일 상속(부모 스타일·열 기본 서식)은 해석하지 않습니다.
- **Alternatives rejected** 셀 값 튜플에 서식 필드 추가(모든 코덱·테스트 시그니처 변경), 서식을 `raw`에 인코딩(사용자 텍스트를 마크업으로 해석하는 위험), 서식 셀마다 개별 `xf`(중복으로 `styles.xml` 비대).
- **Verified by** `cargo test -p easyoffice-formats --test sheet_style_roundtrip`(플래그·크기·중복 제거 1,000셀→`cellXfs` 3개·열 너비·요소 순서·수식 캐시·값 없는 서식 셀·타 앱 스타일 읽기·ODS 반복/단위), `cargo test -p easyoffice-app`(`convert_open`의 저장 → 디코드 → 재저장 왕복·검증 오류·격자 절단), `node --test scripts/test-webui-sheet-styles.mjs`(범위 굵게·열 너비 페이로드, 열 때 적용, 열기 → 편집 → 저장), 헤드리스 Chrome(CDP `Input.*`) 실제 UI 왕복.
- **Revisit when** 글꼴·색·채우기·테두리·정렬·숫자 서식 같은 서식을 시트 편집기에 추가할 때, 열 너비를 실행 취소/자동 저장에 포함할 때, XLS(BIFF8) 서식 기록이 필요할 때.

## 65. DOC/PPT를 실제 MS-DOC·MS-PPT 바이너리로 기록 (PD-090, #112)

- **Status** Implemented
- **Context** `.doc` 출력은 `WordDocument` 스트림 하나(64바이트 FIB 스텁 + UTF-16 본문 + EasyOffice 전용 트레일러)였고 `.ppt` 출력은 `RT_Document` 안에 슬라이드를 중첩한 전용 구조였습니다. `1Table`·조각 표(Clx)·FKP가 없고, `Current User`·persist directory·UserEditAtom도 없어 다른 워드·프레젠테이션 프로그램이 열지 못했으므로 `PD-071`에서 "호환성 제한"으로 표시했습니다(#112).
- **Decision (`PD-090`)**
  1. **템플릿 기반 작성**: 빈 문서의 FIB·스타일시트·글꼴표·문서 속성(`.doc`)과 문서 컨테이너·환경·주 마스터(`.ppt`)를 `crates/easyoffice-formats/assets/legacy/`에 바이너리 템플릿으로 두고 `include_bytes!`로 읽습니다. 원본은 Apache POI의 빈 문서 자원이며(Apache-2.0, 출처·SHA-256·NOTICE는 `THIRD_PARTY_NOTICES.md`), 요약 정보 스트림·작성자 이름·저장 경로·작성 시각을 제거했습니다.
  2. **DOC (`legacy_doc_writer.rs`)**: `WordDocument`(FIB + 512바이트 경계의 UTF-16LE 본문 + CHPX/PAPX/LVC FKP 페이지 + 구역 속성)와 `1Table`(스타일시트·글꼴·문서 속성 + 새 `Clx` + `PlcBteChpx`/`PlcBtePapx`/`PlcBteLvc` + 본문 끝으로 고친 `PlcfSed`)을 기록합니다. FIB의 `fcMin`/`fcMac`·`cbMac`·`ccpText`와 해당 `fc/lcb` 쌍을 모두 고칩니다. 본문은 문단 + 표 행(셀을 탭으로 이은 문단)이며 문단마다 PAPX 한 줄(29줄/페이지)을 둡니다. 문단 안 줄바꿈은 0x0B, 나머지 제어 문자는 공백입니다.
  3. **PPT (`legacy_ppt_writer.rs`)**: 문서 컨테이너에 슬라이드 목록(`SlideListWithText` 인스턴스 0)을 더하고, 슬라이드마다 `SlideContainer`(`SlideAtom` → 주 마스터, `PPDrawing`의 제목·본문 자리표시자 도형과 그 안의 텍스트, 마스터 색 구성표)를 쓰며, 그룹 도형·그림 그룹(드로잉 클러스터)을 슬라이드 수에 맞게 다시 계산합니다. 스트림 끝에 `PersistDirectoryAtom`과 `UserEditAtom`을 쓰고 `Current User`(`offsetToCurrentEdit`)가 이를 가리킵니다. 스트림은 단일 편집으로 자기 완결이라 `offsetLastEdit`은 0입니다.
  4. **왕복 보존**: EasyOffice 전용 데이터는 규격 구조가 참조하지 않는 곳에 둡니다. `.doc`은 `WordDocument` 끝의 `[트레일러][시작 오프셋][매직]`(제목·문단·표 원문), `.ppt`는 persist 디렉터리가 가리키지 않는 최상위 레코드(형식 `0x7EA0`, 제목·노트·테마·전환이 든 이전 중첩 구조)입니다. 읽기는 이를 먼저 찾아 정확히 복원하고, 없으면(다른 프로그램이 저장한 파일) 조각 표(`Clx`) 경로와 기존 레코드 해석을 그대로 씁니다. 이전 형식(`fcMac` 직후 트레일러, 중첩 `RT_Document`)도 계속 읽습니다.
  5. **호환성 표시 제거**: `DocumentFormat::output_compatibility()`는 모든 형식에 대해 `Standard`를, `output_specification()`은 표준 규격 이름을 반환합니다. 응답 필드(`compatibility`, `compatibilityWarning`)와 UI 표시 경로는 다른 작성기가 다시 제한될 때를 위해 남깁니다.
- **Why** 사용자가 `.doc`/`.ppt`로 저장한 파일이 워드·프레젠테이션 프로그램에서 열리고 제목·본문·글머리표가 보여야 한다는 기본 기대를 충족하기 위함입니다. 규격 구조 전체를 새로 쓰는 대신 검증된 빈 문서를 기반으로 바뀌는 부분만 만들어 오류 면적을 줄입니다.
- **Trade-offs**
  1. 서식(굵게·글꼴·색)·진짜 Word 표·그림은 기록하지 않습니다. 표는 탭 문단입니다. 모든 글자는 템플릿 기본 서식(Times New Roman 계열)이며 한글은 프로그램의 글꼴 대체에 따릅니다.
  2. 슬라이드 노트·테마·전환·문서 제목은 아직 규격 구조(노트 페이지, 슬라이드 쇼 정보)로 쓰지 않고 전용 레코드에만 남깁니다. 다른 프로그램에서는 보이지 않으며 EasyOffice가 다시 열 때만 복원됩니다.
  3. 슬라이드는 제목 + 본문(글머리표 1단계)이며 레이아웃은 "제목 및 내용"/"제목만" 두 가지입니다.
  4. 루트 스토리지 CLSID와 `\x01CompObj`, 요약 정보 스트림은 쓰지 않습니다(필수 아님).
  5. 전용 데이터가 파일 크기를 조금 늘립니다. 다른 프로그램이 저장하면 이 데이터는 사라지고 보이는 텍스트만 남습니다.
- **Alternatives rejected** 요약 정보 스트림까지 템플릿 그대로 포함(작성자 이름·경로·시각이 새므로 기각), 이전 구조 유지 + 안내 문구(#112 요구와 반대), 스트림을 처음부터 손으로 생성(스타일시트·마스터는 규모가 커 검증 부담이 큼).
- **Verified by** `cargo test -p easyoffice-formats --test legacy_writers`(FIB·Clx·FKP 512 경계·`PlcfSed`, Current User → UserEditAtom → PersistDirectory → SlideContainer, 한글·빈 문서·슬라이드 0장·다수 왕복, Clx 단독 복원, 이전 중첩 형식), `cargo test -p easyoffice-formats --test output_container_spec_validity`, `cargo test -p easyoffice-app`, 외부 검증: `soffice --headless --convert-to txt` / `pdf`로 문단·슬라이드 제목·글머리표 확인.
- **Revisit when** 서식·표·그림·노트 페이지·전환을 규격 구조로 기록할 때, 요약 정보(작성자·제목) 기록이 필요할 때, 워드·프레젠테이션 프로그램에서 복구 경고가 보고될 때.

## 66. CI 작업 분리와 Rust 캐시: 공통 게이트는 Linux에서 한 번, OS별 작업은 해당 OS에서만 (PD-091)

- **Status** Implemented
- **Superseded by PD-094 (부분)**: 이 항목 중 Swift 앱·`swift build`/`swift test`·`scripts/package-macos-release.sh`·Swift 대응 규칙에 해당하는 부분은 PD-094로 대체되었습니다. Rust 엔진·WebUI·Windows 관련 결정은 그대로 유효합니다.
- **Context** `ci.yml`의 macOS(`verify`)와 Windows(`verify-windows`) 작업이 위생 가드·rhwp 검증·WebUI 테스트·`cargo fmt`·엔진 clippy를 똑같이 두 번 실행했고, Rust 빌드 캐시가 없어 PR마다 의존성을 처음부터 컴파일했습니다(Windows 약 13분). 저장소는 공개라 표준 러너 비용은 없고, 병목은 PR당 대기 시간입니다.
- **Decision (`PD-091`)**
  1. 새 `checks` 작업(`ubuntu-latest`)이 OS와 무관한 게이트를 한 번만 실행합니다: 위생 가드, rhwp 핀·WASM·글꼴 검증, rhwp/hwp-studio 가드 테스트, Info.plist 범위, WebUI 회귀 묶음, `cargo fmt --check`, 엔진 clippy `-D warnings`, `cargo test --workspace --exclude easyoffice-desktop`, `--self-test`.
  2. `verify`(macOS)는 Swift 빌드·테스트와 `.app` 번들 검증만, `verify-windows`는 엔진 테스트·자체 시험(Windows 경로 동작), `embed-hwp-studio`, `--serve` 스모크, 데스크톱 clippy·테스트, NSIS 빌드·스모크·하네스만 실행합니다. 작업 이름은 그대로 두어 기존 필수 검사 설정이 깨지지 않습니다.
  3. 세 작업 모두 `Swatinem/rust-cache`(v2.9.3, 공급망 보호를 위해 커밋 SHA로 고정). GitHub 기본 액션(`checkout` v5.1.0, `setup-node` v5.0.0, `upload-artifact` v6.0.0, `download-artifact` v7.0.0)도 Node 24 런타임 버전을 커밋 SHA와 버전 주석으로 고정합니다(#171)로 `~/.cargo`와 `target`의 의존성 산출물을 캐시합니다(키: 작업·rustc·Cargo.lock).
- **Why** 같은 검사를 두 OS에서 반복할 근거가 없고(순수 JS·서식 검사), 캐시는 의존성 재컴파일 시간을 없애 PR 대기 시간을 가장 크게 줄입니다.
- **Trade-offs** 엔진 테스트는 macOS에서 더 이상 돌지 않습니다(Linux·Windows에서 실행, macOS 전용 엔진 코드 없음). `checks`를 필수 검사로 쓰려면 저장소 설정에서 추가해야 합니다. 캐시가 오래되면 rustc 버전·잠금 파일 변경 시 자동으로 새 키를 씁니다.
- **Alternatives rejected** 문서만 바뀐 PR의 작업 건너뛰기(경로 필터는 필수 검사가 보고되지 않아 PR이 대기 상태로 남으므로 대체 작업이 필요, 개인 프로젝트 규모에서 이득 대비 복잡), 모든 검사를 Linux 하나로 통합(Swift 앱·Tauri MSVC 빌드는 해당 OS 필요).
- **Verified by** `.github/workflows/ci.yml`의 세 작업 결과, `node scripts/check-repo-hygiene.mjs`. 위생 가드는 릴리스와 같은 CRLF 체크아웃에서도 돌도록 Windows 작업에서 한 번 더 실행하고, 가드의 줄 단위 개수 검사는 원본을 LF로 정규화해 읽습니다(v2.0.0 릴리스 실패 후 보완).
- **Revisit when** 문서 전용 PR이 잦아질 때, 캐시 적중률이 낮을 때.

## 67. 예약 글꼴 이름(RFN)이 있는 번들 글꼴은 공식 배포 파일 또는 그 WOFF2 압축본만 사용 (PD-092, #144)

- **Status** Implemented
- **Context** #129에서 rhwp가 웹용으로 변환한 OFL 글꼴 사본을 그대로 번들했습니다. 공식 TTF와 표(table) 단위로 비교하니 나눔고딕·나눔명조·나눔고딕코딩·Spoqa Han Sans 사본은 글리프 일부를 뺀 서브셋(나눔고딕 약 400개, 나눔고딕코딩 약 7,000개 적음)이었고 D2Coding 사본은 `glyf`가 달랐습니다. OFL 3조는 예약 이름(RFN)이 있는 글꼴의 수정본(Modified Version)에 원래 이름을 쓰지 못하게 하고, OFL FAQ 2.2·2.2.1은 형식 변환을 원칙적으로 수정으로 보되 "원래 글꼴 데이터가 WOFF 압축 외에는 바뀌지 않은" WOFF/WOFF2만 예외로 둡니다.
- **Decision (`PD-092`)**
  1. 라이선스에 `with Reserved Font Name …`이 있는 글꼴은 공식 배포 파일을 그대로(`conversion: none`) 또는 공식 TTF의 WOFF2 압축본(`conversion: woff2-compression`)만 번들합니다. 나눔 3종은 google/fonts의 TTF, D2Coding은 공식 1.3.2 릴리스 zip의 TTF를 WOFF2로 압축하고, Pretendard와 Spoqa Han Sans는 공식 woff2를 그대로 씁니다.
  2. WOFF2 압축본은 복원한 글꼴 표가 공식 TTF와 바이트 단위로 같아야 합니다(`head`의 flags·checkSumAdjustment와 WOFF2가 요구하는 `DSIG` 제거만 예외). 기본 `glyf` 변환이 바이트를 바꾸는 나눔고딕코딩·D2Coding은 WOFF2 null 변환을 씁니다.
  3. `fonts.json` 항목은 `upstream`(공식 https URL, 공식 파일 SHA-256, `conversion`, zip이면 `member`·`archiveSha256`)을 기록하고, `scripts/verify-fonts.mjs`가 RFN 라이선스 글꼴에 이를 요구합니다(`none`이면 번들 해시가 공식 해시와 같아야 함). 예약 이름이 이 글꼴과 무관한 경우(Noto Sans KR의 'Source')는 `reservedNameNotUsed`에 사유를 적습니다.
  4. 파일 이름은 그대로 두어 rhwp 글꼴 카탈로그·오버레이는 바뀌지 않습니다.
- **Why** 공개 배포 전에 RFN 위반 가능성을 없애고, 출처·해시를 기계적으로 검사해 다음 갱신 때도 같은 실수가 반복되지 않게 합니다.
- **Trade-offs** 서브셋이 아닌 전체 글꼴이라 번들 글꼴이 18.38 MiB에서 19.44 MiB로 약 1.06 MiB 커집니다(글꼴은 20 MiB 번들 예산과 별도 집계). WOFF2 압축 동일성은 fontTools로 생성 시점에 확인했고 CI는 해시만 검사합니다.
- **Alternatives rejected** 이름을 바꾼 수정본 사용(문서에 지정된 글꼴 이름과 맞지 않아 카탈로그 대체가 깨짐), 공식 TTF 그대로 번들(크기 약 4배), 해당 글꼴 제외(한글 문서 표시 품질 저하).
- **Verified by** `node scripts/verify-fonts.mjs`, `node --test scripts/test-hwp-studio.mjs`(RFN 출처 검사 거부·허용 경우), 생성 시 fontTools 표 비교.
- **Revisit when** 글꼴을 추가·갱신할 때, 공식 배포처가 woff2를 직접 제공할 때.

## 68. 한글 창 크롬 정리·상태 표시줄·시작 아이콘과 데스크톱 파일 수명주기: 단일 인스턴스·제자리 저장·파일 연결 (PD-093, #158, #133)

- **Status** Implemented
- **Context** 한글 창에는 셸 메뉴 줄과 rhwp 메뉴 줄, 셸 상태 표시줄과 편집기 상태 줄, 셸 테마와 편집기 테마가 겹쳐 있었고, 상태 표시줄에는 진단 항목(`모듈: …`, `포맷: …`, `IME…`, `엔진: 로컬 서버…`)이 섞여 1024px에서 잘렸습니다. 시작 창의 문서·한글·PDF 카드는 같은 아이콘이었습니다. 또 데스크톱은 실행할 때마다 새 프로세스가 뜨고, 저장은 매번 대화상자를 열었으며, 더블클릭으로 파일을 열 수 없었습니다(PD-088 Revisit 항목, #111 후속).
- **Decision (`PD-093`)**
  1. **한글 창의 메뉴 줄 하나**: 한글 편집기가 창을 차지하는 동안(`#app[data-hwp-embed="true"]`) 셸 메뉴 줄(`#menu-bar`)을 숨깁니다. 편집기의 `파일` 메뉴는 오버레이(`apps/hwp-studio/index.html`)에서 셸 명령 여섯 개(새로 만들기, 열기, 최근 문서, 저장, 다른 이름으로 저장(HWP·HWPX), 닫기)로 바꿉니다. 항목은 `data-shell-cmd`만 갖고(`data-cmd` 없음) `src/embed/shell-menu.ts`가 기존 임베드 포트로 `{reason:"shell_command", command, id?, format?}` 이벤트를 보냅니다. 셸의 `hwp-host.js`는 이를 `new|open|recent|save|save-as|close` 허용 목록과 인수 형식(최근 문서 id는 `[A-Za-z0-9_-]{1,64}`, 저장 형식은 `hwp|hwpx`)으로 다시 검사한 뒤에만 `shell-chrome.js`의 동작을 실행합니다. 편집기 쪽 목록과 셸 쪽 목록은 테스트가 같은지 확인합니다. 이전의 "업스트림 파일 메뉴 숨김" 오버레이 CSS는 없앴습니다.
  2. **한 테마**: 셸 테마(밝게·어둡게·시스템)를 편집기에 넘깁니다. 첫 페인트는 프레임 주소의 `theme=`(시스템이면 생략), 이후 변경은 새 임베드 RPC `setShellContext({theme, recents})`(capability `shell-context-v1`)로 즉시 반영합니다. 셸은 처음엔 `system`(운영체제 설정을 따라 편집기도 스스로 따름)이고 사용자가 테마 버튼을 누르면 선택한 `light|dark`로 바뀝니다. 편집기는 받은 값을 허용 목록(`light|dark|system`)으로 검사합니다.
  3. **한 상태 표시줄**: 한글 창에서는 셸 상태 표시줄도 숨기고(`#status-bar`), 셸 알림(`#status-command-feedback`에 쓰이는 모든 문구)은 `#shell-toast`(`role="status"`, 4.5초, 오류성 문구 8초)에도 표시합니다. 같은 문구는 다른 창의 상태 줄에도 그대로 남습니다.
  4. **모든 창의 상태 표시줄**: 쪽·위치·입력 모드·파일 이름·일시 메시지·확대만 남기고(`선택 집계`는 시트 전용), 진단 항목(플랫폼, 모듈, 포맷, IME, 엔진)은 새 `도움말 ▸ 제품 정보…` 대화상자(`#about-dialog`)로 옮겼습니다. 위치·입력 모드·확대는 줄어들지 않고(`flex-shrink:0`), 파일 이름과 메시지가 먼저 줄임표로 양보합니다. 데스크톱에서는 `로컬 서버` 문구가 상태 표시줄에 나오지 않습니다(`--serve` 개발 모드에서만 제품 정보에 나옴).
  5. **시작 창 아이콘**: 문서(DOCX)·한글·스프레드시트·프레젠테이션·PDF·변환기가 각자 `icons.js`의 SVG(`file-doc|file-hwp|file-sheet|file-slide|file-pdf|file-convert`)를 쓰고, 색은 단일 강조 토큰입니다(모듈별 색은 PD-098로 폐기, 버튼은 홈 화면의 `home-action`).
  6. **단일 인스턴스(#133)**: `tauri-plugin-single-instance`(Rust 전용, WebView 권한 없음)를 첫 플러그인으로 등록합니다(`--smoke`에서는 등록하지 않아 실행 중인 인스턴스와 겹치지 않음). 첫 실행 인수와 두 번째 실행 인수는 같은 경로(`launch_files` → `route_launch_files` → `open_path`, 시작 창이 쓰는 열기 큐)로 들어가 새 창으로 열리거나, 이미 열린 파일이면 그 창을 앞으로 가져옵니다. 인수에 파일이 없으면 기존 창을 앞으로 가져옵니다. 인수 해석은 지원 확장자의 실제 파일만 통과시키고, WebView는 경로를 받지 않습니다.
  7. **제자리 저장(#133)**: 창 이름 ↔ 경로·지문(크기 + 수정 시각, PD-109부터 내용 해시 포함) 표(`tracked_files.rs`)는 Rust에만 있고, 파일을 열 때와 쓸 때 갱신됩니다. 앱 명령은 늘리지 않고 `save_document_file`에 `inPlace` 인수(불리언)를 더했습니다(허용 목록 9종 그대로, 경로는 여전히 인수·반환 어디에도 없음). `inPlace`이고 이 창이 같은 확장자의 파일을 추적하면, 같은 폴더의 임시 파일(PD-109부터 저장마다 고유한 이름)에 쓰고 `sync_all` 뒤 이름 바꾸기로 원본을 교체합니다(기존 `write_document_bytes`). 쓰기 전에 지문이 열 때와 다르면(`Changed`) 네이티브 확인 창(덮어쓰기/취소)을 띄우고, 취소하면 파일을 건드리지 않고 `cancelled`를 돌려줍니다. 원본이 사라졌으면(`Missing`) 묻지 않고 다시 만듭니다. 추적 파일이 없거나 확장자가 다르면(예: HWPX로 연 문서를 HWP로 저장) 기존 대화상자 경로를 그대로 탑니다. 대화상자 저장을 포함한 모든 저장 결과(`{status, name, bytes}`)에는 디렉터리 없이 파일 이름만 담고, 대화상자로 고른 경로는 Rust의 추적 파일 표에만 남겨 같은 창의 다음 저장이 제자리 저장으로 이어집니다(#166). 저장·내보내기 중 변환기(CONV) 창은 원본을 덮어쓰지 않도록 항상 대화상자를 씁니다. "다른 이름으로 저장"(HWP·HWPX)은 항상 대화상자입니다.
  8. **파일 연결(#133)**: NSIS `bundle.fileAssociations`로 HWP, HWPX, DOCX, ODT, XLSX, ODS, PPTX, ODP, PDF를 등록합니다(PDF는 `Viewer`, 나머지는 `Editor`). TXT·MD·CSV·TSV·RTF와 구형 `.doc`·`.xls`·`.ppt`는 다른 프로그램의 기본값을 빼앗지 않도록 등록하지 않습니다(앱 안에서는 계속 열림). 위생 가드의 "파일 연결 금지" 조건은 "창 모델이 여는 확장자만 등록"으로 바꿨습니다.
  9. **선택 항목**: 네이티브 rhwp 어댑터(작업 6b)는 건너뜁니다.
- **Why** 한 창에서 메뉴·테마·상태 표시줄이 하나이면 화면이 덜 어수선하고, 진단 정보는 필요할 때만 제품 정보에서 봅니다. 열기와 저장이 파일 관리자와 같은 방식(더블클릭, 다시 실행해도 한 프로세스, 저장하면 그 파일에 저장)으로 동작해야 데스크톱 앱으로 쓸 수 있습니다. 경로와 파일 입출력을 Rust에만 두는 PD-088의 경계를 유지하면서 구현합니다.
- **Trade-offs**
  1. 한글 창에서 셸 메뉴 줄이 숨겨지므로 셸 전용 컨트롤(명령 팔레트 버튼, 테마 버튼, 도움말 메뉴)이 눈에 보이지 않습니다. 단축키(`Ctrl+K`, `Ctrl+Alt+D` 등 편집기가 전달하는 조합)와 `Ctrl+S/O/N/W`는 그대로 동작하고, 제품 정보는 다른 창에서 봅니다.
  2. 편집기의 최근 문서 목록은 창을 열 때 한 번 채워집니다(이후 같은 창에서 연 문서는 다음 창부터 반영). 편집기의 PDF 저장·HTML 내보내기·인쇄 같은 파일 메뉴 항목은 이전에도 파일 메뉴 전체가 숨겨져 있었으므로 새로 사라지는 기능은 없습니다.
  3. 지문은 크기와 수정 시각이라 같은 크기로 같은 시각에 바뀐 파일은 놓칩니다(해시는 큰 파일을 열 때마다 읽는 비용 때문에 쓰지 않음). 확인 창은 마지막 저장 직전에만 뜨며, 저장 사이의 감시는 하지 않습니다.
  4. 제자리 저장은 같은 확장자에서만 동작해 `.hwpx`로 연 문서를 저장하면(편집기는 HWP를 내보냄) 대화상자가 뜹니다.
  5. 단일 인스턴스는 `--smoke`에서 꺼져 있어 Windows CI 스모크는 이를 검증하지 못합니다(단위 테스트는 인수 해석과 큐 경로를 확인).
  6. 위의 확장자 선택은 설치 때 Windows가 사용자의 기존 기본 앱을 바꾸지 않는 범위에서의 보수적 목록입니다.
- **Alternatives rejected** 편집기 파일 메뉴를 그대로 두고 셸 메뉴 줄만 숨기기(열기·저장이 편집기 내부 경로로 가서 창 모델·경로 경계를 우회), 편집기 메뉴를 셸이 DOM으로 그리기(프레임 격리를 깨뜨림), 별도 앱 명령 `save_document_in_place` 추가(권한 표면이 늘고 대화상자 경로와 중복), 해시 기반 변경 감지(큰 파일 읽기 비용), 모든 지원 확장자 연결(기본 앱 탈취), 감시자(`notify`)로 외부 변경을 실시간 감지(의존성·복잡도 대비 이득이 작음).
- **Verified by** `node --test scripts/test-webui-hwp-shell.mjs`(허용 목록·인수 검사·편집기/셸 목록 일치, 테마 전달과 `theme=` 주소, 숨김 규칙, 토스트, 상태 표시줄 항목, 시작 아이콘), `scripts/test-webui-desktop-bridge.mjs`(`inPlace` 인수·`로컬 서버` 부재), `cargo test -p easyoffice-desktop`(`window_commands`의 두 번째 실행 인수 → 열기 큐, `save_file`의 임시 파일 + 이름 바꾸기·외부 변경 확인·삭제된 원본, `tracked_files`의 지문, 권한 목록 1:1), `node scripts/check-repo-hygiene.mjs`(연결 확장자 검사). NSIS 설치 파일의 연결 등록, 실제 두 번째 실행, 네이티브 확인 창은 Windows CI/수동 확인 대상입니다.
- **Revisit when** 외부 변경을 저장 전 외에도 알려야 할 때(감시자), HWPX로 연 문서를 HWPX로 제자리 저장하게 할 때, 한글 창에서도 팔레트·테마 컨트롤이 필요해질 때, macOS 번들(#136)이 같은 창 모델을 쓸 때, 연결 확장자를 넓힐 때.

## 69. macOS도 Tauri 데스크톱 셸로 배포: Swift 앱 삭제·서명 없는 .dmg·파일 연결 계약 (PD-094, #136)

- **Status** Implemented
- **Context** 저장소에는 두 개의 데스크톱 앱이 있었습니다. Windows는 Rust 엔진 + WebUI를 Tauri 셸(PD-074)로 배포하고, macOS는 같은 기능을 Swift/SwiftUI로 따로 구현했습니다. 이후 기능(한글 편집기, 문서별 창, 제자리 저장, 파일 연결)은 모두 WebUI와 Rust 쪽에만 생겼고, Swift 앱은 이를 따라가지 못한 채 CI 시간과 검증 규칙(Rust↔Swift 동등성)만 계속 소모했습니다.
- **Decision (`PD-094`)**
  1. **하나의 코드베이스**: macOS 앱은 `crates/easyoffice-desktop`(Tauri v2, WKWebView)로 빌드합니다. Swift 앱(`Sources/`, `Tests/`, `Package.swift`), `scripts/package-macos-release.sh`, `scripts/verify-macos-plist.mjs`, 위생 가드의 Swift 규칙과 Rust↔Swift 동등성 검사(함수 목록·포맷 수 대조)를 같은 변경에서 삭제합니다. 옛 코드는 Git 이력에 남습니다. Swift 구현에 관한 이전 항목은 Superseded로 표시하고 본문은 보존합니다.
  2. **번들**: `bundle.targets`는 `nsis`, `app`, `dmg`. macOS는 `minimumSystemVersion` 11.0, 카테고리 `Productivity`, 아이콘 `icons/icon.icns`(생성기 `scripts/generate-desktop-icons.mjs`가 PNG·ICO와 함께 결정론적으로 만듭니다).
  3. **파일 연결**: PD-093의 `bundle.fileAssociations`가 macOS에서는 `CFBundleDocumentTypes`가 됩니다(HWP, HWPX, DOCX, ODT, XLSX, ODS, PPTX, ODP는 `Editor`, PDF는 `Viewer`). 모든 항목은 `rank: Alternate`로 사용자의 기존 기본 앱을 가로채지 않고 "다음으로 열기"에 나타납니다. 이 계약(필수 확장자, 역할, 순위, 최소 시스템 버전, 서명 정보 미포함, 아이콘)은 `scripts/desktop-file-associations.mjs`가 `tauri.conf.json`에서 정적으로 검사하고 위생 가드와 `scripts/test-desktop-file-associations.mjs`가 실행합니다. macOS에서는 Finder가 파일을 명령줄 인수가 아니라 Apple Event(`RunEvent::Opened`)로 전달하므로 셸이 이를 같은 열기 큐로 보내고, 단일 인스턴스 플러그인은 그대로 둡니다.
  4. **메뉴·종료**: macOS는 Tauri 기본 메뉴(앱·파일·편집·보기·창)를 사용해 ⌘C/⌘V/⌘X/⌘A/⌘Z/⌘W/⌘Q/⌘M이 동작합니다. WebUI는 이미 `metaKey`를 `Ctrl`로 바꿔 단축키 표를 공유합니다(`scripts/test-webui-windows.mjs`가 확인). ⌘Q(사용자 요청 종료)는 프로세스를 바로 끝내지 않고 각 창을 닫는 흐름(저장 / 저장 안 함 / 취소)으로 보냅니다.
  5. **서명 없는 배포**: 저장소에 서명 ID·공증 정보를 두지 않습니다. Apple silicon은 서명이 전혀 없는 바이너리를 실행하지 않으므로 CI와 릴리스 단계는 `APPLE_SIGNING_IDENTITY`를 `secrets.APPLE_SIGNING_IDENTITY || '-'`로 넘겨 ad-hoc 서명만 합니다(신뢰되는 서명이 아니므로 Gatekeeper는 처음 한 번 확인을 요구합니다). 나중에 인증서를 쓰려면 이 시크릿과 공증 변수(`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`)만 추가합니다. 처음 여는 방법(우클릭 → 열기, 시스템 설정 → 개인정보 보호 및 보안 → 그래도 열기, 최후 수단 `xattr -dr com.apple.quarantine`)은 README에 적습니다.
  6. **CI·릴리스**: `ci.yml`의 `verify`(macos-15)는 편집기 번들 빌드, 데스크톱 clippy·테스트, `.app`·`.dmg` 빌드와 크기·Info.plist 검사, `--smoke` 실행을 합니다. `release.yml`은 `aarch64-apple-darwin`과 `x86_64-apple-darwin` 대상을 추가해 `--target universal-apple-darwin`으로 유니버설 `.dmg`를 만들고, 자산 이름 `EasyOffice-<버전>-macos-universal.dmg`와 `SHA256SUMS-macos-universal.txt`를 Windows 자산과 같은 규칙으로 게시합니다(`.zip`은 더 이상 만들지 않습니다).
- **Why** 같은 기능을 두 번 구현하고 맞추는 비용을 없애고, macOS 사용자도 한글 편집기·문서별 창·제자리 저장·파일 연결을 즉시 얻습니다. 서명 없는 배포는 개인 프로젝트에서 유료 개발자 계정 없이 릴리스를 계속하기 위한 선택이며, 나중에 서명을 붙일 자리는 환경 변수로 남겨 둡니다.
- **Trade-offs**
  1. 네이티브 SwiftUI/AppKit 대비 콜드 스타트와 메모리가 WebView 수준입니다.
  2. 서명·공증이 없어 처음 실행할 때 Gatekeeper 경고를 거쳐야 합니다. 다운로드한 앱의 격리 속성은 사용자가 한 번 해제해야 합니다.
  3. Swift 앱에만 있던 기능(PDFKit 렌더링, 네이티브 인스펙터)은 사라지며 WebUI 기능 범위로 통일됩니다.
  4. macOS 번들 동작(Apple Event 열기, ⌘Q 흐름, WKWebView에서의 편집기 구동)은 이 변경을 만든 Linux 환경에서 실행할 수 없고 macOS CI의 `--smoke`와 번들 검사가 일부만 확인합니다.
- **Alternatives rejected** Swift 앱 유지 + Tauri 병행(같은 기능을 두 곳에서 계속 맞춰야 하고 이미 어긋나 있음), 앱을 `.zip`으로도 배포(`.dmg` 하나로 충분하고 자산 목록이 Windows와 달라짐), 파일 연결을 `Default` 순위로 등록(사용자의 기존 기본 앱을 바꿈), 저장소에 `signingIdentity: "-"` 기록(나중에 실제 인증서로 바꿀 때 환경 변수보다 덜 유연하고 계약 검사가 "서명 정보 없음"을 보장할 수 없음).
- **Verified by** `node --test scripts/test-desktop-file-associations.mjs scripts/test-webui-windows.mjs`, `cargo test -p easyoffice-desktop`(번들 설정·Finder URL 변환), `node scripts/check-repo-hygiene.mjs`, `node scripts/generate-desktop-icons.mjs --check`, macOS CI(`ci.yml` `verify`).
- **Revisit when** 유료 개발자 계정을 확보해 서명·공증을 켤 때, `--smoke`가 macOS 러너에서 불안정할 때(저장소 변수 `MACOS_SMOKE=off`로 건너뛸 수 있음), 사용자가 macOS 전용 네이티브 기능을 다시 요구할 때, Mac App Store 배포를 검토할 때.

## 70. 오픈소스 구현 이식으로 엔진 성능 개선: 무의존 유지 + 허용 라이선스 소스 이식 (PD-095)

- **Status** Implemented (base64, DEFLATE 압축기)
- **Context** 엔진 크레이트는 외부 크레이트 의존성 0(PD-053)입니다. 같은 일을 하는 검증된 오픈소스 Rust 구현과 핫패스를 같은 데이터(1MB DOCX, 10만 셀 XLSX, 1MiB 바이트)로 비교한 결과, base64 인코드·디코드가 3~7배, DEFLATE 압축이 약 6배 느렸고 CSV·XML 이스케이프·UTF-16 디코드는 이미 더 빨랐습니다.
- **Decision (`PD-095`)**
  1. **무의존 유지**: 엔진 크레이트(`easyoffice-formats`·`sheet`·`layout`·`app`)에 크레이트 의존성을 추가하지 않습니다. 더 빠른 구현은 소스(알고리즘·블록 구조)를 안전한 Rust로 엔진 안에 이식합니다.
  2. **허용 라이선스만**: MIT·Apache-2.0·Zlib·BSD 계열이며 `unsafe`를 쓰지 않는(또는 쓰지 않게 옮길 수 있는) 구현만 이식합니다. GPL·LGPL·MPL 코드는 복사하지 않습니다.
  3. **고지**: 이식한 파일 머리에 출처·버전·라이선스를 적고 `THIRD_PARTY_NOTICES.md`의 "Engine code adapted from open-source Rust crates"에 라이선스 전문을 남깁니다.
  4. **근거**: 측정으로 개선이 확인된 것만 이식하고, 기존 동작과의 동등성은 이전 구현을 기준 함수로 둔 테스트로 고정합니다. 핫패스는 release `--self-test` 전후 비교를 PR에 남깁니다.
  5. **base64**(`crates/easyoffice-formats/src/base64_codec.rs`): `base64` 크레이트 `GeneralPurpose` 엔진의 블록 구조(8바이트 적재로 6바이트→8기호, 256칸 디코드 표와 8기호 블록, 공백이 없으면 한 번에 디코드하고 실패했을 때만 공백 제거 후 재시도)를 이식했습니다. IPC 파일 열기·저장과 PDF 서명 경로가 쓰던 세 구현(앱 인코더, PDF 디코더, 위임 함수)을 하나로 합쳤습니다. 1MiB 기준 인코드 1.77→0.6ms, 디코드 4.4→0.56ms. 오류 판정(빈 입력, 4의 배수가 아닌 길이, 알파벳 밖 기호, 마지막 4기호 밖의 `=`)은 이전과 같습니다.
  6. **DEFLATE 압축기**(`crates/easyoffice-formats/src/deflate/compress.rs`): miniz_oxide 0.8.9(MIT OR Zlib OR Apache-2.0)의 빠른 압축 레벨 설계(한 번의 패스로 해시 체인 LZ77, 블록마다 빈도 수집 후 길이 제한 허프만 코드 생성, 저장·고정·동적 중 비트 비용이 가장 작은 블록 형식 선택)를 안전한 Rust로 다시 구현해 고정 허프만 압축기를 교체했습니다(`unsafe` 없음, 크레이트 의존성 없음). 레벨 1~4 중 기본은 레벨 2(체인 8단계, 지연 매칭 없음)이고 `deflate_raw_level`로 선택할 수 있습니다. 긴 매치는 양 끝 8위치만 해시에 넣고, 해시·체인·토큰 버퍼는 스레드별로 재사용합니다. 출력 바이트가 바뀌므로 자체 점검 deflate 벤치는 기준(이전 고정 허프만) 구현과의 바이트 동일성 대신 양쪽 디코드 동일성과 크기 비교로 검증하고, ZIP 작성기는 압축률 90배를 넘는 항목을 무압축으로 보관합니다. 측정(release `--self-test` 5회 중앙값, 전→후): 1MB DOCX→DOCX 36.1→24.8ms, DOCX→HWPX 41.8→27.7ms, 10만 셀 XLSX 패킹·해제 157→118ms, 압축 결과 크기 DOCX 184,948→182,640B, XLSX 746,192→511,060B; 32KB 반복 XML 왕복 94→70us. 압축 단독(1.48MB 문서 XML) 21.4→10.4ms, 크기 183,696→181,470B.
  7. **DEFLATE 해제**(`crates/easyoffice-formats/src/deflate.rs`): miniz_oxide 0.8.9의 고속 해제 설계를 안전한 Rust로 이식했습니다(`unsafe` 없음, 의존성 없음). 블록마다 최대 32K칸 단일 표를 다시 만들던 허프만 표를 9비트 1차 표 + 접두어별 2차 표로 바꿨고, 입력이 8바이트 이상 남은 동안은 한 번의 64비트 보충으로 기호를 여러 개 해독하는 빠른 루프를 돌립니다. 출력은 평평한 버퍼에 쓰고 일치 복사는 8바이트 덩어리(거리 1은 채우기, 32바이트 초과 비중첩은 `copy_within`)로 처리하며 버퍼는 예산 상한까지만 `checked_*`로 늘립니다. 입력 꼬리는 기존의 전부 검사하는 해독기가 처리하므로 예산 초과·거리 오류·절단·불완전 표 판정과 오류 종류는 이전과 같습니다. 이전 구현을 테스트 기준 함수(`deflate/reference_inflate.rs`)로 보존해 비트 뒤집기·절단 변이 수백 건에서 성공/실패와 결과 바이트가 같음을 고정했고, 레벨 1~4 왕복도 확인합니다. 측정(해제 MiB/s, 전→후): 문서 XML(자체 레벨2 스트림) 464→575, 시트 XML 509→591, 문서 XML(miniz 레벨1 스트림) 418→464, 시트 XML(miniz 레벨6 스트림) 567→666, 문서 XML(miniz 레벨6 스트림) 835→826(오차 범위), 난수 2MiB 133→136. release `--self-test` 5회 중앙값은 실제 XLSX 10만 셀 패킹·해제, DOCX→DOCX/HWPX, deflate 왕복 모두 3% 이내로 같습니다(측정 잡음 범위).
- **Why** 오프라인·공급망 표면을 넓히지 않으면서(PD-053) 검증된 구현의 성능을 얻기 위함입니다.
- **Trade-offs** 이식한 코드는 상류 수정이 자동으로 들어오지 않아 직접 따라가야 합니다. 출력 형식이 바뀌는 이식(예: DEFLATE 압축기)은 바이트 고정 테스트를 왕복 동등성 기준으로 바꿔야 합니다.
- **Alternatives rejected** 크레이트를 의존성으로 추가(PD-053의 무의존·공급망 원칙 위반), `unsafe` SIMD 구현 이식(코드 원칙의 `unsafe` 금지 위반, 예: CRC32 4중 PCLMULQDQ 폴딩은 문서당 0.1ms 미만 이득이라 보류).
- **Verified by** `cargo test -p easyoffice-formats base64_codec`(RFC 4648 벡터, 0~199바이트 전 꼬리 길이에서 이전 인코더와 바이트 동일·왕복, 잘못된 입력에서 이전 디코더와 결과 동일), `cargo run -p easyoffice-app -- --self-test`
- **Revisit when** 상류 구현이 크게 빨라지거나 보안 수정이 나올 때, 남은 후보를 측정할 때(해제는 miniz_oxide 대비 시트 XML에서 약 25% 느려 다중 리터럴 해독을 더 시도할 여지가 있음).

## 71. 문서 그림 블록: PNG/JPEG 모델, 상한, 형식별 처리 (PD-096)

- **Status** Implemented (모델, DOCX·ODT·HWPX·PDF 작성, DOCX·ODT·HWPX·Pages 읽기, 편집기 JSON 왕복과 표시).
- **Context** 서식 문서 모델(`RichDocument`)에 그림이 없어 Pages·DOCX 그림이 변환 때 사라졌습니다(#168).
- **Decision (`PD-096`)**
  1. **모델**: `RichBlock::Image(RichImage)`. 바이트, 형식(PNG·JPEG, 매직 바이트로 판정), 화소 크기, 표시 크기(EMU 하나로 통일), 대체 텍스트를 가집니다. `RichImage::new`만이 만드는 길이라 모든 그림이 같은 검증을 거칩니다. 표시 크기는 화소×9,525EMU(96dpi)를 쪽 본문(폭 5,731,510EMU, 높이 8,640,000EMU)에 맞춰 비율을 유지하며 줄인 값이고, `set_display_emu`로 바꿀 수 있습니다.
  2. **검증(fail-closed)**: PNG는 시그니처, 모든 청크의 길이·CRC, IHDR 값(색 유형·비트 깊이 조합), PLTE(팔레트 영상), IDAT 1개 이상, IEND 뒤 잔여 바이트 없음을 요구합니다. JPEG는 SOI, 프레임 헤더(SOF0·1·2, 8비트, 크기 0 아님), SOS, 끝 표지(EOI)를 요구하고 무손실·산술 부호화는 거부합니다. 그 밖의 형식(GIF, EMF 등)은 `InvalidArgument`입니다.
  3. **상한**: 그림 하나 32MiB(`MAX_IMAGE_BYTES`), 화소 5,000만(`MAX_IMAGE_PIXELS`), 대체 텍스트 4,096바이트, 문서당 256개(`MAX_IMAGES_PER_DOCUMENT`)·합계 128MiB(`MAX_TOTAL_IMAGE_BYTES`). 작성기는 상한을 넘으면 `ResourceBudgetExceeded`로 실패하고, 읽기기는 개별 그림의 손상·형식·크기 문제는 경고와 함께 건너뛰되 문서당 개수·합계 초과는 오류로 처리합니다.
  4. **작성**: DOCX는 `word/media/imageN.(png|jpg)` + 관계(`rId2`부터) + `[Content_Types].xml` 기본 확장자 + 본문 `w:drawing/wp:inline`(대체 텍스트는 `wp:docPr descr`). ODT는 `Pictures/imageN.*` + 매니페스트 항목 + `draw:frame/draw:image`(as-char, 대체 텍스트는 `svg:desc`). HWPX는 `BinData/imageN.*` + `content.hpf`·매니페스트 항목 + `hp:pic`(`hc:img binaryItemIDRef`, 설명은 `hp:shapeComment`). 모든 파트는 모델의 바이트를 그대로 저장합니다. PDF는 그림마다 한 쪽을 쓰고(A4 여백 안에 비율 유지 축소), JPEG(회색·RGB)는 `DCTDecode`로 그대로, 투명도 없는 PNG(8비트 이하)는 IDAT를 `FlateDecode`+`Predictor 15`로 그대로, 알파가 있는 8비트 PNG는 필터를 되돌려 색과 알파로 나눠 각각 압축하고 알파는 `/SMask`로 둡니다(최대 1,600만 화소). 팔레트·인터레이스·16비트 PNG와 CMYK JPEG는 PDF에 넣지 않고 대체 텍스트 줄로 낮춥니다.
  5. **그림이 없는 형식**: RTF·Markdown·TXT, 그리고 평면 모델을 쓰는 형식은 대체 텍스트(없으면 `[그림]`)를 한 문단으로 씁니다. WebUI 편집기 JSON은 그림을 `{"type":"image","format","base64","widthPx","heightPx","widthEmu","heightEmu","alt"}` 블록으로 주고받습니다. 저장 때 바이트를 `RichImage::new`로 다시 검증하므로 위 상한이 그대로 적용되고, 손상·과대·형식 불일치 그림은 저장이 거부됩니다. 편집기에서 그림은 편집할 수 없는 `<img class="doc-inline-image" contenteditable="false">`이며 바이트는 DOM이 아니라 `sheet-utils.js`의 그림 저장소에 한 번만 두고 노드에는 id·크기·대체 텍스트(`data-eo-image-*`)만 남겨 되돌리기 스냅샷이 커지지 않습니다. 표시는 `blob:` URL이라 데스크톱 셸(CSP `img-src 'self' data: blob:`)에서 보이고, `--serve`의 기본 CSP(`default-src 'self'`)에서는 브라우저가 막아 대체 텍스트만 보입니다(CSP는 바꾸지 않음). 충돌 복구 기록처럼 바이트 저장소가 비어 있으면 저장 때 그림은 대체 텍스트 문단이 됩니다.
  6. **읽기**: ODT는 `draw:frame/draw:image`(`xlink:href`가 패키지 안 상대 경로일 때만 — 절대 경로·`..`·`:`가 든 외부 링크는 경고와 함께 건너뜀, 크기는 `svg:width/height`, 대체 텍스트는 `svg:desc`, 없으면 `svg:title`), HWPX는 `hp:pic`의 `hc:img binaryItemIDRef`를 `content.hpf`의 `opf:item`으로 풀어 `BinData` 파일을 읽고(`hp:curSz` HWPUNIT×127 = EMU, 대체 텍스트는 `hp:shapeComment`) 같은 검증·상한을 적용합니다. 표 안의 그림은 경고와 함께 건너뜁니다. DOCX는 `w:drawing`의 `a:blip r:embed`를 문서 관계로 풀어 `word/` 아래 미디어를 읽고(경로 이탈·외부 대상 거부), `wp:extent`와 `wp:docPr descr`를 크기·대체 텍스트로 씁니다. 표 안의 그림과 포함되지 않은 도형·차트는 경고와 함께 건너뜁니다. Pages는 그림 개체(형식 3005)의 데이터 식별자를 패키지 메타데이터(형식 11006)의 파일 이름으로 풀어 `Data/` 파일을 읽고 기하 크기(pt)를 표시 크기로 씁니다. 그림은 첨부 위치(문단 바로 뒤)의 문서 순서에 놓입니다.
- **Why** 그림이 있는 문서를 열고 다른 형식으로 저장해도 그림이 남게 하기 위함입니다. 형식을 두 가지로 한정한 것은 문서 형식 대부분이 이 둘을 그대로 담을 수 있고 재인코딩 없이 바이트를 보존할 수 있기 때문입니다.
- **Trade-offs** 구조 검증만 하고 화소 디코드는 하지 않으므로 압축 데이터가 깨진 PNG는 열람기에서야 드러납니다(PDF의 알파 PNG만 디코드). 쪽 배치는 인라인(글자처럼 취급) 하나뿐이며 글 감싸기·자르기·회전은 읽지 않습니다. Pages 개체 스키마는 공개 문서가 없어 픽스처로 확인한 필드만 씁니다.
- **Alternatives rejected** GIF·WebP·SVG 허용(재인코딩 또는 별도 파서 필요, 공격 면 증가), 그림 바이트를 열 때 전부 디코드해 검증(큰 PNG에서 메모리 폭증), 모든 그림을 PDF에서 래스터화해 한 형식으로 통일(화질 손실과 의존성).
- **Verified by** `cargo test -p easyoffice-formats --test rich_images --test pages_fixtures`(작성기별 파트·참조·바이트 일치, DOCX 왕복, 손상·과대·과다 그림 거부, PDF 스트림, Pages 픽스처의 정확한 바이트·위치), `cargo test -p easyoffice-formats pdf_image`(PNG 필터 5종 복원).
- **Revisit when** 글 감싸기·자르기 지원을 요구받을 때, GIF·WebP 요구가 생길 때, 편집기에 그림 삽입 UI를 넣을 때.

## 72. Pages 가져오기 2단계: 도형 글자, 각주·머리글·바닥글 부록, 글꼴·첨자·들여쓰기·줄 간격 (PD-097)

- **Status** Implemented (픽스처 3개로 확인된 범위). 표 셀의 문자열(3)·기간(7)·오류 종류는 픽스처에 없어 구현하지 않았습니다.
- **Context** PD-096 다음 단계로 Pages 개체 중 본문 흐름 밖에 있던 글자와 문단·글자 서식을 가져옵니다(#168).
- **Decision (`PD-097`)**
  1. **도형·텍스트 상자(형식 2011)**: 첨부 위치(문단 바로 뒤)에서 도형의 글 저장소(필드 2)를 읽어 빈 문단을 뺀 문단으로 넣습니다. 도형 안의 첨부는 표를 허용하지 않고 건너뛴 개체로 셉니다.
  2. **부록**: 각주(본문 저장소 필드 11 → 형식 2012 → 글 저장소), 머리글, 바닥글 문단을 본문 끝 뒤에 제목 문단("각주", "머리글", "바닥글", 제목 수준 2)과 함께 이어 붙입니다. 머리글·바닥글은 모든 머리글/바닥글 목록(형식 10143)을 개체 번호 순으로 읽고 같은 문단은 한 번만 넣으며, 쪽 번호 같은 표식은 건너뛴 개체로 세지 않습니다(쪽 번호 자리는 빈 글자). 각주는 본문의 표식 위치와 연결하지 않고 목록으로만 둡니다. 댓글은 계속 가져오지 않고 경고로 알립니다.
  3. **글자 서식**: `RichRun.script`(`Normal`/`Superscript`/`Subscript`)를 추가했습니다. 글꼴 이름은 문단 스타일에는 템플릿 기본 글꼴이 항상 들어 있어 글자 스타일에 지정된 것만 가져옵니다.
  4. **문단 서식**: `RichParagraph`에 왼쪽 들여쓰기(twip), 첫 줄 들여쓰기(twip, 음수는 내어쓰기), 줄 간격(단일 간격의 백분율)을 추가했습니다. Pages는 줄 간격 "배수" 방식만 읽고 100%는 지정 없음으로 둡니다. 스타일에서 상속된 값도 그대로 가져옵니다(예: 인용 스타일의 왼쪽 여백).
  5. **작성기**: DOCX는 `w:vertAlign`, `w:spacing w:line`(자동), `w:ind`(left/firstLine/hanging)로, ODT는 `style:text-position`(`super 58%`/`sub 58%`), `fo:margin-left`, `fo:text-indent`, `fo:line-height`로, HWPX는 `hh:supscript`/`hh:subscript`, `hc:intent`/`hc:left`(1twip=5HWPUNIT), `hh:lineSpacing`(PERCENT)로 씁니다. RTF·Markdown·TXT·PDF는 이 서식을 쓰지 않습니다. 읽기는 DOCX, ODT, HWPX가 모두 첨자·들여쓰기·줄 간격을 되돌립니다. ODT는 `fo:margin-left`·`fo:text-indent`·`fo:line-height`(%)와 `style:text-position`을, HWPX는 `hc:left`·`hc:intent`(1twip=5HWPUNIT)·`hh:lineSpacing` PERCENT와 `hh:supscript/subscript`를 읽습니다. 목록 표식으로 쓰는 값(ODT `0.5in`, HWPX 2000HWPUNIT)은 들여쓰기가 아니라 목록으로 읽고, HWPX는 작성기 기본 줄 간격 160%를 지정 없음으로 봅니다.
  6. **표 병합**: 모델에 병합 표현이 없어 기존 방식(병합 영역의 첫 칸만 값, 나머지는 빈 칸)을 유지합니다.
  8. **표 셀 저장 방식 2(Pages 14.4, 파일 형식 26.4.0)**: 실제 문서 2건(`eo-pages-thousands.pages`, `eo-pages-cellkinds.pages`, 메타데이터·미리보기·템플릿 이미지 제거)으로 확인했습니다. 셀은 버전 5 배치(바이트 1 = 종류, 바이트 8 = 플래그 워드, 12부터 켜진 비트 순서대로 필드)이고 필드 크기는 비트 0 = decimal128 16바이트, 비트 1 = double 8바이트, 그 밖의 확인된 비트(3·4·5·6·9·11·12·13·14·16·17)는 4바이트입니다. 값: 종류 2 숫자(비트 0), 3 문자열(비트 3 = 문자열 목록 키), 7 기간(비트 1, 초), 8 오류(비트 3의 캐시 문자열), 9 서식 있는 글. 서식 키는 비트 13(숫자·백분율)·14(통화)·16(기간)입니다. 크기를 모르는 비트(2·7·8·10·15)가 켜진 셀은 이후 오프셋이 추측이 되므로 읽지 않고 "읽을 수 없는 값"으로 남깁니다(fail-closed). 서식 목록 항목은 두 배치를 모두 읽습니다: 신판은 종류 256 = 숫자, 257 = 통화, 258 = 백분율, 소수 자릿수는 필드 2, 통화 코드는 필드 3, 천 단위 구분 기호는 필드 5(켬 1·끔 0, 같은 값에서 켬/끔 비교로 확정)이고, 구판은 256 = 통화(코드 필드 4), 257 = 숫자(소수 필드 3)입니다. 통화 코드 문자열이 있는지로 둘을 가르고 종류를 구판 번호로 맞춥니다. 소수 자릿수 필드가 20보다 크면 "자동"으로 봅니다(검증 필요). 기간 서식은 스타일 1(간략)만 읽습니다: 가장 큰 단위부터 가장 작은 단위까지(주 1, 일 2, 시 4, 분 8, 초 16; 일·시·분은 값으로 확인, 주·초는 같은 2의 거듭제곱 순서로 추정) 값을 Pages 한국어 화면과 같은 단위 낱말로 `1시간 30분`, `2일 3시간`처럼(주·일·시간·분·초) 쓰고 0인 단위도 쓰며 가장 작은 단위는 반올림합니다. 오류 셀은 저장된 오류 문자열(`÷0!`, `#REF!`)을 그대로 보여 주며, 이는 사용자 Pages 한국어 화면의 표시와 같습니다. 내보낼 때도 이 문자열을 그대로 두고 `#DIV/0!` 같은 영어 표기로 바꾸지 않습니다(근거: 원본 화면과 같아야 하고, 문서에 저장된 표시 문자열이며, 영어 오류 이름으로의 대응표를 추측으로 만들지 않기 위함). 기간 단위 낱말은 한국어 고정입니다(다른 언어 문서의 낱말은 확인 전). 표시 값은 사용자 화면(숫자·백분율·통화·기간·오류·병합)과 셀 단위로 대조해 테스트에 고정했습니다. 기간의 0 단위 표기·다른 스타일·반올림은 화면 대조 전이라 검증 필요입니다. 테스트: `crates/easyoffice-formats/tests/pages_cells_v2.rs`, `pages_table` 단위 테스트.
- **Why** 본문 흐름 밖 글자와 서식이 조용히 사라지는 것을 줄이고, 남은 것은 경고로 알리기 위함입니다.
- **Trade-offs** 각주가 부록으로 옮겨져 본문 위치와의 연결을 잃습니다. 필드 번호는 픽스처로 확인한 것만 씁니다(전체 레이아웃의 글꼴·첨자·문단 속성은 확인하지 못해 읽지 않음). 편집기 JSON은 `script`(`superscript`/`subscript`), `leftIndentTwips`, `firstLineIndentTwips`, `lineSpacingPercent`로 새 서식을 주고받습니다(편집기는 `<sup>`/`<sub>`, 문단의 `padding-left`·`text-indent`·`line-height`로 표시하며 열 때의 정확한 twip 값을 `data-eo-*`에 보존).
- **Alternatives rejected** 각주를 각주 번호가 붙은 문단으로 본문 안에 끼우기(표식 위치 해석이 픽스처 한 건뿐), 문단 스타일의 글꼴도 가져오기(모든 런에 템플릿 글꼴이 붙음).
- **Verified by** `cargo test -p easyoffice-formats --test pages_fixtures --test rich_script_and_layout`(도형·텍스트 상자 위치, 부록 문단, 글꼴·첨자·들여쓰기·줄 간격의 픽스처 값, 작성기별 XML과 DOCX·ODT 읽기).
- **Revisit when** 문자열·기간·오류 셀이 있는 Pages 문서를 구하게 되었을 때, 각주 표식 위치를 연결할 때, ODT·HWPX 문단 서식 읽기를 추가할 때.

## 73. UI 기준을 참조 앱/rhwp-studio 크롬으로 통일하고 자체 디자인 시스템을 폐기 (PD-098)

- **Status** Partially implemented (1단계: 토큰·테마·폰트, 2단계: 홈 화면·최근 문서 대화상자·제품 정보 버전 줄·시작 창 정상 크기, 3a단계: 메뉴 바·도구 상자·서식줄·상태줄 표면을 rhwp-studio 규칙으로 맞추고 리본 명령을 메뉴로 노출, 3b단계: 리본·빠른 실행 도구 모음·측면 패널 제거와 도구 대화상자 전환, 4a단계: 시트 화면 재입힘, 4b단계: 슬라이드 화면 재입힘, 4c단계: PDF 스튜디오·변환기 재입힘). 5단계(macOS 네이티브 메뉴·업데이트 알림)는 단계별 PR로 같은 절을 갱신합니다.
- **Context** 사용자 결정: 자체 디자인 설계 없이 앱 전체를 참조 앱(참조 앱, MIT) 구조로 갑니다. 참조 앱의 UI는 rhwp-studio 크롬(메뉴 줄, 아이콘 도구 상자, 서식 도구줄, 상태 표시줄, 색 토큰)에 홈 화면·최근 문서·업데이트 알림 등을 얹은 것이고, EasyOffice의 `third_party/rhwp`는 참조 앱과 같은 v0.8.7이라 같은 CSS를 이미 갖고 있습니다. 계획 전체는 `참조 앱-plan` 검토 결과를 따릅니다.
- **Decision (`PD-098`)**
  1. **토큰**: 색·글꼴·간격·반경·그림자는 rhwp-studio `base.css`의 토큰(`--ui-*`, `--accent-*`, `--doc-*`, `--focus-ring`, `--font-family-ui` …)만 씁니다(`ui/tokens.css`, 값은 그대로, 출처 헤더와 THIRD_PARTY_NOTICES 고지). 라벤더 팔레트(`--eo-*`)와 `docs/design-system.md`의 Swift 시대 설명은 폐기하고, 그 문서는 이 결정의 토큰 사용 규칙으로 바꿨습니다.
  2. **테마**: 모드는 시스템/밝게/어둡게 3택이고 기본은 시스템입니다. `<html data-theme-mode>`·`data-theme-effective`와 `color-scheme`을 `ui/theme.js`가 `<head>`에서 동기로 적용하고(첫 그림부터 올바름), 선택은 `localStorage`의 `easyoffice.theme-mode`에 저장하며 시스템 모드에서만 OS 변경을 따릅니다. 보기 메뉴에 `테마 > 시스템 설정/밝게/어둡게`(radio)가 있고 Ctrl+Alt+D(밝게/어둡게 전환)와 상단 버튼은 같은 API(`toggle`)로 명시 선택이 됩니다. 한글 편집기에는 모드(`system`/`light`/`dark`)를 그대로 전달합니다. 되돌리기 스냅샷의 `theme`은 모드 문자열입니다.
  3. **모듈 강조색 폐기**: 모듈별 색(`--eo-accent-*`, 시작 카드·탭 배지·`module-swatch`·인스펙터의 "테마 색상")을 없애고 강조는 단일 `--accent-strong`(텍스트·선)와 `--ui-menu-open`(열린 메뉴)을 씁니다. 부트 JSON의 `palette`·`moduleAccents`와 레이아웃 크레이트의 `SignatureLavenderPalette`·`OfficeModuleKind::accent_hex`도 삭제했습니다(슬라이드 덱 내보내기용 `SlideDeckTheme::SignatureLavender`는 문서 내용 테마라 유지).
  4. **고정 색**: 문서 종이와 슬라이드 쇼는 테마와 무관하게 자기 색을 갖습니다(rhwp의 `--doc-paper`와 같은 원칙). 이런 규칙은 `ui/content.css`에만 두고 리터럴 색을 허용하며, `#doc-editor`는 라이트 토큰을 다시 선언해 어두운 테마에서도 종이 안쪽이 밝게 유지됩니다(테스트가 라이트 값과의 일치를 검사). `styles.css`·`shell.css`에는 리터럴 색(검정 그림자 제외)과 `data-theme="…"` 선택자를 두지 않습니다.
  5. **대비**: 크롬 글자 4.5:1, 아이콘 3:1을 토큰 값으로 계산하는 테스트(`test-webui-shell-chrome.mjs`)를 새 토큰에 맞춰 다시 썼습니다. rhwp 다크 값 중 흰 글자와 `--ui-menu-open`은 4.2:1이라 다크 테마의 열린 메뉴는 `--ui-selected` 바탕과 `--ui-text`로, 기본 버튼은 `--accent-strong` 바탕과 `--ui-bg` 글자로 그립니다.
  6. **CSP·가드는 그대로**: 참조 앱 CSP의 `unsafe-eval`·`style-src 'unsafe-inline'`은 채택하지 않습니다(`DESKTOP_SHELL_CSP`, `MAIN_CSP`, `STUDIO_CSP` 불변). 참조 앱 코드를 옮길 때는 TypeScript를 JavaScript로 다시 쓰고 인라인 `style` 속성은 클래스로 바꿉니다.
  7. **홈 화면(2단계)**: 시작 창은 컴팩트 전용 창이 아니라 문서 창과 같은 크기(1280x860, 최소 1024x680, 최대화 가능)의 창이고 안에 참조 앱의 홈 화면(`ui/home-screen.js`, `home.css`)이 뜹니다. 구성은 참조 앱과 같은 hero(제목·설명·큰 버튼) + 최근 문서 카드(쪽당 5개, 이전/다음, 빈 자리 카드) + 끌어 놓기 안내이고, EasyOffice 고유 차이는 새 문서 버튼 6종(아이콘 포함)과 "문서 열기" 버튼, 카드가 경로·미리보기 없이 이름과 종류만 보인다는 점입니다(셸은 경로를 페이지에 주지 않음, PD-088). 열기·끌어 놓기·최근 문서 열기는 기존 창 다리(`window-bridge.js`)와 셸 명령을 그대로 쓰고, 문서를 열면 시작 창이 닫히는 동작도 같습니다. `파일 > 최근 문서 > 전체 목록 보기…`가 참조 앱의 최근 문서 대화상자(`ui/recent-documents-dialog.js`: 목록 상자, 방향키·Enter·Esc, 더블클릭)를 열며 "목록 비우기"는 셸에 대응 명령이 없어 넣지 않았습니다. 제품 정보 대화상자에는 부트 응답의 `appVersion`(크레이트 버전)으로 `EasyOffice x.y.z` 줄이 붙습니다.
  8. **메뉴 바(3a단계)**: 메뉴는 rhwp-studio의 `.menu-item/.menu-title/.menu-dropdown/.md-item/.md-sub/.md-sub-panel` 구조와 규칙(`ui/menu-bar.css`)이고 순서는 파일·편집·보기·입력·서식·쪽·표·도구·도움말입니다. 도구 상자·서식줄·상태줄의 배경·경계·호버 값도 rhwp 값으로 바꿨습니다(`ui/toolbar.css`, 크기는 아이콘만 있는 EasyOffice 도구 상자에 맞춰 유지). 명령 id는 그대로이고, 리본 카탈로그 140종(5개 모듈 × 28)은 모두 활성 메뉴 항목에서 실행됩니다. 메뉴 항목은 `data-studio-menu-cmd`가 리본 명령 id의 접미사(`home-bold` → `<모듈>-home-bold`)이거나 모듈 전용 전체 id(`sheet-mod-pivot`, 해당 모듈에서만 활성)이며, 대응은 `ribbon-catalog.js`의 `resolveMenuCommand` 한 곳에 있고 구 항목 3개(`mod-footnote`, `mod-header`, `mod-symbol`)는 별칭입니다. 새 위치: 보기(100% 배율·스플릿 뷰·문서 개요), 입력(목차·메일 머지·누름틀), 쪽(공문서 두문·원고지·슬라이드 마스터/정렬/전환/발표자), 표(표 셀 수식·테두리, 자동 합계·함수 마법사·정렬·유효성, 피벗·차트·조건부 서식·틀 고정), 도구(변경 추적·메모·보안 검사·통계, PDF 주석·회전·분할·변환). 동작은 rhwp 메뉴 바와 같이 호버 전환, Esc로 닫기, 메뉴가 열린 상태에서 단일 키(`/`)로 해당 단축키 항목 실행이고, 최근 문서 대화상자는 Ctrl+Alt+O(단축키 표 `recent-dialog`, 한글 편집기에서도 전달)입니다. 리본 제거는 10항입니다.
  9. **고지**: 참조 앱·rhwp-studio에서 옮긴 파일은 머리에 출처를 쓰고 THIRD_PARTY_NOTICES의 `참조 앱 (code adapted)`·rhwp 절에 목록과 MIT 전문을 둡니다(PD-095 3항과 같은 방식).
  10. **리본 제거와 도구 대화상자(3b단계)**: 리본 탭·도구 띠·빠른 실행 도구 모음·"표준/한글 리본" 전환·접기 단추·좌우 측면 패널과 그 CSS를 없앴습니다. 명령 실행 경로는 그대로라서 메뉴와 아이콘 도구 상자가 `resolveMenuCommand`로 카탈로그 id를 찾아 `execute_ribbon_command`를 호출하고, 카탈로그 140종과 IPC 계약(`execute/undo/redo_ribbon_command`, `switch_ribbon_style`, `get_ribbon_category_commands`)·Rust `RibbonLayoutStyle`은 유지합니다. 다만 리본 접기 항목(`*-view-ribbon`, 모듈당 1개)은 메뉴에서 뺐으므로 메뉴에서 닿는 명령은 모듈당 27개(135종)입니다. 부트 JSON에서 `ribbonCategories`·`ribbonMatrixSlots`·`recentFormatFilters`를 뺐습니다. 스마트 보조(`문장 진단 및 요약`), 개인정보 스캔/마스킹, 보안 검사, 성능 진단, 문서 개요는 측면 패널 대신 `도구`·`보기` 메뉴에서 여는 대화상자(`<dialog class="tool-dialog">`, `.dialog-wrap/.dialog-title/.dialog-body/.dialog-footer` 재사용, 닫기 단추와 Esc)로 옮겼고 요소 id와 실행 동작은 같습니다. 저장·되돌리기·다시 실행은 `파일`·`편집` 메뉴와 단축키로 합니다.
  11. **시트 화면(4a단계)**: 수식 입력줄은 별도 행으로 두되 rhwp 서식 바 표면(`eo-doc-style-bar`, `#sheet-formula-bar`는 `toolbar.css`에서 `#doc-style-bar`와 같은 배경·경계)과 `.sb-field-grid`·`.sb-select`·`.tb-group`·`.tb-btn` 클래스를 쓰고, 시트 탭(`#sheet-tab-bar`)은 시트 칸 안쪽이 아니라 시트 창의 마지막 행으로 옮겨 상태 표시줄 바로 위에서 같은 표면(`--ui-bg-light`, `--ui-border-strong`)과 아래쪽이 둥근 탭 모양(활성 탭은 `--ui-surface`)을 갖습니다. 그리드 머리글은 `--ui-bg-light`, 선택은 `--accent-strong`입니다. 요소 id·동작은 같고 리터럴 색은 없습니다(`test-webui-sheet-chrome.mjs`). 같은 변경에서 `workspace-tools.js`의 개인정보 스캔·마스킹 로직을 `workspace-scrub.js`로 분리했습니다(동작 동일, 1,000줄 상한 여유).
  12. **슬라이드 화면(4b단계)**: 슬라이드 도구줄(`#slide-toolbar`)은 서식 바 표면에 `.tb-group`/`.tb-btn` 아이콘 단추(추가·복제·삭제·위/아래·처음부터·현재 슬라이드부터·레이저 포인터, 이름은 `aria-label`)와 `.sb-field-grid`의 비율·화면 전환 콤보 상자로 바뀌었습니다. 콤보 상자는 참조 앱의 custom-select를 JavaScript로 옮긴 `ui/custom-select.js`(`select.sb-combo`만 대상, 출처 헤더·고지, CSS는 `toolbar.css`)이고 원래 `<select>`는 숨은 상태 보관자로 남아 `value`·`change`·`options`가 그대로입니다(코드에서 `select.value`를 대입해도 표시가 갱신됨). 레이저 포인터 단추는 글자 대신 `aria-pressed`와 `title`("레이저 포인터: 켬/끔")로 상태를 알립니다. 썸네일 패널은 `--ui-bg-light`·`--ui-border`·`--ui-selected`, 편집 스테이지는 `--doc-paper`·`--doc-shadow`이고 스테이지 안쪽 글자·경계는 `content.css`의 `.slide-stage`가 라이트 값으로 고정합니다(슬라이드 쇼 고정색도 `content.css`). 동작과 요소 id는 같고 기존 슬라이드 테스트는 수정 없이 통과합니다(`test-webui-slide-chrome.mjs`가 DOM 계약과 토큰을 검사).
  13. **PDF 스튜디오·변환기(4c단계)**: PDF 도구줄(`#pdf-toolbar`)은 서식 바 표면(`toolbar.css`의 `#doc-style-bar`와 같은 값)에 `.tb-group`/`.tb-btn`(회전은 아이콘 단추, 양식·스탬프 적용은 글자 단추)과 `.sb-field-grid` 안의 `.sb-select` 스탬프 선택·워터마크 입력으로 바뀌었고 회전 배지는 `.sb-size-unit`입니다. 페이지 카드는 `--doc-paper`·`--doc-shadow`이며 안쪽 글자·경계는 `content.css`의 `.pdf-page-card`가 라이트 값으로 고정합니다. 변환기의 입력·선택은 `.sb-select`, 단추는 `.dialog-btn`(변환은 `.dialog-btn-primary`), 결과 상자(`.security-card`)는 `--ui-surface-raised`·`--ui-border` 토큰 상자입니다. 요소 id·동작은 같고 `.secondary-btn`·`.primary-btn`·`.cmd-input`·`.ctx-badge`는 이 두 화면에서 쓰지 않습니다. 스탬프·변환 형식 선택은 4b단계의 콤보 상자(`select.sb-combo`, custom-select)를 씁니다(`test-webui-pdf-conv-chrome.mjs`가 DOM 계약과 토큰을 검사).
- **Why** 두 개의 디자인 체계를 유지하지 않고 한글 편집기(rhwp-studio)와 나머지 창이 같은 토큰·테마를 쓰게 하기 위함입니다.
- **Trade-offs** 모듈을 색으로 구분하던 단서가 사라집니다(아이콘·창 제목으로 구분). rhwp의 일부 토큰(`--ui-text-hint` 등)은 작은 글자에서 대비가 낮아 그 위의 글자에는 쓰지 않습니다.
- **Alternatives rejected** 라벤더를 rhwp 토큰 위에 별칭으로 유지(두 체계가 남음), 참조 앱 CSP 채택(코드 실행 허용 범위 확대).
- **Verified by** `node --test scripts/test-webui-theme.mjs scripts/test-webui-shell-chrome.mjs scripts/test-webui-hwp-shell.mjs scripts/test-webui-menu-bar.mjs scripts/test-webui-tool-dialogs.mjs`(모드 저장·OS 추종·라디오 메뉴, 리터럴 색·`--eo-` 잔재 없음, 정의된 토큰만 사용, 종이 팔레트, 대비), `node scripts/check-repo-hygiene.mjs`.
- **Revisit when** 2단계 이후 PR이 메뉴·도구 상자·시작 화면을 바꿀 때마다 이 절의 상태와 `design-system.md`를 갱신합니다.

## 74. 데스크톱 창 크기·위치·최대화 상태 기억과 macOS 최근 항목 (PD-099)

- **Status** Implemented
- **Context** 데스크톱 셸은 창을 열 때마다 기본 크기(1280x860)로 화면 가운데에 띄워, 창 크기를 바꾸거나 다른 모니터로 옮겨 써도 다음 실행에 사라졌습니다. 참조 셸(참조 앱)은 Tauri `window-state` 플러그인으로 창 위치를 기억합니다. 다만 그 플러그인은 창 이름(label)을 키로 한 JSON 맵을 저장하는데, 우리 문서 창 이름은 실행마다 새로 만들어지므로(`doc-<번호>-<시각>`) 맵이 계속 커지기만 하고 다시 쓰이지 않습니다.
- **Decision (`PD-099`)**
  1. **한 레코드**: 마지막으로 크기·위치를 바꾼 문서 창(시작 창 포함)의 보통 상태 크기(논리 픽셀), 위치, 최대화 여부를 앱 데이터 폴더의 `window-state.bin` 하나에 저장합니다. 형식은 고정 18바이트 리틀 엔디언(`EOWS` + 버전 1 + 플래그 1바이트(비트 0 = 최대화) + 너비·높이 u16 + x·y i32)이고, 길이·매직·버전·모르는 플래그 비트·200 미만의 변 중 하나라도 어긋나면 레코드 전체를 버립니다(기본값으로 시작). 값 변환은 반올림 후 범위 검사, 모니터 경계 계산은 `checked_add`로 오버플로 시 "화면 밖"으로 판단합니다.
  2. **복원**: 새 창은 저장된 크기를 기존 규칙(모니터의 92% 이하, 최소 1024x680)으로 잘라 씁니다. 저장된 위치는 그 창이 앱의 유일한 창일 때만, 그리고 제목 표시줄의 왼쪽 위 모서리와 그 안쪽 120x40 지점이 모두 연결된 모니터 안에 있을 때만 다시 씁니다. 그 밖의 창은 이전처럼 계단식으로 놓입니다. 최대화로 닫았으면 새 창도 최대화로 엽니다.
  3. **기록 시점**: 이동·크기 변경 이벤트는 메모리의 값만 바꾸고(최대화 중에는 플래그만 켜서 최대화 해제 시 이전 크기로 돌아감, 최소화 중에는 무시), 디스크에는 창이 닫힐 때 한 번 씁니다. `--smoke`는 읽지도 쓰지도 않아 이전 실행에 영향을 받지 않습니다.
  4. **의존성 없음**: 플러그인을 추가하지 않고 셸 안의 작은 모듈(`window_state.rs`)로 구현합니다. WebView에는 새 명령·권한이 없습니다.
  5. **macOS 최근 항목**: 문서를 열거나 새 경로에 저장할 때 앱 안 최근 문서(`recent.rs`)와 함께 macOS `NSDocumentController.noteNewRecentDocumentURL`에도 알려 Dock 메뉴와 시스템 최근 항목에 나오게 합니다(참조 앱 `macos_recent_documents.rs` 이식, `os_recent.rs`). 메인 스레드가 아니면 `run_on_main_thread`로 넘기고 실패는 무시합니다(앱 안 목록이 기준). 의존성은 macOS 대상에만 `objc2-app-kit`·`objc2-foundation` 0.3.2(MIT, Tauri가 이미 쓰는 같은 버전)를 두 기능(`NSDocumentController`, `NSURL`)만 켜서 추가합니다. Windows·Linux는 아무 일도 하지 않습니다.
- **Why** 사용자가 맞춘 창 크기와 모니터를 다음 실행에도 쓰게 하고 Dock에서 최근 문서를 다시 여는 것은 참조 앱과 같은 동작입니다. 창 이름이 매번 바뀌는 창 모델에서는 "마지막 창 하나"가 실제로 다시 쓸 수 있는 유일한 정보이고, 고정 레코드는 파일이 커지지 않으며 손상 검사가 단순합니다.
- **Trade-offs** 창마다 따로 기억하지 않습니다(마지막 값 하나). 모니터 구성이 바뀌어 위치를 버리면 크기만 복원됩니다. 앱이 강제 종료되면 마지막 이동은 저장되지 않습니다.
- **Alternatives rejected** `tauri-plugin-window-state`(창 이름 키 맵이 무한히 커지고, 문서 창에는 같은 이름이 다시 오지 않음), 이동할 때마다 디스크 쓰기(불필요한 I/O), JSON 형식(파서·할당이 필요하고 고정 크기 레코드보다 검증이 느슨함).
- **Verified by** `cargo test -p easyoffice-desktop window_state`(레코드 바이트 배치·왕복, 손상 레코드 거부, 반올림·범위, 모니터 판정·오버플로, 데이터 폴더 왕복). 실제 창 복원은 Windows·macOS 수동 확인 대상입니다.
- **Revisit when** 창 종류별로 다른 크기를 기억해야 할 때, 여러 창의 배치를 세션으로 복원할 때.

## 75. 암호가 걸린 HWP 열기: 업스트림 흐름 사용과 암호 비보관 (PD-100)

- **Status** Implemented (기존 오버레이 흐름 확인과 가드 테스트 추가)
- **Context** 참조 앱은 `document-open.ts` 어댑터로 암호 문서를 열지만, EasyOffice가 고정해 쓰는 rhwp-studio의 열기 깔때기(`apps/hwp-studio/src/main.ts`의 `loadDocumentForOpen`)가 같은 흐름을 이미 갖고 있고 고정 WASM(`vendor/rhwp-core`)에도 `HwpDocument.openWithPassword`가 있습니다. 같은 일을 하는 코드를 한 벌 더 옮기면 두 흐름이 남으므로 참조 앱 어댑터는 옮기지 않았고(THIRD_PARTY_NOTICES에 새 파일 없음) 업스트림 흐름을 정책과 테스트로 고정했습니다.
- **Decision (`PD-100`)** (1) 일반 열기를 먼저 시도하고 엔진이 "비밀번호가 필요한 암호 문서"라고 보고할 때만 암호 대화상자(`showHwpPasswordDialog`, 업스트림 `.dialog-wrap` 대화상자)를 띄웁니다. 다른 오류는 그대로 올립니다. (2) 취소하면 `DocumentOpenCancelledError`로 열기를 중단하고 이전 문서 뷰를 되살립니다. (3) 틀린 암호(또는 손상, 구분 불가)는 "암호가 일치하지 않거나 문서가 손상되었습니다. 다시 입력하세요."로 다시 묻습니다. (4) 지원하지 않는 암호화 방식·DRM·그 밖의 실패는 고정된 한국어 오류로 끝내며 엔진 원문 오류(입력값이 섞일 수 있음)는 화면·콘솔에 내지 않습니다. (5) 암호는 한 번의 시도 범위의 지역 변수로만 쓰고 시도 직후 비웁니다. 저장소·최근 문서·자동 저장·로그·`postMessage`·임베드 프로토콜·셸 명령으로 보내지 않으며(셸은 파일 바이트만 전달), 암호로 저장(내보내기)은 이번 범위 밖입니다. (6) 암호 입력을 기다리는 동안 셸의 열기 제한 시간은 10분입니다(`hwp-session.js`).
- **Why** 암호는 문서를 푸는 데만 필요하고 어디에도 남지 않아야 하며, 같은 흐름을 두 벌 유지하지 않기 위함입니다.
- **Trade-offs** 문자열 암호는 JavaScript에서 확실히 지울 수 없어 지역 참조만 비웁니다. 오입력과 암호문 손상을 구분하지 못합니다(업스트림과 같음).
- **Alternatives rejected** 참조 앱 어댑터를 따로 이식(중복), 암호를 셸이 받아 네이티브에서 풀기(암호가 셸 경계를 넘음).
- **Verified by** `node --test scripts/test-hwp-studio.mjs`(실제 흐름 코드를 모의 엔진·대화상자로 실행: 암호 없음·정답·오답 후 재시도·취소·미지원/DRM/손상·암호 비반향, 암호가 임베드·셸 파일에 없음, 고정 WASM과 번들에 흐름 존재), 업스트림 `rhwp-studio/tests/hwp-password-open.test.ts`.
- **Revisit when** 암호로 저장을 지원하거나 업스트림 열기 깔때기가 바뀔 때(오버레이 해시 갱신과 함께 이 절을 갱신합니다).

## 76. 한글 문서 검색 가능한 PDF 내보내기: 데스크톱 셸의 svg2pdf 의존성 정책 (PD-101)

- **Status** Implemented (셸 변환·저장 명령·상한, 글꼴 이름 대체표, 한글 창 파일 메뉴 "PDF로 내보내기" 연결; 번들 TTF 글꼴은 두지 않기로 결정)
- **Context** 한글 문서를 PDF로 만들 때 글자가 그림이 되면 검색·복사가 안 됩니다. 참조 앱은 rhwp의 페이지 SVG를 `svg2pdf`로 바꿔 글꼴 부분집합과 `ToUnicode` 맵이 든 PDF를 만듭니다. rhwp는 같은 변환기의 결정론·그러데이션 수정본을 `vendor/svg2pdf`로 고정해 두며, 우리는 rhwp `1a76570`의 그 사본을 저장소의 `vendor/svg2pdf`로 복사해 씁니다(CI는 서브모듈을 받지 않음, 수정 없음).
- **Decision (`PD-101`)** (1) 의존성은 **데스크톱 셸 크레이트(`easyoffice-desktop`)에만** 둡니다: `svg2pdf 0.13`(`default-features = false`, `text`), `usvg 0.45`, `fontdb 0.23`, `pdf-writer 0.12`. 엔진 크레이트는 외부 의존성 0을 유지합니다(PD-053, PD-095). 이 정책이 셸에 한해 PD-095의 "의존성 0"을 넓히는 유일한 예외이며, 이유는 PDF의 글꼴 서브셋·`ToUnicode`를 자체 구현하면 이식 대상(PD-095의 100줄 기준)을 크게 넘기 때문입니다. (2) 워크스페이스 `[patch.crates-io]`로 `svg2pdf`를 rhwp의 고정 사본으로 바꿉니다(엔진 크레이트는 svg2pdf를 쓰지 않으므로 영향 없음). (3) 라이선스는 전이 의존성 77개를 확인했고 모두 MIT, Apache-2.0, Zlib, BSD-2/3, 0BSD, Unlicense, BSL-1.0 계열(또는 이들의 OR 조합, 유니코드 데이터는 `Unicode-3.0`)이며 MPL·GPL·라이선스 불명은 없습니다. 목록 요약은 THIRD_PARTY_NOTICES에 있습니다. 새 의존성은 같은 확인을 거칩니다. (4) 변환은 `pdf_export.rs`(참조 앱 `pdf_export.rs`와 rhwp PDF 렌더러의 페이지 조립을 옮김): 쪽마다 `usvg::Tree::from_data`(SVG 바이트를 복사 없이 읽음)와 `svg2pdf::to_chunk`, 청크를 한 객체 공간으로 다시 번호 매기고 쪽마다 폼 XObject를 점 단위 MediaBox에 놓습니다. 한 쪽이라도 변환에 실패하면 쪽 번호가 든 오류로 전체를 실패시킵니다(쪽이 조용히 빠진 PDF를 만들지 않음). (5) 상한은 `checked_add`로 변환 전에 확인합니다: 쪽 수 2,000, 쪽당 SVG 64MiB, 합계 512MiB. 글꼴 데이터베이스는 프로세스당 한 번만 만들고(`OnceLock`) 번들 글꼴 폴더(`pdf-fonts`) 다음에 시스템 글꼴을 읽으며 일반 serif/sans/mono 대체는 Noto Sans KR입니다. (6) 새 앱 명령 `export_searchable_pdf(file_name, pages)`는 쪽 SVG 문자열만 받고 경로는 받지 않으며, 기존 저장 대화상자 흐름(`save_via_dialog`)을 `save_document_file`과 공유합니다. 명령 목록·capability는 1:1 테스트로 묶여 있습니다(10개). (7) **글꼴**: TTF는 번들하지 않고 Windows(맑은 고딕, 바탕 등)·macOS(Apple SD Gothic Neo, AppleMyungjo)에 기본으로 깔린 한국어 시스템 글꼴을 씁니다. `pdf_font_fallbacks.rs`(참조 앱 `pdf_font_fallbacks.rs` 이식)가 SVG를 읽기 전에 `font-family` 목록의 문서 글꼴 이름(함초롬바탕·돋움, 바탕·돋움·굴림·궁서 계열, 휴먼·HY 계열 등 정렬된 정적 표 약 35개)을 바탕체/돋움체 구분 없이 명조 계열·고딕 계열로 나눠, 원래 이름은 맨 앞에 두고 그 플랫폼의 설치 글꼴(macOS는 Apple 글꼴 먼저, 그 외는 맑은 고딕·바탕 먼저)과 일반 family를 뒤에 붙입니다. 표 조회는 ASCII 대소문자를 접은 바이트 비교의 이진 탐색이라 할당이 없고, 바꿀 이름이 없는 SVG는 복사 없이 그대로 씁니다. (8) **편집기 연결(데스크톱 전용)**: 오버레이 파일 메뉴의 `data-shell-cmd="export-pdf"` 항목(기본 숨김)을 셸이 `setShellContext`의 `pdfExport: true`로 보일 때만 표시합니다(`--serve`에서는 숨김 유지). 명령 허용 목록은 `new|open|recent|save|save-as|export-pdf|close`이고 편집기·셸이 같은 목록을 쓰며(테스트가 대조) `export-pdf`는 인수를 받지 않습니다. 셸(`hwp-session.js` `exportPdf`)이 임베드 포트로 `pageCount`를 묻고 2,000쪽을 넘으면 쪽을 렌더하기 전에 거부하며, `getPageSvg`를 쪽마다(긴 시간 제한) 받아 문자열 배열을 `export_searchable_pdf`로 넘깁니다. 진행·결과는 기존 알림(토스트)으로 알립니다.
- **측정** 20쪽(320x120 px, 영문 한 줄)을 같은 글꼴로 변환했을 때(`cargo test -p easyoffice-desktop pdf_export -- --nocapture`): 글자 임베드(검색 가능) 약 270ms·202KB, 글자를 윤곽선으로 바꾼 PDF 약 290ms·166KB입니다. 임베드 쪽만 `/ToUnicode`와 `/FontFile2`를 가지며 윤곽선 쪽에는 둘 다 없습니다(검색·복사 불가). 임베드는 파일이 약 22% 크지만 시간은 같은 수준입니다.
- **Why** 한글 문서 PDF의 글자를 검색·복사할 수 있게 하되 엔진 크레이트의 무의존을 지키고, 검증된 변환기와 rhwp가 고정한 수정본을 그대로 쓰기 위함입니다.
- **Trade-offs** 셸 바이너리가 커지고 빌드 시간이 늘어납니다(의존 크레이트 약 77개). 번들된 TTF가 없어 글꼴은 시스템 글꼴에 의존합니다(번들 글꼴은 woff2뿐이라 SFNT 변환이 필요, 리눅스처럼 한국어 글꼴이 없는 환경에서는 글자가 비어 보일 수 있음).
- **Alternatives rejected** `rhwp` 크레이트 전체를 셸에 의존(암호·서명 등 불필요한 큰 의존 트리), 글자를 윤곽선으로 변환(검색 불가), 엔진 크레이트에 PDF 글꼴 서브셋 자체 구현(이식 기준 초과).
- **Verified by** `cargo test -p easyoffice-desktop pdf_export save_file`(상한, 쪽 오류 전파, `/ToUnicode`·글꼴 임베드·쪽 수·MediaBox, 결정론, 글꼴 DB 1회), 명령·capability 1:1 테스트.
- **Revisit when** 시스템에 한국어 글꼴이 없는 환경(리눅스 등)을 지원할 때(번들 TTF 또는 woff2 해제), 하이퍼링크 주석을 넣을 때, 쪽 범위 선택을 추가할 때.

## 77. 네이티브 메뉴 막대: 모든 OS, 한 벌의 표와 같은 핸들러, 이벤트 권한 없음 (PD-102)

- **Status** Implemented, 개정 2026-11-08 (QA 요청: 참조 앱과 같은 동작. macOS·Windows·Linux 실기기 확인 전: 이 환경은 메뉴 구성 함수를 컴파일·clippy·단위 테스트로만 검증했습니다)
- **Context** 처음에는 macOS에만 메뉴 막대를 설치하고 Windows·Linux는 페이지가 그리는 메뉴를 썼는데, macOS에서는 두 메뉴가 겹쳤습니다. 참조 앱은 모든 OS에 네이티브 메뉴를 설치하고 창 안 메뉴는 그리지 않습니다.
- **Decision (`PD-102`, 개정)** (1) **모든 OS에 설치**합니다(`native_menu::install`, cfg 없음). 데스크톱 셸이 있으면 페이지가 `#app`에 `data-native-menu="true"`를 달고 `.studio-menubar`(메뉴 줄)를 숨깁니다. 명령 팔레트 버튼·테마 전환은 그대로이고, `--serve`·브라우저는 페이지 메뉴를 씁니다. 한글 편집기 창은 이전 규칙대로 메뉴 줄 전체가 숨겨집니다. (2) 메뉴는 파일 / 편집 / 보기 / 입력 / 서식 / 쪽 / 표 / 도구 / 도움말이며 macOS는 앞에 앱 메뉴(정보·서비스·숨기기·종료), 도움말 앞에 윈도우 메뉴가 더해집니다. **페이지 메뉴의 모든 항목이 표에 있습니다**(`test-webui-native-menu.mjs`가 `index.html`의 메뉴 항목을 읽어 표의 동작에 모두 있는지 확인합니다): 페이지 메뉴 항목을 누르는 동작(`studio-cmd:<명령>`, `window-new:<종류>`, `theme:<모드>`, `update-check`)이므로 모듈별로 쓸 수 없는 항목은 페이지와 똑같이 아무 일도 하지 않습니다. (3) **표는 하나**입니다: `native_menu.rs`의 `ITEMS`(id·라벨·단축키·페이지 동작)와 `shell-chrome.js`의 `NATIVE_MENU_ACTIONS`가 1:1이고 테스트가 대조합니다. 메뉴를 만드는 함수가 쓰는 id는 표의 id와 정확히 같습니다(Rust 테스트). 기존 항목(새로 만들기·열기·최근 문서·저장·다른 이름으로 저장·닫기·찾기)은 단축키 동작 표의 같은 핸들러를 실행합니다. (4) **라벨은 모든 OS에서 한국어**, 단축키는 `CmdOrCtrl`(OS 관례)입니다. 단축키는 기존 항목과 보기(확대·축소·100%·밝게/어둡게 전환)에만 두고, 진하게·기울임·밑줄·편집 항목은 단축키를 달지 않습니다: 포커스된 편집기(특히 한글 편집기와 입력 칸)가 그 키를 직접 처리해야 하기 때문입니다. (5) 편집 항목: macOS는 시스템 항목(되돌리기·오리기·복사·붙이기·모두 선택)을 씁니다. Windows·Linux는 시스템 되돌리기·다시 실행이 없어(GTK 미지원) 표의 항목(`edit:undo` 등 → 페이지의 같은 명령)을 씁니다. (6) **전달**: 메뉴 id가 표에 있을 때만 `window.eval`로 고정 스크립트(`window.__EASYOFFICE_NATIVE_MENU__("id")`, id는 JSON 문자열)를 **포커스가 있는 창에만** 실행합니다. 이벤트를 쓰지 않아 capability에 `core:event` 권한이 없고, 페이지가 보낸 값은 쓰이지 않으며 경로는 Rust만 다룹니다. 종료는 macOS 시스템 항목(`RunEvent::ExitRequested` → 창마다 저장 확인)입니다. (7) 스모크의 `menu` 단계는 모든 OS에서 이 OS의 모든 표 항목이 설치됐고, 테마·업데이트 확인 항목이 체크 항목이며, 끝내기가 Windows·Linux 메뉴에만 있는지 확인합니다. (8) **끝내기(Windows·Linux)**: 파일 메뉴 끝의 `file:quit`(`CmdOrCtrl+Q`, 참조 앱과 같음)은 페이지 동작이 없는 Rust 전용 항목이라 표(`ITEMS`)에 넣지 않습니다. `app_quit.rs`가 창을 포커스 창 먼저 한 창씩 `close()`해 기존 닫기 흐름(저장 안 된 창은 페이지의 저장/저장 안 함/취소)을 거치고, 창이 사라질 때마다 다음 창을 묻고 마지막 창 뒤에 `app.exit(0)`합니다. 사용자가 취소하거나 저장에 실패하면 페이지가 `end_close_prompt`를 불러 그 창의 닫기 확인을 끝내고(다음 닫기에서 다시 묻도록 `close_prompted` 해제) 종료를 멈춥니다(남은 창은 닫지 않음). 큐 앞 창이 아닌 창이 닫혀도 앞 창에는 다시 `close()`하지 않고, 마지막 창 뒤에 새 창이 있으면 큐를 다시 만듭니다. 이미 저장 확인 중인 창(`close_prompted`)은 끝내기·macOS `Cmd+Q`가 다시 `close()`하지 않고 앞으로만 가져오며(`raise_and_close`), 그 창의 답을 기다립니다(저장·저장 안 함 → 창이 사라져 다음 창, 취소 → `end_close_prompt`로 종료 중단). (9) **체크 표시·비활성**: 테마 세 항목과 업데이트 확인 알림은 체크 항목이고, 쓸 수 없는 항목은 회색입니다. 메뉴 막대는 모든 창이 공유하므로 각 창이 `report_menu_state(theme, disabled, embed)`로 자기 상태를 알리고(테마·모듈·편집기 상태가 바뀔 때), Rust가 **포커스 창**의 상태를 막대에 반영합니다(`menu_state.rs`, 포커스가 옮겨 갈 때와 업데이트 확인을 바꿀 때도). 비활성 규칙은 페이지 메뉴와 같은 한 벌(`studioCommandApplies`)을 표의 `studio-cmd:` 항목에 적용해 페이지가 id 목록으로 보내므로 Rust에 두 번째 표가 없고, 모든 id는 표와 대조합니다. 업데이트 확인 알림의 체크는 Rust의 설정값이며 확인을 하지 않는 실행(스모크·데이터 폴더 없음)에서는 회색입니다. (10) **한글 편집기 창(Windows·Linux)**: 이 창은 편집기가 자기 메뉴 줄(편집·입력·서식·표 등 수백 개 명령)을 가지므로 **그 창의 네이티브 메뉴 막대를 숨깁니다**(`report_menu_state`의 `embed`가 참이면 `hide_menu`, 거짓이면 `show_menu`; 편집기가 뜨지 못하면 막대가 다시 보임). 반대로 편집기 메뉴 줄을 숨기면 편집기 전용 명령 대부분에 닿을 길이 없어 기능 손실이 훨씬 큽니다. 숨긴 막대의 셸 항목 중 편집기 파일 메뉴가 전달하는 8개(새로 만들기·열기·최근·저장·다른 이름·PDF·인쇄·닫기)와 `Ctrl+K`(명령 팔레트: 새 문서 종류·열기·저장·도구)·`Ctrl+Alt+D`(테마)는 그대로 되고, 정보·테마 선택·업데이트 확인 알림은 다른 창(시작 창 등)의 메뉴에서 씁니다. macOS는 앱 전역 메뉴 하나라 변경 없음.
- **Why** 두 메뉴가 겹치지 않게 하고, 모든 OS에서 같은 메뉴 막대를 주면서 명령 경로를 새로 만들지 않고(표 하나, 핸들러 하나) 권한도 넓히지 않기 위함입니다.
- **Trade-offs** 실행을 `eval` 문자열로 넘기므로 호출 대상은 고정 이름 하나이고 id는 정적 표 값뿐입니다. 네이티브 항목은 모듈에 따라 비활성으로 보이지 않고(페이지의 `aria-disabled`가 기준), 누르면 아무 일도 하지 않습니다. 메뉴 상태는 포커스 창 기준이라 포커스가 옮겨 갈 때 한 번 갱신됩니다. 테마는 창마다 저장 값을 따로 읽어(창 사이 자동 동기화는 없음) 막대는 포커스 창의 값을 보여 줍니다. Windows·Linux의 한글 편집기 창에서는 네이티브 막대가 숨어 정보·테마 선택·업데이트 확인 알림이 그 창에 없고, `Ctrl+Q`(끝내기)도 숨긴 막대의 단축키라 Windows에서는 그 창에서 동작하지 않을 수 있습니다(편집기 파일 메뉴의 닫기는 그대로).
- **Alternatives rejected** 참조 앱처럼 `emit_to` + 페이지 `listen`(capability에 이벤트 권한 필요), 모든 창에 emit(포커스 없는 창이 저장/닫기를 실행할 수 있음), 메뉴 항목마다 Rust에서 직접 처리(저장 확인·한글 편집기 흐름 중복), 모든 편집 항목에 단축키(편집기 키 가로채기).
- **Verified by** `cargo test -p easyoffice-desktop native_menu`(id 유일·형식, 단축키 형식·중복 없음, 동작 이름 형식, 표의 id만 스크립트를 만들고 id는 인용됨, 메뉴 함수가 표의 id를 빠짐없이 그대로 씀, 편집 키는 단축키 없음), `node --test scripts/test-webui-native-menu.mjs`(Rust 표와 페이지 목록 일치, 페이지 메뉴 항목 ⊆ 표, 표의 페이지 항목 존재, 데스크톱 셸에서만 페이지 메뉴 숨김, 이벤트 권한 없음), `cargo test -p easyoffice-desktop app_quit menu_state native_menu`(종료 순서·취소·포커스 창 우선, 보고 검증, 체크 상태, 끝내기는 표 밖), `node --test scripts/test-webui-menu-state.mjs`(모듈별 비활성 id가 페이지 규칙과 일치, 테마 변경 보고, 한글 편집기 `embed` 보고, 취소 시 `end_close_prompt`), `cargo clippy -p easyoffice-desktop --all-targets -- -D warnings`.
- **Revisit when** 표·글 상자·그림 메뉴를 넣을 때(한글 편집기의 자체 메뉴와 겹침 확인), 실기기에서 Windows·Linux 한글 편집기 창의 메뉴 숨김·`Ctrl+Q`·체크 표시·회색 항목이 기대대로인지 확인한 뒤(정보·테마·업데이트 확인 알림이 그 창에 필요하면 편집기 메뉴로 전달할지), 실기기에서 편집 항목이 한글 편집기와 충돌할 때.

## 78. 업데이트 알림: 하루 1회 릴리스 조회와 릴리스 페이지 열기, 자동 설치 없음 (PD-103)

- **Status** Implemented (데스크톱 앱 전용; Windows·macOS 실기기와 실제 릴리스 응답 확인 전)
- **Context** 사용자 결정: 업데이트 알림은 참조 앱처럼 넣습니다. 참조 앱은 서명된 업데이터로 내려받아 설치하지만 EasyOffice에는 서명 키가 없어, 새 버전을 알리고 릴리스 페이지로 안내하는 데까지만 합니다.
- **Decision (`PD-103`)** (1) 앱이 시작한 뒤(스모크 실행과 `--serve`는 제외) 백그라운드로 `https://api.github.com/repos/AquilaXk/easyoffice/releases/latest`를 **하루 1회 이하**로 조회합니다. 마지막 확인 시각은 시도 때 먼저 앱 데이터 폴더의 `update-check.json`에 적으므로 네트워크가 실패해도 같은 날 다시 시도하지 않고, 시계가 거꾸로 가면 확인할 때가 된 것으로 봅니다. (2) 요청은 Rust에서만 하고(`ureq` 2.x + `native-tls`, 운영체제 TLS, HTTPS 전용, 리다이렉트 없음, 10초 제한) 사용자·문서 정보는 보내지 않습니다. 사용자 에이전트는 `EasyOffice/<버전>`뿐입니다. 응답은 256KiB로 자르고 JSON에서 `tag_name`·`html_url`·`draft`·`prerelease`만 읽습니다. (3) 버전은 접두사 `v`가 선택인 숫자 3개(`x.y.z`)만 인정하는 바이트 파서(할당 없음, `checked_*`, 부분은 9자리 이하·선행 0 거부)로 읽고, 사전 배포·초안·`-rc` 같은 접미사는 무시하며 현재 버전보다 크면 알립니다. 현재 버전은 컴파일 때 박은 `CARGO_PKG_VERSION`이 아니라 실행 중 앱의 Tauri 설정 버전(`app.package_info().version`, 릴리스 빌드의 `--config` 덮어쓰기가 반영됨)이고, 사용자 에이전트의 `<버전>`도 같은 값입니다(#214: 예전에는 워크스페이스 1.0.0을 읽어 배포된 앱이 항상 새 버전이 있다고 판단함). 비교 함수는 버전 문자열을 인자로 받는 순수 함수라, 0.1.0으로 빌드한 앱은 `v0.1.0`에 알리지 않고 `v0.1.1`·`v0.2.0`에는 알립니다. 릴리스가 하나도 없어 API가 404를 주면 오류 없이 "릴리스 없음"으로 보고 알려 둔 알림을 지우며, 다음 확인은 그대로 하루 뒤입니다. 정식 출시 전 0.x로 재시작했고 이전 v0.1.0~v2.0.0 릴리스·태그는 2026-11-08에 삭제했습니다(QA 결정). (4) 릴리스 페이지 주소는 응답의 `html_url`이 `https://github.com/AquilaXk/easyoffice/releases/`로 시작하고 공백·제어 문자가 없을 때만 받습니다. 저장 기록을 읽을 때도 같은 검사와 현재 버전 비교를 다시 해서, 기록 파일을 고쳐 다른 주소를 넣을 수 없습니다. (5) 페이지 명령은 셋뿐입니다: `get_update_notice`(상태), `set_update_check(enabled)`(켜기/끄기), `open_release_page`(**인수 없음**: 페이지는 주소를 넘기지 않고, Rust가 가진 주소를 기본 브라우저로 엽니다). 명령 목록·capability는 1:1 테스트로 묶여 있습니다(12개). (6) 알림 카드는 참조 앱 `update-notice`를 옮긴 것(`update-notice.js`, `home.css`의 같은 클래스)으로 창 우상단에 "새 버전 x.y.z 사용 가능", "다운로드 페이지 열기", "나중에"를 보입니다. "나중에"와 카드 안에서의 Esc는 그 버전을 이번 실행 동안 숨기고, 더 새 버전이 오면 다시 보입니다. 첫 확인은 창이 뜬 3초 뒤, 두 번째는 15초 뒤(백그라운드 조회가 끝날 시간), 그 밖에 창이 포커스를 받을 때입니다(네트워크 요청 없이 상태만 읽음). (7) 끄기: 도움말 메뉴의 "업데이트 확인 알림"(기본 켬, 데스크톱에서만 보임)이 같은 기록에 저장되며 끄면 요청도 알림도 없습니다. 한글 편집기 창은 편집기 메뉴를 쓰므로 이 항목이 없고, 설정은 다른 창에서 바꿉니다. (8) **스모크용 루프백 훅**: 요청 함수는 `fetch_release(url, https_only, app_version)`이고 실제 확인(`fetch_latest_release(app_version)`)도 같은 함수를 `https_only = true`로 부릅니다. `https_only = true`는 HTTPS 주소만 받습니다. `https_only = false`는 스모크 전용이며 URL을 파싱한 뒤 `http://127.0.0.1:<포트>/…`(호스트가 정확히 `127.0.0.1`, 포트 명시, 사용자 이름·암호 없음)만 허용하고 나머지는 연결 전에 거부합니다(`127.0.0.1.evil.example`, `127.0.0.1:80@evil.example`, `localhost`, 다른 사설 주소 포함). 스모크의 `update` 단계는 `127.0.0.1`에 1회용 가짜 서버(최대 10초 대기, 요청 머리글 8KiB 상한)를 열고 실제 요청을 보내, 요청 줄·`User-Agent`(`EasyOffice/<앱 버전>`, 앱 버전은 `app.package_info().version`)·`Accept`가 실제 확인과 같은지, 응답(`999.0.0`)이 새 버전으로 읽히는지, 훅이 다른 주소를 거부하는지 확인합니다. 응답 읽기 상한(256KiB)은 같습니다. 404는 `Ok(None)`(릴리스 없음, 알림을 지움)이며 스모크 `update` 단계도 가짜 서버의 404 응답으로 확인합니다. (6) 알림 카드는 참조 앱 `update-notice`를 옮긴 것(`update-notice.js`, `home.css`의 같은 클래스)으로 창 우상단에 "새 버전 x.y.z 사용 가능", "다운로드 페이지 열기", "나중에"를 보입니다. "나중에"와 카드 안에서의 Esc는 그 버전을 이번 실행 동안 숨기고, 더 새 버전이 오면 다시 보입니다. 첫 확인은 창이 뜬 3초 뒤, 두 번째는 15초 뒤(백그라운드 조회가 끝날 시간), 그 밖에 창이 포커스를 받을 때입니다(네트워크 요청 없이 상태만 읽음). (7) 끄기: 도움말 메뉴의 "업데이트 확인 알림"(기본 켬, 데스크톱에서만 보임)이 같은 기록에 저장되며 끄면 요청도 알림도 없습니다. 한글 편집기 창은 편집기 메뉴를 쓰므로 이 항목이 없고, 설정은 다른 창에서 바꿉니다.
- **Why** 새 버전을 모르고 오래된 버전을 쓰지 않게 하면서, 서명 없는 자동 설치의 위험과 사용자 정보 전송을 피하기 위함입니다.
- **Trade-offs** 설치는 사용자가 릴리스 페이지에서 직접 합니다. GitHub 속도 제한이나 오프라인이면 조용히 알림 없이 넘어갑니다(오류는 콘솔에만).
- **Alternatives rejected** 서명 없는 자동 다운로드·설치(변조 위험), rustls/ring 계열 HTTP 클라이언트(번들 TLS와 다른 라이선스), 페이지가 주소를 넘기는 열기 명령(임의 주소 열기).
- **Verified by** `cargo test -p easyoffice-desktop update_check`(버전 파서·비교, 앱 버전 문자열 기준 판단, URL 접두사, JSON 추출·상한·잘못된 입력, 하루 1회 기록, 실패 재시도 없음, 끄기, 스모크·데이터 폴더 없음, 기록 변조), `cargo test -p easyoffice-desktop smoke`(가짜 서버 왕복·404, 요청 줄·`User-Agent`·`Accept` 검사, 단계 장부·표식), 루프백 훅의 주소 거부(`update_check`), `node --test scripts/test-webui-update-notice.mjs`(카드 DOM·참조 앱 클래스·Esc·나중에·주소 없는 열기·끄기 토글·토큰 전용), 명령·capability 1:1 테스트.
- **Revisit when** 서명 키가 생겨 자동 설치를 넣을 때, 릴리스에 사전 배포 채널을 둘 때.

## 79. 한글 창 인쇄: 쪽을 나눠 받아 숨은 루트에서 창 인쇄, 취소 가능 (PD-104)

- **Status** Implemented (브라우저 헤드리스에서 파일 메뉴 → 숨은 루트·@page 규칙 생성까지 확인, 데스크톱 실제 인쇄 대화상자는 Windows·macOS 수동 확인 전)
- **Context** 한글 편집기는 `sandbox` iframe(팝업 거부, PD-083) 안에서 돌고, 편집기 안의 인쇄 명령은 포함 모드에서 숨겨져 있어(`EMBED_HIDDEN_FILE_COMMAND_IDS`) 한글 창에는 인쇄가 없었습니다. 참조 앱은 모든 쪽 SVG를 숨은 루트에 쌓고 쪽 크기로 `@page`를 만들어 창을 인쇄합니다. 같은 방식을 가져오되 큰 문서와 취소를 더 안전하게 다룹니다.
- **Decision (`PD-104`)** (1) 파일 메뉴에 `인쇄…`(`print` 셸 명령, 셸 허용 목록 7개)와 한글 모듈의 `Ctrl+P`(편집기에서 전달, 브라우저 예약 키라 데스크톱 셸에서만)를 둡니다. 편집기는 이름만 전달하고 셸이 다시 검사합니다. (2) 셸의 `hwp-print.js`가 임베드 RPC `getPrintPages({from,count})`(capability `print-pages-v1`, `count` 1..=10)로 쪽을 **10쪽씩 순서대로** 받습니다. 편집기는 한 묶음을 인쇄 프로필·이식 가능 지표(`withPortableMetrics`) 한 번에 그려 SVG와 용지 크기(mm, 표준 용지로 맞춤)를 돌려줍니다. 문서 전체 SVG를 한꺼번에 만들거나 보내지 않습니다. (3) 셸은 각 쪽 SVG를 정리(script·foreignObject·iframe 등 제거, `on*`·`javascript:`·외부 참조 제거, 쪽마다 id 이름 공간 분리)해 `#eo-print-root`에 더합니다. 쪽에 쌓는 SVG 글자 수는 96Mi 글자로 제한하고 넘으면 안내와 함께 중단합니다. 용지 크기 값은 10..=2000mm만 받습니다. (4) 용지 크기마다 `@page` 이름 규칙과 쪽 상자 규칙을 하나씩, 첫 크기는 이름 없는 기본 `@page`로도 더합니다(이름 있는 쪽을 모르는 엔진 대비). 규칙은 생성형 스타일시트(CSSOM)로만 더하며 style 요소·속성은 쓰지 않습니다. 쪽 상자는 용지보다 0.1mm 낮게 잡아 빈 쪽을 막습니다. 인쇄용 정적 규칙은 `shell.css`의 `@media print`입니다. (5) 진행: "인쇄 준비 중… (i/n)" 카드와 **취소** 단추(Esc도 같음). 취소하면 가져오기를 멈추고 숨은 루트와 스타일시트를 지우며, 같은 시각에 인쇄가 이미 준비 중이면 새 요청은 안내만 합니다. (6) 준비가 끝나면 창의 `print()`를 부르고 `afterprint`(없으면 5분 뒤)에 지웁니다. 데스크톱 웹뷰는 `print()`를 모든 플랫폼에서 지원하므로 **새 셸 명령·capability는 없습니다**(capability 파일에 인쇄 항목이 없다는 것을 테스트로 고정). `print.html`·`print.css`·`print-pages.ts`는 업스트림 포크 그대로 두며(Sonar 제외 대상) 포함 모드에서는 쓰이지 않습니다.
- **Why** 한글 창에서 문서를 종이나 PDF로 낼 수 있어야 하고, 큰 문서에서도 메모리를 제한하며 취소할 수 있어야 하기 때문입니다.
- **Trade-offs** 쪽 DOM은 인쇄가 끝날 때까지 한 번에 창에 있어야 해서 아주 큰 문서는 상한에서 중단됩니다. 쪽마다 크기가 달라도 `@page` 이름 규칙은 엔진에 따라 무시될 수 있습니다(그때는 첫 용지 크기).
- **Alternatives rejected** 편집기 안의 `file:print`(팝업 거부로 불가), iframe 인쇄(웹뷰별 동작 차이), 전체 쪽을 한 번에 받기(메모리·메시지 크기), 새 셸 `print_webview` 명령(웹 `print()`로 충분).
- **Verified by** `node --test scripts/test-webui-hwp-print.mjs`(10쪽 배치 순서, 크기별 규칙, SVG 정리·id 분리, 취소·Esc·중복 요청, 지원 없음·빈 문서·상한·잘못된 쪽, 인쇄 실패·afterprint 정리, 단축키·capability), `node --test scripts/test-hwp-studio.mjs`(배치 mm·끝 처리·잘못된 범위, 라우터·capability), `scripts/test-webui-hwp-shell.mjs`(셸 명령 7개), 헤드리스 브라우저 확인(메뉴 → 숨은 루트).
- **Revisit when** 쪽 범위·매수·양면 같은 인쇄 옵션을 앱에서 받을 때, 웹뷰가 이름 있는 `@page`를 지원하지 않는 사례가 확인될 때.

## 80. 한글 창의 "도구 상자 라벨 표시" 토글, 다른 창에는 넣지 않음 (PD-105)

- **Status** Implemented (한글 편집기 오버레이)
- **Context** 참조 앱은 보기 메뉴에 "도구 상자 라벨 표시"를 두고 아이콘 도구 상자의 글자 라벨과 서식 줄의 리본 라벨을 숨깁니다. 한글 편집기(rhwp-studio)의 도구 상자는 아이콘 아래에 `.tb-label`을 갖지만 이 토글은 참조 앱 쪽 기능이라 편집기에는 없었습니다. 문서·시트·슬라이드·PDF 창(DOM 메뉴)의 도구 상자는 처음부터 아이콘만 있어 숨길 라벨이 없습니다.
- **Decision (`PD-105`)** (1) 편집기 오버레이의 보기 메뉴에 체크 항목 `view:toolbar-labels`를 넣고, 명령은 편집기 안에서 처리합니다(셸 RPC·셸 명령 추가 없음). (2) `<html data-toolbar-labels="shown|hidden">`가 상태이며 `hidden`이면 `#icon-toolbar .tb-label`, `#style-bar .sb-ribbon-label`을 숨기고 아이콘 도구 상자 높이를 32px로 합니다(참조 앱 CSS 그대로, 색 없음). (3) 선택은 창 표시 선호이므로 `localStorage`의 `toolbar-labels`에 저장합니다. 값이 정확히 `hidden`일 때만 숨기고, 읽기·쓰기가 실패하면 기본값(표시)으로 두며 쓰기 실패여도 이번 창에서는 바뀝니다. 다른 창의 `storage` 이벤트로 동기화합니다. (4) DOM 메뉴 창에는 항목을 넣지 않습니다(숨길 라벨이 없어 눌러도 아무 일도 없는 항목이 되기 때문). macOS 네이티브 메뉴도 같은 이유로 해당 없음이며, 이 항목은 편집기 안에만 있습니다. 나중에 DOM 도구 상자에 라벨을 도입하면 그때 함께 넣습니다.
- **Why** 한글 창에서 참조 앱과 같은 화면 밀도 선택을 주되, 라벨이 없는 창에 의미 없는 항목을 만들지 않기 위함입니다.
- **Trade-offs** 저장소가 창(편집기 iframe)의 출처에 묶여 한글 창들 사이에서만 공유됩니다.
- **Alternatives rejected** 셸이 선호를 저장해 편집기에 밀어 넣기(RPC 추가), DOM 창에도 항목 추가(동작 없음).
- **Verified by** `node --test scripts/test-hwp-studio.mjs`(토글 왕복과 저장값, 저장 실패 시 기본값, 메뉴·명령·CSS·매니페스트 일치), `node scripts/build-hwp-studio.mjs`(번들 빌드와 검사).
- **Revisit when** DOM 창 도구 상자에 글자 라벨을 넣을 때.

## 81. 시작 창 최근 문서 카드의 미리보기: 파일에 든 그림만 읽어 보여 줌 (PD-106)

- **Status** Implemented (합성 컨테이너와 페이지 가짜 환경 테스트; 실제 문서·Windows·macOS 화면 확인 전)
- **Context** 참조 앱은 Rust 네이티브 rhwp로 첫 쪽 SVG를 그려 카드에 보입니다. EasyOffice의 셸에는 쪽을 그리는 엔진이 없고(한글 WASM은 편집기 안에만 있음), 문서·시트·슬라이드·PDF를 그리는 경로도 없습니다. 페이지에 파일 바이트를 넘기는 것은 PD-088의 경계(경로·내용은 Rust 안)를 넓힙니다. 한글(`PrvImage`)·HWPX(`Preview/PrvImage.*`)·OOXML(`docProps/thumbnail.*`)·ODF(`Thumbnails/thumbnail.png`) 파일은 만든 프로그램이 넣어 둔 작은 미리보기 그림을 이미 갖고 있습니다.
- **Decision (`PD-106`)** (1) 새 명령 `get_recent_preview(id)`는 **불투명 id만** 받습니다(형식 `[0-9a-f]{16}` 검사, 아니면 오류). 최근 목록에 없으면 `null`입니다. 경로와 문서 바이트는 페이지로 가지 않고, 돌려주는 것은 그림의 종류(`image/png|jpeg|gif|bmp`)와 Base64뿐입니다. (2) `preview/`가 파일을 **시크로만** 읽습니다: OLE2는 머리글·FAT 목록·디렉터리·`PrvImage` 스트림(작으면 미니 스트림)만, ZIP은 끝 레코드·중앙 디렉터리·그 항목 하나만 읽으며(저장 또는 deflate, 암호·ZIP64는 무시) 256MiB 문서도 수십 KB만 읽습니다. (3) 상한: 그림 256KiB(압축·풀린 크기 모두, deflate는 상한에서 중단), 디렉터리 항목 4096개, FAT 구역 8192개, DIFAT 256개, 사슬 65536칸, ZIP 항목 8192개·중앙 디렉터리 4MiB, 모든 오프셋·크기 계산은 `checked_*`, 사슬 순환은 칸 수 상한으로 끊습니다. (4) 종류는 **앞 바이트(매직)** 로만 정합니다: PNG·JPEG·GIF·BMP만 받고 WMF·EMF·그 밖은 버립니다(이름의 확장자는 후보 선택에만 씀). (5) 캐시는 id별 1개, 파일의 수정 시각과 크기가 같을 때만 유효하고 최대 10개이며, 그림이 없다는 결과도 기억해 같은 파일을 다시 읽지 않습니다. (6) 페이지는 카드가 그려진 뒤(`setTimeout 0`) **한 장씩 차례로** 요청하고, 응답의 `createImageBitmap(new Blob([bytes], {type}))` 결과를 카드 안 `<canvas>`에 그립니다. URL을 만들지 않으므로 페이지 CSP(`default-src 'self'`)의 `img-src`와 무관하며 CSP는 바꾸지 않습니다. Base64는 256KiB 상한을 넘으면 디코드 전에 버립니다. 요청 실패·그림 없음·디코드 실패·`createImageBitmap` 미지원이면 그 카드는 스켈레톤을 유지하고 다른 카드는 계속 진행합니다. 목록을 다시 그리면 이전 실행은 멈춥니다. 개발용 `--serve`에서는 요청하지 않습니다. (7) 명령은 앱 명령 목록과 capability에 1:1로 추가했습니다(11개).
- **Why** 엔진 무의존과 "경로·내용은 Rust 안" 원칙을 지키면서 카드에 실제 모습을 보이기 위함이며, 한 파일의 그림 하나만 읽어 시작 창 비용이 거의 들지 않습니다.
- **Trade-offs** 그림이 없는 파일(다른 프로그램이 만든 것, 이 앱이 저장한 파일, PDF)은 스켈레톤으로 남고, 그림은 저해상도이며 문서와 어긋날 수 있습니다. 그림은 문서 내용의 일부이므로 시작 창 화면과 메모리 캐시에만 있고 디스크에 쓰지 않습니다.
- **Alternatives rejected** 페이지에서 한글 WASM으로 첫 쪽을 그리기(바이트가 페이지로 가고 11.8MB 코어를 시작 창에서 불러옴), 엔진에 쪽 그리기를 새로 만들기(규모), `data:`/`blob:` 이미지(CSP를 풀어야 함).
- **Verified by** `cargo test -p easyoffice-desktop preview`(합성 OLE2 미니·일반 스트림, ZIP 저장·deflate, 매직·확장자·상한·압축 폭탄, 잘린·순환·잘못된 FAT·ZIP64, id 형식, 캐시 키·10개·미적중 기억, Base64 응답), 앱 명령·capability 1:1 테스트, `node --test scripts/test-webui-home-preview.mjs`(순차 요청·캔버스 교체, 실패별 스켈레톤 유지, 큰 응답 거부, 디코더·명령 없음, 새 목록의 취소, CSS 토큰).
- **Revisit when** 그림 없는 파일이 많아 실제 쪽 미리보기가 필요할 때(한글만 편집기 코어로 그리는 안을 다시 검토).

## 82. PR 커밋 메시지 가드와 웹 UI 테스트 전체 실행 (PD-107)

- **개정 (2026-11-08)**: PR을 쓰지 않으므로 같은 가드가 `main` push의 커밋 범위(`before..sha`)를 검사합니다. PR 제목 검사는 없앴습니다.

- **Status** Implemented
- **Context** 스쿼시 머지는 PR 커밋 메시지를 main에 그대로 옮깁니다. 위생 가드는 추적 파일만 검사해서, 금지된 도구 이름·외부 상용 브랜드·귀속 표기(공동 작성자 트레일러 등)가 커밋 메시지로 main에 들어가는 것을 막지 못했습니다(이미 들어간 과거 커밋 36개). 또 CI가 웹 UI 테스트를 파일 이름 목록으로 실행해, 그 뒤 추가된 테스트 11개(슬라이드·PDF/변환기 화면, 인쇄, 업데이트 알림, 홈 미리보기, macOS 메뉴 등)가 CI에서 돌지 않았습니다.
- **Decision (`PD-107`)**
  1. 금지 목록은 `scripts/prohibited-terms.mjs` 한 곳에 두고 위생 가드(추적 파일)와 새 커밋 메시지 가드가 함께 씁니다. 세션 귀속 표기(`…-Session:`)를 목록에 더했습니다.
  2. `scripts/check-commit-messages.mjs <base> <head>`는 PR의 `base..head` 커밋 메시지와 PR 제목(`PR_TITLE` 환경 변수, 스쿼시 머지의 기본 제목)을 검사하고, 찾은 위치만 보고합니다(금지어를 다시 출력하지 않음). CI `checks` 작업이 PR에서만 실행합니다(제목은 환경 변수로 넘겨 명령 주입 없음). PR 본문은 검사하지 않습니다: 저장소 이력에 남지 않고, 호스팅 연동이 본문 끝에 자체 표기를 덧붙일 수 있기 때문입니다(머지 메시지는 직접 작성).
  3. 과거 main 이력은 고치지 않습니다. 이력을 다시 쓰면 모든 커밋 SHA가 바뀌어 기존 태그(v1.8.0~v2.0.0)·릴리스·PR 링크가 옛 이력을 가리키게 되기 때문입니다(사용자 결정: 앞으로만 차단).
  4. CI와 릴리스 워크플로는 웹 UI 테스트를 `"scripts/test-webui-*.mjs"` 패턴으로 실행합니다(Node 22의 `--test` 패턴, Windows PowerShell에서도 같은 동작). 새 테스트 파일은 목록 수정 없이 자동으로 포함됩니다.
- **Why** 저장소 규칙(도구 이름·브랜드·귀속 표기 금지)을 파일뿐 아니라 main에 들어가는 메시지까지 같은 목록으로 강제하고, 작성한 테스트가 실제로 CI에서 돌게 하기 위함입니다.
- **Trade-offs** 과거 커밋 36개의 표기는 남습니다. PR 제목에 금지어를 인용해야 하는 경우도 막힙니다.
- **Verified by** `node --test scripts/test-commit-messages.mjs`(깨끗한 메시지 통과, 각 커밋·제목의 표기 보고, 공유 목록 전체 대소문자 무시 검사, git log 레코드 파싱·SHA 검사), 과거 표기 커밋 범위에서 실제로 실패하는지 수동 확인, `node --test "scripts/test-webui-*.mjs"`(319).
- **Revisit when** main 이력을 정리할 필요가 생길 때(예: 저장소 이전), 금지 목록이 바뀔 때.

## 83. 앱 아이콘을 rhwp 강조색으로 (PD-108)

- **Status** Implemented
- **Context** 앱 아이콘과 파비콘이 폐기된 라벤더(#5C6BC0, PD-098 이전 디자인 시스템)로 남아 있었습니다.
- **Decision (`PD-108`)** 사용자 선택 L-3안입니다. 문서 모양(접힌 모서리, 줄 표시 세 개)은 유지하고 글자는 넣지 않습니다. 바탕은 rhwp 강조색의 세로 그러데이션(#6182D6 → #3E62BB), 종이는 흰색, 접힌 모서리는 #D6DFF5, 줄은 #335095(긴 줄 두 개)와 #6182D6(짧은 줄)이고 줄 끝은 둥글게 합니다. 바탕 사각형은 참조 셸(참조 앱) 아이콘과 같은 비율입니다(1024 기준 64 안쪽, 896 크기, 모서리 반지름 214). 원본은 `crates/easyoffice-app/ui/favicon.svg`(1024 단위) 하나이고, 데스크톱 아이콘(PNG 32·128·256·512, Windows ICO, macOS ICNS)은 `node scripts/generate-desktop-icons.mjs`가 같은 좌표로 도형을 해석적으로 그려 결정론적으로 만듭니다(`--check`로 최신 여부 검사).
- **Why** 앱 안의 색(rhwp 토큰)과 아이콘을 맞추고, 기존 사용자에게 익숙한 문서 모양을 지키기 위함입니다. 글자(E·Easy)는 작은 크기에서 읽히지 않고 문서 모양과 어울리지 않아 넣지 않습니다.
- **Trade-offs** 참조 앱의 단색 자모 아이콘과 달리 문서 모양을 씁니다.
- **Verified by** `node scripts/generate-desktop-icons.mjs --check`, `cargo test -p easyoffice-desktop`(ICNS 포함 검사), 위생 가드, 16·32·64px 육안 확인.
- **Revisit when** 브랜드 아이콘을 새로 디자인할 때.

## 84. 저장 안정성: 내용 해시 지문, 고유 임시 파일, 저장 전 구조 검증 (PD-109, #227)

- **Status** Implemented
- **Context** 외부 변경 감지 지문이 길이와 수정 시각만 봐서, 같은 크기·같은 시각으로 바뀐 파일을 놓쳤습니다. 임시 파일 이름이 `<원본>.easyoffice-saving`으로 고정이라 같은 폴더에 동시에 저장하면 서로의 임시 파일을 덮었습니다. 페이지가 보낸 바이트는 검증 없이 원본을 대체했습니다. 참조 구현(MIT)은 지문에 내용 해시를 더하고, 고유한 임시 파일을 쓰며, 저장을 확정하기 전에 한글 코어로 다시 읽어 봅니다.
- **Decision (`PD-109`)**
  1. **지문 = 길이 + 수정 시각 + 내용 해시(64비트)**. 해시는 표준 라이브러리의 SipHash-1-3(`DefaultHasher`)이고 64KiB 스택 버퍼로 읽어 할당이 없으며, 읽는 양은 열기 상한(256MiB)+1바이트까지입니다(그보다 크면 길이가 이미 다릅니다). 지문은 프로세스 안에만 있고 저장하지 않으므로 알고리즘이 표준에서 바뀌어도 상관없습니다. 후보 비교(Cargo.lock 확인, 새 의존성 없음): FNV-1a는 64MiB에 약 68ms(바이트 단위 곱셈 의존), 같은 크레이트 그래프의 `siphasher`와 표준 `DefaultHasher`는 같은 알고리즘이고 약 12ms(파일 읽기 포함 약 16ms, 읽기만 약 4ms), `rustc-hash`는 약 3ms지만 내용 무결성용으로는 섞임이 약해 쓰지 않았습니다. 그래서 의존성을 늘리지 않고 SipHash-1-3을 택했습니다. 계산은 저장 직전의 외부 변경 확인, 파일을 열 때(읽은 바이트로 계산해 파일을 두 번 읽지 않음), 저장 직후(쓴 바이트로 계산, 읽지 않음)에만 하고 포커스나 주기 점검에는 쓰지 않습니다.
  2. **지문의 출처**: 열 때는 읽기 전에 얻은 메타데이터와 읽은 바이트의 해시, 저장 때는 이름 바꾸기 직후의 메타데이터와 쓴 바이트의 해시입니다. 그 사이에 다른 프로그램이 바꾸면 다음 저장이 "변경됨"으로 묻습니다. 저장 직후 메타데이터를 못 읽으면 지문을 갱신하지 않아 역시 다음 저장이 묻습니다.
  3. **고유 임시 파일**: 같은 폴더에 `.<프로세스 id>-<카운터>-<시각>.easyoffice-saving`을 `create_new`로 만듭니다(이미 있으면 다음 이름, 16번까지). 쓰기, `sync_all`, 이름 바꾸기 순서이고 실패하면 임시 파일을 지우며 원본은 그대로입니다. 이름 바꾸기는 Windows에서도 기존 파일을 덮어씁니다(`MoveFileExW`의 `MOVEFILE_REPLACE_EXISTING`). `tempfile` 크레이트(`native-tls`가 끌어오는 간접 의존성)는 Unix에서 0600 파일을 만들어 교체할 때마다 문서 권한을 좁히므로 쓰지 않았고, 새 파일은 프로세스의 보통 권한을 따르고, 덮어쓰는 파일은 임시 파일을 만들 때 원래 권한으로 열어(더 넓은 권한인 순간이 없음) 쓴 뒤 원래 비트를 다시 맞춥니다(Unix, 일부 볼륨의 `chmod` 거부는 무시).
  4. **저장 전 검증(Fail-Closed)**: `SaveRequest`는 `save_verify::verify_document_bytes`를 통과한 바이트로만 만들어지므로 대화상자와 쓰기 앞에서 막힙니다. 확장자는 `DocumentFormat`에 있는 쓰기 가능한 형식이어야 하고(모르는 확장자와 `.pages`는 거부), `FormatDetector::detect`가 확장자의 서명(텍스트는 UTF-8·NUL 없음)을 확인합니다. ZIP 형식(OOXML·ODF·HWPX)은 `ZipArchiveCodec::unpack`의 전체 걷기(중앙 디렉터리, 로컬 헤더, CRC, 항목 수·크기·경로 상한)와 형식의 주 구성 요소(`word/document.xml`, `xl/workbook.xml`, `ppt/presentation.xml`, ODF `content.xml`, HWPX `Contents/` 아래 항목)를 요구합니다. OLE2 형식은 `Ole2ContainerCodec::unpack`의 전체 걷기를, 한글(HWP·HWT)은 추가로 `HWP Document File`로 시작하는 `FileHeader` 스트림을 요구합니다(암호 플래그는 보지 않음: 암호를 푼 문서의 저장을 막지 않기 위해). PDF는 끝 1KiB 안의 `startxref`와 `%%EOF`입니다. 실패하면 화면에는 내부 용어 없이 "문서 형식을 확인하지 못해 저장하지 않았습니다. 기존 파일은 그대로 두었습니다."만 보이고(구조 손상과 정책 거부를 구분하지 않음) 엔진의 원인은 표준 오류 출력에만 남깁니다. 저장 명령은 디코딩과 검증을 `spawn_blocking`에서 합니다.
  5. **기준은 "다음에 그 파일을 여는 쪽이 받아들이는가"** 입니다. 참조 앱은 저장을 확정하기 전에 다음에 그 파일을 열 코어로 바이트를 다시 읽어 열리면 저장하고(`apps/desktop/src-tauri/src/state.rs` 189-215), 압축률 같은 독자 기준은 두지 않으며, 형식과 확장자가 어긋난 것(HWPX 경로에 HWP 바이트)만 따로 거절합니다. 그래서 (a) 엔진이 여는 형식(OOXML·ODF·OLE2 문서·시트·슬라이드, PDF, 텍스트)은 열기 경로와 같은 엔진 파서·같은 기본 정책으로 검사하고 열기보다 엄격한 기준을 만들지 않습니다(엔진의 ZIP 작성기는 압축률이 90배를 넘는 항목을 저장 항목으로 두므로 엔진이 쓴 파일은 이 정책에 걸리지 않음). (b) 한글(HWP·HWPX·HWT·HWTX)을 다음에 여는 쪽은 편집기의 rhwp이고, 페이지가 모든 한글 저장 경로에서 내보낸 바이트를 rhwp `verifyBytes`로 다시 읽은 뒤에만 셸에 보냅니다(`hwp-session.js`의 `exportVerified`, 저장·다른 이름으로 저장 모두 `doSave`를 거침). 셸에는 rhwp가 없어 컨테이너 구조(ZIP 전체 걷기, OLE2 전체 걷기와 `FileHeader` 서명), 항목 수·항목별·합계 크기 상한, 형식·확장자 일치만 보고 **압축률은 보지 않습니다**: 편집기가 쓰는 정상 HWPX는 단색 그림 하나로 1025배까지 압축되기 때문입니다(3000x2000 24비트 흰색 BMP를 넣은 빈 문서의 `exportHwpx`가 22,831바이트, 열기 기본 정책은 이를 압축 폭탄으로 거부함). 셸 쪽 한글 검사가 막는 것은 잘림, 다른 종류의 파일, 서명 불일치, 부풀린 선언 크기이고 레코드 수준 손상은 rhwp 재파싱 몫입니다. 엔진의 HWP 5.0 해석기는 텍스트 추출용이라 편집기가 쓴 모든 레코드를 받는다는 보장이 없어 셸 검증에는 쓰지 않았습니다.
- **Why** 저장은 사용자의 문서를 대체하는 유일한 동작이라, 외부 변경을 놓치거나 깨진 바이트로 원본을 덮는 일은 데이터 손실입니다. 해시 비용은 64MiB에 십여 ms로 저장 흐름에서 무시할 수 있습니다.
- **Trade-offs** 열기가 받지 않는 파일(항목 하나가 풀린 128MiB 초과, 합계 512MiB 초과, 항목 4,096개 초과, 엔진 형식의 압축률 100배 초과)은 저장도 거부됩니다. 열 수 없는 파일을 저장하지 않는다는 뜻이며, 한글은 압축률만 예외입니다. 한글 레코드 수준의 손상은 셸에서 잡지 못합니다(페이지의 rhwp 재파싱에 의존). 지문이 같은 내용이라도 수정 시각만 바뀌면(동기화 도구 등) 여전히 "변경됨"으로 묻습니다(기존 동작). 메모리: 저장 요청의 바이트(최대 256MiB)에 더해, ZIP 검증은 `unpack`이 모든 항목을 풀어 두므로 풀린 합계(상한 512MiB)까지, OLE2 검증은 스트림 복사본(문서 크기 정도)을 일시적으로 더 씁니다. 검증은 `spawn_blocking`에서 하고 끝나면 해제합니다.
- **Verified by** `cargo test -p easyoffice-desktop`(`tracked_files`: 같은 길이·같은 수정 시각의 내용 변경 감지, 빈 파일·버퍼 경계·짧게 끊어 읽기에서 조각 해시 = 통째 해시; `save_file`: 고유 임시 이름 200개, 동시 저장 8스레드, 늦은 이름 바꾸기 실패에서 원본·임시 파일, 권한 유지, 지문 = 디스크; `save_verify`: 엔진이 쓰는 20개 형식 전부 통과, 다른 종류 패키지, 잘린 컨테이너, 거짓 서명, 부풀린 ZIP·OLE2 선언 크기, 머리 스트림 없는 한글, 압축률 1000배 넘는 한글 패키지 통과와 같은 패키지의 엔진 형식 거부, 반복이 많은 엔진 산출물 7종 통과, 한글 패키지의 크기 상한 유지; `save_file`: 임시 파일이 원래 권한으로 만들어짐), 실제 rhwp WASM 산출물(위 22,831바이트 HWPX)을 직접 통과시켜 확인.
- **Revisit when** 셸이 rhwp 코어를 갖게 될 때(한글 문서 재파싱으로 승격), 정상 문서가 검증에 걸린다는 보고가 있을 때.
