import Foundation

// MARK: - Shared XML Document, Worksheet & Presentation Model Mapper

/// Shared mapper eliminating duplicate paragraph, run, image, table, worksheet, slide, and metadata XML serialization across OOXML, ODF, and HWPX.
internal enum XMLDocumentModelMapper {

    static func sanitizeMediaFileName(_ raw: String) -> String {
        let base = (raw as NSString).lastPathComponent
        let cleaned = base.replacingOccurrences(of: "..", with: "_").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "\\", with: "_")
        return cleaned.isEmpty ? "image.bin" : cleaned
    }

    static func isBinaryMediaPath(_ path: String) -> Bool {
        path.hasPrefix("word/media/") || path.hasPrefix("Pictures/") || path.hasPrefix("BinData/") ||
            path.hasPrefix("ppt/media/") || path.hasPrefix("xl/media/")
    }

    static func paragraphAttributes(_ p: ParagraphBlock) -> String {
        let boldAttr = p.isBold ? "1" : "0"
        let italicAttr = p.isItalic ? "1" : "0"
        let underlineAttr = p.isUnderlined ? "1" : "0"
        let hasRunsAttr = p.runs.isEmpty ? "" : " hasRuns=\"1\""
        return "id=\"\(p.id.uuidString)\" style=\"\(XMLSanitizer.escape(p.style.rawValue))\" align=\"\(XMLSanitizer.escape(p.alignment.rawValue))\" font=\"\(XMLSanitizer.escape(p.fontName))\" size=\"\(p.fontSizePt)\" bold=\"\(boldAttr)\" italic=\"\(italicAttr)\" underline=\"\(underlineAttr)\" lineSpacing=\"\(p.lineSpacingPercent)\"\(hasRunsAttr)"
    }

    static func runAttributes(_ run: DocumentTextRun) -> String {
        var parts: [String] = [
            "id=\"\(run.id.uuidString)\"",
            "bold=\"\(run.isBold ? "1" : "0")\"",
            "italic=\"\(run.isItalic ? "1" : "0")\"",
            "underline=\"\(run.isUnderlined ? "1" : "0")\""
        ]
        if let fontName = run.fontName { parts.append("font=\"\(XMLSanitizer.escape(fontName))\"") }
        if let fontSizePt = run.fontSizePt { parts.append("size=\"\(fontSizePt)\"") }
        if let fg = run.foregroundHex { parts.append("color=\"\(XMLSanitizer.escape(fg))\"") }
        return parts.joined(separator: " ")
    }

    static func ooxmlStyleVal(for role: ParagraphStyleRole) -> String {
        switch role {
        case .title: return "Title"
        case .heading1: return "Heading1"
        case .heading2: return "Heading2"
        case .heading3: return "Heading3"
        case .quote: return "Quote"
        case .officialHeader: return "OfficialHeader"
        case .body: return "Normal"
        }
    }

    static func roleFromOOXMLStyle(_ raw: String) -> ParagraphStyleRole {
        if let exact = ParagraphStyleRole(rawValue: raw) { return exact }
        switch raw.lowercased().replacingOccurrences(of: " ", with: "") {
        case "title": return .title
        case "heading1", "1": return .heading1
        case "heading2", "2": return .heading2
        case "heading3", "3": return .heading3
        case "quote", "intensequote", "blockquote": return .quote
        case "officialheader": return .officialHeader
        default: return .body
        }
    }

    static func ooxmlJustificationVal(for alignment: TextAlignmentMode) -> String {
        switch alignment {
        case .leading: return "left"
        case .center: return "center"
        case .trailing: return "right"
        case .justified: return "both"
        }
    }

    static func alignmentFromOOXMLJustification(_ raw: String) -> TextAlignmentMode {
        if let exact = TextAlignmentMode(rawValue: raw) { return exact }
        switch raw.lowercased() {
        case "left", "start": return .leading
        case "center": return .center
        case "right", "end": return .trailing
        case "both", "distribute", "justify": return .justified
        default: return .justified
        }
    }

    static func parseRuns(from innerXML: String, childTextTag: String?) -> [DocumentTextRun] {
        let odtSpans = XMLSanitizer.extractElements(named: "text:span", in: innerXML)
        if !odtSpans.isEmpty {
            return odtSpans.map { elem in
                DocumentTextRun(
                    id: elem.attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID(),
                    text: XMLSanitizer.unescape(elem.innerXML),
                    isBold: elem.attributes["bold"] == "1",
                    isItalic: elem.attributes["italic"] == "1",
                    isUnderlined: elem.attributes["underline"] == "1",
                    fontName: elem.attributes["font"],
                    fontSizePt: elem.attributes["size"].flatMap(Double.init),
                    foregroundHex: elem.attributes["color"]
                )
            }
        }

        if childTextTag == "hp:t" {
            let hpRuns = XMLSanitizer.extractElements(named: "hp:run", in: innerXML)
            if !hpRuns.isEmpty {
                return hpRuns.map { elem in
                    let tText = XMLSanitizer.extractElements(named: "hp:t", in: elem.innerXML).map { XMLSanitizer.unescape($0.innerXML) }.joined()
                    return DocumentTextRun(
                        id: elem.attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID(),
                        text: tText,
                        isBold: elem.attributes["bold"] == "1",
                        isItalic: elem.attributes["italic"] == "1",
                        isUnderlined: elem.attributes["underline"] == "1",
                        fontName: elem.attributes["font"],
                        fontSizePt: elem.attributes["size"].flatMap(Double.init),
                        foregroundHex: elem.attributes["color"]
                    )
                }
            }
        }

        if childTextTag == "w:t" {
            let wRuns = XMLSanitizer.extractElements(named: "w:r", in: innerXML)
            if !wRuns.isEmpty {
                return wRuns.map { elem in
                    let tText = XMLSanitizer.extractElements(named: "w:t", in: elem.innerXML).map { XMLSanitizer.unescape($0.innerXML) }.joined()
                    var isBold = elem.attributes["bold"] == "1"
                    var isItalic = elem.attributes["italic"] == "1"
                    var isUnderlined = elem.attributes["underline"] == "1"
                    var fontName = elem.attributes["font"]
                    var fontSizePt = elem.attributes["size"].flatMap(Double.init)
                    var foregroundHex = elem.attributes["color"]

                    if let rPr = XMLSanitizer.extractElements(named: "w:rPr", in: elem.innerXML).first?.innerXML {
                        if fontName == nil,
                           let fontAttrs = XMLSanitizer.extractElements(named: "w:rFonts", in: rPr).first?.attributes,
                           let ascii = fontAttrs["w:ascii"] ?? fontAttrs["w:eastAsia"] ?? fontAttrs["ascii"] {
                            fontName = ascii
                        }
                        if elem.attributes["bold"] == nil,
                           let bElem = XMLSanitizer.extractElements(named: "w:b", in: rPr).first {
                            let val = (bElem.attributes["w:val"] ?? bElem.attributes["val"] ?? "1").lowercased()
                            isBold = val != "0" && val != "false"
                        }
                        if elem.attributes["italic"] == nil,
                           let iElem = XMLSanitizer.extractElements(named: "w:i", in: rPr).first {
                            let val = (iElem.attributes["w:val"] ?? iElem.attributes["val"] ?? "1").lowercased()
                            isItalic = val != "0" && val != "false"
                        }
                        if elem.attributes["underline"] == nil,
                           let uElem = XMLSanitizer.extractElements(named: "w:u", in: rPr).first {
                            let val = (uElem.attributes["w:val"] ?? uElem.attributes["val"] ?? "single").lowercased()
                            isUnderlined = val != "none" && val != "0" && val != "false"
                        }
                        if fontSizePt == nil,
                           let szAttrs = XMLSanitizer.extractElements(named: "w:sz", in: rPr).first?.attributes,
                           let valStr = szAttrs["w:val"] ?? szAttrs["val"],
                           let halfPts = Double(valStr) {
                            fontSizePt = halfPts / 2.0
                        }
                        if foregroundHex == nil,
                           let colorAttrs = XMLSanitizer.extractElements(named: "w:color", in: rPr).first?.attributes,
                           let rawHex = colorAttrs["w:val"] ?? colorAttrs["val"],
                           !rawHex.isEmpty, rawHex.lowercased() != "auto" {
                            foregroundHex = rawHex.hasPrefix("#") ? rawHex : "#\(rawHex)"
                        }
                    }

                    return DocumentTextRun(
                        id: elem.attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID(),
                        text: tText, isBold: isBold, isItalic: isItalic, isUnderlined: isUnderlined,
                        fontName: fontName, fontSizePt: fontSizePt, foregroundHex: foregroundHex
                    )
                }
            }
        }

        return []
    }

    static func parseParagraph(
        attributes: [String: String],
        innerXML: String,
        childTextTag: String?
    ) -> ParagraphBlock {
        let id = attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID()
        let parsedRuns = parseRuns(from: innerXML, childTextTag: childTextTag)
        let hasExplicitRuns = attributes["hasRuns"] == "1" || parsedRuns.count > 1 || (parsedRuns.count == 1 && parsedRuns[0].foregroundHex != nil)

        let extractedText: String
        if !parsedRuns.isEmpty {
            extractedText = parsedRuns.map(\.text).joined()
        } else if let childTag = childTextTag {
            let runs = XMLSanitizer.extractElements(named: childTag, in: innerXML)
            extractedText = !runs.isEmpty ? runs.map { XMLSanitizer.unescape($0.innerXML) }.joined() : XMLSanitizer.unescape(innerXML)
        } else {
            extractedText = XMLSanitizer.unescape(innerXML)
        }

        if let styleRaw = attributes["style"] {
            return ParagraphBlock(
                id: id,
                style: ParagraphStyleRole(rawValue: styleRaw) ?? roleFromOOXMLStyle(styleRaw),
                alignment: TextAlignmentMode(rawValue: attributes["align"] ?? "") ?? alignmentFromOOXMLJustification(attributes["align"] ?? ""),
                text: extractedText,
                runs: hasExplicitRuns ? parsedRuns : [],
                fontName: attributes["font"] ?? "Pretendard",
                fontSizePt: Double(attributes["size"] ?? "11.0") ?? 11.0,
                isBold: attributes["bold"] == "1",
                isItalic: attributes["italic"] == "1",
                isUnderlined: attributes["underline"] == "1",
                lineSpacingPercent: Double(attributes["lineSpacing"] ?? "160.0") ?? 160.0
            )
        }

        var style: ParagraphStyleRole = .body
        var alignment: TextAlignmentMode = .justified
        if let pPr = XMLSanitizer.extractElements(named: "w:pPr", in: innerXML).first?.innerXML {
            if let pStyleAttrs = XMLSanitizer.extractElements(named: "w:pStyle", in: pPr).first?.attributes,
               let val = pStyleAttrs["w:val"] ?? pStyleAttrs["val"] {
                style = roleFromOOXMLStyle(val)
            }
            if let jcAttrs = XMLSanitizer.extractElements(named: "w:jc", in: pPr).first?.attributes,
               let val = jcAttrs["w:val"] ?? jcAttrs["val"] {
                alignment = alignmentFromOOXMLJustification(val)
            }
        }

        let firstRun = parsedRuns.first
        return ParagraphBlock(
            id: id, style: style, alignment: alignment, text: extractedText,
            runs: hasExplicitRuns ? parsedRuns : [],
            fontName: firstRun?.fontName ?? "Pretendard",
            fontSizePt: firstRun?.fontSizePt ?? 11.0,
            isBold: parsedRuns.contains(where: \.isBold),
            isItalic: parsedRuns.contains(where: \.isItalic),
            isUnderlined: parsedRuns.contains(where: \.isUnderlined),
            lineSpacingPercent: 160.0
        )
    }

    static func tableAttributes(_ tbl: DocumentTableBlock) -> String {
        "id=\"\(tbl.id.uuidString)\" caption=\"\(XMLSanitizer.escape(tbl.caption))\" headerRow=\"\(tbl.hasHeaderRow ? "1" : "0")\" borderStyle=\"\(XMLSanitizer.escape(tbl.borderStyle.rawValue))\""
    }

    static func serializeTableSpans(_ tbl: DocumentTableBlock) -> String {
        guard !tbl.cellSpans.isEmpty else { return "" }
        return tbl.cellSpans.map { span in
            "<eo:cellSpan row=\"\(span.rowIndex)\" col=\"\(span.columnIndex)\" rowSpan=\"\(span.rowSpan)\" colSpan=\"\(span.columnSpan)\" border=\"\(XMLSanitizer.escape(span.borderStyle.rawValue))\"/>"
        }.joined(separator: "\n")
    }

    static func parseTable(
        attributes: [String: String],
        innerXML: String,
        rowTag: String,
        cellTag: String,
        cellTextTag: String?
    ) -> DocumentTableBlock {
        let id = attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID()
        let caption = attributes["caption"] ?? ""
        let hasHeader = (attributes["headerRow"] ?? "1") == "1"
        let borderStyle = TableCellBorderStyle(rawValue: attributes["borderStyle"] ?? "") ?? .solid

        var rows: [[String]] = []
        for trElem in XMLSanitizer.extractElements(named: rowTag, in: innerXML) {
            let rowCells = XMLSanitizer.extractElements(named: cellTag, in: trElem.innerXML).map { tcElem -> String in
                if let textTag = cellTextTag {
                    let extracted = XMLSanitizer.extractElements(named: textTag, in: tcElem.innerXML)
                    if !extracted.isEmpty {
                        return extracted.map { XMLSanitizer.unescape($0.innerXML) }.joined()
                    }
                }
                let pFallback = XMLSanitizer.extractElements(named: "text:p", in: tcElem.innerXML)
                if !pFallback.isEmpty {
                    return pFallback.map { XMLSanitizer.unescape($0.innerXML) }.joined(separator: "\n")
                }
                return XMLSanitizer.unescape(tcElem.innerXML)
            }
            rows.append(rowCells)
        }

        let cellSpans = XMLSanitizer.extractElements(named: "eo:cellSpan", in: innerXML).map { spanElem in
            TableCellSpan(
                rowIndex: Int(spanElem.attributes["row"] ?? "0") ?? 0,
                columnIndex: Int(spanElem.attributes["col"] ?? "0") ?? 0,
                rowSpan: Int(spanElem.attributes["rowSpan"] ?? "1") ?? 1,
                columnSpan: Int(spanElem.attributes["colSpan"] ?? "1") ?? 1,
                borderStyle: TableCellBorderStyle(rawValue: spanElem.attributes["border"] ?? "") ?? borderStyle
            )
        }

        return DocumentTableBlock(
            id: id, caption: caption, rows: rows, hasHeaderRow: hasHeader, cellSpans: cellSpans, borderStyle: borderStyle
        )
    }

    static func serializeExtendedMetadata(_ doc: OfficeDocumentModel, mediaPrefix: String = "word/media/") -> String {
        var tags: [String] = []
        for sec in doc.sections {
            tags.append("<eo:section id=\"\(sec.id.uuidString)\" paperSize=\"\(XMLSanitizer.escape(sec.paperSize.rawValue))\" orientation=\"\(XMLSanitizer.escape(sec.orientation.rawValue))\" cols=\"\(sec.columnCount)\" mt=\"\(sec.marginTopPt)\" mb=\"\(sec.marginBottomPt)\" ml=\"\(sec.marginLeftPt)\" mr=\"\(sec.marginRightPt)\"/>")
        }
        for fn in doc.footnotes {
            tags.append("<eo:footnote id=\"\(fn.id.uuidString)\" num=\"\(fn.number)\" endnote=\"\(fn.isEndnote ? "1" : "0")\" anchor=\"\(fn.anchorParagraphID.uuidString)\">\(XMLSanitizer.escape(fn.text))</eo:footnote>")
        }
        for rev in doc.revisions {
            tags.append("<eo:revision id=\"\(rev.id.uuidString)\" author=\"\(XMLSanitizer.escape(rev.author))\" kind=\"\(XMLSanitizer.escape(rev.kind.rawValue))\" paragraph=\"\(rev.paragraphID.uuidString)\" original=\"\(XMLSanitizer.escape(rev.originalText))\" proposed=\"\(XMLSanitizer.escape(rev.proposedText))\" ts=\"\(XMLSanitizer.escape(rev.timestampISO8601))\"/>")
        }
        for cmt in doc.comments {
            let repliesXML = cmt.replies.map { "<eo:reply>\(XMLSanitizer.escape($0))</eo:reply>" }.joined()
            tags.append("<eo:comment id=\"\(cmt.id.uuidString)\" anchor=\"\(cmt.anchorParagraphID.uuidString)\" author=\"\(XMLSanitizer.escape(cmt.author))\" message=\"\(XMLSanitizer.escape(cmt.message))\" resolved=\"\(cmt.isResolved ? "1" : "0")\">\(repliesXML)</eo:comment>")
        }
        for (name, targetID) in doc.bookmarks.sorted(by: { $0.key < $1.key }) {
            tags.append("<eo:bookmark name=\"\(XMLSanitizer.escape(name))\" target=\"\(targetID.uuidString)\"/>")
        }
        for img in doc.images {
            let safeName = sanitizeMediaFileName(img.fileName)
            let mediaPath = "\(mediaPrefix)\(safeName)"
            tags.append("<eo:image id=\"\(img.id.uuidString)\" fileName=\"\(XMLSanitizer.escape(safeName))\" path=\"\(XMLSanitizer.escape(mediaPath))\" mime=\"\(XMLSanitizer.escape(img.mimeType))\" w=\"\(img.widthPt)\" h=\"\(img.heightPt)\" caption=\"\(XMLSanitizer.escape(img.caption))\" anchor=\"\(img.anchorParagraphID?.uuidString ?? "")\"/>")
        }
        return tags.joined(separator: "\n")
    }

    static func parseExtendedMetadata(
        from xml: String,
        defaultPaperSize: PaperSizePreset,
        binaryEntries: [String: Data] = [:]
    ) -> (
        sections: [DocumentSectionLayout],
        footnotes: [FootnoteItem],
        revisions: [TrackedRevision],
        comments: [DocumentCommentThread],
        bookmarks: [String: UUID],
        images: [EmbeddedImageAttachment]
    ) {
        let sections = XMLSanitizer.extractElements(named: "eo:section", in: xml).map { elem in
            DocumentSectionLayout(
                id: elem.attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID(),
                paperSize: PaperSizePreset(rawValue: elem.attributes["paperSize"] ?? "") ?? defaultPaperSize,
                orientation: PageOrientation(rawValue: elem.attributes["orientation"] ?? "") ?? .portrait,
                columnCount: Int(elem.attributes["cols"] ?? "1") ?? 1,
                marginTopPt: Double(elem.attributes["mt"] ?? "56.7") ?? 56.7,
                marginBottomPt: Double(elem.attributes["mb"] ?? "56.7") ?? 56.7,
                marginLeftPt: Double(elem.attributes["ml"] ?? "56.7") ?? 56.7,
                marginRightPt: Double(elem.attributes["mr"] ?? "56.7") ?? 56.7
            )
        }

        let footnotes = XMLSanitizer.extractElements(named: "eo:footnote", in: xml).map { elem in
            FootnoteItem(
                id: elem.attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID(),
                number: Int(elem.attributes["num"] ?? "1") ?? 1,
                isEndnote: elem.attributes["endnote"] == "1",
                anchorParagraphID: elem.attributes["anchor"].flatMap(UUID.init(uuidString:)) ?? UUID(),
                text: XMLSanitizer.unescape(elem.innerXML)
            )
        }

        let revisions = XMLSanitizer.extractElements(named: "eo:revision", in: xml).map { elem in
            TrackedRevision(
                id: elem.attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID(),
                author: elem.attributes["author"] ?? "",
                kind: TrackedRevisionKind(rawValue: elem.attributes["kind"] ?? "") ?? .insertion,
                paragraphID: elem.attributes["paragraph"].flatMap(UUID.init(uuidString:)) ?? UUID(),
                originalText: elem.attributes["original"] ?? "",
                proposedText: elem.attributes["proposed"] ?? "",
                timestampISO8601: elem.attributes["ts"] ?? "2026-10-17T00:00:00Z"
            )
        }

        let comments = XMLSanitizer.extractElements(named: "eo:comment", in: xml).map { elem in
            let replies = XMLSanitizer.extractElements(named: "eo:reply", in: elem.innerXML).map { XMLSanitizer.unescape($0.innerXML) }
            return DocumentCommentThread(
                id: elem.attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID(),
                anchorParagraphID: elem.attributes["anchor"].flatMap(UUID.init(uuidString:)) ?? UUID(),
                author: elem.attributes["author"] ?? "",
                message: elem.attributes["message"] ?? "",
                replies: replies,
                isResolved: elem.attributes["resolved"] == "1"
            )
        }

        var bookmarks: [String: UUID] = [:]
        for elem in XMLSanitizer.extractElements(named: "eo:bookmark", in: xml) {
            if let name = elem.attributes["name"],
               let target = elem.attributes["target"].flatMap(UUID.init(uuidString:)) {
                bookmarks[name] = target
            }
        }

        let images = XMLSanitizer.extractElements(named: "eo:image", in: xml).compactMap { elem -> EmbeddedImageAttachment? in
            let fileName = elem.attributes["fileName"] ?? "image.png"
            let path = elem.attributes["path"] ?? ""
            let data = binaryEntries[path] ?? binaryEntries["word/media/\(fileName)"] ?? binaryEntries["Pictures/\(fileName)"] ?? binaryEntries["BinData/\(fileName)"] ?? Data()
            let anchorStr = elem.attributes["anchor"] ?? ""
            return EmbeddedImageAttachment(
                id: elem.attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID(),
                fileName: fileName, caption: elem.attributes["caption"] ?? "",
                mimeType: elem.attributes["mime"] ?? "image/png", imageData: data,
                widthPt: Double(elem.attributes["w"] ?? "320.0") ?? 320.0,
                heightPt: Double(elem.attributes["h"] ?? "200.0") ?? 200.0,
                anchorParagraphID: anchorStr.isEmpty ? nil : UUID(uuidString: anchorStr)
            )
        }

        return (
            sections: sections.isEmpty ? [DocumentSectionLayout(paperSize: defaultPaperSize)] : sections,
            footnotes: footnotes, revisions: revisions, comments: comments, bookmarks: bookmarks, images: images
        )
    }
}

// MARK: - OOXML Container Codec (ECMA-376 DOCX, XLSX, PPTX)

/// ECMA-376 / ISO/IEC 29500 OOXML container encoder and decoder for `.docx`, `.xlsx`, and `.pptx`.
public enum OOXMLContainerCodec {

    // MARK: DOCX (WordprocessingML)

    public static func encodeDocument(_ doc: OfficeDocumentModel) throws -> Data {
        let contentTypesXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
          <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
          <Default Extension="xml" ContentType="application/xml"/>
          <Default Extension="png" ContentType="image/png"/>
          <Default Extension="jpg" ContentType="image/jpeg"/>
          <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
          <Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
        </Types>
        """

        let relsXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
          <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
          <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
        </Relationships>
        """

        let extendedMetaXML = XMLDocumentModelMapper.serializeExtendedMetadata(doc, mediaPrefix: "word/media/")
        let coreXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:eo="urn:easyoffice:metadata:1.0">
          <dc:title>\(XMLSanitizer.escape(doc.title))</dc:title>
          <cp:headerText>\(XMLSanitizer.escape(doc.headerText))</cp:headerText>
          <cp:footerText>\(XMLSanitizer.escape(doc.footerText))</cp:footerText>
          <cp:paperSize>\(XMLSanitizer.escape(doc.paperSize.rawValue))</cp:paperSize>
          <cp:trackChanges>\(doc.trackChangesEnabled ? "true" : "false")</cp:trackChanges>
          \(extendedMetaXML)
        </cp:coreProperties>
        """

        var bodyItems: [String] = []
        for p in doc.paragraphs {
            let pAttrs = XMLDocumentModelMapper.paragraphAttributes(p)
            let styleVal = XMLDocumentModelMapper.ooxmlStyleVal(for: p.style)
            let jcVal = XMLDocumentModelMapper.ooxmlJustificationVal(for: p.alignment)

            let runsXML = p.effectiveRuns.map { run -> String in
                let rAttrs = XMLDocumentModelMapper.runAttributes(run)
                let resolvedFont = run.fontName ?? p.fontName
                let resolvedSize = run.fontSizePt ?? p.fontSizePt
                let halfPtSize = Int((resolvedSize * 2.0).rounded())
                var rPrTags = "<w:rFonts w:ascii=\"\(XMLSanitizer.escape(resolvedFont))\"/>"
                rPrTags += "<w:sz w:val=\"\(halfPtSize)\"/>"
                if run.isBold { rPrTags += "<w:b/>" }
                if run.isItalic { rPrTags += "<w:i/>" }
                if run.isUnderlined { rPrTags += "<w:u w:val=\"single\"/>" }
                if let fg = run.foregroundHex {
                    let cleanHex = fg.hasPrefix("#") ? String(fg.dropFirst()) : fg
                    rPrTags += "<w:color w:val=\"\(XMLSanitizer.escape(cleanHex))\"/>"
                }
                return "      <w:r \(rAttrs)><w:rPr>\(rPrTags)</w:rPr><w:t xml:space=\"preserve\">\(XMLSanitizer.escape(run.text))</w:t></w:r>"
            }.joined(separator: "\n")

            bodyItems.append("""
                <w:p \(pAttrs)>
                  <w:pPr><w:pStyle w:val="\(styleVal)"/><w:jc w:val="\(jcVal)"/></w:pPr>
            \(runsXML)
                </w:p>
            """)
        }

        for tbl in doc.tables {
            let tAttrs = XMLDocumentModelMapper.tableAttributes(tbl)
            let spansXML = XMLDocumentModelMapper.serializeTableSpans(tbl)
            let rowStrings = tbl.rows.map { row -> String in
                let cells = row.map { "<w:tc><w:p><w:r><w:t>\(XMLSanitizer.escape($0))</w:t></w:r></w:p></w:tc>" }.joined()
                return "<w:tr>\(cells)</w:tr>"
            }.joined(separator: "\n      ")

            bodyItems.append("""
                <w:tbl \(tAttrs)>
                  \(spansXML)
                  \(rowStrings)
                </w:tbl>
            """)
        }

        let documentXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:eo="urn:easyoffice:metadata:1.0">
          <w:body>
        \(bodyItems.joined(separator: "\n"))
          </w:body>
        </w:document>
        """

        var entries: [ZipContainerEntry] = [
            ZipContainerEntry(path: "[Content_Types].xml", data: Data(contentTypesXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "_rels/.rels", data: Data(relsXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "docProps/core.xml", data: Data(coreXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "word/document.xml", data: Data(documentXML.utf8), compressionMethod: .deflate)
        ]
        for img in doc.images {
            let safeName = XMLDocumentModelMapper.sanitizeMediaFileName(img.fileName)
            entries.append(ZipContainerEntry(path: "word/media/\(safeName)", data: img.imageData, compressionMethod: .store))
        }
        return try ZipContainerCodec.pack(entries: entries)
    }

    public static func decodeDocument(from data: Data) throws -> OfficeDocumentModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        var entryMap: [String: String] = [:]
        var binaryEntries: [String: Data] = [:]
        for entry in entries {
            if XMLDocumentModelMapper.isBinaryMediaPath(entry.path) {
                binaryEntries[entry.path] = entry.data
            } else {
                entryMap[entry.path] = try XMLSanitizer.validateSafeXMLData(entry.data)
            }
        }
        guard let documentXML = entryMap["word/document.xml"] else {
            throw EasyOfficeError.corruptedContainer(detail: "OOXML DOCX 필수 스트림 'word/document.xml'이 누락되었습니다.")
        }

        let coreXML = entryMap["docProps/core.xml"] ?? ""
        let title = XMLSanitizer.extractElements(named: "dc:title", in: coreXML).first.map { XMLSanitizer.unescape($0.innerXML) } ?? "제목 없음"
        let headerText = XMLSanitizer.extractElements(named: "cp:headerText", in: coreXML).first.map { XMLSanitizer.unescape($0.innerXML) } ?? ""
        let footerText = XMLSanitizer.extractElements(named: "cp:footerText", in: coreXML).first.map { XMLSanitizer.unescape($0.innerXML) } ?? ""
        let paperRaw = XMLSanitizer.extractElements(named: "cp:paperSize", in: coreXML).first.map { XMLSanitizer.unescape($0.innerXML) } ?? ""
        let paperSize = PaperSizePreset(rawValue: paperRaw) ?? .a4
        let trackChanges = XMLSanitizer.extractElements(named: "cp:trackChanges", in: coreXML).first?.innerXML == "true"

        let tables = XMLSanitizer.extractElements(named: "w:tbl", in: documentXML).map { tblElem in
            XMLDocumentModelMapper.parseTable(
                attributes: tblElem.attributes, innerXML: tblElem.innerXML,
                rowTag: "w:tr", cellTag: "w:tc", cellTextTag: "w:t"
            )
        }

        let bodyWithoutTables = XMLSanitizer.stripElements(named: "w:tbl", from: documentXML)
        let paragraphs = XMLSanitizer.extractElements(named: "w:p", in: bodyWithoutTables).map { pElem in
            XMLDocumentModelMapper.parseParagraph(attributes: pElem.attributes, innerXML: pElem.innerXML, childTextTag: "w:t")
        }

        let meta = XMLDocumentModelMapper.parseExtendedMetadata(from: coreXML, defaultPaperSize: paperSize, binaryEntries: binaryEntries)

        return OfficeDocumentModel(
            title: title, format: .docx, paperSize: paperSize,
            headerText: headerText, footerText: footerText,
            paragraphs: paragraphs, tables: tables, trackChangesEnabled: trackChanges,
            sections: meta.sections, footnotes: meta.footnotes, revisions: meta.revisions,
            comments: meta.comments, bookmarks: meta.bookmarks, images: meta.images
        )
    }

    // MARK: XLSX (SpreadsheetML)

    public static func encodeWorksheet(_ sheet: WorksheetModel) throws -> Data {
        let orderedPages = sheet.orderedSheetPages()
        var sharedStrings: [String] = []
        var sharedStringIndex: [String: Int] = [:]
        var totalStringOccurrences = 0

        for page in orderedPages {
            for coord in page.cellsByCoordinate.keys.sorted() {
                guard let raw = page.cellsByCoordinate[coord], !raw.isEmpty else { continue }
                let isFiniteNumber = (Double(raw)?.isFinite == true)
                if !raw.hasPrefix("="), !isFiniteNumber {
                    totalStringOccurrences += 1
                    if sharedStringIndex[raw] == nil {
                        sharedStringIndex[raw] = sharedStrings.count
                        sharedStrings.append(raw)
                    }
                }
            }
        }

        let sheetOverrides = orderedPages.indices.map { idx in
            "  <Override PartName=\"/xl/worksheets/sheet\(idx + 1).xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
        }.joined(separator: "\n")

        let contentTypesXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
          <Default Extension="xml" ContentType="application/xml"/>
          <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
        \(sheetOverrides)
          <Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>
        </Types>
        """

        let sheetEntriesXML = orderedPages.enumerated().map { idx, page in
            "    <sheet name=\"\(XMLSanitizer.escape(page.name))\" sheetId=\"\(idx + 1)\" rId=\"rId\(idx + 1)\"/>"
        }.joined(separator: "\n")

        let metaXML = XMLDocumentModelMapper.serializeWorksheetMetadata(sheet)
        let workbookXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:eo="urn:easyoffice:metadata:1.0">
          \(metaXML)
          <sheets>
        \(sheetEntriesXML)
          </sheets>
        </workbook>
        """

        let siElements = sharedStrings.map { "<si><t xml:space=\"preserve\">\(XMLSanitizer.escape($0))</t></si>" }.joined(separator: "\n  ")
        let sharedStringsXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="\(totalStringOccurrences)" uniqueCount="\(sharedStrings.count)">
          \(siElements)
        </sst>
        """

        var entries: [ZipContainerEntry] = [
            ZipContainerEntry(path: "[Content_Types].xml", data: Data(contentTypesXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "xl/workbook.xml", data: Data(workbookXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "xl/sharedStrings.xml", data: Data(sharedStringsXML.utf8), compressionMethod: .deflate)
        ]

        for (idx, page) in orderedPages.enumerated() {
            let allCoords = Set(page.cellsByCoordinate.keys).union(page.cellStyles.keys).sorted()
            var rowsMap: [Int: [PackedCellCoordinate]] = [:]
            for coord in allCoords {
                rowsMap[coord.row + 1, default: []].append(coord)
            }

            var rowElements: [String] = []
            for rowNum in rowsMap.keys.sorted() {
                guard let coords = rowsMap[rowNum] else { continue }
                var cellTags: [String] = []
                for coord in coords {
                    let a1 = coord.description
                    let raw = page.cellsByCoordinate[coord] ?? ""
                    let styleAttrs = XMLDocumentModelMapper.cellStyleAttributes(page.cellStyles[coord])
                    if raw.hasPrefix("=") {
                        let formulaBody = String(raw.dropFirst())
                        cellTags.append("      <c r=\"\(XMLSanitizer.escape(a1))\" t=\"str\"\(styleAttrs)><f>\(XMLSanitizer.escape(formulaBody))</f></c>")
                    } else if !raw.isEmpty, Double(raw)?.isFinite == true {
                        cellTags.append("      <c r=\"\(XMLSanitizer.escape(a1))\"\(styleAttrs)><v>\(XMLSanitizer.escape(raw))</v></c>")
                    } else if !raw.isEmpty, let sstIdx = sharedStringIndex[raw] {
                        cellTags.append("      <c r=\"\(XMLSanitizer.escape(a1))\" t=\"s\"\(styleAttrs)><v>\(sstIdx)</v></c>")
                    } else if !styleAttrs.isEmpty {
                        cellTags.append("      <c r=\"\(XMLSanitizer.escape(a1))\"\(styleAttrs)/>")
                    }
                }
                rowElements.append("""
                    <row r="\(rowNum)">
                \(cellTags.joined(separator: "\n"))
                    </row>
                """)
            }

            let mergeCellsXML: String
            if page.mergedRanges.isEmpty {
                mergeCellsXML = ""
            } else {
                let items = page.mergedRanges.map { "    <mergeCell ref=\"\(XMLSanitizer.escape($0.description))\"/>" }.joined(separator: "\n")
                mergeCellsXML = "\n  <mergeCells count=\"\(page.mergedRanges.count)\">\n\(items)\n  </mergeCells>"
            }

            let sheetXML = """
            <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
            <worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
              <sheetData>
            \(rowElements.joined(separator: "\n"))
              </sheetData>\(mergeCellsXML)
            </worksheet>
            """
            entries.append(ZipContainerEntry(
                path: "xl/worksheets/sheet\(idx + 1).xml",
                data: Data(sheetXML.utf8),
                compressionMethod: .deflate
            ))
        }

        return try ZipContainerCodec.pack(entries: entries)
    }

    public static func decodeWorksheet(from data: Data) throws -> WorksheetModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        var entryMap: [String: String] = [:]
        for entry in entries where !XMLDocumentModelMapper.isBinaryMediaPath(entry.path) {
            entryMap[entry.path] = try XMLSanitizer.validateSafeXMLData(entry.data)
        }
        guard entryMap["xl/worksheets/sheet1.xml"] != nil else {
            throw EasyOfficeError.corruptedContainer(detail: "OOXML XLSX 필수 스트림 'xl/worksheets/sheet1.xml'이 누락되었습니다.")
        }

        var sharedStrings: [String] = []
        if let sstXML = entryMap["xl/sharedStrings.xml"] {
            for siElem in XMLSanitizer.extractElements(named: "si", in: sstXML) {
                let textParts = XMLSanitizer.extractElements(named: "t", in: siElem.innerXML).map { XMLSanitizer.unescape($0.innerXML) }
                sharedStrings.append(textParts.joined())
            }
        }

        let workbookXML = entryMap["xl/workbook.xml"] ?? ""
        let sheetTags = XMLSanitizer.extractElements(named: "sheet", in: workbookXML)
        let sheetDescriptors: [(name: String, path: String)] = sheetTags.isEmpty
            ? [("Sheet1", "xl/worksheets/sheet1.xml")]
            : sheetTags.enumerated().map { idx, elem in
                (elem.attributes["name"] ?? "Sheet\(idx + 1)", "xl/worksheets/sheet\(idx + 1).xml")
            }

        func decodeSheetPage(name: String, xml: String) throws -> WorksheetModel {
            var pageModel = WorksheetModel(name: name)
            for cellElem in XMLSanitizer.extractElements(named: "c", in: xml) {
                guard let addr = cellElem.attributes["r"] else { continue }
                let cellType = cellElem.attributes["t"] ?? ""

                if let style = XMLDocumentModelMapper.parseCellStyle(from: cellElem.attributes) {
                    try pageModel.setCellStyle(style, at: addr)
                }

                if let fElem = XMLSanitizer.extractElements(named: "f", in: cellElem.innerXML).first {
                    let formulaText = XMLSanitizer.unescape(fElem.innerXML)
                    try pageModel.setCell(addr, rawValue: "=\(formulaText)")
                } else if cellType == "s" {
                    guard let vElem = XMLSanitizer.extractElements(named: "v", in: cellElem.innerXML).first,
                          let sstIndex = Int(vElem.innerXML.trimmingCharacters(in: .whitespacesAndNewlines)),
                          sstIndex >= 0, sstIndex < sharedStrings.count else {
                        throw EasyOfficeError.corruptedContainer(detail: "XLSX sharedStrings 인덱스 참조가 유효하지 않습니다 (cell: \(addr)).")
                    }
                    try pageModel.setCell(addr, rawValue: sharedStrings[sstIndex])
                } else if cellType == "inlineStr" {
                    let tElems = XMLSanitizer.extractElements(named: "t", in: cellElem.innerXML)
                    if !tElems.isEmpty {
                        try pageModel.setCell(addr, rawValue: tElems.map { XMLSanitizer.unescape($0.innerXML) }.joined())
                    } else if let vElem = XMLSanitizer.extractElements(named: "v", in: cellElem.innerXML).first {
                        try pageModel.setCell(addr, rawValue: XMLSanitizer.unescape(vElem.innerXML))
                    }
                } else if cellType == "b" {
                    if let vElem = XMLSanitizer.extractElements(named: "v", in: cellElem.innerXML).first {
                        let trimmed = vElem.innerXML.trimmingCharacters(in: .whitespacesAndNewlines)
                        let boolStr = (trimmed == "1" || trimmed.caseInsensitiveCompare("TRUE") == .orderedSame) ? "TRUE" : "FALSE"
                        try pageModel.setCell(addr, rawValue: boolStr)
                    }
                } else if let vElem = XMLSanitizer.extractElements(named: "v", in: cellElem.innerXML).first {
                    try pageModel.setCell(addr, rawValue: XMLSanitizer.unescape(vElem.innerXML))
                }
            }
            for mergeElem in XMLSanitizer.extractElements(named: "mergeCell", in: xml) {
                if let ref = mergeElem.attributes["ref"], let range = try? CellRange(notation: ref) {
                    if !pageModel.mergedRanges.contains(range) {
                        pageModel.mergedRanges.append(range)
                    }
                }
            }
            return pageModel
        }

        var decodedPages: [WorksheetModel] = []
        for desc in sheetDescriptors {
            if let xml = entryMap[desc.path] {
                decodedPages.append(try decodeSheetPage(name: desc.name, xml: xml))
            }
        }
        guard var primary = decodedPages.first else {
            throw EasyOfficeError.corruptedContainer(detail: "OOXML XLSX 워크시트를 디코딩할 수 없습니다.")
        }
        if decodedPages.count > 1 {
            primary.additionalSheets = decodedPages.dropFirst().map { page in
                WorksheetSheetPage(
                    name: page.name,
                    cellsByCoordinate: page.cellsByCoordinate,
                    cellStyles: page.cellStyles,
                    sheetComments: page.sheetComments,
                    mergedRanges: page.mergedRanges
                )
            }
        }
        XMLDocumentModelMapper.applyWorksheetMetadata(from: workbookXML, to: &primary)
        return primary
    }

    // MARK: PPTX (PresentationML)

    public static func encodePresentation(_ deck: PresentationDeckModel) throws -> Data {
        var entries: [ZipContainerEntry] = []

        var overrides: [String] = [
            "  <Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/>"
        ]
        for (idx, slide) in deck.slides.enumerated() {
            overrides.append("  <Override PartName=\"/ppt/slides/slide\(idx + 1).xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>")
            if !slide.speakerNotes.isEmpty {
                overrides.append("  <Override PartName=\"/ppt/notesSlides/notesSlide\(idx + 1).xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml\"/>")
            }
        }

        let contentTypesXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
          <Default Extension="xml" ContentType="application/xml"/>
        \(overrides.joined(separator: "\n"))
        </Types>
        """
        entries.append(ZipContainerEntry(path: "[Content_Types].xml", data: Data(contentTypesXML.utf8), compressionMethod: .deflate))

        let slideRefs = deck.slides.indices.map { "    <p:sldId id=\"\(256 + $0)\" target=\"ppt/slides/slide\($0 + 1).xml\"/>" }
        let presentationXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" title="\(XMLSanitizer.escape(deck.title))" aspectRatio="\(XMLSanitizer.escape(deck.aspectRatio.rawValue))">
          <p:sldIdLst>
        \(slideRefs.joined(separator: "\n"))
          </p:sldIdLst>
        </p:presentation>
        """
        entries.append(ZipContainerEntry(path: "ppt/presentation.xml", data: Data(presentationXML.utf8), compressionMethod: .deflate))

        for (index, slide) in deck.slides.enumerated() {
            let bulletsXML = slide.bulletPoints.map { "<p:bullet>\(XMLSanitizer.escape($0))</p:bullet>" }.joined(separator: "\n    ")
            let elementsXML = slide.elements.enumerated().map { elIdx, el in
                let attrs = XMLDocumentModelMapper.slideElementAttributes(el)
                let rPrAttrs = "b=\"\(el.isBold ? "1" : "0")\" i=\"\(el.isItalic ? "1" : "0")\" u=\"\(el.isUnderlined ? "sng" : "none")\""
                return """
                    <p:sp \(attrs)>
                      <p:nvSpPr><p:cNvPr id="\(elIdx + 2)" name="\(XMLSanitizer.escape(el.title))"/></p:nvSpPr>
                      <p:spPr><a:xfrm><a:off x="\(Int(el.x.rounded()))" y="\(Int(el.y.rounded()))"/><a:ext cx="\(Int(el.width.rounded()))" cy="\(Int(el.height.rounded()))"/></a:xfrm></p:spPr>
                      <p:txBody><a:bodyPr/><a:p><a:r><a:rPr \(rPrAttrs)/><a:t>\(XMLSanitizer.escape(el.bodyText))</a:t></a:r></a:p></p:txBody>
                    </p:sp>
                """
            }.joined(separator: "\n")

            let slideXML = """
            <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
            <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" id="\(slide.id.uuidString)" title="\(XMLSanitizer.escape(slide.title))" subtitle="\(XMLSanitizer.escape(slide.subtitle))" layout="\(XMLSanitizer.escape(slide.masterLayout.rawValue))" transition="\(XMLSanitizer.escape(slide.transition.rawValue))">
              <p:notes>\(XMLSanitizer.escape(slide.speakerNotes))</p:notes>
              <p:bullets>
                \(bulletsXML)
              </p:bullets>
              <p:cSld>
                <p:spTree>
            \(elementsXML)
                </p:spTree>
              </p:cSld>
            </p:sld>
            """
            entries.append(ZipContainerEntry(path: "ppt/slides/slide\(index + 1).xml", data: Data(slideXML.utf8), compressionMethod: .deflate))

            if !slide.speakerNotes.isEmpty {
                let notesSlideXML = """
                <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
                <p:notes xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
                  <p:cSld><p:spTree><p:sp><p:txBody><a:bodyPr/><a:p><a:r><a:t>\(XMLSanitizer.escape(slide.speakerNotes))</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld>
                </p:notes>
                """
                entries.append(ZipContainerEntry(path: "ppt/notesSlides/notesSlide\(index + 1).xml", data: Data(notesSlideXML.utf8), compressionMethod: .deflate))
            }
        }

        return try ZipContainerCodec.pack(entries: entries)
    }

    public static func decodePresentation(from data: Data) throws -> PresentationDeckModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        var entryMap: [String: String] = [:]
        for entry in entries where !XMLDocumentModelMapper.isBinaryMediaPath(entry.path) {
            entryMap[entry.path] = try XMLSanitizer.validateSafeXMLData(entry.data)
        }
        guard let presentationXML = entryMap["ppt/presentation.xml"] else {
            throw EasyOfficeError.corruptedContainer(detail: "OOXML PPTX 필수 스트림 'ppt/presentation.xml'이 누락되었습니다.")
        }

        let presRoot = XMLSanitizer.extractElements(named: "p:presentation", in: presentationXML).first
        let title = presRoot?.attributes["title"] ?? "발표자료"
        let aspectRatio = SlideAspectRatio(rawValue: presRoot?.attributes["aspectRatio"] ?? "") ?? .widescreen16x9
        let slideRefs = XMLSanitizer.extractElements(named: "p:sldId", in: presentationXML)

        var slides: [SlideModel] = []
        for (idx, ref) in slideRefs.enumerated() {
            let target = ref.attributes["target"] ?? "ppt/slides/slide\(idx + 1).xml"
            guard let slideXML = entryMap[target] else {
                throw EasyOfficeError.corruptedContainer(detail: "PPTX 슬라이드 파트 누락: \(target)")
            }
            guard let sldElem = XMLSanitizer.extractElements(named: "p:sld", in: slideXML).first else {
                continue
            }
            let slideID = sldElem.attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID()
            let elements = XMLSanitizer.extractElements(named: "p:sp", in: sldElem.innerXML).map { sp in
                XMLDocumentModelMapper.parseSlideElement(attributes: sp.attributes, innerXML: sp.innerXML)
            }
            let sTitle = sldElem.attributes["title"] ?? elements.first?.title ?? ""
            let sSubtitle = sldElem.attributes["subtitle"] ?? ""
            let layout = SlideMasterLayout(rawValue: sldElem.attributes["layout"] ?? "") ?? .titleAndContent
            let transition = SlideTransitionKind(rawValue: sldElem.attributes["transition"] ?? "") ?? .fade

            var notes = XMLSanitizer.extractElements(named: "p:notes", in: sldElem.innerXML).first.map { XMLSanitizer.unescape($0.innerXML) } ?? ""
            if notes.isEmpty, let notesSlideXML = entryMap["ppt/notesSlides/notesSlide\(idx + 1).xml"] {
                let noteTexts = XMLSanitizer.extractElements(named: "a:t", in: notesSlideXML).map { XMLSanitizer.unescape($0.innerXML) }
                notes = noteTexts.joined(separator: "\n")
            }

            let bullets = XMLSanitizer.extractElements(named: "p:bullet", in: sldElem.innerXML).map { XMLSanitizer.unescape($0.innerXML) }

            slides.append(SlideModel(
                id: slideID, title: sTitle, subtitle: sSubtitle, bulletPoints: bullets,
                speakerNotes: notes, masterLayout: layout, transition: transition, elements: elements
            ))
        }

        return PresentationDeckModel(title: title, aspectRatio: aspectRatio, slides: slides)
    }
}
