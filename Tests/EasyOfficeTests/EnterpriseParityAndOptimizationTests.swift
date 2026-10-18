import XCTest

@testable import EasyOfficeCore
@testable import EasyOfficeDesignSystem

final class EnterpriseParityAndOptimizationTests: XCTestCase {

  // MARK: - 1. Named Ranges & Standard #NAME? Diagnostics

  func testNamedRangesResolutionAndUnknownIdentifierNameError() throws {
    var sheet = WorksheetModel(
      name: "분기실적",
      rawCells: [
        "B2": "120",
        "C2": "180",
        "B3": "300",
        "C3": "400",
        "D1": "0.1",
      ]
    )
    sheet.setNamedRange("분기매출", rangeOrCellNotation: "B2:C3")
    sheet.setNamedRange("부가세율", rangeOrCellNotation: "D1")

    try sheet.setCell("E2", rawValue: "=SUM(분기매출)")
    try sheet.setCell("E3", rawValue: "=AVERAGE(분기매출)")
    try sheet.setCell("E4", rawValue: "=ROWS(분기매출)")
    try sheet.setCell("E5", rawValue: "=COLUMNS(분기매출)")
    try sheet.setCell("E6", rawValue: "=E2 * 부가세율")
    try sheet.setCell("F1", rawValue: "=SUM(미정의이름)")
    try sheet.setCell("F2", rawValue: "=존재하지않는범위")
    try sheet.setCell("F3", rawValue: "=INVALID_SYNTAX_$$")

    XCTAssertEqual(sheet.resolveNamedRange("분기매출"), "B2:C3")
    XCTAssertEqual(sheet.evaluate(at: "E2"), .number(1000))
    XCTAssertEqual(sheet.evaluate(at: "E3"), .number(250))
    XCTAssertEqual(sheet.evaluate(at: "E4"), .number(2))
    XCTAssertEqual(sheet.evaluate(at: "E5"), .number(2))
    XCTAssertEqual(sheet.evaluate(at: "E6"), .number(100))
    XCTAssertEqual(sheet.evaluate(at: "F1"), .error("#NAME?"))
    XCTAssertEqual(sheet.evaluate(at: "F2"), .error("#NAME?"))
    XCTAssertEqual(sheet.evaluate(at: "F3"), .error("#VALUE!"))

    // Codable & BIFF8 round-trip preserves namedRanges
    let jsonData = try JSONEncoder().encode(sheet)
    let decodedJsonSheet = try JSONDecoder().decode(WorksheetModel.self, from: jsonData)
    XCTAssertEqual(decodedJsonSheet.namedRanges, sheet.namedRanges)
    XCTAssertEqual(decodedJsonSheet.evaluate(at: "E2"), .number(1000))

    let biffData = try LegacyOLE2OfficeCodec.encodeBIFF8Worksheet(sheet)
    let decodedBiffSheet = try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: biffData)
    XCTAssertEqual(decodedBiffSheet.resolveNamedRange("분기매출"), "B2:C3")
    XCTAssertEqual(decodedBiffSheet.evaluate(at: "E2"), .number(1000))

    sheet.removeNamedRange("분기매출")
    XCTAssertNil(sheet.resolveNamedRange("분기매출"))
    XCTAssertEqual(sheet.evaluate(at: "E2"), .error("#NAME?"))
  }

  // MARK: - 2. Spreadsheet Row & Column Insert/Delete with Formula Shifting

  func testSpreadsheetInsertAndDeleteRowAndColumnShiftsCoordinatesAndFormulas() throws {
    var sheet = WorksheetModel(
      name: "구조편집",
      rawCells: [
        "B2": "10",
        "B3": "20",
        "B4": "=SUM(B2:B3)",
        "C2": "100",
        "D2": "=B2+C2",
      ]
    )
    try sheet.setCellStyle(CellStyle(isBold: true), at: "B2")
    sheet.sheetComments[PackedCellCoordinate(row: 1, column: 1)] = "기준값"
    sheet.setRowHeight(42.0, at: 1)
    sheet.setColumnWidth(160.0, at: 1)

    // Insert row at 0-based index 1 (row 2)
    sheet.insertRow(at: 1)
    XCTAssertEqual(sheet.rawValue(at: "B2"), "")
    XCTAssertEqual(sheet.rawValue(at: "B3"), "10")
    XCTAssertEqual(sheet.rawValue(at: "B4"), "20")
    XCTAssertEqual(sheet.rawValue(at: "B5"), "=SUM(B3:B4)")
    XCTAssertEqual(sheet.evaluate(at: "B5"), .number(30))
    XCTAssertTrue(sheet.cellStyle(at: "B3").isBold)
    XCTAssertEqual(sheet.sheetComments[PackedCellCoordinate(row: 2, column: 1)], "기준값")
    XCTAssertEqual(sheet.rowHeight(at: 2), 42.0)

    // Delete the inserted row at 0-based index 1
    sheet.deleteRow(at: 1)
    XCTAssertEqual(sheet.rawValue(at: "B2"), "10")
    XCTAssertEqual(sheet.rawValue(at: "B3"), "20")
    XCTAssertEqual(sheet.rawValue(at: "B4"), "=SUM(B2:B3)")
    XCTAssertEqual(sheet.evaluate(at: "B4"), .number(30))

    // Insert column at 0-based index 1 (column B)
    sheet.insertColumn(at: 1)
    XCTAssertEqual(sheet.rawValue(at: "C2"), "10")
    XCTAssertEqual(sheet.rawValue(at: "D2"), "100")
    XCTAssertEqual(sheet.rawValue(at: "E2"), "=C2+D2")
    XCTAssertEqual(sheet.evaluate(at: "E2"), .number(110))
    XCTAssertEqual(sheet.columnWidth(at: 2), 160.0)

    // Delete the inserted column at 0-based index 1
    sheet.deleteColumn(at: 1)
    XCTAssertEqual(sheet.rawValue(at: "B2"), "10")
    XCTAssertEqual(sheet.rawValue(at: "C2"), "100")
    XCTAssertEqual(sheet.rawValue(at: "D2"), "=B2+C2")
    XCTAssertEqual(sheet.evaluate(at: "D2"), .number(110))
  }

  // MARK: - 3. Goal Seek Numerical Solver & Sparkline Normalization

  func testGoalSeekSolverAndSparklineNormalizedValues() throws {
    var sheet = WorksheetModel(
      name: "목표값찾기",
      rawCells: [
        "A2": "50",
        "C2": "=A2*2 + 100",
        "B1": "10",
        "B2": "30",
        "B3": "50",
      ]
    )

    let solved = try sheet.goalSeek(targetCell: "C2", targetValue: 500, variableCell: "A2")
    let unwrappedSolved = try XCTUnwrap(solved)
    XCTAssertEqual(unwrappedSolved, 200.0, accuracy: 1e-5)
    XCTAssertEqual(sheet.evaluate(at: "A2").numericValue ?? 0, 200.0, accuracy: 1e-5)
    XCTAssertEqual(sheet.evaluate(at: "C2").numericValue ?? 0, 500.0, accuracy: 1e-4)

    let sparkRange = try CellRange(a1Range: "B1:B3")
    let normalized = sheet.sparklineNormalizedValues(for: sparkRange)
    XCTAssertEqual(normalized.count, 3)
    XCTAssertEqual(normalized[0], 0.0, accuracy: 1e-9)
    XCTAssertEqual(normalized[1], 0.5, accuracy: 1e-9)
    XCTAssertEqual(normalized[2], 1.0, accuracy: 1e-9)

    try sheet.setCell("D1", rawValue: "7")
    try sheet.setCell("D2", rawValue: "7")
    let flatRange = try CellRange(a1Range: "D1:D2")
    XCTAssertEqual(sheet.sparklineNormalizedValues(for: flatRange), [0.5, 0.5])
  }

  // MARK: - 4. 15 New Enterprise Functions, MATCH match_type, and TEXT "#,##0"

  func testFifteenEnterpriseFunctionsMatchTypeAndThousandsTextFormatting() throws {
    var sheet = WorksheetModel(
      name: "엔터프라이즈함수",
      rawCells: [
        // Table for *IFS: Col A = Dept, Col B = Region, Col C = Sales
        "A1": "영업", "B1": "서울", "C1": "100",
        "A2": "영업", "B2": "부산", "C2": "200",
        "A3": "영업", "B3": "서울", "C3": "300",
        "A4": "개발", "B4": "서울", "C4": "400",
        // Sorted ascending in E1:E4 for MATCH match_type = 1
        "E1": "10", "E2": "20", "E3": "30", "E4": "40",
        // Sorted descending in F1:F4 for MATCH match_type = -1
        "F1": "40", "F2": "30", "F3": "20", "F4": "10",
        // Text & Info cells
        "G1": "Easy", "G2": "", "G3": "Office",
        "H2": "=1/0",
      ]
    )

    // Multi-criteria *IFS
    try sheet.setCell("D1", rawValue: "=SUMIFS(C1:C4, A1:A4, \"영업\", B1:B4, \"서울\")")
    try sheet.setCell("D2", rawValue: "=COUNTIFS(A1:A4, \"영업\", B1:B4, \"서울\")")
    try sheet.setCell("D3", rawValue: "=AVERAGEIFS(C1:C4, A1:A4, \"영업\", B1:B4, \"서울\")")
    try sheet.setCell("D4", rawValue: "=MINIFS(C1:C4, A1:A4, \"영업\", B1:B4, \"서울\")")
    try sheet.setCell("D5", rawValue: "=MAXIFS(C1:C4, A1:A4, \"영업\", B1:B4, \"서울\")")

    XCTAssertEqual(sheet.evaluate(at: "D1"), .number(400))
    XCTAssertEqual(sheet.evaluate(at: "D2"), .number(2))
    XCTAssertEqual(sheet.evaluate(at: "D3"), .number(200))
    XCTAssertEqual(sheet.evaluate(at: "D4"), .number(100))
    XCTAssertEqual(sheet.evaluate(at: "D5"), .number(300))

    // IFNA, TEXTJOIN, SEARCH, REPLACE
    try sheet.setCell("D6", rawValue: "=IFNA(MATCH(999, E1:E4, 0), \"미발견\")")
    try sheet.setCell("D7", rawValue: "=TEXTJOIN(\"-\", TRUE, G1:G3)")
    try sheet.setCell("D8", rawValue: "=SEARCH(\"office\", \"EasyOffice Suite\")")
    try sheet.setCell("D9", rawValue: "=REPLACE(\"2025-보고서\", 1, 4, \"2026\")")

    XCTAssertEqual(sheet.evaluate(at: "D6"), .text("미발견"))
    XCTAssertEqual(sheet.evaluate(at: "D7"), .text("Easy-Office"))
    XCTAssertEqual(sheet.evaluate(at: "D8"), .number(5))
    XCTAssertEqual(sheet.evaluate(at: "D9"), .text("2026-보고서"))

    // ISBLANK, ISERROR, ISNUMBER, ISTEXT, ROWS, COLUMNS
    try sheet.setCell("I1", rawValue: "=ISBLANK(H1)")
    try sheet.setCell("I2", rawValue: "=ISERROR(H2)")
    try sheet.setCell("I3", rawValue: "=ISNUMBER(C1)")
    try sheet.setCell("I4", rawValue: "=ISTEXT(A1)")
    try sheet.setCell("I5", rawValue: "=ROWS(A1:C4)")
    try sheet.setCell("I6", rawValue: "=COLUMNS(A1:C4)")

    XCTAssertEqual(sheet.evaluate(at: "I1"), .boolean(true))
    XCTAssertEqual(sheet.evaluate(at: "I2"), .boolean(true))
    XCTAssertEqual(sheet.evaluate(at: "I3"), .boolean(true))
    XCTAssertEqual(sheet.evaluate(at: "I4"), .boolean(true))
    XCTAssertEqual(sheet.evaluate(at: "I5"), .number(4))
    XCTAssertEqual(sheet.evaluate(at: "I6"), .number(3))

    // MATCH with match_type 1, 0, -1
    try sheet.setCell("J1", rawValue: "=MATCH(25, E1:E4, 1)")
    try sheet.setCell("J2", rawValue: "=MATCH(30, E1:E4, 0)")
    try sheet.setCell("J3", rawValue: "=MATCH(25, F1:F4, -1)")

    XCTAssertEqual(sheet.evaluate(at: "J1"), .number(2))  // 20 at index 2 is largest <= 25
    XCTAssertEqual(sheet.evaluate(at: "J2"), .number(3))  // exact match 30 at index 3
    XCTAssertEqual(sheet.evaluate(at: "J3"), .number(2))  // 30 at index 2 is smallest >= 25

    // TEXT with "#,##0" and "#,##0.00"
    try sheet.setCell("K1", rawValue: "=TEXT(1234567, \"#,##0\")")
    try sheet.setCell("K2", rawValue: "=TEXT(1234567.89, \"#,##0.00\")")
    XCTAssertEqual(sheet.evaluate(at: "K1"), .text("1,234,567"))
    XCTAssertEqual(sheet.evaluate(at: "K2"), .text("1,234,567.89"))
  }

  // MARK: - 5. Quote-Aware XMLSanitizer, DocumentTextRun & DocumentTableBlock Structural Edits

  func testQuoteAwareXMLSanitizerAndDocumentRunAndTableOperations() throws {
    let xml =
      "<row><cell formula=\"A1>10\" label=\"x<y\">값1</cell><cell formula=\"B1>=20\">값2</cell></row>"
    let cells = XMLSanitizer.extractElements(named: "cell", in: xml)
    XCTAssertEqual(cells.count, 2)
    XCTAssertEqual(cells.first?.innerXML, "값1")
    XCTAssertEqual(cells.first?.attributes["formula"], "A1>10")
    XCTAssertEqual(cells.first?.attributes["label"], "x<y")
    XCTAssertEqual(cells.last?.innerXML, "값2")

    let run = DocumentTextRun(
      text: "강조 링크",
      isBold: true,
      isStrikethrough: true,
      foregroundHex: "#5C6BC0",
      backgroundHex: "#FFF59D",
      linkURL: "https://example.org/spec"
    )
    XCTAssertTrue(run.isStrikethrough)
    XCTAssertEqual(run.backgroundHex, "#FFF59D")
    XCTAssertEqual(run.linkURL, "https://example.org/spec")

    var table = DocumentTableBlock(
      caption: "분기 요약",
      rows: [
        ["항목", "1분기"],
        ["매출", "100"],
      ],
      hasHeaderRow: true
    )
    table.insertRow(at: 1, defaultCellText: "신규")
    XCTAssertEqual(table.rowCount, 3)
    XCTAssertEqual(table.rows[1], ["신규", "신규"])

    table.insertColumn(at: 1, headerTitle: "예산", defaultCellText: "0")
    XCTAssertEqual(table.columnCount, 3)
    XCTAssertEqual(table.rows[0], ["항목", "예산", "1분기"])

    table.deleteColumn(at: 1)
    XCTAssertEqual(table.columnCount, 2)

    table.deleteRow(at: 1)
    XCTAssertEqual(table.rowCount, 2)
    XCTAssertEqual(table.rows[1], ["매출", "100"])

    var pdf = PDFStudioDocumentModel(
      title: "기밀 계약서 (900101-1234567)",
      pages: [
        PDFPageDescriptor(pageNumber: 1, extractedText: "담당자 주민번호 900101-1234567 및 서명란")
      ],
      annotations: [
        PDFAnnotationItem(
          pageNumber: 1, kind: .stickyNote, content: "검토 메모: 900101-1234567 확인 요망", author: "QA")
      ],
      outline: [
        PDFOutlineNode(title: "부록 900101-1234567", targetPageNumber: 1)
      ]
    )
    let redactedCount = pdf.redactText("900101-1234567")
    XCTAssertGreaterThanOrEqual(redactedCount, 1)
    XCTAssertTrue(pdf.isRedacted)
    XCTAssertFalse(pdf.pages[0].extractedText.contains("900101-1234567"))
    XCTAssertFalse(pdf.title.contains("900101-1234567"))
    XCTAssertFalse(pdf.annotations[0].content.contains("900101-1234567"))
    XCTAssertFalse(pdf.outline[0].title.contains("900101-1234567"))
    XCTAssertFalse(pdf.redactedPhrases.joined(separator: " ").contains("900101-1234567"))

    let field = PDFFormField(
      pageNumber: 1, name: "승인자", value: "홍길동", isCheckbox: false, isChecked: false)
    pdf.addFormField(field)
    let pdfBytes = PDFBinarySerializer.encode(pdf)
    let rawPDFString = String(decoding: pdfBytes, as: UTF8.self)
    XCTAssertFalse(rawPDFString.contains("900101-1234567"))
    let decodedPDF = try PDFBinarySerializer.decode(from: pdfBytes)
    XCTAssertTrue(decodedPDF.isRedacted)
    XCTAssertFalse(decodedPDF.redactedPhrases.joined(separator: " ").contains("900101-1234567"))
    XCTAssertEqual(decodedPDF.formFields.first?.name, "승인자")
    XCTAssertEqual(decodedPDF.formFields.first?.value, "홍길동")

    var shiftSheet = WorksheetModel(name: "Shift")
    shiftSheet.setNamedRange("TARGET_BLOCK", rangeOrCellNotation: "B2:C3")
    shiftSheet.insertRow(at: 1)
    XCTAssertEqual(shiftSheet.namedRanges["TARGET_BLOCK"], "B3:C4")
  }

  // MARK: - 6. Cross-Sheet Formula Shift Immunity, Range Shrinking / #REF!, and Data Cleansing

  func testFifthAuditDarkTokensCrossSheetDeleteListsSlideThemesAndPDFOutlines() throws {
    // F2: Cross-sheet immunity, range endpoint shrinking, and single-cell #REF! on deleteRow / insertRow
    var sheet = WorksheetModel(
      name: "Sheet1",
      rawCells: [
        "A1": "=Sheet2!B5 + SUM(B2:B5) + B5",
        "A2": "='Summary Sheet'!A1:B5 + SUM(B5:D5)",
        "B2": "10",
        "B3": "20",
        "B4": "30",
        "B5": "40",
      ]
    )

    // Deleting row 4 (1-based row 5, i.e. B5)
    sheet.deleteRow(at: 4)
    XCTAssertEqual(sheet.rawValue(at: "A1"), "=Sheet2!B5 + SUM(B2:B4) + #REF!")
    XCTAssertEqual(sheet.rawValue(at: "A2"), "='Summary Sheet'!A1:B5 + SUM(#REF!)")
    XCTAssertEqual(sheet.evaluate(at: "A1"), .error("#REF!"))

    // Inserting row at index 1 (1-based row 2) keeps cross-sheet Sheet2!B5 untouched while shifting local B2:B4 -> B3:B5
    try sheet.setCell("A3", rawValue: "=Sheet2!B5 + SUM(B2:B4)")
    sheet.insertRow(at: 1)
    XCTAssertEqual(sheet.rawValue(at: "A4"), "=Sheet2!B5 + SUM(B3:B5)")

    // Deleting the inserted row at index 1 shifts B3:B5 back to B2:B4
    sheet.deleteRow(at: 1)
    XCTAssertEqual(sheet.rawValue(at: "A3"), "=Sheet2!B5 + SUM(B2:B4)")

    // Deleting the start boundary of B2:B4 (index 1, 1-based row 2) shrinks B2:B4 -> B2:B3 (and A3 moves to A2)
    sheet.deleteRow(at: 1)
    XCTAssertEqual(sheet.rawValue(at: "A2"), "=Sheet2!B5 + SUM(B2:B3)")

    // Preserve existing Auto-Fill formula shifter contract
    XCTAssertEqual(
      WorksheetModel.shiftFormulaReferences(
        "=B2+$A$1+$C2+D$2+Sheet2!B2", rowDelta: 2, columnDelta: 1),
      "=C4+$A$1+$C4+E$2+Sheet2!C4"
    )

    // F6: Enterprise data cleansing — removeDuplicateRows and splitColumnByDelimiter
    var cleanSheet = WorksheetModel(
      name: "데이터정제",
      rawCells: [
        "A1": "이름", "B1": "부서",
        "A2": "홍길동", "B2": "개발팀",
        "A3": "김영희", "B3": "기획팀",
        "A4": "홍길동", "B4": "개발팀",
        "A5": "이철수", "B5": "디자인팀",
        "A6": "김영희", "B6": "기획팀",
      ]
    )
    let removedCount = cleanSheet.removeDuplicateRows()
    XCTAssertEqual(removedCount, 2)
    XCTAssertEqual(cleanSheet.rawValue(at: "A2"), "홍길동")
    XCTAssertEqual(cleanSheet.rawValue(at: "A3"), "김영희")
    XCTAssertEqual(cleanSheet.rawValue(at: "A4"), "이철수")
    XCTAssertEqual(cleanSheet.rawValue(at: "A5"), "")

    var splitSheet = WorksheetModel(
      name: "텍스트나누기",
      rawCells: [
        "A1": "홍길동, 개발팀, 서울",
        "A2": "김영희, 기획팀, 부산",
      ]
    )
    let splitCount = splitSheet.splitColumnByDelimiter(column: 0, delimiter: ",")
    XCTAssertEqual(splitCount, 2)
    XCTAssertEqual(splitSheet.rawValue(at: "A1"), "홍길동")
    XCTAssertEqual(splitSheet.rawValue(at: "B1"), "개발팀")
    XCTAssertEqual(splitSheet.rawValue(at: "C1"), "서울")
    XCTAssertEqual(splitSheet.rawValue(at: "A2"), "김영희")
    XCTAssertEqual(splitSheet.rawValue(at: "B2"), "기획팀")
    XCTAssertEqual(splitSheet.rawValue(at: "C2"), "부산")

    // F1 & F12: Dark mode surface tokens and relative luminance helper
    let darkPalette = WorkspaceThemeMode.darkIndigo.surfacePalette(isSystemDark: true)
    XCTAssertEqual(darkPalette.selectedSurface.hex, "#283152")
    XCTAssertTrue(EasyOfficeDesignTokens.isLightFill(hex: "#F8F9FF"))
    XCTAssertFalse(EasyOfficeDesignTokens.isLightFill(hex: "#141724"))

    // F4 & F5: Multilevel list markers and Korean official document list hierarchy (1. -> 가. -> (1) -> (가))
    let doc = OfficeDocumentModel(
      title: "공문서 다단계 개요",
      paragraphs: [
        ParagraphBlock(text: "대항목", listKind: .koreanOfficial, listLevel: 0),
        ParagraphBlock(text: "중항목", listKind: .koreanOfficial, listLevel: 1),
        ParagraphBlock(text: "소항목", listKind: .koreanOfficial, listLevel: 2),
        ParagraphBlock(text: "세부항목", listKind: .koreanOfficial, listLevel: 3),
        ParagraphBlock(text: "영문 중항목", listKind: .numbered, listLevel: 1),
        ParagraphBlock(text: "로마자 소항목", listKind: .numbered, listLevel: 2),
      ]
    )
    XCTAssertEqual(doc.formattedListPrefix(forParagraphAt: 0), "1.")
    XCTAssertEqual(doc.formattedListPrefix(forParagraphAt: 1), "가.")
    XCTAssertEqual(doc.formattedListPrefix(forParagraphAt: 2), "(1)")
    XCTAssertEqual(doc.formattedListPrefix(forParagraphAt: 3), "(가)")
    XCTAssertEqual(doc.formattedListPrefix(forParagraphAt: 4), "a.")
    XCTAssertEqual(doc.formattedListPrefix(forParagraphAt: 5), "i.")

    // F7: SlideDeckTheme & SlideModel.alignElements
    let el1 = SlideCanvasElement(
      title: "Box1", x: 100, y: 100, width: 80, height: 60, kind: .rectangleShape)
    let el2 = SlideCanvasElement(
      title: "Box2", x: 200, y: 140, width: 80, height: 60, kind: .rectangleShape)
    let el3 = SlideCanvasElement(
      title: "Box3", x: 500, y: 180, width: 80, height: 60, kind: .rectangleShape)
    var deck = PresentationDeckModel(
      title: "테마 및 정렬 검증",
      slides: [
        SlideModel(
          title: "슬라이드 1",
          elements: [el1, el2, el3]
        )
      ]
    )
    deck.applyTheme(.corporateNavy)
    XCTAssertEqual(deck.deckTheme, .corporateNavy)
    XCTAssertEqual(deck.slides[0].elements[0].fillHex, SlideDeckTheme.corporateNavy.primaryFillHex)
    deck.slides[0].alignElements(.distributeHorizontally)
    XCTAssertEqual(deck.slides[0].elements[1].x, 300.0, accuracy: 1e-4)

    // F8: PDFStudioDocumentModel.addBookmark and /Outlines dictionary in PDFBinarySerializer
    var pdf = PDFStudioDocumentModel(
      title: "북마크 PDF",
      pages: [
        PDFPageDescriptor(pageNumber: 1, extractedText: "1장 본문"),
        PDFPageDescriptor(pageNumber: 2, extractedText: "2장 본문"),
      ]
    )
    pdf.addBookmark(title: "1장 개요", targetPageNumber: 1)
    let parentID = try XCTUnwrap(pdf.outline.first?.id)
    pdf.addBookmark(title: "1.1 세부절", targetPageNumber: 2, parentID: parentID)
    XCTAssertEqual(pdf.outline.count, 1)
    XCTAssertEqual(pdf.outline.first?.children.first?.title, "1.1 세부절")

    let pdfBytes = PDFBinarySerializer.encode(pdf)
    let pdfString = String(decoding: pdfBytes, as: UTF8.self)
    XCTAssertTrue(pdfString.contains("/Outlines"))
    let decodedPDF = try PDFBinarySerializer.decode(from: pdfBytes)
    XCTAssertEqual(decodedPDF.outline.first?.title, "1장 개요")
    XCTAssertEqual(decodedPDF.outline.first?.children.first?.title, "1.1 세부절")
  }
}
