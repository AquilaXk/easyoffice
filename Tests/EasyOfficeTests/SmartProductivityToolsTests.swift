import XCTest
import PDFKit
@testable import EasyOfficeDesignSystem
@testable import EasyOfficeCore

final class SmartProductivityToolsTests: XCTestCase {

    // MARK: - 1. Bidirectional Format Conversion Studio & PDFBinarySerializer

    func testPDFBinarySerializerRoundTripPreservesPagesRotationTextAndAnnotations() throws {
        let originalPDF = PDFStudioDocumentModel(
            title: "2026 보안 감사 보고서",
            pages: [
                PDFPageDescriptor(
                    pageNumber: 1,
                    rotationDegrees: 90,
                    extractedText: "제1절 개요\n컨테이너 매직 시그니처 검증을 수행합니다."
                ),
                PDFPageDescriptor(
                    pageNumber: 2,
                    rotationDegrees: 180,
                    extractedText: "제2절 상세 결과\n압축 폭탄 및 XXE 방어 테스트 통과."
                )
            ],
            annotations: [
                PDFAnnotationItem(
                    pageNumber: 1,
                    kind: .highlight,
                    content: "핵심 보안 검증 항목",
                    author: "보안팀"
                ),
                PDFAnnotationItem(
                    pageNumber: 2,
                    kind: .stamp,
                    content: "검토 완료",
                    author: "품질관리"
                )
            ],
            outline: [
                PDFOutlineNode(title: "제1절 개요", targetPageNumber: 1),
                PDFOutlineNode(title: "제2절 상세 결과", targetPageNumber: 2)
            ],
            isRedacted: true
        )

        let pdfBytes = PDFBinarySerializer.encode(originalPDF)
        let signature = try FormatDetector.verify(data: pdfBytes, fileName: "audit_report.pdf")
        XCTAssertEqual(signature.format, .pdf)
        XCTAssertEqual(signature.containerFamily, .pdfBinary)

        let asciiPrefix = String(decoding: pdfBytes.prefix(8), as: UTF8.self)
        XCTAssertEqual(asciiPrefix, "%PDF-1.7")

        let decodedPDF = try PDFBinarySerializer.decode(from: pdfBytes)
        XCTAssertEqual(decodedPDF.title, "2026 보안 감사 보고서")
        XCTAssertEqual(decodedPDF.isRedacted, true)
        XCTAssertEqual(decodedPDF.pages.count, 2)
        XCTAssertEqual(decodedPDF.pages[0].pageNumber, 1)
        XCTAssertEqual(decodedPDF.pages[0].rotationDegrees, 90)
        XCTAssertEqual(decodedPDF.pages[0].extractedText, "제1절 개요\n컨테이너 매직 시그니처 검증을 수행합니다.")
        XCTAssertEqual(decodedPDF.pages[1].pageNumber, 2)
        XCTAssertEqual(decodedPDF.pages[1].rotationDegrees, 180)
        XCTAssertEqual(decodedPDF.pages[1].extractedText, "제2절 상세 결과\n압축 폭탄 및 XXE 방어 테스트 통과.")
        XCTAssertEqual(decodedPDF.annotations.count, 2)
        XCTAssertEqual(decodedPDF.annotations[0].kind, .highlight)
        XCTAssertEqual(decodedPDF.annotations[0].content, "핵심 보안 검증 항목")
        XCTAssertEqual(decodedPDF.annotations[0].author, "보안팀")
        XCTAssertEqual(decodedPDF.outline.count, 2)
        XCTAssertEqual(decodedPDF.outline[1].title, "제2절 상세 결과")
        XCTAssertEqual(decodedPDF.outline[1].targetPageNumber, 2)

        // Reject non-PDF data in PDFBinarySerializer.decode
        XCTAssertThrowsError(try PDFBinarySerializer.decode(from: Data("NOT_A_PDF".utf8))) { error in
            XCTAssertEqual(
                error as? EasyOfficeError,
                .signatureMismatch(expected: .pdfBinary, fileName: "document.pdf")
            )
        }
    }

    func testDocumentConversionOrchestratorAcrossAllCoreFormatsAndCrossDomain() throws {
        let sampleDoc = OfficeDocumentModel(
            title: "연간 기술 기획서",
            format: .docx,
            headerText: "문서번호: TECH-2026",
            footerText: "1 / 1",
            paragraphs: [
                ParagraphBlock(style: .title, text: "연간 기술 기획서", isBold: true),
                ParagraphBlock(style: .body, text: "표준 문서 컨테이너 간 무손실 양방향 변환을 검증합니다.")
            ],
            tables: [
                DocumentTableBlock(
                    caption: "표 1. 분기별 목표",
                    rows: [
                        ["분기", "목표치"],
                        ["1분기", "100"],
                        ["2분기", "250"]
                    ]
                )
            ]
        )

        let documentTargetFormats: [DocumentFormat] = [
            .docx, .hwpx, .hwp, .odt, .rtf, .md, .txt, .pdf, .xlsx, .csv
        ]

        for targetFormat in documentTargetFormats {
            let req = ConversionJobRequest(
                sourceFileName: "기획서.docx",
                sourceFormat: .docx,
                targetFormat: targetFormat,
                payload: .document(sampleDoc)
            )
            let result = try DocumentConversionOrchestrator.convert(req)
            XCTAssertEqual(result.id, req.id)
            XCTAssertEqual(result.targetFormat, targetFormat)
            XCTAssertEqual(result.outputFileName, "기획서.\(targetFormat.fileExtension)")
            XCTAssertEqual(result.verifiedSignature.format, targetFormat)

            let reVerified = try FormatDetector.verify(data: result.outputBytes, fileName: result.outputFileName)
            XCTAssertEqual(reVerified, result.verifiedSignature)
        }

        // Sheet -> XLSX, ODS, CSV, TSV, PDF, DOCX
        var sheet = WorksheetModel(name: "매출현황")
        try sheet.setCell("A1", rawValue: "지역")
        try sheet.setCell("B1", rawValue: "매출")
        try sheet.setCell("A2", rawValue: "서울")
        try sheet.setCell("B2", rawValue: "500")
        try sheet.setCell("A3", rawValue: "부산")
        try sheet.setCell("B3", rawValue: "=B2*2")

        for sheetTarget in [DocumentFormat.xlsx, .ods, .csv, .tsv, .pdf, .docx] {
            let req = ConversionJobRequest(
                sourceFileName: "매출현황.xlsx",
                sourceFormat: .xlsx,
                targetFormat: sheetTarget,
                payload: .sheet(sheet)
            )
            let res = try DocumentConversionOrchestrator.convert(req)
            XCTAssertEqual(res.verifiedSignature.format, sheetTarget)
            if sheetTarget == .docx, case let .document(convertedDoc) = res.convertedPayload {
                XCTAssertFalse(convertedDoc.tables.isEmpty)
                XCTAssertEqual(convertedDoc.tables[0].rows[2][1], "1000")
            }
        }

        // Slide -> PPTX, ODP, PDF
        let deck = PresentationDeckModel(
            title: "분기 브리핑",
            slides: [
                SlideModel(
                    title: "핵심 성과",
                    subtitle: "2026 상반기",
                    bulletPoints: ["수식 엔진 고도화", "보안 감사 자동화"]
                )
            ]
        )
        for slideTarget in [DocumentFormat.pptx, .ppsx, .odp, .pdf] {
            let req = ConversionJobRequest(
                sourceFileName: "브리핑.pptx",
                sourceFormat: .pptx,
                targetFormat: slideTarget,
                payload: .slide(deck)
            )
            let res = try DocumentConversionOrchestrator.convert(req)
            XCTAssertEqual(res.verifiedSignature.format, slideTarget)
        }

        // PDF -> DOCX, HWPX, PPTX
        let pdfSource = PDFStudioDocumentModel(
            title: "변환 원본 PDF",
            pages: [
                PDFPageDescriptor(pageNumber: 1, rotationDegrees: 0, extractedText: "슬라이드 제목\n핵심 항목 1\n핵심 항목 2")
            ]
        )
        let pdfToHWPX = try DocumentConversionOrchestrator.convert(
            ConversionJobRequest(
                sourceFileName: "원본.pdf",
                sourceFormat: .pdf,
                targetFormat: .hwpx,
                payload: .pdf(pdfSource)
            )
        )
        XCTAssertEqual(pdfToHWPX.verifiedSignature.format, .hwpx)
        let decodedHWPX = try HWPXContainerCodec.decodeDocument(from: pdfToHWPX.outputBytes)
        XCTAssertEqual(decodedHWPX.paragraphs.count, 3)

        let pdfToPPTX = try DocumentConversionOrchestrator.convert(
            ConversionJobRequest(
                sourceFileName: "원본.pdf",
                sourceFormat: .pdf,
                targetFormat: .pptx,
                payload: .pdf(pdfSource)
            )
        )
        XCTAssertEqual(pdfToPPTX.verifiedSignature.format, .pptx)
        let decodedPPTX = try OOXMLContainerCodec.decodePresentation(from: pdfToPPTX.outputBytes)
        XCTAssertEqual(decodedPPTX.slides.count, 1)
        XCTAssertEqual(decodedPPTX.slides[0].title, "슬라이드 제목")
        XCTAssertEqual(decodedPPTX.slides[0].bulletPoints, ["핵심 항목 1", "핵심 항목 2"])
    }

    func testBatchConversionReportCapturesSuccessesAndFailuresDeterministically() {
        let validDoc = OfficeDocumentModel(
            title: "정상 문서",
            paragraphs: [ParagraphBlock(text: "본문 데이터")]
        )
        let emptySheet = WorksheetModel(name: "빈 시트")
        let failedJobID = UUID()

        let jobs: [ConversionJobRequest] = [
            ConversionJobRequest(
                sourceFileName: "문서1.docx",
                sourceFormat: .docx,
                targetFormat: .hwpx,
                payload: .document(validDoc)
            ),
            ConversionJobRequest(
                sourceFileName: "문서2.docx",
                sourceFormat: .docx,
                targetFormat: .pdf,
                payload: .document(validDoc)
            ),
            ConversionJobRequest(
                id: failedJobID,
                sourceFileName: "빈시트.xlsx",
                sourceFormat: .xlsx,
                targetFormat: .csv,
                payload: .sheet(emptySheet)
            )
        ]

        let report = DocumentConversionOrchestrator.executeBatch(jobs)
        XCTAssertEqual(report.totalJobs, 3)
        XCTAssertEqual(report.succeededResults.count, 2)
        XCTAssertEqual(report.failedErrors.count, 1)
        XCTAssertEqual(report.failedErrors.first?.jobID, failedJobID)
        XCTAssertFalse(report.failedErrors.first?.errorDescription.isEmpty ?? true)
    }

    // MARK: - 2. 100% Offline Local Smart Document Assistant

    func testLocalSmartDocumentAssistantSummarizeAndKeywords() {
        let doc = OfficeDocumentModel(
            title: "차세대 보안 아키텍처 백서",
            paragraphs: [
                ParagraphBlock(style: .title, text: "차세대 보안 아키텍처 및 무결성 검증 백서", isBold: true),
                ParagraphBlock(style: .body, text: "모든 오피스 문서는 컨테이너 매직 시그니처 검증과 압축 폭탄 방어 정책을 통과해야 합니다."),
                ParagraphBlock(style: .body, text: "일반적인 안내 문구와 배경 설명이 이곳에 포함됩니다."),
                ParagraphBlock(style: .body, text: "보안 아키텍처는 오프라인 환경에서 외부 네트워크 연결 없이 무결성 검증과 암호화 감지를 수행합니다."),
                ParagraphBlock(style: .body, text: "부수적인 참고 사항입니다.")
            ],
            tables: [
                DocumentTableBlock(caption: "보안 무결성 검증 지표", rows: [["구분", "결과"], ["시그니처", "100%"]])
            ]
        )

        let summary = LocalSmartDocumentAssistant.summarize(doc, maxSentences: 2)
        XCTAssertEqual(summary.keySentences.count, 2)
        XCTAssertFalse(summary.keywords.isEmpty)
        XCTAssertTrue(summary.keywords.contains(where: { $0.contains("보안") || $0.contains("검증") || $0.contains("무결성") }))
        XCTAssertGreaterThan(summary.estimatedReadingSeconds, 0)
        XCTAssertGreaterThan(summary.compressionRatioPercent, 0.0)
        XCTAssertLessThanOrEqual(summary.compressionRatioPercent, 100.0)
    }

    func testLocalSmartDocumentAssistantWritingQualityDiagnostics() {
        let pLong = ParagraphBlock(
            text: "본 프로젝트는 문서 엔진의 파서 무결성을 검증하며, 동시에 스프레드시트 수식 평가기의 연산 속도를 개선하고, 추가적으로 프레젠테이션 캔버스의 정렬 가이드를 정밀하게 보정하여 최종 사용자의 업무 생산성을 극대화하는 것을 목표로 합니다."
        )
        let pPassive = ParagraphBlock(
            text: "해당 문서는 보안 모듈에 의해 자동으로 검증되어지고 결과 보고서가 즉시 작성되어집니다."
        )
        let pRedundant = ParagraphBlock(
            text: "이번 안건은  미리 사전 검토를 거쳐 매우 매우 신중하게 다시 재검토되었습니다."
        )

        let doc = OfficeDocumentModel(
            title: "문장 교정 테스트",
            paragraphs: [pLong, pPassive, pRedundant]
        )

        let suggestions = LocalSmartDocumentAssistant.analyzeWritingQuality(doc)
        let kinds = Set(suggestions.map(\.issueKind))
        XCTAssertTrue(kinds.contains("장문 분할 권장"))
        XCTAssertTrue(kinds.contains("수동태/피동 표현 완화"))
        XCTAssertTrue(kinds.contains("중복 공백/수식어 정리"))

        if let passiveFix = suggestions.first(where: { $0.issueKind == "수동태/피동 표현 완화" }) {
            XCTAssertEqual(passiveFix.paragraphID, pPassive.id)
            XCTAssertFalse(passiveFix.suggestedRewrite.contains("되어지"))
        }
        if let redundantFix = suggestions.first(where: { $0.issueKind == "중복 공백/수식어 정리" }) {
            XCTAssertEqual(redundantFix.paragraphID, pRedundant.id)
            XCTAssertFalse(redundantFix.suggestedRewrite.contains("  "))
            XCTAssertFalse(redundantFix.suggestedRewrite.contains("매우 매우"))
        }
    }

    func testLocalSmartDocumentAssistantExtractsActionItemsAndEntities() {
        let doc = OfficeDocumentModel(
            title: "주간 실행 계획",
            paragraphs: [
                ParagraphBlock(text: "1. 최종 릴리스 일정은 2026-11-15까지 완료해야 합니다."),
                ParagraphBlock(text: "2. 3분기 인프라 고도화 예산은 총 150,000,000원 규모로 편성되었습니다."),
                ParagraphBlock(text: "3. 렌더링 응답 속도를 98.5% 수준으로 개선하고 메모리 사용량을 45MB 이하로 유지합니다."),
                ParagraphBlock(text: "4. [조치 항목] 보안 감사 결과 보고서 제출 바람 및 담당자 확인 필요.")
            ]
        )

        let items = LocalSmartDocumentAssistant.extractActionItemsAndEntities(doc)
        let categories = Set(items.map(\.category))
        XCTAssertTrue(categories.contains("일정/기한"))
        XCTAssertTrue(categories.contains("예산/금액"))
        XCTAssertTrue(categories.contains("핵심 수치"))
        XCTAssertTrue(categories.contains("조치 항목"))
    }

    func testLocalSmartDocumentAssistantWorksheetInsightsZScoreAndTrend() throws {
        var sheet = WorksheetModel(name: "이상치 및 추세 분석")
        let values = [100.0, 102.0, 99.0, 101.0, 100.0, 98.0, 101.0, 103.0, 500.0, 140.0]
        for (idx, val) in values.enumerated() {
            try sheet.setCell("A\(idx + 1)", rawValue: String(val))
        }

        let range = try CellRange(notation: "A1:A10")
        let insight = LocalSmartDocumentAssistant.analyzeWorksheetInsights(sheet, range: range)

        XCTAssertEqual(insight.numericCellCount, 10)
        XCTAssertEqual(insight.minValue, 98.0, accuracy: 0.001)
        XCTAssertEqual(insight.maxValue, 500.0, accuracy: 0.001)
        XCTAssertEqual(insight.mean, 144.4, accuracy: 0.001)
        XCTAssertGreaterThan(insight.standardDeviation, 100.0)
        XCTAssertTrue(insight.outlierAddresses.contains("A9"))
        XCTAssertEqual(insight.trendClassification, "상승 추세")
        XCTAssertEqual(insight.growthRatePercent, 40.0, accuracy: 0.001)
    }

    func testLocalSmartDocumentAssistantLCSDocumentDiff() {
        let baseline = OfficeDocumentModel(
            title: "계약서 v1",
            paragraphs: [
                ParagraphBlock(text: "제1조 (목적) 본 계약은 양사의 협력 관계를 규정한다."),
                ParagraphBlock(text: "제2조 (기간) 계약 기간은 1년으로 한다."),
                ParagraphBlock(text: "제3조 (보안) 기밀 정보를 외부에 유출하지 않는다.")
            ]
        )
        let revised = OfficeDocumentModel(
            title: "계약서 v2",
            paragraphs: [
                ParagraphBlock(text: "제1조 (목적) 본 계약은 양사의 협력 관계를 규정한다."),
                ParagraphBlock(text: "제2조 (기간) 계약 기간은 2년으로 연장한다."),
                ParagraphBlock(text: "제3조 (보안) 기밀 정보를 외부에 유출하지 않는다."),
                ParagraphBlock(text: "제4조 (관할) 분쟁 발생 시 서울중앙지방법원을 관할로 한다.")
            ]
        )

        let diff = LocalSmartDocumentAssistant.compareDocuments(baseline: baseline, revised: revised)
        XCTAssertEqual(diff.unchangedCount, 2)
        XCTAssertEqual(diff.modifiedCount, 1)
        XCTAssertEqual(diff.addedCount, 1)
        XCTAssertEqual(diff.removedCount, 0)
        XCTAssertEqual(diff.entries.count, 4)

        XCTAssertEqual(diff.entries[0].kind, .unchanged)
        XCTAssertEqual(diff.entries[1].kind, .modified)
        XCTAssertEqual(diff.entries[1].baselineText, "제2조 (기간) 계약 기간은 1년으로 한다.")
        XCTAssertEqual(diff.entries[1].revisedText, "제2조 (기간) 계약 기간은 2년으로 연장한다.")
        XCTAssertEqual(diff.entries[2].kind, .unchanged)
        XCTAssertEqual(diff.entries[3].kind, .added)
        XCTAssertEqual(diff.entries[3].revisedText, "제4조 (관할) 분쟁 발생 시 서울중앙지방법원을 관할로 한다.")
    }

    // MARK: - 3. Container Security & Privacy Auditor

    func testDocumentSecurityAuditorScrubsMetadataAndPII() {
        let p = ParagraphBlock(text: "본문 문단")
        let doc = OfficeDocumentModel(
            title: "외부 배포용 보고서",
            headerText: "작성자: 홍길동 (내선 1234)",
            footerText: "대외비 - 인사팀 내부용",
            paragraphs: [p],
            revisions: [
                TrackedRevision(
                    author: "김철수",
                    kind: .insertion,
                    paragraphID: p.id,
                    originalText: "초안",
                    proposedText: "본문 문단"
                )
            ],
            comments: [
                DocumentCommentThread(
                    anchorParagraphID: p.id,
                    author: "박영희",
                    message: "출시 전 단가표 삭제 요망",
                    replies: ["확인했습니다"]
                )
            ]
        )

        let (scrubbed, removedCount) = DocumentSecurityAuditor.scrubMetadata(from: doc)
        XCTAssertEqual(removedCount, 4) // header + footer + 1 revision author anonymized + 1 comment removed
        XCTAssertEqual(scrubbed.headerText, "")
        XCTAssertEqual(scrubbed.footerText, "")
        XCTAssertTrue(scrubbed.comments.isEmpty)
        XCTAssertEqual(scrubbed.revisions.count, 1)
        XCTAssertEqual(scrubbed.revisions[0].author, "Redacted")
        XCTAssertEqual(scrubbed.paragraphs[0].text, "본문 문단")
    }

    func testDocumentSecurityAuditorDetectsValidAndMaliciousContainers() throws {
        // 1. Valid DOCX container
        let cleanDoc = OfficeDocumentModel(
            title: "안전한 문서",
            paragraphs: [ParagraphBlock(text: "정상 콘텐츠")]
        )
        let cleanBytes = try OOXMLContainerCodec.encodeDocument(cleanDoc)
        let cleanAudit = DocumentSecurityAuditor.auditContainer(data: cleanBytes, fileName: "clean.docx")
        XCTAssertTrue(cleanAudit.isPassed)
        XCTAssertEqual(cleanAudit.detectedFormat, .docx)
        XCTAssertTrue(cleanAudit.isSignatureValid)
        XCTAssertTrue(cleanAudit.isSafeFromBombAndTraversal)
        XCTAssertTrue(cleanAudit.isSafeFromXXE)
        XCTAssertTrue(cleanAudit.isUnencrypted)
        XCTAssertTrue(cleanAudit.issues.isEmpty)

        // 2. XXE payload inside ZIP-XML container
        let xxeXML = """
        <?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE foo [ <!ENTITY xxe SYSTEM "file:///etc/passwd"> ]>
        <w:document>&xxe;</w:document>
        """
        let xxeZip = try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "word/document.xml", data: Data(xxeXML.utf8), compressionMethod: .store)
        ])
        let xxeAudit = DocumentSecurityAuditor.auditContainer(data: xxeZip, fileName: "malicious_xxe.docx")
        XCTAssertFalse(xxeAudit.isPassed)
        XCTAssertTrue(xxeAudit.isSignatureValid)
        XCTAssertFalse(xxeAudit.isSafeFromXXE)
        XCTAssertFalse(xxeAudit.issues.isEmpty)

        // 3. Encrypted HWP 5.0 OLE2 container
        var encryptedHeader = Data(repeating: 0, count: HWP5BinaryCodec.fileHeaderByteLength)
        for (i, b) in Array(HWP5BinaryCodec.signatureString.utf8).enumerated() {
            encryptedHeader[i] = b
        }
        try BitPacking.writeUInt32LE(HWP5BinaryCodec.defaultVersion, into: &encryptedHeader, at: 32)
        try BitPacking.writeUInt32LE(HWP5BinaryCodec.flagEncrypted, into: &encryptedHeader, at: 36)
        let encryptedHWP = try OLE2ContainerCodec.pack(streams: [
            OLE2StreamEntry(name: "FileHeader", data: encryptedHeader),
            OLE2StreamEntry(name: "DocInfo", data: Data()),
            OLE2StreamEntry(name: "BodyText/Section0", data: Data())
        ])
        let encAudit = DocumentSecurityAuditor.auditContainer(data: encryptedHWP, fileName: "secret.hwp")
        XCTAssertFalse(encAudit.isPassed)
        XCTAssertTrue(encAudit.isSignatureValid)
        XCTAssertFalse(encAudit.isUnencrypted)
        XCTAssertFalse(encAudit.issues.isEmpty)

        // 4. Path traversal payload inside ZIP container ("aa/evil.xml" -> "../evil.xml")
        var traversalZip = try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "aa/evil.xml", data: Data("<doc/>".utf8), compressionMethod: .store)
        ])
        let targetBytes = Data("aa/evil.xml".utf8)
        let replacementBytes = Data("../evil.xml".utf8)
        while let range = traversalZip.range(of: targetBytes) {
            traversalZip.replaceSubrange(range, with: replacementBytes)
        }
        let traversalAudit = DocumentSecurityAuditor.auditContainer(data: traversalZip, fileName: "traversal.docx")
        XCTAssertFalse(traversalAudit.isPassed)
        XCTAssertTrue(traversalAudit.isSignatureValid)
        XCTAssertFalse(traversalAudit.isSafeFromBombAndTraversal)
        XCTAssertFalse(traversalAudit.issues.isEmpty)

        // 5. Spoofed signature container
        let spoofAudit = DocumentSecurityAuditor.auditContainer(
            data: Data("Plain text pretending to be HWPX".utf8),
            fileName: "spoofed.hwpx"
        )
        XCTAssertFalse(spoofAudit.isPassed)
        XCTAssertFalse(spoofAudit.isSignatureValid)

        // 6. Non-UTF8 XML inside ZIP container and OLE2 EncryptionInfo stream (F3)
        let nonUTF8Zip = try ZipContainerCodec.pack(entries: [
            ZipContainerEntry(path: "word/document.xml", data: Data([0xFF, 0xFE, 0x3C]), compressionMethod: .store)
        ])
        let nonUTF8Audit = DocumentSecurityAuditor.auditContainer(data: nonUTF8Zip, fileName: "nonutf8.docx")
        XCTAssertFalse(nonUTF8Audit.isPassed)
        XCTAssertFalse(nonUTF8Audit.isSafeFromBombAndTraversal)

        let encryptedOLE2 = try OLE2ContainerCodec.pack(streams: [
            OLE2StreamEntry(name: "EncryptionInfo", data: Data([0x04, 0x00, 0x04, 0x00])),
            OLE2StreamEntry(name: "EncryptedPackage", data: Data(repeating: 0xAA, count: 32))
        ])
        let ole2EncAudit = DocumentSecurityAuditor.auditContainer(data: encryptedOLE2, fileName: "locked.doc")
        XCTAssertFalse(ole2EncAudit.isPassed)
        XCTAssertFalse(ole2EncAudit.isUnencrypted)

        let missingHeaderHWP = try OLE2ContainerCodec.pack(streams: [
            OLE2StreamEntry(name: "BodyText/Section0", data: Data([0x01, 0x02]))
        ])
        let missingHeaderAudit = DocumentSecurityAuditor.auditContainer(data: missingHeaderHWP, fileName: "broken.hwp")
        XCTAssertFalse(missingHeaderAudit.isPassed)
        XCTAssertFalse(missingHeaderAudit.isSafeFromBombAndTraversal)
    }

    func testPDFFallbackParserEscapeOrderEncryptedAndPathTraversalRegression() throws {
        // F1: Standard PDF without studio metadata stream + literal backslash-n ("C:\\new\nLine2")
        let rawMinimalPDF = """
        %PDF-1.7
        1 0 obj
        << /Type /Catalog /Pages 2 0 R >>
        endobj
        2 0 obj
        << /Type /Pages /Count 1 /Kids [3 0 R] >>
        endobj
        3 0 obj
        << /Type /Page /Parent 2 0 R /Rotate 90 >>
        endobj
        4 0 obj
        << /Length 44 >>
        stream
        BT /F1 11 Tf 56 780 Td (C:\\\\new\\nLine2) Tj ET
        endstream
        endobj
        %%EOF
        """
        let decodedFallback = try PDFBinarySerializer.decode(from: Data(rawMinimalPDF.utf8))
        XCTAssertEqual(decodedFallback.pages.count, 1)
        XCTAssertEqual(decodedFallback.pages[0].rotationDegrees, 90)
        XCTAssertEqual(decodedFallback.pages[0].extractedText, "C:\\new\nLine2")

        // F1: Encrypted PDF must throw encryptedDocumentUnsupported(.pdf)
        let encryptedPDF = """
        %PDF-1.7
        1 0 obj
        << /Type /Catalog /Encrypt 5 0 R >>
        endobj
        %%EOF
        """
        XCTAssertThrowsError(try PDFBinarySerializer.decode(from: Data(encryptedPDF.utf8))) { err in
            XCTAssertEqual(err as? EasyOfficeError, .encryptedDocumentUnsupported(format: .pdf))
        }

        // F1: Pageless PDF must throw corruptedContainer
        let pagelessPDF = """
        %PDF-1.7
        1 0 obj
        << /Type /Catalog >>
        endobj
        %%EOF
        """
        XCTAssertThrowsError(try PDFBinarySerializer.decode(from: Data(pagelessPDF.utf8)))

        // F2: Path traversal in DocumentConversionOrchestrator.convert must throw securityViolation
        let doc = OfficeDocumentModel(title: "Test", paragraphs: [ParagraphBlock(text: "Body")])
        let traversalReq = ConversionJobRequest(
            sourceFileName: "../../etc/passwd.docx",
            sourceFormat: .docx,
            targetFormat: .pdf,
            payload: .document(doc)
        )
        XCTAssertThrowsError(try DocumentConversionOrchestrator.convert(traversalReq))

        // F4: Two-character Korean particle "으로" must normalize to the same stem as "은"
        let stemDoc = OfficeDocumentModel(
            title: "아키텍처 보고서",
            paragraphs: [
                ParagraphBlock(text: "차세대 엔진 기반으로 고속 문서 처리를 달성합니다. 아키텍처 기반은 안전합니다.")
            ]
        )
        let summary = LocalSmartDocumentAssistant.summarize(stemDoc, maxSentences: 1)
        XCTAssertTrue(summary.keywords.contains("기반"))
        XCTAssertFalse(summary.keywords.contains("기반으"))
    }

    func testPDFBinarySerializerGeneratesByteExactXrefAndLoadsInNativePDFKit() throws {
        let model = PDFStudioDocumentModel(
            title: "ISO 32000-1 XREF & PDFKit 검증",
            pages: [
                PDFPageDescriptor(pageNumber: 1, rotationDegrees: 0, extractedText: "Page One Content"),
                PDFPageDescriptor(pageNumber: 2, rotationDegrees: 90, extractedText: "Page Two Rotated"),
                PDFPageDescriptor(pageNumber: 3, rotationDegrees: 180, extractedText: "Page Three Summary")
            ],
            annotations: [
                PDFAnnotationItem(pageNumber: 1, kind: .highlight, content: "하이라이트 주석", author: "QA")
            ],
            outline: [
                PDFOutlineNode(title: "1장", targetPageNumber: 1),
                PDFOutlineNode(title: "2장", targetPageNumber: 2)
            ]
        )

        let encoded = PDFBinarySerializer.encode(model)
        let rawText = String(decoding: encoded, as: UTF8.self)

        // Verify startxref points to the exact byte offset of "xref\n"
        guard let xrefRange = encoded.range(of: Data("xref\n".utf8)) else {
            XCTFail("Missing xref table in encoded PDF")
            return
        }
        XCTAssertTrue(
            rawText.contains("startxref\n\(xrefRange.lowerBound)\n%%EOF"),
            "startxref must record exact byte offset \(xrefRange.lowerBound)"
        )

        // Verify Object 1 offset in xref matches actual byte offset of "1 0 obj\n"
        if let obj1Range = encoded.range(of: Data("1 0 obj\n".utf8)) {
            let expectedEntry = String(format: "%010d 00000 n ", obj1Range.lowerBound)
            XCTAssertTrue(rawText.contains(expectedEntry), "xref must contain exact offset entry \(expectedEntry)")
        } else {
            XCTFail("Missing 1 0 obj in encoded PDF")
        }

        // Verify macOS native PDFKit parses the encoded bytes and reports exact page count
        let nativePDF = PDFKit.PDFDocument(data: encoded)
        XCTAssertNotNil(nativePDF)
        XCTAssertEqual(nativePDF?.pageCount, 3)

        // Verify long sentence without commas splits on a whitespace word boundary, never slicing a word in half
        let wordBoundarySentence = "차세대 네이티브 오피스 워크스페이스는 단일 윈도우 환경에서 다중 포맷 문서를 신속하게 처리하고 높은 보안 무결성과 일관된 사용자 경험을 동시에 보장합니다"
        let doc = OfficeDocumentModel(
            title: "장문 공백 경계 분할 검증",
            paragraphs: [ParagraphBlock(text: wordBoundarySentence)]
        )
        let suggestions = LocalSmartDocumentAssistant.analyzeWritingQuality(doc)
        let splitSuggestion = try XCTUnwrap(suggestions.first(where: { $0.issueKind == "장문 분할 권장" }))
        XCTAssertTrue(splitSuggestion.suggestedRewrite.contains(". "))
        // Every word from original sentence must remain intact (ignoring trailing period added at split point)
        let originalWords = Set(wordBoundarySentence.split(separator: " ").map(String.init))
        let rewrittenWords = Set(
            splitSuggestion.suggestedRewrite
                .replacingOccurrences(of: ". ", with: " ")
                .split(separator: " ")
                .map(String.init)
        )
        XCTAssertEqual(originalWords, rewrittenWords)
    }
}

