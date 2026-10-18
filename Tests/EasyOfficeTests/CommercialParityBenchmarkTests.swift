import XCTest

@testable import EasyOfficeCore
@testable import EasyOfficeDesignSystem

final class CommercialParityBenchmarkTests: XCTestCase {

  // MARK: - 1. 20-Format Catalog & End-to-End Round-Trip Matrix

  func testAll20FormatsCatalogAndEndToEndRoundTripMatrix() throws {
    // 1. Verify full 20-format catalog across all 5 office module domains
    let allFormats = DocumentFormat.allCases
    XCTAssertEqual(allFormats.count, 20)
    XCTAssertEqual(OfficeModuleKind.allCases.count, 5)

    let groupedByModule = Dictionary(grouping: allFormats, by: \.moduleKind)
    XCTAssertEqual(
      Set(groupedByModule[.document] ?? []), Set([.docx, .doc, .odt, .rtf, .md, .txt]))
    XCTAssertEqual(Set(groupedByModule[.hwp] ?? []), Set([.hwp, .hwpx, .hwt, .hwtx]))
    XCTAssertEqual(Set(groupedByModule[.sheet] ?? []), Set([.xlsx, .xls, .ods, .csv, .tsv]))
    XCTAssertEqual(Set(groupedByModule[.slide] ?? []), Set([.pptx, .ppt, .ppsx, .odp]))
    XCTAssertEqual(Set(groupedByModule[.pdf] ?? []), Set([.pdf]))

    for format in allFormats {
      XCTAssertFalse(format.standardSpecification.isEmpty)
      XCTAssertEqual(
        DocumentFormat.from(fileNameOrExtension: "sample.\(format.fileExtension)"), format)
      XCTAssertEqual(
        DocumentFormat.from(fileNameOrExtension: format.fileExtension.uppercased()), format)
    }

    // 2. Construct rich baseline models for Document/HWP, Sheet, Slide, and PDF
    let masterDocument = OfficeDocumentModel(
      title: "2026 통합 표준 호환성 검증 보고서 (Compatibility Report)",
      format: .docx,
      paperSize: .a4,
      headerText: "문서번호: EO-STD-2026-01",
      footerText: "대외비 · Page 1",
      paragraphs: [
        ParagraphBlock(
          style: .title,
          alignment: .center,
          text: "국제·국가 표준 문서 컨테이너 라운드트립 검증",
          fontName: "Pretendard",
          fontSizePt: 20.0,
          isBold: true,
          lineSpacingPercent: 160.0
        ),
        ParagraphBlock(
          style: .heading1,
          alignment: .leading,
          text: "1. 한글 및 영문 유니코드 무결성 (Korean & English Fidelity)",
          fontName: "Pretendard",
          fontSizePt: 14.0,
          isBold: true,
          isItalic: true,
          lineSpacingPercent: 150.0
        ),
        ParagraphBlock(
          style: .body,
          alignment: .justified,
          text: "모든 컨테이너 코덱은 매직 바이트 검증 후 문단 스타일, 정렬, 표 셀 데이터를 손실 없이 복원합니다.",
          fontName: "NanumMyeongjo",
          fontSizePt: 11.0,
          isUnderlined: true,
          lineSpacingPercent: 160.0
        ),
      ],
      tables: [
        DocumentTableBlock(
          caption: "표 1. 컨테이너별 표준 규격 매트릭스",
          rows: [
            ["구분", "표준 규격", "상태"],
            ["OOXML / ODF", "ECMA-376 / ISO 26300", "검증완료"],
            ["HWP / HWPX", "OLE2 CFBF / KS X 6101", "검증완료"],
          ],
          hasHeaderRow: true
        )
      ],
      trackChangesEnabled: true
    )

    var masterSheet = WorksheetModel(name: "통합재무시트_2026")
    try masterSheet.setCell("A1", rawValue: "지역")
    try masterSheet.setCell("B1", rawValue: "1분기")
    try masterSheet.setCell("C1", rawValue: "2분기")
    try masterSheet.setCell("D1", rawValue: "합계")
    try masterSheet.setCell("A2", rawValue: "서울 본사")
    try masterSheet.setCell("B2", rawValue: "1250")
    try masterSheet.setCell("C2", rawValue: "1750")
    try masterSheet.setCell("D2", rawValue: "=SUM(B2:C2)")
    try masterSheet.setCell("A3", rawValue: "판교 연구소")
    try masterSheet.setCell("B3", rawValue: "980")
    try masterSheet.setCell("C3", rawValue: "1120")
    try masterSheet.setCell("D3", rawValue: "=SUM(B3:C3)")
    try masterSheet.setCell("D4", rawValue: "=AVERAGE(D2:D3)")

    let masterDeck = PresentationDeckModel(
      title: "2026 아키텍처 발표자료 (Architecture Deck)",
      aspectRatio: .widescreen16x9,
      slides: [
        SlideModel(
          title: "올인원 네이티브 워크스페이스",
          subtitle: "시그니처 라벤더 #5C6BC0 디자인 시스템",
          bulletPoints: [
            "5대 오피스 모듈 단일 윈도우 통합",
            "Fail-Closed 매직 바이트 보안 검증",
          ],
          speakerNotes: "발표자 노트: 첫 슬라이드에서 핵심 보안 아키텍처를 설명합니다.",
          masterLayout: .titleAndContent,
          transition: .morph,
          elements: [
            SlideCanvasElement(
              title: "핵심 지표 박스",
              bodyText: "20종 포맷 · 77종 수식",
              x: 380.0,
              y: 220.0,
              width: 200.0,
              height: 100.0
            )
          ]
        )
      ]
    )

    let masterPDF = PDFStudioDocumentModel(
      title: "ISO 32000 보안 문서 마스터",
      pages: [
        PDFPageDescriptor(
          pageNumber: 1,
          rotationDegrees: 90,
          extractedText: "제1장 PDF 바이너리 직렬화 및 페이지 회전 보존"
        ),
        PDFPageDescriptor(
          pageNumber: 2,
          rotationDegrees: 180,
          extractedText: "제2장 주석 스레드 및 책갈피 트리 무결성 검증"
        ),
      ],
      annotations: [
        PDFAnnotationItem(
          pageNumber: 1,
          kind: .highlight,
          content: "회전 각도 90도 및 주석 보존 확인",
          author: "QA"
        ),
        PDFAnnotationItem(
          pageNumber: 2,
          kind: .stamp,
          content: "릴리스 게이트 승인 완료",
          author: "QA"
        ),
      ],
      outline: [
        PDFOutlineNode(title: "제1장 직렬화", targetPageNumber: 1),
        PDFOutlineNode(title: "제2장 주석 무결성", targetPageNumber: 2),
      ],
      isRedacted: false
    )

    // 3. OOXML (.docx, .xlsx, .pptx) round-trip
    let docxBytes = try OOXMLContainerCodec.encodeDocument(masterDocument)
    let docxSig = try FormatDetector.verify(data: docxBytes, fileName: "report.docx")
    XCTAssertEqual(docxSig.format, .docx)
    XCTAssertEqual(docxSig.containerFamily, .zipXML)
    let decodedDocx = try OOXMLContainerCodec.decodeDocument(from: docxBytes)
    XCTAssertEqual(decodedDocx.title, masterDocument.title)
    XCTAssertEqual(decodedDocx.headerText, masterDocument.headerText)
    XCTAssertEqual(decodedDocx.footerText, masterDocument.footerText)
    XCTAssertEqual(decodedDocx.trackChangesEnabled, true)
    XCTAssertEqual(decodedDocx.paragraphs.map(\.text), masterDocument.paragraphs.map(\.text))
    XCTAssertEqual(decodedDocx.paragraphs.map(\.style), masterDocument.paragraphs.map(\.style))
    XCTAssertEqual(decodedDocx.tables.first?.rows, masterDocument.tables.first?.rows)

    let xlsxBytes = try OOXMLContainerCodec.encodeWorksheet(masterSheet)
    let xlsxSig = try FormatDetector.verify(data: xlsxBytes, fileName: "finance.xlsx")
    XCTAssertEqual(xlsxSig.format, .xlsx)
    let decodedXlsx = try OOXMLContainerCodec.decodeWorksheet(from: xlsxBytes)
    XCTAssertEqual(decodedXlsx.name, masterSheet.name)
    XCTAssertEqual(decodedXlsx.rawValue(at: "D2"), "=SUM(B2:C2)")
    XCTAssertEqual(decodedXlsx.evaluate(at: "D2"), .number(3000.0))
    XCTAssertEqual(decodedXlsx.evaluate(at: "D3"), .number(2100.0))
    XCTAssertEqual(decodedXlsx.evaluate(at: "D4"), .number(2550.0))

    let pptxBytes = try OOXMLContainerCodec.encodePresentation(masterDeck)
    let pptxSig = try FormatDetector.verify(data: pptxBytes, fileName: "deck.pptx")
    XCTAssertEqual(pptxSig.format, .pptx)
    let decodedPptx = try OOXMLContainerCodec.decodePresentation(from: pptxBytes)
    XCTAssertEqual(decodedPptx.title, masterDeck.title)
    XCTAssertEqual(decodedPptx.aspectRatio, .widescreen16x9)
    XCTAssertEqual(decodedPptx.slides.count, 1)
    XCTAssertEqual(decodedPptx.slides[0].title, masterDeck.slides[0].title)
    XCTAssertEqual(decodedPptx.slides[0].speakerNotes, masterDeck.slides[0].speakerNotes)
    XCTAssertEqual(decodedPptx.slides[0].elements.first?.bodyText, "20종 포맷 · 77종 수식")

    // 4. ODF (.odt, .ods, .odp) round-trip
    let odtBytes = try ODFContainerCodec.encodeDocument(masterDocument)
    let odtSig = try FormatDetector.verify(data: odtBytes, fileName: "report.odt")
    XCTAssertEqual(odtSig.format, .odt)
    let decodedOdt = try ODFContainerCodec.decodeDocument(from: odtBytes)
    XCTAssertEqual(decodedOdt.title, masterDocument.title)
    XCTAssertEqual(decodedOdt.paragraphs.map(\.text), masterDocument.paragraphs.map(\.text))
    XCTAssertEqual(decodedOdt.tables.first?.rows, masterDocument.tables.first?.rows)

    let odsBytes = try ODFContainerCodec.encodeWorksheet(masterSheet)
    let odsSig = try FormatDetector.verify(data: odsBytes, fileName: "finance.ods")
    XCTAssertEqual(odsSig.format, .ods)
    let decodedOds = try ODFContainerCodec.decodeWorksheet(from: odsBytes)
    XCTAssertEqual(decodedOds.name, masterSheet.name)
    XCTAssertEqual(decodedOds.evaluate(at: "D2"), .number(3000.0))
    XCTAssertEqual(decodedOds.evaluate(at: "D4"), .number(2550.0))

    let odpBytes = try ODFContainerCodec.encodePresentation(masterDeck)
    let odpSig = try FormatDetector.verify(data: odpBytes, fileName: "deck.odp")
    XCTAssertEqual(odpSig.format, .odp)
    let decodedOdp = try ODFContainerCodec.decodePresentation(from: odpBytes)
    XCTAssertEqual(decodedOdp.title, masterDeck.title)
    XCTAssertEqual(decodedOdp.slides[0].speakerNotes, masterDeck.slides[0].speakerNotes)
    XCTAssertEqual(decodedOdp.slides[0].elements.first?.title, "핵심 지표 박스")

    // 5. HWPX (KS X 6101 OWPML) & HWP 5.0 (OLE2 CFBF) round-trip
    let hwpxBytes = try HWPXContainerCodec.encodeDocument(masterDocument)
    let hwpxSig = try FormatDetector.verify(data: hwpxBytes, fileName: "official.hwpx")
    XCTAssertEqual(hwpxSig.format, .hwpx)
    let decodedHwpx = try HWPXContainerCodec.decodeDocument(from: hwpxBytes)
    XCTAssertEqual(decodedHwpx.title, masterDocument.title)
    XCTAssertEqual(decodedHwpx.paragraphs.map(\.text), masterDocument.paragraphs.map(\.text))
    XCTAssertEqual(decodedHwpx.tables.first?.rows, masterDocument.tables.first?.rows)

    let hwp5Bytes = try HWP5BinaryCodec.encodeDocument(masterDocument)
    let hwp5Sig = try FormatDetector.verify(data: hwp5Bytes, fileName: "official.hwp")
    XCTAssertEqual(hwp5Sig.format, .hwp)
    XCTAssertEqual(hwp5Sig.containerFamily, .ole2Compound)
    let decodedHwp5 = try HWP5BinaryCodec.decodeDocument(from: hwp5Bytes)
    XCTAssertEqual(decodedHwp5.title, masterDocument.title)
    XCTAssertEqual(decodedHwp5.headerText, masterDocument.headerText)
    XCTAssertEqual(decodedHwp5.paragraphs.map(\.text), masterDocument.paragraphs.map(\.text))
    XCTAssertEqual(decodedHwp5.tables.first?.rows, masterDocument.tables.first?.rows)

    // 6. PDF (ISO 32000) round-trip
    let pdfBytes = PDFBinarySerializer.encode(masterPDF)
    let pdfSig = try FormatDetector.verify(data: pdfBytes, fileName: "studio.pdf")
    XCTAssertEqual(pdfSig.format, .pdf)
    XCTAssertEqual(pdfSig.containerFamily, .pdfBinary)
    let decodedPDF = try PDFBinarySerializer.decode(from: pdfBytes)
    XCTAssertEqual(decodedPDF.title, masterPDF.title)
    XCTAssertEqual(decodedPDF.pages.count, 2)
    XCTAssertEqual(decodedPDF.pages[0].rotationDegrees, 90)
    XCTAssertEqual(decodedPDF.pages[1].rotationDegrees, 180)
    XCTAssertEqual(decodedPDF.annotations.count, 2)
    XCTAssertEqual(decodedPDF.annotations[0].content, "회전 각도 90도 및 주석 보존 확인")
    XCTAssertEqual(decodedPDF.outline.map(\.title), ["제1장 직렬화", "제2장 주석 무결성"])

    // 7. CSV, TSV, CommonMark Markdown (.md), and RTF (.rtf) round-trip
    let csvBytes = CSVStateMachineCodec.encodeWorksheet(masterSheet, delimiter: ",")
    let csvSig = try FormatDetector.verify(data: csvBytes, fileName: "finance.csv")
    XCTAssertEqual(csvSig.format, .csv)
    let decodedCsv = try CSVStateMachineCodec.decodeWorksheet(
      from: csvBytes, name: masterSheet.name, delimiter: ",")
    XCTAssertEqual(decodedCsv.evaluate(at: "D2"), .number(3000.0))
    XCTAssertEqual(decodedCsv.evaluate(at: "D4"), .number(2550.0))

    let tsvBytes = CSVStateMachineCodec.encodeWorksheet(masterSheet, delimiter: "\t")
    let tsvSig = try FormatDetector.verify(data: tsvBytes, fileName: "finance.tsv")
    XCTAssertEqual(tsvSig.format, .tsv)
    let decodedTsv = try CSVStateMachineCodec.decodeWorksheet(
      from: tsvBytes, name: masterSheet.name, delimiter: "\t")
    XCTAssertEqual(decodedTsv.evaluate(at: "D3"), .number(2100.0))

    let mdBytes = CommonMarkCodec.encodeDocument(masterDocument)
    let mdSig = try FormatDetector.verify(data: mdBytes, fileName: "report.md")
    XCTAssertEqual(mdSig.format, .md)
    let decodedMd = try CommonMarkCodec.decodeDocument(from: mdBytes)
    XCTAssertEqual(decodedMd.paragraphs.map(\.text), masterDocument.paragraphs.map(\.text))
    XCTAssertEqual(decodedMd.tables.first?.rows, masterDocument.tables.first?.rows)

    let rtfBytes = RTFCodec.encodeDocument(masterDocument)
    let rtfSig = try FormatDetector.verify(data: rtfBytes, fileName: "report.rtf")
    XCTAssertEqual(rtfSig.format, .rtf)
    XCTAssertEqual(rtfSig.containerFamily, .rtfText)
    let decodedRtf = try RTFCodec.decodeDocument(from: rtfBytes)
    XCTAssertEqual(decodedRtf.title, masterDocument.title)
    XCTAssertEqual(decodedRtf.paragraphs.map(\.text), masterDocument.paragraphs.map(\.text))
    XCTAssertEqual(decodedRtf.tables.first?.rows, masterDocument.tables.first?.rows)

    // 8. Verify remaining template & legacy formats (.doc, .hwt, .hwtx, .xls, .ppt, .ppsx, .txt) with full byte decoding
    let docResult = try DocumentConversionOrchestrator.convert(
      ConversionJobRequest(
        sourceFileName: "master.docx",
        sourceFormat: .docx,
        targetFormat: .doc,
        payload: .document(masterDocument)
      )
    )
    XCTAssertEqual(docResult.verifiedSignature.format, .doc)
    XCTAssertEqual(docResult.verifiedSignature.containerFamily, .ole2Compound)
    let decodedLegacyDoc = try LegacyOLE2OfficeCodec.decodeWordBinary(from: docResult.outputBytes)
    XCTAssertEqual(decodedLegacyDoc.title, masterDocument.title)
    XCTAssertEqual(decodedLegacyDoc.paragraphs.map(\.text), masterDocument.paragraphs.map(\.text))

    let hwtResult = try DocumentConversionOrchestrator.convert(
      ConversionJobRequest(
        sourceFileName: "master.docx",
        sourceFormat: .docx,
        targetFormat: .hwt,
        payload: .document(masterDocument)
      )
    )
    XCTAssertEqual(hwtResult.verifiedSignature.format, .hwt)
    XCTAssertEqual(hwtResult.verifiedSignature.containerFamily, .ole2Compound)
    let decodedHwtDoc = try HWP5BinaryCodec.decodeDocument(from: hwtResult.outputBytes)
    XCTAssertEqual(decodedHwtDoc.title, masterDocument.title)
    XCTAssertEqual(decodedHwtDoc.paragraphs.map(\.text), masterDocument.paragraphs.map(\.text))

    let hwtxResult = try DocumentConversionOrchestrator.convert(
      ConversionJobRequest(
        sourceFileName: "master.docx",
        sourceFormat: .docx,
        targetFormat: .hwtx,
        payload: .document(masterDocument)
      )
    )
    XCTAssertEqual(hwtxResult.verifiedSignature.format, .hwtx)
    let decodedHwtx = try HWPXContainerCodec.decodeDocument(from: hwtxResult.outputBytes)
    XCTAssertEqual(decodedHwtx.title, masterDocument.title)
    XCTAssertEqual(decodedHwtx.paragraphs.map(\.text), masterDocument.paragraphs.map(\.text))

    let xlsResult = try DocumentConversionOrchestrator.convert(
      ConversionJobRequest(
        sourceFileName: "master.xlsx",
        sourceFormat: .xlsx,
        targetFormat: .xls,
        payload: .sheet(masterSheet)
      )
    )
    XCTAssertEqual(xlsResult.verifiedSignature.format, .xls)
    let decodedXlsSheet = try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(
      from: xlsResult.outputBytes, name: masterSheet.name)
    XCTAssertEqual(decodedXlsSheet.rawValue(at: "A1"), masterSheet.rawValue(at: "A1"))
    XCTAssertEqual(decodedXlsSheet.rawValue(at: "B2"), masterSheet.rawValue(at: "B2"))

    let pptResult = try DocumentConversionOrchestrator.convert(
      ConversionJobRequest(
        sourceFileName: "master.pptx",
        sourceFormat: .pptx,
        targetFormat: .ppt,
        payload: .slide(masterDeck)
      )
    )
    XCTAssertEqual(pptResult.verifiedSignature.format, .ppt)
    let decodedPptDeck = try LegacyOLE2OfficeCodec.decodePowerPointBinary(
      from: pptResult.outputBytes)
    XCTAssertEqual(decodedPptDeck.title, masterDeck.title)
    XCTAssertEqual(decodedPptDeck.slides.count, masterDeck.slides.count)

    let ppsxResult = try DocumentConversionOrchestrator.convert(
      ConversionJobRequest(
        sourceFileName: "master.pptx",
        sourceFormat: .pptx,
        targetFormat: .ppsx,
        payload: .slide(masterDeck)
      )
    )
    XCTAssertEqual(ppsxResult.verifiedSignature.format, .ppsx)
    let decodedPpsxDeck = try OOXMLContainerCodec.decodePresentation(from: ppsxResult.outputBytes)
    XCTAssertEqual(decodedPpsxDeck.title, masterDeck.title)
    XCTAssertEqual(decodedPpsxDeck.slides.map(\.title), masterDeck.slides.map(\.title))

    let txtResult = try DocumentConversionOrchestrator.convert(
      ConversionJobRequest(
        sourceFileName: "master.docx",
        sourceFormat: .docx,
        targetFormat: .txt,
        payload: .document(masterDocument)
      )
    )
    XCTAssertEqual(txtResult.verifiedSignature.format, .txt)
    let decodedTxt = try XCTUnwrap(String(data: txtResult.outputBytes, encoding: .utf8))
    XCTAssertTrue(decodedTxt.contains(masterDocument.title))
    XCTAssertTrue(decodedTxt.contains(masterDocument.paragraphs[0].text))
  }

  // MARK: - 2. Bit-Level Low-Level Performance Benchmarks

  func testBitLevelLowLevelPerformanceBenchmarks() throws {
    let clock = ContinuousClock()

    // Benchmark A: IEEE 802.3 CRC-32 Lookup Table over 256 KB buffer
    XCTAssertEqual(CRC32Calculator.lookupTable.count, 256)
    XCTAssertEqual(CRC32Calculator.compute(for: Data("123456789".utf8)), 0xCBF4_3926)

    let bufferSize = 256 * 1024
    var deterministicBuffer = Data(count: bufferSize)
    deterministicBuffer.withUnsafeMutableBytes { rawPtr in
      guard let base = rawPtr.bindMemory(to: UInt8.self).baseAddress else { return }
      for idx in 0..<bufferSize {
        base[idx] = UInt8((idx &* 31 &+ 17) & 0xFF)
      }
    }

    var crcChecksum: UInt32 = 0
    let crcElapsed = clock.measure {
      crcChecksum = CRC32Calculator.compute(for: deterministicBuffer)
    }
    let crcRepeat = CRC32Calculator.compute(for: deterministicBuffer)
    XCTAssertNotEqual(crcChecksum, 0)
    XCTAssertEqual(crcChecksum, crcRepeat)
    XCTAssertLessThan(crcElapsed, .seconds(1))

    // Benchmark B: BitPacking Little-Endian Primitives & UTF-16LE Round-Trip (5,000 records)
    let bitPackIterations = 5_000
    var bitPackedData = Data()
    bitPackedData.reserveCapacity(bitPackIterations * 28)
    var unpackedChecksum: UInt64 = 0
    var lastUnpackedText = ""

    let bitPackingElapsed = try clock.measure {
      for idx in 0..<bitPackIterations {
        BitPacking.appendUInt16LE(UInt16(idx & 0xFFFF), to: &bitPackedData)
        BitPacking.appendUInt32LE(UInt32(idx &* 17), to: &bitPackedData)
        BitPacking.appendUInt64LE(UInt64(idx &* 101), to: &bitPackedData)
        BitPacking.appendLengthPrefixedUTF16LE("한글비트패킹", to: &bitPackedData)
      }
      var offset = 0
      for _ in 0..<bitPackIterations {
        let v16 = try BitPacking.readUInt16LE(from: bitPackedData, at: offset)
        offset += 2
        let v32 = try BitPacking.readUInt32LE(from: bitPackedData, at: offset)
        offset += 4
        let v64 = try BitPacking.readUInt64LE(from: bitPackedData, at: offset)
        offset += 8
        let text = try BitPacking.readLengthPrefixedUTF16LE(from: bitPackedData, offset: &offset)
        unpackedChecksum &+= UInt64(v16) &+ UInt64(v32) &+ v64
        lastUnpackedText = text
      }
    }
    XCTAssertGreaterThan(unpackedChecksum, 0)
    XCTAssertEqual(lastUnpackedText, "한글비트패킹")
    XCTAssertLessThan(bitPackingElapsed, .seconds(1))

    // Benchmark C: HWP 5.0 32-bit/64-bit Record Header Pack & Unpack (10,000 headers)
    let headerCount = 10_000
    var packedHeaderStream = Data()
    packedHeaderStream.reserveCapacity(headerCount * 6)
    var decodedHeaders: [HWP5RecordHeader] = []
    decodedHeaders.reserveCapacity(headerCount)

    let hwpHeaderElapsed = try clock.measure {
      for idx in 0..<headerCount {
        let tagID = UInt16((idx % 900) + 16)
        let level = UInt16(idx % 64)
        let payloadSize: UInt32 =
          (idx % 2 == 0)
          ? UInt32(idx % 4095)
          : UInt32(4095 + idx * 13)
        let encoded = try HWP5RecordHeader.encode(
          tagID: tagID, level: level, payloadSize: payloadSize)
        packedHeaderStream.append(encoded)
      }

      var cursor = 0
      while cursor < packedHeaderStream.count {
        let (header, byteLen) = try HWP5RecordHeader.decode(from: packedHeaderStream, at: cursor)
        decodedHeaders.append(header)
        cursor += byteLen
      }
    }

    XCTAssertEqual(decodedHeaders.count, headerCount)
    XCTAssertEqual(decodedHeaders[0], HWP5RecordHeader(tagID: 16, level: 0, payloadSize: 0))
    XCTAssertEqual(decodedHeaders[1], HWP5RecordHeader(tagID: 17, level: 1, payloadSize: 4108))
    XCTAssertEqual(
      decodedHeaders[headerCount - 1],
      HWP5RecordHeader(
        tagID: UInt16(((headerCount - 1) % 900) + 16),
        level: UInt16((headerCount - 1) % 64),
        payloadSize: UInt32(4095 + (headerCount - 1) * 13)
      )
    )
    XCTAssertLessThan(hwpHeaderElapsed, .seconds(1))

    // Benchmark D: PackedCellCoordinate 64-Bit Coordinate Packing, Hashing & Row-Major Sorting (10,000 cells)
    var coordinates: [PackedCellCoordinate] = []
    coordinates.reserveCapacity(10_000)
    var coordinateSet = Set<PackedCellCoordinate>(minimumCapacity: 10_000)
    var unpackedRowColSum = 0

    let coordElapsed = clock.measure {
      for r in stride(from: 99, through: 0, by: -1) {
        for c in stride(from: 99, through: 0, by: -1) {
          let packed = PackedCellCoordinate(row: r, column: c)
          coordinates.append(packed)
          coordinateSet.insert(packed)
        }
      }
      coordinates.sort()
      for item in coordinates {
        unpackedRowColSum += item.row + item.column
      }
    }

    XCTAssertEqual(coordinates.count, 10_000)
    XCTAssertEqual(coordinateSet.count, 10_000)
    XCTAssertEqual(coordinates.first, PackedCellCoordinate(row: 0, column: 0))
    XCTAssertEqual(coordinates[105], PackedCellCoordinate(row: 1, column: 5))
    XCTAssertEqual(coordinates.last, PackedCellCoordinate(row: 99, column: 99))
    // Sum of (r + c) over 0..<100 x 0..<100 = 2 * 100 * (99 * 100 / 2) = 990,000
    XCTAssertEqual(unpackedRowColSum, 990_000)
    XCTAssertLessThan(coordElapsed, .seconds(1))
  }

  // MARK: - 3. Dense Spreadsheet Formula & Pivot Engine Benchmark (200 Rows x 5 Columns)

  func testSpreadsheetFormulaAndPivotBenchmarkOnDenseGrid() throws {
    XCTAssertEqual(WorksheetModel.supportedFunctionNames.count, 122)
    let clock = ContinuousClock()
    var sheet = WorksheetModel(name: "전국지사_매출집계_1000셀")
    let regions = ["서울", "부산", "대전", "광주", "판교"]

    var pivotSummary: PivotTableSummary?
    var chartModel: SheetChartModel?

    let elapsed = try clock.measure {
      // Row 1: Header row (5 cells)
      try sheet.setCell("A1", rawValue: "지역")
      try sheet.setCell("B1", rawValue: "기준단가")
      try sheet.setCell("C1", rawValue: "수량")
      try sheet.setCell("D1", rawValue: "매출액")
      try sheet.setCell("E1", rawValue: "등급판정")

      // Rows 2...196: 195 regional transaction rows (39 cycles of 5 regions = 975 cells)
      for row in 2...196 {
        let regionIdx = (row - 2) % regions.count
        let regionName = regions[regionIdx]
        let unitPrice = (regionIdx + 1) * 100  // 100, 200, 300, 400, 500
        try sheet.setCell("A\(row)", rawValue: regionName)
        try sheet.setCell("B\(row)", rawValue: "\(unitPrice)")
        try sheet.setCell("C\(row)", rawValue: "2")
        try sheet.setCell("D\(row)", rawValue: "=ROUND(B\(row)*C\(row), 0)")
        try sheet.setCell("E\(row)", rawValue: "=IF(D\(row)>=600, \"핵심\", \"일반\")")
      }

      // Rows 197...200: 4 KPI & lookup summary rows (20 cells -> total 1,000 populated cells)
      try sheet.setCell("A197", rawValue: "전체합계")
      try sheet.setCell("B197", rawValue: "=SUM(B2:B196)")
      try sheet.setCell("C197", rawValue: "=SUM(C2:C196)")
      try sheet.setCell("D197", rawValue: "=SUM(D2:D196)")
      try sheet.setCell("E197", rawValue: "=COUNTIF(E2:E196, \"핵심\")")

      try sheet.setCell("A198", rawValue: "전체평균")
      try sheet.setCell("B198", rawValue: "=AVERAGE(B2:B196)")
      try sheet.setCell("C198", rawValue: "=AVERAGE(C2:C196)")
      try sheet.setCell("D198", rawValue: "=AVERAGE(D2:D196)")
      try sheet.setCell("E198", rawValue: "=COUNTIF(E2:E196, \"일반\")")

      try sheet.setCell("A199", rawValue: "판교집계")
      try sheet.setCell("B199", rawValue: "=SUMIF(A2:A196, \"판교\", B2:B196)")
      try sheet.setCell("C199", rawValue: "=COUNTIF(A2:A196, \"판교\")")
      try sheet.setCell("D199", rawValue: "=SUMIF(A2:A196, \"판교\", D2:D196)")
      try sheet.setCell("E199", rawValue: "=VLOOKUP(\"판교\", A2:E6, 5, FALSE)")

      try sheet.setCell("A200", rawValue: "광주조회")
      try sheet.setCell("B200", rawValue: "=VLOOKUP(\"광주\", A2:E6, 2, FALSE)")
      try sheet.setCell("C200", rawValue: "=VLOOKUP(\"광주\", A2:E6, 3, FALSE)")
      try sheet.setCell("D200", rawValue: "=VLOOKUP(\"광주\", A2:E6, 4, FALSE)")
      try sheet.setCell("E200", rawValue: "=IF(D197=117000, \"검산일치\", \"불일치\")")

      let dataRange = try CellRange(notation: "A1:E196")
      pivotSummary = PivotTableEngine.compute(
        from: sheet,
        range: dataRange,
        hasHeaderRow: true,
        configuration: PivotTableConfiguration(
          rowFieldColumnIndex: 0,
          valueFieldColumnIndex: 3,
          aggregation: .sum
        )
      )

      chartModel = SheetChartBuilder.buildChart(
        from: sheet,
        title: "지역별 표본 매출 차트",
        kind: .column,
        categoryRange: try CellRange(notation: "A2:A6"),
        valueRanges: [
          (name: "매출액", range: try CellRange(notation: "D2:D6"))
        ]
      )
    }

    XCTAssertEqual(sheet.rawCells.count, 1_000)
    XCTAssertEqual(sheet.evaluate(at: "B197"), .number(58_500.0))
    XCTAssertEqual(sheet.evaluate(at: "C197"), .number(390.0))
    XCTAssertEqual(sheet.evaluate(at: "D197"), .number(117_000.0))
    XCTAssertEqual(sheet.evaluate(at: "E197"), .number(117.0))

    XCTAssertEqual(sheet.evaluate(at: "B198"), .number(300.0))
    XCTAssertEqual(sheet.evaluate(at: "D198"), .number(600.0))
    XCTAssertEqual(sheet.evaluate(at: "E198"), .number(78.0))

    XCTAssertEqual(sheet.evaluate(at: "D199"), .number(39_000.0))
    XCTAssertEqual(sheet.evaluate(at: "E199"), .text("핵심"))
    XCTAssertEqual(sheet.evaluate(at: "D200"), .number(800.0))
    XCTAssertEqual(sheet.evaluate(at: "E200"), .text("검산일치"))

    let unwrappedPivot = try XCTUnwrap(pivotSummary)
    XCTAssertEqual(unwrappedPivot.rowKeys, ["서울", "부산", "대전", "광주", "판교"])
    XCTAssertEqual(unwrappedPivot.rowTotals, [7_800.0, 15_600.0, 23_400.0, 31_200.0, 39_000.0])
    XCTAssertEqual(unwrappedPivot.grandTotal, 117_000.0)

    let unwrappedChart = try XCTUnwrap(chartModel)
    XCTAssertEqual(unwrappedChart.categories, ["서울", "부산", "대전", "광주", "판교"])
    XCTAssertEqual(unwrappedChart.series.first?.values, [200.0, 400.0, 600.0, 800.0, 1_000.0])
    XCTAssertLessThan(elapsed, .seconds(1))
  }

  // MARK: - 4. Batch Conversion Studio & Container Security Auditor Pipeline

  func testBatchConversionStudioAndSecurityAuditEndToEndPipeline() throws {
    let sourceDocument = OfficeDocumentModel(
      title: "보안 감사 및 일괄 변환 표준 기안문",
      format: .hwpx,
      paperSize: .a4,
      headerText: "문서번호: SEC-2026-900",
      footerText: "보안등급: 일반",
      paragraphs: [
        ParagraphBlock(
          style: .officialHeader, alignment: .center, text: "일 괄 변 환 검 증 기 안 문", fontSizePt: 18,
          isBold: true),
        ParagraphBlock(
          style: .heading1, alignment: .leading, text: "1. 다중 포맷 일괄 변환 파이프라인", fontSizePt: 14,
          isBold: true),
        ParagraphBlock(
          style: .body, alignment: .justified,
          text: "단일 원본 문서와 시트, 슬라이드를 10개 타깃 포맷으로 변환하고 컨테이너 보안을 전수 감사합니다."),
      ],
      tables: [
        DocumentTableBlock(
          caption: "검증 항목 표",
          rows: [
            ["항목", "결과"],
            ["매직 바이트", "통과"],
            ["XXE 방어", "통과"],
          ]
        )
      ]
    )

    var sourceSheet = WorksheetModel(name: "변환대상_실적시트")
    try sourceSheet.setCell("A1", rawValue: "지표")
    try sourceSheet.setCell("B1", rawValue: "값")
    try sourceSheet.setCell("A2", rawValue: "처리량")
    try sourceSheet.setCell("B2", rawValue: "480")
    try sourceSheet.setCell("A3", rawValue: "안정성")
    try sourceSheet.setCell("B3", rawValue: "=B2*2")

    let sourceDeck = PresentationDeckModel(
      title: "변환대상_발표자료",
      aspectRatio: .widescreen16x9,
      slides: [
        SlideModel(
          title: "보안 감사 통과",
          subtitle: "10개 포맷 일괄 변환",
          bulletPoints: ["ZIP/OLE2/PDF 시그니처 일치", "외부 엔티티(XXE) 제로"]
        )
      ]
    )

    let batchJobs: [ConversionJobRequest] = [
      ConversionJobRequest(
        sourceFileName: "기안문.hwpx", sourceFormat: .hwpx, targetFormat: .docx,
        payload: .document(sourceDocument)),
      ConversionJobRequest(
        sourceFileName: "기안문.docx", sourceFormat: .docx, targetFormat: .hwpx,
        payload: .document(sourceDocument)),
      ConversionJobRequest(
        sourceFileName: "기안문.hwpx", sourceFormat: .hwpx, targetFormat: .hwp,
        payload: .document(sourceDocument)),
      ConversionJobRequest(
        sourceFileName: "기안문.hwpx", sourceFormat: .hwpx, targetFormat: .odt,
        payload: .document(sourceDocument)),
      ConversionJobRequest(
        sourceFileName: "기안문.hwpx", sourceFormat: .hwpx, targetFormat: .rtf,
        payload: .document(sourceDocument)),
      ConversionJobRequest(
        sourceFileName: "기안문.hwpx", sourceFormat: .hwpx, targetFormat: .md,
        payload: .document(sourceDocument)),
      ConversionJobRequest(
        sourceFileName: "발표자료.pptx", sourceFormat: .pptx, targetFormat: .pdf,
        payload: .slide(sourceDeck)),
      ConversionJobRequest(
        sourceFileName: "실적시트.xlsx", sourceFormat: .xlsx, targetFormat: .xlsx,
        payload: .sheet(sourceSheet)),
      ConversionJobRequest(
        sourceFileName: "실적시트.xlsx", sourceFormat: .xlsx, targetFormat: .ods,
        payload: .sheet(sourceSheet)),
      ConversionJobRequest(
        sourceFileName: "실적시트.xlsx", sourceFormat: .xlsx, targetFormat: .csv,
        payload: .sheet(sourceSheet)),
    ]

    let clock = ContinuousClock()
    var batchReport: BatchConversionReport?
    let batchElapsed = clock.measure {
      batchReport = DocumentConversionOrchestrator.executeBatch(batchJobs)
    }
    XCTAssertLessThan(batchElapsed, .seconds(1))

    let unwrappedBatchReport = try XCTUnwrap(batchReport)
    XCTAssertEqual(unwrappedBatchReport.totalJobs, 10)
    XCTAssertEqual(unwrappedBatchReport.succeededResults.count, 10)
    XCTAssertTrue(unwrappedBatchReport.failedErrors.isEmpty)

    let expectedTargets: [DocumentFormat] = [
      .docx, .hwpx, .hwp, .odt, .rtf, .md, .pdf, .xlsx, .ods, .csv,
    ]
    XCTAssertEqual(unwrappedBatchReport.succeededResults.map(\.targetFormat), expectedTargets)

    for result in unwrappedBatchReport.succeededResults {
      XCTAssertFalse(result.outputBytes.isEmpty)
      let audit = DocumentSecurityAuditor.auditContainer(
        data: result.outputBytes,
        fileName: result.outputFileName
      )
      XCTAssertTrue(audit.isPassed, "Audit failed for \(result.outputFileName): \(audit.issues)")
      XCTAssertTrue(audit.isSignatureValid)
      XCTAssertTrue(audit.isSafeFromBombAndTraversal)
      XCTAssertTrue(audit.isSafeFromXXE)
      XCTAssertTrue(audit.isUnencrypted)
      XCTAssertEqual(audit.detectedFormat, result.targetFormat)
      XCTAssertTrue(audit.issues.isEmpty)
    }
  }

  // MARK: - 5. 100% Offline Smart Assistant & Metadata Scrubber Integration

  func testOfflineSmartAssistantAndMetadataScrubberIntegration() throws {
    let pHeading = ParagraphBlock(
      style: .heading1,
      text: "2026년 10월 31일까지 차세대 문서 보안 엔진 마일스톤 마감 및 필수 조치 계획"
    )
    let pLongAndPassive = ParagraphBlock(
      style: .body,
      text:
        "본 보안 아키텍처 개편안은 총 예산 450,000,000원 규모로 편성되어지며, 모든 컨테이너 파서의 처리 성능을 12.5% 향상시키고, 매우 매우 엄격한 검증 게이트를 적용하여 다시 재검토 과정을 거치도록 설계되어집니다."
    )
    let pAction = ParagraphBlock(
      style: .body,
      text: "보안팀 확인 필요: 전 모듈 매직 바이트 검사를 2026-10-25 기한 내 완료하고 결과 보고서 제출 바람."
    )

    let baselineDoc = OfficeDocumentModel(
      title: "스마트 어시스턴트 및 개인정보 세정 검증 문서",
      format: .docx,
      headerText: "작성자: 홍길동 (사번 2026001)",
      footerText: "내부 대외비 · 직통 02-555-0199",
      paragraphs: [pHeading, pLongAndPassive, pAction],
      trackChangesEnabled: true,
      revisions: [
        TrackedRevision(
          author: "김보안 팀장",
          kind: .insertion,
          paragraphID: pAction.id,
          originalText: "결과 보고서",
          proposedText: "보안 감사 결과 보고서"
        )
      ],
      comments: [
        DocumentCommentThread(
          anchorParagraphID: pLongAndPassive.id,
          author: "박감사 위원",
          message: "예산 금액 공개 범위를 배포 전 확인하세요.",
          replies: ["확인 완료했습니다."]
        )
      ]
    )

    // 1. Extractive summary
    let summary = LocalSmartDocumentAssistant.summarize(baselineDoc, maxSentences: 2)
    XCTAssertEqual(summary.keySentences.count, 2)
    XCTAssertFalse(summary.keywords.isEmpty)
    XCTAssertGreaterThan(summary.estimatedReadingSeconds, 0)
    XCTAssertGreaterThan(summary.compressionRatioPercent, 0.0)

    // 2. Writing quality diagnostics (long sentence, passive voice, redundant modifier)
    let suggestions = LocalSmartDocumentAssistant.analyzeWritingQuality(baselineDoc)
    let issueKinds = Set(suggestions.map(\.issueKind))
    XCTAssertTrue(issueKinds.contains("장문 분할 권장"))
    XCTAssertTrue(issueKinds.contains("수동태/피동 표현 완화"))
    XCTAssertTrue(issueKinds.contains("중복 공백/수식어 정리"))

    // 3. Action item and entity extraction
    let entities = LocalSmartDocumentAssistant.extractActionItemsAndEntities(baselineDoc)
    let categories = Set(entities.map(\.category))
    XCTAssertTrue(categories.contains("일정/기한"))
    XCTAssertTrue(categories.contains("예산/금액"))
    XCTAssertTrue(categories.contains("핵심 수치"))
    XCTAssertTrue(categories.contains("조치 항목"))

    // 4. Worksheet insights (Z-Score outlier detection & trend classification)
    var metricSheet = WorksheetModel(name: "성능지표_이상치탐지")
    let normalSamples = [100, 102, 101, 103, 100, 102, 101, 104, 103]
    for (idx, val) in normalSamples.enumerated() {
      try metricSheet.setCell("A\(idx + 1)", rawValue: "\(val)")
    }
    try metricSheet.setCell("A10", rawValue: "650")  // Extreme outlier spike (Z-score > 1.8)

    let insight = LocalSmartDocumentAssistant.analyzeWorksheetInsights(
      metricSheet,
      range: try CellRange(notation: "A1:A10")
    )
    XCTAssertEqual(insight.numericCellCount, 10)
    XCTAssertEqual(insight.minValue, 100.0)
    XCTAssertEqual(insight.maxValue, 650.0)
    XCTAssertTrue(insight.outlierAddresses.contains("A10"))
    XCTAssertEqual(insight.trendClassification, "상승 추세")
    XCTAssertGreaterThan(insight.growthRatePercent, 500.0)

    // 5. LCS Document comparison
    var revisedDoc = baselineDoc
    revisedDoc.paragraphs[1].text = "본 보안 아키텍처 개편안은 총 예산 450,000,000원 규모로 편성됩니다."
    revisedDoc.paragraphs.append(ParagraphBlock(style: .body, text: "신규 추가된 배포 승인 절차 안내 문단입니다."))

    let diffReport = LocalSmartDocumentAssistant.compareDocuments(
      baseline: baselineDoc,
      revised: revisedDoc
    )
    XCTAssertEqual(diffReport.unchangedCount, 2)
    XCTAssertEqual(diffReport.modifiedCount, 1)
    XCTAssertEqual(diffReport.addedCount, 1)
    XCTAssertEqual(diffReport.removedCount, 0)

    // 6. Metadata & PII scrubber
    let (scrubbedDoc, removedCount) = DocumentSecurityAuditor.scrubMetadata(from: baselineDoc)
    XCTAssertEqual(removedCount, 4)  // header + footer + 1 revision author + 1 comment thread
    XCTAssertTrue(scrubbedDoc.headerText.isEmpty)
    XCTAssertTrue(scrubbedDoc.footerText.isEmpty)
    XCTAssertTrue(scrubbedDoc.comments.isEmpty)
    XCTAssertEqual(scrubbedDoc.revisions.map(\.author), ["Redacted"])
    XCTAssertEqual(scrubbedDoc.paragraphs.map(\.text), baselineDoc.paragraphs.map(\.text))
  }

  // MARK: - 6. MainActor Workspace End-to-End User Journey

  @MainActor
  func testMainActorWorkspaceEndToEndUserJourney() throws {
    let workspace = WorkspaceViewModel()

    // 1. Verify initial 5 module sample tabs & Home Hub templates
    XCTAssertEqual(workspace.openTabs.count, 5)
    XCTAssertEqual(Set(workspace.openTabs.map(\.moduleKind)), Set(OfficeModuleKind.allCases))
    XCTAssertEqual(WorkspaceHubCatalog.builtInTemplates.count, 8)

    workspace.activeHubSection = .pinned
    XCTAssertEqual(workspace.filteredTabs.count, 3)
    XCTAssertTrue(workspace.filteredTabs.allSatisfy { $0.isPinned || $0.isFavorite })

    workspace.activeHubSection = .recent
    XCTAssertEqual(workspace.filteredTabs.count, 5)
    XCTAssertEqual(workspace.filteredTabs.first?.lastOpenedISO8601, "2026-10-17T09:30:00Z")

    for section in HubSection.allCases {
      workspace.activeHubSection = section
      XCTAssertFalse(section.symbolName.isEmpty)
      XCTAssertFalse(workspace.filteredTabs.isEmpty)
    }
    workspace.activeHubSection = .home

    // 2. Enable Split View between primary Document tab and secondary Sheet tab
    let docTab = try XCTUnwrap(workspace.openTabs.first(where: { $0.moduleKind == .document }))
    let sheetTab = try XCTUnwrap(workspace.openTabs.first(where: { $0.moduleKind == .sheet }))
    let pdfTab = try XCTUnwrap(workspace.openTabs.first(where: { $0.moduleKind == .pdf }))

    workspace.activeTabID = docTab.id
    workspace.enableSplitView(secondaryTabID: sheetTab.id)
    XCTAssertEqual(workspace.splitSecondaryTab?.id, sheetTab.id)

    // 3. Toggle Ribbon collapse & verify context ribbon commands across all categories and modules
    XCTAssertFalse(workspace.isRibbonCollapsed)
    workspace.isRibbonCollapsed.toggle()
    XCTAssertTrue(workspace.isRibbonCollapsed)
    workspace.isRibbonCollapsed.toggle()
    XCTAssertFalse(workspace.isRibbonCollapsed)

    for module in OfficeModuleKind.allCases {
      for category in RibbonCategory.allCases {
        let actions = RibbonCategory.actions(for: category, module: module)
        XCTAssertEqual(actions.count, 4)
      }
    }

    // 4. Switch across all WorkspaceThemeMode cases & verify WCAG 2.1 contrast
    for mode in WorkspaceThemeMode.allCases {
      workspace.themeMode = mode
      let lightPalette = mode.surfacePalette(isSystemDark: false)
      let darkPalette = mode.surfacePalette(isSystemDark: true)
      XCTAssertGreaterThanOrEqual(
        lightPalette.primaryText.contrastRatio(against: lightPalette.background), 4.5)
      XCTAssertGreaterThanOrEqual(
        darkPalette.primaryText.contrastRatio(against: darkPalette.background), 4.5)
    }

    // 5. Append paragraph with Track Changes on active Document tab and persist to workspace.openTabs
    workspace.activeTabID = docTab.id
    workspace.appendParagraphToActiveDocument(
      text: "워크스페이스 통합 시나리오에서 추가된 결론 문단입니다.",
      style: .heading2
    )
    let docIndex = try XCTUnwrap(workspace.openTabs.firstIndex(where: { $0.id == docTab.id }))
    var acceptedRevID: UUID?
    if case .document(var updatedDoc) = workspace.openTabs[docIndex].payload {
      XCTAssertEqual(updatedDoc.paragraphs.last?.text, "워크스페이스 통합 시나리오에서 추가된 결론 문단입니다.")
      let lastID = try XCTUnwrap(updatedDoc.paragraphs.last?.id)
      updatedDoc.recordRevision(
        author: "QA",
        kind: .formattingChange,
        paragraphID: lastID,
        proposedText: "개정된 최종 결론 문단입니다."
      )
      let revID = try XCTUnwrap(updatedDoc.revisions.last?.id)
      acceptedRevID = revID
      updatedDoc.acceptRevision(id: revID)
      workspace.openTabs[docIndex].payload = .document(updatedDoc)
    } else {
      XCTFail("Expected active tab to hold a document payload")
    }
    if case .document(let persistedDoc) = workspace.activeTab?.payload {
      XCTAssertEqual(persistedDoc.paragraphs.last?.text, "개정된 최종 결론 문단입니다.")
      XCTAssertFalse(persistedDoc.revisions.contains(where: { $0.id == acceptedRevID }))
    } else {
      XCTFail("Expected persisted document payload on activeTab")
    }

    // 6. Edit spreadsheet formula via formulaBarDraft on Sheet tab
    workspace.activeTabID = sheetTab.id
    workspace.selectedSheetCell = "E4"
    workspace.formulaBarDraft = "=SUM(D2:D3)*2"
    workspace.applyFormulaBarEdit()

    if case .sheet(let updatedSheet) = workspace.activeTab?.payload {
      XCTAssertEqual(updatedSheet.rawValue(at: "E4"), "=SUM(D2:D3)*2")
      XCTAssertEqual(updatedSheet.evaluate(at: "E4"), .number(1080.0))
    } else {
      XCTFail("Expected active tab to hold a sheet payload")
    }

    // 7. Rotate and annotate PDF page on PDF Studio tab
    workspace.activeTabID = pdfTab.id
    workspace.selectedPDFPageIndex = 0
    workspace.rotateActivePDFPage(clockwiseDegrees: 90)
    workspace.addPDFAnnotation(kind: .signature, content: "QA 최종 승인 서명")

    if case .pdf(let updatedPDF) = workspace.activeTab?.payload {
      XCTAssertEqual(updatedPDF.pages[0].rotationDegrees, 90)
      XCTAssertEqual(updatedPDF.annotations.last?.kind, .signature)
      XCTAssertEqual(updatedPDF.annotations.last?.content, "QA 최종 승인 서명")
    } else {
      XCTFail("Expected active tab to hold a PDF payload")
    }

    // 8. Save custom user template and verify template catalog
    workspace.saveActiveTabAsCustomTemplate(
      title: "QA 표준 PDF 보안 검토 템플릿",
      subtitle: "사용자 정의 PDF 주석·회전 템플릿"
    )
    XCTAssertEqual(workspace.customTemplates.count, 1)
    XCTAssertTrue(workspace.customTemplates[0].isCustomUserTemplate)
    XCTAssertEqual(workspace.allAvailableTemplates.count, 9)
  }
}
