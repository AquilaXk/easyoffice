import Foundation
import EasyOfficeDesignSystem

/// Top-level workspace navigation section in the macOS sidebar.
public enum HubSection: String, CaseIterable, Sendable, Identifiable {
    case home = "홈 허브"
    case recent = "최근 문서"
    case pinned = "고정 문서"
    case templates = "서식 템플릿"
    case converter = "포맷 일괄 변환"

    public var id: String { rawValue }

    public var symbolName: String {
        switch self {
        case .home: return "house.fill"
        case .recent: return "clock.fill"
        case .pinned: return "pin.fill"
        case .templates: return "square.grid.2x2.fill"
        case .converter: return "arrow.triangle.2.circlepath.doc.on.clipboard"
        }
    }
}

/// Context-aware ribbon tabs adapting to the active document module.
public enum RibbonCategory: String, CaseIterable, Sendable, Identifiable {
    case home = "홈 (Home)"
    case insert = "삽입 (Insert)"
    case layout = "모양/레이아웃 (Layout)"
    case moduleSpecific = "전용 도구 (Module)"
    case review = "검토/보안 (Review)"
    case view = "보기 (View)"

    public var id: String { rawValue }

    public func title(for module: OfficeModuleKind) -> String {
        switch self {
        case .home: return "홈"
        case .insert: return "삽입"
        case .layout: return "레이아웃"
        case .moduleSpecific:
            switch module {
            case .document: return "참조·메일머지"
            case .hwp: return "공문서·누름틀"
            case .sheet: return "수식·데이터"
            case .slide: return "슬라이드·전환"
            case .pdf: return "주석·변환"
            }
        case .review: return "검토"
        case .view: return "보기"
        }
    }
}

/// Built-in template descriptor available from the Home Hub.
public struct OfficeTemplateDescriptor: Sendable, Equatable, Identifiable {
    public let id: String
    public let title: String
    public let subtitle: String
    public let format: DocumentFormat
    public let moduleKind: OfficeModuleKind

    public init(id: String, title: String, subtitle: String, format: DocumentFormat) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.format = format
        self.moduleKind = format.moduleKind
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
    public var isModified: Bool
    public var payload: OpenTabPayload

    public init(
        id: UUID = UUID(),
        fileName: String,
        format: DocumentFormat,
        isPinned: Bool = false,
        isModified: Bool = false,
        payload: OpenTabPayload
    ) {
        self.id = id
        self.fileName = fileName
        self.format = format
        self.isPinned = isPinned
        self.isModified = isModified
        self.payload = payload
    }

    public var moduleKind: OfficeModuleKind { format.moduleKind }

    /// Extracts all searchable text across document paragraphs, tables, sheet cells, slide notes, or PDF pages.
    public func searchableCorpus() -> String {
        var chunks: [String] = [fileName, format.rawValue, format.standardSpecification]
        switch payload {
        case let .document(doc):
            chunks.append(doc.title)
            chunks.append(contentsOf: doc.paragraphs.map(\.text))
            chunks.append(contentsOf: doc.tables.flatMap { $0.rows.flatMap { $0 } })
        case let .sheet(sheet):
            chunks.append(sheet.name)
            for (addr, raw) in sheet.rawCells {
                chunks.append("\(addr)=\(raw)")
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
            title: "기안문 및 행정 공문서 양식",
            subtitle: "결재란·두문·본문·협조란 표준 규격 한글 문서",
            format: .hwpx
        ),
        OfficeTemplateDescriptor(
            id: "tpl-sheet-finance",
            title: "예산 집행 및 분기 손익 분석 시트",
            subtitle: "자동 합계·평균·조건부 달성률 수식이 포함된 시트",
            format: .xlsx
        ),
        OfficeTemplateDescriptor(
            id: "tpl-slide-pitch",
            title: "16:9 경영 브리핑 & 제품 발표덱",
            subtitle: "슬라이드 마스터·비교 레이아웃·발표자 노트 구성",
            format: .pptx
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
            let doc = OfficeDocumentModel(
                title: "2026 프로젝트 아키텍처 보고서",
                format: .docx,
                paperSize: .a4,
                headerText: "EasyOffice 기술 문서 표준",
                footerText: "- 1 -",
                paragraphs: [
                    ParagraphBlock(style: .title, alignment: .leading, text: "네이티브 macOS 올인원 오피스 설계 보고서", fontSizePt: 20, isBold: true),
                    ParagraphBlock(style: .heading1, alignment: .leading, text: "1. 아키텍처 목표 및 핵심 가치", fontSizePt: 14, isBold: true),
                    ParagraphBlock(style: .body, alignment: .justified, text: "단일 멀티 탭 윈도우에서 문서, 시트, 슬라이드, 한글(HWP/HWPX), PDF를 즉각 열람·편집하고 시그니처 라벤더 디자인 시스템을 일관되게 제공합니다.")
                ],
                tables: [
                    DocumentTableBlock(
                        caption: "표 1. 모듈별 지원 규격",
                        rows: [
                            ["모듈", "대표 포맷", "렌더링 엔진"],
                            ["Document / HWP", "DOCX, HWP, HWPX, ODT", "Rich Page Layout Engine"],
                            ["Sheet", "XLSX, ODS, CSV", "Virtualized A1 Formula Grid"],
                            ["Slide & PDF", "PPTX, ODP, PDF", "Stage Canvas & Vector PDF Studio"]
                        ]
                    )
                ]
            )
            return WorkspaceTabItem(fileName: "아키텍처_보고서.docx", format: .docx, isPinned: true, payload: .document(doc))

        case .hwp:
            let hwpDoc = OfficeDocumentModel(
                title: "표준 업무 협조전 (공문서)",
                format: .hwpx,
                paperSize: .a4,
                headerText: "문서번호: 기안-2026-0104",
                footerText: "대외비 · 무단 전재 금지",
                paragraphs: [
                    ParagraphBlock(style: .officialHeader, alignment: .center, text: "업 무 협 조 전", fontSizePt: 18, isBold: true),
                    ParagraphBlock(style: .heading1, alignment: .leading, text: "제목: {{부서명}} 2026년도 표준 문서 포맷 호환성 검증 건", fontSizePt: 13, isBold: true),
                    ParagraphBlock(style: .body, alignment: .justified, text: "1. 귀 부서의 무궁한 발전을 기원합니다.\n2. HWP 5.0(OLE2) 및 HWPX(KS X 6101 OWPML) 양식의 표·누름틀·각주 호환성 검증 결과를 아래와 같이 회신합니다.")
                ],
                tables: [
                    DocumentTableBlock(
                        caption: "결재란 및 검토 내역",
                        rows: [
                            ["구분", "담당", "검토", "승인"],
                            ["서명", "김지훈", "박서연", "최현우"]
                        ]
                    )
                ]
            )
            return WorkspaceTabItem(fileName: "업무협조전_공문서.hwpx", format: .hwpx, isPinned: true, payload: .document(hwpDoc))

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
            return WorkspaceTabItem(fileName: "분기별_실적_분석.xlsx", format: .xlsx, isPinned: true, payload: .sheet(sheet))

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
                        masterLayout: .titleSlide
                    ),
                    SlideModel(
                        title: "핵심 모듈 구성",
                        subtitle: "Document · HWP/HWPX · Sheet · Slide · PDF Studio",
                        bulletPoints: [
                            "매직 바이트 기반 Fail-Closed 보안 컨테이너 검증",
                            "결정론적 A1 수식 연산 및 PDF 양방향 변환"
                        ],
                        speakerNotes: "두 번째 슬라이드: 보안 및 포맷 호환성 설명",
                        masterLayout: .twoColumnComparison
                    )
                ]
            )
            return WorkspaceTabItem(fileName: "제품_로드맵_발표자료.pptx", format: .pptx, payload: .slide(deck))

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
                ]
            )
            return WorkspaceTabItem(fileName: "보안_아키텍처_가이드.pdf", format: .pdf, payload: .pdf(pdf))
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
}
