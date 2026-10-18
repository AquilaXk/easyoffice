import EasyOfficeDesignSystem
import Foundation

public typealias FootnoteEntry = FootnoteItem

extension FootnoteItem {
  public var content: String {
    get { text }
    set { text = newValue }
  }

  public init(
    id: UUID = UUID(),
    number: Int,
    referenceParagraphIndex: Int = 0,
    content: String,
    isEndnote: Bool = false,
    anchorParagraphID: UUID = UUID()
  ) {
    self.init(
      id: id,
      number: number,
      isEndnote: isEndnote,
      anchorParagraphID: anchorParagraphID,
      text: content
    )
  }
}

extension SlideModel {
  public init(
    id: UUID = UUID(),
    slideNumber: Int,
    title: String,
    subtitle: String = "",
    bulletPoints: [String] = [],
    speakerNotes: String = "",
    masterLayout: SlideMasterLayout = .titleAndContent,
    transition: SlideTransitionKind = .fade,
    elements: [SlideCanvasElement] = []
  ) {
    self.init(
      id: id,
      title: title,
      subtitle: subtitle,
      bulletPoints: bulletPoints,
      speakerNotes: speakerNotes,
      masterLayout: masterLayout,
      transition: transition,
      elements: elements
    )
  }
}

extension WorkspaceViewModel {

  // MARK: - Document & HWP Interactive Actions

  public func insertHyperlinkIntoActiveParagraph(url: String? = nil) {
    let trimmed = (url ?? inlineLinkURLDraft).trimmingCharacters(in: .whitespacesAndNewlines)
    let resolvedURL = trimmed.isEmpty ? "https://easyoffice.local/reference" : trimmed
    applyInlineRunFormattingToActiveParagraph(
      isUnderlined: true,
      foregroundHex: "#1E88E5",
      linkURL: resolvedURL
    )
    lastExecutedCommandSummary = "하이퍼링크 삽입 완료: \(resolvedURL)"
  }

  public func toggleHighlightOnActiveParagraph(colorHex: String? = nil) {
    let resolvedHex = colorHex ?? inlineHighlightColorHex
    applyInlineRunFormattingToActiveParagraph(backgroundHex: resolvedHex)
    lastExecutedCommandSummary = "텍스트 형광펜 강조 적용: \(resolvedHex)"
  }

  public func toggleStrikethroughOnActiveParagraph() {
    applyInlineRunFormattingToActiveParagraph(isStrikethrough: true)
    lastExecutedCommandSummary = "취소선 서식 적용 완료"
  }

  public func adjustActiveParagraphIndent(
    firstLineDeltaPt: Double = 0,
    leftIndentDeltaPt: Double = 0
  ) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      !doc.paragraphs.isEmpty
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let pIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
    doc.paragraphs[pIdx].firstLineIndentPt = max(
      0,
      min(144, doc.paragraphs[pIdx].firstLineIndentPt + firstLineDeltaPt)
    )
    doc.paragraphs[pIdx].leftIndentPt = max(
      0,
      min(144, doc.paragraphs[pIdx].leftIndentPt + leftIndentDeltaPt)
    )
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary =
      "문단 들여쓰기 조정 (첫 줄 \(Int(doc.paragraphs[pIdx].firstLineIndentPt))pt, 좌측 \(Int(doc.paragraphs[pIdx].leftIndentPt))pt)"
  }

  public func insertRowInDocumentTable(tableIndex: Int = 0, at rowIndex: Int? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      doc.tables.indices.contains(tableIndex)
    else { return }
    recordUndoSnapshot(for: activeTabID)
    doc.tables[tableIndex].insertRow(at: rowIndex)
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "표 행 삽입 완료 (총 \(doc.tables[tableIndex].rowCount)행)"
  }

  public func deleteRowInDocumentTable(tableIndex: Int = 0, at rowIndex: Int? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      doc.tables.indices.contains(tableIndex)
    else { return }
    recordUndoSnapshot(for: activeTabID)
    doc.tables[tableIndex].deleteRow(at: rowIndex)
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "표 행 삭제 완료 (총 \(doc.tables[tableIndex].rowCount)행)"
  }

  public func insertColumnInDocumentTable(tableIndex: Int = 0, at columnIndex: Int? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      doc.tables.indices.contains(tableIndex)
    else { return }
    recordUndoSnapshot(for: activeTabID)
    doc.tables[tableIndex].insertColumn(at: columnIndex)
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "표 열 삽입 완료 (총 \(doc.tables[tableIndex].columnCount)열)"
  }

  public func deleteColumnInDocumentTable(tableIndex: Int = 0, at columnIndex: Int? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload,
      doc.tables.indices.contains(tableIndex)
    else { return }
    recordUndoSnapshot(for: activeTabID)
    doc.tables[tableIndex].deleteColumn(at: columnIndex)
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "표 열 삭제 완료 (총 \(doc.tables[tableIndex].columnCount)열)"
  }

  public func acceptDocumentRevision(id: UUID) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    doc.acceptRevision(id: id)
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "변경 추적 항목 승인 완료"
  }

  public func rejectDocumentRevision(id: UUID) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    doc.rejectRevision(id: id)
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "변경 추적 항목 반려 완료"
  }

  public func insertEndnoteIntoActiveDocument(content: String? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let nextNumber = doc.footnotes.count + 1
    let trimmed = (content ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    let text = trimmed.isEmpty ? "미주 \(nextNumber): 표준 문헌 및 출처 참조" : trimmed
    let anchorID = selectedParagraphID ?? doc.paragraphs.first?.id ?? UUID()
    doc.footnotes.append(
      FootnoteEntry(
        number: nextNumber,
        referenceParagraphIndex: 0,
        content: text,
        isEndnote: true,
        anchorParagraphID: anchorID
      )
    )
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "미주 삽입 완료 (#\(nextNumber))"
  }

  // MARK: - Spreadsheet Structural, Named Range & Goal Seek Actions

  public func insertSheetRowAtSelection() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let addr =
      (try? CellAddress(a1Notation: selectedSheetCell))
      ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    sheet.insertRow(at: addr.row)
    formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "시트 행 삽입 완료 (\(addr.row + 1)행)"
  }

  public func deleteSheetRowAtSelection() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let addr =
      (try? CellAddress(a1Notation: selectedSheetCell))
      ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    sheet.deleteRow(at: addr.row)
    formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "시트 행 삭제 완료 (\(addr.row + 1)행)"
  }

  public func insertSheetColumnAtSelection() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let addr =
      (try? CellAddress(a1Notation: selectedSheetCell))
      ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    sheet.insertColumn(at: addr.column)
    formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "시트 열 삽입 완료 (\(CellAddress.columnLabel(for: addr.column))열)"
  }

  public func deleteSheetColumnAtSelection() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .sheet(var sheet) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let addr =
      (try? CellAddress(a1Notation: selectedSheetCell))
      ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    sheet.deleteColumn(at: addr.column)
    formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
    openTabs[idx].payload = .sheet(sheet)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "시트 열 삭제 완료 (\(CellAddress.columnLabel(for: addr.column))열)"
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

  // MARK: - Slide Deck & Sorter View Actions

  public func addSlideToActiveDeck(title: String? = nil) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let nextNumber = deck.slides.count + 1
    let trimmed = (title ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    let resolvedTitle = trimmed.isEmpty ? "슬라이드 \(nextNumber)" : trimmed
    let newSlide = SlideModel(
      slideNumber: nextNumber,
      title: resolvedTitle,
      bulletPoints: ["핵심 안건을 입력하십시오."]
    )
    deck.slides.append(newSlide)
    selectedSlideIndex = deck.slides.count - 1
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "새 슬라이드 추가 완료 (#\(nextNumber))"
  }

  public func duplicateActiveSlide() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload,
      !deck.slides.isEmpty
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
    let source = deck.slides[sIdx]
    let clonedElements = source.elements.map { elem in
      SlideCanvasElement(
        id: UUID(),
        title: elem.title,
        bodyText: elem.bodyText,
        x: elem.x,
        y: elem.y,
        width: elem.width,
        height: elem.height,
        kind: elem.kind,
        zIndex: elem.zIndex,
        rotationDegrees: elem.rotationDegrees,
        animation: elem.animation,
        fillHex: elem.fillHex,
        strokeHex: elem.strokeHex,
        isBold: elem.isBold,
        isItalic: elem.isItalic,
        isUnderlined: elem.isUnderlined
      )
    }
    let duplicated = SlideModel(
      id: UUID(),
      slideNumber: sIdx + 2,
      title: source.title,
      subtitle: source.subtitle,
      bulletPoints: source.bulletPoints,
      speakerNotes: source.speakerNotes,
      masterLayout: source.masterLayout,
      transition: source.transition,
      elements: clonedElements
    )
    deck.slides.insert(duplicated, at: sIdx + 1)
    selectedSlideIndex = sIdx + 1
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 복제 완료 (#\(sIdx + 2))"
  }

  public func deleteActiveSlide() {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload,
      deck.slides.count > 1
    else { return }
    recordUndoSnapshot(for: activeTabID)
    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
    deck.slides.remove(at: sIdx)
    selectedSlideIndex = max(0, min(deck.slides.count - 1, sIdx))
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 삭제 완료 (총 \(deck.slides.count)장)"
  }

  public func moveActiveSlide(offset: Int) {
    guard let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
      case .slide(var deck) = openTabs[idx].payload,
      !deck.slides.isEmpty
    else { return }
    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
    let clampedTarget = max(0, min(deck.slides.count - 1, sIdx + offset))
    guard clampedTarget != sIdx else { return }
    recordUndoSnapshot(for: activeTabID)
    deck.moveSlide(from: sIdx, to: clampedTarget)
    selectedSlideIndex = clampedTarget
    openTabs[idx].payload = .slide(deck)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "슬라이드 순서 이동 완료 (#\(sIdx + 1) → #\(clampedTarget + 1))"
  }

  public func toggleSlideSorterView() {
    isSlideSorterViewEnabled.toggle()
    lastExecutedCommandSummary =
      isSlideSorterViewEnabled ? "슬라이드 정렬 격자 뷰 활성화" : "슬라이드 정렬 격자 뷰 해제"
  }

  // MARK: - PDF Studio Redaction, Form Fields, Page Reordering & Merging

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
}
