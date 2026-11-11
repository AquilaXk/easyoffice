(function (host) {
  "use strict";

  // "최근 문서" dialog (PD-098, step 2): a listbox of the recent documents with 열기 / 취소,
  // arrow keys, Enter, Escape, double click and a click on the backdrop.
  //
  // Adapted from the reference app `apps/studio-host/src/ui/recent-documents-dialog.ts` (MIT, Copyright (c)
  // 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md), rewritten as plain JavaScript with the same
  // class names. EasyOffice differences: entries are `{ id, name, kind }` (the page never sees
  // paths), and the second line shows the document kind. "목록 비우기" asks the
  // caller to clear the list (no confirmation, as in the reference app) and closes on success.

  const KIND_LABELS = Object.freeze({
    DOC: "문서", HWP: "한글 문서", SHEET: "스프레드시트", SLIDE: "프레젠테이션", PDF: "PDF",
  });

  // appendChild-based so the page also works in hosts without ParentNode.append.
  function add(parent, ...children) {
    for (const child of children) parent.appendChild(child);
  }
  function clear(parent) {
    while (parent.firstChild) parent.firstChild.remove();
  }

  // Resolves with the chosen entry or null. `options.clearRecentDocuments` empties the stored list.
  function openRecentDocumentsDialog(document, entries, options = {}) {
    return new Promise((resolve) => {
      let selected = 0;

      function element(tag, className, text) {
        const el = document.createElement(tag);
        el.className = className;
        if (text !== undefined) el.textContent = text;
        return el;
      }
      function button(className, label) {
        const el = element("button", className, label);
        el.type = "button";
        return el;
      }

      const overlay = element("div", "modal-overlay");
      const dialog = element("div", "dialog-wrap recent-documents-dialog recent-dialog");
      dialog.setAttribute("role", "dialog");
      dialog.setAttribute("aria-modal", "true");
      dialog.setAttribute("aria-label", "최근 문서");
      const titleBar = element("div", "dialog-title dialog-header", "최근 문서");
      const closeBtn = button("dialog-close", "×");
      closeBtn.setAttribute("aria-label", "닫기");
      add(titleBar, closeBtn);
      const body = element("div", "dialog-body recent-documents-body");
      const list = element("div", "recent-documents-list");
      list.setAttribute("role", "listbox");
      list.setAttribute("aria-label", "최근 문서 목록");
      add(body, list);
      const footer = element("div", "dialog-footer");
      const clearBtn = button("dialog-btn recent-documents-clear", "목록 비우기");
      const spacer = element("span", "recent-documents-footer-spacer");
      const openBtn = button("dialog-btn dialog-btn-primary", "열기");
      const cancelBtn = button("dialog-btn", "취소");
      add(footer, clearBtn, spacer, openBtn, cancelBtn);
      add(dialog, titleBar, body, footer);
      add(overlay, dialog);

      let dx = 0, dy = 0, dragging = false, startX = 0, startY = 0;
      function onDragMove(ev) {
        if (!dragging) return;
        dx = (Number(ev.clientX) || 0) - startX;
        dy = (Number(ev.clientY) || 0) - startY;
        dialog.style.transform = `translate(${dx}px, ${dy}px)`;
      }
      function stopDrag() {
        if (!dragging) return;
        dragging = false;
        document.removeEventListener?.("mousemove", onDragMove);
        document.removeEventListener?.("mouseup", stopDrag);
      }
      titleBar.addEventListener("mousedown", (ev) => {
        if (ev.button !== undefined && ev.button !== 0) return;
        if (ev.target?.closest?.(".dialog-close, button, [data-dialog-close]")) return;
        startX = (Number(ev.clientX) || 0) - dx;
        startY = (Number(ev.clientY) || 0) - dy;
        dragging = true;
        document.addEventListener?.("mousemove", onDragMove);
        document.addEventListener?.("mouseup", stopDrag);
        ev.preventDefault?.();
      });

      // Modal: the rest of the page cannot take focus while it is open, and closing hands the focus back.
      const opener = document.activeElement;
      const inertRoots = [];
      function makeBackgroundInert() {
        for (const child of Array.from(document.body.children)) {
          if (child === overlay || child.inert) continue;
          child.inert = true;
          inertRoots.push(child);
        }
      }

      function close(value) {
        stopDrag();
        document.removeEventListener("keydown", onKeyDown, true);
        overlay.remove();
        for (const root of inertRoots) root.inert = false;
        opener?.focus?.();
        resolve(value);
      }

      // Tab and Shift+Tab cycle inside the dialog.
      function trapTab(event) {
        const stops = Array.from(dialog.querySelectorAll("button")).filter((el) => !el.disabled);
        if (stops.length === 0) return;
        const first = stops[0];
        const last = stops[stops.length - 1];
        const active = document.activeElement;
        const outside = !dialog.contains(active);
        if (event.shiftKey && (outside || active === first)) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && (outside || active === last)) {
          event.preventDefault();
          first.focus();
        }
      }

      let items = [];
      let selectedItem = null;

      function updateSelection() {
        const nextItem = items[selected] || null;
        if (selectedItem && selectedItem !== nextItem) {
          selectedItem.setAttribute("aria-selected", "false");
        }
        if (nextItem) {
          nextItem.setAttribute("aria-selected", "true");
          nextItem.scrollIntoView?.({ block: "nearest" });
        }
        selectedItem = nextItem;
      }

      function renderList() {
        clear(list);
        items = [];
        selectedItem = null;
        for (const [index, entry] of entries.entries()) {
          const item = button("recent-documents-item");
          item.setAttribute("role", "option");
          const isSelected = index === selected;
          item.setAttribute("aria-selected", String(isSelected));
          if (isSelected) selectedItem = item;
          item.dataset.index = String(index);
          item.dataset.recentId = entry.id;
          add(
            item,
            element("span", "recent-documents-name", entry.name),
            element("span", "recent-documents-path", KIND_LABELS[entry.kind] || "")
          );
          item.addEventListener("click", () => {
            selected = index;
            updateSelection();
          });
          item.addEventListener("dblclick", () => close(entries[index] || null));
          items.push(item);
          add(list, item);
        }
      }

      function onKeyDown(event) {
        if (event.stopPropagation) event.stopPropagation();
        if (event.key === "Tab") {
          trapTab(event);
        } else if (event.key === "Escape") {
          event.preventDefault();
          close(null);
        } else if (event.key === "Enter") {
          event.preventDefault();
          close(entries[selected] || null);
        } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          const step = event.key === "ArrowDown" ? 1 : -1;
          selected = Math.max(0, Math.min(entries.length - 1, selected + step));
          updateSelection();
        }
      }

      closeBtn.addEventListener("click", () => close(null));
      cancelBtn.addEventListener("click", () => close(null));
      openBtn.addEventListener("click", () => close(entries[selected] || null));
      clearBtn.addEventListener("click", () => {
        clearBtn.disabled = true;
        Promise.resolve(options.clearRecentDocuments && options.clearRecentDocuments())
          .then(() => close(null))
          .catch(() => {
            clearBtn.disabled = false;
          });
      });
      overlay.addEventListener("click", (event) => {
        if (event.target === overlay) close(null);
      });

      renderList();
      add(document.body, overlay);
      makeBackgroundInert();
      document.addEventListener("keydown", onKeyDown, true);
      openBtn.focus();
    });
  }

  host.__EASYOFFICE_RECENT_DIALOG__ = { open: openRecentDocumentsDialog };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
