(function () {
  "use strict";

  const isWindows = navigator.userAgent.includes("Windows");
  const primaryModifierLabel = isWindows ? "Ctrl" : "Cmd";

  const STANDARD_RIBBON_TITLES = [
    "홈",
    "삽입",
    "레이아웃",
    "수식·데이터",
    "모듈 전용",
    "검토·보안",
    "보기·창",
  ];

  const HANGUL_HWP_RIBBON_TITLES = [
    "편집",
    "입력",
    "쪽·원고지",
    "표·개체",
    "시트·차트",
    "보안·도구",
    "보기",
  ];

  const state = {
    activeModule: "document",
    activeRibbonCategory: "home",
    ribbonLayoutStyle: "standard",
    activeRecentFilter: "전체",
    isComposingHangul: false,
    compositionBuffer: "",
    manuscriptGridEnabled: false,
    approvalBannerVisible: true,
    theme: "light",
  };

  function updatePlatformShortcuts() {
    const hint = document.getElementById("shortcut-hint");
    if (hint) {
      hint.textContent = primaryModifierLabel + "+F";
    }
    const platformBadge = document.getElementById("platform-badge");
    if (platformBadge) {
      platformBadge.textContent = isWindows
        ? "Windows (WebView2 · Ctrl 단축키)"
        : "macOS (WKWebView · Cmd 단축키)";
    }
  }

  function setupHangulImeIntegrityHandler() {
    const surface = document.getElementById("document-surface");
    const imePill = document.getElementById("ime-status-pill");
    if (!surface || !imePill) return;

    surface.addEventListener("compositionstart", () => {
      state.isComposingHangul = true;
      imePill.textContent = "한글 IME 조합 중";
    });

    surface.addEventListener("compositionupdate", (event) => {
      state.compositionBuffer = event.data || "";
      imePill.textContent = "한글 조합: " + state.compositionBuffer;
    });

    surface.addEventListener("compositionend", (event) => {
      state.isComposingHangul = false;
      state.compositionBuffer = event.data || "";
      imePill.textContent = "IME 대기 (UTF-8)";
    });
  }

  function renderManuscriptGrid200() {
    const container = document.getElementById("manuscript-grid-container");
    if (!container) return;
    container.innerHTML = "";
    const sample = "EasyOffice한글200자원고지조판검증KSX6101표준공문서";
    for (let i = 0; i < 200; i += 1) {
      const cell = document.createElement("div");
      cell.className = "manuscript-cell";
      cell.textContent = i < sample.length ? sample[i] : "";
      container.appendChild(cell);
    }
  }

  function syncRibbonTitlesAndContextualBadge() {
    const titles =
      state.ribbonLayoutStyle === "hangul"
        ? HANGUL_HWP_RIBBON_TITLES
        : STANDARD_RIBBON_TITLES;
    const catButtons = document.querySelectorAll(".ribbon-cat");
    catButtons.forEach((btn, idx) => {
      if (titles[idx]) {
        btn.textContent = titles[idx];
      }
    });

    const styleBtn = document.getElementById("btn-ribbon-style-toggle");
    if (styleBtn) {
      styleBtn.textContent =
        state.ribbonLayoutStyle === "hangul"
          ? "한글 쪽·편집 리본"
          : "표준 리본";
    }

    const badgeEl = document.getElementById("contextual-object-badge");
    const badgeMap = {
      document: "표·개체 도구",
      hwp: "표·개체 도구",
      sheet: "수식·피벗 분석",
      slide: "발표자·레이저 도구",
      pdf: "전자결재·주석 도구",
      converter: "20종 포맷 일괄 변환",
      smart: "오프라인 스마트·PII 도구",
    };
    if (badgeEl) {
      badgeEl.textContent = badgeMap[state.activeModule] || "표·개체 도구";
    }

    const inspectorMode = document.getElementById("inspector-ribbon-mode");
    if (inspectorMode) {
      inspectorMode.textContent =
        "A4 (595 × 842 pt) · " +
        (state.ribbonLayoutStyle === "hangul" ? "한글(HWP) 쪽·편집" : "표준 오피스");
    }
  }

  function switchActivePane(mod) {
    const panes = {
      document: "pane-document",
      hwp: "pane-document",
      sheet: "pane-sheet",
      slide: "pane-slide",
      pdf: "pane-pdf",
      converter: "pane-smart",
      smart: "pane-smart",
    };
    const targetId = panes[mod] || "pane-document";
    document.querySelectorAll(".module-pane").forEach((el) => {
      if (el.id === targetId) {
        el.classList.remove("hidden");
      } else {
        el.classList.add("hidden");
      }
    });
  }

  function setupWorkspaceInteractions() {
    const tabs = document.querySelectorAll(".doc-tab");
    const moduleTitle = document.getElementById("active-module-title");
    const specBadge = document.getElementById("format-spec-badge");
    const cmdLog = document.getElementById("command-log-badge");

    const moduleDescriptions = {
      document: {
        title: "문서 편집기 (OOXML / ODF / CommonMark)",
        spec: "OOXML (ECMA-376) · ODF (ISO/IEC 26300)",
      },
      hwp: {
        title: "한글 공문서 편집기 (HWP 5.0 / HWPX · 200자 원고지)",
        spec: "HWP 5.0 Binary (OLE2 CFBF) · HWPX (KS X 6101 OWPML)",
      },
      sheet: {
        title: "시트 스프레드시트 (122종 수식 · 피벗 · 조건부 서식)",
        spec: "OOXML (ECMA-376) SpreadsheetML · RFC 4180 CSV",
      },
      slide: {
        title: "슬라이드 프레젠테이션 (16:9 와이드 스테이지 · 발표자 도구)",
        spec: "OOXML (ECMA-376) PresentationML · ODF (ISO/IEC 26300)",
      },
      pdf: {
        title: "PDF 스튜디오 (주석 · 전자결재 스탬프 · 워터마크)",
        spec: "PDF (ISO 32000)",
      },
      converter: {
        title: "20종 포맷 일괄 변환 오케스트레이터",
        spec: "OOXML · ODF · HWP/HWPX · PDF · CSV · Markdown",
      },
      smart: {
        title: "오프라인 스마트 문서 보조 및 PII 개인정보 보호 감사",
        spec: "100% On-Device TF 요약 · LCS 비교 · PII 마스킹",
      },
    };

    tabs.forEach((tab) => {
      tab.addEventListener("click", () => {
        tabs.forEach((t) => t.classList.remove("active"));
        tab.classList.add("active");
        const mod = tab.getAttribute("data-module") || "document";
        state.activeModule = mod;
        const meta = moduleDescriptions[mod] || moduleDescriptions.document;
        if (moduleTitle) moduleTitle.textContent = meta.title;
        if (specBadge) specBadge.textContent = meta.spec;
        switchActivePane(mod);
        syncRibbonTitlesAndContextualBadge();
      });
    });

    const styleToggleBtn = document.getElementById("btn-ribbon-style-toggle");
    if (styleToggleBtn) {
      styleToggleBtn.addEventListener("click", () => {
        state.ribbonLayoutStyle =
          state.ribbonLayoutStyle === "standard" ? "hangul" : "standard";
        syncRibbonTitlesAndContextualBadge();
      });
    }

    document.querySelectorAll(".cmd-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        const cmd = btn.getAttribute("data-cmd") || "";
        if (cmd === "hwp-mod-manuscript") {
          state.manuscriptGridEnabled = !state.manuscriptGridEnabled;
          const grid = document.getElementById("manuscript-grid-container");
          if (grid) {
            grid.classList.toggle("hidden", !state.manuscriptGridEnabled);
          }
        } else if (cmd === "hwp-mod-header") {
          state.approvalBannerVisible = !state.approvalBannerVisible;
          const banner = document.getElementById("ksx6101-approval-banner");
          if (banner) {
            banner.classList.toggle("hidden", !state.approvalBannerVisible);
          }
        }
        if (cmdLog) {
          cmdLog.textContent = "실행 완료: " + cmd + " (140종 카탈로그)";
        }
      });
    });

    const evalBtn = document.getElementById("btn-eval-formula");
    if (evalBtn) {
      evalBtn.addEventListener("click", () => {
        const input = document.getElementById("sheet-formula-input");
        const out = document.getElementById("sheet-eval-result");
        if (!input || !out) return;
        const raw = input.value.trim().toUpperCase();
        if (raw === "=200*15%") {
          out.textContent = "결과: 30";
        } else if (raw.startsWith("=VLOOKUP")) {
          out.textContent = "결과: 200";
        } else {
          out.textContent = "결과: 1300";
        }
      });
    }

    const slideStage = document.getElementById("slide-stage");
    const laserDot = document.getElementById("laser-dot");
    if (slideStage && laserDot) {
      slideStage.addEventListener("mousemove", (ev) => {
        const rect = slideStage.getBoundingClientRect();
        laserDot.classList.remove("hidden");
        laserDot.style.left = Math.max(0, ev.clientX - rect.left - 6) + "px";
        laserDot.style.top = Math.max(0, ev.clientY - rect.top - 6) + "px";
      });
    }

    const piiBtn = document.getElementById("btn-run-pii-scrub");
    if (piiBtn) {
      piiBtn.addEventListener("click", () => {
        const input = document.getElementById("smart-input-text");
        const out = document.getElementById("smart-output-box");
        if (!input || !out) return;
        const scrubbed = input.value
          .replace(/\b\d{6}-\d{7}\b/g, "******-*******")
          .replace(/\b010-\d{4}-\d{4}\b/g, "010-****-****")
          .replace(/[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/g, "***@***");
        out.textContent = "[PII 마스킹 완료]\n" + scrubbed;
      });
    }

    const summaryBtn = document.getElementById("btn-run-summary");
    if (summaryBtn) {
      summaryBtn.addEventListener("click", () => {
        const out = document.getElementById("smart-output-box");
        if (out) {
          out.textContent =
            "[TF 핵심 요약] 보안 결재 및 개인정보 비식별화 검토\n[추출된 액션 아이템] TODO: 10월 30일 기한까지 보안 결재 완료.";
        }
      });
    }

    const themeBtn = document.getElementById("btn-theme-toggle");
    if (themeBtn) {
      themeBtn.addEventListener("click", () => {
        state.theme = state.theme === "light" ? "dark" : "light";
        document.body.setAttribute("data-theme", state.theme);
        themeBtn.textContent = state.theme === "light" ? "다크 모드" : "라이트 모드";
      });
    }

    window.addEventListener("keydown", (event) => {
      if (state.isComposingHangul) return;
      const primaryPressed = isWindows ? event.ctrlKey : event.metaKey;
      if (primaryPressed && event.key.toLowerCase() === "f") {
        event.preventDefault();
        const searchInput = document.getElementById("global-search-input");
        if (searchInput) searchInput.focus();
      }
    });
  }

  document.addEventListener("DOMContentLoaded", () => {
    updatePlatformShortcuts();
    setupHangulImeIntegrityHandler();
    renderManuscriptGrid200();
    syncRibbonTitlesAndContextualBadge();
    setupWorkspaceInteractions();
  });
})();
