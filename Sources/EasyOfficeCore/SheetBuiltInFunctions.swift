import Foundation

extension WorksheetModel {


  internal enum FormulaResolutionResult {
    case success([EvaluatedCellValue])
    case failure(String)
  }

  internal enum NumericResolutionResult {
    case success([Double])
    case failure(String)
  }

  internal struct EvalFrame {
    let args: [FormulaAST]
    var cache: [EvaluatedCellValue?]
    var context: EvaluationContext
    let depth: Int

    var count: Int { args.count }
    var indices: Range<Int> { args.indices }

    mutating func arg(_ index: Int, in sheet: WorksheetModel) -> EvaluatedCellValue {
      guard args.indices.contains(index) else { return .error("#VALUE!") }
      if let cached = cache[index] { return cached }
      let val = sheet.evaluateAST(args[index], context: &context, depth: depth + 1)
      cache[index] = val
      return val
    }

    mutating func rawCellOrArg(_ index: Int, in sheet: WorksheetModel) -> EvaluatedCellValue {
      guard args.indices.contains(index) else { return .error("#VALUE!") }
      switch args[index] {
      case .cellRef(let coord):
        return sheet.evaluateInternal(at: coord, context: &context, depth: depth + 1)
      case .sheetCellRef(let sheetName, let coord):
        return sheet.evaluateOnSheet(
          named: sheetName, at: coord, context: &context, depth: depth + 1)
      case .namedRangeRef(let rangeName):
        guard let notation = sheet.resolveNamedRange(rangeName) else { return .error("#NAME?") }
        if let addr = try? CellAddress(a1Notation: notation) {
          return sheet.evaluateInternal(
            at: addr.packedCoordinate, context: &context, depth: depth + 1)
        }
        return arg(index, in: sheet)
      default:
        return arg(index, in: sheet)
      }
    }

    mutating func resolve(_ range: Range<Int>, in sheet: WorksheetModel) -> FormulaResolutionResult
    {
      var values: [EvaluatedCellValue] = []
      if let err = forEachResolvedCell(in: range, sheet: sheet, { values.append($0) }) {
        return .failure(err)
      }
      return .success(values)
    }

    mutating func resolveNumeric(_ range: Range<Int>, in sheet: WorksheetModel) -> NumericResolutionResult {
      var nums: [Double] = []
      if let err = forEachResolvedCellWithOrigin(
        in: range,
        sheet: sheet,
        { item, isFromReference in
          if isFromReference {
            if case .number(let v) = item { nums.append(v) }
          } else if let v = item.numericValue {
            nums.append(v)
          }
        }
      ) {
        return .failure(err)
      }
      return .success(nums)
    }

    internal mutating func streamRangeCells(
      _ cellRange: CellRange,
      onSheet sheetName: String? = nil,
      in sheet: WorksheetModel,
      onSkippedEmptyCount: ((Int) -> Void)? = nil,
      _ body: (EvaluatedCellValue) -> Void
    ) -> String? {
      let minRow = min(cellRange.start.row, cellRange.end.row)
      let maxRow = max(cellRange.start.row, cellRange.end.row)
      let minCol = min(cellRange.start.column, cellRange.end.column)
      let maxCol = max(cellRange.start.column, cellRange.end.column)
      let count = (maxRow - minRow + 1) * (maxCol - minCol + 1)
      guard count > 0 else { return nil }
      guard count <= CellRange.maxExpandedCellCount || cellRange.isWholeColumnOrRow else {
        return "#REF!"
      }

      if count > 4096 {
        let coords = sheet.populatedCoordinates(
          onSheet: sheetName,
          minRow: minRow,
          maxRow: maxRow,
          minCol: minCol,
          maxCol: maxCol
        )
        for coord in coords {
          let v =
            sheetName.map {
              sheet.evaluateOnSheet(named: $0, at: coord, context: &context, depth: depth + 1)
            } ?? sheet.evaluateInternal(at: coord, context: &context, depth: depth + 1)
          if case .error(let code) = v { return code }
          body(v)
        }
        onSkippedEmptyCount?(max(0, count - coords.count))
        return nil
      }

      for r in minRow...maxRow {
        for c in minCol...maxCol {
          let coord = PackedCellCoordinate(row: r, column: c)
          let v =
            sheetName.map {
              sheet.evaluateOnSheet(named: $0, at: coord, context: &context, depth: depth + 1)
            } ?? sheet.evaluateInternal(at: coord, context: &context, depth: depth + 1)
          if case .error(let code) = v { return code }
          body(v)
        }
      }
      return nil
    }

    mutating func forEachResolvedCell(
      in range: Range<Int>,
      sheet: WorksheetModel,
      _ body: (EvaluatedCellValue) -> Void
    ) -> String? {
      forEachResolvedCellWithOrigin(in: range, sheet: sheet) { item, _ in
        body(item)
      }
    }

    mutating func forEachResolvedCellWithOrigin(
      in range: Range<Int>,
      sheet: WorksheetModel,
      onSkippedEmptyCount: ((Int) -> Void)? = nil,
      _ body: (EvaluatedCellValue, Bool) -> Void
    ) -> String? {
      for idx in range {
        switch args[idx] {
        case .missing:
          continue
        case .rangeRef(let cellRange):
          if let err = streamRangeCells(
            cellRange, in: sheet, onSkippedEmptyCount: onSkippedEmptyCount, { body($0, true) })
          {
            return err
          }
        case .cellRef(let coord):
          let v = sheet.evaluateInternal(at: coord, context: &context, depth: depth + 1)
          if case .error(let code) = v { return code }
          body(v, true)
        case .sheetCellRef(let sheetName, let coord):
          guard sheet.hasSheet(named: sheetName) else { return "#REF!" }
          let v = sheet.evaluateOnSheet(
            named: sheetName, at: coord, context: &context, depth: depth + 1)
          if case .error(let code) = v { return code }
          body(v, true)
        case .namedRangeRef(let rangeName):
          guard let notation = sheet.resolveNamedRange(rangeName) else { return "#NAME?" }
          switch sheet.parseReferenceNotation(notation) {
          case .failure(let err):
            return err
          case .success(let parsed):
            if let err = streamRangeCells(
              parsed.range, onSheet: parsed.sheetName, in: sheet,
              onSkippedEmptyCount: onSkippedEmptyCount, { body($0, true) })
            {
              return err
            }
          }
        case .sheetRangeRef(let sheetName, let cellRange):
          guard sheet.hasSheet(named: sheetName) else { return "#REF!" }
          if let err = streamRangeCells(
            cellRange, onSheet: sheetName, in: sheet,
            onSkippedEmptyCount: onSkippedEmptyCount, { body($0, true) })
          {
            return err
          }
        case .functionCall(let fnName, let fnArgs) where fnName == "OFFSET":
          switch sheet.resolveOffsetRange(args: fnArgs, frame: &self) {
          case .failure(let err):
            return err
          case .success(let sheetName, let resolvedRange):
            if let sheetName {
              guard sheet.hasSheet(named: sheetName) else { return "#REF!" }
              if let err = streamRangeCells(
                resolvedRange, onSheet: sheetName, in: sheet,
                onSkippedEmptyCount: onSkippedEmptyCount, { body($0, true) })
              {
                return err
              }
            } else {
              if let err = streamRangeCells(
                resolvedRange, in: sheet, onSkippedEmptyCount: onSkippedEmptyCount,
                { body($0, true) })
              {
                return err
              }
            }
          }
        case .functionCall(let fnName, let fnArgs) where fnName == "INDIRECT":
          switch sheet.resolveIndirectRange(args: fnArgs, frame: &self) {
          case .failure(let err):
            return err
          case .success(let sheetName, let resolvedRange, _):
            if let sheetName {
              guard sheet.hasSheet(named: sheetName) else { return "#REF!" }
              if let err = streamRangeCells(
                resolvedRange, onSheet: sheetName, in: sheet,
                onSkippedEmptyCount: onSkippedEmptyCount, { body($0, true) })
              {
                return err
              }
            } else {
              if let err = streamRangeCells(
                resolvedRange, in: sheet, onSkippedEmptyCount: onSkippedEmptyCount,
                { body($0, true) })
              {
                return err
              }
            }
          }
        case .functionCall(let fnName, let fnArgs) where fnName == "TRANSPOSE" && fnArgs.count == 1:
          switch sheet.resolveRangeArgument(from: fnArgs[0], frame: &self) {
          case .failure(let err):
            if err != "#VALUE!" { return err }
            let v = arg(idx, in: sheet)
            if case .error(let code) = v { return code }
            body(v, false)
          case .success(let sheetName, let resolvedRange):
            if let sheetName {
              guard sheet.hasSheet(named: sheetName) else { return "#REF!" }
              if let err = streamRangeCells(
                resolvedRange, onSheet: sheetName, in: sheet,
                onSkippedEmptyCount: onSkippedEmptyCount, { body($0, true) })
              {
                return err
              }
            } else {
              if let err = streamRangeCells(
                resolvedRange, in: sheet, onSkippedEmptyCount: onSkippedEmptyCount,
                { body($0, true) })
              {
                return err
              }
            }
          }
        case .error(let err):
          return err.rawValue
        default:
          let v = arg(idx, in: sheet)
          if case .error(let code) = v { return code }
          body(v, false)
        }
      }
      return nil
    }

    internal struct NumericStreamStats {
      let sum: Double
      let product: Double
      let minVal: Double?
      let maxVal: Double?
      let numericCount: Int
    }

    internal struct CountStreamStats {
      let numberOnlyCount: Int
      let nonEmptyCount: Int
      let blankCount: Int
    }

    mutating func streamNumericStats(in sheet: WorksheetModel) -> Result<NumericStreamStats, ErrorCode>
    {
      var s0 = 0.0
      var s1 = 0.0
      var s2 = 0.0
      var s3 = 0.0
      var product = 1.0
      var minVal: Double?
      var maxVal: Double?
      var numericCount = 0

      if let err = forEachResolvedCellWithOrigin(
        in: indices,
        sheet: sheet,
        { item, isFromReference in
          let numericCandidate: Double?
          if isFromReference {
            if case .number(let v) = item {
              numericCandidate = v
            } else {
              numericCandidate = nil
            }
          } else {
            numericCandidate = item.numericValue
          }
          if let v = numericCandidate {
            switch numericCount & 3 {
            case 0: s0 += v
            case 1: s1 += v
            case 2: s2 += v
            default: s3 += v
            }
            product *= v
            minVal = minVal.map { min($0, v) } ?? v
            maxVal = maxVal.map { max($0, v) } ?? v
            numericCount += 1
          }
        }
      ) {
        return .failure(ErrorCode(code: err))
      }

      return .success(
        NumericStreamStats(
          sum: (s0 + s1) + (s2 + s3),
          product: numericCount == 0 ? 0.0 : product,
          minVal: minVal,
          maxVal: maxVal,
          numericCount: numericCount
        )
      )
    }

    mutating func streamCountStats(in sheet: WorksheetModel) -> Result<CountStreamStats, ErrorCode> {
      var numberOnlyCount = 0
      var nonEmptyCount = 0
      var blankCount = 0

      if let err = forEachResolvedCellWithOrigin(
        in: indices,
        sheet: sheet,
        onSkippedEmptyCount: { skipped in
          blankCount += skipped
        },
        { item, isFromReference in
          if isFromReference {
            if case .number = item {
              numberOnlyCount += 1
            }
          } else if item.numericValue != nil {
            numberOnlyCount += 1
          }
          if item != .empty {
            nonEmptyCount += 1
          }
          if item == .empty || item == .text("") {
            blankCount += 1
          }
        }
      ) {
        return .failure(ErrorCode(code: err))
      }

      return .success(
        CountStreamStats(
          numberOnlyCount: numberOnlyCount,
          nonEmptyCount: nonEmptyCount,
          blankCount: blankCount
        )
      )
    }
  }

  internal struct ErrorCode: Error {
    let code: String
  }

  internal static func safeInt(_ value: Double) -> Int? {
    guard value.isFinite else { return nil }
    let t = trunc(value)
    guard t > -9223372036854775808.0, t < 9223372036854775808.0 else { return nil }
    return Int(exactly: t)
  }

  internal func populatedCoordinates(
    onSheet sheetName: String?,
    minRow: Int,
    maxRow: Int,
    minCol: Int,
    maxCol: Int
  ) -> [PackedCellCoordinate] {
    let dict: [PackedCellCoordinate: String]
    if let sheetName {
      if sheetName.caseInsensitiveCompare(name) == .orderedSame {
        dict = cellsByCoordinate
      } else if let page = additionalSheets.first(where: {
        $0.name.caseInsensitiveCompare(sheetName) == .orderedSame
      }) {
        dict = page.cellsByCoordinate
      } else {
        return []
      }
    } else {
      dict = cellsByCoordinate
    }
    var coords: [PackedCellCoordinate] = []
    for coord in dict.keys {
      let r = coord.row
      let c = coord.column
      if r >= minRow && r <= maxRow && c >= minCol && c <= maxCol {
        coords.append(coord)
      }
    }
    coords.sort { a, b in
      if a.row != b.row { return a.row < b.row }
      return a.column < b.column
    }
    return coords
  }

  internal func evaluateFunction(
    name: String,
    args: [FormulaAST],
    context: inout EvaluationContext,
    depth: Int
  ) -> EvaluatedCellValue {
    var takenContext = EvaluationContext()
    swap(&takenContext, &context)
    var frame = EvalFrame(
      args: args,
      cache: [EvaluatedCellValue?](repeating: nil, count: args.count),
      context: takenContext,
      depth: depth
    )
    defer { swap(&context, &frame.context) }

    if !Self.lazyOrRefFunctions.contains(name) && name != "SUMPRODUCT" {
      for (idx, item) in args.enumerated() {
        switch item {
        case .rangeRef, .sheetRangeRef, .missing:
          continue
        case .functionCall(let fnName, _) where fnName == "OFFSET" || fnName == "INDIRECT":
          continue
        case .namedRangeRef(let rangeName):
          guard resolveNamedRange(rangeName) != nil else { return .error("#NAME?") }
          continue
        default:
          let evaluated = frame.arg(idx, in: self)
          if case .error = evaluated { return evaluated }
        }
      }
    }

    if let res = evaluateMathAndTrigFunction(name: name, frame: &frame) { return res }
    if let res = evaluateStatisticalFunction(name: name, frame: &frame) { return res }
    if let res = evaluateLogicalFunction(name: name, frame: &frame) { return res }
    if let res = evaluateLookupAndRefFunction(name: name, frame: &frame) { return res }
    if let res = evaluateTextFunction(name: name, frame: &frame) { return res }
    if let res = evaluateFinancialAndDateFunction(name: name, frame: &frame) { return res }
    if let res = evaluateInfoFunction(name: name, frame: &frame) { return res }

    return .error("#NAME?")
  }


  private func evaluateMathAndTrigFunction(name: String, frame: inout EvalFrame)
    -> EvaluatedCellValue?
  {
    switch name {
    case "SUM":
      switch frame.streamNumericStats(in: self) {
      case .failure(let err): return .error(err.code)
      case .success(let stats): return .number(stats.sum)
      }
    case "AVERAGE":
      switch frame.streamNumericStats(in: self) {
      case .failure(let err): return .error(err.code)
      case .success(let stats):
        guard stats.numericCount > 0 else { return .error("#DIV/0!") }
        return .number(stats.sum / Double(stats.numericCount))
      }
    case "MIN":
      switch frame.streamNumericStats(in: self) {
      case .failure(let err): return .error(err.code)
      case .success(let stats): return .number(stats.minVal ?? 0.0)
      }
    case "MAX":
      switch frame.streamNumericStats(in: self) {
      case .failure(let err): return .error(err.code)
      case .success(let stats): return .number(stats.maxVal ?? 0.0)
      }
    case "COUNT":
      switch frame.streamCountStats(in: self) {
      case .failure(let err): return .error(err.code)
      case .success(let counts): return .number(Double(counts.numberOnlyCount))
      }
    case "COUNTA":
      switch frame.streamCountStats(in: self) {
      case .failure(let err): return .error(err.code)
      case .success(let counts): return .number(Double(counts.nonEmptyCount))
      }
    case "COUNTBLANK":
      switch frame.streamCountStats(in: self) {
      case .failure(let err): return .error(err.code)
      case .success(let counts): return .number(Double(counts.blankCount))
      }
    case "ABS":
      guard frame.count == 1, let num = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      return .number(abs(num))
    case "ROUND", "ROUNDUP", "ROUNDDOWN":
      guard frame.count == 2,
        let num = frame.arg(0, in: self).numericValue,
        let digitsNum = frame.arg(1, in: self).numericValue
      else { return .error("#VALUE!") }
      guard let digits = Self.safeInt(digitsNum) else { return .error("#NUM!") }
      if digits >= 300 { return .number(num) }
      if digits <= -300 { return .number(0.0) }
      let roundedVal: Double
      if digits == 0 {
        if name == "ROUND" {
          roundedVal = num.rounded(.toNearestOrAwayFromZero)
        } else if name == "ROUNDUP" {
          roundedVal = num >= 0 ? ceil(num) : floor(num)
        } else {
          roundedVal = trunc(num)
        }
      } else {
        let factor = pow(10.0, Double(digits))
        guard factor.isFinite, factor != 0 else {
          return digits > 0 ? .number(num) : .number(0.0)
        }
        let scaled = num * factor
        guard scaled.isFinite else {
          return digits > 0 ? .number(num) : .number(0.0)
        }
        let ulpNudge = abs(scaled) * Double.ulpOfOne * 2.0
        if name == "ROUND" {
          let nudged = scaled >= 0 ? scaled + ulpNudge : scaled - ulpNudge
          roundedVal = nudged.rounded(.toNearestOrAwayFromZero) / factor
        } else if name == "ROUNDUP" {
          roundedVal = (scaled >= 0 ? ceil(scaled - ulpNudge) : floor(scaled + ulpNudge)) / factor
        } else {
          let nudged = scaled >= 0 ? scaled + ulpNudge : scaled - ulpNudge
          roundedVal = trunc(nudged) / factor
        }
      }
      guard roundedVal.isFinite else { return digits > 0 ? .number(num) : .error("#NUM!") }
      return .number(roundedVal)
    case "INT":
      guard frame.count == 1, let num = frame.arg(0, in: self).numericValue, num.isFinite else {
        return .error("#VALUE!")
      }
      return .number(floor(num))
    case "TRUNC":
      guard (1...2).contains(frame.count), let num = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      let digits: Int
      if frame.count == 2 {
        guard let dNum = frame.arg(1, in: self).numericValue, let dInt = Self.safeInt(dNum) else {
          return .error("#VALUE!")
        }
        digits = dInt
      } else {
        digits = 0
      }
      if digits >= 300 { return .number(num) }
      if digits <= -300 { return .number(0.0) }
      if digits == 0 { return .number(trunc(num)) }
      let factor = pow(10.0, Double(digits))
      guard factor.isFinite, factor != 0 else { return digits > 0 ? .number(num) : .number(0.0) }
      let scaled = num * factor
      guard scaled.isFinite else { return digits > 0 ? .number(num) : .number(0.0) }
      let result = trunc(scaled) / factor
      return result.isFinite ? .number(result) : .number(num)
    case "MOD":
      guard frame.count == 2,
        let num = frame.arg(0, in: self).numericValue,
        let divisor = frame.arg(1, in: self).numericValue
      else { return .error("#VALUE!") }
      guard divisor != 0.0 else { return .error("#DIV/0!") }
      return .number(num - divisor * floor(num / divisor))
    case "SQRT":
      guard frame.count == 1, let num = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      return num >= 0.0 ? .number(sqrt(num)) : .error("#NUM!")
    case "POWER":
      guard frame.count == 2,
        let base = frame.arg(0, in: self).numericValue,
        let expVal = frame.arg(1, in: self).numericValue
      else { return .error("#VALUE!") }
      let result = pow(base, expVal)
      return result.isFinite ? .number(result) : .error("#NUM!")
    case "PRODUCT":
      switch frame.streamNumericStats(in: self) {
      case .failure(let err): return .error(err.code)
      case .success(let stats): return .number(stats.product)
      }
    case "SUMPRODUCT":
      guard frame.count > 0 else { return .error("#VALUE!") }
      var vectors: [[Double]] = []
      var dims: (Int, Int)? = nil
      for idx in frame.indices {
        switch evaluateASTAs2DArray(frame.args[idx], frame: &frame) {
        case .failure(let err): return .error(err.code)
        case .success((let grid, let isDirectRange)):
          let rCount = grid.count
          let cCount = grid.first?.count ?? 0
          if let expected = dims {
            guard expected.0 == rCount && expected.1 == cCount else {
              return .error("#VALUE!")
            }
          } else {
            dims = (rCount, cCount)
          }
          var flat: [Double] = []
          flat.reserveCapacity(rCount * cCount)
          for row in grid {
            for item in row {
              if isDirectRange {
                if case .number(let n) = item {
                  flat.append(n)
                } else {
                  flat.append(0.0)
                }
              } else {
                flat.append(item.numericValue ?? 0.0)
              }
            }
          }
          vectors.append(flat)
        }
      }
      guard let length = vectors.first?.count, length > 0,
        vectors.allSatisfy({ $0.count == length })
      else {
        return .error("#VALUE!")
      }
      var total = 0.0
      for i in 0..<length {
        total += vectors.reduce(1.0) { $0 * $1[i] }
      }
      return .number(total)
    case "SUMIF":
      guard (2...3).contains(frame.count) else { return .error("#VALUE!") }
      switch streamMultiCriteriaStats(
        targetRangeIndex: frame.count == 3 ? 2 : 0, firstPairIndex: 0, pairEndIndex: 2,
        frame: &frame)
      {
      case .failure(let err): return .error(err.code)
      case .success(let stats): return .number(stats.sum)
      }
    case "COUNTIF":
      guard frame.count == 2 else { return .error("#VALUE!") }
      switch streamMultiCriteriaStats(
        targetRangeIndex: nil, firstPairIndex: 0, pairEndIndex: 2, frame: &frame)
      {
      case .failure(let err): return .error(err.code)
      case .success(let stats): return .number(Double(stats.matchCount))
      }
    case "AVERAGEIF":
      guard (2...3).contains(frame.count) else { return .error("#VALUE!") }
      switch streamMultiCriteriaStats(
        targetRangeIndex: frame.count == 3 ? 2 : 0, firstPairIndex: 0, pairEndIndex: 2,
        frame: &frame)
      {
      case .failure(let err): return .error(err.code)
      case .success(let stats):
        guard stats.numericCount > 0 else { return .error("#DIV/0!") }
        return .number(stats.sum / Double(stats.numericCount))
      }
    case "SUMIFS":
      switch streamMultiCriteriaStats(
        targetRangeIndex: 0, firstPairIndex: 1, pairEndIndex: frame.count, frame: &frame)
      {
      case .failure(let err): return .error(err.code)
      case .success(let stats): return .number(stats.sum)
      }
    case "COUNTIFS":
      switch streamMultiCriteriaStats(
        targetRangeIndex: nil, firstPairIndex: 0, pairEndIndex: frame.count, frame: &frame)
      {
      case .failure(let err): return .error(err.code)
      case .success(let stats): return .number(Double(stats.matchCount))
      }
    case "AVERAGEIFS":
      switch streamMultiCriteriaStats(
        targetRangeIndex: 0, firstPairIndex: 1, pairEndIndex: frame.count, frame: &frame)
      {
      case .failure(let err): return .error(err.code)
      case .success(let stats):
        guard stats.numericCount > 0 else { return .error("#DIV/0!") }
        return .number(stats.sum / Double(stats.numericCount))
      }
    case "MINIFS":
      switch streamMultiCriteriaStats(
        targetRangeIndex: 0, firstPairIndex: 1, pairEndIndex: frame.count, frame: &frame)
      {
      case .failure(let err): return .error(err.code)
      case .success(let stats): return .number(stats.minVal ?? 0.0)
      }
    case "MAXIFS":
      switch streamMultiCriteriaStats(
        targetRangeIndex: 0, firstPairIndex: 1, pairEndIndex: frame.count, frame: &frame)
      {
      case .failure(let err): return .error(err.code)
      case .success(let stats): return .number(stats.maxVal ?? 0.0)
      }
    case "CEILING":
      guard (1...2).contains(frame.count), let num = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      let sig = frame.count == 2 ? (frame.arg(1, in: self).numericValue ?? 1.0) : (num < 0 ? -1.0 : 1.0)
      guard sig != 0.0 else { return .number(0.0) }
      guard !(num > 0.0 && sig < 0.0) else { return .error("#NUM!") }
      let q = num / sig
      let result = ceil(q - (q >= 0 ? 1e-12 : -1e-12)) * sig
      return result.isFinite ? .number(result) : .error("#NUM!")
    case "FLOOR":
      guard (1...2).contains(frame.count), let num = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      let sig = frame.count == 2 ? (frame.arg(1, in: self).numericValue ?? 1.0) : (num < 0 ? -1.0 : 1.0)
      guard sig != 0.0 else { return num == 0.0 ? .number(0.0) : .error("#DIV/0!") }
      guard !(num > 0.0 && sig < 0.0) else { return .error("#NUM!") }
      let q = num / sig
      let result = floor(q + (q >= 0 ? 1e-12 : -1e-12)) * sig
      return result.isFinite ? .number(result) : .error("#NUM!")
    case "SIGN":
      guard frame.count == 1, let num = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      return .number(num > 0 ? 1.0 : (num < 0 ? -1.0 : 0.0))
    case "LN":
      guard frame.count == 1, let num = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      return num > 0 ? .number(log(num)) : .error("#NUM!")
    case "LOG":
      guard (1...2).contains(frame.count), let num = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      let base = frame.count == 2 ? (frame.arg(1, in: self).numericValue ?? 10.0) : 10.0
      guard num > 0, base > 0, base != 1.0 else { return .error("#NUM!") }
      return .number(log(num) / log(base))
    case "LOG10":
      guard frame.count == 1, let num = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      return num > 0 ? .number(log10(num)) : .error("#NUM!")
    case "EXP":
      guard frame.count == 1, let num = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      let result = exp(num)
      return result.isFinite ? .number(result) : .error("#NUM!")
    case "SIN", "COS", "TAN":
      guard frame.count == 1, let num = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      let res = name == "SIN" ? sin(num) : (name == "COS" ? cos(num) : tan(num))
      return res.isFinite ? .number(res) : .error("#NUM!")
    case "EVEN", "ODD":
      guard frame.count == 1, let num = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      if name == "EVEN" {
        if num == 0.0 { return .number(0.0) }
        let c = num >= 0 ? ceil(num) : floor(num)
        guard let i = Self.safeInt(c) else { return .error("#NUM!") }
        let res = i % 2 == 0 ? i : (num >= 0 ? i + 1 : i - 1)
        return .number(Double(res))
      } else {
        if num == 0.0 { return .number(1.0) }
        let c = num >= 0 ? ceil(num) : floor(num)
        guard let i = Self.safeInt(c) else { return .error("#NUM!") }
        let res = i % 2 != 0 ? i : (num >= 0 ? i + 1 : i - 1)
        return .number(Double(res))
      }
    case "RAND":
      guard frame.count == 0 else { return .error("#VALUE!") }
      var x = frame.context.rngState
      x ^= x << 13
      x ^= x >> 7
      x ^= x << 17
      frame.context.rngState = x
      let unit = Double(x & 0x001F_FFFF_FFFF_FFFF) / Double(1 << 53)
      return .number(unit)
    case "RANDBETWEEN":
      guard frame.count == 2,
        let lowVal = frame.arg(0, in: self).numericValue,
        let highVal = frame.arg(1, in: self).numericValue,
        let low = Self.safeInt(ceil(lowVal)),
        let high = Self.safeInt(floor(highVal))
      else { return .error("#VALUE!") }
      guard low <= high else { return .error("#NUM!") }
      if low == high { return .number(Double(low)) }
      var x = frame.context.rngState
      x ^= x << 13
      x ^= x >> 7
      x ^= x << 17
      frame.context.rngState = x
      let span = UInt64(high - low + 1)
      let picked = low + Int(x % span)
      return .number(Double(picked))
    case "SUMSQ":
      var sumSq = 0.0
      if let err = frame.forEachResolvedCellWithOrigin(
        in: frame.indices, sheet: self,
        { v, isFromReference in
          let num: Double? = isFromReference ? { if case .number(let n) = v { return n }; return nil }() : v.numericValue
          if let n = num { sumSq += n * n }
        })
      {
        return .error(err)
      }
      return .number(sumSq)
    case "SUBTOTAL":
      return evaluateSubtotalFunction(frame: &frame)
    case "MROUND":
      guard frame.count == 2,
        let num = frame.arg(0, in: self).numericValue,
        let multiple = frame.arg(1, in: self).numericValue
      else {
        return .error("#VALUE!")
      }
      if multiple == 0.0 || num == 0.0 {
        return .number(0.0)
      }
      guard (num > 0.0 && multiple > 0.0) || (num < 0.0 && multiple < 0.0) else {
        return .error("#NUM!")
      }
      let result = (num / multiple).rounded() * multiple
      return result.isFinite ? .number(result) : .error("#NUM!")
    case "QUOTIENT":
      guard frame.count == 2,
        let num = frame.arg(0, in: self).numericValue,
        let den = frame.arg(1, in: self).numericValue
      else {
        return .error("#VALUE!")
      }
      guard den != 0.0 else {
        return .error("#DIV/0!")
      }
      return .number(trunc(num / den))
    case "PI":
      guard frame.count == 0 else {
        return .error("#VALUE!")
      }
      return .number(Double.pi)
    case "FACT":
      guard frame.count == 1,
        let num = frame.arg(0, in: self).numericValue,
        let n = Self.safeInt(floor(num))
      else {
        return .error("#VALUE!")
      }
      guard n >= 0, n <= 170 else {
        return .error("#NUM!")
      }
      if n <= 1 {
        return .number(1.0)
      }
      var acc = 1.0
      for i in 2...n {
        acc *= Double(i)
      }
      return acc.isFinite ? .number(acc) : .error("#NUM!")
    case "COMBIN":
      guard frame.count == 2,
        let nVal = frame.arg(0, in: self).numericValue,
        let kVal = frame.arg(1, in: self).numericValue,
        let n = Self.safeInt(floor(nVal)),
        let k = Self.safeInt(floor(kVal))
      else {
        return .error("#VALUE!")
      }
      guard n >= 0, k >= 0, k <= n, n <= 1000 else {
        return .error("#NUM!")
      }
      let r = min(k, n - k)
      if r == 0 {
        return .number(1.0)
      }
      var res = 1.0
      for i in 1...r {
        res = res * Double(n - r + i) / Double(i)
      }
      return res.isFinite ? .number(res.rounded()) : .error("#NUM!")
    case "GCD":
      switch frame.resolveNumeric(frame.indices, in: self) {
      case .failure(let err):
        return .error(err)
      case .success(let nums):
        guard !nums.isEmpty else { return .error("#VALUE!") }
        var acc = 0
        for raw in nums {
          guard raw >= 0, let v = Self.safeInt(floor(raw)) else { return .error("#NUM!") }
          var a = acc
          var b = v
          while b != 0 {
            let t = b
            b = a % b
            a = t
          }
          acc = a
        }
        return .number(Double(acc))
      }
    case "LCM":
      switch frame.resolveNumeric(frame.indices, in: self) {
      case .failure(let err):
        return .error(err)
      case .success(let nums):
        guard !nums.isEmpty else { return .error("#VALUE!") }
        var acc = 1
        for raw in nums {
          guard raw >= 0, let v = Self.safeInt(floor(raw)) else { return .error("#NUM!") }
          if v == 0 || acc == 0 {
            acc = 0
            continue
          }
          var a = acc
          var b = v
          while b != 0 {
            let t = b
            b = a % b
            a = t
          }
          let (prod, overflow) = (acc / max(1, a)).multipliedReportingOverflow(by: v)
          guard !overflow else { return .error("#NUM!") }
          acc = prod
        }
        return .number(Double(acc))
      }
    default:
      return nil
    }
  }
}
