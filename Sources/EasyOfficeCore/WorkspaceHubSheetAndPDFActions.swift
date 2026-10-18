import EasyOfficeDesignSystem
import Foundation

public enum PDFStampPreset: String, CaseIterable, Sendable {
  case approved = "승인 (APPROVED)"
  case confidential = "대외비 (CONFIDENTIAL)"
  case reviewed = "검토필 (REVIEWED)"
  case urgent = "긴급 (URGENT)"
}

extension PDFAnnotationKind {
  public static var signatureStamp: PDFAnnotationKind { .signature }
}

extension PDFFormField {
  public var fieldName: String {
    get { name }
    set { name = newValue }
  }
}

extension PDFPageDescriptor {
  public var title: String {
    "Page \(pageNumber)"
  }

  public var bodyText: String {
    get { extractedText }
    set { extractedText = newValue }
  }
}

extension PDFAnnotationItem {
  public var contents: String {
    get { content }
    set { content = newValue }
  }

  public init(
    id: UUID = UUID(),
    pageNumber: Int,
    kind: PDFAnnotationKind,
    author: String = "Workspace User",
    contents: String
  ) {
    self.init(
      id: id,
      pageNumber: pageNumber,
      kind: kind,
      content: contents,
      author: author
    )
  }
}

extension PDFStudioDocumentModel {
  public mutating func addInkStroke(
    pageNumber: Int,
    points: [PDFInkPoint],
    strokeHex: String = "#5C6BC0",
    lineWidth: Double = 2.2,
    label: String = "자유곡선 전자서명"
  ) {
    addInkStroke(
      PDFInkStroke(
        pageNumber: pageNumber,
        points: points,
        strokeHex: strokeHex,
        lineWidth: lineWidth,
        label: label
      )
    )
  }

  public mutating func addAnnotation(
    pageNumber: Int,
    kind: PDFAnnotationKind,
    author: String = "Workspace User",
    contents: String
  ) {
    annotations.append(
      PDFAnnotationItem(pageNumber: pageNumber, kind: kind, content: contents, author: author)
    )
  }
}

extension WorkspaceViewModel {

  // MARK: - Spreadsheet Data Cleansing, Frozen Column, Data Bars, Named Range & Goal Seek Actions

  @discardableResult
  public func removeDuplicatesInActiveSheet() -> Int {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return 0 }
    recordUndoSnapshot(for: activeTabID)
    let range =
      selectedSheetRange.contains(":") ? (try? CellRange(a1Range: selectedSheetRange)) : nil
    var removed = sheet.removeDuplicateRows(in: range)
    if removed == 0, range != nil {
      removed = sheet.removeDuplicateRows(in: nil)
    }
    formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "시트 중복 행 \(removed)건 제거 완료"
    return removed
  }

  @discardableResult
  public func splitActiveSheetColumnByDelimiter(_ delimiter: String? = nil) -> Int {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return 0 }
    recordUndoSnapshot(for: activeTabID)
    let rawDelim = delimiter ?? sheetDelimiterDraft
    let delim = rawDelim.isEmpty ? "," : rawDelim
    let col = (try? CellAddress(a1Notation: selectedSheetCell))?.column ?? 0
    let count = sheet.splitColumnByDelimiter(column: col, delimiter: delim)
    formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "텍스트 나누기 완료 (\(count)행 분할)"
    return count
  }

  public func toggleFirstColumnFrozen() {
    isFirstColumnFrozen.toggle()
    lastExecutedCommandSummary = isFirstColumnFrozen ? "첫 열(A열) 틀 고정 활성화" : "첫 열(A열) 틀 고정 해제"
  }

  public func toggleSheetDataBars() {
    isDataBarsEnabled.toggle()
    lastExecutedCommandSummary = isDataBarsEnabled ? "셀 내 데이터 막대 표시 활성화" : "셀 내 데이터 막대 표시 해제"
  }

  private func mutateActiveSheetAtSelection(
    summary: (CellAddress) -> String,
    mutation: (inout WorksheetModel, CellAddress) -> Void
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let addr =
      (try? CellAddress(a1Notation: selectedSheetCell))
      ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    mutation(&sheet, addr)
    formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = summary(addr)
  }

  public func insertSheetRowAtSelection() {
    mutateActiveSheetAtSelection(
      summary: { "시트 행 삽입 완료 (\($0.row + 1)행)" },
      mutation: { sheet, addr in sheet.insertRow(at: addr.row) }
    )
  }

  public func deleteSheetRowAtSelection() {
    mutateActiveSheetAtSelection(
      summary: { "시트 행 삭제 완료 (\($0.row + 1)행)" },
      mutation: { sheet, addr in sheet.deleteRow(at: addr.row) }
    )
  }

  public func insertSheetColumnAtSelection() {
    mutateActiveSheetAtSelection(
      summary: { "시트 열 삽입 완료 (\(CellAddress.columnLabel(for: $0.column))열)" },
      mutation: { sheet, addr in sheet.insertColumn(at: addr.column) }
    )
  }

  public func deleteSheetColumnAtSelection() {
    mutateActiveSheetAtSelection(
      summary: { "시트 열 삭제 완료 (\(CellAddress.columnLabel(for: $0.column))열)" },
      mutation: { sheet, addr in sheet.deleteColumn(at: addr.column) }
    )
  }

  public func defineNamedRangeOnActiveSheet(
    name: String? = nil,
    targetNotation: String? = nil
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    let candidateName = (name ?? namedRangeNameDraft).trimmingCharacters(
      in: .whitespacesAndNewlines
    )
    let resolvedName = candidateName.isEmpty ? "분기매출" : candidateName
    let candidateTarget = (targetNotation ?? namedRangeTargetDraft).trimmingCharacters(
      in: .whitespacesAndNewlines
    )
    let resolvedTarget = candidateTarget.isEmpty ? selectedSheetRange : candidateTarget
    recordUndoSnapshot(for: activeTabID)
    sheet.setNamedRange(resolvedName, rangeOrCellNotation: resolvedTarget)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "이름 정의 등록 완료: \(resolvedName.uppercased()) = \(resolvedTarget)"
  }

  public func defineSheetNamedRange(name: String? = nil, targetNotation: String? = nil) {
    defineNamedRangeOnActiveSheet(name: name, targetNotation: targetNotation)
  }

  public func mergeSelectedSheetRange() {
    toggleMergeOnSelectedSheetCell()
  }

  public func unmergeSelectedSheetRange() {
    toggleMergeOnSelectedSheetCell()
  }

  @discardableResult
  public func runGoalSeekOnActiveSheet(
    targetCell: String? = nil,
    targetValue: Double? = nil,
    variableCell: String? = nil
  ) -> Double? {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return nil }
    let rawTCell = (targetCell ?? goalSeekTargetCellDraft).trimmingCharacters(
      in: .whitespacesAndNewlines
    )
    let tCell = rawTCell.isEmpty ? "B4" : rawTCell
    let tVal =
      targetValue
      ?? Double(goalSeekTargetValueDraft.trimmingCharacters(in: .whitespacesAndNewlines))
      ?? 500.0
    let rawVCell = (variableCell ?? goalSeekVariableCellDraft).trimmingCharacters(
      in: .whitespacesAndNewlines
    )
    let vCell = rawVCell.isEmpty ? "B2" : rawVCell
    recordUndoSnapshot(for: activeTabID)
    if let solved = try? sheet.goalSeek(
      targetCell: tCell,
      targetValue: tVal,
      variableCell: vCell
    ) {
      openTabs[idx].payload = .sheet(sheet)
      openTabs[idx].isModified = true
      formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
      lastExecutedCommandSummary =
        "목표값 찾기 완료: \(vCell) = \(String(format: "%.2f", solved)) (목표 \(tCell) = \(tVal))"
      return solved
    } else {
      lastExecutedCommandSummary = "목표값 찾기 수렴 실패 (\(tCell))"
      return nil
    }
  }

  // MARK: - PDF Studio Stamps, Bookmarks, Redaction, Form Fields, Page Reordering & Merging

  public func addStampToActivePDFPage(preset: PDFStampPreset) {
    addStampToActivePDFPage(preset: preset.rawValue)
  }

  public func addStampToActivePDFPage(preset: String? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdf) = openTabs[idx].payload
    else { return }
    let stampText = preset ?? selectedPDFStampPreset
    recordUndoSnapshot(for: activeTabID)
    pdf.annotations.append(
      PDFAnnotationItem(
        pageNumber: selectedPDFPageNumber,
        kind: .stamp,
        author: authorName,
        contents: "[결재 스탬프] \(stampText)"
      )
    )
    openTabs[idx].payload = .pdf(pdf)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "PDF 결재 스탬프 날인 (\(stampText))"
  }

  public func addBookmarkToActivePDF(title: String? = nil, parentID: UUID? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdf) = openTabs[idx].payload
    else { return }
    let t = (title ?? bookmarkTitleDraft).trimmingCharacters(in: .whitespacesAndNewlines)
    let resolvedTitle = t.isEmpty ? "페이지 \(selectedPDFPageNumber) 책갈피" : t
    recordUndoSnapshot(for: activeTabID)
    pdf.addBookmark(
      title: resolvedTitle,
      targetPageNumber: selectedPDFPageNumber,
      parentID: parentID
    )
    openTabs[idx].payload = .pdf(pdf)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "PDF 책갈피 추가 완료: \(resolvedTitle)"
  }

  public func removeBookmarkFromActivePDF(id: UUID) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdf) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    pdf.removeBookmark(id: id)
    openTabs[idx].payload = .pdf(pdf)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "PDF 책갈피 삭제 완료"
  }

  public func rotateActivePDFPage(clockwiseDegrees: Int = 90) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdfDoc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let targetIndex = pdfDoc.pages.indices.contains(selectedPDFPageIndex) ? selectedPDFPageIndex : 0
    pdfDoc.rotatePage(at: targetIndex, clockwiseDegrees: clockwiseDegrees)
    openTabs[idx].payload = .pdf(pdfDoc)
    openTabs[idx].isModified = true
  }

  public func insertBlankPDFPage(afterPage: Int? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdfDoc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let targetAfterPage = afterPage ?? selectedPDFPageNumber
    pdfDoc.insertBlankPage(afterPage: targetAfterPage)
    selectedPDFPageIndex = min(pdfDoc.pages.count - 1, targetAfterPage)
    openTabs[idx].payload = .pdf(pdfDoc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "빈 PDF 페이지 추가 완료"
  }

  public func deleteActivePDFPage() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdfDoc) = openTabs[idx].payload,
      pdfDoc.pages.count > 1
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let targetIndex = pdfDoc.pages.indices.contains(selectedPDFPageIndex) ? selectedPDFPageIndex : 0
    pdfDoc.deletePage(at: targetIndex)
    selectedPDFPageIndex = max(0, min(pdfDoc.pages.count - 1, selectedPDFPageIndex))
    openTabs[idx].payload = .pdf(pdfDoc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "현재 PDF 페이지 삭제 완료"
  }

  public func addPDFAnnotation(kind: PDFAnnotationKind, content: String) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdfDoc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let pageNumber =
      pdfDoc.pages.indices.contains(selectedPDFPageIndex)
      ? pdfDoc.pages[selectedPDFPageIndex].pageNumber : 1
    pdfDoc.annotations.append(
      PDFAnnotationItem(pageNumber: pageNumber, kind: kind, content: content, author: authorName)
    )
    openTabs[idx].payload = .pdf(pdfDoc)
    openTabs[idx].isModified = true
  }

  public func addInkStrokeToActivePDFPage(
    points: [PDFInkPoint],
    strokeHex: String? = nil,
    lineWidth: Double? = nil,
    label: String = "자유곡선 전자서명"
  ) {
    guard !points.isEmpty,
      let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdfDoc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    pdfDoc.addInkStroke(
      pageNumber: selectedPDFPageNumber,
      points: points,
      strokeHex: strokeHex ?? pdfInkStrokeColorHex,
      lineWidth: lineWidth ?? pdfInkLineWidth,
      label: label
    )
    openTabs[idx].payload = .pdf(pdfDoc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "PDF 자유곡선 벡터 서명 추가 (\(selectedPDFPageNumber)p)"
  }

  public func clearInkStrokesOnActivePDFPage() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdfDoc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    pdfDoc.clearInkStrokes(onPage: selectedPDFPageNumber)
    openTabs[idx].payload = .pdf(pdfDoc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "PDF 자유곡선 서명 지우기 (\(selectedPDFPageNumber)p)"
  }

  public func addHighlightToActivePDFPage(selectedText: String) {
    let trimmed = selectedText.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty,
      let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdfDoc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    pdfDoc.addAnnotation(
      pageNumber: selectedPDFPageNumber,
      kind: .highlight,
      author: authorName,
      contents: "하이라이트: \(trimmed)"
    )
    openTabs[idx].payload = .pdf(pdfDoc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "PDF 선택 텍스트 하이라이트 추가 (\(selectedPDFPageNumber)p)"
  }

  @discardableResult
  public func redactSensitiveTextOnActivePDF(phrase: String? = nil) -> Int {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdf) = openTabs[idx].payload
    else { return 0 }
    let candidate = (phrase ?? pdfRedactionPhraseDraft).trimmingCharacters(
      in: .whitespacesAndNewlines
    )
    let resolvedPhrase = candidate.isEmpty ? "기밀" : candidate
    recordUndoSnapshot(for: activeTabID)
    let count = pdf.redactText(resolvedPhrase)
    openTabs[idx].payload = .pdf(pdf)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "PDF 민감 구문 리댁션 완료 (\(count)페이지 적용)"
    return count
  }

  @discardableResult
  public func redactTextOnActivePDF(phrase: String? = nil) -> Int {
    redactSensitiveTextOnActivePDF(phrase: phrase)
  }

  @discardableResult
  public func redactTextInActivePDF(phrase: String? = nil) -> Int {
    redactSensitiveTextOnActivePDF(phrase: phrase)
  }

  public func addFormFieldToActivePDF(
    name: String? = nil,
    value: String? = nil,
    isCheckbox: Bool = false
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdf) = openTabs[idx].payload
    else { return }
    let candidateName = (name ?? pdfFormFieldNameDraft).trimmingCharacters(
      in: .whitespacesAndNewlines
    )
    let resolvedName = candidateName.isEmpty ? "승인자" : candidateName
    let resolvedVal = value ?? pdfFormFieldValueDraft
    recordUndoSnapshot(for: activeTabID)
    pdf.addFormField(
      PDFFormField(
        pageNumber: selectedPDFPageNumber,
        name: resolvedName,
        value: resolvedVal,
        isCheckbox: isCheckbox,
        isChecked: isCheckbox
      )
    )
    openTabs[idx].payload = .pdf(pdf)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "PDF 양식 필드 추가 완료: \(resolvedName)"
  }

  public func updatePDFFormField(id: UUID, value: String? = nil, isChecked: Bool? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdf) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(
      for: activeTabID,
      coalescingTargetKey: "pdf-field-\(activeTabID.uuidString)-\(id.uuidString)"
    )
    pdf.updateFormField(id: id, value: value, isChecked: isChecked)
    openTabs[idx].payload = .pdf(pdf)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "PDF 양식 필드 값 갱신 완료"
  }

  public func setActivePDFFormFieldValue(id: UUID, value: String? = nil, isChecked: Bool? = nil) {
    updatePDFFormField(id: id, value: value, isChecked: isChecked)
  }

  public func moveActivePDFPage(offset: Int) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdf) = openTabs[idx].payload,
      !pdf.pages.isEmpty
    else { return }
    let sourceIndex = max(0, min(pdf.pages.count - 1, selectedPDFPageNumber - 1))
    let targetIndex = max(0, min(pdf.pages.count - 1, sourceIndex + offset))
    guard targetIndex != sourceIndex else { return }
    recordUndoSnapshot(for: activeTabID)
    pdf.reorderPage(from: sourceIndex, to: targetIndex)
    selectedPDFPageNumber = targetIndex + 1
    openTabs[idx].payload = .pdf(pdf)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "PDF 페이지 순서 이동 완료 (\(sourceIndex + 1)p → \(targetIndex + 1)p)"
  }

  public func reorderActivePDFPage(offset: Int) {
    moveActivePDFPage(offset: offset)
  }

  public func reorderActivePDFPage(from sourceIndex: Int, to destinationIndex: Int) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdf) = openTabs[idx].payload,
      !pdf.pages.isEmpty
    else { return }
    let clampedSrc = max(0, min(pdf.pages.count - 1, sourceIndex))
    let clampedDst = max(0, min(pdf.pages.count - 1, destinationIndex))
    guard clampedSrc != clampedDst else { return }
    recordUndoSnapshot(for: activeTabID)
    pdf.reorderPage(from: clampedSrc, to: clampedDst)
    selectedPDFPageNumber = clampedDst + 1
    openTabs[idx].payload = .pdf(pdf)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "PDF 페이지 순서 이동 완료 (\(clampedSrc + 1)p → \(clampedDst + 1)p)"
  }

  public func mergeActivePDFWithAnotherOpenTab() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdf) = openTabs[idx].payload
    else { return }
    let otherPDF: PDFStudioDocumentModel
    if let otherPDFTab = openTabs.first(where: {
      if $0.id == activeTabID { return false }
      if case .pdf = $0.payload { return true }
      return false
    }), case .pdf(let foundPDF) = otherPDFTab.payload {
      otherPDF = foundPDF
    } else if let otherTab = openTabs.first(where: { $0.id != activeTabID }) {
      switch otherTab.payload {
      case .document(let doc):
        otherPDF = PDFStudioDocumentModel.fromDocument(doc)
      case .sheet(let sheet):
        otherPDF = PDFStudioDocumentModel.fromWorksheet(sheet)
      case .slide(let deck):
        otherPDF = PDFStudioDocumentModel.fromPresentation(deck)
      case .pdf(let foundPDF):
        otherPDF = foundPDF
      }
    } else {
      otherPDF = PDFStudioDocumentModel(
        title: "\(pdf.title) 부록",
        pages: [
          PDFPageDescriptor(
            pageNumber: 1,
            rotationDegrees: 0,
            extractedText: "병합된 추가 부록 페이지"
          )
        ]
      )
    }
    recordUndoSnapshot(for: activeTabID)
    pdf = pdf.merged(with: otherPDF)
    openTabs[idx].payload = .pdf(pdf)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "열린 탭 PDF 병합 완료 (총 \(pdf.pages.count)p)"
  }

  public func mergeActivePDFWithTab() {
    mergeActivePDFWithAnotherOpenTab()
  }

  public func applyQuickNumberFormatOnSelectedSheetCell(_ format: CellNumberFormat) {
    setSelectedSheetCellNumberFormat(format)
  }

  public func adjustSelectedSheetCellDecimalPlaces(delta: Int) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    var style = sheet.cellStyle(at: selectedSheetCell)
    let current = style.decimalPlaces ?? 2
    let updatedPlaces = max(0, min(6, current + delta))
    style.decimalPlaces = updatedPlaces
    if (try? sheet.setCellStyle(style, at: selectedSheetCell)) == nil {
      sheet.setCellStyle(style, at: PackedCellCoordinate(row: 0, column: 0))
    }
    if let range = try? CellRange(notation: selectedSheetRange), range.start != range.end {
      sheet.updateCellStyleInRange(range) { $0.decimalPlaces = updatedPlaces }
    }
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "셀 소수점 자릿수 설정: \(updatedPlaces)자리"
  }

  public func toggleWrapTextOnSelectedSheetCell() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    var style = sheet.cellStyle(at: selectedSheetCell)
    let newWrap = !style.wrapText
    style.wrapText = newWrap
    if (try? sheet.setCellStyle(style, at: selectedSheetCell)) == nil {
      sheet.setCellStyle(style, at: PackedCellCoordinate(row: 0, column: 0))
    }
    if let range = try? CellRange(notation: selectedSheetRange), range.start != range.end {
      sheet.updateCellStyleInRange(range) { $0.wrapText = newWrap }
    }
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "셀 텍스트 줄바꿈 \(newWrap ? "활성화" : "해제")"
  }

  public func applyWatermarkOnActivePDF(text: String? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .pdf(var pdf) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let resolved = (text ?? pdfWatermarkDraft).trimmingCharacters(in: .whitespacesAndNewlines)
    pdf.applyWatermark(resolved)
    openTabs[idx].payload = .pdf(pdf)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary =
      resolved.isEmpty ? "PDF 워터마크 해제 완료" : "PDF 워터마크 적용: \(resolved)"
  }

  public func sortActiveSheetMultiColumn(
    primaryColumn: Int,
    ascending: Bool = true,
    secondaryColumn: Int,
    secondaryAscending: Bool = true
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    sheet.sortRows(
      byColumn: primaryColumn,
      ascending: ascending,
      hasHeader: true,
      secondaryColumn: secondaryColumn,
      secondaryAscending: secondaryAscending
    )
    formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    let pLabel = CellAddress.columnLabel(for: max(0, primaryColumn))
    let sLabel = CellAddress.columnLabel(for: max(0, secondaryColumn))
    lastExecutedCommandSummary =
      "다중 기준 행 정렬 완료 (1차 \(pLabel)열 \(ascending ? "오름차순" : "내림차순"), 2차 \(sLabel)열 \(secondaryAscending ? "오름차순" : "내림차순"))"
  }

  public func sortActiveSheetBySecondaryColumn(
    primaryColumn: Int,
    ascending: Bool = true,
    secondaryColumn: Int,
    secondaryAscending: Bool = true
  ) {
    sortActiveSheetMultiColumn(
      primaryColumn: primaryColumn,
      ascending: ascending,
      secondaryColumn: secondaryColumn,
      secondaryAscending: secondaryAscending
    )
  }

  // MARK: - Interactive Find & Replace Navigation & Single Replacement

  @discardableResult
  public func recomputeFindMatches() -> Int {
    let total = findMatchCount
    if total == 0 {
      currentFindMatchIndex = 0
    } else if currentFindMatchIndex < 1 || currentFindMatchIndex > total {
      currentFindMatchIndex = 1
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
      return doc.paragraphs.reduce(0) { acc, p in
        acc + countOccurrences(of: needle, in: p.text, caseSensitive: isFindCaseSensitive)
      }
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
}
