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
  private var storedSelectedParagraphIndex: Int = 0
  public var selectedParagraphID: UUID? = nil {
    didSet {
      if let selectedParagraphID,
        let tab = activeTab,
        case .document(let doc) = tab.payload,
        let idx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID })
      {
        storedSelectedParagraphIndex = idx
      }
    }
  }
  public var selectedParagraphIndex: Int {
    get {
      if let selectedParagraphID,
        let tab = activeTab,
        case .document(let doc) = tab.payload,
        let idx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID })
      {
        return idx
      }
      if let tab = activeTab,
        case .document(let doc) = tab.payload,
        !doc.paragraphs.isEmpty
      {
        return min(max(0, storedSelectedParagraphIndex), doc.paragraphs.count - 1)
      }
      return max(0, storedSelectedParagraphIndex)
    }
    set {
      storedSelectedParagraphIndex = max(0, newValue)
      if let tab = activeTab,
        case .document(let doc) = tab.payload,
        !doc.paragraphs.isEmpty
      {
        let clamped = min(max(0, newValue), doc.paragraphs.count - 1)
        selectedParagraphID = doc.paragraphs[clamped].id
      }
    }
  }
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
  public var findQueryText: String = "" {
    didSet {
      if findQueryText != oldValue {
        currentFindMatchIndex = 0
      }
    }
  }
  public var replaceQueryText: String = ""
  public var findQueryDraft: String {
    get { findQueryText }
    set { findQueryText = newValue }
  }
  public var replaceQueryDraft: String {
    get { replaceQueryText }
    set { replaceQueryText = newValue }
  }
  public var isFindCaseSensitive: Bool = false {
    didSet {
      if isFindCaseSensitive != oldValue {
        currentFindMatchIndex = 0
      }
    }
  }
  public var currentFindMatchIndex: Int = 0
  public var isFindReplaceBarVisible: Bool = false
  public var isFindReplaceBarPresented: Bool {
    get { isFindReplaceBarVisible }
    set { isFindReplaceBarVisible = newValue }
  }
  public var lastFindReplaceCount: Int = 0
  public var batchTargetFormat: DocumentFormat = .pdf
  public var lastBatchConversionReport: BatchConversionReport? = nil
  public var lastSecurityAuditReport: SecurityAuditReport? = nil
  public var lastScrubbedMetadataCount: Int = 0
  public var reviewStatsRefreshCount: Int = 0
  public var lastErrorMessage: String? = nil
  public var lastExecutedCommandSummary: String? = nil
  public var inlineHighlightColorHex: String = "#FFF59D"
  public var inlineLinkURLDraft: String = "https://easyoffice.local/reference"
  public var namedRangeNameDraft: String = "분기매출"
  public var namedRangeTargetDraft: String = "B2:B4"
  public var goalSeekTargetCellDraft: String = "B4"
  public var goalSeekTargetValueDraft: String = "500"
  public var goalSeekVariableCellDraft: String = "B2"
  public var isSlideSorterViewEnabled: Bool = false
  public var isSlideGridlinesVisible: Bool = false
  public var isAdvancedToolbarExpanded: Bool = false
  public var selectedHomeTemplateCategoryFilter: String = "전체"
  public var selectedHomeRecentFormatFilter: String = "전체"
  public var pdfRedactionPhraseDraft: String = "기밀"
  public var pdfFormFieldNameDraft: String = "승인자"
  public var pdfFormFieldValueDraft: String = "홍길동"
  public var isManuscriptGridModeEnabled: Bool = false
  public var mailMergeFieldKeyDraft: String = "수신처"
  public var mailMergeFieldValueDraft: String = "전략기획본부"
  public var isFirstColumnFrozen: Bool = false
  public var isDataBarsEnabled: Bool = true
  public var selectedPivotAggregation: PivotAggregationFunction = .sum
  public var sheetDelimiterDraft: String = ","
  public var selectedPDFStampPreset: String = "승인 (APPROVED)"
  public var bookmarkTitleDraft: String = "핵심 섹션 책갈피"
  public var isChartPanelVisible: Bool = true
  public var customChartTitleDraft: String = ""
  public var customChartRangeDraft: String = "A1:D4"
  public var pdfWatermarkDraft: String = "대외비 CONFIDENTIAL"
  public var isPDFInkOverlayExpanded: Bool = false

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

  public func undoActiveTab() {
    undo()
  }

  public func redoActiveTab() {
    redo()
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
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(let sheet) = openTabs[idx].payload
    else { return }
    activeInlineEditTargetKey = nil
    let previousPayload = openTabs[idx].payload
    var updatedSheet = sheet
    do {
      try updatedSheet.setCell(selectedSheetCell, rawValue: formulaBarDraft)
      if let rule = activeDataValidationRule {
        let trimmed = formulaBarDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        let isValid =
          trimmed.hasPrefix("=")
          ? rule.validate(updatedSheet.evaluate(at: selectedSheetCell))
          : rule.validate(rawValue: formulaBarDraft)
        guard isValid else {
          lastErrorMessage = "데이터 유효성 검사 실패: 입력값 '\(formulaBarDraft)'이(가) 허용 범위를 벗어났습니다."
          return
        }
      }
      var stack = undoStacks[activeTabID] ?? []
      stack.append(previousPayload)
      if stack.count > Self.maxUndoDepth {
        stack.removeFirst(stack.count - Self.maxUndoDepth)
      }
      undoStacks[activeTabID] = stack
      redoStacks[activeTabID] = []
      openTabs[idx].payload = .sheet(updatedSheet)
      openTabs[idx].isModified = true
      lastErrorMessage = nil
    } catch {
      lastErrorMessage = error.localizedDescription
    }
  }

  public func commitSheetFormulaBarEdit() {
    applyFormulaBarEdit()
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
    } else if selectedSheetRange.contains(":"),
      let range = try? CellRange(notation: selectedSheetRange),
      range.start != range.end
    {
      resolvedRange = range.a1Range
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
}
