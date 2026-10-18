import Foundation

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
            sheet.evaluate(at: $0.packedCoordinate).description
        }
        let seriesList = valueRanges.map { entry in
            let nums = entry.range.expandedAddresses().map {
                sheet.evaluate(at: $0.packedCoordinate).numericValue ?? 0.0
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
