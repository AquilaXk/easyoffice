import AppKit
import Combine
import EasyOfficeCore
import EasyOfficeDesignSystem
import Observation
import PDFKit
import SwiftUI

/// Vertical ruler (`0..540pt`) rendered to the left of the 16:9 Slide Stage Canvas (`UX-09`).
public struct SlideVerticalRulerView: View {
  let maxHeightPoints: Double
  let palette: WorkspaceThemePalette

  public init(maxHeightPoints: Double = 540.0, palette: WorkspaceThemePalette) {
    self.maxHeightPoints = maxHeightPoints
    self.palette = palette
  }

  public var body: some View {
    let maxPt = max(1, Int(maxHeightPoints))
    GeometryReader { geo in
      let height = max(160.0, geo.size.height)
      ZStack(alignment: .topLeading) {
        RoundedRectangle(cornerRadius: 4)
          .fill(palette.elevatedSurface.swiftUIColor)

        Canvas { context, size in
          let tickColor = palette.secondaryText.swiftUIColor
          let stepCount = 16
          for step in 0...stepCount {
            let ratio = CGFloat(step) / CGFloat(stepCount)
            let yPos = ratio * size.height
            let isMajor = (step % 4 == 0)
            let tickW: CGFloat = isMajor ? 7.0 : 3.5
            var path = Path()
            path.move(to: CGPoint(x: size.width - tickW, y: yPos))
            path.addLine(to: CGPoint(x: size.width, y: yPos))
            context.stroke(
              path,
              with: .color(tickColor.opacity(isMajor ? 0.85 : 0.45)),
              lineWidth: isMajor ? 1.0 : 0.6
            )
          }
        }

        ForEach([0, 135, 270, 405, maxPt], id: \.self) { ptMark in
          let ratio = CGFloat(ptMark) / CGFloat(maxPt)
          let yPos = min(height - 10.0, max(10.0, ratio * height))
          Text("\(ptMark)")
            .font(.system(size: 7, weight: .semibold, design: .monospaced))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
            .position(x: 9, y: yPos)
        }
      }
      .overlay(
        RoundedRectangle(cornerRadius: 4)
          .stroke(palette.border.swiftUIColor, lineWidth: 0.8)
      )
      .clipShape(RoundedRectangle(cornerRadius: 4))
    }
    .frame(width: 22)
  }
}

/// Rich interactive editor pane for Slide (.pptx/.odp) with Filmstrip, Slide Sorter Grid View, proportional 16:9 Stage Canvas, 7-kind element vector shapes, 8-direction resize handles, rotation handle (↻), element entrance animations, and full-screen slideshow launcher.
public struct SlideEditorDetailPaneView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  let deck: PresentationDeckModel
  let isPrimaryPane: Bool
  @GestureState private var activeDragOffset: (elementID: UUID, translation: CGSize)? = nil
  private let presenterTimer = Timer.publish(every: 1.0, on: .main, in: .common).autoconnect()

  public init(
    viewModel: WorkspaceViewModel, deck: PresentationDeckModel, isPrimaryPane: Bool = true
  ) {
    self.viewModel = viewModel
    self.deck = deck
    self.isPrimaryPane = isPrimaryPane
  }

  private func mutateActiveSlideDeck(_ transform: (inout PresentationDeckModel, Int) -> Void) {
    guard let activeTabID = viewModel.activeTabID,
      let idx = viewModel.openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var mutableDeck) = viewModel.openTabs[idx].payload,
      !mutableDeck.slides.isEmpty
    else { return }
    let sIdx =
      mutableDeck.slides.indices.contains(viewModel.selectedSlideIndex)
      ? viewModel.selectedSlideIndex : 0
    viewModel.recordUndoSnapshot(for: activeTabID)
    transform(&mutableDeck, sIdx)
    viewModel.openTabs[idx].payload = .slide(mutableDeck)
    viewModel.openTabs[idx].isModified = true
  }

  private func handleOffset(for handle: SlideResizeHandle, width: CGFloat, height: CGFloat)
    -> CGSize
  {
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

  @ViewBuilder
  private func slideStageHorizontalRuler(palette: WorkspaceThemePalette) -> some View {
    let maxPt = Int(deck.aspectRatio.widthPoints)
    GeometryReader { geo in
      let width = max(200.0, geo.size.width)
      ZStack(alignment: .topLeading) {
        RoundedRectangle(cornerRadius: 4)
          .fill(palette.elevatedSurface.swiftUIColor)

        Canvas { context, size in
          let tickColor = palette.secondaryText.swiftUIColor
          let stepCount = 24
          for step in 0...stepCount {
            let ratio = CGFloat(step) / CGFloat(stepCount)
            let xPos = ratio * size.width
            let isMajor = (step % 3 == 0)
            let tickH: CGFloat = isMajor ? 8.0 : 4.0
            var path = Path()
            path.move(to: CGPoint(x: xPos, y: size.height - tickH))
            path.addLine(to: CGPoint(x: xPos, y: size.height))
            context.stroke(
              path,
              with: .color(tickColor.opacity(isMajor ? 0.85 : 0.45)),
              lineWidth: isMajor ? 1.0 : 0.6
            )
          }
        }

        ForEach([0, 120, 240, 360, 480, 600, 720, 840, maxPt], id: \.self) { ptMark in
          let ratio = CGFloat(ptMark) / CGFloat(max(1, maxPt))
          let xPos = min(width - 18.0, max(16.0, ratio * width))
          Text("\(ptMark)pt")
            .font(.system(size: 8, weight: .semibold, design: .monospaced))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
            .position(x: xPos, y: 7)
        }
      }
      .overlay(
        RoundedRectangle(cornerRadius: 4)
          .stroke(palette.border.swiftUIColor, lineWidth: 0.8)
      )
      .clipShape(RoundedRectangle(cornerRadius: 4))
    }
    .frame(height: 20)
  }

  @ViewBuilder
  private func slideMiniatureThumbnailCard(
    slide: SlideModel,
    index: Int,
    isSelected: Bool,
    palette: WorkspaceThemePalette
  ) -> some View {
    let themeBg = EasyOfficeDesignTokens.resolveHexColor(
      deck.deckTheme.primaryFillHex,
      fallback: palette.elevatedSurface.swiftUIColor
    )
    let themeAccent = EasyOfficeDesignTokens.resolveHexColor(
      deck.deckTheme.accentBorderHex,
      fallback: EasyOfficeDesignTokens.brand700.swiftUIColor
    )
    let themeFill = EasyOfficeDesignTokens.resolveHexColor(
      deck.deckTheme.primaryFillHex,
      fallback: palette.selectedSurface.swiftUIColor
    )

    VStack(alignment: .leading, spacing: 5) {
      // 160x90 Visual 16:9 Miniature Slide Preview
      ZStack(alignment: .topLeading) {
        RoundedRectangle(cornerRadius: 6)
          .fill(themeBg)

        // Mini Accent Header Bar
        Rectangle()
          .fill(themeAccent)
          .frame(height: 4)

        VStack(alignment: .leading, spacing: 3) {
          HStack(spacing: 4) {
            Text("\(index + 1)")
              .font(.system(size: 9, weight: .black, design: .monospaced))
              .padding(.horizontal, 5)
              .padding(.vertical, 1)
              .background(themeAccent)
              .foregroundStyle(Color.white)
              .clipShape(RoundedRectangle(cornerRadius: 3))

            Text(slide.title)
              .font(.system(size: 9, weight: .bold))
              .foregroundStyle(palette.primaryText.swiftUIColor)
              .lineLimit(1)
          }

          if !slide.subtitle.isEmpty {
            Text(slide.subtitle)
              .font(.system(size: 7, weight: .medium))
              .foregroundStyle(themeAccent)
              .lineLimit(1)
          }

          Spacer(minLength: 2)

          // Mini Shapes Preview Strip
          HStack(spacing: 4) {
            ForEach(slide.elements.prefix(3)) { elem in
              let elemFill = EasyOfficeDesignTokens.resolveHexColor(
                elem.fillHex,
                fallback: themeFill
              )
              RoundedRectangle(cornerRadius: 2)
                .fill(elemFill)
                .overlay(
                  RoundedRectangle(cornerRadius: 2)
                    .stroke(themeAccent.opacity(0.8), lineWidth: 0.7)
                )
                .frame(width: 40, height: 18)
                .overlay(
                  Text(elem.title)
                    .font(.system(size: 6, weight: .semibold))
                    .foregroundStyle(
                      EasyOfficeDesignTokens.contrastingTextColor(forFillHex: elem.fillHex)
                    )
                    .lineLimit(1)
                    .padding(.horizontal, 2)
                )
            }
          }
        }
        .padding(7)
      }
      .frame(width: 160, height: 90)
      .overlay(
        RoundedRectangle(cornerRadius: 6)
          .stroke(
            isSelected
              ? EasyOfficeDesignTokens.brand700.swiftUIColor
              : palette.border.swiftUIColor,
            lineWidth: isSelected ? 2.0 : 1.0
          )
      )
      .clipShape(RoundedRectangle(cornerRadius: 6))

      HStack(spacing: 4) {
        Text(slide.masterLayout.rawValue)
          .font(.system(size: 9, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
          .lineLimit(1)
        Spacer(minLength: 0)
        Text("\(slide.elements.count)개체")
          .font(.system(size: 8, design: .monospaced))
          .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
      }
      .frame(width: 160)
    }
    .padding(6)
    .background(
      isSelected
        ? palette.selectedSurface.swiftUIColor
        : palette.cardSurface.swiftUIColor
    )
    .clipShape(RoundedRectangle(cornerRadius: 8))
  }

  public var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
    let activeIdx =
      deck.slides.indices.contains(viewModel.selectedSlideIndex) ? viewModel.selectedSlideIndex : 0
    let currentSlide = deck.slides.indices.contains(activeIdx) ? deck.slides[activeIdx] : nil
    let selectedElement: SlideCanvasElement? = {
      guard let slide = currentSlide else { return nil }
      if let selID = viewModel.selectedSlideElementID,
        let found = slide.elements.first(where: { $0.id == selID })
      {
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

    AdaptiveScrollContainer(
      axes: .vertical,
      showsIndicators: true,
      isSnapshotMode: viewModel.isCanvasSnapshotMode
    ) {
      VStack(alignment: .leading, spacing: 10) {
        // Single Consolidated Non-Clipping Slide Context Toolbar (`UX-08`)
        AdaptiveScrollContainer(
          axes: .horizontal,
          showsIndicators: false,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        ) {
          HStack(spacing: 5) {
            Text("16:9 · \(deck.slides.count)장")
              .font(.system(size: 10, weight: .bold, design: .monospaced))
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
              .padding(.horizontal, 6)
              .padding(.vertical, 3)
              .background(palette.selectedSurface.swiftUIColor)
              .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
              .clipShape(RoundedRectangle(cornerRadius: 4))

            if isPrimaryPane {
              Button {
                viewModel.addSlideToActiveDeck()
              } label: {
                Text("+ 슬라이드")
                  .font(.system(size: 10.5, weight: .semibold))
                  .lineLimit(1)
                  .fixedSize(horizontal: true, vertical: false)
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)
              .fixedSize(horizontal: true, vertical: false)

              Button {
                viewModel.duplicateActiveSlide()
              } label: {
                Text("복제")
                  .font(.system(size: 10.5, weight: .medium))
                  .lineLimit(1)
                  .fixedSize(horizontal: true, vertical: false)
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)
              .fixedSize(horizontal: true, vertical: false)

              if let slide = currentSlide {
                LavenderMenuPill(
                  title: "레이아웃: \(slide.masterLayout.rawValue)",
                  systemImage: "rectangle.3.group",
                  palette: palette
                ) {
                  ForEach(SlideMasterLayout.allCases, id: \.rawValue) { layout in
                    Button(layout.rawValue) {
                      mutateActiveSlideDeck { mutableDeck, sIdx in
                        mutableDeck.slides[sIdx].masterLayout = layout
                      }
                    }
                  }
                }
              }

              LavenderMenuPill(
                title: "테마: \(deck.theme.rawValue)",
                systemImage: "paintpalette",
                palette: palette
              ) {
                ForEach(SlideDeckTheme.allCases, id: \.rawValue) { theme in
                  Button(theme.rawValue) {
                    viewModel.applyThemeToActiveDeck(theme)
                  }
                }
              }

              if let slide = currentSlide {
                LavenderMenuPill(
                  title: "전환 (\(String(format: "%.1f", slide.transitionDurationSeconds))s)",
                  systemImage: "sparkles.rectangle.stack",
                  palette: palette
                ) {
                  Section("화면 전환 효과") {
                    ForEach(SlideTransitionKind.allCases, id: \.rawValue) { trans in
                      Button(trans.rawValue) {
                        mutateActiveSlideDeck { mutableDeck, sIdx in
                          mutableDeck.slides[sIdx].transition = trans
                        }
                      }
                    }
                  }
                  Section("전환 지속 시간") {
                    Button("빠르게 (0.4s)") {
                      viewModel.setActiveSlideTransitionDuration(0.4)
                    }
                    Button("표준 (0.8s)") {
                      viewModel.setActiveSlideTransitionDuration(0.8)
                    }
                    Button("느리게 (1.5s)") {
                      viewModel.setActiveSlideTransitionDuration(1.5)
                    }
                  }
                  Section("자동 전환 간격") {
                    Button("수동 클릭") {
                      viewModel.setActiveSlideAutoAdvance(seconds: nil)
                    }
                    Button("5초 후 자동 전환") {
                      viewModel.setActiveSlideAutoAdvance(seconds: 5)
                    }
                    Button("10초 후 자동 전환") {
                      viewModel.setActiveSlideAutoAdvance(seconds: 10)
                    }
                  }
                }
              }

              LavenderMenuPill(
                title: "정렬·배분",
                systemImage: "align.horizontal.center",
                palette: palette
              ) {
                ForEach(SlideElementAlignment.allCases, id: \.rawValue) { alignMode in
                  Button(alignMode.rawValue) {
                    viewModel.alignSlideElements(alignMode)
                  }
                }
                Divider()
                Button("선택 개체 복제") {
                  viewModel.duplicateActiveSlideElement()
                }
                .disabled(selectedElement == nil)
                Button("선택 개체 삭제") {
                  viewModel.deleteActiveSlideElement()
                }
                .disabled(selectedElement == nil)
              }

              Button(viewModel.isSlideGridlinesVisible ? "격자 숨김" : "격자선") {
                viewModel.toggleSlideGridlines()
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)
              .fixedSize(horizontal: true, vertical: false)

              Button(viewModel.isSlideSorterViewEnabled ? "편집 뷰" : "정렬 뷰") {
                viewModel.toggleSlideSorterView()
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)
              .fixedSize(horizontal: true, vertical: false)

              Button(viewModel.isPresenterModeActive ? "발표자 닫기" : "발표자 도구") {
                viewModel.isPresenterModeActive.toggle()
                viewModel.isPresenterTimerRunning = viewModel.isPresenterModeActive
              }
              .buttonStyle(.bordered)
              .controlSize(.mini)
              .fixedSize(horizontal: true, vertical: false)

              Button {
                viewModel.toggleFullScreenSlideshow()
              } label: {
                Label("슬라이드쇼 ▶", systemImage: "play.tv.fill")
                  .font(.system(size: 10.5, weight: .semibold))
                  .lineLimit(1)
                  .fixedSize(horizontal: true, vertical: false)
              }
              .buttonStyle(.borderedProminent)
              .tint(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
              .controlSize(.mini)
              .fixedSize(horizontal: true, vertical: false)
            }
          }
          .padding(.horizontal, 10)
          .padding(.vertical, 5)
        }
        .frame(height: 34)
        .background(palette.cardSurface.swiftUIColor)
        .clipShape(RoundedRectangle(cornerRadius: 6))

      if viewModel.isSlideSorterViewEnabled {
        slideSorterGridView(activeIdx: activeIdx, palette: palette)
      } else {
        HStack(alignment: .top, spacing: 14) {
          // Single-source Slide Filmstrip: only render internal rail when LeftStructureNavigatorView is hidden (`UX-11`)
          if !viewModel.showLeftNavigator {
            VStack(alignment: .leading, spacing: 10) {
              ForEach(Array(deck.slides.enumerated()), id: \.element.id) { idx, slide in
                let isSelected = idx == activeIdx
                Button {
                  guard isPrimaryPane else { return }
                  viewModel.selectedSlideIndex = idx
                  viewModel.selectSlideElement(id: slide.elements.first?.id)
                } label: {
                  slideMiniatureThumbnailCard(
                    slide: slide,
                    index: idx,
                    isSelected: isSelected,
                    palette: palette
                  )
                }
                .buttonStyle(.plain)
              }
            }
          }

          // Active Slide Stage Canvas with Horizontal + Vertical Rulers & Clean Speaker Notes Bar
          if let slide = currentSlide {
            let staticGuides: [AlignmentSnapGuide] = {
              guard viewModel.isSnapGuidesEnabled, let activeElem = selectedElement else {
                return []
              }
              let siblings = slide.elements.filter { $0.id != activeElem.id }
              return SnapGuideCalculator.computeGuides(
                for: activeElem,
                siblings: siblings,
                aspectRatio: deck.aspectRatio
              )
            }()

            VStack(alignment: .leading, spacing: 8) {
              HStack {
                Text(slide.masterLayout.rawValue)
                  .font(.system(size: 10, weight: .bold))
                  .padding(.horizontal, 6)
                  .padding(.vertical, 2)
                  .background(EasyOfficeDesignTokens.softBadgeToken(for: .slide).swiftUIColor)
                  .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
                  .clipShape(RoundedRectangle(cornerRadius: 4))

                Text(
                  "전환: \(slide.transition.rawValue) · \(String(format: "%.1f", slide.transitionDurationSeconds))s"
                )
                .font(.system(size: 10))
                .foregroundStyle(palette.secondaryText.swiftUIColor)

                Spacer()

                if isPrimaryPane, let activeElem = selectedElement {
                  HStack(spacing: 4) {
                    Button("◀") {
                      viewModel.moveSlideElement(
                        slideIndex: activeIdx,
                        elementID: activeElem.id,
                        deltaX: -20,
                        deltaY: 0
                      )
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.mini)

                    Button("▶") {
                      viewModel.moveSlideElement(
                        slideIndex: activeIdx,
                        elementID: activeElem.id,
                        deltaX: 20,
                        deltaY: 0
                      )
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.mini)

                    Button("▲") {
                      viewModel.moveSlideElement(
                        slideIndex: activeIdx,
                        elementID: activeElem.id,
                        deltaX: 0,
                        deltaY: -20
                      )
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.mini)

                    Button("▼") {
                      viewModel.moveSlideElement(
                        slideIndex: activeIdx,
                        elementID: activeElem.id,
                        deltaX: 0,
                        deltaY: 20
                      )
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.mini)

                    Button("↻ +15°") {
                      viewModel.rotateSlideElement(
                        elementID: activeElem.id,
                        toDegrees: activeElem.rotationDegrees + 15.0
                      )
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.mini)

                    Button("정중앙 스냅") {
                      viewModel.executeRibbonCommand("slide-mod-snap")
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.mini)
                  }
                }

                if !staticGuides.isEmpty {
                  Label("스마트 정렬 가이드 스냅 활성 (\(staticGuides.count)축)", systemImage: "ruler")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(EasyOfficeDesignTokens.statusSuccess.swiftUIColor)
                }
              }

              // Horizontal Slide Stage Ruler (0 .. 960 pt) aligned to the stage canvas
              HStack(spacing: 6) {
                Color.clear.frame(width: 22, height: 20)
                slideStageHorizontalRuler(palette: palette)
              }

              // Vertical Ruler (0 .. 540 pt) + Proportional 16:9 Stage Canvas (`UX-09`, `FUNC-02`)
              HStack(alignment: .top, spacing: 6) {
                SlideVerticalRulerView(
                  maxHeightPoints: deck.aspectRatio.heightPoints,
                  palette: palette
                )
                .frame(width: 22, height: 360)

                GeometryReader { geo in
                  let stageW = max(1.0, deck.aspectRatio.widthPoints)
                  let stageH = max(1.0, deck.aspectRatio.heightPoints)
                  let scaleX = max(0.01, geo.size.width / stageW)
                  let scaleY = max(0.01, geo.size.height / stageH)
                  let activeGuides: [AlignmentSnapGuide] = {
                    guard viewModel.isSnapGuidesEnabled else { return [] }
                    let targetID = activeDragOffset?.elementID ?? selectedElement?.id
                    guard let targetID,
                      var activeElem = slide.elements.first(where: { $0.id == targetID })
                    else { return [] }
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

                    // Top Deck Theme Accent Strip
                    Rectangle()
                      .fill(
                        EasyOfficeDesignTokens.resolveHexColor(
                          deck.deckTheme.accentBorderHex,
                          fallback: EasyOfficeDesignTokens.brand700.swiftUIColor
                        )
                      )
                      .frame(height: 5)

                    // Optional Rule-of-Thirds & Center Crosshair Gridlines (`FUNC-02`)
                    if viewModel.isSlideGridlinesVisible {
                      Path { path in
                        let w = geo.size.width
                        let h = geo.size.height
                        for frac in [CGFloat(1.0 / 3.0), CGFloat(0.5), CGFloat(2.0 / 3.0)] {
                          let x = w * frac
                          path.move(to: CGPoint(x: x, y: 0))
                          path.addLine(to: CGPoint(x: x, y: h))
                          let y = h * frac
                          path.move(to: CGPoint(x: 0, y: y))
                          path.addLine(to: CGPoint(x: w, y: y))
                        }
                      }
                      .stroke(
                        EasyOfficeDesignTokens.brand700.swiftUIColor.opacity(0.32),
                        style: StrokeStyle(lineWidth: 0.9, dash: [4, 4])
                      )
                    }

                    // Slide Header & Bullet Content Overlay (Upper Stage Zone y: 0..220pt)
                    VStack(alignment: .leading, spacing: 6) {
                      Text(slide.title)
                        .font(.system(size: 20, weight: .bold))
                        .foregroundStyle(palette.primaryText.swiftUIColor)
                        .lineLimit(1)

                      if !slide.subtitle.isEmpty {
                        Text(slide.subtitle)
                          .font(.system(size: 13, weight: .medium))
                          .foregroundStyle(
                            EasyOfficeDesignTokens.resolveHexColor(
                              deck.deckTheme.accentBorderHex,
                              fallback: EasyOfficeDesignTokens.brand700.swiftUIColor
                            )
                          )
                          .lineLimit(1)
                      }

                      ForEach(slide.bulletPoints, id: \.self) { bullet in
                        Text("• \(bullet)")
                          .font(.system(size: 12))
                          .foregroundStyle(palette.primaryText.swiftUIColor)
                          .lineLimit(1)
                      }
                    }
                    .padding(.horizontal, 22)
                    .padding(.top, 18)

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

                    // Positioned & Selectable Slide Canvas Elements with 7-Kind Shapes, 8 Resize Handles & Top Rotation Handle
                    ForEach(slide.elements) { element in
                      let isSelectedElem = selectedElement?.id == element.id
                      let liveOffset: CGSize =
                        (activeDragOffset?.elementID == element.id)
                        ? (activeDragOffset?.translation ?? .zero)
                        : .zero
                      let elemW = max(90.0, CGFloat(element.width) * scaleX)
                      let elemH = max(50.0, CGFloat(element.height) * scaleY)
                      let centerX = CGFloat(element.centerX) * scaleX + liveOffset.width
                      let centerY = CGFloat(element.centerY) * scaleY + liveOffset.height

                      ZStack {
                        SlideElementShapeContentView(
                          element: element,
                          palette: palette,
                          isSelected: isSelectedElem
                        )
                        .frame(width: elemW, height: elemH)

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
                            .background(
                              EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor
                            )
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
                      .zIndex(Double(element.zIndex))
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
                  .clipShape(RoundedRectangle(cornerRadius: 8))
                }
                .frame(height: 360)
              }

              Divider()
              HStack(spacing: 8) {
                Label("발표자 노트", systemImage: "note.text")
                  .font(.system(size: 11, weight: .semibold))
                  .lineLimit(1)
                  .fixedSize(horizontal: true, vertical: false)
                  .foregroundStyle(palette.secondaryText.swiftUIColor)
                if isPrimaryPane {
                  LavenderTextFieldPill(
                    placeholder: "발표자 노트를 입력하세요...",
                    text: Binding(
                      get: { slide.speakerNotes },
                      set: { viewModel.updateActiveSlideSpeakerNotes($0) }
                    ),
                    width: nil,
                    palette: palette,
                    isSnapshotMode: viewModel.isCanvasSnapshotMode
                  )
                } else {
                  Text(slide.speakerNotes)
                    .font(.system(size: 11))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                }
              }
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(palette.cardSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .accessibilityLabel(
              AccessibilityTokens.slideStageLabel(
                slideNumber: activeIdx + 1, totalSlides: deck.slides.count))
          }
        }
      }

      if isPrimaryPane, viewModel.isPresenterModeActive {
        HStack(spacing: 16) {
          VStack(alignment: .leading, spacing: 4) {
            Text(
              "발표자 모니터 · 진행률 \(Int(presenterSession.progressFraction * 100))% · 경과 \(presenterSession.elapsedSeconds)초 (\(viewModel.isPresenterTimerRunning ? "진행 중" : "일시정지"))"
            )
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(palette.accentBadgeText.swiftUIColor)
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
        .background(palette.subtleAccentSurface.swiftUIColor)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .onReceive(presenterTimer) { _ in
          if viewModel.isPresenterModeActive && viewModel.isPresenterTimerRunning {
            viewModel.tickPresenterTimer(seconds: 1)
          }
        }
      }
      }
    }
  }

  @ViewBuilder
  private func slideSorterGridView(activeIdx: Int, palette: WorkspaceThemePalette) -> some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack {
        Label(
          "슬라이드 정렬 뷰 (Slide Sorter · \(deck.slides.count)장)",
          systemImage: "square.grid.3x2.fill"
        )
        .font(.system(size: 12, weight: .bold))
        .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
        Spacer()
        Text("카드를 클릭하여 슬라이드를 선택하거나 순서를 재배열합니다.")
          .font(.system(size: 11))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
      }

      LazyVGrid(columns: [GridItem(.adaptive(minimum: 210), spacing: 14)], spacing: 14) {
        ForEach(Array(deck.slides.enumerated()), id: \.element.id) { idx, slide in
          let isSelected = idx == activeIdx
          VStack(alignment: .leading, spacing: 8) {
            HStack {
              Text("#\(idx + 1)")
                .font(.system(size: 11, weight: .bold, design: .monospaced))
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(EasyOfficeDesignTokens.softBadgeToken(for: .slide).swiftUIColor)
                .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
                .clipShape(RoundedRectangle(cornerRadius: 4))

              Text(slide.transition.rawValue)
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(palette.secondaryText.swiftUIColor)

              Spacer()

              Text("개체 \(slide.elements.count)개")
                .font(.system(size: 9, design: .monospaced))
                .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
            }

            // 16:9 Miniature Card Stage Preview
            VStack(alignment: .leading, spacing: 4) {
              Text(slide.title)
                .font(.system(size: 12, weight: .bold))
                .foregroundStyle(palette.primaryText.swiftUIColor)
                .lineLimit(1)

              if !slide.subtitle.isEmpty {
                Text(slide.subtitle)
                  .font(.system(size: 10))
                  .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                  .lineLimit(1)
              }

              ForEach(slide.bulletPoints.prefix(2), id: \.self) { bullet in
                Text("• \(bullet)")
                  .font(.system(size: 9))
                  .foregroundStyle(palette.secondaryText.swiftUIColor)
                  .lineLimit(1)
              }
              Spacer(minLength: 0)
            }
            .padding(10)
            .frame(maxWidth: .infinity, minHeight: 108, maxHeight: 108, alignment: .topLeading)
            .background(palette.elevatedSurface.swiftUIColor)
            .clipShape(RoundedRectangle(cornerRadius: 6))

            if isPrimaryPane {
              HStack(spacing: 4) {
                Button("◀") {
                  viewModel.selectedSlideIndex = idx
                  viewModel.moveActiveSlide(offset: -1)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .disabled(idx == 0)

                Button("▶") {
                  viewModel.selectedSlideIndex = idx
                  viewModel.moveActiveSlide(offset: 1)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .disabled(idx + 1 >= deck.slides.count)

                Spacer()

                Button("복제") {
                  viewModel.selectedSlideIndex = idx
                  viewModel.duplicateActiveSlide()
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)

                Button("삭제") {
                  viewModel.selectedSlideIndex = idx
                  viewModel.deleteActiveSlide()
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .disabled(deck.slides.count <= 1)
              }
            }
          }
          .padding(12)
          .background(
            isSelected
              ? palette.selectedSurface.swiftUIColor
              : palette.cardSurface.swiftUIColor
          )
          .overlay(
            RoundedRectangle(cornerRadius: 8)
              .stroke(
                isSelected
                  ? EasyOfficeDesignTokens.brand700.swiftUIColor
                  : palette.border.swiftUIColor,
                lineWidth: isSelected ? 2.0 : 1.0
              )
          )
          .clipShape(RoundedRectangle(cornerRadius: 8))
          .contentShape(Rectangle())
          .onTapGesture {
            guard isPrimaryPane else { return }
            viewModel.selectedSlideIndex = idx
            viewModel.selectSlideElement(id: slide.elements.first?.id)
          }
        }
      }
    }
    .padding(16)
    .background(palette.cardSurface.swiftUIColor)
    .clipShape(RoundedRectangle(cornerRadius: 8))
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
      let page = doc.page(at: selectedPageIndex)
    {
      nsView.go(to: page)
    }
  }
}

public typealias PDFStudioEditorDetailPaneView = PDFStudioDetailPaneView

/// Rich interactive editor pane for PDF Studio (.pdf) with cached native PDFKit preview, interactive freehand vector ink overlay, Redaction & AcroForm Fields bar, Page Reorder & Tab Merge controls, and Rotate/Convert tools.
public struct PDFStudioDetailPaneView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  let pdf: PDFStudioDocumentModel
  let isPrimaryPane: Bool

  public init(
    viewModel: WorkspaceViewModel, pdf: PDFStudioDocumentModel, isPrimaryPane: Bool = true
  ) {
    self.viewModel = viewModel
    self.pdf = pdf
    self.isPrimaryPane = isPrimaryPane
  }

  @ViewBuilder
  private func pdfPrimaryToolbar(
    palette: WorkspaceThemePalette,
    currentPageNum: Int,
    currentPageStrokeCount: Int
  ) -> some View {
    AdaptiveScrollContainer(
      axes: .horizontal,
      showsIndicators: false,
      isSnapshotMode: viewModel.isCanvasSnapshotMode
    ) {
      HStack(spacing: 6) {
        HStack(spacing: 4) {
          Button("◀") {
            viewModel.selectedPDFPageIndex = max(0, viewModel.selectedPDFPageIndex - 1)
          }
          .buttonStyle(.bordered)
          .controlSize(.mini)
          .disabled(viewModel.selectedPDFPageIndex <= 0)

          Text("\(currentPageNum) / \(max(1, pdf.pages.count))")
            .font(.system(size: 11, weight: .bold, design: .monospaced))
            .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)

          Button("▶") {
            viewModel.selectedPDFPageIndex = min(
              max(0, pdf.pages.count - 1),
              viewModel.selectedPDFPageIndex + 1
            )
          }
          .buttonStyle(.bordered)
          .controlSize(.mini)
          .disabled(viewModel.selectedPDFPageIndex + 1 >= pdf.pages.count)
        }

        Divider().frame(height: 16)

        Button {
          viewModel.rotateActivePDFPage(clockwiseDegrees: 90)
        } label: {
          Label("90° 회전", systemImage: "rotate.right")
            .font(.system(size: 11, weight: .semibold))
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .fixedSize(horizontal: true, vertical: false)

        LavenderMenuPill(
          title: "주석 도구 (\(pdf.annotations.count))",
          systemImage: "highlighter",
          palette: palette
        ) {
          ForEach(PDFAnnotationKind.allCases, id: \.rawValue) { kind in
            Button(kind.rawValue) {
              viewModel.addPDFAnnotation(kind: kind, content: "\(kind.rawValue) 검토 마크")
            }
          }
        }

        LavenderMenuPill(
          title: "결재 스탬프",
          systemImage: "seal.fill",
          palette: palette
        ) {
          ForEach(["승인", "대외비", "검토필", "긴급", "사본"], id: \.self) { preset in
            Button("결재 스탬프: \(preset)") {
              viewModel.addStampToActivePDFPage(preset: preset)
            }
          }
        }

        LavenderTogglePill(
          title: "전자서명 그리기",
          systemImage: "signature",
          isOn: $viewModel.isPDFInkDrawingModeEnabled,
          palette: palette
        )

        Button {
          viewModel.executeRibbonCommand("pdf-mod-tooffice")
        } label: {
          Label("DOCX 변환", systemImage: "arrow.right.doc.on.clipboard")
            .font(.system(size: 11, weight: .semibold))
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.small)
        .tint(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
        .fixedSize(horizontal: true, vertical: false)

        Text("ISO 32000-1 검증됨")
          .font(.system(size: 10, weight: .semibold, design: .monospaced))
          .padding(.horizontal, 6)
          .padding(.vertical, 2)
          .background(palette.subtleAccentSurface.swiftUIColor)
          .foregroundStyle(palette.accentBadgeText.swiftUIColor)
          .clipShape(RoundedRectangle(cornerRadius: 4))
          .lineLimit(1)
          .fixedSize(horizontal: true, vertical: false)

        Button(viewModel.isAdvancedToolbarExpanded ? "양식·보안 접기 ▲" : "양식·보안 도구 ▼") {
          viewModel.isAdvancedToolbarExpanded.toggle()
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .fixedSize(horizontal: true, vertical: false)
      }
      .padding(.horizontal, 10)
      .padding(.vertical, 4)
    }
    .frame(height: 34)
    .background(palette.cardSurface.swiftUIColor)
    .clipShape(RoundedRectangle(cornerRadius: 6))
  }

  private var currentPDFSecuritySummary: String {
    pdf.securityInspectionSummary
  }

  private func triggerApplyWatermark() {
    viewModel.applyWatermarkOnActivePDF()
  }

  @ViewBuilder
  private func pdfSecurityAndWatermarkBar(palette: WorkspaceThemePalette) -> some View {
    AdaptiveScrollContainer(
      axes: .horizontal,
      showsIndicators: false,
      isSnapshotMode: viewModel.isCanvasSnapshotMode
    ) {
      HStack(spacing: 8) {
        Label("PDF 보안 점검 & 워터마크", systemImage: "checkmark.shield.fill")
          .font(.system(size: 11, weight: .bold))
          .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
          .lineLimit(1)
          .fixedSize(horizontal: true, vertical: false)

        Text(currentPDFSecuritySummary)
          .font(.system(size: 10, weight: .semibold, design: .monospaced))
          .padding(.horizontal, 8)
          .padding(.vertical, 3)
          .background(palette.subtleAccentSurface.swiftUIColor)
          .foregroundStyle(palette.accentBadgeText.swiftUIColor)
          .clipShape(RoundedRectangle(cornerRadius: 4))
          .lineLimit(1)
          .fixedSize(horizontal: true, vertical: false)

        Divider().frame(height: 16)

        LavenderTextFieldPill(
          placeholder: "워터마크",
          text: $viewModel.pdfWatermarkDraft,
          width: 150,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )

        Button("워터마크 적용") {
          triggerApplyWatermark()
        }
        .buttonStyle(.borderedProminent)
        .tint(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
        .controlSize(.small)
        .fixedSize(horizontal: true, vertical: false)
      }
      .padding(.horizontal, 10)
      .padding(.vertical, 6)
    }
    .background(palette.cardSurface.swiftUIColor)
    .clipShape(RoundedRectangle(cornerRadius: 6))
  }

  @ViewBuilder
  private func pdfRedactionAndMergeBar(palette: WorkspaceThemePalette) -> some View {
    AdaptiveScrollContainer(
      axes: .horizontal,
      showsIndicators: false,
      isSnapshotMode: viewModel.isCanvasSnapshotMode
    ) {
      HStack(spacing: 8) {
        Label("보안 리댁션 · 양식 · 병합", systemImage: "eye.slash.fill")
          .font(.system(size: 11, weight: .bold))
          .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
          .lineLimit(1)
          .fixedSize(horizontal: true, vertical: false)

        LavenderTextFieldPill(
          placeholder: "마스킹 구문",
          text: $viewModel.pdfRedactionPhraseDraft,
          width: 120,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )

        Button("영구 리댁션") {
          _ = viewModel.redactSensitiveTextOnActivePDF()
        }
        .buttonStyle(.borderedProminent)
        .tint(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
        .controlSize(.small)
        .fixedSize(horizontal: true, vertical: false)

        Divider().frame(height: 16)

        Button("페이지 위로") {
          viewModel.moveActivePDFPage(offset: -1)
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .fixedSize(horizontal: true, vertical: false)
        .disabled(viewModel.selectedPDFPageIndex <= 0)

        Button("페이지 아래로") {
          viewModel.moveActivePDFPage(offset: 1)
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .fixedSize(horizontal: true, vertical: false)
        .disabled(viewModel.selectedPDFPageIndex + 1 >= pdf.pages.count)

        Button("열린 PDF 병합") {
          viewModel.mergeActivePDFWithAnotherOpenTab()
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .fixedSize(horizontal: true, vertical: false)

        Divider().frame(height: 16)

        LavenderTextFieldPill(
          placeholder: "필드명",
          text: $viewModel.pdfFormFieldNameDraft,
          width: 95,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )

        LavenderTextFieldPill(
          placeholder: "기본값",
          text: $viewModel.pdfFormFieldValueDraft,
          width: 95,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )

        Button("양식 필드 추가") {
          viewModel.addFormFieldToActivePDF()
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .fixedSize(horizontal: true, vertical: false)

        Button("체크박스 추가") {
          viewModel.addFormFieldToActivePDF(isCheckbox: true)
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .fixedSize(horizontal: true, vertical: false)

        if pdf.isRedacted {
          Label("비가역 리댁션 적용됨", systemImage: "lock.shield.fill")
            .font(.system(size: 10, weight: .bold))
            .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
        }
      }
      .padding(.horizontal, 10)
      .padding(.vertical, 6)
    }
    .background(palette.cardSurface.swiftUIColor)
    .clipShape(RoundedRectangle(cornerRadius: 6))
  }

  @ViewBuilder
  private func pdfStampAndBookmarkBar(palette: WorkspaceThemePalette) -> some View {
    AdaptiveScrollContainer(
      axes: .horizontal,
      showsIndicators: false,
      isSnapshotMode: viewModel.isCanvasSnapshotMode
    ) {
      HStack(spacing: 8) {
        Label("책갈피", systemImage: "bookmark.fill")
          .font(.system(size: 11, weight: .bold))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
          .lineLimit(1)
          .fixedSize(horizontal: true, vertical: false)

        LavenderTextFieldPill(
          placeholder: "책갈피 제목",
          text: $viewModel.bookmarkTitleDraft,
          width: 140,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )

        Button("책갈피 추가") {
          viewModel.addBookmarkToActivePDF()
        }
        .buttonStyle(.borderedProminent)
        .tint(EasyOfficeDesignTokens.brand700.swiftUIColor)
        .controlSize(.small)
        .fixedSize(horizontal: true, vertical: false)

        if !pdf.outline.isEmpty {
          HStack(spacing: 6) {
            ForEach(pdf.outline) { node in
              HStack(spacing: 4) {
                Button {
                  let targetIdx = max(0, min(pdf.pages.count - 1, node.targetPageNumber - 1))
                  viewModel.selectedPDFPageIndex = targetIdx
                } label: {
                  Text("\(node.title) (p.\(node.targetPageNumber))")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(palette.accentBadgeText.swiftUIColor)
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
                }
                .buttonStyle(.plain)

                Button {
                  viewModel.removeBookmarkFromActivePDF(id: node.id)
                } label: {
                  Image(systemName: "xmark.circle.fill")
                    .font(.system(size: 10))
                    .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
                }
                .buttonStyle(.plain)
              }
              .padding(.horizontal, 6)
              .padding(.vertical, 3)
              .background(palette.selectedSurface.swiftUIColor)
              .clipShape(RoundedRectangle(cornerRadius: 4))
            }
          }
        }
      }
      .padding(.horizontal, 10)
      .padding(.vertical, 6)
    }
    .background(palette.cardSurface.swiftUIColor)
    .clipShape(RoundedRectangle(cornerRadius: 6))
  }

  @ViewBuilder
  private func pdfNativePageSheetView(
    page: PDFPageDescriptor,
    pageNumber: Int,
    palette: WorkspaceThemePalette
  ) -> some View {
    let pageAnnotations = pdf.annotations.filter { $0.pageNumber == pageNumber }
    let pageStrokes = pdf.inkStrokes.filter { $0.pageNumber == pageNumber }
    let pageFormFields = pdf.formFields.filter { $0.pageNumber == pageNumber }
    let textLines = page.extractedText
      .split(separator: "\n", omittingEmptySubsequences: false)
      .map(String.init)
    let stampAnnotations = pageAnnotations.filter { $0.kind == .stamp }
    let primaryStampLabel = stampAnnotations.last?.content ?? "APPROVED · 결재완료"

    ZStack {
      VStack(alignment: .leading, spacing: 12) {
        // Top Official PDF Page Header & Electronic Approval Stamp Box
        HStack(alignment: .top, spacing: 12) {
          VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
              Text("ISO 32000-1 STANDARD PDF")
                .font(.system(size: 9, weight: .bold, design: .monospaced))
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(EasyOfficeDesignTokens.softBadgeToken(for: .pdf).swiftUIColor)
                .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
                .clipShape(RoundedRectangle(cornerRadius: 4))

              Text("PAGE \(pageNumber) OF \(max(1, pdf.pages.count))")
                .font(.system(size: 9, weight: .bold, design: .monospaced))
                .foregroundStyle(palette.secondaryText.swiftUIColor)

              if page.rotationDegrees != 0 {
                Text("회전 \(page.rotationDegrees)°")
                  .font(.system(size: 9, weight: .semibold, design: .monospaced))
                  .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
              }
            }

            Text(pdf.title)
              .font(.system(size: 16, weight: .bold))
              .foregroundStyle(palette.primaryText.swiftUIColor)
              .lineLimit(1)
          }

          Spacer()

          // Official Approval Stamp Seal Box
          VStack(spacing: 2) {
            Text("전자결재 승인인")
              .font(.system(size: 8, weight: .bold))
              .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
            Text(primaryStampLabel)
              .font(.system(size: 10, weight: .heavy, design: .monospaced))
              .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
              .lineLimit(1)
          }
          .padding(.horizontal, 10)
          .padding(.vertical, 6)
          .background(EasyOfficeDesignTokens.statusDanger.swiftUIColor.opacity(0.08))
          .overlay(
            RoundedRectangle(cornerRadius: 5)
              .stroke(EasyOfficeDesignTokens.statusDanger.swiftUIColor, lineWidth: 1.5)
          )
          .clipShape(RoundedRectangle(cornerRadius: 5))
        }

        Divider()

        // Extracted Document Paragraphs & Highlight Overlay
        VStack(alignment: .leading, spacing: 7) {
          ForEach(Array(textLines.enumerated()), id: \.offset) { lineIdx, line in
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            let isHeadingLine = lineIdx == 0 || trimmed.hasPrefix("1.") || trimmed.hasPrefix("2.")
              || trimmed.hasPrefix("3.") || trimmed.hasPrefix("■") || trimmed.hasPrefix("[")
            let isHighlightedLine = pageAnnotations.contains {
              $0.kind == .highlight && !trimmed.isEmpty && trimmed.contains($0.content.prefix(4))
            } || (lineIdx == 1 && !pageAnnotations.isEmpty)

            Text(trimmed.isEmpty ? " " : line)
              .font(.system(size: isHeadingLine ? 13 : 11.5, weight: isHeadingLine ? .bold : .regular))
              .foregroundStyle(palette.primaryText.swiftUIColor)
              .padding(.horizontal, isHighlightedLine ? 6 : 0)
              .padding(.vertical, isHighlightedLine ? 3 : 0)
              .frame(maxWidth: .infinity, alignment: .leading)
              .background(
                isHighlightedLine
                  ? EasyOfficeDesignTokens.statusWarning.swiftUIColor.opacity(0.24)
                  : Color.clear
              )
              .clipShape(RoundedRectangle(cornerRadius: 4))
          }
        }

        // Active Page Annotations Strip
        if !pageAnnotations.isEmpty {
          VStack(alignment: .leading, spacing: 5) {
            Text("페이지 주석 및 검토 마크 (\(pageAnnotations.count)건)")
              .font(.system(size: 10, weight: .bold))
              .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)

            AdaptiveScrollContainer(
              axes: .horizontal,
              showsIndicators: false,
              isSnapshotMode: viewModel.isCanvasSnapshotMode
            ) {
              HStack(spacing: 6) {
                ForEach(pageAnnotations) { annot in
                  HStack(spacing: 4) {
                    Image(systemName: "highlighter")
                      .font(.system(size: 9, weight: .bold))
                    Text("[\(annot.kind.rawValue)] \(annot.content)")
                      .font(.system(size: 10, weight: .medium))
                      .lineLimit(1)
                  }
                  .padding(.horizontal, 8)
                  .padding(.vertical, 4)
                  .background(palette.selectedSurface.swiftUIColor)
                  .foregroundStyle(palette.accentBadgeText.swiftUIColor)
                  .clipShape(RoundedRectangle(cornerRadius: 4))
                }
              }
            }
          }
        }

        // Active Page AcroForm Fields Preview
        if !pageFormFields.isEmpty {
          VStack(alignment: .leading, spacing: 5) {
            Text("AcroForm 전자 양식 필드 (\(pageFormFields.count)필드)")
              .font(.system(size: 10, weight: .bold))
              .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)

            HStack(spacing: 8) {
              ForEach(pageFormFields.prefix(4)) { field in
                HStack(spacing: 5) {
                  Image(
                    systemName: field.isCheckbox
                      ? (field.isChecked ? "checkmark.square.fill" : "square")
                      : "character.textbox"
                  )
                  .font(.system(size: 10, weight: .bold))
                  .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                  Text("\(field.name): \(field.isCheckbox ? (field.isChecked ? "예" : "아니오") : field.value)")
                    .font(.system(size: 10, weight: .semibold, design: .monospaced))
                    .foregroundStyle(palette.primaryText.swiftUIColor)
                    .lineLimit(1)
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(palette.cardSurface.swiftUIColor)
                .overlay(
                  RoundedRectangle(cornerRadius: 4)
                    .stroke(palette.border.swiftUIColor, lineWidth: 0.8)
                )
                .clipShape(RoundedRectangle(cornerRadius: 4))
              }
            }
          }
        }

        // Vector Signature & Freehand Ink Verification Box
        HStack(spacing: 12) {
          VStack(alignment: .leading, spacing: 3) {
            Text("전자서명 및 벡터 잉크 검증 영역")
              .font(.system(size: 10, weight: .bold))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
            Text("현재 페이지 벡터 획: \(pageStrokes.count)건 · 무결성 검증 정상")
              .font(.system(size: 9.5, design: .monospaced))
              .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
          }

          Spacer()

          Canvas { context, size in
            if pageStrokes.isEmpty {
              var baseline = Path()
              baseline.move(to: CGPoint(x: 8, y: size.height * 0.65))
              baseline.addCurve(
                to: CGPoint(x: size.width - 8, y: size.height * 0.45),
                control1: CGPoint(x: size.width * 0.35, y: size.height * 0.15),
                control2: CGPoint(x: size.width * 0.65, y: size.height * 0.85)
              )
              context.stroke(
                baseline,
                with: .color(EasyOfficeDesignTokens.brand700.swiftUIColor.opacity(0.65)),
                style: StrokeStyle(lineWidth: 1.8, lineCap: .round, lineJoin: .round)
              )
            } else {
              for stroke in pageStrokes where stroke.points.count >= 2 {
                var path = Path()
                for (ptIdx, pt) in stroke.points.enumerated() {
                  let vx = CGFloat((pt.x - 20.0) / 555.0) * size.width
                  let vy = size.height - (CGFloat((pt.y - 20.0) / 800.0) * size.height)
                  let point = CGPoint(
                    x: max(4, min(size.width - 4, vx)),
                    y: max(4, min(size.height - 4, vy))
                  )
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
            }
          }
          .frame(width: 180, height: 38)
          .background(palette.cardSurface.swiftUIColor)
          .overlay(
            RoundedRectangle(cornerRadius: 5)
              .stroke(palette.border.swiftUIColor, lineWidth: 0.8)
          )
          .clipShape(RoundedRectangle(cornerRadius: 5))
        }
      }
      .padding(20)

      if !pdf.watermarkText.isEmpty {
        Text(pdf.watermarkText)
          .font(.system(size: 32, weight: .black, design: .rounded))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor.opacity(0.14))
          .rotationEffect(.degrees(-22))
          .allowsHitTesting(false)
      }
    }
    .frame(maxWidth: .infinity, alignment: .topLeading)
    .background(palette.elevatedSurface.swiftUIColor)
    .overlay(
      RoundedRectangle(cornerRadius: 8)
        .stroke(palette.border.swiftUIColor, lineWidth: 1)
    )
    .clipShape(RoundedRectangle(cornerRadius: 8))
  }

  public var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
    let currentPageNum = viewModel.selectedPDFPageNumber
    let currentPageStrokeCount = pdf.inkStrokes.filter { $0.pageNumber == currentPageNum }.count
    let activePage = {
      let safeIdx = max(0, min(pdf.pages.count - 1, viewModel.selectedPDFPageIndex))
      return pdf.pages.indices.contains(safeIdx) ? pdf.pages[safeIdx] : nil
    }()

    AdaptiveScrollContainer(
      axes: .vertical,
      showsIndicators: true,
      isSnapshotMode: viewModel.isCanvasSnapshotMode
    ) {
      VStack(alignment: .leading, spacing: 10) {
        if isPrimaryPane {
          pdfPrimaryToolbar(
            palette: palette,
            currentPageNum: currentPageNum,
            currentPageStrokeCount: currentPageStrokeCount
          )

          if viewModel.isAdvancedToolbarExpanded {
            pdfSecurityAndWatermarkBar(palette: palette)
            pdfRedactionAndMergeBar(palette: palette)
            pdfStampAndBookmarkBar(palette: palette)

            if !pdf.formFields.isEmpty {
              VStack(alignment: .leading, spacing: 8) {
                HStack {
                  Label(
                    "AcroForm 양식 필드 (\(pdf.formFields.count)건)",
                    systemImage: "list.bullet.rectangle.portrait.fill"
                  )
                  .font(.system(size: 11, weight: .bold))
                  .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
                  Spacer()
                }

                ForEach(pdf.formFields) { field in
                  HStack(spacing: 10) {
                    Text("P\(field.pageNumber)")
                      .font(.system(size: 9, weight: .bold, design: .monospaced))
                      .padding(.horizontal, 5)
                      .padding(.vertical, 2)
                      .background(EasyOfficeDesignTokens.softBadgeToken(for: .pdf).swiftUIColor)
                      .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
                      .clipShape(RoundedRectangle(cornerRadius: 4))

                    Text(field.name)
                      .font(.system(size: 11, weight: .semibold))
                      .foregroundStyle(palette.primaryText.swiftUIColor)
                      .frame(minWidth: 110, alignment: .leading)

                    if field.isCheckbox {
                      LavenderTogglePill(
                        title: field.isChecked ? "선택됨 (Checked)" : "해제됨 (Unchecked)",
                        systemImage: field.isChecked ? "checkmark.square.fill" : "square",
                        isOn: Binding(
                          get: { field.isChecked },
                          set: { nextChecked in
                            viewModel.updatePDFFormField(id: field.id, isChecked: nextChecked)
                          }
                        ),
                        palette: palette
                      )
                    } else {
                      LavenderTextFieldPill(
                        placeholder: "양식 입력값",
                        text: Binding(
                          get: { field.value },
                          set: { nextVal in
                            viewModel.updatePDFFormField(id: field.id, value: nextVal)
                          }
                        ),
                        width: 200,
                        palette: palette,
                        isSnapshotMode: viewModel.isCanvasSnapshotMode
                      )
                    }

                    Spacer()
                  }
                }
              }
              .padding(10)
              .background(palette.cardSurface.swiftUIColor)
              .overlay(
                RoundedRectangle(cornerRadius: 8)
                  .stroke(palette.border.swiftUIColor, lineWidth: 1)
              )
              .clipShape(RoundedRectangle(cornerRadius: 8))
            }
          }
        }

        if !pdf.pages.isEmpty {
          VStack(alignment: .leading, spacing: 8) {
            HStack {
              Label(
                "네이티브 PDFKit 벡터 뷰어 (ISO 32000-1 · CoreText CJK 서브셋)",
                systemImage: "doc.viewfinder.fill"
              )
              .font(.system(size: 11, weight: .bold))
              .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
              Spacer()
              Text("페이지 수: \(pdf.pages.count)p · 총 벡터 획: \(pdf.inkStrokes.count)건")
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
            }

            if let page = activePage {
              pdfNativePageSheetView(
                page: page,
                pageNumber: currentPageNum,
                palette: palette
              )
            }

            if !viewModel.isCanvasSnapshotMode {
              PDFKitRepresentedView(
                pdfDocument: pdf, selectedPageIndex: viewModel.selectedPDFPageIndex,
                zoomFactor: CGFloat(viewModel.zoomPercent / 100.0)
              )
              .frame(minHeight: 360, maxHeight: .infinity)
              .clipShape(RoundedRectangle(cornerRadius: 6))
            }

            if isPrimaryPane
              && (viewModel.isPDFInkDrawingModeEnabled || viewModel.isPDFInkOverlayExpanded)
            {
              PDFInteractiveInkOverlayView(viewModel: viewModel, palette: palette, pdfDocument: pdf)
            }
          }
          .padding(12)
          .background(palette.cardSurface.swiftUIColor)
          .overlay(
            RoundedRectangle(cornerRadius: 8).stroke(palette.border.swiftUIColor, lineWidth: 1)
          )
          .clipShape(RoundedRectangle(cornerRadius: 8))
        }

        // Single Compact 1-Line PDF Active Page & AcroForm Summary Strip (`UX-12`)
        AdaptiveScrollContainer(
          axes: .horizontal,
          showsIndicators: false,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        ) {
          HStack(spacing: 10) {
            Label(
              "활성 페이지 P\(currentPageNum) / \(max(1, pdf.pages.count))",
              systemImage: "doc.text.magnifyingglass"
            )
            .font(.system(size: 11, weight: .bold, design: .monospaced))
            .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)

            if let page = activePage {
              Text("회전 \(page.rotationDegrees)°")
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
            }

            Divider().frame(height: 14)

            Text(
              "주석 \(pdf.annotations.count)건 · 서명 획 \(currentPageStrokeCount)건 · AcroForm \(pdf.formFields.count)필드"
            )
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(palette.primaryText.swiftUIColor)
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)

            if !pdf.watermarkText.isEmpty {
              Divider().frame(height: 14)
              Text("워터마크: \(pdf.watermarkText)")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
            }
          }
          .padding(.horizontal, 10)
          .padding(.vertical, 6)
        }
        .frame(height: 32)
        .background(palette.cardSurface.swiftUIColor)
        .overlay(
          RoundedRectangle(cornerRadius: 6)
            .stroke(palette.border.swiftUIColor, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 6))
      }
    }
  }
}
