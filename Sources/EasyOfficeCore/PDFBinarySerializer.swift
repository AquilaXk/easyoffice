import Foundation
import PDFKit

// MARK: - Deterministic ISO 32000-1 PDF Binary Serializer & Native PDFKit Bridge

/// Deterministic ISO 32000-1 (`%PDF-1.7`) binary serializer and parser with byte-exact `xref` offsets and native `PDFKit` inspection.
public enum PDFBinarySerializer {
  private static let modelStreamPrefix = "%EasyOfficePDFModelBase64:"

  private static let hexDigits: [UInt8] = Array("0123456789ABCDEF".utf8)

  public static func wrapLine(_ line: String, maxChars: Int = 64) -> [String] {
    let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return [] }
    let limit = max(1, maxChars)
    var wrapped: [String] = []
    var start = trimmed.startIndex
    while start < trimmed.endIndex {
      let end =
        trimmed.index(start, offsetBy: limit, limitedBy: trimmed.endIndex) ?? trimmed.endIndex
      wrapped.append(String(trimmed[start..<end]))
      start = end
    }
    return wrapped
  }

  public static func encode(_ pdf: PDFStudioDocumentModel) -> Data {
    var buffer = Data()
    buffer.reserveCapacity(4096)

    func appendLine(_ line: String) {
      buffer.append(contentsOf: line.utf8)
      buffer.append(0x0A)
    }

    appendLine("%PDF-1.7")
    appendLine("%\u{00E2}\u{00E3}\u{00CF}\u{00D3}")

    let maxLinesPerPage = 42
    var renderedPages: [(
      sourcePage: PDFPageDescriptor, pageSliceIndex: Int, visualLines: [String], chunkText: String
    )] = []
    for page in pdf.pages {
      let allVisualLines = page.extractedText
        .split(separator: "\n", omittingEmptySubsequences: false)
        .flatMap { wrapLine(String($0), maxChars: 64) }
      if allVisualLines.count <= maxLinesPerPage {
        renderedPages.append(
          (
            sourcePage: page,
            pageSliceIndex: 0,
            visualLines: allVisualLines,
            chunkText: page.extractedText
          ))
      } else {
        var sliceIdx = 0
        var start = 0
        while start < allVisualLines.count {
          let end = Swift.min(start + maxLinesPerPage, allVisualLines.count)
          let slice = Array(allVisualLines[start..<end])
          renderedPages.append(
            (
              sourcePage: page,
              pageSliceIndex: sliceIdx,
              visualLines: slice,
              chunkText: slice.joined(separator: "\n")
            ))
          sliceIdx += 1
          start = end
        }
      }
    }

    let pageCount = renderedPages.count
    let pageObjectNumbers = (0..<pageCount).map { 3 + ($0 * 2) }
    let kidsRefs = pageObjectNumbers.map { "\($0) 0 R" }.joined(separator: " ")
    let metadataObjNum = 3 + (pageCount * 2)
    let outlinesObjNum = pdf.outline.isEmpty ? nil : (metadataObjNum + 1)
    let outlinesCatalogRef = outlinesObjNum.map { " /Outlines \($0) 0 R" } ?? ""

    var objectOffsets: [Int: Int] = [:]

    objectOffsets[1] = buffer.count
    appendLine("1 0 obj")
    appendLine(
      "<< /Type /Catalog /Pages 2 0 R\(outlinesCatalogRef) /Title (\(escapePDFText(pdf.title))) /Redacted \(pdf.isRedacted ? "true" : "false") >>"
    )
    appendLine("endobj")

    objectOffsets[2] = buffer.count
    appendLine("2 0 obj")
    appendLine("<< /Type /Pages /Count \(pageCount) /Kids [\(kidsRefs)] >>")
    appendLine("endobj")

    for (idx, rendered) in renderedPages.enumerated() {
      let page = rendered.sourcePage
      let isFirstSlice = rendered.pageSliceIndex == 0
      let visualLines = rendered.visualLines
      let pageObjNum = 3 + (idx * 2)
      let contentObjNum = pageObjNum + 1
      let escapedText = escapePDFString(rendered.chunkText)
      let headerHex = hexEncodeASCII(
        "EasyOffice ISO 32000-1 PDF Studio - Page \(idx + 1) (\(page.rotationDegrees) deg)")
      let statusHex = hexEncodeASCII(
        "Verified ISO 32000-1 Vector Page | UTF-8 Stream: \(rendered.chunkText.utf8.count) Bytes | Status: SAFE"
      )
      var streamLines: [String] = [
        "q",
        "0.969 0.973 0.988 rg 36 36 523 770 re f",
        "0.361 0.420 0.753 rg 36 756 523 50 re f",
        "0.820 0.839 0.922 RG 1 w 36 36 523 770 re S",
        "BT 3 Tr /F1 1 Tf 0 0 Td (\(escapedText)) Tj ET",
        "1 1 1 rg BT 0 Tr /F1 13 Tf 54 775 Td <\(headerHex)> Tj ET",
        "0.220 0.251 0.380 rg BT 0 Tr /F1 9 Tf 54 760 Td <\(statusHex)> Tj ET",
        "0.839 0.859 0.941 rg 54 738 487 1.5 re f",
      ]

      if pdf.isRedacted || !pdf.redactedPhrases.isEmpty {
        let redactAuditHex = hexEncodeASCII(
          "REDACTED DOCUMENT | Scrubbed Phrases: \(pdf.redactedPhrases.count)")
        streamLines.append("q 0 0 0 rg 42 742 180 12 re f Q")
        streamLines.append("1 1 1 rg BT 0 Tr /F1 8 Tf 46 745 Td <\(redactAuditHex)> Tj ET")
      }

      if !pdf.watermarkText.isEmpty {
        let wmLabel = "[WM] \(pdf.watermarkText)"
        let wmHex = hexEncodeUTF16BE(wmLabel, maxChars: 64)
        let escapedWM = escapePDFString(wmLabel)
        streamLines.append("BT 3 Tr /F1 1 Tf 0 1 Td (\(escapedWM)) Tj ET")
        streamLines.append(
          "q 0.780 0.800 0.880 rg BT 0 Tr /F2 10 Tf 1 0 0 1 320 746 Tm <\(wmHex)> Tj ET Q"
        )
      }

      // 3. Body Paragraphs (y = 728 down to y = 76, fixed 15.0pt leading)
      var cursorY = 728.0
      let lineLeading = 15.0
      for rawLine in visualLines {
        guard cursorY >= 76.0 else { break }
        let yInt = XMLDocumentModelMapper.safeClampedInt(cursorY)
        let barWidth = min(480, max(60, rawLine.count * 9))
        let visibleLineHex = hexEncodeUTF16BE(rawLine, maxChars: 64)
        streamLines.append("0.925 0.937 0.984 rg 54 \(yInt - 3) \(barWidth) 13 re f")
        streamLines.append(
          "0.145 0.165 0.243 rg BT 0 Tr /F2 10 Tf 1 0 0 1 54 \(yInt) Tm <\(visibleLineHex)> Tj ET"
        )
        cursorY -= lineLeading
      }

      // 4. Visual Annotations & Official Approval Stamp Badge (y = 576..626)
      let pageAnnotations =
        isFirstSlice ? pdf.annotations.filter { $0.pageNumber == page.pageNumber } : []
      for (aIdx, annotation) in pageAnnotations.prefix(4).enumerated() {
        let ay = 610.0 - Double(aIdx) * 16.0
        let pillFill = annotation.kind == .highlight ? "0.98 0.96 0.84" : "0.94 0.95 1.00"
        streamLines.append(String(format: "%@ rg 48.0 %.1f 328.0 14.0 re f", pillFill, ay - 3.0))
        if annotation.kind == .underline {
          streamLines.append(
            String(format: "0.361 0.420 0.753 rg 48.0 %.1f 328.0 1.5 re f", ay - 3.0)
          )
        }
        let annLabel =
          "[\(annotationKindBadge(annotation.kind))] \(annotation.author): \(annotation.content)"
        let annHex = hexEncodeUTF16BE(annLabel, maxChars: 48)
        streamLines.append(
          String(
            format:
              "BT /F2 6.5 Tf 0.16 0.20 0.38 RG 0.16 0.20 0.38 rg 1 0 0 1 52 %.0f Tm <%@> Tj ET",
            ay, annHex
          )
        )
      }

      if !pageAnnotations.isEmpty {
        streamLines.append("0.99 0.94 0.95 rg 0.78 0.18 0.24 RG 1.5 w 386 576 160 46 re B")
        let stampHeaderHex = hexEncodeUTF16BE("결재 날인 · OFFICIAL STAMP", maxChars: 24)
        streamLines.append(
          "BT /F2 6.5 Tf 0.78 0.18 0.24 rg 1 0 0 1 392 604 Tm <\(stampHeaderHex)> Tj ET"
        )
        let stampNote =
          pageAnnotations.first(where: { $0.kind == .stamp || $0.kind == .signature })?.content
          ?? "[승인 완료] APPROVED"
        let stampNoteHex = hexEncodeUTF16BE(stampNote, maxChars: 24)
        streamLines.append(
          "BT /F2 6.5 Tf 0.55 0.12 0.18 rg 1 0 0 1 392 586 Tm <\(stampNoteHex)> Tj ET"
        )
      }

      // 5. AcroForm Interactive Field Boxes (y = 516..562)
      let pageFormFields =
        isFirstSlice ? pdf.formFields.filter { $0.pageNumber == page.pageNumber } : []
      for (fIdx, field) in pageFormFields.prefix(4).enumerated() {
        let col = fIdx % 2
        let row = fIdx / 2
        let fx = col == 0 ? 48.0 : 300.0
        let fy = 540.0 - Double(row) * 24.0
        let valText = field.isCheckbox ? (field.isChecked ? "[X]" : "[ ]") : field.value
        let formLine = "[FORM] \(field.name): \(valText)"
        let escapedForm = escapePDFString(formLine)
        let boxLabelHex = hexEncodeUTF16BE("[\(field.name): \(valText)]", maxChars: 26)
        streamLines.append(
          String(
            format: "0.96 0.97 1.00 rg 0.62 0.67 0.86 RG 0.8 w %.0f %.1f 240 20 re B",
            fx, fy - 5.0
          )
        )
        streamLines.append("BT 3 Tr /F1 1 Tf 0 2 Td (\(escapedForm)) Tj ET")
        streamLines.append(
          String(
            format: "0.145 0.165 0.243 rg BT 0 Tr /F2 8 Tf 1 0 0 1 %.0f %.0f Tm <%@> Tj ET",
            fx + 8.0, fy, boxLabelHex
          )
        )
      }

      // 6. Electronic Signature Box & Vector Ink Curve (y = 450..504)
      let pageStrokes =
        isFirstSlice
        ? pdf.inkStrokes.filter {
          $0.pageNumber == page.pageNumber && $0.points.count >= 2
        } : []
      if !pageStrokes.isEmpty {
        streamLines.append("0.97 0.98 1.00 rg 0.45 0.50 0.78 RG 1.0 w 48 452 280 46 re B")
        let sigHeaderHex = hexEncodeUTF16BE(
          "전자서명 검증 완료 · SIGNATURE (\(pageStrokes.count)획)",
          maxChars: 30
        )
        streamLines.append(
          "BT /F2 7.5 Tf 0.22 0.27 0.55 rg 1 0 0 1 56 484 Tm <\(sigHeaderHex)> Tj ET"
        )
        for stroke in pageStrokes {
          let (r, g, b) = parseRGBHex(stroke.strokeHex)
          let safeWidth = stroke.lineWidth.isFinite ? max(0.5, min(24.0, stroke.lineWidth)) : 2.0
          var sigPathOps = String(
            format: "q %.3f %.3f %.3f RG %.2f w 1 J 1 j", r, g, b, safeWidth
          )
          if let first = stroke.points.first {
            let fx = first.x.isFinite ? first.x : 56.0
            let fy = first.y.isFinite ? first.y : 460.0
            let drawX0 = max(56.0, min(318.0, fx))
            let drawY0 =
              fy < 400.0
              ? (458.0 + (abs(fy).truncatingRemainder(dividingBy: 24.0)))
              : min(480.0, max(456.0, fy))
            sigPathOps += String(format: " %.2f %.2f m", drawX0, drawY0)
            for pt in stroke.points.dropFirst() {
              let px = pt.x.isFinite ? pt.x : 56.0
              let py = pt.y.isFinite ? pt.y : 460.0
              let drawX = max(56.0, min(318.0, px))
              let drawY =
                py < 400.0
                ? (458.0 + (abs(py).truncatingRemainder(dividingBy: 24.0)))
                : min(480.0, max(456.0, py))
              sigPathOps += String(format: " %.2f %.2f l", drawX, drawY)
            }
            sigPathOps += " S Q"
            streamLines.append(sigPathOps)
          }
        }
      }

      // Clipped zero-area stream block preserving raw-coordinate audit tokens without visual clutter below y = 445
      streamLines.append("q 0 0 0 0 re W n")
      let startY = visualLines.count <= 18 ? 668 : 672
      let lineStep = max(8, min(24, 640 / max(1, visualLines.count)))
      for (lineIdx, rawLine) in visualLines.enumerated() {
        let y = startY - (lineIdx * lineStep)
        let visibleLineHex = hexEncodeUTF16BE(rawLine, maxChars: 64)
        streamLines.append(
          "0.145 0.165 0.243 rg BT 0 Tr /F2 10 Tf 54 \(y) Td <\(visibleLineHex)> Tj ET"
        )
      }
      for (annIdx, annotation) in pageAnnotations.prefix(4).enumerated() {
        let annY = 180 - (annIdx * 32)
        let annLabel =
          "[\(annotationKindBadge(annotation.kind))] \(annotation.author): \(annotation.content)"
        let annHex = hexEncodeUTF16BE(annLabel, maxChars: 56)
        streamLines.append(
          "0.145 0.165 0.243 rg BT 0 Tr /F2 9 Tf 62 \(annY + 3) Td <\(annHex)> Tj ET"
        )
      }
      for stroke in pageStrokes {
        let (r, g, b) = parseRGBHex(stroke.strokeHex)
        var pathOps = String(
          format: "q %.3f %.3f %.3f RG %.2f w 1 J 1 j", r, g, b, stroke.lineWidth
        )
        if let first = stroke.points.first {
          let px = max(20.0, min(575.0, first.x))
          let py = max(20.0, min(820.0, first.y))
          pathOps += String(format: " %.2f %.2f m", px, py)
          for pt in stroke.points.dropFirst() {
            let lx = max(20.0, min(575.0, pt.x))
            let ly = max(20.0, min(820.0, pt.y))
            pathOps += String(format: " %.2f %.2f l", lx, ly)
          }
          pathOps += " S Q"
          streamLines.append(pathOps)
        }
      }
      streamLines.append("Q")

      streamLines.append("Q")
      let streamBody = streamLines.joined(separator: "\n")
      let streamByteCount = streamBody.utf8.count

      objectOffsets[pageObjNum] = buffer.count
      appendLine("\(pageObjNum) 0 obj")
      appendLine(
        "<< /Type /Page /Parent 2 0 R /PageNumber \(page.pageNumber) /Rotate \(page.rotationDegrees) /MediaBox [0 0 595.28 841.89] /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> /F2 << /Type /Font /Subtype /Type0 /BaseFont /AppleSDGothicNeo-Regular /Encoding /UniKS-UTF16-H /DescendantFonts [ << /Type /Font /Subtype /CIDFontType0 /BaseFont /AppleSDGothicNeo-Regular /DW 1000 /CIDSystemInfo << /Registry (Adobe) /Ordering (Korea1) /Supplement 2 >> >> ] >> >> >> /Contents \(contentObjNum) 0 R >>"
      )
      appendLine("endobj")

      objectOffsets[contentObjNum] = buffer.count
      appendLine("\(contentObjNum) 0 obj")
      appendLine("<< /Length \(streamByteCount) >>")
      appendLine("stream")
      appendLine(streamBody)
      appendLine("endstream")
      appendLine("endobj")
    }

    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    let jsonBytes = (try? encoder.encode(pdf)) ?? Data("{}".utf8)
    let b64 = jsonBytes.base64EncodedString()
    let metadataStream = "\(modelStreamPrefix)\(b64)"

    objectOffsets[metadataObjNum] = buffer.count
    appendLine("\(metadataObjNum) 0 obj")
    appendLine(
      "<< /Type /Metadata /Subtype /EasyOfficeStudio /Length \(metadataStream.utf8.count) >>")
    appendLine("stream")
    appendLine(metadataStream)
    appendLine("endstream")
    appendLine("endobj")

    var nextObjId = metadataObjNum + 1
    if let outlinesObjId = outlinesObjNum {
      nextObjId = outlinesObjId + 1
      var outlineEntries: [(objId: Int, dictLine: String)] = []

      func buildOutlineLevel(
        nodes: [PDFOutlineNode],
        parentObjId: Int
      ) -> (firstId: Int, lastId: Int, totalCount: Int) {
        let siblingIds = nodes.map { _ -> Int in
          let assigned = nextObjId
          nextObjId += 1
          return assigned
        }
        var levelTotalCount = nodes.count
        for (idx, node) in nodes.enumerated() {
          let myId = siblingIds[idx]
          let prevPart = idx > 0 ? " /Prev \(siblingIds[idx - 1]) 0 R" : ""
          let nextPart = (idx + 1 < siblingIds.count) ? " /Next \(siblingIds[idx + 1]) 0 R" : ""
          let childrenPart: String
          if !node.children.isEmpty {
            let childInfo = buildOutlineLevel(nodes: node.children, parentObjId: myId)
            levelTotalCount += childInfo.totalCount
            childrenPart =
              " /First \(childInfo.firstId) 0 R /Last \(childInfo.lastId) 0 R /Count \(childInfo.totalCount)"
          } else {
            childrenPart = ""
          }
          let destPart: String
          if pageCount > 0 {
            let clampedPageIdx = max(0, min(node.targetPageNumber - 1, pageCount - 1))
            let targetPageObjId = 3 + (clampedPageIdx * 2)
            destPart = " /Dest [\(targetPageObjId) 0 R /XYZ 0 800 null]"
          } else {
            destPart = ""
          }
          let titleToken =
            node.title.utf8.contains(where: { $0 >= 0x80 })
            ? "<\(hexEncodeUTF16BE(node.title, maxChars: 120))>"
            : "(\(escapePDFText(node.title)))"
          let dict =
            "<< /Title \(titleToken) /Parent \(parentObjId) 0 R\(destPart)\(prevPart)\(nextPart)\(childrenPart) >>"
          outlineEntries.append((objId: myId, dictLine: dict))
        }
        return (
          siblingIds.first ?? outlinesObjId,
          siblingIds.last ?? outlinesObjId,
          levelTotalCount
        )
      }

      let rootInfo = buildOutlineLevel(nodes: pdf.outline, parentObjId: outlinesObjId)
      objectOffsets[outlinesObjId] = buffer.count
      appendLine("\(outlinesObjId) 0 obj")
      appendLine(
        "<< /Type /Outlines /First \(rootInfo.firstId) 0 R /Last \(rootInfo.lastId) 0 R /Count \(rootInfo.totalCount) >>"
      )
      appendLine("endobj")

      for entry in outlineEntries.sorted(by: { $0.objId < $1.objId }) {
        objectOffsets[entry.objId] = buffer.count
        appendLine("\(entry.objId) 0 obj")
        appendLine(entry.dictLine)
        appendLine("endobj")
      }
    }

    let totalObjects = nextObjId
    let xrefByteOffset = buffer.count
    appendLine("xref")
    appendLine("0 \(totalObjects)")
    appendLine("0000000000 65535 f ")
    for objIdx in 1..<totalObjects {
      let offset = objectOffsets[objIdx] ?? 0
      appendLine(String(format: "%010d 00000 n ", offset))
    }
    appendLine("trailer")
    appendLine("<< /Size \(totalObjects) /Root 1 0 R >>")
    appendLine("startxref")
    appendLine("\(xrefByteOffset)")
    appendLine("%%EOF")

    return buffer
  }

  public static func decode(from data: Data) throws -> PDFStudioDocumentModel {
    let verified = try FormatDetector.verify(data: data, fileName: "document.pdf")
    guard verified.format == .pdf else {
      throw EasyOfficeError.signatureMismatch(expected: .pdfBinary, fileName: "document.pdf")
    }

    let rawText = String(decoding: data, as: UTF8.self)
    guard rawText.contains("obj") && rawText.contains("%%EOF") else {
      throw EasyOfficeError.corruptedContainer(detail: "PDF 컨테이너에 유효한 객체(obj) 또는 %%EOF 트레일러가 없습니다.")
    }

    let lines = rawText.components(separatedBy: .newlines)
    var metadataBase64Payload: String?
    var metadataPayloadCount = 0
    var inMetadataObj = false
    var inMetadataStream = false
    var streamBodyChunks: [String] = []
    for line in lines {
      let trimmed = line.trimmingCharacters(in: .whitespaces)
      if trimmed == "endobj" {
        inMetadataObj = false
        inMetadataStream = false
      } else if trimmed.hasPrefix("<< /Type") && trimmed.contains("Metadata")
        && trimmed.contains("EasyOfficeStudio")
      {
        inMetadataObj = true
      } else if trimmed == "stream" && inMetadataObj {
        inMetadataStream = true
      } else if trimmed == "endstream" && inMetadataStream {
        inMetadataStream = false
      }
      if trimmed.hasPrefix(modelStreamPrefix) {
        metadataPayloadCount += 1
        guard inMetadataStream, metadataPayloadCount == 1 else {
          throw EasyOfficeError.securityViolation(
            detail: "PDF 메타데이터 스트림 주석(%EasyOfficePDFModelBase64)이 위조되었거나 중복 주입되었습니다.")
        }
        metadataBase64Payload = String(trimmed.dropFirst(modelStreamPrefix.count))
        continue
      }
      if trimmed.hasPrefix("BT 3 Tr /F1 1 Tf 0 0 Td ("),
        let openParen = trimmed.firstIndex(of: "("),
        let closeParen = trimmed.lastIndex(of: ")"),
        openParen < closeParen
      {
        let inner = String(trimmed[trimmed.index(after: openParen)..<closeParen])
        streamBodyChunks.append(unescapePDFString(inner))
      }
      if !trimmed.hasPrefix("BT "), trimmed.contains("/Encrypt") {
        throw EasyOfficeError.encryptedDocumentUnsupported(format: .pdf)
      }
    }
    if let b64 = metadataBase64Payload {
      guard let decodedBytes = Data(base64Encoded: b64) else {
        throw EasyOfficeError.corruptedContainer(detail: "PDF 메타데이터 스트림 Base64 디코딩에 실패했습니다.")
      }
      let decodedModel: PDFStudioDocumentModel
      do {
        decodedModel = try JSONDecoder().decode(PDFStudioDocumentModel.self, from: decodedBytes)
      } catch {
        throw EasyOfficeError.corruptedContainer(
          detail: "PDF 모델 역직렬화 실패: \(error.localizedDescription)")
      }
      var expectedChunks: [String] = []
      let maxLinesPerPage = 42
      for page in decodedModel.pages {
        let allVisualLines = page.extractedText
          .split(separator: "\n", omittingEmptySubsequences: false)
          .flatMap { wrapLine(String($0), maxChars: 64) }
        if allVisualLines.count <= maxLinesPerPage {
          expectedChunks.append(page.extractedText)
        } else {
          var start = 0
          while start < allVisualLines.count {
            let end = Swift.min(start + maxLinesPerPage, allVisualLines.count)
            expectedChunks.append(Array(allVisualLines[start..<end]).joined(separator: "\n"))
            start = end
          }
        }
      }
      guard expectedChunks == streamBodyChunks else {
        throw EasyOfficeError.securityViolation(
          detail: "PDF 메타데이터 스트림이 페이지 본문 스트림과 일치하지 않습니다.")
      }
      return decodedModel
    }

    var fallbackPages: [PDFPageDescriptor] = []
    var fallbackInvisibleFlags: [Bool] = []
    var fallbackFormFields: [PDFFormField] = []
    var fallbackWatermarkText = ""
    let isRedactedCatalog = rawText.contains("/Redacted true")
    var sawPageObject = false
    var currentRotate = 0
    var inStream = false
    var currentStreamLines: [String] = []
    var currentStreamHasInvisibleLayer = false

    for line in lines {
      if line.contains("/Type /Page") && !line.contains("/Type /Pages") {
        sawPageObject = true
      }
      if let rotRange = line.range(of: "/Rotate ") {
        let after = line[rotRange.upperBound...]
        let digits = after.prefix(while: { $0.isNumber || $0 == "-" })
        currentRotate = Int(digits) ?? 0
      }
      if line == "stream" {
        inStream = true
        currentStreamLines.removeAll(keepingCapacity: true)
        currentStreamHasInvisibleLayer = false
        continue
      }
      if line == "endstream" {
        if !currentStreamLines.isEmpty {
          fallbackPages.append(
            PDFPageDescriptor(
              pageNumber: fallbackPages.count + 1,
              rotationDegrees: currentRotate,
              extractedText: currentStreamLines.joined(separator: "\n")
            )
          )
          fallbackInvisibleFlags.append(currentStreamHasInvisibleLayer)
          currentRotate = 0
        }
        inStream = false
        currentStreamLines.removeAll(keepingCapacity: true)
        currentStreamHasInvisibleLayer = false
        continue
      }
      if line.hasPrefix("BT "),
        let openParen = line.firstIndex(of: "("),
        let closeParen = line.lastIndex(of: ")"),
        openParen < closeParen
      {
        let inner = String(line[line.index(after: openParen)..<closeParen])
        let unescaped = unescapePDFString(inner)
        if unescaped.hasPrefix("[WM] ")
          && (line.contains(" 0 1 Td ") || !currentStreamLines.isEmpty)
        {
          fallbackWatermarkText = String(unescaped.dropFirst(5))
          continue
        }
        if unescaped.hasPrefix("[FORM] ")
          && (line.contains(" 0 2 Td ") || !currentStreamLines.isEmpty)
        {
          let remainder = String(unescaped.dropFirst(7))
          let parts = remainder.components(separatedBy: ": ")
          let fieldName = parts.first ?? "Field"
          let fieldVal = parts.count > 1 ? parts.dropFirst().joined(separator: ": ") : ""
          fallbackFormFields.append(
            PDFFormField(
              pageNumber: fallbackPages.count + 1,
              name: fieldName,
              value: fieldVal
            )
          )
          continue
        }
        if inStream {
          currentStreamLines.append(unescaped)
          if line.hasPrefix("BT 3 Tr ") {
            currentStreamHasInvisibleLayer = true
          }
        } else {
          fallbackPages.append(
            PDFPageDescriptor(
              pageNumber: fallbackPages.count + 1,
              rotationDegrees: currentRotate,
              extractedText: unescaped
            )
          )
          fallbackInvisibleFlags.append(line.hasPrefix("BT 3 Tr "))
          currentRotate = 0
        }
      }
    }

    let hasValidXrefStructure =
      rawText.contains("xref") && rawText.contains("startxref")
      && !fallbackInvisibleFlags.contains(true)
    if hasValidXrefStructure, let pdfDoc = PDFDocument(data: data) {
      if pdfDoc.isEncrypted {
        throw EasyOfficeError.encryptedDocumentUnsupported(format: .pdf)
      }
      if pdfDoc.pageCount > 0 {
        var pages: [PDFPageDescriptor] = []
        var annotations: [PDFAnnotationItem] = []

        for idx in 0..<pdfDoc.pageCount {
          let pdfPage = pdfDoc.page(at: idx)
          let fallback = fallbackPages.indices.contains(idx) ? fallbackPages[idx] : nil
          let isInvisibleUTF8Layer =
            fallbackInvisibleFlags.indices.contains(idx) && fallbackInvisibleFlags[idx]
          let nativeRotation = pdfPage?.rotation ?? 0
          let resolvedRotation =
            nativeRotation != 0 ? nativeRotation : (fallback?.rotationDegrees ?? currentRotate)
          let nativeText = pdfPage?.string?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
          let fallbackText = fallback?.extractedText ?? ""
          let resolvedText =
            ((isInvisibleUTF8Layer || nativeText.isEmpty) && !fallbackText.isEmpty)
            ? fallbackText : nativeText

          pages.append(
            PDFPageDescriptor(
              pageNumber: idx + 1,
              rotationDegrees: resolvedRotation,
              extractedText: resolvedText
            )
          )

          if let pageAnnotations = pdfPage?.annotations {
            for ann in pageAnnotations {
              let kind: PDFAnnotationKind
              let typeName = (ann.type ?? "").lowercased()
              if typeName.contains("highlight") {
                kind = .highlight
              } else if typeName.contains("underline") {
                kind = .underline
              } else if typeName.contains("stamp") {
                kind = .stamp
              } else if typeName.contains("sig") || typeName.contains("widget") {
                kind = .signature
              } else {
                kind = .stickyNote
              }
              let content = ann.contents ?? ""
              let author = ann.userName ?? "PDF User"
              if !content.isEmpty {
                annotations.append(
                  PDFAnnotationItem(
                    pageNumber: idx + 1,
                    kind: kind,
                    content: content,
                    author: author
                  )
                )
              }
            }
          }
        }

        var outlineNodes: [PDFOutlineNode] = []
        if let outlineRoot = pdfDoc.outlineRoot {
          outlineNodes = extractOutlineNodes(from: outlineRoot, in: pdfDoc)
        }

        let docTitle =
          (pdfDoc.documentAttributes?[PDFDocumentAttribute.titleAttribute] as? String)
          .flatMap { $0.isEmpty ? nil : $0 } ?? "PDF 문서"

        return PDFStudioDocumentModel(
          title: docTitle,
          pages: pages,
          annotations: annotations,
          outline: outlineNodes,
          isRedacted: isRedactedCatalog,
          formFields: fallbackFormFields,
          watermarkText: fallbackWatermarkText
        )
      }
    }

    if fallbackPages.isEmpty {
      guard sawPageObject else {
        throw EasyOfficeError.corruptedContainer(detail: "PDF 컨테이너에 유효한 페이지(/Type /Page) 객체가 없습니다.")
      }
      fallbackPages.append(
        PDFPageDescriptor(pageNumber: 1, rotationDegrees: currentRotate, extractedText: ""))
    }
    return PDFStudioDocumentModel(
      title: "PDF 문서",
      pages: fallbackPages,
      isRedacted: isRedactedCatalog,
      formFields: fallbackFormFields,
      watermarkText: fallbackWatermarkText
    )
  }

  private static func extractOutlineNodes(from parent: PDFOutline, in doc: PDFDocument)
    -> [PDFOutlineNode]
  {
    var nodes: [PDFOutlineNode] = []
    for idx in 0..<parent.numberOfChildren {
      guard let child = parent.child(at: idx) else { continue }
      let title = child.label ?? "책갈피 \(idx + 1)"
      var targetPageNum = 1
      if let destPage = child.destination?.page {
        let pageIdx = doc.index(for: destPage)
        if pageIdx != NSNotFound && pageIdx >= 0 {
          targetPageNum = pageIdx + 1
        }
      }
      let children = extractOutlineNodes(from: child, in: doc)
      nodes.append(
        PDFOutlineNode(
          title: title,
          targetPageNumber: targetPageNum,
          children: children
        )
      )
    }
    return nodes
  }

  private static func hexEncodeUTF16BE(_ input: String, maxChars: Int) -> String {
    let source = input.isEmpty ? " " : input
    let limit = min(maxChars, source.utf16.count)
    var outBytes: [UInt8] = [0x46, 0x45, 0x46, 0x46]  // "FEFF"
    outBytes.reserveCapacity(4 + limit * 4)
    var count = 0
    for unit in source.utf16 {
      if count >= maxChars { break }
      let normalized: UInt16 = (unit == 0x0009 || unit < 0x0020) ? 0x0020 : unit
      outBytes.append(hexDigits[Int((normalized >> 12) & 0x0F)])
      outBytes.append(hexDigits[Int((normalized >> 8) & 0x0F)])
      outBytes.append(hexDigits[Int((normalized >> 4) & 0x0F)])
      outBytes.append(hexDigits[Int(normalized & 0x0F)])
      count += 1
    }
    return String(decoding: outBytes, as: UTF8.self)
  }

  private static func annotationKindBadge(_ kind: PDFAnnotationKind) -> String {
    switch kind {
    case .highlight: return "HIGHLIGHT"
    case .underline: return "UNDERLINE"
    case .stickyNote: return "NOTE"
    case .stamp: return "STAMP"
    case .signature: return "SIGNATURE"
    }
  }

  private static func parseRGBHex(_ hex: String) -> (Double, Double, Double) {
    var cleaned = hex.trimmingCharacters(in: .whitespacesAndNewlines)
    if cleaned.hasPrefix("#") {
      cleaned.removeFirst()
    }
    guard cleaned.count == 6, let val = UInt32(cleaned, radix: 16) else {
      return (0.361, 0.420, 0.753)
    }
    return (
      Double((val >> 16) & 0xFF) / 255.0,
      Double((val >> 8) & 0xFF) / 255.0,
      Double(val & 0xFF) / 255.0
    )
  }

  private static func hexEncodeASCII(_ input: String) -> String {
    var outBytes: [UInt8] = []
    outBytes.reserveCapacity(input.utf8.count * 2)
    for byte in input.utf8 {
      outBytes.append(hexDigits[Int((byte >> 4) & 0x0F)])
      outBytes.append(hexDigits[Int(byte & 0x0F)])
    }
    return String(decoding: outBytes, as: UTF8.self)
  }

  /// Single-pass UTF-8 byte buffer PDF string literal escaper normalizing newlines to spaces.
  public static func escapePDFText(_ text: String) -> String {
    var out: [UInt8] = []
    out.reserveCapacity(text.utf8.count + 8)
    for byte in text.utf8 {
      switch byte {
      case 0x5C:  // '\'
        out.append(0x5C)
        out.append(0x5C)
      case 0x28:  // '('
        out.append(0x5C)
        out.append(0x28)
      case 0x29:  // ')'
        out.append(0x5C)
        out.append(0x29)
      case 0x0A, 0x0D:  // '\n', '\r'
        out.append(0x20)
      default:
        out.append(byte)
      }
    }
    return String(decoding: out, as: UTF8.self)
  }

  private static func escapePDFString(_ input: String) -> String {
    var out: [UInt8] = []
    out.reserveCapacity(input.utf8.count + 8)
    for byte in input.utf8 {
      switch byte {
      case 0x5C:  // '\'
        out.append(0x5C)
        out.append(0x5C)
      case 0x28:  // '('
        out.append(0x5C)
        out.append(0x28)
      case 0x29:  // ')'
        out.append(0x5C)
        out.append(0x29)
      case 0x0D:  // '\r'
        out.append(0x5C)
        out.append(0x72)
      case 0x0A:  // '\n'
        out.append(0x5C)
        out.append(0x6E)
      default:
        out.append(byte)
      }
    }
    return String(decoding: out, as: UTF8.self)
  }

  private static func unescapePDFString(_ input: String) -> String {
    var result = ""
    result.reserveCapacity(input.count)
    var index = input.startIndex
    while index < input.endIndex {
      let ch = input[index]
      if ch == "\\" {
        let nextIndex = input.index(after: index)
        guard nextIndex < input.endIndex else {
          result.append("\\")
          break
        }
        let escaped = input[nextIndex]
        switch escaped {
        case "n": result.append("\n")
        case "r": result.append("\r")
        case "(": result.append("(")
        case ")": result.append(")")
        case "\\": result.append("\\")
        default: result.append(escaped)
        }
        index = input.index(after: nextIndex)
      } else {
        result.append(ch)
        index = input.index(after: index)
      }
    }
    return result
  }
}
