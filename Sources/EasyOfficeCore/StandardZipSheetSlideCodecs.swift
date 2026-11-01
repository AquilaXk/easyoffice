import Foundation

extension OOXMLContainerCodec {


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

    let mediaDefaultsXML = XMLDocumentModelMapper.mediaDefaultContentTypesXML(
      resolvedMedia: [],
      preservedPaths: Array(sheet.preservedArchiveEntries.keys)
    )
    let sheetOverrides = orderedPages.indices.map { idx in
      "  <Override PartName=\"/xl/worksheets/sheet\(idx + 1).xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
    }.joined(separator: "\n")

    let contentTypesXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
        <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
        <Default Extension="xml" ContentType="application/xml"/>
      \(mediaDefaultsXML)
        <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
      \(sheetOverrides)
        <Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>
      </Types>
      """

    let rootRelsXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
      </Relationships>
      """

    var workbookRelItems = orderedPages.indices.map { idx in
      "  <Relationship Id=\"rId\(idx + 1)\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet\(idx + 1).xml\"/>"
    }
    workbookRelItems.append(
      "  <Relationship Id=\"rId\(orderedPages.count + 1)\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/>"
    )
    let workbookRelsXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
      \(workbookRelItems.joined(separator: "\n"))
      </Relationships>
      """

    let sheetEntriesXML = orderedPages.enumerated().map { idx, page in
      "    <sheet name=\"\(XMLSanitizer.escape(page.name))\" sheetId=\"\(idx + 1)\" r:id=\"rId\(idx + 1)\" eo:rId=\"rId\(idx + 1)\"/>"
    }.joined(separator: "\n")

    let definedNamesXML: String
    if sheet.namedRanges.isEmpty {
      definedNamesXML = ""
    } else {
      let items = sheet.namedRanges.keys.sorted().compactMap { key -> String? in
        guard let val = sheet.namedRanges[key] else { return nil }
        return
          "    <definedName name=\"\(XMLSanitizer.escape(key))\">\(XMLSanitizer.escape(val))</definedName>"
      }.joined(separator: "\n")
      definedNamesXML = "\n  <definedNames>\n\(items)\n  </definedNames>"
    }

    let metaXML = XMLDocumentModelMapper.serializeWorksheetMetadata(sheet)
    let workbookXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:mc="\(XMLDocumentModelMapper.mcNamespaceURI)" xmlns:eo="\(XMLDocumentModelMapper.eoNamespaceURI)" mc:Ignorable="eo">
        \(metaXML)
        <sheets>
      \(sheetEntriesXML)
        </sheets>\(definedNamesXML)
      </workbook>
      """

    let siElements = sharedStrings.map {
      "<si><t xml:space=\"preserve\">\(XMLSanitizer.escape($0))</t></si>"
    }.joined(separator: "\n  ")
    let sharedStringsXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="\(totalStringOccurrences)" uniqueCount="\(sharedStrings.count)">
        \(siElements)
      </sst>
      """

    var entries: [ZipContainerEntry] = [
      ZipContainerEntry(
        path: "[Content_Types].xml", data: Data(contentTypesXML.utf8), compressionMethod: .deflate),
      ZipContainerEntry(
        path: "_rels/.rels", data: Data(rootRelsXML.utf8), compressionMethod: .deflate),
      ZipContainerEntry(
        path: "xl/workbook.xml", data: Data(workbookXML.utf8), compressionMethod: .deflate),
      ZipContainerEntry(
        path: "xl/_rels/workbook.xml.rels", data: Data(workbookRelsXML.utf8),
        compressionMethod: .deflate),
      ZipContainerEntry(
        path: "xl/sharedStrings.xml", data: Data(sharedStringsXML.utf8), compressionMethod: .deflate
      ),
    ]

    for (idx, page) in orderedPages.enumerated() {
      var evalSheet = sheet
      evalSheet.switchActiveSheet(to: idx)

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
            let evaluated = evalSheet.evaluate(at: coord)
            let cachedVal = XMLSanitizer.escape(evaluated.description)
            let typeAttr = (evaluated.numericValue != nil) ? "" : " t=\"str\""
            cellTags.append(
              "      <c r=\"\(XMLSanitizer.escape(a1))\"\(typeAttr)\(styleAttrs)><f>\(XMLSanitizer.escape(formulaBody))</f><v>\(cachedVal)</v></c>"
            )
          } else if !raw.isEmpty, Double(raw)?.isFinite == true {
            cellTags.append(
              "      <c r=\"\(XMLSanitizer.escape(a1))\"\(styleAttrs)><v>\(XMLSanitizer.escape(raw))</v></c>"
            )
          } else if !raw.isEmpty, let sstIdx = sharedStringIndex[raw] {
            cellTags.append(
              "      <c r=\"\(XMLSanitizer.escape(a1))\" t=\"s\"\(styleAttrs)><v>\(sstIdx)</v></c>")
          } else if !styleAttrs.isEmpty {
            cellTags.append("      <c r=\"\(XMLSanitizer.escape(a1))\"\(styleAttrs)/>")
          }
        }
        rowElements.append(
          """
              <row r="\(rowNum)">
          \(cellTags.joined(separator: "\n"))
              </row>
          """)
      }

      let mergeCellsXML: String
      if page.mergedRanges.isEmpty {
        mergeCellsXML = ""
      } else {
        let items = page.mergedRanges.map {
          "    <mergeCell ref=\"\(XMLSanitizer.escape($0.description))\"/>"
        }.joined(separator: "\n")
        mergeCellsXML =
          "\n  <mergeCells count=\"\(page.mergedRanges.count)\">\n\(items)\n  </mergeCells>"
      }

      let sheetXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:mc="\(XMLDocumentModelMapper.mcNamespaceURI)" xmlns:eo="\(XMLDocumentModelMapper.eoNamespaceURI)" mc:Ignorable="eo">
          <sheetData>
        \(rowElements.joined(separator: "\n"))
          </sheetData>\(mergeCellsXML)
        </worksheet>
        """
      entries.append(
        ZipContainerEntry(
          path: "xl/worksheets/sheet\(idx + 1).xml",
          data: Data(sheetXML.utf8),
          compressionMethod: .deflate
        ))
    }

    return try XMLDocumentModelMapper.packArchiveEntries(
      entries,
      preserved: sheet.preservedArchiveEntries
    )
  }

  public static func decodeWorksheet(from data: Data) throws -> WorksheetModel {
    let unpacked = try XMLDocumentModelMapper.unpackAndValidateArchiveEntries(from: data)
    let entryMap = unpacked.xmlEntries
    var preservedEntries = unpacked.preservedEntries

    var relTargets: [String: String] = [:]
    var sharedStringsRelPath: String? = nil
    if let relsXML = entryMap["xl/_rels/workbook.xml.rels"] {
      for rel in XMLSanitizer.extractElements(named: "Relationship", in: relsXML) {
        if let rId = rel.attributes["Id"], let rawTarget = rel.attributes["Target"] {
          let normalized: String
          if rawTarget.hasPrefix("/") {
            normalized = String(rawTarget.dropFirst())
          } else if rawTarget.hasPrefix("xl/") {
            normalized = rawTarget
          } else {
            normalized = "xl/\(rawTarget)"
          }
          relTargets[rId] = normalized
          if let relType = rel.attributes["Type"], relType.hasSuffix("/sharedStrings") {
            sharedStringsRelPath = normalized
          }
        }
      }
    }

    let resolvedSSTPath = sharedStringsRelPath ?? "xl/sharedStrings.xml"
    var sharedStrings: [String] = []
    if let sstXML = entryMap[resolvedSSTPath] ?? entryMap["xl/sharedStrings.xml"] {
      for siElem in XMLSanitizer.extractElements(named: "si", in: sstXML) {
        let strippedPhonetic = XMLSanitizer.stripElements(named: "rPh", from: siElem.innerXML)
        let textParts = XMLSanitizer.extractElements(named: "t", in: strippedPhonetic).map {
          XMLSanitizer.unescape($0.innerXML)
        }
        sharedStrings.append(textParts.joined())
      }
    }

    let workbookXML = entryMap["xl/workbook.xml"] ?? ""
    let sheetTags = XMLSanitizer.extractElements(named: "sheet", in: workbookXML)
    let sheetDescriptors: [(name: String, path: String)] =
      sheetTags.isEmpty
      ? [("Sheet1", "xl/worksheets/sheet1.xml")]
      : sheetTags.enumerated().map { idx, elem in
        let sheetName = elem.attributes["name"] ?? "Sheet\(idx + 1)"
        let rId =
          elem.attributes["r:id"] ?? elem.attributes["eo:rId"] ?? elem.attributes["rId"]
        let defaultPath = "xl/worksheets/sheet\(idx + 1).xml"
        if let rId, let mappedPath = relTargets[rId], entryMap[mappedPath] != nil {
          return (sheetName, mappedPath)
        }
        return (sheetName, defaultPath)
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
            sstIndex >= 0, sstIndex < sharedStrings.count
          else {
            throw EasyOfficeError.corruptedContainer(
              detail: "XLSX sharedStrings 인덱스 참조가 유효하지 않습니다 (cell: \(addr)).")
          }
          try pageModel.setCell(addr, rawValue: sharedStrings[sstIndex])
        } else if cellType == "inlineStr" {
          let strippedPhonetic = XMLSanitizer.stripElements(named: "rPh", from: cellElem.innerXML)
          let tElems = XMLSanitizer.extractElements(named: "t", in: strippedPhonetic)
          if !tElems.isEmpty {
            try pageModel.setCell(
              addr, rawValue: tElems.map { XMLSanitizer.unescape($0.innerXML) }.joined())
          } else if let vElem = XMLSanitizer.extractElements(named: "v", in: cellElem.innerXML)
            .first
          {
            try pageModel.setCell(addr, rawValue: XMLSanitizer.unescape(vElem.innerXML))
          }
        } else if cellType == "b" {
          if let vElem = XMLSanitizer.extractElements(named: "v", in: cellElem.innerXML).first {
            let trimmed = vElem.innerXML.trimmingCharacters(in: .whitespacesAndNewlines)
            let boolStr =
              (trimmed == "1" || trimmed.caseInsensitiveCompare("TRUE") == .orderedSame)
              ? "TRUE" : "FALSE"
            try pageModel.setCell(addr, rawValue: boolStr)
          }
        } else if let vElem = XMLSanitizer.extractElements(named: "v", in: cellElem.innerXML).first
        {
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
      throw EasyOfficeError.corruptedContainer(
        detail: "OOXML XLSX 필수 스트림 'xl/worksheets/sheet1.xml'이 누락되었습니다.")
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
    var coreSheetPaths: Set<String> = [
      "[Content_Types].xml", "_rels/.rels", "xl/workbook.xml", "xl/_rels/workbook.xml.rels",
      "xl/sharedStrings.xml", resolvedSSTPath,
    ]
    for desc in sheetDescriptors {
      coreSheetPaths.insert(desc.path)
    }
    for path in coreSheetPaths {
      preservedEntries.removeValue(forKey: path)
    }
    primary.preservedArchiveEntries = preservedEntries
    return primary
  }


  public static func encodePresentation(_ deck: PresentationDeckModel) throws -> Data {
    var entries: [ZipContainerEntry] = []

    let mediaDefaultsXML = XMLDocumentModelMapper.mediaDefaultContentTypesXML(
      resolvedMedia: [],
      preservedPaths: Array(deck.preservedArchiveEntries.keys)
    )
    var overrides: [String] = [
      "  <Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/>",
      "  <Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/>",
      "  <Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/>",
    ]
    for (idx, slide) in deck.slides.enumerated() {
      overrides.append(
        "  <Override PartName=\"/ppt/slides/slide\(idx + 1).xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"
      )
      if !slide.speakerNotes.isEmpty {
        overrides.append(
          "  <Override PartName=\"/ppt/notesSlides/notesSlide\(idx + 1).xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml\"/>"
        )
      }
    }

    let contentTypesXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
        <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
        <Default Extension="xml" ContentType="application/xml"/>
      \(mediaDefaultsXML)
      \(overrides.joined(separator: "\n"))
      </Types>
      """
    entries.append(
      ZipContainerEntry(
        path: "[Content_Types].xml", data: Data(contentTypesXML.utf8), compressionMethod: .deflate))

    let rootRelsXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
      </Relationships>
      """
    entries.append(
      ZipContainerEntry(
        path: "_rels/.rels", data: Data(rootRelsXML.utf8), compressionMethod: .deflate))

    var presRelItems = [
      "  <Relationship Id=\"rIdMaster1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster\" Target=\"slideMasters/slideMaster1.xml\"/>"
    ]
    for idx in deck.slides.indices {
      presRelItems.append(
        "  <Relationship Id=\"rId\(idx + 1)\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide\(idx + 1).xml\"/>"
      )
    }
    let presRelsXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
      \(presRelItems.joined(separator: "\n"))
      </Relationships>
      """
    entries.append(
      ZipContainerEntry(
        path: "ppt/_rels/presentation.xml.rels", data: Data(presRelsXML.utf8),
        compressionMethod: .deflate))

    let slideMasterXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <p:sldMaster xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
        <p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld>
        <p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>
        <p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rIdLayout1"/></p:sldLayoutIdLst>
      </p:sldMaster>
      """
    let slideMasterRelsXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rIdLayout1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
      </Relationships>
      """
    let slideLayoutXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <p:sldLayout xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" type="tx">
        <p:cSld name="Title and Content"><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld>
      </p:sldLayout>
      """
    let slideLayoutRelsXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rIdMaster1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/>
      </Relationships>
      """
    entries.append(
      ZipContainerEntry(
        path: "ppt/slideMasters/slideMaster1.xml", data: Data(slideMasterXML.utf8),
        compressionMethod: .deflate))
    entries.append(
      ZipContainerEntry(
        path: "ppt/slideMasters/_rels/slideMaster1.xml.rels", data: Data(slideMasterRelsXML.utf8),
        compressionMethod: .deflate))
    entries.append(
      ZipContainerEntry(
        path: "ppt/slideLayouts/slideLayout1.xml", data: Data(slideLayoutXML.utf8),
        compressionMethod: .deflate))
    entries.append(
      ZipContainerEntry(
        path: "ppt/slideLayouts/_rels/slideLayout1.xml.rels", data: Data(slideLayoutRelsXML.utf8),
        compressionMethod: .deflate))

    let slideRefs = deck.slides.indices.map {
      "    <p:sldId id=\"\(256 + $0)\" r:id=\"rId\($0 + 1)\" eo:target=\"ppt/slides/slide\($0 + 1).xml\"/>"
    }
    let presentationXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:mc="\(XMLDocumentModelMapper.mcNamespaceURI)" xmlns:eo="\(XMLDocumentModelMapper.eoNamespaceURI)" mc:Ignorable="eo" eo:title="\(XMLSanitizer.escape(deck.title))" eo:aspectRatio="\(XMLSanitizer.escape(deck.aspectRatio.rawValue))" eo:deckTheme="\(XMLSanitizer.escape(deck.deckTheme.rawValue))">
        <p:sldMasterIdLst>
          <p:sldMasterId id="2147483648" r:id="rIdMaster1"/>
        </p:sldMasterIdLst>
        <p:sldIdLst>
      \(slideRefs.joined(separator: "\n"))
        </p:sldIdLst>
      </p:presentation>
      """
    entries.append(
      ZipContainerEntry(
        path: "ppt/presentation.xml", data: Data(presentationXML.utf8), compressionMethod: .deflate)
    )

    for (index, slide) in deck.slides.enumerated() {
      let bulletsXML = slide.bulletPoints.map { "<eo:bullet>\(XMLSanitizer.escape($0))</eo:bullet>" }
        .joined(separator: "\n    ")
      let elementsXML = slide.elements.enumerated().map { elIdx, el in
        let attrs = XMLDocumentModelMapper.slideElementAttributes(el)
        let rPrAttrs =
          "b=\"\(el.isBold ? "1" : "0")\" i=\"\(el.isItalic ? "1" : "0")\" u=\"\(el.isUnderlined ? "sng" : "none")\""
        let offX = XMLDocumentModelMapper.safeClampedInt(el.x)
        let offY = XMLDocumentModelMapper.safeClampedInt(el.y)
        let extW = XMLDocumentModelMapper.safeClampedInt(el.width, min: 0)
        let extH = XMLDocumentModelMapper.safeClampedInt(el.height, min: 0)
        return """
              <p:sp \(attrs)>
                <p:nvSpPr><p:cNvPr id="\(elIdx + 2)" name="\(XMLSanitizer.escape(el.title))"/></p:nvSpPr>
                <p:spPr><a:xfrm><a:off x="\(offX)" y="\(offY)"/><a:ext cx="\(extW)" cy="\(extH)"/></a:xfrm></p:spPr>
                <p:txBody><a:bodyPr/><a:p><a:r><a:rPr \(rPrAttrs)/><a:t>\(XMLSanitizer.escape(el.bodyText))</a:t></a:r></a:p></p:txBody>
              </p:sp>
          """
      }.joined(separator: "\n")

      let durationAttr = String(
        format: "eo:transitionDuration=\"%.2f\"",
        slide.transitionDurationSeconds
      )
      let autoAdvanceAttr = slide.autoAdvanceSeconds.map { " eo:autoAdvance=\"\($0)\"" } ?? ""
      let slideXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:mc="\(XMLDocumentModelMapper.mcNamespaceURI)" xmlns:eo="\(XMLDocumentModelMapper.eoNamespaceURI)" mc:Ignorable="eo" eo:id="\(slide.id.uuidString)" eo:title="\(XMLSanitizer.escape(slide.title))" eo:subtitle="\(XMLSanitizer.escape(slide.subtitle))" eo:layout="\(XMLSanitizer.escape(slide.masterLayout.rawValue))" eo:transition="\(XMLSanitizer.escape(slide.transition.rawValue))" \(durationAttr)\(autoAdvanceAttr)>
          <eo:notes>\(XMLSanitizer.escape(slide.speakerNotes))</eo:notes>
          <eo:bullets>
            \(bulletsXML)
          </eo:bullets>
          <p:cSld>
            <p:spTree>
        \(elementsXML)
            </p:spTree>
          </p:cSld>
        </p:sld>
        """
      entries.append(
        ZipContainerEntry(
          path: "ppt/slides/slide\(index + 1).xml", data: Data(slideXML.utf8),
          compressionMethod: .deflate))

      var slideRelItems = [
        "  <Relationship Id=\"rIdLayout1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/>"
      ]
      if !slide.speakerNotes.isEmpty {
        slideRelItems.append(
          "  <Relationship Id=\"rIdNotes1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide\" Target=\"../notesSlides/notesSlide\(index + 1).xml\"/>"
        )
        let notesSlideXML = """
          <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
          <p:notes xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
            <p:cSld><p:spTree><p:sp><p:txBody><a:bodyPr/><a:p><a:r><a:t>\(XMLSanitizer.escape(slide.speakerNotes))</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld>
          </p:notes>
          """
        entries.append(
          ZipContainerEntry(
            path: "ppt/notesSlides/notesSlide\(index + 1).xml", data: Data(notesSlideXML.utf8),
            compressionMethod: .deflate))
      }
      let slideRelsXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        \(slideRelItems.joined(separator: "\n"))
        </Relationships>
        """
      entries.append(
        ZipContainerEntry(
          path: "ppt/slides/_rels/slide\(index + 1).xml.rels", data: Data(slideRelsXML.utf8),
          compressionMethod: .deflate))
    }

    return try XMLDocumentModelMapper.packArchiveEntries(
      entries,
      preserved: deck.preservedArchiveEntries
    )
  }

  public static func decodePresentation(from data: Data) throws -> PresentationDeckModel {
    let unpacked = try XMLDocumentModelMapper.unpackAndValidateArchiveEntries(from: data)
    let entryMap = unpacked.xmlEntries
    var preservedEntries = unpacked.preservedEntries
    guard let presentationXML = entryMap["ppt/presentation.xml"] else {
      throw EasyOfficeError.corruptedContainer(
        detail: "OOXML PPTX 필수 스트림 'ppt/presentation.xml'이 누락되었습니다.")
    }

    let presRoot = XMLSanitizer.extractElements(named: "p:presentation", in: presentationXML).first
    let presAttrs = presRoot?.attributes ?? [:]
    let title = XMLDocumentModelMapper.attr("title", in: presAttrs) ?? "발표자료"
    let aspectRatio =
      SlideAspectRatio(rawValue: XMLDocumentModelMapper.attr("aspectRatio", in: presAttrs) ?? "")
      ?? .widescreen16x9
    let deckTheme =
      SlideDeckTheme(rawValue: XMLDocumentModelMapper.attr("deckTheme", in: presAttrs) ?? "")
      ?? .signatureLavender

    var relTargets: [String: String] = [:]
    if let relsXML = entryMap["ppt/_rels/presentation.xml.rels"] {
      for rel in XMLSanitizer.extractElements(named: "Relationship", in: relsXML) {
        if let rId = rel.attributes["Id"], let rawTarget = rel.attributes["Target"] {
          let normalized: String
          if rawTarget.hasPrefix("/") {
            normalized = String(rawTarget.dropFirst())
          } else if rawTarget.hasPrefix("ppt/") {
            normalized = rawTarget
          } else {
            normalized = "ppt/\(rawTarget)"
          }
          relTargets[rId] = normalized
        }
      }
    }

    let slideRefs = XMLSanitizer.extractElements(named: "p:sldId", in: presentationXML)
    let resolvedSlidePaths: [String]
    if !slideRefs.isEmpty {
      resolvedSlidePaths = slideRefs.enumerated().map { idx, ref in
        if let rId = ref.attributes["r:id"] ?? ref.attributes["id"],
          let relPath = relTargets[rId],
          entryMap[relPath] != nil
        {
          return relPath
        }
        if let target = XMLDocumentModelMapper.attr("target", in: ref.attributes) {
          return target
        }
        return "ppt/slides/slide\(idx + 1).xml"
      }
    } else {
      resolvedSlidePaths = entryMap.keys
        .filter { $0.hasPrefix("ppt/slides/slide") && $0.hasSuffix(".xml") }
        .sorted { lhs, rhs in
          let lNum = Int(
            lhs.replacingOccurrences(of: "ppt/slides/slide", with: "")
              .replacingOccurrences(of: ".xml", with: "")) ?? Int.max
          let rNum = Int(
            rhs.replacingOccurrences(of: "ppt/slides/slide", with: "")
              .replacingOccurrences(of: ".xml", with: "")) ?? Int.max
          return lNum != rNum ? lNum < rNum : lhs < rhs
        }
    }

    var slides: [SlideModel] = []
    var consumedPaths: Set<String> = [
      "[Content_Types].xml", "_rels/.rels", "ppt/presentation.xml",
      "ppt/_rels/presentation.xml.rels",
      "ppt/slideMasters/slideMaster1.xml",
      "ppt/slideMasters/_rels/slideMaster1.xml.rels",
      "ppt/slideLayouts/slideLayout1.xml",
      "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
    ]
    for (idx, target) in resolvedSlidePaths.enumerated() {
      guard let slideXML = entryMap[target] else {
        throw EasyOfficeError.corruptedContainer(detail: "PPTX 슬라이드 파트 누락: \(target)")
      }
      consumedPaths.insert(target)
      guard let sldElem = XMLSanitizer.extractElements(named: "p:sld", in: slideXML).first else {
        continue
      }
      let sldAttrs = sldElem.attributes
      let slideID = XMLDocumentModelMapper.attr("id", in: sldAttrs).flatMap(UUID.init(uuidString:)) ?? UUID()
      var placeholderTitle: String? = nil
      var placeholderSubtitle: String? = nil
      var placeholderBullets: [String] = []

      let elements = XMLSanitizer.extractElements(named: "p:sp", in: sldElem.innerXML).map {
        sp -> SlideCanvasElement in
        var el = XMLDocumentModelMapper.parseSlideElement(
          attributes: sp.attributes, innerXML: sp.innerXML)
        if XMLDocumentModelMapper.attr("w", in: sp.attributes) == nil
          && (el.width >= 12_700 || el.height >= 12_700)
        {
          el.x /= 12_700.0
          el.y /= 12_700.0
          el.width /= 12_700.0
          el.height /= 12_700.0
        }
        let aParagraphs = XMLSanitizer.extractElements(named: "a:p", in: sp.innerXML)
        let paragraphTexts: [String] = aParagraphs.compactMap { aP in
          let text = XMLSanitizer.extractElements(named: "a:t", in: aP.innerXML)
            .map { XMLSanitizer.unescape($0.innerXML) }
            .joined()
          return text.isEmpty ? nil : text
        }
        if paragraphTexts.count > 1 {
          el.bodyText = paragraphTexts.joined(separator: "\n")
        }

        let phType = XMLSanitizer.extractElements(named: "p:ph", in: sp.innerXML).first?
          .attributes["type"]?.lowercased()
        let lowerName = el.title.lowercased()
        if phType == "title" || phType == "ctrtitle" || lowerName.hasPrefix("title") {
          if placeholderTitle == nil, !el.bodyText.isEmpty {
            placeholderTitle = el.bodyText
          }
        } else if phType == "subtitle" || lowerName.hasPrefix("subtitle") {
          if placeholderSubtitle == nil, !el.bodyText.isEmpty {
            placeholderSubtitle = el.bodyText
          }
        } else if phType == "body" || phType == "obj" || lowerName.hasPrefix("content")
          || lowerName.hasPrefix("body")
        {
          if !paragraphTexts.isEmpty {
            placeholderBullets.append(contentsOf: paragraphTexts)
          } else if !el.bodyText.isEmpty {
            placeholderBullets.append(el.bodyText)
          }
        }
        return el
      }

      let explicitTitle = XMLDocumentModelMapper.attr("title", in: sldAttrs)
      let sTitle =
        explicitTitle
        ?? placeholderTitle
        ?? elements.first(where: { !$0.bodyText.isEmpty })?.bodyText
        ?? elements.first?.title
        ?? ""
      let sSubtitle = XMLDocumentModelMapper.attr("subtitle", in: sldAttrs) ?? placeholderSubtitle ?? ""
      let layout =
        SlideMasterLayout(rawValue: XMLDocumentModelMapper.attr("layout", in: sldAttrs) ?? "")
        ?? .titleAndContent
      let transition =
        SlideTransitionKind(rawValue: XMLDocumentModelMapper.attr("transition", in: sldAttrs) ?? "")
        ?? .fade
      let transitionDuration =
        XMLDocumentModelMapper.attr("transitionDuration", in: sldAttrs)
        .flatMap(Double.init) ?? 0.8
      let autoAdvance =
        XMLDocumentModelMapper.attr("autoAdvance", in: sldAttrs)
        .flatMap(Int.init)

      var notes =
        (XMLSanitizer.extractElements(named: "eo:notes", in: sldElem.innerXML).first
        ?? XMLSanitizer.extractElements(named: "p:notes", in: sldElem.innerXML).first).map {
          XMLSanitizer.unescape($0.innerXML)
        } ?? ""
      let notesPath = "ppt/notesSlides/notesSlide\(idx + 1).xml"
      consumedPaths.insert(notesPath)
      if notes.isEmpty, let notesSlideXML = entryMap[notesPath] {
        let noteTexts = XMLSanitizer.extractElements(named: "a:t", in: notesSlideXML).map {
          XMLSanitizer.unescape($0.innerXML)
        }
        notes = noteTexts.joined(separator: "\n")
      }

      let eoBullets = XMLSanitizer.extractElements(named: "eo:bullet", in: sldElem.innerXML)
      let pBullets = XMLSanitizer.extractElements(named: "p:bullet", in: sldElem.innerXML)
      var bullets = (eoBullets.isEmpty ? pBullets : eoBullets).map {
        XMLSanitizer.unescape($0.innerXML)
      }
      if bullets.isEmpty && explicitTitle == nil && !placeholderBullets.isEmpty {
        bullets = placeholderBullets
      }

      slides.append(
        SlideModel(
          id: slideID,
          title: sTitle,
          subtitle: sSubtitle,
          bulletPoints: bullets,
          speakerNotes: notes,
          masterLayout: layout,
          transition: transition,
          elements: elements,
          transitionDurationSeconds: transitionDuration,
          autoAdvanceSeconds: autoAdvance
        )
      )
    }

    for path in consumedPaths {
      preservedEntries.removeValue(forKey: path)
    }
    var deck = PresentationDeckModel(
      title: title, aspectRatio: aspectRatio, slides: slides, deckTheme: deckTheme)
    deck.preservedArchiveEntries = preservedEntries
    return deck
  }
}
