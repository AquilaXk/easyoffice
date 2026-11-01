import Foundation

public enum SheetValueCoercion {
  public static func isValidDecimalSyntax(_ raw: String) -> Bool {
    let utf8 = Array(raw.utf8)
    var start = 0
    var end = utf8.count
    while start < end && isASCIIWhitespace(utf8[start]) { start += 1 }
    while end > start && isASCIIWhitespace(utf8[end - 1]) { end -= 1 }
    guard start < end else { return false }

    var i = start
    if utf8[i] == 0x2B || utf8[i] == 0x2D {
      i += 1
      guard i < end else { return false }
    }

    var hasDigits = false
    while i < end && utf8[i] >= 0x30 && utf8[i] <= 0x39 {
      hasDigits = true
      i += 1
    }
    if i < end && utf8[i] == 0x2E {
      i += 1
      while i < end && utf8[i] >= 0x30 && utf8[i] <= 0x39 {
        hasDigits = true
        i += 1
      }
    }
    guard hasDigits else { return false }

    if i < end && (utf8[i] == 0x65 || utf8[i] == 0x45) {
      i += 1
      guard i < end else { return false }
      if utf8[i] == 0x2B || utf8[i] == 0x2D {
        i += 1
        guard i < end else { return false }
      }
      var hasExpDigits = false
      while i < end && utf8[i] >= 0x30 && utf8[i] <= 0x39 {
        hasExpDigits = true
        i += 1
      }
      guard hasExpDigits else { return false }
    }

    return i == end
  }

  @inline(__always)
  private static func isASCIIWhitespace(_ b: UInt8) -> Bool {
    b == 0x20 || b == 0x09 || b == 0x0A || b == 0x0D
  }

  public static func parseStrictNumber(_ raw: String) -> Double? {
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    guard isValidDecimalSyntax(trimmed), let v = Double(trimmed), v.isFinite else {
      return nil
    }
    return v == 0.0 ? 0.0 : v
  }

  public static func parseISODateSerial(_ raw: String) -> Double? {
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    let hasDash = trimmed.contains("-")
    let hasSlash = trimmed.contains("/")
    guard (hasDash && !hasSlash) || (!hasDash && hasSlash) else {
      return nil
    }
    let sep: Character = hasDash ? "-" : "/"
    let parts = trimmed.split(separator: sep, omittingEmptySubsequences: false)
    guard parts.count == 3,
      parts[0].count == 4,
      (1...2).contains(parts[1].count),
      (1...2).contains(parts[2].count),
      parts[0].allSatisfy(\.isASCII) && parts[0].allSatisfy(\.isNumber),
      parts[1].allSatisfy(\.isASCII) && parts[1].allSatisfy(\.isNumber),
      parts[2].allSatisfy(\.isASCII) && parts[2].allSatisfy(\.isNumber),
      let y = Int(parts[0]),
      let m = Int(parts[1]),
      let d = Int(parts[2]),
      (1900...9999).contains(y),
      (1...12).contains(m)
    else {
      return nil
    }
    let maxD = daysInMonth(year: y, month: m)
    guard (1...maxD).contains(d) else { return nil }
    return ymdToSerial(year: y, month: m, day: d)
  }

  public static func parseValueText(_ raw: String) -> Double? {
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return nil }
    if let n = parseStrictNumber(trimmed) {
      return n
    }
    if let serial = parseISODateSerial(trimmed) {
      return serial
    }
    var work = trimmed
    var isNegParen = false
    if work.hasPrefix("(") && work.hasSuffix(")") && work.count >= 3 {
      isNegParen = true
      work = String(work.dropFirst().dropLast()).trimmingCharacters(in: .whitespacesAndNewlines)
    }
    var isPercent = false
    if work.hasSuffix("%") {
      isPercent = true
      work = String(work.dropLast()).trimmingCharacters(in: .whitespacesAndNewlines)
    }
    for prefix in ["$", "₩", "€", "¥", "£"] {
      if work.hasPrefix(prefix) {
        work = String(work.dropFirst(prefix.count)).trimmingCharacters(in: .whitespacesAndNewlines)
        break
      }
    }
    let degrouped = work.replacingOccurrences(of: ",", with: "")
    guard var v = parseStrictNumber(degrouped) else { return nil }
    if isNegParen {
      v = -v
    }
    let scaled = isPercent ? (v / 100.0) : v
    guard scaled.isFinite else { return nil }
    return scaled == 0.0 ? 0.0 : scaled
  }

  public struct CoercionError: Error, Equatable, Sendable {
    public let code: String
    public init(_ code: String) {
      self.code = code
    }
  }

  public static func coerceToNumber(_ value: EvaluatedCellValue) -> Result<Double, CoercionError> {
    switch value {
    case .empty:
      return .success(0.0)
    case .number(let n):
      guard n.isFinite else { return .failure(CoercionError("#NUM!")) }
      return .success(n == 0.0 ? 0.0 : n)
    case .boolean(let b):
      return .success(b ? 1.0 : 0.0)
    case .text(let s):
      if let n = parseValueText(s) {
        return .success(n)
      }
      return .failure(CoercionError("#VALUE!"))
    case .error(let e):
      return .failure(CoercionError(e))
    }
  }

  public static func coerceToBool(_ value: EvaluatedCellValue) -> Result<Bool, CoercionError> {
    switch value {
    case .empty:
      return .success(false)
    case .boolean(let b):
      return .success(b)
    case .number(let n):
      guard n.isFinite else { return .failure(CoercionError("#NUM!")) }
      return .success(n != 0.0)
    case .text(let s):
      let t = s.trimmingCharacters(in: .whitespacesAndNewlines)
      if t.caseInsensitiveCompare("TRUE") == .orderedSame { return .success(true) }
      if t.caseInsensitiveCompare("FALSE") == .orderedSame { return .success(false) }
      return .failure(CoercionError("#VALUE!"))
    case .error(let e):
      return .failure(CoercionError(e))
    }
  }

  public static func sanitizeEvaluatedValue(_ value: EvaluatedCellValue) -> EvaluatedCellValue {
    if case .number(let n) = value {
      guard n.isFinite else {
        return .error("#NUM!")
      }
      if n == 0.0 {
        return .number(0.0)
      }
    }
    return value
  }

  public static func formatGeneralNumber(_ value: Double) -> String {
    guard value.isFinite else { return "#NUM!" }
    if value == 0.0 { return "0" }
    let sign = value < 0.0 ? "-" : ""
    let absVal = abs(value)
    let sci = String(format: "%.14e", absVal)
    guard let eIndex = sci.firstIndex(where: { $0 == "e" || $0 == "E" }) else {
      return "\(sign)\(absVal)"
    }
    let mantissaPart = sci[..<eIndex]
    let expPart = sci[sci.index(after: eIndex)...]
    guard let exp = Int(expPart) else {
      return "\(sign)\(absVal)"
    }
    var digits: [UInt8] = []
    digits.reserveCapacity(15)
    for b in mantissaPart.utf8 where b >= 0x30 && b <= 0x39 {
      digits.append(b)
    }
    while digits.count > 1 && digits.last == 0x30 {
      digits.removeLast()
    }
    if exp >= -9 && exp < 15 {
      if exp < 0 {
        let zeros = String(repeating: "0", count: -1 - exp)
        let sig = String(decoding: digits, as: UTF8.self)
        return "\(sign)0.\(zeros)\(sig)"
      }
      let intDigits = exp + 1
      if intDigits >= digits.count {
        let sig = String(decoding: digits, as: UTF8.self)
        let trailingZeros = String(repeating: "0", count: intDigits - digits.count)
        return "\(sign)\(sig)\(trailingZeros)"
      }
      let intPart = String(decoding: digits[0..<intDigits], as: UTF8.self)
      let fracPart = String(decoding: digits[intDigits...], as: UTF8.self)
      return "\(sign)\(intPart).\(fracPart)"
    }
    let expSign = exp >= 0 ? "+" : "-"
    let absExp = abs(exp)
    if digits.count == 1 {
      let d0 = String(decoding: digits, as: UTF8.self)
      return "\(sign)\(d0)E\(expSign)\(absExp)"
    }
    let d0 = String(decoding: digits[0..<1], as: UTF8.self)
    let rest = String(decoding: digits[1...], as: UTF8.self)
    return "\(sign)\(d0).\(rest)E\(expSign)\(absExp)"
  }

  public static func formatFixedNumber(_ value: Double, decimals: Int, noCommas: Bool) -> String {
    guard value.isFinite else { return "#NUM!" }
    let factor = pow(10.0, Double(abs(decimals)))
    let rounded: Double
    if decimals >= 0 {
      if !factor.isFinite || decimals >= 300 {
        rounded = value
      } else {
        let scaled = (value * factor).rounded()
        rounded = scaled.isFinite ? (scaled / factor) : value
      }
    } else {
      if !factor.isFinite || decimals <= -300 {
        rounded = 0.0
      } else {
        rounded = (value / factor).rounded() * factor
      }
    }
    let normalized = rounded == 0.0 ? 0.0 : rounded
    let fracDigits = max(0, min(20, decimals))
    if noCommas {
      return String(format: "%.\(fracDigits)f", normalized)
    }
    return CellNumberFormat.formatGroupedNumber(normalized, fractionDigits: fracDigits)
  }

  public static func formatTextPattern(_ value: EvaluatedCellValue, pattern: String) -> EvaluatedCellValue {
    if case .error = value { return value }
    let trimmedPat = pattern.trimmingCharacters(in: .whitespacesAndNewlines)
    let lowerPat = trimmedPat.lowercased()
    let isDatePattern =
      (lowerPat.contains("y") || lowerPat.contains("d") || lowerPat.contains("m"))
      && !lowerPat.contains("0")
      && !lowerPat.contains("#")
      && !lowerPat.contains("%")
    if isDatePattern {
      if let (year, month, day) = resolveDateYMD(from: value) {
        var out = ""
        let chars = Array(trimmedPat)
        var i = 0
        while i < chars.count {
          let ch = chars[i]
          let lowerCh = ch.lowercased()
          if lowerCh == "y" || lowerCh == "m" || lowerCh == "d" {
            var runLen = 1
            while i + runLen < chars.count && chars[i + runLen].lowercased() == lowerCh {
              runLen += 1
            }
            if lowerCh == "y" {
              out +=
                runLen >= 3
                ? String(format: "%04d", year)
                : String(format: "%02d", year % 100)
            } else if lowerCh == "m" {
              out += runLen >= 2 ? String(format: "%02d", month) : "\(month)"
            } else {
              out += runLen >= 2 ? String(format: "%02d", day) : "\(day)"
            }
            i += runLen
          } else {
            out.append(ch)
            i += 1
          }
        }
        return .text(out)
      }
    }
    guard case .success(let num) = coerceToNumber(value), num.isFinite else {
      return .text(value.description)
    }
    if trimmedPat.contains("%") {
      let pctDecimals: Int
      if let dotIndex = trimmedPat.firstIndex(of: ".") {
        let afterDot = trimmedPat[trimmedPat.index(after: dotIndex)...]
        pctDecimals = min(10, afterDot.prefix(while: { $0 == "0" || $0 == "#" }).count)
      } else {
        pctDecimals = 0
      }
      let scaled = num * 100.0
      return .text("\(formatFixedNumber(scaled, decimals: pctDecimals, noCommas: !trimmedPat.contains(",")))%")
    }
    let decimals: Int
    if let dotIndex = trimmedPat.firstIndex(of: ".") {
      let afterDot = trimmedPat[trimmedPat.index(after: dotIndex)...]
      decimals = min(10, afterDot.prefix(while: { $0 == "0" || $0 == "#" }).count)
    } else {
      decimals = 0
    }
    let useCommas = trimmedPat.contains(",")
    return .text(formatFixedNumber(num, decimals: decimals, noCommas: !useCommas))
  }

  public static func resolveDateYMD(from value: EvaluatedCellValue) -> (Int, Int, Int)? {
    if case .number(let n) = value, n.isFinite {
      let s = floor(n)
      if s >= 19_000_101 && s <= 99_991_231 {
        let iv = Int(s)
        let y = iv / 10_000
        let m = (iv / 100) % 100
        let d = iv % 100
        if (1...12).contains(m) && (1...daysInMonth(year: y, month: m)).contains(d) {
          return (y, m, d)
        }
      }
      return serialToYMD(s)
    }
    if case .text(let str) = value {
      if let serial = parseISODateSerial(str) {
        return serialToYMD(serial)
      }
      let raw = str.trimmingCharacters(in: .whitespacesAndNewlines)
      let parts = raw.components(separatedBy: CharacterSet(charactersIn: "-/."))
      if parts.count == 3,
        let y = Int(parts[0]),
        let m = Int(parts[1]),
        let d = Int(parts[2]),
        (1...9999).contains(y),
        (1...12).contains(m),
        (1...daysInMonth(year: y, month: m)).contains(d)
      {
        return (y, m, d)
      }
    }
    return nil
  }

  public static func isLeapYear(_ year: Int) -> Bool {
    if year == 1900 { return true }
    return (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
  }

  public static func daysInMonth(year: Int, month: Int) -> Int {
    switch month {
    case 1, 3, 5, 7, 8, 10, 12: return 31
    case 4, 6, 9, 11: return 30
    case 2: return isLeapYear(year) ? 29 : 28
    default: return 30
    }
  }

  public static func ymdToSerial(year: Int, month: Int, day: Int) -> Double {
    var y = year
    if (0...1899).contains(y) {
      y += 1900
    }
    var m = month
    let d = day
    while m > 12 {
      y += 1
      m -= 12
    }
    while m < 1 {
      y -= 1
      m += 12
    }
    var serial = 0
    for yr in 1900..<y {
      serial += isLeapYear(yr) ? 366 : 365
    }
    for mo in 1..<m {
      serial += daysInMonth(year: y, month: mo)
    }
    serial += d
    return Double(serial)
  }

  public static func serialToYMD(_ serial: Double) -> (Int, Int, Int)? {
    guard serial.isFinite else { return nil }
    let s = floor(serial)
    guard s >= 1.0 && s <= 2_958_465.0 else { return nil }
    var rem = Int(s)
    if rem == 60 {
      return (1900, 2, 29)
    }
    var y = 1900
    while true {
      let daysInY = isLeapYear(y) ? 366 : 365
      if rem > daysInY {
        rem -= daysInY
        y += 1
      } else {
        break
      }
    }
    var m = 1
    while m <= 12 {
      let dim = daysInMonth(year: y, month: m)
      if rem > dim {
        rem -= dim
        m += 1
      } else {
        break
      }
    }
    return (y, m, max(1, rem))
  }

  public static func formatDateISO(_ serial: Double) -> String? {
    guard serial.isFinite else { return nil }
    let rounded = serial.rounded()
    if rounded >= 19_000_101 && rounded <= 29_991_231 {
      let iv = Int(rounded)
      let y = iv / 10_000
      let m = (iv % 10_000) / 100
      let d = iv % 100
      return String(format: "%04d-%02d-%02d", y, m, d)
    }
    guard let (y, m, d) = serialToYMD(rounded) else { return nil }
    return String(format: "%04d-%02d-%02d", y, m, d)
  }

  public static func isAutoDateFormula(_ ast: FormulaAST) -> Bool {
    guard case .functionCall(let rawName, _) = ast else { return false }
    switch rawName.uppercased() {
    case "DATE", "EDATE", "EOMONTH", "WORKDAY", "TODAY", "DATEVALUE", "오늘":
      return true
    default:
      return false
    }
  }
}
