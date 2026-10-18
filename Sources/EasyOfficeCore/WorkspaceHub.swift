import AppKit
import EasyOfficeDesignSystem
import Foundation
import Observation

/// Unified observable view model coordinating multi-tab editing, split view, ribbon state, undo/redo, file I/O, and document mutations.
@MainActor
@Observable
public final class WorkspaceViewModel {
  private static let maxUndoDepth = 50

  public var openTabs: [WorkspaceTabItem]
  public var activeTabID: UUID? {
    didSet {
      activeInlineEditTargetKey = nil
      if let activeTabID, splitSecondaryTabID == activeTabID { splitSecondaryTabID = nil }
    }
  }
  public var splitSecondaryTabID: UUID? = nil
  public var isShowingHomeHub: Bool = false
  public var activeHubSection: HubSection = .home
  public var activeRibbonCategory: RibbonCategory = .home
  public var isRibbonCollapsed: Bool = false
  public var themeMode: WorkspaceThemeMode = .lightLavender
  public var customTemplates: [OfficeTemplateDescriptor] = []
  public var showLeftNavigator: Bool = true
  public var showRightInspector: Bool = true
  public var searchQuery: String = ""
  public var zoomPercent: Double = 100.0
  public var selectedSheetCell: String = "D2"
  public var selectedSlideIndex: Int = 0
  public var selectedPDFPageIndex: Int = 0
  public var formulaBarDraft: String = ""
  public var isPresenterModeActive: Bool = false

  // Interactive editing, undo/redo, and studio state
  public var selectedParagraphID: UUID? = nil
  public var selectedParagraphUTF16Range: Range<Int>? = nil
  public var paragraphDraftText: String = ""
  public var inlineRunTargetPhrase: String = ""
  public var equationDraftText: String = "\\int_0^\\infty e^{-x^2} dx = \\frac{\\sqrt{\\pi}}{2}"
  public var watermarkDraftText: String = "대외비 (CONFIDENTIAL)"
  public var commentDraftText: String = ""
  public var customTemplateTitleDraft: String = ""
  public var mailMergeFields: [String: String] = ["부서명": "플랫폼아키텍처실", "수신자": "전사 기술위원회"]
  public var selectedSheetRange: String = "B2:D3"
  public var sheetAutoFilterColumn: Int = 0
  public var sheetAutoFilterQuery: String = ""
  public var selectedChartKind: SheetChartKind = .column
  public var isHeaderRowFrozen: Bool = false
  public var mergedSheetRangesByTab: [UUID: [String]] = [:]
  public var activeConditionalFormatRule: ConditionalFormatRule? = nil
  public var activeDataValidationRule: DataValidationRule? = nil
  public var isSnapGuidesEnabled: Bool = true
  public var presenterElapsedSeconds: Int = 95
  public var isPresenterTimerRunning: Bool = false
  public var isFullScreenSlideshowActive: Bool = false
  public var isLaserPointerEnabled: Bool = false
  public var selectedSlideElementID: UUID? = nil
  public var activeSnapGuides: [SnapGuideLine] = []
  public var isPDFInkDrawingModeEnabled: Bool = false
  public var pdfInkStrokeColorHex: String = "#5C6BC0"
  public var pdfInkLineWidth: Double = 2.4
  public var lastPivotGrandTotal: Double = 0
  public var authorName: String = "표준 문서 작성자"
  public var findQueryText: String = ""
  public var replaceQueryText: String = ""
  public var isFindReplaceBarVisible: Bool = false
  public var lastFindReplaceCount: Int = 0
  public var batchTargetFormat: DocumentFormat = .pdf
  public var lastBatchConversionReport: BatchConversionReport? = nil
  public var lastSecurityAuditReport: SecurityAuditReport? = nil
  public var lastScrubbedMetadataCount: Int = 0
  public var reviewStatsRefreshCount: Int = 0
  public var lastErrorMessage: String? = nil
  public var lastExecutedCommandSummary: String? = nil

  internal var undoStacks: [UUID: [OpenTabPayload]] = [:]
  internal var redoStacks: [UUID: [OpenTabPayload]] = [:]
  internal var customTemplatePayloads: [String: OpenTabPayload] = [:]
  internal var activeInlineEditTargetKey: String? = nil
  public internal(set) var sheetSortAscending: Bool = false

  public init() {
    let initialTabs = OfficeModuleKind.allCases.map {
      WorkspaceHubCatalog.instantiateSampleTab(for: $0)
    }
    self.openTabs = initialTabs
    self.activeTabID = initialTabs.first?.id
    self.formulaBarDraft = "=SUM(B2:C2)"
  }

  public var activeTab: WorkspaceTabItem? {
    guard let activeTabID else { return openTabs.first }
    return openTabs.first(where: { $0.id == activeTabID })
  }

  public var splitSecondaryTab: WorkspaceTabItem? {
    guard let splitSecondaryTabID, splitSecondaryTabID != activeTabID else { return nil }
    return openTabs.first(where: { $0.id == splitSecondaryTabID })
  }

  public var filteredTabs: [WorkspaceTabItem] {
    WorkspaceHubCatalog.filterTabs(openTabs, section: activeHubSection, query: searchQuery)
  }

  public var allAvailableTemplates: [OfficeTemplateDescriptor] {
    customTemplates + WorkspaceHubCatalog.builtInTemplates
  }

  public var selectedPDFPageNumber: Int {
    get {
      guard let tab = activeTab, case .pdf(let pdfDoc) = tab.payload,
        pdfDoc.pages.indices.contains(selectedPDFPageIndex)
      else {
        return max(1, selectedPDFPageIndex + 1)
      }
      return pdfDoc.pages[selectedPDFPageIndex].pageNumber
    }
    set { selectedPDFPageIndex = max(0, newValue - 1) }
  }

  // MARK: - Undo / Redo Stack

  public var canUndo: Bool {
    guard let id = activeTab?.id else { return false }
    return !(undoStacks[id]?.isEmpty ?? true)
  }

  public var canRedo: Bool {
    guard let id = activeTab?.id else { return false }
    return !(redoStacks[id]?.isEmpty ?? true)
  }

  public func recordUndoSnapshot(for tabID: UUID, coalescingTargetKey: String? = nil) {
    guard let idx = openTabs.firstIndex(where: { $0.id == tabID }) else { return }
    if let coalescingTargetKey {
      if activeInlineEditTargetKey == coalescingTargetKey { return }
      activeInlineEditTargetKey = coalescingTargetKey
    } else {
      activeInlineEditTargetKey = nil
    }
    var stack = undoStacks[tabID] ?? []
    stack.append(openTabs[idx].payload)
    if stack.count > Self.maxUndoDepth { stack.removeFirst(stack.count - Self.maxUndoDepth) }
    undoStacks[tabID] = stack
    redoStacks[tabID] = []
  }

  public func undo() {
    activeInlineEditTargetKey = nil
    guard let tabID = activeTab?.id,
      let idx = openTabs.firstIndex(where: { $0.id == tabID }),
      var stack = undoStacks[tabID],
      let previousPayload = stack.popLast()
    else { return }
    undoStacks[tabID] = stack
    var rStack = redoStacks[tabID] ?? []
    rStack.append(openTabs[idx].payload)
    redoStacks[tabID] = rStack
    openTabs[idx].payload = previousPayload
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "실행 취소 완료"
  }

  public func redo() {
    activeInlineEditTargetKey = nil
    guard let tabID = activeTab?.id,
      let idx = openTabs.firstIndex(where: { $0.id == tabID }),
      var rStack = redoStacks[tabID],
      let nextPayload = rStack.popLast()
    else { return }
    redoStacks[tabID] = rStack
    var uStack = undoStacks[tabID] ?? []
    uStack.append(openTabs[idx].payload)
    undoStacks[tabID] = uStack
    openTabs[idx].payload = nextPayload
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "다시 실행 완료"
  }

  // MARK: - Tab & Template Management

  public func openNewSampleTab(for module: OfficeModuleKind) {
    activeInlineEditTargetKey = nil
    var newTab = WorkspaceHubCatalog.instantiateSampleTab(for: module)
    newTab.lastOpenedISO8601 = OfficeDocumentModel.currentISO8601Timestamp()
    openTabs.append(newTab)
    activeTabID = newTab.id
    isShowingHomeHub = false
  }

  public func openTab(from template: OfficeTemplateDescriptor) {
    activeInlineEditTargetKey = nil
    if let savedPayload = customTemplatePayloads[template.id] {
      let newTab = WorkspaceTabItem(
        fileName: "\(template.title).\(template.format.rawValue)", format: template.format,
        lastOpenedISO8601: OfficeDocumentModel.currentISO8601Timestamp(), payload: savedPayload)
      openTabs.append(newTab)
      activeTabID = newTab.id
      isShowingHomeHub = false
      return
    }
    var sample = WorkspaceHubCatalog.instantiateSampleTab(for: template.moduleKind)
    sample.fileName = "\(template.title).\(template.format.rawValue)"
    sample.format = template.format
    sample.lastOpenedISO8601 = OfficeDocumentModel.currentISO8601Timestamp()
    if case .document(var doc) = sample.payload {
      doc.title = template.title
      doc.format = template.format
      sample.payload = .document(doc)
    }
    let instantiated = WorkspaceTabItem(
      fileName: sample.fileName, format: sample.format, lastOpenedISO8601: sample.lastOpenedISO8601,
      payload: sample.payload)
    openTabs.append(instantiated)
    activeTabID = instantiated.id
    isShowingHomeHub = false
  }

  public func closeTab(id: UUID) {
    activeInlineEditTargetKey = nil
    openTabs.removeAll { $0.id == id }
    undoStacks.removeValue(forKey: id)
    redoStacks.removeValue(forKey: id)
    mergedSheetRangesByTab.removeValue(forKey: id)
    if splitSecondaryTabID == id { splitSecondaryTabID = nil }
    if activeTabID == id { activeTabID = openTabs.first?.id }
    if splitSecondaryTabID == activeTabID { splitSecondaryTabID = nil }
    if openTabs.isEmpty { isShowingHomeHub = true }
  }

  public func togglePinTab(id: UUID) {
    guard let idx = openTabs.firstIndex(where: { $0.id == id }) else { return }
    openTabs[idx].isPinned.toggle()
  }

  public func toggleFavoriteTab(id: UUID) {
    guard let idx = openTabs.firstIndex(where: { $0.id == id }) else { return }
    openTabs[idx].isFavorite.toggle()
  }

  public func duplicateTab(id: UUID) {
    guard let source = openTabs.first(where: { $0.id == id }) else { return }
    let ext = source.format.rawValue
    let baseName =
      source.fileName.lowercased().hasSuffix(".\(ext)")
      ? String(source.fileName.dropLast(ext.count + 1)) : source.fileName
    let duplicated = WorkspaceTabItem(
      fileName: "\(baseName)_사본.\(ext)", format: source.format, isPinned: source.isPinned,
      isFavorite: source.isFavorite, isModified: true,
      lastOpenedISO8601: OfficeDocumentModel.currentISO8601Timestamp(), payload: source.payload)
    openTabs.append(duplicated)
    activeTabID = duplicated.id
    isShowingHomeHub = false
  }

  public func enableSplitView(secondaryTabID: UUID?) {
    guard let secondaryTabID, openTabs.contains(where: { $0.id == secondaryTabID }),
      secondaryTabID != activeTabID
    else {
      self.splitSecondaryTabID = nil
      return
    }
    self.splitSecondaryTabID = secondaryTabID
  }

  public func saveActiveTabAsCustomTemplate(
    title: String? = nil, subtitle: String = "현재 활성 문서 기반 커스텀 템플릿"
  ) {
    guard let current = activeTab else { return }
    let draft = customTemplateTitleDraft.trimmingCharacters(in: .whitespacesAndNewlines)
    let explicitTitle = (title ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    let resolvedTitle: String
    if !draft.isEmpty && (explicitTitle.isEmpty || explicitTitle == "사용자 정의 표준 양식") {
      resolvedTitle = draft
      customTemplateTitleDraft = ""
    } else if !explicitTitle.isEmpty {
      resolvedTitle = explicitTitle
    } else {
      resolvedTitle = "사용자 정의 표준 양식"
    }
    let descriptor = OfficeTemplateDescriptor(
      id: "custom-\(UUID().uuidString.lowercased())", title: resolvedTitle, subtitle: subtitle,
      format: current.format, isCustomUserTemplate: true)
    customTemplatePayloads[descriptor.id] = current.payload
    customTemplates.insert(descriptor, at: 0)
    lastExecutedCommandSummary = "사용자 템플릿 저장 완료: \(resolvedTitle)"
  }

  public func setZoom(_ percent: Double) {
    guard percent.isFinite else { return }
    zoomPercent = min(400.0, max(25.0, percent))
  }

  // MARK: - Real File Open / Export / Save I/O

  @discardableResult
  public func openDocument(data: Data, fileName: String, fileURL: URL? = nil) throws
    -> WorkspaceTabItem
  {
    let verified = try FormatDetector.verify(data: data, fileName: fileName)
    let format = verified.format
    let payload: OpenTabPayload

    switch format {
    case .docx:
      var doc = try OOXMLContainerCodec.decodeDocument(from: data)
      doc.format = format
      payload = .document(doc)
    case .odt:
      var doc = try ODFContainerCodec.decodeDocument(from: data)
      doc.format = format
      payload = .document(doc)
    case .hwpx, .hwtx:
      var doc = try HWPXContainerCodec.decodeDocument(from: data)
      doc.format = format
      payload = .document(doc)
    case .hwp, .hwt:
      var doc = try HWP5BinaryCodec.decodeDocument(from: data)
      doc.format = format
      payload = .document(doc)
    case .doc:
      var doc = try LegacyOLE2OfficeCodec.decodeWordBinary(from: data)
      doc.format = format
      payload = .document(doc)
    case .rtf:
      var doc = try RTFCodec.decodeDocument(from: data)
      doc.format = format
      payload = .document(doc)
    case .md, .txt:
      var doc = try CommonMarkCodec.decodeDocument(from: data, title: fileName)
      doc.format = format
      payload = .document(doc)
    case .xlsx: payload = .sheet(try OOXMLContainerCodec.decodeWorksheet(from: data))
    case .xls:
      payload = .sheet(try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: data, name: fileName))
    case .ods: payload = .sheet(try ODFContainerCodec.decodeWorksheet(from: data))
    case .csv:
      payload = .sheet(
        try CSVStateMachineCodec.decodeWorksheet(from: data, name: fileName, delimiter: ","))
    case .tsv:
      payload = .sheet(
        try CSVStateMachineCodec.decodeWorksheet(from: data, name: fileName, delimiter: "\t"))
    case .pptx, .ppsx: payload = .slide(try OOXMLContainerCodec.decodePresentation(from: data))
    case .ppt: payload = .slide(try LegacyOLE2OfficeCodec.decodePowerPointBinary(from: data))
    case .odp: payload = .slide(try ODFContainerCodec.decodePresentation(from: data))
    case .pdf: payload = .pdf(try PDFBinarySerializer.decode(from: data))
    }

    let newTab = WorkspaceTabItem(
      fileName: fileName, format: format,
      lastOpenedISO8601: OfficeDocumentModel.currentISO8601Timestamp(), fileURL: fileURL,
      payload: payload)
    openTabs.append(newTab)
    activeTabID = newTab.id
    isShowingHomeHub = false
    lastErrorMessage = nil
    return newTab
  }

  @discardableResult
  public func loadFromDisk(url: URL) throws -> WorkspaceTabItem {
    try openDocument(data: try Data(contentsOf: url), fileName: url.lastPathComponent, fileURL: url)
  }

  private func convertTab(id: UUID, targetFormat: DocumentFormat? = nil) throws
    -> ConversionJobResult
  {
    guard let tab = openTabs.first(where: { $0.id == id }) else {
      throw EasyOfficeError.emptyPayload
    }
    let fmt = targetFormat ?? tab.format
    let safeSourceName =
      tab.fileName.contains(".") ? tab.fileName : "\(tab.fileName).\(tab.format.fileExtension)"
    return try DocumentConversionOrchestrator.convert(
      ConversionJobRequest(
        sourceFileName: safeSourceName, sourceFormat: tab.format, targetFormat: fmt,
        payload: tab.payload))
  }

  public func exportTabData(id: UUID, targetFormat: DocumentFormat? = nil) throws -> Data {
    try convertTab(id: id, targetFormat: targetFormat).outputBytes
  }

  public func saveTabToDisk(id: UUID, to url: URL, targetFormat: DocumentFormat? = nil) throws {
    guard let idx = openTabs.firstIndex(where: { $0.id == id }) else {
      throw EasyOfficeError.emptyPayload
    }
    let fmt =
      targetFormat ?? DocumentFormat.from(fileNameOrExtension: url.lastPathComponent)
      ?? openTabs[idx].format
    let result = try convertTab(id: id, targetFormat: fmt)
    try result.outputBytes.write(to: url, options: .atomic)
    openTabs[idx].payload = result.convertedPayload
    openTabs[idx].isModified = false
    openTabs[idx].fileURL = url
    openTabs[idx].fileName = url.lastPathComponent
    openTabs[idx].format = fmt
    lastErrorMessage = nil
  }

  // MARK: - Interactive Document, Sheet, Slide, and PDF Mutations

  public func applyFormulaBarEdit() {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    activeInlineEditTargetKey = nil
    if let rule = activeDataValidationRule {
      let trimmed = formulaBarDraft.trimmingCharacters(in: .whitespacesAndNewlines)
      let isValid: Bool
      if trimmed.hasPrefix("=") {
        var probe = sheet
        isValid =
          (try? probe.setCell(selectedSheetCell, rawValue: formulaBarDraft)) != nil
          ? rule.validate(probe.evaluate(at: selectedSheetCell)) : false
      } else {
        isValid = rule.validate(rawValue: formulaBarDraft)
      }
      guard isValid else {
        lastErrorMessage = "데이터 유효성 검사 실패: 입력값 '\(formulaBarDraft)'이(가) 허용 범위를 벗어났습니다."
        return
      }
    }
    let previousPayload = openTabs[idx].payload
    do {
      try sheet.setCell(selectedSheetCell, rawValue: formulaBarDraft)
      var stack = undoStacks[activeTabID] ?? []
      stack.append(previousPayload)
      if stack.count > Self.maxUndoDepth { stack.removeFirst(stack.count - Self.maxUndoDepth) }
      undoStacks[activeTabID] = stack
      redoStacks[activeTabID] = []
      openTabs[idx].payload = .sheet(sheet)
      openTabs[idx].isModified = true
      lastErrorMessage = nil
    } catch {
      lastErrorMessage = error.localizedDescription
    }
  }

  public func setSelectedSheetCellNumberFormat(_ format: CellNumberFormat) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let coord =
      (try? CellAddress(a1Notation: selectedSheetCell))?.packedCoordinate
      ?? PackedCellCoordinate(row: 0, column: 0)
    var style = sheet.cellStyle(at: coord)
    style.numberFormat = format
    sheet.setCellStyle(style, at: coord)
    if let range = try? CellRange(notation: selectedSheetRange), range.start != range.end {
      sheet.updateCellStyleInRange(range) { $0.numberFormat = format }
    }
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "셀 숫자 서식 적용: \(format.rawValue)"
  }

  public func setSelectedSheetCellAlignment(_ alignment: CellHorizontalAlignment) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let coord =
      (try? CellAddress(a1Notation: selectedSheetCell))?.packedCoordinate
      ?? PackedCellCoordinate(row: 0, column: 0)
    var style = sheet.cellStyle(at: coord)
    style.alignment = alignment
    sheet.setCellStyle(style, at: coord)
    if let range = try? CellRange(notation: selectedSheetRange), range.start != range.end {
      sheet.updateCellStyleInRange(range) { $0.alignment = alignment }
    }
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "셀 가로 정렬 적용: \(alignment.rawValue)"
  }

  public func selectSheetCellOrRange(primaryCell: String, rangeNotation: String? = nil) {
    activeInlineEditTargetKey = nil
    selectedSheetCell = primaryCell
    selectedSheetRange = rangeNotation ?? primaryCell
    if let tab = activeTab, case .sheet(let sheet) = tab.payload {
      formulaBarDraft = sheet.rawValue(at: primaryCell)
    } else {
      formulaBarDraft = ""
    }
  }

  public func extendSheetSelection(to endCell: String) {
    selectedSheetRange =
      (selectedSheetCell == endCell) ? selectedSheetCell : "\(selectedSheetCell):\(endCell)"
  }

  public func performAutoFillOnActiveSheet(
    sourceRangeNotation: String? = nil, targetRangeNotation: String
  ) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    let srcStr = sourceRangeNotation ?? selectedSheetRange
    do {
      let srcRange = try CellRange(notation: srcStr)
      let dstRange = try CellRange(notation: targetRangeNotation)
      try sheet.autofill(sourceRange: srcRange, targetRange: dstRange)
      recordUndoSnapshot(for: activeTabID)
      selectedSheetRange = dstRange.a1Range
      openTabs[idx].payload = .sheet(sheet)
      openTabs[idx].isModified = true
      lastErrorMessage = nil
      lastExecutedCommandSummary = "시트 자동 채우기 완료 (\(srcRange.a1Range) → \(dstRange.a1Range))"
    } catch {
      lastErrorMessage = error.localizedDescription
    }
  }

  public func updateActiveSheetColumnWidth(column: Int, width: Double) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID, coalescingTargetKey: "colw:\(activeTabID):\(column)")
    sheet.setColumnWidth(width, at: column)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary =
      "열 너비 조정 (\(CellAddress.columnLabel(for: max(0, column)))열: \(Int(sheet.columnWidth(at: column)))pt)"
  }

  public func updateActiveSheetRowHeight(row: Int, height: Double) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID, coalescingTargetKey: "rowh:\(activeTabID):\(row)")
    sheet.setRowHeight(height, at: row)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "행 높이 조정 (\(row + 1)행: \(Int(sheet.rowHeight(at: row)))pt)"
  }

  public func setSelectedSheetCellComment(_ comment: String) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    let coord =
      (try? CellAddress(a1Notation: selectedSheetCell))?.packedCoordinate
      ?? PackedCellCoordinate(row: 0, column: 0)
    let trimmed = comment.trimmingCharacters(in: .whitespacesAndNewlines)
    let existing = sheet.sheetComments[coord] ?? ""
    guard existing != trimmed else { return }
    recordUndoSnapshot(
      for: activeTabID,
      coalescingTargetKey: "sheet-comment-\(activeTabID.uuidString)-\(coord.rawValue)")
    if trimmed.isEmpty {
      sheet.sheetComments.removeValue(forKey: coord)
    } else {
      sheet.sheetComments[coord] = trimmed
    }
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = trimmed.isEmpty ? "셀 메모 삭제 완료" : "셀 메모 저장 완료"
  }

  public func mergedSheetRanges(for tabID: UUID) -> [String] {
    var combined = mergedSheetRangesByTab[tabID] ?? []
    if let tab = openTabs.first(where: { $0.id == tabID }), case .sheet(let sheet) = tab.payload {
      for r in sheet.mergedRanges {
        let a1 = r.a1Range
        if !combined.contains(a1) { combined.append(a1) }
      }
    }
    return combined
  }

  public func toggleMergeOnSelectedSheetCell(targetRange: String? = nil) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let resolvedRange: String
    if let targetRange, !targetRange.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      resolvedRange = targetRange.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
    } else if let addr = try? CellAddress(a1Notation: selectedSheetCell),
      let nextAddr = try? CellAddress(
        column: min(CellAddress.maxZeroBasedColumn, addr.column + 1), row: addr.row)
    {
      resolvedRange = "\(addr.a1Notation):\(nextAddr.a1Notation)"
    } else {
      resolvedRange = "\(selectedSheetCell.uppercased()):\(selectedSheetCell.uppercased())"
    }
    if let parsedRange = try? CellRange(notation: resolvedRange) {
      sheet.toggleMergeCells(parsedRange)
    }
    var ranges = mergedSheetRangesByTab[activeTabID] ?? []
    if let existingIdx = ranges.firstIndex(of: resolvedRange) {
      ranges.remove(at: existingIdx)
      lastExecutedCommandSummary = "셀 병합 해제: \(resolvedRange)"
    } else {
      ranges.append(resolvedRange)
      let topLeftA1 = resolvedRange.components(separatedBy: ":").first ?? selectedSheetCell
      let coord =
        (try? CellAddress(a1Notation: topLeftA1))?.packedCoordinate
        ?? PackedCellCoordinate(row: 0, column: 0)
      var style = sheet.cellStyle(at: coord)
      style.alignment = .center
      sheet.setCellStyle(style, at: coord)
      lastExecutedCommandSummary = "셀 병합 적용: \(resolvedRange)"
    }
    openTabs[idx].payload = .sheet(sheet)
    mergedSheetRangesByTab[activeTabID] = ranges
    openTabs[idx].isModified = true
  }

  public func addWorksheetTab(named: String? = nil) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let resolvedName = named ?? "Sheet\(sheet.allSheetNames.count + 1)"
    sheet.addSheetPage(
      named: resolvedName,
      initialCells: [
        PackedCellCoordinate(row: 0, column: 0): "항목", PackedCellCoordinate(row: 0, column: 1): "값",
      ])
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "새 시트 탭 추가: \(resolvedName)"
  }

  public func selectWorksheetTab(index: Int) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    activeInlineEditTargetKey = nil
    sheet.switchActiveSheet(to: index)
    formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
    openTabs[idx].payload = .sheet(sheet)
    lastExecutedCommandSummary = "시트 탭 전환: \(sheet.name)"
  }

  public func updateSelectedParagraphSelection(paragraphID: UUID, utf16Range: Range<Int>?) {
    selectedParagraphID = paragraphID
    selectedParagraphUTF16Range = utf16Range
  }

  public func appendParagraphToActiveDocument(text: String, style: ParagraphStyleRole = .body) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let block = ParagraphBlock(
      style: style, alignment: style == .officialHeader ? .center : .justified, text: text,
      fontSizePt: style == .title ? 20.0 : (style == .heading1 ? 14.0 : 11.0),
      isBold: style != .body)
    doc.paragraphs.append(block)
    selectedParagraphID = block.id
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
  }

  public func updateParagraphText(paragraphID: UUID, newText: String) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
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
          author: authorName, kind: .formattingChange, paragraphID: paragraphID,
          proposedText: newText)
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
        for clearIdx in (firstChangedIdx + 1)...lastChangedIdx { updatedRuns[clearIdx].text = "" }
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
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      let tIdx = doc.tables.firstIndex(where: { $0.id == tableID }),
      doc.tables[tIdx].rows.indices.contains(row),
      doc.tables[tIdx].rows[row].indices.contains(column),
      doc.tables[tIdx].rows[row][column] != newValue
    else { return }
    recordUndoSnapshot(
      for: activeTabID,
      coalescingTargetKey: "table-\(activeTabID.uuidString)-\(tableID.uuidString)-\(row)-\(column)")
    doc.tables[tIdx].rows[row][column] = newValue
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
  }

  public func updateTableCell(tableID: UUID, row: Int, col: Int, newValue: String) {
    updateTableCell(tableID: tableID, row: row, column: col, newValue: newValue)
  }

  public func insertEquationIntoActiveDocument(expression: String? = nil, caption: String = "수식 블록")
  {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    let candidate = (expression ?? equationDraftText).trimmingCharacters(
      in: .whitespacesAndNewlines)
    let expr = candidate.isEmpty ? "E = mc^2" : candidate
    recordUndoSnapshot(for: activeTabID)
    doc.insertEquation(rawExpression: expr, caption: caption)
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "수식 블록 삽입 완료: \(caption)"
  }

  public func updateDocumentEquation(id: UUID, newExpression: String) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
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
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload, !doc.images.isEmpty
    else { return }
    let imgIdx = imageID.flatMap { id in doc.images.firstIndex(where: { $0.id == id }) } ?? 0
    recordUndoSnapshot(for: activeTabID)
    let allModes = ImageWrapMode.allCases
    let next = allModes[
      ((allModes.firstIndex(of: doc.images[imgIdx].wrapMode) ?? 0) + 1) % allModes.count]
    doc.images[imgIdx].wrapMode = next
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "이미지 배치 모드 전환: \(next.rawValue)"
  }

  public func setDocumentWatermark(_ text: String? = nil) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let trimmed = (text ?? watermarkDraftText).trimmingCharacters(in: .whitespacesAndNewlines)
    doc.watermarkText = trimmed.isEmpty ? nil : trimmed
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = trimmed.isEmpty ? "문서 워터마크 해제" : "문서 워터마크 적용: \(trimmed)"
  }

  private static func makeSampleLavenderDiagramPNG(width: Int = 160, height: Int = 48) -> Data {
    guard
      let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: width * 4, bitsPerPixel: 32
      ), let ptr = rep.bitmapData
    else { return Data() }
    for y in 0..<height {
      for x in 0..<width {
        let offset = (y * width + x) * 4
        let isBorder = x < 2 || x >= width - 2 || y < 2 || y >= height - 2
        let isBar1 = (x >= 20 && x < 48 && y >= 14 && y < height - 8)
        let isBar2 = (x >= 64 && x < 92 && y >= 8 && y < height - 8)
        let isBar3 = (x >= 108 && x < 136 && y >= 18 && y < height - 8)
        if isBorder || isBar2 {
          ptr[offset] = 92
          ptr[offset + 1] = 107
          ptr[offset + 2] = 192
          ptr[offset + 3] = 255
        } else if isBar1 || isBar3 {
          ptr[offset] = 121
          ptr[offset + 1] = 134
          ptr[offset + 2] = 203
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

  public func insertSampleImageAttachment(
    caption: String = "아키텍처 다이어그램 이미지", widthPt: Double = 320, heightPt: Double = 180
  ) {
    let pngData = Self.makeSampleLavenderDiagramPNG(width: 160, height: 48)
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(let doc) = openTabs[idx].payload
    else { return }
    importImageAttachment(
      data: pngData, fileName: "diagram_\(doc.images.count + 1).png", caption: caption,
      widthPt: widthPt, heightPt: heightPt)
  }

  public func insertEmbeddedImageIntoActiveDocument(
    caption: String = "아키텍처 다이어그램 이미지", widthPt: Double = 320, heightPt: Double = 180
  ) { insertSampleImageAttachment(caption: caption, widthPt: widthPt, heightPt: heightPt) }

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
    data: Data, fileName: String, caption: String = "", widthPt: Double? = nil,
    heightPt: Double? = nil
  ) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    guard !data.isEmpty, data.count <= Self.maxImageAttachmentBytes,
      let mimeType = Self.detectRasterImageMimeType(data: data, fileName: fileName),
      let bitmapRep = NSBitmapImageRep(data: data), bitmapRep.pixelsWide > 0,
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
        fileName: fileName, caption: resolvedCaption, mimeType: mimeType, imageData: data,
        widthPt: resolvedW, heightPt: resolvedH,
        anchorParagraphID: selectedParagraphID ?? doc.paragraphs.first?.id))
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "임베디드 이미지 첨부 완료: \(resolvedCaption)"
  }

  public func loadImageAttachmentFromDisk(url: URL) throws {
    let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
    guard values.isRegularFile == true, let fileSize = values.fileSize, fileSize > 0,
      fileSize <= Self.maxImageAttachmentBytes
    else {
      throw EasyOfficeError.corruptedContainer(detail: "유효한 일반 이미지 파일이 아니거나 크기 제한(32MB)을 초과합니다.")
    }
    let data = try Data(contentsOf: url, options: [.mappedIfSafe])
    guard Self.detectRasterImageMimeType(data: data, fileName: url.lastPathComponent) != nil,
      let bitmapRep = NSBitmapImageRep(data: data), bitmapRep.pixelsWide > 0,
      bitmapRep.pixelsHigh > 0
    else {
      throw EasyOfficeError.corruptedContainer(detail: "유효하지 않은 이미지 파일이거나 매직 바이트와 확장자가 일치하지 않습니다.")
    }
    importImageAttachment(
      data: data, fileName: url.lastPathComponent,
      caption: url.deletingPathExtension().lastPathComponent)
  }

  public func applyInlineRunFormattingToActiveParagraph(
    isBold: Bool? = nil, isItalic: Bool? = nil, isUnderlined: Bool? = nil, textHex: String? = nil,
    foregroundHex: String? = nil
  ) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload, !doc.paragraphs.isEmpty
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let pIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
    let resolvedColor = foregroundHex ?? textHex
    if let utf16Range = selectedParagraphUTF16Range, !utf16Range.isEmpty {
      doc.paragraphs[pIdx].applyRunFormatting(
        utf16Range: utf16Range, isBold: isBold, isItalic: isItalic, isUnderlined: isUnderlined,
        foregroundHex: resolvedColor)
      openTabs[idx].payload = .document(doc)
      openTabs[idx].isModified = true
      lastExecutedCommandSummary =
        "선택 구간(\(utf16Range.lowerBound)..<\(utf16Range.upperBound)) 부분 서식 적용"
      return
    }
    let baseText = doc.paragraphs[pIdx].text
    let trimmedPhrase = inlineRunTargetPhrase.trimmingCharacters(in: .whitespacesAndNewlines)
    let fbBold = isBold ?? true
    let fbItalic = isItalic ?? false
    let fbUnder = isUnderlined ?? false
    let fbColor = resolvedColor ?? "#5C6BC0"
    if !trimmedPhrase.isEmpty, let matchRange = baseText.range(of: trimmedPhrase) {
      doc.paragraphs[pIdx].applyRunFormatting(
        range: matchRange, isBold: fbBold, isItalic: fbItalic, isUnderlined: fbUnder,
        foregroundHex: fbColor)
    } else {
      doc.paragraphs[pIdx].runs = []
      let firstWordRange: Range<String.Index> = {
        if let spaceIdx = baseText.firstIndex(of: " "), spaceIdx < baseText.endIndex {
          return baseText.startIndex..<baseText.index(after: spaceIdx)
        }
        return baseText.startIndex..<baseText.endIndex
      }()
      doc.paragraphs[pIdx].applyRunFormatting(
        range: firstWordRange, isBold: fbBold, isItalic: fbItalic, isUnderlined: fbUnder,
        foregroundHex: fbColor)
    }
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "인라인 텍스트 런 서식 적용 완료"
  }

  public func selectSlideElement(id: UUID?) {
    activeInlineEditTargetKey = nil
    selectedSlideElementID = id
  }

  public func moveSlideElement(
    slideIndex: Int? = nil, elementID: UUID, deltaX: Double, deltaY: Double
  ) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
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
      for: elem, siblings: siblings, aspectRatio: deck.aspectRatio, tolerancePoints: 5.0)
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
  }

  public func resizeSlideElement(
    slideIndex: Int? = nil, elementID: UUID, deltaWidth: Double, deltaHeight: Double
  ) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
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
    slideIndex: Int? = nil, elementID: UUID, handle: SlideResizeHandle, deltaX: Double,
    deltaY: Double
  ) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    let sIdx =
      slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
    guard deck.slides.indices.contains(sIdx),
      let eIdx = deck.slides[sIdx].elements.firstIndex(where: { $0.id == elementID })
    else { return }
    recordUndoSnapshot(for: activeTabID)
    deck.slides[sIdx].resizeElement(
      id: elementID, handle: handle, deltaX: deltaX, deltaY: deltaY,
      stageSize: deck.aspectRatio.stageSize)
    let updatedElem = deck.slides[sIdx].elements[eIdx]
    let siblings = deck.slides[sIdx].elements.enumerated().compactMap { $0 == eIdx ? nil : $1 }
    activeSnapGuides = SnapGuideCalculator.computeGuides(
      for: updatedElem, siblings: siblings, aspectRatio: deck.aspectRatio, tolerancePoints: 5.0)
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 개체 핸들 크기 조정 (\(handle.rawValue))"
  }

  public func rotateSlideElement(slideIndex: Int? = nil, elementID: UUID, toDegrees degrees: Double)
  {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
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
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
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
        % allAnims.count]
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
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    let targetSlideIdx =
      slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
    guard deck.slides.indices.contains(targetSlideIdx),
      let eIdx = deck.slides[targetSlideIdx].elements.firstIndex(where: { $0.id == elementID })
    else { return }
    let nextIdx =
      bringForward
      ? min(deck.slides[targetSlideIdx].elements.count - 1, eIdx + 1) : max(0, eIdx - 1)
    guard nextIdx != eIdx else { return }
    recordUndoSnapshot(for: activeTabID)
    deck.slides[targetSlideIdx].elements.swapAt(eIdx, nextIdx)
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = bringForward ? "슬라이드 개체 앞으로 이동" : "슬라이드 개체 뒤로 이동"
  }

  public func updateSlideElementText(
    slideIndex: Int? = nil, elementID: UUID, title: String? = nil, bodyText: String? = nil
  ) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
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
      coalescingTargetKey: "slide-elem-\(activeTabID.uuidString)-\(elementID.uuidString)")
    deck.slides[targetSlideIdx].elements[eIdx].title = nextTitle
    deck.slides[targetSlideIdx].elements[eIdx].bodyText = nextBody
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
  }

  public func updateSlideSpeakerNotes(slideIndex: Int? = nil, notes: String) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    let targetSlideIdx =
      slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
    guard deck.slides.indices.contains(targetSlideIdx),
      deck.slides[targetSlideIdx].speakerNotes != notes
    else { return }
    recordUndoSnapshot(
      for: activeTabID,
      coalescingTargetKey: "slide-notes-\(activeTabID.uuidString)-\(targetSlideIdx)")
    deck.slides[targetSlideIdx].speakerNotes = notes
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
  }

  public func addSlideBulletPoint(slideIndex: Int? = nil, text: String) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
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
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
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

  public func rotateActivePDFPage(clockwiseDegrees: Int = 90) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdfDoc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let targetIndex = pdfDoc.pages.indices.contains(selectedPDFPageIndex) ? selectedPDFPageIndex : 0
    pdfDoc.rotatePage(at: targetIndex, clockwiseDegrees: clockwiseDegrees)
    openTabs[idx].payload = .pdf(pdfDoc)
    openTabs[idx].isModified = true
  }

  public func insertBlankPDFPage(afterPage: Int? = nil) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdfDoc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let targetAfterPage = afterPage ?? selectedPDFPageNumber
    pdfDoc.insertBlankPage(afterPage: targetAfterPage)
    selectedPDFPageIndex = min(pdfDoc.pages.count - 1, targetAfterPage)
    openTabs[idx].payload = .pdf(pdfDoc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "빈 PDF 페이지 추가 완료"
  }

  public func deleteActivePDFPage() {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdfDoc) = openTabs[idx].payload, pdfDoc.pages.count > 1
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let targetIndex = pdfDoc.pages.indices.contains(selectedPDFPageIndex) ? selectedPDFPageIndex : 0
    pdfDoc.deletePage(at: targetIndex)
    selectedPDFPageIndex = max(0, min(pdfDoc.pages.count - 1, selectedPDFPageIndex))
    openTabs[idx].payload = .pdf(pdfDoc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "현재 PDF 페이지 삭제 완료"
  }

  public func addPDFAnnotation(kind: PDFAnnotationKind, content: String) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdfDoc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let pageNumber =
      pdfDoc.pages.indices.contains(selectedPDFPageIndex)
      ? pdfDoc.pages[selectedPDFPageIndex].pageNumber : 1
    pdfDoc.annotations.append(
      PDFAnnotationItem(pageNumber: pageNumber, kind: kind, content: content, author: authorName))
    openTabs[idx].payload = .pdf(pdfDoc)
    openTabs[idx].isModified = true
  }

  public func addInkStrokeToActivePDFPage(
    points: [PDFInkPoint], strokeHex: String? = nil, lineWidth: Double? = nil,
    label: String = "자유곡선 전자서명"
  ) {
    guard !points.isEmpty, let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdfDoc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    pdfDoc.addInkStroke(
      pageNumber: selectedPDFPageNumber, points: points,
      strokeHex: strokeHex ?? pdfInkStrokeColorHex, lineWidth: lineWidth ?? pdfInkLineWidth,
      label: label)
    openTabs[idx].payload = .pdf(pdfDoc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "PDF 자유곡선 벡터 서명 추가 (\(selectedPDFPageNumber)p)"
  }

  public func clearInkStrokesOnActivePDFPage() {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdfDoc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    pdfDoc.clearInkStrokes(onPage: selectedPDFPageNumber)
    openTabs[idx].payload = .pdf(pdfDoc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "PDF 자유곡선 서명 지우기 (\(selectedPDFPageNumber)p)"
  }

  public func addHighlightToActivePDFPage(selectedText: String) {
    let trimmed = selectedText.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty, let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdfDoc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    pdfDoc.addAnnotation(
      pageNumber: selectedPDFPageNumber, kind: .highlight, author: authorName,
      contents: "하이라이트: \(trimmed)")
    openTabs[idx].payload = .pdf(pdfDoc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "PDF 선택 텍스트 하이라이트 추가 (\(selectedPDFPageNumber)p)"
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

extension PDFAnnotationItem {
  public var contents: String {
    get { content }
    set { content = newValue }
  }
}

extension PDFStudioDocumentModel {
  public mutating func addInkStroke(
    pageNumber: Int, points: [PDFInkPoint], strokeHex: String = "#5C6BC0", lineWidth: Double = 2.2,
    label: String = "자유곡선 전자서명"
  ) {
    addInkStroke(
      PDFInkStroke(
        pageNumber: pageNumber, points: points, strokeHex: strokeHex, lineWidth: lineWidth,
        label: label))
  }
  public mutating func addAnnotation(
    pageNumber: Int, kind: PDFAnnotationKind, author: String = "Workspace User", contents: String
  ) {
    annotations.append(
      PDFAnnotationItem(pageNumber: pageNumber, kind: kind, content: contents, author: author))
  }
}
