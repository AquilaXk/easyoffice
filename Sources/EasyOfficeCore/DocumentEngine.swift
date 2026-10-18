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

/// Page orientation for section layouts.
public enum PageOrientation: String, CaseIterable, Sendable, Codable {
    case portrait = "세로 (Portrait)"
    case landscape = "가로 (Landscape)"
}

/// Section layout configuration supporting multi-column and custom margins.
public struct DocumentSectionLayout: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var paperSize: PaperSizePreset
    public var orientation: PageOrientation
    public var columnCount: Int
    public var marginTopPt: Double
    public var marginBottomPt: Double
    public var marginLeftPt: Double
    public var marginRightPt: Double

    public init(
        id: UUID = UUID(),
        paperSize: PaperSizePreset = .a4,
        orientation: PageOrientation = .portrait,
        columnCount: Int = 1,
        marginTopPt: Double = 56.7,
        marginBottomPt: Double = 56.7,
        marginLeftPt: Double = 56.7,
        marginRightPt: Double = 56.7
    ) {
        self.id = id
        self.paperSize = paperSize
        self.orientation = orientation
        self.columnCount = max(1, min(4, columnCount))
        self.marginTopPt = marginTopPt
        self.marginBottomPt = marginBottomPt
        self.marginLeftPt = marginLeftPt
        self.marginRightPt = marginRightPt
    }

    public var effectiveWidthPoints: Double {
        orientation == .portrait ? paperSize.widthPoints : paperSize.heightPoints
    }

    public var effectiveHeightPoints: Double {
        orientation == .portrait ? paperSize.heightPoints : paperSize.widthPoints
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

/// Shared thread-safe ISO-8601 timestamp helper avoiding repeated formatter object instantiation.
@inlinable
public func currentISO8601Timestamp() -> String {
    Date().ISO8601Format()
}

/// Inline styled text run within a paragraph block.
public struct DocumentTextRun: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var text: String
    public var isBold: Bool
    public var isItalic: Bool
    public var isUnderlined: Bool
    public var fontName: String?
    public var fontSizePt: Double?
    public var foregroundHex: String?

    public init(
        id: UUID = UUID(),
        text: String,
        isBold: Bool = false,
        isItalic: Bool = false,
        isUnderlined: Bool = false,
        fontName: String? = nil,
        fontSizePt: Double? = nil,
        foregroundHex: String? = nil
    ) {
        self.id = id
        self.text = text
        self.isBold = isBold
        self.isItalic = isItalic
        self.isUnderlined = isUnderlined
        self.fontName = fontName
        self.fontSizePt = fontSizePt
        self.foregroundHex = foregroundHex
    }
}

/// Paragraph block in a rich document or HWP section.
public struct ParagraphBlock: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var style: ParagraphStyleRole
    public var alignment: TextAlignmentMode
    public var text: String
    public var runs: [DocumentTextRun]
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
        runs: [DocumentTextRun] = [],
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
        self.runs = runs
        self.fontName = fontName
        self.fontSizePt = fontSizePt
        self.isBold = isBold
        self.isItalic = isItalic
        self.isUnderlined = isUnderlined
        self.lineSpacingPercent = lineSpacingPercent
    }

    /// Returns explicit inline runs when present and synchronized with `text`, or a single synthesized run representing the paragraph's base style.
    public var effectiveRuns: [DocumentTextRun] {
        if !runs.isEmpty, runs.map(\.text).joined() == text {
            return runs
        }
        return [
            DocumentTextRun(
                id: id,
                text: text,
                isBold: isBold,
                isItalic: isItalic,
                isUnderlined: isUnderlined,
                fontName: fontName,
                fontSizePt: fontSizePt,
                foregroundHex: nil
            )
        ]
    }

    /// Splits and updates inline `runs` over `range` within `text` with the specified character attributes.
    public mutating func applyRunFormatting(
        range: Range<String.Index>,
        isBold: Bool? = nil,
        isItalic: Bool? = nil,
        isUnderlined: Bool? = nil,
        fontSizePt: Double? = nil,
        foregroundHex: String? = nil
    ) {
        let targetStart = text.distance(from: text.startIndex, to: range.lowerBound)
        let targetEnd = text.distance(from: text.startIndex, to: range.upperBound)
        guard targetStart < targetEnd else { return }

        let sourceRuns = effectiveRuns
        var splitRuns: [DocumentTextRun] = []
        var currentOffset = 0

        for run in sourceRuns {
            let runLen = run.text.count
            let runStart = currentOffset
            let runEnd = currentOffset + runLen
            currentOffset = runEnd

            guard runLen > 0 else { continue }
            if runEnd <= targetStart || runStart >= targetEnd {
                splitRuns.append(run)
                continue
            }

            if runStart < targetStart {
                let prefixLen = targetStart - runStart
                let prefixEndIdx = run.text.index(run.text.startIndex, offsetBy: prefixLen)
                splitRuns.append(
                    DocumentTextRun(
                        text: String(run.text[run.text.startIndex..<prefixEndIdx]),
                        isBold: run.isBold,
                        isItalic: run.isItalic,
                        isUnderlined: run.isUnderlined,
                        fontName: run.fontName,
                        fontSizePt: run.fontSizePt,
                        foregroundHex: run.foregroundHex
                    )
                )
            }

            let overlapStart = max(runStart, targetStart) - runStart
            let overlapEnd = min(runEnd, targetEnd) - runStart
            let midStartIdx = run.text.index(run.text.startIndex, offsetBy: overlapStart)
            let midEndIdx = run.text.index(run.text.startIndex, offsetBy: overlapEnd)
            splitRuns.append(
                DocumentTextRun(
                    text: String(run.text[midStartIdx..<midEndIdx]),
                    isBold: isBold ?? run.isBold,
                    isItalic: isItalic ?? run.isItalic,
                    isUnderlined: isUnderlined ?? run.isUnderlined,
                    fontName: run.fontName,
                    fontSizePt: fontSizePt ?? run.fontSizePt,
                    foregroundHex: foregroundHex ?? run.foregroundHex
                )
            )

            if runEnd > targetEnd {
                let suffixStart = targetEnd - runStart
                let suffixStartIdx = run.text.index(run.text.startIndex, offsetBy: suffixStart)
                splitRuns.append(
                    DocumentTextRun(
                        text: String(run.text[suffixStartIdx..<run.text.endIndex]),
                        isBold: run.isBold,
                        isItalic: run.isItalic,
                        isUnderlined: run.isUnderlined,
                        fontName: run.fontName,
                        fontSizePt: run.fontSizePt,
                        foregroundHex: run.foregroundHex
                    )
                )
            }
        }

        // Coalesce adjacent runs that share identical formatting attributes.
        var coalesced: [DocumentTextRun] = []
        for run in splitRuns where !run.text.isEmpty {
            if let last = coalesced.last,
               last.isBold == run.isBold,
               last.isItalic == run.isItalic,
               last.isUnderlined == run.isUnderlined,
               last.fontName == run.fontName,
               last.fontSizePt == run.fontSizePt,
               last.foregroundHex == run.foregroundHex {
                coalesced[coalesced.count - 1].text += run.text
            } else {
                coalesced.append(run)
            }
        }

        self.runs = coalesced
        self.text = coalesced.map(\.text).joined()
    }
}

public typealias DocumentParagraph = ParagraphBlock

/// Table border style presets including Korean administrative document double-line and diagonal borders.
public enum TableCellBorderStyle: String, CaseIterable, Sendable, Codable {
    case solid = "실선"
    case doubleLine = "이중선"
    case dashed = "점선"
    case diagonalDown = "대각선 (↘)"
    case diagonalUp = "역대각선 (↗)"
    case none = "없음"
}

/// Merged cell span descriptor in a document or HWP table.
public struct TableCellSpan: Sendable, Codable, Equatable {
    public var rowIndex: Int
    public var columnIndex: Int
    public var rowSpan: Int
    public var columnSpan: Int
    public var borderStyle: TableCellBorderStyle

    public init(
        rowIndex: Int,
        columnIndex: Int,
        rowSpan: Int = 1,
        columnSpan: Int = 1,
        borderStyle: TableCellBorderStyle = .solid
    ) {
        self.rowIndex = max(0, rowIndex)
        self.columnIndex = max(0, columnIndex)
        self.rowSpan = max(1, rowSpan)
        self.columnSpan = max(1, columnSpan)
        self.borderStyle = borderStyle
    }
}

/// Structured table block inside a document or HWP form.
public struct DocumentTableBlock: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var caption: String
    public var rows: [[String]]
    public var hasHeaderRow: Bool
    public var cellSpans: [TableCellSpan]
    public var borderStyle: TableCellBorderStyle

    public init(
        id: UUID = UUID(),
        caption: String = "",
        rows: [[String]],
        hasHeaderRow: Bool = true,
        cellSpans: [TableCellSpan] = [],
        borderStyle: TableCellBorderStyle = .solid
    ) {
        self.id = id
        self.caption = caption
        self.rows = rows
        self.hasHeaderRow = hasHeaderRow
        self.cellSpans = cellSpans
        self.borderStyle = borderStyle
    }

    public var rowCount: Int { rows.count }
    public var columnCount: Int { rows.first?.count ?? 0 }

    /// Merges a rectangular block of cells starting at `(startRow, startCol)`.
    public mutating func mergeCells(startRow: Int, startCol: Int, rowSpan: Int, colSpan: Int) {
        guard startRow >= 0, startCol >= 0, rowSpan >= 1, colSpan >= 1, (rowSpan > 1 || colSpan > 1) else {
            return
        }
        let (endRow, rowOverflow) = startRow.addingReportingOverflow(rowSpan)
        let (endCol, colOverflow) = startCol.addingReportingOverflow(colSpan)
        guard !rowOverflow, !colOverflow,
              endRow <= rows.count,
              (startRow..<endRow).allSatisfy({ endCol <= rows[$0].count }) else {
            return
        }
        cellSpans.removeAll { span in
            let spanEndRow = span.rowIndex + span.rowSpan
            let spanEndCol = span.columnIndex + span.columnSpan
            let rowsOverlap = max(startRow, span.rowIndex) < min(endRow, spanEndRow)
            let colsOverlap = max(startCol, span.columnIndex) < min(endCol, spanEndCol)
            return rowsOverlap && colsOverlap
        }
        cellSpans.append(
            TableCellSpan(
                rowIndex: startRow,
                columnIndex: startCol,
                rowSpan: rowSpan,
                columnSpan: colSpan,
                borderStyle: borderStyle
            )
        )
    }

    /// Splits any merged cell covering `(row, col)` back into individual cells.
    public mutating func splitCell(row: Int, col: Int) {
        cellSpans.removeAll { span in
            row >= span.rowIndex && row < (span.rowIndex + span.rowSpan) &&
            col >= span.columnIndex && col < (span.columnIndex + span.columnSpan)
        }
    }

    /// Evaluates inline table formulas (`=SUM(ABOVE)`, `=AVERAGE(ABOVE)`, `=SUM(LEFT)`, `=AVERAGE(LEFT)`, `=COUNT(ABOVE)`, `=MAX(ABOVE)`, `=MIN(ABOVE)`).
    public func evaluateTableFormula(_ expression: String, atRow targetRow: Int, column targetCol: Int) -> Double? {
        var cleaned = expression.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
        if cleaned.hasPrefix("=") {
            cleaned.removeFirst()
            cleaned = cleaned.trimmingCharacters(in: .whitespaces)
        }

        guard let openParen = cleaned.firstIndex(of: "("),
              cleaned.hasSuffix(")") else {
            return nil
        }

        let funcName = String(cleaned[..<openParen]).trimmingCharacters(in: .whitespaces)
        let dirStart = cleaned.index(after: openParen)
        let dirEnd = cleaned.index(before: cleaned.endIndex)
        guard dirStart <= dirEnd else { return nil }
        let direction = String(cleaned[dirStart..<dirEnd]).trimmingCharacters(in: .whitespaces)

        var numbers: [Double] = []
        switch direction {
        case "ABOVE":
            let upperRowBound = min(max(0, targetRow), rows.count)
            for r in 0..<upperRowBound {
                guard targetCol >= 0, targetCol < rows[r].count else { continue }
                if let val = Self.parseNumericCell(rows[r][targetCol]) {
                    numbers.append(val)
                }
            }
        case "LEFT":
            guard targetRow >= 0, targetRow < rows.count else { return nil }
            let upperColBound = min(max(0, targetCol), rows[targetRow].count)
            for c in 0..<upperColBound {
                if let val = Self.parseNumericCell(rows[targetRow][c]) {
                    numbers.append(val)
                }
            }
        default:
            return nil
        }

        switch funcName {
        case "SUM":
            return numbers.reduce(0.0, +)
        case "AVERAGE":
            guard !numbers.isEmpty else { return nil }
            return numbers.reduce(0.0, +) / Double(numbers.count)
        case "COUNT":
            return Double(numbers.count)
        case "MAX":
            return numbers.max()
        case "MIN":
            return numbers.min()
        default:
            return nil
        }
    }

    private static func parseNumericCell(_ raw: String) -> Double? {
        let stripped = raw
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: ",", with: "")
        return Double(stripped)
    }
}

/// Footnote or endnote item anchored to a paragraph.
public struct FootnoteItem: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var number: Int
    public var isEndnote: Bool
    public var anchorParagraphID: UUID
    public var text: String

    public init(
        id: UUID = UUID(),
        number: Int,
        isEndnote: Bool = false,
        anchorParagraphID: UUID,
        text: String
    ) {
        self.id = id
        self.number = number
        self.isEndnote = isEndnote
        self.anchorParagraphID = anchorParagraphID
        self.text = text
    }
}

/// Revision type recorded when Track Changes is enabled.
public enum TrackedRevisionKind: String, CaseIterable, Sendable, Codable {
    case insertion
    case deletion
    case formattingChange
}

/// Tracked revision entry for document review workflows.
public struct TrackedRevision: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var author: String
    public var kind: TrackedRevisionKind
    public var paragraphID: UUID
    public var originalText: String
    public var proposedText: String
    public var timestampISO8601: String

    public init(
        id: UUID = UUID(),
        author: String,
        kind: TrackedRevisionKind,
        paragraphID: UUID,
        originalText: String,
        proposedText: String,
        timestampISO8601: String = currentISO8601Timestamp()
    ) {
        self.id = id
        self.author = author
        self.kind = kind
        self.paragraphID = paragraphID
        self.originalText = originalText
        self.proposedText = proposedText
        self.timestampISO8601 = timestampISO8601
    }
}

/// Review comment thread anchored to a paragraph.
public struct DocumentCommentThread: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var anchorParagraphID: UUID
    public var author: String
    public var message: String
    public var replies: [String]
    public var isResolved: Bool

    public init(
        id: UUID = UUID(),
        anchorParagraphID: UUID,
        author: String,
        message: String,
        replies: [String] = [],
        isResolved: Bool = false
    ) {
        self.id = id
        self.anchorParagraphID = anchorParagraphID
        self.author = author
        self.message = message
        self.replies = replies
        self.isResolved = isResolved
    }
}

/// Generated Table of Contents entry.
public struct TableOfContentsEntry: Sendable, Codable, Equatable {
    public let paragraphID: UUID
    public let title: String
    public let level: Int

    public init(paragraphID: UUID, title: String, level: Int) {
        self.paragraphID = paragraphID
        self.title = title
        self.level = level
    }
}

/// Embedded image attachment stored in a document container (`word/media/`, `Pictures/`, `BinData/`).
public struct EmbeddedImageAttachment: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var fileName: String
    public var caption: String
    public var mimeType: String
    public var imageData: Data
    public var widthPt: Double
    public var heightPt: Double
    public var anchorParagraphID: UUID?

    public init(
        id: UUID = UUID(),
        fileName: String,
        caption: String = "",
        mimeType: String = "image/png",
        imageData: Data,
        widthPt: Double = 320.0,
        heightPt: Double = 200.0,
        anchorParagraphID: UUID? = nil
    ) {
        self.id = id
        self.fileName = fileName
        self.caption = caption
        self.mimeType = mimeType
        self.imageData = imageData
        self.widthPt = widthPt
        self.heightPt = heightPt
        self.anchorParagraphID = anchorParagraphID
    }
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
    public var sections: [DocumentSectionLayout]
    public var footnotes: [FootnoteItem]
    public var revisions: [TrackedRevision]
    public var comments: [DocumentCommentThread]
    public var bookmarks: [String: UUID]
    public var images: [EmbeddedImageAttachment]

    public init(
        id: UUID = UUID(),
        title: String,
        format: DocumentFormat = .docx,
        paperSize: PaperSizePreset = .a4,
        headerText: String = "",
        footerText: String = "",
        paragraphs: [ParagraphBlock] = [],
        tables: [DocumentTableBlock] = [],
        trackChangesEnabled: Bool = false,
        sections: [DocumentSectionLayout] = [DocumentSectionLayout()],
        footnotes: [FootnoteItem] = [],
        revisions: [TrackedRevision] = [],
        comments: [DocumentCommentThread] = [],
        bookmarks: [String: UUID] = [:],
        images: [EmbeddedImageAttachment] = []
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
        self.sections = sections.isEmpty ? [DocumentSectionLayout(paperSize: paperSize)] : sections
        self.footnotes = footnotes
        self.revisions = revisions
        self.comments = comments
        self.bookmarks = bookmarks
        self.images = images
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

    /// Generates a hierarchical Table of Contents from heading styles.
    public func generateTableOfContents() -> [TableOfContentsEntry] {
        paragraphs.compactMap { block in
            switch block.style {
            case .title, .heading1, .officialHeader:
                return TableOfContentsEntry(paragraphID: block.id, title: block.text, level: 1)
            case .heading2:
                return TableOfContentsEntry(paragraphID: block.id, title: block.text, level: 2)
            case .heading3:
                return TableOfContentsEntry(paragraphID: block.id, title: block.text, level: 3)
            case .body, .quote:
                return nil
            }
        }
    }

    /// Records a tracked revision when `trackChangesEnabled` is active, or applies the change immediately if disabled.
    public mutating func recordRevision(
        author: String,
        kind: TrackedRevisionKind,
        paragraphID: UUID,
        proposedText: String,
        timestampISO8601: String = currentISO8601Timestamp()
    ) {
        let original = paragraphs.first(where: { $0.id == paragraphID })?.text ?? ""
        if trackChangesEnabled {
            let revision = TrackedRevision(
                author: author,
                kind: kind,
                paragraphID: paragraphID,
                originalText: original,
                proposedText: proposedText,
                timestampISO8601: timestampISO8601
            )
            revisions.append(revision)
        } else {
            if let idx = paragraphs.firstIndex(where: { $0.id == paragraphID }) {
                paragraphs[idx].text = proposedText
                paragraphs[idx].runs.removeAll()
            }
        }
    }

    /// Accepts a pending tracked revision and applies its proposed text to the target paragraph.
    public mutating func acceptRevision(id: UUID) {
        guard let revIndex = revisions.firstIndex(where: { $0.id == id }) else { return }
        let revision = revisions.remove(at: revIndex)
        if let pIndex = paragraphs.firstIndex(where: { $0.id == revision.paragraphID }) {
            paragraphs[pIndex].text = revision.proposedText
            paragraphs[pIndex].runs.removeAll()
        } else if revision.kind == .insertion {
            paragraphs.append(ParagraphBlock(id: revision.paragraphID, text: revision.proposedText))
        }
    }

    /// Rejects a pending tracked revision and restores the original paragraph text.
    public mutating func rejectRevision(id: UUID) {
        guard let revIndex = revisions.firstIndex(where: { $0.id == id }) else { return }
        let revision = revisions.remove(at: revIndex)
        if let pIndex = paragraphs.firstIndex(where: { $0.id == revision.paragraphID }) {
            paragraphs[pIndex].text = revision.originalText
            paragraphs[pIndex].runs.removeAll()
        }
    }

    /// Performs find-and-replace across paragraphs and table cells, returning the updated document and replacement count.
    public func findAndReplace(
        search: String,
        replacement: String,
        caseSensitive: Bool = true
    ) -> (updated: OfficeDocumentModel, replacementCount: Int) {
        guard !search.isEmpty else { return (self, 0) }
        let options: String.CompareOptions = caseSensitive ? [] : [.caseInsensitive]
        var totalCount = 0

        func replaceAll(in text: String, countReplacements: Bool = true) -> String {
            var result = ""
            var currentIndex = text.startIndex
            while let range = text.range(of: search, options: options, range: currentIndex..<text.endIndex) {
                result += text[currentIndex..<range.lowerBound]
                result += replacement
                if countReplacements {
                    totalCount += 1
                }
                currentIndex = range.upperBound
            }
            result += text[currentIndex..<text.endIndex]
            return result
        }

        var copy = self
        copy.paragraphs = copy.paragraphs.map { block in
            var updated = block
            updated.text = replaceAll(in: block.text, countReplacements: true)
            if !updated.runs.isEmpty {
                updated.runs = updated.runs.map { run in
                    var r = run
                    r.text = replaceAll(in: run.text, countReplacements: false)
                    return r
                }
                if updated.runs.map(\.text).joined() != updated.text {
                    updated.runs.removeAll()
                }
            }
            return updated
        }
        copy.tables = copy.tables.map { table in
            var updatedTable = table
            updatedTable.caption = replaceAll(in: table.caption, countReplacements: true)
            updatedTable.rows = table.rows.map { row in
                row.map { replaceAll(in: $0, countReplacements: true) }
            }
            return updatedTable
        }
        return (copy, totalCount)
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
            if !updated.runs.isEmpty {
                updated.runs = updated.runs.map { run in
                    var r = run
                    r.text = replacePlaceholders(in: run.text)
                    return r
                }
                if updated.runs.map(\.text).joined() != updated.text {
                    updated.runs.removeAll()
                }
            }
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
