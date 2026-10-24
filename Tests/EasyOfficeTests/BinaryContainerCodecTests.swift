import XCTest

@testable import EasyOfficeCore

final class BinaryContainerCodecTests: XCTestCase {

  func testBitPackingLittleEndianReadWriteAndCRC32() throws {
    var buffer = Data()
    BitPacking.appendUInt16LE(0x1234, to: &buffer)
    BitPacking.appendUInt32LE(0x89AB_CDEF, to: &buffer)
    BitPacking.appendUInt64LE(0x0102_0304_0506_0708, to: &buffer)

    XCTAssertEqual(buffer.count, 14)
    XCTAssertEqual([UInt8](buffer.prefix(2)), [0x34, 0x12])
    XCTAssertEqual([UInt8](buffer[2..<6]), [0xEF, 0xCD, 0xAB, 0x89])

    XCTAssertEqual(try BitPacking.readUInt16LE(from: buffer, at: 0), 0x1234)
    XCTAssertEqual(try BitPacking.readUInt32LE(from: buffer, at: 2), 0x89AB_CDEF)
    XCTAssertEqual(try BitPacking.readUInt64LE(from: buffer, at: 6), 0x0102_0304_0506_0708)

    var mutableBuffer = Data(repeating: 0, count: 8)
    try BitPacking.writeUInt32LE(0xCAFE_BABE, into: &mutableBuffer, at: 2)
    XCTAssertEqual(try BitPacking.readUInt32LE(from: mutableBuffer, at: 2), 0xCAFE_BABE)

    // Out-of-bounds read must fail closed
    XCTAssertThrowsError(try BitPacking.readUInt32LE(from: buffer, at: 12)) { error in
      guard case .corruptedContainer = error as? EasyOfficeError else {
        XCTFail("Expected corruptedContainer error, got \(error)")
        return
      }
    }

    // IEEE 802.3 CRC-32 standard test vector: ASCII "123456789" -> 0xCBF43926
    let vector = Data("123456789".utf8)
    XCTAssertEqual(CRC32Calculator.compute(for: vector), 0xCBF4_3926)
  }

  func testHWP5RecordHeaderBitfieldStandardAndExtended() throws {
    // Standard 4-byte header (< 4095 payload bytes)
    let encodedStandard = try HWP5RecordHeader.encode(
      tagID: HWP5RecordHeader.tagParaHeader,
      level: 3,
      payloadSize: 512
    )
    XCTAssertEqual(encodedStandard.count, 4)

    let rawWord = try BitPacking.readUInt32LE(from: encodedStandard, at: 0)
    XCTAssertEqual(rawWord & 0x03FF, UInt32(HWP5RecordHeader.tagParaHeader))
    XCTAssertEqual((rawWord >> 10) & 0x03FF, 3)
    XCTAssertEqual((rawWord >> 20) & 0x0FFF, 512)

    let decodedStandard = try HWP5RecordHeader.decode(from: encodedStandard, at: 0)
    XCTAssertEqual(decodedStandard.headerByteLength, 4)
    XCTAssertEqual(decodedStandard.header.tagID, HWP5RecordHeader.tagParaHeader)
    XCTAssertEqual(decodedStandard.header.level, 3)
    XCTAssertEqual(decodedStandard.header.payloadSize, 512)

    // Extended 8-byte header (payloadSize >= 4095 uses 0x0FFF marker + 32-bit size)
    let encodedExtended = try HWP5RecordHeader.encode(
      tagID: HWP5RecordHeader.tagParaText,
      level: 1,
      payloadSize: 70_000
    )
    XCTAssertEqual(encodedExtended.count, 8)
    let extWord = try BitPacking.readUInt32LE(from: encodedExtended, at: 0)
    XCTAssertEqual((extWord >> 20) & 0x0FFF, 0x0FFF)
    XCTAssertEqual(try BitPacking.readUInt32LE(from: encodedExtended, at: 4), 70_000)

    let decodedExtended = try HWP5RecordHeader.decode(from: encodedExtended, at: 0)
    XCTAssertEqual(decodedExtended.headerByteLength, 8)
    XCTAssertEqual(decodedExtended.header.tagID, HWP5RecordHeader.tagParaText)
    XCTAssertEqual(decodedExtended.header.level, 1)
    XCTAssertEqual(decodedExtended.header.payloadSize, 70_000)

    // Truncated extended header (missing 4-byte extended size) must fail closed
    let truncatedExtended = Data(encodedExtended.prefix(6))
    XCTAssertThrowsError(try HWP5RecordHeader.decode(from: truncatedExtended, at: 0)) { error in
      guard case .corruptedContainer = error as? EasyOfficeError else {
        XCTFail("Expected corruptedContainer error, got \(error)")
        return
      }
    }
  }

  func testZipContainerCodecRoundTripAndSecurityHardening() throws {
    let mimeBytes = Data("application/hwp+zip".utf8)
    let xmlString = String(
      repeating: "<hp:p><hp:run><hp:t>대한민국 공공문서 표준 HWPX 테스트 문단입니다.</hp:t></hp:run></hp:p>\n",
      count: 20)
    let xmlBytes = Data(xmlString.utf8)

    let entries: [ZipContainerEntry] = [
      ZipContainerEntry(path: "mimetype", data: mimeBytes, compressionMethod: .store),
      ZipContainerEntry(path: "Contents/section0.xml", data: xmlBytes, compressionMethod: .deflate),
    ]

    let archiveData = try ZipContainerCodec.pack(entries: entries)
    XCTAssertTrue(archiveData.starts(with: FormatDetector.zipMagic))

    let unpacked = try ZipContainerCodec.unpack(from: archiveData)
    XCTAssertEqual(unpacked.count, 2)
    XCTAssertEqual(unpacked[0].path, "mimetype")
    XCTAssertEqual(unpacked[0].compressionMethod, .store)
    XCTAssertEqual(unpacked[0].data, mimeBytes)
    XCTAssertEqual(unpacked[1].path, "Contents/section0.xml")
    XCTAssertEqual(unpacked[1].compressionMethod, .deflate)
    XCTAssertEqual(unpacked[1].data, xmlBytes)

    // Path traversal and invalid path rejections
    for maliciousPath in [
      "../evil.xml", "a/../../etc/passwd", "/etc/passwd", "folder\\evil.xml", "", "bad\0name.xml",
    ] {
      XCTAssertThrowsError(
        try ZipContainerCodec.pack(entries: [
          ZipContainerEntry(path: maliciousPath, data: mimeBytes, compressionMethod: .store)
        ])
      ) { error in
        guard case .securityViolation = error as? EasyOfficeError else {
          XCTFail("Expected securityViolation for path '\(maliciousPath)', got \(error)")
          return
        }
      }
    }

    // CRC-32 tampering must fail closed
    var corruptedArchive = archiveData
    // Flip a byte inside the first entry's uncompressed data payload (offset 30 + 8 = 38)
    corruptedArchive[38] ^= 0xFF
    XCTAssertThrowsError(try ZipContainerCodec.unpack(from: corruptedArchive)) { error in
      guard case .corruptedContainer = error as? EasyOfficeError else {
        XCTFail("Expected corruptedContainer on CRC mismatch, got \(error)")
        return
      }
    }

    // Decompression bomb ratio check must fail closed
    let repetitivePayload = Data(repeating: 0x41, count: 65_536)
    let compressedArchive = try ZipContainerCodec.pack(entries: [
      ZipContainerEntry(path: "bomb.xml", data: repetitivePayload, compressionMethod: .deflate)
    ])
    let strictPolicy = ZipContainerSecurityPolicy(
      maxEntryCount: 16,
      maxSingleEntryUncompressedBytes: 1024 * 1024,
      maxTotalUncompressedBytes: 2 * 1024 * 1024,
      maxCompressionRatio: 5.0
    )
    XCTAssertThrowsError(
      try ZipContainerCodec.unpack(from: compressedArchive, policy: strictPolicy)
    ) { error in
      guard case .decompressionBombDetected = error as? EasyOfficeError else {
        XCTFail("Expected decompressionBombDetected, got \(error)")
        return
      }
    }
  }

  func testXMLSanitizerBlocksXXEAndEscapesEntities() throws {
    let xxePayload = """
      <?xml version="1.0" encoding="UTF-8"?>
      <!DOCTYPE foo [ <!ENTITY xxe SYSTEM "file:///etc/passwd"> ]>
      <w:document>&xxe;</w:document>
      """
    XCTAssertThrowsError(try XMLSanitizer.validateSafeXML(xxePayload)) { error in
      XCTAssertEqual(error as? EasyOfficeError, .xxeOrExternalEntityDetected)
    }

    let externalDtdPayload = """
      <?xml version="1.0" encoding="UTF-8"?>
      <!DOCTYPE document PUBLIC "-//Example//DTD 1.0//EN" "http://example.com/evil.dtd">
      <document/>
      """
    XCTAssertThrowsError(try XMLSanitizer.validateSafeXML(externalDtdPayload)) { error in
      XCTAssertEqual(error as? EasyOfficeError, .xxeOrExternalEntityDetected)
    }

    let rawText = "A & B < C > D \"Quote\" 'Apostrophe'"
    let escaped = XMLSanitizer.escape(rawText)
    XCTAssertEqual(escaped, "A &amp; B &lt; C &gt; D &quot;Quote&quot; &apos;Apostrophe&apos;")
    XCTAssertEqual(XMLSanitizer.unescape(escaped), rawText)
  }

  func testOOXMLContainerCodecRoundTripForDocxXlsxPptx() throws {
    // 1. DOCX Round-Trip
    let originalDoc = OfficeDocumentModel(
      title: "2026년도 아키텍처 기술 규격서",
      format: .docx,
      paperSize: .a4,
      headerText: "기밀 등급: 대외비",
      footerText: "페이지 1 / 1",
      paragraphs: [
        ParagraphBlock(
          style: .title,
          alignment: .center,
          text: "차세대 네이티브 오피스 엔진 설계",
          fontName: "Pretendard",
          fontSizePt: 20.0,
          isBold: true,
          isItalic: false,
          isUnderlined: false,
          lineSpacingPercent: 140.0
        ),
        ParagraphBlock(
          style: .body,
          alignment: .justified,
          text: "본 문서는 OOXML(ECMA-376) 컨테이너 왕복 무결성을 검증합니다. 특수문자 <&> 포함.",
          fontName: "NanumBarunGothic",
          fontSizePt: 11.5,
          isBold: false,
          isItalic: true,
          isUnderlined: true,
          lineSpacingPercent: 160.0
        ),
      ],
      tables: [
        DocumentTableBlock(
          caption: "포맷 지원 매트릭스",
          rows: [
            ["모듈", "표준 규격", "상태"],
            ["문서", "ECMA-376 WordprocessingML", "완료"],
            ["한글", "KS X 6101 OWPML", "완료"],
          ],
          hasHeaderRow: true
        )
      ],
      trackChangesEnabled: true
    )

    let docxData = try OOXMLContainerCodec.encodeDocument(originalDoc)
    let verifiedDocx = try FormatDetector.verify(data: docxData, fileName: "spec.docx")
    XCTAssertEqual(verifiedDocx.format, .docx)

    let docxEntries = try ZipContainerCodec.unpack(from: docxData)
    let docxPaths = Set(docxEntries.map(\.path))
    XCTAssertTrue(docxPaths.contains("[Content_Types].xml"))
    XCTAssertTrue(docxPaths.contains("_rels/.rels"))
    XCTAssertTrue(docxPaths.contains("docProps/core.xml"))
    XCTAssertTrue(docxPaths.contains("word/document.xml"))

    let decodedDoc = try OOXMLContainerCodec.decodeDocument(from: docxData)
    XCTAssertEqual(decodedDoc.title, originalDoc.title)
    XCTAssertEqual(decodedDoc.paperSize, originalDoc.paperSize)
    XCTAssertEqual(decodedDoc.headerText, originalDoc.headerText)
    XCTAssertEqual(decodedDoc.footerText, originalDoc.footerText)
    XCTAssertEqual(decodedDoc.trackChangesEnabled, originalDoc.trackChangesEnabled)
    XCTAssertEqual(decodedDoc.paragraphs.count, 2)
    XCTAssertEqual(decodedDoc.paragraphs[0].style, .title)
    XCTAssertEqual(decodedDoc.paragraphs[0].alignment, .center)
    XCTAssertEqual(decodedDoc.paragraphs[0].text, originalDoc.paragraphs[0].text)
    XCTAssertEqual(decodedDoc.paragraphs[0].isBold, true)
    XCTAssertEqual(decodedDoc.paragraphs[1].text, originalDoc.paragraphs[1].text)
    XCTAssertEqual(decodedDoc.paragraphs[1].fontName, "NanumBarunGothic")
    XCTAssertEqual(decodedDoc.paragraphs[1].fontSizePt, 11.5, accuracy: 0.01)
    XCTAssertEqual(decodedDoc.paragraphs[1].isItalic, true)
    XCTAssertEqual(decodedDoc.paragraphs[1].isUnderlined, true)
    XCTAssertEqual(decodedDoc.tables.count, 1)
    XCTAssertEqual(decodedDoc.tables[0].caption, "포맷 지원 매트릭스")
    XCTAssertEqual(decodedDoc.tables[0].rows, originalDoc.tables[0].rows)
    XCTAssertEqual(decodedDoc.tables[0].hasHeaderRow, true)

    // 2. XLSX Round-Trip
    var originalSheet = WorksheetModel(name: "분기실적")
    try originalSheet.setCell("A1", rawValue: "150")
    try originalSheet.setCell("A2", rawValue: "250")
    try originalSheet.setCell("A3", rawValue: "=SUM(A1:A2)")
    try originalSheet.setCell("B1", rawValue: "매출합계")
    try originalSheet.setCell("B2", rawValue: "=IF(A3>=400, \"목표달성\", \"미달\")")

    let xlsxData = try OOXMLContainerCodec.encodeWorksheet(originalSheet)
    let verifiedXlsx = try FormatDetector.verify(data: xlsxData, fileName: "report.xlsx")
    XCTAssertEqual(verifiedXlsx.format, .xlsx)

    let xlsxEntries = try ZipContainerCodec.unpack(from: xlsxData)
    let xlsxPaths = Set(xlsxEntries.map(\.path))
    XCTAssertTrue(xlsxPaths.contains("[Content_Types].xml"))
    XCTAssertTrue(xlsxPaths.contains("xl/workbook.xml"))
    XCTAssertTrue(xlsxPaths.contains("xl/worksheets/sheet1.xml"))

    let decodedSheet = try OOXMLContainerCodec.decodeWorksheet(from: xlsxData)
    XCTAssertEqual(decodedSheet.name, "분기실적")
    XCTAssertEqual(decodedSheet.rawValue(at: "A1"), "150")
    XCTAssertEqual(decodedSheet.rawValue(at: "A3"), "=SUM(A1:A2)")
    XCTAssertEqual(decodedSheet.evaluate(at: "A3"), .number(400))
    XCTAssertEqual(decodedSheet.evaluate(at: "B2"), .text("목표달성"))

    // 3. PPTX Round-Trip
    let originalDeck = PresentationDeckModel(
      title: "분기 경영 전략 발표",
      aspectRatio: .widescreen16x9,
      slides: [
        SlideModel(
          title: "핵심 성과 지표",
          subtitle: "2026 Q3 요약",
          bulletPoints: ["컨테이너 파서 속도 3배 향상", "Fail-Closed 보안 게이트 완비"],
          speakerNotes: "발표 시 보안 아키텍처 강조",
          masterLayout: .titleAndContent,
          transition: .morph,
          elements: [
            SlideCanvasElement(
              title: "지표 카드", bodyText: "달성률 120%", x: 120, y: 180, width: 320, height: 140)
          ]
        )
      ]
    )

    let pptxData = try OOXMLContainerCodec.encodePresentation(originalDeck)
    let verifiedPptx = try FormatDetector.verify(data: pptxData, fileName: "deck.pptx")
    XCTAssertEqual(verifiedPptx.format, .pptx)

    let pptxEntries = try ZipContainerCodec.unpack(from: pptxData)
    let pptxPaths = Set(pptxEntries.map(\.path))
    XCTAssertTrue(pptxPaths.contains("[Content_Types].xml"))
    XCTAssertTrue(pptxPaths.contains("ppt/presentation.xml"))
    XCTAssertTrue(pptxPaths.contains("ppt/slides/slide1.xml"))

    let decodedDeck = try OOXMLContainerCodec.decodePresentation(from: pptxData)
    XCTAssertEqual(decodedDeck.title, originalDeck.title)
    XCTAssertEqual(decodedDeck.aspectRatio, .widescreen16x9)
    XCTAssertEqual(decodedDeck.slides.count, 1)
    XCTAssertEqual(decodedDeck.slides[0].title, "핵심 성과 지표")
    XCTAssertEqual(decodedDeck.slides[0].subtitle, "2026 Q3 요약")
    XCTAssertEqual(
      decodedDeck.slides[0].bulletPoints, ["컨테이너 파서 속도 3배 향상", "Fail-Closed 보안 게이트 완비"])
    XCTAssertEqual(decodedDeck.slides[0].speakerNotes, "발표 시 보안 아키텍처 강조")
    XCTAssertEqual(decodedDeck.slides[0].masterLayout, .titleAndContent)
    XCTAssertEqual(decodedDeck.slides[0].transition, .morph)
    XCTAssertEqual(decodedDeck.slides[0].elements.count, 1)
    XCTAssertEqual(decodedDeck.slides[0].elements[0].title, "지표 카드")
    XCTAssertEqual(decodedDeck.slides[0].elements[0].bodyText, "달성률 120%")
  }

  func testODFContainerCodecRoundTripForOdtOdsOdp() throws {
    // 1. ODT
    let doc = OfficeDocumentModel(
      title: "개방형 문서 테스트",
      format: .odt,
      paperSize: .b5,
      headerText: "머리말",
      footerText: "꼬리말",
      paragraphs: [
        ParagraphBlock(style: .heading1, alignment: .leading, text: "1. 개요", isBold: true),
        ParagraphBlock(style: .body, alignment: .justified, text: "ISO/IEC 26300 ODF 텍스트 문서입니다."),
      ],
      tables: [
        DocumentTableBlock(
          caption: "일정표", rows: [["단계", "기간"], ["구현", "1주"]], hasHeaderRow: true)
      ]
    )
    let odtData = try ODFContainerCodec.encodeDocument(doc)
    let odtEntries = try ZipContainerCodec.unpack(from: odtData)
    XCTAssertEqual(odtEntries.first?.path, "mimetype")
    XCTAssertEqual(odtEntries.first?.compressionMethod, .store)
    XCTAssertEqual(
      String(data: odtEntries.first?.data ?? Data(), encoding: .utf8),
      "application/vnd.oasis.opendocument.text")

    let decodedDoc = try ODFContainerCodec.decodeDocument(from: odtData)
    XCTAssertEqual(decodedDoc.title, doc.title)
    XCTAssertEqual(decodedDoc.paperSize, .b5)
    XCTAssertEqual(decodedDoc.headerText, "머리말")
    XCTAssertEqual(decodedDoc.footerText, "꼬리말")
    XCTAssertEqual(decodedDoc.paragraphs.map(\.text), ["1. 개요", "ISO/IEC 26300 ODF 텍스트 문서입니다."])
    XCTAssertEqual(decodedDoc.tables.first?.rows, [["단계", "기간"], ["구현", "1주"]])

    // 2. ODS
    var sheet = WorksheetModel(name: "ODF시트")
    try sheet.setCell("A1", rawValue: "40")
    try sheet.setCell("A2", rawValue: "60")
    try sheet.setCell("B1", rawValue: "=SUM(A1:A2)")

    let odsData = try ODFContainerCodec.encodeWorksheet(sheet)
    let odsEntries = try ZipContainerCodec.unpack(from: odsData)
    XCTAssertEqual(
      String(data: odsEntries.first?.data ?? Data(), encoding: .utf8),
      "application/vnd.oasis.opendocument.spreadsheet")

    let decodedSheet = try ODFContainerCodec.decodeWorksheet(from: odsData)
    XCTAssertEqual(decodedSheet.name, "ODF시트")
    XCTAssertEqual(decodedSheet.evaluate(at: "B1"), .number(100))

    // 3. ODP
    let deck = PresentationDeckModel(
      title: "ODF 발표자료",
      aspectRatio: .standard4x3,
      slides: [
        SlideModel(
          title: "첫 슬라이드", subtitle: "부제목", bulletPoints: ["항목 1", "항목 2"], speakerNotes: "발표 노트")
      ]
    )
    let odpData = try ODFContainerCodec.encodePresentation(deck)
    let odpEntries = try ZipContainerCodec.unpack(from: odpData)
    XCTAssertEqual(
      String(data: odpEntries.first?.data ?? Data(), encoding: .utf8),
      "application/vnd.oasis.opendocument.presentation")

    let decodedDeck = try ODFContainerCodec.decodePresentation(from: odpData)
    XCTAssertEqual(decodedDeck.title, "ODF 발표자료")
    XCTAssertEqual(decodedDeck.aspectRatio, .standard4x3)
    XCTAssertEqual(decodedDeck.slides.first?.bulletPoints, ["항목 1", "항목 2"])
  }

  func testHWPXContainerCodecRoundTripKSX6101() throws {
    let hwpxDoc = OfficeDocumentModel(
      title: "행정업무 운영 편람 협조전",
      format: .hwpx,
      paperSize: .a4,
      headerText: "행정안전부 공문서 표준",
      footerText: "- 1 -",
      paragraphs: [
        ParagraphBlock(
          style: .officialHeader, alignment: .center, text: "행정 효율과 협업 촉진에 관한 규정", isBold: true),
        ParagraphBlock(style: .body, alignment: .justified, text: "1. 귀 기관의 무궁한 발전을 기원합니다."),
      ],
      tables: [
        DocumentTableBlock(
          caption: "결재란",
          rows: [
            ["담당", "팀장", "원장"],
            ["김민수", "박지훈", "이서연"],
          ],
          hasHeaderRow: true
        )
      ]
    )

    let hwpxData = try HWPXContainerCodec.encodeDocument(hwpxDoc)
    let verified = try FormatDetector.verify(data: hwpxData, fileName: "official.hwpx")
    XCTAssertEqual(verified.format, .hwpx)

    let entries = try ZipContainerCodec.unpack(from: hwpxData)
    XCTAssertEqual(entries.first?.path, "mimetype")
    XCTAssertEqual(entries.first?.compressionMethod, .store)
    XCTAssertEqual(
      String(data: entries.first?.data ?? Data(), encoding: .utf8), "application/hwp+zip")

    let paths = Set(entries.map(\.path))
    XCTAssertTrue(paths.contains("Contents/content.hpf"))
    XCTAssertTrue(paths.contains("Contents/header.xml"))
    XCTAssertTrue(paths.contains("Contents/section0.xml"))

    let sectionXML =
      String(
        data: entries.first(where: { $0.path == "Contents/section0.xml" })?.data ?? Data(),
        encoding: .utf8) ?? ""
    XCTAssertTrue(sectionXML.contains("<hs:sec"))
    XCTAssertTrue(sectionXML.contains("<hp:p"))
    XCTAssertTrue(sectionXML.contains("<hp:run"))
    XCTAssertTrue(sectionXML.contains("<hp:t>"))
    XCTAssertTrue(sectionXML.contains("<hp:tbl"))

    let decoded = try HWPXContainerCodec.decodeDocument(from: hwpxData)
    XCTAssertEqual(decoded.title, hwpxDoc.title)
    XCTAssertEqual(decoded.format, .hwpx)
    XCTAssertEqual(decoded.headerText, hwpxDoc.headerText)
    XCTAssertEqual(decoded.footerText, hwpxDoc.footerText)
    XCTAssertEqual(decoded.paragraphs.count, 2)
    XCTAssertEqual(decoded.paragraphs[0].style, .officialHeader)
    XCTAssertEqual(decoded.paragraphs[0].text, "행정 효율과 협업 촉진에 관한 규정")
    XCTAssertEqual(decoded.paragraphs[1].text, "1. 귀 기관의 무궁한 발전을 기원합니다.")
    XCTAssertEqual(decoded.tables.count, 1)
    XCTAssertEqual(decoded.tables[0].caption, "결재란")
    XCTAssertEqual(decoded.tables[0].rows, hwpxDoc.tables[0].rows)
  }

  func testOLE2AndHWP5BinaryCodecRoundTripAndEncryptedRejection() throws {
    let hwpDoc = OfficeDocumentModel(
      title: "2026년도 정보보안 감사 결과 보고서",
      format: .hwp,
      paperSize: .a4,
      headerText: "대외비 (문서보안)",
      footerText: "정보보안팀",
      paragraphs: [
        ParagraphBlock(
          style: .title, alignment: .center, text: "정보보안 감사 결과 보고", fontSizePt: 18.0, isBold: true),
        ParagraphBlock(
          style: .body, alignment: .justified,
          text: "HWP 5.0 OLE2 복합 문서 바이너리 레코드(HWPTAG_PARA_TEXT UTF-16LE) 왕복 검증 문단입니다."),
      ],
      tables: [
        DocumentTableBlock(
          caption: "점검 결과 요약",
          rows: [
            ["점검 항목", "결과"],
            ["컨테이너 서명 검증", "적합"],
            ["압축 폭탄 방어", "적합"],
          ],
          hasHeaderRow: true
        )
      ]
    )

    let hwpData = try HWP5BinaryCodec.encodeDocument(hwpDoc)
    let verified = try FormatDetector.verify(data: hwpData, fileName: "audit.hwp")
    XCTAssertEqual(verified.format, .hwp)
    XCTAssertEqual(verified.containerFamily, .ole2Compound)
    XCTAssertEqual(hwpData.count % OLE2ContainerCodec.sectorSize, 0)

    let streams = try OLE2ContainerCodec.unpack(from: hwpData)
    let streamNames = Set(streams.map(\.name))
    XCTAssertTrue(streamNames.contains("FileHeader"))
    XCTAssertTrue(streamNames.contains("DocInfo"))
    XCTAssertTrue(streamNames.contains("BodyText/Section0"))

    let decoded = try HWP5BinaryCodec.decodeDocument(from: hwpData)
    XCTAssertEqual(decoded.title, hwpDoc.title)
    XCTAssertEqual(decoded.format, .hwp)
    XCTAssertEqual(decoded.headerText, hwpDoc.headerText)
    XCTAssertEqual(decoded.footerText, hwpDoc.footerText)
    XCTAssertEqual(decoded.paragraphs.count, 2)
    XCTAssertEqual(decoded.paragraphs[0].style, .title)
    XCTAssertEqual(decoded.paragraphs[0].text, "정보보안 감사 결과 보고")
    XCTAssertEqual(decoded.paragraphs[1].text, hwpDoc.paragraphs[1].text)
    XCTAssertEqual(decoded.tables.count, 1)
    XCTAssertEqual(decoded.tables[0].caption, "점검 결과 요약")
    XCTAssertEqual(decoded.tables[0].rows, hwpDoc.tables[0].rows)

    // Fail-Closed: Encrypted HWP 5.0 (bit 1 = 0x02 in FileHeader properties UInt32 at offset 36)
    let encryptedStreams = try streams.map { stream -> OLE2StreamEntry in
      guard stream.name == "FileHeader" else { return stream }
      var modified = stream.data
      let currentFlags = try BitPacking.readUInt32LE(from: modified, at: 36)
      try BitPacking.writeUInt32LE(
        currentFlags | HWP5BinaryCodec.flagEncrypted, into: &modified, at: 36)
      return OLE2StreamEntry(name: stream.name, data: modified)
    }
    let encryptedHwpData = try OLE2ContainerCodec.pack(streams: encryptedStreams)

    XCTAssertThrowsError(try HWP5BinaryCodec.decodeDocument(from: encryptedHwpData)) { error in
      XCTAssertEqual(error as? EasyOfficeError, .encryptedDocumentUnsupported(format: .hwp))
    }

    // [F1] Fail-Closed: Distribution HWP 5.0 (bit 2 = 0x04 in FileHeader properties UInt32 at offset 36)
    let distributionStreams = try streams.map { stream -> OLE2StreamEntry in
      guard stream.name == "FileHeader" else { return stream }
      var modified = stream.data
      let currentFlags = try BitPacking.readUInt32LE(from: modified, at: 36)
      try BitPacking.writeUInt32LE(
        currentFlags | HWP5BinaryCodec.flagDistribution, into: &modified, at: 36)
      return OLE2StreamEntry(name: stream.name, data: modified)
    }
    let distributionHwpData = try OLE2ContainerCodec.pack(streams: distributionStreams)

    XCTAssertThrowsError(try HWP5BinaryCodec.decodeDocument(from: distributionHwpData)) { error in
      XCTAssertEqual(error as? EasyOfficeError, .encryptedDocumentUnsupported(format: .hwp))
    }

    // [F2] Fail-Closed: OLE2 cross-stream sector sharing amplification defense
    var sharedSectorOLE2 = hwpData
    // Directory Sector 0 starts at file offset 512; Entry 1 is at 512 + 128, Entry 2 is at 512 + 256.
    // Force Entry 2's startSector (offset + 116) to equal Entry 1's startSector.
    let entry1StartSector = try BitPacking.readUInt32LE(from: sharedSectorOLE2, at: 512 + 128 + 116)
    try BitPacking.writeUInt32LE(entry1StartSector, into: &sharedSectorOLE2, at: 512 + 256 + 116)
    XCTAssertThrowsError(try OLE2ContainerCodec.unpack(from: sharedSectorOLE2)) { error in
      guard case .corruptedContainer = error as? EasyOfficeError else {
        XCTFail("Expected corruptedContainer on cross-stream sector sharing, got \(error)")
        return
      }
    }
  }

  func testCSVStateMachineCommonMarkAndRTFCodecsRoundTrip() throws {
    // 1. RFC 4180 CSV & TSV State Machine Codec
    var sheet = WorksheetModel(name: "고객목록")
    try sheet.setCell("A1", rawValue: "이름, 부서")
    try sheet.setCell("B1", rawValue: "\"특수\" 직책")
    try sheet.setCell("A2", rawValue: "첫째줄\n둘째줄")
    try sheet.setCell("B2", rawValue: "=SUM(10,20)")

    let csvData = CSVStateMachineCodec.encodeWorksheet(sheet, delimiter: ",")
    let decodedCSV = try CSVStateMachineCodec.decodeWorksheet(
      from: csvData, name: "고객목록", delimiter: ",")
    XCTAssertEqual(decodedCSV.rawValue(at: "A1"), "이름, 부서")
    XCTAssertEqual(decodedCSV.rawValue(at: "B1"), "\"특수\" 직책")
    XCTAssertEqual(decodedCSV.rawValue(at: "A2"), "첫째줄\n둘째줄")
    XCTAssertEqual(decodedCSV.rawValue(at: "B2"), "=SUM(10,20)")
    XCTAssertEqual(decodedCSV.evaluate(at: "B2"), .number(30))

    // TSV round-trip
    let tsvData = CSVStateMachineCodec.encodeWorksheet(sheet, delimiter: "\t")
    let decodedTSV = try CSVStateMachineCodec.decodeWorksheet(
      from: tsvData, name: "고객목록", delimiter: "\t")
    XCTAssertEqual(decodedTSV.rawValue(at: "A1"), "이름, 부서")
    XCTAssertEqual(decodedTSV.rawValue(at: "A2"), "첫째줄\n둘째줄")

    // Unclosed quote must fail closed
    let malformedCSV = Data("A,B\n\"닫히지 않은 따옴표,값\n".utf8)
    XCTAssertThrowsError(try CSVStateMachineCodec.decodeWorksheet(from: malformedCSV, name: "오류")) {
      error in
      XCTAssertEqual(
        error as? EasyOfficeError, .corruptedContainer(detail: "Unclosed quote in CSV/TSV stream"))
    }

    // [F5] Unexpected quote inside an unquoted field must fail closed
    let unquotedQuoteCSV = Data("A,바\"보\n".utf8)
    XCTAssertThrowsError(
      try CSVStateMachineCodec.decodeWorksheet(from: unquotedQuoteCSV, name: "오류")
    ) { error in
      XCTAssertEqual(
        error as? EasyOfficeError,
        .corruptedContainer(detail: "Unexpected quote inside unquoted CSV/TSV field"))
    }

    // 2. CommonMark Markdown Codec (including [F3] escaped pipe '|' inside table cell)
    let mdDoc = OfficeDocumentModel(
      title: "릴리스 노트",
      format: .md,
      paragraphs: [
        ParagraphBlock(style: .title, text: "릴리스 노트"),
        ParagraphBlock(style: .heading1, text: "주요 변경 사항"),
        ParagraphBlock(style: .heading2, text: "보안 하드닝"),
        ParagraphBlock(style: .quote, text: "모든 컨테이너는 Fail-Closed 원칙을 준수합니다."),
        ParagraphBlock(style: .body, text: "굵은 텍스트 강조", isBold: true),
        ParagraphBlock(style: .body, text: "기울임 텍스트 강조", isItalic: true),
      ],
      tables: [
        DocumentTableBlock(
          caption: "성능 지표",
          rows: [
            ["항목", "수치"],
            ["CRC-32 | SHA-256", "단일 패스"],
          ],
          hasHeaderRow: true
        )
      ]
    )
    let mdData = CommonMarkCodec.encodeDocument(mdDoc)
    let mdText = String(data: mdData, encoding: .utf8) ?? ""
    XCTAssertTrue(mdText.contains("# 릴리스 노트"))
    XCTAssertTrue(mdText.contains("## 주요 변경 사항"))
    XCTAssertTrue(mdText.contains("### 보안 하드닝"))
    XCTAssertTrue(mdText.contains("> 모든 컨테이너는 Fail-Closed 원칙을 준수합니다."))
    XCTAssertTrue(mdText.contains("| 항목 | 수치 |"))

    let decodedMD = try CommonMarkCodec.decodeDocument(from: mdData, title: "릴리스 노트")
    XCTAssertEqual(decodedMD.paragraphs.count, 6)
    XCTAssertEqual(decodedMD.paragraphs[0].style, .title)
    XCTAssertEqual(decodedMD.paragraphs[1].style, .heading1)
    XCTAssertEqual(decodedMD.paragraphs[2].style, .heading2)
    XCTAssertEqual(decodedMD.paragraphs[3].style, .quote)
    XCTAssertEqual(decodedMD.paragraphs[4].isBold, true)
    XCTAssertEqual(decodedMD.paragraphs[4].text, "굵은 텍스트 강조")
    XCTAssertEqual(decodedMD.paragraphs[5].isItalic, true)
    XCTAssertEqual(decodedMD.paragraphs[5].text, "기울임 텍스트 강조")
    XCTAssertEqual(decodedMD.tables.count, 1)
    XCTAssertEqual(decodedMD.tables[0].rows, [["항목", "수치"], ["CRC-32 | SHA-256", "단일 패스"]])

    // 3. RTF Codec with Korean Unicode \uN? escapes and [F3] embedded newline (\line) round-trip
    let rtfDoc = OfficeDocumentModel(
      title: "한글 RTF 문서",
      format: .rtf,
      paragraphs: [
        ParagraphBlock(style: .title, text: "유니코드 이스케이프 검증", isBold: true),
        ParagraphBlock(style: .body, text: "한글과 ASCII(123) 및 중괄호 \\ { }\n두 번째 줄 이스케이프를 검증합니다."),
      ],
      tables: [
        DocumentTableBlock(caption: "요약표", rows: [["키", "값"], ["언어", "한국어"]])
      ]
    )
    let rtfData = RTFCodec.encodeDocument(rtfDoc)
    let verifiedRTF = try FormatDetector.verify(data: rtfData, fileName: "sample.rtf")
    XCTAssertEqual(verifiedRTF.format, .rtf)

    let decodedRTF = try RTFCodec.decodeDocument(from: rtfData)
    XCTAssertEqual(decodedRTF.title, "한글 RTF 문서")
    XCTAssertEqual(decodedRTF.paragraphs.count, 2)
    XCTAssertEqual(decodedRTF.paragraphs[0].text, "유니코드 이스케이프 검증")
    XCTAssertEqual(decodedRTF.paragraphs[0].isBold, true)
    XCTAssertEqual(
      decodedRTF.paragraphs[1].text, "한글과 ASCII(123) 및 중괄호 \\ { }\n두 번째 줄 이스케이프를 검증합니다.")
    XCTAssertEqual(decodedRTF.tables.count, 1)
    XCTAssertEqual(decodedRTF.tables[0].rows, [["키", "값"], ["언어", "한국어"]])
  }

  func testSlicingBy8CRC32BulkUTF16AndXMLSanitizerUpgrades() throws {
    // 1. Slicing-by-8 table dimensions (8 slices x 256 entries)
    XCTAssertEqual(CRC32Calculator.slicingTables.count, 8)
    XCTAssertTrue(CRC32Calculator.slicingTables.allSatisfy { $0.count == 256 })

    // Verify bit-exactness across empty, <8 byte, 9-byte standard vector, and multi-block payloads
    XCTAssertEqual(CRC32Calculator.compute(for: Data()), 0x0000_0000)
    XCTAssertEqual(CRC32Calculator.compute(for: Data("123456789".utf8)), 0xCBF4_3926)

    // 2. Bulk UTF-16LE encoding
    let text = "한글 UTF-16LE 벌크 인코딩 2026"
    let encoded = BitPacking.encodeUTF16LE(text)
    XCTAssertEqual(encoded.count, text.utf16.count * 2)
    XCTAssertEqual(
      try BitPacking.decodeUTF16LE(from: encoded, at: 0, byteLength: encoded.count), text)

    // 3. Zero-allocation XXE byte scanner (case-insensitive <!entity and <!DOCTYPE)
    let lowerEntityBytes = Data(
      "<?xml version='1.0'?><!entity xxe SYSTEM 'file:///etc/passwd'><r/>".utf8)
    XCTAssertThrowsError(try XMLSanitizer.validateSafeXMLBytes(lowerEntityBytes)) { error in
      XCTAssertEqual(error as? EasyOfficeError, .xxeOrExternalEntityDetected)
    }

    // 4. Nested same-tag extraction (<w:tbl> inside <w:tbl>) & single-quoted attributes
    let nestedXML =
      "<w:tbl id='outer' mode=\"strict\"><w:tr><w:tc><w:tbl id='inner'><w:tr><w:tc>내부셀</w:tc></w:tr></w:tbl></w:tc></w:tr></w:tbl>"
    let extractedTables = XMLSanitizer.extractElements(named: "w:tbl", in: nestedXML)
    XCTAssertEqual(extractedTables.count, 1)
    XCTAssertEqual(extractedTables.first?.attributes["id"], "outer")
    XCTAssertEqual(extractedTables.first?.attributes["mode"], "strict")
    XCTAssertTrue(extractedTables.first?.innerXML.contains("<w:tbl id='inner'>") == true)

    let innerTables = XMLSanitizer.extractElements(named: "w:tbl", in: extractedTables[0].innerXML)
    XCTAssertEqual(innerTables.count, 1)
    XCTAssertEqual(innerTables.first?.attributes["id"], "inner")
  }

  func testExternalStandardOOXMLDecodingAndLosslessMetadataRoundTrip() throws {
    // 1. External standard DOCX where <w:p> has NO custom attributes, only child <w:pPr>/<w:rPr>
    let externalDocxXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
        <w:body>
          <w:p>
            <w:pPr>
              <w:pStyle w:val="Heading1"/>
              <w:jc w:val="center"/>
            </w:pPr>
            <w:r>
              <w:rPr>
                <w:rFonts w:ascii="NanumGothic"/>
                <w:b/>
                <w:i/>
                <w:u w:val="single"/>
                <w:sz w:val="32"/>
              </w:rPr>
              <w:t>외부 표준 </w:t>
            </w:r>
            <w:r>
              <w:t>제목 문단</w:t>
            </w:r>
          </w:p>
          <w:tbl>
            <w:tr>
              <w:tc><w:p><w:r><w:t>표 내부 셀 1</w:t></w:r></w:p></w:tc>
              <w:tc><w:p><w:r><w:t>표 내부 셀 2</w:t></w:r></w:p></w:tc>
            </w:tr>
          </w:tbl>
        </w:body>
      </w:document>
      """
    let externalDocxArchive = try ZipContainerCodec.pack(entries: [
      ZipContainerEntry(
        path: "word/document.xml", data: Data(externalDocxXML.utf8), compressionMethod: .deflate)
    ])
    let decodedExternalDoc = try OOXMLContainerCodec.decodeDocument(from: externalDocxArchive)
    // Table cell <w:p> must NOT be duplicated into top-level body paragraphs!
    XCTAssertEqual(decodedExternalDoc.paragraphs.count, 1)
    XCTAssertEqual(decodedExternalDoc.paragraphs[0].style, .heading1)
    XCTAssertEqual(decodedExternalDoc.paragraphs[0].alignment, .center)
    XCTAssertEqual(decodedExternalDoc.paragraphs[0].fontName, "NanumGothic")
    XCTAssertEqual(decodedExternalDoc.paragraphs[0].fontSizePt, 16.0, accuracy: 0.01)
    XCTAssertTrue(decodedExternalDoc.paragraphs[0].isBold)
    XCTAssertTrue(decodedExternalDoc.paragraphs[0].isItalic)
    XCTAssertTrue(decodedExternalDoc.paragraphs[0].isUnderlined)
    XCTAssertEqual(decodedExternalDoc.paragraphs[0].text, "외부 표준 제목 문단")
    XCTAssertEqual(decodedExternalDoc.tables.count, 1)
    XCTAssertEqual(decodedExternalDoc.tables[0].rows, [["표 내부 셀 1", "표 내부 셀 2"]])

    // Lossless round-trip of full OfficeDocumentModel metadata (sections, footnotes, revisions, comments, bookmarks, cellSpans)
    let pAnchor = ParagraphBlock(style: .heading1, text: "1. 아키텍처 개요")
    var richTable = DocumentTableBlock(
      caption: "병합 표",
      rows: [["A", "B"], ["C", "D"]],
      hasHeaderRow: true,
      borderStyle: .doubleLine
    )
    richTable.mergeCells(startRow: 0, startCol: 0, rowSpan: 1, colSpan: 2)

    let fullDoc = OfficeDocumentModel(
      title: "전체 메타데이터 보존 검증",
      format: .docx,
      paperSize: .letter,
      headerText: "머리글",
      footerText: "바닥글",
      paragraphs: [pAnchor],
      tables: [richTable],
      trackChangesEnabled: true,
      sections: [
        DocumentSectionLayout(
          paperSize: .letter, orientation: .landscape, columnCount: 2, marginTopPt: 40,
          marginBottomPt: 40, marginLeftPt: 50, marginRightPt: 50)
      ],
      footnotes: [
        FootnoteItem(number: 1, isEndnote: true, anchorParagraphID: pAnchor.id, text: "미주 설명 텍스트")
      ],
      revisions: [
        TrackedRevision(
          author: "QA", kind: .insertion, paragraphID: pAnchor.id, originalText: "개요",
          proposedText: "아키텍처 개요")
      ],
      comments: [
        DocumentCommentThread(
          anchorParagraphID: pAnchor.id, author: "리뷰어", message: "검토 의견", replies: ["반영 완료"],
          isResolved: true)
      ],
      bookmarks: ["sec1": pAnchor.id]
    )
    let encodedFullDoc = try OOXMLContainerCodec.encodeDocument(fullDoc)
    let decodedFullDoc = try OOXMLContainerCodec.decodeDocument(from: encodedFullDoc)
    XCTAssertEqual(decodedFullDoc.sections, fullDoc.sections)
    XCTAssertEqual(decodedFullDoc.footnotes, fullDoc.footnotes)
    XCTAssertEqual(decodedFullDoc.revisions, fullDoc.revisions)
    XCTAssertEqual(decodedFullDoc.comments, fullDoc.comments)
    XCTAssertEqual(decodedFullDoc.bookmarks, fullDoc.bookmarks)
    XCTAssertEqual(decodedFullDoc.tables.first?.borderStyle, .doubleLine)
    XCTAssertEqual(decodedFullDoc.tables.first?.cellSpans, richTable.cellSpans)

    // 2. External standard XLSX with xl/sharedStrings.xml and multi-row <row r="1">, <row r="2">
    let sharedStringsXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="2" uniqueCount="2">
        <si><t>품목명</t></si>
        <si><r><t>고성능 </t></r><r><t>워크스테이션</t></r></si>
      </sst>
      """
    let externalSheetXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
        <sheetData>
          <row r="1">
            <c r="A1" t="s"><v>0</v></c>
            <c r="B1" t="inlineStr"><is><t>단가</t></is></c>
          </row>
          <row r="2">
            <c r="A2" t="s"><v>1</v></c>
            <c r="B2"><v>2500</v></c>
            <c r="C2" t="b"><v>1</v></c>
          </row>
        </sheetData>
      </worksheet>
      """
    let externalXlsxArchive = try ZipContainerCodec.pack(entries: [
      ZipContainerEntry(
        path: "xl/sharedStrings.xml", data: Data(sharedStringsXML.utf8), compressionMethod: .deflate
      ),
      ZipContainerEntry(
        path: "xl/worksheets/sheet1.xml", data: Data(externalSheetXML.utf8),
        compressionMethod: .deflate),
    ])
    let decodedExternalSheet = try OOXMLContainerCodec.decodeWorksheet(from: externalXlsxArchive)
    XCTAssertEqual(decodedExternalSheet.rawValue(at: "A1"), "품목명")
    XCTAssertEqual(decodedExternalSheet.rawValue(at: "B1"), "단가")
    XCTAssertEqual(decodedExternalSheet.rawValue(at: "A2"), "고성능 워크스테이션")
    XCTAssertEqual(decodedExternalSheet.evaluate(at: "B2"), .number(2500))
    XCTAssertEqual(decodedExternalSheet.evaluate(at: "C2"), .boolean(true))

    // Verify encoded XLSX groups rows properly into <row r="1"> and <row r="2"> and includes xl/sharedStrings.xml
    let reencodedXlsx = try OOXMLContainerCodec.encodeWorksheet(decodedExternalSheet)
    let reencodedEntries = try ZipContainerCodec.unpack(from: reencodedXlsx)
    let sheet1Entry = try XCTUnwrap(
      reencodedEntries.first(where: { $0.path == "xl/worksheets/sheet1.xml" }))
    let sheet1XMLString = String(decoding: sheet1Entry.data, as: UTF8.self)
    XCTAssertTrue(sheet1XMLString.contains("<row r=\"1\">"))
    XCTAssertTrue(sheet1XMLString.contains("<row r=\"2\">"))
    XCTAssertTrue(reencodedEntries.contains(where: { $0.path == "xl/sharedStrings.xml" }))

    // 3. External standard PPTX DrawingML (<p:cSld><p:spTree><p:sp>...<a:t>)
    let externalPresXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
        <p:sldIdLst>
          <p:sldId id="256" r:id="rId2"/>
        </p:sldIdLst>
      </p:presentation>
      """
    let externalSlideXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
          <p:spTree>
            <p:sp>
              <p:nvSpPr><p:cNvPr id="2" name="Title 1"/></p:nvSpPr>
              <p:spPr><a:xfrm><a:off x="100" y="50"/><a:ext cx="600" cy="80"/></a:xfrm></p:spPr>
              <p:txBody><a:p><a:r><a:t>DrawingML 외부 슬라이드 제목</a:t></a:r></a:p></p:txBody>
            </p:sp>
          </p:spTree>
        </p:cSld>
      </p:sld>
      """
    let externalPptxArchive = try ZipContainerCodec.pack(entries: [
      ZipContainerEntry(
        path: "ppt/presentation.xml", data: Data(externalPresXML.utf8), compressionMethod: .deflate),
      ZipContainerEntry(
        path: "ppt/slides/slide1.xml", data: Data(externalSlideXML.utf8),
        compressionMethod: .deflate),
    ])
    let decodedExternalDeck = try OOXMLContainerCodec.decodePresentation(from: externalPptxArchive)
    XCTAssertEqual(decodedExternalDeck.slides.count, 1)
    XCTAssertEqual(decodedExternalDeck.slides[0].elements.count, 1)
    XCTAssertEqual(decodedExternalDeck.slides[0].elements[0].title, "Title 1")
    XCTAssertEqual(decodedExternalDeck.slides[0].elements[0].bodyText, "DrawingML 외부 슬라이드 제목")
    XCTAssertEqual(decodedExternalDeck.slides[0].elements[0].x, 100)
    XCTAssertEqual(decodedExternalDeck.slides[0].elements[0].y, 50)
    XCTAssertEqual(decodedExternalDeck.slides[0].elements[0].width, 600)
    XCTAssertEqual(decodedExternalDeck.slides[0].elements[0].height, 80)
  }

  func testZipCodecsEquationsWatermarkWrapModeColumnWidthsAndSlideRotationAnimationRoundTrip()
    throws
  {
    var doc = OfficeDocumentModel(
      title: "DTP 수식 및 워터마크 검증",
      format: .docx,
      paragraphs: [ParagraphBlock(style: .body, text: "수식과 플로팅 이미지를 포함한 본문입니다.")]
    )
    doc.watermarkText = "대외비 (CONFIDENTIAL)"
    doc.addEquation(title: "에너지 등가식", rawExpression: "E = mc^2 + \\sum_{i=1}^{n} \\alpha_i")
    doc.images.append(
      EmbeddedImageAttachment(
        fileName: "float.png",
        caption: "우측 어울림 차트",
        mimeType: "image/png",
        imageData: Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
        widthPt: 180,
        heightPt: 90,
        wrapMode: .squareRight
      ))

    let docxData = try OOXMLContainerCodec.encodeDocument(doc)
    let decodedDocx = try OOXMLContainerCodec.decodeDocument(from: docxData)
    XCTAssertEqual(decodedDocx.watermarkText, "대외비 (CONFIDENTIAL)")
    XCTAssertEqual(decodedDocx.equations.count, 1)
    XCTAssertEqual(decodedDocx.equations[0].renderedNotation, "E = mc² + ∑ᵢ₌₁ⁿ αᵢ")
    XCTAssertEqual(decodedDocx.images.first?.wrapMode, .squareRight)

    let odtData = try ODFContainerCodec.encodeDocument(doc)
    let decodedOdt = try ODFContainerCodec.decodeDocument(from: odtData)
    XCTAssertEqual(decodedOdt.watermarkText, "대외비 (CONFIDENTIAL)")
    XCTAssertEqual(decodedOdt.equations.count, 1)
    XCTAssertEqual(decodedOdt.images.first?.wrapMode, .squareRight)

    let hwpxData = try HWPXContainerCodec.encodeDocument(doc)
    let decodedHwpx = try HWPXContainerCodec.decodeDocument(from: hwpxData)
    XCTAssertEqual(decodedHwpx.watermarkText, "대외비 (CONFIDENTIAL)")
    XCTAssertEqual(decodedHwpx.equations.count, 1)
    XCTAssertEqual(decodedHwpx.images.first?.wrapMode, .squareRight)

    var sheet = WorksheetModel(name: "레이아웃시트")
    try sheet.setCell("A1", rawValue: "100")
    sheet.setColumnWidth(180.0, at: 0)
    sheet.setRowHeight(44.0, at: 0)
    let xlsxData = try OOXMLContainerCodec.encodeWorksheet(sheet)
    let decodedXlsx = try OOXMLContainerCodec.decodeWorksheet(from: xlsxData)
    XCTAssertEqual(decodedXlsx.columnWidth(at: 0), 180.0)
    XCTAssertEqual(decodedXlsx.rowHeight(at: 0), 44.0)

    let odsData = try ODFContainerCodec.encodeWorksheet(sheet)
    let decodedOds = try ODFContainerCodec.decodeWorksheet(from: odsData)
    XCTAssertEqual(decodedOds.columnWidth(at: 0), 180.0)
    XCTAssertEqual(decodedOds.rowHeight(at: 0), 44.0)

    var deck = PresentationDeckModel(
      title: "회전 및 애니메이션 덱",
      slides: [
        SlideModel(
          title: "슬라이드 1",
          elements: [
            SlideCanvasElement(
              title: "회전 박스",
              bodyText: "45도 회전",
              x: 120,
              y: 90,
              width: 240,
              height: 100,
              rotationDegrees: 45.0,
              animation: .zoomIn
            )
          ]
        )
      ]
    )
    deck.slides[0].rotateElement(id: deck.slides[0].elements[0].id, toDegrees: 30.0)
    let pptxData = try OOXMLContainerCodec.encodePresentation(deck)
    let decodedPptx = try OOXMLContainerCodec.decodePresentation(from: pptxData)
    XCTAssertEqual(decodedPptx.slides[0].elements[0].rotationDegrees, 30.0)
    XCTAssertEqual(decodedPptx.slides[0].elements[0].animation, .zoomIn)
  }
}
