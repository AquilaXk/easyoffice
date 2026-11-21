# TODO

## 지금
- [ ] 설치본 실기기 점검(아래; `v0.1.0` 태그는 발행 완료)

## v0.1.0 실기기 점검 (macOS·Windows)
- [ ] 설치·실행, 앱 정보 버전 0.1.0, 아이콘
- [ ] 창 크기·위치 복원
- [ ] 한글 창: PDF로 내보내기(글자 검색·복사), 인쇄(진행·취소), 암호 문서, 도구 상자 라벨 토글
- [ ] 업데이트 카드: 최신이면 안 뜸, 새 릴리스가 있을 때만 뜸(이전 1.x·2.x 설치본은 수동 재설치)
- [ ] 업데이트 설치: 서명된 릴리스에서 카드의 '설치하고 다시 시작'으로 설치·재시작(Windows NSIS, macOS, AppImage, deb·rpm), 저장 안 된 창이 있으면 막히고 안내
- [ ] PDF 스튜디오·변환기·슬라이드 화면, Pages 파일 표시
- [ ] macOS: 네이티브 메뉴, 한글 편집기 안 Cmd+Z/X/C/V/A, Cmd+F, Dock 최근 항목
- [ ] Windows·Linux: 모든 창에 페이지 메뉴 줄(한글 창은 편집기 메뉴 줄), Ctrl+Q(한글 창 포함)의 순차 종료
- [ ] Windows 로그오프·종료에서 저장 안 된 창이 있으면 종료 화면에 "저장하지 않은 문서"가 뜨고 저장 확인이 이어지는지, 취소하면 앱이 남는지(PD-191). Linux `kill -TERM`·로그아웃에서 같은 확인(PD-191)

## 배포
- [ ] 릴리스 발행 잡의 쓰기 403 원인 확정: 다음 릴리스 실행에서 "Record the token's release access…" 경고와 "Create or update the GitHub Release" 로그의 `x-accepted-github-permissions`·`x-github-request-id`를 보고 원인을 확정함. 워크플로 파일 차이 가설은 태그 push 실행에서 맞지 않았고 403은 수동 실행에서만 났다(PD-283 6항 (d))
- [ ] v0.2.1에 조건부 서식 메뉴(`2c5b3816`) 포함
- [ ] 정식 릴리스 전: macOS Developer ID 서명·공증(QA 보류, PD-143)

## PDF·미리보기
- [ ] PDF 스튜디오 저장: Square·Circle·Line·Polygon·FileAttachment 주석, 태그 구조 트리, 전자서명은 저장하지 못하고 경고만 한다(PD-266, 증분 저장 또는 종류별 복사)
- [ ] 검색 가능한 PDF: Linux·Windows 설치본에서 번들 글꼴 확인(PD-131)
- [ ] 엔진 PDF 읽기를 크레이트로 바꿀지 QA 결정: 그대로 두기, 워커 프로세스로 옮기기, 읽기 크레이트를 포크해 해제 상한 넣기(PD-269 12항)
- [ ] 엔진 HWP·HWPX 읽기를 rhwp 파서로 바꿀지: 파서를 워커 프로세스로 격리하고 편집기 모델 ↔ `RichDocument` 변환을 새로 짜야 한다(PD-269 13항)
- [ ] CI 세 운영체제에서 `ole2_writer_boundary_pd269.rs`의 고정 해시가 같은지 확인(PD-269 13항)
- [ ] 엔진·셸 PDF 상한 표 통합 여부(객체 65,536 대 1,000,000, 쪽 트리 노드 4,096 대 100,000, PD-269 11항)
- [ ] CI 세 운영체제에서 `the_bytes_are_the_same_on_every_operating_system`의 고정 해시가 같은지 확인(PD-269 11항)

## Pages 읽기
- [ ] 실제 Pages 15.x로 각주·댓글·하이퍼링크·병합·쪽 배치 문서를 만들어 독립 구현 기준 값을 대조(PD-157, QA 필요)
- [ ] Pages 구역마다 다른 머리글·바닥글: 부록에 붙는 다른 구역의 머리글 복사본이 쪽 틀(첫 구역) 글과 달라 본문으로 남아, 열기→저장을 되풀이하면 누적됨. 리더가 읽은 복사본 원문을 문단에 실어 `body_blocks`가 그 글과 비교하도록(`rich_document.rs`, `rich_read_pages/notes.rs`, `frameCopy` JSON 필드)

## 코파일럿
- [ ] 코파일럿 포인터·타이핑 연출, 삭제·삽입 미리보기, 수락·거절 화면 QA 승인(캡처, PD-268)
- [ ] 실제 제공자(OpenAI 호환·Anthropic·Gemini) 스트림 호출과 설치본의 채널 전달 확인(PD-268)
- [ ] 슬라이드 추가·개체·서식·차트와 문서 범위 교체에도 포인터 연출(PD-268)
- [ ] 선택 영역 명령 이름(더 짧게·더 길게·맞춤법 수정·번역)을 상용 오피스 코파일럿 한국어 문구와 대조(QA 필요, PD-267)

## BASE

## 시트 함수 (PD-270·PD-271·PD-273, 비교표 `docs/sheet-function-parity.md`)
- [ ] 기준 앱 실측 대조 필요: `FORECAST.ETS` 계열의 비공개 평활 계수 선택 목적함수(0.001 격자), 계절 경로의 순환식·끝점 고정 처리, `STAT`의 MAE·RMSE·MASE 오차 열 정의, 같은 날짜가 있을 때 `aggregation` 생략·`0` 동작, 범위 밖 인수와 시계열 안 대상 날짜의 오류 값(`docs/forecast-ets-cleanroom.md` 9장)
- [ ] `GETPIVOTDATA`를 앱에 연결: 평가 요청에 시트별 `pivots`를 싣고(`sheet_pivot_json::parse_pivots`, 표 처리 옆 `lib.rs`) `WorkbookEngine::set_pivot_tables`에 넣는다. 페이지 JS가 요청에 `pivots`를 더해야 한다. 지금은 등록이 없어 `#REF!`
- [ ] `GROUPBY`·`PIVOTBY`: 함수 목록(여러 열 집계), 만들어 붙이는 머리글·합계 글 문구(`Total`, `<항목> Total`, `Group`, `Value`)를 기준 앱 출력과 대조
- [ ] `#BLOCKED!`·`#CONNECT!` 오류 값: `IMAGE`(문서에 명시)·`CALL`·`REGISTER.ID`(XLM 매크로 차단)·`STOCKHISTORY`에 필요하다. 지금은 `#NAME?`. 오류 값 종류를 더하려면 포맷·앱·페이지의 오류 목록을 함께 고친다
- [ ] `PHONETIC`: XLSX `rPh` 후리가나를 읽어 셀에 보관하고 저장에도 쓴다(`xlsx_sheet.rs`, 포맷 크레이트 개편 병합 뒤). 지금은 셀 글자를 돌려준다
- [ ] `FILTERXML`: 유지되는 XPath 크레이트가 없다. 클린룸 최소 평가기(상한 포함)나 대체 크레이트를 검토한다(PD-271). 지금은 이름만 알고 `#VALUE!`
- [ ] 재무: `ACCRINT` 기준 1~3과 `ODDFPRICE`·`ODDFYIELD`·`ODDLPRICE` 일부 값이 참조 A 시험(기준 앱 산출값 대조)과 다르다. `base/src/functions/financial/accrint.rs`·`calendar.rs`의 일수 계산을 이식해 대조
- [ ] 큐브 7개(`CUBE*`), `CALL`·`REGISTER.ID`, `STOCKHISTORY`, `DETECTLANGUAGE`·`TRANSLATE`, `IMAGE`는 외부 연결이나 신뢰할 수 없는 문서의 네트워크 접근이라 대상 아님(PD-271). `WEBSERVICE`·`RTD`는 이름만 알고 문서의 오류 값을 돌려준다
- [ ] `A1:OFFSET(...)`처럼 함수 호출을 끝으로 하는 범위 연산자, `@` 암묵적 교차 연산자 일부(`COLUMNS(@A1:C1)`, `OFFSET(@B1:B4,...)`)
- [ ] `TRIMRANGE`에 전체 열·행 참조(`A:A`)를 주면 배열 예산을 넘어 `#SPILL!`; 사용 범위로 먼저 줄여야 한다
- [ ] `CELL`의 `format`·`width`·`color`·`parentheses`·`prefix`·`protect`·`filename`, `INFO`의 `release`·`osversion`·`directory`·`origin`(파일 이름·서식 정보를 평가 맥락에 실어야 함)
- [ ] 의존 라이선스 자동 검사(MPL은 수정 없는 링크일 때 고지 확인, GPL 계열 거부)
- [ ] 날짜 글 해석: `DATEVALUE("2026 - 01 - 01")`(구분자 둘레 공백), `"... 24:00:00"` 올림, `DATEDIF`의 지역 날짜 글, 순환 참조 오류 값(`#NUM!` 대신 `#CIRC!`인지 기준 앱 대조)


## 시트 UI 이식 1차 이후 (PD-275)
- [ ] 포인트 모드가 열 머리글 끌기로 여러 열·행을 잡지 못하고(클릭·Shift+클릭만), 병합 칸은 왼쪽 위 칸 하나만 넣고, 표 열 이름(구조적 참조)·정의된 이름은 넣지 않는다(PD-279)
- [ ] 다른 시트로 넘어간 수식 입력에서 수식이 완성형(`)` 뒤)이 되면 셀 클릭이 무시되고 Enter·Esc만 듣는다(표 계산 프로그램은 확정하고 이동). 다른 통합 문서 참조(`[책]시트!A1`)와 3차원 참조는 없다(PD-279)
- [ ] 이미지·차트 위치의 픽셀 좌표(`left_px`·`top_px`)와 두 칸 앵커 변환(`sheet_chart.rs` `row_top_px`, `xlsx_drawings.rs`, 화면 `sheet-layout.js`·`sheet-media-ui.js`)이 24px 행으로 계산한다. 행 높이·기본 행 높이를 정한 시트에서는 칸 앵커가 없는 그림이 어긋난다(PD-287)
- [ ] Ctrl+PageUp·PageDown으로 이전·다음 시트 이동(지금은 칸을 옮기지 않고 무시). 틀 고정 시트에서 Ctrl+Home이 고정되지 않은 첫 칸으로 가는지 실제 프로그램으로 확인(PD-280)
- [ ] 서식 대화상자 기타 목록의 전화 번호(국내) 형식, 날짜·시간·분수 목록 항목을 실제 한국어 프로그램의 목록과 대조(PD-280)
- [ ] 조건부 서식 테두리 선 목록(가는 실선·파선·점선)과 `빨강 테두리`(가는 실선 `ff0000` 네 변)를 실제 한국어 프로그램의 조건부 서식 대화상자와 대조(PD-284)
- [ ] XLSX 규칙 서식(`dxf`)의 채우기·글자색은 `rgb`만 읽고 테마·`indexed` 색은 풀지 않는다(테두리 색만 푼다, PD-284)
- [ ] 시트 인쇄(`office-print.js`)는 셀 자신의 서식만 그려서 조건부 서식의 채우기·글자색·글꼴·테두리가 인쇄되지 않는다(PD-284)
- [ ] 조건부 서식 메뉴 항목 이름·색·순서를 실제 한국어 프로그램 문서와 대조한다. 갤러리 항목 위에 포인터를 올리면 시트에 미리 보이는 동작은 없다(클릭해야 적용, PD-282)
- [ ] 실제 표 계산 프로그램이 복사한 HTML 표본으로 `sheet-clip-html.js` 대조(현재 시험 자료는 알려진 출력 모양을 따라 쓴 것)
