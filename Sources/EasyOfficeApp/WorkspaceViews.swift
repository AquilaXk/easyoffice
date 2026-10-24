import AppKit
import EasyOfficeCore
import EasyOfficeDesignSystem
import Observation
import PDFKit
import SwiftUI
import UniformTypeIdentifiers

public typealias WorkspaceFilePanelCoordinator = WorkspaceFileDialogPresenter
public typealias LeftStructureNavigatorView = LeftNavigatorPanelView

extension OfficeDocumentModel {
  public var wordCount: Int {
    computeStatistics().wordCount
  }

  public var characterCountIncludingSpaces: Int {
    computeStatistics().characterCountWithSpaces
  }
}

extension WorkspaceViewModel {
  public func executeFindAndReplaceInActiveTab() {
    _ = performFindAndReplace()
  }

  public var activeSheetPageIndex: Int {
    guard let tab = activeTab, case .sheet(let sheet) = tab.payload else {
      return 0
    }
    return sheet.activeSheetIndex
  }

  public var activeWorkbookSheetNames: [String] {
    guard let tab = activeTab, case .sheet(let sheet) = tab.payload else {
      return []
    }
    return sheet.allSheetNames
  }
}

extension WorkspaceThemePalette {
  public var primaryAccent: RGBColorToken {
    EasyOfficeDesignTokens.brand700
  }

  public var hairlineBorder: Color {
    border.swiftUIColor
  }
}

// MARK: - Signature Lavender Pure-SwiftUI Controls & Snapshot-Safe Containers

private struct CanvasSnapshotModeEnvironmentKey: EnvironmentKey {
  static let defaultValue: Bool = false
}

extension EnvironmentValues {
  var isCanvasSnapshotMode: Bool {
    get { self[CanvasSnapshotModeEnvironmentKey.self] }
    set { self[CanvasSnapshotModeEnvironmentKey.self] = newValue }
  }
}

struct AdaptiveScrollContainer<Content: View>: View {
  @Environment(\.isCanvasSnapshotMode) private var envSnapshotMode
  let axes: Axis.Set
  let showsIndicators: Bool
  let isSnapshotMode: Bool
  let content: () -> Content

  init(
    axes: Axis.Set = .vertical,
    showsIndicators: Bool = true,
    isSnapshotMode: Bool = false,
    @ViewBuilder content: @escaping () -> Content
  ) {
    self.axes = axes
    self.showsIndicators = showsIndicators
    self.isSnapshotMode = isSnapshotMode
    self.content = content
  }

  private var effectiveSnapshotMode: Bool {
    isSnapshotMode || envSnapshotMode
  }

  var body: some View {
    if effectiveSnapshotMode {
      if axes == .horizontal {
        content()
          .hidden()
          .frame(width: 0)
          .frame(maxWidth: .infinity, alignment: .leading)
          .overlay(alignment: .leading) {
            HStack(spacing: 0) {
              content()
            }
          }
          .clipped()
      } else {
        GeometryReader { geo in
          VStack(alignment: .leading, spacing: 0) {
            content()
          }
          .frame(width: geo.size.width, alignment: .topLeading)
          .clipped()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .clipped()
      }
    } else if axes == .horizontal {
      ViewThatFits(in: .horizontal) {
        content()
        ScrollView(.horizontal, showsIndicators: showsIndicators) {
          content()
        }
      }
    } else {
      ScrollView(axes, showsIndicators: showsIndicators) {
        content()
      }
    }
  }
}

struct FlatToolbarMenu<MenuContent: View>: View {
  @Environment(\.isCanvasSnapshotMode) private var envSnapshotMode
  let title: String
  var symbolName: String? = nil
  var accent: Color = EasyOfficeDesignTokens.brand700.swiftUIColor
  let palette: WorkspaceThemePalette
  var isSnapshotMode: Bool = false
  var minWidth: CGFloat? = nil
  @ViewBuilder let menuContent: () -> MenuContent

  init(
    title: String,
    symbolName: String? = nil,
    accent: Color = EasyOfficeDesignTokens.brand700.swiftUIColor,
    palette: WorkspaceThemePalette,
    isSnapshotMode: Bool = false,
    minWidth: CGFloat? = nil,
    @ViewBuilder menuContent: @escaping () -> MenuContent
  ) {
    self.title = title
    self.symbolName = symbolName
    self.accent = accent
    self.palette = palette
    self.isSnapshotMode = isSnapshotMode
    self.minWidth = minWidth
    self.menuContent = menuContent
  }

  init(
    title: String,
    systemImage: String?,
    accent: Color = EasyOfficeDesignTokens.brand700.swiftUIColor,
    palette: WorkspaceThemePalette,
    isSnapshotMode: Bool = false,
    minWidth: CGFloat? = nil,
    @ViewBuilder menuContent: @escaping () -> MenuContent
  ) {
    self.title = title
    self.symbolName = systemImage
    self.accent = accent
    self.palette = palette
    self.isSnapshotMode = isSnapshotMode
    self.minWidth = minWidth
    self.menuContent = menuContent
  }

  private var menuLabel: some View {
    HStack(spacing: 4) {
      if let symbolName {
        Image(systemName: symbolName)
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(accent)
      }
      Text(title)
        .font(.system(size: 11, weight: .medium))
        .foregroundStyle(palette.primaryText.swiftUIColor)
        .lineLimit(1)
      Spacer(minLength: 2)
      Image(systemName: "chevron.down")
        .font(.system(size: 8, weight: .bold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
    }
    .padding(.horizontal, 6)
    .padding(.vertical, 3)
    .frame(minWidth: minWidth)
  }

  var body: some View {
    if isSnapshotMode || envSnapshotMode {
      menuLabel
        .fixedSize(horizontal: minWidth == nil, vertical: false)
    } else {
      Menu {
        menuContent()
      } label: {
        menuLabel
      }
      .menuStyle(.borderlessButton)
      .buttonStyle(.plain)
      .fixedSize(horizontal: minWidth == nil, vertical: false)
    }
  }
}

struct FlatUnderlineTextField: View {
  @Environment(\.isCanvasSnapshotMode) private var envSnapshotMode
  let placeholder: String
  @Binding var text: String
  var symbolName: String? = nil
  var width: CGFloat? = nil
  var isMonospaced: Bool = false
  let palette: WorkspaceThemePalette
  var isSnapshotMode: Bool = false
  var onSubmit: (() -> Void)? = nil

  init(
    placeholder: String,
    text: Binding<String>,
    symbolName: String? = nil,
    width: CGFloat? = nil,
    isMonospaced: Bool = false,
    palette: WorkspaceThemePalette,
    isSnapshotMode: Bool = false,
    onSubmit: (() -> Void)? = nil
  ) {
    self.placeholder = placeholder
    self._text = text
    self.symbolName = symbolName
    self.width = width
    self.isMonospaced = isMonospaced
    self.palette = palette
    self.isSnapshotMode = isSnapshotMode
    self.onSubmit = onSubmit
  }

  init(
    placeholder: String,
    text: Binding<String>,
    systemImage: String?,
    width: CGFloat? = nil,
    isMonospaced: Bool = false,
    palette: WorkspaceThemePalette,
    isSnapshotMode: Bool = false,
    onSubmit: (() -> Void)? = nil
  ) {
    self.placeholder = placeholder
    self._text = text
    self.symbolName = systemImage
    self.width = width
    self.isMonospaced = isMonospaced
    self.palette = palette
    self.isSnapshotMode = isSnapshotMode
    self.onSubmit = onSubmit
  }

  var body: some View {
    HStack(spacing: 4) {
      if let symbolName {
        Image(systemName: symbolName)
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
      }
      if isSnapshotMode || envSnapshotMode {
        Text(text.isEmpty ? placeholder : text)
          .font(
            isMonospaced
              ? .system(size: 11, design: .monospaced)
              : .system(size: 11)
          )
          .foregroundStyle(
            text.isEmpty
              ? palette.secondaryText.swiftUIColor.opacity(0.75)
              : palette.primaryText.swiftUIColor
          )
          .lineLimit(1)
          .frame(maxWidth: .infinity, alignment: .leading)
      } else {
        TextField(placeholder, text: $text)
          .textFieldStyle(.plain)
          .font(
            isMonospaced
              ? .system(size: 11, design: .monospaced)
              : .system(size: 11)
          )
          .foregroundStyle(palette.primaryText.swiftUIColor)
          .onSubmit {
            onSubmit?()
          }
      }
    }
    .padding(.horizontal, 6)
    .padding(.vertical, 3)
    .frame(width: width)
    .frame(maxWidth: width == nil ? .infinity : nil, alignment: .leading)
    .overlay(alignment: .bottom) {
      Rectangle()
        .fill(palette.hairlineBorder)
        .frame(height: 1)
    }
  }
}

struct FlatToggleControl: View {
  let title: String
  @Binding var isOn: Bool
  var symbolName: String? = nil
  var accent: Color = EasyOfficeDesignTokens.brand700.swiftUIColor
  let palette: WorkspaceThemePalette

  init(
    title: String,
    isOn: Binding<Bool>,
    symbolName: String? = nil,
    accent: Color = EasyOfficeDesignTokens.brand700.swiftUIColor,
    palette: WorkspaceThemePalette
  ) {
    self.title = title
    self._isOn = isOn
    self.symbolName = symbolName
    self.accent = accent
    self.palette = palette
  }

  init(
    title: String,
    systemImage: String?,
    isOn: Binding<Bool>,
    accent: Color = EasyOfficeDesignTokens.brand700.swiftUIColor,
    palette: WorkspaceThemePalette
  ) {
    self.title = title
    self._isOn = isOn
    self.symbolName = systemImage
    self.accent = accent
    self.palette = palette
  }

  var body: some View {
    Button {
      isOn.toggle()
    } label: {
      HStack(spacing: 4) {
        Image(systemName: isOn ? "checkmark.square.fill" : (symbolName ?? "square"))
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(isOn ? accent : palette.secondaryText.swiftUIColor)
        Text(title)
          .font(.system(size: 11, weight: isOn ? .semibold : .regular))
          .foregroundStyle(
            isOn ? accent : palette.primaryText.swiftUIColor
          )
          .lineLimit(1)
      }
      .padding(.horizontal, 6)
      .padding(.vertical, 3)
    }
    .buttonStyle(.plain)
    .fixedSize(horizontal: true, vertical: false)
  }
}

struct FlatStepperControl: View {
  var title: String? = nil
  let valueText: String
  let palette: WorkspaceThemePalette
  let onDecrement: () -> Void
  let onIncrement: () -> Void

  init(
    title: String? = nil,
    valueText: String,
    palette: WorkspaceThemePalette,
    onDecrement: @escaping () -> Void,
    onIncrement: @escaping () -> Void
  ) {
    self.title = title
    self.valueText = valueText
    self.palette = palette
    self.onDecrement = onDecrement
    self.onIncrement = onIncrement
  }

  init(
    label: String,
    value: Binding<Int>,
    range: ClosedRange<Int>,
    palette: WorkspaceThemePalette
  ) {
    self.title = label
    self.valueText = "\(value.wrappedValue)"
    self.palette = palette
    self.onDecrement = {
      value.wrappedValue = max(range.lowerBound, value.wrappedValue - 1)
    }
    self.onIncrement = {
      value.wrappedValue = min(range.upperBound, value.wrappedValue + 1)
    }
  }

  var body: some View {
    HStack(spacing: 4) {
      if let title {
        Text(title)
          .font(.system(size: 10, weight: .medium))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
      }
      Button(action: onDecrement) {
        Image(systemName: "minus")
          .font(.system(size: 9, weight: .bold))
          .foregroundStyle(palette.primaryText.swiftUIColor)
          .frame(width: 16, height: 16)
      }
      .buttonStyle(.plain)

      Text(valueText)
        .font(.system(size: 11, weight: .semibold, design: .monospaced))
        .foregroundStyle(palette.primaryText.swiftUIColor)
        .padding(.horizontal, 4)

      Button(action: onIncrement) {
        Image(systemName: "plus")
          .font(.system(size: 9, weight: .bold))
          .foregroundStyle(palette.primaryText.swiftUIColor)
          .frame(width: 16, height: 16)
      }
      .buttonStyle(.plain)
    }
    .padding(.horizontal, 6)
    .padding(.vertical, 3)
    .fixedSize(horizontal: true, vertical: false)
  }
}

/// Native macOS file dialog, print panel, and pasteboard presenter for office documents.
@MainActor
public enum WorkspaceFileDialogPresenter {
  public static func presentOpenPanel(viewModel: WorkspaceViewModel) {
    let panel = NSOpenPanel()
    panel.title = "문서 파일 열기"
    panel.canChooseFiles = true
    panel.canChooseDirectories = false
    panel.allowsMultipleSelection = false
    if panel.runModal() == .OK, let url = panel.url {
      do {
        _ = try viewModel.loadFromDisk(url: url)
      } catch {
        viewModel.lastErrorMessage = error.localizedDescription
      }
    }
  }

  public static func presentOpenImagePanel(for viewModel: WorkspaceViewModel) {
    presentOpenImagePanel(viewModel: viewModel)
  }

  public static func presentOpenImagePanel(viewModel: WorkspaceViewModel) {
    let panel = NSOpenPanel()
    panel.title = "본문 첨부 이미지 파일 선택"
    panel.canChooseFiles = true
    panel.canChooseDirectories = false
    panel.allowsMultipleSelection = false
    panel.allowedContentTypes = [.png, .jpeg, .tiff]
    if panel.runModal() == .OK, let url = panel.url {
      do {
        try viewModel.loadImageAttachmentFromDisk(url: url)
      } catch {
        viewModel.lastErrorMessage = error.localizedDescription
      }
    }
  }

  public static func presentSavePanel(viewModel: WorkspaceViewModel) {
    guard let activeTab = viewModel.activeTab else { return }
    let panel = NSSavePanel()
    panel.title = "현재 문서 저장"
    panel.nameFieldStringValue = activeTab.fileName
    panel.canCreateDirectories = true
    if panel.runModal() == .OK, let url = panel.url {
      do {
        try viewModel.saveTabToDisk(id: activeTab.id, to: url)
      } catch {
        viewModel.lastErrorMessage = error.localizedDescription
      }
    }
  }

  public static func presentPrintPanel(viewModel: WorkspaceViewModel) {
    guard let activeTab = viewModel.activeTab else { return }
    do {
      let pdfBytes = try viewModel.exportTabData(id: activeTab.id, targetFormat: .pdf)
      if let pdfDoc = PDFDocument(data: pdfBytes),
        let printOp = pdfDoc.printOperation(
          for: .shared, scalingMode: .pageScaleToFit, autoRotate: true)
      {
        printOp.jobTitle = activeTab.fileName
        if NSApp.keyWindow != nil { printOp.run() }
      }
      viewModel.lastExecutedCommandSummary =
        "인쇄 작업 준비 완료: \(activeTab.fileName) (\(pdfBytes.count) bytes)"
    } catch {
      viewModel.lastErrorMessage = error.localizedDescription
    }
  }

  public static func copySelectionToPasteboard(viewModel: WorkspaceViewModel) {
    guard let activeTab = viewModel.activeTab else { return }
    let textToCopy: String
    switch activeTab.payload {
    case .document(let doc):
      if let selectedID = viewModel.selectedParagraphID,
        let para = doc.paragraphs.first(where: { $0.id == selectedID })
      {
        textToCopy = para.text
      } else {
        textToCopy = doc.paragraphs.first?.text ?? doc.title
      }
    case .sheet(let sheet):
      let raw = sheet.rawValue(at: viewModel.selectedSheetCell)
      textToCopy = raw.isEmpty ? sheet.evaluate(at: viewModel.selectedSheetCell).description : raw
    case .slide(let deck):
      let sIdx =
        deck.slides.indices.contains(viewModel.selectedSlideIndex)
        ? viewModel.selectedSlideIndex : 0
      if deck.slides.indices.contains(sIdx) {
        let slide = deck.slides[sIdx]
        if let elemID = viewModel.selectedSlideElementID,
          let elem = slide.elements.first(where: { $0.id == elemID })
        {
          textToCopy = "\(elem.title): \(elem.bodyText)"
        } else {
          textToCopy = slide.title
        }
      } else {
        textToCopy = deck.title
      }
    case .pdf(let pdf):
      let pIdx =
        pdf.pages.indices.contains(viewModel.selectedPDFPageIndex)
        ? viewModel.selectedPDFPageIndex : 0
      textToCopy = pdf.pages.indices.contains(pIdx) ? pdf.pages[pIdx].extractedText : pdf.title
    }
    NSPasteboard.general.clearContents()
    NSPasteboard.general.setString(textToCopy, forType: .string)
    viewModel.lastExecutedCommandSummary = "클립보드 복사 완료 (\(textToCopy.count)자)"
  }

  public static func pasteFromPasteboard(viewModel: WorkspaceViewModel) {
    guard let activeTab = viewModel.activeTab,
      let pasted = NSPasteboard.general.string(forType: .string)?.trimmingCharacters(
        in: .whitespacesAndNewlines),
      !pasted.isEmpty
    else { return }
    switch activeTab.payload {
    case .document: viewModel.appendParagraphToActiveDocument(text: pasted, style: .body)
    case .sheet:
      viewModel.formulaBarDraft = pasted
      viewModel.applyFormulaBarEdit()
    case .slide:
      viewModel.addSlideBulletPoint(slideIndex: viewModel.selectedSlideIndex, text: pasted)
    case .pdf: viewModel.addPDFAnnotation(kind: .stickyNote, content: pasted)
    }
    viewModel.lastExecutedCommandSummary = "클립보드 붙여넣기 완료"
  }
}

public typealias EasyOfficeMainWorkspaceView = MainWorkspaceWindowView

public struct MainWorkspaceWindowView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel

  public init(viewModel: WorkspaceViewModel) {
    self.viewModel = viewModel
  }

  public var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)

    ZStack {
      VStack(spacing: 0) {
        TopDocumentTabStripView(viewModel: viewModel)
        Divider()

        if viewModel.isShowingHomeHub || viewModel.activeTab == nil {
          HomeHubWorkspaceView(viewModel: viewModel)
        } else if let tab = viewModel.activeTab {
          ContextRibbonBarView(viewModel: viewModel, activeModule: tab.moduleKind)
          Divider()

          if viewModel.isFindReplaceBarVisible {
            FindAndReplaceBarView(viewModel: viewModel, palette: palette)
            Divider()
          }

          HStack(spacing: 0) {
            if viewModel.showLeftNavigator {
              LeftNavigatorPanelView(viewModel: viewModel, tab: tab)
                .frame(width: EasyOfficeDesignTokens.LayoutMetrics.leftNavigatorWidth)
              Divider()
            }

            ActiveEditorCanvasRouterView(viewModel: viewModel, tab: tab)

            if viewModel.showRightInspector {
              Divider()
              RightContextInspectorView(viewModel: viewModel, tab: tab)
                .frame(width: EasyOfficeDesignTokens.LayoutMetrics.rightInspectorWidth)
            }
          }
          Divider()
          BottomWorkspaceStatusBarView(viewModel: viewModel, tab: tab)
        }
      }
      .background(palette.background.swiftUIColor)

      if viewModel.isFullScreenSlideshowActive {
        SlideshowFullScreenOverlayView(viewModel: viewModel, palette: palette)
      }
    }
    .environment(\.isCanvasSnapshotMode, viewModel.isCanvasSnapshotMode)
  }
}

struct TopDocumentTabStripView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel

  private func qatIconButton(
    symbol: String,
    helpText: String,
    isActive: Bool = false,
    isDisabled: Bool = false,
    palette: WorkspaceThemePalette,
    action: @escaping () -> Void
  ) -> some View {
    Button(action: action) {
      Image(systemName: symbol)
        .font(.system(size: 12, weight: isActive ? .bold : .medium))
        .foregroundStyle(
          isActive
            ? EasyOfficeDesignTokens.brand700.swiftUIColor
            : (isDisabled
              ? palette.secondaryText.swiftUIColor.opacity(0.4)
              : palette.primaryText.swiftUIColor)
        )
        .frame(width: 24, height: 24)
        .background(
          isActive
            ? EasyOfficeDesignTokens.brand700.swiftUIColor.opacity(0.14)
            : Color.clear
        )
    }
    .buttonStyle(.plain)
    .disabled(isDisabled)
    .help(helpText)
  }

  var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)

    HStack(spacing: 8) {
      Button {
        viewModel.isShowingHomeHub.toggle()
      } label: {
        HStack(spacing: 5) {
          Image(systemName: "square.grid.2x2.fill")
            .font(.system(size: 11, weight: .semibold))
          Text("홈 허브")
            .font(.system(size: 12, weight: viewModel.isShowingHomeHub ? .bold : .semibold))
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 5)
        .foregroundStyle(
          viewModel.isShowingHomeHub
            ? EasyOfficeDesignTokens.brand700.swiftUIColor
            : palette.primaryText.swiftUIColor
        )
        .overlay(alignment: .bottom) {
          Rectangle()
            .fill(
              viewModel.isShowingHomeHub
                ? EasyOfficeDesignTokens.brand700.swiftUIColor
                : Color.clear
            )
            .frame(height: 2)
        }
      }
      .buttonStyle(.plain)
      .fixedSize(horizontal: true, vertical: false)

      Rectangle()
        .fill(palette.hairlineBorder)
        .frame(width: 1, height: 14)

      HStack(spacing: 2) {
        qatIconButton(
          symbol: "folder",
          helpText: "열기 (⌘O)",
          palette: palette
        ) {
          WorkspaceFileDialogPresenter.presentOpenPanel(viewModel: viewModel)
        }

        qatIconButton(
          symbol: "square.and.arrow.down",
          helpText: "저장 (⌘S)",
          isDisabled: viewModel.activeTab == nil,
          palette: palette
        ) {
          WorkspaceFileDialogPresenter.presentSavePanel(viewModel: viewModel)
        }

        qatIconButton(
          symbol: "printer",
          helpText: "인쇄 (⌘P)",
          isDisabled: viewModel.activeTab == nil,
          palette: palette
        ) {
          WorkspaceFileDialogPresenter.presentPrintPanel(viewModel: viewModel)
        }

        qatIconButton(
          symbol: "arrow.uturn.backward",
          helpText: "실행 취소 (⌘Z)",
          isDisabled: !viewModel.canUndo,
          palette: palette
        ) {
          viewModel.undo()
        }

        qatIconButton(
          symbol: "arrow.uturn.forward",
          helpText: "다시 실행 (⇧⌘Z)",
          isDisabled: !viewModel.canRedo,
          palette: palette
        ) {
          viewModel.redo()
        }

        qatIconButton(
          symbol: "magnifyingglass",
          helpText: "찾기 및 바꾸기 (⌘F)",
          isActive: viewModel.isFindReplaceBarVisible,
          isDisabled: viewModel.activeTab == nil,
          palette: palette
        ) {
          viewModel.isFindReplaceBarVisible.toggle()
        }
      }
      .fixedSize(horizontal: true, vertical: false)

      Rectangle()
        .fill(palette.hairlineBorder)
        .frame(width: 1, height: 14)

      AdaptiveScrollContainer(
        axes: .horizontal,
        showsIndicators: false,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      ) {
        HStack(spacing: 0) {
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

              Circle()
                .fill(accent.swiftUIColor)
                .frame(width: 6, height: 6)

              Text(tab.fileName)
                .font(.system(size: 11, weight: isSelected ? .semibold : .regular))
                .foregroundStyle(
                  isSelected
                    ? palette.primaryText.swiftUIColor
                    : palette.secondaryText.swiftUIColor
                )
                .lineLimit(1)
                .truncationMode(.middle)
                .frame(maxWidth: 188, alignment: .leading)

              if tab.isModified {
                Circle()
                  .fill(EasyOfficeDesignTokens.brand600.swiftUIColor)
                  .frame(width: 5, height: 5)
              }

              Button {
                viewModel.closeTab(id: tab.id)
              } label: {
                Image(systemName: "xmark")
                  .font(.system(size: 9, weight: .semibold))
                  .foregroundStyle(palette.secondaryText.swiftUIColor)
              }
              .buttonStyle(.plain)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(
              isSelected ? palette.cardSurface.swiftUIColor : Color.clear
            )
            .overlay(alignment: .bottom) {
              Rectangle()
                .fill(
                  isSelected
                    ? EasyOfficeDesignTokens.brand700.swiftUIColor
                    : (isSecondarySplit
                      ? EasyOfficeDesignTokens.brand400.swiftUIColor
                      : Color.clear)
                )
                .frame(height: 2)
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
              Divider()
              if viewModel.openTabs.count > 1 {
                Button("다른 탭 닫기") { viewModel.closeOtherTabs(keeping: tab.id) }
              }
              Button("탭 닫기", role: .destructive) { viewModel.closeTab(id: tab.id) }
            }

            Rectangle()
              .fill(palette.hairlineBorder)
              .frame(width: 1, height: 14)
          }

          Button {
            viewModel.openNewSampleTab(for: viewModel.activeTab?.moduleKind ?? .document)
          } label: {
            Image(systemName: "plus")
              .font(.system(size: 11, weight: .semibold))
              .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
              .frame(width: 26, height: 24)
          }
          .buttonStyle(.plain)
          .help("현재 모듈 새 문서 탭 생성 (우클릭으로 형식 선택)")
          .contextMenu {
            Button("새 문서 (.docx)") { viewModel.openNewSampleTab(for: .document) }
            Button("새 한글 문서 (.hwpx)") { viewModel.openNewSampleTab(for: .hwp) }
            Button("새 스프레드시트 (.xlsx)") { viewModel.openNewSampleTab(for: .sheet) }
            Button("새 프레젠테이션 (.pptx)") { viewModel.openNewSampleTab(for: .slide) }
            Button("새 PDF 문서 (.pdf)") { viewModel.openNewSampleTab(for: .pdf) }
          }
        }
      }

      Spacer(minLength: 6)

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

      FlatUnderlineTextField(
        placeholder: "통합 검색",
        text: $viewModel.searchQuery,
        symbolName: "magnifyingglass",
        width: 102,
        palette: palette,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      )
    }
    .padding(.horizontal, 12)
    .frame(height: EasyOfficeDesignTokens.LayoutMetrics.tabStripHeight + 6)
    .background(palette.elevatedSurface.swiftUIColor)
  }
}

struct ContextRibbonBarView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  let activeModule: OfficeModuleKind

  var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
    let actions = RibbonCategory.actions(for: viewModel.activeRibbonCategory, module: activeModule)
    let primaryActions = Array(actions.prefix(2))
    let secondaryActions = Array(actions.dropFirst(2))
    let moduleAccent = EasyOfficeDesignTokens.accentToken(for: activeModule).swiftUIColor

    VStack(alignment: .leading, spacing: 6) {
      HStack(spacing: 8) {
        AdaptiveScrollContainer(
          axes: .horizontal,
          showsIndicators: false,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        ) {
          HStack(spacing: 12) {
            ForEach(RibbonCategory.allCases) { category in
              let isSelected = viewModel.activeRibbonCategory == category
              Button {
                viewModel.activeRibbonCategory = category
              } label: {
                VStack(spacing: 3) {
                  Text(viewModel.ribbonLayoutStyle.displayTitle(for: category))
                    .font(.system(size: 12, weight: isSelected ? .bold : .medium))
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
                    .foregroundStyle(
                      isSelected
                        ? palette.primaryText.swiftUIColor
                        : palette.secondaryText.swiftUIColor
                    )
                    .padding(.vertical, 3)
                    .padding(.horizontal, 4)

                  Rectangle()
                    .fill(isSelected ? moduleAccent : Color.clear)
                    .frame(height: 2)
                }
              }
              .buttonStyle(.plain)
            }
          }
        }

        Spacer(minLength: 8)

        HStack(spacing: 10) {
          Text(viewModel.activeContextualObjectToolBadge)
            .font(.system(size: 10, weight: .semibold))
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
            .foregroundStyle(moduleAccent)

          Button {
            viewModel.toggleRibbonLayoutStyle()
          } label: {
            Label(viewModel.ribbonLayoutStyle.rawValue, systemImage: "slider.horizontal.3")
              .font(.system(size: 11, weight: .medium))
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
              .foregroundStyle(palette.secondaryText.swiftUIColor)
          }
          .buttonStyle(.plain)

          if viewModel.splitSecondaryTab != nil {
            Button {
              viewModel.enableSplitView(secondaryTabID: nil)
            } label: {
              Label("스플릿 해제", systemImage: "rectangle.split.2x1.slash")
                .font(.system(size: 11, weight: .semibold))
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .foregroundStyle(moduleAccent)
            }
            .buttonStyle(.plain)
          } else if let candidate = viewModel.openTabs.first(where: {
            $0.id != viewModel.activeTabID
          }) {
            Button {
              viewModel.enableSplitView(secondaryTabID: candidate.id)
            } label: {
              Label("스플릿 뷰", systemImage: "rectangle.split.2x1")
                .font(.system(size: 11))
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .foregroundStyle(palette.secondaryText.swiftUIColor)
            }
            .buttonStyle(.plain)
          }

          Button {
            viewModel.showLeftNavigator.toggle()
          } label: {
            Label("내비게이터", systemImage: "sidebar.left")
              .font(.system(size: 11, weight: viewModel.showLeftNavigator ? .semibold : .regular))
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
              .foregroundStyle(
                viewModel.showLeftNavigator
                  ? palette.primaryText.swiftUIColor
                  : palette.secondaryText.swiftUIColor
              )
          }
          .buttonStyle(.plain)

          Button {
            viewModel.showRightInspector.toggle()
          } label: {
            Label("인스펙터", systemImage: "sidebar.right")
              .font(.system(size: 11, weight: viewModel.showRightInspector ? .semibold : .regular))
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
              .foregroundStyle(
                viewModel.showRightInspector
                  ? palette.primaryText.swiftUIColor
                  : palette.secondaryText.swiftUIColor
              )
          }
          .buttonStyle(.plain)

          Button {
            viewModel.isRibbonCollapsed.toggle()
          } label: {
            Image(systemName: viewModel.isRibbonCollapsed ? "chevron.down" : "chevron.up")
              .font(.system(size: 11, weight: .semibold))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
          }
          .buttonStyle(.plain)
          .accessibilityLabel(
            AccessibilityTokens.ribbonCollapseToggleLabel(isCollapsed: viewModel.isRibbonCollapsed)
          )
        }
        .fixedSize(horizontal: true, vertical: false)
      }

      if !viewModel.isRibbonCollapsed {
        AdaptiveScrollContainer(
          axes: .horizontal,
          showsIndicators: false,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        ) {
          HStack(alignment: .center, spacing: 10) {
            ribbonGroupContainer(title: "편집·실행", palette: palette) {
              HStack(spacing: 4) {
                ribbonQuickButton(
                  title: "실행 취소",
                  symbolName: "arrow.uturn.backward",
                  accent: moduleAccent,
                  palette: palette
                ) {
                  viewModel.undoActiveTab()
                }
                .disabled(!viewModel.canUndo)

                ribbonQuickButton(
                  title: "다시 실행",
                  symbolName: "arrow.uturn.forward",
                  accent: moduleAccent,
                  palette: palette
                ) {
                  viewModel.redoActiveTab()
                }
                .disabled(!viewModel.canRedo)

                ribbonQuickButton(
                  title: "찾기",
                  symbolName: "magnifyingglass",
                  accent: moduleAccent,
                  palette: palette,
                  isActive: viewModel.isFindReplaceBarPresented
                ) {
                  viewModel.isFindReplaceBarPresented.toggle()
                }
              }
            }

            Rectangle()
              .fill(palette.hairlineBorder)
              .frame(width: 1, height: 22)

            ribbonGroupContainer(title: "핵심 리본 도구", palette: palette) {
              HStack(spacing: 4) {
                ForEach(primaryActions) { command in
                  let isCommandOn = isRibbonCommandActive(command.id)
                  Button {
                    viewModel.executeRibbonCommand(command.id)
                  } label: {
                    HStack(spacing: 4) {
                      Image(systemName: command.symbolName)
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(moduleAccent)
                      Text(command.title)
                        .font(.system(size: 11, weight: isCommandOn ? .bold : .medium))
                        .foregroundStyle(
                          isCommandOn ? moduleAccent : palette.primaryText.swiftUIColor)
                      Text(command.shortcutHint)
                        .font(.system(size: 9, design: .monospaced))
                        .foregroundStyle(palette.secondaryText.swiftUIColor)
                    }
                    .padding(.horizontal, 6)
                    .padding(.vertical, 4)
                    .background(
                      isCommandOn ? moduleAccent.opacity(0.14) : Color.clear
                    )
                  }
                  .buttonStyle(.plain)
                  .fixedSize(horizontal: true, vertical: false)
                  .accessibilityHint(AccessibilityTokens.ribbonControlHint)
                }
              }
            }

            if !secondaryActions.isEmpty {
              Rectangle()
                .fill(palette.hairlineBorder)
                .frame(width: 1, height: 22)

              ribbonGroupContainer(title: "서식·문서 구조", palette: palette) {
                HStack(spacing: 4) {
                  ForEach(secondaryActions) { command in
                    let isCommandOn = isRibbonCommandActive(command.id)
                    Button {
                      viewModel.executeRibbonCommand(command.id)
                    } label: {
                      HStack(spacing: 4) {
                        Image(systemName: command.symbolName)
                          .font(.system(size: 11))
                          .foregroundStyle(moduleAccent)
                        Text(command.title)
                          .font(.system(size: 11, weight: isCommandOn ? .bold : .medium))
                          .foregroundStyle(
                            isCommandOn ? moduleAccent : palette.primaryText.swiftUIColor)
                        Text(command.shortcutHint)
                          .font(.system(size: 9, design: .monospaced))
                          .foregroundStyle(palette.secondaryText.swiftUIColor)
                      }
                      .padding(.horizontal, 6)
                      .padding(.vertical, 4)
                      .background(
                        isCommandOn ? moduleAccent.opacity(0.14) : Color.clear
                      )
                    }
                    .buttonStyle(.plain)
                    .fixedSize(horizontal: true, vertical: false)
                    .accessibilityHint(AccessibilityTokens.ribbonControlHint)
                  }
                }
              }
            }

            Rectangle()
              .fill(palette.hairlineBorder)
              .frame(width: 1, height: 22)

            ribbonGroupContainer(title: "빠른 조판·분석", palette: palette) {
              HStack(spacing: 4) {
                moduleContextualQuickButtons(accent: moduleAccent, palette: palette)
              }
            }
          }
          .padding(.vertical, 2)
        }
      }
    }
    .padding(.horizontal, 14)
    .padding(.vertical, 6)
    .background(palette.cardSurface.swiftUIColor)
  }

  private func isRibbonCommandActive(_ commandID: String) -> Bool {
    if commandID.hasSuffix("-view-split") {
      return viewModel.splitSecondaryTab != nil
    }
    if commandID.hasSuffix("-view-theme") {
      return viewModel.themeMode == .darkIndigo
    }
    if commandID.hasSuffix("-view-zoom") {
      return viewModel.zoomPercent > 100.0
    }
    guard let tab = viewModel.activeTab else { return false }
    switch tab.payload {
    case .document(let doc):
      let activeParagraph =
        doc.paragraphs.first(where: { $0.id == viewModel.selectedParagraphID })
        ?? doc.paragraphs.first
      if commandID.hasSuffix("-home-bold") { return activeParagraph?.isBold == true }
      if commandID.hasSuffix("-home-italic") { return activeParagraph?.isItalic == true }
      if commandID.hasSuffix("-home-underline") { return activeParagraph?.isUnderlined == true }
      if commandID.hasSuffix("-layout-columns") {
        return (doc.sections.first?.columnCount ?? 1) > 1
      }
      if commandID.hasSuffix("-layout-orientation") || commandID.hasSuffix("-layout-orient") {
        return doc.sections.first?.orientation == .landscape
      }
      if commandID == "doc-rev-track" { return doc.trackChangesEnabled }
      return false
    case .sheet(let sheet):
      let style = sheet.cellStyle(at: viewModel.selectedSheetCell)
      if commandID.hasSuffix("-home-bold") { return style.isBold }
      if commandID.hasSuffix("-home-italic") { return style.isItalic }
      if commandID.hasSuffix("-home-underline") { return style.isUnderlined }
      if commandID == "sheet-mod-freeze" {
        return viewModel.isHeaderRowFrozen || viewModel.isFirstColumnFrozen
      }
      if commandID == "sheet-mod-filter" { return !viewModel.sheetAutoFilterQuery.isEmpty }
      return false
    case .slide(let deck):
      let curSlide =
        deck.slides.indices.contains(viewModel.selectedSlideIndex)
        ? deck.slides[viewModel.selectedSlideIndex] : deck.slides.first
      let curEl =
        curSlide?.elements.first(where: { $0.id == viewModel.selectedSlideElementID })
        ?? curSlide?.elements.first
      if commandID.hasSuffix("-home-bold") { return curEl?.isBold == true }
      if commandID.hasSuffix("-home-italic") { return curEl?.isItalic == true }
      if commandID.hasSuffix("-home-underline") { return curEl?.isUnderlined == true }
      return false
    case .pdf(let pdf):
      if commandID.hasSuffix("-home-underline") {
        return pdf.annotations.contains(where: { $0.kind == .underline })
      }
      return false
    }
  }

  @ViewBuilder
  private func ribbonGroupContainer<Content: View>(
    title: String,
    palette: WorkspaceThemePalette,
    @ViewBuilder content: () -> Content
  ) -> some View {
    VStack(alignment: .center, spacing: 2) {
      content()
      Text(title)
        .font(.system(size: 9, weight: .medium))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
        .fixedSize(horizontal: true, vertical: false)
    }
  }

  private func ribbonQuickButton(
    title: String,
    symbolName: String,
    accent: Color,
    palette: WorkspaceThemePalette,
    isActive: Bool = false,
    action: @escaping () -> Void
  ) -> some View {
    Button(action: action) {
      HStack(spacing: 4) {
        Image(systemName: symbolName)
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(accent)
        Text(title)
          .font(.system(size: 11, weight: isActive ? .bold : .medium))
          .foregroundStyle(isActive ? accent : palette.primaryText.swiftUIColor)
      }
      .padding(.horizontal, 6)
      .padding(.vertical, 4)
      .background(isActive ? accent.opacity(0.14) : Color.clear)
    }
    .buttonStyle(.plain)
    .fixedSize(horizontal: true, vertical: false)
  }

  @ViewBuilder
  private func moduleContextualQuickButtons(
    accent: Color,
    palette: WorkspaceThemePalette
  ) -> some View {
    switch activeModule {
    case .document, .hwp:
      let activeDoc: OfficeDocumentModel? = {
        if case .document(let d) = viewModel.activeTab?.payload { return d }
        return nil
      }()
      let activePara =
        activeDoc?.paragraphs.first(where: { $0.id == viewModel.selectedParagraphID })
        ?? activeDoc?.paragraphs.first
      if viewModel.ribbonLayoutStyle == .hangulHwp {
        ribbonQuickButton(
          title: "200자 원고지",
          symbolName: "square.grid.3x3",
          accent: accent,
          palette: palette,
          isActive: viewModel.isManuscriptGridModeEnabled
        ) {
          viewModel.toggleManuscriptGridMode()
        }
        ribbonQuickButton(
          title: "다단 전환",
          symbolName: "rectangle.split.2x1",
          accent: accent,
          palette: palette,
          isActive: (activeDoc?.sections.first?.columnCount ?? 1) > 1
        ) {
          viewModel.cycleActiveDocumentColumnCount()
        }
        ribbonQuickButton(
          title: "각주 삽입",
          symbolName: "text.badge.plus",
          accent: accent,
          palette: palette
        ) {
          viewModel.executeRibbonCommand("doc-mod-footnote")
        }
        ribbonQuickButton(
          title: "자간-",
          symbolName: "arrow.right.and.line.vertical.and.arrow.left",
          accent: accent,
          palette: palette
        ) {
          viewModel.adjustActiveParagraphTypography(letterSpacingDelta: -5.0)
        }
        ribbonQuickButton(
          title: "자간+",
          symbolName: "arrow.left.and.line.vertical.and.arrow.right",
          accent: accent,
          palette: palette
        ) {
          viewModel.adjustActiveParagraphTypography(letterSpacingDelta: 5.0)
        }
        ribbonQuickButton(
          title: "누름틀 치환",
          symbolName: "wand.and.stars",
          accent: accent,
          palette: palette
        ) {
          viewModel.applyMailMergePlaceholdersToActiveDocument()
        }
      } else {
        ribbonQuickButton(
          title: "형광펜",
          symbolName: "highlighter",
          accent: accent,
          palette: palette,
          isActive: activePara?.inlineRuns.contains(where: { !($0.backgroundHex ?? "").isEmpty })
            == true
        ) {
          viewModel.toggleHighlightOnActiveParagraph()
        }
        ribbonQuickButton(
          title: "취소선",
          symbolName: "strikethrough",
          accent: accent,
          palette: palette,
          isActive: activePara?.inlineRuns.contains(where: \.isStrikethrough) == true
        ) {
          viewModel.toggleStrikethroughOnActiveParagraph()
        }
        ribbonQuickButton(
          title: "링크",
          symbolName: "link",
          accent: accent,
          palette: palette
        ) {
          viewModel.insertHyperlinkIntoActiveParagraph()
        }
        ribbonQuickButton(
          title: "자간-",
          symbolName: "arrow.right.and.line.vertical.and.arrow.left",
          accent: accent,
          palette: palette
        ) {
          viewModel.adjustActiveParagraphTypography(letterSpacingDelta: -5.0)
        }
        ribbonQuickButton(
          title: "자간+",
          symbolName: "arrow.left.and.line.vertical.and.arrow.right",
          accent: accent,
          palette: palette
        ) {
          viewModel.adjustActiveParagraphTypography(letterSpacingDelta: 5.0)
        }
      }

    case .sheet:
      let activeSheet: WorksheetModel? = {
        if case .sheet(let s) = viewModel.activeTab?.payload { return s }
        return nil
      }()
      let activeStyle = activeSheet?.cellStyle(at: viewModel.selectedSheetCell)
      ribbonQuickButton(
        title: "₩ 통화",
        symbolName: "wonsign.circle",
        accent: accent,
        palette: palette,
        isActive: activeStyle?.numberFormat == .currencyKRW
      ) {
        viewModel.applyQuickNumberFormatOnSelectedSheetCell(.currencyKRW)
      }
      ribbonQuickButton(
        title: "% 백분율",
        symbolName: "percent",
        accent: accent,
        palette: palette,
        isActive: activeStyle?.numberFormat == .percentage
      ) {
        viewModel.applyQuickNumberFormatOnSelectedSheetCell(.percentage)
      }
      ribbonQuickButton(
        title: "줄바꿈",
        symbolName: "text.word.spacing",
        accent: accent,
        palette: palette,
        isActive: activeStyle?.wrapText == true
      ) {
        viewModel.toggleWrapTextOnSelectedSheetCell()
      }
      ribbonQuickButton(
        title: "A열 고정",
        symbolName: "rectangle.split.2x1",
        accent: accent,
        palette: palette,
        isActive: viewModel.isFirstColumnFrozen
      ) {
        viewModel.toggleFirstColumnFrozen()
      }
      ribbonQuickButton(
        title: "데이터 막대",
        symbolName: "chart.bar.xaxis",
        accent: accent,
        palette: palette,
        isActive: viewModel.isDataBarsEnabled
      ) {
        viewModel.toggleSheetDataBars()
      }

    case .slide:
      ribbonQuickButton(
        title: "+ 슬라이드",
        symbolName: "plus.rectangle.on.rectangle",
        accent: accent,
        palette: palette
      ) {
        viewModel.addSlideToActiveDeck()
      }
      ribbonQuickButton(
        title: "복제",
        symbolName: "doc.on.doc",
        accent: accent,
        palette: palette
      ) {
        viewModel.duplicateActiveSlide()
      }
      ribbonQuickButton(
        title: "정렬 뷰",
        symbolName: "square.grid.3x2",
        accent: accent,
        palette: palette,
        isActive: viewModel.isSlideSorterViewEnabled
      ) {
        viewModel.toggleSlideSorterView()
      }

    case .pdf:
      let activePDF: PDFStudioDocumentModel? = {
        if case .pdf(let p) = viewModel.activeTab?.payload { return p }
        return nil
      }()
      ribbonQuickButton(
        title: "워터마크",
        symbolName: "drop.triangle",
        accent: accent,
        palette: palette,
        isActive: !(activePDF?.watermarkText.isEmpty ?? true)
      ) {
        viewModel.applyWatermarkOnActivePDF()
      }
      ribbonQuickButton(
        title: "리댁션",
        symbolName: "eye.slash.fill",
        accent: accent,
        palette: palette
      ) {
        viewModel.redactSensitiveTextOnActivePDF()
      }
      ribbonQuickButton(
        title: "자유곡선 펜",
        symbolName: "pencil.tip.crop.circle",
        accent: accent,
        palette: palette,
        isActive: viewModel.isPDFInkDrawingModeEnabled
      ) {
        viewModel.isPDFInkDrawingModeEnabled.toggle()
      }
    }
  }
}

struct FindAndReplaceBarView: View {
  @Bindable var viewModel: WorkspaceViewModel
  let palette: WorkspaceThemePalette

  var body: some View {
    HStack(spacing: 10) {
      AdaptiveScrollContainer(
        axes: .horizontal,
        showsIndicators: false,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      ) {
        HStack(spacing: 10) {
          Label("찾기 및 바꾸기", systemImage: "magnifyingglass")
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
            .fixedSize(horizontal: true, vertical: false)

          FlatUnderlineTextField(
            placeholder: "찾을 내용...",
            text: $viewModel.findQueryText,
            symbolName: "magnifyingglass",
            width: 145,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )

          let totalMatches = viewModel.findMatchCount
          let displayMatchIndex =
            totalMatches > 0 ? max(1, min(viewModel.currentFindMatchIndex, totalMatches)) : 0
          Text("\(displayMatchIndex)/\(totalMatches)건")
            .font(.system(size: 11, weight: .semibold, design: .monospaced))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
            .fixedSize(horizontal: true, vertical: false)

          FlatToggleControl(
            title: "대소문자",
            isOn: $viewModel.isFindCaseSensitive,
            symbolName: "textformat",
            palette: palette
          )

          Button {
            viewModel.findPreviousMatch()
          } label: {
            Text("◀ 이전")
              .font(.system(size: 11, weight: .medium))
              .foregroundStyle(palette.primaryText.swiftUIColor)
          }
          .buttonStyle(.plain)
          .fixedSize(horizontal: true, vertical: false)

          Button {
            viewModel.findNextMatch()
          } label: {
            Text("다음 ▶")
              .font(.system(size: 11, weight: .medium))
              .foregroundStyle(palette.primaryText.swiftUIColor)
          }
          .buttonStyle(.plain)
          .fixedSize(horizontal: true, vertical: false)

          Divider().frame(height: 14)

          FlatUnderlineTextField(
            placeholder: "바꿀 내용...",
            text: $viewModel.replaceQueryText,
            symbolName: "arrow.triangle.2.circlepath",
            width: 145,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )

          Button {
            _ = viewModel.replaceCurrentMatch()
          } label: {
            Text("바꾸기")
              .font(.system(size: 11, weight: .medium))
              .foregroundStyle(palette.primaryText.swiftUIColor)
          }
          .buttonStyle(.plain)
          .fixedSize(horizontal: true, vertical: false)

          Button {
            viewModel.executeFindAndReplaceInActiveTab()
          } label: {
            Text("모두 바꾸기")
              .font(.system(size: 11, weight: .bold))
              .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
          }
          .buttonStyle(.plain)
          .fixedSize(horizontal: true, vertical: false)

          Text("\(viewModel.lastFindReplaceCount)건 변경됨")
            .font(.system(size: 11, weight: .medium, design: .monospaced))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
            .fixedSize(horizontal: true, vertical: false)
        }
      }

      Spacer(minLength: 4)

      Button {
        viewModel.isFindReplaceBarVisible = false
      } label: {
        Image(systemName: "xmark")
          .font(.system(size: 11, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
      }
      .buttonStyle(.plain)
    }
    .padding(.horizontal, 14)
    .padding(.vertical, 6)
    .background(palette.elevatedSurface.swiftUIColor)
  }
}

private struct SlideMiniaturePreviewCard: View {
  let slide: SlideModel
  let index: Int
  let isSelected: Bool
  let deckTheme: SlideDeckTheme
  let palette: WorkspaceThemePalette

  var body: some View {
    let themeFillColor = EasyOfficeDesignTokens.resolveHexColor(
      deckTheme.primaryFillHex,
      fallback: palette.elevatedSurface.swiftUIColor
    )
    let themeTextColor = EasyOfficeDesignTokens.contrastingTextColor(
      forFillHex: deckTheme.primaryFillHex
    )
    let slideAccent = EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor

    VStack(alignment: .leading, spacing: 4) {
      VStack(alignment: .leading, spacing: 4) {
        HStack(spacing: 5) {
          Text("\(index + 1)")
            .font(.system(size: 9, weight: .bold, design: .monospaced))
            .foregroundStyle(isSelected ? slideAccent : themeTextColor.opacity(0.75))

          Text(slide.title)
            .font(.system(size: 10, weight: isSelected ? .bold : .semibold))
            .foregroundStyle(themeTextColor)
            .lineLimit(1)

          Spacer(minLength: 2)
        }

        ForEach(Array(slide.bulletPoints.prefix(2).enumerated()), id: \.offset) { _, bullet in
          HStack(spacing: 4) {
            Circle()
              .fill(slideAccent)
              .frame(width: 4, height: 4)
            Text(bullet)
              .font(.system(size: 8))
              .foregroundStyle(themeTextColor.opacity(0.82))
              .lineLimit(1)
          }
        }

        Spacer(minLength: 0)
      }
      .padding(8)
      .frame(maxWidth: .infinity, alignment: .topLeading)
      .aspectRatio(16.0 / 9.0, contentMode: .fit)
      .background(themeFillColor)
      .border(
        isSelected ? slideAccent : palette.hairlineBorder,
        width: isSelected ? 2.0 : 1.0
      )

      HStack(spacing: 4) {
        Text(slide.masterLayout.rawValue)
          .font(.system(size: 8, weight: .medium))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
          .lineLimit(1)
        Spacer(minLength: 2)
        Text(
          "\(slide.transition.rawValue) · \(String(format: "%.1f", slide.transitionDurationSeconds))s"
        )
        .font(.system(size: 8, weight: .semibold, design: .monospaced))
        .foregroundStyle(slideAccent)
        .lineLimit(1)
      }
      .padding(.horizontal, 2)
    }
  }
}

private struct PDFOutlineNodeRowView: View {
  @Bindable var viewModel: WorkspaceViewModel
  let node: PDFOutlineNode
  let pageCount: Int
  let depth: Int
  let palette: WorkspaceThemePalette

  var body: some View {
    VStack(alignment: .leading, spacing: 4) {
      HStack(spacing: 6) {
        Button {
          let targetIdx = max(0, min(pageCount - 1, node.targetPageNumber - 1))
          viewModel.selectedPDFPageIndex = targetIdx
        } label: {
          Text("\(depth == 0 ? "•" : "◦") \(node.title) (p.\(node.targetPageNumber))")
            .font(.system(size: 11, weight: depth == 0 ? .medium : .regular))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
            .lineLimit(1)
        }
        .buttonStyle(.plain)

        Spacer(minLength: 4)

        Button {
          viewModel.removeBookmarkFromActivePDF(id: node.id)
        } label: {
          Image(systemName: "trash")
            .font(.system(size: 9))
            .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
        }
        .buttonStyle(.plain)
        .help("책갈피 삭제")
      }
      .padding(.leading, CGFloat(depth * 12))

      ForEach(node.children) { child in
        PDFOutlineNodeRowView(
          viewModel: viewModel,
          node: child,
          pageCount: pageCount,
          depth: depth + 1,
          palette: palette
        )
      }
    }
  }
}

public struct LeftNavigatorPanelView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  let tab: WorkspaceTabItem

  public var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)

    AdaptiveScrollContainer(
      axes: .vertical,
      showsIndicators: false,
      isSnapshotMode: viewModel.isCanvasSnapshotMode
    ) {
      VStack(alignment: .leading, spacing: 10) {
        let navigatorHeaderTitle: String = {
          switch tab.moduleKind {
          case .document: return "문서 탐색"
          case .hwp: return "한글 문서 탐색"
          case .sheet: return "시트 탐색"
          case .slide: return "슬라이드 탐색"
          case .pdf: return "PDF 탐색"
          }
        }()
        Text(navigatorHeaderTitle)
          .font(.system(size: 11, weight: .bold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)

        switch tab.payload {
        case .document(let doc):
          ForEach(doc.extractOutline()) { heading in
            Button {
              viewModel.selectedParagraphID = heading.id
            } label: {
              Text("• \(heading.text)")
                .font(
                  .system(
                    size: 12,
                    weight: viewModel.selectedParagraphID == heading.id ? .bold : .medium
                  )
                )
                .foregroundStyle(
                  viewModel.selectedParagraphID == heading.id
                    ? EasyOfficeDesignTokens.brand700.swiftUIColor
                    : palette.primaryText.swiftUIColor
                )
                .lineLimit(2)
                .multilineTextAlignment(.leading)
            }
            .buttonStyle(.plain)
          }
        case .sheet(let sheet):
          let formulaCells = sheet.cellsByCoordinate
            .filter { $0.value.trimmingCharacters(in: .whitespaces).hasPrefix("=") }
            .keys.sorted().map { CellAddress(packed: $0).a1Notation }

          Text("워크북 시트 (\(sheet.allSheetNames.count))")
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(palette.primaryText.swiftUIColor)

          ForEach(Array(sheet.allSheetNames.enumerated()), id: \.offset) { idx, sheetName in
            let isSelectedSheet = idx == sheet.activeSheetIndex
            Button {
              viewModel.selectWorksheetTab(index: idx)
            } label: {
              HStack(spacing: 6) {
                Image(systemName: isSelectedSheet ? "tablecells.fill" : "tablecells")
                  .font(.system(size: 10))
                  .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)
                Text(sheetName)
                  .font(.system(size: 12, weight: isSelectedSheet ? .bold : .regular))
                  .foregroundStyle(
                    isSelectedSheet
                      ? EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor
                      : palette.primaryText.swiftUIColor
                  )
                Spacer()
              }
              .padding(.vertical, 3)
            }
            .buttonStyle(.plain)
          }

          Divider()
          Text("수식 셀 (\(formulaCells.count))")
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(palette.primaryText.swiftUIColor)

          if formulaCells.isEmpty {
            Text("없음")
              .font(.system(size: 11))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
          } else {
            ForEach(formulaCells, id: \.self) { addr in
              let isSelectedCell = viewModel.selectedSheetCell == addr
              Button {
                viewModel.selectSheetCellOrRange(primaryCell: addr)
              } label: {
                HStack(spacing: 6) {
                  Text(addr)
                    .font(.system(size: 11, weight: .bold, design: .monospaced))
                    .foregroundStyle(
                      isSelectedCell
                        ? EasyOfficeDesignTokens.brand700.swiftUIColor
                        : palette.primaryText.swiftUIColor
                    )
                  Text(sheet.rawValue(at: addr))
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                    .lineLimit(1)
                  Spacer()
                }
                .padding(.vertical, 2)
              }
              .buttonStyle(.plain)
            }
          }
        case .slide(let deck):
          HStack(spacing: 10) {
            Button {
              viewModel.addSlideToActiveDeck()
            } label: {
              Text("+ 추가")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
            }
            .buttonStyle(.plain)

            Button {
              viewModel.duplicateActiveSlide()
            } label: {
              Text("복제")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(palette.primaryText.swiftUIColor)
            }
            .buttonStyle(.plain)

            Spacer()

            Button {
              viewModel.moveActiveSlide(offset: -1)
            } label: {
              Text("▲")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
            }
            .buttonStyle(.plain)
            .disabled(viewModel.selectedSlideIndex <= 0)

            Button {
              viewModel.moveActiveSlide(offset: 1)
            } label: {
              Text("▼")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
            }
            .buttonStyle(.plain)
            .disabled(viewModel.selectedSlideIndex + 1 >= deck.slides.count)
          }

          ForEach(Array(deck.slides.enumerated()), id: \.element.id) { idx, slide in
            let isSelectedSlide = viewModel.selectedSlideIndex == idx
            Button {
              viewModel.selectedSlideIndex = idx
              viewModel.selectSlideElement(id: slide.elements.first?.id)
            } label: {
              SlideMiniaturePreviewCard(
                slide: slide,
                index: idx,
                isSelected: isSelectedSlide,
                deckTheme: deck.deckTheme,
                palette: palette
              )
            }
            .buttonStyle(.plain)
          }
        case .pdf(let pdf):
          Text("페이지 (\(pdf.pages.count)쪽)")
            .font(.system(size: 10, weight: .bold))
            .foregroundStyle(palette.secondaryText.swiftUIColor)

          ForEach(Array(pdf.pages.enumerated()), id: \.element.id) { idx, page in
            let isSelectedPage = viewModel.selectedPDFPageIndex == idx
            let pageAnnotations = pdf.annotations.filter { $0.pageNumber == page.pageNumber }
            let pageFields = pdf.formFields.filter { $0.pageNumber == page.pageNumber }
            let hasSignature = pdf.inkStrokes.contains(where: { $0.pageNumber == page.pageNumber })
            let pdfAccent = EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor

            Button {
              viewModel.selectedPDFPageIndex = idx
            } label: {
              HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 4) {
                  Rectangle()
                    .fill(EasyOfficeDesignTokens.brand700.swiftUIColor)
                    .frame(height: 4)

                  ForEach(0..<4, id: \.self) { lineIdx in
                    Rectangle()
                      .fill(Color.gray.opacity(0.35))
                      .frame(width: lineIdx == 3 ? 20 : 30, height: 2)
                  }

                  Spacer(minLength: 2)

                  if !pageAnnotations.isEmpty {
                    Circle()
                      .fill(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
                      .frame(width: 5, height: 5)
                  }
                }
                .padding(5)
                .frame(width: 42, height: 56, alignment: .topLeading)
                .background(Color.white)
                .border(
                  isSelectedPage ? pdfAccent : palette.hairlineBorder,
                  width: isSelectedPage ? 1.8 : 1.0
                )

                VStack(alignment: .leading, spacing: 3) {
                  Text("\(page.pageNumber)쪽 · \(page.title)")
                    .font(.system(size: 11, weight: isSelectedPage ? .bold : .semibold))
                    .foregroundStyle(
                      isSelectedPage ? pdfAccent : palette.primaryText.swiftUIColor
                    )
                    .lineLimit(1)

                  Text("주석 \(pageAnnotations.count)건 · 양식 \(pageFields.count)건")
                    .font(.system(size: 9, design: .monospaced))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                    .lineLimit(1)

                  Text("회전 \(page.rotationDegrees)°\(hasSignature ? " · 서명됨" : "")")
                    .font(.system(size: 9, weight: .medium))
                    .foregroundStyle(pdfAccent)
                    .lineLimit(1)
                }
                Spacer(minLength: 0)
              }
              .padding(.vertical, 4)
            }
            .buttonStyle(.plain)
          }

          if !pdf.outline.isEmpty {
            Divider()
            Text("문서 책갈피")
              .font(.system(size: 10, weight: .bold))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
            ForEach(pdf.outline) { node in
              PDFOutlineNodeRowView(
                viewModel: viewModel,
                node: node,
                pageCount: pdf.pages.count,
                depth: 0,
                palette: palette
              )
            }
          }
        }
        Spacer(minLength: 0)
      }
      .padding(12)
    }
    .background(palette.cardSurface.swiftUIColor)
  }
}

struct ActiveEditorCanvasRouterView: View {
  @Bindable var viewModel: WorkspaceViewModel
  let tab: WorkspaceTabItem

  var body: some View {
    HStack(spacing: 0) {
      EditorCanvasSinglePaneView(viewModel: viewModel, tab: tab, isPrimaryPane: true)
        .frame(
          minWidth: EasyOfficeDesignTokens.LayoutMetrics.splitPaneMinimumWidth, maxWidth: .infinity,
          maxHeight: .infinity
        )
        .accessibilityLabel(
          AccessibilityTokens.splitViewPaneLabel(isPrimary: true, fileName: tab.fileName))

      if let secondaryTab = viewModel.splitSecondaryTab {
        Rectangle()
          .fill(EasyOfficeDesignTokens.brand400.swiftUIColor)
          .frame(width: 2)

        EditorCanvasSinglePaneView(viewModel: viewModel, tab: secondaryTab, isPrimaryPane: false)
          .frame(
            minWidth: EasyOfficeDesignTokens.LayoutMetrics.splitPaneMinimumWidth,
            maxWidth: .infinity, maxHeight: .infinity
          )
          .accessibilityLabel(
            AccessibilityTokens.splitViewPaneLabel(
              isPrimary: false, fileName: secondaryTab.fileName))
      }
    }
  }
}

typealias CentralEditorCanvasView = EditorCanvasSinglePaneView

struct EditorCanvasSinglePaneView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  let tab: WorkspaceTabItem
  var isPrimaryPane: Bool = true

  var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
    let isReadOnly = viewModel.workspaceViewMode == .readOnlyView

    VStack(alignment: .leading, spacing: 6) {
      if viewModel.workspaceViewMode != .editView {
        HStack(spacing: 6) {
          Image(systemName: viewModel.workspaceViewMode.symbolName)
            .font(.system(size: 11, weight: .semibold))
          Text(
            isReadOnly
              ? "읽기 전용 모드 · 문서 수정이 잠겨 있습니다"
              : "윤곽 보기 모드 · 문단 계층 및 개요 중심 표시"
          )
          .font(.system(size: 11, weight: .semibold))
          Spacer()
          Button {
            viewModel.setWorkspaceViewMode(.editView)
          } label: {
            Text("편집 전환")
              .font(.system(size: 11, weight: .semibold))
              .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
          }
          .buttonStyle(.plain)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 4)
        .background(palette.elevatedSurface.swiftUIColor)
      }

      Group {
        switch tab.payload {
        case .document(let doc):
          DocumentEditorDetailPaneView(
            viewModel: viewModel, document: doc, isPrimaryPane: isPrimaryPane)
        case .sheet(let sheet):
          SheetEditorDetailPaneView(
            viewModel: viewModel, sheet: sheet, isPrimaryPane: isPrimaryPane)
        case .slide(let deck):
          SlideEditorDetailPaneView(
            viewModel: viewModel, deck: deck, isPrimaryPane: isPrimaryPane)
        case .pdf(let pdf):
          PDFStudioDetailPaneView(viewModel: viewModel, pdf: pdf, isPrimaryPane: isPrimaryPane)
        }
      }
      .disabled(isReadOnly)
    }
    .padding(.horizontal, 16)
    .padding(.vertical, 10)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
  }
}

typealias BottomStatusBarView = BottomWorkspaceStatusBarView

struct BottomWorkspaceStatusBarView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  let tab: WorkspaceTabItem

  var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
    let cleanModuleStatusTitle: String = {
      switch tab.moduleKind {
      case .document: return "문서"
      case .hwp: return "한글 문서"
      case .sheet: return "스프레드시트"
      case .slide: return "프레젠테이션"
      case .pdf: return "PDF"
      }
    }()

    HStack(spacing: 10) {
      AdaptiveScrollContainer(
        axes: .horizontal,
        showsIndicators: false,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      ) {
        HStack(spacing: 10) {
          Text(cleanModuleStatusTitle)
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
            .fixedSize(horizontal: true, vertical: false)

          Divider().frame(height: 14)

          switch tab.payload {
          case .document(let doc):
            let words = doc.wordCount
            let chars = doc.characterCountIncludingSpaces
            let manuscriptPages = max(1, (chars + 199) / 200)
            let readingMinutes = max(1, (words + 199) / 200)
            Text(
              "쪽 1/\(doc.estimatedPageCount) · 단어 \(words) · 글자 \(chars)자 · 원고지 \(manuscriptPages)매 · 읽기 약 \(readingMinutes)분"
            )
            .font(.system(size: 11, weight: .medium, design: .monospaced))
            .foregroundStyle(palette.primaryText.swiftUIColor)
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
            Divider().frame(height: 14)
            Text("스플릿 뷰: \(secondary.fileName)")
              .font(.system(size: 11, weight: .semibold))
              .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
              .fixedSize(horizontal: true, vertical: false)
          }

          if let errorMessage = viewModel.lastErrorMessage {
            Divider().frame(height: 14)
            Text("오류: \(errorMessage)")
              .font(.system(size: 11, weight: .semibold))
              .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
              .lineLimit(1)
          } else if let summary = viewModel.lastExecutedCommandSummary, !summary.isEmpty {
            Divider().frame(height: 14)
            Text(summary)
              .font(.system(size: 11, weight: .medium))
              .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
              .lineLimit(1)
          }
        }
      }

      Spacer(minLength: 8)

      HStack(spacing: 8) {
        ForEach(WorkspaceViewMode.allCases) { mode in
          let isSelectedMode = viewModel.workspaceViewMode == mode
          Button {
            viewModel.setWorkspaceViewMode(mode)
          } label: {
            HStack(spacing: 3) {
              Image(systemName: mode.symbolName)
                .font(.system(size: 9, weight: .semibold))
              Text(mode.rawValue)
                .font(.system(size: 9, weight: isSelectedMode ? .bold : .medium))
            }
            .foregroundStyle(
              isSelectedMode
                ? EasyOfficeDesignTokens.brand700.swiftUIColor
                : palette.secondaryText.swiftUIColor
            )
            .padding(.horizontal, 4)
            .padding(.vertical, 2)
          }
          .buttonStyle(.plain)
          .help(mode.rawValue)
        }

        Divider().frame(height: 14)

        Button("-") { viewModel.setZoom(viewModel.zoomPercent - 25.0) }.buttonStyle(.plain)
        Text("배율 \(Int(viewModel.zoomPercent))%")
          .font(.system(size: 11, design: .monospaced))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        Button("+") { viewModel.setZoom(viewModel.zoomPercent + 25.0) }.buttonStyle(.plain)

        ForEach([75, 100, 125], id: \.self) { preset in
          let isCurrentZoom = Int(viewModel.zoomPercent) == preset
          Button {
            viewModel.applyQuickZoomPreset(Double(preset))
          } label: {
            Text("\(preset)%")
              .font(.system(size: 10, weight: isCurrentZoom ? .bold : .medium, design: .monospaced))
              .foregroundStyle(
                isCurrentZoom
                  ? EasyOfficeDesignTokens.brand700.swiftUIColor
                  : palette.secondaryText.swiftUIColor
              )
              .padding(.horizontal, 3)
              .padding(.vertical, 2)
          }
          .buttonStyle(.plain)
          .help("\(preset)% 배율 프리셋")
        }
      }
      .fixedSize(horizontal: true, vertical: false)
    }
    .padding(.horizontal, 14)
    .frame(height: EasyOfficeDesignTokens.LayoutMetrics.statusBarHeight)
    .background(palette.cardSurface.swiftUIColor)
  }
}

