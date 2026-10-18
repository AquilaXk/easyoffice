import SwiftUI
import AppKit
import Observation
import EasyOfficeDesignSystem
import EasyOfficeCore

/// Rich interactive editor pane for Document (.docx/.odt/.md) and Korean Office (.hwp/.hwpx) modules.
public struct DocumentEditorDetailPaneView: View {
    @Environment(\.colorScheme) private var colorScheme
    @Bindable var viewModel: WorkspaceViewModel
    let document: OfficeDocumentModel
    let isPrimaryPane: Bool

    public init(viewModel: WorkspaceViewModel, document: OfficeDocumentModel, isPrimaryPane: Bool = true) {
        self.viewModel = viewModel
        self.document = document
        self.isPrimaryPane = isPrimaryPane
    }

    public var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
        let tocEntries = document.generateTableOfContents()
        let mergedPreview = document.applyingMailMerge(record: viewModel.mailMergeFields)

        VStack(alignment: .leading, spacing: 18) {
            // Top Document Metadata & Quick Actions Banner
            HStack(spacing: 10) {
                Label(document.paperSize.rawValue, systemImage: "doc.richtext")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(EasyOfficeDesignTokens.brand100.swiftUIColor)
                    .clipShape(RoundedRectangle(cornerRadius: 5))

                if document.trackChangesEnabled {
                    Label("변경 내용 추적 활성 (\(document.revisions.count)건)", systemImage: "pencil.and.outline")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(EasyOfficeDesignTokens.statusWarning.swiftUIColor)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 4)
                        .background(EasyOfficeDesignTokens.brand50.swiftUIColor)
                        .clipShape(RoundedRectangle(cornerRadius: 5))
                }

                Spacer()

                if isPrimaryPane {
                    Button {
                        viewModel.applyInlineRunFormattingToActiveParagraph(
                            isBold: true,
                            isUnderlined: true,
                            foregroundHex: "#5C6BC0"
                        )
                    } label: {
                        Label("인라인 런 강조", systemImage: "paintbrush.pointed")
                            .font(.system(size: 11, weight: .medium))
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)

                    Button {
                        viewModel.insertEmbeddedImageIntoActiveDocument()
                    } label: {
                        Label("이미지 첨부", systemImage: "photo.badge.plus")
                            .font(.system(size: 11, weight: .medium))
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)

                    TextField("추가할 문단 내용을 입력하세요...", text: $viewModel.paragraphDraftText)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11))
                        .frame(width: 220)

                    Button {
                        let trimmed = viewModel.paragraphDraftText.trimmingCharacters(in: .whitespacesAndNewlines)
                        let textToAppend = trimmed.isEmpty
                            ? "신규 검토 문단: 표준 규격 호환성 및 레이아웃 무결성 확인 완료."
                            : trimmed
                        viewModel.appendParagraphToActiveDocument(
                            text: textToAppend,
                            style: .body
                        )
                        viewModel.paragraphDraftText = ""
                    } label: {
                        Label("문단 추가", systemImage: "plus.bubble")
                            .font(.system(size: 11, weight: .semibold))
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(EasyOfficeDesignTokens.brand700.swiftUIColor)
                }
            }

            // Page Canvas Card
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    Text(document.headerText)
                        .font(.system(size: 11))
                        .foregroundStyle(palette.secondaryText.swiftUIColor)
                    Spacer()
                    Text("단 구성: \(document.sections.first?.columnCount ?? 1)단 · \(document.sections.first?.orientation.rawValue ?? PageOrientation.portrait.rawValue)")
                        .font(.system(size: 11))
                        .foregroundStyle(palette.secondaryText.swiftUIColor)
                }
                Divider()

                // Auto Table of Contents Summary Box
                if !tocEntries.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("자동 생성 목차 (TOC)")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)
                        ForEach(Array(tocEntries.enumerated()), id: \.element.paragraphID) { idx, entry in
                            Text("\(idx + 1). \(entry.title)")
                                .font(.system(size: 11))
                                .padding(.leading, CGFloat((entry.level - 1) * 12))
                                .foregroundStyle(palette.secondaryText.swiftUIColor)
                        }
                    }
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(palette.elevatedSurface.swiftUIColor)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                }

                // Editable Document Paragraphs + Inline DocumentTextRun Segments
                ForEach(document.paragraphs) { paragraph in
                    let isSelectedParagraph = viewModel.selectedParagraphID == paragraph.id
                    VStack(alignment: .leading, spacing: 6) {
                        HStack(spacing: 6) {
                            Text(paragraph.style.rawValue)
                                .font(.system(size: 9, weight: .semibold))
                                .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                            if paragraph.isBold {
                                Text("B").font(.system(size: 9, weight: .bold))
                            }
                            if paragraph.isItalic {
                                Text("I").font(.system(size: 9, weight: .semibold)).italic()
                            }
                            if paragraph.isUnderlined {
                                Text("U").font(.system(size: 9, weight: .semibold)).underline()
                            }
                            if paragraph.runs.count > 1 {
                                Text("인라인 런 \(paragraph.runs.count)구간")
                                    .font(.system(size: 9, weight: .semibold))
                                    .padding(.horizontal, 5)
                                    .padding(.vertical, 1)
                                    .background(EasyOfficeDesignTokens.brand100.swiftUIColor)
                                    .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)
                                    .clipShape(RoundedRectangle(cornerRadius: 4))
                            }
                            Spacer()
                        }

                        if isPrimaryPane {
                            TextField(
                                "문단 내용",
                                text: Binding(
                                    get: { paragraph.text },
                                    set: { newValue in
                                        viewModel.selectedParagraphID = paragraph.id
                                        viewModel.updateParagraphText(paragraphID: paragraph.id, newText: newValue)
                                    }
                                ),
                                axis: .vertical
                            )
                            .textFieldStyle(.plain)
                            .font(.system(size: paragraph.fontSizePt, weight: paragraph.isBold ? .bold : .regular))
                            .italic(paragraph.isItalic)
                            .underline(paragraph.isUnderlined)
                            .foregroundStyle(palette.primaryText.swiftUIColor)
                            .onTapGesture {
                                viewModel.selectedParagraphID = paragraph.id
                            }
                        } else {
                            Text(paragraph.text)
                                .font(.system(size: paragraph.fontSizePt, weight: paragraph.isBold ? .bold : .regular))
                                .italic(paragraph.isItalic)
                                .underline(paragraph.isUnderlined)
                                .foregroundStyle(palette.primaryText.swiftUIColor)
                        }

                        if paragraph.runs.count > 1 {
                            ScrollView(.horizontal, showsIndicators: false) {
                                HStack(spacing: 4) {
                                    ForEach(paragraph.runs) { run in
                                        let runColor = resolveOptionalHexColor(run.foregroundHex)
                                            ?? palette.primaryText.swiftUIColor
                                        Text(run.text)
                                            .font(.system(size: run.fontSizePt ?? 11.0, weight: run.isBold ? .bold : .regular))
                                            .italic(run.isItalic)
                                            .underline(run.isUnderlined)
                                            .foregroundStyle(runColor)
                                            .padding(.horizontal, 6)
                                            .padding(.vertical, 2)
                                            .background(palette.elevatedSurface.swiftUIColor)
                                            .clipShape(RoundedRectangle(cornerRadius: 4))
                                    }
                                }
                            }
                        }
                    }
                    .padding(isSelectedParagraph ? 6 : 2)
                    .background(isSelectedParagraph ? EasyOfficeDesignTokens.brand50.swiftUIColor.opacity(0.6) : Color.clear)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                }

                // Embedded Image Attachments Gallery
                if !document.images.isEmpty {
                    Divider()
                    VStack(alignment: .leading, spacing: 8) {
                        Label("첨부 미디어 개체 (\(document.images.count)건)", systemImage: "photo.on.rectangle.angled")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)

                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 10)], spacing: 10) {
                            ForEach(document.images) { img in
                                HStack(spacing: 10) {
                                    Image(systemName: "photo.artframe")
                                        .font(.system(size: 22))
                                        .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                                        .frame(width: 40, height: 40)
                                        .background(EasyOfficeDesignTokens.brand100.swiftUIColor)
                                        .clipShape(RoundedRectangle(cornerRadius: 6))

                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(img.fileName)
                                            .font(.system(size: 11, weight: .bold))
                                            .foregroundStyle(palette.primaryText.swiftUIColor)
                                        if !img.caption.isEmpty {
                                            Text(img.caption)
                                                .font(.system(size: 10))
                                                .foregroundStyle(palette.secondaryText.swiftUIColor)
                                                .lineLimit(1)
                                        }
                                        Text("\(Int(img.widthPt))×\(Int(img.heightPt)) pt · \(img.imageData.count) bytes")
                                            .font(.system(size: 9, design: .monospaced))
                                            .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                                    }
                                    Spacer()
                                }
                                .padding(10)
                                .background(palette.elevatedSurface.swiftUIColor)
                                .overlay(
                                    RoundedRectangle(cornerRadius: 6)
                                        .stroke(palette.border.swiftUIColor, lineWidth: 1)
                                )
                                .clipShape(RoundedRectangle(cornerRadius: 6))
                            }
                        }
                    }
                }

                // Editable Tables + Conditional Inline Table Formula Badge
                ForEach(document.tables) { table in
                    let hasExplicitFormulaCell = table.rows.flatMap { $0 }.contains {
                        $0.trimmingCharacters(in: .whitespaces).hasPrefix("=")
                    }
                    let lastCol = max(0, table.columnCount - 1)
                    let sumAbove = table.evaluateTableFormula("=SUM(ABOVE)", atRow: table.rowCount, column: lastCol)

                    VStack(alignment: .leading, spacing: 6) {
                        HStack {
                            Text("\(table.caption) (\(table.borderStyle.rawValue))")
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(palette.primaryText.swiftUIColor)
                            Spacer()
                            if let sumAbove, (hasExplicitFormulaCell || sumAbove > 0) {
                                Text("표 수식 =SUM(ABOVE): \(Int(sumAbove))")
                                    .font(.system(size: 10, weight: .bold, design: .monospaced))
                                    .foregroundStyle(EasyOfficeDesignTokens.statusSuccess.swiftUIColor)
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 2)
                                    .background(EasyOfficeDesignTokens.brand50.swiftUIColor)
                                    .clipShape(RoundedRectangle(cornerRadius: 4))
                            }
                        }

                        ForEach(Array(table.rows.enumerated()), id: \.offset) { rowIdx, row in
                            HStack(spacing: 2) {
                                ForEach(Array(row.enumerated()), id: \.offset) { colIdx, cell in
                                    if isPrimaryPane {
                                        TextField(
                                            "",
                                            text: Binding(
                                                get: { cell },
                                                set: { updated in
                                                    viewModel.updateTableCell(
                                                        tableID: table.id,
                                                        row: rowIdx,
                                                        col: colIdx,
                                                        newValue: updated
                                                    )
                                                }
                                            )
                                        )
                                        .textFieldStyle(.plain)
                                        .font(.system(size: 12, weight: rowIdx == 0 && table.hasHeaderRow ? .bold : .regular))
                                        .foregroundStyle(palette.primaryText.swiftUIColor)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                        .padding(6)
                                        .background(
                                            rowIdx == 0 && table.hasHeaderRow
                                                ? EasyOfficeDesignTokens.brand100.swiftUIColor
                                                : palette.elevatedSurface.swiftUIColor
                                        )
                                    } else {
                                        Text(cell)
                                            .font(.system(size: 12, weight: rowIdx == 0 && table.hasHeaderRow ? .bold : .regular))
                                            .foregroundStyle(palette.primaryText.swiftUIColor)
                                            .frame(maxWidth: .infinity, alignment: .leading)
                                            .padding(6)
                                            .background(
                                                rowIdx == 0 && table.hasHeaderRow
                                                    ? EasyOfficeDesignTokens.brand100.swiftUIColor
                                                    : palette.elevatedSurface.swiftUIColor
                                            )
                                    }
                                }
                            }
                        }
                    }
                }

                // Footnotes & Mail Merge Preview Footer
                if !document.footnotes.isEmpty {
                    Divider()
                    ForEach(document.footnotes) { note in
                        Text("각주 \(note.number)) \(note.text)")
                            .font(.system(size: 10))
                            .foregroundStyle(palette.secondaryText.swiftUIColor)
                    }
                }

                if let mergedHeading = mergedPreview.paragraphs.first(where: { mergedParagraph in
                    mergedParagraph.text != document.paragraphs.first(where: { $0.id == mergedParagraph.id })?.text
                }) {
                    HStack(spacing: 6) {
                        Image(systemName: "envelope.badge.person.crop")
                            .font(.system(size: 11))
                            .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                        Text("메일 머지 미리보기: \(mergedHeading.text)")
                            .font(.system(size: 11))
                            .foregroundStyle(palette.secondaryText.swiftUIColor)
                            .lineLimit(1)
                    }
                    .padding(8)
                    .background(EasyOfficeDesignTokens.brand50.swiftUIColor)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                }

                Divider()
                HStack {
                    Spacer()
                    Text(document.footerText)
                        .font(.system(size: 11))
                        .foregroundStyle(palette.secondaryText.swiftUIColor)
                    Spacer()
                }
            }
            .padding(28)
            .background(palette.cardSurface.swiftUIColor)
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(palette.border.swiftUIColor, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: 8))

            // Track Changes & Review Comments Strip
            if !document.revisions.isEmpty || !document.comments.isEmpty {
                HStack(alignment: .top, spacing: 12) {
                    if !document.revisions.isEmpty {
                        VStack(alignment: .leading, spacing: 6) {
                            Text("변경 이력 (\(document.revisions.count))")
                                .font(.system(size: 11, weight: .bold))
                                .foregroundStyle(palette.primaryText.swiftUIColor)
                            ForEach(document.revisions) { rev in
                                Text("[\(rev.author)] \(rev.originalText) → \(rev.proposedText)")
                                    .font(.system(size: 11))
                                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                            }
                        }
                        .padding(12)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(palette.cardSurface.swiftUIColor)
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                    }

                    if !document.comments.isEmpty {
                        VStack(alignment: .leading, spacing: 6) {
                            Text("검토 메모 (\(document.comments.count))")
                                .font(.system(size: 11, weight: .bold))
                                .foregroundStyle(palette.primaryText.swiftUIColor)
                            ForEach(document.comments) { thread in
                                Text("• \(thread.author): \(thread.message)")
                                    .font(.system(size: 11))
                                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                            }
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
}

/// Rich interactive editor pane for Sheet (.xlsx/.ods/.csv) with Formula Bar, single-context batch evaluation, per-cell style rendering, dynamic Pivot/Chart ranges, and bottom sheet tab bar.
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
            maxRow = max(maxRow, min(24, coord.row + 1))
            maxCol = max(maxCol, min(12, coord.column + 1))
        }
        return (maxRow, maxCol)
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
        let selectedDisplayResult = sheet.formattedDisplayValue(at: selectedAddress, using: evaluatedGrid)
        let detectedRange = sheet.detectedDataRange()

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

        VStack(alignment: .leading, spacing: 14) {
            // Formula Bar with 77-function catalog badge
            HStack(spacing: 8) {
                Text(viewModel.selectedSheetCell)
                    .font(.system(size: 12, weight: .bold, design: .monospaced))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(EasyOfficeDesignTokens.brand100.swiftUIColor)
                    .foregroundStyle(EasyOfficeDesignTokens.brand900.swiftUIColor)
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

                Text("결과: \(selectedDisplayResult)")
                    .font(.system(size: 12, weight: .bold))
                    .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
            }
            .padding(.horizontal, 12)
            .frame(minHeight: EasyOfficeDesignTokens.LayoutMetrics.formulaBarHeight)
            .background(palette.cardSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 6))

            // Dynamic Spreadsheet Grid (Single-Context Batch Evaluation + CellStyle rendering)
            ScrollView(.horizontal, showsIndicators: true) {
                VStack(spacing: 4) {
                    ForEach(1...maxRow, id: \.self) { row in
                        HStack(spacing: 4) {
                            ForEach(Array(columnLabels.enumerated()), id: \.offset) { colIdx, col in
                                let coord = PackedCellCoordinate(row: row - 1, column: colIdx)
                                let cellAddress = CellAddress(packed: coord)
                                let addr = "\(col)\(row)"
                                let isSelectedCell = viewModel.selectedSheetCell == addr
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
                                    viewModel.selectedSheetCell = addr
                                    viewModel.formulaBarDraft = sheet.rawValue(at: coord)
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
                                            .font(
                                                .system(
                                                    size: 12,
                                                    weight: (cellStyle.isBold || row == 1) ? .bold : .regular
                                                )
                                            )
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
                                    .padding(8)
                                    .frame(minWidth: 105, maxWidth: .infinity, alignment: frameAlignment)
                                    .background(
                                        isSelectedCell
                                            ? EasyOfficeDesignTokens.brand100.swiftUIColor
                                            : (cellFillColor
                                                ?? (isCondHighlighted
                                                    ? EasyOfficeDesignTokens.brand50.swiftUIColor
                                                    : palette.cardSurface.swiftUIColor))
                                    )
                                    .overlay(
                                        RoundedRectangle(cornerRadius: 4)
                                            .stroke(
                                                isSelectedCell
                                                    ? EasyOfficeDesignTokens.brand700.swiftUIColor
                                                    : palette.border.swiftUIColor,
                                                lineWidth: isSelectedCell ? 1.5 : 1.0
                                            )
                                    )
                                    .clipShape(RoundedRectangle(cornerRadius: 4))
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
            }

            // Bottom Worksheet Tab Strip
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

                Text("감지된 데이터 범위: \(detectedRange.start.a1Notation):\(detectedRange.end.a1Notation)")
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
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
                            Text(key)
                                .font(.system(size: 11))
                            Spacer()
                            if pivotSummary.rowTotals.indices.contains(idx) {
                                Text(String(format: "%.0f", pivotSummary.rowTotals[idx]))
                                    .font(.system(size: 11, weight: .semibold, design: .monospaced))
                            }
                        }
                    }
                    Divider()
                    HStack {
                        Text("총계 (Grand Total)")
                            .font(.system(size: 11, weight: .bold))
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
}

func resolveOptionalHexColor(_ hex: String?) -> Color? {
    guard let hex else { return nil }
    let cleaned = hex.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: "#", with: "")
    guard cleaned.count == 6, let rgb = UInt32(cleaned, radix: 16) else { return nil }
    let r = UInt8((rgb >> 16) & 0xFF)
    let g = UInt8((rgb >> 8) & 0xFF)
    let b = UInt8(rgb & 0xFF)
    return RGBColorToken(red: r, green: g, blue: b, hex: "#\(cleaned.uppercased())").swiftUIColor
}
