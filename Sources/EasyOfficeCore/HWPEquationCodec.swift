import Foundation

public enum HWPEquationCodec {
  public static let tagEqEditStandard: UInt16 = 88
  public static let maxDepth: Int = 64

  public static let puaBMPSymbols: Set<UInt16> = {
    var set: Set<UInt16> = [
      0xA0E1, 0xA12B, 0xA289, 0xA28A,
      0xA2FC, 0xA3C5, 0xA807, 0xA80A, 0xA80E, 0xA80F, 0xA810, 0xA81A, 0xA832,
    ]
    for code in UInt16(0xA292)...UInt16(0xA29B) { set.insert(code) }
    for code in UInt16(0xA2B1)...UInt16(0xA2C0) { set.insert(code) }
    for code in UInt16(0xA2C3)...UInt16(0xA2CC) { set.insert(code) }
    for code in UInt16(0xA852)...UInt16(0xA855) { set.insert(code) }
    return set
  }()

  public static func hwp5SymbolToPlane15(_ unit: UInt16) -> UInt32? {
    guard puaBMPSymbols.contains(unit) else { return nil }
    return 0x0F_0000 | UInt32(unit & 0x0FFF)
  }

  public static func plane15ToHWP5Symbol(_ codePoint: UInt32) -> UInt16? {
    guard (codePoint & 0xFF_F000) == 0x0F_0000 else { return nil }
    let unit = UInt16(0xA000) | UInt16(codePoint & 0x0FFF)
    return puaBMPSymbols.contains(unit) ? unit : nil
  }

  public static func decodeParaTextPayload(
    from data: Data,
    at offset: Int,
    byteLength: Int
  ) throws -> String {
    guard byteLength >= 0, byteLength % 2 == 0, offset >= 0, offset + byteLength <= data.count else {
      throw EasyOfficeError.corruptedContainer(
        detail: "HWP 5.0 문단 텍스트 페이로드 길이가 2바이트(WCHAR) 배수가 아니거나 범위를 초과합니다.")
    }
    let payloadEnd = offset + byteLength
    var units: [UInt16] = []
    units.reserveCapacity(byteLength / 2)

    data.withUnsafeBytes { rawBuffer in
      var cursor = offset
      while cursor < payloadEnd {
        let code = rawBuffer.loadUnaligned(fromByteOffset: cursor, as: UInt16.self).littleEndian
        if code < 0x0020 {
          switch code {
          case 0x0009:
            units.append(0x0009)
            if cursor + 16 <= payloadEnd,
              rawBuffer.loadUnaligned(fromByteOffset: cursor + 14, as: UInt16.self).littleEndian
                == 0x0009,
              rawBuffer.loadUnaligned(fromByteOffset: cursor + 12, as: UInt16.self).littleEndian == 0
            {
              cursor += 16
            } else {
              cursor += 2
            }
          case 0x000A:
            units.append(0x000A)
            cursor += 2
          case 0x000D:
            if cursor + 2 < payloadEnd {
              units.append(0x000D)
            }
            cursor += 2
          case 0x0012:
            units.append(0x0020)
            if cursor + 16 <= payloadEnd,
              rawBuffer.loadUnaligned(fromByteOffset: cursor + 14, as: UInt16.self).littleEndian
                == 0x0012
            {
              cursor += 16
            } else {
              cursor += 2
            }
          case 0x0018:
            units.append(0x00AD)
            cursor += 2
          case 0x0019:
            units.append(0x0020)
            cursor += 2
          case 0x001E:
            units.append(0x00A0)
            cursor += 2
          case 0x001F:
            units.append(0x2007)
            cursor += 2
          case 0x0000:
            cursor += 2
          default:
            if code <= 0x0017, cursor + 16 <= payloadEnd,
              rawBuffer.loadUnaligned(fromByteOffset: cursor + 14, as: UInt16.self).littleEndian
                == code
            {
              cursor += 16
            } else {
              cursor += 2
            }
          }
        } else if (0xD800...0xDBFF).contains(code) {
          if cursor + 4 <= payloadEnd {
            let low =
              rawBuffer.loadUnaligned(fromByteOffset: cursor + 2, as: UInt16.self).littleEndian
            if (0xDC00...0xDFFF).contains(low) {
              let cp = 0x10000 + ((UInt32(code) - 0xD800) << 10) + (UInt32(low) - 0xDC00)
              if (cp & 0xFF_F000) == 0x0F_0000,
                let bmp = plane15ToHWP5Symbol(cp),
                UnicodeScalar(bmp) != nil
              {
                units.append(bmp)
              } else {
                units.append(code)
                units.append(low)
              }
              cursor += 4
            } else {
              units.append(0x25A1)
              cursor += 2
            }
          } else {
            units.append(0x25A1)
            cursor += 2
          }
        } else if (0xDC00...0xDFFF).contains(code) {
          units.append(0x25A1)
          cursor += 2
        } else {
          units.append(code)
          cursor += 2
        }
      }
    }

    return String(decoding: units, as: UTF16.self)
  }

  public static func decodeHWP5EqEditRecord(
    from data: Data,
    at payloadStart: Int,
    byteLength: Int
  ) throws -> DocumentEquationBlock {
    guard payloadStart >= 0, byteLength >= 6, payloadStart + byteLength <= data.count else {
      throw EasyOfficeError.corruptedContainer(
        detail: "HWP 5.0 HWPTAG_EQEDIT(88) 수식 레코드 헤더 길이가 부족합니다.")
    }
    let scriptLen = Int(try BitPacking.readUInt16LE(from: data, at: payloadStart + 4))
    let scriptBytes = scriptLen * 2
    guard 6 + scriptBytes <= byteLength else {
      throw EasyOfficeError.corruptedContainer(
        detail: "HWP 5.0 HWPTAG_EQEDIT(88) 수식 스크립트 길이가 레코드 경계를 초과합니다.")
    }
    var units: [UInt16] = []
    units.reserveCapacity(scriptLen)
    let scriptStart = payloadStart + 6
    let scriptEnd = scriptStart + scriptBytes
    data.withUnsafeBytes { rawBuffer in
      var cursor = scriptStart
      while cursor < scriptEnd {
        let ch = rawBuffer.loadUnaligned(fromByteOffset: cursor, as: UInt16.self).littleEndian
        if ch == 0 { break }
        if (0xD800...0xDBFF).contains(ch) {
          if cursor + 4 <= scriptEnd {
            let low =
              rawBuffer.loadUnaligned(fromByteOffset: cursor + 2, as: UInt16.self).littleEndian
            if (0xDC00...0xDFFF).contains(low) {
              let cp = 0x10000 + ((UInt32(ch) - 0xD800) << 10) + (UInt32(low) - 0xDC00)
              if let bmp = plane15ToHWP5Symbol(cp) {
                units.append(bmp)
              } else {
                units.append(ch)
                units.append(low)
              }
              cursor += 4
              continue
            }
          }
          units.append(0x25A1)
          cursor += 2
        } else if (0xDC00...0xDFFF).contains(ch) {
          units.append(0x25A1)
          cursor += 2
        } else {
          units.append(ch)
          cursor += 2
        }
      }
    }
    let rawScript = String(decoding: units, as: UTF16.self)
    let trimmed = rawScript.trimmingCharacters(in: .whitespacesAndNewlines)
    let latex = trimmed.isEmpty ? "" : try convertEqEditToLaTeX(trimmed)
    return DocumentEquationBlock(latexSource: latex)
  }

  public static func normalizeToLaTeXIfNeeded(_ raw: String) throws -> String {
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty, !trimmed.contains("\\") else { return raw }
    guard containsEqEditKeyword(trimmed) else { return raw }
    return try convertEqEditToLaTeX(trimmed)
  }

  private static let eqEditTriggerKeywords: Set<String> = [
    "over", "atop", "choose", "sqrt", "root", "of", "from", "to", "sup", "sub",
    "sum", "prod", "int", "oint", "lim", "matrix", "pmatrix", "bmatrix", "dmatrix", "cases",
    "left", "right", "binom", "bar", "hat", "vec", "dot", "ddot", "tilde", "acute", "grave",
    "check", "breve", "overline", "underline", "overbrace", "underbrace", "rm", "it", "bold",
    "alpha", "beta", "gamma", "delta", "epsilon", "varepsilon", "zeta", "eta", "theta",
    "vartheta", "iota", "kappa", "lambda", "mu", "nu", "xi", "pi", "varpi", "rho", "varrho",
    "sigma", "varsigma", "tau", "upsilon", "phi", "varphi", "chi", "psi", "omega",
    "times", "div", "cdot", "pm", "mp", "leq", "le", "geq", "ge", "neq", "ne",
    "approx", "equiv", "sim", "simeq", "cong", "propto", "in", "notin", "subset", "supset",
    "subseteq", "supseteq", "cup", "cap", "emptyset", "forall", "exist", "exists", "nabla",
    "partial", "therefore", "because", "infty", "inf", "infin", "rarrow", "rightarrow",
    "larrow", "leftarrow", "lrarrow", "leftrightarrow", "cdots", "ldots", "vdots", "ddots",
  ]

  private static func containsEqEditKeyword(_ text: String) -> Bool {
    if text.contains("`") || text.contains("#") { return true }
    var word = ""
    for ch in text {
      if ch.isLetter {
        word.append(ch)
      } else {
        if !word.isEmpty {
          if eqEditTriggerKeywords.contains(word.lowercased()) { return true }
          word.removeAll(keepingCapacity: true)
        }
      }
    }
    if !word.isEmpty, eqEditTriggerKeywords.contains(word.lowercased()) { return true }
    return false
  }

  public static func extractHWPXEquations(
    from sectionXML: String,
    existing: [DocumentEquationBlock]
  ) throws -> [DocumentEquationBlock] {
    var result: [DocumentEquationBlock] = []
    result.reserveCapacity(existing.count)
    for eq in existing {
      result.append(
        DocumentEquationBlock(
          id: eq.id,
          displayTitle: eq.displayTitle,
          rawExpression: try normalizeToLaTeXIfNeeded(eq.rawExpression)
        )
      )
    }
    let existingExpressions = Set(result.map(\.rawExpression))
    for eqElem in XMLSanitizer.extractElements(named: "hp:equation", in: sectionXML) {
      let scriptRaw =
        XMLSanitizer.extractElements(named: "hp:script", in: eqElem.innerXML).first?.innerXML
        ?? eqElem.innerXML
      let unescaped = XMLSanitizer.unescape(scriptRaw).trimmingCharacters(
        in: .whitespacesAndNewlines)
      guard !unescaped.isEmpty else { continue }
      let converted = try convertEqEditToLaTeX(unescaped)
      if !existingExpressions.contains(converted) {
        result.append(DocumentEquationBlock(latexSource: converted))
      }
    }
    return result
  }

  public static func convertEqEditToLaTeX(_ script: String) throws -> String {
    let tokens = try tokenizeEqEdit(script)
    var parser = EqEditParser(tokens: tokens)
    let expr = try parser.parseExpr(depth: 0)
    guard parser.pos == parser.tokens.count else {
      throw EasyOfficeError.corruptedContainer(
        detail: "HWP EqEdit 수식에 닫는 중괄호가 누락되었거나 구문이 잘못되었습니다.")
    }
    var out = ""
    emitExpr(expr, into: &out)
    return out
  }
}

private enum EqToken: Equatable {
  case lBrace, rBrace, caret, underscore, tilde, backtick, amp, hash
  case number(String), ident(String), text(String), symbol(Character)
}

private enum MatrixStyle: Equatable {
  case plain, paren, bracket, det, cases
}

private indirect enum EqExpr: Equatable {
  case empty
  case seq([EqExpr])
  case group(EqExpr)
  case number(String)
  case ident(String)
  case text(String)
  case symbol(Character)
  case space(Character)
  case frac(EqExpr, EqExpr)
  case atop(EqExpr, EqExpr)
  case sqrt(index: EqExpr?, radicand: EqExpr)
  case script(base: EqExpr, sub: EqExpr?, sup: EqExpr?)
  case bigOp(op: String, lower: EqExpr?, upper: EqExpr?)
  case delimited(left: String, body: EqExpr, right: String)
  case matrix(style: MatrixStyle, rows: [[EqExpr]])
  case accent(kind: String, body: EqExpr)
  case font(kind: String, body: EqExpr)
  case binom(EqExpr, EqExpr)
}

private func tokenizeEqEdit(_ input: String) throws -> [EqToken] {
  let chars = Array(input)
  var tokens: [EqToken] = []
  var i = 0
  var braceBalance = 0

  while i < chars.count {
    let c = chars[i]
    if c.isWhitespace {
      i += 1
      continue
    }
    switch c {
    case "{":
      braceBalance += 1
      if braceBalance > HWPEquationCodec.maxDepth {
        throw EasyOfficeError.resourceBudgetExceeded(
          detail: "HWP EqEdit 수식 중괄호 중첩 깊이가 상한(\(HWPEquationCodec.maxDepth))을 초과했습니다.")
      }
      tokens.append(.lBrace)
      i += 1
    case "}":
      braceBalance -= 1
      if braceBalance < 0 {
        throw EasyOfficeError.corruptedContainer(detail: "HWP EqEdit 수식에 닫는 중괄호가 초과되었습니다.")
      }
      tokens.append(.rBrace)
      i += 1
    case "^":
      tokens.append(.caret)
      i += 1
    case "_":
      tokens.append(.underscore)
      i += 1
    case "~":
      tokens.append(.tilde)
      i += 1
    case "`":
      tokens.append(.backtick)
      i += 1
    case "&":
      tokens.append(.amp)
      i += 1
    case "#":
      tokens.append(.hash)
      i += 1
    case "\"":
      i += 1
      var s = ""
      while i < chars.count && chars[i] != "\"" {
        s.append(chars[i])
        i += 1
      }
      if i < chars.count { i += 1 }
      tokens.append(.text(s))
    default:
      if c.isASCII && c.isNumber {
        var n = String(c)
        i += 1
        while i < chars.count && ((chars[i].isASCII && chars[i].isNumber) || chars[i] == ".") {
          n.append(chars[i])
          i += 1
        }
        tokens.append(.number(n))
      } else if c.isASCII && c.isLetter {
        var id = String(c)
        i += 1
        while i < chars.count && chars[i].isASCII && chars[i].isLetter {
          id.append(chars[i])
          i += 1
        }
        tokens.append(.ident(id))
      } else {
        tokens.append(.symbol(c))
        i += 1
      }
    }
  }

  if braceBalance != 0 {
    throw EasyOfficeError.corruptedContainer(detail: "HWP EqEdit 수식에 닫히지 않은 중괄호가 존재합니다.")
  }
  return tokens
}

private struct EqEditParser {
  let tokens: [EqToken]
  var pos: Int = 0

  func peek() -> EqToken? {
    pos < tokens.count ? tokens[pos] : nil
  }

  mutating func next() -> EqToken? {
    guard pos < tokens.count else { return nil }
    let t = tokens[pos]
    pos += 1
    return t
  }

  func peekKeyword(_ kw: String) -> Bool {
    guard case .ident(let s)? = peek() else { return false }
    return s.caseInsensitiveCompare(kw) == .orderedSame
  }

  mutating func parseExpr(
    depth: Int,
    stopOnRight: Bool = false,
    stopOnMatrixDelim: Bool = false
  ) throws -> EqExpr {
    try checkDepth(depth)
    var items: [EqExpr] = []
    while let tok = peek() {
      if tok == .rBrace || (stopOnRight && peekKeyword("right"))
        || (stopOnMatrixDelim && (tok == .amp || tok == .hash))
      {
        break
      }
      if peekKeyword("over") || peekKeyword("atop") || peekKeyword("choose") {
        let isOver = peekKeyword("over")
        let isChoose = peekKeyword("choose")
        _ = next()
        let lhs: EqExpr =
          items.isEmpty ? .empty : (items.count == 1 ? items.removeFirst() : .seq(items))
        items.removeAll(keepingCapacity: true)
        let rhs = try parseExpr(
          depth: depth + 1,
          stopOnRight: stopOnRight,
          stopOnMatrixDelim: stopOnMatrixDelim
        )
        return isOver ? .frac(lhs, rhs) : (isChoose ? .binom(lhs, rhs) : .atop(lhs, rhs))
      }
      items.append(try parseScript(depth: depth))
    }
    switch items.count {
    case 0: return .empty
    case 1: return items[0]
    default: return .seq(items)
    }
  }

  mutating func parseScript(depth: Int) throws -> EqExpr {
    try checkDepth(depth)
    var base = try parseAtom(depth: depth)
    if case .ident(let name) = base, isBigOp(name) {
      var lower: EqExpr?
      var upper: EqExpr?
      var chainCount = 0
      while true {
        if peekKeyword("from") {
          chainCount += 1
          try checkDepth(depth + chainCount)
          _ = next()
          lower = try parseScriptArg(depth: depth + chainCount)
        } else if peekKeyword("to") {
          chainCount += 1
          try checkDepth(depth + chainCount)
          _ = next()
          upper = try parseAtom(depth: depth + chainCount)
        } else if peek() == .underscore || peekKeyword("sub") || peekKeyword("lsub") {
          chainCount += 1
          try checkDepth(depth + chainCount)
          _ = next()
          lower = try parseAtom(depth: depth + chainCount)
        } else if peek() == .caret || peekKeyword("sup") || peekKeyword("lsup") {
          chainCount += 1
          try checkDepth(depth + chainCount)
          _ = next()
          upper = try parseAtom(depth: depth + chainCount)
        } else {
          break
        }
      }
      return .bigOp(op: name.lowercased(), lower: lower, upper: upper)
    }

    var sub: EqExpr?
    var sup: EqExpr?
    var chainCount = 0
    while true {
      if peek() == .underscore || peekKeyword("sub") || peekKeyword("lsub") {
        chainCount += 1
        try checkDepth(depth + chainCount)
        _ = next()
        let arg = try parseAtom(depth: depth + chainCount)
        if sub != nil {
          base = .script(base: base, sub: sub, sup: sup)
          sub = nil
          sup = nil
        }
        sub = arg
      } else if peek() == .caret || peekKeyword("sup") || peekKeyword("lsup") {
        chainCount += 1
        try checkDepth(depth + chainCount)
        _ = next()
        let arg = try parseAtom(depth: depth + chainCount)
        if sup != nil {
          base = .script(base: base, sub: sub, sup: sup)
          sub = nil
          sup = nil
        }
        sup = arg
      } else {
        break
      }
    }
    if sub != nil || sup != nil {
      return .script(base: base, sub: sub, sup: sup)
    }
    return base
  }

  mutating func parseScriptArg(depth: Int) throws -> EqExpr {
    try checkDepth(depth)
    if peek() == .lBrace {
      return try parseAtom(depth: depth)
    }
    var items: [EqExpr] = []
    while let tok = peek() {
      if tok == .rBrace || tok == .amp || tok == .hash || peekKeyword("to")
        || peekKeyword("over") || peekKeyword("atop") || peekKeyword("choose")
        || peekKeyword("right")
      {
        break
      }
      items.append(try parseAtom(depth: depth))
      if items.count == 1, case .symbol(_) = peek() { continue }
      if items.count == 2, case .symbol(_) = items[1] { continue }
      break
    }
    switch items.count {
    case 0: return .empty
    case 1: return items[0]
    default: return .seq(items)
    }
  }

  mutating func parseAtom(depth: Int) throws -> EqExpr {
    try checkDepth(depth)
    guard let tok = next() else { return .empty }
    switch tok {
    case .lBrace:
      let inner = try parseExpr(depth: depth + 1)
      guard peek() == .rBrace else {
        throw EasyOfficeError.corruptedContainer(detail: "HWP EqEdit 수식에 닫는 중괄호가 누락되었습니다.")
      }
      _ = next()
      return .group(inner)
    case .rBrace:
      throw EasyOfficeError.corruptedContainer(detail: "HWP EqEdit 수식에 예기치 않은 닫는 중괄호가 있습니다.")
    case .number(let n): return .number(n)
    case .text(let s): return .text(s)
    case .tilde: return .space("~")
    case .backtick: return .space("`")
    case .symbol(let c): return .symbol(c)
    case .caret: return .symbol("^")
    case .underscore: return .symbol("_")
    case .amp: return .symbol("&")
    case .hash: return .symbol("#")
    case .ident(let word): return try parseIdentCommand(word, depth: depth)
    }
  }

  mutating func parseIdentCommand(_ word: String, depth: Int) throws -> EqExpr {
    let lower = word.lowercased()
    switch lower {
    case "sqrt":
      return .sqrt(index: nil, radicand: try parseAtom(depth: depth + 1))
    case "root":
      let idx = try parseAtom(depth: depth + 1)
      if peekKeyword("of") { _ = next() }
      return .sqrt(index: idx, radicand: try parseAtom(depth: depth + 1))
    case "left":
      let l = parseDelimiter()
      let body = try parseExpr(depth: depth + 1, stopOnRight: true)
      let r: String
      if peekKeyword("right") {
        _ = next()
        r = parseDelimiter()
      } else {
        r = "."
      }
      return .delimited(left: l, body: body, right: r)
    case "matrix", "pile", "lpile", "cpile", "rpile":
      return try parseMatrix(style: .plain, depth: depth + 1)
    case "pmatrix": return try parseMatrix(style: .paren, depth: depth + 1)
    case "bmatrix": return try parseMatrix(style: .bracket, depth: depth + 1)
    case "dmatrix": return try parseMatrix(style: .det, depth: depth + 1)
    case "cases": return try parseMatrix(style: .cases, depth: depth + 1)
    case "binom":
      return .binom(try parseAtom(depth: depth + 1), try parseAtom(depth: depth + 1))
    case "bar", "hat", "vec", "dot", "ddot", "tilde", "acute", "grave", "check", "breve",
      "overline", "underline", "overbrace", "underbrace", "dyad", "under", "arch":
      return .accent(kind: lower, body: try parseAtom(depth: depth + 1))
    case "rm", "it", "bold":
      return .font(kind: lower, body: try parseAtom(depth: depth + 1))
    case "scale", "color", "size":
      _ = try parseAtom(depth: depth + 1)
      return try parseAtom(depth: depth + 1)
    case "roman", "ital":
      return .empty
    default:
      return .ident(word)
    }
  }

  mutating func parseDelimiter() -> String {
    guard let tok = next() else { return "." }
    switch tok {
    case .symbol(let c): return String(c)
    case .lBrace: return "\\{"
    case .rBrace: return "\\}"
    case .ident(let s):
      switch s.lowercased() {
      case "lfloor": return "\\lfloor"
      case "rfloor": return "\\rfloor"
      case "lceil": return "\\lceil"
      case "rceil": return "\\rceil"
      case "langle", "larrow": return "\\langle"
      case "rangle", "rarrow": return "\\rangle"
      case "vert", "line": return "|"
      case "dvert", "dline": return "\\|"
      case "none", "nothing": return "."
      default: return s
      }
    default: return "."
    }
  }

  mutating func parseMatrix(style: MatrixStyle, depth: Int) throws -> EqExpr {
    try checkDepth(depth)
    let expectBrace = (peek() == .lBrace)
    if expectBrace { _ = next() }
    var rows: [[EqExpr]] = []
    var currentRow: [EqExpr] = []
    while let tok = peek() {
      if expectBrace && tok == .rBrace {
        _ = next()
        break
      }
      if !expectBrace && (tok == .rBrace || peekKeyword("right")) {
        break
      }
      let cell = try parseExpr(
        depth: depth,
        stopOnRight: !expectBrace,
        stopOnMatrixDelim: true
      )
      currentRow.append(cell)
      if peek() == .amp {
        _ = next()
      } else if peek() == .hash {
        _ = next()
        rows.append(currentRow)
        currentRow = []
      } else {
        if expectBrace && peek() == .rBrace { _ = next() }
        break
      }
    }
    if !currentRow.isEmpty || rows.isEmpty {
      rows.append(currentRow)
    }
    return .matrix(style: style, rows: rows)
  }

  func checkDepth(_ depth: Int) throws {
    if depth > HWPEquationCodec.maxDepth {
      throw EasyOfficeError.resourceBudgetExceeded(
        detail: "HWP EqEdit 수식 구문 트리 깊이가 상한(\(HWPEquationCodec.maxDepth))을 초과했습니다.")
    }
  }

  func isBigOp(_ s: String) -> Bool {
    switch s.lowercased() {
    case "sum", "prod", "coprod", "int", "oint", "dint", "tint", "lim", "bigcup", "bigcap",
      "bigoplus", "bigotimes":
      return true
    default:
      return false
    }
  }
}

private func emitUnbraced(_ expr: EqExpr, into out: inout String) {
  if case .group(let inner) = expr {
    emitExpr(inner, into: &out)
  } else {
    emitExpr(expr, into: &out)
  }
}

private func emitExpr(_ expr: EqExpr, into out: inout String) {
  switch expr {
  case .empty:
    break
  case .seq(let items):
    for (idx, item) in items.enumerated() {
      if idx > 0 && needsSpaceBetween(items[idx - 1], item) {
        out.append(" ")
      }
      emitExpr(item, into: &out)
    }
  case .group(let inner):
    out.append("{")
    emitExpr(inner, into: &out)
    out.append("}")
  case .number(let n):
    out.append(n)
  case .ident(let word):
    out.append(mapIdent(word))
  case .text(let s):
    if s.isEmpty { return }
    out.append("\\text{\(s)}")
  case .symbol(let c):
    switch c {
    case "{": out.append("\\{")
    case "}": out.append("\\}")
    case "%": out.append("\\%")
    case "#": out.append("\\\\")
    default: out.append(c)
    }
  case .space(let c):
    out.append(c == "~" ? "\\ " : "\\,")
  case .frac(let a, let b):
    out.append("\\frac{")
    emitUnbraced(a, into: &out)
    out.append("}{")
    emitUnbraced(b, into: &out)
    out.append("}")
  case .atop(let a, let b):
    out.append("{")
    emitUnbraced(a, into: &out)
    out.append(" \\atop ")
    emitUnbraced(b, into: &out)
    out.append("}")
  case .sqrt(let index, let rad):
    out.append("\\sqrt")
    if let idx = index {
      out.append("[")
      emitUnbraced(idx, into: &out)
      out.append("]")
    }
    out.append("{")
    emitUnbraced(rad, into: &out)
    out.append("}")
  case .script(let base, let sub, let sup):
    let wrapBase: Bool
    switch base {
    case .frac, .atop, .script: wrapBase = true
    default: wrapBase = false
    }
    if wrapBase { out.append("{") }
    emitUnbraced(base, into: &out)
    if wrapBase { out.append("}") }
    if let s = sub {
      out.append("_")
      emitBraced(s, into: &out)
    }
    if let p = sup {
      out.append("^")
      emitBraced(p, into: &out)
    }
  case .bigOp(let op, let lower, let upper):
    out.append(mapBigOp(op))
    if let l = lower {
      out.append("_")
      emitBraced(l, into: &out)
    }
    if let u = upper {
      out.append("^")
      emitBraced(u, into: &out)
    }
  case .delimited(let left, let body, let right):
    out.append("\\left\(left) ")
    emitUnbraced(body, into: &out)
    out.append(" \\right\(right)")
  case .matrix(let style, let rows):
    let env: String
    switch style {
    case .plain: env = "matrix"
    case .paren: env = "pmatrix"
    case .bracket: env = "bmatrix"
    case .det: env = "vmatrix"
    case .cases: env = "cases"
    }
    out.append("\\begin{\(env)}")
    for (rIdx, row) in rows.enumerated() {
      if rIdx > 0 { out.append(" \\\\ ") }
      for (cIdx, cell) in row.enumerated() {
        if cIdx > 0 { out.append(" & ") }
        emitUnbraced(cell, into: &out)
      }
    }
    out.append("\\end{\(env)}")
  case .accent(let kind, let body):
    let cmd: String
    switch kind {
    case "bar", "overline": cmd = "\\overline"
    case "under", "underline": cmd = "\\underline"
    case "hat": cmd = "\\hat"
    case "vec": cmd = "\\vec"
    case "dyad": cmd = "\\overleftrightarrow"
    case "dot": cmd = "\\dot"
    case "ddot": cmd = "\\ddot"
    case "tilde": cmd = "\\tilde"
    case "acute": cmd = "\\acute"
    case "grave": cmd = "\\grave"
    case "check": cmd = "\\check"
    case "breve": cmd = "\\breve"
    case "arch": cmd = "\\wideparen"
    case "overbrace": cmd = "\\overbrace"
    case "underbrace": cmd = "\\underbrace"
    default: cmd = "\\bar"
    }
    out.append("\(cmd){")
    emitUnbraced(body, into: &out)
    out.append("}")
  case .font(let kind, let body):
    let cmd = (kind == "rm") ? "\\mathrm" : (kind == "bold" ? "\\mathbf" : "\\mathit")
    out.append("\(cmd){")
    emitUnbraced(body, into: &out)
    out.append("}")
  case .binom(let a, let b):
    out.append("\\binom{")
    emitUnbraced(a, into: &out)
    out.append("}{")
    emitUnbraced(b, into: &out)
    out.append("}")
  }
}

private func emitBraced(_ expr: EqExpr, into out: inout String) {
  let isSingle: Bool
  switch expr {
  case .number(let n): isSingle = (n.count == 1)
  case .symbol(_): isSingle = true
  default: isSingle = false
  }
  if isSingle {
    emitExpr(expr, into: &out)
  } else {
    out.append("{")
    emitUnbraced(expr, into: &out)
    out.append("}")
  }
}

private func needsSpaceBetween(_ a: EqExpr, _ b: EqExpr) -> Bool {
  if case .bigOp(let op, _, _) = a {
    switch op {
    case "int", "oint", "dint", "tint": return false
    default: return true
    }
  }
  var buf = ""
  emitExpr(a, into: &buf)
  var bufB = ""
  emitExpr(b, into: &bufB)
  guard let lastA = buf.last, let firstB = bufB.first else { return false }
  if buf.contains("\\") && lastA.isASCII && lastA.isLetter {
    return firstB.isASCII && (firstB.isLetter || firstB.isNumber)
  }
  return lastA.isASCII && (lastA.isLetter || lastA.isNumber) && firstB == "\\"
}

private func mapBigOp(_ op: String) -> String {
  switch op {
  case "sum": return "\\sum"
  case "prod": return "\\prod"
  case "coprod": return "\\coprod"
  case "int": return "\\int"
  case "oint": return "\\oint"
  case "dint": return "\\iint"
  case "tint": return "\\iiint"
  case "lim": return "\\lim"
  case "bigcup": return "\\bigcup"
  case "bigcap": return "\\bigcap"
  case "bigoplus": return "\\bigoplus"
  case "bigotimes": return "\\bigotimes"
  default: return "\\operatorname{\(op)}"
  }
}

private let capitalizedGreekMap: [String: String] = [
  "gamma": "\\Gamma", "delta": "\\Delta", "theta": "\\Theta", "lambda": "\\Lambda",
  "xi": "\\Xi", "pi": "\\Pi", "sigma": "\\Sigma", "upsilon": "\\Upsilon",
  "phi": "\\Phi", "psi": "\\Psi", "omega": "\\Omega",
]

private let lowerIdentMap: [String: String] = [
  "alpha": "\\alpha", "beta": "\\beta", "gamma": "\\gamma", "delta": "\\delta",
  "epsilon": "\\epsilon", "varepsilon": "\\varepsilon", "zeta": "\\zeta", "eta": "\\eta",
  "theta": "\\theta", "vartheta": "\\vartheta", "iota": "\\iota", "kappa": "\\kappa",
  "lambda": "\\lambda", "mu": "\\mu", "nu": "\\nu", "xi": "\\xi", "omicron": "o",
  "pi": "\\pi", "varpi": "\\varpi", "rho": "\\rho", "varrho": "\\varrho",
  "sigma": "\\sigma", "varsigma": "\\varsigma", "tau": "\\tau", "upsilon": "\\upsilon",
  "phi": "\\phi", "varphi": "\\varphi", "chi": "\\chi", "psi": "\\psi", "omega": "\\omega",
  "aleph": "\\aleph", "hbar": "\\hbar", "imath": "\\imath", "jmath": "\\jmath",
  "ell": "\\ell", "wp": "\\wp", "re": "\\Re", "im": "\\Im", "mho": "\\mho",
  "prime": "\\prime", "emptyset": "\\emptyset", "empty": "\\emptyset",
  "nabla": "\\nabla", "grad": "\\nabla", "partial": "\\partial", "del": "\\partial",
  "infty": "\\infty", "inf": "\\infty", "infin": "\\infty",
  "forall": "\\forall", "exist": "\\exists", "exists": "\\exists",
  "neg": "\\neg", "not": "\\neg", "lnot": "\\neg",
  "flat": "\\flat", "natural": "\\natural", "sharp": "\\sharp",
  "clubsuit": "\\clubsuit", "diamondsuit": "\\diamondsuit",
  "heartsuit": "\\heartsuit", "spadesuit": "\\spadesuit",
  "angle": "\\angle", "triangle": "\\triangle", "perp": "\\perp", "bot": "\\bot",
  "top": "\\top", "parallel": "\\parallel",
  "therefore": "\\therefore", "because": "\\because",
  "times": "\\times", "div": "\\div", "divide": "\\div", "cdot": "\\cdot", "centerdot": "\\cdot",
  "pm": "\\pm", "plusminus": "\\pm", "mp": "\\mp", "minusplus": "\\mp",
  "circ": "\\circ", "bullet": "\\bullet", "star": "\\star", "ast": "\\ast",
  "oplus": "\\oplus", "ominus": "\\ominus", "otimes": "\\otimes", "oslash": "\\oslash",
  "odot": "\\odot", "dagger": "\\dagger", "ddagger": "\\ddagger", "amalg": "\\amalg",
  "cap": "\\cap", "inter": "\\cap", "cup": "\\cup", "union": "\\cup",
  "sqcap": "\\sqcap", "sqcup": "\\sqcup", "vee": "\\vee", "or": "\\vee",
  "wedge": "\\wedge", "and": "\\wedge", "setminus": "\\setminus", "wr": "\\wr",
  "leq": "\\leq", "le": "\\leq", "geq": "\\geq", "ge": "\\geq",
  "neq": "\\neq", "ne": "\\neq", "equiv": "\\equiv", "approx": "\\approx",
  "sim": "\\sim", "simeq": "\\simeq", "cong": "\\cong", "propto": "\\propto",
  "prec": "\\prec", "succ": "\\succ", "preceq": "\\preceq", "succeq": "\\succeq",
  "ll": "\\ll", "gg": "\\gg", "subset": "\\subset", "supset": "\\supset",
  "subseteq": "\\subseteq", "supseteq": "\\supseteq",
  "sqsubset": "\\sqsubset", "sqsupset": "\\sqsupset",
  "sqsubseteq": "\\sqsubseteq", "sqsupseteq": "\\sqsupseteq",
  "in": "\\in", "owns": "\\ni", "ni": "\\ni", "notin": "\\notin",
  "vdash": "\\vdash", "dashv": "\\dashv", "models": "\\models", "mid": "\\mid", "doteq": "\\doteq",
  "larrow": "\\leftarrow", "leftarrow": "\\leftarrow", "gets": "\\leftarrow",
  "rarrow": "\\rightarrow", "rightarrow": "\\rightarrow", "to": "\\rightarrow",
  "lrarrow": "\\leftrightarrow", "leftrightarrow": "\\leftrightarrow",
  "uparrow": "\\uparrow", "downarrow": "\\downarrow", "updownarrow": "\\updownarrow",
  "nearrow": "\\nearrow", "searrow": "\\searrow", "swarrow": "\\swarrow", "nwarrow": "\\nwarrow",
  "mapsto": "\\mapsto", "hookleftarrow": "\\hookleftarrow", "hookrightarrow": "\\hookrightarrow",
  "cdots": "\\cdots", "ldots": "\\ldots", "vdots": "\\vdots", "ddots": "\\ddots", "dots": "\\dots",
  "sin": "\\sin", "cos": "\\cos", "tan": "\\tan", "cot": "\\cot", "sec": "\\sec", "csc": "\\csc",
  "arcsin": "\\arcsin", "arccos": "\\arccos", "arctan": "\\arctan",
  "sinh": "\\sinh", "cosh": "\\cosh", "tanh": "\\tanh", "coth": "\\coth",
  "ln": "\\ln", "log": "\\log", "lg": "\\lg", "exp": "\\exp",
  "det": "\\det", "dim": "\\dim", "ker": "\\ker", "hom": "\\hom", "deg": "\\deg", "gcd": "\\gcd",
  "max": "\\max", "min": "\\min", "arg": "\\arg", "lim": "\\lim",
  "liminf": "\\liminf", "limsup": "\\limsup", "infimum": "\\inf", "supremum": "\\sup",
  "mod": "\\bmod", "pmod": "\\pmod",
  "quad": "\\quad", "qquad": "\\qquad", "it": "", "rm": "", "bold": "",
]

private func mapIdent(_ word: String) -> String {
  let lower = word.lowercased()
  if let first = word.first, first.isUppercase, let capGreek = capitalizedGreekMap[lower] {
    return capGreek
  }
  switch word {
  case "Larrow", "Leftarrow": return "\\Leftarrow"
  case "Rarrow", "Rightarrow": return "\\Rightarrow"
  case "LRarrow", "Lrarrow", "Leftrightarrow": return "\\Leftrightarrow"
  case "Uparrow": return "\\Uparrow"
  case "Downarrow": return "\\Downarrow"
  default: break
  }
  if let mapped = lowerIdentMap[lower] {
    return mapped
  }
  return word
}
