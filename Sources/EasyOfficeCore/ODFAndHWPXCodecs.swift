import Foundation

// MARK: - Worksheet & Presentation XML Mapper Extensions

extension XMLDocumentModelMapper {

  static func cellStyleAttributes(_ style: CellStyle?) -> String {
    guard let style, style != CellStyle() else { return "" }
    var attrs: [String] = [
      "eoBold=\"\(style.isBold ? "1" : "0")\"",
      "eoItalic=\"\(style.isItalic ? "1" : "0")\"",
      "eoUnderline=\"\(style.isUnderlined ? "1" : "0")\"",
      "eoAlign=\"\(XMLSanitizer.escape(style.alignment.rawValue))\"",
      "eoNumFmt=\"\(XMLSanitizer.escape(style.numberFormat.rawValue))\"",
    ]
    if let fill = style.fillHex { attrs.append("eoFill=\"\(XMLSanitizer.escape(fill))\"") }
    if let text = style.textHex { attrs.append("eoText=\"\(XMLSanitizer.escape(text))\"") }
    if style.wrapText { attrs.append("eoWrap=\"1\"") }
    if let dec = style.decimalPlaces { attrs.append("eoDecimals=\"\(dec)\"") }
    return " " + attrs.joined(separator: " ")
  }

  static func parseCellStyle(from attributes: [String: String]) -> CellStyle? {
    guard
      attributes["eoBold"] != nil || attributes["eoItalic"] != nil
        || attributes["eoUnderline"] != nil || attributes["eoAlign"] != nil
        || attributes["eoNumFmt"] != nil || attributes["eoFill"] != nil
        || attributes["eoText"] != nil || attributes["eoWrap"] != nil
        || attributes["eoDecimals"] != nil
    else {
      return nil
    }
    return CellStyle(
      isBold: attributes["eoBold"] == "1",
      isItalic: attributes["eoItalic"] == "1",
      isUnderlined: attributes["eoUnderline"] == "1",
      alignment: CellHorizontalAlignment(rawValue: attributes["eoAlign"] ?? "") ?? .leading,
      numberFormat: CellNumberFormat(rawValue: attributes["eoNumFmt"] ?? "") ?? .general,
      fillHex: attributes["eoFill"],
      textHex: attributes["eoText"],
      wrapText: attributes["eoWrap"] == "1",
      decimalPlaces: attributes["eoDecimals"].flatMap(Int.init)
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
    var normalized = ""
    normalized.reserveCapacity(expr.count)
    var inQuotes = false
    var idx = expr.startIndex
    while idx < expr.endIndex {
      let ch = expr[idx]
      if ch == "\"" {
        inQuotes.toggle()
        normalized.append(ch)
        idx = expr.index(after: idx)
        continue
      }
      if !inQuotes {
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
        } else if ch == ";" {
          normalized.append(",")
          idx = expr.index(after: idx)
          continue
        }
      }
      normalized.append(ch)
      idx = expr.index(after: idx)
    }
    return normalized
  }

  static func slideElementAttributes(_ el: SlideCanvasElement) -> String {
    "id=\"\(el.id.uuidString)\" title=\"\(XMLSanitizer.escape(el.title))\" x=\"\(el.x)\" y=\"\(el.y)\" w=\"\(el.width)\" h=\"\(el.height)\" kind=\"\(XMLSanitizer.escape(el.kind.rawValue))\" zIndex=\"\(el.zIndex)\" rotation=\"\(el.rotationDegrees)\" anim=\"\(XMLSanitizer.escape(el.animation.rawValue))\" fillHex=\"\(XMLSanitizer.escape(el.fillHex))\" strokeHex=\"\(XMLSanitizer.escape(el.strokeHex))\" bold=\"\(el.isBold ? "1" : "0")\" italic=\"\(el.isItalic ? "1" : "0")\" underline=\"\(el.isUnderlined ? "1" : "0")\""
  }

  static func parseSlideElement(attributes: [String: String], innerXML: String)
    -> SlideCanvasElement
  {
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
    let animation = SlideElementAnimation(rawValue: attributes["anim"] ?? "") ?? .none
    let fillHex = attributes["fillHex"] ?? "#EEF0FF"
    let strokeHex = attributes["strokeHex"] ?? "#5C6BC0"
    let isBold = (attributes["bold"] == "1") || (rPrAttrs?["b"] == "1")
    let isItalic = (attributes["italic"] == "1") || (rPrAttrs?["i"] == "1")
    let isUnderlined =
      (attributes["underline"] == "1")
      || ((rPrAttrs?["u"] ?? "none") != "none" && rPrAttrs?["u"] != nil)

    let drawingTextRuns = XMLSanitizer.extractElements(named: "a:t", in: innerXML)
    let bodyText =
      !drawingTextRuns.isEmpty
      ? drawingTextRuns.map { XMLSanitizer.unescape($0.innerXML) }.joined()
      : XMLSanitizer.unescape(innerXML)

    return SlideCanvasElement(
      id: id, title: title, bodyText: bodyText, x: x, y: y, width: width, height: height,
      kind: kind, zIndex: zIndex, rotationDegrees: rotation, animation: animation,
      fillHex: fillHex, strokeHex: strokeHex,
      isBold: isBold, isItalic: isItalic, isUnderlined: isUnderlined
    )
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
      if p.runs.isEmpty || p.runs.map(\.text).joined() != p.text {
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
        let cells = row.map {
          "<table:table-cell><text:value>\(XMLSanitizer.escape($0))</text:value></table:table-cell>"
        }.joined()
        return "<table:table-row>\(cells)</table:table-row>"
      }.joined(separator: "\n        ")
      return
        "<table:table \(tAttrs)>\n        \(spansXML)\n        \(rowsXML)\n      </table:table>"
    }.joined(separator: "\n      ")

    let extendedMetaXML = XMLDocumentModelMapper.serializeExtendedMetadata(
      doc, mediaPrefix: "Pictures/")
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
      ZipContainerEntry(
        path: "content.xml", data: Data(contentXML.utf8), compressionMethod: .deflate),
    ]
    for img in doc.images {
      let safeName = XMLDocumentModelMapper.sanitizeMediaFileName(img.fileName)
      entries.append(
        ZipContainerEntry(
          path: "Pictures/\(safeName)", data: img.imageData, compressionMethod: .store))
    }
    return try ZipContainerCodec.pack(entries: entries)
  }

  public static func decodeDocument(from data: Data) throws -> OfficeDocumentModel {
    let entries = try ZipContainerCodec.unpack(from: data)
    guard let mimeEntry = entries.first(where: { $0.path == "mimetype" }),
      String(data: mimeEntry.data, encoding: .utf8) == textMimeType
    else {
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
    guard let root = XMLSanitizer.extractElements(named: "office:document-content", in: xml).first
    else {
      throw EasyOfficeError.corruptedContainer(
        detail: "ODT 'office:document-content' 루트를 찾을 수 없습니다.")
    }

    let tables = XMLSanitizer.extractElements(named: "table:table", in: root.innerXML).map {
      tblElem in
      XMLDocumentModelMapper.parseTable(
        attributes: tblElem.attributes, innerXML: tblElem.innerXML,
        rowTag: "table:table-row", cellTag: "table:table-cell", cellTextTag: "text:value"
      )
    }

    let bodyWithoutTables = XMLSanitizer.stripElements(named: "table:table", from: root.innerXML)
    let paragraphs = XMLSanitizer.extractElements(named: "text:p", in: bodyWithoutTables).map {
      elem -> ParagraphBlock in
      var parsed = XMLDocumentModelMapper.parseParagraph(
        attributes: elem.attributes, innerXML: elem.innerXML, childTextTag: nil)
      if let lsStr = elem.attributes["eo:letterSpacing"] ?? elem.attributes["fo:letter-spacing"],
        let lsVal = Double(lsStr.replacingOccurrences(of: "%", with: ""))
      {
        parsed.letterSpacingPercent = max(-50.0, min(50.0, lsVal))
      }
      if let csStr = elem.attributes["eo:charScale"] ?? elem.attributes["style:text-scale"],
        let csVal = Double(csStr.replacingOccurrences(of: "%", with: ""))
      {
        parsed.charWidthScalePercent = max(50.0, min(200.0, csVal))
      }
      return parsed
    }

    let paperSize = PaperSizePreset(rawValue: root.attributes["paperSize"] ?? "") ?? .a4
    let meta = XMLDocumentModelMapper.parseExtendedMetadata(
      from: root.innerXML, defaultPaperSize: paperSize, binaryEntries: binaryEntries)

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
      images: meta.images,
      equations: meta.equations,
      watermarkText: meta.watermarkText
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
            cellStrings.append(
              "<table:table-cell address=\"\(coord.description)\" raw=\"\(escapedRaw)\" table:formula=\"of:\(escapedRaw)\" office:value-type=\"string\"\(styleAttrs)><text:p>\(escapedRaw)</text:p></table:table-cell>"
            )
          } else if !raw.isEmpty, Double(raw)?.isFinite == true {
            let escapedRaw = XMLSanitizer.escape(raw)
            cellStrings.append(
              "<table:table-cell address=\"\(coord.description)\" raw=\"\(escapedRaw)\" office:value-type=\"float\" office:value=\"\(escapedRaw)\"\(styleAttrs)><text:p>\(escapedRaw)</text:p></table:table-cell>"
            )
          } else {
            let escapedRaw = XMLSanitizer.escape(raw)
            cellStrings.append(
              "<table:table-cell address=\"\(coord.description)\" raw=\"\(escapedRaw)\" office:value-type=\"string\"\(styleAttrs)><text:p>\(escapedRaw)</text:p></table:table-cell>"
            )
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
      ZipContainerEntry(
        path: "mimetype", data: Data(spreadsheetMimeType.utf8), compressionMethod: .store),
      ZipContainerEntry(
        path: "content.xml", data: Data(contentXML.utf8), compressionMethod: .deflate),
    ])
  }

  public static func decodeWorksheet(from data: Data) throws -> WorksheetModel {
    let entries = try ZipContainerCodec.unpack(from: data)
    guard let mimeEntry = entries.first(where: { $0.path == "mimetype" }),
      String(data: mimeEntry.data, encoding: .utf8) == spreadsheetMimeType
    else {
      throw EasyOfficeError.corruptedContainer(detail: "ODS 컨테이너의 mimetype 엔트리가 유효하지 않습니다.")
    }
    guard let contentEntry = entries.first(where: { $0.path == "content.xml" }) else {
      throw EasyOfficeError.corruptedContainer(detail: "ODS 컨테이너에 'content.xml'이 누락되었습니다.")
    }
    let xml = try XMLSanitizer.validateSafeXMLData(contentEntry.data)
    let tableElems = XMLSanitizer.extractElements(named: "table:table", in: xml)

    func decodeTableElement(_ tableElem: (attributes: [String: String], innerXML: String)) throws
      -> WorksheetModel
    {
      let name = tableElem.attributes["table:name"] ?? "Sheet1"
      var pageModel = WorksheetModel(name: name)
      let rowElems = XMLSanitizer.extractElements(named: "table:table-row", in: tableElem.innerXML)
      if !rowElems.isEmpty {
        var rowIdx = 0
        var populatedCellCount = 0
        for rowElem in rowElems {
          let rowRepeat = max(
            1, min(1024, Int(rowElem.attributes["table:number-rows-repeated"] ?? "1") ?? 1))
          var colIdx = 0
          let cellElems = XMLSanitizer.extractElements(
            named: "table:table-cell", in: rowElem.innerXML)
          for cellElem in cellElems {
            let colRepeat = max(
              1, min(1024, Int(cellElem.attributes["table:number-columns-repeated"] ?? "1") ?? 1))
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
              resolvedRaw = textPs.map { XMLSanitizer.unescape($0.innerXML) }.joined(
                separator: "\n")
            }

            let parsedStyle = XMLDocumentModelMapper.parseCellStyle(from: attrs)
            if resolvedRaw.isEmpty && parsedStyle == nil {
              colIdx += colRepeat
              continue
            }

            let expansionCount = rowRepeat * colRepeat
            guard populatedCellCount + expansionCount <= CellRange.maxExpandedCellCount else {
              throw EasyOfficeError.securityViolation(
                detail: "ODF(.ods) 반복 셀 확장 개수가 허용 상한(\(CellRange.maxExpandedCellCount)셀)을 초과했습니다."
              )
            }
            populatedCellCount += expansionCount

            for rOff in 0..<rowRepeat {
              for cOff in 0..<colRepeat {
                let targetRow = rowIdx + rOff
                let targetCol = colIdx + cOff
                if !resolvedRaw.isEmpty {
                  try pageModel.setCell(row: targetRow, column: targetCol, rawValue: resolvedRaw)
                }
                if let parsedStyle {
                  pageModel.setCellStyle(
                    parsedStyle, at: PackedCellCoordinate(row: targetRow, column: targetCol))
                }
              }
            }
            colIdx += colRepeat
          }
          rowIdx += rowRepeat
        }
      } else {
        for cellElem in XMLSanitizer.extractElements(
          named: "table:table-cell", in: tableElem.innerXML)
        {
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
          cellStyles: page.cellStyles,
          sheetComments: page.sheetComments,
          mergedRanges: page.mergedRanges
        )
      }
    }
    XMLDocumentModelMapper.applyWorksheetMetadata(from: xml, to: &primary)
    return primary
  }

  public static func encodePresentation(_ deck: PresentationDeckModel) throws -> Data {
    let pagesXML = deck.slides.map { slide -> String in
      let bullets = slide.bulletPoints.map {
        "<text:list-item>\(XMLSanitizer.escape($0))</text:list-item>"
      }.joined()
      let shapes = slide.elements.map { el in
        "<draw:frame \(XMLDocumentModelMapper.slideElementAttributes(el))>\(XMLSanitizer.escape(el.bodyText))</draw:frame>"
      }.joined()
      return
        "<draw:page id=\"\(slide.id.uuidString)\" title=\"\(XMLSanitizer.escape(slide.title))\" subtitle=\"\(XMLSanitizer.escape(slide.subtitle))\" notes=\"\(XMLSanitizer.escape(slide.speakerNotes))\" layout=\"\(XMLSanitizer.escape(slide.masterLayout.rawValue))\" transition=\"\(XMLSanitizer.escape(slide.transition.rawValue))\">\(bullets)\(shapes)</draw:page>"
    }.joined(separator: "\n      ")

    let contentXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:eo="urn:easyoffice:metadata:1.0" title="\(XMLSanitizer.escape(deck.title))" aspectRatio="\(XMLSanitizer.escape(deck.aspectRatio.rawValue))" eo:deckTheme="\(XMLSanitizer.escape(deck.deckTheme.rawValue))">
        <office:body>
          <office:presentation>
            \(pagesXML)
          </office:presentation>
        </office:body>
      </office:document-content>
      """

    return try ZipContainerCodec.pack(entries: [
      ZipContainerEntry(
        path: "mimetype", data: Data(presentationMimeType.utf8), compressionMethod: .store),
      ZipContainerEntry(
        path: "content.xml", data: Data(contentXML.utf8), compressionMethod: .deflate),
    ])
  }

  public static func decodePresentation(from data: Data) throws -> PresentationDeckModel {
    let entries = try ZipContainerCodec.unpack(from: data)
    guard let mimeEntry = entries.first(where: { $0.path == "mimetype" }),
      String(data: mimeEntry.data, encoding: .utf8) == presentationMimeType
    else {
      throw EasyOfficeError.corruptedContainer(detail: "ODP 컨테이너의 mimetype 엔트리가 유효하지 않습니다.")
    }
    guard let contentEntry = entries.first(where: { $0.path == "content.xml" }) else {
      throw EasyOfficeError.corruptedContainer(detail: "ODP 컨테이너에 'content.xml'이 누락되었습니다.")
    }
    let xml = try XMLSanitizer.validateSafeXMLData(contentEntry.data)
    let root = XMLSanitizer.extractElements(named: "office:document-content", in: xml).first
    let slides = XMLSanitizer.extractElements(named: "draw:page", in: xml).map {
      page -> SlideModel in
      let slideID = page.attributes["id"].flatMap(UUID.init(uuidString:)) ?? UUID()
      let bullets = XMLSanitizer.extractElements(named: "text:list-item", in: page.innerXML).map {
        XMLSanitizer.unescape($0.innerXML)
      }
      let elements = XMLSanitizer.extractElements(named: "draw:frame", in: page.innerXML).map {
        frame in
        XMLDocumentModelMapper.parseSlideElement(
          attributes: frame.attributes, innerXML: frame.innerXML)
      }
      return SlideModel(
        id: slideID, title: page.attributes["title"] ?? "",
        subtitle: page.attributes["subtitle"] ?? "",
        bulletPoints: bullets, speakerNotes: page.attributes["notes"] ?? "",
        masterLayout: SlideMasterLayout(rawValue: page.attributes["layout"] ?? "")
          ?? .titleAndContent,
        transition: SlideTransitionKind(rawValue: page.attributes["transition"] ?? "") ?? .fade,
        elements: elements
      )
    }
    let deckTheme =
      SlideDeckTheme(
        rawValue: root?.attributes["eo:deckTheme"] ?? root?.attributes["deckTheme"] ?? ""
      ) ?? .signatureLavender
    return PresentationDeckModel(
      title: root?.attributes["title"] ?? "발표자료",
      aspectRatio: SlideAspectRatio(rawValue: root?.attributes["aspectRatio"] ?? "")
        ?? .widescreen16x9,
      slides: slides,
      deckTheme: deckTheme
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
      return
        "    <opf:item id=\"bindata\(idx)\" href=\"BinData/\(XMLSanitizer.escape(safeName))\" media-type=\"\(XMLSanitizer.escape(img.mimeType))\"/>"
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

    let extendedMetaXML = XMLDocumentModelMapper.serializeExtendedMetadata(
      doc, mediaPrefix: "BinData/")
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
        let cells = row.map {
          "<hp:tc><hp:cellText>\(XMLSanitizer.escape($0))</hp:cellText></hp:tc>"
        }.joined()
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
      ZipContainerEntry(
        path: "Contents/content.hpf", data: Data(contentHPF.utf8), compressionMethod: .deflate),
      ZipContainerEntry(
        path: "Contents/header.xml", data: Data(headerXML.utf8), compressionMethod: .deflate),
      ZipContainerEntry(
        path: "Contents/section0.xml", data: Data(sectionXML.utf8), compressionMethod: .deflate),
    ]
    for img in doc.images {
      let safeName = XMLDocumentModelMapper.sanitizeMediaFileName(img.fileName)
      entries.append(
        ZipContainerEntry(
          path: "BinData/\(safeName)", data: img.imageData, compressionMethod: .store))
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
      throw EasyOfficeError.corruptedContainer(
        detail: "HWPX(KS X 6101) 컨테이너의 mimetype이 'application/hwp+zip'이 아닙니다.")
    }
    guard let hpfXML = entryMap["Contents/content.hpf"],
      let headerXML = entryMap["Contents/header.xml"],
      let sectionXML = entryMap["Contents/section0.xml"]
    else {
      throw EasyOfficeError.corruptedContainer(
        detail: "HWPX 필수 스트림(content.hpf, header.xml, section0.xml)이 누락되었습니다.")
    }

    let title =
      XMLSanitizer.extractElements(named: "opf:title", in: hpfXML).first.map {
        XMLSanitizer.unescape($0.innerXML)
      } ?? "제목 없음"
    let headElem = XMLSanitizer.extractElements(named: "hh:head", in: headerXML).first
    let headAttrs = headElem?.attributes ?? [:]
    let paperSize = PaperSizePreset(rawValue: headAttrs["paperSize"] ?? "") ?? .a4
    let headerText = headAttrs["headerText"] ?? ""
    let footerText = headAttrs["footerText"] ?? ""
    let trackChanges = headAttrs["trackChanges"] == "1"

    let tables = XMLSanitizer.extractElements(named: "hp:tbl", in: sectionXML).map { tblElem in
      XMLDocumentModelMapper.parseTable(
        attributes: tblElem.attributes, innerXML: tblElem.innerXML,
        rowTag: "hp:tr", cellTag: "hp:tc", cellTextTag: "hp:cellText"
      )
    }

    let sectionWithoutTables = XMLSanitizer.stripElements(named: "hp:tbl", from: sectionXML)
    let paragraphs = XMLSanitizer.extractElements(named: "hp:p", in: sectionWithoutTables).map {
      pElem -> ParagraphBlock in
      var parsed = XMLDocumentModelMapper.parseParagraph(
        attributes: pElem.attributes, innerXML: pElem.innerXML, childTextTag: "hp:t")
      if let lsStr = pElem.attributes["eo:letterSpacing"] ?? pElem.attributes["spacing"],
        let lsVal = Double(lsStr.replacingOccurrences(of: "%", with: ""))
      {
        parsed.letterSpacingPercent = max(-50.0, min(50.0, lsVal))
      }
      if let csStr = pElem.attributes["eo:charScale"] ?? pElem.attributes["ratio"],
        let csVal = Double(csStr.replacingOccurrences(of: "%", with: ""))
      {
        parsed.charWidthScalePercent = max(50.0, min(200.0, csVal))
      }
      return parsed
    }

    let meta = XMLDocumentModelMapper.parseExtendedMetadata(
      from: headerXML, defaultPaperSize: paperSize, binaryEntries: binaryEntries)

    return OfficeDocumentModel(
      title: title, format: .hwpx, paperSize: paperSize,
      headerText: headerText, footerText: footerText,
      paragraphs: paragraphs, tables: tables, trackChangesEnabled: trackChanges,
      sections: meta.sections, footnotes: meta.footnotes, revisions: meta.revisions,
      comments: meta.comments, bookmarks: meta.bookmarks, images: meta.images,
      equations: meta.equations, watermarkText: meta.watermarkText
    )
  }
}
