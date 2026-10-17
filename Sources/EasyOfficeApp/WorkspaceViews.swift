import SwiftUI
import Observation
import EasyOfficeDesignSystem
import EasyOfficeCore

public struct MainWorkspaceWindowView: View {
    @Bindable var viewModel: WorkspaceViewModel

    public init(viewModel: WorkspaceViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: false)

        VStack(spacing: 0) {
            TopDocumentTabStripView(viewModel: viewModel)
            Divider()

            if viewModel.isShowingHomeHub || viewModel.activeTab == nil {
                HomeHubWorkspaceView(viewModel: viewModel)
            } else if let tab = viewModel.activeTab {
                ContextRibbonBarView(viewModel: viewModel, activeModule: tab.moduleKind)
                Divider()
                HStack(spacing: 0) {
                    if viewModel.showLeftNavigator {
                        LeftNavigatorPanelView(viewModel: viewModel, tab: tab)
                            .frame(width: EasyOfficeDesignTokens.LayoutMetrics.leftNavigatorWidth)
                        Divider()
                    }

                    // Primary + Optional Split View Secondary Editor Canvas
                    HStack(spacing: 0) {
                        CentralEditorCanvasView(viewModel: viewModel, tab: tab, isPrimaryPane: true)
                            .frame(
                                minWidth: EasyOfficeDesignTokens.LayoutMetrics.splitPaneMinimumWidth,
                                maxWidth: .infinity,
                                maxHeight: .infinity
                            )
                            .accessibilityLabel(AccessibilityTokens.splitViewPaneLabel(isPrimary: true, fileName: tab.fileName))

                        if let secondaryTab = viewModel.splitSecondaryTab {
                            Rectangle()
                                .fill(EasyOfficeDesignTokens.brand400.swiftUIColor)
                                .frame(width: 2)

                            CentralEditorCanvasView(viewModel: viewModel, tab: secondaryTab, isPrimaryPane: false)
                                .frame(
                                    minWidth: EasyOfficeDesignTokens.LayoutMetrics.splitPaneMinimumWidth,
                                    maxWidth: .infinity,
                                    maxHeight: .infinity
                                )
                                .accessibilityLabel(AccessibilityTokens.splitViewPaneLabel(isPrimary: false, fileName: secondaryTab.fileName))
                        }
                    }

                    if viewModel.showRightInspector {
                        Divider()
                        RightContextualInspectorView(viewModel: viewModel, tab: tab)
                            .frame(width: EasyOfficeDesignTokens.LayoutMetrics.rightInspectorWidth)
                    }
                }
                Divider()
                BottomStatusBarView(viewModel: viewModel, tab: tab)
            }
        }
        .background(palette.background.swiftUIColor)
    }
}

struct TopDocumentTabStripView: View {
    @Bindable var viewModel: WorkspaceViewModel

    var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: false)

        HStack(spacing: 8) {
            Button {
                viewModel.isShowingHomeHub.toggle()
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "square.grid.2x2.fill")
                    Text("홈 허브")
                        .font(.system(size: 12, weight: .semibold))
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .background(
                    viewModel.isShowingHomeHub
                        ? EasyOfficeDesignTokens.brand700.swiftUIColor
                        : EasyOfficeDesignTokens.brand100.swiftUIColor
                )
                .foregroundStyle(
                    viewModel.isShowingHomeHub
                        ? Color.white
                        : EasyOfficeDesignTokens.brand900.swiftUIColor
                )
                .clipShape(RoundedRectangle(cornerRadius: 6))
            }
            .buttonStyle(.plain)

            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach(viewModel.openTabs) { tab in
                        let isSelected = !viewModel.isShowingHomeHub && viewModel.activeTabID == tab.id
                        let isSecondarySplit = viewModel.splitSecondaryTabID == tab.id
                        let accent = EasyOfficeDesignTokens.accentToken(for: tab.moduleKind)
                        HStack(spacing: 6) {
                            if tab.isPinned {
                                Image(systemName: "pin.fill")
                                    .font(.system(size: 9))
                                    .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                            }
                            if tab.isFavorite {
                                Image(systemName: "star.fill")
                                    .font(.system(size: 9))
                                    .foregroundStyle(EasyOfficeDesignTokens.statusWarning.swiftUIColor)
                            }

                            Text(tab.moduleKind.shortBadgeTitle)
                                .font(.system(size: 9, weight: .bold))
                                .padding(.horizontal, 5)
                                .padding(.vertical, 2)
                                .background(accent.swiftUIColor.opacity(0.16))
                                .foregroundStyle(accent.swiftUIColor)
                                .clipShape(RoundedRectangle(cornerRadius: 4))

                            Text(tab.fileName)
                                .font(.system(size: 12, weight: isSelected ? .semibold : .regular))
                                .foregroundStyle(palette.primaryText.swiftUIColor)

                            if tab.isModified {
                                Circle()
                                    .fill(EasyOfficeDesignTokens.brand600.swiftUIColor)
                                    .frame(width: 6, height: 6)
                            }

                            Button {
                                viewModel.closeTab(id: tab.id)
                            } label: {
                                Image(systemName: "xmark")
                                    .font(.system(size: 9, weight: .bold))
                                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                            }
                            .buttonStyle(.plain)
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(
                            isSelected
                                ? palette.cardSurface.swiftUIColor
                                : palette.elevatedSurface.swiftUIColor
                        )
                        .overlay(
                            RoundedRectangle(cornerRadius: 6)
                                .stroke(
                                    isSelected
                                        ? EasyOfficeDesignTokens.brand700.swiftUIColor
                                        : (isSecondarySplit ? EasyOfficeDesignTokens.brand400.swiftUIColor : palette.border.swiftUIColor),
                                    lineWidth: isSelected || isSecondarySplit ? 1.5 : 1.0
                                )
                        )
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                        .accessibilityLabel(
                            AccessibilityTokens.tabLabel(
                                fileName: tab.fileName,
                                module: tab.moduleKind,
                                isPinned: tab.isPinned,
                                isModified: tab.isModified
                            )
                        )
                        .accessibilityHint(AccessibilityTokens.tabHint)
                        .onTapGesture {
                            viewModel.activeTabID = tab.id
                            viewModel.isShowingHomeHub = false
                        }
                        .contextMenu {
                            Button(tab.isPinned ? "탭 고정 해제" : "탭 고정") {
                                viewModel.togglePinTab(id: tab.id)
                            }
                            Button(tab.isFavorite ? "즐겨찾기 해제" : "즐겨찾기 추가") {
                                viewModel.toggleFavoriteTab(id: tab.id)
                            }
                            Button("탭 복제") {
                                viewModel.duplicateTab(id: tab.id)
                            }
                            if tab.id != viewModel.activeTabID {
                                Button(viewModel.splitSecondaryTabID == tab.id ? "스플릿 뷰 닫기" : "우측 스플릿 뷰로 열기") {
                                    if viewModel.splitSecondaryTabID == tab.id {
                                        viewModel.enableSplitView(secondaryTabID: nil)
                                    } else {
                                        viewModel.enableSplitView(secondaryTabID: tab.id)
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Spacer()

            Picker("테마", selection: $viewModel.themeMode) {
                ForEach(WorkspaceThemeMode.allCases) { mode in
                    Text(mode.rawValue).tag(mode)
                }
            }
            .pickerStyle(.menu)
            .frame(width: 145)

            TextField("문서·본문·수식 통합 검색...", text: $viewModel.searchQuery)
                .textFieldStyle(.roundedBorder)
                .frame(width: 210)
        }
        .padding(.horizontal, 12)
        .frame(height: EasyOfficeDesignTokens.LayoutMetrics.tabStripHeight + 6)
        .background(palette.elevatedSurface.swiftUIColor)
    }
}

struct ContextRibbonBarView: View {
    @Bindable var viewModel: WorkspaceViewModel
    let activeModule: OfficeModuleKind

    var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: false)
        let commands = RibbonCategory.actions(for: viewModel.activeRibbonCategory, module: activeModule)

        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 12) {
                ForEach(RibbonCategory.allCases) { category in
                    let isSelected = viewModel.activeRibbonCategory == category
                    Button {
                        viewModel.activeRibbonCategory = category
                    } label: {
                        Text(category.title(for: activeModule))
                            .font(.system(size: 12, weight: isSelected ? .bold : .medium))
                            .foregroundStyle(
                                isSelected
                                    ? EasyOfficeDesignTokens.brand700.swiftUIColor
                                    : palette.secondaryText.swiftUIColor
                            )
                            .padding(.vertical, 4)
                            .padding(.horizontal, 8)
                            .background(
                                isSelected
                                    ? EasyOfficeDesignTokens.brand100.swiftUIColor
                                    : Color.clear
                            )
                            .clipShape(RoundedRectangle(cornerRadius: 5))
                    }
                    .buttonStyle(.plain)
                }

                Spacer()

                if viewModel.splitSecondaryTab != nil {
                    Button {
                        viewModel.enableSplitView(secondaryTabID: nil)
                    } label: {
                        Label("스플릿 해제", systemImage: "rectangle.split.2x1.slash")
                            .font(.system(size: 11, weight: .semibold))
                    }
                } else if let candidate = viewModel.openTabs.first(where: { $0.id != viewModel.activeTabID }) {
                    Button {
                        viewModel.enableSplitView(secondaryTabID: candidate.id)
                    } label: {
                        Label("스플릿 뷰", systemImage: "rectangle.split.2x1")
                            .font(.system(size: 11))
                    }
                }

                Button {
                    viewModel.showLeftNavigator.toggle()
                } label: {
                    Label("내비게이터", systemImage: "sidebar.left")
                        .font(.system(size: 11))
                }

                Button {
                    viewModel.showRightInspector.toggle()
                } label: {
                    Label("인스펙터", systemImage: "sidebar.right")
                        .font(.system(size: 11))
                }

                Button {
                    viewModel.isRibbonCollapsed.toggle()
                } label: {
                    Image(systemName: viewModel.isRibbonCollapsed ? "chevron.down" : "chevron.up")
                        .font(.system(size: 11, weight: .semibold))
                }
                .accessibilityLabel(AccessibilityTokens.ribbonCollapseToggleLabel(isCollapsed: viewModel.isRibbonCollapsed))
            }

            if !viewModel.isRibbonCollapsed {
                HStack(spacing: 8) {
                    ForEach(commands) { command in
                        Button {
                            handleRibbonCommand(command)
                        } label: {
                            HStack(spacing: 6) {
                                Image(systemName: command.symbolName)
                                    .font(.system(size: 11))
                                    .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: activeModule).swiftUIColor)
                                Text(command.title)
                                    .font(.system(size: 11, weight: .medium))
                                    .foregroundStyle(palette.primaryText.swiftUIColor)
                                Text(command.shortcutHint)
                                    .font(.system(size: 9, design: .monospaced))
                                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                            }
                            .padding(.horizontal, 10)
                            .padding(.vertical, 5)
                            .background(palette.elevatedSurface.swiftUIColor)
                            .overlay(
                                RoundedRectangle(cornerRadius: 6)
                                    .stroke(palette.border.swiftUIColor, lineWidth: 1)
                            )
                            .clipShape(RoundedRectangle(cornerRadius: 6))
                        }
                        .buttonStyle(.plain)
                        .accessibilityHint(AccessibilityTokens.ribbonControlHint)
                    }
                    Spacer()
                }
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 6)
        .background(palette.cardSurface.swiftUIColor)
    }

    private func handleRibbonCommand(_ command: RibbonCommandDescriptor) {
        if command.id.hasSuffix("-view-ribbon") {
            viewModel.isRibbonCollapsed.toggle()
        } else if command.id.hasSuffix("-view-zoom") {
            viewModel.setZoom(100.0)
        } else if command.id == "pdf-mod-rotate" {
            viewModel.rotateActivePDFPage(clockwiseDegrees: 90)
        } else if command.id == "slide-mod-presenter" {
            viewModel.isPresenterModeActive.toggle()
        }
    }
}

struct HomeHubWorkspaceView: View {
    @Bindable var viewModel: WorkspaceViewModel

    var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: false)

        HStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 6) {
                Text("EasyOffice")
                    .font(.system(size: 18, weight: .bold))
                    .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)
                    .padding(.bottom, 8)

                ForEach(HubSection.allCases) { section in
                    let selected = viewModel.activeHubSection == section
                    Button {
                        viewModel.activeHubSection = section
                    } label: {
                        HStack(spacing: 10) {
                            Image(systemName: section.symbolName)
                            Text(section.rawValue)
                                .font(.system(size: 13, weight: selected ? .semibold : .regular))
                            Spacer()
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 7)
                        .background(
                            selected
                                ? EasyOfficeDesignTokens.brand100.swiftUIColor
                                : Color.clear
                        )
                        .foregroundStyle(
                            selected
                                ? EasyOfficeDesignTokens.brand800.swiftUIColor
                                : palette.primaryText.swiftUIColor
                        )
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                    }
                    .buttonStyle(.plain)
                }
                Spacer()
            }
            .padding(16)
            .frame(width: 220)
            .background(palette.cardSurface.swiftUIColor)

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    Text("새 문서 시작하기")
                        .font(.system(size: 16, weight: .bold))
                        .foregroundStyle(palette.primaryText.swiftUIColor)

                    HStack(spacing: 12) {
                        ForEach(OfficeModuleKind.allCases) { module in
                            let accent = EasyOfficeDesignTokens.accentToken(for: module)
                            Button {
                                viewModel.openNewSampleTab(for: module)
                            } label: {
                                VStack(alignment: .leading, spacing: 8) {
                                    Image(systemName: module.systemSymbolName)
                                        .font(.system(size: 22))
                                        .foregroundStyle(accent.swiftUIColor)
                                    Text(module.displayNameKo)
                                        .font(.system(size: 13, weight: .semibold))
                                        .foregroundStyle(palette.primaryText.swiftUIColor)
                                }
                                .padding(14)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(palette.cardSurface.swiftUIColor)
                                .overlay(
                                    RoundedRectangle(cornerRadius: 10)
                                        .stroke(palette.border.swiftUIColor, lineWidth: 1)
                                )
                                .clipShape(RoundedRectangle(cornerRadius: 10))
                            }
                            .buttonStyle(.plain)
                        }
                    }

                    HStack {
                        Text("표준 및 사용자 서식 템플릿 (\(viewModel.allAvailableTemplates.count)종)")
                            .font(.system(size: 15, weight: .bold))
                            .foregroundStyle(palette.primaryText.swiftUIColor)
                        Spacer()
                        Button("현재 탭을 사용자 템플릿으로 저장") {
                            viewModel.saveActiveTabAsCustomTemplate(
                                title: "사용자 정의 표준 양식",
                                subtitle: "현재 활성 문서 기반 커스텀 템플릿"
                            )
                        }
                        .font(.system(size: 11, weight: .semibold))
                    }

                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 240), spacing: 12)], spacing: 12) {
                        ForEach(viewModel.allAvailableTemplates) { tpl in
                            VStack(alignment: .leading, spacing: 6) {
                                HStack {
                                    Text(tpl.format.rawValue.uppercased())
                                        .font(.system(size: 10, weight: .bold))
                                        .padding(.horizontal, 6)
                                        .padding(.vertical, 2)
                                        .background(EasyOfficeDesignTokens.softBadgeToken(for: tpl.moduleKind).swiftUIColor)
                                        .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: tpl.moduleKind).swiftUIColor)
                                        .clipShape(RoundedRectangle(cornerRadius: 4))
                                    if tpl.isCustomUserTemplate {
                                        Text("사용자 템플릿")
                                            .font(.system(size: 9, weight: .bold))
                                            .foregroundStyle(EasyOfficeDesignTokens.statusInfo.swiftUIColor)
                                    }
                                    Spacer()
                                }
                                Text(tpl.title)
                                    .font(.system(size: 13, weight: .semibold))
                                    .foregroundStyle(palette.primaryText.swiftUIColor)
                                Text(tpl.subtitle)
                                    .font(.system(size: 11))
                                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                                    .lineLimit(2)
                            }
                            .padding(12)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(palette.cardSurface.swiftUIColor)
                            .overlay(
                                RoundedRectangle(cornerRadius: 8)
                                    .stroke(palette.border.swiftUIColor, lineWidth: 1)
                            )
                            .clipShape(RoundedRectangle(cornerRadius: 8))
                            .onTapGesture {
                                viewModel.openNewSampleTab(for: tpl.moduleKind)
                            }
                        }
                    }

                    Text("\(viewModel.activeHubSection.rawValue) · 문서 목록 (\(viewModel.filteredTabs.count)건)")
                        .font(.system(size: 15, weight: .bold))
                        .foregroundStyle(palette.primaryText.swiftUIColor)

                    VStack(spacing: 8) {
                        ForEach(viewModel.filteredTabs) { tab in
                            HStack {
                                Text(tab.moduleKind.shortBadgeTitle)
                                    .font(.system(size: 10, weight: .bold))
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 3)
                                    .background(EasyOfficeDesignTokens.softBadgeToken(for: tab.moduleKind).swiftUIColor)
                                    .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: tab.moduleKind).swiftUIColor)
                                    .clipShape(RoundedRectangle(cornerRadius: 4))

                                Text(tab.fileName)
                                    .font(.system(size: 13, weight: .medium))
                                    .foregroundStyle(palette.primaryText.swiftUIColor)
                                Spacer()
                                Text(tab.format.standardSpecification)
                                    .font(.system(size: 11))
                                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                            }
                            .padding(12)
                            .background(palette.cardSurface.swiftUIColor)
                            .clipShape(RoundedRectangle(cornerRadius: 8))
                            .onTapGesture {
                                viewModel.activeTabID = tab.id
                                viewModel.isShowingHomeHub = false
                            }
                        }
                    }
                }
                .padding(24)
            }
        }
    }
}

struct LeftNavigatorPanelView: View {
    @Bindable var viewModel: WorkspaceViewModel
    let tab: WorkspaceTabItem

    var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: false)

        VStack(alignment: .leading, spacing: 10) {
            Text("\(tab.moduleKind.shortBadgeTitle) 구조 내비게이터")
                .font(.system(size: 11, weight: .bold))
                .foregroundStyle(palette.secondaryText.swiftUIColor)

            switch tab.payload {
            case let .document(doc):
                ForEach(doc.extractOutline()) { heading in
                    Text("• \(heading.text)")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(palette.primaryText.swiftUIColor)
                        .lineLimit(2)
                }
            case let .sheet(sheet):
                Text("시트: \(sheet.name)")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(palette.primaryText.swiftUIColor)
                Text("내장 함수 카탈로그: \(WorksheetModel.supportedFunctionNames.count)종")
                    .font(.system(size: 11))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                Text("활성 수식 셀: D2, D3, B4, C4, D4")
                    .font(.system(size: 11))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
            case let .slide(deck):
                ForEach(Array(deck.slides.enumerated()), id: \.element.id) { idx, slide in
                    Button {
                        viewModel.selectedSlideIndex = idx
                    } label: {
                        Text("\(idx + 1). \(slide.title)")
                            .font(.system(size: 12, weight: viewModel.selectedSlideIndex == idx ? .bold : .medium))
                            .foregroundStyle(palette.primaryText.swiftUIColor)
                    }
                    .buttonStyle(.plain)
                }
            case let .pdf(pdf):
                ForEach(Array(pdf.pages.enumerated()), id: \.element.id) { idx, page in
                    Button {
                        viewModel.selectedPDFPageIndex = idx
                    } label: {
                        Text("페이지 \(page.pageNumber) (\(page.rotationDegrees)°)")
                            .font(.system(size: 12, weight: viewModel.selectedPDFPageIndex == idx ? .bold : .medium))
                            .foregroundStyle(palette.primaryText.swiftUIColor)
                    }
                    .buttonStyle(.plain)
                }
                if !pdf.outline.isEmpty {
                    Divider()
                    Text("PDF 책갈피 트리")
                        .font(.system(size: 10, weight: .bold))
                        .foregroundStyle(palette.secondaryText.swiftUIColor)
                    ForEach(pdf.outline) { node in
                        Text("• \(node.title) (p.\(node.targetPageNumber))")
                            .font(.system(size: 11))
                            .foregroundStyle(palette.secondaryText.swiftUIColor)
                    }
                }
            }
            Spacer()
        }
        .padding(12)
        .background(palette.cardSurface.swiftUIColor)
    }
}

struct CentralEditorCanvasView: View {
    @Bindable var viewModel: WorkspaceViewModel
    let tab: WorkspaceTabItem
    var isPrimaryPane: Bool = true

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                switch tab.payload {
                case let .document(doc):
                    DocumentEditorDetailPaneView(viewModel: viewModel, document: doc, isPrimaryPane: isPrimaryPane)
                case let .sheet(sheet):
                    SheetEditorDetailPaneView(viewModel: viewModel, sheet: sheet, isPrimaryPane: isPrimaryPane)
                case let .slide(deck):
                    SlideEditorDetailPaneView(viewModel: viewModel, deck: deck)
                case let .pdf(pdf):
                    PDFStudioEditorDetailPaneView(viewModel: viewModel, pdf: pdf, isPrimaryPane: isPrimaryPane)
                }
            }
            .padding(24)
        }
    }
}

struct RightContextualInspectorView: View {
    @Bindable var viewModel: WorkspaceViewModel
    let tab: WorkspaceTabItem

    var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: false)

        VStack(alignment: .leading, spacing: 12) {
            Text("속성 인스펙터")
                .font(.system(size: 12, weight: .bold))
                .foregroundStyle(palette.primaryText.swiftUIColor)

            Text("파일명: \(tab.fileName)")
                .font(.system(size: 11))
                .foregroundStyle(palette.primaryText.swiftUIColor)
            Text("규격: \(tab.format.standardSpecification)")
                .font(.system(size: 11))
                .foregroundStyle(palette.secondaryText.swiftUIColor)

            Divider()

            switch tab.payload {
            case let .document(doc):
                let stats = doc.computeStatistics()
                Text("문서 통계 및 원고지 검산")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(palette.primaryText.swiftUIColor)
                Text("문단 수: \(stats.paragraphCount) · 표: \(stats.tableCount)")
                    .font(.system(size: 11))
                Text("단어 수: \(stats.wordCount)")
                    .font(.system(size: 11))
                Text("글자 수(공백 포함): \(stats.characterCountWithSpaces)")
                    .font(.system(size: 11))
                Text("200자 원고지: \(stats.manuscriptPages200)매")
                    .font(.system(size: 11))
                Text("각주: \(doc.footnotes.count)건 · 변경 추적: \(doc.revisions.count)건")
                    .font(.system(size: 11))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)

            case let .sheet(sheet):
                Text("시트 수식 및 검증 속성")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(palette.primaryText.swiftUIColor)
                Text("선택된 셀: \(viewModel.selectedSheetCell)")
                    .font(.system(size: 11, design: .monospaced))
                Text("원시 수식: \(sheet.rawValue(at: viewModel.selectedSheetCell))")
                    .font(.system(size: 11, design: .monospaced))
                Text("평가 결과: \(sheet.evaluate(at: viewModel.selectedSheetCell).description)")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)

            case let .slide(deck):
                Text("슬라이드 마스터 및 레이아웃")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(palette.primaryText.swiftUIColor)
                Text("화면 비율: \(deck.aspectRatio.rawValue)")
                    .font(.system(size: 11))
                Text("총 슬라이드: \(deck.slides.count)장")
                    .font(.system(size: 11))

            case let .pdf(pdf):
                Text("PDF 스튜디오 속성")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(palette.primaryText.swiftUIColor)
                Text("총 페이지: \(pdf.pages.count)p")
                    .font(.system(size: 11))
                Text("주석 항목: \(pdf.annotations.count)건")
                    .font(.system(size: 11))
                Text("책갈피 노드: \(pdf.outline.count)건")
                    .font(.system(size: 11))
            }

            Spacer()
        }
        .padding(14)
        .background(palette.cardSurface.swiftUIColor)
    }
}

struct BottomStatusBarView: View {
    @Bindable var viewModel: WorkspaceViewModel
    let tab: WorkspaceTabItem

    var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: false)

        HStack(spacing: 12) {
            Text("\(tab.moduleKind.displayNameKo) · \(tab.format.standardSpecification)")
                .font(.system(size: 11))
                .foregroundStyle(palette.secondaryText.swiftUIColor)

            if let secondary = viewModel.splitSecondaryTab {
                Text("스플릿 뷰: \(secondary.fileName)")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
            }

            Spacer()

            Button("-") {
                viewModel.setZoom(viewModel.zoomPercent - 25.0)
            }
            .buttonStyle(.plain)

            Text("배율: \(Int(viewModel.zoomPercent))%")
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(palette.secondaryText.swiftUIColor)

            Button("+") {
                viewModel.setZoom(viewModel.zoomPercent + 25.0)
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 14)
        .frame(height: EasyOfficeDesignTokens.LayoutMetrics.statusBarHeight)
        .background(palette.cardSurface.swiftUIColor)
    }
}
