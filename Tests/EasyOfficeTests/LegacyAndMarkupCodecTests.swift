import XCTest
@testable import EasyOfficeCore

final class LegacyAndMarkupCodecTests: XCTestCase {

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

        // 2. Multi-sheet + CellStyle + PageSetup + per-sheet comments & mergedRanges round-trip across .xlsx and .ods
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
        let activeMerge = try CellRange(notation: "A1:B1")
        multiSheet.toggleMergeCells(activeMerge)
        multiSheet.trackChangesEnabled = true

        let b2Coord = PackedCellCoordinate(row: 1, column: 1)
        let sheet2Merge = try CellRange(notation: "C1:D2")
        multiSheet.additionalSheets.append(
            WorksheetSheetPage(
                name: "2분기",
                cellsByCoordinate: [
                    PackedCellCoordinate(row: 0, column: 0): "300",
                    b2Coord: "=A1+50"
                ],
                sheetComments: [b2Coord: "2분기 예측 메모"],
                mergedRanges: [sheet2Merge]
            )
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
        XCTAssertEqual(decodedXlsx.additionalSheets[0].sheetComments[b2Coord], "2분기 예측 메모")
        XCTAssertEqual(decodedXlsx.additionalSheets[0].mergedRanges, [sheet2Merge])
        XCTAssertEqual(decodedXlsx.mergedRanges, [activeMerge])
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
        XCTAssertEqual(decodedOds.additionalSheets[0].sheetComments[b2Coord], "2분기 예측 메모")
        XCTAssertEqual(decodedOds.additionalSheets[0].mergedRanges, [sheet2Merge])
        XCTAssertEqual(decodedOds.mergedRanges, [activeMerge])
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

        // 2. BIFF8 Multi-Substream + 8-bit Latin-1 SST + RK + FORMULA
        var biffStream = Data()
        BitPacking.appendUInt16LE(0x0809, to: &biffStream)
        BitPacking.appendUInt16LE(16, to: &biffStream)
        BitPacking.appendUInt16LE(0x0600, to: &biffStream)
        BitPacking.appendUInt16LE(0x0005, to: &biffStream)
        biffStream.append(Data(repeating: 0, count: 12))

        var sstPayload = Data()
        BitPacking.appendUInt32LE(2, to: &sstPayload)
        BitPacking.appendUInt32LE(2, to: &sstPayload)
        let str0 = "공유문자열A"
        BitPacking.appendUInt16LE(UInt16(str0.utf16.count), to: &sstPayload)
        sstPayload.append(0x01)
        sstPayload.append(BitPacking.encodeUTF16LE(str0))
        let latin1Bytes: [UInt8] = [0x43, 0x61, 0x66, 0xE9, 0x5F, 0xA9]
        BitPacking.appendUInt16LE(UInt16(latin1Bytes.count), to: &sstPayload)
        sstPayload.append(0x00)
        sstPayload.append(contentsOf: latin1Bytes)

        BitPacking.appendUInt16LE(0x00FC, to: &biffStream)
        BitPacking.appendUInt16LE(UInt16(sstPayload.count), to: &biffStream)
        biffStream.append(sstPayload)

        BitPacking.appendUInt16LE(0x000A, to: &biffStream)
        BitPacking.appendUInt16LE(0, to: &biffStream)

        BitPacking.appendUInt16LE(0x0809, to: &biffStream)
        BitPacking.appendUInt16LE(16, to: &biffStream)
        BitPacking.appendUInt16LE(0x0600, to: &biffStream)
        BitPacking.appendUInt16LE(0x0010, to: &biffStream)
        biffStream.append(Data(repeating: 0, count: 12))

        BitPacking.appendUInt16LE(0x00FD, to: &biffStream)
        BitPacking.appendUInt16LE(10, to: &biffStream)
        BitPacking.appendUInt16LE(0, to: &biffStream)
        BitPacking.appendUInt16LE(0, to: &biffStream)
        BitPacking.appendUInt16LE(0, to: &biffStream)
        BitPacking.appendUInt32LE(0, to: &biffStream)

        BitPacking.appendUInt16LE(0x00FD, to: &biffStream)
        BitPacking.appendUInt16LE(10, to: &biffStream)
        BitPacking.appendUInt16LE(0, to: &biffStream)
        BitPacking.appendUInt16LE(1, to: &biffStream)
        BitPacking.appendUInt16LE(0, to: &biffStream)
        BitPacking.appendUInt32LE(1, to: &biffStream)

        BitPacking.appendUInt16LE(0x027E, to: &biffStream)
        BitPacking.appendUInt16LE(10, to: &biffStream)
        BitPacking.appendUInt16LE(1, to: &biffStream)
        BitPacking.appendUInt16LE(0, to: &biffStream)
        BitPacking.appendUInt16LE(0, to: &biffStream)
        BitPacking.appendUInt32LE((750 << 2) | 0x02, to: &biffStream)

        BitPacking.appendUInt16LE(0x027E, to: &biffStream)
        BitPacking.appendUInt16LE(10, to: &biffStream)
        BitPacking.appendUInt16LE(1, to: &biffStream)
        BitPacking.appendUInt16LE(1, to: &biffStream)
        BitPacking.appendUInt16LE(0, to: &biffStream)
        BitPacking.appendUInt32LE((1234 << 2) | 0x03, to: &biffStream)

        BitPacking.appendUInt16LE(0x0006, to: &biffStream)
        BitPacking.appendUInt16LE(14, to: &biffStream)
        BitPacking.appendUInt16LE(1, to: &biffStream)
        BitPacking.appendUInt16LE(2, to: &biffStream)
        BitPacking.appendUInt16LE(0, to: &biffStream)
        BitPacking.appendUInt64LE(Double(99.5).bitPattern, to: &biffStream)

        BitPacking.appendUInt16LE(0x000A, to: &biffStream)
        BitPacking.appendUInt16LE(0, to: &biffStream)

        let customXlsContainer = try OLE2ContainerCodec.pack(streams: [
            OLE2StreamEntry(name: "Workbook", data: biffStream)
        ])
        let decodedCustomXls = try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: customXlsContainer, name: "SST_RK_Test")
        XCTAssertEqual(decodedCustomXls.rawValue(at: "A1"), "공유문자열A")
        XCTAssertEqual(decodedCustomXls.rawValue(at: "B1"), "Café_©")
        XCTAssertEqual(decodedCustomXls.evaluate(at: "A2"), .number(750))
        XCTAssertEqual(decodedCustomXls.evaluate(at: "B2"), .number(12.34))
        XCTAssertEqual(decodedCustomXls.evaluate(at: "C2"), .number(99.5))

        // 3. OpenFormula quoted ']' & ';' normalization and ODS cumulative repeated-cell cap
        let odsQuotedFormulaXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
          <office:body><office:spreadsheet>
            <table:table table:name="OpenFormulaSheet">
              <table:table-row>
                <table:table-cell office:value-type="float" office:value="10"><text:p>10</text:p></table:table-cell>
                <table:table-cell table:formula="of:=IF([.A1]&gt;5; &quot;[정상]&quot;; &quot;[대기]&quot;)"><text:p>[정상]</text:p></table:table-cell>
                <table:table-cell table:number-columns-repeated="1024"/>
              </table:table-row>
            </table:table>
          </office:spreadsheet></office:body>
        </office:document-content>
        """
        let odsQuotedData = try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "mimetype", data: Data(ODFContainerCodec.spreadsheetMimeType.utf8), compressionMethod: .store),
            ZipContainerEntry(path: "content.xml", data: Data(odsQuotedFormulaXML.utf8), compressionMethod: .deflate)
        ])
        let decodedQuotedODS = try ODFContainerCodec.decodeWorksheet(from: odsQuotedData)
        XCTAssertEqual(decodedQuotedODS.rawValue(at: "B1"), "=IF(A1>5, \"[정상]\", \"[대기]\")")
        XCTAssertEqual(decodedQuotedODS.evaluate(at: "B1"), EvaluatedCellValue.text("[정상]"))

        let odsBombXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0">
          <office:body><office:spreadsheet>
            <table:table table:name="BombSheet">
              <table:table-row table:number-rows-repeated="1024">
                <table:table-cell table:number-columns-repeated="1024" office:value="1"/>
              </table:table-row>
            </table:table>
          </office:spreadsheet></office:body>
        </office:document-content>
        """
        let odsBombData = try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "mimetype", data: Data(ODFContainerCodec.spreadsheetMimeType.utf8), compressionMethod: .store),
            ZipContainerEntry(path: "content.xml", data: Data(odsBombXML.utf8), compressionMethod: .deflate)
        ])
        XCTAssertThrowsError(try ODFContainerCodec.decodeWorksheet(from: odsBombData)) { error in
            guard case .securityViolation = error as? EasyOfficeError else {
                XCTFail("Expected securityViolation on ODS cell expansion bomb, got \(error)")
                return
            }
        }
    }

    func testLosslessMetadataRoundTripAcrossHwpDocXlsRtfAndMd() throws {
        let runs = [
            DocumentTextRun(text: "굵은 런 ", isBold: true, fontName: "Pretendard", fontSizePt: 13.0, foregroundHex: "#5C6BC0"),
            DocumentTextRun(text: "밑줄 런", isItalic: true, isUnderlined: true, fontName: "NanumGothic", fontSizePt: 15.0, foregroundHex: "#3949AB")
        ]
        let para = ParagraphBlock(style: .body, text: "굵은 런 밑줄 런", runs: runs)
        var table = DocumentTableBlock(caption: "메타데이터 표", rows: [["A", "B"], ["C", "D"]], hasHeaderRow: true, borderStyle: .doubleLine)
        table.mergeCells(startRow: 0, startCol: 0, rowSpan: 1, colSpan: 2)
        let img = EmbeddedImageAttachment(
            fileName: "chart.png",
            caption: "차트 캡션",
            mimeType: "image/png",
            imageData: Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
            widthPt: 320,
            heightPt: 180
        )
        let richDoc = OfficeDocumentModel(
            title: "무손실 라운드트립 문서",
            format: .hwp,
            paperSize: .letter,
            headerText: "상단 머리글",
            footerText: "하단 바닥글",
            paragraphs: [para],
            tables: [table],
            trackChangesEnabled: true,
            sections: [DocumentSectionLayout(paperSize: .letter, orientation: .landscape, columnCount: 2)],
            footnotes: [FootnoteItem(number: 1, isEndnote: false, anchorParagraphID: para.id, text: "각주 1")],
            revisions: [TrackedRevision(author: "QA", kind: .insertion, paragraphID: para.id, originalText: "", proposedText: "굵은 런")],
            comments: [DocumentCommentThread(anchorParagraphID: para.id, author: "QA", message: "코멘트", replies: ["답글"], isResolved: true)],
            bookmarks: ["bm1": para.id],
            images: [img]
        )

        // 1. HWP 5.0 lossless metadata round-trip
        let hwpBytes = try HWP5BinaryCodec.encodeDocument(richDoc, compressed: true)
        let decodedHwp = try HWP5BinaryCodec.decodeDocument(from: hwpBytes)
        XCTAssertEqual(decodedHwp.paragraphs.first?.runs, runs)
        XCTAssertEqual(decodedHwp.tables.first?.cellSpans, table.cellSpans)
        XCTAssertEqual(decodedHwp.tables.first?.borderStyle, .doubleLine)
        XCTAssertEqual(decodedHwp.sections, richDoc.sections)
        XCTAssertEqual(decodedHwp.footnotes, richDoc.footnotes)
        XCTAssertEqual(decodedHwp.revisions, richDoc.revisions)
        XCTAssertEqual(decodedHwp.comments, richDoc.comments)
        XCTAssertEqual(decodedHwp.bookmarks, richDoc.bookmarks)
        XCTAssertEqual(decodedHwp.images, richDoc.images)

        // 2. Word Binary (.doc) lossless metadata round-trip
        let docBytes = try LegacyOLE2OfficeCodec.encodeWordBinary(richDoc)
        let decodedDoc = try LegacyOLE2OfficeCodec.decodeWordBinary(from: docBytes)
        XCTAssertEqual(decodedDoc.paragraphs.first?.runs, runs)
        XCTAssertEqual(decodedDoc.tables.first?.cellSpans, table.cellSpans)
        XCTAssertEqual(decodedDoc.tables.first?.borderStyle, .doubleLine)
        XCTAssertEqual(decodedDoc.sections, richDoc.sections)
        XCTAssertEqual(decodedDoc.footnotes, richDoc.footnotes)
        XCTAssertEqual(decodedDoc.revisions, richDoc.revisions)
        XCTAssertEqual(decodedDoc.comments, richDoc.comments)
        XCTAssertEqual(decodedDoc.bookmarks, richDoc.bookmarks)
        XCTAssertEqual(decodedDoc.images, richDoc.images)

        // 3. RTF and CommonMark Markdown lossless runs & images round-trip
        let rtfBytes = RTFCodec.encodeDocument(richDoc)
        let decodedRtf = try RTFCodec.decodeDocument(from: rtfBytes)
        XCTAssertEqual(decodedRtf.paragraphs.first?.runs, runs)
        XCTAssertEqual(decodedRtf.images, richDoc.images)
        XCTAssertEqual(decodedRtf.tables.first?.cellSpans, table.cellSpans)

        let mdBytes = CommonMarkCodec.encodeDocument(richDoc)
        let decodedMd = try CommonMarkCodec.decodeDocument(from: mdBytes)
        XCTAssertEqual(decodedMd.paragraphs.first?.runs, runs)
        XCTAssertEqual(decodedMd.images, richDoc.images)
        XCTAssertEqual(decodedMd.tables.first?.cellSpans, table.cellSpans)

        // 4. BIFF8 (.xls) lossless mergedRanges, sheetComments, cellStyles, and additionalSheets round-trip
        var richSheet = WorksheetModel(name: "본시트")
        try richSheet.setCell("A1", rawValue: "100")
        try richSheet.setCell("B1", rawValue: "200")
        let mergeRange = try CellRange(notation: "A1:B1")
        richSheet.toggleMergeCells(mergeRange)
        richSheet.sheetComments[PackedCellCoordinate(row: 0, column: 0)] = "XLS 메모"
        let subRange = try CellRange(notation: "A1:C1")
        richSheet.additionalSheets.append(
            WorksheetSheetPage(
                name: "보조시트",
                cellsByCoordinate: [PackedCellCoordinate(row: 0, column: 0): "55"],
                sheetComments: [PackedCellCoordinate(row: 0, column: 0): "보조시트 메모"],
                mergedRanges: [subRange]
            )
        )
        let xlsBytes = try LegacyOLE2OfficeCodec.encodeBIFF8Worksheet(richSheet)
        let decodedXls = try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: xlsBytes)
        XCTAssertEqual(decodedXls.mergedRanges, [mergeRange])
        XCTAssertEqual(decodedXls.sheetComments[PackedCellCoordinate(row: 0, column: 0)], "XLS 메모")
        XCTAssertEqual(decodedXls.additionalSheets.first?.name, "보조시트")
        XCTAssertEqual(decodedXls.additionalSheets.first?.sheetComments[PackedCellCoordinate(row: 0, column: 0)], "보조시트 메모")
        XCTAssertEqual(decodedXls.additionalSheets.first?.mergedRanges, [subRange])
    }

    func testXMLSanitizerNumericEntitiesAndLongUnknownEntityWindow() {
        let sample = "Dec: &#65;&#44032; | Hex: &#x41;&#xAC00; | UnknownLong: &veryLongUnclosedOrCustomEntityNameThatExceeds14Chars; | Named: &amp;&lt;&gt;&quot;&apos;"
        let unescaped = XMLSanitizer.unescape(sample)
        XCTAssertEqual(
            unescaped,
            "Dec: A가 | Hex: A가 | UnknownLong: &veryLongUnclosedOrCustomEntityNameThatExceeds14Chars; | Named: &<>\"'"
        )
    }
}
