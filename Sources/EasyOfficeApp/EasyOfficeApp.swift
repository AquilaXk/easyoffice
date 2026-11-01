import AppKit
import EasyOfficeCore
import EasyOfficeDesignSystem
import SwiftUI

@MainActor
final class EasyOfficeAppDelegate: NSObject, NSApplicationDelegate {
  var viewModel: WorkspaceViewModel?
  private var pendingOpenURLs: [URL] = []

  func bind(viewModel: WorkspaceViewModel) {
    self.viewModel = viewModel
    let queued = pendingOpenURLs
    pendingOpenURLs.removeAll()
    if !queued.isEmpty {
      viewModel.handleIncomingOpenURLs(queued)
    }
    viewModel.markWorkspaceReadyAndDrainOpenURLs()
  }

  func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
    if ProcessInfo.processInfo.environment["EO_HEADLESS_QUIT"] == "1" {
      return .terminateNow
    }
    guard let vm = viewModel else { return .terminateNow }
    if vm.requestApplicationTermination() {
      return .terminateNow
    }
    let modifiedTabs = vm.dirtyOpenTabs
    guard !modifiedTabs.isEmpty else { return .terminateNow }

    let alert = NSAlert()
    alert.alertStyle = .warning
    alert.messageText = "저장하지 않은 변경 사항이 있는 탭이 \(modifiedTabs.count)개 있습니다."
    let names = modifiedTabs.prefix(5).map(\.fileName).joined(separator: ", ")
    alert.informativeText = "변경된 문서(\(names))를 저장하지 않고 종료하시겠습니까?"
    alert.addButton(withTitle: "저장 후 종료")
    alert.addButton(withTitle: "취소")
    alert.addButton(withTitle: "저장하지 않고 종료")

    let response = alert.runModal()
    switch response {
    case .alertFirstButtonReturn:
      for tab in modifiedTabs {
        WorkspaceFileDialogPresenter.presentSavePanel(viewModel: vm, tabID: tab.id)
        if vm.openTabs.first(where: { $0.id == tab.id })?.isModified == true {
          vm.cancelCloseTab()
          return .terminateCancel
        }
      }
      vm.cancelCloseTab()
      return .terminateNow
    case .alertSecondButtonReturn:
      vm.cancelCloseTab()
      return .terminateCancel
    default:
      vm.cancelCloseTab()
      return .terminateNow
    }
  }

  func application(_ sender: NSApplication, openFiles filenames: [String]) {
    let urls = filenames.map { URL(fileURLWithPath: $0) }
    guard let vm = viewModel else {
      pendingOpenURLs.append(contentsOf: urls)
      sender.reply(toOpenOrPrint: .success)
      return
    }
    vm.handleIncomingOpenURLs(urls)
    sender.reply(toOpenOrPrint: vm.lastErrorMessage == nil ? .success : .failure)
  }

  func application(_ _: NSApplication, open urls: [URL]) {
    guard let vm = viewModel else {
      pendingOpenURLs.append(contentsOf: urls)
      return
    }
    vm.handleIncomingOpenURLs(urls)
  }
}

@main
struct EasyOfficeApplication: App {
  @NSApplicationDelegateAdaptor(EasyOfficeAppDelegate.self) private var appDelegate
  @State private var viewModel = WorkspaceViewModel()

  @MainActor
  private func syncActiveTextViewSelectionIfNeeded() {
    guard let textView = NSApp.keyWindow?.firstResponder as? NSTextView,
      let coordinator = textView.delegate as? SelectableParagraphTextView.Coordinator
    else { return }
    let nsRange = textView.selectedRange()
    let utf16Range: Range<Int>? =
      nsRange.length > 0 ? (nsRange.location..<(nsRange.location + nsRange.length)) : nil
    viewModel.updateSelectedParagraphSelection(
      paragraphID: coordinator.parent.paragraph.id,
      utf16Range: utf16Range
    )
  }

  var body: some Scene {
    WindowGroup("EasyOffice") {
      MainWorkspaceWindowView(viewModel: viewModel)
        .frame(
          minWidth: EasyOfficeDesignTokens.LayoutMetrics.minimumWindowWidth,
          minHeight: EasyOfficeDesignTokens.LayoutMetrics.minimumWindowHeight
        )
        .onAppear {
          viewModel.restorePersistedRecentDocuments()
          viewModel.restorePersistedCustomTemplates()
          appDelegate.bind(viewModel: viewModel)
        }
        .onOpenURL { url in
          viewModel.handleIncomingOpenURLs([url])
        }
    }
    .commands {
      CommandMenu("파일 및 편집") {
        Button("새 빈 문서 (⌘N)") {
          _ = viewModel.openNewBlankTab(for: viewModel.activeTab?.moduleKind ?? .document)
        }
        .keyboardShortcut("n", modifiers: [.command])

        Button("파일 열기...") {
          WorkspaceFileDialogPresenter.presentOpenPanel(viewModel: viewModel)
        }
        .keyboardShortcut("o", modifiers: [.command])

        Button("현재 탭 저장") {
          WorkspaceFileDialogPresenter.presentSavePanel(viewModel: viewModel)
        }
        .keyboardShortcut("s", modifiers: [.command])
        .disabled(viewModel.activeTab == nil)

        Button("다른 이름으로 저장... (⇧⌘S)") {
          WorkspaceFileDialogPresenter.presentSaveAsPanel(viewModel: viewModel)
        }
        .keyboardShortcut("s", modifiers: [.command, .shift])
        .disabled(viewModel.activeTab == nil)

        Button("현재 탭 닫기 (⌘W)") {
          if let activeID = viewModel.activeTabID {
            viewModel.requestCloseTab(id: activeID)
          }
        }
        .keyboardShortcut("w", modifiers: [.command])
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
      CommandMenu("서식 및 단축키") {
        Button("굵게 (⌘B)") {
          syncActiveTextViewSelectionIfNeeded()
          if let m = viewModel.activeTab?.moduleKind {
            viewModel.executeRibbonCommand("\(m.rawValue)-home-bold")
          }
        }
        .keyboardShortcut("b", modifiers: [.command])
        .disabled(viewModel.activeTab == nil)

        Button("기울임 (⌘I)") {
          syncActiveTextViewSelectionIfNeeded()
          if let m = viewModel.activeTab?.moduleKind {
            viewModel.executeRibbonCommand("\(m.rawValue)-home-italic")
          }
        }
        .keyboardShortcut("i", modifiers: [.command])
        .disabled(viewModel.activeTab == nil)

        Button("밑줄 (⌘U)") {
          syncActiveTextViewSelectionIfNeeded()
          if let m = viewModel.activeTab?.moduleKind {
            viewModel.executeRibbonCommand("\(m.rawValue)-home-underline")
          }
        }
        .keyboardShortcut("u", modifiers: [.command])
        .disabled(viewModel.activeTab == nil)

        Button("책갈피·링크 삽입 (⌘K)") {
          syncActiveTextViewSelectionIfNeeded()
          if let m = viewModel.activeTab?.moduleKind {
            viewModel.executeRibbonCommand("\(m.rawValue)-insert-link")
          }
        }
        .keyboardShortcut("k", modifiers: [.command])
        .disabled(viewModel.activeTab == nil)

        Divider()

        Button("자동 합계 (⌥⇧=)") {
          if let m = viewModel.activeTab?.moduleKind {
            viewModel.executeRibbonCommand("\(m.rawValue)-data-autosum")
          }
        }
        .keyboardShortcut("=", modifiers: [.option, .shift])
        .disabled(viewModel.activeTab == nil)
      }
      CommandMenu("문서 생성") {
        ForEach(OfficeModuleKind.allCases) { module in
          Button("새 빈 \(module.displayNameKo)") {
            _ = viewModel.openNewBlankTab(for: module)
          }
        }
        Divider()
        ForEach(OfficeModuleKind.allCases) { module in
          Button("샘플 \(module.displayNameKo)") {
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
