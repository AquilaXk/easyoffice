import Foundation

// MARK: - Resolved Media Entry

/// Resolved media attachment descriptor computed once per archive pack operation so XML metadata, relationships, and ZIP entries never diverge.
public struct ResolvedMediaEntry: Sendable, Equatable {
  public let attachment: EmbeddedImageAttachment
  public let originalSafeFileName: String
  public let deduplicatedFileName: String
  public let archivePath: String
}

// MARK: - Shared XML Document, Worksheet & Presentation Model Mapper

/// Shared mapper eliminating duplicate paragraph, run, image, table, worksheet, slide, and metadata XML serialization across OOXML, ODF, and HWPX.
public enum XMLDocumentModelMapper {
  public static let eoNamespaceURI = "https://easyoffice.local/ns/meta"
  public static let mcNamespaceURI = "http://schemas.openxmlformats.org/markup-compatibility/2006"

  public static func attr(_ key: String, in attributes: [String: String]) -> String? {
    attributes["eo:\(key)"] ?? attributes[key]
  }

  public static func sanitizeMediaFileName(_ raw: String) -> String {
    let replacedSeparators =
      raw
      .replacingOccurrences(of: "\\", with: "/")
      .replacingOccurrences(of: ":", with: "_")
    let base = (replacedSeparators as NSString).lastPathComponent
    var scalars: [UnicodeScalar] = []
    scalars.reserveCapacity(base.unicodeScalars.count)
    for scalar in base.unicodeScalars {
      if scalar.value < 0x20 || scalar.value == 0x7F {
        continue
      }
      if scalar == "/" || scalar == "\\" || scalar == ":" {
        scalars.append("_")
      } else {
        scalars.append(scalar)
      }
    }
    var cleaned = String(String.UnicodeScalarView(scalars))
    while cleaned.contains("..") {
      cleaned = cleaned.replacingOccurrences(of: "..", with: "_")
    }
    cleaned = cleaned.trimmingCharacters(in: .whitespacesAndNewlines)
    if cleaned.isEmpty || cleaned.allSatisfy({ $0 == "." }) {
      return "image.bin"
    }
    if cleaned.utf8.count > 255 {
      let nsCleaned = cleaned as NSString
      let ext = nsCleaned.pathExtension
      let stem = nsCleaned.deletingPathExtension
      let extSuffix = ext.isEmpty ? "" : ".\(ext)"
      let maxStemBytes = max(1, 255 - extSuffix.utf8.count)
      var truncatedStem = ""
      for ch in stem {
        let next = truncatedStem + String(ch)
        if next.utf8.count > maxStemBytes {
          break
        }
        truncatedStem = next
      }
      if truncatedStem.isEmpty {
        return "image.bin"
      }
      cleaned = truncatedStem + extSuffix
    }
    return cleaned
  }

  public static func resolveUniqueMediaEntries(
    images: [EmbeddedImageAttachment],
    mediaFolderPrefix: String,
    reservedPaths: Set<String> = []
  ) -> [ResolvedMediaEntry] {
    guard !mediaFolderPrefix.isEmpty, !images.isEmpty else { return [] }
    var usedPathsLower = Set(reservedPaths.map { $0.lowercased() })
    var resolved: [ResolvedMediaEntry] = []
    resolved.reserveCapacity(images.count)

    for img in images {
      let origSafe = sanitizeMediaFileName(img.fileName)
      var finalFileName = origSafe
      var candidatePath = "\(mediaFolderPrefix)\(finalFileName)"
      if usedPathsLower.contains(candidatePath.lowercased()) {
        let nsName = origSafe as NSString
        let stem = nsName.deletingPathExtension.isEmpty ? "image" : nsName.deletingPathExtension
        let ext = nsName.pathExtension
        var counter = 2
        while usedPathsLower.contains(candidatePath.lowercased()) {
          finalFileName = ext.isEmpty ? "\(stem)_\(counter)" : "\(stem)_\(counter).\(ext)"
          candidatePath = "\(mediaFolderPrefix)\(finalFileName)"
          counter += 1
        }
      }
      usedPathsLower.insert(candidatePath.lowercased())
      resolved.append(
        ResolvedMediaEntry(
          attachment: img,
          originalSafeFileName: origSafe,
          deduplicatedFileName: finalFileName,
          archivePath: candidatePath
        )
      )
    }
    return resolved
  }

  public static func mimeTypeForMediaExtension(_ ext: String) -> String {
    switch ext.lowercased() {
    case "png": return "image/png"
    case "jpg", "jpeg": return "image/jpeg"
    case "gif": return "image/gif"
    case "svg": return "image/svg+xml"
    case "bmp": return "image/bmp"
    case "webp": return "image/webp"
    case "tif", "tiff": return "image/tiff"
    default: return "application/octet-stream"
    }
  }

  public static func mediaDefaultContentTypesXML(
    resolvedMedia: [ResolvedMediaEntry],
    preservedPaths: [String] = []
  ) -> String {
    var extensions: Set<String> = ["png", "jpg"]
    for media in resolvedMedia {
      let ext = (media.deduplicatedFileName as NSString).pathExtension.lowercased()
      if !ext.isEmpty && ext != "rels" && ext != "xml" {
        extensions.insert(ext)
      }
    }
    for path in preservedPaths where isBinaryMediaPath(path) {
      let ext = (path as NSString).pathExtension.lowercased()
      if !ext.isEmpty && ext != "rels" && ext != "xml" {
        extensions.insert(ext)
      }
    }
    return extensions.sorted().map { ext in
      "  <Default Extension=\"\(XMLSanitizer.escape(ext))\" ContentType=\"\(mimeTypeForMediaExtension(ext))\"/>"
    }.joined(separator: "\n")
  }

  public static func ooxmlHighlightColorName(for raw: String) -> String {
    let cleaned = raw.trimmingCharacters(in: .whitespacesAndNewlines)
      .replacingOccurrences(of: "#", with: "")
    let validNames: [String: String] = [
      "yellow": "yellow", "green": "green", "cyan": "cyan", "magenta": "magenta",
      "blue": "blue", "red": "red", "darkblue": "darkBlue", "darkcyan": "darkCyan",
      "darkgreen": "darkGreen", "darkmagenta": "darkMagenta", "darkred": "darkRed",
      "darkyellow": "darkYellow", "darkgray": "darkGray", "lightgray": "lightGray",
      "black": "black", "white": "white",
    ]
    if let exact = validNames[cleaned.lowercased()] {
      return exact
    }
    guard cleaned.count == 6, let rgb = UInt32(cleaned, radix: 16) else {
      return "yellow"
    }
    let r = Int((rgb >> 16) & 0xFF)
    let g = Int((rgb >> 8) & 0xFF)
    let b = Int(rgb & 0xFF)
    let palette: [(name: String, r: Int, g: Int, b: Int)] = [
      ("yellow", 255, 255, 0),
      ("green", 0, 255, 0),
      ("cyan", 0, 255, 255),
      ("magenta", 255, 0, 255),
      ("blue", 0, 0, 255),
      ("red", 255, 0, 0),
      ("darkBlue", 0, 0, 128),
      ("darkCyan", 0, 128, 128),
      ("darkGreen", 0, 128, 0),
      ("darkMagenta", 128, 0, 128),
      ("darkRed", 128, 0, 0),
      ("darkYellow", 128, 128, 0),
      ("darkGray", 128, 128, 128),
      ("lightGray", 192, 192, 192),
      ("black", 0, 0, 0),
      ("white", 255, 255, 255),
    ]
    var bestName = "yellow"
    var bestDist = Int.max
    for candidate in palette {
      let dr = r - candidate.r
      let dg = g - candidate.g
      let db = b - candidate.b
      let dist = dr * dr + dg * dg + db * db
      if dist < bestDist {
        bestDist = dist
        bestName = candidate.name
      }
    }
    return bestName
  }

  static func isBinaryMediaPath(_ path: String) -> Bool {
    path.hasPrefix("word/media/") || path.hasPrefix("Pictures/") || path.hasPrefix("BinData/")
      || path.hasPrefix("ppt/media/") || path.hasPrefix("xl/media/")
  }

  static func isXMLValidatedPath(_ path: String) -> Bool {
    let lower = path.lowercased()
    return lower.hasSuffix(".xml") || lower.hasSuffix(".rels") || lower.hasSuffix(".hpf")
      || lower == "mimetype"
  }

  static func isSafePreservedArchivePath(_ path: String, archiveEntryPaths: Set<String>) -> Bool {
    let lower = path.lowercased()
    if lower.hasSuffix(".rels") || lower == "[content_types].xml" || lower == "mimetype" {
      return false
    }
    if archiveEntryPaths.contains("word/document.xml") {
      return path == "word/styles.xml" || path == "word/fontTable.xml"
        || path == "word/numbering.xml" || path == "word/webSettings.xml"
        || path.hasPrefix("word/theme/") || path.hasPrefix("word/media/")
        || path.hasPrefix("word/customXml/") || path.hasPrefix("customXml/")
    }
    if archiveEntryPaths.contains("xl/workbook.xml") {
      return path == "xl/styles.xml" || path.hasPrefix("xl/theme/")
        || path.hasPrefix("xl/media/") || path.hasPrefix("customXml/")
    }
    if archiveEntryPaths.contains("ppt/presentation.xml") {
      return path.hasPrefix("ppt/slideMasters/") || path.hasPrefix("ppt/slideLayouts/")
        || path.hasPrefix("ppt/theme/") || path.hasPrefix("ppt/media/")
        || path == "ppt/tableStyles.xml" || path == "ppt/presProps.xml"
        || path == "ppt/viewProps.xml" || path.hasPrefix("customXml/")
    }
    if archiveEntryPaths.contains("Contents/content.hpf") {
      return path.hasPrefix("BinData/") || path == "settings.xml" || path.hasPrefix("Preview/")
    }
    if archiveEntryPaths.contains("content.xml") {
      return path == "styles.xml" || path == "meta.xml" || path == "settings.xml"
        || path.hasPrefix("Pictures/") || path.hasPrefix("Configurations2/")
    }
    return true
  }

  static func packArchiveEntries(
    _ baseEntries: [ZipContainerEntry],
    resolvedMedia: [ResolvedMediaEntry] = [],
    preserved: [String: Data]
  ) throws -> Data {
    var entries = baseEntries
    var existingPaths = Set(entries.map(\.path))
    var existingPathsLower = Set(entries.map { $0.path.lowercased() })
    for media in resolvedMedia where !existingPathsLower.contains(media.archivePath.lowercased()) {
      existingPaths.insert(media.archivePath)
      existingPathsLower.insert(media.archivePath.lowercased())
      entries.append(
        ZipContainerEntry(
          path: media.archivePath,
          data: media.attachment.imageData,
          compressionMethod: .store
        )
      )
    }
    for path in preserved.keys.sorted()
    where !existingPathsLower.contains(path.lowercased())
      && isSafePreservedArchivePath(path, archiveEntryPaths: existingPaths)
    {
      if let rawData = preserved[path] {
        existingPaths.insert(path)
        existingPathsLower.insert(path.lowercased())
        entries.append(
          ZipContainerEntry(
            path: path,
            data: rawData,
            compressionMethod: isXMLValidatedPath(path) ? .deflate : .store
          ))
      }
    }
    return try ZipContainerCodec.pack(entries: entries)
  }

  static func unpackAndValidateArchiveEntries(
    from data: Data,
    excludedPreservedPaths: Set<String> = []
  ) throws -> (
    entries: [ZipContainerEntry],
    xmlEntries: [String: String],
    binaryEntries: [String: Data],
    preservedEntries: [String: Data]
  ) {
    let entries = try ZipContainerCodec.unpack(from: data)
    let allEntryPaths = Set(entries.map(\.path))
    var xmlEntries: [String: String] = [:]
    var binaryEntries: [String: Data] = [:]
    var preservedEntries: [String: Data] = [:]
    for entry in entries {
      if isBinaryMediaPath(entry.path) {
        binaryEntries[entry.path] = entry.data
      } else if isXMLValidatedPath(entry.path) {
        xmlEntries[entry.path] = try XMLSanitizer.validateSafeXMLData(entry.data)
      }
      if !excludedPreservedPaths.contains(entry.path)
        && isSafePreservedArchivePath(entry.path, archiveEntryPaths: allEntryPaths)
      {
        preservedEntries[entry.path] = entry.data
      }
    }
    return (entries, xmlEntries, binaryEntries, preservedEntries)
  }

  public static func safeClampedInt(
    _ value: Double, min minVal: Int = -100_000_000, max maxVal: Int = 100_000_000
  ) -> Int {
    guard value.isFinite else { return Swift.max(minVal, Swift.min(maxVal, 0)) }
    let effectiveMin = Swift.max(Double(minVal), -9223372036854775808.0)
    let effectiveMax = Swift.min(Double(maxVal), 9223372036854774784.0)
    let clamped = Swift.max(effectiveMin, Swift.min(effectiveMax, value.rounded()))
    return Int(clamped)
  }

  public static func safeClampedUInt32(
    _ value: Double, min minVal: UInt32 = 0, max maxVal: UInt32 = 100_000_000
  ) -> UInt32 {
    guard value.isFinite else { return minVal }
    let clamped = Swift.max(Double(minVal), Swift.min(Double(maxVal), value.rounded()))
    return UInt32(clamped)
  }

  static func paragraphAttributes(_ p: ParagraphBlock) -> String {
    let boldAttr = p.isBold ? "1" : "0"
    let italicAttr = p.isItalic ? "1" : "0"
    let underlineAttr = p.isUnderlined ? "1" : "0"
    let hasRunsAttr = p.runs.isEmpty ? "" : " eo:hasRuns=\"1\""
    var indentAttrs = ""
    if p.firstLineIndentPt != 0 {
      indentAttrs += " eo:firstLineIndent=\"\(p.firstLineIndentPt)\""
    }
    if p.leftIndentPt != 0 {
      indentAttrs += " eo:leftIndent=\"\(p.leftIndentPt)\""
    }
    if p.listKind != .none || p.listLevel != 0 {
      indentAttrs +=
        " eo:listKind=\"\(XMLSanitizer.escape(p.listKind.rawValue))\" eo:listLevel=\"\(p.listLevel)\""
    }
    return
      "eo:id=\"\(p.id.uuidString)\" eo:style=\"\(XMLSanitizer.escape(p.style.rawValue))\" eo:align=\"\(XMLSanitizer.escape(p.alignment.rawValue))\" eo:font=\"\(XMLSanitizer.escape(p.fontName))\" eo:size=\"\(p.fontSizePt)\" eo:bold=\"\(boldAttr)\" eo:italic=\"\(italicAttr)\" eo:underline=\"\(underlineAttr)\" eo:lineSpacing=\"\(p.lineSpacingPercent)\" eo:letterSpacing=\"\(p.letterSpacingPercent)\" eo:charScale=\"\(p.charWidthScalePercent)\"\(indentAttrs)\(hasRunsAttr)"
  }

  static func runAttributes(_ run: DocumentTextRun) -> String {
    var parts: [String] = [
      "eo:id=\"\(run.id.uuidString)\"",
      "eo:bold=\"\(run.isBold ? "1" : "0")\"",
      "eo:italic=\"\(run.isItalic ? "1" : "0")\"",
      "eo:underline=\"\(run.isUnderlined ? "1" : "0")\"",
    ]
    if run.isStrikethrough { parts.append("eo:strike=\"1\"") }
    if let fontName = run.fontName {
      parts.append("eo:font=\"\(XMLSanitizer.escape(fontName))\"")
    }
    if let fontSizePt = run.fontSizePt { parts.append("eo:size=\"\(fontSizePt)\"") }
    if let fg = run.foregroundHex { parts.append("eo:color=\"\(XMLSanitizer.escape(fg))\"") }
    if let bg = run.backgroundHex { parts.append("eo:bgColor=\"\(XMLSanitizer.escape(bg))\"") }
    if let link = run.linkURL { parts.append("eo:link=\"\(XMLSanitizer.escape(link))\"") }
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
    let odtMixedRuns = parseODTMixedSpans(in: innerXML)
    if !odtMixedRuns.isEmpty {
      return odtMixedRuns
    }

    if childTextTag == "hp:t" {
      let hpRuns = XMLSanitizer.extractElements(named: "hp:run", in: innerXML)
      if !hpRuns.isEmpty {
        return hpRuns.map { elem in
          let attrs = elem.attributes
          let tText = XMLSanitizer.extractElements(named: "hp:t", in: elem.innerXML).map {
            XMLSanitizer.unescape($0.innerXML)
          }.joined()
          return DocumentTextRun(
            id: attr("id", in: attrs).flatMap(UUID.init(uuidString:)) ?? UUID(),
            text: tText,
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
        }
      }
    }

    if childTextTag == "w:t" {
      let wRuns = XMLSanitizer.extractElements(named: "w:r", in: innerXML)
      if !wRuns.isEmpty {
        return wRuns.compactMap { elem in
          let tElems = XMLSanitizer.extractElements(named: "w:t", in: elem.innerXML)
          if tElems.isEmpty && elem.innerXML.contains("<w:drawing") {
            return nil
          }
          let tText = tElems.map { XMLSanitizer.unescape($0.innerXML) }.joined()
          let attrs = elem.attributes
          var isBold = attr("bold", in: attrs) == "1"
          var isItalic = attr("italic", in: attrs) == "1"
          var isUnderlined = attr("underline", in: attrs) == "1"
          var isStrikethrough = attr("strike", in: attrs) == "1"
          var fontName = attr("font", in: attrs)
          var fontSizePt = attr("size", in: attrs).flatMap(Double.init)
          var foregroundHex = attr("color", in: attrs)
          var backgroundHex = attr("bgColor", in: attrs) ?? attr("highlightHex", in: attrs)
          let linkURL = attr("link", in: attrs)

          if let rPr = XMLSanitizer.extractElements(named: "w:rPr", in: elem.innerXML).first?
            .innerXML
          {
            if fontName == nil,
              let fontAttrs = XMLSanitizer.extractElements(named: "w:rFonts", in: rPr).first?
                .attributes,
              let ascii = fontAttrs["w:ascii"] ?? fontAttrs["w:eastAsia"] ?? fontAttrs["ascii"]
            {
              fontName = ascii
            }
            if attr("bold", in: attrs) == nil,
              let bElem = XMLSanitizer.extractElements(named: "w:b", in: rPr).first
            {
              let val = (bElem.attributes["w:val"] ?? bElem.attributes["val"] ?? "1").lowercased()
              isBold = val != "0" && val != "false"
            }
            if attr("italic", in: attrs) == nil,
              let iElem = XMLSanitizer.extractElements(named: "w:i", in: rPr).first
            {
              let val = (iElem.attributes["w:val"] ?? iElem.attributes["val"] ?? "1").lowercased()
              isItalic = val != "0" && val != "false"
            }
            if attr("underline", in: attrs) == nil,
              let uElem = XMLSanitizer.extractElements(named: "w:u", in: rPr).first
            {
              let val = (uElem.attributes["w:val"] ?? uElem.attributes["val"] ?? "single")
                .lowercased()
              isUnderlined = val != "none" && val != "0" && val != "false"
            }
            if attr("strike", in: attrs) == nil,
              let sElem = XMLSanitizer.extractElements(named: "w:strike", in: rPr).first
            {
              let val = (sElem.attributes["w:val"] ?? sElem.attributes["val"] ?? "1").lowercased()
              isStrikethrough = val != "0" && val != "false"
            }
            if fontSizePt == nil,
              let szAttrs = XMLSanitizer.extractElements(named: "w:sz", in: rPr).first?.attributes,
              let valStr = szAttrs["w:val"] ?? szAttrs["val"], let halfPts = Double(valStr)
            {
              fontSizePt = halfPts / 2.0
            }
            if foregroundHex == nil,
              let colorAttrs = XMLSanitizer.extractElements(named: "w:color", in: rPr).first?
                .attributes,
              let rawHex = colorAttrs["w:val"] ?? colorAttrs["val"],
              !rawHex.isEmpty, rawHex.lowercased() != "auto"
            {
              foregroundHex = rawHex.hasPrefix("#") ? rawHex : "#\(rawHex)"
            }
            if backgroundHex == nil,
              let hiAttrs = XMLSanitizer.extractElements(named: "w:highlight", in: rPr).first?
                .attributes
                ?? XMLSanitizer.extractElements(named: "w:shd", in: rPr).first?.attributes,
              let rawBg = hiAttrs["eo:highlightHex"] ?? hiAttrs["w:val"] ?? hiAttrs["w:fill"]
                ?? hiAttrs["val"],
              !rawBg.isEmpty, rawBg.lowercased() != "none", rawBg.lowercased() != "auto"
            {
              backgroundHex = rawBg.hasPrefix("#") ? rawBg : "#\(rawBg)"
            }
          }

          return DocumentTextRun(
            id: attr("id", in: attrs).flatMap(UUID.init(uuidString:)) ?? UUID(),
            text: tText, isBold: isBold, isItalic: isItalic, isUnderlined: isUnderlined,
            isStrikethrough: isStrikethrough, fontName: fontName, fontSizePt: fontSizePt,
            foregroundHex: foregroundHex, backgroundHex: backgroundHex, linkURL: linkURL
          )
        }
      }
    }
    return []
  }

  static func parseParagraph(attributes: [String: String], innerXML: String, childTextTag: String?)
    -> ParagraphBlock
  {
    let id = attr("id", in: attributes).flatMap(UUID.init(uuidString:)) ?? UUID()
    let parsedRuns = parseRuns(from: innerXML, childTextTag: childTextTag)
    let hasExplicitRuns =
      attr("hasRuns", in: attributes) == "1" || parsedRuns.count > 1
      || (parsedRuns.count == 1
        && (parsedRuns[0].foregroundHex != nil || parsedRuns[0].backgroundHex != nil
          || parsedRuns[0].linkURL != nil || parsedRuns[0].isStrikethrough))

    let extractedText: String
    if !parsedRuns.isEmpty {
      extractedText = parsedRuns.map(\.text).joined()
    } else if let childTag = childTextTag {
      let runs = XMLSanitizer.extractElements(named: childTag, in: innerXML)
      extractedText =
        !runs.isEmpty
        ? runs.map { XMLSanitizer.unescape($0.innerXML) }.joined() : stripAllXMLTags(innerXML)
    } else {
      extractedText = extractODFText(from: innerXML)
    }

    var firstLineIndent = attr("firstLineIndent", in: attributes).flatMap(Double.init)
    var leftIndent = attr("leftIndent", in: attributes).flatMap(Double.init)
    var parsedLetterSpacing =
      (attr("letterSpacing", in: attributes) ?? attributes["letterSpacingPercent"])
      .flatMap(Double.init)
    var parsedCharScale =
      (attr("charScale", in: attributes) ?? attributes["charWidthScalePercent"])
      .flatMap(Double.init)
    var parsedListKind = ParagraphListKind(rawValue: attr("listKind", in: attributes) ?? "") ?? .none
    var parsedListLevel = max(0, min(3, Int(attr("listLevel", in: attributes) ?? "0") ?? 0))
    var pPrStyle: ParagraphStyleRole?
    var pPrAlignment: TextAlignmentMode?

    if let pPr = XMLSanitizer.extractElements(named: "w:pPr", in: innerXML).first?.innerXML {
      if let pStyleAttrs = XMLSanitizer.extractElements(named: "w:pStyle", in: pPr).first?
        .attributes,
        let val = pStyleAttrs["w:val"] ?? pStyleAttrs["val"]
      {
        pPrStyle = roleFromOOXMLStyle(val)
      }
      if let jcAttrs = XMLSanitizer.extractElements(named: "w:jc", in: pPr).first?.attributes,
        let val = jcAttrs["w:val"] ?? jcAttrs["val"]
      {
        pPrAlignment = alignmentFromOOXMLJustification(val)
      }
      if let indAttrs = XMLSanitizer.extractElements(named: "w:ind", in: pPr).first?.attributes {
        if firstLineIndent == nil, let flStr = indAttrs["w:firstLine"] ?? indAttrs["firstLine"],
          let flTwips = Double(flStr)
        {
          firstLineIndent = flTwips / 20.0
        }
        if leftIndent == nil, let leftStr = indAttrs["w:left"] ?? indAttrs["left"],
          let leftTwips = Double(leftStr)
        {
          leftIndent = leftTwips / 20.0
        }
      }
      if let spacingAttrs = XMLSanitizer.extractElements(named: "w:spacing", in: pPr).first?
        .attributes
      {
        if parsedLetterSpacing == nil,
          let lsStr = attr("letterSpacing", in: spacingAttrs),
          let lsVal = Double(lsStr)
        {
          parsedLetterSpacing = lsVal
        }
        if parsedCharScale == nil,
          let csStr = attr("charScale", in: spacingAttrs),
          let csVal = Double(csStr)
        {
          parsedCharScale = csVal
        }
      }
      if parsedListKind == .none,
        let numPr = XMLSanitizer.extractElements(named: "w:numPr", in: pPr).first?.innerXML
      {
        if let ilvlAttrs = XMLSanitizer.extractElements(named: "w:ilvl", in: numPr).first?
          .attributes,
          let ilvlVal = ilvlAttrs["w:val"] ?? ilvlAttrs["val"],
          let level = Int(ilvlVal)
        {
          parsedListLevel = max(0, min(3, level))
        }
        if let numIdAttrs = XMLSanitizer.extractElements(named: "w:numId", in: numPr).first?
          .attributes,
          let numIdVal = numIdAttrs["w:val"] ?? numIdAttrs["val"]
        {
          switch numIdVal {
          case "1": parsedListKind = .bullet
          case "2": parsedListKind = .numbered
          case "3": parsedListKind = .koreanOfficial
          default: parsedListKind = .bullet
          }
        }
      }
    }

    if let styleRaw = attr("style", in: attributes) {
      let alignRaw = attr("align", in: attributes) ?? ""
      return ParagraphBlock(
        id: id,
        style: ParagraphStyleRole(rawValue: styleRaw) ?? roleFromOOXMLStyle(styleRaw),
        alignment: TextAlignmentMode(rawValue: alignRaw)
          ?? alignmentFromOOXMLJustification(alignRaw),
        text: extractedText, runs: hasExplicitRuns ? parsedRuns : [],
        fontName: attr("font", in: attributes) ?? "Pretendard",
        fontSizePt: Double(attr("size", in: attributes) ?? "11.0") ?? 11.0,
        isBold: attr("bold", in: attributes) == "1",
        isItalic: attr("italic", in: attributes) == "1",
        isUnderlined: attr("underline", in: attributes) == "1",
        lineSpacingPercent: Double(attr("lineSpacing", in: attributes) ?? "160.0") ?? 160.0,
        firstLineIndentPt: firstLineIndent ?? 0.0, leftIndentPt: leftIndent ?? 0.0,
        listKind: parsedListKind, listLevel: parsedListLevel,
        letterSpacingPercent: parsedLetterSpacing ?? 0.0,
        charWidthScalePercent: parsedCharScale ?? 100.0
      )
    }

    let firstRun = parsedRuns.first
    return ParagraphBlock(
      id: id, style: pPrStyle ?? .body, alignment: pPrAlignment ?? .justified,
      text: extractedText, runs: hasExplicitRuns ? parsedRuns : [],
      fontName: firstRun?.fontName ?? "Pretendard", fontSizePt: firstRun?.fontSizePt ?? 11.0,
      isBold: parsedRuns.contains(where: \.isBold),
      isItalic: parsedRuns.contains(where: \.isItalic),
      isUnderlined: parsedRuns.contains(where: \.isUnderlined), lineSpacingPercent: 160.0,
      firstLineIndentPt: firstLineIndent ?? 0.0, leftIndentPt: leftIndent ?? 0.0,
      listKind: parsedListKind, listLevel: parsedListLevel,
      letterSpacingPercent: parsedLetterSpacing ?? 0.0,
      charWidthScalePercent: parsedCharScale ?? 100.0
    )
  }
}

// MARK: - OOXML Container Codec (ECMA-376 DOCX, XLSX, PPTX)

/// ECMA-376 / ISO/IEC 29500 OOXML container encoder and decoder for `.docx`, `.xlsx`, and `.pptx`.
public enum OOXMLContainerCodec {

  // MARK: DOCX (WordprocessingML)

  public static func encodeDocument(_ doc: OfficeDocumentModel) throws -> Data {
    let resolvedMedia = XMLDocumentModelMapper.resolveUniqueMediaEntries(
      images: doc.images,
      mediaFolderPrefix: "word/media/"
    )
    let mediaDefaultsXML = XMLDocumentModelMapper.mediaDefaultContentTypesXML(
      resolvedMedia: resolvedMedia,
      preservedPaths: Array(doc.preservedArchiveEntries.keys)
    )

    let contentTypesXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
        <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
        <Default Extension="xml" ContentType="application/xml"/>
      \(mediaDefaultsXML)
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

    let extendedMetaXML = XMLDocumentModelMapper.serializeExtendedMetadata(
      doc, resolvedMedia: resolvedMedia)
    let coreXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:mc="\(XMLDocumentModelMapper.mcNamespaceURI)" xmlns:eo="\(XMLDocumentModelMapper.eoNamespaceURI)" mc:Ignorable="eo">
        <dc:title>\(XMLSanitizer.escape(doc.title))</dc:title>
        <eo:headerText>\(XMLSanitizer.escape(doc.headerText))</eo:headerText>
        <eo:footerText>\(XMLSanitizer.escape(doc.footerText))</eo:footerText>
        <eo:paperSize>\(XMLSanitizer.escape(doc.paperSize.rawValue))</eo:paperSize>
        <eo:trackChanges>\(doc.trackChangesEnabled ? "true" : "false")</eo:trackChanges>
        \(extendedMetaXML)
      </cp:coreProperties>
      """

    var bodyItems: [String] = []
    for p in doc.paragraphs {
      let pAttrs = XMLDocumentModelMapper.paragraphAttributes(p)
      let styleVal = XMLDocumentModelMapper.ooxmlStyleVal(for: p.style)
      let jcVal = XMLDocumentModelMapper.ooxmlJustificationVal(for: p.alignment)
      let indTag: String
      if p.leftIndentPt != 0 || p.firstLineIndentPt != 0 {
        let leftTwips = XMLDocumentModelMapper.safeClampedInt(p.leftIndentPt * 20.0)
        let firstLineTwips = XMLDocumentModelMapper.safeClampedInt(p.firstLineIndentPt * 20.0)
        indTag = "<w:ind w:left=\"\(leftTwips)\" w:firstLine=\"\(firstLineTwips)\"/>"
      } else {
        indTag = ""
      }
      let numPrTag: String
      if p.listKind != .none {
        let numId = p.listKind == .bullet ? 1 : (p.listKind == .numbered ? 2 : 3)
        numPrTag =
          "<w:numPr><w:ilvl w:val=\"\(p.listLevel)\"/><w:numId w:val=\"\(numId)\"/></w:numPr>"
      } else {
        numPrTag = ""
      }

      let runsXML = p.effectiveRuns.map { run -> String in
        let rAttrs = XMLDocumentModelMapper.runAttributes(run)
        let resolvedFont = run.fontName ?? p.fontName
        let resolvedSize = run.fontSizePt ?? p.fontSizePt
        let halfPtSize = XMLDocumentModelMapper.safeClampedInt(
          resolvedSize * 2.0, min: 1, max: 3_276)
        var rPrTags = "<w:rFonts w:ascii=\"\(XMLSanitizer.escape(resolvedFont))\"/>"
        rPrTags += "<w:sz w:val=\"\(halfPtSize)\"/>"
        if run.isBold { rPrTags += "<w:b/>" }
        if run.isItalic { rPrTags += "<w:i/>" }
        if run.isUnderlined { rPrTags += "<w:u w:val=\"single\"/>" }
        if run.isStrikethrough { rPrTags += "<w:strike/>" }
        if let fg = run.foregroundHex {
          let cleanHex = fg.hasPrefix("#") ? String(fg.dropFirst()) : fg
          rPrTags += "<w:color w:val=\"\(XMLSanitizer.escape(cleanHex))\"/>"
        }
        if let bg = run.backgroundHex {
          let highlightName = XMLDocumentModelMapper.ooxmlHighlightColorName(for: bg)
          rPrTags +=
            "<w:highlight w:val=\"\(highlightName)\" eo:highlightHex=\"\(XMLSanitizer.escape(bg))\"/>"
        }
        return
          "      <w:r \(rAttrs)><w:rPr>\(rPrTags)</w:rPr><w:t xml:space=\"preserve\">\(XMLSanitizer.escape(run.text))</w:t></w:r>"
      }.joined(separator: "\n")

      bodyItems.append(
        """
            <w:p \(pAttrs)>
              <w:pPr><w:pStyle w:val="\(styleVal)"/><w:jc w:val="\(jcVal)"/>\(indTag)\(numPrTag)</w:pPr>
        \(runsXML)
            </w:p>
        """)
    }

    for tbl in doc.tables {
      let tAttrs = XMLDocumentModelMapper.tableAttributes(tbl)
      let spansXML = XMLDocumentModelMapper.serializeTableSpans(tbl)
      let rowStrings = tbl.rows.map { row -> String in
        let cells = row.map {
          "<w:tc><w:p><w:r><w:t>\(XMLSanitizer.escape($0))</w:t></w:r></w:p></w:tc>"
        }.joined()
        return "<w:tr>\(cells)</w:tr>"
      }.joined(separator: "\n      ")

      bodyItems.append(
        """
            <w:tbl \(tAttrs)>
              \(spansXML)
              \(rowStrings)
            </w:tbl>
        """)
    }

    for (idx, media) in resolvedMedia.enumerated() {
      let cx = XMLDocumentModelMapper.safeClampedInt(media.attachment.widthPt * 12_700.0, min: 1)
      let cy = XMLDocumentModelMapper.safeClampedInt(media.attachment.heightPt * 12_700.0, min: 1)
      let rId = "rIdMedia\(idx + 1)"
      let safeName = XMLSanitizer.escape(media.deduplicatedFileName)
      bodyItems.append(
        """
            <w:p eo:synthesizedMedia="1">
              <w:r>
                <w:drawing>
                  <wp:inline>
                    <wp:extent cx="\(cx)" cy="\(cy)"/>
                    <wp:docPr id="\(idx + 1)" name="\(safeName)"/>
                    <a:graphic>
                      <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">
                        <pic:pic>
                          <pic:blipFill>
                            <a:blip r:embed="\(rId)"/>
                          </pic:blipFill>
                        </pic:pic>
                      </a:graphicData>
                    </a:graphic>
                  </wp:inline>
                </w:drawing>
              </w:r>
            </w:p>
        """)
    }

    let documentXML = """
      <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
      <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:mc="\(XMLDocumentModelMapper.mcNamespaceURI)" xmlns:eo="\(XMLDocumentModelMapper.eoNamespaceURI)" mc:Ignorable="eo">
        <w:body>
      \(bodyItems.joined(separator: "\n"))
        </w:body>
      </w:document>
      """

    var baseEntries: [ZipContainerEntry] = [
      ZipContainerEntry(
        path: "[Content_Types].xml", data: Data(contentTypesXML.utf8), compressionMethod: .deflate),
      ZipContainerEntry(path: "_rels/.rels", data: Data(relsXML.utf8), compressionMethod: .deflate),
      ZipContainerEntry(
        path: "docProps/core.xml", data: Data(coreXML.utf8), compressionMethod: .deflate),
      ZipContainerEntry(
        path: "word/document.xml", data: Data(documentXML.utf8), compressionMethod: .deflate),
    ]
    if !resolvedMedia.isEmpty {
      let mediaRelItems = resolvedMedia.enumerated().map { idx, media in
        "  <Relationship Id=\"rIdMedia\(idx + 1)\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/\(XMLSanitizer.escape(media.deduplicatedFileName))\"/>"
      }.joined(separator: "\n")
      let docRelsXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        \(mediaRelItems)
        </Relationships>
        """
      baseEntries.append(
        ZipContainerEntry(
          path: "word/_rels/document.xml.rels",
          data: Data(docRelsXML.utf8),
          compressionMethod: .deflate
        )
      )
    }

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
        "[Content_Types].xml", "_rels/.rels", "docProps/core.xml", "word/document.xml",
        "word/_rels/document.xml.rels",
      ]
    )
    let entryMap = unpacked.xmlEntries
    let binaryEntries = unpacked.binaryEntries
    let preservedEntries = unpacked.preservedEntries
    guard let documentXML = entryMap["word/document.xml"] else {
      throw EasyOfficeError.corruptedContainer(
        detail: "OOXML DOCX 필수 스트림 'word/document.xml'이 누락되었습니다.")
    }

    let coreXML = entryMap["docProps/core.xml"] ?? ""
    let title =
      XMLSanitizer.extractElements(named: "dc:title", in: coreXML).first.map {
        XMLSanitizer.unescape($0.innerXML)
      } ?? "제목 없음"
    let headerText =
      (XMLSanitizer.extractElements(named: "eo:headerText", in: coreXML).first
      ?? XMLSanitizer.extractElements(named: "cp:headerText", in: coreXML).first).map {
        XMLSanitizer.unescape($0.innerXML)
      } ?? ""
    let footerText =
      (XMLSanitizer.extractElements(named: "eo:footerText", in: coreXML).first
      ?? XMLSanitizer.extractElements(named: "cp:footerText", in: coreXML).first).map {
        XMLSanitizer.unescape($0.innerXML)
      } ?? ""
    let paperRaw =
      (XMLSanitizer.extractElements(named: "eo:paperSize", in: coreXML).first
      ?? XMLSanitizer.extractElements(named: "cp:paperSize", in: coreXML).first).map {
        XMLSanitizer.unescape($0.innerXML)
      } ?? ""
    let paperSize = PaperSizePreset(rawValue: paperRaw) ?? .a4
    let trackChanges =
      (XMLSanitizer.extractElements(named: "eo:trackChanges", in: coreXML).first?.innerXML
      ?? XMLSanitizer.extractElements(named: "cp:trackChanges", in: coreXML).first?.innerXML)
      == "true"

    let tables = XMLSanitizer.extractElements(named: "w:tbl", in: documentXML).map { tblElem in
      XMLDocumentModelMapper.parseTable(
        attributes: tblElem.attributes, innerXML: tblElem.innerXML,
        rowTag: "w:tr", cellTag: "w:tc", cellTextTag: "w:t"
      )
    }

    let bodyWithoutTables = XMLSanitizer.stripElements(named: "w:tbl", from: documentXML)
    let paragraphs = XMLSanitizer.extractElements(named: "w:p", in: bodyWithoutTables).compactMap {
      pElem -> ParagraphBlock? in
      if XMLDocumentModelMapper.attr("synthesizedMedia", in: pElem.attributes) == "1" {
        return nil
      }
      if pElem.innerXML.contains("<w:drawing")
        && XMLSanitizer.extractElements(named: "w:t", in: pElem.innerXML).isEmpty
      {
        return nil
      }
      return XMLDocumentModelMapper.parseParagraph(
        attributes: pElem.attributes, innerXML: pElem.innerXML, childTextTag: "w:t")
    }

    let meta = XMLDocumentModelMapper.parseExtendedMetadata(
      from: coreXML, defaultPaperSize: paperSize, binaryEntries: binaryEntries)
    var resolvedImages = meta.images
    if resolvedImages.isEmpty, let docRelsXML = entryMap["word/_rels/document.xml.rels"] {
      var relTargetById: [String: String] = [:]
      for rel in XMLSanitizer.extractElements(named: "Relationship", in: docRelsXML) {
        if let rId = rel.attributes["Id"], let rawTarget = rel.attributes["Target"] {
          let normalized =
            rawTarget.hasPrefix("/")
            ? String(rawTarget.dropFirst())
            : (rawTarget.hasPrefix("word/") ? rawTarget : "word/\(rawTarget)")
          relTargetById[rId] = normalized
        }
      }
      for blip in XMLSanitizer.extractElements(named: "a:blip", in: documentXML) {
        if let rId = blip.attributes["r:embed"] ?? blip.attributes["embed"],
          let mediaPath = relTargetById[rId],
          let imgData = binaryEntries[mediaPath]
        {
          let fileName = (mediaPath as NSString).lastPathComponent
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
      title: title, format: .docx, paperSize: paperSize,
      headerText: headerText, footerText: footerText,
      paragraphs: paragraphs, tables: tables, trackChangesEnabled: trackChanges,
      sections: meta.sections, footnotes: meta.footnotes, revisions: meta.revisions,
      comments: meta.comments, bookmarks: meta.bookmarks, images: resolvedImages,
      equations: meta.equations, watermarkText: meta.watermarkText
    )
    decodedDoc.preservedArchiveEntries = preservedEntries
    return decodedDoc
  }
}
