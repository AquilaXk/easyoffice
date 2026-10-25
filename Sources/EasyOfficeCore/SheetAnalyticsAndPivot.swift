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
    var rowKeySet = Set<String>()
    var colKeys: [String] = []
    var colKeySet = Set<String>()
    var buckets: [String: [String: [Double]]] = [:]
    var rowBuckets: [String: [Double]] = [:]
    var colBuckets: [String: [Double]] = [:]
    var allValues: [Double] = []

    for row in dataRows {
      guard configuration.rowFieldColumnIndex >= 0,
        configuration.rowFieldColumnIndex < row.count,
        configuration.valueFieldColumnIndex >= 0,
        configuration.valueFieldColumnIndex < row.count
      else {
        continue
      }
      guard !row.allSatisfy({ $0 == .empty || $0.description.isEmpty }) else { continue }
      let rawRKey = row[configuration.rowFieldColumnIndex].description
      let rKey = rawRKey.isEmpty ? "(빈 값)" : rawRKey
      let cKey: String
      if let colField = configuration.columnFieldColumnIndex,
        colField >= 0,
        colField < row.count
      {
        let rawCKey = row[colField].description
        cKey = rawCKey.isEmpty ? "(빈 값)" : rawCKey
      } else {
        cKey = "Total"
      }

      if rowKeySet.insert(rKey).inserted { rowKeys.append(rKey) }
      if colKeySet.insert(cKey).inserted { colKeys.append(cKey) }

      let valCell = row[configuration.valueFieldColumnIndex]
      if let val = valCell.numericValue {
        buckets[rKey, default: [:]][cKey, default: []].append(val)
        rowBuckets[rKey, default: []].append(val)
        colBuckets[cKey, default: []].append(val)
        allValues.append(val)
      } else if configuration.aggregation == .count,
        valCell != .empty,
        !valCell.description.isEmpty
      {
        buckets[rKey, default: [:]][cKey, default: []].append(1.0)
        rowBuckets[rKey, default: []].append(1.0)
        colBuckets[cKey, default: []].append(1.0)
        allValues.append(1.0)
      }
    }

    let matrix: [[Double]] = rowKeys.map { rKey in
      colKeys.map { cKey in
        aggregate(buckets[rKey]?[cKey] ?? [], using: configuration.aggregation)
      }
    }
    let rowTotals = rowKeys.map {
      aggregate(rowBuckets[$0] ?? [], using: configuration.aggregation)
    }
    let columnTotals = colKeys.map {
      aggregate(colBuckets[$0] ?? [], using: configuration.aggregation)
    }
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

  @inline(__always)
  private static func unrolledSum(_ values: [Double]) -> Double {
    let count = values.count
    var s0 = 0.0
    var s1 = 0.0
    var s2 = 0.0
    var s3 = 0.0
    var idx = 0
    while idx + 4 <= count {
      s0 += values[idx]
      s1 += values[idx + 1]
      s2 += values[idx + 2]
      s3 += values[idx + 3]
      idx += 4
    }
    while idx < count {
      s0 += values[idx]
      idx += 1
    }
    return (s0 + s1) + (s2 + s3)
  }

  private static func aggregate(_ values: [Double], using fn: PivotAggregationFunction) -> Double {
    guard !values.isEmpty else { return 0.0 }
    switch fn {
    case .sum:
      return unrolledSum(values)
    case .average:
      return unrolledSum(values) / Double(values.count)
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

  public func matches(_ value: EvaluatedCellValue, allValuesInRange: [EvaluatedCellValue] = [])
    -> Bool
  {
    switch self {
    case .greaterThan(let threshold):
      guard let num = value.numericValue else { return false }
      return num > threshold
    case .lessThan(let threshold):
      guard let num = value.numericValue else { return false }
      return num < threshold
    case .between(let low, let high):
      guard let num = value.numericValue else { return false }
      return num >= min(low, high) && num <= max(low, high)
    case .containsText(let substring):
      guard !substring.isEmpty else { return false }
      return value.description.localizedCaseInsensitiveContains(substring)
    case .duplicateValues:
      guard value != .empty else { return false }
      var matchCount = 0
      for item in allValuesInRange where item == value {
        matchCount += 1
        if matchCount > 1 { return true }
      }
      return false
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
    case .numberRange(let minVal, let maxVal):
      guard let num = value.numericValue else { return false }
      return num >= minVal && num <= maxVal
    case .allowedList(let allowed):
      return allowed.contains { $0.caseInsensitiveCompare(value.description) == .orderedSame }
    case .nonEmptyText:
      return value != .empty
        && !value.description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    case .maxTextLength(let maxLen):
      return value != .empty && value.description.count <= maxLen
    }
  }
}

public typealias SheetConditionalFormatRule = ConditionalFormatRule
public typealias SheetDataValidationRule = DataValidationRule

