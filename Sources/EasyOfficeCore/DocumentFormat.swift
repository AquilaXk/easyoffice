import Foundation
import EasyOfficeDesignSystem

/// Binary or text container family for office formats.
public enum ContainerFamily: String, Sendable, Codable, Equatable {
    case zipXML = "zip-xml"           // OOXML (DOCX/XLSX/PPTX), ODF (ODT/ODS/ODP), HWPX/HWTX (OWPML)
    case ole2Compound = "ole2-cfbf"   // HWP 5.0, HWT, DOC, XLS, PPT
    case pdfBinary = "pdf-binary"     // ISO 32000 PDF
    case rtfText = "rtf-text"         // Rich Text Format ({\rtf)
    case plainText = "plain-text"     // UTF-8 CSV, TSV, MD, TXT
}

/// Supported file formats across the 5 core office modules.
public enum DocumentFormat: String, CaseIterable, Sendable, Codable, Identifiable {
    // Document (Word)
    case docx
    case doc
    case odt
    case rtf
    case md
    case txt

    // Korean Office (HWP)
    case hwp
    case hwpx
    case hwt
    case hwtx

    // Sheet (Spreadsheet)
    case xlsx
    case xls
    case ods
    case csv
    case tsv

    // Slide (Presentation)
    case pptx
    case ppt
    case ppsx
    case odp

    // PDF Studio
    case pdf

    public var id: String { rawValue }

    public var fileExtension: String { rawValue }

    public var moduleKind: OfficeModuleKind {
        switch self {
        case .docx, .doc, .odt, .rtf, .md, .txt:
            return .document
        case .hwp, .hwpx, .hwt, .hwtx:
            return .hwp
        case .xlsx, .xls, .ods, .csv, .tsv:
            return .sheet
        case .pptx, .ppt, .ppsx, .odp:
            return .slide
        case .pdf:
            return .pdf
        }
    }

    public var containerFamily: ContainerFamily {
        switch self {
        case .docx, .odt, .hwpx, .hwtx, .xlsx, .ods, .pptx, .ppsx, .odp:
            return .zipXML
        case .doc, .hwp, .hwt, .xls, .ppt:
            return .ole2Compound
        case .pdf:
            return .pdfBinary
        case .rtf:
            return .rtfText
        case .md, .txt, .csv, .tsv:
            return .plainText
        }
    }

    public var standardSpecification: String {
        switch self {
        case .docx:
            return "ECMA-376 / ISO/IEC 29500 WordprocessingML"
        case .doc:
            return "MS-DOC OLE2 Compound File Binary"
        case .odt:
            return "ISO/IEC 26300 OpenDocument Text"
        case .rtf:
            return "Rich Text Format 1.9.1"
        case .md:
            return "CommonMark Markdown (UTF-8)"
        case .txt:
            return "UTF-8 Plain Text"
        case .hwp, .hwt:
            return "HWP 5.0 OLE2 Compound File Binary"
        case .hwpx, .hwtx:
            return "KS X 6101 OWPML (ZIP + XML)"
        case .xlsx:
            return "ECMA-376 / ISO/IEC 29500 SpreadsheetML"
        case .xls:
            return "BIFF8 OLE2 Compound File Binary"
        case .ods:
            return "ISO/IEC 26300 OpenDocument Spreadsheet"
        case .csv:
            return "RFC 4180 Comma-Separated Values (UTF-8)"
        case .tsv:
            return "Tab-Separated Values (UTF-8)"
        case .pptx, .ppsx:
            return "ECMA-376 / ISO/IEC 29500 PresentationML"
        case .ppt:
            return "MS-PPT OLE2 Compound File Binary"
        case .odp:
            return "ISO/IEC 26300 OpenDocument Presentation"
        case .pdf:
            return "ISO 32000-1 / 32000-2 Portable Document Format"
        }
    }

    public static func from(fileNameOrExtension input: String) -> DocumentFormat? {
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !trimmed.isEmpty else { return nil }
        let ext: String
        if let dotIndex = trimmed.lastIndex(of: ".") {
            ext = String(trimmed[trimmed.index(after: dotIndex)...])
        } else {
            ext = trimmed
        }
        return DocumentFormat(rawValue: ext)
    }
}
