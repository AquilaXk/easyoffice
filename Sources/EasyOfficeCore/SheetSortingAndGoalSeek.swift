import Foundation

extension WorksheetModel {


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
    let minCol = max(0, min(bounds.start.column, bounds.end.column))
    let maxCol = min(CellAddress.maxZeroBasedColumn, max(bounds.start.column, bounds.end.column))
    let startRow = hasHeader ? (minRow + 1) : minRow
    var effectiveMaxRow = maxRow
    while effectiveMaxRow > startRow {
      if isSummaryFooterRow(at: effectiveMaxRow, minCol: minCol, maxCol: maxCol) {
        effectiveMaxRow -= 1
      } else {
        break
      }
    }
    guard startRow < effectiveMaxRow else { return }
    let (rowCount, overflow) = (effectiveMaxRow - startRow).addingReportingOverflow(1)
    guard !overflow, rowCount > 1, rowCount <= CellRange.maxExpandedCellCount else {
      return
    }

    let rowIndices = Array(startRow...effectiveMaxRow)
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
          Self.shiftSameRowFormulaReferences(rawVal, fromRow: coord.row, toRow: targetRow)
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

  private func isSummaryFooterRow(at row: Int, minCol: Int, maxCol: Int) -> Bool {
    guard minCol <= maxCol else { return false }
    for c in minCol...maxCol {
      guard let raw = cellsByCoordinate[PackedCellCoordinate(row: row, column: c)] else {
        continue
      }
      let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
      guard !trimmed.isEmpty else { continue }
      let upper = trimmed.uppercased()
      if upper == "합계" || upper == "총계" || upper == "소계" || upper == "TOTAL"
        || upper.hasPrefix("합계 ") || upper.hasPrefix("총계 ") || upper.hasPrefix("소계 ")
        || upper.hasPrefix("TOTAL ") || upper.hasPrefix("합계(") || upper.hasPrefix("총계(")
      {
        return true
      }
      return false
    }
    return false
  }

  private static func shiftSameRowFormulaReferences(
    _ formula: String,
    fromRow oldRow: Int,
    toRow newRow: Int
  ) -> String {
    guard oldRow != newRow else { return formula }
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
          if !isCrossSheet && firstRef.row == oldRow && secondRef.row == oldRow {
            let r1 = firstRef.rowAnchored ? firstRef.row : newRow
            let r2 = secondRef.rowAnchored ? secondRef.row : newRow
            let replaced =
              "\(firstRef.formatted(col: firstRef.col, row: r1)):\(secondRef.formatted(col: secondRef.col, row: r2))"
            outBytes.append(contentsOf: replaced.utf8)
          } else {
            outBytes.append(contentsOf: bytes[i..<secondRef.nextIndex])
          }
          i = secondRef.nextIndex
          continue
        }

        if !isCrossSheet && !firstRef.rowAnchored && firstRef.row == oldRow {
          let replaced = firstRef.formatted(col: firstRef.col, row: newRow)
          outBytes.append(contentsOf: replaced.utf8)
        } else {
          outBytes.append(contentsOf: bytes[i..<firstRef.nextIndex])
        }
        i = firstRef.nextIndex
        continue
      }

      outBytes.append(b)
      i += 1
    }
    return String(decoding: outBytes, as: UTF8.self)
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
        !raw.trimmingCharacters(in: .whitespacesAndNewlines).hasPrefix("="),
        raw.contains(delimiter)
      else {
        continue
      }
      let parts = raw.components(separatedBy: delimiter).map {
        $0.trimmingCharacters(in: .whitespacesAndNewlines)
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
