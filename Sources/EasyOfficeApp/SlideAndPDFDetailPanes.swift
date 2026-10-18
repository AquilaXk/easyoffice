import SwiftUI
import AppKit
import PDFKit
import Observation
import EasyOfficeDesignSystem
import EasyOfficeCore

/// Rich interactive editor pane for Slide (.pptx/.odp) with Filmstrip, proportional 16:9 Stage Canvas, 8-direction resize handles, rotation handle (↻), element entrance animations, and full-screen slideshow launcher.
public struct SlideEditorDetailPaneView: View {
    @Environment(\.colorScheme) private var colorScheme
    @Bindable var viewModel: WorkspaceViewModel
    let deck: PresentationDeckModel
    let isPrimaryPane: Bool
    @GestureState private var activeDragOffset: (elementID: UUID, translation: CGSize)? = nil

    public init(viewModel: WorkspaceViewModel, deck: PresentationDeckModel, isPrimaryPane: Bool = true) {
        self.viewModel = viewModel
        self.deck = deck
        self.isPrimaryPane = isPrimaryPane
    }

    private func handleOffset(for handle: SlideResizeHandle, width: CGFloat, height: CGFloat) -> CGSize {
        let halfW = width / 2.0
        let halfH = height / 2.0
        switch handle {
        case .topLeft: return CGSize(width: -halfW, height: -halfH)
        case .top: return CGSize(width: 0, height: -halfH)
        case .topRight: return CGSize(width: halfW, height: -halfH)
        case .right: return CGSize(width: halfW, height: 0)
        case .bottomRight: return CGSize(width: halfW, height: halfH)
        case .bottom: return CGSize(width: 0, height: halfH)
        case .bottomLeft: return CGSize(width: -halfW, height: halfH)
        case .left: return CGSize(width: -halfW, height: 0)
        }
    }

    public var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
        let activeIdx = deck.slides.indices.contains(viewModel.selectedSlideIndex) ? viewModel.selectedSlideIndex : 0
        let currentSlide = deck.slides.indices.contains(activeIdx) ? deck.slides[activeIdx] : nil
        let selectedElement: SlideCanvasElement? = {
            guard let slide = currentSlide else { return nil }
            if let selID = viewModel.selectedSlideElementID,
               let found = slide.elements.first(where: { $0.id == selID }) {
                return found
            }
            return slide.elements.first
        }()
        let presenterSession = PresenterSession(
            deck: deck,
            currentSlideIndex: activeIdx,
            elapsedSeconds: viewModel.presenterElapsedSeconds,
            isTimerRunning: viewModel.isPresenterTimerRunning
        )

        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 8) {
                Label("\(deck.aspectRatio.rawValue) · 슬라이드 \(deck.slides.count)장", systemImage: "rectangle.on.rectangle.angled")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
                Spacer()
                if isPrimaryPane {
                    if let activeElem = selectedElement {
                        Button {
                            viewModel.cycleSlideElementAnimation(elementID: activeElem.id)
                        } label: {
                            Label("애니메이션: \(activeElem.animation.rawValue)", systemImage: "sparkles")
                                .font(.system(size: 11, weight: .medium))
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                    }

                    Button {
                        viewModel.isPresenterModeActive.toggle()
                        viewModel.isPresenterTimerRunning = viewModel.isPresenterModeActive
                    } label: {
                        Label(
                            viewModel.isPresenterModeActive ? "발표자 도구 닫기" : "발표자 도구 미리보기",
                            systemImage: "play.display"
                        )
                        .font(.system(size: 11, weight: .semibold))
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)

                    Button {
                        viewModel.toggleFullScreenSlideshow()
                    } label: {
                        Label("슬라이드쇼 시작 (전체화면)", systemImage: "play.tv.fill")
                            .font(.system(size: 11, weight: .semibold))
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
                }
            }

            HStack(alignment: .top, spacing: 14) {
                // Slide Filmstrip
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(Array(deck.slides.enumerated()), id: \.element.id) { idx, slide in
                        let isSelected = idx == activeIdx
                        Button {
                            guard isPrimaryPane else { return }
                            viewModel.selectedSlideIndex = idx
                            viewModel.selectSlideElement(id: slide.elements.first?.id)
                        } label: {
                            VStack(alignment: .leading, spacing: 4) {
                                Text("슬라이드 \(idx + 1) · \(slide.masterLayout.rawValue)")
                                    .font(.system(size: 10, weight: .bold))
                                    .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
                                Text(slide.title)
                                    .font(.system(size: 12, weight: .semibold))
                                    .foregroundStyle(palette.primaryText.swiftUIColor)
                                    .lineLimit(1)
                            }
                            .padding(10)
                            .frame(width: EasyOfficeDesignTokens.LayoutMetrics.slideFilmstripWidth, alignment: .leading)
                            .background(
                                isSelected
                                    ? EasyOfficeDesignTokens.brand100.swiftUIColor
                                    : palette.cardSurface.swiftUIColor
                            )
                            .overlay(
                                RoundedRectangle(cornerRadius: 6)
                                    .stroke(
                                        isSelected
                                            ? EasyOfficeDesignTokens.brand700.swiftUIColor
                                            : palette.border.swiftUIColor,
                                        lineWidth: isSelected ? 1.5 : 1.0
                                    )
                            )
                            .clipShape(RoundedRectangle(cornerRadius: 6))
                        }
                        .buttonStyle(.plain)
                    }
                }

                // Active Slide Stage Canvas & Inline Editors
                if let slide = currentSlide {
                    let staticGuides: [AlignmentSnapGuide] = {
                        guard viewModel.isSnapGuidesEnabled, let activeElem = selectedElement else { return [] }
                        let siblings = slide.elements.filter { $0.id != activeElem.id }
                        return SnapGuideCalculator.computeGuides(
                            for: activeElem,
                            siblings: siblings,
                            aspectRatio: deck.aspectRatio
                        )
                    }()

                    VStack(alignment: .leading, spacing: 12) {
                        HStack {
                            Text(slide.masterLayout.rawValue)
                                .font(.system(size: 10, weight: .bold))
                                .padding(.horizontal, 6)
                                .padding(.vertical, 2)
                                .background(EasyOfficeDesignTokens.softBadgeToken(for: .slide).swiftUIColor)
                                .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
                                .clipShape(RoundedRectangle(cornerRadius: 4))

                            Text("전환: \(slide.transition.rawValue)")
                                .font(.system(size: 10))
                                .foregroundStyle(palette.secondaryText.swiftUIColor)

                            Spacer()

                            if isPrimaryPane, let activeElem = selectedElement {
                                HStack(spacing: 4) {
                                    Button("◀") { viewModel.moveSlideElement(slideIndex: activeIdx, elementID: activeElem.id, deltaX: -20, deltaY: 0) }
                                        .buttonStyle(.bordered).controlSize(.mini)
                                    Button("▶") { viewModel.moveSlideElement(slideIndex: activeIdx, elementID: activeElem.id, deltaX: 20, deltaY: 0) }
                                        .buttonStyle(.bordered).controlSize(.mini)
                                    Button("▲") { viewModel.moveSlideElement(slideIndex: activeIdx, elementID: activeElem.id, deltaX: 0, deltaY: -20) }
                                        .buttonStyle(.bordered).controlSize(.mini)
                                    Button("▼") { viewModel.moveSlideElement(slideIndex: activeIdx, elementID: activeElem.id, deltaX: 0, deltaY: 20) }
                                        .buttonStyle(.bordered).controlSize(.mini)
                                    Button("↻ +15°") { viewModel.rotateSlideElement(elementID: activeElem.id, toDegrees: activeElem.rotationDegrees + 15.0) }
                                        .buttonStyle(.bordered).controlSize(.mini)
                                    Button("정중앙 스냅") { viewModel.executeRibbonCommand("slide-mod-snap") }
                                        .buttonStyle(.bordered).controlSize(.mini)
                                }
                            }

                            if !staticGuides.isEmpty {
                                Label("스마트 정렬 가이드 스냅 활성 (\(staticGuides.count)축)", systemImage: "ruler")
                                    .font(.system(size: 10, weight: .semibold))
                                    .foregroundStyle(EasyOfficeDesignTokens.statusSuccess.swiftUIColor)
                            }
                        }

                        // Proportional 16:9 Stage Canvas with Visual Snap Guides, 8-Direction Resize Handles & Rotation Handle (↻)
                        GeometryReader { geo in
                            let stageW = max(1.0, deck.aspectRatio.widthPoints)
                            let stageH = max(1.0, deck.aspectRatio.heightPoints)
                            let scaleX = max(0.01, geo.size.width / stageW)
                            let scaleY = max(0.01, geo.size.height / stageH)
                            let activeGuides: [AlignmentSnapGuide] = {
                                guard viewModel.isSnapGuidesEnabled else { return [] }
                                let targetID = activeDragOffset?.elementID ?? selectedElement?.id
                                guard let targetID, var activeElem = slide.elements.first(where: { $0.id == targetID }) else { return [] }
                                if let drag = activeDragOffset, drag.elementID == activeElem.id {
                                    activeElem.x += Double(drag.translation.width / scaleX)
                                    activeElem.y += Double(drag.translation.height / scaleY)
                                }
                                let siblings = slide.elements.filter { $0.id != activeElem.id }
                                return SnapGuideCalculator.computeGuides(
                                    for: activeElem,
                                    siblings: siblings,
                                    aspectRatio: deck.aspectRatio
                                )
                            }()

                            ZStack(alignment: .topLeading) {
                                RoundedRectangle(cornerRadius: 8)
                                    .fill(palette.elevatedSurface.swiftUIColor)

                                // Slide Header & Bullet Content Overlay
                                VStack(alignment: .leading, spacing: 8) {
                                    Text(slide.title)
                                        .font(.system(size: 20, weight: .bold))
                                        .foregroundStyle(palette.primaryText.swiftUIColor)

                                    if !slide.subtitle.isEmpty {
                                        Text(slide.subtitle)
                                            .font(.system(size: 13, weight: .medium))
                                            .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                                    }

                                    ForEach(slide.bulletPoints, id: \.self) { bullet in
                                        Text("• \(bullet)")
                                            .font(.system(size: 12))
                                            .foregroundStyle(palette.primaryText.swiftUIColor)
                                    }
                                }
                                .padding(20)

                                // Visual Alignment Snap Guide Dashed Lines
                                ForEach(Array(activeGuides.enumerated()), id: \.offset) { _, guide in
                                    Path { path in
                                        switch guide.axis {
                                        case .vertical:
                                            let xPos = CGFloat(guide.positionPoints) * scaleX
                                            path.move(to: CGPoint(x: xPos, y: 0))
                                            path.addLine(to: CGPoint(x: xPos, y: geo.size.height))
                                        case .horizontal:
                                            let yPos = CGFloat(guide.positionPoints) * scaleY
                                            path.move(to: CGPoint(x: 0, y: yPos))
                                            path.addLine(to: CGPoint(x: geo.size.width, y: yPos))
                                        }
                                    }
                                    .stroke(
                                        EasyOfficeDesignTokens.statusSuccess.swiftUIColor,
                                        style: StrokeStyle(lineWidth: 1.5, dash: [5, 4])
                                    )
                                }

                                // Positioned & Selectable Slide Canvas Elements with 8 Resize Handles & Top Rotation Handle
                                ForEach(slide.elements) { element in
                                    let isSelectedElem = selectedElement?.id == element.id
                                    let liveOffset: CGSize = (activeDragOffset?.elementID == element.id)
                                        ? (activeDragOffset?.translation ?? .zero)
                                        : .zero
                                    let elemW = max(90.0, CGFloat(element.width) * scaleX)
                                    let elemH = max(50.0, CGFloat(element.height) * scaleY)
                                    let centerX = CGFloat(element.centerX) * scaleX + liveOffset.width
                                    let centerY = CGFloat(element.centerY) * scaleY + liveOffset.height
                                    let fillColor = resolveHexColor(element.fillHex, fallback: EasyOfficeDesignTokens.brand50)
                                    let strokeColor = resolveHexColor(element.strokeHex, fallback: EasyOfficeDesignTokens.brand600)

                                    ZStack {
                                        VStack(alignment: .leading, spacing: 2) {
                                            HStack(spacing: 4) {
                                                Text(element.title)
                                                    .font(.system(size: 11, weight: element.isBold ? .heavy : .bold))
                                                    .italic(element.isItalic)
                                                    .underline(element.isUnderlined)
                                                    .foregroundStyle(EasyOfficeDesignTokens.brand900.swiftUIColor)
                                                Spacer(minLength: 2)
                                                if element.animation != .none {
                                                    Text("✦ \(element.animation.rawValue)")
                                                        .font(.system(size: 8, weight: .bold))
                                                        .padding(.horizontal, 4)
                                                        .padding(.vertical, 1)
                                                        .background(EasyOfficeDesignTokens.brand100.swiftUIColor)
                                                        .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)
                                                        .clipShape(RoundedRectangle(cornerRadius: 3))
                                                }
                                            }
                                            Text(element.bodyText)
                                                .font(.system(size: 10, weight: element.isBold ? .semibold : .regular))
                                                .italic(element.isItalic)
                                                .underline(element.isUnderlined)
                                                .foregroundStyle(palette.secondaryText.swiftUIColor)
                                                .lineLimit(2)
                                        }
                                        .padding(8)
                                        .frame(width: elemW, height: elemH, alignment: .leading)
                                        .background(fillColor.opacity(0.95))
                                        .overlay(
                                            RoundedRectangle(cornerRadius: 6)
                                                .stroke(
                                                    isSelectedElem
                                                        ? EasyOfficeDesignTokens.brand700.swiftUIColor
                                                        : strokeColor,
                                                    lineWidth: isSelectedElem ? 2.5 : 1.5
                                                )
                                        )
                                        .clipShape(RoundedRectangle(cornerRadius: 6))

                                        if isPrimaryPane && isSelectedElem {
                                            // 8 Directional Resize Handles (SlideResizeHandle.allCases)
                                            ForEach(SlideResizeHandle.allCases, id: \.rawValue) { handle in
                                                let offset = handleOffset(for: handle, width: elemW, height: elemH)
                                                Circle()
                                                    .fill(EasyOfficeDesignTokens.brand700.swiftUIColor)
                                                    .frame(width: 8, height: 8)
                                                    .overlay(Circle().stroke(Color.white, lineWidth: 1))
                                                    .offset(offset)
                                                    .help("리사이즈 핸들 (\(handle.rawValue))")
                                                    .gesture(
                                                        DragGesture(minimumDistance: 1)
                                                            .onEnded { value in
                                                                let dX = Double(value.translation.width / scaleX)
                                                                let dY = Double(value.translation.height / scaleY)
                                                                viewModel.resizeSlideElementByHandle(
                                                                    elementID: element.id,
                                                                    handle: handle,
                                                                    deltaX: dX,
                                                                    deltaY: dY
                                                                )
                                                            }
                                                    )
                                            }

                                            // Top Rotation Handle (↻)
                                            Text("↻")
                                                .font(.system(size: 10, weight: .bold))
                                                .foregroundStyle(Color.white)
                                                .frame(width: 16, height: 16)
                                                .background(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
                                                .clipShape(Circle())
                                                .offset(x: 0, y: -(elemH / 2.0) - 14.0)
                                                .help("회전 핸들 (↻): 드래그하거나 클릭하여 개체 회전")
                                                .onTapGesture {
                                                    viewModel.rotateSlideElement(
                                                        elementID: element.id,
                                                        toDegrees: element.rotationDegrees + 15.0
                                                    )
                                                }
                                                .gesture(
                                                    DragGesture(minimumDistance: 1)
                                                        .onEnded { value in
                                                            let angleDelta = Double(value.translation.width) * 0.5
                                                            viewModel.rotateSlideElement(
                                                                elementID: element.id,
                                                                toDegrees: element.rotationDegrees + angleDelta
                                                            )
                                                        }
                                                )
                                        }
                                    }
                                    .rotationEffect(.degrees(element.rotationDegrees))
                                    .position(x: centerX, y: centerY)
                                    .onTapGesture {
                                        guard isPrimaryPane else { return }
                                        viewModel.selectSlideElement(id: element.id)
                                    }
                                    .gesture(
                                        DragGesture(minimumDistance: 2)
                                            .updating($activeDragOffset) { value, state, _ in
                                                guard isPrimaryPane else { return }
                                                state = (elementID: element.id, translation: value.translation)
                                            }
                                            .onEnded { value in
                                                guard isPrimaryPane else { return }
                                                viewModel.selectSlideElement(id: element.id)
                                                let dX = Double(value.translation.width / scaleX)
                                                let dY = Double(value.translation.height / scaleY)
                                                viewModel.moveSlideElement(
                                                    slideIndex: activeIdx,
                                                    elementID: element.id,
                                                    deltaX: dX,
                                                    deltaY: dY
                                                )
                                            }
                                    )
                                }
                            }
                        }
                        .frame(height: 290)

                        // Inline Bullet Point & Selected Element Editors
                        if isPrimaryPane {
                            Divider()
                            HStack(alignment: .top, spacing: 16) {
                                VStack(alignment: .leading, spacing: 6) {
                                    HStack {
                                        Text("슬라이드 불릿 편집 (\(slide.bulletPoints.count)개)")
                                            .font(.system(size: 11, weight: .bold))
                                            .foregroundStyle(palette.primaryText.swiftUIColor)
                                        Spacer()
                                        Button("불릿 추가") {
                                            viewModel.addSlideBulletPoint(
                                                slideIndex: activeIdx,
                                                text: "새 핵심 발표 항목"
                                            )
                                        }
                                        .buttonStyle(.bordered)
                                        .controlSize(.mini)
                                    }

                                    ForEach(Array(slide.bulletPoints.enumerated()), id: \.offset) { bIdx, bulletText in
                                        HStack(spacing: 4) {
                                            Text("•")
                                                .font(.system(size: 11, weight: .bold))
                                            TextField(
                                                "불릿 항목",
                                                text: Binding(
                                                    get: { bulletText },
                                                    set: { updated in
                                                        viewModel.updateSlideBulletPoint(
                                                            slideIndex: activeIdx,
                                                            bulletIndex: bIdx,
                                                            newText: updated
                                                        )
                                                    }
                                                )
                                            )
                                            .textFieldStyle(.roundedBorder)
                                            .font(.system(size: 11))
                                        }
                                    }
                                }
                                .frame(maxWidth: .infinity, alignment: .leading)

                                if let activeElem = selectedElement {
                                    VStack(alignment: .leading, spacing: 6) {
                                        Text("선택된 개체 텍스트 편집 (\(activeElem.kind.rawValue))")
                                            .font(.system(size: 11, weight: .bold))
                                            .foregroundStyle(palette.primaryText.swiftUIColor)

                                        TextField(
                                            "개체 제목",
                                            text: Binding(
                                                get: { activeElem.title },
                                                set: { newTitle in
                                                    viewModel.updateSlideElementText(
                                                        elementID: activeElem.id,
                                                        title: newTitle,
                                                        bodyText: activeElem.bodyText
                                                    )
                                                }
                                            )
                                        )
                                        .textFieldStyle(.roundedBorder)
                                        .font(.system(size: 11, weight: .semibold))

                                        TextField(
                                            "개체 본문",
                                            text: Binding(
                                                get: { activeElem.bodyText },
                                                set: { newBody in
                                                    viewModel.updateSlideElementText(
                                                        elementID: activeElem.id,
                                                        title: activeElem.title,
                                                        bodyText: newBody
                                                    )
                                                }
                                            )
                                        )
                                        .textFieldStyle(.roundedBorder)
                                        .font(.system(size: 11))
                                    }
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                }
                            }
                        }

                        Divider()
                        Text("발표자 노트: \(slide.speakerNotes)")
                            .font(.system(size: 11))
                            .foregroundStyle(palette.secondaryText.swiftUIColor)
                    }
                    .padding(24)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(palette.cardSurface.swiftUIColor)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .accessibilityLabel(AccessibilityTokens.slideStageLabel(slideNumber: activeIdx + 1, totalSlides: deck.slides.count))
                }
            }

            if isPrimaryPane, viewModel.isPresenterModeActive {
                HStack(spacing: 16) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("발표자 모니터 · 진행률 \(Int(presenterSession.progressFraction * 100))% · 경과 \(presenterSession.elapsedSeconds)초 (\(viewModel.isPresenterTimerRunning ? "진행 중" : "일시정지"))")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)
                        Text("현재 노트: \(presenterSession.currentSpeakerNotes)")
                            .font(.system(size: 11))
                            .foregroundStyle(palette.secondaryText.swiftUIColor)
                    }
                    Spacer()
                    if let nextSlide = presenterSession.nextSlide {
                        Text("다음 슬라이드: \(nextSlide.title)")
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                    }
                    HStack(spacing: 6) {
                        Button(viewModel.isPresenterTimerRunning ? "타이머 일시정지" : "타이머 시작") {
                            viewModel.isPresenterTimerRunning.toggle()
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)

                        Button("+10초") {
                            viewModel.tickPresenterTimer(seconds: 10)
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)

                        Button("이전 슬라이드") {
                            viewModel.previousPresenterSlide()
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        .disabled(activeIdx == 0)

                        Button("다음 슬라이드") {
                            viewModel.advancePresenterSlide()
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                        .tint(EasyOfficeDesignTokens.brand700.swiftUIColor)
                        .disabled(activeIdx + 1 >= deck.slides.count)
                    }
                }
                .padding(12)
                .background(EasyOfficeDesignTokens.brand50.swiftUIColor)
                .clipShape(RoundedRectangle(cornerRadius: 8))
                .onReceive(Timer.publish(every: 1.0, on: .main, in: .common).autoconnect()) { _ in
                    if viewModel.isPresenterModeActive && viewModel.isPresenterTimerRunning {
                        viewModel.tickPresenterTimer(seconds: 1)
                    }
                }
            }
        }
    }

    private func resolveHexColor(_ hex: String, fallback: RGBColorToken) -> Color {
        resolveOptionalHexColor(hex) ?? fallback.swiftUIColor
    }
}

/// Native AppKit `PDFKit.PDFView` wrapper that caches `lastRenderedModel` in `Coordinator` to avoid re-encoding ISO 32000-1 bytes on unrelated view updates.
public struct PDFKitRepresentedView: NSViewRepresentable {
    public let pdfDocument: PDFStudioDocumentModel
    public let selectedPageIndex: Int
    public let zoomFactor: CGFloat

    public final class Coordinator {
        public var lastRenderedModel: PDFStudioDocumentModel?
        public var lastEncodedByteCount: Int = 0
    }

    public init(
        pdfDocument: PDFStudioDocumentModel,
        selectedPageIndex: Int,
        zoomFactor: CGFloat = 1.0
    ) {
        self.pdfDocument = pdfDocument
        self.selectedPageIndex = selectedPageIndex
        self.zoomFactor = zoomFactor
    }

    public func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    public func makeNSView(context: Context) -> PDFView {
        let pdfView = PDFView()
        pdfView.autoScales = false
        pdfView.displayMode = .singlePageContinuous
        pdfView.displaysPageBreaks = true
        let data = PDFBinarySerializer.encode(pdfDocument)
        pdfView.document = PDFDocument(data: data)
        pdfView.scaleFactor = max(0.25, min(4.0, zoomFactor))
        context.coordinator.lastRenderedModel = pdfDocument
        context.coordinator.lastEncodedByteCount = data.count
        return pdfView
    }

    public func updateNSView(_ nsView: PDFView, context: Context) {
        nsView.autoScales = false
        nsView.scaleFactor = max(0.25, min(4.0, zoomFactor))

        if context.coordinator.lastRenderedModel != pdfDocument {
            let data = PDFBinarySerializer.encode(pdfDocument)
            nsView.document = PDFDocument(data: data)
            context.coordinator.lastRenderedModel = pdfDocument
            context.coordinator.lastEncodedByteCount = data.count
        }

        if let doc = nsView.document,
           selectedPageIndex >= 0,
           selectedPageIndex < doc.pageCount,
           let page = doc.page(at: selectedPageIndex) {
            nsView.go(to: page)
        }
    }
}

public typealias PDFStudioEditorDetailPaneView = PDFStudioDetailPaneView

/// Rich interactive editor pane for PDF Studio (.pdf) with cached native PDFKit preview, interactive freehand vector ink overlay, Page Thumbnails, Annotation Toolbar, and Rotate/Convert controls.
public struct PDFStudioDetailPaneView: View {
    @Environment(\.colorScheme) private var colorScheme
    @Bindable var viewModel: WorkspaceViewModel
    let pdf: PDFStudioDocumentModel
    let isPrimaryPane: Bool

    public init(viewModel: WorkspaceViewModel, pdf: PDFStudioDocumentModel, isPrimaryPane: Bool = true) {
        self.viewModel = viewModel
        self.pdf = pdf
        self.isPrimaryPane = isPrimaryPane
    }

    public var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
        let currentPageNum = viewModel.selectedPDFPageNumber
        let currentPageStrokeCount = pdf.inkStrokes.filter { $0.pageNumber == currentPageNum }.count

        VStack(alignment: .leading, spacing: 14) {
            if isPrimaryPane {
                HStack(spacing: 8) {
                    Button {
                        viewModel.rotateActivePDFPage(clockwiseDegrees: 90)
                    } label: {
                        Label("90° 회전", systemImage: "rotate.right").font(.system(size: 11, weight: .semibold))
                    }
                    .buttonStyle(.bordered)

                    ForEach([PDFAnnotationKind.highlight, .stickyNote, .signature], id: \.rawValue) { kind in
                        Button {
                            viewModel.addPDFAnnotation(kind: kind, content: "\(kind.rawValue) 검토 마크")
                        } label: {
                            Text(kind.rawValue).font(.system(size: 11, weight: .medium))
                        }
                        .buttonStyle(.bordered)
                        .accessibilityLabel(AccessibilityTokens.pdfAnnotationToolLabel(toolTitle: kind.rawValue))
                    }

                    Toggle(isOn: $viewModel.isPDFInkDrawingModeEnabled) {
                        Label("자유곡선 펜 그리기", systemImage: "pencil.tip.crop.circle").font(.system(size: 11, weight: .semibold))
                    }
                    .toggleStyle(.button)
                    .tint(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)

                    Button {
                        viewModel.executeRibbonCommand("pdf-mod-tooffice")
                    } label: {
                        Label("DOCX 편집 변환", systemImage: "arrow.right.doc.on.clipboard").font(.system(size: 11, weight: .semibold))
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)

                    Spacer()

                    Text("주석 \(pdf.annotations.count)건 · 벡터 획 \(currentPageStrokeCount)건 · 책갈피 \(pdf.outline.count)건")
                        .font(.system(size: 11))
                        .foregroundStyle(palette.secondaryText.swiftUIColor)
                }
                .padding(10)
                .background(palette.cardSurface.swiftUIColor)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            }

            if !pdf.pages.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Label("네이티브 PDFKit 벡터 뷰어 (ISO 32000-1 · CoreText CJK 서브셋)", systemImage: "doc.viewfinder.fill")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
                        Spacer()
                        Text("페이지 수: \(pdf.pages.count)p · 총 벡터 획: \(pdf.inkStrokes.count)건")
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(palette.secondaryText.swiftUIColor)
                    }

                    PDFKitRepresentedView(pdfDocument: pdf, selectedPageIndex: viewModel.selectedPDFPageIndex, zoomFactor: CGFloat(viewModel.zoomPercent / 100.0))
                        .frame(height: 260)
                        .clipShape(RoundedRectangle(cornerRadius: 6))

                    if isPrimaryPane {
                        PDFInteractiveInkOverlayView(viewModel: viewModel, palette: palette, pdfDocument: pdf)
                    }
                }
                .padding(12)
                .background(palette.cardSurface.swiftUIColor)
                .overlay(RoundedRectangle(cornerRadius: 8).stroke(palette.border.swiftUIColor, lineWidth: 1))
                .clipShape(RoundedRectangle(cornerRadius: 8))
            }

            ForEach(Array(pdf.pages.enumerated()), id: \.element.id) { idx, page in
                let isSelected = viewModel.selectedPDFPageIndex == idx
                let pageAnnotations = pdf.annotations.filter { $0.pageNumber == page.pageNumber }
                let pageStrokes = pdf.inkStrokes.filter { $0.pageNumber == page.pageNumber }

                VStack(alignment: .leading, spacing: 10) {
                    HStack {
                        Text("PDF 페이지 \(page.pageNumber) · 회전 \(page.rotationDegrees)° · 벡터 획 \(pageStrokes.count)건")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
                        Spacer()
                        if isSelected {
                            Text("선택된 페이지").font(.system(size: 10, weight: .semibold)).foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                        }
                    }

                    Text(page.extractedText).font(.system(size: 13)).foregroundStyle(palette.primaryText.swiftUIColor)

                    if !pageAnnotations.isEmpty {
                        Divider()
                        ForEach(pageAnnotations) { ann in
                            HStack(spacing: 6) {
                                Text("[\(ann.kind.rawValue)]").font(.system(size: 10, weight: .bold)).foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
                                Text(ann.content).font(.system(size: 11)).foregroundStyle(palette.secondaryText.swiftUIColor)
                            }
                        }
                    }
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(palette.cardSurface.swiftUIColor)
                .overlay(
                    RoundedRectangle(cornerRadius: 8)
                        .stroke(isSelected ? EasyOfficeDesignTokens.brand700.swiftUIColor : palette.border.swiftUIColor, lineWidth: isSelected ? 1.5 : 1.0)
                )
                .clipShape(RoundedRectangle(cornerRadius: 8))
                .onTapGesture {
                    guard isPrimaryPane else { return }
                    viewModel.selectedPDFPageIndex = idx
                }
            }
        }
    }
}
