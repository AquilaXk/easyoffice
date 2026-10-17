import Foundation

/// Typed Fail-Closed errors across format detection, container verification, and formula evaluation.
public enum EasyOfficeError: Error, Sendable, Equatable, LocalizedError {
    case emptyPayload
    case payloadTooLarge(actualBytes: Int, maxAllowedBytes: Int)
    case unsupportedExtension(String)
    case signatureMismatch(expected: ContainerFamily, fileName: String)
    case invalidUTF8Text(fileName: String)
    case invalidCellAddress(String)
    case invalidCellRange(String)
    case formulaSyntaxError(String)

    public var errorDescription: String? {
        switch self {
        case .emptyPayload:
            return "파일 데이터가 비어 있어 문서를 열 수 없습니다 (Fail-Closed)."
        case let .payloadTooLarge(actual, maxAllowed):
            return "파일 크기(\(actual) bytes)가 허용 한도(\(maxAllowed) bytes)를 초과했습니다."
        case let .unsupportedExtension(ext):
            return "지원하지 않는 파일 확장자입니다: \(ext)"
        case let .signatureMismatch(expected, fileName):
            return "파일 '\(fileName)'의 바이너리 매직 시그니처가 기대된 컨테이너 규격(\(expected.rawValue))과 일치하지 않습니다."
        case let .invalidUTF8Text(fileName):
            return "텍스트 문서 '\(fileName)'가 유효한 UTF-8 인코딩이 아니거나 제어 바이트로 오염되어 있습니다."
        case let .invalidCellAddress(addr):
            return "유효하지 않은 셀 주소입니다: \(addr)"
        case let .invalidCellRange(range):
            return "유효하지 않은 셀 범위입니다: \(range)"
        case let .formulaSyntaxError(detail):
            return "수식 구문 오류: \(detail)"
        }
    }
}

/// Verified format inspection result.
public struct VerifiedDocumentSignature: Sendable, Equatable {
    public let format: DocumentFormat
    public let containerFamily: ContainerFamily
    public let byteCount: Int

    public init(format: DocumentFormat, containerFamily: ContainerFamily, byteCount: Int) {
        self.format = format
        self.containerFamily = containerFamily
        self.byteCount = byteCount
    }
}

/// Fail-Closed binary magic-byte and container inspector.
public enum FormatDetector {
    /// Default single-document in-memory safety ceiling (256 MiB).
    public static let defaultMaxPayloadBytes = 256 * 1024 * 1024

    /// Standard ZIP local file header (`PK\x03\x04`).
    public static let zipMagic: [UInt8] = [0x50, 0x4B, 0x03, 0x04]

    /// Standard OLE2 Compound File Binary Format header (`\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1`).
    public static let ole2Magic: [UInt8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]

    /// Standard PDF header (`%PDF-`).
    public static let pdfMagic: [UInt8] = [0x25, 0x50, 0x44, 0x46, 0x2D]

    /// Standard RTF header (`{\rtf`).
    public static let rtfMagic: [UInt8] = [0x7B, 0x5C, 0x72, 0x74, 0x66]

    /// Verifies that `data` matches the container signature required by `fileName`'s extension.
    public static func verify(
        data: Data,
        fileName: String,
        maxAllowedBytes: Int = defaultMaxPayloadBytes
    ) throws -> VerifiedDocumentSignature {
        guard !data.isEmpty else {
            throw EasyOfficeError.emptyPayload
        }
        guard data.count <= maxAllowedBytes else {
            throw EasyOfficeError.payloadTooLarge(actualBytes: data.count, maxAllowedBytes: maxAllowedBytes)
        }
        guard let format = DocumentFormat.from(fileNameOrExtension: fileName) else {
            throw EasyOfficeError.unsupportedExtension(fileName)
        }

        let bytes = [UInt8](data.prefix(16))
        let family = format.containerFamily

        switch family {
        case .zipXML:
            guard bytes.starts(with: zipMagic) else {
                throw EasyOfficeError.signatureMismatch(expected: .zipXML, fileName: fileName)
            }
        case .ole2Compound:
            guard bytes.starts(with: ole2Magic) else {
                throw EasyOfficeError.signatureMismatch(expected: .ole2Compound, fileName: fileName)
            }
        case .pdfBinary:
            guard bytes.starts(with: pdfMagic) else {
                throw EasyOfficeError.signatureMismatch(expected: .pdfBinary, fileName: fileName)
            }
        case .rtfText:
            guard bytes.starts(with: rtfMagic) else {
                throw EasyOfficeError.signatureMismatch(expected: .rtfText, fileName: fileName)
            }
        case .plainText:
            // Reject binary magic headers masquerading as plain text and reject NUL bytes
            if bytes.starts(with: zipMagic) || bytes.starts(with: ole2Magic) || bytes.starts(with: pdfMagic) {
                throw EasyOfficeError.signatureMismatch(expected: .plainText, fileName: fileName)
            }
            if data.contains(0x00) || String(data: data, encoding: .utf8) == nil {
                throw EasyOfficeError.invalidUTF8Text(fileName: fileName)
            }
        }

        return VerifiedDocumentSignature(
            format: format,
            containerFamily: family,
            byteCount: data.count
        )
    }
}
