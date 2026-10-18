import Foundation

extension WorksheetModel {

    // MARK: - Modular Function Dispatch (77 Built-in Functions)

    internal enum FormulaResolutionResult {
        case success([EvaluatedCellValue])
        case failure(String)
    }

    internal struct EvalFrame {
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

    internal func evaluateFunction(
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
            let text = frame.arg(0, in: self).description
            let startOffset = startOneBased - 1
            guard let startIdx = text.index(text.startIndex, offsetBy: startOffset, limitedBy: text.endIndex),
                  startIdx < text.endIndex else {
                return .text("")
            }
            let endIdx = text.index(startIdx, offsetBy: length, limitedBy: text.endIndex) ?? text.endIndex
            return .text(String(text[startIdx..<endIdx]))
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

    // MARK: - Function & Date Helpers

    internal static let utcCalendar: Calendar = {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(secondsFromGMT: 0)!
        return cal
    }()

    internal static func safeInt(_ value: Double) -> Int? {
        guard value.isFinite, value > Double(Int.min), value < Double(Int.max) else {
            return nil
        }
        return Int(value)
    }

    internal static func makeUTCDate(year: Int, month: Int, day: Int) -> Date? {
        var comps = DateComponents()
        comps.year = year
        comps.month = month
        comps.day = day
        comps.hour = 0
        comps.minute = 0
        comps.second = 0
        return utcCalendar.date(from: comps)
    }

    internal static func parseDateValue(_ value: EvaluatedCellValue) -> Date? {
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

    internal static func cellValuesEqual(_ a: EvaluatedCellValue, _ b: EvaluatedCellValue) -> Bool {
        if let aNum = a.numericValue, let bNum = b.numericValue {
            return aNum == bNum
        }
        return a.description.caseInsensitiveCompare(b.description) == .orderedSame
    }

    internal static func matchesCriteriaValue(
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
}
