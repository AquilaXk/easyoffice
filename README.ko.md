# EasyOffice

[English](README.md) | [한국어](README.ko.md)

EasyOffice (`v1.0.0`)는 Swift 6, SwiftUI, AppKit 기반으로 설계된 **100% 독립 실행형(Standalone)·서버리스(Serverless)·오프라인 지원 macOS 네이티브 올인원 오피스 스위트**입니다. 시그니처 라벤더 디자인 시스템(`#5C6BC0`)을 바탕으로 **워드 문서(Document)**, **스프레드시트(Sheet)**, **프레젠테이션(Slide)**, **한글 문서(HWP / HWPX)**, **PDF 스튜디오(PDF Studio)**를 단일 멀티 탭 윈도우에서 통합 제공하며, 비트 수준 컨테이너 코덱과 결정론적 수식 연산, 온디바이스 스마트 문서 보조 도구, Fail-Closed 보안 가드를 내장합니다.

---

## 핵심 특징

- **5대 오피스 모듈 올인원 통합 워크스페이스 (`WorkspaceViewModel`)**:
  - 문서 작성, 한글(HWP 5.0/HWPX) 공문서 편집, 표 계산 및 피벗 분석, 슬라이드 발표자료 제작, PDF 주석 및 변환을 하나의 네이티브 macOS 앱에서 탭으로 전환하며 수행합니다.
  - 두 문서를 좌우로 나란히 비교·편집하는 **분할 보기(Split View, `splitSecondaryTabID`, 최소 너비 `420pt`)**, **접이식 7대 카테고리 컨텍스트 리본(`Cmd+Option+R`)**, **좌측 구조 내비게이터**, **우측 세부 속성 인스펙터**, **5대 인터랙티브 에디터 상세 패널(`DocumentEditorDetailPaneView`, `SheetEditorDetailPaneView`, `SlideEditorDetailPaneView`, `PDFStudioEditorDetailPaneView`)**을 지원합니다.
- **20종 문서·템플릿 포맷 바이너리 라운드트립 코덱**:
  - **ZIP 기반 컨테이너 (`ZipContainerCodec`)**: `OOXMLContainerCodec` (`.docx`, `.xlsx`, `.pptx`, `.ppsx`), `ODFContainerCodec` (`.odt`, `.ods`, `.odp`), `HWPXContainerCodec` (`.hwpx`, `.hwtx` KS X 6101 OWPML 표준).
  - **OLE2 CFBF 및 HWP 5.0 바이너리 (`OLE2ContainerCodec`, `HWP5BinaryCodec`)**: 512바이트 섹터 할당 테이블(FAT) 및 디렉터리 스트림 파서/빌더, 32비트 비트패킹 `HWP5RecordHeader` (`10-bit tagID | 10-bit level | 12-bit size` + `0xFFF` 확장 길이 헤더) 기반 `.hwp`, `.hwt`, `.doc`, `.xls`, `.ppt` 지원.
  - **PDF 및 정형/마크업 코덱**: `PDFBinarySerializer` (`.pdf` ISO 32000 상호참조 테이블 `xref` 및 트레일러 직렬화/파싱), `CSVStateMachineCodec` (`.csv`, `.tsv` RFC 4180 상태 머신), `CommonMarkCodec` (`.md`), `RTFCodec` (`.rtf` 부호 있는 16비트 `\uN?` 유니코드 이스케이프), UTF-8 평문(`.txt`).
- **비트 수준 성능 최적화 (Bit-Level Optimizations)**:
  - `BitPacking` (리틀 엔디언 `UInt16`/`UInt32`/`UInt64` 리더·라이터 및 Zig-Zag Varint 인코딩), 256-엔트리 룩업 테이블 기반 IEEE 802.3 `CRC32Calculator`, `HWP5RecordHeader` 비트필드 연산, 문자열 할당 없는 $O(1)$ 셀 조회 및 순환 참조 검출을 위한 64비트 `PackedCellCoordinate` (`UInt64`: 상위 32비트 `row`, 하위 32비트 `column`).
- **5대 결정론적 에디터 엔진**:
  - **Document & HWP/HWPX (`OfficeDocumentModel`)**: 다중 구역 조판(`PaperSizePreset`, `PageOrientation`), 문단/글자 스타일, 셀 병합 및 인라인 표 수식(`=SUM(ABOVE|LEFT)`, `=AVERAGE(ABOVE|LEFT)`)을 지원하는 `DocumentTableBlock`, 각주(`FootnoteItem`), 변경 내용 추적(`TrackedRevision`), 코멘트 스레드, 자동 목차(`TableOfContentsEntry`), `{{필드}}` 메일 머지.
  - **Sheet (`WorksheetModel`)**: 수학/삼각(27종), 통계(7종), 논리(10종), 찾기/참조(8종), 텍스트(16종), 재무(4종: `PMT`, `FV`, `PV`, `NPV`), 날짜/시간(5종)을 아우르는 **77종 내장 수식 함수**, `PivotTableEngine`, `SheetChartBuilder`, `ConditionalFormatRule`, `DataValidationRule`.
  - **Slide (`PresentationDeckModel`)**: `SlideAspectRatio` (`16:9`, `4:3`, `A4`), `SlideMasterLayout`, `SlideCanvasElement` 레이어(Z-order) 및 정렬/균등 배분, `SnapGuideCalculator` 스마트 정렬 가이드, `SlideTransitionKind`, 듀얼 모니터 `PresenterSession`.
  - **PDF Studio (`PDFStudioDocumentModel`)**: 고속 벡터 페이지 캔버스, 7종 주석(`PDFAnnotationItem`: 형광펜, 밑줄, 취소선, 스티커 메모, 자유형 펜, 스탬프, 전자 서명), 북마크 트리(`PDFOutlineNode`), 페이지 회전/재배치/분할/병합, 양방향 오피스 ↔ PDF 변환.
- **100% 오프라인 스마트 생산성 도구 및 보안 감사 스튜디오**:
  - **포맷 일괄 변환 스튜디오 (`DocumentConversionOrchestrator`)**: 20종 전체 포맷 간 단건·배치 변환 및 출력 바이트 `FormatDetector.verify` 시그니처 사후 검증.
  - **온디바이스 스마트 문서 보조 도구 (`LocalSmartDocumentAssistant`)**: 외부 네트워크 전송 없이 로컬에서 수행하는 단어 빈도·위치 가중치 기반 핵심 문장 추출 요약(`DocumentSummaryReport`), 문장 길이·피동 표현·중복 수식어 교정 진단(`WritingRefinementSuggestion`), 기한·예산·조치 항목 추출(`ExtractedActionItem`), 시트 Z-Score 이상치(`|z| >= 1.8`) 및 추세 인사이트(`WorksheetInsightReport`), 문단 상한($N \le 2,048$) 동적 계획법 LCS 문서 비교(`DocumentDiffReport`).
  - **Fail-Closed 컨테이너 및 메타데이터 보안 (`FormatDetector`, `XMLSanitizer`, `DocumentSecurityAuditor`)**: 확장자 위조(매직 바이트 불일치), ZIP 압축 폭탄(`100:1` 비율 / `512 MiB` 상한), 경로 순회(`..`, `/`, `\`, NUL), XML 외부 엔티티(`<!ENTITY`, `DOCTYPE SYSTEM/PUBLIC`)를 입구에서 차단하고, 공유 전 문서 내 개인정보(PII)·변경 이력·코멘트를 비식별화합니다.
- **시그니처 라벤더 디자인 시스템 및 WCAG AA 접근성**:
  - `EasyOfficeDesignTokens`(`#5C6BC0` Primary Accent, `#F7F8FC` 라이트 라벤더 스캐폴드, `#141724` 다크 인디고 스캐폴드), `WorkspaceThemeMode`(`.system`, `.lightLavender`, `.darkIndigo`), 5대 모듈 시맨틱 컬러, WCAG 2.1 AA 명도 대비(`>= 4.5:1`) 검증 토큰 및 `AccessibilityTokens` VoiceOver 레이블을 적용했습니다.

---

## 지원 포맷 매트릭스 (20종)

| 모듈 | 지원 확장자 | 기술 표준 / 컨테이너 규격 | 핵심 코덱 및 엔진 기능 |
| :--- | :--- | :--- | :--- |
| **Document** | `DOCX`, `DOC`, `ODT`, `RTF`, `MD`, `TXT` | ECMA-376 WordprocessingML (ZIP+XML), OLE2 Word, ISO/IEC 26300 ODF Text, RTF 1.9, CommonMark, UTF-8 | `OOXMLContainerCodec`, `ODFContainerCodec`, `OLE2ContainerCodec`, `RTFCodec`, `CommonMarkCodec`, 다단/구역 조판, 표 계산식, 변경 내용 추적, 메일 머지 |
| **HWP (한글)** | `HWP`, `HWPX`, `HWT`, `HWTX` | HWP 5.0 Binary (OLE2 CFBF `FileHeader`/`DocInfo`/`BodyText`), KS X 6101 OWPML (ZIP+XML) | `HWP5BinaryCodec`, `HWP5RecordHeader`, `HWPXContainerCodec`, 공문서 두문/결재란 표 양식, 머리말/꼬리말, 각주, 서식 템플릿, HWP ↔ HWPX/DOCX/PDF 변환 |
| **Sheet** | `XLSX`, `XLS`, `ODS`, `CSV`, `TSV` | ECMA-376 SpreadsheetML, OLE2 BIFF8, ISO/IEC 26300 ODF Spreadsheet, RFC 4180 CSV/TSV | `PackedCellCoordinate`, 77종 내장 수식 함수, `PivotTableEngine`, `SheetChartBuilder`, `CSVStateMachineCodec`, 다중 정렬, 조건부 서식·유효성 검사 |
| **Slide** | `PPTX`, `PPT`, `PPSX`, `ODP` | ECMA-376 PresentationML, OLE2 Presentation, ISO/IEC 26300 ODF Presentation | `PresentationDeckModel`, 16:9·4:3·A4 캔버스, `SnapGuideCalculator` 스마트 정렬 가이드, 레이어 순서 제어, 전환 효과, `PresenterSession` 발표자 뷰 |
| **PDF Studio** | `PDF` | ISO 32000-1 / 32000-2 (`%PDF-1.x` / `%PDF-2.0`) | `PDFBinarySerializer`, `PDFStudioDocumentModel`, 7종 주석·서명, 북마크 트리, 페이지 병합/분할/재배치/회전, 오피스 ↔ PDF 양방향 변환 |

---

## 아키텍처 개요 (Architecture Overview)

```text
EasyOffice/
├── Package.swift                                  # Swift 6 패키지 매니페스트 (macOS 14+)
├── Sources/
│   ├── EasyOfficeDesignSystem/                    # 시그니처 라벤더 팔레트, 모듈 시맨틱 토큰, WCAG 명도 대비
│   │   └── DesignTokens.swift                     # RGBColorToken, WorkspaceThemeMode, AccessibilityTokens
│   ├── EasyOfficeCore/                            # 순수 결정론적 오피스 엔진, 바이너리 코덱 및 스마트 도구
│   │   ├── DocumentFormat.swift                   # 20종 포맷 레지스트리 및 ContainerFamily 메타데이터
│   │   ├── FormatDetector.swift                   # 매직 바이트 시그니처 검증 가드 및 EasyOfficeError
│   │   ├── BinaryContainerCodec.swift             # BitPacking, CRC32Calculator, ZIP/OOXML/ODF/HWPX/OLE2/HWP5 코덱
│   │   ├── TabularAndMarkupCodec.swift            # CSVStateMachineCodec, CommonMarkCodec, RTFCodec
│   │   ├── DocumentEngine.swift                   # OfficeDocumentModel, 구역 조판, 표 계산식, 변경 추적, 메일 머지
│   │   ├── SheetEngine.swift                      # PackedCellCoordinate, 77종 함수, PivotTableEngine, 차트 빌더
│   │   ├── SlideEngine.swift                      # PresentationDeckModel, SnapGuideCalculator, PresenterSession
│   │   ├── PDFEngine.swift                        # PDFStudioDocumentModel, 주석, 분할/병합, 오피스 ↔ PDF 변환
│   │   ├── SmartProductivityTools.swift           # PDFBinarySerializer, 일괄 변환 스튜디오, 오프라인 보조 도구, 보안 감사
│   │   └── WorkspaceHub.swift                     # WorkspaceViewModel, 7대 리본 카탈로그, 템플릿, 통합 색인 검색
│   └── EasyOfficeApp/                             # macOS 네이티브 SwiftUI + AppKit 워크스페이스 애플리케이션
│       ├── EasyOfficeApp.swift                    # @main 앱 엔트리포인트 및 macOS 메뉴바 커맨드
│       ├── WorkspaceViews.swift                   # 통합 타이틀바, 멀티 탭 스트립, 분할 보기, 컨텍스트 리본, 홈 허브
│       └── EditorDetailPanes.swift                # Document/HWP, Sheet, Slide, PDF Studio 인터랙티브 에디터 패널
├── Tests/
│   └── EasyOfficeTests/                           # 결정론적 단위·보안·UI 패리티 및 벤치마크 검증 스위트
│       ├── EasyOfficeTests.swift                  # 디자인 토큰, 포맷 카탈로그 및 기본 엔진 검증
│       ├── BinaryContainerCodecTests.swift        # BitPacking, CRC32, ZIP/OLE2/HWP5/CSV/MD/RTF 및 보안 가드 검증
│       ├── EditorEnginesParityTests.swift         # 77종 시트 수식, 피벗/차트, 문서/HWP, 슬라이드, PDF 패리티 검증
│       ├── SmartProductivityToolsTests.swift      # 일괄 변환, 오프라인 스마트 보조 도구 및 보안 감사기 검증
│       ├── WorkspaceUIParityTests.swift           # 스플릿 뷰, 7x5 리본 매트릭스, WCAG AA 및 접근성 검증
│       └── CommercialParityBenchmarkTests.swift   # 20종 포맷 라운드트립 매트릭스 및 비트 수준 처리량 벤치마크
├── scripts/
│   ├── check-repo-hygiene.mjs                     # 저장소 위생 및 명세 무결성 검증 가드
│   └── package-macos-release.sh                   # macOS 유니버설 .app 번들, .dmg, .zip, SHA256SUMS 생성기
└── docs/
    ├── architecture.md                            # 시스템 아키텍처, 코덱 계층 및 Fail-Closed 보안 경계
    ├── design-system.md                           # 시그니처 라벤더 UI/UX 디자인 시스템 및 레이아웃 명세
    ├── policy-decisions.md                        # 제품 및 엔지니어링 정책 결정 기록 (SSOT)
    └── product-spec.md                            # 20종 포맷 및 5대 모듈 통합 기능 명세
```

---

## 빠른 시작 (Quick Start)

### 시스템 요구 사항
- **운영체제**: macOS 14.0 (Sonoma) 이상 (Apple Silicon `arm64` 및 Intel `x86_64` 유니버설 지원)
- **개발 도구**: Xcode 16+ / Swift 6.0+

### 로컬 위생 검증 · 빌드 · 테스트 · 실행

```bash
# 1. 저장소 위생 및 필수 계약 검증
node scripts/check-repo-hygiene.mjs

# 2. 전체 모듈 빌드
swift build

# 3. 결정론적 단위 테스트 및 벤치마크 스위트 실행
swift test

# 4. macOS 네이티브 앱 실행
swift run EasyOfficeApp

# 5. 배포용 EasyOffice.app 번들, .dmg, .zip 및 SHA256SUMS.txt 패키징
bash scripts/package-macos-release.sh 1.0.0
```

---

## 라이선스

[MIT License](LICENSE)에 따라 배포됩니다.
