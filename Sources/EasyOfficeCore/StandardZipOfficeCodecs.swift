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
        path.hasPrefix("word/media/") ||
            path.hasPrefix("Pictures/") ||
            path.hasPrefix("BinData/") ||
            path.hasPrefix("ppt/media/") ||
            path.hasPrefix("xl/media/")
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
        if let fontName = run.fontName {
            parts.append("font=\"\(XMLSanitizer.escape(fontName))\"")
        }
        if let fontSizePt = run.fontSizePt {
            parts.append("size=\"\(fontSizePt)\"")
        }
        if let fg = run.foregroundHex {
            parts.append("color=\"\(XMLSanitizer.escape(fg))\"")
        }
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
        // 1. Check ODF <text:span>
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

        // 2. Check HWPX <hp:run>
        if childTextTag == "hp:t" {
            let hpRuns = XMLSanitizer.extractElements(named: "hp:run", in: innerXML)
            if !hpRuns.isEmpty {
                return hpRuns.map { elem in
                    let tText = XMLSanitizer.extractElements(named: "hp:t", in: elem.innerXML)
                        .map { XMLSanitizer.unescape($0.innerXML) }
                        .joined()
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

        // 3. Check OOXML <w:r>
        if childTextTag == "w:t" {
            let wRuns = XMLSanitizer.extractElements(named: "w:r", in: innerXML)
            if !wRuns.isEmpty {
                return wRuns.map { elem in
                    let tText = XMLSanitizer.extractElements(named: "w:t", in: elem.innerXML)
                        .map { XMLSanitizer.unescape($0.innerXML) }
                        .joined()
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
                        text: tText,
                        isBold: isBold,
                        isItalic: isItalic,
                        isUnderlined: isUnderlined,
                        fontName: fontName,
                        fontSizePt: fontSizePt,
                        foregroundHex: foregroundHex
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
        let hasExplicitRuns = attributes["hasRuns"] == "1" ||
            parsedRuns.count > 1 ||
            (parsedRuns.count == 1 && parsedRuns[0].foregroundHex != nil)

        let extractedText: String
        if !parsedRuns.isEmpty {
            extractedText = parsedRuns.map(\.text).joined()
        } else if let childTag = childTextTag {
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
                runs: hasExplicitRuns ? parsedRuns : [],
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

        let firstRun = parsedRuns.first
        let fontName = firstRun?.fontName ?? "Pretendard"
        let fontSizePt = firstRun?.fontSizePt ?? 11.0
        let isBold = parsedRuns.contains(where: \.isBold)
        let isItalic = parsedRuns.contains(where: \.isItalic)
        let isUnderlined = parsedRuns.contains(where: \.isUnderlined)

        return ParagraphBlock(
            id: id,
            style: style,
            alignment: alignment,
            text: extractedText,
            runs: hasExplicitRuns ? parsedRuns : [],
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

    static func serializeExtendedMetadata(_ doc: OfficeDocumentModel, mediaPrefix: String = "word/media/") -> String {
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
        for img in doc.images {
            let safeName = sanitizeMediaFileName(img.fileName)
            let mediaPath = "\(mediaPrefix)\(safeName)"
            tags.append(
                "<eo:image id=\"\(img.id.uuidString)\" fileName=\"\(XMLSanitizer.escape(safeName))\" path=\"\(XMLSanitizer.escape(mediaPath))\" mime=\"\(XMLSanitizer.escape(img.mimeType))\" w=\"\(img.widthPt)\" h=\"\(img.heightPt)\" caption=\"\(XMLSanitizer.escape(img.caption))\" anchor=\"\(img.anchorParagraphID?.uuidString ?? "")\"/>"
            )
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

        let images = XMLSanitizer.extractElements(named: "eo:image", in: xml).compactMap { elem -> EmbeddedImageAttachment? in
            let fileName = elem.attributes["fileName"] ?? "image.png"
            let path = elem.attributes["path"] ?? ""
            let data = binaryEntries[path]
                ?? binaryEntries["word/media/\(fileName)"]
                ?? binaryEntries["Pictures/\(fileName)"]
                ?? binaryEntries["BinData/\(fileName)"]
                ?? Data()
            let anchorStr = elem.attributes["anchor"] ?? ""
            return EmbeddedImageAttachment(
                id: elem.attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID(),
                fileName: fileName,
                caption: elem.attributes["caption"] ?? "",
                mimeType: elem.attributes["mime"] ?? "image/png",
                imageData: data,
                widthPt: Double(elem.attributes["w"] ?? "320.0") ?? 320.0,
                heightPt: Double(elem.attributes["h"] ?? "200.0") ?? 200.0,
                anchorParagraphID: anchorStr.isEmpty ? nil : UUID(uuidString: anchorStr)
            )
        }

        return (
            sections: sections.isEmpty ? [DocumentSectionLayout(paperSize: defaultPaperSize)] : sections,
            footnotes: footnotes,
            revisions: revisions,
            comments: comments,
            bookmarks: bookmarks,
            images: images
        )
    }

    static func cellStyleAttributes(_ style: CellStyle?) -> String {
        guard let style, style != CellStyle() else { return "" }
        var attrs: [String] = [
            "eoBold=\"\(style.isBold ? "1" : "0")\"",
            "eoItalic=\"\(style.isItalic ? "1" : "0")\"",
            "eoUnderline=\"\(style.isUnderlined ? "1" : "0")\"",
            "eoAlign=\"\(XMLSanitizer.escape(style.alignment.rawValue))\"",
            "eoNumFmt=\"\(XMLSanitizer.escape(style.numberFormat.rawValue))\""
        ]
        if let fill = style.fillHex {
            attrs.append("eoFill=\"\(XMLSanitizer.escape(fill))\"")
        }
        if let text = style.textHex {
            attrs.append("eoText=\"\(XMLSanitizer.escape(text))\"")
        }
        return " " + attrs.joined(separator: " ")
    }

    static func parseCellStyle(from attributes: [String: String]) -> CellStyle? {
        guard attributes["eoBold"] != nil ||
                attributes["eoItalic"] != nil ||
                attributes["eoUnderline"] != nil ||
                attributes["eoAlign"] != nil ||
                attributes["eoNumFmt"] != nil ||
                attributes["eoFill"] != nil ||
                attributes["eoText"] != nil else {
            return nil
        }
        return CellStyle(
            isBold: attributes["eoBold"] == "1",
            isItalic: attributes["eoItalic"] == "1",
            isUnderlined: attributes["eoUnderline"] == "1",
            alignment: CellHorizontalAlignment(rawValue: attributes["eoAlign"] ?? "") ?? .leading,
            numberFormat: CellNumberFormat(rawValue: attributes["eoNumFmt"] ?? "") ?? .general,
            fillHex: attributes["eoFill"],
            textHex: attributes["eoText"]
        )
    }

    static func serializeWorksheetMetadata(_ sheet: WorksheetModel) -> String {
        let ps = sheet.pageSetup
        var lines: [String] = [
            "<eo:sheetMeta activeIndex=\"\(sheet.activeSheetIndex)\" trackChanges=\"\(sheet.trackChangesEnabled ? "1" : "0")\" paperSize=\"\(XMLSanitizer.escape(ps.paperSize.rawValue))\" orientation=\"\(XMLSanitizer.escape(ps.orientation.rawValue))\" multiCol=\"\(ps.isMultiColumnPrint ? "1" : "0")\" marginPreset=\"\(XMLSanitizer.escape(ps.marginPresetName))\"/>"
        ]
        for coord in sheet.sheetComments.keys.sorted() {
            if let comment = sheet.sheetComments[coord] {
                lines.append("<eo:cellComment addr=\"\(XMLSanitizer.escape(coord.description))\" text=\"\(XMLSanitizer.escape(comment))\"/>")
            }
        }
        return lines.joined(separator: "\n  ")
    }

    static func applyWorksheetMetadata(from xml: String, to sheet: inout WorksheetModel) {
        if let metaElem = XMLSanitizer.extractElements(named: "eo:sheetMeta", in: xml).first {
            let attrs = metaElem.attributes
            sheet.trackChangesEnabled = attrs["trackChanges"] == "1"
            sheet.pageSetup = WorksheetPageSetup(
                paperSize: PaperSizePreset(rawValue: attrs["paperSize"] ?? "") ?? .a4,
                orientation: PageOrientation(rawValue: attrs["orientation"] ?? "") ?? .portrait,
                isMultiColumnPrint: attrs["multiCol"] == "1",
                marginPresetName: attrs["marginPreset"] ?? "기본 (Normal)"
            )
            if let activeIdx = Int(attrs["activeIndex"] ?? "0") {
                sheet.switchActiveSheet(to: activeIdx)
            }
        }
        for commentElem in XMLSanitizer.extractElements(named: "eo:cellComment", in: xml) {
            if let addrStr = commentElem.attributes["addr"],
               let text = commentElem.attributes["text"],
               let addr = try? CellAddress(a1Notation: addrStr) {
                sheet.sheetComments[addr.packedCoordinate] = text
            }
        }
    }

    static func normalizeOpenFormulaExpression(_ rawFormula: String) -> String {
        var expr = rawFormula.trimmingCharacters(in: .whitespacesAndNewlines)
        if expr.lowercased().hasPrefix("of:") {
            expr = String(expr.dropFirst(3))
        } else if expr.lowercased().hasPrefix("oooc:") {
            expr = String(expr.dropFirst(5))
        }
        if !expr.hasPrefix("=") {
            expr = "=" + expr
        }
        // Normalize OpenFormula bracketed cell references such as [.B1:.C1] -> B1:C1 and [.A1] -> A1
        var normalized = ""
        normalized.reserveCapacity(expr.count)
        var idx = expr.startIndex
        while idx < expr.endIndex {
            let ch = expr[idx]
            if ch == "[" {
                let nextIdx = expr.index(after: idx)
                if nextIdx < expr.endIndex, expr[nextIdx] == "." {
                    idx = expr.index(after: nextIdx)
                    continue
                }
            } else if ch == ":" {
                let nextIdx = expr.index(after: idx)
                if nextIdx < expr.endIndex, expr[nextIdx] == "." {
                    normalized.append(":")
                    idx = expr.index(after: nextIdx)
                    continue
                }
            } else if ch == "]" {
                idx = expr.index(after: idx)
                continue
            }
            normalized.append(ch)
            idx = expr.index(after: idx)
        }
        return normalized
    }

    static func slideElementAttributes(_ el: SlideCanvasElement) -> String {
        "id=\"\(el.id.uuidString)\" title=\"\(XMLSanitizer.escape(el.title))\" x=\"\(el.x)\" y=\"\(el.y)\" w=\"\(el.width)\" h=\"\(el.height)\" kind=\"\(XMLSanitizer.escape(el.kind.rawValue))\" zIndex=\"\(el.zIndex)\" rotation=\"\(el.rotationDegrees)\" fillHex=\"\(XMLSanitizer.escape(el.fillHex))\" strokeHex=\"\(XMLSanitizer.escape(el.strokeHex))\" bold=\"\(el.isBold ? "1" : "0")\" italic=\"\(el.isItalic ? "1" : "0")\" underline=\"\(el.isUnderlined ? "1" : "0")\""
    }

    static func parseSlideElement(attributes: [String: String], innerXML: String) -> SlideCanvasElement {
        let id = attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID()
        let cNvPrAttrs = XMLSanitizer.extractElements(named: "p:cNvPr", in: innerXML).first?.attributes
        let offAttrs = XMLSanitizer.extractElements(named: "a:off", in: innerXML).first?.attributes
        let extAttrs = XMLSanitizer.extractElements(named: "a:ext", in: innerXML).first?.attributes
        let rPrAttrs = XMLSanitizer.extractElements(named: "a:rPr", in: innerXML).first?.attributes

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
        let isBold = (attributes["bold"] == "1") || (rPrAttrs?["b"] == "1")
        let isItalic = (attributes["italic"] == "1") || (rPrAttrs?["i"] == "1")
        let isUnderlined = (attributes["underline"] == "1") || ((rPrAttrs?["u"] ?? "none") != "none" && rPrAttrs?["u"] != nil)

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
            strokeHex: strokeHex,
            isBold: isBold,
            isItalic: isItalic,
            isUnderlined: isUnderlined
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

            let pXML = """
                <w:p \(pAttrs)>
                  <w:pPr><w:pStyle w:val="\(styleVal)"/><w:jc w:val="\(jcVal)"/></w:pPr>
            \(runsXML)
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

        let meta = XMLDocumentModelMapper.parseExtendedMetadata(
            from: coreXML,
            defaultPaperSize: paperSize,
            binaryEntries: binaryEntries
        )

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
            bookmarks: meta.bookmarks,
            images: meta.images
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
                let textParts = XMLSanitizer.extractElements(named: "t", in: siElem.innerXML).map {
                    XMLSanitizer.unescape($0.innerXML)
                }
                sharedStrings.append(textParts.joined())
            }
        }

        let workbookXML = entryMap["xl/workbook.xml"] ?? ""
        let sheetTags = XMLSanitizer.extractElements(named: "sheet", in: workbookXML)
        let sheetDescriptors: [(name: String, path: String)]
        if sheetTags.isEmpty {
            sheetDescriptors = [("Sheet1", "xl/worksheets/sheet1.xml")]
        } else {
            sheetDescriptors = sheetTags.enumerated().map { idx, elem in
                let name = elem.attributes["name"] ?? "Sheet\(idx + 1)"
                return (name, "xl/worksheets/sheet\(idx + 1).xml")
            }
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
                        let inlineText = tElems.map { XMLSanitizer.unescape($0.innerXML) }.joined()
                        try pageModel.setCell(addr, rawValue: inlineText)
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
                    let valText = XMLSanitizer.unescape(vElem.innerXML)
                    try pageModel.setCell(addr, rawValue: valText)
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
                    cellStyles: page.cellStyles
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
            entries.append(ZipContainerEntry(
                path: "ppt/slides/slide\(index + 1).xml",
                data: Data(slideXML.utf8),
                compressionMethod: .deflate
            ))

            if !slide.speakerNotes.isEmpty {
                let notesSlideXML = """
                <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
                <p:notes xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
                  <p:cSld>
                    <p:spTree>
                      <p:sp>
                        <p:txBody>
                          <a:bodyPr/>
                          <a:p><a:r><a:t>\(XMLSanitizer.escape(slide.speakerNotes))</a:t></a:r></a:p>
                        </p:txBody>
                      </p:sp>
                    </p:spTree>
                  </p:cSld>
                </p:notes>
                """
                entries.append(ZipContainerEntry(
                    path: "ppt/notesSlides/notesSlide\(index + 1).xml",
                    data: Data(notesSlideXML.utf8),
                    compressionMethod: .deflate
                ))
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
        let pElements = doc.paragraphs.map { p -> String in
            let pAttrs = XMLDocumentModelMapper.paragraphAttributes(p)
            if p.runs.isEmpty {
                return "<text:p \(pAttrs)>\(XMLSanitizer.escape(p.text))</text:p>"
            }
            let spans = p.runs.map { run in
                "<text:span \(XMLDocumentModelMapper.runAttributes(run))>\(XMLSanitizer.escape(run.text))</text:span>"
            }.joined()
            return "<text:p \(pAttrs)>\(spans)</text:p>"
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

        let extendedMetaXML = XMLDocumentModelMapper.serializeExtendedMetadata(doc, mediaPrefix: "Pictures/")
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

        var entries: [ZipContainerEntry] = [
            ZipContainerEntry(path: "mimetype", data: Data(textMimeType.utf8), compressionMethod: .store),
            ZipContainerEntry(path: "content.xml", data: Data(contentXML.utf8), compressionMethod: .deflate)
        ]
        for img in doc.images {
            let safeName = XMLDocumentModelMapper.sanitizeMediaFileName(img.fileName)
            entries.append(ZipContainerEntry(path: "Pictures/\(safeName)", data: img.imageData, compressionMethod: .store))
        }
        return try ZipContainerCodec.pack(entries: entries)
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
        var binaryEntries: [String: Data] = [:]
        for entry in entries where XMLDocumentModelMapper.isBinaryMediaPath(entry.path) {
            binaryEntries[entry.path] = entry.data
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
        let meta = XMLDocumentModelMapper.parseExtendedMetadata(
            from: root.innerXML,
            defaultPaperSize: paperSize,
            binaryEntries: binaryEntries
        )

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
            bookmarks: meta.bookmarks,
            images: meta.images
        )
    }

    public static func encodeWorksheet(_ sheet: WorksheetModel) throws -> Data {
        let orderedPages = sheet.orderedSheetPages()
        let tablesXML = orderedPages.map { page -> String in
            let allCoords = Set(page.cellsByCoordinate.keys).union(page.cellStyles.keys)
            guard !allCoords.isEmpty else {
                return "<table:table table:name=\"\(XMLSanitizer.escape(page.name))\"/>"
            }
            let maxRow = allCoords.map(\.row).max() ?? 0
            var rowStrings: [String] = []
            for r in 0...maxRow {
                let rowCoords = allCoords.filter { $0.row == r }
                guard !rowCoords.isEmpty else {
                    rowStrings.append("        <table:table-row/>")
                    continue
                }
                let maxCol = rowCoords.map(\.column).max() ?? 0
                var cellStrings: [String] = []
                for c in 0...maxCol {
                    let coord = PackedCellCoordinate(row: r, column: c)
                    let raw = page.cellsByCoordinate[coord] ?? ""
                    let styleAttrs = XMLDocumentModelMapper.cellStyleAttributes(page.cellStyles[coord])
                    if raw.isEmpty && styleAttrs.isEmpty {
                        cellStrings.append("<table:table-cell/>")
                    } else if raw.hasPrefix("=") {
                        let escapedRaw = XMLSanitizer.escape(raw)
                        cellStrings.append("<table:table-cell address=\"\(coord.description)\" raw=\"\(escapedRaw)\" table:formula=\"of:\(escapedRaw)\" office:value-type=\"string\"\(styleAttrs)><text:p>\(escapedRaw)</text:p></table:table-cell>")
                    } else if !raw.isEmpty, Double(raw)?.isFinite == true {
                        let escapedRaw = XMLSanitizer.escape(raw)
                        cellStrings.append("<table:table-cell address=\"\(coord.description)\" raw=\"\(escapedRaw)\" office:value-type=\"float\" office:value=\"\(escapedRaw)\"\(styleAttrs)><text:p>\(escapedRaw)</text:p></table:table-cell>")
                    } else {
                        let escapedRaw = XMLSanitizer.escape(raw)
                        cellStrings.append("<table:table-cell address=\"\(coord.description)\" raw=\"\(escapedRaw)\" office:value-type=\"string\"\(styleAttrs)><text:p>\(escapedRaw)</text:p></table:table-cell>")
                    }
                }
                rowStrings.append("        <table:table-row>\(cellStrings.joined())</table:table-row>")
            }
            return """
                  <table:table table:name="\(XMLSanitizer.escape(page.name))">
            \(rowStrings.joined(separator: "\n"))
                  </table:table>
            """
        }.joined(separator: "\n")

        let metaXML = XMLDocumentModelMapper.serializeWorksheetMetadata(sheet)
        let contentXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:eo="urn:easyoffice:metadata:1.0">
          <office:meta>
            \(metaXML)
          </office:meta>
          <office:body>
            <office:spreadsheet>
        \(tablesXML)
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
        let tableElems = XMLSanitizer.extractElements(named: "table:table", in: xml)

        func decodeTableElement(_ tableElem: (attributes: [String: String], innerXML: String)) throws -> WorksheetModel {
            let name = tableElem.attributes["table:name"] ?? "Sheet1"
            var pageModel = WorksheetModel(name: name)
            let rowElems = XMLSanitizer.extractElements(named: "table:table-row", in: tableElem.innerXML)
            if !rowElems.isEmpty {
                var rowIdx = 0
                for rowElem in rowElems {
                    let rowRepeat = max(1, min(1024, Int(rowElem.attributes["table:number-rows-repeated"] ?? "1") ?? 1))
                    var colIdx = 0
                    let cellElems = XMLSanitizer.extractElements(named: "table:table-cell", in: rowElem.innerXML)
                    for cellElem in cellElems {
                        let colRepeat = max(1, min(1024, Int(cellElem.attributes["table:number-columns-repeated"] ?? "1") ?? 1))
                        let attrs = cellElem.attributes
                        let resolvedRaw: String
                        if let rawAttr = attrs["raw"] {
                            resolvedRaw = rawAttr
                        } else if let formulaAttr = attrs["table:formula"], !formulaAttr.isEmpty {
                            resolvedRaw = XMLDocumentModelMapper.normalizeOpenFormulaExpression(formulaAttr)
                        } else if let valAttr = attrs["office:value"], !valAttr.isEmpty {
                            resolvedRaw = valAttr
                        } else if let boolAttr = attrs["office:boolean-value"], !boolAttr.isEmpty {
                            resolvedRaw = boolAttr.uppercased()
                        } else {
                            let textPs = XMLSanitizer.extractElements(named: "text:p", in: cellElem.innerXML)
                            resolvedRaw = textPs.map { XMLSanitizer.unescape($0.innerXML) }.joined(separator: "\n")
                        }

                        let parsedStyle = XMLDocumentModelMapper.parseCellStyle(from: attrs)
                        for rOff in 0..<rowRepeat {
                            for cOff in 0..<colRepeat {
                                let targetRow = rowIdx + rOff
                                let targetCol = colIdx + cOff
                                if !resolvedRaw.isEmpty {
                                    try pageModel.setCell(row: targetRow, column: targetCol, rawValue: resolvedRaw)
                                }
                                if let parsedStyle {
                                    pageModel.setCellStyle(parsedStyle, at: PackedCellCoordinate(row: targetRow, column: targetCol))
                                }
                            }
                        }
                        colIdx += colRepeat
                    }
                    rowIdx += rowRepeat
                }
            } else {
                for cellElem in XMLSanitizer.extractElements(named: "table:table-cell", in: tableElem.innerXML) {
                    if let addr = cellElem.attributes["address"], let raw = cellElem.attributes["raw"] {
                        try pageModel.setCell(addr, rawValue: raw)
                    }
                }
            }
            return pageModel
        }

        var decodedPages: [WorksheetModel] = []
        for tableElem in tableElems {
            decodedPages.append(try decodeTableElement(tableElem))
        }
        var primary = decodedPages.first ?? WorksheetModel(name: "Sheet1")
        if decodedPages.count > 1 {
            primary.additionalSheets = decodedPages.dropFirst().map { page in
                WorksheetSheetPage(
                    name: page.name,
                    cellsByCoordinate: page.cellsByCoordinate,
                    cellStyles: page.cellStyles
                )
            }
        }
        XMLDocumentModelMapper.applyWorksheetMetadata(from: xml, to: &primary)
        return primary
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
        let binManifestItems = doc.images.enumerated().map { idx, img in
            let safeName = XMLDocumentModelMapper.sanitizeMediaFileName(img.fileName)
            return "    <opf:item id=\"bindata\(idx)\" href=\"BinData/\(XMLSanitizer.escape(safeName))\" media-type=\"\(XMLSanitizer.escape(img.mimeType))\"/>"
        }.joined(separator: "\n")

        let contentHPF = """
        <?xml version="1.0" encoding="UTF-8"?>
        <opf:package xmlns:opf="http://www.idpf.org/2007/opf/" version="1.0">
          <opf:metadata>
            <opf:title>\(XMLSanitizer.escape(doc.title))</opf:title>
          </opf:metadata>
          <opf:manifest>
            <opf:item id="header" href="Contents/header.xml" media-type="application/xml"/>
            <opf:item id="section0" href="Contents/section0.xml" media-type="application/xml"/>
        \(binManifestItems)
          </opf:manifest>
        </opf:package>
        """

        let extendedMetaXML = XMLDocumentModelMapper.serializeExtendedMetadata(doc, mediaPrefix: "BinData/")
        let headerXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <hh:head xmlns:hh="http://www.owpml.org/owpml/2011/head" xmlns:eo="urn:easyoffice:metadata:1.0" paperSize="\(XMLSanitizer.escape(doc.paperSize.rawValue))" headerText="\(XMLSanitizer.escape(doc.headerText))" footerText="\(XMLSanitizer.escape(doc.footerText))" trackChanges="\(doc.trackChangesEnabled ? "1" : "0")">
          \(extendedMetaXML)
        </hh:head>
        """

        let paragraphsXML = doc.paragraphs.map { p in
            let runsXML = p.effectiveRuns.map { run in
                "    <hp:run \(XMLDocumentModelMapper.runAttributes(run))><hp:t>\(XMLSanitizer.escape(run.text))</hp:t></hp:run>"
            }.joined(separator: "\n")
            return """
              <hp:p \(XMLDocumentModelMapper.paragraphAttributes(p))>
            \(runsXML)
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

        var entries: [ZipContainerEntry] = [
            ZipContainerEntry(path: "mimetype", data: Data(hwpxMimeType.utf8), compressionMethod: .store),
            ZipContainerEntry(path: "Contents/content.hpf", data: Data(contentHPF.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "Contents/header.xml", data: Data(headerXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "Contents/section0.xml", data: Data(sectionXML.utf8), compressionMethod: .deflate)
        ]
        for img in doc.images {
            let safeName = XMLDocumentModelMapper.sanitizeMediaFileName(img.fileName)
            entries.append(ZipContainerEntry(path: "BinData/\(safeName)", data: img.imageData, compressionMethod: .store))
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

        let meta = XMLDocumentModelMapper.parseExtendedMetadata(
            from: headerXML,
            defaultPaperSize: paperSize,
            binaryEntries: binaryEntries
        )

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
            bookmarks: meta.bookmarks,
            images: meta.images
        )
    }
}
