import Foundation

extension WorksheetModel {


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

    case "INDIRECT":
      switch resolveIndirectRange(args: frame.args, frame: &frame) {
      case .failure(let err):
        return .error(err)
      case .success(let sheetName, let resolvedRange, let isSingleCell):
        guard isSingleCell else {
          return .error("#VALUE!")
        }
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

    case "VLOOKUP", "HLOOKUP":
      let isVertical = (name == "VLOOKUP")
      guard (3...4).contains(frame.count) else { return .error("#VALUE!") }
      let sheetName: String?
      let range: CellRange
      switch resolveRangeArgument(from: frame.args[1], frame: &frame) {
      case .failure(let err):
        return .error(err)
      case .success(let s, let r):
        sheetName = s
        range = r
      }
      guard let rawIdxVal = frame.arg(2, in: self).numericValue,
        let rawIdx = Self.safeInt(rawIdxVal)
      else { return .error("#VALUE!") }
      guard let bounds = rangeBounds(range, allowWholeColumnOrRow: true) else {
        return .error("#REF!")
      }
      let offsetIdx = rawIdx - 1
      let maxOffset = isVertical ? bounds.colCount : bounds.rowCount
      guard offsetIdx >= 0, offsetIdx < maxOffset else {
        return .error("#REF!")
      }
      let lookupVal = frame.arg(0, in: self)
      if case .error = lookupVal { return lookupVal }
      let isApprox: Bool
      if frame.count == 4 {
        if frame.args[3] == .missing {
          isApprox = false
        } else {
          let rlVal = frame.arg(3, in: self)
          if case .error = rlVal { return rlVal }
          isApprox = rlVal.booleanValue ?? false
        }
      } else {
        isApprox = true
      }
      let targetFixed = (isVertical ? bounds.minCol : bounds.minRow) + offsetIdx
      let scanSpan = isVertical ? bounds.rowCount : bounds.colCount
      let scanCoords: [Int]
      if scanSpan > 4096 {
        let pop = populatedCoordinates(
          onSheet: sheetName,
          minRow: bounds.minRow,
          maxRow: isVertical ? bounds.maxRow : bounds.minRow,
          minCol: bounds.minCol,
          maxCol: isVertical ? bounds.minCol : bounds.maxCol
        )
        scanCoords = pop.map { isVertical ? $0.row : $0.column }
      } else {
        scanCoords =
          isVertical
          ? Array(bounds.minRow...bounds.maxRow)
          : Array(bounds.minCol...bounds.maxCol)
      }
      var approxBestCoord: Int?
      var approxBestNum = -Double.infinity
      var approxBestText: String?
      let targetNum = isApprox ? lookupVal.numericValue : nil
      let targetText: String? =
        (isApprox && targetNum == nil && lookupVal != .empty) ? lookupVal.description : nil

      for coord in scanCoords {
        let keyVal = evaluateSingleCellInRange(
          onSheet: sheetName,
          row: isVertical ? coord : bounds.minRow,
          column: isVertical ? bounds.minCol : coord,
          context: &frame.context,
          depth: frame.depth + 1,
          coerceEmptyToZero: false
        )
        if case .error = keyVal { continue }
        if !isApprox {
          let isExactMatch: Bool
          if Self.cellValuesEqual(keyVal, lookupVal) {
            isExactMatch = true
          } else if case (.text(let keyText), .text(let patText)) = (keyVal, lookupVal) {
            isExactMatch = Self.matchesWildcardPattern(text: keyText, pattern: patText)
          } else {
            isExactMatch = false
          }
          if isExactMatch {
            return evaluateSingleCellInRange(
              onSheet: sheetName,
              row: isVertical ? coord : targetFixed,
              column: isVertical ? targetFixed : coord,
              context: &frame.context,
              depth: frame.depth + 1
            )
          }
        } else if let targetNum, let n = keyVal.numericValue, n <= targetNum, n >= approxBestNum {
          approxBestNum = n
          approxBestCoord = coord
        } else if let targetText, keyVal.numericValue == nil, keyVal != .empty {
          let candidateText = keyVal.description
          let cmpTarget = candidateText.caseInsensitiveCompare(targetText)
          if cmpTarget == .orderedAscending || cmpTarget == .orderedSame {
            let isBetter: Bool
            if let prevBest = approxBestText {
              let cmpPrev = candidateText.caseInsensitiveCompare(prevBest)
              isBetter = (cmpPrev == .orderedDescending || cmpPrev == .orderedSame)
            } else {
              isBetter = true
            }
            if isBetter {
              approxBestText = candidateText
              approxBestCoord = coord
            }
          }
        }
      }
      if let matchedCoord = approxBestCoord {
        return evaluateSingleCellInRange(
          onSheet: sheetName,
          row: isVertical ? matchedCoord : targetFixed,
          column: isVertical ? targetFixed : matchedCoord,
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
        if case .error = lookupVal { return lookupVal }
        let matchMode: Int
        if frame.count >= 5, frame.args[4] != .empty, frame.args[4] != .missing {
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
        if frame.count == 6, frame.args[5] != .empty, frame.args[5] != .missing {
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
          for idx in searchIndices {
            if case .text(let itemText) = lookupArr[idx],
              Self.matchesWildcardPattern(text: itemText, pattern: pattern)
            {
              return returnArr[idx]
            }
            if Self.cellValuesEqual(lookupArr[idx], lookupVal) {
              return returnArr[idx]
            }
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

        if frame.count >= 4, frame.args[3] != .empty, frame.args[3] != .missing {
          return frame.arg(3, in: self)
        }
        return .error("#N/A")
      }

    case "INDEX":
      guard (2...3).contains(frame.count) else { return .error("#VALUE!") }
      let sheetName: String?
      let range: CellRange
      switch resolveRangeArgument(from: frame.args[0], frame: &frame) {
      case .failure(let err):
        return .error(err)
      case .success(let s, let r):
        sheetName = s
        range = r
      }
      guard let rowNumVal = frame.arg(1, in: self).numericValue,
        let rawRowIdx = Self.safeInt(rowNumVal)
      else { return .error("#VALUE!") }
      let colNumVal = frame.count == 3 ? (frame.arg(2, in: self).numericValue ?? 1.0) : 1.0
      guard let rawColIdx = Self.safeInt(colNumVal),
        let bounds = rangeBounds(range, allowWholeColumnOrRow: true)
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
      if frame.count == 3, frame.args[2] != .missing {
        guard let mtNum = frame.arg(2, in: self).numericValue, let mtInt = Self.safeInt(mtNum)
        else {
          return .error("#VALUE!")
        }
        matchType = mtInt
      } else {
        matchType = 1
      }
      if case .success(_, let range) = resolveRangeArgument(from: frame.args[1], frame: &frame) {
        guard let bounds = rangeBounds(range, allowWholeColumnOrRow: true) else {
          return .error("#REF!")
        }
        if bounds.rowCount > 1 && bounds.colCount > 1 {
          return .error("#N/A")
        }
      }
      switch frame.resolve(1..<2, in: self) {
      case .failure(let err): return .error(err)
      case .success(let lookupArr):
        let lookupVal = frame.arg(0, in: self)
        if case .error = lookupVal { return lookupVal }
        if matchType == 0 {
          let isWildcardQuery: Bool
          if case .text(let pat) = lookupVal {
            isWildcardQuery = pat.contains("*") || pat.contains("?") || pat.contains("~")
          } else {
            isWildcardQuery = false
          }
          for (idx, item) in lookupArr.enumerated() where item != .empty {
            if isWildcardQuery, case .text(let pat) = lookupVal {
              if case .text(let itemText) = item,
                Self.matchesWildcardPattern(text: itemText, pattern: pat)
              {
                return .number(Double(idx + 1))
              }
            } else {
              switch (lookupVal, item) {
              case (.number(let a), .number(let b)) where a == b:
                return .number(Double(idx + 1))
              case (.text(let a), .text(let b)) where a.lowercased() == b.lowercased():
                return .number(Double(idx + 1))
              case (.boolean(let a), .boolean(let b)) where a == b:
                return .number(Double(idx + 1))
              default:
                break
              }
            }
          }
          return .error("#N/A")
        } else if matchType > 0 {
          switch lookupVal {
          case .number(let targetNum):
            var bestIdx: Int?
            var bestNum = -Double.infinity
            for (idx, item) in lookupArr.enumerated() {
              if case .number(let n) = item, n <= targetNum, n >= bestNum {
                bestNum = n
                bestIdx = idx
              }
            }
            if let bestIdx { return .number(Double(bestIdx + 1)) }
          case .text(let targetStr):
            let targetFolded = targetStr.lowercased()
            var bestIdx: Int?
            var bestFolded: String?
            for (idx, item) in lookupArr.enumerated() {
              guard case .text(let s) = item else { continue }
              let sFolded = s.lowercased()
              if sFolded <= targetFolded {
                if let prevBest = bestFolded {
                  if sFolded >= prevBest {
                    bestFolded = sFolded
                    bestIdx = idx
                  }
                } else {
                  bestFolded = sFolded
                  bestIdx = idx
                }
              }
            }
            if let bestIdx { return .number(Double(bestIdx + 1)) }
          case .boolean(let targetBool):
            var bestIdx: Int?
            for (idx, item) in lookupArr.enumerated() {
              if case .boolean(let b) = item, (!b || targetBool) {
                bestIdx = idx
              }
            }
            if let bestIdx { return .number(Double(bestIdx + 1)) }
          default:
            break
          }
          return .error("#N/A")
        } else {
          switch lookupVal {
          case .number(let targetNum):
            var bestIdx: Int?
            var bestNum = Double.infinity
            for (idx, item) in lookupArr.enumerated() {
              if case .number(let n) = item, n >= targetNum, n <= bestNum {
                bestNum = n
                bestIdx = idx
              }
            }
            if let bestIdx { return .number(Double(bestIdx + 1)) }
          case .text(let targetStr):
            let targetFolded = targetStr.lowercased()
            var bestIdx: Int?
            var bestFolded: String?
            for (idx, item) in lookupArr.enumerated() {
              guard case .text(let s) = item else { continue }
              let sFolded = s.lowercased()
              if sFolded >= targetFolded {
                if let prevBest = bestFolded {
                  if sFolded <= prevBest {
                    bestFolded = sFolded
                    bestIdx = idx
                  }
                } else {
                  bestFolded = sFolded
                  bestIdx = idx
                }
              }
            }
            if let bestIdx { return .number(Double(bestIdx + 1)) }
          case .boolean(let targetBool):
            var bestIdx: Int?
            for (idx, item) in lookupArr.enumerated() {
              if case .boolean(let b) = item, (b || !targetBool) {
                bestIdx = idx
              }
            }
            if let bestIdx { return .number(Double(bestIdx + 1)) }
          default:
            break
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
      if frame.count == 0 {
        return .number(Double((frame.context.currentCoord?.row ?? 0) + 1))
      }
      guard frame.count == 1 else { return .error("#VALUE!") }
      switch resolveRangeArgument(from: frame.args[0], frame: &frame) {
      case .success(_, let range):
        return .number(Double(range.start.row + 1))
      case .failure(let err):
        return .error(err == "#VALUE!" ? "#REF!" : err)
      }

    case "COLUMN":
      if frame.count == 0 {
        return .number(Double((frame.context.currentCoord?.column ?? 0) + 1))
      }
      guard frame.count == 1 else { return .error("#VALUE!") }
      switch resolveRangeArgument(from: frame.args[0], frame: &frame) {
      case .success(_, let range):
        return .number(Double(range.start.column + 1))
      case .failure(let err):
        return .error(err == "#VALUE!" ? "#REF!" : err)
      }

    case "ROWS":
      guard frame.count == 1 else { return .error("#VALUE!") }
      switch resolveRangeArgument(from: frame.args[0], frame: &frame) {
      case .success(_, let range):
        return .number(Double(abs(range.end.row - range.start.row) + 1))
      case .failure(let err):
        if err != "#VALUE!" { return .error(err) }
        let evaluated = frame.arg(0, in: self)
        if case .error = evaluated { return evaluated }
        return .number(1.0)
      }

    case "COLUMNS":
      guard frame.count == 1 else { return .error("#VALUE!") }
      switch resolveRangeArgument(from: frame.args[0], frame: &frame) {
      case .success(_, let range):
        return .number(Double(abs(range.end.column - range.start.column) + 1))
      case .failure(let err):
        if err != "#VALUE!" { return .error(err) }
        let evaluated = frame.arg(0, in: self)
        if case .error = evaluated { return evaluated }
        return .number(1.0)
      }

    case "TRANSPOSE":
      guard frame.count == 1 else { return .error("#VALUE!") }
      switch resolveRangeArgument(from: frame.args[0], frame: &frame) {
      case .success(let sheetName, let range):
        let coord = range.start.packedCoordinate
        let val: EvaluatedCellValue
        if let sheetName {
          val = evaluateOnSheet(
            named: sheetName, at: coord, context: &frame.context, depth: frame.depth + 1)
        } else {
          val = evaluateInternal(at: coord, context: &frame.context, depth: frame.depth + 1)
        }
        return val == .empty ? .number(0.0) : val
      case .failure(let err):
        if err != "#VALUE!" { return .error(err) }
        return frame.arg(0, in: self)
      }

    default:
      return nil
    }
  }


  internal func evaluateTextFunction(name: String, frame: inout EvalFrame) -> EvaluatedCellValue? {
    switch name {
    case "CONCAT", "CONCATENATE":
      switch frame.resolve(frame.indices, in: self) {
      case .failure(let err): return .error(err)
      case .success(let vals):
        var result = ""
        for val in vals {
          let part = val.description
          guard result.count + part.count <= Self.maxCellTextLength else {
            return .error(.value)
          }
          result.append(part)
        }
        return .text(result)
      }
    case "TEXTJOIN":
      guard frame.count >= 3 else { return .error("#VALUE!") }
      let delimiter = frame.arg(0, in: self).description
      let ignoreEmpty = frame.arg(1, in: self).booleanValue ?? true
      switch frame.resolve(2..<frame.count, in: self) {
      case .failure(let err): return .error(err)
      case .success(let vals):
        var result = ""
        var isFirst = true
        for val in vals {
          let part = val.description
          if ignoreEmpty && part.isEmpty { continue }
          let addedCount = (isFirst ? 0 : delimiter.count) + part.count
          guard result.count + addedCount <= Self.maxCellTextLength else {
            return .error(.value)
          }
          if !isFirst {
            result.append(delimiter)
          }
          result.append(part)
          isFirst = false
        }
        return .text(result)
      }
    case "LEFT", "RIGHT":
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
      return .text(name == "LEFT" ? String(text.prefix(count)) : String(text.suffix(count)))
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
      guard !target.isEmpty else {
        guard source.count <= Self.maxCellTextLength else { return .error(.value) }
        return .text(source)
      }
      let targetOccurrence: Int?
      if frame.count == 4, let instNum = frame.arg(3, in: self).numericValue {
        guard let occ = Self.safeInt(instNum), occ >= 1 else {
          return .error("#VALUE!")
        }
        targetOccurrence = occ
      } else {
        targetOccurrence = nil
      }
      var currentOccurrence = 0
      var result = ""
      var searchStart = source.startIndex
      while let range = source.range(of: target, range: searchStart..<source.endIndex) {
        currentOccurrence += 1
        let prefixSlice = source[searchStart..<range.lowerBound]
        guard result.count + prefixSlice.count <= Self.maxCellTextLength else {
          return .error(.value)
        }
        result.append(contentsOf: prefixSlice)
        if let targetOcc = targetOccurrence {
          if currentOccurrence == targetOcc {
            let suffixSlice = source[range.upperBound..<source.endIndex]
            guard result.count + replacement.count + suffixSlice.count <= Self.maxCellTextLength
            else {
              return .error(.value)
            }
            result.append(replacement)
            result.append(contentsOf: suffixSlice)
            return .text(result)
          } else {
            guard result.count + target.count <= Self.maxCellTextLength else {
              return .error(.value)
            }
            result.append(target)
          }
        } else {
          guard result.count + replacement.count <= Self.maxCellTextLength else {
            return .error(.value)
          }
          result.append(replacement)
        }
        searchStart = range.upperBound
      }
      let trailingSlice = source[searchStart..<source.endIndex]
      guard result.count + trailingSlice.count <= Self.maxCellTextLength else {
        return .error(.value)
      }
      result.append(contentsOf: trailingSlice)
      return .text(result)
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
      let removedCount = oldText.distance(from: startIdx, to: endIdx)
      let finalCount = oldText.count - removedCount + newText.count
      guard finalCount <= Self.maxCellTextLength else {
        return .error(.value)
      }
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
      if name == "SEARCH"
        && (needle.contains("*") || needle.contains("?") || needle.contains("~"))
      {
        let chars = Array(haystack)
        let trailingTildes = needle.reversed().prefix(while: { $0 == "~" }).count
        let prefixPattern = (trailingTildes % 2 == 1) ? (needle + "~*") : (needle + "*")
        for pos in startOffset..<chars.count {
          let tail = String(chars[pos...])
          if Self.matchesWildcardPattern(text: tail, pattern: prefixPattern) {
            return .number(Double(pos + 1))
          }
        }
        return .error("#VALUE!")
      }
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
      let argVal = frame.arg(0, in: self)
      if case .error = argVal { return argVal }
      switch argVal {
      case .number(let n):
        return n.isFinite ? .number(n) : .error("#NUM!")
      case .text(let s):
        guard let num = SheetValueCoercion.parseValueText(s) else { return .error("#VALUE!") }
        return .number(num)
      default:
        return .error("#VALUE!")
      }
    case "TEXT":
      guard frame.count == 2 else { return .error("#VALUE!") }
      let val = frame.arg(0, in: self)
      if case .error = val { return val }
      let pattern = frame.arg(1, in: self).description
      return SheetValueCoercion.formatTextPattern(val, pattern: pattern)
    case "FIXED":
      guard (1...3).contains(frame.count), let num = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      let decimals: Int
      if frame.count >= 2 {
        guard let dNum = frame.arg(1, in: self).numericValue, let dInt = Self.safeInt(dNum) else {
          return .error("#VALUE!")
        }
        decimals = dInt
      } else {
        decimals = 2
      }
      let noCommas = frame.count == 3 ? (frame.arg(2, in: self).booleanValue ?? false) : false
      let formatted = SheetValueCoercion.formatFixedNumber(
        num, decimals: decimals, noCommas: noCommas)
      return .text(formatted)
    case "T":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let v = frame.arg(0, in: self)
      if case .error = v { return v }
      if case .text(let s) = v { return .text(s) }
      return .text("")
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
}
