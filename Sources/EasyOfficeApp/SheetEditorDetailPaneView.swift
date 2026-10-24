import AppKit
import EasyOfficeCore
import EasyOfficeDesignSystem
import Observation
import SwiftUI

public typealias SpreadsheetChartKind = SheetChartKind

/// Normalized data point used by `SheetVectorChartCanvasView` across all 7 chart geometries.
public struct SheetChartRenderPoint: Identifiable, Sendable, Equatable {
  public let id: Int
  public let label: String
  public let value: Double
  public let normalizedRatio: Double

  public init(id: Int, label: String, value: Double, normalizedRatio: Double) {
    self.id = id
    self.label = label
    self.value = value
    self.normalizedRatio = normalizedRatio
  }
}

/// Multi-geometry vector chart canvas supporting all 7 `SheetChartKind` cases (`.column`, `.stackedColumn`, `.bar`, `.line`, `.area`, `.pie`, `.scatter`).
public struct SheetVectorChartCanvasView: View {
  public let chartSpec: SheetChartModel
  public let palette: WorkspaceThemePalette

  public init(chartSpec: SheetChartModel, palette: WorkspaceThemePalette) {
    self.chartSpec = chartSpec
    self.palette = palette
  }

  private static func safeFinite(_ value: Double) -> Double {
    guard value.isFinite else { return 0.0 }
    return max(-1.0e15, min(1.0e15, value))
  }

  private static func formatMetric(_ value: Double) -> String {
    String(format: "%.0f", safeFinite(value))
  }

  private var primaryPoints: [SheetChartRenderPoint] {
    let rawValues: [Double] = {
      if let firstSeries = chartSpec.series.first, !firstSeries.values.isEmpty {
        return firstSeries.values.map { Self.safeFinite($0) }
      }
      return [120.0, 180.0, 145.0]
    }()
    let maxVal = max(1.0, rawValues.map { abs($0) }.max() ?? 1.0)
    return rawValues.enumerated().map { idx, val in
      let categoryLabel =
        chartSpec.categories.indices.contains(idx)
        ? chartSpec.categories[idx]
        : "항목 \(idx + 1)"
      let ratio = max(0.06, min(1.0, max(0.0, val) / maxVal))
      return SheetChartRenderPoint(
        id: idx,
        label: categoryLabel,
        value: val,
        normalizedRatio: ratio
      )
    }
  }

  private var seriesPaletteColors: [Color] {
    [
      EasyOfficeDesignTokens.brand700.swiftUIColor,
      EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor,
      EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor,
      EasyOfficeDesignTokens.brand500.swiftUIColor,
      EasyOfficeDesignTokens.statusInfo.swiftUIColor,
      EasyOfficeDesignTokens.accentToken(for: .hwp).swiftUIColor,
    ]
  }

  public var body: some View {
    let points = primaryPoints
    let primaryAccent = EasyOfficeDesignTokens.brand700.swiftUIColor

    VStack(alignment: .leading, spacing: 8) {
      switch chartSpec.kind {
      case .column:
        HStack(alignment: .bottom, spacing: 14) {
          ForEach(points) { pt in
            VStack(spacing: 4) {
              Text(Self.formatMetric(pt.value))
                .font(.system(size: 9, weight: .bold, design: .monospaced))
                .foregroundStyle(palette.primaryText.swiftUIColor)

              RoundedRectangle(cornerRadius: 4)
                .fill(primaryAccent)
                .frame(width: 28, height: max(10.0, CGFloat(pt.normalizedRatio) * 86.0))

              Text(pt.label)
                .font(.system(size: 9))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
                .lineLimit(1)
            }
            .frame(maxWidth: .infinity)
          }
        }
        .frame(height: 126, alignment: .bottom)
        .padding(.horizontal, 6)

      case .stackedColumn:
        let catCount = max(1, points.count)
        let maxStackSum: Double = {
          var highest = 1.0
          for cIdx in 0..<catCount {
            let sumAtCat = chartSpec.series.reduce(0.0) { acc, s in
              let v = s.values.indices.contains(cIdx) ? Self.safeFinite(s.values[cIdx]) : 0.0
              return acc + max(0.0, v)
            }
            highest = max(highest, sumAtCat)
          }
          return highest
        }()

        HStack(alignment: .bottom, spacing: 14) {
          ForEach(0..<catCount, id: \.self) { cIdx in
            let totalVal = chartSpec.series.reduce(0.0) { acc, s in
              let v = s.values.indices.contains(cIdx) ? Self.safeFinite(s.values[cIdx]) : 0.0
              return acc + max(0.0, v)
            }
            let catLabel =
              chartSpec.categories.indices.contains(cIdx)
              ? chartSpec.categories[cIdx]
              : points[cIdx].label

            VStack(spacing: 3) {
              Text(Self.formatMetric(totalVal))
                .font(.system(size: 9, weight: .bold, design: .monospaced))
                .foregroundStyle(palette.primaryText.swiftUIColor)

              VStack(spacing: 1) {
                ForEach(Array(chartSpec.series.enumerated()), id: \.offset) { sIdx, series in
                  let segVal =
                    series.values.indices.contains(cIdx)
                    ? max(0.0, Self.safeFinite(series.values[cIdx]))
                    : 0.0
                  let segRatio = max(0.05, min(1.0, segVal / maxStackSum))
                  RoundedRectangle(cornerRadius: 2)
                    .fill(seriesPaletteColors[sIdx % seriesPaletteColors.count])
                    .frame(width: 30, height: max(6.0, CGFloat(segRatio) * 76.0))
                }
              }

              Text(catLabel)
                .font(.system(size: 9))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
                .lineLimit(1)
            }
            .frame(maxWidth: .infinity)
          }
        }
        .frame(height: 126, alignment: .bottom)
        .padding(.horizontal, 6)

      case .bar:
        VStack(alignment: .leading, spacing: 6) {
          ForEach(points) { pt in
            HStack(spacing: 8) {
              Text(pt.label)
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(palette.primaryText.swiftUIColor)
                .frame(width: 68, alignment: .leading)
                .lineLimit(1)

              RoundedRectangle(cornerRadius: 4)
                .fill(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)
                .frame(width: max(14.0, CGFloat(pt.normalizedRatio) * 160.0), height: 14)

              Text(Self.formatMetric(pt.value))
                .font(.system(size: 9, weight: .bold, design: .monospaced))
                .foregroundStyle(palette.secondaryText.swiftUIColor)

              Spacer(minLength: 0)
            }
          }
        }
        .padding(.vertical, 4)

      case .line, .area:
        let fillArea = (chartSpec.kind == .area)
        VStack(spacing: 4) {
          Canvas { context, size in
            guard !points.isEmpty else { return }
            let plotW = max(1.0, size.width - 24.0)
            let plotH = max(1.0, size.height - 16.0)
            let baselineY = 8.0 + plotH
            var linePath = Path()
            var areaPath = Path()
            var vertexLocations: [CGPoint] = []

            for (idx, pt) in points.enumerated() {
              let xRatio =
                points.count == 1
                ? 0.5
                : Double(idx) / Double(max(1, points.count - 1))
              let px = 12.0 + CGFloat(xRatio) * plotW
              let py = 8.0 + (1.0 - CGFloat(pt.normalizedRatio)) * plotH
              let loc = CGPoint(x: px, y: py)
              vertexLocations.append(loc)
              if idx == 0 {
                linePath.move(to: loc)
                if fillArea {
                  areaPath.move(to: CGPoint(x: px, y: baselineY))
                  areaPath.addLine(to: loc)
                }
              } else {
                linePath.addLine(to: loc)
                if fillArea {
                  areaPath.addLine(to: loc)
                }
              }
            }

            if fillArea, let lastLoc = vertexLocations.last {
              areaPath.addLine(to: CGPoint(x: lastLoc.x, y: baselineY))
              areaPath.closeSubpath()
              context.fill(areaPath, with: .color(primaryAccent.opacity(0.22)))
            }

            context.stroke(
              linePath,
              with: .color(primaryAccent),
              style: StrokeStyle(
                lineWidth: fillArea ? 2.0 : 2.2,
                lineCap: .round,
                lineJoin: .round
              )
            )

            let markerRadius: CGFloat = fillArea ? 3.5 : 4.0
            for loc in vertexLocations {
              let markerRect = CGRect(
                x: loc.x - markerRadius,
                y: loc.y - markerRadius,
                width: markerRadius * 2.0,
                height: markerRadius * 2.0
              )
              context.fill(Path(ellipseIn: markerRect), with: .color(primaryAccent))
            }
          }
          .frame(height: 98)

          HStack {
            ForEach(points) { pt in
              Text("\(pt.label) (\(Self.formatMetric(pt.value)))")
                .font(.system(size: 9, design: .monospaced))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
                .frame(maxWidth: .infinity)
                .lineLimit(1)
            }
          }
        }

      case .pie:
        let rawSum = points.reduce(0.0) { $0 + max(0.0, Self.safeFinite($1.value)) }
        let totalSum = (rawSum.isFinite && rawSum > 0.0) ? rawSum : 1.0
        HStack(spacing: 16) {
          Canvas { context, size in
            let radius = min(size.width, size.height) * 0.44
            let center = CGPoint(x: size.width / 2.0, y: size.height / 2.0)
            var currentAngle = Angle.degrees(-90.0)

            for (idx, pt) in points.enumerated() {
              let fraction = max(0.0, min(1.0, max(0.0, Self.safeFinite(pt.value)) / totalSum))
              let sweepDegrees = fraction * 360.0
              let startAngle = currentAngle
              let endAngle = currentAngle + Angle.degrees(sweepDegrees)
              currentAngle = endAngle

              var path = Path()
              path.move(to: center)
              path.addArc(
                center: center,
                radius: radius,
                startAngle: startAngle,
                endAngle: endAngle,
                clockwise: false
              )
              path.closeSubpath()

              let sliceColor = seriesPaletteColors[idx % seriesPaletteColors.count]
              context.fill(path, with: .color(sliceColor))
              context.stroke(path, with: .color(palette.cardSurface.swiftUIColor), lineWidth: 1.5)
            }
          }
          .frame(width: 112, height: 112)

          VStack(alignment: .leading, spacing: 5) {
            ForEach(Array(points.enumerated()), id: \.offset) { idx, pt in
              let ratioPct = max(
                0.0, min(100.0, (max(0.0, Self.safeFinite(pt.value)) / totalSum) * 100.0))
              let pctText = String(format: "%.0f", ratioPct)
              HStack(spacing: 6) {
                Circle()
                  .fill(seriesPaletteColors[idx % seriesPaletteColors.count])
                  .frame(width: 8, height: 8)
                Text("\(pt.label): \(Self.formatMetric(pt.value)) (\(pctText)%)")
                  .font(.system(size: 10, design: .monospaced))
                  .foregroundStyle(palette.primaryText.swiftUIColor)
                  .lineLimit(1)
              }
            }
          }
          Spacer(minLength: 0)
        }
        .frame(height: 118)

      case .scatter:
        VStack(spacing: 4) {
          Canvas { context, size in
            let leftPad: CGFloat = 18.0
            let bottomPad: CGFloat = 14.0
            let plotW = max(1.0, size.width - leftPad - 12.0)
            let plotH = max(1.0, size.height - bottomPad - 10.0)

            var axesPath = Path()
            axesPath.move(to: CGPoint(x: leftPad, y: 6.0))
            axesPath.addLine(to: CGPoint(x: leftPad, y: 6.0 + plotH))
            axesPath.addLine(to: CGPoint(x: leftPad + plotW, y: 6.0 + plotH))
            context.stroke(
              axesPath,
              with: .color(palette.border.swiftUIColor),
              lineWidth: 1.2
            )

            for step in 1...2 {
              let gy = 6.0 + plotH * (CGFloat(step) / 3.0)
              var gridLine = Path()
              gridLine.move(to: CGPoint(x: leftPad, y: gy))
              gridLine.addLine(to: CGPoint(x: leftPad + plotW, y: gy))
              context.stroke(
                gridLine,
                with: .color(palette.border.swiftUIColor.opacity(0.5)),
                style: StrokeStyle(lineWidth: 0.8, dash: [3, 3])
              )
            }

            for (idx, pt) in points.enumerated() {
              let xRatio =
                points.count == 1
                ? 0.5
                : Double(idx) / Double(max(1, points.count - 1))
              let cx = leftPad + CGFloat(xRatio) * plotW
              let cy = 6.0 + CGFloat(1.0 - pt.normalizedRatio) * plotH
              let dotRect = CGRect(x: cx - 4.5, y: cy - 4.5, width: 9.0, height: 9.0)
              let dotColor = seriesPaletteColors[idx % seriesPaletteColors.count]
              context.fill(Path(ellipseIn: dotRect), with: .color(dotColor))
            }
          }
          .frame(height: 98)

          HStack {
            ForEach(points) { pt in
              Text("\(pt.label): \(Self.formatMetric(pt.value))")
                .font(.system(size: 9, design: .monospaced))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
                .frame(maxWidth: .infinity)
                .lineLimit(1)
            }
          }
        }
      }
    }
  }
}

/// Rich interactive editor pane for Sheet (.xlsx/.ods/.csv) with Formula Bar, multi-cell range selection, Auto-Fill handle (◢), interactive column/row drag resize, column auto-filter, Named Ranges / Goal Seek bar, mini sparkline, live range aggregation status bar, and 7-kind vector charts.
public struct SheetEditorDetailPaneView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  let sheet: WorksheetModel
  let isPrimaryPane: Bool
  @State private var inlineEditingCoord: PackedCellCoordinate? = nil

  public init(viewModel: WorkspaceViewModel, sheet: WorksheetModel, isPrimaryPane: Bool = true) {
    self.viewModel = viewModel
    self.sheet = sheet
    self.isPrimaryPane = isPrimaryPane
  }

  private func gridBounds(for sheet: WorksheetModel) -> (rows: Int, cols: Int) {
    var maxRow = 10
    var maxCol = 9
    for coord in sheet.cellsByCoordinate.keys {
      maxRow = max(maxRow, coord.row + 1)
      maxCol = max(maxCol, coord.column + 1)
    }
    return (min(128, maxRow), min(26, maxCol))
  }

  private var gridBounds: (rows: Int, cols: Int) {
    gridBounds(for: sheet)
  }

  private func tableSliceWidth(columnIndices: Range<Int>, includeRowHeader: Bool) -> CGFloat {
    let headerWidth: CGFloat = includeRowHeader ? 36.0 : 0.0
    let columnsWidth = columnIndices.reduce(CGFloat(0.0)) { acc, colIdx in
      acc + CGFloat(sheet.columnWidth(at: colIdx))
    }
    return headerWidth + columnsWidth
  }

  private func defaultAutoFillTargetNotation(for range: CellRange, maxRow: Int) -> String {
    let minR = min(range.start.row, range.end.row)
    let maxR = max(range.start.row, range.end.row)
    let minC = min(range.start.column, range.end.column)
    let maxC = max(range.start.column, range.end.column)
    let expandedMaxRow = min(CellAddress.maxZeroBasedRow, max(maxR + 2, min(maxRow + 1, maxR + 3)))
    let startAddr = CellAddress(packed: PackedCellCoordinate(row: minR, column: minC))
    let endAddr = CellAddress(packed: PackedCellCoordinate(row: expandedMaxRow, column: maxC))
    return "\(startAddr.a1Notation):\(endAddr.a1Notation)"
  }

  public var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
    let bounds = gridBounds(for: sheet)
    let maxRow = bounds.rows
    let maxCol = bounds.cols
    let evaluatedGrid = sheet.evaluateGrid(rows: 0...(maxRow - 1), columns: 0...(maxCol - 1))
    let columnLabels = (0..<maxCol).map { CellAddress.columnLabel(for: $0) }
    let selectedAddress =
      (try? CellAddress(a1Notation: viewModel.selectedSheetCell))
      ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    let fallbackRange = CellRange(start: selectedAddress, end: selectedAddress)
    let activeRange = (try? CellRange(notation: viewModel.selectedSheetRange)) ?? fallbackRange
    let selectedDisplayResult = sheet.formattedDisplayValue(
      at: selectedAddress, using: evaluatedGrid)
    let detectedRange = sheet.detectedDataRange()
    let matchingZeroBasedRows = sheet.filterMatchingRowIndices(
      column: min(max(0, viewModel.sheetAutoFilterColumn), maxCol - 1),
      query: viewModel.sheetAutoFilterQuery,
      maxRow: maxRow - 1,
      using: evaluatedGrid
    )
    let rangeSummary = sheet.summarizeRange(activeRange, using: evaluatedGrid)
    let sparklineValues: [Double] = {
      let inSelection = sheet.sparklineNormalizedValues(for: activeRange, using: evaluatedGrid)
      if inSelection.count >= 2 {
        return inSelection
      }
      return sheet.sparklineNormalizedValues(for: detectedRange, using: evaluatedGrid)
    }()

    let pivotSummary: PivotTableSummary = {
      let valueColOffset = max(0, detectedRange.end.column - detectedRange.start.column)
      let config = PivotTableConfiguration(
        rowFieldColumnIndex: 0,
        valueFieldColumnIndex: valueColOffset,
        aggregation: viewModel.selectedPivotAggregation
      )
      return PivotTableEngine.compute(
        from: sheet,
        range: detectedRange,
        hasHeaderRow: true,
        configuration: config
      )
    }()

    let condRangeValues: [EvaluatedCellValue] =
      viewModel.activeConditionalFormatRule != nil ? Array(evaluatedGrid.values) : []

    let maxPositiveValue: Double = {
      let maxFound = evaluatedGrid.values.compactMap { val -> Double? in
        if case .number(let n) = val, n.isFinite, n > 0 {
          return n
        }
        return nil
      }.max()
      return max(1.0, maxFound ?? 1.0)
    }()

    let chartSourceRange: CellRange = {
      let draft = viewModel.customChartRangeDraft.trimmingCharacters(in: .whitespacesAndNewlines)
      if !draft.isEmpty, let parsed = try? CellRange(notation: draft) {
        return parsed
      }
      return detectedRange
    }()

    let chartModel: SheetChartModel = {
      let startRow = chartSourceRange.start.row
      let endRow = chartSourceRange.end.row
      let startCol = chartSourceRange.start.column
      let endCol = chartSourceRange.end.column

      let dataStartRow = min(endRow, startRow + 1)
      let dataEndRow = max(dataStartRow, (endRow - startRow >= 3) ? (endRow - 1) : endRow)

      let catStart = CellAddress(packed: PackedCellCoordinate(row: dataStartRow, column: startCol))
      let catEnd = CellAddress(packed: PackedCellCoordinate(row: dataEndRow, column: startCol))
      let categoryRange = CellRange(start: catStart, end: catEnd)

      let seriesEndCol = max(startCol + 1, (endCol - startCol >= 3) ? (endCol - 1) : endCol)
      var valueRanges: [(name: String, range: CellRange)] = []
      if seriesEndCol >= startCol + 1 {
        for colIdx in (startCol + 1)...min(seriesEndCol, startCol + 3) {
          let headerCoord = PackedCellCoordinate(row: startRow, column: colIdx)
          let headerAddr = CellAddress(packed: headerCoord)
          let rawHeader = sheet.formattedDisplayValue(at: headerAddr, using: evaluatedGrid)
          let seriesName = rawHeader.isEmpty ? CellAddress.columnLabel(for: colIdx) : rawHeader
          let sStart = CellAddress(packed: PackedCellCoordinate(row: dataStartRow, column: colIdx))
          let sEnd = CellAddress(packed: PackedCellCoordinate(row: dataEndRow, column: colIdx))
          valueRanges.append((name: seriesName, range: CellRange(start: sStart, end: sEnd)))
        }
      }
      let customTitle = viewModel.customChartTitleDraft.trimmingCharacters(
        in: .whitespacesAndNewlines)
      let resolvedTitle = customTitle.isEmpty ? "\(sheet.name) 데이터 비교" : customTitle
      return SheetChartBuilder.buildChart(
        from: sheet,
        title: resolvedTitle,
        kind: viewModel.selectedChartKind,
        categoryRange: categoryRange,
        valueRanges: valueRanges
      )
    }()

    let activeMergedList: [String] = {
      if let tabID = viewModel.activeTabID {
        return viewModel.mergedSheetRanges(for: tabID)
      }
      return sheet.mergedRanges.map(\.a1Range)
    }()

    AdaptiveScrollContainer(
      axes: .vertical,
      showsIndicators: true,
      isSnapshotMode: viewModel.isCanvasSnapshotMode
    ) {
      VStack(alignment: .leading, spacing: 10) {
        // Formula Bar with Name Box, [×] [✓] [fx] Action Strip, Active Range Badge & 122-function catalog badge
        AdaptiveScrollContainer(
          axes: .horizontal,
          showsIndicators: false,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        ) {
          HStack(spacing: 8) {
            Text(viewModel.selectedSheetCell)
              .font(.system(size: 12, weight: .bold, design: .monospaced))
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
              .padding(.horizontal, 8)
              .padding(.vertical, 4)
              .background(palette.selectedSurface.swiftUIColor)
              .foregroundStyle(palette.accentBadgeText.swiftUIColor)
              .clipShape(RoundedRectangle(cornerRadius: 4))

            Text("범위: \(viewModel.selectedSheetRange)")
              .font(.system(size: 11, weight: .semibold, design: .monospaced))
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
              .padding(.horizontal, 6)
              .padding(.vertical, 3)
              .background(palette.subtleAccentSurface.swiftUIColor)
              .foregroundStyle(palette.accentBadgeText.swiftUIColor)
              .clipShape(RoundedRectangle(cornerRadius: 4))

            if isPrimaryPane {
              HStack(spacing: 3) {
                Button("×") {
                  viewModel.formulaBarDraft = sheet.rawValue(at: viewModel.selectedSheetCell)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .help("수식 입력 취소 (×)")

                Button("✓") {
                  viewModel.applyFormulaBarEdit()
                }
                .buttonStyle(.bordered)
                .tint(EasyOfficeDesignTokens.statusSuccess.swiftUIColor)
                .controlSize(.mini)
                .help("수식 입력 확인 (✓)")

                Button("fx") {
                  if viewModel.formulaBarDraft.trimmingCharacters(in: .whitespaces).isEmpty {
                    viewModel.formulaBarDraft = "=SUM("
                  } else if !viewModel.formulaBarDraft.hasPrefix("=") {
                    viewModel.formulaBarDraft = "=" + viewModel.formulaBarDraft
                  }
                }
                .buttonStyle(.bordered)
                .tint(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)
                .controlSize(.mini)
                .help("수식 함수 마법사 (\(WorksheetModel.supportedFunctionNames.count)종 내장 함수)")
              }
            }

            Text("fx (\(WorksheetModel.supportedFunctionNames.count)종 함수)")
              .font(.system(size: 11, weight: .semibold, design: .monospaced))
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
              .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)

            if isPrimaryPane {
              LavenderTextFieldPill(
                placeholder: "수식 또는 값 입력 (=SUM(B2:C2))",
                text: $viewModel.formulaBarDraft,
                width: 280,
                isMonospaced: true,
                palette: palette,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              ) {
                viewModel.applyFormulaBarEdit()
              }
              .accessibilityLabel(
                AccessibilityTokens.formulaBarLabel(selectedCell: viewModel.selectedSheetCell)
              )
              .accessibilityHint(AccessibilityTokens.formulaBarHint)

              Button("적용") {
                viewModel.applyFormulaBarEdit()
              }
              .buttonStyle(.borderedProminent)
              .tint(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)
              .controlSize(.small)
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
            } else {
              Text(sheet.rawValue(at: viewModel.selectedSheetCell))
                .font(.system(size: 12, design: .monospaced))
                .foregroundStyle(palette.primaryText.swiftUIColor)
            }

            Spacer(minLength: 8)

            if !activeMergedList.isEmpty {
              Label(
                "병합: \(activeMergedList.joined(separator: ", "))",
                systemImage: "rectangle.compress.vertical"
              )
              .font(.system(size: 10, weight: .semibold, design: .monospaced))
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
              .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)
            }

            if viewModel.isHeaderRowFrozen {
              Label("1행 고정 활성", systemImage: "pin.fill")
                .font(.system(size: 10, weight: .semibold))
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
            }

            if viewModel.isFirstColumnFrozen {
              Label("A열 고정 활성", systemImage: "pin.fill")
                .font(.system(size: 10, weight: .semibold))
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)
            }

            Text("결과: \(selectedDisplayResult)")
              .font(.system(size: 12, weight: .bold))
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
              .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
          }
          .padding(.horizontal, 12)
          .frame(minHeight: EasyOfficeDesignTokens.LayoutMetrics.formulaBarHeight)
        }
        .background(palette.cardSurface.swiftUIColor)
        .clipShape(RoundedRectangle(cornerRadius: 6))

        // Compact Single-Line Sheet Sub-Toolbar (Zero Clipping) & Collapsible Named Ranges / Goal Seek Tray
        if isPrimaryPane {
          AdaptiveScrollContainer(
            axes: .horizontal,
            showsIndicators: false,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          ) {
            HStack(spacing: 5) {
              HStack(spacing: 2) {
                Button("₩") {
                  viewModel.applyQuickNumberFormatOnSelectedSheetCell(.currencyKRW)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .help("원화 통화 서식 (₩)")

                Button("%") {
                  viewModel.applyQuickNumberFormatOnSelectedSheetCell(.percentage)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .help("백분율 서식 (%)")

                Button(",") {
                  viewModel.applyQuickNumberFormatOnSelectedSheetCell(.decimalTwo)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .help("천 단위 구분 및 소수 2자리 (,)")

                Button(".0←") {
                  viewModel.adjustSelectedSheetCellDecimalPlaces(delta: -1)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .help("소수 자릿수 줄이기")

                Button(".00→") {
                  viewModel.adjustSelectedSheetCellDecimalPlaces(delta: 1)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .help("소수 자릿수 늘리기")

                Button("줄바꿈") {
                  viewModel.toggleWrapTextOnSelectedSheetCell()
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .help("셀 내 텍스트 자동 줄바꿈 토글")
              }

              Divider().frame(height: 14)

              LavenderMenuPill(
                title: "\(CellAddress.columnLabel(for: max(0, viewModel.sheetAutoFilterColumn)))열",
                systemImage: "line.3.horizontal.decrease.circle",
                palette: palette
              ) {
                ForEach(0..<maxCol, id: \.self) { colIdx in
                  Button("\(CellAddress.columnLabel(for: colIdx))열") {
                    viewModel.sheetAutoFilterColumn = colIdx
                  }
                }
              }

              LavenderTextFieldPill(
                placeholder: "열 값 필터...",
                text: $viewModel.sheetAutoFilterQuery,
                width: 86,
                palette: palette,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              )

              if !viewModel.sheetAutoFilterQuery.isEmpty {
                Button("해제") {
                  viewModel.sheetAutoFilterQuery = ""
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
              }

              Divider().frame(height: 14)

              HStack(spacing: 2) {
                Button("+행") {
                  viewModel.insertSheetRowAtSelection()
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)

                Button("-행") {
                  viewModel.deleteSheetRowAtSelection()
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)

                Button("+열") {
                  viewModel.insertSheetColumnAtSelection()
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)

                Button("-열") {
                  viewModel.deleteSheetColumnAtSelection()
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
              }

              Button {
                let targetNotation = defaultAutoFillTargetNotation(for: activeRange, maxRow: maxRow)
                viewModel.performAutoFillOnActiveSheet(targetRangeNotation: targetNotation)
              } label: {
                Text("자동채우기")
                  .font(.system(size: 11, weight: .semibold))
                  .lineLimit(1)
                  .fixedSize(horizontal: true, vertical: false)
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)
              .fixedSize(horizontal: true, vertical: false)

              LavenderMenuPill(
                title: "데이터 정리",
                systemImage: "slider.horizontal.3",
                palette: palette
              ) {
                Button("중복 행 제거") {
                  _ = viewModel.removeDuplicatesInActiveSheet()
                }
                Button("텍스트 나누기(,)") {
                  _ = viewModel.splitActiveSheetColumnByDelimiter()
                }
                Button("다중 열 정렬(A↑·B↓)") {
                  viewModel.sortActiveSheetBySecondaryColumn(
                    primaryColumn: 0,
                    ascending: true,
                    secondaryColumn: 1,
                    secondaryAscending: false
                  )
                }
                Divider()
                Button("A열 고정 토글") {
                  viewModel.toggleFirstColumnFrozen()
                }
                Button("데이터 막대 토글") {
                  viewModel.toggleSheetDataBars()
                }
              }

              Button(viewModel.isAdvancedToolbarExpanded ? "고급 ▲" : "고급 ▼") {
                viewModel.isAdvancedToolbarExpanded.toggle()
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)

              if sparklineValues.count >= 2 {
                HStack(spacing: 4) {
                  Text("추세")
                    .font(.system(size: 10, weight: .semibold))
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
                    .foregroundStyle(palette.secondaryText.swiftUIColor)

                  Canvas { context, size in
                    var path = Path()
                    for (idx, norm) in sparklineValues.enumerated() {
                      let xRatio = Double(idx) / Double(max(1, sparklineValues.count - 1))
                      let px = 3.0 + CGFloat(xRatio) * max(1.0, size.width - 6.0)
                      let py = 3.0 + (1.0 - CGFloat(norm)) * max(1.0, size.height - 6.0)
                      if idx == 0 {
                        path.move(to: CGPoint(x: px, y: py))
                      } else {
                        path.addLine(to: CGPoint(x: px, y: py))
                      }
                    }
                    context.stroke(
                      path,
                      with: .color(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor),
                      style: StrokeStyle(lineWidth: 1.8, lineCap: .round, lineJoin: .round)
                    )
                  }
                  .frame(width: 54, height: 18)
                  .background(palette.elevatedSurface.swiftUIColor)
                  .clipShape(RoundedRectangle(cornerRadius: 4))
                }
              }

              Text("\(matchingZeroBasedRows.count)/\(maxRow)행")
                .font(.system(size: 10, design: .monospaced))
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .foregroundStyle(palette.secondaryText.swiftUIColor)
            }
            .padding(.horizontal, 10)
            .frame(height: 32)
          }
          .background(palette.cardSurface.swiftUIColor)
          .clipShape(RoundedRectangle(cornerRadius: 6))

          if viewModel.isAdvancedToolbarExpanded {
            AdaptiveScrollContainer(
              axes: .horizontal,
              showsIndicators: false,
              isSnapshotMode: viewModel.isCanvasSnapshotMode
            ) {
              HStack(spacing: 8) {
                Label("이름 정의 · 목표값 찾기", systemImage: "function")
                  .font(.system(size: 11, weight: .bold))
                  .lineLimit(1)
                  .fixedSize(horizontal: true, vertical: false)
                  .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)

                LavenderTextFieldPill(
                  placeholder: "이름 (예: 상반기매출)",
                  text: $viewModel.namedRangeNameDraft,
                  width: 110,
                  palette: palette,
                  isSnapshotMode: viewModel.isCanvasSnapshotMode
                )

                LavenderTextFieldPill(
                  placeholder: "대상 범위 (예: B2:C3)",
                  text: $viewModel.namedRangeTargetDraft,
                  width: 98,
                  isMonospaced: true,
                  palette: palette,
                  isSnapshotMode: viewModel.isCanvasSnapshotMode
                )

                Button("이름 정의") {
                  viewModel.defineNamedRangeOnActiveSheet()
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)

                if !sheet.namedRanges.isEmpty {
                  HStack(spacing: 4) {
                    ForEach(sheet.namedRanges.keys.sorted(), id: \.self) { nameKey in
                      let targetVal = sheet.namedRanges[nameKey] ?? ""
                      Text("\(nameKey)=\(targetVal)")
                        .font(.system(size: 9, weight: .semibold, design: .monospaced))
                        .lineLimit(1)
                        .fixedSize(horizontal: true, vertical: false)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(palette.selectedSurface.swiftUIColor)
                        .foregroundStyle(palette.accentBadgeText.swiftUIColor)
                        .clipShape(RoundedRectangle(cornerRadius: 4))
                    }
                  }
                }

                Divider().frame(height: 16)

                LavenderTextFieldPill(
                  placeholder: "수식 셀 (D2)",
                  text: $viewModel.goalSeekTargetCellDraft,
                  width: 72,
                  isMonospaced: true,
                  palette: palette,
                  isSnapshotMode: viewModel.isCanvasSnapshotMode
                )

                LavenderTextFieldPill(
                  placeholder: "목표값",
                  text: $viewModel.goalSeekTargetValueDraft,
                  width: 72,
                  isMonospaced: true,
                  palette: palette,
                  isSnapshotMode: viewModel.isCanvasSnapshotMode
                )

                LavenderTextFieldPill(
                  placeholder: "변수 셀 (B2)",
                  text: $viewModel.goalSeekVariableCellDraft,
                  width: 72,
                  isMonospaced: true,
                  palette: palette,
                  isSnapshotMode: viewModel.isCanvasSnapshotMode
                )

                Button("목표값 찾기") {
                  _ = viewModel.runGoalSeekOnActiveSheet()
                }
                .buttonStyle(.borderedProminent)
                .tint(EasyOfficeDesignTokens.brand700.swiftUIColor)
                .controlSize(.small)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
              }
              .padding(.horizontal, 10)
              .padding(.vertical, 5)
            }
            .background(palette.cardSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 6))
          }
        }

        // Authentic Zero-Gap (spacing: 0) 1px Hairline Spreadsheet Grid with Unified Vertical Scroll & Pinned Column A
        let startRow = viewModel.isHeaderRowFrozen ? min(2, maxRow) : 1
        let visibleRows = (startRow...maxRow).filter {
          matchingZeroBasedRows.contains($0 - 1)
        }
        let useEagerGridStack = viewModel.isCanvasSnapshotMode || visibleRows.count <= 64

        if viewModel.isFirstColumnFrozen && columnLabels.count > 1 {
          let pinnedSliceWidth = tableSliceWidth(columnIndices: 0..<1, includeRowHeader: true)
          let scrollSliceWidth = tableSliceWidth(
            columnIndices: 1..<columnLabels.count, includeRowHeader: false)
          AdaptiveScrollContainer(
            axes: .vertical,
            showsIndicators: true,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          ) {
            HStack(alignment: .top, spacing: 0) {
              VStack(alignment: .leading, spacing: 0) {
                columnHeaderRowView(
                  columnLabels: columnLabels,
                  columnIndices: 0..<1,
                  includeRowHeader: true,
                  activeRange: activeRange,
                  palette: palette
                )
                if viewModel.isHeaderRowFrozen {
                  sheetRowView(
                    row: 1,
                    columnLabels: columnLabels,
                    columnIndices: 0..<1,
                    includeRowHeader: true,
                    activeRange: activeRange,
                    evaluatedGrid: evaluatedGrid,
                    condRangeValues: condRangeValues,
                    maxPositiveValue: maxPositiveValue,
                    palette: palette
                  )
                  Rectangle()
                    .fill(EasyOfficeDesignTokens.brand700.swiftUIColor)
                    .frame(width: pinnedSliceWidth, height: 1.5)
                }
                if useEagerGridStack {
                  VStack(alignment: .leading, spacing: 0) {
                    ForEach(visibleRows, id: \.self) { row in
                      sheetRowView(
                        row: row,
                        columnLabels: columnLabels,
                        columnIndices: 0..<1,
                        includeRowHeader: true,
                        activeRange: activeRange,
                        evaluatedGrid: evaluatedGrid,
                        condRangeValues: condRangeValues,
                        maxPositiveValue: maxPositiveValue,
                        palette: palette
                      )
                    }
                  }
                } else {
                  LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(visibleRows, id: \.self) { row in
                      sheetRowView(
                        row: row,
                        columnLabels: columnLabels,
                        columnIndices: 0..<1,
                        includeRowHeader: true,
                        activeRange: activeRange,
                        evaluatedGrid: evaluatedGrid,
                        condRangeValues: condRangeValues,
                        maxPositiveValue: maxPositiveValue,
                        palette: palette
                      )
                    }
                  }
                }
              }
              .fixedSize(horizontal: true, vertical: false)

              Rectangle()
                .fill(EasyOfficeDesignTokens.brand700.swiftUIColor)
                .frame(width: 2)

              AdaptiveScrollContainer(
                axes: .horizontal,
                showsIndicators: true,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              ) {
                VStack(alignment: .leading, spacing: 0) {
                  columnHeaderRowView(
                    columnLabels: columnLabels,
                    columnIndices: 1..<columnLabels.count,
                    includeRowHeader: false,
                    activeRange: activeRange,
                    palette: palette
                  )
                  if viewModel.isHeaderRowFrozen {
                    sheetRowView(
                      row: 1,
                      columnLabels: columnLabels,
                      columnIndices: 1..<columnLabels.count,
                      includeRowHeader: false,
                      activeRange: activeRange,
                      evaluatedGrid: evaluatedGrid,
                      condRangeValues: condRangeValues,
                      maxPositiveValue: maxPositiveValue,
                      palette: palette
                    )
                    Rectangle()
                      .fill(EasyOfficeDesignTokens.brand700.swiftUIColor)
                      .frame(width: scrollSliceWidth, height: 1.5)
                  }
                  if useEagerGridStack {
                    VStack(alignment: .leading, spacing: 0) {
                      ForEach(visibleRows, id: \.self) { row in
                        sheetRowView(
                          row: row,
                          columnLabels: columnLabels,
                          columnIndices: 1..<columnLabels.count,
                          includeRowHeader: false,
                          activeRange: activeRange,
                          evaluatedGrid: evaluatedGrid,
                          condRangeValues: condRangeValues,
                          maxPositiveValue: maxPositiveValue,
                          palette: palette
                        )
                      }
                    }
                  } else {
                    LazyVStack(alignment: .leading, spacing: 0) {
                      ForEach(visibleRows, id: \.self) { row in
                        sheetRowView(
                          row: row,
                          columnLabels: columnLabels,
                          columnIndices: 1..<columnLabels.count,
                          includeRowHeader: false,
                          activeRange: activeRange,
                          evaluatedGrid: evaluatedGrid,
                          condRangeValues: condRangeValues,
                          maxPositiveValue: maxPositiveValue,
                          palette: palette
                        )
                      }
                    }
                  }
                }
                .fixedSize(horizontal: true, vertical: false)
              }
            }
          }
          .background(palette.cardSurface.swiftUIColor)
          .overlay(
            RoundedRectangle(cornerRadius: 6)
              .stroke(palette.border.swiftUIColor, lineWidth: 1)
          )
          .clipShape(RoundedRectangle(cornerRadius: 6))
        } else {
          let fullTableWidth = tableSliceWidth(
            columnIndices: 0..<columnLabels.count, includeRowHeader: true)
          AdaptiveScrollContainer(
            axes: .horizontal,
            showsIndicators: true,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          ) {
            VStack(alignment: .leading, spacing: 0) {
              columnHeaderRowView(
                columnLabels: columnLabels,
                columnIndices: 0..<columnLabels.count,
                includeRowHeader: true,
                activeRange: activeRange,
                palette: palette
              )

              if viewModel.isHeaderRowFrozen {
                sheetRowView(
                  row: 1,
                  columnLabels: columnLabels,
                  columnIndices: 0..<columnLabels.count,
                  includeRowHeader: true,
                  activeRange: activeRange,
                  evaluatedGrid: evaluatedGrid,
                  condRangeValues: condRangeValues,
                  maxPositiveValue: maxPositiveValue,
                  palette: palette
                )
                Rectangle()
                  .fill(EasyOfficeDesignTokens.brand700.swiftUIColor)
                  .frame(width: fullTableWidth, height: 1.5)
              }

              AdaptiveScrollContainer(
                axes: .vertical,
                showsIndicators: true,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              ) {
                if useEagerGridStack {
                  VStack(alignment: .leading, spacing: 0) {
                    ForEach(visibleRows, id: \.self) { row in
                      sheetRowView(
                        row: row,
                        columnLabels: columnLabels,
                        columnIndices: 0..<columnLabels.count,
                        includeRowHeader: true,
                        activeRange: activeRange,
                        evaluatedGrid: evaluatedGrid,
                        condRangeValues: condRangeValues,
                        maxPositiveValue: maxPositiveValue,
                        palette: palette
                      )
                    }
                  }
                  .fixedSize(horizontal: true, vertical: false)
                } else {
                  LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(visibleRows, id: \.self) { row in
                      sheetRowView(
                        row: row,
                        columnLabels: columnLabels,
                        columnIndices: 0..<columnLabels.count,
                        includeRowHeader: true,
                        activeRange: activeRange,
                        evaluatedGrid: evaluatedGrid,
                        condRangeValues: condRangeValues,
                        maxPositiveValue: maxPositiveValue,
                        palette: palette
                      )
                    }
                  }
                  .fixedSize(horizontal: true, vertical: false)
                }
              }
            }
            .fixedSize(horizontal: true, vertical: false)
          }
          .background(palette.cardSurface.swiftUIColor)
          .overlay(
            RoundedRectangle(cornerRadius: 6)
              .stroke(palette.border.swiftUIColor, lineWidth: 1)
          )
          .clipShape(RoundedRectangle(cornerRadius: 6))
        }

        // Bottom Worksheet Tab Strip & Live Range Aggregation Status Bar
        HStack(spacing: 6) {
          ForEach(Array(sheet.allSheetNames.enumerated()), id: \.offset) { idx, sheetName in
            let isActiveTab = idx == sheet.activeSheetIndex
            Button {
              guard isPrimaryPane else { return }
              viewModel.selectWorksheetTab(index: idx)
            } label: {
              HStack(spacing: 5) {
                Image(systemName: "tablecells")
                  .font(.system(size: 10))
                Text(sheetName)
                  .font(.system(size: 11, weight: isActiveTab ? .bold : .medium))
                  .lineLimit(1)
                  .fixedSize(horizontal: true, vertical: false)
              }
              .padding(.horizontal, 10)
              .padding(.vertical, 5)
              .background(
                isActiveTab
                  ? palette.selectedSurface.swiftUIColor
                  : palette.elevatedSurface.swiftUIColor
              )
              .foregroundStyle(
                isActiveTab
                  ? palette.accentBadgeText.swiftUIColor
                  : palette.secondaryText.swiftUIColor
              )
              .overlay(
                RoundedRectangle(cornerRadius: 5)
                  .stroke(
                    isActiveTab
                      ? EasyOfficeDesignTokens.brand700.swiftUIColor
                      : palette.border.swiftUIColor,
                    lineWidth: isActiveTab ? 1.5 : 1.0
                  )
              )
              .clipShape(RoundedRectangle(cornerRadius: 5))
            }
            .buttonStyle(.plain)
          }

          if isPrimaryPane {
            Button {
              viewModel.addWorksheetTab()
            } label: {
              Label("시트 추가", systemImage: "plus")
                .font(.system(size: 11, weight: .semibold))
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .padding(.horizontal, 8)
                .padding(.vertical, 5)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
          }

          Spacer()

          let minDisplay = rangeSummary.minimum.map { String(format: "%.1f", $0) } ?? "-"
          let maxDisplay = rangeSummary.maximum.map { String(format: "%.1f", $0) } ?? "-"
          let summaryText =
            (!isPrimaryPane || viewModel.splitSecondaryTab != nil)
            ? "합계 \(String(format: "%.0f", rangeSummary.sum)) · 평균 \(String(format: "%.0f", rangeSummary.average)) · \(rangeSummary.nonEmptyCount)셀"
            : "합계: \(String(format: "%.1f", rangeSummary.sum)) | 평균: \(String(format: "%.1f", rangeSummary.average)) | 개수: \(rangeSummary.nonEmptyCount) (수치 \(rangeSummary.numericCount)) | 최소: \(minDisplay) | 최대: \(maxDisplay)"
          Text(summaryText)
            .font(.system(size: 10, weight: .semibold, design: .monospaced))
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
            .foregroundStyle(palette.accentBadgeText.swiftUIColor)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(palette.subtleAccentSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 4))
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(palette.cardSurface.swiftUIColor)
        .clipShape(RoundedRectangle(cornerRadius: 6))

        // Pivot Table Summary & 7-Kind Vector Chart Preview Cards
        HStack(alignment: .top, spacing: 12) {
          VStack(alignment: .leading, spacing: 6) {
            HStack {
              Label(
                "피벗 테이블 집계 요약 (\(detectedRange.start.a1Notation):\(detectedRange.end.a1Notation))",
                systemImage: "tablecells.badge.ellipsis"
              )
              .font(.system(size: 11, weight: .bold))
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
              .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)

              Spacer()

              if isPrimaryPane {
                LavenderMenuPill(
                  title: viewModel.selectedPivotAggregation.rawValue,
                  systemImage: "function",
                  palette: palette
                ) {
                  ForEach(PivotAggregationFunction.allCases, id: \.self) { fn in
                    Button(fn.rawValue) {
                      viewModel.selectedPivotAggregation = fn
                    }
                  }
                }
              }
            }
            ForEach(Array(pivotSummary.rowKeys.enumerated()), id: \.offset) { idx, key in
              HStack {
                Text(key).font(.system(size: 11))
                Spacer()
                if pivotSummary.rowTotals.indices.contains(idx) {
                  Text(String(format: "%.0f", pivotSummary.rowTotals[idx]))
                    .font(.system(size: 11, weight: .semibold, design: .monospaced))
                }
              }
            }
            Divider()
            HStack {
              Text("총계 (Grand Total)").font(.system(size: 11, weight: .bold))
              Spacer()
              Text(String(format: "%.0f", pivotSummary.grandTotal))
                .font(.system(size: 11, weight: .bold, design: .monospaced))
                .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
            }
          }
          .padding(12)
          .frame(maxWidth: .infinity, alignment: .leading)
          .background(palette.cardSurface.swiftUIColor)
          .clipShape(RoundedRectangle(cornerRadius: 8))

          if viewModel.isChartPanelVisible {
            VStack(alignment: .leading, spacing: 8) {
              HStack(spacing: 8) {
                Image(systemName: "chart.bar.xaxis")
                  .font(.system(size: 11, weight: .bold))
                  .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)

                Text(chartModel.title)
                  .font(.system(size: 12, weight: .bold))
                  .lineLimit(1)
                  .fixedSize(horizontal: true, vertical: false)
                  .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)

                Spacer(minLength: 4)

                if isPrimaryPane {
                  HStack(spacing: 4) {
                    Button("막대") {
                      viewModel.selectedChartKind = .column
                    }
                    .buttonStyle(.bordered)
                    .tint(
                      viewModel.selectedChartKind == .column
                        ? EasyOfficeDesignTokens.brand700.swiftUIColor : nil
                    )
                    .controlSize(.mini)
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)

                    Button("꺾은선") {
                      viewModel.selectedChartKind = .line
                    }
                    .buttonStyle(.bordered)
                    .tint(
                      viewModel.selectedChartKind == .line
                        ? EasyOfficeDesignTokens.brand700.swiftUIColor : nil
                    )
                    .controlSize(.mini)
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)

                    Button("원형") {
                      viewModel.selectedChartKind = .pie
                    }
                    .buttonStyle(.bordered)
                    .tint(
                      viewModel.selectedChartKind == .pie
                        ? EasyOfficeDesignTokens.brand700.swiftUIColor : nil
                    )
                    .controlSize(.mini)
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)

                    LavenderMenuPill(
                      title: "유형",
                      systemImage: "chart.pie",
                      palette: palette
                    ) {
                      ForEach(SheetChartKind.allCases, id: \.self) { kind in
                        Button(kind.rawValue) {
                          viewModel.selectedChartKind = kind
                        }
                      }
                    }
                  }
                }
              }

              SheetVectorChartCanvasView(chartSpec: chartModel, palette: palette)
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(palette.cardSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 8))
          }
        }
      }
    }
  }

  @ViewBuilder
  private func columnHeaderRowView(
    columnLabels: [String],
    columnIndices: Range<Int>,
    includeRowHeader: Bool,
    activeRange: CellRange,
    palette: WorkspaceThemePalette
  ) -> some View {
    let selMinCol = min(activeRange.start.column, activeRange.end.column)
    let selMaxCol = max(activeRange.start.column, activeRange.end.column)

    HStack(spacing: 0) {
      if includeRowHeader {
        Text("#")
          .font(.system(size: 10, weight: .bold, design: .monospaced))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
          .frame(width: 36, height: 26)
          .background(palette.elevatedSurface.swiftUIColor)
          .overlay(
            Rectangle()
              .strokeBorder(palette.border.swiftUIColor.opacity(0.65), lineWidth: 0.5)
          )
      }

      ForEach(Array(columnIndices), id: \.self) { colIdx in
        let colLabel = columnLabels[colIdx]
        let colWidth = CGFloat(sheet.columnWidth(at: colIdx))
        let isColSelected = (colIdx >= selMinCol && colIdx <= selMaxCol)
        ZStack(alignment: .trailing) {
          Text(colLabel)
            .font(.system(size: 11, weight: .bold, design: .monospaced))
            .foregroundStyle(
              isColSelected
                ? palette.primaryAccent.swiftUIColor : palette.primaryText.swiftUIColor
            )
            .frame(width: colWidth, height: 26)
            .background(
              isColSelected
                ? palette.primaryAccent.swiftUIColor.opacity(0.20)
                : palette.elevatedSurface.swiftUIColor
            )
            .overlay(
              Rectangle()
                .strokeBorder(palette.border.swiftUIColor.opacity(0.65), lineWidth: 0.5)
            )
            .overlay(alignment: .bottom) {
              if isColSelected {
                Rectangle()
                  .fill(palette.primaryAccent.swiftUIColor)
                  .frame(height: 2)
              }
            }
            .help("\(colLabel)열 (너비: \(Int(colWidth))pt)")

          if isPrimaryPane {
            Rectangle()
              .fill(EasyOfficeDesignTokens.brand400.swiftUIColor.opacity(0.45))
              .frame(width: 4, height: 22)
              .gesture(
                DragGesture(minimumDistance: 1)
                  .onEnded { value in
                    let baseWidth = sheet.columnWidth(at: colIdx)
                    viewModel.updateActiveSheetColumnWidth(
                      column: colIdx,
                      width: baseWidth + Double(value.translation.width)
                    )
                  }
              )
          }
        }
      }
    }
  }

  @ViewBuilder
  private func sheetRowView(
    row: Int,
    columnLabels: [String],
    columnIndices: Range<Int>,
    includeRowHeader: Bool,
    activeRange: CellRange,
    evaluatedGrid: [PackedCellCoordinate: EvaluatedCellValue],
    condRangeValues: [EvaluatedCellValue],
    maxPositiveValue: Double,
    palette: WorkspaceThemePalette
  ) -> some View {
    let zeroRow = row - 1
    let rawHeight = sheet.rowHeight(at: zeroRow)
    let hasWrappedCell = (0..<columnLabels.count).contains { colIdx in
      sheet.cellStyle(at: PackedCellCoordinate(row: zeroRow, column: colIdx)).wrapText
    }
    let defaultAdjustedHeight: Double = abs(rawHeight - 46.0) < 0.1 ? 30.0 : rawHeight
    let rowHeightPt = CGFloat(
      hasWrappedCell ? max(defaultAdjustedHeight, 44.0) : defaultAdjustedHeight)
    let selMinRow = min(activeRange.start.row, activeRange.end.row)
    let selMaxRow = max(activeRange.start.row, activeRange.end.row)
    let selMinCol = min(activeRange.start.column, activeRange.end.column)
    let selMaxCol = max(activeRange.start.column, activeRange.end.column)
    let isRowSelected = (zeroRow >= selMinRow && zeroRow <= selMaxRow)

    HStack(spacing: 0) {
      if includeRowHeader {
        ZStack(alignment: .bottom) {
          Text("\(row)")
            .font(.system(size: 10, weight: .bold, design: .monospaced))
            .foregroundStyle(
              isRowSelected
                ? palette.primaryAccent.swiftUIColor : palette.secondaryText.swiftUIColor
            )
            .frame(width: 36, height: rowHeightPt)
            .background(
              isRowSelected
                ? palette.primaryAccent.swiftUIColor.opacity(0.20)
                : palette.elevatedSurface.swiftUIColor
            )
            .overlay(
              Rectangle()
                .strokeBorder(palette.border.swiftUIColor.opacity(0.65), lineWidth: 0.5)
            )
            .overlay(alignment: .trailing) {
              if isRowSelected {
                Rectangle()
                  .fill(palette.primaryAccent.swiftUIColor)
                  .frame(width: 2)
              }
            }

          if isPrimaryPane {
            Rectangle()
              .fill(EasyOfficeDesignTokens.brand400.swiftUIColor.opacity(0.45))
              .frame(width: 32, height: 3)
              .gesture(
                DragGesture(minimumDistance: 1)
                  .onEnded { value in
                    let baseHeight = Double(rowHeightPt)
                    viewModel.updateActiveSheetRowHeight(
                      row: zeroRow,
                      height: baseHeight + Double(value.translation.height)
                    )
                  }
              )
          }
        }
      }

      ForEach(Array(columnIndices), id: \.self) { colIdx in
        let col = columnLabels[colIdx]
        let coord = PackedCellCoordinate(row: zeroRow, column: colIdx)
        let addr = "\(col)\(row)"
        let isPrimaryCell = viewModel.selectedSheetCell == addr
        let isInRange = activeRange.contains(coord)
        let isSelectionTopEdge = isInRange && (zeroRow == selMinRow)
        let isSelectionBottomEdge = isInRange && (zeroRow == selMaxRow)
        let isSelectionLeadingEdge = isInRange && (colIdx == selMinCol)
        let isSelectionTrailingEdge = isInRange && (colIdx == selMaxCol)

        gridCellView(
          row: row,
          zeroRow: zeroRow,
          colIdx: colIdx,
          addr: addr,
          coord: coord,
          rowHeightPt: rowHeightPt,
          activeRange: activeRange,
          isPrimarySelected: isPrimaryCell,
          isInSelectedRange: isInRange,
          isSelectionTopEdge: isSelectionTopEdge,
          isSelectionBottomEdge: isSelectionBottomEdge,
          isSelectionLeadingEdge: isSelectionLeadingEdge,
          isSelectionTrailingEdge: isSelectionTrailingEdge,
          evaluatedGrid: evaluatedGrid,
          condRangeValues: condRangeValues,
          maxPositiveValue: maxPositiveValue,
          palette: palette
        )
      }
    }
  }

  @ViewBuilder
  private func gridCellView(
    row: Int,
    zeroRow: Int,
    colIdx: Int,
    addr: String,
    coord: PackedCellCoordinate,
    rowHeightPt: CGFloat,
    activeRange: CellRange,
    isPrimarySelected: Bool,
    isInSelectedRange: Bool,
    isSelectionTopEdge: Bool,
    isSelectionBottomEdge: Bool,
    isSelectionLeadingEdge: Bool,
    isSelectionTrailingEdge: Bool,
    evaluatedGrid: [PackedCellCoordinate: EvaluatedCellValue],
    condRangeValues: [EvaluatedCellValue],
    maxPositiveValue: Double,
    palette: WorkspaceThemePalette
  ) -> some View {
    let cellAddress = CellAddress(packed: coord)
    let colWidthPt = CGFloat(sheet.columnWidth(at: colIdx))
    let evaluated = evaluatedGrid[coord] ?? .empty
    let displayValue = sheet.formattedDisplayValue(at: cellAddress, using: evaluatedGrid)
    let cellStyle = sheet.cellStyle(at: coord)
    let hasComment = sheet.sheetComments[coord] != nil
    let matchingMergedRange = sheet.mergedRanges.first(where: { $0.contains(coord) })
    let isMergedAnchor = matchingMergedRange?.start == cellAddress
    let isInlineEditing = isPrimaryPane && (inlineEditingCoord == coord)
    let isCondHighlighted =
      viewModel.activeConditionalFormatRule?.matches(
        evaluated,
        allValuesInRange: condRangeValues
      ) ?? false
    let cellFillColor = cellStyle.fillHex.map {
      EasyOfficeDesignTokens.resolveHexColor($0, fallback: palette.cardSurface.swiftUIColor)
    }
    let cellTextColor: Color? = {
      if let textHex = cellStyle.textHex {
        return EasyOfficeDesignTokens.resolveHexColor(
          textHex,
          fallback: palette.primaryText.swiftUIColor
        )
      }
      if let fillHex = cellStyle.fillHex {
        return EasyOfficeDesignTokens.contrastingTextColor(forFillHex: fillHex)
      }
      return nil
    }()
    let frameAlignment: Alignment = {
      switch cellStyle.alignment {
      case .leading: return .leading
      case .center: return .center
      case .trailing: return .trailing
      }
    }()

    Button {
      guard isPrimaryPane else { return }
      if inlineEditingCoord != coord {
        inlineEditingCoord = nil
      }
      if NSEvent.modifierFlags.contains(.shift) {
        viewModel.extendSheetSelection(to: addr)
      } else {
        viewModel.selectSheetCellOrRange(primaryCell: addr)
      }
    } label: {
      ZStack(alignment: frameAlignment) {
        if viewModel.isDataBarsEnabled,
          case .number(let num) = evaluated,
          num.isFinite,
          num > 0
        {
          GeometryReader { geo in
            RoundedRectangle(cornerRadius: 2)
              .fill(EasyOfficeDesignTokens.brand700.swiftUIColor.opacity(0.18))
              .frame(
                width: CGFloat(min(1.0, num / maxPositiveValue)) * max(0, geo.size.width - 4),
                height: max(4, geo.size.height - 4)
              )
              .offset(x: 2, y: 2)
          }
        }

        if isInlineEditing {
          TextField("셀 값 입력", text: $viewModel.formulaBarDraft)
            .textFieldStyle(.plain)
            .font(.system(size: 12, weight: .semibold, design: .monospaced))
            .foregroundStyle(palette.primaryText.swiftUIColor)
            .padding(.horizontal, 6)
            .onSubmit {
              viewModel.applyFormulaBarEdit()
              inlineEditingCoord = nil
            }
        } else {
          Text(displayValue)
            .font(.system(size: 12, weight: (cellStyle.isBold || row == 1) ? .bold : .regular))
            .italic(cellStyle.isItalic)
            .underline(cellStyle.isUnderlined)
            .foregroundStyle(
              cellTextColor
                ?? (isCondHighlighted
                  ? palette.accentBadgeText.swiftUIColor
                  : palette.primaryText.swiftUIColor)
            )
            .lineLimit(cellStyle.wrapText ? 2 : 1)
            .frame(maxWidth: .infinity, alignment: frameAlignment)
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
        }

        if isMergedAnchor {
          Text("병합")
            .font(.system(size: 7.5, weight: .bold))
            .padding(.horizontal, 3)
            .padding(.vertical, 0.5)
            .background(palette.primaryAccent.swiftUIColor.opacity(0.18))
            .foregroundStyle(palette.primaryAccent.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 2))
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .padding(2)
        }

        if hasComment {
          Circle()
            .fill(EasyOfficeDesignTokens.statusWarning.swiftUIColor)
            .frame(width: 6, height: 6)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
            .padding(3)
            .help(sheet.sheetComments[coord] ?? "")
        }
      }
      .frame(width: colWidthPt, height: rowHeightPt, alignment: frameAlignment)
      .background(
        resolveCellBackground(
          isPrimaryCell: isPrimarySelected,
          isInRange: isInSelectedRange,
          cellFillColor: cellFillColor,
          isCondHighlighted: isCondHighlighted,
          palette: palette
        )
      )
      .overlay(
        Rectangle()
          .strokeBorder(
            matchingMergedRange != nil
              ? palette.primaryAccent.swiftUIColor.opacity(0.55)
              : palette.border.swiftUIColor.opacity(0.65),
            lineWidth: matchingMergedRange != nil ? 1.0 : 0.5
          )
      )
      .overlay(alignment: .top) {
        if isSelectionTopEdge {
          Rectangle()
            .fill(palette.primaryAccent.swiftUIColor)
            .frame(height: 2)
        }
      }
      .overlay(alignment: .bottom) {
        if isSelectionBottomEdge {
          Rectangle()
            .fill(palette.primaryAccent.swiftUIColor)
            .frame(height: 2)
        }
      }
      .overlay(alignment: .leading) {
        if isSelectionLeadingEdge {
          Rectangle()
            .fill(palette.primaryAccent.swiftUIColor)
            .frame(width: 2)
        }
      }
      .overlay(alignment: .trailing) {
        if isSelectionTrailingEdge {
          Rectangle()
            .fill(palette.primaryAccent.swiftUIColor)
            .frame(width: 2)
        }
      }
      .overlay(alignment: .bottomTrailing) {
        if isPrimaryPane && isSelectionBottomEdge && isSelectionTrailingEdge {
          Rectangle()
            .fill(palette.primaryAccent.swiftUIColor)
            .frame(width: 6, height: 6)
            .help("자동 채우기 핸들 (■): 드래그하여 수식·수열 채우기")
            .gesture(
              DragGesture(minimumDistance: 2)
                .onEnded { value in
                  let minR = min(activeRange.start.row, activeRange.end.row)
                  let maxR = max(activeRange.start.row, activeRange.end.row)
                  let minC = min(activeRange.start.column, activeRange.end.column)
                  let maxC = max(activeRange.start.column, activeRange.end.column)
                  let startAddr = CellAddress(
                    packed: PackedCellCoordinate(row: minR, column: minC))
                  if abs(value.translation.height) >= abs(value.translation.width) {
                    let extraRows = max(
                      1, Int(round(max(1.0, value.translation.height) / max(22.0, rowHeightPt)))
                    )
                    let endR = min(CellAddress.maxZeroBasedRow, maxR + extraRows)
                    let endAddr = CellAddress(
                      packed: PackedCellCoordinate(row: endR, column: maxC))
                    viewModel.performAutoFillOnActiveSheet(
                      targetRangeNotation: "\(startAddr.a1Notation):\(endAddr.a1Notation)"
                    )
                  } else {
                    let extraCols = max(
                      1, Int(round(max(1.0, value.translation.width) / max(48.0, colWidthPt))))
                    let endC = min(CellAddress.maxZeroBasedColumn, maxC + extraCols)
                    let endAddr = CellAddress(
                      packed: PackedCellCoordinate(row: maxR, column: endC))
                    viewModel.performAutoFillOnActiveSheet(
                      targetRangeNotation: "\(startAddr.a1Notation):\(endAddr.a1Notation)"
                    )
                  }
                }
            )
        }
      }
    }
    .buttonStyle(.plain)
    .simultaneousGesture(
      TapGesture(count: 2)
        .onEnded {
          guard isPrimaryPane else { return }
          viewModel.selectSheetCellOrRange(primaryCell: addr)
          inlineEditingCoord = coord
        }
    )
    .simultaneousGesture(
      DragGesture(minimumDistance: 12)
        .onEnded { value in
          guard isPrimaryPane else { return }
          let dCols = Int(round(value.translation.width / max(48.0, colWidthPt)))
          let dRows = Int(round(value.translation.height / max(22.0, rowHeightPt)))
          let endRow = max(0, min(CellAddress.maxZeroBasedRow, zeroRow + dRows))
          let endCol = max(0, min(CellAddress.maxZeroBasedColumn, colIdx + dCols))
          let targetAddr = CellAddress(
            packed: PackedCellCoordinate(row: endRow, column: endCol))
          viewModel.selectSheetCellOrRange(primaryCell: addr)
          viewModel.extendSheetSelection(to: targetAddr.a1Notation)
        }
    )
  }

  private func resolveCellBackground(
    isPrimaryCell: Bool,
    isInRange: Bool,
    cellFillColor: Color?,
    isCondHighlighted: Bool,
    palette: WorkspaceThemePalette
  ) -> Color {
    if isPrimaryCell { return palette.selectedSurface.swiftUIColor }
    if isInRange { return palette.subtleAccentSurface.swiftUIColor }
    if let cellFillColor { return cellFillColor }
    if isCondHighlighted { return palette.subtleAccentSurface.swiftUIColor.opacity(0.8) }
    return palette.cardSurface.swiftUIColor
  }
}
