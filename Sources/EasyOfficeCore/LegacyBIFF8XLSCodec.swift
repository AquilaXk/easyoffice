import Foundation


internal struct BIFF8ExtendedMetadataEnvelope: Codable, Sendable {
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

extension LegacyOLE2OfficeCodec {
  public static let biff8OpcodeFormula: UInt16 = 0x0006
  public static let biff8OpcodeEOF: UInt16 = 0x000A
  public static let biff8OpcodeFilePass: UInt16 = 0x002F
  public static let biff8OpcodeFont: UInt16 = 0x0031
  public static let biff8OpcodeContinue: UInt16 = 0x003C
  public static let biff8OpcodeBoundSheet: UInt16 = 0x0085
  public static let biff8OpcodeMulRK: UInt16 = 0x00BD
  public static let biff8OpcodeXF: UInt16 = 0x00E0
  public static let biff8OpcodeSST: UInt16 = 0x00FC
  public static let biff8OpcodeLabelSST: UInt16 = 0x00FD
  public static let biff8OpcodeDimensions: UInt16 = 0x0200
  public static let biff8OpcodeNumber: UInt16 = 0x0203
  public static let biff8OpcodeLabel: UInt16 = 0x0204
  public static let biff8OpcodeString: UInt16 = 0x0207
  public static let biff8OpcodeRK: UInt16 = 0x027E
  public static let biff8OpcodeBOF: UInt16 = 0x0809
  public static let biff8OpcodeExtendedMeta: UInt16 = 0x0868

  public static let maxBIFF8Row = 65_535
  public static let maxBIFF8Col = 255
  public static let maxBIFF8Rows = maxBIFF8Row + 1
  public static let maxBIFF8Columns = maxBIFF8Col + 1
  public static let maxBIFF8RecordPayload = 8_224

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

  private static func decodeLatin1Slice(_ data: Data, start: Int, count: Int) -> String {
    guard count > 0, start >= 0, start + count <= data.count else { return "" }
    let slice = data[(data.startIndex + start)..<(data.startIndex + start + count)]
    return String(decoding: slice.map { UInt16($0) }, as: UTF16.self)
  }

  public static func encodeBIFF8Worksheet(_ sheet: WorksheetModel) throws -> Data {
    let allPages: [(name: String, cells: [PackedCellCoordinate: String])] =
      [(sheet.name, sheet.cellsByCoordinate)]
      + sheet.additionalSheets.map { ($0.name, $0.cellsByCoordinate) }

    var sstStrings: [String] = []
    var sstIndexByString: [String: UInt32] = [:]
    var totalSSTReferences: UInt32 = 0

    for page in allPages {
      let nameUnitsCount = page.name.utf16.count
      guard nameUnitsCount <= 255 else {
        throw EasyOfficeError.resourceBudgetExceeded(
          detail: "BIFF8 시트 이름 길이(\(nameUnitsCount))가 255 UTF-16 코드 유닛 한도를 초과했습니다.")
      }
      let sortedCoords = page.cells.keys.sorted()
      for coord in sortedCoords {
        guard coord.row >= 0, coord.row <= maxBIFF8Row,
          coord.column >= 0, coord.column <= maxBIFF8Col
        else {
          throw EasyOfficeError.resourceBudgetExceeded(
            detail:
              "BIFF8(.xls) 워크시트 좌표(\(coord.row), \(coord.column))가 65,536행×256열 규격 한도를 초과했습니다."
          )
        }
        guard let raw = page.cells[coord], !raw.isEmpty else { continue }
        if !raw.hasPrefix("="), let num = Double(raw), num.isFinite {
          continue
        }
        if raw.utf16.count > 32_765 {
          throw EasyOfficeError.resourceBudgetExceeded(
            detail: "BIFF8 셀 문자열 길이가 32,765 UTF-16 코드 유닛 한도를 초과했습니다.")
        }
        let sstValue = raw
        totalSSTReferences &+= 1
        if sstIndexByString[sstValue] == nil {
          let newIndex = UInt32(sstStrings.count)
          sstIndexByString[sstValue] = newIndex
          sstStrings.append(sstValue)
        }
      }
    }

    var stream = Data()

    BitPacking.appendUInt16LE(biff8OpcodeBOF, to: &stream)
    BitPacking.appendUInt16LE(16, to: &stream)
    BitPacking.appendUInt16LE(0x0600, to: &stream)  // BIFF8 version
    BitPacking.appendUInt16LE(0x0005, to: &stream)  // Workbook Globals substream
    BitPacking.appendUInt16LE(0x0DBB, to: &stream)  // build ID
    BitPacking.appendUInt16LE(0x07CC, to: &stream)  // build year
    BitPacking.appendUInt32LE(0, to: &stream)
    BitPacking.appendUInt32LE(0x0006, to: &stream)

    var fontPayload = Data()
    BitPacking.appendUInt16LE(200, to: &fontPayload)  // dyHeight = 10pt
    BitPacking.appendUInt16LE(0, to: &fontPayload)  // grbit
    BitPacking.appendUInt16LE(0x7FFF, to: &fontPayload)  // icv
    BitPacking.appendUInt16LE(400, to: &fontPayload)  // bls (normal weight)
    BitPacking.appendUInt16LE(0, to: &fontPayload)  // sss
    fontPayload.append(0)  // uls
    fontPayload.append(0)  // bFamily
    fontPayload.append(0)  // bCharSet
    fontPayload.append(0)  // reserved
    fontPayload.append(5)  // cch = 5
    fontPayload.append(0)  // grbit = 0 (8-bit Latin-1)
    fontPayload.append(contentsOf: "Arial".utf8)
    BitPacking.appendUInt16LE(biff8OpcodeFont, to: &stream)
    BitPacking.appendUInt16LE(UInt16(fontPayload.count), to: &stream)
    stream.append(fontPayload)

    var xfPayload = Data(repeating: 0, count: 20)
    try BitPacking.writeUInt16LE(0, into: &xfPayload, at: 0)  // ifnt = 0
    try BitPacking.writeUInt16LE(0, into: &xfPayload, at: 2)  // ifmt = 0
    try BitPacking.writeUInt16LE(0x0001, into: &xfPayload, at: 4)  // fLocked = 1
    BitPacking.appendUInt16LE(biff8OpcodeXF, to: &stream)
    BitPacking.appendUInt16LE(UInt16(xfPayload.count), to: &stream)
    stream.append(xfPayload)

    var lbPlyPosOffsets: [Int] = []
    for page in allPages {
      let safeSheetName = page.name.isEmpty ? "Sheet1" : page.name
      let nameBytes = BitPacking.encodeUTF16LE(safeSheetName)
      let nameUnitsCount = safeSheetName.utf16.count
      let boundSheetLen = UInt16(8 + nameBytes.count)
      BitPacking.appendUInt16LE(biff8OpcodeBoundSheet, to: &stream)
      BitPacking.appendUInt16LE(boundSheetLen, to: &stream)
      lbPlyPosOffsets.append(stream.count)
      BitPacking.appendUInt32LE(0, to: &stream)  // lbPlyPos (patched below)
      stream.append(0)  // hsState: visible
      stream.append(0)  // dt: worksheet
      stream.append(UInt8(nameUnitsCount))
      stream.append(0x01)  // grbit = 0x01 (UTF-16LE)
      stream.append(nameBytes)
    }

    if !sstStrings.isEmpty {
      try appendBIFF8SharedStringTable(
        sstStrings: sstStrings,
        totalReferences: totalSSTReferences,
        to: &stream
      )
    }

    BitPacking.appendUInt16LE(biff8OpcodeEOF, to: &stream)
    BitPacking.appendUInt16LE(0, to: &stream)

    for (pageIdx, page) in allPages.enumerated() {
      let worksheetSubstreamOffset = try BitPacking.checkedUInt32(
        stream.count, field: "BIFF8 BOUNDSHEET lbPlyPos")
      try BitPacking.writeUInt32LE(
        worksheetSubstreamOffset, into: &stream, at: lbPlyPosOffsets[pageIdx])

      // Worksheet BOF record (0x0809, length 16, dt = 0x0010)
      BitPacking.appendUInt16LE(biff8OpcodeBOF, to: &stream)
      BitPacking.appendUInt16LE(16, to: &stream)
      BitPacking.appendUInt16LE(0x0600, to: &stream)
      BitPacking.appendUInt16LE(0x0010, to: &stream)
      BitPacking.appendUInt16LE(0x0DBB, to: &stream)
      BitPacking.appendUInt16LE(0x07CC, to: &stream)
      BitPacking.appendUInt32LE(0, to: &stream)
      BitPacking.appendUInt32LE(0x0006, to: &stream)

      let sortedCoords = page.cells.keys.sorted()
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

      for coord in sortedCoords {
        guard let raw = page.cells[coord], !raw.isEmpty else { continue }
        let row16 = UInt16(coord.row)
        let col16 = UInt16(coord.column)

        if !raw.hasPrefix("="), let num = Double(raw), num.isFinite {
          BitPacking.appendUInt16LE(biff8OpcodeNumber, to: &stream)
          BitPacking.appendUInt16LE(14, to: &stream)
          BitPacking.appendUInt16LE(row16, to: &stream)
          BitPacking.appendUInt16LE(col16, to: &stream)
          BitPacking.appendUInt16LE(0, to: &stream)
          BitPacking.appendUInt64LE(num.bitPattern, to: &stream)
        } else {
          let sstValue =
            raw.utf16.count > 32_765
            ? String(decoding: raw.utf16.prefix(32_765), as: UTF16.self) : raw
          if let sstIndex = sstIndexByString[sstValue] {
            BitPacking.appendUInt16LE(biff8OpcodeLabelSST, to: &stream)
            BitPacking.appendUInt16LE(10, to: &stream)
            BitPacking.appendUInt16LE(row16, to: &stream)
            BitPacking.appendUInt16LE(col16, to: &stream)
            BitPacking.appendUInt16LE(0, to: &stream)
            BitPacking.appendUInt32LE(sstIndex, to: &stream)
          }
        }
      }

      if pageIdx == 0 {
        let extEnvelope = BIFF8ExtendedMetadataEnvelope(from: sheet)
        if let extBytes = try? JSONEncoder().encode(extEnvelope), !extBytes.isEmpty {
          var chunkOffset = 0
          while chunkOffset < extBytes.count {
            let chunkLen = min(maxBIFF8RecordPayload, extBytes.count - chunkOffset)
            BitPacking.appendUInt16LE(biff8OpcodeExtendedMeta, to: &stream)
            BitPacking.appendUInt16LE(UInt16(chunkLen), to: &stream)
            let startIdx = extBytes.startIndex + chunkOffset
            stream.append(extBytes[startIdx..<(startIdx + chunkLen)])
            chunkOffset += chunkLen
          }
        }
      }

      // Worksheet EOF record (0x000A, length 0)
      BitPacking.appendUInt16LE(biff8OpcodeEOF, to: &stream)
      BitPacking.appendUInt16LE(0, to: &stream)
    }

    return try OLE2ContainerCodec.pack(streams: [
      OLE2StreamEntry(name: "Workbook", data: stream)
    ])
  }

  private static func appendBIFF8SharedStringTable(
    sstStrings: [String],
    totalReferences: UInt32,
    to stream: inout Data
  ) throws {
    var currentOpcode = biff8OpcodeSST
    var currentPayload = Data()
    currentPayload.reserveCapacity(min(maxBIFF8RecordPayload, sstStrings.count * 16 + 8))
    BitPacking.appendUInt32LE(totalReferences, to: &currentPayload)
    BitPacking.appendUInt32LE(UInt32(sstStrings.count), to: &currentPayload)

    func flushCurrentRecord() {
      BitPacking.appendUInt16LE(currentOpcode, to: &stream)
      BitPacking.appendUInt16LE(UInt16(currentPayload.count), to: &stream)
      stream.append(currentPayload)
      currentOpcode = biff8OpcodeContinue
      currentPayload.removeAll(keepingCapacity: true)
    }

    for str in sstStrings {
      let utf16Units = Array(str.utf16)
      let cch = utf16Units.count
      let neededHeaderAndFirstChar = cch > 0 ? 5 : 3
      if currentPayload.count + neededHeaderAndFirstChar > maxBIFF8RecordPayload {
        flushCurrentRecord()
      }
      BitPacking.appendUInt16LE(UInt16(cch), to: &currentPayload)
      currentPayload.append(0x01)  // grbit = 0x01 (UTF-16LE)

      var unitIndex = 0
      while unitIndex < cch {
        if currentPayload.count + 2 > maxBIFF8RecordPayload {
          flushCurrentRecord()
          currentPayload.append(0x01)  // continuation grbit = 0x01 (UTF-16LE)
        }
        let availUnits = (maxBIFF8RecordPayload - currentPayload.count) / 2
        let take = min(cch - unitIndex, availUnits)
        for i in 0..<take {
          BitPacking.appendUInt16LE(utf16Units[unitIndex + i], to: &currentPayload)
        }
        unitIndex += take
      }
    }

    if !currentPayload.isEmpty {
      flushCurrentRecord()
    }
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
      var payloadEnd = payloadStart + length
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
          } else if (grbitByte & 0x01) == 0, remBytes >= cchByte, cchByte > 0 {
            decodedName = decodeLatin1Slice(workbook, start: payloadStart + 8, count: cchByte)
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
        var sstChunks: [Data] = [
          Data(workbook[(workbook.startIndex + payloadStart)..<(workbook.startIndex + payloadEnd)])
        ]
        while payloadEnd <= workbook.count - 4 {
          let nextOp = try BitPacking.readUInt16LE(from: workbook, at: payloadEnd)
          guard nextOp == biff8OpcodeContinue else { break }
          let nextLen = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadEnd + 2))
          let nextStart = payloadEnd + 4
          let nextEnd = nextStart + nextLen
          guard nextLen >= 0, nextEnd <= workbook.count else {
            throw EasyOfficeError.corruptedContainer(detail: "BIFF8 CONTINUE 레코드 길이가 스트림 경계를 초과합니다.")
          }
          sstChunks.append(
            Data(workbook[(workbook.startIndex + nextStart)..<(workbook.startIndex + nextEnd)])
          )
          payloadEnd = nextEnd
        }
        sstStrings = try parseBIFF8SSTChunks(sstChunks)

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
            let str = decodeLatin1Slice(workbook, start: sCursor, count: charCount)
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
          if (grbit & 0x01) != 0, payloadStart + 9 + charCount * 2 <= payloadEnd {
            str = try BitPacking.decodeUTF16LE(
              from: workbook, at: payloadStart + 9, byteLength: charCount * 2)
          } else if (grbit & 0x01) == 0, payloadStart + 9 + charCount <= payloadEnd {
            str = decodeLatin1Slice(workbook, start: payloadStart + 9, count: charCount)
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

  private static func parseBIFF8SSTChunks(_ chunks: [Data]) throws -> [String] {
    guard let firstChunk = chunks.first, firstChunk.count >= 8 else { return [] }
    let uniqueCount = Int(try BitPacking.readUInt32LE(from: firstChunk, at: 4))
    let cappedCount = min(max(0, uniqueCount), 262_144)
    var results: [String] = []
    results.reserveCapacity(min(cappedCount, 16_384))

    var chunkIdx = 0
    var pos = 8

    func ensureRemainingBytes() -> Bool {
      while chunkIdx < chunks.count && pos >= chunks[chunkIdx].count {
        chunkIdx += 1
        pos = 0
      }
      return chunkIdx < chunks.count
    }

    func readRawBytes(_ count: Int) -> Data? {
      var out = Data()
      out.reserveCapacity(count)
      var rem = count
      while rem > 0 {
        guard ensureRemainingBytes() else { return nil }
        let chunk = chunks[chunkIdx]
        let take = min(rem, chunk.count - pos)
        out.append(chunk[(chunk.startIndex + pos)..<(chunk.startIndex + pos + take)])
        pos += take
        rem -= take
      }
      return out
    }

    func skipRawBytes(_ count: Int) {
      var rem = count
      while rem > 0 && ensureRemainingBytes() {
        let take = min(rem, chunks[chunkIdx].count - pos)
        pos += take
        rem -= take
      }
    }

    for _ in 0..<cappedCount {
      guard ensureRemainingBytes() else { break }
      guard let headerBytes = readRawBytes(3), headerBytes.count == 3 else { break }
      let charCount = Int(try BitPacking.readUInt16LE(from: headerBytes, at: 0))
      let flags = headerBytes[headerBytes.startIndex + 2]

      var richRunCount = 0
      var extRstByteLen = 0
      if (flags & 0x08) != 0 {
        guard let rBytes = readRawBytes(2), rBytes.count == 2 else { break }
        richRunCount = Int(try BitPacking.readUInt16LE(from: rBytes, at: 0))
      }
      if (flags & 0x04) != 0 {
        guard let eBytes = readRawBytes(4), eBytes.count == 4 else { break }
        extRstByteLen = Int(try BitPacking.readUInt32LE(from: eBytes, at: 0))
      }

      var isHighByte = (flags & 0x01) != 0
      var remainingChars = charCount
      var decoded = ""

      while remainingChars > 0 {
        if pos >= chunks[chunkIdx].count {
          chunkIdx += 1
          pos = 0
          while chunkIdx < chunks.count && chunks[chunkIdx].isEmpty {
            chunkIdx += 1
          }
          guard chunkIdx < chunks.count else { break }
          let contGrbit = chunks[chunkIdx][chunks[chunkIdx].startIndex]
          pos = 1
          isHighByte = (contGrbit & 0x01) != 0
        }
        let chunk = chunks[chunkIdx]
        let availBytes = chunk.count - pos
        if isHighByte {
          let takeChars = min(remainingChars, availBytes / 2)
          guard takeChars > 0 else { break }
          let piece = try BitPacking.decodeUTF16LE(
            from: chunk, at: pos, byteLength: takeChars * 2)
          decoded.append(piece)
          pos += takeChars * 2
          remainingChars -= takeChars
        } else {
          let takeChars = min(remainingChars, availBytes)
          guard takeChars > 0 else { break }
          let piece = decodeLatin1Slice(chunk, start: pos, count: takeChars)
          decoded.append(piece)
          pos += takeChars
          remainingChars -= takeChars
        }
      }

      results.append(decoded)
      let extraBytes = richRunCount * 4 + max(0, extRstByteLen)
      if extraBytes > 0 {
        skipRawBytes(extraBytes)
      }
    }

    return results
  }
}
