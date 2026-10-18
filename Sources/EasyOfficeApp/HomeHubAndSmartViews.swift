import SwiftUI
import Observation
import EasyOfficeDesignSystem
import EasyOfficeCore

struct HomeHubWorkspaceView: View {
    @Environment(\.colorScheme) private var colorScheme
    @Bindable var viewModel: WorkspaceViewModel

    var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)

        HStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 6) {
                Text("EasyOffice")
                    .font(.system(size: 18, weight: .bold))
                    .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)
                    .padding(.bottom, 8)

                ForEach(HubSection.allCases) { section in
                    let selected = viewModel.activeHubSection == section
                    Button {
                        viewModel.activeHubSection = section
                    } label: {
                        HStack(spacing: 10) {
                            Image(systemName: section.symbolName)
                            Text(section.rawValue)
                                .font(.system(size: 13, weight: selected ? .semibold : .regular))
                            Spacer()
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 7)
                        .background(
                            selected
                                ? EasyOfficeDesignTokens.brand100.swiftUIColor
                                : Color.clear
                        )
                        .foregroundStyle(
                            selected
                                ? EasyOfficeDesignTokens.brand800.swiftUIColor
                                : palette.primaryText.swiftUIColor
                        )
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                    }
                    .buttonStyle(.plain)
                }
                Spacer()
            }
            .padding(16)
            .frame(width: 220)
            .background(palette.cardSurface.swiftUIColor)

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    if viewModel.activeHubSection == .converter {
                        BatchConversionStudioHubView(viewModel: viewModel)
                    } else if viewModel.activeHubSection == .smartAssistant {
                        SmartDocumentAssistantHubView(viewModel: viewModel)
                    }

                    Text("새 문서 시작하기")
                        .font(.system(size: 16, weight: .bold))
                        .foregroundStyle(palette.primaryText.swiftUIColor)

                    HStack(spacing: 12) {
                        ForEach(OfficeModuleKind.allCases) { module in
                            let accent = EasyOfficeDesignTokens.accentToken(for: module)
                            Button {
                                viewModel.openNewSampleTab(for: module)
                            } label: {
                                VStack(alignment: .leading, spacing: 8) {
                                    Image(systemName: module.systemSymbolName)
                                        .font(.system(size: 22))
                                        .foregroundStyle(accent.swiftUIColor)
                                    Text(module.displayNameKo)
                                        .font(.system(size: 13, weight: .semibold))
                                        .foregroundStyle(palette.primaryText.swiftUIColor)
                                }
                                .padding(14)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(palette.cardSurface.swiftUIColor)
                                .overlay(
                                    RoundedRectangle(cornerRadius: 10)
                                        .stroke(palette.border.swiftUIColor, lineWidth: 1)
                                )
                                .clipShape(RoundedRectangle(cornerRadius: 10))
                            }
                            .buttonStyle(.plain)
                        }
                    }

                    HStack {
                        Text("표준 및 사용자 서식 템플릿 (\(viewModel.allAvailableTemplates.count)종)")
                            .font(.system(size: 15, weight: .bold))
                            .foregroundStyle(palette.primaryText.swiftUIColor)
                        Spacer()
                        Button("현재 탭을 사용자 템플릿으로 저장") {
                            viewModel.saveActiveTabAsCustomTemplate(
                                title: "사용자 정의 표준 양식",
                                subtitle: "현재 활성 문서 기반 커스텀 템플릿"
                            )
                        }
                        .font(.system(size: 11, weight: .semibold))
                    }

                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 240), spacing: 12)], spacing: 12) {
                        ForEach(viewModel.allAvailableTemplates) { tpl in
                            VStack(alignment: .leading, spacing: 6) {
                                HStack {
                                    Text(tpl.format.rawValue.uppercased())
                                        .font(.system(size: 10, weight: .bold))
                                        .padding(.horizontal, 6)
                                        .padding(.vertical, 2)
                                        .background(EasyOfficeDesignTokens.softBadgeToken(for: tpl.moduleKind).swiftUIColor)
                                        .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: tpl.moduleKind).swiftUIColor)
                                        .clipShape(RoundedRectangle(cornerRadius: 4))
                                    if tpl.isCustomUserTemplate {
                                        Text("사용자 템플릿")
                                            .font(.system(size: 9, weight: .bold))
                                            .foregroundStyle(EasyOfficeDesignTokens.statusInfo.swiftUIColor)
                                    }
                                    Spacer()
                                }
                                Text(tpl.title)
                                    .font(.system(size: 13, weight: .semibold))
                                    .foregroundStyle(palette.primaryText.swiftUIColor)
                                Text(tpl.subtitle)
                                    .font(.system(size: 11))
                                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                                    .lineLimit(2)
                            }
                            .padding(12)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(palette.cardSurface.swiftUIColor)
                            .overlay(
                                RoundedRectangle(cornerRadius: 8)
                                    .stroke(palette.border.swiftUIColor, lineWidth: 1)
                            )
                            .clipShape(RoundedRectangle(cornerRadius: 8))
                            .onTapGesture {
                                viewModel.openTab(from: tpl)
                            }
                        }
                    }

                    Text("\(viewModel.activeHubSection.rawValue) · 문서 목록 (\(viewModel.filteredTabs.count)건)")
                        .font(.system(size: 15, weight: .bold))
                        .foregroundStyle(palette.primaryText.swiftUIColor)

                    VStack(spacing: 8) {
                        ForEach(viewModel.filteredTabs) { tab in
                            HStack {
                                Text(tab.moduleKind.shortBadgeTitle)
                                    .font(.system(size: 10, weight: .bold))
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 3)
                                    .background(EasyOfficeDesignTokens.softBadgeToken(for: tab.moduleKind).swiftUIColor)
                                    .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: tab.moduleKind).swiftUIColor)
                                    .clipShape(RoundedRectangle(cornerRadius: 4))

                                Text(tab.fileName)
                                    .font(.system(size: 13, weight: .medium))
                                    .foregroundStyle(palette.primaryText.swiftUIColor)
                                Spacer()
                                Text(tab.format.standardSpecification)
                                    .font(.system(size: 11))
                                    .foregroundStyle(palette.secondaryText.swiftUIColor)
                            }
                            .padding(12)
                            .background(palette.cardSurface.swiftUIColor)
                            .clipShape(RoundedRectangle(cornerRadius: 8))
                            .onTapGesture {
                                viewModel.activeTabID = tab.id
                                viewModel.isShowingHomeHub = false
                            }
                        }
                    }
                }
                .padding(24)
            }
        }
    }
}

struct BatchConversionStudioHubView: View {
    @Environment(\.colorScheme) private var colorScheme
    @Bindable var viewModel: WorkspaceViewModel

    var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)

        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Label("포맷 일괄 변환 스튜디오 (Fail-Closed 컨테이너 검증)", systemImage: "arrow.triangle.2.circlepath.doc.on.clipboard")
                    .font(.system(size: 15, weight: .bold))
                    .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)
                Spacer()
                Picker("목표 포맷", selection: $viewModel.batchTargetFormat) {
                    ForEach([DocumentFormat.pdf, .docx, .hwpx, .odt, .xlsx, .csv, .md], id: \.rawValue) { fmt in
                        Text(fmt.rawValue.uppercased()).tag(fmt)
                    }
                }
                .pickerStyle(.menu)
                .frame(width: 140)

                Button("열린 탭 전체 일괄 변환 실행") {
                    viewModel.runBatchConversionForOpenTabs()
                }
                .buttonStyle(.borderedProminent)
                .tint(EasyOfficeDesignTokens.brand700.swiftUIColor)
            }

            if let report = viewModel.lastBatchConversionReport {
                HStack(spacing: 16) {
                    Text("총 작업: \(report.totalJobs)건")
                        .font(.system(size: 12, weight: .semibold))
                    Text("성공: \(report.succeededResults.count)건")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(EasyOfficeDesignTokens.statusSuccess.swiftUIColor)
                    Text("실패: \(report.failedErrors.count)건")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(
                            !report.failedErrors.isEmpty
                                ? EasyOfficeDesignTokens.statusDanger.swiftUIColor
                                : palette.secondaryText.swiftUIColor
                        )
                }

                ForEach(report.succeededResults) { item in
                    HStack {
                        Text(item.sourceFileName)
                            .font(.system(size: 12, weight: .medium))
                        Image(systemName: "arrow.right")
                            .font(.system(size: 10))
                        Text(item.outputFileName)
                            .font(.system(size: 11, weight: .bold, design: .monospaced))
                        Spacer()
                        Text("완료 (\(item.outputBytes.count) bytes)")
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(EasyOfficeDesignTokens.statusSuccess.swiftUIColor)
                    }
                    .padding(.vertical, 4)
                }

                ForEach(Array(report.failedErrors.enumerated()), id: \.offset) { _, err in
                    Text("차단됨: \(err.errorDescription)")
                        .font(.system(size: 11))
                        .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
                }
            } else {
                Text("열려 있는 모든 문서 탭을 선택한 표준 포맷(PDF, DOCX, HWPX, ODT, XLSX, CSV, MD)으로 일괄 변환하고 바이트 무결성을 검증합니다.")
                    .font(.system(size: 12))
                    .foregroundStyle(palette.secondaryText.swiftUIColor)
            }
        }
        .padding(16)
        .background(palette.cardSurface.swiftUIColor)
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(EasyOfficeDesignTokens.brand400.swiftUIColor, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}

struct SmartDocumentAssistantHubView: View {
    @Environment(\.colorScheme) private var colorScheme
    @Bindable var viewModel: WorkspaceViewModel

    var body: some View {
        let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
        let documentModels: [OfficeDocumentModel] = viewModel.openTabs.compactMap { tab in
            if case let .document(doc) = tab.payload { return doc }
            return nil
        }
        let activeDoc: OfficeDocumentModel? = {
            if let tab = viewModel.activeTab, case let .document(doc) = tab.payload {
                return doc
            }
            return documentModels.first
        }()
        let activeSheet: WorksheetModel? = {
            if let tab = viewModel.activeTab, case let .sheet(sheet) = tab.payload {
                return sheet
            }
            for tab in viewModel.openTabs {
                if case let .sheet(sheet) = tab.payload { return sheet }
            }
            return nil
        }()

        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Label("스마트 문서 보조 도구 (요약·가독성·조치항목·시트통계·문서비교·보안감사)", systemImage: "sparkles.rectangle.stack.fill")
                    .font(.system(size: 15, weight: .bold))
                    .foregroundStyle(EasyOfficeDesignTokens.brand800.swiftUIColor)
                Spacer()
                Button("개인정보·메타데이터 비식별화") {
                    viewModel.scrubActiveTabMetadata()
                }
                .buttonStyle(.bordered)

                Button("활성 탭 보안 감사 실행") {
                    viewModel.runSecurityAuditOnActiveTab()
                }
                .buttonStyle(.borderedProminent)
                .tint(EasyOfficeDesignTokens.brand700.swiftUIColor)
            }

            if let doc = activeDoc {
                let summary = LocalSmartDocumentAssistant.summarize(doc, maxSentences: 3)
                let suggestions = LocalSmartDocumentAssistant.analyzeWritingQuality(doc)
                let actionItems = LocalSmartDocumentAssistant.extractActionItemsAndEntities(doc)

                Text("대상 문서: \(doc.title) · 예상 읽기 시간 \(summary.estimatedReadingSeconds)초 · 압축률 \(String(format: "%.0f", summary.compressionRatioPercent))%")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(palette.primaryText.swiftUIColor)

                ForEach(summary.keySentences, id: \.self) { sentence in
                    Text("• \(sentence)")
                        .font(.system(size: 12))
                        .foregroundStyle(palette.secondaryText.swiftUIColor)
                }

                if !summary.keywords.isEmpty {
                    Text("핵심 키워드: \(summary.keywords.joined(separator: ", "))")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                }

                HStack(alignment: .top, spacing: 12) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("문장 가독성 교정 제안 (\(suggestions.count)건)")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundStyle(palette.primaryText.swiftUIColor)
                        ForEach(suggestions.prefix(2), id: \.paragraphID) { item in
                            Text("• [\(item.issueKind)] \(item.suggestedRewrite)")
                                .font(.system(size: 11))
                                .foregroundStyle(palette.secondaryText.swiftUIColor)
                                .lineLimit(1)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)

                    VStack(alignment: .leading, spacing: 4) {
                        Text("추출된 일정·예산·조치 항목 (\(actionItems.count)건)")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundStyle(palette.primaryText.swiftUIColor)
                        ForEach(Array(actionItems.prefix(2).enumerated()), id: \.offset) { _, act in
                            Text("• [\(act.category)] \(act.extractedText)")
                                .font(.system(size: 11))
                                .foregroundStyle(palette.secondaryText.swiftUIColor)
                                .lineLimit(1)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }

            HStack(alignment: .top, spacing: 12) {
                if let sheet = activeSheet {
                    let range = sheet.detectedNumericRange()
                    let insight = LocalSmartDocumentAssistant.analyzeWorksheetInsights(sheet, range: range)
                    VStack(alignment: .leading, spacing: 4) {
                        Text("시트 Z-Score 통계 인사이트 (\(insight.trendClassification))")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)
                        Text("평균 \(String(format: "%.1f", insight.mean)) · 표준편차 \(String(format: "%.1f", insight.standardDeviation)) · 증감률 \(String(format: "%.1f", insight.growthRatePercent))%")
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(palette.secondaryText.swiftUIColor)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }

                if documentModels.count >= 2 {
                    let diff = LocalSmartDocumentAssistant.compareDocuments(
                        baseline: documentModels[0],
                        revised: documentModels[1]
                    )
                    VStack(alignment: .leading, spacing: 4) {
                        Text("문서 LCS 비교 (Document ↔ HWP)")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                        Text("추가 +\(diff.addedCount) · 삭제 -\(diff.removedCount) · 수정 ~\(diff.modifiedCount) · 동일 =\(diff.unchangedCount)")
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(palette.secondaryText.swiftUIColor)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }

            if viewModel.lastScrubbedMetadataCount > 0 || viewModel.lastSecurityAuditReport != nil {
                Divider()
                HStack(spacing: 12) {
                    if let audit = viewModel.lastSecurityAuditReport {
                        Image(systemName: audit.isPassed ? "checkmark.shield.fill" : "exclamationmark.triangle.fill")
                            .foregroundStyle(
                                audit.isPassed
                                    ? EasyOfficeDesignTokens.statusSuccess.swiftUIColor
                                    : EasyOfficeDesignTokens.statusDanger.swiftUIColor
                            )
                        Text("매직 바이트 보안 감사: \(audit.detectedFormat?.rawValue.uppercased() ?? "UNKNOWN") · 통과 여부: \(audit.isPassed ? "안전" : "위험 감지")")
                            .font(.system(size: 12, weight: .semibold))
                    }
                    if viewModel.lastScrubbedMetadataCount > 0 {
                        Text("· 비식별화된 개인정보/메타데이터: \(viewModel.lastScrubbedMetadataCount)건")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                    }
                }
            }
        }
        .padding(16)
        .background(palette.cardSurface.swiftUIColor)
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(EasyOfficeDesignTokens.brand400.swiftUIColor, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}
