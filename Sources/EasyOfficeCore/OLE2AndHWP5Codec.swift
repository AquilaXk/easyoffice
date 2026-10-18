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
        case paragraphs, tables, sections, footnotes, revisions, comments, bookmarks, images, equations, watermarkText
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
            doc.paragraphs[idx] = ParagraphBlock(
                id: metaP.id,
                style: p.style,
                alignment: p.alignment,
                text: p.text,
                runs: metaP.runs.isEmpty ? p.runs : metaP.runs,
                fontName: p.fontName,
                fontSizePt: p.fontSizePt,
                isBold: p.isBold,
                isItalic: p.isItalic,
                isUnderlined: p.isUnderlined,
                lineSpacingPercent: p.lineSpacingPercent
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
            claimedSectors: &claimedSectors,
            expectedByteCount: sectorSize
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
                    claimedSectors: &claimedSectors,
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
        claimedSectors: inout Set<UInt32>,
        expectedByteCount: Int? = nil
    ) throws -> Data {
        var output = Data()
        if let expectedByteCount, expectedByteCount > 0 {
            output.reserveCapacity(min(expectedByteCount + sectorSize, data.count))
        }
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
            let startIdx = data.startIndex + fileOffset
            output.append(contentsOf: data[startIdx..<(startIdx + sectorSize)])
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
        var extendedEnvelope: ExtendedDocumentMetadataEnvelope?

        if let rawDocInfo = streamMap["DocInfo"], !rawDocInfo.isEmpty {
            let docInfo = isCompressed ? try ZipContainerCodec.decompressRawDeflateStream(rawDocInfo) : rawDocInfo
            var docOffset = 0
            var parsedDocProps = false
            while docOffset < docInfo.count {
                let (recHeader, headerLen) = try HWP5RecordHeader.decode(from: docInfo, at: docOffset)
                let payloadStart = docOffset + headerLen
                let recordEnd = payloadStart + Int(recHeader.payloadSize)
                guard recordEnd <= docInfo.count else {
                    throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 DocInfo 레코드가 손상되었습니다.")
                }
                if recHeader.tagID == HWP5RecordHeader.tagDocumentProperties {
                    var cursor = payloadStart
                    title = try BitPacking.readLengthPrefixedUTF16LE(from: docInfo, offset: &cursor, limit: recordEnd)
                    let paperRaw = try BitPacking.readLengthPrefixedUTF16LE(from: docInfo, offset: &cursor, limit: recordEnd)
                    paperSize = PaperSizePreset(rawValue: paperRaw) ?? .a4
                    headerText = try BitPacking.readLengthPrefixedUTF16LE(from: docInfo, offset: &cursor, limit: recordEnd)
                    footerText = try BitPacking.readLengthPrefixedUTF16LE(from: docInfo, offset: &cursor, limit: recordEnd)
                    if cursor < recordEnd {
                        trackChanges = docInfo[docInfo.startIndex + cursor] != 0
                    }
                    parsedDocProps = true
                } else if recHeader.tagID == HWP5RecordHeader.tagExtendedMetadata {
                    let slice = Data(docInfo[(docInfo.startIndex + payloadStart)..<(docInfo.startIndex + recordEnd)])
                    extendedEnvelope = try? JSONDecoder().decode(ExtendedDocumentMetadataEnvelope.self, from: slice)
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
