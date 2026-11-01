import Foundation
import Observation

extension WorkspaceViewModel {

  public func applyThemeToActiveDeck(_ theme: SlideDeckTheme) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    deck.applyTheme(theme)
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 마스터 테마 적용 (\(theme.rawValue))"
  }

  public func alignSlideElements(_ alignment: SlideElementAlignment) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload,
      !deck.slides.isEmpty
    else { return }
    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
    recordUndoSnapshot(for: activeTabID)
    deck.slides[sIdx].alignElements(
      alignment,
      targetID: selectedSlideElementID,
      stageSize: (deck.aspectRatio.widthPoints, deck.aspectRatio.heightPoints)
    )
    activeSnapGuides = []
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 개체 정렬 (\(alignment.rawValue))"
  }

  public func duplicateActiveSlideElement() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload,
      !deck.slides.isEmpty
    else { return }
    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
    guard let targetID = selectedSlideElementID ?? deck.slides[sIdx].elements.last?.id else {
      return
    }
    recordUndoSnapshot(for: activeTabID)
    if let copy = deck.slides[sIdx].duplicateElement(
      id: targetID,
      stageSize: (deck.aspectRatio.widthPoints, deck.aspectRatio.heightPoints)
    ) {
      selectedSlideElementID = copy.id
    }
    for i in deck.slides[sIdx].elements.indices {
      deck.slides[sIdx].elements[i].zIndex = i
    }
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 개체 복제 완료"
  }

  public func deleteActiveSlideElement() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload,
      !deck.slides.isEmpty
    else { return }
    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
    guard let targetID = selectedSlideElementID,
      deck.slides[sIdx].elements.contains(where: { $0.id == targetID })
    else {
      lastExecutedCommandSummary = "삭제할 슬라이드 개체가 선택되지 않았습니다"
      return
    }
    recordUndoSnapshot(for: activeTabID)
    deck.slides[sIdx].removeElement(id: targetID)
    for i in deck.slides[sIdx].elements.indices {
      deck.slides[sIdx].elements[i].zIndex = i
    }
    selectedSlideElementID = nil
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 개체 삭제 완료"
  }

  public func updateActiveSlideSpeakerNotes(_ notes: String) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload,
      !deck.slides.isEmpty
    else { return }
    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
    guard deck.slides[sIdx].speakerNotes != notes else { return }
    recordUndoSnapshot(
      for: activeTabID,
      coalescingTargetKey: "slide-notes-\(activeTabID.uuidString)-\(sIdx)"
    )
    deck.slides[sIdx].speakerNotes = notes
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "발표자 노트 갱신 완료"
  }

  public func selectSlideElement(id: UUID?) {
    activeInlineEditTargetKey = nil
    selectedSlideElementID = id
    activeSnapGuides = []
  }

  public func moveSlideElement(
    slideIndex: Int? = nil,
    elementID: UUID,
    deltaX: Double,
    deltaY: Double
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    let targetSlideIdx =
      slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
    guard deck.slides.indices.contains(targetSlideIdx),
      let eIdx = deck.slides[targetSlideIdx].elements.firstIndex(where: { $0.id == elementID })
    else { return }
    recordUndoSnapshot(for: activeTabID)
    var elem = deck.slides[targetSlideIdx].elements[eIdx]
    elem.x = max(0.0, min(deck.aspectRatio.widthPoints - elem.width, elem.x + deltaX))
    elem.y = max(0.0, min(deck.aspectRatio.heightPoints - elem.height, elem.y + deltaY))
    deck.slides[targetSlideIdx].elements[eIdx] = elem
    let siblings = deck.slides[targetSlideIdx].elements.enumerated().compactMap {
      $0 == eIdx ? nil : $1
    }
    activeSnapGuides = SnapGuideCalculator.computeGuides(
      for: elem,
      siblings: siblings,
      aspectRatio: deck.aspectRatio,
      tolerancePoints: 5.0
    )
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
  }

  public func resizeSlideElement(
    slideIndex: Int? = nil,
    elementID: UUID,
    deltaWidth: Double,
    deltaHeight: Double
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    let targetSlideIdx =
      slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
    guard deck.slides.indices.contains(targetSlideIdx),
      let eIdx = deck.slides[targetSlideIdx].elements.firstIndex(where: { $0.id == elementID })
    else { return }
    recordUndoSnapshot(for: activeTabID)
    var elem = deck.slides[targetSlideIdx].elements[eIdx]
    elem.width = max(40.0, min(deck.aspectRatio.widthPoints - elem.x, elem.width + deltaWidth))
    elem.height = max(28.0, min(deck.aspectRatio.heightPoints - elem.y, elem.height + deltaHeight))
    deck.slides[targetSlideIdx].elements[eIdx] = elem
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
  }

  public func resizeSlideElementByHandle(
    slideIndex: Int? = nil,
    elementID: UUID,
    handle: SlideResizeHandle,
    deltaX: Double,
    deltaY: Double
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    let sIdx =
      slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
    guard deck.slides.indices.contains(sIdx),
      let eIdx = deck.slides[sIdx].elements.firstIndex(where: { $0.id == elementID })
    else { return }
    recordUndoSnapshot(for: activeTabID)
    deck.slides[sIdx].resizeElement(
      id: elementID,
      handle: handle,
      deltaX: deltaX,
      deltaY: deltaY,
      stageSize: deck.aspectRatio.stageSize
    )
    let updatedElem = deck.slides[sIdx].elements[eIdx]
    let siblings = deck.slides[sIdx].elements.enumerated().compactMap { $0 == eIdx ? nil : $1 }
    activeSnapGuides = SnapGuideCalculator.computeGuides(
      for: updatedElem,
      siblings: siblings,
      aspectRatio: deck.aspectRatio,
      tolerancePoints: 5.0
    )
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 개체 핸들 크기 조정 (\(handle.rawValue))"
  }

  public func rotateSlideElement(
    slideIndex: Int? = nil,
    elementID: UUID,
    toDegrees degrees: Double
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    let sIdx =
      slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
    guard deck.slides.indices.contains(sIdx),
      deck.slides[sIdx].elements.contains(where: { $0.id == elementID })
    else { return }
    recordUndoSnapshot(for: activeTabID)
    deck.slides[sIdx].rotateElement(id: elementID, toDegrees: degrees)
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 개체 회전 (\(Int(degrees))°)"
  }

  public func cycleSlideElementAnimation(slideIndex: Int? = nil, elementID: UUID) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    let sIdx =
      slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
    guard deck.slides.indices.contains(sIdx),
      let eIdx = deck.slides[sIdx].elements.firstIndex(where: { $0.id == elementID })
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let allAnims = SlideElementAnimation.allCases
    let next = allAnims[
      ((allAnims.firstIndex(of: deck.slides[sIdx].elements[eIdx].animation) ?? 0) + 1)
        % allAnims.count
    ]
    deck.slides[sIdx].elements[eIdx].animation = next
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 개체 애니메이션 전환: \(next.rawValue)"
  }

  public func toggleFullScreenSlideshow() {
    isFullScreenSlideshowActive.toggle()
    isPresenterTimerRunning = isFullScreenSlideshowActive
    lastExecutedCommandSummary = isFullScreenSlideshowActive ? "전체화면 슬라이드쇼 시작" : "전체화면 슬라이드쇼 종료"
  }

  public func moveSlideElementZOrder(slideIndex: Int? = nil, elementID: UUID, bringForward: Bool) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    let targetSlideIdx =
      slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
    guard deck.slides.indices.contains(targetSlideIdx) else { return }
    var orderedElements = deck.slides[targetSlideIdx].elements.enumerated().sorted {
      if $0.element.zIndex != $1.element.zIndex {
        return $0.element.zIndex < $1.element.zIndex
      }
      return $0.offset < $1.offset
    }.map(\.element)
    guard let eIdx = orderedElements.firstIndex(where: { $0.id == elementID }) else { return }
    let nextIdx =
      bringForward
      ? min(orderedElements.count - 1, eIdx + 1) : max(0, eIdx - 1)
    guard nextIdx != eIdx else { return }
    recordUndoSnapshot(for: activeTabID)
    orderedElements.swapAt(eIdx, nextIdx)
    for i in orderedElements.indices {
      orderedElements[i].zIndex = i
    }
    deck.slides[targetSlideIdx].elements = orderedElements
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = bringForward ? "슬라이드 개체 앞으로 이동" : "슬라이드 개체 뒤로 이동"
  }

  public func updateSlideElementText(
    slideIndex: Int? = nil,
    elementID: UUID,
    title: String? = nil,
    bodyText: String? = nil
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    let targetSlideIdx =
      slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
    guard deck.slides.indices.contains(targetSlideIdx),
      let eIdx = deck.slides[targetSlideIdx].elements.firstIndex(where: { $0.id == elementID })
    else { return }
    let existing = deck.slides[targetSlideIdx].elements[eIdx]
    let nextTitle = title ?? existing.title
    let nextBody = bodyText ?? existing.bodyText
    guard existing.title != nextTitle || existing.bodyText != nextBody else { return }
    recordUndoSnapshot(
      for: activeTabID,
      coalescingTargetKey: "slide-elem-\(activeTabID.uuidString)-\(elementID.uuidString)"
    )
    deck.slides[targetSlideIdx].elements[eIdx].title = nextTitle
    deck.slides[targetSlideIdx].elements[eIdx].bodyText = nextBody
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
  }

  public func updateSlideSpeakerNotes(slideIndex: Int? = nil, notes: String) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    let targetSlideIdx =
      slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
    guard deck.slides.indices.contains(targetSlideIdx),
      deck.slides[targetSlideIdx].speakerNotes != notes
    else { return }
    recordUndoSnapshot(
      for: activeTabID,
      coalescingTargetKey: "slide-notes-\(activeTabID.uuidString)-\(targetSlideIdx)"
    )
    deck.slides[targetSlideIdx].speakerNotes = notes
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
  }

  public func addSlideBulletPoint(slideIndex: Int? = nil, text: String) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    let targetSlideIdx =
      slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
    guard deck.slides.indices.contains(targetSlideIdx) else { return }
    recordUndoSnapshot(for: activeTabID)
    deck.slides[targetSlideIdx].bulletPoints.append(text)
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
  }

  public func updateSlideBulletPoint(slideIndex: Int? = nil, bulletIndex: Int, newText: String) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    let targetSlideIdx =
      slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
    guard deck.slides.indices.contains(targetSlideIdx),
      deck.slides[targetSlideIdx].bulletPoints.indices.contains(bulletIndex),
      deck.slides[targetSlideIdx].bulletPoints[bulletIndex] != newText
    else { return }
    recordUndoSnapshot(
      for: activeTabID,
      coalescingTargetKey: "slide-bullet-\(activeTabID.uuidString)-\(targetSlideIdx)-\(bulletIndex)"
    )
    deck.slides[targetSlideIdx].bulletPoints[bulletIndex] = newText
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
  }

  public func advancePresenterSlide() {
    guard let activeTab, case .slide(let deck) = activeTab.payload, !deck.slides.isEmpty else {
      return
    }
    if selectedSlideIndex + 1 < deck.slides.count { selectedSlideIndex += 1 }
  }

  public func previousPresenterSlide() {
    guard selectedSlideIndex > 0 else { return }
    selectedSlideIndex -= 1
  }

  public func tickPresenterTimer(seconds: Int = 1) {
    guard isPresenterTimerRunning, seconds > 0 else { return }
    presenterElapsedSeconds += seconds
  }

  public func addSlideToActiveDeck(title: String? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let nextNumber = deck.slides.count + 1
    let trimmed = (title ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    let resolvedTitle = trimmed.isEmpty ? "슬라이드 \(nextNumber)" : trimmed
    let newSlide = SlideModel(
      slideNumber: nextNumber,
      title: resolvedTitle,
      bulletPoints: ["핵심 안건을 입력하십시오."]
    )
    deck.slides.append(newSlide)
    selectedSlideIndex = deck.slides.count - 1
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "새 슬라이드 추가 완료 (#\(nextNumber))"
  }

  public func duplicateActiveSlide() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload,
      !deck.slides.isEmpty
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
    let source = deck.slides[sIdx]
    let clonedElements = source.elements.map { elem in
      SlideCanvasElement(
        id: UUID(),
        title: elem.title,
        bodyText: elem.bodyText,
        x: elem.x,
        y: elem.y,
        width: elem.width,
        height: elem.height,
        kind: elem.kind,
        zIndex: elem.zIndex,
        rotationDegrees: elem.rotationDegrees,
        animation: elem.animation,
        fillHex: elem.fillHex,
        strokeHex: elem.strokeHex,
        isBold: elem.isBold,
        isItalic: elem.isItalic,
        isUnderlined: elem.isUnderlined
      )
    }
    let duplicated = SlideModel(
      id: UUID(),
      slideNumber: sIdx + 2,
      title: source.title,
      subtitle: source.subtitle,
      bulletPoints: source.bulletPoints,
      speakerNotes: source.speakerNotes,
      masterLayout: source.masterLayout,
      transition: source.transition,
      elements: clonedElements,
      transitionDurationSeconds: source.transitionDurationSeconds,
      autoAdvanceSeconds: source.autoAdvanceSeconds
    )
    deck.slides.insert(duplicated, at: sIdx + 1)
    selectedSlideIndex = sIdx + 1
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 복제 완료 (#\(sIdx + 2))"
  }

  public func deleteActiveSlide() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload,
      deck.slides.count > 1
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
    deck.slides.remove(at: sIdx)
    selectedSlideIndex = max(0, min(deck.slides.count - 1, sIdx))
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 삭제 완료 (총 \(deck.slides.count)장)"
  }

  public func moveActiveSlide(offset: Int) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload,
      !deck.slides.isEmpty
    else { return }
    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
    let clampedTarget = max(0, min(deck.slides.count - 1, sIdx + offset))
    guard clampedTarget != sIdx else { return }
    recordUndoSnapshot(for: activeTabID)
    deck.moveSlide(from: sIdx, to: clampedTarget)
    selectedSlideIndex = clampedTarget
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 순서 이동 완료 (#\(sIdx + 1) → #\(clampedTarget + 1))"
  }

  public func toggleSlideSorterView() {
    isSlideSorterViewEnabled.toggle()
    lastExecutedCommandSummary =
      isSlideSorterViewEnabled ? "슬라이드 정렬 격자 뷰 활성화" : "슬라이드 정렬 격자 뷰 해제"
  }

  public func setActiveSlideTransitionDuration(_ seconds: Double) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload,
      !deck.slides.isEmpty
    else { return }
    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
    recordUndoSnapshot(for: activeTabID)
    let clamped = max(0.2, min(5.0, seconds.isFinite ? seconds : 0.8))
    deck.slides[sIdx].transitionDurationSeconds = clamped
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary =
      "슬라이드 전환 시간 설정 (\(String(format: "%.1f", deck.slides[sIdx].transitionDurationSeconds))초)"
  }

  public func setActiveSlideAutoAdvance(seconds: Int?) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload,
      !deck.slides.isEmpty
    else { return }
    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
    recordUndoSnapshot(for: activeTabID)
    let clamped = seconds.map { max(1, min(300, $0)) }
    deck.slides[sIdx].autoAdvanceSeconds = clamped
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    if let adv = deck.slides[sIdx].autoAdvanceSeconds {
      lastExecutedCommandSummary = "슬라이드 자동 전환 설정 (\(adv)초)"
    } else {
      lastExecutedCommandSummary = "슬라이드 자동 전환 해제 (수동 클릭)"
    }
  }

  public func toggleSlideGridlines() {
    isSlideGridlinesVisible.toggle()
    lastExecutedCommandSummary =
      isSlideGridlinesVisible ? "슬라이드 스테이지 격자 안내선 표시" : "슬라이드 스테이지 격자 안내선 숨김"
  }


  public func setActiveParagraphStyle(_ role: ParagraphStyleRole) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID })
    else { return }

    let targetMetrics: (fontSizePt: Double, isBold: Bool, isItalic: Bool)
    switch role {
    case .title:
      targetMetrics = (20.0, true, false)
    case .heading1:
      targetMetrics = (14.0, true, false)
    case .heading2:
      targetMetrics = (12.5, true, false)
    case .heading3:
      targetMetrics = (11.5, true, false)
    case .body:
      targetMetrics = (11.0, false, false)
    case .quote:
      targetMetrics = (11.0, false, true)
    case .officialHeader:
      targetMetrics = (16.0, true, false)
    }

    switch openTabs[idx].payload {
    case .document(var doc):
      guard !doc.paragraphs.isEmpty else { return }
      recordUndoSnapshot(for: activeTabID)
      let pIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
      doc.paragraphs[pIdx].style = role
      doc.paragraphs[pIdx].fontSizePt = targetMetrics.fontSizePt
      doc.paragraphs[pIdx].isBold = targetMetrics.isBold
      doc.paragraphs[pIdx].isItalic = targetMetrics.isItalic
      for rIdx in doc.paragraphs[pIdx].runs.indices {
        doc.paragraphs[pIdx].runs[rIdx].fontSizePt = targetMetrics.fontSizePt
        doc.paragraphs[pIdx].runs[rIdx].isBold = targetMetrics.isBold
        doc.paragraphs[pIdx].runs[rIdx].isItalic = targetMetrics.isItalic
      }
      openTabs[idx].payload = .document(doc)
      openTabs[idx].isModified = true
      lastExecutedCommandSummary = "문단 스타일 변경: \(role.rawValue)"

    default:
      break
    }
  }

  @discardableResult
  public func splitParagraph(
    paragraphID: UUID,
    atUTF16Offset offset: Int,
    selectionLength: Int? = nil
  ) -> UUID? {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      let pIdx = doc.paragraphs.firstIndex(where: { $0.id == paragraphID })
    else { return nil }

    recordUndoSnapshot(for: activeTabID)
    let source = doc.paragraphs[pIdx]
    let resolvedDeletionLength: Int
    if let selectionLength {
      resolvedDeletionLength = max(0, selectionLength)
    } else if selectedParagraphID == paragraphID,
      let selRange = selectedParagraphUTF16Range,
      !selRange.isEmpty,
      selRange.lowerBound == offset
    {
      resolvedDeletionLength = selRange.count
    } else {
      resolvedDeletionLength = 0
    }

    let splitResult = source.splitContentAndRuns(
      atUTF16Offset: offset,
      deletingUTF16Length: resolvedDeletionLength
    )
    let hadExplicitRuns = !source.runs.isEmpty

    doc.paragraphs[pIdx].text = splitResult.leftText
    doc.paragraphs[pIdx].runs = hadExplicitRuns ? splitResult.leftRuns : []

    let nextStyle: ParagraphStyleRole =
      (source.style == .title || source.style == .heading1 || source.style == .heading2
        || source.style == .heading3 || source.style == .officialHeader)
      ? .body : source.style

    let newParagraph = ParagraphBlock(
      style: nextStyle,
      alignment: source.alignment,
      text: splitResult.rightText,
      runs: hadExplicitRuns ? splitResult.rightRuns : [],
      fontName: source.fontName,
      fontSizePt: nextStyle == .body && source.style != .body ? 11.0 : source.fontSizePt,
      isBold: nextStyle == .body && source.style != .body ? false : source.isBold,
      isItalic: source.isItalic,
      isUnderlined: source.isUnderlined,
      lineSpacingPercent: source.lineSpacingPercent,
      firstLineIndentPt: source.firstLineIndentPt,
      leftIndentPt: source.leftIndentPt,
      listKind: source.listKind,
      listLevel: source.listLevel,
      letterSpacingPercent: source.letterSpacingPercent,
      charWidthScalePercent: source.charWidthScalePercent
    )

    doc.paragraphs.insert(newParagraph, at: pIdx + 1)
    selectedParagraphID = newParagraph.id
    selectedParagraphUTF16Range = 0..<0
    pendingFocusParagraphID = newParagraph.id
    pendingCaretUTF16Offset = 0
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "문단 분할 완료"
    return newParagraph.id
  }

  @discardableResult
  public func mergeParagraphWithPrevious(paragraphID: UUID) -> UUID? {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      let pIdx = doc.paragraphs.firstIndex(where: { $0.id == paragraphID }),
      pIdx > 0
    else { return nil }

    recordUndoSnapshot(for: activeTabID)
    let prevIdx = pIdx - 1
    let prevParagraph = doc.paragraphs[prevIdx]
    let current = doc.paragraphs[pIdx]
    let prevUTF16Count = prevParagraph.text.utf16.count
    let hadExplicitRuns = !prevParagraph.runs.isEmpty || !current.runs.isEmpty
    let mergedRuns =
      hadExplicitRuns
      ? DocumentTextRun.coalesceAdjacentRuns(prevParagraph.effectiveRuns + current.effectiveRuns)
      : []

    doc.paragraphs[prevIdx].text = prevParagraph.text + current.text
    doc.paragraphs[prevIdx].runs = mergedRuns

    let targetPrevID = doc.paragraphs[prevIdx].id
    Self.relinkParagraphAnchors(in: &doc, from: current.id, to: targetPrevID)

    doc.paragraphs.remove(at: pIdx)
    selectedParagraphID = targetPrevID
    selectedParagraphUTF16Range = prevUTF16Count..<prevUTF16Count
    pendingFocusParagraphID = targetPrevID
    pendingCaretUTF16Offset = prevUTF16Count
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "이전 문단과 병합 완료"
    return targetPrevID
  }

  internal static func relinkParagraphAnchors(
    in doc: inout OfficeDocumentModel,
    from sourceParagraphID: UUID,
    to targetParagraphID: UUID
  ) {
    guard sourceParagraphID != targetParagraphID else { return }
    for imgIdx in doc.images.indices where doc.images[imgIdx].anchorParagraphID == sourceParagraphID
    {
      doc.images[imgIdx].anchorParagraphID = targetParagraphID
    }
    for fnIdx in doc.footnotes.indices
    where doc.footnotes[fnIdx].anchorParagraphID == sourceParagraphID {
      doc.footnotes[fnIdx].anchorParagraphID = targetParagraphID
    }
    for cmIdx in doc.comments.indices
    where doc.comments[cmIdx].anchorParagraphID == sourceParagraphID {
      doc.comments[cmIdx].anchorParagraphID = targetParagraphID
    }
    for revIdx in doc.revisions.indices where doc.revisions[revIdx].paragraphID == sourceParagraphID
    {
      doc.revisions[revIdx].paragraphID = targetParagraphID
    }
    for (bookmarkName, anchorID) in doc.bookmarks where anchorID == sourceParagraphID {
      doc.bookmarks[bookmarkName] = targetParagraphID
    }
  }

  public func selectAdjacentParagraph(from paragraphID: UUID, delta: Int) {
    guard let tab = activeTab,
      case .document(let doc) = tab.payload,
      let pIdx = doc.paragraphs.firstIndex(where: { $0.id == paragraphID })
    else { return }
    let nextIdx = max(0, min(doc.paragraphs.count - 1, pIdx + delta))
    let targetID = doc.paragraphs[nextIdx].id
    selectedParagraphID = targetID
    pendingFocusParagraphID = targetID
    pendingCaretUTF16Offset = 0
  }

  public func setActiveParagraphFont(fontName: String? = nil, fontSizePt: Double? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      !doc.paragraphs.isEmpty
    else { return }

    recordUndoSnapshot(for: activeTabID)
    let pIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
    if let fontName {
      let trimmed = fontName.trimmingCharacters(in: .whitespacesAndNewlines)
      if !trimmed.isEmpty {
        doc.paragraphs[pIdx].fontName = trimmed
        for rIdx in doc.paragraphs[pIdx].runs.indices {
          doc.paragraphs[pIdx].runs[rIdx].fontName = trimmed
        }
      }
    }
    if let fontSizePt, fontSizePt.isFinite {
      let clampedSize = max(6.0, min(96.0, fontSizePt))
      doc.paragraphs[pIdx].fontSizePt = clampedSize
      for rIdx in doc.paragraphs[pIdx].runs.indices {
        doc.paragraphs[pIdx].runs[rIdx].fontSizePt = clampedSize
      }
    }
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary =
      "문단 글꼴 설정 (\(doc.paragraphs[pIdx].fontName) \(Int(doc.paragraphs[pIdx].fontSizePt))pt)"
  }

  public func setActiveParagraphIndent(leftIndentPt: Double? = nil, firstLineIndentPt: Double? = nil)
  {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      !doc.paragraphs.isEmpty
    else { return }

    recordUndoSnapshot(
      for: activeTabID,
      coalescingTargetKey:
        "doc-indent-\(activeTabID.uuidString)-\(selectedParagraphID?.uuidString ?? "0")"
    )
    let pIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
    if let leftIndentPt, leftIndentPt.isFinite {
      doc.paragraphs[pIdx].leftIndentPt = max(0.0, min(144.0, leftIndentPt))
    }
    if let firstLineIndentPt, firstLineIndentPt.isFinite {
      doc.paragraphs[pIdx].firstLineIndentPt = max(0.0, min(144.0, firstLineIndentPt))
    }
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary =
      "눈금자 들여쓰기 설정 (좌측 \(Int(doc.paragraphs[pIdx].leftIndentPt))pt, 첫 줄 \(Int(doc.paragraphs[pIdx].firstLineIndentPt))pt)"
  }

  public func visibleDocumentOutlineParagraphs(for doc: OfficeDocumentModel) -> [ParagraphBlock] {
    var result: [ParagraphBlock] = []
    var collapsedHeadingLevel: Int? = nil

    func headingLevel(for style: ParagraphStyleRole) -> Int? {
      switch style {
      case .title, .officialHeader: return 0
      case .heading1: return 1
      case .heading2: return 2
      case .heading3: return 3
      case .body, .quote: return nil
      }
    }

    for paragraph in doc.paragraphs {
      let currentLevel = headingLevel(for: paragraph.style)
      if let activeCollapsedLevel = collapsedHeadingLevel {
        if let currentLevel, currentLevel <= activeCollapsedLevel {
          collapsedHeadingLevel = nil
        } else {
          continue
        }
      }
      result.append(paragraph)
      if let currentLevel, collapsedOutlineParagraphIDs.contains(paragraph.id) {
        collapsedHeadingLevel = currentLevel
      }
    }
    return result
  }

  public func formattedTableOfContentsEntryTitle(index: Int, rawTitle: String) -> String {
    let trimmed = rawTitle.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return "\(index + 1)." }
    let pattern = #"^\d+[\.\)]\s*"#
    if trimmed.range(of: pattern, options: .regularExpression) != nil {
      return trimmed
    }
    return "\(index + 1). \(trimmed)"
  }

  public func updateActiveDocumentOfficialHeaderText(_ text: String) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(
      for: activeTabID,
      coalescingTargetKey: "doc-official-header-\(activeTabID.uuidString)"
    )
    doc.headerText = text
    if let bannerIdx = doc.paragraphs.firstIndex(where: { $0.style == .officialHeader }) {
      doc.paragraphs[bannerIdx].text = text
      doc.paragraphs[bannerIdx].runs = []
    }
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "행정 공문서 두문 텍스트 수정"
  }

  public func updateActiveDocumentManuscriptFullText(_ fullText: String) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(
      for: activeTabID,
      coalescingTargetKey: "doc-manuscript-text-\(activeTabID.uuidString)"
    )
    let lines = fullText.replacingOccurrences(of: "\r\n", with: "\n").components(separatedBy: "\n")
    let oldParagraphs = doc.paragraphs
    if lines.isEmpty {
      let fallback = ParagraphBlock(style: .body, alignment: .leading, text: "")
      for oldP in oldParagraphs {
        Self.relinkParagraphAnchors(in: &doc, from: oldP.id, to: fallback.id)
      }
      doc.paragraphs = [fallback]
    } else {
      var prefixCount = 0
      while prefixCount < oldParagraphs.count
        && prefixCount < lines.count
        && oldParagraphs[prefixCount].text == lines[prefixCount]
      {
        prefixCount += 1
      }
      var suffixCount = 0
      while suffixCount < (oldParagraphs.count - prefixCount)
        && suffixCount < (lines.count - prefixCount)
        && oldParagraphs[oldParagraphs.count - 1 - suffixCount].text
          == lines[lines.count - 1 - suffixCount]
      {
        suffixCount += 1
      }

      var updatedParagraphs: [ParagraphBlock] = []
      if prefixCount > 0 {
        updatedParagraphs.append(contentsOf: oldParagraphs[0..<prefixCount])
      }

      let oldMiddleEnd = oldParagraphs.count - suffixCount
      let newMiddleEnd = lines.count - suffixCount
      let oldMiddle = Array(oldParagraphs[prefixCount..<oldMiddleEnd])
      let newMiddle = Array(lines[prefixCount..<newMiddleEnd])

      var consumedOldIndices = Set<Int>()
      var searchFloor = 0
      var matchedOldIndexForNew: [Int: Int] = [:]
      for (newIdx, line) in newMiddle.enumerated() {
        if let foundOldIdx = (searchFloor..<oldMiddle.count).first(where: {
          !consumedOldIndices.contains($0) && oldMiddle[$0].text == line
        }) {
          matchedOldIndexForNew[newIdx] = foundOldIdx
          consumedOldIndices.insert(foundOldIdx)
          searchFloor = foundOldIdx + 1
        }
      }

      var unconsumedSequentialIdx = 0
      for (newIdx, line) in newMiddle.enumerated() {
        if let matchedOldIdx = matchedOldIndexForNew[newIdx] {
          updatedParagraphs.append(oldMiddle[matchedOldIdx])
        } else {
          while unconsumedSequentialIdx < oldMiddle.count
            && consumedOldIndices.contains(unconsumedSequentialIdx)
          {
            unconsumedSequentialIdx += 1
          }
          if unconsumedSequentialIdx < oldMiddle.count {
            var mutated = oldMiddle[unconsumedSequentialIdx]
            consumedOldIndices.insert(unconsumedSequentialIdx)
            unconsumedSequentialIdx += 1
            mutated.text = line
            mutated.runs = []
            updatedParagraphs.append(mutated)
          } else {
            updatedParagraphs.append(ParagraphBlock(style: .body, alignment: .leading, text: line))
          }
        }
      }

      let suffixStart = oldParagraphs.count - suffixCount
      if suffixCount > 0 {
        updatedParagraphs.append(contentsOf: oldParagraphs[suffixStart..<oldParagraphs.count])
      }

      if !updatedParagraphs.isEmpty {
        let fallbackAnchorID: UUID =
          if prefixCount + newMiddle.count > 0 {
            updatedParagraphs[prefixCount + newMiddle.count - 1].id
          } else {
            updatedParagraphs[0].id
          }
        for (oldIdx, removedParagraph) in oldMiddle.enumerated()
        where !consumedOldIndices.contains(oldIdx) {
          Self.relinkParagraphAnchors(in: &doc, from: removedParagraph.id, to: fallbackAnchorID)
        }
      }

      doc.paragraphs = updatedParagraphs
    }
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "원고지 본문 텍스트 편집 반영"
  }
}

