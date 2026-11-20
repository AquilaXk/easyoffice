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
- [ ] 정식 릴리스 전: macOS Developer ID 서명·공증(QA 보류, PD-143)

## PDF·미리보기
- [ ] PDF 스튜디오 저장: Square·Circle·Line·Polygon·FileAttachment 주석, 태그 구조 트리, 전자서명은 저장하지 못하고 경고만 한다(PD-266, 증분 저장 또는 종류별 복사)
- [ ] 검색 가능한 PDF: Linux·Windows 설치본에서 번들 글꼴 확인(PD-131)

## Pages 읽기
- [ ] 실제 Pages 15.x로 각주·댓글·하이퍼링크·병합·쪽 배치 문서를 만들어 독립 구현 기준 값을 대조(PD-157, QA 필요)
- [ ] Pages 구역마다 다른 머리글·바닥글: 부록에 붙는 다른 구역의 머리글 복사본이 쪽 틀(첫 구역) 글과 달라 본문으로 남아, 열기→저장을 되풀이하면 누적됨. 리더가 읽은 복사본 원문을 문단에 실어 `body_blocks`가 그 글과 비교하도록(`rich_document.rs`, `rich_read_pages/notes.rs`, `frameCopy` JSON 필드)

## 코파일럿
- [ ] 코파일럿 포인터·타이핑 연출, 삭제·삽입 미리보기, 수락·거절 화면 QA 승인(캡처, PD-268)
- [ ] 실제 제공자(OpenAI 호환·Anthropic·Gemini) 스트림 호출과 설치본의 채널 전달 확인(PD-268)
- [ ] 슬라이드 추가·개체·서식·차트와 문서 범위 교체에도 포인터 연출(PD-268)
- [ ] 선택 영역 명령 이름(더 짧게·더 길게·맞춤법 수정·번역)을 상용 오피스 코파일럿 한국어 문구와 대조(QA 필요, PD-267)

## BASE
- [ ] 레코드 만 건 격자: 관계 칸 옵션은 지연 생성으로 줄였으나(2,000건 0.35초) 1만 건은 다시 그리기가 3.3초라 행 가상화가 남음(`base-studio.js` `renderGridView`)

## 시트 함수 (PD-270, 비교표 `docs/sheet-function-parity.md`)
- [ ] XLSX `_xlfn.` 접두 목록에 `REGEXTEST`·`REGEXEXTRACT`·`REGEXREPLACE`·`TRIMRANGE`·`PERCENTOF`를 더한다(`easyoffice-formats/src/xlsx_formula_text.rs`와 `tests/fixtures/xlsx_future_functions.txt`, 포맷 크레이트 개편 병합 뒤)
- [ ] 텍스트 나머지: `BAHTTEXT`(참조 B), `JIS`, `PHONETIC`; 온라인 서비스 `DETECTLANGUAGE`·`TRANSLATE`는 대상 아님
- [ ] 통계: `FORECAST.ETS`·`.CONFINT`·`.SEASONALITY`·`.STAT` 4개(참조 A에 있음, 지수 평활 상태 공간 모형)
- [ ] 조회·참조: `GROUPBY`·`PIVOTBY`·`GETPIVOTDATA`(피벗 엔진과 맞춰야 함), `IMAGE`(시트 개체 모델), `RTD`는 대상 아님
- [ ] 재무: `ACCRINT` 기준 1~3과 `ODDFPRICE`·`ODDFYIELD`·`ODDLPRICE` 일부 값이 참조 A 시험(기준 앱 산출값 대조)과 다르다. `base/src/functions/financial/accrint.rs`·`calendar.rs`의 일수 계산을 이식해 대조
- [ ] 큐브 7개(`CUBE*`), 웹(`WEBSERVICE`·`FILTERXML`), 추가 기능(`CALL`·`REGISTER.ID`·`EUROCONVERT`), `STOCKHISTORY`는 외부 연결이라 대상 아님. `EUROCONVERT`만 환산표를 두면 가능(참조 B에 있음)
- [ ] `A1:OFFSET(...)`처럼 함수 호출을 끝으로 하는 범위 연산자, `@` 암묵적 교차 연산자 일부(`COLUMNS(@A1:C1)`, `OFFSET(@B1:B4,...)`)
- [ ] `TRIMRANGE`에 전체 열·행 참조(`A:A`)를 주면 배열 예산을 넘어 `#SPILL!`; 사용 범위로 먼저 줄여야 한다
- [ ] `CELL`의 `format`·`width`·`color`·`parentheses`·`prefix`·`protect`·`filename`, `INFO`의 `release`·`osversion`·`directory`·`origin`(파일 이름·서식 정보를 평가 맥락에 실어야 함)
- [ ] 날짜 글 해석: `DATEVALUE("2026 - 01 - 01")`(구분자 둘레 공백), `"... 24:00:00"` 올림, `DATEDIF`의 지역 날짜 글, 순환 참조 오류 값(`#NUM!` 대신 `#CIRC!`인지 기준 앱 대조)
- [ ] `ROUNDDOWN((0.1+0.7)*10,0)`처럼 이진 오차를 15자리로 보정한 뒤 자르는 반올림 계열(참조 A는 8, 우리는 7): 기준 앱 값 확인 필요(QA)

