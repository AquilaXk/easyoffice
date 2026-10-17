import Foundation

/// Annotation types supported in PDF Studio.
public enum PDFAnnotationKind: String, CaseIterable, Sendable, Codable {
    case highlight = "형광펜 하이라이트"
    case underline = "밑줄"
    case stickyNote = "스티커 메모"
    case stamp = "승인/검토 도장"
    case signature = "전자 서명"
}

/// Single PDF annotation entry anchored to a 1-based page number.
public struct PDFAnnotationItem: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var pageNumber: Int
    public var kind: PDFAnnotationKind
    public var content: String
    public var author: String

    public init(
        id: UUID = UUID(),
        pageNumber: Int,
        kind: PDFAnnotationKind,
        content: String,
        author: String = "Workspace User"
    ) {
        self.id = id
        self.pageNumber = max(1, pageNumber)
        self.kind = kind
        self.content = content
        self.author = author
    }
}

/// Single page descriptor inside PDF Studio.
public struct PDFPageDescriptor: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var pageNumber: Int
    public var rotationDegrees: Int
    public var extractedText: String

    public init(
        id: UUID = UUID(),
        pageNumber: Int,
        rotationDegrees: Int = 0,
        extractedText: String
    ) {
        self.id = id
        self.pageNumber = pageNumber
        self.rotationDegrees = ((rotationDegrees % 360) + 360) % 360
        self.extractedText = extractedText
    }
}

/// PDF Studio model supporting inspection, annotations, page rotation/extraction, and conversion to editable document model.
public struct PDFStudioDocumentModel: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var title: String
    public var pages: [PDFPageDescriptor]
    public var annotations: [PDFAnnotationItem]

    public init(
        id: UUID = UUID(),
        title: String,
        pages: [PDFPageDescriptor] = [],
        annotations: [PDFAnnotationItem] = []
    ) {
        self.id = id
        self.title = title
        self.pages = pages
        self.annotations = annotations
    }

    public mutating func rotatePage(at index: Int, clockwiseDegrees: Int = 90) {
        guard pages.indices.contains(index) else { return }
        let updated = (pages[index].rotationDegrees + clockwiseDegrees) % 360
        pages[index].rotationDegrees = (updated + 360) % 360
    }

    /// Converts extracted PDF text streams into an editable `OfficeDocumentModel` (.docx / .hwpx target).
    public func convertToEditableDocument(targetFormat: DocumentFormat = .docx) -> OfficeDocumentModel {
        let convertedParagraphs = pages.flatMap { page in
            page.extractedText
                .split(separator: "\n", omittingEmptySubsequences: true)
                .map { line in
                    ParagraphBlock(style: .body, alignment: .justified, text: String(line))
                }
        }
        return OfficeDocumentModel(
            title: title,
            format: targetFormat,
            paperSize: .a4,
            paragraphs: convertedParagraphs
        )
    }
}
