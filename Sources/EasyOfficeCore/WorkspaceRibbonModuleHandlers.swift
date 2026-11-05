import EasyOfficeDesignSystem
import Foundation

extension WorkspaceViewModel {

  internal enum InsertActionKind { case table, chart, shape, link }

  internal func applyInsertCommand(at idx: Int, kind: InsertActionKind) {
    switch openTabs[idx].payload {
    case .document(var doc):
      switch kind {
      case .table:
        doc.tables.append(
          DocumentTableBlock(
            caption: "표 \(doc.tables.count + 1). 추가 데이터 표",
            rows: [["구분", "목표치", "실적치"], ["1분기", "100", "120"], ["2분기", "150", "180"]]))
      case .chart:
        doc.tables.append(
          DocumentTableBlock(
            caption: "차트 데이터 테이블",
            rows: [
              ["지표 구분", "목표치", "실적치"],
              ["1분기 핵심 지표", "100", "124"],
              ["2분기 핵심 지표", "150", "182"],
            ]
          )
        )
        let chartPNG = Self.makeSampleDiagramPNG(
          width: 260,
          height: 110,
          accentR: 0x0A,
          accentG: 0x70,
          accentB: 0x5A
        )
        doc.images.append(
          EmbeddedImageAttachment(
            fileName: "chart_diagram_\(doc.images.count + 1).png",
            caption: "실적 요약 차트 그래픽",
            mimeType: "image/png",
            imageData: chartPNG,
            widthPt: 260.0,
            heightPt: 110.0,
            anchorParagraphID: selectedParagraphID ?? doc.paragraphs.last?.id,
            wrapMode: .inline
          )
        )
      case .shape:
        var shapeBlock = ParagraphBlock(
          style: .quote,
          alignment: .center,
          text: "프로세스 블록",
          isBold: true
        )
        shapeBlock.runs = [
          DocumentTextRun(text: "프로세스 블록", isBold: true, foregroundHex: "#5C6BC0")
        ]
        doc.paragraphs.append(shapeBlock)
        let shapePNG = Self.makeSampleDiagramPNG(
          width: 240,
          height: 96,
          accentR: 0x5C,
          accentG: 0x6B,
          accentB: 0xC0
        )
        doc.images.append(
          EmbeddedImageAttachment(
            fileName: "shape_diagram_\(doc.images.count + 1).png",
            caption: "벡터 다이어그램 도형",
            mimeType: "image/png",
            imageData: shapePNG,
            widthPt: 240.0,
            heightPt: 96.0,
            anchorParagraphID: shapeBlock.id,
            wrapMode: .inline
          )
        )
      case .link:
        let pIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
        let anchorID = doc.paragraphs.indices.contains(pIdx) ? doc.paragraphs[pIdx].id : UUID()
        doc.bookmarks["책갈피_\(doc.bookmarks.count + 1)"] = anchorID
        if doc.paragraphs.indices.contains(pIdx) {
          let trimmedURL = inlineLinkURLDraft.trimmingCharacters(in: .whitespacesAndNewlines)
          let rawCandidate =
            trimmedURL.isEmpty ? "https://easyoffice.local/reference" : trimmedURL
          let resolvedURL =
            Self.sanitizeSafeHyperlinkURL(rawCandidate) ?? "https://easyoffice.local/reference"
          if let utf16Range = selectedParagraphUTF16Range, !utf16Range.isEmpty {
            doc.paragraphs[pIdx].applyRunFormatting(
              utf16Range: utf16Range,
              isUnderlined: true,
              foregroundHex: "#1E88E5",
              linkURL: resolvedURL
            )
          } else {
            let baseText = doc.paragraphs[pIdx].text
            let firstWordRange: Range<String.Index> = {
              if let spaceIdx = baseText.firstIndex(of: " "), spaceIdx < baseText.endIndex {
                return baseText.startIndex..<baseText.index(after: spaceIdx)
              }
              return baseText.startIndex..<baseText.endIndex
            }()
            doc.paragraphs[pIdx].applyRunFormatting(
              range: firstWordRange,
              isUnderlined: true,
              foregroundHex: "#1E88E5",
              linkURL: resolvedURL
            )
          }
        }
      }
      openTabs[idx].payload = .document(doc)
      openTabs[idx].isModified = true

    case .sheet(var sheet):
      let coord =
        (try? CellAddress(a1Notation: selectedSheetCell))?.packedCoordinate
        ?? PackedCellCoordinate(row: 0, column: 0)
      switch kind {
      case .table:
        sheet.addSheetPage(
          named: "Sheet\(sheet.allSheetNames.count + 1)",
          initialCells: [
            PackedCellCoordinate(row: 0, column: 0): "항목",
            PackedCellCoordinate(row: 0, column: 1): "값",
          ])
      case .chart:
        isChartPanelVisible = true
        let allKinds = SheetChartKind.allCases
        if let cIdx = allKinds.firstIndex(of: selectedChartKind) {
          selectedChartKind = allKinds[(cIdx + 1) % allKinds.count]
        }
      case .shape:
        isChartPanelVisible = true
        var style = sheet.cellStyle(at: coord)
        let alignments = CellHorizontalAlignment.allCases
        let curIdx = alignments.firstIndex(of: style.alignment) ?? 0
        style.alignment = alignments[(curIdx + 1) % alignments.count]
        style.textHex = "#5C6BC0"
        sheet.setCellStyle(style, at: coord)
        if let range = try? CellRange(notation: selectedSheetRange), range.start != range.end {
          sheet.updateCellStyleInRange(range) { cellStyle in
            cellStyle.alignment = style.alignment
            cellStyle.textHex = "#5C6BC0"
          }
        }
      case .link:
        sheet.sheetComments[coord] = "참조 링크 (\(selectedSheetCell))"
      }
      openTabs[idx].payload = .sheet(sheet)
      openTabs[idx].isModified = true

    case .slide(var deck):
      guard !deck.slides.isEmpty else { return }
      let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
      let elementKind: SlideElementKind
      let title: String
      switch kind {
      case .table:
        elementKind = .tableGrid
        title = "데이터 표 그리드"
      case .chart:
        elementKind = .chartEmbed
        title = "성과 지표 차트"
      case .shape:
        elementKind = .roundedRectangleShape
        title = "아키텍처 도형"
      case .link:
        elementKind = .calloutShape
        title = "참조 링크 콜아웃"
      }
      let offsetCount = Double(deck.slides[sIdx].elements.count)
      let newElem = SlideCanvasElement(
        title: title, bodyText: "슬라이드 \(sIdx + 1) 추가 개체", x: 340.0 + offsetCount * 16.0,
        y: 240.0 + offsetCount * 12.0, width: 220, height: 150, kind: elementKind)
      deck.slides[sIdx].elements.append(newElem)
      selectedSlideElementID = newElem.id
      openTabs[idx].payload = .slide(deck)
      openTabs[idx].isModified = true

    case .pdf(var pdfDoc):
      let pageNum =
        pdfDoc.pages.indices.contains(selectedPDFPageIndex)
        ? pdfDoc.pages[selectedPDFPageIndex].pageNumber : 1
      if kind == .link {
        pdfDoc.outline.append(
          PDFOutlineNode(title: "책갈피 \(pdfDoc.outline.count + 1)", targetPageNumber: pageNum))
      } else {
        pdfDoc.annotations.append(
          PDFAnnotationItem(
            pageNumber: pageNum, kind: .stickyNote, content: "삽입 개체 마크", author: authorName))
      }
      openTabs[idx].payload = .pdf(pdfDoc)
      openTabs[idx].isModified = true
    }
  }

  internal enum LayoutActionKind { case paperSize, orientation, columns, margins }

  internal func applyLayoutCommand(at idx: Int, kind: LayoutActionKind) {
    switch openTabs[idx].payload {
    case .document(var doc):
      if doc.sections.isEmpty { doc.sections = [DocumentSectionLayout(paperSize: doc.paperSize)] }
      switch kind {
      case .paperSize:
        let presets = PaperSizePreset.allCases
        let cur = doc.paperSize
        let next = presets[((presets.firstIndex(of: cur) ?? 0) + 1) % presets.count]
        doc.paperSize = next
        doc.sections[0].paperSize = next
      case .orientation:
        doc.sections[0].orientation =
          (doc.sections[0].orientation == .portrait) ? .landscape : .portrait
      case .columns:
        doc.sections[0].columnCount = (doc.sections[0].columnCount % 3) + 1
      case .margins:
        let targetMargin = (doc.sections[0].marginTopPt < 45.0) ? 56.7 : 36.0
        doc.sections[0].marginTopPt = targetMargin
        doc.sections[0].marginBottomPt = targetMargin
        doc.sections[0].marginLeftPt = targetMargin
        doc.sections[0].marginRightPt = targetMargin
      }
      openTabs[idx].payload = .document(doc)
      openTabs[idx].isModified = true

    case .slide(var deck):
      let ratios = SlideAspectRatio.allCases
      let cur = deck.aspectRatio
      deck.aspectRatio = ratios[((ratios.firstIndex(of: cur) ?? 0) + 1) % ratios.count]
      openTabs[idx].payload = .slide(deck)
      openTabs[idx].isModified = true

    case .pdf(var pdfDoc):
      switch kind {
      case .orientation:
        let targetIndex =
          pdfDoc.pages.indices.contains(selectedPDFPageIndex) ? selectedPDFPageIndex : 0
        pdfDoc.rotatePage(at: targetIndex, clockwiseDegrees: 90)
        openTabs[idx].payload = .pdf(pdfDoc)
        openTabs[idx].isModified = true
      case .paperSize:
        let presets = PaperSizePreset.allCases
        let cur = pdfPaperSizePreset
        pdfPaperSizePreset = presets[((presets.firstIndex(of: cur) ?? 0) + 1) % presets.count]
        reviewStatsRefreshCount += 1
      case .margins:
        setDocumentHorizontalPadding(documentHorizontalPadding < 40.0 ? 56.0 : 28.0)
      case .columns:
        pdfPageColumnCount = (pdfPageColumnCount % 2) + 1
        reviewStatsRefreshCount += 1
      }

    case .sheet(var sheet):
      switch kind {
      case .paperSize:
        sheet.pageSetup.paperSize = (sheet.pageSetup.paperSize == .a4) ? .letter : .a4
      case .orientation:
        sheet.pageSetup.orientation =
          (sheet.pageSetup.orientation == .portrait) ? .landscape : .portrait
      case .columns: sheet.pageSetup.isMultiColumnPrint.toggle()
      case .margins:
        sheet.pageSetup.marginPresetName =
          (sheet.pageSetup.marginPresetName == "기본 여백") ? "좁은 여백" : "기본 여백"
      }
      openTabs[idx].payload = .sheet(sheet)
      openTabs[idx].isModified = true
    }
  }

  internal enum DataActionKind { case autoSum, functionWizard, sort, validation }

  internal func applyDataCommand(at idx: Int, kind: DataActionKind) {
    let rawFn = selectedFunctionWizardName.trimmingCharacters(in: .whitespacesAndNewlines)
      .uppercased()
    let wizardFnName = rawFn.isEmpty ? "AVERAGE" : rawFn

    switch openTabs[idx].payload {
    case .sheet(var sheet):
      switch kind {
      case .autoSum:
        let numRange = sheet.detectedNumericRange().a1Range
        formulaBarDraft = "=SUM(\(numRange))"
        try? sheet.setCell(selectedSheetCell, rawValue: formulaBarDraft)
      case .functionWizard:
        let numRange = sheet.detectedNumericRange().a1Range
        formulaBarDraft = "=\(wizardFnName)(\(numRange))"
        try? sheet.setCell(selectedSheetCell, rawValue: formulaBarDraft)
      case .sort:
        sheetSortAscending.toggle()
        let bounds = sheet.detectedDataRange()
        let cols = Array(bounds.start.column...bounds.end.column)
        let primaryMetricCol = min(bounds.end.column, bounds.start.column + 1)
        let dataCandidateRows = (bounds.start.row...bounds.end.row).filter { r in
          let labelRaw =
            (sheet.cellsByCoordinate[PackedCellCoordinate(row: r, column: bounds.start.column)]
            ?? "").trimmingCharacters(in: .whitespaces)
          if labelRaw == "합계" || labelRaw == "총계" || labelRaw.uppercased() == "TOTAL" {
            return false
          }
          let metricRaw =
            (sheet.cellsByCoordinate[PackedCellCoordinate(row: r, column: primaryMetricCol)] ?? "")
            .trimmingCharacters(in: .whitespaces).uppercased()
          if metricRaw.hasPrefix("=SUM(") || metricRaw.hasPrefix("=AVERAGE(") { return false }
          return true
        }
        let sortableRows: [Int]
        if dataCandidateRows.count > 1 {
          let firstRow = dataCandidateRows[0]
          let firstRowHasNumeric = cols.contains(where: { c in
            guard let raw = sheet.cellsByCoordinate[PackedCellCoordinate(row: firstRow, column: c)]
            else { return false }
            return Double(raw.trimmingCharacters(in: .whitespaces)) != nil
          })
          let laterRowHasNumeric = dataCandidateRows.dropFirst().contains(where: { r in
            cols.contains(where: { c in
              guard let raw = sheet.cellsByCoordinate[PackedCellCoordinate(row: r, column: c)]
              else { return false }
              return Double(raw.trimmingCharacters(in: .whitespaces)) != nil
            })
          })
          sortableRows =
            (!firstRowHasNumeric && laterRowHasNumeric)
            ? Array(dataCandidateRows.dropFirst()) : dataCandidateRows
        } else {
          sortableRows = dataCandidateRows
        }
        if sortableRows.count >= 2 {
          let keyCol: Int = {
            if let selAddr = try? CellAddress(a1Notation: selectedSheetCell),
              cols.contains(selAddr.column),
              sortableRows.contains(where: {
                guard
                  let raw = sheet.cellsByCoordinate[
                    PackedCellCoordinate(row: $0, column: selAddr.column)]
                else { return false }
                return !raw.trimmingCharacters(in: .whitespaces).hasPrefix("=")
              })
            {
              return selAddr.column
            }
            return cols.first(where: { c in
              sortableRows.contains(where: { r in
                guard let raw = sheet.cellsByCoordinate[PackedCellCoordinate(row: r, column: c)]
                else { return false }
                return Double(raw.trimmingCharacters(in: .whitespaces)) != nil
              })
            }) ?? bounds.start.column
          }()
          struct SortableRowSnapshot {
            let originalRow: Int
            let rawCells: [Int: String]
            let styles: [Int: CellStyle]
            let keyVal: EvaluatedCellValue
          }
          var snapshots: [SortableRowSnapshot] = []
          for r in sortableRows {
            var rawMap: [Int: String] = [:]
            var styleMap: [Int: CellStyle] = [:]
            for c in cols {
              let coord = PackedCellCoordinate(row: r, column: c)
              if let raw = sheet.cellsByCoordinate[coord],
                !raw.trimmingCharacters(in: .whitespaces).hasPrefix("=")
              {
                rawMap[c] = raw
              }
              if let st = sheet.cellStyles[coord] { styleMap[c] = st }
            }
            let keyCoord = PackedCellCoordinate(row: r, column: keyCol)
            let keyVal = sheet.evaluate(at: keyCoord)
            snapshots.append(
              SortableRowSnapshot(
                originalRow: r, rawCells: rawMap, styles: styleMap, keyVal: keyVal))
          }
          snapshots.sort { lhs, rhs in
            let cmp: Bool
            switch (lhs.keyVal, rhs.keyVal) {
            case (.number(let a), .number(let b)): cmp = a < b
            default: cmp = lhs.keyVal.description < rhs.keyVal.description
            }
            return sheetSortAscending ? cmp : !cmp
          }
          if snapshots.map(\.originalRow) == sortableRows {
            sheetSortAscending.toggle()
            snapshots.reverse()
          }
          for (idxOffset, targetRow) in sortableRows.enumerated() {
            let snap = snapshots[idxOffset]
            for c in cols {
              let coord = PackedCellCoordinate(row: targetRow, column: c)
              if let existing = sheet.cellsByCoordinate[coord],
                existing.trimmingCharacters(in: .whitespaces).hasPrefix("=")
              {
                continue
              }
              let addr = CellAddress(packed: coord).a1Notation
              try? sheet.setCell(addr, rawValue: snap.rawCells[c] ?? "")
              sheet.setCellStyle(snap.styles[c] ?? CellStyle(), at: coord)
            }
          }
        }
      case .validation:
        activeDataValidationRule =
          (activeDataValidationRule == nil)
          ? DataValidationRule(minimumValue: 0, maximumValue: 1_000_000_000, allowedSet: nil)
          : nil
        sheet.dataValidationRule = activeDataValidationRule
      }
      openTabs[idx].payload = .sheet(sheet)
      openTabs[idx].isModified = true

    case .document(var doc):
      switch kind {
      case .autoSum:
        if doc.tables.isEmpty {
          doc.tables.append(
            DocumentTableBlock(
              caption: "표 1. 데이터 검산 표", rows: [["항목", "수치"], ["기초값", "100"], ["증분값", "150"]]))
        }
        let tIdx = min(max(0, selectedDocumentTableIndex), doc.tables.count - 1)
        let colCount = max(1, doc.tables[tIdx].columnCount)
        let rawCol = selectedDocumentTableCell?.column ?? (colCount - 1)
        let targetCol =
          colCount > 1 && rawCol == 0 ? (colCount - 1) : min(max(0, rawCol), colCount - 1)
        let sum =
          doc.tables[tIdx].evaluateTableFormula(
            "=SUM(ABOVE)", atRow: doc.tables[tIdx].rowCount, column: targetCol) ?? 0.0
        var summaryRow = Array(repeating: "", count: colCount)
        summaryRow[0] = "합계 (=SUM(ABOVE))"
        summaryRow[targetCol] = sum.isFinite ? String(format: "%.0f", sum) : "0"
        doc.tables[tIdx].rows.append(summaryRow)
      case .functionWizard:
        if doc.tables.isEmpty {
          doc.tables.append(
            DocumentTableBlock(
              caption: "표 1. 데이터 검산 표", rows: [["항목", "수치"], ["기초값", "100"], ["증분값", "150"]]))
        }
        let tIdx = min(max(0, selectedDocumentTableIndex), doc.tables.count - 1)
        let colCount = max(1, doc.tables[tIdx].columnCount)
        let rawCol = selectedDocumentTableCell?.column ?? (colCount - 1)
        let targetCol =
          colCount > 1 && rawCol == 0 ? (colCount - 1) : min(max(0, rawCol), colCount - 1)
        let formulaExpr = "=\(wizardFnName)(ABOVE)"
        let evaluated =
          doc.tables[tIdx].evaluateTableFormula(
            formulaExpr, atRow: doc.tables[tIdx].rowCount, column: targetCol)
          ?? doc.tables[tIdx].evaluateTableFormula(
            "=AVERAGE(ABOVE)", atRow: doc.tables[tIdx].rowCount, column: targetCol) ?? 0.0
        var fnRow = Array(repeating: "", count: colCount)
        fnRow[0] = wizardFnName == "AVERAGE" ? "평균 (=AVERAGE(ABOVE))" : "함수 (\(formulaExpr))"
        fnRow[targetCol] = String(format: "%.1f", evaluated)
        doc.tables[tIdx].rows.append(fnRow)
      case .sort:
        sheetSortAscending.toggle()
        let tIdx = doc.tables.isEmpty ? 0 : min(max(0, selectedDocumentTableIndex), doc.tables.count - 1)
        if !doc.tables.isEmpty, doc.tables[tIdx].rows.count > 2 {
          let sortCol = min(
            max(0, selectedDocumentTableCell?.column ?? 0),
            max(0, doc.tables[tIdx].columnCount - 1))
          let header = doc.tables[tIdx].rows[0]
          let body = Array(doc.tables[tIdx].rows.dropFirst())
          let sortedBody = body.sorted { lhs, rhs in
            let lVal = lhs.indices.contains(sortCol) ? lhs[sortCol] : (lhs.first ?? "")
            let rVal = rhs.indices.contains(sortCol) ? rhs[sortCol] : (rhs.first ?? "")
            return sheetSortAscending ? (lVal < rVal) : (lVal > rVal)
          }
          doc.tables[tIdx].rows = [header] + sortedBody
        }
        doc.recordRevision(
          author: authorName, summary: "표 데이터 정렬 (\(sheetSortAscending ? "오름차순" : "내림차순"))")
      case .validation:
        activeDataValidationRule = (activeDataValidationRule == nil) ? .nonEmptyText : nil
        doc.recordRevision(author: authorName, summary: "표 데이터 유효성 검사 규칙 전환")
      }
      openTabs[idx].payload = .document(doc)
      openTabs[idx].isModified = true

    case .slide(var deck):
      guard !deck.slides.isEmpty else { return }
      let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
      let offset = Double(deck.slides[sIdx].elements.count) * 14.0
      switch kind {
      case .autoSum:
        let elem = SlideCanvasElement(
          title: "핵심 지표 합계 (SUM)", bodyText: "누적 실적 합계 요약 블록", x: 340.0 + offset, y: 240.0 + offset,
          width: 220, height: 150, kind: .chartEmbed)
        deck.slides[sIdx].elements.append(elem)
        selectedSlideElementID = elem.id
      case .functionWizard:
        let elem = SlideCanvasElement(
          title: "함수 분석 (\(wizardFnName))", bodyText: "지표 분석 카드", x: 340.0 + offset,
          y: 240.0 + offset,
          width: 220, height: 150, kind: .chartEmbed)
        deck.slides[sIdx].elements.append(elem)
        selectedSlideElementID = elem.id
      case .sort:
        sheetSortAscending.toggle()
        if deck.slides[sIdx].elements.count > 1 {
          let beforeIDs = deck.slides[sIdx].elements.map(\.id)
          deck.slides[sIdx].elements.sort { lhs, rhs in
            sheetSortAscending ? (lhs.title < rhs.title) : (lhs.title > rhs.title)
          }
          if deck.slides[sIdx].elements.map(\.id) == beforeIDs {
            deck.slides[sIdx].elements.reverse()
          }
        } else {
          let elem = SlideCanvasElement(
            title: "정렬 요약 표", bodyText: sheetSortAscending ? "오름차순 정렬 기준" : "내림차순 정렬 기준",
            x: 340.0 + offset, y: 240.0 + offset, width: 220, height: 150, kind: .tableGrid)
          deck.slides[sIdx].elements.append(elem)
          selectedSlideElementID = elem.id
        }
      case .validation:
        activeDataValidationRule = (activeDataValidationRule == nil) ? .nonEmptyText : nil
        let elem = SlideCanvasElement(
          title: "데이터 검증 요약", bodyText: "필수 지표 무결성 확인 완료", x: 340.0 + offset, y: 240.0 + offset,
          width: 220, height: 150, kind: .tableGrid)
        deck.slides[sIdx].elements.append(elem)
        selectedSlideElementID = elem.id
      }
      openTabs[idx].payload = .slide(deck)
      openTabs[idx].isModified = true

    case .pdf(var pdfDoc):
      activeDataValidationRule = (activeDataValidationRule == nil) ? .nonEmptyText : nil
      pdfDoc.annotations.append(
        PDFAnnotationItem(
          pageNumber: selectedPDFPageNumber, kind: .stickyNote, content: "데이터 검산(\(kind)) 완료",
          author: authorName))
      openTabs[idx].payload = .pdf(pdfDoc)
      openTabs[idx].isModified = true
    }
  }


  @discardableResult
  public func performFindAndReplace() -> Int {
    let query =
      findQueryText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      ? findQueryDraft
      : findQueryText
    let replacement =
      replaceQueryText.isEmpty && !replaceQueryDraft.isEmpty ? replaceQueryDraft : replaceQueryText
    guard !query.isEmpty, let activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == activeTabID })
    else {
      lastFindReplaceCount = 0
      return 0
    }

    let options: String.CompareOptions = isFindCaseSensitive ? [] : [.caseInsensitive]
    var count = 0
    func replaceOccurrences(in source: inout String) {
      var searchStart = source.startIndex
      var matches: [Range<String.Index>] = []
      while searchStart < source.endIndex,
        let r = source.range(of: query, options: options, range: searchStart..<source.endIndex)
      {
        matches.append(r)
        searchStart = r.upperBound
      }
      guard !matches.isEmpty else { return }
      count += matches.count
      for matchRange in matches.reversed() {
        source.replaceSubrange(matchRange, with: replacement)
      }
    }

    switch openTabs[idx].payload {
    case .document(var doc):
      let beforeDoc = doc
      for pIdx in doc.paragraphs.indices {
        replaceOccurrences(in: &doc.paragraphs[pIdx].text)
        for rIdx in doc.paragraphs[pIdx].runs.indices {
          doc.paragraphs[pIdx].runs[rIdx].text = doc.paragraphs[pIdx].runs[rIdx].text
            .replacingOccurrences(of: query, with: replacement, options: options)
        }
      }
      for tIdx in doc.tables.indices {
        for rIdx in doc.tables[tIdx].rows.indices {
          for cIdx in doc.tables[tIdx].rows[rIdx].indices {
            replaceOccurrences(in: &doc.tables[tIdx].rows[rIdx][cIdx])
          }
        }
      }
      if doc != beforeDoc {
        recordUndoSnapshot(for: activeTabID)
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
      }

    case .sheet(var sheet):
      let beforeSheet = sheet
      for coord in Array(sheet.cellsByCoordinate.keys).sorted() {
        guard var raw = sheet.cellsByCoordinate[coord], !raw.hasPrefix("=") else { continue }
        let prevCount = count
        replaceOccurrences(in: &raw)
        if count > prevCount {
          try? sheet.setCell(CellAddress(packed: coord).a1Notation, rawValue: raw)
        }
      }
      if sheet != beforeSheet {
        recordUndoSnapshot(for: activeTabID)
        openTabs[idx].payload = .sheet(sheet)
        openTabs[idx].isModified = true
      }

    case .slide(var deck):
      let beforeDeck = deck
      for sIdx in deck.slides.indices {
        replaceOccurrences(in: &deck.slides[sIdx].title)
        replaceOccurrences(in: &deck.slides[sIdx].subtitle)
        for bIdx in deck.slides[sIdx].bulletPoints.indices {
          replaceOccurrences(in: &deck.slides[sIdx].bulletPoints[bIdx])
        }
        for eIdx in deck.slides[sIdx].elements.indices {
          replaceOccurrences(in: &deck.slides[sIdx].elements[eIdx].title)
          replaceOccurrences(in: &deck.slides[sIdx].elements[eIdx].bodyText)
        }
        replaceOccurrences(in: &deck.slides[sIdx].speakerNotes)
      }
      if deck != beforeDeck {
        recordUndoSnapshot(for: activeTabID)
        openTabs[idx].payload = .slide(deck)
        openTabs[idx].isModified = true
      }

    case .pdf(var pdfDoc):
      let beforePDF = pdfDoc
      for pIdx in pdfDoc.pages.indices { replaceOccurrences(in: &pdfDoc.pages[pIdx].extractedText) }
      for oIdx in pdfDoc.outline.indices { replaceOccurrences(in: &pdfDoc.outline[oIdx].title) }
      for aIdx in pdfDoc.annotations.indices {
        replaceOccurrences(in: &pdfDoc.annotations[aIdx].content)
      }
      if pdfDoc != beforePDF {
        recordUndoSnapshot(for: activeTabID)
        openTabs[idx].payload = .pdf(pdfDoc)
        openTabs[idx].isModified = true
      }
    }

    lastFindReplaceCount = count
    lastExecutedCommandSummary = "찾기 및 바꾸기 완료 (\(count)건 치환)"
    return count
  }

  public func runBatchConversionForOpenTabs() {
    runAllBatchConversions()
  }

  public func runSecurityAuditOnActiveTab() {
    guard let current = activeTab else { return }
    if let bytes = try? exportTabData(id: current.id, targetFormat: current.format) {
      lastSecurityAuditReport = DocumentSecurityAuditor.auditContainer(
        data: bytes, fileName: current.fileName)
    }
    reviewStatsRefreshCount += 1
  }

  public func scrubActiveTabMetadata() {
    guard let activeTabID,
      let targetIdx = openTabs.firstIndex(where: { $0.id == activeTabID })
    else { return }
    recordUndoSnapshot(for: openTabs[targetIdx].id)
    let (scrubbed, removedCount) = DocumentSecurityAuditor.scrubPayload(
      openTabs[targetIdx].payload)
    openTabs[targetIdx].payload = scrubbed
    openTabs[targetIdx].isModified = true
    lastScrubbedMetadataCount = removedCount
    lastExecutedCommandSummary = "개인정보·메타데이터 \(removedCount)건 비식별화 완료"
  }
}
