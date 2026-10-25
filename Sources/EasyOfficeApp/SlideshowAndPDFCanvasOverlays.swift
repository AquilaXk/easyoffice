import AppKit
import Combine
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
        .foregroundStyle(palette.accentBadgeText.swiftUIColor)
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
      HStack(alignment: .top, spacing: 8) {
        Rectangle()
          .fill(strokeColor)
          .frame(width: 2.5)
          .padding(.vertical, 2)

        VStack(alignment: .leading, spacing: 4) {
          HStack(spacing: 4) {
            Text(element.title)
              .font(.system(size: 11, weight: element.isBold ? .heavy : .bold))
              .italic(element.isItalic)
              .underline(element.isUnderlined)
              .foregroundStyle(strokeColor)
              .lineLimit(1)
            Spacer(minLength: 2)
            animationBadgeView
          }

          Text(element.bodyText)
            .font(.system(size: 10, weight: element.isBold ? .semibold : .regular))
            .italic(element.isItalic)
            .underline(element.isUnderlined)
            .foregroundStyle(palette.primaryText.swiftUIColor)
            .lineLimit(2)
        }
      }
      .padding(.vertical, 8)
      .padding(.horizontal, 10)
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
      .background(Color(red: 0.97, green: 0.98, blue: 0.99))
      .border(
        isSelected ? EasyOfficeDesignTokens.brand700.swiftUIColor : palette.border.swiftUIColor,
        width: isSelected ? 1.5 : 0.8
      )

    case .tableGrid:
      VStack(alignment: .leading, spacing: 3) {
        HStack {
          Label(element.title, systemImage: "tablecells")
            .font(.system(size: 10, weight: .bold))
            .foregroundStyle(strokeColor)
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
                      ? palette.elevatedSurface.swiftUIColor
                      : palette.cardSurface.swiftUIColor
                  )
              }
            }
          }
        }
        .border(palette.border.swiftUIColor, width: 0.8)
      }
      .padding(4)
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
      .border(isSelected ? EasyOfficeDesignTokens.brand700.swiftUIColor : Color.clear, width: 1)

    case .chartEmbed:
      let barRatios: [CGFloat] = [0.45, 0.82, 0.64, 0.95]
      VStack(alignment: .leading, spacing: 4) {
        HStack {
          Label(element.title, systemImage: "chart.bar.fill")
            .font(.system(size: 10, weight: .bold))
            .foregroundStyle(strokeColor)
            .lineLimit(1)
          Spacer(minLength: 2)
          animationBadgeView
        }

        HStack(alignment: .bottom, spacing: 6) {
          ForEach(Array(barRatios.enumerated()), id: \.offset) { idx, ratio in
            Rectangle()
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
            .foregroundStyle(palette.secondaryText.swiftUIColor)
            .lineLimit(1)
        }
      }
      .padding(6)
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
      .border(isSelected ? EasyOfficeDesignTokens.brand700.swiftUIColor : Color.clear, width: 1)

    case .roundedRectangleShape, .rectangleShape, .textBox:
      HStack(alignment: .top, spacing: 8) {
        Rectangle()
          .fill(strokeColor)
          .frame(width: 2)
          .padding(.vertical, 2)

        VStack(alignment: .leading, spacing: 4) {
          HStack(spacing: 4) {
            Text(element.title)
              .font(.system(size: 11.5, weight: element.isBold ? .heavy : .bold))
              .italic(element.isItalic)
              .underline(element.isUnderlined)
              .foregroundStyle(strokeColor)
              .lineLimit(1)
            Spacer(minLength: 2)
            animationBadgeView
          }
          Text(element.bodyText)
            .font(.system(size: 10, weight: element.isBold ? .semibold : .regular))
            .italic(element.isItalic)
            .underline(element.isUnderlined)
            .foregroundStyle(palette.primaryText.swiftUIColor)
            .lineLimit(2)
        }
      }
      .padding(.vertical, 8)
      .padding(.horizontal, 10)
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
      .background(Color(red: 0.97, green: 0.98, blue: 0.99))
      .border(
        isSelected ? EasyOfficeDesignTokens.brand700.swiftUIColor : palette.border.swiftUIColor,
        width: isSelected ? 1.5 : 0.8
      )
    }
  }
}

/// Full-window dark presentation stage displaying the active slide at 16:9 scale with animated element entrance transitions, live glowing Laser Pointer, speaker notes HUD drawer, elapsed timer, and ESC exit.
public struct SlideshowFullScreenOverlayView: View {
  @Bindable var viewModel: WorkspaceViewModel
  public let palette: WorkspaceThemePalette
  @State private var laserPointerLocation: CGPoint = CGPoint(x: 420, y: 240)
  @State private var isStageAnimatedIn: Bool = false
  @State private var autoAdvanceElapsedForSlide: Int = 0
  private let presenterTimer = Timer.publish(every: 1.0, on: .main, in: .common).autoconnect()

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
        HStack(spacing: 14) {
          Label(
            "발표 스테이지 · \(activeIdx + 1) / \(max(1, deck.slides.count))",
            systemImage: "play.tv.fill"
          )
          .font(.system(size: 13, weight: .bold))
          .foregroundStyle(EasyOfficeDesignTokens.darkText.swiftUIColor)

          Text("경과 \(presenterSession.elapsedSeconds)초")
            .font(.system(size: 12, weight: .semibold, design: .monospaced))
            .foregroundStyle(EasyOfficeDesignTokens.brand200.swiftUIColor)

          FlatToggleControl(
            title: "레이저 포인터",
            systemImage: "laser.burst",
            isOn: $viewModel.isLaserPointerEnabled,
            palette: palette
          )

          Spacer()

          Button("이전 슬라이드") {
            isStageAnimatedIn = false
            viewModel.previousPresenterSlide()
            withAnimation(.easeOut(duration: 0.35)) {
              isStageAnimatedIn = true
            }
          }
          .buttonStyle(.plain)
          .font(.system(size: 12, weight: .medium))
          .foregroundStyle(EasyOfficeDesignTokens.darkText.swiftUIColor)
          .disabled(activeIdx == 0)

          Button("다음 슬라이드") {
            isStageAnimatedIn = false
            viewModel.advancePresenterSlide()
            withAnimation(.easeOut(duration: 0.35)) {
              isStageAnimatedIn = true
            }
          }
          .buttonStyle(.plain)
          .font(.system(size: 12, weight: .semibold))
          .foregroundStyle(EasyOfficeDesignTokens.brand200.swiftUIColor)
          .disabled(activeIdx + 1 >= deck.slides.count)

          Button("슬라이드쇼 종료 (ESC)") {
            viewModel.toggleFullScreenSlideshow()
          }
          .buttonStyle(.plain)
          .font(.system(size: 12, weight: .semibold))
          .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
          .keyboardShortcut(.escape, modifiers: [])
        }
        .padding(.horizontal, 24)
        .padding(.top, 16)

        if let slide = currentSlide {
          GeometryReader { geo in
            let stageW = max(1.0, deck.aspectRatio.widthPoints)
            let stageH = max(1.0, deck.aspectRatio.heightPoints)
            let scaleX = max(0.01, geo.size.width / stageW)
            let scaleY = max(0.01, geo.size.height / stageH)

            ZStack(alignment: .topLeading) {
              Rectangle()
                .fill(
                  EasyOfficeDesignTokens.resolveHexColor(
                    deck.deckTheme.primaryFillHex,
                    fallback: EasyOfficeDesignTokens.lightWhite.swiftUIColor
                  )
                )
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
            .simultaneousGesture(
              TapGesture()
                .onEnded {
                  guard !viewModel.isLaserPointerEnabled else { return }
                  if activeIdx + 1 < deck.slides.count {
                    isStageAnimatedIn = false
                    autoAdvanceElapsedForSlide = 0
                    viewModel.advancePresenterSlide()
                    withAnimation(
                      .easeOut(duration: max(0.2, slide.transitionDurationSeconds))
                    ) {
                      isStageAnimatedIn = true
                    }
                  }
                }
            )
          }
          .padding(.horizontal, 36)

          HStack(alignment: .top, spacing: 16) {
            VStack(alignment: .leading, spacing: 4) {
              Text("발표자 노트 (전환: \(slide.transition.rawValue))")
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
    .onReceive(presenterTimer) { _ in
      if viewModel.isFullScreenSlideshowActive {
        if viewModel.isPresenterTimerRunning && !viewModel.isPresenterModeActive {
          viewModel.tickPresenterTimer(seconds: 1)
        }
        if let autoSeconds = currentSlide?.autoAdvanceSeconds, autoSeconds > 0 {
          autoAdvanceElapsedForSlide += 1
          if autoAdvanceElapsedForSlide >= autoSeconds && activeIdx + 1 < deck.slides.count {
            autoAdvanceElapsedForSlide = 0
            isStageAnimatedIn = false
            viewModel.advancePresenterSlide()
            withAnimation(
              .easeOut(duration: max(0.2, currentSlide?.transitionDurationSeconds ?? 0.4))
            ) {
              isStageAnimatedIn = true
            }
          }
        }
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
      HStack(spacing: 10) {
        Label("자유곡선 펜 · 본문 하이라이트", systemImage: "scribble.variable")
          .font(.system(size: 11, weight: .semibold))
          .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)

        Text(viewModel.isPDFInkDrawingModeEnabled ? "펜 활성" : "펜 대기")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(
            viewModel.isPDFInkDrawingModeEnabled
              ? palette.accentBadgeText.swiftUIColor
              : palette.secondaryText.swiftUIColor
          )

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
              .frame(width: 12, height: 12)
              .opacity(viewModel.pdfInkStrokeColorHex.uppercased() == hex ? 1.0 : 0.55)
          }
          .buttonStyle(.plain)
        }

        Text("굵기 \(String(format: "%.1f", viewModel.pdfInkLineWidth))pt")
          .font(.system(size: 10, design: .monospaced))
          .foregroundStyle(palette.secondaryText.swiftUIColor)

        Divider().frame(height: 14)

        FlatUnderlineTextField(
          placeholder: "하이라이트할 본문 구절...",
          text: $highlightSelectionText,
          width: 165,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )

        Button {
          viewModel.addHighlightToActivePDFPage(selectedText: highlightSelectionText)
        } label: {
          Label("구절 하이라이트", systemImage: "highlighter")
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
        }
        .buttonStyle(.plain)

        Spacer()

        Text("벡터 획 \(pageStrokes.count)건")
          .font(.system(size: 10, weight: .medium, design: .monospaced))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
      }

      GeometryReader { geo in
        let canvasW = max(1.0, geo.size.width)
        let canvasH = max(1.0, geo.size.height)

        ZStack {
          Rectangle()
            .fill(palette.elevatedSurface.swiftUIColor)
            .border(
              viewModel.isPDFInkDrawingModeEnabled
                ? EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor.opacity(0.65)
                : palette.border.swiftUIColor,
              width: 1
            )

          Canvas { context, size in
            var strokeList: [(points: [PDFInkPoint], color: Color, width: Double)] =
              pageStrokes.compactMap { stroke in
                guard stroke.points.count >= 2 else { return nil }
                let strokeColor = EasyOfficeDesignTokens.resolveHexColor(
                  stroke.strokeHex,
                  fallback: EasyOfficeDesignTokens.brand700.swiftUIColor
                )
                return (stroke.points, strokeColor, stroke.lineWidth)
              }
            if activeStrokePoints.count >= 2 {
              strokeList.append((activeStrokePoints, activeColor, viewModel.pdfInkLineWidth))
            }
            for item in strokeList {
              var path = Path()
              for (ptIdx, pt) in item.points.enumerated() {
                let vx = CGFloat((pt.x - 20.0) / 555.0) * size.width
                let vy = size.height - (CGFloat((pt.y - 20.0) / 800.0) * size.height)
                let point = CGPoint(x: max(0, min(size.width, vx)), y: max(0, min(size.height, vy)))
                if ptIdx == 0 {
                  path.move(to: point)
                } else {
                  path.addLine(to: point)
                }
              }
              context.stroke(
                path,
                with: .color(item.color),
                style: StrokeStyle(
                  lineWidth: CGFloat(item.width), lineCap: .round, lineJoin: .round)
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
      .frame(height: 96)
    }
    .padding(.vertical, 8)
  }
}
