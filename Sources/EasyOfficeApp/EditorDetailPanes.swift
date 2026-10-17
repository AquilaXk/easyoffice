import SwiftUI
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
        let mergedPreview = document.applyingMailMerge(record: [
            "부서명": "플랫폼아키텍처실",
            "수신자": "전사 기술위원회"
        ])

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
                        viewModel.appendParagraphToActiveDocument(
                            text: "신규 검토 문단: 표준 규격 호환성 및 레이아웃 무결성 확인 완료.",
                            style: .body
                        )
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
                    Text("단 구성: \(document.sections.first?.columnCount ?? 1)단")
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

                // Document Paragraphs
                ForEach(document.paragraphs) { paragraph in
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(paragraph.style.rawValue)
                                .font(.system(size: 9, weight: .semibold))
                                .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                            Spacer()
                        }
                        Text(paragraph.text)
                            .font(.system(size: paragraph.fontSizePt, weight: paragraph.isBold ? .bold : .regular))
                            .foregroundStyle(palette.primaryText.swiftUIColor)
                    }
                }

                // Tables + Inline Table Formula Badge
                ForEach(document.tables) { table in
                    VStack(alignment: .leading, spacing: 6) {
                        HStack {
                            Text(table.caption)
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(palette.primaryText.swiftUIColor)
                            Spacer()
                            if let sumAbove = table.evaluateTableFormula("=SUM(ABOVE)", atRow: table.rowCount, column: table.columnCount - 1),
                               sumAbove > 0 {
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
                                ForEach(Array(row.enumerated()), id: \.offset) { _, cell in
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

/// Rich interactive editor pane for Sheet (.xlsx/.ods/.csv) with Formula Bar, 77-function badge, Pivot Table summary, and Chart preview.
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

    public var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
        let pivotSummary: PivotTableSummary? = {
            guard let range = try? CellRange(notation: "A1:D4") else { return nil }
            let config = PivotTableConfiguration(rowFieldColumnIndex: 0, valueFieldColumnIndex: 3, aggregation: .sum)
            return PivotTableEngine.compute(from: sheet, range: range, hasHeaderRow: true, configuration: config)
        }()
        let chartModel: SheetChartModel? = {
            guard let catRange = try? CellRange(notation: "A2:A3"),
                  let q1Range = try? CellRange(notation: "B2:B3"),
                  let q2Range = try? CellRange(notation: "C2:C3") else {
                return nil
            }
            return SheetChartBuilder.buildChart(
                from: sheet,
                title: "분기별 실적 비교",
                kind: .column,
                categoryRange: catRange,
                valueRanges: [("1분기", q1Range), ("2분기", q2Range)]
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

                Text("결과: \(sheet.evaluate(at: viewModel.selectedSheetCell).description)")
                    .font(.system(size: 12, weight: .bold))
                    .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
            }
            .padding(.horizontal, 12)
            .frame(minHeight: EasyOfficeDesignTokens.LayoutMetrics.formulaBarHeight)
            .background(palette.cardSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 6))

            // Spreadsheet Grid
            VStack(spacing: 4) {
                ForEach(1...4, id: \.self) { row in
                    HStack(spacing: 4) {
                        ForEach(["A", "B", "C", "D"], id: \.self) { col in
                            let addr = "\(col)\(row)"
                            let isSelectedCell = viewModel.selectedSheetCell == addr
                            Button {
                                viewModel.selectedSheetCell = addr
                                viewModel.formulaBarDraft = sheet.rawValue(at: addr)
                            } label: {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(addr)
                                        .font(.system(size: 9, design: .monospaced))
                                        .foregroundStyle(palette.secondaryText.swiftUIColor)
                                    Text(sheet.evaluate(at: addr).description)
                                        .font(.system(size: 12, weight: row == 1 || row == 4 ? .bold : .regular))
                                        .foregroundStyle(palette.primaryText.swiftUIColor)
                                }
                                .padding(8)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(
                                    isSelectedCell
                                        ? EasyOfficeDesignTokens.brand100.swiftUIColor
                                        : palette.cardSurface.swiftUIColor
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

            // Pivot Table Summary & Chart Preview Cards
            HStack(alignment: .top, spacing: 12) {
                if let pivot = pivotSummary {
                    VStack(alignment: .leading, spacing: 6) {
                        Label("피벗 테이블 집계 요약", systemImage: "tablecells.badge.ellipsis")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)
                        ForEach(Array(pivot.rowKeys.enumerated()), id: \.offset) { idx, key in
                            HStack {
                                Text(key)
                                    .font(.system(size: 11))
                                Spacer()
                                Text(String(format: "%.0f", pivot.rowTotals[idx]))
                                    .font(.system(size: 11, weight: .semibold, design: .monospaced))
                            }
                        }
                        Divider()
                        HStack {
                            Text("총계 (Grand Total)")
                                .font(.system(size: 11, weight: .bold))
                            Spacer()
                            Text(String(format: "%.0f", pivot.grandTotal))
                                .font(.system(size: 11, weight: .bold, design: .monospaced))
                                .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                        }
                    }
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(palette.cardSurface.swiftUIColor)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                }

                if let chart = chartModel {
                    VStack(alignment: .leading, spacing: 6) {
                        Label("\(chart.title) (\(chart.kind.rawValue.uppercased()))", systemImage: "chart.bar.xaxis")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                        ForEach(chart.series) { series in
                            Text("• \(series.name): \(series.values.map { String(format: "%.0f", $0) }.joined(separator: ", "))")
                                .font(.system(size: 11, design: .monospaced))
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

/// Rich interactive editor pane for Slide (.pptx/.odp) with Filmstrip, Stage Canvas, Smart Alignment Guides badge, and Presenter Mode sheet.
public struct SlideEditorDetailPaneView: View {
    @Environment(\.colorScheme) private var colorScheme
    @Bindable var viewModel: WorkspaceViewModel
    let deck: PresentationDeckModel

    public init(viewModel: WorkspaceViewModel, deck: PresentationDeckModel) {
        self.viewModel = viewModel
        self.deck = deck
    }

    public var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
        let activeIdx = deck.slides.indices.contains(viewModel.selectedSlideIndex) ? viewModel.selectedSlideIndex : 0
        let currentSlide = deck.slides.indices.contains(activeIdx) ? deck.slides[activeIdx] : nil
        let presenterSession = PresenterSession(deck: deck, currentSlideIndex: activeIdx, elapsedSeconds: 95)

        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Label("\(deck.aspectRatio.rawValue) · 슬라이드 \(deck.slides.count)장", systemImage: "rectangle.on.rectangle.angled")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
                Spacer()
                Button {
                    viewModel.isPresenterModeActive.toggle()
                } label: {
                    Label(
                        viewModel.isPresenterModeActive ? "발표자 도구 닫기" : "발표자 도구 미리보기",
                        systemImage: "play.display"
                    )
                    .font(.system(size: 11, weight: .semibold))
                }
                .buttonStyle(.borderedProminent)
                .tint(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
            }

            HStack(alignment: .top, spacing: 14) {
                // Slide Filmstrip
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(Array(deck.slides.enumerated()), id: \.element.id) { idx, slide in
                        let isSelected = idx == activeIdx
                        Button {
                            viewModel.selectedSlideIndex = idx
                        } label: {
                            VStack(alignment: .leading, spacing: 4) {
                                Text("슬라이드 \(idx + 1) · \(slide.masterLayout.rawValue)")
                                    .font(.system(size: 10, weight: .bold))
                                    .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
                                Text(slide.title)
                                    .font(.system(size: 12, weight: .semibold))
                                    .foregroundStyle(palette.primaryText.swiftUIColor)
                                    .lineLimit(1)
                            }
                            .padding(10)
                            .frame(width: EasyOfficeDesignTokens.LayoutMetrics.slideFilmstripWidth, alignment: .leading)
                            .background(
                                isSelected
                                    ? EasyOfficeDesignTokens.brand100.swiftUIColor
                                    : palette.cardSurface.swiftUIColor
                            )
                            .overlay(
                                RoundedRectangle(cornerRadius: 6)
                                    .stroke(
                                        isSelected
                                            ? EasyOfficeDesignTokens.brand700.swiftUIColor
                                            : palette.border.swiftUIColor,
                                        lineWidth: isSelected ? 1.5 : 1.0
                                    )
                            )
                            .clipShape(RoundedRectangle(cornerRadius: 6))
                        }
                        .buttonStyle(.plain)
                    }
                }

                // Active Slide Stage Canvas
                if let slide = currentSlide {
                    VStack(alignment: .leading, spacing: 12) {
                        HStack {
                            Text(slide.masterLayout.rawValue)
                                .font(.system(size: 10, weight: .bold))
                                .padding(.horizontal, 6)
                                .padding(.vertical, 2)
                                .background(EasyOfficeDesignTokens.softBadgeToken(for: .slide).swiftUIColor)
                                .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
                                .clipShape(RoundedRectangle(cornerRadius: 4))

                            Text("전환: \(slide.transition.rawValue)")
                                .font(.system(size: 10))
                                .foregroundStyle(palette.secondaryText.swiftUIColor)

                            Spacer()

                            if let firstElem = slide.elements.first {
                                let guides = SnapGuideCalculator.computeGuides(
                                    for: firstElem,
                                    siblings: slide.elements,
                                    aspectRatio: deck.aspectRatio
                                )
                                if !guides.isEmpty {
                                    Label("스마트 정렬 가이드 스냅 활성 (\(guides.count)축)", systemImage: "ruler")
                                        .font(.system(size: 10, weight: .semibold))
                                        .foregroundStyle(EasyOfficeDesignTokens.statusSuccess.swiftUIColor)
                                }
                            }
                        }

                        Text(slide.title)
                            .font(.system(size: 22, weight: .bold))
                            .foregroundStyle(palette.primaryText.swiftUIColor)

                        if !slide.subtitle.isEmpty {
                            Text(slide.subtitle)
                                .font(.system(size: 14, weight: .medium))
                                .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                        }

                        ForEach(slide.bulletPoints, id: \.self) { bullet in
                            Text("• \(bullet)")
                                .font(.system(size: 13))
                                .foregroundStyle(palette.primaryText.swiftUIColor)
                        }

                        if !slide.elements.isEmpty {
                            HStack(spacing: 8) {
                                ForEach(slide.elements) { element in
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(element.title)
                                            .font(.system(size: 11, weight: .bold))
                                        Text(element.bodyText)
                                            .font(.system(size: 10))
                                            .foregroundStyle(palette.secondaryText.swiftUIColor)
                                    }
                                    .padding(10)
                                    .background(EasyOfficeDesignTokens.brand50.swiftUIColor)
                                    .overlay(
                                        RoundedRectangle(cornerRadius: 6)
                                            .stroke(EasyOfficeDesignTokens.brand600.swiftUIColor, lineWidth: 1)
                                    )
                                    .clipShape(RoundedRectangle(cornerRadius: 6))
                                }
                            }
                        }

                        Divider()
                        Text("발표자 노트: \(slide.speakerNotes)")
                            .font(.system(size: 11))
                            .foregroundStyle(palette.secondaryText.swiftUIColor)
                    }
                    .padding(24)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(palette.cardSurface.swiftUIColor)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .accessibilityLabel(AccessibilityTokens.slideStageLabel(slideNumber: activeIdx + 1, totalSlides: deck.slides.count))
                }
            }

            if viewModel.isPresenterModeActive {
                HStack(spacing: 16) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("발표자 모니터 · 진행률 \(Int(presenterSession.progressFraction * 100))%")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)
                        Text("현재 노트: \(presenterSession.currentSpeakerNotes)")
                            .font(.system(size: 11))
                            .foregroundStyle(palette.secondaryText.swiftUIColor)
                    }
                    Spacer()
                    if let nextSlide = presenterSession.nextSlide {
                        Text("다음 슬라이드: \(nextSlide.title)")
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                    }
                }
                .padding(12)
                .background(EasyOfficeDesignTokens.brand50.swiftUIColor)
                .clipShape(RoundedRectangle(cornerRadius: 8))
            }
        }
    }
}

/// Rich interactive editor pane for PDF Studio (.pdf) with Page Thumbnails, Outline Tree, Annotation Toolbar, and Rotate/Convert controls.
public struct PDFStudioEditorDetailPaneView: View {
    @Environment(\.colorScheme) private var colorScheme
    @Bindable var viewModel: WorkspaceViewModel
    let pdf: PDFStudioDocumentModel
    let isPrimaryPane: Bool

    public init(viewModel: WorkspaceViewModel, pdf: PDFStudioDocumentModel, isPrimaryPane: Bool = true) {
        self.viewModel = viewModel
        self.pdf = pdf
        self.isPrimaryPane = isPrimaryPane
    }

    public var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)

        VStack(alignment: .leading, spacing: 14) {
            // PDF Studio Action & Annotation Toolbar
            if isPrimaryPane {
                HStack(spacing: 8) {
                    Button {
                        viewModel.rotateActivePDFPage(clockwiseDegrees: 90)
                    } label: {
                        Label("90° 회전", systemImage: "rotate.right")
                            .font(.system(size: 11, weight: .semibold))
                    }
                    .buttonStyle(.bordered)

                    ForEach([PDFAnnotationKind.highlight, .stickyNote, .signature], id: \.rawValue) { kind in
                        Button {
                            viewModel.addPDFAnnotation(kind: kind, content: "\(kind.rawValue) 검토 마크")
                        } label: {
                            Text(kind.rawValue)
                                .font(.system(size: 11, weight: .medium))
                        }
                        .buttonStyle(.bordered)
                        .accessibilityLabel(AccessibilityTokens.pdfAnnotationToolLabel(toolTitle: kind.rawValue))
                    }

                    Spacer()

                    Text("주석 \(pdf.annotations.count)건 · 책갈피 \(pdf.outline.count)건")
                        .font(.system(size: 11))
                        .foregroundStyle(palette.secondaryText.swiftUIColor)
                }
                .padding(10)
                .background(palette.cardSurface.swiftUIColor)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            }

            // PDF Pages List
            ForEach(Array(pdf.pages.enumerated()), id: \.element.id) { idx, page in
                let isSelected = viewModel.selectedPDFPageIndex == idx
                let pageAnnotations = pdf.annotations.filter { $0.pageNumber == page.pageNumber }

                VStack(alignment: .leading, spacing: 10) {
                    HStack {
                        Text("PDF 페이지 \(page.pageNumber) · 회전 \(page.rotationDegrees)°")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
                        Spacer()
                        if isSelected {
                            Text("선택된 페이지")
                                .font(.system(size: 10, weight: .semibold))
                                .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                        }
                    }

                    Text(page.extractedText)
                        .font(.system(size: 13))
                        .foregroundStyle(palette.primaryText.swiftUIColor)

                    if !pageAnnotations.isEmpty {
                        Divider()
                        ForEach(pageAnnotations) { ann in
                            HStack(spacing: 6) {
                                Text("[\(ann.kind.rawValue)]")
                                    .font(.system(size: 10, weight: .bold))
                                    .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
                                Text(ann.content)
                                    .font(.system(size: 11))
                                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                            }
                        }
                    }
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(palette.cardSurface.swiftUIColor)
                .overlay(
                    RoundedRectangle(cornerRadius: 8)
                        .stroke(
                            isSelected
                                ? EasyOfficeDesignTokens.brand700.swiftUIColor
                                : palette.border.swiftUIColor,
                            lineWidth: isSelected ? 1.5 : 1.0
                        )
                )
                .clipShape(RoundedRectangle(cornerRadius: 8))
                .onTapGesture {
                    viewModel.selectedPDFPageIndex = idx
                }
            }
        }
    }
}
