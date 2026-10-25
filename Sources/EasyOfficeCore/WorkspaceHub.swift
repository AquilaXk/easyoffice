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
  public var workspaceViewMode: WorkspaceViewMode = .editView
  public var activeInspectorSubTab: InspectorSubTab = .propertiesAndLayout

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
  private var isSyncingSheetRulesState: Bool = false
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
  private var isSyncingFindReplaceState: Bool = false
  public var findQueryText: String = "" {
    didSet {
      if findQueryText != oldValue {
        currentFindMatchIndex = 0
      }
      guard !isSyncingFindReplaceState else { return }
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
  public var batchTargetFormat: DocumentFormat = .pdf
  public var additionalBatchJobs: [ConversionJobRequest] = []
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
  public var recentlyClosedTabs: [WorkspaceTabItem] = []
  public var recentDocumentRecords: [WorkspaceTabItem] = []

  internal var undoStacks: [UUID: [OpenTabPayload]] = [:]
  internal var redoStacks: [UUID: [OpenTabPayload]] = [:]
  internal var customTemplatePayloads: [String: OpenTabPayload] = [:]
  internal var activeInlineEditTargetKey: String? = nil
  internal var blankTabCounter: Int = 0
  public internal(set) var sheetSortAscending: Bool = false

  private struct PersistedRecentRecordMetadata: Codable {
    let idString: String
    let fileName: String
    let formatRawValue: String
    let lastOpenedISO8601: String
    let isPinned: Bool
    let isFavorite: Bool
    let filePath: String?
  }

  private static let recentMetadataDefaultsKey = "eo.recentDocumentMetadataV1"

  public init() {
    let initialTabs = OfficeModuleKind.allCases.map {
      WorkspaceHubCatalog.instantiateSampleTab(for: $0)
    }
    self.openTabs = initialTabs
    self.recentDocumentRecords = initialTabs
    self.activeTabID = initialTabs.first?.id
    self.formulaBarDraft = "=SUM(B2:C2)"
  }

  internal func recordRecentDocument(_ tab: WorkspaceTabItem) {
    var updated = tab
    updated.lastOpenedISO8601 = OfficeDocumentModel.currentISO8601Timestamp()
    recentDocumentRecords.removeAll { existing in
      if existing.id == updated.id { return true }
      if let u1 = existing.fileURL, let u2 = updated.fileURL, u1 == u2 { return true }
      return false
    }
    recentDocumentRecords.insert(updated, at: 0)
    if recentDocumentRecords.count > 24 {
      recentDocumentRecords.removeLast(recentDocumentRecords.count - 24)
    }
    persistRecentDocumentMetadata()
  }

  public func persistRecentDocumentMetadata(to defaults: UserDefaults = .standard) {
    let metadataList = recentDocumentRecords.map { item in
      PersistedRecentRecordMetadata(
        idString: item.id.uuidString,
        fileName: item.fileName,
        formatRawValue: item.format.rawValue,
        lastOpenedISO8601: item.lastOpenedISO8601,
        isPinned: item.isPinned,
        isFavorite: item.isFavorite,
        filePath: item.fileURL?.path
      )
    }
    if let encoded = try? JSONEncoder().encode(metadataList) {
      defaults.set(encoded, forKey: Self.recentMetadataDefaultsKey)
    }
  }

  public func restorePersistedRecentDocuments(from defaults: UserDefaults = .standard) {
    guard let rawData = defaults.data(forKey: Self.recentMetadataDefaultsKey),
      let decodedList = try? JSONDecoder().decode([PersistedRecentRecordMetadata].self, from: rawData)
    else {
      return
    }
    var restored: [WorkspaceTabItem] = []
    for meta in decodedList.prefix(24) {
      let resolvedID = UUID(uuidString: meta.idString) ?? UUID()
      if let existingIdx = recentDocumentRecords.firstIndex(where: {
        $0.id == resolvedID || ($0.fileURL != nil && $0.fileURL?.path == meta.filePath)
      }) {
        var existing = recentDocumentRecords[existingIdx]
        existing.isPinned = meta.isPinned
        existing.isFavorite = meta.isFavorite
        existing.lastOpenedISO8601 = meta.lastOpenedISO8601
        restored.append(existing)
      } else if let filePath = meta.filePath,
        FileManager.default.fileExists(atPath: filePath),
        let diskTab = try? loadFromDisk(url: URL(fileURLWithPath: filePath))
      {
        restored.append(
          WorkspaceTabItem(
            id: resolvedID,
            fileName: meta.fileName,
            format: diskTab.format,
            isPinned: meta.isPinned,
            isFavorite: meta.isFavorite,
            isModified: false,
            lastOpenedISO8601: meta.lastOpenedISO8601,
            fileURL: URL(fileURLWithPath: filePath),
            payload: diskTab.payload
          )
        )
      } else if let fmt = DocumentFormat(rawValue: meta.formatRawValue) {
        let blankTab = WorkspaceHubCatalog.instantiateBlankTab(
          for: fmt.moduleKind,
          indexSuffix: 1
        )
        restored.append(
          WorkspaceTabItem(
            id: resolvedID,
            fileName: meta.fileName,
            format: fmt,
            isPinned: meta.isPinned,
            isFavorite: meta.isFavorite,
            isModified: false,
            lastOpenedISO8601: meta.lastOpenedISO8601,
            fileURL: meta.filePath.map { URL(fileURLWithPath: $0) },
            payload: blankTab.payload
          )
        )
      }
    }
    for existing in recentDocumentRecords where !restored.contains(where: { $0.id == existing.id }) {
      if restored.count < 24 {
        restored.append(existing)
      }
    }
    recentDocumentRecords = restored
  }

  internal var combinedRecentPool: [WorkspaceTabItem] {
    var seenIDs = Set<UUID>()
    var merged: [WorkspaceTabItem] = []
    for tab in openTabs {
      seenIDs.insert(tab.id)
      merged.append(tab)
    }
    for record in recentDocumentRecords where !seenIDs.contains(record.id) {
      if let url = record.fileURL, openTabs.contains(where: { $0.fileURL == url }) {
        continue
      }
      seenIDs.insert(record.id)
      merged.append(record)
    }
    return merged
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

  // MARK: - Tab & Template Management

  public func openNewSampleTab(for module: OfficeModuleKind) {
    activeInlineEditTargetKey = nil
    var newTab = WorkspaceHubCatalog.instantiateSampleTab(for: module)
    newTab.lastOpenedISO8601 = OfficeDocumentModel.currentISO8601Timestamp()
    openTabs.append(newTab)
    recordRecentDocument(newTab)
    activeTabID = newTab.id
    isShowingHomeHub = false
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
    if let url = tab.fileURL, FileManager.default.fileExists(atPath: url.path),
      let loaded = try? loadFromDisk(url: url)
    {
      return loaded
    }
    var restored = tab
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
        lastOpenedISO8601: OfficeDocumentModel.currentISO8601Timestamp(), payload: savedPayload)
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
      payload: sample.payload)
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
  }

  public func cancelCloseTab() {
    pendingCloseConfirmTabID = nil
  }

  @discardableResult
  public func reopenLastClosedTab() -> WorkspaceTabItem? {
    guard !recentlyClosedTabs.isEmpty else { return nil }
    let restored = recentlyClosedTabs.removeFirst()
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
      recordRecentDocument(closingTab)
      recentlyClosedTabs.removeAll { $0.id == closingTab.id }
      recentlyClosedTabs.insert(closingTab, at: 0)
      if recentlyClosedTabs.count > 10 {
        recentlyClosedTabs.removeLast(recentlyClosedTabs.count - 10)
      }
    }
    if pendingCloseConfirmTabID == id {
      pendingCloseConfirmTabID = nil
    }
    openTabs.removeAll { $0.id == id }
    undoStacks.removeValue(forKey: id)
    redoStacks.removeValue(forKey: id)
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
      recordRecentDocument(closingTab)
      recentlyClosedTabs.removeAll { $0.id == closingTab.id }
      recentlyClosedTabs.insert(closingTab, at: 0)
      if recentlyClosedTabs.count > 10 {
        recentlyClosedTabs.removeLast(recentlyClosedTabs.count - 10)
      }
    }

    let removedIDs = Set(unmodifiedToClose.map(\.id))
    openTabs.removeAll { removedIDs.contains($0.id) }
    for removedID in removedIDs {
      undoStacks.removeValue(forKey: removedID)
      redoStacks.removeValue(forKey: removedID)
      mergedSheetRangesByTab.removeValue(forKey: removedID)
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
    recordRecentDocument(newTab)
    return newTab
  }

  @discardableResult
  public func loadFromDisk(url: URL) throws -> WorkspaceTabItem {
    try openDocument(data: try Data(contentsOf: url), fileName: url.lastPathComponent, fileURL: url)
  }

  @discardableResult
  public func importDocumentFromDisk(url: URL) throws -> WorkspaceTabItem {
    try loadFromDisk(url: url)
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
    if fmt.moduleKind == openTabs[idx].format.moduleKind {
      openTabs[idx].payload = result.convertedPayload
      openTabs[idx].isModified = false
      openTabs[idx].fileURL = url
      openTabs[idx].fileName = url.lastPathComponent
      openTabs[idx].format = fmt
      openTabs[idx].lastOpenedISO8601 = OfficeDocumentModel.currentISO8601Timestamp()
      recordRecentDocument(openTabs[idx])
      lastExecutedCommandSummary = "저장 완료: \(url.lastPathComponent)"
    } else {
      lastExecutedCommandSummary = "포맷 내보내기 완료: \(url.lastPathComponent)"
    }
    lastErrorMessage = nil
  }

  @discardableResult
  public func saveTabOverwriteIfBackedByFile(id: UUID) throws -> Bool {
    guard let idx = openTabs.firstIndex(where: { $0.id == id }),
      let existingURL = openTabs[idx].fileURL
    else {
      return false
    }
    try saveTabToDisk(id: id, to: existingURL, targetFormat: openTabs[idx].format)
    return true
  }

  @discardableResult
  public func saveActiveTabOverwriteIfBackedByFile() throws -> Bool {
    guard let activeTabID else { return false }
    return try saveTabOverwriteIfBackedByFile(id: activeTabID)
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
    syncSheetRulesFromActiveTab()
    lastExecutedCommandSummary = "시트 탭 전환: \(sheet.name)"
  }

  public func renameWorksheetTab(at index: Int, to newName: String) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    let trimmed = newName.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return }
    recordUndoSnapshot(for: activeTabID)
    sheet.renameSheetPage(at: index, to: trimmed)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "시트 탭 이름 변경: \(trimmed)"
  }

  public func deleteWorksheetTab(at index: Int) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload,
      sheet.allSheetNames.count > 1
    else { return }
    recordUndoSnapshot(for: activeTabID)
    sheet.deleteSheetPage(at: index)
    formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    syncSheetRulesFromActiveTab()
    lastExecutedCommandSummary = "시트 탭 삭제 완료"
  }

  public func copySelectedSheetRangeAsTSV() -> String {
    guard let tab = activeTab, case .sheet(let sheet) = tab.payload else { return "" }
    let originAddress = CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    let fallbackRange =
      (try? CellAddress(a1Notation: selectedSheetCell)).map { CellRange(start: $0, end: $0) }
      ?? CellRange(start: originAddress, end: originAddress)
    let range = (try? CellRange(notation: selectedSheetRange)) ?? fallbackRange
    let minR = min(range.start.row, range.end.row)
    let maxR = max(range.start.row, range.end.row)
    let minC = min(range.start.column, range.end.column)
    let maxC = max(range.start.column, range.end.column)

    var rowStrings: [String] = []
    for r in minR...maxR {
      var colStrings: [String] = []
      for c in minC...maxC {
        let addr = CellAddress(packed: PackedCellCoordinate(row: r, column: c))
        let raw = sheet.rawValue(at: addr.a1Notation)
        let cellStr = raw.hasPrefix("=") ? sheet.formattedDisplayValue(at: addr) : raw
        colStrings.append(cellStr)
      }
      rowStrings.append(colStrings.joined(separator: "\t"))
    }
    return rowStrings.joined(separator: "\n")
  }

  public func pasteSheetCellsFromTSV(_ tsvText: String) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    let normalized = tsvText.replacingOccurrences(of: "\r\n", with: "\n")
    var lines = normalized.components(separatedBy: "\n")
    if lines.count > 1 && lines.last == "" {
      lines.removeLast()
    }
    guard !lines.isEmpty else { return }

    let startAddr =
      (try? CellAddress(a1Notation: selectedSheetCell))
      ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    recordUndoSnapshot(for: activeTabID)

    var maxRowOffset = 0
    var maxColOffset = 0
    for (rOffset, line) in lines.enumerated() {
      let cols = line.components(separatedBy: "\t")
      maxRowOffset = max(maxRowOffset, rOffset)
      for (cOffset, val) in cols.enumerated() {
        maxColOffset = max(maxColOffset, cOffset)
        let targetRow = min(CellAddress.maxZeroBasedRow, startAddr.row + rOffset)
        let targetCol = min(CellAddress.maxZeroBasedColumn, startAddr.column + cOffset)
        try? sheet.setCell(row: targetRow, column: targetCol, rawValue: val)
      }
    }

    let endRow = min(CellAddress.maxZeroBasedRow, startAddr.row + maxRowOffset)
    let endCol = min(CellAddress.maxZeroBasedColumn, startAddr.column + maxColOffset)
    let endAddr = CellAddress(packed: PackedCellCoordinate(row: endRow, column: endCol))
    selectedSheetRange =
      (startAddr == endAddr)
      ? startAddr.a1Notation : "\(startAddr.a1Notation):\(endAddr.a1Notation)"
    formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "TSV 셀 범위 붙여넣기 완료 (\(selectedSheetRange))"
  }

  public func moveSheetSelection(
    rowDelta: Int,
    columnDelta: Int,
    extendSelection: Bool = false
  ) {
    let baseNotation: String
    if extendSelection, selectedSheetRange.contains(":") {
      baseNotation = selectedSheetRange.components(separatedBy: ":").last ?? selectedSheetCell
    } else {
      baseNotation = selectedSheetCell
    }
    let currentAddr =
      (try? CellAddress(a1Notation: baseNotation))
      ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    let nextRow = max(0, min(CellAddress.maxZeroBasedRow, currentAddr.row + rowDelta))
    let nextCol = max(0, min(CellAddress.maxZeroBasedColumn, currentAddr.column + columnDelta))
    let nextAddr = CellAddress(packed: PackedCellCoordinate(row: nextRow, column: nextCol))
    if extendSelection {
      extendSheetSelection(to: nextAddr.a1Notation)
    } else {
      selectSheetCellOrRange(primaryCell: nextAddr.a1Notation)
    }
  }

  public func clearSelectedSheetRange() {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    let originAddress = CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    let fallbackRange =
      (try? CellAddress(a1Notation: selectedSheetCell)).map { CellRange(start: $0, end: $0) }
      ?? CellRange(start: originAddress, end: originAddress)
    let range = (try? CellRange(notation: selectedSheetRange)) ?? fallbackRange
    recordUndoSnapshot(for: activeTabID)
    range.forEachCoordinate { coord in
      sheet.cellsByCoordinate.removeValue(forKey: coord)
      sheet.compiledASTs.removeValue(forKey: coord)
      sheet.sheetComments.removeValue(forKey: coord)
    }
    formulaBarDraft = ""
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "선택 셀 값 지우기 (\(selectedSheetRange))"
  }

  public func setSelectedSheetCellColors(
    fillHex: String? = nil,
    textHex: String? = nil,
    clearFill: Bool = false,
    clearText: Bool = false
  ) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let coord =
      (try? CellAddress(a1Notation: selectedSheetCell))?.packedCoordinate
      ?? PackedCellCoordinate(row: 0, column: 0)
    var style = sheet.cellStyle(at: coord)
    if clearFill {
      style.fillHex = nil
    } else if let fillHex {
      style.fillHex = fillHex
    }
    if clearText {
      style.textHex = nil
    } else if let textHex {
      style.textHex = textHex
    }
    sheet.setCellStyle(style, at: coord)
    if let range = try? CellRange(notation: selectedSheetRange), range.start != range.end {
      sheet.updateCellStyleInRange(range) { cellStyle in
        if clearFill {
          cellStyle.fillHex = nil
        } else if let fillHex {
          cellStyle.fillHex = fillHex
        }
        if clearText {
          cellStyle.textHex = nil
        } else if let textHex {
          cellStyle.textHex = textHex
        }
      }
    }
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "셀 배경·글자 색상 서식 적용"
  }

  public var isTrackChangesEnabled: Bool {
    get {
      guard let tab = activeTab else { return false }
      switch tab.payload {
      case .document(let d): return d.trackChangesEnabled
      case .sheet(let s): return s.trackChangesEnabled
      case .slide, .pdf: return false
      }
    }
    set {
      guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }) else {
        return
      }
      switch openTabs[idx].payload {
      case .document(var d):
        d.trackChangesEnabled = newValue
        openTabs[idx].payload = .document(d)
      case .sheet(var s):
        s.trackChangesEnabled = newValue
        openTabs[idx].payload = .sheet(s)
      case .slide, .pdf:
        break
      }
    }
  }

  public func enqueueOpenTabsIntoBatchConversion(targetFormat: DocumentFormat? = nil) {
    if let targetFormat {
      batchTargetFormat = targetFormat
    }
  }

  public func enqueueFilesFromDiskIntoBatchConversion(
    urls: [URL],
    targetFormat: DocumentFormat? = nil
  ) {
    if let targetFormat {
      batchTargetFormat = targetFormat
    }
    var enqueuedCount = 0
    for url in urls {
      do {
        let tab = try loadFromDisk(url: url)
        let safeSourceName =
          tab.fileName.contains(".") ? tab.fileName : "\(tab.fileName).\(tab.format.fileExtension)"
        additionalBatchJobs.append(
          ConversionJobRequest(
            sourceFileName: safeSourceName,
            sourceFormat: tab.format,
            targetFormat: batchTargetFormat,
            payload: tab.payload
          )
        )
        enqueuedCount += 1
      } catch {
        lastErrorMessage = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
      }
    }
    if enqueuedCount > 0 {
      lastExecutedCommandSummary = "일괄 변환 대기열 파일 추가: \(enqueuedCount)건"
    }
    isShowingHomeHub = true
    activeHubSection = .converter
  }

  public func runAllBatchConversions() {
    var jobs: [ConversionJobRequest] = openTabs.map { tab in
      let safeSourceName =
        tab.fileName.contains(".") ? tab.fileName : "\(tab.fileName).\(tab.format.fileExtension)"
      return ConversionJobRequest(
        id: tab.id,
        sourceFileName: safeSourceName,
        sourceFormat: tab.format,
        targetFormat: batchTargetFormat,
        payload: tab.payload
      )
    }
    for extra in additionalBatchJobs {
      if !jobs.contains(where: { $0.sourceFileName == extra.sourceFileName }) {
        jobs.append(
          ConversionJobRequest(
            id: extra.id,
            sourceFileName: extra.sourceFileName,
            sourceFormat: extra.sourceFormat,
            targetFormat: batchTargetFormat,
            payload: extra.payload
          )
        )
      }
    }
    lastBatchConversionReport = DocumentConversionOrchestrator.executeBatch(jobs)
  }

  public var batchConversionQueue: [BatchConversionQueueItem] {
    guard let report = lastBatchConversionReport else {
      var pendingItems = openTabs.map { tab in
        BatchConversionQueueItem(
          id: tab.id,
          sourceFileName: tab.fileName,
          sourceFormat: tab.format,
          targetFormat: batchTargetFormat,
          status: .pending
        )
      }
      for extra in additionalBatchJobs
      where !pendingItems.contains(where: { $0.sourceFileName == extra.sourceFileName }) {
        pendingItems.append(
          BatchConversionQueueItem(
            id: extra.id,
            sourceFileName: extra.sourceFileName,
            sourceFormat: extra.sourceFormat,
            targetFormat: batchTargetFormat,
            status: .pending
          )
        )
      }
      return pendingItems
    }
    var items: [BatchConversionQueueItem] = report.succeededResults.map { res in
      BatchConversionQueueItem(
        id: res.id,
        sourceFileName: res.sourceFileName,
        sourceFormat: res.sourceFormat,
        targetFormat: res.targetFormat,
        status: .completed,
        convertedPayloadBytes: res.outputBytes,
        result: res
      )
    }
    for failed in report.failedErrors {
      let matchedTab = openTabs.first(where: { $0.id == failed.jobID })
      let matchedExtra = additionalBatchJobs.first(where: { $0.id == failed.jobID })
      items.append(
        BatchConversionQueueItem(
          id: failed.jobID,
          sourceFileName: matchedTab?.fileName ?? matchedExtra?.sourceFileName ?? "변환 작업",
          sourceFormat: matchedTab?.format ?? matchedExtra?.sourceFormat ?? .docx,
          targetFormat: batchTargetFormat,
          status: .failed,
          errorDescription: failed.errorDescription
        )
      )
    }
    return items
  }

  public func exportBatchConversionItem(result: ConversionJobResult, to url: URL) throws {
    try result.outputBytes.write(to: url, options: .atomic)
    lastExecutedCommandSummary = "변환 결과 파일 저장 완료: \(url.lastPathComponent)"
    lastErrorMessage = nil
  }

  @discardableResult
  public func exportAllCompletedBatchItems(toDirectory directoryURL: URL) throws -> [URL] {
    if lastBatchConversionReport == nil {
      runAllBatchConversions()
    }
    guard let report = lastBatchConversionReport else { return [] }
    try FileManager.default.createDirectory(at: directoryURL, withIntermediateDirectories: true)
    var writtenURLs: [URL] = []
    for result in report.succeededResults {
      let outURL = directoryURL.appendingPathComponent(result.outputFileName)
      try result.outputBytes.write(to: outURL, options: .atomic)
      writtenURLs.append(outURL)
    }
    lastExecutedCommandSummary = "일괄 변환 결과 \(writtenURLs.count)건 폴더 저장 완료"
    lastErrorMessage = nil
    return writtenURLs
  }
}
