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
            try BitPacking.writeUInt32LE(currentFlags | 0x0000_0002, into: &modified, at: 36)
            return OLE2StreamEntry(name: stream.name, data: modified)
        }
        let encryptedHwpData = try OLE2ContainerCodec.pack(streams: encryptedStreams)

        XCTAssertThrowsError(try HWP5BinaryCodec.decodeDocument(from: encryptedHwpData)) { error in
            XCTAssertEqual(error as? EasyOfficeError, .encryptedDocumentUnsupported(format: .hwp))
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

        // 2. CommonMark Markdown Codec
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
                        ["CRC-32", "단일 패스"]
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
        XCTAssertEqual(decodedMD.tables[0].rows, [["항목", "수치"], ["CRC-32", "단일 패스"]])

        // 3. RTF Codec with Korean Unicode \uN? escapes
        let rtfDoc = OfficeDocumentModel(
            title: "한글 RTF 문서",
            format: .rtf,
            paragraphs: [
                ParagraphBlock(style: .title, text: "유니코드 이스케이프 검증", isBold: true),
                ParagraphBlock(style: .body, text: "한글과 ASCII(123) 및 중괄호 \\ { } 이스케이프를 검증합니다.")
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
        XCTAssertEqual(decodedRTF.paragraphs[1].text, "한글과 ASCII(123) 및 중괄호 \\ { } 이스케이프를 검증합니다.")
        XCTAssertEqual(decodedRTF.tables.count, 1)
        XCTAssertEqual(decodedRTF.tables[0].rows, [["키", "값"], ["언어", "한국어"]])
    }
}
