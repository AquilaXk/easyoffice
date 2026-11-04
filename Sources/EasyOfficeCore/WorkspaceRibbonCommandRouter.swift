import EasyOfficeDesignSystem
import Foundation

extension WorkspaceViewModel {

  // MARK: - Complete 140-Action Context Ribbon Dispatcher

  public func executeRibbonCommand(_ commandID: String) {
    activeInlineEditTargetKey = nil

    if commandID.hasSuffix("-view-split") {
      if splitSecondaryTab != nil {
        enableSplitView(secondaryTabID: nil)
        lastExecutedCommandSummary = "좌우 스플릿 뷰 해제"
      } else if let candidate = openTabs.first(where: { $0.id != activeTabID }) {
        enableSplitView(secondaryTabID: candidate.id)
        lastExecutedCommandSummary = "좌우 스플릿 뷰 활성: \(candidate.fileName)"
      } else {
        lastExecutedCommandSummary = "좌우 스플릿 뷰 단일 탭 유지"
      }
      return
    }
    if commandID.hasSuffix("-view-ribbon") {
      isRibbonCollapsed.toggle()
      lastExecutedCommandSummary = isRibbonCollapsed ? "컨텍스트 리본 접기 완료" : "컨텍스트 리본 펼치기 완료"
      return
    }
    if commandID.hasSuffix("-view-theme") {
      themeMode = (themeMode == .darkIndigo) ? .lightLavender : .darkIndigo
      lastExecutedCommandSummary = "테마 모드 전환: \(themeMode.rawValue)"
      return
    }
    if commandID.hasSuffix("-view-zoom") {
      setZoom(zoomPercent == 100.0 ? 125.0 : 100.0)
      lastExecutedCommandSummary = "배율 \(Int(zoomPercent))% 전환 완료"
      return
    }

    guard let tabID = activeTab?.id, let idx = openTabs.firstIndex(where: { $0.id == tabID }) else {
      lastExecutedCommandSummary = "활성 탭 없음"
      return
    }

    if commandID.hasSuffix("-home-bold") {
      recordUndoSnapshot(for: tabID)
      applyHomeFormatting(at: idx, action: .bold)
      lastExecutedCommandSummary = "굵게 서식 전환 완료"
      return
    }
    if commandID.hasSuffix("-home-italic") {
      recordUndoSnapshot(for: tabID)
      applyHomeFormatting(at: idx, action: .italic)
      lastExecutedCommandSummary = "기울임 서식 전환 완료"
      return
    }
    if commandID.hasSuffix("-home-underline") {
      recordUndoSnapshot(for: tabID)
      applyHomeFormatting(at: idx, action: .underline)
      lastExecutedCommandSummary = "밑줄 서식 전환 완료"
      return
    }
    if commandID.hasSuffix("-home-style") {
      recordUndoSnapshot(for: tabID)
      applyHomeFormatting(at: idx, action: .styleCycle)
      lastExecutedCommandSummary = "표준 단락·개체 스타일 순환 적용 완료"
      return
    }

    if commandID.hasSuffix("-insert-table") {
      recordUndoSnapshot(for: tabID)
      applyInsertCommand(at: idx, kind: .table)
      lastExecutedCommandSummary = "표 개체 삽입 완료"
      return
    }
    if commandID.hasSuffix("-insert-chart") {
      recordUndoSnapshot(for: tabID)
      applyInsertCommand(at: idx, kind: .chart)
      if case .sheet(let sheet) = openTabs[idx].payload {
        lastExecutedCommandSummary =
          "차트 개체 삽입 완료: \(selectedChartKind.rawValue) (\(sheet.detectedNumericRange().a1Range))"
      } else {
        lastExecutedCommandSummary = "차트 개체 삽입 완료"
      }
      return
    }
    if commandID.hasSuffix("-insert-shape") {
      recordUndoSnapshot(for: tabID)
      applyInsertCommand(at: idx, kind: .shape)
      lastExecutedCommandSummary = "도형·다이어그램 개체 삽입 완료"
      return
    }
    if commandID.hasSuffix("-insert-link") {
      recordUndoSnapshot(for: tabID)
      applyInsertCommand(at: idx, kind: .link)
      lastExecutedCommandSummary = "책갈피·참조 링크 삽입 완료"
      return
    }

    if commandID.hasSuffix("-layout-paper") || commandID.hasSuffix("-layout-size") {
      if openTabs[idx].moduleKind != .pdf {
        recordUndoSnapshot(for: tabID)
      }
      applyLayoutCommand(at: idx, kind: .paperSize)
      lastExecutedCommandSummary = "용지 및 화면 규격 전환 완료"
      return
    }
    if commandID.hasSuffix("-layout-orientation") || commandID.hasSuffix("-layout-orient") {
      recordUndoSnapshot(for: tabID)
      applyLayoutCommand(at: idx, kind: .orientation)
      lastExecutedCommandSummary = "용지 방향 전환 완료"
      return
    }
    if commandID.hasSuffix("-layout-columns") {
      if openTabs[idx].moduleKind != .pdf {
        recordUndoSnapshot(for: tabID)
      }
      applyLayoutCommand(at: idx, kind: .columns)
      lastExecutedCommandSummary = "다단 레이아웃 전환 완료"
      return
    }
    if commandID.hasSuffix("-layout-margins") || commandID.hasSuffix("-layout-margin") {
      recordUndoSnapshot(for: tabID)
      applyLayoutCommand(at: idx, kind: .margins)
      lastExecutedCommandSummary = "여백 프리셋 전환 완료"
      return
    }

    if commandID.hasSuffix("-data-autosum") || commandID.hasSuffix("-data-sum") {
      recordUndoSnapshot(for: tabID)
      applyDataCommand(at: idx, kind: .autoSum)
      lastExecutedCommandSummary = "자동 합계(SUM) 수식 계산 완료"
      return
    }
    if commandID.hasSuffix("-data-fn-browser") || commandID.hasSuffix("-data-fx") {
      recordUndoSnapshot(for: tabID)
      applyDataCommand(at: idx, kind: .functionWizard)
      let fnRaw = selectedFunctionWizardName.trimmingCharacters(in: .whitespacesAndNewlines)
        .uppercased()
      let fnName = fnRaw.isEmpty ? "AVERAGE" : fnRaw
      lastExecutedCommandSummary =
        "\(WorksheetModel.supportedFunctionNames.count)종 함수 마법사(\(fnName)) 수식 적용 완료"
      return
    }
    if commandID.hasSuffix("-data-sort") {
      recordUndoSnapshot(for: tabID)
      applyDataCommand(at: idx, kind: .sort)
      lastExecutedCommandSummary = "데이터 행 정렬 완료 (\(sheetSortAscending ? "오름차순" : "내림차순"))"
      return
    }
    if commandID.hasSuffix("-data-validation") || commandID.hasSuffix("-data-validate") {
      recordUndoSnapshot(for: tabID)
      applyDataCommand(at: idx, kind: .validation)
      lastExecutedCommandSummary = "데이터 유효성 검사 규칙 전환 완료"
      return
    }

    switch commandID {
    case "doc-mod-toc":
      recordUndoSnapshot(for: tabID)
      if case .document(var doc) = openTabs[idx].payload {
        let toc = doc.generateTableOfContents()
        let summaryText =
          toc.isEmpty
          ? "[자동 목차] 등록된 제목 스타일 없음" : "[자동 목차] " + toc.map(\.title).joined(separator: " · ")
        doc.paragraphs.insert(
          ParagraphBlock(style: .quote, alignment: .leading, text: summaryText, isBold: true), at: 0
        )
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
      }
      lastExecutedCommandSummary = "자동 목차(TOC) 생성 완료"
      return

    case "doc-mod-footnote", "doc-insert-footnote":
      recordUndoSnapshot(for: tabID)
      if case .document(var doc) = openTabs[idx].payload {
        let anchorID = selectedParagraphID ?? doc.paragraphs.last?.id ?? UUID()
        let nextNum = doc.footnotes.count + 1
        doc.footnotes.append(
          FootnoteItem(
            number: nextNum, anchorParagraphID: anchorID, text: "각주 \(nextNum): 표준 규격 참조 설명"))
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
      }
      lastExecutedCommandSummary = "각주·미주 삽입 완료"
      return

    case "doc-mod-mailmerge", "doc-mod-merge":
      recordUndoSnapshot(for: tabID)
      if case .document(var doc) = openTabs[idx].payload {
        doc = doc.applyingMailMerge(record: mailMergeFields)
        doc.recordRevision(author: authorName, summary: "메일 머지 필드(\(mailMergeFields.count)개) 일괄 치환")
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
      }
      lastExecutedCommandSummary = "메일 머지 필드 일괄 치환 완료"
      return

    case "doc-mod-tableformula", "doc-mod-header":
      recordUndoSnapshot(for: tabID)
      if case .document(var doc) = openTabs[idx].payload {
        if !doc.tables.isEmpty {
          let lastCol = max(0, doc.tables[0].columnCount - 1)
          if let val = doc.tables[0].evaluateTableFormula(
            "=SUM(ABOVE)", atRow: doc.tables[0].rowCount, column: lastCol)
          {
            var summaryRow = Array(repeating: "", count: max(1, doc.tables[0].columnCount))
            summaryRow[0] = "합계 (=SUM(ABOVE))"
            summaryRow[lastCol] = val.isFinite ? String(format: "%.0f", val) : "0"
            doc.tables[0].rows.append(summaryRow)
          }
        } else {
          doc.headerText = "표준 기술 문서 머리말"
        }
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
      }
      lastExecutedCommandSummary = "표 셀 수식(=SUM(ABOVE)) 계산 반영 완료"
      return

    case "hwp-mod-header", "hwp-mod-official":
      recordUndoSnapshot(for: tabID)
      if case .document(var doc) = openTabs[idx].payload {
        doc.headerText = "행정기관 표준 공문서 · 문서번호 기안-2026-0201"
        let banner = ParagraphBlock(
          style: .officialHeader, alignment: .center, text: "행 정 공 문 서 두 문 · 결 재 란", fontSizePt: 18,
          isBold: true)
        doc.paragraphs.insert(banner, at: 0)
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
      }
      lastExecutedCommandSummary = "행정 공문서 두문·결재란 삽입 완료"
      return

    case "hwp-mod-field":
      recordUndoSnapshot(for: tabID)
      if case .document(var doc) = openTabs[idx].payload {
        doc.paragraphs.append(
          ParagraphBlock(style: .body, alignment: .justified, text: "누름틀 입력란: {{부서명}} / {{수신자}}"))
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
      }
      lastExecutedCommandSummary = "한글 누름틀 필드 삽입 완료"
      return

    case "hwp-mod-border":
      recordUndoSnapshot(for: tabID)
      if case .document(var doc) = openTabs[idx].payload {
        if doc.tables.isEmpty {
          doc.tables.append(
            DocumentTableBlock(
              caption: "결재란 표", rows: [["담당", "검토", "승인"], ["결재", "결재", "결재"]],
              borderStyle: .doubleLine))
        } else {
          let current = doc.tables[0].borderStyle
          doc.tables[0].borderStyle = (current == .doubleLine) ? .diagonalDown : .doubleLine
        }
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
      }
      lastExecutedCommandSummary = "이중선·대각선 표 테두리 적용 완료"
      return

    case "hwp-mod-manuscript":
      recordUndoSnapshot(for: tabID)
      isManuscriptGridModeEnabled.toggle()
      if case .document(var doc) = openTabs[idx].payload {
        let stats = doc.computeStatistics()
        doc.recordRevision(
          author: "원고지계산기",
          summary: "200자 원고지 환산: \(stats.manuscriptPages200)매 (\(stats.characterCountWithSpaces)자)")
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
        showRightInspector = true
        reviewStatsRefreshCount += 1
        lastExecutedCommandSummary =
          "200자 원고지 매수 검산 완료: \(stats.manuscriptPages200)매 (\(stats.characterCountWithSpaces)자)"
      } else {
        showRightInspector = true
        reviewStatsRefreshCount += 1
        lastExecutedCommandSummary = "200자 원고지 매수 검산 완료"
      }
      return

    case "sheet-mod-pivot":
      recordUndoSnapshot(for: tabID)
      if case .sheet(var sheet) = openTabs[idx].payload {
        let range = sheet.detectedDataRange()
        let valueCol = max(0, range.end.column - range.start.column)
        let agg = selectedPivotAggregation
        let summary = PivotTableEngine.compute(
          from: sheet,
          range: range,
          hasHeaderRow: true,
          configuration: PivotTableConfiguration(
            rowFieldColumnIndex: 0,
            valueFieldColumnIndex: valueCol,
            aggregation: agg
          )
        )
        lastPivotGrandTotal = summary.grandTotal
        var pivotCells: [PackedCellCoordinate: String] = [
          PackedCellCoordinate(row: 0, column: 0): "구분",
          PackedCellCoordinate(row: 0, column: 1): "집계 값 (\(agg.rawValue.uppercased()))",
        ]
        let formatPivotValue = { (value: Double) -> String in
          SheetValueCoercion.formatGeneralNumber((value * 100.0).rounded() / 100.0)
        }
        for (rIdx, rowKey) in summary.rowKeys.enumerated() {
          let rowTotal = summary.rowTotals.indices.contains(rIdx) ? summary.rowTotals[rIdx] : 0.0
          pivotCells[PackedCellCoordinate(row: rIdx + 1, column: 0)] = rowKey
          pivotCells[PackedCellCoordinate(row: rIdx + 1, column: 1)] = formatPivotValue(rowTotal)
        }
        let totalRow = summary.rowKeys.count + 1
        let formattedGrandTotal = formatPivotValue(summary.grandTotal)
        pivotCells[PackedCellCoordinate(row: totalRow, column: 0)] = "총계"
        pivotCells[PackedCellCoordinate(row: totalRow, column: 1)] = formattedGrandTotal
        sheet.addSheetPage(named: "피벗 요약", initialCells: pivotCells)
        openTabs[idx].payload = .sheet(sheet)
        openTabs[idx].isModified = true
        lastExecutedCommandSummary =
          "피벗 테이블 요약 집계 완료 (\(range.a1Range), 총계 \(formattedGrandTotal))"
      } else {
        lastExecutedCommandSummary = "피벗 테이블 요약 집계 완료"
      }
      return

    case "sheet-mod-chart":
      let allKinds = SheetChartKind.allCases
      if let currentIdx = allKinds.firstIndex(of: selectedChartKind) {
        selectedChartKind = allKinds[(currentIdx + 1) % allKinds.count]
      }
      lastExecutedCommandSummary = "차트 시각화 유형 전환: \(selectedChartKind.rawValue)"
      return

    case "sheet-mod-condfmt", "sheet-mod-cond":
      activeConditionalFormatRule = (activeConditionalFormatRule == nil) ? .greaterThan(100.0) : nil
      lastExecutedCommandSummary = "조건부 서식(>100 강조) 규칙 적용 완료"
      return

    case "sheet-mod-freeze":
      isHeaderRowFrozen.toggle()
      lastExecutedCommandSummary = isHeaderRowFrozen ? "행·열 틀 고정 활성화" : "행·열 틀 고정 해제"
      return

    case "slide-mod-master":
      recordUndoSnapshot(for: tabID)
      if case .slide(var deck) = openTabs[idx].payload, !deck.slides.isEmpty {
        let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
        let layouts = SlideMasterLayout.allCases
        let cur = deck.slides[sIdx].masterLayout
        deck.slides[sIdx].masterLayout =
          layouts[((layouts.firstIndex(of: cur) ?? 0) + 1) % layouts.count]
        openTabs[idx].payload = .slide(deck)
        openTabs[idx].isModified = true
      }
      lastExecutedCommandSummary = "슬라이드 마스터 레이아웃 순환 적용 완료"
      return

    case "slide-mod-snap":
      recordUndoSnapshot(for: tabID)
      isSnapGuidesEnabled = true
      if case .slide(var deck) = openTabs[idx].payload, !deck.slides.isEmpty {
        let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
        if deck.slides[sIdx].elements.isEmpty {
          deck.slides[sIdx].elements.append(
            SlideCanvasElement(
              title: "스냅 정렬 개체", bodyText: "중앙 정렬", x: 120, y: 90, width: 200, height: 100,
              kind: .textBox))
        }
        let eIdx =
          deck.slides[sIdx].elements.firstIndex(where: { $0.id == selectedSlideElementID }) ?? 0
        let stage = deck.aspectRatio.stageSize
        let elemW = deck.slides[sIdx].elements[eIdx].width
        let elemH = deck.slides[sIdx].elements[eIdx].height
        deck.slides[sIdx].elements[eIdx].x = (stage.width - elemW) / 2.0
        deck.slides[sIdx].elements[eIdx].y = (stage.height - elemH) / 2.0
        let aligned = deck.slides[sIdx].elements[eIdx]
        let siblings = deck.slides[sIdx].elements.enumerated().compactMap { $0 == eIdx ? nil : $1 }
        activeSnapGuides = SnapGuideCalculator.computeGuides(
          for: aligned, siblings: siblings, aspectRatio: deck.aspectRatio, tolerancePoints: 5.0)
        openTabs[idx].payload = .slide(deck)
        openTabs[idx].isModified = true
      }
      lastExecutedCommandSummary = "스마트 정렬 가이드 정중앙 스냅 완료"
      return

    case "slide-mod-transition":
      recordUndoSnapshot(for: tabID)
      if case .slide(var deck) = openTabs[idx].payload, !deck.slides.isEmpty {
        let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
        let transitions = SlideTransitionKind.allCases
        let cur = deck.slides[sIdx].transition
        deck.slides[sIdx].transition =
          transitions[((transitions.firstIndex(of: cur) ?? 0) + 1) % transitions.count]
        openTabs[idx].payload = .slide(deck)
        openTabs[idx].isModified = true
      }
      lastExecutedCommandSummary = "슬라이드 화면 전환 효과 변경 완료"
      return

    case "slide-mod-presenter":
      isPresenterModeActive.toggle()
      isPresenterTimerRunning = isPresenterModeActive
      lastExecutedCommandSummary = isPresenterModeActive ? "발표자 도구 시작" : "발표자 도구 닫기"
      return

    case "pdf-mod-highlight", "pdf-mod-annotate":
      addPDFAnnotation(kind: .highlight, content: "형광펜 하이라이트 주석")
      lastExecutedCommandSummary = "PDF 형광펜 주석 추가 완료"
      return

    case "pdf-mod-rotate":
      rotateActivePDFPage(clockwiseDegrees: 90)
      lastExecutedCommandSummary = "PDF 페이지 90° 회전 완료"
      return

    case "pdf-mod-splitmerge", "pdf-mod-split":
      recordUndoSnapshot(for: tabID)
      if case .pdf(var pdfDoc) = openTabs[idx].payload {
        let clampedPageNum = min(max(1, selectedPDFPageNumber), max(1, pdfDoc.pages.count))
        if pdfDoc.pages.count > 1 {
          let extracted = pdfDoc.extractPages(
            oneBasedPageNumbers: [clampedPageNum],
            newTitle: "\(pdfDoc.title) (\(clampedPageNum)페이지 추출)")
          let baseName = openTabs[idx].fileName.replacingOccurrences(of: ".pdf", with: "")
          let splitTab = WorkspaceTabItem(
            fileName: "\(baseName)_분할_P\(clampedPageNum).pdf", format: .pdf, isModified: true,
            payload: .pdf(extracted))
          openTabs.append(splitTab)
        } else {
          pdfDoc.insertBlankPage(at: clampedPageNum)
          openTabs[idx].payload = .pdf(pdfDoc)
          openTabs[idx].isModified = true
        }
      }
      lastExecutedCommandSummary = "PDF 페이지(\(selectedPDFPageNumber)p) 분할·병합 실행 완료"
      return

    case "pdf-mod-tooffice", "pdf-mod-convert":
      if case .pdf(let pdfDoc) = openTabs[idx].payload {
        let editable = pdfDoc.convertToEditableDocument(targetFormat: .docx)
        let convertedTab = WorkspaceTabItem(
          fileName: "\(pdfDoc.title)_변환본.docx", format: .docx, isModified: true,
          payload: .document(editable))
        openTabs.append(convertedTab)
        activeTabID = convertedTab.id
      }
      lastExecutedCommandSummary = "PDF → 편집 문서(DOCX) 변환 완료"
      return

    default:
      break
    }

    if commandID.hasSuffix("-review-track") {
      recordUndoSnapshot(for: tabID)
      switch openTabs[idx].payload {
      case .document(var doc):
        doc.trackChangesEnabled.toggle()
        openTabs[idx].payload = .document(doc)
      case .sheet(var sheet):
        sheet.trackChangesEnabled.toggle()
        openTabs[idx].payload = .sheet(sheet)
      case .slide:
        isPresenterModeActive.toggle()
      case .pdf(var pdfDoc):
        pdfDoc.annotations.append(
          PDFAnnotationItem(
            pageNumber: selectedPDFPageNumber, kind: .stickyNote, content: "변경 내용 추적 감사 마크",
            author: authorName))
        openTabs[idx].payload = .pdf(pdfDoc)
      }
      openTabs[idx].isModified = true
      lastExecutedCommandSummary = "변경 내용 추적 상태 전환 완료"
      return
    }
    if commandID.hasSuffix("-review-comment") {
      recordUndoSnapshot(for: tabID)
      let noteText =
        commentDraftText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        ? "검토 의견이 등록되었습니다." : commentDraftText
      switch openTabs[idx].payload {
      case .document(var doc):
        let anchorID = doc.paragraphs.first?.id ?? UUID()
        doc.comments.append(
          DocumentCommentThread(anchorParagraphID: anchorID, author: authorName, message: noteText))
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
      case .pdf(var pdfDoc):
        pdfDoc.annotations.append(
          PDFAnnotationItem(
            pageNumber: selectedPDFPageNumber, kind: .stickyNote, content: noteText,
            author: authorName))
        openTabs[idx].payload = .pdf(pdfDoc)
        openTabs[idx].isModified = true
      case .slide(var deck):
        if !deck.slides.isEmpty {
          let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
          let offset = Double(deck.slides[sIdx].elements.count) * 12.0
          let callout = SlideCanvasElement(
            title: "검토 메모 (\(authorName))", bodyText: noteText, x: 220.0 + offset,
            y: 240.0 + offset, width: 210, height: 80, kind: .calloutShape)
          deck.slides[sIdx].elements.append(callout)
          selectedSlideElementID = callout.id
          openTabs[idx].payload = .slide(deck)
          openTabs[idx].isModified = true
        }
      case .sheet(var sheet):
        let coord =
          (try? CellAddress(a1Notation: selectedSheetCell))?.packedCoordinate
          ?? PackedCellCoordinate(row: 0, column: 0)
        if let existing = sheet.sheetComments[coord], !existing.isEmpty {
          sheet.sheetComments[coord] = "\(existing) · \(noteText)"
        } else {
          sheet.sheetComments[coord] = noteText
        }
        openTabs[idx].payload = .sheet(sheet)
        openTabs[idx].isModified = true
      }
      lastExecutedCommandSummary = "검토 메모 스레드 등록 완료"
      return
    }
    if commandID.hasSuffix("-review-verify") || commandID.hasSuffix("-review-security") {
      if case .sheet(var sheet) = openTabs[idx].payload {
        recordUndoSnapshot(for: tabID)
        sheet.trackChangesEnabled = true
        openTabs[idx].payload = .sheet(sheet)
        openTabs[idx].isModified = true
      }
      runSecurityAuditOnActiveTab()
      lastExecutedCommandSummary = "매직 바이트 및 컨테이너 보안 검증 완료"
      return
    }
    if commandID.hasSuffix("-review-stats") {
      showRightInspector = true
      reviewStatsRefreshCount += 1
      lastExecutedCommandSummary = "문서 통계 및 원고지 매수 인스펙터 갱신 완료"
      return
    }

    lastExecutedCommandSummary = "명령 실행: \(commandID)"
  }

  // MARK: - Private Command Helpers

  private enum HomeFormattingAction { case bold, italic, underline, styleCycle }

  private static func shouldEnableAttribute(
    in paragraph: ParagraphBlock,
    utf16Range: Range<Int>,
    keyPath: KeyPath<DocumentTextRun, Bool>
  ) -> Bool {
    var cursor = 0
    var foundOverlap = false
    var allEnabled = true
    for run in paragraph.effectiveRuns {
      let len = run.text.utf16.count
      let runRange = cursor..<(cursor + len)
      cursor += len
      if runRange.overlaps(utf16Range) {
        foundOverlap = true
        if !run[keyPath: keyPath] { allEnabled = false }
      }
    }
    return !(foundOverlap && allEnabled)
  }

  private func applyHomeFormatting(at idx: Int, action: HomeFormattingAction) {
    switch openTabs[idx].payload {
    case .document(var doc):
      if doc.paragraphs.isEmpty { doc.paragraphs.append(ParagraphBlock(text: "기본 본문 문단")) }
      let pIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
      if let utf16Range = selectedParagraphUTF16Range, !utf16Range.isEmpty, action != .styleCycle {
        let p = doc.paragraphs[pIdx]
        switch action {
        case .bold:
          let next = Self.shouldEnableAttribute(in: p, utf16Range: utf16Range, keyPath: \.isBold)
          doc.paragraphs[pIdx].applyRunFormatting(utf16Range: utf16Range, isBold: next)
        case .italic:
          let next = Self.shouldEnableAttribute(in: p, utf16Range: utf16Range, keyPath: \.isItalic)
          doc.paragraphs[pIdx].applyRunFormatting(utf16Range: utf16Range, isItalic: next)
        case .underline:
          let next = Self.shouldEnableAttribute(
            in: p, utf16Range: utf16Range, keyPath: \.isUnderlined)
          doc.paragraphs[pIdx].applyRunFormatting(utf16Range: utf16Range, isUnderlined: next)
        case .styleCycle: break
        }
      } else {
        switch action {
        case .bold:
          doc.paragraphs[pIdx].isBold.toggle()
          let nextVal = doc.paragraphs[pIdx].isBold
          for rIdx in doc.paragraphs[pIdx].runs.indices {
            doc.paragraphs[pIdx].runs[rIdx].isBold = nextVal
          }
        case .italic:
          doc.paragraphs[pIdx].isItalic.toggle()
          let nextVal = doc.paragraphs[pIdx].isItalic
          for rIdx in doc.paragraphs[pIdx].runs.indices {
            doc.paragraphs[pIdx].runs[rIdx].isItalic = nextVal
          }
        case .underline:
          doc.paragraphs[pIdx].isUnderlined.toggle()
          let nextVal = doc.paragraphs[pIdx].isUnderlined
          for rIdx in doc.paragraphs[pIdx].runs.indices {
            doc.paragraphs[pIdx].runs[rIdx].isUnderlined = nextVal
          }
        case .styleCycle:
          let allStyles = ParagraphStyleRole.allCases
          let cur = doc.paragraphs[pIdx].style
          doc.paragraphs[pIdx].style =
            allStyles[((allStyles.firstIndex(of: cur) ?? 0) + 1) % allStyles.count]
        }
      }
      openTabs[idx].payload = .document(doc)
      openTabs[idx].isModified = true

    case .sheet(var sheet):
      let coord =
        (try? CellAddress(a1Notation: selectedSheetCell))?.packedCoordinate
        ?? PackedCellCoordinate(row: 0, column: 0)
      var style = sheet.cellStyle(at: coord)
      switch action {
      case .bold: style.isBold.toggle()
      case .italic: style.isItalic.toggle()
      case .underline: style.isUnderlined.toggle()
      case .styleCycle:
        let formats = CellNumberFormat.allCases
        let curIdx = formats.firstIndex(of: style.numberFormat) ?? 0
        style.numberFormat = formats[(curIdx + 1) % formats.count]
        style.fillHex = (style.fillHex == nil) ? "#EEF0FF" : nil
      }
      sheet.setCellStyle(style, at: coord)
      if let range = try? CellRange(notation: selectedSheetRange), range.start != range.end {
        sheet.updateCellStyleInRange(range) { cellStyle in
          switch action {
          case .bold: cellStyle.isBold = style.isBold
          case .italic: cellStyle.isItalic = style.isItalic
          case .underline: cellStyle.isUnderlined = style.isUnderlined
          case .styleCycle:
            cellStyle.numberFormat = style.numberFormat
            cellStyle.fillHex = style.fillHex
          }
        }
      }
      openTabs[idx].payload = .sheet(sheet)
      openTabs[idx].isModified = true

    case .slide(var deck):
      guard !deck.slides.isEmpty else { return }
      let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
      if deck.slides[sIdx].elements.isEmpty {
        deck.slides[sIdx].elements.append(
          SlideCanvasElement(
            title: "기본 텍스트 상자", bodyText: "슬라이드 본문", x: 200, y: 245, width: 240, height: 150,
            kind: .textBox))
      }
      let eIdx =
        deck.slides[sIdx].elements.firstIndex(where: { $0.id == selectedSlideElementID }) ?? 0
      switch action {
      case .bold: deck.slides[sIdx].elements[eIdx].isBold.toggle()
      case .italic: deck.slides[sIdx].elements[eIdx].isItalic.toggle()
      case .underline: deck.slides[sIdx].elements[eIdx].isUnderlined.toggle()
      case .styleCycle:
        let layouts = SlideMasterLayout.allCases
        let cur = deck.slides[sIdx].masterLayout
        deck.slides[sIdx].masterLayout =
          layouts[((layouts.firstIndex(of: cur) ?? 0) + 1) % layouts.count]
      }
      openTabs[idx].payload = .slide(deck)
      openTabs[idx].isModified = true

    case .pdf(var pdfDoc):
      let pageNum =
        pdfDoc.pages.indices.contains(selectedPDFPageIndex)
        ? pdfDoc.pages[selectedPDFPageIndex].pageNumber : 1
      let annKind: PDFAnnotationKind = (action == .underline) ? .underline : .highlight
      pdfDoc.annotations.append(
        PDFAnnotationItem(
          pageNumber: pageNum, kind: annKind, content: "서식 강조 표시", author: authorName))
      openTabs[idx].payload = .pdf(pdfDoc)
      openTabs[idx].isModified = true
    }
  }
}
