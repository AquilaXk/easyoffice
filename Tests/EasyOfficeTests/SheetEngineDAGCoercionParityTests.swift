import Foundation
import Testing

@testable import EasyOfficeCore

@Suite("Sheet Engine DAG, Coercion, and Formula Parity")
struct SheetEngineDAGCoercionParityTests {

  private struct CoercionFixtureVector: Decodable {
    let id: String
    let cells: [String: String]
    let target: String
    let expectedType: String
    let expectedDisplay: String
  }

  @Test("Shared sheet-coercion-parity.json fixture vectors match Rust and Swift")
  func sharedCoercionParityFixtureVectors() throws {
    let repoRoot = URL(fileURLWithPath: #filePath)
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
    let fixtureURL = repoRoot.appendingPathComponent("fixtures/sheet-coercion-parity.json")
    let data = try Data(contentsOf: fixtureURL)
    let vectors = try JSONDecoder().decode([CoercionFixtureVector].self, from: data)
    #expect(!vectors.isEmpty)

    for vec in vectors {
      var sheet = WorksheetModel(name: "Fixture")
      for (a1, raw) in vec.cells.sorted(by: { $0.key < $1.key }) {
        try sheet.setCell(a1, rawValue: raw)
      }
      let targetAddr = try CellAddress(a1Notation: vec.target)
      let evaluated = sheet.evaluate(at: vec.target)
      let gridEvaluated = sheet.evaluateGrid(
        rows: 0...max(10, targetAddr.row),
        columns: 0...max(10, targetAddr.column)
      )
      #expect(gridEvaluated[targetAddr.packedCoordinate] == evaluated)

      let actualType: String
      switch evaluated {
      case .empty:
        actualType = "empty"
      case .number:
        actualType = "number"
      case .text:
        actualType = "text"
      case .boolean:
        actualType = "boolean"
      case .error:
        actualType = "error"
      }
      #expect(actualType == vec.expectedType)
      #expect(evaluated.description == vec.expectedDisplay)
    }
  }

  @Test("safeInt rejects -2^63 without trapping across rounding, date, and lookup functions")
  func safeIntRejectsIntMinWithoutTrapping() throws {
    var sheet = WorksheetModel(name: "SafeInt")
    try sheet.setCell("A1", rawValue: "10")
    try sheet.setCell("B1", rawValue: "20")
    try sheet.setCell("A2", rawValue: "30")
    try sheet.setCell("B2", rawValue: "40")

    let formulas: [(String, EvaluatedCellValue)] = [
      ("=ROUND(1, -9223372036854775808)", .error("#NUM!")),
      ("=ROUNDUP(1, -9223372036854775808)", .error("#NUM!")),
      ("=ROUNDDOWN(1, -9223372036854775808)", .error("#NUM!")),
      ("=DATE(2024, -9223372036854775808, 1)", .error("#VALUE!")),
      ("=DATE(2024, 1, -9223372036854775808)", .error("#VALUE!")),
      ("=EDATE(\"2024-01-15\", -9223372036854775808)", .error("#VALUE!")),
      ("=EOMONTH(\"2024-01-15\", -9223372036854775808)", .error("#VALUE!")),
      ("=WORKDAY(\"2024-01-15\", -9223372036854775808)", .error("#VALUE!")),
      ("=VLOOKUP(10, A1:B2, -9223372036854775808, FALSE)", .error("#VALUE!")),
      ("=HLOOKUP(10, A1:B2, -9223372036854775808, FALSE)", .error("#VALUE!")),
      ("=INDEX(A1:B2, -9223372036854775808, 1)", .error("#VALUE!")),
      ("=INDEX(A1:B2, 1, -9223372036854775808)", .error("#VALUE!")),
    ]

    for (idx, (formula, expected)) in formulas.enumerated() {
      let cell = "D\(idx + 1)"
      try sheet.setCell(cell, rawValue: formula)
      let result = sheet.evaluate(at: cell)
      #expect(result == expected)
    }
  }

  @Test("DAG 2000-cell forward and backward chains, 99999 range budget, A:A/1:1 refs, and XFD1048576")
  func dagChainsAndLargeRangeParity() throws {
    var fwd = WorksheetModel(name: "ForwardChain")
    try fwd.setCell("A1", rawValue: "1")
    for r in 2...2000 {
      try fwd.setCell("A\(r)", rawValue: "=A\(r - 1)+1")
    }
    #expect(fwd.evaluate(at: "A2000") == .number(2000.0))
    let fwdAll = fwd.evaluateGrid(rows: 0...1999, columns: 0...0)
    #expect(fwdAll[PackedCellCoordinate(row: 1999, column: 0)] == .number(2000.0))
    #expect(fwdAll[PackedCellCoordinate(row: 999, column: 0)] == .number(1000.0))

    var bwd = WorksheetModel(name: "BackwardChain")
    try bwd.setCell("A2000", rawValue: "1")
    for r in stride(from: 1999, through: 1, by: -1) {
      try bwd.setCell("A\(r)", rawValue: "=A\(r + 1)+1")
    }
    #expect(bwd.evaluate(at: "A1") == .number(2000.0))
    let bwdAll = bwd.evaluateGrid(rows: 0...1999, columns: 0...0)
    #expect(bwdAll[PackedCellCoordinate(row: 0, column: 0)] == .number(2000.0))
    #expect(bwdAll[PackedCellCoordinate(row: 1000, column: 0)] == .number(1000.0))

    var large = WorksheetModel(name: "LargeRange")
    for r in 0..<99_999 {
      try large.setCell(row: r, column: 0, rawValue: "1")
    }
    try large.setCell("B1", rawValue: "=SUM(A1:A99999)")
    try large.setCell("B2", rawValue: "=COUNTIF(A1:A99999, 1)")
    try large.setCell("B3", rawValue: "=SUMIF(A1:A99999, 1)")
    try large.setCell("B4", rawValue: "=SUM(A:A)")
    try large.setCell("B5", rawValue: "=COUNTIF(A:A, 1)")
    #expect(large.evaluate(at: "B1") == .number(99999.0))
    #expect(large.evaluate(at: "B2") == .number(99999.0))
    #expect(large.evaluate(at: "B3") == .number(99999.0))
    #expect(large.evaluate(at: "B4") == .number(99999.0))
    #expect(large.evaluate(at: "B5") == .number(99999.0))

    var grid = WorksheetModel(name: "WholeRowCol")
    try grid.setCell("B2", rawValue: "10")
    try grid.setCell("C3", rawValue: "25")
    try grid.setCell("D1", rawValue: "=SUM(B:C)")
    try grid.setCell("E1", rawValue: "=SUM(2:3)")
    #expect(grid.evaluate(at: "D1") == .number(35.0))
    #expect(grid.evaluate(at: "E1") == .number(35.0))

    try grid.setCell("XFD1048576", rawValue: "42")
    try grid.setCell("A1", rawValue: "=XFD1048576+8")
    #expect(grid.evaluate(at: "XFD1048576") == .number(42.0))
    #expect(grid.evaluate(at: "A1") == .number(50.0))

    let terms = Array(repeating: "1", count: 1025).joined(separator: "+")
    try grid.setCell("A2", rawValue: "=\(terms)")
    #expect(grid.evaluate(at: "A2") == .number(1025.0))
  }

  @MainActor
  @Test("Auto-date format, financial type=1, wildcards, error literals, autofill, and pivot reuse")
  func autoDateFinancialWildcardsAndPivotParity() throws {
    var sheet = WorksheetModel(name: "Parity")
    try sheet.setCell("A1", rawValue: "=DATE(2024, 1, 15)")
    #expect(sheet.evaluate(at: "A1") == .number(45306.0))
    #expect(sheet.formattedDisplayValue(at: try CellAddress(a1Notation: "A1")) == "2024-01-15")
    #expect(CellNumberFormat.dateISO.format(.number(1.0)) == "1900-01-01")
    try sheet.setCell("A2", rawValue: "=\"2024-01-15\"+1")
    #expect(sheet.evaluate(at: "A2") == .number(45307.0))
    try sheet.setCell("A3", rawValue: "=DATE(1900, 1, 0)")
    #expect(sheet.evaluate(at: "A3") == .number(0.0))

    try sheet.setCell("F1", rawValue: "=IPMT(0.1/12, 1, 12, 1000, 0, 1)")
    try sheet.setCell("F2", rawValue: "=IPMT(0.1/12, 2, 12, 1000, 0, 1)")
    try sheet.setCell("F3", rawValue: "=PPMT(0.1/12, 2, 12, 1000, 0, 1)")
    try sheet.setCell("F4", rawValue: "=PMT(0.1/12, 12, 1000, 0, 1)")
    try sheet.setCell("F5", rawValue: "=RATE(360, -5000000, 1000000000)")
    #expect(sheet.evaluate(at: "F1") == .number(0.0))
    if case .number(let ipmt2) = sheet.evaluate(at: "F2"),
      case .number(let ppmt2) = sheet.evaluate(at: "F3"),
      case .number(let pmt) = sheet.evaluate(at: "F4"),
      case .number(let rate) = sheet.evaluate(at: "F5")
    {
      #expect(abs(ipmt2 - (-7.606755752920028)) < 1e-9)
      #expect(abs(ppmt2 - (-79.58255389667656)) < 1e-9)
      #expect(abs((ipmt2 + ppmt2) - pmt) < 1e-9)
      #expect(abs(rate - 0.0036562697) < 1e-6)
    } else {
      Issue.record("Expected numeric financial results")
    }

    try sheet.setCell("W1", rawValue: "apple")
    try sheet.setCell("W2", rawValue: "banana")
    try sheet.setCell("W3", rawValue: "10")
    try sheet.setCell("W4", rawValue: "TRUE")
    try sheet.setCell("W5", rawValue: "=COUNTIF(W1:W2, \"=a*\")")
    try sheet.setCell("W6", rawValue: "=COUNTIF(W1:W4, \"<>*\")")
    try sheet.setCell("W7", rawValue: "=SEARCH(\"?\", \"abc\")")
    try sheet.setCell("W8", rawValue: "=MATCH(\"BANANA\", W1:W2, 0)")
    #expect(sheet.evaluate(at: "W5") == .number(1.0))
    #expect(sheet.evaluate(at: "W6") == .number(2.0))
    #expect(sheet.evaluate(at: "W7") == .number(1.0))
    #expect(sheet.evaluate(at: "W8") == .number(2.0))

    try sheet.setCell("E1", rawValue: "=#N/A+1")
    try sheet.setCell("E2", rawValue: "=#N/A&\"x\"")
    try sheet.setCell("E3", rawValue: "=IFERROR(#N/A*2, 9)")
    try sheet.setCell("E4", rawValue: "=3>2>1")
    try sheet.setCell("E5", rawValue: "=FLOOR(-2.5, 2)")
    try sheet.setCell("E6", rawValue: "=TEXT(1234.5, \"#,##0.00\")")
    try sheet.setCell("E7", rawValue: "=XLOOKUP(\"banana\", W1:W2, W1:W2, , 0)")
    #expect(sheet.evaluate(at: "E1") == .error("#N/A"))
    #expect(sheet.evaluate(at: "E2") == .error("#N/A"))
    #expect(sheet.evaluate(at: "E3") == .number(9.0))
    #expect(sheet.evaluate(at: "E4") == .boolean(true))
    #expect(sheet.evaluate(at: "E5") == .number(-4.0))
    #expect(sheet.evaluate(at: "E6") == .text("1,234.50"))
    #expect(sheet.evaluate(at: "E7") == .text("banana"))

    var sub = WorksheetModel(name: "SubtotalSort")
    try sub.setCell("A1", rawValue: "10")
    try sub.setCell("A2", rawValue: "20")
    try sub.setCell("A3", rawValue: "=SUBTOTAL(9, A1:A2)")
    try sub.setCell("A4", rawValue: "=SUBTOTAL(9, A1:A3)")
    #expect(sub.evaluate(at: "A3") == .number(30.0))
    #expect(sub.evaluate(at: "A4") == .number(30.0))

    var af = WorksheetModel(name: "AutoFillShift")
    try af.setCell("A1", rawValue: "0.1")
    try af.setCell("A2", rawValue: "0.2")
    try af.autofill(
      sourceRange: CellRange(start: try CellAddress(a1Notation: "A1"), end: try CellAddress(a1Notation: "A2")),
      targetRange: CellRange(start: try CellAddress(a1Notation: "A1"), end: try CellAddress(a1Notation: "A3"))
    )
    #expect(af.rawValue(at: "A3") == "0.3")
    #expect(WorksheetModel.shiftFormulaReferences("=A1+1", rowDelta: -1, columnDelta: 0) == "=#REF!+1")

    var pivotSheet = WorksheetModel(name: "Sales")
    try pivotSheet.setCell("A1", rawValue: "Dept")
    try pivotSheet.setCell("B1", rawValue: "Amount")
    try pivotSheet.setCell("A2", rawValue: "Core")
    try pivotSheet.setCell("B2", rawValue: "2.5")
    let vm = WorkspaceViewModel()
    if let idx = vm.openTabs.firstIndex(where: { $0.moduleKind == .sheet }) {
      vm.openTabs[idx].payload = .sheet(pivotSheet)
      vm.activeTabID = vm.openTabs[idx].id
      vm.executeRibbonCommand("sheet-mod-pivot")
      vm.executeRibbonCommand("sheet-mod-pivot")
      if let tab = vm.activeTab, case .sheet(let updatedSheet) = tab.payload {
        let pivotPages = updatedSheet.additionalSheets.filter { $0.name == "피벗 요약" }
        #expect(pivotPages.count == 1)
        #expect(pivotPages.first?.cellsByCoordinate[PackedCellCoordinate(row: 1, column: 1)] == "2.5")
      } else {
        Issue.record("Expected updated sheet payload after pivot command")
      }
    } else {
      Issue.record("Expected sheet tab in WorkspaceViewModel")
    }
  }
}
