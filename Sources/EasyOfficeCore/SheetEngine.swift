import Foundation

/// Zero-indexed spreadsheet cell coordinate with A1 notation parser and formatter.
public struct CellAddress: Sendable, Codable, Equatable, Hashable, CustomStringConvertible {
    public let column: Int // 0 = A, 25 = Z, 26 = AA
    public let row: Int    // 0 = row 1

    public init(column: Int, row: Int) throws {
        guard column >= 0, row >= 0 else {
            throw EasyOfficeError.invalidCellAddress("col=\(column), row=\(row)")
        }
        self.column = column
        self.row = row
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

        guard !letters.isEmpty, !digits.isEmpty, let rowOneBased = Int(digits), rowOneBased >= 1 else {
            throw EasyOfficeError.invalidCellAddress(a1Notation)
        }

        var colValue = 0
        for scalar in letters.unicodeScalars {
            let charIndex = Int(scalar.value) - Int(UnicodeScalar("A").value) + 1
            colValue = colValue * 26 + charIndex
        }

        self.column = colValue - 1
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
    }

    /// Expands the range in row-major order.
    public func expandedAddresses() -> [CellAddress] {
        let minRow = min(start.row, end.row)
        let maxRow = max(start.row, end.row)
        let minCol = min(start.column, end.column)
        let maxCol = max(start.column, end.column)

        var list: [CellAddress] = []
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
    case error(String) // e.g. #DIV/0!, #VALUE!, #NAME?, #REF!

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
}

/// Worksheet model storing raw cell inputs (literals or `=...` formulas) and evaluating deterministically.
public struct WorksheetModel: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var name: String
    public var rawCells: [String: String] // Keyed by A1 notation

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
        // Function call pattern: FUNCNAME(args...)
        if let openParen = expr.firstIndex(of: "("), expr.hasSuffix(")") {
            let funcName = String(expr[..<openParen]).trimmingCharacters(in: .whitespaces).uppercased()
            if !funcName.isEmpty && funcName.unicodeScalars.allSatisfy({ CharacterSet.alphanumerics.contains($0) }) {
                let innerStart = expr.index(after: openParen)
                let innerEnd = expr.index(before: expr.endIndex)
                let argString = innerStart <= innerEnd ? String(expr[innerStart..<innerEnd]) : ""
                let args = Self.splitTopLevelArguments(argString)
                return evaluateFunction(name: funcName, arguments: args, visited: &visited)
            }
        }

        // Binary arithmetic: +, -, *, /
        if let result = evaluateBinaryArithmetic(expr, visited: &visited) {
            return result
        }

        // Quoted string literal
        if expr.hasPrefix("\"") && expr.hasSuffix("\"") && expr.count >= 2 {
            let start = expr.index(after: expr.startIndex)
            let end = expr.index(before: expr.endIndex)
            return .text(String(expr[start..<end]))
        }

        // Numeric literal
        if let number = Double(expr) {
            return .number(number)
        }

        // Direct cell reference
        if let addr = try? CellAddress(a1Notation: expr) {
            let resolved = evaluateInternal(at: addr.a1Notation, visited: &visited)
            if case .empty = resolved {
                return .number(0.0)
            }
            return resolved
        }

        return .error("#VALUE!")
    }

    private func evaluateBinaryArithmetic(_ expr: String, visited: inout Set<String>) -> EvaluatedCellValue? {
        // Precedence: scan + / - first at top level, then * / /
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

        switch name {
        case "SUM":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let sum = vals.compactMap { $0.numericValue }.reduce(0.0, +)
                return .number(sum)
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
                let nums = vals.compactMap { $0.numericValue }
                return .number(nums.min() ?? 0.0)
            }
        case "MAX":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                let nums = vals.compactMap { $0.numericValue }
                return .number(nums.max() ?? 0.0)
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
        case "ABS":
            guard arguments.count == 1 else { return .error("#VALUE!") }
            let val = evaluateFormulaExpression(arguments[0], visited: &visited)
            if case .error = val { return val }
            guard let num = val.numericValue else { return .error("#VALUE!") }
            return .number(abs(num))
        case "ROUND":
            guard arguments.count == 2 else { return .error("#VALUE!") }
            let val = evaluateFormulaExpression(arguments[0], visited: &visited)
            let digitsVal = evaluateFormulaExpression(arguments[1], visited: &visited)
            guard let num = val.numericValue, let digits = digitsVal.numericValue else {
                return .error("#VALUE!")
            }
            let factor = pow(10.0, Double(Int(digits)))
            return .number((num * factor).rounded() / factor)
        case "CONCAT":
            switch resolveValues(arguments) {
            case let .failure(err): return .error(err)
            case let .success(vals):
                return .text(vals.map { $0.description }.joined())
            }
        case "IF":
            guard arguments.count == 3 else { return .error("#VALUE!") }
            let conditionPassed = evaluateCondition(arguments[0], visited: &visited)
            let branch = conditionPassed ? arguments[1] : arguments[2]
            return evaluateFormulaExpression(branch.trimmingCharacters(in: .whitespaces), visited: &visited)
        default:
            return .error("#NAME?")
        }
    }

    private func evaluateCondition(_ cond: String, visited: inout Set<String>) -> Bool {
        for op in [">=", "<=", "<>", "=", ">", "<"] {
            if let range = cond.range(of: op) {
                let left = String(cond[..<range.lowerBound]).trimmingCharacters(in: .whitespaces)
                let right = String(cond[range.upperBound...]).trimmingCharacters(in: .whitespaces)
                let lVal = evaluateFormulaExpression(left, visited: &visited)
                let rVal = evaluateFormulaExpression(right, visited: &visited)
                if let lNum = lVal.numericValue, let rNum = rVal.numericValue {
                    switch op {
                    case ">=": return lNum >= rNum
                    case "<=": return lNum <= rNum
                    case "<>": return lNum != rNum
                    case "=":  return lNum == rNum
                    case ">":  return lNum > rNum
                    case "<":  return lNum < rNum
                    default: break
                    }
                } else {
                    switch op {
                    case "=": return lVal.description == rVal.description
                    case "<>": return lVal.description != rVal.description
                    default: return false
                    }
                }
            }
        }
        let single = evaluateFormulaExpression(cond, visited: &visited)
        if let num = single.numericValue {
            return num != 0.0
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
