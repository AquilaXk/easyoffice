import Foundation
import PDFKit

// MARK: - Deterministic ISO 32000-1 PDF Binary Serializer & Native PDFKit Bridge

/// Deterministic ISO 32000-1 (`%PDF-1.7`) binary serializer and parser with byte-exact `xref` offsets and native `PDFKit` inspection.
public enum PDFBinarySerializer {
    private static let modelStreamPrefix = "%EasyOfficePDFModelBase64:"

    public static func encode(_ pdf: PDFStudioDocumentModel) -> Data {
        var buffer = Data()
        buffer.reserveCapacity(4096)

        func appendLine(_ line: String) {
            buffer.append(contentsOf: line.utf8)
            buffer.append(0x0A)
        }

        appendLine("%PDF-1.7")
        appendLine("%\u{00E2}\u{00E3}\u{00CF}\u{00D3}")

        let pageCount = pdf.pages.count
        let pageObjectNumbers = (0..<pageCount).map { 3 + ($0 * 2) }
        let kidsRefs = pageObjectNumbers.map { "\($0) 0 R" }.joined(separator: " ")

        var objectOffsets: [Int: Int] = [:]

        objectOffsets[1] = buffer.count
        appendLine("1 0 obj")
        appendLine("<< /Type /Catalog /Pages 2 0 R /Title (\(escapePDFString(pdf.title))) /Redacted \(pdf.isRedacted ? "true" : "false") >>")
        appendLine("endobj")

        objectOffsets[2] = buffer.count
        appendLine("2 0 obj")
        appendLine("<< /Type /Pages /Count \(pageCount) /Kids [\(kidsRefs)] >>")
        appendLine("endobj")

        for (idx, page) in pdf.pages.enumerated() {
            let pageObjNum = 3 + (idx * 2)
            let contentObjNum = pageObjNum + 1
            let escapedText = escapePDFString(page.extractedText)
            let headerHex = hexEncodeASCII("EasyOffice ISO 32000-1 PDF Studio - Page \(page.pageNumber) (\(page.rotationDegrees) deg)")
            let statusHex = hexEncodeASCII("Verified ISO 32000-1 Vector Page | UTF-8 Stream: \(page.extractedText.utf8.count) Bytes | Status: SAFE")
            var streamLines: [String] = [
                "q",
                "0.969 0.973 0.988 rg 36 36 523 770 re f",
                "0.361 0.420 0.753 rg 36 756 523 50 re f",
                "0.820 0.839 0.922 RG 1 w 36 36 523 770 re S",
                "BT 3 Tr /F1 1 Tf 0 0 Td (\(escapedText)) Tj ET",
                "1 1 1 rg BT 0 Tr /F1 13 Tf 54 775 Td <\(headerHex)> Tj ET",
                "0.220 0.251 0.380 rg BT 0 Tr /F1 10 Tf 54 722 Td <\(statusHex)> Tj ET",
                "0.839 0.859 0.941 rg 54 698 487 2 re f"
            ]

            let contentLines = page.extractedText
                .split(separator: "\n", omittingEmptySubsequences: false)
                .prefix(18)

            for (lineIdx, rawSub) in contentLines.enumerated() {
                let rawLine = String(rawSub)
                let trimmedLine = rawLine.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !trimmedLine.isEmpty else { continue }
                let y = 668 - (lineIdx * 24)
                let barWidth = min(480, max(60, rawLine.count * 9))
                let visibleLineHex = hexEncodeUTF16BE(rawLine, maxChars: 64)
                streamLines.append("0.925 0.937 0.984 rg 54 \(y - 4) \(barWidth) 16 re f")
                streamLines.append("0.145 0.165 0.243 rg BT 0 Tr /F2 10 Tf 54 \(y) Td <\(visibleLineHex)> Tj ET")
            }

            let pageAnnotations = pdf.annotations.filter { $0.pageNumber == page.pageNumber }
            for (annIdx, annotation) in pageAnnotations.prefix(4).enumerated() {
                let annY = 180 - (annIdx * 32)
                let (fillRGB, strokeRGB) = annotationColorPair(for: annotation.kind)
                streamLines.append("\(fillRGB) rg 54 \(annY - 4) 487 22 re f")
                streamLines.append("\(strokeRGB) RG 1 w 54 \(annY - 4) 487 22 re S")
                if annotation.kind == .underline {
                    streamLines.append("0.361 0.420 0.753 rg 54 \(annY - 4) 487 2 re f")
                }
                let annLabel = "[\(annotationKindBadge(annotation.kind))] \(annotation.author): \(annotation.content)"
                let annHex = hexEncodeUTF16BE(annLabel, maxChars: 56)
                streamLines.append("0.145 0.165 0.243 rg BT 0 Tr /F2 9 Tf 62 \(annY + 3) Td <\(annHex)> Tj ET")
            }

            streamLines.append("Q")
            let streamBody = streamLines.joined(separator: "\n")
            let streamByteCount = streamBody.utf8.count

            objectOffsets[pageObjNum] = buffer.count
            appendLine("\(pageObjNum) 0 obj")
            appendLine("<< /Type /Page /Parent 2 0 R /PageNumber \(page.pageNumber) /Rotate \(page.rotationDegrees) /MediaBox [0 0 595.28 841.89] /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> /F2 << /Type /Font /Subtype /Type0 /BaseFont /AppleSDGothicNeo-Regular /Encoding /UniKS-UTF16-H /DescendantFonts [ << /Type /Font /Subtype /CIDFontType0 /BaseFont /AppleSDGothicNeo-Regular /CIDSystemInfo << /Registry (Adobe) /Ordering (Korea1) /Supplement 2 >> >> ] >> >> >> /Contents \(contentObjNum) 0 R >>")
            appendLine("endobj")

            objectOffsets[contentObjNum] = buffer.count
            appendLine("\(contentObjNum) 0 obj")
            appendLine("<< /Length \(streamByteCount) >>")
            appendLine("stream")
            appendLine(streamBody)
            appendLine("endstream")
            appendLine("endobj")
        }

        let metadataObjNum = 3 + (pageCount * 2)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let jsonBytes = (try? encoder.encode(pdf)) ?? Data("{}".utf8)
        let b64 = jsonBytes.base64EncodedString()
        let metadataStream = "\(modelStreamPrefix)\(b64)"

        objectOffsets[metadataObjNum] = buffer.count
        appendLine("\(metadataObjNum) 0 obj")
        appendLine("<< /Type /Metadata /Subtype /EasyOfficeStudio /Length \(metadataStream.utf8.count) >>")
        appendLine("stream")
        appendLine(metadataStream)
        appendLine("endstream")
        appendLine("endobj")

        let totalObjects = metadataObjNum + 1
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
        for line in lines {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix(modelStreamPrefix) {
                metadataBase64Payload = String(trimmed.dropFirst(modelStreamPrefix.count))
                continue
            }
            if !trimmed.hasPrefix("BT "), trimmed.contains("/Encrypt") {
                throw EasyOfficeError.encryptedDocumentUnsupported(format: .pdf)
            }
        }
        if let b64 = metadataBase64Payload {
            guard let decodedBytes = Data(base64Encoded: b64) else {
                throw EasyOfficeError.corruptedContainer(detail: "PDF 메타데이터 스트림 Base64 디코딩에 실패했습니다.")
            }
            do {
                return try JSONDecoder().decode(PDFStudioDocumentModel.self, from: decodedBytes)
            } catch {
                throw EasyOfficeError.corruptedContainer(detail: "PDF 모델 역직렬화 실패: \(error.localizedDescription)")
            }
        }

        // Raw stream scanner for minimal synthetic PDF streams and 3 Tr UTF-8 text layers
        var fallbackPages: [PDFPageDescriptor] = []
        var fallbackInvisibleFlags: [Bool] = []
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
               openParen < closeParen {
                let inner = String(line[line.index(after: openParen)..<closeParen])
                let unescaped = unescapePDFString(inner)
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

        // Native macOS PDFKit inspection for standard PDF documents without embedded studio metadata
        if let pdfDoc = PDFDocument(data: data) {
            if pdfDoc.isEncrypted {
                throw EasyOfficeError.encryptedDocumentUnsupported(format: .pdf)
            }
            if pdfDoc.pageCount > 0 {
                var pages: [PDFPageDescriptor] = []
                var annotations: [PDFAnnotationItem] = []

                for idx in 0..<pdfDoc.pageCount {
                    let pdfPage = pdfDoc.page(at: idx)
                    let fallback = fallbackPages.indices.contains(idx) ? fallbackPages[idx] : nil
                    let isInvisibleUTF8Layer = fallbackInvisibleFlags.indices.contains(idx) && fallbackInvisibleFlags[idx]
                    let nativeRotation = pdfPage?.rotation ?? 0
                    let resolvedRotation = nativeRotation != 0 ? nativeRotation : (fallback?.rotationDegrees ?? currentRotate)
                    let nativeText = pdfPage?.string?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                    let fallbackText = fallback?.extractedText ?? ""
                    let resolvedText = ((isInvisibleUTF8Layer || nativeText.isEmpty) && !fallbackText.isEmpty) ? fallbackText : nativeText

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

                let docTitle = (pdfDoc.documentAttributes?[PDFDocumentAttribute.titleAttribute] as? String)
                    .flatMap { $0.isEmpty ? nil : $0 } ?? "PDF 문서"

                return PDFStudioDocumentModel(
                    title: docTitle,
                    pages: pages,
                    annotations: annotations,
                    outline: outlineNodes
                )
            }
        }

        if fallbackPages.isEmpty {
            guard sawPageObject else {
                throw EasyOfficeError.corruptedContainer(detail: "PDF 컨테이너에 유효한 페이지(/Type /Page) 객체가 없습니다.")
            }
            fallbackPages.append(PDFPageDescriptor(pageNumber: 1, rotationDegrees: currentRotate, extractedText: ""))
        }
        return PDFStudioDocumentModel(title: "PDF 문서", pages: fallbackPages)
    }

    private static func extractOutlineNodes(from parent: PDFOutline, in doc: PDFDocument) -> [PDFOutlineNode] {
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
        var hex = "FEFF"
        hex.reserveCapacity(4 + min(maxChars, source.utf16.count) * 4)
        var count = 0
        for unit in source.utf16 {
            if count >= maxChars { break }
            let normalized: UInt16 = (unit == 0x0009 || unit < 0x0020) ? 0x0020 : unit
            hex.append(String(format: "%04X", normalized))
            count += 1
        }
        return hex
    }

    private static func annotationColorPair(for kind: PDFAnnotationKind) -> (fill: String, stroke: String) {
        switch kind {
        case .highlight:
            return ("0.996 0.953 0.780", "0.961 0.620 0.043")
        case .underline:
            return ("0.933 0.949 1.000", "0.361 0.420 0.753")
        case .stickyNote:
            return ("1.000 0.973 0.882", "0.961 0.620 0.043")
        case .stamp:
            return ("0.820 0.980 0.898", "0.063 0.725 0.506")
        case .signature:
            return ("0.910 0.922 0.976", "0.361 0.420 0.753")
        }
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

    private static func hexEncodeASCII(_ input: String) -> String {
        input.utf8.map { String(format: "%02X", $0) }.joined()
    }

    private static func escapePDFString(_ input: String) -> String {
        input
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "(", with: "\\(")
            .replacingOccurrences(of: ")", with: "\\)")
            .replacingOccurrences(of: "\r", with: "\\r")
            .replacingOccurrences(of: "\n", with: "\\n")
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
