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

    // Table row/column insert and delete
    let initialRowCount: Int
    let initialColCount: Int
    if case .document(let docBeforeTable) = vm.activeTab?.payload {
      initialRowCount = docBeforeTable.tables[0].rowCount
      initialColCount = docBeforeTable.tables[0].columnCount
    } else {
      return XCTFail("Expected document payload")
    }

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
    vm.toggleMergeOnSelectedSheetCell()
    XCTAssertTrue(vm.mergedSheetRanges(for: sheetTab.id).contains("B2:D3"))
    vm.toggleMergeOnSelectedSheetCell()
    XCTAssertFalse(vm.mergedSheetRanges(for: sheetTab.id).contains("B2:D3"))

    // Row & column insert/delete at selection
    vm.selectSheetCellOrRange(primaryCell: "B2")
    vm.insertSheetRowAtSelection()
    if case .sheet(let afterRowInsert) = vm.activeTab?.payload {
      XCTAssertEqual(afterRowInsert.rawValue(at: "A3"), "엔진 최적화")
    }
    vm.deleteSheetRowAtSelection()
    if case .sheet(let afterRowDelete) = vm.activeTab?.payload {
      XCTAssertEqual(afterRowDelete.rawValue(at: "A2"), "엔진 최적화")
    }

    vm.insertSheetColumnAtSelection()
    vm.deleteSheetColumnAtSelection()

    // Named range definition & formula evaluation
    vm.defineNamedRangeOnActiveSheet(name: "핵심실적", targetNotation: "B2:C2")
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
    let redactedPageCount = vm.redactSensitiveTextOnActivePDF(phrase: "매직 바이트")
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
    vm.updatePDFFormField(id: tfID, value: "최현우 수석")
    vm.updatePDFFormField(id: cbID, isChecked: true)

    if case .pdf(let pdfUpdatedFields) = vm.activeTab?.payload {
      let tf = try XCTUnwrap(pdfUpdatedFields.formFields.first(where: { $0.id == tfID }))
      let cb = try XCTUnwrap(pdfUpdatedFields.formFields.first(where: { $0.id == cbID }))
      XCTAssertEqual(tf.value, "최현우 수석")
      XCTAssertTrue(cb.isChecked)
    }

    // Page reorder
    vm.selectedPDFPageIndex = 0
    vm.moveActivePDFPage(offset: 1)
    XCTAssertEqual(vm.selectedPDFPageIndex, 1)

    // Open a second PDF tab and merge into the active PDF tab
    vm.openNewSampleTab(for: .pdf)
    vm.activeTabID = pdfTab.id
    let countBeforeMerge: Int = {
      if case .pdf(let doc) = vm.activeTab?.payload { return doc.pages.count }
      return 0
    }()
    vm.mergeActivePDFWithAnotherOpenTab()
    if case .pdf(let mergedDoc) = vm.activeTab?.payload {
      XCTAssertGreaterThan(mergedDoc.pages.count, countBeforeMerge)
    } else {
      XCTFail("Expected merged PDF payload")
    }
  }
}
