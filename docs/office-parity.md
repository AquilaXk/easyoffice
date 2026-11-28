# 기능 범위 기준표 (office-parity)

- 측정: 기준 기능은 오픈소스 오피스 제품 4종 중 3종 이상이 가진 사용자 기능이다(슬라이드는 해당 제품이 3종이라 기준 수의 분모가 3). 기능 단위는 명령·대화상자·동작이다.
- 점수: 있음 = 1, 일부 = 0.5, 없음 = 0. 제외는 분모에서 뺀다. 완성도 = 점수 합 ÷ (있음 + 일부 + 없음).
- 근거: 화면 메뉴(`index.html`), 리본 목록(`ribbon-catalog.js`), 대화상자 스크립트, `product-spec.md`, `format-support.md`, `policy-decisions.md`. 위치의 `ui/`는 `crates/easyoffice-app/ui/`다. 기능 이름은 MS Office 한국어판 표기를 따르고, 우리 표기가 다르면 메모에 적는다.

이 표는 구현 순서와 완성도의 기준이다(PD-393). 상태는 화면 점검 기준이라, 구현이 바뀌면 같은 커밋에서 표를 고친다. 한글(HWP)·PDF·BASE·BOARD는 이 표에 넣지 않는다.

## 요약

| 모듈 | 기준 기능 | 있음 | 일부 | 없음 | 제외 | 완성도 |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: |
| 문서 | 68 | 53 | 11 | 4 | 3 | 86.0% |
| 시트 | 59 | 51 | 2 | 6 | 5 | 88.1% |
| 슬라이드 | 47 | 31 | 11 | 5 | 3 | 77.7% |
| 전체 | 174 | 135 | 24 | 15 | 11 | 84.5% |

## 문서

| 영역 | 기능 | 기준 수 | 우리 상태 | 우리 위치 | 메모 |
| :--- | :--- | :---: | :--- | :--- | :--- |
| 파일 | 열기·저장·다른 이름으로 저장 | 4 | 있음 | ui/file-open.js | |
| 파일 | 내보내기(PDF·ODT·RTF·TXT·MD) | 4 | 있음 | ui/export-dialog.js | |
| 파일 | 인쇄·인쇄 미리 보기 | 4 | 있음 | ui/doc-page-print.js | |
| 파일 | 문서 속성(제목·작성자·키워드) | 4 | 있음 | ui/doc-word-layout.js | 제목·주제·작성자·키워드·설명. DOCX `docProps/core.xml` 왕복(PD-395) |
| 파일 | 서식 파일(템플릿)로 새로 만들기 | 3 | 없음 | — | 새 문서는 빈 문서만 |
| 파일 | 열기 암호 | 3 | 없음 | — | 한글 문서만 열기 암호(PD-100) |
| 파일 | 버전 기록 | 3 | 없음 | — | |
| 파일 | 문서 비교 | 3 | 일부 | crates/easyoffice-app/src/smart_tools.rs | 엔진 LCS 비교만, 화면 없음 |
| 편집 | 실행 취소·다시 실행 | 4 | 있음 | ui/command-history.js | |
| 편집 | 잘라내기·복사·붙여넣기 | 4 | 있음 | ui/doc-paste.js | |
| 편집 | 서식 없이 붙여넣기 | 3 | 있음 | ui/doc-paste.js | 편집 > 붙여넣기 옵션 > 텍스트만 유지(PD-395) |
| 편집 | 찾기·바꾸기 | 4 | 있음 | ui/find-doc.js | |
| 편집 | 서식 복사 | 4 | 있음 | ui/doc-format-tools.js | 서식 메뉴. 한 번 복사해 다음 선택에 적용(PD-395) |
| 편집 | 서식 지우기 | 4 | 있음 | ui/doc-format-tools.js | 서식 > 모든 서식 지우기(PD-395) |
| 글자 | 글꼴·크기·굵게·기울임·밑줄 | 4 | 있음 | ui/doc-font-dialog.js | 글꼴 대화상자는 공식 표기 |
| 글자 | 글자 색·형광펜 | 4 | 있음 | ui/doc-runs-ui.js | |
| 글자 | 취소선·위 첨자·아래 첨자 | 4 | 있음 | ui/doc-runs-ui.js | |
| 글자 | 문자 간격·위치·합자 | 3 | 있음 | ui/doc-font-dialog.js | |
| 글자 | 대/소문자 바꾸기 | 3 | 있음 | ui/doc-text-case.js | 서식 > 대/소문자 바꾸기 5종과 Shift+F3(PD-400) |
| 글자 | 스타일 적용·새 스타일 만들기·수정 | 4 | 있음 | ui/doc-styles.js | 서식 > 스타일…: 새 스타일 만들기·수정·삭제·적용. DOCX `w:style`(`w:customStyle`) 왕복(PD-401) |
| 문단 | 정렬(왼쪽·가운데·오른쪽·양쪽) | 4 | 있음 | ui/doc-runs-ui.js | |
| 문단 | 줄 간격·문단 앞뒤 간격 | 4 | 있음 | ui/doc-line-spacing.js | |
| 문단 | 들여쓰기·첫 줄 들여쓰기 | 4 | 있음 | ui/doc-runs-ui.js | |
| 문단 | 탭 정지 | 3 | 있음 | ui/doc-word-tabs.js | 서식 > 탭…: 위치·맞춤(왼쪽·가운데·오른쪽·소수점)·채움 문자. DOCX `w:tabs` 왕복, 화면은 탭 문자마다 너비를 정함(PD-401) |
| 문단 | 글머리 기호·번호 매기기·다단계 목록 | 4 | 있음 | ui/doc-runs.js | |
| 문단 | 문단 테두리·음영 | 3 | 있음 | ui/doc-para-border.js | 서식 > 테두리 및 음영…(테두리·음영 탭). DOCX `w:pBdr`·`w:shd` 왕복(PD-401) |
| 문단 | 첫 글자 장식(드롭 캡) | 3 | 있음 | ui/doc-drop-cap.js | 삽입 > 드롭 캡(없음·텍스트에 넣기·여백에 넣기·옵션…). DOCX `w:framePr` `dropCap` 왕복(PD-401) |
| 쪽 | 용지 크기·방향·여백 | 4 | 있음 | ui/doc-page-setup.js | |
| 쪽 | 머리글·바닥글 | 4 | 있음 | ui/doc-page-frame.js | |
| 쪽 | 쪽 번호·총 쪽수 | 4 | 있음 | ui/doc-page-frame.js | |
| 쪽 | 첫 쪽·홀짝 쪽 다르게 | 3 | 있음 | ui/doc-page-frame.js | |
| 쪽 | 쪽 나누기 | 4 | 있음 | ui/doc-pages.js | |
| 쪽 | 구역 나누기 | 3 | 있음 | ui/doc-pages.js | 다음 페이지·연속·짝수·홀수. 구역별 쪽 설정은 문서 하나 값(PD-395) |
| 쪽 | 다단 | 3 | 있음 | ui/doc-word-columns.js | 레이아웃 > 단(하나·둘·셋·왼쪽·오른쪽·기타 단…)과 단 나누기. 구분선·간격·왼쪽/오른쪽 좁은 단은 DOCX `w:cols`로 왕복하고 화면은 같은 너비 단으로 그림(PD-401) |
| 쪽 | 줄 번호 | 3 | 일부 | ui/doc-word-layout.js | 시작 번호·간격·텍스트와의 간격·다시 시작을 DOCX `w:lnNumType`으로 왕복한다. 화면 가장자리에 번호는 그리지 않음(PD-395) |
| 쪽 | 워터마크 | 3 | 일부 | ui/doc-word-layout.js | 텍스트 워터마크를 DOCX 머리글 도형으로 왕복한다. 화면에는 첫 쪽에 한 번만 보이고 글꼴·색·크기 선택은 없음(PD-395) |
| 쪽 | 하이픈 넣기 | 3 | 없음 | — | |
| 삽입 | 표 삽입 | 4 | 있음 | ui/doc-table-grid.js | |
| 삽입 | 표 행·열 삽입·삭제, 셀 병합·분할 | 4 | 있음 | ui/doc-table.js | |
| 삽입 | 표 속성·테두리 및 음영·열 너비 같게 | 4 | 있음 | ui/doc-table-style.js | |
| 삽입 | 표 머리글 행 반복 | 3 | 있음 | ui/doc-word-layout.js | 위쪽 행부터 캐럿 행까지. DOCX `w:tblHeader` 왕복(PD-395) |
| 삽입 | 표 정렬(데이터 정렬)·셀 수식 | 3 | 있음 | ui/doc-table-tab.js | |
| 삽입 | 텍스트와 표 서로 변환 | 3 | 있음 | ui/doc-word-layout.js | 구분 기호: 단락·쉼표·탭·기타(PD-395) |
| 삽입 | 그림 삽입·크기 | 4 | 있음 | ui/doc-image.js | |
| 삽입 | 그림 자르기·회전 | 3 | 있음 | ui/doc-word-image.js | 서식 메뉴 그림 서식. 회전·자르기 핸들, DOCX `a:srcRect`·`a:xfrm rot`(PD-395) |
| 삽입 | 그림 텍스트 배치(줄 바꿈 방식) | 3 | 있음 | ui/doc-word-image.js | 줄 안·정사각형·위/아래·텍스트 뒤·앞. DOCX `wp:anchor`(PD-395) |
| 삽입 | 도형 | 3 | 일부 | ui/doc-dialogs-ext.js | 삽입만, 도형 서식·글 넣기 없음 |
| 삽입 | 차트 | 3 | 일부 | ui/doc-dialogs-ext.js | 삽입만, 삽입 뒤 편집 없음 |
| 삽입 | 텍스트 상자 | 3 | 일부 | ui/doc-text-box.js | 삽입 > 텍스트 상자, 안에 글 쓰기, 줄 바꿈 4종. DOCX `w:pict`·`v:textbox` 쓰기와 `wps:txbx` 읽기. 크기 끌기·채우기·윤곽선 서식은 없음(PD-401) |
| 삽입 | 수식 | 3 | 일부 | ui/doc-word-math.js | DOCX `m:oMath` 읽기·MathML 표시·그대로 저장. 수식 편집·새로 넣기는 없음(PD-401) |
| 삽입 | 기호 | 3 | 있음 | ui/doc-symbol-dialog.js | |
| 삽입 | 날짜·시간 | 3 | 있음 | ui/doc-word-fields.js | 형식 9종, 자동으로 업데이트는 DOCX `DATE`·`TIME` 필드 왕복(PD-395) |
| 삽입 | 하이퍼링크 | 4 | 있음 | ui/doc-link.js | |
| 삽입 | 책갈피 | 3 | 있음 | ui/doc-word-fields.js | 선택한 글에 이름. DOCX `w:bookmarkStart`·`w:bookmarkEnd` 왕복(PD-395) |
| 삽입 | 상호 참조 | 3 | 일부 | ui/doc-word-fields.js | 책갈피·제목·그림·표. 쪽 번호·위/아래 참조 대상 없음, DOCX `REF` 필드 왕복(PD-395) |
| 삽입 | 캡션 | 3 | 있음 | ui/doc-word-fields.js | 그림·표·수식 레이블, 위·아래. DOCX 캡션 스타일과 `SEQ` 필드 왕복(PD-395) |
| 삽입 | 각주·미주 | 3 | 있음 | ui/doc-notes-ui.js | |
| 삽입 | 목차(자동 목차·업데이트) | 4 | 있음 | ui/doc-toc.js | |
| 삽입 | 양식 컨트롤(확인란·목록 상자) | 3 | 일부 | ui/doc-dialogs-ext.js | 체크리스트만 |
| 검토 | 변경 내용 추적·수락·거부 | 3 | 있음 | ui/doc-notes-ui.js | |
| 검토 | 메모 | 4 | 있음 | ui/doc-notes.js | |
| 검토 | 단어 수 | 4 | 있음 | ui/doc-word-count.js | |
| 검토 | 맞춤법 검사 | 3 | 제외(QA 결정, PD-396) | — | 허용 라이선스 한국어 사전이 없음 |
| 검토 | 편지 병합 | 3 | 일부 | ui/doc-dialogs-ext.js | 필드 치환만, 데이터 원본 연결·미리 보기 없음 |
| 보기 | 확대·축소·한 페이지·페이지 너비 | 4 | 있음 | ui/zoom-range.js | |
| 보기 | 눈금자 | 3 | 있음 | ui/index.html | |
| 보기 | 서식 기호(단락 표시) | 3 | 있음 | ui/index.html | |
| 보기 | 탐색 창(문서 개요) | 3 | 있음 | ui/index.html | |
| 보기 | 여러 쪽 보기·전체 화면 | 3 | 일부 | ui/doc-view-pages.js | 보기 > 여러 페이지: 인쇄 미리 보기 쪽 배치를 두 쪽 배열로 연다. 전체 화면은 없음(PD-401) |
| 제외 | 매크로 실행·기록 | 4 | 제외(매크로·VBA 미지원) | — | PD-393 |
| 제외 | 실시간 공동 편집·채팅·멘션·메모 답글 | 4 | 제외(클라우드 공동 편집·계정) | — | PD-393 |

## 시트

| 영역 | 기능 | 기준 수 | 우리 상태 | 우리 위치 | 메모 |
| :--- | :--- | :---: | :--- | :--- | :--- |
| 파일 | 열기·저장·다른 이름으로 저장 | 4 | 있음 | ui/file-open.js | |
| 파일 | 내보내기(XLSX·ODS·CSV·TSV·PDF) | 4 | 있음 | ui/export-dialog.js | |
| 파일 | 인쇄 영역·페이지 설정·인쇄 미리 보기 | 4 | 있음 | ui/sheet-page.js | |
| 파일 | 인쇄 제목(반복 행·열)·머리글·바닥글 | 3 | 있음 | ui/sheet-page.js | |
| 파일 | 쪽 나누기 삽입·제거·원래대로 | 3 | 있음 | ui/sheet-page.js | |
| 파일 | 문서 속성 | 4 | 없음 | — | |
| 파일 | 열기 암호 | 3 | 없음 | — | |
| 파일 | 버전 기록 | 3 | 없음 | — | |
| 편집 | 실행 취소·다시 실행 | 4 | 있음 | ui/sheet-history.js | |
| 편집 | 선택하여 붙여넣기(값·서식·행/열 바꿈·연산) | 4 | 있음 | ui/sheet-paste-ui.js | |
| 편집 | 채우기(자동 채우기·시리즈) | 4 | 있음 | ui/sheet-fill-ui.js | |
| 편집 | 찾기·바꾸기 | 4 | 있음 | ui/find-sheet.js | |
| 편집 | 이동·특수 셀 선택 | 3 | 있음 | ui/sheet-goto.js | 우리 표기: 이동 옵션 |
| 편집 | 서식 복사 | 4 | 있음 | ui/sheet-format-painter.js | 서식 > 서식 복사·붙여넣기, 붓 단추, Ctrl+Shift+C/V. 서식 선택하여 붙여넣기 경로를 재사용(PD-400) |
| 편집 | 지우기(내용·서식·모두) | 4 | 있음 | ui/sheet-clear-ui.js | 편집 > 지우기: 모두·서식·내용·메모·하이퍼링크(PD-400) |
| 편집 | 입력 자동 완성 | 3 | 있음 | ui/sheet-cell-edit.js | |
| 편집 | 셀 메모 | 4 | 있음 | ui/sheet-note-ui.js | |
| 편집 | 하이퍼링크 | 4 | 있음 | ui/sheet-link-ui.js | |
| 편집 | 행·열 삽입·삭제·숨기기·크기·자동 맞춤 | 4 | 있음 | ui/sheet-structure.js | |
| 편집 | 시트 추가·이름·이동·복사·숨기기·탭 색 | 4 | 있음 | ui/sheet-tab-menu.js | |
| 편집 | 셀 병합·병합 해제 | 4 | 있음 | ui/sheet-look-ui.js | |
| 서식 | 글꼴·크기·굵게·기울임·밑줄·취소선·글자 색 | 4 | 있음 | ui/sheet-look-ui.js | |
| 서식 | 채우기 색 | 4 | 있음 | ui/sheet-color-ui.js | |
| 서식 | 테두리 | 4 | 있음 | ui/sheet-look-ui.js | |
| 서식 | 맞춤(가로·세로·줄 바꿈·들여쓰기·텍스트 회전) | 4 | 있음 | ui/sheet-look-ui.js | |
| 서식 | 표시 형식(숫자·통화·회계·날짜·시간·백분율·분수·사용자 지정) | 4 | 있음 | ui/sheet-numfmt-ui.js | |
| 서식 | 셀 서식 대화상자 | 4 | 있음 | ui/sheet-look-ui.js | |
| 서식 | 셀 스타일 | 3 | 일부 | ui/sheet-cell-styles.js | 공식 기본 47개 목록을 서식으로 적용. 새 셀 스타일과 XLSX `cellStyles` 왕복은 없음(PD-400) |
| 서식 | 조건부 서식(강조 규칙·데이터 막대·색조·아이콘 집합) | 4 | 있음 | ui/sheet-rules-ui.js | |
| 서식 | 표로 서식 지정(표 스타일·요약 행) | 3 | 있음 | ui/sheet-table-ui.js | |
| 데이터 | 정렬(오름·내림·사용자 지정) | 4 | 있음 | ui/sheet-sort-ui.js | |
| 데이터 | 자동 필터 | 4 | 있음 | ui/sheet-filter-ui.js | |
| 데이터 | 고급 필터(조건 범위) | 3 | 있음 | ui/sheet-adv-filter.js | 현재 위치·다른 장소에 복사·고유 레코드만. 한 열에 조건 둘까지, 계산 조건 없음(PD-400) |
| 데이터 | 데이터 유효성 검사 | 4 | 있음 | ui/sheet-dv-widgets.js | |
| 데이터 | 중복된 항목 제거 | 3 | 있음 | ui/sheet-table-dedupe.js | |
| 데이터 | 텍스트 나누기 | 3 | 있음 | ui/sheet-text-columns.js | |
| 데이터 | 부분합 | 3 | 있음 | ui/sheet-subtotal-ui.js | |
| 데이터 | 그룹·윤곽 | 3 | 있음 | ui/sheet-outline-ui.js | |
| 데이터 | 데이터 통합 | 3 | 없음 | — | 이식할 허용 라이선스 구현을 찾지 못해 멈춤(PD-400) |
| 데이터 | 피벗 테이블 | 3 | 있음 | ui/sheet-pivot-ui.js | |
| 데이터 | 목표값 찾기 | 3 | 있음 | ui/sheet-goal-seek.js | 데이터 > 가상 분석. 할선법, 반복 100·허용 오차 0.001(PD-400) |
| 데이터 | 외부 데이터 연결·웹 쿼리 | 3 | 제외(외부 데이터 연결) | — | PD-393 |
| 수식 | 내장 함수(수학·통계·논리·찾기·텍스트·날짜·재무·공학·정보·데이터베이스) | 4 | 있음 | crates/easyoffice-sheet/src/function_coverage_tests.rs | 시험이 함수 목록을 대조 |
| 수식 | 네트워크 함수(WEBSERVICE 등) | 3 | 제외(네트워크 함수, PD-318) | — | PD-393 |
| 수식 | 동적 배열·스필 | 3 | 있음 | ui/sheet-spill.js | |
| 수식 | 함수 삽입·자동 합계 | 4 | 있음 | ui/sheet-formula-bar.js | |
| 수식 | 이름 정의·이름 관리자 | 4 | 있음 | ui/sheet-names-ui.js | |
| 수식 | 계산 옵션(자동·수동·지금 계산) | 3 | 있음 | ui/sheet-calc-options.js | |
| 수식 | 수식 표시 | 3 | 있음 | ui/sheet-view-options.js | 수식 > 수식 표시, Ctrl+`. 시트별 설정(PD-400) |
| 개체 | 그림 삽입 | 4 | 있음 | ui/sheet-media-ui.js | |
| 개체 | 차트 삽입(막대·꺾은선·원·분산·영역) | 3 | 있음 | ui/sheet-chart-ui.js | |
| 개체 | 차트 서식(제목·범례·축·데이터 레이블) | 3 | 일부 | ui/sheet-chart-ui.js | 축·데이터 레이블 서식을 확인하지 못함 |
| 개체 | 차트 종류 확장(도넛·방사형·거품·주식·콤보) | 3 | 있음 | ui/chart-kind-doughnut.js | 도넛형·방사형·거품형·주식형·콤보 5종을 슬라이드와 함께 넣음. XLSX `c:doughnutChart`·`c:radarChart`·`c:bubbleChart`·`c:stockChart` 왕복, 콤보는 첫 계열만 막대(PD-402) |
| 개체 | 도형·텍스트 상자 | 3 | 없음 | — | |
| 개체 | 스파크라인 | 3 | 없음 | — | XLSX `x14:sparklineGroups` 왕복과 셀 안 미니 차트 렌더를 새로 짜야 해 이번엔 하지 않음(PD-400) |
| 검토 | 시트 보호·통합 문서 보호·범위 편집 허용 | 4 | 있음 | ui/sheet-protect-ui.js | |
| 검토 | 변경 내용 추적 | 3 | 있음 | ui/index.html | |
| 검토 | 맞춤법 검사 | 3 | 제외(QA 결정, PD-396) | — | 허용 라이선스 한국어 사전이 없음 |
| 보기 | 틀 고정·창 나누기 | 4 | 있음 | ui/sheet-freeze.js | |
| 보기 | 눈금선·머리글·수식 입력줄 표시 | 4 | 있음 | ui/sheet-view-options.js | |
| 보기 | 확대·축소 | 4 | 있음 | ui/zoom-range.js | |
| 보기 | 이름 상자 | 4 | 있음 | ui/index.html | |
| 제외 | 매크로 실행·기록 | 4 | 제외(매크로·VBA 미지원) | — | PD-393 |
| 제외 | 실시간 공동 편집·스레드 댓글 | 4 | 제외(클라우드 공동 편집·계정) | — | PD-393 |

## 슬라이드

| 영역 | 기능 | 기준 수 | 우리 상태 | 우리 위치 | 메모 |
| :--- | :--- | :---: | :--- | :--- | :--- |
| 파일 | 열기·저장·다른 이름으로 저장 | 3 | 있음 | ui/file-open.js | |
| 파일 | 내보내기(PPTX·ODP·PDF) | 3 | 있음 | ui/export-dialog.js | |
| 파일 | 인쇄·인쇄 미리 보기 | 3 | 일부 | ui/office-print.js | 슬라이드 인쇄만, 유인물·메모 인쇄 없음 |
| 파일 | 문서 속성 | 3 | 없음 | — | |
| 파일 | 열기 암호 | 3 | 없음 | — | |
| 편집 | 실행 취소·다시 실행 | 3 | 있음 | ui/command-history.js | |
| 편집 | 잘라내기·복사·붙여넣기 | 3 | 있음 | ui/slide-clipboard.js | |
| 편집 | 찾기·바꾸기 | 3 | 있음 | ui/find-core.js | 글상자·도형·표 칸·그림 대체 텍스트·제목·본문·노트를 바꾼다. 차트와 삽입한 개체의 글자는 찾기만(PD-398) |
| 편집 | 서식 복사 | 3 | 있음 | ui/slide-painter.js | 서식 ▸ 서식 복사·붙여넣기, 도구 줄 붓 단추, Ctrl+Shift+C·V. 개체 단위(글자 서식·채우기·선·그림자)(PD-398) |
| 슬라이드 | 새 슬라이드·복제·삭제 | 3 | 있음 | ui/slide-studio.js | |
| 슬라이드 | 슬라이드 순서 이동 | 3 | 있음 | ui/slide-studio.js | |
| 슬라이드 | 슬라이드 레이아웃 바꾸기 | 3 | 일부 | ui/slide-dialogs.js | 마스터 레이아웃 지정만, 슬라이드별 바꾸기 메뉴 없음 |
| 슬라이드 | 슬라이드 마스터 | 3 | 일부 | ui/slide-dialogs.js | 레이아웃 선택만, 마스터 개체 편집 없음 |
| 슬라이드 | 슬라이드 크기(화면 비율) | 3 | 있음 | ui/slide-size.js | 슬라이드 ▸ 슬라이드 크기(표준 4:3·와이드스크린 16:9·사용자 지정), 너비·높이(cm)·방향. PPTX `p:sldSz`·ODP 쪽 크기 왕복, 1~56인치(PD-402) |
| 슬라이드 | 배경 서식 | 3 | 있음 | ui/slide-page.js | 단색·그라데이션(2색 선형)·그림, 모두 적용(PD-394) |
| 슬라이드 | 테마·색 구성표 | 3 | 일부 | ui/slide-colors.js | 색 구성표 7종 선택과 테마 `a:clrScheme` 왕복(PPTX). 테마 글꼴·효과 선택은 없음(PD-398) |
| 슬라이드 | 슬라이드 숨기기 | 3 | 있음 | ui/slide-page.js | 발표·인쇄·PDF 내보내기에서 건너뜀(PD-394) |
| 슬라이드 | 슬라이드 번호·날짜·바닥글 | 3 | 있음 | ui/slide-page.js | 머리글 및 바닥글 대화상자, 마스터에서 위치를 바꾸는 기능은 없음(PD-394) |
| 삽입 | 텍스트 상자 | 3 | 있음 | ui/slide-dialogs.js | |
| 삽입 | 도형 | 3 | 있음 | ui/slide-dialogs.js | |
| 삽입 | 그림 | 3 | 있음 | ui/slide-dialogs.js | |
| 삽입 | 표 | 3 | 있음 | ui/slide-table.js | 삽입, 행·열 삽입과 삭제(PD-398), 셀 병합·분할(`gridSpan`·`rowSpan`·`hMerge`·`vMerge` 왕복, PD-402). 병합 없는 칸 나누기는 없음 |
| 삽입 | 차트 | 3 | 일부 | ui/slide-dialogs.js | 삽입만, 삽입 뒤 편집 없음 |
| 삽입 | 하이퍼링크 | 3 | 있음 | ui/slide-studio.js | |
| 삽입 | 기호 | 3 | 있음 | ui/doc-symbol-dialog.js | |
| 삽입 | 오디오·비디오 | 3 | 없음 | — | |
| 삽입 | 글꼴 효과 글자(워드아트) | 3 | 없음 | — | |
| 삽입 | 수식 | 3 | 없음 | — | |
| 개체 | 순서(맨 앞·뒤) | 3 | 있음 | ui/slide-canvas.js | |
| 개체 | 맞춤·가로/세로 간격 같게 | 3 | 있음 | ui/slide-canvas.js | |
| 개체 | 스마트 가이드(스냅) | 3 | 있음 | ui/slide-canvas.js | |
| 개체 | 그룹·그룹 해제 | 3 | 있음 | ui/slide-group.js | 중첩 그룹, Ctrl+G·Ctrl+Shift+G(PD-394) |
| 개체 | 회전·좌우/상하 대칭 | 3 | 있음 | ui/slide-group.js | 회전 핸들(Shift 15° 단위), 90° 회전, 대칭. 표·차트는 제외(PD-394) |
| 개체 | 채우기·윤곽선·그림자 서식 | 3 | 일부 | ui/slide-format.js | 단색 채우기만(그라데이션·그림 채우기 없음), 표·차트는 제외(PD-394) |
| 개체 | 그림 자르기·효과 | 3 | 있음 | ui/slide-crop.js | 서식 ▸ 자르기(가장자리 핸들), PPTX `a:srcRect` 왕복. 그림자·테두리는 서식 대화상자(PD-394). ODP 자르기는 쓰지 않음(PD-402) |
| 개체 | 눈금자·눈금선·안내선 | 3 | 있음 | ui/slide-guides.js | 보기 ▸ 눈금자·눈금선·안내선. 안내선은 끌어 옮기고 스마트 가이드가 맞추며 PPTX `p:guideLst` 왕복(PD-402) |
| 개체 | 글자 서식(굵게·기울임·밑줄·글꼴·크기·색) | 3 | 일부 | ui/slide-font.js | 서식 ▸ 글꼴 대화상자로 글꼴(한글·영어)·스타일·크기·색·밑줄(PD-398). 개체 전체에 적용하고 글자별로는 못 나눔 |
| 개체 | 문단 서식(정렬·글머리·줄 간격) | 3 | 일부 | ui/slide-format.js | 개체 전체에 한 번에 적용하고 문단별로는 못 나눔. 들여쓰기 수준 없음(PD-394) |
| 효과 | 슬라이드 전환 | 3 | 있음 | ui/slide-show-setup.js | 9종(없음·페이드·밀기·닦아내기·분할·덮기·벗기기·확대/축소·무작위 선)과 방향, 다음 시간 후·클릭 넘김. 3차원·확장 전환은 없음(PD-398) |
| 효과 | 개체 애니메이션 | 3 | 일부 | ui/slide-anim.js | 나타내기 5종·강조 2종·끝내기 5종, 시작 3가지, 재생 시간·지연, 발표 보기 재생. 이동 경로·트리거·소리 없음(PD-394) |
| 쇼 | 처음부터·현재 슬라이드부터 | 3 | 있음 | ui/slide-studio.js | |
| 쇼 | 발표자 보기·메모 | 3 | 있음 | ui/slide-studio.js | |
| 쇼 | 쇼 설정(반복·자동 진행·타이밍 연습) | 3 | 일부 | ui/slide-show-setup.js | 반복, 수동·설정된 시간 사용, 슬라이드별 자동 넘김. 타이밍 연습 없음(PD-398) |
| 쇼 | 레이저 포인터 | 3 | 있음 | ui/app.js | |
| 검토 | 메모 | 3 | 있음 | ui/slide-studio.js | |
| 검토 | 맞춤법 검사 | 3 | 제외(QA 결정, PD-396) | — | 허용 라이선스 한국어 사전이 없음 |
| 보기 | 확대·축소·창에 맞춤 | 3 | 있음 | ui/zoom-range.js | |
| 보기 | 슬라이드 정렬 보기·개요 보기 | 3 | 일부 | ui/slide-overview.js | 보기 ▸ 슬라이드 정렬(격자 썸네일, 끌어서 순서 바꾸기)(PD-398). 개요 보기 없음 |
| 제외 | 매크로 실행·기록 | 3 | 제외(매크로·VBA 미지원) | — | PD-393 |
| 제외 | 실시간 공동 편집·메모 답글 | 3 | 제외(클라우드 공동 편집·계정) | — | PD-393 |
