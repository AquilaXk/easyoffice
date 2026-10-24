import XCTest

@testable import EasyOfficeCore
@testable import EasyOfficeDesignSystem

final class EasyOfficeTests: XCTestCase {

  func testDesignSystemTokensAndContrast() {
    XCTAssertEqual(EasyOfficeDesignTokens.brand700.hex, "#5C6BC0")
    XCTAssertEqual(EasyOfficeDesignTokens.lightScaffold.hex, "#F7F8FC")
    XCTAssertEqual(EasyOfficeDesignTokens.darkScaffold.hex, "#141724")

    // Verify WCAG contrast ratios for body text and primary brand surface
    let lightBodyContrast = EasyOfficeDesignTokens.inkPrimary.contrastRatio(
      against: EasyOfficeDesignTokens.lightWhite)
    XCTAssertGreaterThan(lightBodyContrast, 12.0)

    let darkBodyContrast = EasyOfficeDesignTokens.darkText.contrastRatio(
      against: EasyOfficeDesignTokens.darkScaffold)
    XCTAssertGreaterThan(darkBodyContrast, 12.0)

    let brandButtonContrast = EasyOfficeDesignTokens.brand700.contrastRatio(
      against: EasyOfficeDesignTokens.lightWhite)
    XCTAssertGreaterThan(brandButtonContrast, 4.5)

    // Ensure all 5 modules have distinct semantic accent colors
    let accents = Set(
      OfficeModuleKind.allCases.map { EasyOfficeDesignTokens.accentToken(for: $0).hex })
    XCTAssertEqual(accents.count, 5)
  }

  func testSupportedFormatCatalogCompleteness() {
    XCTAssertEqual(DocumentFormat.allCases.count, 20)
    XCTAssertEqual(DocumentFormat.from(fileNameOrExtension: "보고서.HWP"), .hwp)
    XCTAssertEqual(DocumentFormat.from(fileNameOrExtension: "공문서.hwpx"), .hwpx)
    XCTAssertEqual(DocumentFormat.from(fileNameOrExtension: "재무제표.XLSX"), .xlsx)
    XCTAssertEqual(DocumentFormat.from(fileNameOrExtension: "발표자료.pptx"), .pptx)
    XCTAssertEqual(DocumentFormat.from(fileNameOrExtension: "계약서.pdf"), .pdf)

    XCTAssertEqual(DocumentFormat.hwp.containerFamily, .ole2Compound)
    XCTAssertEqual(DocumentFormat.hwpx.containerFamily, .zipXML)
    XCTAssertEqual(DocumentFormat.docx.containerFamily, .zipXML)
    XCTAssertEqual(DocumentFormat.pdf.containerFamily, .pdfBinary)
    XCTAssertEqual(DocumentFormat.csv.containerFamily, .plainText)
  }

  func testFormatDetectorMagicSignaturesAndFailClosed() throws {
    // Valid ZIP-XML container (.docx / .hwpx / .xlsx / .pptx)
    let zipPayload = Data([0x50, 0x4B, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00])
    let verifiedHwpx = try FormatDetector.verify(data: zipPayload, fileName: "sample.hwpx")
    XCTAssertEqual(verifiedHwpx.format, .hwpx)
    XCTAssertEqual(verifiedHwpx.containerFamily, .zipXML)

    // Valid OLE2 CFBF container (.hwp / .doc / .xls)
    let ole2Payload = Data([0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0x00, 0x00])
    let verifiedHwp = try FormatDetector.verify(data: ole2Payload, fileName: "official.hwp")
    XCTAssertEqual(verifiedHwp.format, .hwp)
    XCTAssertEqual(verifiedHwp.containerFamily, .ole2Compound)

    // Valid PDF container
    let pdfPayload = Data("%PDF-1.7\n1 0 obj\n".utf8)
    let verifiedPdf = try FormatDetector.verify(data: pdfPayload, fileName: "manual.pdf")
    XCTAssertEqual(verifiedPdf.format, .pdf)

    // Valid UTF-8 Markdown
    let mdPayload = Data("# 제목\n본문 내용입니다.".utf8)
    let verifiedMd = try FormatDetector.verify(data: mdPayload, fileName: "notes.md")
    XCTAssertEqual(verifiedMd.format, .md)

    // Fail-Closed: Empty payload
    XCTAssertThrowsError(try FormatDetector.verify(data: Data(), fileName: "empty.docx")) { error in
      XCTAssertEqual(error as? EasyOfficeError, .emptyPayload)
    }

    // Fail-Closed: Payload exceeds ceiling
    XCTAssertThrowsError(
      try FormatDetector.verify(data: zipPayload, fileName: "huge.docx", maxAllowedBytes: 4)
    ) { error in
      XCTAssertEqual(
        error as? EasyOfficeError, .payloadTooLarge(actualBytes: 8, maxAllowedBytes: 4))
    }

    // Fail-Closed: Signature mismatch (PDF header inside .hwp file)
    XCTAssertThrowsError(try FormatDetector.verify(data: pdfPayload, fileName: "spoofed.hwp")) {
      error in
      XCTAssertEqual(
        error as? EasyOfficeError,
        .signatureMismatch(expected: .ole2Compound, fileName: "spoofed.hwp"))
    }

    // Fail-Closed: Binary ZIP header masquerading as .csv plain text
    XCTAssertThrowsError(try FormatDetector.verify(data: zipPayload, fileName: "spoofed.csv")) {
      error in
      XCTAssertEqual(
        error as? EasyOfficeError, .signatureMismatch(expected: .plainText, fileName: "spoofed.csv")
      )
    }
  }

  func testDocumentStatisticsOutlineAndMailMerge() {
    let doc = OfficeDocumentModel(
      title: "안내문",
      format: .hwpx,
      headerText: "수신: {{수신자}}",
      footerText: "발신: {{부서}}",
      paragraphs: [
        ParagraphBlock(style: .title, text: "{{수신자}} 귀하"),
        ParagraphBlock(style: .body, text: "금년도 프로젝트 검토 결과를 안내드립니다."),
      ],
      tables: [
        DocumentTableBlock(caption: "{{부서}} 현황표", rows: [["담당", "{{담당자}}"]])
      ]
    )

    let merged = doc.applyingMailMerge(record: [
      "수신자": "홍길동",
      "부서": "기술연구소",
      "담당자": "이영희",
    ])

    XCTAssertEqual(merged.headerText, "수신: 홍길동")
    XCTAssertEqual(merged.footerText, "발신: 기술연구소")
    XCTAssertEqual(merged.paragraphs[0].text, "홍길동 귀하")
    XCTAssertEqual(merged.tables[0].caption, "기술연구소 현황표")
    XCTAssertEqual(merged.tables[0].rows[0][1], "이영희")

    let outline = merged.extractOutline()
    XCTAssertEqual(outline.count, 1)
    XCTAssertEqual(outline.first?.text, "홍길동 귀하")

    let stats = merged.computeStatistics()
    XCTAssertEqual(stats.paragraphCount, 2)
    XCTAssertEqual(stats.tableCount, 1)
    XCTAssertGreaterThan(stats.wordCount, 5)
    XCTAssertEqual(stats.manuscriptPages200, 1)
  }

  func testSpreadsheetCellAddressingAndFormulaEngine() throws {
    let a1 = try CellAddress(a1Notation: "A1")
    XCTAssertEqual(a1.column, 0)
    XCTAssertEqual(a1.row, 0)

    let aa25 = try CellAddress(a1Notation: "$AA$25")
    XCTAssertEqual(aa25.column, 26)
    XCTAssertEqual(aa25.row, 24)
    XCTAssertEqual(aa25.a1Notation, "AA25")

    let range = try CellRange(notation: "A1:B2")
    XCTAssertEqual(range.expandedAddresses().map(\.a1Notation), ["A1", "B1", "A2", "B2"])

    var sheet = WorksheetModel(name: "연산검증")
    try sheet.setCell("A1", rawValue: "10")
    try sheet.setCell("A2", rawValue: "20")
    try sheet.setCell("A3", rawValue: "30")
    try sheet.setCell("A4", rawValue: "텍스트")

    try sheet.setCell("B1", rawValue: "=SUM(A1:A3)")
    try sheet.setCell("B2", rawValue: "=AVERAGE(A1:A3)")
    try sheet.setCell("B3", rawValue: "=MIN(A1:A3)")
    try sheet.setCell("B4", rawValue: "=MAX(A1:A3)")
    try sheet.setCell("B5", rawValue: "=COUNT(A1:A4)")
    try sheet.setCell("B6", rawValue: "=COUNTA(A1:A4)")
    try sheet.setCell("B7", rawValue: "=IF(B1>=60, \"PASS\", \"FAIL\")")
    try sheet.setCell("B8", rawValue: "=ROUND(12.3456, 2)")
    try sheet.setCell("B9", rawValue: "=A1 + A2 * 2")
    try sheet.setCell("B10", rawValue: "=A1 / 0")
    try sheet.setCell("B11", rawValue: "=UNKNOWNFUNC(A1)")

    // Circular reference: C1 -> C2 -> C1
    try sheet.setCell("C1", rawValue: "=C2 + 1")
    try sheet.setCell("C2", rawValue: "=C1 + 1")

    XCTAssertEqual(sheet.evaluate(at: "B1"), .number(60))
    XCTAssertEqual(sheet.evaluate(at: "B2"), .number(20))
    XCTAssertEqual(sheet.evaluate(at: "B3"), .number(10))
    XCTAssertEqual(sheet.evaluate(at: "B4"), .number(30))
    XCTAssertEqual(sheet.evaluate(at: "B5"), .number(3))
    XCTAssertEqual(sheet.evaluate(at: "B6"), .number(4))
    XCTAssertEqual(sheet.evaluate(at: "B7"), .text("PASS"))
    XCTAssertEqual(sheet.evaluate(at: "B8"), .number(12.35))
    XCTAssertEqual(sheet.evaluate(at: "B9"), .number(50))
    XCTAssertEqual(sheet.evaluate(at: "B10"), .error("#DIV/0!"))
    XCTAssertEqual(sheet.evaluate(at: "B11"), .error("#NAME?"))
    XCTAssertEqual(sheet.evaluate(at: "C1"), .error("#REF!"))
  }

  func testSlideDeckOrderingSmartGuidesAndPDFConversion() {
    var deck = PresentationDeckModel(
      title: "덱 테스트",
      aspectRatio: .widescreen16x9,
      slides: [
        SlideModel(title: "슬라이드 1"),
        SlideModel(title: "슬라이드 2"),
        SlideModel(title: "슬라이드 3"),
      ]
    )
    deck.moveSlide(from: 2, to: 0)
    XCTAssertEqual(deck.slides.map(\.title), ["슬라이드 3", "슬라이드 1", "슬라이드 2"])

    let centeredBox = SlideCanvasElement(title: "중앙 상자", x: 380, y: 220, width: 200, height: 100)
    let alignment = deck.isCenteredOnStage(centeredBox)
    XCTAssertTrue(alignment.horizontal)
    XCTAssertTrue(alignment.vertical)

    var pdf = PDFStudioDocumentModel(
      title: "샘플 PDF",
      pages: [
        PDFPageDescriptor(pageNumber: 1, rotationDegrees: 0, extractedText: "첫 번째 문단\n두 번째 문단")
      ]
    )
    pdf.rotatePage(at: 0, clockwiseDegrees: -90)
    XCTAssertEqual(pdf.pages[0].rotationDegrees, 270)

    let converted = pdf.convertToEditableDocument(targetFormat: .hwpx)
    XCTAssertEqual(converted.format, .hwpx)
    XCTAssertEqual(converted.paragraphs.count, 2)
    XCTAssertEqual(converted.paragraphs[0].text, "첫 번째 문단")

    // Verify unified full-text search across all 5 modules
    let allTabs = OfficeModuleKind.allCases.map {
      WorkspaceHubCatalog.instantiateSampleTab(for: $0)
    }
    XCTAssertEqual(allTabs.count, 5)
    let hwpMatches = WorkspaceHubCatalog.searchTabs(allTabs, query: "협조전")
    XCTAssertEqual(hwpMatches.count, 1)
    XCTAssertEqual(hwpMatches.first?.moduleKind, .hwp)
  }

  @MainActor
  func testTenthAuditFrontendDeSlopAndWorkspaceModes() {
    XCTAssertEqual(
      WorkspaceViewMode.allCases,
      [.editView, .outlineView, .readOnlyView]
    )
    for mode in WorkspaceViewMode.allCases {
      XCTAssertFalse(mode.rawValue.isEmpty)
      XCTAssertFalse(mode.symbolName.isEmpty)
      XCTAssertEqual(mode.id, mode.rawValue)
    }

    XCTAssertEqual(
      InspectorSubTab.allCases,
      [.propertiesAndLayout, .formattingAndTools]
    )
    for subTab in InspectorSubTab.allCases {
      XCTAssertFalse(subTab.rawValue.isEmpty)
      XCTAssertEqual(subTab.id, subTab.rawValue)
    }

    XCTAssertEqual(DocumentFormat.allCases.count, 20)
    for format in DocumentFormat.allCases {
      let badge = format.shortSpecificationBadge
      XCTAssertFalse(badge.isEmpty, "shortSpecificationBadge must not be empty for \(format)")
      XCTAssertLessThanOrEqual(
        badge.count,
        14,
        "shortSpecificationBadge (\(badge)) exceeded 14 chars for \(format)"
      )
    }

    XCTAssertEqual(OfficeModuleKind.allCases.count, 5)
    for module in OfficeModuleKind.allCases {
      XCTAssertFalse(module.quickLauncherCardTitleKo.isEmpty)
      XCTAssertFalse(module.quickLauncherFormatBadge.isEmpty)
    }

    let viewModel = WorkspaceViewModel()
    XCTAssertEqual(viewModel.workspaceViewMode, .editView)
    XCTAssertEqual(viewModel.activeInspectorSubTab, .propertiesAndLayout)

    viewModel.setWorkspaceViewMode(.outlineView)
    XCTAssertEqual(viewModel.workspaceViewMode, .outlineView)
    XCTAssertEqual(viewModel.lastExecutedCommandSummary, "윤곽 보기 모드로 전환했습니다.")

    viewModel.setWorkspaceViewMode(.readOnlyView)
    XCTAssertEqual(viewModel.workspaceViewMode, .readOnlyView)
    XCTAssertEqual(viewModel.lastExecutedCommandSummary, "읽기 전용 모드로 전환했습니다.")

    viewModel.setWorkspaceViewMode(.editView)
    XCTAssertEqual(viewModel.workspaceViewMode, .editView)

    viewModel.activeInspectorSubTab = .formattingAndTools
    XCTAssertEqual(viewModel.activeInspectorSubTab, .formattingAndTools)
    viewModel.activeInspectorSubTab = .propertiesAndLayout
    XCTAssertEqual(viewModel.activeInspectorSubTab, .propertiesAndLayout)

    viewModel.applyQuickZoomPreset(125.0)
    XCTAssertEqual(viewModel.zoomPercent, 125.0)
    XCTAssertEqual(viewModel.lastExecutedCommandSummary, "화면 배율 125% 적용")

    viewModel.applyQuickZoomPreset(75.0)
    XCTAssertEqual(viewModel.zoomPercent, 75.0)

    viewModel.openNewSampleTab(for: .hwp)
    viewModel.ribbonLayoutStyle = .hangulHwp
    XCTAssertEqual(RibbonCategory.allCases.count, 7)
    for category in RibbonCategory.allCases {
      let actions = RibbonCategory.actions(for: category, module: .hwp)
      XCTAssertFalse(actions.isEmpty, "Category \(category) must keep actions in .hangulHwp style")
    }

    let initialManuscriptMode = viewModel.isManuscriptGridModeEnabled
    viewModel.toggleManuscriptGridMode()
    XCTAssertEqual(viewModel.isManuscriptGridModeEnabled, !initialManuscriptMode)

    viewModel.cycleActiveDocumentColumnCount()
    viewModel.executeRibbonCommand("doc-mod-footnote")
    if let activeTab = viewModel.activeTab, case .document(let doc) = activeTab.payload {
      XCTAssertEqual(doc.sections.first?.columnCount, 2)
      XCTAssertFalse(doc.footnotes.isEmpty)
    } else {
      XCTFail("Expected active HWP document payload")
    }
  }

  @MainActor
  func testEleventhAuditBitLevelPerformanceAndCommercialUXParity() throws {
    XCTAssertEqual(Adler32Calculator.nmax, 5_552)
    XCTAssertEqual(Adler32Calculator.modulo, 65_521)
    XCTAssertEqual(Adler32Calculator.compute(for: Data()), 1)
    let wikiData = Data("Wikipedia".utf8)
    XCTAssertEqual(Adler32Calculator.compute(for: wikiData), 0x11E6_0398)

    var largePayload = Data(count: 12_345)
    for i in 0..<largePayload.count {
      largePayload[i] = UInt8((i &* 73 &+ 19) & 0xFF)
    }
    var refS1: UInt32 = 1
    var refS2: UInt32 = 0
    for byte in largePayload {
      refS1 = (refS1 + UInt32(byte)) % 65_521
      refS2 = (refS2 + refS1) % 65_521
    }
    let expectedLargeAdler = (refS2 << 16) | refS1
    XCTAssertEqual(Adler32Calculator.compute(for: largePayload), expectedLargeAdler)

    var fakeZlibStream = Data([0x78, 0x9C, 0x01, 0x02])
    let wikiAdler = Adler32Calculator.compute(for: wikiData)
    fakeZlibStream.append(UInt8((wikiAdler >> 24) & 0xFF))
    fakeZlibStream.append(UInt8((wikiAdler >> 16) & 0xFF))
    fakeZlibStream.append(UInt8((wikiAdler >> 8) & 0xFF))
    fakeZlibStream.append(UInt8(wikiAdler & 0xFF))
    XCTAssertTrue(
      Adler32Calculator.verifyZlibTrailer(in: fakeZlibStream, decompressed: wikiData)
    )
    fakeZlibStream[fakeZlibStream.count - 1] ^= 0xFF
    XCTAssertFalse(
      Adler32Calculator.verifyZlibTrailer(in: fakeZlibStream, decompressed: wikiData)
    )

    let asciiSample = "Root Entry/Workbook/Section0/BodyText_2026_Commercial_FastPath"
    var asciiUTF16LE = Data()
    for scalar in asciiSample.utf16 {
      asciiUTF16LE.append(UInt8(scalar & 0xFF))
      asciiUTF16LE.append(UInt8((scalar >> 8) & 0xFF))
    }
    let decodedASCII = try BinaryContainerCodec.decodeUTF16LE(
      from: asciiUTF16LE,
      at: 0,
      byteLength: asciiUTF16LE.count
    )
    XCTAssertEqual(decodedASCII, asciiSample)

    let mixedSample = "EasyOffice 2026 표준 오피스 문서 — 한글·영문 혼합 테스트"
    var mixedUTF16LE = Data()
    for scalar in mixedSample.utf16 {
      mixedUTF16LE.append(UInt8(scalar & 0xFF))
      mixedUTF16LE.append(UInt8((scalar >> 8) & 0xFF))
    }
    let decodedMixed = try BinaryContainerCodec.decodeUTF16LE(
      from: mixedUTF16LE,
      at: 0,
      byteLength: mixedUTF16LE.count
    )
    XCTAssertEqual(decodedMixed, mixedSample)

    let validSectorTable = OLE2SectorTable(entries: [
      1, 2, 3, OLE2SectorTable.endOfChain,
    ])
    let validChain = try validSectorTable.chain(startingAt: 0, maxSectors: 16)
    XCTAssertEqual(validChain, [0, 1, 2, 3])

    let cyclicSectorTable = OLE2SectorTable(entries: [
      1, 2, 0, OLE2SectorTable.endOfChain,
    ])
    XCTAssertThrowsError(try cyclicSectorTable.chain(startingAt: 0, maxSectors: 16)) { error in
      guard case EasyOfficeError.corruptedContainer = error else {
        XCTFail("Expected corruptedContainer on cyclic FAT chain, got \(error)")
        return
      }
    }

    var cellBits = FastCellBitset()
    let c0 = PackedCellCoordinate(row: 0, column: 0)
    let cMaxInline = PackedCellCoordinate(row: 63, column: 15)
    let cOutOfInline = PackedCellCoordinate(row: 64, column: 0)
    XCTAssertFalse(cellBits.contains(c0))
    XCTAssertTrue(cellBits.insert(c0))
    XCTAssertTrue(cellBits.contains(c0))
    XCTAssertTrue(cellBits.insert(cMaxInline))
    XCTAssertTrue(cellBits.contains(cMaxInline))
    XCTAssertFalse(cellBits.insert(cOutOfInline))
    XCTAssertFalse(cellBits.contains(cOutOfInline))
    cellBits.remove(c0)
    XCTAssertFalse(cellBits.contains(c0))

    var sheet1 = WorksheetModel(name: "Sheet1")
    sheet1.addSheetPage(
      named: "Sheet2",
      initialCells: [
        PackedCellCoordinate(row: 0, column: 0): "250",
        PackedCellCoordinate(row: 1, column: 0): "=A1*2",
      ]
    )
    try sheet1.setCell("A1", rawValue: "=Sheet2!A2+50")
    try sheet1.setCell("B1", rawValue: "=C1")
    try sheet1.setCell("C1", rawValue: "=B1")
    XCTAssertEqual(sheet1.evaluate(at: "A1"), .number(550))
    XCTAssertEqual(sheet1.evaluate(at: "B1"), .error("#REF!"))

    let vm = WorkspaceViewModel()
    vm.openNewSampleTab(for: .sheet)
    vm.selectSheetCellOrRange(primaryCell: "B2", rangeNotation: "B2:D2")
    let statusSummary = vm.sheetSelectionStatusSummary
    XCTAssertTrue(statusSummary.contains("합계"), "Expected 합계 in \(statusSummary)")
    XCTAssertTrue(statusSummary.contains("평균"), "Expected 평균 in \(statusSummary)")
    XCTAssertTrue(statusSummary.contains("개수"), "Expected 개수 in \(statusSummary)")
    XCTAssertTrue(statusSummary.contains("최대"), "Expected 최대 in \(statusSummary)")

    XCTAssertGreaterThanOrEqual(vm.openTabs.count, 3)
    vm.openTabs[0].isPinned = true
    let pinnedTabID = vm.openTabs[0].id
    let secondTabID = vm.openTabs[1].id
    for i in 1..<vm.openTabs.count {
      vm.openTabs[i].isPinned = false
    }
    vm.closeOtherTabs(keeping: secondTabID)
    XCTAssertTrue(vm.openTabs.contains(where: { $0.id == pinnedTabID }))
    XCTAssertTrue(vm.openTabs.contains(where: { $0.id == secondTabID }))
    XCTAssertEqual(vm.activeTabID, secondTabID)
    XCTAssertTrue(vm.openTabs.allSatisfy { $0.id == secondTabID || $0.isPinned })

    vm.openNewSampleTab(for: .document)
    vm.setDocumentHorizontalPadding(8.0)
    XCTAssertEqual(vm.documentHorizontalPadding, 24.0, accuracy: 1e-6)
    vm.setDocumentHorizontalPadding(64.0)
    XCTAssertEqual(vm.documentHorizontalPadding, 64.0, accuracy: 1e-6)
    vm.setDocumentHorizontalPadding(200.0)
    XCTAssertEqual(vm.documentHorizontalPadding, 96.0, accuracy: 1e-6)

    if let docTab = vm.activeTab, case .document(let doc) = docTab.payload,
      let firstPara = doc.paragraphs.first
    {
      XCTAssertEqual(firstPara.style, .title)
      vm.promoteOrDemoteParagraphOutlineLevel(paragraphID: firstPara.id, delta: 1)
      if case .document(let updatedDoc) = vm.activeTab?.payload {
        XCTAssertEqual(updatedDoc.paragraphs.first?.style, .heading1)
      }
      vm.promoteOrDemoteParagraphOutlineLevel(paragraphID: firstPara.id, delta: -1)
      if case .document(let restoredDoc) = vm.activeTab?.payload {
        XCTAssertEqual(restoredDoc.paragraphs.first?.style, .title)
      }
    } else {
      XCTFail("Expected active document tab")
    }

    vm.openNewSampleTab(for: .pdf)
    vm.updatePDFFormFieldValue(fieldName: "승인자 성명", value: "김민수")
    if case .pdf(let pdfAfterCreate) = vm.activeTab?.payload {
      XCTAssertEqual(
        pdfAfterCreate.formFields.first(where: { $0.name == "승인자 성명" })?.value,
        "김민수"
      )
    } else {
      XCTFail("Expected active PDF tab")
    }
    vm.updatePDFFormFieldValue(fieldName: "승인자 성명", value: "이서연")
    if case .pdf(let pdfAfterUpdate) = vm.activeTab?.payload {
      XCTAssertEqual(
        pdfAfterUpdate.formFields.filter { $0.name == "승인자 성명" }.count,
        1
      )
      XCTAssertEqual(
        pdfAfterUpdate.formFields.first(where: { $0.name == "승인자 성명" })?.value,
        "이서연"
      )
    }
  }
}


