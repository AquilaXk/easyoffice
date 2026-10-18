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
}
