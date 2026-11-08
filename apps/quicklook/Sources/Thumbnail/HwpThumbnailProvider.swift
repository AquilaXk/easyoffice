import CoreGraphics
import Foundation
import ImageIO
import OSLog
import QuickLookThumbnailing
import UniformTypeIdentifiers

// Finder icon thumbnails of HWP and HWPX files. Structure after the reference app's
// `apps/desktop/quicklook/Sources/Thumbnail/HwpThumbnailProvider.swift` (MIT, see
// THIRD_PARTY_NOTICES.md): the picture stored in the file when there is one, else the first page.
// Differences: a failure is answered with no thumbnail (Finder keeps the system's default icon)
// instead of a drawn stand-in, and file names are not logged.

final class HwpThumbnailProvider: QLThumbnailProvider {
    private static let logger = Logger(
        subsystem: Bundle.main.bundleIdentifier ?? "com.aquilaxk.easyoffice.quicklook.thumbnail",
        category: "ThumbnailProvider"
    )

    override func provideThumbnail(
        for request: QLFileThumbnailRequest,
        _ handler: @escaping (QLThumbnailReply?, Error?) -> Void
    ) {
        do {
            let fileData = try easyOfficeQuickLookReadFile(request.fileURL)
            let requested = max(request.maximumSize.width, request.maximumSize.height) * request.scale
            let maxPixels = UInt32(max(1, min(requested, CGFloat(easyOfficeQuickLookMaxThumbnailDimension))))

            if let embedded = try? Self.embeddedThumbnail(from: fileData, maximumPixelDimension: maxPixels) {
                handler(Self.reply(for: request, image: embedded.image, imageSize: embedded.size), nil)
                return
            }
            let rendered = try Self.renderedThumbnail(from: fileData, maximumPixelDimension: maxPixels)
            handler(Self.reply(for: request, image: rendered.image, imageSize: rendered.size), nil)
        } catch {
            Self.logger.warning("Thumbnail skipped reason=\(easyOfficeQuickLookFailureReason(for: error), privacy: .public)")
            handler(nil, error)
        }
    }

    private static func embeddedThumbnail(
        from data: Data,
        maximumPixelDimension: UInt32
    ) throws -> (image: CGImage, size: CGSize) {
        let result = data.withUnsafeBytes { rawBuffer -> EasyOfficeQuickLookThumbnailResult in
            eo_ql_extract_embedded_thumbnail(rawBuffer.bindMemory(to: UInt8.self).baseAddress, data.count)
        }
        guard result.status == EasyOfficeQuickLookStatus.ok.rawValue else {
            throw EasyOfficeQuickLookError.status(result.status)
        }
        let imageData = try easyOfficeQuickLookData(from: result.bytes)
        guard let image = decodeImage(imageData, maximumPixelDimension: maximumPixelDimension) else {
            throw EasyOfficeQuickLookError.renderFailed
        }
        let width = result.width == 0 ? image.width : Int(result.width)
        let height = result.height == 0 ? image.height : Int(result.height)
        return (image, CGSize(width: max(1, width), height: max(1, height)))
    }

    private static func renderedThumbnail(
        from data: Data,
        maximumPixelDimension: UInt32
    ) throws -> (image: CGImage, size: CGSize) {
        let result = data.withUnsafeBytes { rawBuffer -> EasyOfficeQuickLookRenderResult in
            eo_ql_render_first_page_png(rawBuffer.bindMemory(to: UInt8.self).baseAddress, data.count, maximumPixelDimension)
        }
        guard result.status == EasyOfficeQuickLookStatus.ok.rawValue else {
            throw EasyOfficeQuickLookError.status(result.status)
        }
        let pngData = try easyOfficeQuickLookData(from: result.bytes)
        guard let image = decodeImage(pngData, maximumPixelDimension: maximumPixelDimension) else {
            throw EasyOfficeQuickLookError.renderFailed
        }
        return (image, CGSize(width: max(1, result.width), height: max(1, result.height)))
    }

    private static func decodeImage(_ data: Data, maximumPixelDimension: UInt32) -> CGImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else {
            return nil
        }
        let options: CFDictionary = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: Int(maximumPixelDimension)
        ] as CFDictionary
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options)
            ?? CGImageSourceCreateImageAtIndex(source, 0, nil)
    }

    private static func reply(
        for request: QLFileThumbnailRequest,
        image: CGImage,
        imageSize: CGSize
    ) -> QLThumbnailReply {
        let contextSize = aspectFit(imageSize, within: request.maximumSize)
        let reply = QLThumbnailReply(contextSize: contextSize) { context in
            draw(image: image, in: context, fallbackSize: contextSize)
            return true
        }
        reply.extensionBadge = request.fileURL.pathExtension.uppercased()
        return reply
    }

    private static func aspectFit(_ source: CGSize, within maximumSize: CGSize) -> CGSize {
        guard source.width > 0, source.height > 0, maximumSize.width > 0, maximumSize.height > 0 else {
            return CGSize(width: 128, height: 128)
        }
        let scale = min(maximumSize.width / source.width, maximumSize.height / source.height)
        return CGSize(width: max(1, source.width * scale), height: max(1, source.height * scale))
    }

    private static func draw(image: CGImage, in context: CGContext, fallbackSize: CGSize) {
        let rect = drawingBounds(in: context, fallbackSize: fallbackSize)
        context.saveGState()
        context.setFillColor(red: 1, green: 1, blue: 1, alpha: 1)
        context.fill(rect)
        context.draw(image, in: rect)
        context.restoreGState()
    }

    private static func drawingBounds(in context: CGContext, fallbackSize: CGSize) -> CGRect {
        let clipBounds = context.boundingBoxOfClipPath
        guard !clipBounds.isNull, !clipBounds.isInfinite, clipBounds.width > 0, clipBounds.height > 0 else {
            return CGRect(origin: .zero, size: fallbackSize)
        }
        return clipBounds
    }
}
