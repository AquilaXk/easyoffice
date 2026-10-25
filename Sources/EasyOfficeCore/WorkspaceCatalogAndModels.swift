import EasyOfficeDesignSystem
import Foundation

extension DocumentFormat {
  public var moduleKind: OfficeModuleKind {
    switch self {
    case .docx, .doc, .odt, .rtf, .md, .txt: return .document
    case .hwp, .hwpx, .hwt, .hwtx: return .hwp
    case .xlsx, .xls, .ods, .csv, .tsv: return .sheet
    case .pptx, .ppt, .ppsx, .odp: return .slide
    case .pdf: return .pdf
    }
  }

  public var shortSpecificationBadge: String {
    switch self {
    case .docx: return "ECMA-376 DOCX"
    case .xlsx: return "ECMA-376 XLSX"
    case .pptx, .ppsx: return "ECMA-376 PPTX"
    case .hwp, .hwt: return "HWP 5.0 (OLE2)"
    case .hwpx, .hwtx: return "KS X 6101 HWPX"
    case .pdf: return "ISO 32000 PDF"
    case .odt, .ods, .odp: return "ISO 26300 ODF"
    case .doc, .xls, .ppt: return "OLE2 Legacy"
    case .rtf: return "RTF 1.9.1"
    case .csv: return "RFC 4180 CSV"
    case .tsv: return "UTF-8 TSV"
    case .md: return "CommonMark MD"
    case .txt: return "UTF-8 TXT"
    }
  }
}

extension OfficeModuleKind {
  public var quickLauncherCardTitleKo: String {
    switch self {
    case .document: return "새 문서 작성"
    case .hwp: return "새 한글 공문서"
    case .sheet: return "새 시트 통합문서"
    case .slide: return "새 슬라이드 발표"
    case .pdf: return "새 PDF 스튜디오"
    }
  }

  public var quickLauncherFormatBadge: String {
    switch self {
    case .document: return "DOCX · ODT"
    case .hwp: return "HWPX · HWP"
    case .sheet: return "XLSX · CSV"
    case .slide: return "PPTX · ODP"
    case .pdf: return "PDF · 주석"
    }
  }
}

public enum WorkspaceViewMode: String, CaseIterable, Sendable, Codable, Identifiable {
  case editView = "편집 보기"
  case outlineView = "윤곽 보기"
  case readOnlyView = "읽기 전용"

  public var id: String { rawValue }

  public var symbolName: String {
    switch self {
    case .editView: return "pencil.and.outline"
    case .outlineView: return "square.text.square"
    case .readOnlyView: return "book"
    }
  }
}

public enum InspectorSubTab: String, CaseIterable, Sendable, Codable, Identifiable {
  case propertiesAndLayout = "속성·레이아웃"
  case formattingAndTools = "조판·고급"

  public var id: String { rawValue }
}

extension WorkspaceViewModel {
  public func setWorkspaceViewMode(_ mode: WorkspaceViewMode) {
    workspaceViewMode = mode
    if mode == .outlineView {
      showLeftNavigator = true
    }
    lastExecutedCommandSummary = "\(mode.rawValue) 모드로 전환했습니다."
  }

  public func applyQuickZoomPreset(_ percent: Double) {
    setZoom(percent)
    lastExecutedCommandSummary = "화면 배율 \(Int(percent))% 적용"
  }

  public func cycleActiveDocumentColumnCount() {
    guard let tabID = activeTabID,
      let idx = openTabs.firstIndex(where: { $0.id == tabID }),
      case .document(var doc) = openTabs[idx].payload
    else { return }
    recordUndoSnapshot(for: tabID)
    if doc.sections.isEmpty {
      doc.sections = [DocumentSectionLayout(paperSize: doc.paperSize)]
    }
    let nextCols = (doc.sections[0].columnCount % 3) + 1
    doc.sections[0].columnCount = nextCols
    openTabs[idx].payload = .document(doc)
    openTabs[idx].isModified = true
    lastExecutedCommandSummary = "다단 설정 전환: \(nextCols)단"
  }

  public func applyMailMergePlaceholdersToActiveDocument() {
    applyMailMergeFieldReplacement()
  }
}


extension DataValidationRule {
  public init(minimumValue: Double? = nil, maximumValue: Double? = nil, allowedSet: [String]? = nil)
  {
    if let allowedSet, !allowedSet.isEmpty {
      self = .allowedList(allowedSet)
    } else if minimumValue != nil || maximumValue != nil {
      self = .numberRange(
        min: minimumValue ?? -Double.greatestFiniteMagnitude,
        max: maximumValue ?? Double.greatestFiniteMagnitude)
    } else {
      self = .nonEmptyText
    }
  }

  public func validate(rawValue: String) -> Bool {
    let trimmed = rawValue.trimmingCharacters(in: .whitespacesAndNewlines)
    if trimmed.hasPrefix("=") { return true }
    let evaluated: EvaluatedCellValue
    if trimmed.isEmpty {
      evaluated = .empty
    } else if let num = Double(trimmed) {
      evaluated = .number(num)
    } else if trimmed.caseInsensitiveCompare("TRUE") == .orderedSame {
      evaluated = .boolean(true)
    } else if trimmed.caseInsensitiveCompare("FALSE") == .orderedSame {
      evaluated = .boolean(false)
    } else {
      evaluated = .text(trimmed)
    }
    return validate(evaluated)
  }
}

extension CellRange {
  public var a1Range: String {
    start == end ? start.a1Notation : "\(start.a1Notation):\(end.a1Notation)"
  }
}

public typealias SnapGuideLine = AlignmentSnapGuide
public typealias SheetPageSetup = WorksheetPageSetup

extension SlideAspectRatio {
  public var stageSize: (width: Double, height: Double) { (widthPoints, heightPoints) }
}

extension ParagraphBlock {
  public var inlineRuns: [DocumentTextRun] {
    get { runs }
    set { runs = newValue }
  }
}

extension OfficeDocumentModel {
  @inlinable
  public static func currentISO8601Timestamp() -> String {
    EasyOfficeCore.currentISO8601Timestamp()
  }

  public mutating func recordRevision(
    author: String, summary: String, kind: TrackedRevisionKind = .formattingChange
  ) {
    revisions.append(
      TrackedRevision(
        author: author, kind: kind, paragraphID: UUID(), originalText: title, proposedText: summary)
    )
  }
}

extension PDFStudioDocumentModel {
  public mutating func insertBlankPage(afterPage oneBasedPage: Int) {
    insertBlankPage(at: max(0, min(pages.count, oneBasedPage)))
  }
}

/// Top-level workspace navigation section in the macOS sidebar.
public enum HubSection: String, CaseIterable, Sendable, Identifiable {
  case home = "홈 허브"
  case recent = "최근 문서"
  case pinned = "고정 문서"
  case templates = "서식 템플릿"
  case converter = "포맷 일괄 변환"
  case smartAssistant = "스마트 문서 도구"

  public var id: String { rawValue }

  public var symbolName: String {
    switch self {
    case .home: return "house.fill"
    case .recent: return "clock.fill"
    case .pinned: return "pin.fill"
    case .templates: return "square.grid.2x2.fill"
    case .converter: return "arrow.triangle.2.circlepath.doc.on.clipboard"
    case .smartAssistant: return "sparkles.rectangle.stack.fill"
    }
  }

  public static var batchConversion: HubSection { .converter }
}

/// Built-in or user-saved template descriptor available from the Home Hub.
public struct OfficeTemplateDescriptor: Sendable, Equatable, Identifiable {
  public let id: String
  public let title: String
  public let subtitle: String
  public let format: DocumentFormat
  public let moduleKind: OfficeModuleKind
  public let isCustomUserTemplate: Bool

  public init(
    id: String, title: String, subtitle: String, format: DocumentFormat,
    isCustomUserTemplate: Bool = false
  ) {
    self.id = id
    self.title = title
    self.subtitle = subtitle
    self.format = format
    self.moduleKind = format.moduleKind
    self.isCustomUserTemplate = isCustomUserTemplate
  }
}

/// Polymorphic payload held by an open workspace tab.
public enum OpenTabPayload: Sendable, Equatable {
  case document(OfficeDocumentModel)
  case sheet(WorksheetModel)
  case slide(PresentationDeckModel)
  case pdf(PDFStudioDocumentModel)
}

/// Open document tab inside the unified macOS workspace window.
public struct WorkspaceTabItem: Sendable, Equatable, Identifiable {
  public let id: UUID
  public var fileName: String
  public var format: DocumentFormat
  public var isPinned: Bool
  public var isFavorite: Bool
  public var isModified: Bool
  public var lastOpenedISO8601: String
  public var fileURL: URL?
  public var payload: OpenTabPayload

  public init(
    id: UUID = UUID(),
    fileName: String,
    format: DocumentFormat,
    isPinned: Bool = false,
    isFavorite: Bool = false,
    isModified: Bool = false,
    lastOpenedISO8601: String = OfficeDocumentModel.currentISO8601Timestamp(),
    fileURL: URL? = nil,
    payload: OpenTabPayload
  ) {
    self.id = id
    self.fileName = fileName
    self.format = format
    self.isPinned = isPinned
    self.isFavorite = isFavorite
    self.isModified = isModified
    self.lastOpenedISO8601 = lastOpenedISO8601
    self.fileURL = fileURL
    self.payload = payload
  }

  public var moduleKind: OfficeModuleKind { format.moduleKind }

  public func searchableCorpus() -> String {
    var chunks: [String] = [fileName, format.rawValue, format.standardSpecification]
    switch payload {
    case .document(let doc):
      chunks.append(contentsOf: [doc.title, doc.headerText, doc.footerText])
      chunks.append(contentsOf: doc.paragraphs.map(\.text))
      chunks.append(contentsOf: doc.tables.flatMap { $0.rows.flatMap { $0 } })
      chunks.append(contentsOf: doc.comments.map(\.message))
    case .sheet(let sheet):
      chunks.append(sheet.name)
      for (coord, raw) in sheet.cellsByCoordinate {
        let addr = CellAddress(packed: coord).a1Notation
        chunks.append(contentsOf: ["\(addr)=\(raw)", raw, sheet.evaluate(at: coord).description])
      }
      for page in sheet.additionalSheets {
        chunks.append(page.name)
        for (coord, raw) in page.cellsByCoordinate {
          let addr = CellAddress(packed: coord).a1Notation
          chunks.append(contentsOf: ["\(addr)=\(raw)", raw])
        }
      }
    case .slide(let deck):
      chunks.append(deck.title)
      for slide in deck.slides {
        chunks.append(contentsOf: [slide.title, slide.subtitle, slide.speakerNotes])
        chunks.append(contentsOf: slide.bulletPoints)
      }
    case .pdf(let pdfDoc):
      chunks.append(pdfDoc.title)
      chunks.append(contentsOf: pdfDoc.pages.map(\.extractedText))
      chunks.append(contentsOf: pdfDoc.annotations.map(\.content))
    }
    return chunks.joined(separator: " ")
  }
}

/// Central workspace catalog managing built-in templates, sample tabs, and unified full-text search.
public struct WorkspaceHubCatalog: Sendable {
  public static let builtInTemplates: [OfficeTemplateDescriptor] = [
    OfficeTemplateDescriptor(
      id: "tpl-doc-proposal", title: "제품 기획서 및 프로젝트 보고서", subtitle: "목차·표·머리말 스타일이 구성된 표준 워드 문서",
      format: .docx),
    OfficeTemplateDescriptor(
      id: "tpl-hwp-official", title: "기안문 및 행정 공문서 양식 (HWPX)",
      subtitle: "결재란·두문·본문·협조란 표준 규격 개방형 한글 문서", format: .hwpx),
    OfficeTemplateDescriptor(
      id: "tpl-hwp-legacy-form", title: "대외 협조문 및 품의서 양식 (HWP 5.0)",
      subtitle: "OLE2 CFBF 호환 이중선 표·누름틀 포함 한글 양식", format: .hwp),
    OfficeTemplateDescriptor(
      id: "tpl-sheet-finance", title: "예산 집행 및 분기 손익 분석 시트",
      subtitle: "자동 합계·평균·조건부 달성률 수식이 포함된 시트", format: .xlsx),
    OfficeTemplateDescriptor(
      id: "tpl-sheet-ods-kpi", title: "연간 KPI 지표 및 리소스 산출 시트",
      subtitle: "ISO/IEC 26300 ODF 표준 피벗·수식 검증 시트", format: .ods),
    OfficeTemplateDescriptor(
      id: "tpl-slide-pitch", title: "16:9 경영 브리핑 & 제품 발표덱", subtitle: "슬라이드 마스터·비교 레이아웃·발표자 노트 구성",
      format: .pptx),
    OfficeTemplateDescriptor(
      id: "tpl-slide-odp-seminar", title: "기술 세미나 및 아키텍처 리뷰 슬라이드",
      subtitle: "ODF 프레젠테이션 규격 2단 비교 및 다이어그램 덱", format: .odp),
    OfficeTemplateDescriptor(
      id: "tpl-pdf-review", title: "계약서 및 기술 문서 PDF 검토 스튜디오",
      subtitle: "형광펜 하이라이트·스티커 메모·전자서명 워크플로", format: .pdf),
  ]

  public static func instantiateSampleTab(for module: OfficeModuleKind) -> WorkspaceTabItem {
    switch module {
    case .document:
      let p1 = ParagraphBlock(
        style: .title, alignment: .leading, text: "네이티브 macOS 올인원 오피스 설계 보고서", fontSizePt: 20,
        isBold: true)
      let p2 = ParagraphBlock(
        style: .heading1, alignment: .leading, text: "1. 아키텍처 목표 및 핵심 가치", fontSizePt: 14,
        isBold: true)
      let p3 = ParagraphBlock(
        style: .body, alignment: .justified,
        text:
          "단일 멀티 탭 윈도우에서 문서, 시트, 슬라이드, 한글(HWP/HWPX), PDF를 즉각 열람·편집하고 전사 표준 문서 조판 스타일을 일관되게 제공합니다.")
      let doc = OfficeDocumentModel(
        title: "2026 프로젝트 아키텍처 보고서",
        format: .docx,
        paperSize: .a4,
        headerText: "2026 전사 표준 문서 관리 지침",
        footerText: "- 1 -",
        paragraphs: [p1, p2, p3],
        tables: [
          DocumentTableBlock(
            caption: "표 1. 부문별 문서 표준화 및 업무 효율화 핵심 지표",
            rows: [
              ["소관 부문", "적용 표준 양식", "주요 추진 과제", "목표 달성률(%)"],
              ["경영기획실", "DOCX, HWP, HWPX", "전사 보고서 및 기안문 양식 단일화", "98"],
              ["재무회계팀", "XLSX, ODS, CSV", "분기별 예산·결산 수식 자동화", "96"],
              ["법무·보안팀", "PPTX, ODP, PDF", "대외 발송 문서 전자결재 및 보존", "100"],
            ]
          )
        ],
        trackChangesEnabled: true,
        footnotes: [
          FootnoteItem(
            number: 1, anchorParagraphID: p3.id,
            text: "국제 문서 표준(ECMA-376, ISO/IEC 26300, KS X 6101, ISO 32000) 준수.")
        ],
        revisions: [
          TrackedRevision(
            author: "기획조정실", kind: .formattingChange, paragraphID: p2.id,
            originalText: "1. 핵심 목표", proposedText: "1. 아키텍처 목표 및 핵심 가치")
        ],
        comments: [
          DocumentCommentThread(
            anchorParagraphID: p3.id, author: "보안 리뷰어",
            message: "표준 문서 서식 기준이 본문 2절 실행 계획과 일관되게 연결되어야 합니다.", replies: ["반영 완료했습니다."])
        ]
      )
      return WorkspaceTabItem(
        fileName: "아키텍처_보고서.docx", format: .docx, isPinned: true,
        lastOpenedISO8601: "2026-10-17T09:30:00Z", payload: .document(doc))

    case .hwp:
      let pHeader = ParagraphBlock(
        style: .officialHeader, alignment: .center, text: "업 무 협 조 전", fontSizePt: 18, isBold: true)
      let pTitle = ParagraphBlock(
        style: .heading1, alignment: .leading, text: "제목: {{부서명}} 2026년도 표준 문서 포맷 호환성 검증 건",
        fontSizePt: 13, isBold: true)
      let pBody = ParagraphBlock(
        style: .body, alignment: .justified,
        text:
          "1. 귀 부서의 무궁한 발전을 기원합니다.\n2. HWP 5.0(OLE2) 및 HWPX(KS X 6101 OWPML) 양식의 표·누름틀·각주 호환성 검증 결과를 아래와 같이 회신합니다."
      )
      let hwpDoc = OfficeDocumentModel(
        title: "표준 업무 협조전 (공문서)",
        format: .hwpx,
        paperSize: .a4,
        headerText: "문서번호: 기안-2026-0104",
        footerText: "대외비 · 무단 전재 금지",
        paragraphs: [pHeader, pTitle, pBody],
        tables: [
          DocumentTableBlock(
            caption: "결재란 및 검토 내역",
            rows: [["구분", "담당", "검토", "승인"], ["서명", "김지훈", "박서연", "최현우"]], borderStyle: .doubleLine)
        ],
        footnotes: [
          FootnoteItem(number: 1, anchorParagraphID: pBody.id, text: "행정 효율과 협업 촉진에 관한 규정 제7조 참조.")
        ]
      )
      return WorkspaceTabItem(
        fileName: "업무협조전_공문서.hwpx", format: .hwpx, isPinned: true,
        lastOpenedISO8601: "2026-10-17T09:20:00Z", payload: .document(hwpDoc))

    case .sheet:
      var sheet = WorksheetModel(name: "2026 분기별 실적")
      for (addr, val) in [
        ("A1", "항목"), ("B1", "1분기"), ("C1", "2분기"), ("D1", "상반기 합계"),
        ("A2", "엔진 최적화"), ("B2", "120"), ("C2", "180"), ("D2", "=SUM(B2:C2)"),
        ("A3", "포맷 변환"), ("B3", "95"), ("C3", "145"), ("D3", "=SUM(B3:C3)"),
        ("A4", "전체 합계"), ("B4", "=SUM(B2:B3)"), ("C4", "=SUM(C2:C3)"), ("D4", "=SUM(D2:D3)"),
      ] { try? sheet.setCell(addr, rawValue: val) }
      return WorkspaceTabItem(
        fileName: "분기별_실적_분석.xlsx", format: .xlsx, isPinned: true,
        lastOpenedISO8601: "2026-10-17T09:15:00Z", payload: .sheet(sheet))

    case .slide:
      let deck = PresentationDeckModel(
        title: "제품 로드맵 프레젠테이션",
        aspectRatio: .widescreen16x9,
        slides: [
          SlideModel(
            title: "2026 상반기 전략 로드맵",
            subtitle: "전사 문서 워크스페이스 표준화 및 분기별 실행 과제",
            bulletPoints: ["부서 간 표준 양식 단일화 및 결재 리드타임 35% 단축", "분기별 핵심 성과 지표(KPI) 실시간 시트 연동 체계 구축"],
            speakerNotes: "오프닝 슬라이드: 상반기 전사 문서 표준화 목표 및 분기별 KPI 공유",
            masterLayout: .titleSlide,
            elements: [
              SlideCanvasElement(
                title: "1분기 핵심 과제", bodyText: "표준 문서 양식 배포 및 전 부서 파일럿 전환 완료", x: 340, y: 255,
                width: 260, height: 150, kind: .roundedRectangleShape)
            ]
          ),
          SlideModel(
            title: "핵심 실행 모듈 및 거버넌스",
            subtitle: "문서 · 공문서 · 시트 · 프레젠테이션 · PDF 아카이빙",
            bulletPoints: ["표준 컨테이너 무결성 점검 및 부서별 권한 통제", "정밀 수식 집계 자동화 및 보존용 PDF 일괄 변환"],
            speakerNotes: "두 번째 슬라이드: 부서별 실행 모듈 및 보안 거버넌스 설명",
            masterLayout: .twoColumnComparison,
            elements: [
              SlideCanvasElement(
                title: "거버넌스 요약",
                bodyText: "부서별 문서 표준 준수율 및 보존 이관 지표", x: 340,
                y: 255, width: 260,
                height: 150, kind: .rectangleShape)
            ]
          ),
        ]
      )
      return WorkspaceTabItem(
        fileName: "제품_로드맵_발표자료.pptx", format: .pptx, lastOpenedISO8601: "2026-10-17T09:10:00Z",
        payload: .slide(deck))

    case .pdf:
      let pdf = PDFStudioDocumentModel(
        title: "표준 보안 아키텍처 가이드",
        pages: [
          PDFPageDescriptor(
            pageNumber: 1, rotationDegrees: 0,
            extractedText: """
              제1장 컨테이너 보안 원칙
              모든 입력 문서는 확장자와 무관하게 전자서명 해시 및 문서 무결성을 먼저 검증합니다.
              제2절 문서 반출 및 접근 통제 기준
              대외 발송 문서는 부서장 승인 결재 후 표준 전자문서 양식으로 보관하며, 열람 이력과 전자서명 해시를 반기별로 정기 감사합니다.
              점검 항목 | 대상 등급 | 점검 주기 | 소관 부서
              문서 무결성 및 전자서명 해시 | 1등급 (기밀) | 상시 자동 | 정보보안팀
              개인정보 비식별화 마스킹 | 전사 대외문서 | 발송 전 필수 | 법무·컴플라이언스
              보존 연한 및 파기 이력 감사 | 재무·인사 기록 | 분기 1회 | 경영지원본부
              """),
          PDFPageDescriptor(
            pageNumber: 2, rotationDegrees: 0,
            extractedText: "제2장 PDF 양방향 변환\nPDF 페이지의 텍스트 흐름과 표 구조를 분석하여 편집 가능한 문서로 변환합니다."),
        ],
        annotations: [
          PDFAnnotationItem(
            pageNumber: 1, kind: .highlight, content: "반기 보안 감사 필수 점검 항목", author: "정보보안팀")
        ],
        outline: [
          PDFOutlineNode(title: "제1장 컨테이너 보안 원칙", targetPageNumber: 1),
          PDFOutlineNode(title: "제2장 PDF 양방향 변환", targetPageNumber: 2),
        ]
      )
      return WorkspaceTabItem(
        fileName: "보안_아키텍처_가이드.pdf", format: .pdf, lastOpenedISO8601: "2026-10-17T09:05:00Z",
        payload: .pdf(pdf))
    }
  }

  public static func instantiateBlankTab(
    for module: OfficeModuleKind,
    indexSuffix: Int = 1
  ) -> WorkspaceTabItem {
    let safeIndex = max(1, indexSuffix)
    switch module {
    case .document:
      let emptyParagraph = ParagraphBlock(
        style: .body,
        alignment: .leading,
        text: "",
        fontSizePt: 11,
        isBold: false
      )
      let doc = OfficeDocumentModel(
        title: "새 문서 \(safeIndex)",
        format: .docx,
        paperSize: .a4,
        headerText: "",
        footerText: "",
        paragraphs: [emptyParagraph],
        tables: []
      )
      return WorkspaceTabItem(
        fileName: "새_문서_\(safeIndex).docx",
        format: .docx,
        isPinned: false,
        isModified: false,
        payload: .document(doc)
      )

    case .hwp:
      let emptyParagraph = ParagraphBlock(
        style: .body,
        alignment: .justified,
        text: "",
        fontSizePt: 11,
        isBold: false
      )
      let hwpDoc = OfficeDocumentModel(
        title: "새 한글 문서 \(safeIndex)",
        format: .hwpx,
        paperSize: .a4,
        headerText: "문서번호: 기안-2026-000\(safeIndex)",
        footerText: "",
        paragraphs: [emptyParagraph],
        tables: []
      )
      return WorkspaceTabItem(
        fileName: "새_한글문서_\(safeIndex).hwpx",
        format: .hwpx,
        isPinned: false,
        isModified: false,
        payload: .document(hwpDoc)
      )

    case .sheet:
      let sheet = WorksheetModel(name: "Sheet1")
      return WorkspaceTabItem(
        fileName: "새_스프레드시트_\(safeIndex).xlsx",
        format: .xlsx,
        isPinned: false,
        isModified: false,
        payload: .sheet(sheet)
      )

    case .slide:
      let deck = PresentationDeckModel(
        title: "새 프레젠테이션 \(safeIndex)",
        aspectRatio: .widescreen16x9,
        slides: [
          SlideModel(
            title: "새 슬라이드 제목",
            subtitle: "",
            bulletPoints: [],
            speakerNotes: "",
            masterLayout: .titleAndContent,
            elements: []
          )
        ]
      )
      return WorkspaceTabItem(
        fileName: "새_프레젠테이션_\(safeIndex).pptx",
        format: .pptx,
        isPinned: false,
        isModified: false,
        payload: .slide(deck)
      )

    case .pdf:
      let pdf = PDFStudioDocumentModel(
        title: "새 PDF 문서 \(safeIndex)",
        pages: [
          PDFPageDescriptor(
            pageNumber: 1,
            extractedText: ""
          )
        ]
      )
      return WorkspaceTabItem(
        fileName: "새_PDF문서_\(safeIndex).pdf",
        format: .pdf,
        isPinned: false,
        isModified: false,
        payload: .pdf(pdf)
      )
    }
  }

  public static func searchTabs(_ tabs: [WorkspaceTabItem], query: String) -> [WorkspaceTabItem] {
    let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return tabs }
    return tabs.filter { $0.searchableCorpus().localizedCaseInsensitiveContains(trimmed) }
  }

  public static func matchesRecentFormatFilter(_ tab: WorkspaceTabItem, filter: String) -> Bool {
    let normalized = filter.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
    switch normalized {
    case "", "전체", "ALL":
      return true
    case "DOCX":
      return tab.format == .docx || tab.format == .doc || tab.format == .rtf || tab.format == .md
        || tab.format == .txt
    case "HWP", "HWPX", "HWP/HWPX":
      return tab.moduleKind == .hwp
    case "DOCX/HWP":
      return tab.moduleKind == .document || tab.moduleKind == .hwp
    case "XLSX", "XLS", "CSV":
      return tab.format == .xlsx || tab.format == .xls || tab.format == .csv || tab.format == .tsv
    case "PPTX", "PPT":
      return tab.format == .pptx || tab.format == .ppt || tab.format == .ppsx
    case "PDF":
      return tab.moduleKind == .pdf
    case "ODF", "ODT/ODS/ODP":
      return tab.format == .odt || tab.format == .ods || tab.format == .odp
    default:
      return tab.format.rawValue.uppercased() == normalized
    }
  }

  public static func filterRecentTabs(_ tabs: [WorkspaceTabItem], formatFilter: String)
    -> [WorkspaceTabItem]
  {
    tabs.filter { matchesRecentFormatFilter($0, filter: formatFilter) }
  }

  public static func filterTabs(_ tabs: [WorkspaceTabItem], section: HubSection, query: String)
    -> [WorkspaceTabItem]
  {
    let queryFiltered = searchTabs(tabs, query: query)
    switch section {
    case .pinned: return queryFiltered.filter { $0.isPinned || $0.isFavorite }
    case .recent: return queryFiltered.sorted { $0.lastOpenedISO8601 > $1.lastOpenedISO8601 }
    case .home, .templates, .converter, .smartAssistant: return queryFiltered
    }
  }
}

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
