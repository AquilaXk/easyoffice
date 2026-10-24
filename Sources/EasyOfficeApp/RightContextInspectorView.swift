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

    AdaptiveScrollContainer(
      axes: .vertical,
      showsIndicators: false,
      isSnapshotMode: viewModel.isCanvasSnapshotMode
    ) {
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
          LavenderTextFieldPill(
            placeholder: "작성자 이름",
            text: $viewModel.authorName,
            width: nil,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )
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

    HStack(spacing: 6) {
      LavenderMenuPill(
        title: "용지: \(doc.paperSize.rawValue)",
        systemImage: "doc.plaintext",
        palette: palette
      ) {
        ForEach(PaperSizePreset.allCases, id: \.self) { preset in
          Button(preset.rawValue) {
            mutateDocument { d in
              if d.sections.isEmpty { d.sections = [DocumentSectionLayout(paperSize: preset)] }
              d.paperSize = preset
              d.sections[0].paperSize = preset
            }
          }
        }
      }

      LavenderMenuPill(
        title: "방향: \(currentOrientation.rawValue)",
        systemImage: "rectangle.portrait.rotate",
        palette: palette
      ) {
        ForEach(PageOrientation.allCases, id: \.self) { orient in
          Button(orient.rawValue) {
            mutateDocument { d in
              if d.sections.isEmpty { d.sections = [DocumentSectionLayout(paperSize: d.paperSize)] }
              d.sections[0].orientation = orient
            }
          }
        }
      }
    }

    LavenderStepperPill(
      label: "다단 열 수: \(currentColumns)단",
      value: Binding(
        get: { currentColumns },
        set: { newCols in
          mutateDocument { d in
            if d.sections.isEmpty { d.sections = [DocumentSectionLayout(paperSize: d.paperSize)] }
            d.sections[0].columnCount = max(1, min(3, newCols))
          }
        }
      ),
      range: 1...3,
      palette: palette
    )

    LavenderTogglePill(
      title: "변경 내용 추적",
      systemImage: "checkmark.bubble",
      isOn: Binding(
        get: { doc.trackChangesEnabled },
        set: { enabled in mutateDocument { $0.trackChangesEnabled = enabled } }
      ),
      palette: palette
    )

    VStack(alignment: .leading, spacing: 4) {
      Text("머리말 / 꼬리말")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      LavenderTextFieldPill(
        placeholder: "머리말 텍스트",
        text: Binding(
          get: { doc.headerText },
          set: { text in mutateDocument { $0.headerText = text } }
        ),
        width: nil,
        palette: palette,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      )

      LavenderTextFieldPill(
        placeholder: "꼬리말 텍스트",
        text: Binding(
          get: { doc.footerText },
          set: { text in mutateDocument { $0.footerText = text } }
        ),
        width: nil,
        palette: palette,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      )
    }

    Divider()

    VStack(alignment: .leading, spacing: 6) {
      Text("수식 블록 삽입")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 6) {
        LavenderTextFieldPill(
          placeholder: "LaTeX / 수식 입력",
          text: $viewModel.equationDraftText,
          width: nil,
          isMonospaced: true,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
        Button("수식 삽입") { viewModel.insertEquationIntoActiveDocument() }
          .buttonStyle(.bordered)
          .controlSize(.small)
      }

      Text("워터마크 적용/해제")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      LavenderTextFieldPill(
        placeholder: "워터마크 문구",
        text: $viewModel.watermarkDraftText,
        width: nil,
        palette: palette,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      )
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
        LavenderTextFieldPill(
          placeholder: "하이퍼링크 URL",
          text: $viewModel.inlineLinkURLDraft,
          width: nil,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
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
    let activeParagraph: ParagraphBlock? = {
      guard !doc.paragraphs.isEmpty else { return nil }
      let pIdx = min(max(0, viewModel.selectedParagraphIndex), doc.paragraphs.count - 1)
      return doc.paragraphs[pIdx]
    }()
    let activeListKind = activeParagraph?.listKind ?? .koreanOfficial
    let currentLetterSpacing = activeParagraph?.letterSpacingPercent ?? 0.0
    let currentCharScale = activeParagraph?.charWidthScalePercent ?? 100.0

    VStack(alignment: .leading, spacing: 6) {
      Text("자간 · 장평 정밀 조판")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      Text(
        "현재 자간: \(Int(currentLetterSpacing))% · 장평: \(Int(currentCharScale))%"
      )
      .font(.system(size: 10, design: .monospaced))
      .foregroundStyle(palette.accentBadgeText.swiftUIColor)

      HStack(spacing: 4) {
        Button("자간 -5%") {
          viewModel.adjustActiveParagraphTypography(letterSpacingDelta: -5.0)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)

        Button("자간 +5%") {
          viewModel.adjustActiveParagraphTypography(letterSpacingDelta: 5.0)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)

        Button("장평 90%") {
          viewModel.adjustActiveParagraphTypography(charScaleDelta: 90.0 - currentCharScale)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)

        Button("장평 105%") {
          viewModel.adjustActiveParagraphTypography(charScaleDelta: 105.0 - currentCharScale)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)

        Button("조판 초기화") {
          viewModel.adjustActiveParagraphTypography(reset: true)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
      }

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
        LavenderTextFieldPill(
          placeholder: "필드 키",
          text: $viewModel.mailMergeFieldKeyDraft,
          width: 72,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
        LavenderTextFieldPill(
          placeholder: "치환 값",
          text: $viewModel.mailMergeFieldValueDraft,
          width: 72,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
        Button("필드 치환") { viewModel.applyMailMergeFieldReplacement() }
          .buttonStyle(.bordered)
          .controlSize(.small)
      }

      LavenderTogglePill(
        title: "200자 원고지 격자 뷰",
        systemImage: "squareshape.split.3x3",
        isOn: $viewModel.isManuscriptGridModeEnabled,
        palette: palette
      )

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

    HStack(spacing: 6) {
      LavenderMenuPill(
        title: "형식: \(currentStyle.numberFormat.rawValue)",
        systemImage: "number",
        palette: palette
      ) {
        ForEach(CellNumberFormat.allCases, id: \.self) { fmt in
          Button(fmt.rawValue) {
            viewModel.setSelectedSheetCellNumberFormat(fmt)
          }
        }
      }

      LavenderMenuPill(
        title: "정렬: \(currentStyle.alignment.rawValue)",
        systemImage: "text.alignleft",
        palette: palette
      ) {
        ForEach(CellHorizontalAlignment.allCases, id: \.self) { align in
          Button(align.rawValue) {
            viewModel.setSelectedSheetCellAlignment(align)
          }
        }
      }
    }

    VStack(alignment: .leading, spacing: 6) {
      Text(
        "줄바꿈: \(currentStyle.wrapText ? "켜짐" : "꺼짐") · 소수점: \(currentStyle.decimalPlaces ?? 2)자리"
      )
      .font(.system(size: 10, design: .monospaced))
      .foregroundStyle(palette.secondaryText.swiftUIColor)

      HStack(spacing: 6) {
        Button("텍스트 줄바꿈 토글") {
          viewModel.toggleWrapTextOnSelectedSheetCell()
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)

        Button("소수점 -1") {
          viewModel.adjustSelectedSheetCellDecimalPlaces(delta: -1)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)

        Button("소수점 +1") {
          viewModel.adjustSelectedSheetCellDecimalPlaces(delta: 1)
        }
        .buttonStyle(.bordered)
        .controlSize(.mini)
      }
    }

    VStack(alignment: .leading, spacing: 4) {
      Text("선택 셀 메모 (\(viewModel.selectedSheetCell))")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      LavenderTextFieldPill(
        placeholder: "셀 메모 입력",
        text: Binding(
          get: { sheet.sheetComments[selectedAddr.packedCoordinate] ?? "" },
          set: { viewModel.setSelectedSheetCellComment($0) }
        ),
        width: nil,
        palette: palette,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      )
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

      LavenderStepperPill(
        label: "자동 필터 열: \(CellAddress.columnLabel(for: max(0, viewModel.sheetAutoFilterColumn)))열",
        value: $viewModel.sheetAutoFilterColumn,
        range: 0...15,
        palette: palette
      )
      LavenderTextFieldPill(
        placeholder: "열 자동 필터 검색어",
        text: $viewModel.sheetAutoFilterQuery,
        width: nil,
        palette: palette,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      )

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
        LavenderTextFieldPill(
          placeholder: "이름",
          text: $viewModel.namedRangeNameDraft,
          width: 78,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
        LavenderTextFieldPill(
          placeholder: "범위 (예: B2:B4)",
          text: $viewModel.namedRangeTargetDraft,
          width: 96,
          isMonospaced: true,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
      }
      Button("이름 정의 등록") { viewModel.defineNamedRangeOnActiveSheet() }
        .buttonStyle(.bordered)
        .controlSize(.small)

      Divider()

      Text("목표값 찾기 (Goal Seek)")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 4) {
        LavenderTextFieldPill(
          placeholder: "수식 셀",
          text: $viewModel.goalSeekTargetCellDraft,
          width: 56,
          isMonospaced: true,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
        LavenderTextFieldPill(
          placeholder: "목표값",
          text: $viewModel.goalSeekTargetValueDraft,
          width: 56,
          isMonospaced: true,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
        LavenderTextFieldPill(
          placeholder: "변경 셀",
          text: $viewModel.goalSeekVariableCellDraft,
          width: 56,
          isMonospaced: true,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
      }
      Button("목표값 찾기 실행") { viewModel.runGoalSeekOnActiveSheet() }
        .buttonStyle(.bordered)
        .controlSize(.small)

      Divider()

      Button("통화/백분율/소수 서식 전환") { viewModel.executeRibbonCommand("sheet-home-style") }
        .buttonStyle(.bordered).controlSize(.small)

      LavenderTogglePill(
        title: "머리글 행 고정",
        systemImage: "pin.fill",
        isOn: $viewModel.isHeaderRowFrozen,
        palette: palette
      )

      LavenderTogglePill(
        title: "첫 열(A열) 틀 고정",
        systemImage: "pin.fill",
        isOn: $viewModel.isFirstColumnFrozen,
        palette: palette
      )

      LavenderTogglePill(
        title: "셀 내 데이터 막대 표시",
        systemImage: "chart.bar.fill",
        isOn: $viewModel.isDataBarsEnabled,
        palette: palette
      )

      LavenderMenuPill(
        title: "피벗 집계: \(viewModel.selectedPivotAggregation.rawValue)",
        systemImage: "function",
        palette: palette
      ) {
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
          Button(fn.rawValue) {
            viewModel.selectedPivotAggregation = fn
          }
        }
      }

      Text("데이터 정제 (중복 제거 · 텍스트 나누기 · 다중 정렬)")
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
      Button("다중 기준 정렬 (A열↑ · B열↓)") {
        viewModel.sortActiveSheetBySecondaryColumn(
          primaryColumn: 0,
          ascending: true,
          secondaryColumn: 1,
          secondaryAscending: false
        )
      }
      .buttonStyle(.bordered)
      .controlSize(.small)

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

    HStack(spacing: 6) {
      LavenderMenuPill(
        title: "테마: \(deck.theme.rawValue)",
        systemImage: "paintpalette",
        palette: palette
      ) {
        ForEach(SlideDeckTheme.allCases, id: \.self) { theme in
          Button(theme.rawValue) {
            viewModel.applyThemeToActiveDeck(theme)
          }
        }
      }

      LavenderMenuPill(
        title: "비율: \(deck.aspectRatio.rawValue)",
        systemImage: "aspectratio",
        palette: palette
      ) {
        ForEach(SlideAspectRatio.allCases, id: \.self) { ratio in
          Button(ratio.rawValue) {
            mutateSlideDeck { $0.aspectRatio = ratio }
          }
        }
      }
    }

    if let slide = activeSlide {
      HStack(spacing: 6) {
        LavenderMenuPill(
          title: "레이아웃: \(slide.masterLayout.rawValue)",
          systemImage: "rectangle.3.group",
          palette: palette
        ) {
          ForEach(SlideMasterLayout.allCases, id: \.self) { layout in
            Button(layout.rawValue) {
              mutateSlideDeck { d in
                if d.slides.indices.contains(sIdx) { d.slides[sIdx].masterLayout = layout }
              }
            }
          }
        }

        LavenderMenuPill(
          title: "전환: \(slide.transition.rawValue)",
          systemImage: "sparkles.rectangle.stack",
          palette: palette
        ) {
          ForEach(SlideTransitionKind.allCases, id: \.self) { trans in
            Button(trans.rawValue) {
              mutateSlideDeck { d in
                if d.slides.indices.contains(sIdx) { d.slides[sIdx].transition = trans }
              }
            }
          }
        }
      }

      VStack(alignment: .leading, spacing: 4) {
        Text(
          "전환 시간: \(String(format: "%.1f", slide.transitionDurationSeconds))초 · 자동 전환: \(slide.autoAdvanceSeconds.map { "\($0)초" } ?? "수동")"
        )
        .font(.system(size: 10, weight: .semibold, design: .monospaced))
        .foregroundStyle(palette.secondaryText.swiftUIColor)

        HStack(spacing: 4) {
          ForEach([0.4, 0.8, 1.5, 2.5], id: \.self) { sec in
            Button("\(String(format: "%.1f", sec))초") {
              viewModel.setActiveSlideTransitionDuration(sec)
            }
            .buttonStyle(.bordered)
            .controlSize(.mini)
          }
        }

        HStack(spacing: 4) {
          Button("수동") {
            viewModel.setActiveSlideAutoAdvance(seconds: nil)
          }
          .buttonStyle(.bordered)
          .controlSize(.mini)

          Button("5초") {
            viewModel.setActiveSlideAutoAdvance(seconds: 5)
          }
          .buttonStyle(.bordered)
          .controlSize(.mini)

          Button("10초") {
            viewModel.setActiveSlideAutoAdvance(seconds: 10)
          }
          .buttonStyle(.bordered)
          .controlSize(.mini)
        }
      }

      VStack(alignment: .leading, spacing: 4) {
        Text("발표자 노트")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        LavenderTextFieldPill(
          placeholder: "발표자 노트 입력",
          text: Binding(
            get: { slide.speakerNotes },
            set: { viewModel.updateSlideSpeakerNotes(slideIndex: sIdx, notes: $0) }
          ),
          width: nil,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
      }
    }

    Divider()

    if let elem = activeElement, let elementID = targetElementID {
      Text("선택 개체 편집 (\(Int(elem.x)), \(Int(elem.y)) · \(Int(elem.width))×\(Int(elem.height)))")
        .font(.system(size: 10, weight: .semibold, design: .monospaced))
        .foregroundStyle(palette.secondaryText.swiftUIColor)

      LavenderTextFieldPill(
        placeholder: "개체 제목",
        text: Binding(
          get: { elem.title },
          set: {
            viewModel.updateSlideElementText(
              slideIndex: sIdx, elementID: elementID, title: $0, bodyText: nil)
          }
        ),
        width: nil,
        palette: palette,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      )

      LavenderTextFieldPill(
        placeholder: "개체 본문",
        text: Binding(
          get: { elem.bodyText },
          set: {
            viewModel.updateSlideElementText(
              slideIndex: sIdx, elementID: elementID, title: nil, bodyText: $0)
          }
        ),
        width: nil,
        palette: palette,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      )

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

    LavenderTogglePill(
      title: "슬라이드 정렬 격자 뷰",
      systemImage: "square.grid.3x2.fill",
      isOn: $viewModel.isSlideSorterViewEnabled,
      palette: palette
    )

    LavenderTogglePill(
      title: "스테이지 격자 안내선",
      systemImage: "grid",
      isOn: Binding(
        get: { viewModel.isSlideGridlinesVisible },
        set: { _ in viewModel.toggleSlideGridlines() }
      ),
      palette: palette
    )

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

    LavenderStepperPill(
      label: "대상 페이지: \(viewModel.selectedPDFPageNumber)p",
      value: $viewModel.selectedPDFPageNumber,
      range: 1...max(1, pdf.pages.count),
      palette: palette
    )

    Divider()

    VStack(alignment: .leading, spacing: 6) {
      LavenderTogglePill(
        title: "자유곡선 펜 그리기 모드",
        systemImage: "scribble.variable",
        isOn: $viewModel.isPDFInkDrawingModeEnabled,
        palette: palette
      )

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
      Text("PDF 워터마크 및 보안 상태 점검")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)

      LavenderTextFieldPill(
        placeholder: "PDF 워터마크 문구",
        text: $viewModel.pdfWatermarkDraft,
        width: nil,
        palette: palette,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      )

      HStack(spacing: 6) {
        Button("워터마크 적용") {
          viewModel.applyWatermarkOnActivePDF()
        }
        .buttonStyle(.bordered)
        .controlSize(.small)

        Button("워터마크 해제") {
          viewModel.applyWatermarkOnActivePDF(text: "")
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
      }

      Text(pdf.securityInspectionSummary)
        .font(.system(size: 10, weight: .medium))
        .foregroundStyle(palette.accentBadgeText.swiftUIColor)
        .padding(.horizontal, 6)
        .padding(.vertical, 3)
        .background(palette.selectedSurface.swiftUIColor)
        .clipShape(RoundedRectangle(cornerRadius: 4))

      Divider()

      Text("민감 정보 리댁션 (영구 마스킹)")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      LavenderTextFieldPill(
        placeholder: "마스킹할 구문 입력",
        text: $viewModel.pdfRedactionPhraseDraft,
        width: nil,
        palette: palette,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      )
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
        LavenderTextFieldPill(
          placeholder: "필드명",
          text: $viewModel.pdfFormFieldNameDraft,
          width: 78,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
        LavenderTextFieldPill(
          placeholder: "기본값",
          text: $viewModel.pdfFormFieldValueDraft,
          width: 78,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
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
        LavenderTextFieldPill(
          placeholder: "책갈피 제목",
          text: $viewModel.bookmarkTitleDraft,
          width: nil,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
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
