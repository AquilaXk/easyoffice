import Foundation

extension WorksheetModel {
  public static let maxFormulaUTF8Bytes = 65_536
  public static let maxFormulaTokenCount = 8_192
  public static let maxFormulaParseDepth = 64

  private static func estimateFormulaTokenCount(_ utf8Source: String) -> Int {
    var tokens = 0
    var inAlnum = false
    var inQuotes = false
    for b in utf8Source.utf8 {
      if b == 0x22 {
        if !inQuotes {
          tokens += 1
          inQuotes = true
          inAlnum = false
        } else {
          inQuotes = false
        }
        continue
      }
      if inQuotes {
        continue
      }
      let isWord =
        (b >= 0x30 && b <= 0x39) || (b >= 0x41 && b <= 0x5A)
        || (b >= 0x61 && b <= 0x7A) || b == 0x5F || b == 0x2E || b == 0x24 || b >= 0x80
      if isWord {
        if !inAlnum {
          tokens += 1
          inAlnum = true
        }
      } else {
        inAlnum = false
        if b != 0x20 && b != 0x09 && b != 0x0A && b != 0x0D {
          tokens += 1
        }
      }
    }
    return tokens
  }

  public static func compileCellInput(_ raw: String) -> FormulaAST {
    guard !raw.isEmpty else { return .empty }
    if raw.hasPrefix("'") {
      return .literalText(String(raw.dropFirst()))
    }
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    guard trimmed.hasPrefix("=") else {
      if let num = SheetValueCoercion.parseStrictNumber(trimmed) {
        return .literalNumber(num)
      }
      if trimmed.caseInsensitiveCompare("TRUE") == .orderedSame {
        return .literalBoolean(true)
      }
      if trimmed.caseInsensitiveCompare("FALSE") == .orderedSame {
        return .literalBoolean(false)
      }
      return .literalText(raw)
    }
    if trimmed.utf8.count > maxFormulaUTF8Bytes {
      return .error(.value)
    }
    let expression = String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces)
    if estimateFormulaTokenCount(expression) > maxFormulaTokenCount {
      return .error(.value)
    }
    return compileExpression(expression, depth: 0)
  }

  public static func compileExpression(_ expr: String, depth: Int = 0) -> FormulaAST {
    guard depth <= maxFormulaParseDepth else { return .error(.value) }
    let trimmed = expr.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return .empty }

    if trimmed.hasPrefix("#") {
      switch trimmed.uppercased() {
      case "#DIV/0!": return .error(.divByZero)
      case "#VALUE!": return .error(.value)
      case "#NAME?": return .error(.name)
      case "#REF!": return .error(.ref)
      case "#N/A": return .error(.na)
      case "#NUM!": return .error(.num)
      case "#CYCLE!": return .error(.cycle)
      default: break
      }
    }

    if let cmpAST = compileTopLevelComparison(trimmed, depth: depth) {
      return cmpAST
    }

    if let arithAST = compileBinaryArithmetic(trimmed, depth: depth) {
      return arithAST
    }

    if trimmed.hasPrefix("(") && trimmed.hasSuffix(")")
      && matchingCloseIndex(in: trimmed, openAt: trimmed.startIndex)
        == trimmed.index(before: trimmed.endIndex)
    {
      let inner = String(trimmed.dropFirst().dropLast())
      return compileExpression(inner, depth: depth + 1)
    }

    if let openParen = trimmed.firstIndex(of: "("),
      trimmed.hasSuffix(")"),
      matchingCloseIndex(in: trimmed, openAt: openParen) == trimmed.index(before: trimmed.endIndex)
    {
      var funcName = String(trimmed[..<openParen]).trimmingCharacters(in: .whitespaces).uppercased()
      for pfx in ["_XLFN.", "COM.MICROSOFT.", "OF:"] where funcName.hasPrefix(pfx) {
        funcName = String(funcName.dropFirst(pfx.count))
      }
      funcName = canonicalFunctionName(funcName)
      if !funcName.isEmpty
        && funcName.unicodeScalars.allSatisfy({
          CharacterSet.alphanumerics.contains($0) || $0 == "." || $0 == "_"
        })
      {
        let innerStart = trimmed.index(after: openParen)
        let innerEnd = trimmed.index(before: trimmed.endIndex)
        let argString = innerStart <= innerEnd ? String(trimmed[innerStart..<innerEnd]) : ""
        let rawArgs = splitTopLevelArguments(argString)
        let compiledArgs = rawArgs.map { arg -> FormulaAST in
          let trimmedArg = arg.trimmingCharacters(in: .whitespacesAndNewlines)
          if trimmedArg.isEmpty {
            return .missing
          }
          return compileExpression(trimmedArg, depth: depth + 1)
        }
        return .functionCall(name: funcName, args: compiledArgs)
      }
    }

    if trimmed.hasPrefix("\"") {
      if let unescaped = parseQuotedStringLiteral(trimmed) {
        return .text(unescaped)
      }
      return .error(.value)
    }

    if trimmed.caseInsensitiveCompare("TRUE") == .orderedSame {
      return .boolean(true)
    }
    if trimmed.caseInsensitiveCompare("FALSE") == .orderedSame {
      return .boolean(false)
    }

    if let number = SheetValueCoercion.parseStrictNumber(trimmed) {
      return .number(number)
    }

    if trimmed.hasPrefix("+") && trimmed.count > 1 {
      let inner = String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces)
      if !inner.isEmpty {
        return compileExpression(inner, depth: depth + 1)
      }
    }
    if trimmed.hasPrefix("-") && trimmed.count > 1 {
      let inner = String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces)
      if !inner.isEmpty {
        return .unaryMinus(compileExpression(inner, depth: depth + 1))
      }
    }

    if trimmed.hasSuffix("%") && trimmed.count > 1 && !trimmed.hasPrefix("\"") {
      let inner = String(trimmed.dropLast()).trimmingCharacters(in: .whitespaces)
      if !inner.isEmpty {
        return .binaryOp(
          op: "/", lhs: compileExpression(inner, depth: depth + 1), rhs: .number(100.0))
      }
    }

    if let crossSheetRef = parseCrossSheetReference(trimmed) {
      return crossSheetRef
    }

    if trimmed.contains(":") && !trimmed.contains("(") && !trimmed.contains("\"") {
      if let range = try? CellRange(notation: trimmed) {
        return .rangeRef(range)
      }
      return .error(.ref)
    }

    if let addr = try? CellAddress(a1Notation: trimmed) {
      return .cellRef(addr.packedCoordinate)
    }

    if isValidNamedRangeIdentifier(trimmed) {
      return .namedRangeRef(trimmed)
    }

    return .error(.value)
  }

  private static func canonicalFunctionName(_ name: String) -> String {
    switch name {
    case "STDEV.S": return "STDEV"
    case "STDEV.P": return "STDEVP"
    case "VAR.S": return "VAR"
    case "VAR.P": return "VARP"
    case "MODE.SNGL": return "MODE"
    case "PERCENTILE.INC": return "PERCENTILE"
    case "QUARTILE.INC": return "QUARTILE"
    case "RANK.EQ": return "RANK"
    case "CEILING.MATH": return "CEILING"
    case "FLOOR.MATH": return "FLOOR"
    case "합계": return "SUM"
    case "평균": return "AVERAGE"
    case "개수": return "COUNT"
    case "최대", "최댓값": return "MAX"
    case "최소", "최솟값": return "MIN"
    case "조건", "만약": return "IF"
    case "찾기": return "VLOOKUP"
    case "반올림": return "ROUND"
    case "올림": return "ROUNDUP"
    case "내림": return "ROUNDDOWN"
    case "절댓값": return "ABS"
    case "연결": return "CONCAT"
    case "길이": return "LEN"
    case "공백제거": return "TRIM"
    case "오늘": return "TODAY"
    default: return name
    }
  }

  private static func parseQuotedStringLiteral(_ token: String) -> String? {
    guard token.hasPrefix("\""), token.count >= 2 else {
      return nil
    }
    var result = ""
    result.reserveCapacity(token.count - 2)
    var idx = token.index(after: token.startIndex)
    while idx < token.endIndex {
      let ch = token[idx]
      if ch == "\"" {
        let nextIdx = token.index(after: idx)
        if nextIdx == token.endIndex {
          guard result.utf8.count <= Self.maxCellTextLength * 4,
            result.count <= Self.maxCellTextLength
          else {
            return nil
          }
          return result
        }
        if token[nextIdx] == "\"" {
          result.append("\"")
          guard result.utf8.count <= Self.maxCellTextLength * 4 else {
            return nil
          }
          idx = token.index(after: nextIdx)
          continue
        }
        return nil
      }
      result.append(ch)
      guard result.utf8.count <= Self.maxCellTextLength * 4 else {
        return nil
      }
      idx = token.index(after: idx)
    }
    return nil
  }

  private static func isValidNamedRangeIdentifier(_ token: String) -> Bool {
    guard let first = token.unicodeScalars.first,
      CharacterSet.letters.contains(first) || first == "_"
    else {
      return false
    }
    return token.unicodeScalars.allSatisfy { scalar in
      CharacterSet.alphanumerics.contains(scalar) || scalar == "_" || scalar == "."
    }
  }

  private static func parseCrossSheetReference(_ token: String) -> FormulaAST? {
    guard token.contains("!"), !token.contains("("), !token.contains("\"") else {
      return nil
    }
    let sheetName: String
    let refPart: String

    if token.hasPrefix("'") {
      var parsedName = ""
      var idx = token.index(after: token.startIndex)
      var closedQuoteEnd: String.Index?
      while idx < token.endIndex {
        let ch = token[idx]
        if ch == "'" {
          let nextIdx = token.index(after: idx)
          if nextIdx < token.endIndex && token[nextIdx] == "'" {
            parsedName.append("'")
            idx = token.index(after: nextIdx)
            continue
          } else {
            closedQuoteEnd = nextIdx
            break
          }
        }
        parsedName.append(ch)
        idx = token.index(after: idx)
      }
      guard let afterQuote = closedQuoteEnd,
        afterQuote < token.endIndex,
        token[afterQuote] == "!"
      else {
        return .error(.ref)
      }
      sheetName = parsedName.trimmingCharacters(in: .whitespacesAndNewlines)
      let refStart = token.index(after: afterQuote)
      refPart = String(token[refStart...]).trimmingCharacters(in: .whitespacesAndNewlines)
    } else {
      guard let bangIdx = token.firstIndex(of: "!") else { return nil }
      sheetName = String(token[..<bangIdx]).trimmingCharacters(in: .whitespacesAndNewlines)
      let refStart = token.index(after: bangIdx)
      refPart = String(token[refStart...]).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    guard !sheetName.isEmpty, !refPart.isEmpty else {
      return .error(.ref)
    }
    if refPart.contains(":") {
      guard let range = try? CellRange(notation: refPart) else {
        return .error(.ref)
      }
      return .sheetRangeRef(sheetName: sheetName, range)
    } else {
      guard let addr = try? CellAddress(a1Notation: refPart) else {
        return .error(.ref)
      }
      return .sheetCellRef(sheetName: sheetName, addr.packedCoordinate)
    }
  }

  private static func compileTopLevelComparison(_ expr: String, depth: Int) -> FormulaAST? {
    let utf8 = expr.utf8
    var parenDepth = 0
    var inDoubleQuotes = false
    var inSingleQuotes = false
    var idx = utf8.startIndex
    var lastSplit: (op: String, opStart: String.Index, afterOp: String.Index)? = nil
    while idx < utf8.endIndex {
      let b = utf8[idx]
      if b == 0x22 && !inSingleQuotes {
        inDoubleQuotes.toggle()
      } else if b == 0x27 && !inDoubleQuotes {
        inSingleQuotes.toggle()
      } else if !inDoubleQuotes && !inSingleQuotes {
        if b == 0x28 {
          parenDepth += 1
        } else if b == 0x29 {
          parenDepth -= 1
        } else if parenDepth == 0 {
          let nextIdx = utf8.index(after: idx)
          let nextByte: UInt8? = nextIdx < utf8.endIndex ? utf8[nextIdx] : nil
          let op: String?
          let afterOpIdx: String.Index
          if b == 0x3E && nextByte == 0x3D {
            op = ">="
            afterOpIdx = utf8.index(after: nextIdx)
          } else if b == 0x3C && nextByte == 0x3D {
            op = "<="
            afterOpIdx = utf8.index(after: nextIdx)
          } else if b == 0x3C && nextByte == 0x3E {
            op = "<>"
            afterOpIdx = utf8.index(after: nextIdx)
          } else if b == 0x3D {
            op = "="
            afterOpIdx = nextIdx
          } else if b == 0x3E {
            op = ">"
            afterOpIdx = nextIdx
          } else if b == 0x3C {
            op = "<"
            afterOpIdx = nextIdx
          } else {
            op = nil
            afterOpIdx = nextIdx
          }

          if let op {
            let leftStr = String(expr[expr.startIndex..<idx]).trimmingCharacters(in: .whitespaces)
            let rightStr = String(expr[afterOpIdx..<expr.endIndex]).trimmingCharacters(
              in: .whitespaces)
            if !leftStr.isEmpty && !rightStr.isEmpty {
              lastSplit = (op: op, opStart: idx, afterOp: afterOpIdx)
              idx = afterOpIdx
              continue
            }
          }
        }
      }
      idx = utf8.index(after: idx)
    }
    if let split = lastSplit {
      let leftStr = String(expr[expr.startIndex..<split.opStart]).trimmingCharacters(
        in: .whitespaces)
      let rightStr = String(expr[split.afterOp..<expr.endIndex]).trimmingCharacters(
        in: .whitespaces)
      return .comparisonOp(
        op: split.op,
        lhs: compileExpression(leftStr, depth: depth + 1),
        rhs: compileExpression(rightStr, depth: depth + 1)
      )
    }
    return nil
  }

  private static func compileBinaryArithmetic(_ expr: String, depth: Int) -> FormulaAST? {
    let utf8 = expr.utf8
    guard !utf8.isEmpty else { return nil }
    let operatorPasses: [(UInt8, UInt8)] = [
      (UInt8(ascii: "&"), UInt8(ascii: "&")),
      (UInt8(ascii: "+"), UInt8(ascii: "-")),
      (UInt8(ascii: "*"), UInt8(ascii: "/")),
      (UInt8(ascii: "^"), UInt8(ascii: "^")),
    ]
    for (opA, opB) in operatorPasses {
      var parenDepth = 0
      var inDoubleQuotes = false
      var inSingleQuotes = false
      var idx = utf8.endIndex
      var currentEnd = expr.endIndex
      var splits: [(op: String, rhs: String)] = []
      while idx > utf8.startIndex {
        idx = utf8.index(before: idx)
        let b = utf8[idx]
        if b == 0x22 && !inSingleQuotes {
          inDoubleQuotes.toggle()
          continue
        }
        if b == 0x27 && !inDoubleQuotes {
          inSingleQuotes.toggle()
          continue
        }
        if inDoubleQuotes || inSingleQuotes { continue }
        if b == 0x29 {
          parenDepth += 1
        } else if b == 0x28 {
          parenDepth -= 1
        } else if parenDepth == 0 && idx > utf8.startIndex && (b == opA || b == opB) {
          let nextIdx = utf8.index(after: idx)
          let lhsStr = String(expr[expr.startIndex..<idx]).trimmingCharacters(in: .whitespaces)
          let rhsStr = String(expr[nextIdx..<currentEnd]).trimmingCharacters(in: .whitespaces)
          if lhsStr.isEmpty || rhsStr.isEmpty { continue }
          if b == 0x2F {
            let upperLhs = lhsStr.uppercased()
            if upperLhs.hasSuffix("#N") || upperLhs.hasSuffix("#DIV") {
              continue
            }
          }
          if let prevByte = lhsStr.utf8.last,
            prevByte == 0x2B || prevByte == 0x2D || prevByte == 0x2A || prevByte == 0x2F
              || prevByte == 0x5E || prevByte == 0x26 || prevByte == 0x28 || prevByte == 0x2C
              || prevByte == 0x3D || prevByte == 0x3C || prevByte == 0x3E
          {
            continue
          }
          if (b == 0x2B || b == 0x2D) && isScientificNotationExponentPrefix(lhsStr) {
            continue
          }
          let opStr: String
          switch b {
          case 0x26: opStr = "&"
          case 0x2B: opStr = "+"
          case 0x2D: opStr = "-"
          case 0x2A: opStr = "*"
          case 0x2F: opStr = "/"
          case 0x5E: opStr = "^"
          default: opStr = "+"
          }
          splits.append((op: opStr, rhs: rhsStr))
          currentEnd = idx
        }
      }
      if !splits.isEmpty {
        let firstTerm = String(expr[expr.startIndex..<currentEnd]).trimmingCharacters(
          in: .whitespaces)
        guard !firstTerm.isEmpty else { continue }
        var acc = compileExpression(firstTerm, depth: depth + 1)
        for (opStr, rhsStr) in splits.reversed() {
          acc = .binaryOp(
            op: opStr,
            lhs: acc,
            rhs: compileExpression(rhsStr, depth: depth + 1)
          )
        }
        return acc
      }
    }
    return nil
  }

  private static func isScientificNotationExponentPrefix(_ lhsStr: String) -> Bool {
    let bytes = Array(lhsStr.utf8)
    guard bytes.count >= 2 else { return false }
    let last = bytes[bytes.count - 1]
    guard last == 0x45 || last == 0x65 else { return false }
    var idx = bytes.count - 2
    var hasDigit = false
    while idx >= 0 {
      let ch = bytes[idx]
      if ch >= 0x30 && ch <= 0x39 {
        hasDigit = true
        idx -= 1
      } else if ch == 0x2E {
        idx -= 1
      } else {
        let isWordChar =
          (ch >= 0x41 && ch <= 0x5A) || (ch >= 0x61 && ch <= 0x7A) || ch == 0x5F || ch == 0x24
        return hasDigit && !isWordChar
      }
    }
    return hasDigit
  }

  private static func matchingCloseIndex(in text: String, openAt: String.Index) -> String.Index? {
    var depth = 0
    var inDoubleQuotes = false
    var inSingleQuotes = false
    var idx = openAt
    while idx < text.endIndex {
      let ch = text[idx]
      if ch == "\"" && !inSingleQuotes {
        inDoubleQuotes.toggle()
      } else if ch == "'" && !inDoubleQuotes {
        inSingleQuotes.toggle()
      }
      if !inDoubleQuotes && !inSingleQuotes {
        if ch == "(" {
          depth += 1
        } else if ch == ")" {
          depth -= 1
          if depth == 0 { return idx }
        }
      }
      idx = text.index(after: idx)
    }
    return nil
  }

  private static func splitTopLevelArguments(_ input: String) -> [String] {
    guard !input.trimmingCharacters(in: .whitespaces).isEmpty else { return [] }
    var args: [String] = []
    var current = ""
    var depth = 0
    var inDoubleQuotes = false
    var inSingleQuotes = false

    for ch in input {
      if ch == "\"" && !inSingleQuotes {
        inDoubleQuotes.toggle()
      } else if ch == "'" && !inDoubleQuotes {
        inSingleQuotes.toggle()
      }
      if !inDoubleQuotes && !inSingleQuotes {
        if ch == "(" {
          depth += 1
        } else if ch == ")" {
          depth -= 1
        } else if ch == "," && depth == 0 {
          args.append(current.trimmingCharacters(in: .whitespaces))
          current = ""
          continue
        }
      }
      current.append(ch)
    }
    args.append(current.trimmingCharacters(in: .whitespaces))
    return args
  }

  internal struct ParsedReferenceNotation {
    let sheetName: String?
    let range: CellRange
    let isSingleCell: Bool
  }

  internal enum ParsedReferenceResult {
    case success(ParsedReferenceNotation)
    case failure(String)
  }

  internal func parseReferenceNotation(_ rawNotation: String) -> ParsedReferenceResult {
    let trimmed = rawNotation.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return .failure("#REF!") }

    if let bangIdx = trimmed.lastIndex(of: "!") {
      var sheetPart = String(trimmed[..<bangIdx]).trimmingCharacters(in: .whitespaces)
      let refPart = String(trimmed[trimmed.index(after: bangIdx)...]).trimmingCharacters(
        in: .whitespaces)
      if sheetPart.hasPrefix("'") && sheetPart.hasSuffix("'") && sheetPart.count >= 2 {
        sheetPart = String(sheetPart.dropFirst().dropLast()).replacingOccurrences(
          of: "''", with: "'")
      }
      guard !sheetPart.isEmpty, hasSheet(named: sheetPart) else {
        return .failure("#REF!")
      }
      if refPart.contains(":") {
        guard let r = try? CellRange(notation: refPart) else {
          return .failure("#REF!")
        }
        return .success(
          ParsedReferenceNotation(sheetName: sheetPart, range: r, isSingleCell: false))
      }
      guard let addr = try? CellAddress(a1Notation: refPart) else {
        return .failure("#REF!")
      }
      return .success(
        ParsedReferenceNotation(
          sheetName: sheetPart,
          range: CellRange(start: addr, end: addr),
          isSingleCell: true
        )
      )
    }

    if trimmed.contains(":") {
      guard let r = try? CellRange(notation: trimmed) else {
        return .failure("#REF!")
      }
      return .success(ParsedReferenceNotation(sheetName: nil, range: r, isSingleCell: false))
    }
    if let addr = try? CellAddress(a1Notation: trimmed) {
      return .success(
        ParsedReferenceNotation(
          sheetName: nil,
          range: CellRange(start: addr, end: addr),
          isSingleCell: true
        )
      )
    }
    return .failure("#REF!")
  }

  internal enum IndirectResolutionResult {
    case success(sheetName: String?, range: CellRange, isSingleCell: Bool)
    case failure(String)
  }

  internal func resolveIndirectRange(
    args: [FormulaAST],
    frame: inout EvalFrame
  ) -> IndirectResolutionResult {
    guard (1...2).contains(args.count) else {
      return .failure("#VALUE!")
    }
    let refVal = evaluateAST(args[0], context: &frame.context, depth: frame.depth + 1)
    if case .error(let code) = refVal {
      return .failure(code)
    }
    if args.count == 2 {
      let a1Val = evaluateAST(args[1], context: &frame.context, depth: frame.depth + 1)
      if case .error(let code) = a1Val {
        return .failure(code)
      }
    }
    let rawRef = refVal.description.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !rawRef.isEmpty else {
      return .failure("#REF!")
    }
    let targetNotation = resolveNamedRange(rawRef) ?? rawRef
    switch parseReferenceNotation(targetNotation) {
    case .failure(let err):
      return .failure(err)
    case .success(let parsed):
      return .success(
        sheetName: parsed.sheetName,
        range: parsed.range,
        isSingleCell: parsed.isSingleCell
      )
    }
  }

  internal func extractCellRange(from ast: FormulaAST) -> CellRange? {
    if let range = ast.asCellRange {
      return range
    }
    if case .namedRangeRef(let rangeName) = ast,
      let notation = resolveNamedRange(rangeName),
      case .success(let parsed) = parseReferenceNotation(notation)
    {
      return parsed.range
    }
    return nil
  }

  internal enum OffsetResolutionResult {
    case success(sheetName: String?, range: CellRange)
    case failure(String)
  }

  internal func resolveRangeArgument(
    from ast: FormulaAST,
    frame: inout EvalFrame
  ) -> OffsetResolutionResult {
    switch ast {
    case .rangeRef(let r):
      return .success(sheetName: nil, range: r)
    case .sheetRangeRef(let s, let r):
      guard hasSheet(named: s) else { return .failure("#REF!") }
      return .success(sheetName: s, range: r)
    case .cellRef(let coord):
      let addr = CellAddress(packed: coord)
      return .success(sheetName: nil, range: CellRange(start: addr, end: addr))
    case .sheetCellRef(let s, let coord):
      guard hasSheet(named: s) else { return .failure("#REF!") }
      let addr = CellAddress(packed: coord)
      return .success(sheetName: s, range: CellRange(start: addr, end: addr))
    case .namedRangeRef(let name):
      guard let notation = resolveNamedRange(name) else {
        return .failure("#NAME?")
      }
      switch parseReferenceNotation(notation) {
      case .failure(let err):
        return .failure(err)
      case .success(let parsed):
        return .success(sheetName: parsed.sheetName, range: parsed.range)
      }
    case .functionCall(let fnName, let fnArgs) where fnName == "OFFSET":
      switch resolveOffsetRange(args: fnArgs, frame: &frame) {
      case .failure(let err):
        return .failure(err)
      case .success(let s, let r):
        if let s, !hasSheet(named: s) { return .failure("#REF!") }
        return .success(sheetName: s, range: r)
      }
    case .functionCall(let fnName, let fnArgs) where fnName == "INDIRECT":
      switch resolveIndirectRange(args: fnArgs, frame: &frame) {
      case .failure(let err):
        return .failure(err)
      case .success(let s, let r, _):
        return .success(sheetName: s, range: r)
      }
    case .functionCall(let fnName, let fnArgs) where fnName == "TRANSPOSE" && fnArgs.count == 1:
      return resolveRangeArgument(from: fnArgs[0], frame: &frame)
    case .error(let err):
      return .failure(err.rawValue)
    default:
      return .failure("#VALUE!")
    }
  }

  internal func resolveRangeArgumentWithTranspose(
    from ast: FormulaAST,
    frame: inout EvalFrame
  ) -> (result: OffsetResolutionResult, isTransposed: Bool) {
    if case .functionCall(let fnName, let fnArgs) = ast, fnName == "TRANSPOSE", fnArgs.count == 1 {
      let inner = resolveRangeArgumentWithTranspose(from: fnArgs[0], frame: &frame)
      return (inner.result, !inner.isTransposed)
    }
    return (resolveRangeArgument(from: ast, frame: &frame), false)
  }

  internal func resolveOffsetRange(
    args: [FormulaAST],
    frame: inout EvalFrame
  ) -> OffsetResolutionResult {
    guard (3...5).contains(args.count) else {
      return .failure("#VALUE!")
    }
    let baseSheetName: String?
    let baseRange: CellRange
    switch resolveRangeArgument(from: args[0], frame: &frame) {
    case .failure(let err):
      return .failure(err)
    case .success(let s, let r):
      baseSheetName = s
      baseRange = r
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

    let maxRow = CellAddress.maxZeroBasedRow
    let maxCol = CellAddress.maxZeroBasedColumn
    guard (-maxRow...maxRow).contains(rowDelta),
      (-maxCol...maxCol).contains(colDelta)
    else {
      return .failure("#REF!")
    }

    let baseRow = min(baseRange.start.row, baseRange.end.row)
    let baseCol = min(baseRange.start.column, baseRange.end.column)
    let (targetRow, rowOverflow) = baseRow.addingReportingOverflow(rowDelta)
    let (targetCol, colOverflow) = baseCol.addingReportingOverflow(colDelta)
    guard !rowOverflow,
      !colOverflow,
      targetRow >= 0,
      targetCol >= 0,
      targetRow <= maxRow,
      targetCol <= maxCol
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
      guard (1...(maxRow + 1)).contains(hInt) else {
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
      guard (1...(maxCol + 1)).contains(wInt) else {
        return .failure("#REF!")
      }
      width = wInt
    } else {
      width = defaultWidth
    }

    let (endRow, endRowOverflow) = targetRow.addingReportingOverflow(height - 1)
    let (endCol, endColOverflow) = targetCol.addingReportingOverflow(width - 1)
    let (area, areaOverflow) = height.multipliedReportingOverflow(by: width)
    guard !endRowOverflow,
      !endColOverflow,
      !areaOverflow,
      endRow >= 0,
      endCol >= 0,
      endRow <= maxRow,
      endCol <= maxCol,
      area <= CellRange.maxExpandedCellCount
    else {
      return .failure("#REF!")
    }

    let startAddr = CellAddress(packed: PackedCellCoordinate(row: targetRow, column: targetCol))
    let endAddr = CellAddress(packed: PackedCellCoordinate(row: endRow, column: endCol))
    return .success(
      sheetName: baseSheetName,
      range: CellRange(start: startAddr, end: endAddr)
    )
  }
}
