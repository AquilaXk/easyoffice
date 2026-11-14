import Darwin
import Foundation

/// Largest file the extensions read; the Rust side refuses the same size (render.rs MAX_INPUT_BYTES).
let easyOfficeQuickLookMaxFileSize = 50 * 1024 * 1024
/// Largest thumbnail side in pixels (render.rs MAX_PNG_SIDE).
let easyOfficeQuickLookMaxThumbnailDimension: UInt32 = 2048

/// Status codes of the Rust side (status.rs).
enum EasyOfficeQuickLookStatus: Int32 {
    case ok = 0
    case invalidInput = 1
    case parseFailed = 2
    case emptyDocument = 3
    case renderFailed = 4
    case noThumbnail = 5
    case tooLarge = 6
    case timeout = 7
}

/// Any failure ends in the system's default icon or preview (the extensions never draw a stand-in),
/// so the cases only differ in the reason that is logged.
enum EasyOfficeQuickLookError: Error {
    case fileTooLarge
    case notRegularFile
    case emptyData
    case status(Int32)
    case renderFailed

    var reason: String {
        switch self {
        case .fileTooLarge: return "file-too-large"
        case .notRegularFile: return "not-regular-file"
        case .emptyData: return "empty-data"
        case .renderFailed: return "render-failed"
        case .status(let code):
            switch EasyOfficeQuickLookStatus(rawValue: code) {
            case .invalidInput: return "invalid-input"
            case .parseFailed: return "parse-failed"
            case .emptyDocument: return "empty-document"
            case .renderFailed: return "render-failed"
            case .noThumbnail: return "no-thumbnail"
            case .tooLarge: return "too-large"
            case .timeout: return "timeout"
            case .ok, .none: return "unknown-status"
            }
        }
    }
}

/// Copies a buffer of the Rust side into `Data` and releases it.
func easyOfficeQuickLookData(from bytes: EasyOfficeQuickLookBytes) throws -> Data {
    guard let ptr = bytes.ptr, bytes.len > 0 else {
        throw EasyOfficeQuickLookError.renderFailed
    }
    defer { eo_ql_free_bytes(ptr, bytes.len) }
    return Data(bytes: ptr, count: bytes.len)
}

/// Reads the file Finder names via a safe file descriptor loop without mmap to prevent SIGBUS crashes.
/// Validates regular file status and bounds size during chunked Darwin reads.
func easyOfficeQuickLookReadFile(_ fileURL: URL) throws -> Data {
    let path = fileURL.resolvingSymlinksInPath().path
    let fd = Darwin.open(path, O_RDONLY | O_NONBLOCK | O_NOFOLLOW)
    guard fd >= 0 else {
        throw EasyOfficeQuickLookError.notRegularFile
    }
    defer { Darwin.close(fd) }

    var st = stat()
    guard fstat(fd, &st) == 0 else {
        throw EasyOfficeQuickLookError.notRegularFile
    }
    guard (st.st_mode & S_IFMT) == S_IFREG else {
        throw EasyOfficeQuickLookError.notRegularFile
    }
    if st.st_size > off_t(easyOfficeQuickLookMaxFileSize) {
        throw EasyOfficeQuickLookError.fileTooLarge
    }
    if st.st_size == 0 {
        throw EasyOfficeQuickLookError.emptyData
    }

    var data = Data()
    data.reserveCapacity(Int(min(st.st_size, off_t(easyOfficeQuickLookMaxFileSize))))
    let bufferSize = 64 * 1024
    var buffer = [UInt8](repeating: 0, count: bufferSize)

    while true {
        let bytesRead = Darwin.read(fd, &buffer, bufferSize)
        if bytesRead == 0 {
            break
        }
        if bytesRead < 0 {
            if errno == EINTR {
                continue
            }
            throw EasyOfficeQuickLookError.renderFailed
        }
        if data.count + bytesRead > easyOfficeQuickLookMaxFileSize {
            throw EasyOfficeQuickLookError.fileTooLarge
        }
        data.append(buffer, count: bytesRead)
    }

    if data.isEmpty {
        throw EasyOfficeQuickLookError.emptyData
    }
    return data
}

func easyOfficeQuickLookFailureReason(for error: Error) -> String {
    (error as? EasyOfficeQuickLookError)?.reason ?? "io-error"
}
