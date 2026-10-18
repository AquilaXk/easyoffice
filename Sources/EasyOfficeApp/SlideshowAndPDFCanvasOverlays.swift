import AppKit
import EasyOfficeCore
import EasyOfficeDesignSystem
import Observation
import SwiftUI

/// Shared multi-geometry renderer for `SlideCanvasElement` supporting all `SlideElementKind` shapes (`.ellipseShape`, `.arrowShape`, `.calloutShape`, `.tableGrid`, `.chartEmbed`, `.roundedRectangleShape`, `.rectangleShape`, `.textBox`).
public struct SlideElementShapeContentView: View {
  public let element: SlideCanvasElement
  public let palette: WorkspaceThemePalette
  public let isSelected: Bool

  public init(
    element: SlideCanvasElement,
    palette: WorkspaceThemePalette,
    isSelected: Bool = false
  ) {
    self.element = element
    self.palette = palette
    self.isSelected = isSelected
  }

  private var fillColor: Color {
    EasyOfficeDesignTokens.resolveHexColor(
      element.fillHex,
      fallback: EasyOfficeDesignTokens.brand50.swiftUIColor
    )
  }

  private var strokeColor: Color {
    if isSelected {
      return EasyOfficeDesignTokens.brand700.swiftUIColor
    }
    return EasyOfficeDesignTokens.resolveHexColor(
      element.strokeHex,
      fallback: EasyOfficeDesignTokens.brand600.swiftUIColor
    )
  }

  private var elementTextColor: Color {
    EasyOfficeDesignTokens.contrastingTextColor(forFillHex: element.fillHex)
  }

  private var elementSecondaryTextColor: Color {
    elementTextColor.opacity(0.84)
  }

  private var strokeLineWidth: CGFloat {
    isSelected ? 2.5 : 1.6
  }

  private var tableCellsMatrix: [[String]] {
    let rawLines = element.bodyText
      .split(separator: "\n", omittingEmptySubsequences: true)
      .map(String.init)
    if !rawLines.isEmpty,
      rawLines.contains(where: { $0.contains("|") || $0.contains(",") })
    {
      return rawLines.prefix(3).map { line in
        let separator: Character = line.contains("|") ? "|" : ","
        return line.split(separator: separator, omittingEmptySubsequences: false)
          .prefix(3)
          .map { $0.trimmingCharacters(in: .whitespaces) }
      }
    }
    return [
      ["구분", "목표", "실적"],
      [element.title, element.bodyText.isEmpty ? "100%" : element.bodyText, "달성"],
    ]
  }

  @ViewBuilder
  private var animationBadgeView: some View {
    if element.animation != .none {
      Text("✦ \(element.animation.rawValue)")
        .font(.system(size: 8, weight: .bold))
        .padding(.horizontal, 4)
        .padding(.vertical, 1)
        .background(palette.selectedSurface.swiftUIColor)
        .foregroundStyle(palette.accentBadgeText.swiftUIColor)
        .clipShape(RoundedRectangle(cornerRadius: 3))
    }
  }

  public var body: some View {
    switch element.kind {
    case .ellipseShape:
      ZStack {
        Ellipse()
          .fill(fillColor.opacity(0.95))
        Ellipse()
          .stroke(strokeColor, lineWidth: strokeLineWidth)

        VStack(spacing: 2) {
          Text(element.title)
            .font(.system(size: 11, weight: element.isBold ? .heavy : .bold))
            .italic(element.isItalic)
            .underline(element.isUnderlined)
            .foregroundStyle(elementTextColor)
            .multilineTextAlignment(.center)
            .lineLimit(1)

          if !element.bodyText.isEmpty {
            Text(element.bodyText)
              .font(.system(size: 9, weight: element.isBold ? .semibold : .regular))
              .italic(element.isItalic)
              .underline(element.isUnderlined)
              .foregroundStyle(elementSecondaryTextColor)
              .multilineTextAlignment(.center)
              .lineLimit(2)
          }

          animationBadgeView
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
      }

    case .arrowShape:
      ZStack {
        Canvas { context, size in
          let w = max(10.0, size.width)
          let h = max(10.0, size.height)
          let headLength = min(w * 0.32, 34.0)
          let shaftRight = max(4.0, w - headLength)
          let shaftTop = h * 0.24
          let shaftBottom = h * 0.76

          var path = Path()
          path.move(to: CGPoint(x: 2.0, y: shaftTop))
          path.addLine(to: CGPoint(x: shaftRight, y: shaftTop))
          path.addLine(to: CGPoint(x: shaftRight, y: 2.0))
          path.addLine(to: CGPoint(x: w - 2.0, y: h * 0.5))
          path.addLine(to: CGPoint(x: shaftRight, y: h - 2.0))
          path.addLine(to: CGPoint(x: shaftRight, y: shaftBottom))
          path.addLine(to: CGPoint(x: 2.0, y: shaftBottom))
          path.closeSubpath()

          context.fill(path, with: .color(fillColor.opacity(0.95)))
          context.stroke(path, with: .color(strokeColor), lineWidth: strokeLineWidth)
        }

        VStack(alignment: .leading, spacing: 1) {
          Text(element.title)
            .font(.system(size: 10, weight: .bold))
            .italic(element.isItalic)
            .underline(element.isUnderlined)
            .foregroundStyle(elementTextColor)
            .lineLimit(1)

          if !element.bodyText.isEmpty {
            Text(element.bodyText)
              .font(.system(size: 9))
              .foregroundStyle(elementSecondaryTextColor)
              .lineLimit(1)
          }
        }
        .padding(.leading, 10)
        .padding(.trailing, 28)
      }

    case .calloutShape:
      VStack(alignment: .leading, spacing: 0) {
        VStack(alignment: .leading, spacing: 3) {
          HStack(spacing: 4) {
            Text("💬 \(element.title)")
              .font(.system(size: 11, weight: element.isBold ? .heavy : .bold))
              .italic(element.isItalic)
              .underline(element.isUnderlined)
              .foregroundStyle(elementTextColor)
              .lineLimit(1)
            Spacer(minLength: 2)
            animationBadgeView
          }

          Text(element.bodyText)
            .font(.system(size: 10, weight: element.isBold ? .semibold : .regular))
            .italic(element.isItalic)
            .underline(element.isUnderlined)
            .foregroundStyle(elementSecondaryTextColor)
            .lineLimit(2)
        }
        .padding(8)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(fillColor.opacity(0.96))
        .overlay(
          RoundedRectangle(cornerRadius: 8)
            .stroke(strokeColor, lineWidth: strokeLineWidth)
        )
        .clipShape(RoundedRectangle(cornerRadius: 8))

        Canvas { context, size in
          var tail = Path()
          tail.move(to: CGPoint(x: 16.0, y: 0.0))
          tail.addLine(to: CGPoint(x: 28.0, y: 0.0))
          tail.addLine(to: CGPoint(x: 12.0, y: size.height))
          tail.closeSubpath()
          context.fill(tail, with: .color(fillColor))
          context.stroke(tail, with: .color(strokeColor), lineWidth: 1.4)
        }
        .frame(height: 9)
      }

    case .tableGrid:
      VStack(alignment: .leading, spacing: 3) {
        HStack {
          Label(element.title, systemImage: "tablecells")
            .font(.system(size: 10, weight: .bold))
            .foregroundStyle(elementTextColor)
            .lineLimit(1)
          Spacer(minLength: 2)
          animationBadgeView
        }

        VStack(spacing: 1) {
          ForEach(Array(tableCellsMatrix.enumerated()), id: \.offset) { rIdx, rowCells in
            HStack(spacing: 1) {
              ForEach(Array(rowCells.enumerated()), id: \.offset) { _, cellVal in
                Text(cellVal)
                  .font(.system(size: 8, weight: rIdx == 0 ? .bold : .regular))
                  .foregroundStyle(palette.primaryText.swiftUIColor)
                  .lineLimit(1)
                  .frame(maxWidth: .infinity, alignment: .leading)
                  .padding(.horizontal, 4)
                  .padding(.vertical, 2)
                  .background(
                    rIdx == 0
                      ? palette.selectedSurface.swiftUIColor
                      : palette.cardSurface.swiftUIColor
                  )
              }
            }
          }
        }
        .clipShape(RoundedRectangle(cornerRadius: 3))
      }
      .padding(6)
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
      .background(fillColor.opacity(0.95))
      .overlay(
        RoundedRectangle(cornerRadius: 6)
          .stroke(strokeColor, lineWidth: strokeLineWidth)
      )
      .clipShape(RoundedRectangle(cornerRadius: 6))

    case .chartEmbed:
      let barRatios: [CGFloat] = [0.45, 0.82, 0.64, 0.95]
      VStack(alignment: .leading, spacing: 4) {
        HStack {
          Label(element.title, systemImage: "chart.bar.fill")
            .font(.system(size: 10, weight: .bold))
            .foregroundStyle(elementTextColor)
            .lineLimit(1)
          Spacer(minLength: 2)
          animationBadgeView
        }

        HStack(alignment: .bottom, spacing: 6) {
          ForEach(Array(barRatios.enumerated()), id: \.offset) { idx, ratio in
            RoundedRectangle(cornerRadius: 2)
              .fill(
                idx % 2 == 0
                  ? EasyOfficeDesignTokens.brand700.swiftUIColor
                  : EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor
              )
              .frame(maxWidth: .infinity)
              .frame(height: max(8.0, 26.0 * ratio))
          }
        }
        .frame(maxHeight: .infinity, alignment: .bottom)

        if !element.bodyText.isEmpty {
          Text(element.bodyText)
            .font(.system(size: 8, design: .monospaced))
            .foregroundStyle(elementSecondaryTextColor)
            .lineLimit(1)
        }
      }
      .padding(7)
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
      .background(fillColor.opacity(0.95))
      .overlay(
        RoundedRectangle(cornerRadius: 6)
          .stroke(strokeColor, lineWidth: strokeLineWidth)
      )
      .clipShape(RoundedRectangle(cornerRadius: 6))

    case .roundedRectangleShape, .rectangleShape, .textBox:
      let cornerRadius: CGFloat =
        element.kind == .roundedRectangleShape
        ? 12.0
        : (element.kind == .rectangleShape ? 2.0 : 6.0)

      VStack(alignment: .leading, spacing: 2) {
        HStack(spacing: 4) {
          Text(element.title)
            .font(.system(size: 11, weight: element.isBold ? .heavy : .bold))
            .italic(element.isItalic)
            .underline(element.isUnderlined)
            .foregroundStyle(elementTextColor)
            .lineLimit(1)
          Spacer(minLength: 2)
          animationBadgeView
        }
        Text(element.bodyText)
          .font(.system(size: 10, weight: element.isBold ? .semibold : .regular))
          .italic(element.isItalic)
          .underline(element.isUnderlined)
          .foregroundStyle(elementSecondaryTextColor)
          .lineLimit(2)
      }
      .padding(8)
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
      .background(fillColor.opacity(0.95))
      .overlay(
        RoundedRectangle(cornerRadius: cornerRadius)
          .stroke(strokeColor, lineWidth: strokeLineWidth)
      )
      .clipShape(RoundedRectangle(cornerRadius: cornerRadius))
    }
  }
}

/// Full-window dark presentation stage displaying the active slide at 16:9 scale with animated element entrance transitions, live glowing Laser Pointer, speaker notes HUD drawer, elapsed timer, and ESC exit.
public struct SlideshowFullScreenOverlayView: View {
  @Bindable var viewModel: WorkspaceViewModel
  public let palette: WorkspaceThemePalette
  @State private var laserPointerLocation: CGPoint = CGPoint(x: 420, y: 240)
  @State private var isStageAnimatedIn: Bool = false

  public init(viewModel: WorkspaceViewModel, palette: WorkspaceThemePalette) {
    self.viewModel = viewModel
    self.palette = palette
  }

  private var activeDeck: PresentationDeckModel? {
    if let tab = viewModel.activeTab, case .slide(let deck) = tab.payload {
      return deck
    }
    for tab in viewModel.openTabs {
      if case .slide(let deck) = tab.payload {
        return deck
      }
    }
    return nil
  }

  public var body: some View {
    let deck =
      activeDeck
      ?? PresentationDeckModel(
        title: "발표 슬라이드",
        aspectRatio: .widescreen16x9,
        slides: [SlideModel(title: "발표 스테이지")]
      )
    let activeIdx =
      deck.slides.indices.contains(viewModel.selectedSlideIndex) ? viewModel.selectedSlideIndex : 0
    let currentSlide = deck.slides.indices.contains(activeIdx) ? deck.slides[activeIdx] : nil
    let presenterSession = PresenterSession(
      deck: deck,
      currentSlideIndex: activeIdx,
      elapsedSeconds: viewModel.presenterElapsedSeconds,
      isTimerRunning: viewModel.isPresenterTimerRunning
    )

    ZStack {
      EasyOfficeDesignTokens.darkScaffold.swiftUIColor
        .opacity(0.97)
        .ignoresSafeArea()

      VStack(spacing: 16) {
        // Top Presenter Header Bar
        HStack(spacing: 12) {
          Label(
            "전체화면 발표 스테이지 · \(activeIdx + 1) / \(max(1, deck.slides.count))",
            systemImage: "play.tv.fill"
          )
          .font(.system(size: 13, weight: .bold))
          .foregroundStyle(EasyOfficeDesignTokens.darkText.swiftUIColor)

          Text("경과: \(presenterSession.elapsedSeconds)초")
            .font(.system(size: 12, weight: .semibold, design: .monospaced))
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(EasyOfficeDesignTokens.darkElevated.swiftUIColor)
            .foregroundStyle(EasyOfficeDesignTokens.brand200.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 5))

          Toggle(isOn: $viewModel.isLaserPointerEnabled) {
            Label("레이저 포인터", systemImage: "laser.burst")
              .font(.system(size: 12, weight: .semibold))
              .foregroundStyle(EasyOfficeDesignTokens.darkText.swiftUIColor)
          }
          .toggleStyle(.button)
          .tint(EasyOfficeDesignTokens.statusDanger.swiftUIColor)

          Spacer()

          Button("이전 슬라이드") {
            isStageAnimatedIn = false
            viewModel.previousPresenterSlide()
            withAnimation(.easeOut(duration: 0.35)) {
              isStageAnimatedIn = true
            }
          }
          .buttonStyle(.bordered)
          .disabled(activeIdx == 0)

          Button("다음 슬라이드") {
            isStageAnimatedIn = false
            viewModel.advancePresenterSlide()
            withAnimation(.easeOut(duration: 0.35)) {
              isStageAnimatedIn = true
            }
          }
          .buttonStyle(.borderedProminent)
          .tint(EasyOfficeDesignTokens.brand600.swiftUIColor)
          .disabled(activeIdx + 1 >= deck.slides.count)

          Button("슬라이드쇼 종료 (ESC)") {
            viewModel.toggleFullScreenSlideshow()
          }
          .buttonStyle(.borderedProminent)
          .tint(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
          .keyboardShortcut(.escape, modifiers: [])
        }
        .padding(.horizontal, 24)
        .padding(.top, 16)

        // Large 16:9 Presentation Stage with Element Entrance Transitions & Glowing Laser Pointer
        if let slide = currentSlide {
          GeometryReader { geo in
            let stageW = max(1.0, deck.aspectRatio.widthPoints)
            let stageH = max(1.0, deck.aspectRatio.heightPoints)
            let scaleX = max(0.01, geo.size.width / stageW)
            let scaleY = max(0.01, geo.size.height / stageH)

            ZStack(alignment: .topLeading) {
              RoundedRectangle(cornerRadius: 12)
                .fill(EasyOfficeDesignTokens.lightWhite.swiftUIColor)
                .shadow(color: Color.black.opacity(0.45), radius: 18, x: 0, y: 8)

              VStack(alignment: .leading, spacing: 12) {
                Text(slide.title)
                  .font(.system(size: 28, weight: .bold))
                  .foregroundStyle(EasyOfficeDesignTokens.inkPrimary.swiftUIColor)

                if !slide.subtitle.isEmpty {
                  Text(slide.subtitle)
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                }

                ForEach(slide.bulletPoints, id: \.self) { bullet in
                  Text("• \(bullet)")
                    .font(.system(size: 15, weight: .medium))
                    .foregroundStyle(EasyOfficeDesignTokens.inkPrimary.swiftUIColor)
                }
              }
              .padding(32)

              ForEach(slide.elements) { element in
                let elemW = max(90.0, CGFloat(element.width) * scaleX)
                let elemH = max(50.0, CGFloat(element.height) * scaleY)
                let centerX = CGFloat(element.centerX) * scaleX
                let centerY = CGFloat(element.centerY) * scaleY
                let entranceScale: CGFloat =
                  (element.animation == .zoomIn && !isStageAnimatedIn) ? 0.72 : 1.0
                let entranceOffsetY: CGFloat =
                  (element.animation == .flyInUp && !isStageAnimatedIn) ? 36.0 : 0.0
                let entranceOpacity: Double =
                  (element.animation != .none && !isStageAnimatedIn) ? 0.15 : 1.0

                SlideElementShapeContentView(
                  element: element,
                  palette: palette,
                  isSelected: false
                )
                .frame(width: elemW, height: elemH)
                .scaleEffect(entranceScale)
                .offset(y: entranceOffsetY)
                .opacity(entranceOpacity)
                .rotationEffect(.degrees(element.rotationDegrees))
                .position(x: centerX, y: centerY)
                .zIndex(Double(element.zIndex))
              }

              if viewModel.isLaserPointerEnabled {
                ZStack {
                  Circle()
                    .fill(EasyOfficeDesignTokens.statusDanger.swiftUIColor.opacity(0.32))
                    .frame(width: 28, height: 28)
                  Circle()
                    .fill(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
                    .frame(width: 12, height: 12)
                    .shadow(color: EasyOfficeDesignTokens.statusDanger.swiftUIColor, radius: 8)
                }
                .position(laserPointerLocation)
                .allowsHitTesting(false)
              }
            }
            .contentShape(Rectangle())
            .onContinuousHover { phase in
              if case .active(let loc) = phase {
                laserPointerLocation = loc
              }
            }
            .gesture(
              DragGesture(minimumDistance: 0)
                .onChanged { value in
                  laserPointerLocation = value.location
                }
            )
          }
          .padding(.horizontal, 36)

          // Bottom Speaker Notes HUD Drawer
          HStack(alignment: .top, spacing: 16) {
            VStack(alignment: .leading, spacing: 4) {
              Text("발표자 노트 HUD (전환: \(slide.transition.rawValue))")
                .font(.system(size: 11, weight: .bold))
                .foregroundStyle(EasyOfficeDesignTokens.brand200.swiftUIColor)
              Text(slide.speakerNotes.isEmpty ? "등록된 발표자 노트가 없습니다." : slide.speakerNotes)
                .font(.system(size: 12))
                .foregroundStyle(EasyOfficeDesignTokens.darkText.swiftUIColor)
            }
            Spacer()
            if let nextSlide = presenterSession.nextSlide {
              Text("다음 슬라이드: \(nextSlide.title)")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(EasyOfficeDesignTokens.brand300.swiftUIColor)
            }
          }
          .padding(16)
          .background(EasyOfficeDesignTokens.darkSurface.swiftUIColor)
          .clipShape(RoundedRectangle(cornerRadius: 10))
          .padding(.horizontal, 36)
          .padding(.bottom, 20)
        }
      }
    }
    .onAppear {
      withAnimation(.easeOut(duration: 0.35)) {
        isStageAnimatedIn = true
      }
    }
    .onReceive(Timer.publish(every: 1.0, on: .main, in: .common).autoconnect()) { _ in
      if viewModel.isFullScreenSlideshowActive && viewModel.isPresenterTimerRunning
        && !viewModel.isPresenterModeActive
      {
        viewModel.tickPresenterTimer(seconds: 1)
      }
    }
  }
}

/// Interactive freehand vector ink drawing & text-selection highlight overlay for PDF Studio.
public struct PDFInteractiveInkOverlayView: View {
  @Bindable var viewModel: WorkspaceViewModel
  public let palette: WorkspaceThemePalette
  public let pdfDocument: PDFStudioDocumentModel
  @State private var activeStrokePoints: [PDFInkPoint] = []
  @State private var highlightSelectionText: String = "핵심 보안 아키텍처"

  public init(
    viewModel: WorkspaceViewModel,
    palette: WorkspaceThemePalette,
    pdfDocument: PDFStudioDocumentModel
  ) {
    self.viewModel = viewModel
    self.palette = palette
    self.pdfDocument = pdfDocument
  }

  public var body: some View {
    let currentPageNumber = viewModel.selectedPDFPageNumber
    let pageStrokes = pdfDocument.inkStrokes.filter { $0.pageNumber == currentPageNumber }
    let activeColor = EasyOfficeDesignTokens.resolveHexColor(
      viewModel.pdfInkStrokeColorHex,
      fallback: EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor
    )

    VStack(alignment: .leading, spacing: 8) {
      // Freehand Ink & Quick Highlight Bar
      HStack(spacing: 8) {
        Label("자유곡선 벡터 펜 · 본문 하이라이트", systemImage: "scribble.variable")
          .font(.system(size: 11, weight: .bold))
          .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)

        Text(viewModel.isPDFInkDrawingModeEnabled ? "펜 활성" : "펜 대기")
          .font(.system(size: 10, weight: .bold))
          .padding(.horizontal, 6)
          .padding(.vertical, 2)
          .background(
            viewModel.isPDFInkDrawingModeEnabled
              ? palette.selectedSurface.swiftUIColor
              : palette.cardSurface.swiftUIColor
          )
          .foregroundStyle(
            viewModel.isPDFInkDrawingModeEnabled
              ? palette.accentBadgeText.swiftUIColor
              : palette.secondaryText.swiftUIColor
          )
          .clipShape(Capsule())

        ForEach(["#5C6BC0", "#D9383A", "#0A705A"], id: \.self) { hex in
          let chipColor = EasyOfficeDesignTokens.resolveHexColor(
            hex,
            fallback: EasyOfficeDesignTokens.brand700.swiftUIColor
          )
          Button {
            viewModel.pdfInkStrokeColorHex = hex
            viewModel.isPDFInkDrawingModeEnabled = true
          } label: {
            Circle()
              .fill(chipColor)
              .frame(width: 14, height: 14)
              .overlay(
                Circle()
                  .stroke(
                    viewModel.pdfInkStrokeColorHex.uppercased() == hex
                      ? palette.primaryText.swiftUIColor
                      : Color.clear,
                    lineWidth: 1.5
                  )
              )
          }
          .buttonStyle(.plain)
        }

        Text("굵기: \(String(format: "%.1f", viewModel.pdfInkLineWidth))pt")
          .font(.system(size: 10, design: .monospaced))
          .foregroundStyle(palette.secondaryText.swiftUIColor)

        Divider().frame(height: 14)

        TextField("하이라이트할 본문 구절...", text: $highlightSelectionText)
          .textFieldStyle(.roundedBorder)
          .font(.system(size: 11))
          .frame(width: 165)

        Button {
          viewModel.addHighlightToActivePDFPage(selectedText: highlightSelectionText)
        } label: {
          Label("구절 하이라이트", systemImage: "highlighter")
            .font(.system(size: 11, weight: .semibold))
        }
        .buttonStyle(.bordered)
        .controlSize(.small)

        Spacer()

        Text("현재 페이지 벡터 획: \(pageStrokes.count)건")
          .font(.system(size: 10, weight: .semibold, design: .monospaced))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
      }

      // Interactive Vector Drawing Canvas Overlay
      GeometryReader { geo in
        let canvasW = max(1.0, geo.size.width)
        let canvasH = max(1.0, geo.size.height)

        ZStack {
          RoundedRectangle(cornerRadius: 6)
            .fill(
              viewModel.isPDFInkDrawingModeEnabled
                ? palette.subtleAccentSurface.swiftUIColor.opacity(0.45)
                : palette.cardSurface.swiftUIColor.opacity(0.45)
            )
            .overlay(
              RoundedRectangle(cornerRadius: 6)
                .stroke(
                  viewModel.isPDFInkDrawingModeEnabled
                    ? EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor.opacity(0.65)
                    : palette.border.swiftUIColor,
                  style: StrokeStyle(lineWidth: 1.2, dash: [5, 3])
                )
            )

          Canvas { context, size in
            for stroke in pageStrokes where stroke.points.count >= 2 {
              var path = Path()
              for (ptIdx, pt) in stroke.points.enumerated() {
                let vx = CGFloat((pt.x - 20.0) / 555.0) * size.width
                let vy = size.height - (CGFloat((pt.y - 20.0) / 800.0) * size.height)
                let point = CGPoint(x: max(0, min(size.width, vx)), y: max(0, min(size.height, vy)))
                if ptIdx == 0 {
                  path.move(to: point)
                } else {
                  path.addLine(to: point)
                }
              }
              let strokeColor = EasyOfficeDesignTokens.resolveHexColor(
                stroke.strokeHex,
                fallback: EasyOfficeDesignTokens.brand700.swiftUIColor
              )
              context.stroke(
                path,
                with: .color(strokeColor),
                style: StrokeStyle(
                  lineWidth: CGFloat(stroke.lineWidth), lineCap: .round, lineJoin: .round)
              )
            }

            if activeStrokePoints.count >= 2 {
              var livePath = Path()
              for (ptIdx, pt) in activeStrokePoints.enumerated() {
                let vx = CGFloat((pt.x - 20.0) / 555.0) * size.width
                let vy = size.height - (CGFloat((pt.y - 20.0) / 800.0) * size.height)
                let point = CGPoint(x: max(0, min(size.width, vx)), y: max(0, min(size.height, vy)))
                if ptIdx == 0 {
                  livePath.move(to: point)
                } else {
                  livePath.addLine(to: point)
                }
              }
              context.stroke(
                livePath,
                with: .color(activeColor),
                style: StrokeStyle(
                  lineWidth: CGFloat(viewModel.pdfInkLineWidth), lineCap: .round, lineJoin: .round)
              )
            }
          }
        }
        .contentShape(Rectangle())
        .gesture(
          DragGesture(minimumDistance: 1)
            .onChanged { value in
              guard viewModel.isPDFInkDrawingModeEnabled else { return }
              let normX = max(0.0, min(1.0, Double(value.location.x / canvasW)))
              let normY = max(0.0, min(1.0, Double(1.0 - (value.location.y / canvasH))))
              let pdfX = 20.0 + normX * 555.0
              let pdfY = 20.0 + normY * 800.0
              activeStrokePoints.append(PDFInkPoint(x: pdfX, y: pdfY))
            }
            .onEnded { _ in
              guard viewModel.isPDFInkDrawingModeEnabled else {
                activeStrokePoints.removeAll(keepingCapacity: true)
                return
              }
              if activeStrokePoints.count >= 2 {
                viewModel.addInkStrokeToActivePDFPage(
                  points: activeStrokePoints,
                  strokeHex: viewModel.pdfInkStrokeColorHex,
                  lineWidth: viewModel.pdfInkLineWidth
                )
              }
              activeStrokePoints.removeAll(keepingCapacity: true)
            }
        )
      }
      .frame(height: 120)
    }
    .padding(10)
    .background(palette.elevatedSurface.swiftUIColor)
    .clipShape(RoundedRectangle(cornerRadius: 6))
  }
}
