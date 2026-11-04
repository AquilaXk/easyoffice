import EasyOfficeDesignSystem
import Foundation

extension WorkspaceViewModel {


  @discardableResult
  public func recomputeFindMatches() -> Int {
    let total = findMatchCount
    if total == 0 || currentFindMatchIndex < 0 {
      currentFindMatchIndex = 0
    } else if currentFindMatchIndex > total {
      currentFindMatchIndex = total
    }
    return total
  }

  private func countOccurrences(
    of needle: String,
    in haystack: String,
    caseSensitive: Bool
  ) -> Int {
    guard !needle.isEmpty, !haystack.isEmpty else { return 0 }
    let options: String.CompareOptions = caseSensitive ? [] : [.caseInsensitive]
    var count = 0
    var searchStart = haystack.startIndex
    while searchStart < haystack.endIndex,
      let range = haystack.range(
        of: needle,
        options: options,
        range: searchStart..<haystack.endIndex
      )
    {
      count += 1
      if range.isEmpty {
        searchStart = haystack.index(after: range.lowerBound)
      } else {
        searchStart = range.upperBound
      }
    }
    return count
  }

  public var findMatchCount: Int {
    let needle = findQueryDraft.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !needle.isEmpty, let tab = activeTab else { return 0 }
    switch tab.payload {
    case .document(let doc):
      let paragraphTotal = doc.paragraphs.reduce(0) { acc, p in
        acc + countOccurrences(of: needle, in: p.text, caseSensitive: isFindCaseSensitive)
      }
      let tableTotal = doc.tables.reduce(0) { tAcc, tbl in
        tAcc
          + tbl.rows.reduce(0) { rAcc, row in
            rAcc
              + row.reduce(0) { cAcc, cell in
                cAcc + countOccurrences(of: needle, in: cell, caseSensitive: isFindCaseSensitive)
              }
          }
      }
      return paragraphTotal + tableTotal
    case .sheet(let sheet):
      return sheet.cellsByCoordinate.keys.sorted().reduce(0) { acc, coord in
        let raw = sheet.cellsByCoordinate[coord] ?? ""
        return acc + countOccurrences(of: needle, in: raw, caseSensitive: isFindCaseSensitive)
      }
    case .slide(let deck):
      return deck.slides.reduce(0) { acc, slide in
        var slideTotal = countOccurrences(
          of: needle,
          in: slide.title,
          caseSensitive: isFindCaseSensitive
        )
        slideTotal += countOccurrences(
          of: needle,
          in: slide.subtitle,
          caseSensitive: isFindCaseSensitive
        )
        for bullet in slide.bulletPoints {
          slideTotal += countOccurrences(
            of: needle,
            in: bullet,
            caseSensitive: isFindCaseSensitive
          )
        }
        for el in slide.elements {
          slideTotal += countOccurrences(
            of: needle,
            in: el.title,
            caseSensitive: isFindCaseSensitive
          )
          slideTotal += countOccurrences(
            of: needle,
            in: el.bodyText,
            caseSensitive: isFindCaseSensitive
          )
        }
        slideTotal += countOccurrences(
          of: needle,
          in: slide.speakerNotes,
          caseSensitive: isFindCaseSensitive
        )
        return acc + slideTotal
      }
    case .pdf(let pdf):
      return pdf.pages.reduce(0) { acc, page in
        acc
          + countOccurrences(
            of: needle,
            in: page.extractedText,
            caseSensitive: isFindCaseSensitive
          )
      }
    }
  }

  private func navigateSelectionToFindMatch(at targetOneBasedIndex: Int) {
    let needle = findQueryDraft.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !needle.isEmpty, targetOneBasedIndex >= 1, let tab = activeTab else { return }
    var running = 0
    switch tab.payload {
    case .document(let doc):
      for p in doc.paragraphs {
        let c = countOccurrences(of: needle, in: p.text, caseSensitive: isFindCaseSensitive)
        running += c
        if running >= targetOneBasedIndex {
          selectedParagraphID = p.id
          return
        }
      }
      for tbl in doc.tables {
        for row in tbl.rows {
          for cell in row {
            let c = countOccurrences(of: needle, in: cell, caseSensitive: isFindCaseSensitive)
            running += c
            if running >= targetOneBasedIndex {
              return
            }
          }
        }
      }
    case .sheet(let sheet):
      for coord in sheet.cellsByCoordinate.keys.sorted() {
        let raw = sheet.cellsByCoordinate[coord] ?? ""
        let c = countOccurrences(of: needle, in: raw, caseSensitive: isFindCaseSensitive)
        running += c
        if running >= targetOneBasedIndex {
          let a1 = coord.description
          selectedSheetCell = a1
          selectedSheetRange = a1
          formulaBarDraft = raw
          return
        }
      }
    case .slide(let deck):
      for (sIdx, slide) in deck.slides.enumerated() {
        var slideCount = countOccurrences(
          of: needle,
          in: slide.title,
          caseSensitive: isFindCaseSensitive
        )
        slideCount += countOccurrences(
          of: needle,
          in: slide.subtitle,
          caseSensitive: isFindCaseSensitive
        )
        for bullet in slide.bulletPoints {
          slideCount += countOccurrences(
            of: needle,
            in: bullet,
            caseSensitive: isFindCaseSensitive
          )
        }
        for el in slide.elements {
          slideCount += countOccurrences(
            of: needle,
            in: el.title,
            caseSensitive: isFindCaseSensitive
          )
          slideCount += countOccurrences(
            of: needle,
            in: el.bodyText,
            caseSensitive: isFindCaseSensitive
          )
        }
        slideCount += countOccurrences(
          of: needle,
          in: slide.speakerNotes,
          caseSensitive: isFindCaseSensitive
        )
        running += slideCount
        if running >= targetOneBasedIndex {
          selectedSlideIndex = sIdx
          return
        }
      }
    case .pdf(let pdf):
      for (pIdx, page) in pdf.pages.enumerated() {
        let c = countOccurrences(
          of: needle,
          in: page.extractedText,
          caseSensitive: isFindCaseSensitive
        )
        running += c
        if running >= targetOneBasedIndex {
          selectedPDFPageIndex = pIdx
          return
        }
      }
    }
  }

  public func findNextMatch() {
    let total = findMatchCount
    guard total > 0 else {
      currentFindMatchIndex = 0
      lastExecutedCommandSummary = "일치 항목 없음 (0건)"
      return
    }
    currentFindMatchIndex = (currentFindMatchIndex % total) + 1
    navigateSelectionToFindMatch(at: currentFindMatchIndex)
    lastExecutedCommandSummary = "다음 찾기 (\(currentFindMatchIndex)/\(total))"
  }

  public func findPreviousMatch() {
    let total = findMatchCount
    guard total > 0 else {
      currentFindMatchIndex = 0
      lastExecutedCommandSummary = "일치 항목 없음 (0건)"
      return
    }
    currentFindMatchIndex = currentFindMatchIndex <= 1 ? total : (currentFindMatchIndex - 1)
    navigateSelectionToFindMatch(at: currentFindMatchIndex)
    lastExecutedCommandSummary = "이전 찾기 (\(currentFindMatchIndex)/\(total))"
  }

  private func replaceTargetOccurrence(
    in text: inout String,
    needle: String,
    replacement: String,
    caseSensitive: Bool,
    currentCounter: inout Int,
    targetOneBasedIndex: Int
  ) -> Bool {
    guard !needle.isEmpty, !text.isEmpty else { return false }
    let options: String.CompareOptions = caseSensitive ? [] : [.caseInsensitive]
    var searchStart = text.startIndex
    while searchStart < text.endIndex,
      let range = text.range(
        of: needle,
        options: options,
        range: searchStart..<text.endIndex
      )
    {
      currentCounter += 1
      if currentCounter == targetOneBasedIndex {
        text.replaceSubrange(range, with: replacement)
        return true
      }
      if range.isEmpty {
        searchStart = text.index(after: range.lowerBound)
      } else {
        searchStart = range.upperBound
      }
    }
    return false
  }

  @discardableResult
  public func replaceCurrentMatch() -> Int {
    let needle = findQueryDraft.trimmingCharacters(in: .whitespacesAndNewlines)
    let replacement = replaceQueryDraft
    guard !needle.isEmpty,
      let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID })
    else { return 0 }
    let total = findMatchCount
    guard total > 0 else {
      currentFindMatchIndex = 0
      lastExecutedCommandSummary = "일치 항목 없음 (0건)"
      return 0
    }
    let targetOccurrence =
      (currentFindMatchIndex >= 1 && currentFindMatchIndex <= total) ? currentFindMatchIndex : 1
    var counter = 0
    var didReplace = false

    recordUndoSnapshot(for: activeTabID)

    switch openTabs[idx].payload {
    case .document(var doc):
      for pIdx in doc.paragraphs.indices {
        let countBeforeParagraph = counter
        if replaceTargetOccurrence(
          in: &doc.paragraphs[pIdx].text,
          needle: needle,
          replacement: replacement,
          caseSensitive: isFindCaseSensitive,
          currentCounter: &counter,
          targetOneBasedIndex: targetOccurrence
        ) {
          if !doc.paragraphs[pIdx].inlineRuns.isEmpty {
            let localTarget = max(1, targetOccurrence - countBeforeParagraph)
            var runCounter = 0
            for rIdx in doc.paragraphs[pIdx].inlineRuns.indices {
              if replaceTargetOccurrence(
                in: &doc.paragraphs[pIdx].inlineRuns[rIdx].text,
                needle: needle,
                replacement: replacement,
                caseSensitive: isFindCaseSensitive,
                currentCounter: &runCounter,
                targetOneBasedIndex: localTarget
              ) {
                break
              }
            }
          }
          selectedParagraphID = doc.paragraphs[pIdx].id
          didReplace = true
          break
        }
      }
      if !didReplace {
        tableLoop: for tIdx in doc.tables.indices {
          for rIdx in doc.tables[tIdx].rows.indices {
            for cIdx in doc.tables[tIdx].rows[rIdx].indices {
              if replaceTargetOccurrence(
                in: &doc.tables[tIdx].rows[rIdx][cIdx],
                needle: needle,
                replacement: replacement,
                caseSensitive: isFindCaseSensitive,
                currentCounter: &counter,
                targetOneBasedIndex: targetOccurrence
              ) {
                didReplace = true
                break tableLoop
              }
            }
          }
        }
      }
      if didReplace {
        openTabs[idx].payload = .document(doc)
      }

    case .sheet(var sheet):
      for coord in sheet.cellsByCoordinate.keys.sorted() {
        guard var raw = sheet.cellsByCoordinate[coord] else { continue }
        if replaceTargetOccurrence(
          in: &raw,
          needle: needle,
          replacement: replacement,
          caseSensitive: isFindCaseSensitive,
          currentCounter: &counter,
          targetOneBasedIndex: targetOccurrence
        ) {
          try? sheet.setCell(row: coord.row, column: coord.column, rawValue: raw)
          let a1 = coord.description
          selectedSheetCell = a1
          selectedSheetRange = a1
          formulaBarDraft = raw
          didReplace = true
          break
        }
      }
      if didReplace {
        openTabs[idx].payload = .sheet(sheet)
      }

    case .slide(var deck):
      slideLoop: for sIdx in deck.slides.indices {
        if replaceTargetOccurrence(
          in: &deck.slides[sIdx].title,
          needle: needle,
          replacement: replacement,
          caseSensitive: isFindCaseSensitive,
          currentCounter: &counter,
          targetOneBasedIndex: targetOccurrence
        ) {
          selectedSlideIndex = sIdx
          didReplace = true
          break slideLoop
        }
        if replaceTargetOccurrence(
          in: &deck.slides[sIdx].subtitle,
          needle: needle,
          replacement: replacement,
          caseSensitive: isFindCaseSensitive,
          currentCounter: &counter,
          targetOneBasedIndex: targetOccurrence
        ) {
          selectedSlideIndex = sIdx
          didReplace = true
          break slideLoop
        }
        for bIdx in deck.slides[sIdx].bulletPoints.indices {
          if replaceTargetOccurrence(
            in: &deck.slides[sIdx].bulletPoints[bIdx],
            needle: needle,
            replacement: replacement,
            caseSensitive: isFindCaseSensitive,
            currentCounter: &counter,
            targetOneBasedIndex: targetOccurrence
          ) {
            selectedSlideIndex = sIdx
            didReplace = true
            break slideLoop
          }
        }
        for eIdx in deck.slides[sIdx].elements.indices {
          if replaceTargetOccurrence(
            in: &deck.slides[sIdx].elements[eIdx].title,
            needle: needle,
            replacement: replacement,
            caseSensitive: isFindCaseSensitive,
            currentCounter: &counter,
            targetOneBasedIndex: targetOccurrence
          ) {
            selectedSlideIndex = sIdx
            didReplace = true
            break slideLoop
          }
          if replaceTargetOccurrence(
            in: &deck.slides[sIdx].elements[eIdx].bodyText,
            needle: needle,
            replacement: replacement,
            caseSensitive: isFindCaseSensitive,
            currentCounter: &counter,
            targetOneBasedIndex: targetOccurrence
          ) {
            selectedSlideIndex = sIdx
            didReplace = true
            break slideLoop
          }
        }
        if replaceTargetOccurrence(
          in: &deck.slides[sIdx].speakerNotes,
          needle: needle,
          replacement: replacement,
          caseSensitive: isFindCaseSensitive,
          currentCounter: &counter,
          targetOneBasedIndex: targetOccurrence
        ) {
          selectedSlideIndex = sIdx
          didReplace = true
          break slideLoop
        }
      }
      if didReplace {
        openTabs[idx].payload = .slide(deck)
      }

    case .pdf(var pdf):
      for pIdx in pdf.pages.indices {
        if replaceTargetOccurrence(
          in: &pdf.pages[pIdx].extractedText,
          needle: needle,
          replacement: replacement,
          caseSensitive: isFindCaseSensitive,
          currentCounter: &counter,
          targetOneBasedIndex: targetOccurrence
        ) {
          selectedPDFPageIndex = pIdx
          didReplace = true
          break
        }
      }
      if didReplace {
        openTabs[idx].payload = .pdf(pdf)
      }
    }

    guard didReplace else { return 0 }
    openTabs[idx].isModified = true
    lastFindReplaceCount = 1
    let updatedTotal = findMatchCount
    currentFindMatchIndex = updatedTotal == 0 ? 0 : min(targetOccurrence, updatedTotal)
    lastExecutedCommandSummary = "단건 바꾸기 완료 (1건 치환)"
    return 1
  }

  @discardableResult
  public func replaceAllFindMatches() -> Int {
    performFindAndReplace()
  }

  public func applyConditionalFormatRuleOnActiveSheet(_ rule: ConditionalFormatRule?) {
    activeConditionalFormatRule = rule
    persistSheetRulesToActiveTab()
  }

  public func applyDataValidationRuleOnActiveSheet(_ rule: DataValidationRule?) {
    activeDataValidationRule = rule
    persistSheetRulesToActiveTab()
  }

  public var sheetSelectionStatusSummary: String {
    guard let tab = activeTab, case .sheet(let sheet) = tab.payload else {
      return "선택 셀: 0개"
    }
    let originAddress = CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    let fallbackRange =
      (try? CellRange(notation: selectedSheetCell))
      ?? CellRange(start: originAddress, end: originAddress)
    let parsedRange = (try? CellRange(notation: selectedSheetRange)) ?? fallbackRange
    var summary = sheet.summarizeRange(parsedRange)
    if summary.numericCount == 0, parsedRange.start == parsedRange.end {
      let detected = sheet.summarizeRange(sheet.detectedNumericRange())
      if detected.numericCount > 0 {
        summary = detected
      }
    }
    if summary.numericCount > 0 {
      let sumText = Self.formatSheetStatusMetric(summary.sum)
      let avgText = Self.formatSheetStatusMetric(summary.average)
      let maxText = Self.formatSheetStatusMetric(summary.maximum ?? 0)
      return
        "합계: \(sumText) · 평균: \(avgText) · 개수: \(summary.numericCount) · 최대: \(maxText)"
    }
    let raw = sheet.rawValue(at: parsedRange.start.a1Notation)
    if parsedRange.start == parsedRange.end, !raw.isEmpty {
      return "선택 셀: 1개 · 값: \(raw)"
    }
    return "선택 셀: \(summary.cellCount)개 · 입력: \(summary.nonEmptyCount)개"
  }

  private static func formatSheetStatusMetric(_ value: Double) -> String {
    guard value.isFinite else { return "0" }
    let formatter = NumberFormatter()
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.numberStyle = .decimal
    if abs(value.rounded() - value) < 1e-6 {
      formatter.maximumFractionDigits = 0
      return formatter.string(from: NSNumber(value: value.rounded())) ?? "\(Int(value.rounded()))"
    }
    formatter.minimumFractionDigits = 0
    formatter.maximumFractionDigits = 2
    return formatter.string(from: NSNumber(value: value)) ?? String(format: "%.2f", value)
  }

  public func updatePDFFormFieldValue(fieldName: String, value: String) {
    let trimmedName = fieldName.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedName.isEmpty,
      let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdf) = openTabs[idx].payload
    else { return }
    if let existing = pdf.formFields.first(where: { $0.name == trimmedName }) {
      updatePDFFormField(id: existing.id, value: value)
      return
    }
    recordUndoSnapshot(for: activeTabID)
    let pageNumber = max(1, selectedPDFPageIndex + 1)
    pdf.addFormField(
      PDFFormField(
        pageNumber: pageNumber,
        name: trimmedName,
        value: value,
        isCheckbox: false,
        isChecked: false
      )
    )
    openTabs[idx].payload = .pdf(pdf)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "PDF 양식 필드 입력 (\(trimmedName) = \(value))"
  }

  public func updateSlideContent(
    slideIndex: Int? = nil,
    title: String? = nil,
    subtitle: String? = nil,
    bulletPoints: [String]? = nil
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload,
      !deck.slides.isEmpty
    else { return }
    let targetIdx =
      slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
    guard deck.slides.indices.contains(targetIdx) else { return }

    recordUndoSnapshot(
      for: activeTabID,
      coalescingTargetKey: "slide-content-\(activeTabID.uuidString)-\(targetIdx)"
    )
    if let title {
      deck.slides[targetIdx].title = title
    }
    if let subtitle {
      deck.slides[targetIdx].subtitle = subtitle
    }
    if let bulletPoints {
      deck.slides[targetIdx].bulletPoints = bulletPoints
    }
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 제목·부제목·본문 불릿 편집 반영"
  }

  public func setActiveSlideMasterLayout(_ layout: SlideMasterLayout) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload,
      !deck.slides.isEmpty
    else { return }
    let targetIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
    recordUndoSnapshot(for: activeTabID)
    deck.slides[targetIdx].masterLayout = layout
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 마스터 레이아웃 변경: \(layout.rawValue)"
  }

  public enum DocumentInsertionShapeKind: String, Sendable {
    case callout
    case rectangle
    case flowchart
  }

  public func insertFootnoteOnActiveDocument(noteText: String) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let anchorID = selectedParagraphID ?? doc.paragraphs.last?.id ?? UUID()
    let nextNum = doc.footnotes.count + 1
    doc.footnotes.append(
      FootnoteItem(number: nextNum, anchorParagraphID: anchorID, text: noteText)
    )
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "각주 삽입 완료 (#\(nextNum))"
  }

  public func insertTableIntoActiveDocument() {
    executeRibbonCommand("document-insert-table")
  }

  public func insertShapeOnActiveDocument(
    kind: DocumentInsertionShapeKind = .callout,
    caption: String = "다이어그램 도형"
  ) {
    insertSampleImageAttachment(caption: caption)
  }

  public func insertShapeOnActiveSlide(kind: SlideElementKind = .calloutShape) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload,
      !deck.slides.isEmpty
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
    let newEl = SlideCanvasElement(
      title: kind.rawValue,
      bodyText: "핵심 메시지 강조 도형",
      x: 520,
      y: 220,
      width: 260,
      height: 110,
      kind: kind,
      zIndex: deck.slides[sIdx].elements.count + 1
    )
    deck.slides[sIdx].elements.append(newEl)
    selectedSlideElementID = newEl.id
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 도형 추가: \(kind.rawValue)"
  }

  public func mergeActivePDFWithFile(url: URL) throws {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(let currentPDF) = openTabs[idx].payload
    else { return }
    let data = try Data(contentsOf: url)
    let externalPDF = try PDFBinarySerializer.decode(from: data)
    recordUndoSnapshot(for: activeTabID)
    let merged = currentPDF.merged(with: externalPDF, title: currentPDF.title)
    openTabs[idx].payload = .pdf(merged)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary =
      "외부 PDF 병합 완료 (\(externalPDF.pages.count)페이지 추가, 총 \(merged.pages.count)페이지)"
  }


  public func enqueueOpenTabsIntoBatchConversion(targetFormat: DocumentFormat? = nil) {
    if let targetFormat {
      batchTargetFormat = targetFormat
    }
    excludedOpenTabBatchIDs.removeAll()
    batchQueueGeneration &+= 1
    lastBatchConversionReport = nil
  }

  public func enqueueFilesFromDiskIntoBatchConversion(
    urls: [URL],
    targetFormat: DocumentFormat? = nil
  ) {
    if let targetFormat {
      batchTargetFormat = targetFormat
    }
    batchQueueGeneration &+= 1
    lastBatchConversionReport = nil
    var enqueuedCount = 0
    for url in urls {
      let stdPath = url.standardizedFileURL.path
      if additionalBatchJobs.contains(where: {
        $0.sourceFileURL?.standardizedFileURL.path == stdPath
      }) {
        continue
      }
      do {
        let rawData = try Data(contentsOf: url)
        let fileName = url.lastPathComponent
        let decoded = try decodeDocumentPayload(data: rawData, fileName: fileName)
        let safeSourceName =
          fileName.contains(".") ? fileName : "\(fileName).\(decoded.format.fileExtension)"
        additionalBatchJobs.append(
          ConversionJobRequest(
            sourceFileName: safeSourceName,
            sourceFileURL: url,
            sourceFormat: decoded.format,
            targetFormat: batchTargetFormat,
            payload: decoded.payload
          )
        )
        enqueuedCount += 1
      } catch {
        lastErrorMessage =
          (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
      }
    }
    if enqueuedCount > 0 {
      lastExecutedCommandSummary = "일괄 변환 대기열 파일 추가: \(enqueuedCount)건"
    }
    isShowingHomeHub = true
    activeHubSection = .converter
  }

  public func removeBatchConversionJob(id: UUID) {
    additionalBatchJobs.removeAll { $0.id == id }
    if openTabs.contains(where: { $0.id == id }) {
      excludedOpenTabBatchIDs.insert(id)
    }
    batchQueueGeneration &+= 1
    lastBatchConversionReport = nil
  }

  public func clearBatchConversionQueue() {
    additionalBatchJobs.removeAll()
    excludedOpenTabBatchIDs = Set(openTabs.map(\.id))
    batchQueueGeneration &+= 1
    lastBatchConversionReport = nil
    lastExecutedCommandSummary = "일괄 변환 대기열 초기화 완료"
  }

  internal func buildDeduplicatedBatchJobs() -> [ConversionJobRequest] {
    var jobs: [ConversionJobRequest] = []
    var seenPaths = Set<String>()
    var seenIDs = Set<UUID>()

    for tab in openTabs where !excludedOpenTabBatchIDs.contains(tab.id) {
      if let stdPath = tab.fileURL?.standardizedFileURL.path {
        if !seenPaths.insert(stdPath).inserted { continue }
      }
      guard seenIDs.insert(tab.id).inserted else { continue }
      let safeSourceName =
        tab.fileName.contains(".") ? tab.fileName : "\(tab.fileName).\(tab.format.fileExtension)"
      jobs.append(
        ConversionJobRequest(
          id: tab.id,
          sourceFileName: safeSourceName,
          sourceFileURL: tab.fileURL,
          sourceFormat: tab.format,
          targetFormat: batchTargetFormat,
          payload: tab.payload
        )
      )
    }

    for extra in additionalBatchJobs {
      if let stdPath = extra.sourceFileURL?.standardizedFileURL.path {
        if !seenPaths.insert(stdPath).inserted { continue }
      }
      guard seenIDs.insert(extra.id).inserted else { continue }
      jobs.append(
        ConversionJobRequest(
          id: extra.id,
          sourceFileName: extra.sourceFileName,
          sourceFileURL: extra.sourceFileURL,
          sourceFormat: extra.sourceFormat,
          targetFormat: batchTargetFormat,
          payload: extra.payload
        )
      )
    }
    return jobs
  }

  public func runAllBatchConversions(outputDirectory: URL? = nil) {
    batchQueueGeneration &+= 1
    let jobs = buildDeduplicatedBatchJobs()
    let resolvedDir = outputDirectory ?? batchOutputDirectoryURL
    lastBatchConversionReport = DocumentConversionOrchestrator.executeBatch(
      jobs,
      outputDirectory: resolvedDir
    )
  }

  public func runAllBatchConversionsAsync(outputDirectory: URL? = nil) async {
    batchQueueGeneration &+= 1
    let generationSnapshot = batchQueueGeneration
    let jobs = buildDeduplicatedBatchJobs()
    let resolvedDir = outputDirectory ?? batchOutputDirectoryURL
    let report = await DocumentConversionOrchestrator.executeBatchAsync(
      jobs,
      outputDirectory: resolvedDir
    )
    if batchQueueGeneration == generationSnapshot {
      lastBatchConversionReport = report
    }
  }

  public var batchConversionQueue: [BatchConversionQueueItem] {
    guard let report = lastBatchConversionReport else {
      return buildDeduplicatedBatchJobs().map { job in
        BatchConversionQueueItem(
          id: job.id,
          sourceFileName: job.sourceFileName,
          sourceFormat: job.sourceFormat,
          targetFormat: batchTargetFormat,
          status: .pending
        )
      }
    }
    var items: [BatchConversionQueueItem] = report.succeededResults.map { res in
      BatchConversionQueueItem(
        id: res.id,
        sourceFileName: res.sourceFileName,
        sourceFormat: res.sourceFormat,
        targetFormat: res.targetFormat,
        status: .completed,
        convertedPayloadBytes: res.outputBytes,
        result: res
      )
    }
    for failed in report.failedErrors {
      let matchedTab = openTabs.first(where: { $0.id == failed.jobID })
      let matchedExtra = additionalBatchJobs.first(where: { $0.id == failed.jobID })
      items.append(
        BatchConversionQueueItem(
          id: failed.jobID,
          sourceFileName: matchedTab?.fileName ?? matchedExtra?.sourceFileName ?? "변환 작업",
          sourceFormat: matchedTab?.format ?? matchedExtra?.sourceFormat ?? .docx,
          targetFormat: batchTargetFormat,
          status: .failed,
          errorDescription: failed.errorDescription
        )
      )
    }
    return items
  }

  public func exportBatchConversionItem(result: ConversionJobResult, to url: URL) throws {
    try result.outputBytes.write(to: url, options: .atomic)
    lastExecutedCommandSummary = "변환 결과 파일 저장 완료: \(url.lastPathComponent)"
    lastErrorMessage = nil
  }

  @discardableResult
  public func exportAllCompletedBatchItems(toDirectory directoryURL: URL) throws -> [URL] {
    if lastBatchConversionReport == nil {
      runAllBatchConversions(outputDirectory: directoryURL)
    }
    guard let report = lastBatchConversionReport else { return [] }
    try FileManager.default.createDirectory(at: directoryURL, withIntermediateDirectories: true)
    var writtenURLs: [URL] = []
    var usedLowercasedNames: Set<String> = []
    for result in report.succeededResults {
      let uniqueFileName = DocumentConversionOrchestrator.disambiguateOutputFileName(
        result.outputFileName,
        usedLowercasedNames: &usedLowercasedNames
      )
      let outURL = directoryURL.appendingPathComponent(uniqueFileName)
      try result.outputBytes.write(to: outURL, options: .atomic)
      writtenURLs.append(outURL)
    }
    lastExecutedCommandSummary = "일괄 변환 결과 \(writtenURLs.count)건 폴더 저장 완료"
    lastErrorMessage = nil
    return writtenURLs
  }

  @discardableResult
  public func compareOpenDocumentTabs(
    baselineTabID: UUID? = nil,
    revisedTabID: UUID? = nil
  ) -> DocumentDiffReport? {
    let docTabs: [(UUID, String, OfficeDocumentModel)] = openTabs.compactMap { tab in
      if case .document(let doc) = tab.payload {
        return (tab.id, tab.fileName, doc)
      }
      return nil
    }
    guard docTabs.count >= 2 else {
      lastErrorMessage = "문서 비교를 실행하려면 최소 2개의 문서 탭이 열려 있어야 합니다."
      lastExecutedCommandSummary = "문서 비교 불가: 비교 대상 문서 탭 부족 (최소 2개 필요)"
      return nil
    }
    let baseEntry =
      docTabs.first(where: { $0.0 == baselineTabID }) ?? docTabs[0]
    let revEntry =
      docTabs.first(where: { $0.0 == revisedTabID && $0.0 != baseEntry.0 })
      ?? docTabs.first(where: { $0.0 != baseEntry.0 })
      ?? docTabs[1]
    guard baseEntry.0 != revEntry.0 else {
      lastErrorMessage = "동일한 문서 탭끼리는 비교할 수 없습니다. 서로 다른 두 문서를 선택하세요."
      return nil
    }
    let report = LocalSmartDocumentAssistant.compareDocuments(
      baseline: baseEntry.2,
      revised: revEntry.2
    )
    lastErrorMessage = nil
    if report.isTruncated, let limit = report.truncatedAtParagraphLimit {
      lastExecutedCommandSummary =
        "문서 비교 완료 (\(baseEntry.1) vs \(revEntry.1), 최대 \(limit)문단 상한 적용)"
    } else {
      lastExecutedCommandSummary = "문서 비교 완료 (\(baseEntry.1) vs \(revEntry.1))"
    }
    return report
  }
}

