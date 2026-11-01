import Foundation

extension WorksheetModel {


  @inline(__always)
  private static func isTrueFormulaAST(_ ast: FormulaAST?) -> Bool {
    guard let ast else { return false }
    switch ast {
    case .empty, .literalNumber, .literalText, .literalBoolean:
      return false
    default:
      return true
    }
  }

  internal func preEvaluateDAGIterative(
    roots: [PackedCellCoordinate],
    context: inout EvaluationContext
  ) {
    let selfSheetUpper = name.uppercased()
    func isMemoized(_ coord: PackedCellCoordinate) -> Bool {
      if context.memo[coord] != nil { return true }
      if let crossVal = context.crossMemo[
        CrossSheetCellKey(sheetNameUpper: selfSheetUpper, coord: coord)]
      {
        context.memo[coord] = crossVal
        return true
      }
      return false
    }
    var dagState: [PackedCellCoordinate: UInt8] = [:]
    var stack: [(coord: PackedCellCoordinate, expanded: Bool)] = []
    stack.reserveCapacity(min(roots.count, 4096))
    for coord in roots.reversed() {
      let ast = compiledASTs[coord]
      if Self.isTrueFormulaAST(ast) && !isMemoized(coord) {
        stack.append((coord, false))
      }
    }
    var exprStack: [FormulaAST] = []
    while let (coord, expanded) = stack.popLast() {
      if expanded {
        dagState[coord] = 2
        if !isMemoized(coord) {
          _ = evaluateInternal(at: coord, context: &context, depth: 0)
        }
        continue
      }
      if isMemoized(coord) || dagState[coord] != nil {
        continue
      }
      dagState[coord] = 1
      guard let ast = compiledASTs[coord] ?? cellsByCoordinate[coord].map(Self.compileCellInput),
        Self.isTrueFormulaAST(ast)
      else {
        dagState[coord] = 2
        continue
      }
      stack.append((coord, true))
      exprStack.removeAll(keepingCapacity: true)
      exprStack.append(ast)
      while let ex = exprStack.popLast() {
        switch ex {
        case .cellRef(let dep):
          if Self.isTrueFormulaAST(compiledASTs[dep]) {
            if dagState[dep] == 1 {
              context.memo[coord] = .error("#CYCLE!")
            } else if !isMemoized(dep) && dagState[dep] == nil {
              stack.append((dep, false))
            }
          }
        case .rangeRef(let range):
          let minR = min(range.start.row, range.end.row)
          let maxR = max(range.start.row, range.end.row)
          let minC = min(range.start.column, range.end.column)
          let maxC = max(range.start.column, range.end.column)
          let span = (maxR - minR + 1) * (maxC - minC + 1)
          if span > 1024 {
            for dep in compiledASTs.keys where range.contains(dep) {
              if Self.isTrueFormulaAST(compiledASTs[dep]) {
                if dagState[dep] == 1 {
                  context.memo[coord] = .error("#CYCLE!")
                } else if !isMemoized(dep) && dagState[dep] == nil {
                  stack.append((dep, false))
                }
              }
            }
          } else {
            for r in minR...maxR {
              for c in minC...maxC {
                let dep = PackedCellCoordinate(row: r, column: c)
                if Self.isTrueFormulaAST(compiledASTs[dep]) {
                  if dagState[dep] == 1 {
                    context.memo[coord] = .error("#CYCLE!")
                  } else if !isMemoized(dep) && dagState[dep] == nil {
                    stack.append((dep, false))
                  }
                }
              }
            }
          }
        case .namedRangeRef(let name):
          if let notation = resolveNamedRange(name) {
            if let addr = try? CellAddress(a1Notation: notation) {
              exprStack.append(.cellRef(addr.packedCoordinate))
            } else if let range = try? CellRange(notation: notation) {
              exprStack.append(.rangeRef(range))
            }
          }
        case .unaryMinus(let inner):
          exprStack.append(inner)
        case .binaryOp(_, let lhs, let rhs), .comparisonOp(_, let lhs, let rhs):
          exprStack.append(rhs)
          exprStack.append(lhs)
        case .functionCall(_, let args):
          for a in args.reversed() {
            exprStack.append(a)
          }
        default:
          break
        }
      }
    }
  }

  public func evaluate(at a1: String) -> EvaluatedCellValue {
    guard let address = try? CellAddress(a1Notation: a1) else {
      return .error("#REF!")
    }
    return evaluate(at: address.packedCoordinate)
  }

  public func evaluate(at packed: PackedCellCoordinate) -> EvaluatedCellValue {
    guard packed.column >= 0, packed.column <= CellAddress.maxZeroBasedColumn,
      packed.row >= 0, packed.row <= CellAddress.maxZeroBasedRow
    else {
      return .error("#REF!")
    }
    var context = EvaluationContext()
    preEvaluateDAGIterative(roots: [packed], context: &context)
    return evaluateInternal(at: packed, context: &context, depth: 0)
  }

  public func evaluateGrid(
    rows: ClosedRange<Int>,
    columns: ClosedRange<Int>
  ) -> [PackedCellCoordinate: EvaluatedCellValue] {
    var context = EvaluationContext()
    var coordsToEval: [PackedCellCoordinate] = []
    coordsToEval.reserveCapacity(min(cellsByCoordinate.count, rows.count * columns.count))
    for r in rows where r >= 0 && r <= CellAddress.maxZeroBasedRow {
      for c in columns where c >= 0 && c <= CellAddress.maxZeroBasedColumn {
        let coord = PackedCellCoordinate(row: r, column: c)
        if cellsByCoordinate[coord] != nil {
          coordsToEval.append(coord)
        }
      }
    }
    preEvaluateDAGIterative(roots: coordsToEval, context: &context)
    var result: [PackedCellCoordinate: EvaluatedCellValue] = [:]
    result.reserveCapacity(coordsToEval.count)
    for coord in coordsToEval {
      result[coord] = evaluateInternal(at: coord, context: &context, depth: 0)
    }
    return result
  }

  public func sortedRows(
    in range: CellRange,
    byColumnOffset colOffset: Int,
    ascending: Bool = true,
    secondaryColumnOffset: Int? = nil,
    secondaryAscending: Bool = true
  ) -> [[EvaluatedCellValue]] {
    let rows = evaluateGrid(in: range)
    guard let firstRow = rows.first, colOffset >= 0, colOffset < firstRow.count else {
      return rows
    }
    return rows.sorted { lhs, rhs in
      let a = lhs[colOffset]
      let b = rhs[colOffset]
      if let aNum = a.numericValue, let bNum = b.numericValue {
        if aNum != bNum {
          return ascending ? (aNum < bNum) : (aNum > bNum)
        }
      } else if a.description != b.description {
        return ascending ? (a.description < b.description) : (a.description > b.description)
      }
      if let secCol = secondaryColumnOffset, secCol >= 0, secCol < firstRow.count {
        let sa = lhs[secCol]
        let sb = rhs[secCol]
        if let saNum = sa.numericValue, let sbNum = sb.numericValue {
          if saNum != sbNum {
            return secondaryAscending ? (saNum < sbNum) : (saNum > sbNum)
          }
        } else if sa.description != sb.description {
          return secondaryAscending
            ? (sa.description < sb.description)
            : (sa.description > sb.description)
        }
      }
      return false
    }
  }

  public func filteredRows(
    in range: CellRange,
    byColumnOffset colOffset: Int,
    predicate: (EvaluatedCellValue) -> Bool
  ) -> [[EvaluatedCellValue]] {
    let rows = evaluateGrid(in: range)
    guard let firstRow = rows.first, colOffset >= 0, colOffset < firstRow.count else {
      return []
    }
    return rows.filter { predicate($0[colOffset]) }
  }

  public func evaluateGrid(in range: CellRange) -> [[EvaluatedCellValue]] {
    var context = EvaluationContext()
    return evaluateGridInternal(in: range, context: &context, depth: 0)
  }

  internal func evaluateGridInternal(
    onSheet targetSheetName: String? = nil,
    in range: CellRange,
    context: inout EvaluationContext,
    depth: Int
  ) -> [[EvaluatedCellValue]] {
    guard let bounds = rangeBounds(range) else {
      return []
    }
    if targetSheetName == nil {
      var roots: [PackedCellCoordinate] = []
      for r in bounds.minRow...bounds.maxRow {
        for c in bounds.minCol...bounds.maxCol {
          let p = PackedCellCoordinate(row: r, column: c)
          if cellsByCoordinate[p] != nil {
            roots.append(p)
          }
        }
      }
      preEvaluateDAGIterative(roots: roots, context: &context)
    }

    var grid: [[EvaluatedCellValue]] = []
    grid.reserveCapacity(bounds.rowCount)
    for r in bounds.minRow...bounds.maxRow {
      var rowVals: [EvaluatedCellValue] = []
      rowVals.reserveCapacity(bounds.colCount)
      for c in bounds.minCol...bounds.maxCol {
        let packed = PackedCellCoordinate(row: r, column: c)
        if let targetSheetName {
          rowVals.append(
            evaluateOnSheet(named: targetSheetName, at: packed, context: &context, depth: depth + 1)
          )
        } else {
          rowVals.append(evaluateInternal(at: packed, context: &context, depth: depth + 1))
        }
      }
      grid.append(rowVals)
    }
    return grid
  }

  internal func hasSheet(named targetSheetName: String) -> Bool {
    if name.caseInsensitiveCompare(targetSheetName) == .orderedSame {
      return true
    }
    return additionalSheets.contains {
      $0.name.caseInsensitiveCompare(targetSheetName) == .orderedSame
    }
  }

  internal func evaluateOnSheet(
    named targetSheetName: String,
    at packed: PackedCellCoordinate,
    context: inout EvaluationContext,
    depth: Int
  ) -> EvaluatedCellValue {
    guard depth <= Self.maxFormulaRecursionDepth,
      packed.column >= 0, packed.column <= CellAddress.maxZeroBasedColumn,
      packed.row >= 0, packed.row <= CellAddress.maxZeroBasedRow
    else {
      return .error("#REF!")
    }
    if name.caseInsensitiveCompare(targetSheetName) == .orderedSame {
      return evaluateInternal(at: packed, context: &context, depth: depth)
    }
    guard
      let pageIdx = additionalSheets.firstIndex(where: {
        $0.name.caseInsensitiveCompare(targetSheetName) == .orderedSame
      })
    else {
      return .error("#REF!")
    }
    let targetPage = additionalSheets[pageIdx]
    let key = CrossSheetCellKey(sheetNameUpper: targetPage.name.uppercased(), coord: packed)

    let activeSheetUpper = name.uppercased()
    var newlyInsertedActiveKeys: [CrossSheetCellKey] = []
    for activeCoord in context.allVisitedCoordinates() {
      let activeKey = CrossSheetCellKey(sheetNameUpper: activeSheetUpper, coord: activeCoord)
      if context.crossVisited.insert(activeKey).inserted {
        newlyInsertedActiveKeys.append(activeKey)
      }
    }
    defer {
      for activeKey in newlyInsertedActiveKeys {
        context.crossVisited.remove(activeKey)
      }
    }
    if context.crossVisited.contains(key) {
      return .error("#CYCLE!")
    }
    if context.crossVisited.count > Self.maxFormulaRecursionDepth {
      return .error("#REF!")
    }
    if let cached = context.crossMemo[key] {
      return cached
    }
    guard let raw = targetPage.cellsByCoordinate[packed], !raw.isEmpty else {
      return .empty
    }
    let targetSheetUpper = targetPage.name.uppercased()
    let targetModel: WorksheetModel
    if let cachedModel = context.sheetModelCache[targetSheetUpper] {
      targetModel = cachedModel
    } else {
      var remainingSheets = additionalSheets
      remainingSheets.remove(at: pageIdx)
      remainingSheets.append(
        WorksheetSheetPage(
          id: id,
          name: name,
          cellsByCoordinate: cellsByCoordinate,
          cellStyles: cellStyles,
          sheetComments: sheetComments,
          mergedRanges: mergedRanges,
          columnWidths: columnWidths,
          rowHeights: rowHeights
        )
      )
      let builtModel = WorksheetModel(
        id: targetPage.id,
        name: targetPage.name,
        cellsByCoordinate: targetPage.cellsByCoordinate,
        cellStyles: targetPage.cellStyles,
        pageSetup: pageSetup,
        sheetComments: targetPage.sheetComments,
        mergedRanges: targetPage.mergedRanges,
        columnWidths: targetPage.columnWidths,
        rowHeights: targetPage.rowHeights,
        trackChangesEnabled: trackChangesEnabled,
        additionalSheets: remainingSheets,
        activeSheetIndex: 0,
        namedRanges: namedRanges,
        precompileAllCells: true
      )
      context.sheetModelCache[targetSheetUpper] = builtModel
      targetModel = builtModel
    }
    let ast = targetModel.compiledASTs[packed] ?? Self.compileCellInput(raw)
    switch ast {
    case .empty:
      return .empty
    case .literalNumber(let num):
      return .number(num)
    case .literalText(let str):
      return .text(str)
    case .literalBoolean(let flag):
      return .boolean(flag)
    default:
      var savedFast = FastCellBitset()
      var savedCount = 0
      var savedVisited: Set<PackedCellCoordinate> = []
      var savedMemo: [PackedCellCoordinate: EvaluatedCellValue] = [:]
      let savedCoord = context.currentCoord
      swap(&savedFast, &context.fastVisited)
      swap(&savedCount, &context.visitedCount)
      swap(&savedVisited, &context.visited)
      swap(&savedMemo, &context.memo)
      targetModel.preEvaluateDAGIterative(roots: [packed], context: &context)
      if let dagMemo = context.memo[packed] {
        for (subCoord, subVal) in context.memo {
          context.crossMemo[CrossSheetCellKey(sheetNameUpper: targetSheetUpper, coord: subCoord)] =
            subVal
        }
        swap(&savedFast, &context.fastVisited)
        swap(&savedCount, &context.visitedCount)
        swap(&savedVisited, &context.visited)
        swap(&savedMemo, &context.memo)
        context.currentCoord = savedCoord
        context.crossMemo[key] = dagMemo
        return dagMemo
      }
      context.currentCoord = packed
      _ = context.crossVisited.insert(key)
      context.markVisited(packed)
      var evaluated = targetModel.evaluateAST(ast, context: &context, depth: depth + 1)
      if case .text(let str) = evaluated, Self.exceedsCellTextLength(str) {
        evaluated = .error(.value)
      }
      evaluated = SheetValueCoercion.sanitizeEvaluatedValue(evaluated)
      for (subCoord, subVal) in context.memo {
        context.crossMemo[CrossSheetCellKey(sheetNameUpper: targetSheetUpper, coord: subCoord)] =
          subVal
      }
      context.crossVisited.remove(key)
      swap(&savedFast, &context.fastVisited)
      swap(&savedCount, &context.visitedCount)
      swap(&savedVisited, &context.visited)
      swap(&savedMemo, &context.memo)
      context.currentCoord = savedCoord
      context.crossMemo[key] = evaluated
      return evaluated
    }
  }

  internal func evaluateInternal(
    at packed: PackedCellCoordinate,
    context: inout EvaluationContext,
    depth: Int
  ) -> EvaluatedCellValue {
    if context.isVisited(packed) {
      return .error("#CYCLE!")
    }
    let selfSheetUpper = name.uppercased()
    if !context.crossVisited.isEmpty,
      context.crossVisited.contains(
        CrossSheetCellKey(sheetNameUpper: selfSheetUpper, coord: packed))
    {
      return .error("#CYCLE!")
    }
    if let memoized = context.memo[packed] {
      return memoized
    }
    if !context.crossMemo.isEmpty,
      let crossCached = context.crossMemo[
        CrossSheetCellKey(sheetNameUpper: selfSheetUpper, coord: packed)]
    {
      context.memo[packed] = crossCached
      return crossCached
    }
    guard depth <= Self.maxFormulaRecursionDepth,
      context.visitedCount <= Self.maxFormulaRecursionDepth
    else {
      return .error("#REF!")
    }
    let ast: FormulaAST
    if let precompiled = compiledASTs[packed] {
      ast = precompiled
    } else if let raw = cellsByCoordinate[packed], !raw.isEmpty {
      ast = Self.compileCellInput(raw)
    } else {
      return .empty
    }

    switch ast {
    case .empty:
      return .empty
    case .literalNumber(let num):
      return .number(num == 0.0 ? 0.0 : num)
    case .literalText(let str):
      return .text(str)
    case .literalBoolean(let flag):
      return .boolean(flag)
    default:
      let prevCoord = context.currentCoord
      context.currentCoord = packed
      context.markVisited(packed)
      var evaluated = evaluateAST(ast, context: &context, depth: depth + 1)
      if case .text(let str) = evaluated, Self.exceedsCellTextLength(str) {
        evaluated = .error(.value)
      }
      evaluated = SheetValueCoercion.sanitizeEvaluatedValue(evaluated)
      context.unmarkVisited(packed)
      context.currentCoord = prevCoord
      context.memo[packed] = evaluated
      return evaluated
    }
  }

  internal func evaluateAST(
    _ ast: FormulaAST,
    context: inout EvaluationContext,
    depth: Int
  ) -> EvaluatedCellValue {
    guard depth <= Self.maxFormulaRecursionDepth,
      context.visitedCount <= Self.maxFormulaRecursionDepth
    else {
      return .error("#REF!")
    }

    let rawResult: EvaluatedCellValue
    switch ast {
    case .empty:
      rawResult = .empty
    case .missing:
      rawResult = .number(0.0)
    case .literalNumber(let num), .number(let num):
      rawResult = .number(num)
    case .literalText(let str), .text(let str):
      rawResult = .text(str)
    case .literalBoolean(let flag), .boolean(let flag):
      rawResult = .boolean(flag)
    case .error(let err):
      rawResult = .error(err.rawValue)
    case .cellRef(let coord):
      let resolved = evaluateInternal(at: coord, context: &context, depth: depth + 1)
      rawResult = (resolved == .empty) ? .number(0.0) : resolved
    case .rangeRef:
      rawResult = .error("#VALUE!")
    case .namedRangeRef(let rangeName):
      guard let targetNotation = resolveNamedRange(rangeName) else {
        return .error("#NAME?")
      }
      if let addr = try? CellAddress(a1Notation: targetNotation) {
        let resolved = evaluateInternal(
          at: addr.packedCoordinate, context: &context, depth: depth + 1)
        rawResult = (resolved == .empty) ? .number(0.0) : resolved
      } else if let range = try? CellRange(notation: targetNotation) {
        let resolved = evaluateInternal(
          at: range.start.packedCoordinate, context: &context, depth: depth + 1)
        rawResult = (resolved == .empty) ? .number(0.0) : resolved
      } else {
        rawResult = .error("#REF!")
      }
    case .sheetCellRef(let sheetName, let coord):
      let resolved = evaluateOnSheet(
        named: sheetName, at: coord, context: &context, depth: depth + 1)
      rawResult = (resolved == .empty) ? .number(0.0) : resolved
    case .sheetRangeRef(let sheetName, _):
      rawResult = hasSheet(named: sheetName) ? .error("#VALUE!") : .error("#REF!")
    case .unaryMinus(let inner):
      let val = evaluateAST(inner, context: &context, depth: depth + 1)
      if case .error = val { return val }
      guard let num = val.numericValue else { return .error("#VALUE!") }
      rawResult = .number(-num)
    case .binaryOp:
      rawResult = evaluateBinaryOpChain(ast, context: &context, depth: depth)
    case .comparisonOp(let op, let lhs, let rhs):
      rawResult = evaluateComparisonOp(op: op, lhs: lhs, rhs: rhs, context: &context, depth: depth)
    case .functionCall(let name, let args):
      rawResult = evaluateFunction(name: name, args: args, context: &context, depth: depth + 1)
    }
    return SheetValueCoercion.sanitizeEvaluatedValue(rawResult)
  }

  internal func evaluateBinaryOp(
    _ op: String,
    lhs: EvaluatedCellValue,
    rhs: EvaluatedCellValue
  ) -> EvaluatedCellValue {
    if case .error = lhs { return lhs }
    if case .error = rhs { return rhs }
    if op == "&" {
      let leftText = lhs.description
      let rightText = rhs.description
      if leftText.utf8.count + rightText.utf8.count > Self.maxCellTextLength,
        leftText.utf16.count + rightText.utf16.count > Self.maxCellTextLength
      {
        return .error(.value)
      }
      return .text(leftText + rightText)
    }
    let lVal = lhs == .empty ? .number(0.0) : lhs
    let rVal = rhs == .empty ? .number(0.0) : rhs
    guard let lNum = lVal.numericValue, let rNum = rVal.numericValue else {
      return .error("#VALUE!")
    }
    let result: Double
    switch op {
    case "+": result = lNum + rNum
    case "-": result = lNum - rNum
    case "*": result = lNum * rNum
    case "/":
      guard rNum != 0.0 else { return .error("#DIV/0!") }
      result = lNum / rNum
    case "^":
      result = pow(lNum, rNum)
    default:
      return .error("#VALUE!")
    }
    guard result.isFinite else { return .error("#NUM!") }
    return .number(result)
  }

  internal func evaluateComparisonOp(
    _ op: String,
    lhs: EvaluatedCellValue,
    rhs: EvaluatedCellValue
  ) -> EvaluatedCellValue {
    var lVal = lhs
    if case .error = lVal { return lVal }
    var rVal = rhs
    if case .error = rVal { return rVal }

    if lVal == .empty {
      switch rVal {
      case .text: lVal = .text("")
      case .boolean: lVal = .boolean(false)
      default: lVal = .number(0.0)
      }
    }
    if rVal == .empty {
      switch lVal {
      case .text: rVal = .text("")
      case .boolean: rVal = .boolean(false)
      default: rVal = .number(0.0)
      }
    }

    if case .number(let lNum) = lVal, case .number(let rNum) = rVal {
      switch op {
      case ">=": return .boolean(lNum >= rNum)
      case "<=": return .boolean(lNum <= rNum)
      case "<>": return .boolean(lNum != rNum)
      case "=": return .boolean(lNum == rNum)
      case ">": return .boolean(lNum > rNum)
      case "<": return .boolean(lNum < rNum)
      default: return .error("#VALUE!")
      }
    } else if case .text(let lStr) = lVal, case .text(let rStr) = rVal {
      let cmp = lStr.caseInsensitiveCompare(rStr)
      switch op {
      case "=": return .boolean(cmp == .orderedSame)
      case "<>": return .boolean(cmp != .orderedSame)
      case ">": return .boolean(cmp == .orderedDescending)
      case "<": return .boolean(cmp == .orderedAscending)
      case ">=": return .boolean(cmp == .orderedSame || cmp == .orderedDescending)
      case "<=": return .boolean(cmp == .orderedSame || cmp == .orderedAscending)
      default: return .error("#VALUE!")
      }
    } else if case .boolean(let lBool) = lVal, case .boolean(let rBool) = rVal {
      let lInt = lBool ? 1 : 0
      let rInt = rBool ? 1 : 0
      switch op {
      case "=": return .boolean(lInt == rInt)
      case "<>": return .boolean(lInt != rInt)
      case ">": return .boolean(lInt > rInt)
      case "<": return .boolean(lInt < rInt)
      case ">=": return .boolean(lInt >= rInt)
      case "<=": return .boolean(lInt <= rInt)
      default: return .error("#VALUE!")
      }
    } else {
      func typeRank(_ v: EvaluatedCellValue) -> Int {
        switch v {
        case .number, .empty: return 1
        case .text: return 2
        case .boolean: return 3
        case .error: return 4
        }
      }
      let lRank = typeRank(lVal)
      let rRank = typeRank(rVal)
      switch op {
      case "=": return .boolean(false)
      case "<>": return .boolean(true)
      case ">": return .boolean(lRank > rRank)
      case "<": return .boolean(lRank < rRank)
      case ">=": return .boolean(lRank >= rRank)
      case "<=": return .boolean(lRank <= rRank)
      default: return .error("#VALUE!")
      }
    }
  }

  private func evaluateBinaryOpChain(
    _ ast: FormulaAST,
    context: inout EvaluationContext,
    depth: Int
  ) -> EvaluatedCellValue {
    var chain: [(op: String, rhs: FormulaAST)] = []
    var cursor = ast
    while case .binaryOp(let op, let l, let r) = cursor {
      chain.append((op: op, rhs: r))
      cursor = l
    }
    var accVal: EvaluatedCellValue
    if chain.last?.op == "&" {
      accVal = evaluateASTPreservingEmpty(cursor, context: &context, depth: depth)
    } else {
      accVal = evaluateAST(cursor, context: &context, depth: depth + 1)
    }
    if case .error = accVal { return accVal }

    for (op, rhs) in chain.reversed() {
      if op == "&" {
        let rhsVal = evaluateASTPreservingEmpty(rhs, context: &context, depth: depth)
        accVal = evaluateBinaryOp("&", lhs: accVal, rhs: rhsVal)
        if case .error = accVal { return accVal }
        continue
      }
      let rhsVal = evaluateAST(rhs, context: &context, depth: depth + 1)
      accVal = evaluateBinaryOp(op, lhs: accVal, rhs: rhsVal)
      if case .error = accVal { return accVal }
    }
    return accVal
  }

  private func evaluateComparisonOp(
    op: String,
    lhs: FormulaAST,
    rhs: FormulaAST,
    context: inout EvaluationContext,
    depth: Int
  ) -> EvaluatedCellValue {
    let lVal = evaluateASTPreservingEmpty(lhs, context: &context, depth: depth)
    if case .error = lVal { return lVal }
    let rVal = evaluateASTPreservingEmpty(rhs, context: &context, depth: depth)
    if case .error = rVal { return rVal }
    return evaluateComparisonOp(op, lhs: lVal, rhs: rVal)
  }

  private func evaluateASTPreservingEmpty(
    _ ast: FormulaAST,
    context: inout EvaluationContext,
    depth: Int
  ) -> EvaluatedCellValue {
    switch ast {
    case .cellRef(let coord):
      return evaluateInternal(at: coord, context: &context, depth: depth + 1)
    case .sheetCellRef(let sheetName, let coord):
      return evaluateOnSheet(named: sheetName, at: coord, context: &context, depth: depth + 1)
    case .namedRangeRef(let rangeName):
      guard let notation = resolveNamedRange(rangeName) else {
        return .error("#NAME?")
      }
      if let addr = try? CellAddress(a1Notation: notation) {
        return evaluateInternal(at: addr.packedCoordinate, context: &context, depth: depth + 1)
      }
      return .error("#REF!")
    default:
      return evaluateAST(ast, context: &context, depth: depth + 1)
    }
  }

  internal func evaluateASTAs2DArray(
    _ ast: FormulaAST,
    frame: inout EvalFrame
  ) -> Result<([[EvaluatedCellValue]], Bool), ErrorCode> {
    switch ast {
    case .rangeRef, .sheetRangeRef, .namedRangeRef,
      .functionCall("OFFSET", _), .functionCall("INDIRECT", _):
      switch resolveRangeArgument(from: ast, frame: &frame) {
      case .failure(let err):
        return .failure(ErrorCode(code: err))
      case .success(let sheetName, let cellRange):
        guard let bounds = rangeBounds(cellRange) else {
          return .failure(ErrorCode(code: "#REF!"))
        }
        var grid: [[EvaluatedCellValue]] = []
        grid.reserveCapacity(bounds.rowCount)
        for r in bounds.minRow...bounds.maxRow {
          var rowVals: [EvaluatedCellValue] = []
          rowVals.reserveCapacity(bounds.colCount)
          for c in bounds.minCol...bounds.maxCol {
            let coord = PackedCellCoordinate(row: r, column: c)
            let v =
              sheetName.map {
                evaluateOnSheet(named: $0, at: coord, context: &frame.context, depth: frame.depth + 1)
              } ?? evaluateInternal(at: coord, context: &frame.context, depth: frame.depth + 1)
            if case .error(let code) = v { return .failure(ErrorCode(code: code)) }
            rowVals.append(v)
          }
          grid.append(rowVals)
        }
        return .success((grid, true))
      }
    case .unaryMinus(let inner):
      switch evaluateASTAs2DArray(inner, frame: &frame) {
      case .failure(let err): return .failure(err)
      case .success((let grid, _)):
        var out: [[EvaluatedCellValue]] = []
        out.reserveCapacity(grid.count)
        for row in grid {
          var outRow: [EvaluatedCellValue] = []
          outRow.reserveCapacity(row.count)
          for item in row {
            if case .error(let code) = item { return .failure(ErrorCode(code: code)) }
            guard let n = item.numericValue else { return .failure(ErrorCode(code: "#VALUE!")) }
            outRow.append(.number(-n))
          }
          out.append(outRow)
        }
        return .success((out, false))
      }
    case .binaryOp(let op, let lhs, let rhs):
      switch (evaluateASTAs2DArray(lhs, frame: &frame), evaluateASTAs2DArray(rhs, frame: &frame)) {
      case (let .failure(err), _), (_, let .failure(err)):
        return .failure(err)
      case (.success((let gL, _)), .success((let gR, _))):
        let rows = max(gL.count, gR.count)
        let cols = max(gL.first?.count ?? 0, gR.first?.count ?? 0)
        guard (gL.count == rows || gL.count == 1),
          (gR.count == rows || gR.count == 1),
          ((gL.first?.count ?? 0) == cols || (gL.first?.count ?? 0) == 1),
          ((gR.first?.count ?? 0) == cols || (gR.first?.count ?? 0) == 1)
        else {
          return .failure(ErrorCode(code: "#VALUE!"))
        }
        var out: [[EvaluatedCellValue]] = []
        out.reserveCapacity(rows)
        for r in 0..<rows {
          var outRow: [EvaluatedCellValue] = []
          outRow.reserveCapacity(cols)
          for c in 0..<cols {
            let lVal = gL[gL.count == 1 ? 0 : r][(gL.first?.count ?? 1) == 1 ? 0 : c]
            let rVal = gR[gR.count == 1 ? 0 : r][(gR.first?.count ?? 1) == 1 ? 0 : c]
            let res = evaluateBinaryOp(op, lhs: lVal, rhs: rVal)
            if case .error(let code) = res { return .failure(ErrorCode(code: code)) }
            outRow.append(res)
          }
          out.append(outRow)
        }
        return .success((out, false))
      }
    case .comparisonOp(let op, let lhs, let rhs):
      switch (evaluateASTAs2DArray(lhs, frame: &frame), evaluateASTAs2DArray(rhs, frame: &frame)) {
      case (let .failure(err), _), (_, let .failure(err)):
        return .failure(err)
      case (.success((let gL, _)), .success((let gR, _))):
        let rows = max(gL.count, gR.count)
        let cols = max(gL.first?.count ?? 0, gR.first?.count ?? 0)
        guard (gL.count == rows || gL.count == 1),
          (gR.count == rows || gR.count == 1),
          ((gL.first?.count ?? 0) == cols || (gL.first?.count ?? 0) == 1),
          ((gR.first?.count ?? 0) == cols || (gR.first?.count ?? 0) == 1)
        else {
          return .failure(ErrorCode(code: "#VALUE!"))
        }
        var out: [[EvaluatedCellValue]] = []
        out.reserveCapacity(rows)
        for r in 0..<rows {
          var outRow: [EvaluatedCellValue] = []
          outRow.reserveCapacity(cols)
          for c in 0..<cols {
            let lVal = gL[gL.count == 1 ? 0 : r][(gL.first?.count ?? 1) == 1 ? 0 : c]
            let rVal = gR[gR.count == 1 ? 0 : r][(gR.first?.count ?? 1) == 1 ? 0 : c]
            let res = evaluateComparisonOp(op, lhs: lVal, rhs: rVal)
            if case .error(let code) = res { return .failure(ErrorCode(code: code)) }
            outRow.append(res)
          }
          out.append(outRow)
        }
        return .success((out, false))
      }
    default:
      let v = evaluateAST(ast, context: &frame.context, depth: frame.depth + 1)
      if case .error(let code) = v { return .failure(ErrorCode(code: code)) }
      return .success(([[v]], false))
    }
  }
}
