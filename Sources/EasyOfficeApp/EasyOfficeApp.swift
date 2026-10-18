import SwiftUI
import AppKit
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
            CommandMenu("파일 및 편집") {
                Button("파일 열기...") {
                    WorkspaceFileDialogPresenter.presentOpenPanel(viewModel: viewModel)
                }
                .keyboardShortcut("o", modifiers: [.command])

                Button("현재 탭 저장...") {
                    WorkspaceFileDialogPresenter.presentSavePanel(viewModel: viewModel)
                }
                .keyboardShortcut("s", modifiers: [.command])
                .disabled(viewModel.activeTab == nil)

                Button("현재 문서 인쇄 (⌘P)...") {
                    WorkspaceFileDialogPresenter.presentPrintPanel(viewModel: viewModel)
                }
                .keyboardShortcut("p", modifiers: [.command])
                .disabled(viewModel.activeTab == nil)

                Divider()

                Button("실행 취소 (Undo)") {
                    viewModel.undo()
                }
                .keyboardShortcut("z", modifiers: [.command])
                .disabled(!viewModel.canUndo)

                Button("다시 실행 (Redo)") {
                    viewModel.redo()
                }
                .keyboardShortcut("z", modifiers: [.command, .shift])
                .disabled(!viewModel.canRedo)

                Divider()

                Button("찾기 및 바꾸기 (⌘F)") {
                    viewModel.isFindReplaceBarVisible.toggle()
                }
                .keyboardShortcut("f", modifiers: [.command])
                .disabled(viewModel.activeTab == nil)

                Button("선택 영역 복사 (⌘C)") {
                    WorkspaceFileDialogPresenter.copySelectionToPasteboard(viewModel: viewModel)
                }
                .keyboardShortcut("c", modifiers: [.command])
                .disabled(viewModel.activeTab == nil)

                Button("붙여넣기 (⌘V)") {
                    WorkspaceFileDialogPresenter.pasteFromPasteboard(viewModel: viewModel)
                }
                .keyboardShortcut("v", modifiers: [.command])
                .disabled(viewModel.activeTab == nil)
            }
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
