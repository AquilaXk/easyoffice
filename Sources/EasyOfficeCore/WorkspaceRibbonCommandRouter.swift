import Foundation
import EasyOfficeDesignSystem

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

extension WorkspaceViewModel {

    // MARK: - Complete 140-Action Context Ribbon Dispatcher

    public func executeRibbonCommand(_ commandID: String) {
        activeInlineEditTargetKey = nil

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
            setZoom(zoomPercent == 100.0 ? 125.0 : 100.0)
            lastExecutedCommandSummary = "배율 \(Int(zoomPercent))% 전환 완료"
            return
        }

        guard let tabID = activeTab?.id,
              let idx = openTabs.firstIndex(where: { $0.id == tabID }) else {
            lastExecutedCommandSummary = "활성 탭 없음"
            return
        }

        if commandID.hasSuffix("-home-bold") {
            recordUndoSnapshot(for: tabID); applyHomeFormatting(at: idx, action: .bold)
            lastExecutedCommandSummary = "굵게 서식 전환 완료"; return
        }
        if commandID.hasSuffix("-home-italic") {
            recordUndoSnapshot(for: tabID); applyHomeFormatting(at: idx, action: .italic)
            lastExecutedCommandSummary = "기울임 서식 전환 완료"; return
        }
        if commandID.hasSuffix("-home-underline") {
            recordUndoSnapshot(for: tabID); applyHomeFormatting(at: idx, action: .underline)
            lastExecutedCommandSummary = "밑줄 서식 전환 완료"; return
        }
        if commandID.hasSuffix("-home-style") {
            recordUndoSnapshot(for: tabID); applyHomeFormatting(at: idx, action: .styleCycle)
            lastExecutedCommandSummary = "표준 단락·개체 스타일 순환 적용 완료"; return
        }

        if commandID.hasSuffix("-insert-table") {
            recordUndoSnapshot(for: tabID); applyInsertCommand(at: idx, kind: .table)
            lastExecutedCommandSummary = "표 개체 삽입 완료"; return
        }
        if commandID.hasSuffix("-insert-chart") {
            recordUndoSnapshot(for: tabID); applyInsertCommand(at: idx, kind: .chart)
            if case let .sheet(sheet) = openTabs[idx].payload {
                lastExecutedCommandSummary = "차트 개체 삽입 완료: \(selectedChartKind.rawValue) (\(sheet.detectedNumericRange().a1Range))"
            } else {
                lastExecutedCommandSummary = "차트 개체 삽입 완료"
            }
            return
        }
        if commandID.hasSuffix("-insert-shape") {
            recordUndoSnapshot(for: tabID); applyInsertCommand(at: idx, kind: .shape)
            lastExecutedCommandSummary = "도형·다이어그램 개체 삽입 완료"; return
        }
        if commandID.hasSuffix("-insert-link") {
            recordUndoSnapshot(for: tabID); applyInsertCommand(at: idx, kind: .link)
            lastExecutedCommandSummary = "책갈피·참조 링크 삽입 완료"; return
        }

        if commandID.hasSuffix("-layout-paper") || commandID.hasSuffix("-layout-size") {
            recordUndoSnapshot(for: tabID); applyLayoutCommand(at: idx, kind: .paperSize)
            lastExecutedCommandSummary = "용지 및 화면 규격 전환 완료"; return
        }
        if commandID.hasSuffix("-layout-orientation") || commandID.hasSuffix("-layout-orient") {
            recordUndoSnapshot(for: tabID); applyLayoutCommand(at: idx, kind: .orientation)
            lastExecutedCommandSummary = "용지 방향 전환 완료"; return
        }
        if commandID.hasSuffix("-layout-columns") {
            recordUndoSnapshot(for: tabID); applyLayoutCommand(at: idx, kind: .columns)
            lastExecutedCommandSummary = "다단 레이아웃 전환 완료"; return
        }
        if commandID.hasSuffix("-layout-margins") || commandID.hasSuffix("-layout-margin") {
            recordUndoSnapshot(for: tabID); applyLayoutCommand(at: idx, kind: .margins)
            lastExecutedCommandSummary = "여백 프리셋 전환 완료"; return
        }

        if commandID.hasSuffix("-data-autosum") || commandID.hasSuffix("-data-sum") {
            recordUndoSnapshot(for: tabID); applyDataCommand(at: idx, kind: .autoSum)
            lastExecutedCommandSummary = "자동 합계(SUM) 수식 계산 완료"; return
        }
        if commandID.hasSuffix("-data-fn-browser") || commandID.hasSuffix("-data-fx") {
            recordUndoSnapshot(for: tabID); applyDataCommand(at: idx, kind: .functionWizard)
            lastExecutedCommandSummary = "77종 함수 마법사(AVERAGE) 수식 적용 완료"; return
        }
        if commandID.hasSuffix("-data-sort") {
            recordUndoSnapshot(for: tabID); applyDataCommand(at: idx, kind: .sort)
            lastExecutedCommandSummary = "데이터 행 정렬 완료 (\(sheetSortAscending ? "오름차순" : "내림차순"))"; return
        }
        if commandID.hasSuffix("-data-validation") || commandID.hasSuffix("-data-validate") {
            recordUndoSnapshot(for: tabID); applyDataCommand(at: idx, kind: .validation)
            lastExecutedCommandSummary = "데이터 유효성 검사 규칙 전환 완료"; return
        }

        switch commandID {
        case "doc-mod-toc":
            recordUndoSnapshot(for: tabID)
            if case var .document(doc) = openTabs[idx].payload {
                let toc = doc.generateTableOfContents()
                let summaryText = toc.isEmpty ? "[자동 목차] 등록된 제목 스타일 없음" : "[자동 목차] " + toc.map(\.title).joined(separator: " · ")
                doc.paragraphs.insert(ParagraphBlock(style: .quote, alignment: .leading, text: summaryText, isBold: true), at: 0)
                openTabs[idx].payload = .document(doc)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "자동 목차(TOC) 생성 완료"; return

        case "doc-mod-footnote":
            recordUndoSnapshot(for: tabID)
            if case var .document(doc) = openTabs[idx].payload {
                let anchorID = doc.paragraphs.last?.id ?? UUID()
                let nextNum = doc.footnotes.count + 1
                doc.footnotes.append(FootnoteItem(number: nextNum, anchorParagraphID: anchorID, text: "각주 \(nextNum): 표준 규격 참조 설명"))
                openTabs[idx].payload = .document(doc)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "각주·미주 삽입 완료"; return

        case "doc-mod-mailmerge", "doc-mod-merge":
            recordUndoSnapshot(for: tabID)
            if case var .document(doc) = openTabs[idx].payload {
                doc = doc.applyingMailMerge(record: mailMergeFields)
                doc.footerText = "메일 머지 치환 완료 (\(mailMergeFields.count)개 필드)"
                openTabs[idx].payload = .document(doc)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "메일 머지 필드 일괄 치환 완료"; return

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
            lastExecutedCommandSummary = "표 셀 수식(=SUM(ABOVE)) 계산 반영 완료"; return

        case "hwp-mod-header", "hwp-mod-official":
            recordUndoSnapshot(for: tabID)
            if case var .document(doc) = openTabs[idx].payload {
                doc.headerText = "행정기관 표준 공문서 · 문서번호 기안-2026-0201"
                let banner = ParagraphBlock(style: .officialHeader, alignment: .center, text: "행 정 공 문 서 두 문 · 결 재 란", fontSizePt: 18, isBold: true)
                doc.paragraphs.insert(banner, at: 0)
                openTabs[idx].payload = .document(doc)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "행정 공문서 두문·결재란 삽입 완료"; return

        case "hwp-mod-field":
            recordUndoSnapshot(for: tabID)
            if case var .document(doc) = openTabs[idx].payload {
                doc.paragraphs.append(ParagraphBlock(style: .body, alignment: .justified, text: "누름틀 입력란: {{부서명}} / {{수신자}}"))
                openTabs[idx].payload = .document(doc)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "한글 누름틀 필드 삽입 완료"; return

        case "hwp-mod-border":
            recordUndoSnapshot(for: tabID)
            if case var .document(doc) = openTabs[idx].payload {
                if doc.tables.isEmpty {
                    doc.tables.append(DocumentTableBlock(caption: "결재란 표", rows: [["담당", "검토", "승인"], ["결재", "결재", "결재"]], borderStyle: .doubleLine))
                } else {
                    let current = doc.tables[0].borderStyle
                    doc.tables[0].borderStyle = (current == .doubleLine) ? .diagonalDown : .doubleLine
                }
                openTabs[idx].payload = .document(doc)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "이중선·대각선 표 테두리 적용 완료"; return

        case "hwp-mod-manuscript":
            recordUndoSnapshot(for: tabID)
            if case var .document(doc) = openTabs[idx].payload {
                let stats = doc.computeStatistics()
                doc.footerText = "200자 원고지 환산: \(stats.manuscriptPages200)매 (\(stats.characterCountWithSpaces)자)"
                openTabs[idx].payload = .document(doc)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "200자 원고지 매수 검산 반영 완료"; return

        case "sheet-mod-pivot":
            recordUndoSnapshot(for: tabID)
            if case var .sheet(sheet) = openTabs[idx].payload {
                let range = sheet.detectedDataRange()
                let valueCol = max(0, range.end.column - range.start.column)
                let summary = PivotTableEngine.compute(
                    from: sheet,
                    range: range,
                    hasHeaderRow: true,
                    configuration: PivotTableConfiguration(rowFieldColumnIndex: 0, valueFieldColumnIndex: valueCol, aggregation: .sum)
                )
                lastPivotGrandTotal = summary.grandTotal
                let coord = (try? CellAddress(a1Notation: selectedSheetCell))?.packedCoordinate ?? PackedCellCoordinate(row: 0, column: 0)
                sheet.sheetComments[coord] = "피벗 집계(\(range.a1Range)): \(String(format: "%.0f", summary.grandTotal))"
                openTabs[idx].payload = .sheet(sheet)
                openTabs[idx].isModified = true
                lastExecutedCommandSummary = "피벗 테이블 요약 집계 완료 (\(range.a1Range), 총계 \(String(format: "%.0f", summary.grandTotal)))"
            } else {
                lastExecutedCommandSummary = "피벗 테이블 요약 집계 완료"
            }
            return

        case "sheet-mod-chart":
            let allKinds = SheetChartKind.allCases
            if let currentIdx = allKinds.firstIndex(of: selectedChartKind) {
                selectedChartKind = allKinds[(currentIdx + 1) % allKinds.count]
            }
            lastExecutedCommandSummary = "차트 시각화 유형 전환: \(selectedChartKind.rawValue)"; return

        case "sheet-mod-condfmt", "sheet-mod-cond":
            activeConditionalFormatRule = (activeConditionalFormatRule == nil) ? .greaterThan(100.0) : nil
            lastExecutedCommandSummary = "조건부 서식(>100 강조) 규칙 적용 완료"; return

        case "sheet-mod-freeze":
            isHeaderRowFrozen.toggle()
            lastExecutedCommandSummary = isHeaderRowFrozen ? "행·열 틀 고정 활성화" : "행·열 틀 고정 해제"; return

        case "slide-mod-master":
            recordUndoSnapshot(for: tabID)
            if case var .slide(deck) = openTabs[idx].payload, !deck.slides.isEmpty {
                let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
                let layouts = SlideMasterLayout.allCases
                let cur = deck.slides[sIdx].masterLayout
                deck.slides[sIdx].masterLayout = layouts[((layouts.firstIndex(of: cur) ?? 0) + 1) % layouts.count]
                openTabs[idx].payload = .slide(deck)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "슬라이드 마스터 레이아웃 순환 적용 완료"; return

        case "slide-mod-snap":
            recordUndoSnapshot(for: tabID)
            isSnapGuidesEnabled = true
            if case var .slide(deck) = openTabs[idx].payload, !deck.slides.isEmpty {
                let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
                if deck.slides[sIdx].elements.isEmpty {
                    deck.slides[sIdx].elements.append(SlideCanvasElement(title: "스냅 정렬 개체", bodyText: "중앙 정렬", x: 120, y: 90, width: 200, height: 100, kind: .textBox))
                }
                let eIdx = deck.slides[sIdx].elements.firstIndex(where: { $0.id == selectedSlideElementID }) ?? 0
                let stage = deck.aspectRatio.stageSize
                let elemW = deck.slides[sIdx].elements[eIdx].width
                let elemH = deck.slides[sIdx].elements[eIdx].height
                deck.slides[sIdx].elements[eIdx].x = (stage.width - elemW) / 2.0
                deck.slides[sIdx].elements[eIdx].y = (stage.height - elemH) / 2.0
                let aligned = deck.slides[sIdx].elements[eIdx]
                let siblings = deck.slides[sIdx].elements.enumerated().compactMap { $0 == eIdx ? nil : $1 }
                activeSnapGuides = SnapGuideCalculator.computeGuides(for: aligned, siblings: siblings, aspectRatio: deck.aspectRatio, tolerancePoints: 5.0)
                openTabs[idx].payload = .slide(deck)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "스마트 정렬 가이드 정중앙 스냅 완료"; return

        case "slide-mod-transition":
            recordUndoSnapshot(for: tabID)
            if case var .slide(deck) = openTabs[idx].payload, !deck.slides.isEmpty {
                let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
                let transitions = SlideTransitionKind.allCases
                let cur = deck.slides[sIdx].transition
                deck.slides[sIdx].transition = transitions[((transitions.firstIndex(of: cur) ?? 0) + 1) % transitions.count]
                openTabs[idx].payload = .slide(deck)
                openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "슬라이드 화면 전환 효과 변경 완료"; return

        case "slide-mod-presenter":
            isPresenterModeActive.toggle()
            isPresenterTimerRunning = isPresenterModeActive
            lastExecutedCommandSummary = isPresenterModeActive ? "발표자 도구 시작" : "발표자 도구 닫기"; return

        case "pdf-mod-highlight", "pdf-mod-annotate":
            addPDFAnnotation(kind: .highlight, content: "형광펜 하이라이트 주석")
            lastExecutedCommandSummary = "PDF 형광펜 주석 추가 완료"; return

        case "pdf-mod-rotate":
            rotateActivePDFPage(clockwiseDegrees: 90)
            lastExecutedCommandSummary = "PDF 페이지 90° 회전 완료"; return

        case "pdf-mod-splitmerge", "pdf-mod-split":
            recordUndoSnapshot(for: tabID)
            if case var .pdf(pdfDoc) = openTabs[idx].payload {
                if pdfDoc.pages.count > 1 {
                    let extracted = pdfDoc.extractPages(oneBasedPageNumbers: [1], newTitle: "\(pdfDoc.title) (1페이지 추출)")
                    let splitTab = WorkspaceTabItem(fileName: "분할_\(openTabs[idx].fileName)", format: .pdf, isModified: true, payload: .pdf(extracted))
                    openTabs.append(splitTab)
                } else {
                    pdfDoc.insertBlankPage(at: pdfDoc.pages.count)
                    openTabs[idx].payload = .pdf(pdfDoc)
                    openTabs[idx].isModified = true
                }
            }
            lastExecutedCommandSummary = "PDF 페이지 분할·병합 실행 완료"; return

        case "pdf-mod-tooffice", "pdf-mod-convert":
            if case let .pdf(pdfDoc) = openTabs[idx].payload {
                let editable = pdfDoc.convertToEditableDocument(targetFormat: .docx)
                let convertedTab = WorkspaceTabItem(fileName: "\(pdfDoc.title)_변환본.docx", format: .docx, isModified: true, payload: .document(editable))
                openTabs.append(convertedTab)
                activeTabID = convertedTab.id
            }
            lastExecutedCommandSummary = "PDF → 편집 문서(DOCX) 변환 완료"; return

        default:
            break
        }

        if commandID.hasSuffix("-review-track") {
            recordUndoSnapshot(for: tabID)
            switch openTabs[idx].payload {
            case var .document(doc):
                doc.trackChangesEnabled.toggle(); openTabs[idx].payload = .document(doc)
            case var .sheet(sheet):
                sheet.trackChangesEnabled.toggle(); openTabs[idx].payload = .sheet(sheet)
            case var .slide(deck):
                if !deck.slides.isEmpty {
                    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
                    deck.slides[sIdx].speakerNotes += " [변경추적]"
                    openTabs[idx].payload = .slide(deck)
                }
            case var .pdf(pdfDoc):
                pdfDoc.annotations.append(PDFAnnotationItem(pageNumber: 1, kind: .stickyNote, content: "변경 내용 추적 감사 마크", author: authorName))
                openTabs[idx].payload = .pdf(pdfDoc)
            }
            openTabs[idx].isModified = true
            lastExecutedCommandSummary = "변경 내용 추적 상태 전환 완료"; return
        }
        if commandID.hasSuffix("-review-comment") {
            recordUndoSnapshot(for: tabID)
            let noteText = commentDraftText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? "검토 의견이 등록되었습니다."
                : commentDraftText
            switch openTabs[idx].payload {
            case var .document(doc):
                let anchorID = doc.paragraphs.first?.id ?? UUID()
                doc.comments.append(DocumentCommentThread(anchorParagraphID: anchorID, author: authorName, message: noteText))
                openTabs[idx].payload = .document(doc); openTabs[idx].isModified = true
            case var .pdf(pdfDoc):
                pdfDoc.annotations.append(PDFAnnotationItem(pageNumber: 1, kind: .stickyNote, content: noteText, author: authorName))
                openTabs[idx].payload = .pdf(pdfDoc); openTabs[idx].isModified = true
            case var .slide(deck):
                if !deck.slides.isEmpty {
                    let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
                    deck.slides[sIdx].speakerNotes += "\n[검토 메모] \(noteText)"
                    openTabs[idx].payload = .slide(deck); openTabs[idx].isModified = true
                }
            case var .sheet(sheet):
                let coord = (try? CellAddress(a1Notation: selectedSheetCell))?.packedCoordinate ?? PackedCellCoordinate(row: 0, column: 0)
                if let existing = sheet.sheetComments[coord], !existing.isEmpty {
                    sheet.sheetComments[coord] = "\(existing) · \(noteText)"
                } else {
                    sheet.sheetComments[coord] = noteText
                }
                openTabs[idx].payload = .sheet(sheet); openTabs[idx].isModified = true
            }
            lastExecutedCommandSummary = "검토 메모 스레드 등록 완료"; return
        }
        if commandID.hasSuffix("-review-verify") || commandID.hasSuffix("-review-security") {
            if commandID.hasSuffix("-review-security"), case var .sheet(sheet) = openTabs[idx].payload {
                recordUndoSnapshot(for: tabID)
                sheet.sheetComments.removeAll()
                openTabs[idx].payload = .sheet(sheet)
            }
            runSecurityAuditOnActiveTab()
            lastExecutedCommandSummary = "매직 바이트 및 컨테이너 보안 검증 완료"; return
        }
        if commandID.hasSuffix("-review-stats") {
            showRightInspector = true
            reviewStatsRefreshCount += 1
            lastExecutedCommandSummary = "문서 통계 및 원고지 매수 인스펙터 갱신 완료"; return
        }

        lastExecutedCommandSummary = "명령 실행: \(commandID)"
    }

    // MARK: - Private Command Helpers

    private enum HomeFormattingAction {
        case bold, italic, underline, styleCycle
    }

    private func applyHomeFormatting(at idx: Int, action: HomeFormattingAction) {
        switch openTabs[idx].payload {
        case var .document(doc):
            if doc.paragraphs.isEmpty { doc.paragraphs.append(ParagraphBlock(text: "기본 본문 문단")) }
            let pIdx = doc.paragraphs.firstIndex(where: { $0.id == selectedParagraphID }) ?? 0
            switch action {
            case .bold: doc.paragraphs[pIdx].isBold.toggle()
            case .italic: doc.paragraphs[pIdx].isItalic.toggle()
            case .underline: doc.paragraphs[pIdx].isUnderlined.toggle()
            case .styleCycle:
                let allStyles = ParagraphStyleRole.allCases
                let cur = doc.paragraphs[pIdx].style
                doc.paragraphs[pIdx].style = allStyles[((allStyles.firstIndex(of: cur) ?? 0) + 1) % allStyles.count]
            }
            openTabs[idx].payload = .document(doc)
            openTabs[idx].isModified = true

        case var .sheet(sheet):
            let coord = (try? CellAddress(a1Notation: selectedSheetCell))?.packedCoordinate ?? PackedCellCoordinate(row: 0, column: 0)
            var style = sheet.cellStyle(at: coord)
            switch action {
            case .bold: style.isBold.toggle()
            case .italic: style.isItalic.toggle()
            case .underline: style.isUnderlined.toggle()
            case .styleCycle:
                let formats = CellNumberFormat.allCases
                let curIdx = formats.firstIndex(of: style.numberFormat) ?? 0
                style.numberFormat = formats[(curIdx + 1) % formats.count]
                style.fillHex = (style.fillHex == nil) ? "#EEF0FF" : nil
            }
            sheet.setCellStyle(style, at: coord)
            openTabs[idx].payload = .sheet(sheet)
            openTabs[idx].isModified = true

        case var .slide(deck):
            guard !deck.slides.isEmpty else { return }
            let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
            if deck.slides[sIdx].elements.isEmpty {
                deck.slides[sIdx].elements.append(SlideCanvasElement(title: "기본 텍스트 상자", bodyText: "슬라이드 본문", x: 200, y: 160, width: 240, height: 90, kind: .textBox))
            }
            let eIdx = deck.slides[sIdx].elements.firstIndex(where: { $0.id == selectedSlideElementID }) ?? 0
            switch action {
            case .bold: deck.slides[sIdx].elements[eIdx].isBold.toggle()
            case .italic: deck.slides[sIdx].elements[eIdx].isItalic.toggle()
            case .underline: deck.slides[sIdx].elements[eIdx].isUnderlined.toggle()
            case .styleCycle:
                let layouts = SlideMasterLayout.allCases
                let cur = deck.slides[sIdx].masterLayout
                deck.slides[sIdx].masterLayout = layouts[((layouts.firstIndex(of: cur) ?? 0) + 1) % layouts.count]
            }
            openTabs[idx].payload = .slide(deck)
            openTabs[idx].isModified = true

        case var .pdf(pdfDoc):
            let pageNum = pdfDoc.pages.indices.contains(selectedPDFPageIndex) ? pdfDoc.pages[selectedPDFPageIndex].pageNumber : 1
            let annKind: PDFAnnotationKind = (action == .underline) ? .underline : .highlight
            pdfDoc.annotations.append(PDFAnnotationItem(pageNumber: pageNum, kind: annKind, content: "서식 강조 표시", author: authorName))
            openTabs[idx].payload = .pdf(pdfDoc)
            openTabs[idx].isModified = true
        }
    }

    private enum InsertActionKind {
        case table, chart, shape, link
    }

    private func applyInsertCommand(at idx: Int, kind: InsertActionKind) {
        switch openTabs[idx].payload {
        case var .document(doc):
            switch kind {
            case .table:
                doc.tables.append(DocumentTableBlock(caption: "표 \(doc.tables.count + 1). 추가 데이터 표", rows: [["구분", "목표치", "실적치"], ["1분기", "100", "120"], ["2분기", "150", "180"]]))
            case .chart:
                doc.paragraphs.append(ParagraphBlock(style: .quote, alignment: .leading, text: "[차트 개체: 분기별 핵심 성과 지표 시각화]"))
            case .shape:
                doc.paragraphs.append(ParagraphBlock(style: .quote, alignment: .center, text: "[다이어그램 도형: 프로세스 아키텍처 블록]"))
            case .link:
                let anchorID = doc.paragraphs.first?.id ?? UUID()
                doc.bookmarks["책갈피_\(doc.bookmarks.count + 1)"] = anchorID
            }
            openTabs[idx].payload = .document(doc)
            openTabs[idx].isModified = true

        case var .sheet(sheet):
            let coord = (try? CellAddress(a1Notation: selectedSheetCell))?.packedCoordinate ?? PackedCellCoordinate(row: 0, column: 0)
            switch kind {
            case .table:
                sheet.addSheetPage(named: "Sheet\(sheet.allSheetNames.count + 1)", initialCells: [PackedCellCoordinate(row: 0, column: 0): "항목", PackedCellCoordinate(row: 0, column: 1): "값"])
            case .chart:
                let allKinds = SheetChartKind.allCases
                if let cIdx = allKinds.firstIndex(of: selectedChartKind) {
                    selectedChartKind = allKinds[(cIdx + 1) % allKinds.count]
                }
            case .shape:
                var style = sheet.cellStyle(at: coord)
                let alignments = CellHorizontalAlignment.allCases
                let curIdx = alignments.firstIndex(of: style.alignment) ?? 0
                style.alignment = alignments[(curIdx + 1) % alignments.count]
                style.textHex = "#5C6BC0"
                sheet.setCellStyle(style, at: coord)
            case .link:
                sheet.sheetComments[coord] = "참조 링크 (\(selectedSheetCell))"
            }
            openTabs[idx].payload = .sheet(sheet)
            openTabs[idx].isModified = true

        case var .slide(deck):
            guard !deck.slides.isEmpty else { return }
            let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
            let elementKind: SlideElementKind
            let title: String
            switch kind {
            case .table: elementKind = .tableGrid; title = "데이터 표 그리드"
            case .chart: elementKind = .chartEmbed; title = "성과 지표 차트"
            case .shape: elementKind = .roundedRectangleShape; title = "아키텍처 도형"
            case .link: elementKind = .calloutShape; title = "참조 링크 콜아웃"
            }
            let offsetCount = Double(deck.slides[sIdx].elements.count)
            let newElem = SlideCanvasElement(title: title, bodyText: "슬라이드 \(sIdx + 1) 추가 개체", x: 240.0 + offsetCount * 16.0, y: 160.0 + offsetCount * 12.0, width: 200, height: 100, kind: elementKind)
            deck.slides[sIdx].elements.append(newElem)
            selectedSlideElementID = newElem.id
            openTabs[idx].payload = .slide(deck)
            openTabs[idx].isModified = true

        case var .pdf(pdfDoc):
            let pageNum = pdfDoc.pages.indices.contains(selectedPDFPageIndex) ? pdfDoc.pages[selectedPDFPageIndex].pageNumber : 1
            if kind == .link {
                pdfDoc.outline.append(PDFOutlineNode(title: "책갈피 \(pdfDoc.outline.count + 1)", targetPageNumber: pageNum))
            } else {
                pdfDoc.annotations.append(PDFAnnotationItem(pageNumber: pageNum, kind: .stickyNote, content: "삽입 개체 마크", author: authorName))
            }
            openTabs[idx].payload = .pdf(pdfDoc)
            openTabs[idx].isModified = true
        }
    }

    private enum LayoutActionKind {
        case paperSize, orientation, columns, margins
    }

    private func applyLayoutCommand(at idx: Int, kind: LayoutActionKind) {
        switch openTabs[idx].payload {
        case var .document(doc):
            if doc.sections.isEmpty { doc.sections = [DocumentSectionLayout(paperSize: doc.paperSize)] }
            switch kind {
            case .paperSize:
                let presets = PaperSizePreset.allCases
                let cur = doc.paperSize
                let next = presets[((presets.firstIndex(of: cur) ?? 0) + 1) % presets.count]
                doc.paperSize = next; doc.sections[0].paperSize = next
            case .orientation:
                doc.sections[0].orientation = (doc.sections[0].orientation == .portrait) ? .landscape : .portrait
            case .columns:
                doc.sections[0].columnCount = (doc.sections[0].columnCount % 3) + 1
            case .margins:
                let targetMargin = (doc.sections[0].marginTopPt < 45.0) ? 56.7 : 36.0
                doc.sections[0].marginTopPt = targetMargin; doc.sections[0].marginBottomPt = targetMargin
                doc.sections[0].marginLeftPt = targetMargin; doc.sections[0].marginRightPt = targetMargin
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
            let targetIndex = pdfDoc.pages.indices.contains(selectedPDFPageIndex) ? selectedPDFPageIndex : 0
            switch kind {
            case .orientation, .columns:
                pdfDoc.rotatePage(at: targetIndex, clockwiseDegrees: 90)
            case .paperSize:
                pdfDoc.outline.append(PDFOutlineNode(title: "용지규격 프리셋 \(pdfDoc.outline.count + 1)", targetPageNumber: targetIndex + 1))
            case .margins:
                pdfDoc.outline.append(PDFOutlineNode(title: "여백 레이아웃 \(pdfDoc.outline.count + 1)", targetPageNumber: targetIndex + 1))
            }
            openTabs[idx].payload = .pdf(pdfDoc)
            openTabs[idx].isModified = true

        case var .sheet(sheet):
            switch kind {
            case .paperSize:
                sheet.pageSetup.paperSize = (sheet.pageSetup.paperSize == .a4) ? .letter : .a4
            case .orientation:
                sheet.pageSetup.orientation = (sheet.pageSetup.orientation == .portrait) ? .landscape : .portrait
            case .columns:
                sheet.pageSetup.isMultiColumnPrint.toggle()
            case .margins:
                sheet.pageSetup.marginPresetName = (sheet.pageSetup.marginPresetName == "기본 여백") ? "좁은 여백" : "기본 여백"
            }
            openTabs[idx].payload = .sheet(sheet)
            openTabs[idx].isModified = true
        }
    }

    private enum DataActionKind {
        case autoSum, functionWizard, sort, validation
    }

    private func applyDataCommand(at idx: Int, kind: DataActionKind) {
        switch openTabs[idx].payload {
        case var .sheet(sheet):
            switch kind {
            case .autoSum:
                let numRange = sheet.detectedNumericRange().a1Range
                formulaBarDraft = "=SUM(\(numRange))"
                try? sheet.setCell(selectedSheetCell, rawValue: formulaBarDraft)
            case .functionWizard:
                let numRange = sheet.detectedNumericRange().a1Range
                formulaBarDraft = "=AVERAGE(\(numRange))"
                try? sheet.setCell(selectedSheetCell, rawValue: formulaBarDraft)
            case .sort:
                sheetSortAscending.toggle()
                let bounds = sheet.detectedDataRange()
                let cols = Array(bounds.start.column...bounds.end.column)
                let primaryMetricCol = min(bounds.end.column, bounds.start.column + 1)
                let dataCandidateRows = (bounds.start.row...bounds.end.row).filter { r in
                    let labelRaw = (sheet.cellsByCoordinate[PackedCellCoordinate(row: r, column: bounds.start.column)] ?? "")
                        .trimmingCharacters(in: .whitespaces)
                    if labelRaw == "합계" || labelRaw == "총계" || labelRaw.uppercased() == "TOTAL" {
                        return false
                    }
                    let metricRaw = (sheet.cellsByCoordinate[PackedCellCoordinate(row: r, column: primaryMetricCol)] ?? "")
                        .trimmingCharacters(in: .whitespaces)
                        .uppercased()
                    if metricRaw.hasPrefix("=SUM(") || metricRaw.hasPrefix("=AVERAGE(") {
                        return false
                    }
                    return true
                }
                let sortableRows: [Int]
                if dataCandidateRows.count > 1 {
                    let firstRow = dataCandidateRows[0]
                    let firstRowHasNumeric = cols.contains(where: { c in
                        guard let raw = sheet.cellsByCoordinate[PackedCellCoordinate(row: firstRow, column: c)] else { return false }
                        return Double(raw.trimmingCharacters(in: .whitespaces)) != nil
                    })
                    let laterRowHasNumeric = dataCandidateRows.dropFirst().contains(where: { r in
                        cols.contains(where: { c in
                            guard let raw = sheet.cellsByCoordinate[PackedCellCoordinate(row: r, column: c)] else { return false }
                            return Double(raw.trimmingCharacters(in: .whitespaces)) != nil
                        })
                    })
                    sortableRows = (!firstRowHasNumeric && laterRowHasNumeric)
                        ? Array(dataCandidateRows.dropFirst())
                        : dataCandidateRows
                } else {
                    sortableRows = dataCandidateRows
                }
                if sortableRows.count >= 2 {
                    let keyCol: Int = {
                        if let selAddr = try? CellAddress(a1Notation: selectedSheetCell),
                           cols.contains(selAddr.column),
                           sortableRows.contains(where: {
                               guard let raw = sheet.cellsByCoordinate[PackedCellCoordinate(row: $0, column: selAddr.column)] else { return false }
                               return !raw.trimmingCharacters(in: .whitespaces).hasPrefix("=")
                           }) {
                            return selAddr.column
                        }
                        return cols.first(where: { c in
                            sortableRows.contains(where: { r in
                                guard let raw = sheet.cellsByCoordinate[PackedCellCoordinate(row: r, column: c)] else { return false }
                                return Double(raw.trimmingCharacters(in: .whitespaces)) != nil
                            })
                        }) ?? bounds.start.column
                    }()
                    struct SortableRowSnapshot {
                        let originalRow: Int
                        let rawCells: [Int: String]
                        let styles: [Int: CellStyle]
                        let keyVal: EvaluatedCellValue
                    }
                    var snapshots: [SortableRowSnapshot] = []
                    for r in sortableRows {
                        var rawMap: [Int: String] = [:]
                        var styleMap: [Int: CellStyle] = [:]
                        for c in cols {
                            let coord = PackedCellCoordinate(row: r, column: c)
                            if let raw = sheet.cellsByCoordinate[coord], !raw.trimmingCharacters(in: .whitespaces).hasPrefix("=") {
                                rawMap[c] = raw
                            }
                            if let st = sheet.cellStyles[coord] {
                                styleMap[c] = st
                            }
                        }
                        let keyCoord = PackedCellCoordinate(row: r, column: keyCol)
                        let keyVal = (try? sheet.evaluate(at: keyCoord)) ?? .empty
                        snapshots.append(SortableRowSnapshot(originalRow: r, rawCells: rawMap, styles: styleMap, keyVal: keyVal))
                    }
                    snapshots.sort { lhs, rhs in
                        let cmp: Bool
                        switch (lhs.keyVal, rhs.keyVal) {
                        case let (.number(a), .number(b)):
                            cmp = a < b
                        default:
                            cmp = lhs.keyVal.description < rhs.keyVal.description
                        }
                        return sheetSortAscending ? cmp : !cmp
                    }
                    if snapshots.map(\.originalRow) == sortableRows {
                        sheetSortAscending.toggle()
                        snapshots.reverse()
                    }
                    for (idxOffset, targetRow) in sortableRows.enumerated() {
                        let snap = snapshots[idxOffset]
                        for c in cols {
                            let coord = PackedCellCoordinate(row: targetRow, column: c)
                            if let existing = sheet.cellsByCoordinate[coord], existing.trimmingCharacters(in: .whitespaces).hasPrefix("=") {
                                continue
                            }
                            let addr = CellAddress(packed: coord).a1Notation
                            try? sheet.setCell(addr, rawValue: snap.rawCells[c] ?? "")
                            if let st = snap.styles[c] {
                                try? sheet.setCellStyle(st, at: addr)
                            }
                        }
                    }
                }
            case .validation:
                activeDataValidationRule = (activeDataValidationRule == nil)
                    ? DataValidationRule(minimumValue: 0, maximumValue: 1_000_000_000, allowedSet: nil)
                    : nil
            }
            openTabs[idx].payload = .sheet(sheet)
            openTabs[idx].isModified = true

        case var .document(doc):
            switch kind {
            case .autoSum:
                let lastCol = doc.tables.isEmpty ? 0 : max(0, doc.tables[0].columnCount - 1)
                let sum = doc.tables.isEmpty ? 0.0 : (doc.tables[0].evaluateTableFormula("=SUM(ABOVE)", atRow: doc.tables[0].rowCount, column: lastCol) ?? 0.0)
                doc.footerText = "표 합계 검산: \(Int(sum)) (#\(doc.footerText.count + 1))"
            case .functionWizard:
                let lastCol = doc.tables.isEmpty ? 0 : max(0, doc.tables[0].columnCount - 1)
                let avg = doc.tables.isEmpty ? 0.0 : (doc.tables[0].evaluateTableFormula("=AVERAGE(ABOVE)", atRow: doc.tables[0].rowCount, column: lastCol) ?? 0.0)
                doc.footerText = "표 평균 검산: \(String(format: "%.1f", avg))"
            case .sort:
                sheetSortAscending.toggle()
                if !doc.tables.isEmpty, doc.tables[0].rows.count > 2 {
                    let header = doc.tables[0].rows[0]
                    let body = Array(doc.tables[0].rows.dropFirst())
                    let sortedBody = body.sorted { lhs, rhs in
                        let lVal = lhs.first ?? ""
                        let rVal = rhs.first ?? ""
                        return sheetSortAscending ? (lVal < rVal) : (lVal > rVal)
                    }
                    doc.tables[0].rows = [header] + sortedBody
                }
                doc.footerText = "표 데이터 정렬: \(sheetSortAscending ? "오름차순" : "내림차순")"
            case .validation:
                activeDataValidationRule = (activeDataValidationRule == nil) ? .nonEmptyText : nil
                doc.footerText = "표 데이터 유효성 검사 규칙 활성"
            }
            openTabs[idx].payload = .document(doc)
            openTabs[idx].isModified = true

        case var .slide(deck):
            activeDataValidationRule = (activeDataValidationRule == nil) ? .nonEmptyText : nil
            if !deck.slides.isEmpty {
                let sIdx = deck.slides.indices.contains(selectedSlideIndex) ? selectedSlideIndex : 0
                deck.slides[sIdx].speakerNotes += " [데이터:\(kind)]"
                openTabs[idx].payload = .slide(deck)
            }
            openTabs[idx].isModified = true

        case var .pdf(pdfDoc):
            activeDataValidationRule = (activeDataValidationRule == nil) ? .nonEmptyText : nil
            pdfDoc.annotations.append(PDFAnnotationItem(pageNumber: 1, kind: .stickyNote, content: "데이터 검산(\(kind)) 완료", author: authorName))
            openTabs[idx].payload = .pdf(pdfDoc)
            openTabs[idx].isModified = true
        }
    }

    // MARK: - Batch Conversion & Security Audit Integration

    public func runBatchConversionForOpenTabs() {
        let jobs = openTabs.map { tab in
            ConversionJobRequest(sourceFileName: tab.fileName, sourceFormat: tab.format, targetFormat: batchTargetFormat, payload: tab.payload)
        }
        lastBatchConversionReport = DocumentConversionOrchestrator.executeBatch(jobs)
    }

    public func runSecurityAuditOnActiveTab() {
        guard let current = activeTab else { return }
        if let bytes = try? exportTabData(id: current.id, targetFormat: current.format) {
            lastSecurityAuditReport = DocumentSecurityAuditor.auditContainer(data: bytes, fileName: current.fileName)
        }
        reviewStatsRefreshCount += 1
    }

    public func scrubActiveTabMetadata() {
        guard let activeTabID,
              let targetIdx = openTabs.firstIndex(where: { $0.id == activeTabID }),
              case let .document(doc) = openTabs[targetIdx].payload else { return }
        recordUndoSnapshot(for: openTabs[targetIdx].id)
        let (scrubbed, removedCount) = DocumentSecurityAuditor.scrubMetadata(from: doc)
        openTabs[targetIdx].payload = .document(scrubbed)
        openTabs[targetIdx].isModified = true
        lastScrubbedMetadataCount = removedCount
        lastExecutedCommandSummary = "개인정보·메타데이터 \(removedCount)건 비식별화 완료"
    }
}

