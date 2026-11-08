# EasyOffice

[English](README.md) | [한국어](README.ko.md)

EasyOffice (`v0.1.0`) is a 100% standalone, serverless, offline-capable all-in-one productivity and office suite for macOS, Windows and Linux: a Rust engine and an offline WebUI hosted in one Tauri v2 desktop shell, following the rhwp-studio window chrome of the reference app with light and dark themes. It unifies **Document**, **Sheet**, **Slide**, **Korean Office (HWP / HWPX)**, and **PDF Studio** into a single lightweight workspace with bit-level container codecs, deterministic formula evaluation, offline smart document assistance, and fail-closed security guards.

---

## Key Highlights

- **One code base on macOS, Windows and Linux**: a Rust engine (`crates/`), a WebUI (`crates/easyoffice-app/ui/`) and a Tauri v2 desktop shell (`crates/easyoffice-desktop`, WKWebView on macOS, WebView2 on Windows, WebKitGTK on Linux). Every document opens in its own window; a start window offers new documents, recent files and open. Single instance, in-place atomic save with external-change detection, and file associations are handled in Rust; the WebView never receives a file path.
- **HWP/HWPX editor**: the rhwp engine (MIT, pinned upstream submodule) runs as a same-origin iframe editor with HWP/HWPX open, save-before-verify, dirty state and IndexedDB autosave. The shell drives its File menu, theme and status bar.
- **20 read/write formats plus Pages (read-only)**:
  - ZIP containers (`ZipArchiveCodec` with `ZipSecurityGuard`): `OoxmlCodec` (`.docx`, `.xlsx`, `.pptx`, `.ppsx`), `OdfCodec` (`.odt`, `.ods`, `.odp`), `HwpxCodec` (`.hwpx`, `.hwtx`).
  - OLE2 compound files (`Ole2ContainerCodec`, `Ole2SecurityGuard`): `Hwp5Codec` (`.hwp`, `.hwt`), `LegacyOle2Codec` writing real MS-DOC/MS-PPT structures and BIFF8 `.xls`.
  - `PdfSerializer` (ISO 32000), `CsvCodec` (RFC 4180 `.csv`/`.tsv`), `CommonMarkCodec` (`.md`), `RtfCodec` (`.rtf`), UTF-8 `.txt`.
  - Apple Pages (`.pages`, IWA + Snappy variant + protobuf, own decoder) opens as a source for viewing and conversion.
  - `RichDocument` keeps block order and character runs across DOCX, HWPX, ODT, RTF, Markdown, TXT and PDF.
- **Bit-level engine work**: hardware-accelerated and slicing-by-8 `IeeeCrc32`, DEFLATE, SWAR UTF-16/XML/CSV scanning, 64-bit `PackedCellCoordinate` (row in the upper 32 bits, column in the lower 32), `SectorBitset`, checked arithmetic everywhere and no `unsafe`. `--self-test` runs the benchmarks and checks the results match.
- **Spreadsheet engine** (`SheetEngine`, `WorkbookEngine`): **122 built-in functions** (math, statistics, logic, lookup, text, finance such as `PMT`/`NPV`/`IRR`, date/time, information), a dependency DAG with slot memoisation, pivots (`compute_pivot`), charts, conditional formatting and data validation; cell styles and column widths round-trip through XLSX/ODS.
- **Offline smart tools and security**: `UniversalFormatConverter` batch conversion with output verification by `FormatDetector`, `LocalSmartAssistant` (extractive summary, writing diagnostics, document diff), fail-closed `FormatDetector`/`XmlSanitizer`/`DocumentSecurityAuditor` (magic-byte spoofing, ZIP bombs, path traversal, XXE, PII scrubbing).
- **rhwp-studio UI tokens** (PD-098) with one SVG icon set, system/light/dark themes and WCAG 2.1 AA contrast checked by tests.

---

## Supported 20-Format Matrix

| Module | Supported Extensions | Standard / Container Specification | Core Codec & Engine Capabilities |
| :--- | :--- | :--- | :--- |
| **Document** | `DOCX`, `DOC`, `ODT`, `RTF`, `MD`, `TXT` | ECMA-376 WordprocessingML (ZIP+XML), OLE2 Word, ISO/IEC 26300 ODF Text, RTF 1.9, CommonMark, UTF-8 | `OOXMLContainerCodec`, `ODFContainerCodec`, `OLE2ContainerCodec`, `RTFCodec`, `CommonMarkCodec`, multi-section page layout, table formulas, tracked revisions, comments, mail merge |
| **HWP (Korean)** | `HWP`, `HWPX`, `HWT`, `HWTX` | HWP 5.0 Binary (OLE2 CFBF `FileHeader`/`DocInfo`/`BodyText`), KS X 6101 OWPML (ZIP+XML) | `HWP5BinaryCodec`, `HWP5RecordHeader`, `HWPXContainerCodec`, official form headers, approval tables, footnotes, template instantiation, HWP ↔ HWPX/DOCX/PDF conversion |
| **Apple Pages (read-only source)** | `PAGES` | Apple iWork IWA (ZIP + `Index/*.iwa`: Snappy-variant chunks + protobuf records); legacy `index.xml` and encrypted packages are refused with a clear message | `pages_iwa`, `pages_proto`, `rich_read_pages` (own decoder, no Apple schema files copied): text, bold/italic/underline/strike, alignment, headings, simple lists, simple tables; skipped elements are listed as warnings. Open and convert-from only (never a save/convert target), PD-087 |
| **Sheet** | `XLSX`, `XLS`, `ODS`, `CSV`, `TSV` | ECMA-376 SpreadsheetML, OLE2 BIFF8, ISO/IEC 26300 ODF Spreadsheet, RFC 4180 CSV/TSV | `PackedCellCoordinate`, 122 built-in functions, `PivotTableEngine`, `SheetChartBuilder`, `CSVStateMachineCodec`, multi-column sort, conditional format & validation rules |
| **Slide** | `PPTX`, `PPT`, `PPSX`, `ODP` | ECMA-376 PresentationML, OLE2 Presentation, ISO/IEC 26300 ODF Presentation | `PresentationDeckModel`, 16:9 / 4:3 / A4 stage, `SnapGuideCalculator` smart alignment guides, z-order layer arrangement, transitions, `PresenterSession` |
| **PDF Studio** | `PDF` | ISO 32000-1 / 32000-2 (`%PDF-1.x` / `%PDF-2.0`) | `PDFBinarySerializer`, `PDFStudioDocumentModel`, 5 annotation types, outline bookmarks, page split/merge/reorder/rotate, bidirectional Office ↔ PDF conversion |

---

## Architecture Overview

```text
EasyOffice/
├── Cargo.toml                        # Rust workspace manifest
├── crates/easyoffice-formats/        # 20-format registry, binary and container codecs, magic-byte guard
├── crates/easyoffice-sheet/          # Formula engine (122 built-in functions), pivot and DAG recalculation
├── crates/easyoffice-layout/         # Page layout engine
├── crates/easyoffice-app/            # IPC router, CLI (`--serve`, `--self-test`) and the WebUI in `ui/`
├── crates/easyoffice-desktop/        # Tauri v2 desktop shell for Windows (WebView2), macOS (WKWebView) and Linux (WebKitGTK)
├── apps/hwp-studio/                  # HWP editor overlay built on the pinned rhwp upstream
├── scripts/                          # Hygiene guard, bundle contract checks, WebUI regression tests
└── docs/                             # Architecture, design system, policy decisions (SSOT), product spec
```

---

## Requirements & Quick Start

### System Requirements
- macOS 11.0 (Big Sur) or later on Apple Silicon (`arm64`) and Intel (`x86_64`) for the universal `.dmg` (building the desktop shell needs Rust 1.90+, Node.js and Xcode command line tools)
- Windows 10/11 x64 with the WebView2 runtime for the installed Windows desktop app (building the desktop shell needs Rust 1.90+ with the MSVC toolchain)
- Linux x64 (AppImage, deb, rpm) or arm64 (deb) with WebKitGTK 4.1 and glibc 2.35 or newer, e.g. Ubuntu 22.04, Debian 12 or Fedora (building the desktop shell needs Rust 1.90+ and the WebKitGTK 4.1 development packages)
- Rust 1.85+ for the cross-platform engine crates and the developer-only local WebUI server

### Build, Verify & Run Locally

```bash
# 1. Verify repository hygiene & specification contracts
node scripts/check-repo-hygiene.mjs

# 2. Rust engine: format, lint, tests and self-test
cargo fmt --all -- --check
cargo clippy --workspace --exclude easyoffice-desktop --all-targets -- -D warnings
cargo test --workspace --exclude easyoffice-desktop
cargo run -p easyoffice-app -- --self-test

# 3. WebUI regression suite
node --test scripts/test-webui-*.mjs scripts/test-desktop-file-associations.mjs

# 4. Desktop shell (needs the generated editor bundle, see the HWP note above)
node scripts/checkout-rhwp.mjs && node scripts/build-hwp-studio.mjs
cargo run -p easyoffice-desktop
```

### Release Artifacts & First Launch

Every release carries the same fixed asset names, without a version in them, so the links below always point at the latest published release. `SHA256SUMS.txt` lists the SHA-256 checksum of every asset.

- [macOS universal (.dmg)](https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-macos-universal.dmg)
- [Windows x64 (installer)](https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-windows-x64-setup.exe)
- [Linux x64 (AppImage, portable)](https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-linux-x64.AppImage)
- [Linux x64 (.deb, Ubuntu / Debian)](https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-linux-x64.deb)
- [Linux x64 (.rpm, Fedora / openSUSE)](https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-linux-x64.rpm)
- [Linux arm64 (.deb, Ubuntu / Debian)](https://github.com/AquilaXk/easyoffice/releases/latest/download/EasyOffice-linux-arm64.deb)
- [SHA256SUMS.txt](https://github.com/AquilaXk/easyoffice/releases/latest/download/SHA256SUMS.txt)

All releases are listed on the [GitHub Releases](https://github.com/AquilaXk/easyoffice/releases) page.

**Updates.** The installed app checks for a newer release at most once a day (switch it off in the Help menu). When one exists, a card offers **Install and restart**: the app downloads the package from the release, verifies its signature against the public key built into the app, installs it and restarts. Nothing is installed while a window has unsaved changes, and a package whose signature does not verify is never installed. A `.deb` or `.rpm` update asks for administrator permission; an installation the updater does not cover is updated from the release page (the card has a button for it). The release also carries each package's `.sig` file and `latest.json`, which the updater reads.

#### macOS

macOS releases ship a universal disk image, `EasyOffice-macos-universal.dmg` (Apple silicon and Intel, macOS 11 or later). Open the image and drag **EasyOffice** to `/Applications`. The app is the same Tauri v2 desktop shell as on Windows (WKWebView): the WebUI is embedded, every command runs in-process on the Rust engine, and no network connection is involved. The standard menu bar and Command shortcuts (⌘C/⌘V/⌘X/⌘A/⌘Z/⌘S/⌘O/⌘W/⌘Q) work; Finder "Open With" and double-click open files in a new window. Quitting with ⌘Q goes through each window's save / discard / cancel prompt.

The release is **not signed or notarized with an Apple Developer ID**, so macOS Gatekeeper blocks it the first time ("EasyOffice cannot be opened because the developer cannot be verified"). Allow it once:

1. In Finder, right-click (or Control-click) **EasyOffice** in `/Applications`, choose **Open**, then **Open** again in the dialog; or
2. Try to open it once, then go to **System Settings > Privacy & Security**, scroll to the security section and click **Open Anyway** for EasyOffice (macOS 15 requires this route instead of right-click > Open).
3. Fallback, if the app is reported as damaged or does not start (the browser attached the download quarantine flag): clear the flag once.

```bash
xattr -dr com.apple.quarantine /Applications/EasyOffice.app
```

Later opens work normally. Signing is prepared but off: setting the `APPLE_SIGNING_IDENTITY` repository secret (plus the notarization variables) in the macOS CI and release jobs is all that is needed (PD-094). To build the `.dmg` from source on macOS:

```bash
cd crates/easyoffice-desktop
npx --yes --ignore-scripts @tauri-apps/cli@2.12.4 build --bundles app,dmg   # target/release/bundle/dmg/EasyOffice_<version>_<arch>.dmg
```

#### Windows

Windows releases ship an installer, `EasyOffice-windows-x64-setup.exe` (NSIS, per-user install, no administrator rights required). Run it, then start **EasyOffice** from the Start menu. `EasyOffice.exe` is a desktop window on the built-in Windows WebView2 runtime (Tauri v2 shell, `crates/easyoffice-desktop`): the WebUI is embedded in the executable and every command runs in-process on the Rust engine, so no local HTTP server, browser, or network connection is involved. Saving opens the native Save dialog. Uninstall from Settings > Apps or the uninstaller in the Start menu folder. WebView2 is part of Windows 11 and current Windows 10; only if it is missing does the installer download the runtime bootstrapper. The installer registers the main document formats (HWP, HWPX, DOCX, ODT, XLSX, ODS, PPTX, ODP, PDF) so a double-click opens the file in a new window of the running instance (single instance); plain-text formats such as TXT, MD and CSV are not claimed. Saving writes back to the opened file through a temporary file and rename, and asks before overwriting a file that changed on disk in the meantime.

To build the installer from source on Windows (MSVC Rust toolchain 1.90+, Visual Studio Build Tools, Node.js):

```powershell
cd crates/easyoffice-desktop
npx --yes --ignore-scripts @tauri-apps/cli@2.12.4 build --bundles nsis   # target/release/bundle/nsis/EasyOffice_<version>_x64-setup.exe
```

#### Linux

Linux releases ship `EasyOffice-linux-x64.AppImage`, `EasyOffice-linux-x64.deb`, `EasyOffice-linux-x64.rpm` and `EasyOffice-linux-arm64.deb`. Prefer the native package of your distribution: the Korean input method (IME) and the WebKitGTK runtime are most reliable when they come from the distribution, and the package declares them as dependencies. Install it with `sudo apt install ./EasyOffice-linux-x64.deb` (Ubuntu, Debian) or `sudo dnf install ./EasyOffice-linux-x64.rpm` (Fedora); on arm64 only the `.deb` is built. The AppImage needs no installation (`chmod +x EasyOffice-linux-x64.AppImage`, then run it; it needs FUSE 2, the `libfuse2` package) and is meant for cases where a native package is not an option: on some Wayland and IME setups switching between Korean and English input or showing the window can be unreliable. The packages register the main document formats (HWP, HWPX, DOCX, ODT, XLSX, ODS, PPTX, ODP, PDF) as "Open with" targets without taking over another application's default, and they are built on Ubuntu 22.04, so glibc 2.35 or newer and WebKitGTK 4.1 are required. Check a download with `sha256sum --check --ignore-missing SHA256SUMS.txt`. To build the packages from source on Linux (Rust 1.90+, Node.js, `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libxdo-dev`, `librsvg2-dev`, `libssl-dev`, `patchelf`, `rpm`):

```bash
cd crates/easyoffice-desktop
npx --yes --ignore-scripts @tauri-apps/cli@2.12.4 build --bundles appimage,deb,rpm   # target/release/bundle/{appimage,deb,rpm}/
```

**Developer-only local server (`--serve`).** `--serve` is not shipped to users and is not part of any release asset. It exists only for development and automated verification (headless-browser UI checks and CI smoke tests); the installed app opens no network port. From a source checkout, run `cargo run -p easyoffice-app -- --self-test` to verify the engine or `cargo run -p easyoffice-app -- --serve 8787` to open the loopback WebUI at `http://127.0.0.1:8787/`. `--help` lists all options. `--serve` without a port uses 8787 and falls back to 8788-8800 when it is busy; an explicit port (`--serve 18787`) is bound exactly or the command fails. Invalid ports, unknown options, and file paths exit with a non-zero status. The Host/Origin checks, connection and time limits, and CSP stay enabled for developer safety (PD-086).

**One window per document.** The installed app opens each document in its own window. Launching it without a file shows a compact start window (new DOCX / HWP / XLSX / PPTX / PDF / converter, open, recent files); **File → New**, **Open… (`Ctrl+O`)** and drag-and-drop open a new window (a file that is already open just brings its window to the front). The window title is `<file name> — EasyOffice` with a `●` mark for unsaved changes, closing asks to save / discard / cancel, and closing the last window exits. File paths stay in Rust; the WebView only receives the file name and bytes (PD-088). The developer-only `--serve` emulates the same model with `window.open`.

Inside the WebUI, **File → Open… (`Ctrl+O`)** or dragging a file onto the workspace opens your own DOCX/DOC/ODT/RTF/MD/TXT, HWP/HWPX/HWT/HWTX, XLSX/XLS/ODS/CSV/TSV, PPTX/PPT/PPSX/ODP, PDF, or Apple Pages (`.pages`, read-only) file (up to 10 MiB) in the matching module; the tab takes the file name, the open is one undo step, and anything the editor model cannot carry (images, shapes, headers, cell formatting) is reported after opening. Character formatting (size, font, text color, highlight, bold, italic, underline, strikethrough) applies to the selected text only and round-trips through DOCX, HWPX, ODT, and RTF. The converter panel can also convert a chosen source file's real bytes instead of the editor text.

---

## License

Released under the [MIT License](LICENSE).
