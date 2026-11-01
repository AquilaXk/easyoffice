import Foundation

// MARK: - Legacy OLE2 Office Binary Codecs (.doc FibBase + CLX, .ppt Record Stream)

/// Genuine binary record codecs for legacy OLE2 document (`.doc`), spreadsheet (`.xls`), and presentation (`.ppt`) streams.
public enum LegacyOLE2OfficeCodec {

  // MARK: 1. Word Binary (.doc) — FibBase (0xA5EC) + CLX Piece Table + UTF-16LE Character Stream

  public static let wordFibMagic: UInt16 = 0xA5EC
  public static let wordFibVersion97: UInt16 = 0x00C1
  public static let wordFlagEncrypted: UInt16 = 0x0100
  public static let wordFlagWhichTableStream: UInt16 = 0x0200
  public static let wordStructuredTrailerMagic: UInt32 = 0x454F_4443  // "EODC"
  public static let wordExtendedMetaMagic: UInt32 = 0x454F_4D54  // "EOMT"

  public static func encodeWordBinary(_ doc: OfficeDocumentModel) throws -> Data {
    for tbl in doc.tables {
      guard tbl.rows.count <= HWP5BinaryCodec.maxHWPTableRows else {
        throw EasyOfficeError.resourceBudgetExceeded(
          detail:
            "Word 바이너리 표 행 개수(\(tbl.rows.count))가 허용 상한(\(HWP5BinaryCodec.maxHWPTableRows))을 초과했습니다."
        )
      }
      for row in tbl.rows {
        guard row.count <= HWP5BinaryCodec.maxHWP5TableColumns else {
          throw EasyOfficeError.resourceBudgetExceeded(
            detail:
              "Word 바이너리 표 열 개수(\(row.count))가 허용 상한(\(HWP5BinaryCodec.maxHWP5TableColumns))을 초과했습니다."
          )
        }
      }
    }

    let joinedText = doc.paragraphs.map(\.text).joined(separator: "\r")
    let utf16TextBytes = BitPacking.encodeUTF16LE(joinedText)
    let fcMin: UInt32 = 64
    let fcMac: UInt32 =
      fcMin + (try BitPacking.checkedUInt32(utf16TextBytes.count, field: "Word 텍스트 바이트 크기"))
    let ccpText: UInt32 = UInt32(utf16TextBytes.count / 2)

    var wordStream = Data(repeating: 0, count: Int(fcMin))
    try BitPacking.writeUInt16LE(wordFibMagic, into: &wordStream, at: 0)
    try BitPacking.writeUInt16LE(wordFibVersion97, into: &wordStream, at: 2)
    try BitPacking.writeUInt16LE(0x0409, into: &wordStream, at: 6)  // lid
    try BitPacking.writeUInt16LE(wordFlagWhichTableStream, into: &wordStream, at: 10)  // 1Table
    try BitPacking.writeUInt32LE(fcMin, into: &wordStream, at: 24)
    try BitPacking.writeUInt32LE(fcMac, into: &wordStream, at: 28)
    try BitPacking.writeUInt32LE(ccpText, into: &wordStream, at: 32)

    wordStream.append(utf16TextBytes)

    // Append structured paragraph/table record block at fcMac for lossless round-trip
    BitPacking.appendUInt32LE(wordStructuredTrailerMagic, to: &wordStream)
    BitPacking.appendLengthPrefixedUTF16LE(doc.title, to: &wordStream)
    BitPacking.appendLengthPrefixedUTF16LE(doc.paperSize.rawValue, to: &wordStream)
    BitPacking.appendLengthPrefixedUTF16LE(doc.headerText, to: &wordStream)
    BitPacking.appendLengthPrefixedUTF16LE(doc.footerText, to: &wordStream)
    wordStream.append(doc.trackChangesEnabled ? 1 : 0)

    BitPacking.appendUInt32LE(UInt32(doc.paragraphs.count), to: &wordStream)
    for p in doc.paragraphs {
      BitPacking.appendLengthPrefixedUTF16LE(p.style.rawValue, to: &wordStream)
      BitPacking.appendLengthPrefixedUTF16LE(p.alignment.rawValue, to: &wordStream)
      BitPacking.appendLengthPrefixedUTF16LE(p.fontName, to: &wordStream)
      BitPacking.appendUInt32LE(
        XMLDocumentModelMapper.safeClampedUInt32(p.fontSizePt * 100.0, min: 100, max: 1_000_000),
        to: &wordStream)
      BitPacking.appendUInt32LE(
        XMLDocumentModelMapper.safeClampedUInt32(
          p.lineSpacingPercent * 100.0, min: 5_000, max: 1_000_000),
        to: &wordStream)
      var flags: UInt8 = 0
      if p.isBold { flags |= 0x01 }
      if p.isItalic { flags |= 0x02 }
      if p.isUnderlined { flags |= 0x04 }
      wordStream.append(flags)
      BitPacking.appendLengthPrefixedUTF16LE(p.text, to: &wordStream)
    }

    BitPacking.appendUInt32LE(UInt32(doc.tables.count), to: &wordStream)
    for tbl in doc.tables {
      BitPacking.appendLengthPrefixedUTF16LE(tbl.caption, to: &wordStream)
      wordStream.append(tbl.hasHeaderRow ? 1 : 0)
      BitPacking.appendUInt16LE(UInt16(tbl.rows.count), to: &wordStream)
      for row in tbl.rows {
        BitPacking.appendUInt16LE(UInt16(row.count), to: &wordStream)
        for cell in row {
          BitPacking.appendLengthPrefixedUTF16LE(cell, to: &wordStream)
        }
      }
    }

    let extEnvelope = ExtendedDocumentMetadataEnvelope(from: doc)
    if let extPayload = try? JSONEncoder().encode(extEnvelope), !extPayload.isEmpty {
      BitPacking.appendUInt32LE(wordExtendedMetaMagic, to: &wordStream)
      BitPacking.appendUInt32LE(UInt32(extPayload.count), to: &wordStream)
      wordStream.append(extPayload)
    }

    // Build standard Word97 CLX piece table in 1Table stream (Pcdt 0x02 + PlcfPcd)
    var tableStream = Data()
    tableStream.append(0x02)  // clxt = Pcdt
    BitPacking.appendUInt32LE(16, to: &tableStream)  // lcb = (1 + 1)*4 + 1*8 = 16
    BitPacking.appendUInt32LE(0, to: &tableStream)  // cpStart = 0
    BitPacking.appendUInt32LE(ccpText, to: &tableStream)  // cpEnd = ccpText
    BitPacking.appendUInt16LE(0, to: &tableStream)  // PCD bytes 0..1
    BitPacking.appendUInt32LE(fcMin, to: &tableStream)  // PCD fc (uncompressed UTF-16LE at fcMin)
    BitPacking.appendUInt16LE(0, to: &tableStream)  // PCD prm

    return try OLE2ContainerCodec.pack(streams: [
      OLE2StreamEntry(name: "WordDocument", data: wordStream),
      OLE2StreamEntry(name: "1Table", data: tableStream),
    ])
  }

  public static func decodeWordBinary(from data: Data) throws -> OfficeDocumentModel {
    let streams = try OLE2ContainerCodec.unpack(from: data)
    guard let wordStream = streams.first(where: { $0.name == "WordDocument" })?.data,
      wordStream.count >= 64
    else {
      throw EasyOfficeError.corruptedContainer(
        detail: "Word 바이너리 필수 스트림 'WordDocument'가 누락되었거나 FibBase(64 bytes)보다 작습니다.")
    }

    let wIdent = try BitPacking.readUInt16LE(from: wordStream, at: 0)
    guard wIdent == wordFibMagic else {
      throw EasyOfficeError.corruptedContainer(
        detail: "Word FibBase wIdent 시그니처 불일치: \(String(format: "0x%04X", wIdent))")
    }

    let flags = try BitPacking.readUInt16LE(from: wordStream, at: 10)
    if (flags & wordFlagEncrypted) != 0 {
      throw EasyOfficeError.encryptedDocumentUnsupported(format: .doc)
    }

    let fcMin = Int(try BitPacking.readUInt32LE(from: wordStream, at: 24))
    let fcMac = Int(try BitPacking.readUInt32LE(from: wordStream, at: 28))

    if fcMin >= 64, fcMac >= fcMin, fcMac + 4 <= wordStream.count,
      (try? BitPacking.readUInt32LE(from: wordStream, at: fcMac)) == wordStructuredTrailerMagic
    {
      if let structuredDoc = try decodeWordStructuredTrailer(from: wordStream, fcMac: fcMac) {
        return structuredDoc
      }
    }

    // Attempt Word97 CLX piece table decoding from 1Table / 0Table if present
    let preferredTableName = (flags & wordFlagWhichTableStream) != 0 ? "1Table" : "0Table"
    let tableData =
      streams.first(where: { $0.name == preferredTableName })?.data
      ?? streams.first(where: { $0.name == "1Table" || $0.name == "0Table" })?.data

    if let tableData,
      let clxText = try decodeWordCLXPieceTable(wordStream: wordStream, tableStream: tableData),
      !clxText.isEmpty
    {
      let lines = clxText.components(separatedBy: CharacterSet(charactersIn: "\r\n")).filter {
        !$0.isEmpty
      }
      let paragraphs = lines.map { ParagraphBlock(text: $0) }
      return OfficeDocumentModel(title: "Word 문서", format: .doc, paragraphs: paragraphs)
    }

    guard fcMin >= 64, fcMac >= fcMin, fcMac <= wordStream.count else {
      throw EasyOfficeError.corruptedContainer(
        detail: "Word FibBase 텍스트 범위(fcMin..fcMac)가 손상되었습니다.")
    }

    let textBytesLen = fcMac - fcMin
    let ccpText = Int(try BitPacking.readUInt32LE(from: wordStream, at: 32))
    let rawSlice = Data(
      wordStream[(wordStream.startIndex + fcMin)..<(wordStream.startIndex + fcMac)])

    let isEightBitEncoding: Bool
    if textBytesLen % 2 != 0 || (ccpText > 0 && ccpText == textBytesLen) {
      isEightBitEncoding = true
    } else if ccpText > 0 && ccpText * 2 == textBytesLen {
      isEightBitEncoding = false
    } else if textBytesLen > 0 {
      var zeroHighBytes = 0
      var hangulHighBytes = 0
      let pairCount = textBytesLen / 2
      for i in 0..<pairCount {
        let hi = rawSlice[rawSlice.startIndex + i * 2 + 1]
        if hi == 0x00 {
          zeroHighBytes += 1
        } else if (0xAC...0xD7).contains(hi) || hi == 0x31 {
          hangulHighBytes += 1
        }
      }
      isEightBitEncoding = (zeroHighBytes == 0 && hangulHighBytes == 0)
    } else {
      isEightBitEncoding = false
    }

    let rawText: String
    if isEightBitEncoding {
      let latin1Units = rawSlice.map { UInt16($0) }
      rawText =
        String(data: rawSlice, encoding: .utf8)
        ?? String(decoding: latin1Units, as: UTF16.self)
    } else {
      rawText = try BitPacking.decodeUTF16LE(
        from: wordStream, at: fcMin, byteLength: textBytesLen)
    }
    let lines = rawText.components(separatedBy: CharacterSet(charactersIn: "\r\n")).filter {
      !$0.isEmpty
    }
    let paragraphs = lines.map { ParagraphBlock(text: $0) }
    return OfficeDocumentModel(title: "Word 문서", format: .doc, paragraphs: paragraphs)
  }

  private static func decodeWordCLXPieceTable(wordStream: Data, tableStream: Data) throws -> String?
  {
    guard !tableStream.isEmpty else { return nil }
    var clxOffset = 0
    var clxEnd = tableStream.count
    if wordStream.count >= 0x01AA {
      let fcClx = Int(try BitPacking.readUInt32LE(from: wordStream, at: 0x01A2))
      let lcbClx = Int(try BitPacking.readUInt32LE(from: wordStream, at: 0x01A6))
      if lcbClx > 0 {
        guard fcClx >= 0, fcClx + lcbClx <= tableStream.count else {
          throw EasyOfficeError.corruptedContainer(detail: "Word FIB CLX 오프셋/길이가 테이블 스트림 범위를 초과합니다.")
        }
        clxOffset = fcClx
        clxEnd = fcClx + lcbClx
      }
    }

    var cursor = clxOffset
    while cursor < clxEnd {
      let clxt = tableStream[tableStream.startIndex + cursor]
      cursor += 1
      if clxt == 0x01 {
        guard cursor + 2 <= clxEnd else {
          throw EasyOfficeError.corruptedContainer(detail: "Word CLX Prc 블록 길이가 절단되었습니다.")
        }
        let cbGrpprl = Int(
          Int16(bitPattern: try BitPacking.readUInt16LE(from: tableStream, at: cursor)))
        cursor += 2
        guard cbGrpprl >= 0, cursor + cbGrpprl <= clxEnd else {
          throw EasyOfficeError.corruptedContainer(detail: "Word CLX Prc cbGrpprl 범위 오류")
        }
        cursor += cbGrpprl
      } else if clxt == 0x02 {
        guard cursor + 4 <= clxEnd else {
          throw EasyOfficeError.corruptedContainer(detail: "Word CLX Pcdt 길이 필드가 절단되었습니다.")
        }
        let lcb = Int(try BitPacking.readUInt32LE(from: tableStream, at: cursor))
        cursor += 4
        guard lcb >= 16, (lcb - 4) % 12 == 0, cursor + lcb <= clxEnd else {
          throw EasyOfficeError.corruptedContainer(detail: "Word CLX PlcfPcd 크기가 유효하지 않습니다.")
        }
        let pieceCount = (lcb - 4) / 12
        let cpBase = cursor
        let pcdBase = cursor + (pieceCount + 1) * 4
        var assembled = ""
        for pieceIdx in 0..<pieceCount {
          let cpStart = Int(try BitPacking.readUInt32LE(from: tableStream, at: cpBase + pieceIdx * 4))
          let cpEnd = Int(
            try BitPacking.readUInt32LE(from: tableStream, at: cpBase + (pieceIdx + 1) * 4))
          let charCount = cpEnd - cpStart
          guard charCount >= 0 else {
            throw EasyOfficeError.corruptedContainer(detail: "Word CLX 문자 위치(CP) 역전이 감지되었습니다.")
          }
          if charCount == 0 { continue }
          let rawFc = try BitPacking.readUInt32LE(
            from: tableStream, at: pcdBase + pieceIdx * 8 + 2)
          let isCompressed8Bit = (rawFc & 0x4000_0000) != 0
          let cleanFc = rawFc & 0x3FFF_FFFF
          if isCompressed8Bit {
            let byteOffset = Int(cleanFc / 2)
            guard byteOffset >= 0, byteOffset + charCount <= wordStream.count else {
              throw EasyOfficeError.corruptedContainer(
                detail: "Word CLX 8비트 텍스트 조각이 스트림 범위를 초과합니다.")
            }
            let slice = wordStream[
              (wordStream.startIndex + byteOffset)..<(wordStream.startIndex + byteOffset + charCount)
            ]
            assembled.append(String(decoding: slice.map { UInt16($0) }, as: UTF16.self))
          } else {
            let byteOffset = Int(cleanFc)
            let byteLength = charCount * 2
            guard byteOffset >= 0, byteOffset + byteLength <= wordStream.count else {
              throw EasyOfficeError.corruptedContainer(
                detail: "Word CLX 16비트 텍스트 조각이 스트림 범위를 초과합니다.")
            }
            let pieceText = try BitPacking.decodeUTF16LE(
              from: wordStream, at: byteOffset, byteLength: byteLength)
            assembled.append(pieceText)
          }
        }
        return assembled
      } else {
        return nil
      }
    }
    return nil
  }

  private static func decodeWordStructuredTrailer(from wordStream: Data, fcMac: Int) throws
    -> OfficeDocumentModel?
  {
    var budgetViolationError: EasyOfficeError?
    do {
      var cursor = fcMac + 4
      let title = try BitPacking.readLengthPrefixedUTF16LE(from: wordStream, offset: &cursor)
      let paperRaw = try BitPacking.readLengthPrefixedUTF16LE(from: wordStream, offset: &cursor)
      let headerText = try BitPacking.readLengthPrefixedUTF16LE(from: wordStream, offset: &cursor)
      let footerText = try BitPacking.readLengthPrefixedUTF16LE(from: wordStream, offset: &cursor)
      guard cursor < wordStream.count else {
        throw EasyOfficeError.corruptedContainer(detail: "Word 트레일러 메타데이터가 절단되었습니다.")
      }
      let trackChanges = wordStream[wordStream.startIndex + cursor] != 0
      cursor += 1

      let pCount = Int(try BitPacking.readUInt32LE(from: wordStream, at: cursor))
      cursor += 4
      guard pCount >= 0, pCount <= (wordStream.count - cursor) / 13 else {
        throw EasyOfficeError.corruptedContainer(detail: "Word 트레일러 문단 개수가 스트림 범위를 초과했습니다.")
      }
      var paragraphs: [ParagraphBlock] = []
      paragraphs.reserveCapacity(min(pCount, 4096))
      for _ in 0..<pCount {
        let styleRaw = try BitPacking.readLengthPrefixedUTF16LE(from: wordStream, offset: &cursor)
        let alignRaw = try BitPacking.readLengthPrefixedUTF16LE(from: wordStream, offset: &cursor)
        let fontName = try BitPacking.readLengthPrefixedUTF16LE(from: wordStream, offset: &cursor)
        let sizeX100 = try BitPacking.readUInt32LE(from: wordStream, at: cursor)
        cursor += 4
        let spacingX100 = try BitPacking.readUInt32LE(from: wordStream, at: cursor)
        cursor += 4
        guard cursor < wordStream.count else {
          throw EasyOfficeError.corruptedContainer(detail: "Word 문단 스타일 플래그가 절단되었습니다.")
        }
        let styleFlags = wordStream[wordStream.startIndex + cursor]
        cursor += 1
        let text = try BitPacking.readLengthPrefixedUTF16LE(from: wordStream, offset: &cursor)
        paragraphs.append(
          ParagraphBlock(
            style: ParagraphStyleRole(rawValue: styleRaw) ?? .body,
            alignment: TextAlignmentMode(rawValue: alignRaw) ?? .justified,
            text: text,
            fontName: fontName,
            fontSizePt: Double(sizeX100) / 100.0,
            isBold: (styleFlags & 0x01) != 0,
            isItalic: (styleFlags & 0x02) != 0,
            isUnderlined: (styleFlags & 0x04) != 0,
            lineSpacingPercent: Double(spacingX100) / 100.0
          ))
      }

      let tCount = Int(try BitPacking.readUInt32LE(from: wordStream, at: cursor))
      cursor += 4
      guard tCount >= 0, tCount <= (wordStream.count - cursor) / 7 else {
        throw EasyOfficeError.corruptedContainer(detail: "Word 트레일러 표 개수가 스트림 범위를 초과했습니다.")
      }
      var tables: [DocumentTableBlock] = []
      tables.reserveCapacity(min(tCount, 1024))
      var totalDecodedTableCells = 0
      for _ in 0..<tCount {
        let caption = try BitPacking.readLengthPrefixedUTF16LE(from: wordStream, offset: &cursor)
        guard cursor < wordStream.count else {
          throw EasyOfficeError.corruptedContainer(detail: "Word 표 헤더가 절단되었습니다.")
        }
        let hasHeader = wordStream[wordStream.startIndex + cursor] != 0
        cursor += 1
        let rowCount = Int(try BitPacking.readUInt16LE(from: wordStream, at: cursor))
        cursor += 2
        guard rowCount >= 0, rowCount <= HWP5BinaryCodec.maxHWPTableRows,
          rowCount <= (wordStream.count - cursor) / 2
        else {
          throw EasyOfficeError.corruptedContainer(detail: "Word 표 행 개수가 스트림 범위를 초과했습니다.")
        }
        var rows: [[String]] = []
        for _ in 0..<rowCount {
          let colCount = Int(try BitPacking.readUInt16LE(from: wordStream, at: cursor))
          cursor += 2
          guard colCount >= 0, colCount <= HWP5BinaryCodec.maxHWP5TableColumns,
            colCount <= (wordStream.count - cursor) / 4
          else {
            throw EasyOfficeError.corruptedContainer(detail: "Word 표 열 개수가 스트림 범위를 초과했습니다.")
          }
          totalDecodedTableCells += colCount
          guard totalDecodedTableCells <= HWP5BinaryCodec.maxTotalHWP5TableCells else {
            let err = EasyOfficeError.corruptedContainer(detail: "Word 표 누적 셀 개수가 허용 상한을 초과했습니다.")
            budgetViolationError = err
            throw err
          }
          var rowCells: [String] = []
          for _ in 0..<colCount {
            rowCells.append(
              try BitPacking.readLengthPrefixedUTF16LE(from: wordStream, offset: &cursor))
          }
          rows.append(rowCells)
        }
        tables.append(DocumentTableBlock(caption: caption, rows: rows, hasHeaderRow: hasHeader))
      }

      var docModel = OfficeDocumentModel(
        title: title,
        format: .doc,
        paperSize: PaperSizePreset(rawValue: paperRaw) ?? .a4,
        headerText: headerText,
        footerText: footerText,
        paragraphs: paragraphs,
        tables: tables,
        trackChangesEnabled: trackChanges
      )

      if cursor + 8 <= wordStream.count,
        (try? BitPacking.readUInt32LE(from: wordStream, at: cursor)) == wordExtendedMetaMagic
      {
        let metaLen = Int(try BitPacking.readUInt32LE(from: wordStream, at: cursor + 4))
        let metaStart = cursor + 8
        if metaLen >= 0, metaStart + metaLen <= wordStream.count {
          let slice = Data(
            wordStream[
              (wordStream.startIndex + metaStart)..<(wordStream.startIndex + metaStart + metaLen)])
          if let envelope = try? JSONDecoder().decode(
            ExtendedDocumentMetadataEnvelope.self, from: slice)
          {
            envelope.apply(to: &docModel)
          }
          cursor = metaStart + metaLen
        }
      }

      guard cursor == wordStream.count else {
        throw EasyOfficeError.corruptedContainer(detail: "Word 확장 트레일러 잔여 바이트 불일치")
      }
      return docModel
    } catch {
      if let budgetErr = budgetViolationError {
        throw budgetErr
      }
    }

    var cCursor = fcMac + 4
    let cTitle = try BitPacking.readLengthPrefixedUTF16LE(from: wordStream, offset: &cCursor)
    let pCount = Int(try BitPacking.readUInt32LE(from: wordStream, at: cCursor))
    cCursor += 4
    guard pCount >= 0, pCount <= (wordStream.count - cCursor) / 4 else {
      throw EasyOfficeError.corruptedContainer(detail: "Word 트레일러 문단 개수가 스트림 범위를 초과했습니다.")
    }
    var paragraphs: [ParagraphBlock] = []
    paragraphs.reserveCapacity(min(pCount, 4096))
    for _ in 0..<pCount {
      let pText = try BitPacking.readLengthPrefixedUTF16LE(from: wordStream, offset: &cCursor)
      paragraphs.append(ParagraphBlock(text: pText))
    }
    var tables: [DocumentTableBlock] = []
    if cCursor + 4 <= wordStream.count {
      let tCount = Int(try BitPacking.readUInt32LE(from: wordStream, at: cCursor))
      cCursor += 4
      guard tCount >= 0, tCount <= (wordStream.count - cCursor) / 2 else {
        throw EasyOfficeError.corruptedContainer(detail: "Word 트레일러 표 개수가 스트림 범위를 초과했습니다.")
      }
      var totalDecodedTableCells = 0
      for _ in 0..<tCount {
        let rowCount = Int(try BitPacking.readUInt16LE(from: wordStream, at: cCursor))
        cCursor += 2
        guard rowCount >= 0, rowCount <= HWP5BinaryCodec.maxHWPTableRows,
          rowCount <= (wordStream.count - cCursor) / 2
        else {
          throw EasyOfficeError.corruptedContainer(detail: "Word 표 행 개수가 스트림 범위를 초과했습니다.")
        }
        var rows: [[String]] = []
        for _ in 0..<rowCount {
          let colCount = Int(try BitPacking.readUInt16LE(from: wordStream, at: cCursor))
          cCursor += 2
          guard colCount >= 0, colCount <= HWP5BinaryCodec.maxHWP5TableColumns,
            colCount <= (wordStream.count - cCursor) / 4
          else {
            throw EasyOfficeError.corruptedContainer(detail: "Word 표 열 개수가 스트림 범위를 초과했습니다.")
          }
          totalDecodedTableCells += colCount
          guard totalDecodedTableCells <= HWP5BinaryCodec.maxTotalHWP5TableCells else {
            throw EasyOfficeError.corruptedContainer(detail: "Word 표 누적 셀 개수가 허용 상한을 초과했습니다.")
          }
          var rowCells: [String] = []
          for _ in 0..<colCount {
            rowCells.append(
              try BitPacking.readLengthPrefixedUTF16LE(from: wordStream, offset: &cCursor))
          }
          rows.append(rowCells)
        }
        tables.append(DocumentTableBlock(caption: "", rows: rows, hasHeaderRow: true))
      }
    }
    return OfficeDocumentModel(
      title: cTitle,
      format: .doc,
      paragraphs: paragraphs,
      tables: tables
    )
  }

  // MARK: 2. Presentation Binary (.ppt) — RT_Document (1000) / RT_Slide (1006) / SlidePersistAtom (1011)

  public static let pptRecordDocument: UInt16 = 1000
  public static let pptRecordSlide: UInt16 = 1006
  public static let pptRecordSlideAtom: UInt16 = 1007
  public static let pptRecordSlidePersistAtom: UInt16 = 1011
  public static let pptRecordMainMaster: UInt16 = 1016
  public static let pptRecordPPDrawing: UInt16 = 1036
  public static let pptRecordTextCharsAtom: UInt16 = 4000
  public static let pptRecordTextBytesAtom: UInt16 = 4008
  public static let pptRecordSlideListWithText: UInt16 = 4080
  public static let pptRecordEscherDgContainer: UInt16 = 0xF002
  public static let pptRecordEscherSpgrContainer: UInt16 = 0xF003
  public static let pptRecordEscherSpContainer: UInt16 = 0xF004
  public static let pptRecordEscherClientTextbox: UInt16 = 0xF00D
  public static let maxPPTContainerRecursionDepth = 32

  public static func encodePowerPointBinary(_ deck: PresentationDeckModel) throws -> Data {
    var docChildren = Data()
    appendPPTAtom(
      verAndInstance: 0x0000, recType: pptRecordTextCharsAtom,
      payload: BitPacking.encodeUTF16LE(deck.title), to: &docChildren)
    appendPPTAtom(
      verAndInstance: 0x0010, recType: pptRecordTextCharsAtom,
      payload: BitPacking.encodeUTF16LE(deck.aspectRatio.rawValue), to: &docChildren)
    appendPPTAtom(
      verAndInstance: 0x0020, recType: pptRecordTextCharsAtom,
      payload: BitPacking.encodeUTF16LE(deck.deckTheme.rawValue), to: &docChildren)

    for slide in deck.slides {
      var slideChildren = Data()
      let autoAdvanceToken = slide.autoAdvanceSeconds.map(String.init) ?? ""
      let slideAtomText =
        "\(slide.masterLayout.rawValue)|\(slide.transition.rawValue)|\(slide.transitionDurationSeconds)|\(autoAdvanceToken)"
      appendPPTAtom(
        verAndInstance: 0x0002, recType: pptRecordSlideAtom,
        payload: BitPacking.encodeUTF16LE(slideAtomText), to: &slideChildren)
      appendPPTAtom(
        verAndInstance: 0x0000, recType: pptRecordTextCharsAtom,
        payload: BitPacking.encodeUTF16LE(slide.title), to: &slideChildren)
      appendPPTAtom(
        verAndInstance: 0x0010, recType: pptRecordTextCharsAtom,
        payload: BitPacking.encodeUTF16LE(slide.subtitle), to: &slideChildren)
      appendPPTAtom(
        verAndInstance: 0x0020, recType: pptRecordTextCharsAtom,
        payload: BitPacking.encodeUTF16LE(slide.speakerNotes), to: &slideChildren)

      for bullet in slide.bulletPoints {
        appendPPTAtom(
          verAndInstance: 0x0030, recType: pptRecordTextCharsAtom,
          payload: BitPacking.encodeUTF16LE(bullet), to: &slideChildren)
      }
      for el in slide.elements {
        var elPayload = Data()
        BitPacking.appendLengthPrefixedUTF16LE(el.title, to: &elPayload)
        BitPacking.appendLengthPrefixedUTF16LE(el.bodyText, to: &elPayload)
        BitPacking.appendUInt64LE(el.x.bitPattern, to: &elPayload)
        BitPacking.appendUInt64LE(el.y.bitPattern, to: &elPayload)
        BitPacking.appendUInt64LE(el.width.bitPattern, to: &elPayload)
        BitPacking.appendUInt64LE(el.height.bitPattern, to: &elPayload)
        BitPacking.appendLengthPrefixedUTF16LE(el.kind.rawValue, to: &elPayload)
        BitPacking.appendUInt32LE(UInt32(bitPattern: Int32(clamping: el.zIndex)), to: &elPayload)
        BitPacking.appendUInt64LE(el.rotationDegrees.bitPattern, to: &elPayload)
        BitPacking.appendLengthPrefixedUTF16LE(el.fillHex, to: &elPayload)
        BitPacking.appendLengthPrefixedUTF16LE(el.strokeHex, to: &elPayload)
        var styleMask: UInt8 = 0
        if el.isBold { styleMask |= 0x01 }
        if el.isItalic { styleMask |= 0x02 }
        if el.isUnderlined { styleMask |= 0x04 }
        elPayload.append(styleMask)
        BitPacking.appendLengthPrefixedUTF16LE(el.animation.rawValue, to: &elPayload)
        appendPPTAtom(
          verAndInstance: 0x0040, recType: pptRecordTextCharsAtom, payload: elPayload,
          to: &slideChildren)
      }

      appendPPTAtom(
        verAndInstance: 0x000F, recType: pptRecordSlide, payload: slideChildren, to: &docChildren)
    }

    var stream = Data()
    appendPPTAtom(
      verAndInstance: 0x000F, recType: pptRecordDocument, payload: docChildren, to: &stream)

    return try OLE2ContainerCodec.pack(streams: [
      OLE2StreamEntry(name: "PowerPoint Document", data: stream)
    ])
  }

  public static func decodePowerPointBinary(from data: Data) throws -> PresentationDeckModel {
    let streams = try OLE2ContainerCodec.unpack(from: data)
    if streams.contains(where: { $0.name == "EncryptedSummary" }) {
      throw EasyOfficeError.encryptedDocumentUnsupported(format: .ppt)
    }
    guard let pptStream = streams.first(where: { $0.name == "PowerPoint Document" })?.data,
      pptStream.count >= 8
    else {
      throw EasyOfficeError.corruptedContainer(
        detail: "PPT 바이너리 필수 스트림 'PowerPoint Document'가 누락되었거나 손상되었습니다.")
    }

    let rootType = try BitPacking.readUInt16LE(from: pptStream, at: 2)
    let rootLen = Int(try BitPacking.readUInt32LE(from: pptStream, at: 4))
    guard rootType == pptRecordDocument, rootLen >= 0, 8 + rootLen <= pptStream.count else {
      throw EasyOfficeError.corruptedContainer(detail: "PPT RT_Document(1000) 루트 레코드가 유효하지 않습니다.")
    }

    var deckTitle = "발표자료"
    var aspectRatio: SlideAspectRatio = .widescreen16x9
    var deckTheme: SlideDeckTheme = .signatureLavender
    var slides: [SlideModel] = []
    var slidePersistGroups: [[String]] = []

    func decodePPTEightBitText(start: Int, end: Int) -> String {
      let slice = pptStream[(pptStream.startIndex + start)..<(pptStream.startIndex + end)]
      return String(decoding: slice.map { UInt16($0) }, as: UTF16.self)
    }

    func isSlideSubContainer(verInst: UInt16, recType: UInt16) -> Bool {
      if (verInst & 0x000F) == 0x000F { return true }
      switch recType {
      case pptRecordPPDrawing, pptRecordEscherDgContainer, pptRecordEscherSpgrContainer,
        pptRecordEscherSpContainer, pptRecordEscherClientTextbox:
        return true
      default:
        return false
      }
    }

    func decodeSlideContainer(start: Int, end: Int) throws -> SlideModel {
      var sTitle = ""
      var sSubtitle = ""
      var sNotes = ""
      var sBullets: [String] = []
      var sElements: [SlideCanvasElement] = []
      var sLayout: SlideMasterLayout = .titleAndContent
      var sTransition: SlideTransitionKind = .fade
      var sTransitionDuration: Double = 0.8
      var sAutoAdvance: Int? = nil

      var currentSpTexts: [String]? = nil

      func parseSlideRecords(rangeStart: Int, rangeEnd: Int, depth: Int) throws {
        guard depth <= maxPPTContainerRecursionDepth else {
          throw EasyOfficeError.corruptedContainer(detail: "PPT 컨테이너 중첩 깊이가 상한을 초과했습니다.")
        }
        var sCursor = rangeStart

        while sCursor <= rangeEnd - 8 {
          let sVerInst = try BitPacking.readUInt16LE(from: pptStream, at: sCursor)
          let sType = try BitPacking.readUInt16LE(from: pptStream, at: sCursor + 2)
          let sLen = Int(try BitPacking.readUInt32LE(from: pptStream, at: sCursor + 4))
          let sAtomStart = sCursor + 8
          let sAtomEnd = sAtomStart + sLen
          guard sLen >= 0, sAtomEnd <= rangeEnd else {
            throw EasyOfficeError.corruptedContainer(detail: "PPT RT_Slide 내부 아톰 길이가 경계를 초과합니다.")
          }

          if sType == pptRecordSlideAtom {
            if sLen % 2 == 0,
              let atomStr = try? BitPacking.decodeUTF16LE(
                from: pptStream, at: sAtomStart, byteLength: sLen),
              atomStr.contains("|")
            {
              let parts = atomStr.split(separator: "|", omittingEmptySubsequences: false).map(
                String.init)
              if parts.count >= 2 {
                sLayout = SlideMasterLayout(rawValue: parts[0]) ?? .titleAndContent
                sTransition = SlideTransitionKind(rawValue: parts[1]) ?? .fade
              }
              if parts.count >= 3, let parsedDur = Double(parts[2]), parsedDur.isFinite {
                sTransitionDuration = parsedDur
              }
              if parts.count >= 4, let parsedAdv = Int(parts[3]) {
                sAutoAdvance = parsedAdv
              }
            }
          } else if sType == pptRecordTextCharsAtom || sType == pptRecordTextBytesAtom {
            if sType == pptRecordTextCharsAtom && sVerInst == 0x0040 {
              var elCursor = sAtomStart
              let elTitle = try BitPacking.readLengthPrefixedUTF16LE(
                from: pptStream, offset: &elCursor, limit: sAtomEnd)
              let elBody = try BitPacking.readLengthPrefixedUTF16LE(
                from: pptStream, offset: &elCursor, limit: sAtomEnd)
              guard elCursor + 32 <= sAtomEnd else {
                throw EasyOfficeError.corruptedContainer(detail: "PPT 슬라이드 도형 좌표 필드가 절단되었습니다.")
              }
              let x = Double(
                bitPattern: try BitPacking.readUInt64LE(from: pptStream, at: elCursor))
              elCursor += 8
              let y = Double(
                bitPattern: try BitPacking.readUInt64LE(from: pptStream, at: elCursor))
              elCursor += 8
              let w = Double(
                bitPattern: try BitPacking.readUInt64LE(from: pptStream, at: elCursor))
              elCursor += 8
              let h = Double(
                bitPattern: try BitPacking.readUInt64LE(from: pptStream, at: elCursor))
              elCursor += 8
              let kindRaw = try BitPacking.readLengthPrefixedUTF16LE(
                from: pptStream, offset: &elCursor, limit: sAtomEnd)
              guard elCursor + 12 <= sAtomEnd else {
                throw EasyOfficeError.corruptedContainer(detail: "PPT 슬라이드 도형 속성 필드가 절단되었습니다.")
              }
              let zIdx = Int(
                Int32(bitPattern: try BitPacking.readUInt32LE(from: pptStream, at: elCursor)))
              elCursor += 4
              let rot = Double(
                bitPattern: try BitPacking.readUInt64LE(from: pptStream, at: elCursor))
              elCursor += 8
              let fillHex = try BitPacking.readLengthPrefixedUTF16LE(
                from: pptStream, offset: &elCursor, limit: sAtomEnd)
              let strokeHex = try BitPacking.readLengthPrefixedUTF16LE(
                from: pptStream, offset: &elCursor, limit: sAtomEnd)
              var isBold = false
              var isItalic = false
              var isUnderlined = false
              if elCursor < sAtomEnd {
                let styleMask = pptStream[pptStream.startIndex + elCursor]
                isBold = (styleMask & 0x01) != 0
                isItalic = (styleMask & 0x02) != 0
                isUnderlined = (styleMask & 0x04) != 0
                elCursor += 1
              }
              var animation: SlideElementAnimation = .none
              if elCursor + 4 <= sAtomEnd,
                let animRaw = try? BitPacking.readLengthPrefixedUTF16LE(
                  from: pptStream, offset: &elCursor, limit: sAtomEnd),
                let parsedAnim = SlideElementAnimation(rawValue: animRaw)
              {
                animation = parsedAnim
              }
              sElements.append(
                SlideCanvasElement(
                  title: elTitle,
                  bodyText: elBody,
                  x: x,
                  y: y,
                  width: w,
                  height: h,
                  kind: SlideElementKind(rawValue: kindRaw) ?? .textBox,
                  zIndex: zIdx,
                  rotationDegrees: rot,
                  animation: animation,
                  fillHex: fillHex,
                  strokeHex: strokeHex,
                  isBold: isBold,
                  isItalic: isItalic,
                  isUnderlined: isUnderlined
                ))
            } else {
              let decodedText =
                sType == pptRecordTextCharsAtom
                ? try BitPacking.decodeUTF16LE(from: pptStream, at: sAtomStart, byteLength: sLen)
                : decodePPTEightBitText(start: sAtomStart, end: sAtomEnd)
              switch sVerInst {
              case 0x0000:
                if sTitle.isEmpty {
                  sTitle = decodedText
                } else if !decodedText.isEmpty {
                  let lines = decodedText.components(
                    separatedBy: CharacterSet(charactersIn: "\r\n")
                  ).filter { !$0.isEmpty }
                  sBullets.append(contentsOf: lines)
                }
              case 0x0010:
                sSubtitle = decodedText
              case 0x0020:
                sNotes = decodedText
              case 0x0030:
                sBullets.append(decodedText)
              default:
                if !decodedText.isEmpty {
                  if sTitle.isEmpty {
                    sTitle = decodedText
                  } else {
                    sBullets.append(decodedText)
                  }
                }
              }
              if !decodedText.isEmpty {
                currentSpTexts?.append(decodedText)
              }
            }
          } else if isSlideSubContainer(verInst: sVerInst, recType: sType) {
            if sType == pptRecordEscherSpContainer {
              let previousSpTexts = currentSpTexts
              currentSpTexts = []
              try parseSlideRecords(
                rangeStart: sAtomStart,
                rangeEnd: sAtomEnd,
                depth: depth + 1
              )
              if let collected = currentSpTexts, !collected.isEmpty {
                let joined = collected.joined(separator: "\n")
                let shapeTitle = sElements.isEmpty ? "Title 1" : "Shape \(sElements.count + 1)"
                sElements.append(
                  SlideCanvasElement(
                    title: shapeTitle,
                    bodyText: joined,
                    x: 40,
                    y: Double(60 + sElements.count * 80),
                    width: 320,
                    height: 80
                  ))
              }
              currentSpTexts = previousSpTexts
            } else {
              try parseSlideRecords(
                rangeStart: sAtomStart,
                rangeEnd: sAtomEnd,
                depth: depth + 1
              )
            }
          }
          sCursor = sAtomEnd
        }
      }

      try parseSlideRecords(rangeStart: start, rangeEnd: end, depth: 1)

      return SlideModel(
        title: sTitle,
        subtitle: sSubtitle,
        bulletPoints: sBullets,
        speakerNotes: sNotes,
        masterLayout: sLayout,
        transition: sTransition,
        elements: sElements,
        transitionDurationSeconds: sTransitionDuration,
        autoAdvanceSeconds: sAutoAdvance
      )
    }

    func parseDocumentRecords(rangeStart: Int, rangeEnd: Int, depth: Int) throws {
      guard depth <= maxPPTContainerRecursionDepth else {
        throw EasyOfficeError.corruptedContainer(detail: "PPT 문서 컨테이너 중첩 깊이가 상한을 초과했습니다.")
      }
      var offset = rangeStart
      while offset <= rangeEnd - 8 {
        let verAndInst = try BitPacking.readUInt16LE(from: pptStream, at: offset)
        let recType = try BitPacking.readUInt16LE(from: pptStream, at: offset + 2)
        let recLen = Int(try BitPacking.readUInt32LE(from: pptStream, at: offset + 4))
        let payloadStart = offset + 8
        let payloadEnd = payloadStart + recLen
        guard recLen >= 0, payloadEnd <= rangeEnd else {
          throw EasyOfficeError.corruptedContainer(detail: "PPT 레코드 길이가 RT_Document 경계를 초과합니다.")
        }

        if recType == pptRecordSlide {
          slides.append(try decodeSlideContainer(start: payloadStart, end: payloadEnd))
        } else if recType == pptRecordSlidePersistAtom {
          slidePersistGroups.append([])
        } else if recType == pptRecordTextCharsAtom || recType == pptRecordTextBytesAtom {
          let text =
            recType == pptRecordTextCharsAtom
            ? try BitPacking.decodeUTF16LE(from: pptStream, at: payloadStart, byteLength: recLen)
            : decodePPTEightBitText(start: payloadStart, end: payloadEnd)
          if depth == 1 {
            if verAndInst == 0x0000 {
              deckTitle = text
            } else if verAndInst == 0x0010, let parsedRatio = SlideAspectRatio(rawValue: text) {
              aspectRatio = parsedRatio
            } else if verAndInst == 0x0020, let parsedTheme = SlideDeckTheme(rawValue: text) {
              deckTheme = parsedTheme
            }
          } else if !text.isEmpty {
            let lines = text.components(separatedBy: CharacterSet(charactersIn: "\r\n")).filter {
              !$0.isEmpty
            }
            if slidePersistGroups.isEmpty {
              slidePersistGroups.append([])
            }
            slidePersistGroups[slidePersistGroups.count - 1].append(
              contentsOf: lines.isEmpty ? [text] : lines)
          }
        } else if recType == pptRecordSlideListWithText
          || recType == pptRecordDocument
          || (verAndInst & 0x000F) == 0x000F
        {
          if recType != pptRecordMainMaster {
            try parseDocumentRecords(
              rangeStart: payloadStart, rangeEnd: payloadEnd, depth: depth + 1)
          }
        }

        offset = payloadEnd
      }
    }

    let docEnd = 8 + rootLen
    try parseDocumentRecords(rangeStart: 8, rangeEnd: docEnd, depth: 1)
    if docEnd <= pptStream.count - 8 {
      try parseDocumentRecords(rangeStart: docEnd, rangeEnd: pptStream.count, depth: 1)
    }

    let nonEmptyGroups = slidePersistGroups.filter { !$0.isEmpty }
    if slides.isEmpty {
      for group in nonEmptyGroups {
        slides.append(
          SlideModel(
            title: group[0],
            bulletPoints: Array(group.dropFirst())
          ))
      }
    } else if nonEmptyGroups.count == slides.count {
      for idx in slides.indices where slides[idx].title.isEmpty && slides[idx].bulletPoints.isEmpty {
        let group = nonEmptyGroups[idx]
        slides[idx].title = group[0]
        slides[idx].bulletPoints = Array(group.dropFirst())
      }
    }

    return PresentationDeckModel(
      title: deckTitle,
      aspectRatio: aspectRatio,
      slides: slides,
      deckTheme: deckTheme
    )
  }

  private static func appendPPTAtom(
    verAndInstance: UInt16, recType: UInt16, payload: Data, to stream: inout Data
  ) {
    BitPacking.appendUInt16LE(verAndInstance, to: &stream)
    BitPacking.appendUInt16LE(recType, to: &stream)
    BitPacking.appendUInt32LE(UInt32(payload.count), to: &stream)
    stream.append(payload)
  }
}
