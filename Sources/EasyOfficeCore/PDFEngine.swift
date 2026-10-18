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
    id: UUID = UUID(), pageNumber: Int, kind: PDFAnnotationKind, content: String,
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
    id: UUID = UUID(), title: String, targetPageNumber: Int, children: [PDFOutlineNode] = []
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

  public init(id: UUID = UUID(), pageNumber: Int, rotationDegrees: Int = 0, extractedText: String) {
    self.id = id
    self.pageNumber = pageNumber
    self.rotationDegrees = ((rotationDegrees % 360) + 360) % 360
    self.extractedText = extractedText
  }
}

public struct PDFInkPoint: Sendable, Codable, Equatable {
  public var x: Double
  public var y: Double

  public init(x: Double, y: Double) {
    self.x = x
    self.y = y
  }
}

public struct PDFInkStroke: Sendable, Codable, Equatable, Identifiable {
  public var id: UUID
  public var pageNumber: Int
  public var points: [PDFInkPoint]
  public var strokeHex: String
  public var lineWidth: Double
  public var label: String

  public init(
    id: UUID = UUID(), pageNumber: Int, points: [PDFInkPoint], strokeHex: String = "#5C6BC0",
    lineWidth: Double = 2.2, label: String = "자유곡선 전자서명"
  ) {
    self.id = id
    self.pageNumber = max(1, pageNumber)
    self.points = points.filter { $0.x.isFinite && $0.y.isFinite }
    self.strokeHex = strokeHex
    self.lineWidth = lineWidth.isFinite ? max(0.5, min(lineWidth, 12.0)) : 2.2
    self.label = label
  }

  private enum CodingKeys: String, CodingKey {
    case id, pageNumber, points, strokeHex, lineWidth, label
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    let decodedID = try container.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
    let decodedPage = try container.decodeIfPresent(Int.self, forKey: .pageNumber) ?? 1
    let decodedPoints = try container.decodeIfPresent([PDFInkPoint].self, forKey: .points) ?? []
    let decodedHex = try container.decodeIfPresent(String.self, forKey: .strokeHex) ?? "#5C6BC0"
    let decodedWidth = try container.decodeIfPresent(Double.self, forKey: .lineWidth) ?? 2.2
    let decodedLabel = try container.decodeIfPresent(String.self, forKey: .label) ?? "자유곡선 전자서명"
    self.init(
      id: decodedID, pageNumber: decodedPage, points: decodedPoints,
      strokeHex: decodedHex, lineWidth: decodedWidth, label: decodedLabel
    )
  }
}

/// Interactive AcroForm field inside a PDF Studio document.
public struct PDFFormField: Sendable, Codable, Equatable, Identifiable {
  public var id: UUID
  public var pageNumber: Int
  public var name: String
  public var value: String
  public var isCheckbox: Bool
  public var isChecked: Bool

  public init(
    id: UUID = UUID(), pageNumber: Int = 1, name: String, value: String = "",
    isCheckbox: Bool = false, isChecked: Bool = false
  ) {
    self.id = id
    self.pageNumber = max(1, pageNumber)
    self.name = name
    self.value = value
    self.isCheckbox = isCheckbox
    self.isChecked = isChecked
  }
}

/// PDF Studio model supporting inspection, annotations, page rotation/extraction, redaction, form fields, and conversion to editable document model.
public struct PDFStudioDocumentModel: Sendable, Codable, Equatable, Identifiable {
  public let id: UUID
  public var title: String
  public var pages: [PDFPageDescriptor]
  public var annotations: [PDFAnnotationItem]
  public var outline: [PDFOutlineNode]
  public var isRedacted: Bool
  public var inkStrokes: [PDFInkStroke]
  public var formFields: [PDFFormField] = []
  public var redactedPhrases: [String] = []
  public var watermarkText: String = ""

  public init(
    id: UUID = UUID(), title: String, pages: [PDFPageDescriptor] = [],
    annotations: [PDFAnnotationItem] = [], outline: [PDFOutlineNode] = [],
    isRedacted: Bool = false, inkStrokes: [PDFInkStroke] = [],
    formFields: [PDFFormField] = [], redactedPhrases: [String] = [],
    watermarkText: String = ""
  ) {
    self.id = id
    self.title = title
    self.pages = pages
    self.annotations = annotations
    self.outline = outline
    self.isRedacted = isRedacted || !redactedPhrases.isEmpty
    self.inkStrokes = inkStrokes
    self.formFields = formFields
    self.redactedPhrases = redactedPhrases
    self.watermarkText = watermarkText.trimmingCharacters(in: .whitespacesAndNewlines)
  }

  private enum CodingKeys: String, CodingKey {
    case id, title, pages, annotations, outline, isRedacted, inkStrokes, formFields,
      redactedPhrases, watermarkText
  }

  public init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    self.id = try c.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
    self.title = try c.decode(String.self, forKey: .title)
    self.pages = try c.decodeIfPresent([PDFPageDescriptor].self, forKey: .pages) ?? []
    self.annotations = try c.decodeIfPresent([PDFAnnotationItem].self, forKey: .annotations) ?? []
    self.outline = try c.decodeIfPresent([PDFOutlineNode].self, forKey: .outline) ?? []
    let decodedPhrases = try c.decodeIfPresent([String].self, forKey: .redactedPhrases) ?? []
    self.redactedPhrases = decodedPhrases
    let decodedRedacted = try c.decodeIfPresent(Bool.self, forKey: .isRedacted) ?? false
    self.isRedacted = decodedRedacted || !decodedPhrases.isEmpty
    self.inkStrokes = try c.decodeIfPresent([PDFInkStroke].self, forKey: .inkStrokes) ?? []
    self.formFields = try c.decodeIfPresent([PDFFormField].self, forKey: .formFields) ?? []
    let rawWatermark = try c.decodeIfPresent(String.self, forKey: .watermarkText) ?? ""
    self.watermarkText = rawWatermark.trimmingCharacters(in: .whitespacesAndNewlines)
  }

  public func encode(to encoder: Encoder) throws {
    var c = encoder.container(keyedBy: CodingKeys.self)
    try c.encode(id, forKey: .id)
    try c.encode(title, forKey: .title)
    try c.encode(pages, forKey: .pages)
    try c.encode(annotations, forKey: .annotations)
    try c.encode(outline, forKey: .outline)
    try c.encode(isRedacted, forKey: .isRedacted)
    try c.encode(inkStrokes, forKey: .inkStrokes)
    try c.encode(formFields, forKey: .formFields)
    try c.encode(redactedPhrases, forKey: .redactedPhrases)
    try c.encode(watermarkText, forKey: .watermarkText)
  }

  public var totalInkStrokeCount: Int {
    inkStrokes.count
  }

  public mutating func applyWatermark(_ text: String) {
    self.watermarkText = text.trimmingCharacters(in: .whitespacesAndNewlines)
  }

  public var securityInspectionSummary: String {
    let redactionTag =
      isRedacted || !redactedPhrases.isEmpty
      ? "리댁션 영구 마스킹(\(max(1, redactedPhrases.count))건)"
      : "마스킹 없음"
    let signatureTag =
      totalInkStrokeCount > 0
      ? "전자서명 획(\(totalInkStrokeCount)건)"
      : "서명 대기"
    let watermarkTag = watermarkText.isEmpty ? "워터마크 없음" : "워터마크(\(watermarkText))"
    return "ISO 32000-1 무결성 검증 완료 · \(redactionTag) · \(signatureTag) · \(watermarkTag)"
  }

  /// Irreversibly scrubs all case-insensitive occurrences of `targetPhrase` across page text, title, annotations, form fields, and outline nodes, replacing them with `"████"`.
  @discardableResult
  public mutating func redactText(_ targetPhrase: String) -> Int {
    let trimmed = targetPhrase.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return 0 }
    var modifiedPages = 0
    for idx in pages.indices {
      let original = pages[idx].extractedText
      if original.range(of: trimmed, options: [.caseInsensitive]) != nil {
        pages[idx].extractedText = original.replacingOccurrences(
          of: trimmed, with: "████", options: [.caseInsensitive])
        modifiedPages += 1
      }
    }
    if title.range(of: trimmed, options: [.caseInsensitive]) != nil {
      title = title.replacingOccurrences(of: trimmed, with: "████", options: [.caseInsensitive])
    }
    for idx in annotations.indices {
      if annotations[idx].content.range(of: trimmed, options: [.caseInsensitive]) != nil {
        annotations[idx].content = annotations[idx].content.replacingOccurrences(
          of: trimmed, with: "████", options: [.caseInsensitive])
      }
    }
    for idx in formFields.indices {
      if formFields[idx].value.range(of: trimmed, options: [.caseInsensitive]) != nil {
        formFields[idx].value = formFields[idx].value.replacingOccurrences(
          of: trimmed, with: "████", options: [.caseInsensitive])
      }
    }
    Self.redactOutlineNodes(&outline, target: trimmed)
    isRedacted = true
    let maskedToken = "████ (\(trimmed.count)자 마스킹)"
    if !redactedPhrases.contains(maskedToken) {
      redactedPhrases.append(maskedToken)
    }
    return modifiedPages
  }

  private static func redactOutlineNodes(_ nodes: inout [PDFOutlineNode], target: String) {
    for idx in nodes.indices {
      if nodes[idx].title.range(of: target, options: [.caseInsensitive]) != nil {
        nodes[idx].title = nodes[idx].title.replacingOccurrences(
          of: target, with: "████", options: [.caseInsensitive])
      }
      if !nodes[idx].children.isEmpty {
        redactOutlineNodes(&nodes[idx].children, target: target)
      }
    }
  }

  public var pageCount: Int { pages.count }

  public var outlineNodes: [PDFOutlineNode] {
    get { outline }
    set { outline = newValue }
  }

  public mutating func addBookmark(title: String, targetPageNumber: Int, parentID: UUID? = nil) {
    let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return }
    let clampedPage = max(1, min(targetPageNumber, max(1, pages.count)))
    let node = PDFOutlineNode(title: trimmed, targetPageNumber: clampedPage)
    if let parentID, Self.appendBookmark(node, toParentID: parentID, in: &outline) {
      return
    }
    outline.append(node)
  }

  private static func appendBookmark(
    _ child: PDFOutlineNode,
    toParentID parentID: UUID,
    in nodes: inout [PDFOutlineNode]
  ) -> Bool {
    for idx in nodes.indices {
      if nodes[idx].id == parentID {
        nodes[idx].children.append(child)
        return true
      }
      if appendBookmark(child, toParentID: parentID, in: &nodes[idx].children) {
        return true
      }
    }
    return false
  }

  public mutating func removeBookmark(id: UUID) {
    Self.removeBookmarkNode(id: id, from: &outline)
  }

  private static func removeBookmarkNode(id: UUID, from nodes: inout [PDFOutlineNode]) {
    nodes.removeAll { $0.id == id }
    for idx in nodes.indices {
      removeBookmarkNode(id: id, from: &nodes[idx].children)
    }
  }

  public mutating func addFormField(_ field: PDFFormField) { formFields.append(field) }

  public mutating func updateFormField(id: UUID, value: String? = nil, isChecked: Bool? = nil) {
    guard let idx = formFields.firstIndex(where: { $0.id == id }) else { return }
    if let value { formFields[idx].value = value }
    if let isChecked { formFields[idx].isChecked = isChecked }
  }

  public mutating func addInkStroke(_ stroke: PDFInkStroke) { inkStrokes.append(stroke) }

  public mutating func clearInkStrokes(onPage pageNumber: Int? = nil) {
    if let pageNumber {
      inkStrokes.removeAll { $0.pageNumber == pageNumber }
    } else {
      inkStrokes.removeAll()
    }
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
    let newPage = PDFPageDescriptor(
      pageNumber: clampedIndex + 1, rotationDegrees: 0, extractedText: "")
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
    inkStrokes.removeAll { $0.pageNumber == removedPageNum }
    formFields.removeAll { $0.pageNumber == removedPageNum }
    renumberPages()
    remapAnnotationsAndOutline(previousPageIDs: previousPageIDs)
  }

  public mutating func removingPage(at index: Int) {
    deletePage(at: index)
  }

  /// Reorders a page from `sourceIndex` to `destinationIndex` (0-based) and synchronizes page numbers, annotations, and outline targets.
  public mutating func reorderPage(from sourceIndex: Int, to destinationIndex: Int) {
    guard pages.indices.contains(sourceIndex),
      destinationIndex >= 0,
      destinationIndex < pages.count
    else {
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
    var extractedInkStrokes: [PDFInkStroke] = []
    var extractedFormFields: [PDFFormField] = []

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
      for stroke in inkStrokes where stroke.pageNumber == requestedPage {
        var strokeCopy = stroke
        strokeCopy.pageNumber = newPageNumber
        extractedInkStrokes.append(strokeCopy)
      }
      for field in formFields where field.pageNumber == requestedPage {
        var fieldCopy = field
        fieldCopy.pageNumber = newPageNumber
        extractedFormFields.append(fieldCopy)
      }
    }

    return PDFStudioDocumentModel(
      title: newTitle,
      pages: extractedPages,
      annotations: extractedAnnotations,
      isRedacted: isRedacted,
      inkStrokes: extractedInkStrokes,
      formFields: extractedFormFields,
      redactedPhrases: redactedPhrases,
      watermarkText: watermarkText
    )
  }

  public func extractingPage(at oneBasedPage: Int) -> PDFStudioDocumentModel {
    extractPages(oneBasedPageNumbers: [oneBasedPage], newTitle: "\(title) (Page \(oneBasedPage))")
  }

  /// Splits the document after `splitPage` (1-based) into two independent PDF models.
  public func split(atOneBasedPage splitPage: Int) -> (
    firstPart: PDFStudioDocumentModel, secondPart: PDFStudioDocumentModel
  ) {
    let clampedSplit = max(0, min(splitPage, pages.count))
    let firstRange = Array(1...max(1, clampedSplit)).filter { $0 <= clampedSplit }
    let secondRange = clampedSplit < pages.count ? Array((clampedSplit + 1)...pages.count) : []

    let part1 = extractPages(oneBasedPageNumbers: firstRange, newTitle: "\(title) (Part 1)")
    let part2 = extractPages(oneBasedPageNumbers: secondRange, newTitle: "\(title) (Part 2)")
    return (part1, part2)
  }

  public func splitting(afterPage splitPage: Int) -> (
    firstPart: PDFStudioDocumentModel, secondPart: PDFStudioDocumentModel
  ) {
    split(atOneBasedPage: splitPage)
  }

  public func merged(with other: PDFStudioDocumentModel, title: String? = nil)
    -> PDFStudioDocumentModel
  {
    Self.merge(documents: [self, other], mergedTitle: title ?? self.title)
  }

  /// Merges multiple PDF models into a single document with continuous page numbering.
  public static func merge(documents: [PDFStudioDocumentModel], mergedTitle: String)
    -> PDFStudioDocumentModel
  {
    var combinedPages: [PDFPageDescriptor] = []
    var combinedAnnotations: [PDFAnnotationItem] = []
    var combinedOutline: [PDFOutlineNode] = []
    var combinedInkStrokes: [PDFInkStroke] = []
    var combinedFormFields: [PDFFormField] = []
    var combinedRedactedPhrases: [String] = []
    var anyRedacted = false
    let resolvedWatermark =
      documents.first(where: { !$0.watermarkText.isEmpty })?.watermarkText ?? ""

    for doc in documents {
      if doc.isRedacted { anyRedacted = true }
      for phrase in doc.redactedPhrases where !combinedRedactedPhrases.contains(phrase) {
        combinedRedactedPhrases.append(phrase)
      }
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
      for stroke in doc.inkStrokes {
        var copy = stroke
        copy.pageNumber = stroke.pageNumber + pageOffset
        combinedInkStrokes.append(copy)
      }
      for field in doc.formFields {
        var copy = field
        copy.pageNumber = field.pageNumber + pageOffset
        combinedFormFields.append(copy)
      }
      for node in doc.outline {
        combinedOutline.append(Self.offsetOutlineNode(node, by: pageOffset))
      }
    }

    return PDFStudioDocumentModel(
      title: mergedTitle,
      pages: combinedPages,
      annotations: combinedAnnotations,
      outline: combinedOutline,
      isRedacted: anyRedacted,
      inkStrokes: combinedInkStrokes,
      formFields: combinedFormFields,
      redactedPhrases: combinedRedactedPhrases,
      watermarkText: resolvedWatermark
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
    for idx in inkStrokes.indices {
      if let mapped = pageMap[inkStrokes[idx].pageNumber] {
        inkStrokes[idx].pageNumber = mapped
      }
    }
    for idx in formFields.indices {
      if let mapped = pageMap[formFields[idx].pageNumber] {
        formFields[idx].pageNumber = mapped
      }
    }
    outline = outline.map {
      Self.remapOutlineNode($0, using: pageMap, maxPage: max(1, pages.count))
    }
  }

  private static func remapOutlineNode(
    _ node: PDFOutlineNode, using pageMap: [Int: Int], maxPage: Int
  ) -> PDFOutlineNode {
    let mappedTarget = pageMap[node.targetPageNumber] ?? min(max(1, node.targetPageNumber), maxPage)
    return PDFOutlineNode(
      id: node.id,
      title: node.title,
      targetPageNumber: mappedTarget,
      children: node.children.map { remapOutlineNode($0, using: pageMap, maxPage: maxPage) }
    )
  }

  /// Converts extracted PDF text streams into an editable `OfficeDocumentModel` (.docx / .hwpx target).
  public func convertToEditableDocument(targetFormat: DocumentFormat = .docx) -> OfficeDocumentModel
  {
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
      paragraphs: convertedParagraphs,
      watermarkText: watermarkText.isEmpty ? nil : watermarkText
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
      let sublines = p.text.split(separator: "\n", omittingEmptySubsequences: false).map(
        String.init)
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
        contentBlocks.append(
          DocumentPaginationBlock(text: row.joined(separator: " | "), paragraphID: nil))
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

    let totalPages = max(1, pageChunks.count)
    var pages: [PDFPageDescriptor] = []
    var paragraphPageMap: [UUID: Int] = [:]

    for (idx, chunk) in pageChunks.enumerated() {
      let pageNum = idx + 1
      var lines: [String] = []
      if !doc.headerText.isEmpty {
        lines.append(
          OfficeDocumentModel.resolvePageTokens(
            in: doc.headerText, pageNumber: pageNum, totalPages: totalPages))
      }
      for block in chunk {
        lines.append(block.text)
        if let pid = block.paragraphID, paragraphPageMap[pid] == nil {
          paragraphPageMap[pid] = pageNum
        }
      }
      if !doc.footerText.isEmpty {
        lines.append(
          OfficeDocumentModel.resolvePageTokens(
            in: doc.footerText, pageNumber: pageNum, totalPages: totalPages))
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
      outline: outlineNodes,
      watermarkText: doc.watermarkText ?? ""
    )
  }

  /// Builds a `PDFStudioDocumentModel` from all occupied cells in a `WorksheetModel`, paginating every 25 rows.
  public static func fromWorksheet(_ worksheet: WorksheetModel) -> PDFStudioDocumentModel {
    let rowsPerPage = 25
    var coordsByRow: [Int: [PackedCellCoordinate]] = [:]
    for (coord, rawVal) in worksheet.cellsByCoordinate
    where !rawVal.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      coordsByRow[coord.row, default: []].append(coord)
    }

    let sortedRows = coordsByRow.keys.sorted()
    var rowStrings: [String] = []
    rowStrings.reserveCapacity(sortedRows.count)

    for rowIdx in sortedRows {
      guard let rowCoords = coordsByRow[rowIdx]?.sorted(by: { $0.column < $1.column }) else {
        continue
      }
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
  public static func fromWorksheet(_ sheet: WorksheetModel, range: CellRange)
    -> PDFStudioDocumentModel
  {
    let minRow = min(range.start.row, range.end.row)
    let maxRow = max(range.start.row, range.end.row)
    let minCol = min(range.start.column, range.end.column)
    let maxCol = max(range.start.column, range.end.column)
    let (rowSpan, rOverflow) = (maxRow - minRow).addingReportingOverflow(1)
    let (colSpan, cOverflow) = (maxCol - minCol).addingReportingOverflow(1)
    let (totalCells, mulOverflow) = rowSpan.multipliedReportingOverflow(by: colSpan)
    guard !rOverflow, !cOverflow, !mulOverflow,
      totalCells > 0, totalCells <= CellRange.maxExpandedCellCount
    else {
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

  private static func paginateWorksheetRows(_ rowStrings: [String], title: String, rowsPerPage: Int)
    -> PDFStudioDocumentModel
  {
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
