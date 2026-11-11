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

번호 규칙: 절 제목은 `## PD-NNN. 제목`이고 PD 번호가 유일한 식별자입니다(절 번호는 없음). 새 PD는 마지막 번호 다음 번호로 문서 끝에 추가하며, 부여한 PD 번호는 바꾸지 않습니다. 아래 `초기 기록`(1~41)은 PD 번호 부여 이전의 Swift 시기 항목으로 제목 번호를 그대로 둡니다.

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

## 초기 기록 (PD-067 이전, Swift 시기)

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
- **Superseded (부분)**: 1번의 `DocumentDomainEngine`·`SlideDomainEngine`·`PdfDomainEngine`과 표·슬라이드·PDF 도메인 모델은 제품 런타임에서 호출되지 않아 제거했습니다. `RibbonLayoutStyle`·`ContextualObjectToolKind`는 유지합니다.
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

## PD-067. 적대적 아키텍처 리뷰(Red-Team) 기반 모노레포 정합성·파서 무결성·CI 경량화

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

## PD-068. Windows(WebView2)·macOS(AppKit/WKWebView) 실런타임 구동 감사 기반 플랫폼 단축키·PDF 회전 레이아웃·크로스스택 122종 수식 카탈로그 정합성

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

## PD-069. 리본 서식 상태·실행 취소 스택의 클라이언트 세션별·모듈별 분리 (#106)

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

## PD-070. 시트 TEXT 서식 코드·대용량 통합 문서 재계산·없는 시트 `#REF!`·순환 참조 표준 오류값 정합성 (#107)

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

## PD-071. 출력 컨테이너 필수 파트·스키마 정합화, 표준 한글 CID 글꼴 참조 및 DOC/PPT 쓰기 호환성 제한 표시 (#105)

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

## PD-072. WebUI 작업 유실 방지(자동 저장·이탈 경고)·단축키 라벨 1:1 정합성 및 모듈별 도구모음 컨텍스트 (#103, #108)

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

## PD-073. 저장 서식·표 위치·표 계산값·다중 시트 보존, 저장 실패 표시 및 문서 제목 파일명 (#104)

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

## PD-074. Windows 설치형 데스크톱 앱: WebView2 셸·프로세스 내 IPC·NSIS 설치 프로그램 (#119)

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

## PD-075. WebUI 파일 열기·가져오기와 실제 원본 파일 기반 변환 (#111)

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

## PD-076. rhwp upstream 경계: 읽기 전용 서브모듈·위생 가드 제외·라이선스 고지 (#128)
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

## PD-077. 문서 모듈 글자 단위 런 모델: 선택 범위 글자 크기·글꼴·색·형광펜·굵게·기울임·밑줄·취소선 (#134)

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

## PD-078. 시트 편집 기본기: 준비/편집 모드, 범위 선택·클립보드, 가상화 격자, 숫자 표시 (#123)

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

## PD-079. ZIP 기반 출력 DEFLATE 압축·변환 응답 단일 base64·실제 경로 벤치마크 (#124)
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

## PD-080. 슬라이드 발표자 모드·슬라이드 관리·실시간 썸네일·덱 화면 비율 (#125)

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

## PD-081. rhwp WASM 벤더링·hwp-studio 오버레이 빌드·OFL 글꼴 번들 (#129)
- **Context** rhwp 통합 2단계입니다(설계 `docs/rhwp-integration-design.md` §3.3~3.5, §4, §9, 작업 2). 1단계(PD-076)가 upstream을 고정했고, 이 단계는 편집기 엔진(WASM)과 임베드 전용 편집기 번들, 번들 가능한 글꼴을 만듭니다. 앱 런타임은 바뀌지 않습니다(UI·IPC 15종·Tauri 셸 불변, 임베딩은 3단계).
- **Decision (`PD-081`)**
  1. **WASM은 미리 빌드해 커밋하고 CI는 빌드하지 않습니다.** `vendor/rhwp-core/`에 `rhwp_bg.wasm`·`rhwp.js`·`rhwp.d.ts`·`rhwp_bg.wasm.d.ts`·`package.json`·`LICENSE`와 `PROVENANCE.json`(rhwp 커밋·버전, 툴체인 1.93.1, wasm-pack 0.14.0, 빌드 명령·호스트, 파일별 크기·SHA-256)를 둡니다. `scripts/verify-rhwp-core.mjs`(Node 내장 모듈만)가 provenance와 `config/rhwp-upstream.json`(`wasmPackVersion` 추가) 일치, 파일 존재·크기·해시, 미등재 파일 부재를 fail-closed로 검사하고 `verify-rhwp-upstream.mjs`·위생 가드에서 호출됩니다. 갱신은 `scripts/vendor-rhwp-core.mjs`가 wasm-pack 출력 폴더에서 복사하고 provenance를 다시 쓰는 방식뿐입니다. `.gitattributes`가 벤더 파일과 글꼴의 줄바꿈 변환을 막아 Windows 체크아웃에서도 해시가 같습니다.
  2. **빌드 명령(로컬 1회)**: `rustup toolchain install 1.93.1 --target wasm32-unknown-unknown --profile minimal`, wasm-pack 0.14.0은 공식 릴리스 바이너리를 받아 SHA-256으로 확인(`d484c8e8…c721f1`), `third_party/rhwp`에서 `wasm-pack build --target web --out-dir <절대 경로> --release`(기본 기능, `CARGO_TARGET_DIR`는 서브모듈 밖). rhwp 워크스페이스가 `tools/*`·`bindings/Native` 멤버를 선언하므로 sparse 목록에 그 매니페스트와 `saved/blank2010.hwp`(`include_bytes!`)를 추가했습니다. `rhwp.js`·`rhwp.d.ts`는 참조 앱이 같은 커밋에서 만든 파일과 바이트 단위로 같고, `.wasm`은 빌드 호스트(Windows·MSVC 호스트 툴체인)에 따라 18,242바이트 더 큽니다(11,821,975 대 11,803,733). 완전 재현은 보장하지 않으므로 벤더 갱신 PR의 원천은 이 provenance가 기록한 로컬 빌드입니다(설계 Q10).
  3. **편집기 오버레이 `apps/hwp-studio`** (참조 앱 `studio-host` 패턴): npm + 커밋된 `package-lock.json` + `npm ci --ignore-scripts`, Vite 설정이 업스트림을 `@`/`@upstream`으로 별칭하고 `@wasm/rhwp.js`를 벤더 WASM으로 지정합니다. `config/rhwp-studio-overrides.json`이 모든 오버라이드(`strategy`·`reason` 필수)와 upstream 원본 SHA-256(줄바꿈 정규화)을 기록하고, `scripts/hwp-studio-overrides.mjs`가 오버레이 파일과 목록의 1:1 대응·원본 해시를 검사합니다(upstream이 바뀌면 검토 전까지 실패). 대체는 별칭이 아니라 resolve 단계에서 해석된 파일 경로로 매칭하므로 업스트림 안의 상대 import(`./font-loader.ts`)도 대체됩니다.
  4. **오버라이드 내용**: (a) `main.ts` 포크 — `chrome=embed` 고정(열기·저장·새 문서·최근·비교는 호스트 소유), `?url=` 임의 URL 로드·자동저장 복구·PWA·플러그인 로더·개발 훅·CanvasKit 제거, Canvas2D 고정, (b) `core/font-loader` 포크 — `FontFace` API만 사용하고 `<style>`을 만들지 않으며 번들된 글꼴 파일만 요청(CDN·미동봉 항목은 요청하지 않음), (c) `command/print-pages` 포크 — 정적 규칙은 `print.css`, 쪽별 `@page` 규칙은 구성형 스타일시트(CSSOM)로 외부화하고 `print.html`의 `<style>`도 파일로 분리, (d) `embed/protocol`·`embed/rpc-router` 포크 — `plugin-loader-v1`·`hwpctrl-v1` 미광고와 `plugin.*` 메서드 제거, `blank-document-v1`과 `createBlankDocument` RPC 추가(새 문서를 호스트가 요청할 방법이 프로토콜에 없기 때문), (e) `index.html` 포크 — 인라인 `style` 속성 제거, 초기화 스크립트 상대 경로, 아이콘 자산 제거. 글꼴 카탈로그의 CDN URL 문자열은 빌드 플러그인이 비활성 스킴(`blocked-external-font:`)으로 바꿔 번들에 외부 URL이 남지 않습니다. `canvaskit-wasm`·`vite-plugin-pwa`·`@tauri-apps/*`는 의존성에 없습니다.
  5. **번들은 빌드하고 커밋하지 않습니다.** 산출물 `crates/easyoffice-app/ui/hwp-studio/`는 `.gitignore` 대상이며 추적되면 위생 가드가 실패합니다. 근거: 압축 전 약 2 MB의 해시 이름 JS가 빌드마다 바뀌어 diff·저장소 크기만 키우고, 입력(커밋된 WASM, 고정 upstream 커밋, 잠금 파일)이 모두 고정이라 `scripts/build-hwp-studio.mjs`(약 1초 vite + 약 50초 `npm ci`)가 같은 결과를 내며, Windows CI가 `ui/hwp-studio/index.html` 산출·크기 기록·정적 검사·하니스를 매번 증명합니다. WASM만 커밋하는 이유는 툴체인(Rust 1.93.1·wasm-pack)이 엔진 MSRV와 달라 CI에 넣으면 빌드가 크게 늘기 때문입니다.
  6. **번들 정적 검사 (`scripts/check-hwp-studio-bundle.mjs`)**: 외부 URL(허용은 `http://www.w3.org/` 네임스페이스와 입력 칸 예시 `https://example.com`뿐), `eval(`·`new Function`·`__TAURI`·서비스 워커·확장 API 참조, 인라인 스크립트·스타일·핸들러, 소스맵·워크박스·`canvaskit*.wasm`, `.wasm`이 벤더 파일과 다름, `asset-manifest.json`(빌드가 쓰는 파일별 크기·SHA-256)과 불일치·미등재 파일, 글꼴 제외 20 MiB 초과를 실패로 처리합니다. 실측: 글꼴 제외 13.46 MiB(WASM 11.27, JS 1.98, CSS 0.11), 글꼴 18.42 MiB.
  7. **글꼴 (`assets/fonts`)**: SIL OFL 1.1 글꼴 28개(Noto Sans/Serif KR, 나눔고딕·명조·고딕코딩, 고운바탕·돋움, Pretendard 9종, D2Coding, 스포카 한 산스)를 rhwp가 게시한 woff2 그대로 복사했고 파일별 OFL 전문(`licenses/`)을 함께 둡니다. `fonts.json`(파일·가족·라이선스·크기·SHA-256)과 `FONTS.md`를 `scripts/verify-fonts.mjs`가 검사합니다(OFL-1.1이 아니거나 전문이 OFL 1.1이 아니거나 미등재·변조·문서 누락이면 실패). 제외: 카페24·행복고딕(독자 무료 약관), Latin Modern Math(GUST, OFL 아님), Source Han Serif K 옛한글 부분집합(이름 `Source`를 예약한 글꼴의 수정 부분집합), HWP 제작사·Microsoft·OS 글꼴. Pretendard woff2는 공식 배포 파일과 바이트 단위로 같음을 확인했습니다.
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

## PD-082. 엔진 저수준 성능: CRC-32 하드웨어 가속·시트 슬롯 메모/DAG 계획 캐시·대형 워크로드 벤치와 release 게이트 (#135)
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

## PD-083. 한글 패널 iframe 임베딩·CSP·폴백 (#130)
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

## PD-084. 시트 범위 서식: 선택 범위 전체에 굵게·기울임·밑줄·취소선·글자 크기 (#143)

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

## PD-085. 한글 열기·HWP/HWPX 저장·수정 상태·IndexedDB 자동 저장 (#131)
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
  6. 비밀번호로 연 문서를 저장하면 저장본에 암호가 유지되지 않습니다(rhwp 직렬화가 암호·배포용 플래그를 지우고 편집기는 암호 문자열을 보관하지 않음). 이 단계에서는 막지 않았고, PD-120이 원본 저장 거부와 암호 없이 저장 확인으로 막습니다.
- **Alternatives rejected**
  - `getDocumentState()` 폴링: 호출마다 전체 문서를 내보내 대형 문서에서 느려 기각(이벤트 전달로 해결).
  - 부모 쪽 WASM으로 재파싱: 앱 CSP를 넓히거나 12 MiB WASM을 이중 적재해야 해서 기각(편집기 안에서 버려지는 문서로 재파싱).
  - 업스트림 `recovery/`(IndexedDB 복구) 사용: 복원 UI·검증을 부모가 통제해야 해서 기각.
  - rhwp 실패 시 조용한 평문 대체: 서식 유실을 숨기므로 기각(명시 안내를 붙인 대체만 허용).
  - HWPX 저장 금지(업스트림 정책): 사용자 결정으로 허용하되 검증·HWP 대안 제안을 둠.
- **Verified by** `node --test scripts/test-webui-hwp-session.mjs`(열기 라우팅·`detect_format` 힌트·64 MiB 한도·빈 파일·거부·텍스트 대체·비밀번호 취소/DRM·번들 없음, HWP/HWPX 저장 선택·검증 실패·HWPX→HWP 제안·취소/실패 기준선 유지·저장 중 편집, 수정 이벤트, 자동 저장 디바운스·크기 상한·저장 후 삭제, 복원 검증·손상 레코드·거절·오래된 본 정리, 변환기 원본), `node --test scripts/test-webui-hwp-host.mjs`, `node scripts/hwp-roundtrip-cdp.mjs`(헤드리스 Chrome 실제 입력: HWP·HWPX 열기 → 앞 3글자 20pt+굵게 → 저장(같은 형식·다른 형식) → 다시 열기, 글자·문단 모양·표 위치 동일, 11 MiB 문서 열기·저장, 편집 → 5초 후 IndexedDB 본 → 새로고침 → 복원 → 저장 → 본 삭제), 위생 가드.
- **Revisit when** 5단계(단축키 포워딩·다중 탭: 탭 ID별 자동 저장 확장), 6단계(네이티브 어댑터·파일 연결·제자리 저장·원시 바이트 저장 IPC: Base64 3회 복사 제거), rhwp 갱신으로 `exportHwpxWithReport` 손실 보고를 노출할 수 있을 때.

## PD-086. `--serve` 루프백 서버를 개발·테스트 전용으로 한정하고 릴리스 배포 중단 (#151)
- **Context** EasyOffice는 서버 없이 설치해 쓰는 프로그램이고 설치형 데스크톱 앱(#121)은 앱 내부 IPC만 쓰며 열린 네트워크 포트가 없습니다. 그런데 릴리스에는 루프백 HTTP 서버를 띄우는 `--serve` CLI zip이 보조 산출물로 남아 있었습니다.
- **Decision (`PD-086`)**
  1. 제품은 서버리스 설치형입니다. `--serve`는 헤드리스 브라우저 UI 검증과 CI 스모크 같은 개발·자동 검증에만 쓰며 사용자에게 배포하지 않습니다.
  2. `release.yml`은 CLI 빌드·zip·체크섬 항목·`--serve` 스모크를 모두 제거했습니다. 릴리스 자산의 구성은 7~9항이 정합니다. `ci.yml`의 `--serve` 스모크와 cargo 기능 `embed-hwp-studio`(테스트 포함)는 유지합니다.
  3. 위생 가드(`scripts/release-serve-rule.mjs`, 테스트 `scripts/test-release-serve-rule.mjs`)가 `release.yml`(주석 제외)에 CLI 서버 산출물 이름·`--serve`·CLI 실행 파일 복사가 있으면 실패합니다(fail-closed, 파일이 없어도 실패).
  4. `--help`와 `--serve` 시작 메시지가 "개발·테스트 전용, 일반 사용자용이 아님"을 한국어·영어로 밝힙니다. 방어(Host·Origin 검사, 연결 수·시간 제한, CSP)는 개발 중 안전을 위해 그대로 유지합니다.
  5. README·README.ko의 사용자 안내는 설치 프로그램 기준이고 `--serve`는 짧은 개발자 절로 옮겼습니다.
  6. (#215) 릴리스 버전 정합: 루트 `[workspace.package] version`이 제품 버전의 정본이고(현재 0.1.0), `tauri.conf.json`은 `version`을 두지 않아 Tauri가 Cargo 버전을 씁니다(릴리스 빌드는 태그 버전을 `--config`로 덮어씀). `release.yml`의 첫 잡 `prepare`는 버전·태그·드래프트 여부를 한 번 정해 모든 잡에 넘기고, 태그 push일 때 태그(`v` 제거)가 워크스페이스 버전과 다르면 빌드 전에 실패시키며, 모든 빌드 잡이 이 잡을 기다립니다. `workflow_dispatch`에는 `publish` 입력(boolean, 기본 true)이 있어 `false`면 빌드와 Actions 아티팩트 업로드까지만 하고 `publish-release` 잡(`gh release create`·upload)을 건너뜁니다. 릴리스 후보는 `publish=false`로 설치 파일만 받아 확인합니다. 수동 실행은 태그가 없으므로 버전 검사 대상이 아닙니다. 정식 출시 전이라 버전을 0.x로 다시 시작합니다(QA 결정): 이전 v0.1.0~v2.0.0 릴리스·태그 14개는 2026-11-08에 삭제했습니다.
  7. 릴리스 채널(QA 요청): macOS 유니버설 `.dmg`, Windows x64 NSIS 설치 프로그램, Linux x64 AppImage·`.deb`·`.rpm`, Linux arm64 `.deb`. Linux 빌드 잡(`build-linux-release`)은 x64를 `ubuntu-22.04`, arm64를 `ubuntu-22.04-arm`에서 만들어 패키지의 glibc 기준선이 2.35입니다. WebKitGTK 4.1·GTK 3·libxdo·OpenSSL 3(업데이트 확인의 TLS)은 `tauri.conf.json`의 `bundle.linux`가 의존성으로 선언합니다(deb는 패키지 이름, rpm은 배포판마다 패키지 이름이 달라 라이브러리 capability). Linux 파일 연결은 `.desktop`의 `MimeType=`(문서 9종 확장자 전부에 MIME 형식을 지정)이고 메뉴 분류는 Office입니다. Tauri 기본 `.desktop` 템플릿은 `Exec`에 파일 필드 코드를 넣지 않아 더블클릭해도 문서가 앱에 전달되지 않으므로, `crates/easyoffice-desktop/linux/easyoffice.desktop`을 `bundle.linux.deb.desktopTemplate`와 `rpm.desktopTemplate`로 지정합니다(AppImage는 번들러가 deb 템플릿을 그대로 씁니다). 템플릿은 `Exec=… %F`로 선택한 파일을 모두 경로로 넘기고, 앱은 시작 인수와 단일 인스턴스로 전달된 인수를 각각 새 창으로 엽니다(`window_commands.rs`의 `open_startup_files`·`handle_second_instance`). 빌드 도구·패키지 목록은 공용 composite action(`.github/actions/install-linux-deps`, `prepare-desktop-build`)에 한 번만 둡니다.
  8. 고정 파일명(QA 요청): 자산 이름에 버전을 넣지 않아 `https://github.com/AquilaXk/easyoffice/releases/latest/download/<이름>` 링크가 릴리스마다 유지됩니다. 이름은 `EasyOffice-macos-universal.dmg`, `EasyOffice-windows-x64-setup.exe`, `EasyOffice-linux-x64.AppImage`, `EasyOffice-linux-x64.deb`, `EasyOffice-linux-x64.rpm`, `EasyOffice-linux-arm64.deb`이고 체크섬은 합친 `SHA256SUMS.txt` 하나입니다. 이름·번들 위치·`--bundles` 값은 `scripts/release-assets.mjs` 한 곳에 있고, 빌드 잡은 번들이 정확히 하나가 아니면 실패하며, 발행 잡은 위 6개가 정확히 있지 않으면 `SHA256SUMS.txt`를 만들지 않고 실패합니다. README·README.ko의 직링크가 이 이름과 같은지는 테스트가 강제합니다. 이전 `EasyOffice-<version>-…` 이름과 플랫폼별 체크섬 파일은 대체되었습니다. 이미 공개된 릴리스에 다시 실행하면 자산이 하나씩 교체되므로 그동안 `SHA256SUMS.txt`와 자산이 잠깐 어긋날 수 있습니다(드래프트는 공개 전이라 영향이 없음).
  9. 드래프트 릴리스(QA 요청): 릴리스는 기본이 드래프트입니다. 수동 실행의 `release_draft` 입력(기본 true)이 `false`일 때만 바로 공개하고(`publish=true`이면 `main`에서 실행한 경우만 허용), 태그 push는 항상 드래프트로 만듭니다. 수동 실행의 `version`은 `1.2.3` 또는 `1.2.3-rc.1` 형태만 받고(`prepare`의 첫 단계), 동시성 그룹은 버전·`publish`·`release_draft`로 나뉘어 공개 요청이 같은 버전의 드래프트 요청에 취소되지 않습니다. QA가 드래프트의 자산을 확인한 뒤 공개합니다. `publish=false`(Actions 아티팩트만)는 그대로입니다. 이미 있는 릴리스에 다시 실행하면 자산만 교체하고, 공개된 릴리스를 드래프트로 되돌리지는 않습니다. **업데이트 알림(PD-103)과의 관계**: 알림은 `releases/latest`를 보고 GitHub는 드래프트를 `latest`에 넣지 않으므로, 드래프트 동안에는 알림도 README 직링크도 새 릴리스를 가리키지 않습니다. 공개하는 순간 둘이 함께 새 릴리스를 가리킵니다.
  10. 배포물 라이선스 고지(MIT 고지 조건): `THIRD_PARTY_NOTICES.md`를 `bundle.resources`로 모든 OS 산출물에 넣습니다(macOS `.app`의 `Contents/Resources`, Windows 설치 프로그램, Linux 패키지의 앱 리소스 폴더). 구성은 `scripts/desktop-file-associations.mjs`의 계약이 검사하고, 산출물은 CI·릴리스가 직접 열어 확인합니다(macOS는 `.app` 안 파일, Windows는 7-Zip으로 설치 프로그램 목록, Linux는 `scripts/verify-linux-bundles.mjs`).
  11. Linux 산출물 검사(`scripts/verify-linux-bundles.mjs`, CI `verify-linux`와 릴리스 Linux 잡): AppImage는 실행 없이 `--appimage-extract`로 풀어 구조를 보고, deb는 control의 아키텍처·의존성과 풀린 내용을, rpm은 아키텍처·Requires·파일 목록과, `.desktop` 파일의 SHA-256(rpm 자체의 파일 다이제스트)이 검사를 통과한 deb의 `.desktop`과 같은지를 확인합니다(같은 템플릿으로 만들어지므로 내용이 같아야 함. 러너의 `rpm2cpio`가 Tauri가 만든 rpm에서 메시지 없이 실패해 페이로드는 풀지 않습니다). 모든 패키지에 실행 파일·`.desktop`·고지 파일이 있어야 하고 `.desktop`은 Office 분류, 파일 인수 필드 코드(`%U` 등), 문서 9종 MIME 형식을 가져야 합니다. CI `verify-linux`는 x64 패키지까지 빌드해 검사하고 Linux 데스크톱 크레이트의 clippy·단위 테스트를 돌립니다(arm64 `.deb`는 릴리스에서만 빌드). 그 다음 릴리스 바이너리를 `xvfb-run`으로 `--smoke` 실행해 macOS·Windows와 같은 전 단계(`hwp` 편집기 왕복 포함)를 확인합니다(릴리스 잡은 반복하지 않음). 예전에 이 스모크가 `hwp` 단계 "연결 시간 초과"로 실패한 원인은 13항과 같은 CSP `'self'` 불일치였습니다. WebKitGTK(Ubuntu 22.04 `libwebkit2gtk-4.1`)에서 옛 `'self'` 정책으로 다시 빌드하면 같은 지점에서 실패하고, 앱 출처를 적은 현재 정책에서는 전 단계가 통과합니다(arm64 Linux 컨테이너의 Xvfb, 비루트 사용자, 2코어 제한으로 측정; x64는 CI로 확인).
- **Why** 사용자에게 로컬 포트를 여는 별도 실행 파일을 주지 않아 공격면과 안내 혼선을 없애면서, 자동 UI 검증 경로는 잃지 않습니다.
- **Trade-offs** 외부 사용자가 브라우저로 미리 써 보는 경로가 사라지고 소스 빌드가 필요합니다. 릴리스에서는 `--serve` 바이너리 스모크가 빠지고 CI가 같은 코드를 검증합니다.
- **Alternatives rejected** `--serve` 코드와 방어 제거(자동 검증 수단 상실), zip을 계속 배포하되 안내만 변경(사용자가 받을 수 있는 서버 산출물이 남음).
- **Verified by** `node --test scripts/test-release-serve-rule.mjs scripts/test-release-assets.mjs scripts/test-verify-linux-bundles.mjs scripts/test-desktop-file-associations.mjs`, `node scripts/check-repo-hygiene.mjs`, `cargo test -p easyoffice-app`(`--help` 문구), 기존 CI `--serve` 스모크, CI `verify-linux`(Linux 빌드·구조 검사), 릴리스 수동 실행 `publish=false`(세 OS 산출물과 구조 검사).
- **Revisit when** 정식 웹 배포 계획이 생기거나 UI 검증 방식이 `--serve` 없이 가능해질 때, 서명 키를 도입해 자동 설치 업데이터를 만들 때, arm64에 AppImage·rpm 요구가 생길 때.

## PD-087. Apple Pages(.pages) 읽기 지원 범위·스키마 검증 상태·법적 접근 (#154)

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
- **Updated by PD-157**: 지원 범위 표(요소별 지원/경고/미지원)와 "확인" 상수의 근거(픽스처는 템플릿에 다른 도구가 개체를 더한 것이라 순환 검증)는 PD-157이 현재 상태로 대신합니다. 위 5항의 필드 번호 중 독립 구현과 다른 것은 PD-157의 두 자리 읽기를 따릅니다.

## PD-088. 문서별 개별 창 모델·열기 큐 보안·단축키 전달·아이콘 세트 (#132, #126)
- **Context** 문서 탭 줄 시도(모듈 전환 탭, 한 창에 여러 문서)는 결과물이 조잡해 폐기했고, 사용자가 "문서마다 개별 창"으로 방향을 바꿨습니다. 같은 시기에 셸 화면(#126)은 1024px에서 우측 컨트롤이 잘리고, 상태 표시줄이 글을 자르며, 이모지·문자 기호가 아이콘 노릇을 하고, 일부 아이콘 버튼에 이름이 없는 문제가 있었습니다. 참고 구현은 참조 앱의 다중 창 구조(창별 열기 큐, 창 생성, 끌어 놓기)입니다. 코드를 가져오지 않고 구조만 따랐고, 참조 앱과 달리 WebView에는 경로를 주지 않습니다.
- **Decision (`PD-088`)**
  1. **창 모델**: 창 하나는 문서 하나, 변환기 하나 또는 시작 화면 하나만 보여 줍니다. 창이 보여 줄 것은 시작 주소의 `?module=DOC|HWP|SHEET|SLIDE|PDF|CONV|START&win=<창 이름>`이 정합니다. 메뉴 줄의 모듈 전환 탭·포맷 필터 칩·모듈 버튼은 없앴고 `mod-btn`·`tab-pill`·`tab-strip`은 더 이상 없습니다. 인수 없이 실행하면 일반 크기의 시작 창(홈 화면: 새 문서 6종·파일 열기·최근 문서 카드, PD-098로 컴팩트 창에서 바뀜)이 뜨고, 문서 창을 열면 시작 창은 스스로 닫힙니다. 마지막 창을 닫으면 앱이 종료됩니다(Tauri 기본 동작).
  2. **창 제목과 수정 표시**: `파일 이름 — EasyOffice`, 저장하지 않은 변경이 있으면 앞에 `● `를 붙입니다. 새 문서의 이름은 `제목 없음`이고, 열거나 저장한 파일의 이름이 제목이 됩니다. 제목은 페이지가 `report_window_state(name, dirty)`로 알리면 Rust가 네이티브 제목으로 설정하고, `--serve`에서는 `document.title`만 바꿉니다.
  3. **Rust 전용 열기 경로**: 창을 여는 명령은 Rust에만 있습니다. `open_document_window(kind)`는 종류만 받습니다. `open_file_in_new_window()`는 인수가 없고 Rust가 네이티브 열기 대화상자를 보여 주며, 고른 파일을 Rust가 정규 경로로 바꾸고(`canonicalize`), 확장자로 종류를 정하고, 크기(0 초과 256MiB 이하)를 확인한 뒤 읽습니다. 읽은 바이트와 표시용 이름은 **창 이름을 키로 하는 대기 큐**에 넣고 새 창을 만듭니다. 새 창은 `take_pending_open()`을 불러 `{name, base64}`를 **한 번만** 받습니다. 이 모양에는 경로가 없고(구조와 직렬화 테스트로 고정), 큐 항목은 창이 닫히면 버립니다. 끌어 놓기(`DragDrop` 창 이벤트)와 명령줄 인수도 같은 `open_path`를 거치므로 페이지는 경로를 보지 못합니다. 최근 문서는 앱 데이터 폴더의 `recent-documents.json`에 경로로 저장하고(최대 10개), 페이지에는 `{id, name, kind}`만 주며 id는 경로의 FNV-1a 해시(되돌릴 수 없음)입니다. `open_recent_document(id)`가 Rust 쪽에서 경로를 찾아 같은 `open_path`로 엽니다.
  4. **같은 파일 다시 열기**: Rust의 창 레지스트리가 정규 경로(Windows는 소문자 비교)에서 창 이름으로 가는 표를 갖고, 이미 열린 파일이면 새 창 대신 그 창을 복원·앞으로 가져옵니다(`focused`). 다른 이름으로 저장한 경로도 그 창의 것으로 옮깁니다. 이 표와 경로는 Rust 안에만 있습니다.
  5. **창 닫기**: `CloseRequested`에서 Rust가 레지스트리의 수정 상태를 보고, 저장하지 않은 변경이 있으면 닫기를 막은 채 페이지의 `__EASYOFFICE_CLOSE_REQUEST__()`를 실행합니다. 페이지의 확인 창(저장 / 저장 안 함 / 취소)에서 **저장**은 기존 저장 흐름(HWP는 `setExternalDirty`를 통한 수정 상태 포함)을 먼저 실행하고 성공했을 때만, **저장 안 함**은 바로 `close_current_window`로 창을 닫습니다. **취소**와 저장 취소·실패는 창을 그대로 둡니다. 확인이 떠 있는 동안의 두 번째 닫기와 페이지가 응답하지 않을 때의 탈출구는 PD-113을 따릅니다. 닫는 창의 복구 기록은 함께 지웁니다.
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

## PD-089. 시트 셀 서식·열 너비를 XLSX/ODS에 저장하고 다시 읽기 (#148)

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

## PD-090. DOC/PPT를 실제 MS-DOC·MS-PPT 바이너리로 기록 (#112)

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

## PD-091. CI 작업 분리와 Rust 캐시: 공통 게이트는 Linux에서 한 번, OS별 작업은 해당 OS에서만

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

## PD-092. 예약 글꼴 이름(RFN)이 있는 번들 글꼴은 공식 배포 파일 또는 그 WOFF2 압축본만 사용 (#144)

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

## PD-093. 한글 창 크롬 정리·상태 표시줄·시작 아이콘과 데스크톱 파일 수명주기: 단일 인스턴스·제자리 저장·파일 연결 (#158, #133)

- **Status** Implemented
- **Context** 한글 창에는 셸 메뉴 줄과 rhwp 메뉴 줄, 셸 상태 표시줄과 편집기 상태 줄, 셸 테마와 편집기 테마가 겹쳐 있었고, 상태 표시줄에는 진단 항목(`모듈: …`, `포맷: …`, `IME…`, `엔진: 로컬 서버…`)이 섞여 1024px에서 잘렸습니다. 시작 창의 문서·한글·PDF 카드는 같은 아이콘이었습니다. 또 데스크톱은 실행할 때마다 새 프로세스가 뜨고, 저장은 매번 대화상자를 열었으며, 더블클릭으로 파일을 열 수 없었습니다(PD-088 Revisit 항목, #111 후속).
- **Decision (`PD-093`)**
  1. **한글 창의 메뉴 줄 하나**: 한글 편집기가 창을 차지하는 동안(`#app[data-hwp-embed="true"]`) 셸 메뉴 줄(`#menu-bar`)을 숨깁니다. 편집기의 `파일` 메뉴는 오버레이(`apps/hwp-studio/index.html`)에서 셸 명령 여섯 개(새로 만들기, 열기, 최근 문서, 저장, 다른 이름으로 저장(HWP·HWPX), 닫기)로 바꿉니다. 항목은 `data-shell-cmd`만 갖고(`data-cmd` 없음) `src/embed/shell-menu.ts`가 기존 임베드 포트로 `{reason:"shell_command", command, id?, format?}` 이벤트를 보냅니다. 셸의 `hwp-host.js`는 이를 `new|open|recent|save|save-as|close` 허용 목록과 인수 형식(최근 문서 id는 `[A-Za-z0-9_-]{1,64}`, 저장 형식은 `hwp|hwpx`)으로 다시 검사한 뒤에만 `shell-chrome.js`의 동작을 실행합니다. 편집기 쪽 목록과 셸 쪽 목록은 테스트가 같은지 확인합니다. 이전의 "업스트림 파일 메뉴 숨김" 오버레이 CSS는 없앴습니다.
  2. **한 테마**: 셸 테마(밝게·어둡게·시스템)를 편집기에 넘깁니다. 첫 페인트는 프레임 주소의 `theme=`(시스템이면 생략), 이후 변경은 새 임베드 RPC `setShellContext({theme, recents})`(capability `shell-context-v1`)로 즉시 반영합니다. 셸은 처음엔 `system`(운영체제 설정을 따라 편집기도 스스로 따름)이고 사용자가 테마 버튼을 누르면 선택한 `light|dark`로 바뀝니다. 편집기는 받은 값을 허용 목록(`light|dark|system`)으로 검사합니다.
  3. **한 상태 표시줄**: 한글 창에서는 셸 상태 표시줄도 숨기고(`#status-bar`), 셸 알림(`#status-command-feedback`에 쓰이는 모든 문구)은 `#shell-toast`(`role="status"`, 4.5초, 오류성 문구 8초)에도 표시합니다. 같은 문구는 다른 창의 상태 줄에도 그대로 남습니다.
  4. **모든 창의 상태 표시줄**: 쪽·위치·입력 모드·파일 이름·일시 메시지·확대만 남기고(`선택 집계`는 시트 전용), 진단 항목(플랫폼, 모듈, 포맷, IME, 엔진)은 새 `도움말 ▸ 제품 정보…` 대화상자(`#about-dialog`)로 옮겼습니다. 위치·입력 모드·확대는 줄어들지 않고(`flex-shrink:0`), 파일 이름과 메시지가 먼저 줄임표로 양보합니다. 데스크톱에서는 `로컬 서버` 문구가 상태 표시줄에 나오지 않습니다(`--serve` 개발 모드에서만 제품 정보에 나옴).
  5. **시작 창 아이콘**: 문서(DOCX)·한글·스프레드시트·프레젠테이션·PDF·변환기가 각자 `icons.js`의 SVG(`file-doc|file-hwp|file-sheet|file-slide|file-pdf|file-convert`)를 쓰고, 색은 단일 강조 토큰입니다(모듈별 색은 PD-098로 폐기, 버튼은 홈 화면의 `home-action`).
  6. **단일 인스턴스(#133)**: `tauri-plugin-single-instance`(Rust 전용, WebView 권한 없음)를 첫 플러그인으로 등록합니다(`--smoke`에서는 등록하지 않아 실행 중인 인스턴스와 겹치지 않음). 첫 실행 인수와 두 번째 실행 인수는 같은 경로(`launch_files` → `route_launch_files` → `open_path`, 시작 창이 쓰는 열기 큐)로 들어가 새 창으로 열리거나, 이미 열린 파일이면 그 창을 앞으로 가져옵니다. 인수에 파일이 없으면 기존 창을 앞으로 가져옵니다. 인수 해석은 지원 확장자의 실제 파일만 통과시키고, WebView는 경로를 받지 않습니다.
  7. **제자리 저장(#133)**: 창 이름 ↔ 경로·지문(크기 + 수정 시각, PD-109부터 내용 해시 포함) 표(`tracked_files.rs`)는 Rust에만 있고, 파일을 열 때와 쓸 때 갱신됩니다. 앱 명령은 늘리지 않고 `save_document_file`에 `inPlace` 인수(불리언)를 더했습니다(허용 목록 9종 그대로, 경로는 여전히 인수·반환 어디에도 없음). `inPlace`이고 이 창이 같은 확장자의 파일을 추적하면, 같은 폴더의 임시 파일(PD-109부터 저장마다 고유한 이름)에 쓰고 `sync_all` 뒤 이름 바꾸기로 원본을 교체합니다(기존 `write_document_bytes`). 쓰기 전에 지문이 열 때와 다르면(`Changed`) 네이티브 확인 창(덮어쓰기/취소)을 띄우고, 취소하면 파일을 건드리지 않고 `cancelled`를 돌려줍니다. 원본이 사라졌으면(`Missing`) 묻지 않고 다시 만듭니다(PD-137로 개정: 같은 확인 창으로 묻고, 취소하면 다시 만들지 않음). 추적 파일이 없거나 확장자가 다르면(예: HWPX로 연 문서를 HWP로 저장) 기존 대화상자 경로를 그대로 탑니다. 대화상자 저장을 포함한 모든 저장 결과(`{status, name, bytes}`)에는 디렉터리 없이 파일 이름만 담고, 대화상자로 고른 경로는 Rust의 추적 파일 표에만 남겨 같은 창의 다음 저장이 제자리 저장으로 이어집니다(#166). 저장·내보내기 중 변환기(CONV) 창은 원본을 덮어쓰지 않도록 항상 대화상자를 씁니다. "다른 이름으로 저장"(HWP·HWPX)은 항상 대화상자입니다.
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

## PD-094. macOS도 Tauri 데스크톱 셸로 배포: Swift 앱 삭제·서명 없는 .dmg·파일 연결 계약 (#136)

- **Status** Implemented. 서명 보류는 PD-143으로 재확인(2026-11-09).
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

## PD-095. 오픈소스 구현 이식으로 엔진 성능 개선: 무의존 유지 + 허용 라이선스 소스 이식

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

## PD-096. 문서 그림 블록: PNG/JPEG 모델, 상한, 형식별 처리

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

## PD-097. Pages 가져오기 2단계: 도형 글자, 각주·머리글·바닥글 부록, 글꼴·첨자·들여쓰기·줄 간격

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

## PD-098. UI 기준을 참조 앱/rhwp-studio 크롬으로 통일하고 자체 디자인 시스템을 폐기

- **Status** Partially implemented (1단계: 토큰·테마·폰트, 2단계: 홈 화면·최근 문서 대화상자·제품 정보 버전 줄·시작 창 정상 크기, 3a단계: 메뉴 바·도구 상자·서식줄·상태줄 표면을 rhwp-studio 규칙으로 맞추고 리본 명령을 메뉴로 노출, 3b단계: 리본·빠른 실행 도구 모음·측면 패널 제거와 도구 대화상자 전환, 4a단계: 시트 화면 재입힘, 4b단계: 슬라이드 화면 재입힘, 4c단계: PDF 스튜디오·변환기 재입힘). 5단계(macOS 네이티브 메뉴·업데이트 알림)는 단계별 PR로 같은 절을 갱신합니다.
- **Context** 사용자 결정: 자체 디자인 설계 없이 앱 전체를 참조 앱(참조 앱, MIT) 구조로 갑니다. 참조 앱의 UI는 rhwp-studio 크롬(메뉴 줄, 아이콘 도구 상자, 서식 도구줄, 상태 표시줄, 색 토큰)에 홈 화면·최근 문서·업데이트 알림 등을 얹은 것이고, EasyOffice의 `third_party/rhwp`는 참조 앱과 같은 v0.8.7이라 같은 CSS를 이미 갖고 있습니다. 계획 전체는 `참조 앱-plan` 검토 결과를 따릅니다.
- **Decision (`PD-098`)**
  1. **토큰**: 색·글꼴·간격·반경·그림자는 rhwp-studio `base.css`의 토큰(`--ui-*`, `--accent-*`, `--doc-*`, `--focus-ring`, `--font-family-ui` …)만 씁니다(`ui/tokens.css`, 값은 그대로, 출처 헤더와 THIRD_PARTY_NOTICES 고지). 라벤더 팔레트(`--eo-*`)와 `docs/design-system.md`의 Swift 시대 설명은 폐기하고, 그 문서는 이 결정의 토큰 사용 규칙으로 바꿨습니다.
  2. **테마**: 모드는 시스템/밝게/어둡게 3택이고 기본은 시스템입니다. `<html data-theme-mode>`·`data-theme-effective`와 `color-scheme`을 `ui/theme.js`가 `<head>`에서 동기로 적용하고(첫 그림부터 올바름), 선택은 `localStorage`의 `easyoffice.theme-mode`에 저장하며 시스템 모드에서만 OS 변경을 따릅니다. 보기 메뉴에 `테마 > 시스템 설정/밝게/어둡게`(radio)가 있고 Ctrl+Alt+D(밝게/어둡게 전환)와 상단 버튼은 같은 API(`toggle`)로 명시 선택이 됩니다. 한글 편집기에는 모드(`system`/`light`/`dark`)를 그대로 전달합니다. 되돌리기 스냅샷의 `theme`은 모드 문자열입니다.
  3. **모듈 강조색 폐기**: 모듈별 색(`--eo-accent-*`, 시작 카드·탭 배지·`module-swatch`·인스펙터의 "테마 색상")을 없애고 강조는 단일 `--accent-strong`(텍스트·선)와 `--ui-menu-open`(열린 메뉴)을 씁니다. 부트 JSON의 `palette`·`moduleAccents`와 레이아웃 크레이트의 `SignatureLavenderPalette`·`OfficeModuleKind::accent_hex`도 삭제했습니다(슬라이드 덱 내보내기용 `SlideDeckTheme::SignatureLavender`는 문서 내용 테마라 유지).
  4. **고정 색**: 문서 종이와 슬라이드 쇼는 테마와 무관하게 자기 색을 갖습니다(rhwp의 `--doc-paper`와 같은 원칙). 이런 규칙은 `ui/content.css`에만 두고 리터럴 색을 허용하며, `#doc-editor`·`.slide-stage`·`.pdf-page-card`는 라이트 토큰과 `--color-*` 별칭을 한 규칙에서 다시 선언해 어두운 테마에서도 종이 안쪽이 밝게 유지됩니다(테스트가 라이트 값과의 일치와 누락 없음을 검사, PD-166). `styles.css`·`shell.css`에는 리터럴 색(검정 그림자 제외)과 `data-theme="…"` 선택자를 두지 않습니다.
  5. **대비**: 크롬 글자 4.5:1, 아이콘 3:1을 토큰 값으로 계산하는 테스트(`test-webui-shell-chrome.mjs`)를 새 토큰에 맞춰 다시 썼습니다. rhwp 다크 값 중 흰 글자와 `--ui-menu-open`은 4.2:1이라 다크 테마의 열린 메뉴는 `--ui-selected` 바탕과 `--ui-text`로, 기본 버튼은 `--accent-strong` 바탕과 `--ui-bg` 글자로 그립니다. 라이트 테마에서도 기본 버튼은 참조 앱의 `--accent-primary`가 아니라 `--accent-strong` 바탕에 `--ui-text-on-accent` 글자입니다(`--accent-primary` 위 흰 글자는 3.7:1이라 4.5:1에 못 미침, 참조 앱과 다른 의도적 차이, PD-166).
  6. **CSP·가드는 그대로**: 참조 앱 CSP의 `unsafe-eval`·`style-src 'unsafe-inline'`은 채택하지 않습니다(`DESKTOP_SHELL_CSP`, `MAIN_CSP`, `STUDIO_CSP` 불변). 참조 앱 코드를 옮길 때는 TypeScript를 JavaScript로 다시 쓰고 인라인 `style` 속성은 클래스로 바꿉니다.
  7. **홈 화면(2단계)**: 시작 창은 컴팩트 전용 창이 아니라 문서 창과 같은 크기(1280x860, 최소 1024x680, 최대화 가능)의 창이고 안에 참조 앱의 홈 화면(`ui/home-screen.js`, `home.css`)이 뜹니다. 구성은 참조 앱과 같은 hero(제목·설명·큰 버튼) + 최근 문서 카드(쪽당 5개, 이전/다음, 빈 자리 카드) + 끌어 놓기 안내이고, EasyOffice 고유 차이는 새 문서 버튼 6종(아이콘 포함)과 "문서 열기" 버튼, 카드가 경로·미리보기 없이 이름과 종류만 보인다는 점입니다(셸은 경로를 페이지에 주지 않음, PD-088). 열기·끌어 놓기·최근 문서 열기는 기존 창 다리(`window-bridge.js`)와 셸 명령을 그대로 쓰고, 문서를 열면 시작 창이 닫히는 동작도 같습니다. `파일 > 최근 문서 > 전체 목록 보기…`가 참조 앱의 최근 문서 대화상자(`ui/recent-documents-dialog.js`: 목록 상자, 방향키·Enter·Esc, 더블클릭)를 열며 "목록 비우기"는 셸에 대응 명령이 없어 넣지 않았습니다. 제품 정보 대화상자에는 부트 응답의 `appVersion`(크레이트 버전)으로 `EasyOffice x.y.z` 줄이 붙습니다.
  8. **메뉴 바(3a단계)**: 메뉴는 rhwp-studio의 `.menu-item/.menu-title/.menu-dropdown/.md-item/.md-sub/.md-sub-panel` 구조와 규칙(`ui/menu-bar.css`)이고 순서는 파일·편집·보기·입력·서식·쪽·표·도구·도움말입니다. 도구 상자·서식줄·상태줄의 배경·경계·호버 값도 rhwp 값으로 바꿨습니다(`ui/toolbar.css`, 크기는 아이콘만 있는 EasyOffice 도구 상자에 맞춰 유지). 명령 id는 그대로이고, 리본 카탈로그 140종(5개 모듈 × 28)은 모두 활성 메뉴 항목에서 실행됩니다. 메뉴 항목은 `data-studio-menu-cmd`가 리본 명령 id의 접미사(`home-bold` → `<모듈>-home-bold`)이거나 모듈 전용 전체 id(`sheet-mod-pivot`, 해당 모듈에서만 활성)이며, 대응은 `ribbon-catalog.js`의 `resolveMenuCommand` 한 곳에 있고 구 항목 3개(`mod-footnote`, `mod-header`, `mod-symbol`)는 별칭입니다. 새 위치: 보기(100% 배율·스플릿 뷰·문서 개요), 입력(목차·메일 머지·누름틀), 쪽(공문서 두문·원고지·슬라이드 마스터/정렬/전환/발표자), 표(표 셀 수식·테두리, 자동 합계·함수 마법사·정렬·유효성, 피벗·차트·조건부 서식·틀 고정), 도구(변경 추적·메모·보안 검사·통계, PDF 주석·회전·분할·변환). 동작은 rhwp 메뉴 바와 같이 호버 전환, Esc로 닫기, 메뉴가 열린 상태에서 단일 키(`/`)로 해당 단축키 항목 실행이고, 최근 문서 대화상자는 Ctrl+Alt+O(단축키 표 `recent-dialog`, 한글 편집기에서도 전달)입니다. 리본 제거는 10항입니다.
  9. **고지**: 참조 앱·rhwp-studio에서 옮긴 파일은 머리에 출처를 쓰고 THIRD_PARTY_NOTICES의 `참조 앱 (code adapted)`·rhwp 절에 목록과 MIT 전문을 둡니다(PD-095 3항과 같은 방식).
  10. **리본 제거와 도구 대화상자(3b단계)**: 리본 탭·도구 띠·빠른 실행 도구 모음·"표준/한글 리본" 전환·접기 단추·좌우 측면 패널과 그 CSS를 없앴습니다. 명령 실행 경로는 그대로라서 메뉴와 아이콘 도구 상자가 `resolveMenuCommand`로 카탈로그 id를 찾아 `execute_ribbon_command`를 호출하고, 카탈로그 140종과 IPC 계약(`execute/undo/redo_ribbon_command`, `switch_ribbon_style`, `get_ribbon_category_commands`)·Rust `RibbonLayoutStyle`은 유지합니다. 다만 리본 접기 항목(`*-view-ribbon`, 모듈당 1개)은 메뉴에서 뺐으므로 메뉴에서 닿는 명령은 모듈당 27개(135종)입니다. 부트 JSON에서 `ribbonCategories`·`ribbonMatrixSlots`·`recentFormatFilters`를 뺐습니다. 스마트 보조(`문장 진단 및 요약`), 개인정보 스캔/마스킹, 보안 검사, 성능 진단, 문서 개요는 측면 패널 대신 `도구`·`보기` 메뉴에서 여는 대화상자(`<dialog class="tool-dialog">`, `.dialog-wrap/.dialog-title/.dialog-body/.dialog-footer` 재사용, 닫기 단추와 Esc)로 옮겼고 요소 id와 실행 동작은 같습니다. 저장·되돌리기·다시 실행은 `파일`·`편집` 메뉴와 단축키로 합니다.
  11. **시트 화면(4a단계)**: 수식 입력줄은 별도 행으로 두되 rhwp 서식 바 표면(`eo-doc-style-bar`, `#sheet-formula-bar`는 `toolbar.css`에서 `#doc-style-bar`와 같은 배경·경계)과 `.sb-field-grid`·`.sb-select`·`.tb-group`·`.tb-btn` 클래스를 쓰고, 시트 탭(`#sheet-tab-bar`)은 시트 칸 안쪽이 아니라 시트 창의 마지막 행으로 옮겨 상태 표시줄 바로 위에서 같은 표면(`--ui-bg-light`, `--ui-border-strong`)과 아래쪽이 둥근 탭 모양(활성 탭은 `--ui-surface`)을 갖습니다. 그리드 머리글은 `--ui-bg-light`, 선택은 `--accent-strong`입니다. 요소 id·동작은 같고 리터럴 색은 없습니다(`test-webui-sheet-chrome.mjs`). 같은 변경에서 `workspace-tools.js`의 개인정보 스캔·마스킹 로직을 `workspace-scrub.js`로 분리했습니다(동작 동일, 1,000줄 상한 여유).
  12. **슬라이드 화면(4b단계)**: 슬라이드 도구줄(`#slide-toolbar`)은 서식 바 표면에 `.tb-group`/`.tb-btn` 아이콘 단추(추가·복제·삭제·위/아래·처음부터·현재 슬라이드부터·레이저 포인터, 이름은 `aria-label`)와 `.sb-field-grid`의 비율·화면 전환 콤보 상자로 바뀌었습니다. 콤보 상자는 참조 앱의 custom-select를 JavaScript로 옮긴 `ui/custom-select.js`(`select.sb-combo`와 참조 앱처럼 `sb-ls-select`·`dialog-select`·`formula-select`가 대상, PD-166, 출처 헤더·고지, CSS는 `toolbar.css`)이고 원래 `<select>`는 숨은 상태 보관자로 남아 `value`·`change`·`options`가 그대로입니다(코드에서 `select.value`를 대입해도 표시가 갱신됨). 레이저 포인터 단추는 글자 대신 `aria-pressed`와 `title`("레이저 포인터: 켬/끔")로 상태를 알립니다. 썸네일 패널은 `--ui-bg-light`·`--ui-border`·`--ui-selected`, 편집 스테이지는 `--doc-paper`·`--doc-shadow`이고 스테이지 안쪽 글자·경계는 `content.css`의 `.slide-stage`가 라이트 값으로 고정합니다(슬라이드 쇼 고정색도 `content.css`). 동작과 요소 id는 같고 기존 슬라이드 테스트는 수정 없이 통과합니다(`test-webui-slide-chrome.mjs`가 DOM 계약과 토큰을 검사).
  13. **PDF 스튜디오·변환기(4c단계)**: PDF 도구줄(`#pdf-toolbar`)은 서식 바 표면(`toolbar.css`의 `#doc-style-bar`와 같은 값)에 `.tb-group`/`.tb-btn`(회전은 아이콘 단추, 양식·스탬프 적용은 글자 단추)과 `.sb-field-grid` 안의 `.sb-select` 스탬프 선택·워터마크 입력으로 바뀌었고 회전 배지는 `.sb-size-unit`입니다. 페이지 카드는 `--doc-paper`·`--doc-shadow`이며 안쪽 글자·경계는 `content.css`의 `.pdf-page-card`가 라이트 값으로 고정합니다. 변환기의 입력·선택은 `.sb-select`, 단추는 `.dialog-btn`(변환은 `.dialog-btn-primary`), 결과 상자(`.security-card`)는 `--ui-surface-raised`·`--ui-border` 토큰 상자입니다. 요소 id·동작은 같고 `.secondary-btn`·`.primary-btn`·`.cmd-input`·`.ctx-badge`는 이 두 화면에서 쓰지 않습니다. 스탬프·변환 형식 선택은 4b단계의 콤보 상자(`select.sb-combo`, custom-select)를 씁니다(`test-webui-pdf-conv-chrome.mjs`가 DOM 계약과 토큰을 검사).
- **Why** 두 개의 디자인 체계를 유지하지 않고 한글 편집기(rhwp-studio)와 나머지 창이 같은 토큰·테마를 쓰게 하기 위함입니다.
- **Trade-offs** 모듈을 색으로 구분하던 단서가 사라집니다(아이콘·창 제목으로 구분). rhwp의 일부 토큰(`--ui-text-hint` 등)은 작은 글자에서 대비가 낮아 그 위의 글자에는 쓰지 않습니다.
- **Alternatives rejected** 라벤더를 rhwp 토큰 위에 별칭으로 유지(두 체계가 남음), 참조 앱 CSP 채택(코드 실행 허용 범위 확대).
- **Verified by** `node --test scripts/test-webui-theme.mjs scripts/test-webui-shell-chrome.mjs scripts/test-webui-hwp-shell.mjs scripts/test-webui-menu-bar.mjs scripts/test-webui-tool-dialogs.mjs`(모드 저장·OS 추종·라디오 메뉴, 리터럴 색·`--eo-` 잔재 없음, 정의된 토큰만 사용, 종이 팔레트, 대비), `node scripts/check-repo-hygiene.mjs`.
- **Revisit when** 2단계 이후 PR이 메뉴·도구 상자·시작 화면을 바꿀 때마다 이 절의 상태와 `design-system.md`를 갱신합니다.

## PD-099. 데스크톱 창 크기·위치·최대화 상태 기억과 macOS 최근 항목

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

## PD-100. 암호가 걸린 HWP 열기: 업스트림 흐름 사용과 암호 비보관

- **Status** Implemented (기존 오버레이 흐름 확인과 가드 테스트 추가)
- **Context** 참조 앱은 `document-open.ts` 어댑터로 암호 문서를 열지만, EasyOffice가 고정해 쓰는 rhwp-studio의 열기 깔때기(`apps/hwp-studio/src/main.ts`의 `loadDocumentForOpen`)가 같은 흐름을 이미 갖고 있고 고정 WASM(`vendor/rhwp-core`)에도 `HwpDocument.openWithPassword`가 있습니다. 같은 일을 하는 코드를 한 벌 더 옮기면 두 흐름이 남으므로 참조 앱 어댑터는 옮기지 않았고(THIRD_PARTY_NOTICES에 새 파일 없음) 업스트림 흐름을 정책과 테스트로 고정했습니다.
- **Decision (`PD-100`)** (1) 일반 열기를 먼저 시도하고 엔진이 "비밀번호가 필요한 암호 문서"라고 보고할 때만 암호 대화상자(`showHwpPasswordDialog`, 업스트림 `.dialog-wrap` 대화상자)를 띄웁니다. 다른 오류는 그대로 올립니다. (2) 취소하면 `DocumentOpenCancelledError`로 열기를 중단하고 이전 문서 뷰를 되살립니다. (3) 틀린 암호(또는 손상, 구분 불가)는 "암호가 일치하지 않거나 문서가 손상되었습니다. 다시 입력하세요."로 다시 묻습니다. (4) 지원하지 않는 암호화 방식·DRM·그 밖의 실패는 고정된 한국어 오류로 끝내며 엔진 원문 오류(입력값이 섞일 수 있음)는 화면·콘솔에 내지 않습니다. (5) 암호는 한 번의 시도 범위의 지역 변수로만 쓰고 시도 직후 비웁니다. 저장소·최근 문서·자동 저장·로그·`postMessage`·임베드 프로토콜·셸 명령으로 보내지 않으며(셸은 파일 바이트만 전달), 암호로 저장(내보내기)은 이번 범위 밖입니다. (6) 암호 입력을 기다리는 동안 셸의 열기 제한 시간은 10분입니다(`hwp-session.js`).
- **Why** 암호는 문서를 푸는 데만 필요하고 어디에도 남지 않아야 하며, 같은 흐름을 두 벌 유지하지 않기 위함입니다.
- **Trade-offs** 문자열 암호는 JavaScript에서 확실히 지울 수 없어 지역 참조만 비웁니다. 오입력과 암호문 손상을 구분하지 못합니다(업스트림과 같음).
- **Alternatives rejected** 참조 앱 어댑터를 따로 이식(중복), 암호를 셸이 받아 네이티브에서 풀기(암호가 셸 경계를 넘음).
- **Verified by** `node --test scripts/test-hwp-studio.mjs`(실제 흐름 코드를 모의 엔진·대화상자로 실행: 암호 없음·정답·오답 후 재시도·취소·미지원/DRM/손상·암호 비반향, 암호가 임베드·셸 파일에 없음, 고정 WASM과 번들에 흐름 존재), 업스트림 `rhwp-studio/tests/hwp-password-open.test.ts`.
- **Revisit when** 암호로 저장을 지원하거나 업스트림 열기 깔때기가 바뀔 때(오버레이 해시 갱신과 함께 이 절을 갱신합니다).

## PD-101. 한글 문서 검색 가능한 PDF 내보내기: 데스크톱 셸의 svg2pdf 의존성 정책

- **Status** Implemented (셸 변환·저장 명령·상한, 글꼴 이름 대체표, 한글 창 파일 메뉴 "PDF로 내보내기" 연결; 번들 TTF 글꼴은 두지 않기로 결정)
- **Context** 한글 문서를 PDF로 만들 때 글자가 그림이 되면 검색·복사가 안 됩니다. 참조 앱은 rhwp의 페이지 SVG를 `svg2pdf`로 바꿔 글꼴 부분집합과 `ToUnicode` 맵이 든 PDF를 만듭니다. rhwp는 같은 변환기의 결정론·그러데이션 수정본을 `vendor/svg2pdf`로 고정해 두며, 우리는 rhwp `1a76570`의 그 사본을 저장소의 `vendor/svg2pdf`로 복사해 씁니다(CI는 서브모듈을 받지 않음, 수정 없음).
- **Decision (`PD-101`)** (1) 의존성은 **데스크톱 셸 크레이트(`easyoffice-desktop`)에만** 둡니다: `svg2pdf 0.13`(`default-features = false`, `text`), `usvg 0.45`, `fontdb 0.23`, `pdf-writer 0.12`. 엔진 크레이트는 외부 의존성 0을 유지합니다(PD-053, PD-095). 이 정책이 셸에 한해 PD-095의 "의존성 0"을 넓히는 유일한 예외이며, 이유는 PDF의 글꼴 서브셋·`ToUnicode`를 자체 구현하면 이식 대상(PD-095의 100줄 기준)을 크게 넘기 때문입니다. (2) 워크스페이스 `[patch.crates-io]`로 `svg2pdf`를 rhwp의 고정 사본으로 바꿉니다(엔진 크레이트는 svg2pdf를 쓰지 않으므로 영향 없음). (3) 라이선스는 전이 의존성 77개를 확인했고 모두 MIT, Apache-2.0, Zlib, BSD-2/3, 0BSD, Unlicense, BSL-1.0 계열(또는 이들의 OR 조합, 유니코드 데이터는 `Unicode-3.0`)이며 MPL·GPL·라이선스 불명은 없습니다. 목록 요약은 THIRD_PARTY_NOTICES에 있습니다. 새 의존성은 같은 확인을 거칩니다. (4) 변환은 `pdf_export.rs`(참조 앱 `pdf_export.rs`와 rhwp PDF 렌더러의 페이지 조립을 옮김): 쪽마다 `usvg::Tree::from_data`(SVG 바이트를 복사 없이 읽음)와 `svg2pdf::to_chunk`, 청크를 한 객체 공간으로 다시 번호 매기고 쪽마다 폼 XObject를 점 단위 MediaBox에 놓습니다. 한 쪽이라도 변환에 실패하면 쪽 번호가 든 오류로 전체를 실패시킵니다(쪽이 조용히 빠진 PDF를 만들지 않음). (5) 상한은 `checked_add`로 변환 전에 확인합니다: 쪽 수 2,000, 쪽당 SVG 64MiB, 합계 512MiB. 글꼴 데이터베이스는 프로세스당 한 번만 만들고(`OnceLock`) 번들 글꼴 폴더(`pdf-fonts`) 다음에 시스템 글꼴을 읽으며 일반 serif/sans/mono 대체는 Noto Sans KR입니다. (6) 새 앱 명령 `export_searchable_pdf(file_name, pages)`는 쪽 SVG 문자열만 받고 경로는 받지 않으며, 기존 저장 대화상자 흐름(`save_via_dialog`)을 `save_document_file`과 공유합니다. 명령 목록·capability는 1:1 테스트로 묶여 있습니다(10개). (7) **글꼴**: TTF는 번들하지 않고 Windows(맑은 고딕, 바탕 등)·macOS(Apple SD Gothic Neo, AppleMyungjo)에 기본으로 깔린 한국어 시스템 글꼴을 씁니다. `pdf_font_fallbacks.rs`(참조 앱 `pdf_font_fallbacks.rs` 이식)가 SVG를 읽기 전에 `font-family` 목록의 문서 글꼴 이름(함초롬바탕·돋움, 바탕·돋움·굴림·궁서 계열, 휴먼·HY 계열 등 정렬된 정적 표 약 35개)을 바탕체/돋움체 구분 없이 명조 계열·고딕 계열로 나눠, 원래 이름은 맨 앞에 두고 그 플랫폼의 설치 글꼴(macOS는 Apple 글꼴 먼저, 그 외는 맑은 고딕·바탕 먼저)과 일반 family를 뒤에 붙입니다. 표 조회는 ASCII 대소문자를 접은 바이트 비교의 이진 탐색이라 할당이 없고, 바꿀 이름이 없는 SVG는 복사 없이 그대로 씁니다. (8) **편집기 연결(데스크톱 전용)**: 오버레이 파일 메뉴의 `data-shell-cmd="export-pdf"` 항목(기본 숨김)을 셸이 `setShellContext`의 `pdfExport: true`로 보일 때만 표시합니다(`--serve`에서는 숨김 유지). 명령 허용 목록은 `new|open|recent|save|save-as|export-pdf|close`이고 편집기·셸이 같은 목록을 쓰며(테스트가 대조) `export-pdf`는 인수를 받지 않습니다. 셸(`hwp-session.js` `exportPdf`)이 임베드 포트로 `pageCount`를 묻고 2,000쪽을 넘으면 쪽을 렌더하기 전에 거부하며, `getPageSvg`를 쪽마다(긴 시간 제한) 받아 문자열 배열을 `export_searchable_pdf`로 넘깁니다. 진행·결과는 기존 알림(토스트)으로 알립니다.
- **측정** 20쪽(320x120 px, 영문 한 줄)을 같은 글꼴로 변환했을 때(`cargo test -p easyoffice-desktop pdf_export -- --nocapture`): 글자 임베드(검색 가능) 약 270ms·202KB, 글자를 윤곽선으로 바꾼 PDF 약 290ms·166KB입니다. 임베드 쪽만 `/ToUnicode`와 `/FontFile2`를 가지며 윤곽선 쪽에는 둘 다 없습니다(검색·복사 불가). 임베드는 파일이 약 22% 크지만 시간은 같은 수준입니다.
- **Why** 한글 문서 PDF의 글자를 검색·복사할 수 있게 하되 엔진 크레이트의 무의존을 지키고, 검증된 변환기와 rhwp가 고정한 수정본을 그대로 쓰기 위함입니다.
- **Trade-offs** 셸 바이너리가 커지고 빌드 시간이 늘어납니다(의존 크레이트 약 77개). 번들된 TTF가 없어 글꼴은 시스템 글꼴에 의존합니다(번들 글꼴은 woff2뿐이라 SFNT 변환이 필요, 리눅스처럼 한국어 글꼴이 없는 환경에서는 글자가 비어 보일 수 있음).
- **Alternatives rejected** `rhwp` 크레이트 전체를 셸에 의존(암호·서명 등 불필요한 큰 의존 트리), 글자를 윤곽선으로 변환(검색 불가), 엔진 크레이트에 PDF 글꼴 서브셋 자체 구현(이식 기준 초과).
- **Verified by** `cargo test -p easyoffice-desktop pdf_export save_file`(상한, 쪽 오류 전파, `/ToUnicode`·글꼴 임베드·쪽 수·MediaBox, 결정론, 글꼴 DB 1회), 명령·capability 1:1 테스트.
- **Revisit when** 시스템에 한국어 글꼴이 없는 환경(리눅스 등)을 지원할 때(번들 TTF 또는 woff2 해제), 하이퍼링크 주석을 넣을 때, 쪽 범위 선택을 추가할 때.

## PD-102. 네이티브 메뉴 막대: 한 벌의 표와 같은 핸들러, 이벤트 권한 없음 (모든 OS 설치는 PD-121에서 macOS만으로 정정)

- **Status** PD-121(95번)이 "모든 OS에 설치·페이지 메뉴 줄 숨김" 전제를 정정했습니다: 참조 앱의 네이티브 메뉴는 macOS에만 있고 페이지 메뉴 줄은 모든 OS에서 그립니다. 아래 (1)·(7)·(8)·(10)과 Windows·Linux 서술은 PD-121로 바뀌었고 나머지는 macOS에서 그대로입니다. Implemented, 개정 2026-11-08 (QA 요청: 참조 앱과 같은 동작. macOS·Windows·Linux 실기기 확인 전: 이 환경은 메뉴 구성 함수를 컴파일·clippy·단위 테스트로만 검증했습니다)
- **Context** 처음에는 macOS에만 메뉴 막대를 설치하고 Windows·Linux는 페이지가 그리는 메뉴를 썼는데, macOS에서는 두 메뉴가 겹쳤습니다. 참조 앱은 모든 OS에 네이티브 메뉴를 설치하고 창 안 메뉴는 그리지 않습니다.
- **Decision (`PD-102`, 개정)** (1) **모든 OS에 설치**합니다(`native_menu::install`, cfg 없음). 데스크톱 셸이 있으면 페이지가 `#app`에 `data-native-menu="true"`를 달고 `.studio-menubar`(메뉴 줄)를 숨깁니다. 명령 팔레트 버튼·테마 전환은 그대로이고, `--serve`·브라우저는 페이지 메뉴를 씁니다. 한글 편집기 창은 이전 규칙대로 메뉴 줄 전체가 숨겨집니다. (2) 메뉴는 파일 / 편집 / 보기 / 입력 / 서식 / 쪽 / 표 / 도구 / 도움말이며 macOS는 앞에 앱 메뉴(정보·서비스·숨기기·종료), 도움말 앞에 윈도우 메뉴가 더해집니다. **페이지 메뉴의 모든 항목이 표에 있습니다**(`test-webui-native-menu.mjs`가 `index.html`의 메뉴 항목을 읽어 표의 동작에 모두 있는지 확인합니다): 페이지 메뉴 항목을 누르는 동작(`studio-cmd:<명령>`, `window-new:<종류>`, `theme:<모드>`, `update-check`)이므로 모듈별로 쓸 수 없는 항목은 페이지와 똑같이 아무 일도 하지 않습니다. (3) **표는 하나**입니다: `native_menu.rs`의 `ITEMS`(id·라벨·단축키·페이지 동작)와 `shell-chrome.js`의 `NATIVE_MENU_ACTIONS`가 1:1이고 테스트가 대조합니다. 메뉴를 만드는 함수가 쓰는 id는 표의 id와 정확히 같습니다(Rust 테스트). 기존 항목(새로 만들기·열기·최근 문서·저장·다른 이름으로 저장·닫기·찾기)은 단축키 동작 표의 같은 핸들러를 실행합니다. (4) **라벨은 모든 OS에서 한국어**, 단축키는 `CmdOrCtrl`(OS 관례)입니다. 단축키는 기존 항목과 보기(확대·축소·100%·밝게/어둡게 전환)에만 두고, 진하게·기울임·밑줄·편집 항목은 단축키를 달지 않습니다: 포커스된 편집기(특히 한글 편집기와 입력 칸)가 그 키를 직접 처리해야 하기 때문입니다. (5) 편집 항목: macOS는 시스템 항목(되돌리기·오리기·복사·붙이기·모두 선택)을 씁니다. Windows·Linux는 시스템 되돌리기·다시 실행이 없어(GTK 미지원) 표의 항목(`edit:undo` 등 → 페이지의 같은 명령)을 씁니다. (6) **전달**: 메뉴 id가 표에 있을 때만 `window.eval`로 고정 스크립트(`window.__EASYOFFICE_NATIVE_MENU__("id")`, id는 JSON 문자열)를 **포커스가 있는 창에만** 실행합니다. 이벤트를 쓰지 않아 capability에 `core:event` 권한이 없고, 페이지가 보낸 값은 쓰이지 않으며 경로는 Rust만 다룹니다. 종료는 macOS 시스템 항목(`RunEvent::ExitRequested` → 창마다 저장 확인)입니다. (7) 스모크의 `menu` 단계는 모든 OS에서 이 OS의 모든 표 항목이 설치됐고, 테마·업데이트 확인 항목이 체크 항목이며, 끝내기가 Windows·Linux 메뉴에만 있는지 확인합니다. (8) **끝내기(Windows·Linux)**: 파일 메뉴 끝의 `file:quit`(`CmdOrCtrl+Q`, 참조 앱과 같음)은 페이지 동작이 없는 Rust 전용 항목이라 표(`ITEMS`)에 넣지 않습니다. `app_quit.rs`가 창을 포커스 창 먼저 한 창씩 `close()`해 기존 닫기 흐름(저장 안 된 창은 페이지의 저장/저장 안 함/취소)을 거치고, 창이 사라질 때마다 다음 창을 묻고 마지막 창 뒤에 `app.exit(0)`합니다. 사용자가 취소하거나 저장에 실패하면 페이지가 `end_close_prompt`를 불러 그 창의 닫기 확인을 끝내고(다음 닫기에서 다시 묻도록 `close_prompted` 해제) 종료를 멈춥니다(남은 창은 닫지 않음). 큐 앞 창이 아닌 창이 닫혀도 앞 창에는 다시 `close()`하지 않고, 마지막 창 뒤에 새 창이 있으면 큐를 다시 만듭니다. 이미 저장 확인 중인 창(`close_prompted`)은 끝내기·macOS `Cmd+Q`가 다시 `close()`하지 않고 앞으로만 가져오며(`raise_and_close`), 그 창의 답을 기다립니다(저장·저장 안 함 → 창이 사라져 다음 창, 취소 → `end_close_prompt`로 종료 중단). (9) **체크 표시·비활성**: 테마 세 항목과 업데이트 확인 알림은 체크 항목이고, 쓸 수 없는 항목은 회색입니다. 메뉴 막대는 모든 창이 공유하므로 각 창이 `report_menu_state(theme, disabled, embed)`로 자기 상태를 알리고(테마·모듈·편집기 상태가 바뀔 때), Rust가 **포커스 창**의 상태를 막대에 반영합니다(`menu_state.rs`, 포커스가 옮겨 갈 때와 업데이트 확인을 바꿀 때도). 비활성 규칙은 페이지 메뉴와 같은 한 벌(`studioCommandApplies`)을 표의 `studio-cmd:` 항목에 적용해 페이지가 id 목록으로 보내므로 Rust에 두 번째 표가 없고, 모든 id는 표와 대조합니다. 업데이트 확인 알림의 체크는 Rust의 설정값이며 확인을 하지 않는 실행(스모크·데이터 폴더 없음)에서는 회색입니다. (10) **한글 편집기 창(Windows·Linux)**: 이 창은 편집기가 자기 메뉴 줄(편집·입력·서식·표 등 수백 개 명령)을 가지므로 **그 창의 네이티브 메뉴 막대를 숨깁니다**(`report_menu_state`의 `embed`가 참이면 `hide_menu`, 거짓이면 `show_menu`; 편집기가 뜨지 못하면 막대가 다시 보임). 반대로 편집기 메뉴 줄을 숨기면 편집기 전용 명령 대부분에 닿을 길이 없어 기능 손실이 훨씬 큽니다. 숨긴 막대의 셸 항목 중 편집기 파일 메뉴가 전달하는 8개(새로 만들기·열기·최근·저장·다른 이름·PDF·인쇄·닫기)와 `Ctrl+K`(명령 팔레트: 새 문서 종류·열기·저장·도구)·`Ctrl+Alt+D`(테마)는 그대로 되고, 정보·테마 선택·업데이트 확인 알림은 다른 창(시작 창 등)의 메뉴에서 씁니다. macOS는 앱 전역 메뉴 하나라 변경 없음.
- **Why** 두 메뉴가 겹치지 않게 하고, 모든 OS에서 같은 메뉴 막대를 주면서 명령 경로를 새로 만들지 않고(표 하나, 핸들러 하나) 권한도 넓히지 않기 위함입니다.
- **Trade-offs** 실행을 `eval` 문자열로 넘기므로 호출 대상은 고정 이름 하나이고 id는 정적 표 값뿐입니다. 네이티브 항목은 모듈에 따라 비활성으로 보이지 않고(페이지의 `aria-disabled`가 기준), 누르면 아무 일도 하지 않습니다. 메뉴 상태는 포커스 창 기준이라 포커스가 옮겨 갈 때 한 번 갱신됩니다. 테마는 창마다 저장 값을 따로 읽어(창 사이 자동 동기화는 없음) 막대는 포커스 창의 값을 보여 줍니다. Windows·Linux의 한글 편집기 창에서는 네이티브 막대가 숨어 정보·테마 선택·업데이트 확인 알림이 그 창에 없고, `Ctrl+Q`(끝내기)도 숨긴 막대의 단축키라 Windows에서는 그 창에서 동작하지 않을 수 있습니다(편집기 파일 메뉴의 닫기는 그대로).
- **Alternatives rejected** 참조 앱처럼 `emit_to` + 페이지 `listen`(capability에 이벤트 권한 필요), 모든 창에 emit(포커스 없는 창이 저장/닫기를 실행할 수 있음), 메뉴 항목마다 Rust에서 직접 처리(저장 확인·한글 편집기 흐름 중복), 모든 편집 항목에 단축키(편집기 키 가로채기).
- **Verified by** `cargo test -p easyoffice-desktop native_menu`(id 유일·형식, 단축키 형식·중복 없음, 동작 이름 형식, 표의 id만 스크립트를 만들고 id는 인용됨, 메뉴 함수가 표의 id를 빠짐없이 그대로 씀, 편집 키는 단축키 없음), `node --test scripts/test-webui-native-menu.mjs`(Rust 표와 페이지 목록 일치, 페이지 메뉴 항목 ⊆ 표, 표의 페이지 항목 존재, 데스크톱 셸에서만 페이지 메뉴 숨김, 이벤트 권한 없음), `cargo test -p easyoffice-desktop app_quit menu_state native_menu`(종료 순서·취소·포커스 창 우선, 보고 검증, 체크 상태, 끝내기는 표 밖), `node --test scripts/test-webui-menu-state.mjs`(모듈별 비활성 id가 페이지 규칙과 일치, 테마 변경 보고, 한글 편집기 `embed` 보고, 취소 시 `end_close_prompt`), `cargo clippy -p easyoffice-desktop --all-targets -- -D warnings`.
- **Revisit when** 표·글 상자·그림 메뉴를 넣을 때(한글 편집기의 자체 메뉴와 겹침 확인), 실기기에서 Windows·Linux 한글 편집기 창의 메뉴 숨김·`Ctrl+Q`·체크 표시·회색 항목이 기대대로인지 확인한 뒤(정보·테마·업데이트 확인 알림이 그 창에 필요하면 편집기 메뉴로 전달할지), 실기기에서 편집 항목이 한글 편집기와 충돌할 때.

## PD-103. 업데이트: 하루 1회 릴리스 조회와 서명된 자동 설치

- **Status** Implemented (데스크톱 앱 전용; 서명 릴리스에서의 실제 설치·재시작은 Windows·macOS·Linux 실기기와 첫 서명 릴리스 확인 전)
- **Context** 사용자 결정(QA 요청): 업데이트는 참조 앱처럼 넣습니다. 처음에는 서명 키가 없어 알림과 릴리스 페이지 안내까지만 했으나(2026-11-08 이전), 서명 키 쌍이 생겨 참조 앱처럼 서명된 업데이터로 내려받아 설치하고 다시 시작합니다. 하루 1회 조회·끄기·404 처리·스모크 경로는 그대로 두고 알림 카드에 설치 동작을 더했습니다.
- **Decision (`PD-103`)** (1) 앱이 시작한 뒤(스모크 실행과 `--serve`는 제외) 백그라운드로 `https://api.github.com/repos/AquilaXk/easyoffice/releases/latest`를 **하루 1회 이하**로 조회합니다. 마지막 확인 시각은 시도 때 먼저 앱 데이터 폴더의 `update-check.json`에 적으므로 네트워크가 실패해도 같은 날 다시 시도하지 않고, 시계가 거꾸로 가면 확인할 때가 된 것으로 봅니다. (2) 요청은 Rust에서만 하고(`ureq` 2.x + `native-tls`, 운영체제 TLS, HTTPS 전용, 리다이렉트 없음, 10초 제한) 사용자·문서 정보는 보내지 않습니다. 사용자 에이전트는 `EasyOffice/<버전>`뿐입니다. 응답은 256KiB로 자르고 JSON에서 `tag_name`·`html_url`·`draft`·`prerelease`만 읽습니다. (3) 버전은 접두사 `v`가 선택인 숫자 3개(`x.y.z`)만 인정하는 바이트 파서(할당 없음, `checked_*`, 부분은 9자리 이하·선행 0 거부)로 읽고, 사전 배포·초안·`-rc` 같은 접미사는 무시하며 현재 버전보다 크면 알립니다. 현재 버전은 컴파일 때 박은 `CARGO_PKG_VERSION`이 아니라 실행 중 앱의 Tauri 설정 버전(`app.package_info().version`, 릴리스 빌드의 `--config` 덮어쓰기가 반영됨)이고, 사용자 에이전트의 `<버전>`도 같은 값입니다(#214: 예전에는 워크스페이스 1.0.0을 읽어 배포된 앱이 항상 새 버전이 있다고 판단함). 비교 함수는 버전 문자열을 인자로 받는 순수 함수라, 0.1.0으로 빌드한 앱은 `v0.1.0`에 알리지 않고 `v0.1.1`·`v0.2.0`에는 알립니다. 릴리스가 하나도 없어 API가 404를 주면 오류 없이 "릴리스 없음"으로 보고 알려 둔 알림을 지우며, 다음 확인은 그대로 하루 뒤입니다. 정식 출시 전 0.x로 재시작했고 이전 v0.1.0~v2.0.0 릴리스·태그는 2026-11-08에 삭제했습니다(QA 결정). (4) 릴리스 페이지 주소는 응답의 `html_url`이 `https://github.com/AquilaXk/easyoffice/releases/`로 시작하고 공백·제어 문자가 없을 때만 받습니다. 저장 기록을 읽을 때도 같은 검사와 현재 버전 비교를 다시 해서, 기록 파일을 고쳐 다른 주소를 넣을 수 없습니다. (5) 페이지 명령은 넷입니다: `get_update_notice`(상태), `set_update_check(enabled)`(켜기/끄기), `open_release_page`와 `install_update`(둘 다 **인수 없음**: 페이지는 주소도 패키지도 넘기지 않고, Rust가 가진 값만 씁니다). 명령 목록·capability는 1:1 테스트로 묶여 있습니다(17개). (6) 알림 카드는 참조 앱 `update-notice`를 옮긴 것(`update-notice.js`, `home.css`의 같은 클래스)으로 창 우상단에 "새 버전 x.y.z 사용 가능"과 "설치하고 다시 시작", "릴리스 페이지", "나중에"를 보입니다. 설치가 시작되면 참조 앱처럼 "준비 중"(받은 크기 / 전체 크기와 퍼센트), "설치 중", 실패하면 "업데이트 실패"(이유, "다시 시도", "릴리스 페이지", "닫기")를 보이고, 진행 중에는 숨길 수 없습니다. 저장 안 된 변경 때문에 재시작을 보류하면 "설치 완료, 저장한 뒤 앱을 다시 시작하세요"를 보입니다. 상태는 페이지가 0.7초 간격으로 `get_update_notice`를 불러 읽습니다(이벤트 권한 없음). "나중에"와 카드 안에서의 Esc는 그 버전을 이번 실행 동안 숨기고, 더 새 버전이 오면 다시 보입니다. 첫 확인은 창이 뜬 3초 뒤, 두 번째는 15초 뒤(백그라운드 조회가 끝날 시간), 그 밖에 창이 포커스를 받을 때입니다(네트워크 요청 없이 상태만 읽음). (7) 끄기: 도움말 메뉴의 "업데이트 확인 알림"(기본 켬, 데스크톱에서만 보임)이 같은 기록에 저장되며 끄면 요청도 알림도 없습니다. 한글 편집기 창은 편집기 메뉴를 쓰므로 이 항목이 없고, 설정은 다른 창에서 바꿉니다. (8) **스모크용 루프백 훅**: 요청 함수는 `fetch_release(url, https_only, app_version)`이고 실제 확인(`fetch_latest_release(app_version)`)도 같은 함수를 `https_only = true`로 부릅니다. `https_only = true`는 HTTPS 주소만 받습니다. `https_only = false`는 스모크 전용이며 URL을 파싱한 뒤 `http://127.0.0.1:<포트>/…`(호스트가 정확히 `127.0.0.1`, 포트 명시, 사용자 이름·암호 없음)만 허용하고 나머지는 연결 전에 거부합니다(`127.0.0.1.evil.example`, `127.0.0.1:80@evil.example`, `localhost`, 다른 사설 주소 포함). 스모크의 `update` 단계는 `127.0.0.1`에 1회용 가짜 서버(최대 10초 대기, 요청 머리글 8KiB 상한)를 열고 실제 요청을 보내, 요청 줄·`User-Agent`(`EasyOffice/<앱 버전>`, 앱 버전은 `app.package_info().version`)·`Accept`가 실제 확인과 같은지, 응답(`999.0.0`)이 새 버전으로 읽히는지, 훅이 다른 주소를 거부하는지 확인합니다. 응답 읽기 상한(256KiB)은 같습니다. 404는 `Ok(None)`(릴리스 없음, 알림을 지움)이며 스모크 `update` 단계도 가짜 서버의 404 응답으로 확인합니다. (9) **서명된 설치**(`update_install.rs`, 참조 앱 `updates.rs`를 옮김): "설치하고 다시 시작"은 `install_update`로 시작하며, Rust가 `tauri-plugin-updater`로 `https://github.com/AquilaXk/easyoffice/releases/latest/download/latest.json`을 읽습니다(확인 15초 제한). 알린 버전이 알림의 `x.y.z`와 같고 패키지 주소가 `https://github.com/AquilaXk/easyoffice/releases/download/`로 시작할 때만 내려받습니다(진행률, 256MiB 상한: 알린 크기나 받은 크기가 넘으면 내려받기를 끊음, 전체 20분 제한). 플러그인이 `tauri.conf.json`의 공개 키로 minisign 서명을 검증하며, `requireSignedVersion`으로 서명 안의 버전이 `latest.json`이 알린 버전과 같아야 합니다(오래된 서명 패키지를 새 번호에 붙이는 다운그레이드 차단). 다운로드·서명 실패, 지원하지 않는 설치 형태(`latest.json`에 이 설치 형태의 키가 없음), 버전·주소 불일치는 모두 오류 상태로 끝나며 **설치하지 않습니다**(fail-closed, 이유는 짧은 문장만 페이지로, 상세는 stderr). 설치는 블로킹 작업자에서 하고, 성공하면 앱을 다시 시작합니다(Windows는 NSIS 설치기를 `passive` 모드로 실행하고 앱을 종료하며 설치기가 다시 띄움). 릴리스 빌드에서만 동작합니다(개발 빌드는 설치 명령이 오류). (10) **저장 안 된 문서**: 어느 창이든 `dirty`로 보고돼 있으면(`WindowRegistry::has_unsaved`) 시작을 거부하고 "저장한 뒤 다시 시도하세요"를 카드에 보이며, 내려받은 뒤 설치 직전에 한 번 더 확인해 새로 생겼으면 설치하지 않습니다. 설치가 끝난 뒤 재시작 직전에도 확인해(macOS·Linux) 변경이 있으면 재시작을 보류하고 "installed" 상태로 안내합니다(Windows는 설치기가 앱을 종료하므로 설치 직전 확인이 마지막입니다). (11) **플러그인 권한 판단**: 플러그인은 등록하되 `updater:*` 권한을 어느 창에도 주지 않습니다. 플러그인의 JS 명령(`check`·`download`·`install`)은 ACL 기본 거부라 페이지(와 샌드박스 편집기 프레임)는 다운로드·설치를 직접 호출할 수 없고, Rust가 `UpdaterExt`로만 씁니다. 페이지에 열린 것은 인수 없는 `install_update`뿐이며 이것도 창별 토큰(`ShellCaller`)이 필요합니다. 구조 테스트(`capability` 목록 = `APP_COMMANDS`, 플러그인 권한 없음, 모든 명령이 `ShellCaller`를 받음)가 이를 고정합니다. 플러그인은 `default-features = false, features = ["native-tls", "system-proxy"]`로 운영체제 TLS를 쓰며 rustls는 들어오지 않습니다. (12) **지원 범위**: macOS universal(`.app.tar.gz`), Windows NSIS 설치기(`windows-x86_64`·`-nsis`), Linux x64 AppImage·deb·rpm, Linux arm64 deb(`linux-<arch>-<설치 형태>` 키; 플러그인이 설치 형태를 바이너리에 박힌 값으로 고름). 해당 키가 없는 설치는 "자동 설치를 지원하지 않습니다" 안내와 릴리스 페이지 버튼으로 수동 설치합니다. (13) **릴리스 workflow**(`release.yml`, `scripts/release-assets.mjs`): 모든 실행은 서명 키가 있어야 하며(없으면 `prepare`가 빌드 전에 실패, 발행 여부와 무관: 시험 실행 `publish=false`도 실제와 같게 서명해 서명 단계를 검증), 비밀 키는 세 빌드 단계의 환경 변수로만 넘어갑니다. 빌드 설정은 `--config`로 `bundle.createUpdaterArtifacts`를 켭니다(로컬·CI 빌드는 키가 없어 켜지 않으며, `tauri.conf.json`에는 넣지 않고 테스트가 그 사실을 고정). 패키지마다 `.sig`를 고정 파일명(`<자산>.sig`)으로 올리고, macOS는 `EasyOffice-macos-universal.app.tar.gz`(+`.sig`)가 추가됩니다. `publish-release`가 `latest.json`을 조립하며(플랫폼 키별 `url`은 태그가 든 릴리스 주소, `signature`는 `.sig` 내용, 모든 서명이 해당 버전용인지 검사) `SHA256SUMS.txt`에 `latest.json`·서명까지 모두 담습니다. 태그 버전 = 워크스페이스 버전 = 앱 버전 = `latest.json` 버전이라 업데이트한 앱은 같은 릴리스를 새 버전으로 보지 않고(업데이트 루프 없음), 드래프트는 `/releases/latest`에 안 잡히며, 사전 배포 버전(`-rc.1`)은 `--prerelease`로 올려 `latest.json`이 서빙되지 않고 알림도 그 버전을 무시합니다. (14) **키 보관**: 서명 비밀 키(`TAURI_SIGNING_PRIVATE_KEY`와 암호)는 GitHub Secrets와 로컬 `.env`에 둡니다(**백업 필수**: 잃으면 기존 설치본이 새 서명을 받아들이지 못해 수동 재설치가 필요). 공개 키만 `tauri.conf.json`에 있습니다. 값은 문서·PR·로그에 쓰지 않습니다.
- **Why** 새 버전을 모르고 오래된 버전을 쓰지 않게 하고, 사용자가 시작할 때만 서명을 확인한 패키지를 설치하며, 저장 안 된 문서를 잃지 않게 하기 위함입니다. 사용자 정보는 보내지 않습니다.
- **Trade-offs** 설치는 사용자가 카드에서 시작해야 합니다(백그라운드 자동 설치 없음). `latest.json`은 플러그인이 응답 전체를 읽어 크기 상한을 걸 수 없고 15초 제한과 HTTPS(GitHub)에 맡깁니다. 패키지는 메모리에 받아 서명을 확인한 뒤 설치합니다(256MiB 상한). 비밀 키를 잃으면 기존 설치본은 수동 재설치가 필요합니다. macOS 설치본은 Developer ID 서명·공증이 없는 임시 서명이라 첫 실행 허용 절차는 그대로입니다. GitHub 속도 제한이나 오프라인이면 조용히 알림 없이 넘어갑니다(오류는 콘솔에만).
- **Notes** 패키지 다운로드는 리다이렉트를 호스트 제한 없이 따라가며(GitHub 릴리스 자산이 CDN으로 넘어가기 때문), 신뢰의 근거는 주소가 아니라 서명입니다: 공개 키로 검증된 패키지만 설치합니다. 플러그인이 내려받는 `Vec`를 늘려 가므로 상한 근처의 순간 메모리는 상한의 약 2배이고, 그래서 상한을 256MiB로 둡니다(플러그인 안의 버퍼라 크기를 미리 잡을 수 없음). 협업자가 생기면 서명 Secret(`TAURI_SIGNING_PRIVATE_KEY`·암호)을 저장소 Secret에서 `main`·태그로 한정한 environment로 옮깁니다.
- **Alternatives rejected** 서명 없는 자동 다운로드·설치(변조 위험), 플러그인의 JS API를 페이지에 여는 것(페이지와 샌드박스 프레임에 다운로드·설치 권한이 생김), 플러그인 기본 기능(rustls/ring: 번들 TLS와 다른 라이선스), 시험 실행(`publish=false`)에서 키가 없으면 서명을 건너뛰는 것(서명 단계를 시험하지 못함), 페이지가 주소를 넘기는 열기·설치 명령(임의 주소 열기·설치), 백그라운드 무조건 자동 설치(저장 안 된 문서·사용자 동의).
- **Verified by** `cargo test -p easyoffice-desktop update_check`(버전 파서·비교, 앱 버전 문자열 기준 판단, URL 접두사, JSON 추출·상한·잘못된 입력, 하루 1회 기록, 실패 재시도 없음, 끄기, 스모크·데이터 폴더 없음, 기록 변조, 설치 상태가 페이지에 가는 모양과 보인 알림이 있어야 설치가 시작됨), `cargo test -p easyoffice-desktop update_install`(다운로드·서명 실패 시 설치 안 함, 다운로드 뒤 저장 안 된 창이 생기면 설치 안 함, 검증된 바이트 그대로 설치, 한 번에 하나·재시도, 크기 상한, 버전·주소 일치, 플러그인 오류 문구, 업데이트 설정 고정: 공개 키·엔드포인트·`requireSignedVersion`·위험 옵션 없음·capability에 `updater:` 없음·`createUpdaterArtifacts` 미설정), `cargo test -p easyoffice-desktop windows`(`has_unsaved`), `cargo test -p easyoffice-desktop shell_caller`(모든 명령이 토큰을 받음), `cargo test -p easyoffice-desktop smoke`(가짜 서버 왕복·404, 요청 줄·`User-Agent`·`Accept` 검사, 단계 장부·표식; 업데이터 플러그인 설정이 맞아야 앱이 시작함), 루프백 훅의 주소 거부(`update_check`), `node --test scripts/test-webui-update-notice.mjs`(카드 DOM·참조 앱 클래스·Esc·나중에·설치 시작·저장 안 된 창 거부 문구·진행률 폴링·실패 재시도·재시작 보류·모르는 상태 무시·토큰 전용), `node --test scripts/test-release-assets.mjs`(자산·서명 이름, 서명 없는 번들 거부, `latest.json` 구조·플랫폼 키·서명 필드·버전 불일치 거부, 체크섬에 모두 포함, workflow의 서명 환경 변수·키 필수·`createUpdaterArtifacts`·`latest.json` 순서), `node --test scripts/test-verify-linux-bundles.mjs`(서명 파일 필수), 명령·capability 1:1 테스트.
- **Revisit when** 첫 서명 릴리스(`publish=false` 시험 실행 후)에서 실기기 설치·재시작을 확인한 뒤(Windows NSIS `passive`, macOS `.app` 교체, AppImage 교체, deb·rpm의 권한 상승 창), Apple Developer ID 서명·공증을 넣을 때(macOS 업데이트 경로 재확인), 서명 키를 교체할 때(공개 키와 이전 설치본의 수동 재설치), 릴리스에 사전 배포 채널을 둘 때.

## PD-104. 한글 창 인쇄: 쪽을 나눠 받아 숨은 루트에서 창 인쇄, 취소 가능

- **Status** Implemented (브라우저 헤드리스에서 파일 메뉴 → 숨은 루트·@page 규칙 생성까지 확인, 데스크톱 실제 인쇄 대화상자는 Windows·macOS 수동 확인 전). (6)은 PD-139가 개정합니다: 데스크톱 셸은 `window.print()` 대신 `print_webview` 명령으로 인쇄합니다.
- **Context** 한글 편집기는 `sandbox` iframe(팝업 거부, PD-083) 안에서 돌고, 편집기 안의 인쇄 명령은 포함 모드에서 숨겨져 있어(`EMBED_HIDDEN_FILE_COMMAND_IDS`) 한글 창에는 인쇄가 없었습니다. 참조 앱은 모든 쪽 SVG를 숨은 루트에 쌓고 쪽 크기로 `@page`를 만들어 창을 인쇄합니다. 같은 방식을 가져오되 큰 문서와 취소를 더 안전하게 다룹니다.
- **Decision (`PD-104`)** (1) 파일 메뉴에 `인쇄…`(`print` 셸 명령, 셸 허용 목록 7개)와 한글 모듈의 `Ctrl+P`(편집기에서 전달, 브라우저 예약 키라 데스크톱 셸에서만)를 둡니다. 편집기는 이름만 전달하고 셸이 다시 검사합니다. (2) 셸의 `hwp-print.js`가 임베드 RPC `getPrintPages({from,count})`(capability `print-pages-v1`, `count` 1..=10)로 쪽을 **10쪽씩 순서대로** 받습니다. 편집기는 한 묶음을 인쇄 프로필·이식 가능 지표(`withPortableMetrics`) 한 번에 그려 SVG와 용지 크기(mm, 표준 용지로 맞춤)를 돌려줍니다. 문서 전체 SVG를 한꺼번에 만들거나 보내지 않습니다. (3) 셸은 각 쪽 SVG를 정리(script·foreignObject·iframe 등 제거, `on*`·`javascript:`·외부 참조 제거, 쪽마다 id 이름 공간 분리)해 `#eo-print-root`에 더합니다. 쪽에 쌓는 SVG 글자 수는 96Mi 글자로 제한하고 넘으면 안내와 함께 중단합니다. 용지 크기 값은 10..=2000mm만 받습니다. (4) 용지 크기마다 `@page` 이름 규칙과 쪽 상자 규칙을 하나씩, 첫 크기는 이름 없는 기본 `@page`로도 더합니다(이름 있는 쪽을 모르는 엔진 대비). 규칙은 생성형 스타일시트(CSSOM)로만 더하며 style 요소·속성은 쓰지 않습니다. 쪽 상자는 용지보다 0.1mm 낮게 잡아 빈 쪽을 막습니다. 인쇄용 정적 규칙은 `shell.css`의 `@media print`입니다. (5) 진행: "인쇄 준비 중… (i/n)" 카드와 **취소** 단추(Esc도 같음). 취소하면 가져오기를 멈추고 숨은 루트와 스타일시트를 지우며, 같은 시각에 인쇄가 이미 준비 중이면 새 요청은 안내만 합니다. (6) 준비가 끝나면 창의 `print()`를 부르고 `afterprint`(없으면 5분 뒤)에 지웁니다. ~~데스크톱 웹뷰는 `print()`를 모든 플랫폼에서 지원하므로 새 셸 명령·capability는 없습니다.~~ (PD-139로 개정: macOS 웹뷰에서 `window.print()`는 아무 일도 하지 않아 셸 명령 `print_webview`를 씁니다.) `print.html`·`print.css`·`print-pages.ts`는 업스트림 포크 그대로 두며(Sonar 제외 대상) 포함 모드에서는 쓰이지 않습니다.
- **Why** 한글 창에서 문서를 종이나 PDF로 낼 수 있어야 하고, 큰 문서에서도 메모리를 제한하며 취소할 수 있어야 하기 때문입니다.
- **Trade-offs** 쪽 DOM은 인쇄가 끝날 때까지 한 번에 창에 있어야 해서 아주 큰 문서는 상한에서 중단됩니다. 쪽마다 크기가 달라도 `@page` 이름 규칙은 엔진에 따라 무시될 수 있습니다(그때는 첫 용지 크기).
- **Alternatives rejected** 편집기 안의 `file:print`(팝업 거부로 불가), iframe 인쇄(웹뷰별 동작 차이), 전체 쪽을 한 번에 받기(메모리·메시지 크기), 새 셸 `print_webview` 명령(웹 `print()`로 충분; PD-139가 뒤집음).
- **Verified by** `node --test scripts/test-webui-hwp-print.mjs`(10쪽 배치 순서, 크기별 규칙, SVG 정리·id 분리, 취소·Esc·중복 요청, 지원 없음·빈 문서·상한·잘못된 쪽, 인쇄 실패·afterprint 정리, 단축키·capability), `node --test scripts/test-hwp-studio.mjs`(배치 mm·끝 처리·잘못된 범위, 라우터·capability), `scripts/test-webui-hwp-shell.mjs`(셸 명령 7개), 헤드리스 브라우저 확인(메뉴 → 숨은 루트).
- **Revisit when** 쪽 범위·매수·양면 같은 인쇄 옵션을 앱에서 받을 때, 웹뷰가 이름 있는 `@page`를 지원하지 않는 사례가 확인될 때.

## PD-105. 한글 창의 "도구 상자 라벨 표시" 토글, 다른 창에는 넣지 않음

- **Status** Implemented (한글 편집기 오버레이)
- **Context** 참조 앱은 보기 메뉴에 "도구 상자 라벨 표시"를 두고 아이콘 도구 상자의 글자 라벨과 서식 줄의 리본 라벨을 숨깁니다. 한글 편집기(rhwp-studio)의 도구 상자는 아이콘 아래에 `.tb-label`을 갖지만 이 토글은 참조 앱 쪽 기능이라 편집기에는 없었습니다. 문서·시트·슬라이드·PDF 창(DOM 메뉴)의 도구 상자는 처음부터 아이콘만 있어 숨길 라벨이 없습니다.
- **Decision (`PD-105`)** (1) 편집기 오버레이의 보기 메뉴에 체크 항목 `view:toolbar-labels`를 넣고, 명령은 편집기 안에서 처리합니다(셸 RPC·셸 명령 추가 없음). (2) `<html data-toolbar-labels="shown|hidden">`가 상태이며 `hidden`이면 `#icon-toolbar .tb-label`, `#style-bar .sb-ribbon-label`을 숨기고 아이콘 도구 상자 높이를 32px로 합니다(참조 앱 CSS 그대로, 색 없음). (3) 선택은 창 표시 선호이므로 `localStorage`의 `toolbar-labels`에 저장합니다. 값이 정확히 `hidden`일 때만 숨기고, 읽기·쓰기가 실패하면 기본값(표시)으로 두며 쓰기 실패여도 이번 창에서는 바뀝니다. 다른 창의 `storage` 이벤트로 동기화합니다. (4) DOM 메뉴 창에는 항목을 넣지 않습니다(숨길 라벨이 없어 눌러도 아무 일도 없는 항목이 되기 때문). macOS 네이티브 메뉴도 같은 이유로 해당 없음이며, 이 항목은 편집기 안에만 있습니다. 나중에 DOM 도구 상자에 라벨을 도입하면 그때 함께 넣습니다.
- **Why** 한글 창에서 참조 앱과 같은 화면 밀도 선택을 주되, 라벨이 없는 창에 의미 없는 항목을 만들지 않기 위함입니다.
- **Trade-offs** 저장소가 창(편집기 iframe)의 출처에 묶여 한글 창들 사이에서만 공유됩니다.
- **Alternatives rejected** 셸이 선호를 저장해 편집기에 밀어 넣기(RPC 추가), DOM 창에도 항목 추가(동작 없음).
- **Verified by** `node --test scripts/test-hwp-studio.mjs`(토글 왕복과 저장값, 저장 실패 시 기본값, 메뉴·명령·CSS·매니페스트 일치), `node scripts/build-hwp-studio.mjs`(번들 빌드와 검사).
- **Revisit when** DOM 창 도구 상자에 글자 라벨을 넣을 때.

## PD-106. 시작 창 최근 문서 카드의 미리보기: 파일에 든 그림만 읽어 보여 줌

- **Status** Implemented (합성 컨테이너와 페이지 가짜 환경 테스트; 실제 문서·Windows·macOS 화면 확인 전). 한글 파일은 그림이 없을 때 첫 쪽을 그리도록 PD-142가 개정합니다(아래 Context의 "셸에 쪽을 그리는 엔진이 없다"와 Trade-offs의 "그림이 없는 파일은 스켈레톤" 중 한글 문서 부분, 페이지의 Base64 상한은 1MiB).
- **Context** 참조 앱은 Rust 네이티브 rhwp로 첫 쪽 SVG를 그려 카드에 보입니다. EasyOffice의 셸에는 쪽을 그리는 엔진이 없고(한글 WASM은 편집기 안에만 있음), 문서·시트·슬라이드·PDF를 그리는 경로도 없습니다. 페이지에 파일 바이트를 넘기는 것은 PD-088의 경계(경로·내용은 Rust 안)를 넓힙니다. 한글(`PrvImage`)·HWPX(`Preview/PrvImage.*`)·OOXML(`docProps/thumbnail.*`)·ODF(`Thumbnails/thumbnail.png`) 파일은 만든 프로그램이 넣어 둔 작은 미리보기 그림을 이미 갖고 있습니다.
- **Decision (`PD-106`)** (1) 새 명령 `get_recent_preview(id)`는 **불투명 id만** 받습니다(형식 `[0-9a-f]{16}` 검사, 아니면 오류). 최근 목록에 없으면 `null`입니다. 경로와 문서 바이트는 페이지로 가지 않고, 돌려주는 것은 그림의 종류(`image/png|jpeg|gif|bmp`)와 Base64뿐입니다. (2) `preview/`가 파일을 **시크로만** 읽습니다: OLE2는 머리글·FAT 목록·디렉터리·`PrvImage` 스트림(작으면 미니 스트림)만, ZIP은 끝 레코드·중앙 디렉터리·그 항목 하나만 읽으며(저장 또는 deflate, 암호·ZIP64는 무시) 256MiB 문서도 수십 KB만 읽습니다. (3) 상한: 그림 256KiB(압축·풀린 크기 모두, deflate는 상한에서 중단), 디렉터리 항목 4096개, FAT 구역 8192개, DIFAT 256개, 사슬 65536칸, ZIP 항목 8192개·중앙 디렉터리 4MiB, 모든 오프셋·크기 계산은 `checked_*`, 사슬 순환은 칸 수 상한으로 끊습니다. (4) 종류는 **앞 바이트(매직)** 로만 정합니다: PNG·JPEG·GIF·BMP만 받고 WMF·EMF·그 밖은 버립니다(이름의 확장자는 후보 선택에만 씀). (5) 캐시는 id별 1개, 파일의 수정 시각과 크기가 같을 때만 유효하고 최대 10개이며, 그림이 없다는 결과도 기억해 같은 파일을 다시 읽지 않습니다. (6) 페이지는 카드가 그려진 뒤(`setTimeout 0`) **한 장씩 차례로** 요청하고, 응답의 `createImageBitmap(new Blob([bytes], {type}))` 결과를 카드 안 `<canvas>`에 그립니다. URL을 만들지 않으므로 페이지 CSP(`default-src 'self'`)의 `img-src`와 무관하며 CSP는 바꾸지 않습니다. Base64는 256KiB 상한을 넘으면 디코드 전에 버립니다. 요청 실패·그림 없음·디코드 실패·`createImageBitmap` 미지원이면 그 카드는 스켈레톤을 유지하고 다른 카드는 계속 진행합니다. 목록을 다시 그리면 이전 실행은 멈춥니다. 개발용 `--serve`에서는 요청하지 않습니다. (7) 명령은 앱 명령 목록과 capability에 1:1로 추가했습니다(11개).
- **Why** 엔진 무의존과 "경로·내용은 Rust 안" 원칙을 지키면서 카드에 실제 모습을 보이기 위함이며, 한 파일의 그림 하나만 읽어 시작 창 비용이 거의 들지 않습니다.
- **Trade-offs** 그림이 없는 파일(다른 프로그램이 만든 것, 이 앱이 저장한 파일, PDF)은 스켈레톤으로 남고, 그림은 저해상도이며 문서와 어긋날 수 있습니다. 그림은 문서 내용의 일부이므로 시작 창 화면과 메모리 캐시에만 있고 디스크에 쓰지 않습니다.
- **Alternatives rejected** 페이지에서 한글 WASM으로 첫 쪽을 그리기(바이트가 페이지로 가고 11.8MB 코어를 시작 창에서 불러옴), 엔진에 쪽 그리기를 새로 만들기(규모), `data:`/`blob:` 이미지(CSP를 풀어야 함).
- **Verified by** `cargo test -p easyoffice-desktop preview`(합성 OLE2 미니·일반 스트림, ZIP 저장·deflate, 매직·확장자·상한·압축 폭탄, 잘린·순환·잘못된 FAT·ZIP64, id 형식, 캐시 키·10개·미적중 기억, Base64 응답), 앱 명령·capability 1:1 테스트, `node --test scripts/test-webui-home-preview.mjs`(순차 요청·캔버스 교체, 실패별 스켈레톤 유지, 큰 응답 거부, 디코더·명령 없음, 새 목록의 취소, CSS 토큰).
- **Revisit when** 그림 없는 파일이 많아 실제 쪽 미리보기가 필요할 때(한글만 편집기 코어로 그리는 안을 다시 검토).

## PD-107. PR 커밋 메시지 가드와 웹 UI 테스트 전체 실행

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

## PD-108. 앱 아이콘을 rhwp 강조색으로

- **Status** Implemented
- **Context** 앱 아이콘과 파비콘이 폐기된 라벤더(#5C6BC0, PD-098 이전 디자인 시스템)로 남아 있었습니다.
- **Decision (`PD-108`)** 사용자 선택 L-3안입니다. 문서 모양(접힌 모서리, 줄 표시 세 개)은 유지하고 글자는 넣지 않습니다. 바탕은 rhwp 강조색의 세로 그러데이션(#6182D6 → #3E62BB), 종이는 흰색, 접힌 모서리는 #D6DFF5, 줄은 #335095(긴 줄 두 개)와 #6182D6(짧은 줄)이고 줄 끝은 둥글게 합니다. 바탕 사각형은 참조 셸(참조 앱) 아이콘과 같은 비율입니다(1024 기준 64 안쪽, 896 크기, 모서리 반지름 214). 원본은 `crates/easyoffice-app/ui/favicon.svg`(1024 단위) 하나이고, 데스크톱 아이콘(PNG 32·128·256·512, Windows ICO, macOS ICNS)은 `node scripts/generate-desktop-icons.mjs`가 같은 좌표로 도형을 해석적으로 그려 결정론적으로 만듭니다(`--check`로 최신 여부 검사).
- **Why** 앱 안의 색(rhwp 토큰)과 아이콘을 맞추고, 기존 사용자에게 익숙한 문서 모양을 지키기 위함입니다. 글자(E·Easy)는 작은 크기에서 읽히지 않고 문서 모양과 어울리지 않아 넣지 않습니다.
- **Trade-offs** 참조 앱의 단색 자모 아이콘과 달리 문서 모양을 씁니다.
- **Verified by** `node scripts/generate-desktop-icons.mjs --check`, `cargo test -p easyoffice-desktop`(ICNS 포함 검사), 위생 가드, 16·32·64px 육안 확인.
- **Revisit when** 브랜드 아이콘을 새로 디자인할 때.

## PD-109. 저장 안정성: 내용 해시 지문, 고유 임시 파일, 저장 전 구조 검증 (#227)

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

## PD-110. 메뉴·명령 팔레트 키보드 접근성, 찾기 표기, 사용자 문구

- **Status** Implemented
- **Context** 페이지 메뉴(`--serve`·브라우저에서 쓰임)의 항목이 마우스로만 열리는 `div`였고, 명령 팔레트 목록에는 역할 정보가 없었습니다. 편집→찾기와 Ctrl/Cmd+F는 문서·시트·슬라이드에서 실제로는 명령 팔레트를 여는데 이름은 "찾기"였습니다. 오류 문구에는 `--serve`, `엔진 응답`, `데스크톱 셸`, `IPC` 같은 내부 용어가 그대로 보였습니다.
- **Decision (`PD-110`)**
  1. **메뉴 키보드 탐색(`ui/menu-keyboard.js`, 새 모듈)**: 메뉴 열림 상태·마우스 동작·키보드 탐색을 한 곳에서 맡습니다. 모든 항목이 `role="menuitem"`(라디오는 `menuitemradio`)과 `tabindex="-1"`을 갖고, 메뉴 제목은 `aria-haspopup="menu"`와 `aria-expanded`를 갖습니다. 제목에서 좌/우/Home/End는 메뉴 이동(열려 있으면 이웃 메뉴를 엶), 아래/위/Enter/Space는 메뉴를 열어 첫/마지막 항목에 포커스합니다. 항목에서 위/아래/Home/End는 이동(끝에서 돌아옴), 오른쪽은 하위 메뉴 열기(없으면 다음 메뉴), 왼쪽은 하위 메뉴에서 나오기(없으면 이전 메뉴), Enter/Space는 실행입니다. 실행하면 메뉴를 닫고 포커스를 메뉴 제목으로 돌려 놓은 뒤 항목을 실행하므로 대화상자가 뜨면 대화상자가 포커스를 가져갑니다. Esc는 가장 안쪽 단계만 닫고(하위 메뉴 → 메뉴) 메뉴 제목으로 포커스를 돌려줍니다. 비활성 항목(`aria-disabled="true"`)은 포커스할 수 있지만 실행되지 않습니다. 다른 모듈이 메뉴를 닫을 때도 `aria-expanded`가 맞도록 한 함수(`__EASYOFFICE_CLOSE_MENUS__`)를 씁니다. 참조 앱의 메뉴는 마우스 전용 `div`라 이 부분은 참조 앱보다 낫게 갑니다.
  2. **명령 팔레트 ARIA**: 목록 `role="listbox"`, 항목 `role="option"`(고유 id, `aria-selected`), 입력 `role="combobox"`와 `aria-controls`·`aria-autocomplete="list"`, 화살표로 선택이 바뀔 때마다 `aria-activedescendant`가 선택 항목을 가리키고 결과가 없으면 제거됩니다.
  3. **찾기 표기**: 문서·시트·슬라이드·PDF에는 본문 찾기가 없고 Ctrl/Cmd+F는 명령 팔레트입니다. 새 찾기 막대를 만들지 않고 페이지 메뉴와 도구줄 단추의 이름을 실제 동작인 "명령 검색"으로 맞췄습니다. 한글 편집기 창은 편집기 자체 찾기를 그대로 씁니다(셸 메뉴 `edit:find`의 동작은 그대로이며 macOS의 전역 메뉴 라벨은 한글 창과 공유하므로 "찾기"로 둠).
  4. **사용자 문구**: 연결 실패는 "연결할 수 없습니다. 앱을 다시 시작해 주세요.", 응답 처리 실패는 "응답을 처리하지 못했습니다. 다시 시도해 주세요."처럼 사용자 행동으로 씁니다. 상태 표시줄 연결 표시는 "엔진:" 접두사 없이 "연결됨/연결 오류/연결 안 됨"입니다. 연결 실패 여부는 문구 비교 대신 오류의 `connection` 표시로 가립니다.
  5. **`ui/ipc-transport.js`(새 모듈)**: `app.js`에 있던 엔진 요청 전송부(시험 훅·셸 다리·로컬 HTTP 선택, 실패 표시)를 옮겼습니다. 동작 변경은 없고 `app.js`는 1,000줄 상한에서 멀어졌습니다.
- **Why** 키보드만으로 메뉴와 팔레트를 쓸 수 있어야 하고, 이름이 동작과 같아야 하며, 오류 문구는 사용자가 할 일을 말해야 합니다.
- **Trade-offs** 본문 찾기(문서 텍스트 검색)는 없습니다. 만들려면 새 찾기 막대가 필요하고 시각 승인 대상입니다. 네이티브 메뉴 라벨은 "찾기"로 남습니다.
- **Verified by** `node --test scripts/test-webui-menu-keyboard.mjs`(항목 역할·tabindex, 열기·이동·돌아옴, 하위 메뉴, Esc 단계와 포커스 복귀, 메뉴 간 이동, 비활성 항목, 팔레트 ARIA와 화살표, 찾기 표기, 사용자 문구 가드).
- **Revisit when** 문서·시트·슬라이드 본문 찾기를 넣을 때.

## PD-111. 한글 창의 표·배율 메뉴가 편집기 명령에 닿는다

- **Context** 네이티브 메뉴의 `실제 배율`(`Cmd+0`)·확대·축소·쪽 맞춤·폭 맞춤은 한글 편집기가 창을 차지하면 페이지의 `studioCommandApplies`가 거부해 회색이거나 아무 일도 하지 않았고, 표 메뉴에는 참조 앱의 셀 선택·셀 합치기·셀 나누기가 없었습니다.
- **Decision (`PD-111`)** (1) 표 메뉴에 `셀 선택`(`CmdOrCtrl+Alt+T`), `셀 합치기`(기존 `table:merge`), `셀 나누기`를 참조 앱 순서로 둡니다. (2) 한글 편집기 창(`HWP_EMBED`)에서는 배율 5개와 셀 3개를 참조 앱과 같은 명령 id(`view:zoom-in`·`view:zoom-out`·`view:zoom-100`·`view:zoom-fit-page`·`view:zoom-fit-width`, `table:cell-selection-enter`·`table:cell-merge`·`table:cell-split`)로 임베드 포트의 `automation.execute`에 보냅니다. 이 대응표는 `shell-chrome.js`의 `EMBED_EDITOR_COMMANDS` 하나입니다. (3) 새 두 항목(`table:cell-select`, `table:cell-split`)은 편집기에만 있는 동작(`hwp-editor`)이라 다른 모듈 창에서는 회색입니다. 편집기가 그 순간 실행할 수 없는 명령(셀을 고르지 않은 합치기 등)은 편집기가 거절하고 셸은 무시합니다. (4) 편집기에는 `table:cell-selection-enter`가 없어(참조 앱도 같은 id만 보냅니다) 오버레이가 이 명령을 등록하고, 셀 선택을 F5 키 경로로 실행합니다. 글 상자·그림 항목은 참조 앱에 없어 넣지 않습니다.
- **Why** 참조 앱과 같은 메뉴·같은 명령 id로 맞추고, 기존 임베드 경로(`찾기`가 이미 쓰는 `automation.execute`)를 그대로 씁니다.
- **Trade-offs** 항목의 회색 처리는 창의 모듈 단위입니다. 커서가 표 안인지 같은 편집기 상태는 셸에 보고하지 않습니다.
- **Verified by** `node --test scripts/test-webui-native-menu.mjs scripts/test-webui-menu-state.mjs scripts/test-hwp-studio.mjs`, `cargo test -p easyoffice-desktop native_menu`.
- **Revisit when** 편집기 상태에 따라 메뉴를 회색 처리해야 할 때.

## PD-112. 한글 저장 검증의 크기 상한은 편집기가 여는 상한과 같다

- **Status** Implemented, 개정 (QA 요청 2026-11-09): 합계 상한을 텍스트와 그림으로 나눴습니다. 처음 결정(항목당 512MiB 한 상수)은 그대로 유지합니다.
- **Context** 저장 전 구조 검증이 엔진 기본값(항목당 128MiB)을 그대로 써서, 편집기(rhwp)가 넣을 수 있는 더 큰 그림을 가진 정상 문서를 저장하지 못했습니다. 참조 앱의 저장 검증은 "다음에 여는 엔진이 받아들이는가"입니다. 첫 개정 뒤에도 전체 합계 512MiB 하나가 남아, 그림 합계가 큰 문서(압축이 잘 되는 비트맵 여러 장)는 rhwp가 여는데도 저장이 거부될 수 있었습니다. rhwp 리더(`src/parser/hwpx/reader.rs`)는 XML 항목당 256MiB·XML 합계 512MiB, 그림(BinData) 항목당 512MiB이고 그림 합계 상한이 없습니다(그림은 쪽이 필요할 때 하나씩 풂, 원본 컨테이너만 보관).
- **Decision (`PD-112`)** (1) 한글(HWPX·HWTX)의 저장 검증 정책은 `ZipSecurityPolicy::hangul_editor()` 하나입니다. 이름이 `.xml`·`.hpf`로 끝나는 항목은 텍스트: 항목당 256MiB(`HANGUL_EDITOR_MAX_XML_ENTRY_BYTES`), 합계 512MiB(`HANGUL_EDITOR_MAX_XML_TOTAL_BYTES`). 나머지 항목은 그림 등 바이너리: 항목당 512MiB(`HANGUL_EDITOR_MAX_ENTRY_BYTES`), 합계는 `HANGUL_VERIFY_MAX_BINARY_TOTAL_BYTES`(4GiB, 항목 상한의 8배). 항목 수(4096)와 빈 항목 검사, 압축비 규칙 없음(PD-109)은 그대로입니다. (2) 4GiB는 편집기의 상한이 아니라 검증 자체의 작업량 상한입니다. 근거는 측정입니다(릴리스 빌드, 이 Mac): 평평한 512MiB 그림 8장(4GiB, 파일 4.2MB)을 검증하는 데 0.6초, 최대 메모리 539MiB(항목 하나 분량). 압축이 거의 안 되는 데이터는 0.37GiB/s이지만 저장 요청 상한(256MiB, 압축 바이트)이 그 출력을 막습니다. 저장 바이트 자체는 선언 크기로 먼저 검사하므로, 선언이 상한을 넘는 폭탄은 풀기 전에 거부합니다. (3) 검증은 풀어서 보관하지 않고 항목 하나씩 풀어 CRC·크기만 확인한 뒤 버립니다(`ZipArchiveCodec::verify`). 메모리는 합계가 아니라 가장 큰 항목입니다. 항목이 선언한 크기로 출력 버퍼를 한 번에 잡습니다(`inflate_raw_sized`; 선언은 힌트일 뿐 상한은 그대로). 항목마다 보관하지 않아도 크기를 두 배씩 키우던 버퍼는 macOS에서 놓친 블록이 반환되지 않아 같은 4GiB 검증이 최대 4.3GiB를 점유했고(측정), 한 번에 잡으면 539MiB입니다.
- **Why** 저장 검증 상한이 여는 쪽보다 작으면 편집기가 만든 파일을 거부하고, 크면 못 여는 파일을 저장합니다. 편집기 XML 항목 상한(256MiB)과 합계는 페이지가 저장 직전 같은 바이트를 rhwp로 다시 파싱해 걸러내므로 같은 값을 둬도 정상 문서를 막지 않습니다. 그림 합계를 없애지 않고 둔 이유는 신뢰할 수 없는 입력 예산(Fail-Closed)을 유지하기 위해서입니다.
- **재사용 판단** (1) 허용 라이선스의 크레이트: 셸이 이미 가진 rhwp의 `HwpxReader`(`zip` 크레이트)를 직접 쓰면 "여는 엔진" 그대로지만 우리 ZIP 보안 검사(로컬·중앙 헤더 일치, 중복 경로, 경로 순회)를 잃고 파일 전체를 복사합니다. (2) 이미 쓰는 의존성: 엔진의 `ZipArchiveCodec`에 보관하지 않는 `verify`와 분리된 합계(`ZipBinaryBudget`)만 더했습니다. (3) 이식은 하지 않았고 rhwp의 상한값만 따랐습니다(출처는 `zip_policy.rs` 머리 주석).
- **Trade-offs** 엔진 자체 HWPX 읽기(변환 경로)는 기본 128MiB·합계 512MiB 그대로라 그보다 큰 그림의 문서는 변환에서 거부됩니다. 미리보기 그림을 넣는 재작성(`embed_thumbnail`)은 전부 보관해야 해서 64MiB 이하 문서만 다루고 기본 합계를 씁니다. 이름이 `.xml`·`.hpf`가 아닌 텍스트 항목은 그림 쪽 상한으로 셉니다(rhwp보다 느슨). `.xml`이지만 큰 항목은 rhwp처럼 256MiB 한 항목 상한에 걸려, 예전 512MiB 개정에서 통과하던 257~512MiB XML 항목은 이제 거부됩니다(rhwp도 열지 못함).
- **Verified by** `cargo test -p easyoffice-desktop save_verify`(선언 크기 기준: 그림 항목 512MiB 통과·초과 거부, XML 항목 256MiB 통과·초과 거부, XML 합계 512MiB 통과·1바이트 초과 거부, 그림 합계 1000MiB 통과, 8×512MiB 통과·1바이트 더 거부, 텍스트와 그림 예산 분리, 폭탄 선언 거부, 실제 압축 폭탄 헤더 거부, 실제 600MiB 그림 묶음 통과), `cargo test -p easyoffice-formats`(`verify`·분리 예산·`inflate_raw_sized`).
- **Revisit when** rhwp 상한이 바뀌거나 엔진 HWPX 읽기를 편집기와 같은 상한으로 올릴 때, 그림 합계 4GiB가 정상 문서를 막는다는 신고가 있을 때.

## PD-113. 저장 확인 중의 두 번째 닫기는 닫지 않고, 응답 없는 페이지만 시간 뒤에 닫음

- **Context** 저장 확인이 떠 있는 창에 닫기가 다시 오면(Cmd+W, 닫기 버튼, Cmd+Q, 끝내기) 셸이 확인 없이 창을 닫아 저장 안 된 내용을 잃을 수 있었습니다. 그 동작은 페이지가 멈춰 확인을 못 띄울 때 창을 닫을 수 있게 하려는 탈출구였습니다. 참조 앱은 닫기 확인을 전부 페이지에서 처리하고(`CloseRequested`를 막고 확인을 기다림) 확인 중인 창에 다시 오는 요청이나 응답 없는 페이지를 위한 시간 제한이 없으며, 종료는 이미 진행 중이면 두 번째 요청을 무시합니다. 멈춘 페이지는 닫을 수 없습니다.
- **Decision (`PD-113`)** (1) 확인이 떠 있는 동안의 닫기 요청은 창을 닫지 않고 앞으로 가져오며 확인을 그대로 둡니다(`CloseAction::KeepPrompt`). (2) 페이지는 저장 확인을 띄울 때 새 명령 `ack_close_prompt`(인수 없음)로 셸에 알립니다. 셸은 확인을 요청한 시각과 응답 여부를 창마다 기억합니다. (3) 응답이 없는 확인에 닫기가 다시 오면 `CLOSE_ACK_TIMEOUT`(10초) 전에는 확인을 다시 요청하고, 그 뒤에만 확인 없이 닫습니다. 응답이 늦게라도 오면 그 뒤로는 (1)입니다. (4) 취소·저장 실패(`end_close_prompt`)와 저장 완료 보고(수정 없음)는 기억을 지워, 다음 닫기는 처음처럼 확인을 요청합니다. 수정 있음 보고는 기억을 건드리지 않습니다. (5) 끝내기·Cmd+Q도 같은 `CloseRequested` 경로로 창을 닫으므로 같은 규칙이 적용됩니다. (6) 셸은 확인을 유지할 때도 페이지의 닫기 요청 스크립트를 다시 실행하고, 페이지는 이미 확인이 떠 있으면 같은 확인에 합류하면서 응답을 다시 보냅니다(페이지가 먼저 띄운 확인이나 대기 상태가 지워졌다 다시 생긴 경우도 보호됨). (7) 페이지가 (다시) 로드되기 시작하면 셸은 그 창의 대기 상태를 지웁니다. 다시 로드된 페이지는 확인을 잃었기 때문입니다.
- **Why** 저장 안 된 내용을 잃는 쪽이 창이 잠시 안 닫히는 쪽보다 큽니다. 시간 값은 데스크톱 시스템이 창을 응답 없음으로 보는 기준(약 5초)의 두 배로 두어, 오래 걸리는 계산 중인 페이지를 멈춘 것으로 잘못 보지 않게 했습니다. 10초 뒤에도 응답이 없는 페이지는 확인을 띄울 수 없는 상태로 봅니다.
- **Trade-offs** 응답(ack)을 보낸 뒤 멈춘 페이지는 참조 앱처럼 닫히지 않습니다(재로드나 프로세스 종료가 탈출구). 응답 알림을 보내지 못하는 페이지(명령 호출 실패)는 10초 뒤 두 번째 닫기로 확인 없이 닫힐 수 있습니다. 페이지가 확인을 띄우는 중에 시간 제한이 지나면(10초 넘게 걸리는 첫 응답) 같은 위험이 있습니다. 참조 앱과 달리 멈춘 페이지를 닫을 수 있게 하는 대가입니다.
- **Verified by** `cargo test -p easyoffice-desktop windows::`(확인 중 두 번째 닫기는 유지, 응답 없는 페이지는 시간 뒤 닫힘, 늦은 응답, 응답 뒤 취소는 상태 해제, 응답은 요청 없이 효과 없음), `node --test scripts/test-webui-windows.mjs scripts/test-webui-menu-state.mjs`(확인을 띄울 때 한 번 알림, 겹친 요청은 같은 확인에 합쳐짐, 수정 없는 창은 알림 없음), 끝내기 경로는 `app_quit` 구조 테스트(`raise_and_close`만 씀).
- **Revisit when** 멈춘 페이지 감지를 시간 말고 페이지의 심박 같은 신호로 바꿀 때, 시간 값이 실제로 너무 짧거나 길다는 신고가 있을 때.

## PD-114. 검색 가능한 PDF를 참조 앱 수준으로: 번들 글꼴, 글자별 대체 글꼴, 병렬 변환

- **Context** 참조 앱의 PDF 내보내기(`pdf_export.rs`와 rhwp PDF 변환기)와 비교한 결과 우리에게 빠진 것은 (a) 한국어 글꼴이 없는 환경을 위한 번들 글꼴(참조 앱은 `fonts/pdf/`에 Noto Sans KR Regular TTF와 OFL을 싣습니다. PD-101은 TTF를 싣지 않았고 `pdf-fonts/` 폴더는 어디에도 없었습니다), (b) 쪽에 없는 글자를 다른 글꼴에서 찾는 색인(없으면 글자마다 설치된 모든 글꼴을 다시 열어 봅니다), (c) WMF·EMF 그림이 SVG 안의 SVG로 들어온 경우 안의 비트맵을 읽는 해석기, (d) 글꼴 서브셋 실패를 쪽 단위로 다루는 처리였습니다. 300쪽 합성 보고서(쪽당 46줄)를 내보내는 데 108.6초(시스템 시간 97초, 최대 상주 메모리 169MB)가 걸렸는데, 원인은 글꼴 파일을 접근마다 열고 매핑하고 해제하는 fontdb의 동작이었습니다. 링크 주석과 쪽 범위는 참조 앱에도 없습니다: rhwp 변환기는 링크 목록을 받을 수 있지만 참조 앱은 빈 목록을 넘기고, 쪽 범위는 백엔드 인수만 있고 화면이 항상 `null`을 넘깁니다.
- **Decision (`PD-114`)** (1) 설치본에 `pdf-fonts/NotoSansKR-Regular.ttf`와 `NotoSansKR-OFL.txt`를 싣습니다(`tauri.conf.json` resources, 원본은 rhwp `ttfs/opensource/`이므로 `config/rhwp-upstream.json`의 sparse 목록에 TTF를 더함). PD-101 (7)의 "TTF는 번들하지 않고"는 이 항목으로 바뀌며, 시스템 한국어 글꼴이 우선이 아니라 번들 폴더가 글꼴 DB에 먼저 들어가고 일반 serif/sans/monospace는 Noto Sans KR로 갑니다. 스모크 `pdf` 단계는 DB에 번들 폴더의 글꼴이 있는지 확인하고 없으면 실패합니다(시스템 글꼴이 있는 러너에서도 설치 누락이 드러나도록). (2) 쪽에 없는 글자의 대체 글꼴은 rhwp `PdfFontFallbackIndex`를 옮긴 색인(`pdf_fonts.rs`)으로 찾습니다: (글자, 글꼴) 조합마다 한 번만 열고 결과를 비트로 기억하며, 서로 다른 글자 4,096개 또는 조회 262,144회를 넘기면 색인을 버리고 usvg 기본 선택기로 넘어갑니다(결과는 같고 느릴 뿐). (3) 내보내기 한 번 동안 글꼴 DB의 파일을 읽기 전용으로 매핑해 둡니다(`with_mapped_faces`, 내보내기가 끝나면 해제). (4) `data:image/svg+xml` 그림은 안의 `<image>`까지 읽되(rhwp `pdf_image_href_resolver`) 중첩 깊이 4를 넘는 그림은 버립니다. 파일 경로·`file:` href는 쪽과 중첩 SVG 모두에서 읽지 않습니다(쪽 SVG는 신뢰하지 않는 입력이며 편집기는 그림을 전부 데이터 URL로 넣습니다). rhwp 원본은 데이터가 깊은 만큼 재귀합니다. (5) 쪽의 글꼴 서브셋이 실패하면 그 쪽만 글자를 윤곽선으로 바꿔 다시 변환합니다(쪽은 맞게 보이지만 그 쪽은 검색되지 않음). 다시 실패하거나 다른 오류면 쪽 번호가 든 오류로 전체를 실패시킵니다: 참조 앱은 두 번째 실패에서 쪽을 건너뛰지만 쪽이 빠진 PDF는 만들지 않는다는 PD-101 (4)를 유지합니다. (6) 변환은 16쪽씩 묶어 최대 4개 스레드로 하고, 묶음이 끝날 때마다 쪽 객체를 출력에 바로 다시 번호 매겨 넣고 버립니다(쪽 순서와 결과 바이트는 단일 스레드와 같음). 살아 있는 변환 결과는 한 묶음뿐이라 쪽 수에 따라 메모리가 늘지 않습니다. (7) 링크 주석과 쪽 범위는 구현하지 않습니다(참조 앱에 없음). (8) 한글 창 상태줄은 쪽 SVG를 받는 동안 "PDF 만드는 중: n / N쪽"으로 쪽마다 알립니다(참조 앱이 쪽마다 진행 문구를 내는 것과 같은 수준).
- **Why** 정책값·처리 방식은 참조 앱이 같은 문제를 어떻게 풀었는지를 따른다는 원칙입니다(번들 글꼴, rhwp 색인·해석기). 편차는 우리 쪽 근거가 있는 두 곳뿐입니다: 쪽을 건너뛰지 않음, 해석기에 깊이 상한. 병렬·매핑은 측정으로 정했습니다.
- **Trade-offs** 최대 상주 메모리는 보통 문서(측정한 300쪽 보고서) 기준 442MB로, 단일 스레드(199MB)보다 4개 스레드와 매핑된 글꼴 쪽만큼 늘었고 쪽 수에는 비례하지 않습니다. 최악은 입력 상한이 정합니다: 동시에 변환하는 쪽 4개가 각각 쪽당 상한(SVG 64MiB)에 가까운 입력과 그 변환 작업 메모리를 갖고, 묶음 16쪽의 변환 결과가 출력에 들어가기 전까지 함께 살아 있습니다(입력 전체는 합계 상한 512MiB). 이 최악 값은 측정하지 않았습니다. 번들 글꼴은 Regular 한 벌의 한글·라틴·기호 부분집합이라 한자는 시스템 글꼴이 없으면 빠지고 굵게·기울임은 합성되지 않아 그 글꼴로 그려지는 굵은 글자는 보통 두께로 나옵니다. 사설 영역(PUA) 문자(①류)를 표준 문자로 바꾸는 ToUnicode 정리와 합성 굵게·기울임은 참조 앱(rhwp)에 있으나 아직 옮기지 않았습니다. 글꼴 서브셋 실패로 윤곽선이 된 쪽은 경고를 표준 오류에 남길 뿐 화면에 알리지 않습니다.
- **Verified by** `cargo test -p easyoffice-desktop pdf_`(번들 글꼴만 있는 DB에서 한글이 `ToUnicode`에 들어감, 글꼴이 전혀 없으면 검사가 경고, 색인이 기본 선택기와 같은 글꼴을 고름·상한을 넘으면 기본 선택기로 넘어감, 적대적 글자 수 입력, 중첩 그림 깊이 상한, 묶음을 넘는 쪽 순서, 뒤 묶음의 가장 앞 실패 쪽 번호, 서브셋 실패 재시도 한 번), 경로 href 무시 시험, `node --test scripts/test-webui-hwp-session.mjs`(쪽 진행 문구), `scripts/test-verify-linux-bundles.mjs`·`test-desktop-file-associations.mjs`(설치 구조 검사가 번들 글꼴과 OFL을 요구: Linux 패키지 검사, macOS 앱·NSIS 설치 파일 목록 검사는 CI·릴리스 워크플로), 측정은 `cargo test --release -p easyoffice-desktop hundreds_of_pages -- --ignored --nocapture`: 300쪽 합성 보고서 108.6초 → 3.2초(색인+매핑, 단일 스레드) → 1.1초(4스레드), 최대 상주 메모리 169MB → 199MB → 442MB.
- **Revisit when** 사설 영역(PUA) 문자 ToUnicode 정리와 합성 굵게·기울임을 옮길 때, 링크 주석이나 쪽 범위가 참조 앱에 들어올 때, 서브셋 실패 쪽을 화면에 알려야 할 때.

## PD-115. Pages 그림: 지원하지 않는 형식은 경고만, 대체 텍스트·잘림·쪽 위 개체 알림

- **Updated by PD-157**: 4항의 쪽 위 개체는 알림만 하던 것을 글상자 글·그림·표는 가져오도록 바꿨습니다.
- **Context** PD-096(그림 블록·`Data/` 읽기)과 PD-097(도형·텍스트 상자 글자)로 3단계 첫 항목은 이미 구현되어 있었습니다. 남은 틈은 그림 설명(대체 텍스트)을 읽지 않는 것, 자른 그림과 쪽 위에 놓인 개체가 조용히 빠지는 것, 악의적 패키지 시험이 없는 것이었습니다.
- **Decision (`PD-115`)** (1) 데이터가 PNG·JPEG가 아니면(매직 바이트 기준) PD-087대로 본문에 글자를 섞지 않고 건너뛰며 경고 `지원하지 않는 형식의 그림 N개`만 남깁니다. PNG·JPEG 매직 바이트는 있으나 구조가 깨졌거나 화소·크기 한도를 넘은 그림, 데이터 참조·파일 이름·파일이 없는 그림, 그림 하나 32MiB 초과도 같이 건너뛰고 경고만 남깁니다(`[그림]` 같은 자리표시 문단은 넣지 않음). (2) 그림 설명은 개체 공통부의 필드 8에서 읽어 공백을 줄이고 4,096바이트로 자른 뒤 대체 텍스트로 씁니다. (3) 자르기 마스크(그림 필드 5)가 있는 그림은 전체 그림으로 가져오고 `자른 그림(전체 그림으로 가져옴) N개`로 알립니다. (4) 쪽 배치 문서의 쪽 위 개체 목록(형식 10010)은 가져오지 않고 개체 종류별 개수를 `쪽 위의 개체: …`(끊어진 참조는 `쪽 위의 끊어진 개체 참조`)로 알립니다. 같은 개체는 한 번만 셉니다. 한도는 PD-096 그대로(문서당 256개·합계 128MiB 초과는 열기 실패, 그림당 32MiB·5,000만 화소 초과는 건너뜀)입니다.
- **Why** 변환 결과 본문에 `[그림]` 같은 글자가 섞이면 원본에 없던 글이 생기므로 PD-087의 "지원하지 않는 요소를 본문에 섞지 않는다"를 따릅니다. 빠진 그림은 경고로 알립니다.
- **Trade-offs** 새 필드 번호 3개(그림 설명, 마스크, 쪽 위 개체 목록)는 우리 실파일에 값이 없어 공개된 독립 구현(MIT)의 형식 문서에 적힌 번호를 따랐고 `pages_schema.rs`에 `검증 필요`로 표시했습니다. 번호가 틀리면 설명·알림이 비는 것으로 끝나며(잘못된 내용이 들어가지 않음) 합성 패키지로만 시험했습니다. 쪽 위 개체와 도형·표·머리글·바닥글 안의 그림은 여전히 가져오지 않습니다. 그림 복사는 하나당 한 번 남아 있으나 측정상 구조 검사(CRC)보다 훨씬 싸서(8MiB: 복사 0.12ms, 검사 포함 전체 1.09ms) 줄이지 않았습니다.
- **Alternatives rejected** 외부 크레이트로 교체(MIT 크레이트가 있으나 자체 객체 모델이라 `RichDocument` 변환·상한·fail-closed를 다시 짜야 하고 젊은 프로젝트라 의존성 비용이 큼; 필드 번호 지식만 참고), 지원하지 않는 형식을 PNG 등으로 변환(디코더 의존성과 공격 면 증가), 지원하지 않거나 깨진 그림을 자리표시 문단으로 대체(본문에 글자가 섞임).
- **Verified by** `cargo test -p easyoffice-formats --test pages_images`(설명·크기, JPEG, 이상한 기하 크기, 지원하지 않는 형식은 본문 글자 없이 경고만, 모든 지점에서 잘린 PNG·JPEG와 화소 폭탄, 끊어진 참조·`../` 이름, 257개 거부, 합계 155MiB 거부, 33MiB 건너뜀, 자기 참조 첨부·도형, 자른 그림·쪽 위 개체 알림), `pages_fixtures`(기존 그림 위치·바이트).
- **Revisit when** 쪽 배치 문서나 자른 그림, 설명이 든 실파일을 구했을 때(필드 번호 확정), 표·도형 안 그림을 모델이 담을 수 있을 때.

## PD-116. Pages 각주 번호 연결, 도형·각주·머리글·바닥글 안의 그림

- **Updated by PD-157**: 번호 형식(로마·기호·전각·한자)·미주·구역마다 다시 시작은 구현했고 쪽마다 다시 시작은 경고합니다. 각주 표식은 표 11·16과 형식 2012·2008, 문자 `U+FFFC`·`U+000E`를 모두 읽습니다.
- **Supersedes PD-097 (부분)**: 2항의 "각주는 본문의 표식 위치와 연결하지 않고 목록으로만 둔다"와 "도형 안의 첨부는 건너뜀"(1항)을 이 항목이 대체합니다.
- **Context** PD-097 이후 남은 틈은 네 가지였습니다. 각주 표식이 본문에서 사라져 부록 목록과 이어지지 않는 것, 도형·각주·머리글·바닥글 안의 그림이 경고 없이 빠지는 것(머리글·바닥글은 건너뜀 집계를 통째로 되돌렸음), 머리글·바닥글 변형(최대 18개)마다 같은 로고를 읽어 그림 합계·개수 한도에 걸려 문서가 열리지 않을 수 있는 것, 도형 안의 건너뜀이 모두 "표 안의 개체"로 잘못 분류되는 것입니다. 표 셀 종류(문자열·기간·오류)·셀 서식·글꼴·첨자·들여쓰기는 PD-097 8항과 3~5항에서 이미 구현되어 있어 이번에 바꾸지 않았습니다.
- **Decision (`PD-116`)** (1) 본문 저장소의 각주 표(저장소 필드 11)가 가리키는 위치(`U+FFFC`)에 위첨자 번호 글자 구간을 넣습니다. 번호는 각주 표 안의 순서(1부터)이고 부록의 각주 문단 앞에도 같은 위첨자 번호와 공백을 붙입니다(표시 위치와 번호가 항상 같음). 번호 형식은 숫자 연속 번호만 다룹니다. 도형·머리글 등 본문 밖 저장소의 각주 표는 읽지 않습니다. (2) 도형·텍스트 상자, 각주, 머리글, 바닥글 안의 첨부는 그림이면 가져옵니다(도형은 그 자리, 나머지는 부록 문단 뒤). 이 안의 표와 도형은 모델에 중첩이 없어 건너뛰고 `도형·각주·머리글·바닥글 안의 표·도형 N개`로 알립니다. (3) 머리글·바닥글의 쪽 번호 같은 표식은 이전처럼 알리지 않되, 그림의 건너뜀·손상·자름 경고는 항상 알립니다. (4) 머리글·바닥글에서 같은 내용이라 버려지는 그림은 개수·합계 예산을 돌려줍니다(`PageImages::release`). 본문의 같은 그림 반복은 출력에도 반복되므로 그대로 셉니다. 한도 초과는 PD-096대로 열기 실패입니다.
- **Why** 번호 연결은 변환 후에도 어느 문장의 각주인지 알 수 있게 하고, 표식이 픽스처에서 `U+FFFC`와 각주 표 위치로 일치함을 확인했습니다. 예산 반환은 보이는 결과(부록에 한 번 나오는 로고)와 한도 계산을 맞춥니다.
- **Trade-offs** 번호 형식(로마 숫자·기호), 쪽마다 번호를 다시 시작하는 설정, 문서 끝 미주는 실파일이 없어 읽지 않습니다. 각주 번호 글자 구간이 본문 글에 섞이므로 평문 변환 결과에 번호가 붙습니다(`있습니다.1`). 머리글 변형이 다른 그림을 쓰면 모두 부록에 남고 예산을 씁니다. 머리글 안 표와 같은 건너뜀은 여전히 알리지 않습니다(표식과 구분하지 못함).
- **Alternatives rejected** 각주 본문을 표식 자리에 끼우기(문장 흐름이 끊김), 번호를 부록에서만 붙이기(본문과 연결이 끊김), 같은 데이터 참조를 문서 전체에서 한 번만 세기(출력에는 그림이 반복되어 메모리·작성기 한도와 어긋남).
- **Verified by** `cargo test -p easyoffice-formats --test pages_notes --test pages_fixtures`(픽스처의 각주 표식과 부록 번호, 번호 순서, 각주·도형 안 그림, 중첩 표·도형 분류, 머리글 변형 300개의 같은 로고가 한 장으로 열림, 손상된 머리글 그림은 알리고 표식은 알리지 않음, 각주 항목 65,000개로 문단 한도 거부, 범위 밖·끊어진 각주 표식).
- **Revisit when** 병합 영역을 모델에 담을 수 있을 때, 번호 형식·미주·쪽마다 다시 시작하는 각주 실파일을 구했을 때.

## PD-117. 표 병합 영역을 모델에 담고 DOCX·ODT·HWPX·Pages가 보존

- **Updated by PD-157**: 병합 지도의 필드 번호는 실파일로 확인했고(`행<<16|열` varint, 형식 6007) 이전 설명의 중첩 fixed32·열 상위 배치는 틀렸습니다. 모델 필드 47 수식 저장소도 읽습니다.
- **Context** `RichBlock::Table`은 셀 글자 격자(`Vec<Vec<String>>`)뿐이라 병합 셀이 DOCX·ODT·HWPX 읽기에서 사라지고(가로 병합은 짧은 행, 세로 병합은 빈 셀이 됨) Pages 표의 병합 영역은 읽지 않았으며 작성기도 병합을 쓰지 못했습니다.
- **Decision (`PD-117`)** (1) 모델은 `RichBlock::Table(RichTable)`로 바꿉니다. `RichTable`은 기존 글자 격자(`rows`)를 그대로 두고 병합 영역 목록(`merges`: 왼쪽 위 셀의 행·열과 행·열 span)을 더합니다. 덮인 셀은 격자에 빈 칸으로 남아 글자만 보는 소비자(Markdown·TXT·PDF·변환)는 바뀌지 않습니다. (2) 영역 검증은 한 곳(`rich_table.rs`)에서 합니다: 한 칸짜리, span 0, span이 32,767(15비트)을 넘는 것, 격자를 벗어나는 것, 덮을 셀이 없는 행(들쭉날쭉한 행), 앞서 받아들인 영역과 겹치는 것을 버리고 세어 경고로 알립니다(앞의 것이 우선). 겹침은 행 단위 비트 격자와 워드 마스크로 보고, 검증 작업량 상한(비트 격자 워드 2^24개)을 넘으면 나머지를 버립니다. 격자가 400만 셀을 넘으면 영역을 전부 버립니다. 작성기는 쓰기 전에 다시 검증하므로 만든 뒤 바뀐 표가 잘못된 서식을 낳지 않습니다. (3) 읽기: DOCX는 `gridSpan`(빈 셀로 채움)·`vMerge`(위 영역의 같은 열을 늘림, 짝 없는 `continue`는 일반 셀), ODT는 `number-columns/rows-spanned`·`covered-table-cell`·`number-columns-repeated`(행 폭 1,024로 제한), HWPX는 `cellAddr`+`cellSpan`(주소가 없으면 `tr` 순서, 주소·span이 한도를 넘거나 주소가 겹치는 셀은 버림, 격자는 100만 셀까지), Pages는 데이터 저장소 필드 13의 병합 영역 지도입니다. (4) 쓰기: DOCX `gridSpan`/`vMerge`, ODT `number-*-spanned`와 `covered-table-cell`, HWPX `cellSpan`(덮인 셀은 쓰지 않고 셀 크기에 span을 곱함). RTF 작성기는 행마다 `\trowd`와 셀 오른쪽 경계 `\cellxN`(표 폭 9,360트윕을 열 수로 균등 분할, 덮인 셀도 씀)을 셀 내용보다 앞에 쓰고, 가로 병합은 `\clmgf`/`\clmrg`, 세로 병합은 `\clvmgf`/`\clvmrg`로 표시합니다. RTF 읽기는 이 표시를 영역으로 모아 `with_merges`로 다시 검증하며(같은 폭으로 바로 아래 이어지는 세로 표시만 잇고, 버린 영역은 경고), 범위를 벗어나거나 겹치는 표시는 버립니다.
- **Why** 격자를 그대로 두면 사용처 수십 곳(변환·검색·테스트·편집기 JSON)을 건드리지 않고 병합만 더할 수 있습니다. 검증을 모델 한 곳에 모으면 형식마다 신뢰할 수 없는 span을 따로 막지 않아도 되고, 참조 구현(rhwp는 `colSpan`/`rowSpan`을 `u16`으로 읽고 주소를 그대로 씀)에 없는 상한과 겹침 검사를 우리가 더합니다.
- **Trade-offs** Pages 병합 지도의 필드 번호(데이터 저장소 13, 범위 1, 원점·크기 `열<<16|행`)는 우리 실파일(병합 셀 없음)로 확인하지 못했고 공개된 독립 구현(MIT)의 형식 문서를 따랐습니다(`pages_schema.rs`에 `검증 필요`). 최신 파일이 병합을 지도 대신 수식 저장소로만 담으면 병합 없이 읽힙니다(글자는 그대로). 편집기(웹 화면)는 아직 표 병합을 그리지 않아 열기→저장 경로(구조화 JSON)에서는 병합이 빠집니다. RTF 읽기·쓰기, 레거시 DOC/HWP5 표, Markdown·PDF 출력은 병합을 다루지 않고 글자 격자만 씁니다.
- **Alternatives rejected** 셀마다 `{text, row_span, col_span}`를 담는 구조(사용처 전부와 `to_universal_model`·수식 계산이 바뀌고 셀 격자 접근이 느려짐), 위 영역 지도 없이 읽기에서만 span을 푸는 것(저장 시 병합이 다시 사라짐), 기존 표 크레이트(DOCX·ODT·HWPX를 모두 덮는 MIT 크레이트가 없음).
- **Verified by** `cargo test -p easyoffice-formats --lib rich_table`(겹침·범위 밖·`u32::MAX` span·들쭉날쭉한 행·큰 격자 거부, 큰 영역 20만 개가 5초 안에 끝남, 짝 없는 `vMerge`, 먼 주소·중복 주소), `--test table_merge`(DOCX·ODT·HWPX 쓰기→읽기가 같은 표, 마크업 확인, 악의적 DOCX·ODT·HWPX·Pages 입력은 영역을 버리고 경고).
- **Revisit when** 병합이 든 실제 Pages 파일을 구했을 때(필드 번호 확정, 수식 저장소 경로), 편집기가 병합 셀을 그릴 수 있을 때, RTF 작성기가 셀 위치를 쓸 때.

## PD-118. Tauri 번들 도구 캐시와 다운로드 실패 재시도

- **Context** Tauri 번들러는 콜드 빌드마다 GitHub 릴리스에서 도구(Windows NSIS와 플러그인, Linux linuxdeploy·플러그인·AppRun)를 내려받는데, 그곳이 504를 돌려주면 컴파일과 테스트가 끝난 `verify-linux`·`verify-windows` 잡이 번들 단계에서 실패했습니다. 릴리스 빌드도 같은 위험이 있었습니다.
- **Decision (`PD-118`)** (1) 번들러가 도구를 두는 사용자 캐시 디렉터리의 `tauri` 폴더(`dirs::cache_dir()`: Linux `~/.cache/tauri`, Windows `%LOCALAPPDATA%\tauri`)를 `.github/actions/cache-tauri-tools`로 캐시합니다. 키는 OS·아키텍처·Tauri CLI 버전이고, CI의 Linux·Windows 잡과 릴리스 빌드(`prepare-desktop-build`)가 쓰며 macOS는 내려받는 도구가 없어 건너뜁니다. (2) 번들 단계는 `scripts/retry-on-download-failure.mjs`로 감싸 최대 3회(15초, 30초 대기) 다시 실행합니다. 출력에 `failed to bundle project`와 네트워크 오류(`http status: 5xx`·408·429, 요청 전송 오류, 시간 초과, 연결 끊김, DNS)가 함께 있을 때만 재시도하고 컴파일·테스트·그 밖의 번들 실패는 곧바로 실패합니다.
- **Why** 캐시가 있으면 내려받기 자체를 하지 않아 장애의 영향이 없고, 캐시가 비었을 때만 재시도가 필요합니다. 재시도 조건을 번들 단계의 네트워크 오류로 좁혀 실제 결함을 가리지 않습니다.
- **Trade-offs** 캐시는 잡이 성공해야 저장되므로 첫 성공 전까지는 재시도에 기댑니다. 도구 버전은 CLI 버전에 묶여 있어 CLI를 올리면 키가 바뀌어 새로 내려받습니다. 오류 문구는 번들러 출력에 의존합니다.
- **Alternatives rejected** 도구를 리포에 넣기(크기·라이선스 관리), 잡 전체 재실행(컴파일부터 다시), 문구 없이 모든 번들 실패 재시도(실제 결함을 세 번 반복).
- **Verified by** `node --test scripts/test-retry-on-download-failure.mjs`(재시도 판정, 지수 대기, 3회 한도, 컴파일 실패는 한 번만 실행, 워크플로가 모든 빌드를 감싸고 캐시 단계와 CLI 버전이 일치함).
- **Revisit when** 번들러가 도구 내려받기를 자체 재시도하거나 미러 설정을 기본으로 제공할 때.

## PD-119. macOS 끝내기도 순차 큐, macOS 메뉴의 한국어 라벨·⌘W 하나·편집 항목

- **Context** macOS의 끝내기는 시스템 종료 항목이 `ExitRequested`를 일으키면 열린 모든 창에 동시에 `close()`를 보냈습니다. 저장 확인이 여러 창에 한꺼번에 뜨고, 한 창에서 취소해도 깨끗한 다른 창은 이미 닫혔습니다. 순차 큐(`app_quit.rs`)는 Windows·Linux만 썼습니다. 또 macOS 메뉴의 시스템 항목(서비스·가리기·최소화·모두 선택 등)은 시스템 언어로 보였고, ⌘W가 파일 메뉴와 윈도우 메뉴에 둘 있었으며, 시스템 되돌리기·복사는 한글 편집기 프레임 안의 되돌리기 기록과 선택에 닿지 않았습니다.
- **Decision (`PD-119`)** (1) 끝내기는 모든 OS가 `app_quit`의 큐 하나를 씁니다(cfg 제한 없음). 항목은 참조 앱처럼 id `app:quit`·`CmdOrCtrl+Q`의 사용자 지정 항목으로, macOS는 앱 메뉴 끝(라벨 `EasyOffice 끝내기`), Windows·Linux는 파일 메뉴 끝(`끝내기`)입니다(이전 id `file:quit`는 `app:quit`로 바뀜). (2) macOS의 `RunEvent::ExitRequested`(종료 코드 없음: Dock의 종료·시스템 종료)는 열린 창이 있으면 막고 같은 큐로 보냅니다. 코드가 있는 종료(큐가 마지막 창 뒤에 하는 `app.exit(0)`)와 창이 없을 때는 그대로 통과합니다. 큐가 이미 돌고 있으면 두 번째 요청은 무시하되, 큐가 `CLOSE_ACK_TIMEOUT`(10초) 동안 진행(창이 사라짐) 없이 멈춰 있으면 다음 요청이 큐를 새로 시작해 묻고 있던 창을 다시 묻습니다(응답 없는 창이 이후의 모든 끝내기를 삼키지 않음). 메뉴의 끝내기 항목은 `run_item`이 모든 OS에서 이 큐로 보냅니다. 창은 `raise_and_close`로만 닫으므로 PD-113의 확인 유지·응답 없는 페이지 시간 제한과 취소/실패 시 `end_close_prompt`로 끝내기 전체를 멈추는 규칙이 그대로 적용됩니다. 업데이터의 재시작(`app.restart()`)은 큐를 거치지 않으며, 저장 안 한 창이 있으면 시작하지 않는 기존 규칙이 지킵니다. (3) macOS 시스템 항목은 한국어 라벨을 줍니다: 서비스, `EasyOffice 가리기`, 기타 가리기, 모두 보기, 최소화, 닫기, 붙이기, 모두 선택. (4) ⌘W는 윈도우 메뉴의 시스템 `닫기`만 갖습니다(참조 앱과 같음). macOS 파일 메뉴에는 `file:close`를 넣지 않습니다(표에는 남아 Windows·Linux가 `Ctrl+W`로 씁니다). 시스템 닫기도 창의 `CloseRequested`를 거쳐 저장 확인(PD-113)이 같은 경로로 보장됩니다. (5) macOS 편집 메뉴는 참조 앱처럼 되돌리기·다시 실행·오려 두기·복사하기를 표의 사용자 지정 항목(`⌘Z`·`⇧⌘Z`·`⌘X`·`⌘C`, `MACOS_EDIT_KEYS`)으로 두고, 붙이기·모두 선택만 시스템 항목입니다. 한글 편집기 창에서는 이 네 항목이 편집기 명령(`edit:undo`·`edit:redo`·`edit:cut`·`edit:copy`)으로 임베드 포트의 `automation.execute`에 가며(`EMBED_EDITOR_COMMANDS`), 그 창에서는 회색이 되지 않습니다. 다른 창은 기존 페이지 동작(`studio-cmd:`)이며, 입력 칸(`input`·`textarea`)에 포커스가 있으면 브라우저의 편집 명령(`execCommand`)을 실행합니다(메뉴 키가 입력 칸에 닿지 않으므로). 편집기 프레임 안에서도 같습니다: 편집기가 이 네 명령을 받을 때 편집기 자신의 숨은 입력(`aria-label` `문서 편집 입력`)이 아닌 글자 입력 칸(대화상자의 입력 칸 등)에 포커스가 있으면 문서 명령 대신 그 칸에 `execCommand`를 적용하므로 문서 선택이 잘리거나 복사되지 않습니다(`embed/text-field-edit.ts`). Windows·Linux는 PD-102(4)대로 키를 달지 않습니다. (6) PD-102의 (5) 편집 항목 중 macOS 시스템 항목, (6)의 종료, (8)의 Windows·Linux 한정 서술은 이 결정으로 바뀝니다.
- **Why** 한 창씩 묻고 취소로 멈추는 동작은 이미 Windows·Linux에서 검증된 큐라 macOS도 같은 코드를 쓰면 경로가 하나이고 B1~B3 회귀(PD-113)가 같은 보호를 받습니다. 참조 앱도 같은 구조(앱 메뉴 사용자 지정 끝내기 + `ExitRequested` 가로채기)입니다. 메뉴 키가 붙은 항목은 키가 페이지에 닿지 않으므로 macOS에서는 키를 달아야 복사 같은 동작이 웹뷰에 닿습니다(편집 메뉴에 키가 있는 항목이 없으면 macOS 웹뷰의 복사·되돌리기 키가 동작하지 않을 수 있음).
- **Trade-offs** 메뉴 키가 입력 칸에도 먼저 닿으므로 입력 칸의 편집은 `execCommand`로 대신합니다(페이지와 편집기 프레임 모두). 편집기 프레임의 `contenteditable`·`select` 등 `input`·`textarea`가 아닌 편집 요소는 대신하지 않습니다. 큐가 돌고 있을 때 10초 안의 두 번째 끝내기는 무시되고, 답이 없는 창은 그 뒤의 끝내기로 다시 묻거나 창의 닫기 단추를 씁니다. macOS 파일 메뉴에는 닫기가 없습니다(참조 앱과 같은 배치).
- **Alternatives rejected** macOS만 앱 이벤트를 창에 보내 페이지가 묻게 하는 참조 앱의 이벤트 방식(`core:event` 권한과 두 번째 경로가 필요), 시스템 편집 항목 유지(편집기 되돌리기 기록에 닿지 않음), 키 없는 사용자 지정 항목(웹뷰 복사·붙이기 키가 동작하지 않을 위험), 두 번째 끝내기에서 큐를 다시 만들기(창이 둘씩 묻힘).
- **Verified by** `cargo test -p easyoffice-desktop`(큐의 순차 진행·취소·진행 중 두 번째 요청 무시, 끝내기 id·키·라벨, 앱 메뉴에 시스템 끝내기 없음, 시스템 항목 모두 한국어 라벨, ⌘W는 윈도우 메뉴만, 편집 키 네 개, 끝내기 경로가 `app_quit`뿐인 구조 검사), `node --test scripts/test-webui-native-menu.mjs scripts/test-webui-menu-state.mjs`(편집 항목의 편집기 명령 대응, 입력 칸의 편집 명령, 한글 창에서 회색이 아님). `cargo test -p easyoffice-desktop`의 `run_item_sends_the_quit_item_to_the_queue`와 멈춘 큐 재시작 시험, `node --test scripts/test-hwp-studio.mjs`(모달 입력 칸 포커스 + 문서 선택 있음 → 문서 불변). 실기기 확인 필요: ⌘Q 순차·취소, Dock 종료, 한글 편집기에서 ⌘Z·⌘C, 입력 칸에서 ⌘C·⌘Z.
- **Revisit when** 입력 칸의 `execCommand` 복사가 웹뷰에서 막힐 때, 참조 앱이 편집 키 처리를 바꿀 때.

## PD-120. 암호로 연 한글 문서는 원본을 덮어쓰지 않고, 암호 없이 저장하기 전에 묻는다

- **Context** PD-085 트레이드오프 6이 "저장본에 암호가 유지되는지 미검증"으로 남아 있었습니다. 확인 결과 유지되지 않습니다: rhwp의 `serialize_hwp`는 암호를 받지 않으면 파일 머리의 암호·배포용 플래그와 암호 버전을 지우고(`serializer/cfb_writer.rs` `serialize_hwp_inner`) 암호를 받는 경로(`exportHwpWithPassword`)는 편집기가 암호 문자열을 보관하지 않아(`wasm-bridge.ts` `loadDocumentAtomically`) 쓸 수 없습니다. 그래서 암호 문서를 저장하면 사용자가 모르는 사이 암호 없는 파일이 원본 자리에 생겼고, 자동 저장 사본(IndexedDB)도 평문이었습니다. 참조 앱은 암호 문서의 원본 저장을 거부하고 다른 이름으로 저장할 때만 "저장한 HWP 파일에는 암호가 적용되지 않습니다. 암호 없이 저장하시겠습니까?"를 묻습니다(`tauri-bridge.ts` 191-219, 편집기가 암호로 열었는지를 `requiresPasswordForSave` 플래그로 기억).
- **Decision (`PD-120`)** (1) 편집기는 참조 앱과 같은 플래그(`wasm.requiresPasswordForSave`, 암호로 연 경우에만 참, 다른 문서 열기·새 문서에서 거짓)를 임베드 포트로 알립니다. 새 capability `save-protection-v1`과 RPC 둘: `getSaveProtection` → `{ passwordProtected: boolean }`, `clearSaveProtection` → `{ cleared: true }`. 경로·암호 값은 포트를 건너지 않습니다. (2) 셸(`hwp-session.js`)은 저장마다 내보내기 전에 `getSaveProtection`을 묻습니다. capability가 없거나 응답이 불리언이 아니거나 호출이 실패하면 저장하지 않습니다(fail-closed, 검증 RPC와 같음). (3) 암호 문서에서 원본 저장(같은 파일 덮어쓰기: 저장·Ctrl+S)은 거부하고 "다른 이름으로 저장에서 암호 없는 HWP 파일로 저장하세요"를 상태줄에 알립니다. (4) 다른 이름으로 저장(메뉴 HWP·HWPX)은 참조 앱 문구 그대로 묻고, 취소하면 내보내기도 저장 대화상자도 열지 않습니다. (5) 창 닫기의 `저장`(끝내기·Cmd+Q는 같은 경로)은 암호 문서면 원본 저장 대신 같은 확인을 거친 다른 이름으로 저장(저장 대화상자)이 됩니다. 확인 취소·대화상자 취소·실패는 저장 실패로 돌아가 창을 닫지 않고 끝내기도 멈춥니다(PD-113의 `end_close_prompt`). (6) 저장이 전달되면 셸은 `clearSaveProtection`으로 편집기 플래그를 내립니다(저장된 파일에는 암호가 없고 셸이 추적하는 파일도 그 파일이므로 이후 저장은 평범한 원본 저장). 저장 중 편집이 있어 수정됨을 유지할 때도 플래그는 내립니다. (7) 자동 저장은 암호 문서에서 하지 않고 이미 있던 자동 저장본을 지우며, 문서당 한 번 상태줄에 알립니다(참조 rhwp-studio의 `isRecoveryBlocked`와 같은 입장). (8) 확인은 네이티브 확인 창입니다. macOS의 WKWebView(wry)는 `window.confirm`을 구현하지 않아 대화상자 없이 늘 취소로 답하므로(그 탓에 이 기능이 macOS에서 저장 불가가 되고, 기존 "수정 중 다른 문서 열기"·"HWPX 검증 실패 시 HWP 대체 저장"·자동 저장본 복원 확인도 늘 거절이었습니다 — 복원 확인은 늘 "복원 안 함"으로 자동 저장본을 지웠습니다), 새 앱 명령 `confirm_dialog(kind)`(`ShellCaller` 필수, APP_COMMANDS 19번째, capability `allow-confirm-dialog`)를 둡니다. 인자는 확인 종류 enum 하나(`password-free-save`, `replace-unsaved-hwp`, `hwpx-fallback-hwp`, `restore-hwp-autosave`, `replace-unsaved-module`)이고 모르는 이름은 직렬화에서 거부됩니다. 문구·단추 이름은 Rust 상수라 페이지가 임의 문구를 띄울 수 없고, 창은 호출한 창이며(`parent`), `save_tracked`의 `OkCancelCustom`·`blocking_show` 패턴을 `spawn_blocking`에서 씁니다. 암호 확인의 단추는 참조 앱과 같은 "암호 없이 저장"/"취소"입니다. 페이지의 `confirm`은 비동기이고 기본값은 "아니오"입니다(확인 창을 못 띄우면 거절입니다). 복원 확인은 네이티브 창이 "복원 안 함"과 창 닫기·창 없음을 구분하지 못하므로 수락한 복원 외에는 자동 저장본을 지우지 않고 남겨 다음 시작에 다시 묻습니다(단추는 "복원"/"나중에", 7일 뒤 정리·저장·다른 문서 열기가 지움). 확인이 떠 있는 동안 세션은 `busy`라 자동 저장과 두 번째 열기가 끼어들지 못하고, 수락 뒤에도 창이 정리되었거나 그사이 편집·열기가 있었으면 복원하지 않습니다. `--serve` 브라우저 미리보기는 `window.confirm`이 되므로 같은 문구를 `desktop-bridge.js`가 갖고 테스트가 Rust와의 일치를 검사합니다. 파일 이름·모듈 이름 같은 동적 값은 네이티브 문구에 넣지 않아 기존 문구에서 그 부분이 빠졌고(HWPX 검증 실패 사유는 상태줄로 이동).
- **Why** 사용자가 암호로 보호한 문서가 알림 없이 평문으로 바뀌는 것은 데이터 보호 결함이고, 암호를 되살려 쓰려면 암호를 보관해야 하는데 PD-085는 암호가 부모로 오지 않는다는 입장입니다. 참조 앱과 같은 흐름이 가장 좁은 해결입니다.
- **Trade-offs** 참조 앱은 저장 위치를 고른 뒤 묻지만 셸의 저장 전달은 대화상자와 쓰기가 한 번의 셸 명령이라 사이에 끼울 수 없어, 우리는 위치를 고르기 전에 묻습니다(취소는 같음). 원본 거부 안내는 상태줄에만 나옵니다. 암호 문서의 자동 저장이 꺼져 있어 비정상 종료 시 마지막 저장 이후 편집은 복원되지 않습니다. 변환기로 보내는 내보내기(`getConverterSource`)와 PDF·인쇄는 사용자가 고른 다른 형식 출력이라 이 확인 대상이 아닙니다(참조 앱도 묻지 않음). 저장된 사본에 암호를 다시 거는 기능은 없습니다(암호 보관이 필요).
- **Alternatives rejected** 편집기에 암호를 보관해 `exportHwpWithPassword`로 저장(암호가 메모리에 오래 남고 PD-085 입장과 충돌), 원본 저장도 확인 후 허용(원본의 암호가 확인 한 번으로 사라짐), 위치 선택 뒤 확인(셸 명령을 둘로 쪼개야 하고 저장 경로 규칙이 바뀜).
- **Verified by** `node --test scripts/test-webui-hwp-session.mjs`(원본 저장 거부, 다른 이름 저장 확인·취소, 닫기 중 대화상자 전달과 취소, 플래그 해제, 확인 불가 시 fail-closed, 자동 저장 차단, 암호 없는 문서 무영향), `scripts/test-hwp-studio.mjs`(포트 RPC·capability·업스트림 플래그 수명), `scripts/test-webui-windows.mjs`(닫기 저장이 `closing`으로 가는 배선), `scripts/test-webui-desktop-bridge.mjs`(확인 호출은 종류 이름과 토큰만 보냄, true만 수락, 브라우저 문구가 Rust와 같음), `cargo test -p easyoffice-desktop confirm_dialog`(모르는 이름·형식 거부, 종류별 문구, 참조 앱 문구와 단추).
- **Revisit when** 참조 앱이 암호를 유지한 저장을 지원할 때, 셸 저장 명령이 위치 선택과 쓰기를 나눌 수 있을 때, 저장된 사본에 암호를 다시 거는 요구가 생길 때.

## PD-121. 네이티브 메뉴는 macOS만, 페이지 메뉴 줄은 모든 OS의 모든 창에: PD-102 전제 정정, 끝내기 명령

- **Status** Implemented (QA 요청 2026-11-08: 메뉴 막대를 참조 앱과 같게. Windows·Linux 실기기 확인 전: 이 환경은 macOS이므로 비 macOS 경로는 macOS 코드를 cfg로 빼고 clippy·테스트 빌드한 것과 JS 테스트로만 검증했습니다)
- **Context** PD-102는 "참조 앱은 모든 OS에 네이티브 메뉴를 설치하고 창 안 메뉴는 그리지 않는다"고 전제하고 모든 OS에 네이티브 막대를 달았으며 페이지의 메뉴 줄을 숨겼습니다. 이 전제가 틀렸습니다. 참조 앱의 네이티브 메뉴는 macOS에만 있고(`apps/desktop/src-tauri/src/lib.rs:8-9` `#[cfg(target_os = "macos")] mod menu;`, `:67-70` macOS가 아니면 빈 `Menu::new`), 메뉴 줄은 모든 OS에서 항상 페이지가 그립니다(`apps/studio-host/src/main.ts:154` `new MenuBar(#menu-bar, ...)`). 숨기는 규칙은 참조 앱 어디에도 없습니다(`core/desktop-chrome.ts`는 macOS에서 단축키 문구만 바꾸고, 메뉴 줄 숨김 CSS가 없음). 그래서 macOS에서도 네이티브 막대와 페이지 메뉴 줄이 함께 보입니다. 우리 쪽 결과로 Windows·Linux의 한글 창은 네이티브 막대를 숨기려고 창마다 `hide_menu`를 부르는 상태 보고(`embed`)가 필요했고, 막대를 숨긴 창에서는 `Ctrl+Q`가 동작하지 않을 수 있었습니다.
- **Decision (`PD-121`)** (1) **네이티브 메뉴 막대는 macOS에만** 둡니다: `native_menu`·`menu_state` 모듈은 `cfg(target_os = "macos")`이고(PD-102 (2)~(6)·(9)·PD-119 (3)~(5)의 구성은 그대로), Windows·Linux는 메뉴 막대를 만들지 않습니다(참조 앱은 빈 `Menu::new`를 달지만, Tauri는 macOS에만 기본 메뉴를 만들므로 `set_menu` 자체를 부르지 않아 빈 메뉴 줄이 자리를 차지할 일이 없게 했습니다). (2) **페이지 메뉴 줄은 모든 OS·모든 창에서 그립니다**: `#app[data-native-menu]`와 `.studio-menubar` 숨김 규칙을 없앴고 macOS에서도 네이티브 막대 옆에 그립니다(참조 앱과 같음). 한글 창은 이전대로 셸의 메뉴 줄 대신 편집기 자체 메뉴 줄(`apps/hwp-studio`의 `#menu-bar`, 편집·입력·서식·표 등)이 보이며, 이제 네이티브 막대가 없으므로 그 줄이 Windows·Linux의 한글 창 메뉴입니다. (3) 메뉴 상태 동기화는 macOS 창만: 페이지의 `pushMenuState`는 macOS 플랫폼(`activeShortcutModifier === "⌘"`)에서만 `report_menu_state`를 보내고, 명령은 macOS 빌드에만 등록됩니다. `embed` 인수와 `set_bar_visible`(`hide_menu`/`show_menu`)을 없앴습니다. (4) **끝내기**: Windows·Linux에는 네이티브 `app:quit`이 없으므로 페이지가 새 셸 명령 `quit_app`(인수 없음, `ShellCaller` 토큰 검사, capability `allow-quit-app`)로 `app_quit`의 큐 하나를 시작합니다. 참조 앱의 페이지 메뉴에 없는 항목은 만들지 않으므로 메뉴 항목 없이 단축키 `Ctrl+Q`(단축키 표의 `app-quit`, `desktopOnly`)만 두고, 이 키는 한글 편집기에 포커스가 있어도 전달됩니다(`forwardFromEditor`). macOS는 앱 메뉴의 끝내기를 씁니다. 앱 명령은 18종에서 19종이 됩니다. (5) 스모크의 `menu` 단계는 macOS에서는 이전대로 설치된 항목을, Windows·Linux에서는 네이티브 막대가 **없음**을 확인합니다. (6) PD-102 (1)의 "모든 OS에 설치"·"페이지 메뉴 줄 숨김", (7)·(8)의 Windows·Linux 파일 메뉴 끝내기, (10)의 한글 창 네이티브 막대 숨김은 이 결정으로 바뀝니다.
- **Why** 참조 앱과 같은 구성이 목표이고(QA 요청), 페이지 메뉴 줄은 이미 모든 항목(PD-102의 표와 1:1)을 가지므로 Windows·Linux에서 닿는 기능은 줄지 않습니다. 네이티브 막대를 창마다 숨기고 보이는 상태 동기화가 사라져 코드가 줄고, 한글 창의 두 메뉴가 겹칠 일이 없습니다.
- **Trade-offs** Windows·Linux에는 끝내기 메뉴 항목이 없고 `Ctrl+Q`(창 닫기 버튼과 `Alt+F4`는 창 하나만 닫음)로만 앱을 끝냅니다. Windows·Linux의 한글 창은 셸 메뉴 줄이 숨어 있어(PD-093 (1)) 테마 선택·업데이트 확인 알림 항목은 그 창에 없고 시작 창의 메뉴(PD-135로 시작 창에도 메뉴 줄이 보임)나 단축키(`Ctrl+K` 명령 팔레트, `Ctrl+Alt+D` 테마)로 닿습니다(제품 정보는 PD-135로 한글 창 파일 메뉴에도 있음)(PD-102 (10) 때와 같음, 참조 앱의 한글 창은 편집기 메뉴뿐). macOS는 네이티브 막대와 페이지 메뉴 줄이 모두 보이며(참조 앱과 같음) 한글 창이 아닌 창에서 메뉴가 두 벌입니다. `file:close` 같은 표 항목은 macOS에서 시스템 항목이 대신하지만 페이지 메뉴가 쓰므로 표에 남깁니다.
- **Alternatives rejected** Windows·Linux에 빈 `Menu::new()`를 다는 것(참조 앱 그대로이나 Windows에서 빈 메뉴 줄이 자리를 차지할 수 있는지 확인할 기기가 없음), macOS에서 페이지 메뉴 줄을 숨기는 것(참조 앱이 그리므로 따름), 페이지 `파일` 메뉴나 한글 편집기 메뉴에 끝내기 항목을 넣는 것(참조 앱에 없는 UI이고 편집기는 번들 재생성도 필요), `report_menu_state`를 모든 OS에 두고 비 macOS에서 무시하는 것(쓰이지 않는 IPC).
- **Verified by** `cargo test -p easyoffice-desktop`(앱 명령 19종·capability 일치, 모든 명령이 `ShellCaller`를 받음, 끝내기 경로가 `app_quit` 큐뿐), macOS 코드를 cfg로 뺀 상태의 `cargo clippy -p easyoffice-desktop --all-targets -- -D warnings`와 `cargo test --no-run`(비 macOS 빌드 모사), `node --test scripts/test-webui-native-menu.mjs scripts/test-webui-menu-state.mjs scripts/test-webui-windows.mjs`(모든 OS에서 메뉴 줄을 숨기지 않음, Windows·Linux 창은 메뉴 상태를 보내지 않음, `Ctrl+Q`가 `quit_app`을 부르고 페이지 메뉴에 끝내기 항목이 없음, 전달 단축키 목록).
- **Revisit when** 실기기에서 Windows·Linux의 한글 창 메뉴 줄·`Ctrl+Q`·끝내기 항목을 확인한 뒤, 참조 앱이 Windows·Linux에 네이티브 메뉴를 넣을 때, 한글 창에 정보·테마·업데이트 항목이 필요해질 때.

## PD-122. 제품 정보 창에 HWP 공개 문서 고지와 오픈소스 라이선스 표, 위생 가드의 고지 문장 예외

- **Status** Implemented (QA 승인 2026-11-08: 고지 문구는 원문 그대로, 오픈소스 라이선스 목록도 넣고, 위생 가드에 이 문장에 한한 예외를 둠)
- **Context** HWP 공개 문서의 저작권 조항은 제품 정보에 고지 문구를 요구하고, 업스트림 편집기는 제품 정보에 그 문구와 오픈소스 라이선스 표를 둡니다(`rhwp-studio/src/ui/about-dialog.ts`, `i18n/locales/ko.ts`). 참조 앱은 업스트림 대화상자에 버전 줄만 더해 씁니다. 우리 제품 정보 창에는 버전과 진단 알약뿐이었습니다. 문구에 회사 이름이 들어가 위생 가드의 금지 용어와 부딪힙니다.
- **Decision (`PD-122`)** (1) `index.html`의 제품 정보 창에 업스트림과 같은 순서로 고지 문장(원문 그대로), "오픈소스 라이선스" 제목, 라이선스 표, 전체 목록 안내를 넣고 진단 알약(PD-093 (4))은 그대로 둡니다. 한글 창도 같은 창을 씁니다(편집기 번들에는 자체 제품 정보가 없음). 스타일은 업스트림 `dialogs.css`의 `.about-*` 값을 `tokens.css` 토큰으로만 옮겼습니다. (2) 라이선스 표는 `THIRD_PARTY_NOTICES.md`의 WebAssembly 의존성 표에 실제로 있는 크레이트만 두고(업스트림 목록에서 네이티브 전용인 `subsetter`는 뺌), `scripts/test-about-notice.mjs`가 이름과 라이선스가 표와 일치하는지 검사합니다. (3) 고지 문장에 든 회사 이름을 금지 용어에 더하고(커밋 메시지 가드에도 적용), `scripts/prohibited-terms.mjs`의 `NOTICE_EXCEPTIONS`에 정확한 고지 문장과 허용 파일(`index.html`)만 예외로 둡니다. 허용 파일에서도 문장 밖의 이름, 문장을 고친 형태, 다른 파일의 같은 문장은 계속 실패합니다. 커밋 메시지에는 예외가 없습니다.
- **Why** 고지는 법적 요구이고 원문을 바꿀 수 없으므로 가드를 문장 단위로만 열어, 다른 곳에 회사 이름이 새로 생기는 것은 계속 막습니다. 라이선스 목록의 원천을 `THIRD_PARTY_NOTICES.md` 하나로 두고 일치 테스트로 어긋남을 잡습니다.
- **Trade-offs** 표는 `index.html`에 손으로 둔 행이라 의존성이 바뀌면 행을 고쳐야 하며(테스트가 어긋남을 알려 줌), 업스트림처럼 핵심 크레이트만 보여 줍니다.
- **Alternatives rejected** 빌드 시 표를 생성하는 것(정적 페이지와 번들 파이프라인에 단계가 늘어남), 가드를 파일 전체에서 여는 것(다른 문장의 이름이 통과함).
- **Verified by** `node --test scripts/test-about-notice.mjs`(고지 한 번, 표와 일치, 예외 범위), `node scripts/check-repo-hygiene.mjs`.
- **Revisit when** WebAssembly 패키지를 다시 빌드해 의존성이 바뀔 때, 참조 앱이 제품 정보 구성을 바꿀 때.

## PD-123. PDF 내보내기: 저장 위치를 먼저 고르고 쪽을 10쪽씩 흘려보내 저장 후 열기, macOS 파일 메뉴의 PDF·인쇄

- **Status** Implemented (QA 요청 2026-11-08: 참조 앱과 같은 PDF 내보내기 흐름·파일 메뉴. macOS 실기기의 ⌘E·⌘P 키 우선순위와 저장 뒤 열기는 이 환경에서 GUI를 띄울 수 없어 확인 전입니다. 로직은 단위 테스트와 가짜 열기로 검증했습니다)
- **Context** 한글 창의 PDF 내보내기(PD-101, PD-114)는 모든 쪽 SVG를 `Promise.all`로 한꺼번에(최대 2,000쪽) 받은 뒤에야 저장 대화상자를 열었고, 한 번의 명령에 쪽 전체를 실어 보냈습니다. 취소해도 렌더 비용이 들었고 저장된 파일을 열어 주지도 않았습니다. 참조 앱은 저장 위치를 먼저 고르고(`apps/studio-host/src/core/tauri-bridge.ts:226-245`) 끝나면 `openAfter`로 OS 기본 앱을 엽니다(`apps/desktop/src-tauri/src/commands.rs:440-482`, `open::that`). 참조 앱의 macOS 파일 메뉴는 `Export PDF...`(`CmdOrCtrl+E`)와 `Print...`(`CmdOrCtrl+P`)로 끝납니다(`apps/desktop/src-tauri/src/menu.rs:27-32,88-89`). 우리 macOS 파일 메뉴에는 둘 다 없었습니다.
- **Decision (`PD-123`)** (1) **흐름**: 셸 명령 `export_searchable_pdf`를 없애고 세 명령으로 바꿉니다(앱 명령 20 → 22종, `ShellCaller` 토큰·capability·`APP_COMMANDS` 1:1 구조 테스트 그대로). `pdf_export_start(file_name)`이 저장 대화상자를 먼저 열고(취소하면 `{status:"cancelled"}`, 쪽을 렌더하지 않고 글꼴 DB도 만들지 않음) 창마다 하나의 작업을 만들어 번호(`jobId`)를 돌려줍니다. `pdf_export_pages(job_id, pages)`가 다음 쪽 SVG를 받아 `PdfAssembler`(`pdf_export.rs`)에 변환해 넣고 곧 버립니다(한 번에 최대 10쪽, 넘으면 변환 전에 거부). `pdf_export_finish(job_id, commit)`이 페이지 트리를 닫고, 기존 저장 검증(`SaveRequest::from_bytes`, PDF를 다음에 여는 엔진이 받아들이는지)을 거쳐 원자적으로 쓴(`write_document_bytes`: 임시 파일, `sync_all`, rename) 뒤 OS 기본 앱으로 엽니다. `commit = false`면 쓰지 않고 작업만 버립니다. 파일은 마지막에야 생기므로 어느 단계의 실패든 기존 파일은 그대로이고 부분 파일이 없습니다. (2) **경계(PD-088 3)**: 경로는 `pdf_export_start`가 Rust 안에서만 쥐고 페이지는 파일 이름 제안, 작업 번호, SVG 글자, 완료 여부만 다룹니다. 응답은 파일 이름(`name`)·바이트·쪽 수·`opened`만 돌려줍니다. 작업은 창 라벨별이고 번호가 맞지 않거나 창이 다르면 거부하며(오래된 흐름의 늦은 호출이 새 작업을 끝내지 못하게 `commit=false`도 번호를 확인), 창이 닫히면 작업을 버립니다. 쪽을 변환하는 동안 작업은 표에서 빠져 있어 같은 작업에 대한 동시 호출은 거부됩니다. 변환 중에 도착한 `commit=false`·창 닫힘·같은 창의 새 내보내기는 기록해 두었다가 변환이 끝날 때 적용해 버린 작업이 되살아나지 않습니다. (3) **상한**: PD-101·PD-114의 쪽 2,000 / 쪽당 64MiB / 합계 512MiB를 호출 전체에 걸쳐 누적해 변환 전에 `checked_*`로 확인합니다(거부된 묶음은 합계에 들어가지 않음). 쪽 수는 편집기의 `pageCount`로 대화상자보다 먼저 확인해 2,000쪽을 넘는 문서는 대화상자도 렌더도 없이 거부합니다. 출력은 쪽을 한 번에 주든 10쪽씩 주든 바이트가 같습니다(결정적). (4) **열기**: 이미 쓰는 `open` 크레이트(`open_release_page`)의 `open::that`을 씁니다. 파일 이름이 `.pdf`일 때만 엽니다(저장 대화상자가 확장자를 강제하지 않아 프로그램처럼 이름 붙은 파일을 실행하지 않도록, 참조 앱에는 없는 방어). 열기에 실패해도 저장은 성공이며 응답의 `opened: false`로 알려 상태줄에 "저장했으나 기본 앱으로 열지 못했습니다"를 더합니다. (5) **페이지 쪽**: `hwp-session.js`가 쪽 수 확인 → 위치 선택 → 10쪽씩 `getPageSvg`(묶음 안에서는 동시, 묶음 사이에서는 셸이 받은 뒤 다음 묶음을 요청) → 완료 순서로 진행하고, 실패하면 `finish(commit=false)`로 셸의 작업을 버립니다. 진행 문구("PDF 만드는 중: n / N쪽")·완료 문구는 그대로이고, 실행 중 다시 요청하면(메뉴·단축키·편집기 메뉴) "이미 PDF를 내보내고 있습니다"로 거부해 대화상자가 둘 뜨지 않게 합니다. 스모크 `pdf` 단계는 같은 세 명령으로 대화상자 없이 메모리의 PDF를 검사합니다. (6) **PDF 저장은 창의 파일 추적을 바꾸지 않습니다**: 이전 명령은 저장된 PDF를 그 창의 파일로 기록했습니다(`note_saved_path`). 그러면 한글 창의 원본 경로 추적과 최근 문서가 PDF로 바뀌어 원본 변경 감지가 사라지므로, 참조 앱처럼 현재 문서의 파일은 그대로 둡니다. (7) **메뉴**: macOS 네이티브 파일 메뉴 끝에 구분선, `PDF로 내보내기…`(`CmdOrCtrl+E`, 페이지 동작 `hwp-export-pdf`), `인쇄…`(`CmdOrCtrl+P`, `hwp-print`)를 참조 앱과 같은 자리에 둡니다. 두 항목은 한글 편집기 창에서만 켜지고 다른 모든 창에서는 회색입니다(`menu_state`의 비활성 목록, 편집기 전용 셀 항목과 같은 방식). 페이지 메뉴 줄에는 항목을 두지 않습니다: 한글 창에서는 줄 전체가 숨고 편집기 자체 파일 메뉴에 이미 `PDF로 내보내기…`와 `인쇄…`(Ctrl+P)가 있으며, 다른 창에서는 기능이 없습니다(새 화면 요소 없음). (8) **단축키**: 표(`WORKSPACE_SHORTCUT_BINDINGS`)에 `Ctrl+E`(`hwp-export-pdf`, 한글 모듈, 데스크톱, 편집기에서 전달)를 `macOnly`로 더합니다. 참조 앱은 ⌘E를 macOS 메뉴로만 걸고 Windows·Linux에서는 편집기의 `Ctrl+E`(지우기)를 그대로 두며 PDF 항목에 단축키 표기를 달지 않습니다(`tests/rhwp-baseline.test.mjs` "keeps PDF export menu-only without a stale Ctrl+E label", `shortcut-map.test.ts` "keeps Ctrl+E mapped to upstream delete"). 우리도 Windows·Linux에서는 키를 걸지 않고(편집기 전달 목록에도 넣지 않음) 편집기 파일 메뉴 항목에도 키 표기를 달지 않습니다. macOS에서는 메뉴 키가 우선이라는 전제(PD-119)를 따르되, 웹뷰가 키를 먼저 받아 편집기 지우기가 실행되는 경우를 막으려고 같은 키를 편집기 전달 목록에도 넣어 어느 쪽이 먼저 받든 PDF 내보내기가 실행되게 했습니다(전달 키 목록은 `forwardedKeys()` 하나가 만듦, 편집기는 최대 16개).
- **Why** 취소할 수 있는 선택을 렌더 비용보다 먼저 두고, 큰 문서에서 쪽 SVG가 페이지와 셸에 동시에 쌓이지 않게 하며, 결과를 바로 볼 수 있게 하는 것이 참조 앱 수준의 흐름입니다. 경로를 Rust에만 두는 PD-088의 경계와 원자적 저장은 그대로 지킵니다.
- **Trade-offs** 저장 위치는 렌더 전에 정하므로 쓰기 불가 위치는 마지막에야 드러납니다(그때까지의 렌더는 버려짐). 작업은 마지막까지 PDF를 메모리에 쌓습니다(출력 크기만큼, 기존과 같음). 페이지가 새로고침돼 작업이 버려지지 않은 채 남으면 같은 창의 다음 내보내기나 창 닫기까지 PDF가 메모리에 남습니다. 내보내는 동안의 편집은 막지 않아 쪽 수가 바뀌면 PDF가 두 시점의 쪽을 섞을 수 있습니다(이전에도 렌더 시간 동안은 같았고, 지금은 사용자가 대화상자를 고른 뒤 렌더가 시작되므로 창은 더 짧습니다). `opened`는 OS가 열기 요청을 받았다는 뜻이지 보기 프로그램이 떴다는 보장이 아닙니다.
- **의존성 판단** (1) 허용 라이선스의 유지되는 크레이트: `open 5`(MIT)가 참조 앱과 같은 용도로 이미 직접 의존성입니다. (2) 이미 쓰는 의존성: `tauri-plugin-opener`·`tauri-plugin-shell`은 없고(둘 다 JS 명령 권한이 필요해 capability를 넓히므로 쓰지 않음) 위 `open`이 같은 기능입니다. (3) 이식: 새 의존성이 없어 이식 없음. 참조 앱의 `export_pdf_from_core`·`openAfter` 흐름을 `pdf_job.rs`로 옮기되 한 번의 호출을 세 명령으로 쪼갰습니다.
- **Alternatives rejected** 위치를 렌더 뒤에 고르기(이전 방식), 쪽 전체를 한 명령으로 보내기(메모리·IPC 크기), HWP 바이트를 임시 파일로 내보낸 뒤 셸이 읽기(참조 앱 방식: 경로가 페이지를 오가고 임시 파일이 남음), `tauri-plugin-opener`(capability 확대), 페이지가 열기 여부를 정하기(경로 없이는 의미가 없고 열기 요청이 페이지에 노출됨), Windows·Linux에도 `Ctrl+E`를 PDF에 묶기(참조 앱과 달리 편집기 지우기를 가로챔), 페이지 파일 메뉴에 항상 회색인 항목 추가(새 화면 요소).
- **Verified by** `cargo test -p easyoffice-desktop`(`pdf_job`: 묶음별 변환 뒤 마지막에 한 번 쓰기와 열기 한 번, 취소·실패 때 파일·임시 파일 없음과 기존 파일 유지, 10쪽 초과 거부, 빈 작업 거부, 창·번호 불일치와 늦은 중단 무시, 열기 실패해도 저장, `.pdf`만 열기, 스모크 대상 메모리 검사; `pdf_export`: 상한이 묶음에 걸쳐 누적되고 거부된 묶음은 세지 않음, 묶음 크기와 무관한 같은 바이트, 뒤 묶음 실패 쪽의 문서 기준 번호; `native_menu`: 파일 메뉴 끝의 구분선·두 항목·키·동작; 앱 명령 22종·capability 일치·모든 명령이 `ShellCaller`를 받음), `node --test scripts/test-webui-hwp-session.mjs`(위치 먼저·10쪽 묶음·다음 묶음은 셸이 받은 뒤 렌더, 취소 시 렌더 0회, 쪽 수 상한은 대화상자 전에, 실패 시 작업 버림, 중복 요청 거부, 스모크), `scripts/test-webui-hwp-shell.mjs`(브리지가 이름·번호·SVG만 보내고 토큰을 싣는지), `scripts/test-webui-native-menu.mjs`·`test-webui-menu-state.mjs`(표 일치, 편집기 밖에서 회색), `scripts/test-webui-windows.mjs`(`Ctrl+E`는 macOS에서만 전달·바인딩). 300쪽 보고서 측정은 아래.
- **측정** 300쪽 합성 보고서(쪽당 46줄), `cargo test --release -p easyoffice-desktop hundreds_of_ -- --ignored --nocapture`를 `/usr/bin/time -l`로 각각 따로 돌린 최대 상주 메모리(Rust 쪽만; 페이지 쪽 SVG 문자열과 IPC 본문 크기는 포함하지 않음): 쪽을 한 번에 주는 변환(`hundreds_of_pages_are_measured`) 439MB, 10쪽씩 만들어 넣고 버리는 변환(`hundreds_of_streamed_pages_are_measured`) 428MB, 걸린 시간은 각각 1.23초·1.33초입니다. 이 합성 문서의 SVG는 쪽당 약 12.7KB(합계 3.8MB)라 셸 쪽 차이는 SVG 합계만큼(약 4MB)으로 작고, 최대 상주 메모리는 변환 작업(4개 스레드와 매핑된 글꼴, PD-114)이 정합니다. 이 변경의 이득은 그림이 든 문서에서 나타납니다: 쪽 SVG가 쪽당 수 MB가 되면 이전에는 페이지(문자열)와 셸(IPC 본문과 `Vec<String>`)이 문서 전체를 동시에 쥐었고, 지금은 어느 쪽도 10쪽 분량을 넘지 않습니다. 그림이 많은 문서의 실측은 하지 않았습니다.
- **Revisit when** 실기기에서 macOS ⌘E가 편집기 지우기로 가는 경우가 확인될 때(전달 목록을 빼도 되는지), Windows·Linux에서 저장 뒤 열기·파일 메뉴를 확인한 뒤, 참조 앱이 PDF 쪽 범위나 진행률 취소를 넣을 때, 내보내는 동안의 편집을 막아야 할 때.

## PD-124. 열기 피드백, 시작 창 정리, 저장 확인·외부 변경 문구를 참조 앱과 같게

- **Status** Implemented (QA 요청 2026-11-08: 참조 앱과 같게 맞춤)
- **Context** 끌어 놓기·Finder 열기·두 번째 실행에서 파일을 열지 못하면 로그로만 남고, 지원하지 않는 파일은 조용히 무시되며, 끌어오는 동안 안내가 없었습니다. 이런 열기는 Rust가 문서 창을 만들기 때문에 시작 창이 비어 있는 채 뒤에 남았습니다. 닫기 확인과 외부 변경 대화상자의 문구도 참조 앱과 달랐습니다(`desktop-events.ts`, `windows.rs`, `tauri-bridge.ts`).
- **Decision (`PD-124`)** (1) 열기 실패는 사용자 문장으로 포커스 창의 기존 토스트에 보입니다(`__EASYOFFICE_NOTICE__`). 창이 없으면 네이티브 오류 대화상자를 씁니다. 지원하지 않는 형식은 "지원하지 않는 형식입니다 (파일 이름)", 그 밖은 "파일 로드 실패: 사유"이며 사유는 "파일을 찾을 수 없습니다"처럼 짧은 문장과 파일 이름뿐이고 경로와 운영체제 오류 문구는 로그에만 남깁니다(PD-088). (2) 지원하는 파일을 끌어 창 위에 올리면 같은 토스트로 "파일을 놓으면 문서를 엽니다"를 보입니다. 새 시각 요소는 없습니다. (3) Rust가 문서 창을 열거나 이미 열린 창을 앞으로 가져오면 문서도 수정도 없는 시작 창을 레지스트리로 판정해 닫습니다(페이지 쪽 `finishStartWindow`와 같은 규칙). (4) 닫기 확인은 제목 "저장 확인", 본문 "`{문서 이름}`의 변경 내용을 저장할까요?"(표시 이름, 경로 아님), 단추는 저장 / 저장 안 함 / 취소를 유지합니다. (5) 외부 변경 대화상자는 제목 "외부 변경 감지", 본문 "원본 파일이 EasyOffice 밖에서 변경되었습니다. / 파일: {파일 이름} / 그대로 저장하면 외부에서 변경된 내용이 사라질 수 있습니다."이고 단추는 덮어쓰기 / 저장 취소 두 개입니다.
- **Why** 참조 앱은 같은 실패를 상태 메시지로 알리고 끌어오는 동안 안내를 띄웁니다. 참조 앱의 세 번째 단추 "취소"는 "저장 취소"와 같이 저장을 하지 않는 결과(null)만 만들어 의미가 겹치므로 우리는 두 단추로 둡니다.
- **Trade-offs** 명령줄·Finder 인수는 지원하지 않는 확장자의 실제 파일도 열기 시도로 넘겨 오류를 보입니다(이전에는 조용히 무시). 끌어오기를 취소하면 토스트는 시간이 지나야 사라집니다.
- **Alternatives rejected** 새 끌어 놓기 덮개 요소(새 시각 요소 금지), 페이지가 Tauri 이벤트를 직접 듣게 하는 것(페이지에 이벤트 권한을 주지 않는 PD-102 방침).
- **Verified by** `cargo test -p easyoffice-desktop`(`open_feedback`, `windows::only_empty_start_windows_are_listed_for_cleanup`, 외부 변경 문구), `node --test scripts/test-webui-windows.mjs`.
- **Revisit when** 참조 앱의 열기 안내나 저장 확인 문구가 바뀔 때.

## PD-125. Linux 웹뷰 런타임 보정, HWP UTI 선언, 한글 창 macOS 표기, 작은 정리를 참조 앱과 같게

- **Status** Implemented (QA 요청 2026-11-08: 참조 앱과 같게 맞춤. Linux 보정은 이 환경에서 결정 로직 단위 테스트만 돌렸고 실기기 확인 전)
- **Context** 참조 앱은 Linux에서 WebKitGTK 그래픽 문제를 피하는 환경 변수와 AppImage의 입력기 캐시 보정을 웹뷰 생성 전에 적용하고, macOS 파일 연결에 HWP UTI를 선언하며, 한글 편집기 틀에서 macOS 단축키를 기호로 쓰고 크롬 영역의 기본 우클릭 메뉴를 막습니다(`linux_runtime.rs`, `tauri.conf.json`, `desktop-chrome.ts`, `platform.ts`).
- **Decision (`PD-125`)** (1) `linux_runtime.rs`를 이식해 `main`의 첫머리(웹뷰 전)에서 부릅니다. Arch 계열은 항상, Fedora·RHEL 계열은 Wayland 세션일 때 `WEBKIT_DISABLE_DMABUF_RENDERER`·`WEBKIT_DISABLE_COMPOSITING_MODE`를 `1`로 두며 사용자가 이미 정한 값은 덮어쓰지 않습니다. AppImage에서는 요청된 입력기 모듈이 번들 캐시에 없고 호스트 캐시에 있으면 호스트 캐시를 씁니다(사용자가 `GTK_IM_MODULE_FILE`을 정했으면 그대로). 다른 OS에서는 결정 로직 테스트에만 컴파일됩니다. (2) HWP·HWPX 파일 연결에 `contentTypes`와 `exportedType`(우리 식별자 `com.aquilaxk.easyoffice.hwp(x)`만)을 더합니다. 참조 앱이 함께 적은 다른 앱·회사의 식별자는 넣지 않았고, 핸들러 순위는 우리 정책(Alternate)을 유지합니다. `scripts/desktop-file-associations.mjs`가 이를 검사합니다. (3) 한글 창(`apps/hwp-studio`)은 macOS에서 정적 크롬의 단축키 글과 툴팁을 `⌘`·`⌥`·`⇧`로 쓰고(메뉴 항목은 편집기가 이미 그렇게 씀), 메뉴 줄·도구 상자·서식 줄·상태 줄의 기본 우클릭 메뉴를 막습니다. (4) 시작 화면 "문서 열기" 설명에서 `(Ctrl+O)`를 빼고, 개요 대화상자의 정적 예시 항목을 없앱니다(열 때 `workspace-panels.js`가 채움).
- **Why** 참조 앱과 같은 동작을 맞추고, Linux 배포판별 렌더링 문제와 macOS 연결 불일치를 줄입니다.
- **Trade-offs** 다른 앱이 선언한 UTI는 처음에는 넣지 않았으나, QA 결정(2026-11-09)으로 HWP·HWPX마다 그 앱의 UTI 하나씩을 별도 파일 연결의 `contentTypes`로 더했습니다(PD-141). 우리 `exportedType`과 순위(Alternate)는 그대로입니다. Linux 보정은 배포판 판별을 `/etc/os-release`에 의존합니다.
- **Alternatives rejected** 모든 Linux에 그래픽 대체를 켜는 것(성능 저하), 참조 앱의 UTI 식별자를 그대로 쓰는 것(다른 앱의 식별자).
- **Verified by** `cargo test -p easyoffice-desktop linux_runtime`, `node --test scripts/test-desktop-file-associations.mjs scripts/test-hwp-studio.mjs`.
- **Revisit when** 참조 앱의 Linux 보정 목록이나 UTI 선언이 바뀔 때, Linux·macOS 실기기에서 확인한 뒤.

## PD-126. 최근 문서 대화상자의 "목록 비우기"와 빈 목록 안내를 참조 앱과 같게

- **Status** Implemented (QA 요청 2026-11-08: 참조 앱과 같게 맞춤)
- **Context** 참조 앱의 최근 문서 대화상자는 바닥글 왼쪽에 "목록 비우기"가 있고 확인 없이 목록을 지운 뒤 닫습니다. 목록이 비었을 때는 대화상자를 열지 않고 "최근 문서가 없습니다."를 알립니다(`alert`).
- **Decision (`PD-126`)** (1) 새 앱 명령 `clear_recent_documents`는 `ShellCaller`가 필수이고 인수가 없습니다. 앱 안 목록(`recent.rs`)과 저장 파일을 함께 비우며 실패하면 오류를 돌려줍니다. 참조 앱과 같이 macOS Dock·시스템 최근 항목(`os_recent.rs`)은 건드리지 않습니다(참조 앱의 비우기도 앱 저장소만 지움). 명령은 앱 명령 목록·capability·구조 테스트에 1:1로 추가했습니다(23개). (2) 대화상자 바닥글은 `목록 비우기 · 여백 · 열기 · 취소` 순서이고, 확인 대화상자 없이 실행합니다. 성공하면 대화상자를 닫고 시작 화면·최근 문서 메뉴·한글 편집기의 최근 목록을 다시 불러오며, 실패하면 열어 둔 채 단추를 다시 쓸 수 있게 합니다. (3) 목록이 비었을 때 ⌘⌥O는 대화상자 대신 기존 토스트로 "최근 문서가 없습니다."를 보입니다(`alert`는 macOS WKWebView에서 동작하지 않음).
- **Why** 참조 앱과 같은 동작을 맞춥니다.
- **Trade-offs** 확인 없이 지우므로 되돌릴 수 없습니다(참조 앱과 같음). Dock의 최근 항목은 남습니다.
- **Alternatives rejected** 네이티브 확인 대화상자를 거치는 것(참조 앱에 없음), `alert` 사용.
- **Verified by** `cargo test -p easyoffice-desktop`(`recent::clearing_empties_the_list_and_the_stored_file`, 구조 테스트), `node --test scripts/test-webui-windows.mjs`.
- **Revisit when** 참조 앱의 비우기 동작이나 Dock 항목 처리가 바뀔 때.
- **후속 (`PD-127`)** 최근 목록 저장은 문서 저장과 같은 임시 파일·`sync_all`·rename으로 쓰고(`save_file::write_document_bytes` 재사용), 저장이 실패하면 메모리 목록을 이전 상태로 되돌립니다. 목록이 바뀌면(추가·비우기) 셸이 열린 모든 창에서 인수 없는 전역 함수 `__EASYOFFICE_RECENTS_CHANGED__`를 실행하고, 페이지는 기존 `refreshRecents`로 셸에서 다시 받습니다(페이지에 이벤트 권한을 주지 않음, PD-102).

## PD-128. 설치된 글꼴로 문서가 지정한 글꼴을 그림

- **Status** Implemented (QA 요청 2026-11-08: 참조 앱과 같게 맞춤. 실기기에서 문서를 열어 보는 확인은 하지 않았고, 헤드리스 브라우저로 목록 한 번·읽기 한 번·`FontFace` 등록까지 확인)
- **Context** 한글 편집기는 번들 글꼴만 쓰면 바탕·돋움·맑은 고딕처럼 문서가 지정한 글꼴이 이 PC에 있어도 다른 모양으로 그립니다. 참조 앱은 명령 두 개(`list_local_fonts`, `read_local_font`)로 글꼴 목록(패밀리·스타일·굵기·경로)을 주고 경로로 파일을 읽어 `FontFace`로 등록합니다(`font_catalog.rs`, `commands.rs` 339-349, `desktop-local-fonts.ts`). 읽기에 크기 상한이 없고 페이지가 경로를 쥡니다. 편집기 프레임은 불투명 출처 샌드박스라 셸 명령에 닿지 못합니다(PD-086/088).
- **검토 순서(구현 원칙)** (1) 허용 라이선스의 유지되는 크레이트: 이미 쓰는 `fontdb`가 시스템·사용자 글꼴 폴더(macOS `~/Library/Fonts`, Linux 사용자 폴더, Windows 사용자 폴더)를 모두 읽으므로 새 크레이트가 필요 없습니다. (2) 이미 쓰는 의존성: PDF 내보내기가 프로세스당 한 번 만드는 `pdf_fonts::shared_font_db`를 그대로 씁니다(두 번째 폴더 스캔 없음; 첫 호출이 폴더를 정하므로 `pdf_job::pdf_font_dirs`를 둘이 함께 씀). 편집기 쪽도 이미 있는 호스트 글꼴 계약(`HostFontProvider`: 목록·지연 읽기·`FontFace` 등록, 문서가 쓰는 글꼴만)을 쓰고 새 렌더 경로를 만들지 않았습니다. (3) 참조 구현 이식은 목록 항목의 모양(패밀리·현지화 이름을 별칭으로·스타일·굵기)과 허용 확장자뿐이며, 경로 기반 읽기는 가져오지 않았습니다.
- **Decision (`PD-128`)** (1) 새 셸 명령 `list_system_fonts`(이 창의 목록)와 `read_system_font(revision, file)`(글꼴 파일 하나, 바이너리 `Response`)를 `ShellCaller` 필수로 추가합니다(앱 명령 25개, capability·구조 테스트 갱신). (2) 경로는 페이지에 나가지 않습니다. 목록은 패밀리·별칭·스타일·굵기·기울임·PostScript 이름과 글꼴 파일 번호·면 번호뿐이고, 번호는 창마다 목록을 받을 때 새로 정하는 난수 `revision`과 함께일 때만 유효합니다(다른 창, 이전 목록, 범위 밖, 닫힌 창은 거부). 번들 글꼴 폴더의 면, `.`로 시작하는 숨은 시스템 면, 보통 너비가 아닌 면, 같은 패밀리·굵기·기울임의 둘째 면은 목록에서 뺍니다(문서가 패밀리 이름으로 고를 때 후보가 둘이면 못 고름). (3) 읽기 상한(넘으면 Fail-Closed): 파일 하나 64MiB(편집기가 받는 한도와 같음; 이 Mac의 한글 시스템 글꼴 컬렉션이 52.8MB라 32MiB는 한글 기본 글꼴을 막습니다), 빈 파일 거부, TrueType/OpenType/컬렉션 서명(`00010000`, `true`, `OTTO`, `ttcf`) 확인, 창당 서로 다른 파일 합계 512MiB(이미 보낸 파일은 다시 세지 않고, 실패한 읽기는 몫을 돌려줌), 목록 25,000면. 읽는 도중 커진 파일은 한 바이트 더 읽어 걸러 냅니다. 오류 문구에는 경로가 없습니다. (4) 편집기는 `host-fonts-v1`(임베드 RPC `enableHostFonts`, `hostFontsReply`)을 알리고, 셸의 `hwp-host.js`가 연결 직후 켭니다. 프레임이 `shell_fonts` 이벤트로 목록·읽기를 요청하면 셸 페이지만 토큰 있는 명령을 부르고 `hostFontsReply`로 답하며 바이트는 전송(transfer)합니다. 셸은 요청 번호·면 id 형식·revision을 다시 검사하고 목록은 연결당 한 번만 받으며, 같은 파일의 면들은 파일을 한 번만 읽고(128MiB 캐시) 실패에는 일반 문구로만 답합니다. 편집기에 글꼴이 없거나 실패하면 기존 번들·OS 글꼴 경로 그대로입니다. 데스크톱 셸 밖(`--serve`)에서는 켜지 않습니다.
- **Why** 문서가 지정한 글꼴로 그려야 원본과 모양이 같고, 경로를 페이지에 주지 않으며 읽기에 상한을 두어 신뢰할 수 없는 입력에서도 안전합니다.
- **Trade-offs** 목록 번들(약 145KB, 731면)이 창마다 한 번 전송됩니다. 글꼴 파일 전체(컬렉션이면 그 파일 전체)가 한 번 건너가므로 큰 CJK 컬렉션은 수십 MB입니다(문서가 쓰는 글꼴만, 한 번만). 64MiB를 넘는 컬렉션(이 Mac의 이모지 글꼴 183MB 등)은 쓰지 않습니다. 목록은 프로세스 동안 한 번 만든 것을 쓰므로 실행 중에 설치한 글꼴은 다시 시작해야 보입니다(참조 앱은 `force`로 다시 받음).
- **Alternatives rejected** 참조 앱처럼 경로를 페이지에 주는 것(PD-088 3), 프레임에 셸 권한을 주는 것(PD-086), 글꼴 전체를 미리 보내는 것, 읽기 상한 없이 파일을 읽는 것.
- **측정** 이 Mac의 시스템 글꼴 399파일·731면, `cargo test --release -p easyoffice-desktop system_fonts_are_measured -- --ignored --nocapture`: 글꼴 데이터베이스 첫 구축 44ms(PDF 내보내기와 공유, 이후 0), 목록 구축 0.66ms, 직렬화 포함 0.3ms·145KB, 읽기 23.3MB 2.7ms·55.3MB 5.7ms·2.1MB 0.36ms(운영체제 캐시에 있을 때; 디버그 빌드의 첫 읽기는 23.3MB 35ms), 가장 큰 파일(192MB)은 열지 않고 17µs에 거부. 헤드리스 Chrome 하네스에서 빈 문서를 열면 목록 요청 1회, 문서가 쓰는 글꼴 읽기 1회, `FontFace` 1개 등록.
- **Verified by** `cargo test -p easyoffice-desktop`(`font_catalog`: 읽기·서명·빈 파일·64MiB 초과·읽는 중 커지는 파일·창/revision/범위 검사·창당 상한과 반환·경로 비노출 직렬화·번들 폴더 제외; 앱 명령 25개·capability 일치·모든 명령이 `ShellCaller`), `node --test scripts/test-webui-hwp-host.mjs`(셸 쪽: 켜는 조건, 목록에 파일 번호·경로 없음, 파일당 한 번 읽기와 전송, 잘못된 id·revision·요청 번호·실패한 읽기는 일반 문구, 잘못된 목록 거부), `node --test scripts/test-hwp-studio.mjs`(편집기 쪽 포트 공급자: 요청·응답·형식 검사·중단·시간 초과), `node scripts/hwp-studio-harness.mjs`(실제 브라우저에서 샌드박스 프레임 ↔ 셸 흉내 ↔ `FontFace`).
- **Revisit when** 실기기에서 문서를 열어 확인한 뒤, 실행 중 설치한 글꼴을 다시 읽어야 할 때, 참조 앱이 읽기 상한이나 목록 갱신을 넣을 때.

## PD-129. 위생 가드에 회사 이름 약칭 추가, 문서·주석의 해당 표기를 일반 명칭으로

- **Status** Implemented (QA 요청 2026-11-08)
- **Context** 금지 용어에는 회사의 전체 이름과 상품명이 있었지만 두 글자 약칭은 없어, TODO·정책 문서·주석에 약칭이 남아 있었습니다.
- **Decision (`PD-129`)** (1) `scripts/prohibited-terms.mjs`에 한글 약칭을 같은 base64 방식으로 더합니다. 부분 문자열로 맞추므로 조사가 붙은 형태도 걸리고, 고지 문장 예외(PD-122)는 약칭에 적용하지 않습니다. 커밋 메시지 가드도 같은 목록을 씁니다. (2) 걸린 TODO·정책 문서·설계 문서·편집기 주석·부분 패치 설명의 표기는 "사설 영역(PUA) 문자", "HWP 제작사", "상용 뷰어"처럼 일반 명칭으로 바꿉니다.
- **Why** 한 곳의 목록이 저장소와 커밋 메시지를 함께 막으므로, 약칭도 같은 곳에 두면 새로 들어오는 표기를 CI가 잡습니다.
- **Trade-offs** 영문 약칭도 더하되(후속 변경), 등록된 HWPX MIME 형식(`application/vnd.<제작사>.hwpx`)은 파일 연결 정의에 필요한 표준 식별자라 정확한 문자열만 `MIME_EXCEPTIONS`의 파일(`tauri.conf.json`, 리눅스 `.desktop`, `desktop-file-associations.mjs`, 통합 설계 문서)에서 허용합니다. 커밋 메시지 가드에는 예외가 없습니다. 글꼴 설명의 영문 표기는 일반 명칭으로 바꿨습니다.
- **개정 (QA 결정 2026-11-09, PD-141)** 다른 회사의 역도메인 UTI 두 개(HWP·HWPX)도 같은 구조의 정확 문자열 예외(`UTI_EXCEPTIONS`)로 허용합니다. 범위는 PD-141에 있습니다.
- **Alternatives rejected** 약칭을 단어 경계로 맞추는 것(한글 조사 앞에서 경계가 성립하지 않음).
- **Verified by** `node --test scripts/test-about-notice.mjs scripts/test-commit-messages.mjs`(약칭이 조사와 함께, 모든 파일과 고지 파일에서, 커밋 메시지에서 걸림), `node scripts/check-repo-hygiene.mjs`.
- **Revisit when** 다른 약칭이나 별칭이 문서에서 발견될 때.

## PD-130. 편집기(웹 화면)가 표 병합을 그리고 열기→저장에서 보존

- **Status** Implemented (QA 요청 2026-11-08: 열기→편집→저장에서 병합이 사라지는 데이터 손실 방지. 병합/분할 버튼은 이번 범위가 아님)
- **Context** PD-117로 모델과 DOCX·ODT·HWPX·Pages 읽기, DOCX·ODT·HWPX 쓰기는 병합을 담지만, 구조화 열기 JSON에 병합이 없고 편집기가 표를 `colspan`/`rowspan` 없이 그리며 구조화 저장 JSON도 병합을 싣지 않아 열기→저장에서 병합이 모두 풀렸습니다. 현재 편집기에는 열린 표의 행·열 추가·삭제 명령이 없습니다(행 추가·정렬·병합 토글은 기본 문서의 표 하나에만 걸려 있음). 지워지는 경로는 선택 영역을 지우는 브라우저 기본 동작뿐입니다.
- **Decision (`PD-130`)** (1) **열기**: 구조화 열기 응답의 표 블록에 병합이 있을 때만 `"merges":[{"row","col","rowSpan","colSpan"}]`을 더합니다(왼쪽 위 셀 기준, 위치 순). 덮인 셀은 `rows`에 빈 글자로 남습니다. (2) **그리기**: 편집기는 영역의 왼쪽 위 셀에 `colspan`/`rowspan`을 주고 덮인 셀은 그리지 않습니다. 영역은 격자 안에 있고, 덮을 셀이 모두 있으며, 앞선 영역과 겹치지 않을 때만 씁니다(아니면 무시). 각 `tr`에는 원래 행 번호(`data-eo-row`)를 답니다. (3) **저장**: 편집기는 표를 HTML 표 모델 그대로 읽어(span이 가린 칸은 건너뛰고 빈 글자로 채움) 저장 JSON의 표 블록에 `merges`를 싣습니다. span은 표의 마지막 행, 열 1,024칸, 이미 다른 셀이 덮은 칸에서 멈춰 영역이 겹치지 않습니다. 숨김(`hidden`) 셀은 격자에서 뺍니다(기본 표의 병합 토글이 숨기는 방식). 병합이 없으면 `merges`를 보내지 않습니다. (4) **검증**: Rust는 `merges`를 믿지 않고 `RichTable::with_merges`로 다시 검증합니다(한 칸짜리, span 0, 격자 밖, 겹침을 버림). 버린 수는 저장 응답 `warnings`에 알립니다("표의 병합 N곳은 범위를 벗어나거나 겹쳐 저장하지 않았습니다."). 항목이 객체가 아니거나 네 값 중 하나가 없거나 음수·소수·문자열이면 저장을 거부하고, `u32`를 넘는 값은 `u32::MAX`로 두어 격자 밖으로 버립니다. 병합 항목은 셀과 같은 구조화 저장 항목 상한(50만)에 세어집니다. (5) **행 삭제 규칙**: 병합 영역 안의 행이 지워지면 영역은 남은 행 수만큼 줄어듭니다(`data-eo-row`로 남은 행을 셉니다). 한 칸만 남으면(한 행이 남고 열 span도 1) 병합이 아니므로 해제됩니다. 왼쪽 위 셀이 든 행이 지워지면 그 영역은 사라지고 나머지 셀은 화면에 보이는 그대로 저장됩니다. 열은 편집기 연산이 없어 셀이 지워져도 남은 셀의 `colspan`을 그대로 보냅니다.
- **Why** 격자는 그대로 두고 병합 목록만 더하는 PD-117의 모델을 열기·저장 JSON에도 그대로 쓰므로 형식별 코드가 바뀌지 않습니다. 검증을 Rust 한 곳(`with_merges`)에 두어 페이지가 어떤 값을 보내도 작성기에 잘못된 서식이 가지 않습니다. 화면의 표 모델을 그대로 읽으므로 사용자가 보는 표와 저장되는 표가 같습니다.
- **Trade-offs** 덮인 셀에 들어 있던 글자는 편집기에서 보이지 않으므로 저장하면 빈 칸이 됩니다(작성기도 같은 칸을 비워 쓰므로 DOCX·ODT·HWPX 왕복에서는 차이가 없음). 브라우저가 행을 지우면 span을 알아서 줄여 주지 않아 지운 직후 화면에서는 영역이 다음 행으로 번져 보일 수 있고, 저장 때 올바른 크기로 돌아옵니다. 기본 문서의 표에서 병합 토글로 합친 셀도 이제 저장에 병합으로 실립니다. RTF 작성기는 셀 위치(`\cellx`)를 쓰지 않아 병합을 쓰지 못합니다(TODO).
- **Alternatives rejected** 덮인 셀을 숨김 셀로 함께 그리는 것(지운 행 뒤의 칸 위치는 지켜도 영역이 사라지면 빈 구멍이 남음), 병합이 걸린 행 삭제를 모두 거부하는 것(편집기에 삭제 명령이 없어 막을 곳이 없음), 저장 JSON의 `merges`를 그대로 믿는 것(작성기에 잘못된 서식이 감).
- **Verified by** `cargo test -p easyoffice-app --lib editor_merge`(DOCX·ODT·HWPX 열기→편집 없이 저장→다시 읽기에서 병합 동일, 병합 없는 표엔 `merges` 없음, MD·RTF·TXT·PDF 저장, 겹침·격자 밖·`u32::MAX`·span 0·한 칸짜리 병합을 버리고 경고, 잘못된 형식 거부, 병합 50만 개 상한, 큰 겹침 2만 개가 빠르게 끝남), `node --test scripts/test-webui-table-merges.mjs`(그리기, 저장에서 같은 격자·병합, 행 삭제로 병합 축소·해제, 붙여넣은 표, 숨김 셀, 큰 span 상한, 겹치는 span).
- **Revisit when** 편집기에 행·열 추가·삭제나 병합/분할 명령이 생길 때(그 연산이 span을 직접 고치게), RTF 작성기가 셀 위치를 쓸 때, 덮인 셀의 글자를 보존해야 할 때.

## PD-131. 검색 가능한 PDF: 사설 영역 문자의 글자 대응표 정리와 합성 굵게·기울임

- **Status** Implemented (QA 요청 2026-11-08: 참조 앱 rhwp 변환기에 있고 우리에 없던 두 가지를 이식). 실기기 설치본(Windows·Linux)에서 문서를 내보내 보는 확인은 하지 않았고, PDF 객체 검사 테스트까지 확인.
- **Context** 한글 문서는 원문자·글머리표 같은 기호를 사설 영역(PUA) 코드로 저장하고, svg2pdf는 `ToUnicode` 대응표를 SVG 글자 그대로 만듭니다. 그래서 PDF에서 ①을 검색하면 걸리지 않고 복사하면 사설 코드가 나옵니다. 또 굵게·기울임은 SVG 속성일 뿐이라, 번들 글꼴이 Regular 하나이고(PD-114) 문서 글꼴이 없는 PC에서는 굵은 글자가 보통 두께·바로 선 모양으로 나옵니다. rhwp 변환기는 대응표 정리(`remap_pua_in_tounicode`)와 합성 굵게·기울임(`pdf_synthetic_bold.rs`, `pdf_synthetic_italic.rs`)을 갖고 있습니다.
- **검토 순서(구현 원칙)** (1) 허용 라이선스의 유지되는 크레이트: 사설 영역 표나 PDF 대응표 정리, 합성 굵게·기울임을 해 주는 크레이트는 찾지 못했습니다(svg2pdf·usvg·pdf-writer에는 없고, 한글 문서 기호의 뜻은 rhwp의 도메인 지식). (2) 이미 쓰는 의존성: 합성 판단은 이미 쓰는 `usvg`의 글꼴 선택 결과(`Text::layouted`, `fontdb` 면의 굵기·모양)와 SVG 위치 편집용 `roxmltree`(usvg가 쓰는 같은 크레이트, 직접 의존성으로 이름만 올림), 굵게의 한 번 출력은 이미 들어온 svg2pdf의 `Tr 2`(채움+선, rhwp 패치)를 쓰고, 테스트의 스트림 풀기는 svg2pdf가 쓰는 `miniz_oxide`를 개발 의존성으로 씁니다. 새 크레이트는 없고 `Cargo.lock`에는 직접 의존성 두 줄만 더해집니다. (3) 그래서 rhwp 코드를 이식했습니다: `pdf_tounicode.rs`(`src/renderer/pdf.rs` 953-1046), `pdf_pua.rs`(`composer.rs` 4416-4452, 검증된 기호 표, `paragraph_layout.rs` `map_pua_bullet_char` 11018-11145를 정렬된 표 하나로), `pdf_synthetic.rs`(`pdf_synthetic_bold.rs`·`pdf_synthetic_italic.rs` 전체). 출처는 파일 머리 주석과 `THIRD_PARTY_NOTICES.md`에 있습니다.
- **Decision (`PD-131`)** (1) **대응표**: 완성된 PDF 바이트에서 `begincmap`~`endcmap` 안의 `beginbfchar` 블록만 읽어, 도착 글자에 아는 사설 영역 코드가 있으면 표준 글자로 바꿉니다. 새 글자가 더 짧으면 꺾쇠 안을 공백으로 채워(PDF 16진 문자열은 공백을 무시) 크기와 `/Length`·xref 오프셋이 그대로이고, 길어지면 건드리지 않습니다. 모르는 사설 코드는 뜻을 짓지 않고 그대로 둡니다. 그리는 글리프와 글꼴 대체 선택은 바꾸지 않습니다. (2) **합성**: `<text>` 요소마다 usvg가 실제로 고른 면을 보고, 굵게(굵기 600 이상)를 요청했는데 모든 글자 면이 보통 굵기면 글자 크기의 2% 선을 같은 색으로 채움 뒤에 그리고(PDF 글자 한 번 출력), 기울임을 요청했는데 면이 모두 바로 선 모양이면 기준선을 축으로 0.25 전단합니다. 진짜 굵은·기울인 면이 하나라도 섞이거나, 굵기·모양이 다른 구간, 여러 색, 그러데이션 채움, 자체 선, 세로쓰기, 경로 위 글자는 합성하지 않고 쪽당 개수만 표준 오류로 알립니다. (3) **참조 구현과 달라진 점(비용)**: 굵게·기울임을 한 번의 분석으로 함께 판단하고, 굵게·기울임을 요청한 요소가 없는 쪽은 분석 없이 한 번만 읽습니다. 분석 사본에서는 요청하지 않은 글자 요소와 그림을 `display="none"`으로 가려 비싼 글자 배치를 줄이고(스타일 시트가 있으면 모든 요소를 봄), 변환할 쪽은 원본 또는 편집본을 한 번만 읽습니다(참조 구현은 쪽을 최대 3번 읽음). (4) **상한(Fail-Closed가 아니라 합성만 포기)** 글자 요소 50,000개 초과 쪽은 합성 없이 변환하고, 대응표 블록당 256항목·항목당 32 UTF-16 단위를 넘으면 건드리지 않으며, 같은 `id`를 쓰는 요청 요소들은 합성하지 않습니다. 생성 `id`의 접두사는 한 번의 순회로 정합니다. 기존 쪽 수·크기 상한(PD-101/114/123)은 그대로입니다. (5) HWP 5.0 원시 기호 코드(`0xA000|n`)는 실제 Yi 음절과 구분이 안 되어(파서의 표 필요) 대응표에 넣지 않았고 `TODO.md`에 남겼습니다.
- **Why** 참조 앱과 같게 PDF에서 ①을 검색·복사할 수 있고, 시스템에 굵은 글꼴이 없어도 굵은 글자가 굵게 보입니다. 시스템에 진짜 굵은·기울인 글꼴이 있으면 합성하지 않으므로 이중으로 굵어지지 않습니다.
- **Trade-offs** 굵게·기울임이 든 쪽은 쪽당 분석 읽기가 하나 더 들어 변환 시간이 늘어납니다. 합성은 한 요소 안에서 면이 섞이면 포기해 그 글자는 이전과 같습니다. 합성 굵게의 선 폭(2%)과 기울기(0.25)는 rhwp 값이며 글꼴별로 다르지 않습니다. 바이트 수준 정리는 svg2pdf가 CMap을 압축하지 않고 `beginbfchar`만 쓴다는 현재 동작에 기댑니다(테스트가 막음).
- **Alternatives rejected** svg2pdf(벤더 사본)의 `create_cmap`을 고치는 것(제3자 사본을 건드리고 표를 거기 두어야 함), SVG 글자를 표준 글자로 미리 바꾸는 것(글리프 선택이 바뀌어 대체 글꼴이 틀어짐), 참조 구현 그대로 쪽마다 두세 번 읽는 것(아래 측정), 굵게·기울임을 글꼴 질의만으로 미리 판단하는 것(글자 단위 대체 글꼴을 따로 구현해야 함).
- **Verified by** `cargo test -p easyoffice-desktop`(`pdf_tounicode`·`pdf_pua`·`pdf_synthetic` 단위 시험, `pdf_export::text_tests`: PDF 스트림을 풀어 `2 Tr`·`0.48 w`·`1 0 -0.25 1 16 0 cm` 확인, 진짜 굵은·기울인 면이 있으면 합성하지 않음, xref 오프셋 유지, 악의적 쪽·대응표), `export_is_deterministic`. 구현 전 RED는 대응표 정리와 합성 호출을 끄고 같은 시험이 실패하는 것으로 확인.
- **측정** 300쪽 합성 보고서(쪽당 46줄), 같은 머신에서 이식 전·후 시험 바이너리를 번갈아 3회, `hundreds_of_pages`·`hundreds_of_streamed_pages`: 전 1.03~1.15s·1.21~1.24s, 후 1.06~1.15s·1.22~1.31s(차이는 잡음 안), 최대 상주 메모리 약 456MB로 같음. 굵게 5줄에 1줄·기울임 7줄에 1줄과 사설 영역 문자 2개가 매 줄에 있는 같은 300쪽(`hundreds_of_styled_pages_are_measured`, 이 머신에서는 합성이 모든 쪽에서 일어나 최악에 가까운 경우): 합성 없이 1.2s → 합성 1.8s(분석 사본에서 요청하지 않은 글자를 가리기 전에는 2.4s). 굵게·기울임이 없는 쪽의 비용은 요청 단어 검색 정도입니다.
- **HWP 5.0 원시 기호 확인 (2026-11-09)** rhwp 샘플 HWP·HWPX 14개의 쪽 SVG 153쪽을 만들어 `U+A000–U+A4CF` 글자를 찾았으나 0개였습니다. rhwp 렌더러가 파서 표(`src/parser/tags.rs`, 한글 출력으로 실측한 66개 값)에 있는 원시 코드를 SVG를 쓰기 전에 이 문서의 표와 같은 평면 15 코드로 바꾸고(`src/renderer/composer.rs`의 표시 문자열 단계), 표에 없는 값은 실제 Yi 음절(rhwp가 반례를 실측)일 수 있어 뜻을 알 수 없으므로 더하지 않았습니다.
- **Revisit when** 실기기(Windows·Linux)에서 번들 글꼴만으로 내보낸 PDF를 열어 확인한 뒤, svg2pdf를 올려 CMap 형식이 바뀔 때, HWP 5.0 원시 기호가 SVG에 남는 것이 확인될 때.

## PD-132. 저장한 HWP·HWPX 파일에 첫 쪽 미리보기 그림을 넣고, DOCX는 엔진이 받을 수 있게 함

- **Status** Implemented (QA 요청 2026-11-08: 탐색기·시작 화면 카드(PD-106)가 이 앱이 저장한 파일에서도 그림을 보이게. 한글 HWP·HWPX는 저장 경로까지 연결, DOCX는 엔진 기능만이고 쪽을 그리는 호출자가 없어 연결하지 않음)
- **Context** rhwp 직렬화는 문서를 열 때의 미리보기를 그대로 다시 씁니다. HWP는 원본 `PrvImage`(없던 문서는 스트림 없음), HWPX는 원본 `Preview/PrvImage.png`(새 문서는 1x1 투명 PNG)라서 편집해 저장해도 그림은 열 때의 첫 쪽이거나 비어 있습니다(rhwp 0.8.7로 내보낸 샘플에서 원본 그림이 그대로 나옴을 확인). 편집기 WASM에는 미리보기를 바꾸는 호출이 없습니다. 엔진(`easyoffice-formats`)은 쪽을 그리지 않고, 비한글 문서에는 쪽을 그리는 경로 자체가 없습니다. PDF의 `/Thumb`는 아래 7에서 다룹니다.
- **Decision (`PD-132`)** (1) **엔진** `embedded_thumbnail.rs`의 `embed_thumbnail(format, 문서 바이트, PNG)`: HWP·HWT는 OLE2를 풀어 `PrvImage` 스트림을 바꾸거나 더해 다시 묶고, HWPX·HWTX는 ZIP을 풀어 `Preview/PrvImage.png`를 바꾸거나 더해 다시 묶으며(`mimetype` 맨 앞·저장 유지), DOCX는 `docProps/thumbnail.png`와 패키지 루트의 `metadata/thumbnail` 관계, PNG 기본 콘텐츠 형식(이미 있으면 더하지 않음)을 더합니다. 이미 PNG 썸네일 관계가 있으면 그 부분만 바꾸고, PNG가 아닌 썸네일이 있으면 두 개가 되지 않게 거부합니다. 다른 형식은 `UnsupportedExtension`. (2) **상한(Fail-Closed, 무엇도 읽기 전에 검사)**: 그림은 모든 청크 CRC를 검사한 PNG, 128KiB 이하(시작 화면 읽기 상한 256KiB의 절반), 긴 변 256px 이하. 문서는 64MiB 이하(풀어서 다시 묶으므로 메모리가 문서의 몇 배). 넘거나 컨테이너가 깨졌으면 오류이고 호출자는 문서를 그대로 씁니다. (3) **rich 작성기** `encode_rich_with_thumbnail(doc, format, formula_cache, Option<&[u8]>)`: 그림은 호출자가 줄 때만 넣고(엔진은 그리지 않음), `None`이거나 그림 자리가 없는 형식(ODT 등)은 `encode_rich`와 바이트까지 같습니다. (4) **셸** `save_document_file`에 선택 인수 `thumbnailSvg`(첫 쪽 SVG)를 더했습니다. `save_thumbnail.rs`가 PDF 내보내기와 같은 usvg 옵션(공용 글꼴 DB, 대체 글꼴, 경로·`file:` href 읽기 거부)으로 읽어 `resvg`·`tiny-skia`로 긴 변 256px, 흰 바탕 PNG를 그린 뒤 엔진으로 넣고, 넣은 결과에 저장 검증(PD-109)을 다시 적용합니다. SVG는 16MiB까지. 래스터 그림(`<image>`)은 그리지 않습니다. (5) **페이지** `hwp-session.js`가 내보내기·검증 뒤에 `getPrintPages({from: 0, count: 1})`(`print-pages-v1`, 새 편집기 변경 없음)로 첫 쪽 SVG를 받아 저장 전달에 `thumbnailSvg`로 실어 보냅니다(`deliverInPlace`·`deliverSavedFile`·브리지가 그대로 전달). 편집기가 인쇄를 지원하지 않거나, 오류·3초 초과·SVG가 없음·길이가 16MiB÷3자(UTF-8로 16MiB를 넘을 수 있는 길이) 초과면 그림 없이 전달합니다. (6) **저장은 절대 실패하지 않음**: 그림을 그리지 못하거나(SVG 오류·상한), 끼우지 못하거나, 끼운 결과가 검증을 통과하지 못하면 편집기가 만든 바이트 그대로 저장하고 사유는 stderr에만 남깁니다. 이 단계(글꼴 DB 구축, SVG 읽기·그리기, OLE2·ZIP 처리)에서 패닉이 나도 `catch_unwind`로 잡아 같은 방식으로 저장합니다(그렇지 않으면 `spawn_blocking`의 `JoinError`로 저장 명령 전체가 실패해, 같은 문서는 매번 같은 SVG라 영구히 저장할 수 없음). (7) **PDF `/Thumb`는 넣지 않음**: 선택 항목이고 쪽 미리보기를 쓰는 뷰어가 없습니다. 이 Mac의 PDF 45개(Chromium·PDFium·iOS·Canva·한글 문서 작성기·iText 등이 만든 것)에 `/Thumb`가 든 것은 0개이고, 탐색기·Quick Look·브라우저·PDF 뷰어는 본문의 첫 쪽을 직접 그립니다. 우리 PDF 작성기에는 쪽을 그리는 경로도 없습니다.
- **검토 순서(구현 원칙)** (1) 허용 라이선스의 유지되는 크레이트: OLE2·ZIP을 다시 쓰는 일은 이미 있는 `Ole2ContainerCodec`·`ZipArchiveCodec`가 하고, 새 크레이트가 필요 없습니다(`cfb`는 rhwp 쪽 의존성이고 엔진 크레이트는 무의존 원칙, PD-053/095). (2) 이미 쓰는 의존성: SVG→PNG는 셸이 이미 쓰는 `usvg`·`fontdb`와 `svg2pdf`가 끌어오는 `resvg` 0.45·`tiny-skia` 0.11을 직접 의존성으로 선언했습니다(Cargo.lock에는 데스크톱의 의존 목록 두 줄만 늘었고 새 크레이트 없음). 래스터 그림 그리기(`raster-images`)는 디코더 크레이트 8개를 더하므로 쓰지 않았습니다. (3) 참조 구현 이식: 없음. rhwp의 `PrvImage`·`Preview/PrvImage.png` 읽기·쓰기 규칙(`cfb_writer.rs`, `serializer/hwpx/mod.rs`)을 확인해 같은 이름·형식으로 맞췄고, 같은 rhwp 파서(`extract_thumbnail_only`)로 결과를 확인했습니다.
- **Why** 편집기가 쪽을 그릴 수 있는 한글 문서만 정확한 그림을 만들 수 있고, 첫 쪽 SVG는 PDF 내보내기가 이미 받는 것과 같아 새 편집기 변경이 필요 없습니다. 문서의 PrvImage를 바꾸는 일을 엔진 한 곳에 두어 HWP·HWPX·DOCX가 같은 상한과 검사를 쓰고, 호출자가 그릴 수 없는 경우(DOCX 등)에도 같은 함수가 그대로 쓰입니다.
- **Trade-offs** 그림 안의 래스터 사진은 처음에는 흰 칸으로 남는다고 보았으나 실제로는 PD-142 이후 그려집니다(PD-145). 64MiB가 넘는 문서는 그림을 새로 만들지 않아 열 때의 그림(없으면 없음)이 그대로 남습니다. HWP·HWPX는 풀었다 다시 묶으므로 컨테이너를 새로 쓰며(HWPX는 ZIP 항목 날짜가 없어지고 그림 같은 이미 압축된 항목은 저장 형식으로 둠, HWP는 OLE2 시각·루트 CLSID 없음) 이 앱의 HWP 쓰기와 같은 방식입니다. 비한글 문서(DOCX 등)는 쪽을 그릴 곳이 없어 저장 파일에 그림이 없습니다. 저장 한 번에 SVG 한 쪽이 셸로 더 건너가고(수십 KB~수 MB) 그리는 시간이 더해집니다.
- **Alternatives rejected** 편집기에서 PNG를 만들어 보내는 것(캔버스 렌더링은 편집기 번들 변경이 필요하고 글꼴이 PDF 경로와 달라짐), rhwp 문서 모델의 `preview`를 고치는 것(고정된 WASM에 호출이 없음), 문서를 다시 쓰지 않고 OLE2·ZIP 바이트에 직접 끼우는 것(FAT·중앙 디렉터리 직접 조작의 위험, 측정상 다시 묶는 비용이 0.1~20ms), PDF `/Thumb`.
- **측정** 이 Mac(릴리스 빌드, `EASYOFFICE_THUMBNAIL_SVG_DIR=<쪽 SVG 폴더> cargo test --release -p easyoffice-desktop thumbnails_are_measured -- --ignored --nocapture`): rhwp 샘플 6개의 첫 쪽(SVG 27KB~3.6MB)을 그리는 데 8~300ms(글꼴 DB 첫 구축 17ms는 PDF 내보내기와 공유), PNG 6~55KB, 문서 크기 증가는 최대 55KB(원래 큰 그림이 있던 문서는 오히려 줄어듦: 68KB→13KB). 풀어 다시 묶는 시간은 HWP 41MiB 9ms, HWPX 48MiB 20ms, 112KB HWPX 2ms. rhwp로 내보낸 샘플 HWP 6개·HWPX 1개에 넣은 뒤 rhwp 파서가 다시 열어 문단 수가 같고 `extract_thumbnail_only`가 새 PNG(181x256)를 돌려줌을 확인했습니다.
- **Verified by** `cargo test -p easyoffice-formats --test embedded_thumbnail`(HWP·HWPX·DOCX에 항목 생성과 나머지 항목 동일, 교체·중복 없음, PNG 기본 형식 중복 없음, 비PNG 썸네일 관계 거부, 깨진·긴 변 초과·바이트 초과·CRC 오류 PNG와 64MiB 초과·잘린 컨테이너 거부, rich 작성기 `None` 동일), `cargo test -p easyoffice-desktop save_thumbnail`(A4·가로·가는 쪽·거대한 선언 크기에서 긴 변 256, 흰 바탕과 색 픽셀, 읽을 수 없는·큰 SVG 거부와 외부 파일 href 무시, 세 형식이 시작 화면 읽기(`extract_thumbnail`)로 같은 PNG를 돌려줌, 실패해도 저장 바이트 불변, 단계에서 패닉이 나도 원본 바이트로 저장(주입 시험), 파일로 쓴 뒤 읽기와 저장 검증), `node --test scripts/test-webui-hwp-session.mjs scripts/test-webui-desktop-bridge.mjs`(첫 쪽 요청 순서와 인수, 지원 없음·오류·상한 초과에서 그림 없이 전달, 암호 문서 경로, 브리지가 문자열일 때만 `thumbnailSvg` 전송).
- **Revisit when** 비한글 문서를 그리는 경로(쪽 렌더링)가 생길 때(DOCX 저장에 `encode_rich_with_thumbnail` 연결), 참조 앱이 PDF 썸네일을 넣을 때.

## PD-133. 저장 실패는 셸이 네이티브 오류 창으로 알리고, 페이지로는 경로와 시스템 원문을 보내지 않음

- **Status** Implemented (QA 요청 2026-11-08)
- **Context** 저장 쓰기가 실패하면 `save_file.rs`가 `파일을 저장하지 못했습니다 ({전체 경로}): {시스템 오류 원문}`을 명령 오류로 돌려 경로와 운영체제 문구가 페이지로 나갔습니다(PD-088 위반). 참조 앱은 저장 실패를 제목 "저장 실패", 본문 "문서를 저장하지 못했습니다.\n{사유}"의 오류 창으로 보입니다(`apps/studio-host/src/core/tauri-bridge.ts` 459행, 492~499행).
- **Decision (`PD-133`)** (1) 쓰기 실패는 `SaveError`(`save_error.rs`)로 돌려주고, 사유는 `io::ErrorKind`로 고릅니다: `StorageFull`·`QuotaExceeded`는 "디스크 공간이 부족합니다.", `PermissionDenied`·`ReadOnlyFilesystem`은 "쓰기 권한이 없습니다.", 그 밖은 "파일을 저장하지 못했습니다 (파일 이름)"(이름은 파일 이름만). 경로와 시스템 원문은 `eprintln!`에만 남습니다. (2) `save_document_file`이 쓰기에 실패하면 셸이 직접 오류 창(제목 "저장 실패", 본문 "문서를 저장하지 못했습니다.\n{사유}")을 창 위에 띄우고, 명령은 사유 한 문장만 오류로 돌려줍니다(페이지는 상태 줄에 "문서 저장 실패 (HWP): {사유}"를 그대로 보이며 별도 안내 창을 만들지 않습니다). PDF 내보내기의 쓰기 실패는 같은 사유 문장을 오류로 돌려주고 페이지의 PDF 실패 안내가 보입니다. (3) 같은 기준으로 최근 문서 지우기와 새 창 만들기 실패도 시스템 원문 대신 고정 문장을 돌려주고 원문은 로그에만 둡니다. (4) 구조 시험(`path_leak_tests.rs`)이 페이지로 가는 오류를 만드는 소스에서 `eprintln!` 밖의 `.display()`와 `IpcError::new`·`WindowError::new` 인수의 경로 사용을 막습니다.
- **Why** 경로는 사용자 폴더 구조를, 시스템 원문은 환경 정보를 드러내고, 문구를 페이지가 정하면 셸 경계(PD-088)가 느슨해집니다. 참조 앱과 같은 제목·본문으로 맞추면 사용자가 보는 안내가 같습니다.
- **Trade-offs** 사유가 일반 문장 "파일을 저장하지 못했습니다 (이름)"이면 원인 구분이 줄어듭니다(자세한 내용은 로그). 오류 창과 상태 줄이 같은 사유를 두 번 보이지만 상태 줄은 이미 있던 한 줄입니다.
- **Alternatives rejected** 페이지가 오류 창 문구를 정하는 것(PD-088의 문구는 셸 소유), 시스템 원문을 그대로 두는 것(운영체제·언어마다 달라 안내가 되지 못함).
- **Verified by** `cargo test -p easyoffice-desktop save_error path_leak save_file pdf_job`(`ErrorKind`별 문구, 폴더 이름과 `os error`가 사용자 문구·오류 창 본문·명령 오류 JSON에 없음, 없는 폴더에 쓰기 실패가 파일 이름만 말함, 구조 시험이 누출 예를 잡음).
- **Revisit when** 다른 쓰기 경로(PDF 내보내기, 저장 후 미리보기)에서도 오류 창을 셸이 직접 띄우기로 할 때, 참조 앱의 문구가 바뀔 때.

## PD-134. 메뉴·도구 단추의 이름은 실제 동작과 같아야 함

- **Status** Implemented (QA 요청 2026-11-08)
- **Context** 페이지 메뉴와 도구 상자의 일부 항목은 이름이 약속하는 기능과 연결된 명령이 달랐습니다: `모양 복사`·`글자 모양`은 `home-style`(리본 이름 "스타일 적용", 글자 크기 단계 순환), `찾아 바꾸기(E)`는 명령 팔레트, `개체 속성`은 `layout-paper`(용지 규격 전환)를 실행했습니다.
- **Decision (`PD-134`)** 이름대로 하는 기능이 앱 모듈에 없는 항목은 지웁니다(새 기능·새 화면 없음): 편집 > `모양 복사`와 `찾아 바꾸기(E)`, 입력 > `개체 속성`, 도구 상자의 `모양 복사`·`개체 속성` 단추(쓰지 않게 된 아이콘 두 개도 제거). 기능은 있으나 이름이 다른 항목은 기능의 이름으로 고칩니다: 서식 > `글자 모양`과 도구 단추, macOS 서식 메뉴의 항목(`format:char-shape` -> `format:style`)은 `스타일 적용`. `home-style`과 `layout-paper`는 다른 메뉴 항목(`스타일 적용`, `편집 용지`)으로 계속 닿으므로 모듈별 리본 명령 도달 시험은 그대로입니다. 페이지 메뉴와 macOS 메뉴의 1:1 시험에 두 곳의 이름이 같은지 더했습니다.
- **Why** 이름과 동작이 다르면 사용자가 없는 기능이 있다고 믿거나 엉뚱한 변경을 일으킵니다.
- **Trade-offs** 참조 앱 편집기 메뉴에 있는 항목 일부가 이 앱의 문서·시트·슬라이드 화면에는 없습니다(한글 편집기 안의 메뉴는 편집기가 제공).
- **Alternatives rejected** 이름을 그대로 두고 팔레트로 연결하는 것(없는 기능을 있는 것처럼 보임), 새 찾아 바꾸기·모양 복사를 만드는 것(범위 밖).
- **Verified by** `node --test scripts/test-webui-menu-bar.mjs scripts/test-webui-native-menu.mjs scripts/test-webui-menu-state.mjs`(`home-style`·`layout-paper` 이름, 약속한 기능 이름이 메뉴·도구 단추에 없음, 네이티브와 페이지의 이름 일치), `cargo test -p easyoffice-desktop native_menu`.
- **Follow-up (QA 요청 2026-11-08)** 같은 기준으로 남은 불일치를 정리했습니다: `문단 모양`(메뉴·도구 단추·macOS `format:para-shape`)은 편집기 막대의 양쪽 정렬을 누르므로 `양쪽 정렬`(`format:justify`)로, 머리말·꼬리말 도구 단추 두 개와 각주·미주 단추 두 개는 같은 명령 하나이므로 `머리말/꼬리말`·`각주` 단추 하나씩만 남기고, 그림·차트 아이콘이 없는 `그림 삽입` 단추(`insert-chart`는 메뉴 `차트 개체`로 계속 닿음)는 지우며(쓰지 않게 된 아이콘 4개 제거), 메뉴 `명령 검색(F)`는 `(F)` 표기를 빼고 `명령 검색`으로 둡니다(Ctrl+F가 여는 것은 명령 팔레트). 한글 모듈에서 `머리말/꼬리말`은 `행정 공문서 두문·결재란`을, macOS의 `찾기`(Cmd+F)는 한글 창 밖에서 명령 팔레트를 엽니다(알려진 차이).
- **Revisit when** 앱 모듈에 모양 복사나 찾아 바꾸기가 생길 때.

## PD-135. 시작 창에 메뉴 줄·상태 줄을 보이고, 한글 창 파일 메뉴를 참조 앱 순서로

- **Context** PD-121이 "다른 창(시작 창 등)의 메뉴로 닿는다"고 적었지만 시작 창은 `home.css`가 메뉴 줄을 포함한 모든 자식을 숨겨 Windows·Linux(네이티브 메뉴 없음)에서 제품 정보·테마·업데이트 알림 끄기·최근 문서 대화상자에 닿을 곳이 없었습니다. 참조 앱의 시작 화면은 편집기 창 안의 층이라 메뉴·도구 상자·상태 줄이 그대로 보입니다(`apps/studio-host/src/main.ts` 96-111, `styles/home-screen.css` 1-8). 한글 창 파일 메뉴(`apps/hwp-studio/index.html`)는 참조 앱(`apps/studio-host/index.html` 16-29)과 항목 순서·구성이 달랐습니다.
- **Decision (`PD-135`)** (1) 시작 창(`data-window-kind="start"`)은 페이지 메뉴 줄(`#menu-bar`)과 상태 줄(`#status-bar`)을 보이고, 시작 화면은 그 사이에 들어갑니다(`home-screen.js`의 `mount`가 메뉴 줄 바로 뒤에 넣음). 도구 상자·서식줄·편집 영역·배율 조절은 숨깁니다(문서가 없음). (2) 시작 창은 메뉴 명령 문맥 `START`를 쓰고 기존 회색 처리 규칙(`studioCommandApplies`)에서 `palette`·`view-theme`만 적용되며 나머지 편집 명령은 회색입니다(새 창·열기·최근 문서·테마·제품 정보·닫기처럼 문서가 필요 없는 항목은 그대로). macOS 네이티브 막대의 회색 처리도 같은 규칙을 따릅니다. (3) 한글 창 파일 메뉴는 새로 만들기, 열기, 최근 문서(목록), 최근 문서 대화 상자(`Ctrl+Alt+O`), ─, 저장, 다른 이름으로 저장(HWP·HWPX), PDF로 내보내기, 편집 용지(`F7`, 편집기 명령 `file:page-setup`), 인쇄, ─, 닫기, ─, 제품 정보 순입니다. PD-093 (1)의 "셸 명령 여섯 개" 구성과 허용 목록은 이 결정으로 셸 명령 열 개와 편집기 명령 하나(편집 용지)가 됩니다. 셸 명령 화이트리스트(편집기 `shell-menu.ts`, 셸 `hwp-host.js`)에 `recent-dialog`(셸의 최근 문서 대화상자)와 `about`(셸의 제품 정보 창)을 더했습니다. (4) PD-121의 "시작 창 메뉴로 닿는다"는 서술은 이제 사실이며 위 Trade-offs를 고쳤습니다.
- **Why** 참조 앱과 같은 구성이 목표이고(QA 요청), Windows·Linux의 시작 창과 한글 창에서 정보·테마·최근 문서 대화상자에 닿는 길이 있어야 합니다. 새 디자인 요소가 없고 기존 메뉴 줄·상태 줄·회색 처리 규칙을 그대로 씁니다.
- **Trade-offs** 참조 앱은 최근 문서 항목이 대화상자 하나뿐이지만 최근 문서 하위 목록은 PD-093 (1)로 정해져 있어 둘 다 둡니다(대화상자 항목은 "최근 문서 대화 상자…"로 구분). 참조 앱에 있는 새 창(`Ctrl+Shift+N`)은 PD-088로, 닫기·HWPX 저장은 PD-093·PD-085로 의도적으로 다르게 둡니다. 시작 창에서 도구 상자는 보이지 않습니다(참조 앱은 보임). 실기기(Windows·Linux)의 화면은 확인하지 못했습니다.
- **Alternatives rejected** 시작 창에 도구 상자까지 보이는 것(전부 회색이라 쓸모 없음), 시작 창 전용 메뉴를 새로 만드는 것(참조 앱에 없음), 대화상자 항목만 두고 하위 목록을 없애는 것(PD-093 (1) 결정을 바꿈).
- **Verified by** `node --test scripts/test-webui-windows.mjs`(시작 창에서 메뉴 줄 < 시작 화면 < 상태 줄 순서, 편집 명령 회색·`palette`·`view-theme` 활성, CSS가 메뉴 줄·상태 줄을 숨기지 않음), `scripts/test-webui-hwp-shell.mjs`(명령 목록 공유, `recent-dialog`·`about` 전달, 파일 메뉴 항목 순서), `scripts/test-hwp-studio.mjs`.
- **Revisit when** 실기기에서 Windows·Linux 시작 창을 확인한 뒤, 참조 앱이 최근 문서 하위 목록을 넣거나 시작 창의 도구 상자 표시가 필요해질 때.

## PD-136. 닫을 때의 저장 확인은 셸의 네이티브 세 단추 창

- **Status** Implemented (QA 요청 2026-11-08)
- **Context** 변경 내용이 있는 창을 닫으면 페이지의 `<dialog>`(`close-confirm-dialog`)가 저장 / 저장 안 함 / 취소를 물었습니다. 참조 앱은 같은 질문을 네이티브 메시지 창으로 묻습니다(`apps/studio-host/src/core/tauri-bridge.ts` 470-490행: 제목 "저장 확인", 본문 "`{파일 이름}`의 변경 내용을 저장할까요?", 종류 경고, 단추 저장 / 저장 안 함 / 취소). 페이지 대화상자는 참조 앱에 없는 자체 디자인이었습니다.
- **Decision (`PD-136`)** (1) 새 앱 명령 `confirm_close`(`confirm_dialog.rs`, 인수 없음)가 호출한 창 위에 `tauri-plugin-dialog` 2.8.4의 `MessageDialogButtons::YesNoCancelCustom`으로 세 단추 창을 띄우고 `save`·`discard`·`cancel` 한 낱말만 돌려줍니다. 문구(제목, 본문 틀, 단추)는 Rust 상수이고 파일 이름은 페이지가 보내지 않고 셸이 창 레지스트리의 표시 이름(제목 표시줄과 같은 이름, 비면 "제목 없음")으로 만듭니다. 이름의 제어 문자는 공백으로 바꿔 창에 줄을 더하지 못하게 합니다. 경로는 관여하지 않습니다. (2) 답은 저장 단추(예 또는 그 단추의 라벨)와 저장 안 함 단추(아니요 또는 그 라벨)만 `save`·`discard`로 읽고, 창을 닫은 경우를 포함한 그 밖의 모든 답은 `cancel`입니다. 알 수 없는 답과 거부된 호출도 페이지가 `cancel`로 보아 닫기가 작업을 버리는 일이 없게 합니다. (3) 닫기 흐름은 그대로입니다: 확인 신호(`ack_close_prompt`)는 네이티브 창을 호출하기 직전에 보내고(PD-113, 10초 탈출구 유지), 창이 떠 있는 동안 온 두 번째 닫기 요청(Cmd+W, Cmd+Q, 끝내기 큐)은 열린 확인에 합류해 신호만 다시 보내며 두 번째 창을 띄우지 않습니다. 취소와 저장 실패는 `end_close_prompt`로 끝내기 큐를 멈춥니다(PD-119). 암호 문서의 닫기 저장(PD-120)은 저장 답 뒤에 이어집니다. (4) `index.html`의 `close-confirm-dialog`와 쓰이지 않게 된 `.primary-btn` 규칙을 지웁니다(`.close-dialog*` 스타일은 제품 정보 창이 계속 씁니다). `--serve` 미리보기에는 셸이 없어 브라우저 `confirm`이 대신합니다.
- **Why** 참조 앱과 같은 모양·동작이 목표이고(QA 요청), 문구를 셸이 갖는 쪽이 페이지가 임의 문구를 네이티브 창에 올리지 못하게 하는 PD-120의 경계와 같습니다. 창 모달이라 닫기 중에 문서 편집이 막힙니다.
- **Trade-offs** 앱 명령이 26개로 늘었습니다(`confirm_dialog`와 달리 종류를 받지 않는 별도 명령, 결과가 둘이 아니라 셋이어서). 네이티브 단추 순서와 모양은 운영체제가 정합니다. 이름이 없는 창은 참조 앱의 "현재 문서" 대신 제목 표시줄과 같은 "제목 없음"으로 부릅니다.
- **Alternatives rejected** 페이지가 이름을 넘기는 것(페이지가 문구를 정하게 됨), `confirm_dialog`에 세 번째 결과를 억지로 넣는 것(불리언 계약이 깨지고 모든 호출부가 바뀜), 페이지 대화상자 유지(참조 앱에 없는 자체 디자인).
- **Verified by** `cargo test -p easyoffice-desktop confirm_dialog windows`(제목·단추·본문 문구, 이름의 제어 문자, 예/아니요/라벨 → 저장/저장 안 함, 그 밖의 답 → 취소, 직렬화, 표시 이름 대체값, 명령 목록과 권한 1:1), `node --test scripts/test-webui-windows.mjs`(네이티브 창이 한 번만 뜨고 인수 없음, 신호가 창보다 먼저, 두 번째 요청 합류, 취소·저장·저장 안 함, 알 수 없는 답과 거부는 창을 닫지 않음, 페이지에 대화상자 없음), `scripts/test-webui-desktop-bridge.mjs`. 실제 네이티브 창의 모양과 단추 순서는 실기기 확인 대상입니다.
- **Revisit when** 참조 앱의 문구나 단추가 바뀔 때, 실기기에서 운영체제별 단추 순서가 어긋날 때.

## PD-137. 원본 파일이 지워졌거나 옮겨졌으면 제자리 저장 전에 묻기

- **Status** Implemented (QA 요청 2026-11-08). PD-093 (7)을 개정합니다.
- **Context** PD-093 (7)은 추적하던 원본이 없어지면(`Missing`) 묻지 않고 같은 자리에 다시 만들었습니다. 사용자가 일부러 지웠거나 다른 폴더로 옮긴 파일이 저장 한 번에 되살아나거나 옛 자리에 사본이 생깁니다. 참조 앱은 이 경우도 외부 변경 확인 창으로 사유 "원본 파일이 삭제되었거나 이동되었습니다"를 보이고 덮어쓰기 여부를 묻습니다(`apps/desktop/src-tauri/src/state.rs` 518-525행, 확인 창은 `apps/studio-host/src/core/tauri-bridge.ts`의 `confirmExternalOverwriteIfNeeded`).
- **Decision (`PD-137`)** 제자리 저장의 `save_in_place`는 디스크 상태가 `Unchanged`가 아니면(`Changed` 또는 `Missing`) 확인 함수에 그 상태를 넘겨 묻습니다. `Missing`이면 기존 외부 변경 확인 창(제목 "외부 변경 감지", 단추 덮어쓰기 / 저장 취소)의 본문에 참조 앱과 같은 사유 줄을 파일 줄 뒤에 더합니다: "원본 파일이 EasyOffice 밖에서 변경되었습니다.\n파일: {이름}\n원본 파일이 삭제되었거나 이동되었습니다\n\n그대로 저장하면 외부에서 변경된 내용이 사라질 수 있습니다." `Changed`의 본문은 그대로입니다. 덮어쓰기를 고르면 같은 자리에 다시 만들고(임시 파일 + 이름 바꾸기, 새 지문 기록), 저장 취소를 고르면 아무것도 만들지 않고 `cancelled`를 돌려줍니다(지문은 그대로라 다음 저장도 다시 묻습니다). 파일은 이름만 말하고 경로는 창에 나오지 않습니다(PD-088).
- **Why** 참조 앱과 같은 동작이 목표이고, 없어진 파일을 묻지 않고 만드는 것은 사용자가 의도하지 않은 쓰기입니다.
- **Trade-offs** 폴더째 지워졌거나 매체가 빠진 경우에도 먼저 묻고, 덮어쓰기를 골라도 쓰기가 실패하면 저장 실패 창(PD-133)이 이어집니다. 참조 앱의 사유 줄 뒤에 오는 "외부에서 변경된 내용이 사라질 수 있습니다" 문장은 삭제된 경우에는 맞지 않지만 참조 앱과 같게 둡니다.
- **Alternatives rejected** 사유별로 다른 본문과 단추(참조 앱과 달라짐), 묻지 않고 다른 이름 저장 대화상자로 보내는 것(참조 앱에 없음).
- **Verified by** `cargo test -p easyoffice-desktop save_file main`(지워진 원본에 `Missing`으로 묻고, 취소하면 파일이 생기지 않고 임시 파일도 남지 않음, 덮어쓰기하면 다시 만듦, 바뀐 원본은 `Changed`로 묻고 바뀌지 않은 원본은 묻지 않음, 본문 문구에 경로 없음). 실제 네이티브 창은 실기기 확인 대상입니다.
- **Revisit when** 참조 앱의 사유 문구가 바뀔 때, 삭제된 원본에 다른 이름 저장을 권해야 할 때.

## PD-138. 단축키·끌어 놓기 강조·CSS 값·macOS 메뉴 순서·창 기본값을 참조 앱에 맞춤

- **Status** Implemented (QA 요청 2026-11-08). PD-103 (6), PD-124 (2), PD-099의 창 크기 한도를 개정합니다.
- **Context** 참조 앱(`main.ts` 244-287, 342-354·`desktop-events.ts` 76-90·`menu.rs` 79-114·`windows.rs` 7-13, 77-118)과 남은 차이: Windows·Linux에 Ctrl+Shift+S와 Ctrl+=·Ctrl+-가 없음, 끌어오는 동안 문서 영역 강조 없이 토스트만, 메뉴 줄 34px, 업데이트 카드 위치·그림자, 홈 미리보기 여백, 제품 정보 틀, macOS 파일·보기 메뉴 구분선, 창 최소 크기 1024x680과 모니터 전체 92% 한도.
- **Decision (`PD-138`)** (1) 단축키: `Ctrl+Shift+S`(다른 이름으로 저장, 셸 우선·편집기 전달 목록에 추가: 10개, 상한 16), `Ctrl+=`·`Ctrl+Shift++`(확대), `Ctrl+-`(축소)를 단축키 표에 더하고 페이지 메뉴에 표기합니다(`Ctrl+0`은 이미 있음). 한글 편집기는 확대·축소·100% 키를 스스로 처리하므로 전달하지 않습니다. 편집기 쪽 Ctrl+Shift+S의 셀 블록 합계는 임베드에서 파일 명령이 등록되지 않아 이벤트만 삼키는 경로였으므로(`main.ts` 주석) 전달해도 잃는 기능이 없습니다. `Alt+N`(같은 창 새 문서)은 새 창 원칙(PD-088)과 달라 넣지 않습니다. Windows 웹뷰 `zoomHotkeysEnabled`는 켜지 않습니다: 웹뷰 쪽 확대는 앱의 자체 배율 명령(상태 줄 배율·`Ctrl+=`)과 이중으로 걸려 문서 배율과 화면 배율이 엇갈립니다. (2) 끌어 놓기: 지원하는 파일이 창에 들어오면 셸이 인수 없는 `__EASYOFFICE_DRAG_ON__()`을, 나가거나 놓으면 `__EASYOFFICE_DRAG_OFF__()`를 부릅니다. 페이지는 문서 영역(`.editor-pane`)에 `drag-over`(배경 `--ui-selected`, 3px 파선 `--focus-ring`, 안쪽 3px)를 켜고 끄며 상태 줄에 "파일을 놓으면 문서를 엽니다"를 보입니다(참조 앱도 상태 메시지). 한글 창은 임베드 RPC `setDragOver({active})`(capability `drag-over-v1`)로 편집기의 `#scroll-container`에 전달합니다. PD-124 (2)의 토스트는 없앱니다. (3) CSS: 메뉴 줄 28px·경계선 `--ui-border-light`(오른쪽 단추는 24px·22px), 업데이트 카드는 왼쪽 아래(16px/48px)·`--shadow-dialog`, 홈 미리보기는 불러온 뒤 여백 14px·둥근 모서리·가벼운 그림자, 홈 안내 문구는 참조 앱 원문, 제품 정보는 `dialog-wrap` 틀(제목 "제품 정보", 460px, 가운데 정렬, 제품명·"Version x.y.z")이며 고지·라이선스 표·진단 칸은 유지합니다. (4) macOS 네이티브 메뉴: 파일은 새 문서 하위 메뉴 뒤에 구분선, 이어서 열기·최근·저장·다른 이름으로 저장(최근과 저장 사이 구분선 삭제), 보기는 확대/축소/100% 실제 배율, 구분선, 쪽 맞춤/폭 맞춤. 메뉴 막대 순서는 그대로입니다(PD-102). (5) 창: 최소 960x720, 새 창은 기본 모니터의 작업 영역(메뉴 막대·독·작업 표시줄 제외)에서 여백 24px와 창 틀 높이 72px를 뺀 크기의 85% 이하, 작업 영역 가운데(겹침 방지 28px 계단 이동은 유지). PD-099의 기억한 위치·크기 복원 방식은 그대로입니다.
- **Why** 참조 앱과 같은 모양·동작이 목표입니다(QA 요청). PD-103 (6)의 오른쪽 위 위치와 PD-099의 92% 한도는 참조 앱과 다를 근거가 기록돼 있지 않았습니다.
- **Trade-offs** 작업 영역은 기본 모니터만 보며(참조 앱은 활성 창의 모니터), 시작 창에는 끌어 놓기 강조 대상 편집 영역이 없어 상태 줄 안내만 보입니다. 창 틀 높이 72px는 참조 앱 값을 따른 것으로 운영체제별 실측은 아닙니다.
- **Alternatives rejected** 편집기의 확대·축소 키까지 전달하는 것(편집기가 이미 처리), 끌어 놓기 신호에 경로·개수를 싣는 것(PD-088: 페이지에 경로를 주지 않음), 참조 앱에 없는 별도 덮개 요소.
- **Verified by** `node --test scripts/test-*.mjs`(단축키 표와 전달 목록·메뉴 표기·확대·축소·100% 동작, 끌어 놓기 신호와 `drag-over` CSS, `setDragOver` 라우터·핸들러·capability, 메뉴 줄·카드·미리보기·제품 정보 CSS 값), `cargo test -p easyoffice-desktop`(`native_menu`의 파일·보기 순서, `window_commands`의 85% 한도·가운데 배치, `open_feedback`의 인수 없는 신호, 최소 크기).
- **Revisit when** 실기기에서 Windows·Linux 창 틀 높이와 작업 영역을 확인한 뒤, 참조 앱의 단축키·창 기본값이 바뀔 때.

## PD-139. 한글 창 인쇄는 셸의 `print_webview`로 웹뷰를 네이티브 인쇄

- **Status** Implemented (QA 요청 2026-11-08). PD-104 (6)을 개정합니다. 실제 인쇄 대화상자는 macOS·Windows·Linux 수동 확인 전.
- **Context** PD-104는 숨은 루트를 쌓은 뒤 웹 `window.print()`를 불렀습니다. 그러나 macOS의 WKWebView에서는 wry 0.57.1의 UI delegate(`wkwebview/class/wry_web_view_ui_delegate.rs`)가 파일 선택·미디어 권한·새 창 콜백만 구현하고 인쇄 콜백은 구현하지 않아 `window.print()`가 아무것도 하지 않을 가능성이 큽니다(`window.confirm`이 답 없이 취소되던 PD-120과 같은 원인). 같은 wry에는 웹뷰 인쇄가 따로 있습니다: macOS `NSPrintOperation`(`wkwebview/mod.rs` 949-985), Linux WebKitGTK `PrintOperation`(`webkitgtk/mod.rs` 779-783), Windows WebView2 `window.print()` 실행(`webview2/mod.rs` 1886-1891). 참조 앱은 `print_webview` 명령이 `WebviewWindow::print()`를 부릅니다(`apps/desktop/src-tauri/src/commands.rs` 308-312).
- **Decision (`PD-139`)** (1) 새 앱 명령 `print_webview`(`print_webview.rs`, 인수 없음, `ShellCaller` 필수)가 호출한 창의 웹뷰를 `WebviewWindow::print()`로 인쇄합니다. 인수가 없으므로 페이지는 무엇을 인쇄할지 정하지 못하고, 실패 문구는 고정 한 줄이며 시스템 오류는 로그로만 남깁니다. 앱 명령은 27개입니다. (2) `hwp-print.js`는 숨은 루트에 10쪽 묶음으로 쪽을 다 쌓고 진행·취소를 거친 뒤(PD-104 그대로), 셸이 있으면 `desktop-bridge.js`의 `printWebview`(`print_webview`)를, 없으면(`--serve` 브라우저) 웹 `window.print()`를 부릅니다. 취소하면 어느 쪽도 부르지 않습니다. (3) 정리는 PD-104 그대로 `afterprint`와 5분 뒤 늦은 정리입니다. 네이티브 명령은 대화상자를 요청하면 바로 돌아오고 끝을 알리지 않으므로, 쪽은 명령이 돌아온 뒤에도 문서에 남습니다. 참조 앱도 같은 방식입니다(`print-dialog.ts`). (4) 인쇄는 같은 문서를 대상으로 하므로 `shell.css`의 `@media print`(숨은 루트만 보임)와 생성형 `@page` 규칙은 그대로 적용됩니다. 스모크 `print` 단계는 명령을 부르지 않고 숨은 루트·규칙 수만 확인하므로(CI가 인쇄 창에서 멈추지 않음) 바꾸지 않습니다.
- **Why** 참조 앱과 같은 동작이 목표이고(QA 요청), macOS에서 인쇄 단추가 반응이 없으면 한글 창 인쇄 기능이 없는 것과 같습니다.
- **Trade-offs** 앱 명령이 하나 늘고(26 → 27) 새 권한 `allow-print-webview`가 생깁니다. 인쇄 완료 신호가 없어 `afterprint`가 오지 않는 플랫폼에서는 5분 뒤에야 숨은 루트가 지워집니다(화면에는 보이지 않음). 쪽 범위·매수 같은 인쇄 옵션은 운영체제 대화상자가 정합니다.
- **Alternatives rejected** `window.print()` 유지(macOS에서 동작하지 않을 가능성), wry UI delegate에 인쇄 콜백을 직접 구현(의존성 수정·유지 부담, Tauri가 이미 제공), 편집기 iframe 안에서 인쇄(PD-104가 기각).
- **Verified by** `node --test scripts/test-webui-hwp-print.mjs`(셸이 있으면 `printWebview` 한 번과 `window.print` 호출 없음, 셸이 없으면 `window.print`, 취소하면 어느 쪽도 부르지 않음, 명령 실패 시 쪽 정리와 메시지, 스모크는 명령을 부르지 않음, capability에 `allow-print-webview`), `scripts/test-webui-desktop-bridge.mjs`(명령 이름), `cargo test -p easyoffice-desktop`(명령 목록·capability 1:1과 27개, 모든 명령이 `ShellCaller`를 받음, 경로 유출 가드).
- **Revisit when** 실기기에서 `afterprint`가 오지 않아 쪽이 오래 남는 것이 확인될 때, Tauri가 인쇄 완료 콜백이나 인쇄 옵션 API를 제공할 때.

## PD-140. Finder의 HWP·HWPX 빠른 보기와 아이콘 썸네일: Quick Look 확장 두 개

- **Status** Implemented (QA 요청 2026-11-08). 배포본(universal)과 실제 Finder에서의 확인은 아직 없음. 이 Mac에서는 앱 번들과 확장을 만들어 시스템의 썸네일 생성 API와 `qlmanage -p`로 확인함.
- **Context** 스페이스바 미리보기와 아이콘 썸네일은 앱 번들 안의 Quick Look 앱 확장(`PlugIns/*.appex`)이 만든다. Finder가 확장에 넘기는 파일은 무엇이든 될 수 있고, 확장은 샌드박스 안에서 시스템이 정한 시간·메모리 안에 답해야 한다. 참조 앱은 같은 확장 두 개를 Swift로 쓰고 rhwp 어댑터와 Skia로 그린다.
- **Decision (`PD-140`)** (1) **확장**: `EasyOfficeQuickLookPreview.appex`(데이터 기반 미리보기, 전 쪽을 PDF 하나로)와 `EasyOfficeQuickLookThumbnail.appex`(파일에 든 `PrvImage`가 있으면 그것, 없으면 첫 쪽을 그린 PNG). 식별자는 `com.aquilaxk.easyoffice.quicklook.preview`·`.thumbnail`, 최소 macOS 12.0(앱 자체의 11.0은 그대로), 지원 형식은 앱이 이미 내보내는 `com.aquilaxk.easyoffice.hwp`·`.hwpx` 둘뿐(PD-122의 "HWP 형식은 이 앱의 UTI만 선언"을 유지). 권한은 참조 앱 그대로 앱 샌드박스와 사용자가 연 파일 읽기 전용, 네트워크 권한 없음. (2) **Rust 쪽** `apps/quicklook/rust`(`easyoffice-quicklook-ffi`, 정적 라이브러리, 저장소 작업공간 밖의 독립 작업공간이라 CI의 `--workspace` 작업이 rhwp를 빌드하지 않음)가 C 함수 셋을 낸다: `eo_ql_render_preview_pdf`, `eo_ql_render_first_page_png`, `eo_ql_extract_embedded_thumbnail`(+ `eo_ql_free_bytes`). 재사용 판단: ① 유지되는 허용 라이선스 크레이트를 그대로 씀: 문서 읽기·쪽 SVG는 고정된 rhwp 서브모듈(수정 없이, `native-skia` 끄고), 파일 안 그림 읽기는 rhwp의 `extract_thumbnail_only`, SVG→PNG는 셸이 이미 쓰는 `resvg`·`tiny-skia`, SVG→PDF는 셸의 `svg2pdf`(같은 vendor 사본)·`pdf-writer`, 글꼴 대체표는 셸의 `pdf_font_fallbacks.rs`를 모듈로 포함(한 곳에서만 고침). ② 이미 쓰는 의존성: 셸의 PD-106 읽기(`preview/`)는 Tauri 크레이트 안이라 쓸 수 없고 rhwp 쪽이 같은 일을 한도(`MAX_THUMBNAIL_BYTES`)와 함께 한다. ③ 이식: 참조 앱의 C ABI와 다쪽 PDF 조립(청크 번호 다시 매기기)만 가져왔고, 쪽을 PNG로 그린 뒤 PDF에 넣던 길(Skia)은 쪽 SVG를 `svg2pdf`로 바로 변환하는 길로 바꿨다(글자는 윤곽선). (3) **상한(Fail-Closed, 신뢰할 수 없는 입력)**: 파일 50MiB(Swift에서 읽기 전에, Rust에서 복사 전에 거부), 쪽 SVG 16MiB, PNG 긴 변 2048px, 미리보기 64쪽·PDF 32MiB(넘으면 그 앞 쪽까지만 PDF로, 첫 쪽이 이미 넘으면 거부)·15초 안 끝난 쪽은 더하지 않음, 호출당 마감(썸네일 15초, 미리보기 30초, 파일 안 그림 5초: 작업 스레드에서 돌리고 마감이 지나면 `STATUS_TIMEOUT`), 마감을 넘겨 남은 작업 스레드가 3개면 새 요청을 거부, 작업 스레드 스택 32MiB, 모든 호출은 `catch_unwind`로 막아 패닉이 C 경계를 넘지 않음(`panic = "abort"` 금지를 테스트가 지킴). 쪽 SVG는 경로·`file:` href를 읽지 않음(데이터 URL만). 실패는 상태 코드와 빈 버퍼이고, Swift는 그림을 만들지 않고 오류를 돌려 시스템이 기본 아이콘·기본 미리보기를 보임(참조 앱은 회색 상자나 문구를 그림). 로그에는 이유 코드와 쪽 수·바이트 수만 남기고 파일 이름·내용은 남기지 않음. (4) **글꼴**: 확장 프로세스는 자기를 담은 앱의 `Contents/Resources/pdf-fonts`(PD-114의 Noto Sans KR)를 먼저, 이어서 시스템 글꼴을 읽고 일반 계열을 그 글꼴로 이음. (5) **빌드·번들**: `scripts/build-quicklook-macos.mjs`(macOS만, 다른 OS는 아무 일 안 함)가 Rust 정적 라이브러리 → 확장마다 `swiftc`(`-dead_strip`) → `codesign`(ad-hoc 또는 `APPLE_SIGNING_IDENTITY`, 위 권한)으로 `target/quicklook/PlugIns`에 쌓고, `--universal`이면 두 아키텍처를 `lipo`로 합침. `tauri.conf.json`의 `bundle.macOS.files`가 이를 `EasyOffice.app/Contents/PlugIns`로 복사. 확장의 버전은 작업공간 버전 또는 릴리스 버전(`EASYOFFICE_VERSION`). (6) **CI·릴리스**: `verify`(macOS)는 크레이트 검사(`scripts/run-quicklook-tests.mjs`: fmt·clippy·릴리스 테스트) → 확장 universal 빌드 → Tauri 빌드 → `scripts/verify-quicklook-bundle.mjs`(PlugIns 존재, 식별자·확장 지점·주 클래스·버전, 지원 UTI가 앱이 내보내는 UTI에 있음, 두 아키텍처, `codesign --verify`, 샌드박스 권한)를 돌림. `release-macos`도 같은 순서. 설정 정합은 `scripts/test-quicklook-config.mjs`가 모든 OS에서 지킴(표·Info.plist·권한·C 헤더와 Swift·Rust 상태 코드·워크플로 순서).
- **Why** 참조 앱과 같은 Finder 경험이 목표이고(QA 요청), 임의 파일을 샌드박스 확장에서 열므로 한도와 시간 마감이 없으면 파일 하나가 Finder의 썸네일 프로세스를 멈추거나 메모리를 소진시킬 수 있다. 시스템 기본 아이콘으로 물러나는 쪽이 틀린 그림을 보이는 것보다 낫다. Rust 쪽을 앱 크레이트와 분리해 엔진·`--workspace` 작업의 빌드 시간과 의존성 그래프를 지킨다.
- **Trade-offs** 확장 하나가 단일 아키텍처로 약 15~17MB(universal이면 두 배), 둘이면 앱이 그만큼 커진다(rhwp 전체가 정적으로 들어감). macOS CI의 빌드 시간이 rhwp 네이티브 빌드(x86_64·arm64 각각) 만큼 늘어난다. 미리보기 PDF의 글자는 윤곽선이라 선택·검색이 안 되고 형식 상자 하나는 쪽당 1MB 안팎이다. 마감을 넘긴 작업은 취소되지 않고 프로세스가 끝날 때까지 남을 수 있어 세 개로 막는다. `.hwp`를 다른 한글 앱이 자기 UTI로 선언한 Mac에서는 그 파일의 형식이 우리 UTI가 아니어서 이 확장이 불리지 않을 수 있다(이 Mac에서 `.hwpx`는 우리 UTI로 풀렸고 `.hwp`는 다른 앱의 UTI로 풀림). 참조 앱은 그런 타사 UTI를 지원 목록에 함께 적지만 위생 가드가 그 이름들을 막고 PD-122가 이 앱의 UTI만 선언하므로 따르지 않았다. 확장 안 파일 접근(앱의 글꼴 폴더 읽기)이 샌드박스에서 막혀도 시스템 글꼴로 물러난다(막혔는지는 배포본 확인 전).
- **Alternatives rejected** 참조 앱처럼 Skia(`native-skia`)로 쪽을 그림(빌드 시 큰 바이너리 내려받기, 셸과 다른 래스터라이저), 쪽을 PNG로 그려 PDF에 넣음(해상도 한도와 용량, 확대하면 흐림), 확장 안 Swift로 HWP를 직접 읽음(파서 중복), 마감 없이 호출(Finder가 파일 하나로 멎음), Rust 크레이트를 저장소 작업공간에 넣음(모든 `--workspace` 작업이 rhwp를 빌드), 타사 UTI를 지원 목록에 더함(위생 가드·PD-122).
- **Verified by** `node scripts/run-quicklook-tests.mjs`(`cargo fmt --check`, `clippy -D warnings`, 릴리스 테스트 25개: 입력 거부·50MiB 초과를 읽기 전에 거부·문서 아닌 파일 Fail-Closed·샘플 HWP/HWPX 첫 쪽 PNG와 긴 변 한도·PDF 쪽 수·파일 안 그림·잘린 문서와 비트를 깬 문서·ZIP 폭탄·쪽 SVG 한도·경로 href 안 읽음(해제하면 실패하는 것을 확인)·터무니없는 쪽 크기·깊은 중첩·마감·패닉·남은 작업 상한), `node --test scripts/test-quicklook-config.mjs`, `node scripts/build-quicklook-macos.mjs`로 만든 `.appex` 두 개와 `tauri build --bundles app`의 `EasyOffice.app/Contents/PlugIns`에 대한 `node scripts/verify-quicklook-bundle.mjs`와 `codesign --verify --deep --strict`, 시스템의 썸네일 생성 API(`QLThumbnailGenerator`)가 `.hwpx`에서 확장을 불러 첫 쪽 그림을 받음(파일 안 그림이 있는 파일과 없는 파일, 깨진 파일은 시스템 기본 아이콘), `qlmanage -p`가 미리보기 확장을 띄워 10쪽 PDF를 만듦(로그의 쪽 수·바이트 수).
- **Follow-up (QA 요청 2026-11-09)** 쪽 SVG 안 래스터 그림은 썸네일·미리보기 PDF 모두 그림 하나 4096x4096, 쪽 합계 4배로 헤더를 먼저 검사해 넘는 그림만 뺍니다(`raster_budget.rs`를 경로로 포함, PD-145).
- **Revisit when** 배포본에서 Finder 확인 중 샌드박스가 글꼴 폴더 읽기를 막거나 확장이 시간 안에 못 끝날 때, 공증을 도입할 때(확장도 같은 신원으로 서명), 앱 크기가 문제가 될 때(두 확장이 같은 정적 라이브러리를 각각 품음).

## PD-141. 다른 앱이 선언한 HWP·HWPX UTI도 연결·Quick Look 대상에 포함, 위생 가드는 정확 문자열 예외

- **Status** Implemented (QA 결정 2026-11-09). PD-125 (2)의 "다른 앱의 UTI는 넣지 않음"과 PD-140의 `.hwp` 위험 항목을 개정합니다. 실제 Finder에서 그 UTI로 풀리는 Mac의 확인은 수동 확인 전.
- **Context** 다른 회사의 앱이 설치된 Mac은 `.hwp`·`.hwpx`를 그 앱의 UTI로 풀 수 있고, 그러면 우리 UTI만 선언한 앱과 Quick Look 확장은 "다음으로 열기"와 미리보기·썸네일 후보에서 빠집니다.
- **Decision (`PD-141`)** (1) `tauri.conf.json`에 확장자 없는 파일 연결 `EasyOffice.HwpOther`·`EasyOffice.HwpxOther`를 더해 그 UTI를 `contentTypes`로만 둡니다. 번들러(tauri-utils `infer_content_types`)는 `exportedType`이 있는 연결에는 그 식별자만 `LSItemContentTypes`에 쓰므로, 같은 연결에 더하면 빠집니다(CI 번들 검증이 실패해 알게 됨). 로컬 `tauri build --bundles app`의 Info.plist에서 별도 문서 형식 두 개로 들어간 것을 확인했습니다. 우리 연결의 `exportedType`은 그대로, 순위는 모두 Alternate라 기본 앱을 가로채지 않습니다. (2) Quick Look 확장 두 개의 `QLSupportedContentTypes`에도 같은 UTI를 더합니다. (3) 위생 가드의 `UTI_EXCEPTIONS`가 그 정확한 문자열 두 개만, 그 문자열을 담는 네 파일(`tauri.conf.json`, 두 `Info.plist`, `scripts/desktop-file-associations.mjs`)에서 통째 토큰으로만 허용합니다. 뒤에 글자가 더 붙은 형태, 다른 식별자, 다른 파일의 같은 문자열은 계속 걸리고 커밋 메시지 가드에는 예외가 없습니다. (4) 구조 검사는 형식마다 우리 UTI와 그 UTI가 함께 선언됐는지(`REQUIRED_UTIS`의 `external`, 번들러 결과를 흉내 낸 `bundledContentTypes` 기준) 확인하고, 번들 검증은 확장의 지원 UTI를 앱이 내보내거나 문서 형식으로 선언한 UTI로 인정합니다.
- **Why** 참조 앱이 같은 UTI들을 함께 선언해 같은 Mac에서도 동작하고, 연결 후보에서 빠지지 않는 것이 목표(QA 결정).
- **Trade-offs** 다른 회사 식별자 문자열이 저장소에 네 파일에 남습니다. 그 UTI를 소유한 앱이 정의를 바꾸면 따라가야 합니다. 테스트는 문자열을 조합해 쓰므로 허용 목록에 넣지 않았습니다.
- **Alternatives rejected** 우리 UTI만 유지(그 UTI로 풀리는 Mac에서 후보·미리보기 없음), 가드 전체 완화(범위가 넓음).
- **Verified by** `node --test scripts/test-about-notice.mjs scripts/test-desktop-file-associations.mjs scripts/test-quicklook-config.mjs`, `node scripts/check-repo-hygiene.mjs`.
- **Revisit when** 그 앱의 UTI 이름이 바뀔 때, 실기기에서 후보·미리보기가 나타나지 않을 때.

## PD-142. 시작 창 카드: 파일 안 그림이 없는 한글 문서는 첫 쪽을 rhwp로 그림

- **Status** Implemented (QA 요청 2026-11-09). PD-106을 개정합니다. 실제 앱 창에서의 화면 확인(macOS·Windows)은 아직 없고, 이 Mac에서는 셸 코드 경로를 테스트·측정으로 확인함.
- **Context** PD-106은 파일에 든 미리보기 그림만 보여 줘서 그림이 없는 한글 파일(다른 프로그램이 저장한 것, 그림을 빼고 저장된 것)은 빈 카드였다. 참조 앱은 항상 엔진으로 첫 쪽을 SVG로 그려(`commands.rs` `render_document_preview`, `home-screen.ts` `loadRecentPreview`) 카드에 넣지만, 경로를 페이지에서 받고 파일을 통째로 읽으며 상한·마감이 없다. 이 셸은 이미 쪽 SVG를 PNG로 그리는 `usvg`/`resvg` 경로(PD-132)를 갖고 있고, Quick Look 확장(PD-140)은 같은 일을 별도 작업공간에서 한다.
- **Decision (`PD-142`)** (1) **순서**: 최근 문서의 파일 안 그림이 있으면 그것(PD-106, 시크 읽기), 없고 확장자가 HWP·HWPX(`WindowKind::Hwp`)이면 rhwp(`DocumentCore`)가 첫 쪽을 SVG로 레이아웃하고 `save_thumbnail::render_png_sized`가 긴 변 480px의 PNG로 그립니다. 다른 형식(DOCX·XLSX·PPTX·PDF 등)은 이번 범위 밖이라 PD-106 그대로(그림이 있으면 그것, 없으면 빈 카드)입니다. (2) **페이지로 가는 것**: PNG뿐입니다. SVG를 페이지에 넣지 않으므로 CSP·스크립트 안전성 검토가 필요 없고, 응답 모양(`{mime, base64}`)·명령 `get_recent_preview(id)`·불투명 id·경로 비노출(PD-088)은 그대로입니다. 페이지 쪽 변경은 Base64 상한을 256KiB에서 1MiB로 올린 것뿐입니다(파일 안 그림은 Rust가 계속 256KiB로 자름). (3) **상한(Fail-Closed, 모두 실패하면 빈 카드)**: 파일 50MiB(Quick Look과 같음, 메타데이터로 먼저 거르고 읽을 때도 한도+1바이트까지만; 읽기는 렌더 허가를 얻은 뒤 렌더 스레드에서 하므로 허가를 못 얻은 요청은 아무것도 읽지 않음), 렌더는 전용 스레드(스택 32MiB)에서 10초 마감과 `catch_unwind`, 쪽 SVG 16MiB(PD-132 한도), PNG 긴 변 480px·1MiB, 같은 시각에 사는 렌더 스레드 2개(실행 중이든 마감을 넘겨 남은 것이든 같은 허가 하나를 끝까지 쥠; 허가를 2초 기다려도 못 얻으면 포기하고 캐시에 남기지 않음). (4) **캐시**: 기존 id별 메모리 캐시(최대 10개, 파일의 수정 시각·크기가 같을 때만 유효)에 그려진 PNG와 실패(마감 초과·깨진 파일 포함)도 기억해 같은 파일을 다시 그리지 않습니다. 허가를 얻지 못한 요청만 기억하지 않습니다. (5) **의존성**: 셸이 고정된 `third_party/rhwp`를 `default-features = false`(Skia 내려받기 없음)로 의존합니다. `path` 의존이면 `cargo fmt --all`이 스파스 체크아웃 안의 없는 테스트 파일을 따라가 실패하므로, 루트 `[patch.crates-io] rhwp = { path = "third_party/rhwp" }`와 `version = "0.8.7"`로 선언합니다(crates.io에는 다른 버전 0.9.0만 있음). 셸의 `rust-version`은 1.90에서 1.93으로 올립니다(rhwp 고정 툴체인). 잠금 파일에 93개 패키지가 늘며 모두 허용 라이선스입니다(`THIRD_PARTY_NOTICES.md`). `ci.yml`의 `checks` 작업은 Cargo가 매니페스트를 읽도록 `scripts/checkout-rhwp.mjs`를 먼저 실행합니다(컴파일은 안 함). 다른 데스크톱 작업과 릴리스는 이미 체크아웃합니다.
- **Why** 참조 앱과 같은 카드 모습이 목표이고(QA 요청), 임의 파일을 파서·레이아웃에 넣으므로 마감·크기·동시 작업 상한이 없으면 파일 하나가 시작 창을 멈추거나 메모리를 소진시킬 수 있다. 참조 앱에는 그 상한이 없어 그대로 따르지 않았다.
- **재사용 판단** (1) 허용 라이선스의 유지되는 크레이트: rhwp 자체(MIT, 고정 체크아웃)와 `usvg`/`resvg`를 그대로 씀. (2) 이미 쓰는 의존성: 쪽 SVG를 PNG로 그리는 `save_thumbnail::render_png`를 크기 인수만 더해 재사용하고 글꼴 DB(`shared_font_db`)도 PDF 내보내기와 공유함. (3) Quick Look의 `bounded.rs`(스레드·마감·패닉 차단)는 그 작업공간이 분리돼 있고 `status.rs`의 C 상태 코드에 묶여 있어 그대로 포함하지 못하고, 같은 구조를 허가(semaphore) 방식으로 바꿔 `preview/bounded.rs`로 새로 썼다(Quick Look은 마감을 넘긴 작업자 수만 세고, 여기서는 살아 있는 렌더 스레드 전부를 센다).
- **측정(릴리스 빌드, 이 Mac, 카드 10개를 차례로)** 그림을 지운 rhwp 샘플 10개(HWP 9, HWPX 1): 처음(글꼴 DB 만들기 포함) 합계 559ms, 개별 7~183ms(PNG 9~145KB). 같은 목록을 다시 요청하면 0ms(캐시). 레이아웃이 느린 샘플(`issue1949_giant_cell_nested_tables_perf.hwp`)이 692ms라 마감 10초는 넉넉하다. 디버그 빌드는 4.3초. 파일 안 그림을 읽는 경로는 그대로 몇 ms.
- **Trade-offs** 셸 컴파일 시간·바이너리가 rhwp 네이티브만큼 늘고(`cargo test -p easyoffice-desktop` 처음부터 약 1분 → 릴리스 2분), `checks` 작업에 스파스 체크아웃 한 단계가 더해진다. 마감을 넘긴 렌더는 취소되지 않고 끝나거나 프로세스가 끝날 때까지 허가를 쥐므로, 적대적 파일 두 개가 겹치면 이후 렌더 요청은 2초 뒤 포기하고 빈 카드가 된다(캐시에 남지 않아 나중에 다시 시도됨). 그려진 페이지는 문서 내용의 일부이므로 화면과 메모리 캐시에만 있고 디스크에 쓰지 않는다. 암호가 걸린 문서와 렌더가 실패하는 문서는 빈 카드다. 카드 그림은 480px라 고해상도 화면에서 아주 약간 흐릴 수 있다.
- **Alternatives rejected** SVG를 페이지에 그대로 넣기(참조 앱 방식: 페이지 CSP·스크립트 실행 검토 필요, 안전한 PNG가 같은 모습), 페이지에서 한글 WASM으로 그리기(문서 바이트가 페이지로 감, PD-106이 기각), Quick Look 크레이트를 의존성으로 쓰기(별도 작업공간·C ABI·macOS 전용), `path` 의존 + `cargo fmt --all` 명령 변경(문서화된 검증 명령을 바꿈), 별도 프로세스로 렌더(번들·서명·플랫폼별 부속 실행 파일이 규모에 비해 큼; 마감을 넘기면 강제 종료할 수 있다는 장점은 인정), 글꼴·PNG 크기 상한을 더 크게.
- **Verified by** `cargo test -p easyoffice-desktop preview`(그림을 지운 HWP·HWPX 샘플이 PNG로 그려짐·긴 변 480px·흰 종이가 아님, 그림이 있으면 그리지 않음, 비트를 깨거나 잘리거나 빈·가짜 파일은 빈 카드와 패닉 없음, HWP·HWPX 외 확장자는 그리지 않음, 50MiB 초과는 렌더러에 닿지 않고 한도 정확히는 읽어 넘김, 1MiB 초과·PNG가 아닌 결과 버림, 마감 초과 시 호출자가 기다리지 않고 허가를 끝까지 쥠·허가가 없으면 캐시하지 않음·나중에 다시 그림, 패닉 포착과 허가 반환, 동시에 8개를 요청해도 렌더 2개를 넘지 않음, 같은 파일은 한 번만 그리고 바뀌면 다시 그림, 실패도 기억, 캐시 10개; 허가를 일찍 놓거나 busy를 캐시하거나 캐시 조회를 빼거나 PNG 검사를 빼면 해당 테스트가 실패하는 것을 확인), `cargo test -p easyoffice-desktop --release measure_ten_cards -- --ignored --nocapture`(측정), `node --test scripts/test-webui-home-preview.mjs`(1MiB 한도 경계, PNG 응답 표시).
- **Revisit when** 다른 형식(DOCX·XLSX 등)의 첫 쪽 미리보기가 필요할 때, 실기기에서 렌더가 시작 창을 늦추거나 마감을 자주 넘길 때(별도 프로세스 렌더 재검토), rhwp 고정 버전을 올릴 때(`[patch]`의 `version`도 함께).

## PD-143. 정식 릴리스 전까지 macOS 서명·공증 보류, 배포 채널은 GitHub 릴리스만

- **Status** Accepted (QA 결정 2026-11-09). PD-094의 서명 없는 배포를 재확인합니다. 코드 변경은 없습니다.
- **Context** 참조 앱은 macOS 앱에 Developer ID 서명과 공증을 하고, 외부 패키지 관리자 채널(macOS용 하나, Linux 배포판 사용자 저장소 하나)로도 배포합니다. 이 프로젝트는 개인 프로젝트이고 유료 개발자 계정이 없습니다.
- **Decision (`PD-143`)** (1) 정식(안정) 릴리스 전까지 Developer ID 서명·공증을 하지 않습니다. ad-hoc 서명(PD-094)과 README의 Gatekeeper 안내를 유지합니다. (2) 배포 채널은 GitHub 릴리스뿐입니다. 외부 패키지 관리자 채널은 만들지 않습니다.
- **Trade-offs** 참조 앱과 달리 처음 실행할 때 Gatekeeper 확인이 필요하고, 서명된 앱을 기대하는 사용자에게는 설치 마찰이 있습니다. 패키지 관리자로 설치·갱신하는 사용자는 지원하지 않습니다.
- **Revisit when** 정식 릴리스를 준비할 때(서명·공증 도입 여부를 다시 정함), 개발자 계정을 확보할 때, 패키지 관리자 채널 요청이 반복될 때.

## PD-144. 셸이 선언하는 rhwp 버전은 고정 커밋의 버전과 같아야 하고 갱신 스크립트가 함께 맞춤

- **Status** Implemented (QA 요청 2026-11-09, PD-142 리뷰 메모). PD-142 (5)를 보강합니다.
- **Context** PD-142로 셸 `crates/easyoffice-desktop/Cargo.toml`이 `rhwp = { version = "0.8.7" }`를 선언하고 루트 `[patch.crates-io] rhwp = { path = "third_party/rhwp" }`가 체크아웃을 가리킵니다. rhwp 고정을 올리면서 셸의 `version`을 그대로 두면 `[patch]`가 쓰이지 않아 `cargo --locked`가 실패합니다.
- **Decision (`PD-144`)** (1) `scripts/verify-rhwp-upstream.mjs`(위생 가드와 `build-hwp-studio`가 부름)가 체크아웃 유무와 관계없이 `config/rhwp-upstream.json`의 `version`과 셸이 선언한 rhwp 버전의 일치, 루트 `[patch.crates-io]`의 rhwp 경로가 `third_party/rhwp`인지 검사하고, 어긋나면 실패합니다. (2) `scripts/update-rhwp.mjs`는 고정을 옮길 때 셸 `Cargo.toml`의 rhwp 버전과 `Cargo.lock`의 `rhwp` 패키지 버전도 계획에 넣어 같은 규칙(쓰기는 끝에서, 실패하면 복원)으로 갱신합니다. 두 파일에 해당 줄이 없으면 갱신을 거부합니다. 고정은 같은데 셸 선언만 낡은 경우도 "이미 고정됨"으로 건너뛰지 않고 고칩니다. 다음 단계 안내에 `cargo check -p easyoffice-desktop`(잠금 파일의 다른 변화 확인)을 더했습니다.
- **Why** 어긋남을 갱신 뒤 CI에서 처음 발견하는 대신 갱신 시점과 위생 가드에서 잡습니다.
- **Trade-offs** `Cargo.lock`의 rhwp 의존성 목록이 새 버전에서 달라지면 스크립트는 그 줄을 고치지 못하므로 `cargo check`로 마무리해야 합니다. 잠금 파일 버전은 가드가 아니라 `cargo --locked`가 검사합니다.
- **Verified by** `node --test scripts/test-rhwp-upstream.mjs scripts/test-update-rhwp.mjs`(실제 저장소의 세 선언 일치, 어긋난 셸 버전·없는 선언·다른 patch 경로 거부, 갱신이 셸·잠금 파일의 `rhwp`만 고침, 줄이 없으면 거부하고 아무것도 바꾸지 않음, 낡은 선언 복구).
- **Revisit when** 셸이 rhwp를 다른 방식(예: git 의존)으로 선언하게 될 때.

## PD-145. 썸네일·카드의 쪽 안 래스터 그림은 그리되 픽셀 상한을 헤더로 먼저 검사

- **Status** Implemented (QA 요청 2026-11-09). PD-132의 "래스터 사진은 흰 칸" 전제를 바로잡고 PD-142의 카드 렌더 입력 상한을 더합니다.
- **Context** PD-132는 `resvg`의 `raster-images`가 꺼져 있어 쪽 안 사진이 흰 칸이라고 적었습니다. 실제로는 rhwp가 `svg2pdf`를 기본 기능으로 쓰고 그 `filters` 기능이 `resvg/raster-images`를 켜서, PD-142로 rhwp가 들어온 뒤 빌드 전체에서 이미 켜져 있었습니다(`cargo tree -e features -i resvg`; 시험이 기능 선언 없이도 통과함). 그래서 쪽 SVG는 신뢰할 수 없는 입력인데 그림이 상한 없이 디코드되고 있었습니다.
- **재사용 판단** (1) 유지되는 크레이트: `resvg`의 디코더(`gif` 0.13, `image-webp` 0.2, `zune-jpeg` 0.4, PNG는 `tiny-skia`)를 그대로 씀. 새 크레이트는 없고 직접 의존성으로 `imagesize`(usvg가 쓰는 같은 0.13)만 이름을 올림. 라이선스는 모두 MIT/Apache-2.0(+Zlib/Unlicense 병기), GPL 계열 없음. (2) 이미 쓰는 의존성: 위가 모두 이미 그래프에 있어 바이너리 크기 증가는 없음. (3) 이식한 코드 없음.
- **Decision (`PD-145`)** (1) `resvg`의 `raster-images`를 데스크톱 `Cargo.toml`에 명시합니다(우연히 켜진 상태에 기대지 않음). (2) 비트맵으로 그리는 쪽(`save_thumbnail::render_png_sized`: 파일 미리보기 그림과 시작 창 카드)은 usvg의 `image_href_resolver`에서 디코드 전에 `raster_budget::RasterBudget::admit`으로 헤더 크기를 검사합니다: 그림 하나 4096x4096 픽셀 이하, 한 쪽 합계 4배(약 6700만 픽셀) 이하이고, 헤더를 읽을 수 없거나 한도를 넘으면 그 그림만 빼고 쪽을 그립니다(Fail-Closed). 합계는 쪽마다 새 예산이며 중첩 SVG가 같은 예산을 씁니다. (3) PDF 내보내기와 Quick Look은 개정(아래 Follow-up)에서 같은 모듈로 상한을 겁니다.
- **Why** 쪽 SVG가 신뢰할 수 없는 입력이므로 디코더가 헤더 크기로 할당하기 전에 거부해야 합니다. 4096x4096은 RGBA 64MiB로 사진 한 장에 충분합니다. GIF는 프레임 크기가 화면 크기와 다를 수 있어 헤더 검사만으로 막을 수 없지만 `gif` 크레이트의 기본 메모리 한도(50MB)가 프레임 할당을 거부합니다(시험으로 확인).
- **Trade-offs** 4096x4096을 넘는 사진(약 1670만 화소 초과, 큰 카메라 원본)은 썸네일·카드에서 빠집니다. 사진이 든 쪽은 디코드 시간이 더해집니다(아래 측정). 사진이 많은 쪽의 PNG가 미리보기 그림 한도(128KiB)를 넘으면 `embed_thumbnail` 단계가 거부되어 저장은 계속되지만 파일 안 그림은 바뀌지 않습니다(통째로 사진인 1701x2552 JPEG 쪽: 195KB).
- **Alternatives rejected** `raster-images`를 끄고 흰 칸 유지(이미 켜진 기능을 끄려면 rhwp의 `svg2pdf` 기능을 건드려야 하고, 사진이 든 문서의 카드가 비어 보임), 디코드 뒤 크기 검사(할당이 이미 일어남), 썸네일과 같은 4096x4096을 PDF 내보내기에도 적용(내보내는 PDF에서 큰 사진이 사라지는 퇴행: 높은 상한을 따로 둠, Follow-up).
- **Verified by** `cargo test -p easyoffice-desktop raster`(PNG·JPEG가 흰 칸이 아님, 5000x5000을 선언한 헤더만 있는 PNG는 빠지고 나머지 쪽은 그려짐, 화면 1x1에 프레임 65535x65535인 GIF와 깨진 JPEG·WebP는 패닉·실패 없이 넘어감, 4096x1024 PNG 17장 중 16번째까지 그리고 17번째는 합계 초과로 빠짐, 예산 단위 시험). 구현 전 RED는 예산 모듈이 없어 시험이 컴파일되지 않는 것으로 확인.
- **측정(릴리스 빌드, 이 Mac)** 사진이 든 rhwp 샘플 첫 쪽의 PNG 만들기: 7ms~326ms(SVG 3.6MB인 쪽 326ms), 통째로 1701x2552 JPEG인 쪽 49ms, 4096x4096 PNG 한 장 33ms.
- **Follow-up (QA 요청 2026-11-09)** 같은 `raster_budget` 모듈을 매개변수만 달리해 두 길에 더했습니다. (1) **PDF 내보내기**: 그림 하나 64,000,000픽셀 이하(8192x6144 5천만 화소와 9504x6336 6천만 화소 풀프레임은 통과, 16비트 RGBA로 512MB라 `image` 크레이트가 모든 디코드에 거는 할당 한도 512MiB 안에서 항상 디코드됨), 한 쪽 합계 4배. 헤더(PNG·GIF·JPEG·WebP)로 디코드 전에 검사하고 넘는 그림은 그 그림만 빼고 쪽을 내보냅니다. 쪽마다 새 예산이고(쪽을 나란히 변환하므로 한 쪽의 사진이 다른 쪽 몫을 쓰지 않음; 글꼴 대체 색인은 내보내기 전체가 공유) 중첩 SVG는 같은 쪽 예산을 씁니다. 뺀 그림 수는 `pdf_export_finish` 답의 `skippedPictures`로 편집기에 전하고 로그 문구는 따로 없습니다. (2) **Quick Look**: 쪽 SVG를 읽는 `parse_svg`가 썸네일과 같은 4096x4096·합계 4배를 겁니다. 작업공간이 분리돼 있어 새 크레이트 대신 `pdf_font_fallbacks.rs`와 같은 방식으로 `raster_budget.rs`를 경로로 포함합니다(`apps/quicklook/rust/src/raster.rs`; 중복 없음, 루트 작업공간 멤버·`[patch]` 변경 없음, 직접 의존성 `imagesize` 0.13 하나만 더함). 미리보기 PDF도 같은 상한이라 4096x4096을 넘는 사진은 Finder 미리보기에서 빠집니다. (3) `svg2pdf`는 그림을 `image` 크레이트로 전체 디코드한 뒤(JPEG는 원본 바이트를 DCT로 재사용, 나머지는 RGB와 알파로 나눠 Flate로 다시 압축) 넣습니다. `Limits`는 `svg2pdf`가 `load_from_memory_with_format`을 불러 기본값(할당 512MiB)으로 고정이고, 바꾸려면 rhwp와 맞추는 `vendor/svg2pdf`를 고쳐야 해 하지 않았습니다. 헤더 검사가 그 앞 방어선입니다. 알려진 한계: 헤더는 정상인데 내용이 잘린 그림은 `svg2pdf`가 `InvalidImage`로 거부해 내보내기가 그 쪽 오류로 끝납니다(상한 도입 전과 같음).
- **측정(PDF 내보내기)** 릴리스 빌드, 이 Mac, 단색 PNG 한 장을 담은 한 쪽을 내보내는 시간: 6000x4000(2400만 화소) 132ms, 8000x8000(6400만 화소, 상한) 349ms. 헤더 검사는 그림 하나에 마이크로초 단위라 측정되지 않음. 상한 크기 8비트 RGBA는 디코드 256MB입니다(내보내기는 쪽을 최대 4개 나란히 변환하므로 적대적 입력의 최악은 그 몇 배).
- **Verified by (Follow-up)** `cargo test -p easyoffice-desktop raster`(PNG·GIF·JPEG 선언 크기 폭탄과 한도 초과 거부, 6000x4000·8192x6144·9504x6336 통과, 폭탄은 빼고 나머지 그림과 쪽을 내보냄, 중첩 SVG가 쪽 예산을 같이 씀, 쪽마다 예산, 같은 입력의 PDF 바이트 동일), `node scripts/run-quicklook-tests.mjs`(헤더 폭탄을 빼고 쪽이 그려지고 5000x4000은 빠짐).
- **Revisit when** 큰 사진이 카드에서 빠져 문제가 될 때(상한 조정), 썸네일 PNG 한도 초과로 파일 안 그림이 안 바뀌는 문서가 잦을 때(손실 압축 또는 크기 축소).

## PD-146. 릴리스 다시 실행: 드래프트는 대상 커밋도 옮기고, 공개된 릴리스는 같은 커밋에서만 자산을 바꿈

- **Status** Implemented (`release.yml`의 `publish-release`; 실제 GitHub 실행은 다음 릴리스 시험 실행에서 확인)
- **Context** 같은 태그의 릴리스가 있으면 자산만 `--clobber`로 바꾸고 대상 커밋은 그대로여서, 드래프트(태그 아직 없음)를 새 main 커밋에서 다시 빌드하면 자산은 새 커밋, 릴리스 대상은 옛 커밋으로 어긋났습니다. 공개하면 태그가 자산과 다른 소스를 가리킵니다. (PD-103 (13)의 workflow 규칙 보완)
- **Decision (`PD-146`)** (1) 기존 릴리스가 드래프트이면 `gh release edit --target <이번 커밋>`으로 대상 커밋을 옮기고, 이전 실행에서 올라갔지만 이번 자산 목록에 없는 파일은 지운 뒤 자산을 올립니다. (2) 이미 공개된 릴리스(태그 존재)는 대상 커밋을 바꾸지 않습니다. 태그가 가리키는 커밋이 이번 실행 커밋과 다르면 오류로 끝내고, 같을 때만 자산을 교체합니다. 공개된 버전을 다른 소스로 덮어쓰지 않습니다.
- **Why** 공개된 태그·자산·소스가 늘 같은 커밋이어야 서명 확인과 재현이 의미가 있습니다.
- **Trade-offs** 공개된 버전을 고쳐 다시 내고 싶으면 새 버전(태그)으로 내야 합니다.
- **Alternatives rejected** 공개된 릴리스의 태그를 옮기는 것(이미 받아 간 사용자와 어긋남), 드래프트 대상 커밋을 그대로 두는 것(현재 결함).
- **Verified by** `node --test scripts/test-release-assets.mjs`(드래프트는 대상 갱신 뒤 남은 파일 삭제, 공개 릴리스는 태그 커밋 비교 뒤에만 업로드, 불일치 시 실패).
- **Revisit when** 공개 릴리스의 자산만 고쳐야 하는 일이 생길 때.

## PD-147. CI와 릴리스의 Rust 툴체인을 정확한 버전으로 고정

- **Status** Implemented (`.github/rust-toolchain-version`, `.github/actions/setup-rust`; 실제 러너 실행은 첫 CI에서 확인)
- **Context** 러너의 stable이 새 deprecation 경고를 내면서 `-D warnings` clippy 게이트가 코드 변경 없이 깨졌습니다(래스터 예산의 `fetch_update`). 게이트의 결과가 러너가 그날 쓰는 컴파일러에 달려 있었습니다.
- **Decision (`PD-147`)** CI(`ci.yml`의 모든 Rust 잡)와 릴리스(`release.yml`, 공용 액션 `prepare-desktop-build`)와 Quick Look 빌드가 `1.93.1`을 씁니다. 버전은 `.github/rust-toolchain-version` 한 파일에만 적고, 공용 액션 `setup-rust`가 그 파일을 읽어 `rustup toolchain install <버전> --profile minimal --component clippy,rustfmt`, `rustup default`, `RUSTUP_TOOLCHAIN` 내보내기를 합니다(macOS 범용 빌드의 두 타깃도 같은 액션이 추가). 러스트 캐시 키에 버전을 넣습니다. 리포 루트에 `rust-toolchain.toml`은 두지 않아 로컬 개발과 `rhwp` 서브모듈의 툴체인 선택은 그대로입니다. 서드파티 액션 없이 러너에 있는 `rustup`만 씁니다.
- **Why** 데스크톱·Quick Look `rust-version`이 1.93이고 `rhwp`가 1.93.1을 고정하므로 같은 버전이면 MSRV보다 새 API를 쓰는 실수를 CI가 잡습니다.
- **Trade-offs** 새 컴파일러의 lint는 올리기 전까지 보이지 않습니다. 올리는 일이 사람의 몫입니다.
- **올리는 주기·방법** 월 1회 또는 `rhwp`를 갱신할 때. `.github/rust-toolchain-version` 한 줄을 바꾸고, 같은 버전으로 `cargo clippy`·`cargo test` 전체를 돌린 뒤 한 커밋으로 올립니다. 새 버전의 경고가 많으면 코드 수정과 같은 커밋에 넣습니다. 올릴 때 이 항목의 버전도 갱신합니다.
- **Alternatives rejected** 루트 `rust-toolchain.toml`(로컬과 `rhwp` 빌드에 영향), 러너 stable 그대로 두고 경고만 대응(같은 문제 재발), 서드파티 툴체인 액션(핀할 SHA를 하나 더 관리).
- **Verified by** `node --test scripts/test-rust-toolchain-pin.mjs`(버전 원천 하나, 모든 Rust 잡이 cargo 전에 `setup-rust`, 개별 `rustup` 호출 없음, 캐시 키에 버전).
- **Revisit when** 1.93.1에서 기존 코드가 빌드되지 않거나 의존성이 더 새로운 MSRV를 요구할 때(그 시점의 stable 숫자로 올림).

## PD-148. 다른 이름으로 저장은 어느 모듈에서나 저장 대화상자를 거친다

- **Status** Implemented (`workspace-tools.js`의 `saveActiveModuleDocument({ saveAs })`, 메뉴 명령 `studio-save-as`, 단축키 `file-save-as`)
- **Context** 한글 편집기 밖의 모듈(문서·시트·슬라이드·PDF)에서 "다른 이름으로 저장"(메뉴 항목, Ctrl+Shift+S/⌘⇧S, macOS 메뉴 막대)이 일반 저장과 같은 `studio-save`를 불러 열린 파일이 있으면 대화상자 없이 원본을 덮어썼습니다.
- **Decision (`PD-148`)** 다른 이름으로 저장은 항상 셸 저장 대화상자로 보냅니다(`save_document_file`의 `inPlace`를 보내지 않음). 일반 저장(`studio-save`, Ctrl+S)만 열린 파일에 되돌려 씁니다. 메뉴 항목은 `studio-save-as`, 단축키와 네이티브 메뉴는 한글 편집기가 활성이면 편집기의 `saveAs("hwp")`, 아니면 `saveAs: true` 저장을 부릅니다. 저장에 성공하면 열린 파일이 새 경로로 바뀌는 것은 기존 셸 동작 그대로입니다.
- **Why** 이름을 바꿔 저장하려는 사용자가 원본을 잃는 것이 가장 큰 데이터 손실이고, 한글 편집기는 이미 같은 규칙이었습니다.
- **Alternatives rejected** 일반 저장과 같은 경로에서 파일이 바뀌었는지 묻기(저장 대화상자를 건너뛴다는 결함은 그대로).
- **Verified by** `node --test scripts/test-webui-desktop-bridge.mjs`(DOC·SHEET·SLIDE·PDF에서 메뉴 항목과 네이티브 메뉴 `file:save-as`는 `inPlace: false`, 일반 저장만 `true`). 구현 전 RED는 같은 시험에서 `inPlace: true`로 확인.
- **Revisit when** 비-HWP 모듈이 "복사본으로 저장"처럼 열린 파일을 유지하는 저장을 따로 두어야 할 때.

## PD-149. 다중 시트 XLS는 모든 시트를 계산값으로, CSV·TSV는 첫 시트만 경고와 함께, 범위는 읽는 쪽 한도에 맞춤

- **Status** Implemented (`convert_structured.rs`의 `encode_xls`·`encode_delimited`·`computed_sheets`, `converter.rs`의 `cells_to_grid`, `CsvCodec::MAX_GRID_CELLS`)
- **Context** 구조화 저장(`sheets`)에서 XLS·CSV·TSV는 `sheets.first()`만 모델에 담아 둘째 시트부터 알림 없이 사라졌고, 수식은 계산하지 않아 XLS에는 값 0인 수식 레코드가, CSV에는 `=SUM(…)` 글자가 들어갔습니다. CSV·TSV 격자는 512행 x 64열에서 조용히 잘렸습니다.
- **Decision (`PD-149`)**
  1. **XLS**: 모든 시트를 `LegacyOle2Codec::pack_xls_multi`로 시트 이름 그대로 저장합니다. 수식 셀은 `WorkbookEngine`(모든 시트를 함께 올려 시트 간 참조까지)이 계산한 표시값으로 바꿔 숫자는 NUMBER, 글자는 LABELSST 셀로 씁니다. 수식 식 자체는 쓰지 않습니다. 쓸 수 있는 올바른 BIFF8 수식 토큰 열(RPN) 인코더가 없고, 식을 글자로 넣은 FORMULA 레코드는 외부 프로그램이 읽지 못하거나 캐시 값 0으로 보일 수 있어 값 셀이 더 안전하다고 판단했습니다(외부 프로그램으로는 열어 보지 않았음). 바꾼 수식이 있으면 응답 `warnings`에 개수를 알립니다.
  2. **CSV·TSV**: 단일 시트 형식이라 첫 시트만 계산값으로 저장하고, 나머지가 있으면 `warnings`에 첫 시트 이름·빠진 시트 수와 이름(5개까지)을 알립니다.
  3. **범위**: CSV·TSV를 다음에 여는 엔진(`CsvCodec::MAX_ROWS` 1,000,000행 x `MAX_COLS` 16,384열) 안의 셀만 쓰고, 벗어난 셀은 개수를 `warnings`에 알립니다. 격자는 셀 실제 범위로 만들고 행 x 열이 `CsvCodec::MAX_GRID_CELLS`(2,000,000, 문서 형식용 값 표 한도와 같은 값)를 넘으면 잘라 쓰지 않고 `ResourceBudgetExceeded`로 거부합니다(Fail-Closed). 모델 경로(`UniversalFormatConverter::encode`)에서 읽는 쪽 한도를 넘는 시트도 같은 오류입니다.
- **Why** 시트가 말없이 사라지거나 수식이 글자로 남는 것은 데이터 손실입니다. 저장 검증은 "다음에 여는 엔진이 받아들이는가"가 기준이라 한도를 CSV 읽기 한도에 맞췄습니다.
- **Trade-offs** XLS를 다시 열어도 수식은 없고 값만 남습니다. 숫자는 일반 서식의 15자리 표시값으로 저장됩니다. 수식이 만든 글자가 `=`로 시작하면 XLS 코덱이 수식으로 다시 해석합니다(알려진 한계). 열 너비·셀 서식은 XLS·CSV에서 계속 빠집니다.
- **Alternatives rejected** BIFF8 수식 인코더를 새로 쓰기(범위가 크고 검증 수단이 없음), 512x64 상한 유지(조용한 절단), 읽는 쪽 한도 초과를 경고만 하고 거대한 격자를 만들기(메모리 폭탄).
- **Verified by** `cargo test -p easyoffice-app --test structured_sheet_export`(XLS 세 시트와 시트 간 참조 값, CSV·TSV 첫 시트 값과 빠진 시트 경고, 700행·BO열 보존, 읽는 쪽 한도 밖 셀 제외 경고, 희소하고 넓은 시트 거부). 구현 전 RED는 같은 시험에서 시트 하나(`예산`)만 저장되는 것으로 확인.
- **Revisit when** BIFF8 수식 RPN 인코더가 생길 때(수식 유지), XLS 저장에 셀 서식이 필요할 때.

## PD-150. IPC로 보내는 파일의 크기 한도는 엔진과 화면이 같은 10 MiB

- **Status** Implemented (`convert_open.rs`의 `MAX_SOURCE_BYTES`, `file-open.js`의 `MAX_FILE_BYTES`·`converterSourceProblem`)
- **Context** 화면은 IPC로 보내는 파일을 10 MiB, 한글 편집기 경로를 64 MiB로 받고 엔진은 12 MiB까지 디코드했습니다(PD-075의 "화면 10 / 서버 12"). 그래서 (1) 변환기 원본으로 12~64 MiB 한글 파일을 고르면 화면은 받고 엔진이 `PayloadTooLarge`로 거부했고, (2) 편집기가 열지 못한 10~64 MiB 한글 파일의 텍스트 대체 경로는 한글 경로 플래그 없이 10 MiB로 거부해 왜 거부되는지 알 수 없었고, (3) 편집기에서 내보낸 한글 바이트를 변환기에 보낼 때는 크기를 보지 않았습니다. 12 MiB의 Base64(16 MiB)는 HTTP IPC 본문 한도(16 MiB)에도 맞지 않았습니다.
- **Decision (`PD-150`)**
  1. IPC로 가는 파일(엔진 디코드, 변환기 원본, 편집기에서 내보낸 변환기 원본)의 한도는 한 값, 10 MiB입니다. Rust `MAX_SOURCE_BYTES`(12 -> 10 MiB)와 JS `MAX_FILE_BYTES`가 같고 `scripts/test-file-size-limits.mjs`가 두 소스에서 값을 읽어 같음을 강제합니다. Rust 단위 시험은 10 MiB의 Base64에 다른 필드 여유 64 KiB를 더해도 `MAX_IPC_HTTP_BODY_BYTES`(16 MiB) 안임을 확인합니다. 근거는 Base64가 4/3로 커져 13.3 MiB이고 본문 한도 16 MiB 아래입니다.
  2. 한글 편집기 경로의 64 MiB는 IPC를 거치지 않는 값이라 그대로 두고, `file-open.js`와 `hwp-session.js`의 값이 같고 셸의 파일 열기 한도(`MAX_OPEN_BYTES` 256 MiB) 이하임을 같은 시험이 강제합니다. (셸의 한도는 PD-177 3에서 종류별 값으로 바뀌었습니다.)
  3. 변환기 원본은 종류와 관계없이 10 MiB입니다(한글 파일 포함). 초과는 보내기 전에 "파일이 너무 큽니다 (N MiB). 변환기 원본은 최대 10 MiB까지 선택할 수 있습니다"로 알립니다. 변환기를 누를 때 편집기에서 내보낸 한글 바이트가 넘으면 변환하지 않고 같은 문구를 변환기 결과 칸에 보입니다. 편집기가 열지 못한 한글 파일의 텍스트 대체 경로는 "한글 편집기를 쓰지 못하는 경우 텍스트로 불러올 수 있는 한도는 10 MiB입니다"로 이유를 밝힙니다.
- **Why** 화면이 받은 파일을 엔진이 거부하거나 이유를 알 수 없게 거부하지 않고, 한도가 한 곳 값이라 한쪽만 바뀌는 어긋남을 시험이 막습니다.
- **Trade-offs** 엔진의 직접 호출 한도가 12에서 10 MiB로 줄었습니다(사용자 경로는 이미 10 MiB였음). 한글 파일을 변환기에 쓰려면 10 MiB 이하여야 합니다.
- **Alternatives rejected** 화면을 12 MiB로 올리기(HTTP 본문 한도 초과), IPC 본문 한도 상향 또는 분할 전송(PD-075에서 기각), 셸이 부팅 때 한도를 알려 주는 구조(새 IPC 필드와 모든 시험 하니스 변경에 비해 값 두 개를 시험으로 묶는 쪽이 작음).
- **Verified by** `node --test scripts/test-file-size-limits.mjs scripts/test-webui-file-open.mjs scripts/test-webui-hwp-session.mjs`, `cargo test -p easyoffice-app --lib source_`. 구현 전 RED: 크기 시험이 엔진 12 MiB와 화면 10 MiB의 불일치로 실패, 변환기 원본 20 MiB 한글 파일이 받아들여짐.
- **Revisit when** IPC 본문 한도를 올릴 때, 또는 큰 한글 파일을 변환기에 써야 한다는 요구가 있을 때(스트리밍 전송).

## PD-151. 한글 편집기의 오려 두기는 시스템 클립보드가 받은 뒤에만 지우고, 편집기 프레임에 `clipboard-write`를 위임

- **Status** Implemented (`apps/hwp-studio/src/command/safe-cut.ts`·`clipboard-cut.ts`, `hwp-host.js`의 `FRAME_ALLOW`; macOS 메뉴·실제 WebKit 동작은 사람이 확인해야 함)
- **Context** macOS 메뉴의 ⌘X는 PD-119에 따라 편집기의 `edit:cut`(`automation.execute`)로 갑니다. 업스트림 `performCut`은 텍스트 선택에서 `document.execCommand('cut')`을 부르는데, 메뉴가 보낸 명령에는 사용자 제스처가 없고 편집기 프레임은 불투명 출처라 시스템 클립보드 쓰기가 막혀도 `cut` 이벤트의 삭제가 일어날 수 있었습니다. 참조 앱은 클립보드 쓰기 성공 뒤에만 지웁니다(`edit.ts` 120-150행).
- **Decision (`PD-151`)**
  1. 편집기 오버레이가 `edit:cut`을 안전한 명령으로 바꿉니다(`main.ts`가 등록 직후 덮어씀. 메뉴, Ctrl+X, 도구줄, `automation.execute`가 모두 이 한 명령을 지남). 텍스트 선택(본문·셀·중첩 표)은 내부 클립보드에 복사하고 텍스트와 HTML을 비동기 클립보드 API로 쓴 뒤, 쓰기가 받아들여졌고 선택이 그대로일 때만 `performDelete`합니다. 거부되면 아무것도 지우지 않고 콘솔에 남깁니다. 개체(그림·표) 선택, 머리말·꼬리말, 각주, 셀 블록, 양식 모드의 오려 두기는 편집기 기존 명령 그대로입니다.
  2. 편집기 iframe에 `allow="clipboard-write"`를 더합니다(`sandbox="allow-scripts allow-modals"`는 그대로). 이것은 Permissions Policy 위임이지 sandbox 플래그가 아닙니다. 프레임의 출처(불투명), 부모 DOM·전역 접근 차단, Tauri IPC 거부, 팝업·폼·다운로드·내비게이션 금지, CSP(`connect-src 'self'`)에 영향이 없고, `clipboard-read`는 위임하지 않아 프레임이 클립보드 내용을 읽지 못합니다. 쓰기도 브라우저의 사용자 활성화 규칙을 따릅니다. 격리 점검용 프레임(`runFrameProbe`)에도 같은 `allow`를 붙여, 위임이 IPC를 열지 않음을 `frame_ipc` 스모크가 계속 확인합니다.
- **Why** 클립보드에 못 쓴 내용을 지우면 붙여넣을 곳이 없습니다. 편집기가 이미 `copy` 이벤트로 같은 클립보드에 쓸 수 있으므로 쓰기 위임이 새 능력을 주지 않고, 읽기와 IPC 경계는 그대로입니다.
- **Trade-offs** 사용자 제스처가 없는 메뉴 경로에서 WebKit이 비동기 클립보드 쓰기를 거부하면 ⌘X는 아무것도 하지 않습니다(데이터는 안전). 이 경우의 안내 문구는 아직 없습니다. 개체 오려 두기는 안전 경로에 포함되지 않습니다(실행 취소로 되돌릴 수 있음).
- **Alternatives rejected** 업스트림 `input-handler.ts` 포크(5천 줄 이상, 업스트림 갱신 부담), 쓰기 실패를 클립보드 내용 되읽기로 확인(읽기 권한 필요), 셸의 클립보드 명령으로 쓰기(새 IPC 면).
- **Verified by** `node --test scripts/test-hwp-clipboard-cut.mjs scripts/test-webui-hwp-host.mjs`(쓰기 뒤에만 삭제, 거부·동기 예외·선택 변경 시 삭제 없음, 셀·중첩 셀 복사 주소, 안전 경로 밖 컨텍스트는 기존 명령 유지, 프레임 `allow`), `node scripts/build-hwp-studio.mjs --skip-install`(번들에 포함됨). 실제 macOS 메뉴 ⌘X와 WebKit의 쓰기 허용 여부는 이 환경에서 GUI를 실행하지 못해 확인하지 못했습니다.
- **Revisit when** macOS에서 메뉴 ⌘X가 쓰기 거부로 동작하지 않는다고 확인될 때(셸이 클립보드를 쓰는 길을 검토), 개체 오려 두기도 같은 규칙이 필요할 때.

## PD-152. 시트·ODF·RTF·HWPX 코덱의 수식 캐시·목록 종류·CP949·표 수식 정합성 보강

- **Status** Implemented (`biff8_xls_codec.rs`, `office_codecs.rs`, `odf_hwpx_codecs.rs`, `cp949.rs`, `rich_read_rtf.rs`, `rtf_escape.rs`, `rich_read_hwpx.rs`, `convert_open.rs`, `convert_structured.rs`)
- **Context** (1) `"NaN"`, `"inf"` 같은 비유한 부동소수 문자열을 숫자 셀로 오분류하고, XLS 구조화 내보내기에서 수식 결과가 `'='`로 시작하는 문자열(`="=SUM(A1:A2)"`)을 다시 수식 레코드(캐시 0)로 저장했습니다(PD-149 한계). (2) XLS `BOOLERR`/`FORMULA` 불리언·오류 마커와 XLSX `t="b"`·공유 수식(`t="shared"`) 슬레이브 셀이 빈 칸으로 손실됐고, ODS 저장에는 수식 계산값과 `office:value`가 빠졌습니다. (3) ODT·RTF·HWPX 양방향 변환에서 번호 목록(`Ordered`)과 글머리표(`Bullet`) 구분이 사라졌고, RTF의 CP949(`\'xx\'yy` 및 비-UTF-8 원시 바이트) 한글과 표 셀 내부 줄바꿈(`\par`/`\line`)이 깨졌습니다.
- **Decision (`PD-152`)**
  1. **시트 코덱**: 유한 실수(`f64::is_finite()`)만 숫자 셀로 분류합니다. XLS 구조화 저장은 `LegacyOle2Codec::pack_xls_multi_values`로 계산된 값 셀을 기록해 `'='`로 시작하는 문자열 결과도 `LABELSST`로 보존합니다. XLS `BOOLERR`와 수식 캐시 불리언/오류 바이트, XLSX `t="b"`와 공유 수식 슬레이브 셀 오프셋 이동(`shift_shared_formula`)을 복원하고, ODS 저장(`pack_ods_multi_styled_with_cache`)에 계산된 표시값과 `office:value`를 함께 기록합니다.
  2. **목록·슬라이드·문서 표**: HWPX(`hh:heading type="NUMBER"`/`"BULLET"`), ODT(`L_Ordered`/`L_Bullet`), RTF(`\pnlvlbody`/`\pnlvlblt`)에서 목록 종류를 보존하고, HWPX `"JUSTIFY"` 정렬을 `RichAlignment::Justify`에 매핑합니다. ODP는 표준 `<draw:frame draw:layer="layout"><draw:text-box>`와 `<presentation:notes>` 구조로 쓰고 구형 파일도 함께 읽습니다. 파일을 열 때 문서 표의 `=SUM(...)` 등 수식을 계산하고 원본 메타데이터 제목을 보존합니다.
  3. **CP949·RTF**: 정적 `u16` 오프셋 테이블(`KSX1001_OFFSET`, 2,350자 O(1) 조회)과 통합완성형(UHC) 보충 이진 탐색으로 CP949 전체 음절(11,172자)·호환 자모를 복원해 RTF 2바이트 이스케이프와 비-UTF-8 바이트 열에 연결합니다. RTF 표 셀 안의 `\par`/`\line`은 셀 내부 줄바꿈으로 보존하고 `\u` 제어어 판별에서 `\ul` 오탐을 막습니다.
- **Why** 포맷 변환과 저장 후 재열기에서 수식 값·목록 번호·한글 인코딩이 바뀌거나 사라지는 데이터 손실을 막습니다.
- **Verified by** `cargo test -p easyoffice-formats`, `cargo test -p easyoffice-core`, `cargo test -p easyoffice-app --test structured_sheet_export`.

## PD-153. 손실 포맷 덮어쓰기 차단, 열린 확장자 유지 저장 및 시트·한글 편집기 실행 취소·클립보드 보강

- **Status** Implemented (`workspace-tools.js`, `file-open.js`, `workspace-session.js`, `sheet-store.js`, `sheet-ops.js`, `sheet-utils.js`, `sheet-input.js`, `ribbon-history.js`, `save-payload.js`, `tracked_files.rs`, `apps/hwp-studio/src/command/{clipboard-cut,safe-cut}.ts`)
- **Context** (1) PDF나 텍스트 대체 경로로 연 `.doc`·`.ppt`·`.pages`·`.hwp` 파일에서 일반 저장(Ctrl+S)을 누르면 `inPlace: true`로 원본 바이너리 위에 다른 포맷이나 평문을 덮어쓸 위험이 있었고, 양방향 포맷(`.odt`, `.rtf`, `.ods`, `.xls`, `.csv`, `.odp` 등)은 열린 확장자 대신 모듈 기본 확장자(`.docx`, `.xlsx`, `.pptx`)로 저장을 요청했습니다. (2) 시트에서 굵게·기울임·밑줄·취소선·조건부 서식·열 너비·틀 고정을 바꿔도 실행 취소 스냅샷과 수정 표시(`dirty`)가 갱신되지 않았고, 백분율 서식이 값을 100으로 나누어 표시했습니다. (3) 한글 편집기에서 `edit:copy`와 `ClipboardItem` 미지원 환경의 일반 텍스트 클립보드 대체 경로, 오려 두기 거부 시 상태줄 안내가 빠져 있었습니다(PD-151 보완).
- **Decision (`PD-153`)**
  1. **저장 안전성 및 확장자 유지**: PDF 모듈과 손실 폴백으로 연 파일은 `defaultSaveSpecForModule(..., false) -> inPlace: false`(`BIDIRECTIONAL_SAVE_EXTS`)로 두어 일반 저장(Ctrl+S)도 항상 다른 이름으로 저장 대화상자를 거치게 합니다. 양방향 편집 포맷은 모듈별 열린 확장자를 기억해 같은 포맷으로 덮어쓰기 저장하고, 데스크톱 셸 `tracked_files.rs`는 `.markdown`과 `.md`를 같은 형식으로 인정합니다. 시작 창(`?module=START`)에서는 편집·저장·찾기 단축키와 메뉴 명령을 차단합니다.
  2. **시트 서식·집계**: 셀 서식 플래그를 비트 마스크(`styleFlagsMask`)로 패킹해 `sheetCellDataChanged`와 `contentFingerprint`에 반영하고, 열 너비(`sheetColumnWidths`)와 틀 고정(`sheetFrozen`)도 실행 취소·수정 감지에 포함합니다. 백분율 서식은 원시 수치 그대로 `%`를 붙이며, 정렬·필터·함수 마법사는 하드코딩 행(2..6행) 대신 시트 스토어를 단일 패스로 스캔한 실제 데이터 범위를 씁니다.
  3. **한글 편집기 클립보드**: 참조 앱의 `writeTextHtmlWithPlainFallback`과 `safeCopyCommand`(`edit:copy`)를 이식해 `ClipboardItem`이 막힌 환경에서도 `navigator.clipboard.writeText`로 텍스트를 복사하고, 오려 두기가 거부되면 하단 상태줄(`#sb-message`)에 원인을 알립니다.
- **Why** 원본 파일을 다른 포맷으로 덮어쓰는 사고를 원천 차단하고, 서식 편집 후 저장·실행 취소 누락과 클립보드 실패 시 무응답을 없앱니다.
- **Verified by** `node --test scripts/test-webui-desktop-bridge.mjs scripts/test-webui-sheet-Extended.mjs scripts/test-webui-ribbon-doc.mjs scripts/test-hwp-clipboard-cut.mjs`, `cargo test -p easyoffice-desktop`.

## PD-154. 한글 임베드 창에서는 개인정보 지우기·보안 점검을 쓰지 않고, 시작 창은 복구 기록을 복원하지 않는다

- **Context** 두 도구는 페이지의 숨은 대체 DOM을 검사하는데, 한글 편집기가 문서를 맡는 동안 그 DOM은 비어 있어 "안전/0건"이라는 거짓 결과가 나왔습니다. 시작 창도 창 id로 정해진 복구 기록을 읽어 문서 창의 내용을 복원할 수 있었습니다.
- **Decision (`PD-154`)** (1) 한글 임베드가 활성이면 `scrub-pii`·`security-audit`은 메뉴·도구줄에서 회색이고(`HWP_EMBED_COMMANDS`에서 제외) 명령 팔레트로도 실행되지 않습니다. 편집기 포트에 본문 텍스트를 받는 호출이 없어 임베드 문서를 대상으로 삼지는 않습니다. (2) 시작 창에서는 `restoreFromStorage()`를 부르지 않습니다. (3) 시작 화면(`#home-screen`)도 웹뷰 기본 우클릭 메뉴를 막습니다(입력란 제외, 참조 앱과 같은 가드).
- **Why** 검사하지 못한 것을 안전하다고 알리는 것이 더 나쁩니다.
- **Trade-offs** 한글 문서의 개인정보 점검은 편집기가 텍스트를 내보내는 호출을 갖출 때까지 없습니다.
- **Verified by** `node --test scripts/test-webui-a11y.mjs`.
- **Revisit when** 임베드 포트가 본문 텍스트를 넘겨줄 때.

## PD-155. 문서 서식줄의 문단 서식은 선택 문단에 적용하고, 저장 모델에 없는 컨트롤은 두지 않는다

- **Status** Implemented (`doc-runs-ui.js`, `workspace-tools.js`, `workspace-panels.js`, `index.html`, `styles.css`, `toolbar.css`)
- **Context** 정렬·줄 간격·스타일 상자는 편집기 루트의 스타일과 `dataset`만 바꿔 문단별 값이 우선하는 저장 경로에서 사라졌고, 번호·글머리표는 "1. "·"○ " 평문 접두를 넣었으며, 언어별 글꼴 범주와 글자 효과는 `dataset`만 바꾸는 무동작 컨트롤이었습니다. 문서 모델(`RichAlignment`, `RichRun`)은 문단 정렬을 왼쪽·가운데·오른쪽·양쪽으로, 글자 글꼴을 run마다 하나로만 가집니다.
- **Decision (`PD-155`)**
  1. **선택 문단 적용**: 정렬·줄 간격·번호·글머리표·스타일은 선택 영역이 걸친 블록(P, H1-H6, LI)마다 적용하며 한 번의 실행 취소 단위입니다. 줄 간격은 블록 `style.lineHeight`, 정렬은 블록 `style.textAlign`으로 저장 경로(`save-payload.js`)가 읽는 값과 같습니다. 도구줄은 선택의 첫 문단 값을 보여 줍니다.
  2. **배분·나눔 정렬**: 모델에 대응이 없어 문단에 `data-eo-align`을 두어 버튼 상태만 기억하고 `justify`로 저장합니다. 지정할 때와 저장이 끝났을 때 상태줄이 양쪽 정렬로 기록됨을 알립니다(HWPX 읽기도 배분 정렬을 양쪽으로 받습니다).
  3. **번호·글머리표**: `<ol>/<ul><li>` 구조로 바꾸고 같은 종류를 다시 적용하면 문단으로 되돌립니다(중간 항목을 빼면 목록을 둘로 나눔). 문단의 정렬·들여쓰기·글자 run은 유지합니다.
  4. **스타일 상자**: 개요 1-3은 H1-H3, 바탕글·공문서 바탕글은 문단입니다. 모델에 구분이 없는 본문·쪽 번호·머리말·각주 항목은 뺐습니다.
  5. **제거**: 모델에 대응이 없는 언어별 글꼴 범주(`#doc-font-lang`)와 글자 효과(`#btn-stylebar-charfx`)를 뺐고, 쓰이지 않는 `data-strike`·`data-textcolor`·`data-highlight`·`data-charfx` 규칙과 루트 `dataset` 보존 필드를 지웠습니다.
- **Why** 화면에서 바꾼 서식이 저장 파일에 그대로 남고, 동작하지 않는 컨트롤이 없게 합니다.
- **Verified by** `node --test scripts/test-webui-doc-paragraphs.mjs scripts/test-webui-doc-runs.mjs scripts/test-webui-ux-ergonomics.mjs scripts/test-webui-shell-chrome.mjs`.

## PD-156. 시트 정렬 범위·수식 조정, 새 행·없는 시트의 빈 상태, 사용 범위 격자 내보내기, XLS 쓰기 한계

- **Status** Implemented (`sheet-ops.js`, `sheet-controller.js`, `ribbon-history.js`, `convert_structured.rs`, `biff8_xls_codec.rs`, `xls_sheet_names.rs`, `converter.rs`)
- **Context** (1) 시트 정렬이 A–H열과 B열 키로 고정이고, 옮긴 행의 수식에서 원래 행 번호만 바꿔 다른 행을 참조한 수식이 틀어졌으며, 정렬 뒤 재계산이 없었습니다. (2) 행 추가가 새 행 8칸에 `"0"`을, 없는 시트로 전환이 `"<이름> 항목"`·`500`을 넣었고, 탭 줄 정리가 `Sheet1`·`Sheet2`를 항상 남겼습니다. (3) 시트를 문서·PDF로 내보낼 때 행 길이가 제각각이고 빈 행이 사라졌습니다. (4) XLS 쓰기가 시트 이름 규칙·레코드 길이 한계를 지키지 않았고, 문서 텍스트가 `=`로 시작하면 수식 레코드로 나갔습니다.
- **Decision (`PD-156`)**
  1. **정렬**: 둘 이상의 셀을 선택하면 그 블록만 첫 열 기준으로 정렬하고(머리 행 판별 없음), 선택이 없으면 2행부터 사용 중인 모든 열을 B열(없으면 A열) 기준으로 정렬합니다. 선택 없는 정렬의 끝 행은 키 열에 값이 직접 들어 있는 마지막 행이라 아래의 수식 합계 행은 제자리에 남습니다. 키는 숫자 먼저, 텍스트는 로캘 순서, 빈 키는 항상 끝입니다. 이미 오름차순이면 내림차순으로 뒤집습니다(기존 동작). 옮긴 행의 수식은 클립보드 붙여넣기와 같은 규칙(`shiftFormula`)으로 상대 참조를 이동 거리만큼 조정하고, 끝나면 활성 시트를 재계산합니다. `shiftFormula`는 시트 접두(`Name!`, 따옴표 이름)와 셀·셀 범위·열 범위·행 범위를 한 토큰으로 읽어 다른 시트 참조도 범위 양 끝을 함께 옮기며, `$` 고정 부분·문자열 리터럴·함수 이름·`A1B` 같은 이름은 그대로 두고 범위를 벗어나면 `#REF!`로 바꿉니다.
  2. **빈 상태**: 행 추가는 빈 행만 늘립니다. 없는 시트로 전환하면 빈 시트를 만듭니다. 탭 줄은 시트 저장소와 활성 시트 이름만 기준으로 정리합니다.
  3. **문서·PDF 내보내기**: 시트 값 표를 사용 범위(값이 있는 셀의 최소·최대 행·열)의 직사각형으로 만들고 안쪽 빈 행도 유지합니다. 격자 칸 수는 기존 총 예산(`MAX_GRID_CELLS`)에 할당 전에 합산하며, 넘으면 잘라내지 않고 거부합니다.
  4. **XLS**: 시트 이름을 31자(UTF-16)·금지 문자 `: \ / ? * [ ]`(`_`로 대체)·앞뒤 작은따옴표 제거·빈 이름(`SheetN`)·대소문자 무시 중복(`_2`, `_3`)으로 정리하고, 바뀐 이름은 변환 경고에 남깁니다. 수식 레코드가 레코드 한도(8,224바이트)를 넘으면 잘라내지 않고 거부합니다. 문서·슬라이드에서 만든 셀(명시적 시트 셀이 없는 모델)은 `=`로 시작해도 항상 값(`LABELSST`)으로 저장합니다.
- **Why** 정렬과 행 추가가 데이터를 조용히 바꾸거나 깨뜨리지 않게 하고, 내보내기와 XLS 저장이 다음에 여는 엔진이 받아들이는 형태만 쓰게 합니다.
- **Trade-offs** 정렬은 상대 참조를 복사 규칙으로 이동하므로 정렬한 행 밖을 가리키는 수식은 표준 스프레드시트처럼 가리키는 대상이 달라질 수 있습니다. 문서 내보내기는 아주 넓게 흩어진 시트를 거부합니다. 빈 행만 늘린 시트는 저장하면 그 빈 행이 사라집니다.
- **Alternatives rejected** 정렬을 항상 선택 영역 필수로 하기(선택 없는 기존 동작 유지), 문서 내보내기를 A1부터 시작하는 격자로 만들기(앞쪽 빈 행·열이 늘어남).
- **Verified by** `node --test scripts/test-webui-sheet-formula-shift.mjs scripts/test-webui-sheet-rows-sort.mjs scripts/test-webui-sheet-editing.mjs scripts/test-webui-behavior.mjs scripts/test-webui-file-open.mjs scripts/test-webui-ux-ergonomics.mjs`, `cargo test -p easyoffice-app --test structured_sheet_export`, `cargo test -p easyoffice-formats --test xls_writer_limits`.
- **Revisit when** 정렬에 머리 행 자동 판별이 필요할 때.

## PD-157. Pages 읽기 점검: 요소별 지원 범위, 병합 지도·수식 저장소, 하이퍼링크·목록 번호·미주·댓글·쪽 위 개체, 독립 구현 기준 값

- **Supersedes PD-087 (지원 범위 표)·PD-115 (4항 쪽 위 개체)·PD-116 (Trade-offs의 번호 형식·미주)·PD-117 (병합 필드 번호)**. QA 2026-11-11: Pages는 오픈소스(MIT 독립 구현) 기준으로 진행한다. 쓰기는 이 작업 범위 밖이다.
- **Context** 실제 문서 5건(`tests/fixtures/pages/*.pages`)을 풀어 객체 형식을 나열하고 요소별로 점검했습니다. 픽스처는 Pages 템플릿에 다른 도구가 개체를 덧붙인 것으로 보여(2012·2033 등) 우리 "확인" 상수가 순환 검증일 수 있습니다. 공개 MIT 구현(`iwork` 크레이트 0.4.0, 출처 `THIRD_PARTY_NOTICES.md`)과 어긋나는 값은 그쪽을 기본으로 하고 우리 값은 같이 읽는 대체 자리로 둡니다. 확정하지 못한 값은 모두 **독립 구현 기준, 실파일 대조 전**이며 `pages_schema.rs`에 `검증 필요`로 적었습니다.
- **Decision (`PD-157`)**
  1. **조용히 빠지던 것(최우선 결함) 수정**: (a) 병합 영역: 실파일의 데이터 저장소 필드 13은 형식 6007 객체이고 항목은 `{원점 = 1, 크기 = 2}` 둘 다 `행<<16|열`을 담은 varint입니다(표 3·셀 종류 두 픽스처의 격자와 대조해 확인). 기존 코드는 중첩 fixed32·열 상위 배치를 가정해 병합을 전부 버렸습니다. 이제 이 지도를 읽고, 없으면 모델 필드 47 병합 소유자의 수식 저장소(노드 67 콜론 트랙트, 노드 36 단일 셀의 zigzag)를 읽습니다(`merges_from_owner` 이식, 예산·`checked` 연산 추가, 악의적 축·거대 span·쌍 5,000개 시험). (b) 문단 끝 문자: `\r`·`U+000C`(쪽·단 나누기)·`U+0004`(구역 나누기)·`U+0005`를 문단 끝으로 보고(이전에는 지워져 문단이 합쳐지고 서식 위치가 어긋남) 쪽·구역 나누기 개수를 경고합니다. (c) 글 속 표시: 책갈피, 변경 추적(삽입·삭제 표시, 삭제된 글자도 그대로 가져옴), 날짜·시각 필드, 자리표시자·병합·루비 필드, 알 수 없는 표시, 도형·각주·머리글 안의 각주·댓글, 다단 문단 구간을 종류와 개수로 경고합니다. 표식은 놓인 표의 필드 번호가 파일 버전마다 달라(픽스처: 각주 11·댓글 19, 독립 구현: 하이퍼링크 11·각주 16·언어 19·댓글 23) 가리키는 객체의 형식으로 분류합니다. (d) 표 셀 안 개체(그림·필드), 글이 없는 도형, 쪽 번호·쪽 수 첨부, 차트·동영상·그룹·선·목차 개체는 이름과 개수로 경고합니다.
  2. **리치 모델 확장(읽기 전용, 쓰기는 별도 작업)**: `RichRun.link`(하이퍼링크 주소), `RichParagraph.list_level`(0~8)·`list_number`(`RichListNumber`: 형식 십진·소문자 라틴·대문자 라틴·소문자 로마·대문자 로마·한글 음절, 둘러싸기 `1.`·`(1)`·`1)`). 주소는 `http://`·`https://`·`mailto:`·`tel:`·`ftp://`로 시작하고 제어 문자가 없으며 4,096바이트 이하일 때만 가져오고 나머지(`javascript:`·`file:` 등)는 글자만 두고 개수를 경고합니다. 목록 번호 값은 0~2 십진, 3~5 대문자 로마, 6~8 소문자 로마, 9~11 소문자 라틴(`a.`는 픽스처의 a-b-c 목록으로 확인), 12~14 대문자 라틴(각 묶음이 `1.`·`(1)`·`1)`), 45 한글 음절(픽스처의 가나다 목록으로 확인), 그 밖의 값은 `1.`로 두고 경고합니다. 현재 DOCX·ODT·HWPX 작성기는 이 필드를 쓰지 않으므로 변환 결과에서는 링크·수준·번호 형식이 빠집니다(TODO).
  3. **각주·미주·댓글**: 각주 표식은 표 11(우리 픽스처, 형식 2012)과 표 16(독립 구현, 형식 2008, 표식 문자 `U+000E`)을 모두 읽습니다. `TP.SettingsArchive`(형식 10012) 필드 30(0 각주·1 문서 미주·2 구역 미주)이 0이 아니면 부록 제목을 `미주`로, 31 번호 형식(1 로마 소문자·2 기호 `*†‡§‖¶`·3 전각 숫자·4 한자 숫자)을 본문 표식과 부록 번호에 적용하고, 32가 2(구역마다 다시 시작)면 본문 표(17)의 구역 위치에서 번호를 1로 되돌립니다. 32가 1(쪽마다)이면 쪽 배치를 알 수 없어 연속 번호로 표시하고 경고합니다. 댓글은 본문 표식의 형식 2013·2033·2014 객체에서 댓글 본문(형식 3056·3059의 필드 1)을 읽어 본문 뒤 `댓글` 부록 문단으로 옮깁니다(작성자·답글은 가져오지 않음, 읽을 수 없으면 경고).
  4. **쪽 배치 문서와 쪽 위 개체**: `TP.SettingsArchive` 필드 1이 0이면 쪽 배치 문서로 알립니다. 쪽 위 개체 목록(형식 10010, 구조는 픽스처와 독립 구현이 같음)의 글상자·도형 글, 그림, 표를 쪽 번호 순서, 쪽 안에서는 위→왼쪽 위치 순서로 가져오고(위치가 없는 것은 뒤), 그 밖의 개체는 `쪽 위의 개체: 이름(형식 N)`으로 경고합니다. 한 개체는 한 번만 읽고 개체 수는 100,000개로 제한합니다. 실제 쪽 배치 문서가 없어 합성 패키지로만 시험했습니다.
  5. **독립 구현 값을 같이 읽는 것**: 글꼴 이름 11.5(우리 전체 배치 16), 텍스트 배경 11.26(우리 19), 글자색 11.46.1 채우기(그려지는 색이라 11.7보다 우선), 문단 첫 줄 들여쓰기 12.7·왼쪽 들여쓰기 12.11·줄 간격 12.13(우리 13·16·19). 실제 템플릿 스타일은 12.13에 줄 간격(1.2·1.1458)을 가지므로 줄 간격 없음으로 읽던 문단이 이제 템플릿 값(예: 115%)을 가집니다. 표 셀 버전 5 비트 크기: 비트 2는 8바이트(날짜 값), 7·8·10·15·18~20은 4바이트(이제 읽음), 종류 10 통화는 숫자처럼, 날짜 서식 키는 비트 15, 소수 자릿수 253 이상은 "자동"(기존 20 초과 규칙이 포함). 간략 배치(스타일 필드 2/3)·타일 6003·버전 1 셀·각주 2012·댓글 2033의 "확인" 표시는 "검증 필요"로 내렸고 읽기 경로는 유지합니다.
  6. **고정 안내문**: "머리글·바닥글·각주·댓글은 본문 뒤 부록으로 옮기며 쪽 번호 등 필드·구역 설정·표의 테두리·셀 배경·열 너비·차트 등은 가져오지 않는다"로 바꿨습니다(리치 표 모델에 서식이 없어 모든 형식 공통 한계).
- **요소별 판정 (읽기 전용, `rich_read_pages*.rs`·`pages_table/`)**

  | 요소 | 판정 |
  |---|---|
  | 본문 문단·탭·문단 끝 문자 | 지원 (`\r`·`U+000C`·`U+0004`·`U+0005` 포함, 쪽·구역 나누기는 개수 경고) |
  | 글자 서식(굵게·기울임·밑줄·취소선·크기·글꼴·색·강조·첨자) | 지원 (두 배치 모두 읽음, 글꼴 전체 배치 5/16·배경 19/26·색 7/46.1) |
  | 문단 스타일 이름 | 지원 (제목 식별자만 제목 수준으로, 나머지 이름은 모델에 없음) |
  | 문단 정렬·들여쓰기·줄 간격 | 지원 (두 배치) |
  | 목록: 글머리표·번호, 수준, 번호 형식 | 지원 (모델 확장, 글머리 문자열은 모델에 없어 가져오지 않음) |
  | 표: 텍스트·숫자·날짜·통화·백분율·체크박스·기간·오류·수식 캐시 | 지원 (읽을 수 없는 셀은 경고) |
  | 표 병합 | 지원 (지도 + 수식 저장소) |
  | 표 셀 서식·테두리·열 너비·머리 행 | 경고(고정 안내문). 리치 표 모델에 없음 |
  | 표 안 개체(그림·필드)·표 안 하이퍼링크 | 개체는 경고, 셀 안 하이퍼링크는 조용히 글자만(TODO) |
  | 그림 | PNG·JPEG 지원, 그 밖은 경고, 자른 그림·대체 텍스트 처리(PD-115) |
  | 도형·글상자 | 글만 지원, 글 없는 도형·그룹·선은 경고 |
  | 쪽 위 개체 | 글상자 글·그림·표 지원, 나머지는 경고 (실파일 미확인) |
  | 각주·미주(번호 형식, 구역마다 다시 시작) | 지원, 쪽마다 다시 시작은 경고 |
  | 머리글·바닥글 | 부록 문단으로 지원(위치·첫쪽/짝홀 구분은 가져오지 않음), 안의 표는 경고, 쪽 번호·쪽 수 필드는 글자에서 빠지고 고정 안내문에만 나옴 |
  | 구역·단 | 구역 나누기·다단 구간은 개수 경고(설정 합침) |
  | 하이퍼링크 | 지원(모델 `link`, 쓰기 별도) |
  | 책갈피·목차·변경 추적·날짜 필드 | 경고(글자는 그대로) |
  | 주석(댓글) | 지원(부록) |
  | 차트·동영상·방정식 | 차트·동영상은 이름 경고, 방정식은 어떤 객체인지 실파일이 없어 판정 불가(형식 번호만 경고) |
  | 암호화·구형 XML | 오류로 거부(PD-087) |

- **Why** 변환 결과에서 문단이 합쳐지거나 표 병합이 풀리거나 각주·하이퍼링크가 사라지는 것을 사용자가 알 수 없었으므로, 모델에 옮길 수 있는 것은 옮기고 나머지는 종류와 개수를 경고로 남깁니다.
- **Trade-offs** (1) 독립 구현 값 중 실파일로 대조하지 못한 것(2008·`U+000E`·표 16, 하이퍼링크 2032, 댓글 2013→3056, 설정 30~32, 수식 저장소 병합, 목록 번호 3~14, 셀 비트 크기, 12.7·12.11·12.13)은 두 배치를 모두 읽고 실패해도 문서는 열리지만 값이 틀릴 수 있습니다. 실제 Pages로 각주·댓글·하이퍼링크·병합·쪽 배치 문서를 만든 파일이 필요합니다(QA). (2) 줄 간격이 템플릿 기본값으로 모든 문단에 붙습니다. (3) 변경 추적의 삭제 표시 글자도 본문에 남습니다.
- **Alternatives rejected** 독립 구현의 `.proto`·생성 코드·템플릿 가져오기(경계 위반), 하이퍼링크를 글자 뒤에 주소로 덧붙이기(원문에 없는 글이 생김), 쪽 배치 문서를 본문 저장소로 읽기(그려지지 않는 저장소).
- **Verified by** `cargo test -p easyoffice-formats --test pages_audit --test pages_fixtures --test pages_images --test pages_notes --test table_merge`(실픽스처의 병합 영역·목록 형식·수준·댓글 부록, 합성 패키지의 종류별 경고·수식 저장소·악의적 병합·링크 5,000개·쪽 위 개체 2,000개·미주 로마 번호·구역 재시작), `cargo test -p easyoffice-formats --lib pages_table`(셀 비트 크기).
- **Revisit when** 실제 Pages(15.x)로 만든 각주·댓글·하이퍼링크·병합·쪽 배치 문서를 구했을 때(두 배치 중 하나를 정리), 작성기가 링크·수준·번호 형식을 쓸 때.

## PD-158. 텍스트 계열 파일은 UTF-8·UTF-16(BOM)·CP949로 읽고, CP949 표에 기호·한자를 더한다

- **Status** Implemented (`text_decode.rs`, `cp949.rs`, `cp949_tables.rs`, `office_codecs.rs`)
- **Context** CSV·TSV·TXT·Markdown 읽기는 UTF-8이 아니면 거부했습니다. 한국어 스프레드시트의 기본 CSV 저장은 CP949이고 "유니코드 텍스트"는 BOM이 있는 UTF-16LE입니다. 엔진의 CP949 디코더는 한글 음절·호환 자모만 알아 기호(0xA1-0xAC행)와 한자(0xCA-0xFD행)가 빠져 있었습니다. XLSX 공유 문자열 인덱스를 못 읽으면 `unwrap_or(0)`으로 첫 문자열을 보여 주었습니다.
- **Decision (`PD-158`)**
  1. **판정 순서**: UTF-8 BOM, UTF-16LE/BE BOM, BOM 없는 유효 UTF-8, CP949. BOM은 벗기고 UTF-8이면 복사하지 않습니다. 형식 감지(`FormatDetector`)의 텍스트 검사도 같은 판정을 쓰며 해독한 글에 NUL이 있으면 거부합니다.
  2. **BOM 없는 UTF-16은 추측하지 않습니다.** NUL이 섞인 ASCII 위주 UTF-16은 유효한 UTF-8과 구분되지 않고, 스프레드시트의 UTF-16 저장은 항상 BOM을 씁니다.
  3. **Fail-Closed**: 홀수 길이 UTF-16, 짝 없는 서러게이트, BOM 뒤의 잘못된 UTF-8, 잘린 리드 바이트, 미할당 CP949 칸은 U+FFFD로 덮지 않고 `InvalidUtf8`로 거부합니다. 입력 크기 상한은 그대로입니다(디코더는 길이에 선형, 추가 상한 없음).
  4. **CP949 표**: encoding_rs 0.8.44의 EUC-KR 구조와 WHATWG `index-euc-kr`의 기호 1,128칸·한자 4,888칸을 `u16` 배열 두 장으로 이식했습니다(한글은 기존 오프셋 표). 전체 쌍 공간(126x190)의 FNV 해시와 할당 칸 17,048개를 참조 구현 값으로 고정했습니다. 의존성은 늘리지 않습니다(PD-053, PD-095). RTF 읽기도 같은 표를 씁니다.
  5. **XLSX 공유 문자열**: 인덱스가 숫자가 아니거나 범위를 벗어나면 `CorruptedContainer`로 거부합니다(A1 참조 오류와 같은 Fail-Closed).
- **Why** 한국어 사용자가 가장 흔히 받는 CSV가 열리고, 못 읽는 파일을 다른 내용으로 보여 주지 않습니다.
- **Trade-offs** 저장은 UTF-8만 합니다. CP949로 보이는 임의의 바이너리가 우연히 유효한 쌍으로만 이루어지면 텍스트로 열립니다. BOM 없는 UTF-16은 열리지 않습니다.
- **Verified by** `cargo test -p easyoffice-formats`(`cp949::tests`, `text_decode::tests`, `tests/text_encoding_open.rs`, `office_codecs::tests`). 10MB CP949 CSV 디코딩은 릴리스 빌드에서 이전 한글 전용 경로 19.4ms, 새 엄격 디코더 12.0ms(7회 중 최소).
- **Revisit when** BOM 없는 UTF-16 파일 요청이 실제로 들어오거나 CP949 외 지역 인코딩이 필요할 때.

## PD-159. 변환기 입력 소스 단일화, 한글 원본은 HWPX로 내보내기, PDF 줄바꿈·쪽 설정, 슬라이드 구조 보존, Markdown 표 셀·HWPX 표 순서

- **Status** Implemented (`workspace-tools.js` `resolveConverterInput`, `app.js`, `file-open.js`, `hwp-session.js`, `ipc-transport.js`, `rich_rtf_writer.rs`, `pdf_text_wrap.rs`, `pdf_writer.rs`, `convert_structured_doc.rs`, `rich_read_text.rs`, `rich_read_hwpx.rs`)
- **Context** (1) 변환 버튼, 변환기의 Ctrl+S, 보안 감사가 입력 소스를 각자 골라 선택한 원본 파일이나 열린 한글 문서 대신 기본 편집기 내용이 변환되기도 했고, 변환 실패는 오류가 침묵했습니다. (2) 엔진은 HWP 5.0·HWT를 평문으로만 읽어 한글 문서를 `exportBytes("hwp")`로 보내면 서식·표·그림이 사라집니다. (3) RTF 작성기는 제목 크기를 다음 문단으로 흘리고 첨자·들여쓰기·줄 간격·목록을 쓰지 않았습니다. (4) PDF 본문은 줄바꿈이 없어 긴 문단이 A4 밖으로 나갔고, PDF 스튜디오의 회전·워터마크·스탬프는 본문 줄로만 저장됐으며, 슬라이드를 문서로 저장하면 제목·글머리표·노트가 평문 단락으로 합쳐졌습니다.
- **Decision (`PD-159`)**
  1. 변환기 입력은 `resolveConverterInput` 한 곳에서 정합니다: 선택한 원본 파일 → 열린 한글 문서(편집기가 내보낸 바이트) → 편집기 내용 순이고, 크기 한도 검사도 여기서 합니다. 변환 요청의 예외는 전송 계층이 이미 보여준 것(`reported`)을 뺀 나머지를 `#conv-result-box`와 상태 문구에 "변환 실패: 사유"로 보입니다. 한글 편집기로 파일을 열면 `#conv-src-fmt`를 그 형식으로 맞춥니다.
  2. 한글 문서(HWP·HWPX·HWT·HWTX)가 변환 원본이면 항상 HWPX로 내보내 `hwpx` 원본으로 보냅니다. 내보내기가 실패하면 편집기 텍스트로 몰래 바꾸지 않고 오류로 알립니다.
  3. RTF는 문단마다 `\pard\plain`으로 시작하고, 런 서식 뒤 `\plain`에는 제목 크기를 다시 씁니다. 첨자(`\super`/`\sub`), `\li`, `\fi`, `\slN\slmult1`, 목록은 `\listtable`+`\lsN\ilvl0`(1=글머리표, 2=번호)로 씁니다.
  4. PDF 본문 줄은 쪽 너비(A4, 좌우 여백 54pt, 12pt)에서 글꼴 너비로 줄바꿈합니다. 공백 뒤에서 끊고, 한 줄보다 넓은 단어만 글자 사이에서 끊으며, 쪽은 출력 순서로 번호를 매깁니다. 영문 줄은 Helvetica 표준 너비, 한글이 섞인 줄은 글꼴 선언 너비(ASCII 0.5em, 그 외 1em)로 잽니다. 구조화 저장 본문 옆의 `pdf: {rotationDegrees, watermark, stamp}`는 PDF 대상에서만 쪽 회전·워터마크·결재 스탬프로 쓰고(`rotationDegrees`는 0·90·180·270), 다른 대상은 무시합니다.
  5. 슬라이드를 문서 계열 형식(DOCX·DOC·ODT·RTF·MD·TXT·HWP·HWT·HWPX·HWTX·PDF)으로 저장하면 제목은 2단계 제목, 글머리표는 글머리표 목록, 발표자 노트는 기울임 단락으로 씁니다.
  6. Markdown 표 셀은 문단과 같이 인라인 문법을 풀되 셀 모델이 글자 서식을 담지 못하므로 서식 기호만 벗기고 텍스트는 남깁니다. Markdown 작성기는 셀 글자(`*`, `_`, `` ` ``, `[`, `^`, `==` 등)를 이스케이프해 다시 읽어도 같은 글이 되게 합니다.
  7. HWPX에서 문단의 런 안에 표가 있으면 표 앞의 텍스트(와 그림)를 먼저 별도 문단으로 내보내 표가 그 뒤에 오게 하고, 표 뒤의 텍스트는 다음 문단이 됩니다.
- **Why** 입력 소스가 경로마다 달라 엉뚱한 내용이 변환되는 것과 실패의 침묵, 서식 손실, PDF에서 글자가 쪽 밖으로 나가는 것을 막습니다.
- **Trade-offs** PDF 하이라이트(형광펜) 주석은 쪽 설정으로 보내지 않습니다. 엔진의 PDF 모델에 주석이 없고 카드의 하이라이트는 범위가 없는 배지라 결정이 필요합니다. 줄 머리·끝 금칙 처리는 하지 않습니다. PDF 스튜디오의 워터마크·결재 스탬프는 PDF 대상에만 쓰고 PDF가 아닌 변환에서는 빠집니다(이전에는 본문 줄로 남았음). Markdown 표 셀의 링크는 텍스트만 남고 주소는 버려집니다.
- **Verified by** `cargo test -p easyoffice-formats --test rtf_writer_roundtrip --test pdf_line_wrap`, `cargo test -p easyoffice-app --test structured_pdf_slides`, `cargo test -p easyoffice-formats --test markdown_table_cells --test hwpx_table_in_run`, `node --test scripts/test-webui-converter-source.mjs scripts/test-webui-hwp-session.mjs scripts/test-webui-file-open.mjs`.

## PD-160. 숫자 서식 확장과 분포·XMATCH 함수는 단일 값 결과만 다루고, 셀 표시 서식은 모델이 생길 때까지 두지 않는다

- **Status** Implemented (`easyoffice-sheet`: `text_format*`, `special.rs`, `functions_dist.rs`, `functions_lookup.rs`)
- **Context** `TEXT()`의 서식 해석은 색·조건·분수·로캘·`DBNum`을 몰랐고 호출마다 서식 문자열을 다시 쪼갰습니다. 통계 함수에는 분포와 역함수가 없었습니다. 시트 셀 모델(`sheet-store.js`)과 XLSX 저장(`numFmtId="0"`)에는 셀별 표시 형식 자체가 없어 서식 결과가 화면에 닿는 경로가 `TEXT()`뿐입니다.
- **Decision (`PD-160`)**
  1. **서식**: 서식 코드는 한 번 파싱해 스레드별 캐시(256개)에 두고, 색(`[Red]`, `[Color1-56]`)·조건(`[>100]`)·분수(`# ?/?`, `?/16`, 분모 최대 5자리)·`[$기호-LCID]`·한국어 이름(`[$-412]`의 `오전/오후`, `1월`, `월요일`, `aaa`·`aaaa` 요일)·`[DBNum1-4]`·`_x`(공백 하나)·`*x`(채움 위치와 문자를 결과에 실음)를 지원합니다. 색과 채움은 `format_number_styled`가 돌려주고 `TEXT()`는 글자만 씁니다.
  2. **조건 구간 선택**: 첫 조건이 맞으면 1구간, 아니면 둘째 조건, 둘 다 조건이 있고 구간이 셋이면 셋째입니다. 둘째 구간은 음수에 부호를 붙이지 않습니다. 조건이 없으면 양수;음수;0 순서입니다.
  3. **DBNum**: 1은 한자(`一千二百三十四`), 2는 갖은자(`壹阡貳百參拾四`), 3은 전각 숫자, 4는 한글(`일천이백삼십사`)입니다. 1·2·4는 자리 단위를 쓰고 앞자리 1도 적습니다(`일십`). 날짜 구간의 숫자는 바꾸지 않습니다.
  4. **분포 함수**: `T`·`CHISQ`·`BETA`·`GAMMA`·`BINOM`·`NORM`의 `.DIST`/`.INV`와 옛 이름, `GAMMA`, `GAMMALN`, `XMATCH`를 더했습니다. 수치는 불완전 감마·베타(오픈소스 참조 구현 이식, 출처는 `THIRD_PARTY_NOTICES.md`)와 이분 역함수이며 정의역 밖은 `#NUM!`, 숫자가 아닌 인수는 `#VALUE!`입니다.
  5. **범위 밖**: 배열 결과가 필요한 `TEXTSPLIT`, `FILTER`, `SORT`, `UNIQUE`, `SEQUENCE`와 `LET`은 셀이 한 값만 갖는 현재 모델에서는 넣지 않았습니다(스필 모델 도입 때). 셀별 표시 형식(저장 포함)이 없어 서식 색을 화면에 반영하는 UI는 두지 않았습니다.
- **Why** 참조 앱과 같은 서식 결과를 내고, 10만 셀 렌더링에서 파싱·할당 비용을 줄입니다. 값이 한 칸을 넘는 결과는 잘못된 근사 대신 만들지 않습니다.
- **Trade-offs** 조건 구간의 부호 처리는 오픈소스 참조 구현을 따른 근사입니다. 모델이 없는 표시 형식 UI는 저장 형식(`numFmt`)과 함께 설계해야 합니다.
- **Verified by** `cargo test -p easyoffice-sheet`(`sheet_number_format_codes`, `sheet_distribution_functions`), 렌더링 측정 `cargo test -p easyoffice-sheet --release --test format_render_bench -- --ignored --nocapture`.
- **Revisit when** 셀별 표시 형식이 모델·저장에 들어오거나 동적 배열 스필을 도입할 때.

## PD-161. PDF 스튜디오의 형광펜·밑줄은 문구를 칠하는 텍스트 마크업 주석으로 쓰고, 원본 PDF 위 주석은 2단계로 둔다

- **Status** Implemented (`pdf_highlight.rs`, `pdf_writer.rs`, `pdf_text_wrap.rs`, `convert_structured_doc.rs`, `index.html`, `shell.css`, `custom-select.js`, `ribbon-history.js`, `workspace-tools.js`, `workspace-panels.js`)
- **Context** 형광펜·밑줄 주석이 범위 없는 배지라 저장해도 PDF에 아무것도 남지 않았습니다(PD-159 Trade-offs).
- **Decision (`PD-161`)**
  1. 메뉴 명령 "형광펜·밑줄 주석"(`pdf-mod-highlight`, ⌃⌘H)은 참조 앱의 옵션 대화상자 틀(`<dialog class="tool-dialog">`, `.dialog-row`·`.dialog-input`·`dialog-select` 콤보, 확인·취소)을 열어 문구(200자), 종류(형광펜·밑줄), 색(노랑·초록·하늘·분홍)을 받습니다. 종이(PDF 카드)에는 입력 칸을 두지 않고, 확인한 값은 카드 안 숨김 상태(`#pdf-highlight-input`·`-kind`·`-color`)에 저장해 실행 취소 스냅샷·개인정보 지우기·저장이 한 곳을 읽습니다. 확인은 명령을 한 번 실행하는 실행 취소 한 단계이고, 카드의 기존 `.pdf-highlight-badge`에 "형광펜: '문구'"(밑줄은 "밑줄: '문구'")를 보입니다. 문구를 비우고 확인하면 주석을 지웁니다. 취소·닫기는 저장된 값을 바꾸지 않습니다. 구조화 저장의 `pdf` 객체에 `highlight:{text, kind, color}`를 더하고 엔진이 검증합니다(문구 1~200자·제어문자 거부, kind는 `highlight`·`underline`, 색은 `#RRGGBB`). PDF 대상에서만 쓰고 다른 대상은 무시합니다. 대화상자 행 규칙은 참조 앱 `dialogs.css` 110-170의 `.dialog-row`·`.dialog-label`·`.dialog-input`을 `shell.css`에 옮겼습니다.
  2. 엔진(`PdfWriteOptions.highlight`)은 줄바꿈 전의 원문 줄에서 문구의 모든 출현을 찾고, 출력 줄마다 글자 너비(줄바꿈과 같은 Helvetica·한글 선언 너비)로 사각형을 계산합니다. 줄이나 쪽 경계를 넘는 출현은 쪽마다 주석 하나에 줄별 QuadPoints를 담습니다. 주석은 ISO 32000-1 12.5.6.10의 `/Subtype /Highlight`·`/Underline`, `/Rect`, `/QuadPoints`(왼쪽 위·오른쪽 위·왼쪽 아래·오른쪽 아래 순서), `/C`, `/Contents`, `/F 4`와 `/AP /N` 폼 XObject로 씁니다. 형광펜 AP는 `/BM /Multiply`로 칠해 글자가 가려지지 않습니다.
  3. 검색은 `str::match_indices`와 이분 탐색으로 입력에 선형이고, 주석은 문서당 10,000개까지 쓰며 넘는 출현은 건너뛰고 응답 경고로 알립니다. 문구를 본문에서 못 찾으면 주석 없이 경고합니다.
  4. 엔진 의존성이 없으므로 pdf-writer 0.15(MIT OR Apache-2.0)와 pdf.js 편집기(Apache-2.0)의 구조만 참조해 직접 썼고 출처를 `THIRD_PARTY_NOTICES.md`와 파일 머리 주석에 남겼습니다. 이 기능은 새로 만드는 PDF(저장·변환)에만 해당합니다.
- **Why** 저장한 PDF를 다른 뷰어에서 열어도 형광펜·밑줄이 문구 위에 그대로 보여야 합니다.
- **Trade-offs** 모양은 뷰어가 쓰는 글꼴에 따라 한 글자 너비 안에서 어긋날 수 있습니다(글꼴을 내장하지 않아 선언 너비로 계산). 색은 4가지 고정 선택지입니다. 주석은 글자 위치가 아닌 문구 일치로 정해져 같은 문구는 모두 칠해집니다.
- **2단계(범위 밖)** 원본 PDF를 보존한 채 그 위에 주석을 더하는 편집(원본 보존 편집)과 글자 선택 영역 기반 주석은 이 작업에 넣지 않았습니다. 원본 PDF의 글자 위치를 읽는 경로가 먼저 필요합니다.
- **Verified by** `cargo test -p easyoffice-formats --test pdf_highlight`, `cargo test -p easyoffice-app --test structured_pdf_slides`, `node --test scripts/test-webui-converter-source.mjs`, `qpdf --check`와 `pdftoppm` 렌더로 형광펜·밑줄이 글자 위에 맞게 그려지는 것을 확인했습니다.
- **Revisit when** 원본 PDF 위 편집을 시작할 때, 글꼴을 내장할 때.

## PD-162. Pages 최소 쓰기(실험적)

- **Revises PD-087 (1항)**: Pages는 더 이상 "읽기 전용 원본"이 아닙니다. `DocumentFormat::READ_ONLY`·`is_read_only()`를 `EXPERIMENTAL_WRITE`·`is_experimental_write()`로 바꾸었고, `ALL`(읽기·쓰기 20종)에는 넣지 않습니다. 열기 범위·보안·스키마 상수(PD-087 2~5항, PD-157)는 그대로입니다.
- **Context** 허용 라이선스(MIT)의 독립 구현이 Apple 파일·템플릿 없이 Pages가 여는 문서를 처음부터 만든다는 근거(객체 9~11개, 쓰기 규칙 26개)가 있어, QA 승인(2026-11-11)으로 오픈소스 기준의 최소 쓰기를 시범으로 둡니다.
- **Decision (`PD-162`)** (1) 쓰는 것: 리치 문서의 본문 문단, 제목 1~6(스타일 `Heading n`), 정렬(문단 스타일의 이름 없는 변형), 굵게·기울임·밑줄·글자 크기·색(글자 스타일의 이름 없는 변형, 같은 모양은 하나를 공유). 표·그림은 쓰지 않고, 목록 문단은 글만 남기고 표지를 뺍니다. 취소선·링크·글꼴·형광펜·위/아래 첨자·들여쓰기·줄 간격도 쓰지 않습니다. 저장 응답 `warnings`에 `Pages 저장에서 빠진 요소 N개`(표·그림·목록 문단)와 `Pages 저장에서 빠진 서식 N곳`을 알립니다. 시트·슬라이드 내용은 거부합니다(빈 문서로 만들지 않음). (2) 패키지 규칙: ZIP 항목은 저장(STORED)만, Snappy 블록은 64 KiB 이하(리터럴 전용), protobuf 필드는 번호 오름차순(어긋나면 쓰기 오류), `MessageInfo.object_references`는 소유 참조만(독립 구현이 측정한 약한 참조 표의 해당 형식 부분을 이식), `TSP.PackageMetadata`(필드 5·7·8, 구성 요소 1·2·4·5·6·10·12·21)에서 구성 요소 밖으로 나가는 참조를 선언하며 루트는 `{component}` 짧은 형태, 객체 번호는 메타데이터 필드 1 이하(새 번호는 그 위), 색은 모든 필드, 변형 스타일에는 이름·식별자 없음, `Metadata/Properties.plist`(binary plist)·`DocumentIdentifier`의 UUID는 쓸 때마다 새로, 미리보기 그림은 쓰지 않음. 객체별 UUID 표·계산 엔진·표 스타일은 쓰지 않습니다. (3) 값은 우리 것: 본문 12pt·제목 24/20/17/15/13/12pt 굵게, 글꼴 `Helvetica`, 팔레트 27색, A4·여백 1인치. Apple 템플릿 값은 복사하지 않습니다. (4) 한도: 문단 200,000·글자 구간 1,000,000(읽기 한도와 같음), `.iwa` 64 MiB·문서 256 MiB. (5) 검증: 쓴 바이트를 우리 리더로 다시 읽어 문단 글이 쓰려던 것과 같은지 확인하고(다르면 쓰기 오류), 저장 검증(`save_verify.rs`, PD-109)은 ZIP 구조와 우리 리더가 문서로 여는지 확인합니다. 우리 리더로 하는 이 검증 자체는 순환입니다(작성기와 리더가 서로 맞는다는 증거). 그래서 실제 Pages 앱으로 열기·다시 저장을 따로 확인했습니다(아래 Verified by). 밑줄·정렬은 앱에서 읽지 못해 미확인입니다. (6) 변환기의 원본·대상 목록에 `Pages (.pages, 실험적)`으로 노출합니다.
- **Why** 쓸 수 있는 형식이 늘면 변환 경로가 열리고, 범위를 본문·기본 서식으로 줄이면 알려진 규칙 안에서 검증된 구조만 쓰게 됩니다. 빠지는 요소는 숨기지 않고 개수로 알립니다.
- **Trade-offs** 독립 구현이 앱에서 연 구성(테마·그림 미리 설정 28개 포함)을 따랐으나, 변형 스타일을 스타일시트 필드 1·5에 올리는 방식과 필드를 오름차순으로 다시 정렬한 부분(독립 구현은 일부 반복 필드를 섞어 씀)은 앱이 오류 없이 열고 다시 저장해 받아들였습니다. 밑줄·정렬 표시와 스타일 메뉴 구성은 확인하지 못했습니다. 계산 엔진·헤더/푸터·섹션이 없는 문서라 앱이 열고 나서 보정할 수 있습니다. 제목 스타일 이름이 `Heading n`뿐이라 앱의 `Title` 스타일과 다릅니다.
- **Alternatives rejected** 크레이트로 그대로 쓰기(자체 객체 모델·Snappy 압축 의존성·`/dev/urandom` 의존, 상한·Fail-Closed를 다시 짜야 함), Apple 템플릿에서 복사(경계 위반), 표·그림까지 1단계에 쓰기(셀 레코드 쓰기 규칙이 앱 확인 없이는 위험).
- **Verified by** `cargo test -p easyoffice-formats --test pages_write`(왕복 문단·제목·정렬·서식, STORED, 64 KiB 블록, 필드 오름차순, 참조 목록, ID 범위, 구성 요소 색인, 색·변형 이름, 새 UUID, 200,001문단 거부, 손실 개수), `cargo test -p easyoffice-formats --lib pages_write`, `cargo test -p easyoffice-desktop save_verify`. Pages 앱 확인(QA 리드, 2026-11-11, AppleScript 판독): 샘플 3개(`pages-write-{1-basic,2-formats,3-long}.pages`)가 오류·복구 알림 없이 열렸고 문단 수는 10·3·1,200(마지막 `1200.` 문단 있음)이었습니다. 제목은 Helvetica-Bold 24/20/12pt, 굵게·기울임(Helvetica-Oblique)·굵은 기울임·20pt·14pt 굵게 파랑 구간과 이모지 뒤 굵은 구간 위치, long의 번호 굵게가 정확했습니다. basic을 다른 이름으로 저장하고 닫은 뒤 다시 열어도 10문단이 유지되었고 로그에 `TSPersistence`/`TSUAssert`가 없었습니다. 색은 Pages가 돌려주는 값이 FF0000→(251,0,7), D32F2F→(198,26,36), 1565C0→(19,78,178)로 일정한 색 공간 변환 차이로 보이며 문제로 판단하지 않았습니다(관찰만 기록). 밑줄·정렬은 AppleScript로 읽을 수 없어 미확인입니다.
- **Revisit when** 밑줄·정렬·스타일 메뉴를 앱에서 확인했을 때, 표·그림·목록을 쓰려 할 때.

## PD-163. PDF 내보내기: 글꼴 질의 메모, 글꼴 데이터 공유(svg2pdf 로컬 패치 기록), 압축 수준 유지

- **Status** Implemented (`pdf_font_select.rs`, `pdf_export.rs`, `vendor/svg2pdf/src/render/text.rs`, `config/svg2pdf-local-patches.json`, `scripts/rhwp-svg2pdf.mjs`)
- **Context** 성능 측정(`docs/performance.md` 4절)에서 셸 PDF 내보내기의 병목은 (1) 글자 요소마다 글꼴 면 수백 개를 걸러 찾는 `fontdb::Database::query`(포함 시간 20~28.6%), (2) 쪽마다 글꼴 파일 전체를 복사하는 `fill_fonts`(`memmove` 자기 시간 21%, 최대 RSS 425~829 MB), (3) 쪽 스트림 deflate 수준 6(7~13%)이었습니다. PD-101은 `vendor/svg2pdf`가 rhwp 고정 사본과 같아야 하고 다르면 갱신을 막는다고 정했습니다.
- **Decision (`PD-163`)**
  1. **글꼴 질의 메모**: usvg의 글꼴 선택기를 (패밀리 목록, 굵기, 늘이기, 모양) → 면 ID 메모로 감쌉니다(`pdf_font_select.rs`). 메모는 내보내기 한 번에 하나이고 모든 쪽 스레드가 읽기 잠금으로 공유하며 내보내기와 함께 버립니다(글꼴 DB는 내보내는 동안 바뀌지 않음). 요청 4,096개와 패밀리 이름 합계 512바이트를 넘는 요청은 저장하지 않고(신뢰할 수 없는 SVG의 메모리 상한) 매번 DB에 묻습니다. 대체 글꼴 선택기(PD-114)는 그대로입니다. 쪽을 비트맵으로 그리는 경로(`bitmap_parse_options`)는 쪽마다 새 메모를 씁니다.
  2. **글꼴 데이터 공유**: `vendor/svg2pdf/src/render/text.rs`의 `Font.face_data`를 `Arc<Vec<u8>>`에서 `Arc<dyn AsRef<[u8]> + Send + Sync>`로 바꾸고, 글꼴 DB가 메모리에 쥔 면(`Binary`, 파일 매핑 `SharedFile`)은 복사하지 않고 같은 `Arc`를 씁니다. 파일을 접근마다 여는 면(`File`)만 이전처럼 복사합니다. 출력 바이트는 같습니다(세 원본으로 시험).
  3. **svg2pdf 로컬 패치 기록**: 패치한 파일은 `config/svg2pdf-local-patches.json`에 `base`(rhwp 사본의 blob id), `patched`(우리 파일의 blob id), 요약을 적습니다. `update-rhwp.mjs`의 svg2pdf 검사는 로컬 blob이 `patched`이고 상위 blob이 `base`인 파일을 차이로 세지 않습니다. 상위가 그 파일을 바꿨으면 `--sync-svg2pdf`로도 덮어쓰지 않고 "패치를 손으로 합치고 기록을 갱신하라"고 막습니다. 기록과 맞지 않는 파일은 지금처럼 차이로 막습니다. `node --test scripts/test-update-rhwp.mjs`가 기록과 실제 파일의 blob id 일치를 확인하므로, 패치 파일을 고치면 기록도 함께 고쳐야 합니다.
  4. **압축 수준은 6 그대로**: 수준 1~4를 재 보니 CPU는 2~9% 줄지만 PDF가 4~25% 커져(수준 1: form-002 +18%, kps-ai +19%, 합성 300쪽 +25%) 바꾸지 않습니다.
  5. **쪽마다 `ttf_parser::Face::parse`**(usvg 글자→경로 변환, 합성 300쪽에서 포함 약 22%)는 usvg 공개 API로 재사용할 수 없어 손대지 않았습니다(usvg 사본을 패치해야 함).
- **Why** 출력이 바이트 단위로 같은 채 CPU 시간과 최대 RSS를 줄입니다. 메모는 글꼴 DB가 고정이라는 전제 위에서만 맞고, 그래서 내보내기 단위입니다.
- **Trade-offs** 다음 rhwp 갱신 때 `text.rs`를 상위가 바꾸면 패치를 손으로 합쳐야 합니다. 상위가 같은 수정을 넣으면 기록에서 지우면 됩니다.
- **Verified by** `cargo test -p easyoffice-desktop pdf_font_select a_face_gives`, `node --test scripts/test-update-rhwp.mjs`, 전후 PDF 바이트 비교(7개 입력 동일), `docs/performance.md` 4.1절 측정.

## PD-164. 시트 재계산은 엔진이 열어 둔 세션에서 변경분만 증분으로 하고, 화면은 바뀐 셀만 주고받는다

- **Status** Implemented (`crates/easyoffice-sheet/src/recalc/`, `crates/easyoffice-app/src/sheet_session.rs`, `ui/sheet-store.js`, `ui/app.js`, `crates/easyoffice-desktop/src/{main,window_commands}.rs`)
- **Context** 시트 재계산은 편집마다 통합 문서를 새로 만들고 모든 수식을 다시 계산했고(`evaluate_sheet`는 상태가 없음), 화면은 편집마다 모든 셀과 모든 수식 대상을 JSON으로 보냈습니다. 수식 10만 개(셀 13만 개) 시트의 요청은 10.9 MB로 PD-150의 10 MiB(10,485,760 B)를 넘어 재계산 자체를 보낼 수 없었습니다.
- **Decision (`PD-164`)**
  1. **엔진**: `RecalcSession`이 통합 문서, 수식 셀의 값(메모), 역의존 그래프를 들고 있습니다. 편집은 그 셀을 읽는 수식(직접 참조와 범위 참조)만 변경 전파(너비 우선, 더러운 집합은 비트셋)로 더럽히고, 더러운 수식만 기존 의존 사전 패스(반복 DFS, 순환 감지)로 위상 순서대로 계산합니다. 결과는 값이 바뀐 수식 셀 목록입니다. 범위 참조는 같은 사각형을 한 항목으로 두고 열별(폭 16 이하) 또는 공용(그보다 넓음) 행 구간 색인에서 점 조회로 찾으므로 `SUM(A:A)`도 셀 단위 간선을 만들지 않습니다. 셀·수식 색인(`CellIndex`·`FormulaIndex`)은 편집마다 버리지 않고 제자리에서 고칩니다.
  2. **휘발성·순환**: `RAND`·`RANDBETWEEN`·`NOW`·`TODAY`·`OFFSET`·`INDIRECT` 수식은 매 패스 더럽다고 봅니다(`OFFSET`·`INDIRECT`는 참조가 실행 때 정해짐). 순환 참조가 하나라도 있으면 어느 셀이 오류를 지니는지가 진입 순서에 달려 있어서, 그동안은 전체 수식을 전체 계산과 같은 순서로 다시 계산합니다(결과가 전체 재계산과 같음을 보장). 순환이 풀리면 다시 증분입니다.
  3. **IPC 계약**: 새 명령을 늘리지 않습니다(15종 계약 유지, PD-069 선례). `evaluate_sheet` 페이로드에 `sessionAction`이 있으면 세션 동작입니다. `open {sheets:[이름]}` -> `{session}`; `apply {session, changes:[{sheet,cell,value}], recalc?}` -> `{changed:[{sheet,cell,value}], circular, recomputed}` (`recalc:false`는 적재만 하고 `{staged}`를 돌려줌, 큰 시트를 나눠 보낼 때); `close {session}` -> `{closed}`. 부트스트랩에 `sheetSessions:true`가 새로 있습니다. 세션이 없으면(만료·밀려남·창 닫힘) 오류가 아니라 `{expired:true}`로 답하고 화면이 다시 열어 모든 셀을 한 번 보냅니다. `sessionAction`이 없는 `evaluate_sheet`와 `evaluate_workbook_formula`는 그대로이고, 화면은 부트스트랩이 `sheetSessions`를 알릴 때만 세션을 쓰며 열기·적용이 실패하면 그 호출만 기존 방식으로 대신합니다.
  4. **상한(`sheet_session.rs`, `recalc` 예산)**: 의존 집합은 정렬 벡터(원소당 4 B)라서 메모리가 간선 수에 비례하고 정점 번호 크기와 무관합니다(처음 구현은 33개 초과 시 정점 번호 최댓값/8 B짜리 비트셋으로 승격해 수식 3만 개·40칸 읽기 패턴에서 296 MB, 10만 개에서 1.56 GB를 썼음). 엔진은 간선 수·그래프 추정 바이트(`Footprint`)를 세고 예산(`Budget`)을 넘기면 편집을 거부하며 그 세션은 쓸 수 없습니다. 레지스트리 상한: 세션 16개(넘으면 가장 오래 안 쓴 것을 내보냄), 세션당 시트 1,024개·이름 1,024 B·셀 200만 개·간선 800만 개·메모리 512 MiB(원문 + 그래프 + 수식 구문 트리 추정, 수식 원문 1 B당 24 B), 전체 셀 400만 개·메모리 1 GiB(넘으면 오래된 세션부터 내보냄), 한 요청의 변경 25만 개, 유휴 30분 만료. 상한을 넘는 요청은 오류가 아니라 세션을 버리고 `{refused:"limit"}`로 답하며 화면은 그 시트 집합을 세션 없이 기존 방식으로 계산합니다(시트 집합이 바뀌기 전에는 다시 열지 않음). 세션은 연 창(owner)만 쓸 수 있고(`apply`·`close`가 owner를 확인) 세션 id는 OS 난수로 키가 정해진 해시 128비트입니다. 엔진 패닉으로 세션 락이 오염되면 그 세션을 버리고 `expired`로 답합니다. 창이 닫히면(`Destroyed`) 그 창 라벨로 연 세션을 닫습니다. 시트 집합은 세션 동안 고정이라 시트가 늘거나 줄거나 이름이 바뀌면 화면이 새 세션을 엽니다.
  5. **화면**: `sheet-store.js`는 여전히 전체 저장소를 받지만(실행 취소·불러오기·개인정보 지우기가 임의 스냅샷을 되돌림) 세션이 마지막으로 받은 셀 원문의 거울과 비교해 달라진 셀만 보내고(1.5M자 단위로 나눔), 받은 바뀐 값을 거울에 합칩니다. 비교 기반이라 버려진 수식 입력줄 미리보기처럼 거울이 어긋나도 다음 호출이 바로잡습니다. 호출은 한 줄로 직렬화합니다. 수식이 아닌 셀을 수식 입력줄로 평가하는 호출은 표시 문자열을 그대로 두려고 기존 방식(`sessionAction` 없음)을 씁니다.
- **Measured** (`cargo test -p easyoffice-sheet --release --lib -- --ignored --nocapture recalc_bench`, `cargo test -p easyoffice-app --release --lib -- --ignored --nocapture ipc_recalc_bench`, 셀 한 개 변경): 수식 1만 개·셀 1.3만 개에서 기존 `evaluate_sheet`는 요청 1,045,964 B + 응답 304,187 B에 14.6 ms, 세션은 요청 87 B + 응답 271 B(말단 셀 편집)에 0.03 ms입니다. 수식 10만 개·셀 13만 개에서 기존 `evaluate_sheet`는 요청 10,889,319 B(상한 초과) + 응답 3,406,238 B에 165.8 ms, 세션은 요청 88 B + 응답 278 B에 0.33 ms입니다. 계산 사슬의 중간·머리 셀을 바꿔 수식 1.6만~3.3만 개가 바뀌면 9.7~18.9 ms에 응답 0.9~1.8 MB로, 전체 계산(65 ms)과 요청 전송보다 낮습니다.
- **Why** 편집 비용이 시트 전체가 아니라 영향을 받는 수식 수에 비례하고, 요청이 10 MiB 상한과 무관해집니다. 결과는 무작위 편집(비순환·순환 각 수백 시드)에서 전체 재계산과 같음을 시험으로 강제합니다.
- **Trade-offs** 세션은 프로세스 메모리를 씁니다(상한으로 묶음, 추정치라 실제 사용량은 ±수십 %). 상한을 넘는 시트는 편집마다 기존 전체 계산으로 돌아갑니다. 세션을 처음 열 때 한 번은 모든 셀을 보냅니다(수식 10만 개 시트 약 7.4 MB 요청·5.2 MB 응답, 0.16 s). 순환 참조가 있는 동안은 편집마다 전체 계산입니다. 화면의 차이 계산(JS)은 셀 수에 비례합니다.
- **Alternatives rejected** `evaluate_sheet` 요청의 압축·분할(편집마다 전체 재계산이 남음), 화면이 의존 그래프를 직접 들고 있기(엔진과 이중 구현), 순환 구성요소별 부분 재계산(진입 순서에 따라 오류를 지닌 셀이 달라져 전체 재계산과 어긋남).
- **Verified by** `cargo test -p easyoffice-sheet --lib recalc`(무작위 편집 속성 시험, 순환, 시트 간 참조, 행 삽입·삭제 뒤 의존 갱신), `cargo test -p easyoffice-app --lib sheet_session`(상한·owner·락 오염·수식 3만 개 읽기 패턴 거부/상한), `node --test scripts/test-webui-sheet-session.mjs`.
- **Revisit when** 시트 집합 변경에도 세션을 유지해야 할 때, 순환 참조가 흔한 문서에서 편집이 느려질 때, 응답이 매우 큰 첫 계산을 나눠 받아야 할 때.

## PD-165. Quick Look 미리보기: 글꼴 면 한 번 매핑, 글꼴 서브셋 포함, 쪽 스트림 deflate 수준 1

- **Context** Quick Look 측정(`docs/performance.md` 2.4.1)에서 미리보기 PDF가 글자를 윤곽선으로 내보내 77쪽 문서가 32.7MB·9.6초였고, 글꼴 면을 접근마다 열고 매핑해 첫 쪽 PNG의 63%를 시스템 호출이 차지했습니다.
- **Decision (`PD-165`)** (1) Quick Look 확장은 글꼴 DB의 면을 프로세스당 한 번 읽기 전용으로 매핑합니다(셸 PD-114 3과 같은 방식). (2) 미리보기 PDF는 글자를 글꼴 서브셋으로 포함하고, 서브셋이 실패한 쪽만 윤곽선으로 다시 변환합니다(PD-114 5와 같은 재시도, PD-140 2의 "글자는 윤곽선"을 바꿈). (3) 글꼴 파일을 복사하지 않고 DB 매핑을 공유하는 `fill_fonts` 패치(PD-163)가 있어야 RSS가 늘지 않습니다. (4) 쪽 콘텐츠 스트림 deflate 수준을 1로 낮춥니다(미리보기는 한 번 읽는 용도라 속도 우선). `vendor/svg2pdf`에 `ConversionOptions::compression_level`(기본 6)을 더했고(`config/svg2pdf-local-patches.json`에 기록) 셸 출력은 그대로입니다.
- **Why** 77쪽 문서 PDF가 9.6초 → 1.5초, 32.7MB → 4.0MB로 줄고, 글자를 검색·복사할 수 있습니다.
- **Trade-offs** 최대 RSS는 적용 전 이하입니다(77쪽 172 → 91MB, number-bullet 23 → 21MB). 글꼴 DB를 만드는 단계가 약 10ms 늘어 작은 문서의 첫 쪽 PNG가 10~15ms 느려졌습니다. deflate 수준 1은 PDF를 약 3% 키웁니다.
- **Verified by** `node scripts/run-quicklook-tests.mjs`, `node scripts/bench-run.mjs --runs 5 --suite quicklook`.

## PD-166. 종이 위 컨트롤 색 고정, 포커스 링·도구 단추 hover 통일, 색 선택기 이식

- **Status** Implemented (`ui/content.css`, `ui/styles.css`, `ui/shell.css`, `ui/toolbar.css`, `ui/color-picker.js`, `ui/custom-select.js`)
- **Context** 어두운 테마에서 종이 위의 컨트롤이 깨졌습니다. PDF 카드 안 입력은 `--ui-surface`(어두운 면)에 `--ui-text`(어두운 글자)가 겹쳐 대비가 1.0:1이었고, `.pdf-stamp`는 흰 종이 위에서 다크 값 `--accent-strong`(#a9c3ff)이라 1.8:1이었습니다. 종이 규칙이 `--ui-text` 몇 개만 다시 선언했고, `--color-*` 별칭은 `:root`에서 이미 풀려 있어 다시 선언하지 않으면 다크 값이 그대로 내려갔습니다. 그 밖에 포커스 링이 두 가지(2px 바깥, 2px 간격)였고, 도구 단추 hover가 글자색까지 바꿨고, 스타일 막대의 색 입력이 OS 기본 `<input type="color">`로 보였습니다.
- **Decision (`PD-166`)**
  1. **종이 토큰**: `#doc-editor`, `.slide-stage`, `.pdf-page-card`는 `content.css`의 한 규칙에서 라이트 `--ui-*`·`--accent-*`·`--doc-*`·`--focus-ring`과 `--color-*`·`--border-primary`·`--accent`·`--accent-ink`·`--ui-chrome` 별칭을 모두 다시 선언합니다. 종이 안쪽 규칙(입력, 스탬프, 수식 결과, 슬라이드 영역)은 토큰만 읽으므로 코드가 더 필요 없습니다. 테스트가 각 값이 라이트 토큰과 같은지, 라이트의 색 토큰·별칭이 하나도 빠지지 않았는지 검사합니다(새 별칭이 생기면 실패).
  2. **포커스 링**: 버튼·입력·선택·편집 영역은 rhwp-studio `.sb-btn:focus-visible`처럼 `--color-focus-border` 1px 윤곽을 안쪽(`outline-offset: -2px`)에 그립니다. `shell.css`의 2px 바깥 링은 없앴습니다. 메뉴 항목·슬라이드 썸네일·홈 화면·상태줄 슬라이더는 각자의 규칙이 더 구체적이라 그대로입니다(참조 앱 값).
  3. **도구 단추 hover**: `.tb-btn:hover/.active/[aria-pressed]`는 바탕과 테두리만 바꾸고 글자색은 바꾸지 않습니다(`toolbar.css`, 참조 앱과 같음).
  4. **대화상자 제목 줄**: `cursor: move`인 `.dialog-title`는 모두 끌 수 있습니다(도구 대화상자는 `shell-chrome.js`, 최근 문서 대화상자는 자체 끌기). 바꾼 것은 없습니다.
  5. **색 선택기**: 스타일 막대의 글자색·형광펜 선택을 보이는 네이티브 입력 대신 팔레트 팝업(`ui/color-picker.js`, 참조 앱 `color-picker.ts`·`toolbar-popup.ts`를 JavaScript로 이식, 출처 헤더·고지)으로 바꿨습니다. 글자 모양 단추는 마지막 색을 적용하고 옆 `▾` 단추가 팔레트(회색 한 줄 + 6x7 색, 글자색은 최근에 쓴 색 7개, 형광펜은 "색 없음", "다른 색..."은 숨긴 네이티브 입력을 엽니다)를 엽니다. 팝업은 한 번에 하나이고, 바깥 누름·포커스 이동·Esc로 닫히며 Esc는 단추로 포커스를 돌려주고, 단추에서 ArrowDown은 열고 첫 견본으로 이동합니다. 최근 색은 `localStorage`의 `easyoffice.recent-text-colors`이며 저장소를 쓸 수 없어도 메모리에서 동작합니다. 참조 앱의 "문서 버전 보호"는 창 하나에 문서 하나(PD-088)라서 옮기지 않았습니다.
  6. **콤보 상자 대상**: `custom-select.js`는 `select.sb-combo` 외에 참조 앱처럼 `select.sb-ls-select`, `select.dialog-select`, `select.formula-select`도 꾸밉니다.
  7. **기준과 다르게 유지하는 값, 기준에 맞춘 값**: (a) 기본 버튼은 참조 앱과 같게 `--color-primary` 바탕, hover `--accent-light`, 글자 `--ui-text-on-accent`입니다(PD-169로 `--accent-strong` 예외를 없앰). (b) 홈 최근 문서 경로 글자는 `--color-text-muted`(참조 앱의 placeholder 회색은 2.8:1), 선택 행 바탕은 `--color-accent-bg`, 최근 문서 대화상자는 폭 520px. 이유는 `home.css` 머리에 있습니다. (c) 글리프 `▾`(콤보 화살표, 색 선택 단추)는 참조 앱과 같아 유지합니다. (d) 체크·라디오 메뉴 항목은 참조 앱처럼 글리프(`✓`) 없이 `aria-checked="true"`에 `.md-item.active`와 같은 바탕·글자색으로 표시합니다(처음에는 `✓`를 두었다가 기준에 맞춰 바꿈). 업데이트 확인 알림 토글도 같습니다.
- **Why** 종이는 테마와 무관하게 흰 바탕이라 그 안의 모든 색 토큰이 라이트여야 하고, 별칭까지 고정해야 빠지는 곳이 없습니다. 포커스·hover·선택기는 참조 앱과 같은 모양이어야 두 앱을 오가도 어색하지 않습니다.
- **Trade-offs** 색 선택이 한 단계(팝업 열기) 늘어납니다. 대신 OS마다 다른 네이티브 선택기를 최근 색·팔레트로 바로 고를 수 있습니다. 종이 안에서는 다크 테마의 `--ui-border` 같은 값을 쓰지 못합니다(의도).
- **Alternatives rejected** 종이 안 규칙마다 라이트 값 지정(누락이 생김), 네이티브 입력 유지(OS별 모양, 최근 색 없음), 색 선택기를 새로 설계(구현 원칙 1단계: 참조 구현 이식).
- **Verified by** `node --test scripts/test-webui-theme.mjs scripts/test-webui-color-picker.mjs scripts/test-webui-slide-chrome.mjs scripts/test-webui-shell-chrome.mjs`(종이 팔레트 일치·누락 없음, 포커스·hover 규칙, 팔레트 선택, 최근 색, 키보드, 바깥 누름), `node scripts/check-repo-hygiene.mjs`.
- **Revisit when** 종이로 그리는 영역이 늘 때(시트 인쇄 미리보기 등), 참조 앱이 포커스 링이나 색 선택기를 바꿀 때.

## PD-167. 문서→PDF는 셸에서 HWPX 쪽 배치로 그리고, 엔진 PDF는 대체 경로로 둔다

- **Status** Implemented (`doc_pdf.rs`, `doc_pdf_marks.rs`, `main.rs` `ipc_dispatch`, `pdf_export.rs` `set_rotation`, `convert_structured.rs` `encode_target`, `lib.rs` `dispatch_with_pdf_renderer`, `hwpx_parts.rs`)
- **Revises PD-159 (4항)**: 셸에서 문서가 PDF로 바뀔 때 본문 줄을 엔진이 A4 평문으로 쓰던 방식을 새 경로가 대신합니다. 엔진 PDF 작성기와 그 쪽 설정은 그대로 남아 대체 경로가 됩니다.
- **Context** 변환기와 다른 이름으로 저장의 PDF 대상은 엔진 `pdf_writer.rs`를 거쳐 서식·표·그림이 모두 사라졌습니다. 셸에는 rhwp 레이아웃이 그린 쪽 SVG를 글꼴이 든 PDF로 바꾸는 경로가 이미 있고(PD-101·114·163), 엔진은 리치 문서를 HWPX로 씁니다.
- **Decision (`PD-167`)**
  1. **대상**: `convert_document_format`의 PDF 대상 중 내용이 문서이고 원본 형식이 PDF가 아닌 요청(구조화 저장, 원본 파일 변환)입니다. 시트·슬라이드는 엔진 PDF를 그대로 씁니다. 원본이 PDF인 요청은 PDF 스튜디오의 쪽 설정 저장이어서 엔진이 쓴 `[WM]`·`[STAMP]` 표지를 스튜디오가 다시 읽으므로 엔진 PDF를 유지합니다.
  2. **경로**: 엔진이 문서를 HWPX로 쓰고 → 셸이 rhwp로 쪽 배치 → 쪽 SVG를 16쪽씩 `PdfAssembler`에 넘깁니다. 배치·SVG 그리기는 32 MiB 스택 스레드 하나에서, 변환은 호출 스레드에서 돌아 서로 겹칩니다. 글꼴은 기존 번들·시스템 글꼴 DB와 대체 글꼴 표를 씁니다. 한도는 HWPX 50 MiB, 2,000쪽, 기존 SVG 바이트 상한이고, 넘거나 읽지 못하거나 패닉이 나면 엔진 PDF로 대체하고 응답 `warnings`에 사유를 적습니다("서식을 살린 PDF를 만들지 못해 기본 PDF(글자 중심)로 저장했습니다: …").
  3. **쪽 설정**: 회전은 쪽의 `/Rotate`(내용은 그대로), 워터마크는 모든 쪽에 대각선 회색 글자, 결재 스탬프는 첫 쪽 오른쪽 위 붉은 상자 글자로 쪽 SVG에 그립니다. 모두 글자라 검색·복사가 됩니다. 쪽 크기를 읽지 못해 못 그리면 이 경로를 실패로 보고 엔진 PDF(표지 글자 포함)로 대체합니다. 형광펜·밑줄 문구(PD-161)는 PDF 주석 객체(`/Subtype /Highlight`)와 "문구를 못 찾음" 경고를 엔진이 쓰므로, 이 옵션이 있는 요청은 새 경로를 쓰지 않고 엔진 PDF를 씁니다(쪽 SVG의 글자 위치로 주석을 만드는 것은 이번 범위 밖). 원본 객체 보존 편집(별도 작업)과는 겹치지 않습니다. 실제로 `pdf` 쪽 설정 객체는 PDF 스튜디오 저장만 보내고 그 원본은 PDF라서(1항), 새 경로의 회전·워터마크·스탬프는 다른 클라이언트가 보낸 요청에서만 쓰입니다.
  4. **셸이 없는 경우**: `--serve`는 렌더러가 없어 엔진 PDF를 그대로 쓰고 경고도 붙이지 않습니다(예전 동작과 같음).
  5. **HWPX 작성기**: 목록 문단의 `hh:heading idRef`가 가리키는 번호·글머리표 정의가 머리말에 없어 레이아웃이 표지를 그리지 못했습니다. 목록을 쓰는 문서에 한해 `hh:numberings`(`^1.`)와 `hh:bullets`(`•`)를 한 개씩 쓰고 `idRef="1"`로 가리킵니다.
  6. **남은 손실**(엔진 모델에 없어 이 변경으로 못 고침): 각주·미주 본문, 표 셀 안 글자 서식·배경, 목록 중첩 수준(작성기가 수준을 쓰지 않음), 하이퍼링크 주소, 원본 쪽 크기·여백·머리글·바닥글·쪽 번호(항상 A4, 좌우 30 mm), 도형·차트·수식, 문단 간격, 문서 글꼴(번들 글꼴과 시스템 글꼴로 대체).
- **Why** 같은 변환기 입력이 PDF에서 제목·굵기·색·목록·표·그림·한글/영문 혼합을 살린 채 나옵니다. 새 레이아웃 코드를 짜지 않고 편집기가 인쇄하는 쪽과 같은 경로를 씁니다.
- **Trade-offs** 작은 문서도 글꼴 DB 로드와 레이아웃으로 약 60 ms가 듭니다(엔진 PDF는 1 ms 이하). 큰 문서(428쪽)는 CPU 15초, 최대 RSS 약 380 MB, PDF 크기가 엔진 PDF의 약 3배입니다(쪽마다 글꼴 부분집합 포함, `docs/performance.md` 4.2절). 응답의 `paragraphCount`는 엔진이 이 PDF에서 글을 읽지 못해 0입니다(화면은 쓰지 않음). 렌더러 쪽 오류는 사용자에게 경고로만 보입니다.
- **Verified by** `cargo test -p easyoffice-app --test rich_pdf_renderer`, `cargo test -p easyoffice-formats --test hwpx_list_definitions`, `cargo test -p easyoffice-desktop doc_pdf`(쪽 배치·표·그림·목록 표지·회전·워터마크·스탬프·배치 경계 넘는 긴 문서·응답과 저장 검사까지), 전후 PDF 렌더 비교와 시간·RSS 측정(`docs/performance.md` 4.2절).
- **Revisit when** 엔진 문서 모델이 각주·셀 서식·쪽 설정을 담게 될 때, 쪽마다 글꼴을 다시 넣는 크기를 줄일 때.

## PD-168. OOXML·ODF·HWPX 열기의 XML 읽기를 빌린 슬라이스 풀 리더로 바꾸고 XLSX·ODS는 한 번 훑기로 읽는다
- **Status** Implemented (`xml_stream.rs`, `xlsx_sheet.rs`, `ods_grid.rs`, `rich_read_*.rs`, `sheet_styling/ods_styles.rs`)
- **Context** `docs/performance.md` 4절 병목 10과 열기 프로파일에서, 이벤트마다 `String`을 할당하고 `Vec<XmlEvent>`를 모으는 토크나이저(DOCX·ODT·HWPX), 셀마다 같은 조각을 여러 번 훑는 XLSX 읽기, 셀마다 태그를 다시 찾는 ODS 읽기가 열기 시간과 할당의 큰 몫이었습니다.
- **Decision (`PD-168`)**
  1. **검토 3단계**: (1) 허용 라이선스의 크레이트를 그대로 쓰는 안은 엔진 무의존(PD-053, PD-095) 때문에 제외했습니다. quick-xml(MIT)은 이미 의존성 목록에 있지만 셸·rhwp 쪽입니다. (2) 이미 쓰는 의존성에 같은 기능이 없습니다(엔진은 의존성 0). (3) quick-xml의 풀 리더 설계(빌린 `&str` 이벤트, 빈 요소의 지연 끝 이벤트, 속성 지연 해석, `Cow` 해제)와 calamine의 단일 이벤트 루프(공유 문자열 표를 한 번 만들고 셀마다 작은 상태)를 안전한 Rust로 이식했습니다. 출처는 파일 머리 주석과 `THIRD_PARTY_NOTICES.md`에 있습니다.
  2. **리더**: 이벤트는 입력을 빌리고(`Cow`는 `&`가 있을 때만 새로 만듭니다), 속성은 시작 태그를 읽을 때 한 번 검증한 뒤 찾을 때 해석합니다. 오류 조건과 메시지는 이전 토크나이저와 같고, 변이 입력 전수(잘라내기·바이트 치환)로 이벤트 열이 같음을 시험합니다. 이벤트 예산(600만)은 DOCX·ODT·HWPX에 그대로 두었습니다. 이벤트를 보관하지 않는 XLSX·ODS 걷기는 `unbounded`로 읽습니다(메모리가 깊이에만 비례하고, 시간은 압축 해제 크기 상한이 이미 묶습니다).
  3. **XLSX**: 공유 문자열은 한 덩어리 문자열과 끝 오프셋 표로 보관하고, 셀·수식·인라인 문자열·스타일 색인·열 너비를 시트 하나당 한 번 훑어 읽습니다.
  4. **ODS**: 표·열·행·셀을 한 번 걸으면서 텍스트 싱크(기존 예산·반복 규칙 그대로)와 스타일 싱크가 함께 받습니다.
  5. **동작 차이(의도)**: XLSX·ODS 읽기가 잘못된 XML(따옴표 없는 속성 등)을 이제 거부합니다(이전에는 문자열 검색이라 넘어갔습니다). `<v>` 안 CDATA는 래퍼 없이 내용만 읽습니다. 그 밖에는 전·후 결과가 같습니다.
- **Why** 같은 문서를 같은 결과로 열면서 할당과 시간을 줄입니다. 열기 경로는 모든 문서가 지납니다.
- **Trade-offs** 이전 문자열 검색 구현은 비교 시험용으로 테스트 모듈에만 남겼습니다(`xml_stream/tests.rs`, `xlsx_sheet/tests.rs`, `ods_grid/tests.rs`). 구조가 바뀌면 함께 갱신해야 합니다.
- **Verified by** 위 세 테스트 모듈의 전수·변이 비교, 악의적 입력(깊은 중첩 20만, 8 MiB 속성·값, 잘린 엔티티·주석, 예산 초과) 시험, 열기 응답 11개 입력의 바이트 비교, `docs/performance.md` 4.3절 측정.

## PD-169. 문서·시트 찾기·바꾸기와 문서·시트·슬라이드 인쇄, 단축키 정리

- **Status** Implemented (`ui/find-core.js`, `find-doc.js`, `find-sheet.js`, `find-replace.js`, `office-print.js`, `ribbon-catalog.js`, `app.js`, `index.html`, `crates/easyoffice-desktop/src/native_menu.rs`)
- **Context** 문서·시트에는 찾기가 없었고 `Ctrl+F`가 명령 팔레트를 열었습니다. `Ctrl+P`와 `인쇄…`는 한글 모듈만 인쇄했습니다(`print_webview`는 PD-139로 이미 있음).
- **Decision (`PD-169`)**
  1. **단축키**: `Ctrl+F` 찾기, `Ctrl+F2` 찾아 바꾸기(참조 편집기와 같은 키). 명령 팔레트는 `Ctrl+K`(`/`)만 씁니다. 문서·시트가 아닌 모듈(슬라이드·PDF·변환)은 `Ctrl+F`가 팔레트로 가고, 한글 편집기 창은 편집기 자체 찾기(`edit:find`, `edit:find-replace`)로 보냅니다. `Ctrl+H`·`Ctrl+L`은 묶지 않습니다(macOS ⌘H는 앱 숨기기, 브라우저에서 둘 다 예약 키). `Ctrl+P`는 모듈 제한을 풀고 `print` 동작이 앞에 있는 모듈의 인쇄를 고릅니다(한글은 `hwp-print.js`). macOS 메뉴의 `file:print` 동작 이름은 `hwp-print`에서 `print`로, 편집 메뉴에 `찾아 바꾸기`(`edit:replace`, 키 없음)를 더했습니다. `인쇄…`는 한글 편집기 창과 문서·시트·슬라이드 창에서 켜지고 나머지에서 회색입니다. 페이지 메뉴 줄에 `찾기`·`찾아 바꾸기`·`인쇄…`를 더했습니다.
  2. **대화상자**: 비모달 `<dialog id="find-dialog" class="tool-dialog">`, 구조·간격·크기는 참조 `find-dialog.ts`·`find-dialog.css`와 같게(제목줄, 찾을/바꿀 내용 행, 옵션 행의 `대소문자 구분`과 오른쪽 `n / 전체`, 상태 줄 18px, 이전·다음 줄, 바꾸기·모두 바꾸기 줄). 옵션은 참조처럼 `대소문자 구분` 하나이고 찾을 내용은 늘 글자 그대로입니다(정규식·전체 단어·수식 원문 옵션 없음). 이전/다음은 Enter·Shift+Enter, 한글 조합 중에는 검색하지 않고 조합이 끝나면 검색하며, 찾을 내용은 NFC로 맞춥니다. 찾기 대화상자는 제목줄로 끌 수 있고(`find-dialog.ts:287,546`) 열 때 찾을 내용 입력란에 포커스합니다(`:137`). 키는 열려 있는 동안 document capture로 처리합니다(`:318-357`): Esc 닫기, 단추 위 Enter는 단추가, 그 밖 Enter(조합·수정키 제외)는 다음 일치(Shift는 이전, 바꿀 내용 칸은 바꾸기), 대화상자 안의 다른 키는 전파를 막아 되돌리기 같은 페이지 단축키가 문서에 가지 않습니다.
  3. **색인**: 블록(문단·표 칸·`<br>`로 끝나는 줄, 시트는 셀)마다 `{id, text}`와 일치 목록을 들고, 텍스트 노드의 시작 오프셋 표로 일치를 범위로 바꿉니다. 입력은 해당 텍스트 노드의 문단만 다시 읽고 다시 검색하며(`MutationObserver`), 구조가 바뀌면 트리를 한 번 걷되 바뀌지 않은 문단의 기록과 일치를 재사용합니다. 일치 번호는 접두합과 이분 탐색(O(log 블록))입니다.
  4. **강조**: CSS Custom Highlight API가 있으면 DOM을 바꾸지 않고(현재 일치 주변 ±1,000개 범위만 만듦), 없으면 현재 일치를 선택합니다.
  5. **바꾸기**: 일치가 시작하는 텍스트 노드에 바꿀 글을 넣고 이어지는 노드의 덮인 글만 지워 글자 서식(런)을 유지합니다. 모두 바꾸기는 워크스페이스 스냅샷 한 번(`captureWorkspaceSnapshot`/`commitDeferredUndoSnapshot`)이라 실행 취소 한 단계입니다. 시트는 셀이 보여 주는 값을 찾고, 수식 입력줄과 같은 평가·커밋 경로(`computeEvaluatedSheetStore`, `replaceLiveSheetDataStore`)로 바꾸며 수식 셀은 바꾸지 않고 건너뛴 개수를 알립니다.
  6. **상한(Fail-Closed)**: 찾을 내용 256자, 일치 100,000개(넘으면 `N+`로 표시하고 모두 바꾸기를 거부), 검색 1.5초 예산(블록 사이에서 확인, 넘으면 중단하고 다음 호출이 이어 검색).
  7. **도구 대화상자 공통(`shell-chrome.js`, 참조 `dialog.ts` 95-123·`dialog-drag.ts`·`dialogs.css` 이식)**: 열 때 `.dialog-btn-primary`(없으면 대화상자 자체)에 포커스해 제목줄 닫기 단추에 포커스 링이 생기지 않게 합니다. 열려 있는 동안 document capture `keydown`으로 Esc는 닫고, 입력칸 밖 Enter는 기본 단추를 누르며, 그 밖의 키는 전파를 막아 편집기 단축키(Ctrl+Z 등)가 새지 않게 하고 입력칸 안 타이핑은 막지 않습니다(앱 콤보 상자 `custom-select`는 자체 키 처리를 위해 예외). 백드롭 클릭으로 닫지 않고(`closeOnOverlayClick=false`, `dialog.ts:83`), 끌어 옮긴 위치는 닫아도 유지합니다. 개별 Enter 핸들러(PDF 나누기·병합, 형광펜)는 없앴습니다. CSS는 참조값으로 맞췄습니다: 행 `margin-bottom: 6px`, 라벨 `min-width: 40px`·sm 글자, 입력·선택 칸 높이 22px·`padding: 0 4px`·sm 글자, 체크박스 `.dialog-checkbox`, 비활성 묶음, 제목줄 `user-select` 제거, 기본 버튼 `--color-primary`.
  8. **인쇄**: `office-print.js`가 `#eo-print-root`(한글 인쇄와 같은 루트·정적 규칙)에 문서는 편집기 내용 복제(id·`contenteditable` 제거, A4 여백 20mm, 쪽 나눔은 브라우저), 시트는 사용 범위 표(격자선, 숫자 오른쪽 정렬, 열이 8개를 넘으면 가로, 셀 200,000개 초과는 거부), 슬라이드는 한 장당 한 쪽(덱 비율의 용지)을 쌓습니다. `@page`는 생성형 스타일시트로 더하고, 셸이 있으면 `print_webview`, 없으면 `window.print()`를 부른 뒤 `afterprint`(없으면 5분 뒤)나 실패 시 지웁니다.
- **Measured** Node, 1 MiB 문서(1,032,399자, 문단 13,663개, 인라인 굵게 포함, `apple` 25,619건; 스크래치 `bench-find.mjs`·`bench-dialog.mjs`, DOM은 시험용 가벼운 구현이라 실제 웹뷰 DOM 쓰기는 더 느림): 색인 만들기(트리 걷기+오프셋 표) 14 ms, 새 질의 전체 검색 2~3 ms, 대화상자에서 입력부터 첫 결과 표시 19 ms, 한 문단 입력 뒤 동기화+검색 0.1 ms, 모두 바꾸기 25,619건 본체 8 ms(실행 취소 스냅샷 포함 대화상자 동작 1.1 s), 바꾼 뒤 재검색 5 ms.
- **Why** 입력마다 전체를 훑지 않고 바뀐 문단만 다시 검색하며, 실행 취소는 기존 문서 편집(`withDocStyleUndo`)과 같은 스냅샷 한 번입니다. 일치 계산은 문자열만 다루므로 DOM 없이 시험합니다.
- **Trade-offs** 모두 바꾸기 시간의 대부분은 문서 스냅샷(직렬화)입니다. 시트는 활성 시트만 검색하고, 바꾼 뒤 현재 일치는 다음 일치로 넘어갑니다. 문서·시트가 아닌 모듈의 찾기와 시트의 모든 시트 검색은 없습니다. Univer `docs-find-replace`는 자체 문서 모델 위에 있어 `contenteditable` DOM에 맞지 않아 이식하지 않았습니다.
- **Alternatives rejected** `Ctrl+H`(예약 키), 모달 대화상자(문서를 보면서 편집 못 함), 강조용 `<mark>` 삽입(저장 내용·실행 취소를 오염), 입력마다 전체 텍스트 재색인, 인쇄용 `iframe`(웹뷰별 차이, PD-104와 같은 이유).
- **Verified by** `node --test scripts/test-webui-find-core.mjs scripts/test-webui-find-doc.mjs scripts/test-webui-find-replace.mjs scripts/test-webui-office-print.mjs`(여러 문단·표·런, 한글 조합, 모두 바꾸기 실행 취소, 단축키 표 충돌, 인쇄 루트 생성·정리, 상한), `node --test scripts/test-webui-native-menu.mjs scripts/test-webui-menu-state.mjs`, `cargo test -p easyoffice-desktop native_menu`.
- **Revisit when** 시트 전체(모든 시트) 검색이 필요할 때, 슬라이드 텍스트 찾기를 넣을 때, 쪽 단위 문서 인쇄(편집기 쪽 나눔과 일치)가 필요할 때.

## PD-170. PDF 스튜디오 편집은 원본 PDF의 객체를 보존하는 방식으로, 신뢰할 수 없는 입력은 예산과 마감 안에서만 처리

- **Status** Implemented (`crates/easyoffice-desktop/src/pdf_edit_budget.rs`·`pdf_edit_scan.rs`·`pdf_edit_inspect.rs`·`pdf_edit_build.rs`·`pdf_edit_overlay.rs`·`pdf_edit_session.rs`·`pdf_edit_engine.rs`·`pdf_edit_supervisor.rs`·`pdf_edit_worker.rs`·`pdf_edit_wire.rs`·`pdf_edit_os.rs`·`pdf_edit_job.rs`, `tests/pdf_isolation.rs`, `ui/pdf-edit.js`·`desktop-bridge.js`·`file-open.js`·`ribbon-history.js`·`workspace-tools.js`·`app.js`; 실제 데스크톱 창의 대화상자·화면은 이 환경에서 GUI를 실행하지 못해 확인하지 못했습니다)
- **Context** 회전·분할·병합·스탬프·워터마크가 텍스트만 담은 모델(`PdfDocumentModel`) 위에서 동작해, PDF를 열어 저장하면 원본의 그림·글꼴·배치가 사라졌습니다. 기능은 원본 PDF의 객체를 그대로 옮기는 편집이어야 하고, 파일은 신뢰할 수 없는 입력입니다.
- **Decision (`PD-170`)**
  1. **라이브러리(구현 원칙 3단계)**: (1) `hayro-syntax`(읽기)와 `hayro-write`(쪽과 그 쪽이 쓰는 객체를 새 파일로 복사)를 셸에만 추가합니다(`MIT OR Apache-2.0`, 유지되는 크레이트, 엔진 크레이트는 무의존 유지: PD-053/095). 이미지 디코더 기능은 끄고(`default-features = false, features = ["std"]`) 쪽을 그리지 않고 복사만 하므로 컴파일되는 새 크레이트는 `hayro-syntax`, `hayro-write`, `pdf-writer 0.15`(기존 `pdf-writer 0.12`는 그대로) 셋이고 나머지(`flate2`, `rustc-hash`, `smallvec`, `memchr`, `itoa`, `ryu`, `bitflags`)는 이미 폐포에 있었습니다. `Cargo.lock`에는 이 셋에 더해 선택 의존성 `hayro-ccitt`·`hayro-jbig2`·`hayro-jpeg2000`이 적히지만 컴파일되지 않습니다(`cargo tree`로 확인). 대안 `lopdf 0.47`(MIT)은 분할·회전·증분 저장(`IncrementalDocument`)·읽기 한도(`max_decompressed_size`, 중첩 100단계)를 갖추고 문서 전체(주석·링크·양식·개요)를 보존하는 장점이 있으나, 전체 개체를 즉시 읽고 병합은 번호 재부여를 직접 해야 하며 폐포가 기본 기능을 꺼도 56개 크레이트(암호·난수·`nom`·`brotli`)로 `hayro-write`의 15개보다 크고 셸에 새로 컴파일되는 크레이트가 훨씬 많습니다. QA가 정한 방향(hayro)을 따르되, 주석·링크·양식 보존이 필요해지면 다시 검토합니다. (2) 셸이 이미 쓰는 의존성(`pdf-writer`, `svg2pdf`)에는 PDF를 객체 단위로 읽어 옮기는 기능이 없습니다. (3) 포팅한 코드는 없습니다.
  2. **편집 모델**: 세션(`pdf_edit_session.rs`)은 열린 PDF 바이트를 고치지 않고 결과의 쪽 목록(어느 파일의 몇 쪽, 얼마나 더 돌렸는가)과 스탬프·워터마크만 바꿉니다. 저장할 때 `pdf_edit_build.rs`가 목록에서 새 PDF를 씁니다. 쪽은 `hayro-write`의 "폼 XObject로 복사"로 옮기고(쪽 사전을 닫아 두는 "쪽으로 복사"는 `/Rotate`를 바꿀 수 없음) 우리가 쓴 쪽 위에 놓습니다. 쪽의 미디어 박스는 원본의 보이는 크기, `/Rotate`는 원본 값 + 요청한 회전이며, 복사본에 박혀 있는 원본 회전은 그 역변환으로 되돌려 그립니다. 그래서 돌려 있던 쪽은 같은 방식으로 돌려 있고 회전 요청은 `/Rotate`만 바꿉니다. 같은 파일의 쪽이 공유하는 글꼴·그림은 한 번만 복사하고(파일마다 한 번의 `extract` 호출), 내용·글꼴·그림 바이트는 그대로입니다(쪽 내용 스트림만 라이브러리가 풀었다 다시 압축). 제목·저자·주제·키워드는 첫 쪽의 파일에서 바이트 그대로 옮깁니다.
  3. **스탬프·워터마크**: 글자를 글자로 쓴 SVG를 기존 검색 가능 PDF 내보내기(`PdfAssembler`: 번들 Noto Sans KR + 시스템 글꼴, 글꼴 부분집합과 `ToUnicode`)로 한 쪽짜리 PDF로 만들고, 그 쪽을 폼으로 복사해 원본 위에 겹칩니다(새 글꼴 처리 코드 없음). 워터마크는 모든 쪽에 대각선 회색 투명, 스탬프는 첫 쪽 오른쪽 위의 붉은 테두리 상자입니다. 돌려 보이는 쪽에서도 똑바로 서도록 `/Rotate`를 되돌리는 행렬을 씁니다(네 방향 모두 시험). 보이는 크기가 같은 쪽은 오버레이 폼 하나를 함께 씁니다(크기 종류 128개 한도).
  4. **보안: 별도 프로세스와 예산(Fail-Closed)**: 읽기 라이브러리는 시간 제한·메모리 보호를 갖추지 않았고(README의 남은 과제), 개수만 세어서는 막을 수 없는 파일이 실제로 있습니다(2MB 파일이 쪽마다 공유 리소스를 다시 읽어 메모리를 25GB까지 쓰게 하고, 0.5MB 파일이 복사 때 800MB 청크를 만들고, 압축 해제 한도가 없는 객체 스트림이 1000배로 부풀음 — 보안 리뷰에서 실측). 그래서 **PDF를 읽고 쓰는 모든 일은 같은 셸 바이너리의 숨은 하위 명령(`EasyOffice --pdf-edit-worker`, `pdf_edit_worker.rs`)인 자식 프로세스에서** 합니다. 자식은 창·IPC·플러그인 없이 시작해 표준 입력으로 바이트(PDF, 쪽 계획)만 받고 표준 출력으로 바이트만 돌려줍니다(`pdf_edit_wire.rs`의 프레임). 경로는 받지 않고 글꼴도 없으므로 스탬프·워터마크 폼은 셸이 만들어 바이트로 보내 줍니다. 셸 쪽 감시자(`pdf_edit_supervisor.rs`)가 **10ms마다 자식의 상주 메모리를 재서 1.5GiB를 넘거나(macOS `proc_pid_rusage`의 `ri_phys_footprint`로 압축된 메모리까지 셈, Linux `/proc/<pid>/status`의 `VmRSS`, Windows `GetProcessMemoryInfo`의 작업 집합) 60초 마감을 넘으면 즉시 kill**하고 고정 문장(메모리 한도·시간 한도·비정상 종료)만 돌려줍니다. 자식도 스스로 같은 한도를 보고(마감 + 5초, 메모리) 종료 코드로 사유를 알리며, Linux는 주소 공간 한도(8GiB, `RLIMIT_AS`), Windows는 작업 개체(job object, 프로세스 메모리 한도 + 셸이 사라지면 종료)로 커널이 강제하며, 작업 개체에 넣지 못하면 자식을 죽이고 요청을 거부합니다(한도 없이 돌리지 않음). Windows 코드는 이 환경에서 컴파일하지 못해 CI의 Windows 작업이 첫 확인입니다. 자식이 셸로 보내는 프레임은 태그마다 길이 상한이 있고(오버레이 요청 9바이트, 오류 문장 4KiB, 결과만 큼, 모르는 태그는 거부) 셸은 한 번에 한 프레임만 받으며(`sync_channel(1)`), 오버레이를 만드는 셸 코드가 패닉해도 자식을 죽이고 기다립니다. 병합할 파일은 읽는 동안 누적 바이트로 세션 합계 한도를 검사합니다. 동시에 2개까지(`MAX_ACTIVE_JOBS`), 초과 요청은 "이전 작업이 끝나지 않았습니다"로 거부합니다. kill된 자식은 자리를 바로 돌려주므로 마감을 넘긴 스레드가 남는 문제가 없습니다. 자식 안에서는 다음 예산을 한 번 더 세어 넘으면 거부합니다: 입력 크기 64MiB(페이지→셸 전송은 10MiB), 함께 편집하는 합계 256MiB·파일 64개, 개체 1,000,000, 쪽 5,000, 쪽 트리 깊이 64·노드 100,000(반복 스택, 순환은 1회만 방문), 쪽 내용 해제 크기 쪽당 64MiB·합계 512MiB, 쪽 하나의 내용 스트림 10,000개, **선택한 쪽들의 리소스 항목 수 합 4,000,000**(라이브러리는 쪽마다 항목을 모두 다시 쓰므로 복사 전에 결과 크기를 가늠함: 항목당 약 18바이트), 파일 전체 압축 해제 크기 스트림당 256MiB·합계 1GiB, 결과 256MiB, 분할 파일 200개·합계 1GiB, 오버레이 글자 100자·크기 종류 128개. 쪽 내용은 라이브러리가 메모리로 풀어 쓰므로 풀기 전에 스트리밍 인플레이터로 크기만 세고(할당 없음), 압축이 없거나 `FlateDecode` 하나인 것만 받아들입니다. **압축 폭탄 사전 검사**(`pdf_edit_scan.rs`)는 `stream` 키워드 뒤 데이터가 zlib 머리로 시작하면 사전과 무관하게 끝까지(`endstream`으로 자르지 않고) 세고, 사전이 Flate를 말하면 raw deflate도 세며, 사전이 ASCII85·ASCIIHex·LZW·RunLength를 쓰거나 `#` 이스케이프·약어로 필터 이름을 쓰거나 `/Filter`가 간접 참조이면 파일을 거부합니다. 이 검사는 예의일 뿐 보증이 아닙니다(사전을 파서 없이 읽으므로 속일 수 있음). 막는 것은 프로세스 한도입니다. **암호화**는 아무도 모르는 암호로 열어 보는 것으로 판정합니다(암호화 파일은 열리지 않고 아닌 파일은 암호를 무시하므로 라이브러리의 트레일러 해석 결과이며 `/Enc#72ypt` 같은 표기로 숨길 수 없음). 빈 암호로 열리는 파일도 보호가 조용히 사라지므로 거부하고 "암호를 해제한 사본을 만든 뒤 다시 열어 주세요"를 안내합니다.
  5. **명령(앱 명령 27 → 31)**: `pdf_edit_open(file_name, base64)`(열린 PDF를 셸에 한 번 넘김), `pdf_edit_apply(op)`(`turn`·`rotate`·`overlay`, 알 수 없는 종류·필드는 직렬화에서 거부), `pdf_edit_split_merge(mode, ranges)`(`mode`는 `merge`·`range`·`each`, 병합은 다중 선택 열기 대화상자, 분할은 폴더 선택 대화상자, 범위 문자열은 대화상자보다 먼저 읽어 틀리면 거부), `pdf_edit_save()`(저장 대화상자 뒤 원자적 저장). 모두 `ShellCaller` 필수, capability·`APP_COMMANDS`·구조 테스트(`ShellCaller` 인자, 경로 유출 가드)가 1:1입니다. 페이지는 경로를 보지 못하고(PD-088) 이름·쪽 수·문장만 받습니다. 저장은 기존 `SaveRequest`(다음에 여는 엔진이 받아들이는지 검사)와 `write_document_bytes`(임시 파일, `sync_all`, rename)를 씁니다. 분할은 같은 이름의 파일이 하나라도 있으면 아무것도 쓰지 않고, 도중에 실패하면 쓴 파일을 지우며, 세션은 바꾸지 않습니다(병합만 세션을 바꿈). 창이 닫히면 세션을 버리고, 열기가 거부되면 이전 파일의 세션도 버립니다. 세션 버전은 모든 창이 하나의 전역 카운터에서 받아 재사용되지 않으므로, 오래 걸리는 작업이 시작한 뒤 세션이 바뀌었으면(교체·삭제 뒤 새로 열린 경우 포함) 결과를 덮어쓰지 않습니다.
  6. **화면 연결**: PDF를 열면 `desktop-bridge.js`의 `pdfEdit.open`이 바이트를 한 번 넘기고 상태줄에 결과("원본 PDF 편집 사용 (n쪽 …)" 또는 거부 사유와 "읽어 온 텍스트만 편집합니다")를 보입니다. 거부되거나 셸이 없으면(`--serve`) 기존 텍스트 모델 경로가 그대로입니다. 회전 단추·`페이지 90° 회전`은 기존 동작이고, 저장 직전에 `pdf-edit.js`가 화면의 회전 값(`turn`)과 적용된 스탬프·워터마크(`#pdf-stamp-badge`, `#pdf-watermark-layer`)를 절대값으로 셸에 보내므로 실행 취소·다시 실행도 파일에 반영됩니다. `양식·스탬프 적용`은 "저장할 때 원본 PDF 위에 겹쳐 씁니다"를 알립니다. `페이지 분할·병합` 메뉴는 도구 대화상자(`#pdf-splitmerge-dialog`, 기존 `tool-dialog`/`dialog-wrap`/`dialog-title`/`dialog-footer` 틀과 `dialog-row`·`dialog-select`·`dialog-input`, 참조 앱 rhwp-studio의 `dialogs.css` 규칙을 같은 선택자·값으로 옮김)를 열어 [다른 PDF 병합 / 쪽 범위로 분할 / 쪽별로 분할]을 고르고 범위(`1-3, 5, 7-`)를 받습니다(범위 분할일 때만 입력칸이 켜지고, 비어 있으면 대화상자가 열린 채 안내). 확인·취소·닫기 단추·Esc가 이 선택을 끝내며 취소는 파일에 아무것도 하지 않습니다. 결과 문장은 배지에 씁니다(병합은 실행 취소로 되돌릴 수 없음을 함께 알림). 저장은 원본이 열려 있으면 `pdf_edit_save`로, 아니면 기존 경로로 갑니다. 이전의 네이티브 3버튼 대화상자는 이 대화상자로 대체했습니다.
  7. **보존하지 않는 것**: 쪽만 복사하므로 주석·링크, 양식 필드(AcroForm), 개요(북마크), 구조 트리, 메타데이터(위 네 항목 제외)는 결과에 없습니다. 편집한 결과를 다시 편집하면 쪽 폼이 한 겹 더 쌓입니다(내용은 보존). 쪽 내용 미리보기(텍스트 카드)는 병합·분할 뒤에도 갱신하지 않습니다.
- **Why** 편집이 파일의 객체를 옮기면 그림·글꼴·배치가 남고, 모델 없이 계획만 바꾸면 편집을 되돌려도 원본이 손상되지 않습니다. 신뢰할 수 없는 파일의 크기·구조·압축·시간은 라이브러리에 맡기지 않고 한도를 앞에 둡니다.
- **Trade-offs** (a) 자식 프로세스는 한 요청마다 시작합니다(셸 바이너리의 시작 시간만큼 걸리고, 분할은 파일마다 한 번씩 시작). (b) 메모리 감시는 10ms 간격이라 그 사이에 늘어나는 만큼은 한도를 넘을 수 있습니다(자식의 자체 감시와 Linux·Windows의 커널 한도가 겹침). macOS에는 커널이 강제하는 한도가 없습니다. (c) 쪽 내용이 LZW·ASCII85 등이면 편집할 수 없고, 사전 검사가 거부하는 필터를 쓰는 오래된 PDF도 같습니다. (d) 주석·링크·양식·개요가 사라집니다(위 7).
- **Alternatives rejected** `lopdf`(위 1), 쪽 사전을 닫아 두는 "쪽으로 복사"(회전 값을 바꿀 수 없고 오버레이를 둘 수 없음), 오버레이 글자를 PDF 연산자로 직접 쓰기(글꼴 부분집합·`ToUnicode`·한글 처리를 다시 구현), 같은 프로세스의 전용 스레드 + 마감(스레드를 죽일 수 없고 할당 폭주는 중단이라 잡을 수 없음: 첫 구현의 한계로 리뷰에서 실측됨), 전역 할당자로 메모리 한도(할당 실패는 중단이라 잡을 수 없음), 별도 바이너리(Tauri 번들에 같이 담기지 않음).
- **Verified by** `cargo test -p easyoffice-desktop`: 단위 시험 `pdf_edit_*`(회전 `/Rotate` = 원본 + 요청, 분할·병합의 이미지 바이트 보존, 오버레이, 100쪽 병합, 악의적 파일 — 순환·깊은 쪽 트리·20만 단계 중첩·잘린 xref·바이트 손상 64종·압축 폭탄이 사전을 숨기거나 `endstream`을 품은 경우·암호화 표기 우회·리소스 항목 수·내용 배열 길이, 프레임·계획 직렬화, 세션·op·분할·롤백·경로 비노출·명령 목록과 capability)와 통합 시험 `tests/pdf_isolation.rs`(실제 바이너리를 자식으로 띄워: 정상 검사·병합·오버레이 왕복, 5,000쪽 × 항목 100,000 공유 리소스(2MB)와 검사를 빠져나가는 객체 스트림 폭탄이 메모리 한도에서 kill되고 같은 프로세스가 다음 요청을 처리함, 마감에서 kill, 자리 반납과 동시 한도, 없는 실행 파일, 2,000쪽 × 5,000 항목 공유 사전은 복사 전에 거부, 내용 배열 20만 항목은 길이로 거부), `node --test scripts/test-webui-pdf-edit.mjs scripts/test-webui-hwp-shell.mjs`(분할·병합 대화상자: 열림과 기본 선택, 범위 입력칸의 켜짐·꺼짐, 빈 범위 거부, 보낸 `mode`·`ranges`, 쪽별 분할은 범위를 보내지 않음, 취소·닫기·Esc는 셸을 부르지 않음, 셸의 취소·거부 문장).
- **Revisit when** 주석·링크·양식·개요 보존이 필요할 때(`lopdf` 재검토 또는 카탈로그 객체 복사), 객체 스트림 같은 폭탄을 더 일찍 알려 주고 싶을 때(라이브러리가 읽기 한도를 갖추면 사전 검사를 줄일 수 있음), macOS에서 커널이 강제하는 메모리 한도가 필요할 때, 시작 시간이 문제가 될 때(자식을 재사용하는 풀).

## PD-172. 시트 조건·이진 검색의 표준 동작과 저장 때 공백·빈 문단 보존

- **Status** Implemented (`crates/easyoffice-sheet/src/{criteria,functions_lookup,workbook,functions_eng}.rs`, `recalc/mod.rs`, `ui/save-payload.js`, `ui/sheet-store.js`)
- **Context** 조건 문자열을 앞뒤로 잘라 `" "` 조건이 공백 셀과 맞지 않았고, 이진 검색 조회(XLOOKUP·XMATCH `search_mode` ±2)는 오류 셀에서 방향과 무관하게 오른쪽으로 갔고 중복 키의 일치 위치가 정해져 있지 않았습니다. 저장 페이로드는 빈 문단과 문단 안 줄바꿈을 쪼개거나 버렸고 셀 텍스트의 앞뒤 공백을 지웠습니다.
- **Decision (`PD-172`)** (1) 텍스트 조건(`COUNTIF`류)은 앞뒤 공백을 지우지 않습니다. 숫자·날짜로 읽을 때만 공백을 무시합니다. (2) 이진 검색은 오류 셀을 정렬 순서에 넣지 않고 가장 가까운 비오류 셀로 대신 비교하며, 중복 키는 배열에서 먼저 나오는 일치를 돌려줍니다. (3) 정렬이 옮긴 수식은 `Q1!B2`처럼 따옴표 없는 시트 이름의 숫자를 행으로 읽지 않습니다(다른 시트 참조의 상대 행은 `shiftFormula`와 같이 복사 규칙으로 이동). (4) 새 세션의 첫 재계산은 정점 없는 수식(`=5`, `=TRUE`, 파싱 오류)도 보고합니다. (5) 저장은 `<p><br></p>`를 빈 문단으로, 문단·목록 항목 안의 `<br>`를 런 텍스트의 `\n`으로 보내고, 시트 셀 텍스트는 앞뒤 공백을 그대로 보냅니다. (6) 수식 아닌 셀을 수식 입력줄로 평가할 때 그 셀 하나만 무상태 `evaluate_sheet`로 보냅니다(수식은 세션).
- **Why** 입력을 조용히 바꾸지 않고 표준 스프레드시트의 결과와 같게 하며, 큰 시트의 요청이 PD-150 상한과 무관해집니다.
- **Trade-offs** 중복 키를 첫 일치로 좁히려고 이진 검색이 일치 뒤에도 왼쪽 절반을 더 봅니다(여전히 O(log N)). 오류 셀이 많으면 이웃 탐색이 길어질 수 있습니다.
- **Alternatives rejected** 오류 셀을 항상 가장 큰 값으로 취급하기(내림차순 데이터와 어긋남), 정렬 수식 조정에서 다른 시트 참조를 건너뛰기(`shiftFormula`·표준 복사 규칙과 다름).
- **Verified by** `cargo test -p easyoffice-sheet --lib`, `node --test scripts/test-webui-doc-paragraphs.mjs scripts/test-webui-file-open.mjs scripts/test-webui-sheet-session.mjs`.

## PD-173. PDF 리더는 이미지·글꼴 스트림을 풀지 않고, 글자 해독은 쪽의 글꼴 리소스별로 한다

- **Status** Implemented (`pdf_object_graph.rs`, `pdf_opaque_stream.rs`, `text_pdf_codecs.rs`, `pdf_stream_parser.rs`, `pdf_tounicode_map.rs`)
- **Context** 텍스트 추출이 이미지 샘플과 글꼴 프로그램까지 풀어 32 MiB 예산을 썼습니다. 40 MiB로 풀리는 이미지 세 개를 가진 122,969 B PDF가 16 MiB 상한에서 거부됐습니다. `DCTDecode` 이미지는 지원하지 않는 필터로 거부됐습니다. 또 문서 전체의 `ToUnicode`를 맵 하나에 합치고 글꼴 이름이 `/F2`인 글꼴만 UTF-16BE로 읽었습니다.
- **Decision (`PD-173`)**
  1. **불투명 스트림**: 딕셔너리가 `/Subtype /Image`이거나, `/Length1`이 있거나(`FontFile`·`FontFile2`), `/Subtype`이 `Type1C`·`CIDFontType0C`·`OpenType`(`FontFile3`)인 스트림은 풀지 않고 예산에도 넣지 않습니다. 이미지는 선언한 쪽×높이가 2^30 픽셀을 넘으면 `ResourceBudgetExceeded`로 거부하고, 필터가 `FlateDecode` 하나인데 압축 데이터가 선언 크기의 DEFLATE 최대 팽창(1032배)에 못 미치면 `CorruptedContainer`로 거부합니다. `%EasyOfficePDFModelBase64:` 주석은 원본 바이트에서 찾아 있으면 거부합니다(풀지 않으므로 더 엄격한 검사). 콘텐츠 스트림·`ToUnicode`·객체 스트림·메타데이터는 이전처럼 풀고 예산을 씁니다. `/Contents`가 불투명 스트림을 가리키면 거부합니다.
  2. **글꼴별 해독**: 쪽의 `/Resources /Font`(상위 `/Pages`에서 상속) 이름마다 그 글꼴의 `/ToUnicode` 맵 하나를 씁니다. 맵은 글꼴 사전이 가리키는 스트림마다 한 번만 읽습니다. UTF-16BE 우선 읽기는 `/Subtype /Type0`이고 `/Encoding` 이름에 `UCS2`나 `UTF16`이 든 글꼴(`UniKS-UCS2-H` 등)에만 씁니다. 리소스에 없는 이름은 맵 없음·UTF-16BE 아님으로 읽습니다.
- **Why** 정상 스캔·사진 PDF가 열리고, 같은 글자 코드가 글꼴마다 다른 글자로 읽히는 문제가 없어집니다. 이미지는 텍스트 경로가 읽지 않으므로 풀 이유가 없습니다.
- **Trade-offs** 글꼴 리소스가 없는 PDF의 `/F2`는 더 이상 UTF-16BE로 읽히지 않습니다(`BOM`이 붙은 문자열은 그대로 읽힘). 엔진이 쓰는 PDF는 리소스에 `Type0` 글꼴을 쓰므로 영향이 없습니다. `Type0`이어도 `Identity-H`이고 `ToUnicode`가 없으면 글자를 복원할 수 없습니다(이전에도 같음).
- **Verified by** `cargo test -p easyoffice-formats --test pdf_reader_streams_and_fonts`(40 MiB 이미지·글꼴 프로그램·`DCTDecode`가 있는 PDF 열림, 선언 픽셀 초과·압축 부족 거부, 콘텐츠 스트림 예산 유지, 쪽별 `ToUnicode`, `Type0` 판정), `pdf_codecs_parity`, `docs/performance.md` 4.4절.
- **Revisit when** 양식·주석 안의 글자를 읽어야 할 때(`XObject` 폼의 글꼴 리소스), `Identity-H` 글꼴의 `CIDToGIDMap` 해독이 필요할 때.

## PD-174. 원본 HWP·HWPX를 PDF로 바꿀 때 파일 바이트를 그대로 셸에 넘기고, 요청의 쪽 설정을 전달한다

- **Status** Implemented (`convert_structured.rs` `encode_source_target`, `convert_open.rs`, `doc_pdf.rs`)
- **Revises PD-167 (1·2항)**: 원본 파일 변환 요청 중 원본이 HWP·HWPX인 경우에 한합니다.
- **Context** `sourceBase64` 요청은 원본을 엔진 모델로 읽고 A4 HWPX로 다시 써서 셸에 넘겼습니다. 원본의 쪽 크기·여백·머리글·글꼴이 사라졌고, 요청의 `pdf` 객체(회전·워터마크·스탬프·형광펜)는 이 경로에서 읽지 않았습니다.
- **Decision (`PD-174`)**
  1. 원본이 `.hwp`·`.hwpx`이고 대상이 PDF이며 셸 렌더러가 있으면, 원본 바이트를 렌더러(rhwp `DocumentCore::from_bytes`는 둘 다 읽음)에 그대로 넘깁니다. 한도·대체 경로(실패 시 엔진 PDF와 경고)는 PD-167과 같습니다.
  2. 원본 파일 요청도 `pdf` 객체를 읽어 렌더러(모든 원본)와 엔진 PDF에 전달합니다. 형광펜이 있으면 PD-167처럼 엔진 PDF를 씁니다.
  3. 엔진 모델을 거치는 다른 원본(DOCX 등)은 그대로 HWPX로 다시 씁니다. 기관명(`official_agency`)은 모델에 있는 값이 HWPX에 쓰이고, 요청의 `blocks` 저장에는 기관명 필드가 없습니다(화면이 보내지 않음).
- **Why** 원본 한글 문서의 배치가 PDF에 그대로 나옵니다. 모델을 거치며 잃던 서식이 없어집니다.
- **Trade-offs** 원본이 `.hwp`·`.hwpx`일 때 응답의 `paragraphCount`·`tableCount`는 PD-167처럼 0일 수 있습니다.
- **Verified by** `cargo test -p easyoffice-app --test rich_pdf_renderer`(원본 바이트가 같은 채 렌더러에 도착, 쪽 설정 전달, 형광펜은 엔진), `cargo test -p easyoffice-desktop a_native_hangul_source_is_drawn_from_its_own_bytes`(HWPX·HWP 원본이 한 쪽 PDF로 나오고 워터마크가 검색됨).
- **Revisit when** 엔진 HWP 작성기 출력을 rhwp가 일부만 읽는 문제(엔진이 쓴 HWP의 영문 본문이 PDF에 빠짐)를 고칠 때.

## PD-175. 포맷 디코더는 문서 하나가 복원하는 바이트 총량에 예산을 둔다 (256 MiB)

- **Status** Implemented (`decode_budget.rs`, `xlsx_sheet.rs`, `office_codecs.rs`, `ods_grid.rs`, `biff8_xls_codec.rs`, `legacy_ole2_codec.rs`, `hwp5_codec.rs`, `deflate.rs`, `apps/quicklook/rust/src/lib.rs`)
- **Context** PD-168이 XLSX·ODS 읽기에서 이벤트 상한(600만)을 풀면서, 한 단계(요소·레코드·셀)만 재던 한도가 합계를 막지 못하는 경로가 드러났습니다. 공유 문자열 하나를 셀 수십만 개가 가리키면 셀마다 문자열 사본이 생기고, 같은 시트 파트를 가리키는 `<sheet>` 항목이 파트를 항목 수만큼 다시 읽습니다. 작은 파일이 수십 GiB를 복원할 수 있습니다.
- **Decision (`PD-175`)**
  1. **검토 3단계**: (1) 외부 크레이트는 엔진 무의존(PD-053, PD-095) 때문에 쓰지 않습니다. (2) 같은 구조가 이미 Pages 읽기에 있습니다(`pages_iwa.rs`의 문서 단위 `document_budget`, 256 MiB). (3) 그 방식을 `DecodeBudget`(`decode_budget.rs`)으로 일반화해 읽기 경로가 같은 값과 같은 오류(`ResourceBudgetExceeded`)를 씁니다. `pages_iwa::MAX_DOCUMENT_BYTES`는 같은 상수를 가리킵니다.
  2. **예산**: 문서당 256 MiB. 복사하기 전에 복사할 크기를 빼고, 모자라면 열기를 거부합니다(Fail-Closed). 셀은 텍스트 길이에 셀 레코드 크기(32바이트)를 더해 셀 개수도 함께 묶습니다.
  3. **XLSX**: 공유 문자열 참조, 공유 수식 슬레이브가 만드는 수식 사본, 인라인·값 문자열을 시트 전체가 한 예산으로 냅니다. 워크시트 파트는 한 번만 읽고(같은 파트를 가리키는 `<sheet>` 둘 이상은 손상 처리), 시트 항목은 1,024개까지만 받고 쓰기(`pack_xlsx_multi*`)도 같은 상한으로 거부해, 저장한 파일을 다시 못 여는 일이 없게 합니다. 공유 수식 슬레이브는 이동한 결과 길이로 셉니다(`A1`이 `XFD1048576`이 되면 마스터의 3.7배).
  4. **ODS**: 반복 행·열이 만드는 셀 텍스트 사본을 복사 전에 같은 예산에서 뺍니다. 셀 20만 개 상한은 그대로 둡니다(셀 하나가 1 MiB이면 20만 개 상한만으로는 200 GiB까지 갑니다).
  5. **XLS(BIFF8)**: 모든 셀 추가가 셀 텍스트와 셀 레코드 크기를 예산에서 뺍니다. 레코드 하나(14바이트)가 SST의 64 Ki자 문자열을 가리킬 수 있어서, `LABELSST`는 문자열을 복사하기 전에 값을 냅니다.
  6. **MS-DOC**: 조각 표(최대 6만 5,536개)의 조각이 같은 구간을 다시 가리켜도 되므로, 조각마다 글자 수 × 3바이트(UTF-8로 풀었을 때 한 글자의 최대)를 디코딩 전에 뺍니다.
  7. **HWP 5.0**: DocInfo와 모든 BodyText 구역이 해제 바이트를 한 예산으로 냅니다. 해제기는 남은 예산에서 멈춥니다(구역당 128 MiB 한도 안쪽에서). 이전에는 구역마다 128 MiB씩 몇 번이든 풀 수 있었습니다. 압축비 규칙은 두지 않습니다. 참조 앱도 두지 않고, 평평한 그림은 1000:1을 넘기 때문입니다(PD-112의 `ZipSecurityPolicy::hangul_editor`와 같은 판단).
  8. **DEFLATE 출력 버퍼**: ZIP 헤더가 주장하는 크기(최대 512 MiB)를 읽기 전에 통째로 잡던 것을 4 MiB까지로 줄입니다. 주장이 더 크면 실제 출력이 나오는 만큼 4배씩 주장 크기까지만 키웁니다. 주장이 4 MiB 이하이고 맞으면 이전처럼 한 번에 잡습니다. 주장 없는 스트림의 첫 버퍼는 입력의 8배(4 MiB 상한)입니다.
  9. **Quick Look 내장 썸네일**: `eo_ql_extract_embedded_thumbnail`이 `PrvImage`(최대 10 MiB)를 시스템 디코더에 넘기기 전에 헤더의 크기를 `RasterBudget::for_page()`(그림 하나 4096×4096 픽셀, PD-145의 같은 값·같은 헤더 읽기)로 검사합니다. 넘거나 헤더를 읽을 수 없으면 `STATUS_NO_THUMBNAIL`이고, 확장은 기본 아이콘으로 답합니다.
- **Why** 입력 크기로 출력 크기를 묶을 수 없는 읽기는 합계 예산이 유일한 방어선입니다. 한 문서의 복원량이 256 MiB를 넘는 정상 파일은 없습니다(ZIP 합계 한도가 512 MiB이고 셀 하나가 수십 바이트).
- **Trade-offs** (a) 한 문서의 복원량이 256 MiB를 넘는 파일은 열리지 않습니다. 셀은 텍스트에 32바이트를 더해 세므로 셀 수백만 개짜리 시트도 같은 예산을 씁니다. (b) 같은 시트 파트를 가리키는 `<sheet>` 항목이 있는 XLSX는 거부합니다. (c) 평평한 큰 그림(주장 크기가 4 MiB를 넘는 ZIP 항목)은 첫 버퍼를 한 번에 잡지 못해 풀기가 느려집니다. 같은 입력 열 번 중 가장 빠른 값(릴리스 빌드, 부하가 큰 기계): 16 MiB 0.77 ms → 1.46 ms, 64 MiB 3.1 ms → 5.7 ms, 256 MiB 13 ms → 26 ms. 주장 없는 스트림은 같거나 빨라졌습니다(256 MiB 77 ms → 40 ms).
- **열기 시간(전→후, 릴리스 빌드)** XLSX 100만 셀 140 ms → 130 ms, ODS 18만 셀 33.7 ms → 33.6 ms, XLS 6만 5천 셀 1.06~1.10 ms → 1.15~1.31 ms, HWP 5.0 압축 문단 4만 개 6.1~6.3 ms → 6.2~6.5 ms, DOC 조각 2만 개 2.65~2.73 ms → 2.77~2.88 ms. 측정 때 기계 부하가 커서 한 자리 퍼센트 차이는 잡음 안입니다.
- **Verified by** `tests/decode_budget_xlsx.rs`(공유 문자열 300×1 MiB, 공유 수식 슬레이브 300×1 MiB, 두 시트 합산, 같은 파트를 가리키는 시트 둘, 시트 항목 2,000개 거부, 참조가 늘어나는 마스터의 슬레이브 400개 거부, 쓰기 상한, 10만 셀 정상 파일 통과), `tests/decode_budget_ods.rs`(열 반복 300×1 MiB, 행 반복 300×1 MiB 거부, 정상 반복 2만 셀 통과), `tests/decode_budget_ole2.rs`(8,000자 문자열을 가리키는 `LABELSST` 4만 개 거부, 정상 2만 셀 통과, 같은 1 MiB를 가리키는 조각 300개 거부, 정상 조각 표 통과), 같은 `tests/decode_budget_ole2.rs`(100 MiB로 풀리는 구역 4개 거부, 작은 구역 3개 통과), `deflate/tests.rs`(512 MiB 주장·입력 100 MB 첫 버퍼 4 MiB 이하, 주장 크기까지만 성장), Quick Look `tests.rs`(PNG·GIF·BMP 헤더의 100,000² 등 4종과 HWP 5.0 샘플의 IHDR를 고친 파일이 `STATUS_NO_THUMBNAIL`, 정상 64×48 PNG는 그대로 반환), `decode_budget.rs` 단위 시험.

## PD-177. 데스크톱 셸의 열기·저장·종료 경계: 열지 못한 파일은 놓고, 한도는 셸에서 먼저, 저장은 대상 파일에

- **Status** Implemented (`window_commands.rs`, `windows.rs`, `desktop-bridge.js`, `window-bridge.js`, `workspace-tools.js`)
- **Context** 외부 점검 결과(QA 요청)를 코드로 확인했습니다. 열기 경로가 셸에서 먼저 파일을 읽고 경로를 점유한 뒤 페이지가 파싱하는 구조라, 페이지가 열지 못한 파일의 경로가 창에 남았습니다.
- **Decision (`PD-177`)**
  1. **열지 못한 파일 놓기**: 페이지가 대기 열기(`take_pending_open`)로 받은 파일을 보여 주지 못하면(크기 한도, 손상, 암호 입력 취소) 인수 없는 명령 `abandon_tracked_open`을 부릅니다. 셸은 그 창의 추적 파일·경로 점유·대기 바이트를 놓습니다(`AppWindows::abandon_open`). 창은 남고, 같은 파일을 다시 열면 새 창이 열립니다. 페이지는 `loadPending`의 결과가 `ok`가 아닐 때만 부르고, 셸이 아닌 환경에서는 부르지 않습니다. 명령은 `ShellCaller`·`APP_COMMANDS`·capability(`allow-abandon-tracked-open`)에 1:1로 있습니다.
  2. **파일 이름 없는 문서의 저장**: 문서 이름에 확장자가 없으면(새 문서이거나 열기에 실패한 빈 창) 저장은 `inPlace: false`입니다. 저장 대화상자가 뜨고, 저장한 뒤 창이 파일 이름을 받으면 다음 저장부터 그 파일에 덮어씁니다. 이전에는 이름이 비면 `inPlace: true`라, 추적 파일이 남은 빈 창의 Ctrl+S가 원본을 대화상자 없이 덮어쓸 수 있었습니다.
  3. **열기 크기 한도는 셸이 종류별로 먼저 적용합니다**: 한글 편집기 창은 64 MiB, 나머지 문서 창은 10 MiB입니다(`WindowKind::max_open_bytes`, 페이지의 `MAX_HWP_BYTES`·`MAX_FILE_BYTES`와 엔진의 `MAX_SOURCE_BYTES`와 같은 값이며 `scripts/test-file-size-limits.mjs`가 대조). 이전에는 셸이 256 MiB까지 읽어 Base64로 만들고 `takePendingOpen`에서 다시 복사한 뒤에야 페이지가 거부했습니다. 상수 `MAX_OPEN_BYTES`는 없어지고, 파일 지문의 해시 상한은 저장 한도(`MAX_SAVE_BYTES`)를 따릅니다.
  4. **엔진 호출의 페이로드 상한**: `EasyOfficeIpcRouter::dispatch`가 페이로드가 16 MiB(`MAX_IPC_HTTP_BODY_BYTES`, HTTP 서버의 `POST /api/ipc` 본문 상한과 같은 값)를 넘으면 명령을 읽기 전에 `PayloadTooLarge`로 거부합니다. 데스크톱 셸의 `ipc_dispatch`는 HTTP 서버를 거치지 않아 이 상한이 없었고, `detect_format`·`audit_security`는 크기 검사 없이 Base64를 풀었습니다.
  5. **저장은 심볼릭 링크를 따라가 대상 파일에 씁니다**(`write_document_bytes`): 대상이 링크이면 링크가 가리키는 파일의 폴더에 임시 파일을 만들어 원자 교체합니다. 이전에는 임시 파일을 링크 위로 이름 바꿔 링크가 일반 파일로 바뀌고 원래 대상은 그대로였습니다. 가리키는 곳이 없는 링크는 저장 실패이며 링크 뒤에 새 파일을 만들지 않습니다. PDF 내보내기도 같은 함수를 씁니다.
  6. **링크로 연 파일은 링크 이름으로 표시합니다**: 열 경로의 마지막 요소가 심볼릭 링크이면 창 제목·페이지가 읽는 파일 종류·최근 문서에는 링크 이름을 쓰고, 경로 점유(같은 파일 창 찾기)·추적·읽기·저장은 실제 파일(정규 경로)을 씁니다(`OpenTarget`). 링크가 아닌 파일은 이전과 같이 디스크의 이름을 씁니다.
  7. **저장 대화상자에서 바꾼 확장자 확인**: 사용자가 고른 이름의 마지막 확장자가 저장할 형식(`markdown`은 `md`)과 다르거나 없으면, 쓰기 전에 네이티브 확인 창("확장자 확인", 단추 "그대로 저장"/"저장 취소")이 뜹니다. 취소하면 아무것도 쓰지 않고 저장 취소로 돌아갑니다. 문서 저장·PDF 내보내기·PDF 스튜디오 저장이 쓰는 `pick_save_path` 한 곳에서 검사하며, 문구는 파일 이름만 담습니다. 참조 앱의 처리는 로컬 사본이 없어 확인하지 못했습니다.
  8. **끝내기 큐의 정체 판정은 확인 응답을 본다**: 큐가 `CLOSE_ACK_TIMEOUT`(10초) 동안 움직이지 않으면 다음 끝내기 요청이 큐를 새로 시작하는 규칙(PD-119)은, 큐 맨 앞 창의 페이지가 저장 확인을 `ack_close_prompt`로 확인했으면 적용하지 않습니다. 그 창은 멈춘 것이 아니라 사용자의 답을 기다리는 중이며, 큐를 다시 만들면 다른 창에 두 번째 확인이 뜹니다.
  9. **실행 중인 빌드의 버전 비교는 semver 순서를 따릅니다**: `1.2.0-rc.1` 같은 사전 릴리스 빌드(릴리스 워크플로가 받는 형식)는 같은 번호의 정식 릴리스보다 오래된 것이고 앞 번호의 릴리스보다는 새것입니다(`+빌드` 메타데이터는 순서에 영향이 없음). 이전에는 접미사가 있는 버전 글자를 읽지 못해 `0.0.0`으로 취급했고, 그래서 이미 지난 릴리스까지 모두 새 소식으로 알렸습니다. 릴리스 태그는 이전처럼 정식 `x.y.z`만 받습니다.
- **Why** 열기에 실패한 빈 창이 원본을 덮어쓰면 원본 데이터를 잃습니다. 셸이 읽은 파일을 페이지가 거부하는 경우는 한도, 손상, 암호 취소로 여럿이므로, 거부 사유와 상관없이 이 길이 닫혀 있어야 합니다.
- **Verified by** `cargo test -p easyoffice-desktop windows window_commands`(경로 해제 후 다른 창이 점유, 포기한 열기가 추적·대기·점유를 놓음, 창 명령 목록), `node --test scripts/test-webui-windows.mjs scripts/test-webui-desktop-bridge.mjs`(11 MiB DOCX와 깨진 DOCX가 열리지 않으면 `abandon_tracked_open` 한 번, 이름 없는 문서의 저장은 `inPlace: false`), `cargo test -p easyoffice-desktop window_commands`(종류별 열기 한도 경계), `node --test scripts/test-file-size-limits.mjs`(셸·페이지·엔진 한도 일치), `cargo test -p easyoffice-app a_payload_over`(상한 초과 페이로드는 모든 명령이 거부하고 상한 크기는 명령까지 도달).
