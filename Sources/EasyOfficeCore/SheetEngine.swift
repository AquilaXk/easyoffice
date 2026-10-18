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

    private static let lazyOrRefFunctions: Set<String> = [
        "IF", "IFS", "IFERROR", "SWITCH", "CHOOSE", "ROW", "COLUMN", "SUMIF", "COUNTIF", "AVERAGEIF"
    ]

    public init(id: UUID = UUID(), name: String, rawCells: [String: String] = [:]) {
        self.id = id
        self.name = name
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
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(name, forKey: .name)
        try container.encode(cellsByCoordinate, forKey: .cellsByCoordinate)
    }

    public static func == (lhs: WorksheetModel, rhs: WorksheetModel) -> Bool {
        lhs.id == rhs.id && lhs.name == rhs.name && lhs.cellsByCoordinate == rhs.cellsByCoordinate
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

    private struct EvaluationContext {
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

    private func evaluateGridInternal(
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
        let chars = Array(expr)
        var parenDepth = 0
        var inQuotes = false
        var i = 0
        while i < chars.count {
            let ch = chars[i]
            if ch == "\"" { inQuotes.toggle() }
            if !inQuotes {
                if ch == "(" { parenDepth += 1 }
                else if ch == ")" { parenDepth -= 1 }
                else if parenDepth == 0 {
                    let twoChar = (i + 1 < chars.count) ? String([chars[i], chars[i + 1]]) : ""
                    let op: String?
                    let opLen: Int
                    if twoChar == ">=" || twoChar == "<=" || twoChar == "<>" {
                        op = twoChar
                        opLen = 2
                    } else if ch == "=" || ch == ">" || ch == "<" {
                        op = String(ch)
                        opLen = 1
                    } else {
                        op = nil
                        opLen = 0
                    }

                    if let op {
                        let leftStr = String(chars[0..<i]).trimmingCharacters(in: .whitespaces)
                        let rightStr = String(chars[(i + opLen)...]).trimmingCharacters(in: .whitespaces)
                        guard !leftStr.isEmpty, !rightStr.isEmpty else { return nil }
                        return .comparisonOp(
                            op: op,
                            lhs: compileExpression(leftStr),
                            rhs: compileExpression(rightStr)
                        )
                    }
                }
            }
            i += 1
        }
        return nil
    }

    private static func compileBinaryArithmetic(_ expr: String) -> FormulaAST? {
        let chars = Array(expr)
        for operators: [Character] in [["+", "-"], ["*", "/"]] {
            var parenDepth = 0
            var inQuotes = false
            for idx in stride(from: chars.count - 1, through: 0, by: -1) {
                let ch = chars[idx]
                if ch == "\"" { inQuotes.toggle() }
                if inQuotes { continue }
                if ch == ")" { parenDepth += 1 }
                else if ch == "(" { parenDepth -= 1 }
                else if parenDepth == 0 && idx > 0 && operators.contains(ch) {
                    let lhsStr = String(chars[0..<idx]).trimmingCharacters(in: .whitespaces)
                    let rhsStr = String(chars[(idx + 1)...]).trimmingCharacters(in: .whitespaces)
                    if lhsStr.isEmpty || rhsStr.isEmpty { continue }
                    if let prevChar = lhsStr.last, ["+", "-", "*", "/", "(", ",", "=", "<", ">"].contains(prevChar) {
                        continue
                    }
                    return .binaryOp(
                        op: String(ch),
                        lhs: compileExpression(lhsStr),
                        rhs: compileExpression(rhsStr)
                    )
                }
            }
        }
        return nil
    }

    // MARK: - AST Evaluator with DAG Memoization

    private func evaluateInternal(
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

    private func evaluateAST(
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

    // MARK: - Modular Function Dispatch (77 Built-in Functions)

    private enum FormulaResolutionResult {
        case success([EvaluatedCellValue])
        case failure(String)
    }

    private struct EvalFrame {
        let args: [FormulaAST]
        var cache: [EvaluatedCellValue?]
        var context: EvaluationContext
        let depth: Int

        var count: Int { args.count }
        var indices: Range<Int> { args.indices }

        mutating func arg(_ index: Int, in sheet: WorksheetModel) -> EvaluatedCellValue {
            guard args.indices.contains(index) else { return .error("#VALUE!") }
            if let cached = cache[index] {
                return cached
            }
            let val = sheet.evaluateAST(args[index], context: &context, depth: depth + 1)
            cache[index] = val
            return val
        }

        mutating func resolve(_ range: Range<Int>, in sheet: WorksheetModel) -> FormulaResolutionResult {
            var values: [EvaluatedCellValue] = []
            for idx in range {
                let item = args[idx]
                switch item {
                case let .rangeRef(cellRange):
                    for addr in cellRange.expandedAddresses() {
                        let v = sheet.evaluateInternal(at: addr.packedCoordinate, context: &context, depth: depth + 1)
                        if case let .error(code) = v { return .failure(code) }
                        values.append(v)
                    }
                case let .error(err):
                    return .failure(err.rawValue)
                default:
                    let v = arg(idx, in: sheet)
                    if case let .error(code) = v { return .failure(code) }
                    values.append(v)
                }
            }
            return .success(values)
        }
    }

    private func evaluateFunction(
        name: String,
        args: [FormulaAST],
        context: inout EvaluationContext,
        depth: Int
    ) -> EvaluatedCellValue {
        var takenContext = EvaluationContext()
        swap(&takenContext, &context)
        var frame = EvalFrame(
            args: args,
            cache: Array<EvaluatedCellValue?>(repeating: nil, count: args.count),
            context: takenContext,
            depth: depth
        )
        defer { swap(&context, &frame.context) }

        if !Self.lazyOrRefFunctions.contains(name) {
            for (idx, item) in args.enumerated() {
                if case .rangeRef = item { continue }
                let evaluated = frame.arg(idx, in: self)
                if case .error = evaluated {
                    return evaluated
                }
            }
        }

        if let res = evaluateMathAndTrigFunction(name: name, frame: &frame) { return res }
        if let res = evaluateStatisticalFunction(name: name, frame: &frame) { return res }
        if let res = evaluateLogicalFunction(name: name, frame: &frame) { return res }
        if let res = evaluateLookupAndRefFunction(name: name, frame: &frame) { return res }
        if let res = evaluateTextFunction(name: name, frame: &frame) { return res }
        if let res = evaluateFinancialAndDateFunction(name: name, frame: &frame) { return res }

        return .error("#NAME?")
    }

    // MARK: - 1. Math & Trig Functions

    private func evaluateMathAndTrigFunction(name: String, frame: inout EvalFrame) -> EvaluatedCellValue? {
        switch name {
        case "SUM":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals): return .number(vals.compactMap { $0.numericValue }.reduce(0.0, +))
            }
        case "AVERAGE":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let nums = vals.compactMap { $0.numericValue }
                guard !nums.isEmpty else { return .error("#DIV/0!") }
                return .number(nums.reduce(0.0, +) / Double(nums.count))
            }
        case "MIN":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals): return .number(vals.compactMap { $0.numericValue }.min() ?? 0.0)
            }
        case "MAX":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals): return .number(vals.compactMap { $0.numericValue }.max() ?? 0.0)
            }
        case "COUNT":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let count = vals.filter { if case .number = $0 { return true }; return false }.count
                return .number(Double(count))
            }
        case "COUNTA":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                return .number(Double(vals.filter { $0 != EvaluatedCellValue.empty }.count))
            }
        case "COUNTBLANK":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                return .number(Double(vals.filter { $0 == .empty || $0 == .text("") }.count))
            }
        case "ABS":
            guard frame.count == 1, let num = frame.arg(0, in: self).numericValue else { return .error("#VALUE!") }
            return .number(abs(num))
        case "ROUND":
            guard frame.count == 2,
                  let num = frame.arg(0, in: self).numericValue,
                  let digitsNum = frame.arg(1, in: self).numericValue,
                  let digits = Self.safeInt(digitsNum) else { return .error("#VALUE!") }
            let factor = pow(10.0, Double(digits))
            let result = (num * factor).rounded() / factor
            guard result.isFinite else { return .error("#NUM!") }
            return .number(result)
        case "ROUNDUP":
            guard frame.count == 2,
                  let num = frame.arg(0, in: self).numericValue,
                  let digitsNum = frame.arg(1, in: self).numericValue,
                  let digits = Self.safeInt(digitsNum) else { return .error("#VALUE!") }
            let factor = pow(10.0, Double(digits))
            let scaled = num * factor
            let rounded = (scaled >= 0 ? ceil(scaled) : floor(scaled)) / factor
            guard rounded.isFinite else { return .error("#NUM!") }
            return .number(rounded)
        case "ROUNDDOWN":
            guard frame.count == 2,
                  let num = frame.arg(0, in: self).numericValue,
                  let digitsNum = frame.arg(1, in: self).numericValue,
                  let digits = Self.safeInt(digitsNum) else { return .error("#VALUE!") }
            let factor = pow(10.0, Double(digits))
            let rounded = trunc(num * factor) / factor
            guard rounded.isFinite else { return .error("#NUM!") }
            return .number(rounded)
        case "INT":
            guard frame.count == 1, let num = frame.arg(0, in: self).numericValue, num.isFinite else { return .error("#VALUE!") }
            return .number(floor(num))
        case "TRUNC":
            guard (1...2).contains(frame.count), let num = frame.arg(0, in: self).numericValue else { return .error("#VALUE!") }
            let digits: Int
            if frame.count == 2 {
                guard let dNum = frame.arg(1, in: self).numericValue, let dInt = Self.safeInt(dNum) else { return .error("#VALUE!") }
                digits = dInt
            } else {
                digits = 0
            }
            let factor = pow(10.0, Double(digits))
            let result = trunc(num * factor) / factor
            guard result.isFinite else { return .error("#NUM!") }
            return .number(result)
        case "MOD":
            guard frame.count == 2,
                  let num = frame.arg(0, in: self).numericValue,
                  let divisor = frame.arg(1, in: self).numericValue else { return .error("#VALUE!") }
            guard divisor != 0.0 else { return .error("#DIV/0!") }
            return .number(num - divisor * floor(num / divisor))
        case "SQRT":
            guard frame.count == 1, let num = frame.arg(0, in: self).numericValue else { return .error("#VALUE!") }
            guard num >= 0.0 else { return .error("#NUM!") }
            return .number(sqrt(num))
        case "POWER":
            guard frame.count == 2,
                  let base = frame.arg(0, in: self).numericValue,
                  let expVal = frame.arg(1, in: self).numericValue else { return .error("#VALUE!") }
            let result = pow(base, expVal)
            guard result.isFinite else { return .error("#NUM!") }
            return .number(result)
        case "PRODUCT":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let nums = vals.compactMap { $0.numericValue }
                return .number(nums.isEmpty ? 0.0 : nums.reduce(1.0, *))
            }
        case "SUMPRODUCT":
            guard frame.count > 0 else { return .error("#VALUE!") }
            var vectors: [[Double]] = []
            for idx in frame.indices {
                switch frame.resolve(idx..<(idx + 1), in: self) {
                case let .failure(err): return .error(err)
                case let .success(vals): vectors.append(vals.map { $0.numericValue ?? 0.0 })
                }
            }
            guard let length = vectors.first?.count, length > 0, vectors.allSatisfy({ $0.count == length }) else {
                return .error("#VALUE!")
            }
            var total = 0.0
            for i in 0..<length {
                var rowProd = 1.0
                for vec in vectors { rowProd *= vec[i] }
                total += rowProd
            }
            return .number(total)
        case "SUMIF":
            guard (2...3).contains(frame.count) else { return .error("#VALUE!") }
            let critVal = frame.arg(1, in: self)
            switch frame.resolve(0..<1, in: self) {
            case let .failure(err): return .error(err)
            case let .success(criteriaCells):
                let targetCells: [EvaluatedCellValue]
                if frame.count == 3 {
                    switch frame.resolve(2..<3, in: self) {
                    case let .failure(err): return .error(err)
                    case let .success(sumCells):
                        guard sumCells.count == criteriaCells.count else { return .error("#VALUE!") }
                        targetCells = sumCells
                    }
                } else {
                    targetCells = criteriaCells
                }
                var sum = 0.0
                for idx in criteriaCells.indices {
                    if Self.matchesCriteriaValue(criteriaCells[idx], criteriaValue: critVal),
                       let num = targetCells[idx].numericValue {
                        sum += num
                    }
                }
                return .number(sum)
            }
        case "COUNTIF":
            guard frame.count == 2 else { return .error("#VALUE!") }
            let critVal = frame.arg(1, in: self)
            switch frame.resolve(0..<1, in: self) {
            case let .failure(err): return .error(err)
            case let .success(criteriaCells):
                return .number(Double(criteriaCells.filter { Self.matchesCriteriaValue($0, criteriaValue: critVal) }.count))
            }
        case "AVERAGEIF":
            guard (2...3).contains(frame.count) else { return .error("#VALUE!") }
            let critVal = frame.arg(1, in: self)
            switch frame.resolve(0..<1, in: self) {
            case let .failure(err): return .error(err)
            case let .success(criteriaCells):
                let targetCells: [EvaluatedCellValue]
                if frame.count == 3 {
                    switch frame.resolve(2..<3, in: self) {
                    case let .failure(err): return .error(err)
                    case let .success(avgCells):
                        guard avgCells.count == criteriaCells.count else { return .error("#VALUE!") }
                        targetCells = avgCells
                    }
                } else {
                    targetCells = criteriaCells
                }
                var matchedNumbers: [Double] = []
                for idx in criteriaCells.indices {
                    if Self.matchesCriteriaValue(criteriaCells[idx], criteriaValue: critVal),
                       let num = targetCells[idx].numericValue {
                        matchedNumbers.append(num)
                    }
                }
                guard !matchedNumbers.isEmpty else { return .error("#DIV/0!") }
                return .number(matchedNumbers.reduce(0.0, +) / Double(matchedNumbers.count))
            }
        case "CEILING":
            guard (1...2).contains(frame.count), let num = frame.arg(0, in: self).numericValue else { return .error("#VALUE!") }
            let sig = frame.count == 2 ? (frame.arg(1, in: self).numericValue ?? 1.0) : 1.0
            guard sig != 0.0 else { return .number(0.0) }
            let result = ceil(num / sig) * sig
            guard result.isFinite else { return .error("#NUM!") }
            return .number(result)
        case "FLOOR":
            guard (1...2).contains(frame.count), let num = frame.arg(0, in: self).numericValue else { return .error("#VALUE!") }
            let sig = frame.count == 2 ? (frame.arg(1, in: self).numericValue ?? 1.0) : 1.0
            guard sig != 0.0 else { return .error("#DIV/0!") }
            let result = floor(num / sig) * sig
            guard result.isFinite else { return .error("#NUM!") }
            return .number(result)
        case "SIGN":
            guard frame.count == 1, let num = frame.arg(0, in: self).numericValue else { return .error("#VALUE!") }
            return .number(num > 0 ? 1.0 : (num < 0 ? -1.0 : 0.0))
        case "LN":
            guard frame.count == 1, let num = frame.arg(0, in: self).numericValue else { return .error("#VALUE!") }
            guard num > 0 else { return .error("#NUM!") }
            return .number(log(num))
        case "LOG10":
            guard frame.count == 1, let num = frame.arg(0, in: self).numericValue else { return .error("#VALUE!") }
            guard num > 0 else { return .error("#NUM!") }
            return .number(log10(num))
        case "EXP":
            guard frame.count == 1, let num = frame.arg(0, in: self).numericValue else { return .error("#VALUE!") }
            let result = exp(num)
            guard result.isFinite else { return .error("#NUM!") }
            return .number(result)
        default:
            return nil
        }
    }

    // MARK: - 2. Statistical Functions

    private func evaluateStatisticalFunction(name: String, frame: inout EvalFrame) -> EvaluatedCellValue? {
        switch name {
        case "MEDIAN":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let sorted = vals.compactMap { $0.numericValue }.sorted()
                guard !sorted.isEmpty else { return .error("#NUM!") }
                let mid = sorted.count / 2
                return .number(sorted.count % 2 == 1 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2.0)
            }
        case "MODE":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let nums = vals.compactMap { $0.numericValue }
                guard !nums.isEmpty else { return .error("#N/A") }
                var counts: [Double: Int] = [:]
                for n in nums { counts[n, default: 0] += 1 }
                guard let best = nums.max(by: { counts[$0, default: 0] < counts[$1, default: 0] }),
                      (counts[best] ?? 0) > 1 else { return .error("#N/A") }
                return .number(best)
            }
        case "VAR":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let nums = vals.compactMap { $0.numericValue }
                guard nums.count >= 2 else { return .error("#DIV/0!") }
                let mean = nums.reduce(0.0, +) / Double(nums.count)
                let sumSq = nums.reduce(0.0) { $0 + ($1 - mean) * ($1 - mean) }
                return .number(sumSq / Double(nums.count - 1))
            }
        case "STDEV":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let nums = vals.compactMap { $0.numericValue }
                guard nums.count >= 2 else { return .error("#DIV/0!") }
                let mean = nums.reduce(0.0, +) / Double(nums.count)
                let sumSq = nums.reduce(0.0) { $0 + ($1 - mean) * ($1 - mean) }
                return .number(sqrt(sumSq / Double(nums.count - 1)))
            }
        case "LARGE":
            guard frame.count == 2 else { return .error("#VALUE!") }
            switch frame.resolve(0..<1, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                guard let kVal = frame.arg(1, in: self).numericValue, let k = Self.safeInt(kVal) else { return .error("#VALUE!") }
                let sortedDesc = vals.compactMap { $0.numericValue }.sorted(by: >)
                guard k >= 1, k <= sortedDesc.count else { return .error("#NUM!") }
                return .number(sortedDesc[k - 1])
            }
        case "SMALL":
            guard frame.count == 2 else { return .error("#VALUE!") }
            switch frame.resolve(0..<1, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                guard let kVal = frame.arg(1, in: self).numericValue, let k = Self.safeInt(kVal) else { return .error("#VALUE!") }
                let sortedAsc = vals.compactMap { $0.numericValue }.sorted(by: <)
                guard k >= 1, k <= sortedAsc.count else { return .error("#NUM!") }
                return .number(sortedAsc[k - 1])
            }
        case "RANK":
            guard (2...3).contains(frame.count), let target = frame.arg(0, in: self).numericValue else { return .error("#VALUE!") }
            switch frame.resolve(1..<2, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let order = frame.count == 3 ? (Self.safeInt(frame.arg(2, in: self).numericValue ?? 0.0) ?? 0) : 0
                let nums = vals.compactMap { $0.numericValue }
                guard nums.contains(target) else { return .error("#N/A") }
                let rank = order == 0 ? 1 + nums.filter { $0 > target }.count : 1 + nums.filter { $0 < target }.count
                return .number(Double(rank))
            }
        default:
            return nil
        }
    }

    // MARK: - 3. Logical Functions

    private func evaluateLogicalFunction(name: String, frame: inout EvalFrame) -> EvaluatedCellValue? {
        switch name {
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
        case "AND":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                guard !vals.isEmpty else { return .error("#VALUE!") }
                return .boolean(vals.allSatisfy { $0.booleanValue == true })
            }
        case "OR":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                guard !vals.isEmpty else { return .error("#VALUE!") }
                return .boolean(vals.contains { $0.booleanValue == true })
            }
        case "NOT":
            guard frame.count == 1 else { return .error("#VALUE!") }
            let v = frame.arg(0, in: self)
            if case .error = v { return v }
            return .boolean(!(v.booleanValue ?? false))
        case "XOR":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                guard !vals.isEmpty else { return .error("#VALUE!") }
                return .boolean(vals.filter { $0.booleanValue == true }.count % 2 == 1)
            }
        case "IFERROR":
            guard frame.count == 2 else { return .error("#VALUE!") }
            let primary = frame.arg(0, in: self)
            if case .error = primary { return frame.arg(1, in: self) }
            return primary
        case "SWITCH":
            guard frame.count >= 3 else { return .error("#VALUE!") }
            let target = frame.arg(0, in: self)
            if case .error = target { return target }
            let pairLimit = frame.count - (frame.count % 2 == 0 ? 1 : 0)
            var idx = 1
            while idx + 1 <= pairLimit {
                let candidate = frame.arg(idx, in: self)
                if case .error = candidate { return candidate }
                if candidate == target || candidate.description.caseInsensitiveCompare(target.description) == .orderedSame {
                    return frame.arg(idx + 1, in: self)
                }
                idx += 2
            }
            return frame.count % 2 == 0 ? frame.arg(frame.count - 1, in: self) : .error("#N/A")
        default:
            return nil
        }
    }

    // MARK: - 4. Lookup & Reference Functions

    private func evaluateLookupAndRefFunction(name: String, frame: inout EvalFrame) -> EvaluatedCellValue? {
        switch name {
        case "VLOOKUP":
            guard (3...4).contains(frame.count),
                  let range = frame.args[1].asCellRange,
                  let colIndexVal = frame.arg(2, in: self).numericValue,
                  let rawColIdx = Self.safeInt(colIndexVal) else { return .error("#VALUE!") }
            let lookupVal = frame.arg(0, in: self)
            let isApprox = frame.count == 4 ? (frame.arg(3, in: self).booleanValue ?? false) : false
            let grid = evaluateGridInternal(in: range, context: &frame.context, depth: frame.depth + 1)
            let colIdx = rawColIdx - 1
            guard let firstRow = grid.first, colIdx >= 0, colIdx < firstRow.count else { return .error("#REF!") }
            for row in grid where Self.cellValuesEqual(row[0], lookupVal) {
                return row[colIdx]
            }
            if isApprox, let targetNum = lookupVal.numericValue {
                var bestRow: [EvaluatedCellValue]?
                var bestNum = -Double.infinity
                for row in grid {
                    if let n = row[0].numericValue, n <= targetNum, n >= bestNum {
                        bestNum = n
                        bestRow = row
                    }
                }
                if let bestRow { return bestRow[colIdx] }
            }
            return .error("#N/A")

        case "HLOOKUP":
            guard (3...4).contains(frame.count),
                  let range = frame.args[1].asCellRange,
                  let rowIndexVal = frame.arg(2, in: self).numericValue,
                  let rawRowIdx = Self.safeInt(rowIndexVal) else { return .error("#VALUE!") }
            let lookupVal = frame.arg(0, in: self)
            let isApprox = frame.count == 4 ? (frame.arg(3, in: self).booleanValue ?? false) : false
            let grid = evaluateGridInternal(in: range, context: &frame.context, depth: frame.depth + 1)
            let rowIdx = rawRowIdx - 1
            guard rowIdx >= 0, rowIdx < grid.count, let headerRow = grid.first else { return .error("#REF!") }
            for col in headerRow.indices where Self.cellValuesEqual(headerRow[col], lookupVal) {
                return grid[rowIdx][col]
            }
            if isApprox, let targetNum = lookupVal.numericValue {
                var bestCol: Int?
                var bestNum = -Double.infinity
                for col in headerRow.indices {
                    if let n = headerRow[col].numericValue, n <= targetNum, n >= bestNum {
                        bestNum = n
                        bestCol = col
                    }
                }
                if let bestCol { return grid[rowIdx][bestCol] }
            }
            return .error("#N/A")

        case "XLOOKUP":
            guard (3...4).contains(frame.count) else { return .error("#VALUE!") }
            switch (frame.resolve(1..<2, in: self), frame.resolve(2..<3, in: self)) {
            case (let .failure(err), _), (_, let .failure(err)): return .error(err)
            case let (.success(lookupArr), .success(returnArr)):
                guard lookupArr.count == returnArr.count else { return .error("#VALUE!") }
                let lookupVal = frame.arg(0, in: self)
                for idx in lookupArr.indices where Self.cellValuesEqual(lookupArr[idx], lookupVal) {
                    return returnArr[idx]
                }
                return frame.count == 4 ? frame.arg(3, in: self) : .error("#N/A")
            }

        case "INDEX":
            guard (2...3).contains(frame.count),
                  let range = frame.args[0].asCellRange,
                  let rowNumVal = frame.arg(1, in: self).numericValue,
                  let rawRowIdx = Self.safeInt(rowNumVal) else { return .error("#VALUE!") }
            let colNumVal = frame.count == 3 ? (frame.arg(2, in: self).numericValue ?? 1.0) : 1.0
            guard let rawColIdx = Self.safeInt(colNumVal) else { return .error("#VALUE!") }
            let grid = evaluateGridInternal(in: range, context: &frame.context, depth: frame.depth + 1)
            let rIdx = rawRowIdx - 1
            let cIdx = rawColIdx - 1
            if grid.count == 1, frame.count == 2, rIdx >= 0, rIdx < grid[0].count {
                return grid[0][rIdx]
            }
            guard rIdx >= 0, rIdx < grid.count, cIdx >= 0, cIdx < grid[rIdx].count else { return .error("#REF!") }
            return grid[rIdx][cIdx]

        case "MATCH":
            guard (2...3).contains(frame.count) else { return .error("#VALUE!") }
            switch frame.resolve(1..<2, in: self) {
            case let .failure(err): return .error(err)
            case let .success(lookupArr):
                let lookupVal = frame.arg(0, in: self)
                for (idx, item) in lookupArr.enumerated() where Self.cellValuesEqual(item, lookupVal) {
                    return .number(Double(idx + 1))
                }
                return .error("#N/A")
            }

        case "CHOOSE":
            guard frame.count >= 2 else { return .error("#VALUE!") }
            let firstArg = frame.arg(0, in: self)
            if case .error = firstArg { return firstArg }
            guard let idxVal = firstArg.numericValue, let choiceIdx = Self.safeInt(idxVal),
                  choiceIdx >= 1, choiceIdx < frame.count else { return .error("#VALUE!") }
            return frame.arg(choiceIdx, in: self)

        case "ROW":
            guard frame.count == 1 else { return .error("#VALUE!") }
            switch frame.args[0] {
            case let .cellRef(coord): return .number(Double(coord.row + 1))
            case let .rangeRef(range): return .number(Double(range.start.row + 1))
            default: return .error("#REF!")
            }

        case "COLUMN":
            guard frame.count == 1 else { return .error("#VALUE!") }
            switch frame.args[0] {
            case let .cellRef(coord): return .number(Double(coord.column + 1))
            case let .rangeRef(range): return .number(Double(range.start.column + 1))
            default: return .error("#REF!")
            }

        default:
            return nil
        }
    }

    // MARK: - 5. Text Functions

    private func evaluateTextFunction(name: String, frame: inout EvalFrame) -> EvaluatedCellValue? {
        switch name {
        case "CONCAT", "CONCATENATE":
            switch frame.resolve(frame.indices, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals): return .text(vals.map { $0.description }.joined())
            }
        case "LEFT":
            guard (1...2).contains(frame.count) else { return .error("#VALUE!") }
            let text = frame.arg(0, in: self).description
            let count: Int
            if frame.count == 2 {
                guard let cNum = frame.arg(1, in: self).numericValue, let cInt = Self.safeInt(cNum), cInt >= 0 else {
                    return .error("#VALUE!")
                }
                count = cInt
            } else {
                count = 1
            }
            return .text(String(text.prefix(count)))
        case "RIGHT":
            guard (1...2).contains(frame.count) else { return .error("#VALUE!") }
            let text = frame.arg(0, in: self).description
            let count: Int
            if frame.count == 2 {
                guard let cNum = frame.arg(1, in: self).numericValue, let cInt = Self.safeInt(cNum), cInt >= 0 else {
                    return .error("#VALUE!")
                }
                count = cInt
            } else {
                count = 1
            }
            return .text(String(text.suffix(count)))
        case "MID":
            guard frame.count == 3,
                  let startVal = frame.arg(1, in: self).numericValue,
                  let lenVal = frame.arg(2, in: self).numericValue,
                  let startOneBased = Self.safeInt(startVal),
                  let length = Self.safeInt(lenVal),
                  startOneBased >= 1, length >= 0 else { return .error("#VALUE!") }
            let startIdx = startOneBased - 1
            let chars = Array(frame.arg(0, in: self).description)
            guard startIdx < chars.count else { return .text("") }
            let endIdx = min(chars.count, startIdx + min(length, chars.count - startIdx))
            return .text(String(chars[startIdx..<endIdx]))
        case "LEN":
            guard frame.count == 1 else { return .error("#VALUE!") }
            return .number(Double(frame.arg(0, in: self).description.count))
        case "LOWER":
            guard frame.count == 1 else { return .error("#VALUE!") }
            return .text(frame.arg(0, in: self).description.lowercased())
        case "UPPER":
            guard frame.count == 1 else { return .error("#VALUE!") }
            return .text(frame.arg(0, in: self).description.uppercased())
        case "PROPER":
            guard frame.count == 1 else { return .error("#VALUE!") }
            return .text(frame.arg(0, in: self).description.capitalized)
        case "TRIM":
            guard frame.count == 1 else { return .error("#VALUE!") }
            let parts = frame.arg(0, in: self).description
                .components(separatedBy: .whitespacesAndNewlines)
                .filter { !$0.isEmpty }
            return .text(parts.joined(separator: " "))
        case "SUBSTITUTE":
            guard (3...4).contains(frame.count) else { return .error("#VALUE!") }
            let source = frame.arg(0, in: self).description
            let target = frame.arg(1, in: self).description
            let replacement = frame.arg(2, in: self).description
            guard !target.isEmpty else { return .text(source) }
            if frame.count == 4, let instNum = frame.arg(3, in: self).numericValue {
                guard let targetOccurrence = Self.safeInt(instNum), targetOccurrence >= 1 else { return .error("#VALUE!") }
                var currentOccurrence = 0
                var result = ""
                var searchStart = source.startIndex
                while let range = source.range(of: target, range: searchStart..<source.endIndex) {
                    currentOccurrence += 1
                    result += source[searchStart..<range.lowerBound]
                    if currentOccurrence == targetOccurrence {
                        result += replacement
                        result += source[range.upperBound..<source.endIndex]
                        return .text(result)
                    } else {
                        result += target
                    }
                    searchStart = range.upperBound
                }
                return .text(source)
            }
            return .text(source.replacingOccurrences(of: target, with: replacement))
        case "FIND":
            guard (2...3).contains(frame.count) else { return .error("#VALUE!") }
            let needle = frame.arg(0, in: self).description
            let haystack = frame.arg(1, in: self).description
            let startOffset: Int
            if frame.count == 3 {
                guard let sNum = frame.arg(2, in: self).numericValue, let sInt = Self.safeInt(sNum), sInt >= 1 else {
                    return .error("#VALUE!")
                }
                startOffset = sInt - 1
            } else {
                startOffset = 0
            }
            guard startOffset < haystack.count else { return .error("#VALUE!") }
            let startIdx = haystack.index(haystack.startIndex, offsetBy: startOffset)
            guard let foundRange = haystack.range(of: needle, range: startIdx..<haystack.endIndex) else {
                return .error("#VALUE!")
            }
            return .number(Double(haystack.distance(from: haystack.startIndex, to: foundRange.lowerBound) + 1))
        case "REPT":
            guard frame.count == 2,
                  let timesVal = frame.arg(1, in: self).numericValue,
                  let count = Self.safeInt(timesVal), count >= 0 else { return .error("#VALUE!") }
            let str = frame.arg(0, in: self).description
            let (totalChars, overflow) = str.count.multipliedReportingOverflow(by: count)
            guard !overflow, totalChars <= Self.maxReptOutputCharacters else { return .error("#VALUE!") }
            return .text(String(repeating: str, count: count))
        case "EXACT":
            guard frame.count == 2 else { return .error("#VALUE!") }
            return .boolean(frame.arg(0, in: self).description == frame.arg(1, in: self).description)
        case "VALUE":
            guard frame.count == 1 else { return .error("#VALUE!") }
            let cleaned = frame.arg(0, in: self).description
                .trimmingCharacters(in: .whitespacesAndNewlines)
                .replacingOccurrences(of: ",", with: "")
                .replacingOccurrences(of: "₩", with: "")
                .replacingOccurrences(of: "$", with: "")
            guard let num = Double(cleaned), num.isFinite else { return .error("#VALUE!") }
            return .number(num)
        case "TEXT":
            guard frame.count == 2 else { return .error("#VALUE!") }
            let val = frame.arg(0, in: self)
            let pattern = frame.arg(1, in: self).description
            guard let num = val.numericValue, num.isFinite else { return .text(val.description) }
            if pattern.contains("%") {
                return .text(String(format: "%.0f%%", num * 100.0))
            } else if let dotIndex = pattern.firstIndex(of: ".") {
                let decimals = pattern.distance(from: pattern.index(after: dotIndex), to: pattern.endIndex)
                return .text(String(format: "%.\(min(10, max(0, decimals)))f", num))
            }
            return .text(String(format: "%.0f", num))
        default:
            return nil
        }
    }

    // MARK: - 6. Financial & Date Functions

    private func evaluateFinancialAndDateFunction(name: String, frame: inout EvalFrame) -> EvaluatedCellValue? {
        switch name {
        case "PMT":
            guard (3...5).contains(frame.count),
                  let rate = frame.arg(0, in: self).numericValue,
                  let nper = frame.arg(1, in: self).numericValue,
                  let pv = frame.arg(2, in: self).numericValue,
                  nper != 0.0 else { return .error("#VALUE!") }
            let fv = frame.count >= 4 ? (frame.arg(3, in: self).numericValue ?? 0.0) : 0.0
            let type = frame.count == 5 ? (frame.arg(4, in: self).numericValue ?? 0.0) : 0.0
            if rate == 0.0 {
                let res = -(pv + fv) / nper
                return res.isFinite ? .number(res) : .error("#NUM!")
            }
            let factor = pow(1.0 + rate, nper)
            let denom = (1.0 + rate * type) * (factor - 1.0)
            guard factor.isFinite, denom != 0.0 else { return .error("#NUM!") }
            let payment = -rate * (pv * factor + fv) / denom
            return payment.isFinite ? .number(payment) : .error("#NUM!")

        case "FV":
            guard (3...5).contains(frame.count),
                  let rate = frame.arg(0, in: self).numericValue,
                  let nper = frame.arg(1, in: self).numericValue,
                  let pmt = frame.arg(2, in: self).numericValue else { return .error("#VALUE!") }
            let pv = frame.count >= 4 ? (frame.arg(3, in: self).numericValue ?? 0.0) : 0.0
            let type = frame.count == 5 ? (frame.arg(4, in: self).numericValue ?? 0.0) : 0.0
            if rate == 0.0 {
                let res = -(pv + pmt * nper)
                return res.isFinite ? .number(res) : .error("#NUM!")
            }
            let factor = pow(1.0 + rate, nper)
            let futureVal = -(pv * factor + pmt * (1.0 + rate * type) * (factor - 1.0) / rate)
            return futureVal.isFinite ? .number(futureVal) : .error("#NUM!")

        case "PV":
            guard (3...5).contains(frame.count),
                  let rate = frame.arg(0, in: self).numericValue,
                  let nper = frame.arg(1, in: self).numericValue,
                  let pmt = frame.arg(2, in: self).numericValue else { return .error("#VALUE!") }
            let fv = frame.count >= 4 ? (frame.arg(3, in: self).numericValue ?? 0.0) : 0.0
            let type = frame.count == 5 ? (frame.arg(4, in: self).numericValue ?? 0.0) : 0.0
            if rate == 0.0 {
                let res = -(fv + pmt * nper)
                return res.isFinite ? .number(res) : .error("#NUM!")
            }
            let factor = pow(1.0 + rate, nper)
            guard factor.isFinite, factor != 0.0 else { return .error("#NUM!") }
            let presentVal = -(fv + pmt * (1.0 + rate * type) * (factor - 1.0) / rate) / factor
            return presentVal.isFinite ? .number(presentVal) : .error("#NUM!")

        case "NPV":
            guard frame.count >= 2, let rate = frame.arg(0, in: self).numericValue, rate != -1.0 else {
                return .error("#VALUE!")
            }
            switch frame.resolve(1..<frame.count, in: self) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let cashFlows = vals.compactMap { $0.numericValue }
                var npv = 0.0
                for (idx, cf) in cashFlows.enumerated() {
                    npv += cf / pow(1.0 + rate, Double(idx + 1))
                }
                return npv.isFinite ? .number(npv) : .error("#NUM!")
            }

        case "DATE":
            guard frame.count == 3,
                  let yNum = frame.arg(0, in: self).numericValue,
                  let mNum = frame.arg(1, in: self).numericValue,
                  let dNum = frame.arg(2, in: self).numericValue,
                  let y = Self.safeInt(yNum), let m = Self.safeInt(mNum), let d = Self.safeInt(dNum),
                  let date = Self.makeUTCDate(year: y, month: m, day: d) else { return .error("#VALUE!") }
            let comps = Self.utcCalendar.dateComponents([.year, .month, .day], from: date)
            return .text(String(format: "%04d-%02d-%02d", comps.year ?? 0, comps.month ?? 1, comps.day ?? 1))

        case "YEAR":
            guard frame.count == 1, let date = Self.parseDateValue(frame.arg(0, in: self)) else { return .error("#VALUE!") }
            return .number(Double(Self.utcCalendar.component(.year, from: date)))

        case "MONTH":
            guard frame.count == 1, let date = Self.parseDateValue(frame.arg(0, in: self)) else { return .error("#VALUE!") }
            return .number(Double(Self.utcCalendar.component(.month, from: date)))

        case "DAY":
            guard frame.count == 1, let date = Self.parseDateValue(frame.arg(0, in: self)) else { return .error("#VALUE!") }
            return .number(Double(Self.utcCalendar.component(.day, from: date)))

        case "DAYS":
            guard frame.count == 2,
                  let endDate = Self.parseDateValue(frame.arg(0, in: self)),
                  let startDate = Self.parseDateValue(frame.arg(1, in: self)) else { return .error("#VALUE!") }
            return .number(Double(Self.utcCalendar.dateComponents([.day], from: startDate, to: endDate).day ?? 0))

        default:
            return nil
        }
    }

    // MARK: - Helpers

    private static func safeInt(_ value: Double) -> Int? {
        guard value.isFinite, value > Double(Int.min), value < Double(Int.max) else {
            return nil
        }
        return Int(value)
    }

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

    private static var utcCalendar: Calendar {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(secondsFromGMT: 0)!
        return cal
    }

    private static func makeUTCDate(year: Int, month: Int, day: Int) -> Date? {
        var comps = DateComponents()
        comps.year = year
        comps.month = month
        comps.day = day
        comps.hour = 0
        comps.minute = 0
        comps.second = 0
        return utcCalendar.date(from: comps)
    }

    private static func parseDateValue(_ value: EvaluatedCellValue) -> Date? {
        let raw = value.description.trimmingCharacters(in: .whitespacesAndNewlines)
        let separators = CharacterSet(charactersIn: "-/.")
        let parts = raw.components(separatedBy: separators)
        if parts.count == 3,
           let y = Int(parts[0]),
           let m = Int(parts[1]),
           let d = Int(parts[2]) {
            return makeUTCDate(year: y, month: m, day: d)
        }
        return nil
    }

    private static func cellValuesEqual(_ a: EvaluatedCellValue, _ b: EvaluatedCellValue) -> Bool {
        if let aNum = a.numericValue, let bNum = b.numericValue {
            return aNum == bNum
        }
        return a.description.caseInsensitiveCompare(b.description) == .orderedSame
    }

    private static func matchesCriteriaValue(
        _ value: EvaluatedCellValue,
        criteriaValue critVal: EvaluatedCellValue
    ) -> Bool {
        let critStr = critVal.description.trimmingCharacters(in: .whitespaces)

        for op in [">=", "<=", "<>", "=", ">", "<"] {
            if critStr.hasPrefix(op) {
                let operand = String(critStr.dropFirst(op.count)).trimmingCharacters(in: .whitespaces)
                if let targetNum = Double(operand) {
                    guard let cellNum = value.numericValue else { return op == "<>" }
                    switch op {
                    case ">=": return cellNum >= targetNum
                    case "<=": return cellNum <= targetNum
                    case "<>": return cellNum != targetNum
                    case "=":  return cellNum == targetNum
                    case ">":  return cellNum > targetNum
                    case "<":  return cellNum < targetNum
                    default: return false
                    }
                } else {
                    let cmp = value.description.caseInsensitiveCompare(operand)
                    switch op {
                    case "=":  return cmp == .orderedSame
                    case "<>": return cmp != .orderedSame
                    default:   return false
                    }
                }
            }
        }

        if let critNum = critVal.numericValue, let cellNum = value.numericValue {
            return cellNum == critNum
        }
        return value.description.caseInsensitiveCompare(critStr) == .orderedSame
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
