import Foundation


extension XMLDocumentModelMapper {

  static func stripAllXMLTags(_ xml: String) -> String {
    guard xml.contains("<") else { return XMLSanitizer.unescape(xml) }
    var out = ""
    out.reserveCapacity(xml.count)
    var inTag = false
    var activeQuote: Character? = nil
    for ch in xml {
      if inTag {
        if let q = activeQuote {
          if ch == q { activeQuote = nil }
        } else if ch == "\"" || ch == "'" {
          activeQuote = ch
        } else if ch == ">" {
          inTag = false
        }
      } else {
        if ch == "<" {
          inTag = true
          activeQuote = nil
        } else {
          out.append(ch)
        }
      }
    }
    return XMLSanitizer.unescape(out)
  }

  /// Expands ODF `<text:s text:c="N"/>`, `<text:tab/>`, and `<text:line-break/>` before stripping remaining XML tags.
  public static func extractODFText(from innerXML: String) -> String {
    guard innerXML.contains("<") else { return XMLSanitizer.unescape(innerXML) }
    var out = ""
    out.reserveCapacity(innerXML.count)
    var cursor = innerXML.startIndex
    while cursor < innerXML.endIndex {
      guard let ltIdx = innerXML[cursor...].firstIndex(of: "<") else {
        out.append(XMLSanitizer.unescape(String(innerXML[cursor...])))
        break
      }
      if ltIdx > cursor {
        out.append(XMLSanitizer.unescape(String(innerXML[cursor..<ltIdx])))
      }
      var scan = innerXML.index(after: ltIdx)
      var activeQuote: Character? = nil
      var gtIdx: String.Index? = nil
      while scan < innerXML.endIndex {
        let ch = innerXML[scan]
        if let q = activeQuote {
          if ch == q { activeQuote = nil }
        } else if ch == "\"" || ch == "'" {
          activeQuote = ch
        } else if ch == ">" {
          gtIdx = scan
          break
        }
        scan = innerXML.index(after: scan)
      }
      guard let resolvedGt = gtIdx else { break }
      let tagBody = innerXML[innerXML.index(after: ltIdx)..<resolvedGt]
      let trimmedTag = tagBody.trimmingCharacters(in: .whitespacesAndNewlines)
      if trimmedTag == "text:s" || trimmedTag == "text:s/" || trimmedTag.hasPrefix("text:s ") {
        let attrs = XMLSanitizer.parseAttributes(from: tagBody[tagBody.startIndex...])
        let count = max(1, min(4_096, Int(attrs["text:c"] ?? attrs["c"] ?? "1") ?? 1))
        out.append(String(repeating: " ", count: count))
      } else if trimmedTag == "text:tab" || trimmedTag == "text:tab/"
        || trimmedTag.hasPrefix("text:tab ")
      {
        out.append("\t")
      } else if trimmedTag == "text:line-break" || trimmedTag == "text:line-break/"
        || trimmedTag.hasPrefix("text:line-break ")
      {
        out.append("\n")
      }
      cursor = innerXML.index(after: resolvedGt)
    }
    return out
  }

  static func parseODTMixedSpans(in innerXML: String) -> [DocumentTextRun] {
    let openPrefix = "<text:span"
    let closeTag = "</text:span>"
    guard innerXML.contains(openPrefix) else { return [] }

    var runs: [DocumentTextRun] = []
    var searchStart = innerXML.startIndex

    while let openRange = innerXML.range(of: openPrefix, range: searchStart..<innerXML.endIndex) {
      let afterTagName = openRange.upperBound
      if afterTagName < innerXML.endIndex {
        let ch = innerXML[afterTagName]
        guard ch == " " || ch == ">" || ch == "/" || ch == "\t" || ch == "\n" || ch == "\r" else {
          searchStart = afterTagName
          continue
        }
      }
      var tagEndIdx: String.Index? = nil
      var cursor = afterTagName
      var activeQuote: Character? = nil
      while cursor < innerXML.endIndex {
        let ch = innerXML[cursor]
        if let q = activeQuote {
          if ch == q { activeQuote = nil }
        } else if ch == "\"" || ch == "'" {
          activeQuote = ch
        } else if ch == ">" {
          tagEndIdx = cursor
          break
        }
        cursor = innerXML.index(after: cursor)
      }
      guard let resolvedTagEnd = tagEndIdx else { break }

      let leadingSlice = String(innerXML[searchStart..<openRange.lowerBound])
      let leadingText = extractODFText(from: leadingSlice)
      if !leadingText.isEmpty {
        runs.append(DocumentTextRun(text: leadingText))
      }

      let afterTagEnd = innerXML.index(after: resolvedTagEnd)
      let attrSlice = innerXML[afterTagName..<resolvedTagEnd]
      let attrs = XMLSanitizer.parseAttributes(from: attrSlice)

      if XMLSanitizer.isSelfClosingTag(attrSlice) {
        searchStart = afterTagEnd
        continue
      }

      var depth = 1
      var scanCursor = afterTagEnd
      var matchedCloseRange: Range<String.Index>? = nil
      while scanCursor < innerXML.endIndex {
        let nextOpen = innerXML.range(of: openPrefix, range: scanCursor..<innerXML.endIndex)
        let nextClose = innerXML.range(of: closeTag, range: scanCursor..<innerXML.endIndex)
        guard let closeR = nextClose else { break }
        if let openR = nextOpen, openR.lowerBound < closeR.lowerBound {
          let afterNestedName = openR.upperBound
          var nestedEnd = afterNestedName
          var q: Character? = nil
          while nestedEnd < innerXML.endIndex {
            let c = innerXML[nestedEnd]
            if let activeQ = q {
              if c == activeQ { q = nil }
            } else if c == "\"" || c == "'" {
              q = c
            } else if c == ">" {
              break
            }
            nestedEnd = innerXML.index(after: nestedEnd)
          }
          if nestedEnd < innerXML.endIndex {
            let nestedAttrSlice = innerXML[afterNestedName..<nestedEnd]
            if !XMLSanitizer.isSelfClosingTag(nestedAttrSlice) {
              depth += 1
            }
            scanCursor = innerXML.index(after: nestedEnd)
          } else {
            break
          }
        } else {
          depth -= 1
          if depth == 0 {
            matchedCloseRange = closeR
            break
          }
          scanCursor = closeR.upperBound
        }
      }

      guard let closeRange = matchedCloseRange else { break }
      let spanInner = String(innerXML[afterTagEnd..<closeRange.lowerBound])
      let spanText = extractODFText(from: spanInner)
      let hasFormattingAttrs =
        attr("id", in: attrs) != nil || attr("bold", in: attrs) != nil
        || attr("italic", in: attrs) != nil || attr("underline", in: attrs) != nil
        || attr("strike", in: attrs) != nil || attr("font", in: attrs) != nil
        || attr("size", in: attrs) != nil || attr("color", in: attrs) != nil
        || attr("bgColor", in: attrs) != nil || attr("link", in: attrs) != nil
        || attrs["text:style-name"] != nil
      if !spanText.isEmpty || hasFormattingAttrs {
        runs.append(
          DocumentTextRun(
            id: attr("id", in: attrs).flatMap(UUID.init(uuidString:)) ?? UUID(),
            text: spanText,
            isBold: attr("bold", in: attrs) == "1",
            isItalic: attr("italic", in: attrs) == "1",
            isUnderlined: attr("underline", in: attrs) == "1",
            isStrikethrough: attr("strike", in: attrs) == "1",
            fontName: attr("font", in: attrs),
            fontSizePt: attr("size", in: attrs).flatMap(Double.init),
            foregroundHex: attr("color", in: attrs),
            backgroundHex: attr("bgColor", in: attrs),
            linkURL: attr("link", in: attrs)
          )
        )
      }
      searchStart = closeRange.upperBound
    }

    if !runs.isEmpty && searchStart < innerXML.endIndex {
      let trailingSlice = String(innerXML[searchStart..<innerXML.endIndex])
      let trailingText = extractODFText(from: trailingSlice)
      if !trailingText.isEmpty {
        runs.append(DocumentTextRun(text: trailingText))
      }
    }
    return runs
  }

  static func tableAttributes(_ tbl: DocumentTableBlock) -> String {
    "eo:id=\"\(tbl.id.uuidString)\" eo:caption=\"\(XMLSanitizer.escape(tbl.caption))\" eo:headerRow=\"\(tbl.hasHeaderRow ? "1" : "0")\" eo:borderStyle=\"\(XMLSanitizer.escape(tbl.borderStyle.rawValue))\""
  }

  static func serializeTableSpans(_ tbl: DocumentTableBlock) -> String {
    guard !tbl.cellSpans.isEmpty else { return "" }
    return tbl.cellSpans.map { span in
      "<eo:cellSpan row=\"\(span.rowIndex)\" col=\"\(span.columnIndex)\" rowSpan=\"\(span.rowSpan)\" colSpan=\"\(span.columnSpan)\" border=\"\(XMLSanitizer.escape(span.borderStyle.rawValue))\"/>"
    }.joined(separator: "\n")
  }

  static func parseTable(
    attributes: [String: String], innerXML: String, rowTag: String, cellTag: String,
    cellTextTag: String?
  ) -> DocumentTableBlock {
    let id = attr("id", in: attributes).flatMap(UUID.init(uuidString:)) ?? UUID()
    let caption = attr("caption", in: attributes) ?? ""
    let hasHeader = (attr("headerRow", in: attributes) ?? "1") == "1"
    let borderStyle = TableCellBorderStyle(rawValue: attr("borderStyle", in: attributes) ?? "") ?? .solid

    var rows: [[String]] = []
    for trElem in XMLSanitizer.extractElements(named: rowTag, in: innerXML) {
      let rowCells = XMLSanitizer.extractElements(named: cellTag, in: trElem.innerXML).map {
        tcElem -> String in
        if let textTag = cellTextTag {
          let extracted = XMLSanitizer.extractElements(named: textTag, in: tcElem.innerXML)
          if !extracted.isEmpty {
            return extracted.map { XMLSanitizer.unescape($0.innerXML) }.joined()
          }
        }
        let hpParagraphs = XMLSanitizer.extractElements(named: "hp:p", in: tcElem.innerXML)
        if !hpParagraphs.isEmpty {
          return hpParagraphs.map { pElem -> String in
            let tElems = XMLSanitizer.extractElements(named: "hp:t", in: pElem.innerXML)
            if !tElems.isEmpty {
              return tElems.map { XMLSanitizer.unescape($0.innerXML) }.joined()
            }
            return stripAllXMLTags(pElem.innerXML)
          }.joined(separator: "\n")
        }
        let hpTexts = XMLSanitizer.extractElements(named: "hp:t", in: tcElem.innerXML)
        if !hpTexts.isEmpty {
          return hpTexts.map { XMLSanitizer.unescape($0.innerXML) }.joined()
        }
        let wTexts = XMLSanitizer.extractElements(named: "w:t", in: tcElem.innerXML)
        if !wTexts.isEmpty {
          return wTexts.map { XMLSanitizer.unescape($0.innerXML) }.joined()
        }
        let pFallback = XMLSanitizer.extractElements(named: "text:p", in: tcElem.innerXML)
        if !pFallback.isEmpty {
          return pFallback.map { extractODFText(from: $0.innerXML) }.joined(separator: "\n")
        }
        return stripAllXMLTags(tcElem.innerXML)
      }
      rows.append(rowCells)
    }

    let cellSpans = XMLSanitizer.extractElements(named: "eo:cellSpan", in: innerXML).map {
      spanElem in
      TableCellSpan(
        rowIndex: Int(spanElem.attributes["row"] ?? "0") ?? 0,
        columnIndex: Int(spanElem.attributes["col"] ?? "0") ?? 0,
        rowSpan: Int(spanElem.attributes["rowSpan"] ?? "1") ?? 1,
        columnSpan: Int(spanElem.attributes["columnSpan"] ?? spanElem.attributes["colSpan"] ?? "1")
          ?? 1,
        borderStyle: TableCellBorderStyle(rawValue: spanElem.attributes["border"] ?? "")
          ?? borderStyle
      )
    }

    return DocumentTableBlock(
      id: id, caption: caption, rows: rows, hasHeaderRow: hasHeader, cellSpans: cellSpans,
      borderStyle: borderStyle
    )
  }

  static func serializeExtendedMetadata(
    _ doc: OfficeDocumentModel,
    resolvedMedia: [ResolvedMediaEntry]
  ) -> String {
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
      let repliesXML = cmt.replies.map { "<eo:reply>\(XMLSanitizer.escape($0))</eo:reply>" }
        .joined()
      tags.append(
        "<eo:comment id=\"\(cmt.id.uuidString)\" anchor=\"\(cmt.anchorParagraphID.uuidString)\" author=\"\(XMLSanitizer.escape(cmt.author))\" message=\"\(XMLSanitizer.escape(cmt.message))\" resolved=\"\(cmt.isResolved ? "1" : "0")\">\(repliesXML)</eo:comment>"
      )
    }
    for (name, targetID) in doc.bookmarks.sorted(by: { $0.key < $1.key }) {
      tags.append(
        "<eo:bookmark name=\"\(XMLSanitizer.escape(name))\" target=\"\(targetID.uuidString)\"/>")
    }
    for media in resolvedMedia {
      let img = media.attachment
      tags.append(
        "<eo:image id=\"\(img.id.uuidString)\" fileName=\"\(XMLSanitizer.escape(media.originalSafeFileName))\" path=\"\(XMLSanitizer.escape(media.archivePath))\" mime=\"\(XMLSanitizer.escape(img.mimeType))\" w=\"\(img.widthPt)\" h=\"\(img.heightPt)\" caption=\"\(XMLSanitizer.escape(img.caption))\" anchor=\"\(img.anchorParagraphID?.uuidString ?? "")\" wrapMode=\"\(XMLSanitizer.escape(img.wrapMode.rawValue))\"/>"
      )
    }
    for eq in doc.equations {
      tags.append(
        "<eo:equation id=\"\(eq.id.uuidString)\" title=\"\(XMLSanitizer.escape(eq.displayTitle))\">\(XMLSanitizer.escape(eq.rawExpression))</eo:equation>"
      )
    }
    if let wm = doc.watermarkText, !wm.isEmpty {
      tags.append("<eo:watermark>\(XMLSanitizer.escape(wm))</eo:watermark>")
    }
    return tags.joined(separator: "\n")
  }

  static func parseExtendedMetadata(
    from xml: String, defaultPaperSize: PaperSizePreset, binaryEntries: [String: Data] = [:]
  ) -> (
    sections: [DocumentSectionLayout], footnotes: [FootnoteItem], revisions: [TrackedRevision],
    comments: [DocumentCommentThread], bookmarks: [String: UUID], images: [EmbeddedImageAttachment],
    equations: [DocumentEquationBlock], watermarkText: String?
  ) {
    let sections = XMLSanitizer.extractElements(named: "eo:section", in: xml).map { elem in
      DocumentSectionLayout(
        id: elem.attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID(),
        paperSize: PaperSizePreset(rawValue: elem.attributes["paperSize"] ?? "")
          ?? defaultPaperSize,
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
        author: elem.attributes["author"] ?? "", message: elem.attributes["message"] ?? "",
        replies: replies, isResolved: elem.attributes["resolved"] == "1"
      )
    }
    var bookmarks: [String: UUID] = [:]
    for elem in XMLSanitizer.extractElements(named: "eo:bookmark", in: xml) {
      if let name = elem.attributes["name"],
        let target = elem.attributes["target"].flatMap(UUID.init(uuidString:))
      {
        bookmarks[name] = target
      }
    }
    let images = XMLSanitizer.extractElements(named: "eo:image", in: xml).compactMap {
      elem -> EmbeddedImageAttachment? in
      let fileName = elem.attributes["fileName"] ?? "image.png"
      let path = elem.attributes["path"] ?? ""
      let data =
        binaryEntries[path] ?? binaryEntries["word/media/\(fileName)"] ?? binaryEntries[
          "Pictures/\(fileName)"] ?? binaryEntries["BinData/\(fileName)"] ?? Data()
      let anchorStr = elem.attributes["anchor"] ?? ""
      return EmbeddedImageAttachment(
        id: elem.attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID(),
        fileName: fileName, caption: elem.attributes["caption"] ?? "",
        mimeType: elem.attributes["mime"] ?? "image/png", imageData: data,
        widthPt: Double(elem.attributes["w"] ?? "320.0") ?? 320.0,
        heightPt: Double(elem.attributes["h"] ?? "200.0") ?? 200.0,
        anchorParagraphID: anchorStr.isEmpty ? nil : UUID(uuidString: anchorStr),
        wrapMode: ImageWrapMode(rawValue: elem.attributes["wrapMode"] ?? "") ?? .inline
      )
    }
    let equations = XMLSanitizer.extractElements(named: "eo:equation", in: xml).map { elem in
      DocumentEquationBlock(
        id: elem.attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID(),
        displayTitle: elem.attributes["title"] ?? "수식 블록",
        rawExpression: XMLSanitizer.unescape(elem.innerXML)
      )
    }
    let watermarkText = XMLSanitizer.extractElements(named: "eo:watermark", in: xml).first.map {
      XMLSanitizer.unescape($0.innerXML)
    }
    return (
      sections: sections.isEmpty ? [DocumentSectionLayout(paperSize: defaultPaperSize)] : sections,
      footnotes: footnotes, revisions: revisions, comments: comments, bookmarks: bookmarks,
      images: images, equations: equations, watermarkText: watermarkText
    )
  }

  static func cellStyleAttributes(_ style: CellStyle?) -> String {
    guard let style, style != CellStyle() else { return "" }
    var attrs: [String] = [
      "eo:bold=\"\(style.isBold ? "1" : "0")\"",
      "eo:italic=\"\(style.isItalic ? "1" : "0")\"",
      "eo:underline=\"\(style.isUnderlined ? "1" : "0")\"",
      "eo:align=\"\(XMLSanitizer.escape(style.alignment.rawValue))\"",
      "eo:numFmt=\"\(XMLSanitizer.escape(style.numberFormat.rawValue))\"",
    ]
    if let fill = style.fillHex { attrs.append("eo:fill=\"\(XMLSanitizer.escape(fill))\"") }
    if let text = style.textHex { attrs.append("eo:text=\"\(XMLSanitizer.escape(text))\"") }
    if style.wrapText { attrs.append("eo:wrap=\"1\"") }
    if let dec = style.decimalPlaces { attrs.append("eo:decimals=\"\(dec)\"") }
    return " " + attrs.joined(separator: " ")
  }

  static func parseCellStyle(from attributes: [String: String]) -> CellStyle? {
    let boldVal = attributes["eo:bold"] ?? attributes["eoBold"]
    let italicVal = attributes["eo:italic"] ?? attributes["eoItalic"]
    let underlineVal = attributes["eo:underline"] ?? attributes["eoUnderline"]
    let alignVal = attributes["eo:align"] ?? attributes["eoAlign"]
    let numFmtVal = attributes["eo:numFmt"] ?? attributes["eoNumFmt"]
    let fillVal = attributes["eo:fill"] ?? attributes["eoFill"]
    let textVal = attributes["eo:text"] ?? attributes["eoText"]
    let wrapVal = attributes["eo:wrap"] ?? attributes["eoWrap"]
    let decimalsVal = attributes["eo:decimals"] ?? attributes["eoDecimals"]
    guard
      boldVal != nil || italicVal != nil || underlineVal != nil || alignVal != nil
        || numFmtVal != nil || fillVal != nil || textVal != nil || wrapVal != nil
        || decimalsVal != nil
    else {
      return nil
    }
    return CellStyle(
      isBold: boldVal == "1",
      isItalic: italicVal == "1",
      isUnderlined: underlineVal == "1",
      alignment: CellHorizontalAlignment(rawValue: alignVal ?? "") ?? .leading,
      numberFormat: CellNumberFormat(rawValue: numFmtVal ?? "") ?? .general,
      fillHex: fillVal,
      textHex: textVal,
      wrapText: wrapVal == "1",
      decimalPlaces: decimalsVal.flatMap(Int.init)
    )
  }

  static func serializeWorksheetMetadata(_ sheet: WorksheetModel) -> String {
    let ps = sheet.pageSetup
    var lines: [String] = [
      "<eo:sheetMeta activeIndex=\"\(sheet.activeSheetIndex)\" trackChanges=\"\(sheet.trackChangesEnabled ? "1" : "0")\" paperSize=\"\(XMLSanitizer.escape(ps.paperSize.rawValue))\" orientation=\"\(XMLSanitizer.escape(ps.orientation.rawValue))\" multiCol=\"\(ps.isMultiColumnPrint ? "1" : "0")\" marginPreset=\"\(XMLSanitizer.escape(ps.marginPresetName))\"/>"
    ]
    let orderedPages = sheet.orderedSheetPages()
    for (sheetIdx, page) in orderedPages.enumerated() {
      for coord in page.sheetComments.keys.sorted() {
        if let comment = page.sheetComments[coord] {
          lines.append(
            "<eo:cellComment sheetIdx=\"\(sheetIdx)\" addr=\"\(XMLSanitizer.escape(coord.description))\" text=\"\(XMLSanitizer.escape(comment))\"/>"
          )
        }
      }
      for range in page.mergedRanges {
        lines.append(
          "<eo:mergedRange sheetIdx=\"\(sheetIdx)\" ref=\"\(XMLSanitizer.escape(range.description))\"/>"
        )
      }
      for col in page.columnWidths.keys.sorted() {
        if let w = page.columnWidths[col] {
          lines.append("<eo:colWidth sheetIdx=\"\(sheetIdx)\" col=\"\(col)\" width=\"\(w)\"/>")
        }
      }
      for row in page.rowHeights.keys.sorted() {
        if let h = page.rowHeights[row] {
          lines.append("<eo:rowHeight sheetIdx=\"\(sheetIdx)\" row=\"\(row)\" height=\"\(h)\"/>")
        }
      }
      if !page.conditionalFormatRules.isEmpty,
        let encodedCF = try? JSONEncoder().encode(page.conditionalFormatRules).base64EncodedString()
      {
        lines.append(
          "<eo:condFormats sheetIdx=\"\(sheetIdx)\" payload=\"\(XMLSanitizer.escape(encodedCF))\"/>"
        )
      }
      if let dv = page.dataValidationRule,
        let encodedDV = try? JSONEncoder().encode(dv).base64EncodedString()
      {
        lines.append(
          "<eo:dataValidation sheetIdx=\"\(sheetIdx)\" payload=\"\(XMLSanitizer.escape(encodedDV))\"/>"
        )
      }
    }
    for key in sheet.namedRanges.keys.sorted() {
      if let ref = sheet.namedRanges[key] {
        lines.append(
          "<eo:namedRange name=\"\(XMLSanitizer.escape(key))\" ref=\"\(XMLSanitizer.escape(ref))\"/>"
        )
      }
    }
    return lines.joined(separator: "\n  ")
  }

  static func applyWorksheetMetadata(from xml: String, to sheet: inout WorksheetModel) {
    for nrElem in XMLSanitizer.extractElements(named: "eo:namedRange", in: xml) {
      if let name = nrElem.attributes["name"], let ref = nrElem.attributes["ref"] {
        sheet.setNamedRange(name, rangeOrCellNotation: ref)
      }
    }
    for defElem in XMLSanitizer.extractElements(named: "definedName", in: xml) {
      if let name = defElem.attributes["name"] {
        let rawRef = XMLSanitizer.unescape(defElem.innerXML).trimmingCharacters(
          in: .whitespacesAndNewlines)
        let cleanedRef = rawRef.replacingOccurrences(of: "$", with: "")
        let finalRef: String
        if let bangIdx = cleanedRef.firstIndex(of: "!") {
          let prefixSheet = cleanedRef[..<bangIdx].trimmingCharacters(
            in: CharacterSet(charactersIn: "'\" "))
          if prefixSheet.caseInsensitiveCompare(sheet.name) == .orderedSame {
            finalRef = String(cleanedRef[cleanedRef.index(after: bangIdx)...])
          } else {
            finalRef = cleanedRef
          }
        } else {
          finalRef = cleanedRef
        }
        if !finalRef.isEmpty {
          sheet.setNamedRange(name, rangeOrCellNotation: finalRef)
        }
      }
    }
    for commentElem in XMLSanitizer.extractElements(named: "eo:cellComment", in: xml) {
      guard let addrStr = commentElem.attributes["addr"],
        let text = commentElem.attributes["text"],
        let addr = try? CellAddress(a1Notation: addrStr)
      else { continue }
      let sheetIdx = Int(commentElem.attributes["sheetIdx"] ?? "0") ?? 0
      if sheetIdx == 0 {
        sheet.sheetComments[addr.packedCoordinate] = text
      } else if sheetIdx > 0, sheetIdx - 1 < sheet.additionalSheets.count {
        sheet.additionalSheets[sheetIdx - 1].sheetComments[addr.packedCoordinate] = text
      }
    }
    for mergeElem in XMLSanitizer.extractElements(named: "eo:mergedRange", in: xml) {
      guard let refStr = mergeElem.attributes["ref"],
        let range = try? CellRange(notation: refStr)
      else { continue }
      let sheetIdx = Int(mergeElem.attributes["sheetIdx"] ?? "0") ?? 0
      if sheetIdx == 0 {
        if !sheet.mergedRanges.contains(range) {
          sheet.mergedRanges.append(range)
        }
      } else if sheetIdx > 0, sheetIdx - 1 < sheet.additionalSheets.count {
        if !sheet.additionalSheets[sheetIdx - 1].mergedRanges.contains(range) {
          sheet.additionalSheets[sheetIdx - 1].mergedRanges.append(range)
        }
      }
    }
    for colElem in XMLSanitizer.extractElements(named: "eo:colWidth", in: xml) {
      guard let col = Int(colElem.attributes["col"] ?? ""),
        col >= 0, col <= CellAddress.maxZeroBasedColumn,
        let w = Double(colElem.attributes["width"] ?? ""),
        w.isFinite
      else { continue }
      let sheetIdx = Int(colElem.attributes["sheetIdx"] ?? "0") ?? 0
      if sheetIdx == 0 {
        sheet.setColumnWidth(w, at: col)
      } else if sheetIdx > 0, sheetIdx - 1 < sheet.additionalSheets.count {
        sheet.additionalSheets[sheetIdx - 1].columnWidths[col] = max(48.0, min(w, 420.0))
      }
    }
    for rowElem in XMLSanitizer.extractElements(named: "eo:rowHeight", in: xml) {
      guard let row = Int(rowElem.attributes["row"] ?? ""),
        row >= 0, row <= CellAddress.maxZeroBasedRow,
        let h = Double(rowElem.attributes["height"] ?? ""),
        h.isFinite
      else { continue }
      let sheetIdx = Int(rowElem.attributes["sheetIdx"] ?? "0") ?? 0
      if sheetIdx == 0 {
        sheet.setRowHeight(h, at: row)
      } else if sheetIdx > 0, sheetIdx - 1 < sheet.additionalSheets.count {
        sheet.additionalSheets[sheetIdx - 1].rowHeights[row] = max(22.0, min(h, 180.0))
      }
    }
    for cfElem in XMLSanitizer.extractElements(named: "eo:condFormats", in: xml) {
      guard let payloadB64 = cfElem.attributes["payload"],
        let data = Data(base64Encoded: payloadB64),
        let rules = try? JSONDecoder().decode([SheetConditionalFormatRule].self, from: data)
      else { continue }
      let sheetIdx = Int(cfElem.attributes["sheetIdx"] ?? "0") ?? 0
      if sheetIdx == 0 {
        sheet.conditionalFormatRules = rules
      } else if sheetIdx > 0, sheetIdx - 1 < sheet.additionalSheets.count {
        sheet.additionalSheets[sheetIdx - 1].conditionalFormatRules = rules
      }
    }
    for dvElem in XMLSanitizer.extractElements(named: "eo:dataValidation", in: xml) {
      guard let payloadB64 = dvElem.attributes["payload"],
        let data = Data(base64Encoded: payloadB64),
        let rule = try? JSONDecoder().decode(SheetDataValidationRule.self, from: data)
      else { continue }
      let sheetIdx = Int(dvElem.attributes["sheetIdx"] ?? "0") ?? 0
      if sheetIdx == 0 {
        sheet.dataValidationRule = rule
      } else if sheetIdx > 0, sheetIdx - 1 < sheet.additionalSheets.count {
        sheet.additionalSheets[sheetIdx - 1].dataValidationRule = rule
      }
    }
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
  }

  static func slideElementAttributes(_ el: SlideCanvasElement) -> String {
    "eo:id=\"\(el.id.uuidString)\" eo:title=\"\(XMLSanitizer.escape(el.title))\" eo:x=\"\(el.x)\" eo:y=\"\(el.y)\" eo:w=\"\(el.width)\" eo:h=\"\(el.height)\" eo:kind=\"\(XMLSanitizer.escape(el.kind.rawValue))\" eo:zIndex=\"\(el.zIndex)\" eo:rotation=\"\(el.rotationDegrees)\" eo:anim=\"\(XMLSanitizer.escape(el.animation.rawValue))\" eo:fillHex=\"\(XMLSanitizer.escape(el.fillHex))\" eo:strokeHex=\"\(XMLSanitizer.escape(el.strokeHex))\" eo:bold=\"\(el.isBold ? "1" : "0")\" eo:italic=\"\(el.isItalic ? "1" : "0")\" eo:underline=\"\(el.isUnderlined ? "1" : "0")\""
  }

  static func parseSlideElement(attributes: [String: String], innerXML: String)
    -> SlideCanvasElement
  {
    let id = attr("id", in: attributes).flatMap(UUID.init(uuidString:)) ?? UUID()
    let cNvPrAttrs = XMLSanitizer.extractElements(named: "p:cNvPr", in: innerXML).first?.attributes
    let offAttrs = XMLSanitizer.extractElements(named: "a:off", in: innerXML).first?.attributes
    let extAttrs = XMLSanitizer.extractElements(named: "a:ext", in: innerXML).first?.attributes
    let rPrAttrs = XMLSanitizer.extractElements(named: "a:rPr", in: innerXML).first?.attributes

    let title = attr("title", in: attributes) ?? cNvPrAttrs?["name"] ?? ""
    let x = Double(attr("x", in: attributes) ?? offAttrs?["x"] ?? "0") ?? 0
    let y = Double(attr("y", in: attributes) ?? offAttrs?["y"] ?? "0") ?? 0
    let width = Double(attr("w", in: attributes) ?? extAttrs?["cx"] ?? "100") ?? 100
    let height = Double(attr("h", in: attributes) ?? extAttrs?["cy"] ?? "60") ?? 60
    let kind = SlideElementKind(rawValue: attr("kind", in: attributes) ?? "") ?? .textBox
    let zIndex = Int(attr("zIndex", in: attributes) ?? "0") ?? 0
    let rotation = Double(attr("rotation", in: attributes) ?? "0") ?? 0
    let animation = SlideElementAnimation(rawValue: attr("anim", in: attributes) ?? "") ?? .none
    let fillHex = attr("fillHex", in: attributes) ?? "#EEF0FF"
    let strokeHex = attr("strokeHex", in: attributes) ?? "#5C6BC0"
    let isBold = (attr("bold", in: attributes) == "1") || (rPrAttrs?["b"] == "1")
    let isItalic = (attr("italic", in: attributes) == "1") || (rPrAttrs?["i"] == "1")
    let isUnderlined =
      (attr("underline", in: attributes) == "1")
      || ((rPrAttrs?["u"] ?? "none") != "none" && rPrAttrs?["u"] != nil)

    let drawingTextRuns = XMLSanitizer.extractElements(named: "a:t", in: innerXML)
    let bodyText: String
    if !drawingTextRuns.isEmpty {
      bodyText = drawingTextRuns.map { XMLSanitizer.unescape($0.innerXML) }.joined()
    } else if innerXML.contains("<text:") {
      let textParagraphs = XMLSanitizer.extractElements(named: "text:p", in: innerXML)
      if !textParagraphs.isEmpty {
        bodyText = textParagraphs.map { extractODFText(from: $0.innerXML) }.joined(separator: "\n")
      } else {
        bodyText = extractODFText(from: innerXML)
      }
    } else {
      bodyText = stripAllXMLTags(innerXML)
    }

    return SlideCanvasElement(
      id: id, title: title, bodyText: bodyText, x: x, y: y, width: width, height: height,
      kind: kind, zIndex: zIndex, rotationDegrees: rotation, animation: animation,
      fillHex: fillHex, strokeHex: strokeHex,
      isBold: isBold, isItalic: isItalic, isUnderlined: isUnderlined
    )
  }

  /// Extracts `<table:table-cell>` and `<table:covered-table-cell>` elements in document order within a `<table:table-row>`.
  static func extractODSTableRowCells(in rowInnerXML: String) -> [(
    isCovered: Bool, attributes: [String: String], innerXML: String
  )] {
    var results: [(isCovered: Bool, attributes: [String: String], innerXML: String)] = []
    var cursor = rowInnerXML.startIndex
    let cellPrefix = "<table:table-cell"
    let coveredPrefix = "<table:covered-table-cell"

    while cursor < rowInnerXML.endIndex {
      let nextCell = rowInnerXML.range(of: cellPrefix, range: cursor..<rowInnerXML.endIndex)
      let nextCovered = rowInnerXML.range(of: coveredPrefix, range: cursor..<rowInnerXML.endIndex)

      let isCovered: Bool
      let matchRange: Range<String.Index>
      let tagName: String
      if let cRange = nextCell, let covRange = nextCovered {
        if cRange.lowerBound <= covRange.lowerBound {
          isCovered = false
          matchRange = cRange
          tagName = "table:table-cell"
        } else {
          isCovered = true
          matchRange = covRange
          tagName = "table:covered-table-cell"
        }
      } else if let cRange = nextCell {
        isCovered = false
        matchRange = cRange
        tagName = "table:table-cell"
      } else if let covRange = nextCovered {
        isCovered = true
        matchRange = covRange
        tagName = "table:covered-table-cell"
      } else {
        break
      }

      let afterName = matchRange.upperBound
      if afterName < rowInnerXML.endIndex {
        let ch = rowInnerXML[afterName]
        guard ch == " " || ch == ">" || ch == "/" || ch == "\t" || ch == "\n" || ch == "\r" else {
          cursor = afterName
          continue
        }
      }

      var scan = afterName
      var activeQuote: Character? = nil
      var gtIdx: String.Index? = nil
      while scan < rowInnerXML.endIndex {
        let ch = rowInnerXML[scan]
        if let q = activeQuote {
          if ch == q { activeQuote = nil }
        } else if ch == "\"" || ch == "'" {
          activeQuote = ch
        } else if ch == ">" {
          gtIdx = scan
          break
        }
        scan = rowInnerXML.index(after: scan)
      }
      guard let resolvedGt = gtIdx else { break }
      let attrSlice = rowInnerXML[afterName..<resolvedGt]
      let attrs = XMLSanitizer.parseAttributes(from: attrSlice)
      let afterGt = rowInnerXML.index(after: resolvedGt)

      if XMLSanitizer.isSelfClosingTag(attrSlice) {
        results.append((isCovered: isCovered, attributes: attrs, innerXML: ""))
        cursor = afterGt
        continue
      }

      let openTagPrefix = "<\(tagName)"
      let closeTag = "</\(tagName)>"
      var depth = 1
      var searchPos = afterGt
      var matchingClose: Range<String.Index>? = nil
      while searchPos < rowInnerXML.endIndex && depth > 0 {
        let nextOpen = rowInnerXML.range(of: openTagPrefix, range: searchPos..<rowInnerXML.endIndex)
        let nextClose = rowInnerXML.range(of: closeTag, range: searchPos..<rowInnerXML.endIndex)
        guard let cRange = nextClose else { break }
        if let oRange = nextOpen, oRange.lowerBound < cRange.lowerBound {
          let afterOpenName = oRange.upperBound
          let isBoundary: Bool
          if afterOpenName < rowInnerXML.endIndex {
            let ch = rowInnerXML[afterOpenName]
            isBoundary = ch == " " || ch == ">" || ch == "/" || ch == "\t" || ch == "\n" || ch == "\r"
          } else {
            isBoundary = false
          }
          if isBoundary {
            var innerScan = afterOpenName
            var innerQuote: Character? = nil
            var innerGt: String.Index? = nil
            while innerScan < rowInnerXML.endIndex {
              let ch = rowInnerXML[innerScan]
              if let q = innerQuote {
                if ch == q { innerQuote = nil }
              } else if ch == "\"" || ch == "'" {
                innerQuote = ch
              } else if ch == ">" {
                innerGt = innerScan
                break
              }
              innerScan = rowInnerXML.index(after: innerScan)
            }
            if let gt = innerGt {
              if !XMLSanitizer.isSelfClosingTag(rowInnerXML[afterOpenName..<gt]) {
                depth += 1
              }
              searchPos = rowInnerXML.index(after: gt)
              continue
            }
          }
          searchPos = afterOpenName
          continue
        }
        depth -= 1
        if depth == 0 {
          matchingClose = cRange
          break
        }
        searchPos = cRange.upperBound
      }
      guard let closeRange = matchingClose else { break }
      let inner = String(rowInnerXML[afterGt..<closeRange.lowerBound])
      results.append((isCovered: isCovered, attributes: attrs, innerXML: inner))
      cursor = closeRange.upperBound
    }
    return results
  }

  /// Converts an EasyOffice `=...` formula into an OASIS OpenFormula `of:=...` expression with bracketed `[.A1:.B2]` / `[.A1]` references.
  public static func toOpenFormulaExpression(_ rawFormula: String) -> String {
    let trimmed = rawFormula.trimmingCharacters(in: .whitespacesAndNewlines)
    if trimmed.lowercased().hasPrefix("of:") || trimmed.lowercased().hasPrefix("oooc:") {
      return trimmed
    }
    let body = trimmed.hasPrefix("=") ? String(trimmed.dropFirst()) : trimmed
    var out = "of:="
    out.reserveCapacity(body.count + 16)

    var inQuotes = false
    var idx = body.startIndex

    while idx < body.endIndex {
      let ch = body[idx]
      if ch == "\"" {
        inQuotes.toggle()
        out.append(ch)
        idx = body.index(after: idx)
        continue
      }
      if inQuotes {
        out.append(ch)
        idx = body.index(after: idx)
        continue
      }
      if ch == "[" {
        var closeIdx = body.index(after: idx)
        var inSq = false
        while closeIdx < body.endIndex {
          if body[closeIdx] == "'" {
            inSq.toggle()
          } else if body[closeIdx] == "]" && !inSq {
            closeIdx = body.index(after: closeIdx)
            break
          }
          closeIdx = body.index(after: closeIdx)
        }
        out.append(contentsOf: body[idx..<closeIdx])
        idx = closeIdx
        continue
      }

      if ch.isLetter || ch == "$" || ch == "'" {
        var tokenEnd = idx
        var inSingleQuote = false
        while tokenEnd < body.endIndex {
          let c = body[tokenEnd]
          if c == "'" {
            inSingleQuote.toggle()
            tokenEnd = body.index(after: tokenEnd)
            continue
          }
          if inSingleQuote {
            tokenEnd = body.index(after: tokenEnd)
            continue
          }
          if c.isLetter || c.isNumber || c == "$" || c == "!" || c == ":" || c == "_" || c == "." {
            tokenEnd = body.index(after: tokenEnd)
          } else {
            break
          }
        }
        let token = String(body[idx..<tokenEnd])
        var lookahead = tokenEnd
        while lookahead < body.endIndex && body[lookahead].isWhitespace {
          lookahead = body.index(after: lookahead)
        }
        let isFunctionCall = (lookahead < body.endIndex && body[lookahead] == "(")
        if !isFunctionCall, let bracketed = convertA1TokenToOpenFormulaBracketed(token) {
          out.append(bracketed)
        } else {
          out.append(token)
        }
        idx = tokenEnd
        continue
      }

      out.append(ch)
      idx = body.index(after: idx)
    }
    return out
  }

  private static func convertA1TokenToOpenFormulaBracketed(_ token: String) -> String? {
    func parseSingleRef(_ part: String) -> (sheet: String?, cell: String)? {
      let sheet: String?
      let cellPart: String
      if let (rawLeft, rawRight) = splitOnceUnquoted(part, delimiter: "!") {
        var unq = rawLeft.trimmingCharacters(in: .whitespaces)
        if unq.count >= 2 && unq.hasPrefix("'") && unq.hasSuffix("'") {
          unq = String(unq.dropFirst().dropLast()).replacingOccurrences(of: "''", with: "'")
        }
        sheet = unq.isEmpty ? nil : unq
        cellPart = rawRight
      } else {
        sheet = nil
        cellPart = part
      }
      let strippedDollars = cellPart.replacingOccurrences(of: "$", with: "")
      guard (try? CellAddress(a1Notation: strippedDollars)) != nil else {
        return nil
      }
      return (sheet, cellPart)
    }

    if let (lhsStr, rhsStr) = splitOnceUnquoted(token, delimiter: ":") {
      guard let left = parseSingleRef(lhsStr), let right = parseSingleRef(rhsStr) else {
        return nil
      }
      let leftPrefix = left.sheet.map(formatOpenFormulaSheetPrefix) ?? "."
      let rightPrefix: String
      if let rSheet = right.sheet, rSheet != left.sheet {
        rightPrefix = formatOpenFormulaSheetPrefix(rSheet)
      } else {
        rightPrefix = "."
      }
      return "[\(leftPrefix)\(left.cell):\(rightPrefix)\(right.cell)]"
    }
    guard let single = parseSingleRef(token) else { return nil }
    let prefix = single.sheet.map(formatOpenFormulaSheetPrefix) ?? "."
    return "[\(prefix)\(single.cell)]"
  }
}
