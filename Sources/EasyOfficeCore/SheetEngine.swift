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

/// Zero-indexed spreadsheet cell coordinate with A1 notation parser and formatter.
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
        let cleaned = a1Notation.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: "$", with: "").uppercased()
        guard !cleaned.isEmpty else {
            throw EasyOfficeError.invalidCellAddress(a1Notation)
        }

        var letters = ""
        var digits = ""
        for scalar in cleaned.unicodeScalars {
            if CharacterSet.uppercaseLetters.contains(scalar) {
                guard digits.isEmpty else {
                    throw EasyOfficeError.invalidCellAddress(a1Notation)
                }
                letters.unicodeScalars.append(scalar)
            } else if CharacterSet.decimalDigits.contains(scalar) {
                digits.unicodeScalars.append(scalar)
            } else {
                throw EasyOfficeError.invalidCellAddress(a1Notation)
            }
        }

        guard !letters.isEmpty, letters.unicodeScalars.count <= 3,
              !digits.isEmpty, let rowOneBased = Int(digits),
              rowOneBased >= 1, (rowOneBased - 1) <= Self.maxZeroBasedRow else {
            throw EasyOfficeError.invalidCellAddress(a1Notation)
        }

        var colValue = 0
        for scalar in letters.unicodeScalars {
            let charIndex = Int(scalar.value) - Int(UnicodeScalar("A").value) + 1
            let (mulVal, mulOverflow) = colValue.multipliedReportingOverflow(by: 26)
            let (addVal, addOverflow) = mulVal.addingReportingOverflow(charIndex)
            guard !mulOverflow, !addOverflow else {
                throw EasyOfficeError.invalidCellAddress(a1Notation)
            }
            colValue = addVal
        }

        let zeroBasedCol = colValue - 1
        guard zeroBasedCol >= 0, zeroBasedCol <= Self.maxZeroBasedColumn else {
            throw EasyOfficeError.invalidCellAddress(a1Notation)
        }

        self.column = zeroBasedCol
        self.row = rowOneBased - 1
    }

    public static func columnLabel(for zeroBasedColumn: Int) -> String {
        var index = zeroBasedColumn + 1
        var chars: [Character] = []
        while index > 0 {
            let remainder = (index - 1) % 26
            let scalarValue = UInt32(UnicodeScalar("A").value) + UInt32(remainder)
            if let scalar = UnicodeScalar(scalarValue) {
                chars.insert(Character(scalar), at: 0)
            }
            index = (index - 1) / 26
        }
        return String(chars)
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

/// Worksheet model storing raw cell inputs (literals or `=...` formulas) and evaluating deterministically.
public struct WorksheetModel: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var name: String
    public var rawCells: [String: String] // Keyed by A1 notation

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

    public init(id: UUID = UUID(), name: String, rawCells: [String: String] = [:]) {
        self.id = id
        self.name = name
        self.rawCells = rawCells
    }

    public mutating func setCell(_ a1: String, rawValue: String) throws {
        let normalized = try CellAddress(a1Notation: a1).a1Notation
        if rawValue.isEmpty {
            rawCells.removeValue(forKey: normalized)
        } else {
            rawCells[normalized] = rawValue
        }
    }

    public func rawValue(at a1: String) -> String {
        guard let normalized = try? CellAddress(a1Notation: a1).a1Notation else { return "" }
        return rawCells[normalized] ?? ""
    }

    public func evaluate(at a1: String) -> EvaluatedCellValue {
        var visited: Set<String> = []
        return evaluateInternal(at: a1, visited: &visited)
    }

    public func evaluate(at packed: PackedCellCoordinate) -> EvaluatedCellValue {
        let addr = CellAddress(packed: packed)
        return evaluate(at: addr.a1Notation)
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
        var visited: Set<String> = []
        return evaluateGridInternal(in: range, visited: &visited)
    }

    private func evaluateGridInternal(in range: CellRange, visited: inout Set<String>) -> [[EvaluatedCellValue]] {
        let minRow = min(range.start.row, range.end.row)
        let maxRow = max(range.start.row, range.end.row)
        let minCol = min(range.start.column, range.end.column)
        let maxCol = max(range.start.column, range.end.column)

        var grid: [[EvaluatedCellValue]] = []
        for r in minRow...maxRow {
            var rowVals: [EvaluatedCellValue] = []
            for c in minCol...maxCol {
                if let addr = try? CellAddress(column: c, row: r) {
                    rowVals.append(evaluateInternal(at: addr.a1Notation, visited: &visited))
                }
            }
            grid.append(rowVals)
        }
        return grid
    }

    private func evaluateInternal(at a1: String, visited: inout Set<String>) -> EvaluatedCellValue {
        guard let address = try? CellAddress(a1Notation: a1) else {
            return .error("#REF!")
        }
        let key = address.a1Notation
        if visited.contains(key) {
            return .error("#REF!") // Circular reference detected
        }
        guard let raw = rawCells[key], !raw.isEmpty else {
            return .empty
        }

        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.hasPrefix("=") else {
            if let num = Double(trimmed) {
                return .number(num)
            }
            if trimmed.caseInsensitiveCompare("TRUE") == .orderedSame {
                return .boolean(true)
            }
            if trimmed.caseInsensitiveCompare("FALSE") == .orderedSame {
                return .boolean(false)
            }
            return .text(raw)
        }

        visited.insert(key)
        defer { visited.remove(key) }

        let expression = String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces)
        return evaluateFormulaExpression(expression, visited: &visited)
    }

    private func evaluateFormulaExpression(_ expr: String, visited: inout Set<String>) -> EvaluatedCellValue {
        let trimmed = expr.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return .empty }

        // Top-level comparison expression (e.g. A1>=10, B1="서울")
        if let cmpResult = evaluateTopLevelComparison(trimmed, visited: &visited) {
            return cmpResult
        }

        // Binary arithmetic: +, -, *, /
        if let arithResult = evaluateBinaryArithmetic(trimmed, visited: &visited) {
            return arithResult
        }

        // Parenthesized sub-expression: (expr)
        if trimmed.hasPrefix("(") && trimmed.hasSuffix(")") && Self.matchingCloseIndex(in: trimmed, openAt: trimmed.startIndex) == trimmed.index(before: trimmed.endIndex) {
            let inner = String(trimmed.dropFirst().dropLast())
            return evaluateFormulaExpression(inner, visited: &visited)
        }

        // Function call pattern: FUNCNAME(args...) where the closing ')' matches the opening '('
        if let openParen = trimmed.firstIndex(of: "("),
           trimmed.hasSuffix(")"),
           Self.matchingCloseIndex(in: trimmed, openAt: openParen) == trimmed.index(before: trimmed.endIndex) {
            let funcName = String(trimmed[..<openParen]).trimmingCharacters(in: .whitespaces).uppercased()
            if !funcName.isEmpty && funcName.unicodeScalars.allSatisfy({ CharacterSet.alphanumerics.contains($0) }) {
                let innerStart = trimmed.index(after: openParen)
                let innerEnd = trimmed.index(before: trimmed.endIndex)
                let argString = innerStart <= innerEnd ? String(trimmed[innerStart..<innerEnd]) : ""
                let args = Self.splitTopLevelArguments(argString)
                return evaluateFunction(name: funcName, arguments: args, visited: &visited)
            }
        }

        // Quoted string literal
        if trimmed.hasPrefix("\"") && trimmed.hasSuffix("\"") && trimmed.count >= 2 {
            let start = trimmed.index(after: trimmed.startIndex)
            let end = trimmed.index(before: trimmed.endIndex)
            return .text(String(trimmed[start..<end]))
        }

        // Boolean literal
        if trimmed.caseInsensitiveCompare("TRUE") == .orderedSame {
            return .boolean(true)
        }
        if trimmed.caseInsensitiveCompare("FALSE") == .orderedSame {
            return .boolean(false)
        }

        // Numeric literal
        if let number = Double(trimmed) {
            return .number(number)
        }

        // Direct cell reference
        if let addr = try? CellAddress(a1Notation: trimmed) {
            let resolved = evaluateInternal(at: addr.a1Notation, visited: &visited)
            if case .empty = resolved {
                return .number(0.0)
            }
            return resolved
        }

        return .error("#VALUE!")
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

    private func evaluateTopLevelComparison(_ expr: String, visited: inout Set<String>) -> EvaluatedCellValue? {
        let chars = Array(expr)
        var depth = 0
        var inQuotes = false
        var i = 0
        while i < chars.count {
            let ch = chars[i]
            if ch == "\"" { inQuotes.toggle() }
            if !inQuotes {
                if ch == "(" { depth += 1 }
                else if ch == ")" { depth -= 1 }
                else if depth == 0 {
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
                        let lVal = evaluateFormulaExpression(leftStr, visited: &visited)
                        if case .error = lVal { return lVal }
                        let rVal = evaluateFormulaExpression(rightStr, visited: &visited)
                        if case .error = rVal { return rVal }

                        if let lNum = lVal.numericValue, let rNum = rVal.numericValue {
                            switch op {
                            case ">=": return .boolean(lNum >= rNum)
                            case "<=": return .boolean(lNum <= rNum)
                            case "<>": return .boolean(lNum != rNum)
                            case "=":  return .boolean(lNum == rNum)
                            case ">":  return .boolean(lNum > rNum)
                            case "<":  return .boolean(lNum < rNum)
                            default: break
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
                            default: break
                            }
                        }
                    }
                }
            }
            i += 1
        }
        return nil
    }

    private func evaluateBinaryArithmetic(_ expr: String, visited: inout Set<String>) -> EvaluatedCellValue? {
        for operators: [Character] in [["+", "-"], ["*", "/"]] {
            var depth = 0
            var inQuotes = false
            let chars = Array(expr)
            for idx in stride(from: chars.count - 1, through: 0, by: -1) {
                let ch = chars[idx]
                if ch == "\"" { inQuotes.toggle() }
                if inQuotes { continue }
                if ch == ")" { depth += 1 }
                else if ch == "(" { depth -= 1 }
                else if depth == 0 && idx > 0 && operators.contains(ch) {
                    let lhsStr = String(chars[0..<idx]).trimmingCharacters(in: .whitespaces)
                    let rhsStr = String(chars[(idx + 1)...]).trimmingCharacters(in: .whitespaces)
                    if lhsStr.isEmpty || rhsStr.isEmpty { continue }
                    if let prevChar = lhsStr.last, ["+", "-", "*", "/", "(", ",", "=", "<", ">"].contains(prevChar) {
                        continue // Unary sign, not binary operator
                    }

                    let lhsVal = evaluateFormulaExpression(lhsStr, visited: &visited)
                    if case .error = lhsVal { return lhsVal }
                    let rhsVal = evaluateFormulaExpression(rhsStr, visited: &visited)
                    if case .error = rhsVal { return rhsVal }

                    guard let lNum = lhsVal.numericValue, let rNum = rhsVal.numericValue else {
                        return .error("#VALUE!")
                    }
                    switch ch {
                    case "+": return .number(lNum + rNum)
                    case "-": return .number(lNum - rNum)
                    case "*": return .number(lNum * rNum)
                    case "/":
                        guard rNum != 0.0 else { return .error("#DIV/0!") }
                        return .number(lNum / rNum)
                    default: break
                    }
                }
            }
        }
        return nil
    }

    private enum FormulaResolutionResult {
        case success([EvaluatedCellValue])
        case failure(String)
    }

    private func evaluateFunction(name: String, arguments: [String], visited: inout Set<String>) -> EvaluatedCellValue {
        func resolveValues(_ args: [String]) -> FormulaResolutionResult {
            var values: [EvaluatedCellValue] = []
            for arg in args {
                let trimmed = arg.trimmingCharacters(in: .whitespaces)
                if trimmed.contains(":") {
                    guard let range = try? CellRange(notation: trimmed) else {
                        return .failure("#REF!")
                    }
                    for addr in range.expandedAddresses() {
                        let v = evaluateInternal(at: addr.a1Notation, visited: &visited)
                        if case let .error(code) = v { return .failure(code) }
                        values.append(v)
                    }
                } else {
                    let v = evaluateFormulaExpression(trimmed, visited: &visited)
                    if case let .error(code) = v { return .failure(code) }
                    values.append(v)
                }
            }
            return .success(values)
        }

        func evalSingle(_ index: Int) -> EvaluatedCellValue {
            guard arguments.indices.contains(index) else { return .error("#VALUE!") }
            return evaluateFormulaExpression(arguments[index], visited: &visited)
        }

        switch name {
        // MARK: Math & Trig
        case "SUM":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                return .number(vals.compactMap { $0.numericValue }.reduce(0.0, +))
            }
        case "AVERAGE":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let nums = vals.compactMap { $0.numericValue }
                guard !nums.isEmpty else { return .error("#DIV/0!") }
                return .number(nums.reduce(0.0, +) / Double(nums.count))
            }
        case "MIN":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                return .number(vals.compactMap { $0.numericValue }.min() ?? 0.0)
            }
        case "MAX":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                return .number(vals.compactMap { $0.numericValue }.max() ?? 0.0)
            }
        case "COUNT":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let count = vals.filter {
                    if case .number = $0 { return true }
                    return false
                }.count
                return .number(Double(count))
            }
        case "COUNTA":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let count = vals.filter { $0 != EvaluatedCellValue.empty }.count
                return .number(Double(count))
            }
        case "COUNTBLANK":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let count = vals.filter { $0 == .empty || $0 == .text("") }.count
                return .number(Double(count))
            }
        case "ABS":
            guard arguments.count == 1 else { return .error("#VALUE!") }
            let val = evalSingle(0)
            if case .error = val { return val }
            guard let num = val.numericValue else { return .error("#VALUE!") }
            return .number(abs(num))
        case "ROUND":
            guard arguments.count == 2 else { return .error("#VALUE!") }
            guard let num = evalSingle(0).numericValue, let digits = evalSingle(1).numericValue else {
                return .error("#VALUE!")
            }
            let factor = pow(10.0, Double(Int(digits)))
            return .number((num * factor).rounded() / factor)
        case "ROUNDUP":
            guard arguments.count == 2 else { return .error("#VALUE!") }
            guard let num = evalSingle(0).numericValue, let digits = evalSingle(1).numericValue else {
                return .error("#VALUE!")
            }
            let factor = pow(10.0, Double(Int(digits)))
            let scaled = num * factor
            let rounded = scaled >= 0 ? ceil(scaled) : floor(scaled)
            return .number(rounded / factor)
        case "ROUNDDOWN":
            guard arguments.count == 2 else { return .error("#VALUE!") }
            guard let num = evalSingle(0).numericValue, let digits = evalSingle(1).numericValue else {
                return .error("#VALUE!")
            }
            let factor = pow(10.0, Double(Int(digits)))
            return .number(trunc(num * factor) / factor)
        case "INT":
            guard arguments.count == 1, let num = evalSingle(0).numericValue else { return .error("#VALUE!") }
            return .number(floor(num))
        case "TRUNC":
            guard (1...2).contains(arguments.count), let num = evalSingle(0).numericValue else { return .error("#VALUE!") }
            let digits = arguments.count == 2 ? (evalSingle(1).numericValue ?? 0.0) : 0.0
            let factor = pow(10.0, Double(Int(digits)))
            return .number(trunc(num * factor) / factor)
        case "MOD":
            guard arguments.count == 2,
                  let num = evalSingle(0).numericValue,
                  let divisor = evalSingle(1).numericValue else { return .error("#VALUE!") }
            guard divisor != 0.0 else { return .error("#DIV/0!") }
            return .number(num - divisor * floor(num / divisor))
        case "SQRT":
            guard arguments.count == 1, let num = evalSingle(0).numericValue else { return .error("#VALUE!") }
            guard num >= 0.0 else { return .error("#NUM!") }
            return .number(sqrt(num))
        case "POWER":
            guard arguments.count == 2,
                  let base = evalSingle(0).numericValue,
                  let expVal = evalSingle(1).numericValue else { return .error("#VALUE!") }
            return .number(pow(base, expVal))
        case "PRODUCT":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let nums = vals.compactMap { $0.numericValue }
                return .number(nums.isEmpty ? 0.0 : nums.reduce(1.0, *))
            }
        case "SUMPRODUCT":
            guard !arguments.isEmpty else { return .error("#VALUE!") }
            var vectors: [[Double]] = []
            for arg in arguments {
                switch resolveValues([arg]) {
                case let .failure(err): return .error(err)
                case let .success(vals):
                    vectors.append(vals.map { $0.numericValue ?? 0.0 })
                }
            }
            guard let length = vectors.first?.count, length > 0, vectors.allSatisfy({ $0.count == length }) else {
                return .error("#VALUE!")
            }
            var total = 0.0
            for i in 0..<length {
                var rowProd = 1.0
                for vec in vectors {
                    rowProd *= vec[i]
                }
                total += rowProd
            }
            return .number(total)
        case "SUMIF":
            guard (2...3).contains(arguments.count) else { return .error("#VALUE!") }
            guard case let .success(criteriaCells) = resolveValues([arguments[0]]) else { return .error("#REF!") }
            let targetCells: [EvaluatedCellValue]
            if arguments.count == 3 {
                guard case let .success(sumCells) = resolveValues([arguments[2]]),
                      sumCells.count == criteriaCells.count else { return .error("#VALUE!") }
                targetCells = sumCells
            } else {
                targetCells = criteriaCells
            }
            var sum = 0.0
            for idx in criteriaCells.indices {
                if matchesCriteria(criteriaCells[idx], criteriaRaw: arguments[1], visited: &visited),
                   let num = targetCells[idx].numericValue {
                    sum += num
                }
            }
            return .number(sum)
        case "COUNTIF":
            guard arguments.count == 2 else { return .error("#VALUE!") }
            guard case let .success(criteriaCells) = resolveValues([arguments[0]]) else { return .error("#REF!") }
            let count = criteriaCells.filter { matchesCriteria($0, criteriaRaw: arguments[1], visited: &visited) }.count
            return .number(Double(count))
        case "AVERAGEIF":
            guard (2...3).contains(arguments.count) else { return .error("#VALUE!") }
            guard case let .success(criteriaCells) = resolveValues([arguments[0]]) else { return .error("#REF!") }
            let targetCells: [EvaluatedCellValue]
            if arguments.count == 3 {
                guard case let .success(avgCells) = resolveValues([arguments[2]]),
                      avgCells.count == criteriaCells.count else { return .error("#VALUE!") }
                targetCells = avgCells
            } else {
                targetCells = criteriaCells
            }
            var matchedNumbers: [Double] = []
            for idx in criteriaCells.indices {
                if matchesCriteria(criteriaCells[idx], criteriaRaw: arguments[1], visited: &visited),
                   let num = targetCells[idx].numericValue {
                    matchedNumbers.append(num)
                }
            }
            guard !matchedNumbers.isEmpty else { return .error("#DIV/0!") }
            return .number(matchedNumbers.reduce(0.0, +) / Double(matchedNumbers.count))
        case "CEILING":
            guard (1...2).contains(arguments.count), let num = evalSingle(0).numericValue else { return .error("#VALUE!") }
            let sig = arguments.count == 2 ? (evalSingle(1).numericValue ?? 1.0) : 1.0
            guard sig != 0.0 else { return .number(0.0) }
            return .number(ceil(num / sig) * sig)
        case "FLOOR":
            guard (1...2).contains(arguments.count), let num = evalSingle(0).numericValue else { return .error("#VALUE!") }
            let sig = arguments.count == 2 ? (evalSingle(1).numericValue ?? 1.0) : 1.0
            guard sig != 0.0 else { return .error("#DIV/0!") }
            return .number(floor(num / sig) * sig)
        case "SIGN":
            guard arguments.count == 1, let num = evalSingle(0).numericValue else { return .error("#VALUE!") }
            return .number(num > 0 ? 1.0 : (num < 0 ? -1.0 : 0.0))
        case "LN":
            guard arguments.count == 1, let num = evalSingle(0).numericValue else { return .error("#VALUE!") }
            guard num > 0 else { return .error("#NUM!") }
            return .number(log(num))
        case "LOG10":
            guard arguments.count == 1, let num = evalSingle(0).numericValue else { return .error("#VALUE!") }
            guard num > 0 else { return .error("#NUM!") }
            return .number(log10(num))
        case "EXP":
            guard arguments.count == 1, let num = evalSingle(0).numericValue else { return .error("#VALUE!") }
            return .number(exp(num))

        // MARK: Statistical
        case "MEDIAN":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let sorted = vals.compactMap { $0.numericValue }.sorted()
                guard !sorted.isEmpty else { return .error("#NUM!") }
                let mid = sorted.count / 2
                if sorted.count % 2 == 1 {
                    return .number(sorted[mid])
                } else {
                    return .number((sorted[mid - 1] + sorted[mid]) / 2.0)
                }
            }
        case "MODE":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let nums = vals.compactMap { $0.numericValue }
                guard !nums.isEmpty else { return .error("#N/A") }
                var counts: [Double: Int] = [:]
                for n in nums { counts[n, default: 0] += 1 }
                guard let best = nums.max(by: { counts[$0, default: 0] < counts[$1, default: 0] }),
                      (counts[best] ?? 0) > 1 else {
                    return .number(nums[0])
                }
                return .number(best)
            }
        case "VAR":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let nums = vals.compactMap { $0.numericValue }
                guard nums.count >= 2 else { return .error("#DIV/0!") }
                let mean = nums.reduce(0.0, +) / Double(nums.count)
                let sumSq = nums.reduce(0.0) { $0 + ($1 - mean) * ($1 - mean) }
                return .number(sumSq / Double(nums.count - 1))
            }
        case "STDEV":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let nums = vals.compactMap { $0.numericValue }
                guard nums.count >= 2 else { return .error("#DIV/0!") }
                let mean = nums.reduce(0.0, +) / Double(nums.count)
                let sumSq = nums.reduce(0.0) { $0 + ($1 - mean) * ($1 - mean) }
                return .number(sqrt(sumSq / Double(nums.count - 1)))
            }
        case "LARGE":
            guard arguments.count == 2,
                  case let .success(vals) = resolveValues([arguments[0]]),
                  let kVal = evalSingle(1).numericValue else { return .error("#VALUE!") }
            let sortedDesc = vals.compactMap { $0.numericValue }.sorted(by: >)
            let k = Int(kVal)
            guard k >= 1, k <= sortedDesc.count else { return .error("#NUM!") }
            return .number(sortedDesc[k - 1])
        case "SMALL":
            guard arguments.count == 2,
                  case let .success(vals) = resolveValues([arguments[0]]),
                  let kVal = evalSingle(1).numericValue else { return .error("#VALUE!") }
            let sortedAsc = vals.compactMap { $0.numericValue }.sorted(by: <)
            let k = Int(kVal)
            guard k >= 1, k <= sortedAsc.count else { return .error("#NUM!") }
            return .number(sortedAsc[k - 1])
        case "RANK":
            guard (2...3).contains(arguments.count),
                  let target = evalSingle(0).numericValue,
                  case let .success(vals) = resolveValues([arguments[1]]) else { return .error("#VALUE!") }
            let order = arguments.count == 3 ? Int(evalSingle(2).numericValue ?? 0.0) : 0
            let nums = vals.compactMap { $0.numericValue }
            guard nums.contains(target) else { return .error("#N/A") }
            let rank = order == 0
                ? 1 + nums.filter { $0 > target }.count
                : 1 + nums.filter { $0 < target }.count
            return .number(Double(rank))

        // MARK: Logical
        case "TRUE":
            return .boolean(true)
        case "FALSE":
            return .boolean(false)
        case "IF":
            guard (2...3).contains(arguments.count) else { return .error("#VALUE!") }
            let conditionPassed = evaluateCondition(arguments[0], visited: &visited)
            if conditionPassed {
                return evaluateFormulaExpression(arguments[1], visited: &visited)
            } else if arguments.count == 3 {
                return evaluateFormulaExpression(arguments[2], visited: &visited)
            } else {
                return .boolean(false)
            }
        case "IFS":
            guard arguments.count >= 2, arguments.count % 2 == 0 else { return .error("#VALUE!") }
            for i in stride(from: 0, to: arguments.count, by: 2) {
                if evaluateCondition(arguments[i], visited: &visited) {
                    return evaluateFormulaExpression(arguments[i + 1], visited: &visited)
                }
            }
            return .error("#N/A")
        case "AND":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                guard !vals.isEmpty else { return .error("#VALUE!") }
                return .boolean(vals.allSatisfy { $0.booleanValue == true })
            }
        case "OR":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                guard !vals.isEmpty else { return .error("#VALUE!") }
                return .boolean(vals.contains { $0.booleanValue == true })
            }
        case "NOT":
            guard arguments.count == 1 else { return .error("#VALUE!") }
            let v = evalSingle(0)
            if case .error = v { return v }
            return .boolean(!(v.booleanValue ?? false))
        case "XOR":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                guard !vals.isEmpty else { return .error("#VALUE!") }
                let trueCount = vals.filter { $0.booleanValue == true }.count
                return .boolean(trueCount % 2 == 1)
            }
        case "IFERROR":
            guard arguments.count == 2 else { return .error("#VALUE!") }
            let primary = evalSingle(0)
            if case .error = primary {
                return evalSingle(1)
            }
            return primary
        case "SWITCH":
            guard arguments.count >= 3 else { return .error("#VALUE!") }
            let target = evalSingle(0)
            if case .error = target { return target }
            let pairLimit = arguments.count - (arguments.count % 2 == 0 ? 1 : 0)
            var idx = 1
            while idx + 1 <= pairLimit {
                let candidate = evalSingle(idx)
                if candidate == target || candidate.description.caseInsensitiveCompare(target.description) == .orderedSame {
                    return evalSingle(idx + 1)
                }
                idx += 2
            }
            if arguments.count % 2 == 0 {
                return evalSingle(arguments.count - 1)
            }
            return .error("#N/A")

        // MARK: Lookup & Reference
        case "VLOOKUP":
            guard (3...4).contains(arguments.count),
                  let range = try? CellRange(notation: arguments[1]),
                  let colIndexVal = evalSingle(2).numericValue else { return .error("#VALUE!") }
            let lookupVal = evalSingle(0)
            let isApprox = arguments.count == 4 ? (evalSingle(3).booleanValue ?? false) : false
            let grid = evaluateGridInternal(in: range, visited: &visited)
            let colIdx = Int(colIndexVal) - 1
            guard let firstRow = grid.first, colIdx >= 0, colIdx < firstRow.count else {
                return .error("#REF!")
            }
            // Exact match first
            for row in grid {
                if cellValuesEqual(row[0], lookupVal) {
                    return row[colIdx]
                }
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
            guard (3...4).contains(arguments.count),
                  let range = try? CellRange(notation: arguments[1]),
                  let rowIndexVal = evalSingle(2).numericValue else { return .error("#VALUE!") }
            let lookupVal = evalSingle(0)
            let isApprox = arguments.count == 4 ? (evalSingle(3).booleanValue ?? false) : false
            let grid = evaluateGridInternal(in: range, visited: &visited)
            let rowIdx = Int(rowIndexVal) - 1
            guard rowIdx >= 0, rowIdx < grid.count, let headerRow = grid.first else {
                return .error("#REF!")
            }
            for col in headerRow.indices {
                if cellValuesEqual(headerRow[col], lookupVal) {
                    return grid[rowIdx][col]
                }
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
            guard (3...4).contains(arguments.count),
                  case let .success(lookupArr) = resolveValues([arguments[1]]),
                  case let .success(returnArr) = resolveValues([arguments[2]]),
                  lookupArr.count == returnArr.count else { return .error("#VALUE!") }
            let lookupVal = evalSingle(0)
            for idx in lookupArr.indices {
                if cellValuesEqual(lookupArr[idx], lookupVal) {
                    return returnArr[idx]
                }
            }
            if arguments.count == 4 {
                return evalSingle(3)
            }
            return .error("#N/A")

        case "INDEX":
            guard (2...3).contains(arguments.count),
                  let range = try? CellRange(notation: arguments[0]),
                  let rowNumVal = evalSingle(1).numericValue else { return .error("#VALUE!") }
            let colNumVal = arguments.count == 3 ? (evalSingle(2).numericValue ?? 1.0) : 1.0
            let grid = evaluateGridInternal(in: range, visited: &visited)
            let rIdx = Int(rowNumVal) - 1
            let cIdx = Int(colNumVal) - 1
            if grid.count == 1, arguments.count == 2, rIdx >= 0, rIdx < grid[0].count {
                return grid[0][rIdx]
            }
            guard rIdx >= 0, rIdx < grid.count, cIdx >= 0, cIdx < grid[rIdx].count else {
                return .error("#REF!")
            }
            return grid[rIdx][cIdx]

        case "MATCH":
            guard (2...3).contains(arguments.count),
                  case let .success(lookupArr) = resolveValues([arguments[1]]) else { return .error("#VALUE!") }
            let lookupVal = evalSingle(0)
            for (idx, item) in lookupArr.enumerated() {
                if cellValuesEqual(item, lookupVal) {
                    return .number(Double(idx + 1))
                }
            }
            return .error("#N/A")

        case "CHOOSE":
            guard arguments.count >= 2, let idxVal = evalSingle(0).numericValue else { return .error("#VALUE!") }
            let choiceIdx = Int(idxVal)
            guard choiceIdx >= 1, choiceIdx < arguments.count else { return .error("#VALUE!") }
            return evalSingle(choiceIdx)

        case "ROW":
            guard arguments.count == 1 else { return .error("#VALUE!") }
            let ref = arguments[0].split(separator: ":").first.map(String.init) ?? arguments[0]
            guard let addr = try? CellAddress(a1Notation: ref) else { return .error("#REF!") }
            return .number(Double(addr.row + 1))

        case "COLUMN":
            guard arguments.count == 1 else { return .error("#VALUE!") }
            let ref = arguments[0].split(separator: ":").first.map(String.init) ?? arguments[0]
            guard let addr = try? CellAddress(a1Notation: ref) else { return .error("#REF!") }
            return .number(Double(addr.column + 1))

        // MARK: Text
        case "CONCAT", "CONCATENATE":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                return .text(vals.map { $0.description }.joined())
            }
        case "LEFT":
            guard (1...2).contains(arguments.count) else { return .error("#VALUE!") }
            let text = evalSingle(0).description
            let count = arguments.count == 2 ? max(0, Int(evalSingle(1).numericValue ?? 1.0)) : 1
            return .text(String(text.prefix(count)))
        case "RIGHT":
            guard (1...2).contains(arguments.count) else { return .error("#VALUE!") }
            let text = evalSingle(0).description
            let count = arguments.count == 2 ? max(0, Int(evalSingle(1).numericValue ?? 1.0)) : 1
            return .text(String(text.suffix(count)))
        case "MID":
            guard arguments.count == 3,
                  let startVal = evalSingle(1).numericValue,
                  let lenVal = evalSingle(2).numericValue else { return .error("#VALUE!") }
            let startIdx = Int(startVal) - 1
            let length = Int(lenVal)
            guard startIdx >= 0, length >= 0 else { return .error("#VALUE!") }
            let chars = Array(evalSingle(0).description)
            guard startIdx < chars.count else { return .text("") }
            let endIdx = min(chars.count, startIdx + length)
            return .text(String(chars[startIdx..<endIdx]))
        case "LEN":
            guard arguments.count == 1 else { return .error("#VALUE!") }
            return .number(Double(evalSingle(0).description.count))
        case "LOWER":
            guard arguments.count == 1 else { return .error("#VALUE!") }
            return .text(evalSingle(0).description.lowercased())
        case "UPPER":
            guard arguments.count == 1 else { return .error("#VALUE!") }
            return .text(evalSingle(0).description.uppercased())
        case "PROPER":
            guard arguments.count == 1 else { return .error("#VALUE!") }
            return .text(evalSingle(0).description.capitalized)
        case "TRIM":
            guard arguments.count == 1 else { return .error("#VALUE!") }
            let parts = evalSingle(0).description
                .components(separatedBy: .whitespacesAndNewlines)
                .filter { !$0.isEmpty }
            return .text(parts.joined(separator: " "))
        case "SUBSTITUTE":
            guard (3...4).contains(arguments.count) else { return .error("#VALUE!") }
            let source = evalSingle(0).description
            let target = evalSingle(1).description
            let replacement = evalSingle(2).description
            guard !target.isEmpty else { return .text(source) }
            if arguments.count == 4, let instNum = evalSingle(3).numericValue {
                let targetOccurrence = Int(instNum)
                guard targetOccurrence >= 1 else { return .error("#VALUE!") }
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
            guard (2...3).contains(arguments.count) else { return .error("#VALUE!") }
            let needle = evalSingle(0).description
            let haystack = evalSingle(1).description
            let startOffset = arguments.count == 3 ? max(1, Int(evalSingle(2).numericValue ?? 1.0)) - 1 : 0
            guard startOffset < haystack.count else { return .error("#VALUE!") }
            let startIdx = haystack.index(haystack.startIndex, offsetBy: startOffset)
            guard let foundRange = haystack.range(of: needle, range: startIdx..<haystack.endIndex) else {
                return .error("#VALUE!")
            }
            let pos = haystack.distance(from: haystack.startIndex, to: foundRange.lowerBound) + 1
            return .number(Double(pos))
        case "REPT":
            guard arguments.count == 2, let timesVal = evalSingle(1).numericValue else { return .error("#VALUE!") }
            let count = Int(timesVal)
            guard count >= 0 else { return .error("#VALUE!") }
            return .text(String(repeating: evalSingle(0).description, count: count))
        case "EXACT":
            guard arguments.count == 2 else { return .error("#VALUE!") }
            return .boolean(evalSingle(0).description == evalSingle(1).description)
        case "VALUE":
            guard arguments.count == 1 else { return .error("#VALUE!") }
            let cleaned = evalSingle(0).description
                .trimmingCharacters(in: .whitespacesAndNewlines)
                .replacingOccurrences(of: ",", with: "")
                .replacingOccurrences(of: "₩", with: "")
                .replacingOccurrences(of: "$", with: "")
            guard let num = Double(cleaned) else { return .error("#VALUE!") }
            return .number(num)
        case "TEXT":
            guard arguments.count == 2 else { return .error("#VALUE!") }
            let val = evalSingle(0)
            let pattern = evalSingle(1).description
            guard let num = val.numericValue else { return .text(val.description) }
            if pattern.contains("%") {
                return .text(String(format: "%.0f%%", num * 100.0))
            } else if let dotIndex = pattern.firstIndex(of: ".") {
                let decimals = pattern.distance(from: pattern.index(after: dotIndex), to: pattern.endIndex)
                return .text(String(format: "%.\(max(0, decimals))f", num))
            } else {
                return .text(String(format: "%.0f", num))
            }

        // MARK: Financial
        case "PMT":
            guard (3...5).contains(arguments.count),
                  let rate = evalSingle(0).numericValue,
                  let nper = evalSingle(1).numericValue,
                  let pv = evalSingle(2).numericValue,
                  nper != 0.0 else { return .error("#VALUE!") }
            let fv = arguments.count >= 4 ? (evalSingle(3).numericValue ?? 0.0) : 0.0
            let type = arguments.count == 5 ? (evalSingle(4).numericValue ?? 0.0) : 0.0
            if rate == 0.0 {
                return .number(-(pv + fv) / nper)
            }
            let factor = pow(1.0 + rate, nper)
            let payment = -rate * (pv * factor + fv) / ((1.0 + rate * type) * (factor - 1.0))
            return .number(payment)

        case "FV":
            guard (3...5).contains(arguments.count),
                  let rate = evalSingle(0).numericValue,
                  let nper = evalSingle(1).numericValue,
                  let pmt = evalSingle(2).numericValue else { return .error("#VALUE!") }
            let pv = arguments.count >= 4 ? (evalSingle(3).numericValue ?? 0.0) : 0.0
            let type = arguments.count == 5 ? (evalSingle(4).numericValue ?? 0.0) : 0.0
            if rate == 0.0 {
                return .number(-(pv + pmt * nper))
            }
            let factor = pow(1.0 + rate, nper)
            let futureVal = -(pv * factor + pmt * (1.0 + rate * type) * (factor - 1.0) / rate)
            return .number(futureVal)

        case "PV":
            guard (3...5).contains(arguments.count),
                  let rate = evalSingle(0).numericValue,
                  let nper = evalSingle(1).numericValue,
                  let pmt = evalSingle(2).numericValue else { return .error("#VALUE!") }
            let fv = arguments.count >= 4 ? (evalSingle(3).numericValue ?? 0.0) : 0.0
            let type = arguments.count == 5 ? (evalSingle(4).numericValue ?? 0.0) : 0.0
            if rate == 0.0 {
                return .number(-(fv + pmt * nper))
            }
            let factor = pow(1.0 + rate, nper)
            let presentVal = -(fv + pmt * (1.0 + rate * type) * (factor - 1.0) / rate) / factor
            return .number(presentVal)

        case "NPV":
            guard arguments.count >= 2, let rate = evalSingle(0).numericValue, rate != -1.0 else {
                return .error("#VALUE!")
            }
            switch resolveValues(Array(arguments.dropFirst())) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let cashFlows = vals.compactMap { $0.numericValue }
                var npv = 0.0
                for (idx, cf) in cashFlows.enumerated() {
                    npv += cf / pow(1.0 + rate, Double(idx + 1))
                }
                return .number(npv)
            }

        // MARK: Date & Time
        case "DATE":
            guard arguments.count == 3,
                  let y = evalSingle(0).numericValue,
                  let m = evalSingle(1).numericValue,
                  let d = evalSingle(2).numericValue else { return .error("#VALUE!") }
            guard let date = Self.makeUTCDate(year: Int(y), month: Int(m), day: Int(d)) else {
                return .error("#VALUE!")
            }
            let comps = Self.utcCalendar.dateComponents([.year, .month, .day], from: date)
            return .text(String(format: "%04d-%02d-%02d", comps.year ?? 0, comps.month ?? 1, comps.day ?? 1))

        case "YEAR":
            guard arguments.count == 1, let date = Self.parseDateValue(evalSingle(0)) else { return .error("#VALUE!") }
            return .number(Double(Self.utcCalendar.component(.year, from: date)))

        case "MONTH":
            guard arguments.count == 1, let date = Self.parseDateValue(evalSingle(0)) else { return .error("#VALUE!") }
            return .number(Double(Self.utcCalendar.component(.month, from: date)))

        case "DAY":
            guard arguments.count == 1, let date = Self.parseDateValue(evalSingle(0)) else { return .error("#VALUE!") }
            return .number(Double(Self.utcCalendar.component(.day, from: date)))

        case "DAYS":
            guard arguments.count == 2,
                  let endDate = Self.parseDateValue(evalSingle(0)),
                  let startDate = Self.parseDateValue(evalSingle(1)) else { return .error("#VALUE!") }
            let diff = Self.utcCalendar.dateComponents([.day], from: startDate, to: endDate).day ?? 0
            return .number(Double(diff))

        default:
            return .error("#NAME?")
        }
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

    private func cellValuesEqual(_ a: EvaluatedCellValue, _ b: EvaluatedCellValue) -> Bool {
        if let aNum = a.numericValue, let bNum = b.numericValue {
            return aNum == bNum
        }
        return a.description.caseInsensitiveCompare(b.description) == .orderedSame
    }

    private func matchesCriteria(_ value: EvaluatedCellValue, criteriaRaw: String, visited: inout Set<String>) -> Bool {
        let critVal = evaluateFormulaExpression(criteriaRaw, visited: &visited)
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

    private func evaluateCondition(_ cond: String, visited: inout Set<String>) -> Bool {
        let evaluated = evaluateFormulaExpression(cond, visited: &visited)
        if let flag = evaluated.booleanValue {
            return flag
        }
        return false
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

// MARK: - Pivot Table Engine

/// Aggregation functions supported by `PivotTableEngine`.
public enum PivotAggregationFunction: String, CaseIterable, Sendable, Codable {
    case sum
    case average
    case count
    case min
    case max
}

/// Configuration for grouping and aggregating a worksheet range into a Pivot Table.
public struct PivotTableConfiguration: Sendable, Codable, Equatable {
    public var rowFieldColumnIndex: Int
    public var columnFieldColumnIndex: Int?
    public var valueFieldColumnIndex: Int
    public var aggregation: PivotAggregationFunction

    public init(
        rowFieldColumnIndex: Int,
        columnFieldColumnIndex: Int? = nil,
        valueFieldColumnIndex: Int,
        aggregation: PivotAggregationFunction = .sum
    ) {
        self.rowFieldColumnIndex = rowFieldColumnIndex
        self.columnFieldColumnIndex = columnFieldColumnIndex
        self.valueFieldColumnIndex = valueFieldColumnIndex
        self.aggregation = aggregation
    }
}

/// Computed Pivot Table matrix with row/column keys and grand totals.
public struct PivotTableSummary: Sendable, Codable, Equatable {
    public let rowKeys: [String]
    public let columnKeys: [String]
    public let matrix: [[Double]]
    public let rowTotals: [Double]
    public let columnTotals: [Double]
    public let grandTotal: Double

    public init(
        rowKeys: [String],
        columnKeys: [String],
        matrix: [[Double]],
        rowTotals: [Double],
        columnTotals: [Double],
        grandTotal: Double
    ) {
        self.rowKeys = rowKeys
        self.columnKeys = columnKeys
        self.matrix = matrix
        self.rowTotals = rowTotals
        self.columnTotals = columnTotals
        self.grandTotal = grandTotal
    }
}

/// Deterministic Pivot Table computation engine.
public struct PivotTableEngine: Sendable {
    public static func compute(
        from sheet: WorksheetModel,
        range: CellRange,
        hasHeaderRow: Bool = true,
        configuration: PivotTableConfiguration
    ) -> PivotTableSummary {
        let allRows = sheet.evaluateGrid(in: range)
        let dataRows = hasHeaderRow ? Array(allRows.dropFirst()) : allRows

        var rowKeys: [String] = []
        var colKeys: [String] = []
        var buckets: [String: [String: [Double]]] = [:]
        var rowBuckets: [String: [Double]] = [:]
        var colBuckets: [String: [Double]] = [:]
        var allValues: [Double] = []

        for row in dataRows {
            guard configuration.rowFieldColumnIndex >= 0,
                  configuration.rowFieldColumnIndex < row.count,
                  configuration.valueFieldColumnIndex >= 0,
                  configuration.valueFieldColumnIndex < row.count else {
                continue
            }
            let rKey = row[configuration.rowFieldColumnIndex].description
            guard !rKey.isEmpty else { continue }
            let cKey: String
            if let colField = configuration.columnFieldColumnIndex,
               colField >= 0,
               colField < row.count {
                cKey = row[colField].description
            } else {
                cKey = "Total"
            }

            if !rowKeys.contains(rKey) { rowKeys.append(rKey) }
            if !colKeys.contains(cKey) { colKeys.append(cKey) }

            if let val = row[configuration.valueFieldColumnIndex].numericValue {
                buckets[rKey, default: [:]][cKey, default: []].append(val)
                rowBuckets[rKey, default: []].append(val)
                colBuckets[cKey, default: []].append(val)
                allValues.append(val)
            }
        }

        let matrix: [[Double]] = rowKeys.map { rKey in
            colKeys.map { cKey in
                aggregate(buckets[rKey]?[cKey] ?? [], using: configuration.aggregation)
            }
        }
        let rowTotals = rowKeys.map { aggregate(rowBuckets[$0] ?? [], using: configuration.aggregation) }
        let columnTotals = colKeys.map { aggregate(colBuckets[$0] ?? [], using: configuration.aggregation) }
        let grandTotal = aggregate(allValues, using: configuration.aggregation)

        return PivotTableSummary(
            rowKeys: rowKeys,
            columnKeys: colKeys,
            matrix: matrix,
            rowTotals: rowTotals,
            columnTotals: columnTotals,
            grandTotal: grandTotal
        )
    }

    private static func aggregate(_ values: [Double], using fn: PivotAggregationFunction) -> Double {
        guard !values.isEmpty else { return 0.0 }
        switch fn {
        case .sum:
            return values.reduce(0.0, +)
        case .average:
            return values.reduce(0.0, +) / Double(values.count)
        case .count:
            return Double(values.count)
        case .min:
            return values.min() ?? 0.0
        case .max:
            return values.max() ?? 0.0
        }
    }
}

// MARK: - Sheet Chart Engine

/// Supported spreadsheet chart types.
public enum SheetChartKind: String, CaseIterable, Sendable, Codable {
    case column
    case bar
    case line
    case area
    case pie
    case scatter
    case stackedColumn
}

/// Named numeric data series inside a chart.
public struct SheetChartSeries: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var name: String
    public var values: [Double]

    public init(id: UUID = UUID(), name: String, values: [Double]) {
        self.id = id
        self.name = name
        self.values = values
    }
}

/// Chart model bound to worksheet categories and value series.
public struct SheetChartModel: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var title: String
    public var kind: SheetChartKind
    public var categories: [String]
    public var series: [SheetChartSeries]

    public init(
        id: UUID = UUID(),
        title: String,
        kind: SheetChartKind,
        categories: [String],
        series: [SheetChartSeries]
    ) {
        self.id = id
        self.title = title
        self.kind = kind
        self.categories = categories
        self.series = series
    }
}

/// Builder extracting evaluated categories and numeric series from a `WorksheetModel`.
public struct SheetChartBuilder: Sendable {
    public static func buildChart(
        from sheet: WorksheetModel,
        title: String,
        kind: SheetChartKind,
        categoryRange: CellRange,
        valueRanges: [(name: String, range: CellRange)]
    ) -> SheetChartModel {
        let categories = categoryRange.expandedAddresses().map {
            sheet.evaluate(at: $0.a1Notation).description
        }
        let seriesList = valueRanges.map { entry in
            let nums = entry.range.expandedAddresses().map {
                sheet.evaluate(at: $0.a1Notation).numericValue ?? 0.0
            }
            return SheetChartSeries(name: entry.name, values: nums)
        }
        return SheetChartModel(
            title: title,
            kind: kind,
            categories: categories,
            series: seriesList
        )
    }
}

// MARK: - Conditional Formatting & Data Validation

/// Conditional formatting predicate rules for worksheet ranges.
public enum ConditionalFormatRule: Sendable, Codable, Equatable {
    case greaterThan(Double)
    case lessThan(Double)
    case between(Double, Double)
    case containsText(String)
    case duplicateValues

    public func matches(_ value: EvaluatedCellValue, allValuesInRange: [EvaluatedCellValue] = []) -> Bool {
        switch self {
        case let .greaterThan(threshold):
            guard let num = value.numericValue else { return false }
            return num > threshold
        case let .lessThan(threshold):
            guard let num = value.numericValue else { return false }
            return num < threshold
        case let .between(low, high):
            guard let num = value.numericValue else { return false }
            return num >= min(low, high) && num <= max(low, high)
        case let .containsText(substring):
            guard !substring.isEmpty else { return false }
            return value.description.localizedCaseInsensitiveContains(substring)
        case .duplicateValues:
            guard value != .empty else { return false }
            return allValuesInRange.filter { $0 == value }.count > 1
        }
    }
}

/// Cell input data validation rules.
public enum DataValidationRule: Sendable, Codable, Equatable {
    case numberRange(min: Double, max: Double)
    case allowedList([String])
    case nonEmptyText
    case maxTextLength(Int)

    public func validate(_ value: EvaluatedCellValue) -> Bool {
        switch self {
        case let .numberRange(minVal, maxVal):
            guard let num = value.numericValue else { return false }
            return num >= minVal && num <= maxVal
        case let .allowedList(allowed):
            return allowed.contains { $0.caseInsensitiveCompare(value.description) == .orderedSame }
        case .nonEmptyText:
            return value != .empty && !value.description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        case let .maxTextLength(maxLen):
            return value != .empty && value.description.count <= maxLen
        }
    }
}
