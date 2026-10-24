import Foundation

// MARK: - XML Sanitizer (Zero-Allocation XXE Guard & Quote-Aware Nested Element Extractor)

/// Fail-Closed XML sanitizer blocking XXE/external DTD payloads and providing deterministic quote-aware tag boundary parsing and escaping.
public enum XMLSanitizer {

  private static let entityAsciiUpper: (UInt8, UInt8, UInt8, UInt8, UInt8, UInt8) = (
    0x45, 0x4E, 0x54, 0x49, 0x54, 0x59  // "ENTITY"
  )
  private static let doctypeAsciiUpper: (UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8) = (
    0x44, 0x4F, 0x43, 0x54, 0x59, 0x50, 0x45  // "DOCTYPE"
  )

  static let swarLoMask: UInt64 = 0x0101_0101_0101_0101
  static let swarHiMask: UInt64 = 0x8080_8080_8080_8080

  @inline(__always)
  static func zeroByteMask(_ word: UInt64) -> UInt64 {
    (word &- swarLoMask) & ~word & swarHiMask
  }

  @inline(__always)
  static func hasZeroByte(_ word: UInt64) -> Bool {
    zeroByteMask(word) != 0
  }

  @inline(__always)
  static func broadcastByte(_ byte: UInt8) -> UInt64 {
    UInt64(byte) &* swarLoMask
  }

  private static func scanUnsafeXMLBuffer(_ buffer: UnsafeBufferPointer<UInt8>) throws {
    let count = buffer.count
    guard count >= 8, let base = buffer.baseAddress else { return }
    let ltMask = broadcastByte(UInt8(ascii: "<"))
    var idx = 0
    while idx <= count - 8 {
      let word = UnsafeRawPointer(base.advanced(by: idx)).loadUnaligned(as: UInt64.self)
      let matchBits = zeroByteMask(word ^ ltMask)
      if matchBits == 0 {
        idx += 8
        continue
      }
      idx += matchBits.trailingZeroBitCount >> 3
      if idx + 1 < count && buffer[idx] == UInt8(ascii: "<") && buffer[idx + 1] == UInt8(ascii: "!") {
        var kw = idx + 2
        while kw < count
          && (buffer[kw] == 0x20 || buffer[kw] == 0x09 || buffer[kw] == 0x0A || buffer[kw] == 0x0D)
        {
          kw += 1
        }
        if kw <= count - 6
          && (buffer[kw] & 0xDF) == entityAsciiUpper.0
          && (buffer[kw + 1] & 0xDF) == entityAsciiUpper.1
          && (buffer[kw + 2] & 0xDF) == entityAsciiUpper.2
          && (buffer[kw + 3] & 0xDF) == entityAsciiUpper.3
          && (buffer[kw + 4] & 0xDF) == entityAsciiUpper.4
          && (buffer[kw + 5] & 0xDF) == entityAsciiUpper.5
        {
          throw EasyOfficeError.xxeOrExternalEntityDetected
        }
        if kw <= count - 7
          && (buffer[kw] & 0xDF) == doctypeAsciiUpper.0
          && (buffer[kw + 1] & 0xDF) == doctypeAsciiUpper.1
          && (buffer[kw + 2] & 0xDF) == doctypeAsciiUpper.2
          && (buffer[kw + 3] & 0xDF) == doctypeAsciiUpper.3
          && (buffer[kw + 4] & 0xDF) == doctypeAsciiUpper.4
          && (buffer[kw + 5] & 0xDF) == doctypeAsciiUpper.5
          && (buffer[kw + 6] & 0xDF) == doctypeAsciiUpper.6
        {
          throw EasyOfficeError.xxeOrExternalEntityDetected
        }
      }
      idx += 1
    }
  }

  /// Scans UTF-8 bytes directly for `<!ENTITY` or `<!DOCTYPE` (case-insensitive) with zero heap allocations.
  public static func validateSafeXMLBytes(_ data: Data) throws {
    try data.withUnsafeBytes { rawBuffer in
      try scanUnsafeXMLBuffer(rawBuffer.bindMemory(to: UInt8.self))
    }
  }

  public static func validateSafeXML(_ xmlString: String) throws {
    var copy = xmlString
    try copy.withUTF8 { utf8Buffer in
      try scanUnsafeXMLBuffer(utf8Buffer)
    }
  }

  public static func validateSafeXMLData(_ data: Data) throws -> String {
    try validateSafeXMLBytes(data)
    guard let xmlString = String(data: data, encoding: .utf8) else {
      throw EasyOfficeError.corruptedContainer(detail: "XML 스트림이 유효한 UTF-8 인코딩이 아닙니다.")
    }
    return xmlString
  }

  public static func escape(_ input: String) -> String {
    guard !input.isEmpty else { return input }
    var copy = input
    return copy.withUTF8 { buf in
      let count = buf.count
      guard let base = buf.baseAddress else { return input }
      let ampMask = broadcastByte(UInt8(ascii: "&"))
      let ltMask = broadcastByte(UInt8(ascii: "<"))
      let gtMask = broadcastByte(UInt8(ascii: ">"))
      let quotMask = broadcastByte(UInt8(ascii: "\""))
      let aposMask = broadcastByte(UInt8(ascii: "'"))

      var idx = 0
      var spanStart = 0
      var out: [UInt8] = []

      while idx + 8 <= count {
        let word = UnsafeRawPointer(base.advanced(by: idx)).loadUnaligned(as: UInt64.self)
        let matchBits =
          zeroByteMask(word ^ ampMask)
          | zeroByteMask(word ^ ltMask)
          | zeroByteMask(word ^ gtMask)
          | zeroByteMask(word ^ quotMask)
          | zeroByteMask(word ^ aposMask)
        if matchBits == 0 {
          idx += 8
          continue
        }
        idx += matchBits.trailingZeroBitCount >> 3
        let b = buf[idx]
        if out.isEmpty {
          out.reserveCapacity(count + 16)
        }
        if spanStart < idx {
          out.append(contentsOf: buf[spanStart..<idx])
        }
        appendEscapedEntity(for: b, into: &out)
        idx += 1
        spanStart = idx
      }

      while idx < count {
        let b = buf[idx]
        if b == 0x26 || b == 0x3C || b == 0x3E || b == 0x22 || b == 0x27 {
          if out.isEmpty {
            out.reserveCapacity(count + 16)
          }
          if spanStart < idx {
            out.append(contentsOf: buf[spanStart..<idx])
          }
          appendEscapedEntity(for: b, into: &out)
          spanStart = idx + 1
        }
        idx += 1
      }

      if spanStart == 0 {
        return input
      }
      if spanStart < count {
        out.append(contentsOf: buf[spanStart..<count])
      }
      return String(decoding: out, as: UTF8.self)
    }
  }

  private static let ampEntityBytes = Array("&amp;".utf8)
  private static let ltEntityBytes = Array("&lt;".utf8)
  private static let gtEntityBytes = Array("&gt;".utf8)
  private static let quotEntityBytes = Array("&quot;".utf8)
  private static let aposEntityBytes = Array("&apos;".utf8)

  @inline(__always)
  private static func appendEscapedEntity(for byte: UInt8, into out: inout [UInt8]) {
    switch byte {
    case UInt8(ascii: "&"): out.append(contentsOf: ampEntityBytes)
    case UInt8(ascii: "<"): out.append(contentsOf: ltEntityBytes)
    case UInt8(ascii: ">"): out.append(contentsOf: gtEntityBytes)
    case UInt8(ascii: "\""): out.append(contentsOf: quotEntityBytes)
    case UInt8(ascii: "'"): out.append(contentsOf: aposEntityBytes)
    default: out.append(byte)
    }
  }

  public static func unescape(_ input: String) -> String {
    guard !input.isEmpty else { return input }
    var copy = input
    return copy.withUTF8 { buf in
      let count = buf.count
      guard let base = buf.baseAddress else { return input }
      let ampMask = broadcastByte(UInt8(ascii: "&"))
      var idx = 0
      var spanStart = 0
      var out: [UInt8] = []

      while idx + 8 <= count {
        let word = UnsafeRawPointer(base.advanced(by: idx)).loadUnaligned(as: UInt64.self)
        let matchBits = zeroByteMask(word ^ ampMask)
        if matchBits == 0 {
          idx += 8
          continue
        }
        idx += matchBits.trailingZeroBitCount >> 3
        if let (scalar, afterSemi) = decodeXMLEntityScalar(in: buf, at: idx, count: count) {
          if out.isEmpty {
            out.reserveCapacity(count)
          }
          if spanStart < idx {
            out.append(contentsOf: buf[spanStart..<idx])
          }
          appendUnicodeScalarUTF8(scalar, into: &out)
          idx = afterSemi
          spanStart = idx
        } else {
          idx += 1
        }
      }

      while idx < count {
        if buf[idx] == 0x26,
          let (scalar, afterSemi) = decodeXMLEntityScalar(in: buf, at: idx, count: count)
        {
          if out.isEmpty {
            out.reserveCapacity(count)
          }
          if spanStart < idx {
            out.append(contentsOf: buf[spanStart..<idx])
          }
          appendUnicodeScalarUTF8(scalar, into: &out)
          idx = afterSemi
          spanStart = idx
        } else {
          idx += 1
        }
      }

      if spanStart == 0 {
        return input
      }
      if spanStart < count {
        out.append(contentsOf: buf[spanStart..<count])
      }
      return String(decoding: out, as: UTF8.self)
    }
  }

  @inline(__always)
  private static func appendUnicodeScalarUTF8(_ scalar: UnicodeScalar, into out: inout [UInt8]) {
    if scalar.value < 0x80 {
      out.append(UInt8(truncatingIfNeeded: scalar.value))
    } else {
      UTF8.encode(scalar) { out.append($0) }
    }
  }

  @inline(__always)
  private static func decodeXMLEntityScalar(
    in buf: UnsafeBufferPointer<UInt8>,
    at ampIndex: Int,
    count: Int
  ) -> (scalar: UnicodeScalar, afterSemi: Int)? {
    let searchLimit = min(count, ampIndex + 14)
    let bodyStart = ampIndex + 1
    guard bodyStart < searchLimit else { return nil }

    var semiIdx = -1
    var probe = bodyStart
    while probe < searchLimit {
      let b = buf[probe]
      if b == 0x3B {
        semiIdx = probe
        break
      }
      if b == 0x26 || b <= 0x20 {
        return nil
      }
      probe += 1
    }
    guard semiIdx > bodyStart else { return nil }
    let bodyLen = semiIdx - bodyStart

    if buf[bodyStart] == 0x23 {
      let numStart = bodyStart + 1
      guard numStart < semiIdx else { return nil }
      var codePoint: UInt32 = 0
      if buf[numStart] == 0x78 || buf[numStart] == 0x58 {
        let hexStart = numStart + 1
        guard hexStart < semiIdx else { return nil }
        for k in hexStart..<semiIdx {
          let b = buf[k]
          let digit: UInt32
          if b >= 0x30 && b <= 0x39 {
            digit = UInt32(b - 0x30)
          } else if b >= 0x41 && b <= 0x46 {
            digit = UInt32(b - 0x41 + 10)
          } else if b >= 0x61 && b <= 0x66 {
            digit = UInt32(b - 0x61 + 10)
          } else {
            return nil
          }
          codePoint = (codePoint << 4) | digit
          if codePoint > 0x10FFFF { return nil }
        }
      } else {
        for k in numStart..<semiIdx {
          let b = buf[k]
          guard b >= 0x30 && b <= 0x39 else { return nil }
          codePoint = codePoint * 10 + UInt32(b - 0x30)
          if codePoint > 0x10FFFF { return nil }
        }
      }
      guard codePoint > 0, let scalar = UnicodeScalar(codePoint) else { return nil }
      return (scalar, semiIdx + 1)
    }

    switch bodyLen {
    case 2:
      if buf[bodyStart] == 0x6C && buf[bodyStart + 1] == 0x74 {
        return (UnicodeScalar(0x3C), semiIdx + 1)
      }
      if buf[bodyStart] == 0x67 && buf[bodyStart + 1] == 0x74 {
        return (UnicodeScalar(0x3E), semiIdx + 1)
      }
    case 3:
      if buf[bodyStart] == 0x61 && buf[bodyStart + 1] == 0x6D && buf[bodyStart + 2] == 0x70 {
        return (UnicodeScalar(0x26), semiIdx + 1)
      }
    case 4:
      if buf[bodyStart] == 0x71 && buf[bodyStart + 1] == 0x75 && buf[bodyStart + 2] == 0x6F
        && buf[bodyStart + 3] == 0x74
      {
        return (UnicodeScalar(0x22), semiIdx + 1)
      }
      if buf[bodyStart] == 0x61 && buf[bodyStart + 1] == 0x70 && buf[bodyStart + 2] == 0x6F
        && buf[bodyStart + 3] == 0x73
      {
        return (UnicodeScalar(0x27), semiIdx + 1)
      }
    default:
      break
    }
    return nil
  }

  private static func isTagNameBoundary(_ ch: Character) -> Bool {
    ch == " " || ch == ">" || ch == "/" || ch == "\t" || ch == "\n" || ch == "\r"
  }

  /// Scans `xml[start..<xml.endIndex]` for the closing `>` of an opening tag while ignoring `>` characters inside quoted attribute values.
  private static func findTagClosingBracket(in xml: String, from start: String.Index) -> String
    .Index?
  {
    var cursor = start
    var activeQuote: Character? = nil
    while cursor < xml.endIndex {
      let ch = xml[cursor]
      if let quote = activeQuote {
        if ch == quote { activeQuote = nil }
      } else if ch == "\"" || ch == "'" {
        activeQuote = ch
      } else if ch == ">" {
        return cursor
      }
      cursor = xml.index(after: cursor)
    }
    return nil
  }

  /// Zero-allocation reverse UTF-8 scan checking whether an XML tag's attribute slice ends with `/`.
  @inlinable
  internal static func isSelfClosingTag(_ segment: Substring) -> Bool {
    let utf8 = segment.utf8
    var idx = utf8.endIndex
    while idx > utf8.startIndex {
      idx = utf8.index(before: idx)
      let b = utf8[idx]
      if b == 0x20 || b == 0x09 || b == 0x0A || b == 0x0D { continue }
      return b == 0x2F  // '/'
    }
    return false
  }

  /// Extracts all top-level XML elements matching `<tagName ...>inner</tagName>`, tracking nesting depth for identical child tags.
  public static func extractElements(named tagName: String, in xml: String) -> [(
    attributes: [String: String], innerXML: String
  )] {
    var results: [(attributes: [String: String], innerXML: String)] = []
    let openPrefix = "<\(tagName)"
    let closeTag = "</\(tagName)>"
    var searchStart = xml.startIndex

    while let openRange = xml.range(of: openPrefix, range: searchStart..<xml.endIndex) {
      let afterTagName = openRange.upperBound
      if afterTagName < xml.endIndex {
        guard isTagNameBoundary(xml[afterTagName]) else {
          searchStart = afterTagName
          continue
        }
      }
      guard let tagEndIdx = findTagClosingBracket(in: xml, from: afterTagName) else { break }
      let afterTagEnd = xml.index(after: tagEndIdx)
      let attrSlice = xml[afterTagName..<tagEndIdx]
      let attrs = parseAttributes(from: attrSlice)

      if isSelfClosingTag(attrSlice) {
        results.append((attributes: attrs, innerXML: ""))
        searchStart = afterTagEnd
        continue
      }

      var depth = 1
      var scanCursor = afterTagEnd
      var matchedCloseRange: Range<String.Index>?

      while depth > 0 && scanCursor < xml.endIndex {
        guard let nextClose = xml.range(of: closeTag, range: scanCursor..<xml.endIndex) else {
          break
        }
        if let nextOpen = xml.range(of: openPrefix, range: scanCursor..<nextClose.lowerBound) {
          let afterNestedName = nextOpen.upperBound
          if afterNestedName < xml.endIndex && isTagNameBoundary(xml[afterNestedName]) {
            guard let nestedTagEndIdx = findTagClosingBracket(in: xml, from: afterNestedName) else {
              break
            }
            let nestedAttr = xml[afterNestedName..<nestedTagEndIdx]
            if !isSelfClosingTag(nestedAttr) { depth += 1 }
            scanCursor = xml.index(after: nestedTagEndIdx)
          } else {
            scanCursor = afterNestedName
          }
        } else {
          depth -= 1
          if depth == 0 { matchedCloseRange = nextClose } else { scanCursor = nextClose.upperBound }
        }
      }

      guard let closeRange = matchedCloseRange else { break }
      let inner = String(xml[afterTagEnd..<closeRange.lowerBound])
      results.append((attributes: attrs, innerXML: inner))
      searchStart = closeRange.upperBound
    }
    return results
  }

  /// Removes all occurrences of `<tagName ...>...</tagName>` (including nested same-tag elements) from `xml`.
  public static func stripElements(named tagName: String, from xml: String) -> String {
    let openPrefix = "<\(tagName)"
    let closeTag = "</\(tagName)>"
    var output = ""
    output.reserveCapacity(xml.count)
    var searchStart = xml.startIndex

    while let openRange = xml.range(of: openPrefix, range: searchStart..<xml.endIndex) {
      let afterTagName = openRange.upperBound
      if afterTagName < xml.endIndex && !isTagNameBoundary(xml[afterTagName]) {
        output.append(contentsOf: xml[searchStart..<afterTagName])
        searchStart = afterTagName
        continue
      }
      guard let tagEndIdx = findTagClosingBracket(in: xml, from: afterTagName) else { break }
      let afterTagEnd = xml.index(after: tagEndIdx)
      output.append(contentsOf: xml[searchStart..<openRange.lowerBound])
      let attrSegment = xml[afterTagName..<tagEndIdx]
      if isSelfClosingTag(attrSegment) {
        searchStart = afterTagEnd
        continue
      }

      var depth = 1
      var scanCursor = afterTagEnd
      var matchedCloseUpper: String.Index?

      while depth > 0 && scanCursor < xml.endIndex {
        guard let nextClose = xml.range(of: closeTag, range: scanCursor..<xml.endIndex) else {
          break
        }
        if let nextOpen = xml.range(of: openPrefix, range: scanCursor..<nextClose.lowerBound) {
          let afterNestedName = nextOpen.upperBound
          if afterNestedName < xml.endIndex && isTagNameBoundary(xml[afterNestedName]) {
            guard let nestedTagEndIdx = findTagClosingBracket(in: xml, from: afterNestedName) else {
              break
            }
            let nestedAttr = xml[afterNestedName..<nestedTagEndIdx]
            if !isSelfClosingTag(nestedAttr) {
              depth += 1
            }
            scanCursor = xml.index(after: nestedTagEndIdx)
          } else {
            scanCursor = afterNestedName
          }
        } else {
          depth -= 1
          if depth == 0 {
            matchedCloseUpper = nextClose.upperBound
          } else {
            scanCursor = nextClose.upperBound
          }
        }
      }

      guard let closeUpper = matchedCloseUpper else {
        searchStart = xml.endIndex
        break
      }
      searchStart = closeUpper
    }
    if searchStart < xml.endIndex {
      output.append(contentsOf: xml[searchStart..<xml.endIndex])
    }
    return output
  }

  public static func parseAttributes<S: StringProtocol>(from rawAttributeSegment: S) -> [String:
    String]
  {
    var attributes: [String: String] = [:]
    var cursor = rawAttributeSegment.startIndex
    while cursor < rawAttributeSegment.endIndex {
      while cursor < rawAttributeSegment.endIndex
        && (rawAttributeSegment[cursor].isWhitespace || rawAttributeSegment[cursor] == "/")
      {
        cursor = rawAttributeSegment.index(after: cursor)
      }
      guard cursor < rawAttributeSegment.endIndex else { break }

      let nameStart = cursor
      while cursor < rawAttributeSegment.endIndex && rawAttributeSegment[cursor] != "="
        && !rawAttributeSegment[cursor].isWhitespace
      {
        cursor = rawAttributeSegment.index(after: cursor)
      }
      let key = String(rawAttributeSegment[nameStart..<cursor]).trimmingCharacters(in: .whitespaces)
      guard !key.isEmpty else { break }

      while cursor < rawAttributeSegment.endIndex && rawAttributeSegment[cursor].isWhitespace {
        cursor = rawAttributeSegment.index(after: cursor)
      }
      guard cursor < rawAttributeSegment.endIndex, rawAttributeSegment[cursor] == "=" else {
        break
      }
      cursor = rawAttributeSegment.index(after: cursor)
      while cursor < rawAttributeSegment.endIndex && rawAttributeSegment[cursor].isWhitespace {
        cursor = rawAttributeSegment.index(after: cursor)
      }
      guard cursor < rawAttributeSegment.endIndex,
        rawAttributeSegment[cursor] == "\"" || rawAttributeSegment[cursor] == "'"
      else {
        break
      }
      let quoteChar = rawAttributeSegment[cursor]
      cursor = rawAttributeSegment.index(after: cursor)
      let valueStart = cursor
      while cursor < rawAttributeSegment.endIndex && rawAttributeSegment[cursor] != quoteChar {
        cursor = rawAttributeSegment.index(after: cursor)
      }
      let rawValue = String(rawAttributeSegment[valueStart..<cursor])
      attributes[key] = unescape(rawValue)
      if cursor < rawAttributeSegment.endIndex {
        cursor = rawAttributeSegment.index(after: cursor)
      }
    }
    return attributes
  }
}
