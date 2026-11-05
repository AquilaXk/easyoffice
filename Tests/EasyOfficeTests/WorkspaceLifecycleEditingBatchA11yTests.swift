import XCTest

@testable import EasyOfficeCore
@testable import EasyOfficeDesignSystem

@MainActor
final class WorkspaceLifecycleEditingBatchA11yTests: XCTestCase {

  func testExternalFileOverwriteProtectionAndSameModuleConversionUndo() throws {
    let vm = WorkspaceViewModel()
    let tempDir = FileManager.default.temporaryDirectory
      .appendingPathComponent("eo-batchd-save-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: tempDir) }

    let externalDoc = OfficeDocumentModel(
      title: "외부 원본 문서",
      format: .docx,
      paragraphs: [ParagraphBlock(style: .body, text: "외부 시스템에서 작성된 원본 문단입니다.")]
    )
    let externalBytes = try OOXMLContainerCodec.encodeDocument(externalDoc)
    let externalURL = tempDir.appendingPathComponent("external-original.docx")
    try externalBytes.write(to: externalURL, options: .atomic)

    let importedTab = try vm.importDocumentFromDisk(url: externalURL)
    XCTAssertFalse(
      importedTab.isLosslessNativeOrigin,
      "Externally imported disk document must not be marked as lossless native origin by default"
    )

    vm.appendParagraphToActiveDocument(text: "편집 후 추가된 문단", style: .body)
    XCTAssertEqual(vm.activeTab?.isModified, true)

    let didOverwriteWithoutConfirm = try vm.saveTabOverwriteIfBackedByFile(id: importedTab.id)
    XCTAssertFalse(
      didOverwriteWithoutConfirm,
      "saveTabOverwriteIfBackedByFile must refuse silent overwrite of external file and route to Save As / confirmation"
    )
    XCTAssertEqual(vm.pendingLossyOverwriteConfirmationTabID, importedTab.id)
    let diskBytesAfterRefusal = try Data(contentsOf: externalURL)
    XCTAssertEqual(
      diskBytesAfterRefusal,
      externalBytes,
      "Original external file bytes on disk must remain untouched when overwrite is refused"
    )

    let didOverwriteWithConfirm = try vm.saveTabOverwriteIfBackedByFile(
      id: importedTab.id,
      allowExternalOverwrite: true
    )
    XCTAssertTrue(didOverwriteWithConfirm)
    let backupURL = externalURL.appendingPathExtension("bak")
    XCTAssertTrue(
      FileManager.default.fileExists(atPath: backupURL.path),
      "Confirmed external overwrite must preserve original bytes in a .bak backup file"
    )
    XCTAssertEqual(try Data(contentsOf: backupURL), externalBytes)
    XCTAssertEqual(vm.activeTab?.isLosslessNativeOrigin, true)

    vm.selectedParagraphID = try XCTUnwrap(vm.activeDocument?.paragraphs.first?.id)
    vm.setActiveParagraphFont(fontName: "Georgia")
    let richParagraphCountBeforeTxt = try XCTUnwrap(vm.activeDocument?.paragraphs.count)
    let txtURL = tempDir.appendingPathComponent("converted-plain.txt")
    try vm.saveTabToDisk(id: importedTab.id, to: txtURL, targetFormat: .txt)
    XCTAssertEqual(vm.activeTab?.format, .txt)
    XCTAssertTrue(
      vm.canUndo,
      "Same-module conversion in saveTabToDisk must record an undo snapshot before replacing payload"
    )

    vm.undo()
    XCTAssertEqual(vm.activeTab?.format, .docx)
    XCTAssertEqual(vm.activeTab?.fileName, "external-original.docx")
    XCTAssertEqual(vm.activeTab?.fileURL?.standardizedFileURL, externalURL.standardizedFileURL)
    XCTAssertEqual(vm.activeDocument?.paragraphs.count, richParagraphCountBeforeTxt)
    XCTAssertEqual(vm.activeDocument?.paragraphs.first?.fontName, "Georgia")
  }

  func testLazyRecentDocumentRestoreAndUnreadableFileIsolation() throws {
    let tempDir = FileManager.default.temporaryDirectory
      .appendingPathComponent("eo-batchd-recent-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: tempDir) }

    let vm = WorkspaceViewModel()
    let validURL = tempDir.appendingPathComponent("valid-recent.docx")
    let blankTab = vm.openNewBlankTab(for: .document)
    try vm.saveTabToDisk(id: blankTab.id, to: validURL)
    vm.closeTab(id: blankTab.id)

    try Data([0x00, 0xFF, 0x13, 0x37]).write(to: validURL, options: .atomic)
    XCTAssertNil(
      vm.reopenLastClosedTab(),
      "reopenLastClosedTab must fail closed and return nil when the closed tab's disk file is unreadable"
    )
    XCTAssertFalse(
      vm.openTabs.contains(where: {
        $0.fileURL?.standardizedFileURL.path == validURL.standardizedFileURL.path
      })
    )

    let freshVM = WorkspaceViewModel()
    freshVM.restorePersistedRecentDocuments()
    let restoredRecord = try XCTUnwrap(
      freshVM.recentDocumentRecords.first(where: {
        $0.fileURL?.standardizedFileURL.path == validURL.standardizedFileURL.path
      })
    )
    XCTAssertTrue(
      restoredRecord.isMetadataOnlyPlaceholder,
      "Restored disk-backed recent document record must be a lightweight metadata placeholder until opened"
    )

    let openTabCountBefore = freshVM.openTabs.count
    _ = freshVM.activateOrReopenRecentTab(restoredRecord)

    XCTAssertEqual(
      freshVM.openTabs.count,
      openTabCountBefore,
      "Failed recent document load must not open a blank fallback tab"
    )
    XCTAssertFalse(
      freshVM.openTabs.contains(where: {
        $0.fileURL?.standardizedFileURL.path == validURL.standardizedFileURL.path
      }),
      "Failed recent document load must never attach the disk fileURL to a blank fallback tab"
    )
    XCTAssertNotNil(freshVM.lastErrorMessage)
    let updatedRecord = try XCTUnwrap(
      freshVM.recentDocumentRecords.first(where: { $0.id == restoredRecord.id })
    )
    XCTAssertTrue(updatedRecord.isUnreadable)
  }

  func testParagraphSplitMergePreservesRunsDeletesSelectionRelinksAllAnchorsAndHandlesZWJ() throws {
    let vm = WorkspaceViewModel()
    let tab = vm.openNewBlankTab(for: .document)
    vm.activeTabID = tab.id

    let p0ID = try XCTUnwrap(vm.activeDocument?.paragraphs.first?.id)
    let styledRuns: [DocumentTextRun] = [
      DocumentTextRun(
        text: "안녕하세요 ",
        isBold: true,
        fontName: "Pretendard",
        fontSizePt: 14,
        foregroundHex: "#5C6BC0"
      ),
      DocumentTextRun(
        text: "선택삭제구간",
        isItalic: true,
        isUnderlined: true,
        foregroundHex: "#C81E1E"
      ),
      DocumentTextRun(
        text: "반갑습니다",
        isStrikethrough: true,
        isSuperscript: true,
        isCode: true,
        backgroundHex: "#EEF0FF",
        linkURL: "https://example.com"
      ),
    ]

    if case .document(var doc) = vm.activeTab?.payload {
      doc.paragraphs[0].text = "안녕하세요 선택삭제구간반갑습니다"
      doc.paragraphs[0].runs = styledRuns
      doc.comments.append(
        DocumentCommentThread(
          anchorParagraphID: p0ID,
          author: "검토자",
          message: "문단 주석"
        )
      )
      doc.bookmarks["BM_TARGET"] = p0ID
      doc.revisions.append(
        TrackedRevision(
          author: "편집자",
          kind: .insertion,
          paragraphID: p0ID,
          originalText: "",
          proposedText: "문단 수정 이력"
        )
      )
      vm.updateActiveTabPayload(.document(doc))
    }

    vm.selectedParagraphID = p0ID
    vm.selectedParagraphUTF16Range = 6..<12
    vm.splitParagraph(paragraphID: p0ID, atUTF16Offset: 6)

    let docAfterSplit = try XCTUnwrap(vm.activeDocument)
    XCTAssertEqual(docAfterSplit.paragraphs.count, 2)
    XCTAssertEqual(docAfterSplit.paragraphs[0].text, "안녕하세요 ")
    XCTAssertEqual(docAfterSplit.paragraphs[1].text, "반갑습니다")

    XCTAssertEqual(docAfterSplit.paragraphs[0].runs.count, 1)
    XCTAssertEqual(docAfterSplit.paragraphs[0].runs[0].text, "안녕하세요 ")
    XCTAssertTrue(docAfterSplit.paragraphs[0].runs[0].isBold)
    XCTAssertEqual(docAfterSplit.paragraphs[0].runs[0].foregroundHex, "#5C6BC0")

    XCTAssertEqual(docAfterSplit.paragraphs[1].runs.count, 1)
    XCTAssertEqual(docAfterSplit.paragraphs[1].runs[0].text, "반갑습니다")
    XCTAssertTrue(docAfterSplit.paragraphs[1].runs[0].isStrikethrough)
    XCTAssertTrue(docAfterSplit.paragraphs[1].runs[0].isSuperscript)
    XCTAssertTrue(docAfterSplit.paragraphs[1].runs[0].isCode)
    XCTAssertEqual(docAfterSplit.paragraphs[1].runs[0].linkURL, "https://example.com")

    let secondID = docAfterSplit.paragraphs[1].id
    XCTAssertEqual(vm.pendingFocusParagraphID, secondID)
    XCTAssertEqual(vm.pendingCaretUTF16Offset, 0)

    if case .document(var doc2) = vm.activeTab?.payload {
      doc2.comments[0].anchorParagraphID = secondID
      doc2.bookmarks["BM_SECOND"] = secondID
      doc2.revisions[0].paragraphID = secondID
      doc2.footnotes.append(
        FootnoteItem(number: 1, anchorParagraphID: secondID, text: "각주 설명")
      )
      vm.updateActiveTabPayload(.document(doc2))
    }

    vm.mergeParagraphWithPrevious(paragraphID: secondID)
    let docAfterMerge = try XCTUnwrap(vm.activeDocument)
    XCTAssertEqual(docAfterMerge.paragraphs.count, 1)
    XCTAssertEqual(docAfterMerge.paragraphs[0].text, "안녕하세요 반갑습니다")
    XCTAssertEqual(docAfterMerge.paragraphs[0].runs.count, 2)
    XCTAssertTrue(docAfterMerge.paragraphs[0].runs[0].isBold)
    XCTAssertTrue(docAfterMerge.paragraphs[0].runs[1].isStrikethrough)

    XCTAssertEqual(docAfterMerge.comments.first?.anchorParagraphID, p0ID)
    XCTAssertEqual(docAfterMerge.bookmarks["BM_SECOND"], p0ID)
    XCTAssertEqual(docAfterMerge.revisions.first?.paragraphID, p0ID)
    XCTAssertEqual(docAfterMerge.footnotes.first?.anchorParagraphID, p0ID)
    XCTAssertEqual(vm.pendingFocusParagraphID, p0ID)
    XCTAssertEqual(vm.pendingCaretUTF16Offset, ("안녕하세요 " as NSString).length)

    let zwjEmoji = "👨‍👩‍👧‍👦"
    let nfdHangul = "한글".decomposedStringWithCanonicalMapping
    vm.updateParagraphText(paragraphID: p0ID, newText: "\(zwjEmoji)\(nfdHangul)")
    vm.splitParagraph(paragraphID: p0ID, atUTF16Offset: 2)
    let docAfterZWJ = try XCTUnwrap(vm.activeDocument)
    XCTAssertEqual(docAfterZWJ.paragraphs.count, 2)
    XCTAssertEqual(docAfterZWJ.paragraphs[0].text, zwjEmoji)
    XCTAssertEqual(docAfterZWJ.paragraphs[1].text, nfdHangul)
  }

  func testManuscriptGridEditingPreservesUnchangedParagraphIDsRunsAndAnchors() throws {
    let vm = WorkspaceViewModel()
    let tab = vm.openNewBlankTab(for: .hwp)
    vm.activeTabID = tab.id

    let p1 = ParagraphBlock(
      style: .heading1,
      text: "제1장 원고지 보존 검증",
      runs: [DocumentTextRun(text: "제1장 원고지 보존 검증", isBold: true, foregroundHex: "#5C6BC0")]
    )
    let p2 = ParagraphBlock(
      style: .body,
      text: "중간 문단 원본 텍스트"
    )
    let p3 = ParagraphBlock(
      style: .quote,
      text: "마지막 문단 인용구 보존",
      runs: [DocumentTextRun(text: "마지막 문단 인용구 보존", isItalic: true, foregroundHex: "#0F7B55")]
    )

    if case .document(var doc) = vm.activeTab?.payload {
      doc.paragraphs = [p1, p2, p3]
      doc.comments = [
        DocumentCommentThread(anchorParagraphID: p3.id, author: "감수자", message: "마지막 문단 주석")
      ]
      doc.bookmarks["END_MARK"] = p3.id
      vm.updateActiveTabPayload(.document(doc))
    }

    let editedFullText = [
      "제1장 원고지 보존 검증",
      "중간 문단 수정본 텍스트",
      "새로 삽입된 중간 문단",
      "마지막 문단 인용구 보존",
    ].joined(separator: "\n")

    vm.updateActiveDocumentManuscriptFullText(editedFullText)
    let updatedDoc = try XCTUnwrap(vm.activeDocument)
    XCTAssertEqual(updatedDoc.paragraphs.count, 4)

    XCTAssertEqual(updatedDoc.paragraphs[0].id, p1.id)
    XCTAssertEqual(updatedDoc.paragraphs[0].style, .heading1)
    XCTAssertEqual(updatedDoc.paragraphs[0].runs.first?.isBold, true)

    XCTAssertEqual(updatedDoc.paragraphs[3].id, p3.id)
    XCTAssertEqual(updatedDoc.paragraphs[3].style, .quote)
    XCTAssertEqual(updatedDoc.paragraphs[3].runs.first?.isItalic, true)
    XCTAssertEqual(updatedDoc.comments.first?.anchorParagraphID, p3.id)
    XCTAssertEqual(updatedDoc.bookmarks["END_MARK"], p3.id)
  }

  func testBatchConversionPathDeduplicationCollisionDisambiguationAndWriteFailure() throws {
    let tempDir = FileManager.default.temporaryDirectory
      .appendingPathComponent("eo-batchd-conv-\(UUID().uuidString)", isDirectory: true)
    let folderA = tempDir.appendingPathComponent("folderA", isDirectory: true)
    let folderB = tempDir.appendingPathComponent("folderB", isDirectory: true)
    let outDir = tempDir.appendingPathComponent("output", isDirectory: true)
    try FileManager.default.createDirectory(at: folderA, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: folderB, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: tempDir) }

    let docA = OfficeDocumentModel(
      title: "A보고서",
      format: .docx,
      paragraphs: [ParagraphBlock(style: .body, text: "폴더 A의 보고서 본문")]
    )
    let docB = OfficeDocumentModel(
      title: "B보고서",
      format: .docx,
      paragraphs: [ParagraphBlock(style: .body, text: "폴더 B의 보고서 본문")]
    )
    let urlA = folderA.appendingPathComponent("report.docx")
    let urlB = folderB.appendingPathComponent("report.docx")
    try OOXMLContainerCodec.encodeDocument(docA).write(to: urlA)
    try OOXMLContainerCodec.encodeDocument(docB).write(to: urlB)

    let vm = WorkspaceViewModel()
    vm.openTabs.removeAll()
    vm.batchTargetFormat = .pdf

    vm.enqueueFilesFromDiskIntoBatchConversion(urls: [urlA, urlB])
    XCTAssertEqual(
      vm.batchConversionQueue.count,
      2,
      "Files with identical names in different folders must both remain in batchConversionQueue"
    )

    vm.runAllBatchConversions(outputDirectory: outDir)
    let report = try XCTUnwrap(vm.lastBatchConversionReport)
    XCTAssertEqual(report.totalJobs, 2)
    XCTAssertEqual(report.succeeded.count, 2)
    XCTAssertEqual(report.failedErrors.count, 0)

    let outputNames = Set(report.succeeded.map(\.outputFileName))
    XCTAssertEqual(
      outputNames,
      ["report.pdf", "report_2.pdf"],
      "Colliding output filenames must be disambiguated with _2 suffix"
    )
    XCTAssertTrue(FileManager.default.fileExists(atPath: outDir.appendingPathComponent("report.pdf").path))
    XCTAssertTrue(FileManager.default.fileExists(atPath: outDir.appendingPathComponent("report_2.pdf").path))

    vm.clearBatchConversionQueue()
    XCTAssertNil(
      vm.lastBatchConversionReport,
      "Clearing the batch queue must invalidate stale lastBatchConversionReport"
    )
    XCTAssertEqual(vm.batchConversionQueue.count, 0)

    let blockingFileURL = tempDir.appendingPathComponent("not-a-directory.txt")
    try Data("blocking file".utf8).write(to: blockingFileURL)
    let unwritableOutDir = blockingFileURL.appendingPathComponent("subdir", isDirectory: true)

    let failingReport = DocumentConversionOrchestrator.executeBatch(
      [
        ConversionJobRequest(
          sourceFileName: "report.docx",
          sourceFileURL: urlA,
          sourceFormat: .docx,
          targetFormat: .pdf,
          payload: .document(docA)
        )
      ],
      outputDirectory: unwritableOutDir
    )
    XCTAssertEqual(
      failingReport.succeeded.count,
      0,
      "Job whose disk write failed must not be counted in succeeded"
    )
    XCTAssertEqual(
      failingReport.failedErrors.count,
      1,
      "Job whose disk write failed must be recorded in failedErrors"
    )
  }

  func testColdLaunchURLQueueMultiDirtyTerminationCustomTemplatePersistenceAndTSVPasteBounds() throws {
    let tempDir = FileManager.default.temporaryDirectory
      .appendingPathComponent("eo-batchd-ws-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: tempDir) }

    let coldURL = tempDir.appendingPathComponent("cold-launch.md")
    try Data("# 콜드 런치 문서\n정상 열림 확인".utf8).write(to: coldURL)

    let vm = WorkspaceViewModel()
    vm.isWorkspaceReadyForOpenURLs = false
    vm.handleIncomingOpenURLs([coldURL])
    XCTAssertEqual(vm.pendingLaunchURLs, [coldURL])
    XCTAssertFalse(vm.openTabs.contains(where: { $0.fileName == "cold-launch.md" }))

    vm.markWorkspaceReadyAndDrainOpenURLs()
    XCTAssertTrue(vm.pendingLaunchURLs.isEmpty)
    XCTAssertTrue(vm.openTabs.contains(where: { $0.fileName == "cold-launch.md" }))

    let dirty1 = vm.openNewBlankTab(for: .document)
    vm.activeTabID = dirty1.id
    vm.appendParagraphToActiveDocument(text: "미저장 탭 1", style: .body)

    let dirty2 = vm.openNewBlankTab(for: .sheet)
    vm.activeTabID = dirty2.id
    vm.selectedSheetCell = "A1"
    vm.formulaBarDraft = "999"
    vm.applyFormulaBarEdit()

    XCTAssertEqual(vm.dirtyOpenTabs.count, 2)
    XCTAssertFalse(vm.requestApplicationTermination())
    XCTAssertEqual(vm.pendingCloseConfirmTabID, dirty1.id)

    vm.confirmDiscardAndClosePendingTab()
    XCTAssertEqual(
      vm.pendingCloseConfirmTabID,
      dirty2.id,
      "After resolving the first dirty tab during quit, confirmation must advance to the next dirty tab"
    )

    let customTab = vm.openNewBlankTab(for: .document)
    vm.activeTabID = customTab.id
    let uniqueTemplateTitle = "영속화검증템플릿_\(UUID().uuidString.prefix(8))"
    vm.saveActiveTabAsCustomTemplate(title: uniqueTemplateTitle, subtitle: "재실행 복원 테스트")

    let secondVM = WorkspaceViewModel()
    secondVM.restorePersistedCustomTemplates()
    let restoredTemplate = try XCTUnwrap(
      secondVM.customTemplates.first(where: { $0.title == uniqueTemplateTitle })
    )
    secondVM.openTab(from: restoredTemplate)
    XCTAssertEqual(secondVM.activeTab?.moduleKind, .document)
    secondVM.removeCustomTemplate(id: restoredTemplate.id)

    let sheetTab = vm.openNewBlankTab(for: .sheet)
    vm.activeTabID = sheetTab.id
    vm.selectedSheetCell = "XFD1048576"
    vm.pasteSheetCellsFromTSV("FIRST_EDGE\tOUT_COL\nOUT_ROW_1\tOUT_ROW_2")

    let sheetAfterPaste = try XCTUnwrap(vm.activeWorksheet)
    XCTAssertEqual(
      sheetAfterPaste.rawValue(at: "XFD1048576"),
      "FIRST_EDGE",
      "Out-of-bounds TSV cells must not clamp and overwrite the boundary cell XFD1048576"
    )
    XCTAssertNotNil(
      vm.lastErrorMessage,
      "Out-of-bounds TSV cells must report a boundary warning in lastErrorMessage"
    )
  }

  func testWCAGAAContrastRatiosAndRibbonSemantics() throws {
    let lightSurfaces = [
      EasyOfficeDesignTokens.lightWhite,
      EasyOfficeDesignTokens.lightScaffold,
      EasyOfficeDesignTokens.lightSubtle,
    ]
    let lightForegrounds = [
      EasyOfficeDesignTokens.inkPrimary,
      EasyOfficeDesignTokens.inkSecondary,
      EasyOfficeDesignTokens.brand600,
      EasyOfficeDesignTokens.brand700,
      EasyOfficeDesignTokens.statusSuccess,
      EasyOfficeDesignTokens.statusWarning,
      EasyOfficeDesignTokens.statusDanger,
      EasyOfficeDesignTokens.statusInfo,
    ] + OfficeModuleKind.allCases.map { EasyOfficeDesignTokens.accentToken(for: $0, isDark: false) }

    for surface in lightSurfaces {
      for fg in lightForegrounds {
        let ratio = fg.contrastRatio(against: surface)
        XCTAssertGreaterThanOrEqual(
          ratio,
          4.5,
          "Light foreground \(fg.hex) against surface \(surface.hex) must be >= 4.5:1 (got \(ratio))"
        )
      }
    }

    let darkSurfaces = [
      EasyOfficeDesignTokens.darkScaffold,
      EasyOfficeDesignTokens.darkSurface,
      EasyOfficeDesignTokens.darkElevated,
    ]
    let darkForegrounds = [
      EasyOfficeDesignTokens.darkText,
      EasyOfficeDesignTokens.darkMuted,
      EasyOfficeDesignTokens.brand300,
      EasyOfficeDesignTokens.brandAccentToken(isDark: true),
      EasyOfficeDesignTokens.statusSuccessToken(isDark: true),
      EasyOfficeDesignTokens.statusWarningToken(isDark: true),
      EasyOfficeDesignTokens.statusDangerToken(isDark: true),
      EasyOfficeDesignTokens.statusInfoToken(isDark: true),
    ] + OfficeModuleKind.allCases.map { EasyOfficeDesignTokens.accentToken(for: $0, isDark: true) }

    for surface in darkSurfaces {
      for fg in darkForegrounds {
        let ratio = fg.contrastRatio(against: surface)
        XCTAssertGreaterThanOrEqual(
          ratio,
          4.5,
          "Dark foreground \(fg.hex) against surface \(surface.hex) must be >= 4.5:1 (got \(ratio))"
        )
      }
    }

    let vm = WorkspaceViewModel()
    XCTAssertEqual(vm.presenterElapsedSeconds, 0, "presenterElapsedSeconds must start at 0, not 95")

    let pdfTab = vm.openNewSampleTab(for: .pdf)
    vm.activeTabID = pdfTab.id
    let initialZoom = vm.zoomPercent
    let initialViewMode = vm.workspaceViewMode
    let initialPaper = vm.pdfPaperSizePreset
    let initialCols = vm.pdfPageColumnCount

    vm.executeRibbonCommand("doc-layout-size")
    XCTAssertEqual(vm.zoomPercent, initialZoom, "PDF paper size command must not mutate zoomPercent")
    XCTAssertNotEqual(vm.pdfPaperSizePreset, initialPaper)

    vm.executeRibbonCommand("doc-layout-columns")
    XCTAssertEqual(
      vm.workspaceViewMode,
      initialViewMode,
      "PDF columns command must not toggle readOnlyView"
    )
    XCTAssertNotEqual(vm.pdfPageColumnCount, initialCols)

    let slideTab = vm.openNewSampleTab(for: .slide)
    vm.activeTabID = slideTab.id
    vm.selectedSlideElementID = nil
    let elementCountBefore = try XCTUnwrap(vm.activePresentation?.slides.first?.elements.count)
    vm.deleteActiveSlideElement()
    let elementCountAfter = try XCTUnwrap(vm.activePresentation?.slides.first?.elements.count)
    XCTAssertEqual(
      elementCountAfter,
      elementCountBefore,
      "deleteActiveSlideElement must not delete the last element when selectedSlideElementID is nil"
    )

    vm.openTabs.removeAll()
    _ = vm.openNewBlankTab(for: .document)
    XCTAssertNil(
      vm.compareOpenDocumentTabs(),
      "compareOpenDocumentTabs must return nil and set an error when fewer than 2 document tabs are open"
    )
    XCTAssertNotNil(vm.lastErrorMessage)

    let largeParagraphs = (0..<2_055).map { ParagraphBlock(style: .body, text: "문단 \($0)") }
    let largeDocA = OfficeDocumentModel(title: "대용량A", format: .docx, paragraphs: largeParagraphs)
    let largeDocB = OfficeDocumentModel(title: "대용량B", format: .docx, paragraphs: largeParagraphs)
    let truncatedDiff = LocalSmartDocumentAssistant.compareDocuments(
      baseline: largeDocA,
      revised: largeDocB
    )
    XCTAssertTrue(truncatedDiff.isTruncated)
    XCTAssertEqual(truncatedDiff.truncatedAtParagraphLimit, 2_048)

    let clampedSpan = TableCellSpan(
      rowIndex: 999_999,
      columnIndex: -5,
      rowSpan: 999,
      columnSpan: 0
    )
    XCTAssertEqual(clampedSpan.rowIndex, TableCellSpan.maxIndex)
    XCTAssertEqual(clampedSpan.columnIndex, 0)
    XCTAssertEqual(clampedSpan.rowSpan, TableCellSpan.maxSpan)
    XCTAssertEqual(clampedSpan.columnSpan, 1)

    let invalidRowJSON = Data(#"{"rowIndex":200000,"columnIndex":0,"rowSpan":1,"columnSpan":1}"#.utf8)
    XCTAssertThrowsError(try JSONDecoder().decode(TableCellSpan.self, from: invalidRowJSON))
    let clampedSpanJSON = Data(#"{"rowIndex":0,"columnIndex":0,"rowSpan":999,"columnSpan":0}"#.utf8)
    let decodedClamped = try JSONDecoder().decode(TableCellSpan.self, from: clampedSpanJSON)
    XCTAssertEqual(decodedClamped.rowSpan, TableCellSpan.maxSpan)
    XCTAssertEqual(decodedClamped.columnSpan, 1)

    let pdfWithOutline = PDFStudioDocumentModel(
      title: "북마크 분할 검증",
      pages: [
        PDFPageDescriptor(pageNumber: 1, rotationDegrees: 0, extractedText: "1장 서론"),
        PDFPageDescriptor(pageNumber: 2, rotationDegrees: 0, extractedText: "2장 본론"),
        PDFPageDescriptor(pageNumber: 3, rotationDegrees: 0, extractedText: "2.1절 세부"),
      ],
      outline: [
        PDFOutlineNode(
          title: "1장",
          targetPageNumber: 1,
          children: [
            PDFOutlineNode(title: "2.1절(상위 1장 아래 하위 북마크)", targetPageNumber: 3)
          ]
        ),
        PDFOutlineNode(title: "2장", targetPageNumber: 2),
      ]
    )
    let (firstPart, secondPart) = pdfWithOutline.split(atOneBasedPage: 1)
    XCTAssertEqual(firstPart.outline.map(\.title), ["1장"])
    XCTAssertEqual(firstPart.outline.first?.targetPageNumber, 1)
    XCTAssertTrue(firstPart.outline.first?.children.isEmpty ?? false)

    XCTAssertEqual(
      secondPart.outline.map(\.title),
      ["2.1절(상위 1장 아래 하위 북마크)", "2장"],
      "Extracted/split PDF part must preserve and remap outline bookmarks targeting extracted pages"
    )
    XCTAssertEqual(secondPart.outline.map(\.targetPageNumber), [2, 1])
  }

  @MainActor
  func testMacOSWorkspaceUXErgonomicsAndDisconnectFixes() {
    let vm = WorkspaceViewModel()

    let sheetFormulasCommands = RibbonCategory.actions(for: .formulasAndData, module: .sheet)
    let sortCommand = sheetFormulasCommands.first(where: { $0.id == "\(OfficeModuleKind.sheet.rawValue)-data-sort" })
    XCTAssertEqual(sortCommand?.shortcutHint, "⌃⌥S")

    vm.isShowingHomeHub = false
    if let docTab = vm.openTabs.first(where: { $0.moduleKind == .document }) {
      vm.activeTabID = docTab.id
    }
    vm.isFindBarVisible = false
    vm.searchQuery = "이지오피스"
    XCTAssertEqual(vm.findQueryText, "이지오피스")
    XCTAssertTrue(vm.isFindBarVisible)
    vm.performFindNext()
    XCTAssertFalse((vm.lastExecutedCommandSummary ?? "").isEmpty)

    if let sheetTab = vm.openTabs.first(where: { $0.moduleKind == .sheet }) {
      vm.activeTabID = sheetTab.id
      vm.selectSheetCellOrRange(primaryCell: "B2")
      vm.moveSheetSelection(rowDelta: 1, columnDelta: 1, extendSelection: false)
      XCTAssertEqual(vm.selectedSheetCell, "C3")
      vm.moveSheetSelection(rowDelta: 1, columnDelta: 0, extendSelection: true)
      XCTAssertEqual(vm.selectedSheetRange, "C3:C4")
      vm.clearSelectedSheetRange()
      if case .sheet(let updatedSheet) = vm.activeTab?.payload {
        XCTAssertNil(updatedSheet.cellsByCoordinate[PackedCellCoordinate(row: 2, column: 2)])
        XCTAssertNil(updatedSheet.cellsByCoordinate[PackedCellCoordinate(row: 3, column: 2)])
      }
    }

    if let docIdx = vm.openTabs.firstIndex(where: { $0.moduleKind == .document }),
      case .document(var doc) = vm.openTabs[docIdx].payload
    {
      vm.activeTabID = vm.openTabs[docIdx].id
      let secondTable = DocumentTableBlock(
        caption: "표 2. 부서별 세부 집행표",
        rows: [
          ["부서", "예산", "집행"],
          ["연구개발", "100", "90"],
        ],
        hasHeaderRow: true
      )
      doc.tables.append(secondTable)
      vm.openTabs[docIdx].payload = .document(doc)

      let table0InitialRows = doc.tables[0].rowCount
      let table0InitialCols = doc.tables[0].columnCount

      vm.selectDocumentTableCell(tableIndex: 1, row: 0, column: 0)
      XCTAssertEqual(vm.selectedDocumentTableIndex, 1)
      XCTAssertEqual(vm.selectedDocumentTableCell?.row, 0)
      XCTAssertEqual(vm.selectedDocumentTableCell?.column, 0)

      vm.insertRowInDocumentTable()
      vm.insertColumnInDocumentTable()
      vm.mergeActiveDocumentTableCells()

      if case .document(let afterInsertAndMerge) = vm.activeTab?.payload {
        XCTAssertEqual(afterInsertAndMerge.tables[0].rowCount, table0InitialRows)
        XCTAssertEqual(afterInsertAndMerge.tables[0].columnCount, table0InitialCols)
        XCTAssertEqual(afterInsertAndMerge.tables[1].rowCount, 3)
        XCTAssertEqual(afterInsertAndMerge.tables[1].columnCount, 4)
        XCTAssertNotNil(afterInsertAndMerge.tables[1].originSpan(atRow: 0, column: 0))
      }

      vm.splitActiveDocumentTableCell()
      vm.deleteRowInDocumentTable()
      vm.deleteColumnInDocumentTable()

      if case .document(let afterSplitAndDelete) = vm.activeTab?.payload {
        XCTAssertNil(afterSplitAndDelete.tables[1].originSpan(atRow: 0, column: 0))
        XCTAssertEqual(afterSplitAndDelete.tables[1].rowCount, 2)
        XCTAssertEqual(afterSplitAndDelete.tables[1].columnCount, 3)
      }
    }

    let extraTab = vm.openNewBlankTab(for: .document)
    XCTAssertTrue(vm.openTabs.contains(where: { $0.id == extraTab.id }))
    vm.removeRecentDocumentRecord(id: extraTab.id)
    XCTAssertFalse(vm.openTabs.contains(where: { $0.id == extraTab.id }))
    XCTAssertTrue(vm.recentDocumentRecords.contains(where: { $0.id == extraTab.id }))

    vm.removeRecentDocumentRecord(id: extraTab.id)
    XCTAssertFalse(vm.recentDocumentRecords.contains(where: { $0.id == extraTab.id }))
  }
}
