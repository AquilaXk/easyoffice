# EasyOffice 시스템 아키텍처 (System Architecture)

이 문서는 `EasyOffice` (`v1.0.0`)의 모듈 계층 구조, 비트 수준 컨테이너 코덱, 데이터 흐름, Fail-Closed 보안 경계, 결정론적 테스트 및 macOS 릴리스 패키징 아키텍처를 설명합니다.

---

## 1. 모듈 계층 구조 (Swift Package Architecture)

```mermaid
flowchart TD
    subgraph AppLayer["Presentation & macOS Shell Layer"]
        EasyOfficeApp["EasyOfficeApp (Executable)\n- MainWorkspaceWindowView & HomeHubWorkspaceView\n- Multi-Tab Strip & Side-by-Side Split View\n- 7-Category Context Ribbon & Right Inspector\n- Interactive EditorDetailPanes (Document / HWP / Sheet / Slide / PDF)"]
    end

    subgraph DesignLayer["Design System Layer"]
        EasyOfficeDesignSystem["EasyOfficeDesignSystem (Library)\n- Signature Lavender Color Tokens (#5C6BC0)\n- 5 Module Semantic Accents & Status Tokens\n- WorkspaceThemeMode & WorkspaceThemePalette\n- AccessibilityTokens & LayoutMetrics"]
    end

    subgraph CoreLayer["Domain, Codec & Engine Layer (Pure Deterministic Swift)"]
        EasyOfficeCore["EasyOfficeCore (Library)\n- FormatDetector & DocumentFormat (20-Format Registry & Magic-Byte Guard)\n- BinaryContainerCodec (BitPacking, CRC32, ZIP/OOXML/ODF/HWPX/OLE2/HWP5)\n- TabularAndMarkupCodec (CSVStateMachineCodec, CommonMarkCodec, RTFCodec)\n- DocumentEngine (Sections, Styles, Spanned Tables, Revisions, Mail Merge)\n- SheetEngine (PackedCellCoordinate, 77 Functions, PivotTableEngine, Charts)\n- SlideEngine (Master Layouts, SnapGuideCalculator, PresenterSession)\n- PDFEngine & PDFBinarySerializer (Annotations, Split/Merge, Office ↔ PDF)\n- SmartProductivityTools (Batch Conversion, Offline Smart Assistant, Security Auditor)\n- WorkspaceHub (WorkspaceViewModel, 7x5 Ribbon Catalog, Templates, Search)"]
    end

    EasyOfficeApp --> EasyOfficeDesignSystem
    EasyOfficeApp --> EasyOfficeCore
    EasyOfficeCore --> EasyOfficeDesignSystem
```

### 디렉터리 구조 (Directory Layout)

```text
EasyOffice/
├── Package.swift                                  # Swift 6 패키지 매니페스트 (macOS 14+)
├── Sources/
│   ├── EasyOfficeDesignSystem/                    # 시그니처 라벤더 디자인 시스템 및 레이아웃 토큰
│   │   └── DesignTokens.swift                     # RGBColorToken, WorkspaceThemeMode, AccessibilityTokens
│   ├── EasyOfficeCore/                            # 순수 결정론적 오피스 엔진, 코덱 및 스마트 생산성 코어
│   │   ├── DocumentFormat.swift                   # 20종 지원 포맷 레지스트리 및 ContainerFamily 메타데이터
│   │   ├── FormatDetector.swift                   # 매직 바이트 시그니처 검증 및 Fail-Closed 가드
│   │   ├── BinaryContainerCodec.swift             # BitPacking, CRC32Calculator, ZIP/OOXML/ODF/HWPX/OLE2/HWP5 코덱
│   │   ├── TabularAndMarkupCodec.swift            # CSVStateMachineCodec, CommonMarkCodec, RTFCodec
│   │   ├── DocumentEngine.swift                   # 워드/한글(HWP/HWPX) 문서 모델, 표 수식, 변경 추적, 메일 머지
│   │   ├── SheetEngine.swift                      # PackedCellCoordinate, 77종 수식 계산기, 피벗 및 차트 엔진
│   │   ├── SlideEngine.swift                      # 슬라이드/마스터, SnapGuideCalculator, PresenterSession 엔진
│   │   ├── PDFEngine.swift                        # PDF 주석, 북마크, 페이지 분할/병합, 양방향 변환 엔진
│   │   ├── SmartProductivityTools.swift           # PDFBinarySerializer, 일괄 변환, 오프라인 보조 도구, 보안 감사기
│   │   └── WorkspaceHub.swift                     # WorkspaceViewModel, 7대 리본 카탈로그, 템플릿, 전문 검색 엔진
│   └── EasyOfficeApp/                             # macOS 네이티브 SwiftUI + AppKit 애플리케이션
│       ├── EasyOfficeApp.swift                    # @main 앱 엔트리포인트 및 메뉴바 커맨드
│       ├── WorkspaceViews.swift                   # 통합 타이틀바, 멀티 탭, 스플릿 뷰, 리본, 홈 허브, 인스펙터
│       └── EditorDetailPanes.swift                # Document/HWP, Sheet, Slide, PDF Studio 인터랙티브 상세 패널
├── Tests/
│   └── EasyOfficeTests/                           # 결정론적 단위·보안·UI 패리티 및 벤치마크 검증 스위트
│       ├── EasyOfficeTests.swift                  # 디자인 토큰, 포맷 카탈로그 및 기본 엔진 검증
│       ├── BinaryContainerCodecTests.swift        # BitPacking, CRC32, ZIP/OLE2/HWP5/CSV/MD/RTF 및 보안 가드 검증
│       ├── EditorEnginesParityTests.swift         # 77종 시트 함수, 피벗/차트, 문서/HWP, 슬라이드, PDF 패리티 검증
│       ├── SmartProductivityToolsTests.swift      # 일괄 변환, 오프라인 스마트 보조 도구 및 보안 감사기 검증
│       ├── WorkspaceUIParityTests.swift           # 스플릿 뷰, 7x5 리본 매트릭스, WCAG AA 및 접근성 검증
│       └── CommercialParityBenchmarkTests.swift   # 20종 포맷 라운드트립 매트릭스 및 비트 수준 처리량 벤치마크
├── scripts/
│   ├── check-repo-hygiene.mjs                     # 외부 서비스명·AI 흔적·필수 문서 무결성 가드
│   └── package-macos-release.sh                   # EasyOffice.app / .dmg / .zip / SHA256SUMS 생성기
├── docs/
│   ├── policy-decisions.md                        # 정책 결정 기록 (SSOT)
│   ├── design-system.md                           # 시그니처 라벤더 UI/UX 디자인 시스템
│   ├── product-spec.md                            # 올인원 오피스 통합 기능 명세
│   └── architecture.md                            # 시스템 아키텍처 문서
└── .github/
    ├── release.yml                                # GitHub Release 노트 자동 분류 설정
    ├── PULL_REQUEST_TEMPLATE.md                   # 자기완결 PR 표준 템플릿
    ├── ISSUE_TEMPLATE/                            # Epic / Task / Bug 이슈 폼 템플릿
    └── workflows/
        ├── ci.yml                                 # 위생 가드 + macOS 빌드 + 테스트 게이트 (verify)
        ├── automerge.yml                          # automerge 라벨 기반 스쿼시 병합 및 이슈 자동 종료
        └── release.yml                            # 태그 기반 macOS .app/.dmg/.zip GitHub Release 발행
```

---

## 2. 보안 및 Fail-Closed 경계 (Security Boundaries)

1. **입구 시그니처 검증 (`FormatDetector`)**:
   - 모든 파일 입력은 확장자와 무관하게 바이트 헤더(`PK\x03\x04`, `\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1`, `%PDF-`, UTF-8 유효성)를 대조합니다.
   - 빈 파일(`emptyPayload`), 최대 허용 크기 초과(`payloadExceedsMaximumLimit`), 확장자-헤더 불일치(`signatureMismatch`)는 즉시 `EasyOfficeError`를 던져 차단합니다.
2. **ZIP/OLE2 압축 폭탄·경로 순회·XXE 원천 차단 (`ZipContainerCodec`, `OLE2ContainerCodec`, `XMLSanitizer`)**:
   - `ZipContainerSecurityPolicy`는 엔트리 수 상한(`4,096`), 단일 엔트리 해제 상한(`128 MiB`), 전체 해제 상한(`512 MiB`), 최대 압축 비율(`100:1`)을 강제하며, `..`·선행 `/`·`\`·NUL 바이트 경로를 `EasyOfficeError.securityViolation`으로, 압축 비율/용량 초과를 `EasyOfficeError.decompressionBombDetected`로 즉시 차단합니다.
   - `XMLSanitizer`는 컨테이너 내부 XML 파싱 전 `<!ENTITY` 선언 및 외부 DTD(`SYSTEM`/`PUBLIC`/내부 서브셋 `[`)를 검사하여 `EasyOfficeError.xxeOrExternalEntityDetected`로 차단합니다.
   - `OLE2ContainerCodec`과 `HWP5BinaryCodec`은 512바이트 섹터 FAT 체인 순환 루프를 방어하고, `FileHeader` 암호화 비트(`0x02`) 및 배포용 문서 비트(`0x04`) 감지 시 `EasyOfficeError.encryptedDocumentUnsupported`로 Fail-Closed 처리합니다.
3. **수식·범위 경계 및 스크립트 격리 (`SheetEngine`)**:
   - 스프레드시트 수식 평가기는 외부 셸 호출이나 네트워크 접근 권한이 없는 순수 메모리 내 평가기로 동작하며, `PackedCellCoordinate` 방문 집합과 재귀 상한(`maxFormulaRecursionDepth = 256`), 단일 범위 확장 상한(`maxExpandedCellCount = 100,000`)으로 무한 루프와 메모리 고갈을 차단합니다.
4. **메타데이터 비식별화 및 제로 외부 텔레메트리 (`DocumentSecurityAuditor`, `LocalSmartDocumentAssistant`)**:
   - 문서 요약, 문장 교정, 액션 아이템 추출, 시트 이상치 분석, 문서 비교(LCS), 일괄 변환, PII 비식별화(`scrubMetadata`)는 모두 100% 온디바이스 결정론적 알고리즘으로 수행되며 외부 네트워크 호출이나 텔레메트리 전송이 전혀 없습니다.

---

## 3. macOS 릴리스 패키징 파이프라인 (`scripts/package-macos-release.sh`)

GitHub Release 발행 시 `scripts/package-macos-release.sh`는 다음 단계를 결정론적으로 수행합니다:
1. `swift build -c release`로 최적화된 네이티브 실행 바이너리를 컴파일합니다.
2. 표준 macOS 애플리케이션 번들 구조(`EasyOffice.app/Contents/{MacOS,Resources,Info.plist,PkgInfo}`)를 생성하고 문서 확장자 연결(`CFBundleDocumentTypes`: `docx`, `xlsx`, `pptx`, `hwp`, `hwpx`, `pdf`, `odt`, `ods`, `odp`, `csv`, `md`, `txt`) 및 고해상도 Retina 속성(`NSHighResolutionCapable`)을 주입합니다.
3. 로컬 코드 서명(`codesign --force --deep --sign -` 또는 Developer ID 인증서) 및 번들 구조 검증을 수행합니다.
4. `hdiutil create`와 `ditto -c -k`를 사용하여 배포용 디스크 이미지(`EasyOffice-<version>-macos.dmg`), 압축 아카이브(`EasyOffice-<version>-macos.zip`), 그리고 `shasum -a 256` 기반의 `SHA256SUMS.txt`를 생성하여 GitHub Release 자산으로 업로드합니다.
