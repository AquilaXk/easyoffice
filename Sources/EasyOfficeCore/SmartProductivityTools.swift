import Foundation

// MARK: - A. Bidirectional Format Conversion Studio

/// Single format conversion request across any supported office module.
public struct ConversionJobRequest: Sendable, Equatable, Identifiable {
  public let id: UUID
  public var sourceFileName: String
  public var sourceFileURL: URL?
  public var sourceFormat: DocumentFormat
  public var targetFormat: DocumentFormat
  public var payload: OpenTabPayload

  public init(
    id: UUID = UUID(),
    sourceFileName: String,
    sourceFileURL: URL? = nil,
    sourceFormat: DocumentFormat,
    targetFormat: DocumentFormat,
    payload: OpenTabPayload
  ) {
    self.id = id
    self.sourceFileName = sourceFileName
    self.sourceFileURL = sourceFileURL
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

  public var succeeded: [ConversionJobResult] { succeededResults }

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

  public static func disambiguateOutputFileName(
    _ proposedFileName: String,
    usedLowercasedNames: inout Set<String>
  ) -> String {
    let lower = proposedFileName.lowercased()
    if !usedLowercasedNames.contains(lower) {
      usedLowercasedNames.insert(lower)
      return proposedFileName
    }
    let nsName = proposedFileName as NSString
    let stem = nsName.deletingPathExtension
    let ext = nsName.pathExtension
    var counter = 2
    while true {
      let candidate = ext.isEmpty ? "\(stem)_\(counter)" : "\(stem)_\(counter).\(ext)"
      let candidateLower = candidate.lowercased()
      if !usedLowercasedNames.contains(candidateLower) {
        usedLowercasedNames.insert(candidateLower)
        return candidate
      }
      counter += 1
    }
  }

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
      outputBytes = try encodeDocumentModel(
        docModel,
        as: request.targetFormat,
        sourceFormat: request.sourceFormat
      )

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

  public static let maxCumulativeBatchBytes = 512 * 1024 * 1024

  public static func executeBatch(
    _ jobs: [ConversionJobRequest],
    outputDirectory: URL? = nil
  ) -> BatchConversionReport {
    var succeeded: [ConversionJobResult] = []
    var failed: [(jobID: UUID, errorDescription: String)] = []
    var usedOutputNamesLowercased: Set<String> = []
    var cumulativeOutputBytes = 0
    let targetDir =
      outputDirectory
      ?? FileManager.default.temporaryDirectory.appendingPathComponent(
        "EasyOfficeBatchOutput", isDirectory: true)

    for job in jobs {
      do {
        let converted = try convert(job)
        let outData = converted.outputBytes
        if cumulativeOutputBytes + outData.count > maxCumulativeBatchBytes {
          throw EasyOfficeError.resourceExhausted(
            limitName: "batchTotalOutputBytes",
            observed: cumulativeOutputBytes + outData.count,
            maximum: maxCumulativeBatchBytes
          )
        }
        cumulativeOutputBytes += outData.count
        let uniqueOutputName = disambiguateOutputFileName(
          converted.outputFileName,
          usedLowercasedNames: &usedOutputNamesLowercased
        )
        try FileManager.default.createDirectory(at: targetDir, withIntermediateDirectories: true)
        let fileURL = targetDir.appendingPathComponent(uniqueOutputName)
        try outData.write(to: fileURL, options: .atomic)
        let result = ConversionJobResult(
          id: converted.id,
          sourceFileName: converted.sourceFileName,
          outputFileName: uniqueOutputName,
          sourceFormat: converted.sourceFormat,
          targetFormat: converted.targetFormat,
          outputBytes: outData,
          convertedPayload: converted.convertedPayload,
          verifiedSignature: converted.verifiedSignature,
          outputFileURL: fileURL
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

  public static func executeBatchAsync(
    _ jobs: [ConversionJobRequest],
    outputDirectory: URL? = nil
  ) async -> BatchConversionReport {
    await Task.detached(priority: .userInitiated) {
      executeBatch(jobs, outputDirectory: outputDirectory)
    }.value
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

  private static func encodeDocumentModel(
    _ doc: OfficeDocumentModel,
    as format: DocumentFormat,
    sourceFormat: DocumentFormat? = nil
  ) throws -> Data {
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
      let effectiveSource = sourceFormat ?? doc.format
      if effectiveSource != .txt, !doc.title.isEmpty, doc.paragraphs.first?.text != doc.title {
        lines.append(doc.title)
      }
      for p in doc.paragraphs {
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

extension EasyOfficeError {
  public static func resourceExhausted(
    limitName: String,
    observed: Int,
    maximum: Int
  ) -> EasyOfficeError {
    .resourceExhausted(limitName: limitName, limitValue: maximum, actualValue: observed)
  }
}
