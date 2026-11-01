import Foundation

/// Comprehensive container security & privacy inspection report.
public struct SecurityAuditReport: Sendable, Codable, Equatable {
  public let fileName: String
  public let detectedFormat: DocumentFormat?
  public let isSignatureValid: Bool
  public let isSafeFromBombAndTraversal: Bool
  public let isSafeFromXXE: Bool
  public let isUnencrypted: Bool
  public let issues: [String]
  public let isPassed: Bool

  public init(
    fileName: String,
    detectedFormat: DocumentFormat?,
    isSignatureValid: Bool,
    isSafeFromBombAndTraversal: Bool,
    isSafeFromXXE: Bool,
    isUnencrypted: Bool,
    issues: [String],
    isPassed: Bool
  ) {
    self.fileName = fileName
    self.detectedFormat = detectedFormat
    self.isSignatureValid = isSignatureValid
    self.isSafeFromBombAndTraversal = isSafeFromBombAndTraversal
    self.isSafeFromXXE = isSafeFromXXE
    self.isUnencrypted = isUnencrypted
    self.issues = issues
    self.isPassed = isPassed
  }
}

/// Container security auditor and document metadata/PII scrubber.
public enum DocumentSecurityAuditor {

  private static let sensitivePatternRules: [(label: String, regex: NSRegularExpression, mask: String)] = {
    let rawRules: [(String, String, String)] = [
      (
        "주민등록번호",
        #"(?<![0-9])\d{2}(?:0[1-9]|1[0-2])(?:0[1-9]|[12]\d|3[01])[-–]\s*[1-8]\d{6}(?![0-9])|(?<![0-9])\d{2}(?:0[1-9]|1[0-2])(?:0[1-9]|[12]\d|3[01])[1-8]\d{6}(?![0-9])"#,
        "[주민등록번호 비식별]"
      ),
      ("운전면허번호", #"(?<![0-9])\d{2}-\d{2}-\d{6}-\d{2}(?![0-9])"#, "[운전면허 비식별]"),
      ("여권번호", #"(?<![A-Za-z0-9])(?:[MSRGD]\d{8}|[A-Z]{2}\d{7})(?![A-Za-z0-9])"#, "[여권번호 비식별]"),
      ("신용카드번호", #"(?<![0-9])(?:\d{4}[-\s]){3}\d{4}(?![0-9])"#, "[카드번호 비식별]"),
      (
        "전화번호",
        #"(?<![0-9])01[016789][-.\s]?\d{3,4}[-.\s]?\d{4}(?![0-9])|(?<![0-9])0\d{1,2}[-.\s]\d{3,4}[-.\s]\d{4}(?![0-9])"#,
        "[연락처 비식별]"
      ),
      (
        "이메일",
        #"(?<![A-Za-z0-9._%+-])[A-Za-z0-9_%+-]+(?:\.[A-Za-z0-9_%+-]+)*@(?:[\p{L}\p{N}](?:[\p{L}\p{N}-]*[\p{L}\p{N}])?\.)+(?:[A-Za-z]{2,}(?![A-Za-z0-9])|[^\x00-\x7F\P{L}]{2,}(?![^\x00-\x7F\P{L}\P{N}]))|(?<![\p{L}\p{N}._%+-])[\p{L}\p{N}_%+-]+(?:\.[\p{L}\p{N}_%+-]+)*@(?:[\p{L}\p{N}](?:[\p{L}\p{N}-]*[\p{L}\p{N}])?\.)+(?:[A-Za-z]{2,}(?![A-Za-z0-9])|[\p{L}]{2,}(?![\p{L}\p{N}]))"#,
        "[이메일 비식별]"
      ),
      ("사업자/법인등록번호", #"(?<![0-9])\d{3}-\d{2}-\d{5}(?![0-9])"#, "[사업자번호 비식별]"),
      (
        "IPv4주소",
        #"(?<![0-9])(?:(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.){3}(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(?![0-9])"#,
        "[IP 비식별]"
      ),
      (
        "API토큰",
        #"(?<![A-Za-z0-9_-])(?:sk-[A-Za-z0-9_-]{16,}|eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+)(?![A-Za-z0-9_-])"#,
        "[인증토큰 비식별]"
      ),
    ]
    return rawRules.compactMap { label, pattern, mask in
      guard let regex = try? NSRegularExpression(pattern: pattern, options: []) else { return nil }
      return (label, regex, mask)
    }
  }()

  public static func detectSensitivePatterns(in text: String) -> [String] {
    guard !text.isEmpty else { return [] }
    let nsRange = NSRange(text.startIndex..<text.endIndex, in: text)
    var findings: [String] = []
    for rule in sensitivePatternRules {
      let matches = rule.regex.matches(in: text, options: [], range: nsRange)
      if !matches.isEmpty {
        findings.append("\(rule.label) (\(matches.count)건)")
      }
    }
    return findings
  }

  public static func maskSensitivePatterns(in text: String) -> (masked: String, matchCount: Int) {
    guard !text.isEmpty else { return (text, 0) }
    var current = text
    var totalMatches = 0
    for rule in sensitivePatternRules {
      let nsRange = NSRange(current.startIndex..<current.endIndex, in: current)
      let count = rule.regex.numberOfMatches(in: current, options: [], range: nsRange)
      if count > 0 {
        totalMatches += count
        current = rule.regex.stringByReplacingMatches(
          in: current, options: [], range: nsRange, withTemplate: rule.mask)
      }
    }
    return (current, totalMatches)
  }

  private static func scrubStringField(_ field: inout String, countInto removedCount: inout Int) {
    let (masked, count) = maskSensitivePatterns(in: field)
    if count > 0 {
      field = masked
      removedCount += count
    }
  }

  private static func scrubOptionalStringField(
    _ field: inout String?, countInto removedCount: inout Int
  ) {
    guard let value = field else { return }
    let (masked, count) = maskSensitivePatterns(in: value)
    if count > 0 {
      field = masked
      removedCount += count
    }
  }

  private static func scrubFileNamePreservingExtension(
    _ fileName: inout String, countInto removedCount: inout Int
  ) {
    if let dotIdx = fileName.lastIndex(of: "."), dotIdx > fileName.startIndex {
      let stem = String(fileName[..<dotIdx])
      let ext = String(fileName[dotIdx...])
      let (maskedStem, count) = maskSensitivePatterns(in: stem)
      if count > 0 {
        fileName = maskedStem + ext
        removedCount += count
      }
    } else {
      scrubStringField(&fileName, countInto: &removedCount)
    }
  }

  private static func scrubFormulaStringLiterals(_ formula: String) -> (masked: String, matchCount: Int) {
    guard formula.hasPrefix("="), formula.contains("\"") else { return (formula, 0) }
    var out = ""
    out.reserveCapacity(formula.utf8.count)
    var totalCount = 0
    var idx = formula.startIndex
    while idx < formula.endIndex {
      let ch = formula[idx]
      if ch != "\"" {
        out.append(ch)
        idx = formula.index(after: idx)
        continue
      }
      out.append("\"")
      idx = formula.index(after: idx)
      var literal = ""
      while idx < formula.endIndex {
        if formula[idx] == "\"" {
          let nextIdx = formula.index(after: idx)
          if nextIdx < formula.endIndex, formula[nextIdx] == "\"" {
            literal.append("\"")
            idx = formula.index(after: nextIdx)
            continue
          }
          break
        }
        literal.append(formula[idx])
        idx = formula.index(after: idx)
      }
      let (maskedLit, count) = maskSensitivePatterns(in: literal)
      totalCount += count
      out.append(maskedLit.replacingOccurrences(of: "\"", with: "\"\""))
      if idx < formula.endIndex, formula[idx] == "\"" {
        out.append("\"")
        idx = formula.index(after: idx)
      }
    }
    return (out, totalCount)
  }

  /// Strips EXIF, GPS, XMP, IPTC, comment, and ancillary text metadata segments/chunks from JPEG, PNG, and classic TIFF binaries while preserving image bitstream validity.
  public static func stripBinaryImageMetadata(_ data: Data) -> (cleaned: Data, strippedCount: Int) {
    let bytes = [UInt8](data)
    if bytes.count >= 4, bytes[0] == 0xFF, bytes[1] == 0xD8 {
      return stripJpegMetadata(bytes, fallback: data)
    }
    let pngSig: [UInt8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
    if bytes.count >= 8, Array(bytes[0..<8]) == pngSig {
      return stripPngMetadata(bytes, fallback: data)
    }
    if bytes.count >= 8,
      (bytes[0] == 0x49 && bytes[1] == 0x49 && bytes[2] == 0x2A && bytes[3] == 0x00)
        || (bytes[0] == 0x4D && bytes[1] == 0x4D && bytes[2] == 0x00 && bytes[3] == 0x2A)
    {
      return stripTiffMetadata(bytes, fallback: data)
    }
    return (data, 0)
  }

  private static func stripJpegMetadata(
    _ bytes: [UInt8], fallback: Data
  ) -> (cleaned: Data, strippedCount: Int) {
    var out: [UInt8] = [0xFF, 0xD8]
    out.reserveCapacity(bytes.count)
    var stripped = 0
    var pos = 2
    while pos < bytes.count {
      guard bytes[pos] == 0xFF else { return (fallback, 0) }
      let markerStart = pos
      while pos < bytes.count, bytes[pos] == 0xFF {
        pos += 1
      }
      guard pos < bytes.count else { break }
      let marker = bytes[pos]
      pos += 1
      if marker == 0xD9 {
        out.append(contentsOf: [0xFF, 0xD9])
        break
      }
      if marker == 0x01 || (marker >= 0xD0 && marker <= 0xD8) {
        out.append(contentsOf: [0xFF, marker])
        continue
      }
      guard pos + 2 <= bytes.count else { return (fallback, 0) }
      let segLen = (Int(bytes[pos]) << 8) | Int(bytes[pos + 1])
      guard segLen >= 2, pos + segLen <= bytes.count else { return (fallback, 0) }
      let segEnd = pos + segLen
      let isSensitiveSegment = (marker >= 0xE1 && marker <= 0xEF) || marker == 0xFE
      if isSensitiveSegment {
        stripped += 1
        pos = segEnd
        continue
      }
      out.append(contentsOf: bytes[markerStart..<segEnd])
      pos = segEnd
      if marker == 0xDA {
        while pos < bytes.count {
          if bytes[pos] != 0xFF {
            out.append(bytes[pos])
            pos += 1
            continue
          }
          if pos + 1 >= bytes.count {
            out.append(bytes[pos])
            pos += 1
            break
          }
          let next = bytes[pos + 1]
          if next == 0x00 || (next >= 0xD0 && next <= 0xD7) {
            out.append(bytes[pos])
            out.append(next)
            pos += 2
            continue
          }
          break
        }
      }
    }
    return stripped > 0 ? (Data(out), stripped) : (fallback, 0)
  }

  private static func stripPngMetadata(
    _ bytes: [UInt8], fallback: Data
  ) -> (cleaned: Data, strippedCount: Int) {
    let strippedTypes: Set<String> = ["tEXt", "zTXt", "iTXt", "eXIf", "tIME"]
    var out = Array(bytes[0..<8])
    out.reserveCapacity(bytes.count)
    var stripped = 0
    var pos = 8
    while pos < bytes.count {
      guard pos + 12 <= bytes.count else { return (fallback, 0) }
      let len =
        (UInt64(bytes[pos]) << 24)
        | (UInt64(bytes[pos + 1]) << 16)
        | (UInt64(bytes[pos + 2]) << 8)
        | UInt64(bytes[pos + 3])
      guard len <= UInt64(bytes.count - pos - 12) else { return (fallback, 0) }
      let chunkEnd = pos + 12 + Int(len)
      let chunkType = String(decoding: bytes[(pos + 4)..<(pos + 8)], as: UTF8.self)
      if strippedTypes.contains(chunkType) {
        stripped += 1
      } else {
        out.append(contentsOf: bytes[pos..<chunkEnd])
      }
      pos = chunkEnd
      if chunkType == "IEND" { break }
    }
    return stripped > 0 ? (Data(out), stripped) : (fallback, 0)
  }

  private static func stripTiffMetadata(
    _ bytes: [UInt8], fallback: Data
  ) -> (cleaned: Data, strippedCount: Int) {
    let littleEndian = bytes[0] == 0x49
    func readU16(_ buf: [UInt8], _ at: Int) -> UInt16 {
      littleEndian
        ? (UInt16(buf[at]) | (UInt16(buf[at + 1]) << 8))
        : ((UInt16(buf[at]) << 8) | UInt16(buf[at + 1]))
    }
    func writeU16(_ buf: inout [UInt8], _ at: Int, _ val: UInt16) {
      if littleEndian {
        buf[at] = UInt8(val & 0xFF)
        buf[at + 1] = UInt8(val >> 8)
      } else {
        buf[at] = UInt8(val >> 8)
        buf[at + 1] = UInt8(val & 0xFF)
      }
    }
    func readU32(_ buf: [UInt8], _ at: Int) -> Int {
      let raw: UInt32 =
        littleEndian
        ? (UInt32(buf[at]) | (UInt32(buf[at + 1]) << 8) | (UInt32(buf[at + 2]) << 16)
          | (UInt32(buf[at + 3]) << 24))
        : ((UInt32(buf[at]) << 24) | (UInt32(buf[at + 1]) << 16) | (UInt32(buf[at + 2]) << 8)
          | UInt32(buf[at + 3]))
      return Int(raw)
    }
    let typeByteSize: [UInt16: Int] = [
      1: 1, 2: 1, 3: 2, 4: 4, 5: 8, 6: 1, 7: 1, 8: 2, 9: 4, 10: 8, 11: 4, 12: 8,
    ]
    let sensitiveTags: Set<UInt16> = [
      0x010E, 0x010F, 0x0110, 0x0131, 0x013B, 0x02BC, 0x8298, 0x83BB, 0x8769, 0x8825,
    ]
    var buf = bytes
    var stripped = 0

    func wipeSubIFD(at subOffset: Int) {
      guard subOffset >= 8, subOffset + 2 <= buf.count else { return }
      let count = Int(readU16(buf, subOffset))
      guard count >= 0, count <= 512, subOffset + 2 + count * 12 + 4 <= buf.count else { return }
      for i in 0..<count {
        let entryPos = subOffset + 2 + i * 12
        let fieldType = readU16(buf, entryPos + 2)
        let valCount = readU32(buf, entryPos + 4)
        let unit = typeByteSize[fieldType] ?? 1
        let (byteCount, ov) = valCount.multipliedReportingOverflow(by: unit)
        if !ov, byteCount > 4 {
          let valOffset = readU32(buf, entryPos + 8)
          if valOffset >= 8, valOffset + byteCount <= buf.count {
            for b in valOffset..<(valOffset + byteCount) { buf[b] = 0 }
          }
        }
      }
      for b in subOffset..<(subOffset + 2 + count * 12 + 4) { buf[b] = 0 }
    }

    var ifdOffset = readU32(buf, 4)
    var hops = 0
    while ifdOffset >= 8, ifdOffset + 2 <= buf.count, hops < 16 {
      hops += 1
      let entryCount = Int(readU16(buf, ifdOffset))
      guard entryCount >= 0, entryCount <= 1024,
        ifdOffset + 2 + entryCount * 12 + 4 <= buf.count
      else { break }
      let nextIFD = readU32(buf, ifdOffset + 2 + entryCount * 12)
      let nextIFDBytes = Array(buf[(ifdOffset + 2 + entryCount * 12)..<(ifdOffset + 2 + entryCount * 12 + 4)])
      var keptEntries: [[UInt8]] = []
      for i in 0..<entryCount {
        let entryPos = ifdOffset + 2 + i * 12
        let tag = readU16(buf, entryPos)
        if sensitiveTags.contains(tag) {
          stripped += 1
          if tag == 0x8769 || tag == 0x8825 {
            wipeSubIFD(at: readU32(buf, entryPos + 8))
          } else {
            let fieldType = readU16(buf, entryPos + 2)
            let valCount = readU32(buf, entryPos + 4)
            let unit = typeByteSize[fieldType] ?? 1
            let (byteCount, ov) = valCount.multipliedReportingOverflow(by: unit)
            if !ov, byteCount > 4 {
              let valOffset = readU32(buf, entryPos + 8)
              if valOffset >= 8, valOffset + byteCount <= buf.count {
                for b in valOffset..<(valOffset + byteCount) { buf[b] = 0 }
              }
            }
          }
        } else {
          keptEntries.append(Array(buf[entryPos..<(entryPos + 12)]))
        }
      }
      if keptEntries.count != entryCount {
        writeU16(&buf, ifdOffset, UInt16(keptEntries.count))
        var writePos = ifdOffset + 2
        for entry in keptEntries {
          buf.replaceSubrange(writePos..<(writePos + 12), with: entry)
          writePos += 12
        }
        buf.replaceSubrange(writePos..<(writePos + 4), with: nextIFDBytes)
        writePos += 4
        let oldEnd = ifdOffset + 2 + entryCount * 12 + 4
        for b in writePos..<oldEnd { buf[b] = 0 }
      }
      if nextIFD == 0 { break }
      ifdOffset = nextIFD
    }
    return stripped > 0 ? (Data(buf), stripped) : (fallback, 0)
  }

  /// Scrubs header/footer PII markers, anonymizes revision authors to `"Redacted"`, removes all review comments, strips binary EXIF/ancillary image metadata, and masks sensitive PII patterns across all document fields.
  public static func scrubMetadata(
    from document: OfficeDocumentModel
  ) -> (scrubbed: OfficeDocumentModel, removedItemCount: Int) {
    var copy = document
    var removedCount = 0

    scrubStringField(&copy.title, countInto: &removedCount)
    scrubOptionalStringField(&copy.watermarkText, countInto: &removedCount)

    if !copy.headerText.isEmpty {
      copy.headerText = ""
      removedCount += 1
    }
    if !copy.footerText.isEmpty {
      copy.footerText = ""
      removedCount += 1
    }

    for idx in copy.revisions.indices {
      if copy.revisions[idx].author != "Redacted" {
        copy.revisions[idx].author = "Redacted"
        removedCount += 1
      }
      scrubStringField(&copy.revisions[idx].originalText, countInto: &removedCount)
      scrubStringField(&copy.revisions[idx].proposedText, countInto: &removedCount)
    }

    if !copy.comments.isEmpty {
      removedCount += copy.comments.count
      copy.comments.removeAll()
    }

    for pIdx in copy.paragraphs.indices {
      scrubStringField(&copy.paragraphs[pIdx].text, countInto: &removedCount)
      scrubStringField(&copy.paragraphs[pIdx].fontName, countInto: &removedCount)
      for rIdx in copy.paragraphs[pIdx].runs.indices {
        let (maskedRun, rCount) = maskSensitivePatterns(in: copy.paragraphs[pIdx].runs[rIdx].text)
        if rCount > 0 {
          copy.paragraphs[pIdx].runs[rIdx].text = maskedRun
        }
        scrubOptionalStringField(&copy.paragraphs[pIdx].runs[rIdx].linkURL, countInto: &removedCount)
        scrubOptionalStringField(&copy.paragraphs[pIdx].runs[rIdx].fontName, countInto: &removedCount)
      }
    }

    for tIdx in copy.tables.indices {
      scrubStringField(&copy.tables[tIdx].caption, countInto: &removedCount)
      for rIdx in copy.tables[tIdx].rows.indices {
        for cIdx in copy.tables[tIdx].rows[rIdx].indices {
          scrubStringField(&copy.tables[tIdx].rows[rIdx][cIdx], countInto: &removedCount)
        }
      }
    }

    for fIdx in copy.footnotes.indices {
      scrubStringField(&copy.footnotes[fIdx].text, countInto: &removedCount)
    }

    if !copy.bookmarks.isEmpty {
      var scrubbedBookmarks: [String: UUID] = [:]
      for key in copy.bookmarks.keys.sorted() {
        guard let targetID = copy.bookmarks[key] else { continue }
        let (maskedKey, count) = maskSensitivePatterns(in: key)
        removedCount += count
        var finalKey = maskedKey
        var suffix = 2
        while scrubbedBookmarks[finalKey] != nil {
          finalKey = "\(maskedKey)_\(suffix)"
          suffix += 1
        }
        scrubbedBookmarks[finalKey] = targetID
      }
      copy.bookmarks = scrubbedBookmarks
    }

    for iIdx in copy.images.indices {
      scrubStringField(&copy.images[iIdx].caption, countInto: &removedCount)
      scrubFileNamePreservingExtension(&copy.images[iIdx].fileName, countInto: &removedCount)
      let (cleanedImage, strippedMeta) = stripBinaryImageMetadata(copy.images[iIdx].imageData)
      if strippedMeta > 0 {
        copy.images[iIdx].imageData = cleanedImage
        removedCount += strippedMeta
      }
    }

    for eIdx in copy.equations.indices {
      scrubStringField(&copy.equations[eIdx].displayTitle, countInto: &removedCount)
      scrubStringField(&copy.equations[eIdx].rawExpression, countInto: &removedCount)
    }

    let (scrubbedPreserved, preservedRemoved) = scrubPreservedArchiveEntries(
      copy.preservedArchiveEntries)
    copy.preservedArchiveEntries = scrubbedPreserved
    removedCount += preservedRemoved

    return (copy, removedCount)
  }

  private static func scrubPreservedArchiveEntries(
    _ entries: [String: Data]
  ) -> (scrubbed: [String: Data], removedCount: Int) {
    guard !entries.isEmpty else { return (entries, 0) }
    var out: [String: Data] = [:]
    var removedCount = 0
    for path in entries.keys.sorted() {
      guard let data = entries[path] else { continue }
      var cleanPath = path
      scrubFileNamePreservingExtension(&cleanPath, countInto: &removedCount)
      if XMLDocumentModelMapper.isBinaryMediaPath(path) {
        let (cleanedMedia, strippedMeta) = stripBinaryImageMetadata(data)
        removedCount += strippedMeta
        out[cleanPath] = cleanedMedia
        continue
      }
      let lower = path.lowercased()
      if lower.contains("comment") || lower.contains("notesslide")
        || lower.contains("revision") || lower.contains("people.xml")
        || lower.hasSuffix("meta.xml") || lower.hasSuffix("core.xml")
        || lower.hasSuffix("custom.xml") || lower.hasSuffix("app.xml")
        || lower.hasSuffix("settings.xml") || lower.contains("header")
        || lower.contains("footer") || lower.contains("thumbnail")
        || lower.hasPrefix("preview/") || lower.contains("vbaproject")
      {
        removedCount += 1
        continue
      }
      if let xmlText = String(data: data, encoding: .utf8) {
        let (maskedText, count) = maskSensitivePatterns(in: xmlText)
        removedCount += count
        out[cleanPath] = Data(maskedText.utf8)
      } else {
        out[cleanPath] = data
      }
    }
    return (out, removedCount)
  }

  private static func scrubConditionalRules(
    _ rules: inout [ConditionalFormatRule], countInto removedCount: inout Int
  ) {
    for rIdx in rules.indices {
      if case .containsText(var sub) = rules[rIdx] {
        let (masked, count) = maskSensitivePatterns(in: sub)
        if count > 0 {
          sub = masked
          rules[rIdx] = .containsText(sub)
          removedCount += count
        }
      }
    }
  }

  private static func scrubValidationRule(
    _ rule: inout DataValidationRule?, countInto removedCount: inout Int
  ) {
    guard case .allowedList(var items) = rule else { return }
    var changed = false
    for idx in items.indices {
      let (masked, count) = maskSensitivePatterns(in: items[idx])
      if count > 0 {
        items[idx] = masked
        removedCount += count
        changed = true
      }
    }
    if changed {
      rule = .allowedList(items)
    }
  }

  private static func scrubOutlineNodes(
    _ nodes: inout [PDFOutlineNode], countInto removedCount: inout Int
  ) {
    for idx in nodes.indices {
      scrubStringField(&nodes[idx].title, countInto: &removedCount)
      if !nodes[idx].children.isEmpty {
        scrubOutlineNodes(&nodes[idx].children, countInto: &removedCount)
      }
    }
  }

  public static func scrubPayload(
    _ payload: OpenTabPayload
  ) -> (scrubbed: OpenTabPayload, removedItemCount: Int) {
    switch payload {
    case .document(let doc):
      let (scrubbed, count) = scrubMetadata(from: doc)
      return (.document(scrubbed), count)

    case .sheet(var sheet):
      var removedCount = sheet.sheetComments.count
      sheet.sheetComments.removeAll()
      scrubStringField(&sheet.name, countInto: &removedCount)
      scrubStringField(&sheet.pageSetup.marginPresetName, countInto: &removedCount)
      scrubConditionalRules(&sheet.conditionalFormatRules, countInto: &removedCount)
      scrubValidationRule(&sheet.dataValidationRule, countInto: &removedCount)
      if !sheet.namedRanges.isEmpty {
        var nextNamed: [String: String] = [:]
        for key in sheet.namedRanges.keys.sorted() {
          guard var val = sheet.namedRanges[key] else { continue }
          var cleanKey = key
          scrubStringField(&cleanKey, countInto: &removedCount)
          scrubStringField(&val, countInto: &removedCount)
          nextNamed[cleanKey] = val
        }
        sheet.namedRanges = nextNamed
      }
      for coord in Array(sheet.cellsByCoordinate.keys) {
        guard let raw = sheet.cellsByCoordinate[coord] else { continue }
        let (masked, count) =
          raw.hasPrefix("=") ? scrubFormulaStringLiterals(raw) : maskSensitivePatterns(in: raw)
        if count > 0 {
          sheet.cellsByCoordinate[coord] = masked
          sheet.compiledASTs[coord] = WorksheetModel.compileCellInput(masked)
          removedCount += count
        }
      }
      for sIdx in sheet.additionalSheets.indices {
        scrubStringField(&sheet.additionalSheets[sIdx].name, countInto: &removedCount)
        removedCount += sheet.additionalSheets[sIdx].sheetComments.count
        sheet.additionalSheets[sIdx].sheetComments.removeAll()
        scrubConditionalRules(
          &sheet.additionalSheets[sIdx].conditionalFormatRules, countInto: &removedCount)
        scrubValidationRule(
          &sheet.additionalSheets[sIdx].dataValidationRule, countInto: &removedCount)
        for coord in Array(sheet.additionalSheets[sIdx].cellsByCoordinate.keys) {
          guard let raw = sheet.additionalSheets[sIdx].cellsByCoordinate[coord] else { continue }
          let (masked, count) =
            raw.hasPrefix("=") ? scrubFormulaStringLiterals(raw) : maskSensitivePatterns(in: raw)
          if count > 0 {
            sheet.additionalSheets[sIdx].cellsByCoordinate[coord] = masked
            removedCount += count
          }
        }
      }
      let (scrubbedSheetPreserved, sheetPreservedRemoved) = scrubPreservedArchiveEntries(
        sheet.preservedArchiveEntries)
      sheet.preservedArchiveEntries = scrubbedSheetPreserved
      removedCount += sheetPreservedRemoved
      return (.sheet(sheet), removedCount)

    case .slide(var deck):
      var removedCount = 0
      scrubStringField(&deck.title, countInto: &removedCount)
      for sIdx in deck.slides.indices {
        if !deck.slides[sIdx].speakerNotes.isEmpty {
          deck.slides[sIdx].speakerNotes = ""
          removedCount += 1
        }
        scrubStringField(&deck.slides[sIdx].title, countInto: &removedCount)
        scrubStringField(&deck.slides[sIdx].subtitle, countInto: &removedCount)
        for bIdx in deck.slides[sIdx].bulletPoints.indices {
          scrubStringField(&deck.slides[sIdx].bulletPoints[bIdx], countInto: &removedCount)
        }
        for eIdx in deck.slides[sIdx].elements.indices {
          scrubStringField(&deck.slides[sIdx].elements[eIdx].title, countInto: &removedCount)
          scrubStringField(&deck.slides[sIdx].elements[eIdx].bodyText, countInto: &removedCount)
        }
      }
      let (scrubbedSlidePreserved, slidePreservedRemoved) = scrubPreservedArchiveEntries(
        deck.preservedArchiveEntries)
      deck.preservedArchiveEntries = scrubbedSlidePreserved
      removedCount += slidePreservedRemoved
      return (.slide(deck), removedCount)

    case .pdf(var pdf):
      var removedCount = 0
      scrubStringField(&pdf.title, countInto: &removedCount)
      scrubStringField(&pdf.watermarkText, countInto: &removedCount)
      for aIdx in pdf.annotations.indices {
        if pdf.annotations[aIdx].author != "Redacted" {
          pdf.annotations[aIdx].author = "Redacted"
          removedCount += 1
        }
        scrubStringField(&pdf.annotations[aIdx].content, countInto: &removedCount)
      }
      for pIdx in pdf.pages.indices {
        scrubStringField(&pdf.pages[pIdx].extractedText, countInto: &removedCount)
      }
      for fIdx in pdf.formFields.indices {
        scrubStringField(&pdf.formFields[fIdx].name, countInto: &removedCount)
        scrubStringField(&pdf.formFields[fIdx].value, countInto: &removedCount)
      }
      for iIdx in pdf.inkStrokes.indices {
        scrubStringField(&pdf.inkStrokes[iIdx].label, countInto: &removedCount)
      }
      for rIdx in pdf.redactedPhrases.indices {
        scrubStringField(&pdf.redactedPhrases[rIdx], countInto: &removedCount)
      }
      scrubOutlineNodes(&pdf.outline, countInto: &removedCount)
      return (.pdf(pdf), removedCount)
    }
  }

  /// Audits a raw document container against magic-byte spoofing, ZIP decompression bombs, path traversal, XML external entities (XXE), and unsupported encryption.
  public static func auditContainer(data: Data, fileName: String) -> SecurityAuditReport {
    let detectedFormat = DocumentFormat.from(fileNameOrExtension: fileName)
    var isSignatureValid = true
    var isSafeFromBombAndTraversal = true
    var isSafeFromXXE = true
    var isUnencrypted = true
    var issues: [String] = []

    let verified: VerifiedDocumentSignature
    do {
      verified = try FormatDetector.verify(data: data, fileName: fileName)
    } catch {
      isSignatureValid = false
      if let officeError = error as? EasyOfficeError, case .corruptedContainer = officeError {
        isSafeFromBombAndTraversal = false
      }
      if (try? XMLSanitizer.validateSafeXMLBytes(data)) == nil {
        isSafeFromXXE = false
      }
      let desc = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
      issues.append(desc)
      return SecurityAuditReport(
        fileName: fileName,
        detectedFormat: detectedFormat,
        isSignatureValid: false,
        isSafeFromBombAndTraversal: isSafeFromBombAndTraversal,
        isSafeFromXXE: isSafeFromXXE,
        isUnencrypted: isUnencrypted,
        issues: issues,
        isPassed: false
      )
    }

    switch verified.containerFamily {
    case .zipXML:
      do {
        let entries = try ZipContainerCodec.unpack(from: data)
        for entry in entries {
          let lowerPath = entry.path.lowercased()
          let isXMLPayload =
            lowerPath.hasSuffix(".xml")
            || lowerPath.hasSuffix(".rels")
            || lowerPath.hasSuffix(".hpf")
          do {
            try XMLSanitizer.validateSafeXMLBytes(entry.data)
          } catch {
            isSafeFromXXE = false
            let desc = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
            issues.append(desc)
          }
          if isXMLPayload, String(data: entry.data, encoding: .utf8) == nil {
            isSafeFromBombAndTraversal = false
            issues.append("ZIP 내부 XML 엔트리가 유효한 UTF-8이 아닙니다: \(entry.path)")
          }
        }
      } catch let error as EasyOfficeError {
        switch error {
        case .encryptedDocumentUnsupported:
          isUnencrypted = false
        case .xxeOrExternalEntityDetected:
          isSafeFromXXE = false
        default:
          isSafeFromBombAndTraversal = false
        }
        issues.append(error.errorDescription ?? error.localizedDescription)
      } catch {
        isSafeFromBombAndTraversal = false
        issues.append(error.localizedDescription)
      }

    case .ole2Compound:
      do {
        let streams = try OLE2ContainerCodec.unpack(from: data)
        for stream in streams {
          if stream.name == "EncryptionInfo" || stream.name == "EncryptedPackage" {
            isUnencrypted = false
            let err = EasyOfficeError.encryptedDocumentUnsupported(format: verified.format)
            issues.append(err.errorDescription ?? "암호화된 OLE2 컨테이너입니다.")
          }
        }
        if verified.format == .hwp || verified.format == .hwt {
          guard let headerStream = streams.first(where: { $0.name == "FileHeader" }),
            headerStream.data.count >= 40,
            headerStream.data.starts(with: Data(HWP5BinaryCodec.signatureString.utf8))
          else {
            isSafeFromBombAndTraversal = false
            issues.append("HWP 5.0 FileHeader 스트림이 누락되었거나 손상되었습니다.")
            break
          }
          let flags = try BitPacking.readUInt32LE(from: headerStream.data, at: 36)
          if (flags & (HWP5BinaryCodec.flagEncrypted | HWP5BinaryCodec.flagDistribution)) != 0 {
            isUnencrypted = false
            let err = EasyOfficeError.encryptedDocumentUnsupported(format: verified.format)
            issues.append(err.errorDescription ?? "암호화된 문서입니다.")
          }
        } else if verified.format == .doc {
          _ = try LegacyOLE2OfficeCodec.decodeWordBinary(from: data)
        } else if verified.format == .xls {
          _ = try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: data)
        } else if verified.format == .ppt {
          _ = try LegacyOLE2OfficeCodec.decodePowerPointBinary(from: data)
        }
      } catch let error as EasyOfficeError {
        if case .encryptedDocumentUnsupported = error {
          isUnencrypted = false
        } else {
          isSafeFromBombAndTraversal = false
        }
        issues.append(error.errorDescription ?? error.localizedDescription)
      } catch {
        isSafeFromBombAndTraversal = false
        let desc = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
        issues.append(desc)
      }

    case .pdfBinary:
      let rawString = String(decoding: data, as: UTF8.self)
      if !rawString.contains("obj") || !rawString.contains("%%EOF") {
        isSafeFromBombAndTraversal = false
        issues.append("PDF 컨테이너에 유효한 객체(obj) 또는 %%EOF 트레일러가 없습니다.")
      }
      if rawString.contains("/Encrypt") {
        isUnencrypted = false
        let err = EasyOfficeError.encryptedDocumentUnsupported(format: .pdf)
        issues.append(err.errorDescription ?? "암호화된 PDF 문서입니다.")
      }

    case .rtfText, .plainText:
      do {
        try XMLSanitizer.validateSafeXMLBytes(data)
      } catch {
        isSafeFromXXE = false
        let desc = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
        issues.append(desc)
      }
      if String(data: data, encoding: .utf8) == nil || data.contains(0x00) {
        isSafeFromBombAndTraversal = false
        issues.append("텍스트/RTF 컨테이너에 유효하지 않은 UTF-8 또는 NUL 바이트가 포함되어 있습니다.")
      }
    }

    let isPassed =
      isSignatureValid
      && isSafeFromBombAndTraversal
      && isSafeFromXXE
      && isUnencrypted
      && issues.isEmpty

    return SecurityAuditReport(
      fileName: fileName,
      detectedFormat: verified.format,
      isSignatureValid: isSignatureValid,
      isSafeFromBombAndTraversal: isSafeFromBombAndTraversal,
      isSafeFromXXE: isSafeFromXXE,
      isUnencrypted: isUnencrypted,
      issues: issues,
      isPassed: isPassed
    )
  }
}
