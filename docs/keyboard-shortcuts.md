# 키보드 단축키

EasyOffice의 단축키 목록이다. macOS는 ⌘(Command), Windows와 Linux는 Ctrl을 쓴다. 표의 "Windows / Linux" 열은 같은 동작의 Ctrl 표기다.

출처는 소스 한 곳씩이다. 소스가 바뀌면 이 문서도 함께 고친다.

- 셸 공통 키: `crates/easyoffice-app/ui/ribbon-catalog.js`의 `WORKSPACE_SHORTCUT_BINDINGS`
- macOS 메뉴 막대 가속키: `crates/easyoffice-desktop/src/native_menu.rs`
- 한글 편집기에서 셸로 넘기는 키: `crates/easyoffice-app/ui/workspace-session.js`(`forwardFromEditor` 표시가 있는 항목)
- 한글 편집기 자체 키: `third_party/rhwp/rhwp-studio/src/command/shortcut-map.ts`

## 셸 공통

모든 모듈(문서·한글·시트·슬라이드·PDF)에서 쓴다. "데스크톱 앱만"은 설치형 앱 안에서만 묶이는 키다.

| 동작 | macOS | Windows / Linux | 비고 |
| --- | --- | --- | --- |
| 열기 | ⌘O | Ctrl+O | |
| 새로 만들기 | ⌘N | Ctrl+N | 데스크톱 앱만 |
| 창 닫기 | ⌘W | Ctrl+W | 데스크톱 앱만 |
| 끝내기 | ⌘Q | Ctrl+Q | 데스크톱 앱만 |
| 저장 | ⌘S | Ctrl+S | |
| 다른 이름으로 저장 | ⇧⌘S | Ctrl+Shift+S | |
| 최근 문서 | ⌥⌘O | Ctrl+Alt+O | |
| 되돌리기 | ⌘Z | Ctrl+Z | |
| 다시 실행 | ⇧⌘Z | Ctrl+Y, Ctrl+Shift+Z | macOS 메뉴는 ⇧⌘Z |
| 오려 두기 / 복사 / 붙이기 | ⌘X / ⌘C / ⌘V | Ctrl+X / Ctrl+C / Ctrl+V | 브라우저 기본 동작 |
| 명령 팔레트 | ⌘K | Ctrl+K | `/`도 같은 동작(입력란 밖) |
| 찾기(명령 팔레트) | ⌘F | Ctrl+F | |
| 굵게 | ⌘B | Ctrl+B | |
| 기울임 | ⌘I | Ctrl+I | |
| 밑줄 | ⌘U | Ctrl+U | |
| 확대 | ⌘= | Ctrl+=, Ctrl+Shift++ | |
| 축소 | ⌘- | Ctrl+- | |
| 100% 배율 | ⌘0 | Ctrl+0 | |
| 밝게/어둡게 전환 | ⌥⌘D | Ctrl+Alt+D | |
| 한글 문서 인쇄 | ⌘P | Ctrl+P | 데스크톱 앱, 한글 모듈 |
| 한글 문서 PDF 내보내기 | ⌘E | 없음 | macOS만. 다른 OS의 Ctrl+E는 편집기의 삭제 키 |
| 삽입/덮어쓰기 전환 | Insert | Insert | 입력란 밖, 해당 모듈이 처리 |

## 슬라이드

슬라이드 모듈에서만 동작한다. 첫째·둘째 줄은 입력란 밖에서만 잡는다.

| 동작 | 키 |
| --- | --- |
| 처음부터 슬라이드 쇼 | F5 |
| 현재 슬라이드부터 슬라이드 쇼 | Shift+F5 |
| 슬라이드 위로 이동(썸네일 목록 안) | Alt+↑ (macOS는 ⌥↑) |
| 슬라이드 아래로 이동(썸네일 목록 안) | Alt+↓ (macOS는 ⌥↓) |
| 슬라이드 삭제(썸네일 목록 안) | Delete |

## macOS 메뉴 막대

macOS 메뉴 막대에만 있는 가속키다. 위 표와 겹치는 항목은 생략했다.

| 동작 | 키 |
| --- | --- |
| 셀 선택(표) | ⌥⌘T |

Windows와 Linux는 앱 창 안의 메뉴 줄을 쓰므로 편집 키(복사 등)는 편집기가 처리한다.

## 한글 편집기

HWP·HWPX 편집기(rhwp-studio)가 자체로 처리하는 키다. 편집기에 입력 포커스가 있을 때만 동작한다. 아래 표에서 Ctrl은 macOS에서 ⌘, Alt는 ⌥이다. 한글 IME 상태에서도 같은 물리 키로 동작한다.

핵심 항목만 적었다. 전체는 `shortcut-map.ts`에 있다.

| 분류 | 동작 | 키 |
| --- | --- | --- |
| 편집 | 전체 선택 | Ctrl+A |
| 편집 | 지우기 | Ctrl+E |
| 편집 | 모양 복사 | Alt+C |
| 파일 | 새 문서 | Alt+N |
| 서식 | 글자 모양 | Alt+L |
| 서식 | 문단 모양 | Alt+T |
| 서식 | 스타일 | F6 |
| 서식 | 글꼴 크기 키우기 / 줄이기 | Alt+Shift+E / Alt+Shift+R, Ctrl+] / Ctrl+[ |
| 서식 | 줄 간격 줄이기 / 늘리기 | Alt+Shift+A / Alt+Shift+Z |
| 서식 | 장평 줄이기 / 늘리기 | Alt+Shift+J / Alt+Shift+K |
| 서식 | 자간 줄이기 / 늘리기 | Alt+Shift+N / Alt+Shift+W |
| 정렬 | 왼쪽 | Ctrl+Shift+L |
| 정렬 | 양쪽 | Ctrl+Shift+M |
| 정렬 | 오른쪽 | Alt+Shift+H |
| 정렬 | 가운데 | Alt+Shift+C |
| 정렬 | 배분 | Alt+Shift+D |
| 쪽 | 쪽 설정 | F7 |
| 쪽 | 쪽 나누기 | Ctrl+Enter |
| 쪽 | 단 나누기 | Ctrl+Shift+Enter |
| 쪽 | 단 설정 | Ctrl+Alt+Enter |
| 검색 | 찾기 | Ctrl+F |
| 검색 | 찾아 바꾸기 | Ctrl+F2 |
| 검색 | 다시 찾기 | Ctrl+L |
| 검색 | 쪽 이동 | Alt+G |
| 입력 | 문자표 | Alt+F10 |
| 표 | 줄/칸 추가 | Alt+Enter |
| 표 | 줄/칸 지우기 | Alt+Delete |
| 표 | 블록 평균 | Ctrl+Shift+A |
| 표 | 블록 곱 | Ctrl+Shift+P |
| 보기 | 도구 상자 | Ctrl+F1 |

한글 편집기 안에서 셸로 넘어가는 키는 열기, 새로 만들기, 닫기, 끝내기, 저장, 다른 이름으로 저장, 최근 문서, 인쇄, PDF 내보내기(macOS), 명령 팔레트(Ctrl+K)다. 위 "셸 공통" 표의 동작이 실행된다. Ctrl 조합만 넘기고 입력한 글자는 넘기지 않는다.
