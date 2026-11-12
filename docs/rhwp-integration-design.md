# EasyOffice × rhwp 통합 설계서 (HWP 엔진·편집기 도입)

- 작성일: 2026-11-06
- 대상 저장소: `aquilaxk/easyoffice` `origin/main` = `54edd83` (#121 Windows 데스크톱 셸 병합 직후). #111(파일 열기)은 아직 `feat/webui-file-open`(`9c0911c`)에만 있다.
- 참조 저장소(읽기 전용, 실행·빌드하지 않음)
  - rhwp: `https://github.com/edwardkim/rhwp` @ `1a76570e833917d15817415a53c09ad61ab3203f` (v0.8.7, 2026-10-06). MIT.
  - 참조 앱: `` @ `80888af2b8c8a18aea16f824d2d92309a290b1ad` (v0.4.5, 2026-10-08). MIT. 이 커밋의 서브모듈 gitlink도 rhwp `1a76570e…`(v0.8.7)를 가리킨다.
- 표기: `[확인]` = 소스에서 직접 확인, `[추정]` = 실측하지 않은 추정치. 추정치는 첫 구현 PR에서 실측으로 바꿔야 한다.

---

## 0. 요약 (결론 먼저)

1. **rhwp는 HWP/HWPX/HWP3/HML 전용**이다. DOCX·ODT·DOC 문서 본문을 읽거나 쓰는 코드가 없다(`src/`에서 `wordprocessingml`·`.docx`·`odt` 0건). `crates/rhwp-ooxml-chart`는 HWPX 안에 든 차트 XML만 처리한다. 따라서 **HWP 모듈만 rhwp로 바꾸고, DOC 모듈은 EasyOffice 엔진을 유지**하되 문자 런(run) 모델을 보강한다(§2.6, 작업 항목 7).
2. **임베딩 방식: 샌드박스 `<iframe>` 격리 + MessagePort RPC**를 권장한다(PD-083 개정: 처음의 같은 출처 iframe은 셸의 Tauri IPC를 물려받는 것이 실측돼 `allow-same-origin` 없는 불투명 출처로 바꿨다. PD-083 5항). rhwp-studio에 이미 있는 임베드 프로토콜(`rhwp-studio/src/embed/{protocol,runtime,rpc-router}.ts`: `loadFile`, `exportHwp`, `exportHwpx`, `notifySaved`, `getDocumentState`, `automation.execute`, `chrome.set`, `document-change-events-v1`)을 그대로 쓴다. iframe 안의 편집기에는 **Tauri IPC 권한을 주지 않는다**. 파일 I/O는 모두 기존 부모 셸(`desktop-bridge.js` → `save_document_file`, #111 `openDocumentFromBytes`)이 맡는다.
3. **참조 앱에서 가져올 것은 "패턴"이다**: 서브모듈 고정·검증(`config/rhwp-upstream.json`, `scripts/verify-rhwp-upstream.mjs`), Vite alias 오버레이(`apps/studio-host/vite.config.ts`, `참조 앱-overrides.ts`), 미리 빌드한 WASM 벤더링과 출처 증명(`vendor/rhwp-core/PROVENANCE.json`), 단계식 원자적 저장(`state.rs` `atomic_write`, `commit_staged_hwp_save`), 단일 인스턴스와 파일 열기 이벤트(`pending_open.rs`), 글꼴 카탈로그 정책(`assets/fonts/FONTS.md`, `font-catalog.ts`).
   - 참조 앱의 권한 모델(`fs:allow-write`·`fs:allow-remove`를 WebView에 주고 경로를 JS가 다루는 방식)은 EasyOffice PD-074의 fail-closed 경계와 맞지 않으므로 **가져오지 않는다**.
4. **빌드: rhwp는 EasyOffice Cargo 워크스페이스에 넣지 않는다**(1단계~5단계).
   - WASM은 참조 앱처럼 `vendor/rhwp-core/`에 커밋하고 SHA-256으로 검증한다. 원본 빌드 기준은 1.93.1 + wasm-pack 0.14.0이다.
   - 편집기 번들은 `apps/hwp-studio/`(Vite)에서 빌드해 `crates/easyoffice-app/ui/hwp-studio/`(gitignore 대상)로 출력한다.
   - 그러면 엔진 크레이트 MSRV 1.85와 데스크톱 1.90은 그대로 유지된다.
5. **위생 가드의 최소 변경**: 파일 시스템 순회에서 `third_party/rhwp`(서브모듈 작업 트리)와 빌드 산출물 `ui/hwp-studio/`를 루트 기준 경로로 제외한다. 대신 "gitlink 고정 커밋·원격 URL·`.gitmodules` 허용 목록"을 새 검사로 추가한다.
   - EasyOffice가 추적하는 파일에 대한 금지 규칙(AI 도구 파일 금지 등)은 그대로 둔다.

---

## 1. 무엇을 재사용하고 무엇을 유지하는가

### 1.1 rhwp에서 재사용 (수정 없이, 서브모듈로)

| 대상 | 경로 | 용도 |
|---|---|---|
| WASM 코어 | `src/wasm_api.rs`(9051줄, `HwpDocument` 클래스, `js_name` export 약 467개), `src/document_core/**`, `src/parser/{cfb_reader.rs,hwpx/,hml/,hwp3/}`, `src/serializer/{cfb_writer.rs,hwpx/writer.rs,hml/}`, `src/renderer/**` | 파싱, 조판, 편집 명령, 렌더, HWP/HWPX/HML 직렬화 |
| 편집기 UI | `rhwp-studio/src/**`: `core/wasm-bridge.ts`, `view/canvas-view.ts`, `view/page-render-scheduler.ts`, `engine/{history,command}.ts`, `command/**`, `ui/**`, `embed/**` | 캔버스 편집 화면, 캐럿·선택, 문자/문단 모양 대화상자, 표 편집, 실행 취소, 임베드 RPC |
| 임베드 SDK 참고 | `npm/editor/{index.js,transport.js}`(`@rhwp/editor`, iframe 임베드) | 부모 측 프로토콜 구현의 참고본. 코드를 직접 가져오지 않고 `hwp-host.js`를 새로 쓴다 |
| hwpctl 호환층 | `rhwp-studio/src/hwpctl/{index.ts,action-registry.ts,parameter-set.ts,actions/*}` (`HwpCtrl`) | 리본 명령을 rhwp 액션(`CharShape` 등)에 매핑할 때 쓰는 2차 수단 |
| 글꼴 | `assets/fonts/*.woff2` 중 OFL 글꼴, `ttfs/opensource/NotoSansKR-*.ttf` + `NotoSansKR-OFL.txt` | 화면 대체 글꼴. PDF 글꼴은 6단계 이후 |

주요 WASM API(`src/wasm_api.rs` 줄 번호) `[확인]`:

| 분류 | API |
|---|---|
| 생성·로드 | `new(data)`(588, 내부적으로 `DocumentCore::from_bytes`), 비밀번호 변형, `createEmpty`(617), `createBlankDocument`(634) |
| 렌더 | `pageCount`(756), `renderPageSvg`(762), `renderPageToCanvas`(804) |
| 편집 | `insertText`(1314), `deleteText`(1424), `applyCharFormat`(7964), `applyParaFormat`(8272), `applyCharFormatAtCursor`(5997) |
| 캐럿·히트 테스트 | `getCursorRect`(2892), `hitTest`(2947) |
| 스냅샷 | `saveSnapshot` / `restoreSnapshot` / `discardSnapshot`(7073~7085) |
| 직렬화 | `exportHwp`(6813), `exportHwpx`(6853), `exportHml`(6885), `exportHwpVerify`, `*WithReport` |

**WASM에는 PDF 내보내기가 없다.** PDF는 네이티브 전용인 `svg2pdf` 경로를 탄다(`vendor/svg2pdf`, MIT/Apache-2.0 패치본).

**지원 범위**:

| 항목 | 지원 내용 |
|---|---|
| 읽기 | HWP 5.0, HWPX, HWPML/HML 2.9x, HWP 3.x(조합형 포함) |
| 배포용 문서 | 읽기 지원 |
| 암호 문서 | HWP5 EncryptVersion 4, HWP3, HWPX ODF AES-256-CBC |
| 쓰기 | HWP 5.0, HWPX, HML. HWP3은 쓰기 없음 |

배포용·암호 문서 읽기는 현재 EasyOffice `Ole2SecurityGuard`가 `0x02`/`0x04` 비트로 fail-closed 차단하는 대상과 겹친다. 정책 결정이 필요하다(§8 Q5).

### 1.2 참조 앱에서 재사용 (코드 복사가 아니라 설계 패턴. 복사하면 MIT 고지 필요)

| 참조 앱 경로 | 가져올 것 | 버릴 것 / 바꿀 것 |
|---|---|---|
| `.gitmodules`, `config/rhwp-upstream.json`(`version`/`tag`/`commit`/`rustToolchain 1.93.1`/`wasmPackVersion 0.14.0`) | 단일 원천(SSOT) 고정 파일 | `cargoPatches`는 네이티브 어댑터(§3.6)를 도입할 때만 사용 |
| `scripts/verify-rhwp-upstream.mjs`, `docs/operations/RHWP_UPDATE.md` | 검증 항목 전체: 원격 URL, HEAD = 고정 커밋, 작업 트리 clean, 버전 3종 일치, 벤더 파일 크기·SHA-256, 오버라이드 원본 해시 | `pnpm` 대신 Node 내장 모듈만 사용(EasyOffice 스크립트 관례) |
| `apps/studio-host/vite.config.ts`, `참조 앱-overrides.ts`, `config/rhwp-studio-overrides.json` | alias 순서(오버라이드 id → `@wasm/rhwp.js` → `@/upstream` → `@upstream` → `@`). 각 오버라이드 항목에 `strategy`(extension/fork/contribution)와 `reason`을 필수로 둠. 원본 파일의 SHA-256 기준선 | 참조 앱의 데스크톱 전용 오버라이드(`tauri-bridge.ts`, `chunked-fs.ts`, `home-screen`, `recent-documents`, `update-notice`)는 쓰지 않음. EasyOffice는 부모 셸이 그 역할을 함 |
| `apps/studio-host/vendor/rhwp-core/{rhwp.js,rhwp_bg.wasm,rhwp.d.ts,package.json,LICENSE,PROVENANCE.json}` | `wasm-pack --target web` 산출물 벤더링 + 출처 증명. CI는 WASM을 다시 빌드하지 않고 해시만 검증 | 경로는 `vendor/rhwp-core/`(저장소 루트) |
| `apps/studio-host/src/core/font-loader.ts`(fork), `font-catalog.ts`, `font-authoring-policy.ts`, `assets/fonts/FONTS.md` | HWP 글꼴명을 번들 OFL woff2에 대응. 제한 글꼴(HY*, 휴먼명조 등)을 작성 단계에서 차단. `FontFace` API 사용 | `<style>` 주입을 없애도록 fork 범위를 정함(§4) |
| `apps/studio-host/src/core/document-open.ts` | `loadDocument(bytes, fileName)` + 비밀번호 재시도 루프(`showHwpPasswordDialog`) | — |
| `apps/studio-host/src/ui/print-dialog.ts` | 모든 쪽을 `renderPageSvg(i)`로 숨은 루트에 그린 뒤 `@media print`로 인쇄하고, 5쪽마다 양보 | 셸 창에서 `print_webview` 명령(PD-139; `--serve`는 `window.print()`) |
| `apps/desktop/src-tauri/src/state.rs` | `DocumentSession{source_fingerprint{len,modified_millis,content_hash}, dirty, revision}`, `NamedTempFile::new_in(parent)` → `write_all` → `sync_all` → `persist` | EasyOffice `save_file.rs`의 원자적 쓰기를 유지하고, 외부 변경 지문만 추가 |
| `apps/desktop/src-tauri/src/{pending_open.rs,windows.rs,lib.rs}` | `tauri-plugin-single-instance`의 argv → 열기 큐, 창별 큐 | 다중 창은 PD-088(#132)에서 구현: 창별 대기 큐는 경로 대신 바이트·이름만 담는다 |
| `apps/desktop/src-tauri/src/pdf_export.rs` + `apps/desktop/rhwp-adapter` | 네이티브 rhwp `render_page_svg_native` → `svgs_to_pdf_with_options`(검색 가능 PDF), 저장 전 재파싱 검증 | 6단계 이후 선택 사항(툴체인 1.93.1 필요) |
| `tauri.conf.json` `fileAssociations`(`hwp`·`hwpx`, mime `application/x-hwp`·`application/vnd.hancom.hwpx`) | 파일 연결 정의 | NSIS 기준으로 맞추고, #111 셸 열기 명령이 생긴 뒤 등록 |

참조 앱에서 **가져오지 않는 것**:

- `fs:*` 플러그인 권한과 런타임 `fs_scope().allow_file`
- CSP의 `'unsafe-eval'`
- 업데이터
- macOS QuickLook
- 미사용 네이티브 편집 명령(`render_page_svg`, `query_document`, `mutate_document`). 등록만 되어 있고 프런트엔드에서 호출하지 않는다 `[확인]`.

### 1.3 EasyOffice가 유지하는 것

- **엔진 크레이트 전부**: `easyoffice-formats`(20종 포맷 레지스트리, 컨테이너 보안 가드, 변환기 코덱. `hwp5_codec.rs`, `hwpx_codec.rs`, `rich_*` 포함), `easyoffice-sheet`, `easyoffice-layout`, 그리고 `easyoffice-app`(15종 IPC, `--serve`, 리본, PII).
- **모듈 UI**: SHEET, SLIDE, PDF, 일괄 변환기(CONV)는 변경 없음. DOC은 기존 contenteditable 편집기를 유지하되 문자 런 모델을 보강(작업 항목 7).
- **변환기**: HWP↔DOCX/PDF 변환은 계속 `convert_document_format`(EasyOffice 코덱)이 맡는다. rhwp 기반 변환은 §3.6 네이티브 어댑터 도입 뒤 별도 결정(§8 Q4).
- **셸**: 15종 IPC 계약, 단일 `ipc_dispatch`, `save_document_file`(경로는 Rust 대화상자에서만 나옴), `on_navigation` 제한, `withGlobalTauri: false`.
- **macOS**: macOS도 같은 Tauri 셸(WKWebView)로 배포하므로(PD-094) 이 설계는 **Windows·macOS 셸과 `--serve`** 모두를 대상으로 한다. 같은 `hwp-studio` 번들이 실행 파일에 내장되고, macOS CI의 `--smoke`가 한글 편집기 왕복을 확인한다.

---

## 2. 임베딩 방식과 상호 운용

### 2.1 선택지 비교

| 방식 | 장점 | 단점 | 판정 |
|---|---|---|---|
| **A. 같은 출처 iframe + MessagePort RPC** | 1) rhwp-studio 전역 상태(`main.ts`의 `document.body` 조작, 전역 `keydown`, 메뉴바 CSS)와 EasyOffice 전역 단축키·CSS·1,000줄 규칙이 충돌하지 않음<br>2) 업스트림 임베드 프로토콜이 이미 존재하며 출처 검사(`isUsableParentOrigin`: `http:`/`https:`만 허용)가 Windows 셸 출처 `http://tauri.localhost`와 `--serve` `http://127.0.0.1`을 모두 만족<br>3) 편집기에 IPC 권한이 없어 공격 표면 최소 | 1) 포커스가 iframe 안에 있으면 부모 단축키가 동작하지 않음 → 포워딩 필요<br>2) 리본과 편집기 도구모음이 두 겹 | **권장** |
| B. rhwp-studio 모듈을 EasyOffice DOM에 직접 마운트 | 단일 DOM, 리본 직접 연결 | 1) EasyOffice UI는 비모듈 IIFE 스크립트이고 Vite/TS 모듈과 섞어야 함<br>2) 업스트림 `main.ts` 전역 초기화를 대량 fork해야 함(참조 앱도 `main.ts`를 자체 작성)<br>3) 위생 가드의 `ui/*.js` 1,000줄·`innerHTML` 규칙과 충돌<br>4) 업스트림 갱신 비용 큼 | 기각 |
| C. EasyOffice UI 전체를 rhwp-studio 위에 재작성(참조 앱 방식) | 참조 앱과 동일 구조 | 다중 모듈 작업 공간(SHEET/SLIDE/PDF/CONV)을 버려야 함 | 기각 |
| D. 네이티브 권위 세션(Rust가 문서 보유, WebView는 SVG만 표시) | 큰 문서 메모리·속도 유리 | 1) 참조 앱도 아직 미완(`render_page_svg`·`mutate_document` 미사용)<br>2) 캐럿·IME·선택을 전부 재구현해야 함<br>3) `--serve`와 경로가 갈림 | 장기 과제(§6.2) |

### 2.2 구조 (방식 A)

```
index.html (EasyOffice 셸, CSP 엄격)
 ├─ 탭 줄 · 모듈 전환 · 파일/편집 메뉴 · 상태 표시줄 · 리본   ← EasyOffice 소유
 ├─ #pane-doc  (DOC: 기존 #doc-editor contenteditable)
 └─ #pane-hwp  (HWP: 새 패널)
      └─ <iframe id="hwp-studio-frame" src="./hwp-studio/index.html"
                 title="한글 문서 편집기" referrerpolicy="no-referrer">
            rhwp-studio (EasyOffice 오버레이 빌드)
            ├─ embed runtime: 부모와 MessagePort 1개
            ├─ WasmBridge → rhwp_bg.wasm (iframe 안에서만 인스턴스화)
            └─ 업스트림 메뉴바/도구모음 (chrome.set으로 일부 숨김)
```

현재 DOC/HWP는 `#doc-editor` 하나를 공유한다(`index.html:369`). HWP 모듈 활성 시 `#pane-doc` 대신 `#pane-hwp`를 보이도록 `applyModuleAccent` 분기를 추가한다.

iframe은 **HWP 모듈을 처음 활성화할 때 지연 생성**한다. 그래야 12 MB WASM 로드가 앱 시작을 막지 않는다.

### 2.3 부모 ↔ iframe 프로토콜 (`crates/easyoffice-app/ui/hwp-host.js`, 신규 IIFE, 1,000줄 미만, `innerHTML` 미사용)

- 연결: `iframe.contentWindow.postMessage({type:'rhwp-connect', version:1, sessionId, capabilities:['transferable-array-buffer', ...]}, location.origin, [port2])`.
  - 출처는 `location.origin`으로 고정하고 `'*'`은 쓰지 않는다.
  - 응답은 `port1`로만 받는다.
- 사용할 메서드(업스트림 `rpc-router.ts`에 존재) `[확인]`:

  | 용도 | 메서드 |
  |---|---|
  | 상태 확인 | `ready`, `pageCount`, `getDocumentState`, `getSelectionContext` |
  | 열기 | `loadFile({data: ArrayBuffer, fileName, skipUnsavedGuard, suppressDialogs})` |
  | 저장 | `exportHwp`, `exportHwpx`, `notifySaved` |
  | 명령 실행 | `automation.list`, `automation.isEnabled`, `automation.execute` |
  | 화면 크롬 | `chrome.get`, `chrome.set` |
  | 변경 이벤트 | `document-change-events-v1` |

- EasyOffice 오버레이에서 **비활성화할 capability**: `plugin-loader-v1`, `document-agent-command-v1`, `hwpctrl-v1`(빌드 시 `RHWP_WITHOUT_HWPCTRL=1`). 부모가 쓰지 않는 기능은 공격 표면이므로 오버레이 `embed/runtime`에서 제거한다(`strategy: fork`, `reason` 명시).
- 큰 바이트는 `ArrayBuffer`를 **transfer 목록으로 이동**한다(복사 없음).

### 2.4 열기 (#111 `openDocumentFromBytes`)

현재 #111(`feat/webui-file-open`)의 흐름은 다음과 같다:

1. `file-open.js` `openDocumentFromBytes(fileName, bytes)`
2. → `convert_document_format { sourceBase64, action:"decode" }`
3. → 구조화 모델(`blocks`/`sheets`/`slides`)
4. → 모듈 적재(10 MiB 한도)

변경 사항:

1. 확장자와 매직 바이트가 HWP 계열(`hwp`, `hwpx`, `hwt`, `hwtx`, `hml`)이면 디코드 IPC를 거치지 않는다.
   - 먼저 `detect_format` IPC(기존 15종 중 하나)로 EasyOffice `FormatDetector` 시그니처·크기 검사를 그대로 통과시킨다. 이로써 fail-closed 입구 검사가 유지된다.
   - 그다음 `hwpHost.loadFile(bytes, name)`을 호출한다.
2. rhwp 로드에 실패하면(파싱 오류·미지원) 오류를 표시하고 끝낸다. **조용히 기존 평문 디코드로 대체하지 않는다**(편집 후 저장 시 서식 유실을 숨기지 않기 위함).
   - 사용자가 원하면 "텍스트만 가져오기"를 명시 선택하게 할 수 있다(§8 Q6).
3. 10 MiB 한도는 `--serve` HTTP 본문 16 MiB와 Base64 팽창 때문에 생긴 값이다. HWP 경로는 IPC를 거치지 않고 iframe으로 직접 가므로 **HWP는 64 MiB**로 올릴 수 있다(값은 §8 Q8). ZIP·OLE 보안 상한은 rhwp 내부 가드에 의존하므로 EasyOffice `ZipSecurityGuard` 기준과 동등한지 3단계에서 퍼징 표본으로 확인한다.
4. "저장하지 않은 편집이 있으면 확인 후 교체"는 `getDocumentState().dirty` 기준으로 판단한다.
5. 열기는 EasyOffice 실행 취소 스냅샷의 한 단위였지만, HWP 문서 상태는 rhwp가 보유한다. 따라서 HWP 열기는 **EasyOffice 실행 취소 스택에 넣지 않고**, rhwp 쪽 이력을 초기화하는 새 문서 경계로 취급한다.

### 2.5 저장 (#104 구조화 페이로드 / `save_document_file`)

- HWP 모듈의 `Ctrl+S`는 다음 순서로 처리한다:
  1. `hwpHost.exportHwp()`. HWPX 원본이면 `exportHwpx()`. 형식 선택은 §8 Q3.
  2. `ArrayBuffer`
  3. `desktopBridge.saveFile(base64, fileName)`(셸), 또는 Blob 다운로드(`--serve`)
  4. 성공하면 `notifySaved`와 EasyOffice 수정 기준선 갱신.
- **#104 구조화 페이로드(`blocks`)는 HWP 모듈에서 쓰지 않는다.** 그 경로는 HTML → 평탄 모델 → `HwpxCodec::pack_rich`로 가는 손실 경로다. rhwp는 원본 레코드를 보존하는 직렬화(`exportHwp`)를 제공한다.
  - 단 "다른 형식으로 내보내기(DOCX/PDF/ODT)"는 기존 변환기(`convert_document_format`, `sourceBase64` = rhwp가 내보낸 HWP 바이트)로 처리한다. 15종 IPC 계약은 변경 없다.
- **업스트림 주의점**: 참조 앱은 HWPX 원본 저장을 막고 HWP로만 저장한다("HWPX 원본 저장은 아직 안전하게 지원하지 않습니다"). 비밀번호 문서는 경고 후 비밀번호 없이 저장한다. 같은 정책을 기본값으로 삼는다.
- **저장 검증**: 6단계 네이티브 어댑터 도입 전에는 저장 직전 iframe 안에서 `exportHwpVerify`(또는 내보낸 바이트를 `new HwpDocument(bytes)`로 재파싱) 결과가 성공일 때만 저장한다. 참조 앱은 Rust에서 재파싱한다.
- **전송 효율**: `save_document_file`은 Base64 문자열(256 MiB 상한)을 받는다. 수십 MB HWP에서 Base64 인코딩·JSON 직렬화·디코딩이 3회 복사를 만든다.
  - 4단계에서 같은 명령을 Tauri v2 원시 바이트 본문(`tauri::ipc::Request`, `InvokeBody::Raw`; 파일 이름은 헤더)으로 받는 형태로 바꾸거나, 같은 권한 범위의 `save_document_bytes`로 대체한다.
  - 명령 수 변화는 위생 가드 허용 목록에서 1:1로 갱신한다.

### 2.6 DOCX(DOC 모듈) 처리 — 명시적 결론

- rhwp와 rhwp-studio는 **DOCX를 전혀 다루지 않는다**. 저장소 전체에서 docx가 나오는 곳은 AI 도구용 스킬 디렉터리(`.claude/skills/rhwp-exam-ingest`, python-docx) 하나뿐이다 `[확인]`.
- DOC 모듈은 **EasyOffice 엔진 경로를 유지**한다: #111 `decode_rich`(`rich_read_ooxml.rs`) → 편집 → #104 `blocks` → `rich_writers`.
- 사용자가 지적한 결함("글자 크기가 편집기 전체에 적용", "굵게·기울임이 문단 단위", "탭 UI")은 DOC에도 그대로 남는다. 별도 작업 항목 7로 고친다:
  1. 선택 범위에만 `<span data-eo-run>`을 만들어 런 속성(`font`, `sizePt`, `color`, `bold`, `italic`, `underline`, `strike`)을 붙이는 **선택 기반 런 분할기**. `document.execCommand`를 쓰지 않고 Range API로 처리하며, 인라인 `style` 속성 대신 `dataset` + CSSOM(`el.style.fontSize`)을 쓴다. CSP `style-src 'self'`에서 허용되는 방식이다.
  2. `applyStudioFontSize`(`workspace-tools.js:780~786`)를 `editor.style.fontSize`에서 선택 범위 런 적용으로 교체.
  3. `save-payload.js` 런 스키마와 `RichDocument`/`rich_writers`에 `sizeHalfPt`·`font`·`color` 추가. DOCX는 `w:sz`/`w:rFonts`/`w:color`, HWPX는 `charPr height`/`fontRef`/`textColor`. PD-073 Trade-off 1을 해소한다.
- **기각한 대안**: DOCX → HWPX 변환 후 rhwp로 편집. OOXML과 HWP의 스타일·번호 매기기·구역 의미가 달라 왕복할 때마다 손실이 생기고, 사용자는 DOCX를 기대한다.
  - "한글 편집기에서 열기"를 명시적 변환 명령으로 제공하는 것은 선택 사항이다(§8 Q2).

### 2.7 탭 · 메뉴 · 리본 · 단축키 · 자동 저장 · 실행 취소

| 영역 | 설계 |
|---|---|
| 탭(`#tab-strip`) | **[대체됨: PD-088(#132) — 문서마다 개별 창, 탭 줄·모듈 전환 탭 없음. 아래 "탭만" 기본값은 더 이상 유효하지 않다.]** 1) 1단계(작업 항목 3~4)는 HWP 문서 1개<br>2) 5단계에서 "HWP 탭 1개 = iframe 1개", 동시 4개 상한(메모리, §6.1)<br>3) 탭 제목은 PD-073 파일명 규칙 재사용<br>4) 비활성 탭 iframe은 `hidden`으로 두고 언로드하지 않음 |
| 파일 메뉴 | 1) 열기·저장·다른 이름으로 저장·인쇄는 EasyOffice 메뉴가 소유<br>2) 오버레이가 업스트림 메뉴바의 `file:*` 항목을 `chrome.set`/CSS로 숨김<br>3) 업스트림 `command/commands/file.ts`(File System Access, `showSaveFilePicker`, `?url=` fetch)는 오버레이에서 **부모 위임 구현으로 교체**(참조 앱의 `command/commands/file` extension과 같은 위치) |
| 편집기 도구모음 | 1) rhwp 서식 도구모음(글꼴, 크기, 굵게, 문단, 표)을 그대로 사용(문자 모양과 조판 정확성이 핵심 가치)<br>2) HWP 활성 시 EasyOffice `#doc-style-bar`와 문서 전용 리본 명령은 숨기거나 비활성(PD-072 3항 규칙 확장) |
| 리본 연결(5단계) | 1) 리본 140종 중 HWP에 의미가 있는 명령(굵게·기울임·밑줄·정렬·표 삽입·쪽 설정 등)만 `automation.execute(id)`로 매핑<br>2) 매핑이 없으면 비활성<br>3) 매핑표는 `ribbon-catalog.js`에 데이터로 둠(140종 수치 불변) |
| 단축키 | 1) iframe에 포커스가 있으면 부모 `keydown`이 실행되지 않음<br>2) 오버레이가 `WORKSPACE_SHORTCUT_BINDINGS` 중 셸 소유 키(`Ctrl+S`, `Ctrl+O`, `Ctrl+K`, `Ctrl+Alt+D`)만 `preventDefault` 후 포트로 부모에 전달<br>3) `Ctrl+Z/Y/B/I/U`는 rhwp가 처리<br>4) 키 목록은 부모가 연결 시 보냄(SSOT 유지) |
| 실행 취소 | 1) HWP 문서 이력의 원천은 rhwp(`engine/history.ts` `CommandHistory`(최대 1000) + Rust 스냅샷)<br>2) EasyOffice 편집 메뉴의 실행 취소/다시 실행은 HWP 활성 시 `automation.execute('edit:undo'/'edit:redo')`<br>3) `ribbon_session.rs`의 HWP 모듈 스택(PD-069)은 HWP 활성 시 쓰지 않음 |
| 수정 상태 | 1) PD-072의 DOM FNV 지문 대신 `document-change-events-v1` + `getDocumentState()`(revision/dirty) 사용<br>2) `beforeunload` 등록 조건에 "HWP dirty" 추가 |
| 자동 저장 | 1) PD-072 `localStorage` 스냅샷은 5 MB 한도라 HWP 바이너리를 담을 수 없음<br>2) HWP는 **IndexedDB**(`easyoffice.hwp.autosave.v1`)에 `{tabId, fileName, format, savedAt, revision, bytes}`를 디바운스 저장(`exportHwp` 결과)<br>3) `localStorage` 스냅샷에는 탭 참조만 넣음<br>4) 업스트림 `src/recovery/`(IndexedDB 복구)를 그대로 쓸지 부모 소유로 할지는 §8 Q9. 권장은 부모 소유: 복원 UI와 검증을 EasyOffice가 통제하고, 복원 전 `FormatDetector` 통과를 필수로 함<br>5) 큰 문서의 `exportHwp` 비용(§6.1) 때문에 HWP 자동 저장 간격은 5초 이상 + 유휴 시점으로 함 |
| 인쇄 | 1) 5단계에서 참조 앱 `print-dialog.ts` 방식 적용(iframe 안에서 `renderPageSvg` → `@media print` → `window.print()`)<br>2) WebView2 인쇄 대화상자의 "PDF로 저장"이 6단계 전까지의 PDF 경로 |
| 테마 | rhwp-studio 토큰을 그대로 사용(`docs/design-system.md`, PD-098). 업스트림 `theme-init.js`는 부모가 보낸 테마 값으로 초기화 |

---

## 3. 빌드 파이프라인

### 3.1 디렉터리 배치 (제안)

```
.gitmodules                         # [submodule "third_party/rhwp"] url=https://github.com/edwardkim/rhwp.git, shallow=true
config/rhwp-upstream.json           # version/tag/commit/rustToolchain/wasmPackVersion (참조 앱 형식)
config/rhwp-studio-overrides.json   # 오버라이드 목록 + 업스트림 원본 SHA-256
third_party/rhwp/                   # 서브모듈(gitlink, 읽기 전용)
vendor/rhwp-core/                   # rhwp.js, rhwp_bg.wasm(약 11.8 MB), rhwp.d.ts, package.json, LICENSE, PROVENANCE.json
apps/hwp-studio/                    # Vite 오버레이 (package.json, package-lock.json, vite.config.ts, index.html, src/**)
assets/fonts/                       # OFL 글꼴만 (FONTS.md에 출처·라이선스)
scripts/verify-rhwp-upstream.mjs    # Node 내장 모듈만 사용
scripts/build-hwp-studio.mjs        # npm ci --ignore-scripts && vite build → crates/easyoffice-app/ui/hwp-studio/
crates/easyoffice-app/ui/hwp-studio/  # 빌드 산출물 (.gitignore)
THIRD_PARTY_NOTICES.md              # rhwp, (참조 앱 코드 복사 시) 참조 앱, WASM 내 Rust 의존성, 글꼴 OFL
```

### 3.2 서브모듈 크기 문제 (중요)

rhwp 저장소는 **추적 파일 41,624개, 약 5.3 GB**다 `[확인]`(`git ls-files` 합계).

| 디렉터리 | 크기 |
|---|---|
| `mydocs/` | 2.6 GB |
| `pdf/` | 1.3 GB |
| `samples/` | 686 MB |
| `tools/` | 573 MB |
| `rhwp-studio/` | 16.5 MB |
| `src/` | 29.5 MB |

참조 앱 CI는 `submodules: true`로 전체를 받는다. EasyOffice는 다음처럼 대응한다.

- **CI·개발자 기본: sparse + shallow 서브모듈.**
  - 순서: `git submodule update --init --depth 1 --filter=blob:none third_party/rhwp` → `git -C third_party/rhwp sparse-checkout set --no-cone /Cargo.toml /Cargo.lock /rust-toolchain.toml /LICENSE /THIRD_PARTY_LICENSES.md /src/ /crates/ /vendor/svg2pdf/ /rhwp-studio/ /assets/fonts/ /ttfs/opensource/ /npm/editor/`.
  - 정확한 목록은 `vite build` 성공과 참조 앱 검증 스크립트 기준으로 확정한다. `build.rs`가 참조하는 파일이 있으면 추가한다.
  - `actions/checkout@v4`의 `submodules:` 옵션은 sparse를 지원하지 않으므로 별도 스크립트(`scripts/checkout-rhwp.mjs`)로 처리한다.
- **studio를 빌드하지 않는 작업에서는 서브모듈을 받지 않는다.** macOS `verify`, 위생 가드, WebUI 노드 테스트가 여기에 해당한다. 위생 가드는 gitlink를 `git ls-files -s`로 검사하므로 작업 트리가 없어도 된다.

### 3.3 Rust 툴체인 분리

| 대상 | 툴체인 | 근거 |
|---|---|---|
| 엔진 크레이트 4종 | MSRV 1.85(`rust-version.workspace = true`), CI는 러너 기본 stable | 변경 없음 |
| `easyoffice-desktop` | `rust-version = "1.90"` | 변경 없음(1~5단계) |
| rhwp WASM | **1.93.1** + `wasm32-unknown-unknown` + wasm-pack 0.14.0 | `third_party/rhwp/rust-toolchain.toml` |
| (선택) `crates/easyoffice-rhwp-adapter` | `rust-version = "1.93"`, `default-members` 제외 | 6단계 이후. 이 크레이트에 의존하는 순간 desktop도 1.93 이상이 됨 |

- **WASM은 커밋된 벤더 산출물을 쓴다(참조 앱 방식).** CI는 `vendor/rhwp-core/*` 크기와 SHA-256을 `PROVENANCE.json`과 대조할 뿐 다시 빌드하지 않는다. 이유는 다음과 같다:
  1. 매 CI에 1.93.1 + wasm-pack + rhwp 릴리스 빌드(의존성에 암호 크레이트 다수, `wasm_api.rs` 9천 줄)를 추가하면 Windows 작업 시간이 크게 는다(§3.5).
  2. 엔진 MSRV 정책과 툴체인이 섞이지 않는다.
- **재현 검증**: `.github/workflows/rhwp-wasm-rebuild.yml`(수동 실행, ubuntu)이 `RUSTUP_TOOLCHAIN=1.93.1 wasm-pack build --target web --release`로 다시 빌드해 해시를 비교한다.
  - wasm-bindgen 산출물에는 빌드 경로가 패닉 메시지로 들어갈 수 있다. `RUSTFLAGS=--remap-path-prefix=$PWD=/rhwp`로 고정하지 않으면 해시가 달라질 수 있다 `[추정]`.
  - 재현이 되지 않으면 "CI 빌드 산출물 = 벤더 갱신 PR의 유일한 원천"으로 정책을 정한다(§8 Q10).
- 주의: 경로 의존(`path = "../../third_party/rhwp"`)으로 빌드하면 툴체인은 **호출 디렉터리**의 것을 쓴다. 서브모듈의 `rust-toolchain.toml`은 적용되지 않으므로 6단계에서는 `cargo +1.93.1`을 명시해야 한다.

- **업스트림 갱신 절차**: `node scripts/update-rhwp.mjs vX.Y.Z --dry-run`으로 계획(태그 → 커밋, 바뀌는 오버라이드)을 확인한 뒤 `--dry-run` 없이 실행한다. 명시 태그만 받고, 오버라이드가 맞춰 둔 업스트림 파일이 바뀌었으면 실패하므로 `apps/hwp-studio`를 다시 맞춘 뒤 `--accept-overrides`로 기준선을 갱신한다. 설정·gitlink·sparse 체크아웃만 바꾸고 벤더 WASM은 다시 빌드하지 않는다. 이어서 위 방법으로 WASM을 빌드해 `vendor-rhwp-core.mjs`로 `PROVENANCE.json`을 갱신하고 `verify-rhwp-upstream.mjs`·위생 가드를 통과시킨다(PD-076).

### 3.4 프런트엔드 번들과 자산 포함

- `apps/hwp-studio/`는 참조 앱 `apps/studio-host`의 축소판이다.
  - 의존성: devDependencies `vite ^8.3.2`, `typescript ^7.0.2`(업스트림과 동일 메이저). `@tauri-apps/*`는 **넣지 않는다**(iframe은 IPC를 쓰지 않음).
  - `canvaskit-wasm`, `vite-plugin-pwa`, `puppeteer-core`는 제외한다. `renderer-session` 정책을 `canvas2d` 고정으로 두는 참조 앱 방식이다. canvaskit을 빼면 wasm 7 MB와 BSD 고지가 줄어든다 `[추정 크기]`.
  - 빌드 환경 변수: `RHWP_DISABLE_EXTERNAL_WEBFONTS=1`(jsDelivr CDN 글꼴 차단), `RHWP_WITHOUT_HWPCTRL=1`.
  - 패키지 관리자는 npm(`package-lock.json`, `npm ci --ignore-scripts`). EasyOffice는 이미 Node 22 + `npx`를 쓰므로 pnpm을 새로 도입하지 않는다.
  - 출력: `base: './'`, `outDir: ../../crates/easyoffice-app/ui/hwp-studio`, `emptyOutDir: true`, 글꼴은 `hwp-studio/fonts/`로 복사.
- **Tauri 셸**: `frontendDist`는 그대로 `../easyoffice-app/ui`다. 빌드 산출물이 그 아래 `hwp-studio/`에 있으므로 tauri-codegen이 함께 내장(brotli 압축)한다.
  - `.wasm` MIME은 `application/wasm`이어야 `instantiateStreaming`이 동작한다. 아니면 wasm-bindgen이 경고 후 `arrayBuffer` 경로로 대체한다. 3단계 스모크에서 확인한다.
  - 산출물이 없으면 셸은 HWP 패널에 "HWP 편집기 자산이 포함되지 않은 빌드"를 표시한다(fail-closed, 평문 편집기로 조용히 대체하지 않음).
- **`--serve`**: 현재 `http_server.rs`는 `include_str!` 고정 표와 `&str` 본문만 지원한다.
  - **기본 빌드(노드 없이 `cargo build`)가 계속 동작해야 하므로** 내장은 cargo 기능 `hwp-studio`(기본 off)로 둔다.
  - 기능을 켜면 `build.rs`가 `ui/hwp-studio/asset-manifest.json`(빌드 스크립트가 생성: 경로·MIME·SHA-256)을 읽어 `include_bytes!` 표를 생성한다. 응답 함수는 `&[u8]` 본문을 받도록 일반화한다.
  - 경로는 매니페스트 허용 목록만 서빙한다(디렉터리 순회 불가).
  - 릴리스의 CLI 프리뷰 zip은 기능을 켜고 빌드한다. 다른 방법인 실행 파일 옆 디렉터리 런타임 로드는 변조 표면이 생기므로 기각한다.
  - CSP 헤더는 경로별로 다르게 준다(§4.2).

### 3.5 CI · 릴리스 변경과 시간 예산

| 작업 | 변경 | 추가 시간 `[추정]` |
|---|---|---|
| `ci.yml` `verify`(macos-15) | 1) 서브모듈 미체크아웃<br>2) 위생 가드가 gitlink·`config/rhwp-upstream.json`·`vendor/rhwp-core` 해시 검증<br>3) `node --test scripts/test-webui-hwp-host.mjs`(가짜 iframe·MessagePort로 부모 프로토콜 검증) 추가 | +10~20초 |
| `ci.yml` `verify-windows`(windows-latest, 제한 50분) | 1) `scripts/checkout-rhwp.mjs`(sparse): +1~2분<br>2) `node scripts/verify-rhwp-upstream.mjs`<br>3) `npm ci --ignore-scripts` + `vite build`(`scripts/build-hwp-studio.mjs`): +1.5~3분, npm 캐시 사용<br>4) 번들 정적 검사(외부 URL·`eval`·`new Function`): +수 초<br>5) Tauri 빌드: 12 MB wasm brotli 압축·내장 +0.5~1분<br>6) `--smoke` 확장(HWP 패널 iframe `ready` → `createBlankDocument` → `exportHwp` 왕복): +10~20초<br>7) `cargo build -p easyoffice-app --features hwp-studio`로 `--serve` 경로별 CSP 스모크 | **합계 +4~7분** → 제한 50분 유지 |
| `release.yml` Windows | CI와 같은 단계 + `THIRD_PARTY_NOTICES.md`·글꼴 OFL 텍스트를 NSIS 리소스로 포함 | +4~7분 |
| `release.yml` macOS | 변경 없음(§8 Q1 결정 전) | 0 |
| 신규 `rhwp-wasm-rebuild.yml`(수동) | ubuntu-24.04, `rustup toolchain install 1.93.1 --target wasm32-unknown-unknown`, wasm-pack 0.14.0(바이너리 릴리스 해시 고정), 재빌드 후 해시 비교 | 콜드 10~20분 `[추정]` |

---

## 4. 보안 (CSP · 글꼴 · 워커 · 권한)

### 4.1 Tauri CSP (`crates/easyoffice-desktop/tauri.conf.json`)

Tauri CSP는 앱 전체에 하나만 적용된다(경로별로 나눌 수 없음). 필요한 최소 추가는 다음과 같다.

```json
"csp": {
  "default-src": "'self'",
  "connect-src": "'self' ipc: http://ipc.localhost",
  "script-src": "'self' 'wasm-unsafe-eval'",
  "img-src": "'self' data: blob:",
  "frame-src": "'self'",
  "object-src": "'none'",
  "base-uri": "'none'",
  "form-action": "'none'",
  "frame-ancestors": "'self'"
}
```

| 지시어 | 이유 |
|---|---|
| `'wasm-unsafe-eval'` | wasm-bindgen `--target web`의 `init()`은 `WebAssembly.instantiateStreaming`/`instantiate`를 호출하므로 CSP3 브라우저(Chromium 기반 WebView2 포함)에서 필요하다. **JS `eval`은 여전히 금지**된다. 참조 앱은 `'unsafe-eval'`까지 넣었지만 rhwp-studio 소스에는 `eval(`·`new Function`이 0건이다 `[확인]`. 따라서 `'unsafe-eval'`은 넣지 않는다. |
| `img-src data: blob:` | HWP 내장 이미지는 Blob/Data URL로 그려진다(참조 앱 CSP와 동일). |
| `frame-src 'self'` | `default-src 'self'`로도 같은 출처 iframe은 허용되지만 명시해 둔다. |
| `font-src` | 미지정, 즉 `default-src 'self'`. 번들 woff2만 허용한다. `FontFace(name, arrayBuffer)`는 `font-src` 대상이 아니다. CDN 글꼴은 빌드에서 비활성하고 CSP로도 차단된다(이중 방어). |
| `style-src` | **`'unsafe-inline'`을 넣지 않는 것을 목표**로 한다. 업스트림에서 `<style>` 요소를 동적으로 만드는 곳은 두 곳이다 `[확인]`: `core/font-loader.ts:135`(`@font-face` 규칙)와 `command/print-pages.ts:192`. 둘 다 오버레이에서 교체한다. font-loader는 참조 앱도 fork했으며 `FontFace` API만 쓰도록 바꾼다. print-pages는 정적 `print.css`로 옮긴다. `cssText`/`el.style.*` CSSOM 할당 118건은 CSP 차단 대상이 아니다. `office-clipboard-images.ts:310`의 `setAttribute('style')`은 붙여넣기 HTML 처리 경로이므로 오버레이 테스트에서 동작을 확인한다. 교체가 막히면 `'unsafe-inline'` 허용 여부를 결정해야 한다(§8 Q11). |
| 워커 | rhwp-studio에 `new Worker` 0건, 서비스 워커는 PWA 플러그인뿐이고 제외한다. `worker-src`는 미지정(`default-src 'self'`로 처리). |

위생 가드의 `DESKTOP_SHELL_CSP` 정확값 비교는 새 객체로 갱신한다. "정확값 일치" 방식 자체는 유지한다.

### 4.2 `--serve` CSP

- `/`, `/index.html` 및 기존 자산은 **`default-src 'self'` 그대로**다. CI 스모크가 정확값을 검사한다.
- `/hwp-studio/*` 응답에만 `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; img-src 'self' data: blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'self'`를 준다.
  - iframe 문서의 CSP는 그 문서 응답 헤더로 결정되므로 부모 페이지는 엄격한 정책을 유지한다.
  - CI 스모크에 `/hwp-studio/index.html` 헤더 검사를 추가한다.

### 4.3 권한(capabilities)과 fail-closed 유지

- `capabilities/main-window.json`은 **변경하지 않는다**. 대상 창은 `main`, 권한은 `allow-ipc-dispatch`·`allow-save-document-file`뿐이다.
  - iframe은 IPC를 직접 호출하지 않는다. 처음에는 "Tauri v2가 하위 프레임에 IPC를 주입하는지는 플랫폼에 따라 다르다 `[추정]`"로 두고 번들의 `__TAURI_INTERNALS__` 참조 부재만 정적 검사로 고정했다. 실측 결과 같은 출처 iframe은 `window.parent`(전 OS)와 Windows의 하위 프레임 주입 사본으로 창의 앱 명령에 닿았다. 그래서 iframe을 불투명 출처로 만들었고(`Origin: null` 요청은 Tauri가 거부), 편집기 문서 CSP에서 `ipc:`를 뺐으며, Windows의 `postMessage` 폴백은 문서로 닫을 수 없어 모든 앱 명령이 창의 호출 토큰을 요구한다(스모크 `frame_ipc`로 실측). 근거와 한계는 PD-083 5·6·11·12항.
- 5단계(파일 연결·단일 인스턴스)에서 명령이 늘어난다. 예: `open_document_file`은 Rust 대화상자로 경로를 고르고 바이트만 반환하며, 기동 인자 큐를 비우는 명령이 추가된다. 그때도 참조 앱처럼 `fs:*` 권한을 WebView에 주지 않고 앱 명령 + `AppManifest` 기본 거부 + 허용 목록 갱신 방식을 따른다.
- 오버레이에서 제거하는 업스트림 기능:
  1. `?url=` 임의 URL fetch(`main.ts:1874-1902`) — 오버레이는 자체 `main.ts`를 쓰므로 진입점 자체가 없다
  2. `/samples` fetch
  3. File System Access / `launchQueue`(PWA)
  4. 플러그인 로더 / 문서 에이전트 RPC
  5. 외부 웹글꼴
- 번들 정적 검사(`scripts/check-hwp-studio-bundle.mjs`)가 CI에서 다음을 실패로 처리한다:
  - `https?://` 리터럴. 허용 목록은 SVG/XML 네임스페이스 URI(`http://www.w3.org/...`)와 HWP 네임스페이스뿐이다.
  - `eval(`, `new Function`, `__TAURI`, `navigator.serviceWorker`.
- **입구 검사 유지**: HWP 바이트는 rhwp에 넘기기 전에 `detect_format`(EasyOffice `FormatDetector`, 512 MiB 상한, 시그니처 대조)을 통과해야 한다.
  - rhwp 파서 자체의 견고성 근거는 업스트림 `fuzz/`(445 파일)가 있다는 것 정도이며, 3단계에서 EasyOffice 보안 픽스처(ZIP 폭탄, 경로 순회, OLE FAT 순환)로 회귀 테스트를 추가한다.
  - WASM은 메모리 안전하고 샌드박스 안에서 돈다. 최악의 경우는 탭의 OOM이나 무한 루프이므로 iframe 재생성으로 복구한다.

---

## 5. 저장소 위생 가드와의 충돌

### 5.1 현재 동작 (`scripts/check-repo-hygiene.mjs`)

- `git ls-files`로 금지된 로컬 전용 경로(에이전트·도구 규칙 파일과 디렉터리 목록, 해시 인코딩된 `FORBIDDEN_GIT_PATH_B64`)가 **슈퍼프로젝트에 추적되는지**를 검사한다.
  - 서브모듈은 gitlink(`160000 third_party/rhwp`) 한 줄로만 나타난다(`--recurse-submodules`가 없으므로). **이 검사는 통과한다.**
- `walkTrackedCandidateFiles(ROOT)`는 이름과 달리 **디스크를 재귀 순회**한다. 서브모듈 작업 트리가 체크아웃되어 있으면 다음 문제가 생긴다:
  1. `third_party/rhwp` 안의 여러 AI 도구 규칙 파일은 `IGNORED_ROOT_FILES`(루트 기준 경로)에 걸리지 않아 내용 검사 대상이 된다.
     - rhwp의 `mydocs/**`(23,775개 .md)와 이 파일들에는 금지 토큰(외부 상용 제품 브랜드, 도구명, 오케스트레이션 용어, 금지 파일명 참조)이 거의 확실히 들어 있다. 예: `mydocs/tech/benchmark_vs_alternatives.md`의 경쟁 제품 비교.
  2. `.claude/`·`.cursor` 등은 디렉터리 이름 규칙으로 건너뛰지만 `.continue/`, `.kiro/`, `.roo/` 등은 건너뛰지 않는다.
  3. 수천 개 `.js`/`.mjs`에 `node --check`가 실행되어 수 분이 걸린다. ESM 문법 파일은 Node 버전에 따라 실패할 수 있다 `[추정]`.
  4. `ui/hwp-studio/` 빌드 산출물(ESM 번들)도 같은 문제를 일으킨다.

### 5.2 최소 정책 변경 (PD-076 제안)

1. **루트 기준 제외 경로**를 추가한다. `IGNORED_DIRS`(이름 기준)가 아니라 별도 집합이다:
   `IGNORED_ROOT_PATHS = new Set(["third_party/rhwp", "crates/easyoffice-app/ui/hwp-studio", "apps/hwp-studio/node_modules"])`. 순회 중 `relPath`가 이 경로이거나 그 하위이면 건너뛴다.
   - 서드파티 내용은 EasyOffice가 작성하거나 수정하는 파일이 아니고, 다음 2·3항으로 무결성을 따로 고정하기 때문이다.
2. **새 검사(fail-closed)**:
   - `.gitmodules`에는 허용 목록 항목만 있을 것: `third_party/rhwp` ↔ `https://github.com/edwardkim/rhwp.git`.
   - `git ls-files -s third_party/rhwp`의 모드가 `160000`이고 커밋이 `config/rhwp-upstream.json.commit`과 같을 것.
   - 서브모듈 작업 트리가 있으면 `git -C third_party/rhwp status --porcelain`이 비어 있을 것(업스트림 무수정. 참조 앱 `upstream:verify`와 동일).
   - `vendor/rhwp-core/*`의 크기·SHA-256이 `PROVENANCE.json`과 같고 다른 파일이 없을 것.
   - `ui/hwp-studio/`가 git에 추적되지 않을 것(빌드 산출물 커밋 방지).
3. **EasyOffice 소유 파일 규칙은 그대로 적용**한다. `apps/hwp-studio/src/**`(오버레이 TS), `vendor/rhwp-core/rhwp.js`·`rhwp.d.ts`, `config/*.json`, `THIRD_PARTY_NOTICES.md`가 대상이다.
   - `TEXT_EXTENSIONS`에 `.ts`를 추가해 오버레이 TS도 금지 토큰 검사를 받게 한다.
   - `rhwp.js`(ESM)의 `node --check`는 `vendor/rhwp-core/package.json`의 `"type": "module"`로 해결된다(Node가 가장 가까운 package.json의 type을 따름).
   - 업스트림 문자열을 그대로 복사한 오버라이드 원본에 금지 브랜드 토큰이 있으면 오버라이드에서 문구를 바꾼다.
4. **문서 정합성**: `README*`, `docs/architecture.md`의 수치 계약(20종 포맷, 262종 함수, 140종 리본, 15종 IPC)은 변하지 않는다. 데스크톱 명령 허용 목록(`DESKTOP_SHELL_PERMISSIONS`)과 `REQUIRED_TRACKED_CONTRACTS`에는 `hwp-host.js`, `config/rhwp-upstream.json`, `scripts/verify-rhwp-upstream.mjs`를 추가한다.
5. **개발자 작업 트리 주의**: rhwp 체크아웃에는 `.mcp.json`(`rhwp mcp-serve` 실행)과 여러 AI 도구 규칙 파일이 있다. 이 저장소에서 AI 도구를 쓰는 개발자는 해당 도구가 서브모듈의 `.mcp.json`이나 규칙 파일을 자동으로 읽지 않는지 확인해야 한다. sparse 체크아웃 목록에서 이 파일들을 빼면 **디스크에도 생기지 않으므로** 권장 목록(§3.2)에서 제외되어 있다.

---

## 6. 성능

### 6.1 rhwp의 빠른 곳과 느린 곳

| 구간 | 관찰 | 근거 |
|---|---|---|
| WASM 로드 | `rhwp_bg.wasm` 11,803,733 B. WebView2는 코드 캐시가 있어 두 번째 실행부터 빠름. 첫 컴파일은 수백 ms 수준 `[추정]` | 참조 앱 `vendor/rhwp-core` |
| 파싱 | rhwp 자체 벤치(`mydocs/tech/benchmark_vs_alternatives.md`)는 **debug 빌드** CLI로 export-text 중앙값 2234 ms, info 584 ms. 릴리스 WASM은 훨씬 빠를 것 `[추정]`. 세션 재사용 시 호출당 82.1 ms → 0.68 ms(×120.6, `track_f_scale_perf.md` R52), 즉 "한 번 파싱 후 메모리 문서 유지"가 핵심 | rhwp 문서 |
| 렌더 | 쪽 단위 Canvas2D, 가상 스크롤, LRU 표면, 유휴 시간 프리페치 스케줄러(`page-render-scheduler.ts`, `canvas-pool.ts`, `page-surface-lru.ts`, `render-surface-budget.ts`). 큰 문서에서도 보이는 쪽만 그림 | rhwp-studio |
| 텍스트 측정 | WASM과 네이티브가 같은 `EmbeddedTextMeasurer`(내장 글꼴 메트릭)를 사용(`src/renderer/layout/text_measurement.rs:810`). 글자마다 JS `measureText`를 왕복하지 않음 | rhwp 소스 |
| **느린 곳 1: 표 셀 입력** | 참조 앱 #85: Windows에서 표 입력 지연 7초 보고(Mac 미재현). 해결 방향은 "변경 없는 정적 레이어 재사용"(`issue-85-table-input-rendering-1pager.md`). rhwp는 셀 편집 페이지 나누기 지연 API(`insertTextInCellDeferredPagination`)를 가지고 있음 | 참조 앱 문서 |
| **느린 곳 2: 저장·자동 저장** | `exportHwp`는 문서 전체 직렬화 + 압축. 자동 저장을 자주 하면 입력 지연이 생김 → 유휴 시점 + 5초 이상 디바운스(§2.7) | 설계 |
| **느린 곳 3: 실행 취소 스냅샷** | 큰 작업마다 Rust `saveSnapshot`(문서 복제, `MAX_SNAPSHOTS` 상한) → 큰 문서에서 WASM 메모리 증가. 32비트 WASM 메모리 상한 4 GiB, 탭당 iframe 1개면 문서당 한 인스턴스 → 동시 탭 4개 상한 | rhwp `wasm_api.rs:7073~` |
| 바이트 전송 | iframe↔부모는 `ArrayBuffer` 이동으로 복사 없음. 셸 저장은 Base64 경로 → 원시 바이트 IPC로 개선(§2.5) | 설계 |

**참조 앱의 "WASM 미러 대 네이티브 권위" 메모**: `docs/DEVELOPMENT.md:70`은 "큰 문서에서는 현재 WASM mirror를 거치는 구간이 있어 native-authoritative 구조로 더 개선해야 합니다"라고 적는다.

- 참조 앱의 실제 구조는 **WASM이 문서의 권위**다. 네이티브 세션은 메타데이터만 가진다(`core: None`). 저장과 PDF 시점에만 WASM이 내보낸 바이트를 Rust가 다시 파싱한다. 네이티브 편집 명령(`mutate_document`, `render_page_svg`, SVG 캐시)은 등록만 되어 있고 쓰이지 않는다.
- EasyOffice도 **WASM 권위 모델로 시작**한다. 이유는 `--serve`와 셸이 같은 경로를 쓰고, MSRV가 분리되며, 캐럿·IME가 업스트림 그대로이기 때문이다.
- 네이티브 권위(문서는 Rust 프로세스, WebView는 SVG 또는 쪽 비트맵만)는 다음 조건이 충족될 때 재검토한다:
  1. 64 MiB 넘는 문서를 지원해야 하거나
  2. 표 입력 지연이 오버레이 최적화로 해결되지 않을 때

### 6.2 EasyOffice 엔진의 비트·저수준 최적화 계획

#### 6.2.1 `crc32_slicing_by_8` 0.85×의 원인 (`crates/easyoffice-formats/src/zip_codec.rs:6~85`, `crates/easyoffice-app/src/benchmark.rs:495~531`)

1. **`TABLES`가 `static`이 아니라 연관 `const`다.**
   - `Self::TABLES[7][idx]`처럼 런타임 인덱스로 const 배열을 읽으면, 최적화가 없는 debug 빌드에서는 사용 지점마다 8 KiB(`[[u32;256];8]`) 임시값을 스택에 복사(memcpy)한 뒤 인덱싱할 수 있다 `[추정]`(MIR 상수 승격은 `&CONST`에만 보장됨).
   - slicing-by-8은 8바이트당 테이블 참조 8회, 기준선은 1바이트당 1회로 **참조 횟수가 같다**. 그래서 이 복사 비용이 양쪽을 지배하고, slicing의 장점(의존 사슬 단축)이 사라진다.
   - release에서는 LLVM이 전역 상수로 접기 때문에 이 효과가 대부분 없어진다.
2. **debug 빌드의 경계 검사·오버플로 검사.**
   - `bytes[offset+k]` 8회, `TABLES[t][..]` 16회 경계 검사, `offset + 8 <= len` 산술 검사가 붙는다. 기준선의 `for &b in bytes`는 반복자라 경계 검사가 없다.
   - 4바이트를 하나씩 인덱싱해 `u32::from_le_bytes([..])`를 만드는 것도 debug에서는 8회 개별 로드가 된다.
3. **측정 조건.**
   - CI는 `cargo run -p easyoffice-app -- --self-test`를 **debug로** 실행한다.
   - 64 KiB × 16회를 `as_micros()`로 재므로 release에서는 표본이 수백 µs라 해상도는 충분하다. 반면 debug 수치는 성능 지표로 의미가 없다.

계획:

| 항목 | 내용 |
|---|---|
| 테이블 | `static CRC32_TABLES: [[u32; 256]; 8]`(또는 16)로 바꾸고 `IeeeCrc32::TABLES`는 호환 별칭으로 둠 |
| 루프 | `bytes.chunks_exact(8)` + `u64::from_le_bytes(chunk.try_into().unwrap())` 한 번 로드 → 경계 검사 제거. 나머지는 `remainder()` |
| slicing-by-16 | 테이블 16 KiB, L1 적중. 64 KiB 이상 입력에서 추가 이득 기대 `[추정]` |
| 하드웨어 가속 | `std::arch`만 사용(외부 의존 0 원칙 유지):<br>1) aarch64: `crc32` 확장 `__crc32d`/`__crc32b`를 `is_aarch64_feature_detected!("crc")`로 분기. IEEE 다항식과 일치하며 Rust 1.80부터 안정화되어 MSRV 1.85 안에 들어옴 `[확인 필요]`<br>2) x86_64: SSE4.2 `crc32` 명령은 Castagnoli 다항식이라 **ZIP CRC에 쓸 수 없음**. PCLMULQDQ 폴딩(`_mm_clmulepi64_si128`, `is_x86_feature_detected!("pclmulqdq")`)으로 IEEE CRC 구현. 16바이트 4줄 폴딩 + Barrett 축약 |
| 벤치 하니스 | 1) `cfg!(debug_assertions)`이면 보고서에 `"buildProfile":"debug"`를 넣고 속도 비율 판정 비활성<br>2) CI 성능 게이트는 `cargo run --release -p easyoffice-app -- --self-test`로 분리<br>3) A/B 교대 순서를 라운드마다 뒤집어(ABBA) 캐시·클럭 편향 제거<br>4) 입력 크기 4 KiB / 64 KiB / 4 MiB 세 구간 |
| 동등성 | 기존 `crc_eq` 검증 + 무작위 길이(0~4097) 속성 테스트로 경계 처리 고정 |

#### 6.2.2 `sheet_dag_bitset_eval` 0.92×의 원인 (`crates/easyoffice-sheet/src/{lib.rs:248~260, coercion.rs:355~520, types.rs:506~}`)

워크로드: 50행 × 4열 = 200셀. D열이 `=SUM(A1:C{r})`이므로 범위 크기가 행마다 커진다.

"최적화" 경로는 기준선보다 **일을 더 많이 한다**:

1. **2단계 순회**: `recalculate_all`은 `pre_evaluate_dag_iterative`(위상 사전 평가) 후 다시 모든 셀에 대해 `eval_coord_ctx`를 실행한다. 기준선은 재귀 메모이제이션 한 번이다.
2. **사전 평가의 할당**:
   - 스택 원소가 `(Option<String>, u16, PackedCellCoordinate, bool)`이어서 의존성을 하나 넣을 때마다 `sheet_opt.clone()`(시트 이름 `String` 복제)이 생긴다.
   - `state: HashMap<PackedWorkbookCoord, u8>`는 기본 SipHash다.
   - 셀마다 `ctx.memo.contains_key` + `state.contains_key` + `state.get`으로 해시 조회가 세 번 일어난다.
3. **범위 의존성 열거가 O(범위)**: `push_range_formula_deps`는 `range.cell_count() <= compiled_asts.len()`이면 범위의 모든 좌표를 돌며 `compiled_asts.get`(SipHash)을 호출한다. D열 누적 범위 때문에 총 Σ3r ≈ 3,825회 조회가 생긴다(r=1..50). 기준선 재귀는 메모 적중 시 이 비용이 없다.
4. **값 복제**: 메모 적중마다 `EvaluatedCellValue` `clone()`(Text이면 `String` 복제). 텍스트 결과에 `chars().count()`(O(len)) 두 번, `sanitize_evaluated_value`.
5. **비트셋 이점이 이 워크로드에서 드러나지 않음**: `VisitingTracker`의 64×16 비트셋은 방문 집합만 줄이고 지배 비용(해시·할당)에는 영향이 없다. 200셀은 해시 오버헤드가 상대적으로 큰 크기다.
6. debug 빌드에서는 `HashMap`의 제네릭 코드가 인라인되지 않아 해시 비용이 더 커진다.

계획:

| 항목 | 내용 |
|---|---|
| 셀 슬롯 인덱스 | `set_cell` 시 수식 셀에 연속 `u32` 슬롯 번호 부여 → `memo: Vec<Option<Value>>`, 상태는 **2비트/셀 비트셋**(`Vec<u64>`: 방문 중 / 완료). 해시 조회를 배열 인덱싱으로 바꿈 |
| 해셔 | 남는 `HashMap<PackedWorkbookCoord,_>`에는 std만 쓴 곱셈 해셔(`BuildHasherDefault<U64MulHasher>`, `x * 0x9E3779B97F4A7C15` 후 회전)를 사용. 키가 이미 패킹된 u64라 분포가 좋음 |
| 시트 참조 | 스택 원소의 `Option<String>`을 `u16` 시트 ID로 바꾸고 워크북의 ID→시트 표로 해석 → 할당 0 |
| 범위 의존성 | 열별로 정렬된 수식 셀 좌표 `Vec<u32>`(행)를 유지하고 범위 `[min_r, max_r]`를 이진 탐색으로 잘라 **수식 셀만** 방문. 값 셀은 의존성 그래프에 넣지 않음 |
| 위상 순서 캐시 | 위상 순서를 `set_cell` 시점에 증분 갱신(더티 전파)하고 `recalculate_all`은 캐시된 순서로 한 번만 평가 → 2단계 순회 제거 |
| 값 표현 | `EvaluatedCellValue::Text(Rc<str>)`(또는 `Arc<str>`)로 메모 복제를 O(1)로. `MAX_CELL_TEXT_CHARS` 검사는 생성 시 한 번 |
| SUM 범위 | 숫자 열에 대해 `dense_numbers`(현재 16열 한정) 확장 + 누적 범위 반복 시 열 접두합(선택). 결과 동등성은 IEEE 754 덧셈 순서 때문에 달라질 수 있으므로 **같은 순서의 합만 허용**(PD-070 수치 정합성) |
| 벤치 | 200셀 외에 10,000행 × 4열, 시트 간 참조 워크로드를 추가하고 release 기준으로 판정 |

#### 6.2.3 그 밖의 저수준 후보 (우선순위 낮음)

- `deflate.rs`: 이미 64비트 리필 `BitReader`와 9비트 LUT가 있다. 동적 허프만 2단계 LUT(10+5비트)와 `extend_from_within` 복사를 8/16바이트 단위로 겹쳐 쓰는 복사(거리가 8 이상일 때)를 검토할 수 있다.
- `utf16_swar.rs`: SWAR에서 `std::arch` SSE2/NEON(x86_64 기본 SSE2, aarch64 기본 NEON. 런타임 검출 불필요)으로 ASCII 16바이트 일괄 처리.
- `ole2_codec.rs`/`hwp5_codec.rs`: 레코드 헤더 파싱을 `u32::from_le_bytes(slice.try_into())` 한 번 로드 + 비트 마스크로 통일(경계 검사 제거).
- 모든 최적화는 기존 "기준선 함수 + 동등성 검증 + p50" 하니스에 쌍으로 추가하고, release 프로필에서만 판정한다.

---

## 7. 단계별 구현 계획 (이슈 단위)

각 단계는 독립적으로 병합 가능하고 병합 시점에 동작하는 결과물을 낸다. 1→2→3→4→5는 순차이고, 6·7·8은 병행 가능하다.

### 작업 1. rhwp 서브모듈 고정·위생 가드·라이선스 기반 (PD-076)

- **범위**
  - `.gitmodules`(`shallow = true`), `third_party/rhwp` @ `1a76570e…`(v0.8.7)
  - `config/rhwp-upstream.json`
  - `scripts/verify-rhwp-upstream.mjs`(원격 URL / gitlink 커밋 / clean / 버전 3종 / 벤더 해시)
  - `scripts/checkout-rhwp.mjs`(sparse)
  - `check-repo-hygiene.mjs` 변경(§5.2)
  - `THIRD_PARTY_NOTICES.md` 초안
  - `docs/policy-decisions.md` PD-076
- **의존**: 없음
- **위험**: sparse 목록 누락으로 이후 빌드 실패(2단계에서 확정). 위생 가드 변경이 기존 금지 규칙을 약화시키는지 검토 필요.
- **수용 기준**
  - 서브모듈 미체크아웃 상태와 체크아웃 상태 모두에서 `node scripts/check-repo-hygiene.mjs` 통과.
  - gitlink를 다른 커밋으로 바꾸면 실패한다(테스트).
  - `.gitmodules`에 다른 URL을 넣으면 실패한다.
  - 서브모듈 작업 트리를 수정하면 `verify-rhwp-upstream` 실패.
  - EasyOffice 루트에 금지된 로컬 전용 파일(예: 에이전트 규칙 파일)을 추적시키면 여전히 실패.
  - 런타임 변화 없음.

### 작업 2. `vendor/rhwp-core` WASM 벤더링 + `apps/hwp-studio` 오버레이 빌드

- **범위**
  - `vendor/rhwp-core/*` + `PROVENANCE.json`(rustToolchain 1.93.1, wasmPack 0.14.0, 커밋, 파일별 크기·SHA-256)
  - `apps/hwp-studio/`: `package.json`, `package-lock.json`, `vite.config.ts`(참조 앱 alias 순서), `config/rhwp-studio-overrides.json`
  - 오버레이 `main.ts`(임베드 전용 진입점), `font-loader` fork(`FontFace` 전용), `print-pages` 스타일 외부화, `file` 명령 부모 위임, canvaskit·PWA·hwpctrl·외부 웹글꼴 제거
  - `assets/fonts/`(OFL만) + `FONTS.md`
  - `scripts/build-hwp-studio.mjs`, `scripts/check-hwp-studio-bundle.mjs`
  - `rhwp-wasm-rebuild.yml`(수동)
- **의존**: 1
- **위험**
  - 업스트림 `main.ts`의 초기화 순서가 오버레이 진입점에서 빠진 부분 때문에 깨질 수 있다(참조 앱 `main.ts` 참고).
  - TS 7 / Vite 8 툴체인 의존성의 공급망 위험 → lockfile 고정, `--ignore-scripts`.
  - WASM 재현성 미확보(§8 Q10).
- **수용 기준**
  - Windows CI에서 산출물 `ui/hwp-studio/index.html` 생성.
  - 번들 정적 검사 통과(외부 URL / eval / `__TAURI` 0건).
  - 브라우저에서 단독으로 열어 빈 문서 생성 → 입력 → 선택 범위만 글자 크기 변경 → `exportHwp` 바이트를 rhwp로 재로드하면 같은 문자 모양이 나온다(vitest 또는 node 하니스).
  - 번들 총량 목표 ≤ 20 MB(wasm 11.8 MB + 글꼴 + JS) `[목표치]`.

### 작업 3. HWP 패널 iframe 임베딩 (셸·`--serve`) — 새 문서 편집

- **범위**
  - `ui/hwp-host.js`(MessagePort RPC 클라이언트, 지연 생성, 오류·재연결)
  - `index.html` `#pane-hwp`, `applyModuleAccent` 분기
  - Tauri CSP 갱신(§4.1)과 위생 가드 정확값 갱신
  - `http_server.rs` 바이트 본문 + `hwp-studio` cargo 기능 + 경로별 CSP
  - `self_test.rs` 자산 무결성 목록
  - `EasyOffice.exe --smoke`에 HWP 왕복 추가
  - `scripts/test-webui-hwp-host.mjs`
  - HWP 활성 시 `#doc-style-bar` 숨김
- **의존**: 2
- **위험**: WebView2에서 `.wasm` MIME과 `instantiateStreaming` 동작, iframe 포커스와 IME(한글 조합) 동작, 자산 미포함 빌드의 fail-closed 표시.
- **수용 기준**
  - 설치본 `EasyOffice.exe`에서 HWP 모듈 → 빈 문서에 한글 입력.
  - 선택한 글자만 크기·굵게 적용(문단·편집기 전체에 적용되지 않음).
  - 탭·표 도구가 rhwp 도구모음에서 정상 동작.
  - `--serve`(기능 on)에서 동일하게 동작.
  - `/` CSP는 여전히 `default-src 'self'`.
  - 자산 없는 빌드는 명확한 안내를 표시.
  - capabilities 변경 없음.

### 작업 4. 열기·저장·수정 상태·자동 저장 통합 (#111·#104 연동)

- **범위**
  - `file-open.js` HWP 분기(`detect_format` → `loadFile`, 비밀번호 대화상자, 상한 64 MiB)
  - `Ctrl+S`/저장 버튼 → `exportHwp`/`exportHwpx` → `saveFile`(셸) 또는 Blob(`--serve`), `notifySaved`
  - 저장 전 재파싱 검증
  - `save_document_file` 원시 바이트 본문 전환(또는 `save_document_bytes`)
  - dirty 판정·`beforeunload`를 rhwp 이벤트로 전환
  - IndexedDB HWP 자동 저장·복원(복원 전 `FormatDetector`)
  - 변환기 "HWP → DOCX/PDF"에 rhwp가 내보낸 바이트를 원본으로 사용
- **의존**: 3, #111 병합
- **위험**: HWPX 원본 저장(업스트림이 안전하지 않다고 판단 → 기본은 HWP 저장, Q3). 배포용·암호 문서 정책(Q5). 큰 문서 자동 저장 비용.
- **수용 기준**
  - `samples/`에서 고른 HWP 20개, HWPX 20개(sparse에서 제외되므로 CI용 픽스처는 EasyOffice `fixtures/`에 라이선스 확인 후 소수만 복사)를 열기 → 편집 → 저장 → 재열기.
  - 문자 모양·문단 모양·표 위치가 보존된다(rhwp `exportHwpVerify` 일치).
  - 저장 취소·실패 시 기준선이 바뀌지 않는다(PD-073/074 규칙).
  - 새로고침 후 자동 저장본이 복원된다.
  - 10 MiB 넘는 HWP 열기 성공.

### 작업 5. 셸 UX 정합: 리본·단축키·탭·테마·인쇄

- **범위**
  - 셸 단축키 포워딩(부모가 키 목록 전달)
  - 편집 메뉴 실행 취소/다시 실행 → `automation.execute`
  - 리본의 HWP 의미 명령 매핑표(140종 수치 불변), 매핑 없는 명령 비활성
  - HWP 다중 탭(iframe 1개/탭, 상한 4)
  - 라벨 테마 CSS 오버라이드, 업스트림 중복 메뉴(`file:*`) 숨김
  - iframe 인쇄(`renderPageSvg` + `@media print`)
- **의존**: 4
- **위험**: 두 겹 도구모음의 UX 혼란 → 사용성 리뷰 필요. 다중 iframe 메모리 사용.
- **수용 기준**
  - 포커스가 iframe 안에 있어도 `Ctrl+S`/`Ctrl+O`/`Ctrl+K`가 셸 동작을 수행.
  - 리본 굵게·정렬이 HWP 선택 범위에 적용.
  - 탭 4개를 열었을 때 메모리 사용량을 측정해 기록.
  - 인쇄 미리보기 쪽 수가 `pageCount`와 같다.
  - PD-072 단축키 1:1 정합성 테스트 통과.

### 작업 6. 데스크톱 파일 수명주기: 파일 연결·단일 인스턴스·제자리 저장 (선택: 네이티브 어댑터)

> **[구현됨: PD-093(#133, #158)]** 단일 인스턴스(`tauri-plugin-single-instance`, 인수 → 기존 열기 큐), Rust 전용 경로·지문 표와 임시 파일 + `sync_all` + 이름 바꾸기 제자리 저장(외부 변경 시 확인 창), NSIS `fileAssociations`(창 모델이 여는 주요 9종)와 위생 가드 조건 변경을 넣었다. 6b(네이티브 어댑터)는 건너뛴다. 참조 앱에서는 패턴만 따랐고 코드는 복사하지 않았다. 한글 창의 `파일` 메뉴는 셸 명령 6종이며 임베드 포트의 `shell_command` 이벤트와 `setShellContext` RPC(capability `shell-context-v1`)로 셸과 연결한다.

- **범위**
  - `tauri-plugin-single-instance`(Rust만, WebView 권한 없음)
  - 기동 인자 / 두 번째 실행 → 열기 큐 → 부모 `openDocumentFromBytes`. Rust가 파일을 읽어 바이트만 전달하고 WebView는 경로를 받지 않으며, 표시용 파일 이름만 받는다.
  - `open_document_file`(Rust 대화상자)
  - 제자리 저장: 세션 ID ↔ 경로 매핑은 Rust에만 둔다. `source_fingerprint` 외부 변경 감지, 원자적 쓰기
  - NSIS `fileAssociations`(`hwp`, `hwpx`)와 위생 가드의 파일 연결 금지 조건(#111) 해제
  - 선택(6b): `crates/easyoffice-rhwp-adapter`(1.93.1)로 저장 전 네이티브 재파싱 검증 + 검색 가능 PDF 내보내기(`svgs_to_pdf_with_options`, `[patch.crates-io] svg2pdf`)
- **의존**: 4
- **위험**
  - 명령 수 증가에 따른 권한 표면 → 허용 목록 1:1 테스트.
  - 6b는 워크스페이스 Cargo.lock에 rhwp 의존성 트리(암호 크레이트 다수)가 들어오고 desktop MSRV가 1.93으로 바뀜(PD-074 수정 필요).
- **수용 기준**
  - 탐색기에서 `.hwp`를 더블클릭하면 이미 실행 중인 창의 HWP 탭으로 열림.
  - 다른 프로그램이 파일을 수정한 뒤 저장하면 덮어쓰기 확인을 표시.
  - 저장 도중 강제 종료해도 원본이 보존됨.
  - (6b) PDF 텍스트 선택·검색이 가능.

### 작업 7. DOC 모듈 문자 런 모델 (병행, rhwp와 무관)

- **범위**
  - Range API 기반 선택 런 분할기(`execCommand` 없음, `dataset` + CSSOM)
  - `applyStudioFontSize`·글꼴·색 선택 범위 적용
  - 탭 정지·들여쓰기 UI 정리
  - `save-payload.js` 런 스키마 확장(`sizeHalfPt`, `font`, `color`)
  - `rich_document.rs`/`rich_writers.rs`/`rich_read_ooxml.rs`(DOCX `w:sz`·`w:rFonts`·`w:color`), HWPX `charPr`, ODT·RTF
  - `structured_save_fidelity` 테스트 확장
- **의존**: #111(디코드 경로) 병합
- **위험**: contenteditable 정규화(빈 span, 중첩 병합). 1,000줄 규칙에 따른 모듈 분할(`doc-runs.js` 신설).
- **수용 기준**
  - 문단 일부만 14pt·굵게·빨강으로 바꿔 DOCX로 저장 → 다시 열기 → 같은 런이 복원되고, 나머지 글자는 바뀌지 않는다.
  - PD-073 Trade-off 1 해소를 기록.

### 작업 8. 엔진 저수준 성능 (병행)

- **범위**
  - §6.2.1: CRC `static` 테이블, `chunks_exact`, slicing-by-16, aarch64 CRC / x86 PCLMUL 분기
  - §6.2.2: 슬롯 인덱스 메모, 2비트 상태 비트셋, u16 시트 ID, 열별 수식 인덱스, 위상 순서 캐시, `Rc<str>`
  - 벤치 하니스의 debug 표기와 release 성능 게이트(`cargo run --release -p easyoffice-app -- --self-test`)
  - 대형 워크로드 추가
- **의존**: 없음
- **위험**: `unsafe`(`std::arch` 호출) 범위 → 함수 단위로 `#[target_feature]` + 런타임 검출 + 스칼라 폴백 + 동등성 퍼징.
- **수용 기준**
  - release 기준 `crc32` ≥ 3× 기준선(64 KiB, 스칼라 slicing만으로) `[목표치]`.
  - `sheet_dag_bitset_eval` ≥ 1.5×(200셀) 및 ≥ 5×(10k행) `[목표치]`.
  - 모든 동등성 검증이 참이고, `cargo test --workspace`가 통과.
  - debug self-test는 비율을 판정하지 않는다.

---

## 8. 사용자 결정이 필요한 질문

1. **macOS**: 결정됨(PD-094). macOS도 Tauri 셸로 옮겼고 Swift 앱은 삭제했다. 같은 `hwp-studio` 번들을 WKWebView에서 쓰며, 서명 없는 `.dmg`로 배포한다. 원래 선택지 (a) Swift 앱의 `WKWebView`, (b) rhwp 네이티브 바인딩, (d) Windows·`--serve`만 적용은 기각했다.
2. **DOCX 편집**: DOC 모듈 자체 런 모델 보강(작업 7)만 할지, "한글 편집기로 열기"(DOCX → HWPX 변환 후 rhwp, 손실 명시)를 추가 제공할지.
3. **HWPX 저장 정책**: 업스트림 판단대로 HWPX 원본도 HWP로만 저장할지, `exportHwpx`를 실험 옵션으로 열지.
4. **변환기 엔진**: HWP → DOCX/PDF 변환을 계속 EasyOffice `hwp5_codec`으로 할지, 네이티브 어댑터(6b) 도입 후 rhwp 파서로 바꿀지(정확도는 오르지만 툴체인 1.93.1이 필요).
5. **배포용·암호 문서**: 현재 EasyOffice는 fail-closed 차단이다. rhwp는 읽기와 비밀번호 열기를 지원한다. 열기를 허용할지, 저장 시 보호 해제 경고 정책을 어떻게 할지.
6. **rhwp 로드 실패 시**: 오류로 끝낼지, 사용자가 선택하면 기존 평문 디코드로 "텍스트만 가져오기"를 제공할지.
7. **다중 창**: ~~참조 앱처럼 문서별 창을 지원할지, 탭만 쓸지(현재 권장: 탭만).~~ **[확정: 문서마다 개별 창 — PD-088(#132). "탭만" 기본값은 대체됨. 탭 구현 시도는 폐기.]**
8. **HWP 파일 크기 상한**: 64 MiB 제안. 공문서 대용량 첨부 HWP를 고려해 조정할지.
9. **자동 저장 소유**: 업스트림 `src/recovery/`(IndexedDB) 재사용과 EasyOffice 부모 소유 중 어느 쪽으로 할지(권장: 부모 소유).
10. **WASM 출처 정책**: 참조 앱처럼 미리 빌드한 WASM을 저장소에 커밋할지(업데이트마다 약 12 MB 증가), 아니면 CI에서 매번 빌드할지(+10~20분 `[추정]`, 1.93.1 툴체인 상시 필요). 재현 빌드가 안 될 때 어느 산출물을 신뢰할지.
11. **CSP `style-src`**: 오버레이 fork로 `'unsafe-inline'` 없이 가는 것을 목표로 한다. fork 범위가 커지면 `'unsafe-inline'`을 허용할지.
12. **글꼴 번들 범위**: OFL만(Noto Sans/Serif KR, 나눔, Pretendard, 고운, D2Coding, Spoqa) 넣을지, GUST(Latin Modern Math, 수식용)도 넣을지. Cafe24·행복고흥(Happiness Sans) 같은 독자 무료 라이선스 글꼴은 제외할지(권장: 제외). 설치본 크기 증가 허용치는 얼마인지.
13. **참조 앱 코드 직접 복사 여부**: 패턴만 참고할지, 일부 파일(`font-catalog.ts`, `print-dialog.ts`, `verify-rhwp-upstream.mjs`)을 복사할지. 복사하면 참조 앱 MIT 고지를 추가해야 한다. 참조 앱의 `LICENSE` 저작권자 표기가 rhwp 저자(Edward Kim)로 되어 있어 표기 확인이 필요하다.

---

## 9. 라이선스 의무 정리

| 구성요소 | 라이선스 | 의무 |
|---|---|---|
| rhwp(코어 WASM, rhwp-studio 소스 번들) | MIT, "Copyright (c) 2025-2026 Edward Kim" | 저작권·허가 고지를 배포물에 포함(설치본 `THIRD_PARTY_NOTICES`, 앱 정보 화면) |
| WASM에 정적 링크된 Rust 의존성 | MIT / Apache-2.0 / BSD / Zlib / ISC / 0BSD(업스트림 `THIRD_PARTY_LICENSES.md`) | 고지 목록 포함. Apache-2.0 크레이트의 `NOTICE` 파일이 있으면 그대로 포함. 갱신 시 `cargo about`류 생성물로 재확인(오프라인 스크립트) |
| `vendor/svg2pdf`(6b에서만) | MIT/Apache-2.0, `NOTICE`, `RHWP_PATCHES.md` | NOTICE 유지·포함 |
| canvaskit-wasm | BSD-3-Clause | **번들에서 제외**하므로 의무 없음(포함하면 고지 필요) |
| 참조 앱(코드를 복사할 경우) | MIT | 고지 포함. 패턴만 참고하면 의무 없음 |
| 글꼴: Noto Sans/Serif KR, 나눔, Pretendard, 고운, D2Coding, Spoqa Han Sans, Source Han Serif | SIL OFL 1.1 | 1) 라이선스 전문 동봉<br>2) 글꼴 단독 판매 금지<br>3) 수정(서브셋·형식 변환 포함) 시 예약 글꼴 이름(RFN) 사용 제한 → **업스트림 배포 파일을 수정 없이** 사용 |
| Latin Modern Math | GUST Font License | 포함 시 라이선스 동봉 |
| Cafe24 Ssurround/Supermagic, Happiness Sans(`ttfs/redistributable/`) | 각 권리자 독자 무료 라이선스 | 권장: 제외 |
| HWP 제작사·MS·OS 글꼴 | 독점 | **절대 번들 금지**. 업스트림도 gitignore. 사용자 PC에 있으면 OS 글꼴로만 사용 |

---

## 부록 A. 확인한 주요 파일

- EasyOffice(`origin/main` 54edd83): `Cargo.toml`, `crates/easyoffice-desktop/{Cargo.toml,build.rs,tauri.conf.json,capabilities/main-window.json,src/main.rs}`, `crates/easyoffice-app/ui/{index.html,desktop-bridge.js,workspace-tools.js}`, `crates/easyoffice-app/src/{http_server.rs,benchmark.rs}`, `crates/easyoffice-formats/src/zip_codec.rs`, `crates/easyoffice-sheet/src/{lib.rs,coercion.rs,types.rs}`, `scripts/check-repo-hygiene.mjs`, `.github/workflows/{ci.yml,release.yml}`, `docs/{architecture.md,policy-decisions.md §44~49}`. #111: `9c0911c` `crates/easyoffice-app/ui/file-open.js`.
- rhwp(1a76570e): `Cargo.toml`, `rust-toolchain.toml`, `src/wasm_api.rs`, `src/renderer/layout/text_measurement.rs`, `rhwp-studio/{package.json,vite.config.ts,index.html}`, `rhwp-studio/src/{main.ts,core/wasm-bridge.ts,core/font-loader.ts,command/print-pages.ts,embed/*,hwpctl/*}`, `npm/{editor,hwpctrl-ocx}/package.json`, `.github/workflows/npm-publish.yml`, `LICENSE`, `THIRD_PARTY_LICENSES.md`, `ttfs/`, `assets/fonts/`, `mydocs/tech/{benchmark_vs_alternatives.md,agent_roadmap/track_f_scale_perf.md}`.
- the reference app(80888af): `.gitmodules`, `config/{rhwp-upstream.json,rhwp-studio-overrides.json}`, `scripts/verify-rhwp-upstream.mjs`, `docs/{DEVELOPMENT.md,architecture/UPSTREAM.md,operations/RHWP_UPDATE.md,operations/issue-85-table-input-rendering-1pager.md}`, `apps/studio-host/{vite.config.ts,the reference app-overrides.ts,package.json,index.html,vendor/rhwp-core/PROVENANCE.json,src/core/*,src/ui/print-dialog.ts}`, `apps/desktop/src-tauri/{Cargo.toml,tauri.conf.json,capabilities/default.json,src/{lib,state,commands,pdf_export,pending_open,windows,menu,font_catalog}.rs}`, `apps/desktop/rhwp-adapter`, `.github/workflows/{ci.yml,the reference app-desktop.yml}`, `assets/fonts/FONTS.md`, `LICENSE`.
