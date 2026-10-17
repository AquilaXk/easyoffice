# EasyOffice

[English](README.md) | [한국어](README.ko.md)

EasyOffice는 Swift 6, SwiftUI, AppKit 기반으로 설계된 **macOS 네이티브 올인원 오피스 스위트**입니다. 시그니처 라벤더 디자인 시스템(`#5C6BC0`)을 바탕으로 **워드 문서(Document)**, **스프레드시트(Sheet)**, **프레젠테이션(Slide)**, **한글 문서(HWP / HWPX)**, **PDF 스튜디오(PDF Studio)**를 단일 멀티 탭 윈도우에서 통합 제공하며, 클라우드 종속 없이 빠르고 정확한 로컬 문서 편집 및 상호 변환을 지원합니다.

---

## 핵심 특징

- **5대 오피스 모듈 올인원 통합 워크스페이스**: 문서 작성, 표 계산 및 피벗 분석, 슬라이드 발표자료 제작, 한글(HWP 5.0/HWPX) 공문서 편집, PDF 주석 및 변환을 하나의 네이티브 macOS 앱에서 탭으로 전환하며 수행합니다.
- **폭넓은 표준 포맷 호환성**:
  - **문서 (Document)**: `.docx`, `.doc`, `.odt`, `.rtf`, `.md`, `.txt`
  - **한글 문서 (HWP)**: `.hwp` (5.0 OLE2 복합 문서), `.hwpx` (KS X 6101 OWPML 표준), `.hwt`, `.hwtx`
  - **시트 (Sheet)**: `.xlsx`, `.xls`, `.ods`, `.csv`, `.tsv`
  - **슬라이드 (Slide)**: `.pptx`, `.ppt`, `.ppsx`, `.odp`
  - **PDF 스튜디오**: `.pdf` (ISO 32000-1 / 32000-2)
- **컨텍스트 리본 + 우측 속성 인스펙터 결합 UX**: 현재 열린 문서 포맷에 맞춰 자동 전환되는 상단 컴팩트 리본 툴바와 macOS 고유의 우측 세부 속성 인스펙터, 좌측 구조 내비게이터(썸네일/목차/시트 목록/슬라이드 정렬)를 결합했습니다.
- **결정론적 수식 연산 및 데이터 분석 엔진**: A1 셀/범위 파서, 핵심 수학·통계·논리·텍스트 수식 계산기(`SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT`, `COUNTA`, `IF`, `ROUND`, `ABS`, `CONCAT` 등), 피벗 테이블 및 차트 모델을 내장합니다.
- **양방향 PDF 및 포맷 일괄 변환 스튜디오**: 모든 오피스 문서를 표준 PDF로 즉시 내보내거나, PDF에서 편집 가능한 문서 구조를 추출합니다.
- **Fail-Closed 컨테이너 보안**: 확장자만 신뢰하지 않고 바이너리 매직 시그니처(`PK\x03\x04`, `\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1`, `%PDF-`, UTF-8 유효성)와 압축 해제 한도를 선제 검증하여 손상된 파일이나 압축 폭탄을 차단합니다.
- **시그니처 라벤더 디자인 시스템**: 브랜드 시그니처 라벤더 액센트(`#5C6BC0`)와 5대 모듈 시맨틱 컬러, 라이트/다크 모드 완벽 대응으로 장시간 문서 작업에서도 눈의 피로를 최소화합니다.

---

## 지원 포맷 매트릭스

| 모듈 | 지원 확장자 | 기술 표준 / 컨테이너 규격 | 핵심 기능 |
| :--- | :--- | :--- | :--- |
| **Document** | `DOCX`, `DOC`, `ODT`, `RTF`, `MD`, `TXT` | ECMA-376 WordprocessingML, ISO/IEC 26300 ODF, CommonMark | 다단/구역 조판, 문단/글자 스타일, 표 편집, 변경 내용 추적, 메일 머지 |
| **HWP (한글)** | `HWP`, `HWPX`, `HWT`, `HWTX` | HWP 5.0 OLE2 CFBF, KS X 6101 OWPML (ZIP+XML) | 한글 문서 열람·편집, 공문서 결재란/표 양식, 머리말/꼬리말, 각주, HWPX/PDF 변환 |
| **Sheet** | `XLSX`, `XLS`, `ODS`, `CSV`, `TSV` | ECMA-376 SpreadsheetML, ODF Spreadsheet, RFC 4180 | 2D 그리드, 수식 계산 엔진, 피벗 테이블/차트, 다중 정렬·필터·유효성 검사 |
| **Slide** | `PPTX`, `PPT`, `PPSX`, `ODP` | ECMA-376 PresentationML, ODF Presentation | 슬라이드 마스터, 16:9·4:3 캔버스, 스마트 정렬 가이드, 전환 효과, 발표자 노트 뷰 |
| **PDF Studio** | `PDF` | ISO 32000-1 / 32000-2 | 고속 벡터 열람, 하이라이트·주석·서명, 페이지 병합/분할/재배치, 오피스 포맷 변환 |

---

## 빠른 시작 (Quick Start)

### 시스템 요구 사항
- **운영체제**: macOS 14.0 (Sonoma) 이상 (Apple Silicon 및 Intel 지원)
- **개발 도구**: Xcode 16+ / Swift 6.0+

### 로컬 빌드 · 테스트 · 실행

```bash
# 전체 모듈 빌드
swift build

# 결정론적 단위 및 통합 테스트 실행
swift test

# macOS 네이티브 앱 실행
swift run EasyOfficeApp

# 배포용 EasyOffice.app 번들, .dmg, .zip 및 SHA256 체크섬 패키징
bash scripts/package-macos-release.sh 0.1.0
```

---

## 라이선스

[MIT License](LICENSE)에 따라 배포됩니다.
