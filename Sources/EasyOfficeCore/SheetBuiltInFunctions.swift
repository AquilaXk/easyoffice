import Foundation

extension WorksheetModel {

  // MARK: - Modular Function Dispatch (92 Built-in Functions)

  internal enum FormulaResolutionResult {
    case success([EvaluatedCellValue])
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

    private mutating func streamRangeCells(
      _ cellRange: CellRange,
      onSheet sheetName: String? = nil,
      in sheet: WorksheetModel,
      _ body: (EvaluatedCellValue) -> Void
    ) -> String? {
      let minRow = min(cellRange.start.row, cellRange.end.row)
      let maxRow = max(cellRange.start.row, cellRange.end.row)
      let minCol = min(cellRange.start.column, cellRange.end.column)
      let maxCol = max(cellRange.start.column, cellRange.end.column)
      let count = (maxRow - minRow + 1) * (maxCol - minCol + 1)
      guard count > 0 else { return nil }
      guard count <= CellRange.maxExpandedCellCount else { return "#REF!" }
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
      for idx in range {
        switch args[idx] {
        case .rangeRef(let cellRange):
          if let err = streamRangeCells(cellRange, in: sheet, body) { return err }
        case .namedRangeRef(let rangeName):
          guard let notation = sheet.resolveNamedRange(rangeName) else { return "#NAME?" }
          if let cellRange = try? CellRange(notation: notation) {
            if let err = streamRangeCells(cellRange, in: sheet, body) { return err }
          } else if let addr = try? CellAddress(a1Notation: notation) {
            let v = sheet.evaluateInternal(
              at: addr.packedCoordinate, context: &context, depth: depth + 1)
            if case .error(let code) = v { return code }
            body(v)
          } else {
            return "#REF!"
          }
        case .sheetRangeRef(let sheetName, let cellRange):
          guard sheet.hasSheet(named: sheetName) else { return "#REF!" }
          if let err = streamRangeCells(cellRange, onSheet: sheetName, in: sheet, body) {
            return err
          }
        case .functionCall(let fnName, let fnArgs) where fnName == "OFFSET":
          switch sheet.resolveOffsetRange(args: fnArgs, frame: &self) {
          case .failure(let err):
            return err
          case .success(let sheetName, let resolvedRange):
            if let sheetName {
              guard sheet.hasSheet(named: sheetName) else { return "#REF!" }
              if let err = streamRangeCells(resolvedRange, onSheet: sheetName, in: sheet, body) {
                return err
              }
            } else {
              if let err = streamRangeCells(resolvedRange, in: sheet, body) {
                return err
              }
            }
          }
        case .error(let err):
          return err.rawValue
        default:
          let v = arg(idx, in: sheet)
          if case .error(let code) = v { return code }
          body(v)
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

      if let err = forEachResolvedCell(
        in: indices,
        sheet: sheet,
        { item in
          if let v = item.numericValue {
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

      if let err = forEachResolvedCell(
        in: indices,
        sheet: sheet,
        { item in
          if case .number = item {
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

    if !Self.lazyOrRefFunctions.contains(name) {
      for (idx, item) in args.enumerated() {
        switch item {
        case .rangeRef, .sheetRangeRef:
          continue
        case .functionCall(let fnName, _) where fnName == "OFFSET":
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

  // MARK: - 1. Math & Trig Functions

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
        let digitsNum = frame.arg(1, in: self).numericValue,
        let digits = Self.safeInt(digitsNum)
      else { return .error("#VALUE!") }
      guard abs(digits) <= 300 else { return .error("#NUM!") }
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
        let scaled = num * factor
        guard scaled.isFinite, factor.isFinite, factor != 0 else { return .error("#NUM!") }
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
      guard roundedVal.isFinite else { return .error("#NUM!") }
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
      let factor = pow(10.0, Double(digits))
      let result = trunc(num * factor) / factor
      return result.isFinite ? .number(result) : .error("#NUM!")
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
      for idx in frame.indices {
        switch frame.resolve(idx..<(idx + 1), in: self) {
        case .failure(let err): return .error(err)
        case .success(let vals): vectors.append(vals.map { $0.numericValue ?? 0.0 })
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
      let sig = frame.count == 2 ? (frame.arg(1, in: self).numericValue ?? 1.0) : 1.0
      guard sig != 0.0 else { return .number(0.0) }
      let q = num / sig
      let result = ceil(q - (q >= 0 ? 1e-12 : -1e-12)) * sig
      return result.isFinite ? .number(result) : .error("#NUM!")
    case "FLOOR":
      guard (1...2).contains(frame.count), let num = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      let sig = frame.count == 2 ? (frame.arg(1, in: self).numericValue ?? 1.0) : 1.0
      guard sig != 0.0 else { return .error("#DIV/0!") }
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
    case "SUMSQ":
      var sumSq = 0.0
      if let err = frame.forEachResolvedCell(
        in: frame.indices, sheet: self,
        { v in
          if let n = v.numericValue { sumSq += n * n }
        })
      {
        return .error(err)
      }
      return .number(sumSq)
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
      switch frame.resolve(frame.indices, in: self) {
      case .failure(let err):
        return .error(err)
      case .success(let vals):
        let nums = vals.compactMap(\.numericValue)
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
      switch frame.resolve(frame.indices, in: self) {
      case .failure(let err):
        return .error(err)
      case .success(let vals):
        let nums = vals.compactMap(\.numericValue)
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

  internal struct CriteriaStreamStats {
    var matchCount: Int = 0
    var numericCount: Int = 0
    var sum: Double = 0.0
    var minVal: Double? = nil
    var maxVal: Double? = nil
  }

  private struct CriteriaRangeSpec {
    let sheetName: String?
    let minRow: Int
    let minCol: Int
    let rowCount: Int
    let colCount: Int
    var totalCells: Int { rowCount * colCount }

    @inline(__always)
    func coordinate(at offset: Int) -> PackedCellCoordinate {
      PackedCellCoordinate(
        row: minRow + (offset / colCount),
        column: minCol + (offset % colCount)
      )
    }
  }

  private func resolveCriteriaRangeSpec(
    at argIndex: Int,
    frame: inout EvalFrame
  ) -> Result<CriteriaRangeSpec, ErrorCode> {
    let ast = frame.args[argIndex]
    let sheetName: String?
    let resolvedRange: CellRange
    switch ast {
    case .rangeRef(let r):
      sheetName = nil
      resolvedRange = r
    case .sheetRangeRef(let s, let r):
      guard hasSheet(named: s) else { return .failure(ErrorCode(code: "#REF!")) }
      sheetName = s
      resolvedRange = r
    case .namedRangeRef(let name):
      guard let notation = resolveNamedRange(name) else {
        return .failure(ErrorCode(code: "#NAME?"))
      }
      if let r = try? CellRange(notation: notation) {
        sheetName = nil
        resolvedRange = r
      } else if let addr = try? CellAddress(a1Notation: notation) {
        sheetName = nil
        resolvedRange = CellRange(start: addr, end: addr)
      } else {
        return .failure(ErrorCode(code: "#REF!"))
      }
    case .cellRef(let coord):
      return .success(
        CriteriaRangeSpec(
          sheetName: nil, minRow: coord.row, minCol: coord.column, rowCount: 1, colCount: 1
        )
      )
    case .sheetCellRef(let s, let coord):
      guard hasSheet(named: s) else { return .failure(ErrorCode(code: "#REF!")) }
      return .success(
        CriteriaRangeSpec(
          sheetName: s, minRow: coord.row, minCol: coord.column, rowCount: 1, colCount: 1
        )
      )
    case .functionCall(let fnName, let fnArgs) where fnName == "OFFSET":
      switch resolveOffsetRange(args: fnArgs, frame: &frame) {
      case .failure(let err):
        return .failure(ErrorCode(code: err))
      case .success(let s, let r):
        if let s, !hasSheet(named: s) {
          return .failure(ErrorCode(code: "#REF!"))
        }
        sheetName = s
        resolvedRange = r
      }
    case .error(let err):
      return .failure(ErrorCode(code: err.rawValue))
    default:
      return .failure(ErrorCode(code: "#VALUE!"))
    }
    guard let bounds = rangeBounds(resolvedRange) else {
      return .failure(ErrorCode(code: "#REF!"))
    }
    return .success(
      CriteriaRangeSpec(
        sheetName: sheetName,
        minRow: bounds.minRow,
        minCol: bounds.minCol,
        rowCount: bounds.rowCount,
        colCount: bounds.colCount
      )
    )
  }

  private func streamMultiCriteriaStats(
    targetRangeIndex: Int?,
    firstPairIndex: Int,
    pairEndIndex: Int,
    frame: inout EvalFrame
  ) -> Result<CriteriaStreamStats, ErrorCode> {
    guard pairEndIndex >= firstPairIndex + 2,
      (pairEndIndex - firstPairIndex) % 2 == 0
    else {
      return .failure(ErrorCode(code: "#VALUE!"))
    }
    let targetSpec: CriteriaRangeSpec?
    if let targetIdx = targetRangeIndex {
      switch resolveCriteriaRangeSpec(at: targetIdx, frame: &frame) {
      case .failure(let err): return .failure(err)
      case .success(let spec): targetSpec = spec
      }
    } else {
      targetSpec = nil
    }

    var pairSpecs: [(CriteriaRangeSpec, EvaluatedCellValue)] = []
    pairSpecs.reserveCapacity((pairEndIndex - firstPairIndex) / 2)
    for idx in stride(from: firstPairIndex, to: pairEndIndex, by: 2) {
      let critVal = frame.arg(idx + 1, in: self)
      if case .error(let code) = critVal { return .failure(ErrorCode(code: code)) }
      switch resolveCriteriaRangeSpec(at: idx, frame: &frame) {
      case .failure(let err): return .failure(err)
      case .success(let spec):
        let expectedLen = targetSpec?.totalCells ?? pairSpecs.first?.0.totalCells ?? spec.totalCells
        guard spec.totalCells == expectedLen else {
          return .failure(ErrorCode(code: "#VALUE!"))
        }
        pairSpecs.append((spec, critVal))
      }
    }

    let totalCells = targetSpec?.totalCells ?? pairSpecs.first?.0.totalCells ?? 0
    var stats = CriteriaStreamStats()
    for offset in 0..<totalCells {
      var allMatched = true
      for (rangeSpec, critVal) in pairSpecs {
        let coord = rangeSpec.coordinate(at: offset)
        let cellVal =
          rangeSpec.sheetName.map {
            evaluateOnSheet(named: $0, at: coord, context: &frame.context, depth: frame.depth + 1)
          } ?? evaluateInternal(at: coord, context: &frame.context, depth: frame.depth + 1)
        if !Self.matchesCriteriaValue(cellVal, criteriaValue: critVal) {
          allMatched = false
          break
        }
      }
      guard allMatched else { continue }
      stats.matchCount += 1
      if let tSpec = targetSpec {
        let tCoord = tSpec.coordinate(at: offset)
        let tVal =
          tSpec.sheetName.map {
            evaluateOnSheet(named: $0, at: tCoord, context: &frame.context, depth: frame.depth + 1)
          } ?? evaluateInternal(at: tCoord, context: &frame.context, depth: frame.depth + 1)
        if case .error(let code) = tVal {
          return .failure(ErrorCode(code: code))
        }
        if let num = tVal.numericValue {
          stats.numericCount += 1
          stats.sum += num
          stats.minVal = stats.minVal.map { min($0, num) } ?? num
          stats.maxVal = stats.maxVal.map { max($0, num) } ?? num
        }
      }
    }
    return .success(stats)
  }

  // MARK: - 2. Statistical Functions

  private func evaluateStatisticalFunction(name: String, frame: inout EvalFrame)
    -> EvaluatedCellValue?
  {
    switch name {
    case "MEDIAN":
      switch frame.resolve(frame.indices, in: self) {
      case .failure(let err): return .error(err)
      case .success(let vals):
        let sorted = vals.compactMap(\.numericValue).sorted()
        guard !sorted.isEmpty else { return .error("#NUM!") }
        let mid = sorted.count / 2
        return .number(sorted.count % 2 == 1 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2.0)
      }
    case "MODE":
      switch frame.resolve(frame.indices, in: self) {
      case .failure(let err): return .error(err)
      case .success(let vals):
        let nums = vals.compactMap(\.numericValue)
        guard !nums.isEmpty else { return .error("#N/A") }
        var counts: [Double: Int] = [:]
        for n in nums { counts[n, default: 0] += 1 }
        guard let best = nums.max(by: { counts[$0, default: 0] < counts[$1, default: 0] }),
          (counts[best] ?? 0) > 1
        else { return .error("#N/A") }
        return .number(best)
      }
    case "VAR", "STDEV", "VARP", "STDEVP":
      var count = 0
      var mean = 0.0
      var m2 = 0.0
      if let err = frame.forEachResolvedCell(
        in: frame.indices, sheet: self,
        { v in
          if let x = v.numericValue {
            count += 1
            let delta = x - mean
            mean += delta / Double(count)
            let delta2 = x - mean
            m2 += delta * delta2
          }
        })
      {
        return .error(err)
      }
      let isSample = (name == "VAR" || name == "STDEV")
      guard count >= (isSample ? 2 : 1) else { return .error("#DIV/0!") }
      let variance = m2 / Double(isSample ? (count - 1) : count)
      return .number((name == "VAR" || name == "VARP") ? variance : sqrt(variance))
    case "AVEDEV":
      var count = 0
      var sum = 0.0
      if let err = frame.forEachResolvedCell(
        in: frame.indices, sheet: self,
        { v in
          if let x = v.numericValue {
            count += 1
            sum += x
          }
        })
      {
        return .error(err)
      }
      guard count > 0 else { return .error("#DIV/0!") }
      let mean = sum / Double(count)
      var devSum = 0.0
      _ = frame.forEachResolvedCell(in: frame.indices, sheet: self) { v in
        if let x = v.numericValue {
          devSum += abs(x - mean)
        }
      }
      return .number(devSum / Double(count))
    case "GEOMEAN":
      var count = 0
      var logSum = 0.0
      var hasInvalid = false
      if let err = frame.forEachResolvedCell(
        in: frame.indices, sheet: self,
        { v in
          if let x = v.numericValue {
            if x <= 0.0 {
              hasInvalid = true
            } else {
              count += 1
              logSum += log(x)
            }
          }
        })
      {
        return .error(err)
      }
      guard count > 0, !hasInvalid else { return .error("#VALUE!") }
      let result = exp(logSum / Double(count))
      return result.isFinite ? .number(result) : .error("#VALUE!")
    case "HARMEAN":
      var count = 0
      var reciprocalSum = 0.0
      var hasInvalid = false
      if let err = frame.forEachResolvedCell(
        in: frame.indices, sheet: self,
        { v in
          if let x = v.numericValue {
            if x <= 0.0 {
              hasInvalid = true
            } else {
              count += 1
              reciprocalSum += 1.0 / x
            }
          }
        })
      {
        return .error(err)
      }
      guard count > 0, !hasInvalid, reciprocalSum > 0.0 else {
        return .error("#VALUE!")
      }
      return .number(Double(count) / reciprocalSum)
    case "PERCENTILE":
      guard frame.count == 2 else { return .error("#VALUE!") }
      switch frame.resolve(0..<1, in: self) {
      case .failure(let err):
        return .error(err)
      case .success(let vals):
        guard let k = frame.arg(1, in: self).numericValue, k >= 0.0, k <= 1.0 else {
          return .error("#VALUE!")
        }
        let sorted = vals.compactMap(\.numericValue).sorted()
        guard !sorted.isEmpty else { return .error("#VALUE!") }
        let pos = k * Double(sorted.count - 1)
        guard let lower = Self.safeInt(floor(pos)), let upper = Self.safeInt(ceil(pos)) else {
          return .error("#NUM!")
        }
        let interpolated = sorted[lower] + (pos - Double(lower)) * (sorted[upper] - sorted[lower])
        return .number(interpolated)
      }
    case "QUARTILE":
      guard frame.count == 2 else { return .error("#VALUE!") }
      switch frame.resolve(0..<1, in: self) {
      case .failure(let err):
        return .error(err)
      case .success(let vals):
        guard let qNum = frame.arg(1, in: self).numericValue,
          let quart = Self.safeInt(floor(qNum)),
          (0...4).contains(quart)
        else {
          return .error("#VALUE!")
        }
        let sorted = vals.compactMap(\.numericValue).sorted()
        guard !sorted.isEmpty else { return .error("#VALUE!") }
        let k = Double(quart) * 0.25
        let pos = k * Double(sorted.count - 1)
        guard let lower = Self.safeInt(floor(pos)), let upper = Self.safeInt(ceil(pos)) else {
          return .error("#NUM!")
        }
        let interpolated = sorted[lower] + (pos - Double(lower)) * (sorted[upper] - sorted[lower])
        return .number(interpolated)
      }
    case "CORREL", "SLOPE", "INTERCEPT":
      guard frame.count == 2 else { return .error("#VALUE!") }
      switch (frame.resolve(0..<1, in: self), frame.resolve(1..<2, in: self)) {
      case (let .failure(err), _), (_, let .failure(err)):
        return .error(err)
      case (.success(let valsY), .success(let valsX)):
        return Self.computeLinearRegressionMetric(
          name: name,
          valsY: valsY,
          valsX: valsX,
          forecastX: nil
        )
      }
    case "FORECAST":
      guard frame.count == 3, let xVal = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      switch (frame.resolve(1..<2, in: self), frame.resolve(2..<3, in: self)) {
      case (let .failure(err), _), (_, let .failure(err)):
        return .error(err)
      case (.success(let valsY), .success(let valsX)):
        return Self.computeLinearRegressionMetric(
          name: "FORECAST",
          valsY: valsY,
          valsX: valsX,
          forecastX: xVal
        )
      }
    case "LARGE", "SMALL":
      guard frame.count == 2 else { return .error("#VALUE!") }
      switch frame.resolve(0..<1, in: self) {
      case .failure(let err): return .error(err)
      case .success(let vals):
        guard let kVal = frame.arg(1, in: self).numericValue, let k = Self.safeInt(kVal) else {
          return .error("#VALUE!")
        }
        let sorted = vals.compactMap(\.numericValue).sorted(by: name == "LARGE" ? (>) : (<))
        guard k >= 1, k <= sorted.count else { return .error("#NUM!") }
        return .number(sorted[k - 1])
      }
    case "RANK":
      guard (2...3).contains(frame.count), let target = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      switch frame.resolve(1..<2, in: self) {
      case .failure(let err): return .error(err)
      case .success(let vals):
        let order =
          frame.count == 3 ? (Self.safeInt(frame.arg(2, in: self).numericValue ?? 0.0) ?? 0) : 0
        let nums = vals.compactMap(\.numericValue)
        guard nums.contains(target) else { return .error("#N/A") }
        let rank =
          order == 0 ? 1 + nums.filter { $0 > target }.count : 1 + nums.filter { $0 < target }.count
        return .number(Double(rank))
      }
    default:
      return nil
    }
  }

  private static func computeLinearRegressionMetric(
    name: String,
    valsY: [EvaluatedCellValue],
    valsX: [EvaluatedCellValue],
    forecastX: Double?
  ) -> EvaluatedCellValue {
    let yNums = valsY.compactMap(\.numericValue)
    let xNums = valsX.compactMap(\.numericValue)
    guard yNums.count == xNums.count, yNums.count >= 2 else {
      return .error("#DIV/0!")
    }
    let n = Double(yNums.count)
    let meanX = xNums.reduce(0.0, +) / n
    let meanY = yNums.reduce(0.0, +) / n
    var sumXY = 0.0
    var sumX2 = 0.0
    var sumY2 = 0.0
    for i in 0..<yNums.count {
      let dx = xNums[i] - meanX
      let dy = yNums[i] - meanY
      sumXY += dx * dy
      sumX2 += dx * dx
      sumY2 += dy * dy
    }
    if name == "CORREL" {
      let denom = sqrt(sumX2 * sumY2)
      guard denom > 0.0 else { return .error("#DIV/0!") }
      return .number(sumXY / denom)
    }
    guard sumX2 > 0.0 else { return .error("#DIV/0!") }
    let slope = sumXY / sumX2
    let intercept = meanY - slope * meanX
    if name == "SLOPE" {
      return .number(slope)
    }
    if name == "INTERCEPT" {
      return .number(intercept)
    }
    if let fx = forecastX {
      return .number(intercept + slope * fx)
    }
    return .error("#VALUE!")
  }

  // MARK: - 3. Logical Functions

  private func evaluateLogicalFunction(name: String, frame: inout EvalFrame) -> EvaluatedCellValue?
  {
    switch name {
    case "ISBLANK":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let val = frame.rawCellOrArg(0, in: self)
      return .boolean(val == .empty)
    case "TRUE": return .boolean(true)
    case "FALSE": return .boolean(false)
    case "IF":
      guard (2...3).contains(frame.count) else { return .error("#VALUE!") }
      let condVal = frame.arg(0, in: self)
      if case .error = condVal { return condVal }
      if condVal.booleanValue == true { return frame.arg(1, in: self) }
      return frame.count == 3 ? frame.arg(2, in: self) : .boolean(false)
    case "IFS":
      guard frame.count >= 2, frame.count % 2 == 0 else { return .error("#VALUE!") }
      for i in stride(from: 0, to: frame.count, by: 2) {
        let condVal = frame.arg(i, in: self)
        if case .error = condVal { return condVal }
        if condVal.booleanValue == true { return frame.arg(i + 1, in: self) }
      }
      return .error("#N/A")
    case "AND", "OR", "XOR":
      var count = 0
      var allTrue = true
      var anyTrue = false
      var trueCount = 0
      if let err = frame.forEachResolvedCell(
        in: frame.indices, sheet: self,
        { v in
          count += 1
          if v.booleanValue == true {
            anyTrue = true
            trueCount += 1
          } else {
            allTrue = false
          }
        })
      {
        return .error(err)
      }
      guard count > 0 else { return .error("#VALUE!") }
      if name == "AND" { return .boolean(allTrue) }
      if name == "OR" { return .boolean(anyTrue) }
      return .boolean(trueCount % 2 == 1)
    case "NOT":
      guard frame.count == 1 else { return .error("#VALUE!") }
      let v = frame.arg(0, in: self)
      if case .error = v { return v }
      return .boolean(!(v.booleanValue ?? false))
    case "IFERROR":
      guard frame.count == 2 else { return .error("#VALUE!") }
      let primary = frame.arg(0, in: self)
      if case .error = primary { return frame.arg(1, in: self) }
      return primary
    case "IFNA":
      guard frame.count == 2 else { return .error("#VALUE!") }
      let primary = frame.arg(0, in: self)
      return primary == .error("#N/A") ? frame.arg(1, in: self) : primary
    case "SWITCH":
      guard frame.count >= 3 else { return .error("#VALUE!") }
      let target = frame.arg(0, in: self)
      if case .error = target { return target }
      let pairLimit = frame.count - (frame.count % 2 == 0 ? 1 : 0)
      var idx = 1
      while idx + 1 <= pairLimit {
        let candidate = frame.arg(idx, in: self)
        if case .error = candidate { return candidate }
        if candidate == target
          || candidate.description.caseInsensitiveCompare(target.description) == .orderedSame
        {
          return frame.arg(idx + 1, in: self)
        }
        idx += 2
      }
      return frame.count % 2 == 0 ? frame.arg(frame.count - 1, in: self) : .error("#N/A")
    default:
      return nil
    }
  }

  // MARK: - Numeric & Criteria Matching Helpers

  internal static func safeInt(_ value: Double) -> Int? {
    guard value.isFinite else { return nil }
    let t = trunc(value)
    guard t >= -9223372036854775808.0, t < 9223372036854775808.0 else { return nil }
    return Int(exactly: t)
  }

  internal static func cellValuesEqual(_ a: EvaluatedCellValue, _ b: EvaluatedCellValue) -> Bool {
    if let aNum = a.numericValue, let bNum = b.numericValue { return aNum == bNum }
    return a.description.caseInsensitiveCompare(b.description) == .orderedSame
  }

  @inline(__always)
  private static func foldScalar(_ s: UnicodeScalar) -> UInt32 {
    let v = s.value
    if v >= 0x41 && v <= 0x5A { return v + 32 }
    return v
  }

  internal static func matchesWildcardPattern(text: String, pattern: String) -> Bool {
    let tScalars = text.unicodeScalars
    let pScalars = pattern.unicodeScalars
    var tIdx = tScalars.startIndex
    var pIdx = pScalars.startIndex
    var starPatIdx: String.UnicodeScalarView.Index? = nil
    var matchTextIdx = tScalars.startIndex

    while tIdx < tScalars.endIndex {
      if pIdx < pScalars.endIndex {
        let pVal = pScalars[pIdx].value
        if pVal == 0x7E {
          let nextP = pScalars.index(after: pIdx)
          if nextP < pScalars.endIndex {
            if foldScalar(tScalars[tIdx]) == foldScalar(pScalars[nextP]) {
              tIdx = tScalars.index(after: tIdx)
              pIdx = pScalars.index(after: nextP)
              continue
            }
          } else if foldScalar(tScalars[tIdx]) == 0x7E {
            tIdx = tScalars.index(after: tIdx)
            pIdx = nextP
            continue
          }
        } else if pVal == 0x2A {
          starPatIdx = pIdx
          matchTextIdx = tIdx
          pIdx = pScalars.index(after: pIdx)
          continue
        } else if pVal == 0x3F || foldScalar(pScalars[pIdx]) == foldScalar(tScalars[tIdx]) {
          tIdx = tScalars.index(after: tIdx)
          pIdx = pScalars.index(after: pIdx)
          continue
        }
      }

      if let sIdx = starPatIdx {
        pIdx = pScalars.index(after: sIdx)
        matchTextIdx = tScalars.index(after: matchTextIdx)
        tIdx = matchTextIdx
      } else {
        return false
      }
    }

    while pIdx < pScalars.endIndex && pScalars[pIdx].value == 0x2A {
      pIdx = pScalars.index(after: pIdx)
    }
    return pIdx == pScalars.endIndex
  }

  internal static func matchesCriteriaValue(
    _ value: EvaluatedCellValue,
    criteriaValue critVal: EvaluatedCellValue
  ) -> Bool {
    let critStr = critVal.description.trimmingCharacters(in: .whitespaces)
    for op in [">=", "<=", "<>", "=", ">", "<"] where critStr.hasPrefix(op) {
      let operand = String(critStr.dropFirst(op.count)).trimmingCharacters(in: .whitespaces)
      if let targetNum = Double(operand) {
        guard let cellNum = value.numericValue else { return op == "<>" }
        switch op {
        case ">=": return cellNum >= targetNum
        case "<=": return cellNum <= targetNum
        case "<>": return cellNum != targetNum
        case "=": return cellNum == targetNum
        case ">": return cellNum > targetNum
        case "<": return cellNum < targetNum
        default: return false
        }
      } else {
        if (op == "=" || op == "<>")
          && (operand.contains("*") || operand.contains("?") || operand.contains("~"))
        {
          let matched = matchesWildcardPattern(text: value.description, pattern: operand)
          return op == "=" ? matched : !matched
        }
        let cmp = value.description.caseInsensitiveCompare(operand)
        return op == "=" ? (cmp == .orderedSame) : (op == "<>" ? (cmp != .orderedSame) : false)
      }
    }
    if critStr.contains("*") || critStr.contains("?") || critStr.contains("~") {
      return matchesWildcardPattern(text: value.description, pattern: critStr)
    }
    if let critNum = critVal.numericValue, let cellNum = value.numericValue {
      return cellNum == critNum
    }
    return value.description.caseInsensitiveCompare(critStr) == .orderedSame
  }
}
