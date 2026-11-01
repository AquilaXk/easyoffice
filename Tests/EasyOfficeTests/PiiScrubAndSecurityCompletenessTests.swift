import Foundation
import XCTest

@testable import EasyOfficeCore

final class PiiScrubAndSecurityCompletenessTests: XCTestCase {

  private struct SharedPiiVector: Decodable {
    let id: String
    let input: String
    let expectedCount: Int
    let forbiddenSubstrings: [String]
    let expectedRustJsMasked: String
    let expectedSwiftMasked: String
  }

  func testSharedPiiVectorsMatchSwiftEngine() throws {
    let repoRoot = URL(fileURLWithPath: #filePath)
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
    let vectorsURL = repoRoot.appendingPathComponent("fixtures/pii-scrub-vectors.json")
    let rawData = try Data(contentsOf: vectorsURL)
    let vectors = try JSONDecoder().decode([SharedPiiVector].self, from: rawData)
    XCTAssertGreaterThanOrEqual(vectors.count, 12)

    for vec in vectors {
      let (masked, count) = DocumentSecurityAuditor.maskSensitivePatterns(in: vec.input)
      XCTAssertEqual(count, vec.expectedCount, "Vector count mismatch for \(vec.id)")
      XCTAssertEqual(masked, vec.expectedSwiftMasked, "Vector masked output mismatch for \(vec.id)")
      for forbidden in vec.forbiddenSubstrings {
        XCTAssertFalse(
          masked.contains(forbidden),
          "Forbidden substring \(forbidden) leaked in \(vec.id): \(masked)"
        )
      }
    }
  }

  func testPdfRedactTextScrubsAnnotationAuthorFormFieldNameAndInkStrokeLabel() {
    var pdf = PDFStudioDocumentModel(
      title: "계약서_홍길동_원본",
      pages: [PDFPageDescriptor(pageNumber: 1, extractedText: "갑: 홍길동 / 을: 이지오피스")],
      annotations: [
        PDFAnnotationItem(
          pageNumber: 1, kind: .stickyNote, content: "홍길동 서명 확인", author: "검토자 홍길동")
      ],
      outline: [
        PDFOutlineNode(
          title: "1장 홍길동 개요",
          targetPageNumber: 1,
          children: [PDFOutlineNode(title: "1.1 홍길동 세부조항", targetPageNumber: 1)]
        )
      ],
      inkStrokes: [
        PDFInkStroke(
          pageNumber: 1,
          points: [PDFInkPoint(x: 10, y: 20), PDFInkPoint(x: 30, y: 40)],
          label: "홍길동 자필서명"
        )
      ],
      formFields: [
        PDFFormField(pageNumber: 1, name: "홍길동_서명필드", value: "홍길동 승인")
      ],
      watermarkText: "대외비 홍길동 열람용"
    )

    let modifiedPages = pdf.redactText("홍길동")
    XCTAssertEqual(modifiedPages, 1)
    let leaves = Self.collectGraphLeaves(pdf, path: "pdf")
    XCTAssertGreaterThan(leaves.strings.count, 8)
    for item in leaves.strings {
      XCTAssertFalse(
        item.value.contains("홍길동"),
        "Unredacted target leaked at \(item.path): \(item.value)"
      )
    }
  }

  func testMirrorStructuralCompletenessAcrossAllFourDomainModelsAndBinaryImages() throws {
    let canaries = ["011-987-6543", "홍길동@회사.kr", "900101-5123456", "Exif"]
    let jpegWithExif = Self.buildSampleJpegWithExifAndCom()
    let pngWithExif = Self.buildSamplePngWithTextAndExif()
    let tiffWithExif = Self.buildSampleTiffWithExifAndArtist()

    let para = ParagraphBlock(
      text: "본문 연락처 011-987-6543 및 메일 홍길동@회사.kr",
      runs: [
        DocumentTextRun(
          text: "본문 연락처 011-987-6543 및 메일 홍길동@회사.kr",
          fontName: "Font_011-987-6543",
          linkURL: "mailto:홍길동@회사.kr"
        )
      ],
      fontName: "BaseFont_900101-5123456"
    )
    _ = para.id

    var doc = OfficeDocumentModel(
      title: "기안문_011-987-6543",
      headerText: "머리글 홍길동@회사.kr",
      footerText: "바닥글 900101-5123456",
      paragraphs: [para],
      tables: [
        DocumentTableBlock(
          caption: "표캡션 홍길동@회사.kr",
          rows: [["항목 011-987-6543", "번호 900101-5123456"]]
        )
      ],
      footnotes: [
        FootnoteItem(number: 1, anchorParagraphID: para.id, text: "각주 홍길동@회사.kr")
      ],
      revisions: [
        TrackedRevision(
          author: "작성자 011-987-6543",
          kind: .insertion,
          paragraphID: para.id,
          originalText: "이전 900101-5123456",
          proposedText: "변경 홍길동@회사.kr"
        )
      ],
      comments: [
        DocumentCommentThread(
          anchorParagraphID: para.id,
          author: "리뷰어 홍길동@회사.kr",
          message: "코멘트 011-987-6543",
          replies: ["답글 900101-5123456"]
        )
      ],
      bookmarks: ["북마크_011-987-6543": para.id],
      images: [
        EmbeddedImageAttachment(
          fileName: "스캔본_900101-5123456.jpg",
          caption: "사진캡션 홍길동@회사.kr",
          mimeType: "image/jpeg",
          imageData: jpegWithExif
        ),
        EmbeddedImageAttachment(
          fileName: "도표_011-987-6543.png",
          caption: "PNG캡션 900101-5123456",
          mimeType: "image/png",
          imageData: pngWithExif
        ),
        EmbeddedImageAttachment(
          fileName: "원본_홍길동@회사.kr.tiff",
          caption: "TIFF캡션 011-987-6543",
          mimeType: "image/tiff",
          imageData: tiffWithExif
        ),
      ],
      equations: [
        DocumentEquationBlock(
          displayTitle: "수식제목 011-987-6543",
          rawExpression: "x + y = 900101-5123456"
        )
      ],
      watermarkText: "워터마크 홍길동@회사.kr"
    )
    doc.preservedArchiveEntries = [
      "word/media/embed_011-987-6543.jpg": jpegWithExif,
      "word/media/embed_2.png": pngWithExif,
      "word/comments.xml": Data("<w:comments>홍길동@회사.kr</w:comments>".utf8),
      "word/document.xml": Data("<w:t>보존본 900101-5123456</w:t>".utf8),
    ]

    let (scrubbedDocPayload, docRemoved) = DocumentSecurityAuditor.scrubPayload(.document(doc))
    XCTAssertGreaterThan(docRemoved, 15)
    Self.assertZeroCanaryLeaks(in: scrubbedDocPayload, label: "document", canaries: canaries)
    if case .document(let cleanedDoc) = scrubbedDocPayload {
      let cleanedJpeg = cleanedDoc.images[0].imageData
      XCTAssertTrue(cleanedJpeg.starts(with: Data([0xFF, 0xD8, 0xFF, 0xE0])))
      XCTAssertEqual(Array(cleanedJpeg.suffix(2)), [0xFF, 0xD9])
      let cleanedPng = cleanedDoc.images[1].imageData
      XCTAssertTrue(cleanedPng.starts(with: Data([0x89, 0x50, 0x4E, 0x47])))
      XCTAssertNotNil(String(decoding: cleanedPng, as: UTF8.self).range(of: "IDAT"))
    } else {
      XCTFail("Expected document payload")
    }

    let cA1 = try CellAddress(a1Notation: "A1").packedCoordinate
    let cB1 = try CellAddress(a1Notation: "B1").packedCoordinate
    var sheet = WorksheetModel(
      name: "시트_011-987-6543",
      rawCells: [
        "A1": "연락처 011-987-6543",
        "B1": "=\"문의 홍길동@회사.kr\"",
      ],
      pageSetup: WorksheetPageSetup(marginPresetName: "여백_900101-5123456"),
      sheetComments: [cA1: "시트메모 홍길동@회사.kr"],
      additionalSheets: [
        WorksheetSheetPage(
          name: "보조시트_900101-5123456",
          cellsByCoordinate: [
            cA1: "외국인번호 900101-5123456",
            cB1: "=\"비상망 011-987-6543\"",
          ],
          sheetComments: [cB1: "보조메모 011-987-6543"],
          conditionalFormatRules: [.containsText("홍길동@회사.kr")],
          dataValidationRule: .allowedList(["900101-5123456", "승인"])
        )
      ],
      namedRanges: ["범위_011-987-6543": "설명 홍길동@회사.kr"],
      conditionalFormatRules: [.containsText("011-987-6543")],
      dataValidationRule: .allowedList(["홍길동@회사.kr", "일반"])
    )
    sheet.preservedArchiveEntries = ["xl/media/chart_011-987-6543.png": pngWithExif]

    let (scrubbedSheetPayload, sheetRemoved) = DocumentSecurityAuditor.scrubPayload(.sheet(sheet))
    XCTAssertGreaterThan(sheetRemoved, 8)
    Self.assertZeroCanaryLeaks(in: scrubbedSheetPayload, label: "sheet", canaries: canaries)

    var deck = PresentationDeckModel(
      title: "발표_011-987-6543",
      slides: [
        SlideModel(
          title: "슬라이드제목 홍길동@회사.kr",
          subtitle: "부제 900101-5123456",
          bulletPoints: ["항목1 011-987-6543", "항목2 홍길동@회사.kr"],
          speakerNotes: "발표자노트 900101-5123456",
          elements: [
            SlideCanvasElement(
              title: "캔버스요소제목 011-987-6543",
              bodyText: "캔버스본문 홍길동@회사.kr",
              x: 40,
              y: 80,
              width: 300,
              height: 120
            )
          ]
        )
      ]
    )
    deck.preservedArchiveEntries = ["ppt/media/bg.jpg": jpegWithExif]

    let (scrubbedSlidePayload, slideRemoved) = DocumentSecurityAuditor.scrubPayload(.slide(deck))
    XCTAssertGreaterThan(slideRemoved, 6)
    Self.assertZeroCanaryLeaks(in: scrubbedSlidePayload, label: "slide", canaries: canaries)

    let pdf = PDFStudioDocumentModel(
      title: "PDF제목 011-987-6543",
      pages: [PDFPageDescriptor(pageNumber: 1, extractedText: "본문 홍길동@회사.kr / 900101-5123456")],
      annotations: [
        PDFAnnotationItem(
          pageNumber: 1,
          kind: .highlight,
          content: "주석본문 011-987-6543",
          author: "주석작성자 홍길동@회사.kr"
        )
      ],
      outline: [
        PDFOutlineNode(
          title: "목차1 011-987-6543",
          targetPageNumber: 1,
          children: [PDFOutlineNode(title: "하위목차 900101-5123456", targetPageNumber: 1)]
        )
      ],
      inkStrokes: [
        PDFInkStroke(
          pageNumber: 1,
          points: [PDFInkPoint(x: 1, y: 2)],
          label: "서명라벨 홍길동@회사.kr"
        )
      ],
      formFields: [
        PDFFormField(pageNumber: 1, name: "필드명_011-987-6543", value: "필드값_900101-5123456")
      ],
      redactedPhrases: ["이전구문 홍길동@회사.kr"],
      watermarkText: "PDF워터마크 011-987-6543"
    )

    let (scrubbedPdfPayload, pdfRemoved) = DocumentSecurityAuditor.scrubPayload(.pdf(pdf))
    XCTAssertGreaterThan(pdfRemoved, 8)
    Self.assertZeroCanaryLeaks(in: scrubbedPdfPayload, label: "pdf", canaries: canaries)
  }

  private static func assertZeroCanaryLeaks(
    in root: Any, label: String, canaries: [String], file: StaticString = #filePath, line: UInt = #line
  ) {
    let leaves = collectGraphLeaves(root, path: label)
    XCTAssertGreaterThan(leaves.strings.count, 0, file: file, line: line)
    for sLeaf in leaves.strings {
      for canary in canaries {
        XCTAssertFalse(
          sLeaf.value.contains(canary),
          "Canary '\(canary)' leaked in String at \(sLeaf.path): \(sLeaf.value)",
          file: file,
          line: line
        )
      }
    }
    for dLeaf in leaves.datas {
      for canary in canaries {
        let needle = Data(canary.utf8)
        XCTAssertNil(
          dLeaf.value.range(of: needle),
          "Canary '\(canary)' leaked in Data (\(dLeaf.value.count)B) at \(dLeaf.path)",
          file: file,
          line: line
        )
      }
    }
  }

  private static func collectGraphLeaves(
    _ value: Any, path: String
  ) -> (strings: [(path: String, value: String)], datas: [(path: String, value: Data)]) {
    if let str = value as? String {
      return ([(path, str)], [])
    }
    if let data = value as? Data {
      return ([], [(path, data)])
    }
    let mirror = Mirror(reflecting: value)
    if mirror.displayStyle == .optional {
      guard let child = mirror.children.first else { return ([], []) }
      return collectGraphLeaves(child.value, path: path)
    }
    var strings: [(String, String)] = []
    var datas: [(String, Data)] = []
    for (idx, child) in mirror.children.enumerated() {
      let step = child.label ?? "[\(idx)]"
      let sub = collectGraphLeaves(child.value, path: "\(path).\(step)")
      strings.append(contentsOf: sub.strings)
      datas.append(contentsOf: sub.datas)
    }
    return (strings, datas)
  }

  private static func buildSampleJpegWithExifAndCom() -> Data {
    var bytes: [UInt8] = [0xFF, 0xD8]
    let jfifPayload: [UInt8] = [0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00]
    let app0Len = UInt16(jfifPayload.count + 2)
    bytes.append(contentsOf: [0xFF, 0xE0, UInt8(app0Len >> 8), UInt8(app0Len & 0xFF)])
    bytes.append(contentsOf: jfifPayload)

    let exifPayload = Array("Exif\0\0홍길동@회사.kr 011-987-6543".utf8)
    let app1Len = UInt16(exifPayload.count + 2)
    bytes.append(contentsOf: [0xFF, 0xE1, UInt8(app1Len >> 8), UInt8(app1Len & 0xFF)])
    bytes.append(contentsOf: exifPayload)

    let comPayload = Array("Comment 900101-5123456".utf8)
    let comLen = UInt16(comPayload.count + 2)
    bytes.append(contentsOf: [0xFF, 0xFE, UInt8(comLen >> 8), UInt8(comLen & 0xFF)])
    bytes.append(contentsOf: comPayload)

    let sosHeader: [UInt8] = [0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00]
    bytes.append(contentsOf: [0xFF, 0xDA])
    bytes.append(contentsOf: sosHeader)
    bytes.append(contentsOf: [0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD9])
    return Data(bytes)
  }

  private static func buildSamplePngWithTextAndExif() -> Data {
    var out: [UInt8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
    func appendChunk(_ type: String, _ payload: [UInt8]) {
      let len = UInt32(payload.count)
      out.append(contentsOf: [
        UInt8((len >> 24) & 0xFF), UInt8((len >> 16) & 0xFF),
        UInt8((len >> 8) & 0xFF), UInt8(len & 0xFF),
      ])
      out.append(contentsOf: type.utf8)
      out.append(contentsOf: payload)
      out.append(contentsOf: [0x00, 0x00, 0x00, 0x00])
    }
    appendChunk("IHDR", [0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0])
    appendChunk("tEXt", Array("Author\0홍길동@회사.kr".utf8))
    appendChunk("eXIf", Array("Exif\0\0011-987-6543".utf8))
    appendChunk("IDAT", [0x78, 0x9C, 0x63, 0x60, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01])
    appendChunk("IEND", [])
    return Data(out)
  }

  private static func buildSampleTiffWithExifAndArtist() -> Data {
    var buf = [UInt8](repeating: 0, count: 160)
    buf[0] = 0x49
    buf[1] = 0x49
    buf[2] = 0x2A
    buf[3] = 0x00
    buf[4] = 8

    func writeLE16(_ at: Int, _ v: UInt16) {
      buf[at] = UInt8(v & 0xFF)
      buf[at + 1] = UInt8(v >> 8)
    }
    func writeLE32(_ at: Int, _ v: UInt32) {
      buf[at] = UInt8(v & 0xFF)
      buf[at + 1] = UInt8((v >> 8) & 0xFF)
      buf[at + 2] = UInt8((v >> 16) & 0xFF)
      buf[at + 3] = UInt8((v >> 24) & 0xFF)
    }

    writeLE16(8, 3)
    writeLE16(10, 0x0100)
    writeLE16(12, 4)
    writeLE32(14, 1)
    writeLE32(18, 64)

    let artistBytes = Array("홍길동@회사.kr\0".utf8)
    writeLE16(22, 0x013B)
    writeLE16(24, 2)
    writeLE32(26, UInt32(artistBytes.count))
    writeLE32(30, 60)
    buf.replaceSubrange(60..<(60 + artistBytes.count), with: artistBytes)

    writeLE16(34, 0x8769)
    writeLE16(36, 4)
    writeLE32(38, 1)
    writeLE32(42, 96)
    writeLE32(46, 0)

    let subPayload = Array("Exif_900101-5123456\0".utf8)
    writeLE16(96, 1)
    writeLE16(98, 0x9286)
    writeLE16(100, 2)
    writeLE32(102, UInt32(subPayload.count))
    writeLE32(106, 120)
    writeLE32(110, 0)
    buf.replaceSubrange(120..<(120 + subPayload.count), with: subPayload)

    return Data(buf)
  }
}
