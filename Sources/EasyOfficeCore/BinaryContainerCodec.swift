import Compression
import Foundation

// MARK: - Low-Level Bit Packing & IEEE 802.3 Slicing-by-8 CRC-32

/// Deterministic little-endian binary reader and writer with Fail-Closed bounds verification and unaligned hardware word loads.
public enum BitPacking {

  @inlinable
  public static func readUInt16LE(from data: Data, at offset: Int) throws -> UInt16 {
    guard offset >= 0, offset <= data.count - 2 else {
      throw EasyOfficeError.corruptedContainer(
        detail: "UInt16 읽기 범위 초과 (offset: \(offset), size: \(data.count))"
      )
    }
    return data.withUnsafeBytes { rawBuffer in
      rawBuffer.loadUnaligned(fromByteOffset: offset, as: UInt16.self).littleEndian
    }
  }

  @inlinable
  public static func readUInt32LE(from data: Data, at offset: Int) throws -> UInt32 {
    guard offset >= 0, offset <= data.count - 4 else {
      throw EasyOfficeError.corruptedContainer(
        detail: "UInt32 읽기 범위 초과 (offset: \(offset), size: \(data.count))"
      )
    }
    return data.withUnsafeBytes { rawBuffer in
      rawBuffer.loadUnaligned(fromByteOffset: offset, as: UInt32.self).littleEndian
    }
  }

  @inlinable
  public static func readUInt64LE(from data: Data, at offset: Int) throws -> UInt64 {
    guard offset >= 0, offset <= data.count - 8 else {
      throw EasyOfficeError.corruptedContainer(
        detail: "UInt64 읽기 범위 초과 (offset: \(offset), size: \(data.count))"
      )
    }
    return data.withUnsafeBytes { rawBuffer in
      rawBuffer.loadUnaligned(fromByteOffset: offset, as: UInt64.self).littleEndian
    }
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
    guard offset >= 0, offset <= data.count - 2 else {
      throw EasyOfficeError.corruptedContainer(
        detail: "UInt16 쓰기 범위 초과 (offset: \(offset), size: \(data.count))"
      )
    }
    data.withUnsafeMutableBytes { rawBuffer in
      rawBuffer.storeBytes(of: value.littleEndian, toByteOffset: offset, as: UInt16.self)
    }
  }

  @inlinable
  public static func writeUInt32LE(_ value: UInt32, into data: inout Data, at offset: Int) throws {
    guard offset >= 0, offset <= data.count - 4 else {
      throw EasyOfficeError.corruptedContainer(
        detail: "UInt32 쓰기 범위 초과 (offset: \(offset), size: \(data.count))"
      )
    }
    data.withUnsafeMutableBytes { rawBuffer in
      rawBuffer.storeBytes(of: value.littleEndian, toByteOffset: offset, as: UInt32.self)
    }
  }

  @inlinable
  public static func writeUInt64LE(_ value: UInt64, into data: inout Data, at offset: Int) throws {
    guard offset >= 0, offset <= data.count - 8 else {
      throw EasyOfficeError.corruptedContainer(
        detail: "UInt64 쓰기 범위 초과 (offset: \(offset), size: \(data.count))"
      )
    }
    data.withUnsafeMutableBytes { rawBuffer in
      rawBuffer.storeBytes(of: value.littleEndian, toByteOffset: offset, as: UInt64.self)
    }
  }

  @inline(__always)
  public static func expandASCII4ToUTF16LE(_ fourBytes: UInt32) -> UInt64 {
    var x = UInt64(fourBytes)
    x = (x | (x << 16)) & 0x0000_FFFF_0000_FFFF
    x = (x | (x << 8)) & 0x00FF_00FF_00FF_00FF
    return x
  }

  /// Encodes a Swift String as contiguous little-endian UTF-16 code units.
  @inlinable
  public static func encodeUTF16LE(_ text: String) -> Data {
    if text.isEmpty { return Data() }
    var mutableText = text
    let asciiFastResult: Data? = mutableText.withUTF8 { utf8Buf -> Data? in
      let count = utf8Buf.count
      guard count > 0, let base = utf8Buf.baseAddress else { return Data() }
      let rawIn = UnsafeRawPointer(base)
      var idx = 0
      while idx + 8 <= count {
        let word = rawIn.loadUnaligned(fromByteOffset: idx, as: UInt64.self)
        if (word & 0x8080_8080_8080_8080) != 0 {
          return nil
        }
        idx += 8
      }
      while idx < count {
        if (base[idx] & 0x80) != 0 {
          return nil
        }
        idx += 1
      }
      var out = Data(count: count * 2)
      out.withUnsafeMutableBytes { outBuf in
        var srcIdx = 0
        var dstByteOffset = 0
        while srcIdx + 8 <= count {
          let word = rawIn.loadUnaligned(fromByteOffset: srcIdx, as: UInt64.self).littleEndian
          let lo4 = UInt32(truncatingIfNeeded: word)
          let hi4 = UInt32(truncatingIfNeeded: word >> 32)
          let expLo = expandASCII4ToUTF16LE(lo4).littleEndian
          let expHi = expandASCII4ToUTF16LE(hi4).littleEndian
          outBuf.storeBytes(of: expLo, toByteOffset: dstByteOffset, as: UInt64.self)
          outBuf.storeBytes(of: expHi, toByteOffset: dstByteOffset + 8, as: UInt64.self)
          srcIdx += 8
          dstByteOffset += 16
        }
        while srcIdx < count {
          let unit = UInt16(base[srcIdx]).littleEndian
          outBuf.storeBytes(of: unit, toByteOffset: dstByteOffset, as: UInt16.self)
          srcIdx += 1
          dstByteOffset += 2
        }
      }
      return out
    }
    if let asciiFastResult {
      return asciiFastResult
    }
    let utf16 = text.utf16
    let unitCount = utf16.count
    guard unitCount > 0 else { return Data() }
    var out = Data(count: unitCount * 2)
    out.withUnsafeMutableBytes { rawBuf in
      var byteOffset = 0
      for unit in utf16 {
        rawBuf.storeBytes(of: unit.littleEndian, toByteOffset: byteOffset, as: UInt16.self)
        byteOffset += 2
      }
    }
    return out
  }

  /// Encodes a Swift String as UTF-16LE bytes prefixed with a 32-bit byte length.
  public static func appendLengthPrefixedUTF16LE(_ text: String, to data: inout Data) {
    let lengthOffset = data.count
    appendUInt32LE(0, to: &data)
    let utf16Bytes = encodeUTF16LE(text)
    try? writeUInt32LE(UInt32(utf16Bytes.count), into: &data, at: lengthOffset)
    data.append(utf16Bytes)
  }

  /// Decodes a length-prefixed UTF-16LE String from `data` within `limit` and advances `offset`.
  public static func readLengthPrefixedUTF16LE(
    from data: Data, offset: inout Int, limit: Int? = nil
  ) throws -> String {
    let upperBound = min(limit ?? data.count, data.count)
    guard offset >= 0, offset <= upperBound - 4 else {
      throw EasyOfficeError.corruptedContainer(
        detail: "UTF-16LE 문자열 길이 필드 범위 초과 (offset: \(offset), limit: \(upperBound))")
    }
    let byteLength = Int(try readUInt32LE(from: data, at: offset))
    offset += 4
    guard byteLength >= 0, byteLength % 2 == 0, byteLength <= upperBound - offset else {
      throw EasyOfficeError.corruptedContainer(
        detail: "유효하지 않은 UTF-16LE 문자열 길이 (\(byteLength) bytes)")
    }
    let text = try decodeUTF16LE(from: data, at: offset, byteLength: byteLength)
    offset += byteLength
    return text
  }

  /// Decodes raw UTF-16LE bytes of `byteLength` starting at `offset` using a 64-bit SWAR ASCII fast path (`0xFF80_FF80_FF80_FF80`).
  public static func decodeUTF16LE(from data: Data, at offset: Int, byteLength: Int) throws
    -> String
  {
    guard offset >= 0, byteLength >= 0, byteLength % 2 == 0, byteLength <= data.count - offset
    else {
      throw EasyOfficeError.corruptedContainer(
        detail: "UTF-16LE 디코딩 범위 초과 (offset: \(offset), length: \(byteLength))")
    }
    let unitCount = byteLength / 2
    if unitCount == 0 { return "" }
    return data.withUnsafeBytes { rawBuffer in
      var isPureASCII = true
      var scanCursor = 0
      while scanCursor <= byteLength - 8 {
        let word =
          rawBuffer.loadUnaligned(fromByteOffset: offset + scanCursor, as: UInt64.self)
          .littleEndian
        if (word & 0xFF80_FF80_FF80_FF80) != 0 {
          isPureASCII = false
          break
        }
        scanCursor += 8
      }
      if isPureASCII {
        while scanCursor < byteLength {
          let unit =
            rawBuffer.loadUnaligned(fromByteOffset: offset + scanCursor, as: UInt16.self)
            .littleEndian
          if (unit & 0xFF80) != 0 {
            isPureASCII = false
            break
          }
          scanCursor += 2
        }
      }
      if isPureASCII {
        return String(unsafeUninitializedCapacity: unitCount) { dstBuf in
          var srcOffset = 0
          var dstIdx = 0
          while srcOffset <= byteLength - 8 {
            let word =
              rawBuffer.loadUnaligned(fromByteOffset: offset + srcOffset, as: UInt64.self)
              .littleEndian
            dstBuf[dstIdx] = UInt8(truncatingIfNeeded: word)
            dstBuf[dstIdx + 1] = UInt8(truncatingIfNeeded: word >> 16)
            dstBuf[dstIdx + 2] = UInt8(truncatingIfNeeded: word >> 32)
            dstBuf[dstIdx + 3] = UInt8(truncatingIfNeeded: word >> 48)
            srcOffset += 8
            dstIdx += 4
          }
          while srcOffset < byteLength {
            let unit =
              rawBuffer.loadUnaligned(fromByteOffset: offset + srcOffset, as: UInt16.self)
              .littleEndian
            dstBuf[dstIdx] = UInt8(truncatingIfNeeded: unit)
            srcOffset += 2
            dstIdx += 1
          }
          return unitCount
        }
      }

      var units = [UInt16](repeating: 0, count: unitCount)
      for idx in 0..<unitCount {
        units[idx] =
          rawBuffer.loadUnaligned(fromByteOffset: offset + idx * 2, as: UInt16.self).littleEndian
      }
      return String(decoding: units, as: UTF16.self)
    }
  }

  /// Computes RFC 1950 Adler-32 checksum over `data`.
  public static func adler32(_ data: Data) -> UInt32 {
    Adler32Calculator.compute(for: data)
  }
}

public typealias BinaryContainerCodec = BitPacking

/// RFC 1950 Adler-32 checksum calculator with `NMAX = 5,552` deferred modulo reduction.
public enum Adler32Calculator {
  public static let modulo: UInt32 = 65_521
  public static let nmax: Int = 5_552

  public static func compute(for data: Data) -> UInt32 {
    var s1: UInt32 = 1
    var s2: UInt32 = 0
    data.withUnsafeBytes { (rawBuffer: UnsafeRawBufferPointer) -> Void in
      let count = rawBuffer.count
      var offset = 0
      while offset < count {
        let chunkEnd = min(count, offset + nmax)
        while offset <= chunkEnd - 8 {
          let b0 = UInt32(rawBuffer[offset])
          let b1 = UInt32(rawBuffer[offset + 1])
          let b2 = UInt32(rawBuffer[offset + 2])
          let b3 = UInt32(rawBuffer[offset + 3])
          let b4 = UInt32(rawBuffer[offset + 4])
          let b5 = UInt32(rawBuffer[offset + 5])
          let b6 = UInt32(rawBuffer[offset + 6])
          let b7 = UInt32(rawBuffer[offset + 7])
          s2 &+=
            8 &* s1
            &+ 8 &* b0
            &+ 7 &* b1
            &+ 6 &* b2
            &+ 5 &* b3
            &+ 4 &* b4
            &+ 3 &* b5
            &+ 2 &* b6
            &+ b7
          s1 &+= b0 &+ b1 &+ b2 &+ b3 &+ b4 &+ b5 &+ b6 &+ b7
          offset += 8
        }
        while offset < chunkEnd {
          s1 &+= UInt32(rawBuffer[offset])
          s2 &+= s1
          offset += 1
        }
        s1 %= modulo
        s2 %= modulo
      }
    }
    return (s2 << 16) | s1
  }

  public static func verifyZlibTrailer(in zlibData: Data, decompressed: Data) -> Bool {
    guard zlibData.count >= 6 else { return false }
    let base = zlibData.startIndex
    let cmf = UInt16(zlibData[base])
    let flg = UInt16(zlibData[base + 1])
    guard (cmf & 0x0F) == 8, ((cmf << 8) | flg) % 31 == 0 else { return false }
    let trailerBase = base + zlibData.count - 4
    let b0 = UInt32(zlibData[trailerBase])
    let b1 = UInt32(zlibData[trailerBase + 1])
    let b2 = UInt32(zlibData[trailerBase + 2])
    let b3 = UInt32(zlibData[trailerBase + 3])
    let expectedAdler = (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
    return expectedAdler == compute(for: decompressed)
  }
}

/// Slicing-by-8 IEEE 802.3 CRC-32 calculator (reflected polynomial `0xEDB88320`).
public enum CRC32Calculator {
  public static let polynomial: UInt32 = 0xEDB8_8320

  /// Contiguous 1D `8 × 256` (`2,048` elements) Slicing-by-8 lookup table for 64-bit (8-byte) stride CRC-32 computation.
  public static let flatSlicingTable: [UInt32] = {
    var table = [UInt32](repeating: 0, count: 8 * 256)
    for index in 0..<256 {
      var crc = UInt32(index)
      for _ in 0..<8 {
        crc = (crc & 1) != 0 ? ((crc >> 1) ^ polynomial) : (crc >> 1)
      }
      table[index] = crc
    }
    for slice in 1..<8 {
      let prevBase = (slice - 1) << 8
      let currBase = slice << 8
      for index in 0..<256 {
        let prev = table[prevBase + index]
        table[currBase + index] = (prev >> 8) ^ table[Int(prev & 0xFF)]
      }
    }
    return table
  }()

  public static let slicingTables: [[UInt32]] = (0..<8).map { s in
    Array(flatSlicingTable[(s << 8)..<((s + 1) << 8)])
  }

  public static var lookupTable: [UInt32] { slicingTables[0] }

  public static func compute(for data: Data) -> UInt32 {
    var crc: UInt32 = 0xFFFF_FFFF
    flatSlicingTable.withUnsafeBufferPointer { (tablePtr: UnsafeBufferPointer<UInt32>) -> Void in
      data.withUnsafeBytes { (rawBuffer: UnsafeRawBufferPointer) -> Void in
        let count = rawBuffer.count
        var offset = 0
        while offset <= count - 8 {
          let one =
            rawBuffer.loadUnaligned(fromByteOffset: offset, as: UInt32.self).littleEndian ^ crc
          let two = rawBuffer.loadUnaligned(fromByteOffset: offset + 4, as: UInt32.self)
            .littleEndian
          let c0: UInt32 =
            tablePtr[1792 + Int(one & 0xFF)]
            ^ tablePtr[1536 + Int((one >> 8) & 0xFF)]
            ^ tablePtr[1280 + Int((one >> 16) & 0xFF)]
            ^ tablePtr[1024 + Int((one >> 24) & 0xFF)]
          let c1: UInt32 =
            tablePtr[768 + Int(two & 0xFF)]
            ^ tablePtr[512 + Int((two >> 8) & 0xFF)]
            ^ tablePtr[256 + Int((two >> 16) & 0xFF)]
            ^ tablePtr[Int((two >> 24) & 0xFF)]
          crc = c0 ^ c1
          offset += 8
        }
        while offset < count {
          let byte = rawBuffer[offset]
          crc = (crc >> 8) ^ tablePtr[Int((crc ^ UInt32(byte)) & 0xFF)]
          offset += 1
        }
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
    if path.hasPrefix("/") || path.contains("\\") || path.contains(":") || path.utf8.contains(0) {
      throw EasyOfficeError.securityViolation(
        detail: "절대 경로, 역슬래시, 드라이브 구분자, 또는 NUL 바이트가 포함된 ZIP 경로: \(path)")
    }
    let trimmed = path.hasSuffix("/") ? String(path.dropLast()) : path
    guard !trimmed.isEmpty else {
      throw EasyOfficeError.securityViolation(detail: "비어 있는 ZIP 디렉터리 경로: \(path)")
    }
    let segments = trimmed.split(separator: "/", omittingEmptySubsequences: false)
    for segment in segments {
      if segment.isEmpty || segment == "." || segment == ".." {
        throw EasyOfficeError.securityViolation(
          detail: "상위 디렉터리 순회(..) 또는 비정상 세그먼트가 포함된 ZIP 경로: \(path)")
      }
    }
  }

  public static func pack(entries: [ZipContainerEntry]) throws -> Data {
    guard entries.count <= Int(UInt16.max) else {
      throw EasyOfficeError.corruptedContainer(
        detail: "ZIP 엔트리 개수(\(entries.count))가 표준 EOCD 한도(65,535)를 초과합니다.")
    }
    var archive = Data()
    var centralDirectory = Data()

    for entry in entries {
      try validateEntryPath(entry.path)
      let pathBytes = Data(entry.path.utf8)
      guard pathBytes.count <= Int(UInt16.max) else {
        throw EasyOfficeError.corruptedContainer(
          detail: "ZIP 엔트리 경로 길이(\(pathBytes.count) 바이트)가 최대 한도(65,535)를 초과합니다.")
      }
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
      BitPacking.appendUInt16LE(0x0800, to: &archive)  // UTF-8 filename flag (bit 11)
      BitPacking.appendUInt16LE(methodCode, to: &archive)
      BitPacking.appendUInt16LE(0, to: &archive)  // mod time
      BitPacking.appendUInt16LE(0, to: &archive)  // mod date
      BitPacking.appendUInt32LE(crc32, to: &archive)
      BitPacking.appendUInt32LE(compressedSize, to: &archive)
      BitPacking.appendUInt32LE(uncompressedSize, to: &archive)
      BitPacking.appendUInt16LE(UInt16(pathBytes.count), to: &archive)
      BitPacking.appendUInt16LE(0, to: &archive)  // extra field length
      archive.append(pathBytes)
      archive.append(payloadBytes)

      // Central Directory File Header (46 bytes + file name)
      BitPacking.appendUInt32LE(centralDirectorySignature, to: &centralDirectory)
      BitPacking.appendUInt16LE(zipVersionNeeded, to: &centralDirectory)  // version made by
      BitPacking.appendUInt16LE(zipVersionNeeded, to: &centralDirectory)  // version needed
      BitPacking.appendUInt16LE(0x0800, to: &centralDirectory)  // UTF-8 flag
      BitPacking.appendUInt16LE(methodCode, to: &centralDirectory)
      BitPacking.appendUInt16LE(0, to: &centralDirectory)
      BitPacking.appendUInt16LE(0, to: &centralDirectory)
      BitPacking.appendUInt32LE(crc32, to: &centralDirectory)
      BitPacking.appendUInt32LE(compressedSize, to: &centralDirectory)
      BitPacking.appendUInt32LE(uncompressedSize, to: &centralDirectory)
      BitPacking.appendUInt16LE(UInt16(pathBytes.count), to: &centralDirectory)
      BitPacking.appendUInt16LE(0, to: &centralDirectory)  // extra length
      BitPacking.appendUInt16LE(0, to: &centralDirectory)  // comment length
      BitPacking.appendUInt16LE(0, to: &centralDirectory)  // disk number
      BitPacking.appendUInt16LE(0, to: &centralDirectory)  // internal attrs
      BitPacking.appendUInt32LE(0, to: &centralDirectory)  // external attrs
      BitPacking.appendUInt32LE(localHeaderOffset, to: &centralDirectory)
      centralDirectory.append(pathBytes)
    }

    let centralDirOffset = UInt32(archive.count)
    let centralDirSize = UInt32(centralDirectory.count)
    archive.append(centralDirectory)

    // End of Central Directory Record (22 bytes)
    BitPacking.appendUInt32LE(endOfCentralDirectorySignature, to: &archive)
    BitPacking.appendUInt16LE(0, to: &archive)  // disk number
    BitPacking.appendUInt16LE(0, to: &archive)  // disk with CD
    BitPacking.appendUInt16LE(UInt16(entries.count), to: &archive)
    BitPacking.appendUInt16LE(UInt16(entries.count), to: &archive)
    BitPacking.appendUInt32LE(centralDirSize, to: &archive)
    BitPacking.appendUInt32LE(centralDirOffset, to: &archive)
    BitPacking.appendUInt16LE(0, to: &archive)  // comment length

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
      throw EasyOfficeError.corruptedContainer(
        detail: "ZIP End of Central Directory 시그니처(0x06054B50)를 찾을 수 없습니다.")
    }

    let entryCount = Int(try BitPacking.readUInt16LE(from: data, at: eocdOffset + 10))
    guard entryCount <= policy.maxEntryCount else {
      throw EasyOfficeError.securityViolation(
        detail: "ZIP 엔트리 개수(\(entryCount))가 보안 허용 한도(\(policy.maxEntryCount))를 초과했습니다."
      )
    }

    let centralDirSize = Int(try BitPacking.readUInt32LE(from: data, at: eocdOffset + 12))
    let centralDirOffset = Int(try BitPacking.readUInt32LE(from: data, at: eocdOffset + 16))
    guard centralDirOffset >= 0, centralDirSize >= 0,
      centralDirOffset + centralDirSize <= eocdOffset
    else {
      throw EasyOfficeError.corruptedContainer(detail: "ZIP Central Directory 오프셋 또는 크기가 손상되었습니다.")
    }

    var results: [ZipContainerEntry] = []
    results.reserveCapacity(entryCount)
    var seenPaths = Set<String>()
    seenPaths.reserveCapacity(entryCount)

    var cdCursor = centralDirOffset
    var totalUncompressedBytes = 0

    for _ in 0..<entryCount {
      let signature = try BitPacking.readUInt32LE(from: data, at: cdCursor)
      guard signature == centralDirectorySignature else {
        throw EasyOfficeError.corruptedContainer(
          detail: "잘못된 Central Directory 시그니처: \(String(format: "0x%08X", signature))")
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
      let nameData = data.subdata(
        in: (data.startIndex + nameStart)..<(data.startIndex + nameStart + nameLen))
      guard let path = String(data: nameData, encoding: .utf8) else {
        throw EasyOfficeError.corruptedContainer(detail: "ZIP 엔트리 파일명이 유효한 UTF-8이 아닙니다.")
      }
      try validateEntryPath(path)
      guard seenPaths.insert(path).inserted else {
        throw EasyOfficeError.securityViolation(detail: "중복된 ZIP 엔트리 경로가 감지되었습니다: \(path)")
      }

      guard uncompressedSize >= 0, uncompressedSize <= policy.maxSingleEntryUncompressedBytes else {
        throw EasyOfficeError.securityViolation(
          detail:
            "ZIP 단일 엔트리 크기(\(uncompressedSize))가 한도(\(policy.maxSingleEntryUncompressedBytes))를 초과했습니다."
        )
      }
      totalUncompressedBytes += uncompressedSize
      guard totalUncompressedBytes <= policy.maxTotalUncompressedBytes else {
        throw EasyOfficeError.securityViolation(
          detail:
            "ZIP 누적 압축 해제 크기(\(totalUncompressedBytes))가 한도(\(policy.maxTotalUncompressedBytes))를 초과했습니다."
        )
      }

      if compressedSize > 0 {
        let ratio = Double(uncompressedSize) / Double(compressedSize)
        if ratio > policy.maxCompressionRatio {
          throw EasyOfficeError.decompressionBombDetected(
            ratio: ratio, maxRatio: policy.maxCompressionRatio)
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
      guard localHeaderOffset >= 0,
        localHeaderOffset + 30 + localNameLen <= data.count
      else {
        throw EasyOfficeError.corruptedContainer(detail: "Local File Header 파일명 영역이 절단되었습니다.")
      }
      let localNameData = data.subdata(
        in: (data.startIndex + localHeaderOffset + 30)..<(data.startIndex + localHeaderOffset + 30
          + localNameLen))
      guard localNameData == nameData else {
        throw EasyOfficeError.securityViolation(
          detail: "ZIP Local File Header 파일명과 Central Directory 파일명이 일치하지 않습니다: \(path)")
      }

      let payloadStart = localHeaderOffset + 30 + localNameLen + localExtraLen
      guard payloadStart >= 0, compressedSize >= 0, payloadStart + compressedSize <= data.count
      else {
        throw EasyOfficeError.corruptedContainer(detail: "ZIP 엔트리 '\(path)' 데이터 영역이 파일 경계를 벗어납니다.")
      }

      if path.hasSuffix("/") && uncompressedSize == 0 {
        cdCursor = nameStart + nameLen + extraLen + commentLen
        continue
      }

      let compressedSlice = data.subdata(
        in: (data.startIndex + payloadStart)..<(data.startIndex + payloadStart + compressedSize))
      let uncompressedData: Data
      switch method {
      case .store:
        guard compressedSize == uncompressedSize else {
          throw EasyOfficeError.corruptedContainer(
            detail: "Store 엔트리 '\(path)'의 압축/원본 크기가 일치하지 않습니다.")
        }
        uncompressedData = compressedSlice
      case .deflate:
        if uncompressedSize == 0 {
          guard compressedSize == 0 else {
            throw EasyOfficeError.corruptedContainer(
              detail: "Deflate 엔트리 '\(path)'의 원본 크기는 0이지만 압축 데이터가 존재합니다.")
          }
          uncompressedData = Data()
        } else {
          uncompressedData = try decompressRawDeflate(
            compressedSlice, expectedSize: uncompressedSize)
        }
      }

      let actualCRC32 = CRC32Calculator.compute(for: uncompressedData)
      guard actualCRC32 == expectedCRC32 else {
        throw EasyOfficeError.corruptedContainer(
          detail:
            "ZIP 엔트리 '\(path)' CRC-32 무결성 검증 실패 (expected: \(String(format: "0x%08X", expectedCRC32)), actual: \(String(format: "0x%08X", actualCRC32)))"
        )
      }

      results.append(
        ZipContainerEntry(path: path, data: uncompressedData, compressionMethod: method))
      cdCursor = nameStart + nameLen + extraLen + commentLen
    }

    return results
  }

  private static func findEndOfCentralDirectory(in data: Data) -> Int? {
    let minOffset = max(0, data.count - 65_557)
    let eocdSig = endOfCentralDirectorySignature
    return data.withUnsafeBytes { rawBuffer -> Int? in
      for offset in stride(from: rawBuffer.count - 22, through: minOffset, by: -1) {
        let word = rawBuffer.loadUnaligned(fromByteOffset: offset, as: UInt32.self).littleEndian
        if word == eocdSig {
          return offset
        }
      }
      return nil
    }
  }

  internal static func compressRawDeflate(_ input: Data) throws -> Data {
    let destinationCapacity = max(input.count + (input.count / 8) + 512, 1024)
    var destination = Data(count: destinationCapacity)
    let compressedBytes = input.withUnsafeBytes { srcBuffer -> Int in
      guard let srcBase = srcBuffer.bindMemory(to: UInt8.self).baseAddress else { return 0 }
      return destination.withUnsafeMutableBytes { dstBuffer -> Int in
        guard let dstBase = dstBuffer.bindMemory(to: UInt8.self).baseAddress else { return 0 }
        return compression_encode_buffer(
          dstBase, destinationCapacity, srcBase, input.count, nil, COMPRESSION_ZLIB)
      }
    }
    guard compressedBytes > 0 else {
      throw EasyOfficeError.corruptedContainer(detail: "Deflate 압축에 실패했습니다.")
    }
    destination.removeSubrange(compressedBytes..<destination.count)
    return destination
  }

  internal static func decompressRawDeflate(_ input: Data, expectedSize: Int) throws -> Data {
    let probeCapacity = expectedSize + 1
    var destination = Data(count: probeCapacity)
    let decodedBytes = input.withUnsafeBytes { srcBuffer -> Int in
      guard let srcBase = srcBuffer.bindMemory(to: UInt8.self).baseAddress else { return 0 }
      return destination.withUnsafeMutableBytes { dstBuffer -> Int in
        guard let dstBase = dstBuffer.bindMemory(to: UInt8.self).baseAddress else { return 0 }
        return compression_decode_buffer(
          dstBase, probeCapacity, srcBase, input.count, nil, COMPRESSION_ZLIB)
      }
    }
    guard decodedBytes == expectedSize else {
      throw EasyOfficeError.corruptedContainer(
        detail: "Deflate 압축 해제 크기 불일치 (expected: \(expectedSize), actual: \(decodedBytes))")
    }
    destination.removeSubrange(decodedBytes..<destination.count)
    return destination
  }

  /// Decompresses a raw Deflate stream of unknown uncompressed length with Fail-Closed ratio & size bounds.
  internal static func decompressRawDeflateStream(
    _ input: Data,
    maxOutputBytes: Int = 64 * 1024 * 1024,
    maxRatio: Double = 100.0
  ) throws -> Data {
    if input.isEmpty { return Data() }
    let ceiling = min(maxOutputBytes, max(4096, Int(Double(input.count) * maxRatio)))
    var capacity = min(max(input.count * 8, 4096), ceiling)
    while true {
      let probeCapacity = capacity + 1
      var destination = Data(count: probeCapacity)
      let decodedBytes = input.withUnsafeBytes { srcBuffer -> Int in
        guard let srcBase = srcBuffer.bindMemory(to: UInt8.self).baseAddress else { return 0 }
        return destination.withUnsafeMutableBytes { dstBuffer -> Int in
          guard let dstBase = dstBuffer.bindMemory(to: UInt8.self).baseAddress else { return 0 }
          return compression_decode_buffer(
            dstBase, probeCapacity, srcBase, input.count, nil, COMPRESSION_ZLIB)
        }
      }
      guard decodedBytes > 0 else {
        throw EasyOfficeError.corruptedContainer(detail: "Deflate 스트림 압축 해제에 실패했습니다.")
      }
      if decodedBytes <= capacity {
        let ratio = Double(decodedBytes) / Double(input.count)
        if decodedBytes > maxOutputBytes || (decodedBytes > 4096 && ratio > maxRatio) {
          throw EasyOfficeError.decompressionBombDetected(ratio: ratio, maxRatio: maxRatio)
        }
        destination.removeSubrange(decodedBytes..<destination.count)
        return destination
      }
      if capacity >= ceiling {
        let ratio = Double(decodedBytes) / Double(input.count)
        throw EasyOfficeError.decompressionBombDetected(
          ratio: max(ratio, maxRatio + 1.0), maxRatio: maxRatio)
      }
      capacity = min(capacity * 2, ceiling)
    }
  }
}
