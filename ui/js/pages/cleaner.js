// Junk Cleaner and Privacy. Both are the same page over a different list of
// categories: scan, tick what should go, look inside if you want, clear.

import { api, onProgress } from "../api.js";
import { h, icon, fill } from "../dom.js";
import * as fmt from "../format.js";
import { app, cleanOptions, on, setBusy } from "../store.js";
import { navigate } from "../router.js";
import { button, pageHead, emptyState, busyPanel, stats, riskChip, chip, checkbox, showError, revealButton, note } from "../ui.js";
import { runClean } from "../cleanflow.js";

const COPY = {
  junk: {
    id: "junk",
    label: "Junk Cleaner",
    icon: "junk",
    subtitle: "Temporary files, caches and leftovers that are taking up space. Nothing is removed until you choose it.",
    emptyTitle: "Find out what can go",
    emptyText: "Quadrant measures temporary files, browser caches, crash dumps, old logs and more. It shows you every category, and the files inside, before anything is cleared.",
    scanLabel: "Scan for junk",
    unit: "bytes",
  },
  privacy: {
    id: "privacy",
    label: "Privacy",
    icon: "privacy",
    subtitle: "Lists Windows keeps of what you opened, searched for and typed. Clearing them does not touch your files.",
    emptyTitle: "See what Windows remembers",
    emptyText: "Recent files, jump lists, and the history in File Explorer and the Run box. Quadrant counts them first, then you choose what to clear.",
    scanLabel: "Scan for traces",
    unit: "items",
  },
};

/** How much a category holds, in the unit this page talks in. */
const weight = (c, unit) => (unit === "bytes" ? c.bytes : c.itemCount);

function makePage(area) {
  const copy = COPY[area];

  function mount(root) {
    let scan = null;
    let running = false;
    const selected = new Set();
    const expanded = new Set();
    const fileCache = new Map();

    const actions = h("div", { class: "page-actions" });
    const head = pageHead(copy.label, copy.subtitle);
    head.append(actions);
    const busyHost = h("div");
    const body = h("div", { class: "stack" });
    root.append(head, busyHost, body);

    const isEmpty = (c) => c.bytes === 0 && c.itemCount === 0;

    function adopt(next) {
      scan = next;
      selected.clear();
      expanded.clear();
      fileCache.clear();
      for (const c of scan.categories) if (c.defaultSelected && !isEmpty(c)) selected.add(c.id);
    }

    const chosen = () => scan.categories.filter((c) => selected.has(c.id));

    function figure(categories) {
      if (copy.unit === "bytes") return fmt.bytes(categories.reduce((n, c) => n + c.bytes, 0));
      return fmt.count(categories.reduce((n, c) => n + c.itemCount, 0));
    }

    // ── Rendering ───────────────────────────────────────────────────────────

    function renderActions() {
      if (running) {
        fill(actions);
        return;
      }
      if (!scan) {
        fill(actions, button(copy.scanLabel, { variant: "primary", icon: "search", onClick: runScan }));
        return;
      }
      const picks = chosen();
      const what = copy.unit === "bytes" ? figure(picks) : fmt.plural(picks.reduce((n, c) => n + c.itemCount, 0), "item");
      fill(
        actions,
        button("Scan again", { icon: "refresh", onClick: runScan }),
        button(picks.length ? `Clear ${what}` : "Nothing selected", { variant: "primary", icon: "junk", disabled: picks.length === 0, onClick: clean })
      );
    }

    function fileList(id) {
      const host = h("div", { class: "file-list-host" }, h("p", { class: "subtle" }, "Listing files"));
      const show = (result) => {
        if (result.files.length === 0) {
          fill(host, h("p", { class: "subtle" }, "No files to show."));
          return;
        }
        fill(
          host,
          h(
            "ul",
            { class: "file-list" },
            result.files.map((f) =>
              h("li", null, h("span", { class: "path", title: f.path }, fmt.midPath(f.path, 86)), h("span", { class: "subtle num" }, fmt.date(f.modified)), h("span", { class: "mono" }, fmt.bytes(f.bytes)), revealButton(f.path))
            )
          ),
          result.truncated && h("p", { class: "subtle" }, `Showing the ${fmt.count(result.files.length)} biggest of ${fmt.count(result.totalItems)}.`)
        );
      };
      if (fileCache.has(id)) show(fileCache.get(id));
      else {
        api
          .cleanFiles(id, cleanOptions())
          .then((result) => {
            fileCache.set(id, result);
            show(result);
          })
          .catch((err) => fill(host, h("p", { class: "subtle" }, err.message)));
      }
      return host;
    }

    function row(c) {
      const empty = isEmpty(c);
      const open = expanded.has(c.id);
      const toggleOpen = () => {
        if (open) expanded.delete(c.id);
        else expanded.add(c.id);
        renderList();
      };
      const main = h(
        "div",
        { class: `clean-row ${empty ? "is-zero" : ""}`, onclick: toggleOpen },
        checkbox(selected.has(c.id), (on_) => {
          if (on_) selected.add(c.id);
          else selected.delete(c.id);
          renderSummary();
          renderActions();
        }, `Clear ${c.label}`, empty),
        h(
          "div",
          { class: "clean-main" },
          h(
            "div",
            { class: "clean-title" },
            c.label,
            !empty && riskChip(c.risk),
            c.needsAdmin && !app.isElevated && chip("Needs administrator", "info", "shield"),
            c.toRecycleBin && chip("To Recycle Bin", "", "recycle")
          ),
          h("div", { class: "clean-desc" }, c.description)
        ),
        h(
          "div",
          { class: "clean-size" },
          h("div", { class: "mono" }, empty ? "Nothing" : c.bytes > 0 ? fmt.bytes(c.bytes) : fmt.count(c.itemCount)),
          !empty && h("div", { class: "clean-count num" }, c.bytes > 0 ? fmt.plural(c.itemCount, "file") : c.itemCount === 1 ? "entry" : "entries")
        ),
        button(open ? "Hide details" : "Show details", {
          icon: open ? "chevron-up" : "chevron-down",
          iconOnly: true,
          variant: "quiet",
          size: "sm",
          onClick: (e) => {
            e.stopPropagation();
            toggleOpen();
          },
        })
      );
      if (!open) return main;
      return [
        main,
        h(
          "div",
          { class: "clean-detail" },
          h("p", { class: "clean-note" }, icon("info"), c.note),
          c.canList && !empty ? fileList(c.id) : !c.canList && h("p", { class: "subtle" }, "This one is a list kept by Windows rather than files on disk, so there is nothing to show individually."),
          c.id === "recycle-bin" && button("Open the Recycle Bin", { size: "sm", icon: "external", onClick: () => api.openKnown("recycle-bin").catch(showError) })
        ),
      ];
    }

    const summaryHost = h("div");
    const listHost = h("div", { class: "stack" });
    const adviceHost = h("div");

    function renderSummary() {
      const picks = chosen();
      const safe = scan.categories.filter((c) => c.risk === "safe");
      const totalLabel = copy.unit === "bytes" ? "found in total" : scan.totalItems === 1 ? "trace found" : "traces found";
      fill(
        summaryHost,
        stats([
          { value: figure(scan.categories), label: totalLabel },
          { value: figure(safe), label: "rated safe to clear" },
          { value: figure(picks), label: `selected in ${fmt.plural(picks.length, "category", "categories")}` },
        ])
      );
    }

    function renderList() {
      // Group headings in order of how much each group holds, biggest first.
      const groups = new Map();
      for (const c of scan.categories) {
        if (!groups.has(c.group)) groups.set(c.group, []);
        groups.get(c.group).push(c);
      }
      const ordered = [...groups.entries()]
        .map(([name, cats]) => ({ name, cats: [...cats].sort((a, b) => weight(b, copy.unit) - weight(a, copy.unit)), total: cats.reduce((n, c) => n + weight(c, copy.unit), 0) }))
        .sort((a, b) => b.total - a.total);

      fill(
        listHost,
        ordered.map((g) =>
          h(
            "section",
            { class: "clean-group" },
            ordered.length > 1 && h("div", { class: "clean-group-head" }, h("h2", { class: "eyebrow" }, g.name), h("span", { class: "subtle num" }, figure(g.cats))),
            h("div", { class: "clean-list" }, g.cats.map(row))
          )
        )
      );
    }

    function renderAdvice() {
      if (area !== "junk") return;
      const box = h("div", { class: "advice-body" });
      const ask = async () => {
        fill(box, h("p", { class: "subtle" }, "Asking Gemini. This can take a few seconds."));
        try {
          const text = await api.aiSummarise();
          fill(box, text.split(/\n{2,}/).map((para) => h("p", { class: "selectable" }, para.trim())));
        } catch (err) {
          fill(box, note("attention", err.message, button("Open Settings", { size: "sm", onClick: () => navigate("settings") })));
        }
      };
      fill(
        adviceHost,
        h(
          "section",
          { class: "panel advice" },
          h("div", { class: "panel-head" }, icon("sparkles"), h("h2", { class: "section-title" }, "A second opinion"), button("Ask Gemini", { size: "sm", icon: "sparkles", onClick: ask })),
          h("div", { class: "panel-pad" }, box)
        )
      );
      fill(box, h("p", { class: "muted" }, "Optional. Gemini can explain this scan in plain English and suggest an order to clear things in. Only the category names and sizes above are sent. No file names, paths or account details leave this PC."));
    }

    function render() {
      renderActions();
      if (running) return;
      if (!scan) {
        fill(body, emptyState({ icon: copy.icon, title: copy.emptyTitle, text: copy.emptyText, action: button(copy.scanLabel, { variant: "primary", size: "lg", icon: "search", onClick: runScan }) }));
        return;
      }
      const nothing = scan.categories.every(isEmpty);
      if (nothing) {
        fill(body, emptyState({ icon: "good", title: "Nothing to clear", text: `Quadrant checked ${fmt.plural(scan.categories.length, "category", "categories")} and found nothing worth removing.`, action: button("Scan again", { icon: "refresh", onClick: runScan }) }));
        return;
      }
      fill(body, summaryHost, listHost, adviceHost);
      renderSummary();
      renderList();
      renderAdvice();
    }

    // ── Actions ─────────────────────────────────────────────────────────────

    async function runScan() {
      if (running) return;
      running = true;
      setBusy(copy.id, true);
      const busy = busyPanel(() => api.cancel(area));
      fill(busyHost, busy.el);
      fill(body);
      renderActions();
      const stop = onProgress(area, busy.update);
      try {
        adopt(await api.cleanScan(area, cleanOptions()));
      } catch (err) {
        showError(err);
      } finally {
        stop();
        running = false;
        setBusy(copy.id, false);
        fill(busyHost);
        render();
      }
    }

    async function clean() {
      if (running) return;
      const picks = chosen();
      const busy = busyPanel(() => api.cancel("clean"));
      try {
        const report = await runClean(area, picks, (p) => {
          if (!running) {
            running = true;
            setBusy(copy.id, true);
            fill(busyHost, busy.el);
            renderActions();
          }
          busy.update(p);
        });
        running = false;
        setBusy(copy.id, false);
        fill(busyHost);
        // Measure again so the page shows what is really left.
        if (report) await runScan();
      } catch (err) {
        running = false;
        setBusy(copy.id, false);
        fill(busyHost);
        showError(err);
        render();
      }
    }

    // A Health Check already measured this, so there is no need to scan twice.
    function adoptFromHealth() {
      if (!scan && !running && app.health) {
        adopt(app.health[area]);
        render();
      }
    }
    on("health", () => {
      if (!running && app.health) {
        adopt(app.health[area]);
        render();
      }
    });
    // Clearing the "leave recent temp files" rule changes the numbers.
    on("settings", (key) => {
      if (key === "tempMinAgeHours" && scan && !running) {
        scan = null;
        render();
      }
    });

    render();
    adoptFromHealth();

    return { onShow: adoptFromHealth };
  }

  return { id: copy.id, label: copy.label, icon: copy.icon, mount };
}

export const junk = makePage("junk");
export const privacy = makePage("privacy");
