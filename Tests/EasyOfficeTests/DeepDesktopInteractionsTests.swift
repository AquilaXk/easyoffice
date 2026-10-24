import AppKit
import XCTest

@testable import EasyOfficeCore
@testable import EasyOfficeDesignSystem

@MainActor
final class DeepDesktopInteractionsTests: XCTestCase {

  // MARK: - 1. Document UTF-16 Sub-Selection Run Splitting & Ribbon Bold Integration

  func testUTF16SubSelectionRunSplittingAndRibbonBoldFormatting() throws {
    let vm = WorkspaceViewModel()
    let docTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .document }))
    vm.activeTabID = docTab.id

    guard case .document(let initialDoc) = vm.activeTab?.payload else {
      return XCTFail("Expected document payload")
    }
    let targetParagraph = try XCTUnwrap(
      initialDoc.paragraphs.first(where: { !$0.isBold && $0.text.utf16.count >= 8 })
        ?? initialDoc.paragraphs.last)
    let fullUTF16Count = targetParagraph.text.utf16.count
    XCTAssertGreaterThanOrEqual(fullUTF16Count, 6)

    // Select UTF-16 sub-range 2..<6 within the paragraph
    let subRange = 2..<min(6, fullUTF16Count - 1)
    vm.updateSelectedParagraphSelection(paragraphID: targetParagraph.id, utf16Range: subRange)
    XCTAssertEqual(vm.selectedParagraphID, targetParagraph.id)
    XCTAssertEqual(vm.selectedParagraphUTF16Range, subRange)

    // Apply inline run formatting over the selected UTF-16 sub-range
    vm.inlineRunTargetPhrase = ""
    vm.applyInlineRunFormattingToActiveParagraph(
      isBold: false,
      isItalic: true,
      isUnderlined: true,
      foregroundHex: "#5C6BC0"
    )

    if case .document(let afterInlineDoc) = vm.activeTab?.payload {
      let p = try XCTUnwrap(afterInlineDoc.paragraphs.first(where: { $0.id == targetParagraph.id }))
      XCTAssertGreaterThanOrEqual(p.runs.count, 3)
      XCTAssertEqual(p.runs.map(\.text).joined(), targetParagraph.text)
      XCTAssertTrue(p.runs[1].isItalic)
      XCTAssertTrue(p.runs[1].isUnderlined)
      XCTAssertEqual(p.runs[1].foregroundHex, "#5C6BC0")
      XCTAssertFalse(p.runs[0].isItalic)
    } else {
      XCTFail("Expected updated document payload")
    }

    // Execute doc-home-bold while UTF-16 selection is active; must split/toggle bold on selected UTF-16 range
    vm.updateSelectedParagraphSelection(paragraphID: targetParagraph.id, utf16Range: subRange)
    vm.executeRibbonCommand("doc-home-bold")

    if case .document(let afterRibbonDoc) = vm.activeTab?.payload {
      let p = try XCTUnwrap(afterRibbonDoc.paragraphs.first(where: { $0.id == targetParagraph.id }))
      XCTAssertGreaterThanOrEqual(p.runs.count, 3)
      XCTAssertTrue(p.runs[1].isBold)
      XCTAssertFalse(p.runs[0].isBold)
    } else {
      XCTFail("Expected updated document payload after doc-home-bold")
    }

    // Calling doc-home-bold again on the same UTF-16 subRange must toggle bold off
    vm.updateSelectedParagraphSelection(paragraphID: targetParagraph.id, utf16Range: subRange)
    vm.executeRibbonCommand("doc-home-bold")
    if case .document(let afterSecondToggleDoc) = vm.activeTab?.payload {
      let p = try XCTUnwrap(
        afterSecondToggleDoc.paragraphs.first(where: { $0.id == targetParagraph.id }))
      XCTAssertFalse(p.runs[1].isBold)
    } else {
      XCTFail("Expected updated document payload after second doc-home-bold")
    }
  }

  // MARK: - 2. Document Equations, Image Wrap Mode Cycling, and Watermark

  func testDocumentEquationsImageWrapModeAndWatermark() throws {
    let vm = WorkspaceViewModel()
    let docTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .document }))
    vm.activeTabID = docTab.id

    // Equation insertion and update
    vm.equationDraftText = "\\sum_{i=1}^{n} x_i \\approx \\pi r^2"
    vm.insertEquationIntoActiveDocument(caption: "급수 근사 수식")

    var insertedEquationID: UUID?
    if case .document(let doc) = vm.activeTab?.payload {
      let eq = try XCTUnwrap(doc.equations.last)
      insertedEquationID = eq.id
      XCTAssertEqual(eq.displayTitle, "급수 근사 수식")
      XCTAssertTrue(eq.renderedNotation.contains("∑"))
      XCTAssertTrue(eq.renderedNotation.contains("π"))
      XCTAssertTrue(eq.renderedNotation.contains("²"))
    } else {
      XCTFail("Expected document payload")
    }

    let eqID = try XCTUnwrap(insertedEquationID)
    vm.updateDocumentEquation(
      id: eqID, newExpression: "\\int_{0}^{\\infty} \\alpha x_0 dx \\leq \\Delta")
    if case .document(let doc) = vm.activeTab?.payload {
      let updatedEq = try XCTUnwrap(doc.equations.first(where: { $0.id == eqID }))
      XCTAssertTrue(updatedEq.renderedNotation.contains("∫"))
      XCTAssertTrue(updatedEq.renderedNotation.contains("∞"))
      XCTAssertTrue(updatedEq.renderedNotation.contains("α"))
      XCTAssertTrue(updatedEq.renderedNotation.contains("≤"))
      XCTAssertTrue(updatedEq.renderedNotation.contains("Δ"))
    }

    // Image wrap mode cycling across all ImageWrapMode cases
    vm.insertSampleImageAttachment(caption: "텍스트 어울림 검증 이미지")
    var imageID: UUID?
    var initialWrap: ImageWrapMode?
    if case .document(let doc) = vm.activeTab?.payload {
      let img = try XCTUnwrap(doc.images.last)
      imageID = img.id
      initialWrap = img.wrapMode
    }
    let targetImgID = try XCTUnwrap(imageID)
    var seenModes: Set<ImageWrapMode> = [try XCTUnwrap(initialWrap)]
    for _ in 0..<ImageWrapMode.allCases.count {
      vm.cycleImageWrapMode(imageID: targetImgID)
      if case .document(let doc) = vm.activeTab?.payload,
        let img = doc.images.first(where: { $0.id == targetImgID })
      {
        seenModes.insert(img.wrapMode)
      }
    }
    XCTAssertEqual(seenModes.count, ImageWrapMode.allCases.count)

    // Watermark setting and clearing
    vm.setDocumentWatermark("대외비 · 무단 배포 금지")
    if case .document(let doc) = vm.activeTab?.payload {
      XCTAssertEqual(doc.watermarkText, "대외비 · 무단 배포 금지")
      XCTAssertTrue(doc.searchablePlainText().contains("대외비 · 무단 배포 금지"))
    }
    vm.setDocumentWatermark("")
    if case .document(let doc) = vm.activeTab?.payload {
      XCTAssertTrue((doc.watermarkText ?? "").isEmpty)
    }
  }

  // MARK: - 3. Sheet Multi-Cell Selection, Range-Wide Formatting, Auto-Fill & Custom Dimensions

  func testSheetRangeSelectionAutoFillAndColumnRowResize() throws {
    let vm = WorkspaceViewModel()
    let sheetTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .sheet }))
    vm.activeTabID = sheetTab.id

    // Select B2 and extend to C3
    vm.selectSheetCellOrRange(primaryCell: "B2")
    XCTAssertEqual(vm.selectedSheetCell, "B2")
    XCTAssertEqual(vm.selectedSheetRange, "B2")

    vm.extendSheetSelection(to: "C3")
    XCTAssertEqual(vm.selectedSheetRange, "B2:C3")

    // Apply sheet-home-bold across the entire selected range B2:C3
    vm.executeRibbonCommand("sheet-home-bold")
    if case .sheet(let sheetAfterBold) = vm.activeTab?.payload {
      for addr in ["B2", "C2", "B3", "C3"] {
        let coord = try CellAddress(a1Notation: addr).packedCoordinate
        XCTAssertTrue(
          sheetAfterBold.cellStyle(at: coord).isBold,
          "Expected \(addr) in selectedSheetRange B2:C3 to be bold"
        )
      }
    } else {
      XCTFail("Expected sheet payload")
    }

    // Seed numeric progression in B2:B3 (100, 250) and Auto-Fill to B2:B5 (expecting 400, 550 in B4, B5)
    vm.selectSheetCellOrRange(primaryCell: "B2")
    vm.formulaBarDraft = "100"
    vm.applyFormulaBarEdit()
    vm.selectSheetCellOrRange(primaryCell: "B3")
    vm.formulaBarDraft = "250"
    vm.applyFormulaBarEdit()

    vm.selectSheetCellOrRange(primaryCell: "B2", rangeNotation: "B2:B3")
    vm.performAutoFillOnActiveSheet(sourceRangeNotation: "B2:B3", targetRangeNotation: "B2:B5")
    XCTAssertEqual(vm.selectedSheetRange, "B2:B5")

    if case .sheet(let filledSheet) = vm.activeTab?.payload {
      XCTAssertEqual(filledSheet.evaluate(at: "B4"), .number(400))
      XCTAssertEqual(filledSheet.evaluate(at: "B5"), .number(550))
    } else {
      XCTFail("Expected sheet payload after Auto-Fill")
    }

    // Update custom column width and row height
    vm.updateActiveSheetColumnWidth(column: 1, width: 185.0)
    vm.updateActiveSheetRowHeight(row: 2, height: 54.0)
    if case .sheet(let resizedSheet) = vm.activeTab?.payload {
      XCTAssertEqual(resizedSheet.columnWidth(at: 1), 185.0, accuracy: 0.01)
      XCTAssertEqual(resizedSheet.rowHeight(at: 2), 54.0, accuracy: 0.01)
    }
  }

  // MARK: - 4. Slide 8-Direction Resize Handles, Rotation, Animation & Full-Screen Slideshow

  func testSlideResizeHandlesRotationAnimationAndFullScreenSlideshow() throws {
    let vm = WorkspaceViewModel()
    let slideTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .slide }))
    vm.activeTabID = slideTab.id

    guard case .slide(let initialDeck) = vm.activeTab?.payload,
      let firstElem = initialDeck.slides.first?.elements.first
    else {
      return XCTFail("Expected slide deck with at least one element")
    }
    let elementID = firstElem.id
    vm.selectSlideElement(id: elementID)

    // Exercise all 8 SlideResizeHandle cases deterministically
    for handle in SlideResizeHandle.allCases {
      vm.resizeSlideElementByHandle(
        elementID: elementID,
        handle: handle,
        deltaX: 6.0,
        deltaY: 4.0
      )
      if case .slide(let deck) = vm.activeTab?.payload {
        let updated = try XCTUnwrap(deck.slides[0].elements.first(where: { $0.id == elementID }))
        XCTAssertGreaterThanOrEqual(updated.width, 40.0)
        XCTAssertGreaterThanOrEqual(updated.height, 24.0)
        XCTAssertGreaterThanOrEqual(updated.x, 0.0)
        XCTAssertGreaterThanOrEqual(updated.y, 0.0)
      }
    }

    // Rotate element including negative angle normalization
    vm.rotateSlideElement(elementID: elementID, toDegrees: -45.0)
    if case .slide(let deck) = vm.activeTab?.payload {
      let rotated = try XCTUnwrap(deck.slides[0].elements.first(where: { $0.id == elementID }))
      XCTAssertEqual(rotated.rotationDegrees, 315.0, accuracy: 0.001)
    }

    // Cycle slide element animation across all cases
    var observedAnimations: Set<SlideElementAnimation> = []
    for _ in 0..<SlideElementAnimation.allCases.count {
      vm.cycleSlideElementAnimation(elementID: elementID)
      if case .slide(let deck) = vm.activeTab?.payload,
        let elem = deck.slides[0].elements.first(where: { $0.id == elementID })
      {
        observedAnimations.insert(elem.animation)
      }
    }
    XCTAssertEqual(observedAnimations.count, SlideElementAnimation.allCases.count)

    // Toggle Full-Screen Slideshow
    XCTAssertFalse(vm.isFullScreenSlideshowActive)
    vm.toggleFullScreenSlideshow()
    XCTAssertTrue(vm.isFullScreenSlideshowActive)
    XCTAssertTrue(vm.isPresenterTimerRunning)
    vm.toggleFullScreenSlideshow()
    XCTAssertFalse(vm.isFullScreenSlideshowActive)
  }

  // MARK: - 5. PDF Freehand Ink Strokes, Text Highlight & ISO 32000-1 Export Round-Trip

  func testPDFInkStrokesHighlightAndBinaryRoundTrip() throws {
    let vm = WorkspaceViewModel()
    let pdfTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .pdf }))
    vm.activeTabID = pdfTab.id
    vm.selectedPDFPageIndex = 0

    let points: [PDFInkPoint] = [
      PDFInkPoint(x: 80.0, y: 520.0),
      PDFInkPoint(x: 140.0, y: 535.0),
      PDFInkPoint(x: 210.0, y: 515.0),
      PDFInkPoint(x: 265.0, y: 540.0),
    ]
    vm.addInkStrokeToActivePDFPage(
      points: points,
      strokeHex: "#D9383A",
      lineWidth: 3.0,
      label: "검토자 전자서명 획"
    )
    vm.addHighlightToActivePDFPage(selectedText: "핵심 보안 아키텍처")

    if case .pdf(let pdfModel) = vm.activeTab?.payload {
      XCTAssertEqual(pdfModel.inkStrokes.count, 1)
      let stroke = try XCTUnwrap(pdfModel.inkStrokes.first)
      XCTAssertEqual(stroke.pageNumber, 1)
      XCTAssertEqual(stroke.points, points)
      XCTAssertEqual(stroke.strokeHex, "#D9383A")
      XCTAssertEqual(stroke.lineWidth, 3.0, accuracy: 0.01)
      XCTAssertTrue(
        pdfModel.annotations.contains(where: {
          $0.kind == .highlight && $0.content.contains("핵심 보안 아키텍처")
        }))
    } else {
      return XCTFail("Expected PDF payload")
    }

    // Export to PDF bytes and verify both vector path stream operators (m / l / S) and decoded inkStrokes parity
    let exportedBytes = try vm.exportTabData(id: pdfTab.id, targetFormat: .pdf)
    let rawPDFText = String(decoding: exportedBytes, as: UTF8.self)
    XCTAssertTrue(rawPDFText.contains("80.00 520.00 m"))
    XCTAssertTrue(rawPDFText.contains("140.00 535.00 l"))
    XCTAssertTrue(rawPDFText.contains(" S Q"))

    let decodedPDF = try PDFBinarySerializer.decode(from: exportedBytes)
    XCTAssertEqual(decodedPDF.inkStrokes.count, 1)
    XCTAssertEqual(decodedPDF.inkStrokes.first?.points, points)
    XCTAssertEqual(decodedPDF.inkStrokes.first?.label, "검토자 전자서명 획")
  }

  // MARK: - 6. Document Highlight, Strikethrough, Hyperlink, Indent, Table Row/Col, Revisions & Endnote

  func testDocumentHighlightStrikethroughHyperlinkIndentTableStructureRevisionsAndEndnote() throws {
    let vm = WorkspaceViewModel()
    let docTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .document }))
    vm.activeTabID = docTab.id

    guard case .document(let initialDoc) = vm.activeTab?.payload else {
      return XCTFail("Expected document payload")
    }
    let para = try XCTUnwrap(initialDoc.paragraphs.last)
    vm.updateSelectedParagraphSelection(paragraphID: para.id, utf16Range: 0..<4)

    vm.toggleHighlightOnActiveParagraph(colorHex: "#FFF59D")
    vm.toggleStrikethroughOnActiveParagraph()
    vm.insertHyperlinkIntoActiveParagraph(url: "https://example.org/spec")
    vm.adjustActiveParagraphIndent(firstLineDeltaPt: 8.0, leftIndentDeltaPt: 16.0)
    vm.insertEndnoteIntoActiveDocument(content: "참고 표준 미주 1")

    if case .document(let updatedDoc) = vm.activeTab?.payload {
      let updatedPara = try XCTUnwrap(updatedDoc.paragraphs.first(where: { $0.id == para.id }))
      XCTAssertEqual(updatedPara.firstLineIndentPt, 8.0, accuracy: 0.01)
      XCTAssertEqual(updatedPara.leftIndentPt, 16.0, accuracy: 0.01)
      XCTAssertTrue(updatedPara.runs.contains(where: { $0.backgroundHex == "#FFF59D" }))
      XCTAssertTrue(updatedPara.runs.contains(where: { $0.isStrikethrough }))
      XCTAssertTrue(
        updatedPara.runs.contains(where: { $0.linkURL == "https://example.org/spec" }))
      XCTAssertTrue(
        updatedDoc.footnotes.contains(where: { $0.isEndnote && $0.text.contains("참고 표준 미주 1") })
      )
    } else {
      XCTFail("Expected updated document payload")
    }

    // Verify second invocation of toggleHighlightOnActiveParagraph and toggleStrikethroughOnActiveParagraph genuinely toggles OFF
    vm.toggleHighlightOnActiveParagraph(colorHex: "#FFF59D")
    vm.toggleStrikethroughOnActiveParagraph()
    if case .document(let toggledOffDoc) = vm.activeTab?.payload {
      let toggledOffPara = try XCTUnwrap(
        toggledOffDoc.paragraphs.first(where: { $0.id == para.id }))
      XCTAssertFalse(toggledOffPara.runs.contains(where: { $0.backgroundHex == "#FFF59D" }))
      XCTAssertFalse(toggledOffPara.runs.contains(where: { $0.isStrikethrough }))
    }

    // Verify hyperlink scheme allowlist sanitization (HTTPS/HTTP/MAILTO only; reject javascript:, file:, data:)
    XCTAssertNil(WorkspaceViewModel.sanitizeSafeHyperlinkURL("javascript:alert(1)"))
    XCTAssertNil(WorkspaceViewModel.sanitizeSafeHyperlinkURL("file:///etc/passwd"))
    XCTAssertNil(WorkspaceViewModel.sanitizeSafeHyperlinkURL("data:text/html,<script>"))
    XCTAssertEqual(
      WorkspaceViewModel.sanitizeSafeHyperlinkURL("https://easyoffice.local/spec"),
      "https://easyoffice.local/spec"
    )
    XCTAssertEqual(
      WorkspaceViewModel.sanitizeSafeHyperlinkURL("mailto:qa@easyoffice.local"),
      "mailto:qa@easyoffice.local"
    )
    XCTAssertEqual(
      WorkspaceViewModel.sanitizeSafeHyperlinkURL("easyoffice.local/docs"),
      "https://easyoffice.local/docs"
    )
    vm.insertHyperlinkIntoActiveParagraph(url: "javascript:alert(1)")
    XCTAssertNotNil(vm.lastErrorMessage)

    // Table row/column insert and delete + span & border style lookup helpers
    let initialRowCount: Int
    let initialColCount: Int
    if case .document(let docBeforeTable) = vm.activeTab?.payload {
      initialRowCount = docBeforeTable.tables[0].rowCount
      initialColCount = docBeforeTable.tables[0].columnCount
    } else {
      return XCTFail("Expected document payload")
    }

    var spanTestTable = DocumentTableBlock(
      rows: [
        ["구분", "상세", "비고"],
        ["항목 A", "병합 데이터", "참조"],
        ["항목 B", "하위 데이터", "정상"],
      ],
      cellSpans: [
        TableCellSpan(
          rowIndex: 0, columnIndex: 0, rowSpan: 2, columnSpan: 2, borderStyle: .doubleLine)
      ],
      borderStyle: .solid
    )
    XCTAssertNotNil(spanTestTable.originSpan(atRow: 0, column: 0))
    XCTAssertFalse(spanTestTable.isCoveredByNonOriginSpan(atRow: 0, column: 0))
    XCTAssertTrue(spanTestTable.isCoveredByNonOriginSpan(atRow: 0, column: 1))
    XCTAssertTrue(spanTestTable.isCoveredByNonOriginSpan(atRow: 1, column: 0))
    XCTAssertTrue(spanTestTable.isCoveredByNonOriginSpan(atRow: 1, column: 1))
    XCTAssertFalse(spanTestTable.isCoveredByNonOriginSpan(atRow: 1, column: 2))
    XCTAssertEqual(spanTestTable.effectiveBorderStyle(atRow: 0, column: 0), .doubleLine)
    XCTAssertEqual(spanTestTable.effectiveBorderStyle(atRow: 1, column: 2), .solid)
    spanTestTable.insertRow(at: 0, defaultCellText: "신규")
    XCTAssertNotNil(spanTestTable.originSpan(atRow: 1, column: 0))

    vm.insertRowInDocumentTable(tableIndex: 0)
    vm.insertColumnInDocumentTable(tableIndex: 0)
    if case .document(let docAfterInsert) = vm.activeTab?.payload {
      XCTAssertEqual(docAfterInsert.tables[0].rowCount, initialRowCount + 1)
      XCTAssertEqual(docAfterInsert.tables[0].columnCount, initialColCount + 1)
    }

    vm.deleteRowInDocumentTable(tableIndex: 0)
    vm.deleteColumnInDocumentTable(tableIndex: 0)
    if case .document(let docAfterDelete) = vm.activeTab?.payload {
      XCTAssertEqual(docAfterDelete.tables[0].rowCount, initialRowCount)
      XCTAssertEqual(docAfterDelete.tables[0].columnCount, initialColCount)
    }

    // Tracked revision accept & reject
    if case .document(let docWithRev) = vm.activeTab?.payload,
      let firstRev = docWithRev.revisions.first
    {
      vm.acceptDocumentRevision(id: firstRev.id)
      if case .document(let afterAccept) = vm.activeTab?.payload {
        XCTAssertFalse(afterAccept.revisions.contains(where: { $0.id == firstRev.id }))
      }
    }

    // Record another revision and reject it
    if case .document(let docForReject) = vm.activeTab?.payload,
      let firstPara = docForReject.paragraphs.first
    {
      let originalText = firstPara.text
      vm.updateParagraphText(paragraphID: firstPara.id, newText: "임시 수정 제목")
      if case .document(let docWithPendingRev) = vm.activeTab?.payload,
        let latestRev = docWithPendingRev.revisions.last
      {
        vm.rejectDocumentRevision(id: latestRev.id)
        if case .document(let afterReject) = vm.activeTab?.payload {
          let restoredPara = try XCTUnwrap(
            afterReject.paragraphs.first(where: { $0.id == firstPara.id }))
          XCTAssertEqual(restoredPara.text, originalText)
        }
      }
    }
  }

  // MARK: - 7. Sheet Multi-Cell Merge, Row/Col Insert/Delete, Named Ranges & Goal Seek

  func testSheetRangeMergeRowColumnInsertDeleteNamedRangeAndGoalSeek() throws {
    let vm = WorkspaceViewModel()
    let sheetTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .sheet }))
    vm.activeTabID = sheetTab.id

    // Merge honoring selectedSheetRange = "B2:D3"
    vm.selectSheetCellOrRange(primaryCell: "B2", rangeNotation: "B2:D3")
    vm.mergeSelectedSheetRange()
    XCTAssertTrue(vm.mergedSheetRanges(for: sheetTab.id).contains("B2:D3"))
    vm.unmergeSelectedSheetRange()
    XCTAssertFalse(vm.mergedSheetRanges(for: sheetTab.id).contains("B2:D3"))

    // Row & column insert/delete at selection with intermediate shift assertions
    vm.selectSheetCellOrRange(primaryCell: "B2")
    let originalB2: String? = {
      if case .sheet(let s) = vm.activeTab?.payload { return s.rawValue(at: "B2") }
      return nil
    }()
    XCTAssertEqual(originalB2, "120")

    vm.insertSheetRowAtSelection()
    if case .sheet(let afterRowInsert) = vm.activeTab?.payload {
      XCTAssertEqual(afterRowInsert.rawValue(at: "A3"), "엔진 최적화")
      XCTAssertEqual(afterRowInsert.rawValue(at: "B3"), "120")
    }
    vm.deleteSheetRowAtSelection()
    if case .sheet(let afterRowDelete) = vm.activeTab?.payload {
      XCTAssertEqual(afterRowDelete.rawValue(at: "A2"), "엔진 최적화")
      XCTAssertEqual(afterRowDelete.rawValue(at: "B2"), "120")
    }

    vm.insertSheetColumnAtSelection()
    if case .sheet(let afterColInsert) = vm.activeTab?.payload {
      XCTAssertEqual(afterColInsert.rawValue(at: "B2"), "")
      XCTAssertEqual(afterColInsert.rawValue(at: "C2"), "120")
    }
    vm.deleteSheetColumnAtSelection()
    if case .sheet(let afterColDelete) = vm.activeTab?.payload {
      XCTAssertEqual(afterColDelete.rawValue(at: "B2"), "120")
    }

    // Named range definition & formula evaluation
    vm.defineSheetNamedRange(name: "핵심실적", targetNotation: "B2:C2")
    if case .sheet(let sheetWithNamed) = vm.activeTab?.payload {
      XCTAssertEqual(sheetWithNamed.resolveNamedRange("핵심실적"), "B2:C2")
    }

    // Goal Seek on D2 (=SUM(B2:C2)) targeting 500 by varying B2 (with C2 = 180 -> B2 should become 320)
    let solvedB2 = try XCTUnwrap(
      vm.runGoalSeekOnActiveSheet(
        targetCell: "D2",
        targetValue: 500.0,
        variableCell: "B2"
      )
    )
    XCTAssertEqual(solvedB2, 320.0, accuracy: 0.01)
    if case .sheet(let afterGoalSeek) = vm.activeTab?.payload {
      XCTAssertEqual(afterGoalSeek.evaluate(at: "D2").numericValue ?? 0.0, 500.0, accuracy: 0.01)
    }
  }

  // MARK: - 8. Slide Deck Add, Duplicate, Move, Delete & Slide Sorter Toggle

  func testSlideDeckAddDuplicateMoveDeleteAndSorterToggle() throws {
    let vm = WorkspaceViewModel()
    let slideTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .slide }))
    vm.activeTabID = slideTab.id

    guard case .slide(let initialDeck) = vm.activeTab?.payload else {
      return XCTFail("Expected slide deck")
    }
    let initialCount = initialDeck.slides.count

    vm.addSlideToActiveDeck(title: "3. 확장 로드맵")
    if case .slide(let deckAfterAdd) = vm.activeTab?.payload {
      XCTAssertEqual(deckAfterAdd.slides.count, initialCount + 1)
      XCTAssertEqual(deckAfterAdd.slides.last?.title, "3. 확장 로드맵")
    }

    vm.duplicateActiveSlide()
    if case .slide(let deckAfterDup) = vm.activeTab?.payload {
      XCTAssertEqual(deckAfterDup.slides.count, initialCount + 2)
    }

    let indexBeforeMove = vm.selectedSlideIndex
    if indexBeforeMove > 0 {
      vm.moveActiveSlide(offset: -1)
      XCTAssertEqual(vm.selectedSlideIndex, indexBeforeMove - 1)
    }

    vm.deleteActiveSlide()
    if case .slide(let deckAfterDelete) = vm.activeTab?.payload {
      XCTAssertEqual(deckAfterDelete.slides.count, initialCount + 1)
    }

    XCTAssertFalse(vm.isSlideSorterViewEnabled)
    vm.toggleSlideSorterView()
    XCTAssertTrue(vm.isSlideSorterViewEnabled)
    vm.toggleSlideSorterView()
    XCTAssertFalse(vm.isSlideSorterViewEnabled)
  }

  // MARK: - 9. PDF Studio Redaction, AcroForm Fields, Page Reorder & Open Tab Merge

  func testPDFStudioRedactionAcroFormFieldsPageReorderAndOpenTabMerge() throws {
    let vm = WorkspaceViewModel()
    let pdfTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .pdf }))
    vm.activeTabID = pdfTab.id
    vm.selectedPDFPageIndex = 0

    // Irreversible redaction
    let redactedPageCount = vm.redactTextOnActivePDF(phrase: "매직 바이트")
    XCTAssertGreaterThanOrEqual(redactedPageCount, 1)
    if case .pdf(let pdfAfterRedact) = vm.activeTab?.payload {
      XCTAssertTrue(pdfAfterRedact.isRedacted)
      XCTAssertFalse(pdfAfterRedact.pages[0].extractedText.contains("매직 바이트"))
      XCTAssertTrue(pdfAfterRedact.pages[0].extractedText.contains("████"))
    }

    // AcroForm field creation & update
    vm.addFormFieldToActivePDF(name: "검토담당자", value: "박서연", isCheckbox: false)
    vm.addFormFieldToActivePDF(name: "보안검증완료", value: "승인", isCheckbox: true)
    var textFieldID: UUID?
    var checkboxID: UUID?
    if case .pdf(let pdfWithFields) = vm.activeTab?.payload {
      XCTAssertEqual(pdfWithFields.formFields.count, 2)
      textFieldID = pdfWithFields.formFields.first(where: { !$0.isCheckbox })?.id
      checkboxID = pdfWithFields.formFields.first(where: { $0.isCheckbox })?.id
    }

    let tfID = try XCTUnwrap(textFieldID)
    let cbID = try XCTUnwrap(checkboxID)
    vm.setActivePDFFormFieldValue(id: tfID, value: "최현우 수석")
    vm.setActivePDFFormFieldValue(id: cbID, isChecked: true)

    if case .pdf(let pdfUpdatedFields) = vm.activeTab?.payload {
      let tf = try XCTUnwrap(pdfUpdatedFields.formFields.first(where: { $0.id == tfID }))
      let cb = try XCTUnwrap(pdfUpdatedFields.formFields.first(where: { $0.id == cbID }))
      XCTAssertEqual(tf.value, "최현우 수석")
      XCTAssertTrue(cb.isChecked)
    }

    // Page reorder with page content swap verification
    let page0BeforeText: String
    let page1BeforeText: String
    if case .pdf(let pdfBeforeMove) = vm.activeTab?.payload {
      page0BeforeText = pdfBeforeMove.pages[0].extractedText
      page1BeforeText = pdfBeforeMove.pages[1].extractedText
    } else {
      return XCTFail("Expected PDF payload before move")
    }

    vm.selectedPDFPageIndex = 0
    vm.reorderActivePDFPage(offset: 1)
    XCTAssertEqual(vm.selectedPDFPageIndex, 1)
    if case .pdf(let pdfAfterMove) = vm.activeTab?.payload {
      XCTAssertEqual(pdfAfterMove.pages[0].extractedText, page1BeforeText)
      XCTAssertEqual(pdfAfterMove.pages[1].extractedText, page0BeforeText)
      XCTAssertEqual(pdfAfterMove.pages[0].pageNumber, 1)
      XCTAssertEqual(pdfAfterMove.pages[1].pageNumber, 2)
    }

    // Open a second PDF tab and merge into the active PDF tab
    vm.openNewSampleTab(for: .pdf)
    vm.activeTabID = pdfTab.id
    let countBeforeMerge: Int = {
      if case .pdf(let doc) = vm.activeTab?.payload { return doc.pages.count }
      return 0
    }()
    vm.mergeActivePDFWithTab()
    if case .pdf(let mergedDoc) = vm.activeTab?.payload {
      XCTAssertGreaterThan(mergedDoc.pages.count, countBeforeMerge)
    } else {
      XCTFail("Expected merged PDF payload")
    }
  }

  // MARK: - 10. Document Paragraph Alignment/Spacing/List, Table Merge/Split/Border, Mail Merge, Manuscript Grid & Vector Insertions

  func testDocumentParagraphFormattingTableMergeMailMergeManuscriptGridAndVectorInsertions() throws
  {
    let vm = WorkspaceViewModel()
    let docTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .document }))
    vm.activeTabID = docTab.id

    guard case .document(let initialDoc) = vm.activeTab?.payload,
      let firstPara = initialDoc.paragraphs.first
    else {
      return XCTFail("Expected document payload with paragraphs")
    }
    vm.updateSelectedParagraphSelection(paragraphID: firstPara.id, utf16Range: nil)

    // Paragraph alignment
    for align in [
      TextAlignmentMode.leading, .center, .trailing, .justified,
    ] {
      vm.setActiveParagraphAlignment(align)
      if case .document(let doc) = vm.activeTab?.payload {
        let p = try XCTUnwrap(doc.paragraphs.first(where: { $0.id == firstPara.id }))
        XCTAssertEqual(p.alignment, align)
      }
    }

    // Line spacing with clamping (80%...400%)
    vm.setActiveParagraphLineSpacing(200.0)
    if case .document(let doc) = vm.activeTab?.payload {
      let p = try XCTUnwrap(doc.paragraphs.first(where: { $0.id == firstPara.id }))
      XCTAssertEqual(p.lineSpacingPercent, 200.0, accuracy: 0.01)
    }
    vm.setActiveParagraphLineSpacing(40.0)
    if case .document(let doc) = vm.activeTab?.payload {
      let p = try XCTUnwrap(doc.paragraphs.first(where: { $0.id == firstPara.id }))
      XCTAssertEqual(p.lineSpacingPercent, 80.0, accuracy: 0.01)
    }

    // Multi-level list kind & level delta clamping (0...3)
    vm.setActiveParagraphListKind(.koreanOfficial, levelDelta: 0)
    vm.setActiveParagraphListKind(.koreanOfficial, levelDelta: 1)
    if case .document(let doc) = vm.activeTab?.payload {
      let p = try XCTUnwrap(doc.paragraphs.first(where: { $0.id == firstPara.id }))
      XCTAssertEqual(p.listKind, .koreanOfficial)
      XCTAssertEqual(p.listLevel, 1)
    }
    vm.setActiveParagraphListKind(.none, levelDelta: -5)
    if case .document(let doc) = vm.activeTab?.payload {
      let p = try XCTUnwrap(doc.paragraphs.first(where: { $0.id == firstPara.id }))
      XCTAssertEqual(p.listKind, .none)
      XCTAssertEqual(p.listLevel, 0)
    }

    // Table cell merge, split, and border style
    vm.mergeActiveDocumentTableCells(tableIndex: 0, row: 0, column: 0, rowSpan: 1, columnSpan: 2)
    if case .document(let doc) = vm.activeTab?.payload {
      XCTAssertNotNil(doc.tables[0].originSpan(atRow: 0, column: 0))
      XCTAssertTrue(doc.tables[0].isCoveredByNonOriginSpan(atRow: 0, column: 1))
    }
    vm.splitActiveDocumentTableCell(tableIndex: 0, row: 0, column: 0)
    if case .document(let doc) = vm.activeTab?.payload {
      XCTAssertNil(doc.tables[0].originSpan(atRow: 0, column: 0))
    }
    vm.setActiveDocumentTableBorderStyle(.doubleLine, tableIndex: 0)
    if case .document(let doc) = vm.activeTab?.payload {
      XCTAssertEqual(doc.tables[0].borderStyle, .doubleLine)
    }

    // Mail merge replacement
    vm.appendParagraphToActiveDocument(text: "수신 부서: {{수신처}}")
    vm.applyMailMergeFieldReplacement(key: "수신처", value: "아키텍처위원회")
    if case .document(let doc) = vm.activeTab?.payload {
      XCTAssertTrue(doc.searchablePlainText().contains("아키텍처위원회"))
      XCTAssertFalse(doc.searchablePlainText().contains("{{수신처}}"))
    }

    // 200-char Manuscript Grid toggle & hwp-mod-manuscript ribbon command
    XCTAssertFalse(vm.isManuscriptGridModeEnabled)
    vm.toggleManuscriptGridMode()
    XCTAssertTrue(vm.isManuscriptGridModeEnabled)
    vm.toggleManuscriptGridMode()
    XCTAssertFalse(vm.isManuscriptGridModeEnabled)
    vm.executeRibbonCommand("hwp-mod-manuscript")
    XCTAssertTrue(vm.isManuscriptGridModeEnabled)

    // doc-insert-shape and doc-insert-chart attach vector diagram images
    let imgCountBefore: Int = {
      if case .document(let doc) = vm.activeTab?.payload { return doc.images.count }
      return 0
    }()
    vm.executeRibbonCommand("doc-insert-shape")
    vm.executeRibbonCommand("doc-insert-chart")
    if case .document(let doc) = vm.activeTab?.payload {
      XCTAssertEqual(doc.images.count, imgCountBefore + 2)
      XCTAssertTrue(doc.images.contains(where: { $0.caption == "벡터 다이어그램 도형" }))
      XCTAssertTrue(doc.images.contains(where: { $0.caption == "실적 요약 차트 그래픽" }))
      for img in doc.images.suffix(2) {
        XCTAssertTrue(img.imageData.starts(with: [0x89, 0x50, 0x4E, 0x47]))
        XCTAssertEqual(img.wrapMode, .inline)
      }
    }
  }

  // MARK: - 11. Sheet Duplicate Removal, Delimiter Split, Frozen Column, Data Bars & Pivot Aggregation

  func testSheetDuplicateRemovalDelimiterSplitFrozenColumnDataBarsAndPivotAggregation() throws {
    let vm = WorkspaceViewModel()
    let sheetTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .sheet }))
    vm.activeTabID = sheetTab.id

    // Seed duplicate row and delimited text
    vm.selectSheetCellOrRange(primaryCell: "A5")
    vm.formulaBarDraft = "중복항목"
    vm.commitSheetFormulaBarEdit()
    vm.selectSheetCellOrRange(primaryCell: "B5")
    vm.formulaBarDraft = "999"
    vm.commitSheetFormulaBarEdit()

    vm.selectSheetCellOrRange(primaryCell: "A6")
    vm.formulaBarDraft = "중복항목"
    vm.commitSheetFormulaBarEdit()
    vm.selectSheetCellOrRange(primaryCell: "B6")
    vm.formulaBarDraft = "999"
    vm.commitSheetFormulaBarEdit()

    vm.selectSheetCellOrRange(primaryCell: "A5", rangeNotation: "A5:B6")
    let removedCount = vm.removeDuplicatesInActiveSheet()
    XCTAssertEqual(removedCount, 1)

    // Delimiter split on column A
    vm.selectSheetCellOrRange(primaryCell: "A5")
    vm.formulaBarDraft = "서울,부산,대전"
    vm.commitSheetFormulaBarEdit()
    vm.sheetDelimiterDraft = ","
    let splitRows = vm.splitActiveSheetColumnByDelimiter()
    XCTAssertGreaterThanOrEqual(splitRows, 1)
    if case .sheet(let sheet) = vm.activeTab?.payload {
      XCTAssertEqual(sheet.rawValue(at: "A5"), "서울")
      XCTAssertEqual(sheet.rawValue(at: "B5"), "부산")
      XCTAssertEqual(sheet.rawValue(at: "C5"), "대전")
    }

    // Frozen first column, Data Bars toggle, Pivot aggregation & sheet-insert-shape chart visibility
    XCTAssertFalse(vm.isFirstColumnFrozen)
    vm.toggleFirstColumnFrozen()
    XCTAssertTrue(vm.isFirstColumnFrozen)

    XCTAssertTrue(vm.isDataBarsEnabled)
    vm.toggleSheetDataBars()
    XCTAssertFalse(vm.isDataBarsEnabled)

    vm.selectedPivotAggregation = .average
    XCTAssertEqual(vm.selectedPivotAggregation, .average)
    vm.executeRibbonCommand("sheet-mod-pivot")
    XCTAssertGreaterThan(vm.lastPivotGrandTotal, 0)

    vm.isChartPanelVisible = false
    vm.executeRibbonCommand("sheet-insert-shape")
    XCTAssertTrue(vm.isChartPanelVisible)
  }

  // MARK: - 12. Slide Master Theme, Element Alignment, Duplicate/Delete, Speaker Notes & Z-Order Normalization

  func testSlideMasterThemeElementAlignmentDuplicateDeleteSpeakerNotesAndZOrderNormalization()
    throws
  {
    let vm = WorkspaceViewModel()
    let slideTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .slide }))
    vm.activeTabID = slideTab.id

    // Apply deck master theme
    vm.applyThemeToActiveDeck(.corporateNavy)
    if case .slide(let deck) = vm.activeTab?.payload {
      XCTAssertEqual(deck.deckTheme, .corporateNavy)
      for elem in deck.slides[0].elements {
        XCTAssertEqual(elem.fillHex, SlideDeckTheme.corporateNavy.primaryFillHex)
        XCTAssertEqual(elem.strokeHex, SlideDeckTheme.corporateNavy.accentBorderHex)
      }
    }

    // Duplicate element & align elements
    let countBeforeDup: Int = {
      if case .slide(let deck) = vm.activeTab?.payload { return deck.slides[0].elements.count }
      return 0
    }()
    vm.duplicateActiveSlideElement()
    vm.duplicateActiveSlideElement()
    if case .slide(let deck) = vm.activeTab?.payload {
      XCTAssertEqual(deck.slides[0].elements.count, countBeforeDup + 2)
    }

    vm.alignSlideElements(.alignLeft)
    if case .slide(let deck) = vm.activeTab?.payload {
      let aligned = try XCTUnwrap(
        deck.slides[0].elements.first(where: { $0.id == vm.selectedSlideElementID }))
      XCTAssertEqual(aligned.x, 40.0, accuracy: 0.01)
    }

    vm.alignSlideElements(.alignCenter)
    if case .slide(let deck) = vm.activeTab?.payload {
      let aligned = try XCTUnwrap(
        deck.slides[0].elements.first(where: { $0.id == vm.selectedSlideElementID }))
      let expectedCenter = (deck.aspectRatio.widthPoints - aligned.width) / 2.0
      XCTAssertEqual(aligned.x, expectedCenter, accuracy: 0.01)
    }

    vm.alignSlideElements(.alignRight)
    if case .slide(let deck) = vm.activeTab?.payload {
      let aligned = try XCTUnwrap(
        deck.slides[0].elements.first(where: { $0.id == vm.selectedSlideElementID }))
      let expectedRight = deck.aspectRatio.widthPoints - aligned.width - 40.0
      XCTAssertEqual(aligned.x, expectedRight, accuracy: 0.01)
    }

    vm.alignSlideElements(.distributeHorizontally)
    if case .slide(let deck) = vm.activeTab?.payload {
      let sorted = deck.slides[0].elements.sorted { $0.x < $1.x }
      XCTAssertGreaterThanOrEqual(sorted.count, 3)
      let gap1 = sorted[1].x - sorted[0].maxX
      let gap2 = sorted[2].x - sorted[1].maxX
      XCTAssertEqual(gap1, gap2, accuracy: 0.01)
    }

    // Move Z-Order and verify strictly monotonic zIndex (0, 1, 2, ...)
    if case .slide(let deckBeforeZ) = vm.activeTab?.payload,
      let firstElemID = deckBeforeZ.slides[0].elements.first?.id
    {
      vm.moveSlideElementZOrder(slideIndex: 0, elementID: firstElemID, bringForward: true)
      if case .slide(let deckAfterZ) = vm.activeTab?.payload {
        for (idx, elem) in deckAfterZ.slides[0].elements.enumerated() {
          XCTAssertEqual(elem.zIndex, idx, "Expected monotonic zIndex at index \(idx)")
        }
      }
    }

    // Delete active slide element
    let countBeforeDel: Int = {
      if case .slide(let deck) = vm.activeTab?.payload { return deck.slides[0].elements.count }
      return 0
    }()
    vm.deleteActiveSlideElement()
    if case .slide(let deck) = vm.activeTab?.payload {
      XCTAssertEqual(deck.slides[0].elements.count, countBeforeDel - 1)
    }

    // Update active slide speaker notes
    vm.updateActiveSlideSpeakerNotes("핵심 발표자 스크립트 메모")
    if case .slide(let deck) = vm.activeTab?.payload {
      XCTAssertEqual(deck.slides[0].speakerNotes, "핵심 발표자 스크립트 메모")
    }
  }

  // MARK: - 13. PDF Approval Stamp Presets & Bookmark Outline Lifecycle

  func testPDFApprovalStampPresetsAndBookmarkOutlineLifecycle() throws {
    let vm = WorkspaceViewModel()
    let pdfTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .pdf }))
    vm.activeTabID = pdfTab.id
    vm.selectedPDFPageIndex = 0

    for preset in [
      "승인 (APPROVED)", "대외비 (CONFIDENTIAL)", "검토필 (REVIEWED)", "긴급 (URGENT)",
    ] {
      vm.addStampToActivePDFPage(preset: preset)
    }

    if case .pdf(let pdf) = vm.activeTab?.payload {
      let stamps = pdf.annotations.filter { $0.kind == .stamp }
      XCTAssertGreaterThanOrEqual(stamps.count, 4)
      XCTAssertTrue(stamps.contains(where: { $0.content.contains("[결재 스탬프] 승인 (APPROVED)") }))
      XCTAssertTrue(stamps.contains(where: { $0.content.contains("[결재 스탬프] 긴급 (URGENT)") }))
    }

    // Add & remove bookmark
    vm.bookmarkTitleDraft = "1장 아키텍처 개요"
    vm.addBookmarkToActivePDF()
    var addedBookmarkID: UUID?
    if case .pdf(let pdf) = vm.activeTab?.payload {
      let node = try XCTUnwrap(pdf.outline.last(where: { $0.title == "1장 아키텍처 개요" }))
      addedBookmarkID = node.id
      XCTAssertEqual(node.targetPageNumber, 1)
    }

    let bmID = try XCTUnwrap(addedBookmarkID)
    vm.removeBookmarkFromActivePDF(id: bmID)
    if case .pdf(let pdf) = vm.activeTab?.payload {
      XCTAssertFalse(pdf.outline.contains(where: { $0.id == bmID }))
    }
  }

  // MARK: - 14. 6th Audit Typography (자간/장평), Quick Sheet Number Formats & PDF Watermark / Security Summary

  func testSixthAuditTypographyAdjustmentsQuickNumberFormatsAndPDFWatermarkActions() throws {
    let vm = WorkspaceViewModel()

    // 1. Document/HWP 자간(letterSpacingPercent) & 장평(charWidthScalePercent) adjustments + clamping
    let docTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .document }))
    vm.activeTabID = docTab.id
    guard case .document(let initialDoc) = vm.activeTab?.payload,
      let firstPara = initialDoc.paragraphs.first
    else {
      return XCTFail("Expected document payload with paragraphs")
    }
    vm.updateSelectedParagraphSelection(paragraphID: firstPara.id, utf16Range: nil)

    vm.adjustActiveParagraphTypography(letterSpacingDelta: -5.0, charScaleDelta: 5.0)
    if case .document(let updatedDoc) = vm.activeTab?.payload {
      let para = try XCTUnwrap(updatedDoc.paragraphs.first(where: { $0.id == firstPara.id }))
      XCTAssertEqual(para.letterSpacingPercent, -5.0, accuracy: 0.01)
      XCTAssertEqual(para.charWidthScalePercent, 105.0, accuracy: 0.01)
    } else {
      XCTFail("Expected updated document payload")
    }

    // Clamp verification (-50...50 for letterSpacingPercent, 50...200 for charWidthScalePercent)
    vm.adjustActiveParagraphTypography(letterSpacingDelta: -200.0, charScaleDelta: 500.0)
    if case .document(let clampedDoc) = vm.activeTab?.payload {
      let para = try XCTUnwrap(clampedDoc.paragraphs.first(where: { $0.id == firstPara.id }))
      XCTAssertEqual(para.letterSpacingPercent, -50.0, accuracy: 0.01)
      XCTAssertEqual(para.charWidthScalePercent, 200.0, accuracy: 0.01)
    }

    vm.adjustActiveParagraphTypography(letterSpacingDelta: 200.0, charScaleDelta: -500.0)
    if case .document(let oppositeClampedDoc) = vm.activeTab?.payload {
      let para = try XCTUnwrap(
        oppositeClampedDoc.paragraphs.first(where: { $0.id == firstPara.id }))
      XCTAssertEqual(para.letterSpacingPercent, 50.0, accuracy: 0.01)
      XCTAssertEqual(para.charWidthScalePercent, 50.0, accuracy: 0.01)
    }

    vm.adjustActiveParagraphTypography(reset: true)
    if case .document(let resetDoc) = vm.activeTab?.payload {
      let para = try XCTUnwrap(resetDoc.paragraphs.first(where: { $0.id == firstPara.id }))
      XCTAssertEqual(para.letterSpacingPercent, 0.0, accuracy: 0.01)
      XCTAssertEqual(para.charWidthScalePercent, 100.0, accuracy: 0.01)
    }

    // 2. Quick Sheet Number Format, Decimal Places & Wrap Text actions (including multi-cell range) + 122 supported functions
    XCTAssertEqual(WorksheetModel.supportedFunctionNames.count, 122)
    let sheetTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .sheet }))
    vm.activeTabID = sheetTab.id
    vm.selectSheetCellOrRange(primaryCell: "B2", rangeNotation: "B2:C3")

    vm.applyQuickNumberFormatOnSelectedSheetCell(.currencyKRW)
    vm.adjustSelectedSheetCellDecimalPlaces(delta: 2)
    vm.toggleWrapTextOnSelectedSheetCell()

    if case .sheet(let updatedSheet) = vm.activeTab?.payload {
      for addr in ["B2", "C3"] {
        let style = updatedSheet.cellStyle(at: addr)
        XCTAssertEqual(style.numberFormat, .currencyKRW)
        XCTAssertEqual(style.decimalPlaces, 4)
        XCTAssertTrue(style.wrapText)
      }
    } else {
      XCTFail("Expected updated sheet payload")
    }

    vm.adjustSelectedSheetCellDecimalPlaces(delta: -1)
    if case .sheet(let updatedSheetAfterDec) = vm.activeTab?.payload {
      XCTAssertEqual(updatedSheetAfterDec.cellStyle(at: "B2").decimalPlaces, 3)
      XCTAssertEqual(updatedSheetAfterDec.cellStyle(at: "C3").decimalPlaces, 3)
    }

    // 3. PDF Watermark & Security Inspection Summary + Watermark Clear
    let pdfTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .pdf }))
    vm.activeTabID = pdfTab.id
    vm.pdfWatermarkDraft = "대외비 (CONFIDENTIAL)"
    vm.applyWatermarkOnActivePDF()

    if case .pdf(let updatedPDF) = vm.activeTab?.payload {
      XCTAssertEqual(updatedPDF.watermarkText, "대외비 (CONFIDENTIAL)")
      XCTAssertTrue(updatedPDF.securityInspectionSummary.contains("대외비 (CONFIDENTIAL)"))
    } else {
      XCTFail("Expected updated PDF payload")
    }

    vm.applyWatermarkOnActivePDF(text: "   ")
    if case .pdf(let clearedPDF) = vm.activeTab?.payload {
      XCTAssertEqual(clearedPDF.watermarkText, "")
      XCTAssertTrue(clearedPDF.securityInspectionSummary.contains("워터마크 없음"))
    }
  }

  // MARK: - 15. 7th Audit Slide Timing/Gridlines, Interactive Find/Replace Navigation, Multi-Column Sort & Home Filters

  func testSeventhAuditSlideTimingGridlinesFindNavigationAndHomeFilters() throws {
    let vm = WorkspaceViewModel()

    // 1. Slide Transition Duration, Auto-Advance & Gridlines Toggle
    let slideTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .slide }))
    vm.activeTabID = slideTab.id
    vm.selectedSlideIndex = 0

    XCTAssertFalse(vm.isSlideGridlinesVisible)
    vm.toggleSlideGridlines()
    XCTAssertTrue(vm.isSlideGridlinesVisible)

    vm.setActiveSlideTransitionDuration(1.5)
    vm.setActiveSlideAutoAdvance(seconds: 10)
    if case .slide(let deck) = vm.activeTab?.payload {
      XCTAssertEqual(deck.slides[0].transitionDurationSeconds, 1.5, accuracy: 0.01)
      XCTAssertEqual(deck.slides[0].autoAdvanceSeconds, 10)
    } else {
      XCTFail("Expected slide payload")
    }

    vm.setActiveSlideAutoAdvance(seconds: nil)
    if case .slide(let deckAfterManual) = vm.activeTab?.payload {
      XCTAssertNil(deckAfterManual.slides[0].autoAdvanceSeconds)
    }

    // 2. Interactive Find Match Counter, Next/Previous Match & Single-Match Replace across Document, Sheet, Slide, and PDF
    let docTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .document }))
    vm.activeTabID = docTab.id
    vm.appendParagraphToActiveDocument(text: "EasyOffice 표준 문서 검토 및 EasyOffice 아키텍처 검증")
    vm.findQueryText = "EasyOffice"
    vm.replaceQueryText = "NextOffice"
    vm.isFindCaseSensitive = true
    vm.recomputeFindMatches()
    let initialDocMatches = vm.findMatchCount
    XCTAssertGreaterThanOrEqual(initialDocMatches, 2)
    XCTAssertEqual(vm.currentFindMatchIndex, 0)

    vm.findNextMatch()
    XCTAssertEqual(vm.currentFindMatchIndex, 1)
    vm.findNextMatch()
    XCTAssertEqual(vm.currentFindMatchIndex, 2)
    vm.findPreviousMatch()
    XCTAssertEqual(vm.currentFindMatchIndex, 1)

    let replacedSingle = vm.replaceCurrentMatch()
    XCTAssertEqual(replacedSingle, 1)
    XCTAssertEqual(vm.findMatchCount, initialDocMatches - 1)

    // Sheet find & single-match replace
    let sheetTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .sheet }))
    vm.activeTabID = sheetTab.id
    if let sIdx = vm.openTabs.firstIndex(where: { $0.id == sheetTab.id }),
      case .sheet(var sheetModel) = vm.openTabs[sIdx].payload
    {
      try sheetModel.setCell("B2", rawValue: "검증항목_ALPHA")
      vm.openTabs[sIdx].payload = .sheet(sheetModel)
    }
    vm.findQueryText = "ALPHA"
    vm.replaceQueryText = "BETA"
    vm.recomputeFindMatches()
    XCTAssertGreaterThanOrEqual(vm.findMatchCount, 1)
    XCTAssertEqual(vm.replaceCurrentMatch(), 1)

    // Slide find & single-match replace
    vm.activeTabID = slideTab.id
    vm.addSlideBulletPoint(slideIndex: 0, text: "프레젠테이션 핵심 키워드 GAMMA")
    vm.findQueryText = "GAMMA"
    vm.replaceQueryText = "DELTA"
    vm.recomputeFindMatches()
    XCTAssertEqual(vm.findMatchCount, 1)
    XCTAssertEqual(vm.replaceCurrentMatch(), 1)
    if case .slide(let updatedDeck) = vm.activeTab?.payload {
      XCTAssertTrue(updatedDeck.slides[0].bulletPoints.contains(where: { $0.contains("DELTA") }))
    }

    // PDF find match counting
    let pdfTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .pdf }))
    vm.activeTabID = pdfTab.id
    if case .pdf(let pdfDoc) = vm.activeTab?.payload,
      let firstToken = pdfDoc.pages.first?.extractedText.split(separator: " ").first
    {
      vm.findQueryText = String(firstToken)
      vm.recomputeFindMatches()
      XCTAssertGreaterThanOrEqual(vm.findMatchCount, 1)
    }

    // 3. Sheet Multi-Column Secondary Sort (`sortActiveSheetBySecondaryColumn`)
    vm.activeTabID = sheetTab.id
    if let sIdx = vm.openTabs.firstIndex(where: { $0.id == sheetTab.id }),
      case .sheet(var sheetModel) = vm.openTabs[sIdx].payload
    {
      try sheetModel.setCell("G1", rawValue: "그룹")
      try sheetModel.setCell("H1", rawValue: "점수")
      try sheetModel.setCell("G2", rawValue: "그룹B")
      try sheetModel.setCell("H2", rawValue: "10")
      try sheetModel.setCell("G3", rawValue: "그룹A")
      try sheetModel.setCell("H3", rawValue: "20")
      try sheetModel.setCell("G4", rawValue: "그룹A")
      try sheetModel.setCell("H4", rawValue: "90")
      vm.openTabs[sIdx].payload = .sheet(sheetModel)
    }
    vm.sortActiveSheetBySecondaryColumn(
      primaryColumn: 6,
      ascending: true,
      secondaryColumn: 7,
      secondaryAscending: false
    )
    if case .sheet(let sortedSheet) = vm.activeTab?.payload {
      XCTAssertEqual(sortedSheet.rawValue(at: "G2"), "그룹A")
      XCTAssertEqual(sortedSheet.rawValue(at: "H2"), "90")
      XCTAssertEqual(sortedSheet.rawValue(at: "G3"), "그룹A")
      XCTAssertEqual(sortedSheet.rawValue(at: "H3"), "20")
      XCTAssertEqual(sortedSheet.rawValue(at: "G4"), "그룹B")
      XCTAssertEqual(sortedSheet.rawValue(at: "H4"), "10")
      XCTAssertTrue(vm.lastExecutedCommandSummary?.contains("다중 기준 행 정렬 완료") == true)
    } else {
      XCTFail("Expected sorted sheet payload")
    }

    // 4. Advanced Toolbar Drawer & Home Hub Category/Format Filter States
    XCTAssertFalse(vm.isAdvancedToolbarExpanded)
    vm.isAdvancedToolbarExpanded.toggle()
    XCTAssertTrue(vm.isAdvancedToolbarExpanded)

    XCTAssertEqual(vm.selectedHomeTemplateCategoryFilter, "전체")
    vm.selectedHomeTemplateCategoryFilter = "슬라이드"
    XCTAssertEqual(vm.selectedHomeTemplateCategoryFilter, "슬라이드")

    XCTAssertEqual(vm.selectedHomeRecentFormatFilter, "전체")
    vm.selectedHomeRecentFormatFilter = "PDF"
    XCTAssertEqual(vm.selectedHomeRecentFormatFilter, "PDF")

    // 5. Paragraph Style Switching with Inline Runs & HWP Tab Support
    vm.activeTabID = vm.openTabs.first(where: { $0.moduleKind == .document })?.id
    vm.toggleHighlightOnActiveParagraph(colorHex: "#FFF59D")
    vm.setActiveParagraphStyle(.quote)
    if case .document(let quoteDoc) = vm.activeTab?.payload {
      let pIdx = quoteDoc.paragraphs.firstIndex(where: { $0.id == vm.selectedParagraphID }) ?? 0
      XCTAssertEqual(quoteDoc.paragraphs[pIdx].style, .quote)
      XCTAssertEqual(quoteDoc.paragraphs[pIdx].fontSizePt, 11.0, accuracy: 0.001)
      XCTAssertFalse(quoteDoc.paragraphs[pIdx].isBold)
      XCTAssertTrue(quoteDoc.paragraphs[pIdx].isItalic)
      XCTAssertTrue(
        quoteDoc.paragraphs[pIdx].effectiveRuns.allSatisfy {
          $0.isItalic && !$0.isBold && abs(($0.fontSizePt ?? 0.0) - 11.0) < 0.001
        }
      )
    } else {
      XCTFail("Expected document payload after .quote style")
    }

    vm.setActiveParagraphStyle(.title)
    if case .document(let titleDoc) = vm.activeTab?.payload {
      let pIdx = titleDoc.paragraphs.firstIndex(where: { $0.id == vm.selectedParagraphID }) ?? 0
      XCTAssertEqual(titleDoc.paragraphs[pIdx].style, .title)
      XCTAssertEqual(titleDoc.paragraphs[pIdx].fontSizePt, 20.0, accuracy: 0.001)
      XCTAssertTrue(titleDoc.paragraphs[pIdx].isBold)
      XCTAssertFalse(titleDoc.paragraphs[pIdx].isItalic)
      XCTAssertTrue(
        titleDoc.paragraphs[pIdx].effectiveRuns.allSatisfy {
          $0.isBold && !$0.isItalic && abs(($0.fontSizePt ?? 0.0) - 20.0) < 0.001
        }
      )
    } else {
      XCTFail("Expected document payload after .title style")
    }

    vm.activeTabID = vm.openTabs.first(where: { $0.moduleKind == .hwp })?.id
    vm.setActiveParagraphStyle(.heading1)
    if case .document(let hwpDoc) = vm.activeTab?.payload {
      let pIdx = hwpDoc.paragraphs.firstIndex(where: { $0.id == vm.selectedParagraphID }) ?? 0
      XCTAssertEqual(hwpDoc.paragraphs[pIdx].style, .heading1)
      XCTAssertEqual(hwpDoc.paragraphs[pIdx].fontSizePt, 14.0, accuracy: 0.001)
      XCTAssertTrue(hwpDoc.paragraphs[pIdx].isBold)
      XCTAssertFalse(hwpDoc.paragraphs[pIdx].isItalic)
    } else {
      XCTFail("Expected HWP payload after .heading1 style")
    }
  }

  func testEighthAuditFindReplaceSyncSheetRulesPersistencePPTThemeRoundTripAndSnapshotMode() throws {
    let vm = WorkspaceViewModel()

    // 1. Snapshot mode toggle & recent format filter categories
    XCTAssertFalse(vm.isCanvasSnapshotMode)
    vm.isCanvasSnapshotMode = true
    XCTAssertTrue(vm.isCanvasSnapshotMode)
    vm.isCanvasSnapshotMode = false

    vm.selectedHomeRecentFormatFilter = "전체"
    let allCount = vm.filteredHomeRecentTabs.count
    XCTAssertGreaterThanOrEqual(allCount, 5)

    vm.selectedHomeRecentFormatFilter = "DOCX"
    XCTAssertTrue(vm.filteredHomeRecentTabs.allSatisfy { $0.moduleKind == .document })

    vm.selectedHomeRecentFormatFilter = "HWP/HWPX"
    XCTAssertTrue(vm.filteredHomeRecentTabs.allSatisfy { $0.moduleKind == .hwp })

    vm.selectedHomeRecentFormatFilter = "XLSX"
    XCTAssertTrue(vm.filteredHomeRecentTabs.allSatisfy { $0.moduleKind == .sheet })

    vm.selectedHomeRecentFormatFilter = "PPTX"
    XCTAssertTrue(vm.filteredHomeRecentTabs.allSatisfy { $0.moduleKind == .slide })

    vm.selectedHomeRecentFormatFilter = "PDF"
    XCTAssertTrue(vm.filteredHomeRecentTabs.allSatisfy { $0.moduleKind == .pdf })
    vm.selectedHomeRecentFormatFilter = "전체"

    // 2. Find & Replace bidirectional synchronization and inlineRuns/speakerNotes replacement
    let docTabID = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .document })?.id)
    vm.activeTabID = docTabID

    vm.findQueryDraft = "클라우드"
    XCTAssertEqual(vm.findQueryText, "클라우드")
    vm.findQueryText = "아키텍처"
    XCTAssertEqual(vm.findQueryDraft, "아키텍처")

    vm.replaceQueryDraft = "플랫폼"
    XCTAssertEqual(vm.replaceQueryText, "플랫폼")
    vm.replaceQueryText = "엔진아키텍처"
    XCTAssertEqual(vm.replaceQueryDraft, "엔진아키텍처")

    vm.recomputeFindMatches()
    let matchesBefore = vm.findMatchCount
    XCTAssertGreaterThan(matchesBefore, 0)
    let replacedCount = vm.replaceAllFindMatches()
    XCTAssertGreaterThan(replacedCount, 0)

    vm.findQueryDraft = "엔진아키텍처"
    vm.replaceQueryDraft = "아키텍처"
    vm.performFindAndReplace()
    vm.findQueryDraft = "아키텍처"
    vm.recomputeFindMatches()
    XCTAssertGreaterThan(vm.findMatchCount, 0)

    // 3. Per-tab and per-worksheet persistence of conditionalFormatRules and dataValidationRule + BIFF8/XLSX/ODS round-trip
    let sheetTabID = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .sheet })?.id)
    vm.activeTabID = sheetTabID

    let cfRule = ConditionalFormatRule.greaterThan(1000.0)
    let dvRule = DataValidationRule(minimumValue: 0, maximumValue: 999999)
    vm.applyConditionalFormatRuleOnActiveSheet(cfRule)
    vm.applyDataValidationRuleOnActiveSheet(dvRule)
    XCTAssertEqual(vm.activeConditionalFormatRule, cfRule)
    XCTAssertEqual(vm.activeDataValidationRule, dvRule)

    // Switch to another tab and back: rules must persist on the sheet tab
    vm.activeTabID = docTabID
    XCTAssertNil(vm.activeConditionalFormatRule)
    XCTAssertNil(vm.activeDataValidationRule)

    vm.activeTabID = sheetTabID
    XCTAssertEqual(vm.activeConditionalFormatRule, cfRule)
    XCTAssertEqual(vm.activeDataValidationRule, dvRule)

    // Add a second worksheet tab and verify per-worksheet isolation
    vm.addWorksheetTab()
    vm.selectWorksheetTab(index: 1)
    XCTAssertNil(vm.activeConditionalFormatRule)
    XCTAssertNil(vm.activeDataValidationRule)

    vm.selectWorksheetTab(index: 0)
    XCTAssertEqual(vm.activeConditionalFormatRule, cfRule)
    XCTAssertEqual(vm.activeDataValidationRule, dvRule)

    if case .sheet(let sheetModel) = vm.activeTab?.payload {
      let xlsData = try LegacyOLE2OfficeCodec.encodeBIFF8Worksheet(sheetModel)
      let roundTrippedXLS = try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: xlsData)
      XCTAssertEqual(roundTrippedXLS.conditionalFormatRules, [cfRule])
      XCTAssertEqual(roundTrippedXLS.dataValidationRule, dvRule)

      let xlsxData = try OOXMLContainerCodec.encodeWorksheet(sheetModel)
      let roundTrippedXLSX = try OOXMLContainerCodec.decodeWorksheet(from: xlsxData)
      XCTAssertEqual(roundTrippedXLSX.conditionalFormatRules, [cfRule])
      XCTAssertEqual(roundTrippedXLSX.dataValidationRule, dvRule)

      let odsData = try ODFContainerCodec.encodeWorksheet(sheetModel)
      let roundTrippedODS = try ODFContainerCodec.decodeWorksheet(from: odsData)
      XCTAssertEqual(roundTrippedODS.conditionalFormatRules, [cfRule])
      XCTAssertEqual(roundTrippedODS.dataValidationRule, dvRule)
    } else {
      XCTFail("Expected sheet payload")
    }

    // 4. Slide non-overlapping lower-stage coordinates and legacy .ppt deckTheme / transition timing round-trip
    let slideTabID = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .slide })?.id)
    vm.activeTabID = slideTabID
    vm.executeRibbonCommand("도형")

    if case .slide(var deck) = vm.activeTab?.payload {
      for slide in deck.slides {
        for element in slide.elements {
          XCTAssertGreaterThanOrEqual(
            element.y,
            220.0,
            "Canvas element \(element.title) must sit in lower stage zone (y >= 220) to avoid overlapping slide header/bullets"
          )
        }
      }
      deck.deckTheme = .corporateNavy
      deck.slides[0].transition = .morph
      deck.slides[0].transitionDurationSeconds = 1.75
      deck.slides[0].autoAdvanceSeconds = 12

      let pptData = try LegacyOLE2OfficeCodec.encodePowerPointBinary(deck)
      let decodedDeck = try LegacyOLE2OfficeCodec.decodePowerPointBinary(from: pptData)
      XCTAssertEqual(decodedDeck.deckTheme, .corporateNavy)
      XCTAssertEqual(decodedDeck.slides[0].transition, .morph)
      XCTAssertEqual(decodedDeck.slides[0].transitionDurationSeconds, 1.75, accuracy: 0.001)
      XCTAssertEqual(decodedDeck.slides[0].autoAdvanceSeconds, 12)
    } else {
      XCTFail("Expected slide payload")
    }
  }

  func testNinthAuditRibbonLayoutStyleSwitcherAndContextualObjectToolBadge() throws {
    let vm = WorkspaceViewModel()
    XCTAssertEqual(vm.ribbonLayoutStyle, .standardOffice)
    XCTAssertEqual(
      vm.ribbonLayoutStyle.displayTitle(for: .home),
      WorkspaceRibbonCategory.home.rawValue
    )

    vm.toggleRibbonLayoutStyle()
    XCTAssertEqual(vm.ribbonLayoutStyle, .hangulHwp)
    XCTAssertEqual(vm.ribbonLayoutStyle.displayTitle(for: .home), "편집")
    XCTAssertEqual(vm.ribbonLayoutStyle.displayTitle(for: .insert), "입력")
    XCTAssertEqual(vm.ribbonLayoutStyle.displayTitle(for: .layout), "쪽·원고지")
    XCTAssertEqual(vm.ribbonLayoutStyle.displayTitle(for: .formulasAndData), "표·개체")
    XCTAssertEqual(vm.ribbonLayoutStyle.displayTitle(for: .moduleSpecific), "시트·차트")
    XCTAssertEqual(vm.ribbonLayoutStyle.displayTitle(for: .review), "보안·도구")
    XCTAssertEqual(vm.ribbonLayoutStyle.displayTitle(for: .view), "보기")

    vm.toggleRibbonLayoutStyle()
    XCTAssertEqual(vm.ribbonLayoutStyle, .standardOffice)

    let expectedBadges: [(OfficeModuleKind, String)] = [
      (.document, "표·개체 도구"),
      (.hwp, "쪽·결재·원고지 도구"),
      (.sheet, "수식·피벗 분석"),
      (.slide, "발표자·레이저 도구"),
      (.pdf, "전자결재·주석 도구"),
    ]
    for (moduleKind, expectedBadge) in expectedBadges {
      let tab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == moduleKind }))
      vm.activeTabID = tab.id
      XCTAssertEqual(vm.activeContextualObjectToolBadge, expectedBadge)
    }
  }
}

