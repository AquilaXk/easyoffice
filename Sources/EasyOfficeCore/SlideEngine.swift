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
        self.fillHex = fillHex
        self.strokeHex = strokeHex
        self.isBold = isBold
        self.isItalic = isItalic
        self.isUnderlined = isUnderlined
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
            guides.append(AlignmentSnapGuide(axis: .vertical, positionPoints: stageCenterX, reason: "stage-center-x"))
        }
        if abs(movingElement.centerY - stageCenterY) <= tolerancePoints {
            guides.append(AlignmentSnapGuide(axis: .horizontal, positionPoints: stageCenterY, reason: "stage-center-y"))
        }

        // 2. Sibling edge and center alignment guides
        let otherSiblings = siblings.filter { $0.id != movingElement.id }
        let movingAnchorsX = [movingElement.x, movingElement.centerX, movingElement.maxX]
        let movingAnchorsY = [movingElement.y, movingElement.centerY, movingElement.maxY]

        for sibling in otherSiblings {
            let siblingAnchorsX = [sibling.x, sibling.centerX, sibling.maxX]
            for mX in movingAnchorsX {
                if let matchX = siblingAnchorsX.first(where: { abs(mX - $0) <= tolerancePoints }) {
                    let candidate = AlignmentSnapGuide(axis: .vertical, positionPoints: matchX, reason: "element-edge")
                    if !guides.contains(candidate) {
                        guides.append(candidate)
                    }
                }
            }

            let siblingAnchorsY = [sibling.y, sibling.centerY, sibling.maxY]
            for mY in movingAnchorsY {
                if let matchY = siblingAnchorsY.first(where: { abs(mY - $0) <= tolerancePoints }) {
                    let candidate = AlignmentSnapGuide(axis: .horizontal, positionPoints: matchY, reason: "element-edge")
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
                let involvesMoving = first.id == movingElement.id || middle.id == movingElement.id || last.id == movingElement.id
                guard involvesMoving else { continue }

                let gap1 = middle.x - first.maxX
                let gap2 = last.x - middle.maxX
                if gap1 >= 0, gap2 >= 0, abs(gap1 - gap2) <= tolerancePoints {
                    let candidate = AlignmentSnapGuide(axis: .vertical, positionPoints: movingElement.centerX, reason: "equal-spacing")
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
                let involvesMoving = first.id == movingElement.id || middle.id == movingElement.id || last.id == movingElement.id
                guard involvesMoving else { continue }

                let gap1 = middle.y - first.maxY
                let gap2 = last.y - middle.maxY
                if gap1 >= 0, gap2 >= 0, abs(gap1 - gap2) <= tolerancePoints {
                    let candidate = AlignmentSnapGuide(axis: .horizontal, positionPoints: movingElement.centerY, reason: "equal-spacing")
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

    public init(
        id: UUID = UUID(),
        title: String,
        subtitle: String = "",
        bulletPoints: [String] = [],
        speakerNotes: String = "",
        masterLayout: SlideMasterLayout = .titleAndContent,
        transition: SlideTransitionKind = .fade,
        elements: [SlideCanvasElement] = []
    ) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.bulletPoints = bulletPoints
        self.speakerNotes = speakerNotes
        self.masterLayout = masterLayout
        self.transition = transition
        self.elements = elements
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
}

/// Presentation deck model supporting slide ordering, smart alignment guides, and presenter notes.
public struct PresentationDeckModel: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var title: String
    public var aspectRatio: SlideAspectRatio
    public var slides: [SlideModel]

    public init(
        id: UUID = UUID(),
        title: String,
        aspectRatio: SlideAspectRatio = .widescreen16x9,
        slides: [SlideModel] = []
    ) {
        self.id = id
        self.title = title
        self.aspectRatio = aspectRatio
        self.slides = slides
    }

    public mutating func moveSlide(from sourceIndex: Int, to destinationIndex: Int) {
        guard slides.indices.contains(sourceIndex),
              destinationIndex >= 0,
              destinationIndex < slides.count else {
            return
        }
        let item = slides.remove(at: sourceIndex)
        slides.insert(item, at: destinationIndex)
    }

    /// Computes whether an element's center snaps to the slide canvas horizontal/vertical center within `tolerancePoints`.
    public func isCenteredOnStage(_ element: SlideCanvasElement, tolerancePoints: Double = 4.0) -> (horizontal: Bool, vertical: Bool) {
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
        self.currentSlideIndex = deck.slides.isEmpty ? 0 : max(0, min(currentSlideIndex, deck.slides.count - 1))
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
