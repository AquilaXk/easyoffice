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
            let evaluated = preEvaluated?[coord]
                ?? (cellsByCoordinate[coord] != nil ? evaluateInternal(at: coord, context: &context, depth: 0) : .empty)
            if case let .number(num) = evaluated, num.isFinite {
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
    public mutating func updateCellStyleInRange(_ range: CellRange, transform: (inout CellStyle) -> Void) {
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
        let clampedMaxRow = max(0, min(maxRow, min(CellAddress.maxZeroBasedRow, CellRange.maxExpandedCellCount)))
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return Set(0...clampedMaxRow) }
        var result: Set<Int> = [0]
        guard clampedMaxRow >= 1, column >= 0, column <= CellAddress.maxZeroBasedColumn else { return result }
        var context = EvaluationContext()
        for r in 1...clampedMaxRow {
            let coord = PackedCellCoordinate(row: r, column: column)
            if let raw = cellsByCoordinate[coord], raw.range(of: trimmed, options: [.caseInsensitive]) != nil {
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
            if formatted.range(of: trimmed, options: [.caseInsensitive]) != nil ||
               evaluated.description.range(of: trimmed, options: [.caseInsensitive]) != nil {
                result.insert(r)
            }
        }
        return result
    }

    // MARK: - Formula Relative/Absolute ($) Reference Shifter

    /// Shifts relative cell references in `formula` by `(rowDelta, columnDelta)` while respecting `$` anchors, string literals, function names, and sheet prefixes.
    public static func shiftFormulaReferences(_ formula: String, rowDelta: Int, columnDelta: Int) -> String {
        let bytes = Array(formula.utf8)
        var outBytes: [UInt8] = []
        outBytes.reserveCapacity(bytes.count + 16)

        var i = 0
        while i < bytes.count {
            let b = bytes[i]
            if b == 0x22 || b == 0x27 {
                let quote = b
                outBytes.append(quote)
                i += 1
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
                continue
            }

            let atBoundary = (i == 0) || !isIdentifierOrDollarByte(bytes[i - 1])
            if atBoundary,
               let (replacement, nextIndex) = matchAndShiftCellRef(in: bytes, at: i, rowDelta: rowDelta, columnDelta: columnDelta) {
                outBytes.append(contentsOf: replacement.utf8)
                i = nextIndex
                continue
            }

            outBytes.append(b)
            i += 1
        }
        return String(decoding: outBytes, as: UTF8.self)
    }

    private static func isIdentifierOrDollarByte(_ b: UInt8) -> Bool {
        (b >= 0x41 && b <= 0x5A) || (b >= 0x61 && b <= 0x7A) ||
            (b >= 0x30 && b <= 0x39) || b == 0x5F || b == 0x24
    }

    private static func matchAndShiftCellRef(
        in bytes: [UInt8],
        at start: Int,
        rowDelta: Int,
        columnDelta: Int
    ) -> (String, Int)? {
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
        while cursor < bytes.count && digitCount < 6 {
            let b = bytes[cursor]
            guard b >= 0x30 && b <= 0x39 else { break }
            digitCount += 1
            rowOneBased = rowOneBased * 10 + Int(b - 0x30)
            cursor += 1
        }
        guard digitCount >= 1 && digitCount <= 5 else { return nil }

        if cursor < bytes.count {
            let nextByte = bytes[cursor]
            if isIdentifierOrDollarByte(nextByte) || nextByte == 0x28 || nextByte == 0x21 {
                return nil
            }
        }

        let zeroBasedCol = colValue - 1
        guard zeroBasedCol >= 0, zeroBasedCol <= CellAddress.maxZeroBasedColumn,
              rowOneBased >= 1, (rowOneBased - 1) <= CellAddress.maxZeroBasedRow else {
            return nil
        }

        let newCol = colAnchored ? zeroBasedCol : min(max(0, zeroBasedCol + columnDelta), CellAddress.maxZeroBasedColumn)
        let newRowOneBased = rowAnchored ? rowOneBased : min(max(1, rowOneBased + rowDelta), CellAddress.maxZeroBasedRow + 1)
        let shiftedToken = "\(colAnchored ? "$" : "")\(CellAddress.columnLabel(for: newCol))\(rowAnchored ? "$" : "")\(newRowOneBased)"
        return (shiftedToken, cursor)
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

        let isDownward = (srcMinCol == dstMinCol && srcMaxCol == dstMaxCol &&
            dstMinRow >= srcMinRow && dstMinRow <= srcMaxRow + 1 && dstMaxRow > srcMaxRow)
        let isRightward = (srcMinRow == dstMinRow && srcMaxRow == dstMaxRow &&
            dstMinCol >= srcMinCol && dstMinCol <= srcMaxCol + 1 && dstMaxCol > srcMaxCol)

        let dstCellCount = (dstMaxRow - dstMinRow + 1) * (dstMaxCol - dstMinCol + 1)
        guard (isDownward || isRightward), dstCellCount <= CellRange.maxExpandedCellCount else {
            throw EasyOfficeError.invalidCellRange("\(sourceRange)->\(targetRange)")
        }

        if isDownward {
            let srcRowCount = srcMaxRow - srcMinRow + 1
            for c in srcMinCol...srcMaxCol {
                let srcRaws = (srcMinRow...srcMaxRow).map { cellsByCoordinate[PackedCellCoordinate(row: $0, column: c)] ?? "" }
                let srcStyles = (srcMinRow...srcMaxRow).map { cellStyles[PackedCellCoordinate(row: $0, column: c)] ?? CellStyle() }
                let (allNumeric, baseVal, linearDelta) = Self.parseNumericProgression(srcRaws)
                for r in (srcMaxRow + 1)...dstMaxRow {
                    let stepOffset = r - srcMinRow
                    let srcIdx = stepOffset % srcRowCount
                    let srcRow = srcMinRow + srcIdx
                    setCellStyle(srcStyles[srcIdx], at: PackedCellCoordinate(row: r, column: c))
                    let filled = Self.computeAutoFillValue(
                        raw: srcRaws[srcIdx], stepFromStart: stepOffset, deltaFromSource: r - srcRow,
                        rowDelta: r - srcRow, columnDelta: 0, allNumeric: allNumeric, baseVal: baseVal, linearDelta: linearDelta
                    )
                    try setCell(row: r, column: c, rawValue: filled)
                }
            }
        } else if isRightward {
            let srcColCount = srcMaxCol - srcMinCol + 1
            for r in srcMinRow...srcMaxRow {
                let srcRaws = (srcMinCol...srcMaxCol).map { cellsByCoordinate[PackedCellCoordinate(row: r, column: $0)] ?? "" }
                let srcStyles = (srcMinCol...srcMaxCol).map { cellStyles[PackedCellCoordinate(row: r, column: $0)] ?? CellStyle() }
                let (allNumeric, baseVal, linearDelta) = Self.parseNumericProgression(srcRaws)
                for c in (srcMaxCol + 1)...dstMaxCol {
                    let stepOffset = c - srcMinCol
                    let srcIdx = stepOffset % srcColCount
                    let srcCol = srcMinCol + srcIdx
                    setCellStyle(srcStyles[srcIdx], at: PackedCellCoordinate(row: r, column: c))
                    let filled = Self.computeAutoFillValue(
                        raw: srcRaws[srcIdx], stepFromStart: stepOffset, deltaFromSource: c - srcCol,
                        rowDelta: 0, columnDelta: c - srcCol, allNumeric: allNumeric, baseVal: baseVal, linearDelta: linearDelta
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
            guard !trimmed.hasPrefix("="), let val = Double(trimmed), val.isFinite else { return (false, 0.0, 0.0) }
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
            if extrapolated.rounded() == extrapolated, let intVal = safeInt(extrapolated) { return String(intVal) }
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
        return String(String.UnicodeScalarView(scalars[0..<start])) + formattedDigits + String(String.UnicodeScalarView(scalars[end..<scalars.count]))
    }
}
