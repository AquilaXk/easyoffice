import Foundation
import OSLog
import QuickLookUI
import UniformTypeIdentifiers

// Quick Look preview of HWP and HWPX files (the Space bar in Finder). Structure after the reference
// app's `apps/desktop/quicklook/Sources/Preview/HwpPreviewProvider.swift` (MIT, see
// THIRD_PARTY_NOTICES.md). Differences: a failure is thrown (the system shows its default preview)
// instead of replying with a text message, and file names are logged as private.

final class HwpPreviewProvider: QLPreviewProvider, QLPreviewingController {
    private static let logger = Logger(
        subsystem: Bundle.main.bundleIdentifier ?? "com.aquilaxk.easyoffice.quicklook.preview",
        category: "PreviewProvider"
    )

    func providePreview(for request: QLFilePreviewRequest) async throws -> QLPreviewReply {
        do {
            let fileData = try easyOfficeQuickLookReadFile(request.fileURL)
            let result = fileData.withUnsafeBytes { rawBuffer -> EasyOfficeQuickLookRenderResult in
                eo_ql_render_preview_pdf(rawBuffer.bindMemory(to: UInt8.self).baseAddress, fileData.count)
            }
            guard result.status == EasyOfficeQuickLookStatus.ok.rawValue else {
                throw EasyOfficeQuickLookError.status(result.status)
            }
            let data = try easyOfficeQuickLookData(from: result.bytes)
            let size = CGSize(width: max(1, result.width), height: max(1, result.height))
            Self.logger.info("Preview ready pages=\(result.page_count, privacy: .public) bytes=\(data.count, privacy: .public)")
            let title = request.fileURL.lastPathComponent
            return QLPreviewReply(dataOfContentType: .pdf, contentSize: size) { reply in
                reply.title = title
                return data
            }
        } catch {
            Self.logger.warning("Preview skipped reason=\(easyOfficeQuickLookFailureReason(for: error), privacy: .public)")
            throw error
        }
    }
}
