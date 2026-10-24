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
      parent.viewModel.lastExecutedCommandSummary = "안전 하이퍼링크 검증 완료: \(safeURL.absoluteString)"
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
      context.coordinator.isUpdatingProgrammatically = false
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
      var font =
        NSFont(name: fontFamily, size: fontSize)
        ?? NSFont.systemFont(ofSize: fontSize, weight: run.isBold ? .bold : .regular)
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

/// Crisp 24pt-high horizontal page ruler displaying left/right page margin shaded zones, centimeter ticks (`0..16 cm`) with millimeter sub-ticks, and active paragraph indent markers (`▽` first-line indent, `△` left indent).
public struct DocumentHorizontalRulerView: View {
  public let document: OfficeDocumentModel
  public let selectedParagraph: ParagraphBlock?
  public let palette: WorkspaceThemePalette
  public var viewModel: WorkspaceViewModel?

  public init(
    document: OfficeDocumentModel,
    selectedParagraph: ParagraphBlock?,
    palette: WorkspaceThemePalette,
    viewModel: WorkspaceViewModel? = nil
  ) {
    self.document = document
    self.selectedParagraph = selectedParagraph
    self.palette = palette
    self.viewModel = viewModel
  }

  public var body: some View {
    let customPadding = viewModel?.documentHorizontalPadding ?? 28.0
    let leftMarginPt: CGFloat = max(24.0, min(96.0, CGFloat(customPadding + 26.0)))
    let rightMarginPt: CGFloat = leftMarginPt
    let firstLineOffsetPt = CGFloat(selectedParagraph?.firstLineIndentPt ?? 0.0)
    let leftIndentOffsetPt = CGFloat(selectedParagraph?.leftIndentPt ?? 0.0)

    GeometryReader { geo in
      let totalWidth = max(240.0, geo.size.width)
      let bodyWidth = max(80.0, totalWidth - leftMarginPt - rightMarginPt)
      let cmStep = bodyWidth / 16.0

      ZStack(alignment: .topLeading) {
        RoundedRectangle(cornerRadius: 4)
          .fill(palette.cardSurface.swiftUIColor)

        // Left & Right Page Margin Shaded Zones
        Rectangle()
          .fill(palette.selectedSurface.swiftUIColor.opacity(0.65))
          .frame(width: leftMarginPt, height: 24)

        Rectangle()
          .fill(palette.selectedSurface.swiftUIColor.opacity(0.65))
          .frame(width: rightMarginPt, height: 24)
          .offset(x: totalWidth - rightMarginPt)

        // Centimeter & Millimeter Tick Marks
        Canvas { context, size in
          let tickColor = palette.secondaryText.swiftUIColor
          for mmIndex in 0...160 {
            let xPos = leftMarginPt + (CGFloat(mmIndex) / 10.0) * cmStep
            guard xPos <= totalWidth - rightMarginPt + 1.0 else { continue }
            let isCm = (mmIndex % 10 == 0)
            let isHalfCm = (mmIndex % 5 == 0) && !isCm
            let tickHeight: CGFloat = isCm ? 9.0 : (isHalfCm ? 6.0 : 3.0)
            var tickPath = Path()
            tickPath.move(to: CGPoint(x: xPos, y: size.height - tickHeight))
            tickPath.addLine(to: CGPoint(x: xPos, y: size.height))
            context.stroke(
              tickPath,
              with: .color(tickColor.opacity(isCm ? 0.85 : 0.45)),
              lineWidth: isCm ? 1.0 : 0.6
            )
          }
        }

        // Centimeter Labels (0 .. 16 cm)
        ForEach(0...16, id: \.self) { cm in
          let xPos = leftMarginPt + CGFloat(cm) * cmStep
          if xPos <= totalWidth - rightMarginPt + 4.0 {
            Text("\(cm)")
              .font(.system(size: 8, weight: .semibold, design: .monospaced))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
              .position(x: xPos, y: 7)
          }
        }

        // Interactive Indent & Margin Markers (▽ First-Line Indent, △ Left Margin/Indent, ▽ Right Margin)
        Text("▽")
          .font(.system(size: 9, weight: .bold))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
          .position(
            x: min(
              totalWidth - rightMarginPt, leftMarginPt + leftIndentOffsetPt + firstLineOffsetPt),
            y: 5
          )
          .gesture(
            DragGesture(minimumDistance: 2)
              .onChanged { drag in
                let newPadding = max(16.0, min(96.0, Double(drag.location.x) - 26.0))
                viewModel?.setDocumentHorizontalPadding(newPadding)
              }
          )
          .onTapGesture {
            let nextPadding = customPadding >= 56.0 ? 28.0 : (customPadding + 14.0)
            viewModel?.setDocumentHorizontalPadding(nextPadding)
          }
          .help("첫 줄 들여쓰기 · 드래그 또는 클릭하여 여백 조절 (\(Int(customPadding))pt)")

        Text("△")
          .font(.system(size: 9, weight: .bold))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
          .position(
            x: min(totalWidth - rightMarginPt, leftMarginPt + leftIndentOffsetPt),
            y: 19
          )
          .gesture(
            DragGesture(minimumDistance: 2)
              .onChanged { drag in
                let newPadding = max(16.0, min(96.0, Double(drag.location.x) - 26.0))
                viewModel?.setDocumentHorizontalPadding(newPadding)
              }
          )
          .onTapGesture {
            let nextPadding = customPadding >= 56.0 ? 28.0 : (customPadding + 14.0)
            viewModel?.setDocumentHorizontalPadding(nextPadding)
          }
          .help("왼쪽 여백·들여쓰기 마커 (\(Int(customPadding))pt)")

        Text("▽")
          .font(.system(size: 9, weight: .bold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
          .position(x: totalWidth - rightMarginPt, y: 19)
          .gesture(
            DragGesture(minimumDistance: 2)
              .onChanged { drag in
                let distFromRight = max(24.0, min(96.0, Double(totalWidth - drag.location.x)))
                viewModel?.setDocumentHorizontalPadding(max(16.0, distFromRight - 26.0))
              }
          )
          .help("오른쪽 여백 경계 (\(Int(rightMarginPt))pt)")
      }
      .overlay(
        RoundedRectangle(cornerRadius: 4)
          .stroke(palette.border.swiftUIColor, lineWidth: 1)
      )
      .clipShape(RoundedRectangle(cornerRadius: 4))
    }
    .frame(height: 24)
  }
}

extension WorkspaceThemePalette {
  fileprivate var hairlineBorder: RGBColorToken {
    border
  }
}

/// Vertical page ruler (`width: 22`, `0..24 cm`) displaying top/bottom margin shaded zones (`height: 22`), `0` origin marker, 1cm major ticks (`1..24`) with monospaced `8pt` numeral labels, and `0.5cm` minor ticks along the right border.
public struct DocumentVerticalRulerView: View {
  public let palette: WorkspaceThemePalette

  public init(palette: WorkspaceThemePalette) {
    self.palette = palette
  }

  public var body: some View {
    VStack(spacing: 0) {
      // Top Margin Shaded Zone (height: 22) with 0cm Origin Marker
      ZStack(alignment: .bottomTrailing) {
        Rectangle()
          .fill(palette.elevatedSurface.swiftUIColor)
          .frame(width: 22, height: 22)

        Text("0")
          .font(.system(size: 7.5, weight: .semibold, design: .monospaced))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
          .frame(width: 15, alignment: .center)
          .offset(x: -5, y: -2)

        Rectangle()
          .fill(palette.hairlineBorder.swiftUIColor)
          .frame(height: 0.8)
      }

      // 1cm Major Ticks (1..24) & 0.5cm Minor Ticks along the right border
      ForEach(1...24, id: \.self) { cm in
        ZStack(alignment: .topTrailing) {
          // 1cm Major Tick at Top
          Rectangle()
            .fill(palette.secondaryText.swiftUIColor.opacity(0.75))
            .frame(width: 6, height: 0.9)

          // Numeral Label (1..24)
          Text("\(cm)")
            .font(.system(size: 8, weight: .semibold, design: .monospaced))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
            .frame(width: 15, alignment: .center)
            .offset(x: -5, y: 1)

          // 0.5cm Minor Tick at Mid-Height
          Rectangle()
            .fill(palette.hairlineBorder.swiftUIColor)
            .frame(width: 3.5, height: 0.7)
            .offset(y: 8)
        }
        .frame(width: 22, height: 16.5)
      }

      // Bottom Margin Shaded Zone (height: 22)
      Rectangle()
        .fill(palette.elevatedSurface.swiftUIColor)
        .frame(width: 22, height: 22)
        .overlay(alignment: .top) {
          Rectangle()
            .fill(palette.hairlineBorder.swiftUIColor)
            .frame(height: 0.8)
        }
    }
    .frame(width: 22)
    .background(palette.cardSurface.swiftUIColor)
    .overlay(
      RoundedRectangle(cornerRadius: 4)
        .stroke(palette.hairlineBorder.swiftUIColor, lineWidth: 1)
    )
    .clipShape(RoundedRectangle(cornerRadius: 4))
  }
}

/// Korean 200-Character Manuscript Grid Paper View (`20×10 = 200자` 원고지 격자 뷰) with traditional reddish-amber borders and inter-row spacing strips.
public struct HWPManuscriptGridPaperView: View {
  public let document: OfficeDocumentModel
  public let palette: WorkspaceThemePalette

  public init(document: OfficeDocumentModel, palette: WorkspaceThemePalette) {
    self.document = document
    self.palette = palette
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
    let maxPages = 16
    while offset < cells.count && pages.count < maxPages {
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

    VStack(alignment: .leading, spacing: 18) {
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
                    .overlay(
                      Rectangle()
                        .stroke(gridStrokeColor.opacity(0.65), lineWidth: 0.7)
                    )
                }
              }
              .overlay(
                Rectangle()
                  .stroke(gridStrokeColor, lineWidth: 1.0)
              )
            }
          }
          .padding(8)
          .background(palette.elevatedSurface.swiftUIColor)
          .overlay(
            RoundedRectangle(cornerRadius: 4)
              .stroke(gridStrokeColor, lineWidth: 1.6)
          )
        }
      }
    }
  }
}

/// Rich interactive editor pane for Document (.docx/.odt/.md) and Korean Office (.hwp/.hwpx) modules.
public struct DocumentEditorDetailPaneView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  let document: OfficeDocumentModel
  let isPrimaryPane: Bool

  public init(
    viewModel: WorkspaceViewModel, document: OfficeDocumentModel, isPrimaryPane: Bool = true
  ) {
    self.viewModel = viewModel
    self.document = document
    self.isPrimaryPane = isPrimaryPane
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
      return "제목 1"
    case .heading2:
      return "제목 2"
    case .heading3:
      return "제목 3"
    case .body:
      return "본문"
    case .quote:
      return "인용문"
    case .officialHeader:
      return "공문서 두문"
    }
  }

  private func compactParagraphHeight(for paragraph: ParagraphBlock) -> CGFloat {
    let isSplitActive = viewModel.splitSecondaryTab != nil
    let explicitLines = paragraph.text.components(separatedBy: .newlines).count
    let charsPerLine = isSplitActive ? 42 : 62
    let wrappedLines = max(1, (paragraph.text.count + charsPerLine - 1) / charsPerLine)
    let lineCount = max(explicitLines, wrappedLines)
    switch paragraph.style {
    case .title:
      return max(isSplitActive ? 32 : 30, CGFloat(lineCount) * 26)
    case .heading1, .heading2, .officialHeader:
      return max(24, CGFloat(lineCount) * 22)
    case .heading3, .body, .quote:
      return max(24, CGFloat(lineCount) * 22)
    }
  }

  @ViewBuilder
  private func hwpOfficialHeaderBanner(palette: WorkspaceThemePalette) -> some View {
    let hwpAccent = EasyOfficeDesignTokens.accentToken(for: .hwp).swiftUIColor
    HStack(alignment: .center, spacing: 12) {
      VStack(alignment: .leading, spacing: 3) {
        HStack(spacing: 6) {
          Text("공문서 두문 · 행정표준 (KS X 6101 OWPML)")
            .font(.system(size: 10, weight: .bold))
            .foregroundStyle(hwpAccent)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(hwpAccent.opacity(0.14))
            .clipShape(RoundedRectangle(cornerRadius: 4))

          Text("문서번호: 기획조정-2026-0104호")
            .font(.system(size: 9.5, weight: .medium, design: .monospaced))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
        }
        Text(document.title)
          .font(.system(size: 13, weight: .bold))
          .foregroundStyle(palette.primaryText.swiftUIColor)
          .lineLimit(1)
      }

      Spacer(minLength: 8)

      HStack(spacing: 0) {
        ForEach(["담당", "검토", "결재"], id: \.self) { role in
          VStack(spacing: 2) {
            Text(role)
              .font(.system(size: 8.5, weight: .bold))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
              .frame(width: 44, height: 14)
              .background(palette.elevatedSurface.swiftUIColor)
            Text(role == "결재" ? "승인" : "완료")
              .font(.system(size: 9, weight: .bold))
              .foregroundStyle(role == "결재" ? hwpAccent : palette.primaryText.swiftUIColor)
              .frame(width: 44, height: 22)
              .background(palette.cardSurface.swiftUIColor)
          }
          .overlay(
            Rectangle()
              .stroke(hwpAccent.opacity(0.45), lineWidth: 0.8)
          )
        }
      }
      .clipShape(RoundedRectangle(cornerRadius: 4))
    }
    .padding(.horizontal, 10)
    .padding(.vertical, 6)
    .background(palette.elevatedSurface.swiftUIColor.opacity(0.65))
    .overlay(
      RoundedRectangle(cornerRadius: 6)
        .stroke(hwpAccent.opacity(0.45), lineWidth: 1)
    )
    .clipShape(RoundedRectangle(cornerRadius: 6))
  }

  public var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
    let tocEntries = document.generateTableOfContents()
    let mailMergePreviewLine = firstMailMergePreviewLine()
    let colCount = document.sections.first?.columnCount ?? 1
    let currentOrientation = document.sections.first?.orientation ?? .portrait
    let isLandscape = currentOrientation == .landscape
    let paperMaxWidth: CGFloat = isLandscape ? 980 : 840
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
    let isSplitActive = viewModel.splitSecondaryTab != nil

    VStack(alignment: .leading, spacing: 8) {
      // 1. Single Compact 1-Line Context Bar (height: 34, Zero Clipping even in Split View)
      AdaptiveScrollContainer(
        axes: .horizontal,
        showsIndicators: false,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      ) {
        HStack(spacing: isSplitActive ? 4 : 6) {
          Text("A4 \(isLandscape ? "가로" : "세로") · \(colCount)단 · \(totalPages)쪽")
            .font(.system(size: 10, weight: .bold, design: .monospaced))
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
            .foregroundStyle(palette.accentBadgeText.swiftUIColor)
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .background(palette.selectedSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 4))

          if isPrimaryPane {
            LavenderMenuPill(
              title: shortStyleTitle(for: selectedParagraph?.style ?? .body),
              symbolName: "textformat.size",
              palette: palette,
              isSnapshotMode: viewModel.isCanvasSnapshotMode
            ) {
              ForEach(
                [
                  ParagraphStyleRole.title,
                  .heading1,
                  .heading2,
                  .body,
                  .quote,
                  .officialHeader,
                ],
                id: \.self
              ) { role in
                Button(role.rawValue) {
                  viewModel.setActiveParagraphStyle(role)
                }
              }
            }

            if isSplitActive {
              LavenderMenuPill(
                title: "정렬",
                symbolName: "text.alignleft",
                palette: palette,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              ) {
                Button("좌측 정렬") {
                  viewModel.setActiveParagraphAlignment(.leading)
                }
                Button("가운데 정렬") {
                  viewModel.setActiveParagraphAlignment(.center)
                }
                Button("우측 정렬") {
                  viewModel.setActiveParagraphAlignment(.trailing)
                }
                Button("양쪽 정렬") {
                  viewModel.setActiveParagraphAlignment(.justified)
                }
              }
            } else {
              HStack(spacing: 2) {
                Button("좌") {
                  viewModel.setActiveParagraphAlignment(.leading)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)

                Button("중") {
                  viewModel.setActiveParagraphAlignment(.center)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)

                Button("우") {
                  viewModel.setActiveParagraphAlignment(.trailing)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)

                Button("양쪽") {
                  viewModel.setActiveParagraphAlignment(.justified)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
              }
            }

            LavenderMenuPill(
              title: "줄 \(Int(selectedParagraph?.lineSpacingPercent ?? 160))%",
              symbolName: "arrow.up.and.down.text.horizontal",
              palette: palette,
              isSnapshotMode: viewModel.isCanvasSnapshotMode
            ) {
              ForEach([100.0, 115.0, 160.0, 200.0], id: \.self) { pct in
                Button("\(Int(pct))%") {
                  viewModel.setActiveParagraphLineSpacing(pct)
                }
              }
            }

            LavenderMenuPill(
              title: "목록",
              symbolName: "list.bullet.indent",
              palette: palette,
              isSnapshotMode: viewModel.isCanvasSnapshotMode
            ) {
              Button("없음") {
                viewModel.setActiveParagraphListKind(.none)
              }
              Button("• 글머리") {
                viewModel.setActiveParagraphListKind(.bullet)
              }
              Button("1. 번호") {
                viewModel.setActiveParagraphListKind(.numbered)
              }
              Button("가. 공문서") {
                viewModel.setActiveParagraphListKind(.koreanOfficial)
              }
              Divider()
              Button("수준 +1") {
                let currentKind = selectedParagraph?.listKind ?? .numbered
                let resolvedKind = currentKind == .none ? .numbered : currentKind
                viewModel.setActiveParagraphListKind(resolvedKind, levelDelta: 1)
              }
              Button("수준 -1") {
                let currentKind = selectedParagraph?.listKind ?? .numbered
                viewModel.setActiveParagraphListKind(currentKind, levelDelta: -1)
              }
            }

            LavenderMenuPill(
              title: "\(colCount)단",
              symbolName: "rectangle.split.2x1",
              palette: palette,
              isSnapshotMode: viewModel.isCanvasSnapshotMode
            ) {
              ForEach([1, 2, 3], id: \.self) { cols in
                Button("\(cols)단 구성") {
                  if let tabID = viewModel.activeTabID,
                    let idx = viewModel.openTabs.firstIndex(where: { $0.id == tabID }),
                    case .document(var d) = viewModel.openTabs[idx].payload
                  {
                    if d.sections.isEmpty {
                      d.sections = [DocumentSectionLayout(paperSize: d.paperSize)]
                    }
                    d.sections[0].columnCount = cols
                    viewModel.recordUndoSnapshot(for: tabID)
                    viewModel.openTabs[idx].payload = .document(d)
                    viewModel.openTabs[idx].isModified = true
                  }
                }
              }
            }

            LavenderMenuPill(
              title: isLandscape ? "가로" : "세로",
              symbolName: isLandscape ? "rectangle.portrait.rotate" : "doc.richtext",
              palette: palette,
              isSnapshotMode: viewModel.isCanvasSnapshotMode
            ) {
              Button("세로 (Portrait)") {
                if let tabID = viewModel.activeTabID,
                  let idx = viewModel.openTabs.firstIndex(where: { $0.id == tabID }),
                  case .document(var d) = viewModel.openTabs[idx].payload
                {
                  if d.sections.isEmpty {
                    d.sections = [DocumentSectionLayout(paperSize: d.paperSize)]
                  }
                  d.sections[0].orientation = .portrait
                  viewModel.recordUndoSnapshot(for: tabID)
                  viewModel.openTabs[idx].payload = .document(d)
                  viewModel.openTabs[idx].isModified = true
                }
              }
              Button("가로 (Landscape)") {
                if let tabID = viewModel.activeTabID,
                  let idx = viewModel.openTabs.firstIndex(where: { $0.id == tabID }),
                  case .document(var d) = viewModel.openTabs[idx].payload
                {
                  if d.sections.isEmpty {
                    d.sections = [DocumentSectionLayout(paperSize: d.paperSize)]
                  }
                  d.sections[0].orientation = .landscape
                  viewModel.recordUndoSnapshot(for: tabID)
                  viewModel.openTabs[idx].payload = .document(d)
                  viewModel.openTabs[idx].isModified = true
                }
              }
            }

            LavenderMenuPill(
              title: "+ 삽입",
              symbolName: "plus.circle",
              palette: palette,
              isSnapshotMode: viewModel.isCanvasSnapshotMode
            ) {
              Button("문단 추가") {
                let trimmed = viewModel.paragraphDraftText.trimmingCharacters(
                  in: .whitespacesAndNewlines)
                let textToAppend =
                  trimmed.isEmpty
                  ? "신규 검토 문단: 표준 규격 호환성 및 레이아웃 무결성 확인 완료."
                  : trimmed
                viewModel.appendParagraphToActiveDocument(
                  text: textToAppend,
                  style: .body
                )
                viewModel.paragraphDraftText = ""
              }
              Button("표 행 추가") {
                viewModel.insertRowInDocumentTable(tableIndex: 0)
              }
              Button("각주 추가") {
                viewModel.executeRibbonCommand("doc-insert-footnote")
              }
              Button("미주 추가") {
                viewModel.insertEndnoteIntoActiveDocument()
              }
              Button("수식 삽입") {
                viewModel.insertEquationIntoActiveDocument()
              }
              Button("다이어그램 첨부") {
                viewModel.insertSampleImageAttachment()
              }
              Button("이미지 열기…") {
                WorkspaceFileDialogPresenter.presentOpenImagePanel(for: viewModel)
              }
            }

            Button(
              viewModel.isManuscriptGridModeEnabled
                ? "일반 용지"
                : "원고지"
            ) {
              viewModel.toggleManuscriptGridMode()
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)

            Button(
              viewModel.isAdvancedToolbarExpanded
                ? "고급 ▲"
                : "고급 ▼"
            ) {
              viewModel.isAdvancedToolbarExpanded.toggle()
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
          } else if document.trackChangesEnabled {
            Label("변경 추적 (\(document.revisions.count)건)", systemImage: "pencil.and.outline")
              .font(.system(size: 10, weight: .semibold))
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
              .foregroundStyle(EasyOfficeDesignTokens.statusWarning.swiftUIColor)
          }
        }
        .padding(.horizontal, 8)
        .frame(height: 34)
      }
      .background(palette.cardSurface.swiftUIColor)
      .clipShape(RoundedRectangle(cornerRadius: 6))

      // 2. Collapsible Advanced Tool Tray (Expanded Only on Demand)
      if isPrimaryPane && viewModel.isAdvancedToolbarExpanded {
        VStack(alignment: .leading, spacing: 6) {
          AdaptiveScrollContainer(
            axes: .horizontal,
            showsIndicators: false,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          ) {
            HStack(spacing: 6) {
              LavenderTextFieldPill(
                placeholder: "강조 구절 (선택)",
                text: $viewModel.inlineRunTargetPhrase,
                symbolName: "highlighter",
                width: 120,
                palette: palette,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              )

              Button("인라인 강조") {
                viewModel.applyInlineRunFormattingToActiveParagraph(
                  isBold: true,
                  isUnderlined: true,
                  foregroundHex: "#5C6BC0"
                )
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)

              Button("형광펜") {
                viewModel.toggleHighlightOnActiveParagraph()
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)

              Button("취소선") {
                viewModel.toggleStrikethroughOnActiveParagraph()
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)

              LavenderTextFieldPill(
                placeholder: "링크 URL (https://...)",
                text: $viewModel.inlineLinkURLDraft,
                symbolName: "link",
                width: 145,
                isMonospaced: true,
                palette: palette,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              )

              Button("링크 삽입") {
                viewModel.insertHyperlinkIntoActiveParagraph()
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)

              Divider().frame(height: 14)

              Button("들여쓰기 +") {
                viewModel.adjustActiveParagraphIndent(
                  firstLineDeltaPt: 6.0, leftIndentDeltaPt: 12.0)
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)

              Button("내어쓰기 -") {
                viewModel.adjustActiveParagraphIndent(
                  firstLineDeltaPt: -6.0, leftIndentDeltaPt: -12.0)
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
          }

          AdaptiveScrollContainer(
            axes: .horizontal,
            showsIndicators: false,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          ) {
            HStack(spacing: 6) {
              LavenderTextFieldPill(
                placeholder: "LaTeX 수식 입력",
                text: $viewModel.equationDraftText,
                symbolName: "function",
                width: 145,
                isMonospaced: true,
                palette: palette,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              )

              Button("수식 삽입") {
                viewModel.insertEquationIntoActiveDocument()
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)

              LavenderTextFieldPill(
                placeholder: "워터마크 문구",
                text: $viewModel.watermarkDraftText,
                symbolName: "drop.triangle",
                width: 120,
                palette: palette,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              )

              Button(watermarkText.isEmpty ? "워터마크 적용" : "워터마크 토글") {
                if !watermarkText.isEmpty
                  && viewModel.watermarkDraftText.trimmingCharacters(in: .whitespacesAndNewlines)
                    .isEmpty
                {
                  viewModel.setDocumentWatermark("")
                } else {
                  viewModel.setDocumentWatermark()
                }
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)

              Divider().frame(height: 14)

              LavenderTextFieldPill(
                placeholder: "머지 키",
                text: $viewModel.mailMergeFieldKeyDraft,
                width: 80,
                palette: palette,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              )

              LavenderTextFieldPill(
                placeholder: "머지 값",
                text: $viewModel.mailMergeFieldValueDraft,
                width: 90,
                palette: palette,
                isSnapshotMode: viewModel.isCanvasSnapshotMode
              )

              Button("메일 머지 치환") {
                viewModel.applyMailMergeFieldReplacement()
              }
              .buttonStyle(.borderedProminent)
              .tint(EasyOfficeDesignTokens.brand700.swiftUIColor)
              .controlSize(.mini)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
          }
        }
        .background(palette.cardSurface.swiftUIColor)
        .clipShape(RoundedRectangle(cornerRadius: 6))
      }

      // 3. Document Desk Area with Top Horizontal Ruler + Left Vertical Ruler + Crisp A4 Paper Sheet
      AdaptiveScrollContainer(
        axes: .vertical,
        showsIndicators: true,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      ) {
        VStack(alignment: .center, spacing: 4) {
          HStack(spacing: 8) {
            RoundedRectangle(cornerRadius: 4)
              .fill(palette.elevatedSurface.swiftUIColor)
              .frame(width: 22, height: 24)
              .overlay(
                Text("cm")
                  .font(.system(size: 7, weight: .bold, design: .monospaced))
                  .foregroundStyle(palette.secondaryText.swiftUIColor)
              )
              .overlay(
                RoundedRectangle(cornerRadius: 4)
                  .stroke(palette.hairlineBorder.swiftUIColor, lineWidth: 1)
              )

            DocumentHorizontalRulerView(
              document: document,
              selectedParagraph: selectedParagraph,
              palette: palette,
              viewModel: viewModel
            )
            .frame(maxWidth: paperMaxWidth)
          }

          HStack(alignment: .top, spacing: 8) {
            DocumentVerticalRulerView(palette: palette)

            ZStack {
              VStack(alignment: .leading, spacing: 8) {
                // Crisp Page Header
                HStack {
                  Text(resolvedHeaderText)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                  Spacer()
                  if document.trackChangesEnabled {
                    Text("변경 추적 \(document.revisions.count)건")
                      .font(.system(size: 9, weight: .semibold))
                      .foregroundStyle(EasyOfficeDesignTokens.statusWarning.swiftUIColor)
                      .padding(.horizontal, 6)
                      .padding(.vertical, 1)
                      .background(palette.subtleAccentSurface.swiftUIColor)
                      .clipShape(RoundedRectangle(cornerRadius: 4))
                  }
                  Text(
                    "단 구성: \(colCount)단 · \(currentOrientation.rawValue) · 총 \(totalPages)쪽"
                  )
                  .font(.system(size: 10, weight: .semibold))
                  .foregroundStyle(palette.accentBadgeText.swiftUIColor)
                }
                Divider()

                if isHWPModule {
                  hwpOfficialHeaderBanner(palette: palette)
                }

                if viewModel.isManuscriptGridModeEnabled {
                  HWPManuscriptGridPaperView(document: document, palette: palette)
                } else {
                  if viewModel.workspaceViewMode == .outlineView {
                    documentOutlineTreePanel(palette: palette)
                  }
                  // Compact Auto Table of Contents Card
                  if !tocEntries.isEmpty {
                    HStack(spacing: 10) {
                      Text("자동 생성 목차 (TOC)")
                        .font(.system(size: 10, weight: .bold))
                        .lineLimit(1)
                        .fixedSize(horizontal: true, vertical: false)
                        .foregroundStyle(palette.accentBadgeText.swiftUIColor)

                      Divider().frame(height: 12)

                      AdaptiveScrollContainer(
                        axes: .horizontal,
                        showsIndicators: false,
                        isSnapshotMode: viewModel.isCanvasSnapshotMode
                      ) {
                        HStack(spacing: 12) {
                          ForEach(Array(tocEntries.enumerated()), id: \.element.paragraphID) {
                            idx, entry in
                            Button {
                              viewModel.selectedParagraphID = entry.paragraphID
                              viewModel.updateSelectedParagraphSelection(
                                paragraphID: entry.paragraphID,
                                utf16Range: nil
                              )
                            } label: {
                              Text("\(idx + 1). \(entry.title)")
                                .font(.system(size: 10, weight: .medium))
                                .lineLimit(1)
                                .foregroundStyle(
                                  viewModel.selectedParagraphID == entry.paragraphID
                                    ? EasyOfficeDesignTokens.brand700.swiftUIColor
                                    : palette.secondaryText.swiftUIColor
                                )
                            }
                            .buttonStyle(.plain)
                          }
                        }
                      }
                    }
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(palette.elevatedSurface.swiftUIColor)
                    .clipShape(RoundedRectangle(cornerRadius: 5))
                  }

                  // 1-Column or Multi-Column (2..3 Columns) DTP Page Layout
                  if colCount >= 2 && document.paragraphs.count >= 2 {
                    let effectiveCols = min(max(2, colCount), max(2, document.paragraphs.count))
                    let enumeratedParagraphs = Array(document.paragraphs.enumerated())
                    let chunkSize = max(
                      1, (enumeratedParagraphs.count + effectiveCols - 1) / effectiveCols)

                    HStack(alignment: .top, spacing: 20) {
                      ForEach(0..<effectiveCols, id: \.self) { colIdx in
                        let startIdx = min(enumeratedParagraphs.count, colIdx * chunkSize)
                        let endIdx = min(enumeratedParagraphs.count, (colIdx + 1) * chunkSize)
                        let colSlice = Array(enumeratedParagraphs[startIdx..<endIdx])

                        VStack(alignment: .leading, spacing: 4) {
                          ForEach(colSlice, id: \.element.id) { idx, paragraph in
                            paragraphBlockView(
                              for: paragraph, paragraphIndex: idx, palette: palette)
                          }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)

                        if colIdx + 1 < effectiveCols {
                          Rectangle()
                            .fill(palette.border.swiftUIColor)
                            .frame(width: 1)
                        }
                      }
                    }
                  } else {
                    VStack(alignment: .leading, spacing: 4) {
                      ForEach(Array(document.paragraphs.enumerated()), id: \.element.id) {
                        idx, paragraph in
                        paragraphBlockView(for: paragraph, paragraphIndex: idx, palette: palette)
                      }
                    }
                  }

                  // Section Break Divider when document has multiple sections
                  if document.sections.count > 1 {
                    HStack(spacing: 8) {
                      Rectangle()
                        .fill(EasyOfficeDesignTokens.brand400.swiftUIColor.opacity(0.6))
                        .frame(height: 1)
                      Text("--- 구역 나눔 (Section Break · 총 \(document.sections.count)구역) ---")
                        .font(.system(size: 9, weight: .semibold, design: .monospaced))
                        .lineLimit(1)
                        .fixedSize(horizontal: true, vertical: false)
                        .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                      Rectangle()
                        .fill(EasyOfficeDesignTokens.brand400.swiftUIColor.opacity(0.6))
                        .frame(height: 1)
                    }
                    .padding(.vertical, 2)
                  }
                }

                // Side-by-Side Compact Equation & Vector Diagram Strip
                if !document.equations.isEmpty && !document.images.isEmpty {
                  HStack(alignment: .top, spacing: 10) {
                    VStack(alignment: .leading, spacing: 4) {
                      ForEach(document.equations) { eq in
                        compactEquationCardView(for: eq, palette: palette)
                      }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)

                    VStack(alignment: .leading, spacing: 4) {
                      ForEach(document.images) { img in
                        imageWrapCardView(for: img, palette: palette)
                      }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                  }
                } else if !document.equations.isEmpty {
                  VStack(alignment: .leading, spacing: 4) {
                    ForEach(document.equations) { eq in
                      compactEquationCardView(for: eq, palette: palette)
                    }
                  }
                } else if !document.images.isEmpty {
                  VStack(alignment: .leading, spacing: 4) {
                    ForEach(document.images) { img in
                      imageWrapCardView(for: img, palette: palette)
                    }
                  }
                }

                // Compact WYSIWYG Tables (표 1)
                ForEach(Array(document.tables.enumerated()), id: \.element.id) { tIdx, table in
                  documentTableView(for: table, tableIndex: tIdx, palette: palette)
                }

                // Standard Footnote Separator Rule + Compact Footnotes & Endnotes
                if !footnoteItems.isEmpty || !endnoteItems.isEmpty {
                  VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 8) {
                      Rectangle()
                        .fill(palette.secondaryText.swiftUIColor.opacity(0.65))
                        .frame(width: 120, height: 0.8)
                      Text("각주 구분선 (Footnote Separator)")
                        .font(.system(size: 9, weight: .medium))
                        .foregroundStyle(palette.secondaryText.swiftUIColor)
                    }

                    HStack(alignment: .top, spacing: 10) {
                      if !footnoteItems.isEmpty {
                        VStack(alignment: .leading, spacing: 2) {
                          ForEach(footnoteItems) { note in
                            Text("각주 \(note.number)) \(note.text)")
                              .font(.system(size: 9.5))
                              .foregroundStyle(palette.secondaryText.swiftUIColor)
                              .lineLimit(1)
                          }
                        }
                        .padding(.horizontal, 8)
                        .padding(.vertical, 4)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(palette.elevatedSurface.swiftUIColor)
                        .clipShape(RoundedRectangle(cornerRadius: 4))
                      }

                      if !endnoteItems.isEmpty {
                        VStack(alignment: .leading, spacing: 2) {
                          ForEach(endnoteItems) { note in
                            Text("미주 \(note.number)) \(note.text)")
                              .font(.system(size: 9.5))
                              .foregroundStyle(palette.secondaryText.swiftUIColor)
                              .lineLimit(1)
                          }
                        }
                        .padding(.horizontal, 8)
                        .padding(.vertical, 4)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(palette.elevatedSurface.swiftUIColor)
                        .clipShape(RoundedRectangle(cornerRadius: 4))
                      }
                    }
                  }
                }

                if let mailMergePreviewLine {
                  HStack(spacing: 6) {
                    Image(systemName: "envelope.badge.person.crop")
                      .font(.system(size: 10))
                      .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                    Text("메일 머지 미리보기: \(mailMergePreviewLine)")
                      .font(.system(size: 10))
                      .foregroundStyle(palette.secondaryText.swiftUIColor)
                      .lineLimit(1)
                  }
                  .padding(.horizontal, 8)
                  .padding(.vertical, 4)
                  .background(palette.subtleAccentSurface.swiftUIColor)
                  .clipShape(RoundedRectangle(cornerRadius: 4))
                }

                Divider()
                HStack {
                  Spacer()
                  Text(resolvedFooterText)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                  Spacer()
                }
              }
              .padding(.horizontal, CGFloat(viewModel.documentHorizontalPadding))
              .padding(.vertical, 16)
              .background(palette.cardSurface.swiftUIColor)
              .overlay(
                RoundedRectangle(cornerRadius: 8)
                  .stroke(palette.border.swiftUIColor, lineWidth: 1)
              )
              .clipShape(RoundedRectangle(cornerRadius: 8))
              .shadow(
                color: Color.black.opacity(palette.isDark ? 0.25 : 0.06),
                radius: 8,
                x: 0,
                y: 3
              )

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
        }
        .frame(maxWidth: .infinity, alignment: .center)
      }

      // Track Changes (with Accept / Reject actions) & Review Comments Strip
      if !document.revisions.isEmpty || !document.comments.isEmpty {
        HStack(alignment: .top, spacing: 10) {
          if !document.revisions.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
              Text("변경 이력 (\(document.revisions.count))")
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(palette.primaryText.swiftUIColor)
              ForEach(document.revisions) { rev in
                HStack(spacing: 6) {
                  Text("[\(rev.author)] \(rev.originalText) → \(rev.proposedText)")
                    .font(.system(size: 10))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                    .lineLimit(1)
                  Spacer()
                  if isPrimaryPane {
                    Button("승인") {
                      viewModel.acceptDocumentRevision(id: rev.id)
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(EasyOfficeDesignTokens.statusSuccess.swiftUIColor)
                    .controlSize(.mini)

                    Button("반려") {
                      viewModel.rejectDocumentRevision(id: rev.id)
                    }
                    .buttonStyle(.bordered)
                    .tint(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
                    .controlSize(.mini)
                  }
                }
              }
            }
            .padding(8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(palette.cardSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 6))
          }

          if !document.comments.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
              Text("검토 메모 (\(document.comments.count))")
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(palette.primaryText.swiftUIColor)
              ForEach(document.comments) { thread in
                Text("• \(thread.author): \(thread.message)")
                  .font(.system(size: 10))
                  .foregroundStyle(palette.secondaryText.swiftUIColor)
                  .lineLimit(1)
              }
            }
            .padding(8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(palette.cardSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 6))
          }
        }
      }
    }
  }

  private func buildRichParagraphAttributedText(
    for paragraph: ParagraphBlock,
    palette: WorkspaceThemePalette
  ) -> AttributedString {
    var combined = AttributedString()
    if paragraph.firstLineIndentPt > 0 {
      let spaceCount = max(1, min(12, Int(round(paragraph.firstLineIndentPt / 6.0))))
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
    return combined
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

    HStack(alignment: .firstTextBaseline, spacing: 6) {
      if let listPrefix {
        Text(listPrefix)
          .font(.system(size: CGFloat(paragraph.fontSizePt), weight: .bold, design: .monospaced))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
      }
      Text(buildRichParagraphAttributedText(for: paragraph, palette: palette))
        .tracking(kernPt)
        .lineSpacing(CGFloat(max(0.0, (lineSpacingMultiple - 1.0) * paragraph.fontSizePt * 0.4)))
        .frame(maxWidth: .infinity, alignment: swiftUIAlignment(for: paragraph.alignment))
    }
    .padding(.leading, CGFloat(max(0.0, paragraph.leftIndentPt)) + listIndentPt)
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

    HStack(alignment: .center, spacing: 6) {
      RoundedRectangle(cornerRadius: 1.5)
        .fill(isSelectedParagraph ? palette.primaryAccent.swiftUIColor : Color.clear)
        .frame(width: 2.5, height: rowHeight)

      if isPrimaryPane && isSelectedParagraph && !viewModel.isCanvasSnapshotMode {
        HStack(alignment: .top, spacing: 6) {
          if let listPrefix {
            Text(listPrefix)
              .font(
                .system(size: CGFloat(paragraph.fontSizePt), weight: .bold, design: .monospaced)
              )
              .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
              .padding(.leading, CGFloat(paragraph.listLevel) * 14.0)
              .padding(.top, 2)
          }
          SelectableParagraphTextView(
            viewModel: viewModel,
            paragraph: paragraph,
            palette: palette
          )
          .frame(height: rowHeight)
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
          viewModel.lastExecutedCommandSummary = "안전 하이퍼링크 이동 준비: \(safeURLString)"
        } label: {
          Label("링크", systemImage: "link")
            .font(.system(size: 9, weight: .semibold))
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
        .help(safeURLString)
      }
    }
    .padding(.vertical, 2)
    .padding(.horizontal, 6)
    .background(
      isSelectedParagraph ? palette.subtleAccentSurface.swiftUIColor : Color.clear
    )
    .clipShape(RoundedRectangle(cornerRadius: 4))
  }

  @ViewBuilder
  private func compactEquationCardView(
    for eq: DocumentEquationBlock,
    palette: WorkspaceThemePalette
  ) -> some View {
    HStack(spacing: 8) {
      Label(eq.displayTitle, systemImage: "function")
        .font(.system(size: 10, weight: .bold))
        .lineLimit(1)
        .fixedSize(horizontal: true, vertical: false)
        .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)

      Text(eq.renderedNotation)
        .font(.system(size: 13, weight: .semibold, design: .serif))
        .lineLimit(1)
        .foregroundStyle(palette.primaryText.swiftUIColor)

      Spacer(minLength: 4)

      if isPrimaryPane {
        LavenderTextFieldPill(
          placeholder: "LaTeX",
          text: Binding(
            get: { eq.rawExpression },
            set: { updated in
              viewModel.updateDocumentEquation(id: eq.id, newExpression: updated)
            }
          ),
          width: 110,
          isMonospaced: true,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
      }
    }
    .padding(.horizontal, 8)
    .padding(.vertical, 6)
    .frame(maxWidth: .infinity, minHeight: 46, maxHeight: 52, alignment: .leading)
    .background(palette.elevatedSurface.swiftUIColor)
    .overlay(
      RoundedRectangle(cornerRadius: 6)
        .stroke(palette.border.swiftUIColor, lineWidth: 0.8)
    )
    .clipShape(RoundedRectangle(cornerRadius: 6))
  }

  @ViewBuilder
  private func imageWrapCardView(for img: EmbeddedImageAttachment, palette: WorkspaceThemePalette)
    -> some View
  {
    HStack(spacing: 8) {
      if let nsImg = NSImage(data: img.imageData) {
        Image(nsImage: nsImg)
          .resizable()
          .interpolation(.high)
          .aspectRatio(contentMode: .fit)
          .frame(width: 58, height: 34)
          .clipShape(RoundedRectangle(cornerRadius: 4))
          .overlay(
            RoundedRectangle(cornerRadius: 4).stroke(palette.border.swiftUIColor, lineWidth: 0.8))
      } else {
        Image(systemName: "photo.artframe")
          .font(.system(size: 16))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
          .frame(width: 44, height: 34)
          .background(palette.selectedSurface.swiftUIColor)
          .clipShape(RoundedRectangle(cornerRadius: 4))
      }

      VStack(alignment: .leading, spacing: 2) {
        HStack(spacing: 4) {
          Text(img.fileName)
            .font(.system(size: 10, weight: .bold))
            .lineLimit(1)
            .foregroundStyle(palette.primaryText.swiftUIColor)
          Text(img.wrapMode.rawValue)
            .font(.system(size: 8.5, weight: .semibold))
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
            .padding(.horizontal, 4)
            .padding(.vertical, 1)
            .background(palette.selectedSurface.swiftUIColor)
            .foregroundStyle(palette.accentBadgeText.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 3))
        }
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
          Text("배치")
            .font(.system(size: 9.5, weight: .semibold))
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
      }
    }
    .padding(.horizontal, 8)
    .padding(.vertical, 6)
    .frame(maxWidth: .infinity, minHeight: 46, maxHeight: 52, alignment: .leading)
    .background(palette.elevatedSurface.swiftUIColor)
    .overlay(
      RoundedRectangle(cornerRadius: 6)
        .stroke(palette.border.swiftUIColor, lineWidth: 0.8)
    )
    .clipShape(RoundedRectangle(cornerRadius: 6))
  }

  @ViewBuilder
  private func documentTableView(
    for table: DocumentTableBlock,
    tableIndex: Int,
    palette: WorkspaceThemePalette
  ) -> some View {
    let hasExplicitFormulaCell = table.rows.flatMap { $0 }.contains {
      $0.trimmingCharacters(in: .whitespaces).hasPrefix("=")
    }
    let lastCol = max(0, table.columnCount - 1)
    let sumAbove = table.evaluateTableFormula("=SUM(ABOVE)", atRow: table.rowCount, column: lastCol)

    VStack(alignment: .leading, spacing: 3) {
      HStack(spacing: 6) {
        Text("\(table.caption) (\(table.borderStyle.rawValue))")
          .font(.system(size: 10, weight: .semibold))
          .lineLimit(1)
          .foregroundStyle(palette.primaryText.swiftUIColor)

        if !table.cellSpans.isEmpty {
          Text("병합 \(table.cellSpans.count)구간")
            .font(.system(size: 8.5, weight: .bold))
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(palette.selectedSurface.swiftUIColor)
            .foregroundStyle(palette.accentBadgeText.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 3))
        }

        Spacer()

        if isPrimaryPane {
          if viewModel.splitSecondaryTab != nil {
            LavenderMenuPill(
              title: "표 편집",
              symbolName: "tablecells",
              palette: palette,
              isSnapshotMode: viewModel.isCanvasSnapshotMode
            ) {
              Button("표 병합") {
                viewModel.mergeActiveDocumentTableCells(tableIndex: tableIndex)
              }
              Button("표 분할") {
                viewModel.splitActiveDocumentTableCell(tableIndex: tableIndex)
              }
              .disabled(table.cellSpans.isEmpty)
              Button("테두리 전환") {
                let allStyles = TableCellBorderStyle.allCases
                let currentIdx = allStyles.firstIndex(of: table.borderStyle) ?? 0
                let nextStyle = allStyles[(currentIdx + 1) % allStyles.count]
                viewModel.setActiveDocumentTableBorderStyle(nextStyle, tableIndex: tableIndex)
              }
              Divider()
              Button("+행 추가") {
                viewModel.insertRowInDocumentTable(tableIndex: tableIndex)
              }
              Button("-행 삭제") {
                viewModel.deleteRowInDocumentTable(tableIndex: tableIndex)
              }
              .disabled(table.rowCount <= 1)
              Button("+열 추가") {
                viewModel.insertColumnInDocumentTable(tableIndex: tableIndex)
              }
              Button("-열 삭제") {
                viewModel.deleteColumnInDocumentTable(tableIndex: tableIndex)
              }
              .disabled(table.columnCount <= 1)
            }
          } else {
            HStack(spacing: 3) {
              Button("표 병합") {
                viewModel.mergeActiveDocumentTableCells(tableIndex: tableIndex)
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)

              Button("표 분할") {
                viewModel.splitActiveDocumentTableCell(tableIndex: tableIndex)
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)
              .disabled(table.cellSpans.isEmpty)

              Button("테두리") {
                let allStyles = TableCellBorderStyle.allCases
                let currentIdx = allStyles.firstIndex(of: table.borderStyle) ?? 0
                let nextStyle = allStyles[(currentIdx + 1) % allStyles.count]
                viewModel.setActiveDocumentTableBorderStyle(nextStyle, tableIndex: tableIndex)
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)

              Button("+행") {
                viewModel.insertRowInDocumentTable(tableIndex: tableIndex)
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)

              Button("-행") {
                viewModel.deleteRowInDocumentTable(tableIndex: tableIndex)
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)
              .disabled(table.rowCount <= 1)

              Button("+열") {
                viewModel.insertColumnInDocumentTable(tableIndex: tableIndex)
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)

              Button("-열") {
                viewModel.deleteColumnInDocumentTable(tableIndex: tableIndex)
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)
              .disabled(table.columnCount <= 1)
            }
          }
        }

        if let sumAbove, hasExplicitFormulaCell || sumAbove > 0 {
          let sumFormatted = sumAbove.isFinite ? String(format: "%.0f", sumAbove) : "0"
          Text("=SUM(ABOVE): \(sumFormatted)")
            .font(.system(size: 9, weight: .bold, design: .monospaced))
            .foregroundStyle(EasyOfficeDesignTokens.statusSuccess.swiftUIColor)
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(palette.subtleAccentSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 3))
        }
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
                  ? palette.selectedSurface.swiftUIColor
                  : palette.elevatedSurface.swiftUIColor
                HStack(spacing: 4) {
                  if isPrimaryPane && !viewModel.isCanvasSnapshotMode {
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
                    .font(.system(size: 11, weight: isHeader ? .bold : .regular))
                    .foregroundStyle(
                      isHeader
                        ? palette.accentBadgeText.swiftUIColor
                        : palette.primaryText.swiftUIColor
                    )
                  } else {
                    Text(cell)
                      .font(.system(size: 11, weight: isHeader ? .bold : .regular))
                      .foregroundStyle(
                        isHeader
                          ? palette.accentBadgeText.swiftUIColor
                          : palette.primaryText.swiftUIColor
                      )
                      .lineLimit(2)
                      .fixedSize(horizontal: false, vertical: true)
                  }
                  Spacer(minLength: 0)
                  if let span, span.rowSpan > 1 || span.columnSpan > 1 {
                    Text("\(span.rowSpan)×\(span.columnSpan)")
                      .font(.system(size: 8, weight: .bold, design: .monospaced))
                      .padding(.horizontal, 3)
                      .padding(.vertical, 0.5)
                      .background(palette.subtleAccentSurface.swiftUIColor)
                      .foregroundStyle(palette.accentBadgeText.swiftUIColor)
                      .clipShape(RoundedRectangle(cornerRadius: 3))
                  }
                }
                .frame(maxWidth: .infinity, minHeight: 22, alignment: .leading)
                .padding(.vertical, 2)
                .layoutPriority(Double(span?.columnSpan ?? 1))
                .padding(.horizontal, 6)
                .background(cellBg)
                .overlay(
                  tableCellBorderOverlay(
                    style: borderStyle,
                    color: palette.border.swiftUIColor
                  )
                )
              }
            }
          }
        }
      }
    }
  }

  @ViewBuilder
  private func tableCellBorderOverlay(style: TableCellBorderStyle, color: Color) -> some View {
    switch style {
    case .solid:
      Rectangle().stroke(color, lineWidth: 0.5)
    case .doubleLine:
      ZStack {
        Rectangle().stroke(color, lineWidth: 0.6)
        Rectangle().inset(by: 1.5).stroke(color, lineWidth: 0.6)
      }
    case .dashed:
      Rectangle().stroke(color, style: StrokeStyle(lineWidth: 0.6, dash: [4, 3]))
    case .diagonalDown:
      GeometryReader { geo in
        ZStack {
          Rectangle().stroke(color, lineWidth: 0.5)
          Path { p in
            p.move(to: .zero)
            p.addLine(to: CGPoint(x: geo.size.width, y: geo.size.height))
          }
          .stroke(color.opacity(0.65), lineWidth: 0.6)
        }
      }
    case .diagonalUp:
      GeometryReader { geo in
        ZStack {
          Rectangle().stroke(color, lineWidth: 0.5)
          Path { p in
            p.move(to: CGPoint(x: 0, y: geo.size.height))
            p.addLine(to: CGPoint(x: geo.size.width, y: 0))
          }
          .stroke(color.opacity(0.65), lineWidth: 0.6)
        }
      }
    case .none:
      EmptyView()
    }
  }

  private func documentOutlineTreePanel(palette: WorkspaceThemePalette) -> some View {
    VStack(alignment: .leading, spacing: 5) {
      HStack(spacing: 8) {
        Image(systemName: "list.bullet.indent")
          .font(.system(size: 11, weight: .bold))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
        Text("문서 윤곽(Outline) 계층 구조")
          .font(.system(size: 11, weight: .bold))
          .foregroundStyle(palette.primaryText.swiftUIColor)
        Spacer()
        Button("모두 펼치기") {
          viewModel.collapsedOutlineParagraphIDs.removeAll()
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
        Button("모두 접기") {
          let headingIDs = document.paragraphs
            .filter { $0.style == .heading1 || $0.style == .heading2 }
            .map(\.id)
          viewModel.collapsedOutlineParagraphIDs = Set(headingIDs)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
      }

      ForEach(Array(document.paragraphs.enumerated()), id: \.element.id) { idx, p in
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
                .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
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
            .padding(.horizontal, 5)
            .padding(.vertical, 1.5)
            .background(
              isHeading
                ? EasyOfficeDesignTokens.brand700.swiftUIColor.opacity(0.16)
                : palette.selectedSurface.swiftUIColor
            )
            .foregroundStyle(
              isHeading
                ? EasyOfficeDesignTokens.brand700.swiftUIColor
                : palette.secondaryText.swiftUIColor
            )
            .clipShape(RoundedRectangle(cornerRadius: 4))

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

          Button("◀ 승격") {
            viewModel.promoteOrDemoteParagraphOutlineLevel(paragraphID: p.id, delta: -1)
          }
          .buttonStyle(.bordered)
          .controlSize(.mini)

          Button("강등 ▶") {
            viewModel.promoteOrDemoteParagraphOutlineLevel(paragraphID: p.id, delta: 1)
          }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        }
        .padding(.vertical, 2)
      }
    }
    .padding(10)
    .background(palette.elevatedSurface.swiftUIColor)
    .overlay(
      RoundedRectangle(cornerRadius: 6)
        .stroke(EasyOfficeDesignTokens.brand500.swiftUIColor.opacity(0.45), lineWidth: 1)
    )
    .clipShape(RoundedRectangle(cornerRadius: 6))
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
