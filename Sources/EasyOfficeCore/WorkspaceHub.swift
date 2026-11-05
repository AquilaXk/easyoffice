import AppKit
import EasyOfficeDesignSystem
import Foundation
import Observation

/// Unified observable view model coordinating multi-tab editing, split view, ribbon state, undo/redo, file I/O, and document mutations.
@MainActor
@Observable
public final class WorkspaceViewModel {
  internal static let maxUndoDepth = 50

  public var openTabs: [WorkspaceTabItem]
  public var activeTabID: UUID? {
    didSet {
      activeInlineEditTargetKey = nil
      if let activeTabID, splitSecondaryTabID == activeTabID { splitSecondaryTabID = nil }
      clampSelectionIndicesForActiveTab()
      syncSheetRulesFromActiveTab()
    }
  }
  public var splitSecondaryTabID: UUID? = nil
  public var isShowingHomeHub: Bool = false
  public var isCanvasSnapshotMode: Bool = false
  public var activeHubSection: HubSection = .home
  public var activeRibbonCategory: RibbonCategory = .home
  public var ribbonLayoutStyle: RibbonLayoutStyle = .standardOffice
  public var isRibbonCollapsed: Bool = false
  public var themeMode: WorkspaceThemeMode = .lightLavender
  public var usesDarkMode: Bool {
    get { themeMode == .darkIndigo }
    set { themeMode = newValue ? .darkIndigo : .lightLavender }
  }
  public func toggleDarkMode() {
    usesDarkMode.toggle()
    lastExecutedCommandSummary = "테마 모드 전환: \(themeMode.rawValue)"
  }
  public var customTemplates: [OfficeTemplateDescriptor] = []
  public var showLeftNavigator: Bool = true
  public var showRightInspector: Bool = true
  internal var hasPendingTopSearchQuery: Bool = false
  public var searchQuery: String = "" {
    didSet {
      let trimmed = searchQuery.trimmingCharacters(in: .whitespacesAndNewlines)
      guard !isShowingHomeHub, !isSyncingFindReplaceState, !trimmed.isEmpty else { return }
      hasPendingTopSearchQuery = true
      isSyncingFindReplaceState = true
      defer { isSyncingFindReplaceState = false }
      if findQueryText != searchQuery {
        findQueryText = searchQuery
      }
      if findQueryDraft != searchQuery {
        findQueryDraft = searchQuery
      }
      isFindReplaceBarVisible = true
    }
  }
  public var zoomPercent: Double = 100.0
  public var selectedSheetCell: String = "D2"
  public var selectedCellAddress: String {
    get { selectedSheetCell }
    set { selectedSheetCell = newValue }
  }
  public var selectedSlideIndex: Int = 0
  public var selectedPDFPageIndex: Int = 0
  public var selectedDocumentTableIndex: Int = 0 {
    didSet {
      if selectedDocumentTableIndex != oldValue {
        selectedDocumentTableCell = nil
      }
    }
  }
  public var selectedDocumentTableCell: (row: Int, column: Int)? = nil
  public var formulaBarDraft: String = ""
  public var isPresenterModeActive: Bool = false
  public var workspaceViewMode: WorkspaceViewMode = .editView
  public var activeInspectorSubTab: InspectorSubTab = .propertiesAndLayout

  internal var storedSelectedParagraphIndex: Int = 0
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
  public var pendingFocusParagraphID: UUID? = nil
  public var pendingCaretUTF16Offset: Int? = nil
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
  internal var isSyncingSheetRulesState: Bool = false
  public var activeConditionalFormatRule: ConditionalFormatRule? = nil {
    didSet {
      guard !isSyncingSheetRulesState else { return }
      persistSheetRulesToActiveTab()
    }
  }
  public var activeDataValidationRule: DataValidationRule? = nil {
    didSet {
      guard !isSyncingSheetRulesState else { return }
      persistSheetRulesToActiveTab()
    }
  }
  public var isSnapGuidesEnabled: Bool = true
  public var presenterElapsedSeconds: Int = 0
  public var isPresenterTimerRunning: Bool = false
  public var isFullScreenSlideshowActive: Bool = false
  public var isLaserPointerEnabled: Bool = false
  public var selectedSlideElementID: UUID? = nil
  public var activeSnapGuides: [SnapGuideLine] = []
  public var isPDFInkDrawingModeEnabled: Bool = false
  public var pdfInkStrokeColorHex: String = "#5C6BC0"
  public var pdfInkLineWidth: Double = 2.4
  public var pdfPaperSizePreset: PaperSizePreset = .a4
  public var pdfPageColumnCount: Int = 1
  public var selectedFunctionWizardName: String = "AVERAGE"
  public var lastPivotGrandTotal: Double = 0
  public var authorName: String = "표준 문서 작성자"
  internal var isSyncingFindReplaceState: Bool = false
  public var findQueryText: String = "" {
    didSet {
      if findQueryText != oldValue {
        currentFindMatchIndex = 0
      }
      guard !isSyncingFindReplaceState else { return }
      hasPendingTopSearchQuery = false
      isSyncingFindReplaceState = true
      defer { isSyncingFindReplaceState = false }
      if findQueryDraft != findQueryText {
        findQueryDraft = findQueryText
      }
    }
  }
  public var replaceQueryText: String = "" {
    didSet {
      guard !isSyncingFindReplaceState else { return }
      isSyncingFindReplaceState = true
      defer { isSyncingFindReplaceState = false }
      if replaceQueryDraft != replaceQueryText {
        replaceQueryDraft = replaceQueryText
      }
    }
  }
  public var findQueryDraft: String = "" {
    didSet {
      if findQueryDraft != oldValue {
        currentFindMatchIndex = 0
      }
      guard !isSyncingFindReplaceState else { return }
      hasPendingTopSearchQuery = false
      isSyncingFindReplaceState = true
      defer { isSyncingFindReplaceState = false }
      if findQueryText != findQueryDraft {
        findQueryText = findQueryDraft
      }
    }
  }
  public var replaceQueryDraft: String = "" {
    didSet {
      guard !isSyncingFindReplaceState else { return }
      isSyncingFindReplaceState = true
      defer { isSyncingFindReplaceState = false }
      if replaceQueryText != replaceQueryDraft {
        replaceQueryText = replaceQueryDraft
      }
    }
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
  public var batchTargetFormat: DocumentFormat = .pdf {
    didSet {
      if batchTargetFormat != oldValue {
        batchQueueGeneration &+= 1
        lastBatchConversionReport = nil
      }
    }
  }
  public var batchOutputDirectoryURL: URL? = nil
  public var additionalBatchJobs: [ConversionJobRequest] = []
  public var excludedOpenTabBatchIDs: Set<UUID> = []
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
  public var isHWPManuscriptPaperMode: Bool {
    get { isManuscriptGridModeEnabled }
    set { isManuscriptGridModeEnabled = newValue }
  }
  public var activeSmartUtility: HubSection {
    get { activeHubSection }
    set { activeHubSection = newValue }
  }
  public var mailMergeFieldKeyDraft: String = "수신처"
  public var mailMergeFieldValueDraft: String = "전략기획본부"
  public var isFirstColumnFrozen: Bool = false
  public var isDataBarsEnabled: Bool = false
  public var showSheetDataBars: Bool {
    get { isDataBarsEnabled }
    set { isDataBarsEnabled = newValue }
  }
  public var selectedPivotAggregation: PivotAggregationFunction = .sum
  public var sheetDelimiterDraft: String = ","
  public var selectedPDFStampPreset: String = "승인 (APPROVED)"
  public var bookmarkTitleDraft: String = "핵심 섹션 책갈피"
  public var isChartPanelVisible: Bool = true
  public var customChartTitleDraft: String = ""
  public var customChartRangeDraft: String = "A1:D4"
  public var pdfWatermarkDraft: String = "대외비 CONFIDENTIAL"
  public var isPDFInkOverlayExpanded: Bool = false
  public var documentHorizontalPadding: Double = 28.0
  public var collapsedOutlineParagraphIDs: Set<UUID> = []
  public var pendingCloseConfirmTabID: UUID? = nil
  public var pendingLossyOverwriteConfirmationTabID: UUID? = nil
  public var isPendingApplicationTermination: Bool = false
  public var isWorkspaceReadyForOpenURLs: Bool = true
  public var pendingLaunchURLs: [URL] = []
  public var recentlyClosedTabs: [WorkspaceTabItem] = []
  public var recentDocumentRecords: [WorkspaceTabItem] = []

  internal var undoStacks: [UUID: [OpenTabPayload]] = [:]
  internal var redoStacks: [UUID: [OpenTabPayload]] = [:]
  internal var undoMetadataStacks: [UUID: [TabMetadataSnapshot]] = [:]
  internal var redoMetadataStacks: [UUID: [TabMetadataSnapshot]] = [:]
  internal var batchQueueGeneration: Int = 0
  internal var customTemplatePayloads: [String: OpenTabPayload] = [:]
  internal var knownEasyOfficeSavedFilePaths: Set<String> = []
  internal var activeInlineEditTargetKey: String? = nil
  internal var blankTabCounter: Int = 0
  public internal(set) var sheetSortAscending: Bool = false

  public init() {
    let initialTabs = OfficeModuleKind.allCases.map {
      WorkspaceHubCatalog.instantiateSampleTab(for: $0)
    }
    self.openTabs = initialTabs
    self.recentDocumentRecords = initialTabs
    self.activeTabID = initialTabs.first?.id
    self.formulaBarDraft = "=SUM(B2:C2)"
  }

  internal func clampSelectionIndicesForActiveTab() {
    guard let tab = activeTab else {
      selectedSlideIndex = 0
      selectedSlideElementID = nil
      selectedPDFPageIndex = 0
      return
    }
    switch tab.payload {
    case .slide(let deck):
      if deck.slides.isEmpty {
        selectedSlideIndex = 0
        selectedSlideElementID = nil
      } else {
        selectedSlideIndex = min(max(0, selectedSlideIndex), deck.slides.count - 1)
        let currentElements = deck.slides[selectedSlideIndex].elements
        if let selID = selectedSlideElementID, !currentElements.contains(where: { $0.id == selID })
        {
          selectedSlideElementID = nil
        }
      }
      selectedPDFPageIndex = 0
      selectedDocumentTableIndex = 0
      selectedDocumentTableCell = nil
    case .pdf(let pdfDoc):
      selectedPDFPageIndex =
        pdfDoc.pages.isEmpty ? 0 : min(max(0, selectedPDFPageIndex), pdfDoc.pages.count - 1)
      selectedSlideIndex = 0
      selectedSlideElementID = nil
      selectedDocumentTableIndex = 0
      selectedDocumentTableCell = nil
    case .document(let doc):
      if let selParaID = selectedParagraphID,
        !doc.paragraphs.contains(where: { $0.id == selParaID })
      {
        selectedParagraphID = doc.paragraphs.first?.id
      }
      if doc.tables.isEmpty {
        selectedDocumentTableIndex = 0
        selectedDocumentTableCell = nil
      } else {
        selectedDocumentTableIndex = min(max(0, selectedDocumentTableIndex), doc.tables.count - 1)
        let table = doc.tables[selectedDocumentTableIndex]
        if let cell = selectedDocumentTableCell, table.rowCount > 0, table.columnCount > 0 {
          selectedDocumentTableCell = (
            row: min(max(0, cell.row), table.rowCount - 1),
            column: min(max(0, cell.column), table.columnCount - 1)
          )
        } else {
          selectedDocumentTableCell = nil
        }
      }
      selectedSlideIndex = 0
      selectedSlideElementID = nil
      selectedPDFPageIndex = 0
    case .sheet:
      selectedSlideIndex = 0
      selectedSlideElementID = nil
      selectedPDFPageIndex = 0
      selectedDocumentTableIndex = 0
      selectedDocumentTableCell = nil
    }
  }

  internal func syncSheetRulesFromActiveTab() {
    guard !isSyncingSheetRulesState else { return }
    isSyncingSheetRulesState = true
    defer { isSyncingSheetRulesState = false }
    guard let tab = activeTab, case .sheet(let sheet) = tab.payload else {
      activeConditionalFormatRule = nil
      activeDataValidationRule = nil
      return
    }
    activeConditionalFormatRule = sheet.conditionalFormatRules.first
    activeDataValidationRule = sheet.dataValidationRule
    formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
  }

  internal func persistSheetRulesToActiveTab() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    let rules = activeConditionalFormatRule.map { [$0] } ?? []
    if sheet.conditionalFormatRules != rules || sheet.dataValidationRule != activeDataValidationRule
    {
      sheet.conditionalFormatRules = rules
      sheet.dataValidationRule = activeDataValidationRule
      openTabs[idx].payload = .sheet(sheet)
    }
  }

  public var activeTab: WorkspaceTabItem? {
    guard let activeTabID else { return openTabs.first }
    return openTabs.first(where: { $0.id == activeTabID })
  }

  public var activeDocument: OfficeDocumentModel? {
    guard let tab = activeTab, case .document(let doc) = tab.payload else { return nil }
    return doc
  }

  public var activeWorksheet: WorksheetModel? {
    guard let tab = activeTab, case .sheet(let sheet) = tab.payload else { return nil }
    return sheet
  }

  public var activePresentation: PresentationDeckModel? {
    guard let tab = activeTab, case .slide(let deck) = tab.payload else { return nil }
    return deck
  }

  public var activePDFDocument: PDFStudioDocumentModel? {
    guard let tab = activeTab, case .pdf(let pdf) = tab.payload else { return nil }
    return pdf
  }

  public func updateActiveTabPayload(_ payload: OpenTabPayload, markModified: Bool = true) {
    guard let tabID = activeTab?.id,
      let idx = openTabs.firstIndex(where: { $0.id == tabID })
    else { return }
    openTabs[idx].payload = payload
    if markModified {
      openTabs[idx].isModified = true
    }
  }

  public func toggleRibbonLayoutStyle() {
    ribbonLayoutStyle = (ribbonLayoutStyle == .standardOffice) ? .hangulHwp : .standardOffice
    lastExecutedCommandSummary = "리본 레이아웃 전환: \(ribbonLayoutStyle.rawValue)"
  }

  public var activeContextualObjectToolBadge: String {
    switch activeTab?.moduleKind ?? .document {
    case .document: return "표·개체 도구"
    case .hwp: return "쪽·결재·원고지 도구"
    case .sheet: return "수식·피벗 분석"
    case .slide: return "발표자·레이저 도구"
    case .pdf: return "전자결재·주석 도구"
    }
  }

  public var splitSecondaryTab: WorkspaceTabItem? {
    guard let splitSecondaryTabID, splitSecondaryTabID != activeTabID else { return nil }
    return openTabs.first(where: { $0.id == splitSecondaryTabID })
  }

  public var filteredTabs: [WorkspaceTabItem] {
    let source = (activeHubSection == .recent) ? combinedRecentPool : openTabs
    return WorkspaceHubCatalog.filterTabs(source, section: activeHubSection, query: searchQuery)
  }

  public var filteredRecentTabs: [WorkspaceTabItem] {
    let pool = WorkspaceHubCatalog.filterTabs(
      combinedRecentPool,
      section: activeHubSection,
      query: searchQuery
    )
    return WorkspaceHubCatalog.filterRecentTabs(pool, formatFilter: selectedHomeRecentFormatFilter)
  }

  public var filteredHomeRecentTabs: [WorkspaceTabItem] {
    filteredRecentTabs
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

    var metaStack = undoMetadataStacks[tabID] ?? []
    metaStack.append(
      TabMetadataSnapshot(
        format: openTabs[idx].format,
        fileName: openTabs[idx].fileName,
        fileURL: openTabs[idx].fileURL,
        isLosslessNativeOrigin: openTabs[idx].isLosslessNativeOrigin
      )
    )
    if metaStack.count > Self.maxUndoDepth {
      metaStack.removeFirst(metaStack.count - Self.maxUndoDepth)
    }
    undoMetadataStacks[tabID] = metaStack
    redoMetadataStacks[tabID] = []
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

    if var metaStack = undoMetadataStacks[tabID], let previousMeta = metaStack.popLast() {
      undoMetadataStacks[tabID] = metaStack
      var rMetaStack = redoMetadataStacks[tabID] ?? []
      rMetaStack.append(
        TabMetadataSnapshot(
          format: openTabs[idx].format,
          fileName: openTabs[idx].fileName,
          fileURL: openTabs[idx].fileURL,
          isLosslessNativeOrigin: openTabs[idx].isLosslessNativeOrigin
        )
      )
      redoMetadataStacks[tabID] = rMetaStack
      openTabs[idx].format = previousMeta.format
      openTabs[idx].fileName = previousMeta.fileName
      openTabs[idx].fileURL = previousMeta.fileURL
      openTabs[idx].isLosslessNativeOrigin = previousMeta.isLosslessNativeOrigin
    }

    openTabs[idx].payload = previousPayload
    openTabs[idx].isModified = true
    syncSheetRulesFromActiveTab()
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

    if var rMetaStack = redoMetadataStacks[tabID], let nextMeta = rMetaStack.popLast() {
      redoMetadataStacks[tabID] = rMetaStack
      var uMetaStack = undoMetadataStacks[tabID] ?? []
      uMetaStack.append(
        TabMetadataSnapshot(
          format: openTabs[idx].format,
          fileName: openTabs[idx].fileName,
          fileURL: openTabs[idx].fileURL,
          isLosslessNativeOrigin: openTabs[idx].isLosslessNativeOrigin
        )
      )
      undoMetadataStacks[tabID] = uMetaStack
      openTabs[idx].format = nextMeta.format
      openTabs[idx].fileName = nextMeta.fileName
      openTabs[idx].fileURL = nextMeta.fileURL
      openTabs[idx].isLosslessNativeOrigin = nextMeta.isLosslessNativeOrigin
    }

    openTabs[idx].payload = nextPayload
    openTabs[idx].isModified = true
    syncSheetRulesFromActiveTab()
    lastExecutedCommandSummary = "다시 실행 완료"
  }

  public func undoActiveTab() {
    undo()
  }

  public func redoActiveTab() {
    redo()
  }

  @discardableResult
  public func openNewSampleTab(for module: OfficeModuleKind) -> WorkspaceTabItem {
    activeInlineEditTargetKey = nil
    var newTab = WorkspaceHubCatalog.instantiateSampleTab(for: module)
    newTab.lastOpenedISO8601 = OfficeDocumentModel.currentISO8601Timestamp()
    newTab.isLosslessNativeOrigin = true
    openTabs.append(newTab)
    recordRecentDocument(newTab)
    activeTabID = newTab.id
    isShowingHomeHub = false
    return newTab
  }

  @discardableResult
  public func openNewBlankTab(for module: OfficeModuleKind) -> WorkspaceTabItem {
    activeInlineEditTargetKey = nil
    blankTabCounter += 1
    var newTab = WorkspaceHubCatalog.instantiateBlankTab(
      for: module,
      indexSuffix: blankTabCounter
    )
    newTab.lastOpenedISO8601 = OfficeDocumentModel.currentISO8601Timestamp()
    newTab.isLosslessNativeOrigin = true
    openTabs.append(newTab)
    recordRecentDocument(newTab)
    activeTabID = newTab.id
    isShowingHomeHub = false
    if case .document(let doc) = newTab.payload {
      selectedParagraphID = doc.paragraphs.first?.id
    } else if case .sheet = newTab.payload {
      selectedSheetCell = "A1"
      selectedSheetRange = "A1"
      formulaBarDraft = ""
    } else if case .slide(let deck) = newTab.payload {
      selectedSlideIndex = 0
      selectedSlideElementID = deck.slides.first?.elements.first?.id
    } else if case .pdf = newTab.payload {
      selectedPDFPageIndex = 0
    }
    lastExecutedCommandSummary = "새 빈 문서 생성: \(newTab.fileName)"
    return newTab
  }

  @discardableResult
  public func activateOrReopenRecentTab(_ tab: WorkspaceTabItem) -> WorkspaceTabItem {
    if let existingIdx = openTabs.firstIndex(where: { $0.id == tab.id }) {
      activeTabID = openTabs[existingIdx].id
      isShowingHomeHub = false
      recordRecentDocument(openTabs[existingIdx])
      return openTabs[existingIdx]
    }
    if let url = tab.fileURL {
      do {
        let rawData = try Data(contentsOf: url)
        let decoded = try decodeDocumentPayload(data: rawData, fileName: url.lastPathComponent)
        let stdPath = url.standardizedFileURL.path
        let isNativeOrigin = knownEasyOfficeSavedFilePaths.contains(stdPath)
        let reopened = WorkspaceTabItem(
          id: tab.id,
          fileName: url.lastPathComponent,
          format: decoded.format,
          isPinned: tab.isPinned,
          isFavorite: tab.isFavorite,
          isModified: false,
          lastOpenedISO8601: OfficeDocumentModel.currentISO8601Timestamp(),
          fileURL: url,
          payload: decoded.payload,
          isLosslessNativeOrigin: isNativeOrigin,
          isMetadataOnlyPlaceholder: false,
          isUnreadable: false
        )
        recentlyClosedTabs.removeAll { $0.id == reopened.id }
        openTabs.append(reopened)
        recordRecentDocument(reopened)
        activeTabID = reopened.id
        isShowingHomeHub = false
        lastErrorMessage = nil
        return reopened
      } catch {
        if let rIdx = recentDocumentRecords.firstIndex(where: {
          $0.id == tab.id
            || ($0.fileURL?.standardizedFileURL.path == url.standardizedFileURL.path)
        }) {
          recentDocumentRecords[rIdx].isUnreadable = true
        }
        let desc = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
        lastErrorMessage = "최근 문서를 열 수 없습니다 (\(tab.fileName)): \(desc)"
        lastExecutedCommandSummary = "최근 문서 열기 실패: \(tab.fileName)"
        var safeUnbackedFallback = tab
        safeUnbackedFallback.fileURL = nil
        safeUnbackedFallback.isUnreadable = true
        return safeUnbackedFallback
      }
    }
    var restored = tab
    restored.fileURL = nil
    restored.isMetadataOnlyPlaceholder = false
    restored.lastOpenedISO8601 = OfficeDocumentModel.currentISO8601Timestamp()
    recentlyClosedTabs.removeAll { $0.id == restored.id }
    openTabs.append(restored)
    recordRecentDocument(restored)
    activeTabID = restored.id
    isShowingHomeHub = false
    return restored
  }

  public func openTab(from template: OfficeTemplateDescriptor) {
    activeInlineEditTargetKey = nil
    if let savedPayload = customTemplatePayloads[template.id] {
      let newTab = WorkspaceTabItem(
        fileName: "\(template.title).\(template.format.rawValue)", format: template.format,
        lastOpenedISO8601: OfficeDocumentModel.currentISO8601Timestamp(), payload: savedPayload,
        isLosslessNativeOrigin: true)
      openTabs.append(newTab)
      recordRecentDocument(newTab)
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
      payload: sample.payload, isLosslessNativeOrigin: true)
    openTabs.append(instantiated)
    recordRecentDocument(instantiated)
    activeTabID = instantiated.id
    isShowingHomeHub = false
  }

  public func requestCloseTab(id: UUID) {
    guard let tab = openTabs.first(where: { $0.id == id }) else { return }
    if tab.isModified {
      pendingCloseConfirmTabID = id
      lastExecutedCommandSummary = "저장되지 않은 변경사항 확인 대기: \(tab.fileName)"
    } else {
      closeTab(id: id)
    }
  }

  public func confirmCloseTab(id: UUID, saveTo url: URL? = nil) {
    if let url {
      do {
        try saveTabToDisk(id: id, to: url)
      } catch {
        lastErrorMessage = error.localizedDescription
        return
      }
      if let tab = openTabs.first(where: { $0.id == id }), tab.isModified {
        lastErrorMessage = "원본 문서 포맷으로 저장되지 않아 변경사항이 유지됩니다: \(tab.fileName)"
        return
      }
    }
    closeTab(id: id)
    advancePendingTerminationConfirmationIfNeeded()
  }

  public func confirmDiscardAndClosePendingTab() {
    guard let id = pendingCloseConfirmTabID else { return }
    pendingCloseConfirmTabID = nil
    closeTab(id: id)
    advancePendingTerminationConfirmationIfNeeded()
  }

  public func cancelCloseTab() {
    pendingCloseConfirmTabID = nil
    isPendingApplicationTermination = false
  }

  @discardableResult
  public func reopenLastClosedTab() -> WorkspaceTabItem? {
    guard !recentlyClosedTabs.isEmpty else { return nil }
    var restored = recentlyClosedTabs.removeFirst()
    if restored.isMetadataOnlyPlaceholder {
      guard restored.fileURL != nil else { return nil }
      let reopened = activateOrReopenRecentTab(restored)
      guard !reopened.isUnreadable else {
        lastExecutedCommandSummary = "최근 닫은 탭 복원 실패: \(restored.fileName)"
        return nil
      }
      lastExecutedCommandSummary = "최근 닫은 탭 복원: \(reopened.fileName)"
      return reopened
    }
    restored.isMetadataOnlyPlaceholder = false
    openTabs.append(restored)
    recordRecentDocument(restored)
    activeTabID = restored.id
    isShowingHomeHub = false
    lastExecutedCommandSummary = "최근 닫은 탭 복원: \(restored.fileName)"
    return restored
  }

  public func closeTab(id: UUID) {
    activeInlineEditTargetKey = nil
    if let closingTab = openTabs.first(where: { $0.id == id }) {
      recordRecentDocument(closingTab, isOpenInWorkspace: false)
      var snapshotForClosed = closingTab
      if !closingTab.isModified, closingTab.fileURL != nil {
        snapshotForClosed.payload = Self.lightweightPlaceholderPayload(for: closingTab.format)
        snapshotForClosed.isMetadataOnlyPlaceholder = true
      }
      recentlyClosedTabs.removeAll { $0.id == closingTab.id }
      recentlyClosedTabs.insert(snapshotForClosed, at: 0)
      if recentlyClosedTabs.count > 10 {
        recentlyClosedTabs.removeLast(recentlyClosedTabs.count - 10)
      }
    }
    if pendingCloseConfirmTabID == id {
      pendingCloseConfirmTabID = nil
    }
    if pendingLossyOverwriteConfirmationTabID == id {
      pendingLossyOverwriteConfirmationTabID = nil
    }
    openTabs.removeAll { $0.id == id }
    undoStacks.removeValue(forKey: id)
    redoStacks.removeValue(forKey: id)
    undoMetadataStacks.removeValue(forKey: id)
    redoMetadataStacks.removeValue(forKey: id)
    mergedSheetRangesByTab.removeValue(forKey: id)
    if splitSecondaryTabID == id { splitSecondaryTabID = nil }
    if activeTabID == id { activeTabID = openTabs.first?.id }
    if splitSecondaryTabID == activeTabID { splitSecondaryTabID = nil }
    if openTabs.isEmpty { isShowingHomeHub = true }
  }

  public func closeOtherTabs(keeping id: UUID) {
    activeInlineEditTargetKey = nil
    let candidates = openTabs.filter { $0.id != id && !$0.isPinned }
    let unmodifiedToClose = candidates.filter { !$0.isModified }
    let modifiedCandidates = candidates.filter { $0.isModified }

    for closingTab in unmodifiedToClose {
      closeTab(id: closingTab.id)
    }

    if let firstModified = modifiedCandidates.first {
      pendingCloseConfirmTabID = firstModified.id
      lastExecutedCommandSummary = "저장되지 않은 변경사항 확인 대기: \(firstModified.fileName)"
    } else {
      lastExecutedCommandSummary = "다른 탭 닫기 완료"
    }

    if let secID = splitSecondaryTabID, !openTabs.contains(where: { $0.id == secID }) {
      splitSecondaryTabID = nil
    }
    activeTabID = openTabs.contains(where: { $0.id == id }) ? id : openTabs.first?.id
    if splitSecondaryTabID == activeTabID { splitSecondaryTabID = nil }
    if openTabs.isEmpty { isShowingHomeHub = true }
  }

  public func setDocumentHorizontalPadding(_ padding: Double) {
    guard padding.isFinite else { return }
    documentHorizontalPadding = min(96.0, max(24.0, padding))
    lastExecutedCommandSummary = "문서 좌우 여백 조절: \(Int(documentHorizontalPadding))pt"
  }

  public func promoteOrDemoteParagraphOutlineLevel(paragraphID: UUID, delta: Int) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      let pIdx = doc.paragraphs.firstIndex(where: { $0.id == paragraphID })
    else { return }
    let hierarchy: [ParagraphStyleRole] = [.title, .heading1, .heading2, .heading3, .body]
    let currentRole = doc.paragraphs[pIdx].style
    let currentPos = hierarchy.firstIndex(of: currentRole) ?? (hierarchy.count - 1)
    let nextPos = min(max(0, currentPos + delta), hierarchy.count - 1)
    let nextRole = hierarchy[nextPos]
    recordUndoSnapshot(for: activeTabID)
    doc.paragraphs[pIdx].style = nextRole
    switch nextRole {
    case .title:
      doc.paragraphs[pIdx].fontSizePt = 20
      doc.paragraphs[pIdx].isBold = true
    case .heading1:
      doc.paragraphs[pIdx].fontSizePt = 15
      doc.paragraphs[pIdx].isBold = true
    case .heading2:
      doc.paragraphs[pIdx].fontSizePt = 13
      doc.paragraphs[pIdx].isBold = true
    case .heading3:
      doc.paragraphs[pIdx].fontSizePt = 12
      doc.paragraphs[pIdx].isBold = true
    case .body, .quote, .officialHeader:
      doc.paragraphs[pIdx].fontSizePt = 11
      doc.paragraphs[pIdx].isBold = false
    }
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    selectedParagraphID = paragraphID
    lastExecutedCommandSummary = "윤곽 수준 변경: \(nextRole.rawValue)"
  }

  public func togglePinTab(id: UUID) {
    if let idx = openTabs.firstIndex(where: { $0.id == id }) {
      openTabs[idx].isPinned.toggle()
      recordRecentDocument(openTabs[idx])
    } else if let rIdx = recentDocumentRecords.firstIndex(where: { $0.id == id }) {
      recentDocumentRecords[rIdx].isPinned.toggle()
      persistRecentDocumentMetadata()
    }
  }

  public func toggleFavoriteTab(id: UUID) {
    if let idx = openTabs.firstIndex(where: { $0.id == id }) {
      openTabs[idx].isFavorite.toggle()
      recordRecentDocument(openTabs[idx])
    } else if let rIdx = recentDocumentRecords.firstIndex(where: { $0.id == id }) {
      recentDocumentRecords[rIdx].isFavorite.toggle()
      persistRecentDocumentMetadata()
    }
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
      lastOpenedISO8601: OfficeDocumentModel.currentISO8601Timestamp(), payload: source.payload,
      isLosslessNativeOrigin: true)
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

  public func setZoom(_ percent: Double) {
    guard percent.isFinite else { return }
    zoomPercent = min(400.0, max(25.0, percent))
  }
}
