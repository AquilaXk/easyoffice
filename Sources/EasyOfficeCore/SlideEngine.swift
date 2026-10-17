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

/// Visual element placed on a slide canvas.
public struct SlideCanvasElement: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var title: String
    public var bodyText: String
    public var x: Double
    public var y: Double
    public var width: Double
    public var height: Double

    public init(
        id: UUID = UUID(),
        title: String,
        bodyText: String = "",
        x: Double,
        y: Double,
        width: Double,
        height: Double
    ) {
        self.id = id
        self.title = title
        self.bodyText = bodyText
        self.x = x
        self.y = y
        self.width = width
        self.height = height
    }

    public var centerX: Double { x + width / 2.0 }
    public var centerY: Double { y + height / 2.0 }
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
