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
        Rectangle()
          .fill(palette.elevatedSurface.swiftUIColor)

        Canvas { context, size in
          let tickColor = palette.secondaryText.swiftUIColor
          let stepCount = 16
          for step in 0...stepCount {
            let ratio = CGFloat(step) / CGFloat(stepCount)
            let yPos = ratio * size.height
            let isMajor = (step % 4 == 0)
            let tickW: CGFloat = isMajor ? 4.0 : 2.2
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
          let yPos = min(height - 9.0, max(9.0, ratio * height))
          Text("\(ptMark)")
            .font(.system(size: 6.5, weight: .semibold, design: .monospaced))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
            .padding(.horizontal, 1.5)
            .background(palette.elevatedSurface.swiftUIColor)
            .position(x: 9, y: yPos)
        }
      }
      .border(palette.border.swiftUIColor, width: 0.8)
    }
    .frame(width: 24)
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
        Rectangle()
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
      .border(palette.border.swiftUIColor, width: 0.8)
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

    VStack(alignment: .leading, spacing: 5) {
      ZStack(alignment: .topLeading) {
        Rectangle()
          .fill(themeBg)

        Rectangle()
          .fill(themeAccent)
          .frame(height: 3)

        VStack(alignment: .leading, spacing: 3) {
          HStack(spacing: 4) {
            Text("\(index + 1)")
              .font(.system(size: 9, weight: .bold, design: .monospaced))
              .foregroundStyle(themeAccent)

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
        }
        .padding(7)
      }
      .frame(width: 160, height: 90)
      .border(
        isSelected
          ? EasyOfficeDesignTokens.brand700.swiftUIColor
          : palette.border.swiftUIColor,
        width: isSelected ? 2.0 : 1.0
      )

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
      VStack(alignment: .leading, spacing: 0) {
        AdaptiveScrollContainer(
          axes: .horizontal,
          showsIndicators: false,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        ) {
          HStack(spacing: 12) {
            Text("16:9 · \(deck.slides.count)장")
              .font(.system(size: 11, weight: .semibold, design: .monospaced))
              .lineLimit(1)
              .fixedSize(horizontal: true, vertical: false)
              .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)

            if isPrimaryPane {
              Divider().frame(height: 14)

              Button {
                viewModel.addSlideToActiveDeck()
              } label: {
                Text("+ 슬라이드")
                  .font(.system(size: 11, weight: .semibold))
                  .lineLimit(1)
                  .fixedSize(horizontal: true, vertical: false)
                  .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
              }
              .buttonStyle(.plain)
              .fixedSize(horizontal: true, vertical: false)

              Button {
                viewModel.duplicateActiveSlide()
              } label: {
                Text("복제")
                  .font(.system(size: 11, weight: .medium))
                  .lineLimit(1)
                  .fixedSize(horizontal: true, vertical: false)
                  .foregroundStyle(palette.primaryText.swiftUIColor)
              }
              .buttonStyle(.plain)
              .fixedSize(horizontal: true, vertical: false)

              Divider().frame(height: 14)

              if let slide = currentSlide {
                FlatToolbarMenu(
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

              FlatToolbarMenu(
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
                FlatToolbarMenu(
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

              FlatToolbarMenu(
                title: "정렬",
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

              Divider().frame(height: 14)

              Button(viewModel.isSlideGridlinesVisible ? "격자 숨김" : "격자선") {
                viewModel.toggleSlideGridlines()
              }
              .buttonStyle(.plain)
              .font(.system(size: 11, weight: .medium))
              .foregroundStyle(palette.primaryText.swiftUIColor)
              .fixedSize(horizontal: true, vertical: false)

              Button(viewModel.isSlideSorterViewEnabled ? "편집 뷰" : "정렬 뷰") {
                viewModel.toggleSlideSorterView()
              }
              .buttonStyle(.plain)
              .font(.system(size: 11, weight: .medium))
              .foregroundStyle(palette.primaryText.swiftUIColor)
              .fixedSize(horizontal: true, vertical: false)

              Button(viewModel.isPresenterModeActive ? "발표자 닫기" : "발표자") {
                viewModel.isPresenterModeActive.toggle()
                viewModel.isPresenterTimerRunning = viewModel.isPresenterModeActive
              }
              .buttonStyle(.plain)
              .font(.system(size: 11, weight: .medium))
              .foregroundStyle(palette.primaryText.swiftUIColor)
              .fixedSize(horizontal: true, vertical: false)

              Divider().frame(height: 14)

              Button {
                viewModel.toggleFullScreenSlideshow()
              } label: {
                Text("쇼 시작 ▶")
                  .font(.system(size: 11, weight: .semibold))
                  .lineLimit(1)
                  .fixedSize(horizontal: true, vertical: false)
                  .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
              }
              .buttonStyle(.plain)
              .fixedSize(horizontal: true, vertical: false)
            }
          }
          .padding(.horizontal, 12)
          .padding(.vertical, 6)
        }
        .frame(height: 32)
        .background(palette.elevatedSurface.swiftUIColor)

        Divider()

        if viewModel.isSlideSorterViewEnabled {
          slideSorterGridView(activeIdx: activeIdx, palette: palette)
        } else {
          HStack(alignment: .top, spacing: 14) {
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
              .padding(.leading, 12)
              .padding(.top, 12)
            }

            if let slide = currentSlide {
              VStack(alignment: .leading, spacing: 0) {
                VStack(alignment: .leading, spacing: 8) {
                  HStack(spacing: 6) {
                    Color.clear.frame(width: 22, height: 20)
                    slideStageHorizontalRuler(palette: palette)
                  }

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
                        Rectangle()
                          .fill(palette.elevatedSurface.swiftUIColor)

                        Rectangle()
                          .fill(
                            EasyOfficeDesignTokens.resolveHexColor(
                              deck.deckTheme.accentBorderHex,
                              fallback: EasyOfficeDesignTokens.brand700.swiftUIColor
                            )
                          )
                          .frame(height: 4)

                        if viewModel.isSlideGridlinesVisible {
                          ForEach([CGFloat(1.0 / 3.0), CGFloat(0.5), CGFloat(2.0 / 3.0)], id: \.self) { frac in
                            Rectangle()
                              .fill(EasyOfficeDesignTokens.brand700.swiftUIColor.opacity(0.24))
                              .frame(width: 1, height: geo.size.height)
                              .position(x: geo.size.width * frac, y: geo.size.height * 0.5)
                            Rectangle()
                              .fill(EasyOfficeDesignTokens.brand700.swiftUIColor.opacity(0.24))
                              .frame(width: geo.size.width, height: 1)
                              .position(x: geo.size.width * 0.5, y: geo.size.height * frac)
                          }
                        }

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

                        ForEach(Array(activeGuides.enumerated()), id: \.offset) { _, guide in
                          switch guide.axis {
                          case .vertical:
                            Rectangle()
                              .fill(EasyOfficeDesignTokens.statusSuccess.swiftUIColor)
                              .frame(width: 1.5, height: geo.size.height)
                              .position(
                                x: CGFloat(guide.positionPoints) * scaleX,
                                y: geo.size.height * 0.5
                              )
                          case .horizontal:
                            Rectangle()
                              .fill(EasyOfficeDesignTokens.statusSuccess.swiftUIColor)
                              .frame(width: geo.size.width, height: 1.5)
                              .position(
                                x: geo.size.width * 0.5,
                                y: CGFloat(guide.positionPoints) * scaleY
                              )
                          }
                        }

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
                              ForEach(SlideResizeHandle.allCases, id: \.rawValue) { handle in
                                let offset = handleOffset(for: handle, width: elemW, height: elemH)
                                Circle()
                                  .fill(EasyOfficeDesignTokens.brand700.swiftUIColor)
                                  .frame(width: 8, height: 8)
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

                              Text("↻")
                                .font(.system(size: 10, weight: .bold))
                                .foregroundStyle(Color.white)
                                .frame(width: 16, height: 16)
                                .background(
                                  EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor
                                )
                                .clipShape(Circle())
                                .offset(x: 0, y: -(elemH / 2.0) - 14.0)
                                .help("회전 핸들 (↻)")
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
                      .border(palette.border.swiftUIColor, width: 1)
                      .shadow(color: Color.black.opacity(0.08), radius: 6, x: 0, y: 2)
                    }
                    .frame(height: 360)
                  }
                }
                .padding(16)

                Divider()

                HStack(spacing: 8) {
                  Label("발표자 노트", systemImage: "note.text")
                    .font(.system(size: 11, weight: .semibold))
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                  if isPrimaryPane {
                    FlatUnderlineTextField(
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
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
                .background(palette.elevatedSurface.swiftUIColor)
              }
              .frame(maxWidth: .infinity, alignment: .leading)
              .background(palette.background.swiftUIColor)
              .accessibilityLabel(
                AccessibilityTokens.slideStageLabel(
                  slideNumber: activeIdx + 1, totalSlides: deck.slides.count))
            }
          }
        }

        if isPrimaryPane, viewModel.isPresenterModeActive {
          Divider()
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
            HStack(spacing: 10) {
              Button(viewModel.isPresenterTimerRunning ? "타이머 일시정지" : "타이머 시작") {
                viewModel.isPresenterTimerRunning.toggle()
              }
              .buttonStyle(.plain)
              .font(.system(size: 11, weight: .medium))

              Button("+10초") {
                viewModel.tickPresenterTimer(seconds: 10)
              }
              .buttonStyle(.plain)
              .font(.system(size: 11, weight: .medium))

              Button("이전 슬라이드") {
                viewModel.previousPresenterSlide()
              }
              .buttonStyle(.plain)
              .font(.system(size: 11, weight: .medium))
              .disabled(activeIdx == 0)

              Button("다음 슬라이드") {
                viewModel.advancePresenterSlide()
              }
              .buttonStyle(.plain)
              .font(.system(size: 11, weight: .semibold))
              .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
              .disabled(activeIdx + 1 >= deck.slides.count)
            }
          }
          .padding(12)
          .background(palette.subtleAccentSurface.swiftUIColor)
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
          "슬라이드 정렬 뷰 (\(deck.slides.count)장)",
          systemImage: "square.grid.3x2.fill"
        )
        .font(.system(size: 12, weight: .bold))
        .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
        Spacer()
        Text("슬라이드를 선택하거나 순서를 재배열합니다.")
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
                .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)

              Text(slide.transition.rawValue)
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(palette.secondaryText.swiftUIColor)

              Spacer()

              Text("개체 \(slide.elements.count)개")
                .font(.system(size: 9, design: .monospaced))
                .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
            }

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
            .border(palette.border.swiftUIColor, width: 0.8)

            if isPrimaryPane {
              HStack(spacing: 8) {
                Button("◀") {
                  viewModel.selectedSlideIndex = idx
                  viewModel.moveActiveSlide(offset: -1)
                }
                .buttonStyle(.plain)
                .font(.system(size: 10, weight: .semibold))
                .disabled(idx == 0)

                Button("▶") {
                  viewModel.selectedSlideIndex = idx
                  viewModel.moveActiveSlide(offset: 1)
                }
                .buttonStyle(.plain)
                .font(.system(size: 10, weight: .semibold))
                .disabled(idx + 1 >= deck.slides.count)

                Spacer()

                Button("복제") {
                  viewModel.selectedSlideIndex = idx
                  viewModel.duplicateActiveSlide()
                }
                .buttonStyle(.plain)
                .font(.system(size: 10, weight: .medium))

                Button("삭제") {
                  viewModel.selectedSlideIndex = idx
                  viewModel.deleteActiveSlide()
                }
                .buttonStyle(.plain)
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
                .disabled(deck.slides.count <= 1)
              }
            }
          }
          .padding(12)
          .background(palette.cardSurface.swiftUIColor)
          .border(
            isSelected
              ? EasyOfficeDesignTokens.brand700.swiftUIColor
              : palette.border.swiftUIColor,
            width: isSelected ? 2.0 : 1.0
          )
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
    .background(palette.background.swiftUIColor)
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
  @State private var showRawPDFKitView: Bool = false

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
      HStack(spacing: 12) {
        HStack(spacing: 6) {
          Button("◀") {
            viewModel.selectedPDFPageIndex = max(0, viewModel.selectedPDFPageIndex - 1)
          }
          .buttonStyle(.plain)
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
          .disabled(viewModel.selectedPDFPageIndex <= 0)

          Text("\(currentPageNum) / \(max(1, pdf.pages.count))")
            .font(.system(size: 11, weight: .semibold, design: .monospaced))
            .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)

          Button("▶") {
            viewModel.selectedPDFPageIndex = min(
              max(0, pdf.pages.count - 1),
              viewModel.selectedPDFPageIndex + 1
            )
          }
          .buttonStyle(.plain)
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
          .disabled(viewModel.selectedPDFPageIndex + 1 >= pdf.pages.count)
        }

        Divider().frame(height: 14)

        Button {
          viewModel.rotateActivePDFPage(clockwiseDegrees: 90)
        } label: {
          Label("90° 회전", systemImage: "rotate.right")
            .font(.system(size: 11, weight: .medium))
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
            .foregroundStyle(palette.primaryText.swiftUIColor)
        }
        .buttonStyle(.plain)
        .fixedSize(horizontal: true, vertical: false)

        FlatToolbarMenu(
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

        FlatToolbarMenu(
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

        FlatToggleControl(
          title: "전자서명 그리기",
          systemImage: "signature",
          isOn: $viewModel.isPDFInkDrawingModeEnabled,
          palette: palette
        )

        Divider().frame(height: 14)

        Button {
          viewModel.executeRibbonCommand("pdf-mod-tooffice")
        } label: {
          Label("DOCX 변환", systemImage: "arrow.right.doc.on.clipboard")
            .font(.system(size: 11, weight: .semibold))
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
            .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
        }
        .buttonStyle(.plain)
        .fixedSize(horizontal: true, vertical: false)

        if !viewModel.isCanvasSnapshotMode {
          Button(showRawPDFKitView ? "용지 뷰" : "PDFKit 뷰") {
            showRawPDFKitView.toggle()
          }
          .buttonStyle(.plain)
          .font(.system(size: 11, weight: .medium))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
          .fixedSize(horizontal: true, vertical: false)
        }

        Button(viewModel.isAdvancedToolbarExpanded ? "양식·보안 접기 ▲" : "양식·보안 도구 ▼") {
          viewModel.isAdvancedToolbarExpanded.toggle()
        }
        .buttonStyle(.plain)
        .font(.system(size: 11, weight: .medium))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
        .fixedSize(horizontal: true, vertical: false)
      }
      .padding(.horizontal, 12)
      .padding(.vertical, 6)
    }
    .frame(height: 32)
    .background(palette.elevatedSurface.swiftUIColor)
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
      HStack(spacing: 12) {
        Label("워터마크", systemImage: "checkmark.shield.fill")
          .font(.system(size: 11, weight: .semibold))
          .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
          .lineLimit(1)
          .fixedSize(horizontal: true, vertical: false)

        FlatUnderlineTextField(
          placeholder: "워터마크 문구",
          text: $viewModel.pdfWatermarkDraft,
          width: 150,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )

        Button("적용") {
          triggerApplyWatermark()
        }
        .buttonStyle(.plain)
        .font(.system(size: 11, weight: .semibold))
        .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
        .fixedSize(horizontal: true, vertical: false)
      }
      .padding(.horizontal, 12)
      .padding(.vertical, 5)
    }
    .background(palette.elevatedSurface.swiftUIColor)
  }

  @ViewBuilder
  private func pdfRedactionAndMergeBar(palette: WorkspaceThemePalette) -> some View {
    AdaptiveScrollContainer(
      axes: .horizontal,
      showsIndicators: false,
      isSnapshotMode: viewModel.isCanvasSnapshotMode
    ) {
      HStack(spacing: 12) {
        Label("보안 마스킹 · 양식 · 병합", systemImage: "eye.slash.fill")
          .font(.system(size: 11, weight: .semibold))
          .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
          .lineLimit(1)
          .fixedSize(horizontal: true, vertical: false)

        FlatUnderlineTextField(
          placeholder: "마스킹 구문",
          text: $viewModel.pdfRedactionPhraseDraft,
          width: 120,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )

        Button("영구 마스킹") {
          _ = viewModel.redactSensitiveTextOnActivePDF()
        }
        .buttonStyle(.plain)
        .font(.system(size: 11, weight: .semibold))
        .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
        .fixedSize(horizontal: true, vertical: false)

        Divider().frame(height: 14)

        Button("페이지 위로") {
          viewModel.moveActivePDFPage(offset: -1)
        }
        .buttonStyle(.plain)
        .font(.system(size: 11, weight: .medium))
        .foregroundStyle(palette.primaryText.swiftUIColor)
        .fixedSize(horizontal: true, vertical: false)
        .disabled(viewModel.selectedPDFPageIndex <= 0)

        Button("페이지 아래로") {
          viewModel.moveActivePDFPage(offset: 1)
        }
        .buttonStyle(.plain)
        .font(.system(size: 11, weight: .medium))
        .foregroundStyle(palette.primaryText.swiftUIColor)
        .fixedSize(horizontal: true, vertical: false)
        .disabled(viewModel.selectedPDFPageIndex + 1 >= pdf.pages.count)

        Button("열린 PDF 병합") {
          viewModel.mergeActivePDFWithAnotherOpenTab()
        }
        .buttonStyle(.plain)
        .font(.system(size: 11, weight: .medium))
        .foregroundStyle(palette.primaryText.swiftUIColor)
        .fixedSize(horizontal: true, vertical: false)

        Divider().frame(height: 14)

        FlatUnderlineTextField(
          placeholder: "필드명",
          text: $viewModel.pdfFormFieldNameDraft,
          width: 95,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )

        FlatUnderlineTextField(
          placeholder: "기본값",
          text: $viewModel.pdfFormFieldValueDraft,
          width: 95,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )

        Button("양식 필드 추가") {
          viewModel.addFormFieldToActivePDF()
        }
        .buttonStyle(.plain)
        .font(.system(size: 11, weight: .medium))
        .foregroundStyle(palette.primaryText.swiftUIColor)
        .fixedSize(horizontal: true, vertical: false)

        Button("체크박스 추가") {
          viewModel.addFormFieldToActivePDF(isCheckbox: true)
        }
        .buttonStyle(.plain)
        .font(.system(size: 11, weight: .medium))
        .foregroundStyle(palette.primaryText.swiftUIColor)
        .fixedSize(horizontal: true, vertical: false)

        if pdf.isRedacted {
          Text("마스킹 적용됨")
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
        }
      }
      .padding(.horizontal, 12)
      .padding(.vertical, 5)
    }
    .background(palette.elevatedSurface.swiftUIColor)
  }

  @ViewBuilder
  private func pdfStampAndBookmarkBar(palette: WorkspaceThemePalette) -> some View {
    AdaptiveScrollContainer(
      axes: .horizontal,
      showsIndicators: false,
      isSnapshotMode: viewModel.isCanvasSnapshotMode
    ) {
      HStack(spacing: 12) {
        Label("책갈피", systemImage: "bookmark.fill")
          .font(.system(size: 11, weight: .semibold))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
          .lineLimit(1)
          .fixedSize(horizontal: true, vertical: false)

        FlatUnderlineTextField(
          placeholder: "책갈피 제목",
          text: $viewModel.bookmarkTitleDraft,
          width: 140,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )

        Button("책갈피 추가") {
          viewModel.addBookmarkToActivePDF()
        }
        .buttonStyle(.plain)
        .font(.system(size: 11, weight: .semibold))
        .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
        .fixedSize(horizontal: true, vertical: false)

        if !pdf.outline.isEmpty {
          Divider().frame(height: 14)
          HStack(spacing: 10) {
            ForEach(pdf.outline) { node in
              HStack(spacing: 4) {
                Button {
                  let targetIdx = max(0, min(pdf.pages.count - 1, node.targetPageNumber - 1))
                  viewModel.selectedPDFPageIndex = targetIdx
                } label: {
                  Text("\(node.title) (p.\(node.targetPageNumber))")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(palette.primaryText.swiftUIColor)
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
                }
                .buttonStyle(.plain)

                Button {
                  viewModel.removeBookmarkFromActivePDF(id: node.id)
                } label: {
                  Image(systemName: "xmark")
                    .font(.system(size: 9))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                }
                .buttonStyle(.plain)
              }
            }
          }
        }
      }
      .padding(.horizontal, 12)
      .padding(.vertical, 5)
    }
    .background(palette.elevatedSurface.swiftUIColor)
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
    let fileName = viewModel.activeTab?.fileName ?? pdf.title
    let totalPageCount = max(1, pdf.pages.count)
    let textLines = page.extractedText
      .split(separator: "\n", omittingEmptySubsequences: false)
      .map(String.init)
    let stampAnnotations = pageAnnotations.filter { $0.kind == .stamp }
    let primaryStampLabel = stampAnnotations.last?.content

    ZStack {
      VStack(alignment: .leading, spacing: 14) {
        HStack(alignment: .top, spacing: 12) {
          VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
              Text(fileName)
                .font(.system(size: 9.5, weight: .semibold))
                .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)

              Text("PAGE \(page.pageNumber) / \(totalPageCount)")
                .font(.system(size: 9.5, weight: .semibold, design: .monospaced))
                .foregroundStyle(palette.secondaryText.swiftUIColor)

              if page.rotationDegrees != 0 {
                Text("회전 \(page.rotationDegrees)°")
                  .font(.system(size: 9.5, weight: .medium, design: .monospaced))
                  .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
              }
            }

            Text(pdf.title)
              .font(.system(size: 17, weight: .bold))
              .foregroundStyle(palette.primaryText.swiftUIColor)
              .lineLimit(1)
          }

          Spacer()

          if let stampText = primaryStampLabel {
            Text(stampText)
              .font(.system(size: 10, weight: .bold, design: .monospaced))
              .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
              .padding(.horizontal, 8)
              .padding(.vertical, 3)
              .border(EasyOfficeDesignTokens.statusDanger.swiftUIColor, width: 1)
              .lineLimit(1)
          }
        }

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          ForEach(Array(textLines.enumerated()), id: \.offset) { lineIdx, line in
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.contains(" | ") {
              let cells = trimmed.components(separatedBy: " | ")
              let prevTrimmed =
                lineIdx > 0
                ? textLines[lineIdx - 1].trimmingCharacters(in: .whitespaces)
                : ""
              let isTableHeader = !prevTrimmed.contains(" | ")
              HStack(spacing: 0) {
                ForEach(Array(cells.enumerated()), id: \.offset) { _, cellText in
                  Text(cellText)
                    .font(.system(size: 10, weight: isTableHeader ? .bold : .regular))
                    .foregroundStyle(palette.primaryText.swiftUIColor)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 5)
                    .border(palette.border.swiftUIColor, width: 0.5)
                }
              }
              .padding(.top, isTableHeader ? 4 : -8)
            } else {
              let isSubheading =
                trimmed.hasPrefix("제") && (trimmed.contains("장") || trimmed.contains("절"))
              let isHeadingLine =
                lineIdx == 0 || isSubheading || trimmed.hasPrefix("1.")
                || trimmed.hasPrefix("2.") || trimmed.hasPrefix("3.")
                || trimmed.hasPrefix("■") || trimmed.hasPrefix("[")
              let isHighlightedLine =
                pageAnnotations.contains {
                  $0.kind == .highlight && !trimmed.isEmpty
                    && (trimmed.contains($0.content.prefix(4))
                      || $0.content.contains(trimmed.prefix(4)))
                } || (lineIdx == 1 && !pageAnnotations.isEmpty)

              Text(trimmed.isEmpty ? " " : line)
                .font(
                  .system(
                    size: isHeadingLine ? 13 : 11.5,
                    weight: isHeadingLine ? .bold : .regular
                  )
                )
                .foregroundStyle(palette.primaryText.swiftUIColor)
                .padding(.top, (isSubheading && lineIdx > 0) ? 4 : 0)
                .padding(.horizontal, isHighlightedLine ? 4 : 0)
                .padding(.vertical, isHighlightedLine ? 2 : 0)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(
                  isHighlightedLine
                    ? EasyOfficeDesignTokens.statusWarning.swiftUIColor.opacity(0.22)
                    : Color.clear
                )
            }
          }
        }

        if !pageAnnotations.isEmpty {
          Divider()
          HStack(spacing: 12) {
            Text("검토 주석")
              .font(.system(size: 10, weight: .semibold))
              .foregroundStyle(palette.secondaryText.swiftUIColor)

            ForEach(pageAnnotations.prefix(4)) { annot in
              Text("· \(annot.kind.rawValue) \(annot.content)")
                .font(.system(size: 10))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
                .lineLimit(1)
            }
          }
        }

        if !pageFormFields.isEmpty {
          Divider()
          HStack(spacing: 16) {
            ForEach(pageFormFields.prefix(4)) { field in
              HStack(spacing: 6) {
                if field.isCheckbox {
                  Button {
                    viewModel.updatePDFFormField(id: field.id, isChecked: !field.isChecked)
                  } label: {
                    HStack(spacing: 4) {
                      Image(
                        systemName: field.isChecked ? "checkmark.square.fill" : "square"
                      )
                      .font(.system(size: 10, weight: .semibold))
                      .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                      Text("\(field.name) (\(field.isChecked ? "예" : "아니오"))")
                        .font(.system(size: 10, weight: .medium, design: .monospaced))
                        .foregroundStyle(palette.primaryText.swiftUIColor)
                        .lineLimit(1)
                    }
                  }
                  .buttonStyle(.plain)
                } else {
                  Text("\(field.name)")
                    .font(.system(size: 10, weight: .medium, design: .monospaced))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                    .lineLimit(1)

                  VStack(alignment: .leading, spacing: 1) {
                    if viewModel.isCanvasSnapshotMode {
                      Text(field.value)
                        .font(.system(size: 10, weight: .semibold, design: .monospaced))
                        .foregroundStyle(palette.primaryText.swiftUIColor)
                        .lineLimit(1)
                    } else {
                      TextField(
                        "입력값",
                        text: Binding(
                          get: { field.value },
                          set: { updated in
                            viewModel.updatePDFFormField(id: field.id, value: updated)
                          }
                        )
                      )
                      .textFieldStyle(.plain)
                      .font(.system(size: 10, weight: .semibold, design: .monospaced))
                      .foregroundStyle(palette.primaryText.swiftUIColor)
                      .frame(width: 96)
                    }
                    Rectangle()
                      .fill(palette.border.swiftUIColor)
                      .frame(width: 96, height: 1)
                  }
                }
              }
            }
          }
        }

        if !pageStrokes.isEmpty {
          Divider()
          HStack(spacing: 12) {
            Text("전자서명")
              .font(.system(size: 10, weight: .semibold))
              .foregroundStyle(palette.secondaryText.swiftUIColor)

            Spacer()

            Canvas { context, size in
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
            .frame(width: 160, height: 32)
          }
        }

        Spacer(minLength: 32)

        Divider()
        HStack {
          Text("\(fileName) · Page \(page.pageNumber)")
            .font(.system(size: 9.5, design: .monospaced))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
          Spacer()
          Text("- p.\(page.pageNumber) / \(totalPageCount) -")
            .font(.system(size: 9.5, weight: .medium, design: .monospaced))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
        }
      }
      .padding(28)

      if !pdf.watermarkText.isEmpty {
        Text(pdf.watermarkText)
          .font(.system(size: 32, weight: .black, design: .rounded))
          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor.opacity(0.14))
          .rotationEffect(.degrees(-22))
          .allowsHitTesting(false)
      }
    }
    .frame(maxWidth: 620, minHeight: 720, alignment: .topLeading)
    .background(palette.cardSurface.swiftUIColor)
    .border(palette.border.swiftUIColor, width: 1)
    .shadow(color: Color.black.opacity(0.08), radius: 6, x: 0, y: 2)
    .frame(maxWidth: .infinity, alignment: .center)
  }

  public var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
    let currentPageNum = viewModel.selectedPDFPageNumber
    let currentPageStrokeCount = pdf.inkStrokes.filter { $0.pageNumber == currentPageNum }.count
    let safeIdx = max(0, min(pdf.pages.count - 1, viewModel.selectedPDFPageIndex))
    let activePage = pdf.pages.indices.contains(safeIdx) ? pdf.pages[safeIdx] : nil

    AdaptiveScrollContainer(
      axes: .vertical,
      showsIndicators: true,
      isSnapshotMode: viewModel.isCanvasSnapshotMode
    ) {
      VStack(alignment: .leading, spacing: 0) {
        if isPrimaryPane {
          pdfPrimaryToolbar(
            palette: palette,
            currentPageNum: currentPageNum,
            currentPageStrokeCount: currentPageStrokeCount
          )

          Divider()

          if viewModel.isAdvancedToolbarExpanded {
            pdfSecurityAndWatermarkBar(palette: palette)
            Divider()
            pdfRedactionAndMergeBar(palette: palette)
            Divider()
            pdfStampAndBookmarkBar(palette: palette)
            Divider()

            if !pdf.formFields.isEmpty {
              VStack(alignment: .leading, spacing: 6) {
                ForEach(pdf.formFields) { field in
                  HStack(spacing: 10) {
                    Text("P\(field.pageNumber)")
                      .font(.system(size: 9, weight: .bold, design: .monospaced))
                      .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)

                    Text(field.name)
                      .font(.system(size: 11, weight: .semibold))
                      .foregroundStyle(palette.primaryText.swiftUIColor)
                      .frame(minWidth: 110, alignment: .leading)

                    if field.isCheckbox {
                      FlatToggleControl(
                        title: field.isChecked ? "선택됨" : "해제됨",
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
                      FlatUnderlineTextField(
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
              .padding(.horizontal, 12)
              .padding(.vertical, 8)
              .background(palette.elevatedSurface.swiftUIColor)
              Divider()
            }
          }
        }

        if !pdf.pages.isEmpty {
          VStack(alignment: .leading, spacing: 12) {
            if showRawPDFKitView && !viewModel.isCanvasSnapshotMode {
              PDFKitRepresentedView(
                pdfDocument: pdf, selectedPageIndex: viewModel.selectedPDFPageIndex,
                zoomFactor: CGFloat(viewModel.zoomPercent / 100.0)
              )
              .frame(minHeight: 360, maxHeight: .infinity)
              .border(palette.border.swiftUIColor, width: 1)
            } else if let page = activePage {
              pdfNativePageSheetView(
                page: page,
                pageNumber: currentPageNum,
                palette: palette
              )
            }

            if isPrimaryPane
              && (viewModel.isPDFInkDrawingModeEnabled || viewModel.isPDFInkOverlayExpanded)
            {
              PDFInteractiveInkOverlayView(viewModel: viewModel, palette: palette, pdfDocument: pdf)
            }
          }
          .padding(24)
          .background(palette.background.swiftUIColor)
        }
      }
    }
  }
}
