import AppKit
import EasyOfficeCore
import EasyOfficeDesignSystem
import SwiftUI

public struct DocumentHorizontalRulerView: View {
  public let document: OfficeDocumentModel
  public let selectedParagraph: ParagraphBlock?
  public let palette: WorkspaceThemePalette
  public var paperMaxWidth: CGFloat
  public var viewModel: WorkspaceViewModel?

  public init(
    document: OfficeDocumentModel,
    selectedParagraph: ParagraphBlock?,
    palette: WorkspaceThemePalette,
    paperMaxWidth: CGFloat = 820.0,
    viewModel: WorkspaceViewModel? = nil
  ) {
    self.document = document
    self.selectedParagraph = selectedParagraph
    self.palette = palette
    self.paperMaxWidth = paperMaxWidth
    self.viewModel = viewModel
  }

  public var body: some View {
    let customPadding = viewModel?.documentHorizontalPadding ?? 28.0
    let leftMarginPt: CGFloat = max(16.0, min(96.0, CGFloat(customPadding)))
    let rightMarginPt: CGFloat = leftMarginPt
    let firstLineOffsetPt = CGFloat(selectedParagraph?.firstLineIndentPt ?? 0.0)
    let leftIndentOffsetPt = CGFloat(selectedParagraph?.leftIndentPt ?? 0.0)
    let rulerBg = EasyOfficeDesignTokens.resolveHexColor(
      palette.isDark ? "#25292F" : "#FFFFFF",
      fallback: palette.cardSurface.swiftUIColor
    )
    let marginShade = EasyOfficeDesignTokens.resolveHexColor(
      palette.isDark ? "#1F2329" : "#D4D4D4",
      fallback: palette.elevatedSurface.swiftUIColor
    )

    GeometryReader { geo in
      let totalWidth = max(240.0, geo.size.width)
      let clampedPaperWidth = min(totalWidth, max(320.0, paperMaxWidth))
      let paperOffset = max(0.0, (totalWidth - clampedPaperWidth) * 0.5)
      let effectiveLeft = paperOffset + leftMarginPt
      let effectiveRight = paperOffset + rightMarginPt
      let bodyWidth = max(80.0, totalWidth - effectiveLeft - effectiveRight)
      let cmStep = bodyWidth / 40.0

      ZStack(alignment: .topLeading) {
        Rectangle()
          .fill(rulerBg)

        Rectangle()
          .fill(marginShade)
          .frame(width: effectiveLeft, height: 20)

        Rectangle()
          .fill(marginShade)
          .frame(width: effectiveRight, height: 20)
          .offset(x: totalWidth - effectiveRight)

        Canvas { context, size in
          let tickColor = palette.secondaryText.swiftUIColor
          for unitIndex in 0...80 {
            let xPos = effectiveLeft + (CGFloat(unitIndex) * 0.5) * cmStep
            guard xPos <= totalWidth - effectiveRight + 1.0 else { continue }
            let isMajor = (unitIndex % 2 == 0)
            let tickHeight: CGFloat = isMajor ? 7.0 : 3.5
            let tickWidth: CGFloat = isMajor ? 0.9 : 0.6
            let rect = CGRect(
              x: xPos - tickWidth * 0.5,
              y: size.height - tickHeight,
              width: tickWidth,
              height: tickHeight
            )
            context.fill(
              Path(rect),
              with: .color(tickColor.opacity(isMajor ? 0.8 : 0.45))
            )
          }
        }

        ForEach(1...40, id: \.self) { unit in
          if unit % 2 == 1 || unit == 40 {
            let xPos = effectiveLeft + CGFloat(unit) * cmStep
            if xPos <= totalWidth - effectiveRight + 4.0 {
              Text("\(unit)")
                .font(.system(size: 7.0, weight: .semibold, design: .monospaced))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
                .position(x: xPos, y: 6)
            }
          }
        }

        Text("▽")
          .font(.system(size: 8.5, weight: .bold))
          .foregroundStyle(EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor)
          .position(
            x: min(
              totalWidth - effectiveRight, effectiveLeft + leftIndentOffsetPt + firstLineOffsetPt),
            y: 4.5
          )
          .gesture(
            DragGesture(minimumDistance: 2)
              .onChanged { drag in
                let newFirst = max(
                  0.0,
                  min(144.0, Double(drag.location.x - effectiveLeft - leftIndentOffsetPt))
                )
                viewModel?.setActiveParagraphIndent(firstLineIndentPt: newFirst)
              }
          )
          .onTapGesture {
            let currentFirst = Double(firstLineOffsetPt)
            let nextFirst = currentFirst >= 24.0 ? 0.0 : (currentFirst + 12.0)
            viewModel?.setActiveParagraphIndent(firstLineIndentPt: nextFirst)
          }
          .help("첫 줄 들여쓰기 마커 (\(Int(firstLineOffsetPt))pt)")

        Text("△")
          .font(.system(size: 8.5, weight: .bold))
          .foregroundStyle(EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor)
          .position(
            x: min(totalWidth - effectiveRight, effectiveLeft + leftIndentOffsetPt),
            y: 15.5
          )
          .gesture(
            DragGesture(minimumDistance: 2)
              .onChanged { drag in
                let newLeft = max(0.0, min(144.0, Double(drag.location.x - effectiveLeft)))
                viewModel?.setActiveParagraphIndent(leftIndentPt: newLeft)
              }
          )
          .onTapGesture {
            let currentLeft = Double(leftIndentOffsetPt)
            let nextLeft = currentLeft >= 36.0 ? 0.0 : (currentLeft + 18.0)
            viewModel?.setActiveParagraphIndent(leftIndentPt: nextLeft)
          }
          .help("좌측 단락 들여쓰기 마커 (\(Int(leftIndentOffsetPt))pt)")

        Text("▽")
          .font(.system(size: 8.5, weight: .bold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
          .position(x: totalWidth - effectiveRight, y: 15.5)
          .gesture(
            DragGesture(minimumDistance: 2)
              .onChanged { drag in
                let distFromRight = max(
                  24.0, min(96.0, Double(totalWidth - paperOffset - drag.location.x)))
                viewModel?.setDocumentHorizontalPadding(distFromRight)
              }
          )
          .help("오른쪽 여백 경계 (\(Int(rightMarginPt))pt)")
      }
      .border(palette.border.swiftUIColor, width: 1)
    }
    .frame(height: 20)
  }
}

public struct DocumentVerticalRulerView: View {
  public let palette: WorkspaceThemePalette

  public init(palette: WorkspaceThemePalette) {
    self.palette = palette
  }

  public var body: some View {
    let rulerBg = EasyOfficeDesignTokens.resolveHexColor(
      palette.isDark ? "#25292F" : "#FFFFFF",
      fallback: palette.cardSurface.swiftUIColor
    )
    let marginShade = EasyOfficeDesignTokens.resolveHexColor(
      palette.isDark ? "#1F2329" : "#D4D4D4",
      fallback: palette.elevatedSurface.swiftUIColor
    )

    VStack(spacing: 0) {
      ZStack(alignment: .bottomTrailing) {
        Rectangle()
          .fill(marginShade)
          .frame(width: 20, height: 34)

        Text("0")
          .font(.system(size: 7.5, weight: .semibold, design: .monospaced))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
          .frame(width: 14, alignment: .center)
          .offset(x: -4, y: -2)

        Rectangle()
          .fill(palette.hairlineBorder)
          .frame(height: 0.8)
      }

      ForEach(1...36, id: \.self) { cm in
        ZStack(alignment: .topTrailing) {
          Rectangle()
            .fill(palette.secondaryText.swiftUIColor.opacity(0.75))
            .frame(width: 5.5, height: 0.9)

          Text("\(cm)")
            .font(.system(size: 7.5, weight: .semibold, design: .monospaced))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
            .frame(width: 14, alignment: .center)
            .offset(x: -4, y: 1)

          Rectangle()
            .fill(palette.hairlineBorder)
            .frame(width: 3.0, height: 0.7)
            .offset(y: 10)
        }
        .frame(width: 20, height: 20)
      }

      Spacer(minLength: 0)
    }
    .frame(width: 20)
    .frame(maxHeight: .infinity, alignment: .top)
    .background(rulerBg)
    .border(palette.hairlineBorder, width: 1)
  }
}

public struct HWPManuscriptGridPaperView: View {
  public let document: OfficeDocumentModel
  public let palette: WorkspaceThemePalette
  public var viewModel: WorkspaceViewModel?

  public init(
    document: OfficeDocumentModel,
    palette: WorkspaceThemePalette,
    viewModel: WorkspaceViewModel? = nil
  ) {
    self.document = document
    self.palette = palette
    self.viewModel = viewModel
  }

  private var manuscriptPages: [[String]] {
    let rawText = document.paragraphs.map(\.text).joined(separator: "\n")
    var cells: [String] = []
    for ch in rawText {
      if ch == "\n" {
        let remainder = cells.count % 20
        if remainder != 0 {
          let padCount = 20 - remainder
          for _ in 0..<padCount {
            cells.append(" ")
          }
        }
      } else {
        cells.append(String(ch))
      }
    }
    if cells.isEmpty {
      cells = Array(repeating: " ", count: 200)
    }
    var pages: [[String]] = []
    var offset = 0
    while offset < cells.count {
      let end = min(cells.count, offset + 200)
      var slice = Array(cells[offset..<end])
      if slice.count < 200 {
        slice.append(contentsOf: Array(repeating: " ", count: 200 - slice.count))
      }
      pages.append(slice)
      offset += 200
    }
    return pages
  }

  public var body: some View {
    let pages = manuscriptPages
    let gridStrokeColor = EasyOfficeDesignTokens.resolveHexColor(
      palette.isDark ? "#D97706" : "#C84B31",
      fallback: palette.border.swiftUIColor
    )
    let fullText = document.paragraphs.map(\.text).joined(separator: "\n")

    VStack(alignment: .leading, spacing: 14) {
      if let vm = viewModel, !vm.isCanvasSnapshotMode {
        VStack(alignment: .leading, spacing: 4) {
          Text("원고지 본문 직접 편집 (입력 즉시 200자 원고지 격자에 반영)")
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
          TextEditor(
            text: Binding(
              get: { fullText },
              set: { vm.updateActiveDocumentManuscriptFullText($0) }
            )
          )
          .font(.system(size: 12, design: .monospaced))
          .frame(minHeight: 68, maxHeight: 110)
          .padding(6)
          .background(palette.cardSurface.swiftUIColor)
          .border(gridStrokeColor.opacity(0.6), width: 1)
        }
      }

      ForEach(Array(pages.enumerated()), id: \.offset) { pageIdx, pageCells in
        VStack(alignment: .leading, spacing: 6) {
          HStack {
            Label(
              "원고지 \(pageIdx + 1)매 / 총 \(pages.count)매 (20×10 = 200자)",
              systemImage: "squareshape.split.3x3"
            )
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(gridStrokeColor)

            Spacer()

            Text(document.title)
              .font(.system(size: 10, weight: .medium))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
          }

          VStack(spacing: 3) {
            ForEach(0..<10, id: \.self) { rowIdx in
              HStack(spacing: 0) {
                ForEach(0..<20, id: \.self) { colIdx in
                  let cellIdx = rowIdx * 20 + colIdx
                  let ch = pageCells.indices.contains(cellIdx) ? pageCells[cellIdx] : " "
                  Text(ch)
                    .font(.system(size: 12, weight: .medium, design: .monospaced))
                    .foregroundStyle(palette.primaryText.swiftUIColor)
                    .frame(maxWidth: .infinity, minHeight: 24, maxHeight: 24)
                    .background(palette.cardSurface.swiftUIColor)
                    .border(gridStrokeColor.opacity(0.65), width: 0.7)
                }
              }
              .border(gridStrokeColor, width: 1.0)
            }
          }
          .padding(8)
          .background(palette.elevatedSurface.swiftUIColor)
          .border(gridStrokeColor, width: 1.6)
        }
      }
    }
  }
}

struct StudioProcessFlowDiagramView: View {
  var body: some View {
    let boxFill = EasyOfficeDesignTokens.resolveHexColor("#F5E6D3", fallback: Color.orange.opacity(0.15))
    let borderColor = EasyOfficeDesignTokens.resolveHexColor("#B8A082", fallback: Color.gray)
    let textColor = EasyOfficeDesignTokens.resolveHexColor("#111827", fallback: .black)
    let steps: [(num: String, title: String, sub: String?)] = [
      ("①", "적합성검토", nil),
      ("②", "선정평가(평가위원회)", "서류평가 , 발표평가"),
      ("③", "사업비 심의·조정", "(사업비검토위원회)"),
      ("④", "중복 심의", "(중복검토위원회)"),
    ]
    let criteriaItems = [
      "○ 공급기업 POOL(붙임5) 미등록 공급기업과 매칭하여 과제 신청하는 경우",
      "○ 지원사업 참여제한중인 수요 및 공급기업이 신청하는 경우",
      "○ 과거 지원받은 수요기업이 동일 과제로 재신청 하는 경우",
      "○ 수요기업과 공급기업의 대표자가 동일인이거나 특수관계인 경우",
    ]

    VStack(alignment: .leading, spacing: 8) {
      Text("< 선정 절차 >")
        .font(.system(size: 11, weight: .bold))
        .foregroundStyle(textColor)
        .frame(maxWidth: .infinity, alignment: .center)

      HStack(spacing: 5) {
        ForEach(Array(steps.enumerated()), id: \.offset) { idx, item in
          ZStack {
            boxFill
            VStack(spacing: 2) {
              Text("\(item.num) \(item.title)")
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(textColor)
                .lineLimit(1)
              if let sub = item.sub {
                Text(sub)
                  .font(.system(size: 9, weight: .semibold))
                  .foregroundStyle(textColor.opacity(0.85))
                  .lineLimit(1)
              }
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 4)
          }
          .frame(maxWidth: .infinity, minHeight: 38, maxHeight: 38)
          .border(borderColor, width: 1)

          if idx + 1 < steps.count {
            Text("▸")
              .font(.system(size: 11, weight: .bold))
              .foregroundStyle(borderColor)
          }
        }
      }

      ZStack(alignment: .top) {
        VStack(alignment: .leading, spacing: 3) {
          ForEach(criteriaItems, id: \.self) { line in
            Text(line)
              .font(.system(size: 9.5))
              .foregroundStyle(textColor)
              .lineLimit(1)
          }
        }
        .padding(.top, 14)
        .padding(.bottom, 8)
        .padding(.horizontal, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.white)
        .border(EasyOfficeDesignTokens.resolveHexColor("#A0A0A0", fallback: .gray), width: 1)
        .padding(.top, 8)

        Text("적합성 검토 탈락기준")
          .font(.system(size: 10, weight: .bold))
          .foregroundStyle(textColor)
          .padding(.horizontal, 18)
          .padding(.vertical, 2)
          .background(boxFill)
          .border(borderColor, width: 1)
      }
    }
    .padding(.vertical, 4)
  }
}

struct TopDocumentTabStripView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  @Binding var isStudioPanelsExpanded: Bool

  init(
    viewModel: WorkspaceViewModel,
    isStudioPanelsExpanded: Binding<Bool> = .constant(false)
  ) {
    self.viewModel = viewModel
    self._isStudioPanelsExpanded = isStudioPanelsExpanded
  }

  private func topMenuBarItem(
    _ title: String,
    palette: WorkspaceThemePalette,
    action: @escaping () -> Void
  ) -> some View {
    Button(action: action) {
      Text(title)
        .font(.system(size: 12.5, weight: .regular))
        .foregroundStyle(palette.primaryText.swiftUIColor)
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
    }
    .buttonStyle(.plain)
    .fixedSize(horizontal: true, vertical: false)
  }

  @ViewBuilder
  private func topStudioMenu<MenuItems: View>(
    _ title: String,
    palette: WorkspaceThemePalette,
    snapshotAction: @escaping () -> Void,
    @ViewBuilder items: () -> MenuItems
  ) -> some View {
    if viewModel.isCanvasSnapshotMode {
      topMenuBarItem(title, palette: palette, action: snapshotAction)
    } else {
      Menu {
        items()
      } label: {
        Text(title)
          .font(.system(size: 12.5, weight: .regular))
          .foregroundStyle(palette.primaryText.swiftUIColor)
          .padding(.horizontal, 8)
          .padding(.vertical, 4)
      }
      .menuStyle(.borderlessButton)
      .fixedSize(horizontal: true, vertical: false)
    }
  }

  @ViewBuilder
  private func ribbonCategoryMenuItems(_ category: RibbonCategory) -> some View {
    let mod = viewModel.activeTab?.moduleKind ?? .document
    ForEach(RibbonCategory.actions(for: category, module: mod)) { cmd in
      Button("\(cmd.title) (\(cmd.shortcutHint))") {
        viewModel.isShowingHomeHub = false
        viewModel.activeRibbonCategory = category
        viewModel.executeRibbonCommand(cmd.id)
      }
    }
  }

  var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
    let isActiveTabDocumentStudio =
      viewModel.activeTab?.moduleKind == .document || viewModel.activeTab?.moduleKind == .hwp
    let isDocumentStudio = !viewModel.isShowingHomeHub && isActiveTabDocumentStudio
    let rowBg = EasyOfficeDesignTokens.resolveHexColor(
      palette.isDark ? "#2B3037" : "#F5F5F5",
      fallback: palette.elevatedSurface.swiftUIColor
    )

    HStack(spacing: 4) {
      HStack(spacing: 0) {
        topStudioMenu("파일", palette: palette, snapshotAction: { viewModel.activeRibbonCategory = .home }) {
          Button("새 문서") { _ = viewModel.openNewBlankTab(for: viewModel.activeTab?.moduleKind ?? .document) }
          Button("열기...") { WorkspaceFileDialogPresenter.presentOpenPanel(viewModel: viewModel) }
          Button("저장") { _ = WorkspaceFileDialogPresenter.presentSavePanel(viewModel: viewModel) }
          Divider()
          Button("인쇄...") { WorkspaceFileDialogPresenter.presentPrintPanel(viewModel: viewModel) }
        }
        topStudioMenu("편집", palette: palette, snapshotAction: {
          viewModel.isShowingHomeHub = false; viewModel.activeRibbonCategory = .home
          if !isActiveTabDocumentStudio { viewModel.showLeftNavigator = true; viewModel.showRightInspector = true }
        }) {
          Button("실행 취소") { viewModel.undoActiveTab() }.disabled(!viewModel.canUndo)
          Button("다시 실행") { viewModel.redoActiveTab() }.disabled(!viewModel.canRedo)
          Divider()
          Button("찾기 및 바꾸기...") { viewModel.isFindReplaceBarVisible.toggle() }
          Divider()
          ribbonCategoryMenuItems(.home)
        }
        topStudioMenu("보기", palette: palette, snapshotAction: {
          viewModel.isShowingHomeHub = false; viewModel.activeRibbonCategory = .view
          if !isActiveTabDocumentStudio {
            let nextVisible = !(viewModel.showLeftNavigator || viewModel.showRightInspector)
            viewModel.showLeftNavigator = nextVisible; viewModel.showRightInspector = nextVisible
          }
        }) {
          ForEach(WorkspaceViewMode.allCases) { mode in
            Button(mode.rawValue) { viewModel.setWorkspaceViewMode(mode) }
          }
          Divider()
          Button(isStudioPanelsExpanded ? "보조 패널 접기" : "보조 패널 펼치기") {
            isStudioPanelsExpanded.toggle()
            if isStudioPanelsExpanded { viewModel.showLeftNavigator = true; viewModel.showRightInspector = true }
          }
          Divider()
          ribbonCategoryMenuItems(.view)
        }
        topStudioMenu("입력", palette: palette, snapshotAction: {
          viewModel.isShowingHomeHub = false; viewModel.activeRibbonCategory = .insert
          if !isActiveTabDocumentStudio { viewModel.showLeftNavigator = true; viewModel.showRightInspector = true }
        }) { ribbonCategoryMenuItems(.insert) }
        topStudioMenu("서식", palette: palette, snapshotAction: {
          viewModel.isShowingHomeHub = false; viewModel.activeRibbonCategory = .layout
          if !isActiveTabDocumentStudio { viewModel.showLeftNavigator = true; viewModel.showRightInspector = true }
        }) { ribbonCategoryMenuItems(.layout) }
        topStudioMenu("쪽", palette: palette, snapshotAction: {
          viewModel.isShowingHomeHub = false; viewModel.activeRibbonCategory = .layout
          if !isActiveTabDocumentStudio { viewModel.showLeftNavigator = true; viewModel.showRightInspector = true }
        }) { ribbonCategoryMenuItems(.moduleSpecific) }
        topStudioMenu("표", palette: palette, snapshotAction: {
          viewModel.isShowingHomeHub = false; viewModel.activeRibbonCategory = .formulasAndData
          if !isActiveTabDocumentStudio { viewModel.showLeftNavigator = true; viewModel.showRightInspector = true }
        }) { ribbonCategoryMenuItems(.formulasAndData) }
        topStudioMenu("도구", palette: palette, snapshotAction: {
          viewModel.isShowingHomeHub = false; viewModel.activeRibbonCategory = .review
          if !isActiveTabDocumentStudio { viewModel.showLeftNavigator = true; viewModel.showRightInspector = true }
        }) { ribbonCategoryMenuItems(.review) }
      }
      .fixedSize(horizontal: true, vertical: false)

      if !isDocumentStudio || isStudioPanelsExpanded {
        Divider().frame(height: 14)
        HStack(spacing: 4) {
          topMenuBarItem("열기", palette: palette) {
            if !viewModel.isCanvasSnapshotMode { WorkspaceFileDialogPresenter.presentOpenPanel(viewModel: viewModel) }
          }
          topMenuBarItem("저장", palette: palette) {
            if !viewModel.isCanvasSnapshotMode { _ = WorkspaceFileDialogPresenter.presentSavePanel(viewModel: viewModel) }
          }
          topMenuBarItem("인쇄", palette: palette) {
            if !viewModel.isCanvasSnapshotMode { WorkspaceFileDialogPresenter.presentPrintPanel(viewModel: viewModel) }
          }
          topMenuBarItem("실행 취소", palette: palette) { viewModel.undoActiveTab() }
          topMenuBarItem("다시 실행", palette: palette) { viewModel.redoActiveTab() }
          topMenuBarItem("찾기", palette: palette) { viewModel.isFindReplaceBarVisible.toggle() }
          FlatUnderlineTextField(
            placeholder: "통합 검색",
            text: $viewModel.searchQuery,
            width: 96,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          ) {
            if !viewModel.isShowingHomeHub {
              viewModel.performFindNext()
            }
          }
        }
        .fixedSize(horizontal: true, vertical: false)
      }

      Spacer(minLength: 8)

      AdaptiveScrollContainer(
        axes: .horizontal,
        showsIndicators: false,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      ) {
        HStack(spacing: 2) {
          ForEach(viewModel.openTabs) { tab in
            let isSelected = !viewModel.isShowingHomeHub && viewModel.activeTabID == tab.id
            let isSecondarySplit = viewModel.splitSecondaryTabID == tab.id
            let accent = EasyOfficeDesignTokens.accentToken(
              for: tab.moduleKind, isDark: palette.isDark)
            HStack(spacing: 5) {
              if tab.isPinned {
                Image(systemName: "pin.fill")
                  .font(.system(size: 8.5))
                  .foregroundStyle(
                    EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor)
              }
              if tab.isFavorite {
                Image(systemName: "star.fill")
                  .font(.system(size: 8.5))
                  .foregroundStyle(
                    EasyOfficeDesignTokens.statusWarningToken(isDark: palette.isDark).swiftUIColor)
              }

              Circle()
                .fill(accent.swiftUIColor)
                .frame(width: 5, height: 5)

              Text(tab.fileName)
                .font(.system(size: 10.5, weight: isSelected ? .semibold : .regular))
                .foregroundStyle(
                  isSelected
                    ? palette.primaryText.swiftUIColor
                    : palette.secondaryText.swiftUIColor
                )
                .lineLimit(1)
                .truncationMode(.middle)
                .frame(maxWidth: 148, alignment: .leading)

              if tab.isModified {
                Circle()
                  .fill(EasyOfficeDesignTokens.brand600.swiftUIColor)
                  .frame(width: 4.5, height: 4.5)
              }

              Button {
                viewModel.requestCloseTab(id: tab.id)
              } label: {
                Image(systemName: "xmark")
                  .font(.system(size: 8.5, weight: .semibold))
                  .foregroundStyle(palette.secondaryText.swiftUIColor)
              }
              .buttonStyle(.plain)
            }
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(
              isSelected ? palette.cardSurface.swiftUIColor : Color.clear
            )
            .overlay(alignment: .bottom) {
              let barFill = isSecondarySplit ? EasyOfficeDesignTokens.brand400.swiftUIColor : Color.clear
              Rectangle()
                .fill(isSelected ? EasyOfficeDesignTokens.brand700.swiftUIColor : barFill)
                .frame(height: 1.5)
            }
            .fixedSize(horizontal: true, vertical: false)
            .accessibilityLabel(
              AccessibilityTokens.tabLabel(
                fileName: tab.fileName, module: tab.moduleKind, isPinned: tab.isPinned,
                isModified: tab.isModified)
            )
            .accessibilityHint(AccessibilityTokens.tabHint)
            .onTapGesture {
              viewModel.activeTabID = tab.id
              viewModel.isShowingHomeHub = false
            }
            .contextMenu {
              Button(tab.isPinned ? "탭 고정 해제" : "탭 고정") { viewModel.togglePinTab(id: tab.id) }
              Button(tab.isFavorite ? "즐겨찾기 해제" : "즐겨찾기 추가") {
                viewModel.toggleFavoriteTab(id: tab.id)
              }
              Button("탭 복제") { viewModel.duplicateTab(id: tab.id) }
              if tab.id != viewModel.activeTabID {
                Button(viewModel.splitSecondaryTabID == tab.id ? "스플릿 뷰 닫기" : "우측 스플릿 뷰로 열기") {
                  viewModel.enableSplitView(
                    secondaryTabID: viewModel.splitSecondaryTabID == tab.id ? nil : tab.id)
                }
              }
              if !viewModel.recentlyClosedTabs.isEmpty {
                Button("최근 닫은 탭 다시 열기") {
                  _ = viewModel.reopenLastClosedTab()
                }
              }
              Divider()
              if viewModel.openTabs.count > 1 {
                Button("다른 탭 닫기") { viewModel.closeOtherTabs(keeping: tab.id) }
              }
              Button("탭 닫기", role: .destructive) { viewModel.requestCloseTab(id: tab.id) }
            }
          }

          Button {
            _ = viewModel.openNewBlankTab(for: viewModel.activeTab?.moduleKind ?? .document)
          } label: {
            Image(systemName: "plus")
              .font(.system(size: 10, weight: .semibold))
              .foregroundStyle(
                EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor
              )
              .frame(width: 22, height: 20)
          }
          .buttonStyle(.plain)
          .help("현재 모듈 새 빈 문서 탭 생성 (우클릭으로 형식 선택)")
          .contextMenu {
            Button("새 빈 문서 (.docx)") { _ = viewModel.openNewBlankTab(for: .document) }
            Button("새 빈 한글 문서 (.hwpx)") { _ = viewModel.openNewBlankTab(for: .hwp) }
            Button("새 빈 스프레드시트 (.xlsx)") { _ = viewModel.openNewBlankTab(for: .sheet) }
            Button("새 빈 프레젠테이션 (.pptx)") { _ = viewModel.openNewBlankTab(for: .slide) }
            Button("새 빈 PDF 문서 (.pdf)") { _ = viewModel.openNewBlankTab(for: .pdf) }
            Divider()
            Button("샘플 문서 (.docx)") { viewModel.openNewSampleTab(for: .document) }
            Button("샘플 한글 문서 (.hwpx)") { viewModel.openNewSampleTab(for: .hwp) }
            Button("샘플 스프레드시트 (.xlsx)") { viewModel.openNewSampleTab(for: .sheet) }
            Button("샘플 프레젠테이션 (.pptx)") { viewModel.openNewSampleTab(for: .slide) }
            Button("샘플 PDF 문서 (.pdf)") { viewModel.openNewSampleTab(for: .pdf) }
          }
        }
      }

      Divider().frame(height: 14)

      Button {
        viewModel.isShowingHomeHub.toggle()
      } label: {
        Text("홈 허브")
          .font(.system(size: 11, weight: viewModel.isShowingHomeHub ? .bold : .medium))
          .foregroundStyle(
            viewModel.isShowingHomeHub
              ? EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor
              : palette.secondaryText.swiftUIColor
          )
          .padding(.horizontal, 6)
          .padding(.vertical, 3)
      }
      .buttonStyle(.plain)
      .fixedSize(horizontal: true, vertical: false)

      let shortThemeTitle: String = {
        switch viewModel.themeMode {
        case .lightLavender: return "라이트"
        case .darkIndigo: return "다크"
        case .system: return "시스템"
        }
      }()
      FlatToolbarMenu(
        title: shortThemeTitle,
        symbolName: "paintpalette",
        palette: palette,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      ) {
        ForEach(WorkspaceThemeMode.allCases) { mode in
          Button(mode.rawValue) {
            viewModel.themeMode = mode
          }
        }
      }

      let isExpandedChevron = isDocumentStudio
        ? isStudioPanelsExpanded
        : (viewModel.showLeftNavigator || viewModel.showRightInspector)
      Button {
        if isDocumentStudio {
          isStudioPanelsExpanded.toggle()
          if isStudioPanelsExpanded {
            viewModel.showLeftNavigator = true
            viewModel.showRightInspector = true
          }
        } else {
          let nextVisible = !(viewModel.showLeftNavigator || viewModel.showRightInspector)
          viewModel.showLeftNavigator = nextVisible
          viewModel.showRightInspector = nextVisible
        }
      } label: {
        Text(isExpandedChevron ? "▴" : "▾")
          .font(.system(size: 11, weight: .bold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
          .frame(width: 20, height: 20)
      }
      .buttonStyle(.plain)
      .help("보조 리본 및 사이드 패널 펼치기/접기")
    }
    .padding(.horizontal, 8)
    .frame(height: 28)
    .background(rowBg)
  }
}

struct BottomWorkspaceStatusBarView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  let tab: WorkspaceTabItem
  @State private var isOvertypeMode: Bool = false

  var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
    let isDocumentStudio = (tab.moduleKind == .document || tab.moduleKind == .hwp)
    let statusBg = EasyOfficeDesignTokens.resolveHexColor(
      palette.isDark ? "#25292F" : "#F0F0F0",
      fallback: palette.cardSurface.swiftUIColor
    )
    let cleanModuleStatusTitle: String = {
      switch tab.moduleKind {
      case .document: return "문서"
      case .hwp: return "한글 문서"
      case .sheet: return "스프레드시트"
      case .slide: return "프레젠테이션"
      case .pdf: return "PDF"
      }
    }()

    HStack(spacing: 8) {
      AdaptiveScrollContainer(
        axes: .horizontal,
        showsIndicators: false,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      ) {
        HStack(spacing: 8) {
          if !isDocumentStudio {
            Text(cleanModuleStatusTitle)
              .font(.system(size: 11, weight: .medium))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
              .fixedSize(horizontal: true, vertical: false)

            Divider().frame(height: 12)
          }

          switch tab.payload {
          case .document(let doc):
            let sectionCount = max(1, doc.sections.count)
            let colCount = max(1, doc.sections.first?.columnCount ?? 1)
            let lineCount = max(1, doc.paragraphs.count)
            let activeParaChars = max(
              1,
              (doc.paragraphs.first(where: { $0.id == viewModel.selectedParagraphID })
                ?? doc.paragraphs.first)?.text.count ?? 1
            )
            let fileNameAccent = EasyOfficeDesignTokens.resolveHexColor(
              palette.isDark ? "#8BB8FF" : "#6182D6",
              fallback: palette.primaryText.swiftUIColor
            )
            HStack(spacing: 8) {
              Text("1/\(doc.estimatedPageCount) 쪽   \(colCount)단   \(lineCount)줄   \(activeParaChars)칸")
                .font(.system(size: 11, weight: .regular))
                .foregroundStyle(palette.primaryText.swiftUIColor)
              Text("|").font(.system(size: 10)).foregroundStyle(palette.secondaryText.swiftUIColor.opacity(0.6))
              Text("구역: 1 / \(sectionCount)")
                .font(.system(size: 11, weight: .regular))
                .foregroundStyle(palette.primaryText.swiftUIColor)
              Text("|").font(.system(size: 10)).foregroundStyle(palette.secondaryText.swiftUIColor.opacity(0.6))
              Button {
                isOvertypeMode.toggle()
              } label: {
                Text(isOvertypeMode ? "수정" : "삽입")
                  .font(.system(size: 11, weight: .regular))
                  .foregroundStyle(palette.primaryText.swiftUIColor)
              }
              .buttonStyle(.plain)
              .help("삽입 / 수정 입력 모드 전환")
              Text("|").font(.system(size: 10)).foregroundStyle(palette.secondaryText.swiftUIColor.opacity(0.6))
              Text("\(tab.fileName) — \(doc.estimatedPageCount)페이지")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(fileNameAccent)
            }
            .fixedSize(horizontal: true, vertical: false)

          case .sheet:
            Text(
              "시트 \(viewModel.activeSheetPageIndex + 1)/\(max(1, viewModel.activeWorkbookSheetNames.count)) · \(viewModel.selectedSheetRange) · \(viewModel.sheetSelectionStatusSummary)"
            )
            .font(.system(size: 11, weight: .medium, design: .monospaced))
            .foregroundStyle(palette.primaryText.swiftUIColor)
            .fixedSize(horizontal: true, vertical: false)

          case .slide(let deck):
            let curSlide =
              deck.slides.indices.contains(viewModel.selectedSlideIndex)
              ? deck.slides[viewModel.selectedSlideIndex] : deck.slides.first
            Text(
              "슬라이드 \(viewModel.selectedSlideIndex + 1)/\(max(1, deck.slides.count)) · 테마 \(deck.deckTheme.rawValue) · 전환 \(String(format: "%.1f", curSlide?.transitionDurationSeconds ?? 0.8))초"
            )
            .font(.system(size: 11, weight: .medium, design: .monospaced))
            .foregroundStyle(palette.primaryText.swiftUIColor)
            .fixedSize(horizontal: true, vertical: false)

          case .pdf(let pdf):
            HStack(spacing: 6) {
              Button {
                viewModel.selectedPDFPageIndex = max(0, viewModel.selectedPDFPageIndex - 1)
              } label: {
                Text("◀")
                  .font(.system(size: 10, weight: .semibold))
                  .foregroundStyle(palette.secondaryText.swiftUIColor)
              }
              .buttonStyle(.plain)
              .disabled(viewModel.selectedPDFPageIndex <= 0)

              Text("페이지 \(viewModel.selectedPDFPageNumber)/\(max(1, pdf.pages.count))")
                .font(.system(size: 11, weight: .semibold, design: .monospaced))
                .foregroundStyle(palette.primaryText.swiftUIColor)

              Button {
                viewModel.selectedPDFPageIndex = min(
                  max(0, pdf.pages.count - 1),
                  viewModel.selectedPDFPageIndex + 1
                )
              } label: {
                Text("▶")
                  .font(.system(size: 10, weight: .semibold))
                  .foregroundStyle(palette.secondaryText.swiftUIColor)
              }
              .buttonStyle(.plain)
              .disabled(viewModel.selectedPDFPageIndex + 1 >= pdf.pages.count)

              Text("· 주석 \(pdf.annotations.count)건 · 서명 \(pdf.totalInkStrokeCount)획")
                .font(.system(size: 11, weight: .medium, design: .monospaced))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
            }
            .fixedSize(horizontal: true, vertical: false)
          }

          if let secondary = viewModel.splitSecondaryTab {
            Divider().frame(height: 12)
            Text("스플릿 뷰: \(secondary.fileName)")
              .font(.system(size: 11, weight: .semibold))
              .foregroundStyle(EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor)
              .fixedSize(horizontal: true, vertical: false)
          }

          if let errorMessage = viewModel.lastErrorMessage {
            Divider().frame(height: 12)
            Text("오류: \(errorMessage)")
              .font(.system(size: 11, weight: .semibold))
              .foregroundStyle(EasyOfficeDesignTokens.statusDangerToken(isDark: palette.isDark).swiftUIColor)
              .lineLimit(1)
          } else if let summary = viewModel.lastExecutedCommandSummary, !summary.isEmpty {
            Divider().frame(height: 12)
            Text(summary)
              .font(.system(size: 11, weight: .medium))
              .foregroundStyle(EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor)
              .lineLimit(1)
          }
        }
      }

      Spacer(minLength: 8)

      HStack(spacing: 6) {
        ForEach(WorkspaceViewMode.allCases) { mode in
          Button {
            viewModel.setWorkspaceViewMode(mode)
          } label: {
            Text(mode.rawValue)
              .font(.system(size: 10, weight: viewModel.workspaceViewMode == mode ? .bold : .regular))
              .foregroundStyle(
                viewModel.workspaceViewMode == mode
                  ? EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor
                  : palette.secondaryText.swiftUIColor
              )
          }
          .buttonStyle(.plain)
        }
        Text("|").font(.system(size: 10)).foregroundStyle(palette.secondaryText.swiftUIColor.opacity(0.6))

        let zoomBorder = EasyOfficeDesignTokens.resolveHexColor(
          palette.isDark ? "#8BB8FF" : "#4A90D9",
          fallback: palette.hairlineBorder
        )
        Button { viewModel.applyQuickZoomPreset(120.0) } label: {
          Text("↔").font(.system(size: 9.5, weight: .bold)).foregroundStyle(zoomBorder)
            .frame(width: 17, height: 14).border(zoomBorder, width: 1)
        }
        .buttonStyle(.plain)
        .help("폭 맞춤 (120%)")

        Button { viewModel.applyQuickZoomPreset(100.0) } label: {
          Text("□").font(.system(size: 9.5, weight: .bold)).foregroundStyle(zoomBorder)
            .frame(width: 17, height: 14).border(zoomBorder, width: 1)
        }
        .buttonStyle(.plain)
        .help("쪽 맞춤 (100%)")

        Text("|").font(.system(size: 10)).foregroundStyle(palette.secondaryText.swiftUIColor.opacity(0.6))

        Text("\(Int(viewModel.zoomPercent))%")
          .font(.system(size: 11, weight: .regular))
          .foregroundStyle(palette.primaryText.swiftUIColor)
          .frame(minWidth: 34, alignment: .trailing)

        Button { viewModel.setZoom(viewModel.zoomPercent - 10.0) } label: {
          Image(systemName: "minus.magnifyingglass").font(.system(size: 11))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
        }
        .buttonStyle(.plain)
        .help("축소")

        if !viewModel.isCanvasSnapshotMode {
          Slider(
            value: Binding(get: { viewModel.zoomPercent }, set: { viewModel.setZoom($0) }),
            in: 50...200,
            step: 10
          )
          .frame(width: 64)
        }

        Button { viewModel.setZoom(viewModel.zoomPercent + 10.0) } label: {
          Image(systemName: "plus.magnifyingglass").font(.system(size: 11))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
        }
        .buttonStyle(.plain)
        .help("확대")
      }
      .fixedSize(horizontal: true, vertical: false)
    }
    .padding(.horizontal, 10)
    .frame(height: 22)
    .background(statusBg)
  }
}
