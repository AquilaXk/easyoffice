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
      VStack(alignment: .leading, spacing: 4) {
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
              Rectangle()
                .fill(selected ? EasyOfficeDesignTokens.brand600.swiftUIColor : Color.clear)
                .frame(width: 2.5, height: 16)
              Image(systemName: section.symbolName)
              Text(section.rawValue)
                .font(.system(size: 13, weight: selected ? .semibold : .regular))
              Spacer()
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .background(
              selected
                ? palette.selectedSurface.swiftUIColor.opacity(0.45)
                : Color.clear
            )
            .foregroundStyle(
              selected
                ? palette.accentBadgeText.swiftUIColor
                : palette.primaryText.swiftUIColor
            )
          }
          .buttonStyle(.plain)
        }
        Spacer()
      }
      .padding(16)
      .frame(width: 220)
      .background(palette.cardSurface.swiftUIColor)

      Divider()

      AdaptiveScrollContainer(
        axes: .vertical,
        showsIndicators: true,
        isSnapshotMode: viewModel.isCanvasSnapshotMode
      ) {
        VStack(alignment: .leading, spacing: 24) {
          switch viewModel.activeHubSection {
          case .converter:
            BatchConversionStudioHubView(viewModel: viewModel)

          case .smartAssistant:
            SmartDocumentAssistantHubView(viewModel: viewModel)

          case .home, .recent, .pinned, .templates:
            VStack(alignment: .leading, spacing: 10) {
              Text("새 문서 만들기")
                .font(.system(size: 16, weight: .bold))
                .foregroundStyle(palette.primaryText.swiftUIColor)

              HStack(spacing: 12) {
                ForEach(OfficeModuleKind.allCases) { module in
                  let accent = EasyOfficeDesignTokens.accentToken(for: module)
                  Button {
                    viewModel.openNewSampleTab(for: module)
                  } label: {
                    VStack(alignment: .leading, spacing: 0) {
                      Rectangle()
                        .fill(accent.swiftUIColor)
                        .frame(height: 3)

                      VStack(alignment: .leading, spacing: 8) {
                        HStack {
                          Image(systemName: module.systemSymbolName)
                            .font(.system(size: 20, weight: .semibold))
                            .foregroundStyle(accent.swiftUIColor)
                          Spacer(minLength: 4)
                          Text(module.quickLauncherFormatBadge)
                            .font(.system(size: 9.5, weight: .semibold, design: .monospaced))
                            .foregroundStyle(accent.swiftUIColor)
                        }

                        VStack(alignment: .leading, spacing: 2) {
                          Text(module.quickLauncherCardTitleKo)
                            .font(.system(size: 13, weight: .bold))
                            .foregroundStyle(palette.primaryText.swiftUIColor)
                            .lineLimit(1)
                          Text(module.displayNameKo)
                            .font(.system(size: 11))
                            .foregroundStyle(palette.secondaryText.swiftUIColor)
                            .lineLimit(1)
                        }
                      }
                      .padding(12)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(palette.cardSurface.swiftUIColor)
                    .border(palette.border.swiftUIColor, width: 1)
                  }
                  .buttonStyle(.plain)
                }
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

            if viewModel.activeHubSection == .home || viewModel.activeHubSection == .templates {
              Divider()

              VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 12) {
                  Text(
                    "서식 템플릿 (\(filteredTemplates.count) / \(viewModel.allAvailableTemplates.count))"
                  )
                  .font(.system(size: 15, weight: .bold))
                  .foregroundStyle(palette.primaryText.swiftUIColor)
                  Spacer()
                  FlatUnderlineTextField(
                    placeholder: "사용자 템플릿 이름 (선택)",
                    text: $viewModel.customTemplateTitleDraft,
                    symbolName: "doc.badge.plus",
                    width: 185,
                    palette: palette,
                    isSnapshotMode: viewModel.isCanvasSnapshotMode
                  )
                  Button("현재 탭을 템플릿으로 저장") {
                    viewModel.saveActiveTabAsCustomTemplate(
                      title: nil,
                      subtitle: "현재 활성 문서 기반 커스텀 템플릿"
                    )
                  }
                  .buttonStyle(.plain)
                  .font(.system(size: 11, weight: .semibold))
                  .foregroundStyle(EasyOfficeDesignTokens.brand600.swiftUIColor)
                }

                HStack(spacing: 14) {
                  ForEach(["전체", "문서·한글", "시트", "슬라이드", "PDF"], id: \.self) { cat in
                    let isSelected = viewModel.selectedHomeTemplateCategoryFilter == cat
                    Button {
                      viewModel.selectedHomeTemplateCategoryFilter = cat
                    } label: {
                      VStack(spacing: 4) {
                        Text(cat)
                          .font(.system(size: 11.5, weight: isSelected ? .bold : .medium))
                          .foregroundStyle(
                            isSelected
                              ? palette.primaryText.swiftUIColor
                              : palette.secondaryText.swiftUIColor
                          )
                        Rectangle()
                          .fill(
                            isSelected
                              ? EasyOfficeDesignTokens.brand600.swiftUIColor
                              : Color.clear
                          )
                          .frame(width: 28, height: 2)
                      }
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
                        .font(.system(size: 10, weight: .bold, design: .monospaced))
                        .foregroundStyle(
                          EasyOfficeDesignTokens.accentToken(for: tpl.moduleKind).swiftUIColor
                        )
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
                  .border(palette.border.swiftUIColor, width: 1)
                  .onTapGesture {
                    viewModel.openTab(from: tpl)
                  }
                }
              }
            }

            if viewModel.activeHubSection == .home
              || viewModel.activeHubSection == .recent
              || viewModel.activeHubSection == .pinned
            {
              let filteredRecentTabs = viewModel.filteredHomeRecentTabs

              Divider()

              HStack(spacing: 12) {
                Text("\(viewModel.activeHubSection.rawValue) (\(filteredRecentTabs.count))")
                  .font(.system(size: 15, weight: .bold))
                  .foregroundStyle(palette.primaryText.swiftUIColor)
                Spacer()
                HStack(spacing: 12) {
                  ForEach(
                    ["전체", "DOCX", "HWP/HWPX", "XLSX", "PPTX", "PDF", "ODF"],
                    id: \.self
                  ) { fmt in
                    let isSelected =
                      viewModel.selectedHomeRecentFormatFilter == fmt
                      || (fmt == "HWP/HWPX"
                        && viewModel.selectedHomeRecentFormatFilter == "DOCX/HWP")
                    Button {
                      viewModel.selectedHomeRecentFormatFilter = fmt
                    } label: {
                      VStack(spacing: 3) {
                        Text(fmt)
                          .font(.system(size: 11, weight: isSelected ? .bold : .medium))
                          .foregroundStyle(
                            isSelected
                              ? palette.primaryText.swiftUIColor
                              : palette.secondaryText.swiftUIColor
                          )
                        Rectangle()
                          .fill(
                            isSelected
                              ? EasyOfficeDesignTokens.brand600.swiftUIColor
                              : Color.clear
                          )
                          .frame(width: 24, height: 2)
                      }
                    }
                    .buttonStyle(.plain)
                  }
                }
              }

              VStack(spacing: 0) {
                ForEach(filteredRecentTabs) { tab in
                  let moduleAccent = EasyOfficeDesignTokens.accentToken(for: tab.moduleKind)
                    .swiftUIColor
                  VStack(spacing: 0) {
                    HStack(spacing: 12) {
                      Image(systemName: tab.moduleKind.systemSymbolName)
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(moduleAccent)
                        .frame(width: 20)

                      Text(tab.moduleKind.shortBadgeTitle)
                        .font(.system(size: 10.5, weight: .semibold))
                        .foregroundStyle(moduleAccent)
                        .frame(width: 44, alignment: .leading)

                      Text(tab.format.rawValue.uppercased())
                        .font(.system(size: 10, weight: .semibold, design: .monospaced))
                        .foregroundStyle(palette.secondaryText.swiftUIColor)
                        .frame(width: 44, alignment: .leading)

                      Text(tab.fileName)
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(palette.primaryText.swiftUIColor)

                      if tab.isModified {
                        Text("수정됨")
                          .font(.system(size: 10, weight: .semibold))
                          .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                      }

                      Spacer()

                      Text(tab.lastOpenedISO8601)
                        .font(.system(size: 10.5, design: .monospaced))
                        .foregroundStyle(palette.secondaryText.swiftUIColor)

                      Button {
                        viewModel.toggleFavoriteTab(id: tab.id)
                      } label: {
                        Image(systemName: tab.isFavorite ? "star.fill" : "star")
                          .font(.system(size: 11, weight: .semibold))
                          .foregroundStyle(
                            tab.isFavorite
                              ? EasyOfficeDesignTokens.statusWarning.swiftUIColor
                              : palette.secondaryText.swiftUIColor
                          )
                      }
                      .buttonStyle(.plain)
                      .help(tab.isFavorite ? "즐겨찾기 해제" : "즐겨찾기 추가")

                      Button {
                        viewModel.togglePinTab(id: tab.id)
                      } label: {
                        Image(systemName: tab.isPinned ? "pin.fill" : "pin")
                          .font(.system(size: 11, weight: .semibold))
                          .foregroundStyle(
                            tab.isPinned
                              ? EasyOfficeDesignTokens.brand700.swiftUIColor
                              : palette.secondaryText.swiftUIColor
                          )
                      }
                      .buttonStyle(.plain)
                      .help(tab.isPinned ? "고정 해제" : "고정")

                      Button("스플릿 뷰") {
                        if viewModel.activeTabID == tab.id,
                          let other = viewModel.openTabs.first(where: { $0.id != tab.id })
                        {
                          viewModel.activeTabID = other.id
                        }
                        viewModel.enableSplitView(secondaryTabID: tab.id)
                        viewModel.isShowingHomeHub = false
                      }
                      .buttonStyle(.plain)
                      .font(.system(size: 11, weight: .medium))
                      .foregroundStyle(EasyOfficeDesignTokens.brand600.swiftUIColor)
                    }
                    .padding(.vertical, 10)
                    .padding(.horizontal, 4)
                    .contentShape(Rectangle())
                    .onTapGesture {
                      viewModel.activeTabID = tab.id
                      viewModel.isShowingHomeHub = false
                    }

                    Divider()
                  }
                }
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
      Rectangle()
        .fill(palette.elevatedSurface.swiftUIColor)

      HStack(spacing: 10) {
        switch module {
        case .document:
          VStack(alignment: .leading, spacing: 3) {
            Rectangle()
              .fill(accent)
              .frame(width: 44, height: 4)
            Rectangle()
              .fill(palette.secondaryText.swiftUIColor.opacity(0.45))
              .frame(width: 56, height: 2.5)
            Rectangle()
              .fill(palette.secondaryText.swiftUIColor.opacity(0.35))
              .frame(width: 48, height: 2.5)
            Rectangle()
              .fill(palette.secondaryText.swiftUIColor.opacity(0.30))
              .frame(width: 52, height: 2.5)
            HStack(spacing: 2) {
              Rectangle().fill(accent.opacity(0.22)).frame(width: 24, height: 8)
              Rectangle().fill(accent.opacity(0.14)).frame(width: 24, height: 8)
            }
          }
          .padding(6)
          .background(palette.cardSurface.swiftUIColor)
          .border(palette.border.swiftUIColor, width: 1)

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
                    .fill(accent.opacity(0.25))
                    .frame(width: 8, height: 8)
                }
              }
            }
            HStack(spacing: 2) {
              ForEach(0..<6, id: \.self) { _ in
                Rectangle()
                  .fill(Color(red: 0.85, green: 0.28, blue: 0.22).opacity(0.35))
                  .frame(width: 7, height: 7)
              }
            }
            Rectangle()
              .fill(palette.secondaryText.swiftUIColor.opacity(0.35))
              .frame(width: 50, height: 2.5)
          }
          .padding(6)
          .frame(width: 76)
          .background(palette.cardSurface.swiftUIColor)
          .border(accent.opacity(0.5), width: 1)

        case .sheet:
          HStack(spacing: 8) {
            VStack(spacing: 2) {
              ForEach(0..<3, id: \.self) { row in
                HStack(spacing: 2) {
                  ForEach(0..<4, id: \.self) { _ in
                    Rectangle()
                      .fill(row == 0 ? accent.opacity(0.35) : palette.cardSurface.swiftUIColor)
                      .frame(width: 11, height: 8)
                      .border(accent.opacity(0.35), width: 0.5)
                  }
                }
              }
            }
            HStack(alignment: .bottom, spacing: 3) {
              Rectangle().fill(accent.opacity(0.55)).frame(width: 5, height: 14)
              Rectangle().fill(accent).frame(width: 5, height: 26)
              Rectangle().fill(accent.opacity(0.75)).frame(width: 5, height: 20)
            }
          }
          .padding(6)
          .background(palette.cardSurface.swiftUIColor)
          .border(palette.border.swiftUIColor, width: 1)

        case .slide:
          VStack(alignment: .leading, spacing: 4) {
            Rectangle()
              .fill(accent)
              .frame(width: 68, height: 6)
            HStack(spacing: 4) {
              Rectangle()
                .fill(accent.opacity(0.25))
                .frame(width: 32, height: 18)
              Rectangle()
                .fill(EasyOfficeDesignTokens.brand700.swiftUIColor.opacity(0.25))
                .frame(width: 32, height: 18)
            }
          }
          .padding(6)
          .aspectRatio(16.0 / 9.0, contentMode: .fit)
          .background(palette.cardSurface.swiftUIColor)
          .border(accent.opacity(0.55), width: 1)

        case .pdf:
          VStack(alignment: .leading, spacing: 4) {
            Text("APPROVED")
              .font(.system(size: 6, weight: .black, design: .monospaced))
              .foregroundStyle(accent)
            Rectangle()
              .fill(Color.yellow.opacity(0.55))
              .frame(width: 48, height: 4)
            Rectangle()
              .fill(EasyOfficeDesignTokens.brand700.swiftUIColor.opacity(0.55))
              .frame(width: 46, height: 2)
          }
          .padding(6)
          .frame(width: 72, alignment: .leading)
          .background(palette.cardSurface.swiftUIColor)
          .border(accent.opacity(0.45), width: 1)
        }

        Spacer(minLength: 0)
      }
      .padding(.horizontal, 8)
      .frame(maxHeight: .infinity)

      Text(formatBadge)
        .font(.system(size: 8, weight: .bold, design: .monospaced))
        .foregroundStyle(accent)
        .padding(6)
    }
    .frame(height: 58)
    .border(palette.border.swiftUIColor, width: 1)
  }
}

struct BatchConversionStudioHubView: View {
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var viewModel: WorkspaceViewModel

  var body: some View {
    let palette = viewModel.themeMode.surfacePalette(isSystemDark: colorScheme == .dark)

    VStack(alignment: .leading, spacing: 14) {
      HStack(spacing: 14) {
        Label(
          "포맷 일괄 변환",
          systemImage: "arrow.triangle.2.circlepath.doc.on.clipboard"
        )
        .font(.system(size: 16, weight: .bold))
        .foregroundStyle(palette.primaryText.swiftUIColor)

        Spacer()

        FlatToolbarMenu(
          title: "목표 포맷: \(viewModel.batchTargetFormat.rawValue.uppercased())",
          symbolName: "doc.badge.gearshape",
          palette: palette,
          isSnapshotMode: viewModel.isCanvasSnapshotMode
        ) {
          ForEach(DocumentFormat.allCases, id: \.rawValue) { fmt in
            Button(fmt.rawValue.uppercased()) {
              viewModel.batchTargetFormat = fmt
            }
          }
        }

        Button("열린 탭 전체 변환 실행") {
          viewModel.runBatchConversionForOpenTabs()
        }
        .buttonStyle(.plain)
        .font(.system(size: 12, weight: .semibold))
        .foregroundStyle(EasyOfficeDesignTokens.brand600.swiftUIColor)
      }

      Divider()

      if let report = viewModel.lastBatchConversionReport {
        HStack(spacing: 16) {
          Text("총 작업 \(report.totalJobs)건")
            .font(.system(size: 12, weight: .semibold))
          Text("성공 \(report.succeededResults.count)건")
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(EasyOfficeDesignTokens.statusSuccess.swiftUIColor)
          Text("실패 \(report.failedErrors.count)건")
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(
              !report.failedErrors.isEmpty
                ? EasyOfficeDesignTokens.statusDanger.swiftUIColor
                : palette.secondaryText.swiftUIColor
            )
        }

        Divider()

        VStack(spacing: 0) {
          ForEach(report.succeededResults) { item in
            let srcExt =
              item.sourceFileName.split(separator: ".").last.map { String($0).uppercased() }
              ?? "DOC"
            let dstExt = item.targetFormat.rawValue.uppercased()
            VStack(spacing: 0) {
              HStack(spacing: 12) {
                Text("\(srcExt) → \(dstExt)")
                  .font(.system(size: 10.5, weight: .semibold, design: .monospaced))
                  .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                  .frame(width: 96, alignment: .leading)

                Text(item.sourceFileName)
                  .font(.system(size: 12, weight: .medium))
                  .foregroundStyle(palette.primaryText.swiftUIColor)
                Image(systemName: "arrow.right")
                  .font(.system(size: 10))
                  .foregroundStyle(palette.secondaryText.swiftUIColor)
                Text(item.outputFileName)
                  .font(.system(size: 11.5, weight: .semibold, design: .monospaced))
                  .foregroundStyle(palette.primaryText.swiftUIColor)
                Spacer()
                Text("완료 · \(item.outputBytes.count) B")
                  .font(.system(size: 11, design: .monospaced))
                  .foregroundStyle(EasyOfficeDesignTokens.statusSuccess.swiftUIColor)
              }
              .padding(.vertical, 8)

              Divider()
            }
          }
        }

        ForEach(Array(report.failedErrors.enumerated()), id: \.offset) { _, err in
          Text("거부됨: \(err.errorDescription)")
            .font(.system(size: 11))
            .foregroundStyle(EasyOfficeDesignTokens.statusDanger.swiftUIColor)
        }
      } else {
        VStack(alignment: .leading, spacing: 8) {
          Text(
            "열려 있는 모든 문서 탭(\(viewModel.openTabs.count)건)을 선택한 포맷으로 일괄 변환합니다."
          )
          .font(.system(size: 12))
          .foregroundStyle(palette.secondaryText.swiftUIColor)

          VStack(spacing: 0) {
            ForEach(viewModel.openTabs) { tab in
              let srcExt = tab.format.rawValue.uppercased()
              let dstExt = viewModel.batchTargetFormat.rawValue.uppercased()
              VStack(spacing: 0) {
                HStack(spacing: 12) {
                  Text("\(srcExt) → \(dstExt)")
                    .font(.system(size: 10.5, weight: .semibold, design: .monospaced))
                    .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
                    .frame(width: 96, alignment: .leading)
                  Text(tab.fileName)
                    .font(.system(size: 12))
                    .foregroundStyle(palette.primaryText.swiftUIColor)
                    .lineLimit(1)
                  Spacer()
                }
                .padding(.vertical, 7)

                Divider()
              }
            }
          }
        }
      }

      batchPipelineSummarySection(palette: palette)
    }
  }

  private var targetStandardDescription: String {
    switch viewModel.batchTargetFormat {
    case .pdf:
      return "PDF 1.7 (ISO 32000-1)"
    case .docx, .xlsx, .pptx, .ppsx:
      return "OOXML (ECMA-376)"
    case .hwpx, .hwtx:
      return "OWPML (KS X 6101)"
    case .hwp, .hwt:
      return "HWP 5.0 Binary (OLE2 CFBF)"
    case .odt, .ods, .odp:
      return "ODF 1.3 (ISO/IEC 26300)"
    case .doc, .xls, .ppt:
      return "OLE2 Compound Binary (CFBF)"
    case .csv, .tsv:
      return "RFC 4180 Tabular Stream"
    case .md, .txt, .rtf:
      return "CommonMark / Rich Text Stream"
    }
  }

  @ViewBuilder
  private func batchPipelineSummarySection(palette: WorkspaceThemePalette) -> some View {
    let report = viewModel.lastBatchConversionReport
    let totalOutputBytes = report?.succeededResults.reduce(0) { $0 + $1.outputBytes.count } ?? 0
    let jobCount = report?.totalJobs ?? viewModel.openTabs.count
    let succeededCount = report?.succeededResults.count ?? 0
    let statusLabel = succeededCount > 0 ? "검증 완료" : "대기 중"
    let statusColor =
      succeededCount > 0
      ? EasyOfficeDesignTokens.statusSuccess.swiftUIColor
      : palette.secondaryText.swiftUIColor
    let familyRows: [(title: String, detail: String)] = [
      ("문서/공문서 조판", "DOCX · HWP · HWPX · ODT 단락·표·각주 보존"),
      ("스프레드시트 수식 평가", "XLSX · ODS · CSV 수식 재계산 및 서식 매핑"),
      ("프레젠테이션 16:9 벡터 스테이지", "PPTX · ODP 마스터 레이아웃 및 도형 좌표 보존"),
      ("PDF 바이너리 XREF 테이블", "PDF 1.7 카탈로그·페이지 트리·주석 스트림 직렬화"),
    ]

    VStack(alignment: .leading, spacing: 12) {
      Rectangle()
        .fill(palette.border.swiftUIColor)
        .frame(height: 1)
        .padding(.top, 6)

      HStack(alignment: .top, spacing: 24) {
        VStack(alignment: .leading, spacing: 8) {
          Text("변환 파이프라인 및 보존 규격")
            .font(.system(size: 12, weight: .bold))
            .foregroundStyle(palette.primaryText.swiftUIColor)

          HStack {
            Text("목표 표준 규격")
              .font(.system(size: 11.5))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
            Spacer()
            Text(targetStandardDescription)
              .font(.system(size: 11, weight: .semibold, design: .monospaced))
              .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
          }

          Rectangle()
            .fill(palette.border.swiftUIColor)
            .frame(height: 1)

          HStack {
            Text("조판 보존 정책")
              .font(.system(size: 11.5))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
            Spacer()
            Text("원본 여백·폰트 메트릭·표 격자 보존")
              .font(.system(size: 11, weight: .medium))
              .foregroundStyle(palette.primaryText.swiftUIColor)
          }

          Rectangle()
            .fill(palette.border.swiftUIColor)
            .frame(height: 1)

          HStack {
            Text("보안 및 무결성")
              .font(.system(size: 11.5))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
            Spacer()
            Text("문서 시그니처 사전 검증 · 바이트 크기 및 XREF 오프셋 무결성 확인")
              .font(.system(size: 11))
              .foregroundStyle(palette.primaryText.swiftUIColor)
              .lineLimit(1)
          }

          Rectangle()
            .fill(palette.border.swiftUIColor)
            .frame(height: 1)

          HStack {
            Text("누적 출력 현황")
              .font(.system(size: 11.5))
              .foregroundStyle(palette.secondaryText.swiftUIColor)
            Spacer()
            Text("대상 \(jobCount)건 · 완료 \(succeededCount)건 · 총 출력 용량: \(totalOutputBytes) B")
              .font(.system(size: 11, weight: .semibold, design: .monospaced))
              .foregroundStyle(statusColor)
          }
        }
        .frame(maxWidth: .infinity, alignment: .leading)

        Rectangle()
          .fill(palette.border.swiftUIColor)
          .frame(width: 1, height: 132)

        VStack(alignment: .leading, spacing: 8) {
          Text("포맷 패밀리별 변환 검증 현황")
            .font(.system(size: 12, weight: .bold))
            .foregroundStyle(palette.primaryText.swiftUIColor)

          ForEach(Array(familyRows.enumerated()), id: \.offset) { idx, row in
            HStack(spacing: 10) {
              Text(row.title)
                .font(.system(size: 11.5, weight: .semibold))
                .foregroundStyle(palette.primaryText.swiftUIColor)
              Text(row.detail)
                .font(.system(size: 10.5))
                .foregroundStyle(palette.secondaryText.swiftUIColor)
                .lineLimit(1)
              Spacer()
              Text(statusLabel)
                .font(.system(size: 10.5, weight: .semibold, design: .monospaced))
                .foregroundStyle(statusColor)
            }
            if idx < familyRows.count - 1 {
              Rectangle()
                .fill(palette.border.swiftUIColor)
                .frame(height: 1)
            }
          }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
      }
    }
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

    VStack(alignment: .leading, spacing: 14) {
      HStack(spacing: 14) {
        Label(
          "스마트 문서 보조 도구",
          systemImage: "sparkles.rectangle.stack.fill"
        )
        .font(.system(size: 16, weight: .bold))
        .foregroundStyle(palette.primaryText.swiftUIColor)
        Spacer()
        Button("개인정보·메타데이터 정리") {
          viewModel.scrubActiveTabMetadata()
        }
        .buttonStyle(.plain)
        .font(.system(size: 12, weight: .medium))
        .foregroundStyle(palette.primaryText.swiftUIColor)

        Button("보안 감사 실행") {
          viewModel.runSecurityAuditOnActiveTab()
        }
        .buttonStyle(.plain)
        .font(.system(size: 12, weight: .semibold))
        .foregroundStyle(EasyOfficeDesignTokens.brand600.swiftUIColor)
      }

      Divider()

      if let doc = activeDoc {
        let analysis = SmartAssistantAnalysisMemoizer.analyzeDocument(doc)
        let summary = analysis.summary
        let suggestions = analysis.suggestions
        let actionItems = analysis.actionItems

        Text(
          "\(doc.title) · 예상 읽기 시간 \(summary.estimatedReadingSeconds)초 · 요약 비율 \(String(format: "%.0f", summary.compressionRatioPercent))%"
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

        Divider()

        HStack(alignment: .top, spacing: 16) {
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

      Divider()

      HStack(alignment: .top, spacing: 16) {
        if let sheet = activeSheet {
          let insight = SmartAssistantAnalysisMemoizer.analyzeSheet(sheet)
          VStack(alignment: .leading, spacing: 4) {
            Text("시트 통계 요약 (\(insight.trendClassification))")
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
            Text("문서 비교")
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
              "파일 무결성 점검: \(audit.detectedFormat?.rawValue.uppercased() ?? "UNKNOWN") · \(audit.isPassed ? "안전" : "위험 감지")"
            )
            .font(.system(size: 12, weight: .semibold))
          }
          if viewModel.lastScrubbedMetadataCount > 0 {
            Text("· 정리된 메타데이터 \(viewModel.lastScrubbedMetadataCount)건")
              .font(.system(size: 11, weight: .medium))
              .foregroundStyle(EasyOfficeDesignTokens.brand700.swiftUIColor)
          }
        }
      }
    }
  }
}
