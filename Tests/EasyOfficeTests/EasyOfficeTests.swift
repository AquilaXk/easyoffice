import XCTest
@testable import EasyOfficeDesignSystem
@testable import EasyOfficeCore

final class EasyOfficeTests: XCTestCase {

    func testDesignSystemTokensAndContrast() {
        XCTAssertEqual(EasyOfficeDesignTokens.brand700.hex, "#5C6BC0")
        XCTAssertEqual(EasyOfficeDesignTokens.lightScaffold.hex, "#F7F8FC")
        XCTAssertEqual(EasyOfficeDesignTokens.darkScaffold.hex, "#141724")

        // Verify WCAG contrast ratios for body text and primary brand surface
        let lightBodyContrast = EasyOfficeDesignTokens.inkPrimary.contrastRatio(against: EasyOfficeDesignTokens.lightWhite)
        XCTAssertGreaterThan(lightBodyContrast, 12.0)

        let darkBodyContrast = EasyOfficeDesignTokens.darkText.contrastRatio(against: EasyOfficeDesignTokens.darkScaffold)
        XCTAssertGreaterThan(darkBodyContrast, 12.0)

        let brandButtonContrast = EasyOfficeDesignTokens.brand700.contrastRatio(against: EasyOfficeDesignTokens.lightWhite)
        XCTAssertGreaterThan(brandButtonContrast, 4.5)

        // Ensure all 5 modules have distinct semantic accent colors
        let accents = Set(OfficeModuleKind.allCases.map { EasyOfficeDesignTokens.accentToken(for: $0).hex })
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
        XCTAssertThrowsError(try FormatDetector.verify(data: zipPayload, fileName: "huge.docx", maxAllowedBytes: 4)) { error in
            XCTAssertEqual(error as? EasyOfficeError, .payloadTooLarge(actualBytes: 8, maxAllowedBytes: 4))
        }

        // Fail-Closed: Signature mismatch (PDF header inside .hwp file)
        XCTAssertThrowsError(try FormatDetector.verify(data: pdfPayload, fileName: "spoofed.hwp")) { error in
            XCTAssertEqual(error as? EasyOfficeError, .signatureMismatch(expected: .ole2Compound, fileName: "spoofed.hwp"))
        }

        // Fail-Closed: Binary ZIP header masquerading as .csv plain text
        XCTAssertThrowsError(try FormatDetector.verify(data: zipPayload, fileName: "spoofed.csv")) { error in
            XCTAssertEqual(error as? EasyOfficeError, .signatureMismatch(expected: .plainText, fileName: "spoofed.csv"))
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
                ParagraphBlock(style: .body, text: "금년도 프로젝트 검토 결과를 안내드립니다.")
            ],
            tables: [
                DocumentTableBlock(caption: "{{부서}} 현황표", rows: [["담당", "{{담당자}}"]])
            ]
        )

        let merged = doc.applyingMailMerge(record: [
            "수신자": "홍길동",
            "부서": "기술연구소",
            "담당자": "이영희"
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
                SlideModel(title: "슬라이드 3")
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
        let allTabs = OfficeModuleKind.allCases.map { WorkspaceHubCatalog.instantiateSampleTab(for: $0) }
        XCTAssertEqual(allTabs.count, 5)
        let hwpMatches = WorkspaceHubCatalog.searchTabs(allTabs, query: "협조전")
        XCTAssertEqual(hwpMatches.count, 1)
        XCTAssertEqual(hwpMatches.first?.moduleKind, .hwp)
    }
}
