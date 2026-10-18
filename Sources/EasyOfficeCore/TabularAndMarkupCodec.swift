import Foundation

// MARK: - RFC 4180 CSV & TSV State-Machine Codec

/// Single-pass RFC 4180 CSV and TSV UTF-8 byte state-machine parser and serializer with Fail-Closed quote validation.
public enum CSVStateMachineCodec {

    private enum ParserState {
        case startOfField
        case inUnquotedField
        case inQuotedField
        case quoteInQuotedField
    }

    public static let maxExportRows = 10_000
    public static let maxExportColumns = 1_024

    public static func encodeWorksheet(_ sheet: WorksheetModel, delimiter: Character = ",") -> Data {
        var maxRow = -1
        var maxCol = -1
        var parsedCoordinates: [(coord: PackedCellCoordinate, value: String)] = []

        for (coord, rawValue) in sheet.cellsByCoordinate where !rawValue.isEmpty {
            let r = coord.row
            let c = coord.column
            if r >= 0, r < maxExportRows, c >= 0, c < maxExportColumns {
                maxRow = max(maxRow, r)
                maxCol = max(maxCol, c)
                parsedCoordinates.append((coord, rawValue))
            }
        }

        guard maxRow >= 0, maxCol >= 0 else {
            return Data()
        }

        var grid = Array(
            repeating: Array(repeating: "", count: maxCol + 1),
            count: maxRow + 1
        )
        for item in parsedCoordinates {
            grid[item.coord.row][item.coord.column] = item.value
        }

        let delimiterString = String(delimiter)
        let lines = grid.map { row -> String in
            row.map { serializeField($0, delimiter: delimiter) }.joined(separator: delimiterString)
        }
        return Data(lines.joined(separator: "\r\n").utf8)
    }

    public static func decodeWorksheet(
        from data: Data,
        name: String,
        delimiter: Character = ","
    ) throws -> WorksheetModel {
        if data.contains(0x00) {
            throw EasyOfficeError.invalidUTF8Text(fileName: name)
        }
        guard let text = String(data: data, encoding: .utf8) else {
            throw EasyOfficeError.invalidUTF8Text(fileName: name)
        }

        let rows = try parseRows(from: text, delimiter: delimiter)
        var worksheet = WorksheetModel(name: name)

        for (rowIdx, rowFields) in rows.enumerated() {
            for (colIdx, value) in rowFields.enumerated() where !value.isEmpty {
                try worksheet.setCell(row: rowIdx, column: colIdx, rawValue: value)
            }
        }
        return worksheet
    }

    public static func parseRows(from text: String, delimiter: Character = ",") throws -> [[String]] {
        guard !text.isEmpty else { return [] }

        let bytes = Array(text.utf8)
        let quote: UInt8 = 0x22
        let cr: UInt8 = 0x0D
        let lf: UInt8 = 0x0A
        let delimByte: UInt8 = delimiter.asciiValue ?? 0x2C

        var rows: [[String]] = []
        var currentRow: [String] = []
        var quotedBytes: [UInt8] = []
        var unquotedStartIdx = 0
        var state: ParserState = .startOfField
        var idx = 0
        let count = bytes.count

        while idx < count {
            let b = bytes[idx]

            switch state {
            case .startOfField:
                if b == quote {
                    quotedBytes.removeAll(keepingCapacity: true)
                    state = .inQuotedField
                } else if b == delimByte {
                    currentRow.append("")
                } else if b == cr || b == lf {
                    if b == cr && idx + 1 < count && bytes[idx + 1] == lf {
                        idx += 1
                    }
                    currentRow.append("")
                    rows.append(currentRow)
                    currentRow = []
                } else {
                    unquotedStartIdx = idx
                    state = .inUnquotedField
                }

            case .inUnquotedField:
                if b == quote {
                    throw EasyOfficeError.corruptedContainer(
                        detail: "Unexpected quote inside unquoted CSV/TSV field"
                    )
                } else if b == delimByte {
                    let field = String(decoding: bytes[unquotedStartIdx..<idx], as: UTF8.self)
                    currentRow.append(field)
                    state = .startOfField
                } else if b == cr || b == lf {
                    let field = String(decoding: bytes[unquotedStartIdx..<idx], as: UTF8.self)
                    if b == cr && idx + 1 < count && bytes[idx + 1] == lf {
                        idx += 1
                    }
                    currentRow.append(field)
                    rows.append(currentRow)
                    currentRow = []
                    state = .startOfField
                }

            case .inQuotedField:
                if b == quote {
                    state = .quoteInQuotedField
                } else {
                    quotedBytes.append(b)
                }

            case .quoteInQuotedField:
                if b == quote {
                    quotedBytes.append(quote)
                    state = .inQuotedField
                } else if b == delimByte {
                    let field = String(decoding: quotedBytes, as: UTF8.self)
                    currentRow.append(field)
                    quotedBytes.removeAll(keepingCapacity: true)
                    state = .startOfField
                } else if b == cr || b == lf {
                    let field = String(decoding: quotedBytes, as: UTF8.self)
                    if b == cr && idx + 1 < count && bytes[idx + 1] == lf {
                        idx += 1
                    }
                    currentRow.append(field)
                    rows.append(currentRow)
                    currentRow = []
                    quotedBytes.removeAll(keepingCapacity: true)
                    state = .startOfField
                } else {
                    throw EasyOfficeError.corruptedContainer(
                        detail: "CSV/TSV 따옴표 필드 종료 후 잘못된 문자가 발견되었습니다."
                    )
                }
            }

            idx += 1
        }

        switch state {
        case .inQuotedField:
            throw EasyOfficeError.corruptedContainer(detail: "Unclosed quote in CSV/TSV stream")
        case .quoteInQuotedField:
            let field = String(decoding: quotedBytes, as: UTF8.self)
            currentRow.append(field)
            rows.append(currentRow)
        case .inUnquotedField:
            let field = String(decoding: bytes[unquotedStartIdx..<count], as: UTF8.self)
            currentRow.append(field)
            rows.append(currentRow)
        case .startOfField:
            if !currentRow.isEmpty {
                currentRow.append("")
                rows.append(currentRow)
            }
        }

        return rows
    }

    private static func serializeField(_ field: String, delimiter: Character) -> String {
        let needsQuoting = field.contains(delimiter)
            || field.contains("\"")
            || field.contains("\n")
            || field.contains("\r")
        guard needsQuoting else { return field }
        let escaped = field.replacingOccurrences(of: "\"", with: "\"\"")
        return "\"\(escaped)\""
    }
}

// MARK: - CommonMark Markdown + GFM Pipe Table Codec

/// Bidirectional CommonMark Markdown and GFM pipe table converter for `OfficeDocumentModel`.
public enum CommonMarkCodec {

    public static func encodeDocument(_ doc: OfficeDocumentModel) -> Data {
        var blocks: [String] = []

        for p in doc.paragraphs {
            let formattedInline: String
            if !p.runs.isEmpty {
                formattedInline = p.runs.map {
                    formatInline(text: $0.text, isBold: $0.isBold, isItalic: $0.isItalic, isUnderlined: $0.isUnderlined)
                }.joined()
            } else {
                formattedInline = formatInline(text: p.text, isBold: p.isBold, isItalic: p.isItalic, isUnderlined: p.isUnderlined)
            }
            switch p.style {
            case .title:
                blocks.append("# \(p.text)")
            case .heading1:
                blocks.append("## \(p.text)")
            case .heading2:
                blocks.append("### \(p.text)")
            case .heading3, .officialHeader:
                blocks.append("#### \(p.text)")
            case .quote:
                blocks.append("> \(p.text)")
            case .body:
                blocks.append(formattedInline)
            }
        }

        for table in doc.tables where !table.rows.isEmpty {
            var tableLines: [String] = []
            if !table.caption.isEmpty {
                tableLines.append("<!-- caption: \(table.caption) -->")
            }
            for (rIdx, row) in table.rows.enumerated() {
                let escapedCells = row.map { $0.replacingOccurrences(of: "|", with: "\\|") }
                tableLines.append("| " + escapedCells.joined(separator: " | ") + " |")
                if rIdx == 0 {
                    let separator = Array(repeating: "---", count: row.count).joined(separator: " | ")
                    tableLines.append("| " + separator + " |")
                }
            }
            blocks.append(tableLines.joined(separator: "\n"))
        }

        for image in doc.images {
            blocks.append("![\(image.caption)](\(image.fileName))")
        }

        let extEnvelope = ExtendedDocumentMetadataEnvelope(from: doc)
        if let extBytes = try? JSONEncoder().encode(extEnvelope), !extBytes.isEmpty {
            blocks.append("<!-- eo-meta: \(extBytes.base64EncodedString()) -->")
        }

        return Data(blocks.joined(separator: "\n\n").utf8)
    }

    public static func decodeDocument(from data: Data, title: String = "Markdown 문서") throws -> OfficeDocumentModel {
        if data.contains(0x00) {
            throw EasyOfficeError.invalidUTF8Text(fileName: title)
        }
        guard let markdown = String(data: data, encoding: .utf8) else {
            throw EasyOfficeError.invalidUTF8Text(fileName: title)
        }

        var paragraphs: [ParagraphBlock] = []
        var tables: [DocumentTableBlock] = []
        var images: [EmbeddedImageAttachment] = []
        var extendedEnvelope: ExtendedDocumentMetadataEnvelope?

        let rawLines = markdown.components(separatedBy: .newlines)
        var idx = 0
        var pendingCaption = ""

        while idx < rawLines.count {
            let trimmed = rawLines[idx].trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty {
                idx += 1
                continue
            }

            if trimmed.hasPrefix("<!-- eo-meta:") && trimmed.hasSuffix("-->") {
                let start = trimmed.index(trimmed.startIndex, offsetBy: "<!-- eo-meta:".count)
                let end = trimmed.index(trimmed.endIndex, offsetBy: -3)
                let b64 = String(trimmed[start..<end]).trimmingCharacters(in: .whitespaces)
                if let metaData = Data(base64Encoded: b64) {
                    extendedEnvelope = try? JSONDecoder().decode(ExtendedDocumentMetadataEnvelope.self, from: metaData)
                }
                idx += 1
                continue
            }

            if trimmed.hasPrefix("<!-- caption:") && trimmed.hasSuffix("-->") {
                let start = trimmed.index(trimmed.startIndex, offsetBy: "<!-- caption:".count)
                let end = trimmed.index(trimmed.endIndex, offsetBy: -3)
                pendingCaption = String(trimmed[start..<end]).trimmingCharacters(in: .whitespaces)
                idx += 1
                continue
            }

            if trimmed.hasPrefix("![") && trimmed.hasSuffix(")"),
               let midRange = trimmed.range(of: "](") {
                let capStart = trimmed.index(trimmed.startIndex, offsetBy: 2)
                let caption = String(trimmed[capStart..<midRange.lowerBound])
                let fileEnd = trimmed.index(before: trimmed.endIndex)
                let fileName = String(trimmed[midRange.upperBound..<fileEnd])
                images.append(EmbeddedImageAttachment(fileName: fileName, caption: caption, imageData: Data()))
                idx += 1
                continue
            }

            if trimmed.hasPrefix("|") && trimmed.hasSuffix("|") {
                var tableRows: [[String]] = []
                while idx < rawLines.count {
                    let line = rawLines[idx].trimmingCharacters(in: .whitespaces)
                    guard line.hasPrefix("|") && line.hasSuffix("|") else { break }
                    let cells = parsePipeRow(line)
                    let isSeparator = !cells.isEmpty && cells.allSatisfy { cell in
                        let cleaned = cell.replacingOccurrences(of: "-", with: "").replacingOccurrences(of: ":", with: "")
                        return cleaned.isEmpty && cell.contains("-")
                    }
                    if !isSeparator {
                        tableRows.append(cells)
                    }
                    idx += 1
                }
                if !tableRows.isEmpty {
                    tables.append(DocumentTableBlock(caption: pendingCaption, rows: tableRows, hasHeaderRow: true))
                    pendingCaption = ""
                }
                continue
            }

            if trimmed.hasPrefix("#### ") {
                paragraphs.append(ParagraphBlock(style: .heading3, text: String(trimmed.dropFirst(5)), isBold: true))
            } else if trimmed.hasPrefix("### ") {
                paragraphs.append(ParagraphBlock(style: .heading2, text: String(trimmed.dropFirst(4)), isBold: true))
            } else if trimmed.hasPrefix("## ") {
                paragraphs.append(ParagraphBlock(style: .heading1, text: String(trimmed.dropFirst(3)), isBold: true))
            } else if trimmed.hasPrefix("# ") {
                paragraphs.append(ParagraphBlock(style: .title, text: String(trimmed.dropFirst(2)), isBold: true))
            } else if trimmed.hasPrefix("> ") {
                paragraphs.append(ParagraphBlock(style: .quote, text: String(trimmed.dropFirst(2))))
            } else {
                let parsed = parseInline(trimmed)
                paragraphs.append(ParagraphBlock(
                    style: .body,
                    text: parsed.text,
                    isBold: parsed.isBold,
                    isItalic: parsed.isItalic,
                    isUnderlined: parsed.isUnderlined
                ))
            }
            idx += 1
        }

        let resolvedTitle = paragraphs.first(where: { $0.style == .title })?.text ?? title
        var docModel = OfficeDocumentModel(
            title: resolvedTitle,
            format: .md,
            paragraphs: paragraphs,
            tables: tables,
            images: images
        )
        extendedEnvelope?.apply(to: &docModel)
        return docModel
    }

    private static func formatInline(text: String, isBold: Bool, isItalic: Bool, isUnderlined: Bool = false) -> String {
        var result = text
        if isBold && isItalic {
            result = "***\(result)***"
        } else if isBold {
            result = "**\(result)**"
        } else if isItalic {
            result = "*\(result)*"
        }
        if isUnderlined {
            result = "<u>\(result)</u>"
        }
        return result
    }

    private static func parseInline(_ line: String) -> (text: String, isBold: Bool, isItalic: Bool, isUnderlined: Bool) {
        var working = line
        var isUnderlined = false
        if working.hasPrefix("<u>") && working.hasSuffix("</u>") && working.count > 7 {
            working = String(working.dropFirst(3).dropLast(4))
            isUnderlined = true
        }
        if working.hasPrefix("***") && working.hasSuffix("***") && working.count > 6 {
            let inner = String(working.dropFirst(3).dropLast(3))
            return (inner, true, true, isUnderlined)
        }
        if working.hasPrefix("**") && working.hasSuffix("**") && working.count > 4 {
            let inner = String(working.dropFirst(2).dropLast(2))
            return (inner, true, false, isUnderlined)
        }
        if working.hasPrefix("*") && working.hasSuffix("*") && working.count > 2 {
            let inner = String(working.dropFirst(1).dropLast(1))
            return (inner, false, true, isUnderlined)
        }
        return (working, false, false, isUnderlined)
    }

    private static func parsePipeRow(_ line: String) -> [String] {
        let stripped = line.dropFirst().dropLast()
        var cells: [String] = []
        var current = ""
        var idx = stripped.startIndex

        while idx < stripped.endIndex {
            let ch = stripped[idx]
            let nextIdx = stripped.index(after: idx)
            if ch == "\\" && nextIdx < stripped.endIndex && stripped[nextIdx] == "|" {
                current.append("|")
                idx = stripped.index(after: nextIdx)
                continue
            }
            if ch == "|" {
                cells.append(current.trimmingCharacters(in: .whitespaces))
                current = ""
            } else {
                current.append(ch)
            }
            idx = nextIdx
        }
        cells.append(current.trimmingCharacters(in: .whitespaces))
        return cells
    }
}

// MARK: - Rich Text Format (RTF 1.9.1) Codec with Unicode \uN? Escapes

/// RTF 1.9.1 encoder and decoder supporting lossless Korean Unicode `\uN?` escapes, paragraphs, and tables.
public enum RTFCodec {

    public static func encodeDocument(_ doc: OfficeDocumentModel) -> Data {
        var lines: [String] = []
        lines.append("{\\rtf1\\ansi\\deff0")
        lines.append("{\\info{\\title \(escapeRTFText(doc.title))}}")

        let extEnvelope = ExtendedDocumentMetadataEnvelope(from: doc)
        if let extBytes = try? JSONEncoder().encode(extEnvelope), !extBytes.isEmpty {
            lines.append("{\\*\\eometa \(extBytes.base64EncodedString())}")
        }

        for p in doc.paragraphs {
            var controls = "\\pard"
            controls += "\\s\(styleCode(for: p.style))"
            if p.isBold { controls += "\\b" }
            if p.isItalic { controls += "\\i" }
            if p.isUnderlined { controls += "\\ul" }
            let encodedText = escapeRTFText(p.text)
            var resetControls = ""
            if p.isUnderlined { resetControls += "\\ulnone" }
            if p.isItalic { resetControls += "\\i0" }
            if p.isBold { resetControls += "\\b0" }
            lines.append("\(controls) \(encodedText)\(resetControls)\\par")
        }

        for tbl in doc.tables {
            let captionEscaped = escapeRTFText(tbl.caption)
            for row in tbl.rows {
                let cellsEncoded = row.map { "\\cellx \(escapeRTFText($0))\\cell" }.joined(separator: " ")
                lines.append("{\\trowd{\\*\\tblcaption \(captionEscaped)} \(cellsEncoded)\\row}")
            }
        }

        for img in doc.images {
            let hexPayload = img.imageData.map { String(format: "%02x", $0) }.joined()
            lines.append("{\\*\\shppict{\\pict\\pngblip \(hexPayload)}}")
        }

        lines.append("}")
        return Data(lines.joined(separator: "\n").utf8)
    }

    public static func decodeDocument(from data: Data) throws -> OfficeDocumentModel {
        let signature = try FormatDetector.verify(data: data, fileName: "document.rtf")
        guard signature.format == .rtf else {
            throw EasyOfficeError.signatureMismatch(expected: .rtfText, fileName: "document.rtf")
        }
        guard let rawRTF = String(data: data, encoding: .utf8) else {
            throw EasyOfficeError.invalidUTF8Text(fileName: "document.rtf")
        }

        var title = "RTF 문서"
        var paragraphs: [ParagraphBlock] = []
        var tablesByCaption: [(caption: String, rows: [[String]])] = []
        var extendedEnvelope: ExtendedDocumentMetadataEnvelope?

        let lines = rawRTF.components(separatedBy: .newlines)
        for rawLine in lines {
            let line = rawLine.trimmingCharacters(in: .whitespaces)
            if line.hasPrefix("{\\info{\\title ") && line.hasSuffix("}}") {
                let start = line.index(line.startIndex, offsetBy: "{\\info{\\title ".count)
                let end = line.index(line.endIndex, offsetBy: -2)
                title = unescapeRTFText(String(line[start..<end]))
            } else if line.hasPrefix("{\\*\\eometa ") && line.hasSuffix("}") {
                let start = line.index(line.startIndex, offsetBy: "{\\*\\eometa ".count)
                let end = line.index(before: line.endIndex)
                let b64 = String(line[start..<end]).trimmingCharacters(in: .whitespaces)
                if let metaData = Data(base64Encoded: b64) {
                    extendedEnvelope = try? JSONDecoder().decode(ExtendedDocumentMetadataEnvelope.self, from: metaData)
                }
            } else if line.hasPrefix("\\pard") && line.hasSuffix("\\par") {
                if let parsed = parseParagraphLine(line) {
                    paragraphs.append(parsed)
                }
            } else if line.hasPrefix("{\\trowd") && line.hasSuffix("\\row}") {
                let (caption, rowCells) = parseTableRowLine(line)
                if let lastIdx = tablesByCaption.indices.last, tablesByCaption[lastIdx].caption == caption {
                    tablesByCaption[lastIdx].rows.append(rowCells)
                } else {
                    tablesByCaption.append((caption: caption, rows: [rowCells]))
                }
            }
        }

        let tables = tablesByCaption.map { DocumentTableBlock(caption: $0.caption, rows: $0.rows, hasHeaderRow: true) }
        var docModel = OfficeDocumentModel(
            title: title,
            format: .rtf,
            paragraphs: paragraphs,
            tables: tables
        )
        extendedEnvelope?.apply(to: &docModel)
        return docModel
    }

    public static func escapeRTFText(_ input: String) -> String {
        var output = ""
        output.reserveCapacity(input.utf16.count * 2)
        for unit in input.utf16 {
            switch unit {
            case 0x5C: // '\'
                output.append("\\\\")
            case 0x7B: // '{'
                output.append("\\{")
            case 0x7D: // '}'
                output.append("\\}")
            case 0x0A:
                output.append("\\line ")
            case 0x20...0x7E:
                if let scalar = UnicodeScalar(unit) {
                    output.unicodeScalars.append(scalar)
                }
            default:
                let signedUnit = Int16(bitPattern: unit)
                output.append("\\u\(signedUnit)?")
            }
        }
        return output
    }

    /// Unescapes RTF text directly over UTF-16 code units (`[UInt16]`), eliminating per-character `String` allocations.
    public static func unescapeRTFText(_ input: String) -> String {
        let units = Array(input.utf16)
        guard !units.isEmpty else { return "" }

        var utf16Units: [UInt16] = []
        utf16Units.reserveCapacity(units.count)
        let count = units.count
        var idx = 0

        while idx < count {
            let u = units[idx]
            if u == 0x5C && idx + 1 < count { // '\'
                let next = units[idx + 1]
                // Escaped '\', '{', '}'
                if next == 0x5C || next == 0x7B || next == 0x7D {
                    utf16Units.append(next)
                    idx += 2
                    continue
                }
                // '\line' -> [0x5C, 0x6C, 0x69, 0x6E, 0x65]
                if idx + 5 <= count &&
                    units[idx + 1] == 0x6C &&
                    units[idx + 2] == 0x69 &&
                    units[idx + 3] == 0x6E &&
                    units[idx + 4] == 0x65 {
                    utf16Units.append(0x0A)
                    idx += 5
                    if idx < count && units[idx] == 0x20 {
                        idx += 1
                    }
                    continue
                }
                // '\uN?'
                if next == 0x75 { // 'u'
                    var numStart = idx + 2
                    var isNegative = false
                    if numStart < count && units[numStart] == 0x2D { // '-'
                        isNegative = true
                        numStart += 1
                    }
                    var numEnd = numStart
                    var rawVal: Int32 = 0
                    while numEnd < count && units[numEnd] >= 0x30 && units[numEnd] <= 0x39 {
                        rawVal = rawVal &* 10 &+ Int32(units[numEnd] - 0x30)
                        numEnd += 1
                    }
                    if numEnd > numStart {
                        let signedVal = isNegative ? (0 &- rawVal) : rawVal
                        let codeUnit = UInt16(bitPattern: Int16(truncatingIfNeeded: signedVal))
                        utf16Units.append(codeUnit)
                        idx = numEnd
                        if idx < count && units[idx] == 0x3F { // '?'
                            idx += 1
                        }
                        continue
                    }
                }
            }
            utf16Units.append(u)
            idx += 1
        }

        return String(decoding: utf16Units, as: UTF16.self)
    }

    private static func styleCode(for role: ParagraphStyleRole) -> Int {
        switch role {
        case .title: return 1
        case .heading1: return 2
        case .heading2: return 3
        case .heading3: return 4
        case .body: return 0
        case .quote: return 5
        case .officialHeader: return 6
        }
    }

    private static func styleRole(for code: Int) -> ParagraphStyleRole {
        switch code {
        case 1: return .title
        case 2: return .heading1
        case 3: return .heading2
        case 4: return .heading3
        case 5: return .quote
        case 6: return .officialHeader
        default: return .body
        }
    }

    private static func parseParagraphLine(_ line: String) -> ParagraphBlock? {
        guard let spaceIdx = line.firstIndex(of: " ") else { return nil }
        let controlPrefix = String(line[..<spaceIdx])
        var contentPart = String(line[line.index(after: spaceIdx)...])
        if contentPart.hasSuffix("\\par") {
            contentPart.removeLast(4)
        }

        let isBold = controlPrefix.contains("\\b")
        let isItalic = controlPrefix.contains("\\i")
        let isUnderlined = controlPrefix.contains("\\ul")

        for suffix in ["\\b0", "\\i0", "\\ulnone"] {
            if contentPart.hasSuffix(suffix) {
                contentPart.removeLast(suffix.count)
            }
        }

        var role: ParagraphStyleRole = .body
        if let sRange = controlPrefix.range(of: "\\s") {
            let afterS = controlPrefix[sRange.upperBound...]
            let digits = afterS.prefix(while: { $0.isNumber })
            if let code = Int(digits) {
                role = styleRole(for: code)
            }
        }

        return ParagraphBlock(
            style: role,
            text: unescapeRTFText(contentPart),
            isBold: isBold,
            isItalic: isItalic,
            isUnderlined: isUnderlined
        )
    }

    private static func parseTableRowLine(_ line: String) -> (caption: String, cells: [String]) {
        var caption = ""
        if let capRange = line.range(of: "{\\*\\tblcaption ") {
            let afterCap = line[capRange.upperBound...]
            if let closeBrace = afterCap.firstIndex(of: "}") {
                caption = unescapeRTFText(String(afterCap[..<closeBrace]))
            }
        }

        var cells: [String] = []
        let segments = line.components(separatedBy: "\\cellx ")
        for seg in segments.dropFirst() {
            if let cellEnd = seg.range(of: "\\cell") {
                let rawCell = String(seg[..<cellEnd.lowerBound])
                cells.append(unescapeRTFText(rawCell))
            }
        }
        return (caption, cells)
    }
}
