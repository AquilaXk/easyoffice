import SwiftUI
import Observation
import EasyOfficeDesignSystem
import EasyOfficeCore

@MainActor
@Observable
public final class WorkspaceViewModel {
    public var openTabs: [WorkspaceTabItem]
    public var activeTabID: UUID?
    public var isShowingHomeHub: Bool = false
    public var activeHubSection: HubSection = .home
    public var activeRibbonCategory: RibbonCategory = .home
    public var showLeftNavigator: Bool = true
    public var showRightInspector: Bool = true
    public var searchQuery: String = ""
    public var zoomPercent: Double = 100.0
    public var selectedSheetCell: String = "D2"

    public init() {
        let initialTabs = OfficeModuleKind.allCases.map {
            WorkspaceHubCatalog.instantiateSampleTab(for: $0)
        }
        self.openTabs = initialTabs
        self.activeTabID = initialTabs.first?.id
    }

    public var activeTab: WorkspaceTabItem? {
        guard let activeTabID else { return openTabs.first }
        return openTabs.first(where: { $0.id == activeTabID })
    }

    public var filteredTabs: [WorkspaceTabItem] {
        WorkspaceHubCatalog.searchTabs(openTabs, query: searchQuery)
    }

    public func openNewSampleTab(for module: OfficeModuleKind) {
        let newTab = WorkspaceHubCatalog.instantiateSampleTab(for: module)
        openTabs.append(newTab)
        activeTabID = newTab.id
        isShowingHomeHub = false
    }

    public func closeTab(id: UUID) {
        openTabs.removeAll { $0.id == id }
        if activeTabID == id {
            activeTabID = openTabs.first?.id
        }
        if openTabs.isEmpty {
            isShowingHomeHub = true
        }
    }
}

public struct MainWorkspaceWindowView: View {
    @Bindable var viewModel: WorkspaceViewModel

    public init(viewModel: WorkspaceViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
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
                        LeftNavigatorPanelView(tab: tab)
                            .frame(width: EasyOfficeDesignTokens.LayoutMetrics.leftNavigatorWidth)
                        Divider()
                    }

                    CentralEditorCanvasView(viewModel: viewModel, tab: tab)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)

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
        .background(EasyOfficeDesignTokens.lightScaffold.swiftUIColor)
    }
}

struct TopDocumentTabStripView: View {
    @Bindable var viewModel: WorkspaceViewModel

    var body: some View {
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
                        let accent = EasyOfficeDesignTokens.accentToken(for: tab.moduleKind)
                        HStack(spacing: 6) {
                            Text(tab.moduleKind.shortBadgeTitle)
                                .font(.system(size: 9, weight: .bold))
                                .padding(.horizontal, 5)
                                .padding(.vertical, 2)
                                .background(accent.swiftUIColor.opacity(0.16))
                                .foregroundStyle(accent.swiftUIColor)
                                .clipShape(RoundedRectangle(cornerRadius: 4))

                            Text(tab.fileName)
                                .font(.system(size: 12, weight: isSelected ? .semibold : .regular))
                                .foregroundStyle(EasyOfficeDesignTokens.inkPrimary.swiftUIColor)

                            Button {
                                viewModel.closeTab(id: tab.id)
                            } label: {
                                Image(systemName: "xmark")
                                    .font(.system(size: 9, weight: .bold))
                                    .foregroundStyle(EasyOfficeDesignTokens.inkMuted.swiftUIColor)
                            }
                            .buttonStyle(.plain)
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(
                            isSelected
                                ? EasyOfficeDesignTokens.lightWhite.swiftUIColor
                                : EasyOfficeDesignTokens.lightSubtle.swiftUIColor
                        )
                        .overlay(
                            RoundedRectangle(cornerRadius: 6)
                                .stroke(
                                    isSelected
                                        ? EasyOfficeDesignTokens.brand700.swiftUIColor
                                        : EasyOfficeDesignTokens.lightBorder.swiftUIColor,
                                    lineWidth: isSelected ? 1.5 : 1.0
                                )
                        )
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                        .onTapGesture {
                            viewModel.activeTabID = tab.id
                            viewModel.isShowingHomeHub = false
                        }
                    }
                }
            }

            Spacer()

            TextField("문서·본문·수식 통합 검색...", text: $viewModel.searchQuery)
                .textFieldStyle(.roundedBorder)
                .frame(width: 220)
        }
        .padding(.horizontal, 12)
        .frame(height: EasyOfficeDesignTokens.LayoutMetrics.tabStripHeight + 6)
        .background(EasyOfficeDesignTokens.lightSubtle.swiftUIColor)
    }
}

struct ContextRibbonBarView: View {
    @Bindable var viewModel: WorkspaceViewModel
    let activeModule: OfficeModuleKind

    var body: some View {
        VStack(spacing: 6) {
            HStack(spacing: 14) {
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
                                    : EasyOfficeDesignTokens.inkSecondary.swiftUIColor
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
            }
            .padding(.horizontal, 14)
            .padding(.top, 6)
            .padding(.bottom, 6)
        }
        .background(EasyOfficeDesignTokens.lightWhite.swiftUIColor)
    }
}

struct HomeHubWorkspaceView: View {
    @Bindable var viewModel: WorkspaceViewModel

    var body: some View {
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
                                : EasyOfficeDesignTokens.inkPrimary.swiftUIColor
                        )
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                    }
                    .buttonStyle(.plain)
                }
                Spacer()
            }
            .padding(16)
            .frame(width: 220)
            .background(EasyOfficeDesignTokens.lightWhite.swiftUIColor)

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    Text("새 문서 시작하기")
                        .font(.system(size: 16, weight: .bold))
                        .foregroundStyle(EasyOfficeDesignTokens.inkPrimary.swiftUIColor)

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
                                        .foregroundStyle(EasyOfficeDesignTokens.inkPrimary.swiftUIColor)
                                }
                                .padding(14)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(EasyOfficeDesignTokens.lightWhite.swiftUIColor)
                                .overlay(
                                    RoundedRectangle(cornerRadius: 10)
                                        .stroke(EasyOfficeDesignTokens.lightBorder.swiftUIColor, lineWidth: 1)
                                )
                                .clipShape(RoundedRectangle(cornerRadius: 10))
                            }
                            .buttonStyle(.plain)
                        }
                    }

                    Text("열려 있는 문서 및 검색 결과 (\(viewModel.filteredTabs.count)건)")
                        .font(.system(size: 15, weight: .bold))
                        .foregroundStyle(EasyOfficeDesignTokens.inkPrimary.swiftUIColor)

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
                                Spacer()
                                Text(tab.format.standardSpecification)
                                    .font(.system(size: 11))
                                    .foregroundStyle(EasyOfficeDesignTokens.inkMuted.swiftUIColor)
                            }
                            .padding(12)
                            .background(EasyOfficeDesignTokens.lightWhite.swiftUIColor)
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
    let tab: WorkspaceTabItem

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("\(tab.moduleKind.shortBadgeTitle) 구조 내비게이터")
                .font(.system(size: 11, weight: .bold))
                .foregroundStyle(EasyOfficeDesignTokens.inkMuted.swiftUIColor)

            switch tab.payload {
            case let .document(doc):
                ForEach(doc.extractOutline()) { heading in
                    Text("• \(heading.text)")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(EasyOfficeDesignTokens.inkPrimary.swiftUIColor)
                        .lineLimit(2)
                }
            case let .sheet(sheet):
                Text("시트: \(sheet.name)")
                    .font(.system(size: 12, weight: .semibold))
                Text("활성 수식 셀: D2, D3, B4, C4, D4")
                    .font(.system(size: 11))
                    .foregroundStyle(EasyOfficeDesignTokens.inkSecondary.swiftUIColor)
            case let .slide(deck):
                ForEach(Array(deck.slides.enumerated()), id: \.element.id) { idx, slide in
                    Text("\(idx + 1). \(slide.title)")
                        .font(.system(size: 12, weight: .medium))
                }
            case let .pdf(pdf):
                ForEach(pdf.pages) { page in
                    Text("페이지 \(page.pageNumber) (\(page.rotationDegrees)°)")
                        .font(.system(size: 12, weight: .medium))
                }
            }
            Spacer()
        }
        .padding(12)
        .background(EasyOfficeDesignTokens.lightWhite.swiftUIColor)
    }
}

struct CentralEditorCanvasView: View {
    @Bindable var viewModel: WorkspaceViewModel
    let tab: WorkspaceTabItem

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                switch tab.payload {
                case let .document(doc):
                    VStack(alignment: .leading, spacing: 14) {
                        Text(doc.headerText)
                            .font(.system(size: 11))
                            .foregroundStyle(EasyOfficeDesignTokens.inkMuted.swiftUIColor)
                        Divider()
                        ForEach(doc.paragraphs) { p in
                            Text(p.text)
                                .font(.system(size: p.fontSizePt, weight: p.isBold ? .bold : .regular))
                                .foregroundStyle(EasyOfficeDesignTokens.inkPrimary.swiftUIColor)
                        }
                        ForEach(doc.tables) { table in
                            VStack(alignment: .leading, spacing: 4) {
                                Text(table.caption)
                                    .font(.system(size: 11, weight: .semibold))
                                ForEach(Array(table.rows.enumerated()), id: \.offset) { _, row in
                                    HStack {
                                        ForEach(row, id: \.self) { cell in
                                            Text(cell)
                                                .font(.system(size: 12))
                                                .frame(maxWidth: .infinity, alignment: .leading)
                                                .padding(6)
                                                .background(EasyOfficeDesignTokens.lightSubtle.swiftUIColor)
                                        }
                                    }
                                }
                            }
                        }
                    }
                    .padding(32)
                    .background(EasyOfficeDesignTokens.lightWhite.swiftUIColor)
                    .clipShape(RoundedRectangle(cornerRadius: 8))

                case let .sheet(sheet):
                    VStack(alignment: .leading, spacing: 10) {
                        HStack {
                            Text(viewModel.selectedSheetCell)
                                .font(.system(size: 12, weight: .bold, design: .monospaced))
                                .padding(.horizontal, 8)
                                .padding(.vertical, 4)
                                .background(EasyOfficeDesignTokens.brand100.swiftUIColor)
                                .clipShape(RoundedRectangle(cornerRadius: 4))
                            Text("fx = \(sheet.rawValue(at: viewModel.selectedSheetCell))")
                                .font(.system(size: 12, design: .monospaced))
                            Spacer()
                            Text("결과: \(sheet.evaluate(at: viewModel.selectedSheetCell).description)")
                                .font(.system(size: 12, weight: .bold))
                                .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                        }
                        .padding(10)
                        .background(EasyOfficeDesignTokens.lightWhite.swiftUIColor)
                        .clipShape(RoundedRectangle(cornerRadius: 6))

                        ForEach(1...4, id: \.self) { row in
                            HStack(spacing: 4) {
                                ForEach(["A", "B", "C", "D"], id: \.self) { col in
                                    let addr = "\(col)\(row)"
                                    Button {
                                        viewModel.selectedSheetCell = addr
                                    } label: {
                                        VStack(alignment: .leading, spacing: 2) {
                                            Text(addr)
                                                .font(.system(size: 9, design: .monospaced))
                                                .foregroundStyle(EasyOfficeDesignTokens.inkMuted.swiftUIColor)
                                            Text(sheet.evaluate(at: addr).description)
                                                .font(.system(size: 12, weight: row == 1 || row == 4 ? .bold : .regular))
                                                .foregroundStyle(EasyOfficeDesignTokens.inkPrimary.swiftUIColor)
                                        }
                                        .padding(8)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                        .background(EasyOfficeDesignTokens.lightWhite.swiftUIColor)
                                    }
                                    .buttonStyle(.plain)
                                }
                            }
                        }
                    }

                case let .slide(deck):
                    ForEach(deck.slides) { slide in
                        VStack(alignment: .leading, spacing: 10) {
                            Text(slide.title)
                                .font(.system(size: 20, weight: .bold))
                            Text(slide.subtitle)
                                .font(.system(size: 14))
                                .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                            ForEach(slide.bulletPoints, id: \.self) { bullet in
                                Text("• \(bullet)")
                                    .font(.system(size: 13))
                            }
                            Divider()
                            Text("발표자 노트: \(slide.speakerNotes)")
                                .font(.system(size: 11))
                                .foregroundStyle(EasyOfficeDesignTokens.inkMuted.swiftUIColor)
                        }
                        .padding(24)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(EasyOfficeDesignTokens.lightWhite.swiftUIColor)
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                    }

                case let .pdf(pdf):
                    ForEach(pdf.pages) { page in
                        VStack(alignment: .leading, spacing: 8) {
                            Text("PDF Page \(page.pageNumber)")
                                .font(.system(size: 11, weight: .bold))
                                .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
                            Text(page.extractedText)
                                .font(.system(size: 13))
                        }
                        .padding(24)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(EasyOfficeDesignTokens.lightWhite.swiftUIColor)
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                    }
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
        VStack(alignment: .leading, spacing: 12) {
            Text("속성 인스펙터")
                .font(.system(size: 12, weight: .bold))
                .foregroundStyle(EasyOfficeDesignTokens.inkPrimary.swiftUIColor)

            Text("파일명: \(tab.fileName)")
                .font(.system(size: 11))
            Text("규격: \(tab.format.standardSpecification)")
                .font(.system(size: 11))
                .foregroundStyle(EasyOfficeDesignTokens.inkSecondary.swiftUIColor)

            Divider()

            if case let .document(doc) = tab.payload {
                let stats = doc.computeStatistics()
                Text("문서 통계")
                    .font(.system(size: 11, weight: .bold))
                Text("단어 수: \(stats.wordCount)")
                    .font(.system(size: 11))
                Text("글자 수(공백 포함): \(stats.characterCountWithSpaces)")
                    .font(.system(size: 11))
                Text("200자 원고지: \(stats.manuscriptPages200)매")
                    .font(.system(size: 11))
            }
            Spacer()
        }
        .padding(14)
        .background(EasyOfficeDesignTokens.lightWhite.swiftUIColor)
    }
}

struct BottomStatusBarView: View {
    @Bindable var viewModel: WorkspaceViewModel
    let tab: WorkspaceTabItem

    var body: some View {
        HStack {
            Text("\(tab.moduleKind.displayNameKo) · \(tab.format.standardSpecification)")
                .font(.system(size: 11))
                .foregroundStyle(EasyOfficeDesignTokens.inkSecondary.swiftUIColor)
            Spacer()
            Text("배율: \(Int(viewModel.zoomPercent))%")
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(EasyOfficeDesignTokens.inkSecondary.swiftUIColor)
        }
        .padding(.horizontal, 14)
        .frame(height: EasyOfficeDesignTokens.LayoutMetrics.statusBarHeight)
        .background(EasyOfficeDesignTokens.lightWhite.swiftUIColor)
    }
}
