# EasyOffice

[English](README.md) | [한국어](README.ko.md)

EasyOffice (`v1.0.0`) is a 100% standalone, serverless, offline-capable native macOS all-in-one productivity and office suite engineered with Swift 6, SwiftUI, and AppKit, featuring a Signature Lavender Design System (`#5C6BC0`). It unifies **Document**, **Sheet**, **Slide**, **Korean Office (HWP / HWPX)**, and **PDF Studio** into a single lightweight multi-tabbed macOS workspace with bit-level container codecs, deterministic formula evaluation, offline smart document assistance, and fail-closed security guards.

---

## Key Highlights

- **Unified 5-in-1 Native macOS Workspace (`WorkspaceViewModel`)**:
  - Open, edit, create, duplicate, pin, and convert word processing documents, Korean HWP/HWPX files, spreadsheets, presentations, and PDFs inside a single tabbed macOS window.
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
| **Sheet** | `XLSX`, `XLS`, `ODS`, `CSV`, `TSV` | ECMA-376 SpreadsheetML, OLE2 BIFF8, ISO/IEC 26300 ODF Spreadsheet, RFC 4180 CSV/TSV | `PackedCellCoordinate`, 122 built-in functions, `PivotTableEngine`, `SheetChartBuilder`, `CSVStateMachineCodec`, multi-column sort, conditional format & validation rules |
| **Slide** | `PPTX`, `PPT`, `PPSX`, `ODP` | ECMA-376 PresentationML, OLE2 Presentation, ISO/IEC 26300 ODF Presentation | `PresentationDeckModel`, 16:9 / 4:3 / A4 stage, `SnapGuideCalculator` smart alignment guides, z-order layer arrangement, transitions, `PresenterSession` |
| **PDF Studio** | `PDF` | ISO 32000-1 / 32000-2 (`%PDF-1.x` / `%PDF-2.0`) | `PDFBinarySerializer`, `PDFStudioDocumentModel`, 5 annotation types, outline bookmarks, page split/merge/reorder/rotate, bidirectional Office ↔ PDF conversion |

---

## Architecture Overview

```text
EasyOffice/
├── Package.swift                                  # Swift 6 Package Manifest (macOS 14+)
├── Sources/
│   ├── EasyOfficeDesignSystem/                    # Signature Lavender palette, module tokens & WCAG contrast
│   │   └── DesignTokens.swift                     # RGBColorToken, WorkspaceThemeMode, AccessibilityTokens
│   ├── EasyOfficeCore/                            # Pure deterministic office engines, binary codecs & smart tools
│   │   ├── DocumentFormat.swift                   # 20-format registry & ContainerFamily metadata
│   │   ├── FormatDetector.swift                   # Magic-byte signature guard & EasyOfficeError
│   │   ├── BinaryContainerCodec.swift             # BitPacking, CRC32Calculator, ZIP/OOXML/ODF/HWPX/OLE2/HWP5 codecs
│   │   ├── TabularAndMarkupCodec.swift            # CSVStateMachineCodec, CommonMarkCodec, RTFCodec
│   │   ├── DocumentEngine.swift                   # OfficeDocumentModel, sections, tables, revisions, mail merge
│   │   ├── SheetEngine.swift                      # PackedCellCoordinate, 77 functions, PivotTableEngine, charts
│   │   ├── SlideEngine.swift                      # PresentationDeckModel, SnapGuideCalculator, PresenterSession
│   │   ├── PDFEngine.swift                        # PDFStudioDocumentModel, annotations, split/merge, Office ↔ PDF
│   │   ├── SmartProductivityTools.swift           # PDFBinarySerializer, conversion studio, offline assistant, security auditor
│   │   └── WorkspaceHub.swift                     # WorkspaceViewModel, 7-category ribbon catalog, templates, search
│   └── EasyOfficeApp/                             # Native macOS SwiftUI + AppKit workspace application
│       ├── EasyOfficeApp.swift                    # @main application entry point & macOS menu commands
│       ├── WorkspaceViews.swift                   # Unified titlebar, multi-tab strip, split view, ribbon, Home Hub
│       └── EditorDetailPanes.swift                # Interactive Document/HWP, Sheet, Slide, and PDF Studio panes
├── Tests/
│   └── EasyOfficeTests/                           # Deterministic unit, security, UI parity & benchmark suite
│       ├── EasyOfficeTests.swift                  # Core tokens, format catalog & baseline engine tests
│       ├── BinaryContainerCodecTests.swift        # BitPacking, CRC32, ZIP/OLE2/HWP5/CSV/MD/RTF & security guards
│       ├── EditorEnginesParityTests.swift         # 77 sheet functions, pivot/chart, document/HWP, slide & PDF parity
│       ├── SmartProductivityToolsTests.swift      # Batch conversion, offline smart assistant & security auditor tests
│       ├── WorkspaceUIParityTests.swift           # Split view, 7x5 ribbon matrix, WCAG AA & accessibility tests
│       └── CommercialParityBenchmarkTests.swift   # 20-format round-trip matrix & bit-level throughput benchmarks
├── scripts/
│   ├── check-repo-hygiene.mjs                     # Repository hygiene & specification integrity verifier
│   └── package-macos-release.sh                   # Universal macOS .app bundle, .dmg, .zip, and SHA256SUMS builder
└── docs/
    ├── architecture.md                            # System architecture, codecs & fail-closed security boundaries
    ├── design-system.md                           # Signature Lavender UI/UX design system & layout specification
    ├── policy-decisions.md                        # Product & engineering policy decision record (SSOT)
    └── product-spec.md                            # Complete 20-format & 5-module feature specification
```

---

## Requirements & Quick Start

### System Requirements
- **OS**: macOS 14.0 (Sonoma) or later (Apple Silicon `arm64` & Intel `x86_64` Universal)
- **Toolchain**: Xcode 16+ / Swift 6.0+

### Build, Verify & Run Locally

```bash
# 1. Verify repository hygiene & specification contracts
node scripts/check-repo-hygiene.mjs

# 2. Compile all modules
swift build

# 3. Run the deterministic verification & benchmark suite
swift test

# 4. Launch the native macOS application
swift run EasyOfficeApp

# 5. Package a release macOS .app bundle, .dmg, .zip archive, and SHA256SUMS.txt
bash scripts/package-macos-release.sh 1.0.0
```

---

## License

Released under the [MIT License](LICENSE).
