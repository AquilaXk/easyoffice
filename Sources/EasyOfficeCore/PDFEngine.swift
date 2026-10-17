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

/// Hierarchical bookmark/outline node inside a PDF document.
public struct PDFOutlineNode: Sendable, Codable, Equatable, Identifiable {
    public let id: UUID
    public var title: String
    public var targetPageNumber: Int
    public var children: [PDFOutlineNode]

    public init(
        id: UUID = UUID(),
        title: String,
        targetPageNumber: Int,
        children: [PDFOutlineNode] = []
    ) {
        self.id = id
        self.title = title
        self.targetPageNumber = max(1, targetPageNumber)
        self.children = children
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
    public var outline: [PDFOutlineNode]
    public var isRedacted: Bool

    public init(
        id: UUID = UUID(),
        title: String,
        pages: [PDFPageDescriptor] = [],
        annotations: [PDFAnnotationItem] = [],
        outline: [PDFOutlineNode] = [],
        isRedacted: Bool = false
    ) {
        self.id = id
        self.title = title
        self.pages = pages
        self.annotations = annotations
        self.outline = outline
        self.isRedacted = isRedacted
    }

    public mutating func rotatePage(at index: Int, clockwiseDegrees: Int = 90) {
        guard pages.indices.contains(index) else { return }
        let updated = (pages[index].rotationDegrees + clockwiseDegrees) % 360
        pages[index].rotationDegrees = (updated + 360) % 360
    }

    /// Inserts a blank page at the given 0-based index and renumbers all pages sequentially.
    public mutating func insertBlankPage(at index: Int) {
        let clampedIndex = max(0, min(index, pages.count))
        let newPage = PDFPageDescriptor(pageNumber: clampedIndex + 1, rotationDegrees: 0, extractedText: "")
        pages.insert(newPage, at: clampedIndex)
        renumberPages()
    }

    /// Deletes the page at the given 0-based index and renumbers remaining pages.
    public mutating func deletePage(at index: Int) {
        guard pages.indices.contains(index) else { return }
        let removedPageNum = index + 1
        pages.remove(at: index)
        annotations.removeAll { $0.pageNumber == removedPageNum }
        for i in annotations.indices where annotations[i].pageNumber > removedPageNum {
            annotations[i].pageNumber -= 1
        }
        renumberPages()
    }

    /// Reorders a page from `sourceIndex` to `destinationIndex` (0-based) and renumbers all pages.
    public mutating func reorderPage(from sourceIndex: Int, to destinationIndex: Int) {
        guard pages.indices.contains(sourceIndex),
              destinationIndex >= 0,
              destinationIndex < pages.count else {
            return
        }
        let item = pages.remove(at: sourceIndex)
        pages.insert(item, at: destinationIndex)
        renumberPages()
    }

    /// Extracts the specified 1-based page numbers into a new standalone PDF model.
    public func extractPages(oneBasedPageNumbers: [Int], newTitle: String) -> PDFStudioDocumentModel {
        var extractedPages: [PDFPageDescriptor] = []
        var extractedAnnotations: [PDFAnnotationItem] = []

        for requestedPage in oneBasedPageNumbers {
            let zeroIdx = requestedPage - 1
            guard pages.indices.contains(zeroIdx) else { continue }
            var pageCopy = pages[zeroIdx]
            let newPageNumber = extractedPages.count + 1
            pageCopy.pageNumber = newPageNumber
            extractedPages.append(pageCopy)

            for ann in annotations where ann.pageNumber == requestedPage {
                var annCopy = ann
                annCopy.pageNumber = newPageNumber
                extractedAnnotations.append(annCopy)
            }
        }

        return PDFStudioDocumentModel(
            title: newTitle,
            pages: extractedPages,
            annotations: extractedAnnotations
        )
    }

    /// Splits the document after `splitPage` (1-based) into two independent PDF models.
    public func split(atOneBasedPage splitPage: Int) -> (firstPart: PDFStudioDocumentModel, secondPart: PDFStudioDocumentModel) {
        let clampedSplit = max(0, min(splitPage, pages.count))
        let firstRange = Array(1...max(1, clampedSplit)).filter { $0 <= clampedSplit }
        let secondRange = clampedSplit < pages.count ? Array((clampedSplit + 1)...pages.count) : []

        let part1 = extractPages(oneBasedPageNumbers: firstRange, newTitle: "\(title) (Part 1)")
        let part2 = extractPages(oneBasedPageNumbers: secondRange, newTitle: "\(title) (Part 2)")
        return (part1, part2)
    }

    /// Merges multiple PDF models into a single document with continuous page numbering.
    public static func merge(documents: [PDFStudioDocumentModel], mergedTitle: String) -> PDFStudioDocumentModel {
        var combinedPages: [PDFPageDescriptor] = []
        var combinedAnnotations: [PDFAnnotationItem] = []
        var combinedOutline: [PDFOutlineNode] = []

        for doc in documents {
            let pageOffset = combinedPages.count
            for page in doc.pages {
                var copy = page
                copy.pageNumber = combinedPages.count + 1
                combinedPages.append(copy)
            }
            for ann in doc.annotations {
                var copy = ann
                copy.pageNumber = ann.pageNumber + pageOffset
                combinedAnnotations.append(copy)
            }
            for node in doc.outline {
                combinedOutline.append(Self.offsetOutlineNode(node, by: pageOffset))
            }
        }

        return PDFStudioDocumentModel(
            title: mergedTitle,
            pages: combinedPages,
            annotations: combinedAnnotations,
            outline: combinedOutline
        )
    }

    private static func offsetOutlineNode(_ node: PDFOutlineNode, by offset: Int) -> PDFOutlineNode {
        PDFOutlineNode(
            id: node.id,
            title: node.title,
            targetPageNumber: node.targetPageNumber + offset,
            children: node.children.map { offsetOutlineNode($0, by: offset) }
        )
    }

    private mutating func renumberPages() {
        for idx in pages.indices {
            pages[idx].pageNumber = idx + 1
        }
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

    /// Converts each PDF page into a presentation slide inside a `PresentationDeckModel`.
    public func convertToPresentation() -> PresentationDeckModel {
        let slides = pages.map { page -> SlideModel in
            let lines = page.extractedText
                .split(separator: "\n", omittingEmptySubsequences: true)
                .map(String.init)
            let slideTitle = lines.first ?? "페이지 \(page.pageNumber)"
            let bullets = lines.count > 1 ? Array(lines.dropFirst()) : []
            return SlideModel(
                title: slideTitle,
                bulletPoints: bullets,
                masterLayout: bullets.isEmpty ? .titleSlide : .titleAndContent
            )
        }
        return PresentationDeckModel(title: title, aspectRatio: .widescreen16x9, slides: slides)
    }

    /// Builds a `PDFStudioDocumentModel` from an `OfficeDocumentModel`.
    public static func fromDocument(_ doc: OfficeDocumentModel) -> PDFStudioDocumentModel {
        var lines: [String] = []
        if !doc.headerText.isEmpty {
            lines.append(doc.headerText)
        }
        for p in doc.paragraphs {
            lines.append(p.text)
        }
        for table in doc.tables {
            if !table.caption.isEmpty {
                lines.append(table.caption)
            }
            for row in table.rows {
                lines.append(row.joined(separator: " | "))
            }
        }
        if !doc.footerText.isEmpty {
            lines.append(doc.footerText)
        }

        let page = PDFPageDescriptor(
            pageNumber: 1,
            rotationDegrees: 0,
            extractedText: lines.joined(separator: "\n")
        )
        let outlineNodes = doc.generateTableOfContents().map { entry in
            PDFOutlineNode(title: entry.title, targetPageNumber: 1)
        }

        return PDFStudioDocumentModel(
            title: doc.title,
            pages: [page],
            outline: outlineNodes
        )
    }

    /// Builds a `PDFStudioDocumentModel` from a worksheet range.
    public static func fromWorksheet(_ sheet: WorksheetModel, range: CellRange) -> PDFStudioDocumentModel {
        let minRow = min(range.start.row, range.end.row)
        let maxRow = max(range.start.row, range.end.row)
        let minCol = min(range.start.column, range.end.column)
        let maxCol = max(range.start.column, range.end.column)

        var rowStrings: [String] = []
        for r in minRow...maxRow {
            var cells: [String] = []
            for c in minCol...maxCol {
                if let addr = try? CellAddress(column: c, row: r) {
                    cells.append(sheet.evaluate(at: addr.a1Notation).description)
                }
            }
            rowStrings.append(cells.joined(separator: "\t"))
        }

        let page = PDFPageDescriptor(
            pageNumber: 1,
            rotationDegrees: 0,
            extractedText: rowStrings.joined(separator: "\n")
        )
        return PDFStudioDocumentModel(title: sheet.name, pages: [page])
    }

    /// Builds a `PDFStudioDocumentModel` from a `PresentationDeckModel`.
    public static func fromPresentation(_ deck: PresentationDeckModel) -> PDFStudioDocumentModel {
        var pages: [PDFPageDescriptor] = []
        var outlineNodes: [PDFOutlineNode] = []

        for (idx, slide) in deck.slides.enumerated() {
            let pageNum = idx + 1
            var lines: [String] = [slide.title]
            if !slide.subtitle.isEmpty {
                lines.append(slide.subtitle)
            }
            lines.append(contentsOf: slide.bulletPoints)
            pages.append(
                PDFPageDescriptor(
                    pageNumber: pageNum,
                    rotationDegrees: 0,
                    extractedText: lines.joined(separator: "\n")
                )
            )
            outlineNodes.append(PDFOutlineNode(title: slide.title, targetPageNumber: pageNum))
        }

        return PDFStudioDocumentModel(
            title: deck.title,
            pages: pages,
            outline: outlineNodes
        )
    }
}
