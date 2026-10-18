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
    textView.isEditable = true
    textView.isSelectable = true
    textView.drawsBackground = false
    textView.isVerticallyResizable = true
    textView.isHorizontallyResizable = false
    textView.textContainerInset = NSSize(width: 2, height: 4)
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
    context.coordinator.isUpdatingProgrammatically = false

    scrollView.documentView = textView
    return scrollView
  }

  public func updateNSView(_ nsView: NSScrollView, context: Context) {
    context.coordinator.parent = self
    guard let textView = nsView.documentView as? NSTextView else { return }
    let currentRuns = paragraph.effectiveRuns
    let paragraphStyleChanged =
      context.coordinator.lastAlignment != paragraph.alignment
      || context.coordinator.lastLineSpacingPercent != paragraph.lineSpacingPercent
      || context.coordinator.lastFirstLineIndentPt != paragraph.firstLineIndentPt
      || context.coordinator.lastLeftIndentPt != paragraph.leftIndentPt

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

  public var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
    let tocEntries = document.generateTableOfContents()
    let mailMergePreviewLine = firstMailMergePreviewLine()
    let colCount = document.sections.first?.columnCount ?? 1
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

    VStack(alignment: .leading, spacing: 14) {
      // Top Document Metadata & Quick Actions Banner
      HStack(spacing: 8) {
        Label(document.paperSize.rawValue, systemImage: "doc.richtext")
          .font(.system(size: 11, weight: .semibold))
          .foregroundStyle(palette.accentBadgeText.swiftUIColor)
          .padding(.horizontal, 8)
          .padding(.vertical, 4)
          .background(palette.selectedSurface.swiftUIColor)
          .clipShape(RoundedRectangle(cornerRadius: 5))

        if document.trackChangesEnabled {
          Label("변경 내용 추적 활성 (\(document.revisions.count)건)", systemImage: "pencil.and.outline")
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(EasyOfficeDesignTokens.statusWarning.swiftUIColor)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(palette.subtleAccentSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 5))
        }

        if isPrimaryPane {
          Button {
            viewModel.toggleManuscriptGridMode()
          } label: {
            Label(
              viewModel.isManuscriptGridModeEnabled ? "일반 페이지 보기" : "200자 원고지 격자 보기",
              systemImage: "squareshape.split.3x3"
            )
            .font(.system(size: 11, weight: .semibold))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)
        }

        Spacer()

        if isPrimaryPane {
          TextField("강조 구절 (선택)", text: $viewModel.inlineRunTargetPhrase)
            .textFieldStyle(.roundedBorder)
            .font(.system(size: 11))
            .frame(width: 110)

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
            viewModel.toggleHighlightOnActiveParagraph()
          } label: {
            Label("형광펜", systemImage: "highlighter")
              .font(.system(size: 11, weight: .medium))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)

          Button {
            viewModel.toggleStrikethroughOnActiveParagraph()
          } label: {
            Label("취소선", systemImage: "strikethrough")
              .font(.system(size: 11, weight: .medium))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)

          Button {
            viewModel.insertHyperlinkIntoActiveParagraph()
          } label: {
            Label("링크 삽입", systemImage: "link.badge.plus")
              .font(.system(size: 11, weight: .medium))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)

          Button {
            viewModel.insertEndnoteIntoActiveDocument()
          } label: {
            Label("미주 추가", systemImage: "text.Append")
              .font(.system(size: 11, weight: .medium))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)

          Button {
            viewModel.insertSampleImageAttachment()
          } label: {
            Label("샘플 배너 첨부", systemImage: "photo.badge.plus")
              .font(.system(size: 11, weight: .medium))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)

          Button {
            WorkspaceFileDialogPresenter.presentOpenImagePanel(for: viewModel)
          } label: {
            Label("이미지 파일 열기…", systemImage: "folder.badge.plus")
              .font(.system(size: 11, weight: .medium))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)

          TextField("추가할 문단 내용을 입력하세요...", text: $viewModel.paragraphDraftText)
            .textFieldStyle(.roundedBorder)
            .font(.system(size: 11))
            .frame(width: 155)

          Button {
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
          } label: {
            Label("문단 추가", systemImage: "plus.bubble")
              .font(.system(size: 11, weight: .semibold))
          }
          .buttonStyle(.borderedProminent)
          .tint(EasyOfficeDesignTokens.brand700.swiftUIColor)
        }
      }

      // Equation, Watermark, Link & Paragraph Indent DTP Bar
      if isPrimaryPane {
        HStack(spacing: 8) {
          Label("수식·워터마크·들여쓰기", systemImage: "function")
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)

          TextField("LaTeX 수식 입력 (예: \\sum_{i=1}^{n} x_i)", text: $viewModel.equationDraftText)
            .textFieldStyle(.roundedBorder)
            .font(.system(size: 11, design: .monospaced))
            .frame(maxWidth: 210)

          Button {
            viewModel.insertEquationIntoActiveDocument()
          } label: {
            Label("수식 삽입", systemImage: "plus.forwardslash.minus")
              .font(.system(size: 11, weight: .semibold))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)

          Divider().frame(height: 16)

          TextField("링크 URL (https://...)", text: $viewModel.inlineLinkURLDraft)
            .textFieldStyle(.roundedBorder)
            .font(.system(size: 11, design: .monospaced))
            .frame(maxWidth: 165)

          Button("들여쓰기 +") {
            viewModel.adjustActiveParagraphIndent(firstLineDeltaPt: 6.0, leftIndentDeltaPt: 12.0)
          }
          .buttonStyle(.bordered)
          .controlSize(.small)

          Button("내어쓰기 -") {
            viewModel.adjustActiveParagraphIndent(firstLineDeltaPt: -6.0, leftIndentDeltaPt: -12.0)
          }
          .buttonStyle(.bordered)
          .controlSize(.small)

          Divider().frame(height: 16)

          TextField("워터마크 문구", text: $viewModel.watermarkDraftText)
            .textFieldStyle(.roundedBorder)
            .font(.system(size: 11))
            .frame(maxWidth: 170)

          Button {
            if !watermarkText.isEmpty
              && viewModel.watermarkDraftText.trimmingCharacters(in: .whitespacesAndNewlines)
                .isEmpty
            {
              viewModel.setDocumentWatermark("")
            } else {
              viewModel.setDocumentWatermark()
            }
          } label: {
            Label(watermarkText.isEmpty ? "워터마크 적용" : "워터마크 토글", systemImage: "seal")
              .font(.system(size: 11, weight: .medium))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)

          Spacer()
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(palette.cardSurface.swiftUIColor)
        .clipShape(RoundedRectangle(cornerRadius: 6))

        // Paragraph Alignment, Line Spacing, Multi-Level List & Mail Merge Formatting Bar
        HStack(spacing: 6) {
          Label("문단·목록·머지", systemImage: "text.alignleft")
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)

          HStack(spacing: 3) {
            Button("좌") {
              viewModel.setActiveParagraphAlignment(.leading)
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)

            Button("중") {
              viewModel.setActiveParagraphAlignment(.center)
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)

            Button("우") {
              viewModel.setActiveParagraphAlignment(.trailing)
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)

            Button("양쪽") {
              viewModel.setActiveParagraphAlignment(.justified)
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)
          }

          Divider().frame(height: 14)

          HStack(spacing: 3) {
            ForEach([100.0, 115.0, 150.0, 160.0, 200.0], id: \.self) { pct in
              Button("\(Int(pct))%") {
                viewModel.setActiveParagraphLineSpacing(pct)
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)
            }
          }

          Divider().frame(height: 14)

          HStack(spacing: 3) {
            Button("• 글머리") {
              viewModel.setActiveParagraphListKind(.bullet)
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)

            Button("1. 번호") {
              viewModel.setActiveParagraphListKind(.numbered)
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)

            Button("가. 공문서") {
              viewModel.setActiveParagraphListKind(.koreanOfficial)
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)

            Button("수준 +") {
              let currentKind =
                document.paragraphs.first(where: { $0.id == viewModel.selectedParagraphID })?
                .listKind ?? .numbered
              let resolvedKind = currentKind == .none ? .numbered : currentKind
              viewModel.setActiveParagraphListKind(resolvedKind, levelDelta: 1)
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)

            Button("수준 -") {
              let currentKind =
                document.paragraphs.first(where: { $0.id == viewModel.selectedParagraphID })?
                .listKind ?? .numbered
              viewModel.setActiveParagraphListKind(currentKind, levelDelta: -1)
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)
          }

          Divider().frame(height: 14)

          TextField("머지 키 (수신처)", text: $viewModel.mailMergeFieldKeyDraft)
            .textFieldStyle(.roundedBorder)
            .font(.system(size: 11))
            .frame(width: 84)

          TextField("머지 값", text: $viewModel.mailMergeFieldValueDraft)
            .textFieldStyle(.roundedBorder)
            .font(.system(size: 11))
            .frame(width: 96)

          Button("메일 머지 치환") {
            viewModel.applyMailMergeFieldReplacement()
          }
          .buttonStyle(.borderedProminent)
          .tint(EasyOfficeDesignTokens.brand700.swiftUIColor)
          .controlSize(.small)

          Spacer()
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(palette.cardSurface.swiftUIColor)
        .clipShape(RoundedRectangle(cornerRadius: 6))
      }

      // Page Canvas Card with Diagonal Watermark Overlay & Multi-Column DTP Layout or 200-Char Manuscript Grid
      ZStack {
        VStack(alignment: .leading, spacing: 14) {
          HStack {
            Text(resolvedHeaderText)
              .font(.system(size: 11))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
            Spacer()
            Text(
              "단 구성: \(colCount)단 · \(document.sections.first?.orientation.rawValue ?? PageOrientation.portrait.rawValue) · 총 \(totalPages)쪽"
            )
            .font(.system(size: 11))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
          }
          Divider()

          if viewModel.isManuscriptGridModeEnabled {
            HWPManuscriptGridPaperView(document: document, palette: palette)
          } else {
            // Auto Table of Contents Summary Box
            if !tocEntries.isEmpty {
              VStack(alignment: .leading, spacing: 4) {
                Text("자동 생성 목차 (TOC)")
                  .font(.system(size: 11, weight: .bold))
                  .foregroundStyle(palette.accentBadgeText.swiftUIColor)
                ForEach(Array(tocEntries.enumerated()), id: \.element.paragraphID) { idx, entry in
                  Button {
                    viewModel.selectedParagraphID = entry.paragraphID
                    viewModel.updateSelectedParagraphSelection(
                      paragraphID: entry.paragraphID,
                      utf16Range: nil
                    )
                  } label: {
                    HStack(spacing: 4) {
                      Text("\(idx + 1). \(entry.title)")
                        .font(.system(size: 11, weight: .medium))
                        .padding(.leading, CGFloat((entry.level - 1) * 12))
                        .foregroundStyle(
                          viewModel.selectedParagraphID == entry.paragraphID
                            ? EasyOfficeDesignTokens.brand700.swiftUIColor
                            : palette.secondaryText.swiftUIColor
                        )
                      Spacer()
                    }
                  }
                  .buttonStyle(.plain)
                }
              }
              .padding(10)
              .frame(maxWidth: .infinity, alignment: .leading)
              .background(palette.elevatedSurface.swiftUIColor)
              .clipShape(RoundedRectangle(cornerRadius: 6))
            }

            // 1-Column or Genuine Side-by-Side 2-Column DTP Page Layout
            if colCount >= 2 && document.paragraphs.count >= 2 {
              let midIndex = (document.paragraphs.count + 1) / 2
              let leftColumnParagraphs = Array(document.paragraphs.enumerated().prefix(midIndex))
              let rightColumnParagraphs = Array(
                document.paragraphs.enumerated().dropFirst(midIndex))

              HStack(alignment: .top, spacing: 24) {
                VStack(alignment: .leading, spacing: 12) {
                  ForEach(leftColumnParagraphs, id: \.element.id) { idx, paragraph in
                    paragraphBlockView(for: paragraph, paragraphIndex: idx, palette: palette)
                  }
                }
                .frame(maxWidth: .infinity, alignment: .leading)

                Divider()

                VStack(alignment: .leading, spacing: 12) {
                  ForEach(rightColumnParagraphs, id: \.element.id) { idx, paragraph in
                    paragraphBlockView(for: paragraph, paragraphIndex: idx, palette: palette)
                  }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
              }
            } else {
              ForEach(Array(document.paragraphs.enumerated()), id: \.element.id) { idx, paragraph in
                paragraphBlockView(for: paragraph, paragraphIndex: idx, palette: palette)
              }
            }
          }

          // Equation Editor Block Cards
          if !document.equations.isEmpty {
            Divider()
            VStack(alignment: .leading, spacing: 8) {
              Label("수식 블록 개체 (\(document.equations.count)건)", systemImage: "function")
                .font(.system(size: 11, weight: .bold))
                .foregroundStyle(palette.accentBadgeText.swiftUIColor)

              ForEach(document.equations) { eq in
                VStack(alignment: .leading, spacing: 6) {
                  HStack {
                    Text(eq.displayTitle)
                      .font(.system(size: 10, weight: .bold))
                      .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                    Spacer()
                    Text(eq.renderedNotation)
                      .font(.system(size: 18, weight: .semibold, design: .serif))
                      .foregroundStyle(palette.primaryText.swiftUIColor)
                    Spacer()
                  }

                  if isPrimaryPane {
                    TextField(
                      "LaTeX 수식 표현식",
                      text: Binding(
                        get: { eq.rawExpression },
                        set: { updated in
                          viewModel.updateDocumentEquation(id: eq.id, newExpression: updated)
                        }
                      )
                    )
                    .textFieldStyle(.roundedBorder)
                    .font(.system(size: 11, design: .monospaced))
                  }
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(palette.elevatedSurface.swiftUIColor)
                .overlay(
                  RoundedRectangle(cornerRadius: 6)
                    .stroke(palette.border.swiftUIColor, lineWidth: 1)
                )
                .clipShape(RoundedRectangle(cornerRadius: 6))
              }
            }
          }

          // Floating & Inline Image Attachments Honoring `img.wrapMode`
          if !document.images.isEmpty {
            Divider()
            VStack(alignment: .leading, spacing: 8) {
              Label(
                "첨부 미디어 및 텍스트 어울림 개체 (\(document.images.count)건)",
                systemImage: "photo.on.rectangle.angled"
              )
              .font(.system(size: 11, weight: .bold))
              .foregroundStyle(palette.accentBadgeText.swiftUIColor)

              ForEach(document.images) { img in
                imageWrapCardView(for: img, palette: palette)
              }
            }
          }

          // Editable Tables + Row/Column Structure Controls + Conditional Inline Table Formula Badge
          ForEach(Array(document.tables.enumerated()), id: \.element.id) { tIdx, table in
            documentTableView(for: table, tableIndex: tIdx, palette: palette)
          }

          // Separate Footnotes & Endnotes Cards
          if !footnoteItems.isEmpty || !endnoteItems.isEmpty {
            Divider()
            HStack(alignment: .top, spacing: 12) {
              if !footnoteItems.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                  Label(
                    "각주 (Footnotes · \(footnoteItems.count)건)", systemImage: "character.textbox"
                  )
                  .font(.system(size: 10, weight: .bold))
                  .foregroundStyle(palette.accentBadgeText.swiftUIColor)
                  ForEach(footnoteItems) { note in
                    Text("각주 \(note.number)) \(note.text)")
                      .font(.system(size: 10))
                      .foregroundStyle(palette.secondaryText.swiftUIColor)
                  }
                }
                .padding(8)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(palette.elevatedSurface.swiftUIColor)
                .clipShape(RoundedRectangle(cornerRadius: 6))
              }

              if !endnoteItems.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                  Label("미주 (Endnotes · \(endnoteItems.count)건)", systemImage: "text.quote")
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                  ForEach(endnoteItems) { note in
                    Text("미주 \(note.number)) \(note.text)")
                      .font(.system(size: 10))
                      .foregroundStyle(palette.secondaryText.swiftUIColor)
                  }
                }
                .padding(8)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(palette.elevatedSurface.swiftUIColor)
                .clipShape(RoundedRectangle(cornerRadius: 6))
              }
            }
          }

          if let mailMergePreviewLine {
            HStack(spacing: 6) {
              Image(systemName: "envelope.badge.person.crop")
                .font(.system(size: 11))
                .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
              Text("메일 머지 미리보기: \(mailMergePreviewLine)")
                .font(.system(size: 11))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
                .lineLimit(1)
            }
            .padding(8)
            .background(palette.subtleAccentSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 6))
          }

          Divider()
          HStack {
            Spacer()
            Text(resolvedFooterText)
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

        if !watermarkText.isEmpty {
          Text(watermarkText)
            .font(.system(size: 40, weight: .black, design: .rounded))
            .foregroundStyle(EasyOfficeDesignTokens.brand400.swiftUIColor.opacity(0.18))
            .rotationEffect(.degrees(-32))
            .allowsHitTesting(false)
        }
      }

      // Track Changes (with Accept / Reject actions) & Review Comments Strip
      if !document.revisions.isEmpty || !document.comments.isEmpty {
        HStack(alignment: .top, spacing: 12) {
          if !document.revisions.isEmpty {
            VStack(alignment: .leading, spacing: 6) {
              Text("변경 이력 (\(document.revisions.count))")
                .font(.system(size: 11, weight: .bold))
                .foregroundStyle(palette.primaryText.swiftUIColor)
              ForEach(document.revisions) { rev in
                HStack(spacing: 8) {
                  Text("[\(rev.author)] \(rev.originalText) → \(rev.proposedText)")
                    .font(.system(size: 11))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                    .lineLimit(2)
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
    let listIndentPt = paragraph.listKind == .none ? 0.0 : CGFloat(paragraph.listLevel) * 18.0

    HStack(alignment: .firstTextBaseline, spacing: 6) {
      if let listPrefix {
        Text(listPrefix)
          .font(.system(size: CGFloat(paragraph.fontSizePt), weight: .bold, design: .monospaced))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
      }
      Text(buildRichParagraphAttributedText(for: paragraph, palette: palette))
        .lineSpacing(CGFloat(max(0.0, (lineSpacingMultiple - 1.0) * paragraph.fontSizePt)))
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
    let hasRichRuns =
      paragraph.runs.count > 1
      || paragraph.runs.contains {
        $0.isStrikethrough || $0.backgroundHex != nil || !($0.linkURL ?? "").isEmpty
      }

    VStack(alignment: .leading, spacing: 6) {
      HStack(spacing: 6) {
        Text(paragraph.style.rawValue)
          .font(.system(size: 9, weight: .semibold))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
        if let listPrefix {
          Text("목록 \(listPrefix)")
            .font(.system(size: 9, weight: .bold, design: .monospaced))
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(palette.subtleAccentSurface.swiftUIColor)
            .foregroundStyle(palette.accentBadgeText.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 4))
        }
        if paragraph.isBold {
          Text("B").font(.system(size: 9, weight: .bold))
        }
        if paragraph.isItalic {
          Text("I").font(.system(size: 9, weight: .semibold)).italic()
        }
        if paragraph.isUnderlined {
          Text("U").font(.system(size: 9, weight: .semibold)).underline()
        }
        if paragraph.leftIndentPt > 0 || paragraph.firstLineIndentPt > 0 {
          Text("들여쓰기 \(Int(paragraph.leftIndentPt))pt")
            .font(.system(size: 9, design: .monospaced))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
        }
        if hasRichRuns {
          Text("인라인 런 \(paragraph.effectiveRuns.count)구간")
            .font(.system(size: 9, weight: .semibold))
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(palette.selectedSurface.swiftUIColor)
            .foregroundStyle(palette.accentBadgeText.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 4))
        }
        if isSelectedParagraph, let r = viewModel.selectedParagraphUTF16Range, !r.isEmpty {
          Text("선택 구간: \(r.lowerBound)..<\(r.upperBound)")
            .font(.system(size: 9, weight: .bold, design: .monospaced))
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(EasyOfficeDesignTokens.brand700.swiftUIColor)
            .foregroundStyle(Color.white)
            .clipShape(RoundedRectangle(cornerRadius: 4))
        }
        Spacer()
      }

      if isPrimaryPane {
        if isSelectedParagraph {
          HStack(alignment: .top, spacing: 6) {
            if let listPrefix {
              Text(listPrefix)
                .font(
                  .system(size: CGFloat(paragraph.fontSizePt), weight: .bold, design: .monospaced)
                )
                .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                .padding(.leading, CGFloat(paragraph.listLevel) * 18.0)
                .padding(.top, 4)
            }
            SelectableParagraphTextView(
              viewModel: viewModel,
              paragraph: paragraph,
              palette: palette
            )
            .frame(minHeight: 36, maxHeight: 76)
          }
        } else {
          paragraphRow(for: paragraph, paragraphIndex: paragraphIndex, palette: palette)
            .contentShape(Rectangle())
            .onTapGesture {
              viewModel.updateSelectedParagraphSelection(paragraphID: paragraph.id, utf16Range: nil)
            }
        }
      } else {
        paragraphRow(for: paragraph, paragraphIndex: paragraphIndex, palette: palette)
      }

      if hasRichRuns {
        ScrollView(.horizontal, showsIndicators: false) {
          HStack(spacing: 4) {
            ForEach(paragraph.effectiveRuns) { run in
              let runColor =
                run.foregroundHex.map {
                  EasyOfficeDesignTokens.resolveHexColor(
                    $0,
                    fallback: palette.primaryText.swiftUIColor
                  )
                } ?? palette.primaryText.swiftUIColor
              let bgColor =
                run.backgroundHex.map {
                  EasyOfficeDesignTokens.resolveHexColor(
                    $0,
                    fallback: palette.elevatedSurface.swiftUIColor
                  )
                } ?? palette.elevatedSurface.swiftUIColor
              let hasLink = !(run.linkURL ?? "").isEmpty
              Text(hasLink ? "\(run.text) [🔗]" : run.text)
                .font(.system(size: run.fontSizePt ?? 11.0, weight: run.isBold ? .bold : .regular))
                .italic(run.isItalic)
                .underline(run.isUnderlined || hasLink)
                .strikethrough(run.isStrikethrough)
                .foregroundStyle(runColor)
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(bgColor)
                .clipShape(RoundedRectangle(cornerRadius: 4))
            }
          }
        }
      }
    }
    .padding(isSelectedParagraph ? 6 : 2)
    .background(
      isSelectedParagraph ? palette.subtleAccentSurface.swiftUIColor : Color.clear
    )
    .clipShape(RoundedRectangle(cornerRadius: 6))
  }

  @ViewBuilder
  private func imageWrapCardView(for img: EmbeddedImageAttachment, palette: WorkspaceThemePalette)
    -> some View
  {
    let previewThumb = Group {
      if let nsImg = NSImage(data: img.imageData) {
        Image(nsImage: nsImg)
          .resizable()
          .interpolation(.high)
          .aspectRatio(contentMode: .fit)
          .frame(width: 96, height: 52)
          .clipShape(RoundedRectangle(cornerRadius: 6))
          .overlay(
            RoundedRectangle(cornerRadius: 6).stroke(palette.border.swiftUIColor, lineWidth: 1))
      } else {
        Image(systemName: "photo.artframe")
          .font(.system(size: 22))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
          .frame(width: 60, height: 48)
          .background(palette.selectedSurface.swiftUIColor)
          .clipShape(RoundedRectangle(cornerRadius: 6))
      }
    }

    let metadataBlock = VStack(alignment: .leading, spacing: 3) {
      HStack(spacing: 6) {
        Text(img.fileName)
          .font(.system(size: 11, weight: .bold))
          .foregroundStyle(palette.primaryText.swiftUIColor)
        Text(img.wrapMode.rawValue)
          .font(.system(size: 9, weight: .semibold))
          .padding(.horizontal, 6)
          .padding(.vertical, 2)
          .background(palette.selectedSurface.swiftUIColor)
          .foregroundStyle(palette.accentBadgeText.swiftUIColor)
          .clipShape(RoundedRectangle(cornerRadius: 4))
      }
      if !img.caption.isEmpty {
        Text(img.caption)
          .font(.system(size: 10))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
      }
      Text("\(Int(img.widthPt))×\(Int(img.heightPt)) pt · \(img.imageData.count) bytes")
        .font(.system(size: 9, design: .monospaced))
        .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
    }

    let wrapButton = Group {
      if isPrimaryPane {
        Button {
          viewModel.cycleImageWrapMode(imageID: img.id)
        } label: {
          Label("배치 전환", systemImage: "text.below.photo")
            .font(.system(size: 10, weight: .semibold))
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
      }
    }

    Group {
      switch img.wrapMode {
      case .squareLeft:
        HStack(alignment: .top, spacing: 12) {
          previewThumb
          metadataBlock
          Spacer()
          wrapButton
        }
      case .squareRight:
        HStack(alignment: .top, spacing: 12) {
          metadataBlock
          Spacer()
          wrapButton
          previewThumb
        }
      case .topAndBottom:
        VStack(alignment: .center, spacing: 6) {
          previewThumb
          HStack {
            metadataBlock
            Spacer()
            wrapButton
          }
        }
      case .inline:
        HStack(spacing: 10) {
          previewThumb
          metadataBlock
          Spacer()
          wrapButton
        }
      }
    }
    .padding(10)
    .background(palette.elevatedSurface.swiftUIColor)
    .overlay(RoundedRectangle(cornerRadius: 6).stroke(palette.border.swiftUIColor, lineWidth: 1))
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

    VStack(alignment: .leading, spacing: 6) {
      HStack(spacing: 8) {
        Text("\(table.caption) (\(table.borderStyle.rawValue))")
          .font(.system(size: 11, weight: .semibold))
          .foregroundStyle(palette.primaryText.swiftUIColor)

        if !table.cellSpans.isEmpty {
          Text("병합 셀 \(table.cellSpans.count)구간")
            .font(.system(size: 9, weight: .bold))
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(palette.selectedSurface.swiftUIColor)
            .foregroundStyle(palette.accentBadgeText.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 4))
        }

        Spacer()

        if isPrimaryPane {
          HStack(spacing: 4) {
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

        if let sumAbove, hasExplicitFormulaCell || sumAbove > 0 {
          let sumFormatted = sumAbove.isFinite ? String(format: "%.0f", sumAbove) : "0"
          Text("표 수식 =SUM(ABOVE): \(sumFormatted)")
            .font(.system(size: 10, weight: .bold, design: .monospaced))
            .foregroundStyle(EasyOfficeDesignTokens.statusSuccess.swiftUIColor)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(palette.subtleAccentSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 4))
        }
      }

      ForEach(Array(table.rows.enumerated()), id: \.offset) { rowIdx, row in
        HStack(spacing: 2) {
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
                  .font(.system(size: 12, weight: isHeader ? .bold : .regular))
                  .foregroundStyle(
                    isHeader
                      ? palette.accentBadgeText.swiftUIColor
                      : palette.primaryText.swiftUIColor
                  )
                } else {
                  Text(cell)
                    .font(.system(size: 12, weight: isHeader ? .bold : .regular))
                    .foregroundStyle(
                      isHeader
                        ? palette.accentBadgeText.swiftUIColor
                        : palette.primaryText.swiftUIColor
                    )
                }
                Spacer(minLength: 0)
                if let span, span.rowSpan > 1 || span.columnSpan > 1 {
                  Text("\(span.rowSpan)×\(span.columnSpan)")
                    .font(.system(size: 8, weight: .bold, design: .monospaced))
                    .padding(.horizontal, 4)
                    .padding(.vertical, 1)
                    .background(palette.subtleAccentSurface.swiftUIColor)
                    .foregroundStyle(palette.accentBadgeText.swiftUIColor)
                    .clipShape(RoundedRectangle(cornerRadius: 3))
                }
              }
              .frame(maxWidth: .infinity, alignment: .leading)
              .layoutPriority(Double(span?.columnSpan ?? 1))
              .padding(6)
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

  @ViewBuilder
  private func tableCellBorderOverlay(style: TableCellBorderStyle, color: Color) -> some View {
    switch style {
    case .solid:
      Rectangle().stroke(color, lineWidth: 1)
    case .doubleLine:
      ZStack {
        Rectangle().stroke(color, lineWidth: 1)
        Rectangle().inset(by: 2).stroke(color, lineWidth: 1)
      }
    case .dashed:
      Rectangle().stroke(color, style: StrokeStyle(lineWidth: 1, dash: [4, 3]))
    case .diagonalDown:
      GeometryReader { geo in
        ZStack {
          Rectangle().stroke(color, lineWidth: 1)
          Path { p in
            p.move(to: .zero)
            p.addLine(to: CGPoint(x: geo.size.width, y: geo.size.height))
          }
          .stroke(color.opacity(0.65), lineWidth: 1)
        }
      }
    case .diagonalUp:
      GeometryReader { geo in
        ZStack {
          Rectangle().stroke(color, lineWidth: 1)
          Path { p in
            p.move(to: CGPoint(x: 0, y: geo.size.height))
            p.addLine(to: CGPoint(x: geo.size.width, y: 0))
          }
          .stroke(color.opacity(0.65), lineWidth: 1)
        }
      }
    case .none:
      EmptyView()
    }
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
