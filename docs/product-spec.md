# EasyOffice 통합 제품 및 기능 명세 (Product Specification)

이 문서는 `EasyOffice` (`v1.0.0`)가 macOS 환경에서 제공하는 전체 20종 포맷 호환성 매트릭스와 5대 핵심 모듈(`Document`, `HWP/HWPX`, `Sheet`, `Slide`, `PDF Studio`), 바이너리/컨테이너 코덱 및 오프라인 스마트 생산성 도구의 정본 명세를 정의합니다.

---

## 1. 지원 포맷 매트릭스 (20 Supported Formats & Codecs)

| 카테고리 | 확장자 | 기술 표준 / 컨테이너 규격 (`ContainerFamily`) | 코덱 및 지원 범위 |
| :--- | :--- | :--- | :--- |
| **Document (워드)** | `.docx`, `.doc`, `.odt`, `.rtf`, `.md`, `.txt` | ECMA-376 OOXML WordprocessingML (`zipXML`), OLE2 Word (`ole2Compound`), ISO/IEC 26300 ODF Text (`zipXML`), RTF 1.9 (`rtfText`), CommonMark (`plainText`), UTF-8 (`plainText`) | `OOXMLContainerCodec`, `OLE2ContainerCodec`, `ODFContainerCodec`, `RTFCodec`, `CommonMarkCodec` 기반 열람 · 편집 · 라운드트립 직렬화 · PDF/MD/TXT/HWPX 변환 |
| **HWP (한글 문서)** | `.hwp`, `.hwpx`, `.hwt`, `.hwtx` | HWP 5.0 Binary (`ole2Compound`: `FileHeader` / `DocInfo` / `BodyText/Section0`), KS X 6101 OWPML (`zipXML`: `Contents/section0.xml`) | `HWP5BinaryCodec` (`HWP5RecordHeader`), `HWPXContainerCodec` 기반 열람 · 편집 · 서식 양식(`.hwt`/`.hwtx`) · HWP ↔ HWPX/DOCX/PDF 변환 |
| **Sheet (스프레드시트)** | `.xlsx`, `.xls`, `.ods`, `.csv`, `.tsv` | ECMA-376 SpreadsheetML (`zipXML`), OLE2 BIFF8 (`ole2Compound`), ISO/IEC 26300 ODF Spreadsheet (`zipXML`), RFC 4180 CSV/TSV (`plainText`) | `OOXMLContainerCodec`, `OLE2ContainerCodec`, `ODFContainerCodec`, `CSVStateMachineCodec` 기반 열람 · 편집 · 122종 수식 연산 · 피벗/차트 · CSV/TSV/PDF 변환 |
| **Slide (프레젠테이션)** | `.pptx`, `.ppt`, `.ppsx`, `.odp` | ECMA-376 PresentationML (`zipXML`), OLE2 Presentation (`ole2Compound`), ISO/IEC 26300 ODF Presentation (`zipXML`) | `OOXMLContainerCodec`, `OLE2ContainerCodec`, `ODFContainerCodec` 기반 열람 · 편집 · 슬라이드 마스터 · 발표자 뷰 · PDF 변환 |
| **PDF Studio** | `.pdf` | ISO 32000-1 / 32000-2 (`pdfBinary`: `%PDF-1.x` / `%PDF-2.0`) | `PDFBinarySerializer` 기반 벡터 열람 · 5종 주석/서명 · 페이지 병합/분할/재배치/회전 · Office ↔ PDF 양방향 변환 |

### 1.1 출력 컨테이너 규격 및 쓰기 호환성 (Rust 변환 엔진, `PD-071`)
- **XLSX**: `<c>`에는 스키마 속성(`r`, `t`)만 기록하고, 수식 셀의 `<v>`에는 시트 엔진이 계산한 결과를 캐시하며(계산값이 없으면 생략), `workbook.xml`은 `<calcPr fullCalcOnLoad="1"/>`로 열 때 재계산을 요청합니다.
- **PPTX/PPSX**: 테마(`ppt/theme/theme1.xml`), 마스터 `p:clrMap`, 도형 비시각 속성(`p:nvSpPr`/`p:spPr`/`p:txBody`)을 포함하고, 슬라이드 제목은 제목 개체 틀, 발표자 노트는 노트 슬라이드, 문서 제목은 `docProps/core.xml`에 저장합니다.
- **HWPX/HWTX**: `mimetype`(첫 엔트리, 무압축; 나머지 파트는 DEFLATE, PD-079), `version.xml`, `settings.xml`, 글꼴·글자/문단 모양·스타일을 정의한 `header.xml`, 첫 문단 `hp:secPr`, `paraPrIDRef`/`charPrIDRef` 참조, `hp:p > hp:run > hp:tbl` 표 구조를 기록합니다.
- **DOCX**: 제목은 `docProps/core.xml`에 기록하고 표마다 `w:tblPr`·`w:tblGrid`를 둡니다.
- **WebUI 저장 서식 보존 (`PD-073`)**: 문서·한글 저장은 문단 런 서식(굵게·기울임·밑줄·취소선), 문단 정렬·제목 수준·목록과 문서 내 위치의 표를 구조화 블록으로 보내며, DOCX(`w:rPr`/`w:pPr`/`w:pStyle` 제목 스타일), HWPX(`charPr`/`paraPr`/`개요 N` 스타일), ODT·RTF·Markdown은 각 형식의 서식으로 기록합니다. 표 수식 셀(`=SUM(ABOVE)` 등)은 계산값으로 저장하고, 발신 기관 명칭을 임의로 넣지 않습니다. 시트 저장은 모든 시트를 담고, 변환 결과가 없거나 내려받기에 실패하면 "문서 저장 실패"를 표시하며, 파일명은 문서 탭 제목(Windows/macOS 금지 문자·예약 이름 정리)을 따릅니다. 글자 단위 런 모델(`PD-077`)로 선택한 글자에만 적용한 글꼴·크기(반포인트)·글자 색·형광펜을 구조화 런(`sizeHalfPt`/`font`/`color`/`highlight`)으로 보내 DOCX(`w:rFonts`/`w:sz`/`w:color`/`w:highlight`·`w:shd`), HWPX(`charPr` 높이·글자색·음영·`fontRef`), ODT(자동 텍스트 스타일), RTF(`\fs`·글꼴/색 표)에 기록하고 다시 열 때 같은 런으로 복원합니다. 표 셀 안의 글자 서식은 저장하지 않습니다.
- **WebUI 슬라이드 쇼·슬라이드 관리 (`PD-080`)**: 슬라이드 모듈에서 `F5`는 처음부터, `Shift+F5`는 현재 슬라이드부터 전체 화면 슬라이드 쇼를 시작합니다(전체 화면을 쓸 수 없으면 창 안에서 동작, 텍스트 입력 중에는 브라우저 새로 고침 유지). `→`/`Space`/`PageDown`/클릭 다음, `←`/`PageUp` 이전, `Home`/`End`, `B`/`W` 화면 가리기, `P` 발표자 보기(노트·다음 슬라이드·타이머), `Esc` 종료. 슬라이드 추가(빈 개체 틀)·복제·삭제(실행 취소 가능)·끌어 놓기/`Alt+↑/↓`/우클릭 메뉴 순서 변경, 편집이 즉시 반영되는 썸네일(말줄임·툴팁), 16:9·4:3·A4 비율을 유지하는 스테이지를 제공합니다.
- **WebUI 파일 열기·가져오기 (`PD-075`)**: 파일 메뉴 "열기…"/`Ctrl+O`와 끌어 놓기로 자신의 문서를 열면 자기 형식을 매직 바이트로 판별해(ZIP 폭탄·경로 순회·XXE 가드 적용) 문서·한글·시트·슬라이드·PDF 모듈에 적재합니다(DOCX/DOC/ODT/RTF/MD/TXT → 문서, HWP/HWPX/HWT/HWTX → 한글, XLSX/XLS/ODS/CSV/TSV → 시트(모든 시트·수식), PPTX/PPT/PPSX/ODP → 슬라이드, PDF → PDF 모듈(텍스트 추출)). DOCX·HWPX·ODT·RTF·Markdown은 런 서식(DOCX·HWPX·ODT·RTF는 글꼴·크기·글자 색·형광펜 포함, `PD-077`)·정렬·제목·목록·표 위치를, 슬라이드는 제목·글머리·노트를 보존하고 DOC·HWP·PDF는 서식 없이 본문과 표만 가져오며, 그림·도형·머리말·셀 서식 등 가져오지 못한 항목은 열기 후 상태 표시줄에 알립니다. 열기는 하나의 실행 취소 단위이며 탭 제목이 파일 이름이 되어 저장 파일명과 자동 저장이 따릅니다. 대상 모듈에 저장하지 않은 편집이 있으면 확인하고, 파일은 10 MiB까지, 시트는 XLSX 전체 주소 범위(1,048,576행 × 16,384열, `PD-078`)까지 가져옵니다. 변환기에서 원본 파일을 고르면 편집기 텍스트가 아닌 그 파일의 바이트를 대상 형식으로 변환합니다. 손상·미지원·보안 차단 파일은 오류만 표시하고 아무것도 바꾸지 않습니다.
- **시트 편집 기본기 (`PD-078`)**: 셀을 고른 뒤 입력하면 내용을 덮어쓰고(`F2`·더블클릭은 편집), `Enter`/`Tab`/방향키로 이동하고 `Shift`·드래그로 범위를 선택하며 `Ctrl+C`/`X`/`V`로 TSV를 시스템 클립보드와 주고받습니다(수식의 단순 A1 상대 참조 이동). 상태 표시줄 집계는 선택 범위의 합계·평균·개수·최소·최대이고, 격자는 1,048,576행 × 16,384열을 가상 스크롤로 다루며 열 머리글을 끌어 너비를 바꿉니다. 숫자는 오른쪽 정렬·10자리 일반 서식으로 보이고 원본 값은 편집 시 그대로 나타납니다.
- **PDF**: 한글은 사전 정의 CJK 글꼴 참조(`HYGoThic-Medium`, `Adobe-Korea1`, `UniKS-UCS2-H`)로 기록하여 뷰어가 설치된 한글 글꼴로 표시하며, 비ASCII `/Title`은 BOM을 붙인 UTF-16BE로 기록합니다.
- **CSV/TSV**: 출력에 UTF-8 BOM을 붙이고, 입력의 BOM은 제거하여 읽습니다.
- **DOC/PPT 쓰기 호환성 제한**: `.doc`/`.ppt` 출력은 EasyOffice가 다시 읽을 수 있는 OLE2 스트림 하위 집합이며(`.doc`: `1Table`/`0Table` 없음, `.ppt`: `Current User`·persist directory 없음) 다른 프로그램에서 열리지 않을 수 있습니다. 변환 응답은 `"compatibility":"limited"`와 `"compatibilityWarning"`을 반환하고 변환 결과·저장 상태 메시지에 경고를 표시합니다. 호환 저장에는 `.docx`/`.pptx`를 사용합니다(규격 바이너리 작성기는 #112).

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
- **결정론적 수식 엔진 (122종 내장 함수, `WorksheetModel.supportedFunctionNames`)**
  - 수학/삼각 및 조건부 집계 (36종) `ABS`, `AVERAGE`, `AVERAGEIF`, `AVERAGEIFS`, `CEILING`, `COUNT`, `COUNTA`, `COUNTBLANK`, `COUNTIF`, `COUNTIFS`, `EXP`, `FLOOR`, `INT`, `LN`, `LOG10`, `MAX`, `MAXIFS`, `MIN`, `MINIFS`, `MOD`, `MROUND`, `PI`, `POWER`, `PRODUCT`, `QUOTIENT`, `ROUND`, `ROUNDDOWN`, `ROUNDUP`, `SIGN`, `SQRT`, `SUM`, `SUMIF`, `SUMIFS`, `SUMPRODUCT`, `SUMSQ`, `TRUNC`
  - 통계 (18종) `AVEDEV`, `CORREL`, `FORECAST`, `GEOMEAN`, `HARMEAN`, `INTERCEPT`, `LARGE`, `MEDIAN`, `MODE`, `PERCENTILE`, `QUARTILE`, `RANK`, `SLOPE`, `SMALL`, `STDEV`, `STDEVP`, `VAR`, `VARP`
  - 논리 및 정보 (15종) `AND`, `FALSE`, `IF`, `IFERROR`, `IFNA`, `IFS`, `ISBLANK`, `ISERROR`, `ISNUMBER`, `ISTEXT`, `NOT`, `OR`, `SWITCH`, `TRUE`, `XOR`
  - 찾기/참조 (11종) `CHOOSE`, `COLUMN`, `COLUMNS`, `HLOOKUP`, `INDEX`, `MATCH`, `OFFSET`, `ROW`, `ROWS`, `VLOOKUP`, `XLOOKUP`
  - 텍스트 (19종) `CONCAT`, `CONCATENATE`, `EXACT`, `FIND`, `LEFT`, `LEN`, `LOWER`, `MID`, `PROPER`, `REPLACE`, `REPT`, `RIGHT`, `SEARCH`, `SUBSTITUTE`, `TEXT`, `TEXTJOIN`, `TRIM`, `UPPER`, `VALUE`
  - 재무 (11종) `EFFECT`, `FV`, `IRR`, `NOMINAL`, `NPER`, `NPV`, `PMT`, `PV`, `RATE`, `SLN`, `SYD`
  - 날짜/시간 (12종) `DATE`, `DATEDIF`, `DAY`, `DAYS`, `EDATE`, `EOMONTH`, `MONTH`, `NETWORKDAYS`, `WEEKDAY`, `WEEKNUM`, `WORKDAY`, `YEAR`
  - 방문 집합 추적 및 최대 재귀 깊이(`maxRecursionDepth = 64`), 문자열 반복 상한(`maxReptResultUTF16Units = 65_536`), 셀 문자열 상한(`maxCellTextLength = 32_767`) 가드로 순환 참조 및 수식 오류(`EvaluatedCellValue.error`)를 크래시 없이 결정론적으로 반환합니다.
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

### 2.5 통합 워크스페이스 허브 & 오프라인 스마트 생산성 도구 (`SmartProductivityTools.swift`, `DocumentSecurityAuditor.swift`, `WorkspaceHub.swift`)
- **통합 홈 허브 및 멀티 탭/스플릿 뷰 워크스페이스 (`WorkspaceViewModel`)**:
  - 6대 허브 섹션(`HubSection`: `home`, `recent`, `pinned`, `templates`, `converter`, `smartAssistant`), 8종 내장 템플릿 및 사용자 정의 템플릿(`OfficeTemplateDescriptor`), 열려 있는 모든 탭의 파일명·본문·수식·발표자 노트·PDF 주석을 아우르는 전문 색인 검색(`WorkspaceHubCatalog.searchTabs`, `filterTabs`).
  - 단일 윈도우 좌우 분할 보기(`splitSecondaryTabID`), 7대 컨텍스트 리본 카테고리(`RibbonCategory`), 5대 모듈별 인터랙티브 상세 에디터 패널(`EditorDetailPanes.swift`).
- **포맷 일괄 변환 스튜디오 (`DocumentConversionOrchestrator`)**:
  - 20종 전 포맷 간 단건(`convert(_:)`) 및 배치(`executeBatch(_:)`) 변환을 수행하며, 모든 출력 페이로드에 대해 `FormatDetector.verify` 매직 바이트 시그니처 무결성을 검증합니다.
- **100% 오프라인 로컬 스마트 문서 보조 도구 (`LocalSmartDocumentAssistant`)**:
  - 외부 서버 통신 없이 온디바이스에서 핵심 문장 추출 요약(`summarize(_:maxSentences:)`), 문장 길이·피동 표현·중복 수식어 교정 제안(`analyzeWritingQuality(_:)`), 기한·예산·핵심 수치·조치 항목 추출(`extractActionItemsAndEntities(_:)`), 시트 수치 범위 Z-Score 이상치(`|z| >= 1.8`) 및 추세 분석(`analyzeWorksheetInsights(_:range:)`), 최대 2,048 문단 상한 동적 계획법(LCS) 문서 비교(`compareDocuments(baseline:revised:)`)를 수행합니다.
- **문서·이미지 메타데이터 비식별화 및 컨테이너 보안 감사 (`DocumentSecurityAuditor`)**
  - 공유 전 문서(`OfficeDocumentModel`), 시트(`WorksheetModel` 수식 문자열 리터럴·시트 메모·조건부 서식·유효성 검사 포함), 슬라이드(`PresentationDeckModel` 캔버스 도형 제목·본문 포함), PDF(`PDFStudioDocumentModel` 주석 작성자·양식 필드명·펜 레이블·북마크 트리 포함) 전 영역의 주민등록번호·외국인등록번호·연락처·이메일·신용카드(Luhn 검증)·은행 계좌·여권·운전면허·사업자등록번호 PII를 비식별화합니다.
  - 첨부 및 보존 미디어 스트림의 JPEG(`APP1..APP15`, `COM`), PNG(`tEXt`, `zTXt`, `iTXt`, `eXIf`, `tIME`), TIFF(EXIF/GPS/XMP 태그) 바이너리 메타데이터를 제거(`stripBinaryImageMetadata`)하며, 컨테이너 매직 시그니처·ZIP 압축 폭탄·경로 순회·XXE·암호화 종합 감사(`auditContainer(data:fileName:)`)를 수행합니다.

---

## 3. 플랫폼 배포 형태 (Platform Delivery, `PD-074`)
- **macOS**: 네이티브 Swift 앱(`EasyOffice.app`, `.dmg`/`.zip`)으로 배포합니다.
- **Windows**: 설치 프로그램 `EasyOffice-<version>-windows-x64-setup.exe`(NSIS, 사용자 단위 설치, 시작 메뉴 바로 가기, 제거 프로그램)로 배포합니다. 설치된 `EasyOffice.exe`는 Windows 내장 WebView2 위의 데스크톱 창(Tauri v2 셸)이며, 실행 파일에 포함된 WebUI가 프로세스 안의 Rust 엔진(15종 IPC 명령)을 직접 호출합니다. 로컬 HTTP 서버·브라우저·네트워크 연결 없이 동작하고, 저장은 Windows 기본 저장 대화상자로 위치를 고릅니다(취소·실패는 저장 상태 표시에 그대로 보고). 문서 파일 연결(더블클릭으로 열기)은 파일 열기 기능(#111)이 준비될 때까지 등록하지 않습니다.
- **개발자 미리보기**: `easyoffice-app.exe --serve [PORT]`는 같은 WebUI를 `127.0.0.1` 루프백 서버로 브라우저에 띄우는 개발·테스트 경로이며, 보조 아카이브 `EasyOffice-<version>-windows-x64-cli-serve-preview.zip`으로 배포합니다.
