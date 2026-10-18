import XCTest
@testable import EasyOfficeDesignSystem
@testable import EasyOfficeCore

final class EditorEnginesParityTests: XCTestCase {

    // MARK: - 1. PackedCellCoordinate Bit-Level Optimization Tests

    func testPackedCellCoordinateBitLayoutAndRoundTrip() throws {
        let coord = PackedCellCoordinate(row: 1048575, column: 16383)
        XCTAssertEqual(coord.row, 1048575)
        XCTAssertEqual(coord.column, 16383)

        let expectedRaw = (UInt64(1048575) << 32) | UInt64(16383)
        XCTAssertEqual(coord.rawValue, expectedRaw)

        let addr = try CellAddress(a1Notation: "AB128")
        let packed = addr.packedCoordinate
        XCTAssertEqual(packed.row, 127)
        XCTAssertEqual(packed.column, 27)

        let roundTripped = CellAddress(packed: packed)
        XCTAssertEqual(roundTripped, addr)
        XCTAssertEqual(roundTripped.a1Notation, "AB128")

        // Row-major ordering via Comparable on UInt64
        let c1 = PackedCellCoordinate(row: 0, column: 5)
        let c2 = PackedCellCoordinate(row: 1, column: 0)
        let c3 = PackedCellCoordinate(row: 1, column: 10)
        XCTAssertLessThan(c1, c2)
        XCTAssertLessThan(c2, c3)

        // Fail-Closed ECMA-376 bounds and range expansion ceiling
        XCTAssertThrowsError(try CellAddress(a1Notation: "XFE1"))
        XCTAssertThrowsError(try CellAddress(a1Notation: "A1048577"))
        XCTAssertThrowsError(try CellRange(notation: "A1:Z10000"))
    }

    // MARK: - 2. Spreadsheet 50+ Functions Parity Tests

    func testSpreadsheet50PlusFunctionsCatalogAndEvaluation() throws {
        XCTAssertGreaterThanOrEqual(WorksheetModel.supportedFunctionNames.count, 55)
        XCTAssertEqual(WorksheetModel.supportedFunctionNames, WorksheetModel.supportedFunctionNames.sorted())

        var sheet = WorksheetModel(name: "수식엔진검증")
        // Numeric dataset in A1:A6
        try sheet.setCell("A1", rawValue: "10")
        try sheet.setCell("A2", rawValue: "20")
        try sheet.setCell("A3", rawValue: "20")
        try sheet.setCell("A4", rawValue: "40")
        try sheet.setCell("A5", rawValue: "50")
        // A6 left blank intentionally for COUNTBLANK

        // Paired weights in B1:B5
        try sheet.setCell("B1", rawValue: "1")
        try sheet.setCell("B2", rawValue: "2")
        try sheet.setCell("B3", rawValue: "3")
        try sheet.setCell("B4", rawValue: "4")
        try sheet.setCell("B5", rawValue: "5")

        // Category labels in C1:C5
        try sheet.setCell("C1", rawValue: "서울")
        try sheet.setCell("C2", rawValue: "부산")
        try sheet.setCell("C3", rawValue: "서울")
        try sheet.setCell("C4", rawValue: "대전")
        try sheet.setCell("C5", rawValue: "서울")

        // Math & Trig
        try sheet.setCell("D1", rawValue: "=COUNTBLANK(A1:A6)")
        try sheet.setCell("D2", rawValue: "=ROUNDUP(3.14159, 2)")
        try sheet.setCell("D3", rawValue: "=ROUNDDOWN(3.987, 2)")
        try sheet.setCell("D4", rawValue: "=INT(7.8)")
        try sheet.setCell("D5", rawValue: "=MOD(17, 5)")
        try sheet.setCell("D6", rawValue: "=SQRT(81)")
        try sheet.setCell("D7", rawValue: "=POWER(2, 10)")
        try sheet.setCell("D8", rawValue: "=PRODUCT(B1:B4)")
        try sheet.setCell("D9", rawValue: "=SUMPRODUCT(A1:A5, B1:B5)")
        try sheet.setCell("D10", rawValue: "=SUMIF(C1:C5, \"서울\", A1:A5)")
        try sheet.setCell("D11", rawValue: "=COUNTIF(C1:C5, \"서울\")")
        try sheet.setCell("D12", rawValue: "=AVERAGEIF(A1:A5, \">=20\")")
        try sheet.setCell("D13", rawValue: "=CEILING(23, 5)")
        try sheet.setCell("D14", rawValue: "=FLOOR(23, 5)")
        try sheet.setCell("D15", rawValue: "=SIGN(-42)")
        try sheet.setCell("D16", rawValue: "=TRUNC(9.99, 1)")
        try sheet.setCell("D17", rawValue: "=LOG10(1000)")
        try sheet.setCell("D18", rawValue: "=ROUND(LN(EXP(4)), 4)")

        XCTAssertEqual(sheet.evaluate(at: "D1"), .number(1))
        XCTAssertEqual(sheet.evaluate(at: "D2"), .number(3.15))
        XCTAssertEqual(sheet.evaluate(at: "D3"), .number(3.98))
        XCTAssertEqual(sheet.evaluate(at: "D4"), .number(7))
        XCTAssertEqual(sheet.evaluate(at: "D5"), .number(2))
        XCTAssertEqual(sheet.evaluate(at: "D6"), .number(9))
        XCTAssertEqual(sheet.evaluate(at: "D7"), .number(1024))
        XCTAssertEqual(sheet.evaluate(at: "D8"), .number(24))
        // 10*1 + 20*2 + 20*3 + 40*4 + 50*5 = 10 + 40 + 60 + 160 + 250 = 520
        XCTAssertEqual(sheet.evaluate(at: "D9"), .number(520))
        // 서울 rows: A1(10) + A3(20) + A5(50) = 80
        XCTAssertEqual(sheet.evaluate(at: "D10"), .number(80))
        XCTAssertEqual(sheet.evaluate(at: "D11"), .number(3))
        // >=20 in A1:A5: 20, 20, 40, 50 -> avg = 32.5
        XCTAssertEqual(sheet.evaluate(at: "D12"), .number(32.5))
        XCTAssertEqual(sheet.evaluate(at: "D13"), .number(25))
        XCTAssertEqual(sheet.evaluate(at: "D14"), .number(20))
        XCTAssertEqual(sheet.evaluate(at: "D15"), .number(-1))
        XCTAssertEqual(sheet.evaluate(at: "D16"), .number(9.9))
        XCTAssertEqual(sheet.evaluate(at: "D17"), .number(3))
        XCTAssertEqual(sheet.evaluate(at: "D18"), .number(4))

        // Statistical
        try sheet.setCell("E1", rawValue: "=MEDIAN(A1:A5)")
        try sheet.setCell("E2", rawValue: "=MODE(A1:A5)")
        try sheet.setCell("E3", rawValue: "=LARGE(A1:A5, 2)")
        try sheet.setCell("E4", rawValue: "=SMALL(A1:A5, 2)")
        try sheet.setCell("E5", rawValue: "=RANK(40, A1:A5)")
        try sheet.setCell("E6", rawValue: "=ROUND(VAR(A1:A5), 2)")
        try sheet.setCell("E7", rawValue: "=ROUND(STDEV(A1:A5), 2)")
        try sheet.setCell("E8", rawValue: "=MODE(B1:B5)")

        XCTAssertEqual(sheet.evaluate(at: "E1"), .number(20))
        XCTAssertEqual(sheet.evaluate(at: "E2"), .number(20))
        XCTAssertEqual(sheet.evaluate(at: "E3"), .number(40))
        XCTAssertEqual(sheet.evaluate(at: "E4"), .number(20))
        XCTAssertEqual(sheet.evaluate(at: "E5"), .number(2))
        // A1:A5 = [10, 20, 20, 40, 50], mean = 28, sample var = ((324+64+64+144+484)/4) = 1080/4 = 270
        XCTAssertEqual(sheet.evaluate(at: "E6"), .number(270))
        XCTAssertEqual(sheet.evaluate(at: "E7"), .number(16.43))
        // B1:B5 has all distinct values [1, 2, 3, 4, 5] -> MODE must return #N/A (F1)
        XCTAssertEqual(sheet.evaluate(at: "E8"), .error("#N/A"))

        // Logical
        try sheet.setCell("F1", rawValue: "=AND(A1=10, A5=50)")
        try sheet.setCell("F2", rawValue: "=OR(A1=99, A5=50)")
        try sheet.setCell("F3", rawValue: "=NOT(A1=99)")
        try sheet.setCell("F4", rawValue: "=XOR(TRUE, FALSE)")
        try sheet.setCell("F5", rawValue: "=IFERROR(A1/0, 999)")
        try sheet.setCell("F6", rawValue: "=IFS(A5>100, \"S\", A5>=50, \"A\", TRUE, \"B\")")
        try sheet.setCell("F7", rawValue: "=SWITCH(C2, \"서울\", 100, \"부산\", 200, 0)")

        XCTAssertEqual(sheet.evaluate(at: "F1"), .boolean(true))
        XCTAssertEqual(sheet.evaluate(at: "F2"), .boolean(true))
        XCTAssertEqual(sheet.evaluate(at: "F3"), .boolean(true))
        XCTAssertEqual(sheet.evaluate(at: "F4"), .boolean(true))
        XCTAssertEqual(sheet.evaluate(at: "F5"), .number(999))
        XCTAssertEqual(sheet.evaluate(at: "F6"), .text("A"))
        XCTAssertEqual(sheet.evaluate(at: "F7"), .number(200))

        // Lookup & Reference
        try sheet.setCell("G1", rawValue: "=VLOOKUP(\"대전\", C1:A5, 1, FALSE)")
        // Setup lookup table in H1:I3
        try sheet.setCell("H1", rawValue: "P-100")
        try sheet.setCell("I1", rawValue: "1500")
        try sheet.setCell("H2", rawValue: "P-200")
        try sheet.setCell("I2", rawValue: "2800")
        try sheet.setCell("H3", rawValue: "P-300")
        try sheet.setCell("I3", rawValue: "4200")

        try sheet.setCell("G2", rawValue: "=VLOOKUP(\"P-200\", H1:I3, 2, FALSE)")
        try sheet.setCell("G3", rawValue: "=XLOOKUP(\"P-300\", H1:H3, I1:I3, 0)")
        try sheet.setCell("G4", rawValue: "=INDEX(H1:I3, 2, 2)")
        try sheet.setCell("G5", rawValue: "=MATCH(\"P-200\", H1:H3, 0)")
        try sheet.setCell("G6", rawValue: "=CHOOSE(2, \"일\", \"이\", \"삼\")")
        try sheet.setCell("G7", rawValue: "=ROW(H3)")
        try sheet.setCell("G8", rawValue: "=COLUMN(I3)")
        // Horizontal lookup table in K1:M2
        try sheet.setCell("K1", rawValue: "Q1")
        try sheet.setCell("L1", rawValue: "Q2")
        try sheet.setCell("M1", rawValue: "Q3")
        try sheet.setCell("K2", rawValue: "110")
        try sheet.setCell("L2", rawValue: "220")
        try sheet.setCell("M2", rawValue: "330")
        try sheet.setCell("G9", rawValue: "=HLOOKUP(\"Q2\", K1:M2, 2, FALSE)")

        XCTAssertEqual(sheet.evaluate(at: "G1"), .error("#N/A"))
        XCTAssertEqual(sheet.evaluate(at: "G2"), .number(2800))
        XCTAssertEqual(sheet.evaluate(at: "G3"), .number(4200))
        XCTAssertEqual(sheet.evaluate(at: "G4"), .number(2800))
        XCTAssertEqual(sheet.evaluate(at: "G5"), .number(2))
        XCTAssertEqual(sheet.evaluate(at: "G6"), .text("이"))
        XCTAssertEqual(sheet.evaluate(at: "G7"), .number(3))
        XCTAssertEqual(sheet.evaluate(at: "G8"), .number(9))
        XCTAssertEqual(sheet.evaluate(at: "G9"), .number(220))

        // Text Functions & Resource Bounds (F2, F3)
        try sheet.setCell("N1", rawValue: "=LEFT(\"EasyOffice\", 4)")
        try sheet.setCell("N2", rawValue: "=RIGHT(\"EasyOffice\", 6)")
        try sheet.setCell("N3", rawValue: "=MID(\"EasyOffice\", 5, 3)")
        try sheet.setCell("N4", rawValue: "=LEN(\"한글문서\")")
        try sheet.setCell("N5", rawValue: "=LOWER(\" MACOS \")")
        try sheet.setCell("N6", rawValue: "=UPPER(\"macos\")")
        try sheet.setCell("N7", rawValue: "=PROPER(\"easy office suite\")")
        try sheet.setCell("N8", rawValue: "=TRIM(\"  다중   공백   제거  \")")
        try sheet.setCell("N9", rawValue: "=SUBSTITUTE(\"2025-10-17\", \"2025\", \"2026\")")
        try sheet.setCell("N10", rawValue: "=FIND(\"Office\", \"EasyOffice\")")
        try sheet.setCell("N11", rawValue: "=REPT(\"★\", 3)")
        try sheet.setCell("N12", rawValue: "=EXACT(\"ABC\", \"ABC\")")
        try sheet.setCell("N13", rawValue: "=VALUE(\"1,250.5\")")
        try sheet.setCell("N14", rawValue: "=TEXT(12.3, \"0.00\")")
        try sheet.setCell("N15", rawValue: "=CONCATENATE(\"A\", \"-\", \"B\")")
        try sheet.setCell("N16", rawValue: "=REPT(\"ABCD\", 100000)")
        try sheet.setCell("N17", rawValue: "=LEFT(A1/0, 2)")
        try sheet.setCell("N18", rawValue: "=POWER(10, 1000)")

        XCTAssertEqual(sheet.evaluate(at: "N1"), .text("Easy"))
        XCTAssertEqual(sheet.evaluate(at: "N2"), .text("Office"))
        XCTAssertEqual(sheet.evaluate(at: "N3"), .text("Off"))
        XCTAssertEqual(sheet.evaluate(at: "N4"), .number(4))
        XCTAssertEqual(sheet.evaluate(at: "N5"), .text(" macos "))
        XCTAssertEqual(sheet.evaluate(at: "N6"), .text("MACOS"))
        XCTAssertEqual(sheet.evaluate(at: "N7"), .text("Easy Office Suite"))
        XCTAssertEqual(sheet.evaluate(at: "N8"), .text("다중 공백 제거"))
        XCTAssertEqual(sheet.evaluate(at: "N9"), .text("2026-10-17"))
        XCTAssertEqual(sheet.evaluate(at: "N10"), .number(5))
        XCTAssertEqual(sheet.evaluate(at: "N11"), .text("★★★"))
        XCTAssertEqual(sheet.evaluate(at: "N12"), .boolean(true))
        XCTAssertEqual(sheet.evaluate(at: "N13"), .number(1250.5))
        XCTAssertEqual(sheet.evaluate(at: "N14"), .text("12.30"))
        XCTAssertEqual(sheet.evaluate(at: "N15"), .text("A-B"))
        XCTAssertEqual(sheet.evaluate(at: "N16"), .error("#VALUE!"))
        XCTAssertEqual(sheet.evaluate(at: "N17"), .error("#DIV/0!"))
        XCTAssertEqual(sheet.evaluate(at: "N18"), .error("#NUM!"))

        // Financial & Date Functions
        try sheet.setCell("P1", rawValue: "=ROUND(PMT(0.05, 10, -1000), 2)")
        try sheet.setCell("P2", rawValue: "=ROUND(FV(0.05, 10, -100, -1000), 2)")
        try sheet.setCell("P3", rawValue: "=ROUND(PV(0.05, 10, -129.5046), 0)")
        try sheet.setCell("P4", rawValue: "=ROUND(NPV(0.1, 100, 110, 121), 2)")
        try sheet.setCell("P5", rawValue: "=DATE(2026, 10, 17)")
        try sheet.setCell("P6", rawValue: "=YEAR(P5)")
        try sheet.setCell("P7", rawValue: "=MONTH(P5)")
        try sheet.setCell("P8", rawValue: "=DAY(P5)")
        try sheet.setCell("P9", rawValue: "=DAYS(\"2026-10-17\", \"2026-10-01\")")

        XCTAssertEqual(sheet.evaluate(at: "P1"), .number(129.5))
        XCTAssertEqual(sheet.evaluate(at: "P2"), .number(2886.68))
        XCTAssertEqual(sheet.evaluate(at: "P3"), .number(1000))
        // 100/1.1 + 110/1.21 + 121/1.331 = 90.909 + 90.909 + 90.909 = 272.73
        XCTAssertEqual(sheet.evaluate(at: "P4"), .number(272.73))
        XCTAssertEqual(sheet.evaluate(at: "P5"), .text("2026-10-17"))
        XCTAssertEqual(sheet.evaluate(at: "P6"), .number(2026))
        XCTAssertEqual(sheet.evaluate(at: "P7"), .number(10))
        XCTAssertEqual(sheet.evaluate(at: "P8"), .number(17))
        XCTAssertEqual(sheet.evaluate(at: "P9"), .number(16))

        // Direct evaluation via PackedCellCoordinate
        let p5Coord = try CellAddress(a1Notation: "P5").packedCoordinate
        XCTAssertEqual(sheet.evaluate(at: p5Coord), .text("2026-10-17"))
    }

    // MARK: - 3. Pivot Table, Chart Builder, Conditional Format, Validation, Sort/Filter

    func testPivotTableChartConditionalFormatAndSortFilter() throws {
        var sheet = WorksheetModel(name: "피벗데이터")
        // Header: 지역(A), 분기(B), 매출(C)
        try sheet.setCell("A1", rawValue: "지역")
        try sheet.setCell("B1", rawValue: "분기")
        try sheet.setCell("C1", rawValue: "매출")

        try sheet.setCell("A2", rawValue: "서울")
        try sheet.setCell("B2", rawValue: "1Q")
        try sheet.setCell("C2", rawValue: "100")

        try sheet.setCell("A3", rawValue: "서울")
        try sheet.setCell("B3", rawValue: "2Q")
        try sheet.setCell("C3", rawValue: "150")

        try sheet.setCell("A4", rawValue: "부산")
        try sheet.setCell("B4", rawValue: "1Q")
        try sheet.setCell("C4", rawValue: "80")

        try sheet.setCell("A5", rawValue: "부산")
        try sheet.setCell("B5", rawValue: "2Q")
        try sheet.setCell("C5", rawValue: "120")

        let dataRange = try CellRange(notation: "A1:C5")
        let pivotConfig = PivotTableConfiguration(
            rowFieldColumnIndex: 0,
            columnFieldColumnIndex: 1,
            valueFieldColumnIndex: 2,
            aggregation: .sum
        )
        let summary = PivotTableEngine.compute(
            from: sheet,
            range: dataRange,
            hasHeaderRow: true,
            configuration: pivotConfig
        )

        XCTAssertEqual(summary.rowKeys, ["서울", "부산"])
        XCTAssertEqual(summary.columnKeys, ["1Q", "2Q"])
        XCTAssertEqual(summary.matrix, [[100, 150], [80, 120]])
        XCTAssertEqual(summary.rowTotals, [250, 200])
        XCTAssertEqual(summary.columnTotals, [180, 270])
        XCTAssertEqual(summary.grandTotal, 450)

        // Chart Builder
        let catRange = try CellRange(notation: "A2:A5")
        let valRange = try CellRange(notation: "C2:C5")
        let chart = SheetChartBuilder.buildChart(
            from: sheet,
            title: "지역별 매출 차트",
            kind: .column,
            categoryRange: catRange,
            valueRanges: [(name: "매출액", range: valRange)]
        )
        XCTAssertEqual(chart.title, "지역별 매출 차트")
        XCTAssertEqual(chart.kind, .column)
        XCTAssertEqual(chart.categories.count, 4)
        XCTAssertEqual(chart.series.first?.values, [100, 150, 80, 120])

        // Conditional formatting & Data Validation
        let allVals: [EvaluatedCellValue] = [.number(100), .number(150), .number(100)]
        XCTAssertTrue(ConditionalFormatRule.greaterThan(120).matches(.number(150), allValuesInRange: allVals))
        XCTAssertTrue(ConditionalFormatRule.between(90, 110).matches(.number(100), allValuesInRange: allVals))
        XCTAssertTrue(ConditionalFormatRule.duplicateValues.matches(.number(100), allValuesInRange: allVals))
        XCTAssertFalse(ConditionalFormatRule.duplicateValues.matches(.number(150), allValuesInRange: allVals))

        XCTAssertTrue(DataValidationRule.numberRange(min: 0, max: 200).validate(.number(150)))
        XCTAssertFalse(DataValidationRule.numberRange(min: 0, max: 200).validate(.number(250)))
        XCTAssertTrue(DataValidationRule.allowedList(["서울", "부산"]).validate(.text("서울")))
        XCTAssertFalse(DataValidationRule.allowedList(["서울", "부산"]).validate(.text("제주")))

        // Sort & Filter on rows A2:C5
        let rowsOnlyRange = try CellRange(notation: "A2:C5")
        let sortedDesc = sheet.sortedRows(in: rowsOnlyRange, byColumnOffset: 2, ascending: false)
        XCTAssertEqual(sortedDesc.map { $0[2] }, [.number(150), .number(120), .number(100), .number(80)])

        let filtered = sheet.filteredRows(in: rowsOnlyRange, byColumnOffset: 0) { $0 == .text("서울") }
        XCTAssertEqual(filtered.count, 2)
    }

    // MARK: - 4. Document & HWP Engine Parity Tests

    func testDocumentSectionsTableFormulasTrackChangesAndTOC() {
        let p1 = ParagraphBlock(style: .title, text: "2026 기술 백서")
        let p2 = ParagraphBlock(style: .heading1, text: "1. 개요")
        let p3 = ParagraphBlock(style: .heading2, text: "1.1 배경")
        let p4 = ParagraphBlock(style: .body, text: "초기 문단 텍스트입니다. 한글 포맷과 워드 포맷을 모두 지원합니다.")

        var table = DocumentTableBlock(
            caption: "예산 집행표",
            rows: [
                ["항목", "1분기", "2분기", "합계"],
                ["인건비", "120", "140", "=SUM(LEFT)"],
                ["장비비", "80", "60", "=SUM(LEFT)"],
                ["소계", "=SUM(ABOVE)", "=AVERAGE(ABOVE)", ""]
            ],
            borderStyle: .doubleLine
        )

        // Out-of-bounds merge should be rejected (F4)
        table.mergeCells(startRow: 10, startCol: 0, rowSpan: 2, colSpan: 2)
        XCTAssertTrue(table.cellSpans.isEmpty)

        table.mergeCells(startRow: 0, startCol: 0, rowSpan: 1, colSpan: 2)
        XCTAssertEqual(table.cellSpans.count, 1)
        XCTAssertEqual(table.cellSpans.first?.columnSpan, 2)

        table.splitCell(row: 0, col: 0)
        XCTAssertTrue(table.cellSpans.isEmpty)

        // Evaluate table formulas
        XCTAssertEqual(table.evaluateTableFormula("=SUM(LEFT)", atRow: 1, column: 3), 260.0)
        XCTAssertEqual(table.evaluateTableFormula("=SUM(LEFT)", atRow: 2, column: 3), 140.0)
        XCTAssertEqual(table.evaluateTableFormula("=SUM(ABOVE)", atRow: 3, column: 1), 200.0)
        XCTAssertEqual(table.evaluateTableFormula("=AVERAGE(ABOVE)", atRow: 3, column: 2), 100.0)
        XCTAssertEqual(table.evaluateTableFormula("=MAX(ABOVE)", atRow: 3, column: 1), 120.0)
        XCTAssertEqual(table.evaluateTableFormula("=MIN(ABOVE)", atRow: 3, column: 1), 80.0)
        XCTAssertEqual(table.evaluateTableFormula("=COUNT(ABOVE)", atRow: 3, column: 1), 2.0)

        var doc = OfficeDocumentModel(
            title: "백서",
            format: .hwpx,
            paragraphs: [p1, p2, p3, p4],
            tables: [table],
            trackChangesEnabled: true,
            sections: [
                DocumentSectionLayout(paperSize: .a4, orientation: .portrait, columnCount: 2)
            ],
            footnotes: [
                FootnoteItem(number: 1, isEndnote: false, anchorParagraphID: p4.id, text: "KS X 6101 OWPML 국가 표준 참조")
            ],
            comments: [
                DocumentCommentThread(anchorParagraphID: p4.id, author: "QA", message: "문구 검토 완료")
            ],
            bookmarks: ["intro": p2.id]
        )

        // TOC generation
        let toc = doc.generateTableOfContents()
        XCTAssertEqual(toc.count, 3)
        XCTAssertEqual(toc[0].level, 1)
        XCTAssertEqual(toc[1].level, 1)
        XCTAssertEqual(toc[2].level, 2)

        // Track changes: record -> accept / reject
        doc.recordRevision(
            author: "검토자",
            kind: .insertion,
            paragraphID: p4.id,
            proposedText: "개정된 문단 텍스트입니다. 양방향 변환을 지원합니다."
        )
        XCTAssertEqual(doc.revisions.count, 1)
        let revID = doc.revisions[0].id
        doc.acceptRevision(id: revID)
        XCTAssertTrue(doc.revisions.isEmpty)
        XCTAssertEqual(doc.paragraphs[3].text, "개정된 문단 텍스트입니다. 양방향 변환을 지원합니다.")

        // Record another revision and reject it
        doc.recordRevision(
            author: "검토자",
            kind: .insertion,
            paragraphID: p4.id,
            proposedText: "임시 변경안"
        )
        let rejectID = doc.revisions[0].id
        doc.rejectRevision(id: rejectID)
        XCTAssertTrue(doc.revisions.isEmpty)
        XCTAssertEqual(doc.paragraphs[3].text, "개정된 문단 텍스트입니다. 양방향 변환을 지원합니다.")

        // Find and Replace
        let (replacedDoc, count) = doc.findAndReplace(search: "변환", replacement: "호환")
        XCTAssertEqual(count, 1)
        XCTAssertTrue(replacedDoc.paragraphs[3].text.contains("양방향 호환을 지원합니다."))
    }

    // MARK: - 5. Slide Engine Z-Order, Smart Snap Guides, and Presenter Session

    func testSlideZOrderSnapGuidesAndPresenterSession() {
        let el1 = SlideCanvasElement(title: "배경 박스", x: 100, y: 100, width: 100, height: 80, kind: .rectangleShape, zIndex: 1)
        let el2 = SlideCanvasElement(title: "중간 박스", x: 250, y: 100, width: 100, height: 80, kind: .roundedRectangleShape, zIndex: 2)
        // Moving element placed at x=400 (so gap between el1(100..200) and el2(250..350) is 50, and gap between el2(250..350) and el3(400..500) is 50 -> equal-spacing!)
        // Also y=100 matches el1.y and el2.y -> element-edge, while Y gaps are -80 (overlapping on Y), so horizontal equal-spacing must NOT fire (F5)!
        let el3 = SlideCanvasElement(title: "이동 박스", x: 400, y: 100, width: 100, height: 80, kind: .calloutShape, zIndex: 3)

        let guides = SnapGuideCalculator.computeGuides(
            for: el3,
            siblings: [el1, el2],
            aspectRatio: .widescreen16x9,
            tolerancePoints: 4.0
        )
        XCTAssertTrue(guides.contains { $0.reason == "element-edge" && $0.axis == .horizontal })
        XCTAssertTrue(guides.contains { $0.reason == "equal-spacing" && $0.axis == .vertical })
        XCTAssertFalse(guides.contains { $0.reason == "equal-spacing" && $0.axis == .horizontal })

        let centeredEl = SlideCanvasElement(title: "정중앙", x: 380, y: 220, width: 200, height: 100)
        let centerGuides = SnapGuideCalculator.computeGuides(
            for: centeredEl,
            siblings: [],
            aspectRatio: .widescreen16x9,
            tolerancePoints: 4.0
        )
        XCTAssertTrue(centerGuides.contains { $0.reason == "stage-center-x" })
        XCTAssertTrue(centerGuides.contains { $0.reason == "stage-center-y" })

        var slide = SlideModel(title: "레이어 테스트", elements: [el1, el2, el3])
        slide.bringElementToFront(id: el1.id)
        XCTAssertEqual(slide.elements.last?.id, el1.id)
        XCTAssertGreaterThan(slide.elements.last?.zIndex ?? 0, el3.zIndex)

        slide.sendElementToBack(id: el3.id)
        XCTAssertEqual(slide.elements.first?.id, el3.id)

        let deck = PresentationDeckModel(
            title: "발표 세션 덱",
            slides: [
                SlideModel(title: "1장", speakerNotes: "오프닝 멘트"),
                SlideModel(title: "2장", speakerNotes: "아키텍처 상세"),
                SlideModel(title: "3장", speakerNotes: "질의응답")
            ]
        )
        var session = PresenterSession(deck: deck)
        XCTAssertEqual(session.currentSlide?.title, "1장")
        XCTAssertEqual(session.nextSlide?.title, "2장")
        XCTAssertEqual(session.currentSpeakerNotes, "오프닝 멘트")
        XCTAssertEqual(session.progressFraction, 1.0 / 3.0, accuracy: 0.001)

        session.next()
        session.tick(seconds: 45)
        XCTAssertEqual(session.currentSlide?.title, "2장")
        XCTAssertEqual(session.elapsedSeconds, 45)
        XCTAssertEqual(session.progressFraction, 2.0 / 3.0, accuracy: 0.001)

        session.jump(to: 2)
        XCTAssertNil(session.nextSlide)
        session.previous()
        XCTAssertEqual(session.currentSlideIndex, 1)
    }

    // MARK: - 6. PDF Studio Page Manipulation & Bidirectional Conversion

    func testPDFPageManipulationOutlineAndBidirectionalBridges() throws {
        var pdf1 = PDFStudioDocumentModel(
            title: "문서 A",
            pages: [
                PDFPageDescriptor(pageNumber: 1, extractedText: "1페이지 제목\n핵심 요약 1"),
                PDFPageDescriptor(pageNumber: 2, extractedText: "2페이지 제목\n핵심 요약 2")
            ],
            annotations: [
                PDFAnnotationItem(pageNumber: 2, kind: .highlight, content: "2페이지 주석", author: "QA")
            ],
            outline: [
                PDFOutlineNode(title: "제2장", targetPageNumber: 2)
            ]
        )
        let pdf2 = PDFStudioDocumentModel(
            title: "문서 B",
            pages: [
                PDFPageDescriptor(pageNumber: 1, extractedText: "3페이지 제목\n핵심 요약 3")
            ]
        )

        pdf1.insertBlankPage(at: 1)
        XCTAssertEqual(pdf1.pages.count, 3)
        XCTAssertEqual(pdf1.pages.map(\.pageNumber), [1, 2, 3])
        XCTAssertEqual(pdf1.pages[1].extractedText, "")
        // Annotation and outline originally on page 2 must shift to page 3 (F6)
        XCTAssertEqual(pdf1.annotations.first?.pageNumber, 3)
        XCTAssertEqual(pdf1.outline.first?.targetPageNumber, 3)

        pdf1.deletePage(at: 1)
        XCTAssertEqual(pdf1.pages.count, 2)
        XCTAssertEqual(pdf1.pages.map(\.pageNumber), [1, 2])
        XCTAssertEqual(pdf1.annotations.first?.pageNumber, 2)
        XCTAssertEqual(pdf1.outline.first?.targetPageNumber, 2)

        pdf1.reorderPage(from: 1, to: 0)
        XCTAssertTrue(pdf1.pages[0].extractedText.contains("2페이지 제목"))
        XCTAssertEqual(pdf1.pages.map(\.pageNumber), [1, 2])
        XCTAssertEqual(pdf1.annotations.first?.pageNumber, 1)
        XCTAssertEqual(pdf1.outline.first?.targetPageNumber, 1)

        let merged = PDFStudioDocumentModel.merge(documents: [pdf1, pdf2], mergedTitle: "통합 PDF")
        XCTAssertEqual(merged.pages.count, 3)
        XCTAssertEqual(merged.pages.map(\.pageNumber), [1, 2, 3])

        let (part1, part2) = merged.split(atOneBasedPage: 2)
        XCTAssertEqual(part1.pages.count, 2)
        XCTAssertEqual(part2.pages.count, 1)
        XCTAssertEqual(part2.pages[0].pageNumber, 1)

        let extracted = merged.extractPages(oneBasedPageNumbers: [1, 3], newTitle: "발췌본")
        XCTAssertEqual(extracted.pages.count, 2)
        XCTAssertEqual(extracted.pages.map(\.pageNumber), [1, 2])

        // PDF -> Presentation bridge
        let deckFromPDF = merged.convertToPresentation()
        XCTAssertEqual(deckFromPDF.slides.count, 3)
        XCTAssertEqual(deckFromPDF.slides[0].bulletPoints, ["핵심 요약 2"])

        // Presentation -> PDF bridge
        let pdfFromDeck = PDFStudioDocumentModel.fromPresentation(deckFromPDF)
        XCTAssertEqual(pdfFromDeck.pages.count, 3)
        XCTAssertEqual(pdfFromDeck.outline.count, 3)

        // Document -> PDF bridge
        let doc = OfficeDocumentModel(
            title: "기획서",
            paragraphs: [
                ParagraphBlock(style: .heading1, text: "1장 서론"),
                ParagraphBlock(style: .body, text: "본문 문단입니다.")
            ]
        )
        let pdfFromDoc = PDFStudioDocumentModel.fromDocument(doc)
        XCTAssertFalse(pdfFromDoc.pages.isEmpty)
        XCTAssertEqual(pdfFromDoc.outline.first?.title, "1장 서론")

        // Worksheet -> PDF bridge
        var sheet = WorksheetModel(name: "실적표")
        try sheet.setCell("A1", rawValue: "항목")
        try sheet.setCell("B1", rawValue: "금액")
        try sheet.setCell("A2", rawValue: "매출")
        try sheet.setCell("B2", rawValue: "500")
        let pdfFromSheet = PDFStudioDocumentModel.fromWorksheet(sheet, range: try CellRange(notation: "A1:B2"))
        XCTAssertEqual(pdfFromSheet.pages.count, 1)
        XCTAssertTrue(pdfFromSheet.pages[0].extractedText.contains("500"))
    }

    // MARK: - 7. FormulaAST Compilation & 25-Level Diamond DAG Memoization Test

    func testDiamondDependencyDAGMemoizationAndFormulaAST() throws {
        var sheet = WorksheetModel(name: "DAG메모이제이션")
        try sheet.setCell("A1", rawValue: "1")
        for level in 2...25 {
            let prev = "A\(level - 1)"
            let curr = "A\(level)"
            try sheet.setCell(curr, rawValue: "=\(prev)+\(prev)")
        }

        // Verify primary coordinate storage is populated
        XCTAssertEqual(sheet.cellsByCoordinate.count, 25)
        let a25Coord = PackedCellCoordinate(row: 24, column: 0)
        XCTAssertEqual(sheet.cellsByCoordinate[a25Coord], "=A24+A24")

        // Without O(V+E) memoization, 25 levels of A_k = A_{k-1} + A_{k-1} requires 2^24 = 16,777,216 evaluations.
        // With EvaluationContext.memo, it evaluates in 25 steps (< 10 ms).
        let clock = ContinuousClock()
        var evaluated: EvaluatedCellValue = .empty
        let elapsed = clock.measure {
            evaluated = sheet.evaluate(at: "A25")
        }

        XCTAssertEqual(evaluated, .number(Double(1 << 24)))
        XCTAssertLessThan(elapsed, .milliseconds(10))
    }

    // MARK: - 8. Multi-Sheet Tabs, CellStyle Formatting, Batch evaluateGrid, and Inline Run Formatting

    func testMultiSheetSwitchingCellStyleNumberFormatBatchEvaluateGridAndRunSplitting() throws {
        var sheet = WorksheetModel(name: "본사매출")
        try sheet.setCell("B2", rawValue: "1250")
        try sheet.setCell("C2", rawValue: "0.125")
        try sheet.setCell("D3", rawValue: "=B2*2")

        // 1. Detected ranges
        let dataRange = sheet.detectedDataRange()
        XCTAssertEqual(dataRange.start.a1Notation, "B2")
        XCTAssertEqual(dataRange.end.a1Notation, "D3")

        let numRange = sheet.detectedNumericRange()
        XCTAssertEqual(numRange.start.a1Notation, "B2")
        XCTAssertEqual(numRange.end.a1Notation, "D3")

        // 2. CellStyle & CellNumberFormat
        try sheet.setCellStyle(
            CellStyle(isBold: true, alignment: .trailing, numberFormat: .currencyKRW, fillHex: "#EEF0FF", textHex: "#5C6BC0"),
            at: "B2"
        )
        try sheet.setCellStyle(
            CellStyle(alignment: .center, numberFormat: .percentage),
            at: "C2"
        )
        try sheet.setCellStyle(
            CellStyle(alignment: .trailing, numberFormat: .decimal2),
            at: "D3"
        )

        // 3. Single-context batch evaluateGrid(rows:columns:)
        let preEvaluated = sheet.evaluateGrid(rows: 0...4, columns: 0...4)
        XCTAssertEqual(preEvaluated[PackedCellCoordinate(row: 1, column: 1)], .number(1250))
        XCTAssertEqual(preEvaluated[PackedCellCoordinate(row: 2, column: 3)], .number(2500))

        XCTAssertEqual(sheet.formattedDisplayValue(at: try CellAddress(a1Notation: "B2"), using: preEvaluated), "₩1,250")
        XCTAssertEqual(sheet.formattedDisplayValue(at: try CellAddress(a1Notation: "C2"), using: preEvaluated), "12.5%")
        XCTAssertEqual(sheet.formattedDisplayValue(at: try CellAddress(a1Notation: "D3"), using: preEvaluated), "2,500.00")

        // 4. Multi-sheet add & switchActiveSheet
        sheet.addSheetPage(
            named: "지사매출",
            initialCells: [PackedCellCoordinate(row: 0, column: 0): "900"]
        )
        XCTAssertEqual(sheet.allSheetNames, ["본사매출", "지사매출"])
        XCTAssertEqual(sheet.activeSheetIndex, 0)

        sheet.switchActiveSheet(to: 1)
        XCTAssertEqual(sheet.activeSheetIndex, 1)
        XCTAssertEqual(sheet.name, "지사매출")
        XCTAssertEqual(sheet.allSheetNames, ["본사매출", "지사매출"])
        XCTAssertEqual(sheet.evaluate(at: "A1"), .number(900))
        try sheet.setCell("A2", rawValue: "=A1+100")
        XCTAssertEqual(sheet.evaluate(at: "A2"), .number(1000))

        sheet.switchActiveSheet(to: 0)
        XCTAssertEqual(sheet.activeSheetIndex, 0)
        XCTAssertEqual(sheet.name, "본사매출")
        XCTAssertEqual(sheet.allSheetNames, ["본사매출", "지사매출"])
        XCTAssertEqual(sheet.evaluate(at: "D3"), .number(2500))
        XCTAssertEqual(sheet.cellStyle(at: "B2").numberFormat, .currencyKRW)

        // Codable backward & full round-trip on WorksheetModel
        let encodedJSON = try JSONEncoder().encode(sheet)
        let decodedSheet = try JSONDecoder().decode(WorksheetModel.self, from: encodedJSON)
        XCTAssertEqual(decodedSheet, sheet)

        // 5. ParagraphBlock.effectiveRuns & applyRunFormatting
        var paragraph = ParagraphBlock(text: "Hello Lavender World")
        XCTAssertEqual(paragraph.effectiveRuns.count, 1)
        XCTAssertEqual(paragraph.effectiveRuns.first?.text, "Hello Lavender World")

        let lavenderRange = try XCTUnwrap(paragraph.text.range(of: "Lavender"))
        paragraph.applyRunFormatting(
            range: lavenderRange,
            isBold: true,
            isUnderlined: true,
            fontSizePt: 15.0,
            foregroundHex: "#5C6BC0"
        )
        XCTAssertEqual(paragraph.text, "Hello Lavender World")
        XCTAssertEqual(paragraph.runs.count, 3)
        XCTAssertEqual(paragraph.runs[0].text, "Hello ")
        XCTAssertEqual(paragraph.runs[0].isBold, false)
        XCTAssertEqual(paragraph.runs[1].text, "Lavender")
        XCTAssertEqual(paragraph.runs[1].isBold, true)
        XCTAssertEqual(paragraph.runs[1].isUnderlined, true)
        XCTAssertEqual(paragraph.runs[1].fontSizePt, 15.0)
        XCTAssertEqual(paragraph.runs[1].foregroundHex, "#5C6BC0")
        XCTAssertEqual(paragraph.runs[2].text, " World")

        // 6. CellNumberFormat.dateISO overflow safety on huge finite Double (F3)
        XCTAssertEqual(CellNumberFormat.dateISO.format(.number(1e20)), EvaluatedCellValue.number(1e20).description)
        XCTAssertEqual(CellNumberFormat.dateISO.format(.number(20261018)), "2026-10-18")

        // 7. ParagraphBlock.runs synchronization across findAndReplace, applyingMailMerge, and ODF encode/decode (F1)
        var docWithRuns = OfficeDocumentModel(
            title: "런 동기화 문서",
            format: .odt,
            paragraphs: [paragraph]
        )
        let (replacedDoc, repCount) = docWithRuns.findAndReplace(search: "Lavender", replacement: "Indigo")
        XCTAssertEqual(repCount, 1)
        XCTAssertEqual(replacedDoc.paragraphs[0].text, "Hello Indigo World")
        XCTAssertEqual(replacedDoc.paragraphs[0].effectiveRuns.map(\.text).joined(), "Hello Indigo World")
        XCTAssertEqual(replacedDoc.paragraphs[0].runs[1].text, "Indigo")
        XCTAssertEqual(replacedDoc.paragraphs[0].runs[1].foregroundHex, "#5C6BC0")

        let odtEncoded = try ODFContainerCodec.encodeDocument(replacedDoc)
        let odtDecoded = try ODFContainerCodec.decodeDocument(from: odtEncoded)
        XCTAssertEqual(odtDecoded.paragraphs[0].text, "Hello Indigo World")

        docWithRuns.paragraphs[0].text = "직접 수정된 본문 {{부서}}"
        let mergedDoc = docWithRuns.applyingMailMerge(record: ["부서": "보안팀"])
        XCTAssertEqual(mergedDoc.paragraphs[0].text, "직접 수정된 본문 보안팀")
        XCTAssertEqual(mergedDoc.paragraphs[0].effectiveRuns.map(\.text).joined(), "직접 수정된 본문 보안팀")
    }

    func testCrossSheetFormulaReferencesMergeCellsAndPDFPresentationWrapping() throws {
        // 1. Cross-sheet cell & range references, quoted sheet names, VLOOKUP, SUMIF, and cycle detection
        var workbook = WorksheetModel(name: "요약시트")
        let a1 = PackedCellCoordinate(row: 0, column: 0)
        let b1 = PackedCellCoordinate(row: 0, column: 1)
        let a2 = PackedCellCoordinate(row: 1, column: 0)
        let b2 = PackedCellCoordinate(row: 1, column: 1)

        workbook.addSheetPage(
            named: "Sheet2",
            initialCells: [
                a1: "10",
                b1: "25",
                a2: "30",
                b2: "45"
            ]
        )
        workbook.addSheetPage(
            named: "Summary Sheet",
            initialCells: [
                a1: "SKU-01",
                b1: "9900",
                a2: "SKU-02",
                b2: "14500"
            ]
        )

        try workbook.setCell("A1", rawValue: "=Sheet2!A1 + Sheet2!B2")
        try workbook.setCell("A2", rawValue: "=SUM(Sheet2!A1:B2)")
        try workbook.setCell("A3", rawValue: "='Summary Sheet'!B2")
        try workbook.setCell("A4", rawValue: "SKU-02")
        try workbook.setCell("A5", rawValue: "=VLOOKUP(A4, 'Summary Sheet'!A1:B2, 2, FALSE)")
        try workbook.setCell("A6", rawValue: "=SUMIF(Sheet2!A1:A2, \">15\", Sheet2!B1:B2)")
        try workbook.setCell("A7", rawValue: "=UnknownSheet!A1")

        XCTAssertEqual(workbook.evaluate(at: "A1"), .number(55))
        XCTAssertEqual(workbook.evaluate(at: "A2"), .number(110))
        XCTAssertEqual(workbook.evaluate(at: "A3"), .number(14500))
        XCTAssertEqual(workbook.evaluate(at: "A5"), .number(14500))
        XCTAssertEqual(workbook.evaluate(at: "A6"), .number(45))
        XCTAssertEqual(workbook.evaluate(at: "A7"), .error("#REF!"))

        // Cross-sheet cycle detection: Sheet1!C1 -> Sheet2!C1 -> 요약시트!C1
        let c1 = PackedCellCoordinate(row: 0, column: 2)
        workbook.additionalSheets[0].cellsByCoordinate[c1] = "=요약시트!C1"
        try workbook.setCell("C1", rawValue: "=Sheet2!C1")
        XCTAssertEqual(workbook.evaluate(at: "C1"), .error("#REF!"))

        // 2. Per-sheet sheetComments and mergedRanges + toggleMergeCells / isCellMerged
        let mergeRange = try CellRange(notation: "D1:E2")
        XCTAssertFalse(workbook.isCellMerged(PackedCellCoordinate(row: 0, column: 3)))
        workbook.toggleMergeCells(mergeRange)
        XCTAssertTrue(workbook.isCellMerged(PackedCellCoordinate(row: 0, column: 3)))
        XCTAssertTrue(workbook.isCellMerged(PackedCellCoordinate(row: 1, column: 4)))
        workbook.sheetComments[a1] = "요약시트 코멘트"

        workbook.switchActiveSheet(to: 1) // Switch to Sheet2
        XCTAssertEqual(workbook.name, "Sheet2")
        XCTAssertTrue(workbook.mergedRanges.isEmpty)
        XCTAssertTrue(workbook.sheetComments.isEmpty)

        let sheet2Merge = try CellRange(notation: "A1:B1")
        workbook.toggleMergeCells(sheet2Merge)
        workbook.sheetComments[b1] = "Sheet2 코멘트"

        workbook.switchActiveSheet(to: 0) // Switch back to 요약시트
        XCTAssertEqual(workbook.name, "요약시트")
        XCTAssertEqual(workbook.mergedRanges, [mergeRange])
        XCTAssertEqual(workbook.sheetComments[a1], "요약시트 코멘트")
        XCTAssertEqual(workbook.additionalSheets[0].mergedRanges, [sheet2Merge])
        XCTAssertEqual(workbook.additionalSheets[0].sheetComments[b1], "Sheet2 코멘트")

        workbook.toggleMergeCells(mergeRange)
        XCTAssertFalse(workbook.isCellMerged(PackedCellCoordinate(row: 0, column: 3)))

        // 3. CellRange.forEachCoordinate zero-allocation iteration
        var visitedCoords: [PackedCellCoordinate] = []
        mergeRange.forEachCoordinate { coord in
            visitedCoords.append(coord)
        }
        XCTAssertEqual(visitedCoords.count, 4)

        // 4. PDFStudioDocumentModel.fromPresentation includes slide.elements and speakerNotes & PDFBinarySerializer.wrapLine
        let deck = PresentationDeckModel(
            title: "발표자료 PDF 변환",
            slides: [
                SlideModel(
                    title: "슬라이드 제목",
                    subtitle: "부제목",
                    bulletPoints: ["핵심 불릿 1"],
                    speakerNotes: "발표자 전용 상세 노트",
                    elements: [
                        SlideCanvasElement(title: "캔버스 도형 제목", bodyText: "캔버스 도형 본문 텍스트", x: 20, y: 30, width: 200, height: 80)
                    ]
                )
            ]
        )
        let pdfFromDeck = PDFStudioDocumentModel.fromPresentation(deck)
        let pageText = pdfFromDeck.pages.first?.extractedText ?? ""
        XCTAssertTrue(pageText.contains("슬라이드 제목"))
        XCTAssertTrue(pageText.contains("캔버스 도형 제목"))
        XCTAssertTrue(pageText.contains("캔버스 도형 본문 텍스트"))
        XCTAssertTrue(pageText.contains("발표자 전용 상세 노트"))

        let longLine = String(repeating: "가나다라마바사아자차", count: 10) // 100 chars
        let wrapped = PDFBinarySerializer.wrapLine(longLine, maxChars: 64)
        XCTAssertEqual(wrapped.count, 2)
        XCTAssertEqual(wrapped[0].count, 64)
        XCTAssertEqual(wrapped[1].count, 36)
    }
}
