import AppKit
import EasyOfficeCore
import EasyOfficeDesignSystem
import SwiftUI

@MainActor
final class EasyOfficeAppDelegate: NSObject, NSApplicationDelegate {
  let viewModel = WorkspaceViewModel()

  func applicationShouldTerminate(_ _: NSApplication) -> NSApplication.TerminateReply {
    if let modifiedTab = viewModel.openTabs.first(where: \.isModified) {
      viewModel.activeTabID = modifiedTab.id
      viewModel.isShowingHomeHub = false
      viewModel.pendingCloseConfirmTabID = modifiedTab.id
      viewModel.lastExecutedCommandSummary =
        "앱 종료 전 저장되지 않은 변경사항 확인 대기: \(modifiedTab.fileName)"
      return .terminateCancel
    }
    return .terminateNow
  }

  func application(_ _: NSApplication, open urls: [URL]) {
    for url in urls {
      do {
        _ = try viewModel.loadFromDisk(url: url)
      } catch {
        viewModel.lastErrorMessage = error.localizedDescription
      }
    }
  }
}

@main
struct EasyOfficeApplication: App {
  @NSApplicationDelegateAdaptor(EasyOfficeAppDelegate.self) private var appDelegate

  private var viewModel: WorkspaceViewModel {
    appDelegate.viewModel
  }

  var body: some Scene {
    WindowGroup("EasyOffice") {
      MainWorkspaceWindowView(viewModel: viewModel)
        .frame(
          minWidth: EasyOfficeDesignTokens.LayoutMetrics.minimumWindowWidth,
          minHeight: EasyOfficeDesignTokens.LayoutMetrics.minimumWindowHeight
        )
        .onOpenURL { url in
          do {
            _ = try viewModel.loadFromDisk(url: url)
          } catch {
            viewModel.lastErrorMessage = error.localizedDescription
          }
        }
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

        Button("문서 블록 복사") {
          WorkspaceFileDialogPresenter.copySelectionToPasteboard(viewModel: viewModel)
        }
        .disabled(viewModel.activeTab == nil)

        Button("문서 블록 붙여넣기") {
          WorkspaceFileDialogPresenter.pasteFromPasteboard(viewModel: viewModel)
        }
        .disabled(viewModel.activeTab == nil)

        Divider()

        Button("최근 닫은 탭 다시 열기 (⇧⌘T)") {
          _ = viewModel.reopenLastClosedTab()
        }
        .keyboardShortcut("t", modifiers: [.command, .shift])
        .disabled(viewModel.recentlyClosedTabs.isEmpty)
      }
      CommandMenu("문서 생성") {
        ForEach(OfficeModuleKind.allCases) { module in
          Button("새 \(module.displayNameKo)") {
            viewModel.openNewSampleTab(for: module)
          }
        }
      }
      CommandMenu("워크스페이스 보기") {
        Button("홈 허브로 이동 (⇧⌘H)") {
          viewModel.isShowingHomeHub = true
        }
        .keyboardShortcut("h", modifiers: [.command, .shift])

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
