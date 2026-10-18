import Foundation

// MARK: - Shared XML Document & Presentation Model Mapper

/// Shared mapper eliminating duplicate paragraph, table, slide, and metadata XML serialization across OOXML, ODF, and HWPX.
internal enum XMLDocumentModelMapper {

    static func paragraphAttributes(_ p: ParagraphBlock) -> String {
        let boldAttr = p.isBold ? "1" : "0"
        let italicAttr = p.isItalic ? "1" : "0"
        let underlineAttr = p.isUnderlined ? "1" : "0"
        return "id=\"\(p.id.uuidString)\" style=\"\(XMLSanitizer.escape(p.style.rawValue))\" align=\"\(XMLSanitizer.escape(p.alignment.rawValue))\" font=\"\(XMLSanitizer.escape(p.fontName))\" size=\"\(p.fontSizePt)\" bold=\"\(boldAttr)\" italic=\"\(italicAttr)\" underline=\"\(underlineAttr)\" lineSpacing=\"\(p.lineSpacingPercent)\""
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

    static func parseParagraph(
        attributes: [String: String],
        innerXML: String,
        childTextTag: String?
    ) -> ParagraphBlock {
        let id = attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID()
        let extractedText: String
        if let childTag = childTextTag {
            let runs = XMLSanitizer.extractElements(named: childTag, in: innerXML)
            if !runs.isEmpty {
                extractedText = runs.map { XMLSanitizer.unescape($0.innerXML) }.joined()
            } else {
                extractedText = XMLSanitizer.unescape(innerXML)
            }
        } else {
            extractedText = XMLSanitizer.unescape(innerXML)
        }

        if let styleRaw = attributes["style"] {
            return ParagraphBlock(
                id: id,
                style: ParagraphStyleRole(rawValue: styleRaw) ?? roleFromOOXMLStyle(styleRaw),
                alignment: TextAlignmentMode(rawValue: attributes["align"] ?? "") ?? alignmentFromOOXMLJustification(attributes["align"] ?? ""),
                text: extractedText,
                fontName: attributes["font"] ?? "Pretendard",
                fontSizePt: Double(attributes["size"] ?? "11.0") ?? 11.0,
                isBold: attributes["bold"] == "1",
                isItalic: attributes["italic"] == "1",
                isUnderlined: attributes["underline"] == "1",
                lineSpacingPercent: Double(attributes["lineSpacing"] ?? "160.0") ?? 160.0
            )
        }

        // Standard external WordprocessingML (<w:pPr> / <w:rPr>) fallback
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

        var fontName = "Pretendard"
        var fontSizePt = 11.0
        var isBold = false
        var isItalic = false
        var isUnderlined = false

        for rPrElem in XMLSanitizer.extractElements(named: "w:rPr", in: innerXML) {
            let rPr = rPrElem.innerXML
            if let fontAttrs = XMLSanitizer.extractElements(named: "w:rFonts", in: rPr).first?.attributes,
               let asciiFont = fontAttrs["w:ascii"] ?? fontAttrs["w:eastAsia"] ?? fontAttrs["ascii"] {
                fontName = asciiFont
            }
            if let bElem = XMLSanitizer.extractElements(named: "w:b", in: rPr).first {
                let val = (bElem.attributes["w:val"] ?? bElem.attributes["val"] ?? "1").lowercased()
                isBold = val != "0" && val != "false"
            }
            if let iElem = XMLSanitizer.extractElements(named: "w:i", in: rPr).first {
                let val = (iElem.attributes["w:val"] ?? iElem.attributes["val"] ?? "1").lowercased()
                isItalic = val != "0" && val != "false"
            }
            if let uElem = XMLSanitizer.extractElements(named: "w:u", in: rPr).first {
                let val = (uElem.attributes["w:val"] ?? uElem.attributes["val"] ?? "single").lowercased()
                isUnderlined = val != "none" && val != "0" && val != "false"
            }
            if let szAttrs = XMLSanitizer.extractElements(named: "w:sz", in: rPr).first?.attributes,
               let valStr = szAttrs["w:val"] ?? szAttrs["val"],
               let halfPts = Double(valStr) {
                fontSizePt = halfPts / 2.0
            }
        }

        return ParagraphBlock(
            id: id,
            style: style,
            alignment: alignment,
            text: extractedText,
            fontName: fontName,
            fontSizePt: fontSizePt,
            isBold: isBold,
            isItalic: isItalic,
            isUnderlined: isUnderlined,
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
            id: id,
            caption: caption,
            rows: rows,
            hasHeaderRow: hasHeader,
            cellSpans: cellSpans,
            borderStyle: borderStyle
        )
    }

    static func serializeExtendedMetadata(_ doc: OfficeDocumentModel) -> String {
        var tags: [String] = []
        for sec in doc.sections {
            tags.append(
                "<eo:section id=\"\(sec.id.uuidString)\" paperSize=\"\(XMLSanitizer.escape(sec.paperSize.rawValue))\" orientation=\"\(XMLSanitizer.escape(sec.orientation.rawValue))\" cols=\"\(sec.columnCount)\" mt=\"\(sec.marginTopPt)\" mb=\"\(sec.marginBottomPt)\" ml=\"\(sec.marginLeftPt)\" mr=\"\(sec.marginRightPt)\"/>"
            )
        }
        for fn in doc.footnotes {
            tags.append(
                "<eo:footnote id=\"\(fn.id.uuidString)\" num=\"\(fn.number)\" endnote=\"\(fn.isEndnote ? "1" : "0")\" anchor=\"\(fn.anchorParagraphID.uuidString)\">\(XMLSanitizer.escape(fn.text))</eo:footnote>"
            )
        }
        for rev in doc.revisions {
            tags.append(
                "<eo:revision id=\"\(rev.id.uuidString)\" author=\"\(XMLSanitizer.escape(rev.author))\" kind=\"\(XMLSanitizer.escape(rev.kind.rawValue))\" paragraph=\"\(rev.paragraphID.uuidString)\" original=\"\(XMLSanitizer.escape(rev.originalText))\" proposed=\"\(XMLSanitizer.escape(rev.proposedText))\" ts=\"\(XMLSanitizer.escape(rev.timestampISO8601))\"/>"
            )
        }
        for cmt in doc.comments {
            let repliesXML = cmt.replies.map { "<eo:reply>\(XMLSanitizer.escape($0))</eo:reply>" }.joined()
            tags.append(
                "<eo:comment id=\"\(cmt.id.uuidString)\" anchor=\"\(cmt.anchorParagraphID.uuidString)\" author=\"\(XMLSanitizer.escape(cmt.author))\" message=\"\(XMLSanitizer.escape(cmt.message))\" resolved=\"\(cmt.isResolved ? "1" : "0")\">\(repliesXML)</eo:comment>"
            )
        }
        for (name, targetID) in doc.bookmarks.sorted(by: { $0.key < $1.key }) {
            tags.append(
                "<eo:bookmark name=\"\(XMLSanitizer.escape(name))\" target=\"\(targetID.uuidString)\"/>"
            )
        }
        return tags.joined(separator: "\n")
    }

    static func parseExtendedMetadata(
        from xml: String,
        defaultPaperSize: PaperSizePreset
    ) -> (
        sections: [DocumentSectionLayout],
        footnotes: [FootnoteItem],
        revisions: [TrackedRevision],
        comments: [DocumentCommentThread],
        bookmarks: [String: UUID]
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
            let replies = XMLSanitizer.extractElements(named: "eo:reply", in: elem.innerXML).map {
                XMLSanitizer.unescape($0.innerXML)
            }
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

        return (
            sections: sections.isEmpty ? [DocumentSectionLayout(paperSize: defaultPaperSize)] : sections,
            footnotes: footnotes,
            revisions: revisions,
            comments: comments,
            bookmarks: bookmarks
        )
    }

    static func slideElementAttributes(_ el: SlideCanvasElement) -> String {
        "id=\"\(el.id.uuidString)\" title=\"\(XMLSanitizer.escape(el.title))\" x=\"\(el.x)\" y=\"\(el.y)\" w=\"\(el.width)\" h=\"\(el.height)\" kind=\"\(XMLSanitizer.escape(el.kind.rawValue))\" zIndex=\"\(el.zIndex)\" rotation=\"\(el.rotationDegrees)\" fillHex=\"\(XMLSanitizer.escape(el.fillHex))\" strokeHex=\"\(XMLSanitizer.escape(el.strokeHex))\""
    }

    static func parseSlideElement(attributes: [String: String], innerXML: String) -> SlideCanvasElement {
        let id = attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID()
        let cNvPrAttrs = XMLSanitizer.extractElements(named: "p:cNvPr", in: innerXML).first?.attributes
        let offAttrs = XMLSanitizer.extractElements(named: "a:off", in: innerXML).first?.attributes
        let extAttrs = XMLSanitizer.extractElements(named: "a:ext", in: innerXML).first?.attributes

        let title = attributes["title"] ?? cNvPrAttrs?["name"] ?? ""
        let x = Double(attributes["x"] ?? offAttrs?["x"] ?? "0") ?? 0
        let y = Double(attributes["y"] ?? offAttrs?["y"] ?? "0") ?? 0
        let width = Double(attributes["w"] ?? extAttrs?["cx"] ?? "100") ?? 100
        let height = Double(attributes["h"] ?? extAttrs?["cy"] ?? "60") ?? 60
        let kind = SlideElementKind(rawValue: attributes["kind"] ?? "") ?? .textBox
        let zIndex = Int(attributes["zIndex"] ?? "0") ?? 0
        let rotation = Double(attributes["rotation"] ?? "0") ?? 0
        let fillHex = attributes["fillHex"] ?? "#EEF0FF"
        let strokeHex = attributes["strokeHex"] ?? "#5C6BC0"

        let drawingTextRuns = XMLSanitizer.extractElements(named: "a:t", in: innerXML)
        let bodyText: String
        if !drawingTextRuns.isEmpty {
            bodyText = drawingTextRuns.map { XMLSanitizer.unescape($0.innerXML) }.joined()
        } else {
            bodyText = XMLSanitizer.unescape(innerXML)
        }

        return SlideCanvasElement(
            id: id,
            title: title,
            bodyText: bodyText,
            x: x,
            y: y,
            width: width,
            height: height,
            kind: kind,
            zIndex: zIndex,
            rotationDegrees: rotation,
            fillHex: fillHex,
            strokeHex: strokeHex
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

        let extendedMetaXML = XMLDocumentModelMapper.serializeExtendedMetadata(doc)
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
            let halfPtSize = Int((p.fontSizePt * 2.0).rounded())
            var rPrTags = "<w:rFonts w:ascii=\"\(XMLSanitizer.escape(p.fontName))\"/>"
            rPrTags += "<w:sz w:val=\"\(halfPtSize)\"/>"
            if p.isBold { rPrTags += "<w:b/>" }
            if p.isItalic { rPrTags += "<w:i/>" }
            if p.isUnderlined { rPrTags += "<w:u w:val=\"single\"/>" }

            let pXML = """
                <w:p \(pAttrs)>
                  <w:pPr><w:pStyle w:val="\(styleVal)"/><w:jc w:val="\(jcVal)"/></w:pPr>
                  <w:r><w:rPr>\(rPrTags)</w:rPr><w:t xml:space="preserve">\(XMLSanitizer.escape(p.text))</w:t></w:r>
                </w:p>
            """
            bodyItems.append(pXML)
        }

        for tbl in doc.tables {
            let tAttrs = XMLDocumentModelMapper.tableAttributes(tbl)
            let spansXML = XMLDocumentModelMapper.serializeTableSpans(tbl)
            let rowStrings = tbl.rows.map { row -> String in
                let cells = row.map { cellText in
                    "<w:tc><w:p><w:r><w:t>\(XMLSanitizer.escape(cellText))</w:t></w:r></w:p></w:tc>"
                }.joined()
                return "<w:tr>\(cells)</w:tr>"
            }.joined(separator: "\n      ")

            let tblXML = """
                <w:tbl \(tAttrs)>
                  \(spansXML)
                  \(rowStrings)
                </w:tbl>
            """
            bodyItems.append(tblXML)
        }

        let documentXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:eo="urn:easyoffice:metadata:1.0">
          <w:body>
        \(bodyItems.joined(separator: "\n"))
          </w:body>
        </w:document>
        """

        return try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "[Content_Types].xml", data: Data(contentTypesXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "_rels/.rels", data: Data(relsXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "docProps/core.xml", data: Data(coreXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "word/document.xml", data: Data(documentXML.utf8), compressionMethod: .deflate)
        ])
    }

    public static func decodeDocument(from data: Data) throws -> OfficeDocumentModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        var entryMap: [String: String] = [:]
        for entry in entries {
            entryMap[entry.path] = try XMLSanitizer.validateSafeXMLData(entry.data)
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
                attributes: tblElem.attributes,
                innerXML: tblElem.innerXML,
                rowTag: "w:tr",
                cellTag: "w:tc",
                cellTextTag: "w:t"
            )
        }

        // Strip <w:tbl> blocks before extracting top-level <w:p> so table cell paragraphs are not duplicated
        let bodyWithoutTables = XMLSanitizer.stripElements(named: "w:tbl", from: documentXML)
        let paragraphs = XMLSanitizer.extractElements(named: "w:p", in: bodyWithoutTables).map { pElem in
            XMLDocumentModelMapper.parseParagraph(
                attributes: pElem.attributes,
                innerXML: pElem.innerXML,
                childTextTag: "w:t"
            )
        }

        let meta = XMLDocumentModelMapper.parseExtendedMetadata(from: coreXML, defaultPaperSize: paperSize)

        return OfficeDocumentModel(
            title: title,
            format: .docx,
            paperSize: paperSize,
            headerText: headerText,
            footerText: footerText,
            paragraphs: paragraphs,
            tables: tables,
            trackChangesEnabled: trackChanges,
            sections: meta.sections,
            footnotes: meta.footnotes,
            revisions: meta.revisions,
            comments: meta.comments,
            bookmarks: meta.bookmarks
        )
    }

    // MARK: XLSX (SpreadsheetML)

    public static func encodeWorksheet(_ sheet: WorksheetModel) throws -> Data {
        let contentTypesXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
          <Default Extension="xml" ContentType="application/xml"/>
          <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
          <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
          <Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>
        </Types>
        """

        let workbookXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
          <sheets>
            <sheet name="\(XMLSanitizer.escape(sheet.name))" sheetId="1" rId="rId1"/>
          </sheets>
        </workbook>
        """

        let sortedCoords = sheet.cellsByCoordinate.keys.sorted()
        var rowsMap: [Int: [(coord: PackedCellCoordinate, raw: String)]] = [:]
        var sharedStrings: [String] = []
        var sharedStringIndex: [String: Int] = [:]
        var totalStringOccurrences = 0

        for coord in sortedCoords {
            guard let raw = sheet.cellsByCoordinate[coord], !raw.isEmpty else { continue }
            rowsMap[coord.row + 1, default: []].append((coord, raw))
            if !raw.hasPrefix("="), Double(raw) == nil {
                totalStringOccurrences += 1
                if sharedStringIndex[raw] == nil {
                    sharedStringIndex[raw] = sharedStrings.count
                    sharedStrings.append(raw)
                }
            }
        }

        var rowElements: [String] = []
        for rowNum in rowsMap.keys.sorted() {
            guard let cells = rowsMap[rowNum] else { continue }
            var cellTags: [String] = []
            for item in cells {
                let a1 = item.coord.description
                let raw = item.raw
                if raw.hasPrefix("=") {
                    let formulaBody = String(raw.dropFirst())
                    cellTags.append("      <c r=\"\(XMLSanitizer.escape(a1))\" t=\"str\"><f>\(XMLSanitizer.escape(formulaBody))</f></c>")
                } else if Double(raw) != nil {
                    cellTags.append("      <c r=\"\(XMLSanitizer.escape(a1))\"><v>\(XMLSanitizer.escape(raw))</v></c>")
                } else if let sstIdx = sharedStringIndex[raw] {
                    cellTags.append("      <c r=\"\(XMLSanitizer.escape(a1))\" t=\"s\"><v>\(sstIdx)</v></c>")
                }
            }
            let rowXML = """
                <row r="\(rowNum)">
            \(cellTags.joined(separator: "\n"))
                </row>
            """
            rowElements.append(rowXML)
        }

        let sheetXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
          <sheetData>
        \(rowElements.joined(separator: "\n"))
          </sheetData>
        </worksheet>
        """

        let siElements = sharedStrings.map { "<si><t xml:space=\"preserve\">\(XMLSanitizer.escape($0))</t></si>" }.joined(separator: "\n  ")
        let sharedStringsXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="\(totalStringOccurrences)" uniqueCount="\(sharedStrings.count)">
          \(siElements)
        </sst>
        """

        return try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "[Content_Types].xml", data: Data(contentTypesXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "xl/workbook.xml", data: Data(workbookXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "xl/sharedStrings.xml", data: Data(sharedStringsXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "xl/worksheets/sheet1.xml", data: Data(sheetXML.utf8), compressionMethod: .deflate)
        ])
    }

    public static func decodeWorksheet(from data: Data) throws -> WorksheetModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        var entryMap: [String: String] = [:]
        for entry in entries {
            entryMap[entry.path] = try XMLSanitizer.validateSafeXMLData(entry.data)
        }
        guard let sheetXML = entryMap["xl/worksheets/sheet1.xml"] else {
            throw EasyOfficeError.corruptedContainer(detail: "OOXML XLSX 필수 스트림 'xl/worksheets/sheet1.xml'이 누락되었습니다.")
        }

        var sharedStrings: [String] = []
        if let sstXML = entryMap["xl/sharedStrings.xml"] {
            for siElem in XMLSanitizer.extractElements(named: "si", in: sstXML) {
                let textParts = XMLSanitizer.extractElements(named: "t", in: siElem.innerXML).map {
                    XMLSanitizer.unescape($0.innerXML)
                }
                sharedStrings.append(textParts.joined())
            }
        }

        let workbookXML = entryMap["xl/workbook.xml"] ?? ""
        let sheetName = XMLSanitizer.extractElements(named: "sheet", in: workbookXML).first?.attributes["name"] ?? "Sheet1"

        var model = WorksheetModel(name: sheetName)
        for cellElem in XMLSanitizer.extractElements(named: "c", in: sheetXML) {
            guard let addr = cellElem.attributes["r"] else { continue }
            let cellType = cellElem.attributes["t"] ?? ""

            if let fElem = XMLSanitizer.extractElements(named: "f", in: cellElem.innerXML).first {
                let formulaText = XMLSanitizer.unescape(fElem.innerXML)
                try model.setCell(addr, rawValue: "=\(formulaText)")
            } else if cellType == "s" {
                guard let vElem = XMLSanitizer.extractElements(named: "v", in: cellElem.innerXML).first,
                      let sstIndex = Int(vElem.innerXML.trimmingCharacters(in: .whitespacesAndNewlines)),
                      sstIndex >= 0, sstIndex < sharedStrings.count else {
                    throw EasyOfficeError.corruptedContainer(detail: "XLSX sharedStrings 인덱스 참조가 유효하지 않습니다 (cell: \(addr)).")
                }
                try model.setCell(addr, rawValue: sharedStrings[sstIndex])
            } else if cellType == "inlineStr" {
                let tElems = XMLSanitizer.extractElements(named: "t", in: cellElem.innerXML)
                if !tElems.isEmpty {
                    let inlineText = tElems.map { XMLSanitizer.unescape($0.innerXML) }.joined()
                    try model.setCell(addr, rawValue: inlineText)
                } else if let vElem = XMLSanitizer.extractElements(named: "v", in: cellElem.innerXML).first {
                    try model.setCell(addr, rawValue: XMLSanitizer.unescape(vElem.innerXML))
                }
            } else if cellType == "b" {
                if let vElem = XMLSanitizer.extractElements(named: "v", in: cellElem.innerXML).first {
                    let trimmed = vElem.innerXML.trimmingCharacters(in: .whitespacesAndNewlines)
                    let boolStr = (trimmed == "1" || trimmed.caseInsensitiveCompare("TRUE") == .orderedSame) ? "TRUE" : "FALSE"
                    try model.setCell(addr, rawValue: boolStr)
                }
            } else if let vElem = XMLSanitizer.extractElements(named: "v", in: cellElem.innerXML).first {
                let valText = XMLSanitizer.unescape(vElem.innerXML)
                try model.setCell(addr, rawValue: valText)
            }
        }
        return model
    }

    // MARK: PPTX (PresentationML)

    public static func encodePresentation(_ deck: PresentationDeckModel) throws -> Data {
        var entries: [ZipContainerEntry] = []

        let contentTypesXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
          <Default Extension="xml" ContentType="application/xml"/>
          <Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
        </Types>
        """
        entries.append(ZipContainerEntry(path: "[Content_Types].xml", data: Data(contentTypesXML.utf8), compressionMethod: .deflate))

        var slideRefs: [String] = []
        for idx in deck.slides.indices {
            slideRefs.append("    <p:sldId id=\"\(256 + idx)\" target=\"ppt/slides/slide\(idx + 1).xml\"/>")
        }

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
                return """
                    <p:sp \(attrs)>
                      <p:nvSpPr><p:cNvPr id="\(elIdx + 2)" name="\(XMLSanitizer.escape(el.title))"/></p:nvSpPr>
                      <p:spPr><a:xfrm><a:off x="\(Int(el.x.rounded()))" y="\(Int(el.y.rounded()))"/><a:ext cx="\(Int(el.width.rounded()))" cy="\(Int(el.height.rounded()))"/></a:xfrm></p:spPr>
                      <p:txBody><a:bodyPr/><a:p><a:r><a:t>\(XMLSanitizer.escape(el.bodyText))</a:t></a:r></a:p></p:txBody>
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
            entries.append(ZipContainerEntry(
                path: "ppt/slides/slide\(index + 1).xml",
                data: Data(slideXML.utf8),
                compressionMethod: .deflate
            ))
        }

        return try ZipContainerCodec.pack(entries: entries)
    }

    public static func decodePresentation(from data: Data) throws -> PresentationDeckModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        var entryMap: [String: String] = [:]
        for entry in entries {
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
            let notes = XMLSanitizer.extractElements(named: "p:notes", in: sldElem.innerXML).first.map { XMLSanitizer.unescape($0.innerXML) } ?? ""
            let bullets = XMLSanitizer.extractElements(named: "p:bullet", in: sldElem.innerXML).map { XMLSanitizer.unescape($0.innerXML) }

            slides.append(SlideModel(
                id: slideID,
                title: sTitle,
                subtitle: sSubtitle,
                bulletPoints: bullets,
                speakerNotes: notes,
                masterLayout: layout,
                transition: transition,
                elements: elements
            ))
        }

        return PresentationDeckModel(title: title, aspectRatio: aspectRatio, slides: slides)
    }
}

// MARK: - ODF Container Codec (ISO/IEC 26300 ODT, ODS, ODP)

/// ISO/IEC 26300 OpenDocument Format container encoder and decoder (`.odt`, `.ods`, `.odp`).
public enum ODFContainerCodec {
    public static let textMimeType = "application/vnd.oasis.opendocument.text"
    public static let spreadsheetMimeType = "application/vnd.oasis.opendocument.spreadsheet"
    public static let presentationMimeType = "application/vnd.oasis.opendocument.presentation"

    public static func encodeDocument(_ doc: OfficeDocumentModel) throws -> Data {
        let pElements = doc.paragraphs.map { p in
            "<text:p \(XMLDocumentModelMapper.paragraphAttributes(p))>\(XMLSanitizer.escape(p.text))</text:p>"
        }.joined(separator: "\n      ")

        let tblElements = doc.tables.map { tbl in
            let tAttrs = XMLDocumentModelMapper.tableAttributes(tbl)
            let spansXML = XMLDocumentModelMapper.serializeTableSpans(tbl)
            let rowsXML = tbl.rows.map { row in
                let cells = row.map { "<table:table-cell><text:value>\(XMLSanitizer.escape($0))</text:value></table:table-cell>" }.joined()
                return "<table:table-row>\(cells)</table:table-row>"
            }.joined(separator: "\n        ")
            return "<table:table \(tAttrs)>\n        \(spansXML)\n        \(rowsXML)\n      </table:table>"
        }.joined(separator: "\n      ")

        let extendedMetaXML = XMLDocumentModelMapper.serializeExtendedMetadata(doc)
        let contentXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:eo="urn:easyoffice:metadata:1.0" title="\(XMLSanitizer.escape(doc.title))" paperSize="\(XMLSanitizer.escape(doc.paperSize.rawValue))" headerText="\(XMLSanitizer.escape(doc.headerText))" footerText="\(XMLSanitizer.escape(doc.footerText))" trackChanges="\(doc.trackChangesEnabled ? "1" : "0")">
          <office:meta>
            \(extendedMetaXML)
          </office:meta>
          <office:body>
            <office:text>
              \(pElements)
              \(tblElements)
            </office:text>
          </office:body>
        </office:document-content>
        """

        return try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "mimetype", data: Data(textMimeType.utf8), compressionMethod: .store),
            ZipContainerEntry(path: "content.xml", data: Data(contentXML.utf8), compressionMethod: .deflate)
        ])
    }

    public static func decodeDocument(from data: Data) throws -> OfficeDocumentModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        guard let mimeEntry = entries.first(where: { $0.path == "mimetype" }),
              String(data: mimeEntry.data, encoding: .utf8) == textMimeType else {
            throw EasyOfficeError.corruptedContainer(detail: "ODT 컨테이너의 mimetype 엔트리가 유효하지 않습니다.")
        }
        guard let contentEntry = entries.first(where: { $0.path == "content.xml" }) else {
            throw EasyOfficeError.corruptedContainer(detail: "ODT 컨테이너에 'content.xml'이 누락되었습니다.")
        }
        let xml = try XMLSanitizer.validateSafeXMLData(contentEntry.data)
        guard let root = XMLSanitizer.extractElements(named: "office:document-content", in: xml).first else {
            throw EasyOfficeError.corruptedContainer(detail: "ODT 'office:document-content' 루트를 찾을 수 없습니다.")
        }

        let tables = XMLSanitizer.extractElements(named: "table:table", in: root.innerXML).map { tblElem in
            XMLDocumentModelMapper.parseTable(
                attributes: tblElem.attributes,
                innerXML: tblElem.innerXML,
                rowTag: "table:table-row",
                cellTag: "table:table-cell",
                cellTextTag: "text:value"
            )
        }

        let bodyWithoutTables = XMLSanitizer.stripElements(named: "table:table", from: root.innerXML)
        let paragraphs = XMLSanitizer.extractElements(named: "text:p", in: bodyWithoutTables).map { elem in
            XMLDocumentModelMapper.parseParagraph(
                attributes: elem.attributes,
                innerXML: elem.innerXML,
                childTextTag: nil
            )
        }

        let paperSize = PaperSizePreset(rawValue: root.attributes["paperSize"] ?? "") ?? .a4
        let meta = XMLDocumentModelMapper.parseExtendedMetadata(from: root.innerXML, defaultPaperSize: paperSize)

        return OfficeDocumentModel(
            title: root.attributes["title"] ?? "제목 없음",
            format: .odt,
            paperSize: paperSize,
            headerText: root.attributes["headerText"] ?? "",
            footerText: root.attributes["footerText"] ?? "",
            paragraphs: paragraphs,
            tables: tables,
            trackChangesEnabled: root.attributes["trackChanges"] == "1",
            sections: meta.sections,
            footnotes: meta.footnotes,
            revisions: meta.revisions,
            comments: meta.comments,
            bookmarks: meta.bookmarks
        )
    }

    public static func encodeWorksheet(_ sheet: WorksheetModel) throws -> Data {
        let cellsXML = sheet.cellsByCoordinate.keys.sorted().compactMap { coord -> String? in
            guard let raw = sheet.cellsByCoordinate[coord] else { return nil }
            let key = coord.description
            return "<table:table-cell address=\"\(XMLSanitizer.escape(key))\" raw=\"\(XMLSanitizer.escape(raw))\"/>"
        }.joined(separator: "\n        ")

        let contentXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0">
          <office:body>
            <office:spreadsheet>
              <table:table table:name="\(XMLSanitizer.escape(sheet.name))">
                \(cellsXML)
              </table:table>
            </office:spreadsheet>
          </office:body>
        </office:document-content>
        """

        return try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "mimetype", data: Data(spreadsheetMimeType.utf8), compressionMethod: .store),
            ZipContainerEntry(path: "content.xml", data: Data(contentXML.utf8), compressionMethod: .deflate)
        ])
    }

    public static func decodeWorksheet(from data: Data) throws -> WorksheetModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        guard let mimeEntry = entries.first(where: { $0.path == "mimetype" }),
              String(data: mimeEntry.data, encoding: .utf8) == spreadsheetMimeType else {
            throw EasyOfficeError.corruptedContainer(detail: "ODS 컨테이너의 mimetype 엔트리가 유효하지 않습니다.")
        }
        guard let contentEntry = entries.first(where: { $0.path == "content.xml" }) else {
            throw EasyOfficeError.corruptedContainer(detail: "ODS 컨테이너에 'content.xml'이 누락되었습니다.")
        }
        let xml = try XMLSanitizer.validateSafeXMLData(contentEntry.data)
        let tableElem = XMLSanitizer.extractElements(named: "table:table", in: xml).first
        var sheet = WorksheetModel(name: tableElem?.attributes["table:name"] ?? "Sheet1")
        for cellElem in XMLSanitizer.extractElements(named: "table:table-cell", in: xml) {
            if let addr = cellElem.attributes["address"], let raw = cellElem.attributes["raw"] {
                try sheet.setCell(addr, rawValue: raw)
            }
        }
        return sheet
    }

    public static func encodePresentation(_ deck: PresentationDeckModel) throws -> Data {
        let pagesXML = deck.slides.map { slide -> String in
            let bullets = slide.bulletPoints.map { "<text:list-item>\(XMLSanitizer.escape($0))</text:list-item>" }.joined()
            let shapes = slide.elements.map { el in
                "<draw:frame \(XMLDocumentModelMapper.slideElementAttributes(el))>\(XMLSanitizer.escape(el.bodyText))</draw:frame>"
            }.joined()
            return "<draw:page id=\"\(slide.id.uuidString)\" title=\"\(XMLSanitizer.escape(slide.title))\" subtitle=\"\(XMLSanitizer.escape(slide.subtitle))\" notes=\"\(XMLSanitizer.escape(slide.speakerNotes))\" layout=\"\(XMLSanitizer.escape(slide.masterLayout.rawValue))\" transition=\"\(XMLSanitizer.escape(slide.transition.rawValue))\">\(bullets)\(shapes)</draw:page>"
        }.joined(separator: "\n      ")

        let contentXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" title="\(XMLSanitizer.escape(deck.title))" aspectRatio="\(XMLSanitizer.escape(deck.aspectRatio.rawValue))">
          <office:body>
            <office:presentation>
              \(pagesXML)
            </office:presentation>
          </office:body>
        </office:document-content>
        """

        return try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "mimetype", data: Data(presentationMimeType.utf8), compressionMethod: .store),
            ZipContainerEntry(path: "content.xml", data: Data(contentXML.utf8), compressionMethod: .deflate)
        ])
    }

    public static func decodePresentation(from data: Data) throws -> PresentationDeckModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        guard let mimeEntry = entries.first(where: { $0.path == "mimetype" }),
              String(data: mimeEntry.data, encoding: .utf8) == presentationMimeType else {
            throw EasyOfficeError.corruptedContainer(detail: "ODP 컨테이너의 mimetype 엔트리가 유효하지 않습니다.")
        }
        guard let contentEntry = entries.first(where: { $0.path == "content.xml" }) else {
            throw EasyOfficeError.corruptedContainer(detail: "ODP 컨테이너에 'content.xml'이 누락되었습니다.")
        }
        let xml = try XMLSanitizer.validateSafeXMLData(contentEntry.data)
        let root = XMLSanitizer.extractElements(named: "office:document-content", in: xml).first
        let slides = XMLSanitizer.extractElements(named: "draw:page", in: xml).map { page -> SlideModel in
            let slideID = page.attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID()
            let bullets = XMLSanitizer.extractElements(named: "text:list-item", in: page.innerXML).map { XMLSanitizer.unescape($0.innerXML) }
            let elements = XMLSanitizer.extractElements(named: "draw:frame", in: page.innerXML).map { frame in
                XMLDocumentModelMapper.parseSlideElement(attributes: frame.attributes, innerXML: frame.innerXML)
            }
            return SlideModel(
                id: slideID,
                title: page.attributes["title"] ?? "",
                subtitle: page.attributes["subtitle"] ?? "",
                bulletPoints: bullets,
                speakerNotes: page.attributes["notes"] ?? "",
                masterLayout: SlideMasterLayout(rawValue: page.attributes["layout"] ?? "") ?? .titleAndContent,
                transition: SlideTransitionKind(rawValue: page.attributes["transition"] ?? "") ?? .fade,
                elements: elements
            )
        }
        return PresentationDeckModel(
            title: root?.attributes["title"] ?? "발표자료",
            aspectRatio: SlideAspectRatio(rawValue: root?.attributes["aspectRatio"] ?? "") ?? .widescreen16x9,
            slides: slides
        )
    }
}

// MARK: - HWPX Container Codec (KS X 6101 OWPML)

/// KS X 6101 OWPML (`HWPX`) ZIP-XML container encoder and decoder.
public enum HWPXContainerCodec {
    public static let hwpxMimeType = "application/hwp+zip"

    public static func encodeDocument(_ doc: OfficeDocumentModel) throws -> Data {
        let contentHPF = """
        <?xml version="1.0" encoding="UTF-8"?>
        <opf:package xmlns:opf="http://www.idpf.org/2007/opf/" version="1.0">
          <opf:metadata>
            <opf:title>\(XMLSanitizer.escape(doc.title))</opf:title>
          </opf:metadata>
          <opf:manifest>
            <opf:item id="header" href="Contents/header.xml" media-type="application/xml"/>
            <opf:item id="section0" href="Contents/section0.xml" media-type="application/xml"/>
          </opf:manifest>
        </opf:package>
        """

        let extendedMetaXML = XMLDocumentModelMapper.serializeExtendedMetadata(doc)
        let headerXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <hh:head xmlns:hh="http://www.owpml.org/owpml/2011/head" xmlns:eo="urn:easyoffice:metadata:1.0" paperSize="\(XMLSanitizer.escape(doc.paperSize.rawValue))" headerText="\(XMLSanitizer.escape(doc.headerText))" footerText="\(XMLSanitizer.escape(doc.footerText))" trackChanges="\(doc.trackChangesEnabled ? "1" : "0")">
          \(extendedMetaXML)
        </hh:head>
        """

        let paragraphsXML = doc.paragraphs.map { p in
            """
              <hp:p \(XMLDocumentModelMapper.paragraphAttributes(p))>
                <hp:run><hp:t>\(XMLSanitizer.escape(p.text))</hp:t></hp:run>
              </hp:p>
            """
        }.joined(separator: "\n")

        let tablesXML = doc.tables.map { tbl in
            let tAttrs = XMLDocumentModelMapper.tableAttributes(tbl)
            let spansXML = XMLDocumentModelMapper.serializeTableSpans(tbl)
            let rows = tbl.rows.map { row in
                let cells = row.map { "<hp:tc><hp:cellText>\(XMLSanitizer.escape($0))</hp:cellText></hp:tc>" }.joined()
                return "<hp:tr>\(cells)</hp:tr>"
            }.joined(separator: "\n    ")
            return """
              <hp:tbl \(tAttrs)>
                \(spansXML)
                \(rows)
              </hp:tbl>
            """
        }.joined(separator: "\n")

        let sectionXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <hs:sec xmlns:hs="http://www.owpml.org/owpml/2011/section" xmlns:hp="http://www.owpml.org/owpml/2011/paragraph" xmlns:eo="urn:easyoffice:metadata:1.0">
        \(paragraphsXML)
        \(tablesXML)
        </hs:sec>
        """

        return try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "mimetype", data: Data(hwpxMimeType.utf8), compressionMethod: .store),
            ZipContainerEntry(path: "Contents/content.hpf", data: Data(contentHPF.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "Contents/header.xml", data: Data(headerXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "Contents/section0.xml", data: Data(sectionXML.utf8), compressionMethod: .deflate)
        ])
    }

    public static func decodeDocument(from data: Data) throws -> OfficeDocumentModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        var entryMap: [String: String] = [:]
        for entry in entries {
            entryMap[entry.path] = try XMLSanitizer.validateSafeXMLData(entry.data)
        }

        guard entryMap["mimetype"] == hwpxMimeType else {
            throw EasyOfficeError.corruptedContainer(detail: "HWPX(KS X 6101) 컨테이너의 mimetype이 'application/hwp+zip'이 아닙니다.")
        }
        guard let hpfXML = entryMap["Contents/content.hpf"],
              let headerXML = entryMap["Contents/header.xml"],
              let sectionXML = entryMap["Contents/section0.xml"] else {
            throw EasyOfficeError.corruptedContainer(detail: "HWPX 필수 스트림(content.hpf, header.xml, section0.xml)이 누락되었습니다.")
        }

        let title = XMLSanitizer.extractElements(named: "opf:title", in: hpfXML).first.map { XMLSanitizer.unescape($0.innerXML) } ?? "제목 없음"
        let headElem = XMLSanitizer.extractElements(named: "hh:head", in: headerXML).first
        let headAttrs = headElem?.attributes ?? [:]
        let paperSize = PaperSizePreset(rawValue: headAttrs["paperSize"] ?? "") ?? .a4
        let headerText = headAttrs["headerText"] ?? ""
        let footerText = headAttrs["footerText"] ?? ""
        let trackChanges = headAttrs["trackChanges"] == "1"

        let tables = XMLSanitizer.extractElements(named: "hp:tbl", in: sectionXML).map { tblElem in
            XMLDocumentModelMapper.parseTable(
                attributes: tblElem.attributes,
                innerXML: tblElem.innerXML,
                rowTag: "hp:tr",
                cellTag: "hp:tc",
                cellTextTag: "hp:cellText"
            )
        }

        let sectionWithoutTables = XMLSanitizer.stripElements(named: "hp:tbl", from: sectionXML)
        let paragraphs = XMLSanitizer.extractElements(named: "hp:p", in: sectionWithoutTables).map { pElem in
            XMLDocumentModelMapper.parseParagraph(
                attributes: pElem.attributes,
                innerXML: pElem.innerXML,
                childTextTag: "hp:t"
            )
        }

        let meta = XMLDocumentModelMapper.parseExtendedMetadata(from: headerXML, defaultPaperSize: paperSize)

        return OfficeDocumentModel(
            title: title,
            format: .hwpx,
            paperSize: paperSize,
            headerText: headerText,
            footerText: footerText,
            paragraphs: paragraphs,
            tables: tables,
            trackChangesEnabled: trackChanges,
            sections: meta.sections,
            footnotes: meta.footnotes,
            revisions: meta.revisions,
            comments: meta.comments,
            bookmarks: meta.bookmarks
        )
    }
}
