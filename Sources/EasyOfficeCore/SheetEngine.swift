import Foundation

/// Worksheet model storing cell inputs keyed by `PackedCellCoordinate`, pre-compiling `FormulaAST`,
/// and evaluating formulas with $O(V + E)$ DAG memoization and cross-sheet reference resolution.
public struct WorksheetModel: Sendable, Codable, Equatable, Identifiable {
  /// Maximum output characters permitted for `REPT` to prevent memory exhaustion.
  public static let maxReptOutputCharacters = 32_767
  /// Maximum formula evaluation recursion depth before returning `#REF!`.
  public static let maxFormulaRecursionDepth = 256

  public let id: UUID
  public var name: String
  /// Primary coordinate-keyed storage (`PackedCellCoordinate -> rawString`).
  public internal(set) var cellsByCoordinate: [PackedCellCoordinate: String] = [:]
  /// Per-cell style and number formatting table for the active sheet.
  public var cellStyles: [PackedCellCoordinate: CellStyle] = [:]
  /// Page setup and print configuration for the workbook/worksheet.
  public var pageSetup: WorksheetPageSetup = WorksheetPageSetup()
  /// Cell review comments keyed by cell coordinate on the active sheet.
  public var sheetComments: [PackedCellCoordinate: String] = [:]
  /// Merged cell ranges on the active sheet.
  public var mergedRanges: [CellRange] = []
  /// Custom column widths (in points) keyed by zero-based column index on the active sheet.
  public var columnWidths: [Int: Double] = [:]
  /// Custom row heights (in points) keyed by zero-based row index on the active sheet.
  public var rowHeights: [Int: Double] = [:]
  /// Whether revision tracking is enabled for this worksheet.
  public var trackChangesEnabled: Bool = false
  /// Non-active worksheet pages in the workbook.
  public var additionalSheets: [WorksheetSheetPage] = []
  /// Zero-based index of the active sheet among all sheets (`0 ... additionalSheets.count`).
  public var activeSheetIndex: Int = 0
  /// Workbook/worksheet named ranges mapping normalized identifier names to A1 cell or range notation.
  public var namedRanges: [String: String] = [:]
  /// Active worksheet conditional formatting rules.
  public var conditionalFormatRules: [SheetConditionalFormatRule] = []
  /// Active worksheet data validation rule.
  public var dataValidationRule: SheetDataValidationRule? = nil
  /// Pre-compiled AST cache keyed by `PackedCellCoordinate`.
  internal var compiledASTs: [PackedCellCoordinate: FormulaAST] = [:]

  /// Backward-compatible A1-notation dictionary view over `cellsByCoordinate`.
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
          compiledASTs[coord] = Self.compileCellInput(val)
        }
      }
    }
  }

  /// Ordered list of all sheet names in the workbook, including the active sheet at `activeSheetIndex`.
  public var allSheetNames: [String] {
    var names = additionalSheets.map(\.name)
    let insertionIndex = min(max(0, activeSheetIndex), names.count)
    names.insert(name, at: insertionIndex)
    return names
  }

  /// Sorted list of all built-in spreadsheet functions supported by the formula engine.
  public static let supportedFunctionNames: [String] = [
    "ABS", "AND", "AVEDEV", "AVERAGE", "AVERAGEIF", "AVERAGEIFS",
    "CEILING", "CHOOSE", "COLUMN", "COLUMNS", "CONCAT", "CONCATENATE", "CORREL", "COUNT", "COUNTA",
    "COUNTBLANK", "COUNTIF", "COUNTIFS",
    "DATE", "DATEDIF", "DAY", "DAYS",
    "EDATE", "EFFECT", "EOMONTH", "EXACT", "EXP",
    "FALSE", "FIND", "FLOOR", "FORECAST", "FV",
    "GEOMEAN",
    "HARMEAN", "HLOOKUP",
    "IF", "IFERROR", "IFNA", "IFS", "INDEX", "INT", "INTERCEPT", "IRR", "ISBLANK", "ISERROR",
    "ISNUMBER", "ISTEXT",
    "LARGE", "LEFT", "LEN", "LN", "LOG10", "LOWER",
    "MATCH", "MAX", "MAXIFS", "MEDIAN", "MID", "MIN", "MINIFS", "MOD", "MODE", "MONTH", "MROUND",
    "NETWORKDAYS", "NOMINAL", "NOT", "NPER", "NPV",
    "OFFSET", "OR",
    "PERCENTILE", "PI", "PMT", "POWER", "PRODUCT", "PROPER", "PV",
    "QUARTILE", "QUOTIENT",
    "RANK", "RATE", "REPLACE", "REPT", "RIGHT", "ROUND", "ROUNDDOWN", "ROUNDUP", "ROW", "ROWS",
    "SEARCH", "SIGN", "SLN", "SLOPE", "SMALL", "SQRT", "STDEV", "STDEVP", "SUBSTITUTE", "SUM",
    "SUMIF", "SUMIFS", "SUMPRODUCT", "SUMSQ", "SWITCH", "SYD",
    "TEXT", "TEXTJOIN", "TRIM", "TRUE", "TRUNC",
    "UPPER",
    "VALUE", "VAR", "VARP", "VLOOKUP",
    "WEEKDAY", "WEEKNUM", "WORKDAY",
    "XLOOKUP", "XOR",
    "YEAR",
  ].sorted()

  internal static let lazyOrRefFunctions: Set<String> = [
    "IF", "IFS", "IFERROR", "IFNA", "SWITCH", "CHOOSE",
    "OFFSET", "ROW", "COLUMN", "ROWS", "COLUMNS",
    "ISBLANK", "ISERROR", "ISNUMBER", "ISTEXT",
    "SUMIF", "COUNTIF", "AVERAGEIF",
    "SUMIFS", "COUNTIFS", "AVERAGEIFS", "MINIFS", "MAXIFS",
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
        self.compiledASTs[coord] = Self.compileCellInput(val)
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
        self.compiledASTs[coord] = Self.compileCellInput(val)
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
    if let byCoord = try container.decodeIfPresent(
      [PackedCellCoordinate: String].self, forKey: .cellsByCoordinate)
    {
      self.cellsByCoordinate = byCoord
      for (coord, val) in byCoord {
        self.compiledASTs[coord] = Self.compileCellInput(val)
      }
    } else if let legacyRaw = try container.decodeIfPresent(
      [String: String].self, forKey: .rawCells)
    {
      for (key, val) in legacyRaw where !val.isEmpty {
        if let addr = try? CellAddress(a1Notation: key) {
          let coord = addr.packedCoordinate
          self.cellsByCoordinate[coord] = val
          self.compiledASTs[coord] = Self.compileCellInput(val)
        }
      }
    }
    self.cellStyles =
      try container.decodeIfPresent([PackedCellCoordinate: CellStyle].self, forKey: .cellStyles)
      ?? [:]
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

  // MARK: - Named Ranges, Multi-Sheet Management, Cell Merging & Style Helpers

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

  /// Appends a new sheet tab to the workbook while keeping the current active sheet selected.
  public mutating func addSheetPage(
    named sheetName: String,
    initialCells: [PackedCellCoordinate: String] = [:]
  ) {
    let trimmed = sheetName.trimmingCharacters(in: .whitespacesAndNewlines)
    let resolvedName = trimmed.isEmpty ? "Sheet\(allSheetNames.count + 1)" : trimmed
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

  /// Switches the active worksheet tab to `newIndex` (`0 ..< allSheetNames.count`), preserving per-sheet comments, merged ranges, custom dimensions, and tab order.
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
      self.compiledASTs[coord] = Self.compileCellInput(rawVal)
    }
  }

  /// Returns ordered `WorksheetSheetPage` snapshots for all tabs in the workbook.
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

  /// Toggles cell merging for `range` on the active sheet.
  public mutating func toggleMergeCells(_ range: CellRange) {
    if let existingIdx = mergedRanges.firstIndex(where: { $0 == range || $0.overlaps(range) }) {
      mergedRanges.remove(at: existingIdx)
    } else {
      mergedRanges.append(range)
    }
  }

  /// Returns `true` if `coord` lies within any merged cell range on the active sheet.
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

  /// Formats the evaluated display string for `address`, optionally reading from a pre-evaluated batch grid dictionary.
  public func formattedDisplayValue(
    at address: CellAddress,
    using preEvaluated: [PackedCellCoordinate: EvaluatedCellValue]? = nil
  ) -> String {
    let coord = address.packedCoordinate
    let val = preEvaluated?[coord] ?? evaluate(at: coord)
    let style = cellStyles[coord] ?? CellStyle()
    return style.format(val)
  }

  /// Automatically detects the bounding `CellRange` of all non-empty cells in the active sheet (falling back to `A1:D5` if empty).
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

  /// Automatically detects the bounding `CellRange` of all numeric-valued cells in the active sheet (falling back to `detectedDataRange()`).
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

  public mutating func setCell(_ a1: String, rawValue: String) throws {
    let addr = try CellAddress(a1Notation: a1)
    let coord = addr.packedCoordinate
    if rawValue.isEmpty {
      cellsByCoordinate.removeValue(forKey: coord)
      compiledASTs.removeValue(forKey: coord)
    } else {
      cellsByCoordinate[coord] = rawValue
      compiledASTs[coord] = Self.compileCellInput(rawValue)
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
      compiledASTs[coord] = Self.compileCellInput(rawValue)
    }
  }

  public func rawValue(at a1: String) -> String {
    guard let addr = try? CellAddress(a1Notation: a1) else { return "" }
    return cellsByCoordinate[addr.packedCoordinate] ?? ""
  }

  public func rawValue(at packed: PackedCellCoordinate) -> String {
    cellsByCoordinate[packed] ?? ""
  }

  // MARK: - Evaluation Context & Public Entrypoints

  internal struct CrossSheetCellKey: Hashable {
    let sheetNameUpper: String
    let coord: PackedCellCoordinate
  }

  internal struct EvaluationContext {
    var visited: Set<PackedCellCoordinate> = []
    var memo: [PackedCellCoordinate: EvaluatedCellValue] = [:]
    var crossVisited: Set<CrossSheetCellKey> = []
    var crossMemo: [CrossSheetCellKey: EvaluatedCellValue] = [:]
  }

  public func evaluate(at a1: String) -> EvaluatedCellValue {
    guard let address = try? CellAddress(a1Notation: a1) else {
      return .error("#REF!")
    }
    var context = EvaluationContext()
    return evaluateInternal(at: address.packedCoordinate, context: &context, depth: 0)
  }

  public func evaluate(at packed: PackedCellCoordinate) -> EvaluatedCellValue {
    guard packed.column >= 0, packed.column <= CellAddress.maxZeroBasedColumn,
      packed.row >= 0, packed.row <= CellAddress.maxZeroBasedRow
    else {
      return .error("#REF!")
    }
    var context = EvaluationContext()
    return evaluateInternal(at: packed, context: &context, depth: 0)
  }

  /// Evaluates an entire rectangular grid of `rows` and `columns` sharing a single `EvaluationContext` memo table.
  public func evaluateGrid(
    rows: ClosedRange<Int>,
    columns: ClosedRange<Int>
  ) -> [PackedCellCoordinate: EvaluatedCellValue] {
    var context = EvaluationContext()
    var result: [PackedCellCoordinate: EvaluatedCellValue] = [:]
    result.reserveCapacity(min(cellsByCoordinate.count, rows.count * columns.count))
    for r in rows where r >= 0 && r <= CellAddress.maxZeroBasedRow {
      for c in columns where c >= 0 && c <= CellAddress.maxZeroBasedColumn {
        let coord = PackedCellCoordinate(row: r, column: c)
        guard cellsByCoordinate[coord] != nil else { continue }
        result[coord] = evaluateInternal(at: coord, context: &context, depth: 0)
      }
    }
    return result
  }

  /// Evaluates and sorts rows within `range` by the zero-based `colOffset` relative to `range`, breaking ties with optional `secondaryColumnOffset`.
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

  /// Evaluates and filters rows within `range` by applying `predicate` to the column at `colOffset`.
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
    let minRow = min(range.start.row, range.end.row)
    let maxRow = max(range.start.row, range.end.row)
    let minCol = min(range.start.column, range.end.column)
    let maxCol = max(range.start.column, range.end.column)
    let (rowSpan, rOverflow) = (maxRow - minRow).addingReportingOverflow(1)
    let (colSpan, cOverflow) = (maxCol - minCol).addingReportingOverflow(1)
    let (totalCells, mulOverflow) = rowSpan.multipliedReportingOverflow(by: colSpan)
    guard !rOverflow, !cOverflow, !mulOverflow,
      totalCells > 0, totalCells <= CellRange.maxExpandedCellCount
    else {
      return []
    }

    var grid: [[EvaluatedCellValue]] = []
    grid.reserveCapacity(rowSpan)
    for r in minRow...maxRow {
      var rowVals: [EvaluatedCellValue] = []
      rowVals.reserveCapacity(colSpan)
      for c in minCol...maxCol {
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

  // MARK: - AST Evaluator with DAG Memoization & Cross-Sheet Resolution

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
    for activeCoord in context.visited {
      context.crossVisited.insert(
        CrossSheetCellKey(sheetNameUpper: activeSheetUpper, coord: activeCoord))
    }
    if context.crossVisited.contains(key)
      || context.crossVisited.count > Self.maxFormulaRecursionDepth
    {
      return .error("#REF!")
    }
    if let cached = context.crossMemo[key] {
      return cached
    }
    guard let raw = targetPage.cellsByCoordinate[packed], !raw.isEmpty else {
      return .empty
    }
    let ast = Self.compileCellInput(raw)
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
      let targetSheetUpper = targetPage.name.uppercased()
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
      let targetModel = WorksheetModel(
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
        precompileAllCells: false
      )
      var subContext = EvaluationContext(
        visited: [packed],
        memo: [:],
        crossVisited: context.crossVisited.union([key]),
        crossMemo: context.crossMemo
      )
      let evaluated = targetModel.evaluateAST(ast, context: &subContext, depth: depth + 1)
      for (subCoord, subVal) in subContext.memo {
        subContext.crossMemo[CrossSheetCellKey(sheetNameUpper: targetSheetUpper, coord: subCoord)] =
          subVal
      }
      context.crossMemo = subContext.crossMemo
      context.crossMemo[key] = evaluated
      return evaluated
    }
  }

  internal func evaluateInternal(
    at packed: PackedCellCoordinate,
    context: inout EvaluationContext,
    depth: Int
  ) -> EvaluatedCellValue {
    guard depth <= Self.maxFormulaRecursionDepth,
      context.visited.count <= Self.maxFormulaRecursionDepth
    else {
      return .error("#REF!")
    }
    if context.visited.contains(packed) {
      return .error("#REF!")
    }
    let selfSheetUpper = name.uppercased()
    if !context.crossVisited.isEmpty,
      context.crossVisited.contains(
        CrossSheetCellKey(sheetNameUpper: selfSheetUpper, coord: packed))
    {
      return .error("#REF!")
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
      return .number(num)
    case .literalText(let str):
      return .text(str)
    case .literalBoolean(let flag):
      return .boolean(flag)
    default:
      context.visited.insert(packed)
      let evaluated = evaluateAST(ast, context: &context, depth: depth + 1)
      context.visited.remove(packed)
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
      context.visited.count <= Self.maxFormulaRecursionDepth
    else {
      return .error("#REF!")
    }

    switch ast {
    case .empty:
      return .empty
    case .literalNumber(let num), .number(let num):
      return .number(num)
    case .literalText(let str), .text(let str):
      return .text(str)
    case .literalBoolean(let flag), .boolean(let flag):
      return .boolean(flag)
    case .error(let err):
      return .error(err.rawValue)
    case .cellRef(let coord):
      let resolved = evaluateInternal(at: coord, context: &context, depth: depth + 1)
      if case .empty = resolved {
        return .number(0.0)
      }
      return resolved
    case .rangeRef:
      return .error("#VALUE!")
    case .namedRangeRef(let rangeName):
      guard let targetNotation = resolveNamedRange(rangeName) else {
        return .error("#NAME?")
      }
      if let addr = try? CellAddress(a1Notation: targetNotation) {
        let resolved = evaluateInternal(
          at: addr.packedCoordinate, context: &context, depth: depth + 1)
        if case .empty = resolved { return .number(0.0) }
        return resolved
      } else if let range = try? CellRange(notation: targetNotation) {
        let resolved = evaluateInternal(
          at: range.start.packedCoordinate, context: &context, depth: depth + 1)
        if case .empty = resolved { return .number(0.0) }
        return resolved
      }
      return .error("#REF!")
    case .sheetCellRef(let sheetName, let coord):
      let resolved = evaluateOnSheet(
        named: sheetName, at: coord, context: &context, depth: depth + 1)
      if case .empty = resolved {
        return .number(0.0)
      }
      return resolved
    case .sheetRangeRef(let sheetName, _):
      guard hasSheet(named: sheetName) else { return .error("#REF!") }
      return .error("#VALUE!")
    case .unaryMinus(let inner):
      let val = evaluateAST(inner, context: &context, depth: depth + 1)
      if case .error = val { return val }
      guard let num = val.numericValue else { return .error("#VALUE!") }
      return .number(-num)
    case .binaryOp(let op, let lhs, let rhs):
      let lhsVal = evaluateAST(lhs, context: &context, depth: depth + 1)
      if case .error = lhsVal { return lhsVal }
      let rhsVal = evaluateAST(rhs, context: &context, depth: depth + 1)
      if case .error = rhsVal { return rhsVal }
      guard let lNum = lhsVal.numericValue, let rNum = rhsVal.numericValue else {
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
      default:
        return .error("#VALUE!")
      }
      guard result.isFinite else { return .error("#NUM!") }
      return .number(result)

    case .comparisonOp(let op, let lhs, let rhs):
      let lVal = evaluateAST(lhs, context: &context, depth: depth + 1)
      if case .error = lVal { return lVal }
      let rVal = evaluateAST(rhs, context: &context, depth: depth + 1)
      if case .error = rVal { return rVal }

      if let lNum = lVal.numericValue, let rNum = rVal.numericValue {
        switch op {
        case ">=": return .boolean(lNum >= rNum)
        case "<=": return .boolean(lNum <= rNum)
        case "<>": return .boolean(lNum != rNum)
        case "=": return .boolean(lNum == rNum)
        case ">": return .boolean(lNum > rNum)
        case "<": return .boolean(lNum < rNum)
        default: return .error("#VALUE!")
        }
      } else {
        let cmp = lVal.description.caseInsensitiveCompare(rVal.description)
        switch op {
        case "=": return .boolean(cmp == .orderedSame)
        case "<>": return .boolean(cmp != .orderedSame)
        case ">": return .boolean(cmp == .orderedDescending)
        case "<": return .boolean(cmp == .orderedAscending)
        case ">=": return .boolean(cmp == .orderedSame || cmp == .orderedDescending)
        case "<=": return .boolean(cmp == .orderedSame || cmp == .orderedAscending)
        default: return .error("#VALUE!")
        }
      }

    case .functionCall(let name, let args):
      return evaluateFunction(name: name, args: args, context: &context, depth: depth + 1)
    }
  }
}
