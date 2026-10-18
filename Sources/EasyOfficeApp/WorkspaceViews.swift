import AppKit
import EasyOfficeCore
import EasyOfficeDesignSystem
import Observation
import PDFKit
import SwiftUI
import UniformTypeIdentifiers

public typealias WorkspaceFilePanelCoordinator = WorkspaceFileDialogPresenter

extension WorkspaceThemePalette {
  public var primaryAccent: RGBColorToken {
    EasyOfficeDesignTokens.brand700
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
  }
}

struct TopDocumentTabStripView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel

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

      HStack(spacing: 4) {
        Button {
          WorkspaceFileDialogPresenter.presentOpenPanel(viewModel: viewModel)
        } label: {
          Label("열기", systemImage: "folder").font(.system(size: 11, weight: .medium))
        }
        .buttonStyle(.bordered).controlSize(.small)

        Button {
          WorkspaceFileDialogPresenter.presentSavePanel(viewModel: viewModel)
        } label: {
          Label("저장", systemImage: "square.and.arrow.down").font(.system(size: 11, weight: .medium))
        }
        .buttonStyle(.bordered).controlSize(.small)
        .disabled(viewModel.activeTab == nil)

        Button {
          WorkspaceFileDialogPresenter.presentPrintPanel(viewModel: viewModel)
        } label: {
          Label("인쇄 (⌘P)", systemImage: "printer").font(.system(size: 11, weight: .medium))
        }
        .buttonStyle(.bordered).controlSize(.small)
        .disabled(viewModel.activeTab == nil)

        Button {
          viewModel.isFindReplaceBarVisible.toggle()
        } label: {
          Label("찾기 (⌘F)", systemImage: "magnifyingglass").font(.system(size: 11, weight: .medium))
        }
        .buttonStyle(.bordered).controlSize(.small)
        .disabled(viewModel.activeTab == nil)

        Button {
          viewModel.undo()
        } label: {
          Image(systemName: "arrow.uturn.backward").font(.system(size: 11, weight: .semibold))
        }
        .buttonStyle(.bordered).controlSize(.small)
        .disabled(!viewModel.canUndo)
        .help("실행 취소 (⌘Z)")

        Button {
          viewModel.redo()
        } label: {
          Image(systemName: "arrow.uturn.forward").font(.system(size: 11, weight: .semibold))
        }
        .buttonStyle(.bordered).controlSize(.small)
        .disabled(!viewModel.canRedo)
        .help("다시 실행 (⇧⌘Z)")
      }

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
        .frame(width: 190)
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
                  ? palette.accentBadgeText.swiftUIColor
                  : palette.secondaryText.swiftUIColor
              )
              .padding(.vertical, 4)
              .padding(.horizontal, 8)
              .background(isSelected ? palette.selectedSurface.swiftUIColor : Color.clear)
              .clipShape(RoundedRectangle(cornerRadius: 5))
          }
          .buttonStyle(.plain)
        }

        Spacer()

        if viewModel.splitSecondaryTab != nil {
          Button {
            viewModel.enableSplitView(secondaryTabID: nil)
          } label: {
            Label("스플릿 해제", systemImage: "rectangle.split.2x1.slash").font(
              .system(size: 11, weight: .semibold))
          }
        } else if let candidate = viewModel.openTabs.first(where: { $0.id != viewModel.activeTabID }
        ) {
          Button {
            viewModel.enableSplitView(secondaryTabID: candidate.id)
          } label: {
            Label("스플릿 뷰", systemImage: "rectangle.split.2x1").font(.system(size: 11))
          }
        }

        Button {
          viewModel.showLeftNavigator.toggle()
        } label: {
          Label("내비게이터", systemImage: "sidebar.left").font(.system(size: 11))
        }

        Button {
          viewModel.showRightInspector.toggle()
        } label: {
          Label("인스펙터", systemImage: "sidebar.right").font(.system(size: 11))
        }

        Button {
          viewModel.isRibbonCollapsed.toggle()
        } label: {
          Image(systemName: viewModel.isRibbonCollapsed ? "chevron.down" : "chevron.up")
            .font(.system(size: 11, weight: .semibold))
        }
        .accessibilityLabel(
          AccessibilityTokens.ribbonCollapseToggleLabel(isCollapsed: viewModel.isRibbonCollapsed))
      }

      if !viewModel.isRibbonCollapsed {
        ScrollView(.horizontal, showsIndicators: false) {
          HStack(alignment: .bottom, spacing: 10) {
            ribbonGroupContainer(title: "편집·실행", palette: palette) {
              HStack(spacing: 5) {
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
              HStack(spacing: 6) {
                ForEach(primaryActions) { command in
                  Button {
                    viewModel.executeRibbonCommand(command.id)
                  } label: {
                    HStack(spacing: 6) {
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
                    .padding(.horizontal, 10)
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
                HStack(spacing: 6) {
                  ForEach(secondaryActions) { command in
                    Button {
                      viewModel.executeRibbonCommand(command.id)
                    } label: {
                      HStack(spacing: 6) {
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
                    .fixedSize(horizontal: true, vertical: false)
                    .accessibilityHint(AccessibilityTokens.ribbonControlHint)
                  }
                }
              }
            }

            Divider().frame(height: 36)

            ribbonGroupContainer(title: "빠른 조판·분석", palette: palette) {
              HStack(spacing: 5) {
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
    HStack(spacing: 10) {
      Label("찾기 및 바꾸기", systemImage: "magnifyingglass.circle.fill")
        .font(.system(size: 11, weight: .bold))
        .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)

      TextField("찾을 내용...", text: $viewModel.findQueryText)
        .textFieldStyle(.roundedBorder)
        .font(.system(size: 11))
        .frame(width: 180)

      TextField("바꿀 내용...", text: $viewModel.replaceQueryText)
        .textFieldStyle(.roundedBorder)
        .font(.system(size: 11))
        .frame(width: 180)

      Button("모두 바꾸기") { viewModel.performFindAndReplace() }
        .buttonStyle(.borderedProminent)
        .controlSize(.small)
        .tint(EasyOfficeDesignTokens.brand700.swiftUIColor)

      Text("\(viewModel.lastFindReplaceCount)건 변경됨")
        .font(.system(size: 11, weight: .semibold, design: .monospaced))
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .background(palette.selectedSurface.swiftUIColor)
        .foregroundStyle(palette.accentBadgeText.swiftUIColor)
        .clipShape(RoundedRectangle(cornerRadius: 4))

      Spacer()

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
    .padding(.vertical, 8)
    .background(palette.elevatedSurface.swiftUIColor)
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

struct LeftNavigatorPanelView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  let tab: WorkspaceTabItem

  var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)

    ScrollView(.vertical, showsIndicators: false) {
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
                    size: 12, weight: viewModel.selectedParagraphID == heading.id ? .bold : .medium)
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
          let themeFillColor = EasyOfficeDesignTokens.resolveHexColor(
            deck.deckTheme.primaryFillHex,
            fallback: palette.elevatedSurface.swiftUIColor
          )
          let themeTextColor = EasyOfficeDesignTokens.contrastingTextColor(
            forFillHex: deck.deckTheme.primaryFillHex
          )
          let slideAccent = EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor

          ForEach(Array(deck.slides.enumerated()), id: \.element.id) { idx, slide in
            let isSelectedSlide = viewModel.selectedSlideIndex == idx
            Button {
              viewModel.selectedSlideIndex = idx
            } label: {
              VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 5) {
                  Text("\(idx + 1)")
                    .font(.system(size: 9, weight: .bold, design: .monospaced))
                    .foregroundStyle(Color.white)
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1)
                    .background(isSelectedSlide ? slideAccent : palette.secondaryText.swiftUIColor)
                    .clipShape(RoundedRectangle(cornerRadius: 3))

                  Text("\(idx + 1). \(slide.title)")
                    .font(.system(size: 10, weight: isSelectedSlide ? .bold : .semibold))
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

                ForEach(Array(slide.bulletPoints.prefix(2).enumerated()), id: \.offset) {
                  _, bullet in
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
                    isSelectedSlide ? slideAccent : palette.border.swiftUIColor,
                    lineWidth: isSelectedSlide ? 2.0 : 1.0
                  )
              )
              .clipShape(RoundedRectangle(cornerRadius: 6))
            }
            .buttonStyle(.plain)
          }
        case .pdf(let pdf):
          ForEach(Array(pdf.pages.enumerated()), id: \.element.id) { idx, page in
            Button {
              viewModel.selectedPDFPageIndex = idx
            } label: {
              Text("페이지 \(page.pageNumber) (\(page.rotationDegrees)°)")
                .font(
                  .system(size: 12, weight: viewModel.selectedPDFPageIndex == idx ? .bold : .medium)
                )
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
    ScrollView {
      VStack(alignment: .leading, spacing: 16) {
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
      .padding(24)
    }
  }
}

typealias BottomStatusBarView = BottomWorkspaceStatusBarView

struct BottomWorkspaceStatusBarView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  let tab: WorkspaceTabItem

  var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)

    HStack(spacing: 12) {
      Text("\(tab.moduleKind.displayNameKo) · \(tab.format.standardSpecification)")
        .font(.system(size: 11))
        .foregroundStyle(palette.secondaryText.swiftUIColor)

      if let secondary = viewModel.splitSecondaryTab {
        Text("스플릿 뷰: \(secondary.fileName)")
          .font(.system(size: 11, weight: .semibold))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
      }

      if let errorMessage = viewModel.lastErrorMessage {
        Text("오류: \(errorMessage)")
          .font(.system(size: 11, weight: .semibold))
          .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
          .lineLimit(1)
      } else if let summary = viewModel.lastExecutedCommandSummary, !summary.isEmpty {
        Text(summary)
          .font(.system(size: 11, weight: .medium))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
          .lineLimit(1)
      }

      Spacer()

      Button("-") { viewModel.setZoom(viewModel.zoomPercent - 25.0) }.buttonStyle(.plain)
      Text("배율: \(Int(viewModel.zoomPercent))%")
        .font(.system(size: 11, design: .monospaced))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      Button("+") { viewModel.setZoom(viewModel.zoomPercent + 25.0) }.buttonStyle(.plain)
    }
    .padding(.horizontal, 14)
    .frame(height: EasyOfficeDesignTokens.LayoutMetrics.statusBarHeight)
    .background(palette.cardSurface.swiftUIColor)
  }
}
