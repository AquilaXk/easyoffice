import Foundation

/// Bit-packed 64-bit spreadsheet cell coordinate (`row` in upper 32 bits, `column` in lower 32 bits).
/// Enables zero-allocation O(1) dictionary lookups and natural row-major sorting via single `UInt64` comparison.
public struct PackedCellCoordinate: Sendable, Codable, Equatable, Hashable, Comparable,
  CustomStringConvertible
{
  public let rawValue: UInt64

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

/// Zero-indexed spreadsheet cell coordinate with zero-allocation UTF-8 A1 notation parser and stack-buffer formatter.
public struct CellAddress: Sendable, Codable, Equatable, Hashable, CustomStringConvertible {
  /// ECMA-376 SpreadsheetML maximum 0-based column index (`XFD` = 16,383).
  public static let maxZeroBasedColumn = 16_383
  /// ECMA-376 SpreadsheetML maximum 0-based row index (1,048,576 rows -> 1,048,575).
  public static let maxZeroBasedRow = 1_048_575

  public let column: Int  // 0 = A, 25 = Z, 26 = AA
  public let row: Int  // 0 = row 1

  public init(column: Int, row: Int) throws {
    guard column >= 0, column <= Self.maxZeroBasedColumn,
      row >= 0, row <= Self.maxZeroBasedRow
    else {
      throw EasyOfficeError.invalidCellAddress("col=\(column), row=\(row)")
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

      if b == 0x24 {  // '$'
        continue
      }

      let upperByte = (b >= 0x61 && b <= 0x7A) ? (b - 0x20) : b
      if upperByte >= 0x41 && upperByte <= 0x5A {  // 'A'...'Z'
        guard digitCount == 0, letterCount < 3 else {
          throw EasyOfficeError.invalidCellAddress(a1Notation)
        }
        letterCount += 1
        let charIndex = Int(upperByte - 0x41) + 1
        colValue = colValue * 26 + charIndex
      } else if b >= 0x30 && b <= 0x39 {  // '0'...'9'
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

  /// Formats a 0-based column index into its A1 column label (`A`..`XFD`) using a fixed 8-byte stack tuple.
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

/// Rectangular spreadsheet range (e.g. `A1:C4`).
public struct CellRange: Sendable, Codable, Equatable, Hashable, CustomStringConvertible {
  /// Maximum number of cells expanded in a single formula range evaluation to prevent memory exhaustion.
  public static let maxExpandedCellCount = 100_000

  public let start: CellAddress
  public let end: CellAddress

  public init(start: CellAddress, end: CellAddress) {
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
      self.start = try CellAddress(a1Notation: parts[0])
      self.end = try CellAddress(a1Notation: parts[1])
    } else {
      throw EasyOfficeError.invalidCellRange(notation)
    }
    let rowSpan = abs(end.row - start.row) + 1
    let colSpan = abs(end.column - start.column) + 1
    guard rowSpan * colSpan <= Self.maxExpandedCellCount else {
      throw EasyOfficeError.invalidCellRange(notation)
    }
  }

  public init(a1Range: String) throws {
    try self.init(notation: a1Range)
  }

  public var description: String {
    start == end ? start.a1Notation : "\(start.a1Notation):\(end.a1Notation)"
  }

  /// Zero-allocation coordinate iterator in row-major order.
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

  /// Expands the range in row-major order.
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

/// Standard spreadsheet formula error codes.
public enum FormulaError: String, Sendable, Codable, Equatable {
  case divByZero = "#DIV/0!"
  case value = "#VALUE!"
  case name = "#NAME?"
  case ref = "#REF!"
  case na = "#N/A"
  case num = "#NUM!"

  public static let divZero: FormulaError = .divByZero
}

/// Pre-compiled Abstract Syntax Tree (AST) for spreadsheet cell values and formulas.
public indirect enum FormulaAST: Sendable, Equatable {
  case empty
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

/// Evaluated cell value in a worksheet.
public enum EvaluatedCellValue: Sendable, Codable, Equatable, CustomStringConvertible {
  case empty
  case number(Double)
  case text(String)
  case boolean(Bool)
  case error(String)  // e.g. #DIV/0!, #VALUE!, #NAME?, #REF!, #N/A, #NUM!

  public static func error(_ formulaError: FormulaError) -> EvaluatedCellValue {
    .error(formulaError.rawValue)
  }

  public var description: String {
    switch self {
    case .empty:
      return ""
    case .number(let val):
      if val.rounded() == val {
        return String(format: "%.0f", val)
      }
      return String(format: "%.4g", val)
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
    case .number(let val): return val
    case .boolean(let flag): return flag ? 1.0 : 0.0
    case .text(let str): return Double(str.trimmingCharacters(in: .whitespaces))
    case .empty, .error: return nil
    }
  }

  public var booleanValue: Bool? {
    switch self {
    case .boolean(let flag): return flag
    case .number(let val): return val != 0.0
    case .text(let str):
      if str.caseInsensitiveCompare("TRUE") == .orderedSame { return true }
      if str.caseInsensitiveCompare("FALSE") == .orderedSame { return false }
      return nil
    case .empty: return false
    case .error: return nil
    }
  }
}

// MARK: - Cell Style, Number Format, Page Setup, and Multi-Sheet Models

/// Display number format applied to a spreadsheet cell.
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

  public func format(_ value: Double) -> String {
    format(.number(value))
  }

  public func format(_ value: EvaluatedCellValue) -> String {
    guard case .number(let num) = value, num.isFinite else {
      return value.description
    }
    switch self {
    case .general:
      return value.description
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
      guard let intVal = WorksheetModel.safeInt(num.rounded()) else {
        return value.description
      }
      if intVal >= 19_000_101 && intVal <= 29_991_231 {
        let y = intVal / 10000
        let m = (intVal % 10000) / 100
        let d = intVal % 100
        return String(format: "%04d-%02d-%02d", y, m, d)
      }
      if let baseDate = WorksheetModel.makeUTCDate(year: 1899, month: 12, day: 30),
        let target = WorksheetModel.utcCalendar.date(byAdding: .day, value: intVal, to: baseDate)
      {
        let comps = WorksheetModel.utcCalendar.dateComponents([.year, .month, .day], from: target)
        return String(
          format: "%04d-%02d-%02d", comps.year ?? 1970, comps.month ?? 1, comps.day ?? 1)
      }
      return value.description
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
        groupedBytes.append(0x2C)  // ','
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

/// Horizontal text alignment within a spreadsheet cell.
public enum CellHorizontalAlignment: String, Sendable, Codable, Equatable, CaseIterable {
  case leading
  case center
  case trailing
}

/// Per-cell visual style and number formatting descriptor.
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

/// Page setup and print/margin configuration for a worksheet.
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

/// Additional worksheet page stored within a multi-sheet workbook.
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

// MARK: - FormulaAST Compiler

extension WorksheetModel {

  public static func compileCellInput(_ raw: String) -> FormulaAST {
    guard !raw.isEmpty else { return .empty }
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    guard trimmed.hasPrefix("=") else {
      if let num = Double(trimmed), num.isFinite {
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
    let expression = String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces)
    return compileExpression(expression)
  }

  public static func compileExpression(_ expr: String) -> FormulaAST {
    let trimmed = expr.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return .empty }

    // 1. Top-level comparison: >=, <=, <>, =, >, <
    if let cmpAST = compileTopLevelComparison(trimmed) {
      return cmpAST
    }

    // 2. Binary arithmetic: +, -, *, /
    if let arithAST = compileBinaryArithmetic(trimmed) {
      return arithAST
    }

    // 3. Parenthesized sub-expression: (expr)
    if trimmed.hasPrefix("(") && trimmed.hasSuffix(")")
      && matchingCloseIndex(in: trimmed, openAt: trimmed.startIndex)
        == trimmed.index(before: trimmed.endIndex)
    {
      let inner = String(trimmed.dropFirst().dropLast())
      return compileExpression(inner)
    }

    // 4. Function call: FUNCNAME(args...)
    if let openParen = trimmed.firstIndex(of: "("),
      trimmed.hasSuffix(")"),
      matchingCloseIndex(in: trimmed, openAt: openParen) == trimmed.index(before: trimmed.endIndex)
    {
      let funcName = String(trimmed[..<openParen]).trimmingCharacters(in: .whitespaces).uppercased()
      if !funcName.isEmpty
        && funcName.unicodeScalars.allSatisfy({ CharacterSet.alphanumerics.contains($0) })
      {
        let innerStart = trimmed.index(after: openParen)
        let innerEnd = trimmed.index(before: trimmed.endIndex)
        let argString = innerStart <= innerEnd ? String(trimmed[innerStart..<innerEnd]) : ""
        let rawArgs = splitTopLevelArguments(argString)
        let compiledArgs = rawArgs.map { arg -> FormulaAST in
          compileExpression(arg.trimmingCharacters(in: .whitespacesAndNewlines))
        }
        return .functionCall(name: funcName, args: compiledArgs)
      }
    }

    // 5. Quoted string literal
    if trimmed.hasPrefix("\"") && trimmed.hasSuffix("\"") && trimmed.count >= 2 {
      let start = trimmed.index(after: trimmed.startIndex)
      let end = trimmed.index(before: trimmed.endIndex)
      return .text(String(trimmed[start..<end]))
    }

    // 6. Boolean literal
    if trimmed.caseInsensitiveCompare("TRUE") == .orderedSame {
      return .boolean(true)
    }
    if trimmed.caseInsensitiveCompare("FALSE") == .orderedSame {
      return .boolean(false)
    }

    // 7. Numeric literal
    if let number = Double(trimmed) {
      guard number.isFinite else { return .error(.num) }
      return .number(number)
    }

    // 8. Unary plus / minus on non-literal expression
    if trimmed.hasPrefix("+") && trimmed.count > 1 {
      let inner = String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces)
      if !inner.isEmpty {
        return compileExpression(inner)
      }
    }
    if trimmed.hasPrefix("-") && trimmed.count > 1 {
      let inner = String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces)
      if !inner.isEmpty {
        return .unaryMinus(compileExpression(inner))
      }
    }

    // 9. Postfix percentage operator (expr%)
    if trimmed.hasSuffix("%") && trimmed.count > 1 && !trimmed.hasPrefix("\"") {
      let inner = String(trimmed.dropLast()).trimmingCharacters(in: .whitespaces)
      if !inner.isEmpty {
        return .binaryOp(op: "/", lhs: compileExpression(inner), rhs: .number(100.0))
      }
    }

    // 9. Cross-sheet reference (e.g. Sheet2!A1, 'Summary Sheet'!B2:D10)
    if let crossSheetRef = parseCrossSheetReference(trimmed) {
      return crossSheetRef
    }

    // 10. Same-sheet range reference (e.g. A1:B5)
    if trimmed.contains(":") && !trimmed.contains("(") && !trimmed.contains("\"") {
      if let range = try? CellRange(notation: trimmed) {
        return .rangeRef(range)
      }
      return .error(.ref)
    }

    // 11. Direct cell reference
    if let addr = try? CellAddress(a1Notation: trimmed) {
      return .cellRef(addr.packedCoordinate)
    }

    // 12. Named range identifier
    if isValidNamedRangeIdentifier(trimmed) {
      return .namedRangeRef(trimmed)
    }

    return .error(.value)
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

  private static func compileTopLevelComparison(_ expr: String) -> FormulaAST? {
    let utf8 = expr.utf8
    var parenDepth = 0
    var inDoubleQuotes = false
    var inSingleQuotes = false
    var idx = utf8.startIndex
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
            guard !leftStr.isEmpty, !rightStr.isEmpty else { return nil }
            return .comparisonOp(
              op: op,
              lhs: compileExpression(leftStr),
              rhs: compileExpression(rightStr)
            )
          }
        }
      }
      idx = utf8.index(after: idx)
    }
    return nil
  }

  private static func compileBinaryArithmetic(_ expr: String) -> FormulaAST? {
    let utf8 = expr.utf8
    guard !utf8.isEmpty else { return nil }
    let operatorPasses: [(UInt8, UInt8)] = [
      (0x2B, 0x2D),  // '+', '-'
      (0x2A, 0x2F),  // '*', '/'
    ]
    for (opA, opB) in operatorPasses {
      var parenDepth = 0
      var inDoubleQuotes = false
      var inSingleQuotes = false
      var idx = utf8.endIndex
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
          let rhsStr = String(expr[nextIdx..<expr.endIndex]).trimmingCharacters(in: .whitespaces)
          if lhsStr.isEmpty || rhsStr.isEmpty { continue }
          if let prevByte = lhsStr.utf8.last,
            prevByte == 0x2B || prevByte == 0x2D || prevByte == 0x2A || prevByte == 0x2F
              || prevByte == 0x28 || prevByte == 0x2C || prevByte == 0x3D || prevByte == 0x3C
              || prevByte == 0x3E
          {
            continue
          }
          let opStr = b == 0x2B ? "+" : (b == 0x2D ? "-" : (b == 0x2A ? "*" : "/"))
          return .binaryOp(
            op: opStr,
            lhs: compileExpression(lhsStr),
            rhs: compileExpression(rhsStr)
          )
        }
      }
    }
    return nil
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
}
