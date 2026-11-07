(function (host) {
  "use strict";

  // Start screen of a window that has no document yet (PD-098, step 2): a hero with the creation
  // and open buttons, the recent documents as cards (five per page) and a drop hint.
  //
  // Adapted from hop `apps/studio-host/src/ui/home-screen.ts` (MIT, Copyright (c) 2025-2026
  // Edward Kim; see THIRD_PARTY_NOTICES.md): the same section structure, class names, page size and
  // pagination, rewritten as plain JavaScript. EasyOffice differences: six "new document" buttons
  // instead of one, cards show the file name and its kind (the shell never hands paths or previews
  // to the page, PD-088), and opening a file or a recent document goes through the window bridge.

  const RECENT_PER_PAGE = 5;
  const NEW_DOCUMENTS = Object.freeze([
    { kind: "DOC", icon: "file-doc", title: "문서", description: "빈 DOCX 문서 만들기" },
    { kind: "HWP", icon: "file-hwp", title: "한글 문서", description: "빈 HWP·HWPX 문서 만들기" },
    { kind: "SHEET", icon: "file-sheet", title: "스프레드시트", description: "빈 XLSX 통합 문서 만들기" },
    { kind: "SLIDE", icon: "file-slide", title: "프레젠테이션", description: "빈 PPTX 슬라이드 만들기" },
    { kind: "PDF", icon: "file-pdf", title: "PDF 스튜디오", description: "PDF를 열어 주석·도장 넣기" },
    { kind: "CONV", icon: "file-convert", title: "포맷 변환기", description: "문서를 다른 형식으로 변환" },
  ]);
  const KIND_LABELS = Object.freeze({
    DOC: "문서", HWP: "한글 문서", SHEET: "스프레드시트", SLIDE: "프레젠테이션", PDF: "PDF",
  });
  const PREVIEW_LINES = ["home-preview-line is-wide", "home-preview-line", "home-preview-line is-short", "home-preview-line is-wide"];
  const PREVIEW_CELLS = 6;
  const EMPTY_HINT = "아직 최근 문서가 없습니다. 문서를 한 번 열면 여기에 바로 나타납니다.";
  const FILLED_HINT = "창을 열자마자 이어서 작업할 수 있도록 최근에 연 문서를 보여줍니다.";

  // appendChild-based so the page also works in hosts without ParentNode.append.
  function add(parent, ...children) {
    for (const child of children) parent.appendChild(child);
  }
  function clear(parent) {
    while (parent.firstChild) parent.firstChild.remove();
  }

  function createHomeScreen(deps) {
    const { document, onOpenRecent } = deps;

    function element(tag, className, text) {
      const el = document.createElement(tag);
      if (className) el.className = className;
      if (text !== undefined) el.textContent = text;
      return el;
    }

    function button(className, label) {
      const el = element("button", className, label);
      el.type = "button";
      return el;
    }

    function actionButton(title, description, icon) {
      const el = button("home-action");
      if (icon) {
        const slot = element("span", "home-action-icon");
        slot.dataset.icon = icon;
        add(el, slot);
      }
      const text = element("span", "home-action-text");
      add(text, element("span", "home-action-title", title), element("span", "home-action-description", description));
      add(el, text);
      return el;
    }

    function buildHero() {
      const hero = element("div", "home-hero");
      const actions = element("div", "home-actions");
      const open = actionButton("문서 열기", "홈 화면에서 파일 선택 (Ctrl+O)", "folder-open");
      open.dataset.fileOpen = "true";
      add(actions, open);
      for (const doc of NEW_DOCUMENTS) {
        const el = actionButton(doc.title, doc.description, doc.icon);
        el.dataset.windowNew = doc.kind;
        add(actions, el);
      }
      add(hero, 
        element("h1", "home-title", "최근에 작업한 문서를 바로 이어서 열어보세요"),
        element("p", "home-subtitle", "문서를 열거나 새 문서를 만들고, 드래그 앤 드롭으로 바로 시작할 수 있습니다."),
        actions
      );
      return hero;
    }

    const root = element("section", "home-screen");
    root.id = "home-screen";
    root.hidden = true;
    root.setAttribute("aria-label", "시작 화면");

    const recentSection = element("section", "home-recent");
    const header = element("div", "home-section-header");
    const pagination = element("div", "home-recent-pagination");
    const prevButton = button("home-recent-page-btn home-recent-page-prev", "이전");
    const pageLabel = element("span", "home-recent-page-label");
    const nextButton = button("home-recent-page-btn home-recent-page-next", "다음");
    const hint = element("p", "home-recent-hint");
    const track = element("div", "home-recent-track");
    add(pagination, prevButton, pageLabel, nextButton);
    add(header, element("h2", "home-section-title", "최근 문서"), pagination);
    add(recentSection, header, hint, track);
    add(root, buildHero(), recentSection, element("div", "home-drop-hint", "파일을 이 창으로 끌어다 놓아도 열 수 있습니다."));

    let recents = [];
    let page = 0;

    const totalPages = () => Math.ceil(recents.length / RECENT_PER_PAGE);

    function previewSkeleton() {
      const preview = element("div", "home-recent-preview");
      for (const cls of PREVIEW_LINES) add(preview, element("div", cls));
      const grid = element("div", "home-preview-grid");
      for (let i = 0; i < PREVIEW_CELLS; i += 1) add(grid, element("div", "home-preview-cell"));
      add(preview, grid);
      return preview;
    }

    function recentCard(entry) {
      const card = button("home-recent-card");
      card.dataset.recentId = entry.id;
      card.title = entry.name;
      const meta = element("div", "home-recent-meta");
      add(meta, element("div", "home-recent-name", entry.name), element("div", "home-recent-path", KIND_LABELS[entry.kind] || ""));
      add(card, meta, previewSkeleton());
      card.addEventListener("click", (ev) => {
        if (ev.stopPropagation) ev.stopPropagation();
        onOpenRecent(entry.id);
      });
      return card;
    }

    function placeholderCard() {
      const card = element("div", "home-recent-card is-placeholder");
      card.setAttribute("aria-hidden", "true");
      add(card, element("div", "home-recent-meta"), element("div", "home-recent-preview"));
      return card;
    }

    function updatePagination() {
      const pages = totalPages();
      pagination.hidden = pages <= 1;
      pageLabel.textContent = pages === 0 ? "" : `${page + 1} / ${pages}`;
      prevButton.disabled = page === 0;
      nextButton.disabled = pages === 0 || page >= pages - 1;
    }

    function render() {
      const pages = totalPages();
      page = pages === 0 ? 0 : Math.min(page, pages - 1);
      clear(track);
      recentSection.classList.toggle("is-empty", recents.length === 0);
      hint.textContent = recents.length === 0 ? EMPTY_HINT : FILLED_HINT;
      const items = recents.slice(page * RECENT_PER_PAGE, (page + 1) * RECENT_PER_PAGE);
      if (items.length > 0) {
        for (const entry of items) add(track, recentCard(entry));
        for (let i = items.length; i < RECENT_PER_PAGE; i += 1) add(track, placeholderCard());
      }
      updatePagination();
    }

    function changePage(direction) {
      const next = page + direction;
      if (next < 0 || next >= totalPages()) return;
      page = next;
      render();
    }
    prevButton.addEventListener("click", () => changePage(-1));
    nextButton.addEventListener("click", () => changePage(1));

    function setRecents(entries) {
      recents = Array.isArray(entries) ? entries : [];
      render();
    }

    // Puts the screen into the window (first child of the app shell) and draws its icons.
    function mount(container) {
      container.insertBefore(root, container.firstChild);
      host.__EASYOFFICE_ICONS__?.applyIcons(document, root);
      render();
    }

    return { root, mount, setRecents, show: () => { root.hidden = false; }, hide: () => { root.hidden = true; } };
  }

  host.__EASYOFFICE_CREATE_HOME_SCREEN__ = createHomeScreen;
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
