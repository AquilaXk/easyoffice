import Foundation

// MARK: - 100% Offline Local Smart Document Assistant

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
            let tokens: [String]
        }

        var candidates: [CandidateSentence] = []
        for p in document.paragraphs {
            let isHeading = (p.style != .body && p.style != .quote)
            let parts = p.text
                .components(separatedBy: CharacterSet(charactersIn: "\n"))
                .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
                .filter { !$0.isEmpty }
            for part in parts {
                candidates.append(
                    CandidateSentence(
                        index: candidates.count,
                        text: part,
                        isHeading: isHeading,
                        tokens: tokenizeWords(in: part)
                    )
                )
            }
        }
        for table in document.tables {
            let trimmedCaption = table.caption.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmedCaption.isEmpty {
                candidates.append(
                    CandidateSentence(
                        index: candidates.count,
                        text: trimmedCaption,
                        isHeading: false,
                        tokens: tokenizeWords(in: trimmedCaption)
                    )
                )
            }
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

        // Compute term frequencies from pre-tokenized candidate sentences
        var termFrequency: [String: Int] = [:]
        var firstSeenOrder: [String: Int] = [:]
        var tokenCounter = 0

        for candidate in candidates {
            for token in candidate.tokens {
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

        // Score each candidate sentence deterministically using cached tokens
        let scored = candidates.map { candidate -> (candidate: CandidateSentence, score: Double) in
            let tokens = candidate.tokens
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
        let totalCount = text.count
        guard totalCount >= 2 else { return text }
        let mid = totalCount / 2
        let midIndex = text.index(text.startIndex, offsetBy: mid)
        let lastInteriorBound = text.index(before: text.endIndex)

        var bestSpaceIndex: String.Index? = nil
        var fallbackCommaSpaceIndex: String.Index? = nil
        var leftCursor: String.Index? = midIndex
        var rightCursor: String.Index? = midIndex

        for _ in 0...mid {
            if let left = leftCursor, left > text.startIndex, left < lastInteriorBound, text[left] == " " {
                if !isPrecededByComma(text, before: left) {
                    bestSpaceIndex = left
                    break
                } else if fallbackCommaSpaceIndex == nil {
                    fallbackCommaSpaceIndex = left
                }
            }
            if let right = rightCursor, right > text.startIndex, right < lastInteriorBound, text[right] == " " {
                if !isPrecededByComma(text, before: right) {
                    bestSpaceIndex = right
                    break
                } else if fallbackCommaSpaceIndex == nil {
                    fallbackCommaSpaceIndex = right
                }
            }
            if let left = leftCursor {
                leftCursor = left > text.startIndex ? text.index(before: left) : nil
            }
            if let right = rightCursor {
                let nextRight = text.index(after: right)
                rightCursor = nextRight < text.endIndex ? nextRight : nil
            }
        }

        if let splitAt = bestSpaceIndex ?? fallbackCommaSpaceIndex {
            let trimSet = CharacterSet.whitespaces.union(CharacterSet(charactersIn: ","))
            let firstPart = String(text[..<splitAt]).trimmingCharacters(in: trimSet)
            let afterSplit = text.index(after: splitAt)
            let secondPart = String(text[afterSplit...]).trimmingCharacters(in: .whitespaces)
            return "\(firstPart). \(secondPart)"
        }

        return String(text[..<midIndex]) + ". " + String(text[midIndex...])
    }

    private static func isPrecededByComma(_ text: String, before index: String.Index) -> Bool {
        var cursor = index
        while cursor > text.startIndex {
            text.formIndex(before: &cursor)
            let ch = text[cursor]
            if ch == " " {
                continue
            }
            return ch == ","
        }
        return false
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
