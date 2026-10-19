import EasyOfficeCore
import EasyOfficeDesignSystem
import Observation
import SwiftUI

struct HomeHubWorkspaceView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel

  var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)

    HStack(spacing: 0) {
      VStack(alignment: .leading, spacing: 6) {
        Text("EasyOffice")
          .font(.system(size: 18, weight: .bold))
          .foregroundStyle(palette.accentBadgeText.swiftUIColor)
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
                ? palette.selectedSurface.swiftUIColor
                : Color.clear
            )
            .foregroundStyle(
              selected
                ? palette.accentBadgeText.swiftUIColor
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

          let filteredTemplates = viewModel.allAvailableTemplates.filter { tpl in
            switch viewModel.selectedHomeTemplateCategoryFilter {
            case "문서·한글":
              return tpl.moduleKind == .document || tpl.moduleKind == .hwp
            case "시트":
              return tpl.moduleKind == .sheet
            case "슬라이드":
              return tpl.moduleKind == .slide
            case "PDF":
              return tpl.moduleKind == .pdf
            default:
              return true
            }
          }

          let filteredRecentTabs = viewModel.filteredTabs.filter { tab in
            switch viewModel.selectedHomeRecentFormatFilter {
            case "DOCX/HWP":
              return tab.moduleKind == .document || tab.moduleKind == .hwp
            case "XLSX":
              return tab.moduleKind == .sheet
            case "PPTX":
              return tab.moduleKind == .slide
            case "PDF":
              return tab.moduleKind == .pdf
            default:
              return true
            }
          }

          VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
              Text(
                "표준 및 사용자 서식 템플릿 (\(filteredTemplates.count) / \(viewModel.allAvailableTemplates.count)종)"
              )
              .font(.system(size: 15, weight: .bold))
              .foregroundStyle(palette.primaryText.swiftUIColor)
              Spacer()
              TextField("사용자 템플릿 이름 (선택)", text: $viewModel.customTemplateTitleDraft)
                .textFieldStyle(.roundedBorder)
                .font(.system(size: 11))
                .frame(width: 180)
              Button("현재 탭을 사용자 템플릿으로 저장") {
                viewModel.saveActiveTabAsCustomTemplate(
                  title: nil,
                  subtitle: "현재 활성 문서 기반 커스텀 템플릿"
                )
              }
              .font(.system(size: 11, weight: .semibold))
            }

            HStack(spacing: 6) {
              ForEach(["전체", "문서·한글", "시트", "슬라이드", "PDF"], id: \.self) { cat in
                let isSelected = viewModel.selectedHomeTemplateCategoryFilter == cat
                Button {
                  viewModel.selectedHomeTemplateCategoryFilter = cat
                } label: {
                  Text(cat)
                    .font(.system(size: 11, weight: isSelected ? .bold : .medium))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(
                      isSelected
                        ? EasyOfficeDesignTokens.brand700.swiftUIColor
                        : palette.cardSurface.swiftUIColor
                    )
                    .foregroundStyle(
                      isSelected
                        ? Color.white
                        : palette.primaryText.swiftUIColor
                    )
                    .overlay(
                      RoundedRectangle(cornerRadius: 5)
                        .stroke(palette.border.swiftUIColor, lineWidth: isSelected ? 0 : 1)
                    )
                    .clipShape(RoundedRectangle(cornerRadius: 5))
                }
                .buttonStyle(.plain)
              }
            }
          }

          LazyVGrid(columns: [GridItem(.adaptive(minimum: 240), spacing: 12)], spacing: 12) {
            ForEach(filteredTemplates) { tpl in
              VStack(alignment: .leading, spacing: 8) {
                TemplateMiniaturePreviewHeader(
                  module: tpl.moduleKind,
                  formatBadge: tpl.format.rawValue.uppercased(),
                  palette: palette
                )

                HStack {
                  Text(tpl.format.rawValue.uppercased())
                    .font(.system(size: 10, weight: .bold))
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(
                      EasyOfficeDesignTokens.softBadgeToken(for: tpl.moduleKind).swiftUIColor
                    )
                    .foregroundStyle(
                      EasyOfficeDesignTokens.accentToken(for: tpl.moduleKind).swiftUIColor
                    )
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

          HStack(spacing: 8) {
            Text("\(viewModel.activeHubSection.rawValue) · 문서 목록 (\(filteredRecentTabs.count)건)")
              .font(.system(size: 15, weight: .bold))
              .foregroundStyle(palette.primaryText.swiftUIColor)
            Spacer()
            HStack(spacing: 6) {
              ForEach(["전체", "DOCX/HWP", "XLSX", "PPTX", "PDF"], id: \.self) { fmt in
                let isSelected = viewModel.selectedHomeRecentFormatFilter == fmt
                Button {
                  viewModel.selectedHomeRecentFormatFilter = fmt
                } label: {
                  Text(fmt)
                    .font(.system(size: 11, weight: isSelected ? .bold : .medium))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(
                      isSelected
                        ? EasyOfficeDesignTokens.brand700.swiftUIColor
                        : palette.cardSurface.swiftUIColor
                    )
                    .foregroundStyle(
                      isSelected
                        ? Color.white
                        : palette.primaryText.swiftUIColor
                    )
                    .overlay(
                      RoundedRectangle(cornerRadius: 5)
                        .stroke(palette.border.swiftUIColor, lineWidth: isSelected ? 0 : 1)
                    )
                    .clipShape(RoundedRectangle(cornerRadius: 5))
                }
                .buttonStyle(.plain)
              }
            }
          }

          VStack(spacing: 8) {
            ForEach(filteredRecentTabs) { tab in
              let moduleAccent = EasyOfficeDesignTokens.accentToken(for: tab.moduleKind)
                .swiftUIColor
              HStack(spacing: 10) {
                Image(systemName: tab.moduleKind.systemSymbolName)
                  .font(.system(size: 14, weight: .semibold))
                  .foregroundStyle(moduleAccent)
                  .frame(width: 22)

                Text(tab.moduleKind.shortBadgeTitle)
                  .font(.system(size: 10, weight: .bold))
                  .padding(.horizontal, 6)
                  .padding(.vertical, 3)
                  .background(
                    EasyOfficeDesignTokens.softBadgeToken(for: tab.moduleKind).swiftUIColor
                  )
                  .foregroundStyle(moduleAccent)
                  .clipShape(RoundedRectangle(cornerRadius: 4))

                Text(tab.format.rawValue.uppercased())
                  .font(.system(size: 9, weight: .bold, design: .monospaced))
                  .padding(.horizontal, 5)
                  .padding(.vertical, 2)
                  .background(palette.elevatedSurface.swiftUIColor)
                  .foregroundStyle(palette.secondaryText.swiftUIColor)
                  .clipShape(RoundedRectangle(cornerRadius: 4))

                Text(tab.fileName)
                  .font(.system(size: 13, weight: .medium))
                  .foregroundStyle(palette.primaryText.swiftUIColor)

                Text(tab.isModified ? "수정됨" : "정상")
                  .font(.system(size: 10, weight: .semibold))
                  .padding(.horizontal, 6)
                  .padding(.vertical, 2)
                  .background(
                    (tab.isModified
                      ? EasyOfficeDesignTokens.brand700.swiftUIColor
                      : EasyOfficeDesignTokens.statusSuccess.swiftUIColor).opacity(0.14)
                  )
                  .foregroundStyle(
                    tab.isModified
                      ? EasyOfficeDesignTokens.brand700.swiftUIColor
                      : EasyOfficeDesignTokens.statusSuccess.swiftUIColor
                  )
                  .clipShape(RoundedRectangle(cornerRadius: 4))

                Spacer()

                Text(tab.format.standardSpecification)
                  .font(.system(size: 11))
                  .foregroundStyle(palette.secondaryText.swiftUIColor)

                Button {
                  viewModel.togglePinTab(id: tab.id)
                } label: {
                  Text("📌")
                    .font(.system(size: 11))
                    .padding(.horizontal, 6)
                    .padding(.vertical, 3)
                    .background(
                      tab.isPinned
                        ? palette.selectedSurface.swiftUIColor
                        : palette.elevatedSurface.swiftUIColor
                    )
                    .clipShape(RoundedRectangle(cornerRadius: 4))
                }
                .buttonStyle(.plain)
                .help(tab.isPinned ? "고정 해제" : "고정")

                Button("스플릿 뷰로 열기") {
                  if viewModel.activeTabID == tab.id,
                    let other = viewModel.openTabs.first(where: { $0.id != tab.id })
                  {
                    viewModel.activeTabID = other.id
                  }
                  viewModel.enableSplitView(secondaryTabID: tab.id)
                  viewModel.isShowingHomeHub = false
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
              }
              .padding(12)
              .background(palette.cardSurface.swiftUIColor)
              .overlay(
                RoundedRectangle(cornerRadius: 8)
                  .stroke(palette.border.swiftUIColor, lineWidth: 1)
              )
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

private struct TemplateMiniaturePreviewHeader: View {
  let module: OfficeModuleKind
  let formatBadge: String
  var palette: WorkspaceThemePalette = WorkspaceThemeMode.lightLavender.surfacePalette(
    isSystemDark: false
  )

  var body: some View {
    let accent = EasyOfficeDesignTokens.accentToken(for: module).swiftUIColor

    ZStack(alignment: .topTrailing) {
      RoundedRectangle(cornerRadius: 6)
        .fill(palette.elevatedSurface.swiftUIColor)

      HStack(spacing: 10) {
        switch module {
        case .document:
          VStack(alignment: .leading, spacing: 3) {
            RoundedRectangle(cornerRadius: 2)
              .fill(accent)
              .frame(width: 44, height: 5)
            RoundedRectangle(cornerRadius: 1)
              .fill(palette.secondaryText.swiftUIColor.opacity(0.45))
              .frame(width: 56, height: 2.5)
            RoundedRectangle(cornerRadius: 1)
              .fill(palette.secondaryText.swiftUIColor.opacity(0.35))
              .frame(width: 48, height: 2.5)
            RoundedRectangle(cornerRadius: 1)
              .fill(palette.secondaryText.swiftUIColor.opacity(0.30))
              .frame(width: 52, height: 2.5)
            HStack(spacing: 2) {
              Rectangle().fill(accent.opacity(0.22)).frame(width: 24, height: 8)
              Rectangle().fill(accent.opacity(0.14)).frame(width: 24, height: 8)
            }
          }
          .padding(6)
          .background(palette.cardSurface.swiftUIColor)
          .overlay(
            RoundedRectangle(cornerRadius: 4)
              .stroke(palette.border.swiftUIColor, lineWidth: 1)
          )

        case .hwp:
          VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 4) {
              Text("공문")
                .font(.system(size: 7, weight: .bold))
                .foregroundStyle(accent)
              Spacer(minLength: 2)
              HStack(spacing: 1) {
                ForEach(0..<3, id: \.self) { _ in
                  Rectangle()
                    .stroke(accent.opacity(0.65), lineWidth: 0.7)
                    .frame(width: 8, height: 8)
                }
              }
            }
            HStack(spacing: 2) {
              ForEach(0..<6, id: \.self) { _ in
                RoundedRectangle(cornerRadius: 1)
                  .stroke(Color(red: 0.85, green: 0.28, blue: 0.22).opacity(0.55), lineWidth: 0.7)
                  .frame(width: 7, height: 7)
              }
            }
            RoundedRectangle(cornerRadius: 1)
              .fill(palette.secondaryText.swiftUIColor.opacity(0.35))
              .frame(width: 50, height: 2.5)
          }
          .padding(6)
          .frame(width: 76)
          .background(palette.cardSurface.swiftUIColor)
          .overlay(
            RoundedRectangle(cornerRadius: 4)
              .stroke(accent.opacity(0.5), lineWidth: 1)
          )

        case .sheet:
          HStack(spacing: 8) {
            VStack(spacing: 2) {
              ForEach(0..<3, id: \.self) { row in
                HStack(spacing: 2) {
                  ForEach(0..<4, id: \.self) { _ in
                    Rectangle()
                      .fill(row == 0 ? accent.opacity(0.35) : palette.cardSurface.swiftUIColor)
                      .frame(width: 11, height: 8)
                      .overlay(
                        Rectangle()
                          .stroke(accent.opacity(0.35), lineWidth: 0.5)
                      )
                  }
                }
              }
            }
            HStack(alignment: .bottom, spacing: 3) {
              RoundedRectangle(cornerRadius: 1.5).fill(accent.opacity(0.55)).frame(
                width: 5, height: 14)
              RoundedRectangle(cornerRadius: 1.5).fill(accent).frame(width: 5, height: 26)
              RoundedRectangle(cornerRadius: 1.5).fill(accent.opacity(0.75)).frame(
                width: 5, height: 20)
            }
          }
          .padding(6)
          .background(palette.cardSurface.swiftUIColor)
          .clipShape(RoundedRectangle(cornerRadius: 4))

        case .slide:
          VStack(alignment: .leading, spacing: 4) {
            RoundedRectangle(cornerRadius: 2)
              .fill(accent)
              .frame(width: 68, height: 7)
            HStack(spacing: 4) {
              RoundedRectangle(cornerRadius: 2)
                .fill(accent.opacity(0.25))
                .frame(width: 32, height: 18)
              RoundedRectangle(cornerRadius: 2)
                .fill(EasyOfficeDesignTokens.brand700.swiftUIColor.opacity(0.25))
                .frame(width: 32, height: 18)
            }
          }
          .padding(6)
          .aspectRatio(16.0 / 9.0, contentMode: .fit)
          .background(palette.cardSurface.swiftUIColor)
          .overlay(
            RoundedRectangle(cornerRadius: 4)
              .stroke(accent.opacity(0.55), lineWidth: 1)
          )

        case .pdf:
          VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 4) {
              Text("APPROVED")
                .font(.system(size: 6, weight: .black, design: .monospaced))
                .foregroundStyle(accent)
                .padding(.horizontal, 3)
                .padding(.vertical, 1)
                .overlay(
                  RoundedRectangle(cornerRadius: 2)
                    .stroke(accent, lineWidth: 0.8)
                )
              Spacer(minLength: 2)
            }
            RoundedRectangle(cornerRadius: 1.5)
              .fill(Color.yellow.opacity(0.55))
              .frame(width: 48, height: 4)
            Path { path in
              path.move(to: CGPoint(x: 2, y: 8))
              path.addCurve(
                to: CGPoint(x: 46, y: 4),
                control1: CGPoint(x: 16, y: 0),
                control2: CGPoint(x: 30, y: 12)
              )
            }
            .stroke(EasyOfficeDesignTokens.brand700.swiftUIColor, lineWidth: 1.4)
            .frame(width: 50, height: 10)
          }
          .padding(6)
          .frame(width: 72)
          .background(palette.cardSurface.swiftUIColor)
          .overlay(
            RoundedRectangle(cornerRadius: 4)
              .stroke(accent.opacity(0.45), lineWidth: 1)
          )
        }

        Spacer(minLength: 0)
      }
      .padding(.horizontal, 8)
      .frame(maxHeight: .infinity)

      Text(formatBadge)
        .font(.system(size: 8, weight: .bold, design: .monospaced))
        .foregroundStyle(accent)
        .padding(.horizontal, 5)
        .padding(.vertical, 2)
        .background(palette.cardSurface.swiftUIColor.opacity(0.9))
        .clipShape(RoundedRectangle(cornerRadius: 3))
        .padding(6)
    }
    .frame(height: 58)
    .overlay(
      RoundedRectangle(cornerRadius: 6)
        .stroke(palette.border.swiftUIColor, lineWidth: 1)
    )
    .clipShape(RoundedRectangle(cornerRadius: 6))
  }
}

struct BatchConversionStudioHubView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel

  var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)

    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Label(
          "포맷 일괄 변환 스튜디오 (Fail-Closed 컨테이너 검증)",
          systemImage: "arrow.triangle.2.circlepath.doc.on.clipboard"
        )
        .font(.system(size: 15, weight: .bold))
        .foregroundStyle(palette.accentBadgeText.swiftUIColor)
        Spacer()
        Picker("목표 포맷", selection: $viewModel.batchTargetFormat) {
          ForEach([DocumentFormat.pdf, .docx, .hwpx, .odt, .xlsx, .csv, .md], id: \.rawValue) {
            fmt in
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
        Text(
          "열려 있는 모든 문서 탭을 선택한 표준 포맷(PDF, DOCX, HWPX, ODT, XLSX, CSV, MD)으로 일괄 변환하고 바이트 무결성을 검증합니다."
        )
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

@MainActor
private enum SmartAssistantAnalysisMemoizer {
  private static var lastDoc: OfficeDocumentModel?
  private static var lastDocAnalysis:
    (
      summary: DocumentSummaryReport,
      suggestions: [WritingRefinementSuggestion],
      actionItems: [ExtractedActionItem]
    )?
  private static var lastSheet: WorksheetModel?
  private static var lastSheetInsight: WorksheetInsightReport?
  private static var lastDiffPair: (OfficeDocumentModel, OfficeDocumentModel)?
  private static var lastDiffReport: DocumentDiffReport?

  static func analyzeDocument(_ doc: OfficeDocumentModel) -> (
    summary: DocumentSummaryReport,
    suggestions: [WritingRefinementSuggestion],
    actionItems: [ExtractedActionItem]
  ) {
    if let cached = lastDocAnalysis, lastDoc == doc {
      return cached
    }
    let computed = (
      summary: LocalSmartDocumentAssistant.summarize(doc, maxSentences: 3),
      suggestions: LocalSmartDocumentAssistant.analyzeWritingQuality(doc),
      actionItems: LocalSmartDocumentAssistant.extractActionItemsAndEntities(doc)
    )
    lastDoc = doc
    lastDocAnalysis = computed
    return computed
  }

  static func analyzeSheet(_ sheet: WorksheetModel) -> WorksheetInsightReport {
    if let cached = lastSheetInsight, lastSheet == sheet {
      return cached
    }
    let range = sheet.detectedNumericRange()
    let computed = LocalSmartDocumentAssistant.analyzeWorksheetInsights(sheet, range: range)
    lastSheet = sheet
    lastSheetInsight = computed
    return computed
  }

  static func compareDocuments(
    baseline: OfficeDocumentModel,
    revised: OfficeDocumentModel
  ) -> DocumentDiffReport {
    if let cached = lastDiffReport,
      let pair = lastDiffPair,
      pair.0 == baseline && pair.1 == revised
    {
      return cached
    }
    let computed = LocalSmartDocumentAssistant.compareDocuments(
      baseline: baseline,
      revised: revised
    )
    lastDiffPair = (baseline, revised)
    lastDiffReport = computed
    return computed
  }
}

struct SmartDocumentAssistantHubView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel

  var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)
    let documentModels: [OfficeDocumentModel] = viewModel.openTabs.compactMap { tab in
      if case .document(let doc) = tab.payload { return doc }
      return nil
    }
    let activeDoc: OfficeDocumentModel? = {
      if let tab = viewModel.activeTab, case .document(let doc) = tab.payload {
        return doc
      }
      return documentModels.first
    }()
    let activeSheet: WorksheetModel? = {
      if let tab = viewModel.activeTab, case .sheet(let sheet) = tab.payload {
        return sheet
      }
      for tab in viewModel.openTabs {
        if case .sheet(let sheet) = tab.payload { return sheet }
      }
      return nil
    }()

    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Label(
          "스마트 문서 보조 도구 (요약·가독성·조치항목·시트통계·문서비교·보안감사)", systemImage: "sparkles.rectangle.stack.fill"
        )
        .font(.system(size: 15, weight: .bold))
        .foregroundStyle(palette.accentBadgeText.swiftUIColor)
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
        let analysis = SmartAssistantAnalysisMemoizer.analyzeDocument(doc)
        let summary = analysis.summary
        let suggestions = analysis.suggestions
        let actionItems = analysis.actionItems

        Text(
          "대상 문서: \(doc.title) · 예상 읽기 시간 \(summary.estimatedReadingSeconds)초 · 압축률 \(String(format: "%.0f", summary.compressionRatioPercent))%"
        )
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
          let insight = SmartAssistantAnalysisMemoizer.analyzeSheet(sheet)
          VStack(alignment: .leading, spacing: 4) {
            Text("시트 Z-Score 통계 인사이트 (\(insight.trendClassification))")
              .font(.system(size: 11, weight: .bold))
              .foregroundStyle(EasyOfficeDesignTokens.accentToken(for: .sheet).swiftUIColor)
            Text(
              "평균 \(String(format: "%.1f", insight.mean)) · 표준편차 \(String(format: "%.1f", insight.standardDeviation)) · 증감률 \(String(format: "%.1f", insight.growthRatePercent))%"
            )
            .font(.system(size: 11, design: .monospaced))
            .foregroundStyle(palette.secondaryText.swiftUIColor)
          }
          .frame(maxWidth: .infinity, alignment: .leading)
        }

        if documentModels.count >= 2 {
          let diff = SmartAssistantAnalysisMemoizer.compareDocuments(
            baseline: documentModels[0],
            revised: documentModels[1]
          )
          VStack(alignment: .leading, spacing: 4) {
            Text("문서 LCS 비교 (Document ↔ HWP)")
              .font(.system(size: 11, weight: .bold))
              .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
            Text(
              "추가 +\(diff.addedCount) · 삭제 -\(diff.removedCount) · 수정 ~\(diff.modifiedCount) · 동일 =\(diff.unchangedCount)"
            )
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
            Image(
              systemName: audit.isPassed ? "checkmark.shield.fill" : "exclamationmark.triangle.fill"
            )
            .foregroundStyle(
              audit.isPassed
                ? EasyOfficeDesignTokens.statusSuccess.swiftUIColor
                : EasyOfficeDesignTokens.statusDanger.swiftUIColor
            )
            Text(
              "매직 바이트 보안 감사: \(audit.detectedFormat?.rawValue.uppercased() ?? "UNKNOWN") · 통과 여부: \(audit.isPassed ? "안전" : "위험 감지")"
            )
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
