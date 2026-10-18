import PDFKit
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

    var sparseSheet = WorksheetModel(name: "희소시트")
    try sparseSheet.setCell("A1", rawValue: "A")
    try sparseSheet.setCell("XFD1048576", rawValue: "A")
    XCTAssertEqual(sparseSheet.removeDuplicateRows(), 0)
    XCTAssertEqual(
      sparseSheet.splitColumnByDelimiter(column: 0, delimiter: ",", maxRow: 999_999), 0)

    // F1 & F12: Dark mode surface tokens and relative luminance helper
    let darkPalette = WorkspaceThemeMode.darkIndigo.surfacePalette(isSystemDark: true)
    XCTAssertEqual(darkPalette.selectedSurface.hex, "#283152")
    XCTAssertEqual(darkPalette.subtleAccentSurface.hex, "#1E243B")
    XCTAssertTrue(darkPalette.isDark)
    XCTAssertTrue(EasyOfficeDesignTokens.isLightFill(hex: "#F8F9FF"))
    XCTAssertFalse(EasyOfficeDesignTokens.isLightFill(hex: "#141724"))

    // F4 & F5: Multilevel list markers and Korean official document list hierarchy (1. -> 가. -> (1) -> (가)) across all 6 codecs
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
    XCTAssertEqual(doc.estimatedPageCount, 1)
    XCTAssertEqual(
      OfficeDocumentModel.resolvePageTokens(
        in: "쪽 {{PAGE}} / {{TOTAL}}", pageNumber: 2, totalPages: 5),
      "쪽 2 / 5"
    )

    let docxRT = try OOXMLContainerCodec.decodeDocument(
      from: try OOXMLContainerCodec.encodeDocument(doc))
    let odtRT = try ODFContainerCodec.decodeDocument(
      from: try ODFContainerCodec.encodeDocument(doc))
    let hwpxRT = try HWPXContainerCodec.decodeDocument(
      from: try HWPXContainerCodec.encodeDocument(doc))
    let hwpRT = try HWP5BinaryCodec.decodeDocument(
      from: try HWP5BinaryCodec.encodeDocument(doc))
    let rtfRT = try RTFCodec.decodeDocument(from: RTFCodec.encodeDocument(doc))
    let mdEncoded1 = CommonMarkCodec.encodeDocument(doc)
    let mdRT = try CommonMarkCodec.decodeDocument(from: mdEncoded1)
    let mdEncoded2 = CommonMarkCodec.encodeDocument(mdRT)
    let mdBody1 =
      String(decoding: mdEncoded1, as: UTF8.self).components(separatedBy: "<!-- eo-meta:").first
      ?? ""
    let mdBody2 =
      String(decoding: mdEncoded2, as: UTF8.self).components(separatedBy: "<!-- eo-meta:").first
      ?? ""
    XCTAssertEqual(mdBody1, mdBody2)
    XCTAssertFalse(mdBody2.contains("가. 가."))

    for roundTripped: OfficeDocumentModel in [docxRT, odtRT, hwpxRT, hwpRT, rtfRT, mdRT] {
      XCTAssertEqual(roundTripped.paragraphs[1].listKind, .koreanOfficial)
      XCTAssertEqual(roundTripped.paragraphs[1].listLevel, 1)
      XCTAssertEqual(roundTripped.paragraphs[1].text, "중항목")
      XCTAssertEqual(roundTripped.paragraphs[1].effectiveRuns.map(\.text).joined(), "중항목")
      XCTAssertEqual(roundTripped.formattedListPrefix(forParagraphAt: 1), "가.")
      XCTAssertEqual(roundTripped.formattedListPrefix(forParagraphAt: 3), "(가)")
    }

    // F7: SlideDeckTheme & SlideModel.alignElements / duplicateElement / removeElement + PPTX/ODP round-trip
    let el1 = SlideCanvasElement(
      title: "Box1", x: 100, y: 100, width: 80, height: 60, kind: .rectangleShape)
    let el2 = SlideCanvasElement(
      title: "Box2", x: 200, y: 140, width: 80, height: 60, kind: .rectangleShape)
    let el3 = SlideCanvasElement(
      title: "Box3", x: 500, y: 180, width: 80, height: 60, kind: .rectangleShape)
    var deck = SlideDeckModel(
      title: "테마 및 정렬 검증",
      slides: [
        SlideModel(
          title: "슬라이드 1",
          elements: [el1, el2, el3]
        )
      ]
    )
    deck.applyTheme(.corporateNavy)
    XCTAssertEqual(deck.theme, .corporateNavy)
    XCTAssertEqual(deck.slides[0].elements[0].fillHex, SlideDeckTheme.corporateNavy.primaryFillHex)
    deck.slides[0].alignElements(.distributeHorizontally)
    XCTAssertEqual(deck.slides[0].elements[1].x, 300.0, accuracy: 1e-4)

    let dup = try XCTUnwrap(deck.slides[0].duplicateElement(id: el1.id))
    XCTAssertEqual(deck.slides[0].elements.count, 4)
    deck.slides[0].removeElement(id: dup.id)
    XCTAssertEqual(deck.slides[0].elements.count, 3)

    let pptxDeck = try OOXMLContainerCodec.decodePresentation(
      from: try OOXMLContainerCodec.encodePresentation(deck))
    let odpDeck = try ODFContainerCodec.decodePresentation(
      from: try ODFContainerCodec.encodePresentation(deck))
    XCTAssertEqual(pptxDeck.deckTheme, .corporateNavy)
    XCTAssertEqual(odpDeck.deckTheme, .corporateNavy)

    // F8: PDFStudioDocumentModel.addBookmark / removeBookmark and /Outlines dictionary in PDFBinarySerializer + PDFKit outlineRoot
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
    XCTAssertTrue(pdfString.contains("/Type /Outlines"))
    XCTAssertTrue(pdfString.contains("/Dest ["))
    let nativePDF = try XCTUnwrap(PDFDocument(data: pdfBytes))
    let outlineRoot = try XCTUnwrap(nativePDF.outlineRoot)
    XCTAssertEqual(outlineRoot.numberOfChildren, 1)
    XCTAssertEqual(outlineRoot.child(at: 0)?.label, "1장 개요")
    XCTAssertEqual(outlineRoot.child(at: 0)?.child(at: 0)?.label, "1.1 세부절")

    let decodedPDF = try PDFBinarySerializer.decode(from: pdfBytes)
    XCTAssertEqual(decodedPDF.outline.first?.title, "1장 개요")
    XCTAssertEqual(decodedPDF.outline.first?.children.first?.title, "1.1 세부절")

    pdf.removeBookmark(id: parentID)
    XCTAssertTrue(pdf.outline.isEmpty)
  }

  // MARK: - 7. 122 Built-In Functions, OFFSET/XLOOKUP/INDEX, Statistical/Financial/Date & CellStyle Decimal Places

  func testSixthAudit122BuiltInFunctionsOffsetXlookupStatisticalFinancialAndDateFunctions() throws {
    XCTAssertEqual(WorksheetModel.supportedFunctionNames.count, 122)
    XCTAssertEqual(
      WorksheetModel.supportedFunctionNames,
      WorksheetModel.supportedFunctionNames.sorted()
    )

    var sheet = WorksheetModel(
      name: "고급수식검증",
      rawCells: [
        // 2x2 block at B2:C3 (10, 20 / 30, 40)
        "B2": "10", "C2": "20",
        "B3": "30", "C3": "40",
        // Lookup table in A5:B9 (with duplicate key "서울" at A5 and A8, and sorted keys in D5:E8)
        "A5": "서울지사", "B5": "100",
        "A6": "부산지사", "B6": "200",
        "A7": "대전본부", "B7": "300",
        "A8": "서울지사", "B8": "400",
        // Numeric lookup table D5:E8
        "D5": "10", "E5": "등급D",
        "D6": "20", "E6": "등급C",
        "D7": "30", "E7": "등급B",
        "D8": "40", "E8": "등급A",
        // Statistical sample X in G1:G4 (1, 2, 3, 4) and Y in H1:H4 (3, 5, 7, 9 -> y = 2x + 1)
        "G1": "1", "G2": "2", "G3": "3", "G4": "4",
        "H1": "3", "H2": "5", "H3": "7", "H4": "9",
        // Cash flows for IRR in J1:J3 (-1000, 600, 600)
        "J1": "-1000", "J2": "600", "J3": "600",
      ]
    )

    // 1. OFFSET scalar, range in SUM, and out-of-bounds #REF!
    try sheet.setCell("F1", rawValue: "=OFFSET(B2, 1, 1)")
    try sheet.setCell("F2", rawValue: "=SUM(OFFSET(B2, 0, 0, 2, 2))")
    try sheet.setCell("F3", rawValue: "=OFFSET(B2, -5, 0)")
    try sheet.setCell("F4", rawValue: "=OFFSET(B2, 70000, 0)")
    try sheet.setCell("F5", rawValue: "=SUM(OFFSET(B2, -2, 0, 2, 2))")

    XCTAssertEqual(sheet.evaluate(at: "F1"), .number(40))
    XCTAssertEqual(sheet.evaluate(at: "F2"), .number(100))
    XCTAssertEqual(sheet.evaluate(at: "F3"), .error("#REF!"))
    XCTAssertEqual(sheet.evaluate(at: "F4"), .error("#REF!"))
    XCTAssertEqual(sheet.evaluate(at: "F5"), .error("#REF!"))

    // 2. XLOOKUP match_mode (-1, 1, 2 wildcard) & search_mode (-1 reverse), plus INDEX 1D row
    try sheet.setCell("F6", rawValue: "=XLOOKUP(25, D5:D8, E5:E8, \"없음\", -1)")
    try sheet.setCell("F7", rawValue: "=XLOOKUP(25, D5:D8, E5:E8, \"없음\", 1)")
    try sheet.setCell("F8", rawValue: "=XLOOKUP(\"*대전*\", A5:A8, B5:B8, \"없음\", 2)")
    try sheet.setCell("F9", rawValue: "=XLOOKUP(\"서울지사\", A5:A8, B5:B8, \"없음\", 0, -1)")
    try sheet.setCell("F10", rawValue: "=INDEX(B2:C2, 2)")
    try sheet.setCell("F11", rawValue: "=INDEX(B2:C2, 0, 2)")

    XCTAssertEqual(sheet.evaluate(at: "F6"), .text("등급C"))
    XCTAssertEqual(sheet.evaluate(at: "F7"), .text("등급B"))
    XCTAssertEqual(sheet.evaluate(at: "F8"), .number(300))
    XCTAssertEqual(sheet.evaluate(at: "F9"), .number(400))
    XCTAssertEqual(sheet.evaluate(at: "F10"), .number(20))
    XCTAssertEqual(sheet.evaluate(at: "F11"), .number(20))

    // 3. Statistical functions (PERCENTILE, QUARTILE, STDEVP, VARP, AVEDEV, GEOMEAN, HARMEAN, CORREL, SLOPE, INTERCEPT, FORECAST)
    try sheet.setCell("K1", rawValue: "=PERCENTILE(B2:C3, 0.5)")
    try sheet.setCell("K2", rawValue: "=QUARTILE(B2:C3, 1)")
    try sheet.setCell("K3", rawValue: "=QUARTILE(B2:C3, 3)")
    try sheet.setCell("K4", rawValue: "=VARP(B2:C3)")
    try sheet.setCell("K5", rawValue: "=STDEVP(B2:C3)")
    try sheet.setCell("K6", rawValue: "=AVEDEV(B2:C3)")
    try sheet.setCell("K7", rawValue: "=GEOMEAN(4, 9)")
    try sheet.setCell("K8", rawValue: "=HARMEAN(2, 6)")
    try sheet.setCell("K9", rawValue: "=CORREL(H1:H4, G1:G4)")
    try sheet.setCell("K10", rawValue: "=SLOPE(H1:H4, G1:G4)")
    try sheet.setCell("K11", rawValue: "=INTERCEPT(H1:H4, G1:G4)")
    try sheet.setCell("K12", rawValue: "=FORECAST(5, H1:H4, G1:G4)")
    try sheet.setCell("K13", rawValue: "=PERCENTILE(B2:C3, 1.5)")

    XCTAssertEqual(sheet.evaluate(at: "K1").numericValue ?? 0, 25.0, accuracy: 1e-6)
    XCTAssertEqual(sheet.evaluate(at: "K2").numericValue ?? 0, 17.5, accuracy: 1e-6)
    XCTAssertEqual(sheet.evaluate(at: "K3").numericValue ?? 0, 32.5, accuracy: 1e-6)
    XCTAssertEqual(sheet.evaluate(at: "K4").numericValue ?? 0, 125.0, accuracy: 1e-6)
    XCTAssertEqual(sheet.evaluate(at: "K5").numericValue ?? 0, sqrt(125.0), accuracy: 1e-6)
    XCTAssertEqual(sheet.evaluate(at: "K6").numericValue ?? 0, 10.0, accuracy: 1e-6)
    XCTAssertEqual(sheet.evaluate(at: "K7").numericValue ?? 0, 6.0, accuracy: 1e-6)
    XCTAssertEqual(sheet.evaluate(at: "K8").numericValue ?? 0, 3.0, accuracy: 1e-6)
    XCTAssertEqual(sheet.evaluate(at: "K9").numericValue ?? 0, 1.0, accuracy: 1e-6)
    XCTAssertEqual(sheet.evaluate(at: "K10").numericValue ?? 0, 2.0, accuracy: 1e-6)
    XCTAssertEqual(sheet.evaluate(at: "K11").numericValue ?? 0, 1.0, accuracy: 1e-6)
    XCTAssertEqual(sheet.evaluate(at: "K12").numericValue ?? 0, 11.0, accuracy: 1e-6)
    XCTAssertEqual(sheet.evaluate(at: "K13"), .error("#VALUE!"))

    // 4. Financial functions (IRR, NPER, RATE, EFFECT, NOMINAL, SLN, SYD)
    try sheet.setCell("L1", rawValue: "=IRR(J1:J3)")
    try sheet.setCell("L2", rawValue: "=NPER(0.05, -129.50457496545668, 1000)")
    try sheet.setCell("L3", rawValue: "=RATE(10, -129.50457496545668, 1000)")
    try sheet.setCell("L4", rawValue: "=EFFECT(0.12, 12)")
    try sheet.setCell("L5", rawValue: "=NOMINAL(L4, 12)")
    try sheet.setCell("L6", rawValue: "=SLN(30000, 7500, 10)")
    try sheet.setCell("L7", rawValue: "=SYD(30000, 7500, 10, 1)")

    // IRR of (-1000, 600, 600): -1000 + 600/(1+r) + 600/(1+r)^2 = 0 -> r ≈ 0.13066238629
    XCTAssertEqual(sheet.evaluate(at: "L1").numericValue ?? 0, 0.130662386, accuracy: 1e-5)
    XCTAssertEqual(sheet.evaluate(at: "L2").numericValue ?? 0, 10.0, accuracy: 1e-5)
    XCTAssertEqual(sheet.evaluate(at: "L3").numericValue ?? 0, 0.05, accuracy: 1e-5)
    XCTAssertEqual(
      sheet.evaluate(at: "L4").numericValue ?? 0,
      pow(1.0 + 0.12 / 12.0, 12.0) - 1.0,
      accuracy: 1e-7
    )
    XCTAssertEqual(sheet.evaluate(at: "L5").numericValue ?? 0, 0.12, accuracy: 1e-6)
    XCTAssertEqual(sheet.evaluate(at: "L6").numericValue ?? 0, 2250.0, accuracy: 1e-6)
    XCTAssertEqual(sheet.evaluate(at: "L7").numericValue ?? 0, 4090.9090909, accuracy: 1e-5)

    // 5. Date & Time functions (WEEKDAY, WEEKNUM, EDATE, EOMONTH, WORKDAY, NETWORKDAYS, DATEDIF)
    try sheet.setCell("M1", rawValue: "=WEEKDAY(\"2026-10-19\", 2)")  // Monday -> 1
    try sheet.setCell("M2", rawValue: "=WEEKDAY(\"2026-10-18\", 1)")  // Sunday -> 1
    try sheet.setCell("M3", rawValue: "=WEEKNUM(\"2026-01-01\")")
    try sheet.setCell("M4", rawValue: "=EDATE(\"2026-01-15\", 2)")
    try sheet.setCell("M5", rawValue: "=EOMONTH(\"2024-01-15\", 1)")  // Leap year Feb 2024 -> 2024-02-29
    try sheet.setCell("M6", rawValue: "=WORKDAY(\"2026-10-16\", 1)")  // Fri + 1 workday -> Mon 2026-10-19
    try sheet.setCell("M7", rawValue: "=NETWORKDAYS(\"2026-10-19\", \"2026-10-25\")")
    try sheet.setCell("M8", rawValue: "=DATEDIF(\"2024-01-15\", \"2026-04-20\", \"Y\")")
    try sheet.setCell("M9", rawValue: "=DATEDIF(\"2026-01-15\", \"2026-04-15\", \"M\")")
    try sheet.setCell("M10", rawValue: "=DATEDIF(\"2026-10-01\", \"2026-10-19\", \"D\")")

    XCTAssertEqual(sheet.evaluate(at: "M1"), .number(1))
    XCTAssertEqual(sheet.evaluate(at: "M2"), .number(1))
    XCTAssertEqual(sheet.evaluate(at: "M3"), .number(1))
    XCTAssertEqual(sheet.evaluate(at: "M4"), .text("2026-03-15"))
    XCTAssertEqual(sheet.evaluate(at: "M5"), .text("2024-02-29"))
    XCTAssertEqual(sheet.evaluate(at: "M6"), .text("2026-10-19"))
    XCTAssertEqual(sheet.evaluate(at: "M7"), .number(5))
    XCTAssertEqual(sheet.evaluate(at: "M8"), .number(2))
    XCTAssertEqual(sheet.evaluate(at: "M9"), .number(3))
    XCTAssertEqual(sheet.evaluate(at: "M10"), .number(18))

    // 6. Math & Text functions (SUMSQ, MROUND, QUOTIENT, REPT, PI)
    try sheet.setCell("N1", rawValue: "=SUMSQ(3, 4)")
    try sheet.setCell("N2", rawValue: "=MROUND(10, 3)")
    try sheet.setCell("N3", rawValue: "=QUOTIENT(17, 5)")
    try sheet.setCell("N4", rawValue: "=QUOTIENT(17, 0)")
    try sheet.setCell("N5", rawValue: "=REPT(\"AB\", 3)")
    try sheet.setCell("N6", rawValue: "=PI()")
    try sheet.setCell("N7", rawValue: "=COMBIN(5, 2)")
    try sheet.setCell("N8", rawValue: "=GCD(24, 36)")
    try sheet.setCell("N9", rawValue: "=LCM(4, 6)")
    try sheet.setCell("N10", rawValue: "=CHAR(65)")
    try sheet.setCell("N11", rawValue: "=CODE(\"A\")")
    try sheet.setCell("N12", rawValue: "=OFFSET(B2, 999999999, 0)")
    try sheet.setCell("N13", rawValue: "=WORKDAY(\"2026-10-16\", 500000)")

    XCTAssertEqual(sheet.evaluate(at: "N1"), .number(25))
    XCTAssertEqual(sheet.evaluate(at: "N2"), .number(9))
    XCTAssertEqual(sheet.evaluate(at: "N3"), .number(3))
    XCTAssertEqual(sheet.evaluate(at: "N4"), .error("#DIV/0!"))
    XCTAssertEqual(sheet.evaluate(at: "N5"), .text("ABABAB"))
    XCTAssertEqual(sheet.evaluate(at: "N6").numericValue ?? 0, Double.pi, accuracy: 1e-9)
    XCTAssertEqual(sheet.evaluate(at: "N7"), .number(10))
    XCTAssertEqual(sheet.evaluate(at: "N8"), .number(12))
    XCTAssertEqual(sheet.evaluate(at: "N9"), .number(12))
    XCTAssertEqual(sheet.evaluate(at: "N10"), .text("A"))
    XCTAssertEqual(sheet.evaluate(at: "N11"), .number(65))
    XCTAssertEqual(sheet.evaluate(at: "N12"), .error("#REF!"))
    XCTAssertEqual(sheet.evaluate(at: "N13"), .error("#NUM!"))

    // 7. CellStyle.wrapText & decimalPlaces formatting and XLSX/ODS round-trip
    let styleDecimal3 = CellStyle(
      numberFormat: .decimal2,
      wrapText: true,
      decimalPlaces: 3
    )
    XCTAssertTrue(styleDecimal3.wrapText)
    XCTAssertEqual(styleDecimal3.decimalPlaces, 3)
    XCTAssertEqual(styleDecimal3.formatNumber(1234.5), "1,234.500")

    let styleClamped = CellStyle(
      numberFormat: .percentage,
      wrapText: false,
      decimalPlaces: 99
    )
    XCTAssertEqual(styleClamped.decimalPlaces, 6)
    XCTAssertEqual(styleClamped.formatNumber(0.125), "12.500000%")

    try sheet.setCell("P1", rawValue: "1234.5")
    try sheet.setCellStyle(styleDecimal3, at: "P1")
    XCTAssertEqual(sheet.formattedDisplayValue(at: try CellAddress(a1Notation: "P1")), "1,234.500")

    let xlsxRoundTrip = try OOXMLContainerCodec.decodeWorksheet(
      from: try OOXMLContainerCodec.encodeWorksheet(sheet))
    let odsRoundTrip = try ODFContainerCodec.decodeWorksheet(
      from: try ODFContainerCodec.encodeWorksheet(sheet))
    XCTAssertTrue(xlsxRoundTrip.cellStyle(at: "P1").wrapText)
    XCTAssertEqual(xlsxRoundTrip.cellStyle(at: "P1").decimalPlaces, 3)
    XCTAssertTrue(odsRoundTrip.cellStyle(at: "P1").wrapText)
    XCTAssertEqual(odsRoundTrip.cellStyle(at: "P1").decimalPlaces, 3)

    // 8. ParagraphBlock letterSpacingPercent & charWidthScalePercent codec round-trip
    let doc = OfficeDocumentModel(
      title: "자간장평검증",
      paragraphs: [
        ParagraphBlock(
          style: .body,
          text: "공문서 자간 및 장평 조판 테스트",
          letterSpacingPercent: -5.0,
          charWidthScalePercent: 95.0
        )
      ]
    )
    XCTAssertEqual(doc.paragraphs[0].letterSpacingPercent, -5.0)
    XCTAssertEqual(doc.paragraphs[0].charWidthScalePercent, 95.0)

    let roundTripDocs: [OfficeDocumentModel] = [
      try OOXMLContainerCodec.decodeDocument(from: try OOXMLContainerCodec.encodeDocument(doc)),
      try ODFContainerCodec.decodeDocument(from: try ODFContainerCodec.encodeDocument(doc)),
      try HWPXContainerCodec.decodeDocument(from: try HWPXContainerCodec.encodeDocument(doc)),
      try HWP5BinaryCodec.decodeDocument(from: try HWP5BinaryCodec.encodeDocument(doc)),
      try RTFCodec.decodeDocument(from: RTFCodec.encodeDocument(doc)),
      try CommonMarkCodec.decodeDocument(from: CommonMarkCodec.encodeDocument(doc)),
    ]
    for decoded in roundTripDocs {
      XCTAssertEqual(decoded.paragraphs.first?.letterSpacingPercent ?? 0.0, -5.0, accuracy: 1e-6)
      XCTAssertEqual(decoded.paragraphs.first?.charWidthScalePercent ?? 100.0, 95.0, accuracy: 1e-6)
    }

    // 9. PDFStudioDocumentModel watermarkText & securityInspectionSummary round-trip + redaction
    var pdf = PDFStudioDocumentModel.fromDocument(doc)
    pdf.applyWatermark("대외비 CONFIDENTIAL")
    _ = pdf.redactText("테스트")
    XCTAssertEqual(pdf.watermarkText, "대외비 CONFIDENTIAL")
    XCTAssertTrue(pdf.securityInspectionSummary.contains("ISO 32000-1 무결성 검증 완료"))
    XCTAssertTrue(pdf.securityInspectionSummary.contains("워터마크(대외비 CONFIDENTIAL)"))
    let pdfBytes = PDFBinarySerializer.encode(pdf)
    let decodedPDF = try PDFBinarySerializer.decode(from: pdfBytes)
    XCTAssertEqual(decodedPDF.watermarkText, "대외비 CONFIDENTIAL")
    _ = pdf.redactText("CONFIDENTIAL")
    XCTAssertEqual(pdf.watermarkText, "대외비 ████")
  }

  // MARK: - 8. Unary +, Postfix %, ISBLANK Spec Compliance & Multi-Column Worksheet Sort

  @MainActor
  func testSeventhAuditFormulaUnaryPlusPostfixPercentIsBlankAndMultiColumnSort() throws {
    var sheet = WorksheetModel(
      name: "수식연산자및정렬검증",
      rawCells: [
        "A1": "100",
        "A2": "25",
        "B1": "=\"\"",
      ]
    )

    // 1. Unary + operator
    try sheet.setCell("C1", rawValue: "=+A1 + +25")
    try sheet.setCell("C2", rawValue: "=+A1+A2")
    try sheet.setCell("C3", rawValue: "=+100")
    try sheet.setCell("C4", rawValue: "=+(A1+A2)")

    XCTAssertEqual(sheet.evaluate(at: "C1").numericValue ?? 0.0, 125.0, accuracy: 1e-9)
    XCTAssertEqual(sheet.evaluate(at: "C2").numericValue ?? 0.0, 125.0, accuracy: 1e-9)
    XCTAssertEqual(sheet.evaluate(at: "C3").numericValue ?? 0.0, 100.0, accuracy: 1e-9)
    XCTAssertEqual(sheet.evaluate(at: "C4").numericValue ?? 0.0, 125.0, accuracy: 1e-9)

    // 2. Postfix % operator
    try sheet.setCell("D1", rawValue: "=15%")
    try sheet.setCell("D2", rawValue: "=200*15%")
    try sheet.setCell("D3", rawValue: "=50%+25%")
    try sheet.setCell("D4", rawValue: "=(80+20)%")
    try sheet.setCell("D5", rawValue: "=(A1+A2)%")

    XCTAssertEqual(sheet.evaluate(at: "D1").numericValue ?? 0.0, 0.15, accuracy: 1e-9)
    XCTAssertEqual(sheet.evaluate(at: "D2").numericValue ?? 0.0, 30.0, accuracy: 1e-9)
    XCTAssertEqual(sheet.evaluate(at: "D3").numericValue ?? 0.0, 0.75, accuracy: 1e-9)
    XCTAssertEqual(sheet.evaluate(at: "D4").numericValue ?? 0.0, 1.0, accuracy: 1e-9)
    XCTAssertEqual(sheet.evaluate(at: "D5").numericValue ?? 0.0, 1.25, accuracy: 1e-9)

    // 3. ISBLANK ECMA-376 / OpenFormula spec compliance (empty cell -> true, formula "" -> false)
    try sheet.setCell("E1", rawValue: "=ISBLANK(Z99)")
    try sheet.setCell("E2", rawValue: "=ISBLANK(B1)")
    try sheet.setCell("E3", rawValue: "=LEN(B1)")

    XCTAssertEqual(sheet.evaluate(at: "E1"), .boolean(true))
    XCTAssertEqual(sheet.evaluate(at: "E2"), .boolean(false))
    XCTAssertEqual(sheet.evaluate(at: "E3"), .number(0.0))

    // 4. Multi-column sortRows(byColumn:ascending:hasHeader:secondaryColumn:secondaryAscending:)
    var sortSheet = WorksheetModel(
      name: "다중열정렬",
      rawCells: [
        "A1": "부서", "B1": "실적", "C1": "담당자",
        "A2": "영업", "B2": "100", "C2": "김민준",
        "A3": "개발", "B3": "250", "C3": "이서연",
        "A4": "영업", "B4": "300", "C4": "박지호",
        "A5": "개발", "B5": "400", "C5": "최유진",
      ]
    )
    sortSheet.sortRows(
      byColumn: 0,
      ascending: true,
      hasHeader: true,
      secondaryColumn: 1,
      secondaryAscending: false
    )

    // Header row 1 preserved
    XCTAssertEqual(sortSheet.rawValue(at: "A1"), "부서")
    XCTAssertEqual(sortSheet.rawValue(at: "B1"), "실적")
    XCTAssertEqual(sortSheet.rawValue(at: "C1"), "담당자")

    // Primary ascending ("개발" before "영업"), secondary descending (400 before 250; 300 before 100)
    XCTAssertEqual(sortSheet.rawValue(at: "A2"), "개발")
    XCTAssertEqual(sortSheet.evaluate(at: "B2"), .number(400))
    XCTAssertEqual(sortSheet.rawValue(at: "C2"), "최유진")

    XCTAssertEqual(sortSheet.rawValue(at: "A3"), "개발")
    XCTAssertEqual(sortSheet.evaluate(at: "B3"), .number(250))
    XCTAssertEqual(sortSheet.rawValue(at: "C3"), "이서연")

    XCTAssertEqual(sortSheet.rawValue(at: "A4"), "영업")
    XCTAssertEqual(sortSheet.evaluate(at: "B4"), .number(300))
    XCTAssertEqual(sortSheet.rawValue(at: "C4"), "박지호")

    XCTAssertEqual(sortSheet.rawValue(at: "A5"), "영업")
    XCTAssertEqual(sortSheet.evaluate(at: "B5"), .number(100))
    XCTAssertEqual(sortSheet.rawValue(at: "C5"), "김민준")

    // 5. SlideModel transitionDurationSeconds & autoAdvanceSeconds + .pptx / .odp round-trip
    var deck = SlideDeckModel(
      title: "전환시간검증",
      slides: [
        SlideModel(
          title: "슬라이드 1",
          transitionDurationSeconds: 1.5,
          autoAdvanceSeconds: 12
        )
      ]
    )
    deck.slides[0].transitionDurationSeconds = 1.5
    deck.slides[0].autoAdvanceSeconds = 12
    let pptxDeck = try OOXMLContainerCodec.decodePresentation(
      from: try OOXMLContainerCodec.encodePresentation(deck)
    )
    let odpDeck = try ODFContainerCodec.decodePresentation(
      from: try ODFContainerCodec.encodePresentation(deck)
    )
    XCTAssertEqual(pptxDeck.slides[0].transitionDurationSeconds, 1.5, accuracy: 1e-6)
    XCTAssertEqual(pptxDeck.slides[0].autoAdvanceSeconds, 12)
    XCTAssertEqual(odpDeck.slides[0].transitionDurationSeconds, 1.5, accuracy: 1e-6)
    XCTAssertEqual(odpDeck.slides[0].autoAdvanceSeconds, 12)

    // 6. Interactive Find & Replace navigation (findMatchCount, findNextMatch, findPreviousMatch, replaceCurrentMatch)
    let vm = WorkspaceViewModel()
    let docTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .document }))
    vm.activeTabID = docTab.id
    vm.appendParagraphToActiveDocument(text: "첫 번째 검색대상키워드 문단입니다.", style: .body)
    vm.appendParagraphToActiveDocument(text: "두 번째 검색대상키워드 문단입니다.", style: .body)
    vm.findQueryDraft = "검색대상키워드"
    vm.replaceQueryDraft = "치환완료키워드"
    let initialMatches = vm.findMatchCount
    XCTAssertEqual(initialMatches, 2)
    vm.findNextMatch()
    XCTAssertEqual(vm.currentFindMatchIndex, 1)
    vm.findPreviousMatch()
    XCTAssertEqual(vm.currentFindMatchIndex, 2)
    let replacedSingle = vm.replaceCurrentMatch()
    XCTAssertEqual(replacedSingle, 1)
    XCTAssertEqual(vm.recomputeFindMatches(), 1)

    // F1 regression: replacing the 2nd match inside a single paragraph with multiple inlineRuns
    if case .document(var docWithRuns) = vm.activeTab?.payload,
      let tabIdx = vm.openTabs.firstIndex(where: { $0.id == docTab.id })
    {
      docWithRuns.paragraphs.append(
        ParagraphBlock(
          style: .body,
          text: "런토큰 알파 런토큰 베타",
          runs: [
            DocumentTextRun(text: "런토큰 알파 "),
            DocumentTextRun(text: "런토큰 베타"),
          ]
        )
      )
      vm.openTabs[tabIdx].payload = .document(docWithRuns)
      vm.findQueryDraft = "런토큰"
      vm.replaceQueryDraft = "수정토큰"
      XCTAssertEqual(vm.recomputeFindMatches(), 2)
      vm.findNextMatch()  // 1
      vm.findNextMatch()  // 2
      XCTAssertEqual(vm.currentFindMatchIndex, 2)
      XCTAssertEqual(vm.replaceCurrentMatch(), 1)
      if case .document(let afterRunReplace) = vm.activeTab?.payload,
        let lastPara = afterRunReplace.paragraphs.last
      {
        XCTAssertEqual(lastPara.text, "런토큰 알파 수정토큰 베타")
        XCTAssertEqual(lastPara.inlineRuns[0].text, "런토큰 알파 ")
        XCTAssertEqual(lastPara.inlineRuns[1].text, "수정토큰 베타")
      }
    }

    // F2 regression: sortRows(primaryColumn:...) and sortActiveSheetBySecondaryColumn
    sortSheet.sortRows(
      primaryColumn: 0,
      ascending: true,
      secondaryColumn: 1,
      secondaryAscending: true,
      hasHeader: true
    )
    XCTAssertEqual(sortSheet.evaluate(at: "B2"), .number(250))
    let sheetTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .sheet }))
    vm.activeTabID = sheetTab.id
    vm.sortActiveSheetBySecondaryColumn(
      primaryColumn: 0,
      ascending: true,
      secondaryColumn: 1,
      secondaryAscending: false
    )

    // 7. PDFBinarySerializer upper/middle page layout (OFFICIAL STAMP, AcroForm, ELECTRONIC SIGNATURE)
    let pdfTab = try XCTUnwrap(vm.openTabs.first(where: { $0.moduleKind == .pdf }))
    vm.activeTabID = pdfTab.id
    vm.addStampToActivePDFPage(preset: "승인 (APPROVED)")
    vm.addInkStrokeToActivePDFPage(
      points: [
        PDFInkPoint(x: 90, y: 468),
        PDFInkPoint(x: 150, y: 478),
        PDFInkPoint(x: 220, y: 464),
      ]
    )
    if case .pdf(let pdfModel) = vm.activeTab?.payload {
      let encodedPDF = PDFBinarySerializer.encode(pdfModel)
      let pdfASCII = String(decoding: encodedPDF, as: UTF8.self)
      XCTAssertTrue(pdfASCII.contains("386 576 160 46 re B"))
      // F3 regression: signature box rendered only on the signed page (1 time, not on unsigned pages)
      let sigBoxOccurrences = pdfASCII.components(separatedBy: "48 452 280 46 re B").count - 1
      XCTAssertEqual(sigBoxOccurrences, 1)
      let roundTripPDF = try PDFBinarySerializer.decode(from: encodedPDF)
      XCTAssertEqual(roundTripPDF.annotations.count, pdfModel.annotations.count)
      XCTAssertEqual(roundTripPDF.totalInkStrokeCount, pdfModel.totalInkStrokeCount)
    } else {
      XCTFail("Expected active PDF tab")
    }
  }
}
