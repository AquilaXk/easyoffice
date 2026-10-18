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
    id: UUID = UUID(), paperSize: PaperSizePreset = .a4, orientation: PageOrientation = .portrait,
    columnCount: Int = 1, marginTopPt: Double = 56.7, marginBottomPt: Double = 56.7,
    marginLeftPt: Double = 56.7, marginRightPt: Double = 56.7
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
  public var isStrikethrough: Bool = false
  public var fontName: String?
  public var fontSizePt: Double?
  public var foregroundHex: String?
  public var backgroundHex: String? = nil
  public var linkURL: String? = nil

  public init(
    id: UUID = UUID(), text: String, isBold: Bool = false, isItalic: Bool = false,
    isUnderlined: Bool = false, isStrikethrough: Bool = false, fontName: String? = nil,
    fontSizePt: Double? = nil, foregroundHex: String? = nil, backgroundHex: String? = nil,
    linkURL: String? = nil
  ) {
    self.id = id
    self.text = text
    self.isBold = isBold
    self.isItalic = isItalic
    self.isUnderlined = isUnderlined
    self.isStrikethrough = isStrikethrough
    self.fontName = fontName
    self.fontSizePt = fontSizePt
    self.foregroundHex = foregroundHex
    self.backgroundHex = backgroundHex
    self.linkURL = linkURL
  }

  public init(
    id: UUID = UUID(), text: String, isBold: Bool = false, isItalic: Bool = false,
    isUnderlined: Bool = false, fontName: String? = nil, fontSizePt: Double? = nil,
    foregroundHex: String?, isStrikethrough: Bool, backgroundHex: String? = nil,
    linkURL: String? = nil
  ) {
    self.init(
      id: id, text: text, isBold: isBold, isItalic: isItalic, isUnderlined: isUnderlined,
      isStrikethrough: isStrikethrough, fontName: fontName, fontSizePt: fontSizePt,
      foregroundHex: foregroundHex, backgroundHex: backgroundHex, linkURL: linkURL
    )
  }

  private enum CodingKeys: String, CodingKey {
    case id, text, isBold, isItalic, isUnderlined, isStrikethrough
    case fontName, fontSizePt, foregroundHex, backgroundHex, linkURL
  }

  public init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    self.id = try c.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
    self.text = try c.decodeIfPresent(String.self, forKey: .text) ?? ""
    self.isBold = try c.decodeIfPresent(Bool.self, forKey: .isBold) ?? false
    self.isItalic = try c.decodeIfPresent(Bool.self, forKey: .isItalic) ?? false
    self.isUnderlined = try c.decodeIfPresent(Bool.self, forKey: .isUnderlined) ?? false
    self.isStrikethrough = try c.decodeIfPresent(Bool.self, forKey: .isStrikethrough) ?? false
    self.fontName = try c.decodeIfPresent(String.self, forKey: .fontName)
    self.fontSizePt = try c.decodeIfPresent(Double.self, forKey: .fontSizePt)
    self.foregroundHex = try c.decodeIfPresent(String.self, forKey: .foregroundHex)
    self.backgroundHex = try c.decodeIfPresent(String.self, forKey: .backgroundHex)
    self.linkURL = try c.decodeIfPresent(String.self, forKey: .linkURL)
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
  public var firstLineIndentPt: Double = 0.0
  public var leftIndentPt: Double = 0.0

  public init(
    id: UUID = UUID(), style: ParagraphStyleRole = .body, alignment: TextAlignmentMode = .justified,
    text: String, runs: [DocumentTextRun] = [], fontName: String = "Pretendard",
    fontSizePt: Double = 11.0,
    isBold: Bool = false, isItalic: Bool = false, isUnderlined: Bool = false,
    lineSpacingPercent: Double = 160.0, firstLineIndentPt: Double = 0.0, leftIndentPt: Double = 0.0
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
    self.firstLineIndentPt = firstLineIndentPt
    self.leftIndentPt = leftIndentPt
  }

  private enum CodingKeys: String, CodingKey {
    case id, style, alignment, text, runs, fontName, fontSizePt
    case isBold, isItalic, isUnderlined, lineSpacingPercent, firstLineIndentPt, leftIndentPt
  }

  public init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    self.id = try c.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
    self.style = try c.decodeIfPresent(ParagraphStyleRole.self, forKey: .style) ?? .body
    self.alignment = try c.decodeIfPresent(TextAlignmentMode.self, forKey: .alignment) ?? .justified
    self.text = try c.decodeIfPresent(String.self, forKey: .text) ?? ""
    self.runs = try c.decodeIfPresent([DocumentTextRun].self, forKey: .runs) ?? []
    self.fontName = try c.decodeIfPresent(String.self, forKey: .fontName) ?? "Pretendard"
    self.fontSizePt = try c.decodeIfPresent(Double.self, forKey: .fontSizePt) ?? 11.0
    self.isBold = try c.decodeIfPresent(Bool.self, forKey: .isBold) ?? false
    self.isItalic = try c.decodeIfPresent(Bool.self, forKey: .isItalic) ?? false
    self.isUnderlined = try c.decodeIfPresent(Bool.self, forKey: .isUnderlined) ?? false
    self.lineSpacingPercent =
      try c.decodeIfPresent(Double.self, forKey: .lineSpacingPercent) ?? 160.0
    self.firstLineIndentPt = try c.decodeIfPresent(Double.self, forKey: .firstLineIndentPt) ?? 0.0
    self.leftIndentPt = try c.decodeIfPresent(Double.self, forKey: .leftIndentPt) ?? 0.0
  }

  /// Returns explicit inline runs when present and synchronized with `text`, or a single synthesized run representing the paragraph's base style.
  public var effectiveRuns: [DocumentTextRun] {
    if !runs.isEmpty, runs.map(\.text).joined() == text { return runs }
    return [
      DocumentTextRun(
        id: id, text: text, isBold: isBold, isItalic: isItalic, isUnderlined: isUnderlined,
        fontName: fontName, fontSizePt: fontSizePt, foregroundHex: nil
      )
    ]
  }

  /// Splits and updates inline `runs` over `range` within `text` with the specified character attributes.
  public mutating func applyRunFormatting(
    range: Range<String.Index>? = nil, isBold: Bool? = nil, isItalic: Bool? = nil,
    isUnderlined: Bool? = nil, isStrikethrough: Bool? = nil, fontSizePt: Double? = nil,
    foregroundHex: String? = nil, backgroundHex: String? = nil, linkURL: String? = nil
  ) {
    if range == nil {
      if let isBold { self.isBold = isBold }
      if let isItalic { self.isItalic = isItalic }
      if let isUnderlined { self.isUnderlined = isUnderlined }
      if let fontSizePt { self.fontSizePt = fontSizePt }
    }
    let resolvedRange = range ?? (text.startIndex..<text.endIndex)
    let targetStart = text.distance(from: text.startIndex, to: resolvedRange.lowerBound)
    let targetEnd = text.distance(from: text.startIndex, to: resolvedRange.upperBound)
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
            isBold: run.isBold, isItalic: run.isItalic, isUnderlined: run.isUnderlined,
            isStrikethrough: run.isStrikethrough, fontName: run.fontName,
            fontSizePt: run.fontSizePt,
            foregroundHex: run.foregroundHex, backgroundHex: run.backgroundHex, linkURL: run.linkURL
          ))
      }

      let overlapStart = max(runStart, targetStart) - runStart
      let overlapEnd = min(runEnd, targetEnd) - runStart
      let midStartIdx = run.text.index(run.text.startIndex, offsetBy: overlapStart)
      let midEndIdx = run.text.index(run.text.startIndex, offsetBy: overlapEnd)
      splitRuns.append(
        DocumentTextRun(
          text: String(run.text[midStartIdx..<midEndIdx]),
          isBold: isBold ?? run.isBold, isItalic: isItalic ?? run.isItalic,
          isUnderlined: isUnderlined ?? run.isUnderlined,
          isStrikethrough: isStrikethrough ?? run.isStrikethrough,
          fontName: run.fontName, fontSizePt: fontSizePt ?? run.fontSizePt,
          foregroundHex: foregroundHex ?? run.foregroundHex,
          backgroundHex: backgroundHex ?? run.backgroundHex,
          linkURL: linkURL ?? run.linkURL
        ))

      if runEnd > targetEnd {
        let suffixStart = targetEnd - runStart
        let suffixStartIdx = run.text.index(run.text.startIndex, offsetBy: suffixStart)
        splitRuns.append(
          DocumentTextRun(
            text: String(run.text[suffixStartIdx..<run.text.endIndex]),
            isBold: run.isBold, isItalic: run.isItalic, isUnderlined: run.isUnderlined,
            isStrikethrough: run.isStrikethrough, fontName: run.fontName,
            fontSizePt: run.fontSizePt,
            foregroundHex: run.foregroundHex, backgroundHex: run.backgroundHex, linkURL: run.linkURL
          ))
      }
    }

    // Coalesce adjacent runs that share identical formatting attributes.
    var coalesced: [DocumentTextRun] = []
    for run in splitRuns where !run.text.isEmpty {
      if let last = coalesced.last,
        last.isBold == run.isBold, last.isItalic == run.isItalic,
        last.isUnderlined == run.isUnderlined, last.isStrikethrough == run.isStrikethrough,
        last.fontName == run.fontName, last.fontSizePt == run.fontSizePt,
        last.foregroundHex == run.foregroundHex, last.backgroundHex == run.backgroundHex,
        last.linkURL == run.linkURL
      {
        coalesced[coalesced.count - 1].text += run.text
      } else {
        coalesced.append(run)
      }
    }

    self.runs = coalesced
    self.text = coalesced.map(\.text).joined()
  }

  public mutating func applyRunFormatting(
    utf16Range: Range<Int>, isBold: Bool? = nil, isItalic: Bool? = nil,
    isUnderlined: Bool? = nil, isStrikethrough: Bool? = nil, fontSizePt: Double? = nil,
    foregroundHex: String? = nil, backgroundHex: String? = nil, linkURL: String? = nil
  ) {
    let utf16View = text.utf16
    let clampedLower = max(0, min(utf16Range.lowerBound, utf16View.count))
    let clampedUpper = max(clampedLower, min(utf16Range.upperBound, utf16View.count))
    guard clampedLower < clampedUpper else { return }
    let startIdx = String.Index(utf16Offset: clampedLower, in: text)
    let endIdx = String.Index(utf16Offset: clampedUpper, in: text)
    let alignedRange = text.rangeOfComposedCharacterSequences(for: startIdx..<endIdx)
    guard !alignedRange.isEmpty else { return }
    applyRunFormatting(
      range: alignedRange, isBold: isBold, isItalic: isItalic, isUnderlined: isUnderlined,
      isStrikethrough: isStrikethrough, fontSizePt: fontSizePt, foregroundHex: foregroundHex,
      backgroundHex: backgroundHex, linkURL: linkURL
    )
  }
}

public typealias DocumentParagraph = ParagraphBlock

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
  public var equations: [DocumentEquationBlock]
  public var watermarkText: String?

  public init(
    id: UUID = UUID(), title: String, format: DocumentFormat = .docx,
    paperSize: PaperSizePreset = .a4,
    headerText: String = "", footerText: String = "", paragraphs: [ParagraphBlock] = [],
    tables: [DocumentTableBlock] = [], trackChangesEnabled: Bool = false,
    sections: [DocumentSectionLayout] = [DocumentSectionLayout()], footnotes: [FootnoteItem] = [],
    revisions: [TrackedRevision] = [], comments: [DocumentCommentThread] = [],
    bookmarks: [String: UUID] = [:], images: [EmbeddedImageAttachment] = [],
    equations: [DocumentEquationBlock] = [], watermarkText: String? = nil
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
    self.equations = equations
    self.watermarkText = watermarkText
  }

  private enum CodingKeys: String, CodingKey {
    case id, title, format, paperSize, headerText, footerText, paragraphs, tables
    case trackChangesEnabled, sections, footnotes, revisions, comments, bookmarks, images
    case equations, watermarkText
  }

  public init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    self.id = try c.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
    self.title = try c.decode(String.self, forKey: .title)
    self.format = try c.decodeIfPresent(DocumentFormat.self, forKey: .format) ?? .docx
    let size = try c.decodeIfPresent(PaperSizePreset.self, forKey: .paperSize) ?? .a4
    self.paperSize = size
    self.headerText = try c.decodeIfPresent(String.self, forKey: .headerText) ?? ""
    self.footerText = try c.decodeIfPresent(String.self, forKey: .footerText) ?? ""
    self.paragraphs = try c.decodeIfPresent([ParagraphBlock].self, forKey: .paragraphs) ?? []
    self.tables = try c.decodeIfPresent([DocumentTableBlock].self, forKey: .tables) ?? []
    self.trackChangesEnabled =
      try c.decodeIfPresent(Bool.self, forKey: .trackChangesEnabled) ?? false
    let secs = try c.decodeIfPresent([DocumentSectionLayout].self, forKey: .sections) ?? []
    self.sections = secs.isEmpty ? [DocumentSectionLayout(paperSize: size)] : secs
    self.footnotes = try c.decodeIfPresent([FootnoteItem].self, forKey: .footnotes) ?? []
    self.revisions = try c.decodeIfPresent([TrackedRevision].self, forKey: .revisions) ?? []
    self.comments = try c.decodeIfPresent([DocumentCommentThread].self, forKey: .comments) ?? []
    self.bookmarks = try c.decodeIfPresent([String: UUID].self, forKey: .bookmarks) ?? [:]
    self.images = try c.decodeIfPresent([EmbeddedImageAttachment].self, forKey: .images) ?? []
    self.equations = try c.decodeIfPresent([DocumentEquationBlock].self, forKey: .equations) ?? []
    self.watermarkText = try c.decodeIfPresent(String.self, forKey: .watermarkText)
  }

  /// Resolves dynamic header/footer page number tokens (`{{PAGE}}`, `{{NUMPAGES}}`) for the given 1-based page number and total page count.
  public static func resolvePageTokens(in text: String, pageNumber: Int = 1, totalPages: Int = 1)
    -> String
  {
    guard !text.isEmpty else { return text }
    let safePage = "\(max(1, pageNumber))"
    let safeTotal = "\(max(1, totalPages))"
    return
      text
      .replacingOccurrences(of: "{{PAGE}}", with: safePage, options: [.caseInsensitive])
      .replacingOccurrences(of: "{{NUMPAGES}}", with: safeTotal, options: [.caseInsensitive])
  }

  public mutating func addEquation(title: String = "핵심 수식", rawExpression: String) {
    let cleaned = rawExpression.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !cleaned.isEmpty else { return }
    equations.append(DocumentEquationBlock(displayTitle: title, rawExpression: cleaned))
  }

  public mutating func updateEquation(id: UUID, title: String, rawExpression: String) {
    guard let idx = equations.firstIndex(where: { $0.id == id }) else { return }
    equations[idx].displayTitle = title
    equations[idx].rawExpression = rawExpression
  }

  public mutating func removeEquation(id: UUID) {
    equations.removeAll { $0.id == id }
  }

  public func searchablePlainText() -> String {
    var parts: [String] = [title, headerText, footerText]
    parts.append(contentsOf: paragraphs.map(\.text))
    parts.append(contentsOf: tables.flatMap { [$0.caption] + $0.rows.flatMap { $0 } })
    parts.append(
      contentsOf: equations.map { "\($0.displayTitle) \($0.rawExpression) \($0.renderedNotation)" })
    if let watermarkText, !watermarkText.isEmpty {
      parts.append(watermarkText)
    }
    return parts.filter { !$0.isEmpty }.joined(separator: "\n")
  }

  /// Computes live document metrics across all paragraphs and table cells.
  public func computeStatistics() -> DocumentStatistics {
    let paragraphTexts = paragraphs.map(\.text)
    let tableCellTexts = tables.flatMap { $0.rows.flatMap { $0 } }
    let combinedSegments = paragraphTexts + tableCellTexts
    let fullText = combinedSegments.joined(separator: "\n")

    let charsWithSpaces = fullText.unicodeScalars.filter { $0 != "\n" && $0 != "\r" }.count
    let charsWithoutSpaces = fullText.unicodeScalars.filter {
      !CharacterSet.whitespacesAndNewlines.contains($0)
    }.count

    let words =
      fullText
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
      while let range = text.range(
        of: search, options: options, range: currentIndex..<text.endIndex)
      {
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
