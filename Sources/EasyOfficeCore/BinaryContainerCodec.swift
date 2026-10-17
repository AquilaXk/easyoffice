import Foundation
import Compression

// MARK: - Low-Level Bit Packing & IEEE 802.3 CRC-32

/// Deterministic little-endian binary reader and writer with Fail-Closed bounds verification.
public enum BitPacking {

    @inlinable
    public static func readUInt16LE(from data: Data, at offset: Int) throws -> UInt16 {
        guard offset >= 0, offset + 2 <= data.count else {
            throw EasyOfficeError.corruptedContainer(
                detail: "UInt16 읽기 범위 초과 (offset: \(offset), size: \(data.count))"
            )
        }
        let base = data.startIndex + offset
        let b0 = UInt16(data[base])
        let b1 = UInt16(data[base + 1]) << 8
        return b0 | b1
    }

    @inlinable
    public static func readUInt32LE(from data: Data, at offset: Int) throws -> UInt32 {
        guard offset >= 0, offset + 4 <= data.count else {
            throw EasyOfficeError.corruptedContainer(
                detail: "UInt32 읽기 범위 초과 (offset: \(offset), size: \(data.count))"
            )
        }
        let base = data.startIndex + offset
        let b0 = UInt32(data[base])
        let b1 = UInt32(data[base + 1]) << 8
        let b2 = UInt32(data[base + 2]) << 16
        let b3 = UInt32(data[base + 3]) << 24
        return b0 | b1 | b2 | b3
    }

    @inlinable
    public static func readUInt64LE(from data: Data, at offset: Int) throws -> UInt64 {
        guard offset >= 0, offset + 8 <= data.count else {
            throw EasyOfficeError.corruptedContainer(
                detail: "UInt64 읽기 범위 초과 (offset: \(offset), size: \(data.count))"
            )
        }
        let low = UInt64(try readUInt32LE(from: data, at: offset))
        let high = UInt64(try readUInt32LE(from: data, at: offset + 4)) << 32
        return low | high
    }

    @inlinable
    public static func appendUInt16LE(_ value: UInt16, to data: inout Data) {
        let le = value.littleEndian
        withUnsafeBytes(of: le) { data.append(contentsOf: $0) }
    }

    @inlinable
    public static func appendUInt32LE(_ value: UInt32, to data: inout Data) {
        let le = value.littleEndian
        withUnsafeBytes(of: le) { data.append(contentsOf: $0) }
    }

    @inlinable
    public static func appendUInt64LE(_ value: UInt64, to data: inout Data) {
        let le = value.littleEndian
        withUnsafeBytes(of: le) { data.append(contentsOf: $0) }
    }

    @inlinable
    public static func writeUInt16LE(_ value: UInt16, into data: inout Data, at offset: Int) throws {
        guard offset >= 0, offset + 2 <= data.count else {
            throw EasyOfficeError.corruptedContainer(
                detail: "UInt16 쓰기 범위 초과 (offset: \(offset), size: \(data.count))"
            )
        }
        let base = data.startIndex + offset
        data[base] = UInt8(truncatingIfNeeded: value)
        data[base + 1] = UInt8(truncatingIfNeeded: value >> 8)
    }

    @inlinable
    public static func writeUInt32LE(_ value: UInt32, into data: inout Data, at offset: Int) throws {
        guard offset >= 0, offset + 4 <= data.count else {
            throw EasyOfficeError.corruptedContainer(
                detail: "UInt32 쓰기 범위 초과 (offset: \(offset), size: \(data.count))"
            )
        }
        let base = data.startIndex + offset
        data[base] = UInt8(truncatingIfNeeded: value)
        data[base + 1] = UInt8(truncatingIfNeeded: value >> 8)
        data[base + 2] = UInt8(truncatingIfNeeded: value >> 16)
        data[base + 3] = UInt8(truncatingIfNeeded: value >> 24)
    }

    @inlinable
    public static func writeUInt64LE(_ value: UInt64, into data: inout Data, at offset: Int) throws {
        guard offset >= 0, offset + 8 <= data.count else {
            throw EasyOfficeError.corruptedContainer(
                detail: "UInt64 쓰기 범위 초과 (offset: \(offset), size: \(data.count))"
            )
        }
        try writeUInt32LE(UInt32(truncatingIfNeeded: value), into: &data, at: offset)
        try writeUInt32LE(UInt32(truncatingIfNeeded: value >> 32), into: &data, at: offset + 4)
    }

    /// Encodes a Swift String as UTF-16LE bytes prefixed with a 32-bit byte length.
    public static func appendLengthPrefixedUTF16LE(_ text: String, to data: inout Data) {
        var utf16Bytes = Data()
        utf16Bytes.reserveCapacity(text.utf16.count * 2)
        for codeUnit in text.utf16 {
            appendUInt16LE(codeUnit, to: &utf16Bytes)
        }
        appendUInt32LE(UInt32(utf16Bytes.count), to: &data)
        data.append(utf16Bytes)
    }

    /// Decodes a length-prefixed UTF-16LE String from `data` and advances `offset`.
    public static func readLengthPrefixedUTF16LE(from data: Data, offset: inout Int) throws -> String {
        let byteLength = Int(try readUInt32LE(from: data, at: offset))
        offset += 4
        guard byteLength >= 0, byteLength % 2 == 0, offset + byteLength <= data.count else {
            throw EasyOfficeError.corruptedContainer(
                detail: "유효하지 않은 UTF-16LE 문자열 길이 (\(byteLength) bytes)"
            )
        }
        let text = try decodeUTF16LE(from: data, at: offset, byteLength: byteLength)
        offset += byteLength
        return text
    }

    /// Decodes raw UTF-16LE bytes of `byteLength` starting at `offset`.
    public static func decodeUTF16LE(from data: Data, at offset: Int, byteLength: Int) throws -> String {
        guard offset >= 0, byteLength >= 0, byteLength % 2 == 0, offset + byteLength <= data.count else {
            throw EasyOfficeError.corruptedContainer(
                detail: "UTF-16LE 디코딩 범위 초과 (offset: \(offset), length: \(byteLength))"
            )
        }
        let unitCount = byteLength / 2
        var units: [UInt16] = []
        units.reserveCapacity(unitCount)
        for idx in 0..<unitCount {
            let unit = try readUInt16LE(from: data, at: offset + idx * 2)
            units.append(unit)
        }
        return String(decoding: units, as: UTF16.self)
    }
}

/// Table-driven IEEE 802.3 CRC-32 calculator (reflected polynomial `0xEDB88320`).
public enum CRC32Calculator {
    public static let polynomial: UInt32 = 0xEDB8_8320

    public static let lookupTable: [UInt32] = {
        (0..<256).map { index -> UInt32 in
            var crc = UInt32(index)
            for _ in 0..<8 {
                if (crc & 1) != 0 {
                    crc = (crc >> 1) ^ polynomial
                } else {
                    crc >>= 1
                }
            }
            return crc
        }
    }()

    public static func compute(for data: Data) -> UInt32 {
        var crc: UInt32 = 0xFFFF_FFFF
        data.withUnsafeBytes { rawBuffer in
            for byte in rawBuffer {
                let tableIndex = Int((crc ^ UInt32(byte)) & 0xFF)
                crc = (crc >> 8) ^ lookupTable[tableIndex]
            }
        }
        return ~crc
    }
}

// MARK: - HWP 5.0 32-Bit Record Header Bitfield

/// HWP 5.0 binary record header packing `tagID` (10 bits), `level` (10 bits), and `payloadSize` (12 bits or extended 32 bits).
public struct HWP5RecordHeader: Sendable, Equatable {
    public static let tagDocumentProperties: UInt16 = 16
    public static let tagParaHeader: UInt16 = 66
    public static let tagParaText: UInt16 = 67
    public static let tagTableRecord: UInt16 = 77

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
                detail: "HWP 5.0 레코드 헤더 10-bit 범위 초과 (tagID: \(tagID), level: \(level))"
            )
        }
        var output = Data()
        if payloadSize < extendedSizeMarker {
            let word = (UInt32(tagID) & tenBitMask)
                | ((UInt32(level) & tenBitMask) << 10)
                | ((payloadSize & twelveBitMask) << 20)
            BitPacking.appendUInt32LE(word, to: &output)
        } else {
            let word = (UInt32(tagID) & tenBitMask)
                | ((UInt32(level) & tenBitMask) << 10)
                | (extendedSizeMarker << 20)
            BitPacking.appendUInt32LE(word, to: &output)
            BitPacking.appendUInt32LE(payloadSize, to: &output)
        }
        return output
    }

    public static func decode(from data: Data, at offset: Int) throws -> (header: HWP5RecordHeader, headerByteLength: Int) {
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

// MARK: - ZIP Container Codec & Fail-Closed Security Policy

/// Compression methods supported inside standard ZIP-XML office containers.
public enum ZipCompressionMethod: UInt16, Sendable, Equatable {
    case store = 0
    case deflate = 8
}

/// Single entry inside a ZIP archive.
public struct ZipContainerEntry: Sendable, Equatable {
    public let path: String
    public let data: Data
    public let compressionMethod: ZipCompressionMethod

    public init(path: String, data: Data, compressionMethod: ZipCompressionMethod = .deflate) {
        self.path = path
        self.data = data
        self.compressionMethod = compressionMethod
    }
}

/// Security policy enforcing entry limits, path safety, and decompression bomb defense.
public struct ZipContainerSecurityPolicy: Sendable, Equatable {
    public let maxEntryCount: Int
    public let maxSingleEntryUncompressedBytes: Int
    public let maxTotalUncompressedBytes: Int
    public let maxCompressionRatio: Double

    public init(
        maxEntryCount: Int = 4096,
        maxSingleEntryUncompressedBytes: Int = 128 * 1024 * 1024,
        maxTotalUncompressedBytes: Int = 512 * 1024 * 1024,
        maxCompressionRatio: Double = 100.0
    ) {
        self.maxEntryCount = maxEntryCount
        self.maxSingleEntryUncompressedBytes = maxSingleEntryUncompressedBytes
        self.maxTotalUncompressedBytes = maxTotalUncompressedBytes
        self.maxCompressionRatio = maxCompressionRatio
    }

    public static let strictDefault = ZipContainerSecurityPolicy()
}

/// Bit-accurate ZIP archive reader and writer supporting Store (`0`) and Raw Deflate (`8`).
public enum ZipContainerCodec {
    public static let localFileHeaderSignature: UInt32 = 0x0403_4B50
    public static let centralDirectorySignature: UInt32 = 0x0201_4B50
    public static let endOfCentralDirectorySignature: UInt32 = 0x0605_4B50
    private static let zipVersionNeeded: UInt16 = 20

    public static func validateEntryPath(_ path: String) throws {
        guard !path.isEmpty else {
            throw EasyOfficeError.securityViolation(detail: "비어 있는 ZIP 엔트리 경로는 허용되지 않습니다.")
        }
        if path.hasPrefix("/") || path.contains("\\") || path.utf8.contains(0) {
            throw EasyOfficeError.securityViolation(detail: "절대 경로, 역슬래시, 또는 NUL 바이트가 포함된 ZIP 경로: \(path)")
        }
        let segments = path.split(separator: "/", omittingEmptySubsequences: false)
        for segment in segments {
            if segment.isEmpty || segment == "." || segment == ".." {
                throw EasyOfficeError.securityViolation(detail: "상위 디렉터리 순회(..) 또는 비정상 세그먼트가 포함된 ZIP 경로: \(path)")
            }
        }
    }

    public static func pack(entries: [ZipContainerEntry]) throws -> Data {
        var archive = Data()
        var centralDirectory = Data()

        for entry in entries {
            try validateEntryPath(entry.path)
            let pathBytes = Data(entry.path.utf8)
            let uncompressedSize = UInt32(entry.data.count)
            let crc32 = CRC32Calculator.compute(for: entry.data)

            let payloadBytes: Data
            let methodCode: UInt16
            switch entry.compressionMethod {
            case .store:
                payloadBytes = entry.data
                methodCode = ZipCompressionMethod.store.rawValue
            case .deflate:
                if entry.data.isEmpty {
                    payloadBytes = Data()
                    methodCode = ZipCompressionMethod.store.rawValue
                } else {
                    payloadBytes = try compressRawDeflate(entry.data)
                    methodCode = ZipCompressionMethod.deflate.rawValue
                }
            }

            let compressedSize = UInt32(payloadBytes.count)
            let localHeaderOffset = UInt32(archive.count)

            // Local File Header (30 bytes + file name)
            BitPacking.appendUInt32LE(localFileHeaderSignature, to: &archive)
            BitPacking.appendUInt16LE(zipVersionNeeded, to: &archive)
            BitPacking.appendUInt16LE(0x0800, to: &archive) // UTF-8 filename flag (bit 11)
            BitPacking.appendUInt16LE(methodCode, to: &archive)
            BitPacking.appendUInt16LE(0, to: &archive) // mod time
            BitPacking.appendUInt16LE(0, to: &archive) // mod date
            BitPacking.appendUInt32LE(crc32, to: &archive)
            BitPacking.appendUInt32LE(compressedSize, to: &archive)
            BitPacking.appendUInt32LE(uncompressedSize, to: &archive)
            BitPacking.appendUInt16LE(UInt16(pathBytes.count), to: &archive)
            BitPacking.appendUInt16LE(0, to: &archive) // extra field length
            archive.append(pathBytes)
            archive.append(payloadBytes)

            // Central Directory File Header (46 bytes + file name)
            BitPacking.appendUInt32LE(centralDirectorySignature, to: &centralDirectory)
            BitPacking.appendUInt16LE(zipVersionNeeded, to: &centralDirectory) // version made by
            BitPacking.appendUInt16LE(zipVersionNeeded, to: &centralDirectory) // version needed
            BitPacking.appendUInt16LE(0x0800, to: &centralDirectory) // UTF-8 flag
            BitPacking.appendUInt16LE(methodCode, to: &centralDirectory)
            BitPacking.appendUInt16LE(0, to: &centralDirectory)
            BitPacking.appendUInt16LE(0, to: &centralDirectory)
            BitPacking.appendUInt32LE(crc32, to: &centralDirectory)
            BitPacking.appendUInt32LE(compressedSize, to: &centralDirectory)
            BitPacking.appendUInt32LE(uncompressedSize, to: &centralDirectory)
            BitPacking.appendUInt16LE(UInt16(pathBytes.count), to: &centralDirectory)
            BitPacking.appendUInt16LE(0, to: &centralDirectory) // extra length
            BitPacking.appendUInt16LE(0, to: &centralDirectory) // comment length
            BitPacking.appendUInt16LE(0, to: &centralDirectory) // disk number
            BitPacking.appendUInt16LE(0, to: &centralDirectory) // internal attrs
            BitPacking.appendUInt32LE(0, to: &centralDirectory) // external attrs
            BitPacking.appendUInt32LE(localHeaderOffset, to: &centralDirectory)
            centralDirectory.append(pathBytes)
        }

        let centralDirOffset = UInt32(archive.count)
        let centralDirSize = UInt32(centralDirectory.count)
        archive.append(centralDirectory)

        // End of Central Directory Record (22 bytes)
        BitPacking.appendUInt32LE(endOfCentralDirectorySignature, to: &archive)
        BitPacking.appendUInt16LE(0, to: &archive) // disk number
        BitPacking.appendUInt16LE(0, to: &archive) // disk with CD
        BitPacking.appendUInt16LE(UInt16(entries.count), to: &archive)
        BitPacking.appendUInt16LE(UInt16(entries.count), to: &archive)
        BitPacking.appendUInt32LE(centralDirSize, to: &archive)
        BitPacking.appendUInt32LE(centralDirOffset, to: &archive)
        BitPacking.appendUInt16LE(0, to: &archive) // comment length

        return archive
    }

    public static func unpack(
        from data: Data,
        policy: ZipContainerSecurityPolicy = .strictDefault
    ) throws -> [ZipContainerEntry] {
        guard data.count >= 22 else {
            throw EasyOfficeError.corruptedContainer(detail: "ZIP 컨테이너 크기가 최소 헤더(22 bytes)보다 작습니다.")
        }
        guard let eocdOffset = findEndOfCentralDirectory(in: data) else {
            throw EasyOfficeError.corruptedContainer(detail: "ZIP End of Central Directory 시그니처(0x06054B50)를 찾을 수 없습니다.")
        }

        let entryCount = Int(try BitPacking.readUInt16LE(from: data, at: eocdOffset + 10))
        guard entryCount <= policy.maxEntryCount else {
            throw EasyOfficeError.securityViolation(
                detail: "ZIP 엔트리 개수(\(entryCount))가 보안 허용 한도(\(policy.maxEntryCount))를 초과했습니다."
            )
        }

        let centralDirSize = Int(try BitPacking.readUInt32LE(from: data, at: eocdOffset + 12))
        let centralDirOffset = Int(try BitPacking.readUInt32LE(from: data, at: eocdOffset + 16))
        guard centralDirOffset >= 0, centralDirSize >= 0, centralDirOffset + centralDirSize <= eocdOffset else {
            throw EasyOfficeError.corruptedContainer(detail: "ZIP Central Directory 오프셋 또는 크기가 손상되었습니다.")
        }

        var results: [ZipContainerEntry] = []
        results.reserveCapacity(entryCount)

        var cdCursor = centralDirOffset
        var totalUncompressedBytes = 0

        for _ in 0..<entryCount {
            let signature = try BitPacking.readUInt32LE(from: data, at: cdCursor)
            guard signature == centralDirectorySignature else {
                throw EasyOfficeError.corruptedContainer(detail: "잘못된 Central Directory 시그니처: \(String(format: "0x%08X", signature))")
            }

            let generalFlags = try BitPacking.readUInt16LE(from: data, at: cdCursor + 8)
            if (generalFlags & 0x0001) != 0 {
                throw EasyOfficeError.encryptedDocumentUnsupported(format: .docx)
            }

            let rawMethod = try BitPacking.readUInt16LE(from: data, at: cdCursor + 10)
            guard let method = ZipCompressionMethod(rawValue: rawMethod) else {
                throw EasyOfficeError.corruptedContainer(detail: "지원하지 않는 ZIP 압축 방식 코드: \(rawMethod)")
            }

            let expectedCRC32 = try BitPacking.readUInt32LE(from: data, at: cdCursor + 16)
            let compressedSize = Int(try BitPacking.readUInt32LE(from: data, at: cdCursor + 20))
            let uncompressedSize = Int(try BitPacking.readUInt32LE(from: data, at: cdCursor + 24))
            let nameLen = Int(try BitPacking.readUInt16LE(from: data, at: cdCursor + 28))
            let extraLen = Int(try BitPacking.readUInt16LE(from: data, at: cdCursor + 30))
            let commentLen = Int(try BitPacking.readUInt16LE(from: data, at: cdCursor + 32))
            let localHeaderOffset = Int(try BitPacking.readUInt32LE(from: data, at: cdCursor + 42))

            let nameStart = cdCursor + 46
            guard nameStart + nameLen <= data.count else {
                throw EasyOfficeError.corruptedContainer(detail: "ZIP Central Directory 파일명 영역이 절단되었습니다.")
            }
            let nameData = data.subdata(in: (data.startIndex + nameStart)..<(data.startIndex + nameStart + nameLen))
            guard let path = String(data: nameData, encoding: .utf8) else {
                throw EasyOfficeError.corruptedContainer(detail: "ZIP 엔트리 파일명이 유효한 UTF-8이 아닙니다.")
            }
            try validateEntryPath(path)

            guard uncompressedSize >= 0, uncompressedSize <= policy.maxSingleEntryUncompressedBytes else {
                throw EasyOfficeError.securityViolation(
                    detail: "ZIP 단일 엔트리 크기(\(uncompressedSize))가 한도(\(policy.maxSingleEntryUncompressedBytes))를 초과했습니다."
                )
            }
            totalUncompressedBytes += uncompressedSize
            guard totalUncompressedBytes <= policy.maxTotalUncompressedBytes else {
                throw EasyOfficeError.securityViolation(
                    detail: "ZIP 누적 압축 해제 크기(\(totalUncompressedBytes))가 한도(\(policy.maxTotalUncompressedBytes))를 초과했습니다."
                )
            }

            if compressedSize > 0 {
                let ratio = Double(uncompressedSize) / Double(compressedSize)
                if ratio > policy.maxCompressionRatio {
                    throw EasyOfficeError.decompressionBombDetected(ratio: ratio, maxRatio: policy.maxCompressionRatio)
                }
            } else if uncompressedSize > 0 {
                throw EasyOfficeError.corruptedContainer(detail: "압축 크기가 0이지만 원본 크기가 0보다 큽니다.")
            }

            // Validate Local File Header at localHeaderOffset
            let localSig = try BitPacking.readUInt32LE(from: data, at: localHeaderOffset)
            guard localSig == localFileHeaderSignature else {
                throw EasyOfficeError.corruptedContainer(detail: "Local File Header 시그니처 불일치")
            }
            let localNameLen = Int(try BitPacking.readUInt16LE(from: data, at: localHeaderOffset + 26))
            let localExtraLen = Int(try BitPacking.readUInt16LE(from: data, at: localHeaderOffset + 28))
            let payloadStart = localHeaderOffset + 30 + localNameLen + localExtraLen
            guard payloadStart >= 0, compressedSize >= 0, payloadStart + compressedSize <= data.count else {
                throw EasyOfficeError.corruptedContainer(detail: "ZIP 엔트리 '\(path)' 데이터 영역이 파일 경계를 벗어납니다.")
            }

            let compressedSlice = data.subdata(in: (data.startIndex + payloadStart)..<(data.startIndex + payloadStart + compressedSize))
            let uncompressedData: Data
            switch method {
            case .store:
                guard compressedSize == uncompressedSize else {
                    throw EasyOfficeError.corruptedContainer(detail: "Store 엔트리 '\(path)'의 압축/원본 크기가 일치하지 않습니다.")
                }
                uncompressedData = compressedSlice
            case .deflate:
                if uncompressedSize == 0 {
                    uncompressedData = Data()
                } else {
                    uncompressedData = try decompressRawDeflate(compressedSlice, expectedSize: uncompressedSize)
                }
            }

            let actualCRC32 = CRC32Calculator.compute(for: uncompressedData)
            guard actualCRC32 == expectedCRC32 else {
                throw EasyOfficeError.corruptedContainer(
                    detail: "ZIP 엔트리 '\(path)' CRC-32 무결성 검증 실패 (expected: \(String(format: "0x%08X", expectedCRC32)), actual: \(String(format: "0x%08X", actualCRC32)))"
                )
            }

            results.append(ZipContainerEntry(path: path, data: uncompressedData, compressionMethod: method))
            cdCursor = nameStart + nameLen + extraLen + commentLen
        }

        return results
    }

    private static func findEndOfCentralDirectory(in data: Data) -> Int? {
        let minOffset = max(0, data.count - 65_557)
        for offset in stride(from: data.count - 22, through: minOffset, by: -1) {
            if let sig = try? BitPacking.readUInt32LE(from: data, at: offset),
               sig == endOfCentralDirectorySignature {
                return offset
            }
        }
        return nil
    }

    private static func compressRawDeflate(_ input: Data) throws -> Data {
        let destinationCapacity = max(input.count + (input.count / 8) + 512, 1024)
        var destination = Data(count: destinationCapacity)
        let compressedBytes = input.withUnsafeBytes { srcBuffer -> Int in
            guard let srcBase = srcBuffer.bindMemory(to: UInt8.self).baseAddress else { return 0 }
            return destination.withUnsafeMutableBytes { dstBuffer -> Int in
                guard let dstBase = dstBuffer.bindMemory(to: UInt8.self).baseAddress else { return 0 }
                return compression_encode_buffer(
                    dstBase,
                    destinationCapacity,
                    srcBase,
                    input.count,
                    nil,
                    COMPRESSION_ZLIB
                )
            }
        }
        guard compressedBytes > 0 else {
            throw EasyOfficeError.corruptedContainer(detail: "Deflate 압축에 실패했습니다.")
        }
        destination.removeSubrange(compressedBytes..<destination.count)
        return destination
    }

    private static func decompressRawDeflate(_ input: Data, expectedSize: Int) throws -> Data {
        var destination = Data(count: expectedSize)
        let decodedBytes = input.withUnsafeBytes { srcBuffer -> Int in
            guard let srcBase = srcBuffer.bindMemory(to: UInt8.self).baseAddress else { return 0 }
            return destination.withUnsafeMutableBytes { dstBuffer -> Int in
                guard let dstBase = dstBuffer.bindMemory(to: UInt8.self).baseAddress else { return 0 }
                return compression_decode_buffer(
                    dstBase,
                    expectedSize,
                    srcBase,
                    input.count,
                    nil,
                    COMPRESSION_ZLIB
                )
            }
        }
        guard decodedBytes == expectedSize else {
            throw EasyOfficeError.corruptedContainer(
                detail: "Deflate 압축 해제 크기 불일치 (expected: \(expectedSize), actual: \(decodedBytes))"
            )
        }
        return destination
    }
}

// MARK: - XML Sanitizer (XXE & External DTD Guard)

/// Fail-Closed XML sanitizer blocking XXE/external DTD payloads and providing deterministic escaping.
public enum XMLSanitizer {

    public static func validateSafeXML(_ xmlString: String) throws {
        let upper = xmlString.uppercased()
        if upper.contains("<!ENTITY") {
            throw EasyOfficeError.xxeOrExternalEntityDetected
        }
        if let doctypeRange = upper.range(of: "<!DOCTYPE") {
            let tail = upper[doctypeRange.lowerBound...]
            if let closeBracket = tail.firstIndex(of: ">") {
                let headerSlice = tail[...closeBracket]
                if headerSlice.contains("SYSTEM") || headerSlice.contains("PUBLIC") || headerSlice.contains("[") {
                    throw EasyOfficeError.xxeOrExternalEntityDetected
                }
            } else {
                throw EasyOfficeError.xxeOrExternalEntityDetected
            }
        }
    }

    public static func validateSafeXMLData(_ data: Data) throws -> String {
        guard let xmlString = String(data: data, encoding: .utf8) else {
            throw EasyOfficeError.corruptedContainer(detail: "XML 스트림이 유효한 UTF-8 인코딩이 아닙니다.")
        }
        try validateSafeXML(xmlString)
        return xmlString
    }

    public static func escape(_ input: String) -> String {
        var result = ""
        result.reserveCapacity(input.count)
        for ch in input {
            switch ch {
            case "&": result.append("&amp;")
            case "<": result.append("&lt;")
            case ">": result.append("&gt;")
            case "\"": result.append("&quot;")
            case "'": result.append("&apos;")
            default: result.append(ch)
            }
        }
        return result
    }

    public static func unescape(_ input: String) -> String {
        input
            .replacingOccurrences(of: "&quot;", with: "\"")
            .replacingOccurrences(of: "&apos;", with: "'")
            .replacingOccurrences(of: "&lt;", with: "<")
            .replacingOccurrences(of: "&gt;", with: ">")
            .replacingOccurrences(of: "&amp;", with: "&")
    }

    /// Extracts all XML elements matching `<tagName ...>inner</tagName>` along with their opening tag attributes.
    static func extractElements(named tagName: String, in xml: String) -> [(attributes: [String: String], innerXML: String)] {
        var results: [(attributes: [String: String], innerXML: String)] = []
        let openPrefix = "<\(tagName)"
        let closeTag = "</\(tagName)>"
        var searchStart = xml.startIndex

        while let openRange = xml.range(of: openPrefix, range: searchStart..<xml.endIndex) {
            let afterTagName = openRange.upperBound
            if afterTagName < xml.endIndex {
                let nextChar = xml[afterTagName]
                guard nextChar == " " || nextChar == ">" || nextChar == "/" || nextChar == "\t" || nextChar == "\n" else {
                    searchStart = afterTagName
                    continue
                }
            }
            guard let tagEndRange = xml.range(of: ">", range: afterTagName..<xml.endIndex) else {
                break
            }
            let attrSegment = String(xml[afterTagName..<tagEndRange.lowerBound])
            let attrs = parseAttributes(from: attrSegment)

            if attrSegment.trimmingCharacters(in: .whitespaces).hasSuffix("/") {
                results.append((attributes: attrs, innerXML: ""))
                searchStart = tagEndRange.upperBound
                continue
            }

            guard let closeRange = xml.range(of: closeTag, range: tagEndRange.upperBound..<xml.endIndex) else {
                break
            }
            let inner = String(xml[tagEndRange.upperBound..<closeRange.lowerBound])
            results.append((attributes: attrs, innerXML: inner))
            searchStart = closeRange.upperBound
        }
        return results
    }

    static func parseAttributes(from rawAttributeSegment: String) -> [String: String] {
        var attributes: [String: String] = [:]
        var cursor = rawAttributeSegment.startIndex
        while cursor < rawAttributeSegment.endIndex {
            while cursor < rawAttributeSegment.endIndex && (rawAttributeSegment[cursor].isWhitespace || rawAttributeSegment[cursor] == "/") {
                cursor = rawAttributeSegment.index(after: cursor)
            }
            guard cursor < rawAttributeSegment.endIndex else { break }

            let nameStart = cursor
            while cursor < rawAttributeSegment.endIndex && rawAttributeSegment[cursor] != "=" && !rawAttributeSegment[cursor].isWhitespace {
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
            guard cursor < rawAttributeSegment.endIndex, rawAttributeSegment[cursor] == "\"" else {
                break
            }
            cursor = rawAttributeSegment.index(after: cursor)
            let valueStart = cursor
            while cursor < rawAttributeSegment.endIndex && rawAttributeSegment[cursor] != "\"" {
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

// MARK: - OOXML Container Codec (ECMA-376 DOCX, XLSX, PPTX)

/// ECMA-376 / ISO/IEC 29500 OOXML container encoder and decoder for `.docx`, `.xlsx`, and `.pptx`.
public enum OOXMLContainerCodec {

    // MARK: DOCX (WordprocessingML)

    public static func encodeDocument(_ doc: OfficeDocumentModel) throws -> Data {
        let contentTypesXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
          <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
          <Default Extension="xml" ContentType="application/xml"/>
          <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
          <Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
        </Types>
        """

        let relsXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
          <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
          <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
        </Relationships>
        """

        let coreXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/">
          <dc:title>\(XMLSanitizer.escape(doc.title))</dc:title>
          <cp:headerText>\(XMLSanitizer.escape(doc.headerText))</cp:headerText>
          <cp:footerText>\(XMLSanitizer.escape(doc.footerText))</cp:footerText>
          <cp:paperSize>\(XMLSanitizer.escape(doc.paperSize.rawValue))</cp:paperSize>
          <cp:trackChanges>\(doc.trackChangesEnabled ? "true" : "false")</cp:trackChanges>
        </cp:coreProperties>
        """

        var bodyItems: [String] = []
        for p in doc.paragraphs {
            let boldAttr = p.isBold ? "1" : "0"
            let italicAttr = p.isItalic ? "1" : "0"
            let underlineAttr = p.isUnderlined ? "1" : "0"
            let pXML = """
                <w:p style="\(XMLSanitizer.escape(p.style.rawValue))" align="\(XMLSanitizer.escape(p.alignment.rawValue))" font="\(XMLSanitizer.escape(p.fontName))" size="\(p.fontSizePt)" bold="\(boldAttr)" italic="\(italicAttr)" underline="\(underlineAttr)" lineSpacing="\(p.lineSpacingPercent)">
                  <w:r><w:t>\(XMLSanitizer.escape(p.text))</w:t></w:r>
                </w:p>
            """
            bodyItems.append(pXML)
        }

        for tbl in doc.tables {
            let rowStrings = tbl.rows.map { row -> String in
                let cells = row.map { cellText in
                    "<w:tc><w:p><w:r><w:t>\(XMLSanitizer.escape(cellText))</w:t></w:r></w:p></w:tc>"
                }.joined()
                return "<w:tr>\(cells)</w:tr>"
            }.joined(separator: "\n      ")

            let tblXML = """
                <w:tbl caption="\(XMLSanitizer.escape(tbl.caption))" headerRow="\(tbl.hasHeaderRow ? "1" : "0")">
                  \(rowStrings)
                </w:tbl>
            """
            bodyItems.append(tblXML)
        }

        let documentXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
        \(bodyItems.joined(separator: "\n"))
          </w:body>
        </w:document>
        """

        return try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "[Content_Types].xml", data: Data(contentTypesXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "_rels/.rels", data: Data(relsXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "docProps/core.xml", data: Data(coreXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "word/document.xml", data: Data(documentXML.utf8), compressionMethod: .deflate)
        ])
    }

    public static func decodeDocument(from data: Data) throws -> OfficeDocumentModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        var entryMap: [String: String] = [:]
        for entry in entries {
            entryMap[entry.path] = try XMLSanitizer.validateSafeXMLData(entry.data)
        }
        guard let documentXML = entryMap["word/document.xml"] else {
            throw EasyOfficeError.corruptedContainer(detail: "OOXML DOCX 필수 스트림 'word/document.xml'이 누락되었습니다.")
        }

        let coreXML = entryMap["docProps/core.xml"] ?? ""
        let title = XMLSanitizer.extractElements(named: "dc:title", in: coreXML).first.map { XMLSanitizer.unescape($0.innerXML) } ?? "제목 없음"
        let headerText = XMLSanitizer.extractElements(named: "cp:headerText", in: coreXML).first.map { XMLSanitizer.unescape($0.innerXML) } ?? ""
        let footerText = XMLSanitizer.extractElements(named: "cp:footerText", in: coreXML).first.map { XMLSanitizer.unescape($0.innerXML) } ?? ""
        let paperRaw = XMLSanitizer.extractElements(named: "cp:paperSize", in: coreXML).first.map { XMLSanitizer.unescape($0.innerXML) } ?? ""
        let paperSize = PaperSizePreset(rawValue: paperRaw) ?? .a4
        let trackChanges = XMLSanitizer.extractElements(named: "cp:trackChanges", in: coreXML).first?.innerXML == "true"

        // Extract top-level paragraphs (those with style attribute) and tables
        var paragraphs: [ParagraphBlock] = []
        for pElem in XMLSanitizer.extractElements(named: "w:p", in: documentXML) {
            guard let styleRaw = pElem.attributes["style"] else { continue }
            let style = ParagraphStyleRole(rawValue: styleRaw) ?? .body
            let alignment = TextAlignmentMode(rawValue: pElem.attributes["align"] ?? "") ?? .justified
            let fontName = pElem.attributes["font"] ?? "Pretendard"
            let fontSize = Double(pElem.attributes["size"] ?? "11.0") ?? 11.0
            let isBold = pElem.attributes["bold"] == "1"
            let isItalic = pElem.attributes["italic"] == "1"
            let isUnderlined = pElem.attributes["underline"] == "1"
            let lineSpacing = Double(pElem.attributes["lineSpacing"] ?? "160.0") ?? 160.0
            let textContent = XMLSanitizer.extractElements(named: "w:t", in: pElem.innerXML)
                .map { XMLSanitizer.unescape($0.innerXML) }
                .joined()

            paragraphs.append(ParagraphBlock(
                style: style,
                alignment: alignment,
                text: textContent,
                fontName: fontName,
                fontSizePt: fontSize,
                isBold: isBold,
                isItalic: isItalic,
                isUnderlined: isUnderlined,
                lineSpacingPercent: lineSpacing
            ))
        }

        var tables: [DocumentTableBlock] = []
        for tblElem in XMLSanitizer.extractElements(named: "w:tbl", in: documentXML) {
            let caption = tblElem.attributes["caption"] ?? ""
            let hasHeader = (tblElem.attributes["headerRow"] ?? "1") == "1"
            var rows: [[String]] = []
            for trElem in XMLSanitizer.extractElements(named: "w:tr", in: tblElem.innerXML) {
                let rowCells = XMLSanitizer.extractElements(named: "w:tc", in: trElem.innerXML).map { tcElem in
                    XMLSanitizer.extractElements(named: "w:t", in: tcElem.innerXML)
                        .map { XMLSanitizer.unescape($0.innerXML) }
                        .joined()
                }
                rows.append(rowCells)
            }
            tables.append(DocumentTableBlock(caption: caption, rows: rows, hasHeaderRow: hasHeader))
        }

        return OfficeDocumentModel(
            title: title,
            format: .docx,
            paperSize: paperSize,
            headerText: headerText,
            footerText: footerText,
            paragraphs: paragraphs,
            tables: tables,
            trackChangesEnabled: trackChanges
        )
    }

    // MARK: XLSX (SpreadsheetML)

    public static func encodeWorksheet(_ sheet: WorksheetModel) throws -> Data {
        let contentTypesXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
          <Default Extension="xml" ContentType="application/xml"/>
          <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
          <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
        </Types>
        """

        let workbookXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
          <sheets>
            <sheet name="\(XMLSanitizer.escape(sheet.name))" sheetId="1" rId="rId1"/>
          </sheets>
        </workbook>
        """

        let sortedKeys = sheet.rawCells.keys.sorted()
        var cellTags: [String] = []
        for a1 in sortedKeys {
            guard let raw = sheet.rawCells[a1] else { continue }
            if raw.hasPrefix("=") {
                let formulaBody = String(raw.dropFirst())
                cellTags.append("      <c r=\"\(XMLSanitizer.escape(a1))\" t=\"str\"><f>\(XMLSanitizer.escape(formulaBody))</f></c>")
            } else {
                cellTags.append("      <c r=\"\(XMLSanitizer.escape(a1))\" t=\"inlineStr\"><v>\(XMLSanitizer.escape(raw))</v></c>")
            }
        }

        let sheetXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
          <sheetData>
            <row r="1">
        \(cellTags.joined(separator: "\n"))
            </row>
          </sheetData>
        </worksheet>
        """

        return try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "[Content_Types].xml", data: Data(contentTypesXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "xl/workbook.xml", data: Data(workbookXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "xl/worksheets/sheet1.xml", data: Data(sheetXML.utf8), compressionMethod: .deflate)
        ])
    }

    public static func decodeWorksheet(from data: Data) throws -> WorksheetModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        var entryMap: [String: String] = [:]
        for entry in entries {
            entryMap[entry.path] = try XMLSanitizer.validateSafeXMLData(entry.data)
        }
        guard let sheetXML = entryMap["xl/worksheets/sheet1.xml"] else {
            throw EasyOfficeError.corruptedContainer(detail: "OOXML XLSX 필수 스트림 'xl/worksheets/sheet1.xml'이 누락되었습니다.")
        }

        let workbookXML = entryMap["xl/workbook.xml"] ?? ""
        let sheetName = XMLSanitizer.extractElements(named: "sheet", in: workbookXML).first?.attributes["name"] ?? "Sheet1"

        var model = WorksheetModel(name: sheetName)
        for cellElem in XMLSanitizer.extractElements(named: "c", in: sheetXML) {
            guard let addr = cellElem.attributes["r"] else { continue }
            if let fElem = XMLSanitizer.extractElements(named: "f", in: cellElem.innerXML).first {
                let formulaText = XMLSanitizer.unescape(fElem.innerXML)
                try model.setCell(addr, rawValue: "=\(formulaText)")
            } else if let vElem = XMLSanitizer.extractElements(named: "v", in: cellElem.innerXML).first {
                let valText = XMLSanitizer.unescape(vElem.innerXML)
                try model.setCell(addr, rawValue: valText)
            }
        }
        return model
    }

    // MARK: PPTX (PresentationML)

    public static func encodePresentation(_ deck: PresentationDeckModel) throws -> Data {
        var entries: [ZipContainerEntry] = []

        let contentTypesXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
          <Default Extension="xml" ContentType="application/xml"/>
          <Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
        </Types>
        """
        entries.append(ZipContainerEntry(path: "[Content_Types].xml", data: Data(contentTypesXML.utf8), compressionMethod: .deflate))

        var slideRefs: [String] = []
        for idx in deck.slides.indices {
            slideRefs.append("    <p:sldId id=\"\(256 + idx)\" target=\"ppt/slides/slide\(idx + 1).xml\"/>")
        }

        let presentationXML = """
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" title="\(XMLSanitizer.escape(deck.title))" aspectRatio="\(XMLSanitizer.escape(deck.aspectRatio.rawValue))">
          <p:sldIdLst>
        \(slideRefs.joined(separator: "\n"))
          </p:sldIdLst>
        </p:presentation>
        """
        entries.append(ZipContainerEntry(path: "ppt/presentation.xml", data: Data(presentationXML.utf8), compressionMethod: .deflate))

        for (index, slide) in deck.slides.enumerated() {
            let bulletsXML = slide.bulletPoints.map { "<p:bullet>\(XMLSanitizer.escape($0))</p:bullet>" }.joined(separator: "\n    ")
            let elementsXML = slide.elements.map { el in
                "<p:sp title=\"\(XMLSanitizer.escape(el.title))\" x=\"\(el.x)\" y=\"\(el.y)\" w=\"\(el.width)\" h=\"\(el.height)\">\(XMLSanitizer.escape(el.bodyText))</p:sp>"
            }.joined(separator: "\n    ")

            let slideXML = """
            <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
            <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" title="\(XMLSanitizer.escape(slide.title))" subtitle="\(XMLSanitizer.escape(slide.subtitle))" layout="\(XMLSanitizer.escape(slide.masterLayout.rawValue))" transition="\(XMLSanitizer.escape(slide.transition.rawValue))">
              <p:notes>\(XMLSanitizer.escape(slide.speakerNotes))</p:notes>
              <p:bullets>
                \(bulletsXML)
              </p:bullets>
              <p:spTree>
                \(elementsXML)
              </p:spTree>
            </p:sld>
            """
            entries.append(ZipContainerEntry(
                path: "ppt/slides/slide\(index + 1).xml",
                data: Data(slideXML.utf8),
                compressionMethod: .deflate
            ))
        }

        return try ZipContainerCodec.pack(entries: entries)
    }

    public static func decodePresentation(from data: Data) throws -> PresentationDeckModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        var entryMap: [String: String] = [:]
        for entry in entries {
            entryMap[entry.path] = try XMLSanitizer.validateSafeXMLData(entry.data)
        }
        guard let presentationXML = entryMap["ppt/presentation.xml"] else {
            throw EasyOfficeError.corruptedContainer(detail: "OOXML PPTX 필수 스트림 'ppt/presentation.xml'이 누락되었습니다.")
        }

        let presRoot = XMLSanitizer.extractElements(named: "p:presentation", in: presentationXML).first
        let title = presRoot?.attributes["title"] ?? "발표자료"
        let aspectRatio = SlideAspectRatio(rawValue: presRoot?.attributes["aspectRatio"] ?? "") ?? .widescreen16x9
        let slideRefs = XMLSanitizer.extractElements(named: "p:sldId", in: presentationXML)

        var slides: [SlideModel] = []
        for ref in slideRefs {
            guard let target = ref.attributes["target"], let slideXML = entryMap[target] else {
                throw EasyOfficeError.corruptedContainer(detail: "PPTX 슬라이드 파트 누락")
            }
            guard let sldElem = XMLSanitizer.extractElements(named: "p:sld", in: slideXML).first else {
                continue
            }
            let sTitle = sldElem.attributes["title"] ?? ""
            let sSubtitle = sldElem.attributes["subtitle"] ?? ""
            let layout = SlideMasterLayout(rawValue: sldElem.attributes["layout"] ?? "") ?? .titleAndContent
            let transition = SlideTransitionKind(rawValue: sldElem.attributes["transition"] ?? "") ?? .fade
            let notes = XMLSanitizer.extractElements(named: "p:notes", in: sldElem.innerXML).first.map { XMLSanitizer.unescape($0.innerXML) } ?? ""
            let bullets = XMLSanitizer.extractElements(named: "p:bullet", in: sldElem.innerXML).map { XMLSanitizer.unescape($0.innerXML) }
            let elements = XMLSanitizer.extractElements(named: "p:sp", in: sldElem.innerXML).map { sp -> SlideCanvasElement in
                SlideCanvasElement(
                    title: sp.attributes["title"] ?? "",
                    bodyText: XMLSanitizer.unescape(sp.innerXML),
                    x: Double(sp.attributes["x"] ?? "0") ?? 0,
                    y: Double(sp.attributes["y"] ?? "0") ?? 0,
                    width: Double(sp.attributes["w"] ?? "100") ?? 100,
                    height: Double(sp.attributes["h"] ?? "60") ?? 60
                )
            }
            slides.append(SlideModel(
                title: sTitle,
                subtitle: sSubtitle,
                bulletPoints: bullets,
                speakerNotes: notes,
                masterLayout: layout,
                transition: transition,
                elements: elements
            ))
        }

        return PresentationDeckModel(title: title, aspectRatio: aspectRatio, slides: slides)
    }
}

// MARK: - ODF Container Codec (ISO/IEC 26300 ODT, ODS, ODP)

/// ISO/IEC 26300 OpenDocument Format container encoder and decoder (`.odt`, `.ods`, `.odp`).
public enum ODFContainerCodec {
    public static let textMimeType = "application/vnd.oasis.opendocument.text"
    public static let spreadsheetMimeType = "application/vnd.oasis.opendocument.spreadsheet"
    public static let presentationMimeType = "application/vnd.oasis.opendocument.presentation"

    public static func encodeDocument(_ doc: OfficeDocumentModel) throws -> Data {
        let pElements = doc.paragraphs.map { p in
            "<text:p style=\"\(XMLSanitizer.escape(p.style.rawValue))\" align=\"\(XMLSanitizer.escape(p.alignment.rawValue))\" font=\"\(XMLSanitizer.escape(p.fontName))\" size=\"\(p.fontSizePt)\" bold=\"\(p.isBold ? "1" : "0")\" italic=\"\(p.isItalic ? "1" : "0")\" underline=\"\(p.isUnderlined ? "1" : "0")\" lineSpacing=\"\(p.lineSpacingPercent)\">\(XMLSanitizer.escape(p.text))</text:p>"
        }.joined(separator: "\n      ")

        let tblElements = doc.tables.map { tbl in
            let rowsXML = tbl.rows.map { row in
                let cells = row.map { "<table:table-cell><text:value>\(XMLSanitizer.escape($0))</text:value></table:table-cell>" }.joined()
                return "<table:table-row>\(cells)</table:table-row>"
            }.joined(separator: "\n        ")
            return "<table:table caption=\"\(XMLSanitizer.escape(tbl.caption))\" headerRow=\"\(tbl.hasHeaderRow ? "1" : "0")\">\n        \(rowsXML)\n      </table:table>"
        }.joined(separator: "\n      ")

        let contentXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" title="\(XMLSanitizer.escape(doc.title))" paperSize="\(XMLSanitizer.escape(doc.paperSize.rawValue))" headerText="\(XMLSanitizer.escape(doc.headerText))" footerText="\(XMLSanitizer.escape(doc.footerText))" trackChanges="\(doc.trackChangesEnabled ? "1" : "0")">
          <office:body>
            <office:text>
              \(pElements)
              \(tblElements)
            </office:text>
          </office:body>
        </office:document-content>
        """

        return try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "mimetype", data: Data(textMimeType.utf8), compressionMethod: .store),
            ZipContainerEntry(path: "content.xml", data: Data(contentXML.utf8), compressionMethod: .deflate)
        ])
    }

    public static func decodeDocument(from data: Data) throws -> OfficeDocumentModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        guard let mimeEntry = entries.first(where: { $0.path == "mimetype" }),
              String(data: mimeEntry.data, encoding: .utf8) == textMimeType else {
            throw EasyOfficeError.corruptedContainer(detail: "ODT 컨테이너의 mimetype 엔트리가 유효하지 않습니다.")
        }
        guard let contentEntry = entries.first(where: { $0.path == "content.xml" }) else {
            throw EasyOfficeError.corruptedContainer(detail: "ODT 컨테이너에 'content.xml'이 누락되었습니다.")
        }
        let xml = try XMLSanitizer.validateSafeXMLData(contentEntry.data)
        guard let root = XMLSanitizer.extractElements(named: "office:document-content", in: xml).first else {
            throw EasyOfficeError.corruptedContainer(detail: "ODT 'office:document-content' 루트를 찾을 수 없습니다.")
        }

        let paragraphs = XMLSanitizer.extractElements(named: "text:p", in: root.innerXML).map { elem in
            ParagraphBlock(
                style: ParagraphStyleRole(rawValue: elem.attributes["style"] ?? "") ?? .body,
                alignment: TextAlignmentMode(rawValue: elem.attributes["align"] ?? "") ?? .justified,
                text: XMLSanitizer.unescape(elem.innerXML),
                fontName: elem.attributes["font"] ?? "Pretendard",
                fontSizePt: Double(elem.attributes["size"] ?? "11.0") ?? 11.0,
                isBold: elem.attributes["bold"] == "1",
                isItalic: elem.attributes["italic"] == "1",
                isUnderlined: elem.attributes["underline"] == "1",
                lineSpacingPercent: Double(elem.attributes["lineSpacing"] ?? "160.0") ?? 160.0
            )
        }

        let tables = XMLSanitizer.extractElements(named: "table:table", in: root.innerXML).map { tblElem -> DocumentTableBlock in
            let rows = XMLSanitizer.extractElements(named: "table:table-row", in: tblElem.innerXML).map { rowElem in
                XMLSanitizer.extractElements(named: "text:value", in: rowElem.innerXML).map { XMLSanitizer.unescape($0.innerXML) }
            }
            return DocumentTableBlock(
                caption: tblElem.attributes["caption"] ?? "",
                rows: rows,
                hasHeaderRow: (tblElem.attributes["headerRow"] ?? "1") == "1"
            )
        }

        return OfficeDocumentModel(
            title: root.attributes["title"] ?? "제목 없음",
            format: .odt,
            paperSize: PaperSizePreset(rawValue: root.attributes["paperSize"] ?? "") ?? .a4,
            headerText: root.attributes["headerText"] ?? "",
            footerText: root.attributes["footerText"] ?? "",
            paragraphs: paragraphs,
            tables: tables,
            trackChangesEnabled: root.attributes["trackChanges"] == "1"
        )
    }

    public static func encodeWorksheet(_ sheet: WorksheetModel) throws -> Data {
        let cellsXML = sheet.rawCells.keys.sorted().compactMap { key -> String? in
            guard let raw = sheet.rawCells[key] else { return nil }
            return "<table:table-cell address=\"\(XMLSanitizer.escape(key))\" raw=\"\(XMLSanitizer.escape(raw))\"/>"
        }.joined(separator: "\n        ")

        let contentXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0">
          <office:body>
            <office:spreadsheet>
              <table:table table:name="\(XMLSanitizer.escape(sheet.name))">
                \(cellsXML)
              </table:table>
            </office:spreadsheet>
          </office:body>
        </office:document-content>
        """

        return try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "mimetype", data: Data(spreadsheetMimeType.utf8), compressionMethod: .store),
            ZipContainerEntry(path: "content.xml", data: Data(contentXML.utf8), compressionMethod: .deflate)
        ])
    }

    public static func decodeWorksheet(from data: Data) throws -> WorksheetModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        guard let mimeEntry = entries.first(where: { $0.path == "mimetype" }),
              String(data: mimeEntry.data, encoding: .utf8) == spreadsheetMimeType else {
            throw EasyOfficeError.corruptedContainer(detail: "ODS 컨테이너의 mimetype 엔트리가 유효하지 않습니다.")
        }
        guard let contentEntry = entries.first(where: { $0.path == "content.xml" }) else {
            throw EasyOfficeError.corruptedContainer(detail: "ODS 컨테이너에 'content.xml'이 누락되었습니다.")
        }
        let xml = try XMLSanitizer.validateSafeXMLData(contentEntry.data)
        let tableElem = XMLSanitizer.extractElements(named: "table:table", in: xml).first
        var sheet = WorksheetModel(name: tableElem?.attributes["table:name"] ?? "Sheet1")
        for cellElem in XMLSanitizer.extractElements(named: "table:table-cell", in: xml) {
            if let addr = cellElem.attributes["address"], let raw = cellElem.attributes["raw"] {
                try sheet.setCell(addr, rawValue: raw)
            }
        }
        return sheet
    }

    public static func encodePresentation(_ deck: PresentationDeckModel) throws -> Data {
        let pagesXML = deck.slides.map { slide -> String in
            let bullets = slide.bulletPoints.map { "<text:list-item>\(XMLSanitizer.escape($0))</text:list-item>" }.joined()
            let shapes = slide.elements.map { el in
                "<draw:frame title=\"\(XMLSanitizer.escape(el.title))\" x=\"\(el.x)\" y=\"\(el.y)\" w=\"\(el.width)\" h=\"\(el.height)\">\(XMLSanitizer.escape(el.bodyText))</draw:frame>"
            }.joined()
            return "<draw:page title=\"\(XMLSanitizer.escape(slide.title))\" subtitle=\"\(XMLSanitizer.escape(slide.subtitle))\" notes=\"\(XMLSanitizer.escape(slide.speakerNotes))\" layout=\"\(XMLSanitizer.escape(slide.masterLayout.rawValue))\" transition=\"\(XMLSanitizer.escape(slide.transition.rawValue))\">\(bullets)\(shapes)</draw:page>"
        }.joined(separator: "\n      ")

        let contentXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" title="\(XMLSanitizer.escape(deck.title))" aspectRatio="\(XMLSanitizer.escape(deck.aspectRatio.rawValue))">
          <office:body>
            <office:presentation>
              \(pagesXML)
            </office:presentation>
          </office:body>
        </office:document-content>
        """

        return try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "mimetype", data: Data(presentationMimeType.utf8), compressionMethod: .store),
            ZipContainerEntry(path: "content.xml", data: Data(contentXML.utf8), compressionMethod: .deflate)
        ])
    }

    public static func decodePresentation(from data: Data) throws -> PresentationDeckModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        guard let mimeEntry = entries.first(where: { $0.path == "mimetype" }),
              String(data: mimeEntry.data, encoding: .utf8) == presentationMimeType else {
            throw EasyOfficeError.corruptedContainer(detail: "ODP 컨테이너의 mimetype 엔트리가 유효하지 않습니다.")
        }
        guard let contentEntry = entries.first(where: { $0.path == "content.xml" }) else {
            throw EasyOfficeError.corruptedContainer(detail: "ODP 컨테이너에 'content.xml'이 누락되었습니다.")
        }
        let xml = try XMLSanitizer.validateSafeXMLData(contentEntry.data)
        let root = XMLSanitizer.extractElements(named: "office:document-content", in: xml).first
        let slides = XMLSanitizer.extractElements(named: "draw:page", in: xml).map { page -> SlideModel in
            let bullets = XMLSanitizer.extractElements(named: "text:list-item", in: page.innerXML).map { XMLSanitizer.unescape($0.innerXML) }
            let elements = XMLSanitizer.extractElements(named: "draw:frame", in: page.innerXML).map { frame in
                SlideCanvasElement(
                    title: frame.attributes["title"] ?? "",
                    bodyText: XMLSanitizer.unescape(frame.innerXML),
                    x: Double(frame.attributes["x"] ?? "0") ?? 0,
                    y: Double(frame.attributes["y"] ?? "0") ?? 0,
                    width: Double(frame.attributes["w"] ?? "100") ?? 100,
                    height: Double(frame.attributes["h"] ?? "60") ?? 60
                )
            }
            return SlideModel(
                title: page.attributes["title"] ?? "",
                subtitle: page.attributes["subtitle"] ?? "",
                bulletPoints: bullets,
                speakerNotes: page.attributes["notes"] ?? "",
                masterLayout: SlideMasterLayout(rawValue: page.attributes["layout"] ?? "") ?? .titleAndContent,
                transition: SlideTransitionKind(rawValue: page.attributes["transition"] ?? "") ?? .fade,
                elements: elements
            )
        }
        return PresentationDeckModel(
            title: root?.attributes["title"] ?? "발표자료",
            aspectRatio: SlideAspectRatio(rawValue: root?.attributes["aspectRatio"] ?? "") ?? .widescreen16x9,
            slides: slides
        )
    }
}

// MARK: - HWPX Container Codec (KS X 6101 OWPML)

/// KS X 6101 OWPML (`HWPX`) ZIP-XML container encoder and decoder.
public enum HWPXContainerCodec {
    public static let hwpxMimeType = "application/hwp+zip"

    public static func encodeDocument(_ doc: OfficeDocumentModel) throws -> Data {
        let contentHPF = """
        <?xml version="1.0" encoding="UTF-8"?>
        <opf:package xmlns:opf="http://www.idpf.org/2007/opf/" version="1.0">
          <opf:metadata>
            <opf:title>\(XMLSanitizer.escape(doc.title))</opf:title>
          </opf:metadata>
          <opf:manifest>
            <opf:item id="header" href="Contents/header.xml" media-type="application/xml"/>
            <opf:item id="section0" href="Contents/section0.xml" media-type="application/xml"/>
          </opf:manifest>
        </opf:package>
        """

        let headerXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <hh:head xmlns:hh="http://www.owpml.org/owpml/2011/head" paperSize="\(XMLSanitizer.escape(doc.paperSize.rawValue))" headerText="\(XMLSanitizer.escape(doc.headerText))" footerText="\(XMLSanitizer.escape(doc.footerText))" trackChanges="\(doc.trackChangesEnabled ? "1" : "0")"/>
        """

        let paragraphsXML = doc.paragraphs.map { p in
            """
              <hp:p style="\(XMLSanitizer.escape(p.style.rawValue))" align="\(XMLSanitizer.escape(p.alignment.rawValue))" font="\(XMLSanitizer.escape(p.fontName))" size="\(p.fontSizePt)" bold="\(p.isBold ? "1" : "0")" italic="\(p.isItalic ? "1" : "0")" underline="\(p.isUnderlined ? "1" : "0")" lineSpacing="\(p.lineSpacingPercent)">
                <hp:run><hp:t>\(XMLSanitizer.escape(p.text))</hp:t></hp:run>
              </hp:p>
            """
        }.joined(separator: "\n")

        let tablesXML = doc.tables.map { tbl in
            let rows = tbl.rows.map { row in
                let cells = row.map { "<hp:tc><hp:cellText>\(XMLSanitizer.escape($0))</hp:cellText></hp:tc>" }.joined()
                return "<hp:tr>\(cells)</hp:tr>"
            }.joined(separator: "\n    ")
            return """
              <hp:tbl caption="\(XMLSanitizer.escape(tbl.caption))" headerRow="\(tbl.hasHeaderRow ? "1" : "0")">
                \(rows)
              </hp:tbl>
            """
        }.joined(separator: "\n")

        let sectionXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <hs:sec xmlns:hs="http://www.owpml.org/owpml/2011/section" xmlns:hp="http://www.owpml.org/owpml/2011/paragraph">
        \(paragraphsXML)
        \(tablesXML)
        </hs:sec>
        """

        return try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "mimetype", data: Data(hwpxMimeType.utf8), compressionMethod: .store),
            ZipContainerEntry(path: "Contents/content.hpf", data: Data(contentHPF.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "Contents/header.xml", data: Data(headerXML.utf8), compressionMethod: .deflate),
            ZipContainerEntry(path: "Contents/section0.xml", data: Data(sectionXML.utf8), compressionMethod: .deflate)
        ])
    }

    public static func decodeDocument(from data: Data) throws -> OfficeDocumentModel {
        let entries = try ZipContainerCodec.unpack(from: data)
        var entryMap: [String: String] = [:]
        for entry in entries {
            entryMap[entry.path] = try XMLSanitizer.validateSafeXMLData(entry.data)
        }

        guard entryMap["mimetype"] == hwpxMimeType else {
            throw EasyOfficeError.corruptedContainer(detail: "HWPX(KS X 6101) 컨테이너의 mimetype이 'application/hwp+zip'이 아닙니다.")
        }
        guard let hpfXML = entryMap["Contents/content.hpf"],
              let headerXML = entryMap["Contents/header.xml"],
              let sectionXML = entryMap["Contents/section0.xml"] else {
            throw EasyOfficeError.corruptedContainer(detail: "HWPX 필수 스트림(content.hpf, header.xml, section0.xml)이 누락되었습니다.")
        }

        let title = XMLSanitizer.extractElements(named: "opf:title", in: hpfXML).first.map { XMLSanitizer.unescape($0.innerXML) } ?? "제목 없음"
        let headAttrs = XMLSanitizer.extractElements(named: "hh:head", in: headerXML).first?.attributes ?? [:]
        let paperSize = PaperSizePreset(rawValue: headAttrs["paperSize"] ?? "") ?? .a4
        let headerText = headAttrs["headerText"] ?? ""
        let footerText = headAttrs["footerText"] ?? ""
        let trackChanges = headAttrs["trackChanges"] == "1"

        let paragraphs = XMLSanitizer.extractElements(named: "hp:p", in: sectionXML).map { pElem -> ParagraphBlock in
            let text = XMLSanitizer.extractElements(named: "hp:t", in: pElem.innerXML)
                .map { XMLSanitizer.unescape($0.innerXML) }
                .joined()
            return ParagraphBlock(
                style: ParagraphStyleRole(rawValue: pElem.attributes["style"] ?? "") ?? .body,
                alignment: TextAlignmentMode(rawValue: pElem.attributes["align"] ?? "") ?? .justified,
                text: text,
                fontName: pElem.attributes["font"] ?? "Pretendard",
                fontSizePt: Double(pElem.attributes["size"] ?? "11.0") ?? 11.0,
                isBold: pElem.attributes["bold"] == "1",
                isItalic: pElem.attributes["italic"] == "1",
                isUnderlined: pElem.attributes["underline"] == "1",
                lineSpacingPercent: Double(pElem.attributes["lineSpacing"] ?? "160.0") ?? 160.0
            )
        }

        let tables = XMLSanitizer.extractElements(named: "hp:tbl", in: sectionXML).map { tblElem -> DocumentTableBlock in
            let rows = XMLSanitizer.extractElements(named: "hp:tr", in: tblElem.innerXML).map { trElem in
                XMLSanitizer.extractElements(named: "hp:cellText", in: trElem.innerXML).map { XMLSanitizer.unescape($0.innerXML) }
            }
            return DocumentTableBlock(
                caption: tblElem.attributes["caption"] ?? "",
                rows: rows,
                hasHeaderRow: (tblElem.attributes["headerRow"] ?? "1") == "1"
            )
        }

        return OfficeDocumentModel(
            title: title,
            format: .hwpx,
            paperSize: paperSize,
            headerText: headerText,
            footerText: footerText,
            paragraphs: paragraphs,
            tables: tables,
            trackChangesEnabled: trackChanges
        )
    }
}

// MARK: - OLE2 Compound File Binary Format (CFBF) & HWP 5.0 Binary Codec

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
        // Entry 0 is "Root Entry", followed by each stream entry
        let totalDirEntries = 1 + streams.count
        let dirSectorCount = max(1, (totalDirEntries + entriesPerSector - 1) / entriesPerSector)

        // Calculate stream sector allocations starting after directory sectors
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

        // Build FAT array
        var fat: [UInt32] = Array(repeating: freeSector, count: fatSectorCount * fatEntriesPerSector)

        // Chain directory sectors (0 ..< dirSectorCount)
        for sec in 0..<dirSectorCount {
            fat[sec] = (sec + 1 < dirSectorCount) ? UInt32(sec + 1) : endOfChain
        }

        // Chain each stream's sectors
        for idx in streams.indices {
            let count = streamSectorCounts[idx]
            guard count > 0 else { continue }
            let start = Int(streamStartSectors[idx])
            for offset in 0..<count {
                let current = start + offset
                fat[current] = (offset + 1 < count) ? UInt32(current + 1) : endOfChain
            }
        }

        // Mark FAT sectors in FAT
        for fIdx in 0..<fatSectorCount {
            fat[firstFatSectorIndex + fIdx] = fatSectorMarker
        }

        // Assemble 512-byte OLE2 Header
        var container = Data(repeating: 0, count: sectorSize)
        for (i, byte) in FormatDetector.ole2Magic.enumerated() {
            container[i] = byte
        }
        try BitPacking.writeUInt16LE(0x003E, into: &container, at: 24) // Minor version
        try BitPacking.writeUInt16LE(0x0003, into: &container, at: 26) // Major version 3 (512-byte sectors)
        try BitPacking.writeUInt16LE(0xFFFE, into: &container, at: 28) // Little-endian byte order
        try BitPacking.writeUInt16LE(0x0009, into: &container, at: 30) // Sector shift (1 << 9 = 512)
        try BitPacking.writeUInt16LE(0x0006, into: &container, at: 32) // Mini sector shift (1 << 6 = 64)
        try BitPacking.writeUInt32LE(UInt32(fatSectorCount), into: &container, at: 44) // Number of FAT sectors
        try BitPacking.writeUInt32LE(0, into: &container, at: 48) // First Directory sector = Sector 0
        try BitPacking.writeUInt32LE(0, into: &container, at: 56) // Mini stream cutoff = 0 (all streams in standard sectors)
        try BitPacking.writeUInt32LE(endOfChain, into: &container, at: 60) // First Mini FAT sector
        try BitPacking.writeUInt32LE(0, into: &container, at: 64) // Number of Mini FAT sectors
        try BitPacking.writeUInt32LE(endOfChain, into: &container, at: 68) // First DIFAT sector
        try BitPacking.writeUInt32LE(0, into: &container, at: 72) // Number of DIFAT sectors

        // Fill DIFAT array (109 entries at offset 76..512)
        for slot in 0..<109 {
            let value = (slot < fatSectorCount) ? UInt32(firstFatSectorIndex + slot) : freeSector
            try BitPacking.writeUInt32LE(value, into: &container, at: 76 + slot * 4)
        }

        // Append Directory Sectors
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

        // Append Stream Sectors (zero-padded to 512-byte boundary)
        for stream in streams where !stream.data.isEmpty {
            container.append(stream.data)
            let remainder = stream.data.count % sectorSize
            if remainder != 0 {
                container.append(Data(repeating: 0, count: sectorSize - remainder))
            }
        }

        // Append FAT Sectors
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

        // Reconstruct FAT table from DIFAT entries in header
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

        let directoryData = try readSectorChain(
            startSector: firstDirSector,
            fat: fat,
            data: data,
            totalSectors: totalSectorsInFile
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
                let rawChain = try readSectorChain(
                    startSector: startSector,
                    fat: fat,
                    data: data,
                    totalSectors: totalSectorsInFile
                )
                guard rawChain.count >= streamSize else {
                    throw EasyOfficeError.corruptedContainer(detail: "OLE2 스트림 '\(name)'의 섹터 체인 길이가 부족합니다.")
                }
                streams.append(OLE2StreamEntry(name: name, data: Data(rawChain.prefix(streamSize))))
            }
        }

        return streams
    }

    private static func readSectorChain(
        startSector: UInt32,
        fat: [UInt32],
        data: Data,
        totalSectors: Int
    ) throws -> Data {
        var output = Data()
        var current = startSector
        var visited = Set<UInt32>()

        while current != endOfChain {
            guard !visited.contains(current) else {
                throw EasyOfficeError.corruptedContainer(detail: "OLE2 FAT 체인에서 순환 참조(Cycle)가 감지되었습니다.")
            }
            guard current < UInt32(totalSectors), Int(current) < fat.count else {
                throw EasyOfficeError.corruptedContainer(detail: "OLE2 FAT 체인이 유효하지 않은 섹터(\(current))를 참조합니다.")
            }
            visited.insert(current)

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
        // NUL terminator at utf16Units.count * 2 is already 0
        let nameByteLen = UInt16((utf16Units.count + 1) * 2)
        try BitPacking.writeUInt16LE(nameByteLen, into: &directoryBytes, at: base + 64)
        directoryBytes[directoryBytes.startIndex + base + 66] = objectType
        directoryBytes[directoryBytes.startIndex + base + 67] = 1 // Black node in red-black tree
        try BitPacking.writeUInt32LE(freeSector, into: &directoryBytes, at: base + 68) // Left sibling
        try BitPacking.writeUInt32LE(rightSiblingID, into: &directoryBytes, at: base + 72) // Right sibling
        try BitPacking.writeUInt32LE(childID, into: &directoryBytes, at: base + 76) // Child ID
        try BitPacking.writeUInt32LE(startSector, into: &directoryBytes, at: base + 116)
        try BitPacking.writeUInt64LE(streamSize, into: &directoryBytes, at: base + 120)
    }
}

/// HWP 5.0 binary document codec built on top of `OLE2ContainerCodec` and `HWP5RecordHeader`.
public enum HWP5BinaryCodec {
    public static let fileHeaderByteLength = 256
    public static let signatureString = "HWP Document File"
    public static let defaultVersion: UInt32 = 0x0500_0300 // 5.0.3.0
    public static let flagCompressed: UInt32 = 0x0000_0001
    public static let flagEncrypted: UInt32 = 0x0000_0002
    public static let flagDistribution: UInt32 = 0x0000_0004

    public static func encodeDocument(_ doc: OfficeDocumentModel) throws -> Data {
        // 1. Build 256-byte FileHeader stream
        var fileHeader = Data(repeating: 0, count: fileHeaderByteLength)
        let sigBytes = Array(signatureString.utf8)
        for (idx, b) in sigBytes.enumerated() {
            fileHeader[idx] = b
        }
        try BitPacking.writeUInt32LE(defaultVersion, into: &fileHeader, at: 32)
        try BitPacking.writeUInt32LE(0, into: &fileHeader, at: 36) // Uncompressed, unencrypted properties bitfield

        // 2. Build DocInfo stream with HWPTAG_DOCUMENT_PROPERTIES (tag 16)
        var docPropsPayload = Data()
        BitPacking.appendLengthPrefixedUTF16LE(doc.title, to: &docPropsPayload)
        BitPacking.appendLengthPrefixedUTF16LE(doc.paperSize.rawValue, to: &docPropsPayload)
        BitPacking.appendLengthPrefixedUTF16LE(doc.headerText, to: &docPropsPayload)
        BitPacking.appendLengthPrefixedUTF16LE(doc.footerText, to: &docPropsPayload)
        docPropsPayload.append(doc.trackChangesEnabled ? 1 : 0)

        var docInfoStream = try HWP5RecordHeader.encode(
            tagID: HWP5RecordHeader.tagDocumentProperties,
            level: 0,
            payloadSize: UInt32(docPropsPayload.count)
        )
        docInfoStream.append(docPropsPayload)

        // 3. Build BodyText/Section0 stream with HWPTAG_PARA_HEADER (66), HWPTAG_PARA_TEXT (67), HWPTAG_TABLE (77)
        var section0Stream = Data()

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
            section0Stream.append(headerRec)
            section0Stream.append(paraHeaderPayload)

            // HWPTAG_PARA_TEXT (tag 67): raw UTF-16LE characters
            var textPayload = Data()
            textPayload.reserveCapacity(paragraph.text.utf16.count * 2)
            for unit in paragraph.text.utf16 {
                BitPacking.appendUInt16LE(unit, to: &textPayload)
            }
            let textRec = try HWP5RecordHeader.encode(
                tagID: HWP5RecordHeader.tagParaText,
                level: 1,
                payloadSize: UInt32(textPayload.count)
            )
            section0Stream.append(textRec)
            section0Stream.append(textPayload)
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
            section0Stream.append(tableRec)
            section0Stream.append(tablePayload)
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
        if (flags & flagEncrypted) != 0 {
            throw EasyOfficeError.encryptedDocumentUnsupported(format: .hwp)
        }

        // Parse DocInfo stream
        var title = "제목 없음"
        var paperSize: PaperSizePreset = .a4
        var headerText = ""
        var footerText = ""
        var trackChanges = false

        if let docInfo = streamMap["DocInfo"], !docInfo.isEmpty {
            let (recHeader, headerLen) = try HWP5RecordHeader.decode(from: docInfo, at: 0)
            let recordEnd = headerLen + Int(recHeader.payloadSize)
            guard recHeader.tagID == HWP5RecordHeader.tagDocumentProperties,
                  recordEnd <= docInfo.count else {
                throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 DocInfo 레코드가 손상되었습니다.")
            }
            var cursor = headerLen
            title = try BitPacking.readLengthPrefixedUTF16LE(from: docInfo, offset: &cursor)
            let paperRaw = try BitPacking.readLengthPrefixedUTF16LE(from: docInfo, offset: &cursor)
            paperSize = PaperSizePreset(rawValue: paperRaw) ?? .a4
            headerText = try BitPacking.readLengthPrefixedUTF16LE(from: docInfo, offset: &cursor)
            footerText = try BitPacking.readLengthPrefixedUTF16LE(from: docInfo, offset: &cursor)
            if cursor < recordEnd {
                trackChanges = docInfo[docInfo.startIndex + cursor] != 0
            }
        }

        guard let section0 = streamMap["BodyText/Section0"] else {
            throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 'BodyText/Section0' 스트림이 누락되었습니다.")
        }

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
                let styleRaw = try BitPacking.readLengthPrefixedUTF16LE(from: section0, offset: &pCursor)
                let alignRaw = try BitPacking.readLengthPrefixedUTF16LE(from: section0, offset: &pCursor)
                let fontName = try BitPacking.readLengthPrefixedUTF16LE(from: section0, offset: &pCursor)
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
                let caption = try BitPacking.readLengthPrefixedUTF16LE(from: section0, offset: &tCursor)
                guard tCursor + 3 <= payloadEnd else {
                    throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 레코드 헤더 길이가 부족합니다.")
                }
                let hasHeader = section0[section0.startIndex + tCursor] != 0
                tCursor += 1
                let rowCount = Int(try BitPacking.readUInt16LE(from: section0, at: tCursor))
                tCursor += 2
                var rows: [[String]] = []
                rows.reserveCapacity(rowCount)
                for _ in 0..<rowCount {
                    guard tCursor + 2 <= payloadEnd else {
                        throw EasyOfficeError.corruptedContainer(detail: "HWP 5.0 표 행 헤더 길이가 부족합니다.")
                    }
                    let colCount = Int(try BitPacking.readUInt16LE(from: section0, at: tCursor))
                    tCursor += 2
                    var rowCells: [String] = []
                    rowCells.reserveCapacity(colCount)
                    for _ in 0..<colCount {
                        let cell = try BitPacking.readLengthPrefixedUTF16LE(from: section0, offset: &tCursor)
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
