# EasyOffice 통합 제품 및 기능 명세 (Product Specification)

이 문서는 `EasyOffice` (`v1.0.0`)가 macOS 환경에서 제공하는 전체 20종 포맷 호환성 매트릭스와 5대 핵심 모듈(`Document`, `HWP/HWPX`, `Sheet`, `Slide`, `PDF Studio`), 바이너리/컨테이너 코덱 및 오프라인 스마트 생산성 도구의 정본 명세를 정의합니다.

---

## 1. 지원 포맷 매트릭스 (20 Supported Formats & Codecs)

| 카테고리 | 확장자 | 기술 표준 / 컨테이너 규격 (`ContainerFamily`) | 코덱 및 지원 범위 |
| :--- | :--- | :--- | :--- |
| **Document (워드)** | `.docx`, `.doc`, `.odt`, `.rtf`, `.md`, `.txt` | ECMA-376 OOXML WordprocessingML (`zipXML`), OLE2 Word (`ole2Compound`), ISO/IEC 26300 ODF Text (`zipXML`), RTF 1.9 (`rtfText`), CommonMark (`plainText`), UTF-8 (`plainText`) | `OOXMLContainerCodec`, `OLE2ContainerCodec`, `ODFContainerCodec`, `RTFCodec`, `CommonMarkCodec` 기반 열람 · 편집 · 라운드트립 직렬화 · PDF/MD/TXT/HWPX 변환 |
| **HWP (한글 문서)** | `.hwp`, `.hwpx`, `.hwt`, `.hwtx` | HWP 5.0 Binary (`ole2Compound`: `FileHeader` / `DocInfo` / `BodyText/Section0`), KS X 6101 OWPML (`zipXML`: `Contents/section0.xml`) | `HWP5BinaryCodec` (`HWP5RecordHeader`), `HWPXContainerCodec` 기반 열람 · 편집 · 서식 양식(`.hwt`/`.hwtx`) · HWP ↔ HWPX/DOCX/PDF 변환 |
| **Sheet (스프레드시트)** | `.xlsx`, `.xls`, `.ods`, `.csv`, `.tsv` | ECMA-376 SpreadsheetML (`zipXML`), OLE2 BIFF8 (`ole2Compound`), ISO/IEC 26300 ODF Spreadsheet (`zipXML`), RFC 4180 CSV/TSV (`plainText`) | `OOXMLContainerCodec`, `OLE2ContainerCodec`, `ODFContainerCodec`, `CSVStateMachineCodec` 기반 열람 · 편집 · 77종 수식 연산 · 피벗/차트 · CSV/TSV/PDF 변환 |
| **Slide (프레젠테이션)** | `.pptx`, `.ppt`, `.ppsx`, `.odp` | ECMA-376 PresentationML (`zipXML`), OLE2 Presentation (`ole2Compound`), ISO/IEC 26300 ODF Presentation (`zipXML`) | `OOXMLContainerCodec`, `OLE2ContainerCodec`, `ODFContainerCodec` 기반 열람 · 편집 · 슬라이드 마스터 · 발표자 뷰 · PDF 변환 |
| **PDF Studio** | `.pdf` | ISO 32000-1 / 32000-2 (`pdfBinary`: `%PDF-1.x` / `%PDF-2.0`) | `PDFBinarySerializer` 기반 벡터 열람 · 5종 주석/서명 · 페이지 병합/분할/재배치/회전 · Office ↔ PDF 양방향 변환 |

---

## 2. 모듈별 세부 기능 명세 (Core Module Capabilities)

### 2.1 Document & HWP/HWPX (`OfficeDocumentModel`, `DocumentEngine.swift`)
- **문서 조판 및 다중 구역 레이아웃 (`DocumentSectionLayout`)**:
  - 용지 규격(`PaperSizePreset`: `a4`, `letter`, `b5`), 용지 방향(`PageOrientation`: `portrait`, `landscape`), 다단(`columnCount`: 1~4단), 상하좌우 여백(`marginTopPt`, `marginBottomPt`, `marginLeftPt`, `marginRightPt`) 조판.
  - 문단 블록(`ParagraphBlock`) 스타일 역할(`ParagraphStyleRole`: `title`, `heading1`, `heading2`, `heading3`, `body`, `quote`, `officialHeader`)과 문단 정렬(`TextAlignmentMode`: `leading`, `center`, `trailing`, `justified`), 글꼴 이름·크기·굵게·기울임·밑줄 제어.
  - 제목 스타일(`title`, `heading1`~`heading3`, `officialHeader`) 문단 기반 실시간 개요(`outlineHeadings`) 및 목차(`TableOfContentsEntry`, `generateTableOfContents()`) 자동 추출, 본문 찾기/바꾸기(`findAndReplace(search:replacement:caseSensitive:)`).
- **표(`DocumentTableBlock`) 및 공문서/각주 편집**:
  - 셀 병합(`TableCellSpan`, `mergeCells(startRow:startCol:rowSpan:colSpan:)`), 셀 분할(`splitCell(row:col:)`), 테두리 스타일(`TableCellBorderStyle`: `solid`, `doubleLine`, `dashed`, `diagonalDown`, `diagonalUp`, `none`), 인라인 표 수식 평가(`evaluateTableFormula(_:atRow:column:)`: `=SUM(ABOVE)`, `=AVERAGE(ABOVE)`, `=SUM(LEFT)`, `=AVERAGE(LEFT)`, `=COUNT(ABOVE)`, `=MAX(ABOVE)`, `=MIN(ABOVE)`).
  - 한글 문서(`HWP 5.0`/`HWPX`) 특화 구성: 공문서 두문(`officialHeader`), 머리말(`headerText`)/꼬리말(`footerText`), 각주/미주(`FootnoteItem`), 북마크(`bookmarks`), 결재란 이중선(`doubleLine`) 표 양식 호환.
- **검토, 메일 머지 및 통계 도구**:
  - 변경 내용 추적(`TrackedRevision`: `insertion`, `deletion`, `formattingChange` 기록 `recordRevision`, 승인 `acceptRevision(id:)` 및 반려 `rejectRevision(id:)`), 코멘트 스레드(`DocumentCommentThread` 답글 및 해결 상태).
  - 메일 머지(`applyingMailMerge(record:)`): `{{필드명}}` 플레이스홀더에 레코드 사전을 바인딩하여 안내문·계약서·공문서 일괄 생성.
  - 실시간 문서 통계(`DocumentStatistics`, `computeStatistics()`): 단어 수(`wordCount`), 글자 수(`characterCountWithSpaces`, `characterCountWithoutSpaces`), 문단 수(`paragraphCount`), 표 수(`tableCount`), 200자 원고지 매수(`manuscriptPages200`).

### 2.2 Sheet (`WorksheetModel`, `SheetEngine.swift`)
- **64비트 비트패킹 좌표 및 고성능 그리드 (`PackedCellCoordinate`)**:
  - 행(상위 32비트)과 열(하위 32비트)을 단일 `UInt64`에 패킹한 `PackedCellCoordinate`와 ECMA-376 경계(`maxZeroBasedColumn = 16,383`, `maxZeroBasedRow = 1,048,575`)를 검증하는 `CellAddress`, 범위 확장 상한(`maxExpandedCellCount = 100,000`)을 강제하는 `CellRange`를 제공합니다.
  - 범위 그리드 일괄 평가(`evaluateGrid(in:)`), 열 기준 정렬(`sortedRows(in:byColumnOffset:ascending:)`), 조건 필터링(`filteredRows(in:byColumnOffset:predicate:)`), 조건부 서식(`ConditionalFormatRule`: `greaterThan`, `lessThan`, `between`, `containsText`, `duplicateValues`), 데이터 유효성 검사(`DataValidationRule`: `numberRange`, `allowedList`, `nonEmptyText`, `maxTextLength`)를 지원합니다.
- **결정론적 수식 엔진 (77종 내장 함수, `WorksheetModel.supportedFunctionNames`)**:
  - **수학/삼각 및 집계 (27종)**: `ABS`, `AVERAGE`, `AVERAGEIF`, `CEILING`, `COUNT`, `COUNTA`, `COUNTBLANK`, `COUNTIF`, `EXP`, `FLOOR`, `INT`, `LN`, `LOG10`, `MAX`, `MIN`, `MOD`, `POWER`, `PRODUCT`, `ROUND`, `ROUNDDOWN`, `ROUNDUP`, `SIGN`, `SQRT`, `SUM`, `SUMIF`, `SUMPRODUCT`, `TRUNC`
  - **통계 (7종)**: `LARGE`, `MEDIAN`, `MODE`, `RANK`, `SMALL`, `STDEV`, `VAR`
  - **논리 (10종)**: `AND`, `FALSE`, `IF`, `IFERROR`, `IFS`, `NOT`, `OR`, `SWITCH`, `TRUE`, `XOR`
  - **찾기/참조 (8종)**: `CHOOSE`, `COLUMN`, `HLOOKUP`, `INDEX`, `MATCH`, `ROW`, `VLOOKUP`, `XLOOKUP`
  - **텍스트 (16종)**: `CONCAT`, `CONCATENATE`, `EXACT`, `FIND`, `LEFT`, `LEN`, `LOWER`, `MID`, `PROPER`, `REPT`, `RIGHT`, `SUBSTITUTE`, `TEXT`, `TRIM`, `UPPER`, `VALUE`
  - **재무 (4종)**: `FV`, `NPV`, `PMT`, `PV`
  - **날짜/시간 (5종)**: `DATE`, `DAY`, `DAYS`, `MONTH`, `YEAR`
  - 방문 집합 추적 및 최대 재귀 깊이(`maxRecursionDepth = 64`), 문자열 반복 상한(`maxReptResultUTF16Units = 65_536`) 가드로 순환 참조 및 수식 오류(`EvaluatedCellValue.error`)를 크래시 없이 결정론적으로 반환합니다.
- **피벗 테이블 및 차트 엔진 (`PivotTableEngine`, `SheetChartBuilder`)**:
  - `PivotTableConfiguration` 기반 행·열 그룹별 집계(`PivotAggregationFunction`: `sum`, `average`, `count`, `min`, `max`) 및 행/열 소계·총합계 산출(`PivotTableSummary`, `PivotTableEngine.compute`).
  - `SheetChartBuilder.buildChart`를 통한 7종 차트(`SheetChartKind`: `column`, `bar`, `line`, `area`, `pie`, `scatter`, `stackedColumn`) 시리즈(`SheetChartSeries`) 생성.

### 2.3 Slide (`PresentationDeckModel`, `SlideEngine.swift`)
- **슬라이드 마스터 및 화면 비율**:
  - 화면 비율(`SlideAspectRatio`: `widescreen16x9` 960×540pt, `standard4x3` 720×540pt, `handoutA4` 841.89×595.28pt) 및 마스터 레이아웃(`SlideMasterLayout`: `titleSlide`, `titleAndContent`, `sectionHeader`, `twoColumnComparison`, `blankCanvas`).
- **스마트 정렬 가이드 및 캔버스 개체 제어 (`SnapGuideCalculator`)**:
  - `SlideCanvasElement`(`SlideElementKind` 8종: `textBox`, `rectangleShape`, `roundedRectangleShape`, `ellipseShape`, `arrowShape`, `calloutShape`, `tableGrid`, `chartEmbed`) 배치, Z-Order 제어(`bringElementToFront(id:)`, `sendElementToBack(id:)`), 스테이지 중앙 정렬(`centerGuides(for:tolerance:)`).
  - `SnapGuideCalculator.computeGuides(movingElement:siblings:aspectRatio:tolerancePoints:)`를 통해 스테이지 중앙선, 형제 개체 모서리/중심선 및 3개 이상 개체 간 균등 간격 스냅 가이드(`AlignmentSnapGuide`)를 실시간 계산합니다.
- **전환 효과 및 발표자 세션 (`PresenterSession`)**:
  - 슬라이드 전환(`SlideTransitionKind` 4종: `none`, `fade`, `push`, `morph`)과 슬라이드 추가·복제·순서 이동(`moveSlide(from:to:)`), 현재/다음 슬라이드·발표자 노트·진행률(`progressFraction`)·경과 시간 타이머(`tick(seconds:)`)를 관리하는 `PresenterSession`.

### 2.4 PDF Studio (`PDFStudioDocumentModel`, `PDFEngine.swift`)
- **열람, 북마크 및 본문 검색**:
  - `PDFBinarySerializer` 기반 ISO 32000-1 `%PDF-1.7` 호환 바이너리 직렬화·파싱(`encode(_:)`, `decode(from:)`), 북마크 트리(`PDFOutlineNode`), 페이지별 텍스트 추출 및 대소문자 무시 본문 검색(`searchPages(containing:)`).
- **5종 주석 및 서명 (`PDFAnnotationItem`)**:
  - `PDFAnnotationKind`: `highlight`(형광펜), `underline`(밑줄), `stickyNote`(스티커 메모), `stamp`(도장), `signature`(전자 서명).
- **페이지 조작 및 양방향 오피스 변환**:
  - 페이지 90도 단위 회전(`rotatePage(at:clockwiseDegrees:)`), 빈 페이지 삽입(`insertBlankPage(at:)`), 페이지 삭제(`deletePage(at:)`), 순서 재배치(`reorderPage(from:to:)`), 지정 페이지 추출(`extractPages(oneBasedPageNumbers:newTitle:)`), 분할(`split(atOneBasedPage:)`), 다중 PDF 병합(`merge(documents:mergedTitle:)`).
  - `convertToEditableDocument(targetFormat:)`, `convertToPresentation()`, `fromDocument(_:)`, `fromWorksheet(_:range:)`, `fromPresentation(_:)`를 통한 오피스 문서·시트·슬라이드 ↔ PDF 양방향 변환.

### 2.5 통합 워크스페이스 허브 & 오프라인 스마트 생산성 도구 (`SmartProductivityTools.swift`, `WorkspaceHub.swift`)
- **통합 홈 허브 및 멀티 탭/스플릿 뷰 워크스페이스 (`WorkspaceViewModel`)**:
  - 6대 허브 섹션(`HubSection`: `home`, `recent`, `pinned`, `templates`, `converter`, `smartAssistant`), 8종 내장 템플릿 및 사용자 정의 템플릿(`OfficeTemplateDescriptor`), 열려 있는 모든 탭의 파일명·본문·수식·발표자 노트·PDF 주석을 아우르는 전문 색인 검색(`WorkspaceHubCatalog.searchTabs`, `filterTabs`).
  - 단일 윈도우 좌우 분할 보기(`splitSecondaryTabID`), 7대 컨텍스트 리본 카테고리(`RibbonCategory`), 5대 모듈별 인터랙티브 상세 에디터 패널(`EditorDetailPanes.swift`).
- **포맷 일괄 변환 스튜디오 (`DocumentConversionOrchestrator`)**:
  - 20종 전 포맷 간 단건(`convert(_:)`) 및 배치(`executeBatch(_:)`) 변환을 수행하며, 모든 출력 페이로드에 대해 `FormatDetector.verify` 매직 바이트 시그니처 무결성을 검증합니다.
- **100% 오프라인 로컬 스마트 문서 보조 도구 (`LocalSmartDocumentAssistant`)**:
  - 외부 서버 통신 없이 온디바이스에서 핵심 문장 추출 요약(`summarize(_:maxSentences:)`), 문장 길이·피동 표현·중복 수식어 교정 제안(`analyzeWritingQuality(_:)`), 기한·예산·핵심 수치·조치 항목 추출(`extractActionItemsAndEntities(_:)`), 시트 수치 범위 Z-Score 이상치(`|z| >= 1.8`) 및 추세 분석(`analyzeWorksheetInsights(_:range:)`), 최대 2,048 문단 상한 동적 계획법(LCS) 문서 비교(`compareDocuments(baseline:revised:)`)를 수행합니다.
- **문서 메타데이터 비식별화 및 컨테이너 보안 감사 (`DocumentSecurityAuditor`)**:
  - 공유 전 머리말/꼬리말 PII 제거, 변경 이력 작성자 `"Redacted"` 익명화, 코멘트 삭제(`scrubMetadata(from:)`) 및 컨테이너 매직 시그니처·ZIP 압축 폭탄·경로 순회·XXE·암호화 종합 감사(`auditContainer(data:fileName:)`)를 제공합니다.
