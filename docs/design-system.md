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
| 강조 (글자·선·호버) / 포커스 링 | `--accent-strong` / `--ui-primary-hover` / `--focus-ring` |
| 문서 작업 영역 / 종이 / 안내선 | `--doc-workspace` / `--doc-paper` / `--doc-guide-stroke` |
| 위험 / 경고 / 링크 | `--ui-danger` / `--ui-warning-*` / `--ui-link` |

대비 기준은 크롬 글자 4.5:1, 아이콘 3:1이며 테스트가 토큰 값으로 계산합니다. `--ui-text-hint`처럼 대비가 낮은 토큰은 작은 글자의 바탕 위에 쓰지 않습니다. 흰 글자와 `--ui-menu-open`이 4.2:1인 다크 테마에서는 열린 메뉴를 `--ui-selected` 바탕에 `--ui-text`로, 기본 버튼을 `--accent-strong` 바탕에 `--ui-bg` 글자로 그립니다. 라이트·다크 테마 모두에서 `--doc-guide-stroke`는 종이(`--doc-paper`) 위 4.5:1 이상을 유지합니다.

## 2. 테마

- 모드는 시스템 / 밝게 / 어둡게이며 기본은 시스템입니다. `ui/theme.js`가 `<head>`에서 동기로 `<html data-theme-mode data-theme-effective>`와 `color-scheme`을 적용하고, 선택을 `localStorage`의 `easyoffice.theme-mode`에 저장하며, 시스템 모드일 때만 OS 변경을 따릅니다.
- 메뉴는 보기 > 테마 > (시스템 설정 / 밝게 / 어둡게) 라디오 항목(`data-theme-mode-choice`)입니다. Ctrl+Alt+D와 상단 버튼은 밝게/어둡게 전환(명시 선택)입니다.
- 한글 편집기(iframe)에는 같은 모드가 전달됩니다.

## 3. 글꼴·아이콘

- UI 글꼴은 `--font-family-ui`(`-apple-system, BlinkMacSystemFont, '맑은 고딕', 'Malgun Gothic', 'Segoe UI', sans-serif`). 문서용 글꼴은 OFL 번들(PD-081, PD-092)입니다.
- 아이콘은 `ui/icons.js`의 24x24 선 아이콘 한 벌(`currentColor`)만 씁니다. 이모지·화살표·도형 문자를 아이콘 대용으로 쓰지 않고, 아이콘만 있는 컨트롤은 `aria-label`을 갖습니다.
- 드롭다운 화살표(콤보, 색 선택 단추, 이름 상자, 필터 단추)는 글자가 아니라 Lucide `chevron-down` 그림입니다(`.eo-chevron`, `toolbar.css`, PD-377). 숫자 칸의 위·아래 단추는 `chevron-up`/`chevron-down`입니다. 글자 화살표(`▸ ▾ → ‹ ›` 등)는 `▾`도 예외 없이 `scripts/test-design-guard.mjs`가 막고(`SANCTIONED_CARETS`는 비어 있습니다), CSS `content`의 `"\25BE"` 같은 이스케이프도 같이 봅니다. 체크·라디오 메뉴 항목은 글리프 없이 참조 앱처럼 `aria-checked="true"`에 `.md-item.active`와 같은 바탕·글자색(`--ui-selected`, `--color-primary-dark`)을 씁니다.
- 포커스는 어디서나 `--color-focus-border` 1px 윤곽을 안쪽에 그립니다(`styles.css`의 `:focus-visible`, rhwp-studio `.sb-btn:focus-visible`). 컨트롤마다 따로 링을 만들지 않습니다(메뉴 항목·홈 화면처럼 참조 앱이 따로 정한 곳 제외).
- 색 선택은 `ui/color-picker.js`(팔레트 + 최근 색 + 숨긴 네이티브 입력)를 씁니다. 대화상자나 `#board-style-bar`(`board-style-bar.js`, `.sb-color-host.dialog-color-host` + `.dialog-swatch`)에서도 보이는 `<input type="color">`는 두지 않습니다.

## 4. 창 구성 (현재, 단계적으로 참조 앱 구조로 이동 중)

문서 하나가 창 하나입니다(PD-088). 시작 창도 같은 크기의 일반 창이고 참조 앱의 홈 화면(`home-screen.js`, `home.css`)을 보여 줍니다. 상단 메뉴 줄, 아이콘 도구 상자, 서식 도구줄, 편집 영역, 상태 표시줄 순이며 한글 창은 rhwp 도구모음 하나만 보입니다. 리본, 빠른 실행 도구 모음은 없습니다(PD-098 10항). 폭이 넓은 도구줄(`#icon-toolbar`, `#sheet-table-bar`, `#board-toolbar`)은 `.tb-scroll-viewport`/`.tb-scroll-track`/`.tb-scroll-nav`(`«`/`»`, `toolbar-scroll.js`)로 단일 행 스크롤을 제공합니다. 페이지 문서(`eoDocFlavor="pages"`)에서는 공유 토큰과 기존 컨트롤(`.pages-insp-tab`, `.custom-select`, `.eo-glyph-btn`)로 구성한 오른쪽 `#pages-inspector`(`텍스트`/`문서` 탭)를 표시합니다. 정적·동적 도구 대화상자(`#export-dialog`, `#base-import-dialog`, `#sheet-size-dialog` 등)는 `.tool-dialog` 안에 `.dialog-wrap/.dialog-title/.dialog-body/.dialog-footer` 규칙과 `enableDialogDrag`/`enableDialogKeys`, 단일 줄 `.dialog-label`, 테두리 중첩 없는 `.custom-select` 구조를 따릅니다.

## 4-1. 화면에는 설명 문장을 두지 않는다 (QA 2026-11-12, "참조 앱과 같게"보다 우선)

버튼·메뉴·필드 이름 같은 기능 라벨, "최근 문서" 같은 구역 제목, 상태·오류 메시지("열기 실패: …", "일치 3/10")만 화면에 둡니다. 사용법을 알려 주거나 기능을 풀어 쓰는 문장(홈 제목·부제, 끌어다 놓기 안내, 빈 상태 안내, 대화상자 안내 줄, 입력란 안내 문구, 긴 툴팁)은 넣지 않습니다. 툴팁(`title`)은 기능 이름과 단축키만 씁니다. 빈 상태는 빈 작업 영역과 비활성 도구줄로 보입니다. `scripts/test-webui-no-explanations.mjs`가 홈 화면·정적/동적 생성 대화상자(`#base-import-dialog`, `#export-dialog` 등)·빈 화면에 문장형 요소가 다시 들어오는 것을 막습니다.

## 4-2. 대화상자 부품 키트 (PD-375)

대화상자는 아래 클래스만 쓰고, 크기는 `tokens.css`의 토큰으로만 적습니다. 구조와 색은 참조 앱의 `dialogs.css`(표의 줄 번호)를 따르고, 크기(높이·글자·안쪽 여백·간격·모서리·테두리 두께)는 Spectrum S 크기표(4-3, PD-378)를 따릅니다. 키트에 없는 컨트롤 클래스(콤보·스핀·목록·탭·단추·색 선택 같은 새 부품)는 이 표에 PD 번호와 함께 한 줄을 먼저 올린 뒤에 씁니다. `scripts/test-design-guard.mjs`가 index.html의 `<dialog>`에서, `scripts/webkit-dom/dialogs.mjs`가 열린 대화상자(스크립트가 만든 요소 포함)에서 이를 확인합니다. 키트 전에 만든 클래스와 리터럴 px는 `scripts/design-guard-known.mjs`에 있고 그 목록은 줄이기만 합니다.

| 클래스 | 부품 | 크기 (토큰) | 참조 앱 | Spectrum S | PD |
| --- | --- | --- | --- | --- | --- |
| `.tool-dialog` `.dialog-wrap` | 창 틀 | 폭은 창마다, 글자 `--font-size-base` | dialogs.css 17-24 | 글자 12px | PD-098, PD-378 |
| `.dialog-title` `.dialog-close` | 제목 줄(끌어서 이동)과 닫기 | 높이 `--dialog-title-height`, 좌우 안쪽 `--dialog-title-padding`, 글자 `--control-font-size` 굵게, 닫기 `--control-height-sm` 정사각 | dialogs.css 26-54 | 높이 24px, 글자 12px | PD-098, PD-378 |
| `.dialog-body` `.dialog-section` `.dialog-section-title` | 본문, 묶음 틀 | 본문 안쪽 `--dialog-padding`, 묶음 안쪽 `--space-5` `--space-6`, 묶음 제목 `--font-size-sm` | dialogs.css 56-108 | 묶음 사이 `--dialog-group-gap` 8px | PD-098, PD-378 |
| `.dialog-row` `.dialog-stack` | 한 줄(라벨과 컨트롤), 세로 쌓기 | 간격 `--dialog-gap`, 줄 아래 `--dialog-row-gap`, 세로 가운데 맞춤 | dialogs.css 110-119 | 열 간격 8px, 줄 간격 12px | PD-098, PD-378 |
| `.dialog-label` | 라벨 | 글자 `--font-size-sm`, 최소 폭 `--dialog-label-min-width`, 찾기류 열은 `--dialog-label-width`, 오른쪽 맞춤, 칸 위 라벨은 글자 위 `--label-top-to-text`·아래 `--label-bottom-to-text` | dialogs.css 121-126, find-dialog.css 36-38 | 글자 12px, 위 4px, 아래 5px | PD-098, PD-378 |
| `.dialog-input` `.dialog-text-input` | 숫자·글 입력 | 높이 `--control-height-sm`, 글자 `--control-font-size`, 좌우 안쪽 `--control-padding-x`, 테두리 `--control-border-width`, 모서리 `--control-radius` | dialogs.css 128-146·634-646, find-dialog.css 42-47 | 높이 24px, 글자 12px, 글자 시작 9px, 테두리 1px, 모서리 2px(앱 값) | PD-098, PD-378 |
| `.dialog-select` `.sb-combo` `.native-select-hidden` `.custom-select` `.custom-select-trigger` `.custom-select-value` `.custom-select-arrow` `.custom-select-sizer` `.custom-select-menu` `.custom-select-option` `.custom-select-group` | 콤보(`ui/custom-select.js`가 `<select class="dialog-select">`를 `.custom-select`로 바꾸고 원래 요소는 `.native-select-hidden`) | 높이 `--control-height-sm`, 글자 `--control-font-size`, 글자 시작 `--control-padding-x`, 쉐브론 칸 `--chevron-size`와 끝 안쪽 `--picker-padding-end`, 글자와 쉐브론 `--picker-text-gap`, 화살표 칸의 구분선은 없고 쉐브론만 | dialogs.css 154-169 | 높이 24px, 글자 12px, 글자 시작 9px, 쉐브론 10px, 끝 7px, 글자와 쉐브론 5px | PD-098, PD-377, PD-378 |
| `.dialog-checkbox` `.dialog-radio-group` `.dialog-icon-radio-group` | 체크·라디오(`input[type="checkbox"]`·`[type="radio"]`는 전부 `shell.css`의 한 규칙) | 글자 `--control-font-size`, 상자 `--checkbox-size`, 테두리 `--checkbox-border-width`, 모서리 `--checkbox-radius`, 글자와 상자 `--checkbox-label-gap` | dialogs.css 176-193·572-584 | 상자 12px, 테두리 2px, 모서리 2px, 글자와 상자 9px | PD-098, PD-378 |
| `.dialog-unit` | 단위 표기 | 글자 `--font-size-xs` | dialogs.css 171-174 | 글자 11px(`font-size-50`) | PD-098, PD-378 |
| `.dialog-swatch` `.dialog-swatch-chip` `.dialog-color-host` `.sb-color-host` `.eo-color-input` `.eo-color-palette` `.sb-hl-palette` `.sb-hl-palette-row` `.sb-hl-palette-actions` `.sb-hl-swatch` | 색 선택(`ui/color-picker.js`) | 높이 `--control-height-sm`, 콤보와 같은 안쪽 | toolbar.css 팔레트 | 콤보와 같음 | PD-098, PD-378 |
| `.dialog-btn-group` | 붙은 단추 묶음 | 높이 `--control-height-sm`, 글자 `--control-font-size`, 끝 모서리 `--control-radius` | dialogs.css 586-624 | 높이 24px, 글자 12px | PD-098, PD-378 |
| `.dialog-footer` `.dialog-btn` `.dialog-btn-primary` | 아래 단추 줄 | 높이 `--control-height-sm`, 글자 `--control-font-size`, 안쪽 `--button-padding-x`, 테두리 `--button-border-width`, 모서리 `--button-radius`, 단추 간격 `--dialog-button-gap` | dialogs.css 195-238 | 높이 24px, 글자 12px, 안쪽 12px, 테두리 1px·모서리 2px(앱 값), 간격 12px | PD-098, PD-378 |
| `.dialog-tabs` `.dialog-tab` `.dialog-tab-panel` | 탭 | 높이 `--dialog-tab-height`, 좌우 안쪽 `--dialog-tab-padding-x`, 글자 `--control-font-size` | dialogs.css 530-570 | 촘촘한 S 탭: 높이 24px, 시작 12px, 글자 12px | PD-098, PD-378 |
| `.dialog-list` | 목록 상자 | 글자 `--font-size-sm`, 행 안쪽 `--list-row-padding-y` `--list-row-padding-x` | style-dialog.css 30-50 | 글자 12px | PD-098, PD-378 |
| `.dialog-grid` `.dialog-grid-wide` `.dialog-field` `.dialog-swatch-text` | 라벨·칸·라벨·칸·단위 열 격자와 칸이 셀을 채우는 폭, 색 단추의 이름 | 간격 `--dialog-row-gap` `--dialog-gap`, 글자 `--font-size-sm`, 미리 보기 `--dialog-preview-height` | char-shape-dialog.css 4-31 | 줄 간격 12px, 열 간격 8px | PD-376, PD-378 |
| `.eo-stepper` `.eo-stepper-input` `.eo-stepper-buttons` `.eo-stepper-button` `.eo-chevron` `.eo-chevron-up` | 숫자 칸(`ui/number-stepper.js`가 모든 `input[type="number"]`를 `.eo-stepper`로 감싸고, 칸의 클래스는 상자가 이어받는다)과 드롭다운·스텝퍼의 Lucide 쉐브론 | 칸 높이는 칸 그대로(`--control-height-sm`), 단추 셀 폭 `--stepper-width`, 단추 쉐브론 `--stepper-chevron-size`, 글자 안쪽 `--control-padding-x`, 단추는 Spectrum의 쌓인 칸 채움: 바탕 `--stepper-fill-bg`(호버 `--stepper-fill-bg-hover`, 비활성 `--stepper-fill-bg-disabled`), 테두리 `--stepper-fill-border-width`(칸 테두리 색을 따름), 셀 왼쪽 줄 하나, 두 칸 사이 줄 하나, 칸 사이 간격 0 | Spectrum CSS `components/stepper/index.css`, `components/infieldbutton/index.css`, Lucide `chevron-down`·`chevron-up` | 높이 24px, 단추 셀 20px, 쉐브론 6px(`chevron-icon-size-50`), 글자 시작 9px, 채움 `gray-100`/`gray-200`, 테두리 1px | PD-377, PD-378 |
| `.dialog-status-row` `.dialog-status` `.dialog-pdf-note` | 상태·주석 줄 | 최소 높이 `--dialog-status-height`, 글자 `--font-size-sm` | find-dialog.css 59-68, dialogs.css 249-275 | 글자 12px | PD-098, PD-378 |

한 줄(`.dialog-row`, `.dialog-footer`, `.dialog-btn-group`) 안의 컨트롤은 높이와 위쪽 위치가 같고, 높이는 `--control-height-*`(Spectrum S의 24px 하나), 글자 크기는 `--control-font-size*`(12px 하나)입니다(라벨은 제외). 열린 대화상자의 어떤 상자도 내용이 넘쳐 잘리지 않습니다.

## 4-3. 컨트롤 크기표: Spectrum S (QA 2026-11-28, PD-378)

크기(높이·글자·안쪽 여백·간격·아이콘)는 Spectrum CSS(Apache-2.0, 보관된 저장소 커밋 `70d3660a`, `THIRD_PARTY_NOTICES.md`의 Spectrum CSS 항목)의 작은(S) 크기, 데스크톱 배율 값입니다. 색은 이 앱의 토큰 그대로(가족 룩)이고, 모서리와 단추 테두리 두께는 이 앱의 2px·1px입니다(QA 2026-11-28, 표에 Spectrum 값을 함께 적음). 원본 위치는 `tokens/dist/json/tokens.json`(`tokens.json:줄`)과 `components/<이름>/index.css`(`<이름>/index.css:줄`)입니다. 아래 "측정" 열은 열린 화면을 WebKit에서 2x로 잰 값입니다.

| 요소 | 토큰 | Spectrum S | 원본 위치 | 측정 |
| --- | --- | --- | --- | --- |
| 칸·콤보·단추 높이 | `--control-height-sm` | 24px (`component-height-75`) | tokens.json:3339, textfield/index.css:116 | 24px |
| 컨트롤 글자 | `--control-font-size` | 12px (`font-size-75`) | tokens.json:5407, textfield/index.css:119 | 12px |
| 글자 시작(왼쪽 안쪽) | `--control-padding-x` | 10px - 테두리 1px = 9px (`component-edge-to-text-75`) | tokens.json:3186, textfield/index.css:313 | 9px |
| 칸 테두리 두께 | `--control-border-width` | 1px (`border-width-100`) | tokens.json:1353, textfield/themes/spectrum-two.css:26 | 1px |
| 칸 모서리 | `--control-radius` | Spectrum S는 8px (`corner-radius-100`)이나 앱의 2px를 씀(QA 2026-11-28) | tokens.json:3610, textfield/index.css:24 | 2px |
| 쉐브론 그림 크기 | `--chevron-size` | 10px (`chevron-icon-size-75`) | tokens.json:2331 | 10px |
| 콤보 끝 안쪽(쉐브론 오른쪽) | `--picker-padding-end` | 7px (`field-end-edge-to-disclosure-icon-75`) | tokens.json:4876, picker/index.css:102 | 7px |
| 콤보 글자와 쉐브론 사이 | `--picker-text-gap` | 5px (`text-to-visual-75`) | tokens.json:11960, picker/index.css:95 | 5px |
| 스텝퍼 단추 셀 폭 | `--stepper-width` | 20px (`in-field-button-width-stacked-small`) | tokens.json:6702, stepper/index.css:92 | 20px |
| 스텝퍼 쉐브론 | `--stepper-chevron-size` | 6px (`chevron-icon-size-50`, 스토리의 S 스텝퍼는 `ChevronUp50`·`ChevronDown50`) | tokens.json:2304, stepper/stories/template.js:31-32 | 6px |
| 스텝퍼 채움 테두리 | `--stepper-fill-border-width` | 1px (`in-field-button-border-width` = `stepper-button-border-width` = `border-width-100`) | stepper/themes/spectrum-two.css:31, infieldbutton/themes/spectrum-two.css:16 | 1px |
| 스텝퍼 채움 바탕 | `--stepper-fill-bg` `--stepper-fill-bg-hover` `--stepper-fill-bg-disabled` | `gray-100` rgb(233) / 호버·눌림 `gray-200` rgb(225) / 비활성 `disabled-background-color` = `gray-100` (앱 토큰 `--ui-surface-muted` #e8e8e8 / `--ui-hover` #e0e0e0) | tokens.json:5608·5626·4414, infieldbutton/themes/spectrum-two.css:26-28 | rgb(232) / rgb(224) / rgb(232) |
| 스텝퍼 칸 사이, 칸과 가장자리 간격 | (없음: 값이 0) | 0px (`in-field-button-stacked-inner-edge-to-fill`, `in-field-button-edge-to-fill`), 안쪽 모서리 0 (`in-field-button-fill-stacked-inner-border-rounding`) | tokens.json:6686·6642·6646 | 칸 사이 0 |
| 스텝퍼 줄 | (테두리) | 칸마다 1px 테두리: 위 칸은 아래 테두리 없음, 아래 칸은 사방, 색은 상자를 따름(`inherit`) | infieldbutton/index.css:176-232 | 왼쪽 줄 1, 칸 사이 줄 1 |
| 체크·라디오 상자 | `--checkbox-size` | 12px (`checkbox-control-size-small`, `radio-button-control-size-small`) | tokens.json:2151·9282, checkbox/index.css:72-79 | 12px |
| 체크·라디오 테두리 | `--checkbox-border-width` | 2px (`border-width-200`) | tokens.json:1357, checkbox/index.css:55 | 2px |
| 체크 상자 모서리 | `--checkbox-radius` | 2px (`checkbox-control-corner-radius`) | checkbox/themes/spectrum-two.css:22 | 2px |
| 라디오 점 | `--radio-dot-size` | 4px (`radio-button-selection-indicator`) | tokens.json:9291 | 4px |
| 체크·라디오 글자와 상자 | `--checkbox-label-gap` | 9px (`text-to-control-75`) | tokens.json:11906, checkbox/index.css:79 | 9px |
| 단추 높이 | `--control-height-sm` | 24px (`component-height-75`) | button/index.css:32 | 24px |
| 단추 글자 | `--control-font-size` | 12px (`font-size-75`) | button/index.css:33 | 12px |
| 단추 안쪽(왼쪽·오른쪽) | `--button-padding-x` | 13px - 테두리 1px = 12px (`component-pill-edge-to-text-75`, 테두리는 앱의 1px라 2px 대신 1px를 뺀다) | tokens.json:3375, button/index.css:37 | 12px |
| 단추 테두리 두께 | `--button-border-width` | Spectrum S는 2px (`border-width-200`)이나 앱의 1px를 씀(QA 2026-11-28) | tokens.json:1357, button/index.css:348 | 1px |
| 단추 모서리 | `--button-radius` | Spectrum S는 높이 / 2 = 12px (알약)이나 앱의 2px를 씀(QA 2026-11-28) | button/index.css:351 | 2px |
| 단추 사이 간격 | `--dialog-button-gap` | 12px (`spacing-200`, ButtonGroup sizeS) | tokens.json:10228, buttongroup/index.css:23 | 12px |
| 라벨 글자 위 / 아래 | `--label-top-to-text` `--label-bottom-to-text` | 4px / 5px (`component-top-to-text-75`, `component-bottom-to-text-75`) | tokens.json:3528·3141, fieldlabel/index.css:31-32 | 4px / 5px |
| 라벨 옆 간격 | `--dialog-gap` | 8px (`spacing-100`, FieldLabel sizeS 옆 안쪽) | tokens.json:10220, fieldlabel/index.css:36 | 8px |
| 줄 간격 | `--dialog-row-gap` | 12px (`spacing-200`, Form 라벨 위 배치) | tokens.json:10228, form/index.css:16 | 12px |
| 묶음 사이 | `--dialog-group-gap` | 8px (`spacing-100`) | tokens.json:10220 | 8px |
| 탭 | `--dialog-tab-height` `--dialog-tab-padding-x` | 높이 24px (`tab-item-compact-height-small`), 시작 12px (`tab-item-start-to-edge-small`), 글자 12px | tokens.json:10694·10811, tabs/index.css:133-137·67 | 24px / 12px / 12px |
| 제목 줄 | `--dialog-title-height` | 24px (`component-height-75`; Spectrum의 대화상자 제목은 크기별 값이 없다) | 같은 높이를 씀 | 24px |
| 포커스 고리 | `--focus-ring-width` | 2px (`focus-indicator-thickness`) | tokens.json:5295 | 2px |

Spectrum 대화상자(`dialog/index.css`)에는 S 크기 값이 없고(안쪽 격자 40px, 제목 18px는 크기와 무관한 M 값), 이 앱의 촘촘한 도구 대화상자에 쓰기에 너무 커서 본문 안쪽 `--dialog-padding`(12px = `spacing-200`)과 제목 높이만 S 값으로 맞춥니다.

## 5. 새 화면을 만들 때

1. 색은 위 토큰만, 간격·반경은 `--space-*`·`--radius-*`.
2. 새 클래스를 만들기 전에 rhwp-studio의 `tb-btn`, `sb-*`, `dialog-btn`, `md-item` 같은 기존 클래스를 씁니다.
3. 인라인 `style="…"` 속성과 `<style>` 주입을 쓰지 않습니다(CSP).
4. 라이트와 다크 둘 다에서 확인하고, 문서 내용 색이 필요하면 `content.css`에 둡니다.
5. 기본 버튼은 `--accent-strong` 바탕입니다(참조 앱은 `--accent-primary`, 흰 글자 대비 3.7:1이라 4.5:1에 못 미쳐 의도적으로 다릅니다, PD-098 5항·PD-166).
6. UI·UX 구성은 rhwp-studio(참조 앱) 기준을 우선 따르고, rhwp-studio에 대응 요소가 없는 시트·슬라이드·PDF 고유 구성 요소(슬라이서 등)는 표준 데스크톱 오피스 프로그램의 배치·동작 해부 구조를 따르되 색·테두리·아이콘은 위 토큰과 `ui/icons.js`로만 그립니다(QA 승인 2026-11-19).
7. 대화상자는 4-2의 키트 클래스만 씁니다. 키트에 없는 컨트롤 클래스가 필요하면 표에 PD 번호와 함께 한 줄을 먼저 올립니다.
8. 높이·글자 크기·안쪽 여백·간격은 크기 토큰(`--control-height-*`, `--font-size-*`, `--space-*`, `--dialog-*`)으로만 적고 리터럴 px를 새로 쓰지 않습니다. 필요한 토큰이 없으면 참조 앱 값으로 `tokens.css`에 더합니다.
9. 같은 대화상자가 참조 앱에 있으면 두 앱을 나란히 캡처해 라벨 시작 위치, 컨트롤 높이, 글자 크기, 단추 줄이 같은지 봅니다. 없으면 같은 키트의 가장 가까운 대화상자와 맞춥니다.
10. `scripts/webkit-dom/dialogs.mjs`에 대화상자를 여는 단계를 더해 줄 맞춤·크기 토큰·잘림 검사를 통과시킵니다.
