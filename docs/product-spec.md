# EasyOffice 통합 제품 및 기능 명세 (Product Specification)

이 문서는 `EasyOffice` (`v0.1.0`)가 macOS 환경에서 제공하는 전체 20종 포맷 호환성 매트릭스와 5대 핵심 모듈(`Document`, `HWP/HWPX`, `Sheet`, `Slide`, `PDF Studio`), 바이너리/컨테이너 코덱 및 오프라인 스마트 생산성 도구의 정본 명세를 정의합니다.

---

## 1. 지원 포맷 매트릭스 (20 Supported Formats & Codecs)

| 카테고리 | 확장자 | 기술 표준 / 컨테이너 규격 (`ContainerFamily`) | 코덱 및 지원 범위 |
| :--- | :--- | :--- | :--- |
| **Document (워드)** | `.docx`, `.doc`, `.odt`, `.rtf`, `.md`, `.txt` | ECMA-376 OOXML WordprocessingML (`zipXML`), OLE2 Word (`ole2Compound`), ISO/IEC 26300 ODF Text (`zipXML`), RTF 1.9 (`rtfText`), CommonMark (`plainText`), UTF-8 (`plainText`) | `OOXMLContainerCodec`, `OLE2ContainerCodec`, `ODFContainerCodec`, `RTFCodec`, `CommonMarkCodec` 기반 열람 · 편집 · 9단계 목록·체크리스트(`PD-250`) · 6슬롯/3열 머리글·바닥글(`PD-249`) · 로컬 그림·표 열 너비/셀 배경/테두리/셀 그림(`PD-251`) · 각주·미주·새 메모·변경 내용 추적(`PD-252`) 무손실 라운드트립 · PDF/MD/TXT/HWPX 변환 |
| **HWP (한글 문서)** | `.hwp`, `.hwpx`, `.hwt`, `.hwtx` | HWP 5.0 Binary (`ole2Compound`: `FileHeader` / `DocInfo` / `BodyText/Section0`), KS X 6101 OWPML (`zipXML`: `Contents/section0.xml`) | `HWP5BinaryCodec` (`HWP5RecordHeader`), `HWPXContainerCodec` 기반 열람 · 편집 · 서식 양식(`.hwt`/`.hwtx`) · 자체 `HWPX` 변환 코덱의 도형(`drawText`·묶음 개체·글맵시·캡션)·수식(`hp:equation` → LaTeX)·표 셀 안 그림·각주/미주(구역 재시작·WCHAR 장식 기호 포함) 보존(`PD-247`) · HWP ↔ HWPX/DOCX/Pages/PDF 변환 |
| **Pages (읽기·쓰기)** | `.pages` | iWork IWA (`zipXML`: `Index/*.iwa` = Snappy 변형 청크 + protobuf 레코드). 구형 `index.xml`(2009 이전)·암호화 문서는 거부 | `pages_iwa` / `pages_proto` / `rich_read_pages` / `pages_write` 자체 구현. 본문·글자 서식·정렬·제목·9단계 목록·병합 표(셀 링크·그림·방정식·수식 결과 포함)·그림·쪽 위 부동 개체·3구역 및 첫 쪽/짝홀 머리글·쪽별 재시작 각주/미주·댓글·변경 추적·다단·28종 차트 데이터 표를 문서 모듈로 열람(PD-157, PD-240). 쓰기(`PD-162`, `PD-246`)는 본문·제목 1~6·정렬·들여쓰기·줄 간격·다단·확장 글자 서식(굵게/기울임/밑줄/취소선/크기/글꼴/색/형광펜/위·아래첨자)·글머리 및 9단계 번호 목록·외부 하이퍼링크·인라인 PNG/JPEG 그림(`Data/` 중복 제거)·다중 타일 표(병합·셀 링크 포함)·용지/여백/양면 제본·3구역 및 첫 쪽/짝수 쪽 머리글·바닥글(쪽 번호·전체 쪽 수 필드 포함)을 기록하며 미표현 요소(목차 컨테이너·내부 `#_Toc` 앵커 등)만 손실 경고로 알립니다 |
| **Sheet (스프레드시트)** | `.xlsx`, `.xls`, `.ods`, `.csv`, `.tsv` | ECMA-376 SpreadsheetML (`zipXML`), OLE2 BIFF8 (`ole2Compound`), ISO/IEC 26300 ODF Spreadsheet (`zipXML`), RFC 4180 CSV/TSV (`plainText`) | `OOXMLContainerCodec`, `OLE2ContainerCodec`, `ODFContainerCodec`, `CSVStateMachineCodec` 기반 열람 · 편집 · 288종 수식 연산(동적 배열·구조적 참조 포함) · 명시 셀 타입(`PD-241`) · 표 60종+사용자 지정 스타일/테마/슬라이서(`PD-242`) · 필터/유효성/조건부 서식/보호 · 5종 벡터 차트 시각화(SVG·인쇄·벡터 PDF) 및 `XLSX` `xl/charts/` 왕복(`PD-254`) · 동적 피벗 테이블(`pivotCache`·새로 고침·다중 행/열/필터/값 필드·피벗 슬라이서·`XLSX` `xl/pivotTables/` 왕복, `PD-255`) · 부동 그림(`xl/drawings/`·`xl/media/` 중복 제거·PDF Image XObject) 및 셀 메모(`xl/comments*.xml`·`vmlDrawing*.vml`·표시자, `PD-256`) 무손실 왕복 · CSV/TSV/PDF 변환 |
| **Slide (프레젠테이션)** | `.pptx`, `.ppt`, `.ppsx`, `.odp` | ECMA-376 PresentationML (`zipXML`), OLE2 Presentation (`ole2Compound`), ISO/IEC 26300 ODF Presentation (`zipXML`) | `OOXMLContainerCodec`, `OLE2ContainerCodec`, `ODFContainerCodec` 기반 슬라이드 제목·본문·발표자 노트 및 5종 좌표 기반 캔버스 개체(글상자·6종 도형·중복 제거 PNG/JPEG 그림·표·5종 벡터 차트)·화면비(`16:9`·`4:3`·`A4`)·5종 마스터 레이아웃·4종 전환 효과 `PPTX`/`PPSX`/`ODP` 무손실 왕복 · 발표자 쇼 · 다중 쪽 벡터 PDF 변환(`PD-257`) |
| **PDF Studio** | `.pdf` | ISO 32000-1 / 32000-2 (`pdfBinary`: `%PDF-1.x` / `%PDF-2.0`) | 원본 그림·글꼴·안전 링크·북마크·주석·대화형 양식(`/AcroForm`) 보존 페이지 회전·병합·분할·워터마크·결재 스탬프·문구 형광펜/밑줄·신청인/부서 양식 반영(`PD-170`, `PD-245`) 및 새 PDF 텍스트 마크업 주석(`PD-161`) · 오피스 ↔ PDF 변환 |

### 1.1 출력 컨테이너 규격 및 쓰기 호환성 (Rust 변환 엔진, `PD-071`)
- **XLSX**: `<c>`에는 스키마 속성(`r`, `t`)만 기록하고, 수식 셀의 `<v>`에는 시트 엔진이 계산한 결과를 캐시하며(계산값이 없으면 생략), `workbook.xml`은 `<calcPr fullCalcOnLoad="1"/>`로 열 때 재계산을 요청합니다.
- **PPTX/PPSX**: 테마(`ppt/theme/theme1.xml`), 마스터 `p:clrMap`, 도형 비시각 속성(`p:nvSpPr`/`p:spPr`/`p:txBody`)을 포함하고, 슬라이드 제목은 제목 개체 틀, 발표자 노트는 노트 슬라이드, 문서 제목은 `docProps/core.xml`에 저장합니다.
- **HWPX/HWTX**: `mimetype`(첫 엔트리, 무압축; 나머지 파트는 DEFLATE, PD-079), `version.xml`, `settings.xml`, 글꼴·글자/문단 모양·스타일을 정의한 `header.xml`, 첫 문단 `hp:secPr`, `paraPrIDRef`/`charPrIDRef` 참조, `hp:p > hp:run > hp:tbl` 표 구조를 기록합니다.
- **DOCX**: 제목은 `docProps/core.xml`에 기록하고 표마다 `w:tblPr`·`w:tblGrid`를 두며, 좌·중·우 3구역 머리글·바닥글은 단일 `<w:p>` 안 절대 위치 정렬 탭(`<w:ptab w:alignment="center|right" w:relativeTo="margin"/>`)과 첫 쪽(`<w:titlePg/>`)·짝수/홀수 쪽(`<w:evenAndOddHeaders/>`) 파트로 기록·복원합니다(`PD-243`, `PD-249`). ODT도 마스터 페이지 `<style:header>`·`<style:header-first>`·`<style:header-left>`(바닥글 동일) 아래 `<style:region-left|center|right>` 3구역을 양방향 보존합니다(`PD-249`).
- **WebUI 저장 서식 보존 (`PD-073`, `PD-249`, `PD-250`, `PD-251`)**: 문서·한글 저장은 문단 런 서식(굵게·기울임·밑줄·취소선), 문단 정렬·제목 수준·목록(문단 번호·글머리표·대화형 체크리스트 포함), 로컬 PNG/JPEG 그림 삽입·크기(`widthEmu`·`heightEmu`), 문서 내 위치의 표(열 너비·셀 배경색·4방향 개별 테두리·표 셀 안 그림 포함), 그리고 6슬롯 머리말·꼬리말(기본/홀수·첫 쪽·짝수 쪽 × 좌·중·우 3구역 분할 편집)을 구조화 블록으로 보내며, DOCX(`w:rPr`/`w:pPr`/`w:pStyle`·`w14:checkbox` SDT·`w:tblGrid`/`w:tcW`/`w:tcBorders`/`w:shd`/`w:drawing`·`w:ptab`), HWPX(`charPr`/`paraPr`·`checkable` 글머리표·`borderFill`/`cellSz`/`hp:pic`·`개요 N` 스타일), ODT(자동 스타일·체크 문자 목록·표 열/셀 자동 스타일·`draw:frame`·`style:region-*`), Pages(`TP.HeaderFooterList`·`TSD.ImageArchive`), RTF·Markdown(`- [ ]`·`- [x]`)은 각 형식의 서식으로 기록합니다. 표 수식 셀(`=SUM(ABOVE)` 등)은 계산값으로 저장하고, 발신 기관 명칭을 임의로 넣지 않습니다. 시트 저장은 모든 시트를 담고, 변환 결과가 없거나 내려받기에 실패하면 "문서 저장 실패"를 표시하며, 파일명은 문서 탭 제목(Windows/macOS 금지 문자·예약 이름 정리)을 따릅니다. 글자 단위 런 모델(`PD-077`)로 선택한 글자에만 적용한 글꼴·크기(반포인트)·글자 색·형광펜을 구조화 런(`sizeHalfPt`/`font`/`color`/`highlight`)으로 보내 DOCX(`w:rFonts`/`w:sz`/`w:color`/`w:highlight`·`w:shd`), HWPX(`charPr` 높이·글자색·음영·`fontRef`), ODT(자동 텍스트 스타일), RTF(`\fs`·글꼴/색 표)에 기록하고 다시 열 때 같은 런으로 복원합니다. 표 셀 안의 글자 런 서식은 저장하지 않습니다.
- **WebUI 슬라이드 쇼·슬라이드 관리 (`PD-080`)**: 슬라이드 모듈에서 `F5`는 처음부터, `Shift+F5`는 현재 슬라이드부터 전체 화면 슬라이드 쇼를 시작합니다(전체 화면을 쓸 수 없으면 창 안에서 동작, 텍스트 입력 중에는 브라우저 새로 고침 유지). `→`/`Space`/`PageDown`/클릭 다음, `←`/`PageUp` 이전, `Home`/`End`, `B`/`W` 화면 가리기, `P` 발표자 보기(노트·다음 슬라이드·타이머), `Esc` 종료. 슬라이드 추가(빈 개체 틀)·복제·삭제(실행 취소 가능)·끌어 놓기/`Alt+↑/↓`/우클릭 메뉴 순서 변경, 편집이 즉시 반영되는 썸네일(말줄임·툴팁), 16:9·4:3·A4 비율을 유지하는 스테이지를 제공합니다.
- **WebUI 파일 열기·가져오기 (`PD-075`)**: 파일 메뉴 "열기…"/`Ctrl+O`와 끌어 놓기로 자신의 문서를 열면 자기 형식을 매직 바이트로 판별해(ZIP 폭탄·경로 순회·XXE 가드 적용) 문서·한글·시트·슬라이드·PDF 모듈에 적재합니다(DOCX/DOC/ODT/RTF/MD/TXT → 문서, HWP/HWPX/HWT/HWTX → 한글, XLSX/XLS/ODS/CSV/TSV → 시트(모든 시트·수식), PPTX/PPT/PPSX/ODP → 슬라이드, PDF → PDF 모듈(텍스트 추출)). DOCX·HWPX·ODT·RTF·Markdown은 런 서식(DOCX·HWPX·ODT·RTF는 글꼴·크기·글자 색·형광펜 포함, `PD-077`)·정렬·제목·목록·표 위치를, 슬라이드는 제목·글머리·노트를 보존하고 DOC·HWP·PDF는 서식 없이 본문과 표만 가져오며, 그림·도형·머리말·셀 서식 등 가져오지 못한 항목은 열기 후 상태 표시줄에 알립니다. 열기는 하나의 실행 취소 단위이며 탭 제목이 파일 이름이 되어 저장 파일명과 자동 저장이 따릅니다. 대상 모듈에 저장하지 않은 편집이 있으면 확인하고, 파일은 10 MiB까지, 시트는 XLSX 전체 주소 범위(1,048,576행 × 16,384열, `PD-078`)까지 가져옵니다. 변환기에서 원본 파일을 고르면 편집기 텍스트가 아닌 그 파일의 바이트를 대상 형식으로 변환합니다. 손상·미지원·보안 차단 파일은 오류만 표시하고 아무것도 바꾸지 않습니다.
- **시트 편집 기본기 (`PD-078`)**: 셀을 고른 뒤 입력하면 내용을 덮어쓰고(`F2`·더블클릭은 편집), `Enter`/`Tab`/방향키로 이동하고 `Shift`·드래그로 범위를 선택하며 `Ctrl+C`/`X`/`V`로 TSV를 시스템 클립보드와 주고받습니다(수식의 단순 A1 상대 참조 이동). 상태 표시줄 집계는 선택 범위의 합계·평균·개수·최소·최대이고, 격자는 1,048,576행 × 16,384열을 가상 스크롤로 다루며 열 머리글을 끌어 너비를 바꿉니다. 숫자는 오른쪽 정렬·10자리 일반 서식으로 보이고 원본 값은 편집 시 그대로 나타납니다.
- **시트 셀 서식·표·보기·규칙 저장 (`PD-089`, `PD-193`~`PD-244`)**: 선택 범위에 적용한 글꼴 패밀리(`PD-244`)·굵게·기울임·밑줄·취소선·글자 크기·글자 색·단색 채우기·4면 테두리·가로/세로 맞춤·줄 바꿈·숫자 서식(`PD-193`·`PD-235`), 열 너비·행 높이·병합·틀 고정·그룹 접기·보기 상태(`PD-229`), 시트/통합 문서 보호(`PD-216`), 조건부 서식·데이터 유효성·자동 필터(`PD-238`), 표 60종+사용자 지정 스타일·통합 문서 테마·계산 열·표준 슬라이서 파트(`PD-231`·`PD-237`·`PD-242`·`PD-244`), 명시 셀 타입(`PD-241`)을 XLSX·ODS로 왕복 저장합니다.
- **PDF**: 한글은 사전 정의 CJK 글꼴 참조(`HYGoThic-Medium`, `Adobe-Korea1`, `UniKS-UCS2-H`)로 기록하여 뷰어가 설치된 한글 글꼴로 표시하며, 비ASCII `/Title`은 BOM을 붙인 UTF-16BE로 기록합니다.
- **CSV/TSV**: 출력에 UTF-8 BOM을 붙이고, 입력의 BOM은 제거하여 읽습니다.
- **DOC/PPT 규격 바이너리 쓰기 (`PD-090`)**: `.doc`은 `WordDocument` + `1Table`(조각 표 Clx, 문자·문단 FKP, 구역 속성), `.ppt`는 `Current User` + `PowerPoint Document`(슬라이드 컨테이너, persist directory, UserEditAtom)로 기록해 다른 워드·프레젠테이션 프로그램이 열 수 있습니다. 본문은 문단과 탭으로 이은 표 행이고, 슬라이드는 제목·글머리표 본문입니다. 서식·그림·노트 페이지는 기록하지 않으며 노트·테마·전환은 EasyOffice로 다시 열 때만 복원됩니다. 더 이상 호환성 제한 경고를 표시하지 않습니다.

---

## 2. 모듈별 세부 기능 명세 (Core Module Capabilities)

### 2.1 Document & HWP/HWPX (`OfficeDocumentModel`, `crates/easyoffice-*`)
- **문서 조판 및 다중 구역 레이아웃 (`DocumentSectionLayout`)**:
  - 용지 규격(`PaperSizePreset`: `a4`, `letter`, `b5`), 용지 방향(`PageOrientation`: `portrait`, `landscape`), 다단(`columnCount`: 1~4단), 상하좌우 여백(`marginTopPt`, `marginBottomPt`, `marginLeftPt`, `marginRightPt`) 조판.
  - 문단 블록(`ParagraphBlock`) 스타일 역할(`ParagraphStyleRole`: `title`, `heading1`, `heading2`, `heading3`, `body`, `quote`, `officialHeader`)과 문단 정렬(`TextAlignmentMode`: `leading`, `center`, `trailing`, `justified`), 글꼴 이름·크기·굵게·기울임·밑줄 제어.
  - 제목 스타일(`title`, `heading1`~`heading3`, `officialHeader`) 문단 기반 실시간 개요(`outlineHeadings`) 및 목차(`TableOfContentsEntry`, `generateTableOfContents()`) 자동 추출, 본문 찾기/바꾸기(`findAndReplace(search:replacement:caseSensitive:)`).
- **표(`DocumentTableBlock`) 및 공문서/각주 편집**:
  - 셀 병합(`TableCellSpan`, `mergeCells(startRow:startCol:rowSpan:colSpan:)`), 셀 분할(`splitCell(row:col:)`), 테두리 스타일(`TableCellBorderStyle`: `solid`, `doubleLine`, `dashed`, `diagonalDown`, `diagonalUp`, `none`), 인라인 표 수식 평가(`evaluateTableFormula(_:atRow:column:)`: `=SUM(ABOVE)`, `=AVERAGE(ABOVE)`, `=SUM(LEFT)`, `=AVERAGE(LEFT)`, `=COUNT(ABOVE)`, `=MAX(ABOVE)`, `=MIN(ABOVE)`).
  - 한글 문서(`HWP 5.0`/`HWPX`) 특화 구성: 공문서 두문(`officialHeader`), 머리말(`headerText`)/꼬리말(`footerText`), 각주/미주(`FootnoteItem`), 북마크(`bookmarks`), 결재란 이중선(`doubleLine`) 표 양식 호환.
- **검토, 메일 머지 및 통계 도구**:
  - 각주(`각주`)·미주(`미주`) 실시간 번호 부여·본문 양방향 이동(`RichNote` · `noteRef`, `DOCX` `word/footnotes.xml`·`word/endnotes.xml` · `ODT` `text:note` 왕복 및 `HWPX` `hp:footNote`·`hp:endNote` 기록, `PD-252`), 새 메모(`RichComment` · `commentId`, `DOCX` `word/comments.xml` · `ODT` `office:annotation` 왕복, `PD-252`), 변경 내용 추적(`RichRevision`: 삽입 `ins`·삭제 `del` 실시간 기록 및 개별/전체 `수락`·`거부`, `DOCX` `w:ins`/`w:del` · `ODT` `text:tracked-changes` 왕복, `PD-252`).
  - 메일 머지(`applyingMailMerge(record:)`): `{{필드명}}` 플레이스홀더에 레코드 사전을 바인딩하여 안내문·계약서·공문서 일괄 생성.
  - 실시간 문서 통계(`DocumentStatistics`, `computeStatistics()`): 단어 수(`wordCount`), 글자 수(`characterCountWithSpaces`, `characterCountWithoutSpaces`), 문단 수(`paragraphCount`), 표 수(`tableCount`), 200자 원고지 매수(`manuscriptPages200`).
  - 우측 `코파일럿` 대화 사이드바 및 인플레이스 오피스 실행(`PD-253`): `DOC`·`SHEET`·`SLIDE`·`PDF` 모듈 공통 우측 리사이즈 패널(`280px..520px`), `편집`/`질문` 모드 분리, 시트 수식·백분율·격차 행 강조 및 문서 문체 교정·블록 삽입·개인정보 마스킹·슬라이드 생성 인플레이스 실행(`제자리에서 업데이트됨` 배지·위치 칩 클릭 이동·단일 `실행 취소`), 12종 회사별·로컬 AI 제공자 카탈로그·추론 수준 및 SSRF 방어 메모리 전용 API 키 연동.

### 2.2 Sheet (`WorksheetModel`, `crates/easyoffice-*`)
- **64비트 비트패킹 좌표 및 고성능 그리드 (`PackedCellCoordinate`)**:
  - 행(상위 32비트)과 열(하위 32비트)을 단일 `UInt64`에 패킹한 `PackedCellCoordinate`와 ECMA-376 경계(`maxZeroBasedColumn = 16,383`, `maxZeroBasedRow = 1,048,575`)를 검증하는 `CellAddress`, 범위 확장 상한(`maxExpandedCellCount = 100,000`)을 강제하는 `CellRange`를 제공합니다.
  - 범위 그리드 일괄 평가(`evaluateGrid(in:)`), 열 기준 정렬(`sortedRows(in:byColumnOffset:ascending:)`), 조건 필터링(`filteredRows(in:byColumnOffset:predicate:)`), 조건부 서식(`ConditionalFormatRule`: `greaterThan`, `lessThan`, `between`, `containsText`, `duplicateValues`), 데이터 유효성 검사(`DataValidationRule`: `numberRange`, `allowedList`, `nonEmptyText`, `maxTextLength`)를 지원합니다.
- **결정론적 수식 엔진 (288종 내장 함수, `WorksheetModel.supportedFunctionNames`)**
  - 수학/삼각 및 조건부 집계 (36종) `ABS`, `AVERAGE`, `AVERAGEIF`, `AVERAGEIFS`, `CEILING`, `COUNT`, `COUNTA`, `COUNTBLANK`, `COUNTIF`, `COUNTIFS`, `EXP`, `FLOOR`, `INT`, `LN`, `LOG10`, `MAX`, `MAXIFS`, `MIN`, `MINIFS`, `MOD`, `MROUND`, `PI`, `POWER`, `PRODUCT`, `QUOTIENT`, `ROUND`, `ROUNDDOWN`, `ROUNDUP`, `SIGN`, `SQRT`, `SUM`, `SUMIF`, `SUMIFS`, `SUMPRODUCT`, `SUMSQ`, `TRUNC`
  - 통계 (18종) `AVEDEV`, `CORREL`, `FORECAST`, `GEOMEAN`, `HARMEAN`, `INTERCEPT`, `LARGE`, `MEDIAN`, `MODE`, `PERCENTILE`, `QUARTILE`, `RANK`, `SLOPE`, `SMALL`, `STDEV`, `STDEVP`, `VAR`, `VARP`
  - 논리 및 정보 (15종) `AND`, `FALSE`, `IF`, `IFERROR`, `IFNA`, `IFS`, `ISBLANK`, `ISERROR`, `ISNUMBER`, `ISTEXT`, `NOT`, `OR`, `SWITCH`, `TRUE`, `XOR`
  - 찾기/참조 (11종) `CHOOSE`, `COLUMN`, `COLUMNS`, `HLOOKUP`, `INDEX`, `MATCH`, `OFFSET`, `ROW`, `ROWS`, `VLOOKUP`, `XLOOKUP`
  - 텍스트 (19종) `CONCAT`, `CONCATENATE`, `EXACT`, `FIND`, `LEFT`, `LEN`, `LOWER`, `MID`, `PROPER`, `REPLACE`, `REPT`, `RIGHT`, `SEARCH`, `SUBSTITUTE`, `TEXT`, `TEXTJOIN`, `TRIM`, `UPPER`, `VALUE`
  - 재무 (11종) `EFFECT`, `FV`, `IRR`, `NOMINAL`, `NPER`, `NPV`, `PMT`, `PV`, `RATE`, `SLN`, `SYD`
  - 날짜/시간 (12종) `DATE`, `DATEDIF`, `DAY`, `DAYS`, `EDATE`, `EOMONTH`, `MONTH`, `NETWORKDAYS`, `WEEKDAY`, `WEEKNUM`, `WORKDAY`, `YEAR`
  - 방문 집합 추적 및 최대 재귀 깊이(`maxRecursionDepth = 64`), 문자열 반복 상한(`maxReptResultUTF16Units = 65_536`), 셀 문자열 상한(`maxCellTextLength = 32_767`) 가드로 순환 참조 및 수식 오류(`EvaluatedCellValue.error`)를 크래시 없이 결정론적으로 반환합니다.
- **피벗 테이블 및 차트 엔진 (`PivotTableEngine`, `SheetChartBuilder`)**:
  - `DynamicPivotSpec`·`SheetPivotTable` 기반 다중 행·열·보고서 필터·다중 값 집계(`sum`·`average`·`count`·`min`·`max`), `pivotCache` 공유 및 제자리 `새로 고침`(`Alt+F5`)·`모두 새로 고침`, 피벗 슬라이서 교차 필터링, `XLSX` `xl/pivotCache/`·`xl/pivotTables/`·피벗 `slicerCaches` 양방향 왕복(`PD-255`).
  - `SheetChartBuilder.buildChart`를 통한 7종 차트(`SheetChartKind`: `column`, `bar`, `line`, `area`, `pie`, `scatter`, `stackedColumn`) 시리즈(`SheetChartSeries`) 생성.

### 2.3 Slide (`PresentationDeckModel`, `crates/easyoffice-*`)
- **슬라이드 마스터 및 화면 비율**:
  - 화면 비율(`SlideAspectRatio`: `widescreen16x9` 960×540pt, `standard4x3` 720×540pt, `handoutA4` 841.89×595.28pt) 및 마스터 레이아웃(`SlideMasterLayout`: `titleSlide`, `titleAndContent`, `sectionHeader`, `twoColumnComparison`, `blankCanvas`).
- **스마트 정렬 가이드 및 캔버스 개체 제어 (`SnapGuideCalculator`)**:
  - `SlideCanvasElement`(`SlideElementKind` 8종: `textBox`, `rectangleShape`, `roundedRectangleShape`, `ellipseShape`, `arrowShape`, `calloutShape`, `tableGrid`, `chartEmbed`) 배치, Z-Order 제어(`bringElementToFront(id:)`, `sendElementToBack(id:)`), 스테이지 중앙 정렬(`centerGuides(for:tolerance:)`).
  - `SnapGuideCalculator.computeGuides(movingElement:siblings:aspectRatio:tolerancePoints:)`를 통해 스테이지 중앙선, 형제 개체 모서리/중심선 및 3개 이상 개체 간 균등 간격 스냅 가이드(`AlignmentSnapGuide`)를 실시간 계산합니다.
- **전환 효과 및 발표자 세션 (`PresenterSession`)**:
  - 슬라이드 전환(`SlideTransitionKind` 4종: `none`, `fade`, `push`, `morph`)과 슬라이드 추가·복제·순서 이동(`moveSlide(from:to:)`), 현재/다음 슬라이드·발표자 노트·진행률(`progressFraction`)·경과 시간 타이머(`tick(seconds:)`)를 관리하는 `PresenterSession`.

### 2.4 PDF Studio (`PDFStudioDocumentModel`, `crates/easyoffice-*`)
- **열람, 북마크 및 본문 검색**:
  - `PDFBinarySerializer` 기반 ISO 32000-1 `%PDF-1.7` 호환 바이너리 직렬화·파싱(`encode(_:)`, `decode(from:)`), 북마크 트리(`PDFOutlineNode`), 페이지별 텍스트 추출 및 대소문자 무시 본문 검색(`searchPages(containing:)`).
- **5종 주석 및 서명 (`PDFAnnotationItem`)**:
  - `PDFAnnotationKind`: `highlight`(형광펜), `underline`(밑줄), `stickyNote`(스티커 메모), `stamp`(도장), `signature`(전자 서명).
- **페이지 조작 및 양방향 오피스 변환**:
  - 페이지 90도 단위 회전(`rotatePage(at:clockwiseDegrees:)`), 빈 페이지 삽입(`insertBlankPage(at:)`), 페이지 삭제(`deletePage(at:)`), 순서 재배치(`reorderPage(from:to:)`), 지정 페이지 추출(`extractPages(oneBasedPageNumbers:newTitle:)`), 분할(`split(atOneBasedPage:)`), 다중 PDF 병합(`merge(documents:mergedTitle:)`).
  - `convertToEditableDocument(targetFormat:)`, `convertToPresentation()`, `fromDocument(_:)`, `fromWorksheet(_:range:)`, `fromPresentation(_:)`를 통한 오피스 문서·시트·슬라이드 ↔ PDF 양방향 변환.

### 2.5 시작 화면·변환·오프라인 스마트 도구 (`crates/easyoffice-app`)
- **시작 화면과 창 세션**: 인수 없이 실행하면 홈 화면(`ui/home-screen.js`: 새 문서 6종·열기·최근 문서 카드)이 뜨고, 문서 하나는 창 하나입니다(`PD-088`). 창별 편집 상태는 `ui/workspace-session.js`가 맡고 모듈별 편집기 패널은 WebUI `ui/`에 있습니다.
- **포맷 변환 및 구조화 리드백(`PD-248`)**: IPC 명령 `convert_document_format`(`convert_ipc.rs`)이 20종 포맷과 Pages 사이를 변환하며 `"action": "inspect"`(`"readback"`)와 `evaluate_sheet` `"action"`·`"sessionAction": "inspect"` 및 `easyoffice-app --inspect <FILE>`(`--sheet`·`--range`·`--max-items`) CLI가 시트 A1 범위·수식·표시 형식·동적 배열·오류 히스토그램·데이터 유효성 위반, 문서·Pages·한글 블록·개요·표 수식·내부 북마크·대체 텍스트 무결성, 슬라이드 밀도, PDF 쪽 규격·북마크·링크·주석·대화형 양식 수를 단일 패스 JSON으로 점검합니다.
- **100% 오프라인 스마트 보조 (`LocalSmartAssistant`, `smart_tools.rs`)**: 외부 서버 통신 없이 핵심 문장 요약(`summarize_document`), 문장 진단(`diagnose_writing`), 조치 항목 추출(`extract_action_items`), LCS 문서 비교(`diff_documents_lcs`)를 제공합니다.
- **PII 비식별화 (`DocumentSecurityAuditor`)**: `scan_and_scrub_pii`·`scan_and_scrub_pii_with_metadata`가 주민등록번호·외국인등록번호·연락처·이메일·신용카드(Luhn 검증)·은행 계좌·여권·운전면허·사업자등록번호를 마스킹하고, 메타데이터 항목을 제거한 개수를 보고합니다.

---

## 3. 플랫폼 배포 형태 (Platform Delivery, `PD-074`)
- **macOS (`PD-094`)**: Windows와 같은 Tauri 데스크톱 셸(WKWebView)을 유니버설 `.dmg`(`EasyOffice-macos-universal.dmg`, macOS 11 이상)로 배포합니다. Apple Developer ID 서명·공증은 하지 않으므로 처음 한 번 우클릭 → 열기(또는 시스템 설정 → 개인정보 보호 및 보안 → 그래도 열기)가 필요합니다. 표준 메뉴 막대와 Command 단축키가 동작하고 Finder에서 문서를 열 수 있습니다.
- **Windows**: 설치 프로그램 `EasyOffice-windows-x64-setup.exe`(NSIS, 사용자 단위 설치, 시작 메뉴 바로 가기, 제거 프로그램)로 배포합니다. 설치된 `EasyOffice.exe`는 Windows 내장 WebView2 위의 데스크톱 창(Tauri v2 셸)이며, 실행 파일에 포함된 WebUI가 프로세스 안의 Rust 엔진(15종 IPC 명령)을 직접 호출합니다. 로컬 HTTP 서버·브라우저·네트워크 연결 없이 동작하고, 저장은 Windows 기본 저장 대화상자로 위치를 고릅니다(취소·실패는 저장 상태 표시에 그대로 보고). 문서 파일 연결(더블클릭으로 열기)은 `PD-093`에 따라 설치 프로그램이 등록합니다.
- **Linux (`PD-086`)**: 같은 Tauri 셸(WebKitGTK 4.1)을 x64는 `EasyOffice-linux-x64.AppImage`·`EasyOffice-linux-x64.deb`·`EasyOffice-linux-x64.rpm`, arm64는 `EasyOffice-linux-arm64.deb`로 배포합니다(Ubuntu 22.04 기준 빌드, glibc 2.35 이상). 네이티브 패키지가 WebKitGTK·GTK 3·libxdo를 의존성으로 선언하고, AppImage는 설치 없이 실행하는 용도입니다. 메뉴 분류는 Office이고 문서 9종을 "다음으로 열기" 대상으로 등록합니다. 한글 입력기와 WebKitGTK는 배포판 패키지와 함께 쓸 때 가장 안정적입니다.
- **릴리스 자산과 공개 (`PD-086`)**: 자산 이름에 버전이 없어(`releases/latest/download/<이름>`) README 직링크가 릴리스마다 유지되고, 체크섬은 합친 `SHA256SUMS.txt` 하나입니다. 릴리스는 드래프트로 만들어 QA가 확인한 뒤 공개하며, 업데이트 알림(`PD-103`)은 공개된 릴리스만 봅니다. 모든 OS 산출물에 `THIRD_PARTY_NOTICES.md`가 들어갑니다.
- **한글(HWP) 편집기 자산 (`PD-081`)**: 편집기(rhwp 엔진의 미리 빌드한 WASM + 임베드 전용 rhwp-studio 오버레이)와 번들 가능한 OFL 글꼴 28종(약 18 MiB)이 저장소에 고정되어 있고, 단독 하니스에서 빈 문서 → 한글 입력 → 선택 범위만 글자 크기 변경 → HWP 내보내기 → 다시 열기에서 같은 글자 모양이 유지됨을 검증합니다. `PD-083`부터 한글 모듈이 이 편집기를 불투명 출처(`allow-same-origin` 없음) iframe으로 임베드합니다(데스크톱 셸은 실행 파일에 내장, `--serve`는 디스크 또는 `embed-hwp-studio` 기능으로 내장). 빈 문서에서 시작해 한글 입력·선택한 글자만 크기·굵게·표 도구를 쓰고 HWP 5.0으로 저장할 수 있으며, 번들이 없는 빌드는 기존 텍스트 전용 편집기와 안내 문구로 대체됩니다. `PD-085`부터 HWP·HWPX·HWT·HWTX를 열 수 있고(최대 64 MiB, 암호 문서는 편집기의 비밀번호 대화상자, DRM·미지원 암호화는 거부, 편집기가 못 여는 파일은 서식 유실 안내와 함께 텍스트만 열기), HWP·HWPX로 저장할 수 있으며(저장 전 재파싱 검증, HWPX 검증 실패 시 HWP 저장 제안, 취소·실패 시 수정 상태 유지), 수정 상태가 이탈 경고에 반영되고 IndexedDB 자동 저장본이 새로고침 후 복원 확인을 거쳐 복원됩니다. 변환기는 열려 있는 한글 문서를 편집기가 내보낸 바이트로 변환합니다.
- **문서별 개별 창·셸 정리 (`PD-088`)**: Windows 데스크톱 셸에서 문서 하나는 창 하나입니다(모듈 전환 탭 없음). 인수 없이 실행하면 일반 크기의 시작 창(홈 화면: 새 문서 6종·열기·최근 문서 카드)이 뜨고, 새 문서·열기·최근 문서·끌어 놓기는 새 창에서 열리며 같은 파일을 다시 열면 열린 창을 앞으로 가져옵니다. 창 제목은 `파일 이름 — EasyOffice`와 수정 표시(`●`)이고, 저장하지 않은 창을 닫으면 저장 / 저장 안 함 / 취소를 묻습니다. 실행 취소·자동 저장·수정 상태는 창 단위입니다. 경로는 Rust에만 있고 WebView에는 이름과 바이트만 갑니다. 한글 편집기 창은 rhwp 도구모음과 메뉴 줄 하나만 보이고(`PD-093`: 셸 메뉴 줄·상태 표시줄 숨김, 편집기 `파일` 메뉴는 새로 만들기·열기·최근 문서·저장·다른 이름으로 저장·닫기 셸 명령, 셸 테마를 편집기에 전달, 알림은 토스트) `Ctrl+S/O/N/K/W`는 편집기 포커스에서도 셸로 전달됩니다. 상태 표시줄은 쪽·위치·입력 모드·파일 이름·메시지·확대만 보여 주고 진단 정보는 `도움말 ▸ 제품 정보`에 있으며, 시작 창 카드는 형식별 아이콘과 모듈 색을 씁니다. 셸 아이콘은 SVG 한 벌이고 모든 컨트롤에 접근성 이름이 있으며 라이트·다크 모두 명암비 AA입니다.
- **개발자 전용 `--serve` (`PD-086`)**: `easyoffice-app --serve [PORT]`는 같은 WebUI를 `127.0.0.1` 루프백 서버로 띄우는 개발·자동 검증(헤드리스 브라우저 UI 검증, CI 스모크) 전용 경로입니다. 일반 사용자에게 배포하지 않으며 릴리스 자산은 OS별 설치 파일과 체크섬 파일뿐입니다.
- **데스크톱 파일 수명주기 (`PD-093`)**: Windows 설치 프로그램이 HWP·HWPX·DOCX·ODT·XLSX·ODS·PPTX·ODP·PDF를 연결해 더블클릭하면 이미 실행 중인 앱의 새 창(이미 열린 파일이면 그 창)으로 열립니다(단일 인스턴스). 저장은 열었던 파일(같은 형식)에 임시 파일 + 이름 바꾸기로 제자리에 쓰며, 열린 뒤 다른 프로그램이 파일을 바꿨으면 덮어쓰기 여부를 묻습니다. 다른 이름으로 저장·다른 형식 저장은 저장 대화상자를 씁니다.
