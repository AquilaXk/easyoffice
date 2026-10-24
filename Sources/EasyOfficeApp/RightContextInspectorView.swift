import EasyOfficeCore
import EasyOfficeDesignSystem
import Observation
import SwiftUI

typealias RightContextualInspectorView = RightContextInspectorView

struct RightContextInspectorView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel
  let tab: WorkspaceTabItem

  @ViewBuilder
  private func inspectorButton(
    _ title: String,
    isProminent: Bool = false,
    palette: WorkspaceThemePalette,
    action: @escaping () -> Void
  ) -> some View {
    Button(action: action) {
      Text(title)
        .font(.system(size: 10.5, weight: isProminent ? .semibold : .medium))
        .foregroundStyle(
          isProminent
            ? EasyOfficeDesignTokens.brand600.swiftUIColor
            : palette.primaryText.swiftUIColor
        )
    }
    .buttonStyle(.plain)
  }

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

        Text(tab.fileName)
          .font(.system(size: 11, weight: .medium))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
          .lineLimit(1)

        HStack(spacing: 6) {
          Text("작성자")
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
          FlatUnderlineTextField(
            placeholder: "작성자 이름",
            text: $viewModel.authorName,
            width: nil,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )
        }

        HStack(spacing: 0) {
          ForEach(InspectorSubTab.allCases) { subTab in
            let isSelected = viewModel.activeInspectorSubTab == subTab
            Button {
              viewModel.activeInspectorSubTab = subTab
            } label: {
              VStack(spacing: 4) {
                Text(subTab.rawValue)
                  .font(.system(size: 11, weight: isSelected ? .semibold : .regular))
                  .foregroundStyle(
                    isSelected
                      ? palette.primaryText.swiftUIColor
                      : palette.secondaryText.swiftUIColor
                  )
                  .frame(maxWidth: .infinity)
                  .padding(.vertical, 4)

                Rectangle()
                  .fill(
                    isSelected
                      ? EasyOfficeDesignTokens.brand600.swiftUIColor
                      : Color.clear
                  )
                  .frame(height: 2)
              }
            }
            .buttonStyle(.plain)
          }
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

        Spacer(minLength: 0)
      }
      .padding(12)
    }
    .background(palette.cardSurface.swiftUIColor)
  }

  @ViewBuilder
  private func documentInspectorSection(doc: OfficeDocumentModel, palette: WorkspaceThemePalette)
    -> some View
  {
    let stats = doc.computeStatistics()
    let modulePrefix = tab.moduleKind == .hwp ? "hwp" : "doc"
    let currentOrientation = doc.sections.first?.orientation ?? .portrait
    let currentColumns = doc.sections.first?.columnCount ?? 1
    let shortPaperLabel = doc.paperSize == .a4 ? "A4" : "Letter"
    let shortOrientLabel = currentOrientation == .portrait ? "세로" : "가로"

    if viewModel.activeInspectorSubTab == .propertiesAndLayout {
      Text("문서 통계")
        .font(.system(size: 11, weight: .bold))
        .foregroundStyle(palette.primaryText.swiftUIColor)
      Text(
        "문단 \(stats.paragraphCount) · 표 \(stats.tableCount) · 이미지 \(doc.images.count) · 수식 \(doc.equations.count)"
      )
      .font(.system(size: 11))
      Text("단어 \(stats.wordCount) · 글자(공백 포함) \(stats.characterCountWithSpaces)")
        .font(.system(size: 11))
      Text("200자 원고지 \(stats.manuscriptPages200)매")
        .font(.system(size: 11, weight: .semibold))
        .foregroundStyle(palette.accentBadgeText.swiftUIColor)
      Text("각주 \(doc.footnotes.count)건 · 변경 추적 \(doc.revisions.count)건")
        .font(.system(size: 11))
        .foregroundStyle(palette.secondaryText.swiftUIColor)

      Divider()

      Text("레이아웃 및 구역")
        .font(.system(size: 11, weight: .bold))
        .foregroundStyle(palette.primaryText.swiftUIColor)

      HStack(spacing: 12) {
        FlatToolbarMenu(
          title: "용지: \(shortPaperLabel)",
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

        FlatToolbarMenu(
          title: "방향: \(shortOrientLabel)",
          systemImage: "rectangle.portrait.rotate",
          palette: palette
        ) {
          ForEach(PageOrientation.allCases, id: \.self) { orient in
            Button(orient.rawValue) {
              mutateDocument { d in
                if d.sections.isEmpty {
                  d.sections = [DocumentSectionLayout(paperSize: d.paperSize)]
                }
                d.sections[0].orientation = orient
              }
            }
          }
        }
      }

      FlatStepperControl(
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

      FlatToggleControl(
        title: "변경 내용 추적",
        systemImage: "checkmark.bubble",
        isOn: Binding(
          get: { doc.trackChangesEnabled },
          set: { enabled in mutateDocument { $0.trackChangesEnabled = enabled } }
        ),
        palette: palette
      )

      FlatToggleControl(
        title: "200자 원고지 격자 뷰",
        systemImage: "squareshape.split.3x3",
        isOn: $viewModel.isManuscriptGridModeEnabled,
        palette: palette
      )

      VStack(alignment: .leading, spacing: 6) {
        Text("머리말 / 꼬리말")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        FlatUnderlineTextField(
          placeholder: "머리말 텍스트",
          text: Binding(
            get: { doc.headerText },
            set: { text in mutateDocument { $0.headerText = text } }
          ),
          width: nil,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )

        FlatUnderlineTextField(
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

      VStack(alignment: .leading, spacing: 8) {
        HStack(spacing: 12) {
          inspectorButton("변경 추적 전환", palette: palette) {
            viewModel.executeRibbonCommand("\(modulePrefix)-review-track")
          }
          inspectorButton("단 나누기 전환", palette: palette) {
            viewModel.executeRibbonCommand("\(modulePrefix)-layout-columns")
          }
        }
        inspectorButton("용지 방향 전환", palette: palette) {
          viewModel.executeRibbonCommand("\(modulePrefix)-layout-orient")
        }
        inspectorButton("개인정보·메타데이터 정리", isProminent: true, palette: palette) {
          viewModel.executeRibbonCommand("\(modulePrefix)-review-security")
          viewModel.scrubActiveTabMetadata()
        }
      }
    } else {
      VStack(alignment: .leading, spacing: 8) {
        Text("수식 블록 삽입")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        HStack(spacing: 8) {
          FlatUnderlineTextField(
            placeholder: "LaTeX / 수식 입력",
            text: $viewModel.equationDraftText,
            width: nil,
            isMonospaced: true,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )
          inspectorButton("삽입", isProminent: true, palette: palette) {
            viewModel.insertEquationIntoActiveDocument()
          }
        }

        Text("워터마크")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        HStack(spacing: 8) {
          FlatUnderlineTextField(
            placeholder: "워터마크 문구",
            text: $viewModel.watermarkDraftText,
            width: nil,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )
          inspectorButton("적용", isProminent: true, palette: palette) {
            viewModel.setDocumentWatermark(viewModel.watermarkDraftText)
          }
          inspectorButton("해제", palette: palette) {
            viewModel.setDocumentWatermark("")
          }
        }

        if let firstImage = doc.images.first {
          Text("이미지 배치: \(firstImage.wrapMode.rawValue)")
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
          inspectorButton("이미지 텍스트 배치 전환", palette: palette) {
            viewModel.cycleImageWrapMode(imageID: firstImage.id)
          }
        }

        Text("인라인 강조 · 취소선 · 하이퍼링크")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        HStack(spacing: 10) {
          inspectorButton("형광펜 강조", palette: palette) {
            viewModel.toggleHighlightOnActiveParagraph()
          }
          inspectorButton("취소선", palette: palette) {
            viewModel.toggleStrikethroughOnActiveParagraph()
          }
          inspectorButton("미주 삽입", palette: palette) {
            viewModel.insertEndnoteIntoActiveDocument()
          }
        }
        HStack(spacing: 8) {
          FlatUnderlineTextField(
            placeholder: "하이퍼링크 URL",
            text: $viewModel.inlineLinkURLDraft,
            width: nil,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )
          inspectorButton("링크 삽입", isProminent: true, palette: palette) {
            viewModel.insertHyperlinkIntoActiveParagraph()
          }
        }

        Text("문단 들여쓰기 (첫 줄 / 좌측)")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        HStack(spacing: 10) {
          inspectorButton("첫 줄 +10pt", palette: palette) {
            viewModel.adjustActiveParagraphIndent(firstLineDeltaPt: 10, leftIndentDeltaPt: 0)
          }
          inspectorButton("좌측 +10pt", palette: palette) {
            viewModel.adjustActiveParagraphIndent(firstLineDeltaPt: 0, leftIndentDeltaPt: 10)
          }
          inspectorButton("초기화", palette: palette) {
            viewModel.adjustActiveParagraphIndent(firstLineDeltaPt: -144, leftIndentDeltaPt: -144)
          }
        }
      }

      Divider()

      documentAdvancedControlsSubSection(doc: doc, palette: palette)
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

    VStack(alignment: .leading, spacing: 8) {
      Text("자간 · 장평 (\(Int(currentLetterSpacing))% / \(Int(currentCharScale))%)")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)

      HStack(spacing: 8) {
        inspectorButton("자간-5%", palette: palette) {
          viewModel.adjustActiveParagraphTypography(letterSpacingDelta: -5.0)
        }
        inspectorButton("자간+5%", palette: palette) {
          viewModel.adjustActiveParagraphTypography(letterSpacingDelta: 5.0)
        }
        inspectorButton("장평90%", palette: palette) {
          viewModel.adjustActiveParagraphTypography(charScaleDelta: 90.0 - currentCharScale)
        }
        inspectorButton("장평105%", palette: palette) {
          viewModel.adjustActiveParagraphTypography(charScaleDelta: 105.0 - currentCharScale)
        }
        inspectorButton("초기화", palette: palette) {
          viewModel.adjustActiveParagraphTypography(reset: true)
        }
      }

      Text("문단 정렬 · 줄간격 · 다단계 목록")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 10) {
        inspectorButton("좌측", palette: palette) { viewModel.setActiveParagraphAlignment(.leading) }
        inspectorButton("가운데", palette: palette) { viewModel.setActiveParagraphAlignment(.center) }
        inspectorButton("우측", palette: palette) { viewModel.setActiveParagraphAlignment(.trailing) }
        inspectorButton("양쪽", palette: palette) { viewModel.setActiveParagraphAlignment(.justified) }
      }
      HStack(spacing: 8) {
        ForEach([100.0, 115.0, 150.0, 160.0, 200.0], id: \.self) { pct in
          inspectorButton("\(Int(pct))%", palette: palette) {
            viewModel.setActiveParagraphLineSpacing(pct)
          }
        }
      }
      HStack(spacing: 8) {
        inspectorButton("글머리", palette: palette) { viewModel.setActiveParagraphListKind(.bullet) }
        inspectorButton("번호", palette: palette) { viewModel.setActiveParagraphListKind(.numbered) }
        inspectorButton("공문서", palette: palette) {
          viewModel.setActiveParagraphListKind(.koreanOfficial)
        }
        inspectorButton("해제", palette: palette) { viewModel.setActiveParagraphListKind(.none) }
        inspectorButton("수준+1", palette: palette) {
          viewModel.setActiveParagraphListKind(
            activeListKind == .none ? .koreanOfficial : activeListKind,
            levelDelta: 1
          )
        }
      }
    }

    Divider()

    VStack(alignment: .leading, spacing: 8) {
      if !doc.tables.isEmpty {
        Text("표 행·열 및 셀 편집")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        HStack(spacing: 8) {
          inspectorButton("+행", palette: palette) { viewModel.insertRowInDocumentTable() }
          inspectorButton("-행", palette: palette) { viewModel.deleteRowInDocumentTable() }
          inspectorButton("+열", palette: palette) { viewModel.insertColumnInDocumentTable() }
          inspectorButton("-열", palette: palette) { viewModel.deleteColumnInDocumentTable() }
          inspectorButton("병합", palette: palette) { viewModel.mergeActiveDocumentTableCells() }
          inspectorButton("분할", palette: palette) { viewModel.splitActiveDocumentTableCell() }
        }
      }

      Text("메일 머지 (누름틀 치환)")
        .font(.system(size: 10, weight: .semibold))
        .foregroundStyle(palette.secondaryText.swiftUIColor)
      HStack(spacing: 6) {
        FlatUnderlineTextField(
          placeholder: "필드 키",
          text: $viewModel.mailMergeFieldKeyDraft,
          width: 68,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
        FlatUnderlineTextField(
          placeholder: "치환 값",
          text: $viewModel.mailMergeFieldValueDraft,
          width: 68,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
        inspectorButton("치환", isProminent: true, palette: palette) {
          viewModel.applyMailMergeFieldReplacement()
        }
      }
    }
  }

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

    if viewModel.activeInspectorSubTab == .propertiesAndLayout {
      Text("시트 수식 및 셀 서식")
        .font(.system(size: 11, weight: .bold))
        .foregroundStyle(palette.primaryText.swiftUIColor)
      Text("선택 셀 \(viewModel.selectedSheetCell) · 범위 \(viewModel.selectedSheetRange)")
        .font(.system(size: 11, design: .monospaced))
      Text("수식 \(sheet.rawValue(at: viewModel.selectedSheetCell))")
        .font(.system(size: 11, design: .monospaced))
      Text("결과 \(formattedResult)")
        .font(.system(size: 11, weight: .semibold))
        .foregroundStyle(palette.accentBadgeText.swiftUIColor)
      Text(
        "SUM \(String(format: "%.1f", rangeSummary.sum)) · AVG \(String(format: "%.1f", rangeSummary.average)) · COUNT \(rangeSummary.numericCount)/\(rangeSummary.cellCount)"
      )
      .font(.system(size: 10, design: .monospaced))
      .foregroundStyle(palette.secondaryText.swiftUIColor)
      Text(
        "MIN \(rangeSummary.minimum.map { String(format: "%.1f", $0) } ?? "-") · MAX \(rangeSummary.maximum.map { String(format: "%.1f", $0) } ?? "-")"
      )
      .font(.system(size: 10, design: .monospaced))
      .foregroundStyle(palette.secondaryText.swiftUIColor)

      Divider()

      HStack(spacing: 12) {
        FlatToolbarMenu(
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

        FlatToolbarMenu(
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
          "줄바꿈 \(currentStyle.wrapText ? "켜짐" : "꺼짐") · 소수점 \(currentStyle.decimalPlaces ?? 2)자리"
        )
        .font(.system(size: 10, design: .monospaced))
        .foregroundStyle(palette.secondaryText.swiftUIColor)

        HStack(spacing: 10) {
          inspectorButton("줄바꿈 전환", palette: palette) {
            viewModel.toggleWrapTextOnSelectedSheetCell()
          }
          inspectorButton("소수점 -1", palette: palette) {
            viewModel.adjustSelectedSheetCellDecimalPlaces(delta: -1)
          }
          inspectorButton("소수점 +1", palette: palette) {
            viewModel.adjustSelectedSheetCellDecimalPlaces(delta: 1)
          }
        }
      }

      Divider()

      VStack(alignment: .leading, spacing: 8) {
        HStack(spacing: 8) {
          Text("열 폭 (\(Int(colWidth))pt)")
            .font(.system(size: 10, design: .monospaced))
          inspectorButton("-20pt", palette: palette) {
            viewModel.updateActiveSheetColumnWidth(column: selectedAddr.column, width: colWidth - 20)
          }
          inspectorButton("+20pt", palette: palette) {
            viewModel.updateActiveSheetColumnWidth(column: selectedAddr.column, width: colWidth + 20)
          }
        }

        HStack(spacing: 8) {
          Text("행 높이 (\(Int(rowHeight))pt)")
            .font(.system(size: 10, design: .monospaced))
          inspectorButton("-20pt", palette: palette) {
            viewModel.updateActiveSheetRowHeight(row: selectedAddr.row, height: rowHeight - 20)
          }
          inspectorButton("+20pt", palette: palette) {
            viewModel.updateActiveSheetRowHeight(row: selectedAddr.row, height: rowHeight + 20)
          }
        }

        HStack(spacing: 10) {
          inspectorButton("+행", palette: palette) { viewModel.insertSheetRowAtSelection() }
          inspectorButton("-행", palette: palette) { viewModel.deleteSheetRowAtSelection() }
          inspectorButton("+열", palette: palette) { viewModel.insertSheetColumnAtSelection() }
          inspectorButton("-열", palette: palette) { viewModel.deleteSheetColumnAtSelection() }
        }

        FlatToggleControl(
          title: "머리글 행 고정",
          systemImage: "pin.fill",
          isOn: $viewModel.isHeaderRowFrozen,
          palette: palette
        )

        FlatToggleControl(
          title: "첫 열(A열) 틀 고정",
          systemImage: "pin.fill",
          isOn: $viewModel.isFirstColumnFrozen,
          palette: palette
        )

        FlatToggleControl(
          title: "셀 내 데이터 막대 표시",
          systemImage: "chart.bar.fill",
          isOn: $viewModel.isDataBarsEnabled,
          palette: palette
        )

        inspectorButton("시트 추가 (+)", isProminent: true, palette: palette) {
          viewModel.addWorksheetTab()
        }
      }
    } else {
      Text("시트 고급 분석 및 데이터 정제")
        .font(.system(size: 11, weight: .bold))
        .foregroundStyle(palette.primaryText.swiftUIColor)

      VStack(alignment: .leading, spacing: 6) {
        Text("선택 셀 메모 (\(viewModel.selectedSheetCell))")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        FlatUnderlineTextField(
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

      VStack(alignment: .leading, spacing: 8) {
        inspectorButton("아래로 3행 자동 채우기", palette: palette) {
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

        FlatStepperControl(
          label: "자동 필터 열: \(CellAddress.columnLabel(for: max(0, viewModel.sheetAutoFilterColumn)))열",
          value: $viewModel.sheetAutoFilterColumn,
          range: 0...15,
          palette: palette
        )
        FlatUnderlineTextField(
          placeholder: "열 자동 필터 검색어",
          text: $viewModel.sheetAutoFilterQuery,
          width: nil,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )

        inspectorButton("선택 셀 병합 / 해제", palette: palette) {
          viewModel.toggleMergeOnSelectedSheetCell()
        }

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
          FlatUnderlineTextField(
            placeholder: "이름",
            text: $viewModel.namedRangeNameDraft,
            width: 78,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )
          FlatUnderlineTextField(
            placeholder: "범위 (예: B2:B4)",
            text: $viewModel.namedRangeTargetDraft,
            width: 96,
            isMonospaced: true,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )
        }
        inspectorButton("이름 정의 등록", isProminent: true, palette: palette) {
          viewModel.defineNamedRangeOnActiveSheet()
        }

        Divider()

        Text("목표값 찾기 (Goal Seek)")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        HStack(spacing: 6) {
          FlatUnderlineTextField(
            placeholder: "수식 셀",
            text: $viewModel.goalSeekTargetCellDraft,
            width: 56,
            isMonospaced: true,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )
          FlatUnderlineTextField(
            placeholder: "목표값",
            text: $viewModel.goalSeekTargetValueDraft,
            width: 56,
            isMonospaced: true,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )
          FlatUnderlineTextField(
            placeholder: "변경 셀",
            text: $viewModel.goalSeekVariableCellDraft,
            width: 56,
            isMonospaced: true,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )
        }
        inspectorButton("목표값 찾기 실행", isProminent: true, palette: palette) {
          viewModel.runGoalSeekOnActiveSheet()
        }

        Divider()

        inspectorButton("통화/백분율/소수 서식 전환", palette: palette) {
          viewModel.executeRibbonCommand("sheet-home-style")
        }

        FlatToolbarMenu(
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

        Text("데이터 정제")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        HStack(spacing: 10) {
          inspectorButton("중복 행 제거", palette: palette) {
            viewModel.removeDuplicatesInActiveSheet()
          }
          inspectorButton("텍스트 나누기(,)", palette: palette) {
            viewModel.splitActiveSheetColumnByDelimiter()
          }
        }
        inspectorButton("다중 기준 정렬 (A열↑ · B열↓)", palette: palette) {
          viewModel.sortActiveSheetBySecondaryColumn(
            primaryColumn: 0,
            ascending: true,
            secondaryColumn: 1,
            secondaryAscending: false
          )
        }

        inspectorButton(
          viewModel.activeConditionalFormatRule == nil ? "조건부 서식 켜기 (>100)" : "조건부 서식 끄기",
          palette: palette
        ) {
          viewModel.executeRibbonCommand("sheet-mod-condfmt")
        }
      }
    }
  }

  @ViewBuilder
  private func slideInspectorSection(deck: PresentationDeckModel, palette: WorkspaceThemePalette)
    -> some View
  {
    let sIdx =
      deck.slides.indices.contains(viewModel.selectedSlideIndex) ? viewModel.selectedSlideIndex : 0
    let activeSlide = deck.slides.indices.contains(sIdx) ? deck.slides[sIdx] : nil
    let targetElementID = viewModel.selectedSlideElementID ?? activeSlide?.elements.first?.id
    let activeElement = activeSlide?.elements.first(where: { $0.id == targetElementID })

    if viewModel.activeInspectorSubTab == .propertiesAndLayout {
      Text("슬라이드 마스터 및 개체 제어")
        .font(.system(size: 11, weight: .bold))
        .foregroundStyle(palette.primaryText.swiftUIColor)
      Text("총 슬라이드 \(deck.slides.count)장")
        .font(.system(size: 11))

      FlatToolbarMenu(
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

      if let slide = activeSlide {
        FlatToolbarMenu(
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

        let shortTransLabel =
          slide.transition.rawValue.components(separatedBy: " ").first ?? slide.transition.rawValue
        HStack(spacing: 12) {
          FlatToolbarMenu(
            title: "비율: \(deck.aspectRatio == .widescreen16x9 ? "16:9" : "4:3")",
            systemImage: "aspectratio",
            palette: palette
          ) {
            ForEach(SlideAspectRatio.allCases, id: \.self) { ratio in
              Button(ratio.rawValue) {
                mutateSlideDeck { $0.aspectRatio = ratio }
              }
            }
          }

          FlatToolbarMenu(
            title: "전환: \(shortTransLabel)",
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

        VStack(alignment: .leading, spacing: 6) {
          Text(
            "전환 시간 \(String(format: "%.1f", slide.transitionDurationSeconds))초 · 자동 전환 \(slide.autoAdvanceSeconds.map { "\($0)초" } ?? "수동")"
          )
          .font(.system(size: 10, weight: .semibold, design: .monospaced))
          .foregroundStyle(palette.secondaryText.swiftUIColor)

          HStack(spacing: 10) {
            ForEach([0.4, 0.8, 1.5, 2.5], id: \.self) { sec in
              inspectorButton("\(String(format: "%.1f", sec))초", palette: palette) {
                viewModel.setActiveSlideTransitionDuration(sec)
              }
            }
          }

          HStack(spacing: 10) {
            inspectorButton("수동", palette: palette) {
              viewModel.setActiveSlideAutoAdvance(seconds: nil)
            }
            inspectorButton("5초", palette: palette) {
              viewModel.setActiveSlideAutoAdvance(seconds: 5)
            }
            inspectorButton("10초", palette: palette) {
              viewModel.setActiveSlideAutoAdvance(seconds: 10)
            }
          }
        }

        VStack(alignment: .leading, spacing: 6) {
          Text("발표자 노트")
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
          FlatUnderlineTextField(
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

      FlatToggleControl(
        title: "슬라이드 정렬 격자 뷰",
        systemImage: "square.grid.3x2.fill",
        isOn: $viewModel.isSlideSorterViewEnabled,
        palette: palette
      )

      FlatToggleControl(
        title: "스테이지 격자 안내선",
        systemImage: "grid",
        isOn: Binding(
          get: { viewModel.isSlideGridlinesVisible },
          set: { _ in viewModel.toggleSlideGridlines() }
        ),
        palette: palette
      )

      VStack(alignment: .leading, spacing: 8) {
        Text("슬라이드 덱 관리")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        HStack(spacing: 10) {
          inspectorButton("슬라이드 추가", palette: palette) { viewModel.addSlideToActiveDeck() }
          inspectorButton("복제", palette: palette) { viewModel.duplicateActiveSlide() }
          inspectorButton("삭제", palette: palette) { viewModel.deleteActiveSlide() }
            .disabled(deck.slides.count <= 1)
        }
        HStack(spacing: 10) {
          inspectorButton("위로 이동", palette: palette) { viewModel.moveActiveSlide(offset: -1) }
          inspectorButton("아래로 이동", palette: palette) { viewModel.moveActiveSlide(offset: 1) }
        }
      }

      inspectorButton(
        viewModel.isFullScreenSlideshowActive ? "전체화면 슬라이드쇼 종료" : "전체화면 슬라이드쇼 시작",
        isProminent: true,
        palette: palette
      ) {
        viewModel.toggleFullScreenSlideshow()
      }
    } else {
      Text("선택 개체 조판 및 정렬")
        .font(.system(size: 11, weight: .bold))
        .foregroundStyle(palette.primaryText.swiftUIColor)

      if let elem = activeElement, let elementID = targetElementID {
        Text("선택 개체 (\(Int(elem.x)), \(Int(elem.y)) · \(Int(elem.width))×\(Int(elem.height)))")
          .font(.system(size: 10, weight: .semibold, design: .monospaced))
          .foregroundStyle(palette.secondaryText.swiftUIColor)

        FlatUnderlineTextField(
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

        FlatUnderlineTextField(
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

        HStack(spacing: 10) {
          inspectorButton("X -10", palette: palette) {
            viewModel.moveSlideElement(slideIndex: sIdx, elementID: elementID, deltaX: -10, deltaY: 0)
          }
          inspectorButton("X +10", palette: palette) {
            viewModel.moveSlideElement(slideIndex: sIdx, elementID: elementID, deltaX: 10, deltaY: 0)
          }
          inspectorButton("Y -10", palette: palette) {
            viewModel.moveSlideElement(slideIndex: sIdx, elementID: elementID, deltaX: 0, deltaY: -10)
          }
          inspectorButton("Y +10", palette: palette) {
            viewModel.moveSlideElement(slideIndex: sIdx, elementID: elementID, deltaX: 0, deltaY: 10)
          }
        }

        HStack(spacing: 10) {
          inspectorButton("폭 -20", palette: palette) {
            viewModel.resizeSlideElement(
              slideIndex: sIdx, elementID: elementID, deltaWidth: -20, deltaHeight: 0)
          }
          inspectorButton("폭 +20", palette: palette) {
            viewModel.resizeSlideElement(
              slideIndex: sIdx, elementID: elementID, deltaWidth: 20, deltaHeight: 0)
          }
          inspectorButton("높이 -20", palette: palette) {
            viewModel.resizeSlideElement(
              slideIndex: sIdx, elementID: elementID, deltaWidth: 0, deltaHeight: -20)
          }
          inspectorButton("높이 +20", palette: palette) {
            viewModel.resizeSlideElement(
              slideIndex: sIdx, elementID: elementID, deltaWidth: 0, deltaHeight: 20)
          }
        }

        HStack(spacing: 8) {
          Text("회전 \(Int(elem.rotationDegrees))°")
            .font(.system(size: 10, design: .monospaced))
          inspectorButton("-15°", palette: palette) {
            viewModel.rotateSlideElement(
              slideIndex: sIdx, elementID: elementID, toDegrees: elem.rotationDegrees - 15)
          }
          inspectorButton("+15°", palette: palette) {
            viewModel.rotateSlideElement(
              slideIndex: sIdx, elementID: elementID, toDegrees: elem.rotationDegrees + 15)
          }
          inspectorButton("0° 초기화", palette: palette) {
            viewModel.rotateSlideElement(slideIndex: sIdx, elementID: elementID, toDegrees: 0)
          }
        }

        VStack(alignment: .leading, spacing: 6) {
          Text("애니메이션: \(elem.animation.rawValue)")
            .font(.system(size: 10))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
          inspectorButton("애니메이션 전환", palette: palette) {
            viewModel.cycleSlideElementAnimation(slideIndex: sIdx, elementID: elementID)
          }
        }

        HStack(spacing: 10) {
          inspectorButton("앞으로", palette: palette) {
            viewModel.moveSlideElementZOrder(
              slideIndex: sIdx, elementID: elementID, bringForward: true)
          }
          inspectorButton("뒤로", palette: palette) {
            viewModel.moveSlideElementZOrder(
              slideIndex: sIdx, elementID: elementID, bringForward: false)
          }
        }

        Text("개체 정렬 · 배분 · 복제")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        HStack(spacing: 8) {
          inspectorButton("좌측", palette: palette) { viewModel.alignSlideElements(.alignLeft) }
          inspectorButton("중앙", palette: palette) { viewModel.alignSlideElements(.alignCenter) }
          inspectorButton("우측", palette: palette) { viewModel.alignSlideElements(.alignRight) }
          inspectorButton("배분", palette: palette) {
            viewModel.alignSlideElements(.distributeHorizontally)
          }
        }
        HStack(spacing: 10) {
          inspectorButton("개체 복제", palette: palette) { viewModel.duplicateActiveSlideElement() }
          inspectorButton("개체 삭제", palette: palette) { viewModel.deleteActiveSlideElement() }
        }
      }

      Divider()

      inspectorButton("스마트 스냅 정렬", isProminent: true, palette: palette) {
        viewModel.executeRibbonCommand("slide-mod-snap")
      }
    }
  }

  @ViewBuilder
  private func pdfInspectorSection(pdf: PDFStudioDocumentModel, palette: WorkspaceThemePalette)
    -> some View
  {
    let activePageStrokes = pdf.inkStrokes.filter {
      $0.pageNumber == viewModel.selectedPDFPageNumber
    }

    if viewModel.activeInspectorSubTab == .propertiesAndLayout {
      Text("PDF 페이지·주석 제어")
        .font(.system(size: 11, weight: .bold))
        .foregroundStyle(palette.primaryText.swiftUIColor)
      Text(
        "총 페이지 \(pdf.pages.count)p · 주석 \(pdf.annotations.count)건 · 서명 획 \(pdf.inkStrokes.count)건"
      )
      .font(.system(size: 11))

      FlatStepperControl(
        label: "대상 페이지: \(viewModel.selectedPDFPageNumber)p",
        value: $viewModel.selectedPDFPageNumber,
        range: 1...max(1, pdf.pages.count),
        palette: palette
      )

      Divider()

      VStack(alignment: .leading, spacing: 8) {
        HStack(spacing: 10) {
          inspectorButton("90° 회전", palette: palette) {
            viewModel.rotateActivePDFPage(clockwiseDegrees: 90)
          }
          inspectorButton("하이라이트 주석", palette: palette) {
            viewModel.addPDFAnnotation(kind: .highlight, content: "하이라이트 주석")
          }
        }

        HStack(spacing: 10) {
          inspectorButton("빈 페이지 추가", palette: palette) {
            viewModel.insertBlankPDFPage(afterPage: viewModel.selectedPDFPageNumber)
          }
          inspectorButton("페이지 삭제", palette: palette) {
            viewModel.deleteActivePDFPage()
          }
          .disabled(pdf.pages.count <= 1)
        }

        HStack(spacing: 10) {
          inspectorButton("페이지 위로", palette: palette) { viewModel.moveActivePDFPage(offset: -1) }
          inspectorButton("페이지 아래로", palette: palette) { viewModel.moveActivePDFPage(offset: 1) }
        }

        HStack(spacing: 10) {
          inspectorButton("PDF 탭 병합", palette: palette) {
            viewModel.mergeActivePDFWithAnotherOpenTab()
          }
          inspectorButton("페이지 분할 (\(viewModel.selectedPDFPageNumber)p)", palette: palette) {
            viewModel.executeRibbonCommand("pdf-mod-splitmerge")
          }
        }

        inspectorButton("문서(.docx)로 변환", isProminent: true, palette: palette) {
          viewModel.executeRibbonCommand("pdf-mod-tooffice")
        }
      }

      Divider()

      VStack(alignment: .leading, spacing: 8) {
        Text("전자 결재 스탬프")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        HStack(spacing: 10) {
          inspectorButton("승인", palette: palette) {
            viewModel.addStampToActivePDFPage(preset: .approved)
          }
          inspectorButton("대외비", palette: palette) {
            viewModel.addStampToActivePDFPage(preset: .confidential)
          }
          inspectorButton("검토필", palette: palette) {
            viewModel.addStampToActivePDFPage(preset: .reviewed)
          }
          inspectorButton("긴급", palette: palette) {
            viewModel.addStampToActivePDFPage(preset: .urgent)
          }
        }

        Divider()

        Text("워터마크")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)

        FlatUnderlineTextField(
          placeholder: "워터마크 문구",
          text: $viewModel.pdfWatermarkDraft,
          width: nil,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )

        HStack(spacing: 10) {
          inspectorButton("워터마크 적용", isProminent: true, palette: palette) {
            viewModel.applyWatermarkOnActivePDF()
          }
          inspectorButton("워터마크 해제", palette: palette) {
            viewModel.applyWatermarkOnActivePDF(text: "")
          }
        }
      }
    } else {
      Text("PDF 서명·마스킹·양식")
        .font(.system(size: 11, weight: .bold))
        .foregroundStyle(palette.primaryText.swiftUIColor)

      VStack(alignment: .leading, spacing: 8) {
        FlatToggleControl(
          title: "자유곡선 펜 그리기 모드",
          systemImage: "scribble.variable",
          isOn: $viewModel.isPDFInkDrawingModeEnabled,
          palette: palette
        )

        HStack(spacing: 8) {
          Text("펜 색상")
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
          ForEach(["#5C6BC0", "#D32F2F", "#1E293B"], id: \.self) { hex in
            inspectorButton(hex, palette: palette) {
              viewModel.pdfInkStrokeColorHex = hex
            }
          }
        }

        inspectorButton("샘플 벡터 서명 획 추가", palette: palette) {
          viewModel.addInkStrokeToActivePDFPage(points: [
            PDFInkPoint(x: 90, y: 140),
            PDFInkPoint(x: 145, y: 168),
            PDFInkPoint(x: 210, y: 136),
            PDFInkPoint(x: 265, y: 162),
          ])
        }

        inspectorButton("현재 페이지 자유곡선 지우기 (\(activePageStrokes.count)건)", palette: palette) {
          viewModel.clearInkStrokesOnActivePDFPage()
        }
        .disabled(activePageStrokes.isEmpty)
      }

      Divider()

      VStack(alignment: .leading, spacing: 8) {
        Text("민감 정보 영구 마스킹")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        FlatUnderlineTextField(
          placeholder: "마스킹할 구문 입력",
          text: $viewModel.pdfRedactionPhraseDraft,
          width: nil,
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        )
        inspectorButton("영구 마스킹 실행", isProminent: true, palette: palette) {
          viewModel.redactSensitiveTextOnActivePDF()
        }

        Divider()

        Text("양식 필드")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        HStack(spacing: 6) {
          FlatUnderlineTextField(
            placeholder: "필드명",
            text: $viewModel.pdfFormFieldNameDraft,
            width: 78,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )
          FlatUnderlineTextField(
            placeholder: "기본값",
            text: $viewModel.pdfFormFieldValueDraft,
            width: 78,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )
        }
        HStack(spacing: 10) {
          inspectorButton("텍스트 필드 추가", palette: palette) {
            viewModel.addFormFieldToActivePDF(isCheckbox: false)
          }
          inspectorButton("체크박스 추가", palette: palette) {
            viewModel.addFormFieldToActivePDF(isCheckbox: true)
          }
        }

        Divider()

        Text("책갈피 목차 관리")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(palette.secondaryText.swiftUIColor)
        HStack(spacing: 8) {
          FlatUnderlineTextField(
            placeholder: "책갈피 제목",
            text: $viewModel.bookmarkTitleDraft,
            width: nil,
            palette: palette,
            isSnapshotMode: viewModel.isCanvasSnapshotMode
          )
          inspectorButton("추가", isProminent: true, palette: palette) {
            viewModel.addBookmarkToActivePDF()
          }
        }
      }
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
