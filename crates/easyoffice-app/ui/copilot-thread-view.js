// 참조 구현: assistant-ui/assistant-ui (MIT, Copyright (c) 2026 AgentbaseAI Inc., see THIRD_PARTY_NOTICES.md), 커밋 98c2aa08
// `templates/minimal/components/assistant-ui/elements/thread.aui.tsx` 323-491(AssistantMessage·AssistantActionBar·UserMessage·UserActionBar의 요소 순서와 자리)와
// `tool-fallback.aui.tsx` 120-199(도구 줄: 상태 아이콘, 라벨, 펼침 화살표)를 바닐라 DOM으로 옮겼다. 모양 값은 `copilot.css`와 `tokens.css`의 `--copilot-*` 토큰이다.
// `packages/tw-shimmer/src/index.css` 1-30(흐린 글자 위로 밝은 띠가 지나가는 shimmer 방식)은 `.eo-copilot-shimmer`에 있다.
// 싫어요 이유 목록은 LibreChat(MIT, Copyright (c) 2026 LibreChat) 커밋 4700c44e `packages/chat/src/components/messages/Feedback.tsx` 98-224(평가 단추 두 개와 이유 Popover,
// 평가 뒤에는 고른 단추 하나만)와 `ICONS`·`FEEDBACK_TAGS`의 아이콘을 옮겼다. 이유 항목 값은 `copilot.css`의 `.eo-copilot-reason-option`.

(function (host) {
  "use strict";

  const RELATIVE_UNITS = Object.freeze([["day", 86400], ["hour", 3600], ["minute", 60], ["second", 0]]);
  const relativeFormat = new Intl.RelativeTimeFormat("ko", { numeric: "auto" });

  /** "2분 전", "어제"처럼 지금 기준 상대 시간. 1분 미만은 "지금". */
  function relativeTime(at, now = Date.now()) {
    const seconds = Math.max(0, Math.round((now - at) / 1000));
    const [unit, size] = RELATIVE_UNITS.find(([, floor]) => seconds >= floor);
    return relativeFormat.format(size ? -Math.floor(seconds / size) : 0, unit);
  }

  function stepsLabel(msg) {
    if (msg.pending) return "작업 중";
    return Math.max(1, Math.round((msg.durationMs || 0) / 1000)) + "초 동안 작업";
  }

  /**
   * 대화 한 줄(말풍선·답변·단계 접힘·미리보기 칩·메시지 아래 행)을 그린다. 상태는 `msg`에 두고(`stepsOpen`, `feedback`)
   * 바뀐 줄만 그 자리에서 고친다. 편집 흐름은 `hooks`가 이어 준다(`copilot-ui.js`).
   */
  function createThreadView({ document, copilotActions, editMode, mkEl, hooks }) {
    const icons = host.__EASYOFFICE_ICONS__;
    const feedback = host.__EASYOFFICE_COPILOT_FEEDBACK__;

    function iconButton(className, icon, label, onClick) {
      const btn = mkEl("button", className);
      btn.type = "button";
      btn.setAttribute("aria-label", label);
      btn.title = label;
      icons.setIcon(document, btn, icon);
      if (onClick) btn.addEventListener("click", onClick);
      return btn;
    }

    function chipButton(className, label, { turn, action, onClick }) {
      const btn = mkEl("button", className, label);
      btn.type = "button";
      btn.dataset.turn = turn.id;
      btn.dataset.copilotAction = action;
      btn.disabled = hooks.actionDisabled(action, turn);
      btn.addEventListener("click", onClick);
      return btn;
    }

    function copyButton(getText) {
      return iconButton("eo-copilot-meta-btn", "copy", "복사", () => {
        const clipboard = document.defaultView?.navigator?.clipboard;
        Promise.resolve(clipboard?.writeText(getText())).then(
          () => hooks.notify("복사했습니다"),
          () => hooks.notify("복사하지 못했습니다"),
        );
      });
    }

    function timeLabel(msg) {
      const label = mkEl("span", "eo-copilot-meta-time", relativeTime(msg.at));
      label.dataset.at = String(msg.at);
      return label;
    }

    // 싫어요 이유 목록: 원본 Feedback.tsx의 Popover(단추 아래, 항목은 아이콘 + 글자, 고른 항목은 aria-pressed)를 하나의 메뉴 요소로 옮겼다.
    const reasonMenuEl = document.getElementById("copilot-feedback-menu");
    const reasonMenu = reasonMenuEl ? host.__EASYOFFICE_COPILOT_POPUP__.createAnchoredMenu({ document, menu: reasonMenuEl }) : null;

    function reasonItems(msg) {
      return feedback.getTagsForRating("down").map((tag) => {
        const item = mkEl("button", "eo-copilot-reason-option");
        item.type = "button";
        item.dataset.tag = tag.key;
        item.setAttribute("role", "menuitem");
        item.setAttribute("aria-pressed", msg.feedbackTag === tag.key && msg.feedback === "down" ? "true" : "false");
        icons.setIcon(document, item, tag.icon);
        item.appendChild(mkEl("span", undefined, tag.label));
        item.addEventListener("click", () => {
          if (msg.feedback !== "down") feedback.rate(msg, "down");
          feedback.pickTag(msg, tag.key);
          hooks.rerender();
        });
        return item;
      });
    }

    // 원본처럼 평가가 없으면 좋아요·싫어요 두 단추, 평가가 있으면 고른 단추 하나(누르면 평가를 거둔다)만 보인다.
    // 평가는 대화 기록(`msg`)에 남고 다음 요청의 평가 줄이 된다(`copilot-context-compact.js`). 어디로도 따로 보내지 않는다.
    function feedbackButtons(msg) {
      const make = (value, icon, label) => {
        const btn = iconButton("eo-copilot-meta-btn", icon, label);
        btn.dataset.feedback = value;
        btn.setAttribute("aria-pressed", msg.feedback === value ? "true" : "false");
        return btn;
      };
      const up = make("up", "thumbs-up", "좋아요");
      const down = make("down", "thumbs-down", "싫어요");
      const takeBack = (value) => () => {
        feedback.rate(msg, value);
        hooks.rerender();
      };
      if (msg.feedback === "up") {
        up.addEventListener("click", takeBack("up"));
        return [up];
      }
      if (msg.feedback === "down") {
        down.addEventListener("click", takeBack("down"));
        return [down];
      }
      up.addEventListener("click", takeBack("up"));
      down.setAttribute("aria-haspopup", "menu");
      down.setAttribute("aria-expanded", "false");
      down.addEventListener("click", () => (reasonMenu ? reasonMenu.open(down, reasonItems(msg)) : takeBack("down")()));
      return [up, down];
    }

    function stepsFold(msg) {
      const steps = Array.isArray(msg.steps) ? msg.steps : [];
      if (steps.length === 0 && !msg.reasoning) return null;
      const wrap = mkEl("div", "eo-copilot-steps");
      const toggle = mkEl("button", "eo-copilot-steps-toggle");
      toggle.type = "button";
      toggle.dataset.running = msg.pending ? "true" : "false";
      // 진행 중이면 돌아가는 아이콘, 끝나면 체크(원본 `statusIconMap`의 running·complete). 반짝이는 문구는 아래 대기 줄 하나만 둔다.
      const status = mkEl("span", "eo-copilot-steps-status");
      icons.setIcon(document, status, msg.pending ? "loader" : "check");
      toggle.appendChild(status);
      toggle.appendChild(mkEl("span", "eo-copilot-steps-label", stepsLabel(msg)));
      const caret = mkEl("span", "eo-copilot-steps-caret");
      icons.setIcon(document, caret, "chevron-right");
      toggle.appendChild(caret);
      const panel = mkEl("div", "eo-copilot-steps-panel");
      if (msg.reasoning) panel.appendChild(mkEl("div", "eo-copilot-reasoning", msg.reasoning));
      if (steps.length > 0) {
        const list = mkEl("ul", "eo-copilot-steps-list");
        for (const label of steps) list.appendChild(mkEl("li", undefined, label));
        panel.appendChild(list);
      }
      const apply = () => {
        toggle.setAttribute("aria-expanded", msg.stepsOpen ? "true" : "false");
        panel.hidden = !msg.stepsOpen;
      };
      toggle.addEventListener("click", () => {
        msg.stepsOpen = !msg.stepsOpen;
        apply();
      });
      apply();
      wrap.replaceChildren(toggle, panel);
      return wrap;
    }

    function chipRow(msg) {
      const locs = Array.isArray(msg.locations) ? msg.locations : [];
      const reviewButtons = editMode.reviewActions(msg.change);
      const showUndo = !msg.undone && copilotActions.hasUndoEntry(msg.id) && msg.change?.state !== "draft";
      const showRegenerate = !msg.pending && Boolean(msg.request) && hooks.isLastTurn(msg);
      if (locs.length === 0 && !showUndo && !showRegenerate && reviewButtons.length === 0) return null;
      const row = mkEl("div", "eo-copilot-chip-row");
      for (const [action, label] of reviewButtons) {
        row.appendChild(chipButton("eo-copilot-chip-btn eo-copilot-review-btn", label, { turn: msg, action, onClick: () => void hooks.review[action](msg) }));
      }
      for (const loc of locs) {
        const chip = mkEl("button", "eo-copilot-loc-chip");
        chip.type = "button";
        chip.appendChild(hooks.locationIcon(loc.module));
        chip.appendChild(mkEl("span", undefined, loc.label));
        chip.addEventListener("click", () => hooks.locate(loc));
        row.appendChild(chip);
      }
      if (showUndo) row.appendChild(chipButton("eo-copilot-undo-btn", "실행 취소", { turn: msg, action: "undo", onClick: () => hooks.undo(msg) }));
      if (showRegenerate) row.appendChild(chipButton("eo-copilot-chip-btn", "다시 생성", { turn: msg, action: "regenerate", onClick: () => void hooks.regenerate() }));
      return row;
    }

    function renderAssistant(msg) {
      const row = mkEl("div", "eo-copilot-row eo-copilot-row--assistant");
      const body = mkEl("div", "eo-copilot-msg eo-copilot-msg--assistant");
      const fold = stepsFold(msg);
      if (fold) body.appendChild(fold);
      if (msg.updatedInPlace && !msg.undone) {
        const statusRow = mkEl("div", "eo-copilot-inplace-row");
        statusRow.appendChild(hooks.checkIcon());
        statusRow.appendChild(mkEl("span", undefined, "제자리에서 업데이트됨"));
        body.appendChild(statusRow);
      }
      if (msg.reply) {
        const replyEl = mkEl("div", "eo-copilot-reply" + (msg.error ? " eo-copilot-reply--error" : ""));
        replyEl.dataset.turnReply = msg.id;
        hooks.renderReply(replyEl, msg.reply, msg.error);
        body.appendChild(replyEl);
      }
      if (msg.pending) body.appendChild(mkEl("div", "eo-copilot-loading eo-copilot-shimmer", "응답을 기다리는 중…"));
      if (msg.change?.state === "pending") {
        body.appendChild(copilotActions.renderChangeList(msg.change.changes, (loc) => hooks.locate(loc)));
      } else if (editMode.isDiscarded(msg.change)) {
        body.appendChild(mkEl("div", "eo-copilot-inplace-row", editMode.DISCARDED_LABEL));
      }
      const chips = chipRow(msg);
      if (chips) body.appendChild(chips);
      row.appendChild(body);
      if (!msg.pending && msg.reply) {
        const meta = mkEl("div", "eo-copilot-meta eo-copilot-meta--footer");
        meta.replaceChildren(
          copyButton(() => msg.reply),
          ...feedbackButtons(msg),
          timeLabel(msg),
        );
        row.appendChild(meta);
      }
      return row;
    }

    // 사용자 메시지: 말풍선 아래 오른쪽에 시간 · 복사 · 다시 보내기(QA 목표 화면의 배치). 말풍선과 단추 크기·간격은 원본 값이다.
    function renderUser(msg) {
      const row = mkEl("div", "eo-copilot-row eo-copilot-row--user");
      const bubble = mkEl("div", "eo-copilot-msg eo-copilot-msg--user");
      bubble.appendChild(mkEl("div", "eo-copilot-msg-text", msg.text));
      const meta = mkEl("div", "eo-copilot-meta eo-copilot-meta--user");
      const resend = iconButton("eo-copilot-meta-btn eo-copilot-resend-btn", "rotate-ccw", "다시 보내기", () => hooks.resend(msg));
      resend.disabled = hooks.isBusy();
      meta.replaceChildren(timeLabel(msg), copyButton(() => msg.text), resend);
      row.replaceChildren(bubble, meta);
      return row;
    }

    // 새로 생긴 줄만 들어오는 움직임을 준다(다시 그릴 때마다 모든 줄이 깜박이지 않게 `entered`를 대화 기록에 남긴다).
    function renderTurn(msg) {
      const row = msg.role === "user" ? renderUser(msg) : renderAssistant(msg);
      if (!msg.entered) {
        msg.entered = true;
        row.classList.add("is-entering");
      }
      return row;
    }

    /** 상대 시간 글자와 다시 보내기 단추를 지금 상태로 맞춘다(타이머 없이 패널을 만질 때마다 부른다). */
    function sync(threadEl) {
      const now = Date.now();
      for (const label of threadEl.querySelectorAll("[data-at]")) label.textContent = relativeTime(Number(label.dataset.at), now);
      for (const btn of threadEl.querySelectorAll(".eo-copilot-resend-btn")) btn.disabled = hooks.isBusy();
    }

    return { renderTurn, sync };
  }

  // 맨 아래에서 이만큼(소수 스크롤 오차) 안쪽이면 맨 아래로 본다.
  const SCROLL_END_TOLERANCE_PX = 2;

  /**
   * 본문은 `scroll-behavior: smooth`(원본 scroll-smooth)라서 다시 그릴 때는 그 자리로 바로 가고(`jumpTo`), 맨 아래로 단추만 부드럽게 간다.
   * 단추는 맨 아래에서 벗어났을 때만 보인다(원본 ThreadScrollToBottom의 disabled:invisible).
   */
  function createScrollControl({ document }) {
    const body = () => document.getElementById("copilot-body");
    const button = () => document.getElementById("btn-copilot-scroll-bottom");
    function scrollTo(top, smooth) {
      const el = body();
      if (!el) return;
      el.style.scrollBehavior = smooth ? "" : "auto";
      el.scrollTop = top;
      el.style.scrollBehavior = "";
    }
    function sync() {
      const el = body();
      const btn = button();
      if (el && btn) btn.hidden = el.scrollHeight - el.scrollTop - el.clientHeight <= SCROLL_END_TOLERANCE_PX;
    }
    function bind() {
      body()?.addEventListener("scroll", sync);
      button()?.addEventListener("click", () => scrollTo(body()?.scrollHeight || 0, true));
    }
    return { jumpTo: (top) => scrollTo(top, false), sync, bind };
  }

  host.__EASYOFFICE_COPILOT_THREAD_VIEW__ = { createThreadView, createScrollControl, relativeTime };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
