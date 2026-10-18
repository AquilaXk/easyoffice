import Foundation

// MARK: - Legacy OLE2 Office Binary Codecs (.doc FibBase, .xls BIFF8, .ppt Record Stream)

private struct BIFF8ExtendedMetadataEnvelope: Codable, Sendable {
  var cellStyles: [PackedCellCoordinate: CellStyle]
  var pageSetup: WorksheetPageSetup
  var sheetComments: [PackedCellCoordinate: String]
  var mergedRanges: [CellRange]
  var trackChangesEnabled: Bool
  var additionalSheets: [WorksheetSheetPage]
  var activeSheetIndex: Int
  var columnWidths: [Int: Double]?
  var rowHeights: [Int: Double]?
  var namedRanges: [String: String]?

  init(from sheet: WorksheetModel) {
    self.cellStyles = sheet.cellStyles
    self.pageSetup = sheet.pageSetup
    self.sheetComments = sheet.sheetComments
    self.mergedRanges = sheet.mergedRanges
    self.trackChangesEnabled = sheet.trackChangesEnabled
    self.additionalSheets = sheet.additionalSheets
    self.activeSheetIndex = sheet.activeSheetIndex
    self.columnWidths = sheet.columnWidths.isEmpty ? nil : sheet.columnWidths
    self.rowHeights = sheet.rowHeights.isEmpty ? nil : sheet.rowHeights
    self.namedRanges = sheet.namedRanges.isEmpty ? nil : sheet.namedRanges
  }

  func apply(to sheet: inout WorksheetModel) {
    if !cellStyles.isEmpty { sheet.cellStyles = cellStyles }
    sheet.pageSetup = pageSetup
    if !sheetComments.isEmpty { sheet.sheetComments = sheetComments }
    if !mergedRanges.isEmpty { sheet.mergedRanges = mergedRanges }
    sheet.trackChangesEnabled = trackChangesEnabled
    if !additionalSheets.isEmpty { sheet.additionalSheets = additionalSheets }
    sheet.activeSheetIndex = activeSheetIndex
    if let columnWidths {
      for (col, w) in columnWidths where w.isFinite {
        sheet.setColumnWidth(w, at: col)
      }
    }
    if let rowHeights {
      for (row, h) in rowHeights where h.isFinite {
        sheet.setRowHeight(h, at: row)
      }
    }
    if let namedRanges, !namedRanges.isEmpty {
      sheet.namedRanges = namedRanges
    }
  }
}

/// Genuine binary record codecs for legacy OLE2 document (`.doc`), spreadsheet (`.xls`), and presentation (`.ppt`) streams.
public enum LegacyOLE2OfficeCodec {

  // MARK: 1. Word Binary (.doc) — FibBase (0xA5EC) + UTF-16LE Character Stream

  public static let wordFibMagic: UInt16 = 0xA5EC
  public static let wordFibVersion97: UInt16 = 0x00C1
  public static let wordFlagEncrypted: UInt16 = 0x0100
  public static let wordStructuredTrailerMagic: UInt32 = 0x454F_4443  // "EODC"
  public static let wordExtendedMetaMagic: UInt32 = 0x454F_4D54  // "EOMT"

  public static func encodeWordBinary(_ doc: OfficeDocumentModel) throws -> Data {
    let joinedText = doc.paragraphs.map(\.text).joined(separator: "\r")
    let utf16TextBytes = BitPacking.encodeUTF16LE(joinedText)
    let fcMin: UInt32 = 64
    let fcMac: UInt32 = fcMin + UInt32(utf16TextBytes.count)
    let ccpText: UInt32 = UInt32(utf16TextBytes.count / 2)

    var wordStream = Data(repeating: 0, count: Int(fcMin))
    try BitPacking.writeUInt16LE(wordFibMagic, into: &wordStream, at: 0)
    try BitPacking.writeUInt16LE(wordFibVersion97, into: &wordStream, at: 2)
    try BitPacking.writeUInt16LE(0x0409, into: &wordStream, at: 6)  // lid
    try BitPacking.writeUInt16LE(0x0000, into: &wordStream, at: 10)  // flags (unencrypted, Unicode)
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
      BitPacking.appendUInt32LE(UInt32((p.fontSizePt * 100.0).rounded()), to: &wordStream)
      BitPacking.appendUInt32LE(UInt32((p.lineSpacingPercent * 100.0).rounded()), to: &wordStream)
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

    return try OLE2ContainerCodec.pack(streams: [
      OLE2StreamEntry(name: "WordDocument", data: wordStream)
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
    guard fcMin >= 64, fcMac >= fcMin, fcMac <= wordStream.count else {
      throw EasyOfficeError.corruptedContainer(
        detail: "Word FibBase 텍스트 범위(fcMin..fcMac)가 손상되었습니다.")
    }

    if fcMac + 4 <= wordStream.count,
      (try? BitPacking.readUInt32LE(from: wordStream, at: fcMac)) == wordStructuredTrailerMagic
    {
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
      for _ in 0..<tCount {
        let caption = try BitPacking.readLengthPrefixedUTF16LE(from: wordStream, offset: &cursor)
        guard cursor < wordStream.count else {
          throw EasyOfficeError.corruptedContainer(detail: "Word 표 헤더가 절단되었습니다.")
        }
        let hasHeader = wordStream[wordStream.startIndex + cursor] != 0
        cursor += 1
        let rowCount = Int(try BitPacking.readUInt16LE(from: wordStream, at: cursor))
        cursor += 2
        guard rowCount >= 0, rowCount <= (wordStream.count - cursor) / 2 else {
          throw EasyOfficeError.corruptedContainer(detail: "Word 표 행 개수가 스트림 범위를 초과했습니다.")
        }
        var rows: [[String]] = []
        for _ in 0..<rowCount {
          let colCount = Int(try BitPacking.readUInt16LE(from: wordStream, at: cursor))
          cursor += 2
          guard colCount >= 0, colCount <= (wordStream.count - cursor) / 4 else {
            throw EasyOfficeError.corruptedContainer(detail: "Word 표 열 개수가 스트림 범위를 초과했습니다.")
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
        }
      }

      return docModel
    }

    let textBytesLen = fcMac - fcMin
    let rawText = try BitPacking.decodeUTF16LE(
      from: wordStream, at: fcMin, byteLength: textBytesLen)
    let lines = rawText.components(separatedBy: CharacterSet(charactersIn: "\r\n")).filter {
      !$0.isEmpty
    }
    let paragraphs = lines.map { ParagraphBlock(text: $0) }
    return OfficeDocumentModel(title: "Word 문서", format: .doc, paragraphs: paragraphs)
  }

  // MARK: 2. BIFF8 Spreadsheet (.xls) — BOF / DIMENSIONS / SST / LABELSST / RK / FORMULA / NUMBER / LABEL / EOF

  public static let biff8OpcodeFormula: UInt16 = 0x0006
  public static let biff8OpcodeEOF: UInt16 = 0x000A
  public static let biff8OpcodeFilePass: UInt16 = 0x002F
  public static let biff8OpcodeBoundSheet: UInt16 = 0x0085
  public static let biff8OpcodeSST: UInt16 = 0x00FC
  public static let biff8OpcodeLabelSST: UInt16 = 0x00FD
  public static let biff8OpcodeDimensions: UInt16 = 0x0200
  public static let biff8OpcodeNumber: UInt16 = 0x0203
  public static let biff8OpcodeLabel: UInt16 = 0x0204
  public static let biff8OpcodeRK: UInt16 = 0x027E
  public static let biff8OpcodeBOF: UInt16 = 0x0809
  public static let biff8OpcodeExtendedMeta: UInt16 = 0x0868

  private static func formatBIFF8Numeric(_ num: Double) -> String {
    if num.rounded() == num && abs(num) < 1e15 {
      return String(Int64(num))
    }
    return String(num)
  }

  public static func encodeBIFF8Worksheet(_ sheet: WorksheetModel) throws -> Data {
    var stream = Data()

    // 1. BOF record (0x0809, length 16)
    BitPacking.appendUInt16LE(biff8OpcodeBOF, to: &stream)
    BitPacking.appendUInt16LE(16, to: &stream)
    BitPacking.appendUInt16LE(0x0600, to: &stream)  // BIFF8 version
    BitPacking.appendUInt16LE(0x0010, to: &stream)  // Worksheet substream
    BitPacking.appendUInt16LE(0x0DBB, to: &stream)  // build ID
    BitPacking.appendUInt16LE(0x07CC, to: &stream)  // build year
    BitPacking.appendUInt32LE(0, to: &stream)
    BitPacking.appendUInt32LE(0x0006, to: &stream)

    // 2. BOUNDSHEET record (0x0085) storing UTF-16LE sheet name
    let nameBytes = BitPacking.encodeUTF16LE(sheet.name)
    guard 8 + nameBytes.count <= Int(UInt16.max) else {
      throw EasyOfficeError.corruptedContainer(detail: "BIFF8 시트 이름 길이가 레코드 한도를 초과했습니다.")
    }
    let boundSheetLen = UInt16(8 + nameBytes.count)
    BitPacking.appendUInt16LE(biff8OpcodeBoundSheet, to: &stream)
    BitPacking.appendUInt16LE(boundSheetLen, to: &stream)
    BitPacking.appendUInt32LE(0, to: &stream)  // lbPlyPos
    stream.append(0)  // visible
    stream.append(0)  // worksheet
    BitPacking.appendUInt16LE(UInt16(sheet.name.utf16.count), to: &stream)
    stream.append(nameBytes)

    // 3. DIMENSIONS record (0x0200, length 14) — single-pass min/max over sortedCoords
    let sortedCoords = sheet.cellsByCoordinate.keys.sorted()
    var minRow = 0
    var maxRow = 0
    var minCol = 0
    var maxCol = 0
    if let first = sortedCoords.first {
      minRow = first.row
      maxRow = first.row
      minCol = first.column
      maxCol = first.column
      for coord in sortedCoords.dropFirst() {
        if coord.row < minRow { minRow = coord.row }
        if coord.row > maxRow { maxRow = coord.row }
        if coord.column < minCol { minCol = coord.column }
        if coord.column > maxCol { maxCol = coord.column }
      }
    }
    let firstRow = UInt32(minRow)
    let lastRowPlus1 = UInt32(sortedCoords.isEmpty ? 0 : maxRow + 1)
    let firstCol = UInt16(minCol)
    let lastColPlus1 = UInt16(sortedCoords.isEmpty ? 0 : maxCol + 1)

    BitPacking.appendUInt16LE(biff8OpcodeDimensions, to: &stream)
    BitPacking.appendUInt16LE(14, to: &stream)
    BitPacking.appendUInt32LE(firstRow, to: &stream)
    BitPacking.appendUInt32LE(lastRowPlus1, to: &stream)
    BitPacking.appendUInt16LE(firstCol, to: &stream)
    BitPacking.appendUInt16LE(lastColPlus1, to: &stream)
    BitPacking.appendUInt16LE(0, to: &stream)

    // 4. Cell records: NUMBER (0x0203) for numbers, LABEL (0x0204) for strings and formulas
    for coord in sortedCoords {
      guard let raw = sheet.cellsByCoordinate[coord], !raw.isEmpty else { continue }
      let row16 = UInt16(clamping: coord.row)
      let col16 = UInt16(clamping: coord.column)

      if !raw.hasPrefix("="), let num = Double(raw), num.isFinite {
        BitPacking.appendUInt16LE(biff8OpcodeNumber, to: &stream)
        BitPacking.appendUInt16LE(14, to: &stream)
        BitPacking.appendUInt16LE(row16, to: &stream)
        BitPacking.appendUInt16LE(col16, to: &stream)
        BitPacking.appendUInt16LE(0, to: &stream)  // xf index
        BitPacking.appendUInt64LE(num.bitPattern, to: &stream)
      } else {
        let textUTF16 = BitPacking.encodeUTF16LE(raw)
        guard 8 + textUTF16.count <= Int(UInt16.max) else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 셀 문자열 길이가 16비트 레코드 한도를 초과했습니다.")
        }
        let recLen = UInt16(8 + textUTF16.count)
        BitPacking.appendUInt16LE(biff8OpcodeLabel, to: &stream)
        BitPacking.appendUInt16LE(recLen, to: &stream)
        BitPacking.appendUInt16LE(row16, to: &stream)
        BitPacking.appendUInt16LE(col16, to: &stream)
        BitPacking.appendUInt16LE(0, to: &stream)  // xf index
        BitPacking.appendUInt16LE(UInt16(raw.utf16.count), to: &stream)
        stream.append(textUTF16)
      }
    }

    // 5. Extended worksheet metadata record (0x0868)
    let extEnvelope = BIFF8ExtendedMetadataEnvelope(from: sheet)
    if let extBytes = try? JSONEncoder().encode(extEnvelope),
      !extBytes.isEmpty,
      extBytes.count <= Int(UInt16.max)
    {
      BitPacking.appendUInt16LE(biff8OpcodeExtendedMeta, to: &stream)
      BitPacking.appendUInt16LE(UInt16(extBytes.count), to: &stream)
      stream.append(extBytes)
    }

    // 6. EOF record (0x000A, length 0)
    BitPacking.appendUInt16LE(biff8OpcodeEOF, to: &stream)
    BitPacking.appendUInt16LE(0, to: &stream)

    return try OLE2ContainerCodec.pack(streams: [
      OLE2StreamEntry(name: "Workbook", data: stream)
    ])
  }

  public static func decodeBIFF8Worksheet(from data: Data, name: String = "Sheet1") throws
    -> WorksheetModel
  {
    let streams = try OLE2ContainerCodec.unpack(from: data)
    guard let workbook = streams.first(where: { $0.name == "Workbook" || $0.name == "Book" })?.data,
      workbook.count >= 4
    else {
      throw EasyOfficeError.corruptedContainer(detail: "BIFF8 필수 스트림 'Workbook'이 누락되었거나 손상되었습니다.")
    }

    let firstOpcode = try BitPacking.readUInt16LE(from: workbook, at: 0)
    guard firstOpcode == biff8OpcodeBOF else {
      throw EasyOfficeError.corruptedContainer(
        detail: "BIFF8 BOF(0x0809) 레코드 시그니처 불일치: \(String(format: "0x%04X", firstOpcode))")
    }

    var worksheet = WorksheetModel(name: name)
    var sstStrings: [String] = []
    var currentSubstreamType: UInt16 = 0x0010
    var offset = 0

    while offset <= workbook.count - 4 {
      let opcode = try BitPacking.readUInt16LE(from: workbook, at: offset)
      let length = Int(try BitPacking.readUInt16LE(from: workbook, at: offset + 2))
      let payloadStart = offset + 4
      let payloadEnd = payloadStart + length
      guard length >= 0, payloadEnd <= workbook.count else {
        throw EasyOfficeError.corruptedContainer(
          detail: "BIFF8 레코드 길이가 스트림 경계를 초과합니다 (opcode: \(String(format: "0x%04X", opcode))).")
      }

      switch opcode {
      case biff8OpcodeBOF:
        if length >= 4 {
          currentSubstreamType = try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 2)
        }

      case biff8OpcodeFilePass:
        throw EasyOfficeError.encryptedDocumentUnsupported(format: .xls)

      case biff8OpcodeBoundSheet:
        if length >= 8 {
          let charCount = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 6))
          let byteLen = charCount * 2
          if payloadStart + 8 + byteLen <= payloadEnd {
            let decodedName = try BitPacking.decodeUTF16LE(
              from: workbook, at: payloadStart + 8, byteLength: byteLen)
            if !decodedName.isEmpty {
              worksheet.name = decodedName
            }
          }
        }

      case biff8OpcodeSST:
        guard length >= 8 else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 SST 레코드 길이 오류: \(length)")
        }
        let uniqueCount = Int(try BitPacking.readUInt32LE(from: workbook, at: payloadStart + 4))
        var sCursor = payloadStart + 8
        for _ in 0..<min(max(0, uniqueCount), 65_536) {
          guard sCursor + 3 <= payloadEnd else { break }
          let charCount = Int(try BitPacking.readUInt16LE(from: workbook, at: sCursor))
          let flags = workbook[workbook.startIndex + sCursor + 2]
          sCursor += 3

          var richRunCount = 0
          var extRstByteLen = 0
          if (flags & 0x08) != 0 {
            guard sCursor + 2 <= payloadEnd else { break }
            richRunCount = Int(try BitPacking.readUInt16LE(from: workbook, at: sCursor))
            sCursor += 2
          }
          if (flags & 0x04) != 0 {
            guard sCursor + 4 <= payloadEnd else { break }
            extRstByteLen = Int(try BitPacking.readUInt32LE(from: workbook, at: sCursor))
            sCursor += 4
          }

          let decoded: String
          if (flags & 0x01) != 0 {
            let byteLen = charCount * 2
            guard sCursor + byteLen <= payloadEnd else { break }
            decoded = try BitPacking.decodeUTF16LE(from: workbook, at: sCursor, byteLength: byteLen)
            sCursor += byteLen
          } else {
            guard sCursor + charCount <= payloadEnd else { break }
            let slice = workbook[
              (workbook.startIndex + sCursor)..<(workbook.startIndex + sCursor + charCount)]
            let latin1Units = slice.map { UInt16($0) }
            decoded = String(decoding: latin1Units, as: UTF16.self)
            sCursor += charCount
          }
          sstStrings.append(decoded)
          sCursor = min(payloadEnd, sCursor + richRunCount * 4 + max(0, extRstByteLen))
        }

      case biff8OpcodeLabelSST:
        guard length >= 10 else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 LABELSST 레코드 길이 오류: \(length)")
        }
        let row = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart))
        let col = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 2))
        let sstIdx = Int(try BitPacking.readUInt32LE(from: workbook, at: payloadStart + 6))
        guard sstIdx >= 0, sstIdx < sstStrings.count else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 LABELSST 인덱스 범위 초과: \(sstIdx)")
        }
        try worksheet.setCell(row: row, column: col, rawValue: sstStrings[sstIdx])

      case biff8OpcodeRK:
        guard length >= 10 else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 RK 레코드 길이 오류: \(length)")
        }
        let row = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart))
        let col = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 2))
        let rkRaw = try BitPacking.readUInt32LE(from: workbook, at: payloadStart + 6)
        var numVal: Double
        if (rkRaw & 0x02) != 0 {
          let signedInt = Int32(bitPattern: rkRaw) >> 2
          numVal = Double(signedInt)
        } else {
          let floatBits = UInt64(rkRaw & ~0x03) << 32
          numVal = Double(bitPattern: floatBits)
        }
        if (rkRaw & 0x01) != 0 {
          numVal /= 100.0
        }
        try worksheet.setCell(row: row, column: col, rawValue: formatBIFF8Numeric(numVal))

      case biff8OpcodeFormula:
        guard length >= 14 else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 FORMULA 레코드 길이 오류: \(length)")
        }
        let row = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart))
        let col = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 2))
        let bits = try BitPacking.readUInt64LE(from: workbook, at: payloadStart + 6)
        let num = Double(bitPattern: bits)
        if num.isFinite {
          try worksheet.setCell(row: row, column: col, rawValue: formatBIFF8Numeric(num))
        }

      case biff8OpcodeNumber:
        guard length == 14 else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 NUMBER 레코드 길이 오류: \(length)")
        }
        let row = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart))
        let col = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 2))
        let bits = try BitPacking.readUInt64LE(from: workbook, at: payloadStart + 6)
        let num = Double(bitPattern: bits)
        try worksheet.setCell(row: row, column: col, rawValue: formatBIFF8Numeric(num))

      case biff8OpcodeLabel:
        guard length >= 8 else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 LABEL 레코드 길이 오류: \(length)")
        }
        let row = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart))
        let col = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 2))
        let charCount = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 6))
        let byteLen = charCount * 2
        guard payloadStart + 8 + byteLen <= payloadEnd else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 LABEL 문자열 길이가 레코드 경계를 초과합니다.")
        }
        let str = try BitPacking.decodeUTF16LE(
          from: workbook, at: payloadStart + 8, byteLength: byteLen)
        try worksheet.setCell(row: row, column: col, rawValue: str)

      case biff8OpcodeExtendedMeta:
        let slice = Data(
          workbook[(workbook.startIndex + payloadStart)..<(workbook.startIndex + payloadEnd)])
        if let ext = try? JSONDecoder().decode(BIFF8ExtendedMetadataEnvelope.self, from: slice) {
          ext.apply(to: &worksheet)
        }

      case biff8OpcodeEOF:
        if currentSubstreamType == 0x0010 || payloadEnd > workbook.count - 4 {
          return worksheet
        }

      default:
        break
      }

      offset = payloadEnd
    }

    return worksheet
  }

  // MARK: 3. Presentation Binary (.ppt) — RT_Document (1000) / RT_Slide (1006) / RT_SlideAtom (1007) / RT_TextCharsAtom (4000)

  public static let pptRecordDocument: UInt16 = 1000
  public static let pptRecordSlide: UInt16 = 1006
  public static let pptRecordSlideAtom: UInt16 = 1007
  public static let pptRecordTextCharsAtom: UInt16 = 4000

  public static func encodePowerPointBinary(_ deck: PresentationDeckModel) throws -> Data {
    var docChildren = Data()
    appendPPTAtom(
      verAndInstance: 0x0000, recType: pptRecordTextCharsAtom,
      payload: BitPacking.encodeUTF16LE(deck.title), to: &docChildren)
    appendPPTAtom(
      verAndInstance: 0x0010, recType: pptRecordTextCharsAtom,
      payload: BitPacking.encodeUTF16LE(deck.aspectRatio.rawValue), to: &docChildren)

    for slide in deck.slides {
      var slideChildren = Data()
      let slideAtomText = "\(slide.masterLayout.rawValue)|\(slide.transition.rawValue)"
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
    guard rootType == pptRecordDocument, 8 + rootLen <= pptStream.count else {
      throw EasyOfficeError.corruptedContainer(detail: "PPT RT_Document(1000) 루트 레코드가 유효하지 않습니다.")
    }

    var deckTitle = "발표자료"
    var aspectRatio: SlideAspectRatio = .widescreen16x9
    var slides: [SlideModel] = []

    var offset = 8
    let docEnd = 8 + rootLen

    while offset <= docEnd - 8 {
      let verAndInst = try BitPacking.readUInt16LE(from: pptStream, at: offset)
      let recType = try BitPacking.readUInt16LE(from: pptStream, at: offset + 2)
      let recLen = Int(try BitPacking.readUInt32LE(from: pptStream, at: offset + 4))
      let payloadStart = offset + 8
      let payloadEnd = payloadStart + recLen
      guard recLen >= 0, payloadEnd <= docEnd else {
        throw EasyOfficeError.corruptedContainer(detail: "PPT 레코드 길이가 RT_Document 경계를 초과합니다.")
      }

      if recType == pptRecordTextCharsAtom {
        let text = try BitPacking.decodeUTF16LE(
          from: pptStream, at: payloadStart, byteLength: recLen)
        if verAndInst == 0x0000 {
          deckTitle = text
        } else if verAndInst == 0x0010, let parsedRatio = SlideAspectRatio(rawValue: text) {
          aspectRatio = parsedRatio
        }
      } else if recType == pptRecordSlide {
        var sTitle = ""
        var sSubtitle = ""
        var sNotes = ""
        var sBullets: [String] = []
        var sElements: [SlideCanvasElement] = []
        var sLayout: SlideMasterLayout = .titleAndContent
        var sTransition: SlideTransitionKind = .fade

        var sCursor = payloadStart
        while sCursor <= payloadEnd - 8 {
          let sVerInst = try BitPacking.readUInt16LE(from: pptStream, at: sCursor)
          let sType = try BitPacking.readUInt16LE(from: pptStream, at: sCursor + 2)
          let sLen = Int(try BitPacking.readUInt32LE(from: pptStream, at: sCursor + 4))
          let sAtomStart = sCursor + 8
          let sAtomEnd = sAtomStart + sLen
          guard sLen >= 0, sAtomEnd <= payloadEnd else {
            throw EasyOfficeError.corruptedContainer(detail: "PPT RT_Slide 내부 아톰 길이가 경계를 초과합니다.")
          }

          if sType == pptRecordSlideAtom {
            let atomStr = try BitPacking.decodeUTF16LE(
              from: pptStream, at: sAtomStart, byteLength: sLen)
            let parts = atomStr.split(separator: "|", omittingEmptySubsequences: false).map(
              String.init)
            if parts.count >= 2 {
              sLayout = SlideMasterLayout(rawValue: parts[0]) ?? .titleAndContent
              sTransition = SlideTransitionKind(rawValue: parts[1]) ?? .fade
            }
          } else if sType == pptRecordTextCharsAtom {
            switch sVerInst {
            case 0x0000:
              sTitle = try BitPacking.decodeUTF16LE(
                from: pptStream, at: sAtomStart, byteLength: sLen)
            case 0x0010:
              sSubtitle = try BitPacking.decodeUTF16LE(
                from: pptStream, at: sAtomStart, byteLength: sLen)
            case 0x0020:
              sNotes = try BitPacking.decodeUTF16LE(
                from: pptStream, at: sAtomStart, byteLength: sLen)
            case 0x0030:
              sBullets.append(
                try BitPacking.decodeUTF16LE(from: pptStream, at: sAtomStart, byteLength: sLen))
            case 0x0040:
              var elCursor = sAtomStart
              let elTitle = try BitPacking.readLengthPrefixedUTF16LE(
                from: pptStream, offset: &elCursor, limit: sAtomEnd)
              let elBody = try BitPacking.readLengthPrefixedUTF16LE(
                from: pptStream, offset: &elCursor, limit: sAtomEnd)
              guard elCursor + 32 <= sAtomEnd else {
                throw EasyOfficeError.corruptedContainer(detail: "PPT 슬라이드 도형 좌표 필드가 절단되었습니다.")
              }
              let x = Double(bitPattern: try BitPacking.readUInt64LE(from: pptStream, at: elCursor))
              elCursor += 8
              let y = Double(bitPattern: try BitPacking.readUInt64LE(from: pptStream, at: elCursor))
              elCursor += 8
              let w = Double(bitPattern: try BitPacking.readUInt64LE(from: pptStream, at: elCursor))
              elCursor += 8
              let h = Double(bitPattern: try BitPacking.readUInt64LE(from: pptStream, at: elCursor))
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
            default:
              break
            }
          }

          sCursor = sAtomEnd
        }

        slides.append(
          SlideModel(
            title: sTitle,
            subtitle: sSubtitle,
            bulletPoints: sBullets,
            speakerNotes: sNotes,
            masterLayout: sLayout,
            transition: sTransition,
            elements: sElements
          ))
      }

      offset = payloadEnd
    }

    return PresentationDeckModel(title: deckTitle, aspectRatio: aspectRatio, slides: slides)
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
