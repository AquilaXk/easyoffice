import XCTest
@testable import EasyOfficeDesignSystem
@testable import EasyOfficeCore

final class WorkspaceUIParityTests: XCTestCase {

    func testThemeModesSemanticStatusTokensLayoutMetricsAndAccessibility() {
        // Semantic Status Tokens
        XCTAssertEqual(EasyOfficeDesignTokens.statusSuccess.hex, "#0A705A")
        XCTAssertEqual(EasyOfficeDesignTokens.statusWarning.hex, "#B45309")
        XCTAssertEqual(EasyOfficeDesignTokens.statusDanger.hex, "#C81E1E")
        XCTAssertEqual(EasyOfficeDesignTokens.statusInfo.hex, "#4A58A9")

        // Verify WCAG 2.1 AA contrast ratios (>= 4.5) against lightWhite surface
        XCTAssertGreaterThanOrEqual(
            EasyOfficeDesignTokens.statusSuccess.contrastRatio(against: EasyOfficeDesignTokens.lightWhite),
            4.5
        )
        XCTAssertGreaterThanOrEqual(
            EasyOfficeDesignTokens.statusWarning.contrastRatio(against: EasyOfficeDesignTokens.lightWhite),
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
        let pinnedModifiedTabLabel = AccessibilityTokens.tabLabel(fileName: "기획안.docx", module: .document, isPinned: true, isModified: true)
        let plainTabLabel = AccessibilityTokens.tabLabel(fileName: "기획안.docx", module: .document, isPinned: false, isModified: false)
        XCTAssertTrue(pinnedModifiedTabLabel.contains("기획안.docx"))
        XCTAssertNotEqual(pinnedModifiedTabLabel, plainTabLabel)
        XCTAssertFalse(AccessibilityTokens.tabHint.isEmpty)
        let expandedRibbonLabel = AccessibilityTokens.ribbonCollapseToggleLabel(isCollapsed: false)
        let collapsedRibbonLabel = AccessibilityTokens.ribbonCollapseToggleLabel(isCollapsed: true)
        XCTAssertNotEqual(expandedRibbonLabel, collapsedRibbonLabel)
        let primaryPaneLabel = AccessibilityTokens.splitViewPaneLabel(isPrimary: true, fileName: "문서A.hwpx")
        let secondaryPaneLabel = AccessibilityTokens.splitViewPaneLabel(isPrimary: false, fileName: "문서B.hwpx")
        XCTAssertTrue(primaryPaneLabel.contains("문서A.hwpx"))
        XCTAssertTrue(secondaryPaneLabel.contains("문서B.hwpx"))
        XCTAssertNotEqual(primaryPaneLabel, secondaryPaneLabel)
        XCTAssertTrue(AccessibilityTokens.formulaBarLabel(selectedCell: "D2").contains("D2"))
        XCTAssertTrue(AccessibilityTokens.slideStageLabel(slideNumber: 1, totalSlides: 5).contains("5"))
        XCTAssertTrue(AccessibilityTokens.pdfAnnotationToolLabel(toolTitle: "형광펜 하이라이트").contains("형광펜 하이라이트"))
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
        let recentFiltered = WorkspaceHubCatalog.filterTabs([tabDoc, tabSheet, tabSlide], section: .recent, query: "")
        XCTAssertEqual(recentFiltered.map(\.id), [tabSheet.id, tabSlide.id, tabDoc.id])

        // Full-text query across formulas, speaker notes, and PDF annotations
        let formulaMatches = WorkspaceHubCatalog.filterTabs(tabs, section: .home, query: "=SUM(B2:C2)")
        XCTAssertEqual(formulaMatches.map(\.id), [tabSheet.id])

        let notesMatches = WorkspaceHubCatalog.filterTabs(tabs, section: .home, query: "오프닝 슬라이드")
        XCTAssertEqual(notesMatches.map(\.id), [tabSlide.id])

        let pdfAnnotationMatches = WorkspaceHubCatalog.filterTabs(tabs, section: .home, query: "매직 바이트 시그니처 검증 필수")
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
              let secondID = vm.openTabs.dropFirst().first?.id else {
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
        if case let .document(doc) = vm.activeTab?.payload {
            initialParaCount = doc.paragraphs.count
        } else {
            XCTFail("Expected document payload")
            return
        }
        vm.appendParagraphToActiveDocument(text: "추가된 결론 문단입니다.", style: .heading2)
        if case let .document(updatedDoc) = vm.activeTab?.payload {
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
        if case let .sheet(updatedSheet) = vm.activeTab?.payload {
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
        if case let .pdf(updatedPDF) = vm.activeTab?.payload {
            XCTAssertEqual(updatedPDF.pages[0].rotationDegrees, 90)
            XCTAssertTrue(updatedPDF.annotations.contains(where: {
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

                    let didMutateState = (vm.activeTab?.payload != beforePayload)
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
        if case let .document(restoredDoc) = freshVM.activeTab?.payload {
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
        if case let .sheet(redoneSheet) = freshVM.activeTab?.payload {
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
        if case let .document(reopenedDoc) = openedDocTab.payload {
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

        // 5. Verify scrubActiveTabMetadata() and cross-format saveTabToDisk payload sync (F3, F4)
        freshVM.activeTabID = restoredFromTemplateTab.id
        freshVM.scrubActiveTabMetadata()
        XCTAssertGreaterThan(freshVM.lastScrubbedMetadataCount, 0)

        let tempPDFURL = FileManager.default.temporaryDirectory.appendingPathComponent("easyoffice_f4_sync_\(UUID().uuidString).pdf")
        defer { try? FileManager.default.removeItem(at: tempPDFURL) }
        try freshVM.saveTabToDisk(id: restoredFromTemplateTab.id, to: tempPDFURL, targetFormat: .pdf)
        let syncedTab = try XCTUnwrap(freshVM.openTabs.first(where: { $0.id == restoredFromTemplateTab.id }))
        XCTAssertEqual(syncedTab.format, .pdf)
        if case let .pdf(convertedPDF) = syncedTab.payload {
            XCTAssertGreaterThanOrEqual(convertedPDF.pages.count, 1)
        } else {
            XCTFail("saveTabToDisk with targetFormat .pdf must update tab.payload to .pdf")
        }
    }
}

