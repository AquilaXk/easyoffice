import Foundation

public typealias SheetEngine = WorksheetModel

public struct WorksheetModel: Sendable, Codable, Equatable, Identifiable {
  public static let maxCellTextLength = 32_767
  public static let maxReptOutputCharacters = maxCellTextLength
  public static let maxFormulaRecursionDepth = 256
  public static let maxEvaluationSteps = 2_097_152

  public let id: UUID
  public var name: String
  public internal(set) var cellsByCoordinate: [PackedCellCoordinate: String] = [:]
  public var cellStyles: [PackedCellCoordinate: CellStyle] = [:]
  public var pageSetup: WorksheetPageSetup = WorksheetPageSetup()
  public var sheetComments: [PackedCellCoordinate: String] = [:]
  public var mergedRanges: [CellRange] = []
  public var columnWidths: [Int: Double] = [:]
  public var rowHeights: [Int: Double] = [:]
  public var trackChangesEnabled: Bool = false
  public var additionalSheets: [WorksheetSheetPage] = []
  public var activeSheetIndex: Int = 0
  public var namedRanges: [String: String] = [:]
  public var conditionalFormatRules: [SheetConditionalFormatRule] = []
  public var dataValidationRule: SheetDataValidationRule? = nil
  public var preservedArchiveEntries: [String: Data] = [:]
  internal var compiledASTs: [PackedCellCoordinate: FormulaAST] = [:]

  @inline(__always)
  internal static func exceedsCellTextLength(_ text: String) -> Bool {
    guard text.utf8.count > maxCellTextLength else { return false }
    return text.utf16.count > maxCellTextLength
  }

  public var rawCells: [String: String] {
    get {
      var result: [String: String] = [:]
      result.reserveCapacity(cellsByCoordinate.count)
      for (coord, val) in cellsByCoordinate {
        result[coord.description] = val
      }
      return result
    }
    set {
      cellsByCoordinate.removeAll(keepingCapacity: true)
      compiledASTs.removeAll(keepingCapacity: true)
      for (key, val) in newValue where !val.isEmpty {
        if let addr = try? CellAddress(a1Notation: key) {
          let coord = addr.packedCoordinate
          cellsByCoordinate[coord] = val
          let ast = Self.compileCellInput(val)
          compiledASTs[coord] = ast
          applyAutoDateFormatIfNeeded(for: ast, at: coord)
        }
      }
    }
  }

  public var allSheetNames: [String] {
    var names = additionalSheets.map(\.name)
    let insertionIndex = min(max(0, activeSheetIndex), names.count)
    names.insert(name, at: insertionIndex)
    return names
  }

  public static let supportedFunctionNames: [String] = [
    "ABS", "AND", "AVERAGE", "AVERAGEA", "AVERAGEIF", "AVERAGEIFS",
    "CEILING", "CHAR", "CHOOSE", "CODE", "COLUMN", "COLUMNS", "CONCAT", "CONCATENATE", "COS",
    "COUNT", "COUNTA", "COUNTBLANK", "COUNTIF", "COUNTIFS",
    "DATE", "DAY", "DAYS", "DDB",
    "EDATE", "EXACT", "EXP",
    "FIND", "FLOOR", "FV",
    "HLOOKUP", "HOUR",
    "IF", "IFERROR", "IFNA", "IFS", "INDEX", "INT", "IPMT", "IRR", "ISBLANK", "ISERR", "ISERROR",
    "ISEVEN", "ISLOGICAL", "ISNA", "ISNUMBER", "ISODD", "ISTEXT",
    "LARGE", "LEFT", "LEN", "LN", "LOG", "LOG10", "LOWER",
    "MATCH", "MAX", "MEDIAN", "MID", "MIN", "MINUTE", "MOD", "MODE", "MONTH",
    "NOT", "NPER", "NPV",
    "OFFSET", "OR",
    "PERCENTILE", "PI", "PMT", "POWER", "PPMT", "PRODUCT", "PROPER", "PV",
    "QUARTILE", "QUOTIENT",
    "RAND", "RANK", "RATE", "REPLACE", "REPT", "RIGHT", "ROUND", "ROUNDDOWN", "ROUNDUP", "ROW",
    "ROWS",
    "SEARCH", "SECOND", "SIGN", "SIN", "SLN", "SMALL", "SQRT", "STDEV", "STDEVP", "SUBSTITUTE",
    "SUM", "SUMIF", "SUMIFS", "SUMPRODUCT", "SWITCH",
    "TAN", "TEXT", "TEXTJOIN", "TIME", "TRANSPOSE", "TRIM", "TRUE", "TRUNC",
    "UPPER",
    "VALUE", "VAR", "VARP", "VLOOKUP",
    "XLOOKUP", "XOR",
    "YEAR",
  ].sorted()

  internal static let lazyOrRefFunctions: Set<String> = [
    "IF", "IFS", "IFERROR", "IFNA", "SWITCH", "CHOOSE",
    "OFFSET", "INDIRECT", "SUBTOTAL", "ROW", "COLUMN", "ROWS", "COLUMNS", "TRANSPOSE",
    "ISBLANK", "ISERR", "ISERROR", "ISNA", "ISNUMBER", "ISTEXT", "ISLOGICAL", "ISNONTEXT", "TYPE",
    "N", "T",
    "SUMIF", "COUNTIF", "AVERAGEIF",
    "SUMIFS", "COUNTIFS", "AVERAGEIFS", "MINIFS", "MAXIFS",
    "VLOOKUP", "HLOOKUP", "XLOOKUP", "INDEX", "MATCH",
  ]

  public init(
    id: UUID = UUID(),
    name: String,
    rawCells: [String: String] = [:],
    cellStyles: [PackedCellCoordinate: CellStyle] = [:],
    pageSetup: WorksheetPageSetup = WorksheetPageSetup(),
    sheetComments: [PackedCellCoordinate: String] = [:],
    mergedRanges: [CellRange] = [],
    columnWidths: [Int: Double] = [:],
    rowHeights: [Int: Double] = [:],
    trackChangesEnabled: Bool = false,
    additionalSheets: [WorksheetSheetPage] = [],
    activeSheetIndex: Int = 0,
    namedRanges: [String: String] = [:],
    conditionalFormatRules: [SheetConditionalFormatRule] = [],
    dataValidationRule: SheetDataValidationRule? = nil
  ) {
    self.id = id
    self.name = name
    self.cellStyles = cellStyles
    self.pageSetup = pageSetup
    self.sheetComments = sheetComments
    self.mergedRanges = mergedRanges
    self.columnWidths = columnWidths
    self.rowHeights = rowHeights
    self.trackChangesEnabled = trackChangesEnabled
    self.additionalSheets = additionalSheets
    self.activeSheetIndex = activeSheetIndex
    self.namedRanges = namedRanges
    self.conditionalFormatRules = conditionalFormatRules
    self.dataValidationRule = dataValidationRule
    for (key, val) in rawCells where !val.isEmpty {
      if let addr = try? CellAddress(a1Notation: key) {
        let coord = addr.packedCoordinate
        self.cellsByCoordinate[coord] = val
        let ast = Self.compileCellInput(val)
        self.compiledASTs[coord] = ast
        applyAutoDateFormatIfNeeded(for: ast, at: coord)
      }
    }
  }

  internal init(
    id: UUID,
    name: String,
    cellsByCoordinate: [PackedCellCoordinate: String],
    cellStyles: [PackedCellCoordinate: CellStyle],
    pageSetup: WorksheetPageSetup,
    sheetComments: [PackedCellCoordinate: String],
    mergedRanges: [CellRange],
    columnWidths: [Int: Double] = [:],
    rowHeights: [Int: Double] = [:],
    trackChangesEnabled: Bool,
    additionalSheets: [WorksheetSheetPage],
    activeSheetIndex: Int,
    namedRanges: [String: String] = [:],
    conditionalFormatRules: [SheetConditionalFormatRule] = [],
    dataValidationRule: SheetDataValidationRule? = nil,
    precompileAllCells: Bool = true
  ) {
    self.id = id
    self.name = name
    self.cellsByCoordinate = cellsByCoordinate
    self.cellStyles = cellStyles
    self.pageSetup = pageSetup
    self.sheetComments = sheetComments
    self.mergedRanges = mergedRanges
    self.columnWidths = columnWidths
    self.rowHeights = rowHeights
    self.trackChangesEnabled = trackChangesEnabled
    self.additionalSheets = additionalSheets
    self.activeSheetIndex = activeSheetIndex
    self.namedRanges = namedRanges
    self.conditionalFormatRules = conditionalFormatRules
    self.dataValidationRule = dataValidationRule
    if precompileAllCells {
      for (coord, val) in cellsByCoordinate where !val.isEmpty {
        let ast = Self.compileCellInput(val)
        self.compiledASTs[coord] = ast
        applyAutoDateFormatIfNeeded(for: ast, at: coord)
      }
    }
  }

  private enum CodingKeys: String, CodingKey {
    case id
    case name
    case cellsByCoordinate
    case rawCells
    case cellStyles
    case pageSetup
    case sheetComments
    case mergedRanges
    case columnWidths
    case rowHeights
    case trackChangesEnabled
    case additionalSheets
    case activeSheetIndex
    case namedRanges
    case conditionalFormatRules
    case dataValidationRule
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    self.id = try container.decode(UUID.self, forKey: .id)
    self.name = try container.decode(String.self, forKey: .name)
    self.cellStyles =
      try container.decodeIfPresent([PackedCellCoordinate: CellStyle].self, forKey: .cellStyles)
      ?? [:]
    if let byCoord = try container.decodeIfPresent(
      [PackedCellCoordinate: String].self, forKey: .cellsByCoordinate)
    {
      self.cellsByCoordinate = byCoord
      for (coord, val) in byCoord {
        let ast = Self.compileCellInput(val)
        self.compiledASTs[coord] = ast
        applyAutoDateFormatIfNeeded(for: ast, at: coord)
      }
    } else if let legacyRaw = try container.decodeIfPresent(
      [String: String].self, forKey: .rawCells)
    {
      for (key, val) in legacyRaw where !val.isEmpty {
        if let addr = try? CellAddress(a1Notation: key) {
          let coord = addr.packedCoordinate
          self.cellsByCoordinate[coord] = val
          let ast = Self.compileCellInput(val)
          self.compiledASTs[coord] = ast
          applyAutoDateFormatIfNeeded(for: ast, at: coord)
        }
      }
    }
    self.pageSetup =
      try container.decodeIfPresent(WorksheetPageSetup.self, forKey: .pageSetup)
      ?? WorksheetPageSetup()
    self.sheetComments =
      try container.decodeIfPresent([PackedCellCoordinate: String].self, forKey: .sheetComments)
      ?? [:]
    self.mergedRanges = try container.decodeIfPresent([CellRange].self, forKey: .mergedRanges) ?? []
    self.columnWidths =
      try container.decodeIfPresent([Int: Double].self, forKey: .columnWidths) ?? [:]
    self.rowHeights = try container.decodeIfPresent([Int: Double].self, forKey: .rowHeights) ?? [:]
    self.trackChangesEnabled =
      try container.decodeIfPresent(Bool.self, forKey: .trackChangesEnabled) ?? false
    self.additionalSheets =
      try container.decodeIfPresent([WorksheetSheetPage].self, forKey: .additionalSheets) ?? []
    self.activeSheetIndex = try container.decodeIfPresent(Int.self, forKey: .activeSheetIndex) ?? 0
    self.namedRanges =
      try container.decodeIfPresent([String: String].self, forKey: .namedRanges) ?? [:]
    self.conditionalFormatRules =
      try container.decodeIfPresent(
        [SheetConditionalFormatRule].self, forKey: .conditionalFormatRules) ?? []
    self.dataValidationRule =
      try container.decodeIfPresent(SheetDataValidationRule.self, forKey: .dataValidationRule)
  }

  public func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(id, forKey: .id)
    try container.encode(name, forKey: .name)
    try container.encode(cellsByCoordinate, forKey: .cellsByCoordinate)
    try container.encode(cellStyles, forKey: .cellStyles)
    try container.encode(pageSetup, forKey: .pageSetup)
    try container.encode(sheetComments, forKey: .sheetComments)
    try container.encode(mergedRanges, forKey: .mergedRanges)
    try container.encode(columnWidths, forKey: .columnWidths)
    try container.encode(rowHeights, forKey: .rowHeights)
    try container.encode(trackChangesEnabled, forKey: .trackChangesEnabled)
    try container.encode(additionalSheets, forKey: .additionalSheets)
    try container.encode(activeSheetIndex, forKey: .activeSheetIndex)
    try container.encode(namedRanges, forKey: .namedRanges)
    try container.encode(conditionalFormatRules, forKey: .conditionalFormatRules)
    try container.encodeIfPresent(dataValidationRule, forKey: .dataValidationRule)
  }

  public static func == (lhs: WorksheetModel, rhs: WorksheetModel) -> Bool {
    lhs.id == rhs.id && lhs.name == rhs.name && lhs.cellsByCoordinate == rhs.cellsByCoordinate
      && lhs.cellStyles == rhs.cellStyles && lhs.pageSetup == rhs.pageSetup
      && lhs.sheetComments == rhs.sheetComments && lhs.mergedRanges == rhs.mergedRanges
      && lhs.columnWidths == rhs.columnWidths && lhs.rowHeights == rhs.rowHeights
      && lhs.trackChangesEnabled == rhs.trackChangesEnabled
      && lhs.additionalSheets == rhs.additionalSheets
      && lhs.activeSheetIndex == rhs.activeSheetIndex && lhs.namedRanges == rhs.namedRanges
      && lhs.conditionalFormatRules == rhs.conditionalFormatRules
      && lhs.dataValidationRule == rhs.dataValidationRule
  }

  public mutating func setNamedRange(_ name: String, rangeOrCellNotation: String) {
    let key = name.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
    let val = rangeOrCellNotation.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !key.isEmpty, !val.isEmpty else { return }
    namedRanges[key] = val
  }

  public mutating func removeNamedRange(_ name: String) {
    let key = name.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
    namedRanges.removeValue(forKey: key)
  }

  public func resolveNamedRange(_ name: String) -> String? {
    let key = name.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
    if let exact = namedRanges[key] {
      return exact
    }
    return namedRanges.first(where: { $0.key.caseInsensitiveCompare(key) == .orderedSame })?.value
  }

  public mutating func addSheetPage(
    named sheetName: String,
    initialCells: [PackedCellCoordinate: String] = [:]
  ) {
    let trimmed = sheetName.trimmingCharacters(in: .whitespacesAndNewlines)
    let resolvedName = trimmed.isEmpty ? "Sheet\(allSheetNames.count + 1)" : trimmed
    if !initialCells.isEmpty && name.caseInsensitiveCompare(resolvedName) == .orderedSame {
      cellsByCoordinate = initialCells
      compiledASTs.removeAll(keepingCapacity: true)
      for (coord, rawVal) in cellsByCoordinate where !rawVal.isEmpty {
        let ast = Self.compileCellInput(rawVal)
        compiledASTs[coord] = ast
        applyAutoDateFormatIfNeeded(for: ast, at: coord)
      }
      return
    }
    if let existingIdx = additionalSheets.firstIndex(where: {
      $0.name.caseInsensitiveCompare(resolvedName) == .orderedSame
    }) {
      additionalSheets[existingIdx].cellsByCoordinate = initialCells
      return
    }
    additionalSheets.append(
      WorksheetSheetPage(
        name: resolvedName,
        cellsByCoordinate: initialCells,
        cellStyles: [:],
        sheetComments: [:],
        mergedRanges: [],
        columnWidths: [:],
        rowHeights: [:]
      )
    )
  }

  public mutating func switchActiveSheet(to newIndex: Int) {
    var allPages = additionalSheets
    let currentInsertionIndex = min(max(0, activeSheetIndex), allPages.count)
    let currentActivePage = WorksheetSheetPage(
      name: name,
      cellsByCoordinate: cellsByCoordinate,
      cellStyles: cellStyles,
      sheetComments: sheetComments,
      mergedRanges: mergedRanges,
      columnWidths: columnWidths,
      rowHeights: rowHeights,
      conditionalFormatRules: conditionalFormatRules,
      dataValidationRule: dataValidationRule
    )
    allPages.insert(currentActivePage, at: currentInsertionIndex)
    guard allPages.indices.contains(newIndex), newIndex != currentInsertionIndex else {
      return
    }

    let targetPage = allPages.remove(at: newIndex)
    self.name = targetPage.name
    self.cellsByCoordinate = targetPage.cellsByCoordinate
    self.cellStyles = targetPage.cellStyles
    self.sheetComments = targetPage.sheetComments
    self.mergedRanges = targetPage.mergedRanges
    self.columnWidths = targetPage.columnWidths
    self.rowHeights = targetPage.rowHeights
    self.conditionalFormatRules = targetPage.conditionalFormatRules
    self.dataValidationRule = targetPage.dataValidationRule
    self.additionalSheets = allPages
    self.activeSheetIndex = newIndex

    self.compiledASTs.removeAll(keepingCapacity: true)
    for (coord, rawVal) in self.cellsByCoordinate where !rawVal.isEmpty {
      let ast = Self.compileCellInput(rawVal)
      self.compiledASTs[coord] = ast
      applyAutoDateFormatIfNeeded(for: ast, at: coord)
    }
  }

  public func orderedSheetPages() -> [WorksheetSheetPage] {
    var allPages = additionalSheets
    let currentInsertionIndex = min(max(0, activeSheetIndex), allPages.count)
    let currentActivePage = WorksheetSheetPage(
      id: id,
      name: name,
      cellsByCoordinate: cellsByCoordinate,
      cellStyles: cellStyles,
      sheetComments: sheetComments,
      mergedRanges: mergedRanges,
      columnWidths: columnWidths,
      rowHeights: rowHeights,
      conditionalFormatRules: conditionalFormatRules,
      dataValidationRule: dataValidationRule
    )
    allPages.insert(currentActivePage, at: currentInsertionIndex)
    return allPages
  }

  public mutating func toggleMergeCells(_ range: CellRange) {
    if let existingIdx = mergedRanges.firstIndex(where: { $0 == range || $0.overlaps(range) }) {
      mergedRanges.remove(at: existingIdx)
    } else {
      mergedRanges.append(range)
    }
  }

  public func isCellMerged(_ coord: PackedCellCoordinate) -> Bool {
    mergedRanges.contains { $0.contains(coord) }
  }

  public mutating func setCellStyle(_ style: CellStyle, at a1: String) throws {
    let addr = try CellAddress(a1Notation: a1)
    setCellStyle(style, at: addr.packedCoordinate)
  }

  public mutating func setCellStyle(_ style: CellStyle, at coord: PackedCellCoordinate) {
    if style == CellStyle() {
      cellStyles.removeValue(forKey: coord)
    } else {
      cellStyles[coord] = style
    }
  }

  public func cellStyle(at a1: String) -> CellStyle {
    guard let addr = try? CellAddress(a1Notation: a1) else { return CellStyle() }
    return cellStyles[addr.packedCoordinate] ?? CellStyle()
  }

  public func cellStyle(at coord: PackedCellCoordinate) -> CellStyle {
    cellStyles[coord] ?? CellStyle()
  }

  public func formattedDisplayValue(
    at address: CellAddress,
    using preEvaluated: [PackedCellCoordinate: EvaluatedCellValue]? = nil
  ) -> String {
    let coord = address.packedCoordinate
    let val = preEvaluated?[coord] ?? evaluate(at: coord)
    let style = cellStyles[coord] ?? CellStyle()
    return style.format(val)
  }

  public func detectedDataRange() -> CellRange {
    guard !cellsByCoordinate.isEmpty else {
      let start = CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
      let end = CellAddress(packed: PackedCellCoordinate(row: 4, column: 3))
      return CellRange(start: start, end: end)
    }
    var minRow = Int.max
    var maxRow = 0
    var minCol = Int.max
    var maxCol = 0
    for coord in cellsByCoordinate.keys {
      minRow = min(minRow, coord.row)
      maxRow = max(maxRow, coord.row)
      minCol = min(minCol, coord.column)
      maxCol = max(maxCol, coord.column)
    }
    let start = CellAddress(packed: PackedCellCoordinate(row: minRow, column: minCol))
    let end = CellAddress(packed: PackedCellCoordinate(row: maxRow, column: maxCol))
    return CellRange(start: start, end: end)
  }

  public func detectedNumericRange() -> CellRange {
    var context = EvaluationContext()
    var minRow = Int.max
    var maxRow = -1
    var minCol = Int.max
    var maxCol = -1
    for coord in cellsByCoordinate.keys {
      let val = evaluateInternal(at: coord, context: &context, depth: 0)
      if case .number = val {
        minRow = min(minRow, coord.row)
        maxRow = max(maxRow, coord.row)
        minCol = min(minCol, coord.column)
        maxCol = max(maxCol, coord.column)
      }
    }
    guard maxRow >= minRow, maxCol >= minCol else {
      return detectedDataRange()
    }
    let start = CellAddress(packed: PackedCellCoordinate(row: minRow, column: minCol))
    let end = CellAddress(packed: PackedCellCoordinate(row: maxRow, column: maxCol))
    return CellRange(start: start, end: end)
  }

  internal mutating func applyAutoDateFormatIfNeeded(
    for ast: FormulaAST,
    at coord: PackedCellCoordinate
  ) {
    guard SheetValueCoercion.isAutoDateFormula(ast) else { return }
    var style = cellStyles[coord] ?? CellStyle()
    if style.numberFormat == .general {
      style.numberFormat = .dateISO
      cellStyles[coord] = style
    }
  }

  public mutating func setCell(_ a1: String, rawValue: String) throws {
    let addr = try CellAddress(a1Notation: a1)
    let coord = addr.packedCoordinate
    if rawValue.isEmpty {
      cellsByCoordinate.removeValue(forKey: coord)
      compiledASTs.removeValue(forKey: coord)
    } else {
      cellsByCoordinate[coord] = rawValue
      let ast = Self.compileCellInput(rawValue)
      compiledASTs[coord] = ast
      applyAutoDateFormatIfNeeded(for: ast, at: coord)
    }
  }

  public mutating func setCell(row: Int, column: Int, rawValue: String) throws {
    let addr = try CellAddress(column: column, row: row)
    let coord = addr.packedCoordinate
    if rawValue.isEmpty {
      cellsByCoordinate.removeValue(forKey: coord)
      compiledASTs.removeValue(forKey: coord)
    } else {
      cellsByCoordinate[coord] = rawValue
      let ast = Self.compileCellInput(rawValue)
      compiledASTs[coord] = ast
      applyAutoDateFormatIfNeeded(for: ast, at: coord)
    }
  }

  public func rawValue(at a1: String) -> String {
    guard let addr = try? CellAddress(a1Notation: a1) else { return "" }
    return cellsByCoordinate[addr.packedCoordinate] ?? ""
  }

  public func rawValue(at packed: PackedCellCoordinate) -> String {
    cellsByCoordinate[packed] ?? ""
  }

  public struct FastCellBitset: Sendable, Equatable {
    public var words:
      (
        UInt64, UInt64, UInt64, UInt64,
        UInt64, UInt64, UInt64, UInt64,
        UInt64, UInt64, UInt64, UInt64,
        UInt64, UInt64, UInt64, UInt64
      ) = (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)

    public init() {}

    public static func == (lhs: FastCellBitset, rhs: FastCellBitset) -> Bool {
      withUnsafeBytes(of: lhs.words) { lBuf in
        withUnsafeBytes(of: rhs.words) { rBuf in
          for idx in 0..<16 {
            let lWord = lBuf.load(fromByteOffset: idx * 8, as: UInt64.self)
            let rWord = rBuf.load(fromByteOffset: idx * 8, as: UInt64.self)
            if lWord != rWord { return false }
          }
          return true
        }
      }
    }

    @inline(__always)
    public func canTrack(_ coord: PackedCellCoordinate) -> Bool {
      let r = coord.row
      let c = coord.column
      return r >= 0 && r < 64 && c >= 0 && c < 16
    }

    @inline(__always)
    public func contains(_ coord: PackedCellCoordinate) -> Bool {
      let r = coord.row
      let c = coord.column
      guard r >= 0, r < 64, c >= 0, c < 16 else { return false }
      let mask: UInt64 = 1 << UInt64(r)
      return withUnsafeBytes(of: words) { buf in
        let word = buf.load(fromByteOffset: c * 8, as: UInt64.self)
        return (word & mask) != 0
      }
    }

    @inline(__always)
    @discardableResult
    public mutating func insert(_ coord: PackedCellCoordinate) -> Bool {
      let r = coord.row
      let c = coord.column
      guard r >= 0, r < 64, c >= 0, c < 16 else { return false }
      let mask: UInt64 = 1 << UInt64(r)
      return withUnsafeMutableBytes(of: &words) { buf in
        let current = buf.load(fromByteOffset: c * 8, as: UInt64.self)
        let wasAbsent = (current & mask) == 0
        buf.storeBytes(of: current | mask, toByteOffset: c * 8, as: UInt64.self)
        return wasAbsent
      }
    }

    @inline(__always)
    public mutating func remove(_ coord: PackedCellCoordinate) {
      let r = coord.row
      let c = coord.column
      guard r >= 0, r < 64, c >= 0, c < 16 else { return }
      let mask: UInt64 = ~(1 << UInt64(r))
      withUnsafeMutableBytes(of: &words) { buf in
        let current = buf.load(fromByteOffset: c * 8, as: UInt64.self)
        buf.storeBytes(of: current & mask, toByteOffset: c * 8, as: UInt64.self)
      }
    }

    public func activeCoordinates() -> [PackedCellCoordinate] {
      var coords: [PackedCellCoordinate] = []
      withUnsafeBytes(of: words) { buf in
        for c in 0..<16 {
          var word = buf.load(fromByteOffset: c * 8, as: UInt64.self)
          while word != 0 {
            let r = word.trailingZeroBitCount
            coords.append(PackedCellCoordinate(row: r, column: c))
            word &= word &- 1
          }
        }
      }
      return coords
    }
  }

  internal struct CrossSheetCellKey: Hashable {
    let sheetNameUpper: String
    let coord: PackedCellCoordinate
  }

  internal struct EvaluationContext {
    var fastVisited = FastCellBitset()
    var visitedCount: Int = 0
    var visited: Set<PackedCellCoordinate> = []
    var memo: [PackedCellCoordinate: EvaluatedCellValue] = [:]
    var crossVisited: Set<CrossSheetCellKey> = []
    var crossMemo: [CrossSheetCellKey: EvaluatedCellValue] = [:]
    var sheetModelCache: [String: WorksheetModel] = [:]
    var currentCoord: PackedCellCoordinate? = nil
    var rngState: UInt64 = 0x853C_49E6_748F_EA9B

    mutating func nextRandomUnit() -> Double {
      rngState = rngState &* 6_364_136_223_846_793_005 &+ 1_442_695_040_888_963_407
      let bits = rngState >> 11
      return Double(bits) * (1.0 / 9_007_199_254_740_992.0)
    }

    @inline(__always)
    func isVisited(_ coord: PackedCellCoordinate) -> Bool {
      if fastVisited.canTrack(coord) {
        return fastVisited.contains(coord)
      }
      return visited.contains(coord)
    }

    @inline(__always)
    mutating func markVisited(_ coord: PackedCellCoordinate) {
      if fastVisited.canTrack(coord) {
        if fastVisited.insert(coord) {
          visitedCount += 1
        }
      } else {
        if visited.insert(coord).inserted {
          visitedCount += 1
        }
      }
    }

    @inline(__always)
    mutating func unmarkVisited(_ coord: PackedCellCoordinate) {
      if fastVisited.canTrack(coord) {
        if fastVisited.contains(coord) {
          fastVisited.remove(coord)
          visitedCount = max(0, visitedCount - 1)
        }
      } else {
        if visited.remove(coord) != nil {
          visitedCount = max(0, visitedCount - 1)
        }
      }
    }

    func allVisitedCoordinates() -> [PackedCellCoordinate] {
      var all = fastVisited.activeCoordinates()
      if !visited.isEmpty {
        all.append(contentsOf: visited)
      }
      return all
    }
  }
}

public typealias FastCellBitset = WorksheetModel.FastCellBitset
