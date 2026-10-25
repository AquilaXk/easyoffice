import Foundation

// MARK: - A. Bidirectional Format Conversion Studio

/// Single format conversion request across any supported office module.
public struct ConversionJobRequest: Sendable, Equatable, Identifiable {
  public let id: UUID
  public var sourceFileName: String
  public var sourceFormat: DocumentFormat
  public var targetFormat: DocumentFormat
  public var payload: OpenTabPayload

  public init(
    id: UUID = UUID(),
    sourceFileName: String,
    sourceFormat: DocumentFormat,
    targetFormat: DocumentFormat,
    payload: OpenTabPayload
  ) {
    self.id = id
    self.sourceFileName = sourceFileName
    self.sourceFormat = sourceFormat
    self.targetFormat = targetFormat
    self.payload = payload
  }
}

/// Verified result of a single format conversion job.
public struct ConversionJobResult: Sendable, Equatable, Identifiable {
  public let id: UUID
  public let sourceFileName: String
  public let outputFileName: String
  public let sourceFormat: DocumentFormat
  public let targetFormat: DocumentFormat
  public let outputBytes: Data
  public let convertedPayload: OpenTabPayload
  public let verifiedSignature: VerifiedDocumentSignature
  public let outputFileURL: URL?

  public init(
    id: UUID,
    sourceFileName: String,
    outputFileName: String,
    sourceFormat: DocumentFormat,
    targetFormat: DocumentFormat,
    outputBytes: Data,
    convertedPayload: OpenTabPayload,
    verifiedSignature: VerifiedDocumentSignature,
    outputFileURL: URL? = nil
  ) {
    self.id = id
    self.sourceFileName = sourceFileName
    self.outputFileName = outputFileName
    self.sourceFormat = sourceFormat
    self.targetFormat = targetFormat
    self.outputBytes = outputBytes
    self.convertedPayload = convertedPayload
    self.verifiedSignature = verifiedSignature
    self.outputFileURL = outputFileURL
  }
}

/// Summary report returned after executing a batch of format conversion jobs.
public struct BatchConversionReport: Sendable {
  public let totalJobs: Int
  public let succeededResults: [ConversionJobResult]
  public let failedErrors: [(jobID: UUID, errorDescription: String)]

  public init(
    totalJobs: Int,
    succeededResults: [ConversionJobResult],
    failedErrors: [(jobID: UUID, errorDescription: String)]
  ) {
    self.totalJobs = totalJobs
    self.succeededResults = succeededResults
    self.failedErrors = failedErrors
  }
}

/// Bidirectional format conversion studio orchestrating document, sheet, slide, HWP/HWPX, and PDF codecs with Fail-Closed signature verification.
public enum DocumentConversionOrchestrator {

  public static func convert(_ request: ConversionJobRequest) throws -> ConversionJobResult {
    if request.sourceFileName.utf8.contains(0x00) {
      throw EasyOfficeError.securityViolation(detail: "변환 파일명에 NUL 바이트가 포함되어 있습니다.")
    }
    let trimmedSourceName = request.sourceFileName.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedSourceName.isEmpty else {
      throw EasyOfficeError.unsupportedExtension(request.sourceFileName)
    }
    if trimmedSourceName.contains("/")
      || trimmedSourceName.contains("\\")
      || trimmedSourceName.contains("..")
    {
      throw EasyOfficeError.securityViolation(
        detail: "변환 원본 파일명에 경로 구분자 또는 상위 디렉터리 참조가 포함되어 있습니다: \(request.sourceFileName)")
    }

    let baseName: String
    if let dotIdx = trimmedSourceName.lastIndex(of: "."), dotIdx > trimmedSourceName.startIndex {
      baseName = String(trimmedSourceName[..<dotIdx])
    } else {
      baseName = trimmedSourceName
    }
    guard !baseName.isEmpty, baseName != "." else {
      throw EasyOfficeError.unsupportedExtension(request.sourceFileName)
    }
    let outputFileName = "\(baseName).\(request.targetFormat.fileExtension)"

    let convertedPayload: OpenTabPayload
    let outputBytes: Data

    switch request.targetFormat {
    case .docx, .doc, .odt, .rtf, .md, .txt, .hwp, .hwpx, .hwt, .hwtx:
      var docModel = try convertPayloadToDocument(request.payload, targetFormat: request.targetFormat)
      if request.sourceFormat != request.targetFormat {
        docModel.preservedArchiveEntries = [:]
      }
      convertedPayload = .document(docModel)
      outputBytes = try encodeDocumentModel(docModel, as: request.targetFormat)

    case .xlsx, .xls, .ods, .csv, .tsv:
      var sheetModel = try convertPayloadToWorksheet(request.payload)
      if request.sourceFormat != request.targetFormat {
        sheetModel.preservedArchiveEntries = [:]
      }
      convertedPayload = .sheet(sheetModel)
      outputBytes = try encodeWorksheetModel(sheetModel, as: request.targetFormat)

    case .pptx, .ppt, .ppsx, .odp:
      var deckModel = try convertPayloadToPresentation(request.payload)
      if request.sourceFormat != request.targetFormat {
        deckModel.preservedArchiveEntries = [:]
      }
      convertedPayload = .slide(deckModel)
      outputBytes = try encodePresentationModel(deckModel, as: request.targetFormat)

    case .pdf:
      let pdfModel = convertPayloadToPDF(request.payload)
      convertedPayload = .pdf(pdfModel)
      outputBytes = PDFBinarySerializer.encode(pdfModel)
    }

    let verifiedSignature = try FormatDetector.verify(
      data: outputBytes,
      fileName: outputFileName
    )

    return ConversionJobResult(
      id: request.id,
      sourceFileName: request.sourceFileName,
      outputFileName: outputFileName,
      sourceFormat: request.sourceFormat,
      targetFormat: request.targetFormat,
      outputBytes: outputBytes,
      convertedPayload: convertedPayload,
      verifiedSignature: verifiedSignature
    )
  }

  public static func executeBatch(
    _ jobs: [ConversionJobRequest],
    outputDirectory: URL? = nil
  ) -> BatchConversionReport {
    var succeeded: [ConversionJobResult] = []
    var failed: [(jobID: UUID, errorDescription: String)] = []
    let targetDir =
      outputDirectory
      ?? FileManager.default.temporaryDirectory.appendingPathComponent(
        "EasyOfficeBatchOutput", isDirectory: true)
    try? FileManager.default.createDirectory(at: targetDir, withIntermediateDirectories: true)

    for job in jobs {
      do {
        let converted = try convert(job)
        let fileURL = targetDir.appendingPathComponent(converted.outputFileName)
        var writtenURL: URL? = nil
        if (try? converted.outputBytes.write(to: fileURL, options: .atomic)) != nil {
          writtenURL = fileURL
        }
        let result = ConversionJobResult(
          id: converted.id,
          sourceFileName: converted.sourceFileName,
          outputFileName: converted.outputFileName,
          sourceFormat: converted.sourceFormat,
          targetFormat: converted.targetFormat,
          outputBytes: converted.outputBytes,
          convertedPayload: converted.convertedPayload,
          verifiedSignature: converted.verifiedSignature,
          outputFileURL: writtenURL
        )
        succeeded.append(result)
      } catch {
        let desc = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
        failed.append((jobID: job.id, errorDescription: desc))
      }
    }

    return BatchConversionReport(
      totalJobs: jobs.count,
      succeededResults: succeeded,
      failedErrors: failed
    )
  }

  // MARK: Cross-Domain Model Adapters

  private static func convertPayloadToDocument(
    _ payload: OpenTabPayload,
    targetFormat: DocumentFormat
  ) throws -> OfficeDocumentModel {
    switch payload {
    case .document(var doc):
      if doc.format != targetFormat {
        doc.preservedArchiveEntries = [:]
      }
      doc.format = targetFormat
      return doc

    case .pdf(let pdf):
      return pdf.convertToEditableDocument(targetFormat: targetFormat)

    case .sheet(let sheet):
      let rows = try extractGridRows(from: sheet)
      var tables: [DocumentTableBlock] = []
      if !rows.isEmpty {
        tables.append(DocumentTableBlock(caption: sheet.name, rows: rows, hasHeaderRow: true))
      }
      return OfficeDocumentModel(
        title: sheet.name,
        format: targetFormat,
        paragraphs: [ParagraphBlock(style: .heading1, text: sheet.name, isBold: true)],
        tables: tables
      )

    case .slide(let deck):
      var paragraphs: [ParagraphBlock] = []
      for slide in deck.slides {
        paragraphs.append(ParagraphBlock(style: .heading1, text: slide.title, isBold: true))
        if !slide.subtitle.isEmpty {
          paragraphs.append(ParagraphBlock(style: .heading2, text: slide.subtitle))
        }
        for bullet in slide.bulletPoints {
          paragraphs.append(ParagraphBlock(style: .body, text: bullet))
        }
        for element in slide.elements {
          if !element.title.isEmpty {
            paragraphs.append(ParagraphBlock(style: .heading3, text: element.title, isBold: true))
          }
          if !element.bodyText.isEmpty {
            paragraphs.append(ParagraphBlock(style: .body, text: element.bodyText))
          }
        }
        if !slide.speakerNotes.isEmpty {
          paragraphs.append(ParagraphBlock(style: .quote, text: slide.speakerNotes))
        }
      }
      return OfficeDocumentModel(
        title: deck.title,
        format: targetFormat,
        paragraphs: paragraphs
      )
    }
  }

  private static func convertPayloadToWorksheet(_ payload: OpenTabPayload) throws -> WorksheetModel
  {
    switch payload {
    case .sheet(let sheet):
      return sheet

    case .document(let doc):
      var sheet = WorksheetModel(name: doc.title)
      if let firstTable = doc.tables.first, !firstTable.rows.isEmpty {
        for (rIdx, row) in firstTable.rows.enumerated() {
          for (cIdx, cellText) in row.enumerated() where !cellText.isEmpty {
            let addr = try CellAddress(column: cIdx, row: rIdx)
            try sheet.setCell(addr.a1Notation, rawValue: cellText)
          }
        }
      } else {
        for (rIdx, p) in doc.paragraphs.enumerated() where !p.text.isEmpty {
          let addr = try CellAddress(column: 0, row: rIdx)
          try sheet.setCell(addr.a1Notation, rawValue: p.text)
        }
      }
      return sheet

    case .pdf(let pdf):
      var sheet = WorksheetModel(name: pdf.title)
      var currentRow = 0
      for page in pdf.pages {
        let lines = page.extractedText.components(separatedBy: .newlines)
        for line in lines where !line.isEmpty {
          let columns = line.components(separatedBy: "\t")
          for (cIdx, colText) in columns.enumerated() where !colText.isEmpty {
            let addr = try CellAddress(column: cIdx, row: currentRow)
            try sheet.setCell(addr.a1Notation, rawValue: colText)
          }
          currentRow += 1
        }
      }
      return sheet

    case .slide(let deck):
      var sheet = WorksheetModel(name: deck.title)
      for (rIdx, slide) in deck.slides.enumerated() {
        let titleAddr = try CellAddress(column: 0, row: rIdx)
        try sheet.setCell(titleAddr.a1Notation, rawValue: slide.title)
        let bodyText = slide.bulletPoints.joined(separator: "; ")
        if !bodyText.isEmpty {
          let bodyAddr = try CellAddress(column: 1, row: rIdx)
          try sheet.setCell(bodyAddr.a1Notation, rawValue: bodyText)
        }
      }
      return sheet
    }
  }

  private static func convertPayloadToPresentation(_ payload: OpenTabPayload) throws
    -> PresentationDeckModel
  {
    switch payload {
    case .slide(let deck):
      return deck

    case .pdf(let pdf):
      return pdf.convertToPresentation()

    case .document(let doc):
      let slides: [SlideModel]
      if doc.paragraphs.isEmpty {
        slides = [SlideModel(title: doc.title, masterLayout: .titleSlide)]
      } else {
        let bullets = doc.paragraphs.dropFirst().map(\.text)
        slides = [
          SlideModel(
            title: doc.paragraphs.first?.text ?? doc.title,
            subtitle: doc.headerText,
            bulletPoints: bullets,
            masterLayout: bullets.isEmpty ? .titleSlide : .titleAndContent
          )
        ]
      }
      return PresentationDeckModel(title: doc.title, slides: slides)

    case .sheet(let sheet):
      let rows = try extractGridRows(from: sheet)
      let bullets = rows.map { $0.joined(separator: " | ") }
      return PresentationDeckModel(
        title: sheet.name,
        slides: [
          SlideModel(
            title: sheet.name,
            bulletPoints: bullets,
            masterLayout: .titleAndContent
          )
        ]
      )
    }
  }

  private static func convertPayloadToPDF(_ payload: OpenTabPayload) -> PDFStudioDocumentModel {
    switch payload {
    case .pdf(let pdf):
      return pdf
    case .document(let doc):
      return PDFStudioDocumentModel.fromDocument(doc)
    case .sheet(let sheet):
      return PDFStudioDocumentModel.fromWorksheet(sheet)
    case .slide(let deck):
      return PDFStudioDocumentModel.fromPresentation(deck)
    }
  }

  private static func extractGridRows(from sheet: WorksheetModel) throws -> [[String]] {
    var maxRow = -1
    var maxCol = -1
    for (coord, raw) in sheet.cellsByCoordinate where !raw.isEmpty {
      if coord.row >= 0, coord.row <= CellAddress.maxZeroBasedRow,
        coord.column >= 0, coord.column <= CellAddress.maxZeroBasedColumn
      {
        maxRow = max(maxRow, coord.row)
        maxCol = max(maxCol, coord.column)
      }
    }
    guard maxRow >= 0, maxCol >= 0 else { return [] }
    let (denseCells, overflow) = (maxRow + 1).multipliedReportingOverflow(by: maxCol + 1)
    guard !overflow, denseCells <= CSVStateMachineCodec.maxDenseExportCells else {
      throw EasyOfficeError.resourceExhausted(
        limitName: "conversionMaxDenseGridCells",
        limitValue: CSVStateMachineCodec.maxDenseExportCells,
        actualValue: overflow ? Int.max : denseCells
      )
    }
    var rows: [[String]] = []
    rows.reserveCapacity(maxRow + 1)
    for r in 0...maxRow {
      var rowCells: [String] = []
      rowCells.reserveCapacity(maxCol + 1)
      for c in 0...maxCol {
        let packed = PackedCellCoordinate(row: r, column: c)
        rowCells.append(sheet.evaluate(at: packed).description)
      }
      rows.append(rowCells)
    }
    return rows
  }

  // MARK: Format Encoders

  private static func encodeDocumentModel(_ doc: OfficeDocumentModel, as format: DocumentFormat)
    throws -> Data
  {
    switch format {
    case .docx:
      return try OOXMLContainerCodec.encodeDocument(doc)
    case .odt:
      return try ODFContainerCodec.encodeDocument(doc)
    case .hwpx, .hwtx:
      return try HWPXContainerCodec.encodeDocument(doc)
    case .hwp, .hwt:
      return try HWP5BinaryCodec.encodeDocument(doc)
    case .doc:
      return try LegacyOLE2OfficeCodec.encodeWordBinary(doc)
    case .rtf:
      return RTFCodec.encodeDocument(doc)
    case .md:
      return CommonMarkCodec.encodeDocument(doc)
    case .txt:
      var lines: [String] = []
      if !doc.title.isEmpty {
        lines.append(doc.title)
      }
      for p in doc.paragraphs where !p.text.isEmpty {
        lines.append(p.text)
      }
      for tbl in doc.tables {
        if !tbl.caption.isEmpty {
          lines.append(tbl.caption)
        }
        for row in tbl.rows {
          lines.append(row.joined(separator: "\t"))
        }
      }
      return Data(lines.joined(separator: "\n").utf8)
    default:
      throw EasyOfficeError.unsupportedExtension(format.rawValue)
    }
  }

  private static func encodeWorksheetModel(_ sheet: WorksheetModel, as format: DocumentFormat)
    throws -> Data
  {
    switch format {
    case .xlsx:
      return try OOXMLContainerCodec.encodeWorksheet(sheet)
    case .ods:
      return try ODFContainerCodec.encodeWorksheet(sheet)
    case .csv:
      return try CSVStateMachineCodec.encodeWorksheet(sheet, delimiter: ",")
    case .tsv:
      return try CSVStateMachineCodec.encodeWorksheet(sheet, delimiter: "\t")
    case .xls:
      return try LegacyOLE2OfficeCodec.encodeBIFF8Worksheet(sheet)
    default:
      throw EasyOfficeError.unsupportedExtension(format.rawValue)
    }
  }

  private static func encodePresentationModel(
    _ deck: PresentationDeckModel, as format: DocumentFormat
  ) throws -> Data {
    switch format {
    case .pptx, .ppsx:
      return try OOXMLContainerCodec.encodePresentation(deck)
    case .odp:
      return try ODFContainerCodec.encodePresentation(deck)
    case .ppt:
      return try LegacyOLE2OfficeCodec.encodePowerPointBinary(deck)
    default:
      throw EasyOfficeError.unsupportedExtension(format.rawValue)
    }
  }
}

// MARK: - B. Container Security & Privacy Auditor

/// Comprehensive container security & privacy inspection report.
public struct SecurityAuditReport: Sendable, Codable, Equatable {
  public let fileName: String
  public let detectedFormat: DocumentFormat?
  public let isSignatureValid: Bool
  public let isSafeFromBombAndTraversal: Bool
  public let isSafeFromXXE: Bool
  public let isUnencrypted: Bool
  public let issues: [String]
  public let isPassed: Bool

  public init(
    fileName: String,
    detectedFormat: DocumentFormat?,
    isSignatureValid: Bool,
    isSafeFromBombAndTraversal: Bool,
    isSafeFromXXE: Bool,
    isUnencrypted: Bool,
    issues: [String],
    isPassed: Bool
  ) {
    self.fileName = fileName
    self.detectedFormat = detectedFormat
    self.isSignatureValid = isSignatureValid
    self.isSafeFromBombAndTraversal = isSafeFromBombAndTraversal
    self.isSafeFromXXE = isSafeFromXXE
    self.isUnencrypted = isUnencrypted
    self.issues = issues
    self.isPassed = isPassed
  }
}

/// Container security auditor and document metadata/PII scrubber.
public enum DocumentSecurityAuditor {

  private static let sensitivePatternRules: [(label: String, regex: NSRegularExpression, mask: String)] = {
    let rawRules: [(String, String, String)] = [
      (
        "주민등록번호",
        #"(?<![0-9])\d{6}[-–]\s*[1-8]\d{6}(?![0-9])|(?<![0-9])\d{2}(?:0[1-9]|1[0-2])(?:0[1-9]|[12]\d|3[01])[1-8]\d{6}(?![0-9])"#,
        "[주민등록번호 비식별]"
      ),
      ("운전면허번호", #"(?<![0-9])\d{2}-\d{2}-\d{6}-\d{2}(?![0-9])"#, "[운전면허 비식별]"),
      ("여권번호", #"(?<![A-Za-z0-9])(?:[MSRGD]\d{8}|[A-Z]{2}\d{7})(?![A-Za-z0-9])"#, "[여권번호 비식별]"),
      ("신용카드번호", #"(?<![0-9])(?:\d{4}[-\s]){3}\d{4}(?![0-9])"#, "[카드번호 비식별]"),
      (
        "전화번호",
        #"(?<![0-9])01[016789][-.\s]?\d{3,4}[-.\s]?\d{4}(?![0-9])|(?<![0-9])0\d{1,2}[-.\s]\d{3,4}[-.\s]\d{4}(?![0-9])"#,
        "[연락처 비식별]"
      ),
      (
        "이메일",
        #"(?<![A-Za-z0-9._%+-])[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}(?![A-Za-z0-9])"#,
        "[이메일 비식별]"
      ),
      ("사업자/법인등록번호", #"(?<![0-9])\d{3}-\d{2}-\d{5}(?![0-9])"#, "[사업자번호 비식별]"),
      (
        "IPv4주소",
        #"(?<![0-9])(?:(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.){3}(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(?![0-9])"#,
        "[IP 비식별]"
      ),
      (
        "API토큰",
        #"(?<![A-Za-z0-9_-])(?:sk-[A-Za-z0-9_-]{16,}|eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+)(?![A-Za-z0-9_-])"#,
        "[인증토큰 비식별]"
      ),
    ]
    return rawRules.compactMap { label, pattern, mask in
      guard let regex = try? NSRegularExpression(pattern: pattern, options: []) else { return nil }
      return (label, regex, mask)
    }
  }()

  public static func detectSensitivePatterns(in text: String) -> [String] {
    guard !text.isEmpty else { return [] }
    let nsRange = NSRange(text.startIndex..<text.endIndex, in: text)
    var findings: [String] = []
    for rule in sensitivePatternRules {
      let matches = rule.regex.matches(in: text, options: [], range: nsRange)
      if !matches.isEmpty {
        findings.append("\(rule.label) (\(matches.count)건)")
      }
    }
    return findings
  }

  public static func maskSensitivePatterns(in text: String) -> (masked: String, matchCount: Int) {
    guard !text.isEmpty else { return (text, 0) }
    var current = text
    var totalMatches = 0
    for rule in sensitivePatternRules {
      let nsRange = NSRange(current.startIndex..<current.endIndex, in: current)
      let count = rule.regex.numberOfMatches(in: current, options: [], range: nsRange)
      if count > 0 {
        totalMatches += count
        current = rule.regex.stringByReplacingMatches(
          in: current, options: [], range: nsRange, withTemplate: rule.mask)
      }
    }
    return (current, totalMatches)
  }

  /// Scrubs header/footer PII markers, anonymizes revision authors to `"Redacted"`, removes all review comments, and masks sensitive PII patterns in body paragraphs and tables.
  public static func scrubMetadata(
    from document: OfficeDocumentModel
  ) -> (scrubbed: OfficeDocumentModel, removedItemCount: Int) {
    var copy = document
    var removedCount = 0

    if !copy.headerText.isEmpty {
      copy.headerText = ""
      removedCount += 1
    }
    if !copy.footerText.isEmpty {
      copy.footerText = ""
      removedCount += 1
    }

    for idx in copy.revisions.indices {
      if copy.revisions[idx].author != "Redacted" {
        copy.revisions[idx].author = "Redacted"
        removedCount += 1
      }
    }

    if !copy.comments.isEmpty {
      removedCount += copy.comments.count
      copy.comments.removeAll()
    }

    for pIdx in copy.paragraphs.indices {
      let (maskedText, count) = maskSensitivePatterns(in: copy.paragraphs[pIdx].text)
      if count > 0 {
        copy.paragraphs[pIdx].text = maskedText
        removedCount += count
      }
      for rIdx in copy.paragraphs[pIdx].runs.indices {
        let (maskedRun, rCount) = maskSensitivePatterns(in: copy.paragraphs[pIdx].runs[rIdx].text)
        if rCount > 0 {
          copy.paragraphs[pIdx].runs[rIdx].text = maskedRun
        }
      }
    }

    for tIdx in copy.tables.indices {
      for rIdx in copy.tables[tIdx].rows.indices {
        for cIdx in copy.tables[tIdx].rows[rIdx].indices {
          let (maskedCell, count) = maskSensitivePatterns(in: copy.tables[tIdx].rows[rIdx][cIdx])
          if count > 0 {
            copy.tables[tIdx].rows[rIdx][cIdx] = maskedCell
            removedCount += count
          }
        }
      }
    }

    let (scrubbedPreserved, preservedRemoved) = scrubPreservedArchiveEntries(
      copy.preservedArchiveEntries)
    copy.preservedArchiveEntries = scrubbedPreserved
    removedCount += preservedRemoved

    return (copy, removedCount)
  }

  private static func scrubPreservedArchiveEntries(
    _ entries: [String: Data]
  ) -> (scrubbed: [String: Data], removedCount: Int) {
    guard !entries.isEmpty else { return (entries, 0) }
    var out: [String: Data] = [:]
    var removedCount = 0
    for (path, data) in entries {
      if XMLDocumentModelMapper.isBinaryMediaPath(path) {
        out[path] = data
        continue
      }
      let lower = path.lowercased()
      if lower.contains("comment") || lower.contains("notesslide")
        || lower.contains("revision") || lower.contains("people.xml")
        || lower.hasSuffix("meta.xml") || lower.hasSuffix("core.xml")
        || lower.hasSuffix("custom.xml") || lower.hasSuffix("app.xml")
        || lower.hasSuffix("settings.xml") || lower.contains("header")
        || lower.contains("footer") || lower.contains("thumbnail")
        || lower.hasPrefix("preview/") || lower.contains("vbaproject")
      {
        removedCount += 1
        continue
      }
      if let xmlText = String(data: data, encoding: .utf8) {
        let (maskedText, count) = maskSensitivePatterns(in: xmlText)
        removedCount += count
        out[path] = Data(maskedText.utf8)
      } else {
        out[path] = data
      }
    }
    return (out, removedCount)
  }

  public static func scrubPayload(
    _ payload: OpenTabPayload
  ) -> (scrubbed: OpenTabPayload, removedItemCount: Int) {
    switch payload {
    case .document(let doc):
      let (scrubbed, count) = scrubMetadata(from: doc)
      return (.document(scrubbed), count)

    case .sheet(var sheet):
      var removedCount = sheet.sheetComments.count
      sheet.sheetComments.removeAll()
      for coord in Array(sheet.cellsByCoordinate.keys) {
        if let raw = sheet.cellsByCoordinate[coord], !raw.hasPrefix("=") {
          let (masked, count) = maskSensitivePatterns(in: raw)
          if count > 0 {
            sheet.cellsByCoordinate[coord] = masked
            removedCount += count
          }
        }
      }
      for sIdx in sheet.additionalSheets.indices {
        removedCount += sheet.additionalSheets[sIdx].sheetComments.count
        sheet.additionalSheets[sIdx].sheetComments.removeAll()
        for coord in Array(sheet.additionalSheets[sIdx].cellsByCoordinate.keys) {
          if let raw = sheet.additionalSheets[sIdx].cellsByCoordinate[coord], !raw.hasPrefix("=") {
            let (masked, count) = maskSensitivePatterns(in: raw)
            if count > 0 {
              sheet.additionalSheets[sIdx].cellsByCoordinate[coord] = masked
              removedCount += count
            }
          }
        }
      }
      let (scrubbedSheetPreserved, sheetPreservedRemoved) = scrubPreservedArchiveEntries(
        sheet.preservedArchiveEntries)
      sheet.preservedArchiveEntries = scrubbedSheetPreserved
      removedCount += sheetPreservedRemoved
      return (.sheet(sheet), removedCount)

    case .slide(var deck):
      var removedCount = 0
      for sIdx in deck.slides.indices {
        if !deck.slides[sIdx].speakerNotes.isEmpty {
          deck.slides[sIdx].speakerNotes = ""
          removedCount += 1
        }
        let (mTitle, c1) = maskSensitivePatterns(in: deck.slides[sIdx].title)
        if c1 > 0 {
          deck.slides[sIdx].title = mTitle
          removedCount += c1
        }
        let (mSub, c2) = maskSensitivePatterns(in: deck.slides[sIdx].subtitle)
        if c2 > 0 {
          deck.slides[sIdx].subtitle = mSub
          removedCount += c2
        }
        for bIdx in deck.slides[sIdx].bulletPoints.indices {
          let (mBullet, c3) = maskSensitivePatterns(in: deck.slides[sIdx].bulletPoints[bIdx])
          if c3 > 0 {
            deck.slides[sIdx].bulletPoints[bIdx] = mBullet
            removedCount += c3
          }
        }
        for eIdx in deck.slides[sIdx].elements.indices {
          let (mBody, c4) = maskSensitivePatterns(in: deck.slides[sIdx].elements[eIdx].bodyText)
          if c4 > 0 {
            deck.slides[sIdx].elements[eIdx].bodyText = mBody
            removedCount += c4
          }
        }
      }
      let (scrubbedSlidePreserved, slidePreservedRemoved) = scrubPreservedArchiveEntries(
        deck.preservedArchiveEntries)
      deck.preservedArchiveEntries = scrubbedSlidePreserved
      removedCount += slidePreservedRemoved
      return (.slide(deck), removedCount)

    case .pdf(var pdf):
      var removedCount = 0
      for aIdx in pdf.annotations.indices {
        if pdf.annotations[aIdx].author != "Redacted" {
          pdf.annotations[aIdx].author = "Redacted"
          removedCount += 1
        }
        let (mContent, c0) = maskSensitivePatterns(in: pdf.annotations[aIdx].content)
        if c0 > 0 {
          pdf.annotations[aIdx].content = mContent
          removedCount += c0
        }
      }
      for pIdx in pdf.pages.indices {
        let (maskedPage, count) = maskSensitivePatterns(in: pdf.pages[pIdx].extractedText)
        if count > 0 {
          pdf.pages[pIdx].extractedText = maskedPage
          removedCount += count
        }
      }
      for fIdx in pdf.formFields.indices {
        let (maskedVal, count) = maskSensitivePatterns(in: pdf.formFields[fIdx].value)
        if count > 0 {
          pdf.formFields[fIdx].value = maskedVal
          removedCount += count
        }
      }
      return (.pdf(pdf), removedCount)
    }
  }

  /// Audits a raw document container against magic-byte spoofing, ZIP decompression bombs, path traversal, XML external entities (XXE), and unsupported encryption.
  public static func auditContainer(data: Data, fileName: String) -> SecurityAuditReport {
    let detectedFormat = DocumentFormat.from(fileNameOrExtension: fileName)
    var isSignatureValid = true
    var isSafeFromBombAndTraversal = true
    var isSafeFromXXE = true
    var isUnencrypted = true
    var issues: [String] = []

    let verified: VerifiedDocumentSignature
    do {
      verified = try FormatDetector.verify(data: data, fileName: fileName)
    } catch {
      isSignatureValid = false
      if let officeError = error as? EasyOfficeError, case .corruptedContainer = officeError {
        isSafeFromBombAndTraversal = false
      }
      if (try? XMLSanitizer.validateSafeXMLBytes(data)) == nil {
        isSafeFromXXE = false
      }
      let desc = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
      issues.append(desc)
      return SecurityAuditReport(
        fileName: fileName,
        detectedFormat: detectedFormat,
        isSignatureValid: false,
        isSafeFromBombAndTraversal: isSafeFromBombAndTraversal,
        isSafeFromXXE: isSafeFromXXE,
        isUnencrypted: isUnencrypted,
        issues: issues,
        isPassed: false
      )
    }

    switch verified.containerFamily {
    case .zipXML:
      do {
        let entries = try ZipContainerCodec.unpack(from: data)
        for entry in entries {
          let lowerPath = entry.path.lowercased()
          let isXMLPayload =
            lowerPath.hasSuffix(".xml")
            || lowerPath.hasSuffix(".rels")
            || lowerPath.hasSuffix(".hpf")
          do {
            try XMLSanitizer.validateSafeXMLBytes(entry.data)
          } catch {
            isSafeFromXXE = false
            let desc = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
            issues.append(desc)
          }
          if isXMLPayload, String(data: entry.data, encoding: .utf8) == nil {
            isSafeFromBombAndTraversal = false
            issues.append("ZIP 내부 XML 엔트리가 유효한 UTF-8이 아닙니다: \(entry.path)")
          }
        }
      } catch let error as EasyOfficeError {
        switch error {
        case .encryptedDocumentUnsupported:
          isUnencrypted = false
        case .xxeOrExternalEntityDetected:
          isSafeFromXXE = false
        default:
          isSafeFromBombAndTraversal = false
        }
        issues.append(error.errorDescription ?? error.localizedDescription)
      } catch {
        isSafeFromBombAndTraversal = false
        issues.append(error.localizedDescription)
      }

    case .ole2Compound:
      do {
        let streams = try OLE2ContainerCodec.unpack(from: data)
        for stream in streams {
          if stream.name == "EncryptionInfo" || stream.name == "EncryptedPackage" {
            isUnencrypted = false
            let err = EasyOfficeError.encryptedDocumentUnsupported(format: verified.format)
            issues.append(err.errorDescription ?? "암호화된 OLE2 컨테이너입니다.")
          }
        }
        if verified.format == .hwp || verified.format == .hwt {
          guard let headerStream = streams.first(where: { $0.name == "FileHeader" }),
            headerStream.data.count >= 40,
            headerStream.data.starts(with: Data(HWP5BinaryCodec.signatureString.utf8))
          else {
            isSafeFromBombAndTraversal = false
            issues.append("HWP 5.0 FileHeader 스트림이 누락되었거나 손상되었습니다.")
            break
          }
          let flags = try BitPacking.readUInt32LE(from: headerStream.data, at: 36)
          if (flags & (HWP5BinaryCodec.flagEncrypted | HWP5BinaryCodec.flagDistribution)) != 0 {
            isUnencrypted = false
            let err = EasyOfficeError.encryptedDocumentUnsupported(format: verified.format)
            issues.append(err.errorDescription ?? "암호화된 문서입니다.")
          }
        } else if verified.format == .doc {
          _ = try LegacyOLE2OfficeCodec.decodeWordBinary(from: data)
        } else if verified.format == .xls {
          _ = try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: data)
        } else if verified.format == .ppt {
          _ = try LegacyOLE2OfficeCodec.decodePowerPointBinary(from: data)
        }
      } catch let error as EasyOfficeError {
        if case .encryptedDocumentUnsupported = error {
          isUnencrypted = false
        } else {
          isSafeFromBombAndTraversal = false
        }
        issues.append(error.errorDescription ?? error.localizedDescription)
      } catch {
        isSafeFromBombAndTraversal = false
        let desc = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
        issues.append(desc)
      }

    case .pdfBinary:
      let rawString = String(decoding: data, as: UTF8.self)
      if !rawString.contains("obj") || !rawString.contains("%%EOF") {
        isSafeFromBombAndTraversal = false
        issues.append("PDF 컨테이너에 유효한 객체(obj) 또는 %%EOF 트레일러가 없습니다.")
      }
      if rawString.contains("/Encrypt") {
        isUnencrypted = false
        let err = EasyOfficeError.encryptedDocumentUnsupported(format: .pdf)
        issues.append(err.errorDescription ?? "암호화된 PDF 문서입니다.")
      }

    case .rtfText, .plainText:
      do {
        try XMLSanitizer.validateSafeXMLBytes(data)
      } catch {
        isSafeFromXXE = false
        let desc = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
        issues.append(desc)
      }
      if String(data: data, encoding: .utf8) == nil || data.contains(0x00) {
        isSafeFromBombAndTraversal = false
        issues.append("텍스트/RTF 컨테이너에 유효하지 않은 UTF-8 또는 NUL 바이트가 포함되어 있습니다.")
      }
    }

    let isPassed =
      isSignatureValid
      && isSafeFromBombAndTraversal
      && isSafeFromXXE
      && isUnencrypted
      && issues.isEmpty

    return SecurityAuditReport(
      fileName: fileName,
      detectedFormat: verified.format,
      isSignatureValid: isSignatureValid,
      isSafeFromBombAndTraversal: isSafeFromBombAndTraversal,
      isSafeFromXXE: isSafeFromXXE,
      isUnencrypted: isUnencrypted,
      issues: issues,
      isPassed: isPassed
    )
  }
}
