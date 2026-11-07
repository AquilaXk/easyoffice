# EasyOffice

[English](README.md) | [한국어](README.ko.md)

EasyOffice (`v1.0.0`) is a 100% standalone, serverless, offline-capable all-in-one productivity and office suite for macOS and Windows: a Rust engine and an offline WebUI hosted in one Tauri v2 desktop shell, featuring a Signature Lavender Design System (`#5C6BC0`). It unifies **Document**, **Sheet**, **Slide**, **Korean Office (HWP / HWPX)**, and **PDF Studio** into a single lightweight workspace with bit-level container codecs, deterministic formula evaluation, offline smart document assistance, and fail-closed security guards.

---

## Key Highlights

- **Unified 5-in-1 Workspace (macOS and Windows, one code base)**:
  - The HWP module embeds the rhwp-based HWP editor (same-origin iframe, no IPC access) in the Windows desktop shell and `--serve`; builds without the generated editor bundle fall back to the text-only editor. Build it with `node scripts/checkout-rhwp.mjs && node scripts/build-hwp-studio.mjs` before the desktop shell (PD-083). HWP, HWPX, HWT and HWTX files open in it (up to 64 MiB; files the editor cannot read open as text only, with a notice) and save as HWP or HWPX after the exported bytes are re-parsed; unsaved changes are tracked and autosaved to IndexedDB for restore after a reload (PD-085).
- Open, edit, create, duplicate, pin, and convert word processing documents, Korean HWP/HWPX files, spreadsheets, presentations, and PDFs inside per-document windows of the desktop app.
  - Supports **Side-by-Side Split View** (`splitSecondaryTabID`, minimum `420pt` per pane) for comparing two documents simultaneously, a **Collapsible 7-Category Context Ribbon** (`Cmd+Option+R`), a **Left Structure Navigator**, a **Right Contextual Inspector**, and **5 Interactive Editor Detail Panes** (`DocumentEditorDetailPaneView`, `SheetEditorDetailPaneView`, `SlideEditorDetailPaneView`, `PDFStudioEditorDetailPaneView`).
- **20-Format Binary & Container Round-Trip Codecs**:
  - **ZIP-based Containers (`ZipContainerCodec`)**: `OOXMLContainerCodec` (`.docx`, `.xlsx`, `.pptx`, `.ppsx`), `ODFContainerCodec` (`.odt`, `.ods`, `.odp`), `HWPXContainerCodec` (`.hwpx`, `.hwtx` per KS X 6101 OWPML).
  - **OLE2 CFBF & HWP 5.0 Binary (`OLE2ContainerCodec`, `HWP5BinaryCodec`)**: 512-byte sector allocation table, directory stream builder/parser, and 32-bit packed `HWP5RecordHeader` (`10-bit tagID | 10-bit level | 12-bit size` + `0xFFF` extended length) for `.hwp`, `.hwt`, `.doc`, `.xls`, `.ppt`.
  - **PDF & Tabular/Markup Codecs**: `PDFBinarySerializer` (`.pdf` ISO 32000 cross-reference table & trailer builder/parser), `CSVStateMachineCodec` (`.csv`, `.tsv` per RFC 4180), `CommonMarkCodec` (`.md`), `RTFCodec` (`.rtf` with `\uN?` signed 16-bit Unicode escapes), and UTF-8 plain text (`.txt`).
- **Bit-Level Performance Optimizations**:
  - `BitPacking` (little-endian `UInt16`/`UInt32`/`UInt64` reader/writer & length-prefixed UTF-16LE encoding), 256-entry lookup table IEEE 802.3 `CRC32Calculator`, `HWP5RecordHeader` bitfield packing, and 64-bit `PackedCellCoordinate` (`UInt64`: upper 32-bit `row`, lower 32-bit `column`) for zero-allocation $O(1)$ cell lookup and cycle detection.
- **5 Core Deterministic Editor Engines**:
  - **Document & HWP/HWPX (`OfficeDocumentModel`)**: Multi-section page layout (`PaperSizePreset`, `PageOrientation`), paragraph/character styles, spanned tables with inline `=SUM(ABOVE|LEFT)` / `=AVERAGE(ABOVE|LEFT)` formulas (`DocumentTableBlock`), footnotes/endnotes, tracked revisions (`TrackedRevision`), comment threads, auto-generated table of contents (`TableOfContentsEntry`), and `{{field}}` mail merge.
  - **Sheet (`WorksheetModel`)**: **122 built-in functions** across Math/Trig, Statistical, Logical, Lookup/Reference, Text, Financial (`PMT`, `FV`, `PV`, `NPV`, `IRR`, `RATE`, `NPER`, `SLN`, `SYD`), Date/Time, and Information/Engineering, plus `PivotTableEngine`, `SheetChartBuilder`, `ConditionalFormatRule`, and `DataValidationRule`.
  - **Slide (`PresentationDeckModel`)**: `SlideAspectRatio` (`16:9`, `4:3`, `A4`), `SlideMasterLayout`, `SlideCanvasElement` z-order & alignment/distribution, `SnapGuideCalculator` smart alignment guides, `SlideTransitionKind`, and `PresenterSession`.
  - **PDF Studio (`PDFStudioDocumentModel`)**: Vector page canvas, `PDFAnnotationItem` (`highlight`, `underline`, `stickyNote`, `stamp`, `signature`), `PDFOutlineNode` tree, page rotation/reorder/split/merge, and bidirectional Office ↔ PDF conversion.
- **100% Offline Smart Productivity & Security Studio**:
  - **Batch Conversion Studio (`DocumentConversionOrchestrator`)**: Single and batch conversion across all 20 supported formats with post-conversion `FormatDetector.verify` signature verification.
  - **Offline Smart Document Assistant (`LocalSmartDocumentAssistant`)**: Extractive TF-position summarization (`DocumentSummaryReport`), writing quality diagnostics (`WritingRefinementSuggestion`), action item extraction (`ExtractedActionItem`), sheet Z-score anomaly (`|z| >= 1.8`) & linear trend insights (`WorksheetInsightReport`), and bounded ($N \le 2,048$) dynamic-programming LCS document diff (`DocumentDiffReport`).
  - **Fail-Closed Container & Metadata Security (`FormatDetector`, `XMLSanitizer`, `DocumentSecurityAuditor`)**: Blocks magic-byte spoofing, ZIP decompression bombs (`100:1` ratio / `512 MiB` ceiling), path traversal (`..`, `/`, `\`, NUL), XML External Entities (`<!ENTITY`, `DOCTYPE SYSTEM/PUBLIC`), and scrubs document PII/revisions/comments before sharing.
- **Signature Lavender Design System & WCAG AA Accessibility**:
  - Built around `EasyOfficeDesignTokens` (`#5C6BC0` Primary Accent, `#F7F8FC` Light Lavender Scaffold, `#141724` Dark Indigo-Charcoal Scaffold), `WorkspaceThemeMode` (`.system`, `.lightLavender`, `.darkIndigo`), 5 module semantic accents, and `AccessibilityTokens` with verified WCAG 2.1 AA contrast ratios (`>= 4.5:1`).

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
├── crates/easyoffice-desktop/        # Tauri v2 desktop shell for Windows (WebView2) and macOS (WKWebView)
├── apps/hwp-studio/                  # HWP editor overlay built on the pinned rhwp upstream
├── scripts/                          # Hygiene guard, bundle contract checks, WebUI regression tests
└── docs/                             # Architecture, design system, policy decisions (SSOT), product spec
```

---

## Requirements & Quick Start

### System Requirements
- macOS 11.0 (Big Sur) or later on Apple Silicon (`arm64`) and Intel (`x86_64`) for the universal `.dmg` (building the desktop shell needs Rust 1.90+, Node.js and Xcode command line tools)
- Windows 10/11 x64 with the WebView2 runtime for the installed Windows desktop app (building the desktop shell needs Rust 1.90+ with the MSVC toolchain)
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

#### macOS

macOS releases ship a universal disk image, `EasyOffice-<version>-macos-universal.dmg` (Apple silicon and Intel, macOS 11 or later) with a `SHA256SUMS-macos-universal.txt` checksum file. Open the image and drag **EasyOffice** to `/Applications`. The app is the same Tauri v2 desktop shell as on Windows (WKWebView): the WebUI is embedded, every command runs in-process on the Rust engine, and no network connection is involved. The standard menu bar and Command shortcuts (⌘C/⌘V/⌘X/⌘A/⌘Z/⌘S/⌘O/⌘W/⌘Q) work; Finder "Open With" and double-click open files in a new window. Quitting with ⌘Q goes through each window's save / discard / cancel prompt.

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
npx @tauri-apps/cli@2 build --bundles app,dmg   # target/release/bundle/dmg/EasyOffice_<version>_<arch>.dmg
```

#### Windows

Windows releases ship an installer, `EasyOffice-<version>-windows-x64-setup.exe` (NSIS, per-user install, no administrator rights required). Run it, then start **EasyOffice** from the Start menu. `EasyOffice.exe` is a desktop window on the built-in Windows WebView2 runtime (Tauri v2 shell, `crates/easyoffice-desktop`): the WebUI is embedded in the executable and every command runs in-process on the Rust engine, so no local HTTP server, browser, or network connection is involved. Saving opens the native Save dialog. Uninstall from Settings > Apps or the uninstaller in the Start menu folder. WebView2 is part of Windows 11 and current Windows 10; only if it is missing does the installer download the runtime bootstrapper. The installer registers the main document formats (HWP, HWPX, DOCX, ODT, XLSX, ODS, PPTX, ODP, PDF) so a double-click opens the file in a new window of the running instance (single instance); plain-text formats such as TXT, MD and CSV are not claimed. Saving writes back to the opened file through a temporary file and rename, and asks before overwriting a file that changed on disk in the meantime.

To build the installer from source on Windows (MSVC Rust toolchain 1.90+, Visual Studio Build Tools, Node.js):

```powershell
cd crates/easyoffice-desktop
npx @tauri-apps/cli@2 build --bundles nsis   # target/release/bundle/nsis/EasyOffice_<version>_x64-setup.exe
```

**Developer-only local server (`--serve`).** `--serve` is not shipped to users and is not part of any release asset. It exists only for development and automated verification (headless-browser UI checks and CI smoke tests); the installed app opens no network port. From a source checkout, run `cargo run -p easyoffice-app -- --self-test` to verify the engine or `cargo run -p easyoffice-app -- --serve 8787` to open the loopback WebUI at `http://127.0.0.1:8787/`. `--help` lists all options. `--serve` without a port uses 8787 and falls back to 8788-8800 when it is busy; an explicit port (`--serve 18787`) is bound exactly or the command fails. Invalid ports, unknown options, and file paths exit with a non-zero status. The Host/Origin checks, connection and time limits, and CSP stay enabled for developer safety (PD-086).

**One window per document.** The installed app opens each document in its own window. Launching it without a file shows a compact start window (new DOCX / HWP / XLSX / PPTX / PDF / converter, open, recent files); **File → New**, **Open… (`Ctrl+O`)** and drag-and-drop open a new window (a file that is already open just brings its window to the front). The window title is `<file name> — EasyOffice` with a `●` mark for unsaved changes, closing asks to save / discard / cancel, and closing the last window exits. File paths stay in Rust; the WebView only receives the file name and bytes (PD-088). The developer-only `--serve` emulates the same model with `window.open`.

Inside the WebUI, **File → Open… (`Ctrl+O`)** or dragging a file onto the workspace opens your own DOCX/DOC/ODT/RTF/MD/TXT, HWP/HWPX/HWT/HWTX, XLSX/XLS/ODS/CSV/TSV, PPTX/PPT/PPSX/ODP, PDF, or Apple Pages (`.pages`, read-only) file (up to 10 MiB) in the matching module; the tab takes the file name, the open is one undo step, and anything the editor model cannot carry (images, shapes, headers, cell formatting) is reported after opening. Character formatting (size, font, text color, highlight, bold, italic, underline, strikethrough) applies to the selected text only and round-trips through DOCX, HWPX, ODT, and RTF. The converter panel can also convert a chosen source file's real bytes instead of the editor text.

---

## License

Released under the [MIT License](LICENSE).
