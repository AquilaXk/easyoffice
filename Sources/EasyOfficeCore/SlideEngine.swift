import Foundation

/// Presentation slide aspect ratio presets.
public enum SlideAspectRatio: String, CaseIterable, Sendable, Codable {
  case widescreen16x9 = "16:9 와이드스크린"
  case standard4x3 = "4:3 표준"
  case handoutA4 = "A4 유인물"

  public var widthPoints: Double {
    switch self {
    case .widescreen16x9: return 960.0
    case .standard4x3: return 720.0
    case .handoutA4: return 841.89
    }
  }

  public var heightPoints: Double {
    switch self {
    case .widescreen16x9: return 540.0
    case .standard4x3: return 540.0
    case .handoutA4: return 595.28
    }
  }
}

/// Master layout archetypes for slides.
public enum SlideMasterLayout: String, CaseIterable, Sendable, Codable {
  case titleSlide = "제목 슬라이드"
  case titleAndContent = "제목 및 내용"
  case sectionHeader = "구역 머리글"
  case twoColumnComparison = "2단 비교"
  case blankCanvas = "빈 화면"
}

/// Transition effect between slides.
public enum SlideTransitionKind: String, CaseIterable, Sendable, Codable {
  case none = "없음"
  case fade = "페이드 (Fade)"
  case push = "밀어내기 (Push)"
  case morph = "모핑 (Morph)"
}

/// Shape or content archetype for a slide canvas element.
public enum SlideElementKind: String, CaseIterable, Sendable, Codable {
  case textBox = "텍스트 상자"
  case rectangleShape = "사각형 도형"
  case roundedRectangleShape = "둥근 사각형 도형"
  case ellipseShape = "타원 도형"
  case arrowShape = "화살표 도형"
  case calloutShape = "말풍선 도형"
  case tableGrid = "표 그리드"
  case chartEmbed = "차트 개체"
}

public enum SlideElementAnimation: String, Sendable, Codable, Equatable, CaseIterable {
  case none = "없음"
  case fadeIn = "페이드 인 (Fade In)"
  case flyInUp = "아래에서 날아오기 (Fly In)"
  case zoomIn = "확대 등장 (Zoom In)"
}

public enum SlideResizeHandle: String, Sendable, Codable, Equatable, CaseIterable {
  case topLeft = "NW"
  case top = "N"
  case topRight = "NE"
  case right = "E"
  case bottomRight = "SE"
  case bottom = "S"
  case bottomLeft = "SW"
  case left = "W"
}

/// Master color theme palette applied across presentation slides and canvas elements.
public enum SlideDeckTheme: String, Sendable, Codable, Equatable, CaseIterable {
  case signatureLavender = "시그니처 라벤더"
  case corporateNavy = "코퍼레이트 네이비"
  case forestEmerald = "포레스트 에메랄드"
  case warmAmber = "웜 앰버"
  case slateMono = "슬레이트 모노"

  public var primaryFillHex: String {
    switch self {
    case .signatureLavender: return "#EEF0FF"
    case .corporateNavy: return "#E8EEF7"
    case .forestEmerald: return "#E6F4EA"
    case .warmAmber: return "#FEF3C7"
    case .slateMono: return "#F1F5F9"
    }
  }

  public var accentBorderHex: String {
    switch self {
    case .signatureLavender: return "#5C6BC0"
    case .corporateNavy: return "#1E40AF"
    case .forestEmerald: return "#047857"
    case .warmAmber: return "#D97706"
    case .slateMono: return "#334155"
    }
  }
}

/// Multi-element and stage-relative alignment & distribution commands on a slide canvas.
public enum SlideElementAlignment: String, Sendable, Codable, Equatable, CaseIterable {
  case alignLeft = "좌측 맞춤"
  case alignCenter = "가로 중앙 맞춤"
  case alignRight = "우측 맞춤"
  case alignTop = "상단 맞춤"
  case alignMiddle = "세로 중앙 맞춤"
  case alignBottom = "하단 맞춤"
  case distributeHorizontally = "가로 균등 배분"
  case distributeVertically = "세로 균등 배분"
}

/// Visual element placed on a slide canvas.
public struct SlideCanvasElement: Sendable, Codable, Equatable, Identifiable {
  public let id: UUID
  public var title: String
  public var bodyText: String
  public var x: Double
  public var y: Double
  public var width: Double
  public var height: Double
  public var kind: SlideElementKind
  public var zIndex: Int
  public var rotationDegrees: Double
  public var animation: SlideElementAnimation
  public var fillHex: String
  public var strokeHex: String
  public var isBold: Bool
  public var isItalic: Bool
  public var isUnderlined: Bool

  public init(
    id: UUID = UUID(),
    title: String,
    bodyText: String = "",
    x: Double,
    y: Double,
    width: Double,
    height: Double,
    kind: SlideElementKind = .textBox,
    zIndex: Int = 0,
    rotationDegrees: Double = 0.0,
    animation: SlideElementAnimation = .none,
    fillHex: String = "#EEF0FF",
    strokeHex: String = "#5C6BC0",
    isBold: Bool = false,
    isItalic: Bool = false,
    isUnderlined: Bool = false
  ) {
    self.id = id
    self.title = title
    self.bodyText = bodyText
    self.x = x
    self.y = y
    self.width = width
    self.height = height
    self.kind = kind
    self.zIndex = zIndex
    self.rotationDegrees = rotationDegrees
    self.animation = animation
    self.fillHex = fillHex
    self.strokeHex = strokeHex
    self.isBold = isBold
    self.isItalic = isItalic
    self.isUnderlined = isUnderlined
  }

  private enum CodingKeys: String, CodingKey {
    case id, title, bodyText, x, y, width, height, kind, zIndex
    case rotationDegrees, animation, fillHex, strokeHex, isBold, isItalic, isUnderlined
  }

  public init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    self.id = try c.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
    self.title = try c.decode(String.self, forKey: .title)
    self.bodyText = try c.decodeIfPresent(String.self, forKey: .bodyText) ?? ""
    self.x = try c.decode(Double.self, forKey: .x)
    self.y = try c.decode(Double.self, forKey: .y)
    self.width = try c.decode(Double.self, forKey: .width)
    self.height = try c.decode(Double.self, forKey: .height)
    self.kind = try c.decodeIfPresent(SlideElementKind.self, forKey: .kind) ?? .textBox
    self.zIndex = try c.decodeIfPresent(Int.self, forKey: .zIndex) ?? 0
    self.rotationDegrees = try c.decodeIfPresent(Double.self, forKey: .rotationDegrees) ?? 0.0
    self.animation = try c.decodeIfPresent(SlideElementAnimation.self, forKey: .animation) ?? .none
    self.fillHex = try c.decodeIfPresent(String.self, forKey: .fillHex) ?? "#EEF0FF"
    self.strokeHex = try c.decodeIfPresent(String.self, forKey: .strokeHex) ?? "#5C6BC0"
    self.isBold = try c.decodeIfPresent(Bool.self, forKey: .isBold) ?? false
    self.isItalic = try c.decodeIfPresent(Bool.self, forKey: .isItalic) ?? false
    self.isUnderlined = try c.decodeIfPresent(Bool.self, forKey: .isUnderlined) ?? false
  }

  public func encode(to encoder: Encoder) throws {
    var c = encoder.container(keyedBy: CodingKeys.self)
    try c.encode(id, forKey: .id)
    try c.encode(title, forKey: .title)
    try c.encode(bodyText, forKey: .bodyText)
    try c.encode(x, forKey: .x)
    try c.encode(y, forKey: .y)
    try c.encode(width, forKey: .width)
    try c.encode(height, forKey: .height)
    try c.encode(kind, forKey: .kind)
    try c.encode(zIndex, forKey: .zIndex)
    try c.encode(rotationDegrees, forKey: .rotationDegrees)
    try c.encode(animation, forKey: .animation)
    try c.encode(fillHex, forKey: .fillHex)
    try c.encode(strokeHex, forKey: .strokeHex)
    try c.encode(isBold, forKey: .isBold)
    try c.encode(isItalic, forKey: .isItalic)
    try c.encode(isUnderlined, forKey: .isUnderlined)
  }

  public var body: String {
    get { bodyText }
    set { bodyText = newValue }
  }

  public var centerX: Double { x + width / 2.0 }
  public var centerY: Double { y + height / 2.0 }
  public var maxX: Double { x + width }
  public var maxY: Double { y + height }
}

/// Axis of an alignment snap guide on the slide canvas.
public enum SnapAxis: String, CaseIterable, Sendable, Codable {
  case vertical
  case horizontal
}

/// Active smart alignment guide emitted when dragging or resizing a slide element.
public struct AlignmentSnapGuide: Sendable, Codable, Equatable {
  public let axis: SnapAxis
  public let positionPoints: Double
  public let reason: String

  public init(axis: SnapAxis, positionPoints: Double, reason: String) {
    self.axis = axis
    self.positionPoints = positionPoints
    self.reason = reason
  }
}

/// Calculator for stage-center, sibling-edge/center, and equal-spacing alignment snap guides.
public struct SnapGuideCalculator: Sendable {
  public static func computeGuides(
    for movingElement: SlideCanvasElement,
    siblings: [SlideCanvasElement],
    aspectRatio: SlideAspectRatio,
    tolerancePoints: Double = 4.0
  ) -> [AlignmentSnapGuide] {
    var guides: [AlignmentSnapGuide] = []

    // 1. Stage center guides
    let stageCenterX = aspectRatio.widthPoints / 2.0
    let stageCenterY = aspectRatio.heightPoints / 2.0
    if abs(movingElement.centerX - stageCenterX) <= tolerancePoints {
      guides.append(
        AlignmentSnapGuide(axis: .vertical, positionPoints: stageCenterX, reason: "stage-center-x"))
    }
    if abs(movingElement.centerY - stageCenterY) <= tolerancePoints {
      guides.append(
        AlignmentSnapGuide(
          axis: .horizontal, positionPoints: stageCenterY, reason: "stage-center-y"))
    }

    // 2. Sibling edge and center alignment guides
    let otherSiblings = siblings.filter { $0.id != movingElement.id }
    let movingAnchorsX = [movingElement.x, movingElement.centerX, movingElement.maxX]
    let movingAnchorsY = [movingElement.y, movingElement.centerY, movingElement.maxY]

    for sibling in otherSiblings {
      let siblingAnchorsX = [sibling.x, sibling.centerX, sibling.maxX]
      for mX in movingAnchorsX {
        if let matchX = siblingAnchorsX.first(where: { abs(mX - $0) <= tolerancePoints }) {
          let candidate = AlignmentSnapGuide(
            axis: .vertical, positionPoints: matchX, reason: "element-edge")
          if !guides.contains(candidate) {
            guides.append(candidate)
          }
        }
      }

      let siblingAnchorsY = [sibling.y, sibling.centerY, sibling.maxY]
      for mY in movingAnchorsY {
        if let matchY = siblingAnchorsY.first(where: { abs(mY - $0) <= tolerancePoints }) {
          let candidate = AlignmentSnapGuide(
            axis: .horizontal, positionPoints: matchY, reason: "element-edge")
          if !guides.contains(candidate) {
            guides.append(candidate)
          }
        }
      }
    }

    // 3. Equal spacing guides across 3+ elements
    let allElements = otherSiblings + [movingElement]
    if allElements.count >= 3 {
      let sortedByX = allElements.sorted { $0.x < $1.x }
      for i in 0..<(sortedByX.count - 2) {
        let first = sortedByX[i]
        let middle = sortedByX[i + 1]
        let last = sortedByX[i + 2]
        let involvesMoving =
          first.id == movingElement.id || middle.id == movingElement.id
          || last.id == movingElement.id
        guard involvesMoving else { continue }

        let gap1 = middle.x - first.maxX
        let gap2 = last.x - middle.maxX
        if gap1 >= 0, gap2 >= 0, abs(gap1 - gap2) <= tolerancePoints {
          let candidate = AlignmentSnapGuide(
            axis: .vertical, positionPoints: movingElement.centerX, reason: "equal-spacing")
          if !guides.contains(candidate) {
            guides.append(candidate)
          }
        }
      }

      let sortedByY = allElements.sorted { $0.y < $1.y }
      for i in 0..<(sortedByY.count - 2) {
        let first = sortedByY[i]
        let middle = sortedByY[i + 1]
        let last = sortedByY[i + 2]
        let involvesMoving =
          first.id == movingElement.id || middle.id == movingElement.id
          || last.id == movingElement.id
        guard involvesMoving else { continue }

        let gap1 = middle.y - first.maxY
        let gap2 = last.y - middle.maxY
        if gap1 >= 0, gap2 >= 0, abs(gap1 - gap2) <= tolerancePoints {
          let candidate = AlignmentSnapGuide(
            axis: .horizontal, positionPoints: movingElement.centerY, reason: "equal-spacing")
          if !guides.contains(candidate) {
            guides.append(candidate)
          }
        }
      }
    }

    return guides
  }
}

/// Single slide inside a presentation deck.
public struct SlideModel: Sendable, Codable, Equatable, Identifiable {
  public let id: UUID
  public var title: String
  public var subtitle: String
  public var bulletPoints: [String]
  public var speakerNotes: String
  public var masterLayout: SlideMasterLayout
  public var transition: SlideTransitionKind
  public var elements: [SlideCanvasElement]
  public var transitionDurationSeconds: Double {
    didSet {
      let clamped = max(
        0.2,
        min(5.0, transitionDurationSeconds.isFinite ? transitionDurationSeconds : 0.8)
      )
      if transitionDurationSeconds != clamped {
        transitionDurationSeconds = clamped
      }
    }
  }
  public var autoAdvanceSeconds: Int? {
    didSet {
      let clamped = autoAdvanceSeconds.map { max(1, min(300, $0)) }
      if autoAdvanceSeconds != clamped {
        autoAdvanceSeconds = clamped
      }
    }
  }

  public var bodyBullets: [String] {
    get { bulletPoints }
    set { bulletPoints = newValue }
  }

  public init(
    id: UUID = UUID(),
    title: String,
    subtitle: String = "",
    bulletPoints: [String] = [],
    speakerNotes: String = "",
    masterLayout: SlideMasterLayout = .titleAndContent,
    transition: SlideTransitionKind = .fade,
    elements: [SlideCanvasElement] = [],
    transitionDurationSeconds: Double = 0.8,
    autoAdvanceSeconds: Int? = nil
  ) {
    self.id = id
    self.title = title
    self.subtitle = subtitle
    self.bulletPoints = bulletPoints
    self.speakerNotes = speakerNotes
    self.masterLayout = masterLayout
    self.transition = transition
    self.elements = elements
    self.transitionDurationSeconds = max(
      0.2,
      min(5.0, transitionDurationSeconds.isFinite ? transitionDurationSeconds : 0.8)
    )
    self.autoAdvanceSeconds = autoAdvanceSeconds.map { max(1, min(300, $0)) }
  }

  private enum CodingKeys: String, CodingKey {
    case id, title, subtitle, bulletPoints, speakerNotes, masterLayout, transition, elements
    case transitionDurationSeconds, autoAdvanceSeconds
  }

  public init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    self.id = try c.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
    self.title = try c.decode(String.self, forKey: .title)
    self.subtitle = try c.decodeIfPresent(String.self, forKey: .subtitle) ?? ""
    self.bulletPoints = try c.decodeIfPresent([String].self, forKey: .bulletPoints) ?? []
    self.speakerNotes = try c.decodeIfPresent(String.self, forKey: .speakerNotes) ?? ""
    self.masterLayout =
      try c.decodeIfPresent(SlideMasterLayout.self, forKey: .masterLayout) ?? .titleAndContent
    self.transition = try c.decodeIfPresent(SlideTransitionKind.self, forKey: .transition) ?? .fade
    self.elements = try c.decodeIfPresent([SlideCanvasElement].self, forKey: .elements) ?? []
    let rawDuration = try c.decodeIfPresent(Double.self, forKey: .transitionDurationSeconds) ?? 0.8
    self.transitionDurationSeconds = max(
      0.2,
      min(5.0, rawDuration.isFinite ? rawDuration : 0.8)
    )
    let rawAutoAdvance = try c.decodeIfPresent(Int.self, forKey: .autoAdvanceSeconds)
    self.autoAdvanceSeconds = rawAutoAdvance.map { max(1, min(300, $0)) }
  }

  public func encode(to encoder: Encoder) throws {
    var c = encoder.container(keyedBy: CodingKeys.self)
    try c.encode(id, forKey: .id)
    try c.encode(title, forKey: .title)
    try c.encode(subtitle, forKey: .subtitle)
    try c.encode(bulletPoints, forKey: .bulletPoints)
    try c.encode(speakerNotes, forKey: .speakerNotes)
    try c.encode(masterLayout, forKey: .masterLayout)
    try c.encode(transition, forKey: .transition)
    try c.encode(elements, forKey: .elements)
    try c.encode(transitionDurationSeconds, forKey: .transitionDurationSeconds)
    try c.encodeIfPresent(autoAdvanceSeconds, forKey: .autoAdvanceSeconds)
  }

  /// Moves the target element to the top of the z-order stack.
  public mutating func bringElementToFront(id: UUID) {
    guard let idx = elements.firstIndex(where: { $0.id == id }) else { return }
    let maxZ = elements.map(\.zIndex).max() ?? 0
    var target = elements.remove(at: idx)
    target.zIndex = maxZ + 1
    elements.append(target)
  }

  /// Moves the target element to the bottom of the z-order stack.
  public mutating func sendElementToBack(id: UUID) {
    guard let idx = elements.firstIndex(where: { $0.id == id }) else { return }
    let minZ = elements.map(\.zIndex).min() ?? 0
    var target = elements.remove(at: idx)
    target.zIndex = minZ - 1
    elements.insert(target, at: 0)
  }

  public mutating func resizeElement(
    id: UUID,
    handle: SlideResizeHandle,
    deltaX: Double,
    deltaY: Double,
    stageSize: (width: Double, height: Double) = (960.0, 540.0)
  ) {
    guard deltaX.isFinite, deltaY.isFinite, stageSize.width.isFinite, stageSize.height.isFinite,
      let idx = elements.firstIndex(where: { $0.id == id })
    else { return }
    var el = elements[idx]
    let clampedLeftDX = max(-el.x, min(deltaX, el.width - 40.0))
    let clampedTopDY = max(-el.y, min(deltaY, el.height - 24.0))
    switch handle {
    case .topLeft:
      el.x += clampedLeftDX
      el.width -= clampedLeftDX
      el.y += clampedTopDY
      el.height -= clampedTopDY
    case .top:
      el.y += clampedTopDY
      el.height -= clampedTopDY
    case .topRight:
      el.y += clampedTopDY
      el.height -= clampedTopDY
      el.width += deltaX
    case .right:
      el.width += deltaX
    case .bottomRight:
      el.width += deltaX
      el.height += deltaY
    case .bottom:
      el.height += deltaY
    case .bottomLeft:
      el.x += clampedLeftDX
      el.width -= clampedLeftDX
      el.height += deltaY
    case .left:
      el.x += clampedLeftDX
      el.width -= clampedLeftDX
    }
    el.x = max(0.0, min(el.x, max(0.0, stageSize.width - 40.0)))
    el.y = max(0.0, min(el.y, max(0.0, stageSize.height - 24.0)))
    el.width = max(40.0, min(el.width, max(40.0, stageSize.width - el.x)))
    el.height = max(24.0, min(el.height, max(24.0, stageSize.height - el.y)))
    elements[idx] = el
  }

  public mutating func rotateElement(id: UUID, toDegrees degrees: Double) {
    guard degrees.isFinite, let idx = elements.firstIndex(where: { $0.id == id }) else { return }
    let normalized = ((degrees.truncatingRemainder(dividingBy: 360.0)) + 360.0).truncatingRemainder(
      dividingBy: 360.0)
    elements[idx].rotationDegrees = normalized
  }

  /// Duplicates an existing canvas element into a side-by-side column when horizontal stage space permits, or with a 24pt stage-clamped offset, at top zIndex.
  @discardableResult
  public mutating func duplicateElement(
    id: UUID,
    stageSize: (width: Double, height: Double) = (960.0, 540.0)
  ) -> SlideCanvasElement? {
    guard let elem = elements.first(where: { $0.id == id }) else { return nil }
    let maxZ = elements.map(\.zIndex).max() ?? 0
    let horizontalCandidateX = elem.x + elem.width + 24.0
    let fitsBesideHorizontally = horizontalCandidateX + elem.width <= stageSize.width - 24.0
    let newX =
      fitsBesideHorizontally
      ? max(0.0, horizontalCandidateX)
      : max(0.0, min(stageSize.width - elem.width, elem.x + 24.0))
    let newY =
      fitsBesideHorizontally
      ? elem.y
      : max(0.0, min(stageSize.height - elem.height, elem.y + 24.0))
    let isDefaultPrimaryCard = (elem.title == "1분기 핵심 과제" || elem.title == "핵심 가치 배너")
    let copyTitle = isDefaultPrimaryCard ? "2분기 실행 지표" : elem.title
    let copyBodyText =
      isDefaultPrimaryCard
      ? "자동 집계 시트 전환율 95% 달성 및 문서 검토 리드타임 단축"
      : elem.bodyText
    let copy = SlideCanvasElement(
      id: UUID(),
      title: copyTitle,
      bodyText: copyBodyText,
      x: newX,
      y: newY,
      width: elem.width,
      height: elem.height,
      kind: elem.kind,
      zIndex: maxZ + 1,
      rotationDegrees: elem.rotationDegrees,
      animation: elem.animation,
      fillHex: elem.fillHex,
      strokeHex: elem.strokeHex,
      isBold: elem.isBold,
      isItalic: elem.isItalic,
      isUnderlined: elem.isUnderlined
    )
    elements.append(copy)
    return copy
  }

  /// Removes the canvas element with the given `id`.
  public mutating func removeElement(id: UUID) {
    elements.removeAll { $0.id == id }
  }

  /// Aligns or evenly distributes canvas elements relative to the stage or collective bounding box.
  public mutating func alignElements(
    _ alignment: SlideElementAlignment,
    targetID: UUID? = nil,
    stageSize: (width: Double, height: Double) = (960.0, 540.0)
  ) {
    guard !elements.isEmpty else { return }

    switch alignment {
    case .distributeHorizontally:
      guard elements.count >= 2 else { return }
      let sortedIndices = elements.indices.sorted { elements[$0].x < elements[$1].x }
      let totalWidths = sortedIndices.reduce(0.0) { $0 + elements[$1].width }
      if elements.count == 2 {
        let gap = max(24.0, (stageSize.width - totalWidths) / 3.0)
        elements[sortedIndices[0]].x = gap
        elements[sortedIndices[1]].x = gap + elements[sortedIndices[0]].width + gap
        return
      }
      let firstX = elements[sortedIndices[0]].x
      let lastMaxX = elements[sortedIndices[sortedIndices.count - 1]].maxX
      let gap = (lastMaxX - firstX - totalWidths) / Double(sortedIndices.count - 1)
      var cursorX = firstX
      for idx in sortedIndices {
        elements[idx].x = cursorX
        cursorX += elements[idx].width + gap
      }
      return

    case .distributeVertically:
      guard elements.count >= 2 else { return }
      let sortedIndices = elements.indices.sorted { elements[$0].y < elements[$1].y }
      let totalHeights = sortedIndices.reduce(0.0) { $0 + elements[$1].height }
      if elements.count == 2 {
        let lowerStageTop = 240.0
        let availableHeight = max(totalHeights + 24.0, stageSize.height - lowerStageTop)
        let gap = max(16.0, (availableHeight - totalHeights) / 3.0)
        elements[sortedIndices[0]].y = lowerStageTop + gap
        elements[sortedIndices[1]].y = lowerStageTop + gap + elements[sortedIndices[0]].height + gap
        return
      }
      let firstY = elements[sortedIndices[0]].y
      let lastMaxY = elements[sortedIndices[sortedIndices.count - 1]].maxY
      let gap = (lastMaxY - firstY - totalHeights) / Double(sortedIndices.count - 1)
      var cursorY = firstY
      for idx in sortedIndices {
        elements[idx].y = cursorY
        cursorY += elements[idx].height + gap
      }
      return

    default:
      break
    }

    if let targetID, let idx = elements.firstIndex(where: { $0.id == targetID }) {
      alignSingleElement(at: idx, alignment: alignment, stageSize: stageSize)
      return
    }

    if elements.count == 1 {
      alignSingleElement(at: 0, alignment: alignment, stageSize: stageSize)
      return
    }

    let minX = elements.map(\.x).min() ?? 0.0
    let maxX = elements.map(\.maxX).max() ?? stageSize.width
    let midX = (minX + maxX) / 2.0
    let minY = elements.map(\.y).min() ?? 0.0
    let maxY = elements.map(\.maxY).max() ?? stageSize.height
    let midY = (minY + maxY) / 2.0

    for idx in elements.indices {
      switch alignment {
      case .alignLeft:
        elements[idx].x = minX
      case .alignCenter:
        elements[idx].x = midX - elements[idx].width / 2.0
      case .alignRight:
        elements[idx].x = maxX - elements[idx].width
      case .alignTop:
        elements[idx].y = minY
      case .alignMiddle:
        elements[idx].y = midY - elements[idx].height / 2.0
      case .alignBottom:
        elements[idx].y = maxY - elements[idx].height
      case .distributeHorizontally, .distributeVertically:
        break
      }
    }
  }

  private mutating func alignSingleElement(
    at idx: Int,
    alignment: SlideElementAlignment,
    stageSize: (width: Double, height: Double)
  ) {
    switch alignment {
    case .alignLeft:
      elements[idx].x = 40.0
    case .alignCenter:
      elements[idx].x = max(0.0, (stageSize.width - elements[idx].width) / 2.0)
    case .alignRight:
      elements[idx].x = max(0.0, stageSize.width - elements[idx].width - 40.0)
    case .alignTop:
      elements[idx].y = 40.0
    case .alignMiddle:
      elements[idx].y = max(0.0, (stageSize.height - elements[idx].height) / 2.0)
    case .alignBottom:
      elements[idx].y = max(0.0, stageSize.height - elements[idx].height - 40.0)
    case .distributeHorizontally, .distributeVertically:
      break
    }
  }
}

public typealias SlideDeckModel = PresentationDeckModel

/// Presentation deck model supporting slide ordering, smart alignment guides, and presenter notes.
public struct PresentationDeckModel: Sendable, Codable, Equatable, Identifiable {
  public let id: UUID
  public var title: String
  public var aspectRatio: SlideAspectRatio
  public var slides: [SlideModel]
  public var deckTheme: SlideDeckTheme
  public var preservedArchiveEntries: [String: Data] = [:]

  public var theme: SlideDeckTheme {
    get { deckTheme }
    set { deckTheme = newValue }
  }

  public init(
    id: UUID = UUID(),
    title: String,
    aspectRatio: SlideAspectRatio = .widescreen16x9,
    slides: [SlideModel] = [],
    deckTheme: SlideDeckTheme = .signatureLavender
  ) {
    self.id = id
    self.title = title
    self.aspectRatio = aspectRatio
    self.slides = slides
    self.deckTheme = deckTheme
  }

  private enum CodingKeys: String, CodingKey {
    case id, title, aspectRatio, slides, deckTheme
  }

  public init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    self.id = try c.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
    self.title = try c.decode(String.self, forKey: .title)
    self.aspectRatio =
      try c.decodeIfPresent(SlideAspectRatio.self, forKey: .aspectRatio) ?? .widescreen16x9
    self.slides = try c.decodeIfPresent([SlideModel].self, forKey: .slides) ?? []
    self.deckTheme =
      try c.decodeIfPresent(SlideDeckTheme.self, forKey: .deckTheme) ?? .signatureLavender
  }

  public func encode(to encoder: Encoder) throws {
    var c = encoder.container(keyedBy: CodingKeys.self)
    try c.encode(id, forKey: .id)
    try c.encode(title, forKey: .title)
    try c.encode(aspectRatio, forKey: .aspectRatio)
    try c.encode(slides, forKey: .slides)
    try c.encode(deckTheme, forKey: .deckTheme)
  }

  /// Applies a master color theme across the presentation deck and all slide canvas elements.
  public mutating func applyTheme(_ theme: SlideDeckTheme) {
    self.deckTheme = theme
    for sIdx in slides.indices {
      for eIdx in slides[sIdx].elements.indices {
        slides[sIdx].elements[eIdx].fillHex = theme.primaryFillHex
        slides[sIdx].elements[eIdx].strokeHex = theme.accentBorderHex
      }
    }
  }

  public mutating func moveSlide(from sourceIndex: Int, to destinationIndex: Int) {
    guard slides.indices.contains(sourceIndex),
      destinationIndex >= 0,
      destinationIndex < slides.count
    else {
      return
    }
    let item = slides.remove(at: sourceIndex)
    slides.insert(item, at: destinationIndex)
  }

  /// Computes whether an element's center snaps to the slide canvas horizontal/vertical center within `tolerancePoints`.
  public func isCenteredOnStage(_ element: SlideCanvasElement, tolerancePoints: Double = 4.0) -> (
    horizontal: Bool, vertical: Bool
  ) {
    let stageCenterX = aspectRatio.widthPoints / 2.0
    let stageCenterY = aspectRatio.heightPoints / 2.0
    let hAligned = abs(element.centerX - stageCenterX) <= tolerancePoints
    let vAligned = abs(element.centerY - stageCenterY) <= tolerancePoints
    return (hAligned, vAligned)
  }
}

/// Presenter session state tracking current/next slide preview, speaker notes, progress fraction, and elapsed timer.
public struct PresenterSession: Sendable, Codable, Equatable {
  public var deck: PresentationDeckModel
  public var currentSlideIndex: Int
  public var elapsedSeconds: Int
  public var isTimerRunning: Bool

  public init(
    deck: PresentationDeckModel,
    currentSlideIndex: Int = 0,
    elapsedSeconds: Int = 0,
    isTimerRunning: Bool = true
  ) {
    self.deck = deck
    self.currentSlideIndex =
      deck.slides.isEmpty ? 0 : max(0, min(currentSlideIndex, deck.slides.count - 1))
    self.elapsedSeconds = max(0, elapsedSeconds)
    self.isTimerRunning = isTimerRunning
  }

  public var currentSlide: SlideModel? {
    guard deck.slides.indices.contains(currentSlideIndex) else { return nil }
    return deck.slides[currentSlideIndex]
  }

  public var nextSlide: SlideModel? {
    let nextIndex = currentSlideIndex + 1
    guard deck.slides.indices.contains(nextIndex) else { return nil }
    return deck.slides[nextIndex]
  }

  public var currentSpeakerNotes: String {
    currentSlide?.speakerNotes ?? ""
  }

  public var progressFraction: Double {
    guard !deck.slides.isEmpty else { return 0.0 }
    let clamped = max(0, min(currentSlideIndex, deck.slides.count - 1))
    return Double(clamped + 1) / Double(deck.slides.count)
  }

  public mutating func next() {
    guard currentSlideIndex + 1 < deck.slides.count else { return }
    currentSlideIndex += 1
  }

  public mutating func previous() {
    guard currentSlideIndex > 0 else { return }
    currentSlideIndex -= 1
  }

  public mutating func jump(to index: Int) {
    guard deck.slides.indices.contains(index) else { return }
    currentSlideIndex = index
  }

  public mutating func tick(seconds: Int = 1) {
    guard isTimerRunning, seconds > 0 else { return }
    elapsedSeconds += seconds
  }
}
