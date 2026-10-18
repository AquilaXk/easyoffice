import SwiftUI
import AppKit
import Observation
import EasyOfficeDesignSystem
import EasyOfficeCore

/// Rich interactive editor pane for Sheet (.xlsx/.ods/.csv) with Formula Bar, multi-cell range selection, Auto-Fill handle (◢), interactive column/row drag resize, column auto-filter, live range aggregation status bar, and dynamic Pivot/Chart cards.
public struct SheetEditorDetailPaneView: View {
    @Environment(\.colorScheme) private var colorScheme
    @Bindable var viewModel: WorkspaceViewModel
    let sheet: WorksheetModel
    let isPrimaryPane: Bool

    public init(viewModel: WorkspaceViewModel, sheet: WorksheetModel, isPrimaryPane: Bool = true) {
        self.viewModel = viewModel
        self.sheet = sheet
        self.isPrimaryPane = isPrimaryPane
    }

    private var gridBounds: (rows: Int, cols: Int) {
        var maxRow = 8
        var maxCol = 6
        for coord in sheet.cellsByCoordinate.keys {
            maxRow = max(maxRow, coord.row + 1)
            maxCol = max(maxCol, coord.column + 1)
        }
        return (min(128, maxRow), min(26, maxCol))
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
        let bounds = gridBounds
        let maxRow = bounds.rows
        let maxCol = bounds.cols
        let evaluatedGrid = sheet.evaluateGrid(rows: 0...(maxRow - 1), columns: 0...(maxCol - 1))
        let columnLabels = (0..<maxCol).map { CellAddress.columnLabel(for: $0) }
        let selectedAddress = (try? CellAddress(a1Notation: viewModel.selectedSheetCell))
            ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
        let fallbackRange = CellRange(start: selectedAddress, end: selectedAddress)
        let activeRange = (try? CellRange(notation: viewModel.selectedSheetRange)) ?? fallbackRange
        let selectedDisplayResult = sheet.formattedDisplayValue(at: selectedAddress, using: evaluatedGrid)
        let detectedRange = sheet.detectedDataRange()
        let matchingZeroBasedRows = sheet.filterMatchingRowIndices(
            column: min(max(0, viewModel.sheetAutoFilterColumn), maxCol - 1),
            query: viewModel.sheetAutoFilterQuery,
            maxRow: maxRow - 1,
            using: evaluatedGrid
        )
        let rangeSummary = sheet.summarizeRange(activeRange, using: evaluatedGrid)

        let pivotSummary: PivotTableSummary = {
            let valueColOffset = max(0, detectedRange.end.column - detectedRange.start.column)
            let config = PivotTableConfiguration(
                rowFieldColumnIndex: 0,
                valueFieldColumnIndex: valueColOffset,
                aggregation: .sum
            )
            return PivotTableEngine.compute(
                from: sheet,
                range: detectedRange,
                hasHeaderRow: true,
                configuration: config
            )
        }()

        let chartModel: SheetChartModel = {
            let startRow = detectedRange.start.row
            let endRow = detectedRange.end.row
            let startCol = detectedRange.start.column
            let endCol = detectedRange.end.column

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
            return SheetChartBuilder.buildChart(
                from: sheet,
                title: "\(sheet.name) 데이터 비교",
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

        VStack(alignment: .leading, spacing: 12) {
            // Formula Bar with Active Range Badge & 77-function catalog badge
            HStack(spacing: 8) {
                Text(viewModel.selectedSheetCell)
                    .font(.system(size: 12, weight: .bold, design: .monospaced))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(EasyOfficeDesignTokens.brand100.swiftUIColor)
                    .foregroundStyle(EasyOfficeDesignTokens.brand900.swiftUIColor)
                    .clipShape(RoundedRectangle(cornerRadius: 4))

                Text("범위: \(viewModel.selectedSheetRange)")
                    .font(.system(size: 11, weight: .semibold, design: .monospaced))
                    .padding(.horizontal, 6)
                    .padding(.vertical, 3)
                    .background(EasyOfficeDesignTokens.brand50.swiftUIColor)
                    .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)
                    .clipShape(RoundedRectangle(cornerRadius: 4))

                Text("fx (\(WorksheetModel.supportedFunctionNames.count)종 함수)")
                    .font(.system(size: 11, weight: .semibold, design: .monospaced))
                    .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)

                if isPrimaryPane {
                    TextField("수식 또는 값 입력 (=SUM(B2:C2))", text: $viewModel.formulaBarDraft)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 12, design: .monospaced))
                        .accessibilityLabel(AccessibilityTokens.formulaBarLabel(selectedCell: viewModel.selectedSheetCell))
                        .accessibilityHint(AccessibilityTokens.formulaBarHint)
                        .onSubmit {
                            viewModel.applyFormulaBarEdit()
                        }

                    Button("적용") {
                        viewModel.applyFormulaBarEdit()
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)
                } else {
                    Text(sheet.rawValue(at: viewModel.selectedSheetCell))
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundStyle(palette.primaryText.swiftUIColor)
                }

                Spacer()

                if !activeMergedList.isEmpty {
                    Label("병합: \(activeMergedList.joined(separator: ", "))", systemImage: "rectangle.compress.vertical")
                        .font(.system(size: 10, weight: .semibold, design: .monospaced))
                        .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)
                }

                if viewModel.isHeaderRowFrozen {
                    Label("1행 고정 활성", systemImage: "pin.fill")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                }

                Text("결과: \(selectedDisplayResult)")
                    .font(.system(size: 12, weight: .bold))
                    .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
            }
            .padding(.horizontal, 12)
            .frame(minHeight: EasyOfficeDesignTokens.LayoutMetrics.formulaBarHeight)
            .background(palette.cardSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 6))

            // Column Auto-Filter Bar & Quick Auto-Fill Controls
            if isPrimaryPane {
                HStack(spacing: 8) {
                    Label("열 자동 필터", systemImage: "line.3.horizontal.decrease.circle")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)

                    Picker("필터 열", selection: $viewModel.sheetAutoFilterColumn) {
                        ForEach(0..<maxCol, id: \.self) { colIdx in
                            Text("\(CellAddress.columnLabel(for: colIdx))열").tag(colIdx)
                        }
                    }
                    .pickerStyle(.menu)
                    .frame(width: 90)

                    TextField("열 값 필터...", text: $viewModel.sheetAutoFilterQuery)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11))
                        .frame(width: 160)

                    if !viewModel.sheetAutoFilterQuery.isEmpty {
                        Button("필터 해제") {
                            viewModel.sheetAutoFilterQuery = ""
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                    }

                    Divider().frame(height: 16)

                    Button {
                        let targetNotation = defaultAutoFillTargetNotation(for: activeRange, maxRow: maxRow)
                        viewModel.performAutoFillOnActiveSheet(targetRangeNotation: targetNotation)
                    } label: {
                        Label("자동 채우기 (◢)", systemImage: "arrow.down.right.square.fill")
                            .font(.system(size: 11, weight: .semibold))
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)

                    Spacer()

                    Text("표시 행: \(matchingZeroBasedRows.count)/\(maxRow)행")
                        .font(.system(size: 10, design: .monospaced))
                        .foregroundStyle(palette.secondaryText.swiftUIColor)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .background(palette.cardSurface.swiftUIColor)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            }

            // Dynamic Spreadsheet Grid with Interactive Column/Row Resize Handles & Auto-Fill Corner Handle
            ScrollView(.horizontal, showsIndicators: true) {
                VStack(alignment: .leading, spacing: 4) {
                    columnHeaderRowView(columnLabels: columnLabels, palette: palette)

                    if viewModel.isHeaderRowFrozen {
                        sheetRowView(
                            row: 1,
                            columnLabels: columnLabels,
                            activeRange: activeRange,
                            evaluatedGrid: evaluatedGrid,
                            palette: palette
                        )
                        Divider()
                    }

                    ScrollView(.vertical, showsIndicators: true) {
                        LazyVStack(alignment: .leading, spacing: 4) {
                            let startRow = viewModel.isHeaderRowFrozen ? min(2, maxRow) : 1
                            let visibleRows = (startRow...maxRow).filter { matchingZeroBasedRows.contains($0 - 1) }
                            ForEach(visibleRows, id: \.self) { row in
                                sheetRowView(
                                    row: row,
                                    columnLabels: columnLabels,
                                    activeRange: activeRange,
                                    evaluatedGrid: evaluatedGrid,
                                    palette: palette
                                )
                            }
                        }
                    }
                    .frame(maxHeight: 420)
                }
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
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 5)
                        .background(
                            isActiveTab
                                ? EasyOfficeDesignTokens.brand100.swiftUIColor
                                : palette.elevatedSurface.swiftUIColor
                        )
                        .foregroundStyle(
                            isActiveTab
                                ? EasyOfficeDesignTokens.brand800.swiftUIColor
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
                            .padding(.horizontal, 8)
                            .padding(.vertical, 5)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                }

                Spacer()

                let minDisplay = rangeSummary.minimum.map { String(format: "%.1f", $0) } ?? "-"
                let maxDisplay = rangeSummary.maximum.map { String(format: "%.1f", $0) } ?? "-"
                Text(
                    "합계: \(String(format: "%.1f", rangeSummary.sum)) | 평균: \(String(format: "%.1f", rangeSummary.average)) | 개수: \(rangeSummary.nonEmptyCount) (수치 \(rangeSummary.numericCount)) | 최소: \(minDisplay) | 최대: \(maxDisplay)"
                )
                .font(.system(size: 10, weight: .semibold, design: .monospaced))
                .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(EasyOfficeDesignTokens.brand50.swiftUIColor)
                .clipShape(RoundedRectangle(cornerRadius: 4))
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(palette.cardSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 6))

            // Pivot Table Summary & Visual Vector Chart Preview Cards
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 6) {
                    Label("피벗 테이블 집계 요약 (\(detectedRange.start.a1Notation):\(detectedRange.end.a1Notation))", systemImage: "tablecells.badge.ellipsis")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)
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

                let maxVal = max(1.0, chartModel.series.flatMap(\.values).max() ?? 1.0)
                VStack(alignment: .leading, spacing: 8) {
                    Label("\(chartModel.title) (\(chartModel.kind.rawValue.uppercased()))", systemImage: "chart.bar.xaxis")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)

                    ForEach(chartModel.series) { series in
                        VStack(alignment: .leading, spacing: 4) {
                            Text("• \(series.name): \(series.values.map { String(format: "%.0f", $0) }.joined(separator: ", "))")
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundStyle(palette.secondaryText.swiftUIColor)

                            HStack(spacing: 6) {
                                ForEach(Array(series.values.enumerated()), id: \.offset) { vIdx, val in
                                    let ratio = CGFloat(max(0.08, min(1.0, val / maxVal)))
                                    VStack(alignment: .leading, spacing: 2) {
                                        RoundedRectangle(cornerRadius: 3)
                                            .fill(EasyOfficeDesignTokens.brand600.swiftUIColor)
                                            .frame(width: 140 * ratio, height: 8)
                                        if chartModel.categories.indices.contains(vIdx) {
                                            Text("\(chartModel.categories[vIdx]): \(Int(val))")
                                                .font(.system(size: 9, design: .monospaced))
                                                .foregroundStyle(palette.secondaryText.swiftUIColor)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(palette.cardSurface.swiftUIColor)
                .clipShape(RoundedRectangle(cornerRadius: 8))
            }
        }
    }

    @ViewBuilder
    private func columnHeaderRowView(columnLabels: [String], palette: WorkspaceThemePalette) -> some View {
        HStack(spacing: 4) {
            Text("#")
                .font(.system(size: 10, weight: .bold, design: .monospaced))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
                .frame(width: 36, height: 24)
                .background(palette.elevatedSurface.swiftUIColor)
                .clipShape(RoundedRectangle(cornerRadius: 4))

            ForEach(Array(columnLabels.enumerated()), id: \.offset) { colIdx, colLabel in
                let colWidth = CGFloat(sheet.columnWidth(at: colIdx))
                ZStack(alignment: .trailing) {
                    Text("\(colLabel) (\(Int(colWidth))pt)")
                        .font(.system(size: 10, weight: .bold, design: .monospaced))
                        .foregroundStyle(palette.primaryText.swiftUIColor)
                        .frame(width: colWidth, height: 24)
                        .background(palette.elevatedSurface.swiftUIColor)
                        .overlay(
                            RoundedRectangle(cornerRadius: 4)
                                .stroke(palette.border.swiftUIColor, lineWidth: 1)
                        )
                        .clipShape(RoundedRectangle(cornerRadius: 4))

                    if isPrimaryPane {
                        Rectangle()
                            .fill(EasyOfficeDesignTokens.brand400.swiftUIColor.opacity(0.6))
                            .frame(width: 4, height: 20)
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
        activeRange: CellRange,
        evaluatedGrid: [PackedCellCoordinate: EvaluatedCellValue],
        palette: WorkspaceThemePalette
    ) -> some View {
        let zeroRow = row - 1
        let rowHeightPt = CGFloat(sheet.rowHeight(at: zeroRow))
        let bottomRightCoord = PackedCellCoordinate(
            row: max(activeRange.start.row, activeRange.end.row),
            column: max(activeRange.start.column, activeRange.end.column)
        )

        HStack(spacing: 4) {
            // Row Header with Bottom 4pt Drag Resize Handle
            ZStack(alignment: .bottom) {
                Text("\(row)")
                    .font(.system(size: 10, weight: .bold, design: .monospaced))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                    .frame(width: 36)
                    .frame(minHeight: rowHeightPt)
                    .background(palette.elevatedSurface.swiftUIColor)
                    .overlay(
                        RoundedRectangle(cornerRadius: 4)
                            .stroke(palette.border.swiftUIColor, lineWidth: 1)
                    )
                    .clipShape(RoundedRectangle(cornerRadius: 4))

                if isPrimaryPane {
                    Rectangle()
                        .fill(EasyOfficeDesignTokens.brand400.swiftUIColor.opacity(0.6))
                        .frame(width: 30, height: 4)
                        .gesture(
                            DragGesture(minimumDistance: 1)
                                .onEnded { value in
                                    let baseHeight = sheet.rowHeight(at: zeroRow)
                                    viewModel.updateActiveSheetRowHeight(
                                        row: zeroRow,
                                        height: baseHeight + Double(value.translation.height)
                                    )
                                }
                        )
                }
            }

            ForEach(Array(columnLabels.enumerated()), id: \.offset) { colIdx, col in
                let coord = PackedCellCoordinate(row: zeroRow, column: colIdx)
                let cellAddress = CellAddress(packed: coord)
                let addr = "\(col)\(row)"
                let colWidthPt = CGFloat(sheet.columnWidth(at: colIdx))
                let isPrimaryCell = viewModel.selectedSheetCell == addr
                let isInRange = activeRange.contains(coord)
                let isAutoFillCorner = (coord == bottomRightCoord)
                let evaluated = evaluatedGrid[coord] ?? .empty
                let displayValue = sheet.formattedDisplayValue(at: cellAddress, using: evaluatedGrid)
                let cellStyle = sheet.cellStyle(at: coord)
                let hasComment = sheet.sheetComments[coord] != nil
                let isCondHighlighted = viewModel.activeConditionalFormatRule?.matches(evaluated) ?? false
                let cellFillColor = resolveOptionalHexColor(cellStyle.fillHex)
                let cellTextColor = resolveOptionalHexColor(cellStyle.textHex)
                let frameAlignment: Alignment = {
                    switch cellStyle.alignment {
                    case .leading: return .leading
                    case .center: return .center
                    case .trailing: return .trailing
                    }
                }()

                Button {
                    guard isPrimaryPane else { return }
                    if NSEvent.modifierFlags.contains(.shift) {
                        viewModel.extendSheetSelection(to: addr)
                    } else {
                        viewModel.selectSheetCellOrRange(primaryCell: addr)
                    }
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: 4) {
                            Text(addr)
                                .font(.system(size: 9, design: .monospaced))
                                .foregroundStyle(palette.secondaryText.swiftUIColor)
                            Spacer(minLength: 2)
                            if hasComment {
                                Circle()
                                    .fill(EasyOfficeDesignTokens.statusWarning.swiftUIColor)
                                    .frame(width: 6, height: 6)
                                    .help(sheet.sheetComments[coord] ?? "")
                            }
                        }
                        Text(displayValue)
                            .font(.system(size: 12, weight: (cellStyle.isBold || row == 1) ? .bold : .regular))
                            .italic(cellStyle.isItalic)
                            .underline(cellStyle.isUnderlined)
                            .foregroundStyle(
                                cellTextColor
                                    ?? (isCondHighlighted
                                        ? EasyOfficeDesignTokens.brand800.swiftUIColor
                                        : palette.primaryText.swiftUIColor)
                            )
                            .lineLimit(1)
                            .frame(maxWidth: .infinity, alignment: frameAlignment)
                    }
                    .padding(6)
                    .frame(width: colWidthPt)
                    .frame(minHeight: rowHeightPt, alignment: frameAlignment)
                    .background(
                        resolveCellBackground(
                            isPrimaryCell: isPrimaryCell,
                            isInRange: isInRange,
                            cellFillColor: cellFillColor,
                            isCondHighlighted: isCondHighlighted,
                            palette: palette
                        )
                    )
                    .overlay(
                        RoundedRectangle(cornerRadius: 4)
                            .stroke(
                                resolveCellBorderColor(isPrimaryCell: isPrimaryCell, isInRange: isInRange, palette: palette),
                                lineWidth: resolveCellBorderWidth(isPrimaryCell: isPrimaryCell, isInRange: isInRange)
                            )
                    )
                    .overlay(alignment: .bottomTrailing) {
                        if isPrimaryPane && isAutoFillCorner {
                            Rectangle()
                                .fill(EasyOfficeDesignTokens.brand700.swiftUIColor)
                                .frame(width: 6, height: 6)
                                .help("자동 채우기 핸들 (◢): 드래그하여 수식·수열 채우기")
                                .gesture(
                                    DragGesture(minimumDistance: 2)
                                        .onEnded { value in
                                            let minR = min(activeRange.start.row, activeRange.end.row)
                                            let maxR = max(activeRange.start.row, activeRange.end.row)
                                            let minC = min(activeRange.start.column, activeRange.end.column)
                                            let maxC = max(activeRange.start.column, activeRange.end.column)
                                            let startAddr = CellAddress(packed: PackedCellCoordinate(row: minR, column: minC))
                                            if abs(value.translation.height) >= abs(value.translation.width) {
                                                let extraRows = max(1, Int(round(max(1.0, value.translation.height) / max(22.0, rowHeightPt))))
                                                let endR = min(CellAddress.maxZeroBasedRow, maxR + extraRows)
                                                let endAddr = CellAddress(packed: PackedCellCoordinate(row: endR, column: maxC))
                                                viewModel.performAutoFillOnActiveSheet(
                                                    targetRangeNotation: "\(startAddr.a1Notation):\(endAddr.a1Notation)"
                                                )
                                            } else {
                                                let extraCols = max(1, Int(round(max(1.0, value.translation.width) / max(48.0, colWidthPt))))
                                                let endC = min(CellAddress.maxZeroBasedColumn, maxC + extraCols)
                                                let endAddr = CellAddress(packed: PackedCellCoordinate(row: maxR, column: endC))
                                                viewModel.performAutoFillOnActiveSheet(
                                                    targetRangeNotation: "\(startAddr.a1Notation):\(endAddr.a1Notation)"
                                                )
                                            }
                                        }
                                )
                        }
                    }
                    .clipShape(RoundedRectangle(cornerRadius: 4))
                }
                .buttonStyle(.plain)
                .simultaneousGesture(
                    DragGesture(minimumDistance: 12)
                        .onEnded { value in
                            guard isPrimaryPane else { return }
                            let dCols = Int(round(value.translation.width / max(48.0, colWidthPt)))
                            let dRows = Int(round(value.translation.height / max(22.0, rowHeightPt)))
                            let endRow = max(0, min(CellAddress.maxZeroBasedRow, zeroRow + dRows))
                            let endCol = max(0, min(CellAddress.maxZeroBasedColumn, colIdx + dCols))
                            let targetAddr = CellAddress(packed: PackedCellCoordinate(row: endRow, column: endCol))
                            viewModel.selectSheetCellOrRange(primaryCell: addr)
                            viewModel.extendSheetSelection(to: targetAddr.a1Notation)
                        }
                )
            }
        }
    }

    private func resolveCellBackground(
        isPrimaryCell: Bool,
        isInRange: Bool,
        cellFillColor: Color?,
        isCondHighlighted: Bool,
        palette: WorkspaceThemePalette
    ) -> Color {
        if isPrimaryCell { return EasyOfficeDesignTokens.brand100.swiftUIColor }
        if isInRange { return EasyOfficeDesignTokens.brand50.swiftUIColor }
        if let cellFillColor { return cellFillColor }
        if isCondHighlighted { return EasyOfficeDesignTokens.brand50.swiftUIColor.opacity(0.6) }
        return palette.cardSurface.swiftUIColor
    }

    private func resolveCellBorderColor(isPrimaryCell: Bool, isInRange: Bool, palette: WorkspaceThemePalette) -> Color {
        if isPrimaryCell { return EasyOfficeDesignTokens.brand700.swiftUIColor }
        if isInRange { return EasyOfficeDesignTokens.brand600.swiftUIColor }
        return palette.border.swiftUIColor
    }

    private func resolveCellBorderWidth(isPrimaryCell: Bool, isInRange: Bool) -> CGFloat {
        if isPrimaryCell { return 1.8 }
        if isInRange { return 1.3 }
        return 1.0
    }
}
