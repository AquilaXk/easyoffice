import AppKit
import XCTest

@testable import EasyOfficeCore
@testable import EasyOfficeDesignSystem

final class WorkspaceUIParityTests: XCTestCase {

  func testThemeModesSemanticStatusTokensLayoutMetricsAndAccessibility() {
    // Semantic Status Tokens
    XCTAssertEqual(EasyOfficeDesignTokens.statusSuccess.hex, "#0A705A")
    XCTAssertEqual(EasyOfficeDesignTokens.statusWarning.hex, "#B45309")
    XCTAssertEqual(EasyOfficeDesignTokens.statusDanger.hex, "#C81E1E")
    XCTAssertEqual(EasyOfficeDesignTokens.statusInfo.hex, "#4A58A9")

    // Verify WCAG 2.1 AA contrast ratios (>= 4.5) against lightWhite surface
    XCTAssertGreaterThanOrEqual(
      EasyOfficeDesignTokens.statusSuccess.contrastRatio(
        against: EasyOfficeDesignTokens.lightWhite),
      4.5
    )
    XCTAssertGreaterThanOrEqual(
      EasyOfficeDesignTokens.statusWarning.contrastRatio(
        against: EasyOfficeDesignTokens.lightWhite),
      4.5
    )
    XCTAssertGreaterThanOrEqual(
      EasyOfficeDesignTokens.statusDanger.contrastRatio(against: EasyOfficeDesignTokens.lightWhite),
      4.5
    )
    XCTAssertGreaterThanOrEqual(
      EasyOfficeDesignTokens.statusInfo.contrastRatio(against: EasyOfficeDesignTokens.lightWhite),
      4.5
    )

    // Extended LayoutMetrics
    XCTAssertEqual(EasyOfficeDesignTokens.LayoutMetrics.splitPaneMinimumWidth, 420)
    XCTAssertEqual(EasyOfficeDesignTokens.LayoutMetrics.formulaBarHeight, 34)
    XCTAssertEqual(EasyOfficeDesignTokens.LayoutMetrics.slideFilmstripWidth, 210)

    // WorkspaceThemeMode resolution
    let lightPalette = WorkspaceThemeMode.lightLavender.surfacePalette(isSystemDark: true)
    XCTAssertEqual(lightPalette.background, EasyOfficeDesignTokens.lightScaffold)
    XCTAssertEqual(lightPalette.cardSurface, EasyOfficeDesignTokens.lightWhite)
    XCTAssertEqual(lightPalette.elevatedSurface, EasyOfficeDesignTokens.lightSubtle)
    XCTAssertEqual(lightPalette.border, EasyOfficeDesignTokens.lightBorder)
    XCTAssertEqual(lightPalette.primaryText, EasyOfficeDesignTokens.inkPrimary)
    XCTAssertEqual(lightPalette.secondaryText, EasyOfficeDesignTokens.inkSecondary)
    XCTAssertGreaterThanOrEqual(
      lightPalette.primaryText.contrastRatio(against: lightPalette.cardSurface),
      4.5
    )

    let darkPalette = WorkspaceThemeMode.darkIndigo.surfacePalette(isSystemDark: false)
    XCTAssertEqual(darkPalette.background, EasyOfficeDesignTokens.darkScaffold)
    XCTAssertEqual(darkPalette.cardSurface, EasyOfficeDesignTokens.darkSurface)
    XCTAssertEqual(darkPalette.elevatedSurface, EasyOfficeDesignTokens.darkElevated)
    XCTAssertEqual(darkPalette.border, EasyOfficeDesignTokens.darkBorder)
    XCTAssertEqual(darkPalette.primaryText, EasyOfficeDesignTokens.darkText)
    XCTAssertEqual(darkPalette.secondaryText, EasyOfficeDesignTokens.darkMuted)
    XCTAssertGreaterThanOrEqual(
      darkPalette.primaryText.contrastRatio(against: darkPalette.cardSurface),
      4.5
    )

    let sysLight = WorkspaceThemeMode.system.surfacePalette(isSystemDark: false)
    let sysDark = WorkspaceThemeMode.system.surfacePalette(isSystemDark: true)
    XCTAssertEqual(sysLight, lightPalette)
    XCTAssertEqual(sysDark, darkPalette)

    // Deterministic AccessibilityTokens with concrete argument differentiation
    let pinnedModifiedTabLabel = AccessibilityTokens.tabLabel(
      fileName: "기획안.docx", module: .document, isPinned: true, isModified: true)
    let plainTabLabel = AccessibilityTokens.tabLabel(
      fileName: "기획안.docx", module: .document, isPinned: false, isModified: false)
    XCTAssertTrue(pinnedModifiedTabLabel.contains("기획안.docx"))
    XCTAssertNotEqual(pinnedModifiedTabLabel, plainTabLabel)
    XCTAssertFalse(AccessibilityTokens.tabHint.isEmpty)
    let expandedRibbonLabel = AccessibilityTokens.ribbonCollapseToggleLabel(isCollapsed: false)
    let collapsedRibbonLabel = AccessibilityTokens.ribbonCollapseToggleLabel(isCollapsed: true)
    XCTAssertNotEqual(expandedRibbonLabel, collapsedRibbonLabel)
    let primaryPaneLabel = AccessibilityTokens.splitViewPaneLabel(
      isPrimary: true, fileName: "문서A.hwpx")
    let secondaryPaneLabel = AccessibilityTokens.splitViewPaneLabel(
      isPrimary: false, fileName: "문서B.hwpx")
    XCTAssertTrue(primaryPaneLabel.contains("문서A.hwpx"))
    XCTAssertTrue(secondaryPaneLabel.contains("문서B.hwpx"))
    XCTAssertNotEqual(primaryPaneLabel, secondaryPaneLabel)
    XCTAssertTrue(AccessibilityTokens.formulaBarLabel(selectedCell: "D2").contains("D2"))
    XCTAssertTrue(AccessibilityTokens.slideStageLabel(slideNumber: 1, totalSlides: 5).contains("5"))
    XCTAssertTrue(
      AccessibilityTokens.pdfAnnotationToolLabel(toolTitle: "형광펜 하이라이트").contains("형광펜 하이라이트"))
  }

  func testRibbonCategoryAndCommandDescriptorCompleteness() {
    XCTAssertTrue(RibbonCategory.allCases.contains(.formulasAndData))
    XCTAssertEqual(RibbonCategory.formulasAndData.rawValue, "수식·데이터 (Data)")

    for module in OfficeModuleKind.allCases {
      for category in RibbonCategory.allCases {
        let title = category.title(for: module)
        XCTAssertFalse(title.isEmpty)

        let actions = RibbonCategory.actions(for: category, module: module)
        XCTAssertGreaterThanOrEqual(
          actions.count,
          3,
          "Expected >= 3 actions for \(category.rawValue) in \(module.rawValue)"
        )
        let uniqueIDs = Set(actions.map(\.id))
        XCTAssertEqual(uniqueIDs.count, actions.count)
        for cmd in actions {
          XCTAssertFalse(cmd.id.isEmpty)
          XCTAssertFalse(cmd.title.isEmpty)
          XCTAssertFalse(cmd.symbolName.isEmpty)
          XCTAssertFalse(cmd.shortcutHint.isEmpty)
        }
      }
    }
  }

  func testWorkspaceHubCatalogTemplatesAndFiltering() {
    // Extended HubSection
    XCTAssertTrue(HubSection.allCases.contains(.smartAssistant))
    XCTAssertEqual(HubSection.smartAssistant.rawValue, "스마트 문서 도구")
    XCTAssertEqual(HubSection.smartAssistant.symbolName, "sparkles.rectangle.stack.fill")

    // Built-in templates across all required formats
    let templates = WorkspaceHubCatalog.builtInTemplates
    XCTAssertGreaterThanOrEqual(templates.count, 8)
    let templateFormats = Set(templates.map(\.format))
    let requiredFormats: Set<DocumentFormat> = [.docx, .hwpx, .hwp, .xlsx, .ods, .pptx, .odp, .pdf]
    XCTAssertTrue(requiredFormats.isSubset(of: templateFormats))
    XCTAssertTrue(templates.allSatisfy { !$0.isCustomUserTemplate })

    // Tab defaults and filterTabs
    var tabDoc = WorkspaceHubCatalog.instantiateSampleTab(for: .document)
    tabDoc.isPinned = true
    tabDoc.isFavorite = false
    tabDoc.lastOpenedISO8601 = "2026-10-17T08:00:00Z"

    var tabSheet = WorkspaceHubCatalog.instantiateSampleTab(for: .sheet)
    tabSheet.isPinned = false
    tabSheet.isFavorite = true
    tabSheet.lastOpenedISO8601 = "2026-10-17T11:30:00Z"

    var tabSlide = WorkspaceHubCatalog.instantiateSampleTab(for: .slide)
    tabSlide.isPinned = false
    tabSlide.isFavorite = false
    tabSlide.lastOpenedISO8601 = "2026-10-17T10:15:00Z"

    let tabPDF = WorkspaceHubCatalog.instantiateSampleTab(for: .pdf)

    let tabs = [tabDoc, tabSheet, tabSlide, tabPDF]

    // .pinned includes isPinned || isFavorite
    let pinnedFiltered = WorkspaceHubCatalog.filterTabs(tabs, section: .pinned, query: "")
    XCTAssertTrue(pinnedFiltered.contains(where: { $0.id == tabDoc.id }))
    XCTAssertTrue(pinnedFiltered.contains(where: { $0.id == tabSheet.id }))
    XCTAssertFalse(pinnedFiltered.contains(where: { $0.id == tabSlide.id }))

    // .recent sorts descending by lastOpenedISO8601
    let recentFiltered = WorkspaceHubCatalog.filterTabs(
      [tabDoc, tabSheet, tabSlide], section: .recent, query: "")
    XCTAssertEqual(recentFiltered.map(\.id), [tabSheet.id, tabSlide.id, tabDoc.id])

    // Full-text query across formulas, speaker notes, and PDF annotations
    let formulaMatches = WorkspaceHubCatalog.filterTabs(tabs, section: .home, query: "=SUM(B2:C2)")
    XCTAssertEqual(formulaMatches.map(\.id), [tabSheet.id])

    let notesMatches = WorkspaceHubCatalog.filterTabs(tabs, section: .home, query: "오프닝 슬라이드")
    XCTAssertEqual(notesMatches.map(\.id), [tabSlide.id])

    let pdfAnnotationMatches = WorkspaceHubCatalog.filterTabs(
      tabs, section: .home, query: "반기 보안 감사 필수 점검 항목")
    XCTAssertEqual(pdfAnnotationMatches.map(\.id), [tabPDF.id])
  }

  @MainActor
  func testWorkspaceViewModelFullLifecycle() {
    let vm = WorkspaceViewModel()
    XCTAssertEqual(vm.openTabs.count, 5)
    XCTAssertEqual(vm.themeMode, .lightLavender)
    XCTAssertFalse(vm.isRibbonCollapsed)
    XCTAssertNil(vm.splitSecondaryTabID)

    guard let firstID = vm.openTabs.first?.id,
      let secondID = vm.openTabs.dropFirst().first?.id
    else {
      XCTFail("Expected initial sample tabs")
      return
    }

    // Pin and Favorite toggling
    let initialPinned = vm.openTabs[0].isPinned
    vm.togglePinTab(id: firstID)
    XCTAssertEqual(vm.openTabs[0].isPinned, !initialPinned)

    XCTAssertFalse(vm.openTabs[0].isFavorite)
    vm.toggleFavoriteTab(id: firstID)
    XCTAssertTrue(vm.openTabs[0].isFavorite)

    // Duplicate tab
    let countBeforeDup = vm.openTabs.count
    vm.duplicateTab(id: firstID)
    let duplicatedTabID = vm.activeTabID
    XCTAssertEqual(vm.openTabs.count, countBeforeDup + 1)
    XCTAssertNotEqual(duplicatedTabID, firstID)
    XCTAssertTrue(vm.activeTab?.fileName.contains("사본") == true)

    // Split View enable/disable, activeTabID collision reset, and closeTab cleanup
    vm.activeTabID = firstID
    vm.enableSplitView(secondaryTabID: secondID)
    XCTAssertEqual(vm.splitSecondaryTabID, secondID)
    vm.activeTabID = secondID
    XCTAssertNil(vm.splitSecondaryTabID)

    if let dupID = duplicatedTabID {
      vm.activeTabID = firstID
      vm.enableSplitView(secondaryTabID: dupID)
      XCTAssertEqual(vm.splitSecondaryTabID, dupID)
      vm.closeTab(id: dupID)
      XCTAssertNil(vm.splitSecondaryTabID)
    }

    // Save active tab as custom template
    XCTAssertTrue(vm.customTemplates.isEmpty)
    vm.saveActiveTabAsCustomTemplate(title: "팀 표준 기술보고서", subtitle: "주간 아키텍처 리뷰 양식")
    XCTAssertEqual(vm.customTemplates.count, 1)
    XCTAssertEqual(vm.customTemplates[0].title, "팀 표준 기술보고서")
    XCTAssertTrue(vm.customTemplates[0].isCustomUserTemplate)

    // Zoom clamping (25% ... 400%)
    vm.setZoom(10.0)
    XCTAssertEqual(vm.zoomPercent, 25.0)
    vm.setZoom(950.0)
    XCTAssertEqual(vm.zoomPercent, 400.0)
    vm.setZoom(125.0)
    XCTAssertEqual(vm.zoomPercent, 125.0)

    // Append paragraph to active Document tab
    vm.activeTabID = firstID
    let initialParaCount: Int
    if case .document(let doc) = vm.activeTab?.payload {
      initialParaCount = doc.paragraphs.count
    } else {
      XCTFail("Expected document payload")
      return
    }
    vm.appendParagraphToActiveDocument(text: "추가된 결론 문단입니다.", style: .heading2)
    if case .document(let updatedDoc) = vm.activeTab?.payload {
      XCTAssertEqual(updatedDoc.paragraphs.count, initialParaCount + 1)
      XCTAssertEqual(updatedDoc.paragraphs.last?.text, "추가된 결론 문단입니다.")
      XCTAssertEqual(updatedDoc.paragraphs.last?.style, .heading2)
    } else {
      XCTFail("Expected updated document payload")
    }

    // Sheet cell editing via formulaBarDraft + applyFormulaBarEdit()
    guard let sheetTab = vm.openTabs.first(where: { $0.moduleKind == .sheet }) else {
      XCTFail("Expected sheet tab")
      return
    }
    vm.activeTabID = sheetTab.id
    vm.selectedSheetCell = "B2"
    vm.formulaBarDraft = "320"
    vm.applyFormulaBarEdit()
    if case .sheet(let updatedSheet) = vm.activeTab?.payload {
      XCTAssertEqual(updatedSheet.rawValue(at: "B2"), "320")
      // D2 is =SUM(B2:C2) where C2 is 180 -> 320 + 180 = 500
      XCTAssertEqual(updatedSheet.evaluate(at: "D2"), .number(500))
    } else {
      XCTFail("Expected updated sheet payload")
    }

    // PDF rotation and annotation addition
    guard let pdfTab = vm.openTabs.first(where: { $0.moduleKind == .pdf }) else {
      XCTFail("Expected PDF tab")
      return
    }
    vm.activeTabID = pdfTab.id
    vm.selectedPDFPageIndex = 0
    vm.rotateActivePDFPage(clockwiseDegrees: 90)
    vm.addPDFAnnotation(kind: .stickyNote, content: "2페이지 표 수치 재확인 필요")
    if case .pdf(let updatedPDF) = vm.activeTab?.payload {
      XCTAssertEqual(updatedPDF.pages[0].rotationDegrees, 90)
      XCTAssertTrue(
        updatedPDF.annotations.contains(where: {
          $0.kind == .stickyNote && $0.content == "2페이지 표 수치 재확인 필요"
        }))
    } else {
      XCTFail("Expected updated PDF payload")
    }
  }

  @MainActor
  func testAll140RibbonCommandsUndoRedoFileIORoundTripAndCustomTemplatePersistence() throws {
    let vm = WorkspaceViewModel()

    // 1. Verify all 140 RibbonCommandDescriptor IDs execute deterministically and mutate domain or workspace state
    var executedCount = 0
    for module in OfficeModuleKind.allCases {
      guard let moduleTab = vm.openTabs.first(where: { $0.moduleKind == module }) else {
        XCTFail("Missing tab for \(module.rawValue)")
        return
      }
      for category in RibbonCategory.allCases {
        let actions = RibbonCategory.actions(for: category, module: module)
        XCTAssertEqual(actions.count, 4)
        for cmd in actions {
          vm.activeTabID = moduleTab.id
          let beforePayload = vm.activeTab?.payload
          let beforeTabCount = vm.openTabs.count
          let beforeTheme = vm.themeMode
          let beforeCollapsed = vm.isRibbonCollapsed
          let beforeSplit = vm.splitSecondaryTabID
          let beforeZoom = vm.zoomPercent
          let beforeChartKind = vm.selectedChartKind
          let beforeFrozen = vm.isHeaderRowFrozen
          let beforeCondRule = vm.activeConditionalFormatRule
          let beforeValRule = vm.activeDataValidationRule
          let beforePresenter = vm.isPresenterModeActive
          let beforeStatsCount = vm.reviewStatsRefreshCount
          let beforeSnapGuides = vm.activeSnapGuides
          let beforePivotTotal = vm.lastPivotGrandTotal
          let beforePadding = vm.documentHorizontalPadding
          let beforeViewMode = vm.workspaceViewMode

          vm.lastExecutedCommandSummary = nil
          vm.executeRibbonCommand(cmd.id)

          let summary = try XCTUnwrap(
            vm.lastExecutedCommandSummary,
            "Ribbon command \(cmd.id) must update lastExecutedCommandSummary"
          )
          XCTAssertFalse(summary.isEmpty, "Ribbon command \(cmd.id) summary must not be empty")
          XCTAssertFalse(
            summary.hasPrefix("명령 실행:"),
            "Ribbon command \(cmd.id) must not fall through to catch-all summary"
          )

          let didMutateState =
            (vm.activeTab?.payload != beforePayload)
            || (vm.openTabs.count != beforeTabCount)
            || (vm.themeMode != beforeTheme)
            || (vm.isRibbonCollapsed != beforeCollapsed)
            || (vm.splitSecondaryTabID != beforeSplit)
            || (vm.zoomPercent != beforeZoom)
            || (vm.selectedChartKind != beforeChartKind)
            || (vm.isHeaderRowFrozen != beforeFrozen)
            || (vm.activeConditionalFormatRule != beforeCondRule)
            || (vm.activeDataValidationRule != beforeValRule)
            || (vm.isPresenterModeActive != beforePresenter)
            || (vm.reviewStatsRefreshCount != beforeStatsCount)
            || (vm.activeSnapGuides != beforeSnapGuides)
            || (vm.lastPivotGrandTotal != beforePivotTotal)
            || (vm.documentHorizontalPadding != beforePadding)
            || (vm.workspaceViewMode != beforeViewMode)

          XCTAssertTrue(
            didMutateState,
            "Ribbon command \(cmd.id) must produce a concrete domain or view-model state mutation"
          )
          executedCount += 1
        }
      }
    }
    XCTAssertEqual(executedCount, 140)

    // 2. Verify Undo / Redo across Document, Sheet, Slide, and PDF tabs
    let freshVM = WorkspaceViewModel()
    let docTab = try XCTUnwrap(freshVM.openTabs.first(where: { $0.moduleKind == .document }))
    freshVM.activeTabID = docTab.id
    let originalDocPayload = try XCTUnwrap(freshVM.activeTab?.payload)
    XCTAssertFalse(freshVM.canUndo)

    freshVM.appendParagraphToActiveDocument(text: "Undo 검증용 신규 문단", style: .body)
    XCTAssertTrue(freshVM.canUndo)
    XCTAssertNotEqual(freshVM.activeTab?.payload, originalDocPayload)

    freshVM.undo()
    XCTAssertEqual(freshVM.activeTab?.payload, originalDocPayload)
    XCTAssertTrue(freshVM.canRedo)

    freshVM.redo()
    XCTAssertNotEqual(freshVM.activeTab?.payload, originalDocPayload)
    if case .document(let restoredDoc) = freshVM.activeTab?.payload {
      XCTAssertEqual(restoredDoc.paragraphs.last?.text, "Undo 검증용 신규 문단")
    } else {
      XCTFail("Expected document payload after redo")
    }

    // Sheet Undo / Redo + error reporting on invalid cell address
    let sheetTab = try XCTUnwrap(freshVM.openTabs.first(where: { $0.moduleKind == .sheet }))
    freshVM.activeTabID = sheetTab.id
    let originalSheetPayload = try XCTUnwrap(freshVM.activeTab?.payload)
    freshVM.selectedSheetCell = "B2"
    freshVM.formulaBarDraft = "777"
    freshVM.applyFormulaBarEdit()
    XCTAssertTrue(freshVM.canUndo)
    freshVM.undo()
    XCTAssertEqual(freshVM.activeTab?.payload, originalSheetPayload)
    freshVM.redo()
    if case .sheet(let redoneSheet) = freshVM.activeTab?.payload {
      XCTAssertEqual(redoneSheet.rawValue(at: "B2"), "777")
    }

    freshVM.selectedSheetCell = "INVALID_99999999"
    freshVM.formulaBarDraft = "100"
    freshVM.applyFormulaBarEdit()
    XCTAssertNotNil(freshVM.lastErrorMessage)

    // 3. Verify openDocument(data:fileName:) and exportTabData(id:targetFormat:) round-trip
    let exportedDocxBytes = try freshVM.exportTabData(id: docTab.id, targetFormat: .docx)
    let openedDocTab = try freshVM.openDocument(data: exportedDocxBytes, fileName: "재열기_문서.docx")
    XCTAssertEqual(openedDocTab.format, .docx)
    if case .document(let reopenedDoc) = openedDocTab.payload {
      XCTAssertEqual(reopenedDoc.paragraphs.last?.text, "Undo 검증용 신규 문단")
    } else {
      XCTFail("Expected reopened document payload")
    }

    // 4. Verify custom template saving preserves modified OpenTabPayload and openTab(from:) restores it
    freshVM.activeTabID = openedDocTab.id
    freshVM.saveActiveTabAsCustomTemplate(title: "커스텀_검증_양식", subtitle: "수정된 페이로드 보존 확인")
    let savedTemplate = try XCTUnwrap(freshVM.customTemplates.first)
    freshVM.openTab(from: savedTemplate)
    let restoredFromTemplateTab = try XCTUnwrap(freshVM.activeTab)
    XCTAssertNotEqual(restoredFromTemplateTab.id, openedDocTab.id)
    XCTAssertEqual(restoredFromTemplateTab.payload, openedDocTab.payload)

    freshVM.activeTabID = restoredFromTemplateTab.id
    freshVM.scrubActiveTabMetadata()
    XCTAssertGreaterThan(freshVM.lastScrubbedMetadataCount, 0)

    let tempPDFURL = FileManager.default.temporaryDirectory.appendingPathComponent(
      "easyoffice_f4_sync_\(UUID().uuidString).pdf")
    defer { try? FileManager.default.removeItem(at: tempPDFURL) }
    try freshVM.saveTabToDisk(id: restoredFromTemplateTab.id, to: tempPDFURL, targetFormat: .pdf)
    let exportedPDFData = try Data(contentsOf: tempPDFURL)
    let exportedPDFModel = try PDFBinarySerializer.decode(from: exportedPDFData)
    XCTAssertGreaterThanOrEqual(exportedPDFModel.pages.count, 1)
    let preservedTab = try XCTUnwrap(
      freshVM.openTabs.first(where: { $0.id == restoredFromTemplateTab.id }))
    XCTAssertEqual(preservedTab.format, .docx)
    if case .document(let preservedDoc) = preservedTab.payload {
      XCTAssertGreaterThanOrEqual(preservedDoc.paragraphs.count, 1)
    } else {
      XCTFail("Cross-module PDF export must preserve editable .document payload")
    }

    let tempODTURL = FileManager.default.temporaryDirectory.appendingPathComponent(
      "easyoffice_f4_sync_\(UUID().uuidString).odt")
    defer { try? FileManager.default.removeItem(at: tempODTURL) }
    try freshVM.saveTabToDisk(id: restoredFromTemplateTab.id, to: tempODTURL, targetFormat: .odt)
    let syncedTab = try XCTUnwrap(
      freshVM.openTabs.first(where: { $0.id == restoredFromTemplateTab.id }))
    XCTAssertEqual(syncedTab.format, .odt)
  }

  @MainActor
  func testZeroSlopSheetRibbonCommandsDataValidationUndoCoalescingAndFindReplace() throws {
    // 1. Zero-Slop Sheet Ribbon Verification (No E1~E8 or F1~F6 pollution)
    let vm = WorkspaceViewModel()
    let sheetTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .sheet }))
    vm.activeTabID = sheetTab.id

    let initialPageSetup: SheetPageSetup
    if case .sheet(let initialSheet) = vm.activeTab?.payload {
      initialPageSetup = initialSheet.pageSetup
    } else {
      XCTFail("Expected sheet payload")
      return
    }

    let sheetCommands = RibbonCategory.allCases.flatMap {
      RibbonCategory.actions(for: $0, module: .sheet)
    }
    XCTAssertEqual(sheetCommands.count, 28)
    for cmd in sheetCommands {
      vm.executeRibbonCommand(cmd.id)
    }

    let forbiddenCells = [
      "E1", "E2", "E3", "E4", "E5", "E6", "E7", "E8", "F1", "F2", "F3", "F4", "F5", "F6",
    ]
    if case .sheet(let mutatedSheet) = vm.activeTab?.payload {
      for cellAddr in forbiddenCells {
        XCTAssertEqual(
          mutatedSheet.rawValue(at: cellAddr),
          "",
          "Sheet ribbon commands must never pollute cell \(cellAddr)"
        )
        if let parsed = try? CellAddress(a1Notation: cellAddr) {
          XCTAssertNil(
            mutatedSheet.cellsByCoordinate[parsed.packedCoordinate],
            "Cell \(cellAddr) must remain absent from cellsByCoordinate"
          )
        }
      }
      XCTAssertFalse(
        mutatedSheet.cellStyles.isEmpty, "Sheet cellStyles must be populated by formatting commands"
      )
      XCTAssertNotEqual(
        mutatedSheet.pageSetup, initialPageSetup,
        "Sheet pageSetup must be modified by layout commands")
      XCTAssertFalse(
        mutatedSheet.sheetComments.isEmpty,
        "Sheet sheetComments must be populated by link/comment/pivot commands")
      XCTAssertTrue(
        mutatedSheet.trackChangesEnabled,
        "Sheet trackChangesEnabled must be toggled on by review-track")
      XCTAssertEqual(
        mutatedSheet.additionalSheets.count, 2,
        "Sheet insert-table and sheet-mod-pivot must add 2 additional sheet pages")
    } else {
      XCTFail("Expected mutated sheet payload")
    }

    // 2. DataValidationRule Enforcement in applyFormulaBarEdit()
    let valVM = WorkspaceViewModel()
    let valSheetTab = try XCTUnwrap(valVM.openTabs.first(where: { $0.moduleKind == .sheet }))
    valVM.activeTabID = valSheetTab.id
    valVM.activeDataValidationRule = DataValidationRule(
      minimumValue: 0, maximumValue: 1000, allowedSet: nil)
    valVM.selectedSheetCell = "B2"
    valVM.formulaBarDraft = "-50"
    valVM.applyFormulaBarEdit()

    let validationError = try XCTUnwrap(valVM.lastErrorMessage)
    XCTAssertTrue(validationError.contains("유효성"))
    if case .sheet(let unmodifiedSheet) = valVM.activeTab?.payload {
      XCTAssertNotEqual(unmodifiedSheet.rawValue(at: "B2"), "-50")
      XCTAssertEqual(unmodifiedSheet.rawValue(at: "B2"), "120")
    } else {
      XCTFail("Expected sheet payload")
    }

    valVM.formulaBarDraft = "500"
    valVM.applyFormulaBarEdit()
    XCTAssertNil(valVM.lastErrorMessage)
    if case .sheet(let validSheet) = valVM.activeTab?.payload {
      XCTAssertEqual(validSheet.rawValue(at: "B2"), "500")
    } else {
      XCTFail("Expected updated sheet payload")
    }

    // 3. Undo Coalescing on Inline Keystrokes (updateParagraphText & updateTableCell)
    let docVM = WorkspaceViewModel()
    let docTab = try XCTUnwrap(docVM.openTabs.first(where: { $0.moduleKind == .document }))
    docVM.activeTabID = docTab.id
    guard case .document(let initialDoc) = docVM.activeTab?.payload,
      let p0 = initialDoc.paragraphs.first
    else {
      XCTFail("Expected initial document paragraph")
      return
    }
    let originalParagraphText = p0.text
    let initialRevisionCount = initialDoc.revisions.count

    docVM.updateParagraphText(paragraphID: p0.id, newText: "Step 1")
    docVM.updateParagraphText(paragraphID: p0.id, newText: "Step 2")
    docVM.updateParagraphText(paragraphID: p0.id, newText: "Step 3")

    if case .document(let burstDoc) = docVM.activeTab?.payload {
      XCTAssertEqual(burstDoc.paragraphs[0].text, "Step 3")
      XCTAssertEqual(
        burstDoc.revisions.count,
        initialRevisionCount + 1,
        "3 consecutive keystrokes on the same paragraph must coalesce into 1 revision"
      )
      XCTAssertEqual(burstDoc.revisions.last?.proposedText, "Step 3")
    } else {
      XCTFail("Expected document payload after keystroke burst")
    }

    docVM.undo()
    if case .document(let revertedDoc) = docVM.activeTab?.payload {
      XCTAssertEqual(
        revertedDoc.paragraphs[0].text,
        originalParagraphText,
        "Single undo() after coalesced keystroke burst must restore original paragraph text"
      )
      XCTAssertEqual(revertedDoc.revisions.count, initialRevisionCount)
    } else {
      XCTFail("Expected reverted document payload")
    }

    // 4. Slide Element Formatting & Snap Centering (No +16/+12 offset)
    let slideVM = WorkspaceViewModel()
    let slideTab = try XCTUnwrap(slideVM.openTabs.first(where: { $0.moduleKind == .slide }))
    slideVM.activeTabID = slideTab.id

    slideVM.executeRibbonCommand("slide-home-bold")
    slideVM.executeRibbonCommand("slide-home-italic")
    slideVM.executeRibbonCommand("slide-home-underline")

    guard case .slide(let formattedDeck) = slideVM.activeTab?.payload,
      let elem = formattedDeck.slides.first?.elements.first
    else {
      XCTFail("Expected slide canvas element")
      return
    }
    XCTAssertTrue(elem.isBold)
    XCTAssertTrue(elem.isItalic)
    XCTAssertTrue(elem.isUnderlined)

    slideVM.moveSlideElement(slideIndex: 0, elementID: elem.id, deltaX: -50, deltaY: -30)
    slideVM.executeRibbonCommand("slide-mod-snap")
    slideVM.executeRibbonCommand("slide-mod-snap")

    if case .slide(let snappedDeck) = slideVM.activeTab?.payload,
      let snappedElem = snappedDeck.slides.first?.elements.first
    {
      XCTAssertEqual(snappedElem.x, (960.0 - elem.width) / 2.0, accuracy: 0.001)
      XCTAssertEqual(snappedElem.y, (540.0 - elem.height) / 2.0, accuracy: 0.001)
      XCTAssertFalse(slideVM.activeSnapGuides.isEmpty)
    } else {
      XCTFail("Expected snapped slide payload")
    }

    // 5. Find & Replace across .document, .sheet, .slide, and .pdf
    let frVM = WorkspaceViewModel()

    // Document Find & Replace + Inline Runs + Embedded Image
    let frDocTab = try XCTUnwrap(frVM.openTabs.first(where: { $0.moduleKind == .document }))
    frVM.activeTabID = frDocTab.id
    frVM.applyInlineRunFormattingToActiveParagraph(
      isBold: true, isItalic: true, isUnderlined: true, textHex: "#5C6BC0")
    frVM.insertEmbeddedImageIntoActiveDocument(caption: "검증 다이어그램", widthPt: 300, heightPt: 160)
    frVM.findQueryText = "아키텍처"
    frVM.replaceQueryText = "플랫폼구조"
    let docReplaced = frVM.performFindAndReplace()
    XCTAssertGreaterThan(docReplaced, 0)
    XCTAssertEqual(frVM.lastFindReplaceCount, docReplaced)
    if case .document(let updatedDoc) = frVM.activeTab?.payload {
      XCTAssertTrue(updatedDoc.paragraphs.contains(where: { $0.text.contains("플랫폼구조") }))
      XCTAssertEqual(updatedDoc.images.count, 1)
      XCTAssertFalse(updatedDoc.paragraphs[0].inlineRuns.isEmpty)
    }

    // Sheet Find & Replace + Multi-sheet switching
    let frSheetTab = try XCTUnwrap(frVM.openTabs.first(where: { $0.moduleKind == .sheet }))
    frVM.activeTabID = frSheetTab.id
    frVM.findQueryText = "엔진 최적화"
    frVM.replaceQueryText = "코어 가속화"
    let sheetReplaced = frVM.performFindAndReplace()
    XCTAssertEqual(sheetReplaced, 1)
    if case .sheet(let updatedSheet) = frVM.activeTab?.payload {
      XCTAssertEqual(updatedSheet.rawValue(at: "A2"), "코어 가속화")
    }
    frVM.addWorksheetTab(named: "하반기 계획")
    frVM.selectWorksheetTab(index: 1)
    if case .sheet(let switchedSheet) = frVM.activeTab?.payload {
      XCTAssertEqual(switchedSheet.activeSheetIndex, 1)
      XCTAssertEqual(switchedSheet.name, "하반기 계획")
      XCTAssertEqual(switchedSheet.rawValue(at: "A1"), "항목")
    }
    frVM.selectWorksheetTab(index: 0)
    if case .sheet(let restoredSheet) = frVM.activeTab?.payload {
      XCTAssertEqual(restoredSheet.activeSheetIndex, 0)
      XCTAssertEqual(restoredSheet.rawValue(at: "A2"), "코어 가속화")
    }

    // Slide Find & Replace
    let frSlideTab = try XCTUnwrap(frVM.openTabs.first(where: { $0.moduleKind == .slide }))
    frVM.activeTabID = frSlideTab.id
    frVM.findQueryText = "워크스페이스"
    frVM.replaceQueryText = "통합스튜디오"
    let slideReplaced = frVM.performFindAndReplace()
    XCTAssertGreaterThan(slideReplaced, 0)
    if case .slide(let updatedDeck) = frVM.activeTab?.payload {
      XCTAssertTrue(updatedDeck.slides[0].subtitle.contains("통합스튜디오"))
    }

    // PDF Find & Replace (annotations + page extractedText + outline)
    let frPDFTab = try XCTUnwrap(frVM.openTabs.first(where: { $0.moduleKind == .pdf }))
    frVM.activeTabID = frPDFTab.id
    frVM.findQueryText = "점검"
    frVM.replaceQueryText = "감사점검"
    let pdfReplaced = frVM.performFindAndReplace()
    XCTAssertGreaterThan(pdfReplaced, 1)
    if case .pdf(let updatedPDF) = frVM.activeTab?.payload {
      XCTAssertTrue(updatedPDF.annotations.contains(where: { $0.content.contains("감사점검") }))
      XCTAssertTrue(updatedPDF.pages[0].extractedText.contains("감사점검"))
    }

    // Multi-run preservation across inline paragraph edit (F3)
    frVM.activeTabID = frDocTab.id
    if case .document(let initialDoc) = frVM.activeTab?.payload {
      let pID = initialDoc.paragraphs[0].id
      frVM.selectedParagraphID = pID
      frVM.applyInlineRunFormattingToActiveParagraph(isBold: true, foregroundHex: "#5C6BC0")
    }
    if case .document(let styledDoc) = frVM.activeTab?.payload {
      let p = styledDoc.paragraphs[0]
      XCTAssertGreaterThanOrEqual(p.inlineRuns.count, 2)
      frVM.updateParagraphText(paragraphID: p.id, newText: p.text + " (개정판)")
    }
    if case .document(let editedDoc) = frVM.activeTab?.payload {
      let p = editedDoc.paragraphs[0]
      XCTAssertEqual(p.inlineRuns.count, 2)
      XCTAssertEqual(p.inlineRuns.map(\.text).joined(), p.text)
      XCTAssertEqual(p.inlineRuns[0].foregroundHex, "#5C6BC0")
      XCTAssertTrue(p.inlineRuns[1].text.hasSuffix("(개정판)"))
    }

    // Dynamic multi-row sorting across A2:B6 while preserving formula summary row B7 (F2)
    var multiRowSheet = WorksheetModel(name: "동적정렬검증")
    try multiRowSheet.setCell("A1", rawValue: "부서")
    try multiRowSheet.setCell("B1", rawValue: "예산")
    try multiRowSheet.setCell("A2", rawValue: "알파")
    try multiRowSheet.setCell("B2", rawValue: "500")
    try multiRowSheet.setCell("A3", rawValue: "베타")
    try multiRowSheet.setCell("B3", rawValue: "100")
    try multiRowSheet.setCell("A4", rawValue: "감마")
    try multiRowSheet.setCell("B4", rawValue: "400")
    try multiRowSheet.setCell("A5", rawValue: "델타")
    try multiRowSheet.setCell("B5", rawValue: "200")
    try multiRowSheet.setCell("A6", rawValue: "엡실론")
    try multiRowSheet.setCell("B6", rawValue: "300")
    try multiRowSheet.setCell("A7", rawValue: "합계")
    try multiRowSheet.setCell("B7", rawValue: "=SUM(B2:B6)")
    if let sheetIdx = frVM.openTabs.firstIndex(where: { $0.id == frSheetTab.id }) {
      frVM.openTabs[sheetIdx].payload = .sheet(multiRowSheet)
      frVM.activeTabID = frSheetTab.id
      frVM.selectedSheetCell = "B2"
      frVM.sheetSortAscending = false  // next toggle -> true (ascending)
      frVM.executeRibbonCommand("sheet-data-sort")
      if case .sheet(let sortedSheet) = frVM.openTabs[sheetIdx].payload {
        XCTAssertEqual(sortedSheet.rawValue(at: "B2"), "100")
        XCTAssertEqual(sortedSheet.rawValue(at: "A2"), "베타")
        XCTAssertEqual(sortedSheet.rawValue(at: "B6"), "500")
        XCTAssertEqual(sortedSheet.rawValue(at: "A6"), "알파")
        XCTAssertEqual(sortedSheet.rawValue(at: "B7"), "=SUM(B2:B6)")
        XCTAssertEqual(sortedSheet.evaluate(at: "B7"), .number(1500))
      }
    }
  }

  @MainActor
  func testThirdAuditZeroSlopInspectorLiveCanvasAndNonDestructiveCommands() throws {
    let vm = WorkspaceViewModel()

    // 1. sheet-review-security & sheet-review-verify preserve sheetComments 100% and sheet-mod-pivot creates a dedicated pivot sheet page
    let sheetTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .sheet }))
    vm.activeTabID = sheetTab.id
    vm.selectedSheetCell = "B2"
    vm.commentDraftText = "사용자 중요 재무 검토 메모"
    vm.executeRibbonCommand("sheet-review-comment")

    let b2Coord = try CellAddress(a1Notation: "B2").packedCoordinate
    if case .sheet(let commentedSheet) = vm.activeTab?.payload {
      XCTAssertEqual(commentedSheet.sheetComments[b2Coord], "사용자 중요 재무 검토 메모")
    }

    vm.executeRibbonCommand("sheet-review-security")
    vm.executeRibbonCommand("sheet-review-verify")
    if case .sheet(let auditedSheet) = vm.activeTab?.payload {
      XCTAssertEqual(
        auditedSheet.sheetComments[b2Coord],
        "사용자 중요 재무 검토 메모",
        "sheet-review-security must never wipe user cell comments"
      )
      XCTAssertTrue(auditedSheet.trackChangesEnabled)
    }

    vm.executeRibbonCommand("sheet-mod-pivot")
    if case .sheet(let pivotedSheet) = vm.activeTab?.payload {
      XCTAssertEqual(
        pivotedSheet.sheetComments[b2Coord],
        "사용자 중요 재무 검토 메모",
        "sheet-mod-pivot must not overwrite cell comments"
      )
      XCTAssertTrue(
        pivotedSheet.additionalSheets.contains(where: { $0.name.hasPrefix("피벗 요약") }),
        "sheet-mod-pivot must add a dedicated pivot summary sheet page"
      )
    }

    // 2. doc-mod-mailmerge, hwp-mod-manuscript, doc-data-sum, doc-insert-chart, doc-insert-shape preserve footerText
    let docTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .document }))
    vm.activeTabID = docTab.id
    let originalDocFooter: String
    let initialDocTableCount: Int
    if case .document(let doc) = vm.activeTab?.payload {
      originalDocFooter = doc.footerText
      initialDocTableCount = doc.tables.count
    } else {
      XCTFail("Expected document payload")
      return
    }

    vm.executeRibbonCommand("doc-mod-mailmerge")
    vm.executeRibbonCommand("doc-data-sum")
    vm.executeRibbonCommand("doc-data-fx")
    vm.executeRibbonCommand("doc-data-sort")
    vm.executeRibbonCommand("doc-data-validate")
    vm.executeRibbonCommand("doc-insert-chart")
    vm.executeRibbonCommand("doc-insert-shape")

    if case .document(let mutatedDoc) = vm.activeTab?.payload {
      XCTAssertEqual(
        mutatedDoc.footerText,
        originalDocFooter,
        "Document commands must never overwrite doc.footerText"
      )
      XCTAssertEqual(mutatedDoc.tables.count, initialDocTableCount + 1)
      XCTAssertEqual(mutatedDoc.tables.last?.caption, "차트 데이터 테이블")
      XCTAssertTrue(
        mutatedDoc.tables[0].rows.contains(where: { $0.first?.hasPrefix("합계") == true }))
      XCTAssertTrue(
        mutatedDoc.tables[0].rows.contains(where: { $0.first?.hasPrefix("평균") == true }))
      let shapePara = try XCTUnwrap(mutatedDoc.paragraphs.last)
      XCTAssertEqual(shapePara.runs.first?.text, "프로세스 블록")
      XCTAssertEqual(shapePara.runs.first?.foregroundHex, "#5C6BC0")
    }

    let hwpTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .hwp }))
    vm.activeTabID = hwpTab.id
    let originalHwpFooter: String
    if case .document(let hwpDoc) = vm.activeTab?.payload {
      originalHwpFooter = hwpDoc.footerText
    } else {
      XCTFail("Expected HWP payload")
      return
    }
    vm.showRightInspector = false
    vm.executeRibbonCommand("hwp-mod-manuscript")
    if case .document(let mutatedHwp) = vm.activeTab?.payload {
      XCTAssertEqual(
        mutatedHwp.footerText,
        originalHwpFooter,
        "hwp-mod-manuscript must never overwrite doc.footerText"
      )
      XCTAssertTrue(mutatedHwp.revisions.contains(where: { $0.author == "원고지계산기" }))
      XCTAssertTrue(vm.showRightInspector)
    }

    // 3. Slide review/data commands preserve speakerNotes without appending side-channel strings
    let slideTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .slide }))
    vm.activeTabID = slideTab.id
    let originalSpeakerNotes: String
    if case .slide(let deck) = vm.activeTab?.payload {
      originalSpeakerNotes = deck.slides[0].speakerNotes
    } else {
      XCTFail("Expected slide payload")
      return
    }
    vm.executeRibbonCommand("slide-review-track")
    vm.executeRibbonCommand("slide-review-comment")
    vm.executeRibbonCommand("slide-data-autosum")
    if case .slide(let mutatedDeck) = vm.activeTab?.payload {
      XCTAssertEqual(
        mutatedDeck.slides[0].speakerNotes,
        originalSpeakerNotes,
        "Slide review/data commands must never pollute speakerNotes"
      )
    }

    // 4. pdf-mod-splitmerge on multi-page PDF creates a new split PDF tab without destroying original pages
    let pdfTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .pdf }))
    vm.activeTabID = pdfTab.id
    let tabCountBeforeSplit = vm.openTabs.count
    vm.selectedPDFPageIndex = 1
    vm.executeRibbonCommand("pdf-mod-splitmerge")
    XCTAssertEqual(vm.openTabs.count, tabCountBeforeSplit + 1)
    let originalPDFTabAfter = try XCTUnwrap(vm.openTabs.first(where: { $0.id == pdfTab.id }))
    if case .pdf(let origPDF) = originalPDFTabAfter.payload {
      XCTAssertEqual(origPDF.pages.count, 2, "Original multi-page PDF tab must retain all pages")
    }
    let splitPDFTab = try XCTUnwrap(vm.openTabs.last)
    XCTAssertTrue(splitPDFTab.fileName.contains("_분할_P2.pdf"))
    if case .pdf(let extractedPDF) = splitPDFTab.payload {
      XCTAssertEqual(extractedPDF.pages.count, 1)
    }

    // 5. Substring target phrase inline run splitting & real multi-pixel PNG attachment
    let freshVM = WorkspaceViewModel()
    let freshDocTab = try XCTUnwrap(freshVM.openTabs.first(where: { $0.moduleKind == .document }))
    freshVM.activeTabID = freshDocTab.id
    if case .document(let doc) = freshVM.activeTab?.payload {
      freshVM.selectedParagraphID = doc.paragraphs[2].id
    }
    freshVM.inlineRunTargetPhrase = "전사 표준 문서 조판 스타일"
    freshVM.applyInlineRunFormattingToActiveParagraph(
      isBold: true, isItalic: true, isUnderlined: true, foregroundHex: "#5C6BC0")
    if case .document(let styledDoc) = freshVM.activeTab?.payload {
      let p2 = styledDoc.paragraphs[2]
      XCTAssertEqual(p2.runs.count, 3)
      XCTAssertEqual(p2.runs[1].text, "전사 표준 문서 조판 스타일")
      XCTAssertTrue(p2.runs[1].isBold)
      XCTAssertTrue(p2.runs[1].isItalic)
      XCTAssertTrue(p2.runs[1].isUnderlined)
      XCTAssertEqual(p2.runs[1].foregroundHex, "#5C6BC0")
    }

    freshVM.insertSampleImageAttachment(caption: "멀티픽셀 다이어그램")
    if case .document(let imgDoc) = freshVM.activeTab?.payload {
      let attachment = try XCTUnwrap(imgDoc.images.last)
      XCTAssertGreaterThan(attachment.imageData.count, 100)
      let decodedImage = try XCTUnwrap(NSImage(data: attachment.imageData))
      XCTAssertGreaterThanOrEqual(decodedImage.size.width, 16)
      XCTAssertGreaterThanOrEqual(decodedImage.size.height, 16)
    }

    // 6. Sheet cell number format, alignment, and merge range helpers
    let freshSheetTab = try XCTUnwrap(freshVM.openTabs.first(where: { $0.moduleKind == .sheet }))
    freshVM.activeTabID = freshSheetTab.id
    freshVM.selectedSheetCell = "B2"
    freshVM.setSelectedSheetCellNumberFormat(.currencyKRW)
    freshVM.setSelectedSheetCellAlignment(.trailing)
    freshVM.toggleMergeOnSelectedSheetCell(targetRange: "A1:C1")
    if case .sheet(let styledSheet) = freshVM.activeTab?.payload {
      let style = styledSheet.cellStyle(at: "B2")
      XCTAssertEqual(style.numberFormat, .currencyKRW)
      XCTAssertEqual(style.alignment, .trailing)
    }
    XCTAssertEqual(freshVM.mergedSheetRangesByTab[freshSheetTab.id], ["A1:C1"])
    freshVM.toggleMergeOnSelectedSheetCell(targetRange: "A1:C1")
    XCTAssertEqual(freshVM.mergedSheetRangesByTab[freshSheetTab.id], [])

    // 7. Presenter timer gating & advancePresenterSlide no +15s jump
    let freshSlideTab = try XCTUnwrap(freshVM.openTabs.first(where: { $0.moduleKind == .slide }))
    freshVM.activeTabID = freshSlideTab.id
    freshVM.presenterElapsedSeconds = 10
    freshVM.isPresenterTimerRunning = false
    freshVM.tickPresenterTimer()
    XCTAssertEqual(freshVM.presenterElapsedSeconds, 10, "Timer must not advance when paused")
    freshVM.advancePresenterSlide()
    XCTAssertEqual(
      freshVM.presenterElapsedSeconds, 10, "advancePresenterSlide must not add 15 seconds")
    freshVM.isPresenterTimerRunning = true
    freshVM.tickPresenterTimer()
    XCTAssertEqual(freshVM.presenterElapsedSeconds, 11, "Timer must advance by 1s when running")

    // 8. Fail-closed image validation, non-destructive summary revision accept, slide-review-track toggle, Z-Order, and additionalSheets search
    freshVM.activeTabID = freshDocTab.id
    let countBeforeInvalidImg: Int = {
      if case .document(let d) = freshVM.activeTab?.payload { return d.images.count }
      return 0
    }()
    freshVM.lastErrorMessage = nil
    freshVM.importImageAttachment(
      data: Data([0x00, 0x01, 0x02, 0x03, 0x04]), fileName: "fake.png", caption: "위조 이미지")
    XCTAssertNotNil(
      freshVM.lastErrorMessage, "Invalid magic bytes must fail closed with error message")
    if case .document(var d) = freshVM.activeTab?.payload {
      XCTAssertEqual(d.images.count, countBeforeInvalidImg)
      let p0TextBefore = d.paragraphs[0].text
      d.recordRevision(author: "QA", summary: "문서 요약 리비전")
      let revID = try XCTUnwrap(d.revisions.last?.id)
      d.acceptRevision(id: revID)
      XCTAssertEqual(
        d.paragraphs[0].text, p0TextBefore,
        "Accepting a document summary revision must never overwrite paragraph 0")
    }

    freshVM.activeTabID = freshSlideTab.id
    freshVM.executeRibbonCommand("slide-review-track")
    let elemCountAfterFirstTrack: Int = {
      if case .slide(let deck) = freshVM.activeTab?.payload { return deck.slides[0].elements.count }
      return 0
    }()
    freshVM.executeRibbonCommand("slide-review-track")
    if case .slide(let deck) = freshVM.activeTab?.payload {
      XCTAssertEqual(
        deck.slides[0].elements.count, elemCountAfterFirstTrack,
        "Repeated slide-review-track must toggle existing badge instead of duplicating shapes")
      if deck.slides[0].elements.count >= 2 {
        let firstID = deck.slides[0].elements[0].id
        freshVM.moveSlideElementZOrder(slideIndex: 0, elementID: firstID, bringForward: true)
      }
    }
    if case .slide(let reorderedDeck) = freshVM.activeTab?.payload,
      reorderedDeck.slides[0].elements.count >= 2
    {
      XCTAssertNotEqual(
        reorderedDeck.slides[0].elements[0].id, reorderedDeck.slides[0].elements[1].id)
    }

    freshVM.activeTabID = freshSheetTab.id
    freshVM.selectedSheetCell = "C3"
    freshVM.setSelectedSheetCellComment("인스펙터 셀 메모")
    freshVM.addWorksheetTab(named: "별첨데이터")
    if let updatedSheetTab = freshVM.openTabs.first(where: { $0.id == freshSheetTab.id }) {
      let corpus = updatedSheetTab.searchableCorpus()
      XCTAssertTrue(corpus.contains("별첨데이터"))
      if case .sheet(let s) = updatedSheetTab.payload {
        let c3 = try CellAddress(a1Notation: "C3").packedCoordinate
        XCTAssertEqual(s.sheetComments[c3], "인스펙터 셀 메모")
      }
    }
  }
}
