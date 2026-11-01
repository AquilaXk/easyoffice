import Foundation

extension WorksheetModel {


  internal struct CriteriaStreamStats {
    var matchCount: Int = 0
    var numericCount: Int = 0
    var sum: Double = 0.0
    var minVal: Double? = nil
    var maxVal: Double? = nil
  }

  internal struct CriteriaRangeSpec {
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

    @inline(__always)
    func offset(for coord: PackedCellCoordinate) -> Int? {
      let r = coord.row - minRow
      let c = coord.column - minCol
      guard r >= 0 && r < rowCount && c >= 0 && c < colCount else { return nil }
      return r * colCount + c
    }
  }

  private func resolveCriteriaRangeSpec(
    at argIndex: Int,
    frame: inout EvalFrame
  ) -> Result<CriteriaRangeSpec, ErrorCode> {
    switch resolveRangeArgument(from: frame.args[argIndex], frame: &frame) {
    case .failure(let err):
      return .failure(ErrorCode(code: err))
    case .success(let sheetName, let resolvedRange):
      guard let bounds = rangeBounds(resolvedRange, allowWholeColumnOrRow: true) else {
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
  }

  internal func streamMultiCriteriaStats(
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
    var targetSpec: CriteriaRangeSpec?
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
        if firstPairIndex == 0, let tSpec = targetSpec,
          tSpec.rowCount == 1 && tSpec.colCount == 1
            && (spec.rowCount != 1 || spec.colCount != 1)
        {
          guard tSpec.minRow + spec.rowCount <= CellAddress.maxZeroBasedRow + 1,
            tSpec.minCol + spec.colCount <= CellAddress.maxZeroBasedColumn + 1
          else {
            return .failure(ErrorCode(code: "#REF!"))
          }
          targetSpec = CriteriaRangeSpec(
            sheetName: tSpec.sheetName,
            minRow: tSpec.minRow,
            minCol: tSpec.minCol,
            rowCount: spec.rowCount,
            colCount: spec.colCount
          )
        }
        let expectedRows = targetSpec?.rowCount ?? pairSpecs.first?.0.rowCount ?? spec.rowCount
        let expectedCols = targetSpec?.colCount ?? pairSpecs.first?.0.colCount ?? spec.colCount
        guard spec.rowCount == expectedRows && spec.colCount == expectedCols else {
          return .failure(ErrorCode(code: "#VALUE!"))
        }
        pairSpecs.append((spec, critVal))
      }
    }

    let totalCells = targetSpec?.totalCells ?? pairSpecs.first?.0.totalCells ?? 0
    var stats = CriteriaStreamStats()

    if totalCells > 4096, let firstPair = pairSpecs.first,
      !Self.matchesCriteriaValue(.empty, criteriaValue: firstPair.1)
    {
      let candidateCoords = populatedCoordinates(
        onSheet: firstPair.0.sheetName,
        minRow: firstPair.0.minRow,
        maxRow: firstPair.0.minRow + firstPair.0.rowCount - 1,
        minCol: firstPair.0.minCol,
        maxCol: firstPair.0.minCol + firstPair.0.colCount - 1
      )
      for candidateCoord in candidateCoords {
        guard let offset = firstPair.0.offset(for: candidateCoord) else { continue }
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
          if case .number(let num) = tVal {
            stats.numericCount += 1
            stats.sum += num
            stats.minVal = stats.minVal.map { min($0, num) } ?? num
            stats.maxVal = stats.maxVal.map { max($0, num) } ?? num
          }
        }
      }
      return .success(stats)
    }

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
        if case .number(let num) = tVal {
          stats.numericCount += 1
          stats.sum += num
          stats.minVal = stats.minVal.map { min($0, num) } ?? num
          stats.maxVal = stats.maxVal.map { max($0, num) } ?? num
        }
      }
    }
    return .success(stats)
  }


  internal func evaluateStatisticalFunction(name: String, frame: inout EvalFrame)
    -> EvaluatedCellValue?
  {
    switch name {
    case "MEDIAN":
      switch frame.resolveNumeric(frame.indices, in: self) {
      case .failure(let err): return .error(err)
      case .success(let nums):
        let sorted = nums.sorted()
        guard !sorted.isEmpty else { return .error("#NUM!") }
        let mid = sorted.count / 2
        return .number(sorted.count % 2 == 1 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2.0)
      }
    case "MODE":
      switch frame.resolveNumeric(frame.indices, in: self) {
      case .failure(let err): return .error(err)
      case .success(let nums):
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
      if let err = frame.forEachResolvedCellWithOrigin(
        in: frame.indices, sheet: self,
        { v, isFromReference in
          let num: Double? = isFromReference ? { if case .number(let n) = v { return n }; return nil }() : v.numericValue
          if let x = num {
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
      switch frame.resolveNumeric(frame.indices, in: self) {
      case .failure(let err):
        return .error(err)
      case .success(let nums):
        guard !nums.isEmpty else { return .error("#DIV/0!") }
        let mean = nums.reduce(0.0, +) / Double(nums.count)
        let devSum = nums.reduce(0.0) { $0 + abs($1 - mean) }
        return .number(devSum / Double(nums.count))
      }
    case "GEOMEAN":
      var count = 0
      var logSum = 0.0
      var hasInvalid = false
      if let err = frame.forEachResolvedCellWithOrigin(
        in: frame.indices, sheet: self,
        { v, isFromReference in
          let num: Double? = isFromReference ? { if case .number(let n) = v { return n }; return nil }() : v.numericValue
          if let x = num {
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
      if let err = frame.forEachResolvedCellWithOrigin(
        in: frame.indices, sheet: self,
        { v, isFromReference in
          let num: Double? = isFromReference ? { if case .number(let n) = v { return n }; return nil }() : v.numericValue
          if let x = num {
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
      switch frame.resolveNumeric(0..<1, in: self) {
      case .failure(let err):
        return .error(err)
      case .success(let nums):
        guard let k = frame.arg(1, in: self).numericValue, k >= 0.0, k <= 1.0 else {
          return .error("#VALUE!")
        }
        let sorted = nums.sorted()
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
      switch frame.resolveNumeric(0..<1, in: self) {
      case .failure(let err):
        return .error(err)
      case .success(let nums):
        guard let qNum = frame.arg(1, in: self).numericValue,
          let quart = Self.safeInt(floor(qNum)),
          (0...4).contains(quart)
        else {
          return .error("#VALUE!")
        }
        let sorted = nums.sorted()
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
      switch frame.resolveNumeric(0..<1, in: self) {
      case .failure(let err): return .error(err)
      case .success(let nums):
        guard let kVal = frame.arg(1, in: self).numericValue, let k = Self.safeInt(kVal) else {
          return .error("#VALUE!")
        }
        let sorted = nums.sorted(by: name == "LARGE" ? (>) : (<))
        guard k >= 1, k <= sorted.count else { return .error("#NUM!") }
        return .number(sorted[k - 1])
      }
    case "RANK":
      guard (2...3).contains(frame.count), let target = frame.arg(0, in: self).numericValue else {
        return .error("#VALUE!")
      }
      switch frame.resolveNumeric(1..<2, in: self) {
      case .failure(let err): return .error(err)
      case .success(let nums):
        let order =
          frame.count == 3 ? (Self.safeInt(frame.arg(2, in: self).numericValue ?? 0.0) ?? 0) : 0
        guard nums.contains(target) else { return .error("#N/A") }
        let rank =
          order == 0 ? 1 + nums.filter { $0 > target }.count : 1 + nums.filter { $0 < target }.count
        return .number(Double(rank))
      }
    case "AVERAGEA", "MINA", "MAXA":
      var nums: [Double] = []
      if let err = frame.forEachResolvedCellWithOrigin(
        in: frame.indices,
        sheet: self,
        { v, isFromReference in
          if v == .empty { return }
          if isFromReference {
            switch v {
            case .number(let n): nums.append(n)
            case .boolean(let b): nums.append(b ? 1.0 : 0.0)
            case .text: nums.append(0.0)
            default: break
            }
          } else if let n = v.numericValue {
            nums.append(n)
          }
        }
      ) {
        return .error(err)
      }
      if name == "AVERAGEA" {
        guard !nums.isEmpty else { return .error("#DIV/0!") }
        return .number(nums.reduce(0.0, +) / Double(nums.count))
      }
      if name == "MINA" {
        return .number(nums.min() ?? 0.0)
      }
      return .number(nums.max() ?? 0.0)
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


  internal func evaluateLogicalFunction(name: String, frame: inout EvalFrame) -> EvaluatedCellValue?
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
      var invalidScalar = false
      if let err = frame.forEachResolvedCellWithOrigin(
        in: frame.indices, sheet: self,
        { v, isFromReference in
          if v == .empty { return }
          if isFromReference {
            switch v {
            case .boolean(let b):
              count += 1
              if b {
                anyTrue = true
                trueCount += 1
              } else {
                allTrue = false
              }
            case .number(let n):
              count += 1
              if n != 0.0 {
                anyTrue = true
                trueCount += 1
              } else {
                allTrue = false
              }
            default:
              return
            }
          } else {
            guard let b = v.booleanValue else {
              invalidScalar = true
              return
            }
            count += 1
            if b {
              anyTrue = true
              trueCount += 1
            } else {
              allTrue = false
            }
          }
        })
      {
        return .error(err)
      }
      if invalidScalar { return .error("#VALUE!") }
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


  internal static func cellValuesEqual(_ a: EvaluatedCellValue, _ b: EvaluatedCellValue) -> Bool {
    if let aNum = a.numericValue, let bNum = b.numericValue { return aNum == bNum }
    return a.description.caseInsensitiveCompare(b.description) == .orderedSame
  }

  @inline(__always)
  private static func foldScalar(_ s: UnicodeScalar) -> UInt32 {
    let v = s.value
    if v >= 0x41 && v <= 0x5A { return v + 32 }
    if v >= 0x80 {
      for lower in String(s).lowercased().unicodeScalars {
        return lower.value
      }
    }
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

  private static func criteriaNumericCandidate(_ value: EvaluatedCellValue) -> Double? {
    switch value {
    case .number(let n): return n
    case .text(let s): return SheetValueCoercion.parseStrictNumber(s)
    default: return nil
    }
  }

  internal static func matchesCriteriaValue(
    _ value: EvaluatedCellValue,
    criteriaValue critVal: EvaluatedCellValue
  ) -> Bool {
    if case .number(let critNum) = critVal {
      guard let cellNum = criteriaNumericCandidate(value) else { return false }
      return cellNum == critNum
    }
    if case .boolean(let critBool) = critVal {
      if case .boolean(let cellBool) = value { return cellBool == critBool }
      return false
    }
    let critStr = critVal.description.trimmingCharacters(in: .whitespaces)
    for op in [">=", "<=", "<>", "=", ">", "<"] where critStr.hasPrefix(op) {
      let operand = String(critStr.dropFirst(op.count)).trimmingCharacters(in: .whitespaces)
      if let targetNum = SheetValueCoercion.parseStrictNumber(operand) {
        guard let cellNum = criteriaNumericCandidate(value) else { return op == "<>" }
        switch op {
        case ">=": return cellNum >= targetNum
        case "<=": return cellNum <= targetNum
        case "<>": return cellNum != targetNum
        case "=": return cellNum == targetNum
        case ">": return cellNum > targetNum
        case "<": return cellNum < targetNum
        default: return false
        }
      } else if let targetDate = Self.parseDateValue(.text(operand)) {
        guard let cellDate = Self.parseDateValue(value) else { return op == "<>" }
        switch op {
        case ">=": return cellDate >= targetDate
        case "<=": return cellDate <= targetDate
        case "<>": return cellDate != targetDate
        case "=": return cellDate == targetDate
        case ">": return cellDate > targetDate
        case "<": return cellDate < targetDate
        default: return false
        }
      } else {
        if (op == "=" || op == "<>")
          && (operand.contains("*") || operand.contains("?") || operand.contains("~"))
        {
          let matched: Bool
          if case .text(let cellText) = value {
            matched = matchesWildcardPattern(text: cellText, pattern: operand)
          } else {
            matched = false
          }
          return op == "=" ? matched : !matched
        }
        if operand.isEmpty {
          let isBlank = (value == .empty || value == .text(""))
          switch op {
          case "=", ">=", "<=": return isBlank
          case "<>": return !isBlank
          default: return false
          }
        }
        if op == "=" || op == "<>" {
          let cmp = value.description.caseInsensitiveCompare(operand)
          return op == "=" ? (cmp == .orderedSame) : (cmp != .orderedSame)
        }
        guard case .text(let cellStr) = value, !cellStr.isEmpty else {
          return false
        }
        let cmp = cellStr.caseInsensitiveCompare(operand)
        switch op {
        case ">": return cmp == .orderedDescending
        case "<": return cmp == .orderedAscending
        case ">=": return cmp == .orderedSame || cmp == .orderedDescending
        case "<=": return cmp == .orderedSame || cmp == .orderedAscending
        default: return false
        }
      }
    }
    if critStr.contains("*") || critStr.contains("?") || critStr.contains("~") {
      guard case .text(let cellText) = value else { return false }
      return matchesWildcardPattern(text: cellText, pattern: critStr)
    }
    if let critNum = SheetValueCoercion.parseStrictNumber(critStr),
      let cellNum = criteriaNumericCandidate(value)
    {
      return cellNum == critNum
    }
    if let critDate = Self.parseDateValue(critVal), let cellDate = Self.parseDateValue(value) {
      return cellDate == critDate
    }
    return value.description.caseInsensitiveCompare(critStr) == .orderedSame
  }

  internal static func astContainsSubtotal(_ ast: FormulaAST) -> Bool {
    switch ast {
    case .functionCall(let fnName, let fnArgs):
      if fnName == "SUBTOTAL" { return true }
      return fnArgs.contains(where: astContainsSubtotal)
    case .unaryMinus(let inner):
      return astContainsSubtotal(inner)
    case .binaryOp(_, let lhs, let rhs), .comparisonOp(_, let lhs, let rhs):
      return astContainsSubtotal(lhs) || astContainsSubtotal(rhs)
    default:
      return false
    }
  }

  internal func cellContainsSubtotal(onSheet sheetName: String?, at coord: PackedCellCoordinate)
    -> Bool
  {
    if let sheetName {
      if sheetName.caseInsensitiveCompare(name) == .orderedSame {
        if let ast = compiledASTs[coord], Self.astContainsSubtotal(ast) { return true }
        if let raw = cellsByCoordinate[coord], raw.hasPrefix("="),
          raw.uppercased().contains("SUBTOTAL(")
        {
          return true
        }
        return false
      }
      guard
        let page = additionalSheets.first(where: {
          $0.name.caseInsensitiveCompare(sheetName) == .orderedSame
        })
      else {
        return false
      }
      if let raw = page.cellsByCoordinate[coord], raw.hasPrefix("="),
        raw.uppercased().contains("SUBTOTAL(")
      {
        return true
      }
      return false
    }
    if let ast = compiledASTs[coord], Self.astContainsSubtotal(ast) { return true }
    if let raw = cellsByCoordinate[coord], raw.hasPrefix("="),
      raw.uppercased().contains("SUBTOTAL(")
    {
      return true
    }
    return false
  }

  internal enum SubtotalFunction: Int {
    case average = 1
    case count = 2
    case countA = 3
    case max = 4
    case min = 5
    case product = 6
    case stdevSample = 7
    case stdevPopulation = 8
    case sum = 9
    case varSample = 10
    case varPopulation = 11
  }

  internal func evaluateSubtotalFunction(frame: inout EvalFrame) -> EvaluatedCellValue {
    guard frame.count >= 2 else { return .error("#VALUE!") }
    let firstArg = frame.arg(0, in: self)
    if case .error = firstArg { return firstArg }
    guard let fnNumDouble = firstArg.numericValue,
      let rawFnNum = Self.safeInt(fnNumDouble)
    else {
      return .error("#VALUE!")
    }
    let normalizedCode = rawFnNum > 100 ? rawFnNum - 100 : rawFnNum
    guard let subtotalFn = SubtotalFunction(rawValue: normalizedCode) else {
      return .error("#VALUE!")
    }

    var nums: [Double] = []
    var numberOnlyCount = 0
    var nonEmptyCount = 0

    for idx in 1..<frame.count {
      switch resolveRangeArgument(from: frame.args[idx], frame: &frame) {
      case .success(let sheetName, let cellRange):
        guard let bounds = rangeBounds(cellRange, allowWholeColumnOrRow: true) else {
          return .error("#REF!")
        }
        let total = bounds.rowCount * bounds.colCount
        let coords: [PackedCellCoordinate]
        if total > 4096 {
          coords = populatedCoordinates(
            onSheet: sheetName,
            minRow: bounds.minRow,
            maxRow: bounds.maxRow,
            minCol: bounds.minCol,
            maxCol: bounds.maxCol
          )
        } else {
          var list: [PackedCellCoordinate] = []
          list.reserveCapacity(total)
          for r in bounds.minRow...bounds.maxRow {
            for c in bounds.minCol...bounds.maxCol {
              list.append(PackedCellCoordinate(row: r, column: c))
            }
          }
          coords = list
        }
        for coord in coords {
          if cellContainsSubtotal(onSheet: sheetName, at: coord) {
            continue
          }
          let v =
            sheetName.map {
              evaluateOnSheet(named: $0, at: coord, context: &frame.context, depth: frame.depth + 1)
            } ?? evaluateInternal(at: coord, context: &frame.context, depth: frame.depth + 1)
          if case .error(let code) = v { return .error(code) }
          if case .number(let n) = v {
            nums.append(n)
            numberOnlyCount += 1
          }
          if v != .empty {
            nonEmptyCount += 1
          }
        }
      case .failure(let err):
        if err != "#VALUE!" {
          return .error(err)
        }
        if Self.astContainsSubtotal(frame.args[idx]) {
          continue
        }
        let v = frame.arg(idx, in: self)
        if case .error = v { return v }
        if let n = v.numericValue {
          nums.append(n)
          if case .number = v { numberOnlyCount += 1 }
        }
        if v != .empty {
          nonEmptyCount += 1
        }
      }
    }

    switch subtotalFn {
    case .average:
      guard !nums.isEmpty else { return .error("#DIV/0!") }
      return .number(nums.reduce(0.0, +) / Double(nums.count))
    case .count:
      return .number(Double(numberOnlyCount))
    case .countA:
      return .number(Double(nonEmptyCount))
    case .max:
      return .number(nums.max() ?? 0.0)
    case .min:
      return .number(nums.min() ?? 0.0)
    case .product:
      return .number(nums.isEmpty ? 0.0 : nums.reduce(1.0, *))
    case .stdevSample, .stdevPopulation, .varSample, .varPopulation:
      let isSample = (subtotalFn == .stdevSample || subtotalFn == .varSample)
      guard nums.count >= (isSample ? 2 : 1) else { return .error("#DIV/0!") }
      let mean = nums.reduce(0.0, +) / Double(nums.count)
      let m2 = nums.reduce(0.0) { $0 + ($1 - mean) * ($1 - mean) }
      let variance = m2 / Double(isSample ? (nums.count - 1) : nums.count)
      return .number(
        (subtotalFn == .varSample || subtotalFn == .varPopulation) ? variance : sqrt(variance))
    case .sum:
      return .number(nums.reduce(0.0, +))
    }
  }
}
