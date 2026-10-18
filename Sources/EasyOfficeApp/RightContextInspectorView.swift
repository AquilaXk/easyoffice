import EasyOfficeCore
import EasyOfficeDesignSystem
import Observation
import SwiftUI

typealias RightContextualInspectorView = RightContextInspectorView

struct RightContextInspectorView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  let tab: WorkspaceTabItem

  var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)

    ScrollView {
      VStack(alignment: .leading, spacing: 12) {
        Text("속성 인스펙터")
          .font(.system(size: 12, weight: .bold))
          .foregroundStyle(palette.primaryText.swiftUIColor)

        Text("파일명: \(tab.fileName)")
          .font(.system(size: 11))
          .foregroundStyle(palette.primaryText.swiftUIColor)
        Text("규격: \(tab.format.standardSpecification)")
          .font(.system(size: 11))
          .foregroundStyle(palette.secondaryText.swiftUIColor)

        HStack(spacing: 6) {
          Text("작성자:")
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
          TextField("작성자 이름", text: $viewModel.authorName)
            .textFieldStyle(.roundedBorder)
            .font(.system(size: 11))
        }

        Divider()

        switch tab.payload {
        case .document(let doc):
          documentInspectorSection(doc: doc, palette: palette)
        case .sheet(let sheet):
          sheetInspectorSection(sheet: sheet, palette: palette)
        case .slide(let deck):
          slideInspectorSection(deck: deck, palette: palette)
        case .pdf(let pdf):
          pdfInspectorSection(pdf: pdf, palette: palette)
        }

        Spacer()
      }
      .padding(14)
    }
    .background(palette.cardSurface.swiftUIColor)
  }

  // MARK: - Document & HWP Inspector

  @ViewBuilder
  private func documentInspectorSection(doc: OfficeDocumentModel, palette: WorkspaceThemePalette)
    -> some View
  {
    let stats = doc.computeStatistics()
    let modulePrefix = tab.moduleKind == .hwp ? "hwp" : "doc"
    let currentOrientation = doc.sections.first?.orientation ?? .portrait
    let currentColumns = doc.sections.first?.columnCount ?? 1

    Text("문서 통계 및 원고지 검산")
      .font(.system(size: 11, weight: .bold))
      .foregroundStyle(palette.primaryText.swiftUIColor)
    Text(
      "문단 수: \(stats.paragraphCount) · 표: \(stats.tableCount) · 이미지: \(doc.images.count) · 수식: \(doc.equations.count)"
    )
    .font(.system(size: 11))
    Text("단어 수: \(stats.wordCount) · 글자 수(공백 포함): \(stats.characterCountWithSpaces)")
      .font(.system(size: 11))
    Text("200자 원고지: \(stats.manuscriptPages200)매")
      .font(.system(size: 11, weight: .semibold))
      .foregroundStyle(palette.accentBadgeText.swiftUIColor)
    Text("각주: \(doc.footnotes.count)건 · 변경 추적: \(doc.revisions.count)건")
      .font(.system(size: 11))
      .foregroundStyle(palette.secondaryText.swiftUIColor)

    Divider()

    Text("레이아웃 및 구역 제어")
      .font(.system(size: 11, weight: .bold))
      .foregroundStyle(palette.primaryText.swiftUIColor)

    Picker(
      "용지 규격",
      selection: Binding(
        get: { doc.paperSize },
        set: { newSize in
          mutateDocument { d in
            if d.sections.isEmpty { d.sections = [DocumentSectionLayout(paperSize: newSize)] }
            d.paperSize = newSize
            d.sections[0].paperSize = newSize
          }
        }
      )
    ) {
      ForEach(PaperSizePreset.allCases, id: \.self) { preset in
        Text(preset.rawValue).tag(preset)
      }
    }
    .pickerStyle(.menu)
    .controlSize(.small)

    Picker(
      "용지 방향",
      selection: Binding(
        get: { currentOrientation },
        set: { newOrientation in
          mutateDocument { d in
            if d.sections.isEmpty { d.sections = [DocumentSectionLayout(paperSize: d.paperSize)] }
            d.sections[0].orientation = newOrientation
          }
        }
      )
    ) {
      ForEach(PageOrientation.allCases, id: \.self) { orient in
        Text(orient.rawValue).tag(orient)
      }
    }
    .pickerStyle(.segmented)
    .controlSize(.small)

    Stepper(
      "다단 열 수: \(currentColumns)단",
      value: Binding(
        get: { currentColumns },
        set: { newCols in
          mutateDocument { d in
            if d.sections.isEmpty { d.sections = [DocumentSectionLayout(paperSize: d.paperSize)] }
            d.sections[0].columnCount = max(1, min(3, newCols))
          }
        }
      ), in: 1...3
    )
    .font(.system(size: 11))

    Toggle(
      "변경 내용 추적",
      isOn: Binding(
        get: { doc.trackChangesEnabled },
        set: { enabled in mutateDocument { $0.trackChangesEnabled = enabled } }
      )
    )
    .font(.system(size: 11, weight: .medium))

    VStack(alignment: .leading, spacing: 4) {
      Text("머리말 / 꼬리말")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      TextField(
        "머리말 텍스트",
        text: Binding(
          get: { doc.headerText },
          set: { text in mutateDocument { $0.headerText = text } }
        )
      )
      .textFieldStyle(.roundedBorder)
      .font(.system(size: 11))

      TextField(
        "꼬리말 텍스트",
        text: Binding(
          get: { doc.footerText },
          set: { text in mutateDocument { $0.footerText = text } }
        )
      )
      .textFieldStyle(.roundedBorder)
      .font(.system(size: 11))
    }

    Divider()

    VStack(alignment: .leading, spacing: 6) {
      Text("수식 블록 삽입")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 6) {
        TextField("LaTeX / 수식 입력", text: $viewModel.equationDraftText)
          .textFieldStyle(.roundedBorder)
          .font(.system(size: 11, design: .monospaced))
        Button("수식 삽입") { viewModel.insertEquationIntoActiveDocument() }
          .buttonStyle(.bordered)
          .controlSize(.small)
      }

      Text("워터마크 적용/해제")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      TextField("워터마크 문구", text: $viewModel.watermarkDraftText)
        .textFieldStyle(.roundedBorder)
        .font(.system(size: 11))
      HStack(spacing: 6) {
        Button("워터마크 적용") { viewModel.setDocumentWatermark(viewModel.watermarkDraftText) }
          .buttonStyle(.bordered)
          .controlSize(.small)
        Button("해제") { viewModel.setDocumentWatermark("") }
          .buttonStyle(.bordered)
          .controlSize(.small)
      }

      if let firstImage = doc.images.first {
        Text("이미지 배치: \(firstImage.wrapMode.rawValue)")
          .font(.system(size: 10, weight: .medium))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        Button("이미지 텍스트 배치 전환") { viewModel.cycleImageWrapMode(imageID: firstImage.id) }
          .buttonStyle(.bordered)
          .controlSize(.small)
      }

      Text("인라인 강조 · 취소선 · 하이퍼링크")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 6) {
        Button("형광펜 강조") { viewModel.toggleHighlightOnActiveParagraph() }
          .buttonStyle(.bordered)
          .controlSize(.small)
        Button("취소선") { viewModel.toggleStrikethroughOnActiveParagraph() }
          .buttonStyle(.bordered)
          .controlSize(.small)
      }
      HStack(spacing: 6) {
        TextField("하이퍼링크 URL", text: $viewModel.inlineLinkURLDraft)
          .textFieldStyle(.roundedBorder)
          .font(.system(size: 11))
        Button("링크 삽입") { viewModel.insertHyperlinkIntoActiveParagraph() }
          .buttonStyle(.bordered)
          .controlSize(.small)
      }

      Text("문단 들여쓰기 (첫 줄 / 좌측)")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 6) {
        Button("첫 줄 +10pt") {
          viewModel.adjustActiveParagraphIndent(firstLineDeltaPt: 10, leftIndentDeltaPt: 0)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
        Button("좌측 +10pt") {
          viewModel.adjustActiveParagraphIndent(firstLineDeltaPt: 0, leftIndentDeltaPt: 10)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
        Button("초기화") {
          viewModel.adjustActiveParagraphIndent(firstLineDeltaPt: -144, leftIndentDeltaPt: -144)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
      }
    }

    documentAdvancedControlsSubSection(doc: doc, palette: palette)

    Divider()

    VStack(alignment: .leading, spacing: 6) {
      Button("변경 내용 추적 전환") { viewModel.executeRibbonCommand("\(modulePrefix)-review-track") }
        .buttonStyle(.bordered).controlSize(.small)
      Button("단 나누기 전환") { viewModel.executeRibbonCommand("\(modulePrefix)-layout-columns") }
        .buttonStyle(.bordered).controlSize(.small)
      Button("용지 방향 전환") { viewModel.executeRibbonCommand("\(modulePrefix)-layout-orient") }
        .buttonStyle(.bordered).controlSize(.small)
      Button("개인정보/메타데이터 정리") {
        viewModel.executeRibbonCommand("\(modulePrefix)-review-security")
        viewModel.scrubActiveTabMetadata()
      }
      .buttonStyle(.borderedProminent)
      .controlSize(.small)
      .tint(palette.primaryAccent.swiftUIColor)
    }
  }

  @ViewBuilder
  private func documentAdvancedControlsSubSection(
    doc: OfficeDocumentModel,
    palette: WorkspaceThemePalette
  ) -> some View {
    let activeListKind =
      doc.paragraphs.first(where: { $0.id == viewModel.selectedParagraphID })?.listKind
      ?? doc.paragraphs.first?.listKind
      ?? .koreanOfficial

    VStack(alignment: .leading, spacing: 6) {
      Text("문단 정렬 · 줄간격 · 다단계 목록")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 4) {
        Button("좌측") { viewModel.setActiveParagraphAlignment(.leading) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("가운데") { viewModel.setActiveParagraphAlignment(.center) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("우측") { viewModel.setActiveParagraphAlignment(.trailing) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("양쪽") { viewModel.setActiveParagraphAlignment(.justified) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
      }
      HStack(spacing: 4) {
        ForEach([100.0, 115.0, 150.0, 160.0, 200.0], id: \.self) { pct in
          Button("\(Int(pct))%") {
            viewModel.setActiveParagraphLineSpacing(pct)
          }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        }
      }
      HStack(spacing: 4) {
        Button("글머리") { viewModel.setActiveParagraphListKind(.bullet) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("번호") { viewModel.setActiveParagraphListKind(.numbered) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("공문서(1.가.)") { viewModel.setActiveParagraphListKind(.koreanOfficial) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("해제") { viewModel.setActiveParagraphListKind(.none) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
      }
      HStack(spacing: 6) {
        Button("수준 +1") {
          viewModel.setActiveParagraphListKind(
            activeListKind == .none ? .koreanOfficial : activeListKind,
            levelDelta: 1
          )
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
        Button("수준 -1") {
          viewModel.setActiveParagraphListKind(
            activeListKind == .none ? .koreanOfficial : activeListKind,
            levelDelta: -1
          )
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
      }
    }

    VStack(alignment: .leading, spacing: 6) {
      if !doc.tables.isEmpty {
        Text("표 행·열 및 셀 병합·테두리 편집")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        HStack(spacing: 6) {
          Button("+행") { viewModel.insertRowInDocumentTable() }
            .buttonStyle(.bordered)
            .controlSize(.mini)
          Button("-행") { viewModel.deleteRowInDocumentTable() }
            .buttonStyle(.bordered)
            .controlSize(.mini)
          Button("+열") { viewModel.insertColumnInDocumentTable() }
            .buttonStyle(.bordered)
            .controlSize(.mini)
          Button("-열") { viewModel.deleteColumnInDocumentTable() }
            .buttonStyle(.bordered)
            .controlSize(.mini)
        }
        HStack(spacing: 6) {
          Button("셀 병합") { viewModel.mergeActiveDocumentTableCells() }
            .buttonStyle(.bordered)
            .controlSize(.mini)
          Button("셀 분할") { viewModel.splitActiveDocumentTableCell() }
            .buttonStyle(.bordered)
            .controlSize(.mini)
          Button("테두리 전환") {
            let styles = TableBorderStyle.allCases
            let cur = doc.tables[0].borderStyle
            let next = styles[((styles.firstIndex(of: cur) ?? 0) + 1) % styles.count]
            viewModel.setActiveDocumentTableBorderStyle(next)
          }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        }
      }

      Text("메일 머지 (누름틀 치환)")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 4) {
        TextField("필드 키", text: $viewModel.mailMergeFieldKeyDraft)
          .textFieldStyle(.roundedBorder)
          .font(.system(size: 11))
        TextField("치환 값", text: $viewModel.mailMergeFieldValueDraft)
          .textFieldStyle(.roundedBorder)
          .font(.system(size: 11))
        Button("필드 치환") { viewModel.applyMailMergeFieldReplacement() }
          .buttonStyle(.bordered)
          .controlSize(.small)
      }

      Toggle("200자 원고지 격자 뷰", isOn: $viewModel.isManuscriptGridModeEnabled)
        .font(.system(size: 11, weight: .semibold))

      Button("미주(Endnote) 삽입") { viewModel.insertEndnoteIntoActiveDocument() }
        .buttonStyle(.bordered)
        .controlSize(.small)
    }
  }

  // MARK: - Sheet Inspector

  @ViewBuilder
  private func sheetInspectorSection(sheet: WorksheetModel, palette: WorkspaceThemePalette)
    -> some View
  {
    let selectedAddr =
      (try? CellAddress(a1Notation: viewModel.selectedSheetCell))
      ?? CellAddress(packed: PackedCellCoordinate(row: 0, column: 0))
    let formattedResult = sheet.formattedDisplayValue(at: selectedAddr)
    let currentStyle = sheet.cellStyle(at: selectedAddr.packedCoordinate)
    let mergedList = viewModel.mergedSheetRanges(for: tab.id).joined(separator: ", ")
    let activeRange =
      (try? CellRange(notation: viewModel.selectedSheetRange))
      ?? (try? CellRange(notation: viewModel.selectedSheetCell))
      ?? sheet.detectedNumericRange()
    let rangeSummary = sheet.summarizeRange(activeRange)
    let colWidth = sheet.columnWidth(at: selectedAddr.column)
    let rowHeight = sheet.rowHeight(at: selectedAddr.row)

    Text("시트 수식 및 셀 서식 제어")
      .font(.system(size: 11, weight: .bold))
      .foregroundStyle(palette.primaryText.swiftUIColor)
    Text("선택된 셀: \(viewModel.selectedSheetCell) · 선택 범위: \(viewModel.selectedSheetRange)")
      .font(.system(size: 11, design: .monospaced))
    Text("원시 수식: \(sheet.rawValue(at: viewModel.selectedSheetCell))")
      .font(.system(size: 11, design: .monospaced))
    Text("평가 결과: \(formattedResult)")
      .font(.system(size: 11, weight: .semibold))
      .foregroundStyle(palette.accentBadgeText.swiftUIColor)
    Text(
      "SUM: \(String(format: "%.1f", rangeSummary.sum)) · AVG: \(String(format: "%.1f", rangeSummary.average)) · COUNT: \(rangeSummary.numericCount)/\(rangeSummary.cellCount)"
    )
    .font(.system(size: 10, design: .monospaced))
    .foregroundStyle(palette.secondaryText.swiftUIColor)
    Text(
      "MIN: \(rangeSummary.minimum.map { String(format: "%.1f", $0) } ?? "-") · MAX: \(rangeSummary.maximum.map { String(format: "%.1f", $0) } ?? "-")"
    )
    .font(.system(size: 10, design: .monospaced))
    .foregroundStyle(palette.secondaryText.swiftUIColor)

    Divider()

    Picker(
      "숫자 표시 형식",
      selection: Binding(
        get: { currentStyle.numberFormat },
        set: { viewModel.setSelectedSheetCellNumberFormat($0) }
      )
    ) {
      ForEach(CellNumberFormat.allCases, id: \.self) { fmt in
        Text(fmt.rawValue).tag(fmt)
      }
    }
    .pickerStyle(.menu)
    .controlSize(.small)

    Picker(
      "가로 정렬",
      selection: Binding(
        get: { currentStyle.alignment },
        set: { viewModel.setSelectedSheetCellAlignment($0) }
      )
    ) {
      ForEach(CellHorizontalAlignment.allCases, id: \.self) { align in
        Text(align.rawValue).tag(align)
      }
    }
    .pickerStyle(.segmented)
    .controlSize(.small)

    VStack(alignment: .leading, spacing: 4) {
      Text("선택 셀 메모 (\(viewModel.selectedSheetCell))")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      TextField(
        "셀 메모 입력",
        text: Binding(
          get: { sheet.sheetComments[selectedAddr.packedCoordinate] ?? "" },
          set: { viewModel.setSelectedSheetCellComment($0) }
        )
      )
      .textFieldStyle(.roundedBorder)
      .font(.system(size: 11))
    }

    VStack(alignment: .leading, spacing: 6) {
      Button("아래로 3행 자동 채우기") {
        let minCol = min(activeRange.start.column, activeRange.end.column)
        let maxCol = max(activeRange.start.column, activeRange.end.column)
        let minRow = min(activeRange.start.row, activeRange.end.row)
        let maxRow = max(activeRange.start.row, activeRange.end.row)
        if let startAddr = try? CellAddress(column: minCol, row: minRow),
          let endAddr = try? CellAddress(
            column: maxCol, row: min(CellAddress.maxZeroBasedRow, maxRow + 3))
        {
          viewModel.performAutoFillOnActiveSheet(
            sourceRangeNotation: activeRange.a1Range,
            targetRangeNotation: "\(startAddr.a1Notation):\(endAddr.a1Notation)"
          )
        }
      }
      .buttonStyle(.bordered).controlSize(.small)

      HStack(spacing: 6) {
        Text("열 폭 (\(Int(colWidth))pt):")
          .font(.system(size: 10, design: .monospaced))
        Button("-20pt") {
          viewModel.updateActiveSheetColumnWidth(column: selectedAddr.column, width: colWidth - 20)
        }
        .buttonStyle(.bordered).controlSize(.mini)
        Button("+20pt") {
          viewModel.updateActiveSheetColumnWidth(column: selectedAddr.column, width: colWidth + 20)
        }
        .buttonStyle(.bordered).controlSize(.mini)
      }

      HStack(spacing: 6) {
        Text("행 높이 (\(Int(rowHeight))pt):")
          .font(.system(size: 10, design: .monospaced))
        Button("-20pt") {
          viewModel.updateActiveSheetRowHeight(row: selectedAddr.row, height: rowHeight - 20)
        }
        .buttonStyle(.bordered).controlSize(.mini)
        Button("+20pt") {
          viewModel.updateActiveSheetRowHeight(row: selectedAddr.row, height: rowHeight + 20)
        }
        .buttonStyle(.bordered).controlSize(.mini)
      }

      HStack(spacing: 6) {
        Button("+행 삽입") { viewModel.insertSheetRowAtSelection() }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("-행 삭제") { viewModel.deleteSheetRowAtSelection() }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("+열 삽입") { viewModel.insertSheetColumnAtSelection() }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("-열 삭제") { viewModel.deleteSheetColumnAtSelection() }
          .buttonStyle(.bordered)
          .controlSize(.mini)
      }

      Stepper(
        "자동 필터 기준 열: \(CellAddress.columnLabel(for: max(0, viewModel.sheetAutoFilterColumn)))열",
        value: $viewModel.sheetAutoFilterColumn, in: 0...15
      )
      .font(.system(size: 10))
      TextField("열 자동 필터 검색어", text: $viewModel.sheetAutoFilterQuery)
        .textFieldStyle(.roundedBorder)
        .font(.system(size: 11))

      Button("선택 셀 병합 / 해제") { viewModel.toggleMergeOnSelectedSheetCell() }
        .buttonStyle(.bordered).controlSize(.small)

      if !mergedList.isEmpty {
        Text("병합 범위: \(mergedList)")
          .font(.system(size: 10, design: .monospaced))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
      }

      Divider()

      Text("이름 정의 (Named Range)")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 6) {
        TextField("이름", text: $viewModel.namedRangeNameDraft)
          .textFieldStyle(.roundedBorder)
          .font(.system(size: 11))
        TextField("범위 (예: B2:B4)", text: $viewModel.namedRangeTargetDraft)
          .textFieldStyle(.roundedBorder)
          .font(.system(size: 11, design: .monospaced))
      }
      Button("이름 정의 등록") { viewModel.defineNamedRangeOnActiveSheet() }
        .buttonStyle(.bordered)
        .controlSize(.small)

      Divider()

      Text("목표값 찾기 (Goal Seek)")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 4) {
        TextField("수식 셀", text: $viewModel.goalSeekTargetCellDraft)
          .textFieldStyle(.roundedBorder)
          .font(.system(size: 10, design: .monospaced))
        TextField("목표값", text: $viewModel.goalSeekTargetValueDraft)
          .textFieldStyle(.roundedBorder)
          .font(.system(size: 10, design: .monospaced))
        TextField("변경 셀", text: $viewModel.goalSeekVariableCellDraft)
          .textFieldStyle(.roundedBorder)
          .font(.system(size: 10, design: .monospaced))
      }
      Button("목표값 찾기 실행") { viewModel.runGoalSeekOnActiveSheet() }
        .buttonStyle(.bordered)
        .controlSize(.small)

      Divider()

      Button("통화/백분율/소수 서식 전환") { viewModel.executeRibbonCommand("sheet-home-style") }
        .buttonStyle(.bordered).controlSize(.small)

      Toggle("머리글 행 고정", isOn: $viewModel.isHeaderRowFrozen)
        .font(.system(size: 11, weight: .medium))

      Toggle("첫 열(A열) 틀 고정", isOn: $viewModel.isFirstColumnFrozen)
        .font(.system(size: 11, weight: .medium))

      Toggle("셀 내 데이터 막대 표시", isOn: $viewModel.isDataBarsEnabled)
        .font(.system(size: 11, weight: .medium))

      Picker("피벗 집계", selection: $viewModel.selectedPivotAggregation) {
        ForEach(
          [
            PivotAggregationFunction.sum,
            .average,
            .count,
            .min,
            .max,
          ],
          id: \.self
        ) { fn in
          Text(fn.rawValue).tag(fn)
        }
      }
      .pickerStyle(.menu)
      .controlSize(.small)

      Text("데이터 정제 (중복 제거 · 텍스트 나누기)")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 6) {
        Button("중복 행 제거") { viewModel.removeDuplicatesInActiveSheet() }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("텍스트 나누기(,)") { viewModel.splitActiveSheetColumnByDelimiter() }
          .buttonStyle(.bordered)
          .controlSize(.mini)
      }

      Button(viewModel.activeConditionalFormatRule == nil ? "조건부 서식 켜기 (>100)" : "조건부 서식 끄기") {
        viewModel.executeRibbonCommand("sheet-mod-condfmt")
      }
      .buttonStyle(.bordered).controlSize(.small)

      Button("시트 추가 (+)") { viewModel.addWorksheetTab() }
        .buttonStyle(.borderedProminent)
        .controlSize(.small)
        .tint(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)
    }
  }

  // MARK: - Slide Inspector

  @ViewBuilder
  private func slideInspectorSection(deck: PresentationDeckModel, palette: WorkspaceThemePalette)
    -> some View
  {
    let sIdx =
      deck.slides.indices.contains(viewModel.selectedSlideIndex) ? viewModel.selectedSlideIndex : 0
    let activeSlide = deck.slides.indices.contains(sIdx) ? deck.slides[sIdx] : nil
    let targetElementID = viewModel.selectedSlideElementID ?? activeSlide?.elements.first?.id
    let activeElement = activeSlide?.elements.first(where: { $0.id == targetElementID })

    Text("슬라이드 마스터 및 개체 제어")
      .font(.system(size: 11, weight: .bold))
      .foregroundStyle(palette.primaryText.swiftUIColor)
    Text("총 슬라이드: \(deck.slides.count)장")
      .font(.system(size: 11))

    Picker(
      "마스터 테마",
      selection: Binding(
        get: { deck.theme },
        set: { viewModel.applyThemeToActiveDeck($0) }
      )
    ) {
      ForEach(SlideDeckTheme.allCases, id: \.self) { theme in
        Text(theme.rawValue).tag(theme)
      }
    }
    .pickerStyle(.menu)
    .controlSize(.small)

    Picker(
      "화면 비율",
      selection: Binding(
        get: { deck.aspectRatio },
        set: { newRatio in mutateSlideDeck { $0.aspectRatio = newRatio } }
      )
    ) {
      ForEach(SlideAspectRatio.allCases, id: \.self) { ratio in
        Text(ratio.rawValue).tag(ratio)
      }
    }
    .pickerStyle(.menu)
    .controlSize(.small)

    if let slide = activeSlide {
      Picker(
        "마스터 레이아웃",
        selection: Binding(
          get: { slide.masterLayout },
          set: { newLayout in
            mutateSlideDeck { d in
              if d.slides.indices.contains(sIdx) { d.slides[sIdx].masterLayout = newLayout }
            }
          }
        )
      ) {
        ForEach(SlideMasterLayout.allCases, id: \.self) { layout in
          Text(layout.rawValue).tag(layout)
        }
      }
      .pickerStyle(.menu)
      .controlSize(.small)

      Picker(
        "화면 전환",
        selection: Binding(
          get: { slide.transition },
          set: { newTrans in
            mutateSlideDeck { d in
              if d.slides.indices.contains(sIdx) { d.slides[sIdx].transition = newTrans }
            }
          }
        )
      ) {
        ForEach(SlideTransitionKind.allCases, id: \.self) { trans in
          Text(trans.rawValue).tag(trans)
        }
      }
      .pickerStyle(.menu)
      .controlSize(.small)

      VStack(alignment: .leading, spacing: 4) {
        Text("발표자 노트")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        TextField(
          "발표자 노트 입력",
          text: Binding(
            get: { slide.speakerNotes },
            set: { viewModel.updateSlideSpeakerNotes(slideIndex: sIdx, notes: $0) }
          )
        )
        .textFieldStyle(.roundedBorder)
        .font(.system(size: 11))
      }
    }

    Divider()

    if let elem = activeElement, let elementID = targetElementID {
      Text("선택 개체 편집 (\(Int(elem.x)), \(Int(elem.y)) · \(Int(elem.width))×\(Int(elem.height)))")
        .font(.system(size: 10, weight: .semibold, design: .monospaced))
        .foregroundStyle(palette.secondaryText.swiftUIColor)

      TextField(
        "개체 제목",
        text: Binding(
          get: { elem.title },
          set: {
            viewModel.updateSlideElementText(
              slideIndex: sIdx, elementID: elementID, title: $0, bodyText: nil)
          }
        )
      )
      .textFieldStyle(.roundedBorder)
      .font(.system(size: 11))

      TextField(
        "개체 본문",
        text: Binding(
          get: { elem.bodyText },
          set: {
            viewModel.updateSlideElementText(
              slideIndex: sIdx, elementID: elementID, title: nil, bodyText: $0)
          }
        )
      )
      .textFieldStyle(.roundedBorder)
      .font(.system(size: 11))

      HStack(spacing: 6) {
        Button("X -10") {
          viewModel.moveSlideElement(slideIndex: sIdx, elementID: elementID, deltaX: -10, deltaY: 0)
        }
        .buttonStyle(.bordered).controlSize(.mini)
        Button("X +10") {
          viewModel.moveSlideElement(slideIndex: sIdx, elementID: elementID, deltaX: 10, deltaY: 0)
        }
        .buttonStyle(.bordered).controlSize(.mini)
        Button("Y -10") {
          viewModel.moveSlideElement(slideIndex: sIdx, elementID: elementID, deltaX: 0, deltaY: -10)
        }
        .buttonStyle(.bordered).controlSize(.mini)
        Button("Y +10") {
          viewModel.moveSlideElement(slideIndex: sIdx, elementID: elementID, deltaX: 0, deltaY: 10)
        }
        .buttonStyle(.bordered).controlSize(.mini)
      }

      HStack(spacing: 6) {
        Button("폭 -20") {
          viewModel.resizeSlideElement(
            slideIndex: sIdx, elementID: elementID, deltaWidth: -20, deltaHeight: 0)
        }
        .buttonStyle(.bordered).controlSize(.mini)
        Button("폭 +20") {
          viewModel.resizeSlideElement(
            slideIndex: sIdx, elementID: elementID, deltaWidth: 20, deltaHeight: 0)
        }
        .buttonStyle(.bordered).controlSize(.mini)
        Button("높이 -20") {
          viewModel.resizeSlideElement(
            slideIndex: sIdx, elementID: elementID, deltaWidth: 0, deltaHeight: -20)
        }
        .buttonStyle(.bordered).controlSize(.mini)
        Button("높이 +20") {
          viewModel.resizeSlideElement(
            slideIndex: sIdx, elementID: elementID, deltaWidth: 0, deltaHeight: 20)
        }
        .buttonStyle(.bordered).controlSize(.mini)
      }

      HStack(spacing: 6) {
        Text("회전: \(Int(elem.rotationDegrees))°")
          .font(.system(size: 10, design: .monospaced))
        Button("-15°") {
          viewModel.rotateSlideElement(
            slideIndex: sIdx, elementID: elementID, toDegrees: elem.rotationDegrees - 15)
        }
        .buttonStyle(.bordered).controlSize(.mini)
        Button("+15°") {
          viewModel.rotateSlideElement(
            slideIndex: sIdx, elementID: elementID, toDegrees: elem.rotationDegrees + 15)
        }
        .buttonStyle(.bordered).controlSize(.mini)
        Button("0° 초기화") {
          viewModel.rotateSlideElement(slideIndex: sIdx, elementID: elementID, toDegrees: 0)
        }
        .buttonStyle(.bordered).controlSize(.mini)
      }

      VStack(alignment: .leading, spacing: 4) {
        Text("애니메이션: \(elem.animation.rawValue)")
          .font(.system(size: 10))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        Button("애니메이션 전환") {
          viewModel.cycleSlideElementAnimation(slideIndex: sIdx, elementID: elementID)
        }
        .buttonStyle(.bordered).controlSize(.small)
      }

      HStack(spacing: 6) {
        Button("앞으로") {
          viewModel.moveSlideElementZOrder(
            slideIndex: sIdx, elementID: elementID, bringForward: true)
        }
        .buttonStyle(.bordered).controlSize(.small)
        Button("뒤로") {
          viewModel.moveSlideElementZOrder(
            slideIndex: sIdx, elementID: elementID, bringForward: false)
        }
        .buttonStyle(.bordered).controlSize(.small)
      }

      Text("개체 정렬 · 배분 · 복제")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 4) {
        Button("좌측 맞춤") { viewModel.alignSlideElements(.alignLeft) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("가로 중앙") { viewModel.alignSlideElements(.alignCenter) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("우측 맞춤") { viewModel.alignSlideElements(.alignRight) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("가로 배분") { viewModel.alignSlideElements(.distributeHorizontally) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
      }
      HStack(spacing: 6) {
        Button("개체 복제") { viewModel.duplicateActiveSlideElement() }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("개체 삭제") { viewModel.deleteActiveSlideElement() }
          .buttonStyle(.bordered)
          .controlSize(.mini)
      }
    }

    Divider()

    Toggle("슬라이드 정렬 격자 뷰", isOn: $viewModel.isSlideSorterViewEnabled)
      .font(.system(size: 11, weight: .semibold))

    VStack(alignment: .leading, spacing: 6) {
      Text("슬라이드 덱 관리")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 6) {
        Button("슬라이드 추가") { viewModel.addSlideToActiveDeck() }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("복제") { viewModel.duplicateActiveSlide() }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("삭제") { viewModel.deleteActiveSlide() }
          .buttonStyle(.bordered)
          .controlSize(.mini)
          .disabled(deck.slides.count <= 1)
      }
      HStack(spacing: 6) {
        Button("위로 이동") { viewModel.moveActiveSlide(offset: -1) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("아래로 이동") { viewModel.moveActiveSlide(offset: 1) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
      }
    }

    Button("스마트 스냅 정렬") { viewModel.executeRibbonCommand("slide-mod-snap") }
      .buttonStyle(.bordered).controlSize(.small)

    Button(viewModel.isFullScreenSlideshowActive ? "전체화면 슬라이드쇼 종료" : "전체화면 슬라이드쇼 시작") {
      viewModel.toggleFullScreenSlideshow()
    }
    .buttonStyle(.borderedProminent)
    .controlSize(.small)
    .tint(EasyOfficeDesignTokens.accentToken(for: .slide).swiftUIColor)
  }

  // MARK: - PDF Inspector

  @ViewBuilder
  private func pdfInspectorSection(pdf: PDFStudioDocumentModel, palette: WorkspaceThemePalette)
    -> some View
  {
    let activePageStrokes = pdf.inkStrokes.filter {
      $0.pageNumber == viewModel.selectedPDFPageNumber
    }

    Text("PDF 스튜디오 페이지·주석 제어")
      .font(.system(size: 11, weight: .bold))
      .foregroundStyle(palette.primaryText.swiftUIColor)
    Text(
      "총 페이지: \(pdf.pages.count)p · 주석: \(pdf.annotations.count)건 · 서명 획: \(pdf.inkStrokes.count)건"
    )
    .font(.system(size: 11))

    Stepper(
      "대상 페이지: \(viewModel.selectedPDFPageNumber)p", value: $viewModel.selectedPDFPageNumber,
      in: 1...max(1, pdf.pages.count)
    )
    .font(.system(size: 11, weight: .medium))

    Divider()

    VStack(alignment: .leading, spacing: 6) {
      Toggle("자유곡선 펜 그리기 모드", isOn: $viewModel.isPDFInkDrawingModeEnabled)
        .font(.system(size: 11, weight: .semibold))

      HStack(spacing: 6) {
        Text("펜 색상:")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        ForEach(["#5C6BC0", "#D32F2F", "#1E293B"], id: \.self) { hex in
          Button(hex) { viewModel.pdfInkStrokeColorHex = hex }
            .buttonStyle(.bordered)
            .controlSize(.mini)
        }
      }

      Button("샘플 벡터 서명 획 추가") {
        viewModel.addInkStrokeToActivePDFPage(points: [
          PDFInkPoint(x: 90, y: 140),
          PDFInkPoint(x: 145, y: 168),
          PDFInkPoint(x: 210, y: 136),
          PDFInkPoint(x: 265, y: 162),
        ])
      }
      .buttonStyle(.bordered).controlSize(.small)

      Button("현재 페이지 자유곡선 지우기 (\(activePageStrokes.count)건)") {
        viewModel.clearInkStrokesOnActivePDFPage()
      }
      .buttonStyle(.bordered).controlSize(.small)
      .disabled(activePageStrokes.isEmpty)
    }

    Divider()

    VStack(alignment: .leading, spacing: 6) {
      Text("민감 정보 리댁션 (영구 마스킹)")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      TextField("마스킹할 구문 입력", text: $viewModel.pdfRedactionPhraseDraft)
        .textFieldStyle(.roundedBorder)
        .font(.system(size: 11))
      Button("민감 구문 영구 리댁션") {
        viewModel.redactSensitiveTextOnActivePDF()
      }
      .buttonStyle(.bordered)
      .controlSize(.small)

      Divider()

      Text("양식 필드 (AcroForm)")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 6) {
        TextField("필드명", text: $viewModel.pdfFormFieldNameDraft)
          .textFieldStyle(.roundedBorder)
          .font(.system(size: 11))
        TextField("기본값", text: $viewModel.pdfFormFieldValueDraft)
          .textFieldStyle(.roundedBorder)
          .font(.system(size: 11))
      }
      HStack(spacing: 6) {
        Button("텍스트 필드 추가") {
          viewModel.addFormFieldToActivePDF(isCheckbox: false)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
        Button("체크박스 추가") {
          viewModel.addFormFieldToActivePDF(isCheckbox: true)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
      }

      Divider()

      Text("전자 결재 도장 프리셋 (Official Stamp)")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 4) {
        ForEach(PDFStampPreset.allCases, id: \.self) { preset in
          Button(preset.rawValue) {
            viewModel.addStampToActivePDFPage(preset: preset)
          }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        }
      }

      Text("책갈피 목차 관리 (Bookmark Outline)")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 6) {
        TextField("책갈피 제목", text: $viewModel.bookmarkTitleDraft)
          .textFieldStyle(.roundedBorder)
          .font(.system(size: 11))
        Button("책갈피 추가") {
          viewModel.addBookmarkToActivePDF()
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
      }
    }

    Divider()

    VStack(alignment: .leading, spacing: 6) {
      Button("90° 회전") { viewModel.rotateActivePDFPage(clockwiseDegrees: 90) }
        .buttonStyle(.bordered).controlSize(.small)

      HStack(spacing: 6) {
        Button("빈 페이지 추가") {
          viewModel.insertBlankPDFPage(afterPage: viewModel.selectedPDFPageNumber)
        }
        .buttonStyle(.bordered).controlSize(.small)
        Button("선택 페이지 삭제") { viewModel.deleteActivePDFPage() }
          .buttonStyle(.bordered).controlSize(.small)
          .disabled(pdf.pages.count <= 1)
      }

      HStack(spacing: 6) {
        Button("페이지 위로") { viewModel.moveActivePDFPage(offset: -1) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        Button("페이지 아래로") { viewModel.moveActivePDFPage(offset: 1) }
          .buttonStyle(.bordered)
          .controlSize(.mini)
      }

      Button("열린 PDF 탭 병합") { viewModel.mergeActivePDFWithAnotherOpenTab() }
        .buttonStyle(.bordered)
        .controlSize(.small)

      Button("하이라이트 주석 추가") { viewModel.addPDFAnnotation(kind: .highlight, content: "하이라이트 주석") }
        .buttonStyle(.bordered).controlSize(.small)

      Button("페이지 분할 (\(viewModel.selectedPDFPageNumber)p 추출)") {
        viewModel.executeRibbonCommand("pdf-mod-splitmerge")
      }
      .buttonStyle(.bordered).controlSize(.small)

      Button("문서(.docx)로 변환") { viewModel.executeRibbonCommand("pdf-mod-tooffice") }
        .buttonStyle(.borderedProminent)
        .controlSize(.small)
        .tint(EasyOfficeDesignTokens.accentToken(for: .pdf).swiftUIColor)
    }
  }

  // MARK: - Mutation Helpers

  private func mutateDocument(_ transform: (inout OfficeDocumentModel) -> Void) {
    guard let idx = viewModel.openTabs.firstIndex(where: { $0.id == tab.id }),
      case .document(var doc) = viewModel.openTabs[idx].payload
    else { return }
    viewModel.recordUndoSnapshot(
      for: tab.id, coalescingTargetKey: "inspector-doc-\(tab.id.uuidString)")
    transform(&doc)
    viewModel.openTabs[idx].payload = .document(doc)
    viewModel.openTabs[idx].isModified = true
  }

  private func mutateSlideDeck(_ transform: (inout PresentationDeckModel) -> Void) {
    guard let idx = viewModel.openTabs.firstIndex(where: { $0.id == tab.id }),
      case .slide(var deck) = viewModel.openTabs[idx].payload
    else { return }
    viewModel.recordUndoSnapshot(
      for: tab.id, coalescingTargetKey: "inspector-slide-\(tab.id.uuidString)")
    transform(&deck)
    viewModel.openTabs[idx].payload = .slide(deck)
    viewModel.openTabs[idx].isModified = true
  }
}
