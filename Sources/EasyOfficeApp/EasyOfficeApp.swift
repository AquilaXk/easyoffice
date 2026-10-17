import SwiftUI
import EasyOfficeDesignSystem
import EasyOfficeCore

@main
struct EasyOfficeApplication: App {
    @State private var viewModel = WorkspaceViewModel()

    var body: some Scene {
        WindowGroup("EasyOffice") {
            MainWorkspaceWindowView(viewModel: viewModel)
                .frame(
                    minWidth: EasyOfficeDesignTokens.LayoutMetrics.minimumWindowWidth,
                    minHeight: EasyOfficeDesignTokens.LayoutMetrics.minimumWindowHeight
                )
        }
        .commands {
            CommandMenu("문서 생성") {
                ForEach(OfficeModuleKind.allCases) { module in
                    Button("새 \(module.displayNameKo)") {
                        viewModel.openNewSampleTab(for: module)
                    }
                }
            }
            CommandMenu("워크스페이스 보기") {
                Button("홈 허브로 이동") {
                    viewModel.isShowingHomeHub = true
                }
                .keyboardShortcut("0", modifiers: [.command])

                Button("컨텍스트 리본 접기/펼치기") {
                    viewModel.isRibbonCollapsed.toggle()
                }
                .keyboardShortcut("r", modifiers: [.command, .option])

                Button("좌측 내비게이터 토글") {
                    viewModel.showLeftNavigator.toggle()
                }
                .keyboardShortcut("[", modifiers: [.command, .option])

                Button("우측 속성 인스펙터 토글") {
                    viewModel.showRightInspector.toggle()
                }
                .keyboardShortcut("i", modifiers: [.command, .option])
            }
        }
    }
}
