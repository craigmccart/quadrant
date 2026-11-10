// Shared UI pieces: buttons, chips, dialogs, toasts, tables, progress.
// Styles are in css/components.css.

import { api } from "./api.js";
import { h, icon, fill } from "./dom.js";
import * as fmt from "./format.js";
import { app } from "./store.js";

// ── Buttons and chips ───────────────────────────────────────────────────────

/**
 * button("Scan", { icon: "play", variant: "primary", size: "lg", onClick })
 * variant: "primary" | "danger" | "quiet" | undefined (outlined)
 */
export function button(label, opts = {}) {
  const classes = ["btn"];
  if (opts.variant) classes.push(`btn-${opts.variant}`);
  if (opts.size) classes.push(`btn-${opts.size}`);
  if (opts.iconOnly) classes.push("btn-icon");
  return h(
    "button",
    {
      type: "button",
      class: classes.join(" "),
      onclick: opts.onClick,
      disabled: opts.disabled,
      title: opts.title || (opts.iconOnly ? label : undefined),
      "aria-label": opts.iconOnly ? label : undefined,
    },
    opts.icon && icon(opts.icon),
    !opts.iconOnly && label,
    opts.iconAfter && icon(opts.iconAfter)
  );
}

export function link(label, onClick, iconName = "chevron-right") {
  return h("button", { type: "button", class: "link", onclick: onClick }, label, icon(iconName));
}

export function chip(text, tone, iconName) {
  return h("span", { class: `chip ${tone || ""}` }, iconName && icon(iconName), text);
}

const STATUS_SHORT = { good: "Good", attention: "Attention", problem: "Problem", unknown: "Not checked" };
const STATUS_ICON = { good: "good", attention: "attention", problem: "problem", unknown: "unknown" };

/** A status chip: always an icon and a word, never colour alone. */
export function statusChip(status, text) {
  return chip(text || STATUS_SHORT[status] || STATUS_SHORT.unknown, status === "unknown" ? "" : status, STATUS_ICON[status] || "unknown");
}

export function riskChip(risk) {
  return risk === "safe"
    ? chip("Safe to clear", "good", "check")
    : chip("Check first", "attention", "eye");
}

// ── Page furniture ──────────────────────────────────────────────────────────

export function pageHead(title, subtitle, ...actions) {
  return h(
    "header",
    { class: "page-head" },
    h("div", { class: "page-head-text" }, h("h1", { class: "page-title" }, title), subtitle && h("p", { class: "page-sub" }, subtitle)),
    actions.length > 0 && h("div", { class: "page-actions" }, actions)
  );
}

export function note(tone, text, action) {
  const iconName = tone === "attention" ? "attention" : tone === "problem" ? "problem" : "info";
  return h("div", { class: `note ${tone}` }, icon(iconName), h("div", { class: "note-body" }, text), action);
}

export function emptyState({ icon: iconName, title, text, action }) {
  return h(
    "div",
    { class: "empty" },
    h("div", { class: "empty-icon" }, icon(iconName)),
    h("div", { class: "empty-title" }, title),
    text && h("p", { class: "empty-text" }, text),
    action
  );
}

/** A row of headline numbers. items: [{ value, label }] */
export function stats(items) {
  return h(
    "div",
    { class: "stats" },
    items.map((item) => h("div", { class: "stat" }, h("div", { class: "stat-value" }, item.value), h("div", { class: "stat-label" }, item.label)))
  );
}

/** How full something is. tone: "good" | "attention" | "problem" | "quiet" */
export function meter(fraction, tone = "") {
  const pct = Math.max(0, Math.min(1, fraction || 0)) * 100;
  return h(
    "div",
    { class: `meter ${tone}`, role: "meter", "aria-valuemin": 0, "aria-valuemax": 100, "aria-valuenow": Math.round(pct) },
    h("div", { class: "meter-fill", style: { "--pct": `${pct}%` } })
  );
}

/** Tone for how full a drive or memory is. */
export function fullnessTone(usedFraction) {
  if (usedFraction >= 0.9) return "problem";
  if (usedFraction >= 0.75) return "attention";
  return "good";
}

/** segmented([{ value, label, icon }], current, onChange) */
export function segmented(options, current, onChange, label) {
  const group = h("div", { class: "segmented", role: "group", "aria-label": label });
  const render = (value) => {
    fill(
      group,
      options.map((opt) =>
        h(
          "button",
          {
            type: "button",
            "aria-pressed": String(opt.value === value),
            onclick: () => {
              render(opt.value);
              onChange(opt.value);
            },
          },
          opt.icon && icon(opt.icon),
          opt.label
        )
      )
    );
  };
  render(current);
  return group;
}

export function checkbox(checked, onChange, label, disabled = false) {
  return h("input", {
    type: "checkbox",
    class: "check",
    checked,
    disabled,
    "aria-label": label,
    onchange: (e) => onChange(e.target.checked),
    onclick: (e) => e.stopPropagation(),
  });
}

export function toggle(checked, onChange, label, disabled = false, title) {
  return h("input", {
    type: "checkbox",
    role: "switch",
    class: "switch",
    checked,
    disabled,
    title,
    "aria-label": label,
    onchange: (e) => onChange(e.target.checked, e.target),
  });
}

export function searchBox(placeholder, onInput) {
  return h(
    "div",
    { class: "search" },
    icon("search"),
    h("input", { type: "search", class: "input", placeholder, "aria-label": placeholder, oninput: (e) => onInput(e.target.value.trim().toLowerCase()) })
  );
}

// ── Progress ────────────────────────────────────────────────────────────────

/**
 * The "working" strip shown while a job runs.
 *   const busy = busyPanel(() => api.cancel("disk"));
 *   busy.update(progress)   // { message, fraction, items, bytes }
 */
export function busyPanel(onStop, stopLabel = "Stop") {
  const message = h("div", { class: "busy-message" }, "Starting");
  const tally = h("div", { class: "busy-tally num" });
  const bar = h("div", { class: "progress indeterminate" }, h("div", { class: "progress-fill" }));
  const fillEl = bar.firstChild;
  const stop = onStop && button(stopLabel, { icon: "stop", onClick: () => { stop.disabled = true; onStop(); } });
  const el = h(
    "div",
    { class: "busy", role: "status", "aria-live": "polite" },
    h("div", { class: "busy-main" }, h("div", { class: "busy-text" }, message, tally), bar),
    stop
  );
  return {
    el,
    update(p) {
      if (p.message) message.textContent = p.message;
      const parts = [];
      if (p.items) parts.push(fmt.plural(p.items, "file"));
      if (p.bytes) parts.push(fmt.bytes(p.bytes));
      tally.textContent = parts.join(" · ");
      if (typeof p.fraction === "number") {
        bar.classList.remove("indeterminate");
        fillEl.style.width = `${Math.round(p.fraction * 100)}%`;
      } else {
        bar.classList.add("indeterminate");
        fillEl.style.width = "";
      }
    },
  };
}

// ── Toasts ──────────────────────────────────────────────────────────────────

let toastHost;

/** toast("Saved", "good") or toast(msg, "problem", { action: buttonEl, sticky: true }) */
export function toast(message, tone = "good", opts = {}) {
  if (!toastHost) {
    toastHost = h("div", { class: "toasts", role: "status", "aria-live": "polite" });
    document.body.append(toastHost);
  }
  const iconName = tone === "problem" ? "problem" : tone === "attention" ? "attention" : tone === "info" ? "info" : "good";
  const el = h("div", { class: `toast ${tone}` }, icon(iconName), h("div", { class: "toast-body" }, message), opts.action);
  const close = () => {
    el.classList.add("leaving");
    setTimeout(() => el.remove(), 260);
  };
  el.append(button("Dismiss", { icon: "close", iconOnly: true, variant: "quiet", size: "sm", onClick: close }));
  toastHost.append(el);
  if (!opts.sticky) setTimeout(close, tone === "problem" ? 9000 : 5000);
  return close;
}

/**
 * Show a backend error. Cancelling is not an error. When Windows refused
 * access and we are not elevated, offer the one thing that fixes it.
 */
export function showError(err) {
  if (!err || err.kind === "cancelled") return;
  if (err.kind === "denied" && !app.isElevated) {
    toast(err.message, "problem", {
      action: button("Restart as administrator", { size: "sm", onClick: restartAsAdmin }),
      sticky: true,
    });
    return;
  }
  toast(err.message || String(err), "problem");
}

export async function restartAsAdmin() {
  try {
    await api.relaunchAsAdmin();
  } catch (err) {
    // Saying no to the Windows prompt is a choice, not a failure.
    if (err.kind !== "cancelled") toast(err.message, "problem");
  }
}

// ── Dialogs ─────────────────────────────────────────────────────────────────

/**
 * A modal dialog built on <dialog>, which gives focus trapping and Escape
 * for free. Resolves with the value of the button pressed, or null if the
 * dialog was dismissed.
 *   buttons: [{ label, value, variant, autofocus }]
 */
export function dialog({ title, body, buttons, wide }) {
  return new Promise((resolve) => {
    let result = null;
    const el = h(
      "dialog",
      { class: "dialog", style: wide ? { width: "min(720px, calc(100vw - 48px))" } : undefined },
      h("div", { class: "dialog-head" }, h("h2", { class: "dialog-title" }, title)),
      h("div", { class: "dialog-body" }, body),
      h(
        "div",
        { class: "dialog-foot" },
        buttons.map((b) => {
          const btn = button(b.label, {
            variant: b.variant,
            icon: b.icon,
            onClick: () => {
              result = b.value;
              el.close();
            },
          });
          if (b.autofocus) btn.autofocus = true;
          return btn;
        })
      )
    );
    el.addEventListener("close", () => {
      el.remove();
      resolve(result);
    });
    // Clicking the dimmed area outside the dialog dismisses it.
    el.addEventListener("click", (e) => {
      if (e.target === el) el.close();
    });
    document.body.append(el);
    el.showModal();
  });
}

/** Ask before doing something. Resolves true only on the confirm button. */
export async function confirm({ title, body, confirmLabel, danger = false, icon: iconName, wide }) {
  const answer = await dialog({
    title,
    body,
    wide,
    buttons: [
      { label: "Cancel", value: false, autofocus: true },
      { label: confirmLabel, value: true, variant: danger ? "danger" : "primary", icon: iconName },
    ],
  });
  return answer === true;
}

/** An itemised list for dialogs. rows: [{ name, note, value, total }] */
export function ledger(rows) {
  return h(
    "div",
    { class: "ledger" },
    rows.map((row) =>
      h(
        "div",
        { class: `ledger-row ${row.total ? "total" : ""}` },
        h("div", { class: "ledger-name" }, row.name, row.note && h("span", { class: "ledger-note" }, row.note)),
        h("div", { class: "ledger-value" }, row.value)
      )
    )
  );
}

// ── Tables ──────────────────────────────────────────────────────────────────

/**
 * A sortable table.
 *   columns: [{ label, render(row) -> Node|string, sort(row) -> string|number,
 *               align: "right", tight: true, wrap: true }]
 *   opts: { sortBy: columnIndex, descending, pageSize, rowClass(row), empty }
 * Returns { el, setRows(rows) }.
 */
export function table(columns, opts = {}) {
  let rows = [];
  let sortIndex = opts.sortBy ?? -1;
  let descending = opts.descending ?? false;
  let shown = opts.pageSize ?? 300;
  const pageSize = shown;

  const thead = h("thead");
  const tbody = h("tbody");
  const foot = h("div", { class: "table-foot" });
  const el = h("div", { class: "table-wrap" }, h("div", { class: "table-scroll" }, h("table", { class: "table" }, thead, tbody)), foot);

  const cellClass = (col) => [col.align === "right" && "right", col.tight && "tight", col.wrap && "wrap", col.mono && "mono"].filter(Boolean).join(" ");

  function renderHead() {
    fill(
      thead,
      h(
        "tr",
        null,
        columns.map((col, i) => {
          const active = i === sortIndex;
          const inner = col.sort
            ? h(
                "button",
                {
                  type: "button",
                  onclick: () => {
                    if (sortIndex === i) descending = !descending;
                    else {
                      sortIndex = i;
                      descending = Boolean(col.descFirst);
                    }
                    shown = pageSize;
                    renderHead();
                    renderBody();
                  },
                },
                col.label,
                icon(active ? (descending ? "sort-down" : "sort-up") : "sort")
              )
            : h("span", null, col.label || h("span", { class: "visually-hidden" }, col.srLabel || "Actions"));
          return h("th", { scope: "col", class: cellClass(col), "aria-sort": active ? (descending ? "descending" : "ascending") : undefined }, inner);
        })
      )
    );
  }

  function sorted() {
    const col = columns[sortIndex];
    if (!col || !col.sort) return rows;
    const dir = descending ? -1 : 1;
    return [...rows].sort((a, b) => {
      const x = col.sort(a);
      const y = col.sort(b);
      if (typeof x === "number" && typeof y === "number") return (x - y) * dir;
      return String(x).localeCompare(String(y), "en-GB", { sensitivity: "base", numeric: true }) * dir;
    });
  }

  function renderBody() {
    const all = sorted();
    const visible = all.slice(0, shown);
    if (all.length === 0) {
      fill(tbody, h("tr", null, h("td", { colSpan: columns.length, class: "muted" }, opts.empty || "Nothing to show.")));
    } else {
      fill(
        tbody,
        visible.map((row) =>
          h(
            "tr",
            { class: opts.rowClass ? opts.rowClass(row) : undefined },
            columns.map((col) => h("td", { class: cellClass(col) }, col.render(row)))
          )
        )
      );
    }
    if (all.length > visible.length) {
      foot.hidden = false;
      fill(
        foot,
        h("span", null, `Showing ${fmt.count(visible.length)} of ${fmt.count(all.length)}`),
        button("Show more", {
          size: "sm",
          onClick: () => {
            shown += pageSize;
            renderBody();
          },
        })
      );
    } else {
      foot.hidden = true;
    }
  }

  renderHead();
  renderBody();

  return {
    el,
    setRows(next) {
      rows = next;
      renderBody();
    },
    refresh: renderBody,
  };
}

/** Two-line table cell: a main line with a quieter line under it. */
export function twoLine(main, sub, subClass = "") {
  return h("div", null, h("div", { class: "cell-main" }, main), sub && h("div", { class: `cell-sub ${subClass}` }, sub));
}

/** "Show in File Explorer" icon button for a path. */
export function revealButton(path) {
  return button("Show in File Explorer", {
    icon: "folder-open",
    iconOnly: true,
    variant: "quiet",
    size: "sm",
    onClick: (e) => {
      e.stopPropagation();
      api.reveal(path).catch(showError);
    },
  });
}

/** A folder field with a Browse button. Returns { el, value() }. */
export function folderField(initial, label = "Folder") {
  const input = h("input", { type: "text", class: "input mono", value: initial, spellcheck: false, "aria-label": label });
  const browse = button("Browse", {
    icon: "folder-open",
    onClick: async () => {
      try {
        const picked = await api.pickFolder();
        if (picked) input.value = picked;
      } catch (err) {
        showError(err);
      }
    },
  });
  const el = h("div", { class: "folder-field" }, input, browse);
  return { el, input, value: () => input.value.trim() };
}

export function select(options, current, label) {
  return h(
    "select",
    { class: "select", "aria-label": label },
    options.map((opt) => h("option", { value: String(opt.value), selected: opt.value === current }, opt.label))
  );
}
