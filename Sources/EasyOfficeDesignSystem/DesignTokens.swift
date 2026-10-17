import Foundation
import SwiftUI

/// RGB color representation with deterministic hex conversion and WCAG 2.1 contrast calculation.
public struct RGBColorToken: Sendable, Equatable, Hashable {
    public let red: UInt8
    public let green: UInt8
    public let blue: UInt8
    public let hex: String

    public init(red: UInt8, green: UInt8, blue: UInt8, hex: String) {
        self.red = red
        self.green = green
        self.blue = blue
        self.hex = hex.uppercased()
    }

    public var swiftUIColor: Color {
        Color(
            .sRGB,
            red: Double(red) / 255.0,
            green: Double(green) / 255.0,
            blue: Double(blue) / 255.0,
            opacity: 1.0
        )
    }

    /// Relative luminance per WCAG 2.1 specification.
    public var relativeLuminance: Double {
        func linearize(_ channel: UInt8) -> Double {
            let sRGB = Double(channel) / 255.0
            return sRGB <= 0.03928 ? (sRGB / 12.92) : pow((sRGB + 0.055) / 1.055, 2.4)
        }
        let r = linearize(red)
        let g = linearize(green)
        let b = linearize(blue)
        return 0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// WCAG 2.1 contrast ratio against another RGB color token (1.0 ... 21.0).
    public func contrastRatio(against other: RGBColorToken) -> Double {
        let l1 = max(self.relativeLuminance, other.relativeLuminance)
        let l2 = min(self.relativeLuminance, other.relativeLuminance)
        return (l1 + 0.05) / (l2 + 0.05)
    }
}

/// Five core office modules visually distinguished in tabs, badges, and status bars.
public enum OfficeModuleKind: String, CaseIterable, Sendable, Codable, Identifiable {
    case document = "document"
    case hwp = "hwp"
    case sheet = "sheet"
    case slide = "slide"
    case pdf = "pdf"

    public var id: String { rawValue }

    public var displayNameKo: String {
        switch self {
        case .document: return "문서 (Document)"
        case .hwp: return "한글 문서 (HWP/HWPX)"
        case .sheet: return "시트 (Sheet)"
        case .slide: return "슬라이드 (Slide)"
        case .pdf: return "PDF 스튜디오 (PDF)"
        }
    }

    public var shortBadgeTitle: String {
        switch self {
        case .document: return "DOC"
        case .hwp: return "HWP"
        case .sheet: return "SHEET"
        case .slide: return "SLIDE"
        case .pdf: return "PDF"
        }
    }

    public var systemSymbolName: String {
        switch self {
        case .document: return "doc.richtext.fill"
        case .hwp: return "doc.text.image.fill"
        case .sheet: return "tablecells.fill"
        case .slide: return "rectangle.on.rectangle.angled.fill"
        case .pdf: return "doc.viewfinder.fill"
        }
    }
}

/// Signature Lavender Design System tokens and macOS workspace layout metrics.
public enum EasyOfficeDesignTokens {
    // MARK: - Signature Brand Lavender Scale ("우리의 결")
    public static let brand50 = RGBColorToken(red: 0xF8, green: 0xF9, blue: 0xFF, hex: "#F8F9FF")
    public static let brand100 = RGBColorToken(red: 0xF0, green: 0xF2, blue: 0xFE, hex: "#F0F2FE")
    public static let brand200 = RGBColorToken(red: 0xE2, green: 0xE5, blue: 0xFD, hex: "#E2E5FD")
    public static let brand300 = RGBColorToken(red: 0xCC, green: 0xD2, blue: 0xFC, hex: "#CCD2FC")
    public static let brand400 = RGBColorToken(red: 0xB4, green: 0xBC, blue: 0xFB, hex: "#B4BCFB")
    public static let brand500 = RGBColorToken(red: 0x8E, green: 0x9C, blue: 0xE6, hex: "#8E9CE6")
    public static let brand600 = RGBColorToken(red: 0x74, green: 0x80, blue: 0xD2, hex: "#7480D2")
    public static let brand700 = RGBColorToken(red: 0x5C, green: 0x6B, blue: 0xC0, hex: "#5C6BC0")
    public static let brand800 = RGBColorToken(red: 0x4A, green: 0x58, blue: 0xA9, hex: "#4A58A9")
    public static let brand900 = RGBColorToken(red: 0x3B, green: 0x48, blue: 0x90, hex: "#3B4890")
    public static let brand950 = RGBColorToken(red: 0x1F, green: 0x23, blue: 0x40, hex: "#1F2340")

    // MARK: - Light & Dark Neutral Surfaces
    public static let lightWhite = RGBColorToken(red: 0xFF, green: 0xFF, blue: 0xFF, hex: "#FFFFFF")
    public static let lightScaffold = RGBColorToken(red: 0xF7, green: 0xF8, blue: 0xFC, hex: "#F7F8FC")
    public static let lightSubtle = RGBColorToken(red: 0xF0, green: 0xF2, blue: 0xF7, hex: "#F0F2F7")
    public static let lightBorder = RGBColorToken(red: 0xE1, green: 0xE4, blue: 0xEE, hex: "#E1E4EE")

    public static let inkPrimary = RGBColorToken(red: 0x1F, green: 0x23, blue: 0x40, hex: "#1F2340")
    public static let inkSecondary = RGBColorToken(red: 0x4D, green: 0x53, blue: 0x6B, hex: "#4D536B")
    public static let inkMuted = RGBColorToken(red: 0x69, green: 0x70, blue: 0x89, hex: "#697089")

    public static let darkScaffold = RGBColorToken(red: 0x14, green: 0x17, blue: 0x24, hex: "#141724")
    public static let darkSurface = RGBColorToken(red: 0x1B, green: 0x20, blue: 0x32, hex: "#1B2032")
    public static let darkElevated = RGBColorToken(red: 0x24, green: 0x2B, blue: 0x42, hex: "#242B42")
    public static let darkBorder = RGBColorToken(red: 0x2C, green: 0x34, blue: 0x52, hex: "#2C3452")
    public static let darkText = RGBColorToken(red: 0xF8, green: 0xF9, blue: 0xFD, hex: "#F8F9FD")
    public static let darkMuted = RGBColorToken(red: 0x8A, green: 0x94, blue: 0xB8, hex: "#8A94B8")

    // MARK: - Module Semantic Accents
    public static func accentToken(for module: OfficeModuleKind) -> RGBColorToken {
        switch module {
        case .document:
            return brand700 // Signature Indigo-Lavender (#5C6BC0)
        case .hwp:
            return RGBColorToken(red: 0x6E, green: 0x56, blue: 0xCF, hex: "#6E56CF") // Royal Violet
        case .sheet:
            return RGBColorToken(red: 0x0A, green: 0x70, blue: 0x5A, hex: "#0A705A") // Deep Emerald
        case .slide:
            return RGBColorToken(red: 0xD9, green: 0x6B, blue: 0x27, hex: "#D96B27") // Warm Amber-Coral
        case .pdf:
            return RGBColorToken(red: 0xD9, green: 0x38, blue: 0x3A, hex: "#D9383A") // Crimson
        }
    }

    public static func softBadgeToken(for module: OfficeModuleKind) -> RGBColorToken {
        switch module {
        case .document:
            return brand100
        case .hwp:
            return RGBColorToken(red: 0xF3, green: 0xF0, blue: 0xFF, hex: "#F3F0FF")
        case .sheet:
            return RGBColorToken(red: 0xF0, green: 0xFB, blue: 0xF7, hex: "#F0FBF7")
        case .slide:
            return RGBColorToken(red: 0xFF, green: 0xF4, blue: 0xEC, hex: "#FFF4EC")
        case .pdf:
            return RGBColorToken(red: 0xFE, green: 0xE2, blue: 0xE2, hex: "#FEE2E2")
        }
    }

    // MARK: - macOS Workspace Layout Metrics
    public enum LayoutMetrics {
        public static let tabStripHeight: CGFloat = 36
        public static let compactRibbonHeight: CGFloat = 46
        public static let expandedRibbonHeight: CGFloat = 68
        public static let leftNavigatorWidth: CGFloat = 224
        public static let rightInspectorWidth: CGFloat = 268
        public static let statusBarHeight: CGFloat = 28
        public static let minimumWindowWidth: CGFloat = 1120
        public static let minimumWindowHeight: CGFloat = 740
    }
}
