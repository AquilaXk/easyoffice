import EasyOfficeDesignSystem
import Foundation

/// Concrete command button descriptor displayed inside a context ribbon bar category.
public struct RibbonCommandDescriptor: Sendable, Equatable, Identifiable {
  public let id: String
  public let title: String
  public let symbolName: String
  public let shortcutHint: String

  public init(id: String, title: String, symbolName: String, shortcutHint: String) {
    self.id = id
    self.title = title
    self.symbolName = symbolName
    self.shortcutHint = shortcutHint
  }
}

public typealias WorkspaceRibbonCategory = RibbonCategory

/// Ribbon layout style supporting both global standard office and Korean HWP page/manuscript modes.
public enum RibbonLayoutStyle: String, CaseIterable, Sendable, Identifiable {
  case standardOffice = "표준 오피스"
  case hangulHwp = "한글(HWP) 쪽·편집"

  public var id: String { rawValue }

  public func displayTitle(for category: WorkspaceRibbonCategory) -> String {
    switch self {
    case .standardOffice:
      return category.rawValue
    case .hangulHwp:
      switch category {
      case .home: return "편집"
      case .insert: return "입력"
      case .layout: return "쪽·원고지"
      case .formulasAndData: return "표·개체"
      case .moduleSpecific: return "시트·차트"
      case .review: return "보안·도구"
      case .view: return "보기"
      }
    }
  }
}

/// Context-aware ribbon tabs adapting to the active document module.
public enum RibbonCategory: String, CaseIterable, Sendable, Identifiable {
  case home = "홈 (Home)"
  case insert = "삽입 (Insert)"
  case layout = "모양/레이아웃 (Layout)"
  case formulasAndData = "수식·데이터 (Data)"
  case moduleSpecific = "전용 도구 (Module)"
  case review = "검토/보안 (Review)"
  case view = "보기 (View)"

  public var id: String { rawValue }

  public func title(for module: OfficeModuleKind) -> String {
    switch self {
    case .home: return "홈"
    case .insert: return "삽입"
    case .layout: return "레이아웃"
    case .formulasAndData: return "수식·데이터"
    case .moduleSpecific:
      switch module {
      case .document: return "참조·메일머지"
      case .hwp: return "공문서·누름틀"
      case .sheet: return "피벗·차트"
      case .slide: return "슬라이드·전환"
      case .pdf: return "주석·변환"
      }
    case .review: return "검토"
    case .view: return "보기"
    }
  }

  public static func actions(for category: RibbonCategory, module: OfficeModuleKind)
    -> [RibbonCommandDescriptor]
  {
    switch category {
    case .home:
      return [
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-home-bold", title: "굵게", symbolName: "bold", shortcutHint: "⌘B"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-home-italic", title: "기울임", symbolName: "italic",
          shortcutHint: "⌘I"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-home-underline", title: "밑줄", symbolName: "underline",
          shortcutHint: "⌘U"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-home-style", title: "스타일 적용", symbolName: "textformat.size",
          shortcutHint: "⌥⌘S"),
      ]
    case .insert:
      return [
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-insert-table", title: "표 삽입", symbolName: "tablecells",
          shortcutHint: "⌃⌘T"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-insert-chart", title: "차트 개체", symbolName: "chart.bar.xaxis",
          shortcutHint: "⌃⌘C"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-insert-shape", title: "도형·다이어그램", symbolName: "square.on.circle",
          shortcutHint: "⌃⌘S"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-insert-link", title: "책갈피·링크", symbolName: "link",
          shortcutHint: "⌘K"),
      ]
    case .layout:
      return [
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-layout-paper", title: "용지 규격 (A4/Letter)",
          symbolName: "doc.viewfinder", shortcutHint: "⇧⌘P"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-layout-orientation", title: "용지 방향 전환",
          symbolName: "rectangle.portrait.rotate", shortcutHint: "⌥⌘O"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-layout-columns", title: "다단 설정",
          symbolName: "rectangle.split.3x1", shortcutHint: "⌥⌘M"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-layout-margins", title: "여백 프리셋",
          symbolName: "arrow.up.left.and.down.right.magnifyingglass", shortcutHint: "⌥⌘G"),
      ]
    case .formulasAndData:
      return [
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-data-autosum", title: "자동 합계 (SUM)", symbolName: "sum",
          shortcutHint: "⌥⇧="),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-data-fn-browser",
          title: "함수 마법사", symbolName: "function",
          shortcutHint: "⇧F3"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-data-sort", title: "오름차순·내림차순 정렬",
          symbolName: "arrow.up.arrow.down.square", shortcutHint: "⌥⌘R"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-data-validation", title: "데이터 유효성 검사",
          symbolName: "checkmark.shield", shortcutHint: "⌥⌘V"),
      ]
    case .moduleSpecific:
      switch module {
      case .document:
        return [
          RibbonCommandDescriptor(
            id: "doc-mod-toc", title: "자동 목차 생성", symbolName: "list.Number", shortcutHint: "⌃⌥T"),
          RibbonCommandDescriptor(
            id: "doc-mod-footnote", title: "각주·미주 삽입", symbolName: "text.badge.plus",
            shortcutHint: "⌥⌘F"),
          RibbonCommandDescriptor(
            id: "doc-mod-mailmerge", title: "메일 머지 치환", symbolName: "envelope.badge.person.crop",
            shortcutHint: "⌃⌘M"),
          RibbonCommandDescriptor(
            id: "doc-mod-tableformula", title: "표 셀 수식 계산", symbolName: "equal.square",
            shortcutHint: "⌃⌘="),
        ]
      case .hwp:
        return [
          RibbonCommandDescriptor(
            id: "hwp-mod-header", title: "행정 공문서 두문·결재란", symbolName: "signature",
            shortcutHint: "⌃⌥H"),
          RibbonCommandDescriptor(
            id: "hwp-mod-field", title: "누름틀 필드 삽입",
            symbolName: "rectangle.and.pencil.and.ellipsis", shortcutHint: "⌃⌘E"),
          RibbonCommandDescriptor(
            id: "hwp-mod-border", title: "이중선·대각선 테두리",
            symbolName: "square.grid.3x3.topleft.filled", shortcutHint: "⌃⌘B"),
          RibbonCommandDescriptor(
            id: "hwp-mod-manuscript", title: "200자 원고지 매수 검산", symbolName: "character.book.closed",
            shortcutHint: "⌃⌘W"),
        ]
      case .sheet:
        return [
          RibbonCommandDescriptor(
            id: "sheet-mod-pivot", title: "피벗 테이블 요약", symbolName: "tablecells.badge.ellipsis",
            shortcutHint: "⌥⌘P"),
          RibbonCommandDescriptor(
            id: "sheet-mod-chart", title: "7종 차트 시각화", symbolName: "chart.pie.fill",
            shortcutHint: "⌥F1"),
          RibbonCommandDescriptor(
            id: "sheet-mod-condfmt", title: "조건부 서식 규칙", symbolName: "paintbrush.pointed",
            shortcutHint: "⌃⌘F"),
          RibbonCommandDescriptor(
            id: "sheet-mod-freeze", title: "행·열 틀 고정", symbolName: "lock.rectangle.on.rectangle",
            shortcutHint: "⌃⌘L"),
        ]
      case .slide:
        return [
          RibbonCommandDescriptor(
            id: "slide-mod-master", title: "5대 마스터 레이아웃", symbolName: "rectangle.3.group",
            shortcutHint: "⇧⌘M"),
          RibbonCommandDescriptor(
            id: "slide-mod-snap", title: "스마트 정렬 가이드", symbolName: "ruler", shortcutHint: "⌘;"),
          RibbonCommandDescriptor(
            id: "slide-mod-transition", title: "화면 전환 효과", symbolName: "sparkles.tv",
            shortcutHint: "⌥⌘T"),
          RibbonCommandDescriptor(
            id: "slide-mod-presenter", title: "발표자 도구 시작", symbolName: "play.display",
            shortcutHint: "⌥⌘P"),
        ]
      case .pdf:
        return [
          RibbonCommandDescriptor(
            id: "pdf-mod-highlight", title: "형광펜·밑줄 주석", symbolName: "highlighter",
            shortcutHint: "⌃⌘H"),
          RibbonCommandDescriptor(
            id: "pdf-mod-rotate", title: "페이지 90° 회전", symbolName: "rotate.right",
            shortcutHint: "⌘R"),
          RibbonCommandDescriptor(
            id: "pdf-mod-splitmerge", title: "페이지 분할·병합", symbolName: "doc.on.doc",
            shortcutHint: "⇧⌘S"),
          RibbonCommandDescriptor(
            id: "pdf-mod-tooffice", title: "편집 문서(DOCX/HWPX) 변환",
            symbolName: "arrow.right.doc.on.clipboard", shortcutHint: "⇧⌘E"),
        ]
      }
    case .review:
      return [
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-review-track", title: "변경 내용 추적",
          symbolName: "pencil.and.outline", shortcutHint: "⇧⌘E"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-review-comment", title: "검토 메모 스레드", symbolName: "text.bubble",
          shortcutHint: "⌥⌘C"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-review-verify", title: "문서 무결성 점검", symbolName: "lock.shield",
          shortcutHint: "⌃⌘V"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-review-stats", title: "문서 통계·글자 수",
          symbolName: "chart.bar.doc.horizontal", shortcutHint: "⇧⌘I"),
      ]
    case .view:
      return [
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-view-split", title: "좌우 스플릿 뷰", symbolName: "rectangle.split.2x1",
          shortcutHint: "⌃⌘2"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-view-ribbon", title: "리본 접기·펼치기", symbolName: "menubar.rectangle",
          shortcutHint: "⌥⌘R"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-view-theme", title: "라이트·다크 테마 전환",
          symbolName: "circle.lefthalf.filled", shortcutHint: "⌃⌥D"),
        RibbonCommandDescriptor(
          id: "\(module.rawValue)-view-zoom", title: "100% 실제 배율", symbolName: "1.magnifyingglass",
          shortcutHint: "⌘0"),
      ]
    }
  }
}

extension SlideMasterLayout {
  public static var twoColumn: SlideMasterLayout { .twoColumnComparison }
  public static var comparison: SlideMasterLayout { .twoColumnComparison }
  public static var blank: SlideMasterLayout { .blankCanvas }
}

extension DocumentFormat {
  public var primaryFileExtension: String { fileExtension }
}

public enum BatchConversionQueueStatus: String, Sendable, Equatable {
  case pending = "대기"
  case completed = "완료"
  case failed = "실패"
}

public struct BatchConversionQueueItem: Identifiable, Sendable, Equatable {
  public let id: UUID
  public let sourceFileName: String
  public let sourceFormat: DocumentFormat
  public let targetFormat: DocumentFormat
  public let status: BatchConversionQueueStatus
  public let convertedPayloadBytes: Data?
  public let result: ConversionJobResult?
  public let errorDescription: String?

  public init(
    id: UUID = UUID(),
    sourceFileName: String,
    sourceFormat: DocumentFormat,
    targetFormat: DocumentFormat,
    status: BatchConversionQueueStatus,
    convertedPayloadBytes: Data? = nil,
    result: ConversionJobResult? = nil,
    errorDescription: String? = nil
  ) {
    self.id = id
    self.sourceFileName = sourceFileName
    self.sourceFormat = sourceFormat
    self.targetFormat = targetFormat
    self.status = status
    self.convertedPayloadBytes = convertedPayloadBytes
    self.result = result
    self.errorDescription = errorDescription
  }
}

extension WorksheetModel {
  public mutating func renameSheetPage(at index: Int, to newName: String) {
    let trimmed = newName.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return }
    let currentIndex = min(max(0, activeSheetIndex), additionalSheets.count)
    if index == currentIndex {
      self.name = trimmed
    } else {
      let adjusted = index < currentIndex ? index : (index - 1)
      guard additionalSheets.indices.contains(adjusted) else { return }
      additionalSheets[adjusted].name = trimmed
    }
  }

  public mutating func deleteSheetPage(at index: Int) {
    guard allSheetNames.count > 1 else { return }
    let currentIndex = min(max(0, activeSheetIndex), additionalSheets.count)
    if index == currentIndex {
      let replacementIndex = min(index, additionalSheets.count - 1)
      guard additionalSheets.indices.contains(replacementIndex) else { return }
      let nextActive = additionalSheets.remove(at: replacementIndex)
      self.name = nextActive.name
      self.cellsByCoordinate = nextActive.cellsByCoordinate
      self.cellStyles = nextActive.cellStyles
      self.sheetComments = nextActive.sheetComments
      self.mergedRanges = nextActive.mergedRanges
      self.columnWidths = nextActive.columnWidths
      self.rowHeights = nextActive.rowHeights
      self.conditionalFormatRules = nextActive.conditionalFormatRules
      self.dataValidationRule = nextActive.dataValidationRule
      self.activeSheetIndex = replacementIndex
      self.compiledASTs.removeAll(keepingCapacity: true)
      for (coord, rawVal) in self.cellsByCoordinate where !rawVal.isEmpty {
        self.compiledASTs[coord] = Self.compileCellInput(rawVal)
      }
    } else {
      let adjusted = index < currentIndex ? index : (index - 1)
      guard additionalSheets.indices.contains(adjusted) else { return }
      additionalSheets.remove(at: adjusted)
      if index < currentIndex {
        activeSheetIndex = max(0, currentIndex - 1)
      }
    }
  }

  public func expandedGridBounds(
    minRows: Int = 30,
    minCols: Int = 12,
    extraRows: Int = 12,
    extraCols: Int = 4,
    maxRows: Int = 512,
    maxCols: Int = 64
  ) -> (rows: Int, cols: Int) {
    var detectedMaxRow = 0
    var detectedMaxCol = 0
    let safeUpperRow = max(minRows, min(maxRows, CellAddress.maxZeroBasedRow + 1))
    let safeUpperCol = max(minCols, min(maxCols, CellAddress.maxZeroBasedColumn + 1))
    for coord in cellsByCoordinate.keys {
      if coord.row < safeUpperRow {
        detectedMaxRow = max(detectedMaxRow, coord.row + 1)
      } else {
        detectedMaxRow = max(detectedMaxRow, safeUpperRow)
      }
      if coord.column < safeUpperCol {
        detectedMaxCol = max(detectedMaxCol, coord.column + 1)
      } else {
        detectedMaxCol = max(detectedMaxCol, safeUpperCol)
      }
    }
    for merged in mergedRanges {
      let mRow = min(safeUpperRow, max(merged.start.row, merged.end.row) + 1)
      let mCol = min(safeUpperCol, max(merged.start.column, merged.end.column) + 1)
      detectedMaxRow = max(detectedMaxRow, mRow)
      detectedMaxCol = max(detectedMaxCol, mCol)
    }
    let rows = min(safeUpperRow, max(minRows, detectedMaxRow + extraRows))
    let cols = min(safeUpperCol, max(minCols, detectedMaxCol + extraCols))
    let (area, overflow) = rows.multipliedReportingOverflow(by: cols)
    if overflow || area > CellRange.maxExpandedCellCount {
      let cappedRows = min(rows, max(1, CellRange.maxExpandedCellCount / max(1, cols)))
      return (cappedRows, cols)
    }
    return (rows, cols)
  }

  public func isSummaryOrTotalRow(
    _ zeroBasedRow: Int,
    startColumn: Int,
    valueColumn: Int
  ) -> Bool {
    let labelCoord = PackedCellCoordinate(row: zeroBasedRow, column: startColumn)
    let rawLabel = (cellsByCoordinate[labelCoord] ?? "").trimmingCharacters(
      in: .whitespacesAndNewlines)
    let normalizedLabel = rawLabel.lowercased()
    let summaryKeywords = ["전체 합계", "합계", "총계", "소계", "total", "grand total", "subtotal"]
    if summaryKeywords.contains(normalizedLabel)
      || rawLabel.contains("합계")
      || rawLabel.contains("총계")
    {
      return true
    }

    let valCoord = PackedCellCoordinate(row: zeroBasedRow, column: valueColumn)
    let rawVal = (cellsByCoordinate[valCoord] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
      .uppercased()
    if rawVal.hasPrefix("=SUM(") && rawLabel.isEmpty {
      return true
    }
    if rawVal.hasPrefix("=SUM(") {
      let colLabel = CellAddress.columnLabel(for: valueColumn).uppercased()
      let prevColLabel = CellAddress.columnLabel(for: max(0, startColumn + 1)).uppercased()
      let checkCoord = PackedCellCoordinate(row: zeroBasedRow, column: max(0, startColumn + 1))
      let checkVal = (cellsByCoordinate[checkCoord] ?? "").trimmingCharacters(
        in: .whitespacesAndNewlines
      ).uppercased()
      if checkVal.hasPrefix("=SUM(\(prevColLabel)") || rawVal.hasPrefix("=SUM(\(colLabel)") {
        return true
      }
    }
    return false
  }

  public func computeFilteredPivotSummary(
    range: CellRange? = nil,
    hasHeaderRow: Bool = true,
    configuration: PivotTableConfiguration? = nil,
    aggregation: PivotAggregationFunction = .sum
  ) -> PivotTableSummary {
    let rawBaseRange = range ?? detectedDataRange()
    let rawMinR = min(rawBaseRange.start.row, rawBaseRange.end.row)
    let rawMaxR = max(rawBaseRange.start.row, rawBaseRange.end.row)
    let rawMinC = min(rawBaseRange.start.column, rawBaseRange.end.column)
    let rawMaxC = max(rawBaseRange.start.column, rawBaseRange.end.column)
    let (rSpan, rOv) = (rawMaxR - rawMinR).addingReportingOverflow(1)
    let (cSpan, cOv) = (rawMaxC - rawMinC).addingReportingOverflow(1)
    let (area, aOv) = rSpan.multipliedReportingOverflow(by: cSpan)
    let baseRange: CellRange
    if rOv || cOv || aOv || area > CellRange.maxExpandedCellCount {
      let clampedMaxR = min(rawMaxR, rawMinR + 511)
      let clampedMaxC = min(rawMaxC, rawMinC + 63)
      baseRange = CellRange(
        start: CellAddress(packed: PackedCellCoordinate(row: rawMinR, column: rawMinC)),
        end: CellAddress(packed: PackedCellCoordinate(row: clampedMaxR, column: clampedMaxC))
      )
    } else {
      baseRange = rawBaseRange
    }
    let minR = min(baseRange.start.row, baseRange.end.row)
    let maxR = max(baseRange.start.row, baseRange.end.row)
    let minC = min(baseRange.start.column, baseRange.end.column)
    let maxC = max(baseRange.start.column, baseRange.end.column)
    let valueColOffset = max(0, maxC - minC)
    let resolvedConfig =
      configuration
      ?? PivotTableConfiguration(
        rowFieldColumnIndex: 0,
        valueFieldColumnIndex: valueColOffset,
        aggregation: aggregation
      )

    let dataStartRow = hasHeaderRow ? min(maxR, minR + 1) : minR
    let rowKeyColumn = minC + max(0, resolvedConfig.rowFieldColumnIndex)
    let valueColumn = minC + max(0, resolvedConfig.valueFieldColumnIndex)

    var cleanedSheet = self
    if dataStartRow <= maxR {
      for r in dataStartRow...maxR {
        if isSummaryOrTotalRow(r, startColumn: rowKeyColumn, valueColumn: valueColumn) {
          for c in minC...maxC {
            let coord = PackedCellCoordinate(row: r, column: c)
            cleanedSheet.cellsByCoordinate.removeValue(forKey: coord)
            cleanedSheet.compiledASTs.removeValue(forKey: coord)
          }
        }
      }
    }

    return PivotTableEngine.compute(
      from: cleanedSheet,
      range: baseRange,
      hasHeaderRow: hasHeaderRow,
      configuration: resolvedConfig
    )
  }
}
