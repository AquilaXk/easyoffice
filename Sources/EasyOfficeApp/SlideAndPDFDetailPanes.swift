import AppKit
import EasyOfficeCore
import EasyOfficeDesignSystem
import Observation
import PDFKit
import SwiftUI

/// Rich interactive editor pane for Slide (.pptx/.odp) with Filmstrip, Slide Sorter Grid View, proportional 16:9 Stage Canvas, 7-kind element vector shapes, 8-direction resize handles, rotation handle (↻), element entrance animations, and full-screen slideshow launcher.
public struct SlideEditorDetailPaneView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  let deck: PresentationDeckModel
  let isPrimaryPane: Bool
  @GestureState private var activeDragOffset: (elementID: UUID, translation: CGSize)? = nil

  public init(
    viewModel: WorkspaceViewModel, deck: PresentationDeckModel, isPrimaryPane: Bool = true
  ) {
    self.viewModel = viewModel
    self.deck = deck
    self.isPrimaryPane = isPrimaryPane
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

    VStack(alignment: .leading, spacing: 14) {
      // Top Slide Deck Management & Sorter / Presenter / Slideshow Bar
      HStack(spacing: 8) {
        Label(
          "\(deck.aspectRatio.rawValue) · 슬라이드 \(deck.slides.count)장",
          systemImage: "rectangle.on.rectangle.angled"
        )
        .font(.system(size: 11, weight: .semibold))
        .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)

        if isPrimaryPane {
          Divider().frame(height: 16)

          Button {
            viewModel.addSlideToActiveDeck()
          } label: {
            Label("슬라이드 추가", systemImage: "plus.rectangle.on.rectangle")
              .font(.system(size: 11, weight: .semibold))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)

          Button {
            viewModel.duplicateActiveSlide()
          } label: {
            Label("복제", systemImage: "doc.on.doc")
              .font(.system(size: 11, weight: .medium))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)

          Button {
            viewModel.moveActiveSlide(offset: -1)
          } label: {
            Label("위로", systemImage: "arrow.up")
              .font(.system(size: 11, weight: .medium))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)
          .disabled(activeIdx == 0)

          Button {
            viewModel.moveActiveSlide(offset: 1)
          } label: {
            Label("아래로", systemImage: "arrow.down")
              .font(.system(size: 11, weight: .medium))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)
          .disabled(activeIdx + 1 >= deck.slides.count)

          Button {
            viewModel.deleteActiveSlide()
          } label: {
            Label("삭제", systemImage: "trash")
              .font(.system(size: 11, weight: .medium))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)
          .disabled(deck.slides.count <= 1)

          Button {
            viewModel.toggleSlideSorterView()
          } label: {
            Label(
              viewModel.isSlideSorterViewEnabled ? "슬라이드 편집 뷰" : "슬라이드 정렬 뷰",
              systemImage: "square.grid.3x2.fill"
            )
            .font(.system(size: 11, weight: .semibold))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)
        }

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

      if isPrimaryPane {
        HStack(spacing: 8) {
          Menu {
            ForEach(SlideDeckTheme.allCases, id: \.rawValue) { theme in
              Button(theme.rawValue) {
                viewModel.applyThemeToActiveDeck(theme)
              }
            }
          } label: {
            Label("테마: \(deck.theme.rawValue)", systemImage: "paintpalette.fill")
              .font(.system(size: 11, weight: .semibold))
          }
          .controlSize(.small)

          Divider().frame(height: 16)

          HStack(spacing: 4) {
            Button("좌") {
              viewModel.alignSlideElements(.alignLeft)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)

            Button("중") {
              viewModel.alignSlideElements(.alignCenter)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)

            Button("우") {
              viewModel.alignSlideElements(.alignRight)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)

            Button("배분") {
              viewModel.alignSlideElements(.distributeHorizontally)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
          }

          Divider().frame(height: 16)

          Button {
            viewModel.duplicateActiveSlideElement()
          } label: {
            Label("개체 복제", systemImage: "plus.square.on.square")
              .font(.system(size: 11, weight: .medium))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)
          .disabled(selectedElement == nil)

          Button {
            viewModel.deleteActiveSlideElement()
          } label: {
            Label("개체 삭제", systemImage: "trash")
              .font(.system(size: 11, weight: .medium))
          }
          .buttonStyle(.bordered)
          .controlSize(.small)
          .disabled(selectedElement == nil)

          Spacer()
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(palette.cardSurface.swiftUIColor)
        .clipShape(RoundedRectangle(cornerRadius: 6))
      }

      if viewModel.isSlideSorterViewEnabled {
        slideSorterGridView(activeIdx: activeIdx, palette: palette)
      } else {
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
                .frame(
                  width: EasyOfficeDesignTokens.LayoutMetrics.slideFilmstripWidth,
                  alignment: .leading
                )
                .background(
                  isSelected
                    ? palette.selectedSurface.swiftUIColor
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

              // Proportional 16:9 Stage Canvas with Visual Snap Guides, 7-Kind Shapes, 8-Direction Resize Handles & Rotation Handle (↻)
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

                    ForEach(Array(slide.bulletPoints.enumerated()), id: \.offset) {
                      bIdx, bulletText in
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
              HStack(spacing: 8) {
                Label("발표자 노트", systemImage: "note.text")
                  .font(.system(size: 11, weight: .semibold))
                  .foregroundStyle(palette.secondaryText.swiftUIColor)
                if isPrimaryPane {
                  TextField(
                    "발표자 노트를 입력하세요...",
                    text: Binding(
                      get: { slide.speakerNotes },
                      set: { viewModel.updateActiveSlideSpeakerNotes($0) }
                    )
                  )
                  .textFieldStyle(.roundedBorder)
                  .font(.system(size: 11))
                } else {
                  Text(slide.speakerNotes)
                    .font(.system(size: 11))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                }
              }
            }
            .padding(24)
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
        .onReceive(Timer.publish(every: 1.0, on: .main, in: .common).autoconnect()) { _ in
          if viewModel.isPresenterModeActive && viewModel.isPresenterTimerRunning {
            viewModel.tickPresenterTimer(seconds: 1)
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

          ForEach(PDFAnnotationKind.allCases, id: \.rawValue) { kind in
            Button {
              viewModel.addPDFAnnotation(kind: kind, content: "\(kind.rawValue) 검토 마크")
            } label: {
              Text(kind.rawValue).font(.system(size: 11, weight: .medium))
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .accessibilityLabel(
              AccessibilityTokens.pdfAnnotationToolLabel(toolTitle: kind.rawValue))
          }

          Toggle(isOn: $viewModel.isPDFInkDrawingModeEnabled) {
            Label("자유곡선 펜 그리기", systemImage: "pencil.tip.crop.circle").font(
              .system(size: 11, weight: .semibold))
          }
          .toggleStyle(.button)
          .tint(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)

          Button {
            viewModel.executeRibbonCommand("pdf-mod-tooffice")
          } label: {
            Label("DOCX 편집 변환", systemImage: "arrow.right.doc.on.clipboard").font(
              .system(size: 11, weight: .semibold))
          }
          .buttonStyle(.borderedProminent)
          .tint(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)

          Spacer()

          Text(
            "주석 \(pdf.annotations.count)건 · 벡터 획 \(currentPageStrokeCount)건 · 양식 \(pdf.formFields.count)건"
          )
          .font(.system(size: 11))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        }
        .padding(10)
        .background(palette.cardSurface.swiftUIColor)
        .clipShape(RoundedRectangle(cornerRadius: 6))

        // Redaction, AcroForm Field Creation, Page Reordering & Open PDF Tab Merge Bar
        HStack(spacing: 8) {
          Label("보안 리댁션 · 양식 · 병합", systemImage: "eye.slash.fill")
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)

          TextField("마스킹 구문", text: $viewModel.pdfRedactionPhraseDraft)
            .textFieldStyle(.roundedBorder)
            .font(.system(size: 11))
            .frame(width: 120)

          Button("영구 리댁션") {
            _ = viewModel.redactSensitiveTextOnActivePDF()
          }
          .buttonStyle(.borderedProminent)
          .tint(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
          .controlSize(.small)

          Divider().frame(height: 16)

          Button("페이지 위로") {
            viewModel.moveActivePDFPage(offset: -1)
          }
          .buttonStyle(.bordered)
          .controlSize(.small)
          .disabled(viewModel.selectedPDFPageIndex <= 0)

          Button("페이지 아래로") {
            viewModel.moveActivePDFPage(offset: 1)
          }
          .buttonStyle(.bordered)
          .controlSize(.small)
          .disabled(viewModel.selectedPDFPageIndex + 1 >= pdf.pages.count)

          Button("열린 PDF 병합") {
            viewModel.mergeActivePDFWithAnotherOpenTab()
          }
          .buttonStyle(.bordered)
          .controlSize(.small)

          Divider().frame(height: 16)

          TextField("필드명", text: $viewModel.pdfFormFieldNameDraft)
            .textFieldStyle(.roundedBorder)
            .font(.system(size: 11))
            .frame(width: 90)

          TextField("기본값", text: $viewModel.pdfFormFieldValueDraft)
            .textFieldStyle(.roundedBorder)
            .font(.system(size: 11))
            .frame(width: 90)

          Button("양식 필드 추가") {
            viewModel.addFormFieldToActivePDF()
          }
          .buttonStyle(.bordered)
          .controlSize(.small)

          Button("체크박스 추가") {
            viewModel.addFormFieldToActivePDF(isCheckbox: true)
          }
          .buttonStyle(.bordered)
          .controlSize(.small)

          Spacer()

          if pdf.isRedacted {
            Label("비가역 리댁션 적용됨", systemImage: "lock.shield.fill")
              .font(.system(size: 10, weight: .bold))
              .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
          }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(palette.cardSurface.swiftUIColor)
        .clipShape(RoundedRectangle(cornerRadius: 6))

        // 4-Preset Official Approval Stamp Bar & Interactive Bookmark Bar
        HStack(spacing: 8) {
          Label("결재 스탬프", systemImage: "seal.fill")
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)

          ForEach(["승인", "대외비", "검토필", "긴급"], id: \.self) { preset in
            Button {
              viewModel.addStampToActivePDFPage(preset: preset)
            } label: {
              Text(preset)
                .font(.system(size: 11, weight: .bold))
            }
            .buttonStyle(.bordered)
            .tint(
              preset == "긴급" || preset == "대외비"
                ? EasyOfficeDesignTokens.statusDanger.swiftUIColor
                : EasyOfficeDesignTokens.brand700.swiftUIColor
            )
            .controlSize(.small)
          }

          Divider().frame(height: 16)

          Label("책갈피", systemImage: "bookmark.fill")
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)

          TextField("책갈피 제목", text: $viewModel.bookmarkTitleDraft)
            .textFieldStyle(.roundedBorder)
            .font(.system(size: 11))
            .frame(width: 140)

          Button("책갈피 추가") {
            viewModel.addBookmarkToActivePDF()
          }
          .buttonStyle(.borderedProminent)
          .tint(EasyOfficeDesignTokens.brand700.swiftUIColor)
          .controlSize(.small)

          if !pdf.outline.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
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
            .frame(maxWidth: 260)
          }

          Spacer()
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(palette.cardSurface.swiftUIColor)
        .clipShape(RoundedRectangle(cornerRadius: 6))
      }

      // Interactive AcroForm Fields Card
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
                if isPrimaryPane {
                  Toggle(
                    isOn: Binding(
                      get: { field.isChecked },
                      set: { nextChecked in
                        viewModel.updatePDFFormField(id: field.id, isChecked: nextChecked)
                      }
                    )
                  ) {
                    Text(field.isChecked ? "선택됨 (Checked)" : "해제됨 (Unchecked)")
                      .font(.system(size: 11))
                  }
                } else {
                  Text(field.isChecked ? "☑ 선택됨" : "☐ 해제됨")
                    .font(.system(size: 11))
                }
              } else {
                if isPrimaryPane {
                  TextField(
                    "양식 입력값",
                    text: Binding(
                      get: { field.value },
                      set: { nextVal in
                        viewModel.updatePDFFormField(id: field.id, value: nextVal)
                      }
                    )
                  )
                  .textFieldStyle(.roundedBorder)
                  .font(.system(size: 11))
                } else {
                  Text(field.value)
                    .font(.system(size: 11))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                }
              }

              Spacer()
            }
          }
        }
        .padding(12)
        .background(palette.cardSurface.swiftUIColor)
        .overlay(
          RoundedRectangle(cornerRadius: 8)
            .stroke(palette.border.swiftUIColor, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 8))
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

          PDFKitRepresentedView(
            pdfDocument: pdf, selectedPageIndex: viewModel.selectedPDFPageIndex,
            zoomFactor: CGFloat(viewModel.zoomPercent / 100.0)
          )
          .frame(height: 260)
          .clipShape(RoundedRectangle(cornerRadius: 6))

          if isPrimaryPane {
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

      ForEach(Array(pdf.pages.enumerated()), id: \.element.id) { idx, page in
        let isSelected = viewModel.selectedPDFPageIndex == idx
        let pageAnnotations = pdf.annotations.filter { $0.pageNumber == page.pageNumber }
        let pageStrokes = pdf.inkStrokes.filter { $0.pageNumber == page.pageNumber }

        VStack(alignment: .leading, spacing: 10) {
          HStack {
            Text(
              "PDF 페이지 \(page.pageNumber) · 회전 \(page.rotationDegrees)° · 벡터 획 \(pageStrokes.count)건"
            )
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
            Spacer()
            if isSelected {
              Text("선택된 페이지").font(.system(size: 10, weight: .semibold)).foregroundStyle(
                EasyOfficeDesignTokens.brand700.swiftUIColor)
            }
          }

          Text(page.extractedText).font(.system(size: 13)).foregroundStyle(
            palette.primaryText.swiftUIColor)

          if !pageAnnotations.isEmpty {
            Divider()
            ForEach(pageAnnotations) { ann in
              if ann.kind == .stamp {
                HStack(spacing: 6) {
                  Image(systemName: "seal.fill")
                    .font(.system(size: 11))
                    .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
                  Text("[결재 스탬프] \(ann.content)")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(palette.subtleAccentSurface.swiftUIColor)
                .overlay(
                  RoundedRectangle(cornerRadius: 5)
                    .stroke(EasyOfficeDesignTokens.statusDanger.swiftUIColor, lineWidth: 1.2)
                )
                .clipShape(RoundedRectangle(cornerRadius: 5))
              } else {
                HStack(spacing: 6) {
                  Text("[\(ann.kind.rawValue)]").font(.system(size: 10, weight: .bold))
                    .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
                  Text(ann.content).font(.system(size: 11)).foregroundStyle(
                    palette.secondaryText.swiftUIColor)
                }
              }
            }
          }
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(palette.cardSurface.swiftUIColor)
        .overlay(
          RoundedRectangle(cornerRadius: 8)
            .stroke(
              isSelected
                ? EasyOfficeDesignTokens.brand700.swiftUIColor : palette.border.swiftUIColor,
              lineWidth: isSelected ? 1.5 : 1.0)
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
