(function () {
  "use strict";

  const isWindows = navigator.userAgent.includes("Windows");
  const primaryModifierLabel = isWindows ? "Ctrl" : "Cmd";

  const state = {
    activeModule: "document",
    activeRibbonCategory: "home",
    activeRecentFilter: "전체",
    isComposingHangul: false,
    compositionBuffer: "",
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

  function setupWorkspaceInteractions() {
    const tabs = document.querySelectorAll(".doc-tab");
    const moduleTitle = document.getElementById("active-module-title");
    const specBadge = document.getElementById("format-spec-badge");

    const moduleDescriptions = {
      document: {
        title: "문서 편집기 (OOXML / ODF / CommonMark)",
        spec: "OOXML (ECMA-376) · ODF (ISO/IEC 26300)",
      },
      hwp: {
        title: "한글 공문서 편집기 (HWP 5.0 / HWPX)",
        spec: "HWP 5.0 Binary (OLE2 CFBF) · HWPX (KS X 6101 OWPML)",
      },
      sheet: {
        title: "시트 스프레드시트 (수식 · 피벗 · 조건부 서식)",
        spec: "OOXML (ECMA-376) SpreadsheetML · RFC 4180 CSV",
      },
      slide: {
        title: "슬라이드 프레젠테이션 (16:9 와이드 스테이지)",
        spec: "OOXML (ECMA-376) PresentationML · ODF (ISO/IEC 26300)",
      },
      pdf: {
        title: "PDF 스튜디오 (주석 · 전자결재 스탬프 · 워터마크)",
        spec: "PDF (ISO 32000)",
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
      });
    });

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
    setupWorkspaceInteractions();
  });
})();
