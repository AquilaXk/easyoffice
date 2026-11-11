# EasyOffice UI 디자인 기준 (PD-098)

EasyOffice는 자체 디자인 시스템을 두지 않습니다. 모든 창은 참조 앱이 쓰는 rhwp-studio 크롬을 기준으로 하며, 이 문서는 그 기준을 코드에서 지키는 규칙만 적습니다. 결정과 단계별 상태는 `docs/policy-decisions.md`의 PD-098입니다. Signature Lavender 팔레트와 Swift 시절의 설명은 폐기했습니다(Git 이력에 남아 있습니다).

## 1. 토큰

- `crates/easyoffice-app/ui/tokens.css`가 색·글꼴·간격·반경·그림자 토큰의 유일한 정의입니다. 값은 rhwp-studio `src/styles/base.css`와 같습니다(`--ui-*`, `--accent-*`, `--doc-*`, `--focus-ring`, `--font-family-ui` 등). 라이트가 기본이고 다크는 `:root[data-theme-effective="dark"]`가 덮어씁니다.
- `styles.css`와 `shell.css`는 색을 `var(--…)`로만 씁니다. 리터럴 색, `--eo-*` 변수, `data-theme="…"` 선택자는 둘 수 없습니다(`test-webui-theme.mjs`가 검사).
- 문서 종이·공문서 서식·슬라이드 쇼처럼 테마와 무관한 고정 색은 `content.css`에만 둡니다. 종이(`#doc-editor`, `.slide-stage`, `.pdf-page-card`)는 라이트 `--ui-*`·`--accent-*`와 `--color-*` 별칭을 `content.css`의 한 규칙에서 다시 선언해 어두운 테마에서도 밝게 유지됩니다. 별칭은 `:root`에서 이미 풀리므로 `--ui-*`만 다시 선언하면 다크 값이 새어 들어옵니다. 종이 안 컨트롤은 토큰만 읽으면 됩니다.

| 용도 | 토큰 |
| --- | --- |
| 창 배경 / 메뉴·도구 상자 배경 / 입력·패널 면 | `--ui-bg` / `--ui-bg-light` / `--ui-surface` |
| 글자 (본문 / 보조 / 설명 / 힌트) | `--ui-text` / `--ui-text-secondary` / `--ui-text-muted` / `--ui-text-hint` |
| 경계선 (기본 / 옅은 / 강한 / 구분) | `--ui-border` / `--ui-border-light` / `--ui-border-strong` / `--ui-border-subtle` |
| 호버 / 선택 / 열린 메뉴 | `--ui-hover` / `--ui-selected` / `--ui-menu-open` |
| 강조 (글자·선) / 포커스 링 | `--accent-strong` / `--focus-ring` |
| 문서 작업 영역 / 종이 | `--doc-workspace` / `--doc-paper` |
| 위험 / 경고 / 링크 | `--ui-danger` / `--ui-warning-*` / `--ui-link` |

대비 기준은 크롬 글자 4.5:1, 아이콘 3:1이며 테스트가 토큰 값으로 계산합니다. `--ui-text-hint`처럼 대비가 낮은 토큰은 작은 글자의 바탕 위에 쓰지 않습니다. 흰 글자와 `--ui-menu-open`이 4.2:1인 다크 테마에서는 열린 메뉴를 `--ui-selected` 바탕에 `--ui-text`로, 기본 버튼을 `--accent-strong` 바탕에 `--ui-bg` 글자로 그립니다.

## 2. 테마

- 모드는 시스템 / 밝게 / 어둡게이며 기본은 시스템입니다. `ui/theme.js`가 `<head>`에서 동기로 `<html data-theme-mode data-theme-effective>`와 `color-scheme`을 적용하고, 선택을 `localStorage`의 `easyoffice.theme-mode`에 저장하며, 시스템 모드일 때만 OS 변경을 따릅니다.
- 메뉴는 보기 > 테마 > (시스템 설정 / 밝게 / 어둡게) 라디오 항목(`data-theme-mode-choice`)입니다. Ctrl+Alt+D와 상단 버튼은 밝게/어둡게 전환(명시 선택)입니다.
- 한글 편집기(iframe)에는 같은 모드가 전달됩니다.

## 3. 글꼴·아이콘

- UI 글꼴은 `--font-family-ui`(`-apple-system, BlinkMacSystemFont, '맑은 고딕', 'Malgun Gothic', 'Segoe UI', sans-serif`). 문서용 글꼴은 OFL 번들(PD-081, PD-092)입니다.
- 아이콘은 `ui/icons.js`의 24x24 선 아이콘 한 벌(`currentColor`)만 씁니다. 이모지·화살표·도형 문자를 아이콘 대용으로 쓰지 않고, 아이콘만 있는 컨트롤은 `aria-label`을 갖습니다.
- 예외 한 가지. `▾`(콤보 화살표, 색 선택 단추)는 참조 앱과 같은 글리프라 `aria-hidden` 요소 안에 둡니다. 체크·라디오 메뉴 항목은 글리프 없이 참조 앱처럼 `aria-checked="true"`에 `.md-item.active`와 같은 바탕·글자색(`--ui-selected`, `--color-primary-dark`)을 씁니다.
- 포커스는 어디서나 `--color-focus-border` 1px 윤곽을 안쪽에 그립니다(`styles.css`의 `:focus-visible`, rhwp-studio `.sb-btn:focus-visible`). 컨트롤마다 따로 링을 만들지 않습니다(메뉴 항목·홈 화면처럼 참조 앱이 따로 정한 곳 제외).
- 색 선택은 `ui/color-picker.js`(팔레트 + 최근 색 + 숨긴 네이티브 입력)를 씁니다. 보이는 `<input type="color">`는 두지 않습니다.

## 4. 창 구성 (현재, 단계적으로 참조 앱 구조로 이동 중)

문서 하나가 창 하나입니다(PD-088). 시작 창도 같은 크기의 일반 창이고 참조 앱의 홈 화면(`home-screen.js`, `home.css`)을 보여 줍니다. 상단 메뉴 줄, 아이콘 도구 상자, 서식 도구줄, 편집 영역, 상태 표시줄 순이며 한글 창은 rhwp 도구모음 하나만 보입니다. 리본, 빠른 실행 도구 모음, 측면 패널은 없습니다(PD-098 10항). 스마트 보조·개인정보 스캔·보안 검사·성능 진단·문서 개요는 메뉴에서 여는 대화상자(`.tool-dialog` 안에 `.dialog-wrap/.dialog-title/.dialog-body/.dialog-footer`)이고, 시트·슬라이드·PDF 재입힘은 이후 단계에서 이 절을 갱신합니다.

## 4-1. 화면에는 설명 문장을 두지 않는다 (QA 2026-11-12, "참조 앱과 같게"보다 우선)

버튼·메뉴·필드 이름 같은 기능 라벨, "최근 문서" 같은 구역 제목, 상태·오류 메시지("열기 실패: …", "일치 3/10")만 화면에 둡니다. 사용법을 알려 주거나 기능을 풀어 쓰는 문장(홈 제목·부제, 끌어다 놓기 안내, 빈 상태 안내, 대화상자 안내 줄, 입력란 안내 문구, 긴 툴팁)은 넣지 않습니다. 툴팁(`title`)은 기능 이름과 단축키만 씁니다. 빈 상태는 빈 작업 영역과 비활성 도구줄로 보입니다. `scripts/test-webui-no-explanations.mjs`가 홈 화면·대화상자·빈 화면에 문장형 요소가 다시 들어오는 것을 막습니다.

## 5. 새 화면을 만들 때

1. 색은 위 토큰만, 간격·반경은 `--space-*`·`--radius-*`.
2. 새 클래스를 만들기 전에 rhwp-studio의 `tb-btn`, `sb-*`, `dialog-btn`, `md-item` 같은 기존 클래스를 씁니다.
3. 인라인 `style="…"` 속성과 `<style>` 주입을 쓰지 않습니다(CSP).
4. 라이트와 다크 둘 다에서 확인하고, 문서 내용 색이 필요하면 `content.css`에 둡니다.
5. 기본 버튼은 `--accent-strong` 바탕입니다(참조 앱은 `--accent-primary`, 흰 글자 대비 3.7:1이라 4.5:1에 못 미쳐 의도적으로 다릅니다, PD-098 5항·PD-166).
