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
  var conditionalFormatRules: [SheetConditionalFormatRule]?
  var dataValidationRule: SheetDataValidationRule?

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
    self.conditionalFormatRules =
      sheet.conditionalFormatRules.isEmpty ? nil : sheet.conditionalFormatRules
    self.dataValidationRule = sheet.dataValidationRule
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
    if let conditionalFormatRules, !conditionalFormatRules.isEmpty {
      sheet.conditionalFormatRules = conditionalFormatRules
    }
    if let dataValidationRule {
      sheet.dataValidationRule = dataValidationRule
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
      guard tbl.rows.count <= HWP5BinaryCodec.maxHWP5TableRows else {
        throw EasyOfficeError.corruptedContainer(
          detail: "Word 바이너리 표 행 개수(\(tbl.rows.count))가 허용 상한(\(HWP5BinaryCodec.maxHWP5TableRows))을 초과했습니다."
        )
      }
      BitPacking.appendLengthPrefixedUTF16LE(tbl.caption, to: &wordStream)
      wordStream.append(tbl.hasHeaderRow ? 1 : 0)
      BitPacking.appendUInt16LE(UInt16(tbl.rows.count), to: &wordStream)
      for row in tbl.rows {
        guard row.count <= HWP5BinaryCodec.maxHWP5TableColumns else {
          throw EasyOfficeError.corruptedContainer(
            detail: "Word 바이너리 표 열 개수(\(row.count))가 허용 상한(\(HWP5BinaryCodec.maxHWP5TableColumns))을 초과했습니다."
          )
        }
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
          guard rowCount >= 0, rowCount <= HWP5BinaryCodec.maxHWP5TableRows,
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
          guard rowCount >= 0, rowCount <= HWP5BinaryCodec.maxHWP5TableRows,
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
      rawText =
        String(data: rawSlice, encoding: .utf8)
        ?? String(data: rawSlice, encoding: .windowsCP1252)
        ?? String(decoding: rawSlice, as: UTF8.self)
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

  // MARK: 2. BIFF8 Spreadsheet (.xls) — BOF / DIMENSIONS / SST / LABELSST / RK / MULRK / FORMULA / NUMBER / LABEL / EOF

  public static let biff8OpcodeFormula: UInt16 = 0x0006
  public static let biff8OpcodeEOF: UInt16 = 0x000A
  public static let biff8OpcodeFilePass: UInt16 = 0x002F
  public static let biff8OpcodeBoundSheet: UInt16 = 0x0085
  public static let biff8OpcodeMulRK: UInt16 = 0x00BD
  public static let biff8OpcodeSST: UInt16 = 0x00FC
  public static let biff8OpcodeLabelSST: UInt16 = 0x00FD
  public static let biff8OpcodeDimensions: UInt16 = 0x0200
  public static let biff8OpcodeNumber: UInt16 = 0x0203
  public static let biff8OpcodeLabel: UInt16 = 0x0204
  public static let biff8OpcodeString: UInt16 = 0x0207
  public static let biff8OpcodeRK: UInt16 = 0x027E
  public static let biff8OpcodeBOF: UInt16 = 0x0809
  public static let biff8OpcodeExtendedMeta: UInt16 = 0x0868
  public static let maxBIFF8Rows = 65_536
  public static let maxBIFF8Columns = 256

  private static func formatBIFF8Numeric(_ num: Double) -> String {
    if num.rounded() == num && abs(num) < 1e15 {
      return String(Int64(num))
    }
    return String(num)
  }

  private static func decodeRKValue(_ rkRaw: UInt32) -> Double {
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
    return numVal
  }

  private static func biff8ErrorCodeString(_ code: UInt8) -> String {
    switch code {
    case 0x00: return "#NULL!"
    case 0x07: return "#DIV/0!"
    case 0x0F: return "#VALUE!"
    case 0x17: return "#REF!"
    case 0x1D: return "#NAME?"
    case 0x24: return "#NUM!"
    case 0x2A: return "#N/A"
    default: return "#VALUE!"
    }
  }

  public static func encodeBIFF8Worksheet(_ sheet: WorksheetModel) throws -> Data {
    let nameUnitsCount = sheet.name.utf16.count
    guard nameUnitsCount <= 255 else {
      throw EasyOfficeError.corruptedContainer(
        detail: "BIFF8 시트 이름 길이(\(nameUnitsCount))가 255 UTF-16 코드 유닛 한도를 초과했습니다.")
    }

    let sortedCoords = sheet.cellsByCoordinate.keys.sorted()
    for coord in sortedCoords {
      guard coord.row >= 0, coord.row < maxBIFF8Rows,
        coord.column >= 0, coord.column < maxBIFF8Columns
      else {
        throw EasyOfficeError.corruptedContainer(
          detail: "BIFF8(.xls) 워크시트 좌표(\(coord.row), \(coord.column))가 65,536행×256열 규격 한도를 초과했습니다.")
      }
    }

    var stream = Data()

    // 1. Workbook Globals Substream: BOF record (0x0809, length 16, dt = 0x0005)
    BitPacking.appendUInt16LE(biff8OpcodeBOF, to: &stream)
    BitPacking.appendUInt16LE(16, to: &stream)
    BitPacking.appendUInt16LE(0x0600, to: &stream)  // BIFF8 version
    BitPacking.appendUInt16LE(0x0005, to: &stream)  // Workbook Globals substream
    BitPacking.appendUInt16LE(0x0DBB, to: &stream)  // build ID
    BitPacking.appendUInt16LE(0x07CC, to: &stream)  // build year
    BitPacking.appendUInt32LE(0, to: &stream)
    BitPacking.appendUInt32LE(0x0006, to: &stream)

    // 2. BOUNDSHEET record (0x0085) with standard ShortXLUnicodeString (1-byte cch + 1-byte grbit=0x01)
    let nameBytes = BitPacking.encodeUTF16LE(sheet.name)
    guard 8 + nameBytes.count <= Int(UInt16.max) else {
      throw EasyOfficeError.corruptedContainer(detail: "BIFF8 시트 이름 길이가 레코드 한도를 초과했습니다.")
    }
    let boundSheetLen = UInt16(8 + nameBytes.count)
    BitPacking.appendUInt16LE(biff8OpcodeBoundSheet, to: &stream)
    BitPacking.appendUInt16LE(boundSheetLen, to: &stream)
    let lbPlyPosOffset = stream.count
    BitPacking.appendUInt32LE(0, to: &stream)  // lbPlyPos (patched to Worksheet BOF offset below)
    stream.append(0)  // hsState: visible
    stream.append(0)  // dt: worksheet
    stream.append(UInt8(nameUnitsCount))  // 1-byte cch
    stream.append(0x01)  // grbit = 0x01 (uncompressed UTF-16LE)
    stream.append(nameBytes)

    // 3. Workbook Globals EOF record (0x000A, length 0)
    BitPacking.appendUInt16LE(biff8OpcodeEOF, to: &stream)
    BitPacking.appendUInt16LE(0, to: &stream)

    // Patch lbPlyPos in BOUNDSHEET to point to the start of the Worksheet substream
    let worksheetSubstreamOffset = UInt32(stream.count)
    try BitPacking.writeUInt32LE(worksheetSubstreamOffset, into: &stream, at: lbPlyPosOffset)

    // 4. Worksheet Substream: BOF record (0x0809, length 16, dt = 0x0010)
    BitPacking.appendUInt16LE(biff8OpcodeBOF, to: &stream)
    BitPacking.appendUInt16LE(16, to: &stream)
    BitPacking.appendUInt16LE(0x0600, to: &stream)  // BIFF8 version
    BitPacking.appendUInt16LE(0x0010, to: &stream)  // Worksheet substream
    BitPacking.appendUInt16LE(0x0DBB, to: &stream)  // build ID
    BitPacking.appendUInt16LE(0x07CC, to: &stream)  // build year
    BitPacking.appendUInt32LE(0, to: &stream)
    BitPacking.appendUInt32LE(0x0006, to: &stream)

    // 5. DIMENSIONS record (0x0200, length 14) — single-pass min/max over sortedCoords
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

    // 6. Cell records: NUMBER (0x0203) for numbers, LABEL (0x0204) with XLUnicodeString for strings and formulas
    for coord in sortedCoords {
      guard let raw = sheet.cellsByCoordinate[coord], !raw.isEmpty else { continue }
      let row16 = UInt16(coord.row)
      let col16 = UInt16(coord.column)

      if !raw.hasPrefix("="), let num = Double(raw), num.isFinite {
        BitPacking.appendUInt16LE(biff8OpcodeNumber, to: &stream)
        BitPacking.appendUInt16LE(14, to: &stream)
        BitPacking.appendUInt16LE(row16, to: &stream)
        BitPacking.appendUInt16LE(col16, to: &stream)
        BitPacking.appendUInt16LE(0, to: &stream)  // xf index
        BitPacking.appendUInt64LE(num.bitPattern, to: &stream)
      } else {
        let rawUTF16Count = raw.utf16.count
        let textUTF16 = BitPacking.encodeUTF16LE(raw)
        guard rawUTF16Count <= Int(UInt16.max), 9 + textUTF16.count <= Int(UInt16.max) else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 셀 문자열 길이가 16비트 레코드 한도를 초과했습니다.")
        }
        let recLen = UInt16(9 + textUTF16.count)
        BitPacking.appendUInt16LE(biff8OpcodeLabel, to: &stream)
        BitPacking.appendUInt16LE(recLen, to: &stream)
        BitPacking.appendUInt16LE(row16, to: &stream)
        BitPacking.appendUInt16LE(col16, to: &stream)
        BitPacking.appendUInt16LE(0, to: &stream)  // xf index
        BitPacking.appendUInt16LE(UInt16(rawUTF16Count), to: &stream)
        stream.append(0x01)  // XLUnicodeString grbit = 0x01 (UTF-16LE)
        stream.append(textUTF16)
      }
    }

    let extEnvelope = BIFF8ExtendedMetadataEnvelope(from: sheet)
    if let extBytes = try? JSONEncoder().encode(extEnvelope), !extBytes.isEmpty {
      let maxChunkLen = Int(UInt16.max)
      var chunkOffset = 0
      while chunkOffset < extBytes.count {
        let chunkLen = min(maxChunkLen, extBytes.count - chunkOffset)
        BitPacking.appendUInt16LE(biff8OpcodeExtendedMeta, to: &stream)
        BitPacking.appendUInt16LE(UInt16(chunkLen), to: &stream)
        let startIdx = extBytes.startIndex + chunkOffset
        stream.append(extBytes[startIdx..<(startIdx + chunkLen)])
        chunkOffset += chunkLen
      }
    }

    // 8. Worksheet EOF record (0x000A, length 0)
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

    var boundSheetNames: [String] = []
    var completedSheets: [WorksheetModel] = []
    var currentSheet = WorksheetModel(name: name)
    var currentSheetIndex = 0
    var inWorksheetSubstream = false
    var extendedMetaBytes = Data()
    var pendingFormulaCoord: (row: Int, col: Int)? = nil
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
        pendingFormulaCoord = nil
        if length >= 4 {
          currentSubstreamType = try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 2)
        } else {
          currentSubstreamType = 0x0010
        }
        if currentSubstreamType == 0x0010 {
          if inWorksheetSubstream {
            completedSheets.append(currentSheet)
            currentSheetIndex += 1
          }
          let resolvedName: String
          if currentSheetIndex < boundSheetNames.count, !boundSheetNames[currentSheetIndex].isEmpty
          {
            resolvedName = boundSheetNames[currentSheetIndex]
          } else if currentSheetIndex == 0 {
            resolvedName = name
          } else {
            resolvedName = "Sheet\(currentSheetIndex + 1)"
          }
          currentSheet = WorksheetModel(name: resolvedName)
          inWorksheetSubstream = true
        }

      case biff8OpcodeFilePass:
        throw EasyOfficeError.encryptedDocumentUnsupported(format: .xls)

      case biff8OpcodeBoundSheet:
        if length >= 8 {
          let cchByte = Int(workbook[workbook.startIndex + payloadStart + 6])
          let grbitByte = workbook[workbook.startIndex + payloadStart + 7]
          let remBytes = payloadEnd - (payloadStart + 8)
          var decodedName = ""
          if (grbitByte & 0x01) != 0, remBytes >= cchByte * 2 {
            decodedName = try BitPacking.decodeUTF16LE(
              from: workbook, at: payloadStart + 8, byteLength: cchByte * 2)
          } else if grbitByte == 0x00, remBytes == cchByte, cchByte > 0 {
            let slice = Data(
              workbook[
                (workbook.startIndex + payloadStart + 8)..<(workbook.startIndex + payloadStart + 8
                  + cchByte)])
            decodedName =
              String(data: slice, encoding: .windowsCP1252)
              ?? String(decoding: slice, as: UTF8.self)
          } else {
            let charCount = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 6))
            let byteLen = charCount * 2
            if payloadStart + 8 + byteLen <= payloadEnd {
              decodedName = try BitPacking.decodeUTF16LE(
                from: workbook, at: payloadStart + 8, byteLength: byteLen)
            }
          }
          if !decodedName.isEmpty {
            boundSheetNames.append(decodedName)
            if inWorksheetSubstream && currentSheetIndex == 0 && boundSheetNames.count == 1 {
              currentSheet.name = decodedName
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
        pendingFormulaCoord = nil
        guard length >= 10 else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 LABELSST 레코드 길이 오류: \(length)")
        }
        let row = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart))
        let col = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 2))
        let sstIdx = Int(try BitPacking.readUInt32LE(from: workbook, at: payloadStart + 6))
        guard sstIdx >= 0, sstIdx < sstStrings.count else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 LABELSST 인덱스 범위 초과: \(sstIdx)")
        }
        try currentSheet.setCell(row: row, column: col, rawValue: sstStrings[sstIdx])

      case biff8OpcodeRK:
        pendingFormulaCoord = nil
        guard length >= 10 else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 RK 레코드 길이 오류: \(length)")
        }
        let row = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart))
        let col = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 2))
        let rkRaw = try BitPacking.readUInt32LE(from: workbook, at: payloadStart + 6)
        let numVal = decodeRKValue(rkRaw)
        try currentSheet.setCell(row: row, column: col, rawValue: formatBIFF8Numeric(numVal))

      case biff8OpcodeMulRK:
        guard length >= 12 else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 MULRK 레코드 길이 오류: \(length)")
        }
        let row = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart))
        let firstCol = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 2))
        let lastCol = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadEnd - 2))
        var rkCursor = payloadStart + 4
        var col = firstCol
        while rkCursor + 6 <= payloadEnd - 2 && col <= lastCol {
          let rkRaw = try BitPacking.readUInt32LE(from: workbook, at: rkCursor + 2)
          let numVal = decodeRKValue(rkRaw)
          try currentSheet.setCell(row: row, column: col, rawValue: formatBIFF8Numeric(numVal))
          rkCursor += 6
          col += 1
        }

      case biff8OpcodeFormula:
        guard length >= 14 else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 FORMULA 레코드 길이 오류: \(length)")
        }
        let row = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart))
        let col = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 2))
        var setFormulaExpr = false
        if length >= 18 {
          let cce = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 14))
          if cce > 0, cce % 2 == 0, payloadStart + 16 + cce <= payloadEnd,
            let expr = try? BitPacking.decodeUTF16LE(
              from: workbook, at: payloadStart + 16, byteLength: cce),
            expr.hasPrefix("=")
          {
            try currentSheet.setCell(row: row, column: col, rawValue: expr)
            setFormulaExpr = true
          }
        }
        if !setFormulaExpr {
          let fExprO = try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 12)
          if fExprO == 0xFFFF {
            let valueType = workbook[workbook.startIndex + payloadStart + 6]
            switch valueType {
            case 0x00:
              pendingFormulaCoord = (row: row, col: col)
            case 0x01:
              pendingFormulaCoord = nil
              let boolByte = workbook[workbook.startIndex + payloadStart + 8]
              try currentSheet.setCell(
                row: row, column: col, rawValue: boolByte != 0 ? "TRUE" : "FALSE")
            case 0x02:
              pendingFormulaCoord = nil
              let errByte = workbook[workbook.startIndex + payloadStart + 8]
              try currentSheet.setCell(
                row: row, column: col, rawValue: biff8ErrorCodeString(errByte))
            default:
              pendingFormulaCoord = nil
            }
          } else {
            pendingFormulaCoord = nil
            let bits = try BitPacking.readUInt64LE(from: workbook, at: payloadStart + 6)
            let num = Double(bitPattern: bits)
            if num.isFinite {
              try currentSheet.setCell(row: row, column: col, rawValue: formatBIFF8Numeric(num))
            }
          }
        } else {
          pendingFormulaCoord = nil
        }

      case biff8OpcodeString:
        if let target = pendingFormulaCoord, length >= 3 {
          let charCount = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart))
          let flags = workbook[workbook.startIndex + payloadStart + 2]
          var sCursor = payloadStart + 3
          if (flags & 0x08) != 0 { sCursor += 2 }
          if (flags & 0x04) != 0 { sCursor += 4 }
          if (flags & 0x01) != 0 {
            let byteLen = charCount * 2
            if sCursor + byteLen <= payloadEnd {
              let str = try BitPacking.decodeUTF16LE(
                from: workbook, at: sCursor, byteLength: byteLen)
              try currentSheet.setCell(row: target.row, column: target.col, rawValue: str)
            }
          } else if sCursor + charCount <= payloadEnd {
            let slice = Data(
              workbook[(workbook.startIndex + sCursor)..<(workbook.startIndex + sCursor + charCount)]
            )
            let str =
              String(data: slice, encoding: .windowsCP1252)
              ?? String(decoding: slice, as: UTF8.self)
            try currentSheet.setCell(row: target.row, column: target.col, rawValue: str)
          }
        }
        pendingFormulaCoord = nil

      case biff8OpcodeNumber:
        pendingFormulaCoord = nil
        guard length == 14 else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 NUMBER 레코드 길이 오류: \(length)")
        }
        let row = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart))
        let col = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 2))
        let bits = try BitPacking.readUInt64LE(from: workbook, at: payloadStart + 6)
        let num = Double(bitPattern: bits)
        try currentSheet.setCell(row: row, column: col, rawValue: formatBIFF8Numeric(num))

      case biff8OpcodeLabel:
        pendingFormulaCoord = nil
        guard length >= 8 else {
          throw EasyOfficeError.corruptedContainer(detail: "BIFF8 LABEL 레코드 길이 오류: \(length)")
        }
        let row = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart))
        let col = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 2))
        let charCount = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 6))
        let str: String
        if payloadStart + 9 <= payloadEnd {
          let grbit = workbook[workbook.startIndex + payloadStart + 8]
          if (grbit & 0x01) != 0, payloadStart + 9 + charCount * 2 == payloadEnd {
            str = try BitPacking.decodeUTF16LE(
              from: workbook, at: payloadStart + 9, byteLength: charCount * 2)
          } else if grbit == 0x00, payloadStart + 9 + charCount == payloadEnd {
            let slice = workbook[
              (workbook.startIndex + payloadStart + 9)..<(workbook.startIndex + payloadEnd)]
            str = String(decoding: slice.map { UInt16($0) }, as: UTF16.self)
          } else {
            let byteLen = charCount * 2
            guard payloadStart + 8 + byteLen <= payloadEnd else {
              throw EasyOfficeError.corruptedContainer(detail: "BIFF8 LABEL 문자열 길이가 레코드 경계를 초과합니다.")
            }
            str = try BitPacking.decodeUTF16LE(
              from: workbook, at: payloadStart + 8, byteLength: byteLen)
          }
        } else {
          let byteLen = charCount * 2
          guard payloadStart + 8 + byteLen <= payloadEnd else {
            throw EasyOfficeError.corruptedContainer(detail: "BIFF8 LABEL 문자열 길이가 레코드 경계를 초과합니다.")
          }
          str = try BitPacking.decodeUTF16LE(
            from: workbook, at: payloadStart + 8, byteLength: byteLen)
        }
        try currentSheet.setCell(row: row, column: col, rawValue: str)

      case biff8OpcodeExtendedMeta:
        extendedMetaBytes.append(
          workbook[(workbook.startIndex + payloadStart)..<(workbook.startIndex + payloadEnd)]
        )

      case biff8OpcodeEOF:
        pendingFormulaCoord = nil
        if currentSubstreamType == 0x0010 && inWorksheetSubstream {
          if currentSheetIndex < boundSheetNames.count, !boundSheetNames[currentSheetIndex].isEmpty
          {
            currentSheet.name = boundSheetNames[currentSheetIndex]
          }
          completedSheets.append(currentSheet)
          currentSheetIndex += 1
          inWorksheetSubstream = false
        }

      default:
        break
      }

      offset = payloadEnd
    }

    if inWorksheetSubstream {
      if currentSheetIndex < boundSheetNames.count, !boundSheetNames[currentSheetIndex].isEmpty {
        currentSheet.name = boundSheetNames[currentSheetIndex]
      }
      completedSheets.append(currentSheet)
    }

    var result = completedSheets.first ?? currentSheet
    if completedSheets.count > 1 {
      result.additionalSheets = completedSheets.dropFirst().map { page in
        WorksheetSheetPage(
          name: page.name,
          cellsByCoordinate: page.cellsByCoordinate,
          cellStyles: page.cellStyles,
          sheetComments: page.sheetComments,
          mergedRanges: page.mergedRanges
        )
      }
    }
    if !extendedMetaBytes.isEmpty,
      let ext = try? JSONDecoder().decode(BIFF8ExtendedMetadataEnvelope.self, from: extendedMetaBytes)
    {
      ext.apply(to: &result)
    }
    return result
  }

  // MARK: 3. Presentation Binary (.ppt) — RT_Document (1000) / RT_Slide (1006) / RT_SlideAtom (1007) / RT_TextCharsAtom (4000) / RT_TextBytesAtom (4008)

  public static let pptRecordDocument: UInt16 = 1000
  public static let pptRecordSlide: UInt16 = 1006
  public static let pptRecordSlideAtom: UInt16 = 1007
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
    var slideListTexts: [String] = []

    func decodePPTEightBitText(start: Int, end: Int) -> String {
      let slice = Data(
        pptStream[(pptStream.startIndex + start)..<(pptStream.startIndex + end)])
      return String(data: slice, encoding: .windowsCP1252)
        ?? String(decoding: slice, as: UTF8.self)
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
            slideListTexts.append(text)
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

    if slides.isEmpty && !slideListTexts.isEmpty {
      slides.append(
        SlideModel(
          title: slideListTexts[0],
          bulletPoints: Array(slideListTexts.dropFirst())
        ))
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
