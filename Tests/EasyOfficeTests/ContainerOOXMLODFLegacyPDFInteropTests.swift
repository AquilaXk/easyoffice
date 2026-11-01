import Foundation
import XCTest

@testable import EasyOfficeCore

final class ContainerOOXMLODFLegacyPDFInteropTests: XCTestCase {


  func testCheckedBitPackingRejectsIntegerOverflowAndAcceptsBoundaries() throws {
    XCTAssertEqual(try BitPacking.checkedUInt16(0, field: "u16.min"), 0)
    XCTAssertEqual(try BitPacking.checkedUInt16(65_535, field: "u16.max"), 65_535)
    XCTAssertThrowsError(try BitPacking.checkedUInt16(-1, field: "u16.neg")) { error in
      guard case EasyOfficeError.resourceBudgetExceeded(let detail) = error else {
        return XCTFail("Expected resourceBudgetExceeded, got \(error)")
      }
      XCTAssertTrue(detail.contains("u16.neg"))
    }
    XCTAssertThrowsError(try BitPacking.checkedUInt16(65_536, field: "u16.overflow"))

    XCTAssertEqual(try BitPacking.checkedUInt32(0, field: "u32.min"), 0)
    XCTAssertEqual(
      try BitPacking.checkedUInt32(Int(UInt32.max), field: "u32.max"),
      UInt32.max
    )
    XCTAssertThrowsError(try BitPacking.checkedUInt32(-1, field: "u32.neg"))
    XCTAssertThrowsError(
      try BitPacking.checkedUInt32(Int(UInt32.max) + 1, field: "u32.overflow")
    )
  }

  func testZipContainerRejectsCaseInsensitiveDuplicatesAndFileDirectoryCollisions() throws {
    XCTAssertThrowsError(
      try ZipContainerCodec.pack(entries: [
        ZipContainerEntry(path: "word/document.xml", data: Data("a".utf8)),
        ZipContainerEntry(path: "WORD/DOCUMENT.XML", data: Data("b".utf8)),
      ])
    ) { error in
      guard case EasyOfficeError.securityViolation(let detail) = error else {
        return XCTFail("Expected securityViolation, got \(error)")
      }
      XCTAssertTrue(detail.contains("중복"))
    }

    XCTAssertThrowsError(
      try ZipContainerCodec.pack(entries: [
        ZipContainerEntry(path: "word/media", data: Data("file".utf8)),
        ZipContainerEntry(path: "word/media/image1.png", data: Data("nested".utf8)),
      ])
    ) { error in
      guard case EasyOfficeError.securityViolation(let detail) = error else {
        return XCTFail("Expected securityViolation, got \(error)")
      }
      XCTAssertTrue(detail.contains("충돌"))
    }

    XCTAssertThrowsError(
      try ZipContainerCodec.pack(entries: [
        ZipContainerEntry(path: "word/media/image1.png", data: Data("nested".utf8)),
        ZipContainerEntry(path: "word/media", data: Data("file".utf8)),
      ])
    )
  }

  func testZipContainerUnpackCrossVerifiesLFHAgainstCentralDirectory() throws {
    let validArchive = try ZipContainerCodec.pack(entries: [
      ZipContainerEntry(path: "content.xml", data: Data("hello-world-12345".utf8))
    ])

    var tamperedName = validArchive
    tamperedName[30] = UInt8(ascii: "x")
    XCTAssertThrowsError(try ZipContainerCodec.unpack(from: tamperedName)) { error in
      guard case EasyOfficeError.securityViolation(let detail) = error else {
        return XCTFail("Expected securityViolation, got \(error)")
      }
      XCTAssertTrue(detail.contains("일치하지 않습니다"))
    }

    var tamperedCRC = validArchive
    tamperedCRC[14] ^= 0xFF
    XCTAssertThrowsError(try ZipContainerCodec.unpack(from: tamperedCRC)) { error in
      guard case EasyOfficeError.corruptedContainer(let detail) = error else {
        return XCTFail("Expected corruptedContainer, got \(error)")
      }
      XCTAssertTrue(detail.contains("CRC"))
    }

    var tamperedMethod = validArchive
    tamperedMethod[8] = 0
    tamperedMethod[9] = 0
    XCTAssertThrowsError(try ZipContainerCodec.unpack(from: tamperedMethod)) { error in
      guard case EasyOfficeError.corruptedContainer(let detail) = error else {
        return XCTFail("Expected corruptedContainer, got \(error)")
      }
      XCTAssertTrue(detail.contains("압축 방식"))
    }
  }


  func testOLE2RejectsDuplicateSiblingNamesAndRoundTripsBoundaryAnd20MBStreams() throws {
    XCTAssertThrowsError(
      try OLE2ContainerCodec.pack(streams: [
        OLE2StreamEntry(name: "Workbook", data: Data([1, 2, 3])),
        OLE2StreamEntry(name: "WORKBOOK", data: Data([4, 5, 6])),
      ])
    )

    // Boundary sector counts: 127, 128, 255, 256 sectors (512 bytes each)
    for sectorCount in [127, 128, 255, 256] {
      let payload = Data((0..<(sectorCount * 512)).map { UInt8($0 & 0xFF) })
      let packed = try OLE2ContainerCodec.pack(streams: [
        OLE2StreamEntry(name: "BoundaryStream", data: payload)
      ])
      let unpacked = try OLE2ContainerCodec.unpack(from: packed)
      XCTAssertEqual(unpacked.count, 1)
      XCTAssertEqual(unpacked.first?.data, payload, "Failed at \(sectorCount) sectors")
    }

    // 20MB stream (> 109 FAT sectors, exercising DIFAT sector chain allocation & traversal)
    let twentyMB = Data(repeating: 0x5A, count: 20 * 1_024 * 1_024)
    let packedLarge = try OLE2ContainerCodec.pack(streams: [
      OLE2StreamEntry(name: "LargePayload", data: twentyMB)
    ])
    let unpackedLarge = try OLE2ContainerCodec.unpack(from: packedLarge)
    XCTAssertEqual(unpackedLarge.first?.data.count, twentyMB.count)
    XCTAssertEqual(unpackedLarge.first?.data, twentyMB)
  }

  func testOLE2DeepSiblingTreeWithoutStackOverflow() throws {
    let entries = (0..<300).map { idx in
      OLE2StreamEntry(name: String(format: "Stream_%04d", idx), data: Data("val-\(idx)".utf8))
    }
    let packed = try OLE2ContainerCodec.pack(streams: entries)
    let unpacked = try OLE2ContainerCodec.unpack(from: packed)
    XCTAssertEqual(unpacked.count, 300)
    var map: [String: Data] = [:]
    for entry in unpacked {
      map[entry.name] = entry.data
    }
    XCTAssertEqual(map["Stream_0000"], Data("val-0".utf8))
    XCTAssertEqual(map["Stream_0299"], Data("val-299".utf8))
  }


  func testDOCXHighlightEnumIgnorableNamespaceAndMediaDrawingRoundTrip() throws {
    let pngBytes = Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x01, 0x02])
    let p1 = ParagraphBlock(
      style: .body,
      text: "형광펜과 이미지",
      runs: [
        DocumentTextRun(text: "형광펜과 ", backgroundHex: "#FFAA12"),
        DocumentTextRun(text: "이미지", backgroundHex: "#FFFF00"),
      ],
      lineSpacingPercent: 135.0
    )
    let p2 = ParagraphBlock(style: .body, text: "두 번째 배너")

    let img1 = EmbeddedImageAttachment(
      fileName: "../unsafe:dir/banner.png",
      mimeType: "image/png",
      imageData: pngBytes,
      widthPt: 240,
      heightPt: 120,
      anchorParagraphID: p1.id
    )
    let img2 = EmbeddedImageAttachment(
      fileName: "banner.png",
      mimeType: "image/png",
      imageData: Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x03, 0x04]),
      widthPt: 180,
      heightPt: 90,
      anchorParagraphID: p2.id
    )

    let model = OfficeDocumentModel(
      title: "OOXML 표준 검증",
      format: .docx,
      paragraphs: [p1, p2],
      images: [img1, img2]
    )
    let docxData = try OOXMLContainerCodec.encodeDocument(model)
    let entries = try ZipContainerCodec.unpack(from: docxData)
    var entryMap: [String: Data] = [:]
    for entry in entries {
      entryMap[entry.path] = entry.data
    }

    let docXML = try XCTUnwrap(
      String(data: XCTUnwrap(entryMap["word/document.xml"]), encoding: .utf8)
    )
    XCTAssertTrue(docXML.contains("mc:Ignorable=\"eo\""))
    XCTAssertTrue(docXML.contains("xmlns:eo=\"https://easyoffice.local/ns/meta\""))
    XCTAssertTrue(docXML.contains("<w:highlight w:val=\"yellow\" eo:highlightHex=\"#FFAA12\"/>"))
    XCTAssertTrue(docXML.contains("<w:drawing>"))
    XCTAssertTrue(docXML.contains("<a:blip r:embed=\"rIdMedia1\"/>"))
    XCTAssertTrue(docXML.contains("<a:blip r:embed=\"rIdMedia2\"/>"))

    let relsXML = try XCTUnwrap(
      String(data: XCTUnwrap(entryMap["word/_rels/document.xml.rels"]), encoding: .utf8)
    )
    XCTAssertTrue(relsXML.contains("Id=\"rIdMedia1\""))
    XCTAssertTrue(relsXML.contains("Target=\"media/banner.png\""))
    XCTAssertTrue(relsXML.contains("Id=\"rIdMedia2\""))
    XCTAssertTrue(relsXML.contains("Target=\"media/banner_2.png\""))
    XCTAssertNotNil(entryMap["word/media/banner.png"])
    XCTAssertNotNil(entryMap["word/media/banner_2.png"])

    let decoded = try OOXMLContainerCodec.decodeDocument(from: docxData)
    XCTAssertEqual(decoded.paragraphs.count, 2)
    XCTAssertEqual(decoded.paragraphs[0].runs.first?.backgroundHex, "#FFAA12")
    XCTAssertEqual(decoded.images.count, 2)
    XCTAssertEqual(decoded.images[0].imageData, pngBytes)
  }

  func testXLSXRelsResolutionPhoneticStrippingAndEmptySSTPreservation() throws {
    let workbookXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
        <sheets>
          <sheet name="매출시트" sheetId="1" r:id="rCustom9"/>
        </sheets>
      </workbook>
      """
    let workbookRelsXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rCustom9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/custom_target.xml"/>
      </Relationships>
      """
    let sharedStringsXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="3" uniqueCount="3">
        <si/>
        <si><t>한글</t><rPh sb="0" eb="2"><t>ハングル</t></rPh></si>
        <si><t>세번째</t></si>
      </sst>
      """
    let sheetXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
        <sheetData>
          <row r="1">
            <c r="A1" t="s"><v>0</v></c>
            <c r="B1" t="s"><v>1</v></c>
            <c r="C1" t="s"><v>2</v></c>
          </row>
        </sheetData>
      </worksheet>
      """
    let xlsxData = try ZipContainerCodec.pack(entries: [
      ZipContainerEntry(path: "[Content_Types].xml", data: Data("<Types/>".utf8)),
      ZipContainerEntry(path: "xl/workbook.xml", data: Data(workbookXML.utf8)),
      ZipContainerEntry(path: "xl/_rels/workbook.xml.rels", data: Data(workbookRelsXML.utf8)),
      ZipContainerEntry(path: "xl/sharedStrings.xml", data: Data(sharedStringsXML.utf8)),
      ZipContainerEntry(path: "xl/worksheets/custom_target.xml", data: Data(sheetXML.utf8)),
    ])

    let decoded = try OOXMLContainerCodec.decodeWorksheet(from: xlsxData)
    XCTAssertEqual(decoded.name, "매출시트")
    XCTAssertEqual(decoded.rawValue(at: "A1"), "")
    XCTAssertEqual(decoded.rawValue(at: "B1"), "한글")
    XCTAssertEqual(decoded.rawValue(at: "C1"), "세번째")
  }

  func testPPTXIncludesSlideMasterAndSlideLayoutParts() throws {
    let deck = PresentationDeckModel(
      title: "마스터 검증",
      slides: [SlideModel(title: "첫 슬라이드")]
    )
    let pptxData = try OOXMLContainerCodec.encodePresentation(deck)
    let entries = try ZipContainerCodec.unpack(from: pptxData)
    let paths = Set(entries.map { $0.path })
    XCTAssertTrue(paths.contains("ppt/slideMasters/slideMaster1.xml"))
    XCTAssertTrue(paths.contains("ppt/slideMasters/_rels/slideMaster1.xml.rels"))
    XCTAssertTrue(paths.contains("ppt/slideLayouts/slideLayout1.xml"))
    XCTAssertTrue(paths.contains("ppt/slideLayouts/_rels/slideLayout1.xml.rels"))
    XCTAssertTrue(paths.contains("ppt/slides/_rels/slide1.xml.rels"))

    let presXML = try XCTUnwrap(
      String(
        data: XCTUnwrap(entries.first(where: { $0.path == "ppt/presentation.xml" })?.data),
        encoding: .utf8
      )
    )
    XCTAssertTrue(presXML.contains("<p:sldMasterIdLst>"))
  }


  func testODSOpenFormulaNormalizationAndCoveredCellColumnAdvance() throws {
    var sheet = WorksheetModel(name: "예산표")
    try sheet.setCell("A1", rawValue: "10")
    try sheet.setCell("B1", rawValue: "20")
    try sheet.setCell("C1", rawValue: "=SUM(A1:B1)")

    let odsData = try ODFContainerCodec.encodeWorksheet(sheet)
    let entries = try ZipContainerCodec.unpack(from: odsData)
    let contentXML = try XCTUnwrap(
      String(
        data: XCTUnwrap(entries.first(where: { $0.path == "content.xml" })?.data),
        encoding: .utf8
      )
    )
    XCTAssertTrue(contentXML.contains("table:formula=\"of:=SUM([.A1:.B1])\""))

    let decoded = try ODFContainerCodec.decodeWorksheet(from: odsData)
    XCTAssertEqual(decoded.rawValue(at: "C1"), "=SUM(A1:B1)")
    XCTAssertEqual(decoded.evaluate(at: "C1"), .number(30.0))

    // Covered table cell advancing column offset
    let coveredContentXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
        <office:body><office:spreadsheet>
          <table:table table:name="병합테스트">
            <table:table-row>
              <table:table-cell office:value-type="string"><text:p>시작</text:p></table:table-cell>
              <table:covered-table-cell table:number-columns-repeated="2"/>
              <table:table-cell office:value-type="string"><text:p>네번째열</text:p></table:table-cell>
            </table:table-row>
          </table:table>
        </office:spreadsheet></office:body>
      </office:document-content>
      """
    let customODS = try ZipContainerCodec.pack(entries: [
      ZipContainerEntry(
        path: "mimetype",
        data: Data("application/vnd.oasis.opendocument.spreadsheet".utf8)
      ),
      ZipContainerEntry(path: "content.xml", data: Data(coveredContentXML.utf8)),
    ])
    let decodedCovered = try ODFContainerCodec.decodeWorksheet(from: customODS)
    XCTAssertEqual(decodedCovered.rawValue(at: "A1"), "시작")
    XCTAssertEqual(decodedCovered.rawValue(at: "D1"), "네번째열")
  }

  func testODTAndODPPreserveTitleInMetaXMLAndExcludeNotesFromSlideBody() throws {
    let doc = OfficeDocumentModel(
      title: "고유 문서 제목",
      format: .odt,
      paragraphs: [ParagraphBlock(style: .body, text: "첫 문단 본문")]
    )
    let odtData = try ODFContainerCodec.encodeDocument(doc)
    let decodedDoc = try ODFContainerCodec.decodeDocument(from: odtData)
    XCTAssertEqual(decodedDoc.title, "고유 문서 제목")
    XCTAssertEqual(decodedDoc.paragraphs.first?.text, "첫 문단 본문")

    let deck = PresentationDeckModel(
      title: "발표 제목 보존",
      slides: [
        SlideModel(
          title: "슬라이드 1",
          speakerNotes: "발표자 전용 비밀 노트",
          elements: [
            SlideCanvasElement(
              title: "요소 제목",
              bodyText: "본문 요소",
              x: 60,
              y: 120,
              width: 400,
              height: 80
            )
          ]
        )
      ]
    )
    let odpData = try ODFContainerCodec.encodePresentation(deck)
    let decodedDeck = try ODFContainerCodec.decodePresentation(from: odpData)
    XCTAssertEqual(decodedDeck.title, "발표 제목 보존")
    XCTAssertEqual(decodedDeck.slides.first?.speakerNotes, "발표자 전용 비밀 노트")
    let elementTexts = decodedDeck.slides.first?.elements.map { $0.bodyText } ?? []
    XCTAssertFalse(elementTexts.contains(where: { $0.contains("발표자 전용 비밀 노트") }))
  }


  func testHWP5MultiSectionTableTag61AndMaxTableRowsBudget() throws {
    let table = DocumentTableBlock(
      caption: "표준 표",
      rows: [["항목", "수량"], ["노트북", "5"]]
    )
    let model = OfficeDocumentModel(
      title: "다중 구역 문서",
      format: .hwp,
      paragraphs: [
        ParagraphBlock(style: .heading1, text: "1구역 제목"),
        ParagraphBlock(style: .body, text: "2구역 본문"),
      ],
      tables: [table],
      sections: [
        DocumentSectionLayout(paperSize: .a4),
        DocumentSectionLayout(paperSize: .a4),
      ]
    )

    let packed = try HWP5BinaryCodec.encodeDocument(model)
    let streams = try OLE2ContainerCodec.unpack(from: packed)
    let streamNames = Set(streams.map { $0.name })
    XCTAssertTrue(streamNames.contains("BodyText/Section0"))
    XCTAssertTrue(streamNames.contains("BodyText/Section1"))

    let unpacked = try HWP5BinaryCodec.decodeDocument(from: packed)
    XCTAssertEqual(unpacked.title, "다중 구역 문서")
    XCTAssertEqual(unpacked.tables.first?.rows, [["항목", "수량"], ["노트북", "5"]])
    XCTAssertEqual(unpacked.sections.count, 2)

    // Oversized table (> 4,096 rows) must throw EasyOfficeError.resourceBudgetExceeded
    let oversizedRows = Array(repeating: ["A"], count: HWP5BinaryCodec.maxHWPTableRows + 1)
    let oversizedModel = OfficeDocumentModel(
      title: "초과 표",
      format: .hwp,
      paragraphs: [ParagraphBlock(style: .body, text: "본문")],
      tables: [DocumentTableBlock(caption: "대형표", rows: oversizedRows)]
    )
    XCTAssertThrowsError(try HWP5BinaryCodec.encodeDocument(oversizedModel)) { error in
      guard case EasyOfficeError.resourceBudgetExceeded = error else {
        return XCTFail("Expected resourceBudgetExceeded, got \(error)")
      }
    }
  }


  func testWord97CLXPieceTableRoundTripAndMixedCompressedPieces() throws {
    let doc = OfficeDocumentModel(
      title: "Word97 CLX 문서",
      format: .doc,
      paragraphs: [
        ParagraphBlock(style: .heading1, text: "첫 번째 제목"),
        ParagraphBlock(style: .body, text: "Word97 CLX 조각 테이블 본문입니다."),
      ]
    )
    let encoded = try LegacyOLE2OfficeCodec.encodeWordBinary(doc)
    let streams = try OLE2ContainerCodec.unpack(from: encoded)
    XCTAssertTrue(streams.contains(where: { $0.name == "1Table" }))

    let decoded = try LegacyOLE2OfficeCodec.decodeWordBinary(from: encoded)
    XCTAssertEqual(decoded.title, "Word97 CLX 문서")
    XCTAssertTrue(decoded.paragraphs.contains(where: { $0.text.contains("Word97 CLX 조각 테이블 본문입니다.") }))
  }

  func testPowerPoint97SplitsSlidesOnSlideContainerRecord0x03EE() throws {
    func makeRecord(verInstance: UInt16, recType: UInt16, payload: Data) -> Data {
      var out = Data()
      BitPacking.appendUInt16LE(verInstance, to: &out)
      BitPacking.appendUInt16LE(recType, to: &out)
      BitPacking.appendUInt32LE(UInt32(payload.count), to: &out)
      out.append(payload)
      return out
    }
    func makeUTF16Atom(_ text: String) -> Data {
      var payload = Data()
      for unit in text.utf16 {
        BitPacking.appendUInt16LE(unit, to: &payload)
      }
      return makeRecord(verInstance: 0, recType: 4000, payload: payload)
    }

    let slide1 = makeRecord(verInstance: 0x000F, recType: 0x03EE, payload: makeUTF16Atom("첫 슬라이드 제목"))
    let slide2 = makeRecord(verInstance: 0x000F, recType: 0x03EE, payload: makeUTF16Atom("두 번째 슬라이드"))
    var slidesPayload = Data()
    slidesPayload.append(slide1)
    slidesPayload.append(slide2)
    let docContainer = makeRecord(verInstance: 0x000F, recType: 1000, payload: slidesPayload)

    let pptData = try OLE2ContainerCodec.pack(streams: [
      OLE2StreamEntry(name: "PowerPoint Document", data: docContainer)
    ])
    let decoded = try LegacyOLE2OfficeCodec.decodePowerPointBinary(from: pptData)
    XCTAssertEqual(decoded.slides.count, 2)
    XCTAssertEqual(decoded.slides[0].title, "첫 슬라이드 제목")
    XCTAssertEqual(decoded.slides[1].title, "두 번째 슬라이드")
  }


  func testBIFF8XLSSSTContinueSplittingLatin1AndCoordinateBudget() throws {
    var sheet = WorksheetModel(name: "대용량문자열")
    // 600 distinct strings of 40 chars each (> 8,224 bytes total SST payload -> forces CONTINUE records)
    for idx in 0..<600 {
      let row = idx / 10
      let col = idx % 10
      let text = String(format: "셀데이터_%04d_한글과영문혼합문자열테스트패리티검증", idx)
      let addr = try CellAddress(column: col, row: row)
      try sheet.setCell(addr.description, rawValue: text)
    }

    let xlsData = try LegacyOLE2OfficeCodec.encodeBIFF8Worksheet(sheet)
    let unpackedSheet = try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: xlsData)
    XCTAssertEqual(unpackedSheet.name, "대용량문자열")
    XCTAssertEqual(
      unpackedSheet.rawValue(at: "A1"),
      "셀데이터_0000_한글과영문혼합문자열테스트패리티검증"
    )
    XCTAssertEqual(
      unpackedSheet.rawValue(at: "J60"),
      "셀데이터_0599_한글과영문혼합문자열테스트패리티검증"
    )

    // Out-of-range BIFF8 row (65,536, 1-based row 65537) and col (256, 1-based col IV+1 = IW) throw resourceBudgetExceeded
    var outOfBoundsRowSheet = WorksheetModel(name: "초과행")
    try outOfBoundsRowSheet.setCell("A65537", rawValue: "초과")
    XCTAssertThrowsError(try LegacyOLE2OfficeCodec.encodeBIFF8Worksheet(outOfBoundsRowSheet)) { error in
      guard case EasyOfficeError.resourceBudgetExceeded = error else {
        return XCTFail("Expected resourceBudgetExceeded, got \(error)")
      }
    }

    var outOfBoundsColSheet = WorksheetModel(name: "초과열")
    try outOfBoundsColSheet.setCell("IW1", rawValue: "초과")
    XCTAssertThrowsError(try LegacyOLE2OfficeCodec.encodeBIFF8Worksheet(outOfBoundsColSheet)) { error in
      guard case EasyOfficeError.resourceBudgetExceeded = error else {
        return XCTFail("Expected resourceBudgetExceeded, got \(error)")
      }
    }
  }


  func testPDFStringEscapingEncryptDetectionToUnicodeAndCanonicalVerification() throws {
    let originalText = "줄바꿈\u{2028}문단\u{2029}다음줄\u{0085}탭\t끝"
    let escaped = PDFBinarySerializer.escapePDFString(originalText)
    XCTAssertFalse(escaped.contains("\u{2028}"))
    XCTAssertFalse(escaped.contains("\u{2029}"))
    XCTAssertFalse(escaped.contains("\u{0085}"))
    XCTAssertEqual(PDFBinarySerializer.unescapePDFString(escaped), originalText)

    // Document containing literal "/Encrypt" in body text must NOT be rejected as encrypted
    let model = PDFStudioDocumentModel(
      title: "보안 명세서",
      pages: [
        PDFPageDescriptor(
          pageNumber: 1,
          rotationDegrees: 0,
          extractedText: "PDF /Encrypt 토큰 설명\n본문 안에 /Encrypt 문자열이 포함되어도 암호화 문서로 오탐하지 않습니다."
        )
      ],
      annotations: [
        PDFAnnotationItem(pageNumber: 1, kind: .stickyNote, author: "검토자", contents: "첫 메모")
      ],
      inkStrokes: [
        PDFInkStroke(
          pageNumber: 1,
          points: [PDFInkPoint(x: 40, y: 40), PDFInkPoint(x: 90, y: 90)],
          strokeHex: "#5C6BC0",
          lineWidth: .nan
        )
      ],
      formFields: [PDFFormField(pageNumber: 1, name: "부서", value: "보안팀")],
      watermarkText: "내부용"
    )

    let pdfData = PDFBinarySerializer.encode(model)
    let pdfASCII = String(decoding: pdfData, as: UTF8.self)
    XCTAssertTrue(pdfASCII.contains("/ToUnicode"))
    XCTAssertTrue(pdfASCII.contains("1.00 w"))

    let parsed = try PDFBinarySerializer.decode(from: pdfData)
    XCTAssertTrue(parsed.pages.first?.extractedText.contains("/Encrypt") == true)

    // Actual /Encrypt dictionary token outside strings must be rejected
    let encryptedBytes = Data("%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Encrypt 9 0 R >>\nendobj\n%%EOF".utf8)
    XCTAssertThrowsError(try PDFBinarySerializer.decode(from: encryptedBytes)) { error in
      guard case EasyOfficeError.encryptedDocumentUnsupported = error else {
        return XCTFail("Expected encryptedDocumentUnsupported, got \(error)")
      }
    }

    // External PDF with /Type /Page but 0 readable page text streams must throw unsupportedFormat
    let unreadableExternalPDF = Data(
      "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R >>\nendobj\n%%EOF".utf8
    )
    XCTAssertThrowsError(try PDFBinarySerializer.decode(from: unreadableExternalPDF)) { error in
      guard case EasyOfficeError.unsupportedFormat = error else {
        return XCTFail("Expected unsupportedFormat, got \(error)")
      }
    }
  }
}
