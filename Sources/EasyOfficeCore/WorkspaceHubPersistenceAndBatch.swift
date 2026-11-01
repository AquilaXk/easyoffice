import EasyOfficeDesignSystem
import Foundation

extension WorkspaceViewModel {
  internal struct PersistedRecentRecordMetadata: Codable {
    let idString: String
    let fileName: String
    let formatRawValue: String
    let lastOpenedISO8601: String
    let isPinned: Bool
    let isFavorite: Bool
    let filePath: String?
  }

  internal struct PersistedCustomTemplateDTO: Codable {
    let descriptor: OfficeTemplateDescriptor
    let payload: OpenTabPayload
  }

  internal static let recentMetadataDefaultsKey = "eo.recentDocumentMetadataV1"
  internal static let customTemplatesDefaultsKey = "eo.customTemplatesV1"

  internal static func lightweightPlaceholderPayload(for format: DocumentFormat) -> OpenTabPayload {
    switch format.moduleKind {
    case .document, .hwp:
      return .document(
        OfficeDocumentModel(
          title: "",
          format: format,
          paragraphs: [ParagraphBlock(style: .body, text: "")]
        )
      )
    case .sheet:
      return .sheet(WorksheetModel(name: "Sheet1"))
    case .slide:
      return .slide(
        PresentationDeckModel(
          title: "",
          aspectRatio: .widescreen16x9,
          slides: [SlideModel(title: "")]
        )
      )
    case .pdf:
      return .pdf(
        PDFStudioDocumentModel(
          title: "",
          pages: [PDFPageDescriptor(pageNumber: 1, extractedText: "")]
        )
      )
    }
  }

  internal func recordRecentDocument(_ tab: WorkspaceTabItem, isOpenInWorkspace: Bool = true) {
    var updated = tab
    updated.lastOpenedISO8601 = OfficeDocumentModel.currentISO8601Timestamp()
    if !isOpenInWorkspace, updated.fileURL != nil, !updated.isModified {
      updated.payload = Self.lightweightPlaceholderPayload(for: updated.format)
      updated.isMetadataOnlyPlaceholder = true
    }
    let updatedStdPath = updated.fileURL?.standardizedFileURL.path
    recentDocumentRecords.removeAll { existing in
      if existing.id == updated.id { return true }
      if let p1 = existing.fileURL?.standardizedFileURL.path, let p2 = updatedStdPath, p1 == p2 {
        return true
      }
      return false
    }
    recentDocumentRecords.insert(updated, at: 0)
    if recentDocumentRecords.count > 24 {
      recentDocumentRecords.removeLast(recentDocumentRecords.count - 24)
    }
    persistRecentDocumentMetadata()
  }

  public func persistRecentDocumentMetadata(to defaults: UserDefaults = .standard) {
    var seenPaths = Set<String>()
    var seenIDs = Set<String>()
    var metadataList: [PersistedRecentRecordMetadata] = []
    for item in recentDocumentRecords {
      if let stdPath = item.fileURL?.standardizedFileURL.path {
        if !seenPaths.insert(stdPath).inserted { continue }
      } else {
        if !seenIDs.insert(item.id.uuidString).inserted { continue }
      }
      metadataList.append(
        PersistedRecentRecordMetadata(
          idString: item.id.uuidString,
          fileName: item.fileName,
          formatRawValue: item.format.rawValue,
          lastOpenedISO8601: item.lastOpenedISO8601,
          isPinned: item.isPinned,
          isFavorite: item.isFavorite,
          filePath: item.fileURL?.standardizedFileURL.path
        )
      )
    }
    if let encoded = try? JSONEncoder().encode(metadataList) {
      defaults.set(encoded, forKey: Self.recentMetadataDefaultsKey)
    }
  }

  public func restorePersistedRecentDocuments(from defaults: UserDefaults = .standard) {
    guard let rawData = defaults.data(forKey: Self.recentMetadataDefaultsKey),
      let decodedList = try? JSONDecoder().decode(
        [PersistedRecentRecordMetadata].self, from: rawData)
    else {
      return
    }
    var restored: [WorkspaceTabItem] = []
    var seenPaths = Set<String>()
    var seenIDs = Set<UUID>()

    for meta in decodedList.prefix(24) {
      let resolvedID = UUID(uuidString: meta.idString) ?? UUID()
      let resolvedURL = meta.filePath.map { URL(fileURLWithPath: $0) }
      let stdPath = resolvedURL?.standardizedFileURL.path
      if let stdPath {
        if !seenPaths.insert(stdPath).inserted { continue }
      }
      if !seenIDs.insert(resolvedID).inserted { continue }

      if let existingIdx = recentDocumentRecords.firstIndex(where: {
        $0.id == resolvedID
          || ($0.fileURL != nil && $0.fileURL?.standardizedFileURL.path == stdPath)
      }), resolvedURL == nil {
        var existing = recentDocumentRecords[existingIdx]
        existing.isPinned = meta.isPinned
        existing.isFavorite = meta.isFavorite
        existing.lastOpenedISO8601 = meta.lastOpenedISO8601
        restored.append(existing)
      } else if let fmt = DocumentFormat(rawValue: meta.formatRawValue) {
        let isPlaceholder = resolvedURL != nil
        restored.append(
          WorkspaceTabItem(
            id: resolvedID,
            fileName: meta.fileName,
            format: fmt,
            isPinned: meta.isPinned,
            isFavorite: meta.isFavorite,
            isModified: false,
            lastOpenedISO8601: meta.lastOpenedISO8601,
            fileURL: resolvedURL,
            payload: Self.lightweightPlaceholderPayload(for: fmt),
            isLosslessNativeOrigin: false,
            isMetadataOnlyPlaceholder: isPlaceholder,
            isUnreadable: false
          )
        )
      }
    }

    for existing in recentDocumentRecords {
      let existingPath = existing.fileURL?.standardizedFileURL.path
      let alreadyIncluded = restored.contains { item in
        if item.id == existing.id { return true }
        if let p1 = item.fileURL?.standardizedFileURL.path, let p2 = existingPath, p1 == p2 {
          return true
        }
        return false
      }
      if !alreadyIncluded && restored.count < 24 {
        restored.append(existing)
      }
    }
    recentDocumentRecords = restored
  }

  internal var combinedRecentPool: [WorkspaceTabItem] {
    var seenIDs = Set<UUID>()
    var seenPaths = Set<String>()
    var merged: [WorkspaceTabItem] = []
    for tab in openTabs {
      seenIDs.insert(tab.id)
      if let p = tab.fileURL?.standardizedFileURL.path {
        seenPaths.insert(p)
      }
      merged.append(tab)
    }
    for record in recentDocumentRecords where !seenIDs.contains(record.id) {
      if let p = record.fileURL?.standardizedFileURL.path, !seenPaths.insert(p).inserted {
        continue
      }
      seenIDs.insert(record.id)
      merged.append(record)
    }
    return merged
  }


  public func saveActiveTabAsCustomTemplate(
    title: String? = nil, subtitle: String = "현재 활성 문서 기반 커스텀 템플릿"
  ) {
    guard let current = activeTab else { return }
    let draft = customTemplateTitleDraft.trimmingCharacters(in: .whitespacesAndNewlines)
    let explicitTitle = (title ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    let resolvedTitle: String
    if !draft.isEmpty && (explicitTitle.isEmpty || explicitTitle == "사용자 정의 표준 양식") {
      resolvedTitle = draft
      customTemplateTitleDraft = ""
    } else if !explicitTitle.isEmpty {
      resolvedTitle = explicitTitle
    } else {
      resolvedTitle = "사용자 정의 표준 양식"
    }
    let descriptor = OfficeTemplateDescriptor(
      id: "custom-\(UUID().uuidString.lowercased())", title: resolvedTitle, subtitle: subtitle,
      format: current.format, isCustomUserTemplate: true)
    customTemplatePayloads[descriptor.id] = current.payload
    customTemplates.insert(descriptor, at: 0)
    persistCustomTemplates()
    lastExecutedCommandSummary = "사용자 템플릿 저장 완료: \(resolvedTitle)"
  }

  public func removeCustomTemplate(id: String) {
    customTemplates.removeAll { $0.id == id }
    customTemplatePayloads.removeValue(forKey: id)
    persistCustomTemplates()
  }

  public func persistCustomTemplates(to defaults: UserDefaults = .standard) {
    let dtos: [PersistedCustomTemplateDTO] = customTemplates.prefix(24).compactMap { tpl in
      guard let payload = customTemplatePayloads[tpl.id] else { return nil }
      return PersistedCustomTemplateDTO(descriptor: tpl, payload: payload)
    }
    if let encoded = try? JSONEncoder().encode(dtos) {
      defaults.set(encoded, forKey: Self.customTemplatesDefaultsKey)
    }
  }

  public func restorePersistedCustomTemplates(from defaults: UserDefaults = .standard) {
    guard let raw = defaults.data(forKey: Self.customTemplatesDefaultsKey),
      let dtos = try? JSONDecoder().decode([PersistedCustomTemplateDTO].self, from: raw)
    else { return }
    var templates: [OfficeTemplateDescriptor] = []
    var payloads: [String: OpenTabPayload] = [:]
    for dto in dtos {
      templates.append(dto.descriptor)
      payloads[dto.descriptor.id] = dto.payload
    }
    customTemplates = templates
    customTemplatePayloads = payloads
  }


  internal func decodeDocumentPayload(data: Data, fileName: String) throws -> (
    format: DocumentFormat, payload: OpenTabPayload
  ) {
    let verified = try FormatDetector.verify(data: data, fileName: fileName)
    let format = verified.format
    let payload: OpenTabPayload

    switch format {
    case .docx:
      var doc = try OOXMLContainerCodec.decodeDocument(from: data)
      doc.format = format
      payload = .document(doc)
    case .odt:
      var doc = try ODFContainerCodec.decodeDocument(from: data)
      doc.format = format
      payload = .document(doc)
    case .hwpx, .hwtx:
      var doc = try HWPXContainerCodec.decodeDocument(from: data)
      doc.format = format
      payload = .document(doc)
    case .hwp, .hwt:
      var doc = try HWP5BinaryCodec.decodeDocument(from: data)
      doc.format = format
      payload = .document(doc)
    case .doc:
      var doc = try LegacyOLE2OfficeCodec.decodeWordBinary(from: data)
      doc.format = format
      payload = .document(doc)
    case .rtf:
      var doc = try RTFCodec.decodeDocument(from: data)
      doc.format = format
      payload = .document(doc)
    case .md, .txt:
      var doc = try CommonMarkCodec.decodeDocument(from: data, title: fileName)
      doc.format = format
      payload = .document(doc)
    case .xlsx: payload = .sheet(try OOXMLContainerCodec.decodeWorksheet(from: data))
    case .xls:
      payload = .sheet(try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: data, name: fileName))
    case .ods: payload = .sheet(try ODFContainerCodec.decodeWorksheet(from: data))
    case .csv:
      payload = .sheet(
        try CSVStateMachineCodec.decodeWorksheet(from: data, name: fileName, delimiter: ","))
    case .tsv:
      payload = .sheet(
        try CSVStateMachineCodec.decodeWorksheet(from: data, name: fileName, delimiter: "\t"))
    case .pptx, .ppsx: payload = .slide(try OOXMLContainerCodec.decodePresentation(from: data))
    case .ppt: payload = .slide(try LegacyOLE2OfficeCodec.decodePowerPointBinary(from: data))
    case .odp: payload = .slide(try ODFContainerCodec.decodePresentation(from: data))
    case .pdf: payload = .pdf(try PDFBinarySerializer.decode(from: data))
    }
    return (format, payload)
  }

  @discardableResult
  public func openDocument(data: Data, fileName: String, fileURL: URL? = nil) throws
    -> WorkspaceTabItem
  {
    let decoded = try decodeDocumentPayload(data: data, fileName: fileName)
    let isNativeOrigin: Bool
    if let fileURL {
      isNativeOrigin = knownEasyOfficeSavedFilePaths.contains(fileURL.standardizedFileURL.path)
    } else {
      isNativeOrigin = true
    }
    let newTab = WorkspaceTabItem(
      fileName: fileName, format: decoded.format,
      lastOpenedISO8601: OfficeDocumentModel.currentISO8601Timestamp(), fileURL: fileURL,
      payload: decoded.payload,
      isLosslessNativeOrigin: isNativeOrigin,
      isMetadataOnlyPlaceholder: false,
      isUnreadable: false)
    openTabs.append(newTab)
    activeTabID = newTab.id
    isShowingHomeHub = false
    lastErrorMessage = nil
    recordRecentDocument(newTab)
    return newTab
  }

  @discardableResult
  public func loadFromDisk(url: URL) throws -> WorkspaceTabItem {
    try openDocument(data: try Data(contentsOf: url), fileName: url.lastPathComponent, fileURL: url)
  }

  @discardableResult
  public func importDocumentFromDisk(url: URL) throws -> WorkspaceTabItem {
    try loadFromDisk(url: url)
  }

  internal func convertTab(id: UUID, targetFormat: DocumentFormat? = nil) throws
    -> ConversionJobResult
  {
    guard let tab = openTabs.first(where: { $0.id == id }) else {
      throw EasyOfficeError.emptyPayload
    }
    let fmt = targetFormat ?? tab.format
    let safeSourceName =
      tab.fileName.contains(".") ? tab.fileName : "\(tab.fileName).\(tab.format.fileExtension)"
    return try DocumentConversionOrchestrator.convert(
      ConversionJobRequest(
        sourceFileName: safeSourceName, sourceFileURL: tab.fileURL, sourceFormat: tab.format,
        targetFormat: fmt, payload: tab.payload))
  }

  public func exportTabData(id: UUID, targetFormat: DocumentFormat? = nil) throws -> Data {
    try convertTab(id: id, targetFormat: targetFormat).outputBytes
  }

  public func saveTabToDisk(id: UUID, to url: URL, targetFormat: DocumentFormat? = nil) throws {
    guard let idx = openTabs.firstIndex(where: { $0.id == id }) else {
      throw EasyOfficeError.emptyPayload
    }
    let fmt =
      targetFormat ?? DocumentFormat.from(fileNameOrExtension: url.lastPathComponent)
      ?? openTabs[idx].format
    let result = try convertTab(id: id, targetFormat: fmt)
    try result.outputBytes.write(to: url, options: .atomic)
    if fmt.moduleKind == openTabs[idx].format.moduleKind {
      if openTabs[idx].format != fmt || openTabs[idx].payload != result.convertedPayload {
        recordUndoSnapshot(for: id)
      }
      openTabs[idx].payload = result.convertedPayload
      openTabs[idx].isModified = false
      openTabs[idx].fileURL = url
      openTabs[idx].fileName = url.lastPathComponent
      openTabs[idx].format = fmt
      openTabs[idx].isLosslessNativeOrigin = true
      openTabs[idx].isMetadataOnlyPlaceholder = false
      openTabs[idx].isUnreadable = false
      openTabs[idx].lastOpenedISO8601 = OfficeDocumentModel.currentISO8601Timestamp()
      knownEasyOfficeSavedFilePaths.insert(url.standardizedFileURL.path)
      if pendingLossyOverwriteConfirmationTabID == id {
        pendingLossyOverwriteConfirmationTabID = nil
      }
      recordRecentDocument(openTabs[idx])
      lastExecutedCommandSummary = "저장 완료: \(url.lastPathComponent)"
    } else {
      lastExecutedCommandSummary = "포맷 내보내기 완료: \(url.lastPathComponent)"
    }
    lastErrorMessage = nil
  }

  @discardableResult
  public func saveTabOverwriteIfBackedByFile(
    id: UUID,
    allowExternalOverwrite: Bool = false
  ) throws -> Bool {
    guard let idx = openTabs.firstIndex(where: { $0.id == id }),
      let existingURL = openTabs[idx].fileURL
    else {
      return false
    }
    if !openTabs[idx].isLosslessNativeOrigin && !allowExternalOverwrite {
      pendingLossyOverwriteConfirmationTabID = id
      lastExecutedCommandSummary =
        "외부 원본 파일 보호: '\(openTabs[idx].fileName)' 덮어쓰기 전 다른 이름으로 저장(Save As) 또는 백업 저장 확인이 필요합니다"
      return false
    }
    if !openTabs[idx].isLosslessNativeOrigin && allowExternalOverwrite {
      if FileManager.default.fileExists(atPath: existingURL.path) {
        let backupURL = existingURL.appendingPathExtension("bak")
        let originalData = try Data(contentsOf: existingURL)
        try originalData.write(to: backupURL, options: .atomic)
      }
    }
    try saveTabToDisk(id: id, to: existingURL, targetFormat: openTabs[idx].format)
    return true
  }

  @discardableResult
  public func saveActiveTabOverwriteIfBackedByFile(
    allowExternalOverwrite: Bool = false
  ) throws -> Bool {
    guard let activeTabID else { return false }
    return try saveTabOverwriteIfBackedByFile(
      id: activeTabID,
      allowExternalOverwrite: allowExternalOverwrite
    )
  }


  public func applyFormulaBarEdit() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(let sheet) = openTabs[idx].payload
    else { return }
    activeInlineEditTargetKey = nil
    let previousPayload = openTabs[idx].payload
    var updatedSheet = sheet
    do {
      try updatedSheet.setCell(selectedSheetCell, rawValue: formulaBarDraft)
      if let rule = activeDataValidationRule {
        let trimmed = formulaBarDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        let isValid =
          trimmed.hasPrefix("=")
          ? rule.validate(updatedSheet.evaluate(at: selectedSheetCell))
          : rule.validate(rawValue: formulaBarDraft)
        guard isValid else {
          lastErrorMessage = "데이터 유효성 검사 실패: 입력값 '\(formulaBarDraft)'이(가) 허용 범위를 벗어났습니다."
          return
        }
      }
      var stack = undoStacks[activeTabID] ?? []
      stack.append(previousPayload)
      if stack.count > Self.maxUndoDepth {
        stack.removeFirst(stack.count - Self.maxUndoDepth)
      }
      undoStacks[activeTabID] = stack
      redoStacks[activeTabID] = []
      openTabs[idx].payload = .sheet(updatedSheet)
      openTabs[idx].isModified = true
      lastErrorMessage = nil
    } catch {
      lastErrorMessage = error.localizedDescription
    }
  }

  public func commitSheetFormulaBarEdit() {
    applyFormulaBarEdit()
  }

  public func setSelectedSheetCellNumberFormat(_ format: CellNumberFormat) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let coord =
      (try? CellAddress(a1Notation: selectedSheetCell))?.packedCoordinate
      ?? PackedCellCoordinate(row: 0, column: 0)
    var style = sheet.cellStyle(at: coord)
    style.numberFormat = format
    sheet.setCellStyle(style, at: coord)
    if let range = try? CellRange(notation: selectedSheetRange), range.start != range.end {
      sheet.updateCellStyleInRange(range) { $0.numberFormat = format }
    }
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "셀 숫자 서식 적용: \(format.rawValue)"
  }

  public func setSelectedSheetCellAlignment(_ alignment: CellHorizontalAlignment) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let coord =
      (try? CellAddress(a1Notation: selectedSheetCell))?.packedCoordinate
      ?? PackedCellCoordinate(row: 0, column: 0)
    var style = sheet.cellStyle(at: coord)
    style.alignment = alignment
    sheet.setCellStyle(style, at: coord)
    if let range = try? CellRange(notation: selectedSheetRange), range.start != range.end {
      sheet.updateCellStyleInRange(range) { $0.alignment = alignment }
    }
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "셀 가로 정렬 적용: \(alignment.rawValue)"
  }

  public func selectSheetCellOrRange(primaryCell: String, rangeNotation: String? = nil) {
    activeInlineEditTargetKey = nil
    selectedSheetCell = primaryCell
    selectedSheetRange = rangeNotation ?? primaryCell
    if let tab = activeTab, case .sheet(let sheet) = tab.payload {
      formulaBarDraft = sheet.rawValue(at: primaryCell)
    } else {
      formulaBarDraft = ""
    }
  }

  public func extendSheetSelection(to endCell: String) {
    selectedSheetRange =
      (selectedSheetCell == endCell) ? selectedSheetCell : "\(selectedSheetCell):\(endCell)"
  }

  public func performAutoFillOnActiveSheet(
    sourceRangeNotation: String? = nil, targetRangeNotation: String
  ) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    let srcStr = sourceRangeNotation ?? selectedSheetRange
    do {
      let srcRange = try CellRange(notation: srcStr)
      let dstRange = try CellRange(notation: targetRangeNotation)
      try sheet.autofill(sourceRange: srcRange, targetRange: dstRange)
      recordUndoSnapshot(for: activeTabID)
      selectedSheetRange = dstRange.a1Range
      openTabs[idx].payload = .sheet(sheet)
      openTabs[idx].isModified = true
      lastErrorMessage = nil
      lastExecutedCommandSummary = "시트 자동 채우기 완료 (\(srcRange.a1Range) → \(dstRange.a1Range))"
    } catch {
      lastErrorMessage = error.localizedDescription
    }
  }

  public func updateActiveSheetColumnWidth(column: Int, width: Double) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID, coalescingTargetKey: "colw:\(activeTabID):\(column)")
    sheet.setColumnWidth(width, at: column)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary =
      "열 너비 조정 (\(CellAddress.columnLabel(for: max(0, column)))열: \(Int(sheet.columnWidth(at: column)))pt)"
  }

  public func updateActiveSheetRowHeight(row: Int, height: Double) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID, coalescingTargetKey: "rowh:\(activeTabID):\(row)")
    sheet.setRowHeight(height, at: row)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "행 높이 조정 (\(row + 1)행: \(Int(sheet.rowHeight(at: row)))pt)"
  }

  public func setSelectedSheetCellComment(_ comment: String) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    let coord =
      (try? CellAddress(a1Notation: selectedSheetCell))?.packedCoordinate
      ?? PackedCellCoordinate(row: 0, column: 0)
    let trimmed = comment.trimmingCharacters(in: .whitespacesAndNewlines)
    let existing = sheet.sheetComments[coord] ?? ""
    guard existing != trimmed else { return }
    recordUndoSnapshot(
      for: activeTabID,
      coalescingTargetKey: "sheet-comment-\(activeTabID.uuidString)-\(coord.rawValue)")
    if trimmed.isEmpty {
      sheet.sheetComments.removeValue(forKey: coord)
    } else {
      sheet.sheetComments[coord] = trimmed
    }
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = trimmed.isEmpty ? "셀 메모 삭제 완료" : "셀 메모 저장 완료"
  }

  public func mergedSheetRanges(for tabID: UUID) -> [String] {
    var combined = mergedSheetRangesByTab[tabID] ?? []
    if let tab = openTabs.first(where: { $0.id == tabID }), case .sheet(let sheet) = tab.payload {
      for r in sheet.mergedRanges {
        let a1 = r.a1Range
        if !combined.contains(a1) { combined.append(a1) }
      }
    }
    return combined
  }

  public func toggleMergeOnSelectedSheetCell(targetRange: String? = nil) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let resolvedRange: String
    if let targetRange, !targetRange.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      resolvedRange = targetRange.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
    } else if selectedSheetRange.contains(":"),
      let range = try? CellRange(notation: selectedSheetRange),
      range.start != range.end
    {
      resolvedRange = range.a1Range
    } else if let addr = try? CellAddress(a1Notation: selectedSheetCell),
      let nextAddr = try? CellAddress(
        column: min(CellAddress.maxZeroBasedColumn, addr.column + 1), row: addr.row)
    {
      resolvedRange = "\(addr.a1Notation):\(nextAddr.a1Notation)"
    } else {
      resolvedRange = "\(selectedSheetCell.uppercased()):\(selectedSheetCell.uppercased())"
    }
    if let parsedRange = try? CellRange(notation: resolvedRange) {
      sheet.toggleMergeCells(parsedRange)
    }
    var ranges = mergedSheetRangesByTab[activeTabID] ?? []
    if let existingIdx = ranges.firstIndex(of: resolvedRange) {
      ranges.remove(at: existingIdx)
      lastExecutedCommandSummary = "셀 병합 해제: \(resolvedRange)"
    } else {
      ranges.append(resolvedRange)
      let topLeftA1 = resolvedRange.components(separatedBy: ":").first ?? selectedSheetCell
      let coord =
        (try? CellAddress(a1Notation: topLeftA1))?.packedCoordinate
        ?? PackedCellCoordinate(row: 0, column: 0)
      var style = sheet.cellStyle(at: coord)
      style.alignment = .center
      sheet.setCellStyle(style, at: coord)
      lastExecutedCommandSummary = "셀 병합 적용: \(resolvedRange)"
    }
    openTabs[idx].payload = .sheet(sheet)
    mergedSheetRangesByTab[activeTabID] = ranges
    openTabs[idx].isModified = true
  }

  public func addWorksheetTab(named: String? = nil) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let resolvedName = named ?? "Sheet\(sheet.allSheetNames.count + 1)"
    sheet.addSheetPage(
      named: resolvedName,
      initialCells: [
        PackedCellCoordinate(row: 0, column: 0): "항목", PackedCellCoordinate(row: 0, column: 1): "값",
      ])
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "새 시트 탭 추가: \(resolvedName)"
  }

  public func selectWorksheetTab(index: Int) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    activeInlineEditTargetKey = nil
    sheet.switchActiveSheet(to: index)
    formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
    openTabs[idx].payload = .sheet(sheet)
    syncSheetRulesFromActiveTab()
    lastExecutedCommandSummary = "시트 탭 전환: \(sheet.name)"
  }

  public func renameWorksheetTab(at index: Int, to newName: String) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    let trimmed = newName.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return }
    recordUndoSnapshot(for: activeTabID)
    sheet.renameSheetPage(at: index, to: trimmed)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "시트 탭 이름 변경: \(trimmed)"
  }

  public func deleteWorksheetTab(at index: Int) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload,
      sheet.allSheetNames.count > 1
    else { return }
    recordUndoSnapshot(for: activeTabID)
    sheet.deleteSheetPage(at: index)
    formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    syncSheetRulesFromActiveTab()
    lastExecutedCommandSummary = "시트 탭 삭제 완료"
  }

  public func copySelectedSheetRangeAsTSV() -> String {
    guard let tab = activeTab, case .sheet(let sheet) = tab.payload else { return "" }
    let originAddress = CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    let fallbackRange =
      (try? CellAddress(a1Notation: selectedSheetCell)).map { CellRange(start: $0, end: $0) }
      ?? CellRange(start: originAddress, end: originAddress)
    let range = (try? CellRange(notation: selectedSheetRange)) ?? fallbackRange
    let minR = min(range.start.row, range.end.row)
    let maxR = max(range.start.row, range.end.row)
    let minC = min(range.start.column, range.end.column)
    let maxC = max(range.start.column, range.end.column)

    var rowStrings: [String] = []
    for r in minR...maxR {
      var colStrings: [String] = []
      for c in minC...maxC {
        let addr = CellAddress(packed: PackedCellCoordinate(row: r, column: c))
        let raw = sheet.rawValue(at: addr.a1Notation)
        let cellStr = raw.hasPrefix("=") ? sheet.formattedDisplayValue(at: addr) : raw
        colStrings.append(cellStr)
      }
      rowStrings.append(colStrings.joined(separator: "\t"))
    }
    return rowStrings.joined(separator: "\n")
  }

  public func pasteSheetCellsFromTSV(_ tsvText: String) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    let normalized = tsvText.replacingOccurrences(of: "\r\n", with: "\n")
    var lines = normalized.components(separatedBy: "\n")
    if lines.count > 1 && lines.last == "" {
      lines.removeLast()
    }
    guard !lines.isEmpty else { return }

    let startAddr =
      (try? CellAddress(a1Notation: selectedSheetCell))
      ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    recordUndoSnapshot(for: activeTabID)

    var maxValidRow = startAddr.row
    var maxValidCol = startAddr.column
    var updatedCount = 0
    var skippedOrFailedCount = 0
    var firstErrorDetail: String? = nil

    for (rOffset, line) in lines.enumerated() {
      let cols = line.components(separatedBy: "\t")
      let (targetRow, rowOverflow) = startAddr.row.addingReportingOverflow(rOffset)
      for (cOffset, val) in cols.enumerated() {
        let (targetCol, colOverflow) = startAddr.column.addingReportingOverflow(cOffset)
        if rowOverflow || colOverflow || targetRow > CellAddress.maxZeroBasedRow
          || targetCol > CellAddress.maxZeroBasedColumn
        {
          skippedOrFailedCount += 1
          if firstErrorDetail == nil {
            firstErrorDetail =
              "시트 최대 경계(\(CellAddress.maxZeroBasedRow + 1)행 × \(CellAddress.maxZeroBasedColumn + 1)열)를 초과한 셀이 제외되었습니다."
          }
          continue
        }
        do {
          try sheet.setCell(row: targetRow, column: targetCol, rawValue: val)
          updatedCount += 1
          maxValidRow = max(maxValidRow, targetRow)
          maxValidCol = max(maxValidCol, targetCol)
        } catch {
          skippedOrFailedCount += 1
          if firstErrorDetail == nil {
            firstErrorDetail = error.localizedDescription
          }
        }
      }
    }

    let endAddr = CellAddress(packed: PackedCellCoordinate(row: maxValidRow, column: maxValidCol))
    selectedSheetRange =
      (startAddr == endAddr)
      ? startAddr.a1Notation : "\(startAddr.a1Notation):\(endAddr.a1Notation)"
    formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    if skippedOrFailedCount > 0 {
      lastErrorMessage =
        "TSV 붙여넣기 일부 제한 (\(skippedOrFailedCount)개 셀 제외/실패): \(firstErrorDetail ?? "")"
      lastExecutedCommandSummary =
        "TSV 셀 범위 붙여넣기 (\(updatedCount)개 반영, \(skippedOrFailedCount)개 제외)"
    } else {
      lastErrorMessage = nil
      lastExecutedCommandSummary = "TSV 셀 범위 붙여넣기 완료 (\(selectedSheetRange))"
    }
  }

  public func moveSheetSelection(
    rowDelta: Int,
    columnDelta: Int,
    extendSelection: Bool = false
  ) {
    let baseNotation: String
    if extendSelection, selectedSheetRange.contains(":") {
      baseNotation = selectedSheetRange.components(separatedBy: ":").last ?? selectedSheetCell
    } else {
      baseNotation = selectedSheetCell
    }
    let currentAddr =
      (try? CellAddress(a1Notation: baseNotation))
      ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    let nextRow = max(0, min(CellAddress.maxZeroBasedRow, currentAddr.row + rowDelta))
    let nextCol = max(0, min(CellAddress.maxZeroBasedColumn, currentAddr.column + columnDelta))
    let nextAddr = CellAddress(packed: PackedCellCoordinate(row: nextRow, column: nextCol))
    if extendSelection {
      extendSheetSelection(to: nextAddr.a1Notation)
    } else {
      selectSheetCellOrRange(primaryCell: nextAddr.a1Notation)
    }
  }

  public func clearSelectedSheetRange() {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    let originAddress = CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    let fallbackRange =
      (try? CellAddress(a1Notation: selectedSheetCell)).map { CellRange(start: $0, end: $0) }
      ?? CellRange(start: originAddress, end: originAddress)
    let range = (try? CellRange(notation: selectedSheetRange)) ?? fallbackRange
    recordUndoSnapshot(for: activeTabID)
    range.forEachCoordinate { coord in
      sheet.cellsByCoordinate.removeValue(forKey: coord)
      sheet.compiledASTs.removeValue(forKey: coord)
      sheet.sheetComments.removeValue(forKey: coord)
    }
    formulaBarDraft = ""
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "선택 셀 값 지우기 (\(selectedSheetRange))"
  }

  public func setSelectedSheetCellColors(
    fillHex: String? = nil,
    textHex: String? = nil,
    clearFill: Bool = false,
    clearText: Bool = false
  ) {
    guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let coord =
      (try? CellAddress(a1Notation: selectedSheetCell))?.packedCoordinate
      ?? PackedCellCoordinate(row: 0, column: 0)
    var style = sheet.cellStyle(at: coord)
    if clearFill {
      style.fillHex = nil
    } else if let fillHex {
      style.fillHex = fillHex
    }
    if clearText {
      style.textHex = nil
    } else if let textHex {
      style.textHex = textHex
    }
    sheet.setCellStyle(style, at: coord)
    if let range = try? CellRange(notation: selectedSheetRange), range.start != range.end {
      sheet.updateCellStyleInRange(range) { cellStyle in
        if clearFill {
          cellStyle.fillHex = nil
        } else if let fillHex {
          cellStyle.fillHex = fillHex
        }
        if clearText {
          cellStyle.textHex = nil
        } else if let textHex {
          cellStyle.textHex = textHex
        }
      }
    }
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "셀 배경·글자 색상 서식 적용"
  }

  public var isTrackChangesEnabled: Bool {
    get {
      guard let tab = activeTab else { return false }
      switch tab.payload {
      case .document(let d): return d.trackChangesEnabled
      case .sheet(let s): return s.trackChangesEnabled
      case .slide, .pdf: return false
      }
    }
    set {
      guard let activeTabID, let idx = openTabs.firstIndex(where: { $0.id == activeTabID }) else {
        return
      }
      switch openTabs[idx].payload {
      case .document(var d):
        d.trackChangesEnabled = newValue
        openTabs[idx].payload = .document(d)
      case .sheet(var s):
        s.trackChangesEnabled = newValue
        openTabs[idx].payload = .sheet(s)
      case .slide, .pdf:
        break
      }
    }
  }
}

