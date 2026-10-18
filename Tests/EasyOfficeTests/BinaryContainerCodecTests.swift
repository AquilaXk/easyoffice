import XCTest
@testable import EasyOfficeCore

final class BinaryContainerCodecTests: XCTestCase {

    func testBitPackingLittleEndianReadWriteAndCRC32() throws {
        var buffer = Data()
        BitPacking.appendUInt16LE(0x1234, to: &buffer)
        BitPacking.appendUInt32LE(0x89ABCDEF, to: &buffer)
        BitPacking.appendUInt64LE(0x0102030405060708, to: &buffer)

        XCTAssertEqual(buffer.count, 14)
        XCTAssertEqual([UInt8](buffer.prefix(2)), [0x34, 0x12])
        XCTAssertEqual([UInt8](buffer[2..<6]), [0xEF, 0xCD, 0xAB, 0x89])

        XCTAssertEqual(try BitPacking.readUInt16LE(from: buffer, at: 0), 0x1234)
        XCTAssertEqual(try BitPacking.readUInt32LE(from: buffer, at: 2), 0x89ABCDEF)
        XCTAssertEqual(try BitPacking.readUInt64LE(from: buffer, at: 6), 0x0102030405060708)

        var mutableBuffer = Data(repeating: 0, count: 8)
        try BitPacking.writeUInt32LE(0xCAFEBABE, into: &mutableBuffer, at: 2)
        XCTAssertEqual(try BitPacking.readUInt32LE(from: mutableBuffer, at: 2), 0xCAFEBABE)

        // Out-of-bounds read must fail closed
        XCTAssertThrowsError(try BitPacking.readUInt32LE(from: buffer, at: 12)) { error in
            guard case .corruptedContainer = error as? EasyOfficeError else {
                XCTFail("Expected corruptedContainer error, got \(error)")
                return
            }
        }

        // IEEE 802.3 CRC-32 standard test vector: ASCII "123456789" -> 0xCBF43926
        let vector = Data("123456789".utf8)
        XCTAssertEqual(CRC32Calculator.compute(for: vector), 0xCBF43926)
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
        let xmlString = String(repeating: "<hp:p><hp:run><hp:t>대한민국 공공문서 표준 HWPX 테스트 문단입니다.</hp:t></hp:run></hp:p>\n", count: 20)
        let xmlBytes = Data(xmlString.utf8)

        let entries: [ZipContainerEntry] = [
            ZipContainerEntry(path: "mimetype", data: mimeBytes, compressionMethod: .store),
            ZipContainerEntry(path: "Contents/section0.xml", data: xmlBytes, compressionMethod: .deflate)
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
        for maliciousPath in ["../evil.xml", "a/../../etc/passwd", "/etc/passwd", "folder\\evil.xml", "", "bad\0name.xml"] {
            XCTAssertThrowsError(
                try ZipContainerCodec.pack(entries: [ZipContainerEntry(path: maliciousPath, data: mimeBytes, compressionMethod: .store)])
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
        XCTAssertThrowsError(try ZipContainerCodec.unpack(from: compressedArchive, policy: strictPolicy)) { error in
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
                )
            ],
            tables: [
                DocumentTableBlock(
                    caption: "포맷 지원 매트릭스",
                    rows: [
                        ["모듈", "표준 규격", "상태"],
                        ["문서", "ECMA-376 WordprocessingML", "완료"],
                        ["한글", "KS X 6101 OWPML", "완료"]
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
                        SlideCanvasElement(title: "지표 카드", bodyText: "달성률 120%", x: 120, y: 180, width: 320, height: 140)
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
        XCTAssertEqual(decodedDeck.slides[0].bulletPoints, ["컨테이너 파서 속도 3배 향상", "Fail-Closed 보안 게이트 완비"])
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
                ParagraphBlock(style: .body, alignment: .justified, text: "ISO/IEC 26300 ODF 텍스트 문서입니다.")
            ],
            tables: [
                DocumentTableBlock(caption: "일정표", rows: [["단계", "기간"], ["구현", "1주"]], hasHeaderRow: true)
            ]
        )
        let odtData = try ODFContainerCodec.encodeDocument(doc)
        let odtEntries = try ZipContainerCodec.unpack(from: odtData)
        XCTAssertEqual(odtEntries.first?.path, "mimetype")
        XCTAssertEqual(odtEntries.first?.compressionMethod, .store)
        XCTAssertEqual(String(data: odtEntries.first?.data ?? Data(), encoding: .utf8), "application/vnd.oasis.opendocument.text")

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
        XCTAssertEqual(String(data: odsEntries.first?.data ?? Data(), encoding: .utf8), "application/vnd.oasis.opendocument.spreadsheet")

        let decodedSheet = try ODFContainerCodec.decodeWorksheet(from: odsData)
        XCTAssertEqual(decodedSheet.name, "ODF시트")
        XCTAssertEqual(decodedSheet.evaluate(at: "B1"), .number(100))

        // 3. ODP
        let deck = PresentationDeckModel(
            title: "ODF 발표자료",
            aspectRatio: .standard4x3,
            slides: [
                SlideModel(title: "첫 슬라이드", subtitle: "부제목", bulletPoints: ["항목 1", "항목 2"], speakerNotes: "발표 노트")
            ]
        )
        let odpData = try ODFContainerCodec.encodePresentation(deck)
        let odpEntries = try ZipContainerCodec.unpack(from: odpData)
        XCTAssertEqual(String(data: odpEntries.first?.data ?? Data(), encoding: .utf8), "application/vnd.oasis.opendocument.presentation")

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
                ParagraphBlock(style: .officialHeader, alignment: .center, text: "행정 효율과 협업 촉진에 관한 규정", isBold: true),
                ParagraphBlock(style: .body, alignment: .justified, text: "1. 귀 기관의 무궁한 발전을 기원합니다.")
            ],
            tables: [
                DocumentTableBlock(
                    caption: "결재란",
                    rows: [
                        ["담당", "팀장", "원장"],
                        ["김민수", "박지훈", "이서연"]
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
        XCTAssertEqual(String(data: entries.first?.data ?? Data(), encoding: .utf8), "application/hwp+zip")

        let paths = Set(entries.map(\.path))
        XCTAssertTrue(paths.contains("Contents/content.hpf"))
        XCTAssertTrue(paths.contains("Contents/header.xml"))
        XCTAssertTrue(paths.contains("Contents/section0.xml"))

        let sectionXML = String(data: entries.first(where: { $0.path == "Contents/section0.xml" })?.data ?? Data(), encoding: .utf8) ?? ""
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
                ParagraphBlock(style: .title, alignment: .center, text: "정보보안 감사 결과 보고", fontSizePt: 18.0, isBold: true),
                ParagraphBlock(style: .body, alignment: .justified, text: "HWP 5.0 OLE2 복합 문서 바이너리 레코드(HWPTAG_PARA_TEXT UTF-16LE) 왕복 검증 문단입니다.")
            ],
            tables: [
                DocumentTableBlock(
                    caption: "점검 결과 요약",
                    rows: [
                        ["점검 항목", "결과"],
                        ["컨테이너 서명 검증", "적합"],
                        ["압축 폭탄 방어", "적합"]
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
            try BitPacking.writeUInt32LE(currentFlags | HWP5BinaryCodec.flagEncrypted, into: &modified, at: 36)
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
            try BitPacking.writeUInt32LE(currentFlags | HWP5BinaryCodec.flagDistribution, into: &modified, at: 36)
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
        let decodedCSV = try CSVStateMachineCodec.decodeWorksheet(from: csvData, name: "고객목록", delimiter: ",")
        XCTAssertEqual(decodedCSV.rawValue(at: "A1"), "이름, 부서")
        XCTAssertEqual(decodedCSV.rawValue(at: "B1"), "\"특수\" 직책")
        XCTAssertEqual(decodedCSV.rawValue(at: "A2"), "첫째줄\n둘째줄")
        XCTAssertEqual(decodedCSV.rawValue(at: "B2"), "=SUM(10,20)")
        XCTAssertEqual(decodedCSV.evaluate(at: "B2"), .number(30))

        // TSV round-trip
        let tsvData = CSVStateMachineCodec.encodeWorksheet(sheet, delimiter: "\t")
        let decodedTSV = try CSVStateMachineCodec.decodeWorksheet(from: tsvData, name: "고객목록", delimiter: "\t")
        XCTAssertEqual(decodedTSV.rawValue(at: "A1"), "이름, 부서")
        XCTAssertEqual(decodedTSV.rawValue(at: "A2"), "첫째줄\n둘째줄")

        // Unclosed quote must fail closed
        let malformedCSV = Data("A,B\n\"닫히지 않은 따옴표,값\n".utf8)
        XCTAssertThrowsError(try CSVStateMachineCodec.decodeWorksheet(from: malformedCSV, name: "오류")) { error in
            XCTAssertEqual(error as? EasyOfficeError, .corruptedContainer(detail: "Unclosed quote in CSV/TSV stream"))
        }

        // [F5] Unexpected quote inside an unquoted field must fail closed
        let unquotedQuoteCSV = Data("A,바\"보\n".utf8)
        XCTAssertThrowsError(try CSVStateMachineCodec.decodeWorksheet(from: unquotedQuoteCSV, name: "오류")) { error in
            XCTAssertEqual(error as? EasyOfficeError, .corruptedContainer(detail: "Unexpected quote inside unquoted CSV/TSV field"))
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
                ParagraphBlock(style: .body, text: "기울임 텍스트 강조", isItalic: true)
            ],
            tables: [
                DocumentTableBlock(
                    caption: "성능 지표",
                    rows: [
                        ["항목", "수치"],
                        ["CRC-32 | SHA-256", "단일 패스"]
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
                ParagraphBlock(style: .body, text: "한글과 ASCII(123) 및 중괄호 \\ { }\n두 번째 줄 이스케이프를 검증합니다.")
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
        XCTAssertEqual(decodedRTF.paragraphs[1].text, "한글과 ASCII(123) 및 중괄호 \\ { }\n두 번째 줄 이스케이프를 검증합니다.")
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
        XCTAssertEqual(try BitPacking.decodeUTF16LE(from: encoded, at: 0, byteLength: encoded.count), text)

        // 3. Zero-allocation XXE byte scanner (case-insensitive <!entity and <!DOCTYPE)
        let lowerEntityBytes = Data("<?xml version='1.0'?><!entity xxe SYSTEM 'file:///etc/passwd'><r/>".utf8)
        XCTAssertThrowsError(try XMLSanitizer.validateSafeXMLBytes(lowerEntityBytes)) { error in
            XCTAssertEqual(error as? EasyOfficeError, .xxeOrExternalEntityDetected)
        }

        // 4. Nested same-tag extraction (<w:tbl> inside <w:tbl>) & single-quoted attributes
        let nestedXML = "<w:tbl id='outer' mode=\"strict\"><w:tr><w:tc><w:tbl id='inner'><w:tr><w:tc>내부셀</w:tc></w:tr></w:tbl></w:tc></w:tr></w:tbl>"
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
            ZipContainerEntry(path: "word/document.xml", data: Data(externalDocxXML.utf8), compressionMethod: .deflate)
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
                DocumentSectionLayout(paperSize: .letter, orientation: .landscape, columnCount: 2, marginTopPt: 40, marginBottomPt: 40, marginLeftPt: 50, marginRightPt: 50)
            ],
            footnotes: [
                FootnoteItem(number: 1, isEndnote: true, anchorParagraphID: pAnchor.id, text: "미주 설명 텍스트")
            ],
            revisions: [
                TrackedRevision(author: "QA", kind: .insertion, paragraphID: pAnchor.id, originalText: "개요", proposedText: "아키텍처 개요")
            ],
            comments: [
                DocumentCommentThread(anchorParagraphID: pAnchor.id, author: "리뷰어", message: "검토 의견", replies: ["반영 완료"], isResolved: true)
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
            ZipContainerEntry(path: "xl/sharedStrings.xml", data: Data(sharedStringsXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "xl/worksheets/sheet1.xml", data: Data(externalSheetXML.utf8), compressionMethod: .deflate)
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
        let sheet1Entry = try XCTUnwrap(reencodedEntries.first(where: { $0.path == "xl/worksheets/sheet1.xml" }))
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
            ZipContainerEntry(path: "ppt/presentation.xml", data: Data(externalPresXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "ppt/slides/slide1.xml", data: Data(externalSlideXML.utf8), compressionMethod: .deflate)
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

    func testHWP5CompressedAndUncompressedAndLegacyOLE2CodecsRoundTrip() throws {
        // 1. HWP 5.0 Compressed (flagCompressed = 0x01) vs Uncompressed (0x00)
        let sampleDoc = OfficeDocumentModel(
            title: "HWP 5.0 압축 스트림 검증",
            format: .hwp,
            paragraphs: Array(
                repeating: ParagraphBlock(style: .body, text: "반복되는 한글 본문 스트림은 RFC 1951 Raw Deflate로 크게 압축됩니다."),
                count: 15
            ),
            tables: [
                DocumentTableBlock(caption: "압축표", rows: [["구분", "내용"], ["방식", "Deflate"]])
            ]
        )

        let compressedHwp = try HWP5BinaryCodec.encodeDocument(sampleDoc, compressed: true)
        let uncompressedHwp = try HWP5BinaryCodec.encodeDocument(sampleDoc, compressed: false)

        let compStreams = try OLE2ContainerCodec.unpack(from: compressedHwp)
        let compHeader = try XCTUnwrap(compStreams.first(where: { $0.name == "FileHeader" })?.data)
        let compFlags = try BitPacking.readUInt32LE(from: compHeader, at: 36)
        XCTAssertNotEqual(compFlags & HWP5BinaryCodec.flagCompressed, 0)

        let uncompStreams = try OLE2ContainerCodec.unpack(from: uncompressedHwp)
        let uncompHeader = try XCTUnwrap(uncompStreams.first(where: { $0.name == "FileHeader" })?.data)
        let uncompFlags = try BitPacking.readUInt32LE(from: uncompHeader, at: 36)
        XCTAssertEqual(uncompFlags & HWP5BinaryCodec.flagCompressed, 0)

        let compSectionSize = try XCTUnwrap(compStreams.first(where: { $0.name == "BodyText/Section0" })?.data.count)
        let uncompSectionSize = try XCTUnwrap(uncompStreams.first(where: { $0.name == "BodyText/Section0" })?.data.count)
        XCTAssertLessThan(compSectionSize, uncompSectionSize)

        let decodedComp = try HWP5BinaryCodec.decodeDocument(from: compressedHwp)
        let decodedUncomp = try HWP5BinaryCodec.decodeDocument(from: uncompressedHwp)
        XCTAssertEqual(decodedComp.title, sampleDoc.title)
        XCTAssertEqual(decodedComp.paragraphs.map(\.text), sampleDoc.paragraphs.map(\.text))
        XCTAssertEqual(decodedUncomp.paragraphs.map(\.text), sampleDoc.paragraphs.map(\.text))

        // 2. LegacyOLE2OfficeCodec: Word Binary (.doc) FibBase 0xA5EC round-trip
        let wordBinaryData = try LegacyOLE2OfficeCodec.encodeWordBinary(sampleDoc)
        let wordStreams = try OLE2ContainerCodec.unpack(from: wordBinaryData)
        let wordStream = try XCTUnwrap(wordStreams.first(where: { $0.name == "WordDocument" })?.data)
        XCTAssertEqual(try BitPacking.readUInt16LE(from: wordStream, at: 0), 0xA5EC)
        XCTAssertEqual(try BitPacking.readUInt16LE(from: wordStream, at: 2), 0x00C1)

        let decodedWordDoc = try LegacyOLE2OfficeCodec.decodeWordBinary(from: wordBinaryData)
        XCTAssertEqual(decodedWordDoc.title, sampleDoc.title)
        XCTAssertEqual(decodedWordDoc.format, .doc)
        XCTAssertEqual(decodedWordDoc.paragraphs.map(\.text), sampleDoc.paragraphs.map(\.text))
        XCTAssertEqual(decodedWordDoc.tables.first?.rows, sampleDoc.tables.first?.rows)

        // 3. LegacyOLE2OfficeCodec: BIFF8 Worksheet (.xls) BOF/DIMENSIONS/NUMBER/LABEL/EOF round-trip
        var sheet = WorksheetModel(name: "BIFF8시트")
        try sheet.setCell("A1", rawValue: "항목")
        try sheet.setCell("B1", rawValue: "수치")
        try sheet.setCell("A2", rawValue: "매출")
        try sheet.setCell("B2", rawValue: "1250.5")
        try sheet.setCell("B3", rawValue: "=B2*2")

        let biff8Data = try LegacyOLE2OfficeCodec.encodeBIFF8Worksheet(sheet)
        let xlsStreams = try OLE2ContainerCodec.unpack(from: biff8Data)
        let workbookBytes = try XCTUnwrap(xlsStreams.first(where: { $0.name == "Workbook" })?.data)
        // First record must be BOF (0x0809)
        XCTAssertEqual(try BitPacking.readUInt16LE(from: workbookBytes, at: 0), 0x0809)

        let decodedBiff8 = try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: biff8Data, name: "BIFF8시트")
        XCTAssertEqual(decodedBiff8.name, "BIFF8시트")
        XCTAssertEqual(decodedBiff8.rawValue(at: "A1"), "항목")
        XCTAssertEqual(decodedBiff8.evaluate(at: "B2"), .number(1250.5))
        XCTAssertEqual(decodedBiff8.evaluate(at: "B3"), .number(2501.0))

        // 4. LegacyOLE2OfficeCodec: PowerPoint Binary (.ppt) RT_Document / RT_Slide / RT_TextCharsAtom round-trip
        let pptDeck = PresentationDeckModel(
            title: "레거시 PPT 발표자료",
            aspectRatio: .standard4x3,
            slides: [
                SlideModel(
                    title: "1장 바이너리 레코드",
                    subtitle: "RT_Slide (1006)",
                    bulletPoints: ["RT_TextCharsAtom (4000)", "UTF-16LE 인코딩"],
                    speakerNotes: "발표자 노트 보존",
                    masterLayout: .sectionHeader,
                    transition: .push,
                    elements: [
                        SlideCanvasElement(title: "도형 1", bodyText: "본문 텍스트", x: 40, y: 60, width: 240, height: 90)
                    ]
                )
            ]
        )
        let pptBinaryData = try LegacyOLE2OfficeCodec.encodePowerPointBinary(pptDeck)
        let pptStreams = try OLE2ContainerCodec.unpack(from: pptBinaryData)
        let pptStream = try XCTUnwrap(pptStreams.first(where: { $0.name == "PowerPoint Document" })?.data)
        // First record type at offset 2 must be RT_Document (1000)
        XCTAssertEqual(try BitPacking.readUInt16LE(from: pptStream, at: 2), 1000)

        let decodedPpt = try LegacyOLE2OfficeCodec.decodePowerPointBinary(from: pptBinaryData)
        XCTAssertEqual(decodedPpt.title, "레거시 PPT 발표자료")
        XCTAssertEqual(decodedPpt.aspectRatio, .standard4x3)
        XCTAssertEqual(decodedPpt.slides.count, 1)
        XCTAssertEqual(decodedPpt.slides[0].title, "1장 바이너리 레코드")
        XCTAssertEqual(decodedPpt.slides[0].subtitle, "RT_Slide (1006)")
        XCTAssertEqual(decodedPpt.slides[0].bulletPoints, ["RT_TextCharsAtom (4000)", "UTF-16LE 인코딩"])
        XCTAssertEqual(decodedPpt.slides[0].speakerNotes, "발표자 노트 보존")
        XCTAssertEqual(decodedPpt.slides[0].masterLayout, .sectionHeader)
        XCTAssertEqual(decodedPpt.slides[0].transition, .push)
        XCTAssertEqual(decodedPpt.slides[0].elements.first?.title, "도형 1")
        XCTAssertEqual(decodedPpt.slides[0].elements.first?.bodyText, "본문 텍스트")

        // 5. Regression: RTF \u-2147483648? Int32.min wrapping negation, OOXML Infinity/NaN shared strings, BIFF8 length guard, small repetitive HWP5 stream
        let rtfIntMinPayload = Data("{\\rtf1\\ansi\\deff0\n{\\info{\\title 테스트}}\n\\pard\\s0 \\u-2147483648?안전파싱\\par\n}".utf8)
        let decodedRtfIntMin = try RTFCodec.decodeDocument(from: rtfIntMinPayload)
        XCTAssertTrue(decodedRtfIntMin.paragraphs.first?.text.hasSuffix("안전파싱") == true)

        var infNanSheet = WorksheetModel(name: "SpecialStrings")
        try infNanSheet.setCell("A1", rawValue: "Infinity")
        try infNanSheet.setCell("A2", rawValue: "NaN")
        let infNanXlsx = try OOXMLContainerCodec.encodeWorksheet(infNanSheet)
        let decodedInfNanSheet = try OOXMLContainerCodec.decodeWorksheet(from: infNanXlsx)
        XCTAssertEqual(decodedInfNanSheet.rawValue(at: "A1"), "Infinity")
        XCTAssertEqual(decodedInfNanSheet.rawValue(at: "A2"), "NaN")

        var oversizedBiffSheet = WorksheetModel(name: "Oversized")
        try oversizedBiffSheet.setCell("A1", rawValue: String(repeating: "가", count: 32_766))
        XCTAssertThrowsError(try LegacyOLE2OfficeCodec.encodeBIFF8Worksheet(oversizedBiffSheet))

        let repetitiveHwpDoc = OfficeDocumentModel(
            title: "반복문단",
            format: .hwp,
            paragraphs: [ParagraphBlock(text: String(repeating: "가", count: 1500))]
        )
        let repetitiveHwpBytes = try HWP5BinaryCodec.encodeDocument(repetitiveHwpDoc, compressed: true)
        let decodedRepetitiveHwp = try HWP5BinaryCodec.decodeDocument(from: repetitiveHwpBytes)
        XCTAssertEqual(decodedRepetitiveHwp.paragraphs.first?.text.count, 1500)
    }

    func testStandardExternalODSInteroperabilityAndMultiSheetXLSXODSRoundTrip() throws {
        // 1. Standard external .ods XML without custom address/raw attributes
        let externalODSContentXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
          <office:body>
            <office:spreadsheet>
              <table:table table:name="외부표준ODS">
                <table:table-row>
                  <table:table-cell office:value-type="float"><text:p>42</text:p></table:table-cell>
                  <table:table-cell table:formula="of:=A1*2"><text:p>84</text:p></table:table-cell>
                </table:table-row>
                <table:table-row>
                  <table:table-cell office:value-type="string" table:number-columns-repeated="2"><text:p>반복셀</text:p></table:table-cell>
                  <table:table-cell office:value-type="float"><text:p>100</text:p></table:table-cell>
                </table:table-row>
              </table:table>
            </office:spreadsheet>
          </office:body>
        </office:document-content>
        """
        let externalODSData = try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "mimetype", data: Data(ODFContainerCodec.spreadsheetMimeType.utf8), compressionMethod: .store),
            ZipContainerEntry(path: "content.xml", data: Data(externalODSContentXML.utf8), compressionMethod: .deflate)
        ])

        let decodedExternalODS = try ODFContainerCodec.decodeWorksheet(from: externalODSData)
        XCTAssertEqual(decodedExternalODS.name, "외부표준ODS")
        XCTAssertEqual(decodedExternalODS.rawValue(at: "A1"), "42")
        XCTAssertEqual(decodedExternalODS.rawValue(at: "B1"), "=A1*2")
        XCTAssertEqual(decodedExternalODS.evaluate(at: "B1"), .number(84))
        XCTAssertEqual(decodedExternalODS.rawValue(at: "A2"), "반복셀")
        XCTAssertEqual(decodedExternalODS.rawValue(at: "B2"), "반복셀")
        XCTAssertEqual(decodedExternalODS.evaluate(at: "C2"), .number(100))

        // 2. Multi-sheet + CellStyle + PageSetup round-trip across .xlsx and .ods
        var multiSheet = WorksheetModel(name: "1분기")
        try multiSheet.setCell("A1", rawValue: "12500")
        try multiSheet.setCell("B1", rawValue: "=A1*2")
        try multiSheet.setCellStyle(
            CellStyle(
                isBold: true,
                isItalic: true,
                isUnderlined: true,
                alignment: .trailing,
                numberFormat: .currencyKRW,
                fillHex: "#EEF0FF",
                textHex: "#5C6BC0"
            ),
            at: "A1"
        )
        multiSheet.pageSetup = WorksheetPageSetup(
            paperSize: .letter,
            orientation: .landscape,
            isMultiColumnPrint: true,
            marginPresetName: "좁은 여백"
        )
        multiSheet.sheetComments[PackedCellCoordinate(row: 0, column: 0)] = "1분기 확정 매출"
        multiSheet.trackChangesEnabled = true

        let b2Coord = PackedCellCoordinate(row: 1, column: 1)
        multiSheet.addSheetPage(
            named: "2분기",
            initialCells: [
                PackedCellCoordinate(row: 0, column: 0): "300",
                b2Coord: "=A1+50"
            ]
        )

        let xlsxBytes = try OOXMLContainerCodec.encodeWorksheet(multiSheet)
        let xlsxEntries = try ZipContainerCodec.unpack(from: xlsxBytes)
        let xlsxPaths = Set(xlsxEntries.map(\.path))
        XCTAssertTrue(xlsxPaths.contains("xl/worksheets/sheet1.xml"))
        XCTAssertTrue(xlsxPaths.contains("xl/worksheets/sheet2.xml"))

        let decodedXlsx = try OOXMLContainerCodec.decodeWorksheet(from: xlsxBytes)
        XCTAssertEqual(decodedXlsx.name, "1분기")
        XCTAssertEqual(decodedXlsx.allSheetNames, ["1분기", "2분기"])
        XCTAssertEqual(decodedXlsx.additionalSheets.count, 1)
        XCTAssertEqual(decodedXlsx.additionalSheets[0].name, "2분기")
        XCTAssertEqual(decodedXlsx.additionalSheets[0].cellsByCoordinate[b2Coord], "=A1+50")
        XCTAssertEqual(decodedXlsx.cellStyle(at: "A1").isBold, true)
        XCTAssertEqual(decodedXlsx.cellStyle(at: "A1").numberFormat, .currencyKRW)
        XCTAssertEqual(decodedXlsx.cellStyle(at: "A1").fillHex, "#EEF0FF")
        XCTAssertEqual(decodedXlsx.cellStyle(at: "A1").textHex, "#5C6BC0")
        XCTAssertEqual(decodedXlsx.pageSetup, multiSheet.pageSetup)
        XCTAssertEqual(decodedXlsx.sheetComments[PackedCellCoordinate(row: 0, column: 0)], "1분기 확정 매출")
        XCTAssertTrue(decodedXlsx.trackChangesEnabled)

        let odsBytes = try ODFContainerCodec.encodeWorksheet(multiSheet)
        let decodedOds = try ODFContainerCodec.decodeWorksheet(from: odsBytes)
        XCTAssertEqual(decodedOds.allSheetNames, ["1분기", "2분기"])
        XCTAssertEqual(decodedOds.additionalSheets.count, 1)
        XCTAssertEqual(decodedOds.additionalSheets[0].cellsByCoordinate[b2Coord], "=A1+50")
        XCTAssertEqual(decodedOds.cellStyle(at: "A1"), multiSheet.cellStyle(at: "A1"))
        XCTAssertEqual(decodedOds.pageSetup, multiSheet.pageSetup)
    }

    func testDocumentRunsAndEmbeddedImagesRoundTripAcrossDocxOdtHwpx() throws {
        let styledRuns = [
            DocumentTextRun(
                text: "시그니처 ",
                isBold: true,
                fontName: "Pretendard",
                fontSizePt: 14.0,
                foregroundHex: "#5C6BC0"
            ),
            DocumentTextRun(
                text: "라벤더 오피스",
                isItalic: true,
                isUnderlined: true,
                fontName: "NanumGothic",
                fontSizePt: 16.0,
                foregroundHex: "#3949AB"
            )
        ]
        let pWithRuns = ParagraphBlock(
            style: .body,
            alignment: .leading,
            text: "시그니처 라벤더 오피스",
            runs: styledRuns
        )
        let sampleImageBytes = Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x01, 0x02, 0x03, 0x04])
        let attachment = EmbeddedImageAttachment(
            fileName: "diagram.png",
            caption: "시스템 아키텍처 다이어그램",
            mimeType: "image/png",
            imageData: sampleImageBytes,
            widthPt: 420.0,
            heightPt: 260.0
        )

        let doc = OfficeDocumentModel(
            title: "리치 런 및 이미지 보존 문서",
            format: .docx,
            paragraphs: [pWithRuns],
            images: [attachment]
        )

        // 1. DOCX
        let docxData = try OOXMLContainerCodec.encodeDocument(doc)
        let docxEntries = try ZipContainerCodec.unpack(from: docxData)
        XCTAssertTrue(docxEntries.contains(where: { $0.path == "word/media/diagram.png" && $0.data == sampleImageBytes }))
        let decodedDocx = try OOXMLContainerCodec.decodeDocument(from: docxData)
        XCTAssertEqual(decodedDocx.paragraphs.first?.runs.count, 2)
        XCTAssertEqual(decodedDocx.paragraphs.first?.runs[0].text, "시그니처 ")
        XCTAssertEqual(decodedDocx.paragraphs.first?.runs[0].isBold, true)
        XCTAssertEqual(decodedDocx.paragraphs.first?.runs[0].foregroundHex, "#5C6BC0")
        XCTAssertEqual(decodedDocx.paragraphs.first?.runs[1].text, "라벤더 오피스")
        XCTAssertEqual(decodedDocx.paragraphs.first?.runs[1].isItalic, true)
        XCTAssertEqual(decodedDocx.paragraphs.first?.runs[1].isUnderlined, true)
        XCTAssertEqual(decodedDocx.paragraphs.first?.runs[1].foregroundHex, "#3949AB")
        XCTAssertEqual(decodedDocx.images.count, 1)
        XCTAssertEqual(decodedDocx.images.first?.fileName, "diagram.png")
        XCTAssertEqual(decodedDocx.images.first?.caption, "시스템 아키텍처 다이어그램")
        XCTAssertEqual(decodedDocx.images.first?.imageData, sampleImageBytes)
        XCTAssertEqual(decodedDocx.images.first?.widthPt ?? 0, 420.0, accuracy: 0.01)

        // 2. ODT
        let odtData = try ODFContainerCodec.encodeDocument(doc)
        let odtEntries = try ZipContainerCodec.unpack(from: odtData)
        XCTAssertTrue(odtEntries.contains(where: { $0.path == "Pictures/diagram.png" && $0.data == sampleImageBytes }))
        let decodedOdt = try ODFContainerCodec.decodeDocument(from: odtData)
        XCTAssertEqual(decodedOdt.paragraphs.first?.runs.count, 2)
        XCTAssertEqual(decodedOdt.paragraphs.first?.runs[0].foregroundHex, "#5C6BC0")
        XCTAssertEqual(decodedOdt.paragraphs.first?.runs[1].isUnderlined, true)
        XCTAssertEqual(decodedOdt.images.count, 1)
        XCTAssertEqual(decodedOdt.images.first?.imageData, sampleImageBytes)

        // 3. HWPX
        let hwpxData = try HWPXContainerCodec.encodeDocument(doc)
        let hwpxEntries = try ZipContainerCodec.unpack(from: hwpxData)
        XCTAssertTrue(hwpxEntries.contains(where: { $0.path == "BinData/diagram.png" && $0.data == sampleImageBytes }))
        let decodedHwpx = try HWPXContainerCodec.decodeDocument(from: hwpxData)
        XCTAssertEqual(decodedHwpx.paragraphs.first?.runs.count, 2)
        XCTAssertEqual(decodedHwpx.paragraphs.first?.runs[0].foregroundHex, "#5C6BC0")
        XCTAssertEqual(decodedHwpx.paragraphs.first?.runs[1].isItalic, true)
        XCTAssertEqual(decodedHwpx.images.count, 1)
        XCTAssertEqual(decodedHwpx.images.first?.imageData, sampleImageBytes)
    }

    func testPPTXNotesSlidesAndSlideElementFormattingAndBIFF8SSTAndRKDecoding() throws {
        // 1. PPTX notesSlides/notesSlide1.xml emission & SlideCanvasElement formatting round-trip
        let styledElement = SlideCanvasElement(
            title: "강조 도형",
            bodyText: "굵고 기울임 밑줄 텍스트",
            x: 80,
            y: 120,
            width: 300,
            height: 90,
            kind: .roundedRectangleShape,
            isBold: true,
            isItalic: true,
            isUnderlined: true
        )
        let deck = PresentationDeckModel(
            title: "발표 노트 검증 덱",
            slides: [
                SlideModel(
                    title: "슬라이드 1",
                    speakerNotes: "표준 notesSlide1.xml 파트에 기록되는 발표자 노트",
                    elements: [styledElement]
                )
            ]
        )

        let pptxData = try OOXMLContainerCodec.encodePresentation(deck)
        let pptxEntries = try ZipContainerCodec.unpack(from: pptxData)
        let notesEntry = try XCTUnwrap(pptxEntries.first(where: { $0.path == "ppt/notesSlides/notesSlide1.xml" }))
        let notesXML = String(decoding: notesEntry.data, as: UTF8.self)
        XCTAssertTrue(notesXML.contains("<p:notes"))
        XCTAssertTrue(notesXML.contains("표준 notesSlide1.xml 파트에 기록되는 발표자 노트"))

        let decodedDeck = try OOXMLContainerCodec.decodePresentation(from: pptxData)
        XCTAssertEqual(decodedDeck.slides.first?.speakerNotes, "표준 notesSlide1.xml 파트에 기록되는 발표자 노트")
        XCTAssertEqual(decodedDeck.slides.first?.elements.first?.isBold, true)
        XCTAssertEqual(decodedDeck.slides.first?.elements.first?.isItalic, true)
        XCTAssertEqual(decodedDeck.slides.first?.elements.first?.isUnderlined, true)

        // External PPTX where slide1.xml has NO <p:notes> tag, only ppt/notesSlides/notesSlide1.xml
        let externalPresXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
          <p:sldIdLst><p:sldId id="256" target="ppt/slides/slide1.xml"/></p:sldIdLst>
        </p:presentation>
        """
        let externalSlideXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
          <p:cSld><p:spTree></p:spTree></p:cSld>
        </p:sld>
        """
        let externalPptxWithNotes = try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "ppt/presentation.xml", data: Data(externalPresXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "ppt/slides/slide1.xml", data: Data(externalSlideXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "ppt/notesSlides/notesSlide1.xml", data: notesEntry.data, compressionMethod: .deflate)
        ])
        let decodedExternalNotesDeck = try OOXMLContainerCodec.decodePresentation(from: externalPptxWithNotes)
        XCTAssertEqual(decodedExternalNotesDeck.slides.first?.speakerNotes, "표준 notesSlide1.xml 파트에 기록되는 발표자 노트")

        // 2. BIFF8 SST (0x00FC) + LABELSST (0x00FD), RK (0x027E), and FORMULA (0x0006) decoding
        var biffStream = Data()
        // BOF (0x0809, len 16)
        BitPacking.appendUInt16LE(0x0809, to: &biffStream)
        BitPacking.appendUInt16LE(16, to: &biffStream)
        biffStream.append(Data(repeating: 0, count: 16))

        // SST (0x00FC) with 2 strings: "공유문자열A" (UTF-16LE, flag 0x01) and "ASCII_B" (compressed 8-bit, flag 0x00)
        var sstPayload = Data()
        BitPacking.appendUInt32LE(2, to: &sstPayload) // cstTotal
        BitPacking.appendUInt32LE(2, to: &sstPayload) // cstUnique
        let str0 = "공유문자열A"
        BitPacking.appendUInt16LE(UInt16(str0.utf16.count), to: &sstPayload)
        sstPayload.append(0x01) // UTF-16LE
        sstPayload.append(BitPacking.encodeUTF16LE(str0))
        let str1 = "ASCII_B"
        BitPacking.appendUInt16LE(UInt16(str1.utf8.count), to: &sstPayload)
        sstPayload.append(0x00) // Compressed 8-bit
        sstPayload.append(Data(str1.utf8))

        BitPacking.appendUInt16LE(0x00FC, to: &biffStream)
        BitPacking.appendUInt16LE(UInt16(sstPayload.count), to: &biffStream)
        biffStream.append(sstPayload)

        // LABELSST (0x00FD, len 10) at A1 (row 0, col 0) -> sstIndex 0
        BitPacking.appendUInt16LE(0x00FD, to: &biffStream)
        BitPacking.appendUInt16LE(10, to: &biffStream)
        BitPacking.appendUInt16LE(0, to: &biffStream) // row 0
        BitPacking.appendUInt16LE(0, to: &biffStream) // col 0
        BitPacking.appendUInt16LE(0, to: &biffStream) // xf
        BitPacking.appendUInt32LE(0, to: &biffStream) // sstIndex 0

        // LABELSST (0x00FD, len 10) at B1 (row 0, col 1) -> sstIndex 1
        BitPacking.appendUInt16LE(0x00FD, to: &biffStream)
        BitPacking.appendUInt16LE(10, to: &biffStream)
        BitPacking.appendUInt16LE(0, to: &biffStream) // row 0
        BitPacking.appendUInt16LE(1, to: &biffStream) // col 1
        BitPacking.appendUInt16LE(0, to: &biffStream) // xf
        BitPacking.appendUInt32LE(1, to: &biffStream) // sstIndex 1

        // RK (0x027E, len 10) at A2 (row 1, col 0): integer 750 -> (750 << 2) | 0x02
        BitPacking.appendUInt16LE(0x027E, to: &biffStream)
        BitPacking.appendUInt16LE(10, to: &biffStream)
        BitPacking.appendUInt16LE(1, to: &biffStream) // row 1
        BitPacking.appendUInt16LE(0, to: &biffStream) // col 0
        BitPacking.appendUInt16LE(0, to: &biffStream) // xf
        BitPacking.appendUInt32LE((750 << 2) | 0x02, to: &biffStream)

        // RK (0x027E, len 10) at B2 (row 1, col 1): integer 1234 / 100 = 12.34 -> (1234 << 2) | 0x03
        BitPacking.appendUInt16LE(0x027E, to: &biffStream)
        BitPacking.appendUInt16LE(10, to: &biffStream)
        BitPacking.appendUInt16LE(1, to: &biffStream) // row 1
        BitPacking.appendUInt16LE(1, to: &biffStream) // col 1
        BitPacking.appendUInt16LE(0, to: &biffStream) // xf
        BitPacking.appendUInt32LE((1234 << 2) | 0x03, to: &biffStream)

        // FORMULA (0x0006, len 14) at C2 (row 1, col 2): IEEE-754 double 99.5
        BitPacking.appendUInt16LE(0x0006, to: &biffStream)
        BitPacking.appendUInt16LE(14, to: &biffStream)
        BitPacking.appendUInt16LE(1, to: &biffStream) // row 1
        BitPacking.appendUInt16LE(2, to: &biffStream) // col 2
        BitPacking.appendUInt16LE(0, to: &biffStream) // xf
        BitPacking.appendUInt64LE(Double(99.5).bitPattern, to: &biffStream)

        // EOF (0x000A, len 0)
        BitPacking.appendUInt16LE(0x000A, to: &biffStream)
        BitPacking.appendUInt16LE(0, to: &biffStream)

        let customXlsContainer = try OLE2ContainerCodec.pack(streams: [
            OLE2StreamEntry(name: "Workbook", data: biffStream)
        ])
        let decodedCustomXls = try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: customXlsContainer, name: "SST_RK_Test")
        XCTAssertEqual(decodedCustomXls.rawValue(at: "A1"), "공유문자열A")
        XCTAssertEqual(decodedCustomXls.rawValue(at: "B1"), "ASCII_B")
        XCTAssertEqual(decodedCustomXls.evaluate(at: "A2"), .number(750))
        XCTAssertEqual(decodedCustomXls.evaluate(at: "B2"), .number(12.34))
        XCTAssertEqual(decodedCustomXls.evaluate(at: "C2"), .number(99.5))
    }
}



