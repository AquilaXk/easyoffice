import AppKit
import EasyOfficeDesignSystem
import Foundation

public typealias FootnoteEntry = FootnoteItem
public typealias TableBorderStyle = TableCellBorderStyle

extension FootnoteItem {
  public var content: String {
    get { text }
    set { text = newValue }
  }

  public init(
    id: UUID = UUID(),
    number: Int,
    referenceParagraphIndex: Int = 0,
    content: String,
    isEndnote: Bool = false,
    anchorParagraphID: UUID = UUID()
  ) {
    self.init(
      id: id,
      number: number,
      isEndnote: isEndnote,
      anchorParagraphID: anchorParagraphID,
      text: content
    )
  }
}

extension TableCellSpan {
  public var row: Int {
    get { rowIndex }
    set { rowIndex = newValue }
  }

  public var column: Int {
    get { columnIndex }
    set { columnIndex = newValue }
  }
}

extension DocumentTableBlock {
  public mutating func mergeCells(row: Int, column: Int, rowSpan: Int, columnSpan: Int) {
    mergeCells(startRow: row, startCol: column, rowSpan: rowSpan, colSpan: columnSpan)
  }

  public mutating func unmergeCell(row: Int, column: Int) {
    splitCell(row: row, col: column)
  }
}

extension SlideModel {
  public init(
    id: UUID = UUID(),
    slideNumber: Int,
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
    self.init(
      id: id,
      title: title,
      subtitle: subtitle,
      bulletPoints: bulletPoints,
      speakerNotes: speakerNotes,
      masterLayout: masterLayout,
      transition: transition,
      elements: elements,
      transitionDurationSeconds: transitionDurationSeconds,
      autoAdvanceSeconds: autoAdvanceSeconds
    )
  }
}

extension ImageWrapMode {
  public static var inlineWithText: ImageWrapMode { .inline }
}

extension DocumentEquationBlock {
  public var caption: String {
    get { displayTitle }
    set { displayTitle = newValue }
  }
}

extension OfficeDocumentModel {
  public mutating func insertEquation(rawExpression: String, caption: String = "수식 블록") {
    addEquation(title: caption, rawExpression: rawExpression)
  }

  public mutating func updateEquation(id: UUID, rawExpression: String) {
    guard let idx = equations.firstIndex(where: { $0.id == id }) else { return }
    equations[idx].rawExpression = rawExpression
  }
}

extension WorkspaceViewModel {

  // MARK: - URI Scheme Allowlist Validator

  /// Validates and normalizes a hyperlink URL string against the safe scheme allowlist (`https`, `http`, `mailto`).
  public static func sanitizeSafeHyperlinkURL(_ rawInput: String) -> String? {
    let trimmed = rawInput.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return nil }
    for scalar in trimmed.unicodeScalars {
      if scalar.value < 0x20 || scalar.value == 0x7F {
        return nil
      }
    }
    let candidate: String
    if !trimmed.contains(":") {
      candidate = "https://\(trimmed)"
    } else {
      candidate = trimmed
    }
    guard let components = URLComponents(string: candidate),
      let scheme = components.scheme?.lowercased(),
      let url = components.url
    else {
      return nil
    }
    switch scheme {
    case "https", "http":
      guard let host = components.host, !host.isEmpty else { return nil }
      return url.absoluteString
    case "mailto":
      guard !components.path.isEmpty else { return nil }
      return url.absoluteString
    default:
      return nil
    }
  }

  // MARK: - Document & HWP Interactive Actions

  public func toggleManuscriptGridMode() {
    isManuscriptGridModeEnabled.toggle()
    lastExecutedCommandSummary =
      isManuscriptGridModeEnabled ? "200자 원고지 격자 뷰 활성화" : "일반 문서 페이지 뷰 전환"
  }

  public func applyMailMergeFieldReplacement(key: String? = nil, value: String? = nil) {
    let k = (key ?? mailMergeFieldKeyDraft).trimmingCharacters(in: .whitespacesAndNewlines)
    let v = (value ?? mailMergeFieldValueDraft).trimmingCharacters(in: .whitespacesAndNewlines)
    guard !k.isEmpty,
      let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    mailMergeFields[k] = v
    let placeholder = "{{\(k)}}"
    let hadPlaceholder = doc.searchablePlainText().contains(placeholder)
    doc = doc.applyingMailMerge(record: [k: v])
    if !hadPlaceholder {
      let summaryLine = "\(k): \(v)"
      doc.paragraphs.append(
        ParagraphBlock(style: .body, alignment: .justified, text: summaryLine)
      )
    }
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "메일 머지 치환 완료 ({{\(k)}} → \(v))"
  }

  public func setActiveParagraphAlignment(_ alignment: TextAlignmentMode) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      !doc.paragraphs.isEmpty
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let targetIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
    doc.paragraphs[targetIdx].alignment = alignment
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "문단 정렬 변경: \(alignment.rawValue)"
  }

  public func setActiveParagraphLineSpacing(_ percent: Double) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      !doc.paragraphs.isEmpty
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let targetIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
    let clamped = max(80.0, min(400.0, percent))
    doc.paragraphs[targetIdx].lineSpacingPercent = clamped
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "문단 줄간격 변경: \(Int(clamped))%"
  }

  public func setActiveParagraphListKind(_ kind: ParagraphListKind, levelDelta: Int = 0) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      !doc.paragraphs.isEmpty
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let targetIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
    doc.paragraphs[targetIdx].listKind = kind
    let nextLevel = max(0, min(3, doc.paragraphs[targetIdx].listLevel + levelDelta))
    doc.paragraphs[targetIdx].listLevel = nextLevel
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "문단 다단계 목록 설정: \(kind.rawValue) (수준 \(nextLevel + 1))"
  }

  public func mergeActiveDocumentTableCells(
    tableIndex: Int? = nil,
    row: Int? = nil,
    column: Int? = nil,
    rowSpan: Int = 1,
    columnSpan: Int = 2
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      !doc.tables.isEmpty
    else { return }
    let resolvedIdx = min(max(0, tableIndex ?? selectedDocumentTableIndex), doc.tables.count - 1)
    let targetRow = row ?? selectedDocumentTableCell?.row ?? 0
    let targetCol = column ?? selectedDocumentTableCell?.column ?? 0
    recordUndoSnapshot(for: activeTabID)
    doc.tables[resolvedIdx].mergeCells(
      row: targetRow,
      column: targetCol,
      rowSpan: rowSpan,
      columnSpan: columnSpan
    )
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary =
      "표 셀 병합 완료 (\(targetRow + 1)행 \(targetCol + 1)열, \(rowSpan)×\(columnSpan))"
  }

  public func splitActiveDocumentTableCell(
    tableIndex: Int? = nil,
    row: Int? = nil,
    column: Int? = nil
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      !doc.tables.isEmpty
    else { return }
    let resolvedIdx = min(max(0, tableIndex ?? selectedDocumentTableIndex), doc.tables.count - 1)
    recordUndoSnapshot(for: activeTabID)
    if let r = row, let c = column {
      doc.tables[resolvedIdx].unmergeCell(row: r, column: c)
    } else if let cell = selectedDocumentTableCell,
      doc.tables[resolvedIdx].cellSpans.contains(where: {
        cell.row >= $0.rowIndex && cell.row < ($0.rowIndex + $0.rowSpan)
          && cell.column >= $0.columnIndex && cell.column < ($0.columnIndex + $0.columnSpan)
      })
    {
      doc.tables[resolvedIdx].unmergeCell(row: cell.row, column: cell.column)
    } else if let lastSpan = doc.tables[resolvedIdx].cellSpans.last {
      doc.tables[resolvedIdx].unmergeCell(row: lastSpan.row, column: lastSpan.column)
    }
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "표 병합 셀 분할 완료"
  }

  public func setActiveDocumentTableBorderStyle(_ style: TableBorderStyle, tableIndex: Int? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      !doc.tables.isEmpty
    else { return }
    let resolvedIdx = min(max(0, tableIndex ?? selectedDocumentTableIndex), doc.tables.count - 1)
    recordUndoSnapshot(for: activeTabID)
    doc.tables[resolvedIdx].borderStyle = style
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "표 테두리 스타일 변경: \(style.rawValue)"
  }

  public func updateSelectedParagraphSelection(paragraphID: UUID, utf16Range: Range<Int>?) {
    selectedParagraphID = paragraphID
    selectedParagraphUTF16Range = utf16Range
  }

  public func appendParagraphToActiveDocument(text: String, style: ParagraphStyleRole = .body) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let block = ParagraphBlock(
      style: style,
      alignment: style == .officialHeader ? .center : .justified,
      text: text,
      fontSizePt: style == .title ? 20.0 : (style == .heading1 ? 14.0 : 11.0),
      isBold: style != .body
    )
    doc.paragraphs.append(block)
    selectedParagraphID = block.id
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
  }

  public func updateParagraphText(paragraphID: UUID, newText: String) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      let pIdx = doc.paragraphs.firstIndex(where: { $0.id == paragraphID }),
      doc.paragraphs[pIdx].text != newText
    else { return }
    let targetKey = "para-\(activeTabID.uuidString)-\(paragraphID.uuidString)"
    let isFirstStrokeInBurst = (activeInlineEditTargetKey != targetKey)
    recordUndoSnapshot(for: activeTabID, coalescingTargetKey: targetKey)
    if doc.trackChangesEnabled {
      if isFirstStrokeInBurst {
        doc.recordRevision(
          author: authorName,
          kind: .formattingChange,
          paragraphID: paragraphID,
          proposedText: newText
        )
      } else if let rIdx = doc.revisions.indices.last,
        doc.revisions[rIdx].paragraphID == paragraphID
      {
        doc.revisions[rIdx].proposedText = newText
      }
    }
    doc.paragraphs[pIdx].text = newText
    if doc.paragraphs[pIdx].inlineRuns.count == 1 {
      doc.paragraphs[pIdx].inlineRuns[0].text = newText
    } else if doc.paragraphs[pIdx].inlineRuns.count > 1 {
      var updatedRuns = doc.paragraphs[pIdx].inlineRuns
      var remainingStart = newText.startIndex
      var firstChangedIdx = 0
      while firstChangedIdx < updatedRuns.count - 1 {
        let runText = updatedRuns[firstChangedIdx].text
        if newText[remainingStart...].hasPrefix(runText) {
          remainingStart = newText.index(remainingStart, offsetBy: runText.count)
          firstChangedIdx += 1
        } else {
          break
        }
      }
      var remainingEnd = newText.endIndex
      var lastChangedIdx = updatedRuns.count - 1
      while lastChangedIdx > firstChangedIdx {
        let runText = updatedRuns[lastChangedIdx].text
        if newText[remainingStart..<remainingEnd].hasSuffix(runText) {
          remainingEnd = newText.index(remainingEnd, offsetBy: -runText.count)
          lastChangedIdx -= 1
        } else {
          break
        }
      }
      updatedRuns[firstChangedIdx].text = String(newText[remainingStart..<remainingEnd])
      if firstChangedIdx < lastChangedIdx {
        for clearIdx in (firstChangedIdx + 1)...lastChangedIdx {
          updatedRuns[clearIdx].text = ""
        }
        updatedRuns.removeAll(where: { $0.text.isEmpty })
      }
      if updatedRuns.isEmpty, var fallback = doc.paragraphs[pIdx].inlineRuns.first {
        fallback.text = newText
        updatedRuns = [fallback]
      }
      doc.paragraphs[pIdx].inlineRuns = updatedRuns
    }
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
  }

  public func updateTableCell(tableID: UUID, row: Int, column: Int, newValue: String) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      let tIdx = doc.tables.firstIndex(where: { $0.id == tableID }),
      doc.tables[tIdx].rows.indices.contains(row),
      doc.tables[tIdx].rows[row].indices.contains(column)
    else { return }
    selectedDocumentTableIndex = tIdx
    selectedDocumentTableCell = (row: row, column: column)
    guard doc.tables[tIdx].rows[row][column] != newValue else { return }
    recordUndoSnapshot(
      for: activeTabID,
      coalescingTargetKey: "table-\(activeTabID.uuidString)-\(tableID.uuidString)-\(row)-\(column)"
    )
    doc.tables[tIdx].rows[row][column] = newValue
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
  }

  public func updateTableCell(tableID: UUID, row: Int, col: Int, newValue: String) {
    updateTableCell(tableID: tableID, row: row, column: col, newValue: newValue)
  }

  public func insertEquationIntoActiveDocument(
    expression: String? = nil,
    caption: String = "수식 블록"
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    let candidate = (expression ?? equationDraftText).trimmingCharacters(
      in: .whitespacesAndNewlines
    )
    let expr = candidate.isEmpty ? "E = mc^2" : candidate
    recordUndoSnapshot(for: activeTabID)
    doc.insertEquation(rawExpression: expr, caption: caption)
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "수식 블록 삽입 완료: \(caption)"
  }

  public func updateDocumentEquation(id: UUID, newExpression: String) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      let eqIdx = doc.equations.firstIndex(where: { $0.id == id }),
      doc.equations[eqIdx].rawExpression != newExpression
    else { return }
    recordUndoSnapshot(for: activeTabID, coalescingTargetKey: "eq:\(activeTabID):\(id)")
    doc.updateEquation(id: id, rawExpression: newExpression)
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "수식 표현식 수정 완료"
  }

  public func cycleImageWrapMode(imageID: UUID? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      !doc.images.isEmpty
    else { return }
    let imgIdx = imageID.flatMap { id in doc.images.firstIndex(where: { $0.id == id }) } ?? 0
    recordUndoSnapshot(for: activeTabID)
    let allModes = ImageWrapMode.allCases
    let next = allModes[
      ((allModes.firstIndex(of: doc.images[imgIdx].wrapMode) ?? 0) + 1) % allModes.count
    ]
    doc.images[imgIdx].wrapMode = next
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "이미지 배치 모드 전환: \(next.rawValue)"
  }

  public func setDocumentWatermark(_ text: String? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let trimmed = (text ?? watermarkDraftText).trimmingCharacters(in: .whitespacesAndNewlines)
    doc.watermarkText = trimmed.isEmpty ? nil : trimmed
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = trimmed.isEmpty ? "문서 워터마크 해제" : "문서 워터마크 적용: \(trimmed)"
  }

  internal static func makeSampleDiagramPNG(
    width: Int = 160,
    height: Int = 48,
    accentR: UInt8 = 0x5C,
    accentG: UInt8 = 0x6B,
    accentB: UInt8 = 0xC0
  ) -> Data {
    let safeW = max(16, width)
    let safeH = max(16, height)
    guard
      let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil,
        pixelsWide: safeW,
        pixelsHigh: safeH,
        bitsPerSample: 8,
        samplesPerPixel: 4,
        hasAlpha: true,
        isPlanar: false,
        colorSpaceName: .deviceRGB,
        bytesPerRow: safeW * 4,
        bitsPerPixel: 32
      ),
      let ptr = rep.bitmapData
    else { return Data() }
    let softR = UInt8(min(255, Int(accentR) + 32))
    let softG = UInt8(min(255, Int(accentG) + 32))
    let softB = UInt8(min(255, Int(accentB) + 24))
    for y in 0..<safeH {
      for x in 0..<safeW {
        let offset = (y * safeW + x) * 4
        let isBorder = x < 2 || x >= safeW - 2 || y < 2 || y >= safeH - 2
        let isBar1 = (x >= safeW / 8 && x < safeW * 3 / 10 && y >= safeH / 3 && y < safeH - 8)
        let isBar2 = (x >= safeW * 2 / 5 && x < safeW * 3 / 5 && y >= safeH / 6 && y < safeH - 8)
        let isBar3 = (x >= safeW * 7 / 10 && x < safeW * 7 / 8 && y >= safeH / 4 && y < safeH - 8)
        if isBorder || isBar2 {
          ptr[offset] = accentR
          ptr[offset + 1] = accentG
          ptr[offset + 2] = accentB
          ptr[offset + 3] = 255
        } else if isBar1 || isBar3 {
          ptr[offset] = softR
          ptr[offset + 1] = softG
          ptr[offset + 2] = softB
          ptr[offset + 3] = 255
        } else {
          ptr[offset] = 238
          ptr[offset + 1] = 240
          ptr[offset + 2] = 255
          ptr[offset + 3] = 255
        }
      }
    }
    return rep.representation(using: .png, properties: [:]) ?? Data()
  }

  private static func makeSampleLavenderDiagramPNG(width: Int = 160, height: Int = 48) -> Data {
    makeSampleDiagramPNG(width: width, height: height, accentR: 0x5C, accentG: 0x6B, accentB: 0xC0)
  }

  public func insertSampleImageAttachment(
    caption: String = "아키텍처 다이어그램 이미지",
    widthPt: Double = 320,
    heightPt: Double = 180
  ) {
    let pngData = Self.makeSampleLavenderDiagramPNG(width: 160, height: 48)
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(let doc) = openTabs[idx].payload
    else { return }
    importImageAttachment(
      data: pngData,
      fileName: "diagram_\(doc.images.count + 1).png",
      caption: caption,
      widthPt: widthPt,
      heightPt: heightPt
    )
  }

  public func insertEmbeddedImageIntoActiveDocument(
    caption: String = "아키텍처 다이어그램 이미지",
    widthPt: Double = 320,
    heightPt: Double = 180
  ) {
    insertSampleImageAttachment(caption: caption, widthPt: widthPt, heightPt: heightPt)
  }

  public static let maxImageAttachmentBytes: Int = 32 * 1024 * 1024

  private static func detectRasterImageMimeType(data: Data, fileName: String) -> String? {
    guard data.count >= 4, data.count <= maxImageAttachmentBytes else { return nil }
    let lowerName = fileName.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    let hasExt = lowerName.contains(".")
    if data.count >= 8 && data.starts(with: [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
      return (!hasExt || lowerName.hasSuffix(".png")) ? "image/png" : nil
    }
    if data.starts(with: [0xFF, 0xD8, 0xFF]) {
      return (!hasExt || lowerName.hasSuffix(".jpg") || lowerName.hasSuffix(".jpeg"))
        ? "image/jpeg" : nil
    }
    if data.starts(with: [0x49, 0x49, 0x2A, 0x00]) || data.starts(with: [0x4D, 0x4D, 0x00, 0x2A]) {
      return (!hasExt || lowerName.hasSuffix(".tif") || lowerName.hasSuffix(".tiff"))
        ? "image/tiff" : nil
    }
    return nil
  }

  public func importImageAttachment(
    data: Data,
    fileName: String,
    caption: String = "",
    widthPt: Double? = nil,
    heightPt: Double? = nil
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    guard !data.isEmpty,
      data.count <= Self.maxImageAttachmentBytes,
      let mimeType = Self.detectRasterImageMimeType(data: data, fileName: fileName),
      let bitmapRep = NSBitmapImageRep(data: data),
      bitmapRep.pixelsWide > 0,
      bitmapRep.pixelsHigh > 0
    else {
      lastErrorMessage = "유효하지 않은 이미지 시그니처(매직 바이트 불일치)이거나 크기 제한(32MB)을 초과하여 거부되었습니다."
      return
    }
    recordUndoSnapshot(for: activeTabID)
    let resolvedW = widthPt ?? Double(max(64, bitmapRep.pixelsWide))
    let resolvedH = heightPt ?? Double(max(36, bitmapRep.pixelsHigh))
    let resolvedCaption = caption.isEmpty ? fileName : caption
    doc.images.append(
      EmbeddedImageAttachment(
        fileName: fileName,
        caption: resolvedCaption,
        mimeType: mimeType,
        imageData: data,
        widthPt: resolvedW,
        heightPt: resolvedH,
        anchorParagraphID: selectedParagraphID ?? doc.paragraphs.first?.id
      )
    )
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "임베디드 이미지 첨부 완료: \(resolvedCaption)"
  }

  public func loadImageAttachmentFromDisk(url: URL) throws {
    let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
    guard values.isRegularFile == true,
      let fileSize = values.fileSize,
      fileSize > 0,
      fileSize <= Self.maxImageAttachmentBytes
    else {
      throw EasyOfficeError.corruptedContainer(detail: "유효한 일반 이미지 파일이 아니거나 크기 제한(32MB)을 초과합니다.")
    }
    let data = try Data(contentsOf: url, options: [.mappedIfSafe])
    guard Self.detectRasterImageMimeType(data: data, fileName: url.lastPathComponent) != nil,
      let bitmapRep = NSBitmapImageRep(data: data),
      bitmapRep.pixelsWide > 0,
      bitmapRep.pixelsHigh > 0
    else {
      throw EasyOfficeError.corruptedContainer(detail: "유효하지 않은 이미지 파일이거나 매직 바이트와 확장자가 일치하지 않습니다.")
    }
    importImageAttachment(
      data: data,
      fileName: url.lastPathComponent,
      caption: url.deletingPathExtension().lastPathComponent
    )
  }

  public func applyInlineRunFormattingToActiveParagraph(
    isBold: Bool? = nil,
    isItalic: Bool? = nil,
    isUnderlined: Bool? = nil,
    isStrikethrough: Bool? = nil,
    fontSizePt: Double? = nil,
    foregroundHex: String? = nil,
    backgroundHex: String? = nil,
    linkURL: String? = nil
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      !doc.paragraphs.isEmpty
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let pIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
    if let utf16Range = selectedParagraphUTF16Range, !utf16Range.isEmpty {
      doc.paragraphs[pIdx].applyRunFormatting(
        utf16Range: utf16Range,
        isBold: isBold,
        isItalic: isItalic,
        isUnderlined: isUnderlined,
        isStrikethrough: isStrikethrough,
        fontSizePt: fontSizePt,
        foregroundHex: foregroundHex,
        backgroundHex: backgroundHex,
        linkURL: linkURL
      )
      openTabs[idx].payload = .document(doc)
      openTabs[idx].isModified = true
      lastExecutedCommandSummary =
        "선택 구간(\(utf16Range.lowerBound)..<\(utf16Range.upperBound)) 부분 서식 적용"
      return
    }
    let baseText = doc.paragraphs[pIdx].text
    let trimmedPhrase = inlineRunTargetPhrase.trimmingCharacters(in: .whitespacesAndNewlines)
    let hasExplicitExtendedFormatting =
      (isStrikethrough != nil || fontSizePt != nil || backgroundHex != nil || linkURL != nil)
    let fbBold: Bool? = isBold ?? (hasExplicitExtendedFormatting ? nil : true)
    let fbItalic: Bool? = isItalic ?? (hasExplicitExtendedFormatting ? nil : false)
    let fbUnder: Bool? = isUnderlined ?? (hasExplicitExtendedFormatting ? nil : false)
    let fbColor: String? = foregroundHex ?? (hasExplicitExtendedFormatting ? nil : "#5C6BC0")
    if !trimmedPhrase.isEmpty, let matchRange = baseText.range(of: trimmedPhrase) {
      doc.paragraphs[pIdx].applyRunFormatting(
        range: matchRange,
        isBold: fbBold,
        isItalic: fbItalic,
        isUnderlined: fbUnder,
        isStrikethrough: isStrikethrough,
        fontSizePt: fontSizePt,
        foregroundHex: fbColor,
        backgroundHex: backgroundHex,
        linkURL: linkURL
      )
    } else {
      let firstWordRange: Range<String.Index> = {
        if let spaceIdx = baseText.firstIndex(of: " "), spaceIdx < baseText.endIndex {
          return baseText.startIndex..<baseText.index(after: spaceIdx)
        }
        return baseText.startIndex..<baseText.endIndex
      }()
      doc.paragraphs[pIdx].applyRunFormatting(
        range: firstWordRange,
        isBold: fbBold,
        isItalic: fbItalic,
        isUnderlined: fbUnder,
        isStrikethrough: isStrikethrough,
        fontSizePt: fontSizePt,
        foregroundHex: fbColor,
        backgroundHex: backgroundHex,
        linkURL: linkURL
      )
    }
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "인라인 텍스트 런 서식 적용 완료"
  }

  public func applyInlineRunFormattingToActiveParagraph(
    isBold: Bool? = nil,
    isItalic: Bool? = nil,
    isUnderlined: Bool? = nil,
    textHex: String
  ) {
    applyInlineRunFormattingToActiveParagraph(
      isBold: isBold,
      isItalic: isItalic,
      isUnderlined: isUnderlined,
      foregroundHex: textHex
    )
  }

  private func activeTargetRunsForParagraph(_ paragraph: ParagraphBlock) -> [DocumentTextRun] {
    let sourceRuns = paragraph.effectiveRuns
    guard !sourceRuns.isEmpty else { return [] }
    let baseText = paragraph.text
    let targetCharRange: Range<Int>
    if let utf16Range = selectedParagraphUTF16Range, !utf16Range.isEmpty {
      let utf16View = baseText.utf16
      let lower = max(0, min(utf16Range.lowerBound, utf16View.count))
      let upper = max(lower, min(utf16Range.upperBound, utf16View.count))
      if lower < upper,
        let startIdx = utf16View.index(
          utf16View.startIndex, offsetBy: lower, limitedBy: utf16View.endIndex
        )?.samePosition(in: baseText),
        let endIdx = utf16View.index(
          utf16View.startIndex, offsetBy: upper, limitedBy: utf16View.endIndex
        )?.samePosition(in: baseText)
      {
        let startOffset = baseText.distance(from: baseText.startIndex, to: startIdx)
        let endOffset = baseText.distance(from: baseText.startIndex, to: endIdx)
        targetCharRange = startOffset..<endOffset
      } else {
        targetCharRange = 0..<baseText.count
      }
    } else {
      let trimmedPhrase = inlineRunTargetPhrase.trimmingCharacters(in: .whitespacesAndNewlines)
      if !trimmedPhrase.isEmpty, let matchRange = baseText.range(of: trimmedPhrase) {
        let startOffset = baseText.distance(from: baseText.startIndex, to: matchRange.lowerBound)
        let endOffset = baseText.distance(from: baseText.startIndex, to: matchRange.upperBound)
        targetCharRange = startOffset..<endOffset
      } else if let spaceIdx = baseText.firstIndex(of: " "), spaceIdx < baseText.endIndex {
        let endOffset = baseText.distance(
          from: baseText.startIndex, to: baseText.index(after: spaceIdx))
        targetCharRange = 0..<endOffset
      } else {
        targetCharRange = 0..<baseText.count
      }
    }
    var overlapping: [DocumentTextRun] = []
    var offset = 0
    for run in sourceRuns {
      let nextOffset = offset + run.text.count
      if nextOffset > targetCharRange.lowerBound && offset < targetCharRange.upperBound {
        overlapping.append(run)
      }
      offset = nextOffset
    }
    return overlapping.isEmpty ? sourceRuns : overlapping
  }

  public func insertHyperlinkIntoActiveParagraph(url: String? = nil) {
    let trimmed = (url ?? inlineLinkURLDraft).trimmingCharacters(in: .whitespacesAndNewlines)
    let rawCandidate = trimmed.isEmpty ? "https://easyoffice.local/reference" : trimmed
    guard let resolvedURL = Self.sanitizeSafeHyperlinkURL(rawCandidate) else {
      lastErrorMessage = "보안 정책상 허용되지 않는 하이퍼링크 스킴입니다 (https, http, mailto만 허용)."
      return
    }
    applyInlineRunFormattingToActiveParagraph(
      isUnderlined: true,
      foregroundHex: "#1E88E5",
      linkURL: resolvedURL
    )
    lastExecutedCommandSummary = "하이퍼링크 삽입 완료: \(resolvedURL)"
  }

  public func toggleHighlightOnActiveParagraph(colorHex: String? = nil) {
    let resolvedHex = colorHex ?? inlineHighlightColorHex
    var shouldClear = false
    if let tab = activeTab, case .document(let doc) = tab.payload, !doc.paragraphs.isEmpty {
      let pIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
      let runs = activeTargetRunsForParagraph(doc.paragraphs[pIdx])
      shouldClear =
        !runs.isEmpty
        && runs.allSatisfy { run in
          guard let bg = run.backgroundHex else { return false }
          return !bg.isEmpty
        }
    }
    applyInlineRunFormattingToActiveParagraph(backgroundHex: shouldClear ? "" : resolvedHex)
    lastExecutedCommandSummary =
      shouldClear ? "텍스트 형광펜 강조 해제" : "텍스트 형광펜 강조 적용: \(resolvedHex)"
  }

  public func toggleStrikethroughOnActiveParagraph() {
    var nextState = true
    if let tab = activeTab, case .document(let doc) = tab.payload, !doc.paragraphs.isEmpty {
      let pIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
      let runs = activeTargetRunsForParagraph(doc.paragraphs[pIdx])
      if !runs.isEmpty && runs.allSatisfy(\.isStrikethrough) {
        nextState = false
      }
    }
    applyInlineRunFormattingToActiveParagraph(isStrikethrough: nextState)
    lastExecutedCommandSummary = nextState ? "취소선 서식 적용 완료" : "취소선 서식 해제 완료"
  }

  public func adjustActiveParagraphIndent(
    firstLineDeltaPt: Double = 0,
    leftIndentDeltaPt: Double = 0
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      !doc.paragraphs.isEmpty
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let pIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
    doc.paragraphs[pIdx].firstLineIndentPt = max(
      0,
      min(144, doc.paragraphs[pIdx].firstLineIndentPt + firstLineDeltaPt)
    )
    doc.paragraphs[pIdx].leftIndentPt = max(
      0,
      min(144, doc.paragraphs[pIdx].leftIndentPt + leftIndentDeltaPt)
    )
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary =
      "문단 들여쓰기 조정 (첫 줄 \(Int(doc.paragraphs[pIdx].firstLineIndentPt))pt, 좌측 \(Int(doc.paragraphs[pIdx].leftIndentPt))pt)"
  }

  public func adjustActiveParagraphTypography(
    letterSpacingDelta: Double = 0.0,
    charScaleDelta: Double = 0.0,
    reset: Bool = false
  ) {
    guard let tab = activeTab,
      tab.moduleKind == .document || tab.moduleKind == .hwp,
      let idx = openTabs.firstIndex(where: { $0.id == tab.id }),
      case .document(var doc) = openTabs[idx].payload,
      !doc.paragraphs.isEmpty
    else { return }
    let pIdx = min(max(0, selectedParagraphIndex), doc.paragraphs.count - 1)
    recordUndoSnapshot(for: tab.id)
    if reset {
      doc.paragraphs[pIdx].letterSpacingPercent = 0.0
      doc.paragraphs[pIdx].charWidthScalePercent = 100.0
    } else {
      doc.paragraphs[pIdx].letterSpacingPercent = max(
        -50.0,
        min(50.0, doc.paragraphs[pIdx].letterSpacingPercent + letterSpacingDelta)
      )
      doc.paragraphs[pIdx].charWidthScalePercent = max(
        50.0,
        min(200.0, doc.paragraphs[pIdx].charWidthScalePercent + charScaleDelta)
      )
    }
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary =
      "문단 자간(\(Int(doc.paragraphs[pIdx].letterSpacingPercent))%) · 장평(\(Int(doc.paragraphs[pIdx].charWidthScalePercent))%) 조판 적용"
  }

  private func mutateActiveDocumentTable(
    tableIndex: Int? = nil,
    summary: (DocumentTableBlock) -> String,
    mutation: (inout DocumentTableBlock) -> Void
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      !doc.tables.isEmpty
    else { return }
    let resolvedIndex = min(max(0, tableIndex ?? selectedDocumentTableIndex), doc.tables.count - 1)
    selectedDocumentTableIndex = resolvedIndex
    recordUndoSnapshot(for: activeTabID)
    mutation(&doc.tables[resolvedIndex])
    if let cell = selectedDocumentTableCell, doc.tables[resolvedIndex].rowCount > 0,
      doc.tables[resolvedIndex].columnCount > 0
    {
      selectedDocumentTableCell = (
        row: min(max(0, cell.row), doc.tables[resolvedIndex].rowCount - 1),
        column: min(max(0, cell.column), doc.tables[resolvedIndex].columnCount - 1)
      )
    }
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = summary(doc.tables[resolvedIndex])
  }

  public func insertRowInDocumentTable(tableIndex: Int? = nil, at rowIndex: Int? = nil) {
    let targetRow = rowIndex ?? selectedDocumentTableCell.map { $0.row + 1 }
    mutateActiveDocumentTable(
      tableIndex: tableIndex,
      summary: { "표 행 삽입 완료 (총 \($0.rowCount)행)" },
      mutation: { $0.insertRow(at: targetRow) }
    )
  }

  public func deleteRowInDocumentTable(tableIndex: Int? = nil, at rowIndex: Int? = nil) {
    let targetRow = rowIndex ?? selectedDocumentTableCell?.row
    mutateActiveDocumentTable(
      tableIndex: tableIndex,
      summary: { "표 행 삭제 완료 (총 \($0.rowCount)행)" },
      mutation: { $0.deleteRow(at: targetRow) }
    )
  }

  public func insertColumnInDocumentTable(tableIndex: Int? = nil, at columnIndex: Int? = nil) {
    let targetCol = columnIndex ?? selectedDocumentTableCell.map { $0.column + 1 }
    mutateActiveDocumentTable(
      tableIndex: tableIndex,
      summary: { "표 열 삽입 완료 (총 \($0.columnCount)열)" },
      mutation: { $0.insertColumn(at: targetCol) }
    )
  }

  public func deleteColumnInDocumentTable(tableIndex: Int? = nil, at columnIndex: Int? = nil) {
    let targetCol = columnIndex ?? selectedDocumentTableCell?.column
    mutateActiveDocumentTable(
      tableIndex: tableIndex,
      summary: { "표 열 삭제 완료 (총 \($0.columnCount)열)" },
      mutation: { $0.deleteColumn(at: targetCol) }
    )
  }

  public func acceptDocumentRevision(id: UUID) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    doc.acceptRevision(id: id)
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "변경 추적 항목 승인 완료"
  }

  public func rejectDocumentRevision(id: UUID) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    doc.rejectRevision(id: id)
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "변경 추적 항목 반려 완료"
  }

  public func insertEndnoteIntoActiveDocument(content: String? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let nextNumber = doc.footnotes.count + 1
    let trimmed = (content ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    let text = trimmed.isEmpty ? "미주 \(nextNumber): 표준 문헌 및 출처 참조" : trimmed
    let anchorID = selectedParagraphID ?? doc.paragraphs.first?.id ?? UUID()
    doc.footnotes.append(
      FootnoteEntry(
        number: nextNumber,
        referenceParagraphIndex: 0,
        content: text,
        isEndnote: true,
        anchorParagraphID: anchorID
      )
    )
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "미주 삽입 완료 (#\(nextNumber))"
  }
}
