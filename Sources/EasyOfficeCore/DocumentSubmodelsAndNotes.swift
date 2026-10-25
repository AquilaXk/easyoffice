import Foundation

// MARK: - Document Lists, Tables, Footnotes, Tracked Revisions, Comments, Attachments & Equations

/// Multi-level paragraph list kind including Korean official administrative outline numbering.
public enum ParagraphListKind: String, Sendable, Codable, Equatable, CaseIterable {
  case none = "없음"
  case bullet = "글머리 기호 (•)"
  case numbered = "번호 매기기 (1. 2. 3.)"
  case koreanOfficial = "공문서 다단계 개요 (1. → 가. → (1))"
}

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
  public static let maxSpan = 256
  public static let maxIndex = 100_000
  public var rowIndex: Int
  public var columnIndex: Int
  public var rowSpan: Int
  public var columnSpan: Int
  public var borderStyle: TableCellBorderStyle

  private enum CodingKeys: String, CodingKey {
    case rowIndex
    case columnIndex
    case rowSpan
    case columnSpan
    case borderStyle
  }

  public init(
    rowIndex: Int,
    columnIndex: Int,
    rowSpan: Int = 1,
    columnSpan: Int = 1,
    borderStyle: TableCellBorderStyle = .solid
  ) {
    self.rowIndex = min(max(0, rowIndex), Self.maxIndex)
    self.columnIndex = min(max(0, columnIndex), Self.maxIndex)
    self.rowSpan = min(max(1, rowSpan), Self.maxSpan)
    self.columnSpan = min(max(1, columnSpan), Self.maxSpan)
    self.borderStyle = borderStyle
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    let rawRow = try container.decode(Int.self, forKey: .rowIndex)
    let rawCol = try container.decode(Int.self, forKey: .columnIndex)
    guard rawRow >= 0, rawRow <= CellAddress.maxZeroBasedRow,
      rawCol >= 0, rawCol <= CellAddress.maxZeroBasedColumn
    else {
      throw DecodingError.dataCorrupted(
        DecodingError.Context(
          codingPath: decoder.codingPath,
          debugDescription:
            "TableCellSpan coordinate out of bounds: rowIndex=\(rawRow), columnIndex=\(rawCol)"
        )
      )
    }
    let rawRowSpan = try container.decodeIfPresent(Int.self, forKey: .rowSpan) ?? 1
    let rawColSpan = try container.decodeIfPresent(Int.self, forKey: .columnSpan) ?? 1
    let border =
      try container.decodeIfPresent(TableCellBorderStyle.self, forKey: .borderStyle) ?? .solid
    self.rowIndex = rawRow
    self.columnIndex = rawCol
    self.rowSpan = min(max(1, rawRowSpan), Self.maxSpan)
    self.columnSpan = min(max(1, rawColSpan), Self.maxSpan)
    self.borderStyle = border
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
  public var headers: [String] { rows.first ?? [] }

  public func originSpan(atRow row: Int, column: Int) -> TableCellSpan? {
    cellSpans.first { $0.rowIndex == row && $0.columnIndex == column }
  }

  public func isCoveredByNonOriginSpan(atRow row: Int, column: Int) -> Bool {
    cellSpans.contains { span in
      let coversRow = row >= span.rowIndex && row < (span.rowIndex + span.rowSpan)
      let coversCol = column >= span.columnIndex && column < (span.columnIndex + span.columnSpan)
      let isOrigin = (row == span.rowIndex && column == span.columnIndex)
      return coversRow && coversCol && !isOrigin
    }
  }

  public func effectiveBorderStyle(atRow row: Int, column: Int) -> TableCellBorderStyle {
    originSpan(atRow: row, column: column)?.borderStyle ?? borderStyle
  }

  /// Inserts a row of `columnCount` cells at `index ?? rows.count` and shifts affected `cellSpans` down by 1.
  public mutating func insertRow(at index: Int? = nil, defaultCellText: String = "") {
    let cols = max(1, columnCount)
    let insertIdx = max(0, min(index ?? rows.count, rows.count))
    let newRow = Array(repeating: defaultCellText, count: cols)
    rows.insert(newRow, at: insertIdx)
    for i in cellSpans.indices {
      if cellSpans[i].rowIndex >= insertIdx {
        cellSpans[i].rowIndex += 1
      } else if cellSpans[i].rowIndex + cellSpans[i].rowSpan > insertIdx {
        cellSpans[i].rowSpan += 1
      }
    }
  }

  /// Removes the row at `index ?? (rows.count - 1)` when `rows.count > 1` and updates `cellSpans`.
  public mutating func deleteRow(at index: Int? = nil) {
    guard rows.count > 1 else { return }
    let removeIdx = max(0, min(index ?? (rows.count - 1), rows.count - 1))
    rows.remove(at: removeIdx)
    var updatedSpans: [TableCellSpan] = []
    for var span in cellSpans {
      if span.rowIndex == removeIdx {
        if span.rowSpan > 1 {
          span.rowSpan -= 1
          if span.rowSpan > 1 || span.columnSpan > 1 {
            updatedSpans.append(span)
          }
        }
      } else if span.rowIndex > removeIdx {
        span.rowIndex -= 1
        updatedSpans.append(span)
      } else {
        if span.rowIndex + span.rowSpan > removeIdx {
          span.rowSpan -= 1
        }
        if span.rowSpan > 1 || span.columnSpan > 1 {
          updatedSpans.append(span)
        }
      }
    }
    cellSpans = updatedSpans
  }

  /// Inserts a column at `index ?? columnCount` across all rows and shifts affected `cellSpans` right by 1.
  public mutating func insertColumn(
    at index: Int? = nil, headerTitle: String? = nil, defaultCellText: String = ""
  ) {
    let currentCols = columnCount
    let insertIdx = max(0, min(index ?? currentCols, currentCols))
    if rows.isEmpty {
      rows = [[headerTitle ?? (hasHeaderRow ? "열 1" : defaultCellText)]]
    } else {
      for rIdx in rows.indices {
        let safeColIdx = min(insertIdx, rows[rIdx].count)
        let cellText: String
        if rIdx == 0 && hasHeaderRow, let headerTitle {
          cellText = headerTitle
        } else {
          cellText = defaultCellText
        }
        rows[rIdx].insert(cellText, at: safeColIdx)
      }
    }
    for i in cellSpans.indices {
      if cellSpans[i].columnIndex >= insertIdx {
        cellSpans[i].columnIndex += 1
      } else if cellSpans[i].columnIndex + cellSpans[i].columnSpan > insertIdx {
        cellSpans[i].columnSpan += 1
      }
    }
  }

  /// Removes the column at `index ?? (columnCount - 1)` when `columnCount > 1` and updates `cellSpans`.
  public mutating func deleteColumn(at index: Int? = nil) {
    let currentCols = columnCount
    guard currentCols > 1 else { return }
    let removeIdx = max(0, min(index ?? (currentCols - 1), currentCols - 1))
    for rIdx in rows.indices where removeIdx < rows[rIdx].count {
      rows[rIdx].remove(at: removeIdx)
    }
    var updatedSpans: [TableCellSpan] = []
    for var span in cellSpans {
      if span.columnIndex == removeIdx {
        if span.columnSpan > 1 {
          span.columnSpan -= 1
          if span.rowSpan > 1 || span.columnSpan > 1 {
            updatedSpans.append(span)
          }
        }
      } else if span.columnIndex > removeIdx {
        span.columnIndex -= 1
        updatedSpans.append(span)
      } else {
        if span.columnIndex + span.columnSpan > removeIdx {
          span.columnSpan -= 1
        }
        if span.rowSpan > 1 || span.columnSpan > 1 {
          updatedSpans.append(span)
        }
      }
    }
    cellSpans = updatedSpans
  }

  /// Merges a rectangular block of cells starting at `(startRow, startCol)`.
  public mutating func mergeCells(startRow: Int, startCol: Int, rowSpan: Int, colSpan: Int) {
    guard startRow >= 0, startCol >= 0, rowSpan >= 1, colSpan >= 1, rowSpan > 1 || colSpan > 1
    else {
      return
    }
    let (endRow, rowOverflow) = startRow.addingReportingOverflow(rowSpan)
    let (endCol, colOverflow) = startCol.addingReportingOverflow(colSpan)
    guard !rowOverflow, !colOverflow,
      endRow <= rows.count,
      (startRow..<endRow).allSatisfy({ endCol <= rows[$0].count })
    else {
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
      row >= span.rowIndex && row < (span.rowIndex + span.rowSpan) && col >= span.columnIndex
        && col < (span.columnIndex + span.columnSpan)
    }
  }

  /// Evaluates inline table formulas (`=SUM(ABOVE)`, `=AVERAGE(ABOVE)`, `=SUM(LEFT)`, `=AVERAGE(LEFT)`, `=COUNT(ABOVE)`, `=MAX(ABOVE)`, `=MIN(ABOVE)`).
  public func evaluateTableFormula(
    _ expression: String, atRow targetRow: Int, column targetCol: Int
  ) -> Double? {
    var cleaned = expression.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
    if cleaned.hasPrefix("=") {
      cleaned.removeFirst()
      cleaned = cleaned.trimmingCharacters(in: .whitespaces)
    }

    guard let openParen = cleaned.firstIndex(of: "("),
      cleaned.hasSuffix(")")
    else {
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
    let stripped =
      raw
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

public typealias DocumentFootnote = FootnoteItem

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

public typealias DocumentComment = DocumentCommentThread

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

public enum ImageWrapMode: String, Sendable, Codable, Equatable, CaseIterable {
  case inline = "글자처럼 취급 (인라인)"
  case squareLeft = "어울림 (좌측 배치)"
  case squareRight = "어울림 (우측 배치)"
  case topAndBottom = "자리 차지 (상하 분리)"
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
  public var wrapMode: ImageWrapMode

  public init(
    id: UUID = UUID(), fileName: String, caption: String = "", mimeType: String = "image/png",
    imageData: Data, widthPt: Double = 320.0, heightPt: Double = 200.0,
    anchorParagraphID: UUID? = nil, wrapMode: ImageWrapMode = .inline
  ) {
    self.id = id
    self.fileName = fileName
    self.caption = caption
    self.mimeType = mimeType
    self.imageData = imageData
    self.widthPt = widthPt
    self.heightPt = heightPt
    self.anchorParagraphID = anchorParagraphID
    self.wrapMode = wrapMode
  }

  private enum CodingKeys: String, CodingKey {
    case id, fileName, caption, mimeType, imageData, widthPt, heightPt, anchorParagraphID, wrapMode
  }

  public init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    self.id = try c.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
    self.fileName = try c.decode(String.self, forKey: .fileName)
    self.caption = try c.decodeIfPresent(String.self, forKey: .caption) ?? ""
    self.mimeType = try c.decodeIfPresent(String.self, forKey: .mimeType) ?? "image/png"
    self.imageData = try c.decodeIfPresent(Data.self, forKey: .imageData) ?? Data()
    self.widthPt = try c.decodeIfPresent(Double.self, forKey: .widthPt) ?? 320.0
    self.heightPt = try c.decodeIfPresent(Double.self, forKey: .heightPt) ?? 200.0
    self.anchorParagraphID = try c.decodeIfPresent(UUID.self, forKey: .anchorParagraphID)
    self.wrapMode = try c.decodeIfPresent(ImageWrapMode.self, forKey: .wrapMode) ?? .inline
  }
}

public struct DocumentEquationBlock: Sendable, Codable, Equatable, Identifiable {
  public var id: UUID
  public var displayTitle: String
  public var rawExpression: String

  public init(id: UUID = UUID(), displayTitle: String = "수식 블록", rawExpression: String) {
    self.id = id
    self.displayTitle = displayTitle
    self.rawExpression = rawExpression
  }

  public init(id: UUID = UUID(), displayTitle: String = "수식 블록", latexSource: String) {
    self.id = id
    self.displayTitle = displayTitle
    self.rawExpression = latexSource
  }

  public var latexSource: String {
    get { rawExpression }
    set { rawExpression = newValue }
  }

  public var renderedNotation: String { Self.formatMathSymbols(rawExpression) }

  public var displayText: String { renderedNotation }

  private static let subscriptTable: [Character: Character] = [
    "0": "₀", "1": "₁", "2": "₂", "3": "₃", "4": "₄",
    "5": "₅", "6": "₆", "7": "₇", "8": "₈", "9": "₉",
    "+": "₊", "-": "₋", "=": "₌", "(": "₍", ")": "₎",
    "a": "ₐ", "e": "ₑ", "h": "ₕ", "i": "ᵢ", "j": "ⱼ",
    "k": "ₖ", "l": "ₗ", "m": "ₘ", "n": "ₙ", "o": "ₒ",
    "p": "ₚ", "r": "ᵣ", "s": "ₛ", "t": "ₜ", "u": "ᵤ",
    "v": "ᵥ", "x": "ₓ",
  ]

  private static let superscriptTable: [Character: Character] = [
    "0": "⁰", "1": "¹", "2": "²", "3": "³", "4": "⁴",
    "5": "⁵", "6": "⁶", "7": "⁷", "8": "⁸", "9": "⁹",
    "+": "⁺", "-": "⁻", "=": "⁼", "(": "⁽", ")": "⁾",
    "a": "ᵃ", "b": "ᵇ", "c": "ᶜ", "d": "ᵈ", "e": "ᵉ",
    "f": "ᶠ", "g": "ᵍ", "h": "ʰ", "i": "ⁱ", "j": "ʲ",
    "k": "ᵏ", "l": "ˡ", "m": "ᵐ", "n": "ⁿ", "o": "ᵒ",
    "p": "ᵖ", "r": "ʳ", "s": "ˢ", "t": "ᵗ", "u": "ᵘ",
    "v": "ᵛ", "w": "ʷ", "x": "ˣ", "y": "ʸ", "z": "ᶻ",
  ]

  private static let maxMathNestingDepth = 16
  private static let maxMathMacroIterations = 64

  public static func formatMathSymbols(_ raw: String) -> String {
    formatMathSymbolsBounded(raw, depth: 0)
  }

  private static func formatMathSymbolsBounded(_ raw: String, depth: Int) -> String {
    var s = raw
    let replacements: [(String, String)] = [
      ("\\partial", "∂"), ("\\approx", "≈"), ("\\infty", "∞"), ("\\nabla", "∇"),
      ("\\equiv", "≡"), ("\\times", "×"), ("\\alpha", "α"), ("\\gamma", "γ"),
      ("\\delta", "δ"), ("\\Delta", "Δ"), ("\\lambda", "λ"), ("\\sigma", "σ"),
      ("\\Sigma", "Σ"), ("\\theta", "θ"), ("\\omega", "ω"), ("\\prod", "∏"),
      ("\\cdot", "·"), ("\\beta", "β"), ("\\sum", "∑"), ("\\int", "∫"),
      ("\\neq", "≠"), ("\\leq", "≤"), ("\\geq", "≥"), ("\\div", "÷"),
      ("\\ne", "≠"), ("\\le", "≤"), ("\\ge", "≥"), ("\\pm", "±"),
      ("\\mu", "μ"), ("\\pi", "π"),
    ]
    for (from, to) in replacements {
      s = s.replacingOccurrences(of: from, with: to)
    }

    guard depth < maxMathNestingDepth else {
      return s.replacingOccurrences(of: "\\sqrt", with: "√")
    }

    var sqrtIter = 0
    while sqrtIter < maxMathMacroIterations, let sqrtRange = s.range(of: "\\sqrt{") {
      sqrtIter += 1
      let openBrace = s.index(before: sqrtRange.upperBound)
      guard let (inner, afterClose) = extractBracedGroup(in: s, from: openBrace) else { break }
      let formattedInner =
        formatMathSymbolsBounded(inner, depth: depth + 1).trimmingCharacters(in: .whitespaces)
      let replacement = formattedInner.count <= 1 ? "√\(formattedInner)" : "√(\(formattedInner))"
      s.replaceSubrange(sqrtRange.lowerBound..<afterClose, with: replacement)
    }
    s = s.replacingOccurrences(of: "\\sqrt", with: "√")

    var fracIter = 0
    while fracIter < maxMathMacroIterations, let fracRange = s.range(of: "\\frac{") {
      fracIter += 1
      let firstOpen = s.index(before: fracRange.upperBound)
      guard let (rawNum, afterFirst) = extractBracedGroup(in: s, from: firstOpen) else { break }
      var secondOpen = afterFirst
      while secondOpen < s.endIndex && s[secondOpen].isWhitespace {
        secondOpen = s.index(after: secondOpen)
      }
      guard secondOpen < s.endIndex, s[secondOpen] == "{",
        let (rawDen, afterSecond) = extractBracedGroup(in: s, from: secondOpen)
      else { break }
      let num =
        formatMathSymbolsBounded(rawNum, depth: depth + 1).trimmingCharacters(in: .whitespaces)
      let den =
        formatMathSymbolsBounded(rawDen, depth: depth + 1).trimmingCharacters(in: .whitespaces)
      let formattedNum = hasTopLevelAdditiveOperator(num) ? "(\(num))" : num
      let formattedDen = hasTopLevelAdditiveOperator(den) ? "(\(den))" : den
      s.replaceSubrange(fracRange.lowerBound..<afterSecond, with: "\(formattedNum) / \(formattedDen)")
    }

    var out = ""
    out.reserveCapacity(s.count)
    var idx = s.startIndex
    while idx < s.endIndex {
      let ch = s[idx]
      if ch == "^" || ch == "_" {
        let isSuper = (ch == "^")
        let table = isSuper ? superscriptTable : subscriptTable
        let nextIdx = s.index(after: idx)
        if nextIdx < s.endIndex {
          if s[nextIdx] == "{", let (inner, afterClose) = extractBracedGroup(in: s, from: nextIdx) {
            let cleanedInner = inner.filter { $0 != "{" && $0 != "}" && $0 != (isSuper ? "^" : "_") }
            var mapped = ""
            var allMapped = !cleanedInner.isEmpty
            for c in cleanedInner {
              if let m = table[c] {
                mapped.append(m)
              } else {
                allMapped = false
                break
              }
            }
            if allMapped {
              out.append(mapped)
            } else {
              out.append(ch)
              out.append(formatMathSymbolsBounded(inner, depth: depth + 1))
            }
            idx = afterClose
            continue
          } else if let mappedChar = table[s[nextIdx]] {
            out.append(mappedChar)
            idx = s.index(after: nextIdx)
            continue
          }
        }
      }
      if ch != "{" && ch != "}" {
        out.append(ch)
      }
      idx = s.index(after: idx)
    }
    return out
  }

  private static func extractBracedGroup(
    in text: String,
    from openBraceIndex: String.Index
  ) -> (content: String, afterCloseIndex: String.Index)? {
    guard openBraceIndex < text.endIndex, text[openBraceIndex] == "{" else { return nil }
    var depth = 0
    var cursor = openBraceIndex
    let contentStart = text.index(after: openBraceIndex)
    while cursor < text.endIndex {
      let ch = text[cursor]
      if ch == "{" {
        depth += 1
      } else if ch == "}" {
        depth -= 1
        if depth == 0 {
          let afterClose = text.index(after: cursor)
          return (String(text[contentStart..<cursor]), afterClose)
        }
      }
      cursor = text.index(after: cursor)
    }
    return nil
  }

  private static func hasTopLevelAdditiveOperator(_ term: String) -> Bool {
    var depth = 0
    for (idx, ch) in term.enumerated() {
      if ch == "(" || ch == "[" || ch == "{" {
        depth += 1
      } else if ch == ")" || ch == "]" || ch == "}" {
        depth = max(0, depth - 1)
      } else if depth == 0 && idx > 0 && (ch == "+" || ch == "-" || ch == "±") {
        return true
      }
    }
    return false
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
