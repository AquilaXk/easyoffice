import Foundation
import Observation
import EasyOfficeDesignSystem

extension DocumentFormat {
    public var moduleKind: OfficeModuleKind {
        switch self {
        case .docx, .doc, .odt, .rtf, .md, .txt:
            return .document
        case .hwp, .hwpx, .hwt, .hwtx:
            return .hwp
        case .xlsx, .xls, .ods, .csv, .tsv:
            return .sheet
        case .pptx, .ppt, .ppsx, .odp:
            return .slide
        case .pdf:
            return .pdf
        }
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

    /// Returns context-sensitive ribbon commands for a given category and active office module.
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

/// Built-in or user-saved template descriptor available from the Home Hub.
public struct OfficeTemplateDescriptor: Sendable, Equatable, Identifiable {
    public let id: String
    public let title: String
    public let subtitle: String
    public let format: DocumentFormat
    public let moduleKind: OfficeModuleKind
    public let isCustomUserTemplate: Bool

    public init(
        id: String,
        title: String,
        subtitle: String,
        format: DocumentFormat,
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
            chunks.append(doc.title)
            chunks.append(doc.headerText)
            chunks.append(doc.footerText)
            chunks.append(contentsOf: doc.paragraphs.map(\.text))
            chunks.append(contentsOf: doc.tables.flatMap { $0.rows.flatMap { $0 } })
            chunks.append(contentsOf: doc.comments.map(\.message))
        case let .sheet(sheet):
            chunks.append(sheet.name)
            for (addr, raw) in sheet.rawCells {
                chunks.append("\(addr)=\(raw)")
                chunks.append(raw)
                chunks.append(sheet.evaluate(at: addr).description)
            }
        case let .slide(deck):
            chunks.append(deck.title)
            for slide in deck.slides {
                chunks.append(slide.title)
                chunks.append(slide.subtitle)
                chunks.append(contentsOf: slide.bulletPoints)
                chunks.append(slide.speakerNotes)
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
        OfficeTemplateDescriptor(
            id: "tpl-doc-proposal",
            title: "제품 기획서 및 프로젝트 보고서",
            subtitle: "목차·표·머리말 스타일이 구성된 표준 워드 문서",
            format: .docx
        ),
        OfficeTemplateDescriptor(
            id: "tpl-hwp-official",
            title: "기안문 및 행정 공문서 양식 (HWPX)",
            subtitle: "결재란·두문·본문·협조란 표준 규격 개방형 한글 문서",
            format: .hwpx
        ),
        OfficeTemplateDescriptor(
            id: "tpl-hwp-legacy-form",
            title: "대외 협조문 및 품의서 양식 (HWP 5.0)",
            subtitle: "OLE2 CFBF 호환 이중선 표·누름틀 포함 한글 양식",
            format: .hwp
        ),
        OfficeTemplateDescriptor(
            id: "tpl-sheet-finance",
            title: "예산 집행 및 분기 손익 분석 시트",
            subtitle: "자동 합계·평균·조건부 달성률 수식이 포함된 시트",
            format: .xlsx
        ),
        OfficeTemplateDescriptor(
            id: "tpl-sheet-ods-kpi",
            title: "연간 KPI 지표 및 리소스 산출 시트",
            subtitle: "ISO/IEC 26300 ODF 표준 피벗·수식 검증 시트",
            format: .ods
        ),
        OfficeTemplateDescriptor(
            id: "tpl-slide-pitch",
            title: "16:9 경영 브리핑 & 제품 발표덱",
            subtitle: "슬라이드 마스터·비교 레이아웃·발표자 노트 구성",
            format: .pptx
        ),
        OfficeTemplateDescriptor(
            id: "tpl-slide-odp-seminar",
            title: "기술 세미나 및 아키텍처 리뷰 슬라이드",
            subtitle: "ODF 프레젠테이션 규격 2단 비교 및 다이어그램 덱",
            format: .odp
        ),
        OfficeTemplateDescriptor(
            id: "tpl-pdf-review",
            title: "계약서 및 기술 문서 PDF 검토 스튜디오",
            subtitle: "형광펜 하이라이트·스티커 메모·전자서명 워크플로",
            format: .pdf
        )
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
                footnotes: [
                    FootnoteItem(number: 1, anchorParagraphID: p3.id, text: "국제 문서 표준(ECMA-376, ISO/IEC 26300, KS X 6101, ISO 32000) 준수.")
                ],
                revisions: [
                    TrackedRevision(
                        author: "QA 아키텍트",
                        kind: .formattingChange,
                        paragraphID: p2.id,
                        originalText: "1. 핵심 목표",
                        proposedText: "1. 아키텍처 목표 및 핵심 가치"
                    )
                ],
                comments: [
                    DocumentCommentThread(
                        anchorParagraphID: p3.id,
                        author: "보안 리뷰어",
                        message: "매직 바이트 컨테이너 검증 규칙이 본문 2절에도 연결되어야 합니다.",
                        replies: ["반영 완료했습니다."]
                    )
                ]
            )
            return WorkspaceTabItem(
                fileName: "아키텍처_보고서.docx",
                format: .docx,
                isPinned: true,
                lastOpenedISO8601: "2026-10-17T09:30:00Z",
                payload: .document(doc)
            )

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
                        rows: [
                            ["구분", "담당", "검토", "승인"],
                            ["서명", "김지훈", "박서연", "최현우"]
                        ],
                        borderStyle: .doubleLine
                    )
                ],
                footnotes: [
                    FootnoteItem(number: 1, anchorParagraphID: pBody.id, text: "행정 효율과 협업 촉진에 관한 규정 제7조 참조.")
                ]
            )
            return WorkspaceTabItem(
                fileName: "업무협조전_공문서.hwpx",
                format: .hwpx,
                isPinned: true,
                lastOpenedISO8601: "2026-10-17T09:20:00Z",
                payload: .document(hwpDoc)
            )

        case .sheet:
            var sheet = WorksheetModel(name: "2026 분기별 실적")
            try? sheet.setCell("A1", rawValue: "항목")
            try? sheet.setCell("B1", rawValue: "1분기")
            try? sheet.setCell("C1", rawValue: "2분기")
            try? sheet.setCell("D1", rawValue: "상반기 합계")
            try? sheet.setCell("A2", rawValue: "엔진 최적화")
            try? sheet.setCell("B2", rawValue: "120")
            try? sheet.setCell("C2", rawValue: "180")
            try? sheet.setCell("D2", rawValue: "=SUM(B2:C2)")
            try? sheet.setCell("A3", rawValue: "포맷 변환")
            try? sheet.setCell("B3", rawValue: "95")
            try? sheet.setCell("C3", rawValue: "145")
            try? sheet.setCell("D3", rawValue: "=SUM(B3:C3)")
            try? sheet.setCell("A4", rawValue: "전체 합계")
            try? sheet.setCell("B4", rawValue: "=SUM(B2:B3)")
            try? sheet.setCell("C4", rawValue: "=SUM(C2:C3)")
            try? sheet.setCell("D4", rawValue: "=SUM(D2:D3)")
            return WorkspaceTabItem(
                fileName: "분기별_실적_분석.xlsx",
                format: .xlsx,
                isPinned: true,
                lastOpenedISO8601: "2026-10-17T09:15:00Z",
                payload: .sheet(sheet)
            )

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
                        elements: [
                            SlideCanvasElement(
                                title: "핵심 가치 배너",
                                bodyText: "Signature Lavender #5C6BC0",
                                x: 380,
                                y: 220,
                                width: 200,
                                height: 100,
                                kind: .roundedRectangleShape
                            )
                        ]
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
                        elements: [
                            SlideCanvasElement(
                                title: "아키텍처 카드",
                                bodyText: "77종 수식 & 양방향 변환",
                                x: 380,
                                y: 220,
                                width: 200,
                                height: 100,
                                kind: .rectangleShape
                            )
                        ]
                    )
                ]
            )
            return WorkspaceTabItem(
                fileName: "제품_로드맵_발표자료.pptx",
                format: .pptx,
                lastOpenedISO8601: "2026-10-17T09:10:00Z",
                payload: .slide(deck)
            )

        case .pdf:
            let pdf = PDFStudioDocumentModel(
                title: "표준 보안 아키텍처 가이드",
                pages: [
                    PDFPageDescriptor(
                        pageNumber: 1,
                        rotationDegrees: 0,
                        extractedText: "제1장 컨테이너 보안 원칙\n모든 입력 문서는 확장자와 무관하게 매직 바이트 시그니처를 먼저 검증합니다."
                    ),
                    PDFPageDescriptor(
                        pageNumber: 2,
                        rotationDegrees: 0,
                        extractedText: "제2장 PDF 양방향 변환\nPDF 페이지의 텍스트 흐름과 표 구조를 분석하여 편집 가능한 문서로 변환합니다."
                    )
                ],
                annotations: [
                    PDFAnnotationItem(pageNumber: 1, kind: .highlight, content: "매직 바이트 시그니처 검증 필수", author: "QA")
                ],
                outline: [
                    PDFOutlineNode(title: "제1장 컨테이너 보안 원칙", targetPageNumber: 1),
                    PDFOutlineNode(title: "제2장 PDF 양방향 변환", targetPageNumber: 2)
                ]
            )
            return WorkspaceTabItem(
                fileName: "보안_아키텍처_가이드.pdf",
                format: .pdf,
                lastOpenedISO8601: "2026-10-17T09:05:00Z",
                payload: .pdf(pdf)
            )
        }
    }

    /// Performs case-insensitive full-text search across all tabs (file name + body content + formulas + notes).
    public static func searchTabs(_ tabs: [WorkspaceTabItem], query: String) -> [WorkspaceTabItem] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return tabs }
        return tabs.filter { tab in
            tab.searchableCorpus().localizedCaseInsensitiveContains(trimmed)
        }
    }

    /// Filters and sorts tabs according to the selected Home Hub section and search query.
    public static func filterTabs(
        _ tabs: [WorkspaceTabItem],
        section: HubSection,
        query: String
    ) -> [WorkspaceTabItem] {
        let queryFiltered = searchTabs(tabs, query: query)
        switch section {
        case .pinned:
            return queryFiltered.filter { $0.isPinned || $0.isFavorite }
        case .recent:
            return queryFiltered.sorted { $0.lastOpenedISO8601 > $1.lastOpenedISO8601 }
        case .home, .templates, .converter, .smartAssistant:
            return queryFiltered
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
    public var mailMergeFields: [String: String] = [
        "부서명": "플랫폼아키텍처실",
        "수신자": "전사 기술위원회"
    ]
    public var selectedChartKind: SheetChartKind = .column
    public var isHeaderRowFrozen: Bool = false
    public var activeConditionalFormatRule: ConditionalFormatRule? = nil
    public var activeDataValidationRule: DataValidationRule? = nil
    public var isSnapGuidesEnabled: Bool = true
    public var presenterElapsedSeconds: Int = 95
    public var batchTargetFormat: DocumentFormat = .pdf
    public var lastBatchConversionReport: BatchConversionReport? = nil
    public var lastSecurityAuditReport: SecurityAuditReport? = nil
    public var lastErrorMessage: String? = nil
    public var lastExecutedCommandSummary: String? = nil

    private var undoStacks: [UUID: [OpenTabPayload]] = [:]
    private var redoStacks: [UUID: [OpenTabPayload]] = [:]
    private var customTemplatePayloads: [String: OpenTabPayload] = [:]
    private var sheetSortAscending: Bool = true

    public init() {
        let initialTabs = OfficeModuleKind.allCases.map {
            WorkspaceHubCatalog.instantiateSampleTab(for: $0)
        }
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

    private func recordUndoSnapshot(for tabID: UUID) {
        guard let idx = openTabs.firstIndex(where: { $0.id == tabID }) else { return }
        var stack = undoStacks[tabID] ?? []
        stack.append(openTabs[idx].payload)
        if stack.count > Self.maxUndoDepth {
            stack.removeFirst(stack.count - Self.maxUndoDepth)
        }
        undoStacks[tabID] = stack
        redoStacks[tabID] = []
    }

    public func undo() {
        guard let tabID = activeTab?.id,
              let idx = openTabs.firstIndex(where: { $0.id == tabID }),
              var stack = undoStacks[tabID],
              let previousPayload = stack.popLast() else {
            return
        }
        undoStacks[tabID] = stack
        var rStack = redoStacks[tabID] ?? []
        rStack.append(openTabs[idx].payload)
        redoStacks[tabID] = rStack
        openTabs[idx].payload = previousPayload
        openTabs[idx].isModified = true
        lastExecutedCommandSummary = "실행 취소 완료"
    }

    public func redo() {
        guard let tabID = activeTab?.id,
              let idx = openTabs.firstIndex(where: { $0.id == tabID }),
              var rStack = redoStacks[tabID],
              let nextPayload = rStack.popLast() else {
            return
        }
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
        var newTab = WorkspaceHubCatalog.instantiateSampleTab(for: module)
        newTab.lastOpenedISO8601 = ISO8601DateFormatter().string(from: Date())
        openTabs.append(newTab)
        activeTabID = newTab.id
        isShowingHomeHub = false
    }

    public func openTab(from template: OfficeTemplateDescriptor) {
        if let savedPayload = customTemplatePayloads[template.id] {
            let newTab = WorkspaceTabItem(
                fileName: "\(template.title).\(template.format.rawValue)",
                format: template.format,
                payload: savedPayload
            )
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
        openTabs.removeAll { $0.id == id }
        undoStacks.removeValue(forKey: id)
        redoStacks.removeValue(forKey: id)
        if splitSecondaryTabID == id {
            splitSecondaryTabID = nil
        }
        if activeTabID == id {
            activeTabID = openTabs.first?.id
        }
        if splitSecondaryTabID == activeTabID {
            splitSecondaryTabID = nil
        }
        if openTabs.isEmpty {
            isShowingHomeHub = true
        }
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
        let baseName: String
        if source.fileName.lowercased().hasSuffix(".\(ext)") {
            baseName = String(source.fileName.dropLast(ext.count + 1))
        } else {
            baseName = source.fileName
        }
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
            var doc = try OOXMLContainerCodec.decodeDocument(from: data)
            doc.format = format
            payload = .document(doc)
        case .odt:
            var doc = try ODFContainerCodec.decodeDocument(from: data)
            doc.format = format
            payload = .document(doc)
        case .hwpx, .hwtx:
            var doc = try HWPXContainerCodec.decodeDocument(from: data)
            doc.format = format
            payload = .document(doc)
        case .hwp, .hwt:
            var doc = try HWP5BinaryCodec.decodeDocument(from: data)
            doc.format = format
            payload = .document(doc)
        case .doc:
            var doc = try LegacyOLE2OfficeCodec.decodeWordBinary(from: data)
            doc.format = format
            payload = .document(doc)
        case .rtf:
            var doc = try RTFCodec.decodeDocument(from: data)
            doc.format = format
            payload = .document(doc)
        case .md, .txt:
            var doc = try CommonMarkCodec.decodeDocument(from: data, title: fileName)
            doc.format = format
            payload = .document(doc)
        case .xlsx:
            let sheet = try OOXMLContainerCodec.decodeWorksheet(from: data)
            payload = .sheet(sheet)
        case .xls:
            let sheet = try LegacyOLE2OfficeCodec.decodeBIFF8Worksheet(from: data, name: fileName)
            payload = .sheet(sheet)
        case .ods:
            let sheet = try ODFContainerCodec.decodeWorksheet(from: data)
            payload = .sheet(sheet)
        case .csv:
            let sheet = try CSVStateMachineCodec.decodeWorksheet(from: data, name: fileName, delimiter: ",")
            payload = .sheet(sheet)
        case .tsv:
            let sheet = try CSVStateMachineCodec.decodeWorksheet(from: data, name: fileName, delimiter: "\t")
            payload = .sheet(sheet)
        case .pptx, .ppsx:
            let deck = try OOXMLContainerCodec.decodePresentation(from: data)
            payload = .slide(deck)
        case .ppt:
            let deck = try LegacyOLE2OfficeCodec.decodePowerPointBinary(from: data)
            payload = .slide(deck)
        case .odp:
            let deck = try ODFContainerCodec.decodePresentation(from: data)
            payload = .slide(deck)
        case .pdf:
            let pdfDoc = try PDFBinarySerializer.decode(from: data)
            payload = .pdf(pdfDoc)
        }

        let newTab = WorkspaceTabItem(
            fileName: fileName,
            format: format,
            fileURL: fileURL,
            payload: payload
        )
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

    public func exportTabData(id: UUID, targetFormat: DocumentFormat? = nil) throws -> Data {
        guard let tab = openTabs.first(where: { $0.id == id }) else {
            throw EasyOfficeError.emptyPayload
        }
        let fmt = targetFormat ?? tab.format
        let safeSourceName = tab.fileName.contains(".") ? tab.fileName : "\(tab.fileName).\(tab.format.fileExtension)"
        let request = ConversionJobRequest(
            sourceFileName: safeSourceName,
            sourceFormat: tab.format,
            targetFormat: fmt,
            payload: tab.payload
        )
        return try DocumentConversionOrchestrator.convert(request).outputBytes
    }

    public func saveTabToDisk(id: UUID, to url: URL, targetFormat: DocumentFormat? = nil) throws {
        guard let idx = openTabs.firstIndex(where: { $0.id == id }) else {
            throw EasyOfficeError.emptyPayload
        }
        let fmt = targetFormat ?? DocumentFormat.from(fileNameOrExtension: url.lastPathComponent) ?? openTabs[idx].format
        let data = try exportTabData(id: id, targetFormat: fmt)
        try data.write(to: url, options: .atomic)
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
              case var .sheet(sheet) = openTabs[idx].payload else {
            return
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

    public func appendParagraphToActiveDocument(text: String, style: ParagraphStyleRole = .body) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .document(doc) = openTabs[idx].payload else {
            return
        }
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
              let pIdx = doc.paragraphs.firstIndex(where: { $0.id == paragraphID }) else {
            return
        }
        guard doc.paragraphs[pIdx].text != newText else { return }
        recordUndoSnapshot(for: activeTabID)
        if doc.trackChangesEnabled {
            doc.recordRevision(
                author: "QA",
                kind: .formattingChange,
                paragraphID: paragraphID,
                proposedText: newText
            )
        }
        doc.paragraphs[pIdx].text = newText
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
    }

    public func updateTableCell(tableID: UUID, row: Int, col: Int, newValue: String) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .document(doc) = openTabs[idx].payload,
              let tIdx = doc.tables.firstIndex(where: { $0.id == tableID }),
              doc.tables[tIdx].rows.indices.contains(row),
              doc.tables[tIdx].rows[row].indices.contains(col) else {
            return
        }
        guard doc.tables[tIdx].rows[row][col] != newValue else { return }
        recordUndoSnapshot(for: activeTabID)
        doc.tables[tIdx].rows[row][col] = newValue
        openTabs[idx].payload = .document(doc)
        openTabs[idx].isModified = true
    }

    public func moveSlideElement(slideIndex: Int, elementID: UUID, deltaX: Double, deltaY: Double) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .slide(deck) = openTabs[idx].payload,
              deck.slides.indices.contains(slideIndex),
              let eIdx = deck.slides[slideIndex].elements.firstIndex(where: { $0.id == elementID }) else {
            return
        }
        recordUndoSnapshot(for: activeTabID)
        let maxW = deck.aspectRatio.widthPoints
        let maxH = deck.aspectRatio.heightPoints
        var elem = deck.slides[slideIndex].elements[eIdx]
        elem.x = max(0.0, min(maxW - elem.width, elem.x + deltaX))
        elem.y = max(0.0, min(maxH - elem.height, elem.y + deltaY))
        deck.slides[slideIndex].elements[eIdx] = elem
        openTabs[idx].payload = .slide(deck)
        openTabs[idx].isModified = true
    }

    public func advancePresenterSlide() {
        guard let activeTab,
              case let .slide(deck) = activeTab.payload,
              !deck.slides.isEmpty else { return }
        if selectedSlideIndex + 1 < deck.slides.count {
            selectedSlideIndex += 1
        }
        presenterElapsedSeconds += 15
    }

    public func previousPresenterSlide() {
        guard selectedSlideIndex > 0 else { return }
        selectedSlideIndex -= 1
    }

    public func rotateActivePDFPage(clockwiseDegrees: Int = 90) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .pdf(pdfDoc) = openTabs[idx].payload else {
            return
        }
        recordUndoSnapshot(for: activeTabID)
        let targetIndex = pdfDoc.pages.indices.contains(selectedPDFPageIndex) ? selectedPDFPageIndex : 0
        pdfDoc.rotatePage(at: targetIndex, clockwiseDegrees: clockwiseDegrees)
        openTabs[idx].payload = .pdf(pdfDoc)
        openTabs[idx].isModified = true
    }

    public func addPDFAnnotation(kind: PDFAnnotationKind, content: String) {
        guard let activeTabID,
              let idx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case var .pdf(pdfDoc) = openTabs[idx].payload else {
            return
        }
        recordUndoSnapshot(for: activeTabID)
        let pageNumber = pdfDoc.pages.indices.contains(selectedPDFPageIndex)
            ? pdfDoc.pages[selectedPDFPageIndex].pageNumber
            : 1
        let item = PDFAnnotationItem(
            pageNumber: pageNumber,
            kind: kind,
            content: content,
            author: "QA"
        )
        pdfDoc.annotations.append(item)
        openTabs[idx].payload = .pdf(pdfDoc)
        openTabs[idx].isModified = true
    }

    // MARK: - Batch Conversion & Security Audit Integration

    public func runBatchConversionForOpenTabs() {
        let jobs = openTabs.map { tab in
            ConversionJobRequest(
                sourceFileName: tab.fileName,
                sourceFormat: tab.format,
                targetFormat: batchTargetFormat,
                payload: tab.payload
            )
        }
        lastBatchConversionReport = DocumentConversionOrchestrator.executeBatch(jobs)
    }

    public func runSecurityAuditOnActiveTab() {
        guard let current = activeTab else { return }
        if let bytes = try? exportTabData(id: current.id, targetFormat: current.format) {
            lastSecurityAuditReport = DocumentSecurityAuditor.auditContainer(data: bytes, fileName: current.fileName)
        }
    }

    // MARK: - Complete 140-Action Context Ribbon Dispatcher

    public func executeRibbonCommand(_ commandID: String) {
        // 1. View category commands
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
            setZoom(100.0)
            lastExecutedCommandSummary = "배율 100% 초기화 완료"
            return
        }

        guard let tabID = activeTab?.id,
              let idx = openTabs.firstIndex(where: { $0.id == tabID }) else {
            lastExecutedCommandSummary = "활성 탭 없음"
            return
        }

        // 2. Home formatting commands
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

        // 3. Insert commands
        if commandID.hasSuffix("-insert-table") {
            recordUndoSnapshot(for: tabID)
            applyInsertCommand(at: idx, kind: .table)
            lastExecutedCommandSummary = "표 개체 삽입 완료"
            return
        }
        if commandID.hasSuffix("-insert-chart") {
            recordUndoSnapshot(for: tabID)
            applyInsertCommand(at: idx, kind: .chart)
            lastExecutedCommandSummary = "차트 개체 삽입 완료"
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

        // 4. Layout commands
        if commandID.hasSuffix("-layout-paper") || commandID.hasSuffix("-layout-size") {
            recordUndoSnapshot(for: tabID)
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
            recordUndoSnapshot(for: tabID)
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

        // 5. Formulas & Data commands
        if commandID.hasSuffix("-data-autosum") || commandID.hasSuffix("-data-sum") {
            recordUndoSnapshot(for: tabID)
            applyDataCommand(at: idx, kind: .autoSum)
            lastExecutedCommandSummary = "자동 합계(SUM) 수식 계산 완료"
            return
        }
        if commandID.hasSuffix("-data-fn-browser") || commandID.hasSuffix("-data-fx") {
            recordUndoSnapshot(for: tabID)
            applyDataCommand(at: idx, kind: .functionWizard)
            lastExecutedCommandSummary = "77종 함수 마법사(AVERAGE) 수식 적용 완료"
            return
        }
        if commandID.hasSuffix("-data-sort") {
            recordUndoSnapshot(for: tabID)
            applyDataCommand(at: idx, kind: .sort)
            lastExecutedCommandSummary = "데이터 행 정렬 완료"
            return
        }
        if commandID.hasSuffix("-data-validation") || commandID.hasSuffix("-data-validate") {
            recordUndoSnapshot(for: tabID)
            applyDataCommand(at: idx, kind: .validation)
            lastExecutedCommandSummary = "데이터 유효성 검사 규칙 활성화 완료"
            return
        }

        // 6. Module-specific commands
        switch commandID {
        case "doc-mod-toc":
            recordUndoSnapshot(for: tabID)
            if case var .document(doc) = openTabs[idx].payload {
                let toc = doc.generateTableOfContents()
                let summaryText = toc.isEmpty
                    ? "[자동 목차] 등록된 제목 스타일 없음"
                    : "[자동 목차] " + toc.map(\.title).joined(separator: " · ")
                doc.paragraphs.insert(
                    ParagraphBlock(style: .quote, alignment: .leading, text: summaryText, isBold: true),
                    at: 0
                )
                openTabs[idx].payload = .document(doc)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "자동 목차(TOC) 생성 완료"
            return

        case "doc-mod-footnote":
            recordUndoSnapshot(for: tabID)
            if case var .document(doc) = openTabs[idx].payload {
                let anchorID = doc.paragraphs.last?.id ?? UUID()
                let nextNum = doc.footnotes.count + 1
                doc.footnotes.append(
                    FootnoteItem(number: nextNum, anchorParagraphID: anchorID, text: "각주 \(nextNum): 표준 규격 참조 설명")
                )
                openTabs[idx].payload = .document(doc)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "각주·미주 삽입 완료"
            return

        case "doc-mod-mailmerge", "doc-mod-merge":
            recordUndoSnapshot(for: tabID)
            if case let .document(doc) = openTabs[idx].payload {
                let merged = doc.applyingMailMerge(record: mailMergeFields)
                openTabs[idx].payload = .document(merged)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "메일 머지 필드 일괄 치환 완료"
            return

        case "doc-mod-tableformula", "doc-mod-header":
            recordUndoSnapshot(for: tabID)
            if case var .document(doc) = openTabs[idx].payload {
                if !doc.tables.isEmpty {
                    let lastCol = max(0, doc.tables[0].columnCount - 1)
                    if let val = doc.tables[0].evaluateTableFormula("=SUM(ABOVE)", atRow: doc.tables[0].rowCount, column: lastCol) {
                        var summaryRow = Array(repeating: "", count: max(1, doc.tables[0].columnCount))
                        summaryRow[0] = "합계 (=SUM(ABOVE))"
                        summaryRow[lastCol] = String(Int(val))
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
            if case var .document(doc) = openTabs[idx].payload {
                doc.headerText = "행정기관 표준 공문서 · 문서번호 기안-2026-0201"
                let banner = ParagraphBlock(
                    style: .officialHeader,
                    alignment: .center,
                    text: "행 정 공 문 서 두 문 · 결 재 란",
                    fontSizePt: 18,
                    isBold: true
                )
                doc.paragraphs.insert(banner, at: 0)
                openTabs[idx].payload = .document(doc)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "행정 공문서 두문·결재란 삽입 완료"
            return

        case "hwp-mod-field":
            recordUndoSnapshot(for: tabID)
            if case var .document(doc) = openTabs[idx].payload {
                let fieldPara = ParagraphBlock(
                    style: .body,
                    alignment: .justified,
                    text: "누름틀 입력란: {{부서명}} / {{수신자}}"
                )
                doc.paragraphs.append(fieldPara)
                openTabs[idx].payload = .document(doc)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "한글 누름틀 필드 삽입 완료"
            return

        case "hwp-mod-border":
            recordUndoSnapshot(for: tabID)
            if case var .document(doc) = openTabs[idx].payload {
                if doc.tables.isEmpty {
                    doc.tables.append(
                        DocumentTableBlock(
                            caption: "결재란 표",
                            rows: [["담당", "검토", "승인"], ["결재", "결재", "결재"]],
                            borderStyle: .doubleLine
                        )
                    )
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
            if case var .document(doc) = openTabs[idx].payload {
                let stats = doc.computeStatistics()
                doc.footerText = "200자 원고지 환산: \(stats.manuscriptPages200)매 (\(stats.characterCountWithSpaces)자)"
                openTabs[idx].payload = .document(doc)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "200자 원고지 매수 검산 반영 완료"
            return

        case "sheet-mod-pivot":
            recordUndoSnapshot(for: tabID)
            if case var .sheet(sheet) = openTabs[idx].payload,
               let range = try? CellRange(notation: "A1:D4") {
                let summary = PivotTableEngine.compute(
                    from: sheet,
                    range: range,
                    hasHeaderRow: true,
                    configuration: PivotTableConfiguration(rowFieldColumnIndex: 0, valueFieldColumnIndex: 3, aggregation: .sum)
                )
                try? sheet.setCell("E1", rawValue: "피벗 총계")
                try? sheet.setCell("E2", rawValue: String(format: "%.0f", summary.grandTotal))
                openTabs[idx].payload = .sheet(sheet)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "피벗 테이블 요약 집계 완료"
            return

        case "sheet-mod-chart":
            let allKinds = SheetChartKind.allCases
            if let currentIdx = allKinds.firstIndex(of: selectedChartKind) {
                selectedChartKind = allKinds[(currentIdx + 1) % allKinds.count]
            }
            lastExecutedCommandSummary = "차트 시각화 유형 전환: \(selectedChartKind.rawValue)"
            return

        case "sheet-mod-condfmt", "sheet-mod-cond":
            activeConditionalFormatRule = .greaterThan(100.0)
            lastExecutedCommandSummary = "조건부 서식(>100 강조) 규칙 적용 완료"
            return

        case "sheet-mod-freeze":
            isHeaderRowFrozen.toggle()
            lastExecutedCommandSummary = isHeaderRowFrozen ? "행·열 틀 고정 활성화" : "행·열 틀 고정 해제"
            return

        case "slide-mod-master":
            recordUndoSnapshot(for: tabID)
            if case var .slide(deck) = openTabs[idx].payload, !deck.slides.isEmpty {
                let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
                let layouts = SlideMasterLayout.allCases
                let cur = deck.slides[sIdx].masterLayout
                let nextIdx = ((layouts.firstIndex(of: cur) ?? 0) + 1) % layouts.count
                deck.slides[sIdx].masterLayout = layouts[nextIdx]
                openTabs[idx].payload = .slide(deck)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "슬라이드 마스터 레이아웃 순환 적용 완료"
            return

        case "slide-mod-snap":
            recordUndoSnapshot(for: tabID)
            isSnapGuidesEnabled = true
            if case var .slide(deck) = openTabs[idx].payload, !deck.slides.isEmpty {
                let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
                if !deck.slides[sIdx].elements.isEmpty {
                    let stageW = deck.aspectRatio.widthPoints
                    let stageH = deck.aspectRatio.heightPoints
                    let elemW = deck.slides[sIdx].elements[0].width
                    let elemH = deck.slides[sIdx].elements[0].height
                    deck.slides[sIdx].elements[0].x = (stageW - elemW) / 2.0
                    deck.slides[sIdx].elements[0].y = (stageH - elemH) / 2.0
                }
                openTabs[idx].payload = .slide(deck)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "스마트 정렬 가이드 정중앙 스냅 완료"
            return

        case "slide-mod-transition":
            recordUndoSnapshot(for: tabID)
            if case var .slide(deck) = openTabs[idx].payload, !deck.slides.isEmpty {
                let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
                let transitions = SlideTransitionKind.allCases
                let cur = deck.slides[sIdx].transition
                let nextIdx = ((transitions.firstIndex(of: cur) ?? 0) + 1) % transitions.count
                deck.slides[sIdx].transition = transitions[nextIdx]
                openTabs[idx].payload = .slide(deck)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "슬라이드 화면 전환 효과 변경 완료"
            return

        case "slide-mod-presenter":
            isPresenterModeActive.toggle()
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
            if case var .pdf(pdfDoc) = openTabs[idx].payload {
                if pdfDoc.pages.count > 1 {
                    let extracted = pdfDoc.extractPages(oneBasedPageNumbers: [1], newTitle: "\(pdfDoc.title) (1페이지 추출)")
                    let splitTab = WorkspaceTabItem(
                        fileName: "분할_\(openTabs[idx].fileName)",
                        format: .pdf,
                        isModified: true,
                        payload: .pdf(extracted)
                    )
                    openTabs.append(splitTab)
                } else {
                    pdfDoc.insertBlankPage(at: pdfDoc.pages.count)
                    openTabs[idx].payload = .pdf(pdfDoc)
                    openTabs[idx].isModified = true
                }
            }
            lastExecutedCommandSummary = "PDF 페이지 분할·병합 실행 완료"
            return

        case "pdf-mod-tooffice", "pdf-mod-convert":
            if case let .pdf(pdfDoc) = openTabs[idx].payload {
                let editable = pdfDoc.convertToEditableDocument(targetFormat: .docx)
                let convertedTab = WorkspaceTabItem(
                    fileName: "\(pdfDoc.title)_변환본.docx",
                    format: .docx,
                    isModified: true,
                    payload: .document(editable)
                )
                openTabs.append(convertedTab)
                activeTabID = convertedTab.id
            }
            lastExecutedCommandSummary = "PDF → 편집 문서(DOCX) 변환 완료"
            return

        default:
            break
        }

        // 7. Review & Security commands
        if commandID.hasSuffix("-review-track") {
            recordUndoSnapshot(for: tabID)
            if case var .document(doc) = openTabs[idx].payload {
                doc.trackChangesEnabled.toggle()
                openTabs[idx].payload = .document(doc)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "변경 내용 추적 상태 전환 완료"
            return
        }
        if commandID.hasSuffix("-review-comment") {
            recordUndoSnapshot(for: tabID)
            let noteText = commentDraftText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? "검토 의견이 등록되었습니다."
                : commentDraftText
            switch openTabs[idx].payload {
            case var .document(doc):
                let anchorID = doc.paragraphs.first?.id ?? UUID()
                doc.comments.append(
                    DocumentCommentThread(anchorParagraphID: anchorID, author: "QA", message: noteText)
                )
                openTabs[idx].payload = .document(doc)
                openTabs[idx].isModified = true
            case var .pdf(pdfDoc):
                pdfDoc.annotations.append(
                    PDFAnnotationItem(pageNumber: 1, kind: .stickyNote, content: noteText, author: "QA")
                )
                openTabs[idx].payload = .pdf(pdfDoc)
                openTabs[idx].isModified = true
            case var .slide(deck):
                if !deck.slides.isEmpty {
                    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
                    deck.slides[sIdx].speakerNotes += "\n[검토 메모] \(noteText)"
                    openTabs[idx].payload = .slide(deck)
                    openTabs[idx].isModified = true
                }
            case var .sheet(sheet):
                try? sheet.setCell("F1", rawValue: "[검토] \(noteText)")
                openTabs[idx].payload = .sheet(sheet)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "검토 메모 스레드 등록 완료"
            return
        }
        if commandID.hasSuffix("-review-verify") || commandID.hasSuffix("-review-security") {
            runSecurityAuditOnActiveTab()
            lastExecutedCommandSummary = "매직 바이트 및 컨테이너 보안 검증 완료"
            return
        }
        if commandID.hasSuffix("-review-stats") {
            showRightInspector = true
            lastExecutedCommandSummary = "문서 통계 및 원고지 매수 인스펙터 갱신 완료"
            return
        }

        lastExecutedCommandSummary = "명령 실행: \(commandID)"
    }

    // MARK: - Private Command Helpers

    private enum HomeFormattingAction {
        case bold
        case italic
        case underline
        case styleCycle
    }

    private func applyHomeFormatting(at idx: Int, action: HomeFormattingAction) {
        switch openTabs[idx].payload {
        case var .document(doc):
            if doc.paragraphs.isEmpty {
                doc.paragraphs.append(ParagraphBlock(text: "기본 본문 문단"))
            }
            let pIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
            switch action {
            case .bold:
                doc.paragraphs[pIdx].isBold.toggle()
            case .italic:
                doc.paragraphs[pIdx].isItalic.toggle()
            case .underline:
                doc.paragraphs[pIdx].isUnderlined.toggle()
            case .styleCycle:
                let allStyles = ParagraphStyleRole.allCases
                let cur = doc.paragraphs[pIdx].style
                let nextIdx = ((allStyles.firstIndex(of: cur) ?? 0) + 1) % allStyles.count
                doc.paragraphs[pIdx].style = allStyles[nextIdx]
            }
            openTabs[idx].payload = .document(doc)
            openTabs[idx].isModified = true

        case var .sheet(sheet):
            let current = sheet.rawValue(at: selectedSheetCell)
            if current.isEmpty {
                try? sheet.setCell(selectedSheetCell, rawValue: "서식적용")
            }
            openTabs[idx].payload = .sheet(sheet)
            openTabs[idx].isModified = true

        case var .slide(deck):
            guard !deck.slides.isEmpty else { return }
            let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
            switch action {
            case .bold, .italic, .underline:
                if !deck.slides[sIdx].elements.isEmpty {
                    let currentStroke = deck.slides[sIdx].elements[0].strokeHex
                    deck.slides[sIdx].elements[0].strokeHex = (currentStroke == "#5C6BC0") ? "#3949AB" : "#5C6BC0"
                }
            case .styleCycle:
                let layouts = SlideMasterLayout.allCases
                let cur = deck.slides[sIdx].masterLayout
                let nextIdx = ((layouts.firstIndex(of: cur) ?? 0) + 1) % layouts.count
                deck.slides[sIdx].masterLayout = layouts[nextIdx]
            }
            openTabs[idx].payload = .slide(deck)
            openTabs[idx].isModified = true

        case var .pdf(pdfDoc):
            let pageNum = pdfDoc.pages.indices.contains(selectedPDFPageIndex)
                ? pdfDoc.pages[selectedPDFPageIndex].pageNumber
                : 1
            let annKind: PDFAnnotationKind = (action == .underline) ? .underline : .highlight
            pdfDoc.annotations.append(
                PDFAnnotationItem(pageNumber: pageNum, kind: annKind, content: "서식 강조 표시", author: "QA")
            )
            openTabs[idx].payload = .pdf(pdfDoc)
            openTabs[idx].isModified = true
        }
    }

    private enum InsertActionKind {
        case table
        case chart
        case shape
        case link
    }

    private func applyInsertCommand(at idx: Int, kind: InsertActionKind) {
        switch openTabs[idx].payload {
        case var .document(doc):
            switch kind {
            case .table:
                doc.tables.append(
                    DocumentTableBlock(
                        caption: "표 \(doc.tables.count + 1). 추가 데이터 표",
                        rows: [
                            ["구분", "목표치", "실적치"],
                            ["1분기", "100", "120"],
                            ["2분기", "150", "180"]
                        ]
                    )
                )
            case .chart:
                doc.paragraphs.append(
                    ParagraphBlock(style: .quote, alignment: .leading, text: "[차트 개체: 분기별 핵심 성과 지표 시각화]")
                )
            case .shape:
                doc.paragraphs.append(
                    ParagraphBlock(style: .quote, alignment: .center, text: "[다이어그램 도형: 프로세스 아키텍처 블록]")
                )
            case .link:
                let anchorID = doc.paragraphs.first?.id ?? UUID()
                doc.bookmarks["책갈피_\(doc.bookmarks.count + 1)"] = anchorID
            }
            openTabs[idx].payload = .document(doc)
            openTabs[idx].isModified = true

        case var .sheet(sheet):
            switch kind {
            case .table:
                try? sheet.setCell("E1", rawValue: "추가항목")
                try? sheet.setCell("E2", rawValue: "250")
            case .chart:
                let allKinds = SheetChartKind.allCases
                if let cIdx = allKinds.firstIndex(of: selectedChartKind) {
                    selectedChartKind = allKinds[(cIdx + 1) % allKinds.count]
                }
            case .shape:
                try? sheet.setCell("E3", rawValue: "[도형 콜아웃]")
            case .link:
                try? sheet.setCell("E4", rawValue: "https://standard.example.org")
            }
            openTabs[idx].payload = .sheet(sheet)
            openTabs[idx].isModified = true

        case var .slide(deck):
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
            deck.slides[sIdx].elements.append(
                SlideCanvasElement(
                    title: title,
                    bodyText: "슬라이드 \(sIdx + 1) 추가 개체",
                    x: 380,
                    y: 220,
                    width: 200,
                    height: 100,
                    kind: elementKind
                )
            )
            openTabs[idx].payload = .slide(deck)
            openTabs[idx].isModified = true

        case var .pdf(pdfDoc):
            let pageNum = pdfDoc.pages.indices.contains(selectedPDFPageIndex)
                ? pdfDoc.pages[selectedPDFPageIndex].pageNumber
                : 1
            if kind == .link {
                pdfDoc.outline.append(
                    PDFOutlineNode(title: "책갈피 \(pdfDoc.outline.count + 1)", targetPageNumber: pageNum)
                )
            } else {
                pdfDoc.annotations.append(
                    PDFAnnotationItem(pageNumber: pageNum, kind: .stickyNote, content: "삽입 개체 마크", author: "QA")
                )
            }
            openTabs[idx].payload = .pdf(pdfDoc)
            openTabs[idx].isModified = true
        }
    }

    private enum LayoutActionKind {
        case paperSize
        case orientation
        case columns
        case margins
    }

    private func applyLayoutCommand(at idx: Int, kind: LayoutActionKind) {
        switch openTabs[idx].payload {
        case var .document(doc):
            if doc.sections.isEmpty {
                doc.sections = [DocumentSectionLayout(paperSize: doc.paperSize)]
            }
            switch kind {
            case .paperSize:
                let presets = PaperSizePreset.allCases
                let cur = doc.paperSize
                let next = presets[((presets.firstIndex(of: cur) ?? 0) + 1) % presets.count]
                doc.paperSize = next
                doc.sections[0].paperSize = next
            case .orientation:
                doc.sections[0].orientation = (doc.sections[0].orientation == .portrait) ? .landscape : .portrait
            case .columns:
                let nextCols = (doc.sections[0].columnCount % 3) + 1
                doc.sections[0].columnCount = nextCols
            case .margins:
                let isNarrow = doc.sections[0].marginTopPt < 45.0
                let targetMargin = isNarrow ? 56.7 : 36.0
                doc.sections[0].marginTopPt = targetMargin
                doc.sections[0].marginBottomPt = targetMargin
                doc.sections[0].marginLeftPt = targetMargin
                doc.sections[0].marginRightPt = targetMargin
            }
            openTabs[idx].payload = .document(doc)
            openTabs[idx].isModified = true

        case var .slide(deck):
            let ratios = SlideAspectRatio.allCases
            let cur = deck.aspectRatio
            deck.aspectRatio = ratios[((ratios.firstIndex(of: cur) ?? 0) + 1) % ratios.count]
            openTabs[idx].payload = .slide(deck)
            openTabs[idx].isModified = true

        case var .pdf(pdfDoc):
            if kind == .orientation {
                let targetIndex = pdfDoc.pages.indices.contains(selectedPDFPageIndex) ? selectedPDFPageIndex : 0
                pdfDoc.rotatePage(at: targetIndex, clockwiseDegrees: 90)
                openTabs[idx].payload = .pdf(pdfDoc)
                openTabs[idx].isModified = true
            }

        case .sheet:
            openTabs[idx].isModified = true
        }
    }

    private enum DataActionKind {
        case autoSum
        case functionWizard
        case sort
        case validation
    }

    private func applyDataCommand(at idx: Int, kind: DataActionKind) {
        switch openTabs[idx].payload {
        case var .sheet(sheet):
            switch kind {
            case .autoSum:
                formulaBarDraft = "=SUM(B2:C2)"
                try? sheet.setCell(selectedSheetCell, rawValue: formulaBarDraft)
            case .functionWizard:
                formulaBarDraft = "=AVERAGE(B2:C3)"
                try? sheet.setCell(selectedSheetCell, rawValue: formulaBarDraft)
            case .sort:
                sheetSortAscending.toggle()
                if let range = try? CellRange(notation: "A2:C3") {
                    let sorted = sheet.sortedRows(in: range, byColumnOffset: 1, ascending: sheetSortAscending)
                    for (rOffset, rowVals) in sorted.enumerated() {
                        let targetRow = 2 + rOffset
                        if rowVals.indices.contains(0) {
                            try? sheet.setCell("A\(targetRow)", rawValue: rowVals[0].description)
                        }
                        if rowVals.indices.contains(1) {
                            try? sheet.setCell("B\(targetRow)", rawValue: rowVals[1].description)
                        }
                        if rowVals.indices.contains(2) {
                            try? sheet.setCell("C\(targetRow)", rawValue: rowVals[2].description)
                        }
                    }
                }
            case .validation:
                activeDataValidationRule = .numberRange(min: 0.0, max: 100_000.0)
            }
            openTabs[idx].payload = .sheet(sheet)
            openTabs[idx].isModified = true

        case var .document(doc):
            if kind == .autoSum, !doc.tables.isEmpty {
                let lastCol = max(0, doc.tables[0].columnCount - 1)
                if let sum = doc.tables[0].evaluateTableFormula("=SUM(ABOVE)", atRow: doc.tables[0].rowCount, column: lastCol) {
                    doc.footerText = "표 합계 검산: \(Int(sum))"
                }
            } else if kind == .validation {
                activeDataValidationRule = .nonEmptyText
            }
            openTabs[idx].payload = .document(doc)
            openTabs[idx].isModified = true

        case .slide, .pdf:
            activeDataValidationRule = .nonEmptyText
            openTabs[idx].isModified = true
        }
    }
}
