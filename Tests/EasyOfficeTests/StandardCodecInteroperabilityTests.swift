import Foundation
import XCTest

@testable import EasyOfficeCore

final class StandardCodecInteroperabilityTests: XCTestCase {

  func testPDFMultiLineVisibleRenderingWithoutZeroAreaClipping() throws {
    let lines = (1...24).map { "Line \($0) visible PDF content" }
    let page = PDFPageDescriptor(
      pageNumber: 1,
      extractedText: lines.joined(separator: "\n")
    )
    let doc = PDFStudioDocumentModel(title: "24-Line PDF", pages: [page])
    let pdfData = PDFBinarySerializer.encode(doc)
    let rawPDF = String(decoding: pdfData, as: UTF8.self)

    XCTAssertFalse(rawPDF.contains("0 0 0 0 re W n"))
    let visibleBodyLineOps =
      rawPDF.components(separatedBy: "0.145 0.165 0.243 rg BT 0 Tr /F2 10 Tf 54 ").count - 1
    XCTAssertEqual(visibleBodyLineOps, 24)

    let decoded = try PDFBinarySerializer.decode(from: pdfData)
    XCTAssertEqual(decoded.pages.count, 1)
    for idx in 1...24 {
      XCTAssertTrue(decoded.pages[0].extractedText.contains("Line \(idx) visible PDF content"))
    }
  }

  func testPDF15CompressedXRefStreamAndMultiLineBTContentDecoding() throws {
    let contentStream = """
      BT
      /F1 12 Tf
      54 720 Td
      (First line in PDF 1.5 stream) Tj
      0 -18 Td
      (Second line in PDF 1.5 stream) Tj
      ET
      """
    let pdf15String = """
      %PDF-1.6
      1 0 obj
      << /Type /Catalog /Pages 2 0 R >>
      endobj
      2 0 obj
      << /Type /Pages /Kids [3 0 R] /Count 1 >>
      endobj
      3 0 obj
      << /Type/Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>
      endobj
      4 0 obj
      << /Length \(contentStream.utf8.count) >>
      stream
      \(contentStream)
      endstream
      endobj
      5 0 obj
      << /Type /XRef /Size 6 /W [1 2 1] /Root 1 0 R /Info << /Title (PDF 1.5 XRef Document) >> >>
      stream
      \u{01}\u{00}\u{00}\u{00}
      endstream
      endobj
      startxref
      320
      %%EOF
      """
    let decoded = try PDFBinarySerializer.decode(from: Data(pdf15String.utf8))
    XCTAssertEqual(decoded.title, "PDF 1.5 XRef Document")
    XCTAssertEqual(decoded.pages.count, 1)
    XCTAssertTrue(decoded.pages[0].extractedText.contains("First line in PDF 1.5 stream"))
    XCTAssertTrue(decoded.pages[0].extractedText.contains("Second line in PDF 1.5 stream"))
  }

  func testODTMixedParagraphAndHWPXNestedSubListTableCellDecoding() throws {
    let odtContentXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0">
        <office:body>
          <office:text>
            <text:p>앞부분 일반 텍스트 <text:span bold="1">강조 구간</text:span> 뒷부분 일반 텍스트</text:p>
            <table:table table:name="ODTTable">
              <table:table-row>
                <table:table-cell><text:p>셀앞 <text:span bold="1">셀강조</text:span> 셀뒤</text:p></table:table-cell>
              </table:table-row>
            </table:table>
          </office:text>
        </office:body>
      </office:document-content>
      """
    let odtData = try ZipContainerCodec.pack(entries: [
      ZipContainerEntry(
        path: "mimetype", data: Data(ODFContainerCodec.textMimeType.utf8),
        compressionMethod: .store),
      ZipContainerEntry(
        path: "content.xml", data: Data(odtContentXML.utf8), compressionMethod: .deflate),
    ])

    let decodedODT = try ODFContainerCodec.decodeDocument(from: odtData)
    XCTAssertEqual(decodedODT.paragraphs.first?.text, "앞부분 일반 텍스트 강조 구간 뒷부분 일반 텍스트")
    XCTAssertEqual(decodedODT.paragraphs.first?.runs.count, 3)
    XCTAssertEqual(decodedODT.paragraphs.first?.runs[1].text, "강조 구간")
    XCTAssertEqual(decodedODT.paragraphs.first?.runs[1].isBold, true)
    XCTAssertEqual(decodedODT.tables.first?.rows.first?.first, "셀앞 셀강조 셀뒤")

    let hwpxContentHPF = """
      <?xml version="1.0" encoding="UTF-8"?>
      <opf:package xmlns:opf="http://www.idpf.org/2007/opf/">
        <opf:metadata><opf:title>표준 HWPX 문서</opf:title></opf:metadata>
      </opf:package>
      """
    let hwpxHeaderXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <hh:head xmlns:hh="http://www.owpml.org/owpml/2011/head" title="표준 HWPX 문서"/>
      """
    let hwpxSectionXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <hs:sec xmlns:hs="http://www.owpml.org/owpml/2011/section" xmlns:hp="http://www.owpml.org/owpml/2011/paragraph">
        <hp:p><hp:run><hp:t>본문 문단</hp:t></hp:run></hp:p>
        <hp:tbl caption="HWPX 표준 표" hasHeader="true">
          <hp:tr>
            <hp:tc><hp:subList><hp:p><hp:run><hp:t>중첩 셀 텍스트 A</hp:t></hp:run></hp:p></hp:subList></hp:tc>
            <hp:tc><hp:subList><hp:p><hp:run><hp:t>중첩 셀 텍스트 B</hp:t></hp:run></hp:p></hp:subList></hp:tc>
          </hp:tr>
        </hp:tbl>
      </hs:sec>
      """
    let hwpxData = try ZipContainerCodec.pack(entries: [
      ZipContainerEntry(
        path: "mimetype", data: Data(HWPXContainerCodec.hwpxMimeType.utf8),
        compressionMethod: .store),
      ZipContainerEntry(
        path: "Contents/content.hpf", data: Data(hwpxContentHPF.utf8),
        compressionMethod: .deflate),
      ZipContainerEntry(
        path: "Contents/header.xml", data: Data(hwpxHeaderXML.utf8), compressionMethod: .deflate),
      ZipContainerEntry(
        path: "Contents/section0.xml", data: Data(hwpxSectionXML.utf8),
        compressionMethod: .deflate),
    ])

    let decodedHWPX = try HWPXContainerCodec.decodeDocument(from: hwpxData)
    XCTAssertEqual(decodedHWPX.tables.count, 1)
    XCTAssertEqual(decodedHWPX.tables[0].rows, [["중첩 셀 텍스트 A", "중첩 셀 텍스트 B"]])
  }

  func testXLSXRelationshipsResolutionAndFormulaCachedValue() throws {
    var sheet = WorksheetModel(name: "매출시트")
    try sheet.setCell("A1", rawValue: "10")
    try sheet.setCell("A2", rawValue: "25")
    try sheet.setCell("A3", rawValue: "=A1+A2")

    let encodedXLSX = try OOXMLContainerCodec.encodeWorksheet(sheet)
    let unpacked = try ZipContainerCodec.unpack(from: encodedXLSX)
    let paths = Set(unpacked.map(\.path))
    XCTAssertTrue(paths.contains("_rels/.rels"))
    XCTAssertTrue(paths.contains("xl/_rels/workbook.xml.rels"))

    let sheet1XML = String(
      decoding: try XCTUnwrap(unpacked.first(where: { $0.path == "xl/worksheets/sheet1.xml" })?.data),
      as: UTF8.self
    )
    XCTAssertTrue(sheet1XML.contains("<f>A1+A2</f><v>35</v>"))

    let customWorkbookXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
        <sheets>
          <sheet name="연결된시트" sheetId="1" r:id="rIdCustom9"/>
        </sheets>
      </workbook>
      """
    let customRelsXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rIdCustom9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet9.xml"/>
      </Relationships>
      """
    let customSheet9XML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
        <sheetData>
          <row r="1"><c r="A1"><v>777</v></c></row>
        </sheetData>
      </worksheet>
      """
    let customXLSX = try ZipContainerCodec.pack(entries: [
      ZipContainerEntry(
        path: "[Content_Types].xml",
        data: Data("<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>".utf8),
        compressionMethod: .deflate),
      ZipContainerEntry(
        path: "xl/workbook.xml", data: Data(customWorkbookXML.utf8), compressionMethod: .deflate),
      ZipContainerEntry(
        path: "xl/_rels/workbook.xml.rels", data: Data(customRelsXML.utf8),
        compressionMethod: .deflate),
      ZipContainerEntry(
        path: "xl/worksheets/sheet9.xml", data: Data(customSheet9XML.utf8),
        compressionMethod: .deflate),
    ])

    let decodedCustom = try OOXMLContainerCodec.decodeWorksheet(from: customXLSX)
    XCTAssertEqual(decodedCustom.name, "연결된시트")
    XCTAssertEqual(decodedCustom.evaluate(at: "A1"), .number(777))
  }

  func testPPTXStandardPlaceholderAndEMUCoordinateNormalization() throws {
    let deck = PresentationDeckModel(
      title: "패키지 검증 덱",
      slides: [SlideModel(title: "슬라이드 1", subtitle: "부제목 1", bulletPoints: ["항목 1"])]
    )
    let encodedPPTX = try OOXMLContainerCodec.encodePresentation(deck)
    let unpacked = try ZipContainerCodec.unpack(from: encodedPPTX)
    let paths = Set(unpacked.map(\.path))
    XCTAssertTrue(paths.contains("_rels/.rels"))
    XCTAssertTrue(paths.contains("ppt/_rels/presentation.xml.rels"))

    let externalPresXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
        <p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst>
      </p:presentation>
      """
    let externalPresRels = """
      <?xml version="1.0" encoding="UTF-8"?>
      <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
      </Relationships>
      """
    let externalSlide1XML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
          <p:spTree>
            <p:sp>
              <p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
              <p:spPr><a:xfrm><a:off x="685800" y="457200"/><a:ext cx="4572000" cy="1143000"/></a:xfrm></p:spPr>
              <p:txBody><a:bodyPr/><a:p><a:r><a:t>표준 플레이스홀더 제목</a:t></a:r></a:p></p:txBody>
            </p:sp>
            <p:sp>
              <p:nvSpPr><p:cNvPr id="3" name="Subtitle 2"/><p:nvPr><p:ph type="subTitle"/></p:nvPr></p:nvSpPr>
              <p:spPr><a:xfrm><a:off x="685800" y="1600200"/><a:ext cx="4572000" cy="685800"/></a:xfrm></p:spPr>
              <p:txBody><a:bodyPr/><a:p><a:r><a:t>표준 플레이스홀더 부제목</a:t></a:r></a:p></p:txBody>
            </p:sp>
            <p:sp>
              <p:nvSpPr><p:cNvPr id="4" name="Content Placeholder 3"/><p:nvPr><p:ph type="body"/></p:nvPr></p:nvSpPr>
              <p:spPr><a:xfrm><a:off x="685800" y="2400300"/><a:ext cx="4572000" cy="2286000"/></a:xfrm></p:spPr>
              <p:txBody><a:bodyPr/><a:p><a:r><a:t>본문 불릿 1</a:t></a:r></a:p><a:p><a:r><a:t>본문 불릿 2</a:t></a:r></a:p></p:txBody>
            </p:sp>
          </p:spTree>
        </p:cSld>
      </p:sld>
      """
    let externalPPTX = try ZipContainerCodec.pack(entries: [
      ZipContainerEntry(
        path: "[Content_Types].xml",
        data: Data("<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>".utf8),
        compressionMethod: .deflate),
      ZipContainerEntry(
        path: "ppt/presentation.xml", data: Data(externalPresXML.utf8), compressionMethod: .deflate),
      ZipContainerEntry(
        path: "ppt/_rels/presentation.xml.rels", data: Data(externalPresRels.utf8),
        compressionMethod: .deflate),
      ZipContainerEntry(
        path: "ppt/slides/slide1.xml", data: Data(externalSlide1XML.utf8),
        compressionMethod: .deflate),
    ])

    let decodedDeck = try OOXMLContainerCodec.decodePresentation(from: externalPPTX)
    XCTAssertEqual(decodedDeck.slides.count, 1)
    let slide = decodedDeck.slides[0]
    XCTAssertEqual(slide.title, "표준 플레이스홀더 제목")
    XCTAssertEqual(slide.subtitle, "표준 플레이스홀더 부제목")
    XCTAssertEqual(slide.bulletPoints, ["본문 불릿 1", "본문 불릿 2"])
    XCTAssertEqual(slide.elements[0].x, 54.0, accuracy: 0.01)
    XCTAssertEqual(slide.elements[0].y, 36.0, accuracy: 0.01)
    XCTAssertEqual(slide.elements[0].width, 360.0, accuracy: 0.01)
    XCTAssertEqual(slide.elements[0].height, 90.0, accuracy: 0.01)
  }

  func testLegacyDocEightBitASCIIAndCP1252StreamDecoding() throws {
    let eightBitText = "First 8-bit Paragraph\rSecond 8-bit Paragraph\r"
    let eightBitBytes = Data(eightBitText.utf8)
    let fcMin: UInt32 = 64
    let fcMac: UInt32 = fcMin + UInt32(eightBitBytes.count)

    var wordStream = Data(repeating: 0, count: Int(fcMin))
    try BitPacking.writeUInt16LE(LegacyOLE2OfficeCodec.wordFibMagic, into: &wordStream, at: 0)
    try BitPacking.writeUInt16LE(LegacyOLE2OfficeCodec.wordFibVersion97, into: &wordStream, at: 2)
    try BitPacking.writeUInt32LE(fcMin, into: &wordStream, at: 24)
    try BitPacking.writeUInt32LE(fcMac, into: &wordStream, at: 28)
    try BitPacking.writeUInt32LE(UInt32(eightBitBytes.count), into: &wordStream, at: 32)
    wordStream.append(eightBitBytes)

    let oleData = try OLE2ContainerCodec.pack(streams: [
      OLE2StreamEntry(name: "WordDocument", data: wordStream)
    ])
    let decoded = try LegacyOLE2OfficeCodec.decodeWordBinary(from: oleData)
    XCTAssertEqual(
      decoded.paragraphs.map(\.text),
      ["First 8-bit Paragraph", "Second 8-bit Paragraph"]
    )
  }

  func testLegacyXLSBIFF8MultiSheetAndFormulaSpecialMarkerDecoding() throws {
    var stream = Data()

    func appendRecord(opcode: UInt16, payload: Data) {
      BitPacking.appendUInt16LE(opcode, to: &stream)
      BitPacking.appendUInt16LE(UInt16(payload.count), to: &stream)
      stream.append(payload)
    }

    func makeBOF(substreamType: UInt16) -> Data {
      var d = Data()
      BitPacking.appendUInt16LE(0x0600, to: &d)
      BitPacking.appendUInt16LE(substreamType, to: &d)
      BitPacking.appendUInt16LE(0x0DBB, to: &d)
      BitPacking.appendUInt16LE(0x07CC, to: &d)
      BitPacking.appendUInt32LE(0, to: &d)
      BitPacking.appendUInt32LE(0x0006, to: &d)
      return d
    }

    func makeStandardBoundSheet8Bit(name: String) -> Data {
      var d = Data()
      BitPacking.appendUInt32LE(0, to: &d)
      d.append(0)
      d.append(0)
      let nameBytes = Data(name.utf8)
      d.append(UInt8(nameBytes.count))
      d.append(0x00)
      d.append(nameBytes)
      return d
    }

    appendRecord(opcode: LegacyOLE2OfficeCodec.biff8OpcodeBOF, payload: makeBOF(substreamType: 0x0005))
    appendRecord(
      opcode: LegacyOLE2OfficeCodec.biff8OpcodeBoundSheet,
      payload: makeStandardBoundSheet8Bit(name: "AlphaSheet"))
    appendRecord(
      opcode: LegacyOLE2OfficeCodec.biff8OpcodeBoundSheet,
      payload: makeStandardBoundSheet8Bit(name: "BetaSheet"))
    appendRecord(opcode: LegacyOLE2OfficeCodec.biff8OpcodeEOF, payload: Data())

    appendRecord(opcode: LegacyOLE2OfficeCodec.biff8OpcodeBOF, payload: makeBOF(substreamType: 0x0010))

    var strFormula = Data()
    BitPacking.appendUInt16LE(0, to: &strFormula)
    BitPacking.appendUInt16LE(0, to: &strFormula)
    BitPacking.appendUInt16LE(0, to: &strFormula)
    strFormula.append(contentsOf: [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF])
    BitPacking.appendUInt16LE(0, to: &strFormula)
    BitPacking.appendUInt32LE(0, to: &strFormula)
    BitPacking.appendUInt16LE(0, to: &strFormula)
    appendRecord(opcode: LegacyOLE2OfficeCodec.biff8OpcodeFormula, payload: strFormula)

    var strRecord = Data()
    let formulaStrBytes = Data("FormulaResultText".utf8)
    BitPacking.appendUInt16LE(UInt16(formulaStrBytes.count), to: &strRecord)
    strRecord.append(0x00)
    strRecord.append(formulaStrBytes)
    appendRecord(opcode: LegacyOLE2OfficeCodec.biff8OpcodeString, payload: strRecord)

    var boolFormula = Data()
    BitPacking.appendUInt16LE(0, to: &boolFormula)
    BitPacking.appendUInt16LE(1, to: &boolFormula)
    BitPacking.appendUInt16LE(0, to: &boolFormula)
    boolFormula.append(contentsOf: [0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0xFF, 0xFF])
    appendRecord(opcode: LegacyOLE2OfficeCodec.biff8OpcodeFormula, payload: boolFormula)

    var errFormula = Data()
    BitPacking.appendUInt16LE(0, to: &errFormula)
    BitPacking.appendUInt16LE(2, to: &errFormula)
    BitPacking.appendUInt16LE(0, to: &errFormula)
    errFormula.append(contentsOf: [0x02, 0x00, 0x07, 0x00, 0x00, 0x00, 0xFF, 0xFF])
    appendRecord(opcode: LegacyOLE2OfficeCodec.biff8OpcodeFormula, payload: errFormula)

    appendRecord(opcode: LegacyOLE2OfficeCodec.biff8OpcodeEOF, payload: Data())

    appendRecord(opcode: LegacyOLE2OfficeCodec.biff8OpcodeBOF, payload: makeBOF(substreamType: 0x0010))
    var numRecord = Data()
    BitPacking.appendUInt16LE(0, to: &numRecord)
    BitPacking.appendUInt16LE(0, to: &numRecord)
    BitPacking.appendUInt16LE(0, to: &numRecord)
    BitPacking.appendUInt64LE(Double(420.0).bitPattern, to: &numRecord)
    appendRecord(opcode: LegacyOLE2OfficeCodec.biff8OpcodeNumber, payload: numRecord)
    appendRecord(opcode: LegacyOLE2OfficeCodec.biff8OpcodeEOF, payload: Data())

    let xlsData = try OLE2ContainerCodec.pack(streams: [
      OLE2StreamEntry(name: "Workbook", data: stream)
    ])
    let decoded = try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: xlsData)
    XCTAssertEqual(decoded.name, "AlphaSheet")
    XCTAssertEqual(decoded.allSheetNames, ["AlphaSheet", "BetaSheet"])
    XCTAssertEqual(decoded.rawValue(at: "A1"), "FormulaResultText")
    XCTAssertEqual(decoded.rawValue(at: "B1"), "TRUE")
    XCTAssertEqual(decoded.rawValue(at: "C1"), "#DIV/0!")
    XCTAssertEqual(decoded.additionalSheets.count, 1)
    XCTAssertEqual(decoded.additionalSheets[0].name, "BetaSheet")
    XCTAssertEqual(
      decoded.additionalSheets[0].cellsByCoordinate[PackedCellCoordinate(row: 0, column: 0)],
      "420"
    )
  }

  func testLegacyPPTNestedPPDrawingEscherClientTextboxDecoding() throws {
    func makePPTAtom(verAndInst: UInt16, recType: UInt16, payload: Data) -> Data {
      var out = Data()
      BitPacking.appendUInt16LE(verAndInst, to: &out)
      BitPacking.appendUInt16LE(recType, to: &out)
      BitPacking.appendUInt32LE(UInt32(payload.count), to: &out)
      out.append(payload)
      return out
    }

    let titleAtom = makePPTAtom(
      verAndInst: 0x0000,
      recType: LegacyOLE2OfficeCodec.pptRecordTextCharsAtom,
      payload: BitPacking.encodeUTF16LE("Nested Escher Slide Title")
    )
    let clientTextbox1 = makePPTAtom(
      verAndInst: 0x000F,
      recType: LegacyOLE2OfficeCodec.pptRecordEscherClientTextbox,
      payload: titleAtom
    )
    let spContainer1 = makePPTAtom(
      verAndInst: 0x000F,
      recType: LegacyOLE2OfficeCodec.pptRecordEscherSpContainer,
      payload: clientTextbox1
    )

    let bodyBytesAtom = makePPTAtom(
      verAndInst: 0x0000,
      recType: LegacyOLE2OfficeCodec.pptRecordTextBytesAtom,
      payload: Data("Bullet One\rBullet Two".utf8)
    )
    let clientTextbox2 = makePPTAtom(
      verAndInst: 0x000F,
      recType: LegacyOLE2OfficeCodec.pptRecordEscherClientTextbox,
      payload: bodyBytesAtom
    )
    let spContainer2 = makePPTAtom(
      verAndInst: 0x000F,
      recType: LegacyOLE2OfficeCodec.pptRecordEscherSpContainer,
      payload: clientTextbox2
    )

    var spgrPayload = Data()
    spgrPayload.append(spContainer1)
    spgrPayload.append(spContainer2)
    let spgrContainer = makePPTAtom(
      verAndInst: 0x000F,
      recType: LegacyOLE2OfficeCodec.pptRecordEscherSpgrContainer,
      payload: spgrPayload
    )
    let dgContainer = makePPTAtom(
      verAndInst: 0x000F,
      recType: LegacyOLE2OfficeCodec.pptRecordEscherDgContainer,
      payload: spgrContainer
    )
    let ppDrawing = makePPTAtom(
      verAndInst: 0x000F,
      recType: LegacyOLE2OfficeCodec.pptRecordPPDrawing,
      payload: dgContainer
    )
    let slideContainer = makePPTAtom(
      verAndInst: 0x000F,
      recType: LegacyOLE2OfficeCodec.pptRecordSlide,
      payload: ppDrawing
    )
    let docContainer = makePPTAtom(
      verAndInst: 0x000F,
      recType: LegacyOLE2OfficeCodec.pptRecordDocument,
      payload: slideContainer
    )

    let pptData = try OLE2ContainerCodec.pack(streams: [
      OLE2StreamEntry(name: "PowerPoint Document", data: docContainer)
    ])
    let decodedDeck = try LegacyOLE2OfficeCodec.decodePowerPointBinary(from: pptData)
    XCTAssertEqual(decodedDeck.slides.count, 1)
    XCTAssertEqual(decodedDeck.slides[0].title, "Nested Escher Slide Title")
    XCTAssertEqual(decodedDeck.slides[0].bulletPoints, ["Bullet One", "Bullet Two"])
    XCTAssertEqual(decodedDeck.slides[0].elements.count, 2)
    XCTAssertEqual(decodedDeck.slides[0].elements[0].bodyText, "Nested Escher Slide Title")
    XCTAssertEqual(decodedDeck.slides[0].elements[1].bodyText, "Bullet One\rBullet Two")
  }

  func testOLE2HierarchicalStorageAndStandardHWP5BinaryRecords() throws {
    var fileHeader = Data(repeating: 0, count: HWP5BinaryCodec.fileHeaderByteLength)
    for (idx, b) in Array(HWP5BinaryCodec.signatureString.utf8).enumerated() {
      fileHeader[idx] = b
    }
    try BitPacking.writeUInt32LE(HWP5BinaryCodec.defaultVersion, into: &fileHeader, at: 32)
    try BitPacking.writeUInt32LE(0, into: &fileHeader, at: 36)

    // Standard 26-byte HWPTAG_DOCUMENT_PROPERTIES (sectionCount = 1 at offset 0)
    var stdDocProps = Data(repeating: 0, count: 26)
    try BitPacking.writeUInt16LE(1, into: &stdDocProps, at: 0)
    var docInfoStream = try HWP5RecordHeader.encode(
      tagID: HWP5RecordHeader.tagDocumentProperties,
      level: 0,
      payloadSize: 26
    )
    docInfoStream.append(stdDocProps)

    // Standard 24-byte HWPTAG_PARA_HEADER + HWPTAG_PARA_TEXT with 8-WCHAR control (0x000B) & trailing 0x000D
    let stdParaHeader = Data(repeating: 0, count: 24)
    var section0Stream = try HWP5RecordHeader.encode(
      tagID: HWP5RecordHeader.tagParaHeader,
      level: 0,
      payloadSize: 24
    )
    section0Stream.append(stdParaHeader)

    var paraTextPayload = BitPacking.encodeUTF16LE("표준 한글 본문")
    // Append 8-WCHAR extended control (code 0x000B at +0 and +14)
    var controlBlock = Data(repeating: 0, count: 16)
    try BitPacking.writeUInt16LE(0x000B, into: &controlBlock, at: 0)
    try BitPacking.writeUInt16LE(0x000B, into: &controlBlock, at: 14)
    paraTextPayload.append(controlBlock)
    paraTextPayload.append(BitPacking.encodeUTF16LE(" 정상 추출"))
    BitPacking.appendUInt16LE(0x000D, to: &paraTextPayload)

    let textRec = try HWP5RecordHeader.encode(
      tagID: HWP5RecordHeader.tagParaText,
      level: 1,
      payloadSize: UInt32(paraTextPayload.count)
    )
    section0Stream.append(textRec)
    section0Stream.append(paraTextPayload)

    // Pack hierarchical stream "BodyText/Section0" so OLE2ContainerCodec writes storage "BodyText" (objectType = 1) -> child stream "Section0"
    let packed = try OLE2ContainerCodec.pack(streams: [
      OLE2StreamEntry(name: "FileHeader", data: fileHeader),
      OLE2StreamEntry(name: "DocInfo", data: docInfoStream),
      OLE2StreamEntry(name: "BodyText/Section0", data: section0Stream),
    ])

    let unpackedStreams = try OLE2ContainerCodec.unpack(from: packed)
    XCTAssertTrue(unpackedStreams.contains(where: { $0.name == "BodyText/Section0" }))

    let decodedDoc = try HWP5BinaryCodec.decodeDocument(from: packed)
    XCTAssertEqual(decodedDoc.paragraphs.count, 1)
    XCTAssertEqual(decodedDoc.paragraphs[0].text, "표준 한글 본문 정상 추출")
  }

  func testODFHeadingExtractionAndOpenFormulaCrossSheetNormalization() throws {
    let contentXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
        <office:body>
          <office:text>
            <text:h text:outline-level="1">제1장 서론</text:h>
            <text:p>본문 단락입니다.</text:p>
            <text:h text:outline-level="2">제1절 배경</text:h>
          </office:text>
        </office:body>
      </office:document-content>
      """
    let odtBytes = try ZipContainerCodec.pack(entries: [
      ZipContainerEntry(
        path: "mimetype",
        data: Data(ODFContainerCodec.textMimeType.utf8),
        compressionMethod: .store
      ),
      ZipContainerEntry(
        path: "content.xml",
        data: Data(contentXML.utf8),
        compressionMethod: .deflate
      ),
    ])
    let decodedODT = try ODFContainerCodec.decodeDocument(from: odtBytes)
    XCTAssertEqual(decodedODT.paragraphs.count, 3)
    XCTAssertEqual(decodedODT.paragraphs[0].style, .heading1)
    XCTAssertEqual(decodedODT.paragraphs[0].text, "제1장 서론")
    XCTAssertEqual(decodedODT.paragraphs[1].style, .body)
    XCTAssertEqual(decodedODT.paragraphs[2].style, .heading2)
    XCTAssertEqual(decodedODT.paragraphs[2].text, "제1절 배경")

    let normalizedRange = XMLDocumentModelMapper.normalizeOpenFormulaExpression(
      "of:=SUM([Sheet2.A1:Sheet2.A3])"
    )
    XCTAssertEqual(normalizedRange, "=SUM(Sheet2!A1:A3)")
  }

  func testFormulaASTScientificNotationErrorLiteralsAndLeadingApostrophe() throws {
    var sheet = WorksheetModel(name: "ScientificAndErrors")
    try sheet.setCell("A1", rawValue: "=1E-5+2.5e+2")
    try sheet.setCell("A2", rawValue: "=IFNA(#N/A, \"대체완료\")")
    try sheet.setCell("A3", rawValue: "=ISERROR(#DIV/0!)")
    try sheet.setCell("A4", rawValue: "'=10+20")

    XCTAssertEqual(sheet.evaluate(at: "A1").numericValue ?? 0.0, 250.00001, accuracy: 1e-8)
    XCTAssertEqual(sheet.evaluate(at: "A2"), .text("대체완료"))
    XCTAssertEqual(sheet.evaluate(at: "A3"), .boolean(true))
    XCTAssertEqual(sheet.evaluate(at: "A4"), .text("=10+20"))
  }

  func testSheetSortShiftsRelativeRowFormulasAndSplitColumnEscapesFormulaInjection() throws {
    var sheet = WorksheetModel(name: "SortAndSplit")
    try sheet.setCell("A1", rawValue: "품목")
    try sheet.setCell("B1", rawValue: "단가")
    try sheet.setCell("C1", rawValue: "수량")
    try sheet.setCell("D1", rawValue: "금액")

    try sheet.setCell("A2", rawValue: "나-항목")
    try sheet.setCell("B2", rawValue: "10")
    try sheet.setCell("C2", rawValue: "3")
    try sheet.setCell("D2", rawValue: "=B2*C2")

    try sheet.setCell("A3", rawValue: "가-항목")
    try sheet.setCell("B3", rawValue: "50")
    try sheet.setCell("C3", rawValue: "4")
    try sheet.setCell("D3", rawValue: "=B3*C3")

    sheet.sortRows(byColumn: 0, ascending: true, hasHeader: true)
    XCTAssertEqual(sheet.rawValue(at: "A2"), "가-항목")
    XCTAssertEqual(sheet.rawValue(at: "D2"), "=B2*C2")
    XCTAssertEqual(sheet.evaluate(at: "D2"), .number(200.0))
    XCTAssertEqual(sheet.rawValue(at: "A3"), "나-항목")
    XCTAssertEqual(sheet.rawValue(at: "D3"), "=B3*C3")
    XCTAssertEqual(sheet.evaluate(at: "D3"), .number(30.0))

    var splitSheet = WorksheetModel(name: "SplitInjection")
    try splitSheet.setCell("A1", rawValue: "홍길동, =99+1")
    try splitSheet.setCell("A2", rawValue: "이순신,\n=40+2")
    let splitRows = splitSheet.splitColumnByDelimiter(column: 0, delimiter: ",")
    XCTAssertEqual(splitRows, 2)
    XCTAssertEqual(splitSheet.evaluate(at: "B1"), .text("=99+1"))
    XCTAssertEqual(splitSheet.evaluate(at: "B2"), .text("=40+2"))
  }

  func testVLOOKUPHLOOKUPDefaultApproximateMatchAndErrorPropagation() throws {
    var sheet = WorksheetModel(name: "LookupParity")
    try sheet.setCell("A1", rawValue: "10")
    try sheet.setCell("B1", rawValue: "Bronze")
    try sheet.setCell("A2", rawValue: "20")
    try sheet.setCell("B2", rawValue: "Silver")
    try sheet.setCell("A3", rawValue: "30")
    try sheet.setCell("B3", rawValue: "Gold")

    try sheet.setCell("D1", rawValue: "=VLOOKUP(25, A1:B3, 2)")
    try sheet.setCell("D2", rawValue: "=MATCH(25, A1:A3)")
    try sheet.setCell("D3", rawValue: "=VLOOKUP(#N/A, A1:B3, 2, FALSE)")
    try sheet.setCell("D4", rawValue: "=MATCH(#DIV/0!, A1:A3, 0)")
    try sheet.setCell("D5", rawValue: "=XLOOKUP(#NAME?, A1:A3, B1:B3)")

    XCTAssertEqual(sheet.evaluate(at: "D1"), .text("Silver"))
    XCTAssertEqual(sheet.evaluate(at: "D2"), .number(2.0))
    XCTAssertEqual(sheet.evaluate(at: "D3"), .error("#N/A"))
    XCTAssertEqual(sheet.evaluate(at: "D4"), .error("#DIV/0!"))
    XCTAssertEqual(sheet.evaluate(at: "D5"), .error("#NAME?"))
  }

  func testDocumentSecurityAuditorScrubPayloadUpdatesSheetCompiledASTs() throws {
    var sheet = WorksheetModel(name: "PrivacySheet")
    try sheet.setCell("A1", rawValue: "010-1234-5678")
    try sheet.setCell("B1", rawValue: "=A1")
    XCTAssertEqual(sheet.evaluate(at: "B1"), .text("010-1234-5678"))

    let (scrubbedPayload, removedCount) = DocumentSecurityAuditor.scrubPayload(.sheet(sheet))
    XCTAssertGreaterThan(removedCount, 0)
    guard case .sheet(let scrubbedSheet) = scrubbedPayload else {
      return XCTFail("Expected sheet payload")
    }
    XCTAssertEqual(scrubbedSheet.evaluate(at: "A1"), .text("[연락처 비식별]"))
    XCTAssertEqual(scrubbedSheet.evaluate(at: "B1"), .text("[연락처 비식별]"))
  }

  func testBitLevelMortonUTF16LESWARAndXMLSanitizerSWARContinuation() throws {
    let asciiText = "EasyOffice_SWAR_Morton_Interleave_2026_Test_0123456789!"
    let encodedASCII = BitPacking.encodeUTF16LE(asciiText)
    XCTAssertEqual(encodedASCII.count, asciiText.utf16.count * 2)
    let decodedASCII = try BitPacking.decodeUTF16LE(
      from: encodedASCII,
      at: 0,
      byteLength: encodedASCII.count
    )
    XCTAssertEqual(decodedASCII, asciiText)

    let mixedText = "StandardPrefix_한글과_이모지_😀_Suffix_0123456789"
    var packet = Data([0xAA, 0xBB])
    BitPacking.appendLengthPrefixedUTF16LE(mixedText, to: &packet)
    var readOffset = 2
    let decodedMixed = try BitPacking.readLengthPrefixedUTF16LE(from: packet, offset: &readOffset)
    XCTAssertEqual(decodedMixed, mixedText)
    XCTAssertEqual(readOffset, packet.count)

    let xmlInput = "<tag attr=\"val\">" + String(repeating: "PlainBodyText1234", count: 16) + "&tail</tag>"
    let escaped = XMLSanitizer.escape(xmlInput)
    XCTAssertTrue(escaped.hasPrefix("&lt;tag attr=&quot;val&quot;&gt;PlainBodyText1234"))
    XCTAssertTrue(escaped.hasSuffix("&amp;tail&lt;/tag&gt;"))
  }

  func testBIFF8ExtendedMetadataChunkingOver64KBAndFinancialWorkdayParity() throws {
    var largeSheet = WorksheetModel(name: "PrimarySheet")
    try largeSheet.setCell("A1", rawValue: "헤더")
    let paddingNotes = String(repeating: "가나다라마바사아자차카타파하_0123456789_", count: 1500)
    largeSheet.addSheetPage(
      named: "SecondSheetOver64KB",
      initialCells: [
        PackedCellCoordinate(row: 0, column: 0): paddingNotes,
        PackedCellCoordinate(row: 1, column: 0): "보존검증",
      ]
    )
    let xlsBytes = try LegacyOLE2OfficeCodec.encodeBIFF8Worksheet(largeSheet)
    let roundtripped = try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: xlsBytes)
    XCTAssertEqual(roundtripped.allSheetNames, ["PrimarySheet", "SecondSheetOver64KB"])
    var switched = roundtripped
    switched.switchActiveSheet(to: 1)
    XCTAssertEqual(switched.rawValue(at: "A2"), "보존검증")
    XCTAssertEqual(switched.rawValue(at: "A1").count, paddingNotes.count)

    var finSheet = WorksheetModel(name: "FinAndDate")
    try finSheet.setCell("A1", rawValue: "2026-05-01")
    try finSheet.setCell("A2", rawValue: "2026-05-05")
    try finSheet.setCell("B1", rawValue: "=WORKDAY(\"2026-04-30\", 2, A1:A2)")
    try finSheet.setCell("B2", rawValue: "=NETWORKDAYS(\"2026-04-30\", \"2026-05-06\", A1:A2)")
    try finSheet.setCell("C1", rawValue: "-1000")
    try finSheet.setCell("C2", rawValue: "0")
    try finSheet.setCell("C3", rawValue: "1210")
    try finSheet.setCell("C4", rawValue: "=IRR(C1:C3, -0.5)")
    XCTAssertEqual(finSheet.evaluate(at: "B1"), .number(46148.0))
    XCTAssertEqual(finSheet.evaluate(at: "B2"), .number(3.0))
    XCTAssertEqual(finSheet.evaluate(at: "C4").numericValue ?? 0.0, 0.10, accuracy: 1e-3)
  }
}
