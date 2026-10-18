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

    /// Inserts a blank page at the given 0-based index and renumbers all pages, annotations, and outline targets sequentially.
    public mutating func insertBlankPage(at index: Int) {
        let previousPageIDs = pages.map(\.id)
        let clampedIndex = max(0, min(index, pages.count))
        let newPage = PDFPageDescriptor(pageNumber: clampedIndex + 1, rotationDegrees: 0, extractedText: "")
        pages.insert(newPage, at: clampedIndex)
        renumberPages()
        remapAnnotationsAndOutline(previousPageIDs: previousPageIDs)
    }

    /// Deletes the page at the given 0-based index and renumbers remaining pages, annotations, and outline targets.
    public mutating func deletePage(at index: Int) {
        guard pages.indices.contains(index) else { return }
        let previousPageIDs = pages.map(\.id)
        let removedPageNum = index + 1
        pages.remove(at: index)
        annotations.removeAll { $0.pageNumber == removedPageNum }
        renumberPages()
        remapAnnotationsAndOutline(previousPageIDs: previousPageIDs)
    }

    /// Reorders a page from `sourceIndex` to `destinationIndex` (0-based) and synchronizes page numbers, annotations, and outline targets.
    public mutating func reorderPage(from sourceIndex: Int, to destinationIndex: Int) {
        guard pages.indices.contains(sourceIndex),
              destinationIndex >= 0,
              destinationIndex < pages.count else {
            return
        }
        let previousPageIDs = pages.map(\.id)
        let item = pages.remove(at: sourceIndex)
        pages.insert(item, at: destinationIndex)
        renumberPages()
        remapAnnotationsAndOutline(previousPageIDs: previousPageIDs)
    }

    /// Extracts the specified 1-based page numbers into a new standalone PDF model.
    public func extractPages(oneBasedPageNumbers: [Int], newTitle: String) -> PDFStudioDocumentModel {
        var extractedPages: [PDFPageDescriptor] = []
        var extractedAnnotations: [PDFAnnotationItem] = []

        for requestedPage in oneBasedPageNumbers {
            guard requestedPage >= 1 else { continue }
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

    private mutating func remapAnnotationsAndOutline(previousPageIDs: [UUID]) {
        var pageMap: [Int: Int] = [:]
        for (oldZeroIdx, id) in previousPageIDs.enumerated() {
            if let newZeroIdx = pages.firstIndex(where: { $0.id == id }) {
                pageMap[oldZeroIdx + 1] = newZeroIdx + 1
            }
        }
        for idx in annotations.indices {
            if let mapped = pageMap[annotations[idx].pageNumber] {
                annotations[idx].pageNumber = mapped
            }
        }
        outline = outline.map { Self.remapOutlineNode($0, using: pageMap, maxPage: max(1, pages.count)) }
    }

    private static func remapOutlineNode(_ node: PDFOutlineNode, using pageMap: [Int: Int], maxPage: Int) -> PDFOutlineNode {
        let mappedTarget = pageMap[node.targetPageNumber] ?? min(max(1, node.targetPageNumber), maxPage)
        return PDFOutlineNode(
            id: node.id,
            title: node.title,
            targetPageNumber: mappedTarget,
            children: node.children.map { remapOutlineNode($0, using: pageMap, maxPage: maxPage) }
        )
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

    private struct DocumentPaginationBlock {
        let text: String
        let paragraphID: UUID?
    }

    /// Builds a `PDFStudioDocumentModel` from an `OfficeDocumentModel` with multi-page pagination across sections and 12-block chunks.
    public static func fromDocument(_ doc: OfficeDocumentModel) -> PDFStudioDocumentModel {
        let maxBlocksPerPage = 12
        var contentBlocks: [DocumentPaginationBlock] = []

        for p in doc.paragraphs {
            let sublines = p.text.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
            if sublines.count <= 1 {
                contentBlocks.append(DocumentPaginationBlock(text: p.text, paragraphID: p.id))
            } else {
                for (subIdx, subline) in sublines.enumerated() {
                    contentBlocks.append(
                        DocumentPaginationBlock(
                            text: subline,
                            paragraphID: subIdx == 0 ? p.id : nil
                        )
                    )
                }
            }
        }
        for table in doc.tables {
            if !table.caption.isEmpty {
                contentBlocks.append(DocumentPaginationBlock(text: table.caption, paragraphID: nil))
            }
            for row in table.rows {
                contentBlocks.append(DocumentPaginationBlock(text: row.joined(separator: " | "), paragraphID: nil))
            }
        }

        let sectionCount = max(1, doc.sections.count)
        var pageChunks: [[DocumentPaginationBlock]] = []

        if sectionCount == 1 && contentBlocks.count <= maxBlocksPerPage {
            pageChunks = [contentBlocks]
        } else if sectionCount > 1 {
            for sectionIdx in 0..<sectionCount {
                let startIdx = (contentBlocks.count * sectionIdx) / sectionCount
                let endIdx = (contentBlocks.count * (sectionIdx + 1)) / sectionCount
                let sectionSlice = Array(contentBlocks[startIdx..<endIdx])
                if sectionSlice.isEmpty {
                    pageChunks.append([])
                } else {
                    var offset = 0
                    while offset < sectionSlice.count {
                        let nextOffset = min(offset + maxBlocksPerPage, sectionSlice.count)
                        pageChunks.append(Array(sectionSlice[offset..<nextOffset]))
                        offset = nextOffset
                    }
                }
            }
        } else {
            var offset = 0
            while offset < contentBlocks.count {
                let nextOffset = min(offset + maxBlocksPerPage, contentBlocks.count)
                pageChunks.append(Array(contentBlocks[offset..<nextOffset]))
                offset = nextOffset
            }
        }

        var pages: [PDFPageDescriptor] = []
        var paragraphPageMap: [UUID: Int] = [:]

        for (idx, chunk) in pageChunks.enumerated() {
            let pageNum = idx + 1
            var lines: [String] = []
            if !doc.headerText.isEmpty {
                lines.append(doc.headerText)
            }
            for block in chunk {
                lines.append(block.text)
                if let pid = block.paragraphID, paragraphPageMap[pid] == nil {
                    paragraphPageMap[pid] = pageNum
                }
            }
            if !doc.footerText.isEmpty {
                lines.append(doc.footerText)
            }
            pages.append(
                PDFPageDescriptor(
                    pageNumber: pageNum,
                    rotationDegrees: 0,
                    extractedText: lines.joined(separator: "\n")
                )
            )
        }

        let outlineNodes = doc.generateTableOfContents().map { entry in
            PDFOutlineNode(
                title: entry.title,
                targetPageNumber: paragraphPageMap[entry.paragraphID] ?? 1
            )
        }

        return PDFStudioDocumentModel(
            title: doc.title,
            pages: pages,
            outline: outlineNodes
        )
    }

    /// Builds a `PDFStudioDocumentModel` from all occupied cells in a `WorksheetModel`, paginating every 25 rows.
    public static func fromWorksheet(_ worksheet: WorksheetModel) -> PDFStudioDocumentModel {
        let rowsPerPage = 25
        var coordsByRow: [Int: [PackedCellCoordinate]] = [:]
        for (coord, rawVal) in worksheet.cellsByCoordinate where !rawVal.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            coordsByRow[coord.row, default: []].append(coord)
        }

        let sortedRows = coordsByRow.keys.sorted()
        var rowStrings: [String] = []
        rowStrings.reserveCapacity(sortedRows.count)

        for rowIdx in sortedRows {
            guard let rowCoords = coordsByRow[rowIdx]?.sorted(by: { $0.column < $1.column }) else { continue }
            let formattedCells = rowCoords.map { coord -> String in
                let addr = CellAddress(packed: coord).a1Notation
                let val = worksheet.evaluate(at: coord).description
                return "\(addr): \(val)"
            }
            if !formattedCells.isEmpty {
                rowStrings.append(formattedCells.joined(separator: " | "))
            }
        }

        return paginateWorksheetRows(rowStrings, title: worksheet.name, rowsPerPage: rowsPerPage)
    }

    /// Builds a `PDFStudioDocumentModel` from a worksheet range, paginating every 25 rows.
    public static func fromWorksheet(_ sheet: WorksheetModel, range: CellRange) -> PDFStudioDocumentModel {
        let minRow = min(range.start.row, range.end.row)
        let maxRow = max(range.start.row, range.end.row)
        let minCol = min(range.start.column, range.end.column)
        let maxCol = max(range.start.column, range.end.column)
        let (rowSpan, rOverflow) = (maxRow - minRow).addingReportingOverflow(1)
        let (colSpan, cOverflow) = (maxCol - minCol).addingReportingOverflow(1)
        let (totalCells, mulOverflow) = rowSpan.multipliedReportingOverflow(by: colSpan)
        guard !rOverflow, !cOverflow, !mulOverflow,
              totalCells > 0, totalCells <= CellRange.maxExpandedCellCount else {
            return PDFStudioDocumentModel(title: sheet.name, pages: [])
        }

        var rowStrings: [String] = []
        for r in minRow...maxRow {
            var cells: [String] = []
            for c in minCol...maxCol {
                let packed = PackedCellCoordinate(row: r, column: c)
                let val = sheet.evaluate(at: packed)
                if val != .empty {
                    let addr = CellAddress(packed: packed).a1Notation
                    cells.append("\(addr): \(val.description)")
                }
            }
            if !cells.isEmpty {
                rowStrings.append(cells.joined(separator: " | "))
            }
        }

        return paginateWorksheetRows(rowStrings, title: sheet.name, rowsPerPage: 25)
    }

    private static func paginateWorksheetRows(_ rowStrings: [String], title: String, rowsPerPage: Int) -> PDFStudioDocumentModel {
        guard !rowStrings.isEmpty else {
            return PDFStudioDocumentModel(
                title: title,
                pages: [PDFPageDescriptor(pageNumber: 1, rotationDegrees: 0, extractedText: "")]
            )
        }
        var pages: [PDFPageDescriptor] = []
        var offset = 0
        while offset < rowStrings.count {
            let nextOffset = min(offset + rowsPerPage, rowStrings.count)
            let chunk = rowStrings[offset..<nextOffset]
            pages.append(
                PDFPageDescriptor(
                    pageNumber: pages.count + 1,
                    rotationDegrees: 0,
                    extractedText: chunk.joined(separator: "\n")
                )
            )
            offset = nextOffset
        }
        return PDFStudioDocumentModel(title: title, pages: pages)
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
            for element in slide.elements {
                if !element.title.isEmpty {
                    lines.append(element.title)
                }
                if !element.bodyText.isEmpty {
                    lines.append(element.bodyText)
                }
            }
            if !slide.speakerNotes.isEmpty {
                lines.append(slide.speakerNotes)
            }
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
