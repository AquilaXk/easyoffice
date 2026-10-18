import Foundation

/// Bit-packed 64-bit spreadsheet cell coordinate (`row` in upper 32 bits, `column` in lower 32 bits).
/// Enables zero-allocation O(1) dictionary lookups and natural row-major sorting via single `UInt64` comparison.
public struct PackedCellCoordinate: Sendable, Codable, Equatable, Hashable, Comparable, CustomStringConvertible {
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

    public let column: Int // 0 = A, 25 = Z, 26 = AA
    public let row: Int    // 0 = row 1

    public init(column: Int, row: Int) throws {
        guard column >= 0, column <= Self.maxZeroBasedColumn,
              row >= 0, row <= Self.maxZeroBasedRow else {
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

            if b == 0x24 { // '$'
                continue
            }

            let upperByte: UInt8
            if b >= 0x61 && b <= 0x7A {
                upperByte = b - 0x20
            } else {
                upperByte = b
            }

            if upperByte >= 0x41 && upperByte <= 0x5A { // 'A'...'Z'
                guard digitCount == 0, letterCount < 3 else {
                    throw EasyOfficeError.invalidCellAddress(a1Notation)
                }
                letterCount += 1
                let charIndex = Int(upperByte - 0x41) + 1
                colValue = colValue * 26 + charIndex
            } else if b >= 0x30 && b <= 0x39 { // '0'...'9'
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
              rowOneBased >= 1, (rowOneBased - 1) <= Self.maxZeroBasedRow else {
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
public struct CellRange: Sendable, Codable, Equatable {
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
        for r in minRow...maxRow {
            for c in minCol...maxCol {
                if let addr = try? CellAddress(column: c, row: r) {
                    list.append(addr)
                }
            }
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
    case unaryMinus(FormulaAST)
    case binaryOp(op: String, lhs: FormulaAST, rhs: FormulaAST)
    case comparisonOp(op: String, lhs: FormulaAST, rhs: FormulaAST)
    case functionCall(name: String, args: [FormulaAST])

    var asCellRange: CellRange? {
        switch self {
        case let .rangeRef(range):
            return range
        case let .cellRef(coord):
            guard let addr = try? CellAddress(column: coord.column, row: coord.row) else { return nil }
            return CellRange(start: addr, end: addr)
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
    case error(String) // e.g. #DIV/0!, #VALUE!, #NAME?, #REF!, #N/A, #NUM!

    public var description: String {
        switch self {
        case .empty:
            return ""
        case let .number(val):
            if val.rounded() == val {
                return String(format: "%.0f", val)
            }
            return String(format: "%.4g", val)
        case let .text(str):
            return str
        case let .boolean(flag):
            return flag ? "TRUE" : "FALSE"
        case let .error(code):
            return code
        }
    }

    public var numericValue: Double? {
        switch self {
        case let .number(val): return val
        case let .boolean(flag): return flag ? 1.0 : 0.0
        case let .text(str): return Double(str.trimmingCharacters(in: .whitespaces))
        case .empty, .error: return nil
        }
    }

    public var booleanValue: Bool? {
        switch self {
        case let .boolean(flag): return flag
        case let .number(val): return val != 0.0
        case let .text(str):
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

    public func format(_ value: EvaluatedCellValue) -> String {
        guard case let .number(num) = value, num.isFinite else {
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
            let intVal = Int(num.rounded())
            if intVal >= 19000101 && intVal <= 29991231 {
                let y = intVal / 10000
                let m = (intVal % 10000) / 100
                let d = intVal % 100
                return String(format: "%04d-%02d-%02d", y, m, d)
            }
            if let baseDate = WorksheetModel.makeUTCDate(year: 1899, month: 12, day: 30),
               let target = WorksheetModel.utcCalendar.date(byAdding: .day, value: intVal, to: baseDate) {
                let comps = WorksheetModel.utcCalendar.dateComponents([.year, .month, .day], from: target)
                return String(format: "%04d-%02d-%02d", comps.year ?? 1970, comps.month ?? 1, comps.day ?? 1)
            }
            return value.description
        }
    }

    private static func formatGroupedNumber(_ value: Double, fractionDigits: Int) -> String {
        let isNegative = value < 0
        let absVal = abs(value)
        let rawFormatted = String(format: "%.\(max(0, fractionDigits))f", absVal)
        let parts = rawFormatted.split(separator: ".", omittingEmptySubsequences: false).map(String.init)
        let intDigits = Array(parts.first ?? "0")
        var grouped: [Character] = []
        grouped.reserveCapacity(intDigits.count + intDigits.count / 3)
        for (idx, ch) in intDigits.enumerated() {
            if idx > 0 && (intDigits.count - idx) % 3 == 0 {
                grouped.append(",")
            }
            grouped.append(ch)
        }
        let groupedInt = String(grouped)
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

    public init(
        isBold: Bool = false,
        isItalic: Bool = false,
        isUnderlined: Bool = false,
        alignment: CellHorizontalAlignment = .leading,
        numberFormat: CellNumberFormat = .general,
        fillHex: String? = nil,
        textHex: String? = nil
    ) {
        self.isBold = isBold
        self.isItalic = isItalic
        self.isUnderlined = isUnderlined
        self.alignment = alignment
        self.numberFormat = numberFormat
        self.fillHex = fillHex
        self.textHex = textHex
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

    public init(
        id: UUID = UUID(),
        name: String,
        cellsByCoordinate: [PackedCellCoordinate: String] = [:],
        cellStyles: [PackedCellCoordinate: CellStyle] = [:]
    ) {
        self.id = id
        self.name = name
        self.cellsByCoordinate = cellsByCoordinate
        self.cellStyles = cellStyles
    }
}

/// Worksheet model storing cell inputs keyed by `PackedCellCoordinate`, pre-compiling `FormulaAST`,
/// and evaluating formulas with $O(V + E)$ DAG memoization.
public struct WorksheetModel: Sendable, Codable, Equatable, Identifiable {
    /// Maximum output characters permitted for `REPT` to prevent memory exhaustion.
    public static let maxReptOutputCharacters = 32_767
    /// Maximum formula evaluation recursion depth before returning `#REF!`.
    public static let maxFormulaRecursionDepth = 256

    public let id: UUID
    public var name: String
    /// Primary coordinate-keyed storage (`PackedCellCoordinate -> rawString`).
    public private(set) var cellsByCoordinate: [PackedCellCoordinate: String] = [:]
    /// Per-cell style and number formatting table for the active sheet.
    public var cellStyles: [PackedCellCoordinate: CellStyle] = [:]
    /// Page setup and print configuration for the workbook/worksheet.
    public var pageSetup: WorksheetPageSetup = WorksheetPageSetup()
    /// Cell review comments keyed by cell coordinate.
    public var sheetComments: [PackedCellCoordinate: String] = [:]
    /// Whether revision tracking is enabled for this worksheet.
    public var trackChangesEnabled: Bool = false
    /// Non-active worksheet pages in the workbook.
    public var additionalSheets: [WorksheetSheetPage] = []
    /// Zero-based index of the active sheet among all sheets (`0 ... additionalSheets.count`).
    public var activeSheetIndex: Int = 0
    /// Pre-compiled AST cache keyed by `PackedCellCoordinate`.
    private var compiledASTs: [PackedCellCoordinate: FormulaAST] = [:]

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
        "ABS", "AND", "AVERAGE", "AVERAGEIF",
        "CEILING", "CHOOSE", "COLUMN", "CONCAT", "CONCATENATE", "COUNT", "COUNTA", "COUNTBLANK", "COUNTIF",
        "DATE", "DAY", "DAYS",
        "EXACT", "EXP",
        "FALSE", "FIND", "FLOOR", "FV",
        "HLOOKUP",
        "IF", "IFERROR", "IFS", "INDEX", "INT",
        "LARGE", "LEFT", "LEN", "LN", "LOG10", "LOWER",
        "MATCH", "MAX", "MEDIAN", "MID", "MIN", "MOD", "MODE", "MONTH",
        "NOT", "NPV",
        "OR",
        "PMT", "POWER", "PRODUCT", "PROPER", "PV",
        "RANK", "REPT", "RIGHT", "ROUND", "ROUNDDOWN", "ROUNDUP", "ROW",
        "SIGN", "SMALL", "SQRT", "STDEV", "SUBSTITUTE", "SUM", "SUMIF", "SUMPRODUCT", "SWITCH",
        "TEXT", "TRIM", "TRUE", "TRUNC",
        "UPPER",
        "VALUE", "VAR", "VLOOKUP",
        "XLOOKUP", "XOR",
        "YEAR"
    ].sorted()

    internal static let lazyOrRefFunctions: Set<String> = [
        "IF", "IFS", "IFERROR", "SWITCH", "CHOOSE", "ROW", "COLUMN", "SUMIF", "COUNTIF", "AVERAGEIF"
    ]

    public init(
        id: UUID = UUID(),
        name: String,
        rawCells: [String: String] = [:],
        cellStyles: [PackedCellCoordinate: CellStyle] = [:],
        pageSetup: WorksheetPageSetup = WorksheetPageSetup(),
        sheetComments: [PackedCellCoordinate: String] = [:],
        trackChangesEnabled: Bool = false,
        additionalSheets: [WorksheetSheetPage] = [],
        activeSheetIndex: Int = 0
    ) {
        self.id = id
        self.name = name
        self.cellStyles = cellStyles
        self.pageSetup = pageSetup
        self.sheetComments = sheetComments
        self.trackChangesEnabled = trackChangesEnabled
        self.additionalSheets = additionalSheets
        self.activeSheetIndex = activeSheetIndex
        for (key, val) in rawCells where !val.isEmpty {
            if let addr = try? CellAddress(a1Notation: key) {
                let coord = addr.packedCoordinate
                self.cellsByCoordinate[coord] = val
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
        case trackChangesEnabled
        case additionalSheets
        case activeSheetIndex
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.id = try container.decode(UUID.self, forKey: .id)
        self.name = try container.decode(String.self, forKey: .name)
        if let byCoord = try container.decodeIfPresent([PackedCellCoordinate: String].self, forKey: .cellsByCoordinate) {
            self.cellsByCoordinate = byCoord
            for (coord, val) in byCoord {
                self.compiledASTs[coord] = Self.compileCellInput(val)
            }
        } else if let legacyRaw = try container.decodeIfPresent([String: String].self, forKey: .rawCells) {
            for (key, val) in legacyRaw where !val.isEmpty {
                if let addr = try? CellAddress(a1Notation: key) {
                    let coord = addr.packedCoordinate
                    self.cellsByCoordinate[coord] = val
                    self.compiledASTs[coord] = Self.compileCellInput(val)
                }
            }
        }
        self.cellStyles = try container.decodeIfPresent([PackedCellCoordinate: CellStyle].self, forKey: .cellStyles) ?? [:]
        self.pageSetup = try container.decodeIfPresent(WorksheetPageSetup.self, forKey: .pageSetup) ?? WorksheetPageSetup()
        self.sheetComments = try container.decodeIfPresent([PackedCellCoordinate: String].self, forKey: .sheetComments) ?? [:]
        self.trackChangesEnabled = try container.decodeIfPresent(Bool.self, forKey: .trackChangesEnabled) ?? false
        self.additionalSheets = try container.decodeIfPresent([WorksheetSheetPage].self, forKey: .additionalSheets) ?? []
        self.activeSheetIndex = try container.decodeIfPresent(Int.self, forKey: .activeSheetIndex) ?? 0
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(name, forKey: .name)
        try container.encode(cellsByCoordinate, forKey: .cellsByCoordinate)
        try container.encode(cellStyles, forKey: .cellStyles)
        try container.encode(pageSetup, forKey: .pageSetup)
        try container.encode(sheetComments, forKey: .sheetComments)
        try container.encode(trackChangesEnabled, forKey: .trackChangesEnabled)
        try container.encode(additionalSheets, forKey: .additionalSheets)
        try container.encode(activeSheetIndex, forKey: .activeSheetIndex)
    }

    public static func == (lhs: WorksheetModel, rhs: WorksheetModel) -> Bool {
        lhs.id == rhs.id &&
            lhs.name == rhs.name &&
            lhs.cellsByCoordinate == rhs.cellsByCoordinate &&
            lhs.cellStyles == rhs.cellStyles &&
            lhs.pageSetup == rhs.pageSetup &&
            lhs.sheetComments == rhs.sheetComments &&
            lhs.trackChangesEnabled == rhs.trackChangesEnabled &&
            lhs.additionalSheets == rhs.additionalSheets &&
            lhs.activeSheetIndex == rhs.activeSheetIndex
    }

    // MARK: - Multi-Sheet Management & Cell Style Helpers

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
                cellStyles: [:]
            )
        )
    }

    /// Switches the active worksheet tab to `newIndex` (`0 ..< allSheetNames.count`), preserving tab order and rebuilding `compiledASTs`.
    public mutating func switchActiveSheet(to newIndex: Int) {
        var allPages = additionalSheets
        let currentInsertionIndex = min(max(0, activeSheetIndex), allPages.count)
        let currentActivePage = WorksheetSheetPage(
            name: name,
            cellsByCoordinate: cellsByCoordinate,
            cellStyles: cellStyles
        )
        allPages.insert(currentActivePage, at: currentInsertionIndex)
        guard allPages.indices.contains(newIndex), newIndex != currentInsertionIndex else {
            return
        }

        let targetPage = allPages.remove(at: newIndex)
        self.name = targetPage.name
        self.cellsByCoordinate = targetPage.cellsByCoordinate
        self.cellStyles = targetPage.cellStyles
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
            cellStyles: cellStyles
        )
        allPages.insert(currentActivePage, at: currentInsertionIndex)
        return allPages
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
        return style.numberFormat.format(val)
    }

    /// Automatically detects the bounding `CellRange` of all non-empty cells in the active sheet (falling back to `A1:D5` if empty).
    public func detectedDataRange() -> CellRange {
        guard !cellsByCoordinate.isEmpty else {
            let start = (try? CellAddress(column: 0, row: 0)) ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
            let end = (try? CellAddress(column: 3, row: 4)) ?? start
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
        let start = (try? CellAddress(column: minCol, row: minRow)) ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
        let end = (try? CellAddress(column: maxCol, row: maxRow)) ?? start
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
        let start = (try? CellAddress(column: minCol, row: minRow)) ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
        let end = (try? CellAddress(column: maxCol, row: maxRow)) ?? start
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

    internal struct EvaluationContext {
        var visited: Set<PackedCellCoordinate> = []
        var memo: [PackedCellCoordinate: EvaluatedCellValue] = [:]
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
              packed.row >= 0, packed.row <= CellAddress.maxZeroBasedRow else {
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

    /// Evaluates and sorts rows within `range` by the zero-based `colOffset` relative to `range`.
    public func sortedRows(
        in range: CellRange,
        byColumnOffset colOffset: Int,
        ascending: Bool = true
    ) -> [[EvaluatedCellValue]] {
        let rows = evaluateGrid(in: range)
        guard let firstRow = rows.first, colOffset >= 0, colOffset < firstRow.count else {
            return rows
        }
        return rows.sorted { lhs, rhs in
            let a = lhs[colOffset]
            let b = rhs[colOffset]
            if let aNum = a.numericValue, let bNum = b.numericValue {
                return ascending ? (aNum < bNum) : (aNum > bNum)
            }
            return ascending ? (a.description < b.description) : (a.description > b.description)
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
              totalCells > 0, totalCells <= CellRange.maxExpandedCellCount else {
            return []
        }

        var grid: [[EvaluatedCellValue]] = []
        grid.reserveCapacity(rowSpan)
        for r in minRow...maxRow {
            var rowVals: [EvaluatedCellValue] = []
            rowVals.reserveCapacity(colSpan)
            for c in minCol...maxCol {
                let packed = PackedCellCoordinate(row: r, column: c)
                rowVals.append(evaluateInternal(at: packed, context: &context, depth: depth + 1))
            }
            grid.append(rowVals)
        }
        return grid
    }

    // MARK: - FormulaAST Compiler

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
        if trimmed.hasPrefix("(") && trimmed.hasSuffix(")") &&
            matchingCloseIndex(in: trimmed, openAt: trimmed.startIndex) == trimmed.index(before: trimmed.endIndex) {
            let inner = String(trimmed.dropFirst().dropLast())
            return compileExpression(inner)
        }

        // 4. Function call: FUNCNAME(args...)
        if let openParen = trimmed.firstIndex(of: "("),
           trimmed.hasSuffix(")"),
           matchingCloseIndex(in: trimmed, openAt: openParen) == trimmed.index(before: trimmed.endIndex) {
            let funcName = String(trimmed[..<openParen]).trimmingCharacters(in: .whitespaces).uppercased()
            if !funcName.isEmpty && funcName.unicodeScalars.allSatisfy({ CharacterSet.alphanumerics.contains($0) }) {
                let innerStart = trimmed.index(after: openParen)
                let innerEnd = trimmed.index(before: trimmed.endIndex)
                let argString = innerStart <= innerEnd ? String(trimmed[innerStart..<innerEnd]) : ""
                let rawArgs = splitTopLevelArguments(argString)
                let compiledArgs = rawArgs.map { arg -> FormulaAST in
                    let cleanArg = arg.trimmingCharacters(in: .whitespacesAndNewlines)
                    if cleanArg.contains(":") && !cleanArg.contains("(") && !cleanArg.contains("\"") {
                        if let range = try? CellRange(notation: cleanArg) {
                            return .rangeRef(range)
                        }
                        return .error(.ref)
                    }
                    return compileExpression(cleanArg)
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

        // 8. Unary minus on non-literal expression
        if trimmed.hasPrefix("-") && trimmed.count > 1 {
            let inner = String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces)
            if !inner.isEmpty {
                return .unaryMinus(compileExpression(inner))
            }
        }

        // 9. Direct cell reference
        if let addr = try? CellAddress(a1Notation: trimmed) {
            return .cellRef(addr.packedCoordinate)
        }

        return .error(.value)
    }

    private static func compileTopLevelComparison(_ expr: String) -> FormulaAST? {
        let utf8 = expr.utf8
        var parenDepth = 0
        var inQuotes = false
        var idx = utf8.startIndex
        while idx < utf8.endIndex {
            let b = utf8[idx]
            if b == 0x22 { // '"'
                inQuotes.toggle()
            } else if !inQuotes {
                if b == 0x28 { // '('
                    parenDepth += 1
                } else if b == 0x29 { // ')'
                    parenDepth -= 1
                } else if parenDepth == 0 {
                    let nextIdx = utf8.index(after: idx)
                    let nextByte: UInt8? = nextIdx < utf8.endIndex ? utf8[nextIdx] : nil
                    let op: String?
                    let afterOpIdx: String.Index
                    if b == 0x3E && nextByte == 0x3D { // ">="
                        op = ">="
                        afterOpIdx = utf8.index(after: nextIdx)
                    } else if b == 0x3C && nextByte == 0x3D { // "<="
                        op = "<="
                        afterOpIdx = utf8.index(after: nextIdx)
                    } else if b == 0x3C && nextByte == 0x3E { // "<>"
                        op = "<>"
                        afterOpIdx = utf8.index(after: nextIdx)
                    } else if b == 0x3D { // "="
                        op = "="
                        afterOpIdx = nextIdx
                    } else if b == 0x3E { // ">"
                        op = ">"
                        afterOpIdx = nextIdx
                    } else if b == 0x3C { // "<"
                        op = "<"
                        afterOpIdx = nextIdx
                    } else {
                        op = nil
                        afterOpIdx = nextIdx
                    }

                    if let op {
                        let leftStr = String(expr[expr.startIndex..<idx]).trimmingCharacters(in: .whitespaces)
                        let rightStr = String(expr[afterOpIdx..<expr.endIndex]).trimmingCharacters(in: .whitespaces)
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
            (0x2B, 0x2D), // '+', '-'
            (0x2A, 0x2F)  // '*', '/'
        ]
        for (opA, opB) in operatorPasses {
            var parenDepth = 0
            var inQuotes = false
            var idx = utf8.endIndex
            while idx > utf8.startIndex {
                idx = utf8.index(before: idx)
                let b = utf8[idx]
                if b == 0x22 { // '"'
                    inQuotes.toggle()
                    continue
                }
                if inQuotes { continue }
                if b == 0x29 { // ')'
                    parenDepth += 1
                } else if b == 0x28 { // '('
                    parenDepth -= 1
                } else if parenDepth == 0 && idx > utf8.startIndex && (b == opA || b == opB) {
                    let nextIdx = utf8.index(after: idx)
                    let lhsStr = String(expr[expr.startIndex..<idx]).trimmingCharacters(in: .whitespaces)
                    let rhsStr = String(expr[nextIdx..<expr.endIndex]).trimmingCharacters(in: .whitespaces)
                    if lhsStr.isEmpty || rhsStr.isEmpty { continue }
                    if let prevByte = lhsStr.utf8.last,
                       prevByte == 0x2B || prevByte == 0x2D || prevByte == 0x2A || prevByte == 0x2F ||
                       prevByte == 0x28 || prevByte == 0x2C || prevByte == 0x3D || prevByte == 0x3C || prevByte == 0x3E {
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

    // MARK: - AST Evaluator with DAG Memoization

    internal func evaluateInternal(
        at packed: PackedCellCoordinate,
        context: inout EvaluationContext,
        depth: Int
    ) -> EvaluatedCellValue {
        guard depth <= Self.maxFormulaRecursionDepth,
              context.visited.count <= Self.maxFormulaRecursionDepth else {
            return .error("#REF!")
        }
        if context.visited.contains(packed) {
            return .error("#REF!")
        }
        if let memoized = context.memo[packed] {
            return memoized
        }
        guard let ast = compiledASTs[packed] else {
            return .empty
        }

        switch ast {
        case .empty:
            return .empty
        case let .literalNumber(num):
            return .number(num)
        case let .literalText(str):
            return .text(str)
        case let .literalBoolean(flag):
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
              context.visited.count <= Self.maxFormulaRecursionDepth else {
            return .error("#REF!")
        }

        switch ast {
        case .empty:
            return .empty
        case let .literalNumber(num), let .number(num):
            return .number(num)
        case let .literalText(str), let .text(str):
            return .text(str)
        case let .literalBoolean(flag), let .boolean(flag):
            return .boolean(flag)
        case let .error(err):
            return .error(err.rawValue)
        case let .cellRef(coord):
            let resolved = evaluateInternal(at: coord, context: &context, depth: depth + 1)
            if case .empty = resolved {
                return .number(0.0)
            }
            return resolved
        case .rangeRef:
            return .error("#VALUE!")
        case let .unaryMinus(inner):
            let val = evaluateAST(inner, context: &context, depth: depth + 1)
            if case .error = val { return val }
            guard let num = val.numericValue else { return .error("#VALUE!") }
            return .number(-num)
        case let .binaryOp(op, lhs, rhs):
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

        case let .comparisonOp(op, lhs, rhs):
            let lVal = evaluateAST(lhs, context: &context, depth: depth + 1)
            if case .error = lVal { return lVal }
            let rVal = evaluateAST(rhs, context: &context, depth: depth + 1)
            if case .error = rVal { return rVal }

            if let lNum = lVal.numericValue, let rNum = rVal.numericValue {
                switch op {
                case ">=": return .boolean(lNum >= rNum)
                case "<=": return .boolean(lNum <= rNum)
                case "<>": return .boolean(lNum != rNum)
                case "=":  return .boolean(lNum == rNum)
                case ">":  return .boolean(lNum > rNum)
                case "<":  return .boolean(lNum < rNum)
                default:   return .error("#VALUE!")
                }
            } else {
                let cmp = lVal.description.caseInsensitiveCompare(rVal.description)
                switch op {
                case "=":  return .boolean(cmp == .orderedSame)
                case "<>": return .boolean(cmp != .orderedSame)
                case ">":  return .boolean(cmp == .orderedDescending)
                case "<":  return .boolean(cmp == .orderedAscending)
                case ">=": return .boolean(cmp == .orderedSame || cmp == .orderedDescending)
                case "<=": return .boolean(cmp == .orderedSame || cmp == .orderedAscending)
                default:   return .error("#VALUE!")
                }
            }

        case let .functionCall(name, args):
            return evaluateFunction(name: name, args: args, context: &context, depth: depth + 1)
        }
    }

    // MARK: - Parser Helpers

    private static func matchingCloseIndex(in text: String, openAt: String.Index) -> String.Index? {
        var depth = 0
        var inQuotes = false
        var idx = openAt
        while idx < text.endIndex {
            let ch = text[idx]
            if ch == "\"" { inQuotes.toggle() }
            if !inQuotes {
                if ch == "(" { depth += 1 }
                else if ch == ")" {
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
        var inQuotes = false

        for ch in input {
            if ch == "\"" { inQuotes.toggle() }
            if !inQuotes {
                if ch == "(" { depth += 1 }
                else if ch == ")" { depth -= 1 }
                else if ch == "," && depth == 0 {
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
