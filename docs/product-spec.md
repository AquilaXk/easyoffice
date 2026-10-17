# EasyOffice 통합 제품 및 기능 명세 (Product Specification)

이 문서는 `EasyOffice`가 macOS 환경에서 제공하는 전체 포맷 호환성 매트릭스와 5대 핵심 모듈(`Document`, `Sheet`, `Slide`, `HWP/HWPX`, `PDF Studio`) 및 공통 생산성 기능의 목표 명세를 정의합니다.

---

## 1. 지원 포맷 매트릭스 (Supported Formats)

| 카테고리 | 확장자 | 기술 표준 / 컨테이너 규격 | 지원 범위 |
| :--- | :--- | :--- | :--- |
| **Document (워드)** | `.docx`, `.doc`, `.odt`, `.rtf`, `.md`, `.txt` | ECMA-376 OOXML WordprocessingML, OLE2 Word, ISO/IEC 26300 ODF Text, RTF, CommonMark, UTF-8 | 열람 · 편집 · 저장 · PDF/HTML/MD 내보내기 |
| **HWP (한글 문서)** | `.hwp`, `.hwpx`, `.hwt`, `.hwtx` | HWP 5.0 Binary (OLE2 CFBF `FileHeader`/`DocInfo`/`BodyText`), KS X 6101 OWPML (ZIP+XML) | 열람 · 편집 · 서식 양식(HWT/HWTX) · HWPX/PDF/DOCX 변환 |
| **Sheet (스프레드시트)** | `.xlsx`, `.xls`, `.ods`, `.csv`, `.tsv` | ECMA-376 SpreadsheetML, BIFF8 OLE2, ODF Spreadsheet, RFC 4180 CSV/TSV | 열람 · 편집 · 수식 연산 · 피벗/차트 · CSV/PDF 내보내기 |
| **Slide (프레젠테이션)** | `.pptx`, `.ppt`, `.ppsx`, `.odp` | ECMA-376 PresentationML, OLE2 PowerPoint, ODF Presentation | 열람 · 편집 · 슬라이드 마스터 · 발표자 뷰 · PDF/이미지 내보내기 |
| **PDF Studio** | `.pdf` | ISO 32000-1 / 32000-2 (PDF 1.2 ~ 2.0) | 열람 · 주석/하이라이트/서명 · 페이지 병합/분할 · Office 포맷 변환 |

---

## 2. 모듈별 세부 기능 명세 (Core Module Capabilities)

### 2.1 Document & HWP/HWPX (워드프로세서 및 한글 문서 엔진)
- **문서 조판 및 레이아웃**:
  - 용지 규격(A4, Letter, B5, 사용자 정의), 여백, 다단(Multi-column) 조판, 구역(Section) 나누기, 가로/세로 방향 혼용.
  - 글자 모양(글꼴, 크기, 굵게, 기울임, 밑줄, 취소선, 글자색, 음영, 자간, 장평, 첨자) 및 문단 모양(정렬, 줄간격, 문단 앞/뒤 간격, 첫 줄 들여쓰기/내어쓰기, 탭 설정).
  - 스타일 시스템(제목 1~6, 본문, 인용, 코드 블록, 사용자 정의 스타일) 및 실시간 목차(Table of Contents) 자동 생성.
- **표(Table) 및 개체 편집**:
  - 셀 병합/분할, 대각선 선 그리기, 셀 테두리 종류/두께/색상, 셀 배경 음영, 표 자동 채우기 및 표 내부 계산식(합계/평균).
  - 한글 문서(`HWP 5.0`/`HWPX`) 특화 기능: 글상자, 누름틀(필드), 머리말/꼬리말, 각주/미주, 쪽 테두리/배경, 결재란 표 양식 완벽 호환.
- **검토 및 자동화 도구**:
  - 변경 내용 추적(Track Changes) 및 메모(Comments) 스레드, 문서 비교.
  - 메일 머지(Mail Merge): CSV/XLSX 데이터 소스를 바인딩하여 안내문·계약서·공문서 일괄 생성.
  - 맞춤법/문법 검사, 단어·글자 수(공백 포함/제외)·원고지 매수 실시간 통계.

### 2.2 Sheet (스프레드시트 및 데이터 분석 엔진)
- **고성능 그리드 및 셀 서식**:
  - 가상화(Virtualized) 2D 캔버스 기반 대용량 행/열 스크롤, 틀 고정(Freeze Panes), 행/열 그룹화 및 숨기기, 셀 병합.
  - 표시 형식(통화, 회계, 백분율, 날짜/시간, 지수, 사용자 정의 포맷 마스크) 및 조건부 서식(색조, 데이터 막대, 아이콘 집합, 수식 규칙).
- **결정론적 수식 엔진 (300+ Built-in Functions)**:
  - 수학/삼각(`SUM`, `SUMIF`, `SUMIFS`, `PRODUCT`, `ROUND`, `ROUNDUP`, `ROUNDDOWN`, `ABS`, `MOD`, `SQRT`, `POWER` 등)
  - 통계(`AVERAGE`, `AVERAGEIF`, `COUNT`, `COUNTA`, `COUNTIF`, `MEDIAN`, `STDEV`, `VAR`, `MIN`, `MAX`, `LARGE`, `SMALL`, `RANK` 등)
  - 논리(`IF`, `IFS`, `AND`, `OR`, `NOT`, `XOR`, `IFERROR`, `IFNA`, `SWITCH`)
  - 찾기/참조(`VLOOKUP`, `HLOOKUP`, `XLOOKUP`, `INDEX`, `MATCH`, `OFFSET`, `INDIRECT`, `CHOOSE`)
  - 텍스트(`CONCAT`, `TEXTJOIN`, `LEFT`, `RIGHT`, `MID`, `LEN`, `TRIM`, `UPPER`, `LOWER`, `SUBSTITUTE`, `FIND`, `TEXT`)
  - 날짜/시간 및 재무(`TODAY`, `NOW`, `DATE`, `YEAR`, `MONTH`, `DAY`, `DATEDIF`, `NETWORKDAYS`, `PMT`, `FV`, `PV`, `NPV`, `IRR`)
  - 의존성 방향 그래프(DAG) 기반 재계산 및 순환 참조(`#REF!`, `#NAME?`, `#DIV/0!`, `#VALUE!`, `#N/A`, `#NUM!`) 검출.
- **피벗 테이블, 차트 및 데이터 도구**:
  - 피벗 테이블(행/열/값/필터 드래그 배치, 요약 집계 및 부분합).
  - 2D/3D 차트(막대, 꺾은선, 원형/도넛, 영역, 분산형, 콤보 차트).
  - 다중 기준 정렬, 자동 필터, 중복된 항목 제거, 텍스트 나누기, 데이터 유효성 검사(드롭다운 목록, 숫자/날짜 범위 제한).

### 2.3 Slide (프레젠테이션 및 슬라이드 제작 엔진)
- **슬라이드 마스터 및 레이아웃**:
  - 슬라이드 마스터(Slide Master) 및 표준 레이아웃(제목 슬라이드, 제목 및 내용,구역 머리글, 2단 비교, 빈 화면), 화면 비율(16:9 와이드스크린, 4:3 표준, A4 유인물) 지원.
- **스마트 캔버스 및 개체 조작**:
  - 스마트 정렬 가이드(Smart Guides: 개체 간 중심선·등간격 스냅), 도형/화살표/순서도 라이브러리, 레이어 순서(맨 앞으로/뒤로), 그룹화, 도형 병합(결합/교차/빼기).
  - 텍스트 상자, 표, 차트, 고해상도 이미지(클리핑 마스크/배경 투명화), 비디오/오디오 미디어 삽입.
- **전환 효과 및 발표자 도구**:
  - 슬라이드 전환(페이드, 밀어내기, 모핑 등) 및 개체 애니메이션 타임라인.
  - 듀얼 모니터 **발표자 보기(Presenter View)**: 현재 슬라이드, 다음 슬라이드 미리보기, 발표자 노트(Speaker Notes), 경과 타이머, 레이저 포인터/펜 판서 오버레이.

### 2.4 PDF Studio (PDF 열람 · 주석 · 편집 · 상호 변환)
- **열람 및 네비게이션**:
  - 고속 벡터 렌더링, 목차(Bookmarks) 트리, 썸네일 그리드, 단면/양면 연속 보기, 본문 정규식/키워드 하이라이트 검색.
- **주석(Annotations) 및 서명**:
  - 형광펜 하이라이트, 밑줄, 취소선, 스티커 메모, 텍스트 상자, 자유형 펜 그리기, 도장(Stamp) 및 전자 서명(Signature) 삽입.
- **페이지 조작 및 양방향 변환**:
  - 페이지 회전, 순서 재배치, 페이지 추출/분할(Split), 다중 PDF 병합(Merge).
  - **PDF → 편집 가능 문서 변환**: PDF 내 텍스트 흐름·표·이미지를 분석하여 `DOCX`, `XLSX`, `PPTX`, `HWPX`, `TXT`로 변환.
  - **문서 → PDF 변환**: 모든 오피스 문서를 글꼴 임베딩 및 북마크가 보존된 표준 PDF로 즉시 내보내기.

### 2.5 통합 워크스페이스 허브 & 스마트 보조 도구
- **홈 허브 (Home Hub)**:
  - 최근 문서, 즐겨찾기(고정) 문서, 파일 포맷별 필터링, 내장 서식 템플릿 갤러리(`My Templates` 사용자 템플릿 등록 포함).
  - 파일 이름과 문서 내부 본문/셀/노트를 아우르는 로컬 고속 통합 색인 검색.
- **온디바이스 스마트 작성 및 요약 도구 (Local Smart Assistant)**:
  - 외부 서버로 문서를 전송하지 않는 프라이버시 우선 구조로, 문서 핵심 요약, 문장 다듬기/교정, 표 데이터 요약, 다국어 번역 보조 인터페이스를 에디터 우측 패널에 제공.
