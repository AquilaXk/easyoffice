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
        caption: t.caption,
        rows: t.rows,
        hasHeaderRow: t.hasHeaderRow,
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

// MARK: - OLE2 Compound File Binary Format (CFBF) Container Codec

/// Named stream inside an OLE2 Compound File Binary container.
public struct OLE2StreamEntry: Sendable, Equatable {
  public let name: String
  public let data: Data

  public init(name: String, data: Data) {
    self.name = name
    self.data = data
  }
}

/// OLE2 FAT sector chain resolver using a compact `UInt64` bitset (`64` sectors per word) for $O(1)$ cycle and sharing detection.
public struct OLE2SectorTable: Sendable, Equatable {
  public static let endOfChain: UInt32 = 0xFFFF_FFFE
  public static let freeSector: UInt32 = 0xFFFF_FFFF

  public let entries: [UInt32]

  public init(entries: [UInt32]) {
    self.entries = entries
  }

  public func chain(
    startingAt startSector: UInt32,
    maxSectors: Int = 65_536
  ) throws -> [UInt32] {
    guard maxSectors > 0 else { return [] }
    if startSector == OLE2ContainerCodec.endOfChain { return [] }
    let wordCount = max(1, (max(entries.count, maxSectors) + 63) >> 6)
    var visitedWords = [UInt64](repeating: 0, count: wordCount)
    var result: [UInt32] = []
    result.reserveCapacity(min(entries.count, 64))
    var current = startSector

    while current != OLE2ContainerCodec.endOfChain {
      let idx = Int(current)
      guard idx >= 0, idx < entries.count, result.count < maxSectors else {
        throw EasyOfficeError.corruptedContainer(
          detail: "OLE2 FAT 체인이 유효하지 않은 섹터(\(current))를 참조하거나 최대 섹터 한도(\(maxSectors))를 초과했습니다."
        )
      }
      let wordIdx = idx >> 6
      let mask: UInt64 = 1 << UInt64(idx & 63)
      if (visitedWords[wordIdx] & mask) != 0 {
        throw EasyOfficeError.corruptedContainer(
          detail: "OLE2 sector cycle or cross-stream sector sharing detected"
        )
      }
      visitedWords[wordIdx] |= mask
      result.append(current)
      current = entries[idx]
    }
    return result
  }
}

/// 512-byte sector OLE2 Compound File Binary Format (`D0 CF 11 E0 A1 B1 1A E1`) container codec.
public enum OLE2ContainerCodec {
  public static let sectorSize = 512
  public static let miniSectorSize = 64
  public static let miniStreamCutoffSize = 4096
  public static let directoryEntrySize = 128
  public static let entriesPerSector = sectorSize / directoryEntrySize  // 4
  public static let fatEntriesPerSector = sectorSize / 4  // 128

  public static let freeSector: UInt32 = 0xFFFF_FFFF
  public static let endOfChain: UInt32 = 0xFFFF_FFFE
  public static let fatSectorMarker: UInt32 = 0xFFFF_FFFD
  public static let maxTotalExtractedBytes = 64 * 1024 * 1024

  private static let objectTypeUnallocated: UInt8 = 0
  private static let objectTypeStorage: UInt8 = 1
  private static let objectTypeStream: UInt8 = 2
  private static let objectTypeRootStorage: UInt8 = 5

  private static func compareCFBNames(_ lhs: String, _ rhs: String) -> Bool {
    let lLen = lhs.utf16.count
    let rLen = rhs.utf16.count
    if lLen != rLen { return lLen < rLen }
    let lUpper = Array(lhs.uppercased().utf16)
    let rUpper = Array(rhs.uppercased().utf16)
    return lUpper.lexicographicallyPrecedes(rUpper)
  }

  private static func buildBalancedDirectoryBST(
    sortedEntryIDs: [Int],
    range: Range<Int>,
    depth: Int,
    maxDepth: Int,
    links: inout [Int: (left: UInt32, right: UInt32, color: UInt8)]
  ) -> UInt32 {
    guard !range.isEmpty else { return freeSector }
    let mid = range.lowerBound + (range.count / 2)
    let entryID = sortedEntryIDs[mid]
    let leftChild = buildBalancedDirectoryBST(
      sortedEntryIDs: sortedEntryIDs,
      range: range.lowerBound..<mid,
      depth: depth + 1,
      maxDepth: maxDepth,
      links: &links
    )
    let rightChild = buildBalancedDirectoryBST(
      sortedEntryIDs: sortedEntryIDs,
      range: (mid + 1)..<range.upperBound,
      depth: depth + 1,
      maxDepth: maxDepth,
      links: &links
    )
    let color: UInt8 = (depth > 0 && depth == maxDepth) ? 0 : 1  // 0 = red on deepest incomplete level, 1 = black
    links[entryID] = (left: leftChild, right: rightChild, color: color)
    return UInt32(entryID)
  }

  private static func treeHeight(for count: Int) -> Int {
    guard count > 1 else { return 0 }
    var h = 0
    var n = count
    while n > 1 {
      n >>= 1
      h += 1
    }
    return h
  }

  public static func pack(streams: [OLE2StreamEntry]) throws -> Data {
    // Decompose hierarchical stream names ("Storage/Stream") into storage + leaf stream
    var storageNames: [String] = []
    var streamLeafNames: [String] = []
    var streamParentStorageName: [String?] = []

    for stream in streams {
      if let slashIdx = stream.name.firstIndex(of: "/") {
        let storageName = String(stream.name[..<slashIdx])
        let leafName = String(stream.name[stream.name.index(after: slashIdx)...])
        guard !storageName.isEmpty, !leafName.isEmpty, !leafName.contains("/") else {
          throw EasyOfficeError.corruptedContainer(
            detail: "유효하지 않은 OLE2 계층 스트림 경로: \(stream.name)")
        }
        if !storageNames.contains(storageName) {
          storageNames.append(storageName)
        }
        streamLeafNames.append(leafName)
        streamParentStorageName.append(storageName)
      } else {
        streamLeafNames.append(stream.name)
        streamParentStorageName.append(nil)
      }
    }

    // Entry 0: Root Entry
    // Entries 1..streams.count: Stream entries
    // Entries (streams.count + 1)..<(1 + streams.count + storageNames.count): Storage entries
    let totalDirEntries = 1 + streams.count + storageNames.count
    let dirSectorCount = max(1, (totalDirEntries + entriesPerSector - 1) / entriesPerSector)

    var nextSectorIndex = UInt32(dirSectorCount)
    var streamStartSectors: [UInt32] = Array(repeating: endOfChain, count: streams.count)
    var regularStreamSectorCounts: [Int] = Array(repeating: 0, count: streams.count)

    var miniStreamBuffer = Data()
    var miniFat: [UInt32] = []
    var nextMiniSectorIndex: UInt32 = 0

    for (idx, stream) in streams.enumerated() {
      if stream.data.isEmpty {
        streamStartSectors[idx] = endOfChain
      } else if stream.data.count < miniStreamCutoffSize {
        let miniCount = (stream.data.count + miniSectorSize - 1) / miniSectorSize
        streamStartSectors[idx] = nextMiniSectorIndex
        for offset in 0..<miniCount {
          let current = nextMiniSectorIndex + UInt32(offset)
          miniFat.append((offset + 1 < miniCount) ? (current + 1) : endOfChain)
        }
        nextMiniSectorIndex += UInt32(miniCount)
        miniStreamBuffer.append(stream.data)
        let rem = stream.data.count % miniSectorSize
        if rem != 0 {
          miniStreamBuffer.append(Data(repeating: 0, count: miniSectorSize - rem))
        }
      } else {
        let count = (stream.data.count + sectorSize - 1) / sectorSize
        streamStartSectors[idx] = nextSectorIndex
        regularStreamSectorCounts[idx] = count
        nextSectorIndex += UInt32(count)
      }
    }

    var firstMiniFatSector: UInt32 = endOfChain
    var miniFatSectorCount = 0
    var rootStartSector: UInt32 = endOfChain
    var rootMiniStreamSectorCount = 0

    if !miniStreamBuffer.isEmpty {
      let remMiniFat = miniFat.count % fatEntriesPerSector
      if remMiniFat != 0 {
        miniFat.append(contentsOf: repeatElement(freeSector, count: fatEntriesPerSector - remMiniFat))
      }
      miniFatSectorCount = max(1, miniFat.count / fatEntriesPerSector)
      firstMiniFatSector = nextSectorIndex
      nextSectorIndex += UInt32(miniFatSectorCount)

      rootMiniStreamSectorCount = (miniStreamBuffer.count + sectorSize - 1) / sectorSize
      rootStartSector = nextSectorIndex
      nextSectorIndex += UInt32(rootMiniStreamSectorCount)
    }

    let contentSectorCount = Int(nextSectorIndex)
    let fatSectorCount = max(
      1, (contentSectorCount + fatEntriesPerSector - 1) / fatEntriesPerSector)
    guard fatSectorCount <= 109 else {
      throw EasyOfficeError.corruptedContainer(detail: "OLE2 DIFAT 109 섹터 한도를 초과했습니다.")
    }

    let firstFatSectorIndex = Int(nextSectorIndex)
    let totalSectors = contentSectorCount + fatSectorCount

    var fat: [UInt32] = Array(repeating: freeSector, count: fatSectorCount * fatEntriesPerSector)

    for sec in 0..<dirSectorCount {
      fat[sec] = (sec + 1 < dirSectorCount) ? UInt32(sec + 1) : endOfChain
    }

    for idx in streams.indices {
      let count = regularStreamSectorCounts[idx]
      guard count > 0 else { continue }
      let start = Int(streamStartSectors[idx])
      for offset in 0..<count {
        let current = start + offset
        fat[current] = (offset + 1 < count) ? UInt32(current + 1) : endOfChain
      }
    }

    if miniFatSectorCount > 0 {
      let start = Int(firstMiniFatSector)
      for offset in 0..<miniFatSectorCount {
        let current = start + offset
        fat[current] = (offset + 1 < miniFatSectorCount) ? UInt32(current + 1) : endOfChain
      }
    }

    if rootMiniStreamSectorCount > 0 {
      let start = Int(rootStartSector)
      for offset in 0..<rootMiniStreamSectorCount {
        let current = start + offset
        fat[current] = (offset + 1 < rootMiniStreamSectorCount) ? UInt32(current + 1) : endOfChain
      }
    }

    for fIdx in 0..<fatSectorCount {
      fat[firstFatSectorIndex + fIdx] = fatSectorMarker
    }

    var container = Data(repeating: 0, count: sectorSize)
    for (i, byte) in FormatDetector.ole2Magic.enumerated() {
      container[i] = byte
    }
    try BitPacking.writeUInt16LE(0x003E, into: &container, at: 24)
    try BitPacking.writeUInt16LE(0x0003, into: &container, at: 26)
    try BitPacking.writeUInt16LE(0xFFFE, into: &container, at: 28)
    try BitPacking.writeUInt16LE(0x0009, into: &container, at: 30)
    try BitPacking.writeUInt16LE(0x0006, into: &container, at: 32)
    try BitPacking.writeUInt32LE(UInt32(fatSectorCount), into: &container, at: 44)
    try BitPacking.writeUInt32LE(0, into: &container, at: 48)
    try BitPacking.writeUInt32LE(UInt32(miniStreamCutoffSize), into: &container, at: 56)
    try BitPacking.writeUInt32LE(firstMiniFatSector, into: &container, at: 60)
    try BitPacking.writeUInt32LE(UInt32(miniFatSectorCount), into: &container, at: 64)
    try BitPacking.writeUInt32LE(endOfChain, into: &container, at: 68)
    try BitPacking.writeUInt32LE(0, into: &container, at: 72)

    for slot in 0..<109 {
      let value = (slot < fatSectorCount) ? UInt32(firstFatSectorIndex + slot) : freeSector
      try BitPacking.writeUInt32LE(value, into: &container, at: 76 + slot * 4)
    }

    // Build Red-Black BST links for Root Entry children and each Storage's children
    var entryNamesByID: [Int: String] = [0: "Root Entry"]
    var rootChildEntryIDs: [Int] = []
    var storageChildEntryIDs: [String: [Int]] = [:]

    for sIdx in streams.indices {
      let entryID = sIdx + 1
      entryNamesByID[entryID] = streamLeafNames[sIdx]
      if let storageName = streamParentStorageName[sIdx] {
        storageChildEntryIDs[storageName, default: []].append(entryID)
      } else {
        rootChildEntryIDs.append(entryID)
      }
    }
    for (stIdx, stName) in storageNames.enumerated() {
      let entryID = 1 + streams.count + stIdx
      entryNamesByID[entryID] = stName
      rootChildEntryIDs.append(entryID)
    }

    var bstLinks: [Int: (left: UInt32, right: UInt32, color: UInt8)] = [:]
    let sortedRootChildren = rootChildEntryIDs.sorted {
      compareCFBNames(entryNamesByID[$0] ?? "", entryNamesByID[$1] ?? "")
    }
    let rootChildID = buildBalancedDirectoryBST(
      sortedEntryIDs: sortedRootChildren,
      range: 0..<sortedRootChildren.count,
      depth: 0,
      maxDepth: treeHeight(for: sortedRootChildren.count),
      links: &bstLinks
    )

    var storageRootChildByIndex: [Int: UInt32] = [:]
    for (stIdx, stName) in storageNames.enumerated() {
      let children = (storageChildEntryIDs[stName] ?? []).sorted {
        compareCFBNames(entryNamesByID[$0] ?? "", entryNamesByID[$1] ?? "")
      }
      storageRootChildByIndex[stIdx] = buildBalancedDirectoryBST(
        sortedEntryIDs: children,
        range: 0..<children.count,
        depth: 0,
        maxDepth: treeHeight(for: children.count),
        links: &bstLinks
      )
    }

    var directoryBytes = Data(repeating: 0, count: dirSectorCount * sectorSize)
    try writeDirectoryEntry(
      into: &directoryBytes,
      entryIndex: 0,
      name: "Root Entry",
      objectType: objectTypeRootStorage,
      colorFlag: 1,
      leftSiblingID: freeSector,
      rightSiblingID: freeSector,
      childID: rootChildID,
      startSector: rootStartSector,
      streamSize: UInt64(miniStreamBuffer.count)
    )

    for sIdx in streams.indices {
      let entryIndex = sIdx + 1
      let link = bstLinks[entryIndex] ?? (left: freeSector, right: freeSector, color: 1)
      try writeDirectoryEntry(
        into: &directoryBytes,
        entryIndex: entryIndex,
        name: streamLeafNames[sIdx],
        objectType: objectTypeStream,
        colorFlag: link.color,
        leftSiblingID: link.left,
        rightSiblingID: link.right,
        childID: freeSector,
        startSector: streamStartSectors[sIdx],
        streamSize: UInt64(streams[sIdx].data.count)
      )
    }

    for (stIdx, stName) in storageNames.enumerated() {
      let entryIndex = 1 + streams.count + stIdx
      let link = bstLinks[entryIndex] ?? (left: freeSector, right: freeSector, color: 1)
      try writeDirectoryEntry(
        into: &directoryBytes,
        entryIndex: entryIndex,
        name: stName,
        objectType: objectTypeStorage,
        colorFlag: link.color,
        leftSiblingID: link.left,
        rightSiblingID: link.right,
        childID: storageRootChildByIndex[stIdx] ?? freeSector,
        startSector: 0,
        streamSize: 0
      )
    }

    container.append(directoryBytes)

    // Append regular streams (>= 4096 bytes)
    for (idx, stream) in streams.enumerated() where regularStreamSectorCounts[idx] > 0 {
      container.append(stream.data)
      let remainder = stream.data.count % sectorSize
      if remainder != 0 {
        container.append(Data(repeating: 0, count: sectorSize - remainder))
      }
    }

    // Append MiniFAT sectors and Root Entry MiniStream sectors if present
    if miniFatSectorCount > 0 {
      var miniFatBytes = Data(count: miniFat.count * 4)
      miniFatBytes.withUnsafeMutableBytes { rawBuffer in
        for (idx, entry) in miniFat.enumerated() {
          rawBuffer.storeBytes(of: entry.littleEndian, toByteOffset: idx * 4, as: UInt32.self)
        }
      }
      container.append(miniFatBytes)

      container.append(miniStreamBuffer)
      let rem = miniStreamBuffer.count % sectorSize
      if rem != 0 {
        container.append(Data(repeating: 0, count: sectorSize - rem))
      }
    }

    var fatBytes = Data(count: fat.count * 4)
    fatBytes.withUnsafeMutableBytes { rawBuffer in
      for (idx, entry) in fat.enumerated() {
        rawBuffer.storeBytes(of: entry.littleEndian, toByteOffset: idx * 4, as: UInt32.self)
      }
    }
    container.append(fatBytes)

    assert(container.count == (1 + totalSectors) * sectorSize)
    return container
  }

  public static func unpack(from data: Data) throws -> [OLE2StreamEntry] {
    guard data.count >= sectorSize * 2, data.count % sectorSize == 0 else {
      throw EasyOfficeError.corruptedContainer(detail: "OLE2 컨테이너 크기가 512-byte 섹터 배수가 아닙니다.")
    }
    guard data.starts(with: FormatDetector.ole2Magic) else {
      throw EasyOfficeError.signatureMismatch(expected: .ole2Compound, fileName: "ole2")
    }

    let sectorShift = try BitPacking.readUInt16LE(from: data, at: 30)
    guard sectorShift == 9 else {
      throw EasyOfficeError.corruptedContainer(detail: "지원하지 않는 OLE2 섹터 시프트 값: \(sectorShift)")
    }

    let fatSectorCount = Int(try BitPacking.readUInt32LE(from: data, at: 44))
    let firstDirSector = try BitPacking.readUInt32LE(from: data, at: 48)
    let cutoffSize = Int(try BitPacking.readUInt32LE(from: data, at: 56))
    let firstMiniFatSector = try BitPacking.readUInt32LE(from: data, at: 60)
    let miniFatSectorCount = Int(try BitPacking.readUInt32LE(from: data, at: 64))
    let firstDifatSector = try BitPacking.readUInt32LE(from: data, at: 68)
    let difatSectorCount = Int(try BitPacking.readUInt32LE(from: data, at: 72))
    let totalSectorsInFile = (data.count / sectorSize) - 1

    guard fatSectorCount > 0, fatSectorCount <= totalSectorsInFile else {
      throw EasyOfficeError.corruptedContainer(detail: "유효하지 않은 OLE2 FAT 섹터 개수: \(fatSectorCount)")
    }

    var fatSectorIndices: [Int] = []
    fatSectorIndices.reserveCapacity(fatSectorCount)
    let headerSlots = min(fatSectorCount, 109)
    for slot in 0..<headerSlots {
      let fatSecIndex = Int(try BitPacking.readUInt32LE(from: data, at: 76 + slot * 4))
      fatSectorIndices.append(fatSecIndex)
    }

    if fatSectorCount > 109 {
      var currentDifat = firstDifatSector
      var remainingDifatSectors = max(0, difatSectorCount)
      var visitedDifat = Set<UInt32>()
      while currentDifat != endOfChain && currentDifat != freeSector
        && fatSectorIndices.count < fatSectorCount && remainingDifatSectors > 0
      {
        guard Int(currentDifat) < totalSectorsInFile, visitedDifat.insert(currentDifat).inserted
        else {
          throw EasyOfficeError.corruptedContainer(detail: "OLE2 DIFAT 체인이 손상되었습니다.")
        }
        let difatOffset = (1 + Int(currentDifat)) * sectorSize
        for slot in 0..<127 where fatSectorIndices.count < fatSectorCount {
          let secIdx = try BitPacking.readUInt32LE(from: data, at: difatOffset + slot * 4)
          if secIdx != freeSector {
            fatSectorIndices.append(Int(secIdx))
          }
        }
        currentDifat = try BitPacking.readUInt32LE(from: data, at: difatOffset + 127 * 4)
        remainingDifatSectors -= 1
      }
    }

    var fat: [UInt32] = []
    fat.reserveCapacity(fatSectorIndices.count * fatEntriesPerSector)
    for fatSecIndex in fatSectorIndices {
      guard fatSecIndex >= 0, fatSecIndex < totalSectorsInFile else {
        throw EasyOfficeError.corruptedContainer(detail: "OLE2 FAT 섹터 인덱스가 파일 범위를 벗어납니다.")
      }
      let secOffset = (1 + fatSecIndex) * sectorSize
      for entryIdx in 0..<fatEntriesPerSector {
        let val = try BitPacking.readUInt32LE(from: data, at: secOffset + entryIdx * 4)
        fat.append(val)
      }
    }

    let wordCount = max(1, (max(fat.count, totalSectorsInFile) + 63) >> 6)
    var claimedWords = [UInt64](repeating: 0, count: wordCount)
    let directoryData = try readSectorChain(
      startSector: firstDirSector,
      fat: fat,
      data: data,
      totalSectors: totalSectorsInFile,
      claimedWords: &claimedWords,
      expectedByteCount: sectorSize
    )
    guard !directoryData.isEmpty else {
      throw EasyOfficeError.corruptedContainer(detail: "OLE2 디렉터리 섹터 체인이 비어 있습니다.")
    }

    var miniFat: [UInt32] = []
    var miniStreamData = Data()
    if cutoffSize > 0, firstMiniFatSector != endOfChain, firstMiniFatSector != freeSector,
      miniFatSectorCount > 0
    {
      guard miniFatSectorCount <= totalSectorsInFile else {
        throw EasyOfficeError.corruptedContainer(
          detail: "OLE2 MiniFAT 섹터 개수(\(miniFatSectorCount))가 파일 범위를 초과합니다.")
      }
      let miniFatBytes = try readSectorChain(
        startSector: firstMiniFatSector,
        fat: fat,
        data: data,
        totalSectors: totalSectorsInFile,
        claimedWords: &claimedWords,
        expectedByteCount: miniFatSectorCount * sectorSize
      )
      let count = miniFatBytes.count / 4
      miniFat.reserveCapacity(count)
      for i in 0..<count {
        miniFat.append(try BitPacking.readUInt32LE(from: miniFatBytes, at: i * 4))
      }
      if directoryData.count >= directoryEntrySize {
        let rootStartSector = try BitPacking.readUInt32LE(from: directoryData, at: 116)
        let rawRootStreamSize = try BitPacking.readUInt64LE(from: directoryData, at: 120)
        guard rawRootStreamSize <= UInt64(maxTotalExtractedBytes) else {
          throw EasyOfficeError.resourceExhausted(
            limitName: "ole2RootStreamBytes",
            limitValue: maxTotalExtractedBytes,
            actualValue: Int(min(rawRootStreamSize, UInt64(Int.max)))
          )
        }
        guard rawRootStreamSize <= UInt64(data.count) else {
          throw EasyOfficeError.corruptedContainer(
            detail: "OLE2 Root Entry 미니스트림 크기 오류: \(rawRootStreamSize)")
        }
        let rootStreamSize = Int(rawRootStreamSize)
        if rootStartSector != endOfChain, rootStartSector != freeSector, rootStreamSize > 0 {
          miniStreamData = try readSectorChain(
            startSector: rootStartSector,
            fat: fat,
            data: data,
            totalSectors: totalSectorsInFile,
            claimedWords: &claimedWords,
            expectedByteCount: rootStreamSize
          )
          if miniStreamData.count > rootStreamSize {
            miniStreamData.removeSubrange(rootStreamSize..<miniStreamData.count)
          }
        }
      }
    }

    let entryCount = directoryData.count / directoryEntrySize
    struct ParsedDirNode {
      let objectType: UInt8
      let name: String
      let leftSiblingID: UInt32
      let rightSiblingID: UInt32
      let childID: UInt32
      let startSector: UInt32
      let streamSize: Int
    }
    var dirNodes: [Int: ParsedDirNode] = [:]
    for idx in 0..<entryCount {
      let entryBase = idx * directoryEntrySize
      let objectType = directoryData[directoryData.startIndex + entryBase + 66]
      guard objectType != objectTypeUnallocated else { continue }
      let nameByteLen = Int(try BitPacking.readUInt16LE(from: directoryData, at: entryBase + 64))
      guard nameByteLen >= 2, nameByteLen <= 64, nameByteLen % 2 == 0 else {
        throw EasyOfficeError.corruptedContainer(detail: "OLE2 디렉터리 엔트리 이름 길이 오류: \(nameByteLen)")
      }
      let name = try BitPacking.decodeUTF16LE(
        from: directoryData, at: entryBase, byteLength: nameByteLen - 2)
      let leftSibling = try BitPacking.readUInt32LE(from: directoryData, at: entryBase + 68)
      let rightSibling = try BitPacking.readUInt32LE(from: directoryData, at: entryBase + 72)
      let childID = try BitPacking.readUInt32LE(from: directoryData, at: entryBase + 76)
      let startSector = try BitPacking.readUInt32LE(from: directoryData, at: entryBase + 116)
      let rawStreamSize = try BitPacking.readUInt64LE(from: directoryData, at: entryBase + 120)
      if objectType == objectTypeStream {
        guard rawStreamSize <= UInt64(data.count) else {
          throw EasyOfficeError.corruptedContainer(
            detail: "OLE2 스트림 '\(name)' 크기 오류: \(rawStreamSize)")
        }
      }
      dirNodes[idx] = ParsedDirNode(
        objectType: objectType,
        name: name,
        leftSiblingID: leftSibling,
        rightSiblingID: rightSibling,
        childID: childID,
        startSector: startSector,
        streamSize: objectType == objectTypeStream ? Int(rawStreamSize) : 0
      )
    }

    // Reconstruct hierarchical paths from Storage -> Child BST
    var resolvedStreamPaths: [Int: String] = [:]
    var visitedDirIDs = Set<Int>()
    func traverseDirectorySubtree(entryID: UInt32, prefix: String) {
      guard entryID != freeSector, entryID != endOfChain else { return }
      let idx = Int(entryID)
      guard idx > 0, idx < entryCount, let node = dirNodes[idx],
        visitedDirIDs.insert(idx).inserted
      else { return }
      traverseDirectorySubtree(entryID: node.leftSiblingID, prefix: prefix)
      let fullPath = prefix.isEmpty ? node.name : "\(prefix)/\(node.name)"
      if node.objectType == objectTypeStream {
        resolvedStreamPaths[idx] = fullPath
      } else if node.objectType == objectTypeStorage {
        traverseDirectorySubtree(entryID: node.childID, prefix: fullPath)
      }
      traverseDirectorySubtree(entryID: node.rightSiblingID, prefix: prefix)
    }
    if let rootNode = dirNodes[0] {
      traverseDirectorySubtree(entryID: rootNode.childID, prefix: "")
    }

    var streams: [OLE2StreamEntry] = []
    var claimedMiniSectors = Set<UInt32>()

    for idx in 0..<entryCount {
      guard let node = dirNodes[idx], node.objectType == objectTypeStream else { continue }
      let name = resolvedStreamPaths[idx] ?? node.name
      let startSector = node.startSector
      let streamSize = node.streamSize

      if streamSize == 0 {
        streams.append(OLE2StreamEntry(name: name, data: Data()))
      } else if cutoffSize > 0, streamSize < cutoffSize, !miniFat.isEmpty,
        !miniStreamData.isEmpty
      {
        let totalMiniSectors = miniStreamData.count / miniSectorSize
        var output = Data()
        output.reserveCapacity(streamSize)
        var current = startSector
        while current != endOfChain {
          guard Int(current) < totalMiniSectors, Int(current) < miniFat.count else {
            throw EasyOfficeError.corruptedContainer(
              detail: "OLE2 MiniFAT 체인이 유효하지 않거나 중복된 미니섹터(\(current))를 참조합니다.")
          }
          guard claimedMiniSectors.insert(current).inserted else {
            throw EasyOfficeError.malformedContainer(
              detail: "OLE2 MiniFAT sector \(current) claimed by multiple streams or cycle"
            )
          }
          let offset = Int(current) * miniSectorSize
          let startIdx = miniStreamData.startIndex + offset
          output.append(contentsOf: miniStreamData[startIdx..<(startIdx + miniSectorSize)])
          current = miniFat[Int(current)]
        }
        guard output.count >= streamSize else {
          throw EasyOfficeError.corruptedContainer(
            detail: "OLE2 미니스트림 '\(name)'의 섹터 체인 길이가 부족합니다.")
        }
        output.removeSubrange(streamSize..<output.count)
        streams.append(OLE2StreamEntry(name: name, data: output))
      } else {
        var rawChain = try readSectorChain(
          startSector: startSector,
          fat: fat,
          data: data,
          totalSectors: totalSectorsInFile,
          claimedWords: &claimedWords,
          expectedByteCount: streamSize
        )
        guard rawChain.count >= streamSize else {
          throw EasyOfficeError.corruptedContainer(detail: "OLE2 스트림 '\(name)'의 섹터 체인 길이가 부족합니다.")
        }
        rawChain.removeSubrange(streamSize..<rawChain.count)
        streams.append(OLE2StreamEntry(name: name, data: rawChain))
      }
    }

    return streams
  }

  private static func readSectorChain(
    startSector: UInt32,
    fat: [UInt32],
    data: Data,
    totalSectors: Int,
    claimedWords: inout [UInt64],
    expectedByteCount: Int? = nil
  ) throws -> Data {
    var output = Data()
    if let expectedByteCount, expectedByteCount > 0 {
      output.reserveCapacity(min(expectedByteCount + sectorSize, data.count))
    }
    var current = startSector

    while current != endOfChain {
      guard current < UInt32(totalSectors), Int(current) < fat.count else {
        throw EasyOfficeError.corruptedContainer(
          detail: "OLE2 FAT 체인이 유효하지 않은 섹터(\(current))를 참조합니다.")
      }
      let idx = Int(current)
      let wordIdx = idx >> 6
      let mask: UInt64 = 1 << UInt64(idx & 63)
      guard (claimedWords[wordIdx] & mask) == 0 else {
        throw EasyOfficeError.corruptedContainer(
          detail: "OLE2 sector cycle or cross-stream sector sharing detected"
        )
      }
      claimedWords[wordIdx] |= mask

      let fileOffset = (1 + idx) * sectorSize
      let startIdx = data.startIndex + fileOffset
      output.append(contentsOf: data[startIdx..<(startIdx + sectorSize)])
      current = fat[idx]
    }

    return output
  }

  private static func writeDirectoryEntry(
    into directoryBytes: inout Data,
    entryIndex: Int,
    name: String,
    objectType: UInt8,
    colorFlag: UInt8 = 1,
    leftSiblingID: UInt32 = freeSector,
    rightSiblingID: UInt32,
    childID: UInt32,
    startSector: UInt32,
    streamSize: UInt64
  ) throws {
    let base = entryIndex * directoryEntrySize
    let utf16Units = Array(name.utf16)
    guard utf16Units.count <= 31 else {
      throw EasyOfficeError.corruptedContainer(
        detail: "OLE2 스트림 이름이 최대 길이(31 UTF-16 코드 유닛)를 초과했습니다: \(name)")
    }
    for (i, unit) in utf16Units.enumerated() {
      try BitPacking.writeUInt16LE(unit, into: &directoryBytes, at: base + i * 2)
    }
    let nameByteLen = UInt16((utf16Units.count + 1) * 2)
    try BitPacking.writeUInt16LE(nameByteLen, into: &directoryBytes, at: base + 64)
    directoryBytes[directoryBytes.startIndex + base + 66] = objectType
    directoryBytes[directoryBytes.startIndex + base + 67] = colorFlag
    try BitPacking.writeUInt32LE(leftSiblingID, into: &directoryBytes, at: base + 68)
    try BitPacking.writeUInt32LE(rightSiblingID, into: &directoryBytes, at: base + 72)
    try BitPacking.writeUInt32LE(childID, into: &directoryBytes, at: base + 76)
    try BitPacking.writeUInt32LE(startSector, into: &directoryBytes, at: base + 116)
    try BitPacking.writeUInt64LE(streamSize, into: &directoryBytes, at: base + 120)
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
  public static let maxHWP5TableRows = 4096
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
      payloadSize: UInt32(docPropsPayload.count)
    )
    rawDocInfoStream.append(docPropsPayload)

    let extEnvelope = ExtendedDocumentMetadataEnvelope(from: doc)
    if let extPayload = try? JSONEncoder().encode(extEnvelope), !extPayload.isEmpty {
      let extRec = try HWP5RecordHeader.encode(
        tagID: HWP5RecordHeader.tagExtendedMetadata,
        level: 0,
        payloadSize: UInt32(extPayload.count)
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
        payloadSize: UInt32(paraHeaderPayload.count)
      )
      rawSection0Stream.append(headerRec)
      rawSection0Stream.append(paraHeaderPayload)

      let textPayload = BitPacking.encodeUTF16LE(paragraph.text)
      let textRec = try HWP5RecordHeader.encode(
        tagID: HWP5RecordHeader.tagParaText,
        level: 1,
        payloadSize: UInt32(textPayload.count)
      )
      rawSection0Stream.append(textRec)
      rawSection0Stream.append(textPayload)
    }

    for table in doc.tables {
      guard table.rows.count <= maxHWP5TableRows else {
        throw EasyOfficeError.corruptedContainer(
          detail: "HWP 5.0 표 행 개수(\(table.rows.count))가 허용 상한(\(maxHWP5TableRows))을 초과했습니다.")
      }
      var tablePayload = Data()
      BitPacking.appendLengthPrefixedUTF16LE(table.caption, to: &tablePayload)
      tablePayload.append(table.hasHeaderRow ? 1 : 0)
      BitPacking.appendUInt16LE(UInt16(table.rows.count), to: &tablePayload)
      for row in table.rows {
        guard row.count <= maxHWP5TableColumns else {
          throw EasyOfficeError.corruptedContainer(
            detail: "HWP 5.0 표 열 개수(\(row.count))가 허용 상한(\(maxHWP5TableColumns))을 초과했습니다.")
        }
        BitPacking.appendUInt16LE(UInt16(row.count), to: &tablePayload)
        for cell in row {
          BitPacking.appendLengthPrefixedUTF16LE(cell, to: &tablePayload)
        }
      }
      let tableRec = try HWP5RecordHeader.encode(
        tagID: HWP5RecordHeader.tagTableRecord,
        level: 0,
        payloadSize: UInt32(tablePayload.count)
      )
      rawSection0Stream.append(tableRec)
      rawSection0Stream.append(tablePayload)
    }

    let docInfoStream: Data
    let section0Stream: Data
    if compressed {
      docInfoStream =
        rawDocInfoStream.isEmpty
        ? Data() : try ZipContainerCodec.compressRawDeflate(rawDocInfoStream)
      section0Stream =
        rawSection0Stream.isEmpty
        ? Data() : try ZipContainerCodec.compressRawDeflate(rawSection0Stream)
    } else {
      docInfoStream = rawDocInfoStream
      section0Stream = rawSection0Stream
    }

    return try OLE2ContainerCodec.pack(streams: [
      OLE2StreamEntry(name: "FileHeader", data: fileHeader),
      OLE2StreamEntry(name: "DocInfo", data: docInfoStream),
      OLE2StreamEntry(name: "BodyText/Section0", data: section0Stream),
    ])
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

    guard let rawSection0 = streamMap["BodyText/Section0"] else {
      throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 'BodyText/Section0' 스트림이 누락되었습니다.")
    }
    let section0 =
      (isCompressed && !rawSection0.isEmpty)
      ? try ZipContainerCodec.decompressRawDeflateStream(rawSection0)
      : rawSection0

    var paragraphs: [ParagraphBlock] = []
    var tables: [DocumentTableBlock] = []
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
          detail: "HWP 5.0 Section0 레코드 페이로드가 스트림 경계를 초과합니다.")
      }

      switch recHeader.tagID {
      case HWP5RecordHeader.tagParaHeader:
        if let prev = pendingParagraph {
          paragraphs.append(prev)
        }
        var pCursor = payloadStart
        let styleRaw = try BitPacking.readLengthPrefixedUTF16LE(
          from: section0, offset: &pCursor, limit: payloadEnd)
        if pCursor + 1 == payloadEnd {
          let isOfficial = section0[section0.startIndex + pCursor] != 0
          let resolvedStyle: ParagraphStyleRole =
            isOfficial ? .officialHeader : (ParagraphStyleRole(rawValue: styleRaw) ?? .body)
          pendingParagraph = ParagraphBlock(
            style: resolvedStyle,
            text: ""
          )
        } else {
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
          pendingParagraph = ParagraphBlock(
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

      case HWP5RecordHeader.tagParaText:
        let text = try BitPacking.decodeUTF16LE(
          from: section0, at: payloadStart, byteLength: payloadLen)
        if var current = pendingParagraph {
          current.text = text
          paragraphs.append(current)
          pendingParagraph = nil
        } else {
          paragraphs.append(ParagraphBlock(text: text))
        }

      case HWP5RecordHeader.tagTableRecord:
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
          guard rowCount <= maxHWP5TableRows, rowCount <= (payloadEnd - tCursor) / 2 else {
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
          tables.append(table)
        } else {
          var rCursor = payloadStart
          guard rCursor + 2 <= payloadEnd else {
            throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 레코드 헤더 길이가 부족합니다.")
          }
          let rowCount = Int(try BitPacking.readUInt16LE(from: section0, at: rCursor))
          rCursor += 2
          guard rowCount <= maxHWP5TableRows, rowCount <= (payloadEnd - rCursor) / 2 else {
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
          tables.append(DocumentTableBlock(caption: "", rows: rows, hasHeaderRow: true))
        }

      default:
        break
      }

      offset = payloadEnd
    }

    if let trailing = pendingParagraph {
      paragraphs.append(trailing)
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
    extendedEnvelope?.apply(to: &docModel)
    return docModel
  }
}
