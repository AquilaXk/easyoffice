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
      .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
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
    }

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
      .tint(EasyOfficeDesignTokens.brand700.swiftUIColor)
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
      .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
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

      Button("통화/백분율/소수 서식 전환") { viewModel.executeRibbonCommand("sheet-home-style") }
        .buttonStyle(.bordered).controlSize(.small)

      Toggle("머리글 행 고정", isOn: $viewModel.isHeaderRowFrozen)
        .font(.system(size: 11, weight: .medium))

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
