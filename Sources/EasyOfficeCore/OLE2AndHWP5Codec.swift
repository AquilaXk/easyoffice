import Foundation

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

/// OLE2 Compound File Binary Format (`D0 CF 11 E0 A1 B1 1A E1`) container codec supporting v3 (512B) and v4 (4096B) sectors and DIFAT chains.
public enum OLE2ContainerCodec {
  public static let sectorSize = 512
  public static let miniSectorSize = 64
  public static let miniStreamCutoffSize = 4096
  public static let directoryEntrySize = 128
  public static let entriesPerSector = sectorSize / directoryEntrySize  // 4
  public static let fatEntriesPerSector = sectorSize / 4  // 128
  public static let difatEntriesPerSector = fatEntriesPerSector - 1  // 127
  public static let maxDirectoryEntries = 4_096

  public static let freeSector: UInt32 = 0xFFFF_FFFF
  public static let endOfChain: UInt32 = 0xFFFF_FFFE
  public static let fatSectorMarker: UInt32 = 0xFFFF_FFFD
  public static let difatSectorMarker: UInt32 = 0xFFFF_FFFC
  public static let maxTotalExtractedBytes = 64 * 1024 * 1024

  private static let objectTypeUnallocated: UInt8 = 0
  private static let objectTypeStorage: UInt8 = 1
  private static let objectTypeStream: UInt8 = 2
  private static let objectTypeRootStorage: UInt8 = 5

  @inline(__always)
  private static func foldCFBCodeUnit(_ unit: UInt16) -> UInt16 {
    if unit >= 0x0061 && unit <= 0x007A {
      return unit &- 0x0020
    }
    if unit < 0x0080 {
      return unit
    }
    if let scalar = UnicodeScalar(unit) {
      let upper = String(scalar).uppercased().utf16
      if upper.count == 1, let first = upper.first {
        return first
      }
    }
    return unit
  }

  private static func foldCFBName(_ name: String) -> [UInt16] {
    name.utf16.map(foldCFBCodeUnit)
  }

  private static func compareCFBNames(_ lhs: String, _ rhs: String) -> Bool {
    let lUTF16 = lhs.utf16
    let rUTF16 = rhs.utf16
    let lLen = lUTF16.count
    let rLen = rUTF16.count
    if lLen != rLen { return lLen < rLen }
    var lIdx = lUTF16.startIndex
    var rIdx = rUTF16.startIndex
    while lIdx < lUTF16.endIndex {
      let lu = foldCFBCodeUnit(lUTF16[lIdx])
      let ru = foldCFBCodeUnit(rUTF16[rIdx])
      if lu != ru { return lu < ru }
      lIdx = lUTF16.index(after: lIdx)
      rIdx = rUTF16.index(after: rIdx)
    }
    return false
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
    let color: UInt8 = (depth > 0 && depth == maxDepth) ? 0 : 1
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
    var storageNames: [String] = []
    var storageCanonicalByFolded: [[UInt16]: String] = [:]
    var rootStreamFoldedNames = Set<[[UInt16]]>()
    var storageChildrenFoldedNames: [[UInt16]: Set<[UInt16]>] = [:]
    var streamLeafNames: [String] = []
    var streamParentStorageName: [String?] = []

    for stream in streams {
      guard !stream.name.isEmpty else {
        throw EasyOfficeError.corruptedContainer(detail: "비어 있는 OLE2 스트림 이름은 허용되지 않습니다.")
      }
      if let slashIdx = stream.name.firstIndex(of: "/") {
        let storageName = String(stream.name[..<slashIdx])
        let leafName = String(stream.name[stream.name.index(after: slashIdx)...])
        guard !storageName.isEmpty, !leafName.isEmpty, !leafName.contains("/") else {
          throw EasyOfficeError.corruptedContainer(
            detail: "유효하지 않은 OLE2 계층 스트림 경로: \(stream.name)")
        }
        let foldedStorage = foldCFBName(storageName)
        let foldedLeaf = foldCFBName(leafName)
        if rootStreamFoldedNames.contains([foldedStorage]) {
          throw EasyOfficeError.securityViolation(
            detail: "OLE2 스토리지 이름이 기존 루트 스트림과 충돌합니다: \(stream.name)")
        }
        let canonicalStorage: String
        if let existing = storageCanonicalByFolded[foldedStorage] {
          canonicalStorage = existing
        } else {
          storageCanonicalByFolded[foldedStorage] = storageName
          storageNames.append(storageName)
          canonicalStorage = storageName
        }
        guard storageChildrenFoldedNames[foldedStorage, default: []].insert(foldedLeaf).inserted else {
          throw EasyOfficeError.securityViolation(
            detail: "OLE2 동일 스토리지 내 중복된 스트림 이름이 감지되었습니다: \(stream.name)")
        }
        streamLeafNames.append(leafName)
        streamParentStorageName.append(canonicalStorage)
      } else {
        let foldedStream = foldCFBName(stream.name)
        if storageCanonicalByFolded[foldedStream] != nil {
          throw EasyOfficeError.securityViolation(
            detail: "OLE2 루트 스트림 이름이 기존 스토리지와 충돌합니다: \(stream.name)")
        }
        guard rootStreamFoldedNames.insert([foldedStream]).inserted else {
          throw EasyOfficeError.securityViolation(
            detail: "OLE2 동일 스토리지 내 중복된 스트림 이름이 감지되었습니다: \(stream.name)")
        }
        streamLeafNames.append(stream.name)
        streamParentStorageName.append(nil)
      }
    }

    let totalDirEntries = 1 + streams.count + storageNames.count
    guard totalDirEntries <= maxDirectoryEntries else {
      throw EasyOfficeError.resourceBudgetExceeded(
        detail: "OLE2 디렉터리 엔트리 개수(\(totalDirEntries))가 허용 상한(\(maxDirectoryEntries))을 초과했습니다.")
    }
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
        nextSectorIndex += try BitPacking.checkedUInt32(count, field: "OLE2 스트림 섹터 개수")
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
    var fatSectorCount = max(1, (contentSectorCount + fatEntriesPerSector - 1) / fatEntriesPerSector)
    var difatSectorCount =
      fatSectorCount > 109
      ? ((fatSectorCount - 109 + difatEntriesPerSector - 1) / difatEntriesPerSector)
      : 0
    while contentSectorCount + fatSectorCount + difatSectorCount > fatSectorCount * fatEntriesPerSector {
      fatSectorCount += 1
      difatSectorCount =
        fatSectorCount > 109
        ? ((fatSectorCount - 109 + difatEntriesPerSector - 1) / difatEntriesPerSector)
        : 0
    }

    let firstFatSectorIndex = contentSectorCount
    let firstDifatSectorIndex = firstFatSectorIndex + fatSectorCount
    let totalSectors = contentSectorCount + fatSectorCount + difatSectorCount

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

    for dIdx in 0..<difatSectorCount {
      fat[firstDifatSectorIndex + dIdx] = difatSectorMarker
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
    try BitPacking.writeUInt32LE(
      try BitPacking.checkedUInt32(fatSectorCount, field: "OLE2 FAT 섹터 개수"),
      into: &container,
      at: 44
    )
    try BitPacking.writeUInt32LE(0, into: &container, at: 48)
    try BitPacking.writeUInt32LE(UInt32(miniStreamCutoffSize), into: &container, at: 56)
    try BitPacking.writeUInt32LE(firstMiniFatSector, into: &container, at: 60)
    try BitPacking.writeUInt32LE(
      try BitPacking.checkedUInt32(miniFatSectorCount, field: "OLE2 MiniFAT 섹터 개수"),
      into: &container,
      at: 64
    )
    let headerFirstDifat: UInt32 =
      difatSectorCount > 0 ? UInt32(firstDifatSectorIndex) : endOfChain
    try BitPacking.writeUInt32LE(headerFirstDifat, into: &container, at: 68)
    try BitPacking.writeUInt32LE(
      try BitPacking.checkedUInt32(difatSectorCount, field: "OLE2 DIFAT 섹터 개수"),
      into: &container,
      at: 72
    )

    for slot in 0..<109 {
      let value = (slot < fatSectorCount) ? UInt32(firstFatSectorIndex + slot) : freeSector
      try BitPacking.writeUInt32LE(value, into: &container, at: 76 + slot * 4)
    }

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

    for (idx, stream) in streams.enumerated() where regularStreamSectorCounts[idx] > 0 {
      container.append(stream.data)
      let remainder = stream.data.count % sectorSize
      if remainder != 0 {
        container.append(Data(repeating: 0, count: sectorSize - remainder))
      }
    }

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

    if difatSectorCount > 0 {
      var difatBytes = Data(repeating: 0xFF, count: difatSectorCount * sectorSize)
      for dIdx in 0..<difatSectorCount {
        let sectorBase = dIdx * sectorSize
        for slot in 0..<difatEntriesPerSector {
          let fatOrdinal = 109 + dIdx * difatEntriesPerSector + slot
          let val: UInt32 =
            (fatOrdinal < fatSectorCount) ? UInt32(firstFatSectorIndex + fatOrdinal) : freeSector
          try BitPacking.writeUInt32LE(val, into: &difatBytes, at: sectorBase + slot * 4)
        }
        let nextDifat: UInt32 =
          (dIdx + 1 < difatSectorCount) ? UInt32(firstDifatSectorIndex + dIdx + 1) : endOfChain
        try BitPacking.writeUInt32LE(
          nextDifat, into: &difatBytes, at: sectorBase + difatEntriesPerSector * 4)
      }
      container.append(difatBytes)
    }

    assert(container.count == (1 + totalSectors) * sectorSize)
    return container
  }

  public static func unpack(from data: Data) throws -> [OLE2StreamEntry] {
    guard data.count >= sectorSize * 2 else {
      throw EasyOfficeError.corruptedContainer(detail: "OLE2 컨테이너 크기가 최소 섹터 크기보다 작습니다.")
    }
    guard data.starts(with: FormatDetector.ole2Magic) else {
      throw EasyOfficeError.signatureMismatch(expected: .ole2Compound, fileName: "ole2")
    }

    let majorVersion = try BitPacking.readUInt16LE(from: data, at: 26)
    let sectorShift = try BitPacking.readUInt16LE(from: data, at: 30)
    guard sectorShift == 9 || sectorShift == 12 else {
      throw EasyOfficeError.corruptedContainer(detail: "지원하지 않는 OLE2 섹터 시프트 값: \(sectorShift)")
    }
    if majorVersion == 3 && sectorShift != 9 {
      throw EasyOfficeError.corruptedContainer(detail: "OLE2 v3 헤더의 섹터 시프트(\(sectorShift))가 9가 아닙니다.")
    }
    if majorVersion == 4 && sectorShift != 12 {
      throw EasyOfficeError.corruptedContainer(detail: "OLE2 v4 헤더의 섹터 시프트(\(sectorShift))가 12가 아닙니다.")
    }

    let actualSectorSize = 1 << Int(sectorShift)
    guard data.count >= actualSectorSize * 2, data.count % actualSectorSize == 0 else {
      throw EasyOfficeError.corruptedContainer(
        detail: "OLE2 컨테이너 크기가 섹터 크기(\(actualSectorSize) bytes) 배수가 아닙니다.")
    }
    let actualFatEntriesPerSector = actualSectorSize / 4
    let actualDifatSlotsPerSector = actualFatEntriesPerSector - 1

    let fatSectorCount = Int(try BitPacking.readUInt32LE(from: data, at: 44))
    let firstDirSector = try BitPacking.readUInt32LE(from: data, at: 48)
    let cutoffSize = Int(try BitPacking.readUInt32LE(from: data, at: 56))
    let firstMiniFatSector = try BitPacking.readUInt32LE(from: data, at: 60)
    let miniFatSectorCount = Int(try BitPacking.readUInt32LE(from: data, at: 64))
    let firstDifatSector = try BitPacking.readUInt32LE(from: data, at: 68)
    let difatSectorCount = Int(try BitPacking.readUInt32LE(from: data, at: 72))
    let totalSectorsInFile = (data.count / actualSectorSize) - 1

    guard fatSectorCount > 0, fatSectorCount <= totalSectorsInFile else {
      throw EasyOfficeError.corruptedContainer(detail: "유효하지 않은 OLE2 FAT 섹터 개수: \(fatSectorCount)")
    }

    var fatSectorIndices: [Int] = []
    fatSectorIndices.reserveCapacity(fatSectorCount)
    let headerSlots = min(fatSectorCount, 109)
    for slot in 0..<headerSlots {
      let rawSec = try BitPacking.readUInt32LE(from: data, at: 76 + slot * 4)
      guard rawSec != freeSector, rawSec != endOfChain else {
        throw EasyOfficeError.corruptedContainer(detail: "OLE2 헤더 DIFAT 슬롯(\(slot))이 비어 있습니다.")
      }
      fatSectorIndices.append(Int(rawSec))
    }

    var visitedDifat = Set<UInt32>()
    if fatSectorCount > 109 {
      guard difatSectorCount > 0, difatSectorCount <= totalSectorsInFile,
        firstDifatSector != endOfChain, firstDifatSector != freeSector
      else {
        throw EasyOfficeError.corruptedContainer(detail: "OLE2 DIFAT 체인 헤더가 누락되었거나 손상되었습니다.")
      }
      var currentDifat = firstDifatSector
      var walkedDifatSectors = 0
      while currentDifat != endOfChain && currentDifat != freeSector
        && walkedDifatSectors < difatSectorCount
      {
        guard Int(currentDifat) >= 0, Int(currentDifat) < totalSectorsInFile,
          visitedDifat.insert(currentDifat).inserted
        else {
          throw EasyOfficeError.corruptedContainer(detail: "OLE2 DIFAT 체인이 손상되었거나 순환합니다.")
        }
        let difatOffset = (1 + Int(currentDifat)) * actualSectorSize
        for slot in 0..<actualDifatSlotsPerSector where fatSectorIndices.count < fatSectorCount {
          let secIdx = try BitPacking.readUInt32LE(from: data, at: difatOffset + slot * 4)
          guard secIdx != freeSector, secIdx != endOfChain else {
            throw EasyOfficeError.corruptedContainer(detail: "OLE2 DIFAT 섹터 내 필수 FAT 인덱스가 누락되었습니다.")
          }
          fatSectorIndices.append(Int(secIdx))
        }
        currentDifat = try BitPacking.readUInt32LE(
          from: data, at: difatOffset + actualDifatSlotsPerSector * 4)
        walkedDifatSectors += 1
      }
      guard walkedDifatSectors == difatSectorCount,
        fatSectorIndices.count == fatSectorCount,
        currentDifat == endOfChain || currentDifat == freeSector
      else {
        throw EasyOfficeError.corruptedContainer(detail: "OLE2 DIFAT 체인 길이 또는 종단 마커가 일치하지 않습니다.")
      }
    }

    let wordCount = max(
      1, (max(fatSectorIndices.count * actualFatEntriesPerSector, totalSectorsInFile) + 63) >> 6)
    var claimedWords = [UInt64](repeating: 0, count: wordCount)

    for difatSec in visitedDifat {
      let idx = Int(difatSec)
      claimedWords[idx >> 6] |= (1 << UInt64(idx & 63))
    }

    var fat: [UInt32] = []
    fat.reserveCapacity(fatSectorIndices.count * actualFatEntriesPerSector)
    for fatSecIndex in fatSectorIndices {
      guard fatSecIndex >= 0, fatSecIndex < totalSectorsInFile else {
        throw EasyOfficeError.corruptedContainer(detail: "OLE2 FAT 섹터 인덱스가 파일 범위를 벗어납니다.")
      }
      let wordIdx = fatSecIndex >> 6
      let mask: UInt64 = 1 << UInt64(fatSecIndex & 63)
      guard (claimedWords[wordIdx] & mask) == 0 else {
        throw EasyOfficeError.corruptedContainer(detail: "OLE2 FAT 섹터 인덱스가 중복 참조되었습니다.")
      }
      claimedWords[wordIdx] |= mask

      let secOffset = (1 + fatSecIndex) * actualSectorSize
      for entryIdx in 0..<actualFatEntriesPerSector {
        let val = try BitPacking.readUInt32LE(from: data, at: secOffset + entryIdx * 4)
        fat.append(val)
      }
    }

    let directoryData = try readSectorChain(
      startSector: firstDirSector,
      fat: fat,
      data: data,
      sectorSize: actualSectorSize,
      totalSectors: totalSectorsInFile,
      claimedWords: &claimedWords,
      expectedByteCount: actualSectorSize
    )
    guard !directoryData.isEmpty else {
      throw EasyOfficeError.corruptedContainer(detail: "OLE2 디렉터리 섹터 체인이 비어 있습니다.")
    }

    let entryCount = directoryData.count / directoryEntrySize
    guard entryCount <= maxDirectoryEntries else {
      throw EasyOfficeError.resourceBudgetExceeded(
        detail: "OLE2 디렉터리 엔트리 개수(\(entryCount))가 허용 한도(\(maxDirectoryEntries))를 초과했습니다.")
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
        sectorSize: actualSectorSize,
        totalSectors: totalSectorsInFile,
        claimedWords: &claimedWords,
        expectedByteCount: miniFatSectorCount * actualSectorSize
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
            sectorSize: actualSectorSize,
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
        guard rawStreamSize <= UInt64(data.count),
          rawStreamSize <= UInt64(maxTotalExtractedBytes)
        else {
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

    enum TraversalStep {
      case enter(entryID: UInt32, prefix: String)
      case visit(idx: Int, prefix: String)
    }

    var resolvedStreamPaths: [Int: String] = [:]
    var visitedDirIDs = Set<Int>()
    var seenSiblingsByPrefix: [String: Set<[UInt16]>] = [:]
    var traversalSteps = 0

    if let rootNode = dirNodes[0] {
      var stack: [TraversalStep] = [.enter(entryID: rootNode.childID, prefix: "")]
      while let step = stack.popLast() {
        switch step {
        case .enter(let entryID, let prefix):
          guard entryID != freeSector, entryID != endOfChain else { continue }
          traversalSteps += 1
          guard traversalSteps <= maxDirectoryEntries else {
            throw EasyOfficeError.resourceBudgetExceeded(
              detail: "OLE2 디렉터리 트리 순회 횟수가 허용 한도(\(maxDirectoryEntries))를 초과했습니다.")
          }
          let idx = Int(entryID)
          guard idx > 0, idx < entryCount, let node = dirNodes[idx] else {
            throw EasyOfficeError.corruptedContainer(
              detail: "OLE2 디렉터리 트리가 유효하지 않은 엔트리(\(entryID))를 참조합니다.")
          }
          guard visitedDirIDs.insert(idx).inserted else {
            throw EasyOfficeError.corruptedContainer(
              detail: "OLE2 디렉터리 트리에서 순환 참조가 감지되었습니다.")
          }
          stack.append(.enter(entryID: node.rightSiblingID, prefix: prefix))
          stack.append(.visit(idx: idx, prefix: prefix))
          stack.append(.enter(entryID: node.leftSiblingID, prefix: prefix))

        case .visit(let idx, let prefix):
          guard let node = dirNodes[idx] else { continue }
          let folded = foldCFBName(node.name)
          guard seenSiblingsByPrefix[prefix, default: []].insert(folded).inserted else {
            throw EasyOfficeError.corruptedContainer(
              detail: "OLE2 동일 스토리지 내 중복된 엔트리 이름이 감지되었습니다: \(node.name)")
          }
          let fullPath = prefix.isEmpty ? node.name : "\(prefix)/\(node.name)"
          if node.objectType == objectTypeStream {
            resolvedStreamPaths[idx] = fullPath
          } else if node.objectType == objectTypeStorage {
            stack.append(.enter(entryID: node.childID, prefix: fullPath))
          }
        }
      }
    }

    var streams: [OLE2StreamEntry] = []
    var seenResolvedStreamFoldedPaths = Set<[[UInt16]]>()
    let totalMiniSectorsCapacity = max(miniStreamData.count / miniSectorSize, miniFat.count)
    var claimedMiniWords = [UInt64](repeating: 0, count: (totalMiniSectorsCapacity + 63) / 64)
    var cumulativeExtractedBytes = 0

    for idx in 0..<entryCount {
      guard let node = dirNodes[idx], node.objectType == objectTypeStream else { continue }
      let name = resolvedStreamPaths[idx] ?? node.name
      let foldedPath = name.split(separator: "/", omittingEmptySubsequences: false).map {
        foldCFBName(String($0))
      }
      guard seenResolvedStreamFoldedPaths.insert(foldedPath).inserted else {
        throw EasyOfficeError.corruptedContainer(
          detail: "OLE2 동일 스토리지 내 중복된 스트림 이름이 감지되었습니다: \(name)")
      }
      let startSector = node.startSector
      let streamSize = node.streamSize
      cumulativeExtractedBytes += streamSize
      guard cumulativeExtractedBytes <= maxTotalExtractedBytes else {
        throw EasyOfficeError.resourceBudgetExceeded(
          detail: "OLE2 누적 추출 스트림 크기(\(cumulativeExtractedBytes))가 허용 상한(\(maxTotalExtractedBytes))을 초과했습니다.")
      }

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
          let miniIdx = Int(current)
          let miniWordIdx = miniIdx >> 6
          let miniBitMask: UInt64 = 1 << (miniIdx & 63)
          guard miniWordIdx < claimedMiniWords.count,
            (claimedMiniWords[miniWordIdx] & miniBitMask) == 0
          else {
            throw EasyOfficeError.malformedContainer(
              detail: "OLE2 MiniFAT sector \(current) claimed by multiple streams or cycle"
            )
          }
          claimedMiniWords[miniWordIdx] |= miniBitMask
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
          sectorSize: actualSectorSize,
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
    sectorSize: Int,
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
