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

/// 512-byte sector OLE2 Compound File Binary Format (`D0 CF 11 E0 A1 B1 1A E1`) container codec.
public enum OLE2ContainerCodec {
    public static let sectorSize = 512
    public static let directoryEntrySize = 128
    public static let entriesPerSector = sectorSize / directoryEntrySize // 4
    public static let fatEntriesPerSector = sectorSize / 4 // 128

    public static let freeSector: UInt32 = 0xFFFF_FFFF
    public static let endOfChain: UInt32 = 0xFFFF_FFFE
    public static let fatSectorMarker: UInt32 = 0xFFFF_FFFD

    private static let objectTypeUnallocated: UInt8 = 0
    private static let objectTypeStream: UInt8 = 2
    private static let objectTypeRootStorage: UInt8 = 5

    public static func pack(streams: [OLE2StreamEntry]) throws -> Data {
        let totalDirEntries = 1 + streams.count
        let dirSectorCount = max(1, (totalDirEntries + entriesPerSector - 1) / entriesPerSector)

        var nextSectorIndex = UInt32(dirSectorCount)
        var streamStartSectors: [UInt32] = []
        var streamSectorCounts: [Int] = []

        for stream in streams {
            if stream.data.isEmpty {
                streamStartSectors.append(endOfChain)
                streamSectorCounts.append(0)
            } else {
                let count = (stream.data.count + sectorSize - 1) / sectorSize
                streamStartSectors.append(nextSectorIndex)
                streamSectorCounts.append(count)
                nextSectorIndex += UInt32(count)
            }
        }

        let contentSectorCount = Int(nextSectorIndex)
        let fatSectorCount = max(1, (contentSectorCount + fatEntriesPerSector - 1) / fatEntriesPerSector)
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
            let count = streamSectorCounts[idx]
            guard count > 0 else { continue }
            let start = Int(streamStartSectors[idx])
            for offset in 0..<count {
                let current = start + offset
                fat[current] = (offset + 1 < count) ? UInt32(current + 1) : endOfChain
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
        try BitPacking.writeUInt32LE(0, into: &container, at: 56)
        try BitPacking.writeUInt32LE(endOfChain, into: &container, at: 60)
        try BitPacking.writeUInt32LE(0, into: &container, at: 64)
        try BitPacking.writeUInt32LE(endOfChain, into: &container, at: 68)
        try BitPacking.writeUInt32LE(0, into: &container, at: 72)

        for slot in 0..<109 {
            let value = (slot < fatSectorCount) ? UInt32(firstFatSectorIndex + slot) : freeSector
            try BitPacking.writeUInt32LE(value, into: &container, at: 76 + slot * 4)
        }

        var directoryBytes = Data(repeating: 0, count: dirSectorCount * sectorSize)
        try writeDirectoryEntry(
            into: &directoryBytes,
            entryIndex: 0,
            name: "Root Entry",
            objectType: objectTypeRootStorage,
            childID: streams.isEmpty ? freeSector : 1,
            rightSiblingID: freeSector,
            startSector: endOfChain,
            streamSize: 0
        )

        for (sIdx, stream) in streams.enumerated() {
            let entryIndex = sIdx + 1
            let nextSibling = (sIdx + 1 < streams.count) ? UInt32(entryIndex + 1) : freeSector
            try writeDirectoryEntry(
                into: &directoryBytes,
                entryIndex: entryIndex,
                name: stream.name,
                objectType: objectTypeStream,
                childID: freeSector,
                rightSiblingID: nextSibling,
                startSector: streamStartSectors[sIdx],
                streamSize: UInt64(stream.data.count)
            )
        }
        container.append(directoryBytes)

        for stream in streams where !stream.data.isEmpty {
            container.append(stream.data)
            let remainder = stream.data.count % sectorSize
            if remainder != 0 {
                container.append(Data(repeating: 0, count: sectorSize - remainder))
            }
        }

        var fatBytes = Data()
        fatBytes.reserveCapacity(fat.count * 4)
        for entry in fat {
            BitPacking.appendUInt32LE(entry, to: &fatBytes)
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
        let totalSectorsInFile = (data.count / sectorSize) - 1

        guard fatSectorCount > 0, fatSectorCount <= 109 else {
            throw EasyOfficeError.corruptedContainer(detail: "유효하지 않은 OLE2 FAT 섹터 개수: \(fatSectorCount)")
        }

        var fat: [UInt32] = []
        fat.reserveCapacity(fatSectorCount * fatEntriesPerSector)
        for slot in 0..<fatSectorCount {
            let fatSecIndex = Int(try BitPacking.readUInt32LE(from: data, at: 76 + slot * 4))
            guard fatSecIndex >= 0, fatSecIndex < totalSectorsInFile else {
                throw EasyOfficeError.corruptedContainer(detail: "OLE2 FAT 섹터 인덱스가 파일 범위를 벗어납니다.")
            }
            let secOffset = (1 + fatSecIndex) * sectorSize
            for entryIdx in 0..<fatEntriesPerSector {
                let val = try BitPacking.readUInt32LE(from: data, at: secOffset + entryIdx * 4)
                fat.append(val)
            }
        }

        var claimedSectors = Set<UInt32>()
        let directoryData = try readSectorChain(
            startSector: firstDirSector,
            fat: fat,
            data: data,
            totalSectors: totalSectorsInFile,
            claimedSectors: &claimedSectors
        )
        guard !directoryData.isEmpty else {
            throw EasyOfficeError.corruptedContainer(detail: "OLE2 디렉터리 섹터 체인이 비어 있습니다.")
        }

        let entryCount = directoryData.count / directoryEntrySize
        var streams: [OLE2StreamEntry] = []

        for idx in 0..<entryCount {
            let entryBase = idx * directoryEntrySize
            let objectType = directoryData[directoryData.startIndex + entryBase + 66]
            guard objectType == objectTypeStream else { continue }

            let nameByteLen = Int(try BitPacking.readUInt16LE(from: directoryData, at: entryBase + 64))
            guard nameByteLen >= 2, nameByteLen <= 64, nameByteLen % 2 == 0 else {
                throw EasyOfficeError.corruptedContainer(detail: "OLE2 디렉터리 엔트리 이름 길이 오류: \(nameByteLen)")
            }
            let name = try BitPacking.decodeUTF16LE(from: directoryData, at: entryBase, byteLength: nameByteLen - 2)
            let startSector = try BitPacking.readUInt32LE(from: directoryData, at: entryBase + 116)
            let rawStreamSize = try BitPacking.readUInt64LE(from: directoryData, at: entryBase + 120)

            guard rawStreamSize <= UInt64(data.count) else {
                throw EasyOfficeError.corruptedContainer(detail: "OLE2 스트림 '\(name)' 크기 오류: \(rawStreamSize)")
            }
            let streamSize = Int(rawStreamSize)

            if streamSize == 0 {
                streams.append(OLE2StreamEntry(name: name, data: Data()))
            } else {
                var rawChain = try readSectorChain(
                    startSector: startSector,
                    fat: fat,
                    data: data,
                    totalSectors: totalSectorsInFile,
                    claimedSectors: &claimedSectors
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
        claimedSectors: inout Set<UInt32>
    ) throws -> Data {
        var output = Data()
        var current = startSector

        while current != endOfChain {
            guard !claimedSectors.contains(current) else {
                throw EasyOfficeError.corruptedContainer(
                    detail: "OLE2 sector cycle or cross-stream sector sharing detected"
                )
            }
            guard current < UInt32(totalSectors), Int(current) < fat.count else {
                throw EasyOfficeError.corruptedContainer(detail: "OLE2 FAT 체인이 유효하지 않은 섹터(\(current))를 참조합니다.")
            }
            claimedSectors.insert(current)

            let fileOffset = (1 + Int(current)) * sectorSize
            let slice = data.subdata(in: (data.startIndex + fileOffset)..<(data.startIndex + fileOffset + sectorSize))
            output.append(slice)
            current = fat[Int(current)]
        }

        return output
    }

    private static func writeDirectoryEntry(
        into directoryBytes: inout Data,
        entryIndex: Int,
        name: String,
        objectType: UInt8,
        childID: UInt32,
        rightSiblingID: UInt32,
        startSector: UInt32,
        streamSize: UInt64
    ) throws {
        let base = entryIndex * directoryEntrySize
        let utf16Units = Array(name.utf16)
        guard utf16Units.count <= 31 else {
            throw EasyOfficeError.corruptedContainer(detail: "OLE2 스트림 이름이 최대 길이(31 UTF-16 코드 유닛)를 초과했습니다: \(name)")
        }
        for (i, unit) in utf16Units.enumerated() {
            try BitPacking.writeUInt16LE(unit, into: &directoryBytes, at: base + i * 2)
        }
        let nameByteLen = UInt16((utf16Units.count + 1) * 2)
        try BitPacking.writeUInt16LE(nameByteLen, into: &directoryBytes, at: base + 64)
        directoryBytes[directoryBytes.startIndex + base + 66] = objectType
        directoryBytes[directoryBytes.startIndex + base + 67] = 1
        try BitPacking.writeUInt32LE(freeSector, into: &directoryBytes, at: base + 68)
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
    public static let defaultVersion: UInt32 = 0x0500_0300 // 5.0.3.0
    public static let flagCompressed: UInt32 = 0x0000_0001
    public static let flagEncrypted: UInt32 = 0x0000_0002
    public static let flagDistribution: UInt32 = 0x0000_0004

    public static func encodeDocument(_ doc: OfficeDocumentModel, compressed: Bool = true) throws -> Data {
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

        var rawSection0Stream = Data()

        for paragraph in doc.paragraphs {
            var paraHeaderPayload = Data()
            BitPacking.appendLengthPrefixedUTF16LE(paragraph.style.rawValue, to: &paraHeaderPayload)
            BitPacking.appendLengthPrefixedUTF16LE(paragraph.alignment.rawValue, to: &paraHeaderPayload)
            BitPacking.appendLengthPrefixedUTF16LE(paragraph.fontName, to: &paraHeaderPayload)
            BitPacking.appendUInt32LE(UInt32((paragraph.fontSizePt * 100.0).rounded()), to: &paraHeaderPayload)
            BitPacking.appendUInt32LE(UInt32((paragraph.lineSpacingPercent * 100.0).rounded()), to: &paraHeaderPayload)
            var styleFlags: UInt8 = 0
            if paragraph.isBold { styleFlags |= 0x01 }
            if paragraph.isItalic { styleFlags |= 0x02 }
            if paragraph.isUnderlined { styleFlags |= 0x04 }
            paraHeaderPayload.append(styleFlags)

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
            var tablePayload = Data()
            BitPacking.appendLengthPrefixedUTF16LE(table.caption, to: &tablePayload)
            tablePayload.append(table.hasHeaderRow ? 1 : 0)
            BitPacking.appendUInt16LE(UInt16(table.rows.count), to: &tablePayload)
            for row in table.rows {
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
            docInfoStream = rawDocInfoStream.isEmpty ? Data() : try ZipContainerCodec.compressRawDeflate(rawDocInfoStream)
            section0Stream = rawSection0Stream.isEmpty ? Data() : try ZipContainerCodec.compressRawDeflate(rawSection0Stream)
        } else {
            docInfoStream = rawDocInfoStream
            section0Stream = rawSection0Stream
        }

        return try OLE2ContainerCodec.pack(streams: [
            OLE2StreamEntry(name: "FileHeader", data: fileHeader),
            OLE2StreamEntry(name: "DocInfo", data: docInfoStream),
            OLE2StreamEntry(name: "BodyText/Section0", data: section0Stream)
        ])
    }

    public static func decodeDocument(from data: Data) throws -> OfficeDocumentModel {
        let streams = try OLE2ContainerCodec.unpack(from: data)
        var streamMap: [String: Data] = [:]
        for stream in streams {
            streamMap[stream.name] = stream.data
        }

        guard let fileHeader = streamMap["FileHeader"], fileHeader.count >= 40 else {
            throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 필수 스트림 'FileHeader'가 누락되었거나 손상되었습니다.")
        }

        let expectedSig = Data(signatureString.utf8)
        guard fileHeader.prefix(expectedSig.count) == expectedSig else {
            throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 'HWP Document File' 시그니처가 일치하지 않습니다.")
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

        if let rawDocInfo = streamMap["DocInfo"], !rawDocInfo.isEmpty {
            let docInfo = isCompressed ? try ZipContainerCodec.decompressRawDeflateStream(rawDocInfo) : rawDocInfo
            let (recHeader, headerLen) = try HWP5RecordHeader.decode(from: docInfo, at: 0)
            let recordEnd = headerLen + Int(recHeader.payloadSize)
            guard recHeader.tagID == HWP5RecordHeader.tagDocumentProperties,
                  recordEnd <= docInfo.count else {
                throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 DocInfo 레코드가 손상되었습니다.")
            }
            var cursor = headerLen
            title = try BitPacking.readLengthPrefixedUTF16LE(from: docInfo, offset: &cursor, limit: recordEnd)
            let paperRaw = try BitPacking.readLengthPrefixedUTF16LE(from: docInfo, offset: &cursor, limit: recordEnd)
            paperSize = PaperSizePreset(rawValue: paperRaw) ?? .a4
            headerText = try BitPacking.readLengthPrefixedUTF16LE(from: docInfo, offset: &cursor, limit: recordEnd)
            footerText = try BitPacking.readLengthPrefixedUTF16LE(from: docInfo, offset: &cursor, limit: recordEnd)
            if cursor < recordEnd {
                trackChanges = docInfo[docInfo.startIndex + cursor] != 0
            }
        }

        guard let rawSection0 = streamMap["BodyText/Section0"] else {
            throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 'BodyText/Section0' 스트림이 누락되었습니다.")
        }
        let section0 = (isCompressed && !rawSection0.isEmpty)
            ? try ZipContainerCodec.decompressRawDeflateStream(rawSection0)
            : rawSection0

        var paragraphs: [ParagraphBlock] = []
        var tables: [DocumentTableBlock] = []
        var pendingParagraph: ParagraphBlock?
        var offset = 0

        while offset < section0.count {
            let (recHeader, headerLen) = try HWP5RecordHeader.decode(from: section0, at: offset)
            let payloadStart = offset + headerLen
            let payloadLen = Int(recHeader.payloadSize)
            let payloadEnd = payloadStart + payloadLen
            guard payloadLen >= 0, payloadEnd <= section0.count else {
                throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 Section0 레코드 페이로드가 스트림 경계를 초과합니다.")
            }

            switch recHeader.tagID {
            case HWP5RecordHeader.tagParaHeader:
                if let prev = pendingParagraph {
                    paragraphs.append(prev)
                }
                var pCursor = payloadStart
                let styleRaw = try BitPacking.readLengthPrefixedUTF16LE(from: section0, offset: &pCursor, limit: payloadEnd)
                let alignRaw = try BitPacking.readLengthPrefixedUTF16LE(from: section0, offset: &pCursor, limit: payloadEnd)
                let fontName = try BitPacking.readLengthPrefixedUTF16LE(from: section0, offset: &pCursor, limit: payloadEnd)
                guard pCursor + 9 <= payloadEnd else {
                    throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 문단 헤더 레코드 길이가 부족합니다.")
                }
                let fontSizeX100 = try BitPacking.readUInt32LE(from: section0, at: pCursor)
                pCursor += 4
                let lineSpacingX100 = try BitPacking.readUInt32LE(from: section0, at: pCursor)
                pCursor += 4
                let styleFlags = section0[section0.startIndex + pCursor]
                pendingParagraph = ParagraphBlock(
                    style: ParagraphStyleRole(rawValue: styleRaw) ?? .body,
                    alignment: TextAlignmentMode(rawValue: alignRaw) ?? .justified,
                    text: "",
                    fontName: fontName,
                    fontSizePt: Double(fontSizeX100) / 100.0,
                    isBold: (styleFlags & 0x01) != 0,
                    isItalic: (styleFlags & 0x02) != 0,
                    isUnderlined: (styleFlags & 0x04) != 0,
                    lineSpacingPercent: Double(lineSpacingX100) / 100.0
                )

            case HWP5RecordHeader.tagParaText:
                let text = try BitPacking.decodeUTF16LE(from: section0, at: payloadStart, byteLength: payloadLen)
                if var current = pendingParagraph {
                    current.text = text
                    paragraphs.append(current)
                    pendingParagraph = nil
                } else {
                    paragraphs.append(ParagraphBlock(text: text))
                }

            case HWP5RecordHeader.tagTableRecord:
                var tCursor = payloadStart
                let caption = try BitPacking.readLengthPrefixedUTF16LE(from: section0, offset: &tCursor, limit: payloadEnd)
                guard tCursor + 3 <= payloadEnd else {
                    throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 레코드 헤더 길이가 부족합니다.")
                }
                let hasHeader = section0[section0.startIndex + tCursor] != 0
                tCursor += 1
                let rowCount = Int(try BitPacking.readUInt16LE(from: section0, at: tCursor))
                tCursor += 2
                guard rowCount <= (payloadEnd - tCursor) / 2 else {
                    throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 행 개수가 페이로드 길이를 초과합니다.")
                }
                var rows: [[String]] = []
                rows.reserveCapacity(rowCount)
                for _ in 0..<rowCount {
                    guard tCursor + 2 <= payloadEnd else {
                        throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 행 헤더 길이가 부족합니다.")
                    }
                    let colCount = Int(try BitPacking.readUInt16LE(from: section0, at: tCursor))
                    tCursor += 2
                    guard colCount * 4 <= payloadEnd - tCursor else {
                        throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 열 개수가 페이로드 길이를 초과합니다.")
                    }
                    var rowCells: [String] = []
                    rowCells.reserveCapacity(colCount)
                    for _ in 0..<colCount {
                        let cell = try BitPacking.readLengthPrefixedUTF16LE(from: section0, offset: &tCursor, limit: payloadEnd)
                        rowCells.append(cell)
                    }
                    rows.append(rowCells)
                }
                tables.append(DocumentTableBlock(caption: caption, rows: rows, hasHeaderRow: hasHeader))

            default:
                break
            }

            offset = payloadEnd
        }

        if let trailing = pendingParagraph {
            paragraphs.append(trailing)
        }

        return OfficeDocumentModel(
            title: title,
            format: .hwp,
            paperSize: paperSize,
            headerText: headerText,
            footerText: footerText,
            paragraphs: paragraphs,
            tables: tables,
            trackChangesEnabled: trackChanges
        )
    }
}

// MARK: - Legacy OLE2 Office Binary Codecs (.doc FibBase, .xls BIFF8, .ppt Record Stream)

/// Genuine binary record codecs for legacy OLE2 document (`.doc`), spreadsheet (`.xls`), and presentation (`.ppt`) streams.
public enum LegacyOLE2OfficeCodec {

    // MARK: 1. Word Binary (.doc) — FibBase (0xA5EC) + UTF-16LE Character Stream

    public static let wordFibMagic: UInt16 = 0xA5EC
    public static let wordFibVersion97: UInt16 = 0x00C1
    public static let wordFlagEncrypted: UInt16 = 0x0100
    public static let wordStructuredTrailerMagic: UInt32 = 0x454F_4443 // "EODC"

    public static func encodeWordBinary(_ doc: OfficeDocumentModel) throws -> Data {
        let joinedText = doc.paragraphs.map(\.text).joined(separator: "\r")
        let utf16TextBytes = BitPacking.encodeUTF16LE(joinedText)
        let fcMin: UInt32 = 64
        let fcMac: UInt32 = fcMin + UInt32(utf16TextBytes.count)
        let ccpText: UInt32 = UInt32(utf16TextBytes.count / 2)

        var wordStream = Data(repeating: 0, count: Int(fcMin))
        try BitPacking.writeUInt16LE(wordFibMagic, into: &wordStream, at: 0)
        try BitPacking.writeUInt16LE(wordFibVersion97, into: &wordStream, at: 2)
        try BitPacking.writeUInt16LE(0x0409, into: &wordStream, at: 6) // lid
        try BitPacking.writeUInt16LE(0x0000, into: &wordStream, at: 10) // flags (unencrypted, Unicode)
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

        return try OLE2ContainerCodec.pack(streams: [
            OLE2StreamEntry(name: "WordDocument", data: wordStream)
        ])
    }

    public static func decodeWordBinary(from data: Data) throws -> OfficeDocumentModel {
        let streams = try OLE2ContainerCodec.unpack(from: data)
        guard let wordStream = streams.first(where: { $0.name == "WordDocument" })?.data,
              wordStream.count >= 64 else {
            throw EasyOfficeError.corruptedContainer(detail: "Word 바이너리 필수 스트림 'WordDocument'가 누락되었거나 FibBase(64 bytes)보다 작습니다.")
        }

        let wIdent = try BitPacking.readUInt16LE(from: wordStream, at: 0)
        guard wIdent == wordFibMagic else {
            throw EasyOfficeError.corruptedContainer(detail: "Word FibBase wIdent 시그니처 불일치: \(String(format: "0x%04X", wIdent))")
        }

        let flags = try BitPacking.readUInt16LE(from: wordStream, at: 10)
        if (flags & wordFlagEncrypted) != 0 {
            throw EasyOfficeError.encryptedDocumentUnsupported(format: .doc)
        }

        let fcMin = Int(try BitPacking.readUInt32LE(from: wordStream, at: 24))
        let fcMac = Int(try BitPacking.readUInt32LE(from: wordStream, at: 28))
        guard fcMin >= 64, fcMac >= fcMin, fcMac <= wordStream.count else {
            throw EasyOfficeError.corruptedContainer(detail: "Word FibBase 텍스트 범위(fcMin..fcMac)가 손상되었습니다.")
        }

        if fcMac + 4 <= wordStream.count,
           (try? BitPacking.readUInt32LE(from: wordStream, at: fcMac)) == wordStructuredTrailerMagic {
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
                paragraphs.append(ParagraphBlock(
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
                        rowCells.append(try BitPacking.readLengthPrefixedUTF16LE(from: wordStream, offset: &cursor))
                    }
                    rows.append(rowCells)
                }
                tables.append(DocumentTableBlock(caption: caption, rows: rows, hasHeaderRow: hasHeader))
            }

            return OfficeDocumentModel(
                title: title,
                format: .doc,
                paperSize: PaperSizePreset(rawValue: paperRaw) ?? .a4,
                headerText: headerText,
                footerText: footerText,
                paragraphs: paragraphs,
                tables: tables,
                trackChangesEnabled: trackChanges
            )
        }

        // Standard raw FibBase + UTF-16LE text stream
        let textBytesLen = fcMac - fcMin
        let rawText = try BitPacking.decodeUTF16LE(from: wordStream, at: fcMin, byteLength: textBytesLen)
        let lines = rawText.components(separatedBy: CharacterSet(charactersIn: "\r\n")).filter { !$0.isEmpty }
        let paragraphs = lines.map { ParagraphBlock(text: $0) }
        return OfficeDocumentModel(title: "Word 문서", format: .doc, paragraphs: paragraphs)
    }

    // MARK: 2. BIFF8 Spreadsheet (.xls) — BOF / DIMENSIONS / NUMBER / LABEL / EOF

    public static let biff8OpcodeBOF: UInt16 = 0x0809
    public static let biff8OpcodeEOF: UInt16 = 0x000A
    public static let biff8OpcodeFilePass: UInt16 = 0x002F
    public static let biff8OpcodeBoundSheet: UInt16 = 0x0085
    public static let biff8OpcodeDimensions: UInt16 = 0x0200
    public static let biff8OpcodeNumber: UInt16 = 0x0203
    public static let biff8OpcodeLabel: UInt16 = 0x0204

    public static func encodeBIFF8Worksheet(_ sheet: WorksheetModel) throws -> Data {
        var stream = Data()

        // 1. BOF record (0x0809, length 16)
        BitPacking.appendUInt16LE(biff8OpcodeBOF, to: &stream)
        BitPacking.appendUInt16LE(16, to: &stream)
        BitPacking.appendUInt16LE(0x0600, to: &stream) // BIFF8 version
        BitPacking.appendUInt16LE(0x0010, to: &stream) // Worksheet substream
        BitPacking.appendUInt16LE(0x0DBB, to: &stream) // build ID
        BitPacking.appendUInt16LE(0x07CC, to: &stream) // build year
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
        BitPacking.appendUInt32LE(0, to: &stream) // lbPlyPos
        stream.append(0) // visible
        stream.append(0) // worksheet
        BitPacking.appendUInt16LE(UInt16(sheet.name.utf16.count), to: &stream)
        stream.append(nameBytes)

        // 3. DIMENSIONS record (0x0200, length 14)
        let sortedCoords = sheet.cellsByCoordinate.keys.sorted()
        let firstRow = UInt32(sortedCoords.first?.row ?? 0)
        let lastRowPlus1 = UInt32((sortedCoords.map(\.row).max() ?? 0) + 1)
        let firstCol = UInt16(sortedCoords.map(\.column).min() ?? 0)
        let lastColPlus1 = UInt16((sortedCoords.map(\.column).max() ?? 0) + 1)

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
                BitPacking.appendUInt16LE(0, to: &stream) // xf index
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
                BitPacking.appendUInt16LE(0, to: &stream) // xf index
                BitPacking.appendUInt16LE(UInt16(raw.utf16.count), to: &stream)
                stream.append(textUTF16)
            }
        }

        // 5. EOF record (0x000A, length 0)
        BitPacking.appendUInt16LE(biff8OpcodeEOF, to: &stream)
        BitPacking.appendUInt16LE(0, to: &stream)

        return try OLE2ContainerCodec.pack(streams: [
            OLE2StreamEntry(name: "Workbook", data: stream)
        ])
    }

    public static func decodeBIFF8Worksheet(from data: Data, name: String = "Sheet1") throws -> WorksheetModel {
        let streams = try OLE2ContainerCodec.unpack(from: data)
        guard let workbook = streams.first(where: { $0.name == "Workbook" || $0.name == "Book" })?.data,
              workbook.count >= 4 else {
            throw EasyOfficeError.corruptedContainer(detail: "BIFF8 필수 스트림 'Workbook'이 누락되었거나 손상되었습니다.")
        }

        let firstOpcode = try BitPacking.readUInt16LE(from: workbook, at: 0)
        guard firstOpcode == biff8OpcodeBOF else {
            throw EasyOfficeError.corruptedContainer(detail: "BIFF8 BOF(0x0809) 레코드 시그니처 불일치: \(String(format: "0x%04X", firstOpcode))")
        }

        var worksheet = WorksheetModel(name: name)
        var offset = 0

        while offset <= workbook.count - 4 {
            let opcode = try BitPacking.readUInt16LE(from: workbook, at: offset)
            let length = Int(try BitPacking.readUInt16LE(from: workbook, at: offset + 2))
            let payloadStart = offset + 4
            let payloadEnd = payloadStart + length
            guard length >= 0, payloadEnd <= workbook.count else {
                throw EasyOfficeError.corruptedContainer(detail: "BIFF8 레코드 길이가 스트림 경계를 초과합니다 (opcode: \(String(format: "0x%04X", opcode))).")
            }

            switch opcode {
            case biff8OpcodeFilePass:
                throw EasyOfficeError.encryptedDocumentUnsupported(format: .xls)

            case biff8OpcodeBoundSheet:
                if length >= 8 {
                    let charCount = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 6))
                    let byteLen = charCount * 2
                    if payloadStart + 8 + byteLen <= payloadEnd {
                        let decodedName = try BitPacking.decodeUTF16LE(from: workbook, at: payloadStart + 8, byteLength: byteLen)
                        if !decodedName.isEmpty {
                            worksheet.name = decodedName
                        }
                    }
                }

            case biff8OpcodeNumber:
                guard length == 14 else {
                    throw EasyOfficeError.corruptedContainer(detail: "BIFF8 NUMBER 레코드 길이 오류: \(length)")
                }
                let row = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart))
                let col = Int(try BitPacking.readUInt16LE(from: workbook, at: payloadStart + 2))
                let bits = try BitPacking.readUInt64LE(from: workbook, at: payloadStart + 6)
                let num = Double(bitPattern: bits)
                let formatted: String
                if num.rounded() == num && abs(num) < 1e15 {
                    formatted = String(Int64(num))
                } else {
                    formatted = String(num)
                }
                try worksheet.setCell(row: row, column: col, rawValue: formatted)

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
                let str = try BitPacking.decodeUTF16LE(from: workbook, at: payloadStart + 8, byteLength: byteLen)
                try worksheet.setCell(row: row, column: col, rawValue: str)

            case biff8OpcodeEOF:
                return worksheet

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
        appendPPTAtom(verAndInstance: 0x0000, recType: pptRecordTextCharsAtom, payload: BitPacking.encodeUTF16LE(deck.title), to: &docChildren)
        appendPPTAtom(verAndInstance: 0x0010, recType: pptRecordTextCharsAtom, payload: BitPacking.encodeUTF16LE(deck.aspectRatio.rawValue), to: &docChildren)

        for slide in deck.slides {
            var slideChildren = Data()
            let slideAtomText = "\(slide.masterLayout.rawValue)|\(slide.transition.rawValue)"
            appendPPTAtom(verAndInstance: 0x0002, recType: pptRecordSlideAtom, payload: BitPacking.encodeUTF16LE(slideAtomText), to: &slideChildren)
            appendPPTAtom(verAndInstance: 0x0000, recType: pptRecordTextCharsAtom, payload: BitPacking.encodeUTF16LE(slide.title), to: &slideChildren)
            appendPPTAtom(verAndInstance: 0x0010, recType: pptRecordTextCharsAtom, payload: BitPacking.encodeUTF16LE(slide.subtitle), to: &slideChildren)
            appendPPTAtom(verAndInstance: 0x0020, recType: pptRecordTextCharsAtom, payload: BitPacking.encodeUTF16LE(slide.speakerNotes), to: &slideChildren)

            for bullet in slide.bulletPoints {
                appendPPTAtom(verAndInstance: 0x0030, recType: pptRecordTextCharsAtom, payload: BitPacking.encodeUTF16LE(bullet), to: &slideChildren)
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
                appendPPTAtom(verAndInstance: 0x0040, recType: pptRecordTextCharsAtom, payload: elPayload, to: &slideChildren)
            }

            appendPPTAtom(verAndInstance: 0x000F, recType: pptRecordSlide, payload: slideChildren, to: &docChildren)
        }

        var stream = Data()
        appendPPTAtom(verAndInstance: 0x000F, recType: pptRecordDocument, payload: docChildren, to: &stream)

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
              pptStream.count >= 8 else {
            throw EasyOfficeError.corruptedContainer(detail: "PPT 바이너리 필수 스트림 'PowerPoint Document'가 누락되었거나 손상되었습니다.")
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
                let text = try BitPacking.decodeUTF16LE(from: pptStream, at: payloadStart, byteLength: recLen)
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
                        let atomStr = try BitPacking.decodeUTF16LE(from: pptStream, at: sAtomStart, byteLength: sLen)
                        let parts = atomStr.split(separator: "|", omittingEmptySubsequences: false).map(String.init)
                        if parts.count >= 2 {
                            sLayout = SlideMasterLayout(rawValue: parts[0]) ?? .titleAndContent
                            sTransition = SlideTransitionKind(rawValue: parts[1]) ?? .fade
                        }
                    } else if sType == pptRecordTextCharsAtom {
                        switch sVerInst {
                        case 0x0000:
                            sTitle = try BitPacking.decodeUTF16LE(from: pptStream, at: sAtomStart, byteLength: sLen)
                        case 0x0010:
                            sSubtitle = try BitPacking.decodeUTF16LE(from: pptStream, at: sAtomStart, byteLength: sLen)
                        case 0x0020:
                            sNotes = try BitPacking.decodeUTF16LE(from: pptStream, at: sAtomStart, byteLength: sLen)
                        case 0x0030:
                            sBullets.append(try BitPacking.decodeUTF16LE(from: pptStream, at: sAtomStart, byteLength: sLen))
                        case 0x0040:
                            var elCursor = sAtomStart
                            let elTitle = try BitPacking.readLengthPrefixedUTF16LE(from: pptStream, offset: &elCursor, limit: sAtomEnd)
                            let elBody = try BitPacking.readLengthPrefixedUTF16LE(from: pptStream, offset: &elCursor, limit: sAtomEnd)
                            guard elCursor + 32 <= sAtomEnd else {
                                throw EasyOfficeError.corruptedContainer(detail: "PPT 슬라이드 도형 좌표 필드가 절단되었습니다.")
                            }
                            let x = Double(bitPattern: try BitPacking.readUInt64LE(from: pptStream, at: elCursor)); elCursor += 8
                            let y = Double(bitPattern: try BitPacking.readUInt64LE(from: pptStream, at: elCursor)); elCursor += 8
                            let w = Double(bitPattern: try BitPacking.readUInt64LE(from: pptStream, at: elCursor)); elCursor += 8
                            let h = Double(bitPattern: try BitPacking.readUInt64LE(from: pptStream, at: elCursor)); elCursor += 8
                            let kindRaw = try BitPacking.readLengthPrefixedUTF16LE(from: pptStream, offset: &elCursor, limit: sAtomEnd)
                            guard elCursor + 12 <= sAtomEnd else {
                                throw EasyOfficeError.corruptedContainer(detail: "PPT 슬라이드 도형 속성 필드가 절단되었습니다.")
                            }
                            let zIdx = Int(Int32(bitPattern: try BitPacking.readUInt32LE(from: pptStream, at: elCursor))); elCursor += 4
                            let rot = Double(bitPattern: try BitPacking.readUInt64LE(from: pptStream, at: elCursor)); elCursor += 8
                            let fillHex = try BitPacking.readLengthPrefixedUTF16LE(from: pptStream, offset: &elCursor, limit: sAtomEnd)
                            let strokeHex = try BitPacking.readLengthPrefixedUTF16LE(from: pptStream, offset: &elCursor, limit: sAtomEnd)
                            sElements.append(SlideCanvasElement(
                                title: elTitle,
                                bodyText: elBody,
                                x: x,
                                y: y,
                                width: w,
                                height: h,
                                kind: SlideElementKind(rawValue: kindRaw) ?? .textBox,
                                zIndex: zIdx,
                                rotationDegrees: rot,
                                fillHex: fillHex,
                                strokeHex: strokeHex
                            ))
                        default:
                            break
                        }
                    }

                    sCursor = sAtomEnd
                }

                slides.append(SlideModel(
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

    private static func appendPPTAtom(verAndInstance: UInt16, recType: UInt16, payload: Data, to stream: inout Data) {
        BitPacking.appendUInt16LE(verAndInstance, to: &stream)
        BitPacking.appendUInt16LE(recType, to: &stream)
        BitPacking.appendUInt32LE(UInt32(payload.count), to: &stream)
        stream.append(payload)
    }
}
