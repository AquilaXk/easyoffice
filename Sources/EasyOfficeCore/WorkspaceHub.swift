import Foundation
import Observation
import EasyOfficeDesignSystem

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
}

extension DataValidationRule {
    public init(minimumValue: Double? = nil, maximumValue: Double? = nil, allowedSet: [String]? = nil) {
        if let allowedSet, !allowedSet.isEmpty {
            self = .allowedList(allowedSet)
        } else if minimumValue != nil || maximumValue != nil {
            self = .numberRange(
                min: minimumValue ?? -Double.greatestFiniteMagnitude,
                max: maximumValue ?? Double.greatestFiniteMagnitude
            )
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
}

/// Built-in or user-saved template descriptor available from the Home Hub.
public struct OfficeTemplateDescriptor: Sendable, Equatable, Identifiable {
    public let id: String
    public let title: String
    public let subtitle: String
    public let format: DocumentFormat
    public let moduleKind: OfficeModuleKind
    public let isCustomUserTemplate: Bool

    public init(id: String, title: String, subtitle: String, format: DocumentFormat, isCustomUserTemplate: Bool = false) {
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
        lastOpenedISO8601: String = ISO8601DateFormatter().string(from: Date()),
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

    /// Extracts all searchable text across document paragraphs, tables, sheet cells, slide notes, or PDF pages.
    public func searchableCorpus() -> String {
        var chunks: [String] = [fileName, format.rawValue, format.standardSpecification]
        switch payload {
        case let .document(doc):
            chunks.append(contentsOf: [doc.title, doc.headerText, doc.footerText])
            chunks.append(contentsOf: doc.paragraphs.map(\.text))
            chunks.append(contentsOf: doc.tables.flatMap { $0.rows.flatMap { $0 } })
            chunks.append(contentsOf: doc.comments.map(\.message))
        case let .sheet(sheet):
            chunks.append(sheet.name)
            for (addr, raw) in sheet.rawCells {
                chunks.append(contentsOf: ["\(addr)=\(raw)", raw, sheet.evaluate(at: addr).description])
            }
        case let .slide(deck):
            chunks.append(deck.title)
            for slide in deck.slides {
                chunks.append(contentsOf: [slide.title, slide.subtitle, slide.speakerNotes])
                chunks.append(contentsOf: slide.bulletPoints)
            }
        case let .pdf(pdfDoc):
            chunks.append(pdfDoc.title)
            chunks.append(contentsOf: pdfDoc.pages.map(\.extractedText))
            chunks.append(contentsOf: pdfDoc.annotations.map(\.content))
        }
        return chunks.joined(separator: " ")
    }
}

/// Central workspace state managing open tabs, templates, and unified full-text search.
public struct WorkspaceHubCatalog: Sendable {
    public static let builtInTemplates: [OfficeTemplateDescriptor] = [
        OfficeTemplateDescriptor(id: "tpl-doc-proposal", title: "제품 기획서 및 프로젝트 보고서", subtitle: "목차·표·머리말 스타일이 구성된 표준 워드 문서", format: .docx),
        OfficeTemplateDescriptor(id: "tpl-hwp-official", title: "기안문 및 행정 공문서 양식 (HWPX)", subtitle: "결재란·두문·본문·협조란 표준 규격 개방형 한글 문서", format: .hwpx),
        OfficeTemplateDescriptor(id: "tpl-hwp-legacy-form", title: "대외 협조문 및 품의서 양식 (HWP 5.0)", subtitle: "OLE2 CFBF 호환 이중선 표·누름틀 포함 한글 양식", format: .hwp),
        OfficeTemplateDescriptor(id: "tpl-sheet-finance", title: "예산 집행 및 분기 손익 분석 시트", subtitle: "자동 합계·평균·조건부 달성률 수식이 포함된 시트", format: .xlsx),
        OfficeTemplateDescriptor(id: "tpl-sheet-ods-kpi", title: "연간 KPI 지표 및 리소스 산출 시트", subtitle: "ISO/IEC 26300 ODF 표준 피벗·수식 검증 시트", format: .ods),
        OfficeTemplateDescriptor(id: "tpl-slide-pitch", title: "16:9 경영 브리핑 & 제품 발표덱", subtitle: "슬라이드 마스터·비교 레이아웃·발표자 노트 구성", format: .pptx),
        OfficeTemplateDescriptor(id: "tpl-slide-odp-seminar", title: "기술 세미나 및 아키텍처 리뷰 슬라이드", subtitle: "ODF 프레젠테이션 규격 2단 비교 및 다이어그램 덱", format: .odp),
        OfficeTemplateDescriptor(id: "tpl-pdf-review", title: "계약서 및 기술 문서 PDF 검토 스튜디오", subtitle: "형광펜 하이라이트·스티커 메모·전자서명 워크플로", format: .pdf)
    ]

    /// Creates a pre-populated sample tab for a given template or module kind.
    public static func instantiateSampleTab(for module: OfficeModuleKind) -> WorkspaceTabItem {
        switch module {
        case .document:
            let p1 = ParagraphBlock(style: .title, alignment: .leading, text: "네이티브 macOS 올인원 오피스 설계 보고서", fontSizePt: 20, isBold: true)
            let p2 = ParagraphBlock(style: .heading1, alignment: .leading, text: "1. 아키텍처 목표 및 핵심 가치", fontSizePt: 14, isBold: true)
            let p3 = ParagraphBlock(style: .body, alignment: .justified, text: "단일 멀티 탭 윈도우에서 문서, 시트, 슬라이드, 한글(HWP/HWPX), PDF를 즉각 열람·편집하고 시그니처 라벤더 디자인 시스템을 일관되게 제공합니다.")
            let doc = OfficeDocumentModel(
                title: "2026 프로젝트 아키텍처 보고서",
                format: .docx,
                paperSize: .a4,
                headerText: "EasyOffice 기술 문서 표준",
                footerText: "- 1 -",
                paragraphs: [p1, p2, p3],
                tables: [
                    DocumentTableBlock(
                        caption: "표 1. 모듈별 지원 규격 및 목표 성능(ms)",
                        rows: [
                            ["모듈", "대표 포맷", "렌더링 엔진", "목표 지연(ms)"],
                            ["Document / HWP", "DOCX, HWP, HWPX, ODT", "Rich Page Layout Engine", "12"],
                            ["Sheet", "XLSX, ODS, CSV", "Virtualized A1 Formula Grid", "8"],
                            ["Slide & PDF", "PPTX, ODP, PDF", "Stage Canvas & Vector PDF Studio", "15"]
                        ]
                    )
                ],
                trackChangesEnabled: true,
                footnotes: [FootnoteItem(number: 1, anchorParagraphID: p3.id, text: "국제 문서 표준(ECMA-376, ISO/IEC 26300, KS X 6101, ISO 32000) 준수.")],
                revisions: [TrackedRevision(author: "QA 아키텍트", kind: .formattingChange, paragraphID: p2.id, originalText: "1. 핵심 목표", proposedText: "1. 아키텍처 목표 및 핵심 가치")],
                comments: [DocumentCommentThread(anchorParagraphID: p3.id, author: "보안 리뷰어", message: "매직 바이트 컨테이너 검증 규칙이 본문 2절에도 연결되어야 합니다.", replies: ["반영 완료했습니다."])]
            )
            return WorkspaceTabItem(fileName: "아키텍처_보고서.docx", format: .docx, isPinned: true, lastOpenedISO8601: "2026-10-17T09:30:00Z", payload: .document(doc))

        case .hwp:
            let pHeader = ParagraphBlock(style: .officialHeader, alignment: .center, text: "업 무 협 조 전", fontSizePt: 18, isBold: true)
            let pTitle = ParagraphBlock(style: .heading1, alignment: .leading, text: "제목: {{부서명}} 2026년도 표준 문서 포맷 호환성 검증 건", fontSizePt: 13, isBold: true)
            let pBody = ParagraphBlock(style: .body, alignment: .justified, text: "1. 귀 부서의 무궁한 발전을 기원합니다.\n2. HWP 5.0(OLE2) 및 HWPX(KS X 6101 OWPML) 양식의 표·누름틀·각주 호환성 검증 결과를 아래와 같이 회신합니다.")
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
                        rows: [["구분", "담당", "검토", "승인"], ["서명", "김지훈", "박서연", "최현우"]],
                        borderStyle: .doubleLine
                    )
                ],
                footnotes: [FootnoteItem(number: 1, anchorParagraphID: pBody.id, text: "행정 효율과 협업 촉진에 관한 규정 제7조 참조.")]
            )
            return WorkspaceTabItem(fileName: "업무협조전_공문서.hwpx", format: .hwpx, isPinned: true, lastOpenedISO8601: "2026-10-17T09:20:00Z", payload: .document(hwpDoc))

        case .sheet:
            var sheet = WorksheetModel(name: "2026 분기별 실적")
            for (addr, val) in [
                ("A1", "항목"), ("B1", "1분기"), ("C1", "2분기"), ("D1", "상반기 합계"),
                ("A2", "엔진 최적화"), ("B2", "120"), ("C2", "180"), ("D2", "=SUM(B2:C2)"),
                ("A3", "포맷 변환"), ("B3", "95"), ("C3", "145"), ("D3", "=SUM(B3:C3)"),
                ("A4", "전체 합계"), ("B4", "=SUM(B2:B3)"), ("C4", "=SUM(C2:C3)"), ("D4", "=SUM(D2:D3)")
            ] {
                try? sheet.setCell(addr, rawValue: val)
            }
            return WorkspaceTabItem(fileName: "분기별_실적_분석.xlsx", format: .xlsx, isPinned: true, lastOpenedISO8601: "2026-10-17T09:15:00Z", payload: .sheet(sheet))

        case .slide:
            let deck = PresentationDeckModel(
                title: "제품 로드맵 프레젠테이션",
                aspectRatio: .widescreen16x9,
                slides: [
                    SlideModel(
                        title: "EasyOffice for macOS",
                        subtitle: "시그니처 라벤더 디자인 시스템과 올인원 문서 워크스페이스",
                        bulletPoints: [
                            "단일 멀티 탭 윈도우에서 5대 오피스 포맷 즉시 편집",
                            "컨텍스트 리본과 우측 인스펙터 결합으로 세로 편집 공간 극대화"
                        ],
                        speakerNotes: "오프닝 슬라이드: macOS 네이티브 구동 성능과 HWPX/OOXML/PDF 통합 강조",
                        masterLayout: .titleSlide,
                        elements: [SlideCanvasElement(title: "핵심 가치 배너", bodyText: "Signature Lavender #5C6BC0", x: 380, y: 220, width: 200, height: 100, kind: .roundedRectangleShape)]
                    ),
                    SlideModel(
                        title: "핵심 모듈 구성",
                        subtitle: "Document · HWP/HWPX · Sheet · Slide · PDF Studio",
                        bulletPoints: [
                            "매직 바이트 기반 Fail-Closed 보안 컨테이너 검증",
                            "결정론적 A1 수식 연산 및 PDF 양방향 변환"
                        ],
                        speakerNotes: "두 번째 슬라이드: 보안 및 포맷 호환성 설명",
                        masterLayout: .twoColumnComparison,
                        elements: [SlideCanvasElement(title: "아키텍처 카드", bodyText: "77종 수식 & 양방향 변환", x: 380, y: 220, width: 200, height: 100, kind: .rectangleShape)]
                    )
                ]
            )
            return WorkspaceTabItem(fileName: "제품_로드맵_발표자료.pptx", format: .pptx, lastOpenedISO8601: "2026-10-17T09:10:00Z", payload: .slide(deck))

        case .pdf:
            let pdf = PDFStudioDocumentModel(
                title: "표준 보안 아키텍처 가이드",
                pages: [
                    PDFPageDescriptor(pageNumber: 1, rotationDegrees: 0, extractedText: "제1장 컨테이너 보안 원칙\n모든 입력 문서는 확장자와 무관하게 매직 바이트 시그니처를 먼저 검증합니다."),
                    PDFPageDescriptor(pageNumber: 2, rotationDegrees: 0, extractedText: "제2장 PDF 양방향 변환\nPDF 페이지의 텍스트 흐름과 표 구조를 분석하여 편집 가능한 문서로 변환합니다.")
                ],
                annotations: [PDFAnnotationItem(pageNumber: 1, kind: .highlight, content: "매직 바이트 시그니처 검증 필수", author: "QA")],
                outline: [
                    PDFOutlineNode(title: "제1장 컨테이너 보안 원칙", targetPageNumber: 1),
                    PDFOutlineNode(title: "제2장 PDF 양방향 변환", targetPageNumber: 2)
                ]
            )
            return WorkspaceTabItem(fileName: "보안_아키텍처_가이드.pdf", format: .pdf, lastOpenedISO8601: "2026-10-17T09:05:00Z", payload: .pdf(pdf))
        }
    }

    /// Performs case-insensitive full-text search across all tabs (file name + body content + formulas + notes).
    public static func searchTabs(_ tabs: [WorkspaceTabItem], query: String) -> [WorkspaceTabItem] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return tabs }
        return tabs.filter { $0.searchableCorpus().localizedCaseInsensitiveContains(trimmed) }
    }

    /// Filters and sorts tabs according to the selected Home Hub section and search query.
    public static func filterTabs(_ tabs: [WorkspaceTabItem], section: HubSection, query: String) -> [WorkspaceTabItem] {
        let queryFiltered = searchTabs(tabs, query: query)
        switch section {
        case .pinned: return queryFiltered.filter { $0.isPinned || $0.isFavorite }
        case .recent: return queryFiltered.sorted { $0.lastOpenedISO8601 > $1.lastOpenedISO8601 }
        case .home, .templates, .converter, .smartAssistant: return queryFiltered
        }
    }
}

/// Unified observable view model coordinating multi-tab editing, split view, ribbon state, undo/redo, file I/O, and document mutations.
@MainActor
@Observable
public final class WorkspaceViewModel {
    private static let maxUndoDepth = 50

    public var openTabs: [WorkspaceTabItem]
    public var activeTabID: UUID? {
        didSet {
            activeInlineEditTargetKey = nil
            if let activeTabID, splitSecondaryTabID == activeTabID {
                splitSecondaryTabID = nil
            }
        }
    }
    public var splitSecondaryTabID: UUID? = nil
    public var isShowingHomeHub: Bool = false
    public var activeHubSection: HubSection = .home
    public var activeRibbonCategory: RibbonCategory = .home
    public var isRibbonCollapsed: Bool = false
    public var themeMode: WorkspaceThemeMode = .lightLavender
    public var customTemplates: [OfficeTemplateDescriptor] = []
    public var showLeftNavigator: Bool = true
    public var showRightInspector: Bool = true
    public var searchQuery: String = ""
    public var zoomPercent: Double = 100.0
    public var selectedSheetCell: String = "D2"
    public var selectedSlideIndex: Int = 0
    public var selectedPDFPageIndex: Int = 0
    public var formulaBarDraft: String = ""
    public var isPresenterModeActive: Bool = false

    // Interactive editing, undo/redo, and studio state
    public var selectedParagraphID: UUID? = nil
    public var paragraphDraftText: String = ""
    public var commentDraftText: String = ""
    public var mailMergeFields: [String: String] = ["부서명": "플랫폼아키텍처실", "수신자": "전사 기술위원회"]
    public var selectedChartKind: SheetChartKind = .column
    public var isHeaderRowFrozen: Bool = false
    public var activeConditionalFormatRule: ConditionalFormatRule? = nil
    public var activeDataValidationRule: DataValidationRule? = nil
    public var isSnapGuidesEnabled: Bool = true
    public var presenterElapsedSeconds: Int = 95
    public var isPresenterTimerRunning: Bool = false
    public var selectedSlideElementID: UUID? = nil
    public var activeSnapGuides: [SnapGuideLine] = []
    public var lastPivotGrandTotal: Double = 0
    public var authorName: String = "표준 문서 작성자"
    public var findQueryText: String = ""
    public var replaceQueryText: String = ""
    public var isFindReplaceBarVisible: Bool = false
    public var lastFindReplaceCount: Int = 0
    public var batchTargetFormat: DocumentFormat = .pdf
    public var lastBatchConversionReport: BatchConversionReport? = nil
    public var lastSecurityAuditReport: SecurityAuditReport? = nil
    public var lastScrubbedMetadataCount: Int = 0
    public var reviewStatsRefreshCount: Int = 0
    public var lastErrorMessage: String? = nil
    public var lastExecutedCommandSummary: String? = nil

    internal var undoStacks: [UUID: [OpenTabPayload]] = [:]
    internal var redoStacks: [UUID: [OpenTabPayload]] = [:]
    internal var customTemplatePayloads: [String: OpenTabPayload] = [:]
    internal var activeInlineEditTargetKey: String? = nil
    public internal(set) var sheetSortAscending: Bool = false

    public init() {
        let initialTabs = OfficeModuleKind.allCases.map { WorkspaceHubCatalog.instantiateSampleTab(for: $0) }
        self.openTabs = initialTabs
        self.activeTabID = initialTabs.first?.id
        self.formulaBarDraft = "=SUM(B2:C2)"
    }

    public var activeTab: WorkspaceTabItem? {
        guard let activeTabID else { return openTabs.first }
        return openTabs.first(where: { $0.id == activeTabID })
    }

    public var splitSecondaryTab: WorkspaceTabItem? {
        guard let splitSecondaryTabID, splitSecondaryTabID != activeTabID else { return nil }
        return openTabs.first(where: { $0.id == splitSecondaryTabID })
    }

    public var filteredTabs: [WorkspaceTabItem] {
        WorkspaceHubCatalog.filterTabs(openTabs, section: activeHubSection, query: searchQuery)
    }

    public var allAvailableTemplates: [OfficeTemplateDescriptor] {
        customTemplates + WorkspaceHubCatalog.builtInTemplates
    }

    // MARK: - Undo / Redo Stack

    public var canUndo: Bool {
        guard let id = activeTab?.id else { return false }
        return !(undoStacks[id]?.isEmpty ?? true)
    }

    public var canRedo: Bool {
        guard let id = activeTab?.id else { return false }
        return !(redoStacks[id]?.isEmpty ?? true)
    }

    internal func recordUndoSnapshot(for tabID: UUID, coalescingTargetKey: String? = nil) {
        guard let idx = openTabs.firstIndex(where: { $0.id == tabID }) else { return }
        if let coalescingTargetKey {
            if activeInlineEditTargetKey == coalescingTargetKey { return }
            activeInlineEditTargetKey = coalescingTargetKey
        } else {
            activeInlineEditTargetKey = nil
        }
        var stack = undoStacks[tabID] ?? []
        stack.append(openTabs[idx].payload)
        if stack.count > Self.maxUndoDepth {
            stack.removeFirst(stack.count - Self.maxUndoDepth)
        }
        undoStacks[tabID] = stack
        redoStacks[tabID] = []
    }

    public func undo() {
        activeInlineEditTargetKey = nil
        guard let tabID = activeTab?.id,
              let idx = openTabs.firstIndex(where: { $0.id == tabID }),
              var stack = undoStacks[tabID],
              let previousPayload = stack.popLast() else { return }
        undoStacks[tabID] = stack
        var rStack = redoStacks[tabID] ?? []
        rStack.append(openTabs[idx].payload)
        redoStacks[tabID] = rStack
        openTabs[idx].payload = previousPayload
        openTabs[idx].isModified = true
        lastExecutedCommandSummary = "실행 취소 완료"
    }

    public func redo() {
        activeInlineEditTargetKey = nil
        guard let tabID = activeTab?.id,
              let idx = openTabs.firstIndex(where: { $0.id == tabID }),
              var rStack = redoStacks[tabID],
              let nextPayload = rStack.popLast() else { return }
        redoStacks[tabID] = rStack
        var uStack = undoStacks[tabID] ?? []
        uStack.append(openTabs[idx].payload)
        undoStacks[tabID] = uStack
        openTabs[idx].payload = nextPayload
        openTabs[idx].isModified = true
        lastExecutedCommandSummary = "다시 실행 완료"
    }

    // MARK: - Tab & Template Management

    public func openNewSampleTab(for module: OfficeModuleKind) {
        activeInlineEditTargetKey = nil
        var newTab = WorkspaceHubCatalog.instantiateSampleTab(for: module)
        newTab.lastOpenedISO8601 = ISO8601DateFormatter().string(from: Date())
        openTabs.append(newTab)
        activeTabID = newTab.id
        isShowingHomeHub = false
    }

    public func openTab(from template: OfficeTemplateDescriptor) {
        activeInlineEditTargetKey = nil
        if let savedPayload = customTemplatePayloads[template.id] {
            let newTab = WorkspaceTabItem(fileName: "\(template.title).\(template.format.rawValue)", format: template.format, payload: savedPayload)
            openTabs.append(newTab)
            activeTabID = newTab.id
            isShowingHomeHub = false
            return
        }

        var sample = WorkspaceHubCatalog.instantiateSampleTab(for: template.moduleKind)
        sample.fileName = "\(template.title).\(template.format.rawValue)"
        sample.format = template.format
        sample.lastOpenedISO8601 = ISO8601DateFormatter().string(from: Date())
        if case var .document(doc) = sample.payload {
            doc.title = template.title
            doc.format = template.format
            sample.payload = .document(doc)
        }
        let instantiated = WorkspaceTabItem(
            fileName: sample.fileName,
            format: sample.format,
            lastOpenedISO8601: sample.lastOpenedISO8601,
            payload: sample.payload
        )
        openTabs.append(instantiated)
        activeTabID = instantiated.id
        isShowingHomeHub = false
    }

    public func closeTab(id: UUID) {
        activeInlineEditTargetKey = nil
        openTabs.removeAll { $0.id == id }
        undoStacks.removeValue(forKey: id)
        redoStacks.removeValue(forKey: id)
        if splitSecondaryTabID == id { splitSecondaryTabID = nil }
        if activeTabID == id { activeTabID = openTabs.first?.id }
        if splitSecondaryTabID == activeTabID { splitSecondaryTabID = nil }
        if openTabs.isEmpty { isShowingHomeHub = true }
    }

    public func togglePinTab(id: UUID) {
        guard let idx = openTabs.firstIndex(where: { $0.id == id }) else { return }
        openTabs[idx].isPinned.toggle()
    }

    public func toggleFavoriteTab(id: UUID) {
        guard let idx = openTabs.firstIndex(where: { $0.id == id }) else { return }
        openTabs[idx].isFavorite.toggle()
    }

    public func duplicateTab(id: UUID) {
        guard let source = openTabs.first(where: { $0.id == id }) else { return }
        let ext = source.format.rawValue
        let baseName = source.fileName.lowercased().hasSuffix(".\(ext)")
            ? String(source.fileName.dropLast(ext.count + 1))
            : source.fileName
        let duplicated = WorkspaceTabItem(
            fileName: "\(baseName)_사본.\(ext)",
            format: source.format,
            isPinned: source.isPinned,
            isFavorite: source.isFavorite,
            isModified: true,
            payload: source.payload
        )
        openTabs.append(duplicated)
        activeTabID = duplicated.id
        isShowingHomeHub = false
    }

    public func enableSplitView(secondaryTabID: UUID?) {
        guard let secondaryTabID,
              openTabs.contains(where: { $0.id == secondaryTabID }),
              secondaryTabID != activeTabID else {
            self.splitSecondaryTabID = nil
            return
        }
        self.splitSecondaryTabID = secondaryTabID
    }

    public func saveActiveTabAsCustomTemplate(title: String, subtitle: String) {
        guard let current = activeTab else { return }
        let descriptor = OfficeTemplateDescriptor(
            id: "custom-\(UUID().uuidString.lowercased())",
            title: title,
            subtitle: subtitle,
            format: current.format,
            isCustomUserTemplate: true
        )
        customTemplatePayloads[descriptor.id] = current.payload
        customTemplates.insert(descriptor, at: 0)
    }

    public func setZoom(_ percent: Double) {
        guard percent.isFinite else { return }
        zoomPercent = min(400.0, max(25.0, percent))
    }

    // MARK: - Real File Open / Export / Save I/O

    @discardableResult
    public func openDocument(data: Data, fileName: String, fileURL: URL? = nil) throws -> WorkspaceTabItem {
        let verified = try FormatDetector.verify(data: data, fileName: fileName)
        let format = verified.format
        let payload: OpenTabPayload

        switch format {
        case .docx:
            var doc = try OOXMLContainerCodec.decodeDocument(from: data); doc.format = format; payload = .document(doc)
        case .odt:
            var doc = try ODFContainerCodec.decodeDocument(from: data); doc.format = format; payload = .document(doc)
        case .hwpx, .hwtx:
            var doc = try HWPXContainerCodec.decodeDocument(from: data); doc.format = format; payload = .document(doc)
        case .hwp, .hwt:
            var doc = try HWP5BinaryCodec.decodeDocument(from: data); doc.format = format; payload = .document(doc)
        case .doc:
            var doc = try LegacyOLE2OfficeCodec.decodeWordBinary(from: data); doc.format = format; payload = .document(doc)
        case .rtf:
            var doc = try RTFCodec.decodeDocument(from: data); doc.format = format; payload = .document(doc)
        case .md, .txt:
            var doc = try CommonMarkCodec.decodeDocument(from: data, title: fileName); doc.format = format; payload = .document(doc)
        case .xlsx:
            payload = .sheet(try OOXMLContainerCodec.decodeWorksheet(from: data))
        case .xls:
            payload = .sheet(try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: data, name: fileName))
        case .ods:
            payload = .sheet(try ODFContainerCodec.decodeWorksheet(from: data))
        case .csv:
            payload = .sheet(try CSVStateMachineCodec.decodeWorksheet(from: data, name: fileName, delimiter: ","))
        case .tsv:
            payload = .sheet(try CSVStateMachineCodec.decodeWorksheet(from: data, name: fileName, delimiter: "\t"))
        case .pptx, .ppsx:
            payload = .slide(try OOXMLContainerCodec.decodePresentation(from: data))
        case .ppt:
            payload = .slide(try LegacyOLE2OfficeCodec.decodePowerPointBinary(from: data))
        case .odp:
            payload = .slide(try ODFContainerCodec.decodePresentation(from: data))
        case .pdf:
            payload = .pdf(try PDFBinarySerializer.decode(from: data))
        }

        let newTab = WorkspaceTabItem(fileName: fileName, format: format, fileURL: fileURL, payload: payload)
        openTabs.append(newTab)
        activeTabID = newTab.id
        isShowingHomeHub = false
        lastErrorMessage = nil
        return newTab
    }

    @discardableResult
    public func loadFromDisk(url: URL) throws -> WorkspaceTabItem {
        let data = try Data(contentsOf: url)
        return try openDocument(data: data, fileName: url.lastPathComponent, fileURL: url)
    }

    private func convertTab(id: UUID, targetFormat: DocumentFormat? = nil) throws -> ConversionJobResult {
        guard let tab = openTabs.first(where: { $0.id == id }) else {
            throw EasyOfficeError.emptyPayload
        }
        let fmt = targetFormat ?? tab.format
        let safeSourceName = tab.fileName.contains(".") ? tab.fileName : "\(tab.fileName).\(tab.format.fileExtension)"
        let request = ConversionJobRequest(sourceFileName: safeSourceName, sourceFormat: tab.format, targetFormat: fmt, payload: tab.payload)
        return try DocumentConversionOrchestrator.convert(request)
    }

    public func exportTabData(id: UUID, targetFormat: DocumentFormat? = nil) throws -> Data {
        try convertTab(id: id, targetFormat: targetFormat).outputBytes
    }

    public func saveTabToDisk(id: UUID, to url: URL, targetFormat: DocumentFormat? = nil) throws {
        guard let idx = openTabs.firstIndex(where: { $0.id == id }) else {
            throw EasyOfficeError.emptyPayload
        }
        let fmt = targetFormat ?? DocumentFormat.from(fileNameOrExtension: url.lastPathComponent) ?? openTabs[idx].format
        let result = try convertTab(id: id, targetFormat: fmt)
        try result.outputBytes.write(to: url, options: .atomic)
        openTabs[idx].payload = result.convertedPayload
        openTabs[idx].isModified = false
        openTabs[idx].fileURL = url
        openTabs[idx].fileName = url.lastPathComponent
        openTabs[idx].format = fmt
        lastErrorMessage = nil
    }

    // MARK: - Interactive Document, Sheet, Slide, and PDF Mutations

    public func applyFormulaBarEdit() {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .sheet(sheet) = openTabs[idx].payload else { return }
        activeInlineEditTargetKey = nil
        if let rule = activeDataValidationRule {
            let trimmed = formulaBarDraft.trimmingCharacters(in: .whitespacesAndNewlines)
            let isValid: Bool
            if trimmed.hasPrefix("=") {
                var probe = sheet
                if (try? probe.setCell(selectedSheetCell, rawValue: formulaBarDraft)) != nil {
                    isValid = rule.validate(probe.evaluate(at: selectedSheetCell))
                } else {
                    isValid = false
                }
            } else {
                isValid = rule.validate(rawValue: formulaBarDraft)
            }
            guard isValid else {
                lastErrorMessage = "데이터 유효성 검사 실패: 입력값 '\(formulaBarDraft)'이(가) 허용 범위를 벗어났습니다."
                return
            }
        }

        let previousPayload = openTabs[idx].payload
        do {
            try sheet.setCell(selectedSheetCell, rawValue: formulaBarDraft)
            var stack = undoStacks[activeTabID] ?? []
            stack.append(previousPayload)
            if stack.count > Self.maxUndoDepth {
                stack.removeFirst(stack.count - Self.maxUndoDepth)
            }
            undoStacks[activeTabID] = stack
            redoStacks[activeTabID] = []
            openTabs[idx].payload = .sheet(sheet)
            openTabs[idx].isModified = true
            lastErrorMessage = nil
        } catch {
            lastErrorMessage = error.localizedDescription
        }
    }

    public func addWorksheetTab(named: String? = nil) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .sheet(sheet) = openTabs[idx].payload else { return }
        recordUndoSnapshot(for: activeTabID)
        let resolvedName = named ?? "Sheet\(sheet.allSheetNames.count + 1)"
        sheet.addSheetPage(
            named: resolvedName,
            initialCells: [PackedCellCoordinate(row: 0, column: 0): "항목", PackedCellCoordinate(row: 0, column: 1): "값"]
        )
        openTabs[idx].payload = .sheet(sheet)
        openTabs[idx].isModified = true
        lastExecutedCommandSummary = "새 시트 탭 추가: \(resolvedName)"
    }

    public func selectWorksheetTab(index: Int) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .sheet(sheet) = openTabs[idx].payload else { return }
        activeInlineEditTargetKey = nil
        sheet.switchActiveSheet(to: index)
        formulaBarDraft = sheet.rawValue(at: selectedSheetCell)
        openTabs[idx].payload = .sheet(sheet)
        lastExecutedCommandSummary = "시트 탭 전환: \(sheet.name)"
    }

    public func appendParagraphToActiveDocument(text: String, style: ParagraphStyleRole = .body) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .document(doc) = openTabs[idx].payload else { return }
        recordUndoSnapshot(for: activeTabID)
        let block = ParagraphBlock(
            style: style,
            alignment: style == .officialHeader ? .center : .justified,
            text: text,
            fontSizePt: style == .title ? 20.0 : (style == .heading1 ? 14.0 : 11.0),
            isBold: style != .body
        )
        doc.paragraphs.append(block)
        selectedParagraphID = block.id
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
    }

    public func updateParagraphText(paragraphID: UUID, newText: String) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .document(doc) = openTabs[idx].payload,
              let pIdx = doc.paragraphs.firstIndex(where: { $0.id == paragraphID }) else { return }
        guard doc.paragraphs[pIdx].text != newText else { return }
        let targetKey = "para-\(activeTabID.uuidString)-\(paragraphID.uuidString)"
        let isFirstStrokeInBurst = (activeInlineEditTargetKey != targetKey)
        recordUndoSnapshot(for: activeTabID, coalescingTargetKey: targetKey)
        if doc.trackChangesEnabled {
            if isFirstStrokeInBurst {
                doc.recordRevision(author: authorName, kind: .formattingChange, paragraphID: paragraphID, proposedText: newText)
            } else if let rIdx = doc.revisions.indices.last, doc.revisions[rIdx].paragraphID == paragraphID {
                doc.revisions[rIdx].proposedText = newText
            }
        }
        doc.paragraphs[pIdx].text = newText
        if !doc.paragraphs[pIdx].inlineRuns.isEmpty {
            var firstRun = doc.paragraphs[pIdx].inlineRuns[0]
            firstRun.text = newText
            doc.paragraphs[pIdx].inlineRuns = [firstRun]
        }
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
    }

    public func updateTableCell(tableID: UUID, row: Int, column: Int, newValue: String) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .document(doc) = openTabs[idx].payload,
              let tIdx = doc.tables.firstIndex(where: { $0.id == tableID }),
              doc.tables[tIdx].rows.indices.contains(row),
              doc.tables[tIdx].rows[row].indices.contains(column) else { return }
        guard doc.tables[tIdx].rows[row][column] != newValue else { return }
        let targetKey = "table-\(activeTabID.uuidString)-\(tableID.uuidString)-\(row)-\(column)"
        recordUndoSnapshot(for: activeTabID, coalescingTargetKey: targetKey)
        doc.tables[tIdx].rows[row][column] = newValue
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
    }

    public func updateTableCell(tableID: UUID, row: Int, col: Int, newValue: String) {
        updateTableCell(tableID: tableID, row: row, column: col, newValue: newValue)
    }

    public func insertEmbeddedImageIntoActiveDocument(
        caption: String = "아키텍처 다이어그램 PNG",
        widthPt: Double = 320,
        heightPt: Double = 180
    ) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .document(doc) = openTabs[idx].payload else { return }
        recordUndoSnapshot(for: activeTabID)
        let pngSignature = Data([
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
            0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
            0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
            0x42, 0x60, 0x82
        ])
        doc.images.append(
            EmbeddedImageAttachment(
                fileName: "image_\(doc.images.count + 1).png",
                caption: caption,
                mimeType: "image/png",
                imageData: pngSignature,
                widthPt: widthPt,
                heightPt: heightPt,
                anchorParagraphID: selectedParagraphID ?? doc.paragraphs.first?.id
            )
        )
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
        lastExecutedCommandSummary = "임베디드 이미지 첨부 완료: \(caption)"
    }

    public func applyInlineRunFormattingToActiveParagraph(
        isBold: Bool = true,
        isItalic: Bool = false,
        isUnderlined: Bool = false,
        textHex: String? = "#5C6BC0",
        foregroundHex: String? = nil
    ) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .document(doc) = openTabs[idx].payload,
              !doc.paragraphs.isEmpty else { return }
        recordUndoSnapshot(for: activeTabID)
        let pIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
        let resolvedColor = foregroundHex ?? textHex
        let baseText = doc.paragraphs[pIdx].text
        let words = baseText.split(separator: " ", maxSplits: 1, omittingEmptySubsequences: false)
        if words.count == 2 {
            doc.paragraphs[pIdx].runs = [
                DocumentTextRun(text: String(words[0]) + " ", isBold: isBold, isItalic: isItalic, isUnderlined: isUnderlined, foregroundHex: resolvedColor),
                DocumentTextRun(text: String(words[1]), isBold: doc.paragraphs[pIdx].isBold, isItalic: doc.paragraphs[pIdx].isItalic, isUnderlined: doc.paragraphs[pIdx].isUnderlined)
            ]
        } else {
            doc.paragraphs[pIdx].runs = [
                DocumentTextRun(text: baseText, isBold: isBold, isItalic: isItalic, isUnderlined: isUnderlined, foregroundHex: resolvedColor)
            ]
        }
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
        lastExecutedCommandSummary = "인라인 텍스트 런 서식 적용 완료"
    }

    public func selectSlideElement(id: UUID?) {
        activeInlineEditTargetKey = nil
        selectedSlideElementID = id
    }

    public func moveSlideElement(slideIndex: Int? = nil, elementID: UUID, deltaX: Double, deltaY: Double) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .slide(deck) = openTabs[idx].payload else { return }
        let targetSlideIdx = slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
        guard deck.slides.indices.contains(targetSlideIdx),
              let eIdx = deck.slides[targetSlideIdx].elements.firstIndex(where: { $0.id == elementID }) else { return }
        recordUndoSnapshot(for: activeTabID)
        let maxW = deck.aspectRatio.widthPoints
        let maxH = deck.aspectRatio.heightPoints
        var elem = deck.slides[targetSlideIdx].elements[eIdx]
        elem.x = max(0.0, min(maxW - elem.width, elem.x + deltaX))
        elem.y = max(0.0, min(maxH - elem.height, elem.y + deltaY))
        deck.slides[targetSlideIdx].elements[eIdx] = elem
        let siblings = deck.slides[targetSlideIdx].elements.enumerated().compactMap { $0 == eIdx ? nil : $1 }
        activeSnapGuides = SnapGuideCalculator.computeGuides(for: elem, siblings: siblings, aspectRatio: deck.aspectRatio, tolerancePoints: 5.0)
        openTabs[idx].payload = .slide(deck)
        openTabs[idx].isModified = true
    }

    public func resizeSlideElement(slideIndex: Int? = nil, elementID: UUID, deltaWidth: Double, deltaHeight: Double) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .slide(deck) = openTabs[idx].payload else { return }
        let targetSlideIdx = slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
        guard deck.slides.indices.contains(targetSlideIdx),
              let eIdx = deck.slides[targetSlideIdx].elements.firstIndex(where: { $0.id == elementID }) else { return }
        recordUndoSnapshot(for: activeTabID)
        let maxW = deck.aspectRatio.widthPoints
        let maxH = deck.aspectRatio.heightPoints
        var elem = deck.slides[targetSlideIdx].elements[eIdx]
        elem.width = max(40.0, min(maxW - elem.x, elem.width + deltaWidth))
        elem.height = max(28.0, min(maxH - elem.y, elem.height + deltaHeight))
        deck.slides[targetSlideIdx].elements[eIdx] = elem
        openTabs[idx].payload = .slide(deck)
        openTabs[idx].isModified = true
    }

    public func updateSlideElementText(slideIndex: Int? = nil, elementID: UUID, title: String? = nil, bodyText: String? = nil) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .slide(deck) = openTabs[idx].payload else { return }
        let targetSlideIdx = slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
        guard deck.slides.indices.contains(targetSlideIdx),
              let eIdx = deck.slides[targetSlideIdx].elements.firstIndex(where: { $0.id == elementID }) else { return }
        let targetKey = "slide-elem-\(activeTabID.uuidString)-\(elementID.uuidString)"
        recordUndoSnapshot(for: activeTabID, coalescingTargetKey: targetKey)
        if let title { deck.slides[targetSlideIdx].elements[eIdx].title = title }
        if let bodyText { deck.slides[targetSlideIdx].elements[eIdx].bodyText = bodyText }
        openTabs[idx].payload = .slide(deck)
        openTabs[idx].isModified = true
    }

    public func addSlideBulletPoint(slideIndex: Int? = nil, text: String) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .slide(deck) = openTabs[idx].payload else { return }
        let targetSlideIdx = slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
        guard deck.slides.indices.contains(targetSlideIdx) else { return }
        recordUndoSnapshot(for: activeTabID)
        deck.slides[targetSlideIdx].bulletPoints.append(text)
        openTabs[idx].payload = .slide(deck)
        openTabs[idx].isModified = true
    }

    public func updateSlideBulletPoint(slideIndex: Int? = nil, bulletIndex: Int, newText: String) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .slide(deck) = openTabs[idx].payload else { return }
        let targetSlideIdx = slideIndex ?? (deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0)
        guard deck.slides.indices.contains(targetSlideIdx),
              deck.slides[targetSlideIdx].bulletPoints.indices.contains(bulletIndex) else { return }
        let targetKey = "slide-bullet-\(activeTabID.uuidString)-\(targetSlideIdx)-\(bulletIndex)"
        recordUndoSnapshot(for: activeTabID, coalescingTargetKey: targetKey)
        deck.slides[targetSlideIdx].bulletPoints[bulletIndex] = newText
        openTabs[idx].payload = .slide(deck)
        openTabs[idx].isModified = true
    }

    public func advancePresenterSlide() {
        guard let activeTab, case let .slide(deck) = activeTab.payload, !deck.slides.isEmpty else { return }
        if selectedSlideIndex + 1 < deck.slides.count { selectedSlideIndex += 1 }
        presenterElapsedSeconds += 15
    }

    public func previousPresenterSlide() {
        guard selectedSlideIndex > 0 else { return }
        selectedSlideIndex -= 1
    }

    public func tickPresenterTimer(seconds: Int = 1) {
        guard seconds > 0 else { return }
        presenterElapsedSeconds += seconds
    }

    public func rotateActivePDFPage(clockwiseDegrees: Int = 90) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .pdf(pdfDoc) = openTabs[idx].payload else { return }
        recordUndoSnapshot(for: activeTabID)
        let targetIndex = pdfDoc.pages.indices.contains(selectedPDFPageIndex) ? selectedPDFPageIndex : 0
        pdfDoc.rotatePage(at: targetIndex, clockwiseDegrees: clockwiseDegrees)
        openTabs[idx].payload = .pdf(pdfDoc)
        openTabs[idx].isModified = true
    }

    public func addPDFAnnotation(kind: PDFAnnotationKind, content: String) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .pdf(pdfDoc) = openTabs[idx].payload else { return }
        recordUndoSnapshot(for: activeTabID)
        let pageNumber = pdfDoc.pages.indices.contains(selectedPDFPageIndex) ? pdfDoc.pages[selectedPDFPageIndex].pageNumber : 1
        pdfDoc.annotations.append(PDFAnnotationItem(pageNumber: pageNumber, kind: kind, content: content, author: authorName))
        openTabs[idx].payload = .pdf(pdfDoc)
        openTabs[idx].isModified = true
    }

    // MARK: - Find & Replace Across Active Tab

    @discardableResult
    public func performFindAndReplace() -> Int {
        let query = findQueryText
        let replacement = replaceQueryText
        guard !query.isEmpty,
              let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }) else {
            lastFindReplaceCount = 0
            return 0
        }

        var count = 0
        func replaceOccurrences(in source: inout String) {
            guard source.contains(query) else { return }
            let components = source.components(separatedBy: query)
            let occurrences = max(0, components.count - 1)
            if occurrences > 0 {
                count += occurrences
                source = components.joined(separator: replacement)
            }
        }

        switch openTabs[idx].payload {
        case var .document(doc):
            let beforeDoc = doc
            for pIdx in doc.paragraphs.indices {
                replaceOccurrences(in: &doc.paragraphs[pIdx].text)
                for rIdx in doc.paragraphs[pIdx].inlineRuns.indices {
                    var runText = doc.paragraphs[pIdx].inlineRuns[rIdx].text
                    if runText.contains(query) {
                        runText = runText.replacingOccurrences(of: query, with: replacement)
                        doc.paragraphs[pIdx].inlineRuns[rIdx].text = runText
                    }
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

        case var .sheet(sheet):
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

        case var .slide(deck):
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
            }
            if deck != beforeDeck {
                recordUndoSnapshot(for: activeTabID)
                openTabs[idx].payload = .slide(deck)
                openTabs[idx].isModified = true
            }

        case var .pdf(pdfDoc):
            let beforePDF = pdfDoc
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
}

