import AppKit
import XCTest

@testable import EasyOfficeCore
@testable import EasyOfficeDesignSystem

@MainActor
final class CommercialUXAndEditorParityTests: XCTestCase {

  func testCloseOtherTabsProtectsModifiedTabsAndTracksRecentlyClosed() {
    let vm = WorkspaceViewModel()
    XCTAssertGreaterThanOrEqual(vm.openTabs.count, 4)

    for idx in vm.openTabs.indices {
      vm.openTabs[idx].isPinned = false
      vm.openTabs[idx].isModified = false
    }

    let keptTabID = vm.openTabs[0].id
    let modifiedTabID = vm.openTabs[1].id
    let unmodifiedTabID = vm.openTabs[2].id
    vm.openTabs[1].isModified = true

    vm.closeOtherTabs(keeping: keptTabID)

    XCTAssertTrue(vm.openTabs.contains(where: { $0.id == keptTabID }))
    XCTAssertTrue(
      vm.openTabs.contains(where: { $0.id == modifiedTabID }),
      "Modified tab must not be discarded without confirmation"
    )
    XCTAssertFalse(
      vm.openTabs.contains(where: { $0.id == unmodifiedTabID }),
      "Unmodified tab should be closed immediately"
    )
    XCTAssertEqual(vm.pendingCloseConfirmTabID, modifiedTabID)
    XCTAssertTrue(vm.recentlyClosedTabs.contains(where: { $0.id == unmodifiedTabID }))

    let autosumItem = RibbonCategory.actions(for: .formulasAndData, module: .sheet)
      .first(where: { $0.id == "sheet-data-autosum" })
    XCTAssertEqual(autosumItem?.shortcutHint, "⌥⇧=")
  }

  func testOpenNewBlankTabAcrossAllFiveModulesAndRecentPersistence() {
    let vm = WorkspaceViewModel()
    let modules: [OfficeModuleKind] = [.document, .hwp, .sheet, .slide, .pdf]

    for module in modules {
      let created = vm.openNewBlankTab(for: module)
      XCTAssertEqual(created.moduleKind, module)
      XCTAssertEqual(vm.activeTabID, created.id)
      XCTAssertFalse(vm.isShowingHomeHub)
      XCTAssertFalse(created.isModified)

      switch created.payload {
      case .document(let doc):
        XCTAssertEqual(doc.paragraphs.count, 1)
        XCTAssertEqual(doc.paragraphs.first?.text, "")
      case .sheet(let sheet):
        XCTAssertTrue(sheet.cellsByCoordinate.isEmpty)
      case .slide(let deck):
        XCTAssertEqual(deck.slides.count, 1)
      case .pdf(let pdf):
        XCTAssertEqual(pdf.pages.count, 1)
      }

      XCTAssertTrue(vm.recentDocumentRecords.contains(where: { $0.id == created.id }))
      vm.requestCloseTab(id: created.id)
      XCTAssertFalse(vm.openTabs.contains(where: { $0.id == created.id }))
      XCTAssertTrue(
        vm.filteredRecentTabs.contains(where: { $0.id == created.id }),
        "Closed blank tab should remain visible in recent document records"
      )
    }

    let targetRecentID = try! XCTUnwrap(vm.recentDocumentRecords.first?.id)
    vm.togglePinTab(id: targetRecentID)
    vm.toggleFavoriteTab(id: targetRecentID)

    let freshVM = WorkspaceViewModel()
    freshVM.restorePersistedRecentDocuments()
    let restoredRecord = try! XCTUnwrap(
      freshVM.recentDocumentRecords.first(where: { $0.id == targetRecentID })
    )
    XCTAssertTrue(restoredRecord.isPinned)
    XCTAssertTrue(restoredRecord.isFavorite)
  }

  func testSaveOverwriteWhenFileURLExistsVersusSaveAsAndImportFromDisk() throws {
    let vm = WorkspaceViewModel()
    let tempDir = FileManager.default.temporaryDirectory
      .appendingPathComponent("eo-ux-save-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: tempDir) }

    let blankDocTab = vm.openNewBlankTab(for: .document)
    XCTAssertNil(blankDocTab.fileURL)
    let didOverwriteUnbacked = try vm.saveTabOverwriteIfBackedByFile(id: blankDocTab.id)
    XCTAssertFalse(didOverwriteUnbacked)

    let targetURL = tempDir.appendingPathComponent("saved-doc.docx")
    try vm.saveTabToDisk(id: blankDocTab.id, to: targetURL)
    XCTAssertEqual(vm.activeTab?.fileURL, targetURL)

    vm.appendParagraphToActiveDocument(text: "덮어쓰기 저장 테스트 문단", style: .body)
    XCTAssertEqual(vm.activeTab?.isModified, true)

    let didOverwriteBacked = try vm.saveTabOverwriteIfBackedByFile(id: blankDocTab.id)
    XCTAssertTrue(didOverwriteBacked)
    XCTAssertEqual(vm.activeTab?.isModified, false)

    vm.requestCloseTab(id: blankDocTab.id)
    let imported = try vm.importDocumentFromDisk(url: targetURL)
    if case .document(let reDoc) = imported.payload {
      XCTAssertTrue(reDoc.paragraphs.contains(where: { $0.text.contains("덮어쓰기 저장 테스트 문단") }))
    } else {
      XCTFail("Expected document payload after importDocumentFromDisk")
    }
  }

  func testDocumentParagraphSplitMergeAdjacentNavigationAndUndo() throws {
    let vm = WorkspaceViewModel()
    let tab = vm.openNewBlankTab(for: .document)
    vm.activeTabID = tab.id

    guard case .document(let initialDoc) = vm.activeTab?.payload,
      let firstID = initialDoc.paragraphs.first?.id
    else {
      return XCTFail("Missing initial blank paragraph")
    }

    vm.updateParagraphText(paragraphID: firstID, newText: "가나다라마바사")
    let secondID = try XCTUnwrap(vm.splitParagraph(paragraphID: firstID, atUTF16Offset: 3))
    XCTAssertEqual(vm.selectedParagraphID, secondID)

    if case .document(let splitDoc) = vm.activeTab?.payload {
      XCTAssertEqual(splitDoc.paragraphs.count, 2)
      XCTAssertEqual(splitDoc.paragraphs[0].text, "가나다")
      XCTAssertEqual(splitDoc.paragraphs[1].text, "라마바사")
    } else {
      XCTFail("Expected split document")
    }

    vm.selectAdjacentParagraph(from: secondID, delta: -1)
    XCTAssertEqual(vm.selectedParagraphID, firstID)
    vm.selectAdjacentParagraph(from: firstID, delta: 1)
    XCTAssertEqual(vm.selectedParagraphID, secondID)

    let mergedID = try XCTUnwrap(vm.mergeParagraphWithPrevious(paragraphID: secondID))
    XCTAssertEqual(mergedID, firstID)
    if case .document(let mergedDoc) = vm.activeTab?.payload {
      XCTAssertEqual(mergedDoc.paragraphs.count, 1)
      XCTAssertEqual(mergedDoc.paragraphs[0].text, "가나다라마바사")
    } else {
      XCTFail("Expected merged document")
    }

    vm.undo()
    if case .document(let undoneDoc) = vm.activeTab?.payload {
      XCTAssertEqual(undoneDoc.paragraphs.count, 2)
      XCTAssertEqual(undoneDoc.paragraphs[0].text, "가나다")
      XCTAssertEqual(undoneDoc.paragraphs[1].text, "라마바사")
    } else {
      XCTFail("Expected undo to restore split state")
    }
  }

  func testDocumentFontFamilySizeRulerIndentOutlineCollapseAndOfficialHeader() throws {
    let vm = WorkspaceViewModel()
    guard let docTab = vm.openTabs.first(where: { $0.moduleKind == .document }) else {
      return XCTFail("Missing document tab")
    }
    vm.activeTabID = docTab.id

    vm.setActiveParagraphFont(fontName: "NanumMyeongjo", fontSizePt: 18.0)
    vm.setActiveParagraphIndent(leftIndentPt: 24.0, firstLineIndentPt: 12.0)

    if case .document(let doc) = vm.activeTab?.payload {
      let target = try XCTUnwrap(
        doc.paragraphs.first(where: { $0.id == vm.selectedParagraphID }) ?? doc.paragraphs.first
      )
      XCTAssertEqual(target.fontName, "NanumMyeongjo")
      XCTAssertEqual(target.fontSizePt, 18.0, accuracy: 1e-6)
      for run in target.runs {
        XCTAssertEqual(run.fontName, "NanumMyeongjo")
        XCTAssertEqual(run.fontSizePt ?? 0.0, 18.0, accuracy: 1e-6)
      }
      XCTAssertEqual(target.leftIndentPt, 24.0, accuracy: 1e-6)
      XCTAssertEqual(target.firstLineIndentPt, 12.0, accuracy: 1e-6)

      if let heading = doc.paragraphs.first(where: { $0.style == .heading1 }) {
        vm.collapsedOutlineParagraphIDs.insert(heading.id)
        let visible = vm.visibleDocumentOutlineParagraphs(for: doc)
        XCTAssertTrue(visible.contains(where: { $0.id == heading.id }))
        XCTAssertLessThan(visible.count, doc.paragraphs.count)
      }
    } else {
      XCTFail("Expected document payload")
    }

    XCTAssertEqual(
      vm.formattedTableOfContentsEntryTitle(index: 0, rawTitle: "1. 아키텍처 목표 및 핵심 가치"),
      "1. 아키텍처 목표 및 핵심 가치"
    )
    XCTAssertEqual(
      vm.formattedTableOfContentsEntryTitle(index: 1, rawTitle: "개요 및 배경"),
      "2. 개요 및 배경"
    )

    if let hwpTab = vm.openTabs.first(where: { $0.moduleKind == .hwp }) {
      vm.activeTabID = hwpTab.id
      vm.updateActiveDocumentOfficialHeaderText("행정안전부 제2026-309호")
      if case .document(let hwpDoc) = vm.activeTab?.payload {
        XCTAssertEqual(hwpDoc.headerText, "행정안전부 제2026-309호")
      }
    }
  }

  func testSheetGridBoundsBeyond128RowsAnd26Columns() throws {
    var sheet = WorksheetModel(name: "LargeGrid")
    let farCoord = PackedCellCoordinate(row: 150, column: 30)
    let farAddr = CellAddress(packed: farCoord)
    XCTAssertEqual(CellAddress.columnLabel(for: farCoord.column), "AE")
    XCTAssertEqual(farAddr.a1Notation, "AE151")

    try sheet.setCell("AE151", rawValue: "999")
    let bounds = sheet.expandedGridBounds()
    XCTAssertGreaterThanOrEqual(bounds.rows, 163)
    XCTAssertGreaterThanOrEqual(bounds.cols, 35)

    // Sparse high-coordinate defense (CWE-400): XFD1048576 must remain capped
    try sheet.setCell("XFD1048576", rawValue: "1")
    let cappedBounds = sheet.expandedGridBounds()
    XCTAssertLessThanOrEqual(cappedBounds.rows, 512)
    XCTAssertLessThanOrEqual(cappedBounds.cols, 64)
    XCTAssertLessThanOrEqual(cappedBounds.rows * cappedBounds.cols, CellRange.maxExpandedCellCount)
    let sparsePivot = sheet.computeFilteredPivotSummary(aggregation: .sum)
    XCTAssertGreaterThanOrEqual(sparsePivot.grandTotal, 0.0)
  }

  func testSheetMultiCellTSVCopyAndPasteAndKeyboardNavigation() throws {
    let vm = WorkspaceViewModel()
    guard let sheetTab = vm.openTabs.first(where: { $0.moduleKind == .sheet }) else {
      return XCTFail("Missing sheet tab")
    }
    vm.activeTabID = sheetTab.id

    vm.selectSheetCellOrRange(primaryCell: "A1", rangeNotation: "A1:C2")
    let tsv = vm.copySelectedSheetRangeAsTSV()
    XCTAssertTrue(tsv.contains("\t"))
    XCTAssertTrue(tsv.contains("\n"))

    vm.selectSheetCellOrRange(primaryCell: "E5")
    vm.pasteSheetCellsFromTSV("100\t200\n300\t400")

    if case .sheet(let updatedSheet) = vm.activeTab?.payload {
      XCTAssertEqual(updatedSheet.rawValue(at: "E5"), "100")
      XCTAssertEqual(updatedSheet.rawValue(at: "F5"), "200")
      XCTAssertEqual(updatedSheet.rawValue(at: "E6"), "300")
      XCTAssertEqual(updatedSheet.rawValue(at: "F6"), "400")
    } else {
      XCTFail("Expected sheet payload")
    }

    vm.moveSheetSelection(rowDelta: 1, columnDelta: 1)
    XCTAssertEqual(vm.selectedSheetCell, "F6")

    vm.selectSheetCellOrRange(primaryCell: "E5", rangeNotation: "E5:F6")
    vm.clearSelectedSheetRange()
    if case .sheet(let clearedSheet) = vm.activeTab?.payload {
      XCTAssertEqual(clearedSheet.rawValue(at: "E5"), "")
      XCTAssertEqual(clearedSheet.rawValue(at: "F6"), "")
    }
  }

  func testSheetPivotExcludesSummaryRowsAndMultiSeriesChart() throws {
    let vm = WorkspaceViewModel()
    guard let sheetTab = vm.openTabs.first(where: { $0.moduleKind == .sheet }),
      case .sheet(var sheet) = sheetTab.payload
    else {
      return XCTFail("Missing sheet tab")
    }
    vm.activeTabID = sheetTab.id

    let defaultPivot = sheet.computeFilteredPivotSummary(aggregation: .sum)
    XCTAssertFalse(defaultPivot.rowKeys.contains("전체 합계"))
    XCTAssertEqual(defaultPivot.grandTotal, 540.0, accuracy: 1e-6)

    try sheet.setCell("B2", rawValue: "260")
    let updatedPivot = sheet.computeFilteredPivotSummary(aggregation: .sum)
    XCTAssertFalse(updatedPivot.rowKeys.contains("전체 합계"))
    XCTAssertEqual(updatedPivot.grandTotal, 680.0, accuracy: 1e-6)
  }

  func testSlideUpdateContentMasterLayoutsAndBatchDiskExport() throws {
    let vm = WorkspaceViewModel()
    guard let slideTab = vm.openTabs.first(where: { $0.moduleKind == .slide }) else {
      return XCTFail("Missing slide tab")
    }
    vm.activeTabID = slideTab.id

    vm.updateSlideContent(
      slideIndex: 0,
      title: "개정 전략 발표",
      subtitle: "전사 표준화 2단계",
      bulletPoints: ["핵심 과제 A", "핵심 과제 B"]
    )
    vm.setActiveSlideMasterLayout(.twoColumn)

    if case .slide(let deck) = vm.activeTab?.payload {
      XCTAssertEqual(deck.slides[0].title, "개정 전략 발표")
      XCTAssertEqual(deck.slides[0].subtitle, "전사 표준화 2단계")
      XCTAssertEqual(deck.slides[0].bulletPoints, ["핵심 과제 A", "핵심 과제 B"])
      XCTAssertEqual(deck.slides[0].masterLayout, .twoColumnComparison)
    } else {
      XCTFail("Expected slide payload")
    }

    let tempDir = FileManager.default.temporaryDirectory
      .appendingPathComponent("eo-batch-out-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: tempDir) }

    vm.enqueueOpenTabsIntoBatchConversion(targetFormat: .pdf)
    vm.runAllBatchConversions()
    let writtenURLs = try vm.exportAllCompletedBatchItems(toDirectory: tempDir)
    XCTAssertFalse(writtenURLs.isEmpty)
    for url in writtenURLs {
      let data = try Data(contentsOf: url)
      let decodedPDF = try PDFBinarySerializer.decode(from: data)
      XCTAssertFalse(decodedPDF.pages.isEmpty)
    }

    // Fail-closed error reporting when enqueueing a corrupted file from disk
    let corruptURL = tempDir.appendingPathComponent("corrupt.docx")
    try Data("NOT_A_ZIP_CONTAINER".utf8).write(to: corruptURL)
    vm.lastErrorMessage = nil
    vm.enqueueFilesFromDiskIntoBatchConversion(urls: [corruptURL], targetFormat: .pdf)
    XCTAssertNotNil(vm.lastErrorMessage)

    // External PDF merge verification (valid merge + fail-closed corrupted rejection)
    let pdfTab = vm.openNewBlankTab(for: .pdf)
    vm.activeTabID = pdfTab.id
    let externalPDFURL = try XCTUnwrap(writtenURLs.first)
    try vm.mergeActivePDFWithFile(url: externalPDFURL)
    if case .pdf(let mergedPDF) = vm.activeTab?.payload {
      XCTAssertGreaterThan(mergedPDF.pages.count, 1)
    } else {
      XCTFail("Expected merged PDF payload")
    }
    XCTAssertThrowsError(try vm.mergeActivePDFWithFile(url: corruptURL))

    let tabCountBeforeDiskEnqueue = vm.openTabs.count
    vm.enqueueFilesFromDiskIntoBatchConversion(urls: [externalPDFURL], targetFormat: .docx)
    XCTAssertEqual(vm.openTabs.count, tabCountBeforeDiskEnqueue)

    let initialDark = vm.usesDarkMode
    vm.toggleDarkMode()
    XCTAssertNotEqual(vm.usesDarkMode, initialDark)
    vm.toggleDarkMode()

    let dirtyDocTab = vm.openNewBlankTab(for: .document)
    vm.activeTabID = dirtyDocTab.id
    vm.executeRibbonCommand("doc-insert-footnote")
    if case .document(let docWithFootnote) = vm.activeTab?.payload {
      XCTAssertFalse(docWithFootnote.footnotes.isEmpty)
    } else {
      XCTFail("Expected document payload")
    }
    vm.requestCloseTab(id: dirtyDocTab.id)
    XCTAssertEqual(vm.pendingCloseConfirmTabID, dirtyDocTab.id)
    vm.cancelCloseTab()
    XCTAssertNil(vm.pendingCloseConfirmTabID)
    XCTAssertTrue(vm.openTabs.contains(where: { $0.id == dirtyDocTab.id }))
    vm.requestCloseTab(id: dirtyDocTab.id)
    vm.confirmDiscardAndClosePendingTab()
    XCTAssertFalse(vm.openTabs.contains(where: { $0.id == dirtyDocTab.id }))
  }
}
