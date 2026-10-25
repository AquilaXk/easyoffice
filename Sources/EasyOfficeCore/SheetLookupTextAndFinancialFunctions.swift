import Foundation

extension WorksheetModel {

  internal func extractCellRange(from ast: FormulaAST) -> CellRange? {
    if let range = ast.asCellRange {
      return range
    }
    if case .namedRangeRef(let rangeName) = ast,
      let notation = resolveNamedRange(rangeName),
      let range = try? CellRange(notation: notation)
    {
      return range
    }
    return nil
  }

  internal enum OffsetResolutionResult {
    case success(sheetName: String?, range: CellRange)
    case failure(String)
  }

  internal func resolveOffsetRange(
    args: [FormulaAST],
    frame: inout EvalFrame
  ) -> OffsetResolutionResult {
    guard (3...5).contains(args.count) else {
      return .failure("#VALUE!")
    }
    if let sheetName = args[0].targetSheetName, !hasSheet(named: sheetName) {
      return .failure("#REF!")
    }
    if case .namedRangeRef(let rangeName) = args[0], resolveNamedRange(rangeName) == nil {
      return .failure("#NAME?")
    }
    guard let baseRange = extractCellRange(from: args[0]) else {
      return .failure("#VALUE!")
    }

    let rVal = evaluateAST(args[1], context: &frame.context, depth: frame.depth + 1)
    if case .error(let code) = rVal {
      return .failure(code)
    }
    let cVal = evaluateAST(args[2], context: &frame.context, depth: frame.depth + 1)
    if case .error(let code) = cVal {
      return .failure(code)
    }
    guard let rNum = rVal.numericValue,
      let rowDelta = Self.safeInt(rNum),
      let cNum = cVal.numericValue,
      let colDelta = Self.safeInt(cNum)
    else {
      return .failure("#VALUE!")
    }

    guard (-65535...65535).contains(rowDelta),
      (-65535...65535).contains(colDelta)
    else {
      return .failure("#REF!")
    }

    let baseRow = min(baseRange.start.row, baseRange.end.row)
    let baseCol = min(baseRange.start.column, baseRange.end.column)
    let targetRow = baseRow + rowDelta
    let targetCol = baseCol + colDelta
    guard targetRow >= 0,
      targetCol >= 0,
      targetRow <= 65535,
      targetCol <= 65535,
      targetCol <= CellAddress.maxZeroBasedColumn
    else {
      return .failure("#REF!")
    }

    let defaultHeight = abs(baseRange.end.row - baseRange.start.row) + 1
    let defaultWidth = abs(baseRange.end.column - baseRange.start.column) + 1
    let height: Int
    if args.count >= 4 {
      let hVal = evaluateAST(args[3], context: &frame.context, depth: frame.depth + 1)
      if case .error(let code) = hVal {
        return .failure(code)
      }
      guard let hNum = hVal.numericValue, let hInt = Self.safeInt(hNum) else {
        return .failure("#VALUE!")
      }
      guard (1...65536).contains(hInt) else {
        return .failure("#REF!")
      }
      height = hInt
    } else {
      height = defaultHeight
    }

    let width: Int
    if args.count == 5 {
      let wVal = evaluateAST(args[4], context: &frame.context, depth: frame.depth + 1)
      if case .error(let code) = wVal {
        return .failure(code)
      }
      guard let wNum = wVal.numericValue, let wInt = Self.safeInt(wNum) else {
        return .failure("#VALUE!")
      }
      guard (1...65536).contains(wInt) else {
        return .failure("#REF!")
      }
      width = wInt
    } else {
      width = defaultWidth
    }

    let endRow = targetRow + height - 1
    let endCol = targetCol + width - 1
    guard endRow >= 0,
      endCol >= 0,
      endRow <= 65535,
      endCol <= 65535,
      endCol <= CellAddress.maxZeroBasedColumn,
      height * width <= CellRange.maxExpandedCellCount
    else {
      return .failure("#REF!")
    }

    let startAddr = CellAddress(packed: PackedCellCoordinate(row: targetRow, column: targetCol))
    let endAddr = CellAddress(packed: PackedCellCoordinate(row: endRow, column: endCol))
    return .success(
      sheetName: args[0].targetSheetName,
      range: CellRange(start: startAddr, end: endAddr)
    )
  }

  // MARK: - 4. Lookup & Reference Functions

  internal func evaluateLookupAndRefFunction(name: String, frame: inout EvalFrame)
    -> EvaluatedCellValue?
  {
    switch name {
    case "OFFSET":
      switch resolveOffsetRange(args: frame.args, frame: &frame) {
      case .failure(let err):
        return .error(err)
      case .success(let sheetName, let resolvedRange):
        let coord = resolvedRange.start.packedCoordinate
        let val: EvaluatedCellValue
        if let sheetName {
          val = evaluateOnSheet(
            named: sheetName, at: coord, context: &frame.context, depth: frame.depth + 1)
        } else {
          val = evaluateInternal(at: coord, context: &frame.context, depth: frame.depth + 1)
        }
        if case .empty = val {
          return .number(0.0)
        }
        return val
      }

    case "VLOOKUP":
      guard (3...4).contains(frame.count),
        let range = extractCellRange(from: frame.args[1]),
        let colIndexVal = frame.arg(2, in: self).numericValue,
        let rawColIdx = Self.safeInt(colIndexVal)
      else { return .error("#VALUE!") }
      let sheetName = frame.args[1].targetSheetName
      if let sheetName, !hasSheet(named: sheetName) {
        return .error("#REF!")
      }
      guard let bounds = rangeBounds(range) else { return .error("#REF!") }
      let colIdx = rawColIdx - 1
      guard colIdx >= 0, colIdx < bounds.colCount else {
        return .error("#REF!")
      }
      let lookupVal = frame.arg(0, in: self)
      let isApprox = frame.count == 4 ? (frame.arg(3, in: self).booleanValue ?? false) : false
      let targetCol = bounds.minCol + colIdx
      var approxBestRow: Int?
      var approxBestNum = -Double.infinity
      let targetNum = isApprox ? lookupVal.numericValue : nil

      for r in bounds.minRow...bounds.maxRow {
        let keyVal = evaluateSingleCellInRange(
          onSheet: sheetName,
          row: r,
          column: bounds.minCol,
          context: &frame.context,
          depth: frame.depth + 1
        )
        if Self.cellValuesEqual(keyVal, lookupVal) {
          if colIdx == 0 { return keyVal }
          return evaluateSingleCellInRange(
            onSheet: sheetName,
            row: r,
            column: targetCol,
            context: &frame.context,
            depth: frame.depth + 1
          )
        }
        if let targetNum, let n = keyVal.numericValue, n <= targetNum, n >= approxBestNum {
          approxBestNum = n
          approxBestRow = r
        }
      }
      if let matchedRow = approxBestRow {
        return evaluateSingleCellInRange(
          onSheet: sheetName,
          row: matchedRow,
          column: targetCol,
          context: &frame.context,
          depth: frame.depth + 1
        )
      }
      return .error("#N/A")

    case "HLOOKUP":
      guard (3...4).contains(frame.count),
        let range = extractCellRange(from: frame.args[1]),
        let rowIndexVal = frame.arg(2, in: self).numericValue,
        let rawRowIdx = Self.safeInt(rowIndexVal)
      else { return .error("#VALUE!") }
      let sheetName = frame.args[1].targetSheetName
      if let sheetName, !hasSheet(named: sheetName) {
        return .error("#REF!")
      }
      guard let bounds = rangeBounds(range) else { return .error("#REF!") }
      let rowIdx = rawRowIdx - 1
      guard rowIdx >= 0, rowIdx < bounds.rowCount else {
        return .error("#REF!")
      }
      let lookupVal = frame.arg(0, in: self)
      let isApprox = frame.count == 4 ? (frame.arg(3, in: self).booleanValue ?? false) : false
      let targetRow = bounds.minRow + rowIdx
      var approxBestCol: Int?
      var approxBestNum = -Double.infinity
      let targetNum = isApprox ? lookupVal.numericValue : nil

      for c in bounds.minCol...bounds.maxCol {
        let keyVal = evaluateSingleCellInRange(
          onSheet: sheetName,
          row: bounds.minRow,
          column: c,
          context: &frame.context,
          depth: frame.depth + 1
        )
        if Self.cellValuesEqual(keyVal, lookupVal) {
          if rowIdx == 0 { return keyVal }
          return evaluateSingleCellInRange(
            onSheet: sheetName,
            row: targetRow,
            column: c,
            context: &frame.context,
            depth: frame.depth + 1
          )
        }
        if let targetNum, let n = keyVal.numericValue, n <= targetNum, n >= approxBestNum {
          approxBestNum = n
          approxBestCol = c
        }
      }
      if let matchedCol = approxBestCol {
        return evaluateSingleCellInRange(
          onSheet: sheetName,
          row: targetRow,
          column: matchedCol,
          context: &frame.context,
          depth: frame.depth + 1
        )
      }
      return .error("#N/A")

    case "XLOOKUP":
      guard (3...6).contains(frame.count) else { return .error("#VALUE!") }
      switch (frame.resolve(1..<2, in: self), frame.resolve(2..<3, in: self)) {
      case (let .failure(err), _), (_, let .failure(err)):
        return .error(err)
      case (.success(let lookupArr), .success(let returnArr)):
        guard lookupArr.count == returnArr.count, !lookupArr.isEmpty else {
          return .error("#VALUE!")
        }
        let lookupVal = frame.arg(0, in: self)
        let matchMode: Int
        if frame.count >= 5, frame.args[4] != .empty {
          guard let mmNum = frame.arg(4, in: self).numericValue,
            let mmInt = Self.safeInt(mmNum),
            [-1, 0, 1, 2].contains(mmInt)
          else {
            return .error("#VALUE!")
          }
          matchMode = mmInt
        } else {
          matchMode = 0
        }

        let searchMode: Int
        if frame.count == 6, frame.args[5] != .empty {
          guard let smNum = frame.arg(5, in: self).numericValue,
            let smInt = Self.safeInt(smNum),
            [-2, -1, 1, 2].contains(smInt)
          else {
            return .error("#VALUE!")
          }
          searchMode = smInt
        } else {
          searchMode = 1
        }

        let searchIndices: [Int] =
          searchMode < 0 ? Array(lookupArr.indices.reversed()) : Array(lookupArr.indices)

        if matchMode == 2 {
          let pattern = lookupVal.description
          for idx in searchIndices
          where Self.matchesWildcardPattern(text: lookupArr[idx].description, pattern: pattern)
            || Self.cellValuesEqual(lookupArr[idx], lookupVal)
          {
            return returnArr[idx]
          }
        } else {
          for idx in searchIndices where Self.cellValuesEqual(lookupArr[idx], lookupVal) {
            return returnArr[idx]
          }
          if matchMode == -1 {
            if let targetNum = lookupVal.numericValue {
              var bestIdx: Int?
              var bestNum = -Double.infinity
              for idx in searchIndices {
                if let n = lookupArr[idx].numericValue, n < targetNum, n > bestNum {
                  bestNum = n
                  bestIdx = idx
                }
              }
              if let bestIdx {
                return returnArr[bestIdx]
              }
            } else {
              var bestIdx: Int?
              var bestStr: String?
              for idx in searchIndices where lookupArr[idx] != .empty {
                let s = lookupArr[idx].description
                if s.caseInsensitiveCompare(lookupVal.description) == .orderedAscending {
                  if let currentBest = bestStr {
                    if s.caseInsensitiveCompare(currentBest) == .orderedDescending {
                      bestStr = s
                      bestIdx = idx
                    }
                  } else {
                    bestStr = s
                    bestIdx = idx
                  }
                }
              }
              if let bestIdx {
                return returnArr[bestIdx]
              }
            }
          } else if matchMode == 1 {
            if let targetNum = lookupVal.numericValue {
              var bestIdx: Int?
              var bestNum = Double.infinity
              for idx in searchIndices {
                if let n = lookupArr[idx].numericValue, n > targetNum, n < bestNum {
                  bestNum = n
                  bestIdx = idx
                }
              }
              if let bestIdx {
                return returnArr[bestIdx]
              }
            } else {
              var bestIdx: Int?
              var bestStr: String?
              for idx in searchIndices where lookupArr[idx] != .empty {
                let s = lookupArr[idx].description
                if s.caseInsensitiveCompare(lookupVal.description) == .orderedDescending {
                  if let currentBest = bestStr {
                    if s.caseInsensitiveCompare(currentBest) == .orderedAscending {
                      bestStr = s
                      bestIdx = idx
                    }
                  } else {
                    bestStr = s
                    bestIdx = idx
                  }
                }
              }
              if let bestIdx {
                return returnArr[bestIdx]
              }
            }
          }
        }

        if frame.count >= 4, frame.args[3] != .empty {
          return frame.arg(3, in: self)
        }
        return .error("#N/A")
      }

    case "INDEX":
      guard (2...3).contains(frame.count),
        let range = extractCellRange(from: frame.args[0]),
        let rowNumVal = frame.arg(1, in: self).numericValue,
        let rawRowIdx = Self.safeInt(rowNumVal)
      else { return .error("#VALUE!") }
      let sheetName = frame.args[0].targetSheetName
      if let sheetName, !hasSheet(named: sheetName) {
        return .error("#REF!")
      }
      let colNumVal = frame.count == 3 ? (frame.arg(2, in: self).numericValue ?? 1.0) : 1.0
      guard let rawColIdx = Self.safeInt(colNumVal),
        let bounds = rangeBounds(range)
      else { return .error("#VALUE!") }
      let rIdx = rawRowIdx - 1
      let cIdx = rawColIdx - 1
      if frame.count == 2,
        bounds.rowCount == 1,
        bounds.colCount > 1,
        rIdx >= 0,
        rIdx < bounds.colCount
      {
        return evaluateSingleCellInRange(
          onSheet: sheetName,
          row: bounds.minRow,
          column: bounds.minCol + rIdx,
          context: &frame.context,
          depth: frame.depth + 1
        )
      }
      if frame.count == 3, rawRowIdx == 0, cIdx >= 0, cIdx < bounds.colCount {
        return evaluateSingleCellInRange(
          onSheet: sheetName,
          row: bounds.minRow,
          column: bounds.minCol + cIdx,
          context: &frame.context,
          depth: frame.depth + 1
        )
      }
      if frame.count == 3, rawColIdx == 0, rIdx >= 0, rIdx < bounds.rowCount {
        return evaluateSingleCellInRange(
          onSheet: sheetName,
          row: bounds.minRow + rIdx,
          column: bounds.minCol,
          context: &frame.context,
          depth: frame.depth + 1
        )
      }
      guard rIdx >= 0, rIdx < bounds.rowCount, cIdx >= 0, cIdx < bounds.colCount else {
        return .error("#REF!")
      }
      return evaluateSingleCellInRange(
        onSheet: sheetName,
        row: bounds.minRow + rIdx,
        column: bounds.minCol + cIdx,
        context: &frame.context,
        depth: frame.depth + 1
      )

    case "MATCH":
      guard (2...3).contains(frame.count) else { return .error("#VALUE!") }
      let matchType: Int
      if frame.count == 3 {
        guard let mtNum = frame.arg(2, in: self).numericValue, let mtInt = Self.safeInt(mtNum)
        else {
          return .error("#VALUE!")
        }
        matchType = mtInt
      } else {
        matchType = 0
      }
      switch frame.resolve(1..<2, in: self) {
      case .failure(let err): return .error(err)
      case .success(let lookupArr):
        let lookupVal = frame.arg(0, in: self)
        if matchType == 0 {
          for (idx, item) in lookupArr.enumerated() where Self.cellValuesEqual(item, lookupVal) {
            return .number(Double(idx + 1))
          }
          return .error("#N/A")
        } else if matchType > 0 {
          if let targetNum = lookupVal.numericValue {
            var bestIdx: Int?
            var bestNum = -Double.infinity
            for (idx, item) in lookupArr.enumerated() {
              if let n = item.numericValue, n <= targetNum, n >= bestNum {
                bestNum = n
                bestIdx = idx
              }
            }
            if let bestIdx { return .number(Double(bestIdx + 1)) }
          } else {
            for (idx, item) in lookupArr.enumerated() where Self.cellValuesEqual(item, lookupVal) {
              return .number(Double(idx + 1))
            }
          }
          return .error("#N/A")
        } else {
          if let targetNum = lookupVal.numericValue {
            var bestIdx: Int?
            var bestNum = Double.infinity
            for (idx, item) in lookupArr.enumerated() {
              if let n = item.numericValue, n >= targetNum, n <= bestNum {
                bestNum = n
                bestIdx = idx
              }
            }
            if let bestIdx { return .number(Double(bestIdx + 1)) }
          }
          return .error("#N/A")
        }
      }

    case "CHOOSE":
      guard frame.count >= 2 else { return .error("#VALUE!") }
      let firstArg = frame.arg(0, in: self)
      if case .error = firstArg { return firstArg }
      guard let idxVal = firstArg.numericValue, let choiceIdx = Self.safeInt(idxVal),
        choiceIdx >= 1, choiceIdx < frame.count
      else { return .error("#VALUE!") }
      return frame.arg(choiceIdx, in: self)

    case "ROW":
      guard frame.count == 1 else { return .error("#VALUE!") }
      if let sheetName = frame.args[0].targetSheetName, !hasSheet(named: sheetName) {
        return .error("#REF!")
      }
      if case .namedRangeRef(let rangeName) = frame.args[0], resolveNamedRange(rangeName) == nil {
        return .error("#NAME?")
      }
      if let range = extractCellRange(from: frame.args[0]) {
        return .number(Double(range.start.row + 1))
      }
      return .error("#REF!")

    case "COLUMN":
      guard frame.count == 1 else { return .error("#VALUE!") }
      if let sheetName = frame.args[0].targetSheetName, !hasSheet(named: sheetName) {
        return .error("#REF!")
      }
      if case .namedRangeRef(let rangeName) = frame.args[0], resolveNamedRange(rangeName) == nil {
        return .error("#NAME?")
      }
      if let range = extractCellRange(from: frame.args[0]) {
        return .number(Double(range.start.column + 1))
      }
      return .error("#REF!")

    case "ROWS":
      guard frame.count == 1 else { return .error("#VALUE!") }
      if let sheetName = frame.args[0].targetSheetName, !hasSheet(named: sheetName) {
        return .error("#REF!")
      }
      if case .namedRangeRef(let rangeName) = frame.args[0], resolveNamedRange(rangeName) == nil {
        return .error("#NAME?")
      }
      if let range = extractCellRange(from: frame.args[0]) {
        return .number(Double(abs(range.end.row - range.start.row) + 1))
      }
      let evaluated = frame.arg(0, in: self)
      if case .error = evaluated { return evaluated }
      return .number(1.0)

    case "COLUMNS":
      guard frame.count == 1 else { return .error("#VALUE!") }
      if let sheetName = frame.args[0].targetSheetName, !hasSheet(named: sheetName) {
        return .error("#REF!")
      }
      if case .namedRangeRef(let rangeName) = frame.args[0], resolveNamedRange(rangeName) == nil {
        return .error("#NAME?")
      }
      if let range = extractCellRange(from: frame.args[0]) {
        return .number(Double(abs(range.end.column - range.start.column) + 1))
      }
      let evaluated = frame.arg(0, in: self)
      if case .error = evaluated { return evaluated }
      return .number(1.0)

    default:
      return nil
    }
  }

  // MARK: - 5. Text Functions

  internal func evaluateTextFunction(name: String, frame: inout EvalFrame) -> EvaluatedCellValue? {
    switch name {
    case "CONCAT", "CONCATENATE":
      switch frame.resolve(frame.indices, in: self) {
      case .failure(let err): return .error(err)
      case .success(let vals): return .text(vals.map { $0.description }.joined())
      }
    case "TEXTJOIN":
      guard frame.count >= 3 else { return .error("#VALUE!") }
      let delimiter = frame.arg(0, in: self).description
      let ignoreEmpty = frame.arg(1, in: self).booleanValue ?? true
      switch frame.resolve(2..<frame.count, in: self) {
      case .failure(let err): return .error(err)
      case .success(let vals):
        let parts = vals.map(\.description).filter { !ignoreEmpty || !$0.isEmpty }
        return .text(parts.joined(separator: delimiter))
      }
    case "LEFT":
      guard (1...2).contains(frame.count) else { return .error("#VALUE!") }
      let text = frame.arg(0, in: self).description
      let count: Int
      if frame.count == 2 {
        guard let cNum = frame.arg(1, in: self).numericValue, let cInt = Self.safeInt(cNum),
          cInt >= 0
        else {
          return .error("#VALUE!")
        }
        count = cInt
      } else {
        count = 1
      }
      return .text(String(text.prefix(count)))
    case "RIGHT":
      guard (1...2).contains(frame.count) else { return .error("#VALUE!") }
      let text = frame.arg(0, in: self).description
      let count: Int
      if frame.count == 2 {
        guard let cNum = frame.arg(1, in: self).numericValue, let cInt = Self.safeInt(cNum),
          cInt >= 0
        else {
          return .error("#VALUE!")
        }
        count = cInt
      } else {
        count = 1
      }
      return .text(String(text.suffix(count)))
    case "MID":
      guard frame.count == 3,
        let startVal = frame.arg(1, in: self).numericValue,
        let lenVal = frame.arg(2, in: self).numericValue,
        let startOneBased = Self.safeInt(startVal),
        let length = Self.safeInt(lenVal),
        startOneBased >= 1, length >= 0
      else { return .error("#VALUE!") }
      let text = frame.arg(0, in: self).description
      let startOffset = startOneBased - 1
      guard
        let startIdx = text.index(text.startIndex, offsetBy: startOffset, limitedBy: text.endIndex),
        startIdx < text.endIndex
      else {
        return .text("")
      }
      let endIdx = text.index(startIdx, offsetBy: length, limitedBy: text.endIndex) ?? text.endIndex
      return .text(String(text[startIdx..<endIdx]))
    case "LEN":
      guard frame.count == 1 else { return .error("#VALUE!") }
      return .number(Double(frame.arg(0, in: self).description.count))
    case "LOWER":
      guard frame.count == 1 else { return .error("#VALUE!") }
      return .text(frame.arg(0, in: self).description.lowercased())
    case "UPPER":
      guard frame.count == 1 else { return .error("#VALUE!") }
      return .text(frame.arg(0, in: self).description.uppercased())
    case "PROPER":
      guard frame.count == 1 else { return .error("#VALUE!") }
      return .text(frame.arg(0, in: self).description.capitalized)
    case "TRIM":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let parts = frame.arg(0, in: self).description
        .components(separatedBy: .whitespacesAndNewlines)
        .filter { !$0.isEmpty }
      return .text(parts.joined(separator: " "))
    case "SUBSTITUTE":
      guard (3...4).contains(frame.count) else { return .error("#VALUE!") }
      let source = frame.arg(0, in: self).description
      let target = frame.arg(1, in: self).description
      let replacement = frame.arg(2, in: self).description
      guard !target.isEmpty else { return .text(source) }
      if frame.count == 4, let instNum = frame.arg(3, in: self).numericValue {
        guard let targetOccurrence = Self.safeInt(instNum), targetOccurrence >= 1 else {
          return .error("#VALUE!")
        }
        var currentOccurrence = 0
        var result = ""
        var searchStart = source.startIndex
        while let range = source.range(of: target, range: searchStart..<source.endIndex) {
          currentOccurrence += 1
          result += source[searchStart..<range.lowerBound]
          if currentOccurrence == targetOccurrence {
            result += replacement
            result += source[range.upperBound..<source.endIndex]
            return .text(result)
          } else {
            result += target
          }
          searchStart = range.upperBound
        }
        return .text(source)
      }
      return .text(source.replacingOccurrences(of: target, with: replacement))
    case "REPLACE":
      guard frame.count == 4,
        let startVal = frame.arg(1, in: self).numericValue,
        let countVal = frame.arg(2, in: self).numericValue,
        let startOneBased = Self.safeInt(startVal),
        let numChars = Self.safeInt(countVal),
        startOneBased >= 1, numChars >= 0
      else { return .error("#VALUE!") }
      let oldText = frame.arg(0, in: self).description
      let newText = frame.arg(3, in: self).description
      let startOffset = min(startOneBased - 1, oldText.count)
      let startIdx = oldText.index(oldText.startIndex, offsetBy: startOffset)
      let endIdx =
        oldText.index(startIdx, offsetBy: numChars, limitedBy: oldText.endIndex) ?? oldText.endIndex
      var result = oldText
      result.replaceSubrange(startIdx..<endIdx, with: newText)
      return .text(result)
    case "FIND", "SEARCH":
      guard (2...3).contains(frame.count) else { return .error("#VALUE!") }
      let needle = frame.arg(0, in: self).description
      let haystack = frame.arg(1, in: self).description
      let startOffset: Int
      if frame.count == 3 {
        guard let sNum = frame.arg(2, in: self).numericValue, let sInt = Self.safeInt(sNum),
          sInt >= 1
        else {
          return .error("#VALUE!")
        }
        startOffset = sInt - 1
      } else {
        startOffset = 0
      }
      if needle.isEmpty {
        return startOffset <= haystack.count ? .number(Double(startOffset + 1)) : .error("#VALUE!")
      }
      guard startOffset < haystack.count else { return .error("#VALUE!") }
      let startIdx = haystack.index(haystack.startIndex, offsetBy: startOffset)
      let compareOptions: String.CompareOptions = (name == "SEARCH") ? [.caseInsensitive] : []
      guard
        let foundRange = haystack.range(
          of: needle, options: compareOptions, range: startIdx..<haystack.endIndex)
      else {
        return .error("#VALUE!")
      }
      return .number(
        Double(haystack.distance(from: haystack.startIndex, to: foundRange.lowerBound) + 1))
    case "REPT":
      guard frame.count == 2,
        let timesVal = frame.arg(1, in: self).numericValue,
        let count = Self.safeInt(timesVal), count >= 0
      else { return .error("#VALUE!") }
      let str = frame.arg(0, in: self).description
      let (totalChars, overflow) = str.count.multipliedReportingOverflow(by: count)
      guard !overflow, totalChars <= Self.maxReptOutputCharacters else { return .error("#VALUE!") }
      return .text(String(repeating: str, count: count))
    case "EXACT":
      guard frame.count == 2 else { return .error("#VALUE!") }
      return .boolean(frame.arg(0, in: self).description == frame.arg(1, in: self).description)
    case "VALUE":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let cleaned = frame.arg(0, in: self).description
        .trimmingCharacters(in: .whitespacesAndNewlines)
        .replacingOccurrences(of: ",", with: "")
        .replacingOccurrences(of: "₩", with: "")
        .replacingOccurrences(of: "$", with: "")
      guard let num = Double(cleaned), num.isFinite else { return .error("#VALUE!") }
      return .number(num)
    case "TEXT":
      guard frame.count == 2 else { return .error("#VALUE!") }
      let val = frame.arg(0, in: self)
      let pattern = frame.arg(1, in: self).description
      guard let num = val.numericValue, num.isFinite else { return .text(val.description) }
      if pattern.contains("%") {
        return .text(String(format: "%.0f%%", num * 100.0))
      }
      let decimals: Int
      if let dotIndex = pattern.firstIndex(of: ".") {
        let afterDot = pattern[pattern.index(after: dotIndex)...]
        let digitPlaceholders = afterDot.prefix(while: { $0 == "0" || $0 == "#" }).count
        decimals = min(
          10,
          max(
            0,
            digitPlaceholders > 0
              ? digitPlaceholders
              : pattern.distance(from: pattern.index(after: dotIndex), to: pattern.endIndex)))
      } else {
        decimals = 0
      }
      if pattern.contains(",") {
        return .text(CellNumberFormat.formatGroupedNumber(num, fractionDigits: decimals))
      }
      return .text(String(format: "%.\(decimals)f", num))
    case "CLEAN":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let raw = frame.arg(0, in: self).description
      let cleaned = String(raw.unicodeScalars.filter { $0.value >= 32 })
      return .text(cleaned)
    case "CHAR":
      guard frame.count == 1,
        let num = frame.arg(0, in: self).numericValue,
        let code = Self.safeInt(trunc(num)),
        (1...1_114_111).contains(code),
        let scalar = UnicodeScalar(code)
      else {
        return .error("#VALUE!")
      }
      return .text(String(scalar))
    case "CODE":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let str = frame.arg(0, in: self).description
      guard let scalar = str.unicodeScalars.first else {
        return .error("#VALUE!")
      }
      return .number(Double(scalar.value))
    default:
      return nil
    }
  }

  // MARK: - 6. Financial & Date Functions

  internal func evaluateFinancialAndDateFunction(name: String, frame: inout EvalFrame)
    -> EvaluatedCellValue?
  {
    switch name {
    case "PMT":
      guard (3...5).contains(frame.count),
        let rate = frame.arg(0, in: self).numericValue,
        let nper = frame.arg(1, in: self).numericValue,
        let pv = frame.arg(2, in: self).numericValue,
        nper != 0.0
      else { return .error("#VALUE!") }
      let fv = frame.count >= 4 ? (frame.arg(3, in: self).numericValue ?? 0.0) : 0.0
      let type = frame.count == 5 ? (frame.arg(4, in: self).numericValue ?? 0.0) : 0.0
      if rate == 0.0 {
        let res = -(pv + fv) / nper
        return res.isFinite ? .number(res) : .error("#NUM!")
      }
      let factor = pow(1.0 + rate, nper)
      let denom = (1.0 + rate * type) * (factor - 1.0)
      guard factor.isFinite, denom != 0.0 else { return .error("#NUM!") }
      let payment = -rate * (pv * factor + fv) / denom
      return payment.isFinite ? .number(payment) : .error("#NUM!")

    case "FV":
      guard (3...5).contains(frame.count),
        let rate = frame.arg(0, in: self).numericValue,
        let nper = frame.arg(1, in: self).numericValue,
        let pmt = frame.arg(2, in: self).numericValue
      else { return .error("#VALUE!") }
      let pv = frame.count >= 4 ? (frame.arg(3, in: self).numericValue ?? 0.0) : 0.0
      let type = frame.count == 5 ? (frame.arg(4, in: self).numericValue ?? 0.0) : 0.0
      if rate == 0.0 {
        let res = -(pv + pmt * nper)
        return res.isFinite ? .number(res) : .error("#NUM!")
      }
      let factor = pow(1.0 + rate, nper)
      let futureVal = -(pv * factor + pmt * (1.0 + rate * type) * (factor - 1.0) / rate)
      return futureVal.isFinite ? .number(futureVal) : .error("#NUM!")

    case "PV":
      guard (3...5).contains(frame.count),
        let rate = frame.arg(0, in: self).numericValue,
        let nper = frame.arg(1, in: self).numericValue,
        let pmt = frame.arg(2, in: self).numericValue
      else { return .error("#VALUE!") }
      let fv = frame.count >= 4 ? (frame.arg(3, in: self).numericValue ?? 0.0) : 0.0
      let type = frame.count == 5 ? (frame.arg(4, in: self).numericValue ?? 0.0) : 0.0
      if rate == 0.0 {
        let res = -(fv + pmt * nper)
        return res.isFinite ? .number(res) : .error("#NUM!")
      }
      let factor = pow(1.0 + rate, nper)
      guard factor.isFinite, factor != 0.0 else { return .error("#NUM!") }
      let presentVal = -(fv + pmt * (1.0 + rate * type) * (factor - 1.0) / rate) / factor
      return presentVal.isFinite ? .number(presentVal) : .error("#NUM!")

    case "NPV":
      guard frame.count >= 2, let rate = frame.arg(0, in: self).numericValue, rate != -1.0 else {
        return .error("#VALUE!")
      }
      switch frame.resolve(1..<frame.count, in: self) {
      case .failure(let err): return .error(err)
      case .success(let vals):
        let cashFlows = vals.compactMap { $0.numericValue }
        var npv = 0.0
        for (idx, cf) in cashFlows.enumerated() {
          npv += cf / pow(1.0 + rate, Double(idx + 1))
        }
        return npv.isFinite ? .number(npv) : .error("#NUM!")
      }

    case "IRR":
      guard (1...2).contains(frame.count) else { return .error("#VALUE!") }
      switch frame.resolve(0..<1, in: self) {
      case .failure(let err):
        return .error(err)
      case .success(let vals):
        let rawCashFlows = vals.compactMap(\.numericValue)
        guard rawCashFlows.contains(where: { $0 > 0.0 }),
          rawCashFlows.contains(where: { $0 < 0.0 })
        else {
          return .error("#NUM!")
        }
        let maxMag = max(1.0, rawCashFlows.map { abs($0) }.max() ?? 1.0)
        let cashFlows = rawCashFlows.map { $0 / maxMag }
        let guess = frame.count == 2 ? (frame.arg(1, in: self).numericValue ?? 0.1) : 0.1
        var rate = guess <= -1.0 ? 0.1 : guess
        var lastResidual = Double.infinity
        for _ in 0..<100 {
          var npv = 0.0
          var dNpv = 0.0
          let base = 1.0 + rate
          guard base > 1e-9 else {
            rate = 0.05
            continue
          }
          for (t, cf) in cashFlows.enumerated() {
            let tDouble = Double(t)
            npv += cf / pow(base, tDouble)
            if t > 0 {
              dNpv -= tDouble * cf / pow(base, tDouble + 1.0)
            }
          }
          lastResidual = abs(npv)
          if lastResidual < 1e-9 {
            return .number(rate)
          }
          guard abs(dNpv) > 1e-12, dNpv.isFinite else {
            break
          }
          let nextRate = rate - npv / dNpv
          if abs(nextRate - rate) < 1e-9, lastResidual < 1e-5 {
            return .number(nextRate)
          }
          rate = max(-0.9999, nextRate)
        }
        guard rate.isFinite, lastResidual < 1e-5 else {
          return .error("#NUM!")
        }
        return .number(rate)
      }

    case "NPER":
      guard (3...5).contains(frame.count),
        let rate = frame.arg(0, in: self).numericValue,
        let pmt = frame.arg(1, in: self).numericValue,
        let pv = frame.arg(2, in: self).numericValue
      else {
        return .error("#VALUE!")
      }
      let fv = frame.count >= 4 ? (frame.arg(3, in: self).numericValue ?? 0.0) : 0.0
      let type = frame.count == 5 ? (frame.arg(4, in: self).numericValue ?? 0.0) : 0.0
      if rate == 0.0 {
        guard pmt != 0.0 else { return .error("#DIV/0!") }
        let nper = -(pv + fv) / pmt
        return nper.isFinite ? .number(nper) : .error("#NUM!")
      }
      let pmtAdj = pmt * (1.0 + rate * type)
      let num = pmtAdj - fv * rate
      let den = pv * rate + pmtAdj
      guard den != 0.0, (num / den) > 0.0, (1.0 + rate) > 0.0 else {
        return .error("#NUM!")
      }
      let result = log(num / den) / log(1.0 + rate)
      return result.isFinite ? .number(result) : .error("#NUM!")

    case "RATE":
      guard (3...6).contains(frame.count),
        let nper = frame.arg(0, in: self).numericValue,
        let pmt = frame.arg(1, in: self).numericValue,
        let pv = frame.arg(2, in: self).numericValue,
        nper > 0.0
      else {
        return .error("#VALUE!")
      }
      let fv = frame.count >= 4 ? (frame.arg(3, in: self).numericValue ?? 0.0) : 0.0
      let type = frame.count >= 5 ? (frame.arg(4, in: self).numericValue ?? 0.0) : 0.0
      let guess = frame.count == 6 ? (frame.arg(5, in: self).numericValue ?? 0.1) : 0.1

      func evalTVM(_ r: Double) -> Double {
        if abs(r) < 1e-12 {
          return pv + pmt * nper + fv
        }
        let factor = pow(1.0 + r, nper)
        return pv * factor + pmt * (1.0 + r * type) * (factor - 1.0) / r + fv
      }

      var r = guess <= -1.0 ? 0.1 : guess
      let eps = 1e-7
      for _ in 0..<100 {
        let y = evalTVM(r)
        if abs(y) < 1e-9 {
          return .number(r)
        }
        let dy = (evalTVM(r + eps) - evalTVM(r - eps)) / (2.0 * eps)
        guard abs(dy) > 1e-12, dy.isFinite else {
          break
        }
        let nextR = r - y / dy
        if abs(nextR - r) < 1e-9 {
          return .number(nextR)
        }
        r = max(-0.9999, nextR)
      }
      if abs(evalTVM(r)) < 1e-5 && r.isFinite {
        return .number(r)
      }
      return .error("#NUM!")

    case "EFFECT":
      guard frame.count == 2,
        let nominal = frame.arg(0, in: self).numericValue,
        let nperyVal = frame.arg(1, in: self).numericValue
      else {
        return .error("#VALUE!")
      }
      let npery = floor(nperyVal)
      guard nominal > 0.0, npery >= 1.0 else {
        return .error("#NUM!")
      }
      let result = pow(1.0 + nominal / npery, npery) - 1.0
      return result.isFinite ? .number(result) : .error("#NUM!")

    case "NOMINAL":
      guard frame.count == 2,
        let effect = frame.arg(0, in: self).numericValue,
        let nperyVal = frame.arg(1, in: self).numericValue
      else {
        return .error("#VALUE!")
      }
      let npery = floor(nperyVal)
      guard effect > 0.0, npery >= 1.0 else {
        return .error("#NUM!")
      }
      let result = npery * (pow(1.0 + effect, 1.0 / npery) - 1.0)
      return result.isFinite ? .number(result) : .error("#NUM!")

    case "SLN":
      guard frame.count == 3,
        let cost = frame.arg(0, in: self).numericValue,
        let salvage = frame.arg(1, in: self).numericValue,
        let life = frame.arg(2, in: self).numericValue
      else {
        return .error("#VALUE!")
      }
      guard life != 0.0 else {
        return .error("#DIV/0!")
      }
      return .number((cost - salvage) / life)

    case "SYD":
      guard frame.count == 4,
        let cost = frame.arg(0, in: self).numericValue,
        let salvage = frame.arg(1, in: self).numericValue,
        let life = frame.arg(2, in: self).numericValue,
        let per = frame.arg(3, in: self).numericValue
      else {
        return .error("#VALUE!")
      }
      guard life > 0.0, per > 0.0, per <= life else {
        return .error("#NUM!")
      }
      let result = ((cost - salvage) * (life - per + 1.0) * 2.0) / (life * (life + 1.0))
      return result.isFinite ? .number(result) : .error("#NUM!")

    case "DATE":
      guard frame.count == 3,
        let yNum = frame.arg(0, in: self).numericValue,
        let mNum = frame.arg(1, in: self).numericValue,
        let dNum = frame.arg(2, in: self).numericValue,
        let y = Self.safeInt(yNum), let m = Self.safeInt(mNum), let d = Self.safeInt(dNum),
        let date = Self.makeUTCDate(year: y, month: m, day: d, allowRollover: true)
      else { return .error("#VALUE!") }
      return .text(Self.formatUTCDate(date))

    case "YEAR":
      guard frame.count == 1, let date = Self.parseDateValue(frame.arg(0, in: self)) else {
        return .error("#VALUE!")
      }
      return .number(Double(Self.utcCalendar.component(.year, from: date)))

    case "MONTH":
      guard frame.count == 1, let date = Self.parseDateValue(frame.arg(0, in: self)) else {
        return .error("#VALUE!")
      }
      return .number(Double(Self.utcCalendar.component(.month, from: date)))

    case "DAY":
      guard frame.count == 1, let date = Self.parseDateValue(frame.arg(0, in: self)) else {
        return .error("#VALUE!")
      }
      return .number(Double(Self.utcCalendar.component(.day, from: date)))

    case "DAYS":
      guard frame.count == 2,
        let endDate = Self.parseDateValue(frame.arg(0, in: self)),
        let startDate = Self.parseDateValue(frame.arg(1, in: self))
      else { return .error("#VALUE!") }
      return .number(
        Double(Self.utcCalendar.dateComponents([.day], from: startDate, to: endDate).day ?? 0))

    case "WEEKDAY":
      guard (1...2).contains(frame.count),
        let date = Self.parseDateValue(frame.arg(0, in: self))
      else {
        return .error("#VALUE!")
      }
      let returnType: Int
      if frame.count == 2 {
        guard let rtNum = frame.arg(1, in: self).numericValue,
          let rtInt = Self.safeInt(rtNum)
        else {
          return .error("#VALUE!")
        }
        returnType = rtInt
      } else {
        returnType = 1
      }
      let calWeekday = Self.utcCalendar.component(.weekday, from: date)
      switch returnType {
      case 1:
        return .number(Double(calWeekday))
      case 2:
        return .number(Double(calWeekday == 1 ? 7 : calWeekday - 1))
      case 3:
        return .number(Double(calWeekday == 1 ? 6 : calWeekday - 2))
      default:
        return .error("#NUM!")
      }

    case "WEEKNUM":
      guard (1...2).contains(frame.count),
        let date = Self.parseDateValue(frame.arg(0, in: self))
      else {
        return .error("#VALUE!")
      }
      let returnType: Int
      if frame.count == 2 {
        guard let rtNum = frame.arg(1, in: self).numericValue,
          let rtInt = Self.safeInt(rtNum)
        else {
          return .error("#VALUE!")
        }
        returnType = rtInt
      } else {
        returnType = 1
      }
      var cal = Self.utcCalendar
      cal.firstWeekday = (returnType == 2 || returnType == 21) ? 2 : 1
      cal.minimumDaysInFirstWeek = 1
      return .number(Double(cal.component(.weekOfYear, from: date)))

    case "EDATE":
      guard frame.count == 2,
        let startDate = Self.parseDateValue(frame.arg(0, in: self)),
        let mNum = frame.arg(1, in: self).numericValue,
        let months = Self.safeInt(trunc(mNum)),
        let targetDate = Self.utcCalendar.date(byAdding: .month, value: months, to: startDate)
      else {
        return .error("#VALUE!")
      }
      return .text(Self.formatUTCDate(targetDate))

    case "EOMONTH":
      guard frame.count == 2,
        let startDate = Self.parseDateValue(frame.arg(0, in: self)),
        let mNum = frame.arg(1, in: self).numericValue,
        let months = Self.safeInt(trunc(mNum))
      else {
        return .error("#VALUE!")
      }
      let startComps = Self.utcCalendar.dateComponents([.year, .month], from: startDate)
      guard
        let firstOfStartMonth = Self.makeUTCDate(
          year: startComps.year ?? 1970, month: startComps.month ?? 1, day: 1),
        let firstOfNextAfterTarget = Self.utcCalendar.date(
          byAdding: .month, value: months + 1, to: firstOfStartMonth),
        let lastDayOfTargetMonth = Self.utcCalendar.date(
          byAdding: .day, value: -1, to: firstOfNextAfterTarget)
      else {
        return .error("#VALUE!")
      }
      return .text(Self.formatUTCDate(lastDayOfTargetMonth))

    case "WORKDAY":
      guard frame.count == 2,
        let startDate = Self.parseDateValue(frame.arg(0, in: self)),
        let dNum = frame.arg(1, in: self).numericValue,
        let days = Self.safeInt(trunc(dNum))
      else {
        return .error("#VALUE!")
      }
      guard abs(days) <= 100_000 else {
        return .error("#NUM!")
      }
      var current = startDate
      var remaining = abs(days)
      let step = days >= 0 ? 1 : -1
      while remaining > 0 {
        guard let next = Self.utcCalendar.date(byAdding: .day, value: step, to: current) else {
          return .error("#VALUE!")
        }
        current = next
        let wd = Self.utcCalendar.component(.weekday, from: current)
        if wd != 1 && wd != 7 {
          remaining -= 1
        }
      }
      return .text(Self.formatUTCDate(current))

    case "NETWORKDAYS":
      guard frame.count == 2,
        let startDate = Self.parseDateValue(frame.arg(0, in: self)),
        let endDate = Self.parseDateValue(frame.arg(1, in: self))
      else {
        return .error("#VALUE!")
      }
      let isReverse = startDate > endDate
      let fromDate = isReverse ? endDate : startDate
      let toDate = isReverse ? startDate : endDate
      let daySpan = abs(
        Self.utcCalendar.dateComponents([.day], from: fromDate, to: toDate).day ?? 0)
      guard daySpan <= 100_000 else {
        return .error("#NUM!")
      }
      var current = fromDate
      var count = 0
      while current <= toDate {
        let wd = Self.utcCalendar.component(.weekday, from: current)
        if wd != 1 && wd != 7 {
          count += 1
        }
        guard let next = Self.utcCalendar.date(byAdding: .day, value: 1, to: current) else {
          break
        }
        current = next
      }
      return .number(Double(isReverse ? -count : count))

    case "DATEDIF":
      guard frame.count == 3,
        let startDate = Self.parseDateValue(frame.arg(0, in: self)),
        let endDate = Self.parseDateValue(frame.arg(1, in: self))
      else {
        return .error("#VALUE!")
      }
      guard startDate <= endDate else {
        return .error("#NUM!")
      }
      let unit = frame.arg(2, in: self).description.trimmingCharacters(in: .whitespacesAndNewlines)
        .uppercased()
      switch unit {
      case "D":
        let days = Self.utcCalendar.dateComponents([.day], from: startDate, to: endDate).day ?? 0
        return .number(Double(days))
      case "M":
        let months =
          Self.utcCalendar.dateComponents([.month], from: startDate, to: endDate).month ?? 0
        return .number(Double(months))
      case "Y":
        let years = Self.utcCalendar.dateComponents([.year], from: startDate, to: endDate).year ?? 0
        return .number(Double(years))
      case "YM":
        let months =
          Self.utcCalendar.dateComponents([.year, .month], from: startDate, to: endDate).month ?? 0
        return .number(Double(months))
      case "MD":
        let days =
          Self.utcCalendar.dateComponents([.year, .month, .day], from: startDate, to: endDate).day
          ?? 0
        return .number(Double(days))
      default:
        return .error("#VALUE!")
      }

    default:
      return nil
    }
  }

  // MARK: - 7. Information Functions (ISBLANK, ISERROR, ISNUMBER, ISTEXT)

  internal func evaluateInfoFunction(name: String, frame: inout EvalFrame) -> EvaluatedCellValue? {
    switch name {
    case "ISBLANK":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      return .boolean(val == .empty)
    case "ISERROR":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      if case .error = val { return .boolean(true) }
      return .boolean(false)
    case "ISNUMBER":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      if case .number = val { return .boolean(true) }
      return .boolean(false)
    case "ISTEXT":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      if case .text = val { return .boolean(true) }
      return .boolean(false)
    default:
      return nil
    }
  }

  internal func rangeBounds(_ range: CellRange) -> (
    minRow: Int, maxRow: Int, minCol: Int, maxCol: Int, rowCount: Int, colCount: Int
  )? {
    let minRow = min(range.start.row, range.end.row)
    let maxRow = max(range.start.row, range.end.row)
    let minCol = min(range.start.column, range.end.column)
    let maxCol = max(range.start.column, range.end.column)
    let (rowSpan, rOverflow) = (maxRow - minRow).addingReportingOverflow(1)
    let (colSpan, cOverflow) = (maxCol - minCol).addingReportingOverflow(1)
    let (totalCells, mulOverflow) = rowSpan.multipliedReportingOverflow(by: colSpan)
    guard !rOverflow, !cOverflow, !mulOverflow,
      totalCells > 0, totalCells <= CellRange.maxExpandedCellCount
    else {
      return nil
    }
    return (minRow, maxRow, minCol, maxCol, rowSpan, colSpan)
  }

  internal func evaluateSingleCellInRange(
    onSheet sheetName: String?,
    row: Int,
    column: Int,
    context: inout EvaluationContext,
    depth: Int
  ) -> EvaluatedCellValue {
    let coord = PackedCellCoordinate(row: row, column: column)
    let val =
      sheetName.map {
        evaluateOnSheet(named: $0, at: coord, context: &context, depth: depth)
      } ?? evaluateInternal(at: coord, context: &context, depth: depth)
    if case .empty = val {
      return .number(0.0)
    }
    return val
  }

  internal static let utcCalendar: Calendar = {
    var cal = Calendar(identifier: .gregorian)
    cal.timeZone = TimeZone(secondsFromGMT: 0)!
    return cal
  }()

  internal static func makeUTCDate(
    year: Int, month: Int, day: Int, allowRollover: Bool = false
  ) -> Date? {
    let resolvedYear = (allowRollover && (0...1899).contains(year)) ? (1900 + year) : year
    guard (1...9999).contains(resolvedYear) else { return nil }
    if allowRollover {
      var baseComps = DateComponents()
      baseComps.year = resolvedYear
      baseComps.month = 1
      baseComps.day = 1
      baseComps.hour = 0
      baseComps.minute = 0
      baseComps.second = 0
      guard let jan1 = utcCalendar.date(from: baseComps),
        let monthShifted = utcCalendar.date(byAdding: .month, value: month - 1, to: jan1),
        let dayShifted = utcCalendar.date(byAdding: .day, value: day - 1, to: monthShifted)
      else {
        return nil
      }
      return dayShifted
    }
    guard (1...12).contains(month), (1...31).contains(day) else { return nil }
    var comps = DateComponents()
    comps.year = resolvedYear
    comps.month = month
    comps.day = day
    comps.hour = 0
    comps.minute = 0
    comps.second = 0
    guard let constructed = utcCalendar.date(from: comps),
      utcCalendar.component(.month, from: constructed) == month,
      utcCalendar.component(.day, from: constructed) == day
    else {
      return nil
    }
    return constructed
  }

  internal static func formatUTCDate(_ date: Date) -> String {
    let comps = utcCalendar.dateComponents([.year, .month, .day], from: date)
    return String(format: "%04d-%02d-%02d", comps.year ?? 1970, comps.month ?? 1, comps.day ?? 1)
  }

  internal static func parseDateValue(_ value: EvaluatedCellValue) -> Date? {
    if case .number(let n) = value, n.isFinite {
      let intVal = Int(floor(n))
      if intVal >= 19_000_101 && intVal <= 99_991_231 {
        let y = intVal / 10_000
        let m = (intVal / 100) % 100
        let d = intVal % 100
        if let ymd = makeUTCDate(year: y, month: m, day: d, allowRollover: false) {
          return ymd
        }
      }
      if intVal >= 1 && intVal <= 2_958_465 {
        var epochComps = DateComponents()
        epochComps.year = 1899
        epochComps.month = 12
        epochComps.day = intVal < 60 ? 31 : 30
        if let base = utcCalendar.date(from: epochComps) {
          return utcCalendar.date(byAdding: .day, value: intVal, to: base)
        }
      }
    }
    let raw = value.description.trimmingCharacters(in: .whitespacesAndNewlines)
    let separators = CharacterSet(charactersIn: "-/.")
    let parts = raw.components(separatedBy: separators)
    if parts.count == 3,
      let y = Int(parts[0]),
      let m = Int(parts[1]),
      let d = Int(parts[2])
    {
      return makeUTCDate(year: y, month: m, day: d, allowRollover: false)
    }
    return nil
  }
}
