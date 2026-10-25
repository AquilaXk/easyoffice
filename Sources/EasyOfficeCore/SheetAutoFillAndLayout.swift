import Foundation

/// Aggregated statistics for a rectangular cell selection in a worksheet.
public struct CellRangeAggregationSummary: Sendable, Equatable {
  public let cellCount: Int
  public let nonEmptyCount: Int
  public let numericCount: Int
  public let sum: Double
  public let average: Double
  public let minimum: Double?
  public let maximum: Double?

  public init(
    cellCount: Int,
    nonEmptyCount: Int,
    numericCount: Int,
    sum: Double,
    average: Double,
    minimum: Double?,
    maximum: Double?
  ) {
    self.cellCount = cellCount
    self.nonEmptyCount = nonEmptyCount
    self.numericCount = numericCount
    self.sum = sum
    self.average = average
    self.minimum = minimum
    self.maximum = maximum
  }
}

// MARK: - Custom Column/Row Dimensions, Range Summary, Auto-Filter & Auto-Fill Engine

extension WorksheetModel {
  public static let defaultColumnWidthPt: Double = 112.0
  public static let minColumnWidthPt: Double = 48.0
  public static let maxColumnWidthPt: Double = 420.0
  public static let defaultRowHeightPt: Double = 30.0
  public static let minRowHeightPt: Double = 22.0
  public static let maxRowHeightPt: Double = 180.0

  /// Returns the custom width (in points) for `column`, or `112.0` pt if unset.
  public func columnWidth(at column: Int) -> Double {
    columnWidths[column] ?? Self.defaultColumnWidthPt
  }

  /// Sets the custom width (in points) for `column`, clamped to `48.0...420.0` pt.
  public mutating func setColumnWidth(_ width: Double, at column: Int) {
    guard column >= 0, column <= CellAddress.maxZeroBasedColumn, width.isFinite else { return }
    columnWidths[column] = min(max(width, Self.minColumnWidthPt), Self.maxColumnWidthPt)
  }

  /// Returns the custom height (in points) for `row`, or `30.0` pt if unset.
  public func rowHeight(at row: Int) -> Double {
    rowHeights[row] ?? Self.defaultRowHeightPt
  }

  /// Sets the custom height (in points) for `row`, clamped to `22.0...180.0` pt.
  public mutating func setRowHeight(_ height: Double, at row: Int) {
    guard row >= 0, row <= CellAddress.maxZeroBasedRow, height.isFinite else { return }
    rowHeights[row] = min(max(height, Self.minRowHeightPt), Self.maxRowHeightPt)
  }

  /// Computes live selection statistics (`cellCount`, `nonEmptyCount`, `numericCount`, `sum`, `average`, `minimum`, `maximum`) across `range`.
  public func summarizeRange(
    _ range: CellRange,
    using preEvaluated: [PackedCellCoordinate: EvaluatedCellValue]? = nil
  ) -> CellRangeAggregationSummary {
    var cellCount = 0
    var nonEmptyCount = 0
    var numericCount = 0
    var sum = 0.0
    var minVal: Double?
    var maxVal: Double?
    var context = EvaluationContext()

    range.forEachCoordinate { coord in
      cellCount += 1
      if let raw = cellsByCoordinate[coord], !raw.isEmpty { nonEmptyCount += 1 }
      let evaluated =
        preEvaluated?[coord]
        ?? (cellsByCoordinate[coord] != nil
          ? evaluateInternal(at: coord, context: &context, depth: 0) : .empty)
      if case .number(let num) = evaluated, num.isFinite {
        numericCount += 1
        sum += num
        minVal = minVal.map { min($0, num) } ?? num
        maxVal = maxVal.map { max($0, num) } ?? num
      }
    }

    let avg = numericCount > 0 ? (sum / Double(numericCount)) : 0.0
    return CellRangeAggregationSummary(
      cellCount: cellCount,
      nonEmptyCount: nonEmptyCount,
      numericCount: numericCount,
      sum: sum,
      average: avg,
      minimum: minVal,
      maximum: maxVal
    )
  }

  /// Applies `transform` to the `CellStyle` of every coordinate in `range`.
  public mutating func updateCellStyleInRange(
    _ range: CellRange, transform: (inout CellStyle) -> Void
  ) {
    range.forEachCoordinate { coord in
      var style = cellStyles[coord] ?? CellStyle()
      transform(&style)
      setCellStyle(style, at: coord)
    }
  }

  /// Returns the set of zero-based row indices matching `query` in `column` (`0...maxRow`), always preserving header row `0`.
  public func filterMatchingRowIndices(
    column: Int,
    query: String,
    maxRow: Int,
    using preEvaluated: [PackedCellCoordinate: EvaluatedCellValue]? = nil
  ) -> Set<Int> {
    let clampedMaxRow = max(
      0, min(maxRow, min(CellAddress.maxZeroBasedRow, CellRange.maxExpandedCellCount)))
    let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return Set(0...clampedMaxRow) }
    var result: Set<Int> = [0]
    guard clampedMaxRow >= 1, column >= 0, column <= CellAddress.maxZeroBasedColumn else {
      return result
    }
    var context = EvaluationContext()
    for r in 1...clampedMaxRow {
      let coord = PackedCellCoordinate(row: r, column: column)
      if let raw = cellsByCoordinate[coord],
        raw.range(of: trimmed, options: [.caseInsensitive]) != nil
      {
        result.insert(r)
        continue
      }
      let evaluated: EvaluatedCellValue
      if let pre = preEvaluated?[coord] {
        evaluated = pre
      } else if cellsByCoordinate[coord] != nil {
        evaluated = evaluateInternal(at: coord, context: &context, depth: 0)
      } else {
        continue
      }
      let style = cellStyles[coord] ?? CellStyle()
      let formatted = style.numberFormat.format(evaluated)
      if formatted.range(of: trimmed, options: [.caseInsensitive]) != nil
        || evaluated.description.range(of: trimmed, options: [.caseInsensitive]) != nil
      {
        result.insert(r)
      }
    }
    return result
  }

  // MARK: - Formula Relative/Absolute ($) Reference Shifter

  /// Shifts relative cell references in `formula` by `(rowDelta, columnDelta)` while respecting `$` anchors, string literals, function names, and sheet prefixes.
  public static func shiftFormulaReferences(
    _ formula: String,
    rowDelta: Int,
    columnDelta: Int,
    respectAnchors: Bool = true,
    minRow: Int? = nil,
    minCol: Int? = nil
  ) -> String {
    let bytes = Array(formula.utf8)
    var outBytes: [UInt8] = []
    outBytes.reserveCapacity(bytes.count + 16)

    var i = 0
    while i < bytes.count {
      let b = bytes[i]
      if b == 0x22 || b == 0x27 {
        i = consumeQuotedLiteral(in: bytes, from: i, into: &outBytes)
        continue
      }

      let atBoundary = (i == 0) || !isIdentifierOrDollarByte(bytes[i - 1])
      if atBoundary,
        let (replacement, nextIndex) = matchAndShiftCellRef(
          in: bytes, at: i, rowDelta: rowDelta, columnDelta: columnDelta,
          respectAnchors: respectAnchors, minRow: minRow, minCol: minCol
        )
      {
        outBytes.append(contentsOf: replacement.utf8)
        i = nextIndex
        continue
      }

      outBytes.append(b)
      i += 1
    }
    return String(decoding: outBytes, as: UTF8.self)
  }

  private static func consumeQuotedLiteral(
    in bytes: [UInt8],
    from start: Int,
    into outBytes: inout [UInt8]
  ) -> Int {
    let quote = bytes[start]
    outBytes.append(quote)
    var i = start + 1
    while i < bytes.count {
      let ch = bytes[i]
      outBytes.append(ch)
      i += 1
      if ch == quote {
        if i < bytes.count && bytes[i] == quote {
          outBytes.append(bytes[i])
          i += 1
        } else {
          break
        }
      }
    }
    return i
  }

  private static func isIdentifierOrDollarByte(_ b: UInt8) -> Bool {
    (b >= 0x41 && b <= 0x5A) || (b >= 0x61 && b <= 0x7A) || (b >= 0x30 && b <= 0x39) || b == 0x5F
      || b == 0x24
  }

  private static func matchAndShiftCellRef(
    in bytes: [UInt8],
    at start: Int,
    rowDelta: Int,
    columnDelta: Int,
    respectAnchors: Bool = true,
    minRow: Int? = nil,
    minCol: Int? = nil
  ) -> (String, Int)? {
    guard let parsed = parseRawCellRef(in: bytes, at: start) else { return nil }
    let canShiftCol =
      !(respectAnchors && parsed.colAnchored) && (minCol.map { parsed.col >= $0 } ?? true)
    let canShiftRow =
      !(respectAnchors && parsed.rowAnchored) && (minRow.map { parsed.row >= $0 } ?? true)
    let newCol =
      canShiftCol
      ? min(max(0, parsed.col + columnDelta), CellAddress.maxZeroBasedColumn) : parsed.col
    let newRow =
      canShiftRow
      ? min(max(0, parsed.row + rowDelta), CellAddress.maxZeroBasedRow) : parsed.row
    return (parsed.formatted(col: newCol, row: newRow), parsed.nextIndex)
  }

  // MARK: - Auto-Fill Handle Engine

  /// Fills `targetRange` from `sourceRange` downward or rightward, shifting formula references and extrapolating numeric/label series.
  public mutating func autofill(sourceRange: CellRange, targetRange: CellRange) throws {
    let srcMinRow = min(sourceRange.start.row, sourceRange.end.row)
    let srcMaxRow = max(sourceRange.start.row, sourceRange.end.row)
    let srcMinCol = min(sourceRange.start.column, sourceRange.end.column)
    let srcMaxCol = max(sourceRange.start.column, sourceRange.end.column)

    let dstMinRow = min(targetRange.start.row, targetRange.end.row)
    let dstMaxRow = max(targetRange.start.row, targetRange.end.row)
    let dstMinCol = min(targetRange.start.column, targetRange.end.column)
    let dstMaxCol = max(targetRange.start.column, targetRange.end.column)

    let isDownward =
      (srcMinCol == dstMinCol && srcMaxCol == dstMaxCol && dstMinRow >= srcMinRow
        && dstMinRow <= srcMaxRow + 1 && dstMaxRow > srcMaxRow)
    let isRightward =
      (srcMinRow == dstMinRow && srcMaxRow == dstMaxRow && dstMinCol >= srcMinCol
        && dstMinCol <= srcMaxCol + 1 && dstMaxCol > srcMaxCol)

    let dstCellCount = (dstMaxRow - dstMinRow + 1) * (dstMaxCol - dstMinCol + 1)
    guard isDownward || isRightward, dstCellCount <= CellRange.maxExpandedCellCount else {
      throw EasyOfficeError.invalidCellRange("\(sourceRange)->\(targetRange)")
    }

    if isDownward {
      let srcRowCount = srcMaxRow - srcMinRow + 1
      for c in srcMinCol...srcMaxCol {
        let srcRaws = (srcMinRow...srcMaxRow).map {
          cellsByCoordinate[PackedCellCoordinate(row: $0, column: c)] ?? ""
        }
        let srcStyles = (srcMinRow...srcMaxRow).map {
          cellStyles[PackedCellCoordinate(row: $0, column: c)] ?? CellStyle()
        }
        let (allNumeric, baseVal, linearDelta) = Self.parseNumericProgression(srcRaws)
        for r in (srcMaxRow + 1)...dstMaxRow {
          let stepOffset = r - srcMinRow
          let srcIdx = stepOffset % srcRowCount
          let srcRow = srcMinRow + srcIdx
          setCellStyle(srcStyles[srcIdx], at: PackedCellCoordinate(row: r, column: c))
          let filled = Self.computeAutoFillValue(
            raw: srcRaws[srcIdx], stepFromStart: stepOffset, deltaFromSource: r - srcRow,
            rowDelta: r - srcRow, columnDelta: 0, allNumeric: allNumeric, baseVal: baseVal,
            linearDelta: linearDelta
          )
          try setCell(row: r, column: c, rawValue: filled)
        }
      }
    } else if isRightward {
      let srcColCount = srcMaxCol - srcMinCol + 1
      for r in srcMinRow...srcMaxRow {
        let srcRaws = (srcMinCol...srcMaxCol).map {
          cellsByCoordinate[PackedCellCoordinate(row: r, column: $0)] ?? ""
        }
        let srcStyles = (srcMinCol...srcMaxCol).map {
          cellStyles[PackedCellCoordinate(row: r, column: $0)] ?? CellStyle()
        }
        let (allNumeric, baseVal, linearDelta) = Self.parseNumericProgression(srcRaws)
        for c in (srcMaxCol + 1)...dstMaxCol {
          let stepOffset = c - srcMinCol
          let srcIdx = stepOffset % srcColCount
          let srcCol = srcMinCol + srcIdx
          setCellStyle(srcStyles[srcIdx], at: PackedCellCoordinate(row: r, column: c))
          let filled = Self.computeAutoFillValue(
            raw: srcRaws[srcIdx], stepFromStart: stepOffset, deltaFromSource: c - srcCol,
            rowDelta: 0, columnDelta: c - srcCol, allNumeric: allNumeric, baseVal: baseVal,
            linearDelta: linearDelta
          )
          try setCell(row: r, column: c, rawValue: filled)
        }
      }
    }
  }

  private static func parseNumericProgression(_ srcRaws: [String]) -> (Bool, Double, Double) {
    guard srcRaws.count >= 2 else { return (false, 0.0, 0.0) }
    var nums: [Double] = []
    nums.reserveCapacity(srcRaws.count)
    for raw in srcRaws {
      let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
      guard !trimmed.hasPrefix("="), let val = Double(trimmed), val.isFinite else {
        return (false, 0.0, 0.0)
      }
      nums.append(val)
    }
    let step = nums[1] - nums[0]
    if nums.count > 2 {
      let isLinear = (2..<nums.count).allSatisfy { abs((nums[$0] - nums[$0 - 1]) - step) <= 1e-9 }
      guard isLinear else { return (false, 0.0, 0.0) }
    }
    return (true, nums[0], step)
  }

  private static func computeAutoFillValue(
    raw: String, stepFromStart: Int, deltaFromSource: Int, rowDelta: Int, columnDelta: Int,
    allNumeric: Bool, baseVal: Double, linearDelta: Double
  ) -> String {
    if raw.trimmingCharacters(in: .whitespacesAndNewlines).hasPrefix("=") {
      return shiftFormulaReferences(raw, rowDelta: rowDelta, columnDelta: columnDelta)
    }
    if allNumeric {
      let extrapolated = baseVal + Double(stepFromStart) * linearDelta
      if extrapolated.rounded() == extrapolated, let intVal = safeInt(extrapolated) {
        return String(intVal)
      }
      return String(extrapolated)
    }
    return incrementTrailingIntegerLabel(raw, delta: deltaFromSource) ?? raw
  }

  private static func incrementTrailingIntegerLabel(_ raw: String, delta: Int) -> String? {
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty, Double(trimmed) == nil else { return nil }
    let scalars = Array(raw.unicodeScalars)
    var lastDigitEnd: Int?
    var lastDigitStart: Int?
    var idx = scalars.count - 1
    while idx >= 0 {
      let v = scalars[idx].value
      if v >= 0x30 && v <= 0x39 {
        if lastDigitEnd == nil { lastDigitEnd = idx + 1 }
        lastDigitStart = idx
      } else if lastDigitEnd != nil {
        break
      }
      idx -= 1
    }
    guard let start = lastDigitStart, let end = lastDigitEnd else { return nil }
    let digitStr = String(String.UnicodeScalarView(scalars[start..<end]))
    guard let baseVal = Int(digitStr) else { return nil }
    let (sumVal, overflow) = baseVal.addingReportingOverflow(delta)
    let nextVal = overflow ? (delta >= 0 ? Int.max : 0) : max(0, sumVal)
    let nextStr = String(nextVal)
    let formattedDigits: String
    if digitStr.count > 1 && digitStr.hasPrefix("0") && nextStr.count < digitStr.count {
      formattedDigits = String(repeating: "0", count: digitStr.count - nextStr.count) + nextStr
    } else {
      formattedDigits = nextStr
    }
    return String(String.UnicodeScalarView(scalars[0..<start])) + formattedDigits
      + String(String.UnicodeScalarView(scalars[end..<scalars.count]))
  }

  // MARK: - Structural Row & Column Insertion / Deletion

  public mutating func insertRow(at rowIndex: Int) {
    guard rowIndex >= 0, rowIndex <= CellAddress.maxZeroBasedRow else { return }
    applyStructuralAxisShift(isRowAxis: true, index: rowIndex, delta: 1)
  }

  public mutating func deleteRow(at rowIndex: Int) {
    guard rowIndex >= 0, rowIndex <= CellAddress.maxZeroBasedRow else { return }
    applyStructuralAxisShift(isRowAxis: true, index: rowIndex, delta: -1)
  }

  public mutating func insertColumn(at columnIndex: Int) {
    guard columnIndex >= 0, columnIndex <= CellAddress.maxZeroBasedColumn else { return }
    applyStructuralAxisShift(isRowAxis: false, index: columnIndex, delta: 1)
  }

  public mutating func deleteColumn(at columnIndex: Int) {
    guard columnIndex >= 0, columnIndex <= CellAddress.maxZeroBasedColumn else { return }
    applyStructuralAxisShift(isRowAxis: false, index: columnIndex, delta: -1)
  }

  private mutating func applyStructuralAxisShift(isRowAxis: Bool, index: Int, delta: Int) {
    func shiftedCoord(_ coord: PackedCellCoordinate) -> PackedCellCoordinate? {
      let axisVal = isRowAxis ? coord.row : coord.column
      if delta < 0 && axisVal == index { return nil }
      let shouldShift = delta > 0 ? (axisVal >= index) : (axisVal > index)
      guard shouldShift else { return coord }
      if isRowAxis {
        let newRow = min(max(0, coord.row + delta), CellAddress.maxZeroBasedRow)
        return PackedCellCoordinate(row: newRow, column: coord.column)
      } else {
        let newCol = min(max(0, coord.column + delta), CellAddress.maxZeroBasedColumn)
        return PackedCellCoordinate(row: coord.row, column: newCol)
      }
    }

    var nextCells: [PackedCellCoordinate: String] = [:]
    nextCells.reserveCapacity(cellsByCoordinate.count)
    for (coord, raw) in cellsByCoordinate {
      guard let targetCoord = shiftedCoord(coord) else { continue }
      if raw.trimmingCharacters(in: .whitespacesAndNewlines).hasPrefix("=") {
        nextCells[targetCoord] = Self.shiftFormulaReferencesAtAxis(
          raw, isRowAxis: isRowAxis, thresholdIndex: index, delta: delta
        )
      } else {
        nextCells[targetCoord] = raw
      }
    }
    cellsByCoordinate = nextCells

    var nextStyles: [PackedCellCoordinate: CellStyle] = [:]
    for (coord, style) in cellStyles {
      if let targetCoord = shiftedCoord(coord) {
        nextStyles[targetCoord] = style
      }
    }
    cellStyles = nextStyles

    var nextComments: [PackedCellCoordinate: String] = [:]
    for (coord, comment) in sheetComments {
      if let targetCoord = shiftedCoord(coord) {
        nextComments[targetCoord] = comment
      }
    }
    sheetComments = nextComments

    if isRowAxis {
      var nextHeights: [Int: Double] = [:]
      for (r, h) in rowHeights {
        if delta < 0 && r == index { continue }
        let newR =
          (delta > 0 ? r >= index : r > index)
          ? min(max(0, r + delta), CellAddress.maxZeroBasedRow) : r
        nextHeights[newR] = h
      }
      rowHeights = nextHeights
    } else {
      var nextWidths: [Int: Double] = [:]
      for (c, w) in columnWidths {
        if delta < 0 && c == index { continue }
        let newC =
          (delta > 0 ? c >= index : c > index)
          ? min(max(0, c + delta), CellAddress.maxZeroBasedColumn) : c
        nextWidths[newC] = w
      }
      columnWidths = nextWidths
    }

    mergedRanges = mergedRanges.compactMap { range in
      let sAxis = isRowAxis ? range.start.row : range.start.column
      let eAxis = isRowAxis ? range.end.row : range.end.column
      if delta < 0 && sAxis == index && eAxis == index { return nil }
      let newStartRow =
        isRowAxis && (delta > 0 ? range.start.row >= index : range.start.row > index)
        ? min(max(0, range.start.row + delta), CellAddress.maxZeroBasedRow) : range.start.row
      let newEndRow =
        isRowAxis && (delta > 0 ? range.end.row >= index : range.end.row > index)
        ? min(max(0, range.end.row + delta), CellAddress.maxZeroBasedRow) : range.end.row
      let newStartCol =
        !isRowAxis && (delta > 0 ? range.start.column >= index : range.start.column > index)
        ? min(max(0, range.start.column + delta), CellAddress.maxZeroBasedColumn)
        : range.start.column
      let newEndCol =
        !isRowAxis && (delta > 0 ? range.end.column >= index : range.end.column > index)
        ? min(max(0, range.end.column + delta), CellAddress.maxZeroBasedColumn) : range.end.column
      let startAddr = CellAddress(
        packed: PackedCellCoordinate(row: newStartRow, column: newStartCol))
      let endAddr = CellAddress(packed: PackedCellCoordinate(row: newEndRow, column: newEndCol))
      return CellRange(start: startAddr, end: endAddr)
    }

    var nextNamedRanges: [String: String] = [:]
    for (key, notation) in namedRanges {
      let shiftedFormula = Self.shiftFormulaReferencesAtAxis(
        "=" + notation,
        isRowAxis: isRowAxis,
        thresholdIndex: index,
        delta: delta
      )
      let shiftedNotation =
        shiftedFormula.hasPrefix("=") ? String(shiftedFormula.dropFirst()) : shiftedFormula
      nextNamedRanges[key] = shiftedNotation
    }
    namedRanges = nextNamedRanges

    compiledASTs.removeAll(keepingCapacity: true)
    for (coord, rawVal) in cellsByCoordinate where !rawVal.isEmpty {
      compiledASTs[coord] = Self.compileCellInput(rawVal)
    }
  }

  private struct ParsedCellRef {
    let col: Int
    let row: Int
    let colAnchored: Bool
    let rowAnchored: Bool
    let nextIndex: Int

    func formatted(col newCol: Int, row newRow: Int) -> String {
      let clampedCol = min(max(0, newCol), CellAddress.maxZeroBasedColumn)
      let clampedRow = min(max(0, newRow), CellAddress.maxZeroBasedRow)
      return
        "\(colAnchored ? "$" : "")\(CellAddress.columnLabel(for: clampedCol))\(rowAnchored ? "$" : "")\(clampedRow + 1)"
    }
  }

  private static func parseRawCellRef(in bytes: [UInt8], at start: Int) -> ParsedCellRef? {
    var cursor = start
    let colAnchored = (cursor < bytes.count && bytes[cursor] == 0x24)
    if colAnchored { cursor += 1 }

    var letterCount = 0
    var colValue = 0
    while cursor < bytes.count && letterCount < 4 {
      let b = bytes[cursor]
      let upper = (b >= 0x61 && b <= 0x7A) ? (b - 0x20) : b
      guard upper >= 0x41 && upper <= 0x5A else { break }
      letterCount += 1
      colValue = colValue * 26 + Int(upper - 0x41) + 1
      cursor += 1
    }
    guard letterCount >= 1 && letterCount <= 3 else { return nil }

    let rowAnchored = (cursor < bytes.count && bytes[cursor] == 0x24)
    if rowAnchored { cursor += 1 }

    var digitCount = 0
    var rowOneBased = 0
    while cursor < bytes.count && digitCount < 8 {
      let b = bytes[cursor]
      guard b >= 0x30 && b <= 0x39 else { break }
      digitCount += 1
      rowOneBased = rowOneBased * 10 + Int(b - 0x30)
      cursor += 1
    }
    guard digitCount >= 1 && digitCount <= 7 else { return nil }

    if cursor < bytes.count {
      let nextByte = bytes[cursor]
      if isIdentifierOrDollarByte(nextByte) || nextByte == 0x28 || nextByte == 0x21 {
        return nil
      }
    }

    let zeroBasedCol = colValue - 1
    let zeroBasedRow = rowOneBased - 1
    guard zeroBasedCol >= 0, zeroBasedCol <= CellAddress.maxZeroBasedColumn,
      zeroBasedRow >= 0, zeroBasedRow <= CellAddress.maxZeroBasedRow
    else {
      return nil
    }

    return ParsedCellRef(
      col: zeroBasedCol,
      row: zeroBasedRow,
      colAnchored: colAnchored,
      rowAnchored: rowAnchored,
      nextIndex: cursor
    )
  }

  private static func shiftFormulaReferencesAtAxis(
    _ formula: String,
    isRowAxis: Bool,
    thresholdIndex: Int,
    delta: Int
  ) -> String {
    let bytes = Array(formula.utf8)
    var outBytes: [UInt8] = []
    outBytes.reserveCapacity(bytes.count + 16)

    var i = 0
    while i < bytes.count {
      let b = bytes[i]
      if b == 0x22 || b == 0x27 {
        i = consumeQuotedLiteral(in: bytes, from: i, into: &outBytes)
        continue
      }

      let atBoundary = (i == 0) || !isIdentifierOrDollarByte(bytes[i - 1])
      if atBoundary, let firstRef = parseRawCellRef(in: bytes, at: i) {
        let isCrossSheet = (i > 0 && bytes[i - 1] == 0x21)
        if firstRef.nextIndex < bytes.count,
          bytes[firstRef.nextIndex] == 0x3A,
          let secondRef = parseRawCellRef(in: bytes, at: firstRef.nextIndex + 1)
        {
          if isCrossSheet {
            outBytes.append(contentsOf: bytes[i..<secondRef.nextIndex])
            i = secondRef.nextIndex
            continue
          }
          let shiftedRange = shiftLocalRangeAtAxis(
            first: firstRef,
            second: secondRef,
            isRowAxis: isRowAxis,
            thresholdIndex: thresholdIndex,
            delta: delta
          )
          outBytes.append(contentsOf: shiftedRange.utf8)
          i = secondRef.nextIndex
          continue
        }

        if isCrossSheet {
          outBytes.append(contentsOf: bytes[i..<firstRef.nextIndex])
          i = firstRef.nextIndex
          continue
        }

        let shiftedSingle = shiftLocalSingleCellAtAxis(
          firstRef,
          isRowAxis: isRowAxis,
          thresholdIndex: thresholdIndex,
          delta: delta
        )
        outBytes.append(contentsOf: shiftedSingle.utf8)
        i = firstRef.nextIndex
        continue
      }

      outBytes.append(b)
      i += 1
    }
    return String(decoding: outBytes, as: UTF8.self)
  }

  private static func shiftLocalRangeAtAxis(
    first: ParsedCellRef,
    second: ParsedCellRef,
    isRowAxis: Bool,
    thresholdIndex: Int,
    delta: Int
  ) -> String {
    let a1 = isRowAxis ? first.row : first.col
    let a2 = isRowAxis ? second.row : second.col
    let minAxis = min(a1, a2)
    let maxAxis = max(a1, a2)

    let nextA1: Int
    let nextA2: Int
    if delta < 0 {
      if minAxis == thresholdIndex && maxAxis == thresholdIndex {
        return "#REF!"
      }
      let newMinAxis: Int
      let newMaxAxis: Int
      if thresholdIndex >= minAxis && thresholdIndex <= maxAxis {
        newMinAxis = minAxis > thresholdIndex ? minAxis - 1 : minAxis
        newMaxAxis = maxAxis - 1
      } else if maxAxis < thresholdIndex {
        newMinAxis = minAxis
        newMaxAxis = maxAxis
      } else {
        newMinAxis = max(0, minAxis + delta)
        newMaxAxis = max(0, maxAxis + delta)
      }
      nextA1 = (a1 <= a2) ? newMinAxis : newMaxAxis
      nextA2 = (a1 <= a2) ? newMaxAxis : newMinAxis
    } else {
      nextA1 = a1 >= thresholdIndex ? a1 + delta : a1
      nextA2 = a2 >= thresholdIndex ? a2 + delta : a2
    }
    let startStr =
      isRowAxis
      ? first.formatted(col: first.col, row: nextA1)
      : first.formatted(col: nextA1, row: first.row)
    let endStr =
      isRowAxis
      ? second.formatted(col: second.col, row: nextA2)
      : second.formatted(col: nextA2, row: second.row)
    return "\(startStr):\(endStr)"
  }

  private static func shiftLocalSingleCellAtAxis(
    _ ref: ParsedCellRef,
    isRowAxis: Bool,
    thresholdIndex: Int,
    delta: Int
  ) -> String {
    let axisVal = isRowAxis ? ref.row : ref.col
    if delta < 0 && axisVal == thresholdIndex {
      return "#REF!"
    }
    let nextAxisVal: Int
    if delta < 0 && axisVal > thresholdIndex {
      nextAxisVal = max(0, axisVal + delta)
    } else if delta > 0 && axisVal >= thresholdIndex {
      nextAxisVal = axisVal + delta
    } else {
      nextAxisVal = axisVal
    }
    return isRowAxis
      ? ref.formatted(col: ref.col, row: nextAxisVal)
      : ref.formatted(col: nextAxisVal, row: ref.row)
  }

  // MARK: - Enterprise Data Cleansing & Multi-Column Row Sorting

  /// Sorts data rows in the active worksheet by `primaryColumn` (`ascending`) and optional `secondaryColumn` (`secondaryAscending`), preserving row 0 when `hasHeader` is `true`.
  public mutating func sortRows(
    primaryColumn: Int,
    ascending: Bool = true,
    secondaryColumn: Int? = nil,
    secondaryAscending: Bool = true,
    hasHeader: Bool = true
  ) {
    sortRows(
      byColumn: primaryColumn,
      ascending: ascending,
      hasHeader: hasHeader,
      secondaryColumn: secondaryColumn,
      secondaryAscending: secondaryAscending
    )
  }

  /// Sorts data rows in the active worksheet by `column` (`ascending`), breaking ties with optional `secondaryColumn` (`secondaryAscending`), and preserving row 0 when `hasHeader` is `true`.
  public mutating func sortRows(
    byColumn column: Int,
    ascending: Bool = true,
    hasHeader: Bool = true,
    secondaryColumn: Int? = nil,
    secondaryAscending: Bool = true
  ) {
    guard !cellsByCoordinate.isEmpty,
      column >= 0,
      column <= CellAddress.maxZeroBasedColumn
    else {
      return
    }
    let bounds = detectedDataRange()
    let minRow = max(0, min(bounds.start.row, bounds.end.row))
    let maxRow = min(CellAddress.maxZeroBasedRow, max(bounds.start.row, bounds.end.row))
    let startRow = hasHeader ? (minRow + 1) : minRow
    guard startRow < maxRow else { return }
    let (rowCount, overflow) = (maxRow - startRow).addingReportingOverflow(1)
    guard !overflow, rowCount > 1, rowCount <= CellRange.maxExpandedCellCount else {
      return
    }

    let rowIndices = Array(startRow...maxRow)
    var context = EvaluationContext()
    var primaryValues: [Int: EvaluatedCellValue] = [:]
    primaryValues.reserveCapacity(rowCount)
    var secondaryValues: [Int: EvaluatedCellValue] = [:]
    let validSecondaryCol: Int? =
      if let secCol = secondaryColumn, secCol >= 0, secCol <= CellAddress.maxZeroBasedColumn {
        secCol
      } else {
        nil
      }
    if validSecondaryCol != nil {
      secondaryValues.reserveCapacity(rowCount)
    }

    for r in rowIndices {
      let pCoord = PackedCellCoordinate(row: r, column: column)
      primaryValues[r] =
        cellsByCoordinate[pCoord] != nil
        ? evaluateInternal(at: pCoord, context: &context, depth: 0)
        : .empty
      if let secCol = validSecondaryCol {
        let sCoord = PackedCellCoordinate(row: r, column: secCol)
        secondaryValues[r] =
          cellsByCoordinate[sCoord] != nil
          ? evaluateInternal(at: sCoord, context: &context, depth: 0)
          : .empty
      }
    }

    let sortedRowIndices = rowIndices.sorted { rA, rB in
      let vA = primaryValues[rA] ?? .empty
      let vB = primaryValues[rB] ?? .empty
      let primaryOrder = Self.compareEvaluatedValuesForSort(vA, vB, ascending: ascending)
      if primaryOrder != .orderedSame {
        return primaryOrder == .orderedAscending
      }
      if validSecondaryCol != nil {
        let sA = secondaryValues[rA] ?? .empty
        let sB = secondaryValues[rB] ?? .empty
        let secondaryOrder = Self.compareEvaluatedValuesForSort(
          sA,
          sB,
          ascending: secondaryAscending
        )
        if secondaryOrder != .orderedSame {
          return secondaryOrder == .orderedAscending
        }
      }
      return rA < rB
    }

    guard sortedRowIndices != rowIndices else { return }

    var oldToNewRow: [Int: Int] = [:]
    oldToNewRow.reserveCapacity(rowCount)
    for (offset, oldRow) in sortedRowIndices.enumerated() {
      oldToNewRow[oldRow] = startRow + offset
    }

    var nextCells: [PackedCellCoordinate: String] = [:]
    nextCells.reserveCapacity(cellsByCoordinate.count)
    for (coord, rawVal) in cellsByCoordinate {
      let targetRow = oldToNewRow[coord.row] ?? coord.row
      let rowDelta = targetRow - coord.row
      if rowDelta != 0 && rawVal.trimmingCharacters(in: .whitespacesAndNewlines).hasPrefix("=") {
        nextCells[PackedCellCoordinate(row: targetRow, column: coord.column)] =
          Self.shiftFormulaReferences(rawVal, rowDelta: rowDelta, columnDelta: 0)
      } else {
        nextCells[PackedCellCoordinate(row: targetRow, column: coord.column)] = rawVal
      }
    }
    cellsByCoordinate = nextCells

    var nextStyles: [PackedCellCoordinate: CellStyle] = [:]
    nextStyles.reserveCapacity(cellStyles.count)
    for (coord, style) in cellStyles {
      let targetRow = oldToNewRow[coord.row] ?? coord.row
      nextStyles[PackedCellCoordinate(row: targetRow, column: coord.column)] = style
    }
    cellStyles = nextStyles

    var nextComments: [PackedCellCoordinate: String] = [:]
    nextComments.reserveCapacity(sheetComments.count)
    for (coord, comment) in sheetComments {
      let targetRow = oldToNewRow[coord.row] ?? coord.row
      nextComments[PackedCellCoordinate(row: targetRow, column: coord.column)] = comment
    }
    sheetComments = nextComments

    var nextRowHeights: [Int: Double] = [:]
    nextRowHeights.reserveCapacity(rowHeights.count)
    for (r, height) in rowHeights {
      let targetRow = oldToNewRow[r] ?? r
      nextRowHeights[targetRow] = height
    }
    rowHeights = nextRowHeights

    compiledASTs.removeAll(keepingCapacity: true)
    for (coord, rawVal) in cellsByCoordinate where !rawVal.isEmpty {
      compiledASTs[coord] = Self.compileCellInput(rawVal)
    }
  }

  private static func compareEvaluatedValuesForSort(
    _ lhs: EvaluatedCellValue,
    _ rhs: EvaluatedCellValue,
    ascending: Bool
  ) -> ComparisonResult {
    if lhs == .empty && rhs == .empty {
      return .orderedSame
    }
    if lhs == .empty {
      return .orderedDescending
    }
    if rhs == .empty {
      return .orderedAscending
    }
    if let lNum = lhs.numericValue, let rNum = rhs.numericValue {
      if lNum == rNum {
        return .orderedSame
      }
      if ascending {
        return lNum < rNum ? .orderedAscending : .orderedDescending
      } else {
        return lNum > rNum ? .orderedAscending : .orderedDescending
      }
    }
    let lText = lhs.description
    let rText = rhs.description
    let cmp = lText.caseInsensitiveCompare(rText)
    if cmp == .orderedSame {
      return .orderedSame
    }
    if ascending {
      return cmp
    } else {
      return cmp == .orderedAscending ? .orderedDescending : .orderedAscending
    }
  }

  /// Removes duplicate rows within `range` (or `detectedDataRange()` when `nil`), preserving the first occurrence of each non-empty row signature and returning the number of deleted rows.
  @discardableResult
  public mutating func removeDuplicateRows(in range: CellRange? = nil) -> Int {
    let target = range ?? detectedDataRange()
    let minRow = max(0, min(target.start.row, target.end.row))
    let maxRow = min(CellAddress.maxZeroBasedRow, max(target.start.row, target.end.row))
    let minCol = max(0, min(target.start.column, target.end.column))
    let maxCol = min(CellAddress.maxZeroBasedColumn, max(target.start.column, target.end.column))
    guard minRow <= maxRow, minCol <= maxCol else { return 0 }
    let (rowSpan, rowOverflow) = (maxRow - minRow).addingReportingOverflow(1)
    let (colSpan, colOverflow) = (maxCol - minCol).addingReportingOverflow(1)
    let (totalCells, mulOverflow) = rowSpan.multipliedReportingOverflow(by: colSpan)
    guard !rowOverflow, !colOverflow, !mulOverflow,
      totalCells > 0, totalCells <= CellRange.maxExpandedCellCount
    else {
      return 0
    }

    var seenKeys: Set<[String]> = []
    var duplicateRowsToDelete: [Int] = []

    for r in minRow...maxRow {
      let key = (minCol...maxCol).map { c in
        (cellsByCoordinate[PackedCellCoordinate(row: r, column: c)] ?? "")
          .trimmingCharacters(in: .whitespacesAndNewlines)
      }
      if key.allSatisfy(\.isEmpty) {
        continue
      }
      if seenKeys.contains(key) {
        duplicateRowsToDelete.append(r)
      } else {
        seenKeys.insert(key)
      }
    }

    for r in duplicateRowsToDelete.sorted(by: >) {
      deleteRow(at: r)
    }
    return duplicateRowsToDelete.count
  }

  /// Splits text cells in `column` by `delimiter` across adjacent columns to the right and returns the number of split rows.
  @discardableResult
  public mutating func splitColumnByDelimiter(
    column: Int,
    delimiter: String,
    maxRow: Int? = nil
  ) -> Int {
    guard !delimiter.isEmpty,
      column >= 0,
      column <= CellAddress.maxZeroBasedColumn
    else {
      return 0
    }
    let rawUpperRow = maxRow ?? detectedDataRange().end.row
    let upperRow = min(
      max(0, rawUpperRow),
      min(CellAddress.maxZeroBasedRow, CellRange.maxExpandedCellCount - 1)
    )
    var splitRowCount = 0

    for r in 0...upperRow {
      let coord = PackedCellCoordinate(row: r, column: column)
      guard let raw = cellsByCoordinate[coord],
        !raw.hasPrefix("="),
        raw.contains(delimiter)
      else {
        continue
      }
      let parts = raw.components(separatedBy: delimiter).map {
        $0.trimmingCharacters(in: .whitespaces)
      }
      guard parts.count > 1 else { continue }
      for (offset, part) in parts.enumerated()
      where column + offset <= CellAddress.maxZeroBasedColumn {
        let safePart = part.hasPrefix("=") ? ("'" + part) : part
        cellsByCoordinate[PackedCellCoordinate(row: r, column: column + offset)] = safePart
      }
      splitRowCount += 1
    }

    if splitRowCount > 0 {
      compiledASTs.removeAll(keepingCapacity: true)
      for (coord, rawVal) in cellsByCoordinate where !rawVal.isEmpty {
        compiledASTs[coord] = Self.compileCellInput(rawVal)
      }
    }
    return splitRowCount
  }

  // MARK: - Goal Seek Numerical Solver & Sparkline Normalization

  /// Solves for the numeric input in `variableCell` that makes `targetCell` evaluate to `targetValue` within `tolerance`.
  @discardableResult
  public mutating func goalSeek(
    targetCell: String,
    targetValue: Double,
    variableCell: String,
    maxIterations: Int = 80,
    tolerance: Double = 1e-6
  ) throws -> Double? {
    let targetCoord = try CellAddress(a1Notation: targetCell).packedCoordinate
    let varCoord = try CellAddress(a1Notation: variableCell).packedCoordinate
    let originalRaw = cellsByCoordinate[varCoord] ?? ""

    func evaluateError(at x: Double) -> Double? {
      cellsByCoordinate[varCoord] = String(x)
      compiledASTs[varCoord] = .literalNumber(x)
      guard let y = evaluate(at: targetCoord).numericValue, y.isFinite else { return nil }
      return y - targetValue
    }

    let x0 = evaluate(at: varCoord).numericValue ?? 0.0
    guard var prevErr = evaluateError(at: x0) else {
      try setCell(variableCell, rawValue: originalRaw)
      return nil
    }
    if abs(prevErr) <= tolerance {
      let solved = Self.cleanSolvedDouble(x0, tolerance: tolerance)
      try setCell(variableCell, rawValue: Self.formatSolvedDouble(solved))
      return solved
    }

    var prevX = x0
    var currX = abs(x0) > 1e-3 ? x0 * 1.1 : 10.0

    for _ in 0..<max(1, maxIterations) {
      guard let currErr = evaluateError(at: currX) else { break }
      if abs(currErr) <= tolerance {
        let candidate = Self.cleanSolvedDouble(currX, tolerance: tolerance)
        let finalX =
          (evaluateError(at: candidate).map { abs($0) <= tolerance } ?? false) ? candidate : currX
        try setCell(variableCell, rawValue: Self.formatSolvedDouble(finalX))
        return finalX
      }

      let denom = currErr - prevErr
      let nextX: Double
      if abs(denom) > 1e-14 {
        nextX = currX - currErr * (currX - prevX) / denom
      } else {
        nextX = currX + (currX == 0 ? 1.0 : currX * 0.5)
      }
      guard nextX.isFinite else { break }
      prevX = currX
      prevErr = currErr
      currX = nextX
    }

    try setCell(variableCell, rawValue: originalRaw)
    return nil
  }

  private static func cleanSolvedDouble(_ value: Double, tolerance: Double) -> Double {
    let rounded = value.rounded()
    if abs(value - rounded) <= max(tolerance, 1e-7) {
      return rounded
    }
    return value
  }

  private static func formatSolvedDouble(_ value: Double) -> String {
    if value.rounded() == value, let intVal = safeInt(value) {
      return String(intVal)
    }
    return String(value)
  }

  /// Collects all numeric values in `range` (in coordinate order) and normalizes them into `0.0...1.0`.
  public func sparklineNormalizedValues(
    for range: CellRange,
    using preEvaluated: [PackedCellCoordinate: EvaluatedCellValue]? = nil
  ) -> [Double] {
    var context = EvaluationContext()
    var nums: [Double] = []
    range.forEachCoordinate { coord in
      let evaluated =
        preEvaluated?[coord]
        ?? (cellsByCoordinate[coord] != nil
          ? evaluateInternal(at: coord, context: &context, depth: 0) : .empty)
      if case .number(let num) = evaluated, num.isFinite {
        nums.append(num)
      }
    }
    guard let minVal = nums.min(), let maxVal = nums.max() else { return [] }
    let span = maxVal - minVal
    guard span > 1e-9 else {
      return Array(repeating: 0.5, count: nums.count)
    }
    return nums.map { ($0 - minVal) / span }
  }
}
