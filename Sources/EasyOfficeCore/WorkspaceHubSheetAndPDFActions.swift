import EasyOfficeDesignSystem
import Foundation

public enum PDFStampPreset: String, CaseIterable, Sendable {
  case approved = "승인 (APPROVED)"
  case confidential = "대외비 (CONFIDENTIAL)"
  case reviewed = "검토필 (REVIEWED)"
  case urgent = "긴급 (URGENT)"
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
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    var style = sheet.cellStyle(at: selectedSheetCell)
    style.numberFormat = format
    if (try? sheet.setCellStyle(style, at: selectedSheetCell)) == nil {
      sheet.setCellStyle(style, at: PackedCellCoordinate(row: 0, column: 0))
    }
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "셀 숫자 서식 적용: \(format.rawValue)"
  }

  public func adjustSelectedSheetCellDecimalPlaces(delta: Int) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    var style = sheet.cellStyle(at: selectedSheetCell)
    let current = style.decimalPlaces ?? 2
    style.decimalPlaces = max(0, min(6, current + delta))
    if (try? sheet.setCellStyle(style, at: selectedSheetCell)) == nil {
      sheet.setCellStyle(style, at: PackedCellCoordinate(row: 0, column: 0))
    }
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "셀 소수점 자릿수 설정: \(style.decimalPlaces ?? 2)자리"
  }

  public func toggleWrapTextOnSelectedSheetCell() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    var style = sheet.cellStyle(at: selectedSheetCell)
    style.wrapText.toggle()
    if (try? sheet.setCellStyle(style, at: selectedSheetCell)) == nil {
      sheet.setCellStyle(style, at: PackedCellCoordinate(row: 0, column: 0))
    }
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "셀 텍스트 줄바꿈 \(style.wrapText ? "활성화" : "해제")"
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
}
