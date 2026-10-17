import Foundation

/// Standard page paper sizes for Document and HWP/HWPX layouts.
public enum PaperSizePreset: String, CaseIterable, Sendable, Codable {
    case a4 = "A4 (210 × 297 mm)"
    case letter = "Letter (215.9 × 279.4 mm)"
    case b5 = "B5 (176 × 250 mm)"

    public var widthPoints: Double {
        switch self {
        case .a4: return 595.28
        case .letter: return 612.0
        case .b5: return 498.90
        }
    }

    public var heightPoints: Double {
        switch self {
        case .a4: return 841.89
        case .letter: return 792.0
        case .b5: return 708.66
        }
    }
}

/// Paragraph style roles shared by WordprocessingML, ODF Text, and HWP/HWPX documents.
public enum ParagraphStyleRole: String, CaseIterable, Sendable, Codable {
    case title = "제목 (Title)"
    case heading1 = "제목 1 (Heading 1)"
    case heading2 = "제목 2 (Heading 2)"
    case heading3 = "제목 3 (Heading 3)"
    case body = "본문 (Body)"
    case quote = "인용문 (Block Quote)"
    case officialHeader = "공문서 두문/결재란 (Official Header)"
}

/// Text alignment options.
public enum TextAlignmentMode: String, CaseIterable, Sendable, Codable {
    case leading = "왼쪽 정렬"
    case center = "가운데 정렬"
    case trailing = "오른쪽 정렬"
    case justified = "양쪽 정렬"
}

/// Paragraph block in a rich document or HWP section.
public struct ParagraphBlock: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var style: ParagraphStyleRole
    public var alignment: TextAlignmentMode
    public var text: String
    public var fontName: String
    public var fontSizePt: Double
    public var isBold: Bool
    public var isItalic: Bool
    public var isUnderlined: Bool
    public var lineSpacingPercent: Double

    public init(
        id: UUID = UUID(),
        style: ParagraphStyleRole = .body,
        alignment: TextAlignmentMode = .justified,
        text: String,
        fontName: String = "Pretendard",
        fontSizePt: Double = 11.0,
        isBold: Bool = false,
        isItalic: Bool = false,
        isUnderlined: Bool = false,
        lineSpacingPercent: Double = 160.0
    ) {
        self.id = id
        self.style = style
        self.alignment = alignment
        self.text = text
        self.fontName = fontName
        self.fontSizePt = fontSizePt
        self.isBold = isBold
        self.isItalic = isItalic
        self.isUnderlined = isUnderlined
        self.lineSpacingPercent = lineSpacingPercent
    }
}

/// Structured table block inside a document or HWP form.
public struct DocumentTableBlock: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var caption: String
    public var rows: [[String]]
    public var hasHeaderRow: Bool

    public init(
        id: UUID = UUID(),
        caption: String = "",
        rows: [[String]],
        hasHeaderRow: Bool = true
    ) {
        self.id = id
        self.caption = caption
        self.rows = rows
        self.hasHeaderRow = hasHeaderRow
    }

    public var rowCount: Int { rows.count }
    public var columnCount: Int { rows.first?.count ?? 0 }
}

/// Document statistics summary (words, characters, 200-character Korean manuscript pages, tables).
public struct DocumentStatistics: Sendable, Equatable {
    public let paragraphCount: Int
    public let tableCount: Int
    public let wordCount: Int
    public let characterCountWithSpaces: Int
    public let characterCountWithoutSpaces: Int
    public let manuscriptPages200: Int
}

/// Unified rich document model for Document (.docx/.odt/.md/.txt) and Korean Office (.hwp/.hwpx).
public struct OfficeDocumentModel: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var title: String
    public var format: DocumentFormat
    public var paperSize: PaperSizePreset
    public var headerText: String
    public var footerText: String
    public var paragraphs: [ParagraphBlock]
    public var tables: [DocumentTableBlock]
    public var trackChangesEnabled: Bool

    public init(
        id: UUID = UUID(),
        title: String,
        format: DocumentFormat = .docx,
        paperSize: PaperSizePreset = .a4,
        headerText: String = "",
        footerText: String = "",
        paragraphs: [ParagraphBlock] = [],
        tables: [DocumentTableBlock] = [],
        trackChangesEnabled: Bool = false
    ) {
        self.id = id
        self.title = title
        self.format = format
        self.paperSize = paperSize
        self.headerText = headerText
        self.footerText = footerText
        self.paragraphs = paragraphs
        self.tables = tables
        self.trackChangesEnabled = trackChangesEnabled
    }

    /// Computes live document metrics across all paragraphs and table cells.
    public func computeStatistics() -> DocumentStatistics {
        let paragraphTexts = paragraphs.map(\.text)
        let tableCellTexts = tables.flatMap { $0.rows.flatMap { $0 } }
        let combinedSegments = paragraphTexts + tableCellTexts
        let fullText = combinedSegments.joined(separator: "\n")

        let charsWithSpaces = fullText.unicodeScalars.filter { $0 != "\n" && $0 != "\r" }.count
        let charsWithoutSpaces = fullText.unicodeScalars.filter { !CharacterSet.whitespacesAndNewlines.contains($0) }.count

        let words = fullText
            .components(separatedBy: .whitespacesAndNewlines)
            .filter { !$0.isEmpty }
            .count

        let manuscriptPages = charsWithSpaces == 0 ? 0 : Int(ceil(Double(charsWithSpaces) / 200.0))

        return DocumentStatistics(
            paragraphCount: paragraphs.count,
            tableCount: tables.count,
            wordCount: words,
            characterCountWithSpaces: charsWithSpaces,
            characterCountWithoutSpaces: charsWithoutSpaces,
            manuscriptPages200: manuscriptPages
        )
    }

    /// Extracts heading outline entries (Title, Heading 1~3, Official Header) for the left navigator.
    public func extractOutline() -> [ParagraphBlock] {
        paragraphs.filter {
            switch $0.style {
            case .title, .heading1, .heading2, .heading3, .officialHeader:
                return true
            case .body, .quote:
                return false
            }
        }
    }

    /// Performs deterministic Mail Merge substitution of `{{key}}` placeholders across paragraphs, headers, and tables.
    public func applyingMailMerge(record: [String: String]) -> OfficeDocumentModel {
        func replacePlaceholders(in source: String) -> String {
            var output = source
            for key in record.keys.sorted() {
                if let value = record[key] {
                    output = output.replacingOccurrences(of: "{{\(key)}}", with: value)
                }
            }
            return output
        }

        var copy = self
        copy.headerText = replacePlaceholders(in: copy.headerText)
        copy.footerText = replacePlaceholders(in: copy.footerText)
        copy.paragraphs = copy.paragraphs.map { block in
            var updated = block
            updated.text = replacePlaceholders(in: block.text)
            return updated
        }
        copy.tables = copy.tables.map { table in
            var updatedTable = table
            updatedTable.caption = replacePlaceholders(in: updatedTable.caption)
            updatedTable.rows = updatedTable.rows.map { row in
                row.map { replacePlaceholders(in: $0) }
            }
            return updatedTable
        }
        return copy
    }
}
