import Foundation
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
            self = .numberRange(min: minimumValue ?? -Double.greatestFiniteMagnitude, max: maximumValue ?? Double.greatestFiniteMagnitude)
        } else {
            self = .nonEmptyText
        }
    }

    public func validate(rawValue: String) -> Bool {
        let trimmed = rawValue.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.hasPrefix("=") { return true }
        let evaluated: EvaluatedCellValue
        if trimmed.isEmpty { evaluated = .empty }
        else if let num = Double(trimmed) { evaluated = .number(num) }
        else if trimmed.caseInsensitiveCompare("TRUE") == .orderedSame { evaluated = .boolean(true) }
        else if trimmed.caseInsensitiveCompare("FALSE") == .orderedSame { evaluated = .boolean(false) }
        else { evaluated = .text(trimmed) }
        return validate(evaluated)
    }
}

extension CellRange {
    public var a1Range: String { start == end ? start.a1Notation : "\(start.a1Notation):\(end.a1Notation)" }
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
    public static func currentISO8601Timestamp() -> String { EasyOfficeCore.currentISO8601Timestamp() }

    public mutating func recordRevision(author: String, summary: String, kind: TrackedRevisionKind = .formattingChange) {
        revisions.append(TrackedRevision(author: author, kind: kind, paragraphID: UUID(), originalText: title, proposedText: summary))
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
        self.id = id; self.title = title; self.subtitle = subtitle
        self.format = format; self.moduleKind = format.moduleKind; self.isCustomUserTemplate = isCustomUserTemplate
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
        self.id = id; self.fileName = fileName; self.format = format
        self.isPinned = isPinned; self.isFavorite = isFavorite; self.isModified = isModified
        self.lastOpenedISO8601 = lastOpenedISO8601; self.fileURL = fileURL; self.payload = payload
    }

    public var moduleKind: OfficeModuleKind { format.moduleKind }

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

/// Central workspace catalog managing built-in templates, sample tabs, and unified full-text search.
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
                tables: [DocumentTableBlock(caption: "결재란 및 검토 내역", rows: [["구분", "담당", "검토", "승인"], ["서명", "김지훈", "박서연", "최현우"]], borderStyle: .doubleLine)],
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
            ] { try? sheet.setCell(addr, rawValue: val) }
            return WorkspaceTabItem(fileName: "분기별_실적_분석.xlsx", format: .xlsx, isPinned: true, lastOpenedISO8601: "2026-10-17T09:15:00Z", payload: .sheet(sheet))

        case .slide:
            let deck = PresentationDeckModel(
                title: "제품 로드맵 프레젠테이션",
                aspectRatio: .widescreen16x9,
                slides: [
                    SlideModel(
                        title: "EasyOffice for macOS",
                        subtitle: "시그니처 라벤더 디자인 시스템과 올인원 문서 워크스페이스",
                        bulletPoints: ["단일 멀티 탭 윈도우에서 5대 오피스 포맷 즉시 편집", "컨텍스트 리본과 우측 인스펙터 결합으로 세로 편집 공간 극대화"],
                        speakerNotes: "오프닝 슬라이드: macOS 네이티브 구동 성능과 HWPX/OOXML/PDF 통합 강조",
                        masterLayout: .titleSlide,
                        elements: [SlideCanvasElement(title: "핵심 가치 배너", bodyText: "Signature Lavender #5C6BC0", x: 380, y: 220, width: 200, height: 100, kind: .roundedRectangleShape)]
                    ),
                    SlideModel(
                        title: "핵심 모듈 구성",
                        subtitle: "Document · HWP/HWPX · Sheet · Slide · PDF Studio",
                        bulletPoints: ["매직 바이트 기반 Fail-Closed 보안 컨테이너 검증", "결정론적 A1 수식 연산 및 PDF 양방향 변환"],
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

    public static func searchTabs(_ tabs: [WorkspaceTabItem], query: String) -> [WorkspaceTabItem] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return tabs }
        return tabs.filter { $0.searchableCorpus().localizedCaseInsensitiveContains(trimmed) }
    }

    public static func filterTabs(_ tabs: [WorkspaceTabItem], section: HubSection, query: String) -> [WorkspaceTabItem] {
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
        self.id = id; self.title = title; self.symbolName = symbolName; self.shortcutHint = shortcutHint
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

    public static func actions(for category: RibbonCategory, module: OfficeModuleKind) -> [RibbonCommandDescriptor] {
        switch category {
        case .home:
            return [
                RibbonCommandDescriptor(id: "\(module.rawValue)-home-bold", title: "굵게", symbolName: "bold", shortcutHint: "⌘B"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-home-italic", title: "기울임", symbolName: "italic", shortcutHint: "⌘I"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-home-underline", title: "밑줄", symbolName: "underline", shortcutHint: "⌘U"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-home-style", title: "스타일 적용", symbolName: "textformat.size", shortcutHint: "⌥⌘S")
            ]
        case .insert:
            return [
                RibbonCommandDescriptor(id: "\(module.rawValue)-insert-table", title: "표 삽입", symbolName: "tablecells", shortcutHint: "⌃⌘T"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-insert-chart", title: "차트 개체", symbolName: "chart.bar.xaxis", shortcutHint: "⌃⌘C"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-insert-shape", title: "도형·다이어그램", symbolName: "square.on.circle", shortcutHint: "⌃⌘S"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-insert-link", title: "책갈피·링크", symbolName: "link", shortcutHint: "⌘K")
            ]
        case .layout:
            return [
                RibbonCommandDescriptor(id: "\(module.rawValue)-layout-paper", title: "용지 규격 (A4/Letter)", symbolName: "doc.viewfinder", shortcutHint: "⇧⌘P"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-layout-orientation", title: "용지 방향 전환", symbolName: "rectangle.portrait.rotate", shortcutHint: "⌥⌘O"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-layout-columns", title: "다단 설정", symbolName: "rectangle.split.3x1", shortcutHint: "⌥⌘M"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-layout-margins", title: "여백 프리셋", symbolName: "arrow.up.left.and.down.right.magnifyingglass", shortcutHint: "⌥⌘G")
            ]
        case .formulasAndData:
            return [
                RibbonCommandDescriptor(id: "\(module.rawValue)-data-autosum", title: "자동 합계 (SUM)", symbolName: "sum", shortcutHint: "⇧⌘T"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-data-fn-browser", title: "77종 함수 마법사", symbolName: "function", shortcutHint: "⇧F3"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-data-sort", title: "오름차순·내림차순 정렬", symbolName: "arrow.up.arrow.down.square", shortcutHint: "⌥⌘R"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-data-validation", title: "데이터 유효성 검사", symbolName: "checkmark.shield", shortcutHint: "⌥⌘V")
            ]
        case .moduleSpecific:
            switch module {
            case .document:
                return [
                    RibbonCommandDescriptor(id: "doc-mod-toc", title: "자동 목차 생성", symbolName: "list.Number", shortcutHint: "⌃⌥T"),
                    RibbonCommandDescriptor(id: "doc-mod-footnote", title: "각주·미주 삽입", symbolName: "text.badge.plus", shortcutHint: "⌥⌘F"),
                    RibbonCommandDescriptor(id: "doc-mod-mailmerge", title: "메일 머지 치환", symbolName: "envelope.badge.person.crop", shortcutHint: "⌃⌘M"),
                    RibbonCommandDescriptor(id: "doc-mod-tableformula", title: "표 셀 수식 계산", symbolName: "equal.square", shortcutHint: "⌃⌘=")
                ]
            case .hwp:
                return [
                    RibbonCommandDescriptor(id: "hwp-mod-header", title: "행정 공문서 두문·결재란", symbolName: "signature", shortcutHint: "⌃⌥H"),
                    RibbonCommandDescriptor(id: "hwp-mod-field", title: "누름틀 필드 삽입", symbolName: "rectangle.and.pencil.and.ellipsis", shortcutHint: "⌃⌘E"),
                    RibbonCommandDescriptor(id: "hwp-mod-border", title: "이중선·대각선 테두리", symbolName: "square.grid.3x3.topleft.filled", shortcutHint: "⌃⌘B"),
                    RibbonCommandDescriptor(id: "hwp-mod-manuscript", title: "200자 원고지 매수 검산", symbolName: "character.book.closed", shortcutHint: "⌃⌘W")
                ]
            case .sheet:
                return [
                    RibbonCommandDescriptor(id: "sheet-mod-pivot", title: "피벗 테이블 요약", symbolName: "tablecells.badge.ellipsis", shortcutHint: "⌥⌘P"),
                    RibbonCommandDescriptor(id: "sheet-mod-chart", title: "7종 차트 시각화", symbolName: "chart.pie.fill", shortcutHint: "⌥F1"),
                    RibbonCommandDescriptor(id: "sheet-mod-condfmt", title: "조건부 서식 규칙", symbolName: "paintbrush.pointed", shortcutHint: "⌃⌘F"),
                    RibbonCommandDescriptor(id: "sheet-mod-freeze", title: "행·열 틀 고정", symbolName: "lock.rectangle.on.rectangle", shortcutHint: "⌃⌘L")
                ]
            case .slide:
                return [
                    RibbonCommandDescriptor(id: "slide-mod-master", title: "5대 마스터 레이아웃", symbolName: "rectangle.3.group", shortcutHint: "⇧⌘M"),
                    RibbonCommandDescriptor(id: "slide-mod-snap", title: "스마트 정렬 가이드", symbolName: "ruler", shortcutHint: "⌘;"),
                    RibbonCommandDescriptor(id: "slide-mod-transition", title: "화면 전환 효과", symbolName: "sparkles.tv", shortcutHint: "⌥⌘T"),
                    RibbonCommandDescriptor(id: "slide-mod-presenter", title: "발표자 도구 시작", symbolName: "play.display", shortcutHint: "⌥⌘P")
                ]
            case .pdf:
                return [
                    RibbonCommandDescriptor(id: "pdf-mod-highlight", title: "형광펜·밑줄 주석", symbolName: "highlighter", shortcutHint: "⌃⌘H"),
                    RibbonCommandDescriptor(id: "pdf-mod-rotate", title: "페이지 90° 회전", symbolName: "rotate.right", shortcutHint: "⌘R"),
                    RibbonCommandDescriptor(id: "pdf-mod-splitmerge", title: "페이지 분할·병합", symbolName: "doc.on.doc", shortcutHint: "⇧⌘S"),
                    RibbonCommandDescriptor(id: "pdf-mod-tooffice", title: "편집 문서(DOCX/HWPX) 변환", symbolName: "arrow.right.doc.on.clipboard", shortcutHint: "⇧⌘E")
                ]
            }
        case .review:
            return [
                RibbonCommandDescriptor(id: "\(module.rawValue)-review-track", title: "변경 내용 추적", symbolName: "pencil.and.outline", shortcutHint: "⇧⌘E"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-review-comment", title: "검토 메모 스레드", symbolName: "text.bubble", shortcutHint: "⌥⌘C"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-review-verify", title: "매직 바이트 보안 검증", symbolName: "lock.shield", shortcutHint: "⌃⌘V"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-review-stats", title: "문서 통계·글자 수", symbolName: "chart.bar.doc.horizontal", shortcutHint: "⇧⌘I")
            ]
        case .view:
            return [
                RibbonCommandDescriptor(id: "\(module.rawValue)-view-split", title: "좌우 스플릿 뷰", symbolName: "rectangle.split.2x1", shortcutHint: "⌃⌘2"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-view-ribbon", title: "리본 접기·펼치기", symbolName: "menubar.rectangle", shortcutHint: "⌥⌘R"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-view-theme", title: "라이트·다크 테마 전환", symbolName: "circle.lefthalf.filled", shortcutHint: "⌃⌥D"),
                RibbonCommandDescriptor(id: "\(module.rawValue)-view-zoom", title: "100% 실제 배율", symbolName: "1.magnifyingglass", shortcutHint: "⌘0")
            ]
        }
    }
}
