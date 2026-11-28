// 참조 구현: assistant-ui/assistant-ui (MIT, Copyright (c) 2026 AgentbaseAI Inc., see THIRD_PARTY_NOTICES.md)
// packages/tw-shimmer/src/index.css 1-30(흐린 글자 위로 밝은 띠가 지나가는 shimmer 방식)을 copilot.css `.eo-copilot-shimmer`에 옮겼다.
// 말풍선·접힘 단계·메시지 아래 행은 같은 모양을 바닐라 DOM으로 새로 짰다.

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

    function feedbackButton(msg, value, icon, label) {
      const btn = iconButton("eo-copilot-meta-btn", icon, label);
      btn.dataset.feedback = value;
      btn.setAttribute("aria-pressed", msg.feedback === value ? "true" : "false");
      btn.addEventListener("click", () => {
        // 표시만 로컬에 남긴다(어디로도 보내지 않는다).
        msg.feedback = msg.feedback === value ? null : value;
        for (const peer of btn.parentElement.querySelectorAll("[data-feedback]")) {
          peer.setAttribute("aria-pressed", msg.feedback === peer.dataset.feedback ? "true" : "false");
        }
      });
      return btn;
    }

    function stepsFold(msg) {
      const steps = Array.isArray(msg.steps) ? msg.steps : [];
      if (steps.length === 0 && !msg.reasoning) return null;
      const wrap = mkEl("div", "eo-copilot-steps");
      const toggle = mkEl("button", "eo-copilot-steps-toggle");
      toggle.type = "button";
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
      const row = mkEl("div", "eo-copilot-row");
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
        const meta = mkEl("div", "eo-copilot-meta");
        meta.replaceChildren(
          copyButton(() => msg.reply),
          feedbackButton(msg, "up", "thumbs-up", "좋아요"),
          feedbackButton(msg, "down", "thumbs-down", "싫어요"),
          timeLabel(msg),
        );
        row.appendChild(meta);
      }
      return row;
    }

    function renderUser(msg) {
      const row = mkEl("div", "eo-copilot-row eo-copilot-row--user");
      const bubble = mkEl("div", "eo-copilot-msg eo-copilot-msg--user");
      bubble.appendChild(mkEl("div", "eo-copilot-msg-text", msg.text));
      const meta = mkEl("div", "eo-copilot-meta");
      const resend = iconButton("eo-copilot-meta-btn eo-copilot-resend-btn", "rotate-ccw", "다시 보내기", () => hooks.resend(msg));
      resend.disabled = hooks.isBusy();
      meta.replaceChildren(timeLabel(msg), copyButton(() => msg.text), resend);
      row.replaceChildren(bubble, meta);
      return row;
    }

    const renderTurn = (msg) => (msg.role === "user" ? renderUser(msg) : renderAssistant(msg));

    /** 상대 시간 글자와 다시 보내기 단추를 지금 상태로 맞춘다(타이머 없이 패널을 만질 때마다 부른다). */
    function sync(threadEl) {
      const now = Date.now();
      for (const label of threadEl.querySelectorAll("[data-at]")) label.textContent = relativeTime(Number(label.dataset.at), now);
      for (const btn of threadEl.querySelectorAll(".eo-copilot-resend-btn")) btn.disabled = hooks.isBusy();
    }

    return { renderTurn, sync };
  }

  host.__EASYOFFICE_COPILOT_THREAD_VIEW__ = { createThreadView, relativeTime };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
