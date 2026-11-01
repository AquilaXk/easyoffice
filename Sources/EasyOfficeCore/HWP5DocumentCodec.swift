import Foundation

// MARK: - Extended Document Metadata Envelope (Lossless Round-Trip Support)

internal typealias HWP5ExtendedMetadata = ExtendedDocumentMetadataEnvelope

internal struct ExtendedDocumentMetadataEnvelope: Codable, Sendable {
  var paragraphs: [ParagraphBlock]
  var tables: [DocumentTableBlock]
  var sections: [DocumentSectionLayout]
  var footnotes: [FootnoteItem]
  var revisions: [TrackedRevision]
  var comments: [DocumentCommentThread]
  var bookmarks: [String: UUID]
  var images: [EmbeddedImageAttachment]
  var equations: [DocumentEquationBlock]
  var watermarkText: String?

  init(from doc: OfficeDocumentModel) {
    self.paragraphs = doc.paragraphs
    self.tables = doc.tables
    self.sections = doc.sections
    self.footnotes = doc.footnotes
    self.revisions = doc.revisions
    self.comments = doc.comments
    self.bookmarks = doc.bookmarks
    self.images = doc.images
    self.equations = doc.equations
    self.watermarkText = doc.watermarkText
  }

  private enum CodingKeys: String, CodingKey {
    case paragraphs, tables, sections, footnotes, revisions, comments, bookmarks, images, equations,
      watermarkText
  }

  init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    self.paragraphs = try c.decodeIfPresent([ParagraphBlock].self, forKey: .paragraphs) ?? []
    self.tables = try c.decodeIfPresent([DocumentTableBlock].self, forKey: .tables) ?? []
    self.sections = try c.decodeIfPresent([DocumentSectionLayout].self, forKey: .sections) ?? []
    self.footnotes = try c.decodeIfPresent([FootnoteItem].self, forKey: .footnotes) ?? []
    self.revisions = try c.decodeIfPresent([TrackedRevision].self, forKey: .revisions) ?? []
    self.comments = try c.decodeIfPresent([DocumentCommentThread].self, forKey: .comments) ?? []
    self.bookmarks = try c.decodeIfPresent([String: UUID].self, forKey: .bookmarks) ?? [:]
    self.images = try c.decodeIfPresent([EmbeddedImageAttachment].self, forKey: .images) ?? []
    self.equations = try c.decodeIfPresent([DocumentEquationBlock].self, forKey: .equations) ?? []
    self.watermarkText = try c.decodeIfPresent(String.self, forKey: .watermarkText)
  }

  func apply(to doc: inout OfficeDocumentModel) {
    for idx in doc.paragraphs.indices where idx < paragraphs.count {
      let p = doc.paragraphs[idx]
      let metaP = paragraphs[idx]
      let resolvedListKind = metaP.listKind != .none ? metaP.listKind : p.listKind
      let resolvedListLevel = metaP.listKind != .none ? metaP.listLevel : p.listLevel
      let resolvedText =
        (metaP.listKind != .none && !metaP.text.isEmpty) ? metaP.text : p.text
      let resolvedRuns: [DocumentTextRun]
      if !metaP.runs.isEmpty {
        resolvedRuns = metaP.runs
      } else if p.runs.map(\.text).joined() == resolvedText {
        resolvedRuns = p.runs
      } else {
        resolvedRuns = []
      }
      doc.paragraphs[idx] = ParagraphBlock(
        id: metaP.id,
        style: p.style,
        alignment: p.alignment,
        text: resolvedText,
        runs: resolvedRuns,
        fontName: p.fontName,
        fontSizePt: p.fontSizePt,
        isBold: p.isBold,
        isItalic: p.isItalic,
        isUnderlined: p.isUnderlined,
        lineSpacingPercent: p.lineSpacingPercent,
        firstLineIndentPt: metaP.firstLineIndentPt != 0
          ? metaP.firstLineIndentPt : p.firstLineIndentPt,
        leftIndentPt: metaP.leftIndentPt != 0 ? metaP.leftIndentPt : p.leftIndentPt,
        listKind: resolvedListKind,
        listLevel: resolvedListLevel,
        letterSpacingPercent: metaP.letterSpacingPercent != 0.0
          ? metaP.letterSpacingPercent : p.letterSpacingPercent,
        charWidthScalePercent: metaP.charWidthScalePercent != 100.0
          ? metaP.charWidthScalePercent : p.charWidthScalePercent
      )
    }
    for idx in doc.tables.indices where idx < tables.count {
      let t = doc.tables[idx]
      let metaT = tables[idx]
      doc.tables[idx] = DocumentTableBlock(
        id: metaT.id,
        caption: !metaT.caption.isEmpty ? metaT.caption : t.caption,
        rows: t.rows,
        hasHeaderRow: metaT.hasHeaderRow,
        cellSpans: metaT.cellSpans.isEmpty ? t.cellSpans : metaT.cellSpans,
        borderStyle: metaT.borderStyle
      )
    }
    if !sections.isEmpty { doc.sections = sections }
    if !footnotes.isEmpty { doc.footnotes = footnotes }
    if !revisions.isEmpty { doc.revisions = revisions }
    if !comments.isEmpty { doc.comments = comments }
    if !bookmarks.isEmpty { doc.bookmarks = bookmarks }
    if !images.isEmpty { doc.images = images }
    if !equations.isEmpty { doc.equations = equations }
    if let watermarkText, !watermarkText.isEmpty { doc.watermarkText = watermarkText }
  }
}

// MARK: - HWP 5.0 32-Bit Record Header Bitfield

/// HWP 5.0 binary record header packing `tagID` (10 bits), `level` (10 bits), and `payloadSize` (12 bits or extended 32 bits).
public struct HWP5RecordHeader: Sendable, Equatable {
  public static let tagDocumentProperties: UInt16 = 16
  public static let tagParaHeaderStandard: UInt16 = 50
  public static let tagParaTextStandard: UInt16 = 51
  public static let tagListHeaderStandard: UInt16 = 56
  public static let tagTableControlStandard: UInt16 = 61
  public static let tagParaHeader: UInt16 = 66
  public static let tagParaText: UInt16 = 67
  public static let tagListHeader: UInt16 = 72
  public static let tagTableRecord: UInt16 = 77
  public static let tagExtendedMetadata: UInt16 = 90

  public static let tenBitMask: UInt32 = 0x03FF
  public static let twelveBitMask: UInt32 = 0x0FFF
  public static let extendedSizeMarker: UInt32 = 0x0FFF

  public let tagID: UInt16
  public let level: UInt16
  public let payloadSize: UInt32

  public init(tagID: UInt16, level: UInt16, payloadSize: UInt32) {
    self.tagID = tagID
    self.level = level
    self.payloadSize = payloadSize
  }

  public static func encode(tagID: UInt16, level: UInt16, payloadSize: UInt32) throws -> Data {
    guard UInt32(tagID) <= tenBitMask, UInt32(level) <= tenBitMask else {
      throw EasyOfficeError.corruptedContainer(
        detail: "HWP 5.0 레코드 헤더 10-bit 범위 초과 (tagID: \(tagID), level: \(level))")
    }
    var output = Data()
    if payloadSize < extendedSizeMarker {
      let word =
        (UInt32(tagID) & tenBitMask) | ((UInt32(level) & tenBitMask) << 10)
        | ((payloadSize & twelveBitMask) << 20)
      BitPacking.appendUInt32LE(word, to: &output)
    } else {
      let word =
        (UInt32(tagID) & tenBitMask) | ((UInt32(level) & tenBitMask) << 10)
        | (extendedSizeMarker << 20)
      BitPacking.appendUInt32LE(word, to: &output)
      BitPacking.appendUInt32LE(payloadSize, to: &output)
    }
    return output
  }

  public static func decode(from data: Data, at offset: Int) throws -> (
    header: HWP5RecordHeader, headerByteLength: Int
  ) {
    let word = try BitPacking.readUInt32LE(from: data, at: offset)
    let tagID = UInt16(word & tenBitMask)
    let level = UInt16((word >> 10) & tenBitMask)
    let inlineSize = (word >> 20) & twelveBitMask

    if inlineSize == extendedSizeMarker {
      let extendedSize = try BitPacking.readUInt32LE(from: data, at: offset + 4)
      let header = HWP5RecordHeader(tagID: tagID, level: level, payloadSize: extendedSize)
      return (header, 8)
    } else {
      let header = HWP5RecordHeader(tagID: tagID, level: level, payloadSize: inlineSize)
      return (header, 4)
    }
  }
}

// MARK: - HWP 5.0 Binary Document Codec (Compressed & Uncompressed Streams)

/// HWP 5.0 binary document codec built on top of `OLE2ContainerCodec` and `HWP5RecordHeader`.
public enum HWP5BinaryCodec {
  public static let fileHeaderByteLength = 256
  public static let signatureString = "HWP Document File"
  public static let defaultVersion: UInt32 = 0x0500_0300  // 5.0.3.0
  public static let flagCompressed: UInt32 = 0x0000_0001
  public static let flagEncrypted: UInt32 = 0x0000_0002
  public static let flagDistribution: UInt32 = 0x0000_0004
  public static let maxHWP5Records = 524_288
  public static let maxTotalHWP5TableCells = 65_536
  public static let maxHWPTableRows = 4_096
  public static let maxHWP5TableRows = maxHWPTableRows
  public static let maxHWP5TableColumns = 256

  private static func safeClampedUInt32(
    _ value: Double, min minVal: UInt32 = 0, max maxVal: UInt32 = 100_000_000
  ) -> UInt32 {
    guard value.isFinite else { return minVal }
    let clamped = Swift.max(Double(minVal), Swift.min(Double(maxVal), value.rounded()))
    return UInt32(clamped)
  }

  private static func safeClampedInt32(
    _ value: Double, min minVal: Int32 = -100_000_000, max maxVal: Int32 = 100_000_000
  ) -> Int32 {
    guard value.isFinite else { return 0 }
    let clamped = Swift.max(Double(minVal), Swift.min(Double(maxVal), value.rounded()))
    return Int32(clamped)
  }

  public static func encodeDocument(_ doc: OfficeDocumentModel, compressed: Bool = true) throws
    -> Data
  {
    for table in doc.tables {
      guard table.rows.count <= maxHWPTableRows else {
        throw EasyOfficeError.resourceBudgetExceeded(
          detail: "HWP 5.0 표 행 개수(\(table.rows.count))가 허용 상한(\(maxHWPTableRows))을 초과했습니다.")
      }
      for row in table.rows {
        guard row.count <= maxHWP5TableColumns else {
          throw EasyOfficeError.resourceBudgetExceeded(
            detail: "HWP 5.0 표 열 개수(\(row.count))가 허용 상한(\(maxHWP5TableColumns))을 초과했습니다.")
        }
      }
    }

    var fileHeader = Data(repeating: 0, count: fileHeaderByteLength)
    let sigBytes = Array(signatureString.utf8)
    for (idx, b) in sigBytes.enumerated() {
      fileHeader[idx] = b
    }
    try BitPacking.writeUInt32LE(defaultVersion, into: &fileHeader, at: 32)
    let headerFlags: UInt32 = compressed ? flagCompressed : 0
    try BitPacking.writeUInt32LE(headerFlags, into: &fileHeader, at: 36)

    var docPropsPayload = Data()
    BitPacking.appendLengthPrefixedUTF16LE(doc.title, to: &docPropsPayload)
    BitPacking.appendLengthPrefixedUTF16LE(doc.paperSize.rawValue, to: &docPropsPayload)
    BitPacking.appendLengthPrefixedUTF16LE(doc.headerText, to: &docPropsPayload)
    BitPacking.appendLengthPrefixedUTF16LE(doc.footerText, to: &docPropsPayload)
    docPropsPayload.append(doc.trackChangesEnabled ? 1 : 0)

    var rawDocInfoStream = try HWP5RecordHeader.encode(
      tagID: HWP5RecordHeader.tagDocumentProperties,
      level: 0,
      payloadSize: try BitPacking.checkedUInt32(docPropsPayload.count, field: "HWP 5.0 문서 속성 크기")
    )
    rawDocInfoStream.append(docPropsPayload)

    let extEnvelope = ExtendedDocumentMetadataEnvelope(from: doc)
    if let extPayload = try? JSONEncoder().encode(extEnvelope), !extPayload.isEmpty {
      let extRec = try HWP5RecordHeader.encode(
        tagID: HWP5RecordHeader.tagExtendedMetadata,
        level: 0,
        payloadSize: try BitPacking.checkedUInt32(extPayload.count, field: "HWP 5.0 확장 메타데이터 크기")
      )
      rawDocInfoStream.append(extRec)
      rawDocInfoStream.append(extPayload)
    }

    var rawSection0Stream = Data()

    for paragraph in doc.paragraphs {
      var paraHeaderPayload = Data()
      BitPacking.appendLengthPrefixedUTF16LE(paragraph.style.rawValue, to: &paraHeaderPayload)
      BitPacking.appendLengthPrefixedUTF16LE(paragraph.alignment.rawValue, to: &paraHeaderPayload)
      BitPacking.appendLengthPrefixedUTF16LE(paragraph.fontName, to: &paraHeaderPayload)
      BitPacking.appendUInt32LE(
        safeClampedUInt32(paragraph.fontSizePt * 100.0, min: 100, max: 1_000_000),
        to: &paraHeaderPayload)
      BitPacking.appendUInt32LE(
        safeClampedUInt32(paragraph.lineSpacingPercent * 100.0, min: 5_000, max: 1_000_000),
        to: &paraHeaderPayload)
      var styleFlags: UInt8 = 0
      if paragraph.isBold { styleFlags |= 0x01 }
      if paragraph.isItalic { styleFlags |= 0x02 }
      if paragraph.isUnderlined { styleFlags |= 0x04 }
      paraHeaderPayload.append(styleFlags)
      BitPacking.appendUInt32LE(
        UInt32(bitPattern: safeClampedInt32(paragraph.letterSpacingPercent * 100.0)),
        to: &paraHeaderPayload)
      BitPacking.appendUInt32LE(
        safeClampedUInt32(paragraph.charWidthScalePercent * 100.0, min: 1_000, max: 1_000_000),
        to: &paraHeaderPayload)

      let headerRec = try HWP5RecordHeader.encode(
        tagID: HWP5RecordHeader.tagParaHeader,
        level: 0,
        payloadSize: try BitPacking.checkedUInt32(paraHeaderPayload.count, field: "HWP 5.0 문단 헤더 크기")
      )
      rawSection0Stream.append(headerRec)
      rawSection0Stream.append(paraHeaderPayload)

      let textPayload = BitPacking.encodeUTF16LE(paragraph.text)
      let textRec = try HWP5RecordHeader.encode(
        tagID: HWP5RecordHeader.tagParaText,
        level: 1,
        payloadSize: try BitPacking.checkedUInt32(textPayload.count, field: "HWP 5.0 문단 텍스트 크기")
      )
      rawSection0Stream.append(textRec)
      rawSection0Stream.append(textPayload)
    }

    for table in doc.tables {
      let rowCount = table.rows.count
      let colCount = table.rows.map(\.count).max() ?? 0
      var tableControlPayload = Data()
      BitPacking.appendUInt32LE(table.hasHeaderRow ? 1 : 0, to: &tableControlPayload)
      BitPacking.appendUInt16LE(UInt16(rowCount), to: &tableControlPayload)
      BitPacking.appendUInt16LE(UInt16(colCount), to: &tableControlPayload)
      for rIdx in 0..<rowCount {
        BitPacking.appendUInt16LE(UInt16(table.rows[rIdx].count), to: &tableControlPayload)
      }
      let tableControlRec = try HWP5RecordHeader.encode(
        tagID: HWP5RecordHeader.tagTableControlStandard,
        level: 1,
        payloadSize: UInt32(tableControlPayload.count)
      )
      rawSection0Stream.append(tableControlRec)
      rawSection0Stream.append(tableControlPayload)

      for rIdx in 0..<rowCount {
        let row = table.rows[rIdx]
        let rowLimit = row.count
        for cIdx in 0..<rowLimit {
          var listHeaderPayload = Data()
          BitPacking.appendUInt16LE(1, to: &listHeaderPayload)  // paragraph count in cell
          BitPacking.appendUInt16LE(UInt16(cIdx), to: &listHeaderPayload)
          BitPacking.appendUInt16LE(UInt16(rIdx), to: &listHeaderPayload)
          BitPacking.appendUInt16LE(0, to: &listHeaderPayload)
          let listRec = try HWP5RecordHeader.encode(
            tagID: HWP5RecordHeader.tagListHeaderStandard,
            level: 2,
            payloadSize: UInt32(listHeaderPayload.count)
          )
          rawSection0Stream.append(listRec)
          rawSection0Stream.append(listHeaderPayload)

          let cellTextBytes = BitPacking.encodeUTF16LE(row[cIdx])
          let cellTextRec = try HWP5RecordHeader.encode(
            tagID: HWP5RecordHeader.tagParaTextStandard,
            level: 3,
            payloadSize: try BitPacking.checkedUInt32(cellTextBytes.count, field: "HWP 5.0 셀 텍스트 크기")
          )
          rawSection0Stream.append(cellTextRec)
          rawSection0Stream.append(cellTextBytes)
        }
      }
    }

    let docInfoStream =
      (compressed && !rawDocInfoStream.isEmpty)
      ? try ZipContainerCodec.compressRawDeflate(rawDocInfoStream)
      : rawDocInfoStream
    let section0Stream =
      (compressed && !rawSection0Stream.isEmpty)
      ? try ZipContainerCodec.compressRawDeflate(rawSection0Stream)
      : rawSection0Stream

    var oleStreams: [OLE2StreamEntry] = [
      OLE2StreamEntry(name: "FileHeader", data: fileHeader),
      OLE2StreamEntry(name: "DocInfo", data: docInfoStream),
      OLE2StreamEntry(name: "BodyText/Section0", data: section0Stream),
    ]
    if doc.sections.count > 1 {
      for secIdx in 1..<doc.sections.count {
        oleStreams.append(OLE2StreamEntry(name: "BodyText/Section\(secIdx)", data: Data()))
      }
    }

    return try OLE2ContainerCodec.pack(streams: oleStreams)
  }

  public static func decodeDocument(from data: Data) throws -> OfficeDocumentModel {
    let streams = try OLE2ContainerCodec.unpack(from: data)
    var streamMap: [String: Data] = [:]
    for stream in streams {
      streamMap[stream.name] = stream.data
    }

    guard let fileHeader = streamMap["FileHeader"], fileHeader.count >= 40 else {
      throw EasyOfficeError.corruptedContainer(
        detail: "HWP 5.0 필수 스트림 'FileHeader'가 누락되었거나 손상되었습니다.")
    }

    let expectedSig = Data(signatureString.utf8)
    guard fileHeader.prefix(expectedSig.count) == expectedSig else {
      throw EasyOfficeError.corruptedContainer(
        detail: "HWP 5.0 'HWP Document File' 시그니처가 일치하지 않습니다.")
    }

    let flags = try BitPacking.readUInt32LE(from: fileHeader, at: 36)
    if (flags & (flagEncrypted | flagDistribution)) != 0 {
      throw EasyOfficeError.encryptedDocumentUnsupported(format: .hwp)
    }
    let isCompressed = (flags & flagCompressed) != 0

    var title = "제목 없음"
    var paperSize: PaperSizePreset = .a4
    var headerText = ""
    var footerText = ""
    var trackChanges = false
    var extendedEnvelope: ExtendedDocumentMetadataEnvelope?
    var totalDecodedRecords = 0
    var totalDecodedTableCells = 0

    if let rawDocInfo = streamMap["DocInfo"], !rawDocInfo.isEmpty {
      let docInfo =
        isCompressed ? try ZipContainerCodec.decompressRawDeflateStream(rawDocInfo) : rawDocInfo
      var docOffset = 0
      var parsedDocProps = false
      while docOffset < docInfo.count {
        totalDecodedRecords += 1
        guard totalDecodedRecords <= maxHWP5Records else {
          throw EasyOfficeError.securityViolation(
            detail: "HWP 5.0 레코드 개수가 허용 상한(\(maxHWP5Records))을 초과했습니다.")
        }
        let (recHeader, headerLen) = try HWP5RecordHeader.decode(from: docInfo, at: docOffset)
        let payloadStart = docOffset + headerLen
        let recordEnd = payloadStart + Int(recHeader.payloadSize)
        guard recordEnd <= docInfo.count else {
          throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 DocInfo 레코드가 손상되었습니다.")
        }
        if recHeader.tagID == HWP5RecordHeader.tagDocumentProperties {
          if recHeader.payloadSize == 26 {
            let sectionCount = try BitPacking.readUInt16LE(from: docInfo, at: payloadStart)
            guard sectionCount >= 1 else {
              throw EasyOfficeError.corruptedContainer(
                detail: "HWP 5.0 표준 문서 속성 레코드의 구역 개수가 유효하지 않습니다.")
            }
            parsedDocProps = true
          } else {
            var cursor = payloadStart
            title = try BitPacking.readLengthPrefixedUTF16LE(
              from: docInfo, offset: &cursor, limit: recordEnd)
            let paperRaw = try BitPacking.readLengthPrefixedUTF16LE(
              from: docInfo, offset: &cursor, limit: recordEnd)
            if cursor == recordEnd {
              paperSize = .a4
              headerText = paperRaw
            } else {
              paperSize = PaperSizePreset(rawValue: paperRaw) ?? .a4
              headerText = try BitPacking.readLengthPrefixedUTF16LE(
                from: docInfo, offset: &cursor, limit: recordEnd)
              footerText = try BitPacking.readLengthPrefixedUTF16LE(
                from: docInfo, offset: &cursor, limit: recordEnd)
              if cursor < recordEnd {
                trackChanges = docInfo[docInfo.startIndex + cursor] != 0
              }
            }
            parsedDocProps = true
          }
        } else if recHeader.tagID == HWP5RecordHeader.tagExtendedMetadata {
          let slice = Data(
            docInfo[(docInfo.startIndex + payloadStart)..<(docInfo.startIndex + recordEnd)])
          extendedEnvelope = try? JSONDecoder().decode(
            ExtendedDocumentMetadataEnvelope.self, from: slice)
        }
        docOffset = recordEnd
      }
      if !parsedDocProps {
        throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 DocInfo 문서 속성 레코드가 누락되었습니다.")
      }
    }

    var rawSectionStreams: [Data] = []
    if let sec0 = streamMap["BodyText/Section0"] ?? streamMap["Section0"] {
      rawSectionStreams.append(sec0)
    }
    var secIdx = 1
    while let nextSec = streamMap["BodyText/Section\(secIdx)"] ?? streamMap["Section\(secIdx)"] {
      rawSectionStreams.append(nextSec)
      secIdx += 1
    }
    guard !rawSectionStreams.isEmpty else {
      throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 'BodyText/Section0' 스트림이 누락되었습니다.")
    }

    var paragraphs: [ParagraphBlock] = []
    var tables: [DocumentTableBlock] = []

    for rawSection in rawSectionStreams {
      let sectionData =
        (isCompressed && !rawSection.isEmpty)
        ? try ZipContainerCodec.decompressRawDeflateStream(rawSection)
        : rawSection
      try decodeSectionStream(
        sectionData,
        paragraphs: &paragraphs,
        tables: &tables,
        totalDecodedRecords: &totalDecodedRecords,
        totalDecodedTableCells: &totalDecodedTableCells
      )
    }

    var docModel = OfficeDocumentModel(
      title: title,
      format: .hwp,
      paperSize: paperSize,
      headerText: headerText,
      footerText: footerText,
      paragraphs: paragraphs,
      tables: tables,
      trackChangesEnabled: trackChanges
    )
    if rawSectionStreams.count > 1 && docModel.sections.count < rawSectionStreams.count {
      docModel.sections = (0..<rawSectionStreams.count).map { _ in
        DocumentSectionLayout(paperSize: paperSize)
      }
    }
    extendedEnvelope?.apply(to: &docModel)
    return docModel
  }

  private static func decodeSectionStream(
    _ section0: Data,
    paragraphs: inout [ParagraphBlock],
    tables: inout [DocumentTableBlock],
    totalDecodedRecords: inout Int,
    totalDecodedTableCells: inout Int
  ) throws {
    var pendingParagraph: ParagraphBlock?
    var offset = 0

    while offset < section0.count {
      totalDecodedRecords += 1
      guard totalDecodedRecords <= maxHWP5Records else {
        throw EasyOfficeError.securityViolation(
          detail: "HWP 5.0 레코드 개수가 허용 상한(\(maxHWP5Records))을 초과했습니다.")
      }
      let (recHeader, headerLen) = try HWP5RecordHeader.decode(from: section0, at: offset)
      let payloadStart = offset + headerLen
      let payloadLen = Int(recHeader.payloadSize)
      let payloadEnd = payloadStart + payloadLen
      guard payloadLen >= 0, payloadEnd <= section0.count else {
        throw EasyOfficeError.corruptedContainer(
          detail: "HWP 5.0 Section 레코드 페이로드가 스트림 경계를 초과합니다.")
      }

      switch recHeader.tagID {
      case HWP5RecordHeader.tagParaHeader, HWP5RecordHeader.tagParaHeaderStandard:
        if let prev = pendingParagraph {
          paragraphs.append(prev)
        }
        pendingParagraph = try decodeParaHeaderPayload(
          from: section0, payloadStart: payloadStart, payloadEnd: payloadEnd, payloadLen: payloadLen)

      case HWP5RecordHeader.tagParaText, HWP5RecordHeader.tagParaTextStandard:
        let text = try decodeParaTextPayload(
          from: section0, at: payloadStart, byteLength: payloadLen)
        if var current = pendingParagraph {
          current.text = text
          paragraphs.append(current)
          pendingParagraph = nil
        } else {
          paragraphs.append(ParagraphBlock(text: text))
        }

      case HWP5RecordHeader.tagTableControlStandard:
        if let prev = pendingParagraph {
          paragraphs.append(prev)
          pendingParagraph = nil
        }
        var nextOffset = payloadEnd
        let table = try decodeStandardTableControl(
          from: section0,
          payloadStart: payloadStart,
          payloadEnd: payloadEnd,
          nextOffset: &nextOffset,
          totalDecodedRecords: &totalDecodedRecords,
          totalDecodedTableCells: &totalDecodedTableCells
        )
        tables.append(table)
        offset = nextOffset
        continue

      case HWP5RecordHeader.tagTableRecord:
        if let prev = pendingParagraph {
          paragraphs.append(prev)
          pendingParagraph = nil
        }
        let table = try decodeLegacyTableRecord(
          from: section0,
          payloadStart: payloadStart,
          payloadEnd: payloadEnd,
          totalDecodedTableCells: &totalDecodedTableCells
        )
        tables.append(table)

      default:
        break
      }

      offset = payloadEnd
    }

    if let trailing = pendingParagraph {
      paragraphs.append(trailing)
    }
  }

  private static func decodeParaHeaderPayload(
    from section0: Data,
    payloadStart: Int,
    payloadEnd: Int,
    payloadLen: Int
  ) throws -> ParagraphBlock {
    if payloadLen == 22 || payloadLen == 24 {
      return ParagraphBlock(text: "")
    }
    var pCursor = payloadStart
    guard let styleRaw = try? BitPacking.readLengthPrefixedUTF16LE(
      from: section0, offset: &pCursor, limit: payloadEnd)
    else {
      return ParagraphBlock(text: "")
    }
    if pCursor + 1 == payloadEnd {
      let isOfficial = section0[section0.startIndex + pCursor] != 0
      let resolvedStyle: ParagraphStyleRole =
        isOfficial ? .officialHeader : (ParagraphStyleRole(rawValue: styleRaw) ?? .body)
      return ParagraphBlock(style: resolvedStyle, text: "")
    }
    let alignRaw = try BitPacking.readLengthPrefixedUTF16LE(
      from: section0, offset: &pCursor, limit: payloadEnd)
    let fontName = try BitPacking.readLengthPrefixedUTF16LE(
      from: section0, offset: &pCursor, limit: payloadEnd)
    guard pCursor + 9 <= payloadEnd else {
      throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 문단 헤더 레코드 길이가 부족합니다.")
    }
    let fontSizeX100 = try BitPacking.readUInt32LE(from: section0, at: pCursor)
    pCursor += 4
    let lineSpacingX100 = try BitPacking.readUInt32LE(from: section0, at: pCursor)
    pCursor += 4
    let styleFlags = section0[section0.startIndex + pCursor]
    pCursor += 1
    var letterSpacingPct = 0.0
    var charWidthScalePct = 100.0
    if pCursor + 8 <= payloadEnd {
      let letterSpacingBits = try BitPacking.readUInt32LE(from: section0, at: pCursor)
      pCursor += 4
      let charScaleX100 = try BitPacking.readUInt32LE(from: section0, at: pCursor)
      pCursor += 4
      letterSpacingPct = Double(Int32(bitPattern: letterSpacingBits)) / 100.0
      charWidthScalePct = Double(charScaleX100) / 100.0
    }
    return ParagraphBlock(
      style: ParagraphStyleRole(rawValue: styleRaw) ?? .body,
      alignment: TextAlignmentMode(rawValue: alignRaw) ?? .justified,
      text: "",
      fontName: fontName,
      fontSizePt: Double(fontSizeX100) / 100.0,
      isBold: (styleFlags & 0x01) != 0,
      isItalic: (styleFlags & 0x02) != 0,
      isUnderlined: (styleFlags & 0x04) != 0,
      lineSpacingPercent: Double(lineSpacingX100) / 100.0,
      letterSpacingPercent: letterSpacingPct,
      charWidthScalePercent: charWidthScalePct
    )
  }

  private static func decodeStandardTableControl(
    from section0: Data,
    payloadStart: Int,
    payloadEnd: Int,
    nextOffset: inout Int,
    totalDecodedRecords: inout Int,
    totalDecodedTableCells: inout Int
  ) throws -> DocumentTableBlock {
    guard payloadStart + 8 <= payloadEnd else {
      throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 HWPTAG_TABLE(61) 레코드 길이가 부족합니다.")
    }
    let flags = try BitPacking.readUInt32LE(from: section0, at: payloadStart)
    let hasHeader = (flags & 0x01) != 0
    let rowCount = Int(try BitPacking.readUInt16LE(from: section0, at: payloadStart + 4))
    let colCount = Int(try BitPacking.readUInt16LE(from: section0, at: payloadStart + 6))
    guard rowCount <= maxHWPTableRows else {
      throw EasyOfficeError.resourceBudgetExceeded(
        detail: "HWP 5.0 표 행 개수(\(rowCount))가 허용 상한(\(maxHWPTableRows))을 초과했습니다.")
    }
    guard colCount <= maxHWP5TableColumns else {
      throw EasyOfficeError.resourceBudgetExceeded(
        detail: "HWP 5.0 표 열 개수(\(colCount))가 허용 상한(\(maxHWP5TableColumns))을 초과했습니다.")
    }

    var rowLengths: [Int] = Array(repeating: colCount, count: rowCount)
    if payloadStart + 8 + rowCount * 2 <= payloadEnd {
      var validPerRowCounts = true
      var parsedCounts: [Int] = []
      parsedCounts.reserveCapacity(rowCount)
      for rIdx in 0..<rowCount {
        let c = Int(try BitPacking.readUInt16LE(from: section0, at: payloadStart + 8 + rIdx * 2))
        if c > 0 && c <= maxHWP5TableColumns {
          parsedCounts.append(c)
        } else if c == 0 && colCount == 0 {
          parsedCounts.append(0)
        } else {
          validPerRowCounts = false
          break
        }
      }
      if validPerRowCounts && !parsedCounts.isEmpty {
        rowLengths = parsedCounts
      }
    }

    let expectedTotalCells = rowLengths.reduce(0, +)
    totalDecodedTableCells += expectedTotalCells
    guard totalDecodedTableCells <= maxTotalHWP5TableCells else {
      throw EasyOfficeError.corruptedContainer(
        detail: "HWP 5.0 누적 표 셀 개수(\(totalDecodedTableCells))가 허용 상한(\(maxTotalHWP5TableCells))을 초과했습니다."
      )
    }

    var collectedCells: [String] = []
    collectedCells.reserveCapacity(expectedTotalCells)

    while collectedCells.count < expectedTotalCells && nextOffset < section0.count {
      let (peekHeader, peekHeaderLen) = try HWP5RecordHeader.decode(from: section0, at: nextOffset)
      let pStart = nextOffset + peekHeaderLen
      let pLen = Int(peekHeader.payloadSize)
      let pEnd = pStart + pLen
      guard pLen >= 0, pEnd <= section0.count else {
        throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 셀 하위 레코드가 손상되었습니다.")
      }

      if peekHeader.tagID == HWP5RecordHeader.tagListHeaderStandard
        || peekHeader.tagID == HWP5RecordHeader.tagListHeader
      {
        totalDecodedRecords += 1
        guard totalDecodedRecords <= maxHWP5Records else {
          throw EasyOfficeError.securityViolation(
            detail: "HWP 5.0 레코드 개수가 허용 상한(\(maxHWP5Records))을 초과했습니다.")
        }
        nextOffset = pEnd
        var cellText = ""
        if nextOffset < section0.count {
          let (subHeader, subLen) = try HWP5RecordHeader.decode(from: section0, at: nextOffset)
          let subStart = nextOffset + subLen
          let subEnd = subStart + Int(subHeader.payloadSize)
          if subEnd <= section0.count,
            subHeader.tagID == HWP5RecordHeader.tagParaHeaderStandard
              || subHeader.tagID == HWP5RecordHeader.tagParaHeader
          {
            totalDecodedRecords += 1
            nextOffset = subEnd
          }
        }
        if nextOffset < section0.count {
          let (txtHeader, txtLen) = try HWP5RecordHeader.decode(from: section0, at: nextOffset)
          let txtStart = nextOffset + txtLen
          let txtPayloadLen = Int(txtHeader.payloadSize)
          let txtEnd = txtStart + txtPayloadLen
          if txtEnd <= section0.count,
            txtHeader.tagID == HWP5RecordHeader.tagParaTextStandard
              || txtHeader.tagID == HWP5RecordHeader.tagParaText
          {
            totalDecodedRecords += 1
            cellText = try decodeParaTextPayload(
              from: section0, at: txtStart, byteLength: txtPayloadLen)
            nextOffset = txtEnd
          }
        }
        collectedCells.append(cellText)
      } else if peekHeader.tagID == HWP5RecordHeader.tagParaTextStandard
        || peekHeader.tagID == HWP5RecordHeader.tagParaText
      {
        totalDecodedRecords += 1
        let cellText = try decodeParaTextPayload(from: section0, at: pStart, byteLength: pLen)
        collectedCells.append(cellText)
        nextOffset = pEnd
      } else {
        break
      }
    }

    var rows: [[String]] = []
    rows.reserveCapacity(rowCount)
    var cellCursor = 0
    for rIdx in 0..<rowCount {
      let count = rowLengths[rIdx]
      var row: [String] = []
      row.reserveCapacity(count)
      for _ in 0..<count {
        if cellCursor < collectedCells.count {
          row.append(collectedCells[cellCursor])
          cellCursor += 1
        } else {
          row.append("")
        }
      }
      rows.append(row)
    }

    return DocumentTableBlock(caption: "", rows: rows, hasHeaderRow: hasHeader)
  }

  private static func decodeLegacyTableRecord(
    from section0: Data,
    payloadStart: Int,
    payloadEnd: Int,
    totalDecodedTableCells: inout Int
  ) throws -> DocumentTableBlock {
    let savedCellTotal = totalDecodedTableCells
    var decodedTable: DocumentTableBlock?
    var budgetExceededError: EasyOfficeError?
    do {
      var tCursor = payloadStart
      let caption = try BitPacking.readLengthPrefixedUTF16LE(
        from: section0, offset: &tCursor, limit: payloadEnd)
      guard tCursor + 3 <= payloadEnd else {
        throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 레코드 헤더 길이가 부족합니다.")
      }
      let hasHeader = section0[section0.startIndex + tCursor] != 0
      tCursor += 1
      let rowCount = Int(try BitPacking.readUInt16LE(from: section0, at: tCursor))
      tCursor += 2
      guard rowCount <= maxHWPTableRows, rowCount <= (payloadEnd - tCursor) / 2 else {
        throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 행 개수가 허용 범위 또는 페이로드 길이를 초과합니다.")
      }
      var rows: [[String]] = []
      rows.reserveCapacity(rowCount)
      for _ in 0..<rowCount {
        guard tCursor + 2 <= payloadEnd else {
          throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 행 헤더 길이가 부족합니다.")
        }
        let colCount = Int(try BitPacking.readUInt16LE(from: section0, at: tCursor))
        tCursor += 2
        guard colCount <= maxHWP5TableColumns, colCount * 4 <= payloadEnd - tCursor else {
          throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 열 개수가 허용 범위 또는 페이로드 길이를 초과합니다.")
        }
        totalDecodedTableCells += colCount
        guard totalDecodedTableCells <= maxTotalHWP5TableCells else {
          let err = EasyOfficeError.corruptedContainer(
            detail: "HWP 5.0 누적 표 셀 개수(\(totalDecodedTableCells))가 허용 상한(\(maxTotalHWP5TableCells))을 초과했습니다."
          )
          budgetExceededError = err
          throw err
        }
        var rowCells: [String] = []
        rowCells.reserveCapacity(colCount)
        for _ in 0..<colCount {
          let cell = try BitPacking.readLengthPrefixedUTF16LE(
            from: section0, offset: &tCursor, limit: payloadEnd)
          rowCells.append(cell)
        }
        rows.append(rowCells)
      }
      guard tCursor == payloadEnd else {
        throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 레코드 잔여 바이트 불일치")
      }
      decodedTable = DocumentTableBlock(caption: caption, rows: rows, hasHeaderRow: hasHeader)
    } catch {
      if let budgetErr = budgetExceededError {
        throw budgetErr
      }
      totalDecodedTableCells = savedCellTotal
    }
    if let table = decodedTable {
      return table
    }

    var rCursor = payloadStart
    guard rCursor + 2 <= payloadEnd else {
      throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 레코드 헤더 길이가 부족합니다.")
    }
    let rowCount = Int(try BitPacking.readUInt16LE(from: section0, at: rCursor))
    rCursor += 2
    guard rowCount <= maxHWPTableRows, rowCount <= (payloadEnd - rCursor) / 2 else {
      throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 행 개수가 허용 범위 또는 페이로드 길이를 초과합니다.")
    }
    var rows: [[String]] = []
    rows.reserveCapacity(rowCount)
    for _ in 0..<rowCount {
      guard rCursor + 2 <= payloadEnd else {
        throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 행 헤더 길이가 부족합니다.")
      }
      let colCount = Int(try BitPacking.readUInt16LE(from: section0, at: rCursor))
      rCursor += 2
      guard colCount <= maxHWP5TableColumns, colCount * 4 <= payloadEnd - rCursor else {
        throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 열 개수가 허용 범위 또는 페이로드 길이를 초과합니다.")
      }
      totalDecodedTableCells += colCount
      guard totalDecodedTableCells <= maxTotalHWP5TableCells else {
        throw EasyOfficeError.corruptedContainer(
          detail: "HWP 5.0 누적 표 셀 개수(\(totalDecodedTableCells))가 허용 상한(\(maxTotalHWP5TableCells))을 초과했습니다."
        )
      }
      var rowCells: [String] = []
      rowCells.reserveCapacity(colCount)
      for _ in 0..<colCount {
        let cell = try BitPacking.readLengthPrefixedUTF16LE(
          from: section0, offset: &rCursor, limit: payloadEnd)
        rowCells.append(cell)
      }
      rows.append(rowCells)
    }
    return DocumentTableBlock(caption: "", rows: rows, hasHeaderRow: true)
  }

  private static func decodeParaTextPayload(
    from data: Data,
    at offset: Int,
    byteLength: Int
  ) throws -> String {
    guard byteLength >= 0, byteLength % 2 == 0, offset >= 0, offset + byteLength <= data.count else {
      throw EasyOfficeError.corruptedContainer(
        detail: "HWP 5.0 문단 텍스트 페이로드 길이가 2바이트(WCHAR) 배수가 아니거나 범위를 초과합니다.")
    }
    let payloadEnd = offset + byteLength
    var units: [UInt16] = []
    units.reserveCapacity(byteLength / 2)

    data.withUnsafeBytes { rawBuffer in
      var cursor = offset
      while cursor < payloadEnd {
        let code = rawBuffer.loadUnaligned(fromByteOffset: cursor, as: UInt16.self).littleEndian
        if code < 0x0020 {
          if code == 0x0009 {
            units.append(0x0009)
            if cursor + 16 <= payloadEnd,
              rawBuffer.loadUnaligned(fromByteOffset: cursor + 14, as: UInt16.self).littleEndian
                == 0x0009
            {
              var allZeroAttrs = true
              for attrOff in stride(from: 2, through: 12, by: 2) {
                if rawBuffer.loadUnaligned(fromByteOffset: cursor + attrOff, as: UInt16.self)
                  .littleEndian != 0
                {
                  allZeroAttrs = false
                  break
                }
              }
              cursor += allZeroAttrs ? 16 : 2
            } else {
              cursor += 2
            }
          } else if code == 0x000A {
            units.append(0x000A)
            cursor += 2
          } else if code == 0x000D {
            if cursor + 2 < payloadEnd {
              units.append(0x000D)
            }
            cursor += 2
          } else if code == 0x0000 {
            cursor += 2
          } else if code <= 0x0017, cursor + 16 <= payloadEnd,
            rawBuffer.loadUnaligned(fromByteOffset: cursor + 14, as: UInt16.self).littleEndian == code
          {
            cursor += 16
          } else {
            cursor += 2
          }
        } else {
          units.append(code)
          cursor += 2
        }
      }
    }

    return String(decoding: units, as: UTF16.self)
  }
}
