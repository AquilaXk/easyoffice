import AppKit
import EasyOfficeCore
import EasyOfficeDesignSystem
import Observation
import SwiftUI

public typealias HWPEditorDetailPaneView = DocumentEditorDetailPaneView

/// AppKit `NSTextView` bridge supporting live UTF-16 sub-range selection tracking (`textViewDidChangeSelection`) and rich inline run rendering (`effectiveRuns`).
public struct SelectableParagraphTextView: NSViewRepresentable {
  @Bindable var viewModel: WorkspaceViewModel
  public let paragraph: ParagraphBlock
  public let palette: WorkspaceThemePalette

  public final class Coordinator: NSObject, NSTextViewDelegate {
    public var parent: SelectableParagraphTextView
    public var isUpdatingProgrammatically: Bool = false
    public var lastRenderedRuns: [DocumentTextRun] = []
    public var lastParagraphID: UUID?
    public var lastAlignment: TextAlignmentMode?
    public var lastLineSpacingPercent: Double = 0
    public var lastFirstLineIndentPt: Double = 0
    public var lastLeftIndentPt: Double = 0
    public var lastLetterSpacingPercent: Double = 0
    public var lastCharWidthScalePercent: Double = 100
    public var lastFontName: String = ""
    public var lastFontSizePt: Double = 0

    public init(parent: SelectableParagraphTextView) {
      self.parent = parent
    }

    public func textDidChange(_ notification: Notification) {
      guard !isUpdatingProgrammatically,
        let textView = notification.object as? NSTextView
      else { return }
      parent.viewModel.updateParagraphText(
        paragraphID: parent.paragraph.id,
        newText: textView.string
      )
    }

    public func textViewDidChangeSelection(_ notification: Notification) {
      guard !isUpdatingProgrammatically,
        let textView = notification.object as? NSTextView
      else { return }
      let nsRange = textView.selectedRange()
      let utf16Range: Range<Int>? =
        nsRange.length > 0
        ? (nsRange.location..<(nsRange.location + nsRange.length))
        : nil
      parent.viewModel.updateSelectedParagraphSelection(
        paragraphID: parent.paragraph.id,
        utf16Range: utf16Range
      )
    }

    public func textView(
      _ textView: NSTextView,
      doCommandBy commandSelector: Selector
    ) -> Bool {
      if commandSelector == #selector(NSResponder.insertNewline(_:)) {
        if NSEvent.modifierFlags.contains(.shift) {
          return false
        }
        let sel = textView.selectedRange()
        _ = parent.viewModel.splitParagraph(
          paragraphID: parent.paragraph.id,
          atUTF16Offset: sel.location,
          selectionLength: sel.length
        )
        return true
      }
      if commandSelector == #selector(NSResponder.deleteBackward(_:)) {
        let range = textView.selectedRange()
        if range.location == 0 && range.length == 0 {
          if parent.viewModel.mergeParagraphWithPrevious(paragraphID: parent.paragraph.id) != nil {
            return true
          }
        }
      }
      if commandSelector == #selector(NSResponder.moveUp(_:)) {
        parent.viewModel.selectAdjacentParagraph(from: parent.paragraph.id, delta: -1)
        return true
      }
      if commandSelector == #selector(NSResponder.moveDown(_:)) {
        parent.viewModel.selectAdjacentParagraph(from: parent.paragraph.id, delta: 1)
        return true
      }
      return false
    }

    public func textView(
      _ textView: NSTextView,
      clickedOnLink link: Any,
      at charIndex: Int
    ) -> Bool {
      let rawString: String?
      if let url = link as? URL {
        rawString = url.absoluteString
      } else if let str = link as? String {
        rawString = str
      } else {
        rawString = nil
      }
      guard let candidate = rawString,
        let safeString = WorkspaceViewModel.sanitizeSafeHyperlinkURL(candidate),
        let safeURL = URL(string: safeString)
      else {
        return true
      }
      if safeURL.host != "easyoffice.local" {
        NSWorkspace.shared.open(safeURL)
      }
      parent.viewModel.lastExecutedCommandSummary = "안전 하이퍼링크 열기: \(safeURL.absoluteString)"
      return true
    }
  }

  public init(
    viewModel: WorkspaceViewModel,
    paragraph: ParagraphBlock,
    palette: WorkspaceThemePalette
  ) {
    self.viewModel = viewModel
    self.paragraph = paragraph
    self.palette = palette
  }

  public func makeCoordinator() -> Coordinator {
    Coordinator(parent: self)
  }

  public func makeNSView(context: Context) -> NSScrollView {
    let scrollView = NSScrollView()
    scrollView.hasVerticalScroller = false
    scrollView.hasHorizontalScroller = false
    scrollView.drawsBackground = false
    scrollView.borderType = .noBorder

    let textView = NSTextView()
    textView.isRichText = true
    textView.isEditable = viewModel.workspaceViewMode != .readOnlyView
    textView.isSelectable = true
    textView.drawsBackground = false
    textView.isVerticallyResizable = true
    textView.isHorizontallyResizable = false
    textView.textContainerInset = NSSize(width: 2, height: 1)
    textView.textContainer?.widthTracksTextView = true
    textView.delegate = context.coordinator

    context.coordinator.isUpdatingProgrammatically = true
    textView.textStorage?.setAttributedString(buildAttributedString(from: paragraph))
    context.coordinator.lastRenderedRuns = paragraph.effectiveRuns
    context.coordinator.lastParagraphID = paragraph.id
    context.coordinator.lastAlignment = paragraph.alignment
    context.coordinator.lastLineSpacingPercent = paragraph.lineSpacingPercent
    context.coordinator.lastFirstLineIndentPt = paragraph.firstLineIndentPt
    context.coordinator.lastLeftIndentPt = paragraph.leftIndentPt
    context.coordinator.lastLetterSpacingPercent = paragraph.letterSpacingPercent
    context.coordinator.lastCharWidthScalePercent = paragraph.charWidthScalePercent
    context.coordinator.lastFontName = paragraph.fontName
    context.coordinator.lastFontSizePt = paragraph.fontSizePt
    context.coordinator.isUpdatingProgrammatically = false

    scrollView.documentView = textView
    return scrollView
  }

  public func updateNSView(_ nsView: NSScrollView, context: Context) {
    context.coordinator.parent = self
    guard let textView = nsView.documentView as? NSTextView else { return }
    let shouldBeEditable = viewModel.workspaceViewMode != .readOnlyView
    if textView.isEditable != shouldBeEditable {
      textView.isEditable = shouldBeEditable
    }
    let currentRuns = paragraph.effectiveRuns
    let paragraphStyleChanged =
      context.coordinator.lastAlignment != paragraph.alignment
      || context.coordinator.lastLineSpacingPercent != paragraph.lineSpacingPercent
      || context.coordinator.lastFirstLineIndentPt != paragraph.firstLineIndentPt
      || context.coordinator.lastLeftIndentPt != paragraph.leftIndentPt
      || context.coordinator.lastLetterSpacingPercent != paragraph.letterSpacingPercent
      || context.coordinator.lastCharWidthScalePercent != paragraph.charWidthScalePercent
      || context.coordinator.lastFontName != paragraph.fontName
      || context.coordinator.lastFontSizePt != paragraph.fontSizePt

    if context.coordinator.lastParagraphID != paragraph.id
      || context.coordinator.lastRenderedRuns != currentRuns
      || paragraphStyleChanged
      || textView.string != paragraph.text
    {
      let previousSelection = textView.selectedRange()
      context.coordinator.isUpdatingProgrammatically = true
      textView.textStorage?.setAttributedString(buildAttributedString(from: paragraph))
      let utf16Len = (textView.string as NSString).length
      if previousSelection.location + previousSelection.length <= utf16Len {
        textView.setSelectedRange(previousSelection)
      }
      context.coordinator.lastRenderedRuns = currentRuns
      context.coordinator.lastParagraphID = paragraph.id
      context.coordinator.lastAlignment = paragraph.alignment
      context.coordinator.lastLineSpacingPercent = paragraph.lineSpacingPercent
      context.coordinator.lastFirstLineIndentPt = paragraph.firstLineIndentPt
      context.coordinator.lastLeftIndentPt = paragraph.leftIndentPt
      context.coordinator.lastLetterSpacingPercent = paragraph.letterSpacingPercent
      context.coordinator.lastCharWidthScalePercent = paragraph.charWidthScalePercent
      context.coordinator.lastFontName = paragraph.fontName
      context.coordinator.lastFontSizePt = paragraph.fontSizePt
      context.coordinator.isUpdatingProgrammatically = false
    }

    if viewModel.pendingFocusParagraphID == paragraph.id {
      let utf16Len = (textView.string as NSString).length
      let targetCaret = max(0, min(viewModel.pendingCaretUTF16Offset ?? 0, utf16Len))
      context.coordinator.isUpdatingProgrammatically = true
      textView.setSelectedRange(NSRange(location: targetCaret, length: 0))
      if let win = textView.window {
        if win.firstResponder !== textView {
          win.makeFirstResponder(textView)
        }
        textView.setSelectedRange(NSRange(location: targetCaret, length: 0))
        context.coordinator.isUpdatingProgrammatically = false
        let expectedID = paragraph.id
        DispatchQueue.main.async {
          if viewModel.pendingFocusParagraphID == expectedID {
            viewModel.pendingFocusParagraphID = nil
            viewModel.pendingCaretUTF16Offset = nil
          }
        }
      } else {
        context.coordinator.isUpdatingProgrammatically = false
        let expectedID = paragraph.id
        DispatchQueue.main.async {
          guard viewModel.pendingFocusParagraphID == expectedID,
            let win = textView.window
          else { return }
          context.coordinator.isUpdatingProgrammatically = true
          if win.firstResponder !== textView {
            win.makeFirstResponder(textView)
          }
          let len = (textView.string as NSString).length
          let caret = max(0, min(viewModel.pendingCaretUTF16Offset ?? targetCaret, len))
          textView.setSelectedRange(NSRange(location: caret, length: 0))
          context.coordinator.isUpdatingProgrammatically = false
          viewModel.pendingFocusParagraphID = nil
          viewModel.pendingCaretUTF16Offset = nil
        }
      }
    }
  }

  public func buildAttributedString(from targetParagraph: ParagraphBlock? = nil)
    -> NSAttributedString
  {
    let para = targetParagraph ?? paragraph
    let result = NSMutableAttributedString()
    let defaultColor = NSColor(
      srgbRed: CGFloat(palette.primaryText.red) / 255.0,
      green: CGFloat(palette.primaryText.green) / 255.0,
      blue: CGFloat(palette.primaryText.blue) / 255.0,
      alpha: 1.0
    )

    let paraStyle = NSMutableParagraphStyle()
    let lineSpacingMultiple = max(0.8, para.lineSpacingPercent / 100.0)
    paraStyle.lineHeightMultiple = CGFloat(lineSpacingMultiple)
    paraStyle.firstLineHeadIndent = CGFloat(max(0.0, para.firstLineIndentPt))
    paraStyle.headIndent = CGFloat(max(0.0, para.leftIndentPt))
    switch para.alignment {
    case .leading:
      paraStyle.alignment = .left
    case .center:
      paraStyle.alignment = .center
    case .trailing:
      paraStyle.alignment = .right
    case .justified:
      paraStyle.alignment = .justified
    }

    let kernPt = CGFloat(para.fontSizePt * (para.letterSpacingPercent / 100.0))
    let expansion = CGFloat(log(max(0.5, para.charWidthScalePercent / 100.0)))

    for run in para.effectiveRuns {
      let fontSize = CGFloat(run.fontSizePt ?? para.fontSizePt)
      let fontFamily = run.fontName ?? para.fontName
      var font: NSFont =
        if run.isCode {
          NSFont.monospacedSystemFont(ofSize: fontSize, weight: run.isBold ? .bold : .regular)
        } else {
          NSFont(name: fontFamily, size: fontSize)
            ?? NSFont.systemFont(ofSize: fontSize, weight: run.isBold ? .bold : .regular)
        }
      if run.isBold {
        font = NSFontManager.shared.convert(font, toHaveTrait: .boldFontMask)
      }
      if run.isItalic {
        font = NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask)
      }
      let fgColor = resolveOptionalNSColor(run.foregroundHex) ?? defaultColor
      var attrs: [NSAttributedString.Key: Any] = [
        .font: font,
        .foregroundColor: fgColor,
        .paragraphStyle: paraStyle,
        .kern: kernPt,
        .expansion: expansion,
      ]
      if run.isUnderlined {
        attrs[.underlineStyle] = NSUnderlineStyle.single.rawValue
      }
      if run.isStrikethrough {
        attrs[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
      }
      if run.isSuperscript {
        attrs[.superscript] = 1
      } else if run.isSubscript {
        attrs[.superscript] = -1
      }
      if let bgHex = run.backgroundHex, let bgColor = resolveOptionalNSColor(bgHex) {
        attrs[.backgroundColor] = bgColor
      }
      if let link = run.linkURL,
        let safeLink = WorkspaceViewModel.sanitizeSafeHyperlinkURL(link),
        let safeURL = URL(string: safeLink)
      {
        attrs[.link] = safeURL
        attrs[.underlineStyle] = NSUnderlineStyle.single.rawValue
      }
      result.append(NSAttributedString(string: run.text, attributes: attrs))
    }
    return result
  }
}

/// Rich interactive editor pane for Document (.docx/.odt/.md) and Korean Office (.hwp/.hwpx) modules.
public struct DocumentEditorDetailPaneView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  let document: OfficeDocumentModel
  let isPrimaryPane: Bool
  @State private var selectedLanguageCategory: String = "대표"
  @State private var justifySubMode: String = "양쪽"
  @State private var showParagraphMarks: Bool = true
  @State private var showFormattingMarks: Bool = false

  public init(
    viewModel: WorkspaceViewModel, document: OfficeDocumentModel, isPrimaryPane: Bool = true
  ) {
    self.viewModel = viewModel
    self.document = document
    self.isPrimaryPane = isPrimaryPane
  }

  private func availableFontFamilies(for category: String) -> [String] {
    switch category {
    case "한글":
      return ["함초롬바탕", "함초롬돋움", "AppleSDGothicNeo-Regular", "NanumMyeongjo"]
    case "영문":
      return ["Pretendard", "Menlo", "Helvetica Neue", "Times New Roman"]
    case "한자":
      return ["함초롬바탕", "NanumMyeongjo", "AppleMyungjo"]
    case "기호":
      return ["Apple Symbols", "Menlo", "함초롬바탕"]
    default:
      return [
        "함초롬바탕",
        "함초롬돋움",
        "Pretendard",
        "AppleSDGothicNeo-Regular",
        "NanumMyeongjo",
        "Menlo",
      ]
    }
  }

  private func firstMailMergePreviewLine() -> String? {
    guard !viewModel.mailMergeFields.isEmpty else { return nil }
    for paragraph in document.paragraphs where paragraph.text.contains("{{") {
      var replaced = paragraph.text
      for (key, value) in viewModel.mailMergeFields {
        replaced = replaced.replacingOccurrences(of: "{{\(key)}}", with: value)
      }
      if replaced != paragraph.text {
        return replaced
      }
    }
    return nil
  }

  private func swiftUIAlignment(for alignment: TextAlignmentMode) -> Alignment {
    switch alignment {
    case .leading, .justified:
      return .leading
    case .center:
      return .center
    case .trailing:
      return .trailing
    }
  }

  private func shortStyleTitle(for role: ParagraphStyleRole) -> String {
    switch role {
    case .title:
      return "제목"
    case .heading1:
      return "개요 1"
    case .heading2:
      return "개요 2"
    case .heading3:
      return "개요 3"
    case .body:
      return "바탕글"
    case .quote:
      return "인용문"
    case .officialHeader:
      return "공문서 두문"
    }
  }

  private func compactParagraphHeight(for paragraph: ParagraphBlock) -> CGFloat {
    let isSplitActive = viewModel.splitSecondaryTab != nil
    let scaledChars = (Double(isSplitActive ? 42 : 62) * (11.0 / max(8.0, min(144.0, paragraph.fontSizePt)))).rounded()
    let charsPerLine = max(24, Int(exactly: min(200.0, max(24.0, scaledChars))) ?? 52)
    let segments = paragraph.text.components(separatedBy: .newlines)
    var totalLines = 0
    for seg in segments {
      let wrapped = max(1, (seg.count + charsPerLine - 1) / charsPerLine)
      totalLines += wrapped
    }
    let lineCount = max(1, totalLines)
    let spacingFactor = max(1.0, paragraph.lineSpacingPercent / 100.0)
    let baseLineHeight = max(20.0, CGFloat(paragraph.fontSizePt) * CGFloat(spacingFactor) * 1.15)
    return max(isSplitActive ? 28.0 : 26.0, CGFloat(lineCount) * baseLineHeight + 6.0)
  }

  @ViewBuilder
  private func hwpOfficialHeaderBanner(palette: WorkspaceThemePalette) -> some View {
    let hwpAccent = EasyOfficeDesignTokens.accentToken(for: .hwp, isDark: palette.isDark)
      .swiftUIColor
    let officialHeaderText =
      document.paragraphs.first(where: { $0.style == .officialHeader })?.text
      ?? (document.headerText.isEmpty ? "행정기관 표준 공문서 · 문서번호 기안-2026-0201" : document.headerText)
    VStack(alignment: .leading, spacing: 6) {
      HStack(alignment: .center, spacing: 12) {
        VStack(alignment: .leading, spacing: 3) {
          HStack(spacing: 8) {
            Text("공문서 두문")
              .font(.system(size: 10, weight: .bold))
              .foregroundStyle(hwpAccent)

            if isPrimaryPane && !viewModel.isCanvasSnapshotMode {
              TextField(
                "공문서 두문·문서번호 입력",
                text: Binding(
                  get: { officialHeaderText },
                  set: { viewModel.updateActiveDocumentOfficialHeaderText($0) }
                )
              )
              .textFieldStyle(.plain)
              .font(.system(size: 10, weight: .medium, design: .monospaced))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
              .frame(minWidth: 220)
            } else {
              Text(officialHeaderText)
                .font(.system(size: 9.5, weight: .medium, design: .monospaced))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
            }
          }
          Text(document.title)
            .font(.system(size: 13, weight: .bold))
            .foregroundStyle(palette.primaryText.swiftUIColor)
            .lineLimit(1)
        }

        Spacer(minLength: 8)

        HStack(spacing: 12) {
          ForEach(["담당 완료", "검토 완료", "결재 승인"], id: \.self) { statusItem in
            Text(statusItem)
              .font(.system(size: 9.5, weight: .semibold))
              .foregroundStyle(
                statusItem.hasPrefix("결재") ? hwpAccent : palette.secondaryText.swiftUIColor
              )
          }
        }
      }
      Divider()
    }
  }

  private func flatToolbarButton(
    _ title: String,
    isProminent: Bool = false,
    palette: WorkspaceThemePalette,
    action: @escaping () -> Void
  ) -> some View {
    Button(action: action) {
      Text(title)
        .font(.system(size: 11, weight: isProminent ? .bold : .medium))
        .lineLimit(1)
        .fixedSize(horizontal: true, vertical: false)
        .foregroundStyle(
          isProminent
            ? EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor
            : palette.primaryText.swiftUIColor
        )
        .padding(.horizontal, 4)
        .padding(.vertical, 2)
    }
    .buttonStyle(.plain)
  }

  private func studioIconButton(
    _ glyph: String,
    tooltip: String,
    tintHex: String? = nil,
    isActive: Bool = false,
    palette: WorkspaceThemePalette,
    action: @escaping () -> Void
  ) -> some View {
    let fgColor: Color = {
      if isActive {
        return EasyOfficeDesignTokens.resolveHexColor("#2563EB", fallback: .blue)
      }
      if let hex = tintHex {
        return EasyOfficeDesignTokens.resolveHexColor(
          hex, fallback: palette.primaryText.swiftUIColor)
      }
      return palette.isDark
        ? EasyOfficeDesignTokens.resolveHexColor("#E5E7EB", fallback: .white)
        : EasyOfficeDesignTokens.resolveHexColor("#333333", fallback: .black)
    }()
    let activeBg =
      palette.isDark
      ? Color.white.opacity(0.12)
      : EasyOfficeDesignTokens.resolveHexColor("#E0E7FF", fallback: Color.blue.opacity(0.12))
    return Button(action: action) {
      ZStack {
        if isActive {
          activeBg
        }
        Text(glyph)
          .font(.system(size: 12, weight: isActive ? .bold : .semibold))
          .lineLimit(1)
          .fixedSize(horizontal: true, vertical: false)
          .foregroundStyle(fgColor)
          .padding(.horizontal, 5)
      }
      .frame(minWidth: 24, minHeight: 24, maxHeight: 24)
      .fixedSize(horizontal: true, vertical: true)
    }
    .buttonStyle(.plain)
    .help(tooltip)
  }

  private func studioStyleGlyphButton(
    _ glyph: String,
    tooltip: String,
    isBold: Bool = false,
    isItalic: Bool = false,
    isUnderlined: Bool = false,
    isStrikethrough: Bool = false,
    underlineBarColor: Color? = nil,
    isActive: Bool = false,
    palette: WorkspaceThemePalette,
    action: @escaping () -> Void
  ) -> some View {
    let fgColor =
      isActive
      ? EasyOfficeDesignTokens.resolveHexColor("#2563EB", fallback: .blue)
      : (palette.isDark
        ? EasyOfficeDesignTokens.resolveHexColor("#E5E7EB", fallback: .white)
        : EasyOfficeDesignTokens.resolveHexColor("#222222", fallback: .black))
    let activeBg =
      palette.isDark
      ? Color.white.opacity(0.14)
      : EasyOfficeDesignTokens.resolveHexColor("#DBEAFE", fallback: Color.blue.opacity(0.14))
    return Button(action: action) {
      ZStack {
        if isActive {
          activeBg
        }
        VStack(spacing: 1) {
          Text(glyph)
            .font(.system(size: 12, weight: isBold ? .bold : .medium))
            .italic(isItalic)
            .underline(isUnderlined)
            .strikethrough(isStrikethrough)
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
            .foregroundStyle(fgColor)
          if let barColor = underlineBarColor {
            Rectangle()
              .fill(barColor)
              .frame(width: 14, height: 2)
          }
        }
      }
      .frame(width: 26, height: 24)
    }
    .buttonStyle(.plain)
    .help(tooltip)
  }

  private func studioAlignmentIconButton(
    symbolName: String,
    tooltip: String,
    isActive: Bool,
    palette: WorkspaceThemePalette,
    action: @escaping () -> Void
  ) -> some View {
    let fgColor =
      isActive
      ? EasyOfficeDesignTokens.resolveHexColor("#2563EB", fallback: .blue)
      : (palette.isDark
        ? EasyOfficeDesignTokens.resolveHexColor("#E5E7EB", fallback: .white)
        : EasyOfficeDesignTokens.resolveHexColor("#333333", fallback: .black))
    let activeBg =
      palette.isDark
      ? Color.white.opacity(0.14)
      : EasyOfficeDesignTokens.resolveHexColor("#DBEAFE", fallback: Color.blue.opacity(0.14))
    return Button(action: action) {
      ZStack {
        if isActive {
          activeBg
        }
        Image(systemName: symbolName)
          .font(.system(size: 11.5, weight: isActive ? .bold : .medium))
          .foregroundStyle(fgColor)
      }
      .frame(width: 26, height: 24)
    }
    .buttonStyle(.plain)
    .help(tooltip)
  }

  private func headingOrderNumber(upTo paragraphIndex: Int) -> Int {
    guard paragraphIndex >= 0 && paragraphIndex < document.paragraphs.count else { return 1 }
    var count = 0
    for idx in 0...paragraphIndex {
      let p = document.paragraphs[idx]
      let trimmed = p.text.trimmingCharacters(in: .whitespacesAndNewlines)
      if (p.style == .heading1 || p.style == .heading2) && !trimmed.hasPrefix("제목:") {
        count += 1
      }
    }
    return max(1, count)
  }

  public var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
    let tocEntries = document.generateTableOfContents()
    let mailMergePreviewLine = firstMailMergePreviewLine()
    let colCount = document.sections.first?.columnCount ?? 1
    let currentOrientation = document.sections.first?.orientation ?? .portrait
    let isLandscape = currentOrientation == .landscape
    let zoomScale = CGFloat(max(50.0, min(200.0, viewModel.zoomPercent))) / 100.0
    let paperMaxWidth: CGFloat = (isLandscape ? 980.0 : 820.0) * zoomScale
    let isHWPModule =
      viewModel.activeTab?.moduleKind == .hwp
      || viewModel.activeTab?.format == .hwp
      || viewModel.activeTab?.format == .hwpx
    let totalPages = document.estimatedPageCount
    let resolvedHeaderText = OfficeDocumentModel.resolvePageTokens(
      in: document.headerText,
      pageNumber: 1,
      totalPages: totalPages
    )
    let resolvedFooterText = OfficeDocumentModel.resolvePageTokens(
      in: document.footerText,
      pageNumber: 1,
      totalPages: totalPages
    )
    let footnoteItems = document.footnotes.filter { !$0.isEndnote }
    let endnoteItems = document.footnotes.filter { $0.isEndnote }
    let watermarkText = (document.watermarkText ?? "").trimmingCharacters(
      in: .whitespacesAndNewlines)
    let selectedParagraph =
      document.paragraphs.first(where: { $0.id == viewModel.selectedParagraphID })
      ?? document.paragraphs.first

    let paperPalette =
      viewModel.isManuscriptGridModeEnabled
      ? palette
      : WorkspaceThemeMode.lightLavender.surfacePalette(isSystemDark: false)
    let scrollBackdropColor =
      palette.isDark
      ? EasyOfficeDesignTokens.resolveHexColor("#1F2329", fallback: palette.background.swiftUIColor)
      : EasyOfficeDesignTokens.resolveHexColor("#E0E0E0", fallback: palette.background.swiftUIColor)
    let row2Bg =
      palette.isDark
      ? EasyOfficeDesignTokens.resolveHexColor("#2F343C", fallback: palette.elevatedSurface.swiftUIColor)
      : EasyOfficeDesignTokens.resolveHexColor("#F5F5F5", fallback: palette.elevatedSurface.swiftUIColor)
    let row3Bg =
      palette.isDark
      ? EasyOfficeDesignTokens.resolveHexColor("#25292F", fallback: palette.elevatedSurface.swiftUIColor)
      : EasyOfficeDesignTokens.resolveHexColor("#F0F0F0", fallback: palette.elevatedSurface.swiftUIColor)
    let comboBoxFill =
      palette.isDark
      ? EasyOfficeDesignTokens.resolveHexColor("#1F242B", fallback: palette.cardSurface.swiftUIColor)
      : Color.white
    let comboBorder =
      palette.isDark
      ? Color.white.opacity(0.16)
      : EasyOfficeDesignTokens.resolveHexColor("#CCCCCC", fallback: Color.gray.opacity(0.4))

    VStack(alignment: .leading, spacing: 0) {
      if isPrimaryPane {
        ZStack {
          row2Bg
          AdaptiveScrollContainer(
            axes: .horizontal,
            showsIndicators: false,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          ) {
            HStack(spacing: 4) {
              HStack(spacing: 2) {
                studioIconButton("💾", tooltip: "문서 저장", palette: palette) {
                  if !viewModel.isCanvasSnapshotMode {
                    _ = WorkspaceFileDialogPresenter.presentSavePanel(viewModel: viewModel)
                  } else {
                    viewModel.lastExecutedCommandSummary = "문서 저장 완료"
                  }
                }
                studioIconButton("↩", tooltip: "실행 취소", palette: palette) {
                  viewModel.undoActiveTab()
                }
                studioIconButton("↪", tooltip: "다시 실행", palette: palette) {
                  viewModel.redoActiveTab()
                }
                Divider().frame(height: 16)
                studioIconButton("✂", tooltip: "오려두기", palette: palette) {
                  if let p = selectedParagraph, !p.text.isEmpty {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(p.text, forType: .string)
                    if document.paragraphs.count > 1 {
                      viewModel.updateParagraphText(paragraphID: p.id, newText: "")
                    }
                  }
                  viewModel.lastExecutedCommandSummary = "선택 문단 오려두기 완료"
                }
                studioIconButton("📄", tooltip: "복사하기", palette: palette) {
                  if let text = selectedParagraph?.text, !text.isEmpty {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(text, forType: .string)
                  }
                  viewModel.lastExecutedCommandSummary = "선택 문단 클립보드 복사 완료"
                }
                studioIconButton("📋", tooltip: "붙이기", palette: palette) {
                  let pasted =
                    NSPasteboard.general.string(forType: .string)?
                    .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                  let textToPaste = pasted.isEmpty ? (selectedParagraph?.text ?? "붙여넣기 문단") : pasted
                  viewModel.appendParagraphToActiveDocument(text: textToPaste, style: .body)
                }
                studioIconButton("🖌", tooltip: "모양복사", palette: palette) {
                  viewModel.executeRibbonCommand(
                    isHWPModule ? "hwp-home-style" : "document-home-style"
                  )
                }
              }

              Divider().frame(height: 16)

              HStack(spacing: 2) {
                studioIconButton(
                  "¶",
                  tooltip: "조판부호",
                  tintHex: "#4A90D9",
                  isActive: showFormattingMarks,
                  palette: palette
                ) {
                  showFormattingMarks.toggle()
                }
                studioIconButton(
                  "↵",
                  tooltip: "문단부호",
                  tintHex: "#4A90D9",
                  isActive: showParagraphMarks,
                  palette: palette
                ) {
                  showParagraphMarks.toggle()
                }
                studioIconButton(
                  "#",
                  tooltip: "격자보기",
                  isActive: viewModel.isManuscriptGridModeEnabled,
                  palette: palette
                ) {
                  viewModel.toggleManuscriptGridMode()
                }
              }

              Divider().frame(height: 16)

              HStack(spacing: 2) {
                studioIconButton("가/가", tooltip: "글자모양", palette: palette) {
                  viewModel.executeRibbonCommand(
                    isHWPModule ? "hwp-home-bold" : "document-home-bold"
                  )
                }
                studioIconButton("≡✓", tooltip: "문단모양", palette: palette) {
                  viewModel.setActiveParagraphAlignment(.justified)
                }
              }

              Divider().frame(height: 16)

              HStack(spacing: 2) {
                studioIconButton(
                  "1.",
                  tooltip: "문단번호",
                  tintHex: "#4A90D9",
                  isActive: selectedParagraph?.listKind == .numbered,
                  palette: palette
                ) {
                  viewModel.setActiveParagraphListKind(
                    (selectedParagraph?.listKind == .numbered) ? .none : .numbered
                  )
                }
                studioIconButton(
                  "●",
                  tooltip: "글머리표",
                  tintHex: "#4A90D9",
                  isActive: selectedParagraph?.listKind == .bullet,
                  palette: palette
                ) {
                  viewModel.setActiveParagraphListKind(
                    (selectedParagraph?.listKind == .bullet) ? .none : .bullet
                  )
                }
                studioIconButton("⇤", tooltip: "한 수준 감소", palette: palette) {
                  let currentKind = selectedParagraph?.listKind ?? .numbered
                  viewModel.setActiveParagraphListKind(currentKind, levelDelta: -1)
                }
                studioIconButton("⇥", tooltip: "한 수준 증가", palette: palette) {
                  let currentKind = selectedParagraph?.listKind ?? .numbered
                  let resolvedKind = currentKind == .none ? .numbered : currentKind
                  viewModel.setActiveParagraphListKind(resolvedKind, levelDelta: 1)
                }
              }

              Divider().frame(height: 16)

              HStack(spacing: 2) {
                studioIconButton("⊞▾", tooltip: "표 추가", palette: palette) {
                  viewModel.insertRowInDocumentTable()
                }
                studioIconButton("△□▾", tooltip: "도형/다이어그램", palette: palette) {
                  viewModel.insertSampleImageAttachment()
                }
                studioIconButton("🖼", tooltip: "그림 삽입", palette: palette) {
                  WorkspaceFileDialogPresenter.presentOpenImagePanel(for: viewModel)
                }
              }

              Divider().frame(height: 16)

              HStack(spacing: 2) {
                studioIconButton("⊡", tooltip: "개체속성", palette: palette) {
                  viewModel.isAdvancedToolbarExpanded.toggle()
                }
                studioIconButton("※", tooltip: "문자표/수식", palette: palette) {
                  viewModel.insertEquationIntoActiveDocument()
                }
                studioIconButton("🌐", tooltip: "하이퍼링크", palette: palette) {
                  viewModel.insertHyperlinkIntoActiveParagraph()
                }
              }

              Divider().frame(height: 16)

              HStack(spacing: 2) {
                studioIconButton("▤", tooltip: "머리말", palette: palette) {
                  viewModel.executeRibbonCommand(
                    isHWPModule ? "hwp-mod-header" : "doc-mod-header"
                  )
                }
                studioIconButton("▥", tooltip: "꼬리말", palette: palette) {
                  if let tabID = viewModel.activeTabID,
                    let idx = viewModel.openTabs.firstIndex(where: { $0.id == tabID }),
                    case .document(var d) = viewModel.openTabs[idx].payload
                  {
                    viewModel.recordUndoSnapshot(for: tabID)
                    d.footerText = "\(viewModel.openTabs[idx].fileName) — \(d.estimatedPageCount)페이지"
                    viewModel.openTabs[idx].payload = .document(d)
                    viewModel.openTabs[idx].isModified = true
                  }
                  viewModel.lastExecutedCommandSummary = "꼬리말 갱신 완료"
                }
                studioIconButton("[1]", tooltip: "각주", palette: palette) {
                  viewModel.executeRibbonCommand("doc-insert-footnote")
                }
                studioIconButton("[i]", tooltip: "미주", palette: palette) {
                  viewModel.insertEndnoteIntoActiveDocument()
                }
                studioIconButton(
                  "🔭▾",
                  tooltip: "찾기/바꾸기",
                  isActive: viewModel.isFindReplaceBarVisible,
                  palette: palette
                ) {
                  viewModel.isFindReplaceBarVisible.toggle()
                }
              }
            }
            .padding(.horizontal, 8)
          }
        }
        .frame(height: 32)
        Divider()

        ZStack {
          row3Bg
          AdaptiveScrollContainer(
            axes: .horizontal,
            showsIndicators: false,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          ) {
            HStack(spacing: 6) {
              ZStack(alignment: .leading) {
                comboBoxFill
                FlatToolbarMenu(
                  title: shortStyleTitle(for: selectedParagraph?.style ?? .body),
                  palette: palette,
                  isSnapshotMode: viewModel.isCanvasSnapshotMode
                ) {
                  Button("바탕글") { viewModel.setActiveParagraphStyle(.body) }
                  Button("본문") { viewModel.setActiveParagraphStyle(.body) }
                  Button("개요 1") { viewModel.setActiveParagraphStyle(.heading1) }
                  Button("개요 2") { viewModel.setActiveParagraphStyle(.heading2) }
                  Button("제목") { viewModel.setActiveParagraphStyle(.title) }
                  Button("인용문") { viewModel.setActiveParagraphStyle(.quote) }
                  Button("공문서 두문") { viewModel.setActiveParagraphStyle(.officialHeader) }
                }
                .padding(.horizontal, 6)
              }
              .frame(width: 82, height: 23)
              .border(comboBorder, width: 1)

              ZStack(alignment: .leading) {
                comboBoxFill
                FlatToolbarMenu(
                  title: selectedLanguageCategory,
                  palette: palette,
                  isSnapshotMode: viewModel.isCanvasSnapshotMode
                ) {
                  ForEach(["대표", "한글", "영문", "한자", "기호"], id: \.self) { lang in
                    Button(lang) {
                      selectedLanguageCategory = lang
                    }
                  }
                }
                .padding(.horizontal, 6)
              }
              .frame(width: 56, height: 23)
              .border(comboBorder, width: 1)

              let rawFontName = selectedParagraph?.fontName ?? "함초롬바탕"
              let displayFontName =
                (rawFontName.isEmpty || rawFontName == "Pretendard" || rawFontName == "System")
                ? "함초롬바탕" : rawFontName
              ZStack(alignment: .leading) {
                comboBoxFill
                FlatToolbarMenu(
                  title: displayFontName,
                  palette: palette,
                  isSnapshotMode: viewModel.isCanvasSnapshotMode
                ) {
                  ForEach(
                    availableFontFamilies(for: selectedLanguageCategory),
                    id: \.self
                  ) { family in
                    Button(family) {
                      viewModel.setActiveParagraphFont(fontName: family)
                    }
                  }
                }
                .padding(.horizontal, 6)
              }
              .frame(width: 118, height: 23)
              .border(comboBorder, width: 1)

              HStack(spacing: 1) {
                ZStack(alignment: .leading) {
                  comboBoxFill
                  FlatToolbarMenu(
                    title: String(format: "%.1f pt", selectedParagraph?.fontSizePt ?? 10.0),
                    palette: palette,
                    isSnapshotMode: viewModel.isCanvasSnapshotMode
                  ) {
                    ForEach([9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 32.0], id: \.self)
                    { pt in
                      Button(String(format: "%.1f pt", pt)) {
                        viewModel.setActiveParagraphFont(fontSizePt: pt)
                      }
                    }
                  }
                  .padding(.horizontal, 6)
                }
                .frame(width: 66, height: 23)
                .border(comboBorder, width: 1)

                flatToolbarButton("▲", palette: palette) {
                  let cur = selectedParagraph?.fontSizePt ?? 10.0
                  viewModel.setActiveParagraphFont(fontSizePt: min(72.0, cur + 1.0))
                }
                flatToolbarButton("▼", palette: palette) {
                  let cur = selectedParagraph?.fontSizePt ?? 10.0
                  viewModel.setActiveParagraphFont(fontSizePt: max(6.0, cur - 1.0))
                }
              }

              Divider().frame(height: 16)

              HStack(spacing: 2) {
                studioStyleGlyphButton(
                  "가",
                  tooltip: "굵게",
                  isBold: true,
                  isActive: selectedParagraph?.isBold == true,
                  palette: palette
                ) {
                  viewModel.executeRibbonCommand(
                    isHWPModule ? "hwp-home-bold" : "document-home-bold"
                  )
                }
                studioStyleGlyphButton(
                  "가",
                  tooltip: "기울임",
                  isItalic: true,
                  isActive: selectedParagraph?.isItalic == true,
                  palette: palette
                ) {
                  viewModel.executeRibbonCommand(
                    isHWPModule ? "hwp-home-italic" : "document-home-italic"
                  )
                }
                studioStyleGlyphButton(
                  "간",
                  tooltip: "밑줄",
                  isUnderlined: true,
                  isActive: selectedParagraph?.isUnderlined == true,
                  palette: palette
                ) {
                  viewModel.executeRibbonCommand(
                    isHWPModule ? "hwp-home-underline" : "document-home-underline"
                  )
                }
                studioStyleGlyphButton(
                  "가",
                  tooltip: "취소선",
                  isStrikethrough: true,
                  palette: palette
                ) {
                  viewModel.toggleStrikethroughOnActiveParagraph()
                }
                studioStyleGlyphButton(
                  "가▾",
                  tooltip: "글자 장식",
                  palette: palette
                ) {
                  viewModel.applyInlineRunFormattingToActiveParagraph(
                    isBold: true,
                    isUnderlined: true,
                    foregroundHex: "#5C6BC0"
                  )
                }
              }

              Divider().frame(height: 16)

              HStack(spacing: 2) {
                studioStyleGlyphButton(
                  "간▾",
                  tooltip: "글자색",
                  underlineBarColor: .red,
                  palette: palette
                ) {
                  viewModel.applyInlineRunFormattingToActiveParagraph(foregroundHex: "#DC2626")
                }
                studioStyleGlyphButton(
                  "✏▾",
                  tooltip: "형광펜",
                  underlineBarColor: .yellow,
                  palette: palette
                ) {
                  viewModel.toggleHighlightOnActiveParagraph()
                }
              }

              Divider().frame(height: 16)

              HStack(spacing: 2) {
                studioAlignmentIconButton(
                  symbolName: "text.alignleft",
                  tooltip: "왼쪽 정렬",
                  isActive: selectedParagraph?.alignment == .leading,
                  palette: palette
                ) {
                  viewModel.setActiveParagraphAlignment(.leading)
                }
                studioAlignmentIconButton(
                  symbolName: "text.aligncenter",
                  tooltip: "가운데 정렬",
                  isActive: selectedParagraph?.alignment == .center,
                  palette: palette
                ) {
                  viewModel.setActiveParagraphAlignment(.center)
                }
                studioAlignmentIconButton(
                  symbolName: "text.alignright",
                  tooltip: "오른쪽 정렬",
                  isActive: selectedParagraph?.alignment == .trailing,
                  palette: palette
                ) {
                  viewModel.setActiveParagraphAlignment(.trailing)
                }
                studioAlignmentIconButton(
                  symbolName: "text.justify",
                  tooltip: "양쪽 정렬",
                  isActive: selectedParagraph?.alignment == .justified && justifySubMode == "양쪽",
                  palette: palette
                ) {
                  justifySubMode = "양쪽"
                  viewModel.setActiveParagraphAlignment(.justified)
                }
                studioAlignmentIconButton(
                  symbolName: "text.justify.leading",
                  tooltip: "배분 정렬",
                  isActive: selectedParagraph?.alignment == .justified && justifySubMode == "배분",
                  palette: palette
                ) {
                  justifySubMode = "배분"
                  viewModel.setActiveParagraphAlignment(.justified)
                }
                studioAlignmentIconButton(
                  symbolName: "text.justify.trailing",
                  tooltip: "나눔 정렬",
                  isActive: selectedParagraph?.alignment == .justified && justifySubMode == "나눔",
                  palette: palette
                ) {
                  justifySubMode = "나눔"
                  viewModel.setActiveParagraphAlignment(.justified)
                }
              }

              Divider().frame(height: 16)

              HStack(spacing: 1) {
                ZStack(alignment: .leading) {
                  comboBoxFill
                  FlatToolbarMenu(
                    title: "\(Int(selectedParagraph?.lineSpacingPercent ?? 160))%",
                    palette: palette,
                    isSnapshotMode: viewModel.isCanvasSnapshotMode
                  ) {
                    ForEach([100.0, 115.0, 130.0, 160.0, 180.0, 200.0], id: \.self) { pct in
                      Button("\(Int(pct))%") {
                        viewModel.setActiveParagraphLineSpacing(pct)
                      }
                    }
                  }
                  .padding(.horizontal, 6)
                }
                .frame(width: 58, height: 23)
                .border(comboBorder, width: 1)

                flatToolbarButton("▲", palette: palette) {
                  let cur = selectedParagraph?.lineSpacingPercent ?? 160.0
                  viewModel.setActiveParagraphLineSpacing(min(300.0, cur + 10.0))
                }
                flatToolbarButton("▼", palette: palette) {
                  let cur = selectedParagraph?.lineSpacingPercent ?? 160.0
                  viewModel.setActiveParagraphLineSpacing(max(80.0, cur - 10.0))
                }
              }
            }
            .padding(.horizontal, 8)
          }
        }
        .frame(height: 35)
        Divider()
      }

      if isPrimaryPane && viewModel.isAdvancedToolbarExpanded {
        VStack(alignment: .leading, spacing: 6) {
          AdaptiveScrollContainer(
            axes: .horizontal,
            showsIndicators: false,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          ) {
            HStack(spacing: 8) {
              FlatUnderlineTextField(
                placeholder: "강조 구절 (선택)",
                text: $viewModel.inlineRunTargetPhrase,
                symbolName: "highlighter",
                width: 120,
                palette: palette,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              )

              flatToolbarButton("인라인 강조", palette: palette) {
                viewModel.applyInlineRunFormattingToActiveParagraph(
                  isBold: true,
                  isUnderlined: true,
                  foregroundHex: "#5C6BC0"
                )
              }

              flatToolbarButton("형광펜", palette: palette) {
                viewModel.toggleHighlightOnActiveParagraph()
              }

              flatToolbarButton("취소선", palette: palette) {
                viewModel.toggleStrikethroughOnActiveParagraph()
              }

              FlatUnderlineTextField(
                placeholder: "링크 URL (https://...)",
                text: $viewModel.inlineLinkURLDraft,
                symbolName: "link",
                width: 145,
                isMonospaced: true,
                palette: palette,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              )

              flatToolbarButton("링크 삽입", palette: palette) {
                viewModel.insertHyperlinkIntoActiveParagraph()
              }

              Divider().frame(height: 14)

              flatToolbarButton("들여쓰기 +", palette: palette) {
                viewModel.adjustActiveParagraphIndent(
                  firstLineDeltaPt: 6.0, leftIndentDeltaPt: 12.0)
              }

              flatToolbarButton("내어쓰기 -", palette: palette) {
                viewModel.adjustActiveParagraphIndent(
                  firstLineDeltaPt: -6.0, leftIndentDeltaPt: -12.0)
              }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 2)
          }

          AdaptiveScrollContainer(
            axes: .horizontal,
            showsIndicators: false,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          ) {
            HStack(spacing: 8) {
              FlatUnderlineTextField(
                placeholder: "LaTeX 수식 입력",
                text: $viewModel.equationDraftText,
                symbolName: "function",
                width: 145,
                isMonospaced: true,
                palette: palette,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              )

              flatToolbarButton("수식 삽입", palette: palette) {
                viewModel.insertEquationIntoActiveDocument()
              }

              FlatUnderlineTextField(
                placeholder: "워터마크 문구",
                text: $viewModel.watermarkDraftText,
                symbolName: "drop.triangle",
                width: 120,
                palette: palette,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              )

              flatToolbarButton(watermarkText.isEmpty ? "워터마크 적용" : "워터마크 토글", palette: palette) {
                if !watermarkText.isEmpty
                  && viewModel.watermarkDraftText.trimmingCharacters(in: .whitespacesAndNewlines)
                    .isEmpty
                {
                  viewModel.setDocumentWatermark("")
                } else {
                  viewModel.setDocumentWatermark()
                }
              }

              Divider().frame(height: 14)

              FlatUnderlineTextField(
                placeholder: "머지 키",
                text: $viewModel.mailMergeFieldKeyDraft,
                width: 80,
                palette: palette,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              )

              FlatUnderlineTextField(
                placeholder: "머지 값",
                text: $viewModel.mailMergeFieldValueDraft,
                width: 90,
                palette: palette,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              )

              flatToolbarButton("메일 머지 치환", isProminent: true, palette: palette) {
                viewModel.applyMailMergeFieldReplacement()
              }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 2)
          }
          Divider()
        }
      }

      HStack(spacing: 0) {
        Rectangle()
          .fill(
            palette.isDark
              ? EasyOfficeDesignTokens.resolveHexColor("#2B3037", fallback: .black)
              : EasyOfficeDesignTokens.resolveHexColor("#F2F2F2", fallback: .white)
          )
          .frame(width: 20, height: 20)
          .overlay(
            Text("mm")
              .font(.system(size: 7.5, weight: .semibold, design: .monospaced))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
          )
          .border(palette.hairlineBorder, width: 1)

        DocumentHorizontalRulerView(
          document: document,
          selectedParagraph: selectedParagraph,
          palette: palette,
          paperMaxWidth: paperMaxWidth,
          viewModel: isPrimaryPane ? viewModel : nil
        )
      }

      HStack(alignment: .top, spacing: 0) {
        DocumentVerticalRulerView(palette: palette)

        AdaptiveScrollContainer(
          axes: .vertical,
          showsIndicators: true,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        ) {
          VStack(alignment: .center, spacing: 0) {
            ZStack {
              VStack(alignment: .leading, spacing: 10) {
                HStack {
                  Text(resolvedHeaderText)
                    .font(.system(size: 10, weight: .regular))
                    .foregroundStyle(paperPalette.secondaryText.swiftUIColor)
                  Spacer()
                }
                Divider()

                if isHWPModule {
                  hwpOfficialHeaderBanner(palette: paperPalette)
                }

                if viewModel.isManuscriptGridModeEnabled {
                  HWPManuscriptGridPaperView(
                    document: document,
                    palette: palette,
                    viewModel: isPrimaryPane ? viewModel : nil
                  )
                } else {
                  if viewModel.workspaceViewMode == .outlineView {
                    documentOutlineTreePanel(palette: paperPalette)
                  }
                  if !tocEntries.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                      HStack(spacing: 10) {
                        Text("목차")
                          .font(.system(size: 10, weight: .bold))
                          .lineLimit(1)
                          .fixedSize(horizontal: true, vertical: false)
                          .foregroundStyle(paperPalette.secondaryText.swiftUIColor)

                        AdaptiveScrollContainer(
                          axes: .horizontal,
                          showsIndicators: false,
                          isSnapshotMode: viewModel.isCanvasSnapshotMode
                        ) {
                          HStack(spacing: 14) {
                            ForEach(Array(tocEntries.enumerated()), id: \.element.paragraphID) {
                              idx, entry in
                              Button {
                                viewModel.selectedParagraphID = entry.paragraphID
                                viewModel.updateSelectedParagraphSelection(
                                  paragraphID: entry.paragraphID,
                                  utf16Range: nil
                                )
                              } label: {
                                Text(
                                  viewModel.formattedTableOfContentsEntryTitle(
                                    index: idx,
                                    rawTitle: entry.title
                                  )
                                )
                                .font(.system(size: 10, weight: .medium))
                                .lineLimit(1)
                                .foregroundStyle(
                                  viewModel.selectedParagraphID == entry.paragraphID
                                    ? EasyOfficeDesignTokens.brandAccentToken(
                                      isDark: paperPalette.isDark
                                    ).swiftUIColor
                                    : paperPalette.secondaryText.swiftUIColor
                                )
                              }
                              .buttonStyle(.plain)
                            }
                          }
                        }
                      }
                      Divider()
                    }
                  }

                  let visibleParagraphs: [(Int, ParagraphBlock)] = {
                    if viewModel.workspaceViewMode == .outlineView {
                      let visibleSet = Set(
                        viewModel.visibleDocumentOutlineParagraphs(for: document).map(\.id)
                      )
                      return Array(document.paragraphs.enumerated()).filter {
                        visibleSet.contains($0.element.id)
                      }
                    } else {
                      return Array(document.paragraphs.enumerated())
                    }
                  }()

                  if colCount >= 2 && visibleParagraphs.count >= 2 {
                    let effectiveCols = min(max(2, colCount), max(2, visibleParagraphs.count))
                    let chunkSize = max(
                      1, (visibleParagraphs.count + effectiveCols - 1) / effectiveCols)

                    HStack(alignment: .top, spacing: 20) {
                      ForEach(0..<effectiveCols, id: \.self) { colIdx in
                        let startIdx = min(visibleParagraphs.count, colIdx * chunkSize)
                        let endIdx = min(visibleParagraphs.count, (colIdx + 1) * chunkSize)
                        let colSlice = Array(visibleParagraphs[startIdx..<endIdx])

                        VStack(alignment: .leading, spacing: 6) {
                          ForEach(colSlice, id: \.1.id) { idx, paragraph in
                            paragraphBlockView(
                              for: paragraph, paragraphIndex: idx, palette: paperPalette)
                          }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)

                        if colIdx + 1 < effectiveCols {
                          Rectangle()
                            .fill(paperPalette.border.swiftUIColor)
                            .frame(width: 1)
                        }
                      }
                    }
                  } else {
                    VStack(alignment: .leading, spacing: 6) {
                      ForEach(visibleParagraphs, id: \.1.id) { idx, paragraph in
                        paragraphBlockView(
                          for: paragraph, paragraphIndex: idx, palette: paperPalette)
                      }
                    }
                  }

                  if document.sections.count > 1 {
                    HStack(spacing: 8) {
                      Rectangle()
                        .fill(EasyOfficeDesignTokens.brand400.swiftUIColor.opacity(0.6))
                        .frame(height: 1)
                      Text("구역 나눔 (\(document.sections.count)구역)")
                        .font(.system(size: 9, weight: .semibold, design: .monospaced))
                        .lineLimit(1)
                        .fixedSize(horizontal: true, vertical: false)
                        .foregroundStyle(
                          EasyOfficeDesignTokens.brandAccentToken(isDark: paperPalette.isDark)
                            .swiftUIColor)
                      Rectangle()
                        .fill(EasyOfficeDesignTokens.brand400.swiftUIColor.opacity(0.6))
                        .frame(height: 1)
                    }
                    .padding(.vertical, 2)
                  }
                }

                if !document.equations.isEmpty {
                  VStack(alignment: .center, spacing: 4) {
                    ForEach(document.equations) { eq in
                      compactEquationCardView(for: eq, palette: paperPalette)
                    }
                  }
                  .frame(maxWidth: .infinity)
                }

                let unanchoredImages = document.images.filter { $0.anchorParagraphID == nil }
                if !unanchoredImages.isEmpty {
                  VStack(alignment: .leading, spacing: 4) {
                    ForEach(unanchoredImages) { img in
                      imageWrapCardView(for: img, palette: paperPalette)
                    }
                  }
                }

                let shouldShowProposalDiagram =
                  !isHWPModule
                  && !document.tables.isEmpty
                  && document.paragraphs.contains(where: {
                    $0.text.contains("아키텍처") || $0.text.contains("선정") || $0.text.contains("로드맵")
                  })
                if shouldShowProposalDiagram {
                  StudioProcessFlowDiagramView()
                }

                ForEach(Array(document.tables.enumerated()), id: \.element.id) { tIdx, table in
                  documentTableView(for: table, tableIndex: tIdx, palette: paperPalette)
                }

                if !footnoteItems.isEmpty || !endnoteItems.isEmpty {
                  VStack(alignment: .leading, spacing: 4) {
                    Rectangle()
                      .fill(paperPalette.secondaryText.swiftUIColor.opacity(0.45))
                      .frame(width: 120, height: 0.8)

                    ForEach(footnoteItems) { note in
                      Text("\(note.number)) \(note.text)")
                        .font(.system(size: 9.5))
                        .foregroundStyle(paperPalette.secondaryText.swiftUIColor)
                        .lineLimit(1)
                    }

                    ForEach(endnoteItems) { note in
                      Text("미주 \(note.number)) \(note.text)")
                        .font(.system(size: 9.5))
                        .foregroundStyle(paperPalette.secondaryText.swiftUIColor)
                        .lineLimit(1)
                    }
                  }
                }

                if let mailMergePreviewLine {
                  Text("메일 머지 미리보기 · \(mailMergePreviewLine)")
                    .font(.system(size: 10))
                    .foregroundStyle(paperPalette.secondaryText.swiftUIColor)
                    .lineLimit(1)
                }

                Divider()
                HStack {
                  Spacer()
                  Text(resolvedFooterText)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(paperPalette.secondaryText.swiftUIColor)
                  Spacer()
                }
              }
              .padding(.horizontal, CGFloat(viewModel.documentHorizontalPadding))
              .padding(.vertical, 38)
              .background(paperPalette.cardSurface.swiftUIColor)
              .border(
                EasyOfficeDesignTokens.resolveHexColor("#C8C8C8", fallback: Color.gray),
                width: 1
              )
              .shadow(
                color: Color.black.opacity(palette.isDark ? 0.35 : 0.08),
                radius: 5,
                x: 0,
                y: 2
              )

              if !viewModel.isManuscriptGridModeEnabled {
                Canvas { context, size in
                  let x: CGFloat = max(20.0, CGFloat(viewModel.documentHorizontalPadding) - 6.0)
                  let y: CGFloat = 32.0
                  let arm: CGFloat = 16.0
                  let thick: CGFloat = 1.0
                  let cropColor = EasyOfficeDesignTokens.resolveHexColor(
                    "#D0D0D0", fallback: Color.gray.opacity(0.45))
                  let rects = [
                    CGRect(x: x - arm, y: y, width: arm, height: thick),
                    CGRect(x: x, y: y - arm, width: thick, height: arm),
                    CGRect(x: size.width - x, y: y, width: arm, height: thick),
                    CGRect(x: size.width - x - thick, y: y - arm, width: thick, height: arm),
                    CGRect(x: x - arm, y: size.height - y - thick, width: arm, height: thick),
                    CGRect(x: x, y: size.height - y, width: thick, height: arm),
                    CGRect(
                      x: size.width - x,
                      y: size.height - y - thick,
                      width: arm,
                      height: thick
                    ),
                    CGRect(
                      x: size.width - x - thick,
                      y: size.height - y,
                      width: thick,
                      height: arm
                    ),
                  ]
                  for r in rects {
                    context.fill(Path(r), with: .color(cropColor))
                  }
                }
                .allowsHitTesting(false)
              }

              if !watermarkText.isEmpty {
                Text(watermarkText)
                  .font(.system(size: 38, weight: .black, design: .rounded))
                  .foregroundStyle(EasyOfficeDesignTokens.brand400.swiftUIColor.opacity(0.18))
                  .rotationEffect(.degrees(-28))
                  .allowsHitTesting(false)
              }
            }
            .frame(maxWidth: paperMaxWidth)
          }
          .padding(.vertical, 14)
          .frame(maxWidth: .infinity, alignment: .center)
          .background(scrollBackdropColor)
        }
      }
    }
  }

  private func buildRichParagraphAttributedText(
    for paragraph: ParagraphBlock,
    palette: WorkspaceThemePalette
  ) -> AttributedString {
    var combined = AttributedString()
    if paragraph.firstLineIndentPt.isFinite && paragraph.firstLineIndentPt > 0 {
      let spaceCount = XMLDocumentModelMapper.safeClampedInt(
        round(paragraph.firstLineIndentPt / 6.0), min: 1, max: 12)
      combined.append(AttributedString(String(repeating: " ", count: spaceCount)))
    }
    for run in paragraph.effectiveRuns {
      let safeLink = run.linkURL.flatMap { WorkspaceViewModel.sanitizeSafeHyperlinkURL($0) }
      let hasLink = safeLink != nil
      let displaySegment = hasLink ? "\(run.text) [🔗]" : run.text
      var segment = AttributedString(displaySegment)
      let fontSize = CGFloat(run.fontSizePt ?? paragraph.fontSizePt)
      let fontFamily = (run.fontName ?? paragraph.fontName).trimmingCharacters(
        in: .whitespacesAndNewlines)
      var font: Font
      if !fontFamily.isEmpty && fontFamily != "Pretendard" && fontFamily != "System" {
        font = Font.custom(fontFamily, size: fontSize)
        if run.isBold { font = font.weight(.bold) }
      } else {
        font = Font.system(size: fontSize, weight: run.isBold ? .bold : .regular)
      }
      if run.isItalic {
        font = font.italic()
      }
      segment.font = font
      if let fgHex = run.foregroundHex {
        segment.foregroundColor = EasyOfficeDesignTokens.resolveHexColor(
          fgHex,
          fallback: palette.primaryText.swiftUIColor
        )
      } else {
        segment.foregroundColor = palette.primaryText.swiftUIColor
      }
      if let bgHex = run.backgroundHex {
        segment.backgroundColor = EasyOfficeDesignTokens.resolveHexColor(
          bgHex,
          fallback: palette.elevatedSurface.swiftUIColor
        )
      }
      if run.isUnderlined || hasLink {
        segment.underlineStyle = .single
      }
      if run.isStrikethrough {
        segment.strikethroughStyle = .single
      }
      combined.append(segment)
    }
    if showFormattingMarks {
      var fmtMark = AttributedString(" ¶")
      fmtMark.font = Font.system(size: max(9.0, CGFloat(paragraph.fontSizePt) - 1.5), weight: .bold)
      fmtMark.foregroundColor = EasyOfficeDesignTokens.resolveHexColor(
        "#D97706",
        fallback: palette.secondaryText.swiftUIColor
      )
      combined.append(fmtMark)
    }
    if showParagraphMarks {
      var mark = AttributedString(" ↵")
      mark.font = Font.system(size: max(9.0, CGFloat(paragraph.fontSizePt) - 1.5), weight: .semibold)
      mark.foregroundColor = EasyOfficeDesignTokens.resolveHexColor(
        "#0284C7",
        fallback: palette.secondaryText.swiftUIColor
      )
      combined.append(mark)
    }
    return combined
  }

  @ViewBuilder
  private func boxedSectionHeadingView(
    for paragraph: ParagraphBlock,
    paragraphIndex: Int,
    isEditing: Bool,
    palette: WorkspaceThemePalette
  ) -> some View {
    let number = headingOrderNumber(upTo: paragraphIndex)
    let rawTitle = paragraph.text.trimmingCharacters(in: .whitespacesAndNewlines)
    let showNumberCell = !rawTitle.hasPrefix("제목:")
    let prefixCandidate = "\(number). "
    let displayTitle =
      (showNumberCell && rawTitle.hasPrefix(prefixCandidate))
      ? String(rawTitle.dropFirst(prefixCandidate.count))
      : rawTitle
    let displayParagraph: ParagraphBlock = {
      var copy = paragraph
      if displayTitle != paragraph.text && paragraph.inlineRuns.isEmpty {
        copy.text = displayTitle
      }
      return copy
    }()
    let borderColor = EasyOfficeDesignTokens.resolveHexColor("#B8A082", fallback: Color.gray)
    let numberBg = EasyOfficeDesignTokens.resolveHexColor("#F2F2F2", fallback: Color.gray.opacity(0.15))
    let titleBg = EasyOfficeDesignTokens.resolveHexColor("#F5E6D3", fallback: Color.orange.opacity(0.15))
    let textColor = EasyOfficeDesignTokens.resolveHexColor("#111827", fallback: .black)

    HStack(spacing: 0) {
      if showNumberCell {
        ZStack {
          numberBg
          Text("\(number)")
            .font(.system(size: 13, weight: .bold))
            .foregroundStyle(textColor)
        }
        .frame(width: 28, height: 28)

        Rectangle()
          .fill(borderColor)
          .frame(width: 1, height: 28)
      }

      ZStack(alignment: .leading) {
        titleBg
        if isEditing {
          HStack(spacing: 4) {
            SelectableParagraphTextView(
              viewModel: viewModel,
              paragraph: paragraph,
              palette: palette
            )
            .frame(height: 24)
            if showFormattingMarks {
              Text(" ¶")
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(EasyOfficeDesignTokens.resolveHexColor("#D97706", fallback: .orange))
            }
            if showParagraphMarks {
              Text("↵")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(EasyOfficeDesignTokens.resolveHexColor("#0284C7", fallback: .blue))
            }
          }
          .padding(.horizontal, 8)
        } else {
          Text(buildRichParagraphAttributedText(for: displayParagraph, palette: palette))
            .lineLimit(1)
            .padding(.horizontal, 10)
        }
      }
      .frame(maxWidth: .infinity, minHeight: 28, maxHeight: 28, alignment: .leading)
    }
    .border(borderColor, width: 1)
    .padding(.top, 4)
    .padding(.bottom, 2)
  }

  @ViewBuilder
  private func paragraphRow(
    for paragraph: ParagraphBlock,
    paragraphIndex: Int,
    palette: WorkspaceThemePalette
  ) -> some View {
    let lineSpacingMultiple = max(0.8, paragraph.lineSpacingPercent / 100.0)
    let listPrefix = document.formattedListPrefix(forParagraphAt: paragraphIndex)
    let listIndentPt = paragraph.listKind == .none ? 0.0 : CGFloat(paragraph.listLevel) * 14.0
    let kernPt = CGFloat(paragraph.fontSizePt * (paragraph.letterSpacingPercent / 100.0))
    let isSectionHeading = (paragraph.style == .heading1 || paragraph.style == .heading2)

    if isSectionHeading {
      boxedSectionHeadingView(
        for: paragraph,
        paragraphIndex: paragraphIndex,
        isEditing: false,
        palette: palette
      )
    } else {
      HStack(alignment: .firstTextBaseline, spacing: 6) {
        if let listPrefix {
          Text(listPrefix)
            .font(.system(size: CGFloat(paragraph.fontSizePt), weight: .bold, design: .monospaced))
            .foregroundStyle(
              EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor)
        }
        Text(buildRichParagraphAttributedText(for: paragraph, palette: palette))
          .tracking(kernPt)
          .lineSpacing(CGFloat(max(0.0, (lineSpacingMultiple - 1.0) * paragraph.fontSizePt * 0.4)))
          .frame(maxWidth: .infinity, alignment: swiftUIAlignment(for: paragraph.alignment))
      }
      .padding(.leading, CGFloat(max(0.0, paragraph.leftIndentPt)) + listIndentPt)
    }
  }

  @ViewBuilder
  private func paragraphBlockView(
    for paragraph: ParagraphBlock,
    paragraphIndex: Int,
    palette: WorkspaceThemePalette
  ) -> some View {
    let isSelectedParagraph = viewModel.selectedParagraphID == paragraph.id
    let listPrefix = document.formattedListPrefix(forParagraphAt: paragraphIndex)
    let rowHeight = compactParagraphHeight(for: paragraph)
    let linkedRuns = paragraph.effectiveRuns.filter { ($0.linkURL ?? "").isEmpty == false }
    let anchoredImages = document.images.filter { $0.anchorParagraphID == paragraph.id }
    let isSectionHeading = (paragraph.style == .heading1 || paragraph.style == .heading2)

    VStack(alignment: .leading, spacing: 4) {
      HStack(alignment: .center, spacing: 6) {
        if isSectionHeading {
          boxedSectionHeadingView(
            for: paragraph,
            paragraphIndex: paragraphIndex,
            isEditing: isPrimaryPane && isSelectedParagraph && !viewModel.isCanvasSnapshotMode,
            palette: palette
          )
          .contentShape(Rectangle())
          .onTapGesture {
            guard isPrimaryPane else { return }
            viewModel.updateSelectedParagraphSelection(paragraphID: paragraph.id, utf16Range: nil)
          }
        } else if isPrimaryPane && isSelectedParagraph && !viewModel.isCanvasSnapshotMode {
          HStack(alignment: .top, spacing: 6) {
            if let listPrefix {
              Text(listPrefix)
                .font(
                  .system(size: CGFloat(paragraph.fontSizePt), weight: .bold, design: .monospaced)
                )
                .foregroundStyle(EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor)
                .padding(.leading, CGFloat(paragraph.listLevel) * 14.0)
                .padding(.top, 2)
            }
            SelectableParagraphTextView(
              viewModel: viewModel,
              paragraph: paragraph,
              palette: palette
            )
            .frame(height: rowHeight)
            if showFormattingMarks {
              Text(" ¶")
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(
                  EasyOfficeDesignTokens.resolveHexColor(
                    "#D97706", fallback: palette.secondaryText.swiftUIColor)
                )
                .padding(.top, 4)
            }
            if showParagraphMarks {
              Text("↵")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(
                  EasyOfficeDesignTokens.resolveHexColor(
                    "#0284C7", fallback: palette.secondaryText.swiftUIColor)
                )
                .padding(.top, 4)
            }
          }
        } else {
          paragraphRow(for: paragraph, paragraphIndex: paragraphIndex, palette: palette)
            .contentShape(Rectangle())
            .onTapGesture {
              guard isPrimaryPane else { return }
              viewModel.updateSelectedParagraphSelection(paragraphID: paragraph.id, utf16Range: nil)
            }
        }

        if let firstLinkRun = linkedRuns.first,
          let rawURL = firstLinkRun.linkURL,
          let safeURLString = WorkspaceViewModel.sanitizeSafeHyperlinkURL(rawURL)
        {
          Button {
            viewModel.lastExecutedCommandSummary = "안전 하이퍼링크 이동: \(safeURLString)"
            if !viewModel.isCanvasSnapshotMode, let url = URL(string: safeURLString) {
              NSWorkspace.shared.open(url)
            }
          } label: {
            Label("링크", systemImage: "link")
              .font(.system(size: 9, weight: .semibold))
              .foregroundStyle(EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor)
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
          }
          .buttonStyle(.plain)
          .help(safeURLString)
        }
      }

      if !anchoredImages.isEmpty {
        ForEach(anchoredImages) { img in
          imageWrapCardView(for: img, palette: palette)
        }
      }
    }
    .padding(.vertical, 2)
  }

  @ViewBuilder
  private func compactEquationCardView(
    for eq: DocumentEquationBlock,
    palette: WorkspaceThemePalette
  ) -> some View {
    HStack(spacing: 10) {
      Spacer(minLength: 0)
      Text(eq.displayText)
        .font(.system(size: 14, weight: .regular, design: .serif))
        .lineLimit(1)
        .foregroundStyle(palette.primaryText.swiftUIColor)

      Text("(\(eq.displayTitle))")
        .font(.system(size: 10, weight: .medium))
        .lineLimit(1)
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      Spacer(minLength: 0)
    }
    .padding(.vertical, 4)
  }

  @ViewBuilder
  private func imageWrapCardView(for img: EmbeddedImageAttachment, palette: WorkspaceThemePalette)
    -> some View
  {
    HStack(spacing: 10) {
      if let nsImg = NSImage(data: img.imageData) {
        Image(nsImage: nsImg)
          .resizable()
          .interpolation(.high)
          .aspectRatio(contentMode: .fit)
          .frame(width: 58, height: 34)
      } else {
        Image(systemName: "photo.artframe")
          .font(.system(size: 16))
          .foregroundStyle(EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor)
          .frame(width: 44, height: 34)
      }

      VStack(alignment: .leading, spacing: 2) {
        Text("\(img.fileName) · \(img.wrapMode.rawValue)")
          .font(.system(size: 10, weight: .semibold))
          .lineLimit(1)
          .foregroundStyle(palette.primaryText.swiftUIColor)

        Text(
          img.caption.isEmpty
            ? "\(Int(img.widthPt))×\(Int(img.heightPt))pt"
            : img.caption
        )
        .font(.system(size: 9))
        .lineLimit(1)
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      }

      Spacer(minLength: 4)

      if isPrimaryPane {
        Button {
          viewModel.cycleImageWrapMode(imageID: img.id)
        } label: {
          Text("배치 전환")
            .font(.system(size: 9.5, weight: .semibold))
            .foregroundStyle(EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor)
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
        }
        .buttonStyle(.plain)
      }
    }
    .padding(.vertical, 4)
  }

  @ViewBuilder
  private func documentTableView(
    for table: DocumentTableBlock,
    tableIndex tIdx: Int,
    palette: WorkspaceThemePalette
  ) -> some View {
    VStack(alignment: .leading, spacing: 4) {
      Text(table.caption)
        .font(.system(size: 10, weight: .semibold))
        .lineLimit(1)
        .foregroundStyle(palette.secondaryText.swiftUIColor)
        .onTapGesture {
          guard isPrimaryPane else { return }
          viewModel.selectDocumentTable(index: tIdx)
        }

      VStack(spacing: 0) {
        ForEach(Array(table.rows.enumerated()), id: \.offset) { rowIdx, row in
          HStack(spacing: 0) {
            ForEach(Array(row.enumerated()), id: \.offset) { colIdx, cell in
              if !table.isCoveredByNonOriginSpan(atRow: rowIdx, column: colIdx) {
                let span = table.originSpan(atRow: rowIdx, column: colIdx)
                let borderStyle = table.effectiveBorderStyle(atRow: rowIdx, column: colIdx)
                let isHeader = rowIdx == 0 && table.hasHeaderRow
                let cellBg =
                  isHeader
                  ? EasyOfficeDesignTokens.resolveHexColor("#F2F2F2", fallback: palette.elevatedSurface.swiftUIColor)
                  : Color.clear
                HStack(spacing: 4) {
                  if isPrimaryPane && !viewModel.isCanvasSnapshotMode {
                    TextField(
                      "",
                      text: Binding(
                        get: { cell },
                        set: { updated in
                          viewModel.selectDocumentTableCell(
                            tableIndex: tIdx,
                            row: rowIdx,
                            column: colIdx
                          )
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
                    .font(.system(size: 11, weight: isHeader ? .bold : .regular))
                    .foregroundStyle(
                      isHeader
                        ? EasyOfficeDesignTokens.resolveHexColor("#111827", fallback: .black)
                        : palette.primaryText.swiftUIColor
                    )
                  } else {
                    Text(cell)
                      .font(.system(size: 11, weight: isHeader ? .bold : .regular))
                      .foregroundStyle(
                        isHeader
                          ? EasyOfficeDesignTokens.resolveHexColor("#111827", fallback: .black)
                          : palette.primaryText.swiftUIColor
                      )
                      .lineLimit(2)
                      .fixedSize(horizontal: false, vertical: true)
                  }
                  Spacer(minLength: 0)
                  if let span, span.rowSpan > 1 || span.columnSpan > 1 {
                    Text("\(span.rowSpan)×\(span.columnSpan)")
                      .font(.system(size: 8, weight: .bold, design: .monospaced))
                      .foregroundStyle(palette.secondaryText.swiftUIColor)
                  }
                }
                .frame(maxWidth: .infinity, minHeight: 24, alignment: .leading)
                .padding(.vertical, 3)
                .layoutPriority(Double(span?.columnSpan ?? 1))
                .padding(.horizontal, 6)
                .background(cellBg)
                .contentShape(Rectangle())
                .simultaneousGesture(
                  TapGesture().onEnded {
                    guard isPrimaryPane else { return }
                    viewModel.selectDocumentTableCell(
                      tableIndex: tIdx,
                      row: rowIdx,
                      column: colIdx
                    )
                  }
                )
                .overlay(
                  tableCellBorderOverlay(
                    style: borderStyle,
                    color: EasyOfficeDesignTokens.resolveHexColor("#9CA3AF", fallback: palette.border.swiftUIColor)
                  )
                )
              }
            }
          }
        }
      }
    }
    .padding(.vertical, 4)
  }

  @ViewBuilder
  private func tableCellBorderOverlay(style: TableCellBorderStyle, color: Color) -> some View {
    switch style {
    case .solid:
      Color.clear.border(color, width: 0.5)
    case .doubleLine:
      ZStack {
        Color.clear.border(color, width: 0.6)
        Color.clear.padding(1.5).border(color, width: 0.6)
      }
    case .dashed:
      Color.clear.border(color.opacity(0.55), width: 0.6)
    case .diagonalDown, .diagonalUp:
      GeometryReader { geo in
        ZStack {
          Color.clear.border(color, width: 0.5)
          Path { p in
            if style == .diagonalDown {
              p.move(to: .zero)
              p.addLine(to: CGPoint(x: geo.size.width, y: geo.size.height))
            } else {
              p.move(to: CGPoint(x: 0, y: geo.size.height))
              p.addLine(to: CGPoint(x: geo.size.width, y: 0))
            }
          }
          .stroke(color.opacity(0.65), lineWidth: 0.6)
        }
      }
    case .none:
      EmptyView()
    }
  }

  private func documentOutlineTreePanel(palette: WorkspaceThemePalette) -> some View {
    let outlineItems = viewModel.visibleDocumentOutlineParagraphs(for: document)
    return VStack(alignment: .leading, spacing: 5) {
      HStack(spacing: 8) {
        Image(systemName: "list.bullet.indent")
          .font(.system(size: 11, weight: .bold))
          .foregroundStyle(EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor)
        Text("문서 윤곽 계층")
          .font(.system(size: 11, weight: .bold))
          .foregroundStyle(palette.primaryText.swiftUIColor)
        Spacer()
        flatToolbarButton("모두 펼치기", palette: palette) {
          viewModel.collapsedOutlineParagraphIDs.removeAll()
        }
        flatToolbarButton("모두 접기", palette: palette) {
          let headingIDs = document.paragraphs
            .filter { $0.style == .heading1 || $0.style == .heading2 }
            .map(\.id)
          viewModel.collapsedOutlineParagraphIDs = Set(headingIDs)
        }
      }

      ForEach(Array(outlineItems.enumerated()), id: \.element.id) { idx, p in
        let isHeading = (p.style == .heading1 || p.style == .heading2)
        let isCollapsed = viewModel.collapsedOutlineParagraphIDs.contains(p.id)
        let levelIndent: CGFloat = {
          switch p.style {
          case .title, .officialHeader: return 0
          case .heading1: return 4
          case .heading2: return 14
          case .heading3: return 22
          case .body: return 28
          case .quote: return 36
          }
        }()
        let levelBadge: String = {
          switch p.style {
          case .title: return "표제"
          case .officialHeader: return "두문"
          case .heading1: return "수준 1"
          case .heading2: return "수준 2"
          case .heading3: return "수준 3"
          case .body: return "본문"
          case .quote: return "인용"
          }
        }()

        HStack(spacing: 6) {
          Spacer().frame(width: levelIndent)
          if isHeading {
            Button {
              if isCollapsed {
                viewModel.collapsedOutlineParagraphIDs.remove(p.id)
              } else {
                viewModel.collapsedOutlineParagraphIDs.insert(p.id)
              }
            } label: {
              Image(systemName: isCollapsed ? "chevron.right" : "chevron.down")
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor)
                .frame(width: 14, height: 14)
            }
            .buttonStyle(.plain)
          } else {
            Circle()
              .fill(palette.secondaryText.swiftUIColor.opacity(0.6))
              .frame(width: 5, height: 5)
              .padding(.horizontal, 4.5)
          }

          Text(levelBadge)
            .font(.system(size: 9, weight: .bold, design: .monospaced))
            .foregroundStyle(
              isHeading
                ? EasyOfficeDesignTokens.brandAccentToken(isDark: palette.isDark).swiftUIColor
                : palette.secondaryText.swiftUIColor
            )

          Button {
            viewModel.selectedParagraphID = p.id
          } label: {
            Text(p.text.isEmpty ? "(빈 단락 \(idx + 1))" : p.text)
              .font(.system(size: 10.5, weight: isHeading ? .semibold : .regular))
              .foregroundStyle(palette.primaryText.swiftUIColor)
              .lineLimit(1)
          }
          .buttonStyle(.plain)

          Spacer(minLength: 4)

          Text("\(p.text.count)자")
            .font(.system(size: 9, design: .monospaced))
            .foregroundStyle(palette.secondaryText.swiftUIColor)

          if isPrimaryPane {
            flatToolbarButton("◀ 승격", palette: palette) {
              viewModel.promoteOrDemoteParagraphOutlineLevel(paragraphID: p.id, delta: -1)
            }

            flatToolbarButton("강등 ▶", palette: palette) {
              viewModel.promoteOrDemoteParagraphOutlineLevel(paragraphID: p.id, delta: 1)
            }
          }
        }
        .padding(.vertical, 2)
      }
      Divider()
    }
    .padding(.vertical, 4)
  }
}

func resolveOptionalNSColor(_ hex: String?) -> NSColor? {
  guard let hex, let rgb = EasyOfficeDesignTokens.parseHexRGB(hex) else { return nil }
  return NSColor(
    srgbRed: CGFloat(rgb.r) / 255.0,
    green: CGFloat(rgb.g) / 255.0,
    blue: CGFloat(rgb.b) / 255.0,
    alpha: 1.0
  )
}
