import Foundation

// MARK: - A. Bidirectional Format Conversion Studio


/// Single format conversion request across any supported office module.
public struct ConversionJobRequest: Sendable, Equatable, Identifiable {
    public let id: UUID
    public var sourceFileName: String
    public var sourceFormat: DocumentFormat
    public var targetFormat: DocumentFormat
    public var payload: OpenTabPayload

    public init(
        id: UUID = UUID(),
        sourceFileName: String,
        sourceFormat: DocumentFormat,
        targetFormat: DocumentFormat,
        payload: OpenTabPayload
    ) {
        self.id = id
        self.sourceFileName = sourceFileName
        self.sourceFormat = sourceFormat
        self.targetFormat = targetFormat
        self.payload = payload
    }
}

/// Verified result of a single format conversion job.
public struct ConversionJobResult: Sendable, Equatable, Identifiable {
    public let id: UUID
    public let sourceFileName: String
    public let outputFileName: String
    public let sourceFormat: DocumentFormat
    public let targetFormat: DocumentFormat
    public let outputBytes: Data
    public let convertedPayload: OpenTabPayload
    public let verifiedSignature: VerifiedDocumentSignature

    public init(
        id: UUID,
        sourceFileName: String,
        outputFileName: String,
        sourceFormat: DocumentFormat,
        targetFormat: DocumentFormat,
        outputBytes: Data,
        convertedPayload: OpenTabPayload,
        verifiedSignature: VerifiedDocumentSignature
    ) {
        self.id = id
        self.sourceFileName = sourceFileName
        self.outputFileName = outputFileName
        self.sourceFormat = sourceFormat
        self.targetFormat = targetFormat
        self.outputBytes = outputBytes
        self.convertedPayload = convertedPayload
        self.verifiedSignature = verifiedSignature
    }
}

/// Summary report returned after executing a batch of format conversion jobs.
public struct BatchConversionReport: Sendable {
    public let totalJobs: Int
    public let succeededResults: [ConversionJobResult]
    public let failedErrors: [(jobID: UUID, errorDescription: String)]

    public init(
        totalJobs: Int,
        succeededResults: [ConversionJobResult],
        failedErrors: [(jobID: UUID, errorDescription: String)]
    ) {
        self.totalJobs = totalJobs
        self.succeededResults = succeededResults
        self.failedErrors = failedErrors
    }
}

/// Bidirectional format conversion studio orchestrating document, sheet, slide, HWP/HWPX, and PDF codecs with Fail-Closed signature verification.
public enum DocumentConversionOrchestrator {

    public static func convert(_ request: ConversionJobRequest) throws -> ConversionJobResult {
        if request.sourceFileName.utf8.contains(0x00) {
            throw EasyOfficeError.securityViolation(detail: "변환 파일명에 NUL 바이트가 포함되어 있습니다.")
        }
        let trimmedSourceName = request.sourceFileName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedSourceName.isEmpty else {
            throw EasyOfficeError.unsupportedExtension(request.sourceFileName)
        }
        if trimmedSourceName.contains("/")
            || trimmedSourceName.contains("\\")
            || trimmedSourceName.contains("..") {
            throw EasyOfficeError.securityViolation(detail: "변환 원본 파일명에 경로 구분자 또는 상위 디렉터리 참조가 포함되어 있습니다: \(request.sourceFileName)")
        }

        let baseName: String
        if let dotIdx = trimmedSourceName.lastIndex(of: "."), dotIdx > trimmedSourceName.startIndex {
            baseName = String(trimmedSourceName[..<dotIdx])
        } else {
            baseName = trimmedSourceName
        }
        guard !baseName.isEmpty, baseName != "." else {
            throw EasyOfficeError.unsupportedExtension(request.sourceFileName)
        }
        let outputFileName = "\(baseName).\(request.targetFormat.fileExtension)"

        let convertedPayload: OpenTabPayload
        let outputBytes: Data

        switch request.targetFormat {
        case .docx, .doc, .odt, .rtf, .md, .txt, .hwp, .hwpx, .hwt, .hwtx:
            let docModel = convertPayloadToDocument(request.payload, targetFormat: request.targetFormat)
            convertedPayload = .document(docModel)
            outputBytes = try encodeDocumentModel(docModel, as: request.targetFormat)

        case .xlsx, .xls, .ods, .csv, .tsv:
            let sheetModel = try convertPayloadToWorksheet(request.payload)
            convertedPayload = .sheet(sheetModel)
            outputBytes = try encodeWorksheetModel(sheetModel, as: request.targetFormat)

        case .pptx, .ppt, .ppsx, .odp:
            let deckModel = convertPayloadToPresentation(request.payload)
            convertedPayload = .slide(deckModel)
            outputBytes = try encodePresentationModel(deckModel, as: request.targetFormat)

        case .pdf:
            let pdfModel = convertPayloadToPDF(request.payload)
            convertedPayload = .pdf(pdfModel)
            outputBytes = PDFBinarySerializer.encode(pdfModel)
        }

        let verifiedSignature = try FormatDetector.verify(
            data: outputBytes,
            fileName: outputFileName
        )

        return ConversionJobResult(
            id: request.id,
            sourceFileName: request.sourceFileName,
            outputFileName: outputFileName,
            sourceFormat: request.sourceFormat,
            targetFormat: request.targetFormat,
            outputBytes: outputBytes,
            convertedPayload: convertedPayload,
            verifiedSignature: verifiedSignature
        )
    }

    public static func executeBatch(_ jobs: [ConversionJobRequest]) -> BatchConversionReport {
        var succeeded: [ConversionJobResult] = []
        var failed: [(jobID: UUID, errorDescription: String)] = []

        for job in jobs {
            do {
                let result = try convert(job)
                succeeded.append(result)
            } catch {
                let desc = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
                failed.append((jobID: job.id, errorDescription: desc))
            }
        }

        return BatchConversionReport(
            totalJobs: jobs.count,
            succeededResults: succeeded,
            failedErrors: failed
        )
    }

    // MARK: Cross-Domain Model Adapters

    private static func convertPayloadToDocument(
        _ payload: OpenTabPayload,
        targetFormat: DocumentFormat
    ) -> OfficeDocumentModel {
        switch payload {
        case var .document(doc):
            doc.format = targetFormat
            return doc

        case let .pdf(pdf):
            return pdf.convertToEditableDocument(targetFormat: targetFormat)

        case let .sheet(sheet):
            let rows = extractGridRows(from: sheet)
            var tables: [DocumentTableBlock] = []
            if !rows.isEmpty {
                tables.append(DocumentTableBlock(caption: sheet.name, rows: rows, hasHeaderRow: true))
            }
            return OfficeDocumentModel(
                title: sheet.name,
                format: targetFormat,
                paragraphs: [ParagraphBlock(style: .heading1, text: sheet.name, isBold: true)],
                tables: tables
            )

        case let .slide(deck):
            var paragraphs: [ParagraphBlock] = []
            for slide in deck.slides {
                paragraphs.append(ParagraphBlock(style: .heading1, text: slide.title, isBold: true))
                if !slide.subtitle.isEmpty {
                    paragraphs.append(ParagraphBlock(style: .heading2, text: slide.subtitle))
                }
                for bullet in slide.bulletPoints {
                    paragraphs.append(ParagraphBlock(style: .body, text: bullet))
                }
            }
            return OfficeDocumentModel(
                title: deck.title,
                format: targetFormat,
                paragraphs: paragraphs
            )
        }
    }

    private static func convertPayloadToWorksheet(_ payload: OpenTabPayload) throws -> WorksheetModel {
        switch payload {
        case let .sheet(sheet):
            return sheet

        case let .document(doc):
            var sheet = WorksheetModel(name: doc.title)
            if let firstTable = doc.tables.first, !firstTable.rows.isEmpty {
                for (rIdx, row) in firstTable.rows.enumerated() {
                    for (cIdx, cellText) in row.enumerated() where !cellText.isEmpty {
                        let addr = try CellAddress(column: cIdx, row: rIdx)
                        try sheet.setCell(addr.a1Notation, rawValue: cellText)
                    }
                }
            } else {
                for (rIdx, p) in doc.paragraphs.enumerated() where !p.text.isEmpty {
                    let addr = try CellAddress(column: 0, row: rIdx)
                    try sheet.setCell(addr.a1Notation, rawValue: p.text)
                }
            }
            return sheet

        case let .pdf(pdf):
            var sheet = WorksheetModel(name: pdf.title)
            var currentRow = 0
            for page in pdf.pages {
                let lines = page.extractedText.components(separatedBy: .newlines)
                for line in lines where !line.isEmpty {
                    let columns = line.components(separatedBy: "\t")
                    for (cIdx, colText) in columns.enumerated() where !colText.isEmpty {
                        let addr = try CellAddress(column: cIdx, row: currentRow)
                        try sheet.setCell(addr.a1Notation, rawValue: colText)
                    }
                    currentRow += 1
                }
            }
            return sheet

        case let .slide(deck):
            var sheet = WorksheetModel(name: deck.title)
            for (rIdx, slide) in deck.slides.enumerated() {
                let titleAddr = try CellAddress(column: 0, row: rIdx)
                try sheet.setCell(titleAddr.a1Notation, rawValue: slide.title)
                let bodyText = slide.bulletPoints.joined(separator: "; ")
                if !bodyText.isEmpty {
                    let bodyAddr = try CellAddress(column: 1, row: rIdx)
                    try sheet.setCell(bodyAddr.a1Notation, rawValue: bodyText)
                }
            }
            return sheet
        }
    }

    private static func convertPayloadToPresentation(_ payload: OpenTabPayload) -> PresentationDeckModel {
        switch payload {
        case let .slide(deck):
            return deck

        case let .pdf(pdf):
            return pdf.convertToPresentation()

        case let .document(doc):
            let slides: [SlideModel]
            if doc.paragraphs.isEmpty {
                slides = [SlideModel(title: doc.title, masterLayout: .titleSlide)]
            } else {
                let bullets = doc.paragraphs.dropFirst().map(\.text)
                slides = [
                    SlideModel(
                        title: doc.paragraphs.first?.text ?? doc.title,
                        subtitle: doc.headerText,
                        bulletPoints: bullets,
                        masterLayout: bullets.isEmpty ? .titleSlide : .titleAndContent
                    )
                ]
            }
            return PresentationDeckModel(title: doc.title, slides: slides)

        case let .sheet(sheet):
            let rows = extractGridRows(from: sheet)
            let bullets = rows.map { $0.joined(separator: " | ") }
            return PresentationDeckModel(
                title: sheet.name,
                slides: [
                    SlideModel(
                        title: sheet.name,
                        bulletPoints: bullets,
                        masterLayout: .titleAndContent
                    )
                ]
            )
        }
    }

    private static func convertPayloadToPDF(_ payload: OpenTabPayload) -> PDFStudioDocumentModel {
        switch payload {
        case let .pdf(pdf):
            return pdf
        case let .document(doc):
            return PDFStudioDocumentModel.fromDocument(doc)
        case let .sheet(sheet):
            let range = boundingRange(for: sheet)
            return PDFStudioDocumentModel.fromWorksheet(sheet, range: range)
        case let .slide(deck):
            return PDFStudioDocumentModel.fromPresentation(deck)
        }
    }

    private static func extractGridRows(from sheet: WorksheetModel) -> [[String]] {
        var maxRow = -1
        var maxCol = -1
        for coord in sheet.cellsByCoordinate.keys {
            if coord.row < CSVStateMachineCodec.maxExportRows,
               coord.column < CSVStateMachineCodec.maxExportColumns {
                maxRow = max(maxRow, coord.row)
                maxCol = max(maxCol, coord.column)
            }
        }
        guard maxRow >= 0, maxCol >= 0 else { return [] }
        var rows: [[String]] = []
        rows.reserveCapacity(maxRow + 1)
        for r in 0...maxRow {
            var rowCells: [String] = []
            rowCells.reserveCapacity(maxCol + 1)
            for c in 0...maxCol {
                let packed = PackedCellCoordinate(row: r, column: c)
                rowCells.append(sheet.evaluate(at: packed).description)
            }
            rows.append(rowCells)
        }
        return rows
    }

    private static func boundingRange(for sheet: WorksheetModel) -> CellRange {
        var maxRow = 0
        var maxCol = 0
        for coord in sheet.cellsByCoordinate.keys {
            maxRow = max(maxRow, min(coord.row, 200))
            maxCol = max(maxCol, min(coord.column, 50))
        }
        let start = (try? CellAddress(column: 0, row: 0)) ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
        let end = (try? CellAddress(column: maxCol, row: maxRow)) ?? start
        return CellRange(start: start, end: end)
    }

    // MARK: Format Encoders

    private static func encodeDocumentModel(_ doc: OfficeDocumentModel, as format: DocumentFormat) throws -> Data {
        switch format {
        case .docx:
            return try OOXMLContainerCodec.encodeDocument(doc)
        case .odt:
            return try ODFContainerCodec.encodeDocument(doc)
        case .hwpx, .hwtx:
            return try HWPXContainerCodec.encodeDocument(doc)
        case .hwp, .hwt:
            return try HWP5BinaryCodec.encodeDocument(doc)
        case .doc:
            return try LegacyOLE2OfficeCodec.encodeWordBinary(doc)
        case .rtf:
            return RTFCodec.encodeDocument(doc)
        case .md:
            return CommonMarkCodec.encodeDocument(doc)
        case .txt:
            var lines: [String] = []
            if !doc.title.isEmpty {
                lines.append(doc.title)
            }
            for p in doc.paragraphs where !p.text.isEmpty {
                lines.append(p.text)
            }
            for tbl in doc.tables {
                if !tbl.caption.isEmpty {
                    lines.append(tbl.caption)
                }
                for row in tbl.rows {
                    lines.append(row.joined(separator: "\t"))
                }
            }
            return Data(lines.joined(separator: "\n").utf8)
        default:
            throw EasyOfficeError.unsupportedExtension(format.rawValue)
        }
    }

    private static func encodeWorksheetModel(_ sheet: WorksheetModel, as format: DocumentFormat) throws -> Data {
        switch format {
        case .xlsx:
            return try OOXMLContainerCodec.encodeWorksheet(sheet)
        case .ods:
            return try ODFContainerCodec.encodeWorksheet(sheet)
        case .csv:
            return CSVStateMachineCodec.encodeWorksheet(sheet, delimiter: ",")
        case .tsv:
            return CSVStateMachineCodec.encodeWorksheet(sheet, delimiter: "\t")
        case .xls:
            return try LegacyOLE2OfficeCodec.encodeBIFF8Worksheet(sheet)
        default:
            throw EasyOfficeError.unsupportedExtension(format.rawValue)
        }
    }

    private static func encodePresentationModel(_ deck: PresentationDeckModel, as format: DocumentFormat) throws -> Data {
        switch format {
        case .pptx, .ppsx:
            return try OOXMLContainerCodec.encodePresentation(deck)
        case .odp:
            return try ODFContainerCodec.encodePresentation(deck)
        case .ppt:
            return try LegacyOLE2OfficeCodec.encodePowerPointBinary(deck)
        default:
            throw EasyOfficeError.unsupportedExtension(format.rawValue)
        }
    }
}

// MARK: - B. 100% Offline Local Smart Document Assistant

/// Extractive document summary report produced deterministically offline.
public struct DocumentSummaryReport: Sendable, Codable, Equatable {
    public let keySentences: [String]
    public let keywords: [String]
    public let estimatedReadingSeconds: Int
    public let compressionRatioPercent: Double

    public init(
        keySentences: [String],
        keywords: [String],
        estimatedReadingSeconds: Int,
        compressionRatioPercent: Double
    ) {
        self.keySentences = keySentences
        self.keywords = keywords
        self.estimatedReadingSeconds = estimatedReadingSeconds
        self.compressionRatioPercent = compressionRatioPercent
    }
}

/// Offline Korean/English writing refinement diagnostic item.
public struct WritingRefinementSuggestion: Sendable, Codable, Equatable, Identifiable {
    public var id: String { "\(paragraphID.uuidString)-\(issueKind)" }
    public let paragraphID: UUID
    public let issueKind: String
    public let originalSnippet: String
    public let suggestedRewrite: String

    public init(
        paragraphID: UUID,
        issueKind: String,
        originalSnippet: String,
        suggestedRewrite: String
    ) {
        self.paragraphID = paragraphID
        self.issueKind = issueKind
        self.originalSnippet = originalSnippet
        self.suggestedRewrite = suggestedRewrite
    }
}

/// Action item or domain entity extracted from a document paragraph.
public struct ExtractedActionItem: Sendable, Codable, Equatable, Identifiable {
    public var id: String { "\(sourceParagraphID.uuidString)-\(category)-\(extractedText)" }
    public let sourceParagraphID: UUID
    public let category: String
    public let extractedText: String

    public init(
        sourceParagraphID: UUID,
        category: String,
        extractedText: String
    ) {
        self.sourceParagraphID = sourceParagraphID
        self.category = category
        self.extractedText = extractedText
    }
}

/// Statistical and trend analysis report over a spreadsheet cell range.
public struct WorksheetInsightReport: Sendable, Codable, Equatable {
    public let numericCellCount: Int
    public let mean: Double
    public let standardDeviation: Double
    public let minValue: Double
    public let maxValue: Double
    public let outlierAddresses: [String]
    public let trendClassification: String
    public let growthRatePercent: Double

    public init(
        numericCellCount: Int,
        mean: Double,
        standardDeviation: Double,
        minValue: Double,
        maxValue: Double,
        outlierAddresses: [String],
        trendClassification: String,
        growthRatePercent: Double
    ) {
        self.numericCellCount = numericCellCount
        self.mean = mean
        self.standardDeviation = standardDeviation
        self.minValue = minValue
        self.maxValue = maxValue
        self.outlierAddresses = outlierAddresses
        self.trendClassification = trendClassification
        self.growthRatePercent = growthRatePercent
    }
}

/// Paragraph-level change classification for document comparison.
public enum DocumentDiffChangeKind: String, Sendable, Codable, Equatable {
    case unchanged
    case added
    case removed
    case modified
}

/// Single paragraph diff entry between a baseline and revised document.
public struct DocumentDiffEntry: Sendable, Codable, Equatable {
    public let kind: DocumentDiffChangeKind
    public let baselineText: String
    public let revisedText: String

    public init(kind: DocumentDiffChangeKind, baselineText: String, revisedText: String) {
        self.kind = kind
        self.baselineText = baselineText
        self.revisedText = revisedText
    }
}

/// Structured diff report produced by Longest Common Subsequence (LCS) comparison.
public struct DocumentDiffReport: Sendable, Codable, Equatable {
    public let entries: [DocumentDiffEntry]
    public let addedCount: Int
    public let removedCount: Int
    public let modifiedCount: Int
    public let unchangedCount: Int

    public init(
        entries: [DocumentDiffEntry],
        addedCount: Int,
        removedCount: Int,
        modifiedCount: Int,
        unchangedCount: Int
    ) {
        self.entries = entries
        self.addedCount = addedCount
        self.removedCount = removedCount
        self.modifiedCount = modifiedCount
        self.unchangedCount = unchangedCount
    }
}

/// 100% offline, deterministic local smart assistant for summarization, writing refinement, entity extraction, sheet insights, and LCS document comparison.
public enum LocalSmartDocumentAssistant {
    public static let maxDiffParagraphCount = 2_048

    private static let dateRegex = try! NSRegularExpression(
        pattern: #"(\d{4}-\d{2}-\d{2}|\d{4}년|\d{1,2}월\s*\d{1,2}일|마감|기한|까지\s*완료)"#,
        options: [.caseInsensitive]
    )
    private static let budgetRegex = try! NSRegularExpression(
        pattern: #"(\d[\d,]*\s*(원|만원|억원|조원|달러|USD|KRW)|예산|비용\s*편성)"#,
        options: [.caseInsensitive]
    )
    private static let metricRegex = try! NSRegularExpression(
        pattern: #"\d+(\.\d+)?\s*(%|배|건|명|ms|MB|GB|TB|초|페이지)"#,
        options: [.caseInsensitive]
    )
    private static let actionRegex = try! NSRegularExpression(
        pattern: #"(조치\s*항목|TODO|Action|제출\s*바람|확인\s*필요|검토\s*요망|필수\s*조치|회신\s*바람)"#,
        options: [.caseInsensitive]
    )

    private static let stopwords: Set<String> = [
        "및", "등", "수", "것", "이", "그", "저", "를", "을", "은", "는", "이", "가",
        "에", "에서", "으로", "로", "와", "과", "의", "도", "만", "있는", "없는", "하는",
        "합니다", "입니다", "있습니다", "대한", "위해", "통해", "따라", "관련", "경우",
        "the", "and", "for", "with", "that", "this", "from", "are", "was", "were"
    ]

    // MARK: 1. Extractive Summarization & Keyword Extraction

    public static func summarize(_ document: OfficeDocumentModel, maxSentences: Int = 3) -> DocumentSummaryReport {
        let clampedLimit = max(1, maxSentences)

        struct CandidateSentence {
            let index: Int
            let text: String
            let isHeading: Bool
        }

        var candidates: [CandidateSentence] = []
        for p in document.paragraphs {
            let isHeading = (p.style != .body && p.style != .quote)
            let parts = p.text
                .components(separatedBy: CharacterSet(charactersIn: "\n"))
                .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
                .filter { !$0.isEmpty }
            for part in parts {
                candidates.append(CandidateSentence(index: candidates.count, text: part, isHeading: isHeading))
            }
        }
        for table in document.tables where !table.caption.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            candidates.append(
                CandidateSentence(
                    index: candidates.count,
                    text: table.caption.trimmingCharacters(in: .whitespacesAndNewlines),
                    isHeading: false
                )
            )
        }

        let stats = document.computeStatistics()
        guard !candidates.isEmpty else {
            return DocumentSummaryReport(
                keySentences: [],
                keywords: [],
                estimatedReadingSeconds: 0,
                compressionRatioPercent: 0.0
            )
        }

        // Tokenize and compute term frequencies
        var termFrequency: [String: Int] = [:]
        var firstSeenOrder: [String: Int] = [:]
        var tokenCounter = 0

        for candidate in candidates {
            let tokens = tokenizeWords(in: candidate.text)
            for token in tokens {
                termFrequency[token, default: 0] += 1
                if firstSeenOrder[token] == nil {
                    firstSeenOrder[token] = tokenCounter
                    tokenCounter += 1
                }
            }
        }

        let sortedKeywords = termFrequency.keys.sorted { lhs, rhs in
            let f1 = termFrequency[lhs] ?? 0
            let f2 = termFrequency[rhs] ?? 0
            if f1 != f2 { return f1 > f2 }
            return (firstSeenOrder[lhs] ?? Int.max) < (firstSeenOrder[rhs] ?? Int.max)
        }
        let topKeywords = Array(sortedKeywords.prefix(6))

        // Score each candidate sentence deterministically
        let scored = candidates.map { candidate -> (candidate: CandidateSentence, score: Double) in
            let tokens = tokenizeWords(in: candidate.text)
            let tfSum = tokens.reduce(0.0) { acc, tok in
                acc + Double(termFrequency[tok] ?? 0)
            }
            let lengthNormalizer = max(1.0, sqrt(Double(max(1, tokens.count))))
            var score = tfSum / lengthNormalizer
            // Position salience bonus
            score += 2.0 / Double(candidate.index + 1)
            // Heading or numeric salience bonus
            if candidate.isHeading {
                score += 1.5
            }
            if candidate.text.unicodeScalars.contains(where: { CharacterSet.decimalDigits.contains($0) }) {
                score += 0.8
            }
            return (candidate, score)
        }

        let topSelected = scored
            .sorted { lhs, rhs in
                if abs(lhs.score - rhs.score) > 1e-9 {
                    return lhs.score > rhs.score
                }
                return lhs.candidate.index < rhs.candidate.index
            }
            .prefix(clampedLimit)
            .sorted { $0.candidate.index < $1.candidate.index }
            .map { $0.candidate.text }

        let totalChars = max(1, stats.characterCountWithSpaces)
        let summaryChars = topSelected.joined(separator: " ").unicodeScalars.count
        let rawRatio = (Double(summaryChars) / Double(totalChars)) * 100.0
        let compressionRatio = min(100.0, max(0.0, rawRatio))

        // Reading speed ~ 500 Korean chars/min (~8.33 chars/sec)
        let rawSeconds = ceil(Double(stats.characterCountWithSpaces) / 8.33)
        let readingSeconds = max(1, safeClampedInt(rawSeconds))

        return DocumentSummaryReport(
            keySentences: topSelected,
            keywords: topKeywords,
            estimatedReadingSeconds: readingSeconds,
            compressionRatioPercent: compressionRatio
        )
    }

    // MARK: 2. Writing Quality Diagnostics

    public static func analyzeWritingQuality(_ document: OfficeDocumentModel) -> [WritingRefinementSuggestion] {
        var suggestions: [WritingRefinementSuggestion] = []

        let passiveReplacements: [(pattern: String, replacement: String)] = [
            ("되어집니다", "됩니다"),
            ("되어지고", "되고"),
            ("되어지다", "되다"),
            ("되어진", "된"),
            ("보여집니다", "보입니다"),
            ("보여지다", "보이다"),
            ("불리워지다", "불리다"),
            ("생각되어집니다", "생각됩니다")
        ]

        let redundantReplacements: [(pattern: String, replacement: String)] = [
            ("매우 매우", "매우"),
            ("가장 최선의", "최선의"),
            ("다시 재검토", "재검토"),
            ("미리 사전", "사전"),
            ("과반수 이상", "과반수"),
            ("함께 공유", "공유")
        ]

        for paragraph in document.paragraphs {
            let text = paragraph.text
            guard !text.isEmpty else { continue }

            // 1. Long sentence splitting check ("장문 분할 권장")
            if text.count >= 80 {
                var splitRewrite = text
                if splitRewrite.contains("하며, ") {
                    splitRewrite = splitRewrite.replacingOccurrences(of: "하며, ", with: "합니다. ")
                } else if splitRewrite.contains("하고, ") {
                    splitRewrite = splitRewrite.replacingOccurrences(of: "하고, ", with: "합니다. ")
                } else if splitRewrite.contains("고, ") {
                    splitRewrite = splitRewrite.replacingOccurrences(of: "고, ", with: "습니다. ")
                } else {
                    splitRewrite = splitLongSentenceAtWordBoundary(text)
                }
                suggestions.append(
                    WritingRefinementSuggestion(
                        paragraphID: paragraph.id,
                        issueKind: "장문 분할 권장",
                        originalSnippet: text,
                        suggestedRewrite: splitRewrite
                    )
                )
            }

            // 2. Double passive / heavy passive check ("수동태/피동 표현 완화")
            var passiveRewrite = text
            var matchedPassive = false
            for item in passiveReplacements where passiveRewrite.contains(item.pattern) {
                matchedPassive = true
                passiveRewrite = passiveRewrite.replacingOccurrences(of: item.pattern, with: item.replacement)
            }
            if matchedPassive {
                suggestions.append(
                    WritingRefinementSuggestion(
                        paragraphID: paragraph.id,
                        issueKind: "수동태/피동 표현 완화",
                        originalSnippet: text,
                        suggestedRewrite: passiveRewrite
                    )
                )
            }

            // 3. Redundant spaces & filler modifiers ("중복 공백/수식어 정리")
            var redundantRewrite = text
            var matchedRedundant = false
            if redundantRewrite.contains("  ") {
                matchedRedundant = true
                while redundantRewrite.contains("  ") {
                    redundantRewrite = redundantRewrite.replacingOccurrences(of: "  ", with: " ")
                }
            }
            for item in redundantReplacements where redundantRewrite.contains(item.pattern) {
                matchedRedundant = true
                redundantRewrite = redundantRewrite.replacingOccurrences(of: item.pattern, with: item.replacement)
            }
            if matchedRedundant {
                suggestions.append(
                    WritingRefinementSuggestion(
                        paragraphID: paragraph.id,
                        issueKind: "중복 공백/수식어 정리",
                        originalSnippet: text,
                        suggestedRewrite: redundantRewrite
                    )
                )
            }
        }

        return suggestions
    }

    // MARK: 3. Action Item & Entity Extraction

    public static func extractActionItemsAndEntities(_ document: OfficeDocumentModel) -> [ExtractedActionItem] {
        var results: [ExtractedActionItem] = []

        for paragraph in document.paragraphs {
            let text = paragraph.text.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !text.isEmpty else { continue }

            // Category 1: 일정/기한
            if matchesRegex(dateRegex, in: text) {
                results.append(
                    ExtractedActionItem(
                        sourceParagraphID: paragraph.id,
                        category: "일정/기한",
                        extractedText: text
                    )
                )
            }

            // Category 2: 예산/금액
            if matchesRegex(budgetRegex, in: text) {
                results.append(
                    ExtractedActionItem(
                        sourceParagraphID: paragraph.id,
                        category: "예산/금액",
                        extractedText: text
                    )
                )
            }

            // Category 3: 핵심 수치
            if matchesRegex(metricRegex, in: text) {
                results.append(
                    ExtractedActionItem(
                        sourceParagraphID: paragraph.id,
                        category: "핵심 수치",
                        extractedText: text
                    )
                )
            }

            // Category 4: 조치 항목
            if matchesRegex(actionRegex, in: text) {
                results.append(
                    ExtractedActionItem(
                        sourceParagraphID: paragraph.id,
                        category: "조치 항목",
                        extractedText: text
                    )
                )
            }
        }

        return results
    }

    // MARK: 4. Worksheet Insights (Z-Score Outliers & Trend Classification)

    public static func analyzeWorksheetInsights(_ sheet: WorksheetModel, range: CellRange) -> WorksheetInsightReport {
        var numericEntries: [(address: String, value: Double)] = []
        for addr in range.expandedAddresses() {
            let evaluated = sheet.evaluate(at: addr.packedCoordinate)
            if case let .number(val) = evaluated, val.isFinite {
                numericEntries.append((address: addr.a1Notation, value: val))
            }
        }

        guard !numericEntries.isEmpty else {
            return WorksheetInsightReport(
                numericCellCount: 0,
                mean: 0.0,
                standardDeviation: 0.0,
                minValue: 0.0,
                maxValue: 0.0,
                outlierAddresses: [],
                trendClassification: "보합/안정",
                growthRatePercent: 0.0
            )
        }

        let count = numericEntries.count
        let values = numericEntries.map(\.value)
        let sum = values.reduce(0.0, +)
        let mean = sum / Double(count)

        let varianceSum = values.reduce(0.0) { acc, val in
            let diff = val - mean
            return acc + (diff * diff)
        }
        let stdDev = sqrt(varianceSum / Double(count))
        let minVal = values.min() ?? 0.0
        let maxVal = values.max() ?? 0.0

        var outliers: [String] = []
        if stdDev > 1e-12 {
            for entry in numericEntries {
                let zScore = abs(entry.value - mean) / stdDev
                if zScore >= 1.8 {
                    outliers.append(entry.address)
                }
            }
        }

        let firstVal = values.first ?? 0.0
        let lastVal = values.last ?? 0.0
        let growthRate: Double
        if abs(firstVal) > 1e-9 {
            growthRate = ((lastVal - firstVal) / abs(firstVal)) * 100.0
        } else {
            growthRate = 0.0
        }

        let trend: String
        if growthRate >= 5.0 {
            trend = "상승 추세"
        } else if growthRate <= -5.0 {
            trend = "하락 추세"
        } else {
            trend = "보합/안정"
        }

        return WorksheetInsightReport(
            numericCellCount: count,
            mean: mean,
            standardDeviation: stdDev,
            minValue: minVal,
            maxValue: maxVal,
            outlierAddresses: outliers,
            trendClassification: trend,
            growthRatePercent: growthRate
        )
    }

    // MARK: 5. Document Diff via Longest Common Subsequence (LCS)

    public static func compareDocuments(
        baseline: OfficeDocumentModel,
        revised: OfficeDocumentModel
    ) -> DocumentDiffReport {
        let baseTexts = Array(baseline.paragraphs.prefix(maxDiffParagraphCount).map(\.text))
        let revTexts = Array(revised.paragraphs.prefix(maxDiffParagraphCount).map(\.text))
        let m = baseTexts.count
        let n = revTexts.count

        // Build contiguous 1D DP matrix of size (m + 1) * (n + 1)
        let strideWidth = n + 1
        var dp = [Int32](repeating: 0, count: (m + 1) * strideWidth)
        if m > 0 && n > 0 {
            for i in stride(from: m - 1, through: 0, by: -1) {
                let rowOffset = i * strideWidth
                let nextRowOffset = (i + 1) * strideWidth
                for j in stride(from: n - 1, through: 0, by: -1) {
                    if baseTexts[i] == revTexts[j] {
                        dp[rowOffset + j] = 1 + dp[nextRowOffset + j + 1]
                    } else {
                        dp[rowOffset + j] = max(dp[nextRowOffset + j], dp[rowOffset + j + 1])
                    }
                }
            }
        }

        enum RawOp {
            case unchanged(String)
            case removed(String)
            case added(String)
        }

        var rawOps: [RawOp] = []
        var i = 0
        var j = 0
        while i < m && j < n {
            if baseTexts[i] == revTexts[j] {
                rawOps.append(.unchanged(baseTexts[i]))
                i += 1
                j += 1
            } else if dp[(i + 1) * strideWidth + j] >= dp[i * strideWidth + j + 1] {
                rawOps.append(.removed(baseTexts[i]))
                i += 1
            } else {
                rawOps.append(.added(revTexts[j]))
                j += 1
            }
        }
        while i < m {
            rawOps.append(.removed(baseTexts[i]))
            i += 1
        }
        while j < n {
            rawOps.append(.added(revTexts[j]))
            j += 1
        }

        // Coalesce adjacent removed + added blocks into .modified pairs
        var entries: [DocumentDiffEntry] = []
        var cursor = 0
        while cursor < rawOps.count {
            switch rawOps[cursor] {
            case let .unchanged(text):
                entries.append(DocumentDiffEntry(kind: .unchanged, baselineText: text, revisedText: text))
                cursor += 1

            case .removed:
                var removedBatch: [String] = []
                while cursor < rawOps.count, case let .removed(rText) = rawOps[cursor] {
                    removedBatch.append(rText)
                    cursor += 1
                }
                var addedBatch: [String] = []
                while cursor < rawOps.count, case let .added(aText) = rawOps[cursor] {
                    addedBatch.append(aText)
                    cursor += 1
                }
                let pairedCount = min(removedBatch.count, addedBatch.count)
                for idx in 0..<pairedCount {
                    entries.append(
                        DocumentDiffEntry(
                            kind: .modified,
                            baselineText: removedBatch[idx],
                            revisedText: addedBatch[idx]
                        )
                    )
                }
                if removedBatch.count > pairedCount {
                    for idx in pairedCount..<removedBatch.count {
                        entries.append(
                            DocumentDiffEntry(
                                kind: .removed,
                                baselineText: removedBatch[idx],
                                revisedText: ""
                            )
                        )
                    }
                }
                if addedBatch.count > pairedCount {
                    for idx in pairedCount..<addedBatch.count {
                        entries.append(
                            DocumentDiffEntry(
                                kind: .added,
                                baselineText: "",
                                revisedText: addedBatch[idx]
                            )
                        )
                    }
                }

            case let .added(text):
                entries.append(DocumentDiffEntry(kind: .added, baselineText: "", revisedText: text))
                cursor += 1
            }
        }

        let addedCount = entries.filter { $0.kind == .added }.count
        let removedCount = entries.filter { $0.kind == .removed }.count
        let modifiedCount = entries.filter { $0.kind == .modified }.count
        let unchangedCount = entries.filter { $0.kind == .unchanged }.count

        return DocumentDiffReport(
            entries: entries,
            addedCount: addedCount,
            removedCount: removedCount,
            modifiedCount: modifiedCount,
            unchangedCount: unchangedCount
        )
    }

    // MARK: Private Helpers

    private static func splitLongSentenceAtWordBoundary(_ text: String) -> String {
        let chars = Array(text)
        guard chars.count >= 2 else { return text }
        let mid = chars.count / 2

        // Prefer a whitespace index closest to the midpoint that is not immediately preceded by a comma
        var bestSpaceOffset: Int? = nil
        var fallbackCommaSpaceOffset: Int? = nil
        for delta in 0...mid {
            let left = mid - delta
            if left > 0 && left < chars.count - 1 && chars[left] == " " {
                if chars[left - 1] != "," {
                    bestSpaceOffset = left
                    break
                } else if fallbackCommaSpaceOffset == nil {
                    fallbackCommaSpaceOffset = left
                }
            }
            let right = mid + delta
            if right > 0 && right < chars.count - 1 && chars[right] == " " {
                if chars[right - 1] != "," {
                    bestSpaceOffset = right
                    break
                } else if fallbackCommaSpaceOffset == nil {
                    fallbackCommaSpaceOffset = right
                }
            }
        }

        if let splitAt = bestSpaceOffset ?? fallbackCommaSpaceOffset {
            let firstPart = String(chars[0..<splitAt]).trimmingCharacters(in: .whitespaces)
            let secondPart = String(chars[(splitAt + 1)...]).trimmingCharacters(in: .whitespaces)
            return "\(firstPart). \(secondPart)"
        }

        let midIndex = text.index(text.startIndex, offsetBy: mid)
        return String(text[..<midIndex]) + ". " + String(text[midIndex...])
    }

    private static func tokenizeWords(in text: String) -> [String] {
        let components = text.components(separatedBy: CharacterSet.alphanumerics.inverted)
        var tokens: [String] = []
        for raw in components {
            var cleaned = raw.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
            guard cleaned.count >= 2 else { continue }
            for suffix in ["으로", "은", "는", "이", "가", "을", "를", "에", "의", "와", "과", "도", "로"] {
                if cleaned.count > suffix.count + 1 && cleaned.hasSuffix(suffix) {
                    cleaned.removeLast(suffix.count)
                    break
                }
            }
            if cleaned.count >= 2 && !stopwords.contains(cleaned) {
                tokens.append(cleaned)
            }
        }
        return tokens
    }

    private static func matchesRegex(_ regex: NSRegularExpression, in text: String) -> Bool {
        let range = NSRange(text.startIndex..<text.endIndex, in: text)
        return regex.firstMatch(in: text, options: [], range: range) != nil
    }

    private static func safeClampedInt(_ value: Double) -> Int {
        guard value.isFinite else { return 0 }
        if value <= Double(Int.min) { return Int.min }
        if value >= Double(Int.max) { return Int.max }
        return Int(value)
    }
}

// MARK: - C. Container Security & Privacy Auditor

/// Comprehensive container security & privacy inspection report.
public struct SecurityAuditReport: Sendable, Codable, Equatable {
    public let fileName: String
    public let detectedFormat: DocumentFormat?
    public let isSignatureValid: Bool
    public let isSafeFromBombAndTraversal: Bool
    public let isSafeFromXXE: Bool
    public let isUnencrypted: Bool
    public let issues: [String]
    public let isPassed: Bool

    public init(
        fileName: String,
        detectedFormat: DocumentFormat?,
        isSignatureValid: Bool,
        isSafeFromBombAndTraversal: Bool,
        isSafeFromXXE: Bool,
        isUnencrypted: Bool,
        issues: [String],
        isPassed: Bool
    ) {
        self.fileName = fileName
        self.detectedFormat = detectedFormat
        self.isSignatureValid = isSignatureValid
        self.isSafeFromBombAndTraversal = isSafeFromBombAndTraversal
        self.isSafeFromXXE = isSafeFromXXE
        self.isUnencrypted = isUnencrypted
        self.issues = issues
        self.isPassed = isPassed
    }
}

/// Container security auditor and document metadata/PII scrubber.
public enum DocumentSecurityAuditor {

    /// Scrubs header/footer PII markers, anonymizes revision authors to `"Redacted"`, and removes all review comments.
    public static func scrubMetadata(
        from document: OfficeDocumentModel
    ) -> (scrubbed: OfficeDocumentModel, removedItemCount: Int) {
        var copy = document
        var removedCount = 0

        if !copy.headerText.isEmpty {
            copy.headerText = ""
            removedCount += 1
        }
        if !copy.footerText.isEmpty {
            copy.footerText = ""
            removedCount += 1
        }

        for idx in copy.revisions.indices {
            if copy.revisions[idx].author != "Redacted" {
                copy.revisions[idx].author = "Redacted"
                removedCount += 1
            }
        }

        if !copy.comments.isEmpty {
            removedCount += copy.comments.count
            copy.comments.removeAll()
        }

        return (copy, removedCount)
    }

    /// Audits a raw document container against magic-byte spoofing, ZIP decompression bombs, path traversal, XML external entities (XXE), and unsupported encryption.
    public static func auditContainer(data: Data, fileName: String) -> SecurityAuditReport {
        let detectedFormat = DocumentFormat.from(fileNameOrExtension: fileName)
        var isSignatureValid = true
        var isSafeFromBombAndTraversal = true
        var isSafeFromXXE = true
        var isUnencrypted = true
        var issues: [String] = []

        let verified: VerifiedDocumentSignature
        do {
            verified = try FormatDetector.verify(data: data, fileName: fileName)
        } catch {
            isSignatureValid = false
            if let officeError = error as? EasyOfficeError, case .corruptedContainer = officeError {
                isSafeFromBombAndTraversal = false
            }
            if (try? XMLSanitizer.validateSafeXMLBytes(data)) == nil {
                isSafeFromXXE = false
            }
            let desc = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
            issues.append(desc)
            return SecurityAuditReport(
                fileName: fileName,
                detectedFormat: detectedFormat,
                isSignatureValid: false,
                isSafeFromBombAndTraversal: isSafeFromBombAndTraversal,
                isSafeFromXXE: isSafeFromXXE,
                isUnencrypted: isUnencrypted,
                issues: issues,
                isPassed: false
            )
        }

        switch verified.containerFamily {
        case .zipXML:
            do {
                let entries = try ZipContainerCodec.unpack(from: data)
                for entry in entries {
                    let lowerPath = entry.path.lowercased()
                    let isXMLPayload = lowerPath.hasSuffix(".xml")
                        || lowerPath.hasSuffix(".rels")
                        || lowerPath.hasSuffix(".hpf")
                    do {
                        try XMLSanitizer.validateSafeXMLBytes(entry.data)
                    } catch {
                        isSafeFromXXE = false
                        let desc = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
                        issues.append(desc)
                    }
                    if isXMLPayload, String(data: entry.data, encoding: .utf8) == nil {
                        isSafeFromBombAndTraversal = false
                        issues.append("ZIP 내부 XML 엔트리가 유효한 UTF-8이 아닙니다: \(entry.path)")
                    }
                }
            } catch let error as EasyOfficeError {
                switch error {
                case .encryptedDocumentUnsupported:
                    isUnencrypted = false
                case .xxeOrExternalEntityDetected:
                    isSafeFromXXE = false
                default:
                    isSafeFromBombAndTraversal = false
                }
                issues.append(error.errorDescription ?? error.localizedDescription)
            } catch {
                isSafeFromBombAndTraversal = false
                issues.append(error.localizedDescription)
            }

        case .ole2Compound:
            do {
                let streams = try OLE2ContainerCodec.unpack(from: data)
                for stream in streams {
                    if stream.name == "EncryptionInfo" || stream.name == "EncryptedPackage" {
                        isUnencrypted = false
                        let err = EasyOfficeError.encryptedDocumentUnsupported(format: verified.format)
                        issues.append(err.errorDescription ?? "암호화된 OLE2 컨테이너입니다.")
                    }
                }
                if verified.format == .hwp || verified.format == .hwt {
                    guard let headerStream = streams.first(where: { $0.name == "FileHeader" }),
                          headerStream.data.count >= 40,
                          headerStream.data.starts(with: Data(HWP5BinaryCodec.signatureString.utf8)) else {
                        isSafeFromBombAndTraversal = false
                        issues.append("HWP 5.0 FileHeader 스트림이 누락되었거나 손상되었습니다.")
                        break
                    }
                    let flags = try BitPacking.readUInt32LE(from: headerStream.data, at: 36)
                    if (flags & (HWP5BinaryCodec.flagEncrypted | HWP5BinaryCodec.flagDistribution)) != 0 {
                        isUnencrypted = false
                        let err = EasyOfficeError.encryptedDocumentUnsupported(format: verified.format)
                        issues.append(err.errorDescription ?? "암호화된 문서입니다.")
                    }
                } else if verified.format == .doc {
                    _ = try LegacyOLE2OfficeCodec.decodeWordBinary(from: data)
                } else if verified.format == .xls {
                    _ = try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: data)
                } else if verified.format == .ppt {
                    _ = try LegacyOLE2OfficeCodec.decodePowerPointBinary(from: data)
                }
            } catch let error as EasyOfficeError {
                if case .encryptedDocumentUnsupported = error {
                    isUnencrypted = false
                } else {
                    isSafeFromBombAndTraversal = false
                }
                issues.append(error.errorDescription ?? error.localizedDescription)
            } catch {
                isSafeFromBombAndTraversal = false
                let desc = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
                issues.append(desc)
            }

        case .pdfBinary:
            let rawString = String(decoding: data, as: UTF8.self)
            if !rawString.contains("obj") || !rawString.contains("%%EOF") {
                isSafeFromBombAndTraversal = false
                issues.append("PDF 컨테이너에 유효한 객체(obj) 또는 %%EOF 트레일러가 없습니다.")
            }
            if rawString.contains("/Encrypt") {
                isUnencrypted = false
                let err = EasyOfficeError.encryptedDocumentUnsupported(format: .pdf)
                issues.append(err.errorDescription ?? "암호화된 PDF 문서입니다.")
            }

        case .rtfText, .plainText:
            do {
                try XMLSanitizer.validateSafeXMLBytes(data)
            } catch {
                isSafeFromXXE = false
                let desc = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
                issues.append(desc)
            }
            if String(data: data, encoding: .utf8) == nil || data.contains(0x00) {
                isSafeFromBombAndTraversal = false
                issues.append("텍스트/RTF 컨테이너에 유효하지 않은 UTF-8 또는 NUL 바이트가 포함되어 있습니다.")
            }
        }

        let isPassed = isSignatureValid
            && isSafeFromBombAndTraversal
            && isSafeFromXXE
            && isUnencrypted
            && issues.isEmpty

        return SecurityAuditReport(
            fileName: fileName,
            detectedFormat: verified.format,
            isSignatureValid: isSignatureValid,
            isSafeFromBombAndTraversal: isSafeFromBombAndTraversal,
            isSafeFromXXE: isSafeFromXXE,
            isUnencrypted: isUnencrypted,
            issues: issues,
            isPassed: isPassed
        )
    }
}

