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

  // MARK: - 4. Lookup & Reference Functions

  internal func evaluateLookupAndRefFunction(name: String, frame: inout EvalFrame)
    -> EvaluatedCellValue?
  {
    switch name {
    case "VLOOKUP":
      guard (3...4).contains(frame.count),
        let range = extractCellRange(from: frame.args[1]),
        let colIndexVal = frame.arg(2, in: self).numericValue,
        let rawColIdx = Self.safeInt(colIndexVal)
      else { return .error("#VALUE!") }
      if let sheetName = frame.args[1].targetSheetName, !hasSheet(named: sheetName) {
        return .error("#REF!")
      }
      let lookupVal = frame.arg(0, in: self)
      let isApprox = frame.count == 4 ? (frame.arg(3, in: self).booleanValue ?? false) : false
      let grid = evaluateGridInternal(
        onSheet: frame.args[1].targetSheetName, in: range, context: &frame.context,
        depth: frame.depth + 1)
      let colIdx = rawColIdx - 1
      guard let firstRow = grid.first, colIdx >= 0, colIdx < firstRow.count else {
        return .error("#REF!")
      }
      for row in grid where Self.cellValuesEqual(row[0], lookupVal) {
        return row[colIdx]
      }
      if isApprox, let targetNum = lookupVal.numericValue {
        var bestRow: [EvaluatedCellValue]?
        var bestNum = -Double.infinity
        for row in grid {
          if let n = row[0].numericValue, n <= targetNum, n >= bestNum {
            bestNum = n
            bestRow = row
          }
        }
        if let bestRow { return bestRow[colIdx] }
      }
      return .error("#N/A")

    case "HLOOKUP":
      guard (3...4).contains(frame.count),
        let range = extractCellRange(from: frame.args[1]),
        let rowIndexVal = frame.arg(2, in: self).numericValue,
        let rawRowIdx = Self.safeInt(rowIndexVal)
      else { return .error("#VALUE!") }
      if let sheetName = frame.args[1].targetSheetName, !hasSheet(named: sheetName) {
        return .error("#REF!")
      }
      let lookupVal = frame.arg(0, in: self)
      let isApprox = frame.count == 4 ? (frame.arg(3, in: self).booleanValue ?? false) : false
      let grid = evaluateGridInternal(
        onSheet: frame.args[1].targetSheetName, in: range, context: &frame.context,
        depth: frame.depth + 1)
      let rowIdx = rawRowIdx - 1
      guard rowIdx >= 0, rowIdx < grid.count, let headerRow = grid.first else {
        return .error("#REF!")
      }
      for col in headerRow.indices where Self.cellValuesEqual(headerRow[col], lookupVal) {
        return grid[rowIdx][col]
      }
      if isApprox, let targetNum = lookupVal.numericValue {
        var bestCol: Int?
        var bestNum = -Double.infinity
        for col in headerRow.indices {
          if let n = headerRow[col].numericValue, n <= targetNum, n >= bestNum {
            bestNum = n
            bestCol = col
          }
        }
        if let bestCol { return grid[rowIdx][bestCol] }
      }
      return .error("#N/A")

    case "XLOOKUP":
      guard (3...4).contains(frame.count) else { return .error("#VALUE!") }
      switch (frame.resolve(1..<2, in: self), frame.resolve(2..<3, in: self)) {
      case (let .failure(err), _), (_, let .failure(err)): return .error(err)
      case (.success(let lookupArr), .success(let returnArr)):
        guard lookupArr.count == returnArr.count else { return .error("#VALUE!") }
        let lookupVal = frame.arg(0, in: self)
        for idx in lookupArr.indices where Self.cellValuesEqual(lookupArr[idx], lookupVal) {
          return returnArr[idx]
        }
        return frame.count == 4 ? frame.arg(3, in: self) : .error("#N/A")
      }

    case "INDEX":
      guard (2...3).contains(frame.count),
        let range = extractCellRange(from: frame.args[0]),
        let rowNumVal = frame.arg(1, in: self).numericValue,
        let rawRowIdx = Self.safeInt(rowNumVal)
      else { return .error("#VALUE!") }
      if let sheetName = frame.args[0].targetSheetName, !hasSheet(named: sheetName) {
        return .error("#REF!")
      }
      let colNumVal = frame.count == 3 ? (frame.arg(2, in: self).numericValue ?? 1.0) : 1.0
      guard let rawColIdx = Self.safeInt(colNumVal) else { return .error("#VALUE!") }
      let grid = evaluateGridInternal(
        onSheet: frame.args[0].targetSheetName, in: range, context: &frame.context,
        depth: frame.depth + 1)
      let rIdx = rawRowIdx - 1
      let cIdx = rawColIdx - 1
      if grid.count == 1, frame.count == 2, rIdx >= 0, rIdx < grid[0].count {
        return grid[0][rIdx]
      }
      guard rIdx >= 0, rIdx < grid.count, cIdx >= 0, cIdx < grid[rIdx].count else {
        return .error("#REF!")
      }
      return grid[rIdx][cIdx]

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

    case "DATE":
      guard frame.count == 3,
        let yNum = frame.arg(0, in: self).numericValue,
        let mNum = frame.arg(1, in: self).numericValue,
        let dNum = frame.arg(2, in: self).numericValue,
        let y = Self.safeInt(yNum), let m = Self.safeInt(mNum), let d = Self.safeInt(dNum),
        let date = Self.makeUTCDate(year: y, month: m, day: d)
      else { return .error("#VALUE!") }
      let comps = Self.utcCalendar.dateComponents([.year, .month, .day], from: date)
      return .text(
        String(format: "%04d-%02d-%02d", comps.year ?? 0, comps.month ?? 1, comps.day ?? 1))

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
      return .boolean(val == .empty || val == .text(""))
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

  // MARK: - Date Helpers

  internal static let utcCalendar: Calendar = {
    var cal = Calendar(identifier: .gregorian)
    cal.timeZone = TimeZone(secondsFromGMT: 0)!
    return cal
  }()

  internal static func makeUTCDate(year: Int, month: Int, day: Int) -> Date? {
    var comps = DateComponents()
    comps.year = year
    comps.month = month
    comps.day = day
    comps.hour = 0
    comps.minute = 0
    comps.second = 0
    return utcCalendar.date(from: comps)
  }

  internal static func parseDateValue(_ value: EvaluatedCellValue) -> Date? {
    let raw = value.description.trimmingCharacters(in: .whitespacesAndNewlines)
    let separators = CharacterSet(charactersIn: "-/.")
    let parts = raw.components(separatedBy: separators)
    if parts.count == 3,
      let y = Int(parts[0]),
      let m = Int(parts[1]),
      let d = Int(parts[2])
    {
      return makeUTCDate(year: y, month: m, day: d)
    }
    return nil
  }
}
