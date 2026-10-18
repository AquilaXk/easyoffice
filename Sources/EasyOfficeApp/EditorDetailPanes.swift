import SwiftUI
import AppKit
import Observation
import EasyOfficeDesignSystem
import EasyOfficeCore

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

        public init(parent: SelectableParagraphTextView) {
            self.parent = parent
        }

        public func textDidChange(_ notification: Notification) {
            guard !isUpdatingProgrammatically,
                  let textView = notification.object as? NSTextView else { return }
            parent.viewModel.updateParagraphText(
                paragraphID: parent.paragraph.id,
                newText: textView.string
            )
        }

        public func textViewDidChangeSelection(_ notification: Notification) {
            guard !isUpdatingProgrammatically,
                  let textView = notification.object as? NSTextView else { return }
            let nsRange = textView.selectedRange()
            let utf16Range: Range<Int>? = nsRange.length > 0
                ? (nsRange.location ..< (nsRange.location + nsRange.length))
                : nil
            parent.viewModel.updateSelectedParagraphSelection(
                paragraphID: parent.paragraph.id,
                utf16Range: utf16Range
            )
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
        textView.textStorage?.setAttributedString(buildAttributedString())
        context.coordinator.lastRenderedRuns = paragraph.effectiveRuns
        context.coordinator.lastParagraphID = paragraph.id
        context.coordinator.isUpdatingProgrammatically = false

        scrollView.documentView = textView
        return scrollView
    }

    public func updateNSView(_ nsView: NSScrollView, context: Context) {
        context.coordinator.parent = self
        guard let textView = nsView.documentView as? NSTextView else { return }
        let currentRuns = paragraph.effectiveRuns
        if context.coordinator.lastParagraphID != paragraph.id ||
           context.coordinator.lastRenderedRuns != currentRuns ||
           textView.string != paragraph.text {
            let previousSelection = textView.selectedRange()
            context.coordinator.isUpdatingProgrammatically = true
            textView.textStorage?.setAttributedString(buildAttributedString())
            let utf16Len = (textView.string as NSString).length
            if previousSelection.location + previousSelection.length <= utf16Len {
                textView.setSelectedRange(previousSelection)
            }
            context.coordinator.lastRenderedRuns = currentRuns
            context.coordinator.lastParagraphID = paragraph.id
            context.coordinator.isUpdatingProgrammatically = false
        }
    }

    private func buildAttributedString() -> NSAttributedString {
        let result = NSMutableAttributedString()
        let defaultColor = NSColor(
            srgbRed: CGFloat(palette.primaryText.red) / 255.0,
            green: CGFloat(palette.primaryText.green) / 255.0,
            blue: CGFloat(palette.primaryText.blue) / 255.0,
            alpha: 1.0
        )

        for run in paragraph.effectiveRuns {
            let size = CGFloat(run.fontSizePt ?? paragraph.fontSizePt)
            var font = NSFont.systemFont(ofSize: size, weight: run.isBold ? .bold : .regular)
            if run.isItalic {
                font = NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask)
            }
            let fgColor = resolveOptionalNSColor(run.foregroundHex) ?? defaultColor
            var attrs: [NSAttributedString.Key: Any] = [
                .font: font,
                .foregroundColor: fgColor
            ]
            if run.isUnderlined {
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

    public init(viewModel: WorkspaceViewModel, document: OfficeDocumentModel, isPrimaryPane: Bool = true) {
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

    public var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
        let tocEntries = document.generateTableOfContents()
        let mailMergePreviewLine = firstMailMergePreviewLine()
        let colCount = document.sections.first?.columnCount ?? 1
        let watermarkText = (document.watermarkText ?? "").trimmingCharacters(in: .whitespacesAndNewlines)

        VStack(alignment: .leading, spacing: 14) {
            // Top Document Metadata & Quick Actions Banner
            HStack(spacing: 8) {
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
                    TextField("강조 구절 (선택)", text: $viewModel.inlineRunTargetPhrase)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11))
                        .frame(width: 115)

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
                        .frame(width: 175)

                    Button {
                        let trimmed = viewModel.paragraphDraftText.trimmingCharacters(in: .whitespacesAndNewlines)
                        let textToAppend = trimmed.isEmpty
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

            // Equation & Watermark DTP Bar
            if isPrimaryPane {
                HStack(spacing: 8) {
                    Label("수식·워터마크 조판", systemImage: "function")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)

                    TextField("LaTeX 수식 입력 (예: \\sum_{i=1}^{n} x_i)", text: $viewModel.equationDraftText)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11, design: .monospaced))
                        .frame(maxWidth: 260)

                    Button {
                        viewModel.insertEquationIntoActiveDocument()
                    } label: {
                        Label("수식 삽입", systemImage: "plus.forwardslash.minus")
                            .font(.system(size: 11, weight: .semibold))
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)

                    Divider().frame(height: 16)

                    TextField("워터마크 문구 (예: 대외비 · 무단 배포 금지)", text: $viewModel.watermarkDraftText)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11))
                        .frame(maxWidth: 220)

                    Button {
                        if !watermarkText.isEmpty && viewModel.watermarkDraftText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
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
            }

            // Page Canvas Card with Diagonal Watermark Overlay & Multi-Column DTP Layout
            ZStack {
                VStack(alignment: .leading, spacing: 14) {
                    HStack {
                        Text(document.headerText)
                            .font(.system(size: 11))
                            .foregroundStyle(palette.secondaryText.swiftUIColor)
                        Spacer()
                        Text("단 구성: \(colCount)단 · \(document.sections.first?.orientation.rawValue ?? PageOrientation.portrait.rawValue)")
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

                    // 1-Column or Genuine Side-by-Side 2-Column DTP Page Layout
                    if colCount >= 2 && document.paragraphs.count >= 2 {
                        let midIndex = (document.paragraphs.count + 1) / 2
                        let leftColumnParagraphs = Array(document.paragraphs.prefix(midIndex))
                        let rightColumnParagraphs = Array(document.paragraphs.dropFirst(midIndex))

                        HStack(alignment: .top, spacing: 24) {
                            VStack(alignment: .leading, spacing: 12) {
                                ForEach(leftColumnParagraphs) { paragraph in
                                    paragraphBlockView(for: paragraph, palette: palette)
                                }
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)

                            Divider()

                            VStack(alignment: .leading, spacing: 12) {
                                ForEach(rightColumnParagraphs) { paragraph in
                                    paragraphBlockView(for: paragraph, palette: palette)
                                }
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    } else {
                        ForEach(document.paragraphs) { paragraph in
                            paragraphBlockView(for: paragraph, palette: palette)
                        }
                    }

                    // Equation Editor Block Cards
                    if !document.equations.isEmpty {
                        Divider()
                        VStack(alignment: .leading, spacing: 8) {
                            Label("수식 블록 개체 (\(document.equations.count)건)", systemImage: "function")
                                .font(.system(size: 11, weight: .bold))
                                .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)

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
                                        .stroke(EasyOfficeDesignTokens.brand200.swiftUIColor, lineWidth: 1)
                                )
                                .clipShape(RoundedRectangle(cornerRadius: 6))
                            }
                        }
                    }

                    // Floating & Inline Image Attachments Honoring `img.wrapMode`
                    if !document.images.isEmpty {
                        Divider()
                        VStack(alignment: .leading, spacing: 8) {
                            Label("첨부 미디어 및 텍스트 어울림 개체 (\(document.images.count)건)", systemImage: "photo.on.rectangle.angled")
                                .font(.system(size: 11, weight: .bold))
                                .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)

                            ForEach(document.images) { img in
                                imageWrapCardView(for: img, palette: palette)
                            }
                        }
                    }

                    // Editable Tables + Conditional Inline Table Formula Badge
                    ForEach(document.tables) { table in
                        documentTableView(for: table, palette: palette)
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

                if !watermarkText.isEmpty {
                    Text(watermarkText)
                        .font(.system(size: 40, weight: .black, design: .rounded))
                        .foregroundStyle(EasyOfficeDesignTokens.brand400.swiftUIColor.opacity(0.18))
                        .rotationEffect(.degrees(-32))
                        .allowsHitTesting(false)
                }
            }

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

    @ViewBuilder
    private func paragraphBlockView(for paragraph: ParagraphBlock, palette: WorkspaceThemePalette) -> some View {
        let isSelectedParagraph = viewModel.selectedParagraphID == paragraph.id
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Text(paragraph.style.rawValue)
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                if paragraph.isBold { Text("B").font(.system(size: 9, weight: .bold)) }
                if paragraph.isItalic { Text("I").font(.system(size: 9, weight: .semibold)).italic() }
                if paragraph.isUnderlined { Text("U").font(.system(size: 9, weight: .semibold)).underline() }
                if paragraph.runs.count > 1 {
                    Text("인라인 런 \(paragraph.runs.count)구간")
                        .font(.system(size: 9, weight: .semibold))
                        .padding(.horizontal, 5)
                        .padding(.vertical, 1)
                        .background(EasyOfficeDesignTokens.brand100.swiftUIColor)
                        .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)
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
                    SelectableParagraphTextView(
                        viewModel: viewModel,
                        paragraph: paragraph,
                        palette: palette
                    )
                    .frame(minHeight: 34, maxHeight: 72)
                } else {
                    TextField(
                        "문단 내용",
                        text: Binding(
                            get: { paragraph.text },
                            set: { newValue in
                                viewModel.updateSelectedParagraphSelection(paragraphID: paragraph.id, utf16Range: nil)
                                viewModel.updateParagraphText(paragraphID: paragraph.id, newText: newValue)
                            }
                        ),
                        axis: .vertical
                    )
                    .textFieldStyle(.plain)
                    .font(.system(size: paragraph.fontSizePt, weight: paragraph.isBold ? .bold : .regular))
                    .italic(paragraph.isItalic)
                    .underline(paragraph.isUnderlined)
                    .foregroundStyle(palette.primaryText.swiftUIColor)
                    .onTapGesture {
                        viewModel.updateSelectedParagraphSelection(paragraphID: paragraph.id, utf16Range: nil)
                    }
                }
            } else {
                Text(paragraph.text)
                    .font(.system(size: paragraph.fontSizePt, weight: paragraph.isBold ? .bold : .regular))
                    .italic(paragraph.isItalic)
                    .underline(paragraph.isUnderlined)
                    .foregroundStyle(palette.primaryText.swiftUIColor)
            }

            if paragraph.runs.count > 1 {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 4) {
                        ForEach(paragraph.runs) { run in
                            let runColor = resolveOptionalHexColor(run.foregroundHex)
                                ?? palette.primaryText.swiftUIColor
                            Text(run.text)
                                .font(.system(size: run.fontSizePt ?? 11.0, weight: run.isBold ? .bold : .regular))
                                .italic(run.isItalic)
                                .underline(run.isUnderlined)
                                .foregroundStyle(runColor)
                                .padding(.horizontal, 6)
                                .padding(.vertical, 2)
                                .background(palette.elevatedSurface.swiftUIColor)
                                .clipShape(RoundedRectangle(cornerRadius: 4))
                        }
                    }
                }
            }
        }
        .padding(isSelectedParagraph ? 6 : 2)
        .background(isSelectedParagraph ? EasyOfficeDesignTokens.brand50.swiftUIColor.opacity(0.6) : Color.clear)
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }

    @ViewBuilder
    private func imageWrapCardView(for img: EmbeddedImageAttachment, palette: WorkspaceThemePalette) -> some View {
        let previewThumb = Group {
            if let nsImg = NSImage(data: img.imageData) {
                Image(nsImage: nsImg)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fit)
                    .frame(width: 96, height: 52)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(palette.border.swiftUIColor, lineWidth: 1))
            } else {
                Image(systemName: "photo.artframe")
                    .font(.system(size: 22))
                    .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                    .frame(width: 60, height: 48)
                    .background(EasyOfficeDesignTokens.brand100.swiftUIColor)
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
                    .background(EasyOfficeDesignTokens.brand100.swiftUIColor)
                    .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)
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
    private func documentTableView(for table: DocumentTableBlock, palette: WorkspaceThemePalette) -> some View {
        let hasExplicitFormulaCell = table.rows.flatMap { $0 }.contains {
            $0.trimmingCharacters(in: .whitespaces).hasPrefix("=")
        }
        let lastCol = max(0, table.columnCount - 1)
        let sumAbove = table.evaluateTableFormula("=SUM(ABOVE)", atRow: table.rowCount, column: lastCol)

        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("\(table.caption) (\(table.borderStyle.rawValue))")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(palette.primaryText.swiftUIColor)
                Spacer()
                if let sumAbove, (hasExplicitFormulaCell || sumAbove > 0) {
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
                    ForEach(Array(row.enumerated()), id: \.offset) { colIdx, cell in
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
                            .font(.system(size: 12, weight: rowIdx == 0 && table.hasHeaderRow ? .bold : .regular))
                            .foregroundStyle(palette.primaryText.swiftUIColor)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(6)
                            .background(
                                rowIdx == 0 && table.hasHeaderRow
                                    ? EasyOfficeDesignTokens.brand100.swiftUIColor
                                    : palette.elevatedSurface.swiftUIColor
                            )
                        } else {
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
    }
}

func resolveOptionalHexColor(_ hex: String?) -> Color? {
    guard let hex else { return nil }
    let cleaned = hex.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: "#", with: "")
    guard cleaned.count == 6, let rgb = UInt32(cleaned, radix: 16) else { return nil }
    let r = UInt8((rgb >> 16) & 0xFF)
    let g = UInt8((rgb >> 8) & 0xFF)
    let b = UInt8(rgb & 0xFF)
    return RGBColorToken(red: r, green: g, blue: b, hex: "#\(cleaned.uppercased())").swiftUIColor
}

func resolveOptionalNSColor(_ hex: String?) -> NSColor? {
    guard let hex else { return nil }
    let cleaned = hex.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: "#", with: "")
    guard cleaned.count == 6, let rgb = UInt32(cleaned, radix: 16) else { return nil }
    let r = CGFloat((rgb >> 16) & 0xFF) / 255.0
    let g = CGFloat((rgb >> 8) & 0xFF) / 255.0
    let b = CGFloat(rgb & 0xFF) / 255.0
    return NSColor(srgbRed: r, green: g, blue: b, alpha: 1.0)
}
