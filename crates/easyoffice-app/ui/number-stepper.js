(function (host) {
  "use strict";

  // The number field of the dialogs and bars: the field with a stacked up/down button cell on its right end, in place of the
  // browser's own spin button (PD-377).
  //
  // Reference implementation (structure ported): the Stepper of the Apache-2.0 CSS design system named in THIRD_PARTY_NOTICES.md
  // ("Spectrum CSS" section; its repository is archived, the successor repository is named there too), files
  // `components/stepper/index.css` and `components/stepper/stories/template.js`. Kept from the original: the text field plus a
  // column of two stacked in-field buttons (bordered fills, PD-378) that is as high as the field and rounds only the end-side
  // corners, and the disabled state of the buttons following the field. Changed to
  // fit: plain DOM in place of the Lit template, the stacked buttons carry the chevrons of toolbar.css, the colors, sizes and
  // focus are the tokens of tokens.css.
  //
  // The step follows the native one: the `step` of the field (default 1), the `min` and `max` clamp, the decimals of the step
  // and of the value (no `0.30000000000000004`), the `input` and `change` events after a press, nothing for a disabled field.
  // The keys (up and down arrows) stay the native ones of the field.

  const SKIPPED = new WeakSet();

  function el(document, tag, className) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    return node;
  }

  function notify(input) {
    for (const type of ["input", "change"]) {
      const EventCtor = host.Event;
      input.dispatchEvent(typeof EventCtor === "function" ? new EventCtor(type, { bubbles: true }) : { type, bubbles: true, target: input });
    }
  }

  const decimalsOf = (text) => {
    const at = String(text).indexOf(".");
    return at < 0 ? 0 : String(text).length - at - 1;
  };

  // One press: `direction` is 1 (up) or -1 (down). Returns true when the value changed.
  function stepField(input, direction) {
    if (input.disabled || input.readOnly) return false;
    const limit = (name, fallback) => {
      const text = input.getAttribute(name);
      return text === null || text === "" || !Number.isFinite(Number(text)) ? fallback : Number(text);
    };
    const stepSize = limit("step", 1) > 0 ? limit("step", 1) : 1;
    const min = limit("min", Number.NEGATIVE_INFINITY);
    const max = limit("max", Number.POSITIVE_INFINITY);
    const current = input.value === "" ? Number.NaN : Number(input.value);
    const from = Number.isFinite(current) ? current : (Number.isFinite(min) ? min - direction * stepSize : -direction * stepSize);
    const decimals = Math.max(decimalsOf(input.getAttribute("step") || "1"), decimalsOf(input.value));
    const next = Number(Math.min(max, Math.max(min, from + direction * stepSize)).toFixed(decimals));
    if (!Number.isFinite(next) || (Number.isFinite(current) && next === current)) return false;
    input.value = String(next);
    notify(input);
    return true;
  }

  function nameOf(document, input) {
    const label = input.id ? document.querySelector(`label[for="${input.id}"]`) : null;
    return input.getAttribute("aria-label") || (label?.textContent || "").trim() || input.title || "";
  }

  function enhanceNumberField(document, input) {
    if (SKIPPED.has(input) || input.closest?.(".eo-stepper") || input.dataset?.nativeNumber === "true") return;
    SKIPPED.add(input);
    // The box takes the field's classes (`dialog-input`, `sb-spacing-input` ...), the way a combo box does, so the sizes and the
    // frame stay what the page gave the field; the field inside loses its own frame (toolbar.css).
    const root = el(document, "span", `eo-stepper ${input.className}`.trim());
    input.parentNode.insertBefore(root, input);
    root.appendChild(input);
    input.classList.add("eo-stepper-input");
    const name = nameOf(document, input);
    const cell = el(document, "span", "eo-stepper-buttons");
    const buttons = [["up", 1, "크게"], ["down", -1, "작게"]].map(([way, direction, text]) => {
      const button = el(document, "button", "eo-stepper-button");
      button.type = "button";
      button.tabIndex = -1;
      button.setAttribute("aria-label", `${name} ${text}`.trim());
      button.appendChild(el(document, "span", `eo-chevron eo-chevron-${way}`));
      button.firstChild.setAttribute("aria-hidden", "true");
      // The press keeps the focus in the field, as the buttons of a combo box do.
      button.addEventListener("mousedown", (event) => {
        event.preventDefault?.();
        input.focus?.();
      });
      button.addEventListener("click", () => stepField(input, direction));
      cell.appendChild(button);
      return button;
    });
    root.appendChild(cell);
    const sync = () => {
      root.classList.toggle("disabled", Boolean(input.disabled));
      for (const button of buttons) button.disabled = Boolean(input.disabled);
    };
    // A page switches a field off with `input.disabled = true`: the buttons follow, as a combo box follows its value.
    const own = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(input), "disabled");
    if (own?.get && own?.set) {
      Object.defineProperty(input, "disabled", {
        configurable: true,
        get() {
          return own.get.call(input);
        },
        set(value) {
          own.set.call(input, value);
          sync();
        },
      });
    }
    if (typeof host.MutationObserver === "function") new host.MutationObserver(sync).observe(input, { attributes: true, attributeFilter: ["disabled"] });
    sync();
  }

  function enhanceNumberFields(document, scope) {
    for (const input of Array.from((scope || document).querySelectorAll('input[type="number"]'))) enhanceNumberField(document, input);
  }

  host.__EASYOFFICE_NUMBER_STEPPER__ = { enhanceNumberFields, stepField };
})(globalThis.__EASYOFFICE_HOST_ENV__ || globalThis);
