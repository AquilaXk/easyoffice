import Foundation

// MARK: - ODF Container Codec (ISO/IEC 26300 ODT, ODS, ODP)

/// ISO/IEC 26300 OpenDocument Format container encoder and decoder (`.odt`, `.ods`, `.odp`).
public enum ODFContainerCodec {
  public static let textMimeType = "application/vnd.oasis.opendocument.text"
  public static let spreadsheetMimeType = "application/vnd.oasis.opendocument.spreadsheet"
  public static let presentationMimeType = "application/vnd.oasis.opendocument.presentation"
  public static let maxSingleRepeatedCellExpansion = 100_000
  public static let maxPopulatedODSCells = 1_000_000

  private static func buildODFMetaXML(title: String) -> String {
    """
    <?xml version="1.0" encoding="UTF-8"?>
    <office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/">
      <office:meta>
        <dc:title>\(XMLSanitizer.escape(title))</dc:title>
      </office:meta>
    </office:document-meta>
    """
  }

  public static func encodeDocument(_ doc: OfficeDocumentModel) throws -> Data {
    let resolvedMedia = XMLDocumentModelMapper.resolveUniqueMediaEntries(
      images: doc.images,
      mediaFolderPrefix: "Pictures/"
    )

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
          "<table:table-cell><text:p>\(XMLSanitizer.escape($0))</text:p></table:table-cell>"
        }.joined()
        return "<table:table-row>\(cells)</table:table-row>"
      }.joined(separator: "\n        ")
      return
        "<table:table \(tAttrs)>\n        \(spansXML)\n        \(rowsXML)\n      </table:table>"
    }.joined(separator: "\n      ")

    let mediaFramesXML = resolvedMedia.map { media in
      "<draw:frame eo:synthesizedMedia=\"1\" draw:name=\"\(XMLSanitizer.escape(media.deduplicatedFileName))\"><draw:image xlink:href=\"\(XMLSanitizer.escape(media.archivePath))\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame>"
    }.joined(separator: "\n      ")

    let extendedMetaXML = XMLDocumentModelMapper.serializeExtendedMetadata(
      doc, resolvedMedia: resolvedMedia)
    let contentXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:eo="\(XMLDocumentModelMapper.eoNamespaceURI)" eo:title="\(XMLSanitizer.escape(doc.title))" eo:paperSize="\(XMLSanitizer.escape(doc.paperSize.rawValue))" eo:headerText="\(XMLSanitizer.escape(doc.headerText))" eo:footerText="\(XMLSanitizer.escape(doc.footerText))" eo:trackChanges="\(doc.trackChangesEnabled ? "1" : "0")">
        <office:meta>
          <dc:title>\(XMLSanitizer.escape(doc.title))</dc:title>
          \(extendedMetaXML)
        </office:meta>
        <office:body>
          <office:text>
            \(pElements)
            \(tblElements)
            \(mediaFramesXML)
          </office:text>
        </office:body>
      </office:document-content>
      """

    let metaXML = buildODFMetaXML(title: doc.title)
    let baseEntries: [ZipContainerEntry] = [
      ZipContainerEntry(path: "mimetype", data: Data(textMimeType.utf8), compressionMethod: .store),
      ZipContainerEntry(
        path: "meta.xml", data: Data(metaXML.utf8), compressionMethod: .deflate),
      ZipContainerEntry(
        path: "content.xml", data: Data(contentXML.utf8), compressionMethod: .deflate),
    ]
    return try XMLDocumentModelMapper.packArchiveEntries(
      baseEntries,
      resolvedMedia: resolvedMedia,
      preserved: doc.preservedArchiveEntries
    )
  }

  public static func decodeDocument(from data: Data) throws -> OfficeDocumentModel {
    let unpacked = try XMLDocumentModelMapper.unpackAndValidateArchiveEntries(
      from: data,
      excludedPreservedPaths: ["mimetype", "content.xml", "meta.xml"]
    )
    guard unpacked.xmlEntries["mimetype"] == textMimeType else {
      throw EasyOfficeError.corruptedContainer(detail: "ODT 컨테이너의 mimetype 엔트리가 유효하지 않습니다.")
    }
    guard let xml = unpacked.xmlEntries["content.xml"] else {
      throw EasyOfficeError.corruptedContainer(detail: "ODT 컨테이너에 'content.xml'이 누락되었습니다.")
    }
    let metaFileXML = unpacked.xmlEntries["meta.xml"] ?? ""
    let binaryEntries = unpacked.binaryEntries
    let preservedEntries = unpacked.preservedEntries
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

    let bodyWithoutTablesAndFrames = XMLSanitizer.stripElements(
      named: "draw:frame",
      from: XMLSanitizer.stripElements(named: "table:table", from: root.innerXML)
    )
    .replacingOccurrences(of: "<text:h ", with: "<text:p eo:isHeading=\"1\" ")
    .replacingOccurrences(of: "<text:h>", with: "<text:p eo:isHeading=\"1\">")
    .replacingOccurrences(of: "</text:h>", with: "</text:p>")

    let paragraphs = XMLSanitizer.extractElements(named: "text:p", in: bodyWithoutTablesAndFrames)
      .map { elem -> ParagraphBlock in
        var parsed = XMLDocumentModelMapper.parseParagraph(
          attributes: elem.attributes, innerXML: elem.innerXML, childTextTag: nil)
        if elem.attributes["eo:isHeading"] == "1"
          && XMLDocumentModelMapper.attr("style", in: elem.attributes) == nil
        {
          let level = Int(elem.attributes["text:outline-level"] ?? "1") ?? 1
          switch level {
          case 1:
            parsed.style = .heading1
            parsed.fontSizePt = 20.0
            parsed.isBold = true
          case 2:
            parsed.style = .heading2
            parsed.fontSizePt = 16.0
            parsed.isBold = true
          default:
            parsed.style = .heading3
            parsed.fontSizePt = 13.5
            parsed.isBold = true
          }
        }
        if let lsStr = XMLDocumentModelMapper.attr("letterSpacing", in: elem.attributes)
          ?? elem.attributes["fo:letter-spacing"],
          let lsVal = Double(lsStr.replacingOccurrences(of: "%", with: ""))
        {
          parsed.letterSpacingPercent = max(-50.0, min(50.0, lsVal))
        }
        if let csStr = XMLDocumentModelMapper.attr("charScale", in: elem.attributes)
          ?? elem.attributes["style:text-scale"],
          let csVal = Double(csStr.replacingOccurrences(of: "%", with: ""))
        {
          parsed.charWidthScalePercent = max(50.0, min(200.0, csVal))
        }
        return parsed
      }

    let rootAttrs = root.attributes
    let metaTitle =
      (XMLSanitizer.extractElements(named: "dc:title", in: metaFileXML).first
      ?? XMLSanitizer.extractElements(named: "dc:title", in: root.innerXML).first).map {
        XMLSanitizer.unescape($0.innerXML)
      }
    let resolvedTitle =
      XMLDocumentModelMapper.attr("title", in: rootAttrs) ?? metaTitle ?? "제목 없음"
    let paperSize =
      PaperSizePreset(rawValue: XMLDocumentModelMapper.attr("paperSize", in: rootAttrs) ?? "")
      ?? .a4
    let meta = XMLDocumentModelMapper.parseExtendedMetadata(
      from: root.innerXML, defaultPaperSize: paperSize, binaryEntries: binaryEntries)
    var resolvedImages = meta.images
    if resolvedImages.isEmpty {
      for imgElem in XMLSanitizer.extractElements(named: "draw:image", in: root.innerXML) {
        if let href = imgElem.attributes["xlink:href"] ?? imgElem.attributes["href"],
          let imgData = binaryEntries[href]
        {
          let fileName = (href as NSString).lastPathComponent
          let ext = (fileName as NSString).pathExtension
          resolvedImages.append(
            EmbeddedImageAttachment(
              fileName: fileName,
              mimeType: XMLDocumentModelMapper.mimeTypeForMediaExtension(ext),
              imageData: imgData
            )
          )
        }
      }
    }

    var decodedDoc = OfficeDocumentModel(
      title: resolvedTitle,
      format: .odt,
      paperSize: paperSize,
      headerText: XMLDocumentModelMapper.attr("headerText", in: rootAttrs) ?? "",
      footerText: XMLDocumentModelMapper.attr("footerText", in: rootAttrs) ?? "",
      paragraphs: paragraphs,
      tables: tables,
      trackChangesEnabled: XMLDocumentModelMapper.attr("trackChanges", in: rootAttrs) == "1",
      sections: meta.sections,
      footnotes: meta.footnotes,
      revisions: meta.revisions,
      comments: meta.comments,
      bookmarks: meta.bookmarks,
      images: resolvedImages,
      equations: meta.equations,
      watermarkText: meta.watermarkText
    )
    decodedDoc.preservedArchiveEntries = preservedEntries
    return decodedDoc
  }

  public static func encodeWorksheet(_ sheet: WorksheetModel) throws -> Data {
    let orderedPages = sheet.orderedSheetPages()
    let tablesXML = orderedPages.enumerated().map { pageIdx, page -> String in
      var evalSheet = sheet
      evalSheet.switchActiveSheet(to: pageIdx)

      let allCoords = Set(page.cellsByCoordinate.keys).union(page.cellStyles.keys)
      guard !allCoords.isEmpty else {
        return "<table:table table:name=\"\(XMLSanitizer.escape(page.name))\"/>"
      }
      let coordsByRow = Dictionary(grouping: allCoords, by: \.row)
      let sortedRows = coordsByRow.keys.sorted()
      var rowStrings: [String] = []
      var previousRow = -1

      for r in sortedRows {
        let rowGap = r - previousRow - 1
        if rowGap > 1 {
          rowStrings.append(
            "        <table:table-row table:number-rows-repeated=\"\(rowGap)\"><table:table-cell/></table:table-row>"
          )
        } else if rowGap == 1 {
          rowStrings.append("        <table:table-row><table:table-cell/></table:table-row>")
        }

        let sortedRowCoords = (coordsByRow[r] ?? []).sorted { $0.column < $1.column }
        var cellStrings: [String] = []
        var previousCol = -1
        for coord in sortedRowCoords {
          let c = coord.column
          let colGap = c - previousCol - 1
          if colGap > 1 {
            cellStrings.append(
              "<table:table-cell table:number-columns-repeated=\"\(colGap)\"/>"
            )
          } else if colGap == 1 {
            cellStrings.append("<table:table-cell/>")
          }

          let raw = page.cellsByCoordinate[coord] ?? ""
          let styleAttrs = XMLDocumentModelMapper.cellStyleAttributes(page.cellStyles[coord])
          if raw.isEmpty && styleAttrs.isEmpty {
            cellStrings.append("<table:table-cell/>")
          } else if raw.hasPrefix("=") {
            let escapedRaw = XMLSanitizer.escape(raw)
            let openFormula = XMLSanitizer.escape(
              XMLDocumentModelMapper.toOpenFormulaExpression(raw))
            let evaluated = evalSheet.evaluate(at: coord)
            let displayStr = XMLSanitizer.escape(evaluated.description)
            if let numVal = evaluated.numericValue, numVal.isFinite {
              cellStrings.append(
                "<table:table-cell eo:address=\"\(coord.description)\" eo:raw=\"\(escapedRaw)\" table:formula=\"\(openFormula)\" office:value-type=\"float\" office:value=\"\(numVal)\"\(styleAttrs)><text:p>\(displayStr)</text:p></table:table-cell>"
              )
            } else {
              cellStrings.append(
                "<table:table-cell eo:address=\"\(coord.description)\" eo:raw=\"\(escapedRaw)\" table:formula=\"\(openFormula)\" office:value-type=\"string\"\(styleAttrs)><text:p>\(displayStr)</text:p></table:table-cell>"
              )
            }
          } else if !raw.isEmpty, Double(raw)?.isFinite == true {
            let escapedRaw = XMLSanitizer.escape(raw)
            cellStrings.append(
              "<table:table-cell eo:address=\"\(coord.description)\" eo:raw=\"\(escapedRaw)\" office:value-type=\"float\" office:value=\"\(escapedRaw)\"\(styleAttrs)><text:p>\(escapedRaw)</text:p></table:table-cell>"
            )
          } else {
            let escapedRaw = XMLSanitizer.escape(raw)
            cellStrings.append(
              "<table:table-cell eo:address=\"\(coord.description)\" eo:raw=\"\(escapedRaw)\" office:value-type=\"string\"\(styleAttrs)><text:p>\(escapedRaw)</text:p></table:table-cell>"
            )
          }
          previousCol = c
        }
        rowStrings.append("        <table:table-row>\(cellStrings.joined())</table:table-row>")
        previousRow = r
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
      <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:eo="\(XMLDocumentModelMapper.eoNamespaceURI)">
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

    let baseEntries: [ZipContainerEntry] = [
      ZipContainerEntry(
        path: "mimetype", data: Data(spreadsheetMimeType.utf8), compressionMethod: .store),
      ZipContainerEntry(
        path: "content.xml", data: Data(contentXML.utf8), compressionMethod: .deflate),
    ]
    return try XMLDocumentModelMapper.packArchiveEntries(
      baseEntries,
      preserved: sheet.preservedArchiveEntries
    )
  }

  public static func decodeWorksheet(from data: Data) throws -> WorksheetModel {
    let unpacked = try XMLDocumentModelMapper.unpackAndValidateArchiveEntries(
      from: data,
      excludedPreservedPaths: ["mimetype", "content.xml", "meta.xml"]
    )
    guard unpacked.xmlEntries["mimetype"] == spreadsheetMimeType else {
      throw EasyOfficeError.corruptedContainer(detail: "ODS 컨테이너의 mimetype 엔트리가 유효하지 않습니다.")
    }
    guard let xml = unpacked.xmlEntries["content.xml"] else {
      throw EasyOfficeError.corruptedContainer(detail: "ODS 컨테이너에 'content.xml'이 누락되었습니다.")
    }
    let preservedEntries = unpacked.preservedEntries
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
            1,
            Int(rowElem.attributes["table:number-rows-repeated"] ?? "1") ?? 1
          )
          var colIdx = 0
          let orderedCells = XMLDocumentModelMapper.extractODSTableRowCells(in: rowElem.innerXML)
          for cellItem in orderedCells {
            let attrs = cellItem.attributes
            let colRepeat = max(
              1,
              Int(attrs["table:number-columns-repeated"] ?? "1") ?? 1
            )
            if cellItem.isCovered {
              colIdx += colRepeat
              continue
            }

            let resolvedRaw: String
            if let rawAttr = XMLDocumentModelMapper.attr("raw", in: attrs) {
              resolvedRaw = rawAttr
            } else if let formulaAttr = attrs["table:formula"], !formulaAttr.isEmpty {
              resolvedRaw = XMLDocumentModelMapper.normalizeOpenFormulaExpression(formulaAttr)
            } else if let valAttr = attrs["office:value"], !valAttr.isEmpty {
              resolvedRaw = valAttr
            } else if let boolAttr = attrs["office:boolean-value"], !boolAttr.isEmpty {
              resolvedRaw = boolAttr.uppercased()
            } else {
              let textPs = XMLSanitizer.extractElements(named: "text:p", in: cellItem.innerXML)
              resolvedRaw = textPs.map {
                XMLDocumentModelMapper.extractODFText(from: $0.innerXML)
              }.joined(separator: "\n")
            }

            let parsedStyle = XMLDocumentModelMapper.parseCellStyle(from: attrs)
            if resolvedRaw.isEmpty && parsedStyle == nil {
              colIdx += colRepeat
              continue
            }

            let (expansionCount, expOverflow) = rowRepeat.multipliedReportingOverflow(by: colRepeat)
            guard !expOverflow, expansionCount <= maxSingleRepeatedCellExpansion,
              populatedCellCount + expansionCount <= maxPopulatedODSCells
            else {
              throw EasyOfficeError.securityViolation(
                detail:
                  "ODF(.ods) 반복 셀 확장 개수가 허용 상한(\(maxSingleRepeatedCellExpansion)셀)을 초과했습니다."
              )
            }
            guard rowIdx + rowRepeat - 1 <= CellAddress.maxZeroBasedRow,
              colIdx + colRepeat - 1 <= CellAddress.maxZeroBasedColumn
            else {
              throw EasyOfficeError.resourceBudgetExceeded(
                detail: "ODF(.ods) 셀 좌표가 최대 워크시트 범위(1,048,576행×16,384열)를 초과했습니다."
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
          if let addr = XMLDocumentModelMapper.attr("address", in: cellElem.attributes),
            let raw = XMLDocumentModelMapper.attr("raw", in: cellElem.attributes)
          {
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
    primary.preservedArchiveEntries = preservedEntries
    return primary
  }

  public static func encodePresentation(_ deck: PresentationDeckModel) throws -> Data {
    let pagesXML = deck.slides.map { slide -> String in
      let bullets = slide.bulletPoints.map {
        "<text:list-item><text:p>\(XMLSanitizer.escape($0))</text:p></text:list-item>"
      }.joined()
      let shapes = slide.elements.map { el in
        "<draw:frame \(XMLDocumentModelMapper.slideElementAttributes(el))><draw:text-box><text:p>\(XMLSanitizer.escape(el.bodyText))</text:p></draw:text-box></draw:frame>"
      }.joined()
      let notesXML =
        slide.speakerNotes.isEmpty
        ? ""
        : "<presentation:notes><draw:frame><draw:text-box><text:p>\(XMLSanitizer.escape(slide.speakerNotes))</text:p></draw:text-box></draw:frame></presentation:notes>"
      let durationAttr = String(
        format: "eo:transitionDuration=\"%.2f\"",
        slide.transitionDurationSeconds
      )
      let autoAdvanceAttr = slide.autoAdvanceSeconds.map { " eo:autoAdvance=\"\($0)\"" } ?? ""
      return
        "<draw:page eo:id=\"\(slide.id.uuidString)\" draw:name=\"\(XMLSanitizer.escape(slide.title))\" eo:title=\"\(XMLSanitizer.escape(slide.title))\" eo:subtitle=\"\(XMLSanitizer.escape(slide.subtitle))\" eo:notes=\"\(XMLSanitizer.escape(slide.speakerNotes))\" eo:layout=\"\(XMLSanitizer.escape(slide.masterLayout.rawValue))\" eo:transition=\"\(XMLSanitizer.escape(slide.transition.rawValue))\" \(durationAttr)\(autoAdvanceAttr)>\(bullets)\(shapes)\(notesXML)</draw:page>"
    }.joined(separator: "\n      ")

    let contentXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:eo="\(XMLDocumentModelMapper.eoNamespaceURI)" eo:title="\(XMLSanitizer.escape(deck.title))" eo:aspectRatio="\(XMLSanitizer.escape(deck.aspectRatio.rawValue))" eo:deckTheme="\(XMLSanitizer.escape(deck.deckTheme.rawValue))">
        <office:body>
          <office:presentation>
            \(pagesXML)
          </office:presentation>
        </office:body>
      </office:document-content>
      """

    let metaXML = buildODFMetaXML(title: deck.title)
    let baseEntries: [ZipContainerEntry] = [
      ZipContainerEntry(
        path: "mimetype", data: Data(presentationMimeType.utf8), compressionMethod: .store),
      ZipContainerEntry(
        path: "meta.xml", data: Data(metaXML.utf8), compressionMethod: .deflate),
      ZipContainerEntry(
        path: "content.xml", data: Data(contentXML.utf8), compressionMethod: .deflate),
    ]
    return try XMLDocumentModelMapper.packArchiveEntries(
      baseEntries,
      preserved: deck.preservedArchiveEntries
    )
  }

  public static func decodePresentation(from data: Data) throws -> PresentationDeckModel {
    let unpacked = try XMLDocumentModelMapper.unpackAndValidateArchiveEntries(
      from: data,
      excludedPreservedPaths: ["mimetype", "content.xml", "meta.xml"]
    )
    guard unpacked.xmlEntries["mimetype"] == presentationMimeType else {
      throw EasyOfficeError.corruptedContainer(detail: "ODP 컨테이너의 mimetype 엔트리가 유효하지 않습니다.")
    }
    guard let xml = unpacked.xmlEntries["content.xml"] else {
      throw EasyOfficeError.corruptedContainer(detail: "ODP 컨테이너에 'content.xml'이 누락되었습니다.")
    }
    let metaFileXML = unpacked.xmlEntries["meta.xml"] ?? ""
    let preservedEntries = unpacked.preservedEntries
    let root = XMLSanitizer.extractElements(named: "office:document-content", in: xml).first
    let rootAttrs = root?.attributes ?? [:]

    let slides = XMLSanitizer.extractElements(named: "draw:page", in: xml).map {
      page -> SlideModel in
      let pAttrs = page.attributes
      let slideID =
        XMLDocumentModelMapper.attr("id", in: pAttrs).flatMap(UUID.init(uuidString:)) ?? UUID()

      var extractedNotes = XMLDocumentModelMapper.attr("notes", in: pAttrs) ?? ""
      if extractedNotes.isEmpty,
        let notesElem = XMLSanitizer.extractElements(
          named: "presentation:notes", in: page.innerXML
        ).first
      {
        let withoutPageNum = XMLSanitizer.stripElements(
          named: "text:page-number", from: notesElem.innerXML)
        let notePs = XMLSanitizer.extractElements(named: "text:p", in: withoutPageNum)
        extractedNotes = notePs.map {
          XMLDocumentModelMapper.extractODFText(from: $0.innerXML)
        }.filter { !$0.isEmpty }.joined(separator: "\n")
      }

      // Exclude <presentation:notes>, <office:forms>, and <text:page-number> from slide body
      let slideBodyXML = XMLSanitizer.stripElements(
        named: "text:page-number",
        from: XMLSanitizer.stripElements(
          named: "office:forms",
          from: XMLSanitizer.stripElements(named: "presentation:notes", from: page.innerXML)
        )
      )

      let bullets = XMLSanitizer.extractElements(named: "text:list-item", in: slideBodyXML).map {
        XMLDocumentModelMapper.extractODFText(from: $0.innerXML)
      }.filter { !$0.isEmpty }

      let elements = XMLSanitizer.extractElements(named: "draw:frame", in: slideBodyXML).compactMap
      {
        frame -> SlideCanvasElement? in
        let presClass = frame.attributes["presentation:class"]?.lowercased()
        if presClass == "page-number" || presClass == "footer" || presClass == "header"
          || presClass == "date-time"
        {
          return nil
        }
        return XMLDocumentModelMapper.parseSlideElement(
          attributes: frame.attributes, innerXML: frame.innerXML)
      }
      let transitionDuration =
        XMLDocumentModelMapper.attr("transitionDuration", in: pAttrs)
        .flatMap(Double.init) ?? 0.8
      let autoAdvance =
        XMLDocumentModelMapper.attr("autoAdvance", in: pAttrs)
        .flatMap(Int.init)
      let slideTitle =
        XMLDocumentModelMapper.attr("title", in: pAttrs)
        ?? pAttrs["draw:name"]
        ?? ""
      return SlideModel(
        id: slideID,
        title: slideTitle,
        subtitle: XMLDocumentModelMapper.attr("subtitle", in: pAttrs) ?? "",
        bulletPoints: bullets,
        speakerNotes: extractedNotes,
        masterLayout: SlideMasterLayout(
          rawValue: XMLDocumentModelMapper.attr("layout", in: pAttrs) ?? "")
          ?? .titleAndContent,
        transition: SlideTransitionKind(
          rawValue: XMLDocumentModelMapper.attr("transition", in: pAttrs) ?? "") ?? .fade,
        elements: elements,
        transitionDurationSeconds: transitionDuration,
        autoAdvanceSeconds: autoAdvance
      )
    }

    let metaTitle =
      (XMLSanitizer.extractElements(named: "dc:title", in: metaFileXML).first
      ?? XMLSanitizer.extractElements(named: "dc:title", in: xml).first).map {
        XMLSanitizer.unescape($0.innerXML)
      }
    let deckTitle = XMLDocumentModelMapper.attr("title", in: rootAttrs) ?? metaTitle ?? "발표자료"
    let deckTheme =
      SlideDeckTheme(rawValue: XMLDocumentModelMapper.attr("deckTheme", in: rootAttrs) ?? "")
      ?? .signatureLavender
    var deck = PresentationDeckModel(
      title: deckTitle,
      aspectRatio: SlideAspectRatio(
        rawValue: XMLDocumentModelMapper.attr("aspectRatio", in: rootAttrs) ?? "")
        ?? .widescreen16x9,
      slides: slides,
      deckTheme: deckTheme
    )
    deck.preservedArchiveEntries = preservedEntries
    return deck
  }
}

// MARK: - HWPX Container Codec (KS X 6101 OWPML)

/// KS X 6101 OWPML (`HWPX`) ZIP-XML container encoder and decoder.
public enum HWPXContainerCodec {
  public static let hwpxMimeType = "application/hwp+zip"

  public static func encodeDocument(_ doc: OfficeDocumentModel) throws -> Data {
    let resolvedMedia = XMLDocumentModelMapper.resolveUniqueMediaEntries(
      images: doc.images,
      mediaFolderPrefix: "BinData/"
    )

    let binManifestItems = resolvedMedia.enumerated().map { idx, media in
      "    <opf:item id=\"bindata\(idx)\" href=\"\(XMLSanitizer.escape(media.archivePath))\" media-type=\"\(XMLSanitizer.escape(media.attachment.mimeType))\" isEmbeded=\"1\"/>"
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
      doc, resolvedMedia: resolvedMedia)
    let headerXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <hh:head xmlns:hh="http://www.owpml.org/owpml/2011/head" xmlns:eo="\(XMLDocumentModelMapper.eoNamespaceURI)" eo:paperSize="\(XMLSanitizer.escape(doc.paperSize.rawValue))" eo:headerText="\(XMLSanitizer.escape(doc.headerText))" eo:footerText="\(XMLSanitizer.escape(doc.footerText))" eo:trackChanges="\(doc.trackChangesEnabled ? "1" : "0")">
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
          "<hp:tc><hp:p><hp:run><hp:t>\(XMLSanitizer.escape($0))</hp:t></hp:run></hp:p><hp:cellText>\(XMLSanitizer.escape($0))</hp:cellText></hp:tc>"
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

    let picsXML = resolvedMedia.enumerated().map { idx, media in
      "  <hp:pic id=\"\(idx + 1)\" binaryItemIDRef=\"bindata\(idx)\" fileName=\"\(XMLSanitizer.escape(media.deduplicatedFileName))\"/>"
    }.joined(separator: "\n")

    let sectionXML = """
      <?xml version="1.0" encoding="UTF-8"?>
      <hs:sec xmlns:hs="http://www.owpml.org/owpml/2011/section" xmlns:hp="http://www.owpml.org/owpml/2011/paragraph" xmlns:eo="\(XMLDocumentModelMapper.eoNamespaceURI)">
      \(paragraphsXML)
      \(tablesXML)
      \(picsXML)
      </hs:sec>
      """

    let baseEntries: [ZipContainerEntry] = [
      ZipContainerEntry(path: "mimetype", data: Data(hwpxMimeType.utf8), compressionMethod: .store),
      ZipContainerEntry(
        path: "Contents/content.hpf", data: Data(contentHPF.utf8), compressionMethod: .deflate),
      ZipContainerEntry(
        path: "Contents/header.xml", data: Data(headerXML.utf8), compressionMethod: .deflate),
      ZipContainerEntry(
        path: "Contents/section0.xml", data: Data(sectionXML.utf8), compressionMethod: .deflate),
    ]
    return try XMLDocumentModelMapper.packArchiveEntries(
      baseEntries,
      resolvedMedia: resolvedMedia,
      preserved: doc.preservedArchiveEntries
    )
  }

  public static func decodeDocument(from data: Data) throws -> OfficeDocumentModel {
    let unpacked = try XMLDocumentModelMapper.unpackAndValidateArchiveEntries(
      from: data,
      excludedPreservedPaths: [
        "mimetype", "Contents/content.hpf", "Contents/header.xml", "Contents/section0.xml",
      ]
    )
    let entryMap = unpacked.xmlEntries
    let binaryEntries = unpacked.binaryEntries
    let preservedEntries = unpacked.preservedEntries

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
    let paperSize =
      PaperSizePreset(rawValue: XMLDocumentModelMapper.attr("paperSize", in: headAttrs) ?? "")
      ?? .a4
    let headerText = XMLDocumentModelMapper.attr("headerText", in: headAttrs) ?? ""
    let footerText = XMLDocumentModelMapper.attr("footerText", in: headAttrs) ?? ""
    let trackChanges = XMLDocumentModelMapper.attr("trackChanges", in: headAttrs) == "1"

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
      if let lsStr = XMLDocumentModelMapper.attr("letterSpacing", in: pElem.attributes)
        ?? pElem.attributes["spacing"],
        let lsVal = Double(lsStr.replacingOccurrences(of: "%", with: ""))
      {
        parsed.letterSpacingPercent = max(-50.0, min(50.0, lsVal))
      }
      if let csStr = XMLDocumentModelMapper.attr("charScale", in: pElem.attributes)
        ?? pElem.attributes["ratio"],
        let csVal = Double(csStr.replacingOccurrences(of: "%", with: ""))
      {
        parsed.charWidthScalePercent = max(50.0, min(200.0, csVal))
      }
      return parsed
    }

    let meta = XMLDocumentModelMapper.parseExtendedMetadata(
      from: headerXML, defaultPaperSize: paperSize, binaryEntries: binaryEntries)
    var resolvedImages = meta.images
    if resolvedImages.isEmpty {
      var manifestHrefById: [String: String] = [:]
      for item in XMLSanitizer.extractElements(named: "opf:item", in: hpfXML) {
        if let id = item.attributes["id"], let href = item.attributes["href"] {
          manifestHrefById[id] = href
        }
      }
      for pic in XMLSanitizer.extractElements(named: "hp:pic", in: sectionXML) {
        if let refId = pic.attributes["binaryItemIDRef"] ?? pic.attributes["binItem"],
          let href = manifestHrefById[refId],
          let imgData = binaryEntries[href]
        {
          let fileName = (href as NSString).lastPathComponent
          let ext = (fileName as NSString).pathExtension
          resolvedImages.append(
            EmbeddedImageAttachment(
              fileName: fileName,
              mimeType: XMLDocumentModelMapper.mimeTypeForMediaExtension(ext),
              imageData: imgData
            )
          )
        }
      }
    }

    var decodedDoc = OfficeDocumentModel(
      title: title, format: .hwpx, paperSize: paperSize,
      headerText: headerText, footerText: footerText,
      paragraphs: paragraphs, tables: tables, trackChangesEnabled: trackChanges,
      sections: meta.sections, footnotes: meta.footnotes, revisions: meta.revisions,
      comments: meta.comments, bookmarks: meta.bookmarks, images: resolvedImages,
      equations: HWPEquationCodec.extractHWPXEquations(from: sectionXML, existing: meta.equations),
      watermarkText: meta.watermarkText
    )
    decodedDoc.preservedArchiveEntries = preservedEntries
    return decodedDoc
  }
}

extension XMLDocumentModelMapper {
  static func formatOpenFormulaSheetPrefix(_ rawSheet: String) -> String {
    let isSimple =
      !rawSheet.isEmpty
      && (rawSheet.first?.isLetter == true || rawSheet.first == "_")
      && rawSheet.allSatisfy { $0.isLetter || $0.isNumber || $0 == "_" }
      && rawSheet.uppercased() != "TRUE"
      && rawSheet.uppercased() != "FALSE"
      && (try? CellAddress(a1Notation: rawSheet)) == nil
    if isSimple {
      return "\(rawSheet)."
    }
    return "'\(rawSheet.replacingOccurrences(of: "'", with: "''"))'."
  }

  static func formatA1SheetPrefix(_ rawSheet: String) -> String {
    let isSimple =
      !rawSheet.isEmpty
      && (rawSheet.first?.isLetter == true || rawSheet.first == "_")
      && rawSheet.allSatisfy { $0.isLetter || $0.isNumber || $0 == "_" }
      && rawSheet.uppercased() != "TRUE"
      && rawSheet.uppercased() != "FALSE"
      && (try? CellAddress(a1Notation: rawSheet)) == nil
    if isSimple {
      return rawSheet
    }
    return "'\(rawSheet.replacingOccurrences(of: "'", with: "''"))'"
  }

  static func splitOnceUnquoted(_ text: String, delimiter: Character) -> (String, String)? {
    var inSq = false
    var idx = text.startIndex
    while idx < text.endIndex {
      let c = text[idx]
      if c == "'" {
        inSq.toggle()
      } else if c == delimiter && !inSq {
        return (String(text[..<idx]), String(text[text.index(after: idx)...]))
      }
      idx = text.index(after: idx)
    }
    return nil
  }

  public static func normalizeOpenFormulaExpression(_ rawFormula: String) -> String {
    var expr = rawFormula.trimmingCharacters(in: .whitespacesAndNewlines)
    if expr.lowercased().hasPrefix("of:") {
      expr = String(expr.dropFirst(3)).trimmingCharacters(in: .whitespaces)
    } else if expr.lowercased().hasPrefix("oooc:") {
      expr = String(expr.dropFirst(5)).trimmingCharacters(in: .whitespaces)
    } else if expr.lowercased().hasPrefix("msoxl:") {
      expr = String(expr.dropFirst(6)).trimmingCharacters(in: .whitespaces)
    }
    if !expr.hasPrefix("=") {
      expr = "=" + expr
    }
    var normalized = ""
    normalized.reserveCapacity(expr.count)
    var inQuotes = false
    var inSingleQuotes = false
    var idx = expr.startIndex
    while idx < expr.endIndex {
      let ch = expr[idx]
      if ch == "\"" && !inSingleQuotes {
        inQuotes.toggle()
        normalized.append(ch)
        idx = expr.index(after: idx)
        continue
      }
      if ch == "'" && !inQuotes {
        inSingleQuotes.toggle()
        normalized.append(ch)
        idx = expr.index(after: idx)
        continue
      }
      if !inQuotes && !inSingleQuotes {
        if ch == "[" {
          var closeIdx = expr.index(after: idx)
          var bracketSq = false
          while closeIdx < expr.endIndex {
            if expr[closeIdx] == "'" {
              bracketSq.toggle()
            } else if expr[closeIdx] == "]" && !bracketSq {
              break
            }
            closeIdx = expr.index(after: closeIdx)
          }
          if closeIdx < expr.endIndex {
            let inner = String(expr[expr.index(after: idx)..<closeIdx])
            normalized.append(normalizeBracketedOpenFormulaRef(inner))
            idx = expr.index(after: closeIdx)
            continue
          }
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

  private static func normalizeBracketedOpenFormulaRef(_ inner: String) -> String {
    func splitSheetAndCell(_ part: String) -> (sheet: String?, cell: String) {
      let trimmed = part.trimmingCharacters(in: .whitespaces)
      if trimmed.hasPrefix(".") {
        return (nil, String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces))
      }
      var inSq = false
      var lastDot: String.Index?
      var scan = trimmed.startIndex
      while scan < trimmed.endIndex {
        if trimmed[scan] == "'" {
          inSq.toggle()
        } else if trimmed[scan] == "." && !inSq {
          lastDot = scan
        }
        scan = trimmed.index(after: scan)
      }
      if let dotIdx = lastDot {
        var rawSheet = String(trimmed[..<dotIdx]).trimmingCharacters(in: .whitespaces)
        if rawSheet.hasPrefix("$") {
          rawSheet = String(rawSheet.dropFirst())
        }
        if rawSheet.count >= 2 && rawSheet.hasPrefix("'") && rawSheet.hasSuffix("'") {
          rawSheet = String(rawSheet.dropFirst().dropLast()).replacingOccurrences(
            of: "''", with: "'")
        }
        let cellPart = String(trimmed[trimmed.index(after: dotIdx)...]).trimmingCharacters(
          in: .whitespaces)
        return (rawSheet.isEmpty ? nil : formatA1SheetPrefix(rawSheet), cellPart)
      }
      return (nil, trimmed)
    }

    if let (lhsStr, rhsStr) = splitOnceUnquoted(inner, delimiter: ":") {
      let left = splitSheetAndCell(lhsStr)
      let right = splitSheetAndCell(rhsStr)
      let leftStr = left.sheet.map { "\($0)!\(left.cell)" } ?? left.cell
      let rightStr: String
      if let rSheet = right.sheet, rSheet != left.sheet {
        rightStr = "\(rSheet)!\(right.cell)"
      } else {
        rightStr = right.cell
      }
      return "\(leftStr):\(rightStr)"
    }
    let single = splitSheetAndCell(inner)
    if let sheet = single.sheet {
      return "\(sheet)!\(single.cell)"
    }
    return single.cell
  }
}
