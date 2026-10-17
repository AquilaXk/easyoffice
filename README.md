# EasyOffice

[English](README.md) | [한국어](README.ko.md)

EasyOffice is a native macOS all-in-one productivity and office suite engineered with Swift 6, SwiftUI, and AppKit, featuring a signature lavender design system. It unifies **Document**, **Sheet**, **Slide**, **Korean Office (HWP / HWPX)**, and **PDF Studio** into a single lightweight multi-tabbed macOS workspace with deterministic format validation and zero cloud lock-in.

---

## Key Highlights

- **Unified 5-in-1 Native Workspace**: Open, edit, create, and convert word processing documents, spreadsheets, presentations, Korean HWP/HWPX files, and PDFs inside a single tabbed macOS window.
- **Comprehensive Format Compatibility**:
  - **Documents**: `.docx`, `.doc`, `.odt`, `.rtf`, `.md`, `.txt`
  - **Korean Office (HWP)**: `.hwp` (5.0 OLE2 Compound File), `.hwpx` (KS X 6101 OWPML), `.hwt`, `.hwtx`
  - **Spreadsheets**: `.xlsx`, `.xls`, `.ods`, `.csv`, `.tsv`
  - **Presentations**: `.pptx`, `.ppt`, `.ppsx`, `.odp`
  - **PDF Studio**: `.pdf` (ISO 32000-1 / 32000-2)
- **Context-Aware Compact Ribbon + Contextual Inspector**: Combines an intuitive top ribbon toolbar with a macOS-native right-hand format inspector and collapsible left navigator (Page Thumbnails, Outline, Sheet List, Slide Sorter, PDF Bookmarks).
- **Deterministic Formula & Analysis Engine**: Built-in A1 cell addressing, range parser, and formula evaluation engine (`SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT`, `COUNTA`, `IF`, `ROUND`, `ABS`, `CONCAT`, and statistical/lookup extensions), pivot table descriptors, and conditional formatting rules.
- **Bidirectional PDF & Batch Conversion Studio**: Convert office documents to high-fidelity PDFs or extract editable document structures from PDF files directly on device.
- **Fail-Closed Container Security**: Every ingested payload is verified against binary magic signatures (`PK\x03\x04`, `\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1`, `%PDF-`, UTF-8 validation) and decompression safety limits before parsing.
- **Signature Lavender Design System**: High-contrast, accessible interface built around a signature lavender palette (`#5C6BC0`) and 5 module semantic accents with seamless Light and Dark mode adaptation.

---

## Supported Format Matrix

| Module | Supported Extensions | Standard / Container Specification | Core Capabilities |
| :--- | :--- | :--- | :--- |
| **Document** | `DOCX`, `DOC`, `ODT`, `RTF`, `MD`, `TXT` | ECMA-376 WordprocessingML, ISO/IEC 26300 ODF, CommonMark | Multi-section page layout, paragraph/character styles, tables, track changes, mail merge |
| **HWP (Korean)** | `HWP`, `HWPX`, `HWT`, `HWTX` | HWP 5.0 OLE2 CFBF, KS X 6101 OWPML (ZIP+XML) | Native viewing & editing, official form tables, headers/footers, footnotes, HWPX/PDF export |
| **Sheet** | `XLSX`, `XLS`, `ODS`, `CSV`, `TSV` | ECMA-376 SpreadsheetML, ODF Spreadsheet, RFC 4180 | Virtualized 2D grid, formula evaluation engine, pivot table specs, charts, sort & validation |
| **Slide** | `PPTX`, `PPT`, `PPSX`, `ODP` | ECMA-376 PresentationML, ODF Presentation | Slide masters, 16:9 & 4:3 stage, smart alignment guides, transitions, presenter view & notes |
| **PDF Studio** | `PDF` | ISO 32000-1 / 32000-2 | Vector viewing, highlights & sticky annotations, page merge/split/reorder, Office ↔ PDF conversion |

---

## Architecture Overview

```text
EasyOffice/
├── Package.swift                        # Swift 6 Package Manifest (macOS 14+)
├── Sources/
│   ├── EasyOfficeDesignSystem/          # Signature Lavender palette, module tokens & layout metrics
│   ├── EasyOfficeCore/                  # Format detector, Document/Sheet/Slide/PDF/HWP engines & Hub
│   └── EasyOfficeApp/                   # Native macOS SwiftUI + AppKit workspace application
├── Tests/
│   └── EasyOfficeTests/                 # Deterministic unit & round-trip verification suite
├── scripts/
│   ├── check-repo-hygiene.mjs           # Repository hygiene & specification integrity verifier
│   └── package-macos-release.sh         # macOS .app bundle, .dmg, .zip, and SHA256SUMS builder
└── docs/
    ├── architecture.md                  # System architecture & security boundaries
    ├── design-system.md                 # Signature Lavender UI/UX design system specification
    ├── policy-decisions.md              # Product & engineering policy decision record
    └── product-spec.md                  # Complete module & format feature specification
```

---

## Requirements & Quick Start

### System Requirements
- **OS**: macOS 14.0 (Sonoma) or later (Apple Silicon & Intel Universal)
- **Toolchain**: Xcode 16+ / Swift 6.0+

### Build, Test & Run Locally

```bash
# Compile all modules
swift build

# Run the deterministic verification suite
swift test

# Launch the native macOS application
swift run EasyOfficeApp

# Package a release macOS .app bundle, .dmg, and .zip archive
bash scripts/package-macos-release.sh 0.1.0
```

---

## License

Released under the [MIT License](LICENSE).
