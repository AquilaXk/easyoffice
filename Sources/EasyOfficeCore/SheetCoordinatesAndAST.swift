import Foundation

public struct PackedCellCoordinate: Sendable, Codable, Equatable, Hashable, Comparable,
  CustomStringConvertible
{
  public let rawValue: UInt64

  private enum CodingKeys: String, CodingKey {
    case rawValue
  }

  @inlinable
  public init(rawValue: UInt64) {
    self.rawValue = rawValue
  }

  @inlinable
  public init(row: Int, column: Int) {
    let clampedRow = UInt32(clamping: max(0, row))
    let clampedColumn = UInt32(clamping: max(0, column))
    self.rawValue = (UInt64(clampedRow) << 32) | UInt64(clampedColumn)
  }

  public init(from decoder: Decoder) throws {
    let raw: UInt64
    if let container = try? decoder.container(keyedBy: CodingKeys.self),
      let keyedRaw = try? container.decode(UInt64.self, forKey: .rawValue)
    {
      raw = keyedRaw
    } else {
      let single = try decoder.singleValueContainer()
      raw = try single.decode(UInt64.self)
    }
    let decodedRow = Int(raw >> 32)
    let decodedCol = Int(raw & 0xFFFF_FFFF)
    guard decodedRow >= 0, decodedRow <= CellAddress.maxZeroBasedRow,
      decodedCol >= 0, decodedCol <= CellAddress.maxZeroBasedColumn
    else {
      throw DecodingError.dataCorrupted(
        DecodingError.Context(
          codingPath: decoder.codingPath,
          debugDescription:
            "PackedCellCoordinate out of ECMA-376 bounds: row=\(decodedRow), col=\(decodedCol)"
        )
      )
    }
    self.rawValue = raw
  }

  @inlinable
  public var row: Int {
    Int(rawValue >> 32)
  }

  @inlinable
  public var column: Int {
    Int(rawValue & 0xFFFF_FFFF)
  }

  @inlinable
  public static func < (lhs: PackedCellCoordinate, rhs: PackedCellCoordinate) -> Bool {
    lhs.rawValue < rhs.rawValue
  }

  public var description: String {
    "\(CellAddress.columnLabel(for: column))\(row + 1)"
  }
}

public struct CellAddress: Sendable, Codable, Equatable, Hashable, CustomStringConvertible {
  public static let maxZeroBasedColumn = 16_383
  public static let maxZeroBasedRow = 1_048_575

  public let column: Int
  public let row: Int

  private enum CodingKeys: String, CodingKey {
    case column
    case row
  }

  public init(column: Int, row: Int) throws {
    guard column >= 0, column <= Self.maxZeroBasedColumn,
      row >= 0, row <= Self.maxZeroBasedRow
    else {
      throw EasyOfficeError.invalidCellAddress("col=\(column), row=\(row)")
    }
    self.column = column
    self.row = row
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    let column = try container.decode(Int.self, forKey: .column)
    let row = try container.decode(Int.self, forKey: .row)
    guard column >= 0, column <= Self.maxZeroBasedColumn,
      row >= 0, row <= Self.maxZeroBasedRow
    else {
      throw DecodingError.dataCorrupted(
        DecodingError.Context(
          codingPath: decoder.codingPath,
          debugDescription: "CellAddress out of ECMA-376 bounds: col=\(column), row=\(row)"
        )
      )
    }
    self.column = column
    self.row = row
  }

  public init(packed: PackedCellCoordinate) {
    self.column = packed.column
    self.row = packed.row
  }

  public var packedCoordinate: PackedCellCoordinate {
    PackedCellCoordinate(row: row, column: column)
  }

  public init(a1Notation: String) throws {
    let utf8 = a1Notation.utf8
    var startIdx = utf8.startIndex
    var endIdx = utf8.endIndex

    while startIdx < endIdx {
      let b = utf8[startIdx]
      if b == 0x20 || b == 0x09 || b == 0x0A || b == 0x0D {
        startIdx = utf8.index(after: startIdx)
      } else {
        break
      }
    }
    while endIdx > startIdx {
      let prev = utf8.index(before: endIdx)
      let b = utf8[prev]
      if b == 0x20 || b == 0x09 || b == 0x0A || b == 0x0D {
        endIdx = prev
      } else {
        break
      }
    }

    guard startIdx < endIdx else {
      throw EasyOfficeError.invalidCellAddress(a1Notation)
    }

    var letterCount = 0
    var digitCount = 0
    var colValue = 0
    var rowOneBased = 0

    var idx = startIdx
    while idx < endIdx {
      let b = utf8[idx]
      idx = utf8.index(after: idx)

      if b == 0x24 {
        continue
      }

      let upperByte = (b >= 0x61 && b <= 0x7A) ? (b - 0x20) : b
      if upperByte >= 0x41 && upperByte <= 0x5A {
        guard digitCount == 0, letterCount < 3 else {
          throw EasyOfficeError.invalidCellAddress(a1Notation)
        }
        letterCount += 1
        let charIndex = Int(upperByte - 0x41) + 1
        colValue = colValue * 26 + charIndex
      } else if b >= 0x30 && b <= 0x39 {
        guard letterCount > 0 else {
          throw EasyOfficeError.invalidCellAddress(a1Notation)
        }
        digitCount += 1
        let digit = Int(b - 0x30)
        let (mulVal, mulOverflow) = rowOneBased.multipliedReportingOverflow(by: 10)
        let (addVal, addOverflow) = mulVal.addingReportingOverflow(digit)
        guard !mulOverflow, !addOverflow else {
          throw EasyOfficeError.invalidCellAddress(a1Notation)
        }
        rowOneBased = addVal
      } else {
        throw EasyOfficeError.invalidCellAddress(a1Notation)
      }
    }

    let zeroBasedCol = colValue - 1
    guard letterCount >= 1, letterCount <= 3,
      digitCount >= 1,
      zeroBasedCol >= 0, zeroBasedCol <= Self.maxZeroBasedColumn,
      rowOneBased >= 1, (rowOneBased - 1) <= Self.maxZeroBasedRow
    else {
      throw EasyOfficeError.invalidCellAddress(a1Notation)
    }

    self.column = zeroBasedCol
    self.row = rowOneBased - 1
  }

  public static func parseColumnOnly(_ raw: String) -> Int? {
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return nil }
    var bytes = Array(trimmed.utf8)
    if bytes.first == 0x24 {
      bytes.removeFirst()
    }
    guard (1...3).contains(bytes.count) else { return nil }
    var colVal = 0
    for b in bytes {
      let upper = (b >= 0x61 && b <= 0x7A) ? (b - 0x20) : b
      guard upper >= 0x41 && upper <= 0x5A else { return nil }
      colVal = colVal * 26 + Int(upper - 0x41) + 1
    }
    let zeroBased = colVal - 1
    guard zeroBased >= 0 && zeroBased <= maxZeroBasedColumn else { return nil }
    return zeroBased
  }

  public static func parseRowOnly(_ raw: String) -> Int? {
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return nil }
    var bytes = Array(trimmed.utf8)
    if bytes.first == 0x24 {
      bytes.removeFirst()
    }
    guard (1...7).contains(bytes.count), bytes[0] != 0x30 else { return nil }
    var rowOneBased = 0
    for b in bytes {
      guard b >= 0x30 && b <= 0x39 else { return nil }
      rowOneBased = rowOneBased * 10 + Int(b - 0x30)
    }
    let zeroBased = rowOneBased - 1
    guard zeroBased >= 0 && zeroBased <= maxZeroBasedRow else { return nil }
    return zeroBased
  }

  public static func columnLabel(for zeroBasedColumn: Int) -> String {
    guard zeroBasedColumn >= 0 else { return "A" }
    var index = zeroBasedColumn + 1
    var buffer: (UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8) = (0, 0, 0, 0, 0, 0, 0, 0)
    var writeIdx = 8

    return withUnsafeMutableBytes(of: &buffer) { rawBuf in
      while index > 0 && writeIdx > 0 {
        let remainder = (index - 1) % 26
        writeIdx -= 1
        rawBuf[writeIdx] = UInt8(0x41 + remainder)
        index = (index - 1) / 26
      }
      let slice = UnsafeRawBufferPointer(rebasing: rawBuf[writeIdx..<8])
      return String(decoding: slice, as: UTF8.self)
    }
  }

  public var a1Notation: String {
    "\(Self.columnLabel(for: column))\(row + 1)"
  }

  public var description: String { a1Notation }
}

public struct CellRange: Sendable, Codable, Equatable, Hashable, CustomStringConvertible {
  public static let maxExpandedCellCount = 100_000

  public let start: CellAddress
  public let end: CellAddress

  private enum CodingKeys: String, CodingKey {
    case start
    case end
  }

  public init(start: CellAddress, end: CellAddress) {
    self.start = start
    self.end = end
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    let start = try container.decode(CellAddress.self, forKey: .start)
    let end = try container.decode(CellAddress.self, forKey: .end)
    let rowSpan = abs(end.row - start.row) + 1
    let colSpan = abs(end.column - start.column) + 1
    let (area, overflow) = rowSpan.multipliedReportingOverflow(by: colSpan)
    guard !overflow, area <= Self.maxExpandedCellCount else {
      throw DecodingError.dataCorrupted(
        DecodingError.Context(
          codingPath: decoder.codingPath,
          debugDescription:
            "CellRange span (\(rowSpan)x\(colSpan)) exceeds maxExpandedCellCount (\(Self.maxExpandedCellCount))"
        )
      )
    }
    self.start = start
    self.end = end
  }

  public init(notation: String) throws {
    let parts = notation.split(separator: ":", omittingEmptySubsequences: false).map(String.init)
    if parts.count == 1 {
      let addr = try CellAddress(a1Notation: parts[0])
      self.start = addr
      self.end = addr
    } else if parts.count == 2 {
      if let c1 = CellAddress.parseColumnOnly(parts[0]),
        let c2 = CellAddress.parseColumnOnly(parts[1])
      {
        guard abs(c2 - c1) + 1 <= 256 else {
          throw EasyOfficeError.invalidCellRange(notation)
        }
        self.start = try CellAddress(column: c1, row: 0)
        self.end = try CellAddress(column: c2, row: CellAddress.maxZeroBasedRow)
        return
      }
      if let r1 = CellAddress.parseRowOnly(parts[0]),
        let r2 = CellAddress.parseRowOnly(parts[1])
      {
        guard abs(r2 - r1) + 1 <= 256 else {
          throw EasyOfficeError.invalidCellRange(notation)
        }
        self.start = try CellAddress(column: 0, row: r1)
        self.end = try CellAddress(column: CellAddress.maxZeroBasedColumn, row: r2)
        return
      }
      self.start = try CellAddress(a1Notation: parts[0])
      self.end = try CellAddress(a1Notation: parts[1])
    } else {
      throw EasyOfficeError.invalidCellRange(notation)
    }
    let rowSpan = abs(end.row - start.row) + 1
    let colSpan = abs(end.column - start.column) + 1
    guard rowSpan * colSpan <= Self.maxExpandedCellCount || isWholeColumnOrRow else {
      throw EasyOfficeError.invalidCellRange(notation)
    }
  }

  public init(a1Range: String) throws {
    try self.init(notation: a1Range)
  }

  public var isWholeColumnOrRow: Bool {
    let minRow = min(start.row, end.row)
    let maxRow = max(start.row, end.row)
    let minCol = min(start.column, end.column)
    let maxCol = max(start.column, end.column)
    return (minRow == 0 && maxRow == CellAddress.maxZeroBasedRow && (maxCol - minCol + 1) <= 256)
      || (minCol == 0 && maxCol == CellAddress.maxZeroBasedColumn && (maxRow - minRow + 1) <= 256)
  }

  public var description: String {
    start == end ? start.a1Notation : "\(start.a1Notation):\(end.a1Notation)"
  }

  @inlinable
  public func forEachCoordinate(_ body: (PackedCellCoordinate) -> Void) {
    let minRow = min(start.row, end.row)
    let maxRow = max(start.row, end.row)
    let minCol = min(start.column, end.column)
    let maxCol = max(start.column, end.column)
    let count = (maxRow - minRow + 1) * (maxCol - minCol + 1)
    guard count > 0, count <= Self.maxExpandedCellCount else { return }
    for r in minRow...maxRow {
      for c in minCol...maxCol {
        body(PackedCellCoordinate(row: r, column: c))
      }
    }
  }

  @inlinable
  public func contains(_ coord: PackedCellCoordinate) -> Bool {
    let minRow = min(start.row, end.row)
    let maxRow = max(start.row, end.row)
    let minCol = min(start.column, end.column)
    let maxCol = max(start.column, end.column)
    return coord.row >= minRow && coord.row <= maxRow && coord.column >= minCol
      && coord.column <= maxCol
  }

  @inlinable
  public func overlaps(_ other: CellRange) -> Bool {
    let aMinR = min(start.row, end.row)
    let aMaxR = max(start.row, end.row)
    let aMinC = min(start.column, end.column)
    let aMaxC = max(start.column, end.column)
    let bMinR = min(other.start.row, other.end.row)
    let bMaxR = max(other.start.row, other.end.row)
    let bMinC = min(other.start.column, other.end.column)
    let bMaxC = max(other.start.column, other.end.column)
    return aMinR <= bMaxR && aMaxR >= bMinR && aMinC <= bMaxC && aMaxC >= bMinC
  }

  public func expandedAddresses() -> [CellAddress] {
    let minRow = min(start.row, end.row)
    let maxRow = max(start.row, end.row)
    let minCol = min(start.column, end.column)
    let maxCol = max(start.column, end.column)
    let count = (maxRow - minRow + 1) * (maxCol - minCol + 1)
    guard count > 0, count <= Self.maxExpandedCellCount else { return [] }

    var list: [CellAddress] = []
    list.reserveCapacity(count)
    forEachCoordinate { coord in
      list.append(CellAddress(packed: coord))
    }
    return list
  }
}

public enum FormulaError: String, Sendable, Codable, Equatable {
  case divByZero = "#DIV/0!"
  case value = "#VALUE!"
  case name = "#NAME?"
  case ref = "#REF!"
  case na = "#N/A"
  case num = "#NUM!"
  case cycle = "#CYCLE!"

  public static let divZero: FormulaError = .divByZero
}

public indirect enum FormulaAST: Sendable, Equatable {
  case empty
  case missing
  case literalNumber(Double)
  case literalText(String)
  case literalBoolean(Bool)
  case number(Double)
  case text(String)
  case boolean(Bool)
  case error(FormulaError)
  case cellRef(PackedCellCoordinate)
  case rangeRef(CellRange)
  case namedRangeRef(String)
  case sheetCellRef(sheetName: String, PackedCellCoordinate)
  case sheetRangeRef(sheetName: String, CellRange)
  case unaryMinus(FormulaAST)
  case binaryOp(op: String, lhs: FormulaAST, rhs: FormulaAST)
  case comparisonOp(op: String, lhs: FormulaAST, rhs: FormulaAST)
  case functionCall(name: String, args: [FormulaAST])

  var asCellRange: CellRange? {
    switch self {
    case .rangeRef(let range), .sheetRangeRef(_, let range):
      return range
    case .cellRef(let coord), .sheetCellRef(_, let coord):
      let addr = CellAddress(packed: coord)
      return CellRange(start: addr, end: addr)
    default:
      return nil
    }
  }

  var targetSheetName: String? {
    switch self {
    case .sheetCellRef(let sheetName, _), .sheetRangeRef(let sheetName, _):
      return sheetName
    default:
      return nil
    }
  }
}

public enum EvaluatedCellValue: Sendable, Codable, Equatable, CustomStringConvertible {
  case empty
  case number(Double)
  case text(String)
  case boolean(Bool)
  case error(String)

  public static func error(_ formulaError: FormulaError) -> EvaluatedCellValue {
    .error(formulaError.rawValue)
  }

  public var description: String {
    switch self {
    case .empty:
      return ""
    case .number(let val):
      return SheetValueCoercion.formatGeneralNumber(val)
    case .text(let str):
      return str
    case .boolean(let flag):
      return flag ? "TRUE" : "FALSE"
    case .error(let code):
      return code
    }
  }

  public var numericValue: Double? {
    switch self {
    case .number(let val):
      return val.isFinite ? (val == 0.0 ? 0.0 : val) : nil
    case .boolean(let flag):
      return flag ? 1.0 : 0.0
    case .text(let str):
      return SheetValueCoercion.parseValueText(str)
    case .empty, .error:
      return nil
    }
  }

  public var booleanValue: Bool? {
    if case .error = self { return nil }
    if case .success(let b) = SheetValueCoercion.coerceToBool(self) {
      return b
    }
    return nil
  }
}

public typealias SheetNumberFormat = CellNumberFormat

public enum CellNumberFormat: String, Sendable, Codable, Equatable, CaseIterable {
  case general
  case integer
  case decimal2
  case thousands
  case currencyKRW
  case currencyUSD
  case percentage
  case scientific
  case dateISO

  public static let decimalTwo: CellNumberFormat = .decimal2

  public var displayTitle: String {
    switch self {
    case .general: return "일반"
    case .integer: return "정수"
    case .decimal2: return "소수점 2자리"
    case .thousands: return "천 단위 구분"
    case .currencyKRW: return "통화 (₩)"
    case .currencyUSD: return "통화 ($)"
    case .percentage: return "백분율 (%)"
    case .scientific: return "지수 표기"
    case .dateISO: return "날짜 (ISO)"
    }
  }

  public func format(_ value: Double) -> String {
    format(.number(value))
  }

  public func format(_ value: EvaluatedCellValue) -> String {
    guard case .number(let num) = value, num.isFinite else {
      return value.description
    }
    switch self {
    case .general:
      return SheetValueCoercion.formatGeneralNumber(num)
    case .integer:
      return Self.formatGroupedNumber(num.rounded(), fractionDigits: 0)
    case .decimal2:
      return Self.formatGroupedNumber(num, fractionDigits: 2)
    case .thousands:
      let frac = num.rounded() == num ? 0 : 2
      return Self.formatGroupedNumber(num, fractionDigits: frac)
    case .currencyKRW:
      let sign = num < 0 ? "-" : ""
      return "\(sign)₩\(Self.formatGroupedNumber(abs(num.rounded()), fractionDigits: 0))"
    case .currencyUSD:
      let sign = num < 0 ? "-" : ""
      return "\(sign)$\(Self.formatGroupedNumber(abs(num), fractionDigits: 2))"
    case .percentage:
      let pct = num * 100.0
      if pct.rounded() == pct {
        return String(format: "%.0f%%", pct)
      }
      return String(format: "%.1f%%", pct)
    case .scientific:
      return String(format: "%.2E", num)
    case .dateISO:
      return SheetValueCoercion.formatDateISO(num) ?? value.description
    }
  }

  internal static func formatGroupedNumber(_ value: Double, fractionDigits: Int) -> String {
    let isNegative = value < 0
    let absVal = abs(value)
    let rawFormatted = String(format: "%.\(max(0, fractionDigits))f", absVal)
    let parts = rawFormatted.split(separator: ".", omittingEmptySubsequences: false)
    let intSub = parts.first ?? "0"
    let utf8Digits = intSub.utf8
    let digitCount = utf8Digits.count
    var groupedBytes: [UInt8] = []
    groupedBytes.reserveCapacity(digitCount + digitCount / 3)
    for (idx, byte) in utf8Digits.enumerated() {
      if idx > 0 && (digitCount - idx) % 3 == 0 {
        groupedBytes.append(0x2C)
      }
      groupedBytes.append(byte)
    }
    let groupedInt = String(decoding: groupedBytes, as: UTF8.self)
    let signPrefix = (isNegative && groupedInt != "0") ? "-" : ""
    if fractionDigits > 0, parts.count > 1 {
      return "\(signPrefix)\(groupedInt).\(parts[1])"
    }
    return "\(signPrefix)\(groupedInt)"
  }
}

public typealias SheetHorizontalAlignment = CellHorizontalAlignment

public enum CellHorizontalAlignment: String, Sendable, Codable, Equatable, CaseIterable {
  case leading
  case center
  case trailing

  public var displayTitle: String {
    switch self {
    case .leading: return "왼쪽 정렬"
    case .center: return "가운데 정렬"
    case .trailing: return "오른쪽 정렬"
    }
  }
}

public struct CellStyle: Sendable, Codable, Equatable {
  public var isBold: Bool
  public var isItalic: Bool
  public var isUnderlined: Bool
  public var alignment: CellHorizontalAlignment
  public var numberFormat: CellNumberFormat
  public var fillHex: String?
  public var textHex: String?
  public var wrapText: Bool
  public var decimalPlaces: Int? {
    didSet {
      if let dp = decimalPlaces {
        let clamped = max(0, min(6, dp))
        if decimalPlaces != clamped {
          decimalPlaces = clamped
        }
      }
    }
  }

  public init(
    isBold: Bool = false,
    isItalic: Bool = false,
    isUnderlined: Bool = false,
    alignment: CellHorizontalAlignment = .leading,
    numberFormat: CellNumberFormat = .general,
    fillHex: String? = nil,
    textHex: String? = nil,
    wrapText: Bool = false,
    decimalPlaces: Int? = nil
  ) {
    self.isBold = isBold
    self.isItalic = isItalic
    self.isUnderlined = isUnderlined
    self.alignment = alignment
    self.numberFormat = numberFormat
    self.fillHex = fillHex
    self.textHex = textHex
    self.wrapText = wrapText
    self.decimalPlaces = decimalPlaces.map { max(0, min(6, $0)) }
  }

  private enum CodingKeys: String, CodingKey {
    case isBold
    case isItalic
    case isUnderlined
    case alignment
    case numberFormat
    case fillHex
    case textHex
    case wrapText
    case decimalPlaces
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    self.isBold = try container.decodeIfPresent(Bool.self, forKey: .isBold) ?? false
    self.isItalic = try container.decodeIfPresent(Bool.self, forKey: .isItalic) ?? false
    self.isUnderlined = try container.decodeIfPresent(Bool.self, forKey: .isUnderlined) ?? false
    self.alignment =
      try container.decodeIfPresent(CellHorizontalAlignment.self, forKey: .alignment) ?? .leading
    self.numberFormat =
      try container.decodeIfPresent(CellNumberFormat.self, forKey: .numberFormat) ?? .general
    self.fillHex = try container.decodeIfPresent(String.self, forKey: .fillHex)
    self.textHex = try container.decodeIfPresent(String.self, forKey: .textHex)
    self.wrapText = try container.decodeIfPresent(Bool.self, forKey: .wrapText) ?? false
    self.decimalPlaces =
      try container.decodeIfPresent(Int.self, forKey: .decimalPlaces).map { max(0, min(6, $0)) }
  }

  public func formatNumber(_ value: Double) -> String {
    guard value.isFinite else {
      return EvaluatedCellValue.number(value).description
    }
    guard let dp = decimalPlaces else {
      return numberFormat.format(.number(value))
    }
    let clamped = max(0, min(6, dp))
    switch numberFormat {
    case .decimal2, .integer, .thousands:
      return CellNumberFormat.formatGroupedNumber(value, fractionDigits: clamped)
    case .currencyKRW:
      let sign = value < 0 ? "-" : ""
      return "\(sign)₩\(CellNumberFormat.formatGroupedNumber(abs(value), fractionDigits: clamped))"
    case .currencyUSD:
      let sign = value < 0 ? "-" : ""
      return "\(sign)$\(CellNumberFormat.formatGroupedNumber(abs(value), fractionDigits: clamped))"
    case .percentage:
      return String(format: "%.\(clamped)f%%", value * 100.0)
    case .scientific:
      return String(format: "%.\(clamped)E", value)
    case .dateISO:
      return numberFormat.format(.number(value))
    case .general:
      return String(format: "%.\(clamped)f", value)
    }
  }

  public func format(_ value: EvaluatedCellValue) -> String {
    if case .number(let num) = value, num.isFinite {
      return formatNumber(num)
    }
    return numberFormat.format(value)
  }
}

public struct WorksheetPageSetup: Sendable, Codable, Equatable {
  public var paperSize: PaperSizePreset
  public var orientation: PageOrientation
  public var isMultiColumnPrint: Bool
  public var marginPresetName: String

  public init(
    paperSize: PaperSizePreset = .a4,
    orientation: PageOrientation = .portrait,
    isMultiColumnPrint: Bool = false,
    marginPresetName: String = "기본 (Normal)"
  ) {
    self.paperSize = paperSize
    self.orientation = orientation
    self.isMultiColumnPrint = isMultiColumnPrint
    self.marginPresetName = marginPresetName
  }
}

public struct WorksheetSheetPage: Sendable, Codable, Equatable, Identifiable {
  public let id: UUID
  public var name: String
  public var cellsByCoordinate: [PackedCellCoordinate: String]
  public var cellStyles: [PackedCellCoordinate: CellStyle]
  public var sheetComments: [PackedCellCoordinate: String]
  public var mergedRanges: [CellRange]
  public var columnWidths: [Int: Double]
  public var rowHeights: [Int: Double]
  public var conditionalFormatRules: [SheetConditionalFormatRule]
  public var dataValidationRule: SheetDataValidationRule?

  public init(
    id: UUID = UUID(),
    name: String,
    cellsByCoordinate: [PackedCellCoordinate: String] = [:],
    cellStyles: [PackedCellCoordinate: CellStyle] = [:],
    sheetComments: [PackedCellCoordinate: String] = [:],
    mergedRanges: [CellRange] = [],
    columnWidths: [Int: Double] = [:],
    rowHeights: [Int: Double] = [:],
    conditionalFormatRules: [SheetConditionalFormatRule] = [],
    dataValidationRule: SheetDataValidationRule? = nil
  ) {
    self.id = id
    self.name = name
    self.cellsByCoordinate = cellsByCoordinate
    self.cellStyles = cellStyles
    self.sheetComments = sheetComments
    self.mergedRanges = mergedRanges
    self.columnWidths = columnWidths
    self.rowHeights = rowHeights
    self.conditionalFormatRules = conditionalFormatRules
    self.dataValidationRule = dataValidationRule
  }

  private enum CodingKeys: String, CodingKey {
    case id
    case name
    case cellsByCoordinate
    case cellStyles
    case sheetComments
    case mergedRanges
    case columnWidths
    case rowHeights
    case conditionalFormatRules
    case dataValidationRule
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    self.id = try container.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
    self.name = try container.decode(String.self, forKey: .name)
    self.cellsByCoordinate =
      try container.decodeIfPresent([PackedCellCoordinate: String].self, forKey: .cellsByCoordinate)
      ?? [:]
    self.cellStyles =
      try container.decodeIfPresent([PackedCellCoordinate: CellStyle].self, forKey: .cellStyles)
      ?? [:]
    self.sheetComments =
      try container.decodeIfPresent([PackedCellCoordinate: String].self, forKey: .sheetComments)
      ?? [:]
    self.mergedRanges = try container.decodeIfPresent([CellRange].self, forKey: .mergedRanges) ?? []
    self.columnWidths =
      try container.decodeIfPresent([Int: Double].self, forKey: .columnWidths) ?? [:]
    self.rowHeights = try container.decodeIfPresent([Int: Double].self, forKey: .rowHeights) ?? [:]
    self.conditionalFormatRules =
      try container.decodeIfPresent(
        [SheetConditionalFormatRule].self, forKey: .conditionalFormatRules) ?? []
    self.dataValidationRule =
      try container.decodeIfPresent(SheetDataValidationRule.self, forKey: .dataValidationRule)
  }
}
