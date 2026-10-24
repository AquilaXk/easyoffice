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
        HStack(spacing: 0) {
          content()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .clipped()
      } else {
        GeometryReader { geo in
          VStack(alignment: .leading, spacing: 0) {
            content()
          }
          .frame(width: geo.size.width, alignment: .topLeading)
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

struct LavenderMenuPill<MenuContent: View>: View {
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

  private var pillBadge: some View {
    HStack(spacing: 5) {
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
    .padding(.horizontal, 8)
    .padding(.vertical, 5)
    .frame(minWidth: minWidth)
    .background(palette.elevatedSurface.swiftUIColor)
    .overlay(
      RoundedRectangle(cornerRadius: 6)
        .stroke(palette.border.swiftUIColor, lineWidth: 1)
    )
    .clipShape(RoundedRectangle(cornerRadius: 6))
  }

  var body: some View {
    if isSnapshotMode || envSnapshotMode {
      pillBadge
        .fixedSize(horizontal: minWidth == nil, vertical: false)
    } else {
      Menu {
        menuContent()
      } label: {
        pillBadge
      }
      .menuStyle(.borderlessButton)
      .buttonStyle(.plain)
      .fixedSize(horizontal: minWidth == nil, vertical: false)
    }
  }
}

struct LavenderTextFieldPill: View {
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
    HStack(spacing: 5) {
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
    .padding(.horizontal, 8)
    .padding(.vertical, 5)
    .frame(width: width)
    .frame(maxWidth: width == nil ? .infinity : nil, alignment: .leading)
    .background(palette.cardSurface.swiftUIColor)
    .overlay(
      RoundedRectangle(cornerRadius: 6)
        .stroke(palette.border.swiftUIColor, lineWidth: 1)
    )
    .clipShape(RoundedRectangle(cornerRadius: 6))
  }
}

struct LavenderTogglePill: View {
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
      HStack(spacing: 5) {
        Image(systemName: isOn ? "checkmark.circle.fill" : (symbolName ?? "circle"))
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(isOn ? accent : palette.secondaryText.swiftUIColor)
        Text(title)
          .font(.system(size: 11, weight: isOn ? .semibold : .medium))
          .foregroundStyle(
            isOn ? palette.accentBadgeText.swiftUIColor : palette.primaryText.swiftUIColor
          )
          .lineLimit(1)
      }
      .padding(.horizontal, 8)
      .padding(.vertical, 5)
      .background(
        isOn ? palette.selectedSurface.swiftUIColor : palette.elevatedSurface.swiftUIColor
      )
      .overlay(
        RoundedRectangle(cornerRadius: 6)
          .stroke(
            isOn ? accent.opacity(0.75) : palette.border.swiftUIColor,
            lineWidth: isOn ? 1.2 : 1.0
          )
      )
      .clipShape(RoundedRectangle(cornerRadius: 6))
    }
    .buttonStyle(.plain)
    .fixedSize(horizontal: true, vertical: false)
  }
}

struct LavenderStepperPill: View {
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
          .frame(width: 18, height: 18)
          .background(palette.cardSurface.swiftUIColor)
          .clipShape(RoundedRectangle(cornerRadius: 4))
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
          .frame(width: 18, height: 18)
          .background(palette.cardSurface.swiftUIColor)
          .clipShape(RoundedRectangle(cornerRadius: 4))
      }
      .buttonStyle(.plain)
    }
    .padding(.horizontal, 6)
    .padding(.vertical, 3)
    .background(palette.elevatedSurface.swiftUIColor)
    .overlay(
      RoundedRectangle(cornerRadius: 6)
        .stroke(palette.border.swiftUIColor, lineWidth: 1)
    )
    .clipShape(RoundedRectangle(cornerRadius: 6))
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
        .font(.system(size: 11, weight: .semibold))
        .foregroundStyle(
          isActive
            ? Color.white
            : (isDisabled
              ? palette.secondaryText.swiftUIColor.opacity(0.45)
              : palette.primaryText.swiftUIColor)
        )
        .frame(width: 24, height: 24)
        .background(
          isActive
            ? EasyOfficeDesignTokens.brand700.swiftUIColor
            : palette.cardSurface.swiftUIColor
        )
        .overlay(
          RoundedRectangle(cornerRadius: 5)
            .stroke(palette.border.swiftUIColor, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 5))
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
        HStack(spacing: 6) {
          Image(systemName: "square.grid.2x2.fill")
          Text("홈 허브").font(.system(size: 12, weight: .semibold))
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 5)
        .background(
          viewModel.isShowingHomeHub
            ? EasyOfficeDesignTokens.brand700.swiftUIColor
            : palette.selectedSurface.swiftUIColor
        )
        .foregroundStyle(
          viewModel.isShowingHomeHub ? Color.white : palette.accentBadgeText.swiftUIColor
        )
        .clipShape(RoundedRectangle(cornerRadius: 6))
      }
      .buttonStyle(.plain)
      .fixedSize(horizontal: true, vertical: false)

      HStack(spacing: 3) {
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

      Divider().frame(height: 18)

      AdaptiveScrollContainer(
        axes: .horizontal,
        showsIndicators: false,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      ) {
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
                .lineLimit(1)

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
            .padding(.horizontal, 7)
            .padding(.vertical, 5)
            .background(
              isSelected ? palette.cardSurface.swiftUIColor : palette.elevatedSurface.swiftUIColor
            )
            .overlay(
              RoundedRectangle(cornerRadius: 6)
                .stroke(
                  isSelected
                    ? EasyOfficeDesignTokens.brand700.swiftUIColor
                    : (isSecondarySplit
                      ? EasyOfficeDesignTokens.brand400.swiftUIColor : palette.border.swiftUIColor),
                  lineWidth: isSelected || isSecondarySplit ? 1.5 : 1.0
                )
            )
            .clipShape(RoundedRectangle(cornerRadius: 6))
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
            }
          }

          Button {
            viewModel.openNewSampleTab(for: .document)
          } label: {
            Image(systemName: "plus")
              .font(.system(size: 10, weight: .bold))
              .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
              .frame(width: 24, height: 24)
              .background(palette.cardSurface.swiftUIColor)
              .overlay(
                RoundedRectangle(cornerRadius: 5)
                  .stroke(palette.border.swiftUIColor, lineWidth: 1)
              )
              .clipShape(RoundedRectangle(cornerRadius: 5))
          }
          .buttonStyle(.plain)
          .help("새 문서 탭 생성")
        }
      }

      Spacer(minLength: 6)

      LavenderMenuPill(
        title: viewModel.themeMode.rawValue,
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

      LavenderTextFieldPill(
        placeholder: "명령·본문 통합 검색",
        text: $viewModel.searchQuery,
        symbolName: "magnifyingglass",
        width: 132,
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
          HStack(spacing: 8) {
            ForEach(RibbonCategory.allCases) { category in
              let isSelected = viewModel.activeRibbonCategory == category
              Button {
                viewModel.activeRibbonCategory = category
              } label: {
                VStack(spacing: 2) {
                  Text(viewModel.ribbonLayoutStyle.displayTitle(for: category))
                    .font(.system(size: 12, weight: isSelected ? .bold : .medium))
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
                    .foregroundStyle(
                      isSelected
                        ? palette.accentBadgeText.swiftUIColor
                        : palette.secondaryText.swiftUIColor
                    )
                    .padding(.vertical, 3)
                    .padding(.horizontal, 8)
                    .background(isSelected ? palette.selectedSurface.swiftUIColor : Color.clear)
                    .clipShape(RoundedRectangle(cornerRadius: 5))

                  RoundedRectangle(cornerRadius: 1.5)
                    .fill(isSelected ? moduleAccent : Color.clear)
                    .frame(height: 2.5)
                }
              }
              .buttonStyle(.plain)
            }
          }
        }

        Spacer(minLength: 8)

        HStack(spacing: 6) {
          Text(viewModel.activeContextualObjectToolBadge)
            .font(.system(size: 10, weight: .bold))
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(moduleAccent.opacity(0.14))
            .foregroundStyle(moduleAccent)
            .overlay(
              RoundedRectangle(cornerRadius: 5)
                .stroke(moduleAccent.opacity(0.45), lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: 5))

          Button {
            viewModel.toggleRibbonLayoutStyle()
          } label: {
            Label(viewModel.ribbonLayoutStyle.rawValue, systemImage: "slider.horizontal.3")
              .font(.system(size: 11, weight: .semibold))
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
              .padding(.horizontal, 7)
              .padding(.vertical, 3)
              .background(palette.selectedSurface.swiftUIColor)
              .foregroundStyle(palette.accentBadgeText.swiftUIColor)
              .clipShape(RoundedRectangle(cornerRadius: 5))
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
            }
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
            }
          }

          Button {
            viewModel.showLeftNavigator.toggle()
          } label: {
            Label("내비게이터", systemImage: "sidebar.left")
              .font(.system(size: 11))
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
          }

          Button {
            viewModel.showRightInspector.toggle()
          } label: {
            Label("인스펙터", systemImage: "sidebar.right")
              .font(.system(size: 11))
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
          }

          Button {
            viewModel.isRibbonCollapsed.toggle()
          } label: {
            Image(systemName: viewModel.isRibbonCollapsed ? "chevron.down" : "chevron.up")
              .font(.system(size: 11, weight: .semibold))
          }
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
          HStack(alignment: .bottom, spacing: 8) {
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
                  palette: palette
                ) {
                  viewModel.isFindReplaceBarPresented.toggle()
                }
              }
            }

            Divider().frame(height: 36)

            ribbonGroupContainer(title: "핵심 리본 도구", palette: palette) {
              HStack(spacing: 5) {
                ForEach(primaryActions) { command in
                  Button {
                    viewModel.executeRibbonCommand(command.id)
                  } label: {
                    HStack(spacing: 5) {
                      Image(systemName: command.symbolName)
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(moduleAccent)
                      Text(command.title)
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(palette.primaryText.swiftUIColor)
                      Text(command.shortcutHint)
                        .font(.system(size: 9, design: .monospaced))
                        .foregroundStyle(palette.secondaryText.swiftUIColor)
                    }
                    .padding(.horizontal, 8)
                    .padding(.vertical, 5)
                    .background(palette.selectedSurface.swiftUIColor)
                    .overlay(
                      RoundedRectangle(cornerRadius: 6)
                        .stroke(moduleAccent.opacity(0.45), lineWidth: 1)
                    )
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                  }
                  .buttonStyle(.plain)
                  .fixedSize(horizontal: true, vertical: false)
                  .accessibilityHint(AccessibilityTokens.ribbonControlHint)
                }
              }
            }

            if !secondaryActions.isEmpty {
              Divider().frame(height: 36)

              ribbonGroupContainer(title: "서식·문서 구조", palette: palette) {
                HStack(spacing: 5) {
                  ForEach(secondaryActions) { command in
                    Button {
                      viewModel.executeRibbonCommand(command.id)
                    } label: {
                      HStack(spacing: 5) {
                        Image(systemName: command.symbolName)
                          .font(.system(size: 11))
                          .foregroundStyle(moduleAccent)
                        Text(command.title)
                          .font(.system(size: 11, weight: .medium))
                          .foregroundStyle(palette.primaryText.swiftUIColor)
                        Text(command.shortcutHint)
                          .font(.system(size: 9, design: .monospaced))
                          .foregroundStyle(palette.secondaryText.swiftUIColor)
                      }
                      .padding(.horizontal, 8)
                      .padding(.vertical, 5)
                      .background(palette.elevatedSurface.swiftUIColor)
                      .overlay(
                        RoundedRectangle(cornerRadius: 6)
                          .stroke(palette.border.swiftUIColor, lineWidth: 1)
                      )
                      .clipShape(RoundedRectangle(cornerRadius: 6))
                    }
                    .buttonStyle(.plain)
                    .fixedSize(horizontal: true, vertical: false)
                    .accessibilityHint(AccessibilityTokens.ribbonControlHint)
                  }
                }
              }
            }

            Divider().frame(height: 36)

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

  @ViewBuilder
  private func ribbonGroupContainer<Content: View>(
    title: String,
    palette: WorkspaceThemePalette,
    @ViewBuilder content: () -> Content
  ) -> some View {
    VStack(alignment: .center, spacing: 3) {
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
    action: @escaping () -> Void
  ) -> some View {
    Button(action: action) {
      HStack(spacing: 4) {
        Image(systemName: symbolName)
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(accent)
        Text(title)
          .font(.system(size: 11, weight: .medium))
          .foregroundStyle(palette.primaryText.swiftUIColor)
      }
      .padding(.horizontal, 8)
      .padding(.vertical, 5)
      .background(palette.elevatedSurface.swiftUIColor)
      .overlay(
        RoundedRectangle(cornerRadius: 5)
          .stroke(palette.border.swiftUIColor, lineWidth: 1)
      )
      .clipShape(RoundedRectangle(cornerRadius: 5))
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
      if viewModel.ribbonLayoutStyle == .hangulHwp {
        ribbonQuickButton(
          title: "200자 원고지",
          symbolName: "square.grid.3x3",
          accent: accent,
          palette: palette
        ) {
          viewModel.toggleManuscriptGridMode()
        }
        ribbonQuickButton(
          title: "다단 전환",
          symbolName: "rectangle.split.2x1",
          accent: accent,
          palette: palette
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
          viewModel.executeRibbonCommand(
            activeModule == .hwp ? "hwp-insert-footnote" : "doc-insert-footnote"
          )
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
          palette: palette
        ) {
          viewModel.toggleHighlightOnActiveParagraph()
        }
        ribbonQuickButton(
          title: "취소선",
          symbolName: "strikethrough",
          accent: accent,
          palette: palette
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
      ribbonQuickButton(
        title: "₩ 통화",
        symbolName: "wonsign.circle",
        accent: accent,
        palette: palette
      ) {
        viewModel.applyQuickNumberFormatOnSelectedSheetCell(.currencyKRW)
      }
      ribbonQuickButton(
        title: "% 백분율",
        symbolName: "percent",
        accent: accent,
        palette: palette
      ) {
        viewModel.applyQuickNumberFormatOnSelectedSheetCell(.percentage)
      }
      ribbonQuickButton(
        title: "줄바꿈",
        symbolName: "text.word.spacing",
        accent: accent,
        palette: palette
      ) {
        viewModel.toggleWrapTextOnSelectedSheetCell()
      }
      ribbonQuickButton(
        title: "A열 고정",
        symbolName: "rectangle.split.2x1",
        accent: accent,
        palette: palette
      ) {
        viewModel.toggleFirstColumnFrozen()
      }
      ribbonQuickButton(
        title: "데이터 막대",
        symbolName: "chart.bar.xaxis",
        accent: accent,
        palette: palette
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
        palette: palette
      ) {
        viewModel.toggleSlideSorterView()
      }

    case .pdf:
      ribbonQuickButton(
        title: "워터마크",
        symbolName: "drop.triangle",
        accent: accent,
        palette: palette
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
        palette: palette
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
    HStack(spacing: 8) {
      AdaptiveScrollContainer(
        axes: .horizontal,
        showsIndicators: false,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      ) {
        HStack(spacing: 8) {
          Label("찾기 및 바꾸기", systemImage: "magnifyingglass.circle.fill")
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
            .fixedSize(horizontal: true, vertical: false)

          LavenderTextFieldPill(
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
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .background(palette.selectedSurface.swiftUIColor)
            .foregroundStyle(palette.accentBadgeText.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 4))
            .fixedSize(horizontal: true, vertical: false)

          LavenderTogglePill(
            title: "대소문자",
            isOn: $viewModel.isFindCaseSensitive,
            symbolName: "textformat",
            palette: palette
          )

          Button("◀ 이전") { viewModel.findPreviousMatch() }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .fixedSize(horizontal: true, vertical: false)

          Button("다음 ▶") { viewModel.findNextMatch() }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .fixedSize(horizontal: true, vertical: false)

          Divider().frame(height: 16)

          LavenderTextFieldPill(
            placeholder: "바꿀 내용...",
            text: $viewModel.replaceQueryText,
            symbolName: "arrow.triangle.2.circlepath",
            width: 145,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )

          Button("바꾸기(1건)") { _ = viewModel.replaceCurrentMatch() }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .fixedSize(horizontal: true, vertical: false)

          Button("모두 바꾸기") { viewModel.executeFindAndReplaceInActiveTab() }
            .buttonStyle(.borderedProminent)
            .controlSize(.small)
            .tint(EasyOfficeDesignTokens.brand700.swiftUIColor)
            .fixedSize(horizontal: true, vertical: false)

          Text("\(viewModel.lastFindReplaceCount)건 변경됨")
            .font(.system(size: 11, weight: .semibold, design: .monospaced))
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .background(palette.selectedSurface.swiftUIColor)
            .foregroundStyle(palette.accentBadgeText.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 4))
            .fixedSize(horizontal: true, vertical: false)
        }
      }

      Spacer(minLength: 4)

      Button {
        viewModel.isFindReplaceBarVisible = false
      } label: {
        Image(systemName: "xmark.circle.fill")
          .font(.system(size: 12))
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
            .foregroundStyle(Color.white)
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(isSelected ? slideAccent : palette.secondaryText.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 3))

          Text(slide.title)
            .font(.system(size: 10, weight: isSelected ? .bold : .semibold))
            .foregroundStyle(themeTextColor)
            .lineLimit(1)

          Spacer(minLength: 2)

          Text("\(slide.elements.count)개")
            .font(.system(size: 8, weight: .semibold, design: .monospaced))
            .foregroundStyle(themeTextColor.opacity(0.85))
            .padding(.horizontal, 4)
            .padding(.vertical, 1)
            .background(themeTextColor.opacity(0.14))
            .clipShape(RoundedRectangle(cornerRadius: 3))
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
      .overlay(
        RoundedRectangle(cornerRadius: 6)
          .stroke(
            isSelected ? slideAccent : palette.border.swiftUIColor,
            lineWidth: isSelected ? 2.0 : 1.0
          )
      )
      .clipShape(RoundedRectangle(cornerRadius: 6))

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
        Text("\(tab.moduleKind.shortBadgeTitle) 구조 내비게이터")
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
                .foregroundStyle(palette.primaryText.swiftUIColor)
                .lineLimit(2)
                .multilineTextAlignment(.leading)
            }
            .buttonStyle(.plain)
          }
        case .sheet(let sheet):
          let formulaCells = sheet.cellsByCoordinate
            .filter { $0.value.trimmingCharacters(in: .whitespaces).hasPrefix("=") }
            .keys.sorted().map { CellAddress(packed: $0).a1Notation }

          Text("워크북 시트 목록 (\(sheet.allSheetNames.count))")
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
                  .foregroundStyle(palette.primaryText.swiftUIColor)
                Spacer()
              }
              .padding(.horizontal, 6)
              .padding(.vertical, 3)
              .background(
                isSelectedSheet ? palette.selectedSurface.swiftUIColor : Color.clear
              )
              .clipShape(RoundedRectangle(cornerRadius: 4))
            }
            .buttonStyle(.plain)
          }

          Divider()
          Text("내장 함수 카탈로그: \(WorksheetModel.supportedFunctionNames.count)종")
            .font(.system(size: 11))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
          Text("수식 셀 목록 (\(formulaCells.count)개)")
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
                    .foregroundStyle(palette.accentBadgeText.swiftUIColor)
                  Text(sheet.rawValue(at: addr))
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                    .lineLimit(1)
                  Spacer()
                }
                .padding(.horizontal, 6)
                .padding(.vertical, 3)
                .background(
                  isSelectedCell ? palette.selectedSurface.swiftUIColor : Color.clear
                )
                .clipShape(RoundedRectangle(cornerRadius: 4))
              }
              .buttonStyle(.plain)
            }
          }
        case .slide(let deck):
          HStack(spacing: 4) {
            Button("+ 추가") {
              viewModel.addSlideToActiveDeck()
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)

            Button("복제") {
              viewModel.duplicateActiveSlide()
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)

            Button("▲") {
              viewModel.moveActiveSlide(offset: -1)
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)
            .disabled(viewModel.selectedSlideIndex <= 0)

            Button("▼") {
              viewModel.moveActiveSlide(offset: 1)
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)
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
          Text("페이지 미니어처 (\(pdf.pages.count)쪽)")
            .font(.system(size: 10, weight: .bold))
            .foregroundStyle(palette.secondaryText.swiftUIColor)

          ForEach(Array(pdf.pages.enumerated()), id: \.element.id) { idx, page in
            let isSelectedPage = viewModel.selectedPDFPageIndex == idx
            let pageAnnotations = pdf.annotations.filter { $0.pageNumber == page.pageNumber }
            let pageFields = pdf.formFields.filter { $0.pageNumber == page.pageNumber }
            let hasSignature = pdf.inkStrokes.contains(where: { $0.pageNumber == page.pageNumber })

            Button {
              viewModel.selectedPDFPageIndex = idx
            } label: {
              HStack(spacing: 8) {
                ZStack(alignment: .topLeading) {
                  RoundedRectangle(cornerRadius: 4)
                    .fill(Color.white)

                  VStack(alignment: .leading, spacing: 4) {
                    RoundedRectangle(cornerRadius: 1.5)
                      .fill(EasyOfficeDesignTokens.brand700.swiftUIColor)
                      .frame(height: 5)

                    ForEach(0..<4, id: \.self) { lineIdx in
                      RoundedRectangle(cornerRadius: 1)
                        .fill(Color.gray.opacity(0.38))
                        .frame(width: lineIdx == 3 ? 22 : 32, height: 2.5)
                    }

                    Spacer(minLength: 2)

                    HStack(spacing: 4) {
                      if !pageAnnotations.isEmpty {
                        Circle()
                          .fill(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
                          .frame(width: 6, height: 6)
                      }
                      if hasSignature {
                        Path { path in
                          path.move(to: CGPoint(x: 0, y: 5))
                          path.addCurve(
                            to: CGPoint(x: 16, y: 2),
                            control1: CGPoint(x: 5, y: 0),
                            control2: CGPoint(x: 11, y: 7)
                          )
                        }
                        .stroke(EasyOfficeDesignTokens.brand700.swiftUIColor, lineWidth: 1.2)
                        .frame(width: 18, height: 7)
                      }
                    }
                  }
                  .padding(5)
                }
                .frame(width: 44, height: 58)
                .overlay(
                  RoundedRectangle(cornerRadius: 4)
                    .stroke(
                      isSelectedPage
                        ? EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor
                        : palette.border.swiftUIColor,
                      lineWidth: isSelectedPage ? 1.8 : 1.0
                    )
                )
                .clipShape(RoundedRectangle(cornerRadius: 4))

                VStack(alignment: .leading, spacing: 3) {
                  Text("P.\(page.pageNumber) · \(page.title)")
                    .font(.system(size: 11, weight: isSelectedPage ? .bold : .semibold))
                    .foregroundStyle(palette.primaryText.swiftUIColor)
                    .lineLimit(1)

                  Text("주석 \(pageAnnotations.count)건 · 양식 \(pageFields.count)건")
                    .font(.system(size: 9, design: .monospaced))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                    .lineLimit(1)

                  Text("회전 \(page.rotationDegrees)°\(hasSignature ? " · 서명됨" : "")")
                    .font(.system(size: 9, weight: .medium))
                    .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
                    .lineLimit(1)
                }
                Spacer(minLength: 0)
              }
              .padding(6)
              .background(
                isSelectedPage
                  ? palette.selectedSurface.swiftUIColor
                  : palette.elevatedSurface.swiftUIColor
              )
              .clipShape(RoundedRectangle(cornerRadius: 6))
            }
            .buttonStyle(.plain)
          }

          if !pdf.outline.isEmpty {
            Divider()
            Text("문서 책갈피 (Outlines)")
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
  @Bindable var viewModel: WorkspaceViewModel
  let tab: WorkspaceTabItem
  var isPrimaryPane: Bool = true

  var body: some View {
    VStack(alignment: .leading, spacing: 0) {
      switch tab.payload {
      case .document(let doc):
        DocumentEditorDetailPaneView(
          viewModel: viewModel, document: doc, isPrimaryPane: isPrimaryPane)
      case .sheet(let sheet):
        SheetEditorDetailPaneView(
          viewModel: viewModel, sheet: sheet, isPrimaryPane: isPrimaryPane)
      case .slide(let deck):
        SlideEditorDetailPaneView(viewModel: viewModel, deck: deck, isPrimaryPane: isPrimaryPane)
      case .pdf(let pdf):
        PDFStudioDetailPaneView(viewModel: viewModel, pdf: pdf, isPrimaryPane: isPrimaryPane)
      }
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

    HStack(spacing: 10) {
      AdaptiveScrollContainer(
        axes: .horizontal,
        showsIndicators: false,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      ) {
        HStack(spacing: 10) {
          Text("\(tab.moduleKind.displayNameKo) · \(tab.format.shortSpecificationBadge)")
            .font(.system(size: 11))
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
              "시트 \(viewModel.activeSheetPageIndex + 1)/\(max(1, viewModel.activeWorkbookSheetNames.count)) · 범위 \(viewModel.selectedSheetRange) · 수식 \(WorksheetModel.supportedFunctionNames.count)종"
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
              Button("◀") {
                viewModel.selectedPDFPageIndex = max(0, viewModel.selectedPDFPageIndex - 1)
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)
              .disabled(viewModel.selectedPDFPageIndex <= 0)

              Text("페이지 \(viewModel.selectedPDFPageNumber)/\(max(1, pdf.pages.count))")
                .font(.system(size: 11, weight: .semibold, design: .monospaced))
                .foregroundStyle(palette.primaryText.swiftUIColor)

              Button("▶") {
                viewModel.selectedPDFPageIndex = min(
                  max(0, pdf.pages.count - 1),
                  viewModel.selectedPDFPageIndex + 1
                )
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)
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

      HStack(spacing: 4) {
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
                ? Color.white
                : palette.secondaryText.swiftUIColor
            )
            .padding(.horizontal, 5)
            .padding(.vertical, 2)
            .background(
              isSelectedMode
                ? EasyOfficeDesignTokens.brand700.swiftUIColor
                : palette.elevatedSurface.swiftUIColor
            )
            .clipShape(RoundedRectangle(cornerRadius: 4))
          }
          .buttonStyle(.plain)
          .help(mode.rawValue)
        }

        Divider().frame(height: 14)

        Button("-") { viewModel.setZoom(viewModel.zoomPercent - 25.0) }.buttonStyle(.plain)
        Text("배율: \(Int(viewModel.zoomPercent))%")
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
                  ? palette.accentBadgeText.swiftUIColor
                  : palette.secondaryText.swiftUIColor
              )
              .padding(.horizontal, 5)
              .padding(.vertical, 2)
              .background(
                isCurrentZoom
                  ? palette.selectedSurface.swiftUIColor
                  : palette.elevatedSurface.swiftUIColor
              )
              .clipShape(RoundedRectangle(cornerRadius: 4))
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
