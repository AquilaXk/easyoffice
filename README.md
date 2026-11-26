<div align="center">

<img src="crates/easyoffice-desktop/icons/icon.png" alt="EasyOffice 아이콘" width="88" height="88" />

<h1>EasyOffice</h1>

<p>
  <strong>문서·시트·슬라이드·PDF·한글을 한 앱에서 여는 오프라인 올인원 오피스</strong>
  <br />
  macOS · Windows · Linux에서 실행하는 Rust 엔진 + Tauri v2 데스크톱 앱
  <br />
  <a href="#다운로드"><b>다운로드 ↓</b></a>
</p>

<img src="docs/images/easyoffice-home.png" alt="EasyOffice 홈 화면" width="840" />

<p>
  <a href="https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-windows-x64-setup.exe"><img src="docs/images/download-windows.svg" alt="Windows 다운로드" height="48" /></a>
  &nbsp;
  <a href="https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-macos-arm64.dmg"><img src="docs/images/download-macos.svg" alt="macOS 다운로드" height="48" /></a>
  &nbsp;
  <a href="https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-linux-x64.deb"><img src="docs/images/download-linux.svg" alt="Linux 다운로드" height="48" /></a>
  <br />
  <sub>macOS 버튼은 Apple Silicon(M1 이후)용입니다. Intel Mac은 <a href="https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-macos-x64.dmg">Intel Mac용 다운로드</a>를 받으세요. Linux 버튼은 x64 <code>.deb</code>이며 <code>.rpm</code>·<code>AppImage</code>·ARM64 빌드는 아래 표에 있습니다.</sub>
</p>

<p>
  <a href="https://github.com/AquilaXk/easyoffice/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/AquilaXk/easyoffice/ci.yml?branch=main&amp;label=CI&amp;style=flat-square" alt="CI" /></a>
  <a href="https://github.com/AquilaXk/easyoffice/releases/latest"><img src="https://img.shields.io/github/v/release/AquilaXk/easyoffice?label=Release&amp;style=flat-square" alt="Release" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache--2.0-blue?style=flat-square" alt="License Apache-2.0" /></a>
  <img src="https://img.shields.io/badge/Platform-macOS%20%7C%20Windows%20%7C%20Linux-555?style=flat-square" alt="Platforms" />
</p>

<p>
  <a href="#할-수-있는-일"><b>할 수 있는 일</b></a> ·
  <a href="#다운로드"><b>다운로드</b></a> ·
  <a href="#설치-유의사항"><b>설치 유의사항</b></a> ·
  <a href="#성능"><b>성능</b></a> ·
  <a href="#개발하기"><b>개발하기</b></a> ·
  <a href="#문서"><b>문서</b></a>
</p>

</div>

## 할 수 있는 일

EasyOffice는 Rust 엔진 위에 [Tauri](https://tauri.app) v2 셸과 WebUI를 올린 오프라인 데스크톱 앱입니다. 한글 문서 파싱과 렌더링은 [rhwp](https://github.com/edwardkim/rhwp)를 기반으로 동작하며 셸이 파일 열기·저장·PDF 내보내기·인쇄·파일 연결 같은 OS 통합을 맡습니다.

- **문서** — `DOCX`, `DOC`, `ODT`, `RTF`, `MD`, `TXT` 편집과 Pages(`.pages`) 읽기·실험적 쓰기
- **시트** — 내장 함수 **314종**, 피벗, 차트, 조건부 서식 (`XLSX`, `XLS`, `ODS`, `CSV`, `TSV`)
- **슬라이드** — 글상자·도형·그림·표·차트 개체 편집과 발표자 보기 (`PPTX`, `PPT`, `PPSX`, `ODP`)
- **PDF** — 보기, 주석, 쪽 편집, 원본 객체 보존 저장 (PDF ISO 32000)
- **한글** — `HWP` (5.0 OLE2), `HWPX` (KS X 6101), `HWT`, `HWTX` 열기와 편집
- **BASE** — 표·관계·수식과 데이터시트·보드·일정 보기를 갖춘 관계형 데이터베이스 (`.eobase`)
- **BOARD** — 도형·글·그림·차트·연결선을 놓는 무한 캔버스 (`.eoboard`)
- **코파일럿** — 앱에 모델을 내장하지 않고 사용자가 등록한 AI 모델 서버에 연결하는 채팅 패널
- **변환** — **20종** 포맷 사이의 변환과 일괄 변환

문서 처리는 모두 로컬에서 실행합니다. 네트워크를 쓰는 기능은 코파일럿 요청과 하루 1회 업데이트 확인뿐입니다. 포맷별 읽기·쓰기·보존 범위와 상한은 [docs/format-support.md](docs/format-support.md), 기능 명세는 [docs/product-spec.md](docs/product-spec.md)에 있습니다.

## 다운로드

최신 릴리스 설치 파일은 플랫폼과 CPU 아키텍처에 맞게 받으세요.

| 플랫폼 | 아키텍처 | 패키지 | 다운로드 |
| --- | --- | --- | --- |
| macOS | Apple Silicon (M1 이후) | `.dmg` | [EasyOffice-macos-arm64.dmg](https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-macos-arm64.dmg) |
| macOS | Intel | `.dmg` | [EasyOffice-macos-x64.dmg](https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-macos-x64.dmg) |
| Windows | x64 | 설치 프로그램 (`.exe`) | [EasyOffice-windows-x64-setup.exe](https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-windows-x64-setup.exe) |
| Linux | x64 (Ubuntu / Debian 권장) | `.deb` | [EasyOffice-linux-x64.deb](https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-linux-x64.deb) |
| Linux | x64 (Fedora / openSUSE) | `.rpm` | [EasyOffice-linux-x64.rpm](https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-linux-x64.rpm) |
| Linux | x64 (무설치) | `.AppImage` | [EasyOffice-linux-x64.AppImage](https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-linux-x64.AppImage) |
| Linux | arm64 (Ubuntu / Debian) | `.deb` | [EasyOffice-linux-arm64.deb](https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-linux-arm64.deb) |

Mac은 화면 왼쪽 위 Apple 메뉴의 `이 Mac에 관하여`에서 칩이 Apple M으로 시작하면 Apple Silicon, Intel이면 Intel 파일을 받습니다. 내려받은 파일은 [SHA256SUMS.txt](https://github.com/AquilaXk/easyoffice/releases/latest/download/SHA256SUMS.txt)와 대조해 무결성을 확인할 수 있으며 전체 릴리스 목록은 [GitHub Releases](https://github.com/AquilaXk/easyoffice/releases)에 있습니다.

## 설치 유의사항

<details>
<summary><strong>macOS (Gatekeeper 허용 및 격리 속성 제거)</strong></summary>

macOS 빌드는 아직 Apple Developer ID로 서명·공증하지 않아 처음 실행할 때 Gatekeeper가 막습니다. 한 번만 허용하면 됩니다.

1. Finder에서 `/Applications`의 EasyOffice를 우클릭(또는 Control-클릭)해 `열기`를 고르고 대화상자에서 다시 `열기`를 누릅니다.
2. macOS 15에서는 한 번 열기를 시도한 뒤 `시스템 설정 > 개인정보 보호 및 보안`에서 `그래도 열기`를 누릅니다.
3. 앱이 손상되었다고 표시되면 격리 속성을 한 번 제거합니다.

```sh
xattr -dr com.apple.quarantine /Applications/EasyOffice.app
```

</details>

<details>
<summary><strong>Windows (SmartScreen 경고 및 해시 검증)</strong></summary>

Windows 빌드는 아직 코드 서명되지 않아 SmartScreen에서 "Windows의 PC 보호" 경고가 뜰 수 있습니다. `추가 정보`를 누른 뒤 `실행`을 선택합니다. 브라우저가 다운로드를 막으면 다운로드 목록에서 먼저 `유지`를 선택합니다.

해시를 직접 확인하려면 아래 명령 결과를 [SHA256SUMS.txt](https://github.com/AquilaXk/easyoffice/releases/latest/download/SHA256SUMS.txt)와 비교합니다.

```powershell
Get-FileHash .\EasyOffice-windows-x64-setup.exe -Algorithm SHA256
```

</details>

<details>
<summary><strong>Linux (배포판 패키지 설치 및 AppImage 주의사항)</strong></summary>

Linux에서는 배포판에 맞는 네이티브 패키지를 우선 사용해 주세요. 한글 입력기와 WebKitGTK 런타임은 배포판이 제공하는 것을 쓸 때 가장 안정적입니다. arm64 Linux는 Ubuntu/Debian 계열 `.deb`만 제공합니다.

```sh
sudo apt install ./EasyOffice-linux-x64.deb   # Ubuntu, Debian
sudo dnf install ./EasyOffice-linux-x64.rpm   # Fedora
```

AppImage는 설치가 필요 없지만 `chmod +x EasyOffice-linux-x64.AppImage` 뒤에 실행하며 `libfuse2` 패키지가 있어야 합니다. 일부 Wayland·IME 환경에서는 한영 전환이나 창 표시가 불안정할 수 있어 네이티브 패키지를 쓸 수 없을 때만 권장합니다. 패키지는 glibc 2.35 이상과 WebKitGTK 4.1이 필요합니다(Ubuntu 22.04 이상, Debian 12 이상).

</details>

## 성능

Apple M4 Pro release 빌드 실측 수치입니다. `HWP → PDF 내보내기`는 `scripts/bench-run.mjs --suite hwp`(5회 중앙값, Rust `1.98.1`), 엔진 경로는 `cargo run --release -p easyoffice-app -- --self-test`(3회 중앙값, Rust `1.93.1`) 기준입니다. 전체 측정 조건과 회귀 비교는 [성능 문서](docs/performance.md)에 있습니다.

| 항목 | 측정 경로 | EasyOffice 실측 | 비교 기준 / 비고 |
| --- | --- | ---: | --- |
| HWP → PDF 내보내기 (`biz_plan.hwp`, 6쪽) | `bench-run.mjs` (§4.1 / §2.1) | **67 ms** (최적화 전 `80 ms`) | 참조 앱 `710 ms` 대비 **8.8~10.7배** (§2.1 기준) |
| HWP → PDF 내보내기 (`kps-ai.hwp`, 77쪽) | `bench-run.mjs` (§4.1 / §2.1) | **700 ms** (최적화 전 `798 ms`) | 참조 앱 `11,389 ms` 대비 **7.7~14배** (§2.1 벽시계 비) |
| HWP → PDF 내보내기 (합성 300쪽) | `bench-run.mjs` (§4.1 / §2.1) | **1,194 ms** (최적화 전 `1,462 ms`) | 참조 앱 `97,410 ms` 대비 **약 67~70배** (§2.1 기준) |
| 하드웨어 CRC-32 (`crc32_hardware_1mib`) | `--self-test` (§7.2) | **108 µs** | 기준 구현 `2,180 µs` 대비 **19.95배** |
| Deflate/Zlib 왕복 (`deflate_zlib_roundtrip`) | `--self-test` (§7.2) | **45 µs** | 기준 구현 `250 µs` 대비 **5.91배** |
| 시트 증분 재계산 (`sheet_recalc_10k_rows`, 5만 셀) | `--self-test` (§7.2) | **4,235 µs** | 기준 구현 `26,791 µs` 대비 **6.31배** |
| DOCX 왕복 (`docx_to_docx_1mb`) | `--self-test` (§7.3) | **10.6 ms** | 1 MB 문서 파싱·직렬화 |
| XLSX 10만 셀 왕복 (`xlsx_100k_cells_pack_unpack`) | `--self-test` (§7.3) | **44.6 ms** | 10만 셀 생성·압축·복원 |

## 개발하기

```sh
node scripts/checkout-rhwp.mjs && node scripts/build-hwp-studio.mjs   # 한글 편집기 번들 준비
cargo run -p easyoffice-desktop                                        # 데스크톱 셸 실행
cargo run -p easyoffice-app -- --self-test                             # 엔진 자체 시험과 벤치마크
```

Rust는 `.github/rust-toolchain-version`의 `1.93.1`, Node.js는 `.nvmrc`의 `22`를 씁니다. 저장소 검증 게이트는 아래와 같습니다.

```sh
node scripts/check-repo-hygiene.mjs
node --test scripts/test-*.mjs
cargo fmt --all -- --check
cargo clippy --workspace --exclude easyoffice-desktop --all-targets -- -D warnings
cargo test --workspace --exclude easyoffice-desktop
cargo run -p easyoffice-app -- --self-test
```

플랫폼별 패키지 빌드와 오픈소스 이식 규칙은 [개발 문서](docs/DEVELOPMENT.md)에서 확인할 수 있습니다.

## 문서

| 문서 | 다루는 내용 |
| --- | --- |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | 개발 환경, CI/CD 워크플로, 패키지 빌드, 오픈소스 이식 절차 |
| [docs/architecture.md](docs/architecture.md) | 크레이트 구성, 엔진·데스크톱 셸 경계, 데이터 흐름 |
| [docs/product-spec.md](docs/product-spec.md) | 앱별 기능 명세, 편집기 동작, 단축키 계약 |
| [docs/format-support.md](docs/format-support.md) | 20종 포맷 읽기·쓰기·보존 범위와 입력 상한 |
| [docs/performance.md](docs/performance.md) | 벤치마크 측정 방법과 구간별 최적화 기록 |
| [docs/design-system.md](docs/design-system.md) | WebUI 디자인 토큰과 창 크롬 규격 |
| [docs/policy-decisions.md](docs/policy-decisions.md) | 설계 결정과 기본값 변경 이력 (`PD-*`) |

## 문제 신고

버그는 [이슈 양식](https://github.com/AquilaXk/easyoffice/issues/new/choose)으로 신고해 주세요. 기능 요청과 PR은 받지 않습니다. 자세한 내용은 [CONTRIBUTING.md](CONTRIBUTING.md), 보안 취약점 제보는 [SECURITY.md](SECURITY.md)를 참고해 주세요.

## 라이선스 및 출처

EasyOffice는 [rhwp](https://github.com/edwardkim/rhwp)(MIT)를 기반으로 합니다. HWP 엔진을 공개해 주신 개발자분께 감사드립니다. 가져다 쓴 코드의 출처와 라이선스 전문은 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)에 있습니다.

이 프로젝트 자체 코드는 [Apache-2.0](LICENSE) 라이선스로 배포합니다.
